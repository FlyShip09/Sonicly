//! The dialog that downloads from a link through the listener's own yt-dlp: the link, the music
//! folder the file goes to, and what yt-dlp reports while it runs.

use std::path::PathBuf;
use std::time::Duration;

use gpui::accesskit::{Role, Toggled};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Global, Pixels, Render, SharedString, Window,
    div, px, relative,
};
use i18n::t;
use music::ytdlp::{self, Progress, Trouble};
use router::{Destination, SettingsPage, navigate};
use state::{LinkDownload, LinkState, Sonicly};
use ui::{
    ActiveTheme as _, Button, Checkbox, Dismiss, FORM_CONTEXT, Field, Form, Input, Modal, Radio,
    Submit, Text,
};

use crate::shared::local;

/// How thick the progress bar is drawn.
const BAR: Pixels = px(4.);
/// The bytes in the units a download's speed is shown in.
const KILOBYTE: f64 = 1024.;
const MEGABYTE: f64 = 1024. * 1024.;

/// The download from a link dialog. It only gathers what to fetch and shows how that goes: the
/// download itself belongs to [`LinkDownload`] and carries on with the dialog closed.
pub(crate) struct LinkDownloadDialog {
    downloads: Entity<LinkDownload>,
    link: Entity<Input>,
    /// The folder chosen among the library's, by its place in their list.
    folder: usize,
    playlist: bool,
    open: bool,
    /// Whether a download was running at the last look, so the dialog closes itself when the
    /// one it started ends well.
    running: bool,
    /// Whether the listener stopped the download that is running.
    stopped: bool,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
}

struct Installed(Entity<LinkDownloadDialog>);

impl Global for Installed {}

impl LinkDownloadDialog {
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if cx.try_global::<Installed>().is_none() {
            let downloads = Sonicly::global(cx).link_download.clone();
            let dialog = cx.new(|cx| {
                let link = cx.new(|cx| Input::new("link-download-hint", cx));
                cx.observe(&link, |this: &mut Self, _, cx| {
                    // What is typed after a failure is a new try, so the old reason goes.
                    this.downloads
                        .update(cx, |downloads, cx| downloads.dismiss(cx));
                    cx.notify();
                })
                .detach();
                cx.observe(&downloads, |this: &mut Self, downloads, cx| {
                    let state = downloads.read(cx).state();
                    let running = matches!(state, LinkState::Running { .. });
                    // A download the listener stopped leaves the dialog up, to try another.
                    if this.running && *state == LinkState::Idle && !this.stopped {
                        this.open = false;
                    }
                    this.stopped &= running;
                    this.running = running;
                    cx.notify();
                })
                .detach();

                Self {
                    downloads,
                    link,
                    folder: 0,
                    playlist: false,
                    open: false,
                    running: false,
                    stopped: false,
                    focus: cx.focus_handle(),
                    restore: None,
                }
            });
            cx.set_global(Installed(dialog));
        }
        cx.global::<Installed>().0.clone()
    }

    /// Opens the dialog, with `link` already in its field when the listener pasted one
    /// somewhere else first.
    pub fn open(link: Option<String>, window: &mut Window, cx: &mut App) {
        let dialog = Self::entity(cx);
        dialog.update(cx, |this, cx| this.show(link, window, cx));
    }

    fn show(&mut self, link: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open {
            self.restore = window.focused(cx);
        }
        self.open = true;
        self.downloads.update(cx, |downloads, cx| {
            downloads.look(cx);
            downloads.dismiss(cx);
        });
        self.link.update(cx, |input, cx| {
            if let Some(link) = link {
                input.set_text(link, cx);
            }
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    /// Starts the download of what the field holds into the chosen folder.
    fn start(&mut self, cx: &mut Context<Self>) {
        let folders = self.downloads.read(cx).folders(cx);
        // The same clamp the list draws its mark by, for a folder removed since it was chosen.
        let chosen = self.folder.min(folders.len().saturating_sub(1));
        let Some(folder) = folders.get(chosen).cloned() else {
            return;
        };
        let link = self.link.read(cx).text().to_owned();
        let playlist = self.playlist;
        self.downloads.update(cx, |downloads, cx| {
            downloads.start(&link, folder, playlist, cx)
        });
    }

    /// The link field, the folders to choose from and the playlist switch.
    fn form(&self, folders: &[PathBuf], invalid: bool, cx: &Context<Self>) -> Form {
        let error = invalid.then(|| t!("link-download-invalid"));
        let playlist = self.playlist;
        Form::new("link-download-form")
            .field(
                Field::new(&self.link, cx)
                    .label(t!("link-download-link"))
                    .error(error),
            )
            .child(self.folders(folders, cx))
            .child(
                Checkbox::new("link-download-playlist", playlist)
                    .label(t!("link-download-playlist"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.playlist = !playlist;
                        cx.notify();
                    })),
            )
    }

    /// Where the file goes: the one music folder there is, or a choice among several.
    fn folders(&self, folders: &[PathBuf], cx: &Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let chosen = self.folder.min(folders.len().saturating_sub(1));
        let count = folders.len();
        let heading = ui::spoken("link-download-folder", t!("link-download-folder"))
            .text_size(theme.text(Text::Small))
            .text_color(theme.muted_foreground);
        let rows = folders.iter().enumerate().map(|(place, folder)| {
            let name = SharedString::from(folder.display().to_string());
            let selected = place == chosen;
            div()
                .id(("link-download-folder", place))
                .role(Role::RadioButton)
                .aria_label(name.clone())
                .aria_toggled(Toggled::from(selected))
                .aria_position_in_set(place + 1)
                .aria_size_of_set(count)
                .flex()
                .items_center()
                .gap_3()
                .min_h(theme.metrics.control)
                .px_2()
                .rounded(theme.radius)
                .cursor_pointer()
                .hover(|this| this.bg(theme.secondary_hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.folder = place;
                    cx.notify();
                }))
                .child(div().flex_1().min_w_0().truncate().child(name))
                .when(count > 1, |this| {
                    this.child(Radio::new("link-download-folder-mark", selected).silent())
                })
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(heading)
            .children(rows)
            .into_any_element()
    }

    /// What yt-dlp says of the file it is fetching: its name, a bar for the share that has
    /// arrived, and the speed and time left. Each shows only when yt-dlp reported it.
    fn progress(name: Option<SharedString>, progress: Progress, cx: &App) -> AnyElement {
        let theme = *cx.theme();
        let name = name.unwrap_or_else(|| t!("link-download-starting"));
        let percent = progress
            .fraction
            .map(|fraction| t!("link-download-percent", percent = (fraction * 100.) as u32));
        let speed = progress.speed.map(|speed| match speed >= MEGABYTE {
            true => t!(
                "link-download-speed-mb",
                speed = format!("{:.1}", speed / MEGABYTE)
            ),
            false => t!("link-download-speed-kb", speed = (speed / KILOBYTE) as u64),
        });
        let left = progress.eta.map(|seconds| {
            let left = ui::clock(Duration::from_secs(seconds));
            t!("link-download-left", time = &left)
        });
        let figures = [percent, speed, left].into_iter().flatten().enumerate();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(ui::spoken("link-download-name", name).truncate())
            .when_some(progress.fraction, |this, fraction| {
                this.child(
                    div()
                        .h(BAR)
                        .w_full()
                        .rounded_full()
                        .bg(theme.secondary)
                        .child(
                            div()
                                .h_full()
                                .w(relative(fraction))
                                .rounded_full()
                                .bg(theme.primary),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_4()
                    .font_features(ui::tabular())
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground)
                    .children(figures.map(|(place, figure)| {
                        ui::spoken(("link-download-figure", place), figure)
                    })),
            )
            .into_any_element()
    }

    /// A paragraph of the dialog, in the danger colour when it reports a failure.
    fn paragraph(
        id: &'static str,
        text: SharedString,
        failed: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = cx.theme();
        div()
            .id(id)
            .role(match failed {
                true => Role::Alert,
                false => Role::Paragraph,
            })
            .aria_label(text.clone())
            .text_size(theme.text(Text::Small))
            .text_color(match failed {
                true => theme.danger,
                false => theme.muted_foreground,
            })
            .child(text)
    }
}

impl Render for LinkDownloadDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            if let Some(focus) = self.restore.take() {
                window.focus(&focus, cx);
            }
            return div().into_any_element();
        }
        let theme = *cx.theme();
        let downloads = self.downloads.read(cx);
        let state = downloads.state().clone();
        let found = downloads.program().is_some();
        let folders = downloads.folders(cx);
        let typed = self.link.read(cx).text().trim().to_owned();
        let invalid = !typed.is_empty() && ytdlp::link(&typed).is_none();
        let running = matches!(state, LinkState::Running { .. });
        let ready = found && !folders.is_empty() && !typed.is_empty() && !invalid;

        let modal = Modal::new("link-download", t!("link-download-title"))
            .w(theme.metrics.cover * 3.2)
            .detail(t!("link-download-detail"));
        let modal = match &state {
            LinkState::Running { name, progress } => {
                modal.child(Self::progress(name.clone(), *progress, cx))
            }
            _ if !found => modal
                .child(Self::paragraph(
                    "link-download-missing",
                    missing(),
                    true,
                    cx,
                ))
                .child(
                    div().flex().child(
                        Button::new("link-download-settings")
                            .label(t!("link-download-open-settings"))
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close(cx);
                                navigate(Destination::Settings(SettingsPage::Storage), cx);
                            })),
                    ),
                ),
            _ if folders.is_empty() => modal
                .child(Self::paragraph(
                    "link-download-no-folder",
                    t!("link-download-no-folder"),
                    false,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .child(local::choose_button("link-download-add-folder").outline()),
                ),
            _ => modal.child(self.form(&folders, invalid, cx)),
        };
        let modal = match &state {
            LinkState::Failed(trouble) if found => {
                let said = match trouble {
                    Trouble::Failed(said) if !said.is_empty() => Some(said.clone()),
                    _ => None,
                };
                modal
                    .child(Self::paragraph(
                        "link-download-trouble",
                        explain(trouble),
                        true,
                        cx,
                    ))
                    .when_some(said, |modal, said| {
                        // yt-dlp's own words, shown as they are and never acted on.
                        modal.child(Self::paragraph(
                            "link-download-said",
                            SharedString::from(said),
                            false,
                            cx,
                        ))
                    })
            }
            _ => modal,
        };

        div()
            .absolute()
            .inset_0()
            .key_context(FORM_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                cx.stop_propagation();
                this.close(cx);
            }))
            .on_action(cx.listener(move |this, _: &Submit, _, cx| {
                cx.stop_propagation();
                if ready && !running {
                    this.start(cx);
                }
            }))
            .child(
                modal
                    .action(
                        Button::new("link-download-close")
                            .ghost()
                            .label(match running {
                                true => t!("link-download-hide"),
                                false => t!("common-cancel"),
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    )
                    .action(match running {
                        true => Button::new("link-download-stop")
                            .destructive()
                            .label(t!("link-download-stop"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stopped = true;
                                this.downloads
                                    .update(cx, |downloads, cx| downloads.cancel(cx));
                            })),
                        false => Button::new("link-download-start")
                            .primary()
                            .label(t!("link-download-start"))
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
                    })
                    .on_dismiss(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .into_any_element()
    }
}

/// How to get yt-dlp, said wherever Sonicly finds none: the dialog and its row in the settings.
pub(crate) fn missing() -> SharedString {
    match cfg!(windows) {
        true => t!("link-download-missing-windows"),
        false => t!("link-download-missing"),
    }
}

/// What happened to a download that failed, and what to do about it.
fn explain(trouble: &Trouble) -> SharedString {
    match trouble {
        Trouble::Missing => missing(),
        Trouble::Link => t!("link-download-invalid"),
        Trouble::Folder => t!("link-download-folder-gone"),
        Trouble::Unsupported => t!("link-download-unsupported"),
        Trouble::Protected => t!("link-download-protected"),
        Trouble::NoAudio => t!("link-download-no-audio"),
        Trouble::Network => t!("link-download-network"),
        Trouble::Unavailable => t!("link-download-unavailable"),
        Trouble::Cancelled | Trouble::Failed(_) => t!("link-download-failed"),
    }
}
