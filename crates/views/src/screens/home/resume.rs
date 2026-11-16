use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{App, Entity, FontWeight, Pixels, SharedString, Window, div, px, relative};
use i18n::t;
use router::{Destination, Link as _};
use state::{Playback, Sonicly};
use ui::ActiveTheme as _;
use ui::{Artwork, Button, Skeleton, Text, clock, spoken_heading, tabular};

use crate::shared::cards;

/// How thick the line under the title that shows how far the song got is drawn.
const BAR: Pixels = px(4.);

/// Home's first block: the song in the player, playing or held where the last session left
/// it, with how far it got and the one button that plays or pauses it. Nothing is drawn when
/// the player holds no song.
#[derive(IntoElement)]
pub(crate) struct Resume {
    playback: Entity<Playback>,
}

impl Resume {
    pub(crate) fn new(playback: Entity<Playback>) -> Self {
        Self { playback }
    }

    /// Whether the block has anything to draw: a song in the player, or one the last session
    /// left that the account being restored is about to hand back, drawn as a skeleton so the
    /// page below does not move when it lands.
    pub(crate) fn shown(playback: &Entity<Playback>, cx: &App) -> bool {
        playback.read(cx).shown_track(cx).is_some() || restoring(cx)
    }
}

impl RenderOnce for Resume {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.theme();
        let art = theme.metrics.cover / 2.;
        let playback = self.playback.read(cx);
        let track = playback.shown_track(cx);
        let playing = playback.control() == Some(true);
        let (position, length) = playback.shown_time(cx);
        let heading = match (&track, playing) {
            (Some(_), true) => t!("home-now-playing"),
            _ => t!("home-continue"),
        };
        let sheet = div()
            .id("home-resume")
            .flex()
            .items_center()
            .gap_4()
            .w_full()
            .max_w(theme.metrics.cover * 4.)
            .p_3()
            .rounded(ui::rim(theme.radius))
            .bg(theme.secondary)
            .border_1()
            .border_color(theme.border);

        let body = match track {
            None => sheet
                .child(Skeleton::new().flex_none().size(art).rounded(theme.radius))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .gap_2()
                        .child(Skeleton::new().w(art * 2.).h(theme.text(Text::Body)))
                        .child(Skeleton::new().w(art * 1.5).h(theme.text(Text::Small)))
                        .child(Skeleton::new().w_full().h(BAR)),
                )
                .into_any_element(),
            Some(track) => {
                let fraction = match length.is_zero() {
                    true => 0.,
                    false => (position.as_secs_f32() / length.as_secs_f32()).clamp(0., 1.),
                };
                // The position only moves on the player's events, so it is shown where it holds
                // still: a song that plays shows its length alone, and the player bar the clock.
                let held = !playing && !position.is_zero();
                let time = match held {
                    true => t!(
                        "home-resume-time",
                        position = clock(position).to_string(),
                        length = clock(length).to_string()
                    ),
                    false => clock(length),
                };
                let name = SharedString::from(track.name.clone());
                let toggled = self.playback.clone();
                let (icon, tip) = match playing {
                    true => ("icons/pause.svg", "play-pause"),
                    false => ("icons/play.svg", "play-resume"),
                };

                sheet
                    .role(Role::Group)
                    .aria_label(heading.clone())
                    .child(
                        Artwork::new(track.cover.clone())
                            .seed(cards::seed(&track))
                            .edged()
                            .size(art)
                            .flex_none(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .id("home-resume-title")
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .map(|this| match track.album_id.clone() {
                                        Some(album) => this
                                            .aria_label(name.clone())
                                            .hover(|style| style.underline())
                                            .link(Destination::Album(album.into())),
                                        None => this.role(Role::Label).aria_value(name.clone()),
                                    })
                                    .child(name),
                            )
                            .child(cards::track_artists("home-resume-artist", &track, &theme))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .pt_1()
                                    .child(div().flex_1().h(BAR).rounded_full().when(held, |bar| {
                                        bar.bg(theme.muted).child(
                                            div()
                                                .h_full()
                                                .w(relative(fraction))
                                                .rounded_full()
                                                .bg(theme.progress_bar),
                                        )
                                    }))
                                    .child(
                                        div()
                                            .id("home-resume-time")
                                            .role(Role::Label)
                                            .aria_value(time.clone())
                                            .flex_none()
                                            .text_size(theme.text(Text::Small))
                                            .text_color(theme.muted_foreground)
                                            .font_features(tabular())
                                            .child(time),
                                    ),
                            ),
                    )
                    .child(
                        Button::new("home-resume-play")
                            .icon(icon)
                            .primary()
                            .tooltip(tip)
                            .on_click(move |_, _, cx| {
                                toggled.update(cx, |playback, cx| playback.toggle_play(cx));
                            }),
                    )
                    .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(spoken_heading("home-resume-heading", heading, 2, cx))
            .child(body)
    }
}

/// Whether an account is still being restored with a song from the last session waiting to be
/// handed back to the player.
fn restoring(cx: &App) -> bool {
    let sonicly = Sonicly::global(cx);
    sonicly.session.read(cx).is_pending() && sonicly.settings.read(cx).resume().is_some()
}
