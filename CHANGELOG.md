# Changelog

Follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Each entry is
one line about what someone using Sonicly can now do.

## [Unreleased]

### Added

- Import PLS and XSPF playlists into Local Music, the same way as M3U.
- An album kept as one file with a cue sheet beside it shows and plays as its separate tracks.
- Download audio from a link with your own yt-dlp, into a music folder of your choice (Ctrl+Shift+D, the link button on Local Music, or paste a link into quick search).
- A star button in the title bar and under Settings, About opens Sonicly's repository and shows how many stars it has. The count is read from GitHub at most once a day; "Show the star count" under Settings, About turns that off.
- Settings, About opens the licences of everything in the build, from the `THIRD-PARTY.md` shipped beside the program.

### Changed

- `settings.json` now records its version, so a later Sonicly can rename a setting without losing yours. A file written by a newer Sonicly is copied to `settings.v<version>.json` before an older one saves over it.
- The licence text is in `LICENSE` (it was `COPYING`).

### Security

- Spotify and YouTube Music sign-ins are now sealed on disk like the others; see `docs/PRIVACY.md` for the one-minute gap that remains for YouTube.
