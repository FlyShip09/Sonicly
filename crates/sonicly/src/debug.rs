//! Launch arguments that only debug builds read, to put the app into a state a capture needs
//! without any input. They are taken off the command line before it is parsed, so a release
//! build hands them on as ordinary arguments.
//!
//! - `--debug-mini=card` or `--debug-mini=bar` opens the mini player, folded or not.
//! - `--debug-drop=<path>` shows the drag-over state as if `<path>` were dragged over the window.
//! - `--debug-ask=<path>` drops `<path>` on Local Music, which asks where songs from outside the
//!   music folders go when the profile has a music folder and no answer yet.
//! - `--debug-covers` shows the generated covers at the sizes they are drawn at.

use std::ffi::OsString;
use std::path::PathBuf;

use gpui::App;
use state::Sonicly;

/// What the debug arguments asked for.
#[derive(Default)]
pub struct Launch {
    mini: Option<bool>,
    drop: Option<PathBuf>,
    ask: Option<PathBuf>,
    covers: bool,
}

/// Takes the debug arguments out of `args`. Release builds take none.
pub fn take(args: Vec<OsString>) -> (Vec<OsString>, Launch) {
    let mut launch = Launch::default();
    if cfg!(not(debug_assertions)) {
        return (args, launch);
    }
    let kept = args
        .into_iter()
        .filter(|arg| {
            let text = arg.to_string_lossy();
            match text.split_once('=') {
                Some(("--debug-mini", fold)) => launch.mini = Some(fold == "bar"),
                Some(("--debug-drop", path)) => launch.drop = Some(PathBuf::from(path)),
                Some(("--debug-ask", path)) => launch.ask = Some(PathBuf::from(path)),
                _ if text == "--debug-covers" => launch.covers = true,
                _ => return true,
            }
            false
        })
        .collect();
    (kept, launch)
}

impl Launch {
    /// Puts the app into the states asked for, once the main window is open.
    pub fn apply(self, cx: &mut App) {
        if let Some(drop) = self.drop {
            let file_drop = state::FileDrop::entity(cx);
            file_drop.update(cx, |file_drop, cx| file_drop.preview(vec![drop], cx));
        }
        if let Some(ask) = self.ask {
            let file_drop = state::FileDrop::entity(cx);
            file_drop.update(cx, |file_drop, cx| {
                file_drop.drop_on(vec![ask], state::DropTarget::Library, cx)
            });
        }
        if self.covers {
            views::show_cover_specimens(cx);
        }
        if let Some(bar) = self.mini {
            let settings = Sonicly::global(cx).settings.clone();
            settings.update(cx, |settings, cx| settings.set_mini_player_bar(bar, cx));
            crate::mini::open(cx);
        }
    }
}
