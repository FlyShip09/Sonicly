//! The Style and Presets groups of the Lyrics page: the background behind Now playing's lyrics,
//! the face and spacing of the lines, and the looks saved by name. Every row that differs from
//! the default carries a button that puts that one setting back.

use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, PathPromptOptions, SharedString, Window, div, svg};
use i18n::t;
use state::{
    LYRICS_WEIGHTS, LyricsAlign, LyricsBackground, LyricsEmphasis, LyricsStyle, LyricsStyleChange,
    MAX_DIM, MAX_SPACING, MIN_DIM, MIN_INACTIVE, MIN_SPACING, Outcome, SYSTEM_FONT, Toasts,
};
use ui::{
    ActiveTheme as _, Button, Field, Form, MenuItem, OptionCard, Picker, Scrubber, ScrubberState,
    Switch,
};

use super::rows::{Announced as _, grid, segmented};
use super::{Group, SettingsView, Slot};
use crate::shared::ambient::Ambient;

/// How large the icon of a background still to be chosen draws on its card.
const THUMB_ICON: gpui::Pixels = gpui::px(20.);
/// The popover key of the weight picker.
const WEIGHTS: &str = "lyrics-weights";

impl SettingsView {
    /// The groups the lyrics style adds to the Lyrics page, under the switch that shows lyrics.
    pub(super) fn lyrics_style_groups(&self, cx: &App) -> Vec<Group> {
        let style = self.settings.read(cx).lyrics_style();
        let background = style.background;
        let presets = self.settings.read(cx).lyrics_presets().len();
        vec![
            Group::new("settings-group-lyrics-background", "icons/image.svg")
                .cards(Slot::LyricsBackground)
                .rows(
                    [
                        (background == LyricsBackground::Image).then_some(Slot::LyricsImage),
                        (background == LyricsBackground::Video).then_some(Slot::LyricsVideo),
                        background.pictured().then_some(Slot::LyricsDim),
                        background.pictured().then_some(Slot::LyricsBlur),
                        (background == LyricsBackground::Video).then_some(Slot::LyricsVideoPause),
                    ]
                    .into_iter()
                    .flatten(),
                ),
            Group::new("settings-group-text", "icons/text-select.svg").rows([
                Slot::LyricsTypeface,
                Slot::LyricsWeight,
                Slot::FullscreenLyricsSize,
                Slot::PanelLyricsSize,
                Slot::LyricsSpacing,
                Slot::LyricsAlign,
                Slot::LyricsEmphasis,
                Slot::LyricsInactive,
                Slot::BlurLyrics,
                Slot::Karaoke,
                Slot::Romanized,
            ]),
            Group::new(
                "settings-group-lyrics-presets",
                "icons/sliders-horizontal.svg",
            )
            .rows(
                std::iter::once(Slot::LyricsPresetSave)
                    .chain((0..presets).map(Slot::LyricsPreset))
                    .chain(std::iter::once(Slot::LyricsLookReset)),
            ),
        ]
    }

    /// Changes one field of the lyrics style.
    pub(super) fn restyle(&mut self, change: LyricsStyleChange, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_lyrics_style(change, cx));
        cx.notify();
    }

    /// A row whose control is `control`, led by a button that puts the setting back to
    /// `default` while it differs from it.
    fn style_row(
        &self,
        id: &'static str,
        title: SharedString,
        control: impl IntoElement,
        default: Option<LyricsStyleChange>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let reset = default.map(|default| {
            Button::new(SharedString::from(format!("{id}-reset")))
                .ghost()
                .small()
                .icon("icons/undo-2.svg")
                .tooltip("settings-lyrics-reset")
                .on_click(cx.listener(move |this, _, _, cx| this.restyle(default.clone(), cx)))
        });
        self.row(
            title,
            div()
                .flex()
                .items_center()
                .gap_2()
                .children(reset)
                .child(control)
                .into_any_element(),
        )
    }

    /// One card per background, its thumbnail a still of what it paints.
    pub(super) fn lyrics_background_cards(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let style = self.settings.read(cx).lyrics_style();
        let cover = self.previews.playing_or_picked(cx).large_cover(cx);
        let colors = Ambient::colors(&theme);
        let offered = LyricsBackground::offered().collect::<Vec<_>>();
        let count = offered.len();

        grid(count as u16, window, cx)
            .children(offered.into_iter().enumerate().map(|(place, background)| {
                let label = i18n::lookup(background.key(), None);
                let chosen = style.background == background;
                OptionCard::new(SharedString::from(format!(
                    "lyrics-background-{}",
                    background.id()
                )))
                .choice(label.clone(), chosen, place, count)
                .thumb(thumb(background, &style, cover.clone(), colors, cx))
                .label(label)
                .selected(chosen)
                .on_click(cx.listener(move |this, _, _, cx| this.choose_background(background, cx)))
            }))
            .into_any_element()
    }

    /// Puts `background` on, asking for its file first when it needs one it does not have.
    fn choose_background(&mut self, background: LyricsBackground, cx: &mut Context<Self>) {
        let style = self.settings.read(cx).lyrics_style();
        let missing = match background {
            LyricsBackground::Image => style.image.is_none(),
            LyricsBackground::Video => style.video.is_none(),
            _ => false,
        };
        match missing {
            true => self.pick_backdrop(background, cx),
            false => self.restyle(LyricsStyleChange::Background(background), cx),
        }
    }

    /// Asks for a picture or a video and puts it on once chosen. A picture is read first, so one
    /// that will not open is refused with a toast and the background stays as it was. A video is
    /// checked as it plays, which says what went wrong the same way.
    fn pick_backdrop(&mut self, background: LyricsBackground, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        self.backdrop_pick = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let readable = match background {
                LyricsBackground::Image => {
                    let checked = path.clone();
                    cx.background_spawn(async move { ui::Picture::load(&checked, 64).is_ok() })
                        .await
                }
                _ => true,
            };
            this.update(cx, |this, cx| {
                this.backdrop_pick = None;
                if !readable {
                    Toasts::about(
                        Outcome::Failed,
                        "toast-lyrics-picture-failed",
                        file_name(&path),
                        cx,
                    );
                    return;
                }
                let change = match background {
                    LyricsBackground::Video => LyricsStyleChange::Video(Some(path)),
                    _ => LyricsStyleChange::Image(Some(path)),
                };
                this.restyle(change, cx);
                this.restyle(LyricsStyleChange::Background(background), cx);
            })
            .ok();
        }));
    }

    /// The chosen picture or video by its file name, with the button that picks another.
    pub(super) fn lyrics_file_row(
        &self,
        background: LyricsBackground,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let style = self.settings.read(cx).lyrics_style();
        let (id, title, chosen) = match background {
            LyricsBackground::Video => ("lyrics-video", t!("settings-lyrics-video"), style.video),
            _ => ("lyrics-image", t!("settings-lyrics-image"), style.image),
        };
        let name = chosen
            .as_deref()
            .map(file_name)
            .unwrap_or_else(|| t!("settings-lyrics-file-none"));
        let control = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(
                ui::spoken(SharedString::from(format!("{id}-name")), name)
                    .min_w_0()
                    .truncate()
                    .text_color(theme.muted_foreground),
            )
            .child(
                Button::new(SharedString::from(format!("{id}-choose")))
                    .flex_none()
                    .secondary()
                    .small()
                    .label(t!("settings-lyrics-file-choose"))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.pick_backdrop(background, cx)),
                    ),
            );
        self.yielding_row(title, control.into_any_element())
    }

    /// A style setting drawn as a slider from `low` to `high`, its value said in percent.
    fn style_slider(
        &self,
        id: &'static str,
        title: &'static str,
        state: &ScrubberState,
        (value, low, high, default): (f32, f32, f32, f32),
        change: fn(f32) -> LyricsStyleChange,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let fraction = ((value - low) / (high - low)).clamp(0., 1.);
        let percent = t!(
            "settings-lyrics-percent",
            percent = (value * 100.).round() as i64
        );
        let name = i18n::lookup(title, None);
        let slider = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id(SharedString::from(format!("{id}-slider")))
                    .role(Role::Group)
                    .aria_label(name.clone())
                    .w(theme.metrics.cover)
                    .child(
                        Scrubber::new(state, fraction)
                            .name(name.clone())
                            .value(percent.clone())
                            .colors(theme.progress_bar, theme.muted, theme.foreground)
                            .on_move(cx.listener(move |this, fraction: &f32, _, cx| {
                                this.restyle(change(low + (high - low) * fraction), cx)
                            })),
                    ),
            )
            .child(
                ui::spoken(SharedString::from(format!("{id}-value")), percent)
                    .flex_none()
                    .w(theme.metrics.control * 1.5)
                    .whitespace_nowrap()
                    .text_right(),
            );
        let reset = ((value - default).abs() > 0.005).then(|| change(default));
        self.style_row(id, name, slider, reset, cx)
    }

    pub(super) fn lyrics_dim_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.settings.read(cx).lyrics_style().dim;
        let default = LyricsStyle::default().dim;
        self.style_slider(
            "lyrics-dim",
            "settings-lyrics-dim",
            &self.lyrics_dim,
            (value, MIN_DIM, MAX_DIM, default),
            LyricsStyleChange::Dim,
            cx,
        )
    }

    pub(super) fn lyrics_blur_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.settings.read(cx).lyrics_style().blur;
        let default = LyricsStyle::default().blur;
        self.style_slider(
            "lyrics-blur",
            "settings-lyrics-background-blur",
            &self.lyrics_blur,
            (value, 0., 1., default),
            LyricsStyleChange::Blur,
            cx,
        )
    }

    pub(super) fn lyrics_spacing_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.settings.read(cx).lyrics_style().spacing;
        let default = LyricsStyle::default().spacing;
        self.style_slider(
            "lyrics-spacing",
            "settings-lyrics-spacing",
            &self.lyrics_spacing,
            (value, MIN_SPACING, MAX_SPACING, default),
            LyricsStyleChange::Spacing,
            cx,
        )
    }

    pub(super) fn lyrics_inactive_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.settings.read(cx).lyrics_style().inactive;
        let default = LyricsStyle::default().inactive;
        self.style_slider(
            "lyrics-inactive",
            "settings-lyrics-inactive",
            &self.lyrics_inactive,
            (value, MIN_INACTIVE, 1., default),
            LyricsStyleChange::Inactive,
            cx,
        )
    }

    pub(super) fn lyrics_video_pause_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).lyrics_style().video_follows_playback;
        self.row(
            t!("settings-lyrics-video-pause"),
            Switch::new("lyrics-video-pause", on)
                .name(t!("settings-lyrics-video-pause"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.restyle(LyricsStyleChange::VideoFollowsPlayback(!on), cx)
                }))
                .into_any_element(),
        )
    }

    pub(super) fn lyrics_weight_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).lyrics_style().weight;
        let picker = Picker::new(WEIGHTS, &self.popovers, weight_name(chosen))
            .name(t!("settings-lyrics-weight"))
            .width(Picker::NARROW)
            .items(LYRICS_WEIGHTS.map(|weight| {
                MenuItem::new(
                    SharedString::from(format!("lyrics-weight-{weight}")),
                    weight_name(weight),
                )
                .checked(weight == chosen)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.popovers.close();
                    this.restyle(LyricsStyleChange::Weight(weight), cx)
                }))
            }));
        let default = LyricsStyle::default().weight;
        let reset = (chosen != default).then_some(LyricsStyleChange::Weight(default));
        self.style_row(
            "lyrics-weight",
            t!("settings-lyrics-weight"),
            picker,
            reset,
            cx,
        )
    }

    pub(super) fn lyrics_align_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).lyrics_style().align;
        let view = cx.entity();
        let control = segmented(
            "lyrics-align",
            LyricsAlign::ALL.map(|align| (align, i18n::lookup(align.key(), None))),
            chosen,
            move |align, _, cx| {
                let align = *align;
                view.update(cx, |this, cx| {
                    this.restyle(LyricsStyleChange::Align(align), cx)
                });
            },
        );
        let reset = (chosen != LyricsAlign::default())
            .then_some(LyricsStyleChange::Align(LyricsAlign::default()));
        self.style_row(
            "lyrics-align",
            t!("settings-lyrics-align"),
            control,
            reset,
            cx,
        )
    }

    pub(super) fn lyrics_emphasis_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).lyrics_style().emphasis;
        let view = cx.entity();
        let control = segmented(
            "lyrics-emphasis",
            LyricsEmphasis::ALL.map(|emphasis| (emphasis, i18n::lookup(emphasis.key(), None))),
            chosen,
            move |emphasis, _, cx| {
                let emphasis = *emphasis;
                view.update(cx, |this, cx| {
                    this.restyle(LyricsStyleChange::Emphasis(emphasis), cx)
                });
            },
        );
        let reset = (chosen != LyricsEmphasis::default())
            .then_some(LyricsStyleChange::Emphasis(LyricsEmphasis::default()));
        self.style_row(
            "lyrics-emphasis",
            t!("settings-lyrics-emphasis"),
            control,
            reset,
            cx,
        )
    }

    /// The face the lines are set in, picked from the same list as the app's own font, with a
    /// first entry that follows the app.
    pub(super) fn lyrics_typeface_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.settings.read(cx).lyrics_style().font;
        let reset =
            (chosen != SYSTEM_FONT).then(|| LyricsStyleChange::Font(SYSTEM_FONT.to_owned()));
        let picker = self.typeface_picker(super::LYRIC_FACES, cx);
        self.style_row(
            "lyrics-typeface",
            t!("settings-lyrics-typeface"),
            picker,
            reset,
            cx,
        )
    }

    /// The field that names the look about to be saved and the button that saves it.
    pub(super) fn lyrics_preset_save_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let save = Button::new("lyrics-preset-save")
            .flex_none()
            .secondary()
            .small()
            .label(t!("settings-lyrics-preset-save"))
            .on_click(cx.listener(|this, _, _, cx| this.save_preset(cx)));
        self.yielding_row(
            t!("settings-lyrics-preset-new"),
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .justify_end()
                .child(
                    Form::new("lyrics-preset-form")
                        .flex_1()
                        .min_w_0()
                        .max_w(Picker::WIDE)
                        .field(Field::new(&self.preset_name, cx).end(save))
                        .on_submit(cx.listener(|this, _, _, cx| this.save_preset(cx))),
                )
                .into_any_element(),
        )
    }

    /// Saves the look under the typed name, or under a numbered one when the field is empty.
    fn save_preset(&mut self, cx: &mut Context<Self>) {
        let typed = self.preset_name.read(cx).text().trim().to_owned();
        let name = match typed.is_empty() {
            true => {
                let count = self.settings.read(cx).lyrics_presets().len() + 1;
                t!("settings-lyrics-preset-numbered", number = count).to_string()
            }
            false => typed,
        };
        let saved = self
            .settings
            .update(cx, |settings, cx| settings.save_lyrics_preset(&name, cx));
        match saved {
            true => self
                .preset_name
                .update(cx, |input, cx| input.set_text("", cx)),
            false => Toasts::show(Outcome::Failed, "toast-lyrics-preset-full", cx),
        }
        cx.notify();
    }

    /// One saved look: its name, and the buttons that put it on, rename it and delete it. While
    /// it is renamed the name gives way to a field.
    pub(super) fn lyrics_preset_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings.read(cx);
        let Some(preset) = settings.lyrics_presets().get(index) else {
            return div().into_any_element();
        };
        let name = preset.name.clone();
        let worn = settings.worn_lyrics_preset() == Some(name.as_str());
        let count = settings.lyrics_presets().len();
        let id = |part: &str| SharedString::from(format!("lyrics-preset-{part}-{index}"));

        if self.renaming.as_deref() == Some(name.as_str()) {
            let from = name.clone();
            let done = Button::new(id("rename-done"))
                .flex_none()
                .secondary()
                .small()
                .label(t!("settings-lyrics-preset-rename-done"))
                .on_click(cx.listener({
                    let from = from.clone();
                    move |this, _, _, cx| this.rename_preset(&from, cx)
                }));
            return self.yielding_row(
                SharedString::from(name),
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .justify_end()
                    .child(
                        Form::new(id("rename-form"))
                            .flex_1()
                            .min_w_0()
                            .max_w(Picker::WIDE)
                            .field(Field::new(&self.preset_rename, cx).end(done))
                            .on_submit(
                                cx.listener(move |this, _, _, cx| this.rename_preset(&from, cx)),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let actions = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                Button::new(id("apply"))
                    .secondary()
                    .small()
                    .label(match worn {
                        true => t!("settings-lyrics-preset-worn"),
                        false => t!("settings-lyrics-preset-apply"),
                    })
                    .disabled(worn)
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| {
                            this.settings
                                .update(cx, |settings, cx| settings.apply_lyrics_preset(&name, cx));
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new(id("rename"))
                    .ghost()
                    .small()
                    .icon("icons/pencil.svg")
                    .tooltip("settings-lyrics-preset-rename")
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, window, cx| {
                            this.preset_rename
                                .update(cx, |input, cx| input.set_text(name.clone(), cx));
                            this.preset_rename
                                .update(cx, |input, cx| input.focus(window, cx));
                            this.renaming = Some(name.clone());
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new(id("delete"))
                    .ghost()
                    .small()
                    .icon("icons/trash-2.svg")
                    .tooltip("settings-lyrics-preset-delete")
                    .on_click(cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| {
                            this.settings.update(cx, |settings, cx| {
                                settings.delete_lyrics_preset(&name, cx)
                            });
                            cx.notify();
                        }
                    })),
            );

        div()
            .id(id("row"))
            .role(Role::ListItem)
            .aria_label(SharedString::from(name.clone()))
            .aria_position_in_set(index + 1)
            .aria_size_of_set(count)
            .child(self.row(SharedString::from(name), actions.into_any_element()))
            .into_any_element()
    }

    /// Renames the look `from` to what the field holds. A name another look has, or none at all,
    /// is refused with a toast and the field stays up.
    fn rename_preset(&mut self, from: &str, cx: &mut Context<Self>) {
        let to = self.preset_rename.read(cx).text().to_owned();
        let renamed = self.settings.update(cx, |settings, cx| {
            settings.rename_lyrics_preset(from, &to, cx)
        });
        match renamed {
            true => self.renaming = None,
            false => Toasts::show(Outcome::Failed, "toast-lyrics-preset-name-taken", cx),
        }
        cx.notify();
    }

    /// The button that puts the whole look back to the default, saved looks left alone.
    pub(super) fn lyrics_look_reset_row(&self, cx: &mut Context<Self>) -> AnyElement {
        self.row(
            t!("settings-lyrics-look-reset"),
            Button::new("lyrics-look-reset")
                .secondary()
                .small()
                .label(t!("settings-lyrics-look-reset-action"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.reset_lyrics_look(cx));
                    cx.notify();
                }))
                .into_any_element(),
        )
    }
}

/// The thumbnail of one background card: the two fields drawn still in the cover's colours, the
/// cover, the chosen picture, or the icon of what is still to be chosen.
fn thumb(
    background: LyricsBackground,
    style: &LyricsStyle,
    cover: Option<String>,
    colors: [gpui::Hsla; 5],
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let icon = |path: &'static str| {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.secondary)
            .child(
                svg()
                    .path(icons::path(path))
                    .size(THUMB_ICON)
                    .text_color(theme.muted_foreground),
            )
            .into_any_element()
    };
    let field = |element: AnyElement| {
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .child(element)
            .into_any_element()
    };
    match background {
        LyricsBackground::Waves => field(
            ui::Waves::new(colors)
                .stage(gpui::black())
                .time(7.)
                .into_any_element(),
        ),
        LyricsBackground::Fluid => {
            let [base, highlight, second, shadow, _] = colors;
            field(
                ui::Fluid::new([shadow, base, second, highlight])
                    .stage(gpui::black())
                    .time(7.)
                    .into_any_element(),
            )
        }
        LyricsBackground::Cover => match cover {
            Some(cover) => field(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        ui::Artwork::new(Some(cover))
                            .size(theme.metrics.cover * 1.5)
                            .corner_radius(gpui::px(0.)),
                    )
                    .into_any_element(),
            ),
            None => icon("icons/disc-3.svg"),
        },
        LyricsBackground::Image => match &style.image {
            Some(path) => field(
                gpui::img(path.clone())
                    .size_full()
                    .object_fit(gpui::ObjectFit::Cover)
                    .into_any_element(),
            ),
            None => icon("icons/image.svg"),
        },
        LyricsBackground::Video => icon("icons/play.svg"),
    }
}

/// What a weight is called in the picker.
fn weight_name(weight: u16) -> SharedString {
    let key = match weight {
        ..=400 => "settings-lyrics-weight-regular",
        401..=500 => "settings-lyrics-weight-medium",
        501..=600 => "settings-lyrics-weight-semibold",
        601..=700 => "settings-lyrics-weight-bold",
        _ => "settings-lyrics-weight-heavy",
    };
    i18n::lookup(key, None)
}

/// A file's name, for a toast or a row that names it.
fn file_name(path: &std::path::Path) -> SharedString {
    SharedString::from(
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
    )
}
