# Report

Written on the `integration` branch. It says what is built, what was checked in a running
Sonicly and how, what was not, and what is known to be wrong. Everything here was done on
Windows 11, with a debug build unless a line says release.

## What is built

- **Engine and library** ported from Sonora and owned by Sonicly (`docs/UPSTREAM.md`,
  `docs/DESIGN.md` 13): playback, queue, local library, lyrics, equalizer, the YouTube Music,
  Spotify, Tidal, Subsonic and Jellyfin providers, CLI control, tray, single instance.
- **Sources (M1):** PLS and XSPF playlists beside M3U, cue sheets that split one file into
  tracks, "Download from a link" through the user's own yt-dlp, sealed Spotify and YouTube
  sign-ins, `docs/PRIVACY.md`.
- **Identity (M2):** the frame on the system material, one type scale, edged covers, cover
  washes, shelf reveal, sidebar shortcuts, plainer empty and error states.
- **Lyrics style (M5):** Waves, Fluid, cover, picture and video backgrounds (video through
  Media Foundation, Windows only), typeface, weight, spacing, alignment, emphasis, saved looks.
- **Floating (M6):** the mini player, file drops into the library, drawn covers for Favorites,
  Local Music and Recents, and CLI requests reaching the running app on Windows.
- **Integration:** one module of debug launch flags (`crates/sonicly/src/debug.rs`), one
  `scripts/shot.ps1`, neutral sample lines in the lyrics settings preview, and the playback
  fix below.
- **Onboarding and settings (M8):** a welcome screen that leads with music on this computer,
  then the services, then browsing without an account; Home, Songs and the rest working with
  local files alone; Tab reaching buttons, switches and option cards; covers without art
  coloured from the album's name; dead controls replaced with one plain line (Discord).
- **Release (M8):** `LICENSE`, generated `THIRD-PARTY.md` (14 licences, 866 crates), a star
  count in the title bar and under Settings, About, a version in `settings.json` with
  migration steps, a portable Windows zip, and the installer script made Sonicly's own.
- **Removed for safety:** the code that downloaded and ran an installer to update the app.
- **Apple Music (M9):** back, played by Apple's own web player in a hidden page of the system
  web view (`docs/DESIGN.md` 37). Sonicly holds no decryption module, asks for no licence and
  receives no audio, so these tracks have no equalizer, visualizer or download.
- **Look and motion (M9):** the built-in themes moved to calm desktop neutrals, a full-height
  source-list sidebar with a highlight that glides, a unified title bar, a floating player
  bar, grouped settings lists, one soft shadow for raised surfaces, a sliding segmented
  control, pages that enter from the side of the move, a title that rises in on a track
  change, a heart that pops only from a click on it (`docs/DESIGN.md` 38-42).

## What was verified, and how

Every run was started by `scripts/shot.ps1` (off-screen, unfocused) in a throwaway profile with
`--debug-mute`, so the volume stayed at 0 and nothing was heard. The running app was asked over
its own CLI (`target/debug/sonicly.exe --json current`, with `SONICLY_PROFILE` set to the same
profile).

"Sound reaches the output" is shown without the speakers: `--debug-level=<file>` holds a watch
on the engine's analyzer, which hears the decoded music as the output pulls it, ahead of the
equalizer and the volume, and writes the position and the loudest band in dBFS twice a second.
While a track plays the level moves between about -33 and -22 dBFS; once the queue ends it
falls to the -200 floor within seconds; while paused it holds its last value, so a level counts
only together with a moving position.

After the merge the lead re-ran `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and `cargo build -p sonicly` (all pass),
read the playback fix, and captured Home and the lyrics view of the merged build with a local
file playing.

### Local playback, one file per format

Files from `.targets/media`. "Position" is two CLI readings (whole seconds) about 5.7 s apart;
the level log of the same run covers the first 8 to 11 s in half-second steps and advanced
0.51 s per step in every run.

| Format | File | Length from tags | Position | Level above silence | Lyrics beside the file |
| --- | --- | --- | --- | --- | --- |
| FLAC | Paper Lanterns/01 Paper Lanterns.flac | 26 s | 2 s to 8 s in 5.7 s | yes, -32.8 to -23.7 dBFS | found, synced |
| MP3 | Salt & Cedar/01 Salt & Cedar.mp3 | 21 s | 2 s to 8 s in 5.7 s | yes, -32.8 to -24.2 dBFS | none beside it |
| AAC (.m4a) | Night Transit/02 Night Transit.m4a | 24 s | 2 s to 8 s in 6.1 s | yes, -30.9 to -24.5 dBFS | found, synced |
| ALAC (.m4a) | Glasshouse/01 Glasshouse.m4a | 24 s | 2 s to 8 s in 5.8 s | yes, -30.1 to -24.5 dBFS | found, synced |
| Opus | 雨の庭/01 苔 (Moss).opus | 26 s | 2 s to 8 s in 5.6 s | yes, -32.2 to -21.9 dBFS | none beside it |
| Vorbis (.ogg) | Fjordlys/01 Fjordlys I- Dæmring.ogg | 22 s | 2 s to 8 s in 5.6 s | yes, -32.8 to -22.8 dBFS | none beside it |
| AIFF | Late Bloom/01 Peony.aiff | 16 s | 2 s to 8 s in 5.6 s | yes, -30.7 to -24.7 dBFS | none beside it |
| WAV | Demos (No Artwork)/01 Teaspoon.wav | 13 s | 2 s to 9 s in 6.6 s | yes, -31.5 to -23.4 dBFS | none beside it |

All eight decode and play. The lengths match what ffprobe reads from the same files. Titles and
artists came from the tags, the Japanese and Nordic ones included.

### Embedded artwork

Three albums were copied to `.targets/media-art` with a cover embedded by ffmpeg (FLAC picture
block, MP3 ID3v2 APIC, M4A `covr`). Home showed each album's cover on its tile, on its tracks
in Quick picks and on its artist; Now playing showed the FLAC's cover and took the background's
colour from it.

### Controls, with FLAC

One run of `01 Paper Lanterns.flac` with `02 Harbor Road.flac` added through `sonicly add`:

| Step | Result |
| --- | --- |
| `pause` | paused at 1 s, still 1 s five seconds later |
| `play` | playing, 5 s four seconds later |
| `seek 0:18` | playing at 19 s one second later |
| `next` | Harbor Road playing from the start |
| `prev` | Paper Lanterns playing from the start |
| `next`, `seek 0:10`, process killed at 13 s | |
| relaunch with no file | Harbor Road paused at 10 s, then `play` continues from there with sound |

The position restored after a kill is the last one saved, which is written on a seek or pause
and otherwise every 5 s of playing.

### Playlists and cue sheets

Hand-written files in `.targets/integration/lists`, imported with `--debug-import` (the same
call the Import playlist button makes) into a profile whose library was pointed at
`.targets/media` and `.targets/integration/cuelib` with `--debug-folder`. Results were read from
the profile's `state.sqlite` and a capture of the Playlists page.

| File | Entries | Imported |
| --- | --- | --- |
| `list-m3u.m3u8` | a relative path, an absolute path, a Japanese path, a file not on disk, the cue-split WAV | 4 tracks: the three files, and one of the WAV's two cue tracks. The missing file was left out |
| `list-pls.pls` | a relative path with backslashes, an absolute path with slashes, a percent-encoded `file://` URL | 3 of 3 |
| `list-xspf.xspf` | a percent-encoded `file://` URL with non-ASCII letters, a percent-encoded relative path, a relative path with spaces and `&amp;` | 3 of 3 |
| `cuelib/big.cue` beside `big.wav` (28 s, two test WAVs joined with ffmpeg) | two tracks, the second at 00:13:00 | the library lists "Teaspoon (cue)", 13 s, and "Back Burner (cue)", 15 s, in an album "Demos, Joined"; the WAV itself is not listed |

Played with `--debug-play=big.wav`: the first cue track ran from 0 to 13 s, the second followed
without a pause and ran from 0 to 15 s, with sound throughout, and the queue then ended. After a
kill 10 s into the second track, a relaunch restored that cue track paused at 10 s.

### The playback freeze

Reported: in a capture profile playback sometimes stayed at 0:00 until `state.sqlite` was
deleted. Reproduced in 2 of 5 relaunches: play a file, kill the process mid-track, start again
opening the same file.

Cause: on start Sonicly restores the last track and has the engine hold it paused at the saved
position. If the same track is opened before the engine's "paused" for that hold has been read,
the event carries the id of the track now loading and was taken as being about it. It set the
intent to pause, and the load waiting to be sent then held the track at 0:00 instead of loading
it. Opening a different file never froze, since its id does not match. The same code is in
upstream; the removal of the alternate Spotify engine dropped nothing the restore path needs.

Fix, in `crates/state/src/playback.rs`: a load is `unsent` until it reaches the engine, and
while it is, engine events that name the track are dropped, as events about another track
already were. After the fix, 6 of 6 relaunches on the same saved state played. No saved state
is deleted.

### Apple Music, as a signed-out visitor

Run muted through `scripts/shot.ps1` with `--debug-guest=apple`, which plays the unprotected
previews Apple gives anyone.

| Check | Result |
| --- | --- |
| Catalog | `sonicly search "daft punk discovery"` returned songs, the artist and the album |
| Position | 5, 8, 11 s at 3 s intervals |
| `pause`, `play` | held at 12 s, then playing at 14 s |
| `seek 0:20` | 23 s three seconds later |
| `next`, and the end of a preview | the next track played in both cases |
| Hidden page | no window of the web view was visible, focused or on the taskbar |
| Lifecycle | no web view before the first play; closed 3 minutes after a pause; reopened on `play` and resumed |
| Memory | Sonicly 120-131 MB; the web view's seven processes 500-531 MB summed while a track plays |

Read by the lead: the script run in Apple's page (`crates/music/src/apple/player.js`) calls
only the player's own queue, play, pause, seek, volume and stop, and posts state back. A grep
of the tree finds no key handling, licence request or decryption, and no DRM dependency.

### The look, seen by the lead in the merged build

Home with a local library (dark), Songs, an album page (light), Settings, General, the welcome
screen with Apple Music listed, a menu, and the floating player bar. Two faults found in those
captures were fixed and seen fixed: Home showed only Quick picks when no account was signed in
(a sign-out settling at startup cancelled the local feed), and rows passing under the Settings
header stayed legible through it.

Cost of the new look, Home at 1080x1080 on the test library, one run each: 114 MB idle before
and after; CPU between 12 s and 42 s after start 0.3 s before, 0.1 s after; with a track
playing 131 MB and 0.6 s before, 126 MB and 0.7 s after.

## Layout, components and the last motion items (M10, M11)

Built: a seek bar that swells under the pointer, clipped titles that scroll once on hover, a
queue row that closes its gap, a compact title in the title bar on scrolled album, artist and
playlist pages, a hairline under the title bar once a page has scrolled, quick search answers
grouped by kind, slim pinned rows; Home opening with Continue listening and Recently played;
album and playlist pages in two columns when wide; artist pages as rails (Albums, Singles and
EPs, Appears on) drawn only for data that exists; a hover card on artist and album names; the
queue panel sliding in; a count that runs to its new value; a scan ring; a quality badge drawn
from the stream the engine reports; shortcut and destructive styling in menus
(`docs/DESIGN.md` 43-51). Patterns were read from shadcn/ui and Magic UI and rebuilt with
Sonicly's own elements; Magic UI's glows, particles and shine borders were left out on purpose.

Seen by the lead in the merged build: the compact title appearing and leaving on a scrolled
album page (frame sequence); Home with Continue listening, Recently played and real shelves
(YouTube Music as a guest); the two-column album page at 1440 wide; the artist page's rails;
after a relaunch with local files alone, the last song back paused at 0:10 of 0:18 with the
badge reading `FLAC 44.1 kHz / 16 bit`, and the play recorded in the history.

Found in that review and fixed: with local files alone there was no play history and no
restored song (both were keyed to a signed-in service); an artist without a biography showed a
stock sentence, which now says no biography is available.

Not seen: the hover card (debug pointer moves raise no hover surface), the menus' new look
(the debug steps cannot right-click), grouped quick search on screen, shelf snapping.

Cost, Home at 1080x1080 on the test library, debug: 119 MB idle and 0.2 s of CPU between 12 s
and 42 s after start; 129-130 MB and 0.7-0.9 s with a track playing.

## Errors pass

Started from the owner's own log, then a hunt through the app without an account.

| Fault | Cause | Fix | Seen or reasoned |
| --- | --- | --- | --- |
| The settings file had stopped loading, so no change was being saved | `discord_name` and `spotify_connect_name` held the value an early build wrote, and one unreadable value fails the whole file | Both values are read again; unit tests | The owner's own file loads in a scratch profile. Tests compile, not run |
| Heart as a guest answered "could not be added" (401) | Library changes were sent whatever the session | `state::has_account` at the three places every save, library change and playlist change passes through; a toast names the service and opens Settings, Providers | Seen: song heart, album heart, `sonicly like on`. Reasoned: menus, artist follow, playlist follow |
| Failure toasts gave no reason | Errors were logged and dropped | The reason rides on the toast's second line; a track that will not play carries the engine's reason | Seen for a damaged local file |
| WASAPI "stream configuration is no longer valid" | Device or format change | None needed: the output is retired and the engine reloads the track where it was heard | The owner's log shows the reopen in the same second; position read from code |
| `gpui::window: RefCell already borrowed` | Taskbar and tray shell calls made inside an app update let Windows deliver window messages gpui then drops | Both make their shell calls from a task of their own | Mechanism seen (a message inside the call on five of five launches); none since in the logs kept, which is not proof |
| Debug `key:` steps crashed | The step lent out `Root` while a key press redraws it | Steps go through the untyped window handle; `key:a` types | Seen: typing, Enter, Escape in quick search |
| Search found nothing in a files-only library | Ranking read the streaming shelf only | Both shelves ranked | Seen |
| Junk files listed as songs | Any file with an audio extension became a track | Left out unless a reader knows it as audio | Seen, 6 of 6 |
| Home showed only Quick picks with no account | A sign-out settling at startup cancelled the local feed | The local feed is asked for again after it | Seen by the lead |

Added: `Playback::stream_info` and `stream` in `sonicly --json current`, read for the eight
local formats (the lead read `FLAC, 44100 Hz, 16 bit, 2 channels` from a playing file in the
merged build). Nothing is guessed: a service that does not say reports nothing.

Known from this pass and not fixed:

- Files that are not audio and were indexed before this build stay listed until Rescan.
- The Apple Music visitor mode (`--debug-guest=apple`) asks for the account's home feed and
  recents and logs three 403s.
- A seek past the end of a track logs a decoder warning before moving on.
- An Ogg file cut short lists as 0:00.
- One unreadable value still fails the whole `settings.json`; the app then names the line.
- After a relaunch the app reloads a track that failed and fails on it again at start.
- `sonicly search` on the command line answers only from a signed-in service, not local files.
- `data/library.db` under the config folder, left by the first from-scratch build, is never
  read.

Not exercised in this pass: rescan after files change, queue editing, playlist export,
the equalizer, settings toggles across a relaunch, the mini player, file open, `sonicly://`
links, single-instance hand-over, guest context-menu saves and follows, Apple previews.

## Memory

Working set of the debug build as `scripts/shot.ps1` prints it.

| State | Working set |
| --- | --- |
| Home with the test library, merged build, 1080x1080 | 130 MB (107 MB private) |
| Mini player open, merged build | 137 MB |
| Now playing with lyrics, a local file playing, merged build | 137-151 MB |
| Playlists page, a local file playing, merged build | 139 MB |
| Lyrics with a video background (measured on its own branch) | 206-210 MB |

### Release build (Windows x64, version 0.1.0)

| Measure | Value |
| --- | --- |
| `sonicly.exe` | 82.6 MB, GUI subsystem, Sonicly icon and version resource |
| Portable zip | 30.1 MB (`scripts/windows/package-portable.ps1`) |
| Process start to first window | 716 ms |
| Home at 1080x1080 after 8 s, YouTube Music guest | 145 MB working set, 150 MB private, 0.9 s CPU |

One run each, not an average. Those figures are from the build before the M9 look. Rebuilt
from the final branch: the portable zip is 30.5 MB, and a first run (the welcome screen) shows
its window 602 ms after start at 111 MB working set, 91 MB private.

### A 10,000-track library (debug build)

10,000 one-second MP3 files in 100 artist folders (`.targets/big`), Songs page open at
1080x1080.

| Measure | Value |
| --- | --- |
| Working set after relaunch with the library already scanned | 149-151 MB (121 MB private) |
| Process start to first window | 592-778 ms |
| CPU time between 12 s and 42 s after start, nothing playing | about 0.8 s, under 3% of one core |

The list draws only the rows on screen. Scrolling it was not measured: the capture window takes
no input, so frame times at 60 or 120 Hz are still unknown.

## Not verified

- Apple Music with an account: the sign-in window, the library, a whole protected track, and
  the not-subscribed and sign-in-again errors. It needs the owner to sign in inside the app.
  If the web view refuses protected audio in its private, hidden page, the listener is told
  so; the fix would be a non-private profile for that page.
- Anything that needs a pointer or a keyboard: hover states (card play button, shelf arrows,
  row hover), the heart pop, Tab order and focus rings, drag and drop from the system, and
  scrolling. Captures take no input.
- The frame's Mica material on an active window: captures show the inactive flat fallback.
- User theme files over the new structure.
- The Windows installer compiles (`Sonicly-0.1.0-windows-x64-setup.exe`, 23.4 MB) but has
  never been run: installing it is the owner's to try. Nothing is code-signed, so SmartScreen
  will warn.
- Scrolling frame times, and the fix for playlists that name a cue-split file (unit test
  written, not run; `cargo test` is run only when the owner asks).

- Streaming from YouTube Music, Spotify, Tidal, Subsonic or Jellyfin, any sign-in, and a yt-dlp
  download: these need the owner.
- Anything at audible volume: what was shown is that decoded, non-silent audio is pulled by the
  output chain at real time, not what it sounds like.
- Gapless joins between files, loudness normalization, the equalizer, output device switching,
  OS media controls.
- `.ttml` lyrics (no test file has them).
- A real drag from the system onto the window; the lyrics video background and the Fluid field
  in the merged build (seen on their own branch only).
- macOS and Linux: nothing was built or run there.
- `cargo test`: not run on `integration`, as the house rules ask.
- Pause, seek and resume were checked with FLAC only.

## Known issues

- `gpui::window: RefCell already borrowed` is logged at startup now and then (twice in about
  thirty launches). Nothing visibly failed. Cause not looked into.
- A late engine event read after a load has reached the engine can still show the track as
  paused for a moment before the engine's "playing" corrects it. It no longer stops the load.
- No test covers the `unsent` fix; it was checked by reproduction only.
- A hard kill loses up to 5 s of position.
- Every CLI request logs `single: cannot bound the read: named pipes do not support I/O
  timeouts` on Windows.
- The periodic memory line in the log reads `rss unknown, heap 0.0 MiB` on Windows.
- By default a local file's title, artist, album and length are sent to the lyrics sources, and
  a first launch opens a YouTube Music guest session. Both are described in `docs/PRIVACY.md`.
- `--debug-mute` and `--debug-theme` write the volume and the theme into the profile they run
  in, so they belong in throwaway profiles.
- A window asked for a size taller than the display opens at the display's height.
- YouTube's rotating cookies can lie unsealed for up to a minute (`docs/DESIGN.md` 16).
- The lyrics video background is Windows only.
- While Apple Music plays, the system web view adds about half a gigabyte of memory.
- Apple Music works on Windows only. Linux's web view has no licensed player; macOS needs a
  message channel and cookie seeding that are not written.
- Apple Music uses the web player's own access token, as upstream did, not one registered to
  Sonicly. Apple may block that.
- Not animated on purpose: a level meter on the playing row and the sidebar's width, since
  both would redraw the window every frame. Not done: the seek bar growing on hover, titles
  scrolling on hover, list rows closing a gap, a sticky title on scrolled detail pages, a
  hairline under the title bar when content scrolls beneath it.
- Pinned sidebar entries are still 52 px cards rather than 28 px rows.
- The text size default moved from 14 to 13 px, and a saved size of exactly 14 moves with it.
