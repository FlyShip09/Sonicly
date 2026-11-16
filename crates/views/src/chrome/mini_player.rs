use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, SharedString, Size, Window, WindowControlArea, div, px, relative,
    size,
};
use i18n::t;
use input::{LeaveMiniPlayer, WORKSPACE_CONTEXT};
use state::{AppSettings, Playback, Sonicly};
use ui::{ActiveTheme as _, Artwork, Button, LEADING, Text, blurring, clock, snapped};

use crate::shared::transport::{fraction, next, previous, toggle};

/// How wide the card is, in covers. Its cover is as wide as the card.
const CARD_WIDTH: f32 = 2.;
/// How wide the bar is, in covers.
const BAR_WIDTH: f32 = 2.7;
/// How much of the popover colour the frost keeps over the blurred cover, and how hard it blurs.
/// Without the blur the fill is all there is, so it thickens.
const FROST_FILL: f32 = 0.72;
const FLAT_FILL: f32 = 0.92;
const FROST_BLUR: Pixels = px(28.);
/// How thick the progress line draws.
const LINE: Pixels = px(2.);
/// How much of the foreground the progress line's track keeps.
const TRACK: f32 = 0.16;
/// How much of the foreground the artist line keeps. A flat grey reads poorly over a tinted frost.
const SECONDARY: f32 = 0.72;

/// The compact player in its own always-on-top window: the cover, title and artist, previous,
/// play or pause and next, and a thin progress line. It folds into a bar or unfolds into a card,
/// and a button takes the listener back to the main window. The progress line follows the
/// position by itself and redraws only when it moves by a device pixel, so neither a playing
/// track redraws the main window nor a paused one redraws anything.
pub struct MiniPlayer {
    playback: Entity<Playback>,
    settings: Entity<AppSettings>,
    focus: FocusHandle,
    bar: bool,
    /// Whether a press on the backdrop is waiting for the pointer to move, which starts a
    /// window drag where the platform leaves that to the app.
    grabbed: bool,
    /// The window's scale factor at the last render, for telling whether a position report
    /// moves the line by a device pixel.
    scale: f32,
    /// How far along the line was drawn, in device pixels.
    drawn: Option<i64>,
}

impl MiniPlayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let playback = Sonicly::global(cx).playback.clone();
        let settings = Sonicly::global(cx).settings.clone();
        let devices = Sonicly::global(cx).devices.clone();
        let ticks = playback.read(cx).ticks().clone();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        cx.observe(&devices, |_, _, cx| cx.notify()).detach();
        cx.observe(&ticks, |this, _, cx| this.follow(cx)).detach();
        let focus = cx.focus_handle();
        // capture mode: scripts/shot.ps1 photographs the window without stealing focus
        if std::env::var_os("SONICLY_SHOT").is_none() {
            focus.focus(window, cx);
        }

        Self {
            bar: settings.read(cx).mini_player_bar(),
            playback,
            settings,
            focus,
            grabbed: false,
            scale: window.scale_factor(),
            drawn: None,
        }
    }

    /// The size of the window for the bar or the card. Both follow the theme's metrics, so a
    /// larger text size grows them with everything else.
    pub fn size(bar: bool, cx: &App) -> Size<Pixels> {
        let theme = cx.theme();
        let metrics = theme.metrics;
        match bar {
            true => size(metrics.cover * BAR_WIDTH, metrics.row + metrics.pad * 2.),
            false => {
                let width = metrics.cover * CARD_WIDTH;
                let panel = metrics.pad * 4.
                    + (theme.text(Text::Body) + theme.text(Text::Small)) * LEADING
                    + metrics.control_small;
                size(width, width + panel)
            }
        }
    }

    /// Redraws when a position report moves the line by a device pixel.
    fn follow(&mut self, cx: &mut Context<Self>) {
        if self.drawn != Some(self.fill(cx)) {
            cx.notify();
        }
    }

    /// How far along the line is, in device pixels of the window's width.
    fn fill(&self, cx: &App) -> i64 {
        let (elapsed, total) = self.playback.read(cx).shown_time(cx);
        let width = f32::from(Self::size(self.bar, cx).width);
        (fraction(elapsed, total) * width * self.scale).round() as i64
    }

    /// Folds the card into a bar or unfolds the bar into a card, keeping the window's top left
    /// corner where it is.
    fn fold(&mut self, bar: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.bar = bar;
        self.drawn = None;
        self.settings
            .update(cx, |settings, cx| settings.set_mini_player_bar(bar, cx));
        window.resize(Self::size(bar, cx));
        cx.notify();
    }

    /// The thin line along the seam, filled as far as the track has played.
    fn line(&self, width: Pixels, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (elapsed, total) = self.playback.read(cx).shown_time(cx);
        div()
            .id("mini-player-progress")
            .role(Role::ProgressIndicator)
            .aria_label(t!("song-position"))
            .aria_value(clock(elapsed))
            .relative()
            .w(width)
            .h(LINE)
            .flex_none()
            .bg(theme.popover_foreground.opacity(TRACK))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(width * fraction(elapsed, total))
                    .bg(theme.progress_bar),
            )
    }

    /// The title over the artist, or a quiet line while nothing plays.
    fn words(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let track = self.playback.read(cx).shown_track(cx);
        let secondary = theme.popover_foreground.opacity(SECONDARY);
        let (title, artists): (SharedString, Option<SharedString>) = match &track {
            Some(track) => (
                track.name.clone().into(),
                Some(track.artists.clone().into()),
            ),
            None => (t!("player-nothing-playing"), None),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .line_height(relative(LEADING))
            .child(
                ui::spoken("mini-player-title", title)
                    .min_w_0()
                    .truncate()
                    .text_size(theme.text(Text::Body))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(track.is_none(), |this| this.text_color(secondary)),
            )
            .when_some(artists, |this, artists| {
                this.child(
                    ui::spoken("mini-player-artist", artists)
                        .min_w_0()
                        .truncate()
                        .text_size(theme.text(Text::Small))
                        .text_color(secondary),
                )
            })
    }

    /// Previous, play or pause and next, the same buttons the player bar has.
    fn transport(&self, cx: &App) -> impl IntoElement {
        controls()
            .child(previous(&self.playback, false, cx))
            .child(toggle(&self.playback, false, false, cx))
            .child(next(&self.playback, false, cx))
    }

    /// The fold button and the way back to the main window.
    fn corner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().popover_foreground.opacity(SECONDARY);
        let bar = self.bar;
        let (icon, hint) = match bar {
            true => ("icons/chevron-down.svg", "mini-player-card"),
            false => ("icons/chevron-up.svg", "mini-player-bar"),
        };

        controls()
            .child(
                Button::new("mini-player-fold")
                    .ghost()
                    .small()
                    .icon(icon)
                    .tooltip(hint)
                    .tint(muted)
                    .on_click(cx.listener(move |this, _, window, cx| this.fold(!bar, window, cx))),
            )
            .child(
                Button::new("mini-player-return")
                    .ghost()
                    .small()
                    .icon("icons/maximize.svg")
                    .tooltip("mini-player-return")
                    .tint(muted)
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(LeaveMiniPlayer), cx)
                    }),
            )
    }

    /// The single frosted surface: the popover colour over a hard blur of the cover beneath,
    /// or a thicker flat fill where blurs are off.
    fn frost(cx: &App) -> gpui::Div {
        let theme = cx.theme();
        match blurring(cx) {
            true => div()
                .bg(theme.popover.opacity(FROST_FILL))
                .backdrop_blur(FROST_BLUR),
            false => div().bg(theme.popover.opacity(FLAT_FILL)),
        }
    }
}

impl Render for MiniPlayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.scale = window.scale_factor();
        self.drawn = Some(self.fill(cx));
        let theme = *cx.theme();
        window.set_rem_size(theme.font_size);
        let metrics = theme.metrics;
        let shape = Self::size(self.bar, cx);
        let (width, height) = (shape.width, shape.height);
        let track = self.playback.read(cx).shown_track(cx);
        let seed = track
            .as_ref()
            .map(crate::shared::cards::seed)
            .unwrap_or_default();
        let cover = track.and_then(|track| track.cover);
        // The backdrop is the cover again, stretched over the whole window under the frost.
        // It reuses the decode of the cover drawn above it, since the blur hides the detail.
        let side = width.max(height);
        let backdrop = Artwork::new(cover.clone())
            .seed(seed.clone())
            .size(side)
            .corner_radius(Pixels::ZERO)
            .sampled_at(match self.bar {
                true => metrics.row,
                false => width,
            })
            .overlay()
            .absolute()
            .left((width - side) / 2.)
            .top((height - side) / 2.);

        // One frost, and the line rides its edge: along the foot of the bar, and along the seam
        // under the card's cover, where the cover hands over to the controls.
        let body = match self.bar {
            true => Self::frost(cx)
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap_2()
                        .px(metrics.pad)
                        .child(
                            Artwork::new(cover)
                                .seed(seed)
                                .size(snapped(metrics.row, window))
                                .flex_none(),
                        )
                        .child(self.words(cx))
                        .child(self.transport(cx))
                        .child(self.corner(cx)),
                )
                .child(self.line(width, cx))
                .into_any_element(),
            false => div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .child(
                    Artwork::new(cover)
                        .seed(seed)
                        .size(width)
                        .corner_radius(Pixels::ZERO)
                        .flex_none(),
                )
                .child(
                    Self::frost(cx)
                        .flex()
                        .flex_col()
                        .flex_1()
                        .child(self.line(width, cx))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .justify_between()
                                .px(metrics.pad * 1.5)
                                .pt(metrics.pad * 1.5)
                                .pb(metrics.pad)
                                .child(self.words(cx))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        // a button's inset lines its icon up with the text
                                        .ml(-metrics.pad)
                                        .mr(-metrics.pad * 0.5)
                                        .child(self.transport(cx))
                                        .child(self.corner(cx)),
                                ),
                        ),
                )
                .into_any_element(),
        };

        div()
            .id("mini-player")
            .role(Role::Group)
            .aria_label(t!("player-mini"))
            .key_context(WORKSPACE_CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .font(crate::root::ui_font(self.settings.read(cx).font()))
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            // Every part of the window that is not a button moves it. Windows hit-tests the
            // area itself; macOS and Linux are asked to move the window once the press drags.
            .window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.grabbed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.grabbed = false),
            )
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if std::mem::take(&mut this.grabbed) {
                    window.start_window_move();
                }
            }))
            .child(backdrop)
            .child(body)
    }
}

/// A row of buttons inside the window's drag area. It covers what is under it, so Windows does
/// not take a press on a button for the start of a window drag, and it keeps the press from the
/// backdrop's own drag.
fn controls() -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_1()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}
