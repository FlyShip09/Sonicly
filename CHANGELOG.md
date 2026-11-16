# Changelog

Follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Each entry is
one line about what someone using Sonicly can now do.

## [Unreleased]

### Added

- Apple Music on Windows, for subscribers: sign in on Apple's page inside Sonicly, browse your library and the catalog, and play. Songs play in Apple's own web player, hidden, so the equalizer, visualizer, loudness normalization, output device choice and downloads do not apply to them.
- Import PLS and XSPF playlists into Local Music, the same way as M3U.
- An album kept as one file with a cue sheet beside it shows and plays as its separate tracks.
- Download audio from a link with your own yt-dlp, into a music folder of your choice (Ctrl+Shift+D, the link button on Local Music, or paste a link into quick search).
- Style the lyrics in Now playing: a Waves, Fluid, cover, picture or video background (video on Windows), typeface, weight, spacing, alignment and emphasis, and keep a style as a named look.
- A floating mini player that stays on top of other windows (Ctrl+Shift+M, or "Open the mini player" in quick search).
- Drag audio files or folders onto the window to add them to Local Music, or onto a playlist in the sidebar.
- Favorites, Local Music and Recents have covers drawn for them.
- Open Home, Songs, Albums, Artists and Playlists with Ctrl+1 to Ctrl+5, and show or hide the sidebar with Ctrl+Alt+S.
- A star button in the title bar and under Settings, About opens Sonicly's repository and shows how many stars it has. The count is read from GitHub at most once a day; "Show the star count" under Settings, About turns that off.
- Settings, About opens the licences of everything in the build, from the `THIRD-PARTY.md` shipped beside the program.

### Changed

- The window has Sonicly's own look: a neutral frame on the system material, one type scale, covers with an edge, and album, playlist and song pages washed with the cover's colour.
- A calmer desktop look: soft charcoal and white pages, a full-height sidebar whose highlight glides between rows, the page running up under the title bar, bold page titles, 13 px text by default, grouped settings, inset list rows and a player bar that floats above the page.
- Small motion that answers what you do: a liked heart pops, a card's play button rises under the pointer, segmented controls slide, pages come in from the side you moved towards, and a new song's title rises into the player bar. Reduce motion turns all of it off.
- The seek bar thickens under the pointer, long titles in the player bar scroll to their end when you rest on them, songs removed from the queue close their gap, and a scrolled album, playlist or artist page shows its name in the title bar above a hairline.
- Quick search groups its answers under Songs, Artists, Albums, Playlists and Commands, and pinned sidebar entries are slim rows like the rest of the sidebar.
- An empty local library and a page that failed to load say what to do next.
- `settings.json` now records its version, so a later Sonicly can rename a setting without losing yours. A file written by a newer Sonicly is copied to `settings.v<version>.json` before an older one saves over it.
- The licence text is in `LICENSE` (it was `COPYING`).

### Fixed

- Opening the song Sonicly was restoring from the last run no longer leaves it paused at 0:00.
- Commands sent from a terminal reach the running Sonicly on Windows.
- A playlist that names a file split by a cue sheet imports every track of it.

### Security

- Sonicly can no longer download and run an installer to update itself. That code is deleted; an update notice, once there are releases, only links to the release page.
- Spotify and YouTube Music sign-ins are now sealed on disk like the others; see `docs/PRIVACY.md` for the one-minute gap that remains for YouTube.
