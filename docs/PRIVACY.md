# Privacy

What Sonicly contacts and what it keeps on your computer. It has no telemetry,
no update pings, no account of its own, and no server of its own. The one
request it makes without being asked reads a public number from GitHub, and it
can be switched off; see "The star count" below. This page is
written from the source and kept by hand; `docs/UPSTREAM.md` lists what was
removed from the code Sonicly started from.

## What Sonicly contacts

Only services you use, and only while you use them.

| When | Who is contacted | What is sent |
| --- | --- | --- |
| You play or browse a streaming service | That service: YouTube Music, Spotify, Tidal, or your own Subsonic, Navidrome or Jellyfin server | Your sign-in and what you ask for, as that service's own apps send it |
| Lyrics are looked up | The lyrics sources switched on under Settings, Playback: LRCLIB, Musixmatch, NetEase, Kugou, and the third-party proxy Binimum, which is off until you turn it on | The title, artist, album and length of the track |
| Discord status shows a local file, once Discord Rich Presence exists (it is disabled today) | Deezer, iTunes search, MusicBrainz and the Cover Art Archive, to find a public cover | The artist and album or title of the track |
| YouTube plays | A hidden YouTube page inside Sonicly, which YouTube's bot check needs, and SponsorBlock when you switch it on | The id of the video |
| Scrobbling is set up | Last.fm, Libre.fm, ListenBrainz, or your Maloja server | What you played and when |
| You join a shared listening session | The session server whose address you typed | What the room is playing |
| You download from a link | See below | |
| Sonicly starts and "Show the star count" is on, at most once a day | GitHub, at `api.github.com` | See below |

Local files are never uploaded. Looking up lyrics for them, which sends their
title and artist to the lyrics sources, can be switched off under Settings.

## The star count

The star button in the title bar and under Settings, About shows how many stars
Sonicly's repository has. To know that number Sonicly asks GitHub.

- **What is sent.** One request, `GET https://api.github.com/repos/FlyShip09/Sonicly`,
  with no sign-in, no cookie and no identifier. Its only description of the
  sender is the user agent `sonicly/<version>`. GitHub sees the IP address the
  request comes from, as any site you connect to does.
- **When.** At startup, and only if the last request was at least 24 hours ago.
  The answer and the time of the request are kept in `cache.sqlite`. A request
  that failed counts too, so a network that keeps failing is tried once a day,
  not over and over. Nothing is asked in offline mode or while the connection
  is known to be gone.
- **How to turn it off.** Settings, About, "Show the star count". With it off
  no request is made and the button stays, without a number. Pressing the
  button opens the repository in your browser, which is your browser's
  business, not Sonicly's.

## Downloads from links

"Download from a link" runs **your own copy of yt-dlp**. Sonicly does not
contain yt-dlp and never downloads, installs or updates it: you install it
(`winget install yt-dlp` on Windows, or from
<https://github.com/yt-dlp/yt-dlp/releases>), and Sonicly finds it on `PATH` or
at the path you choose under Settings, Storage.

- Sonicly itself contacts nobody for a download. yt-dlp contacts the site the
  link points at, from your computer, the same as if you ran it in a terminal.
  The link is sent nowhere else.
- Only `http://` and `https://` links are accepted. yt-dlp is started directly,
  with a fixed list of options and no shell. Your yt-dlp configuration files
  are not read (`--ignore-config`), so no setting of yours can run a program
  afterwards or move the file.
- It asks for the audio as the site serves it. Nothing is converted, and
  ffmpeg is not used.
- The file is saved in the music folder you pick. Sonicly then writes the
  title, artist, album and cover that yt-dlp reported into the file's tags and
  rescans the folder. A thumbnail yt-dlp saved for the cover is deleted once it
  is in the file.
- Stopping a download ends yt-dlp and deletes the unfinished file.
- DRM-protected media is not downloaded. yt-dlp refuses it and Sonicly adds
  nothing that would get around that.

Sonicly keeps no list of the links you downloaded. What you may download is
between you, the site and the law where you live.

## What Sonicly stores

Everything stays on your computer, under your user profile.

| What | Where | Notes |
| --- | --- | --- |
| Sign-ins | The `accounts` folder in Sonicly's data folder | See below |
| Settings | `settings.json` in Sonicly's config folder | Plain text you can edit. Holds the path to yt-dlp if you set one |
| Favorites, playlists of local files, play history | `state.sqlite` in the data folder | Never sent anywhere, except plays you chose to scrobble |
| The scan of your music folders, cached pages, lyrics | `cache.sqlite` and the cache folder | Safe to delete; Sonicly reads it all again |
| Tracks kept after playing, and downloads from streaming services | The cache and downloads folders | Limits and a way to clear them are under Settings, Storage |
| Logs | The log file shown under Settings, About | Stays on your computer |

### Sign-ins

On Windows every stored sign-in is sealed with DPAPI under your Windows
account's key, so another user of the computer, or someone who copies the file
elsewhere, cannot read it. On macOS and Linux the files are readable by your
user alone (mode 600). A sign-in file an older build left in plain text is
sealed the first time it is read.

This covers Tidal, Subsonic, Jellyfin, YouTube Music and Spotify:

- **Spotify.** The library Sonicly uses for Spotify (librespot) would write the
  credential as plain JSON. Sonicly has it keep the credential in memory only
  and stores it itself, sealed.
- **YouTube Music.** Google changes the session cookies every few minutes, and
  the library Sonicly uses for YouTube (`ytmusic`) can only save them to a
  plain file of its own. Sonicly keeps the lasting copy sealed
  (`cookies.json`) and gives the library a working file
  (`cookies.live.json`) instead. That file is deleted as soon as the library
  has loaded it, and whenever the library writes it again Sonicly seals the
  contents and deletes it: right after each change of cookies Sonicly asked
  for, and otherwise within a minute.

  **Known gap:** between such a write and that sweep, the cookies are on disk
  unsealed, in your own profile folder, for up to a minute, or until the next
  launch if Sonicly closes or crashes inside that minute. Removing the gap
  needs a change to the `ytmusic` library itself, which Sonicly does not carry.

Signing out deletes the sign-in file and, for YouTube, both cookie files.
