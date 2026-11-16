# Changelog

Follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Each entry is
one line about what someone using Sonicly can now do.

## [Unreleased]

### Added

- Import PLS and XSPF playlists into Local Music, the same way as M3U.
- An album kept as one file with a cue sheet beside it shows and plays as its separate tracks.
- Download audio from a link with your own yt-dlp, into a music folder of your choice (Ctrl+Shift+D, the link button on Local Music, or paste a link into quick search).
- Style the lyrics in Now playing: a Waves, Fluid, cover, picture or video background (video on Windows), typeface, weight, spacing, alignment and emphasis, and keep a style as a named look.
- A floating mini player that stays on top of other windows (Ctrl+Shift+M, or "Open the mini player" in quick search).
- Drag audio files or folders onto the window to add them to Local Music, or onto a playlist in the sidebar.
- Favorites, Local Music and Recents have covers drawn for them.
- Open Home, Songs, Albums, Artists and Playlists with Ctrl+1 to Ctrl+5, and show or hide the sidebar with Ctrl+Alt+S.

### Changed

- The window has Sonicly's own look: a neutral frame on the system material, one type scale, covers with an edge, and album, playlist and song pages washed with the cover's colour.
- An empty local library and a page that failed to load say what to do next.

### Fixed

- Opening the song Sonicly was restoring from the last run no longer leaves it paused at 0:00.
- Commands sent from a terminal reach the running Sonicly on Windows.

### Security

- Spotify and YouTube Music sign-ins are now sealed on disk like the others; see `docs/PRIVACY.md` for the one-minute gap that remains for YouTube.
