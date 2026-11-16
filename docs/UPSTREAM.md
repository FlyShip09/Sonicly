# Provenance

Sonicly's engine was ported from Sonora, <https://github.com/sonorahq/sonora>, GPL-3.0-or-later.
It is Sonicly's own code now (`docs/DESIGN.md`, decision 13): nothing is kept in sync with
upstream, and this file records where the code came from and what was changed on the way in.

## Starting point

The port started from Sonora commit `6e3cfb014c5e66e9b5d7a10f0cbcce9e048b2b71`.
`reference/sonora` is a read-only, git-ignored clone at that commit, kept to compare behaviour
against. Nothing is built or run there.

| Sonora crate | Sonicly crate |
| --- | --- |
| `crates/music` | `crates/music` |
| `crates/state` | `crates/state` |
| `crates/storage` | `crates/storage` |
| `crates/webview` | `crates/webview` |
| `crates/i18n` | `crates/i18n` |
| `crates/embed` | `crates/embed` |
| `crates/sonora` | `crates/sonicly` |
| `crates/views`, `ui`, `router`, `input`, `icons`, `cli` | `crates/<same>` |
| `crates/widevine` | never imported |

The first six sat under `vendor/` until the ownership pass moved them with `git mv`, so their
history follows. Identifiers, comments, tests and environment variables that said Sonora say
Sonicly (`state::Sonicly`, `SONICLY_*`). The `gpui`, `ytmusic` and `librespot` git dependencies in
the root `Cargo.toml` still point at the `sonorahq` forks, pinned by revision.

## Deleted

| What | Why |
| --- | --- |
| Apple Music provider (`music/src/apple`), its live test, icon, cookie walkthrough, the `Subscription` sign-in problem | its streams are DRM-protected and need a Widevine module, which Sonicly does not ship |
| Widevine surface: `music/src/drm.rs`, `state::Drm`, the prompt, the settings row, its confirm dialog, `MusicProvider::protected`, `Session::wants_drm` | no DRM code in the tree; upstream's `widevine` crate was never imported |
| Deezer provider (`music/src/deezer`), its example, live tests, icon, cookie walkthrough, the `blowfish` and `cbc` dependencies | not shipped |
| Spotify Soloist: `music/src/soloist`, `state::Soloist`, the setup dialog, settings rows and notes, the engine hooks only it used (`PlaybackEvent::Queue*`, `QueueAppend`, `Player::append_to_queue`, `Player::can_order_queue`, `PlaybackFactory::qualities`, `TrackSignal::direct`, `MusicProvider::soloist`) and the shared-queue sync in `state/src/playback.rs` and `queue.rs` | it downloaded a program from a CDN and ran it with no signature or hash check. Spotify plays through librespot |
| The parts of `music/src/stream.rs` only those providers used (`WeakStream`, `Stream::new`, `Stream::open`, `Spool::view`, `Spool::write_at`) | dead once they were gone |
| Install counter (`state/src/usage.rs`) and the consent checkbox | it posted to upstream's server |
| Official Sessions server address, the `sessions_official` setting and its row | Sonicly has no server. A session needs `SONICLY_SESSION_SERVER` or an address in Settings |
| Upstream's Discord application id, the project button and its `discord_sonora_button` setting | `DISCORD_APPLICATION_ID` is `None` until Sonicly has an application; presence stays off |
| Cover upload to an image host (`artwork::upload`, `discord_upload_covers`) | no cover from disk or a private server leaves the machine |
| The third-party Spotify hash service | see "Hardened" |
| Links to upstream's site: the playback keys FAQ, community themes, "What to install", the team list | not Sonicly's pages |
| Update check against upstream's releases | `RELEASE_REPO` is `None`, so nothing is requested and the rows are hidden |
| Fluent strings of everything above, in every locale (106 keys) | unused |
| `scripts/rebrand.py`, `vendor/SONORA_REV` | the rename is done and the commit is recorded here |

## Hardened

| Where | Change | Why |
| --- | --- | --- |
| `music/src/spotify/pathfinder/hashes.rs` | GraphQL operation hashes are read from the web player's own scripts: the page on `open.spotify.com`, the main bundle on `open.spotifycdn.com`, and for an operation the bundle leaves out, the lazily loaded chunks its chunk map names. Redirects off those two hosts are refused, at most 8 chunks of 2 MiB are read, and a hash must be 64 hex digits | upstream asked a personal third-party server for them |
| `music/src/credentials.rs` | on Windows a stored sign-in is sealed with DPAPI for the current user (`sonicly-dpapi-1` prefix); a plaintext file is read once and written back sealed | the files were plaintext |
| `music/src/subsonic/auth.rs`, `jellyfin/auth.rs` | an address with no scheme is taken as `https://`; a typed `http://` is kept | a password should not cross the network in the clear by default |
| `music/src/binimum`, `state/src/settings.rs` | the lyrics source upstream labelled "Apple Music" is named Binimum, in its id and its credit, and starts switched off | it is a third-party proxy |
| `music/src/{artwork,binimum,kugou,lrclib,netease}` | the user agent is `sonicly/<version>` with no repository url | not upstream's client |
| `crates/sonicly`, installer, desktop entry, `Info.plist` | app id `io.github.flyship09.sonicly`, scheme `sonicly://` | a separate application |

## Left as it is

- Spotify's `credentials.json` is written by librespot and YouTube's `cookies.json` by the
  `ytmusic` crate, so neither is sealed on Windows.
- A self-hosted server on plain http needs `http://` typed in front of its address.
- YouTube playback loads a hidden YouTube page to satisfy its bot check, and Tidal sign-in uses
  client ids taken from Tidal's own apps (`docs/DESIGN.md`, decision 12).
- `Catalog::Editable` and the `stream::Body` hooks for prepared or confidential bytes have no
  provider using them now. They are part of the engine's model and were kept.
- `THIRD-PARTY.md` is generated by `scripts/generate-notices.py` and is not in the repository.
