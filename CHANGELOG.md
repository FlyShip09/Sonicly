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
- Home opens with the song you were listening to, where you left it, and a Recently played shelf from your history.
- Resting on an artist's or album's name shows a small card with its cover, a line of facts from the service and a play button.
- The player bar and Now playing show what really plays, such as "FLAC 44.1 kHz / 16 bit" or "AAC 256 kbps", with every detail in its tooltip.
- `sonicly --json current` describes the stream that plays under `stream`: codec, sample rate, bit depth of a lossless file, channels and bit rate of a lossy one, each only when the decoder, the file or the service reported it.

### Changed

- The window has Sonicly's own look: a neutral frame on the system material, one type scale, covers with an edge, and album, playlist and song pages washed with the cover's colour.
- A calmer desktop look: soft charcoal and white pages, a full-height sidebar whose highlight glides between rows, the page running up under the title bar, bold page titles, 13 px text by default, grouped settings, inset list rows and a player bar that floats above the page.
- Small motion that answers what you do: a liked heart pops, a card's play button rises under the pointer, segmented controls slide, pages come in from the side you moved towards, and a new song's title rises into the player bar. Reduce motion turns all of it off.
- The seek bar thickens under the pointer, long titles in the player bar scroll to their end when you rest on them, songs removed from the queue close their gap, and a scrolled album, playlist or artist page shows its name in the title bar above a hairline.
- Quick search groups its answers under Songs, Artists, Albums, Playlists and Commands, and pinned sidebar entries are slim rows like the rest of the sidebar.
- In a wide window, album and playlist pages keep the cover, title and buttons in a column beside the songs.
- Artist pages list albums and singles and EPs on their own shelves and leave out sections the artist has nothing for.
- Shelf arrows page by whole cards, the queue sidebar slides in as it opens, song counts run to their new value, the library scan shows a progress ring, and removals in menus read in red with their shortcut beside them.
- An empty local library and a page that failed to load say what to do next.
- `settings.json` now records its version, so a later Sonicly can rename a setting without losing yours. A file written by a newer Sonicly is copied to `settings.v<version>.json` before an older one saves over it.
- The licence text is in `LICENSE` (it was `COPYING`).

### Fixed

- Opening the song Sonicly was restoring from the last run no longer leaves it paused at 0:00.
- Commands sent from a terminal reach the running Sonicly on Windows.
- A playlist that names a file split by a cue sheet imports every track of it.
- Browsing a service without an account, the heart, "Add to library" and "New playlist" no longer send a request the service refuses. They say "Sign in to YouTube Music to save songs" with a Sign in button that opens Settings, Providers. Favorites and playlists of your own files work as before.
- Search and quick search find your own files, with or without a service signed in. They found nothing in a library of files alone.
- A failure says why it happened and what to do: the reason is on a second line of the toast, and a song that will not play says whether its file is damaged or the service turned it down.
- A file that is not audio, such as an empty or cut-off download or a text file named `.mp3`, is left out of Local Music instead of listed as a song that never plays. Files already listed go after "Rescan".
- Importing an empty or unreadable playlist file says so, where it used to say none of its songs were in your library.
- A settings file from an early build that named this device "sonora" on Spotify Connect loads again.
- Messages Windows sends the window while Sonicly talks to the taskbar or the tray are no longer dropped (logged as `RefCell already borrowed`).
- `sonicly provider` and `sonicly providers` no longer call a signed-out service current, `sonicly queue` shows the playing song's length, and `sonicly session start` or `join` says when no Sessions server is set.
- Home no longer tells a guest with no files of their own to like songs for Quick picks.

### Security

- Sonicly can no longer download and run an installer to update itself. That code is deleted; an update notice, once there are releases, only links to the release page.
- Spotify and YouTube Music sign-ins are now sealed on disk like the others; see `docs/PRIVACY.md` for the one-minute gap that remains for YouTube.
