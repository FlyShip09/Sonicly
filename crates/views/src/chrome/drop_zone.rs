use std::path::{Path, PathBuf};

use gpui::accesskit::{Role, Toggled};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, ExternalPaths, FontWeight, InteractiveElement, Render,
    SharedString, Window, div, relative, svg,
};
use i18n::t;
use state::{Ask, DropTarget, FileDrop, Placement, Shelf, Sonicly};
use ui::{ActiveTheme as _, Button, LEADING, Modal, Radio, Text, corners};

use crate::chrome::{Chrome, PlayerBar};

/// How much of the foreground the dashed edge of the drop area keeps.
const EDGE: f32 = 0.28;
/// The most music folders the question offers to copy into. The settings page names any other.
const COPY_CHOICES: usize = 4;

/// The window's answer to files dragged over it from the system. While they hover, the page
/// between the sidebars gives way to a calm sheet that says what a drop does: how many songs
/// it adds and where, or that nothing in the drag is something Sonicly plays. The sidebars stay
/// clear, since a local playlist there and the queue take a drop of their own. A drop that
/// brings songs from outside the music folders asks once where they go.
pub(crate) struct DropZone {
    file_drop: Entity<FileDrop>,
    /// The answer the question has picked, an index into its choices.
    choice: usize,
    /// Whether the sheet covers the whole window under the title bar, as it does over the
    /// welcome screen, which has no sidebars or player bar to keep clear.
    whole: bool,
}

impl DropZone {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let file_drop = FileDrop::entity(cx);
        cx.observe(&file_drop, |this, file_drop, cx| {
            if file_drop.read(cx).asking().is_none() {
                this.choice = 0;
            }
            cx.notify();
        })
        .detach();
        Self {
            file_drop,
            choice: 0,
            whole: false,
        }
    }

    /// Lets the sheet cover the whole window while the welcome screen is up.
    pub fn whole_window(&mut self, whole: bool) {
        self.whole = whole;
    }

    /// The sheet over the page while files hover, or nothing.
    fn sheet(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let drop = self.file_drop.read(cx);
        let survey = drop.survey()?;
        if !cx.has_active_drag() && !drop.previewing() {
            return None;
        }
        let theme = cx.theme();
        let chrome = Chrome::get(cx);
        let ink = theme.foreground;
        let (icon, headline, detail): (&str, SharedString, SharedString) = match survey.count() {
            None => (
                "icons/folder-plus.svg",
                t!("drop-counting"),
                t!("drop-detail-kept"),
            ),
            Some(0) => (
                "icons/circle-alert.svg",
                t!("drop-nothing"),
                t!("drop-nothing-detail"),
            ),
            Some(count) => (
                "icons/folder-plus.svg",
                t!("drop-add-library", count = count),
                where_they_go(survey.songs.as_deref().unwrap_or_default(), cx),
            ),
        };
        let playable = survey.count().is_some_and(|count| count > 0);
        let elsewhere = playable && local_playlists(cx);
        let edge = match playable || survey.count().is_none() {
            true => ink.opacity(EDGE),
            false => theme.danger.opacity(EDGE * 2.),
        };

        Some(
            div()
                .id("drop-zone")
                .role(Role::Status)
                .aria_label(headline.clone())
                .absolute()
                .top(ui::snapped(theme.metrics.title_bar, window))
                .map(|this| match self.whole {
                    true => this.bottom_0().left_0().right_0(),
                    false => this
                        .bottom(PlayerBar::height(window, cx))
                        .left(chrome.left())
                        .right(chrome.right()),
                })
                .p(theme.metrics.inset)
                // Opaque: a page half seen through it only competes with the one message.
                .bg(theme.background)
                .text_color(ink)
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .p(theme.metrics.inset)
                        .rounded(corners(theme.radius))
                        .border_2()
                        .border_dashed()
                        .border_color(edge)
                        .line_height(relative(LEADING))
                        .text_center()
                        .child(
                            svg()
                                .path(icons::path(icon))
                                .size(theme.metrics.control)
                                .mb_2()
                                .text_color(match playable || survey.count().is_none() {
                                    true => theme.muted_foreground,
                                    false => theme.danger,
                                }),
                        )
                        .child(
                            div()
                                .text_size(theme.text(Text::Title))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(headline),
                        )
                        .child(
                            div()
                                .max_w(theme.metrics.cover * 4.)
                                .text_size(theme.text(Text::Body))
                                .text_color(theme.muted_foreground)
                                .child(detail),
                        )
                        .when(elsewhere, |this| {
                            this.child(
                                div()
                                    .mt_4()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(t!("drop-hint-playlist")),
                            )
                        }),
                )
                .into_any_element(),
        )
    }

    /// The question a drop of songs from outside the music folders waits on: add their folder
    /// as it is, or copy them into one of the music folders.
    fn question(&self, ask: &Ask, cx: &mut Context<Self>) -> Modal {
        let choices = choices(ask);
        let total = choices.len();
        let picked = self.choice.min(total.saturating_sub(1));
        let rows = choices
            .iter()
            .enumerate()
            .map(|(index, (label, detail, _))| {
                let theme = cx.theme();
                let selected = index == picked;
                div()
                    .id(("drop-choice", index))
                    .role(Role::RadioButton)
                    .aria_label(label.clone())
                    .aria_toggled(Toggled::from(selected))
                    .aria_position_in_set(index + 1)
                    .aria_size_of_set(total)
                    .flex()
                    .items_center()
                    .gap_3()
                    .w_full()
                    .min_h(theme.metrics.row)
                    .px_2()
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(|this| this.bg(theme.secondary_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.choice = index;
                        cx.notify();
                    }))
                    .child(Radio::new(("drop-choice-mark", index), selected).silent())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .line_height(relative(LEADING))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(theme.text(Text::Body))
                                    .child(label.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(theme.text(Text::Small))
                                    .text_color(theme.muted_foreground)
                                    .child(detail.clone()),
                            ),
                    )
            });
        let answer = choices
            .get(picked)
            .map(|(_, _, placement)| placement.clone());

        Modal::new("drop-question", t!("drop-ask-title"))
            .detail(t!("drop-ask-detail", count = ask.outside))
            .child(
                div()
                    .id("drop-choices")
                    .role(Role::RadioGroup)
                    .aria_label(t!("drop-ask-title"))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows),
            )
            .action(
                Button::new("drop-cancel")
                    .ghost()
                    .label(t!("common-cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.file_drop.update(cx, |drop, cx| drop.dismiss(cx));
                    })),
            )
            .action(
                Button::new("drop-confirm")
                    .primary()
                    .label(t!("drop-ask-confirm"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(answer) = answer.clone() {
                            this.file_drop
                                .update(cx, |drop, cx| drop.answer(answer, cx));
                        }
                    })),
            )
            .on_dismiss(cx.listener(|this, _, _, cx| {
                this.file_drop.update(cx, |drop, cx| drop.dismiss(cx));
            }))
    }
}

impl Render for DropZone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A drag that left the window or was called off leaves no event behind here, so the
        // sheet lets go of its count once the drag is gone.
        let lingering = {
            let drop = self.file_drop.read(cx);
            drop.survey().is_some() && !drop.previewing() && !cx.has_active_drag()
        };
        if lingering {
            let file_drop = self.file_drop.clone();
            cx.defer(move |cx| file_drop.update(cx, |drop, cx| drop.leave(cx)));
        }
        let ask = self.file_drop.read(cx).asking().cloned();

        // The window's size, so the sheet can stand between the sidebars. It takes no pointer
        // events of its own, so whatever it covers still takes the drop.
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(self.sheet(window, cx))
            .children(ask.map(|ask| self.question(&ask, cx)))
    }
}

/// Makes an element take files dropped from the system for `target`, lighting it while files
/// hover over it.
pub(crate) fn takes_files<E: InteractiveElement>(element: E, target: DropTarget, cx: &App) -> E {
    let lit = cx.theme().secondary_hover;
    element
        .drag_over::<ExternalPaths>(move |style, _, _, _| style.bg(lit))
        .on_drop(move |paths: &ExternalPaths, _, cx| {
            let (paths, target) = (paths.paths().to_vec(), target.clone());
            let file_drop = FileDrop::entity(cx);
            file_drop.update(cx, |drop, cx| drop.drop_on(paths, target, cx));
        })
}

/// The answers the question offers, each with its label, its detail and what it keeps.
fn choices(ask: &Ask) -> Vec<(SharedString, SharedString, Placement)> {
    // One folder is named in the label and spelled out under it; several are counted in the
    // label and named under it.
    let (own, detail) = match ask.folders.as_slice() {
        [folder] => (
            t!("drop-ask-folder", name = name(folder)),
            folder.display().to_string(),
        ),
        folders => (
            t!("drop-ask-folders", count = folders.len()),
            folders
                .iter()
                .map(|folder| name(folder))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    };
    let mut choices = vec![(own, SharedString::from(detail), Placement::Folder)];
    choices.extend(ask.music_folders.iter().take(COPY_CHOICES).map(|folder| {
        (
            t!("drop-ask-copy", name = name(folder)),
            SharedString::from(folder.display().to_string()),
            Placement::Copy(folder.clone()),
        )
    }));
    choices
}

/// What the drop does with the files, said under the count: nothing to them when the music
/// folders hold them already, and otherwise what was answered before, or that it will ask.
fn where_they_go(songs: &[PathBuf], cx: &App) -> SharedString {
    let settings = Sonicly::global(cx).settings.read(cx);
    let folders = settings.local_folders();
    let outside = songs
        .iter()
        .any(|song| !folders.iter().any(|folder| song.starts_with(folder)));
    match (outside, settings.dropped_files()) {
        (false, _) => t!("drop-detail-kept"),
        (true, Some(Placement::Copy(folder))) if folders.contains(folder) => {
            t!("drop-detail-copy", name = name(folder))
        }
        (true, Some(Placement::Folder)) => t!("drop-detail-folder"),
        (true, _) if folders.is_empty() => t!("drop-detail-folder"),
        (true, _) => t!("drop-detail-ask"),
    }
}

/// Whether there is a local playlist the listener can add to, which the sheet points to.
fn local_playlists(cx: &App) -> bool {
    let library = Sonicly::global(cx).library.read(cx);
    library
        .state(Shelf::Local)
        .playlists()
        .iter()
        .any(|playlist| library.is_editable(&playlist.id))
}

fn name(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.display().to_string())
}
