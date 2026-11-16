# Report

Written on the `integration` branch after the four feature branches were merged. It says what
is built, what was checked in a running Sonicly and how, what was not, and what is known to be
wrong. Everything here was done on Windows 11 with a debug build. It is updated as later
milestones land.

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
  `scripts/shot.ps1`, design decisions renumbered 14-28, neutral sample lines in the lyrics
  settings preview, and the playback fix below.

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
artists came from the tags, the Japanese and Nordic ones included. None of the test files
carries embedded art, so the covers are the placeholder.

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

### Screens seen in the merged build

Home, Playlists, Albums (light theme), Lyrics settings with the new sample lines, the mini
player, and Now playing with synced lyrics.

## Memory

Working set of the debug build as `scripts/shot.ps1` prints it.

| State | Working set |
| --- | --- |
| Home with the test library, merged build, 1080x1080 | 130 MB (107 MB private) |
| Mini player open, merged build | 137 MB |
| Now playing with lyrics, a local file playing, merged build | 137-151 MB |
| Playlists page, a local file playing, merged build | 139 MB |
| Lyrics with a video background (measured on its own branch) | 206-210 MB |

No release build has been measured yet.

## Not verified

- Streaming from YouTube Music, Spotify, Tidal, Subsonic or Jellyfin, any sign-in, and a yt-dlp
  download: these need the owner.
- Anything at audible volume: what was shown is that decoded, non-silent audio is pulled by the
  output chain at real time, not what it sounds like.
- Gapless joins between files, loudness normalization, the equalizer, output device switching,
  OS media controls.
- Embedded artwork and `.ttml` lyrics (the test files have neither).
- A real drag from the system onto the window; the lyrics video background and the Fluid field
  in the merged build (seen on their own branch only).
- macOS and Linux: nothing was built or run there.
- `cargo test`: not run on `integration`, as the house rules ask.
- Pause, seek and resume were checked with FLAC only.

## Known issues

- `gpui::window: RefCell already borrowed` is logged at startup now and then (twice in about
  thirty launches). Nothing visibly failed. Cause not looked into.
- A playlist that names a file split by a cue sheet takes one of the sheet's tracks, not all of
  them and not the file.
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
- Albums and artists without artwork all draw the same placeholder, so a library with no
  embedded art reads as a wall of identical tiles.
