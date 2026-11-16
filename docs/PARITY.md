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
| Sources | Local files, tags, embedded art, `.lrc`/`.ttml` | Ported; tags (Latin, Nordic and Japanese titles) and `.lrc` beside the file seen, 3 of 3 found. Embedded art and `.ttml` not seen: the test files carry neither |
| Sources | Local playlists: M3U | Ported; a hand-written `.m3u8` seen imported (relative, absolute and non-Latin paths; an entry not on disk left out) |
| Sources | Local playlists: PLS, XSPF | Not in upstream; built. Hand-written files seen imported, 3 of 3 entries each (relative paths, absolute paths, percent-encoded `file://` URLs) |
| Sources | CUE sheets: one file shown and played as its tracks | Not in upstream; built. Seen: a 28 s WAV with a two-track sheet lists as two tracks of 13 s and 15 s, which play one after the other and resume after a relaunch. A playlist that names the whole file takes one of its tracks only |
| Sources | YouTube Music (guest and signed in) | Ported; guest home seen |
| Sources | Spotify (Premium, librespot) | Ported; search/genre lookup rewritten, not yet seen |
| Sources | Tidal, Subsonic/Navidrome, Jellyfin | Ported |
| Sources | Apple Music, Deezer | Removed (DRM) |
| Sources | Downloaded Spotify helper ("Soloist") | Removed (ran an unverified download) |
| Library | Songs, albums, artists, playlists, favorites, recents | Ported |
| Queue | Shuffle, repeat, reorder, play next/last | Ported |
| Lyrics | Synced, word-level, romanization, source priority | Ported; third-party proxy off by default |
| Offline | Sealed download cache | Ported |
| Offline | Download from a link via the user's yt-dlp | Not in upstream; built; never run against a real yt-dlp |
| Accounts | Sign-ins sealed at rest on Windows | Spotify and YouTube now sealed too; YouTube's rotating cookies can be unsealed for up to a minute (`docs/PRIVACY.md`); not yet seen |
| Integrations | Last.fm, ListenBrainz, Maloja scrobbling | Ported |
| Integrations | Discord Rich Presence | Ported, disabled until Sonicly has an application id |
| Integrations | Shared listening sessions | Ported; needs a user-supplied server |
| App | Themes, theme files, adaptive tint | Ported |
| App | Keyboard shortcuts, quick search, CLI control | Ported; CLI `current`, `pause`, `play`, `seek`, `next`, `prev`, `add`, `queue` and `lyrics` seen against a running Sonicly on Windows |
| App | Tray, single instance, deep links (`sonicly://`) | Ported |
| App | Auto-update | Disabled (no releases yet) |
| App | Install counter | Removed |
| UI | Lyrics page styling (background, typeface, spacing, emphasis, saved looks) | Not in upstream; built, seen on Windows, the settings page again in the merged build |
| UI | Lyrics video background | Windows only (Media Foundation). Hidden on macOS and Linux, which have no decoder in Sonicly yet |
| UI | Floating mini player | Built; seen on Windows in captures, again in the merged build; macOS and Linux not run |
| UI | Drag files onto the window to import | Built; drag-over state and question seen through debug launch flags, a real system drag not yet tried |
| UI | Drawn covers for Favorites, Local Music, Recents | Built; seen in captures |
