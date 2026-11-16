//! The ambient background: a flowing colour field behind fullscreen, five colours folded into
//! one another by slow waves in the `ui::Waves` shader, darkened so lyrics stay readable. `Root`
//! owns the one entity, paints it under everything, title bar included, and paints no
//! background of its own beneath it. The field stands in for the page and carries the window's
//! own opacity, so a see-through window stays see-through in fullscreen.
//!
//! `ambient` decides whether it is painted at all, and with it whether the fullscreen controls
//! frost what they float over. `ambient_motion` decides whether the field drifts.
//!
//! The colours are the theme's `tint` and `tint_secondary`, which fullscreen samples off the
//! cover whatever the adaptive theme setting says, resolved through `Theme::accent` rather than
//! read off `Theme::selection`, which lags a track change by the length of the theme's own fade.
//! Art with no colour leaves no tint and the stage stays neutral. `painted` eases the colours on
//! screen toward those, so a cover change washes in rather than cuts.
//!
//! While the lyrics panel is up, the lyrics style may ask for another background: the softer
//! `ui::Fluid` field, the cover itself, or a picture or video of the listener's own. It fades in
//! over the waves and out again with the panel. A picture lies under a scrim that never hides
//! less of it than keeps white lines at 4.5 to 1 over its brighter parts. Both fields swell a
//! little with the music's loudness, read off the real spectrum and eased over most of a second,
//! so the motion follows a passage and never a beat.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Context, Hsla, ObjectFit, Pixels, Render, RenderImage, Rgba, Subscription, Task, Window,
    div, img, px,
};
use music::Listening;
use state::{LyricsBackground, LyricsStyle, PlaybackState, SideTab, Sonicly, Toasts};
use ui::{ActiveTheme as _, CoverPalette, Picture, Theme, VideoFrames, VideoProblem};

use crate::shared::adaptive;

/// How many colours make up the field.
const COLORS: usize = 5;
/// How much of itself the field keeps under the dark overlay that holds lyrics readable over
/// it. The overlay is folded into the colours rather than painted as a sheet of its own, which
/// over an opaque field is the same picture and leaves a see-through window's alpha alone.
const SHADE: f32 = 0.55;
/// Time constant of the exponential ease onto a new palette, in seconds. The
/// field lands within a few percent of the target after about three of these.
const WASH: f32 = 0.8;
/// How much of the cover's colours the fluid field keeps, more than the waves' `1 - SHADE`.
const FLUID_KEPT: f32 = 0.62;
/// How many times smaller across each side the field behind the window is drawn before it is
/// stretched back over it. Nothing in it is sharper than a heavy blur, so the stretch does not show,
/// and the shader runs on a sixteenth of the pixels.
const FIELD_SHRINK: f32 = 4.;
/// How many ticks of the shared 120 Hz clock pass between drift frames once the colours have
/// settled, which makes 10 Hz. The brightest channel of the field moves about ten 8 bit steps a
/// second, so a frame moves it by one step and the drift shows no stepping. Each drift frame
/// redraws all of fullscreen, which is what an integrated GPU pays for.
const DRIFT_TICKS: u32 = 12;
/// How many ticks pass between frames while the colours still wash onto a new cover, which makes
/// 30 Hz. The wash moves far faster than the drift and steps visibly at the drift's pace. A video
/// background asks for its frames at the same pace, the most it is decoded at.
const WASH_TICKS: u32 = 4;
/// How close every channel of the painted colours has to sit to its target before the wash
/// counts as done.
const SETTLED: f32 = 1. / 255.;
/// How long the lyrics background takes to fade in over the waves or out of them, in seconds.
/// A whole-window surface fades rather than moves, and a little slower than a panel.
const BACKDROP_FADE: f32 = 0.45;
/// The time constant the loudness is eased by, in seconds. Long enough that ten updates a
/// second never step and a single beat barely registers.
const ENERGY_EASE: f32 = 0.7;
/// How far the loudness pushes the waves past their resting swell.
const ENERGY_SWELL: f32 = 0.35;
/// How many of the lowest spectrum bands the loudness is read from: the bass and low mids, where
/// a passage's weight sits.
const ENERGY_BANDS: usize = 12;
/// How long the field may go undrawn before it lets go of the spectrum and any video it holds.
const UNDRAWN: Duration = Duration::from_secs(1);
/// The strongest blur a picture background takes, in logical pixels at full size.
const MOST_BLUR: f32 = 64.;
/// Below this share of `MOST_BLUR` a picture is drawn sharp at full size.
const SHARP: f32 = 0.04;
/// The longest side a picture background is decoded at.
const PICTURE_EDGE: u32 = 1920;
/// How bright a cover is taken to be past the brightness of its two leading colours, which
/// say how colourful it is but not how bright its highlights run.
const COVER_HEADROOM: f32 = 1.6;

/// The flowing colour field. One sits behind the whole window in fullscreen, and a preview can
/// hold another of its own made with `Ambient::framed`.
pub(crate) struct Ambient {
    started: Instant,
    stepped: Instant,
    painted: Option<[Hsla; COLORS]>,
    /// Whether the painted colours are still easing toward the theme's, which keeps the faster
    /// frame pace.
    washing: bool,
    /// Whether a wait for the next drift frame is already running.
    pacing: Option<Task<()>>,
    /// The palette of the cover a framed field follows, `None` for the one behind the window.
    framed: Option<Sampled>,
    /// Whether this field shows the lyrics style's background: always for the Lyrics preview,
    /// and behind the window while the lyrics panel is up.
    styled: Styling,
    /// How far the lyrics background has faded in, from 0 to 1, and when it last stepped.
    presence: (f32, Instant),
    /// The music's eased loudness and what keeps the analyzer listening for it.
    energy: Energy,
    picture: Option<Loaded>,
    video: Option<Playing>,
    /// The last file whose failure was told, so a broken file is reported once.
    reported: Option<PathBuf>,
    /// When the field last drew, which tells a field that left the screen.
    drawn: Instant,
    /// Lets go of the spectrum and the video once the field stops being drawn.
    watch: Option<Task<()>>,
    /// Wakes the field when its window is shown again, since a hidden one asks for no frames
    /// and nothing else would start it drifting.
    shown_again: Option<Subscription>,
}

/// When the lyrics style applies to a field.
#[derive(Clone, Copy, PartialEq)]
enum Styling {
    Never,
    Always,
    WithLyrics,
}

/// The palette of the cover a framed field follows, sampled by the field itself.
#[derive(Default)]
struct Sampled {
    cover: Option<String>,
    palette: CoverPalette,
    _task: Option<Task<()>>,
}

#[derive(Default)]
struct Energy {
    level: f32,
    stepped: Option<Instant>,
    listening: Option<Listening>,
}

/// A picture background, loading or loaded.
struct Loaded {
    path: PathBuf,
    picture: Option<Picture>,
    _task: Task<()>,
}

/// A video background and the frame on screen.
struct Playing {
    path: PathBuf,
    frames: VideoFrames,
    shown: Option<(u64, Arc<RenderImage>)>,
}

impl Ambient {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let settings = Sonicly::global(cx).settings.clone();
        // Turning the drift back on has no frame of its own to land in: with
        // motion off nothing asks for one.
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();

        let now = Instant::now();
        Self {
            started: now,
            stepped: now,
            painted: None,
            washing: false,
            pacing: None,
            framed: None,
            styled: Styling::WithLyrics,
            presence: (0., now),
            energy: Energy::default(),
            picture: None,
            video: None,
            reported: None,
            drawn: now,
            watch: None,
            shown_again: None,
        }
    }

    /// A field for a preview that fills a frame rather than the window. It takes its colours
    /// from the cover `follow` hands it whatever the adaptive theme says, and leaves its corners
    /// square for the frame to round.
    pub(crate) fn framed(cx: &mut Context<Self>) -> Self {
        Self {
            framed: Some(Sampled::default()),
            styled: Styling::Never,
            ..Self::new(cx)
        }
    }

    /// A framed field that wears the lyrics style's background, for the Lyrics preview.
    pub(crate) fn lyrics_framed(cx: &mut Context<Self>) -> Self {
        Self {
            styled: Styling::Always,
            presence: (1., Instant::now()),
            ..Self::framed(cx)
        }
    }

    /// Samples `cover` for a framed field once it differs from the one sampled last. A field
    /// behind the window takes its colours from the theme and ignores this.
    pub(crate) fn follow(&mut self, cover: Option<String>, cx: &mut Context<Self>) {
        let Some(sampled) = self.framed.as_mut() else {
            return;
        };
        if sampled.cover == cover {
            return;
        }
        sampled.cover = cover.clone();
        let Some(cover) = cover else {
            sampled._task = None;
            sampled.palette = CoverPalette::default();
            cx.notify();
            return;
        };
        let palette = ui::palette(cover, cx);
        sampled._task = Some(cx.spawn(async move |this, cx| {
            let palette = palette.await;
            this.update(cx, |this, cx| {
                if let Some(sampled) = this.framed.as_mut() {
                    sampled.palette = palette;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The theme this field is painted from: the app's own behind the window, and in a frame
    /// the one fullscreen would wear for the sampled cover. A preview draws over it with the same.
    pub(crate) fn theme(&self, cx: &App) -> Theme {
        match &self.framed {
            None => *cx.theme(),
            Some(sampled) => adaptive::stage(sampled.palette, cx),
        }
    }

    /// Schedules the next update without keeping display callbacks pending between updates.
    /// It comes at `ticks` of the shared clock, the wash's pace while the colours still ease and
    /// a video plays, and the drift's once they settle, so `wash` has to run first in a frame.
    fn pace(&mut self, ticks: u32, window: &mut Window, cx: &mut Context<Self>) {
        if self.pacing.is_some() {
            return;
        }
        let timer = super::animation::next(ticks, cx);
        let handle = window.window_handle();
        self.pacing = Some(cx.spawn(async move |this, cx| {
            timer.await;
            if this
                .update(cx, |this, cx| {
                    this.pacing = None;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            handle
                .update(cx, |_, window, _| window.on_next_frame(|_, _| {}))
                .ok();
        }));
    }

    /// Eases the painted colours one frame toward `target` and returns them.
    /// The step is the real time since the last frame, so a gap long enough to
    /// mean the field was off screen lands on the target outright and entering
    /// fullscreen never washes in from whatever played before. With motion off
    /// there are no frames to ease over, so the target is taken as it is.
    fn wash(&mut self, target: [Hsla; COLORS], animates: bool) -> [Hsla; COLORS] {
        let now = Instant::now();
        let step = now.duration_since(self.stepped).as_secs_f32();
        self.stepped = now;

        let painted = match self.painted {
            Some(painted) if animates => {
                let delta = 1. - (-step / WASH).exp();
                std::array::from_fn(|index| blend(painted[index], target[index], delta))
            }
            _ => target,
        };
        self.washing = !settled(painted, target);
        self.painted = Some(painted);
        painted
    }

    /// Steps the lyrics background toward shown or hidden and returns how far in it is. A fade
    /// still under way keeps the faster pace. Reduced motion keeps the fade, as a fade is not
    /// motion.
    fn present(&mut self, wanted: bool) -> f32 {
        let (at, stepped) = self.presence;
        let target = f32::from(u8::from(wanted));
        let step = stepped.elapsed().as_secs_f32() / BACKDROP_FADE;
        let at = match target > at {
            true => (at + step).min(target),
            false => (at - step).max(target),
        };
        self.presence = (at, Instant::now());
        if at != target {
            self.washing = true;
        }
        at
    }

    /// Eases the music's loudness one step and returns it, listening to the spectrum only while
    /// a field that follows it is drawn and the music plays.
    fn loudness(&mut self, wanted: bool, cx: &App) -> f32 {
        let playback = Sonicly::global(cx).playback.read(cx);
        let playing = matches!(playback.state(), PlaybackState::Playing);
        let spectrum = (wanted && playing).then(|| playback.spectrum(cx)).flatten();
        let target = match &spectrum {
            Some(spectrum) => {
                if self.energy.listening.is_none() {
                    self.energy.listening = Some(spectrum.listen());
                }
                let bands = spectrum.bands();
                let low = &bands[..bands.len().min(ENERGY_BANDS)];
                match low.is_empty() {
                    true => 0.,
                    false => (low.iter().sum::<f32>() / low.len() as f32).clamp(0., 1.),
                }
            }
            None => {
                self.energy.listening = None;
                0.
            }
        };
        let now = Instant::now();
        let step = self
            .energy
            .stepped
            .map_or(1., |at| now.duration_since(at).as_secs_f32());
        self.energy.stepped = Some(now);
        self.energy.level += (target - self.energy.level) * (1. - (-step / ENERGY_EASE).exp());
        self.energy.level
    }

    /// Starts the watch that lets go of the spectrum and the video once the field is no longer
    /// drawn, which is the only sign a field behind a closed fullscreen gets.
    fn keep_watch(&mut self, cx: &mut Context<Self>) {
        if self.watch.is_some() || (self.energy.listening.is_none() && self.video.is_none()) {
            return;
        }
        self.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(UNDRAWN).await;
                let done = this
                    .update(cx, |this, cx| {
                        if this.drawn.elapsed() < UNDRAWN {
                            return false;
                        }
                        this.energy.listening = None;
                        if let Some(shown) = this.video.take().and_then(|video| video.shown) {
                            cx.drop_image(shown.1, None);
                        }
                        this.watch = None;
                        true
                    })
                    .unwrap_or(true);
                if done {
                    return;
                }
            }
        }));
    }

    /// The picture background for `path`, loading it on first sight.
    fn picture(&mut self, path: &PathBuf, cx: &mut Context<Self>) -> Option<Picture> {
        if self
            .picture
            .as_ref()
            .is_none_or(|loaded| &loaded.path != path)
        {
            let wanted = path.clone();
            let task = cx.spawn(async move |this, cx| {
                let load = wanted.clone();
                let picture = cx
                    .background_spawn(async move { Picture::load(&load, PICTURE_EDGE) })
                    .await;
                this.update(cx, |this, cx| {
                    let Some(loaded) = this.picture.as_mut().filter(|loaded| loaded.path == wanted)
                    else {
                        return;
                    };
                    match picture {
                        Ok(picture) => loaded.picture = Some(picture),
                        Err(error) => {
                            log::warn!("ambient: cannot read {}: {error}", wanted.display());
                            this.report(&wanted, "toast-lyrics-picture-failed", cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
            });
            if let Some(old) = self.picture.take().and_then(|loaded| loaded.picture) {
                cx.drop_image(old.image, None);
            }
            self.picture = Some(Loaded {
                path: path.clone(),
                picture: None,
                _task: task,
            });
        }
        self.picture
            .as_ref()
            .and_then(|loaded| loaded.picture.clone())
    }

    /// The video background for `path`, opened on first sight. `wanted` keeps it decoding; a
    /// video nobody wants holds its last frame. Returns the frame to draw and how bright it is.
    fn video(
        &mut self,
        path: &PathBuf,
        wanted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Arc<RenderImage>, f32)> {
        if self.video.as_ref().is_none_or(|video| &video.path != path) {
            if let Some(shown) = self.video.take().and_then(|video| video.shown) {
                window.drop_image(shown.1).ok();
            }
            self.video = Some(Playing {
                path: path.clone(),
                frames: VideoFrames::open(path.clone()),
                shown: None,
            });
        }
        let video = self.video.as_mut()?;
        if let Some(problem) = video.frames.problem() {
            let key = match problem {
                VideoProblem::Unsupported => "toast-lyrics-video-unsupported",
                VideoProblem::Unreadable => "toast-lyrics-video-unreadable",
                VideoProblem::NoVideo => "toast-lyrics-video-empty",
                VideoProblem::Undecodable => "toast-lyrics-video-codec",
            };
            let path = path.clone();
            self.report(&path, key, cx);
            return None;
        }
        // Without a frame yet the decoder is asked for one, so a paused song or reduced motion
        // still shows a still of the video.
        if wanted || video.shown.is_none() {
            video.frames.want();
        }
        if let Some(latest) = video.frames.latest()
            && video
                .shown
                .as_ref()
                .is_none_or(|(number, _)| *number != latest.0)
            && let Some(old) = video.shown.replace(latest)
        {
            window.drop_image(old.1).ok();
        }
        let luma = video.frames.luma();
        video.shown.as_ref().map(|(_, image)| (image.clone(), luma))
    }

    /// Tells the listener once that the background file at `path` cannot be shown.
    fn report(&mut self, path: &PathBuf, key: &'static str, cx: &mut Context<Self>) {
        if self.reported.as_ref() == Some(path) {
            return;
        }
        self.reported = Some(path.clone());
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Toasts::about(state::Outcome::Failed, key, name, cx);
    }

    /// The cover the field shows as a background: the sampled one in a frame, and behind the
    /// window the largest the app holds of the track on show.
    fn cover(&self, cx: &App) -> Option<String> {
        if let Some(sampled) = &self.framed {
            return sampled.cover.clone();
        }
        let track = Sonicly::global(cx).playback.read(cx).shown_track(cx)?;
        let large = track
            .album_id
            .as_deref()
            .and_then(|album| Sonicly::global(cx).cover.read(cx).large_for(album))
            .map(str::to_owned);
        large.or(track.cover)
    }

    /// Resolves the five field colours from the theme: the cover's leading hue
    /// carried from a light highlight down to a dark shadow, plus its runner-up
    /// where the art names one. Art with no colour leaves no tints behind, and
    /// then the stage is quiet neutrals.
    pub(crate) fn colors(theme: &Theme) -> [Hsla; COLORS] {
        let neutral = |light: f32| Hsla {
            h: 0.06,
            s: 0.12,
            l: light,
            a: 1.,
        };
        // A dark theme gets a dark stage; lift the floor a little so the
        // pools stay apart from the stage.
        let floor = (0.13 + theme.background.l * 0.8).clamp(0.13, 0.30);

        let Some(tint) = theme.tint else {
            return [
                neutral(floor),
                neutral(floor + 0.025),
                neutral(floor + 0.05),
                neutral(floor + 0.015),
                neutral(floor + 0.04),
            ];
        };
        let accent = theme.accent(tint);
        let base = Hsla {
            h: accent.h,
            s: (accent.s * 0.9 + 0.04).clamp(0.45, 0.75),
            l: (0.40 + (accent.l - 0.45) * 0.5).clamp(0.28, 0.52),
            a: 1.,
        };
        let second = match theme.tint_secondary {
            Some(second) => Hsla {
                h: second.h,
                s: (second.s * 0.9).clamp(0.4, 0.72),
                l: (0.38 + (second.l - 0.45) * 0.5).clamp(0.28, 0.5),
                a: 1.,
            },
            None => neutral(floor + 0.03),
        };
        [
            base,
            Hsla {
                l: (base.l + 0.14).clamp(0.3, 0.62),
                s: (base.s - 0.08).clamp(0.4, 0.75),
                ..base
            },
            second,
            Hsla {
                l: (base.l - 0.13).clamp(0.18, 0.45),
                s: (base.s + 0.04).clamp(0.45, 0.8),
                ..base
            },
            Hsla {
                h: (base.h + 0.97).rem_euclid(1.),
                l: (base.l - 0.16).clamp(0.16, 0.4),
                s: (base.s - 0.02).clamp(0.4, 0.78),
                ..base
            },
        ]
    }

    /// The lyrics background over the waves: the fluid field, or a picture under its scrim.
    /// `wanted` keeps a video decoding. A picture still loading paints nothing, so the waves
    /// show through.
    fn backdrop(
        &mut self,
        style: &LyricsStyle,
        field: Field,
        wanted: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let stage = field.stage;
        let pictured = match style.background {
            LyricsBackground::Waves => return None,
            LyricsBackground::Fluid => {
                // The streams keep more of the cover than the waves do: they are softer, and the
                // shader's own ceiling still holds them under the lines.
                let [base, highlight, second, shadow, _] =
                    field.colors.map(|color| kept(color, FLUID_KEPT));
                let fluid = ui::Fluid::new([shadow, base, second, highlight])
                    .stage(stage)
                    .time(field.elapsed)
                    .energy(field.energy);
                return Some(self.shrunk(fluid.into_any_element(), window));
            }
            LyricsBackground::Cover => {
                let cover = self.cover(cx)?;
                // A cover's palette says how bright its colours are, not its highlights, so it is
                // read with headroom.
                let theme = self.theme(cx);
                let luma = [theme.tint, theme.tint_secondary]
                    .into_iter()
                    .flatten()
                    .map(|tint| luminance(theme.accent(tint)))
                    .fold(0.3_f32, f32::max)
                    * COVER_HEADROOM;
                (Backdrop::Cover(cover), luma.min(1.))
            }
            LyricsBackground::Image => {
                let path = style.image.clone()?;
                let picture = self.picture(&path, cx)?;
                (Backdrop::Image(picture.image), picture.luma)
            }
            LyricsBackground::Video => {
                let path = style.video.clone()?;
                let (frame, luma) = self.video(&path, wanted, window, cx)?;
                (Backdrop::Image(frame), luma)
            }
        };
        let (backdrop, luma) = pictured;
        let dim = style.dim.max(ui::legible_dim(luma));
        let blur = style.blur.clamp(0., 1.);
        let viewport = match &self.framed {
            None => window.viewport_size(),
            // a frame is small enough to draw at full size
            Some(_) => gpui::size(px(0.), px(0.)),
        };
        let picture = |side: Option<(Pixels, Pixels)>| {
            let fill = div().absolute().inset_0();
            match &backdrop {
                Backdrop::Image(image) => fill
                    .child(img(image.clone()).size_full().object_fit(ObjectFit::Cover))
                    .into_any_element(),
                Backdrop::Cover(cover) => {
                    // a square cover covers the frame by its longer side, centred
                    let edge = side.map_or(px(0.), |(wide, high)| wide.max(high));
                    fill.flex()
                        .items_center()
                        .justify_center()
                        .overflow_hidden()
                        .child(
                            ui::Artwork::new(Some(cover.clone()))
                                .size(edge)
                                .corner_radius(px(0.)),
                        )
                        .into_any_element()
                }
            }
        };
        let painted = match (blur > SHARP, &self.framed) {
            // A blurred picture is drawn small and stretched back, like the field, which
            // costs a sixteenth of the pixels and blurs a quarter as far for the same look.
            (true, None) => {
                let scale = window.scale_factor();
                let (wide, high) = (
                    field_side(viewport.width, scale),
                    field_side(viewport.height, scale),
                );
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(wide)
                    .h(high)
                    .layer_scale(FIELD_SHRINK)
                    .layer_scale_origin(gpui::point(0., 0.))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .when(ui::effects(), |this| {
                                this.blur(px(MOST_BLUR * blur / FIELD_SHRINK))
                            })
                            .child(picture(Some((wide, high)))),
                    )
                    .into_any_element()
            }
            (true, Some(_)) => div()
                .absolute()
                .inset_0()
                .when(ui::effects(), |this| {
                    this.blur(px(MOST_BLUR * blur / FIELD_SHRINK))
                })
                .child(picture(Some(cover_edge(window))))
                .into_any_element(),
            (false, _) => picture(Some(match &self.framed {
                None => (viewport.width, viewport.height),
                Some(_) => cover_edge(window),
            })),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .bg(stage)
                .child(painted)
                .child(div().absolute().inset_0().bg(gpui::black()).opacity(dim))
                .into_any_element(),
        )
    }

    /// `element` drawn small behind the window and stretched back over it, at full size in a
    /// frame.
    fn shrunk(&self, element: gpui::AnyElement, window: &Window) -> gpui::AnyElement {
        match self.framed {
            Some(_) => element,
            None => {
                let viewport = window.viewport_size();
                let scale = window.scale_factor();
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(field_side(viewport.width, scale))
                    .h(field_side(viewport.height, scale))
                    .layer_scale(FIELD_SHRINK)
                    .layer_scale_origin(gpui::point(0., 0.))
                    .child(element)
                    .into_any_element()
            }
        }
    }
}

/// What a field is painted from this frame.
#[derive(Clone, Copy)]
struct Field {
    colors: [Hsla; COLORS],
    stage: Hsla,
    elapsed: f32,
    energy: f32,
}

/// What a picture background paints.
enum Backdrop {
    Image(Arc<RenderImage>),
    Cover(String),
}

impl Render for Ambient {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.drawn = Instant::now();
        if self.shown_again.is_none() {
            let field = cx.entity().downgrade();
            self.shown_again = Some(window.observe_window_visibility(move |_, _, cx| {
                field.update(cx, |_, cx| cx.notify()).ok();
            }));
        }
        let theme = self.theme(cx);
        let settings = Sonicly::global(cx).settings.read(cx);
        let style = settings.lyrics_style();
        let styled = match self.styled {
            Styling::Never => false,
            Styling::Always => true,
            Styling::WithLyrics => settings.fullscreen_tab() == Some(SideTab::Lyrics),
        } && style.background != LyricsBackground::Waves;
        let waves_on = self.framed.is_some() || settings.ambient();
        // Without motion the field is a still gradient: no frame requests, so
        // fullscreen stops redrawing once settled. Either the system preference
        // or the setting of its own is enough to stop it. A minimized window draws nothing,
        // so it asks for nothing either.
        let animates = ui::motion::animates(cx) && settings.ambient_motion() && window.is_visible();
        let playing = matches!(
            Sonicly::global(cx).playback.read(cx).state(),
            PlaybackState::Playing
        );
        let presence = self.present(styled);
        let colors = self.wash(Self::colors(&theme), animates);
        // The music reaches only the fields, and only while they drift.
        let follows = animates
            && ((styled && style.background == LyricsBackground::Fluid)
                || (waves_on && presence < 1.));
        let energy = self.loudness(follows, cx);
        let video = presence > 0. && style.background == LyricsBackground::Video;
        let video_wanted = video
            && ui::motion::animates(cx)
            && window.is_visible()
            && (playing || !style.video_follows_playback);
        let elapsed = match animates {
            true => self.started.elapsed().as_secs_f32(),
            false => 0.,
        };
        if animates || video_wanted || (presence > 0. && presence < 1.) {
            let ticks = match self.washing || video_wanted {
                true => WASH_TICKS,
                false => DRIFT_TICKS,
            };
            self.pace(ticks, window, cx);
        }
        // The field stands in for the page rather than sitting on it, so it carries the
        // window's own opacity and `Root` paints no background beneath it.
        let clarity = match theme.transparent {
            true => theme.background.a,
            false => 1.,
        };
        // The field reaches into the corners the chrome rounds, so it takes that radius itself.
        // A framed field leaves its corners to the frame.
        let radius = match self.framed {
            None => window_radius(window, cx),
            Some(_) => None,
        };
        let stage = shaded(Hsla {
            a: 1.,
            ..theme.background
        });
        let backdrop = match presence > 0. {
            true => self.backdrop(
                &style,
                Field {
                    colors,
                    stage,
                    elapsed,
                    energy,
                },
                video_wanted,
                window,
                cx,
            ),
            false => None,
        };
        if !video && let Some(shown) = self.video.take().and_then(|video| video.shown) {
            window.drop_image(shown.1).ok();
        }
        self.keep_watch(cx);
        // Once the background covers the waves whole, they are not drawn under it.
        let covered = backdrop.is_some() && presence >= 1.;

        let waves = (waves_on && !covered).then(|| {
            ui::Waves::new(layered(colors).map(shaded))
                .stage(stage)
                .time(elapsed)
                .swell(1. + ENERGY_SWELL * energy)
        });

        div()
            .id("ambient")
            .absolute()
            .inset_0()
            .opacity(clarity)
            .when(!waves_on, |this| this.bg(stage))
            .when_some(waves, |this, waves| match self.framed {
                // A preview is small enough to draw at full size.
                Some(_) => this.child(waves),
                None => {
                    let viewport = window.viewport_size();
                    let scale = window.scale_factor();
                    this.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .w(field_side(viewport.width, scale))
                            .h(field_side(viewport.height, scale))
                            .layer_scale(FIELD_SHRINK)
                            .layer_scale_origin(gpui::point(0., 0.))
                            .child(waves.when_some(
                                radius.map(|radius| radius / FIELD_SHRINK),
                                ui::Waves::corner_radius,
                            )),
                    )
                }
            })
            .when_some(backdrop, |this, backdrop| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .when(presence < 1., |this| this.opacity(ease(presence)))
                        .child(backdrop),
                )
            })
    }
}

/// Whether a field is painted behind fullscreen: the ambient waves are on, or the lyrics panel
/// is up with a background of its own. Fullscreen reads it for the frosted glass on its
/// controls as well, which only has the field to blur.
pub(crate) fn shown(cx: &App) -> bool {
    let settings = Sonicly::global(cx).settings.read(cx);
    settings.ambient() || settings.lyrics_backdrop()
}

/// The radius of the window's own corners, where the chrome rounds them.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn window_radius(window: &Window, cx: &App) -> Option<Pixels> {
    crate::chrome::window_radius(Sonicly::global(cx).settings.read(cx), cx, window)
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
fn window_radius(_window: &Window, _cx: &App) -> Option<Pixels> {
    None
}

/// The colours in the order the field lays them down, the runner-up last. Each pool covers the
/// ones under it, so drawn third it sank beneath the two shadows and a cover's second colour
/// barely showed.
fn layered(colors: [Hsla; COLORS]) -> [Hsla; COLORS] {
    let [base, highlight, second, shadow, deep] = colors;
    [base, highlight, shadow, deep, second]
}

/// A colour as the dark overlay leaves it. Black at `SHADE` over an opaque colour is that
/// colour's channels cut by what the sheet lets through, so cutting them here is the same
/// picture with one layer fewer to spend the window's alpha on.
fn shaded(color: Hsla) -> Hsla {
    kept(color, 1. - SHADE)
}

/// A colour with its channels cut to `kept` of themselves, as black laid over it would leave it.
fn kept(color: Hsla, kept: f32) -> Hsla {
    let rgba = Rgba::from(color);

    Hsla::from(Rgba {
        r: rgba.r * kept,
        g: rgba.g * kept,
        b: rgba.b * kept,
        a: rgba.a,
    })
}

/// Whether every channel of `painted` sits within `SETTLED` of `target`.
fn settled(painted: [Hsla; COLORS], target: [Hsla; COLORS]) -> bool {
    painted.iter().zip(target.iter()).all(|(painted, target)| {
        let (painted, target) = (Rgba::from(*painted), Rgba::from(*target));
        [
            painted.r - target.r,
            painted.g - target.g,
            painted.b - target.b,
            painted.a - target.a,
        ]
        .iter()
        .all(|gap| gap.abs() <= SETTLED)
    })
}

/// Straight-line blend between two colours through RGB, so a wash between two
/// unrelated hues passes through grey rather than through every hue between
/// them.
fn blend(from: Hsla, to: Hsla, delta: f32) -> Hsla {
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let channel = |from: f32, to: f32| from + (to - from) * delta;

    Hsla::from(Rgba {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a: channel(from.a, to.a),
    })
}

/// The relative luminance of a colour, from 0 to 1.
fn luminance(color: Hsla) -> f32 {
    let Rgba { r, g, b, .. } = color.into();
    let linear = |channel: f32| match channel <= 0.04045 {
        true => channel / 12.92,
        false => ((channel + 0.055) / 1.055).powf(2.4),
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// The ease the lyrics background fades along: quick to arrive, gentle to land.
fn ease(at: f32) -> f32 {
    ui::ease_out_cubic(at.clamp(0., 1.))
}

/// The size a cover is drawn at to cover a framed field: the frame's own, which the preview
/// sets from the window's width.
fn cover_edge(window: &Window) -> (Pixels, Pixels) {
    let side = window.viewport_size().width;
    (side, side)
}

/// The logical length of one side of the shrunken field, a whole number of device pixels that
/// reaches the window's own `side` once stretched by `FIELD_SHRINK`. A plain logical quarter can
/// snap below that and leave a line of whatever lies behind the window along its bottom or right.
fn field_side(side: Pixels, scale: f32) -> Pixels {
    let device = (f32::from(side) * scale).round();
    px((device / FIELD_SHRINK).ceil() / scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tinted(hue: f32) -> Theme {
        let mut theme = Theme::dark();
        theme.tint = Some(Hsla {
            h: hue,
            s: 0.6,
            l: 0.4,
            a: 1.,
        });
        theme.selection = Hsla {
            h: hue,
            s: 0.7,
            l: 0.44,
            a: 1.,
        };
        theme.tint_secondary = Some(Hsla {
            h: (hue + 0.5).rem_euclid(1.),
            s: 0.6,
            l: 0.4,
            a: 1.,
        });
        theme
    }

    #[test]
    fn accent_theme_yields_its_family() {
        let colors = Ambient::colors(&tinted(0.78));

        for (index, color) in colors.iter().enumerate() {
            // The runner-up sits in slot two; everything else is family.
            if index == 2 {
                let gap = (color.h - 0.28).abs().min(1. - (color.h - 0.28).abs());
                assert!(gap < 0.06, "second hue was {}", color.h);
                continue;
            }
            let gap = (color.h - 0.78).abs().min(1. - (color.h - 0.78).abs());
            assert!(gap < 0.06, "hue was {}", color.h);
        }
        assert!(colors[1].l > colors[0].l, "highlight lifts");
        assert!(colors[3].l < colors[0].l, "shadow sinks");
    }

    #[test]
    fn untinted_theme_yields_neutrals() {
        let colors = Ambient::colors(&Theme::dark());

        assert!(colors.iter().all(|color| color.s < 0.2));
    }
}
