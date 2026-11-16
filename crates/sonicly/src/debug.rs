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
//! - `--debug-guest=<slug>` signs in to the service `<slug>` with no account, where it has such
//!   a session. `apple` has one only under this flag: Apple's catalog and, in the hidden web
//!   player, the 30-second previews Apple gives any visitor, which is how the web player is
//!   checked without an account. It also turns lyrics off in the profile, so the run looks
//!   nothing up anywhere else.
//! - `--debug-play-id=<id>[,<id>]` plays the signed-in service's tracks with those ids, in
//!   order, once it is signed in.
//! - `--debug-level=<path>` appends a line to `<path>` twice a second: the position in seconds
//!   and the loudest band of the music in dBFS, heard ahead of the equalizer and the volume, so
//!   a muted run can still show that sound reaches the output chain. `-200.0` is silence, and
//!   `none` means no audio has been pulled yet.
//! - `--debug-at=<ms>:<step>` does `<step>` that many milliseconds after the window opens, so a
//!   capture can catch motion. It may be given more than once. A step is a pointer event at
//!   window coordinates in logical pixels (`move:x,y`, `down:x,y`, `up:x,y`, `click:x,y`,
//!   `wheel:x,y,lines`), a keystroke (`key:ctrl-k`, `key:enter`; a bare `key:a` or `key:space`
//!   types into the focused field), `toggle`, `next` or `prev` on the player,
//!   `back` or `forward` in the history, or `go:<what>` with anything `--debug-screen` takes.

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
    guest: Option<String>,
    play_ids: Vec<String>,
    level: Option<PathBuf>,
    steps: Vec<(Duration, String)>,
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
                "--debug-guest" => launch.guest = Some(value.to_owned()),
                "--debug-play-id" => {
                    launch.play_ids = value.split(',').map(str::to_owned).collect();
                }
                "--debug-level" => launch.level = Some(PathBuf::from(value)),
                "--debug-at" => {
                    if let Some((at, step)) = value.split_once(':')
                        && let Ok(at) = at.parse()
                    {
                        launch
                            .steps
                            .push((Duration::from_millis(at), step.to_owned()));
                    }
                }
                _ => return true,
            }
            false
        })
        .collect();
    kept.extend(opened);
    (kept, launch)
}

impl Launch {
    /// The service `--debug-guest` named.
    pub fn guest(&self) -> Option<&str> {
        self.guest.as_deref()
    }

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
        if self.guest.is_some() {
            // a check of one service's player then reaches that service and nobody else
            settings.update(cx, |settings, cx| settings.set_lyrics_enabled(false, cx));
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
        if let Some(slug) = self.guest {
            let session = Sonicly::global(cx).session.clone();
            session.update(cx, |session, cx| {
                session.sign_in(&slug, music::SignIn::Anonymous, cx)
            });
        }
        if !self.play_ids.is_empty() {
            let session = Sonicly::global(cx).session.clone();
            let playback = Sonicly::global(cx).playback.clone();
            let io = state::Io::global(cx);
            let ids = self.play_ids;
            cx.spawn(async move |cx| {
                let client = loop {
                    cx.background_executor().timer(TICK).await;
                    let client = session.read_with(cx, |session, _| {
                        session.playback().and_then(|_| session.client())
                    });
                    if let Some(client) = client {
                        break client;
                    }
                };
                let found = io
                    .spawn(async move {
                        let mut tracks = Vec::new();
                        for id in ids {
                            tracks.push(client.track(&id).await?);
                        }
                        anyhow::Ok(tracks)
                    })
                    .await;
                match found {
                    Ok(Ok(tracks)) => {
                        playback.update(cx, |playback, cx| playback.start(tracks, 0, None, cx))
                    }
                    Ok(Err(error)) => log::warn!("debug: cannot read a track to play: {error:#}"),
                    Err(error) => log::warn!("debug: cannot read a track to play: {error:#}"),
                }
            })
            .detach();
        }
        if !self.steps.is_empty()
            && let Some(window) = window
        {
            let mut steps = self.steps;
            steps.sort_by_key(|(at, _)| *at);
            cx.spawn(async move |cx| {
                let mut waited = Duration::ZERO;
                for (at, step) in steps {
                    cx.background_executor()
                        .timer(at.saturating_sub(waited))
                        .await;
                    waited = at;
                    cx.update(|cx| perform(&step, window, cx));
                }
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

/// Does one `--debug-at` step on the main window.
fn perform(step: &str, window: gpui::WindowHandle<Root>, cx: &mut App) {
    use gpui::{
        Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, PlatformInput, ScrollDelta, ScrollWheelEvent, point,
    };

    let (kind, value) = step.split_once(':').unwrap_or((step, ""));
    let numbers: Vec<f32> = value.split(',').filter_map(|n| n.parse().ok()).collect();
    let position = point(
        px(numbers.first().copied().unwrap_or(0.)),
        px(numbers.get(1).copied().unwrap_or(0.)),
    );
    let modifiers = Modifiers::default();
    let down = |click_count| {
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        })
    };
    let up = PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers,
        click_count: 1,
    });
    let events = match kind {
        "move" => vec![PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers,
        })],
        "down" => vec![down(1)],
        "up" => vec![up],
        "click" => vec![down(1), up],
        "wheel" => vec![PlatformInput::ScrollWheel(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Lines(point(0., numbers.get(2).copied().unwrap_or(-1.))),
            modifiers,
            touch_phase: Default::default(),
        })],
        "key" => {
            let Ok(keystroke) = Keystroke::parse(value) else {
                return log::warn!("debug: cannot read the keystroke {value}");
            };
            // Through the untyped handle, as the platform sends its own events: the typed one
            // lends out `Root` for the length of the call, and a key press draws the window,
            // which renders `Root`. A key no binding takes types its character.
            cx.update_window(window.into(), |_, window, cx| {
                window.dispatch_keystroke(keystroke, cx);
            })
            .ok();
            return;
        }
        _ => {
            let playback = Sonicly::global(cx).playback.clone();
            match kind {
                "toggle" => playback.update(cx, |playback, cx| playback.toggle_play(cx)),
                "next" => playback.update(cx, |playback, cx| playback.next(cx)),
                "prev" => playback.update(cx, |playback, cx| playback.previous(cx)),
                "back" => router::back(cx),
                "forward" => router::forward(cx),
                "go" if matches!(value, "now-playing" | "lyrics") => {
                    router::navigate(Destination::Fullscreen, cx)
                }
                "go" => match destination(value) {
                    Some(destination) => router::navigate(destination, cx),
                    None => {
                        window
                            .update(cx, |root, window, cx| root.stage(value, window, cx))
                            .ok();
                    }
                },
                _ => log::warn!("debug: no step called {step}"),
            }
            return;
        }
    };
    cx.update_window(window.into(), |_, window, cx| {
        for event in events {
            window.dispatch_event(event, cx);
        }
    })
    .ok();
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
