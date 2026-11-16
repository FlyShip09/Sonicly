# Parity with upstream

Status of each feature of the engine Sonicly was ported from. "Ported" means the
code is in the tree and compiles; "seen" means it has been observed working in a
running Sonicly.

| Area | Feature | Status |
| --- | --- | --- |
| Playback | Engine, seeking, buffering, gapless, loudness normalization | Ported. Seen with local files: FLAC, MP3, AAC, ALAC, Opus, Vorbis, AIFF and WAV play at real time with sound reaching the output chain; pause, resume, seek, next, previous and resume after a relaunch seen (`docs/REPORT.md`). Play/seek seen earlier on YouTube Music guest. Gapless joins and normalization not measured |
| Playback | Output device selection, sample rate, buffer | Ported |
| Playback | OS media controls (SMTC, MPRIS, Now Playing) | Ported |
| Sound | Parametric equalizer, presets, import/export | Ported |
| Sound | Spectrum and analyzer | Ported; the analyzer's levels read through `--debug-level` while playing. The screens that draw them not seen |
| Sources | Local files, tags, embedded art, `.lrc`/`.ttml` | Ported; tags (Latin, Nordic and Japanese titles) and `.lrc` beside the file seen, 3 of 3 found. Embedded art seen for FLAC, MP3 and M4A. A folder dropped on the welcome screen scanned and listed. `.ttml` not seen A file no reader knows as audio (empty, cut off before its header, text or noise with an audio extension) is left out with a line in the log: 6 of 6 made for the check, seen. A file cut short after its header is listed and says "may be damaged or cut short" with the decoder's reason when played, seen. |
| Sources | Local playlists: M3U | Ported; a hand-written `.m3u8` seen imported (relative, absolute and non-Latin paths; an entry not on disk left out) |
| Sources | Local playlists: PLS, XSPF | Not in upstream; built. Hand-written files seen imported, 3 of 3 entries each (relative paths, absolute paths, percent-encoded `file://` URLs) An empty or unreadable file now says it lists no songs, seen with an empty M3U and a garbage XSPF. |
| Sources | CUE sheets: one file shown and played as its tracks | Not in upstream; built. Seen: a 28 s WAV with a two-track sheet lists as two tracks of 13 s and 15 s, which play one after the other and resume after a relaunch. A playlist that names the whole file now takes every track of it (unit test written, not run) |
| Sources | YouTube Music (guest and signed in) | Ported; guest home seen As a guest, seen on Windows: home, quick search and a playlist page, a song playing muted with the queue filling behind it, and the heart on a song and on an album answering "Sign in to YouTube Music to save…" with a button to Settings, Providers and no request sent. An album download started as a guest; whether it finished was not seen. |
| Sources | Spotify (Premium, librespot) | Ported; search/genre lookup rewritten, not yet seen |
| Sources | Tidal, Subsonic/Navidrome, Jellyfin | Ported |
| Sources | Apple Music | Back, played by Apple's own web player in a hidden page of the system web view (`docs/DESIGN.md`, decision 37); upstream's Widevine path is not ported. Windows only. Seen without an account: catalog search, and Apple's previews play, pause, seek, skip and advance at their end. A signed-in account and a whole track not seen. No equalizer, visualizer, normalization, gapless, device choice or download for these tracks |
| Sources | Deezer | Removed (DRM) |
| Sources | Downloaded Spotify helper ("Soloist") | Removed (ran an unverified download) |
| Library | Songs, albums, artists, playlists, favorites, recents | Ported Search and quick search now rank the local shelf beside the streaming one; a song of a files-only library found from both, seen. |
| Queue | Shuffle, repeat, reorder, play next/last | Ported |
| Lyrics | Synced, word-level, romanization, source priority | Ported; third-party proxy off by default |
| Offline | Sealed download cache | Ported |
| Offline | Download from a link via the user's yt-dlp | Not in upstream; built; never run against a real yt-dlp Without yt-dlp the dialog says it was not found and how to get it, with Start off, seen. |
| Accounts | Sign-ins sealed at rest on Windows | Spotify and YouTube now sealed too; YouTube's rotating cookies can be unsealed for up to a minute (`docs/PRIVACY.md`); not yet seen |
| Integrations | Last.fm, ListenBrainz, Maloja scrobbling | Ported |
| Integrations | Discord Rich Presence | Ported, disabled until Sonicly has an application id; its settings page says so in one row, seen |
| Integrations | Shared listening sessions | Ported; needs a user-supplied server, which the settings row now explains on screen |
| App | Themes, theme files, adaptive tint | Ported; theme previews redrawn as frame, page and player bar, seen. Built-in dark and light moved to calm desktop neutrals (decision 38), Home, Songs, Settings, a menu, Now playing and the welcome screen seen in both; theme files not re-captured |
| App | Keyboard shortcuts, quick search, CLI control | Ported; CLI `current`, `pause`, `play`, `seek`, `next`, `prev`, `add`, `queue` and `lyrics` seen against a running Sonicly on Windows; Tab walks buttons, switches and option cards (built, not yet driven from a keyboard); quick search dims instead of blurring Every subcommand run against a files-only profile with right and wrong input, seen; `current` now carries `stream`, read for all eight local formats. |
| App | Tray, single instance, deep links (`sonicly://`) | Ported |
| App | Auto-update | The code that downloaded and ran an installer is deleted. What is left can tell you a newer release exists and link to its page; it is off until Sonicly has releases |
| App | Install counter | Removed |
| App | GitHub star count (title bar, Settings, About) | Not in upstream; built; the count seen in the title bar of a release build on Windows. The About rows, the off switch and the narrow-window form compile but have not been seen |
| App | `settings.json` version and migration | Upstream had atomic saves, a backup and broken-file protection but no version; version and steps built, unit tested, no step exists yet The owner's file from an early build loads in a scratch profile, seen; the two enum values that build wrote under the old name are read through aliases, unit tested. |
| Release | Third-party notices (`THIRD-PARTY.md`) | Generated with cargo-about 0.9.3 for Sonicly's own tree: 14 licences, 866 crates |
| Release | Windows release build, portable zip | Built and launched off-screen on Windows x64; `scripts/windows/package-portable.ps1` |
| Release | Windows installer (Inno Setup) | Compiles with Inno Setup 6 (23.4 MB); never run |
| Release | macOS bundle, Linux packages | Not built. `assets/macos/Info.plist` is Sonicly's; there are no `flatpak/`, `aur/` or `nix/` files in the tree |
| UI | Lyrics page styling (background, typeface, spacing, emphasis, saved looks) | Not in upstream; built, seen on Windows, the settings page again in the merged build |
| UI | Lyrics video background | Windows only (Media Foundation). Hidden on macOS and Linux, which have no decoder in Sonicly yet |
| UI | Floating mini player | Built; seen on Windows in captures, again in the merged build; macOS and Linux not run |
| UI | Drag files onto the window to import | Built; drag-over state and question seen through debug launch flags, a real system drag not yet tried |
| UI | Drawn covers for Favorites, Local Music, Recents | Built; seen in captures |
| UI | Welcome screen with three ways in (this computer, a service, no account) | Not in upstream; first run, the drop sheet over it and the landing in the library seen in captures; the folder picker not opened |
| UI | Quiet coloured covers for albums, artists and songs without art | Not in upstream; built; seen in captures, real embedded art still wins |
