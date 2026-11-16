<p align="center"><img src="assets/brand/sonicly-wordmark.svg" alt="Sonicly" width="260"></p>

A native desktop music player for Windows, macOS and Linux, written in Rust
with GPUI. It plays your local files and streams from services you have an
account with, in one library.

Sonicly is free software under the GPL, version 3 or later. Its engine began as
a port of [Sonora](https://github.com/sonorahq/sonora); see `NOTICE.md`.

## What it plays

| Source | Needs |
| --- | --- |
| Local files (FLAC, MP3, AAC, ALAC, Opus, Vorbis, WAV, AIFF) | nothing |
| YouTube Music | nothing as a guest; sign in for your library |
| Spotify | a Premium account |
| Tidal | a Tidal account |
| Subsonic / Navidrome, Jellyfin | your own server |

Sonicly does not circumvent DRM. Apple Music and Deezer are not supported.

## Build

Rust is pinned in `rust-toolchain.toml`; `rustup` installs it on first use. On
Windows you also need the MSVC build tools and the Windows SDK.

```sh
cargo run --package sonicly
scripts/check            # format, lints, tests, build
scripts/check --no-test  # the same without the test run
```

## Privacy and safety

Sonicly has no telemetry, no update pings, and no servers of its own. It
contacts only the music, lyrics, artwork and scrobbling services you use. Saved
logins are sealed with your Windows account's key. What was removed from the
upstream code, and why, is listed in `docs/UPSTREAM.md`.

## Documents

- `docs/DESIGN.md` — every design decision and its reason
- `docs/PARITY.md` — each upstream feature and its status here
- `docs/BRAND.md` — the mark, colors and icon rules
- `docs/RESUME.md` — where work stands
