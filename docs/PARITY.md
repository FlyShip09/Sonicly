# Parity with upstream

Status of each feature of the engine Sonicly was ported from. "Ported" means the
code is in the tree and compiles; "seen" means it has been observed working in a
running Sonicly.

| Area | Feature | Status |
| --- | --- | --- |
| Playback | Engine, seeking, buffering, gapless, loudness normalization | Ported; play/seek seen (YouTube Music guest) |
| Playback | Output device selection, sample rate, buffer | Ported |
| Playback | OS media controls (SMTC, MPRIS, Now Playing) | Ported |
| Sound | Parametric equalizer, presets, import/export | Ported |
| Sound | Spectrum and analyzer | Ported |
| Sources | Local files, tags, embedded art, `.lrc`/`.ttml` | Ported |
| Sources | Local playlists: M3U | Ported |
| Sources | Local playlists: PLS, XSPF | Not in upstream; built, imported like M3U; not yet seen |
| Sources | CUE sheets: one file shown and played as its tracks | Not in upstream; built; not yet seen or heard |
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
| App | Keyboard shortcuts, quick search, CLI control | Ported |
| App | Tray, single instance, deep links (`sonicly://`) | Ported |
| App | Auto-update | Disabled (no releases yet) |
| App | Install counter | Removed |
| App | GitHub star count (title bar, Settings, About) | Not in upstream; built; the count seen in the title bar of a release build on Windows. The About rows, the off switch and the narrow-window form compile but have not been seen |
| App | `settings.json` version and migration | Upstream had atomic saves, a backup and broken-file protection but no version; version and steps built, unit tested, no step exists yet |
| Release | Third-party notices (`THIRD-PARTY.md`) | Generated with cargo-about 0.9.3 for Sonicly's own tree: 14 licences, 866 crates |
| Release | Windows release build, portable zip | Built and launched off-screen on Windows x64; `scripts/windows/package-portable.ps1` |
| Release | Windows installer (Inno Setup) | Script reviewed for Sonicly's name, id, scheme and licence path; never compiled, Inno Setup is not installed |
| Release | macOS bundle, Linux packages | Not built. `assets/macos/Info.plist` is Sonicly's; there are no `flatpak/`, `aur/` or `nix/` files in the tree |
| UI | Lyrics page styling (background, typeface, spacing, emphasis, saved looks) | Not in upstream; built, seen on Windows |
| UI | Lyrics video background | Windows only (Media Foundation). Hidden on macOS and Linux, which have no decoder in Sonicly yet |
| UI | Floating mini player | Built; seen on Windows in captures, macOS and Linux not run |
| UI | Drag files onto the window to import | Built; drag-over state and question seen through debug launch flags, a real system drag not yet tried |
| UI | Drawn covers for Favorites, Local Music, Recents | Built; seen in captures |
