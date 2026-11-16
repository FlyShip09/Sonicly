# Upstream

Sonicly is built on Sonora, <https://github.com/sonorahq/sonora>, GPL-3.0-or-later.

## Pinned commit

`vendor/SONORA_REV` holds the commit the import was taken from:
`6e3cfb014c5e66e9b5d7a10f0cbcce9e048b2b71`. `reference/sonora` is a read-only, git-ignored
clone at that commit, kept for comparison. Nothing is built or run there.

## Import layout

| Upstream | Here | Policy |
| --- | --- | --- |
| `crates/music` | `vendor/music` | vendored, hand changes listed below |
| `crates/state` | `vendor/state` | vendored, hand changes listed below |
| `crates/storage` | `vendor/storage` | vendored, unchanged by hand |
| `crates/webview` | `vendor/webview` | vendored, unchanged by hand |
| `crates/i18n` | `vendor/i18n` | vendored, unchanged by hand |
| `crates/embed` | `vendor/embed` | vendored, unchanged by hand |
| `crates/sonora` | `crates/sonicly` | Sonicly's own, edited freely |
| `crates/views`, `ui`, `router`, `input`, `icons`, `cli` | `crates/<same>` | Sonicly's own, edited freely |
| `crates/widevine` | not imported | see "Compiled out" |

The `gpui`, `ytmusic` and `librespot` git dependencies in the root `Cargo.toml` still point at
the `sonorahq` forks, pinned by revision.

## Rebrand script

After merging upstream changes into `vendor/` or porting files into `crates/`, run from the
repository root:

    python scripts/rebrand.py

It renames Sonora to Sonicly inside Rust string literals and in text assets. Identifiers such as
`state::Sonora` are left alone so `vendor/` keeps merging. Lines that mention `sonorahq`,
`nolight` or `sonora-stats` are skipped on purpose and are handled by hand, as listed here. Then
check that nothing new points at upstream's hosts:

    grep -rIn -i "sonorahq\|nolight\|sonora-stats" crates vendor assets scripts

## Compiled out

- **Apple Music**: `vendor/music` feature `apple-music`, off by default and enabled nowhere. It
  **cannot build in this repository**: the provider needs upstream's `widevine` crate, which is
  not imported and must not be.
- **Deezer**: `vendor/music` feature `deezer`, off by default and enabled nowhere. It owns the
  optional `blowfish` and `cbc` dependencies. `cargo check -p music --features deezer` passes.
- **Spotify Soloist**: `vendor/music` feature `soloist`, off by default and enabled nowhere. See
  "Security hardening".
- **Widevine**: `vendor/music/src/drm.rs` is a stub that reports no support. `state::Drm` and
  the settings row and prompt in `views` are still compiled and hide themselves, because
  `Drm::shown` and the `SignedIn` handler both ask `drm::supported()` first.

## Hand-made changes under `vendor/`

Most are marked `Sonicly:` in a comment beside the change. This table is the full list.

### Providers and DRM

| File | Change | Reason |
| --- | --- | --- |
| `music/Cargo.toml` | features `apple-music`, `deezer`, `soloist` added, none default; `widevine` feature and dependency removed; `blowfish` and `cbc` optional under `deezer`; `deezer-probe` example gets `required-features = ["deezer"]`; `windows` gains `Win32_Security_Cryptography` | compile the two providers and Soloist out, drop the widevine crate, DPAPI |
| `music/src/lib.rs` | `apple` and `deezer` modules behind their features; `soloist` behind its feature with `soloist_off.rs` in its place otherwise | as above |
| `music/src/drm.rs` | re-export of the `widevine` crate replaced by a self-contained stub with the same names (`CDM_PATH`, `Found`, `Offer`, `Origin`, `available`, `fetch`, `find`, `host`, `hosted`, `offer`, `supported`, `uninstall`); it searches, downloads and loads nothing | no widevine crate, no DRM code |
| `music/src/stream.rs` | `allow(dead_code)` unless both provider features are on | the parts only Apple Music and Deezer use are otherwise reported dead |
| `music/src/live_tests/release_type.rs`, `playlist_cover.rs` | the Apple and Deezer tests and their imports behind the features | they name gated modules |

### Sonora's own infrastructure

| File | Change | Reason |
| --- | --- | --- |
| `state/src/usage.rs` | deleted | the install counter posted to `sonora-stats.nolight.dev`; nothing is sent any more |
| `state/src/lib.rs` | `mod usage`, `pub use usage::Usage`, the `Sonora::usage` field and its construction removed; `RELEASE_REPO` and `DISCORD_APPLICATION_ID` exported | follows from the above, and lets `views` hide what is switched off |
| `state/src/updates/mod.rs` | `RELEASES` url replaced by `pub const RELEASE_REPO: Option<&str> = None`; `Updates::look` returns before any request while it is `None`; `updates_itself()` is false while it is `None`; the url is built from the slug; user agent is `sonicly/<version>` | Sonicly has no release repository yet. Set `RELEASE_REPO` to `Some("owner/name")` to switch update checks on |
| `state/src/settings.rs` | `check_updates` defaults to false while `RELEASE_REPO` is `None` | the setting defaults off |
| `state/src/listening/server.rs` | `OFFICIAL` (`wss://sessions.sonorahq.org/ws`) removed | no official Sessions server |
| `state/src/listening/mod.rs` | `server()` returns `Option`: the `SONICLY_SESSION_SERVER` variable, else the saved address, else none; `connect` shows `toast-sessions-server-required` and opens no link without one | nothing connects unless the user configured a server. The stored `sessions_official` setting is no longer read |
| `state/src/discord/mod.rs` | `APPLICATION_ID` is `pub const ...: Option<&str> = None`; `attach` builds nothing and `run` returns while it is `None`; `SONORA_URL` and the "Get Sonicly" button removed | Sonicly has no Discord application yet, and no link to sonorahq.org. The stored `discord_sonora_button` setting is no longer read |
| `state/src/playback.rs` | `KEYS_FAQ` (sonorahq.org) and the toast button that opened it removed | the toast keeps its message and note |
| `music/src/{artwork,binimum/mod,kugou/mod,lrclib/mod,netease/mod}.rs` | user agent is `sonicly/<version>`, without Sonora's repository url | not Sonora's client |

### Security hardening

| File | Change | Reason |
| --- | --- | --- |
| `music/src/soloist_off.rs` (new) | an inert stand-in for the `soloist` module, used while the `soloist` feature is off: same public names, `supported()` false, no file, process or connection | upstream's module downloads a program from a CDN and runs it with no signature or hash check, and replaces it unasked. None of `music/src/soloist/` is compiled |
| `music/src/spotify/playback.rs` | the `Official` engine and the branch of `Factory::start` that picks it are behind the `soloist` feature | Spotify always plays through librespot |
| `music/src/spotify/pathfinder/hashes.rs` | the request to `billowing-resonance-da83.johnwatson.workers.dev` removed with its parser and tests; `resolve` and `refetch` read every operation from the open.spotify.com web player bundle, looking the asked one up by name as a query or a mutation | no third-party hash service. See "Known consequences" |
| `music/src/spotify/pathfinder.rs` | the two calls follow the new signatures | as above |
| `music/src/artwork.rs` | `upload()`, `fetch()`, the Uguu endpoint and their types removed | no cover from disk or a private server is uploaded anywhere |
| `state/src/discord/mod.rs` | `Wanted::Upload`, the `Upload` type, the `uploaded` and `upload_failed` fields, `own_upload` and the `upload` argument removed | the caller of the above. The stored `discord_upload_covers` setting is no longer read |
| `music/src/credentials.rs` | on Windows `write` seals the contents with DPAPI (current user) behind a `sonicly-dpapi-1` prefix; new `read` unseals, and reads a plaintext file from an older build as it lies and writes it back sealed; `migrate` reads through it; one unit test | `secure()` does nothing on Windows, so the files were plaintext |
| `music/src/{jellyfin,subsonic,tidal,deezer,apple}/auth.rs`, `music/src/youtube/mod.rs` | the stored sign-in is read with `credentials::read` | the readers of sealed files |
| `music/src/subsonic/auth.rs`, `music/src/jellyfin/auth.rs` | `normalize_server` assumes `https://` for an address with no scheme; a typed `http://` is kept; the Jellyfin test follows | a password should not cross the network in the clear by default |
| `state/src/settings.rs` | the lyrics source stored as `"Apple Music"` is left out of the default enabled list | it is the third-party Binimum proxy, and now starts switched off |

## Changes in `crates/` and assets that remove upstream behaviour

| File | Change |
| --- | --- |
| `crates/sonicly/src/main.rs` | Apple Music and Deezer are not registered; the widevine-host early exit is gone; the window opens with `focus: false` while `SONICLY_SHOT` is set |
| `crates/sonicly/src/logging.rs` | `console()`, used only by the widevine host, removed |
| `crates/sonicly/src/assets.rs`, `assets/macos/Info.plist` | app id is `io.github.flyship09.sonicly` |
| `scripts/windows/sonicly.iss` | new `AppId` GUID, Deezer url scheme removed |
| `assets/linux/sonicly.desktop`, `assets/macos/Info.plist` | Deezer url scheme removed |
| `crates/views/src/screens/login.rs` | the usage consent checkbox and every report to `Usage` removed (telemetry) |
| `crates/views/src/screens/settings/about.rs` | the Sonora team list and its avatars removed; `SOURCE_URL` is Sonicly's repository; the update rows show only while `state::RELEASE_REPO` is set (updater); new "Open-source licenses" row opens `THIRD-PARTY.md` beside the executable |
| `crates/views/src/screens/settings/appearance.rs` | the community themes link (sonorahq.org) and its row removed |
| `crates/views/src/chrome/notice.rs` | the "What to install" link to Sonora's README removed from requirement cards |
| `crates/views/src/screens/settings/sessions.rs` | the "official server" switch removed; the address row always shows and says it is required |
| `crates/views/src/screens/settings/discord.rs`, `preview/discord.rs` | the Sonicly button and the cover upload row removed; the master switch is disabled with a note while `state::DISCORD_APPLICATION_ID` is `None` |
| `crates/views/src/screens/settings/mod.rs` | slots `CommunityThemes`, `SessionsOfficial`, `DiscordUploadCovers` removed, `Licenses` added |
| `assets/i18n/en-US/main.ftl` | strings for the rows above; every locale relabels the Binimum lyrics source |
| `scripts/generate-notices.py` | the corresponding source is Sonicly's repository, with Sonora credited |

## Known consequences

- **Spotify search and genre pages.** With the hash service gone, an operation's hash is only
  found if the main web player bundle names it. On 2026-11-16 that bundle carried `getAlbum`,
  `getTrack`, `home`, `libraryV3`, `pinLibraryItem`, `unpinLibraryItem` and
  `queryArtistOverview`, and did **not** carry `searchDesktop`, `searchAlbums`,
  `searchPlaylists`, `browseAll` or `browsePage`. Those queries now fail with "Spotify bundle has
  no ... operation" unless a hash is already cached.
- **Still plaintext on Windows.** Spotify's `credentials.json` is written by librespot's cache
  and YouTube's `cookies.json` by the `ytmusic` crate, not by `credentials.rs`, so neither is
  sealed. Soloist's key file is not converted either, and Soloist is compiled out.
- **Self-hosted servers on plain http** need `http://` typed in front of the address.
- **Inert settings.** `sessions_official`, `discord_sonora_button` and `discord_upload_covers`
  are still stored and have accessors in `state/src/settings.rs`, and nothing reads them.
- **Leftover strings.** The Fluent keys of removed rows are still in the locale files.
- **`THIRD-PARTY.md`** is not in the repository. `scripts/generate-notices.py` writes it, and the
  Windows installer script expects it.
