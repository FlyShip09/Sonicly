//! Debug builds only: what `scripts/shot.ps1 -ExeArgs "--screen <what> --theme <kind> --size
//! <w>x<h>"` asks the window to show, so a capture reaches a page or an overlay without
//! synthesized input. The size is the window's own, since a window resized off-screen by the
//! script does not draw again at its new size.

use std::ffi::OsString;
use std::sync::OnceLock;

use gpui::{App, Pixels, SharedString, Size, px, size};
use router::{Destination, Screen, SettingsPage};
use state::Sonicly;
use views::Root;

static SIZE: OnceLock<Size<Pixels>> = OnceLock::new();

/// The page or overlay a capture run opens on, and the built-in theme it is drawn in.
#[derive(Default)]
pub struct Staging {
    screen: Option<String>,
    theme: Option<String>,
}

impl Staging {
    /// Takes `--screen <what>` and `--theme <kind>` out of `args`, leaving the rest for the
    /// command line parser.
    pub fn take(args: &mut Vec<OsString>) -> Self {
        let mut staging = Self::default();
        let mut index = 1;
        while index + 1 < args.len() {
            let value = args[index + 1].to_string_lossy().into_owned();
            match args[index].to_str() {
                Some("--screen") => staging.screen = Some(value),
                Some("--theme") => staging.theme = Some(value),
                Some("--size") => {
                    if let Some((width, height)) = value.split_once('x')
                        && let (Ok(width), Ok(height)) = (width.parse(), height.parse())
                    {
                        SIZE.set(size(px(width), px(height))).ok();
                    }
                }
                _ => {
                    index += 1;
                    continue;
                }
            }
            args.drain(index..index + 2);
        }
        staging
    }

    /// Applies the theme, and returns the page to open on: one named by `--screen` with its
    /// address, `start` otherwise. The theme is written to the capture profile's settings.
    pub fn start(&self, start: Destination, cx: &mut App) -> Destination {
        if let Some(theme) = self.theme.clone() {
            Sonicly::global(cx)
                .settings
                .clone()
                .update(cx, |settings, cx| settings.set_theme(theme, cx));
        }
        self.screen
            .as_deref()
            .and_then(destination)
            .unwrap_or(start)
    }

    /// Sizes the window again once it exists, since a window asked for more room than its
    /// display has opens at the display's default instead. Then opens what `--screen` names that
    /// needs the window: the quick search, a context menu, or the first album, artist or
    /// playlist Home lists.
    pub fn stage(&self, cx: &mut App) {
        let Some(window) = cx
            .windows()
            .first()
            .and_then(|window| window.downcast::<Root>())
        else {
            return;
        };
        if let Some(size) = window_size() {
            window.update(cx, |_, window, _| window.resize(size)).ok();
        }
        let Some(what) = self
            .screen
            .clone()
            .filter(|what| destination(what).is_none())
        else {
            return;
        };
        window
            .update(cx, |root, window, cx| root.stage(&what, window, cx))
            .ok();
    }
}

/// The window size `--size` asked for, which wins over the size the window was last closed at.
pub fn window_size() -> Option<Size<Pixels>> {
    SIZE.get().copied()
}

/// The page `what` names: a screen id, `settings:<page>`, or `album:<id>`, `artist:<id>` and
/// `playlist:<id>`.
fn destination(what: &str) -> Option<Destination> {
    let (kind, id) = what.split_once(':').unwrap_or((what, ""));
    let id = SharedString::from(id.to_owned());
    match kind {
        "settings" => Some(Destination::Settings(
            SettingsPage::from_id(&id).unwrap_or(SettingsPage::General),
        )),
        "album" | "artist" | "playlist" if id.is_empty() => None,
        "album" => Some(Destination::Album(id)),
        "artist" => Some(Destination::Artist(id)),
        "playlist" => Some(Destination::Playlist(id)),
        _ => Screen::from_id(kind).map(Screen::destination),
    }
}
