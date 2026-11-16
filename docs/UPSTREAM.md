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
| Apple Music's playback (`music/src/apple/playback.rs`, `progressive.rs`, `stream.rs`), its live test, its play-activity beacon and its loudness lookup | they fetch protected streams, ask for a Widevine licence and decrypt, which Sonicly does not do. The rest of the provider was deleted with them and later ported back; see "Ported back" |
| Widevine surface: `music/src/drm.rs`, `state::Drm`, the prompt, the settings row, its confirm dialog, `MusicProvider::protected`, `Session::wants_drm` | no DRM code in the tree; upstream's `widevine` crate was never imported |
| Deezer provider (`music/src/deezer`), its example, live tests, icon, cookie walkthrough, the `blowfish` and `cbc` dependencies | not shipped |
| Spotify Soloist: `music/src/soloist`, `state::Soloist`, the setup dialog, settings rows and notes, the engine hooks only it used (`PlaybackEvent::Queue*`, `QueueAppend`, `Player::append_to_queue`, `Player::can_order_queue`, `PlaybackFactory::qualities`, `MusicProvider::soloist`; `TrackSignal::direct` went too and is back with another meaning, see "Ported back") and the shared-queue sync in `state/src/playback.rs` and `queue.rs` | it downloaded a program from a CDN and ran it with no signature or hash check. Spotify plays through librespot |
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

## Ported back

Apple Music returned after the first port (`docs/DESIGN.md`, decision 37), from the same Sonora
commit.

| What | From | Notes |
| --- | --- | --- |
| `music/src/apple/{auth,client,wire,links,recommend}.rs`, the icon, the cookie walkthrough, the `Subscription` sign-in problem | the same files upstream | Sonicly's names. Left out: the play-activity beacon (Apple's page reports plays itself), the loudness lookup (nothing of Sonicly's normalizes these tracks), `http()` and `user_token()` (only the Widevine path read them). Added: a client with no account for the debug-only visitor session |
| `music/src/apple/player.rs`, `player.js`, `music/src/webplayer.rs`, `state/src/webplayer.rs`, the player pages of `crates/webview` | not in upstream | Sonicly's own. Playback happens in Apple's web player; upstream's `playback.rs`, `progressive.rs`, `stream.rs`, `drm.rs` and the `widevine` crate were not ported, copied or re-created |
| `TrackSignal::direct`, `PlaybackEvent::Barred`, `PlaybackFactory::barrier` | not in upstream in this meaning | a player Sonicly commands but does not feed says so, and says when it cannot play at all |

## Hardened

| Where | Change | Why |
| --- | --- | --- |
| `music/src/spotify/pathfinder/hashes.rs` | GraphQL operation hashes are read from the web player's own scripts: the page on `open.spotify.com`, the main bundle on `open.spotifycdn.com`, and for an operation the bundle leaves out, the lazily loaded chunks its chunk map names. Redirects off those two hosts are refused, at most 8 chunks of 2 MiB are read, and a hash must be 64 hex digits | upstream asked a personal third-party server for them |
| `music/src/credentials.rs` | on Windows a stored sign-in is sealed with DPAPI for the current user (`sonicly-dpapi-1` prefix); a plaintext file is read once and written back sealed | the files were plaintext |
| `music/src/spotify/auth.rs` | librespot keeps the Spotify credential in memory only (`Cache::with_memory_credentials`); Sonicly reads and writes `credentials.json` itself, through `credentials.rs` | librespot wrote it as plain JSON |
| `music/src/youtube/mod.rs` | YouTube's rotating `cookies.json` is sealed through `credentials.rs`. The `ytmusic` crate can only read and write a plain file, so it is given `cookies.live.json`: laid out for the moment a client loads it, and sealed and deleted after every rotation and at least once a minute (`docs/DESIGN.md`, decision 16) | the crate wrote it as plain JSON |
| `music/src/subsonic/auth.rs`, `jellyfin/auth.rs` | an address with no scheme is taken as `https://`; a typed `http://` is kept | a password should not cross the network in the clear by default |
| `music/src/binimum`, `state/src/settings.rs` | the lyrics source upstream labelled "Apple Music" is named Binimum, in its id and its credit, and starts switched off | it is a third-party proxy |
| `music/src/{artwork,binimum,kugou,lrclib,netease}` | the user agent is `sonicly/<version>` with no repository url | not upstream's client |
| `crates/sonicly`, installer, desktop entry, `Info.plist` | app id `io.github.flyship09.sonicly`, scheme `sonicly://` | a separate application |

## Left as it is

- The `ytmusic` crate writes YouTube's cookies to a plain file whenever Google sets one, and
  has no hook to do otherwise. Sonicly seals and deletes that file within a minute, so the
  cookies can sit unsealed on disk for up to that long, or until the next launch if Sonicly is
  closed inside that minute. Closing the gap needs a change to the crate.
- A self-hosted server on plain http needs `http://` typed in front of its address.
- YouTube playback loads a hidden YouTube page to satisfy its bot check, and Tidal sign-in uses
  client ids taken from Tidal's own apps (`docs/DESIGN.md`, decision 12).
- The `stream::Body` hooks for prepared or confidential bytes have no provider using them now
  (`Catalog::Editable` is Apple Music's again). They are part of the engine's model and were kept.
- `THIRD-PARTY.md` is generated by `scripts/generate-notices.py` and is not in the repository.
