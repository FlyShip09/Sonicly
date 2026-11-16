use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, div, img};
use i18n::t;
use state::{Channel, Stars};
use ui::{ActiveTheme as _, Switch, Text};

use super::rows::{outward, segmented};
use super::{Group, SettingsView, Slot};

/// How large the app icon stands, in covers.
const MARK_TILE: f32 = 0.5;
/// The app icon as this platform shows it, registered by the binary's asset source.
const APP_ICON: &str = "brand/sonicly.svg";
const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
/// The third-party notices as a package ships them.
const NOTICES: &str = "THIRD-PARTY.md";

impl SettingsView {
    pub(super) fn about_groups(&self) -> Vec<Group> {
        // no release repository means nothing to check, so neither update row stands
        let updates = state::RELEASE_REPO.is_some().then_some(Slot::Updates);
        let channel = state::updates_itself().then_some(Slot::Channel);
        vec![
            Group::untitled().rows(
                [Slot::Version]
                    .into_iter()
                    .chain(updates)
                    .chain(channel)
                    .chain([
                        Slot::Log,
                        Slot::License,
                        Slot::Licenses,
                        Slot::Source,
                        Slot::Star,
                        Slot::StarCount,
                    ]),
            ),
        ]
    }

    /// The version row, which for a nightly also names the commit it was built from.
    pub(super) fn version_row(&self) -> AnyElement {
        let version: SharedString = match state::commit() {
            Some(commit) => t!(
                "settings-version-commit",
                version = state::version(),
                commit = commit
            ),
            None => state::version().into(),
        };
        self.row(
            t!("settings-version"),
            ui::spoken("settings-version-value", version).into_any_element(),
        )
    }

    pub(super) fn updates_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).check_updates();

        self.row(
            t!("settings-check-updates"),
            Switch::new("check-updates", on)
                .name(t!("settings-check-updates"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_check_updates(!on, cx));
                }))
                .into_any_element(),
        )
    }

    pub(super) fn channel_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.settings.read(cx).update_channel();

        let choices = segmented(
            "update-channel",
            Channel::ALL.map(|channel| (channel, channel.label())),
            current,
            cx.listener(|this, channel: &Channel, _, cx| {
                let channel = *channel;
                this.settings
                    .update(cx, |settings, cx| settings.set_update_channel(channel, cx));
            }),
        );

        self.row(
            t!("settings-update-channel"),
            choices
                .name(t!("settings-update-channel"))
                .into_any_element(),
        )
    }

    /// The row that opens a copy of the current log file in whatever the system reads text
    /// with. It is disabled when the platform names no state or cache folder to log into.
    pub(super) fn log_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = state::log_file();

        outward("open-log", t!("settings-log-open"), None, cx)
            .disabled(path.is_none())
            .on_click(move |_, _, _| {
                let Some(path) = &path else { return };
                if let Err(error) = snapshot(path).and_then(open::that_detached) {
                    log::warn!(
                        "settings: cannot open a copy of {}: {error}",
                        path.display()
                    );
                }
            })
            .into_any_element()
    }

    pub(super) fn license_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward(
            "license",
            t!("settings-license"),
            Some(t!("settings-license-short")),
            cx,
        )
        .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
        .into_any_element()
    }

    /// The row that opens the third-party notices. It is disabled when the build ships none,
    /// as an unpackaged one does.
    pub(super) fn licenses_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = notices();

        outward("licenses", t!("settings-licenses"), None, cx)
            .disabled(path.is_none())
            .on_click(move |_, _, _| {
                let Some(path) = &path else { return };
                if let Err(error) = open::that_detached(path) {
                    log::warn!("settings: cannot open {}: {error}", path.display());
                }
            })
            .into_any_element()
    }

    pub(super) fn source_row(&self, cx: &mut Context<Self>) -> AnyElement {
        outward("source", t!("settings-source"), None, cx)
            .on_click(|_, _, cx| cx.open_url(state::REPO_URL))
            .into_any_element()
    }

    /// The row that opens the repository to star it, with the count beside it once known.
    pub(super) fn star_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = Stars::global(cx).read(cx).label().map(SharedString::from);

        outward("star", t!("github-star"), count, cx)
            .on_click(|_, _, cx| cx.open_url(state::REPO_URL))
            .into_any_element()
    }

    /// The switch for the one request that reads the star count.
    pub(super) fn star_count_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.settings.read(cx).github_stars();

        self.row(
            t!("settings-star-count"),
            Switch::new("star-count", on)
                .name(t!("settings-star-count"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings
                        .update(cx, |settings, cx| settings.set_github_stars(!on, cx));
                }))
                .into_any_element(),
        )
    }

    /// Opens About with the app icon beside its name, aligned with the settings below. The icon
    /// is the one this platform shows, in its own colours whatever the theme.
    pub(super) fn mark(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let tile = theme.metrics.cover * MARK_TILE;

        div()
            .flex()
            .items_center()
            .gap_5()
            .child(img(APP_ICON).size(tile).flex_none())
            .child(
                div()
                    .id("settings-about-name")
                    .role(gpui::accesskit::Role::Heading)
                    .aria_level(2)
                    .aria_label(t!("settings-about-name"))
                    .text_size(theme.text(Text::Title))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("settings-about-name")),
            )
    }

    pub(super) fn notice(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();

        div()
            .id("settings-notice")
            .role(gpui::accesskit::Role::Paragraph)
            .aria_label(t!("settings-notice"))
            .text_color(theme.muted_foreground)
            .text_size(theme.text(Text::Small))
            .child(t!("settings-notice"))
    }
}

/// The notices file beside the executable, when the package put one there.
// shortcut: only the Windows installer's layout is known, widen once Linux and macOS packages exist
fn notices() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.with_file_name(NOTICES);
    path.exists().then_some(path)
}

/// Notepad on Windows refuses a file that Sonicly still writes to, so the button opens a copy.
/// `File::open` shares write access, and `fs::copy` documents no share mode.
fn snapshot(log: &Path) -> io::Result<PathBuf> {
    let copy = log.with_file_name("sonicly.copy.log");
    io::copy(&mut File::open(log)?, &mut File::create(&copy)?)?;
    Ok(copy)
}
