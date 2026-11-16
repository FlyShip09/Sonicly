//! Launch arguments that only debug builds read, to put the app into a state a capture or a
//! check needs without any input. The module is compiled in debug builds alone, so a release
//! build hands these on to the command line parser as ordinary arguments. Every flag is spelled
//! `--debug-<name>` or `--debug-<name>=<value>`, which passes through
//! `scripts/shot.ps1 -ExeArgs "..."` without quoting games, and they combine freely.
//!
//! - `--debug-screen=<what>` opens on a page or lays something over it. A page is a screen id
//!   (`home`, `search`, `history`, `songs`, `albums`, `artists`, `playlists`),
//!   `settings:<page>` (`settings:lyrics`), `equalizer`, or `album:<id>`, `artist:<id>` and
//!   `playlist:<id>`.
//!   `now-playing` opens the fullscreen view and `lyrics` opens it on its lyrics panel.
//!   `powerbar` opens the quick search, and `menu`, `album`, `artist` and `playlist` without an
//!   id open a context menu or the first of that kind Home lists.
//! - `--debug-theme=<kind>` draws in that built-in theme, written to the profile's settings.
//! - `--debug-size=<w>x<h>` opens the window at that size, past the display if asked. The size
//!   is the window's own, since a window resized off-screen by the script does not draw again.
//! - `--debug-mute` plays at no volume, so a capture that opens a song makes no sound.
//! - `--debug-open=<path>` opens a file or a playlist file as if it were passed on its own.
//! - `--debug-mini=card` or `--debug-mini=bar` opens the mini player, folded or not.
//! - `--debug-drop=<path>` shows the drag-over state as if `<path>` were dragged over the window.
//! - `--debug-ask=<path>` drops `<path>` on Local Music, which asks where songs from outside the
//!   music folders go when the profile has a music folder and no answer yet.
//! - `--debug-covers` shows the generated covers at the sizes they are drawn at.
//! - `--debug-folder=<path>` adds a music folder to the profile's local library and scans it.
//!   It may be given more than once.
//! - `--debug-import=<path>` imports an M3U, PLS or XSPF playlist once no scan is under way, as
//!   the Import playlist button would. It may be given more than once.
//! - `--debug-play=<text>` plays the local library's tracks whose id, which holds the file's
//!   path, contains `<text>`, once no scan is under way: `big.wav` plays the tracks a cue sheet
//!   splits that file into.
//! - `--debug-level=<path>` appends a line to `<path>` twice a second: the position in seconds
//!   and the loudest band of the music in dBFS, heard ahead of the equalizer and the volume, so
//!   a muted run can still show that sound reaches the output chain. `-200.0` is silence, and
//!   `none` means no audio has been pulled yet.

use std::ffi::OsString;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use gpui::{App, AppContext as _, Pixels, SharedString, Size, px, size};
use router::{Destination, Screen, SettingsPage};
use state::{SideTab, Sonicly};
use views::Root;

/// The window size `--debug-size` asked for. A static, since the window can open again later.
static SIZE: OnceLock<Size<Pixels>> = OnceLock::new();

/// What the debug arguments asked for.
#[derive(Default)]
pub struct Launch {
    screen: Option<String>,
    theme: Option<String>,
    mute: bool,
    mini: Option<bool>,
    drop: Option<PathBuf>,
    ask: Option<PathBuf>,
    covers: bool,
    folders: Vec<PathBuf>,
    imports: Vec<PathBuf>,
    play: Option<String>,
    level: Option<PathBuf>,
}

/// How often `--debug-level` writes a line, and `--debug-import` looks whether the scan is over.
const TICK: Duration = Duration::from_millis(500);

/// Takes the debug arguments out of `args`, leaving the rest for the command line parser.
pub fn take(args: Vec<OsString>) -> (Vec<OsString>, Launch) {
    let mut launch = Launch::default();
    let mut opened = Vec::new();
    let mut kept: Vec<OsString> = args
        .into_iter()
        .filter(|arg| {
            let text = arg.to_string_lossy();
            let (name, value) = text.split_once('=').unwrap_or((&text, ""));
            match name {
                "--debug-screen" => launch.screen = Some(value.to_owned()),
                "--debug-theme" => launch.theme = Some(value.to_owned()),
                "--debug-size" => {
                    parse_size(value).map(|size| SIZE.set(size));
                }
                "--debug-mute" => launch.mute = true,
                "--debug-open" => opened.push(OsString::from(value)),
                "--debug-mini" => launch.mini = Some(value == "bar"),
                "--debug-drop" => launch.drop = Some(PathBuf::from(value)),
                "--debug-ask" => launch.ask = Some(PathBuf::from(value)),
                "--debug-covers" => launch.covers = true,
                "--debug-folder" => launch.folders.push(PathBuf::from(value)),
                "--debug-import" => launch.imports.push(PathBuf::from(value)),
                "--debug-play" => launch.play = Some(value.to_owned()),
                "--debug-level" => launch.level = Some(PathBuf::from(value)),
                _ => return true,
            }
            false
        })
        .collect();
    kept.extend(opened);
    (kept, launch)
}

impl Launch {
    /// Applies what has to be in place before the window opens: the theme, the volume and the
    /// fullscreen panel. Returns the page to open on, the one `--debug-screen` names by its
    /// address or `start` otherwise.
    pub fn start(&self, start: Destination, cx: &mut App) -> Destination {
        let settings = Sonicly::global(cx).settings.clone();
        if let Some(theme) = self.theme.clone() {
            settings.update(cx, |settings, cx| settings.set_theme(theme, cx));
        }
        if self.screen.as_deref() == Some("lyrics") {
            settings.update(cx, |settings, cx| {
                settings.set_fullscreen_tab(Some(SideTab::Lyrics), cx)
            });
        }
        if self.mute {
            let playback = Sonicly::global(cx).playback.clone();
            playback.update(cx, |playback, cx| playback.set_volume(0., cx));
        }
        self.screen
            .as_deref()
            .and_then(destination)
            .unwrap_or(start)
    }

    /// Puts the app into the states asked for, once the main window is open. The window is
    /// sized again here, since one asked for more room than its display has opens at the
    /// display's default instead.
    pub fn apply(self, cx: &mut App) {
        let window = cx
            .windows()
            .into_iter()
            .find_map(|window| window.downcast::<Root>());
        if let (Some(window), Some(size)) = (window, window_size()) {
            window.update(cx, |_, window, _| window.resize(size)).ok();
        }
        match self.screen.as_deref() {
            Some("now-playing" | "lyrics") => router::navigate(Destination::Fullscreen, cx),
            Some(what) if destination(what).is_none() => {
                if let Some(window) = window {
                    window
                        .update(cx, |root, window, cx| root.stage(what, window, cx))
                        .ok();
                }
            }
            _ => {}
        }
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
        let library = Sonicly::global(cx).library.clone();
        if !self.folders.is_empty() {
            library.update(cx, |library, cx| {
                library.add_local_folders(self.folders, cx)
            });
        }
        if !self.imports.is_empty() || self.play.is_some() {
            let session = Sonicly::global(cx).session.clone();
            let playback = Sonicly::global(cx).playback.clone();
            let io = state::Io::global(cx);
            let (imports, play) = (self.imports, self.play);
            cx.spawn(async move |cx| {
                // a playlist only takes the songs the library lists, so the scan goes first
                loop {
                    cx.background_executor().timer(TICK * 2).await;
                    if !session.read_with(cx, |session, _| session.scanning()) {
                        break;
                    }
                }
                for playlist in imports {
                    library.update(cx, |library, cx| library.import_playlist(playlist, cx));
                    // shortcut: the library takes one change at a time and says nothing here
                    // when one ends, so each import gets a second; watch the library if a
                    // playlist ever takes longer.
                    cx.background_executor().timer(TICK * 2).await;
                }
                let Some((wanted, client)) =
                    play.zip(session.read_with(cx, |session, _| session.local_client()))
                else {
                    return;
                };
                let listed = io.spawn(async move { client.all_tracks().await }).await;
                let Ok(Ok(mut tracks)) = listed else {
                    return log::warn!("debug: cannot list the local library");
                };
                tracks.retain(|track| track.id.as_deref().is_some_and(|id| id.contains(&wanted)));
                playback.update(cx, |playback, cx| playback.start(tracks, 0, None, cx));
            })
            .detach();
        }
        if let Some(path) = self.level {
            let playback = Sonicly::global(cx).playback.clone();
            let watch = std::sync::Arc::new(playback.read(cx).analyzer().watch());
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(TICK).await;
                    let reading = watch.clone();
                    let levels = cx.background_spawn(async move { reading.read() }).await;
                    let loudest = levels
                        .and_then(|levels| levels.iter().map(|level| level.dbfs).reduce(f32::max))
                        .map_or_else(|| "none".to_owned(), |dbfs| format!("{dbfs:.1}"));
                    let position = playback.read_with(cx, |playback, _| playback.live_position());
                    let line = format!(
                        "{:.1} {loudest}
",
                        position.as_secs_f32()
                    );
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path);
                    if let Err(error) = file.and_then(|mut file| file.write_all(line.as_bytes())) {
                        return log::warn!("debug: cannot write {}: {error}", path.display());
                    }
                }
            })
            .detach();
        }
    }
}

/// The window size `--debug-size` asked for, which wins over the size the window was last
/// closed at.
pub fn window_size() -> Option<Size<Pixels>> {
    SIZE.get().copied()
}

/// The size `<w>x<h>` spells, in the window's own pixels.
fn parse_size(value: &str) -> Option<Size<Pixels>> {
    let (width, height) = value.split_once('x')?;
    Some(size(px(width.parse().ok()?), px(height.parse().ok()?)))
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
        "equalizer" => Some(Destination::Equalizer),
        "album" | "artist" | "playlist" if id.is_empty() => None,
        "album" => Some(Destination::Album(id)),
        "artist" => Some(Destination::Artist(id)),
        "playlist" => Some(Destination::Playlist(id)),
        _ => Screen::from_id(kind).map(Screen::destination),
    }
}
