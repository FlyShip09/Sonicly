# Where things stand

Read this first in a new session.

- **Branch:** `m0/setup`. `main` holds only the license. `archive/v0-native` is
  the earlier from-scratch build (gpui-kit, own engine), kept for reference.
- **Base:** Sonora at commit `6e3cfb0` (full hash in `docs/UPSTREAM.md`), cloned
  read-only into the git-ignored `reference/sonora`. Do not build or run it.
- **Done:** import; upstream's servers removed and the engine hardened; the
  ownership pass of `docs/DESIGN.md` 13: every crate lives under `crates/`,
  internals are named Sonicly, and Apple Music, Deezer, Widevine and Soloist are
  deleted rather than gated. Spotify search and genre hashes are read from the
  web player's own lazily loaded chunks. `fmt`, `check`, `clippy -D warnings` and
  `cargo build -p sonicly` pass. `docs/UPSTREAM.md` is the provenance record.
- **M2 identity (`m2/identity`):** frame and page with the frame on Mica /
  vibrancy, one type scale, edged covers, cover washes on album, playlist and
  song pages, shelf reveal, sidebar shortcuts; `docs/DESIGN.md` 17-24, captures
  in `docs/screenshots/m2/`. Debug builds take `--screen`, `--theme` and
  `--size` for `scripts/shot.ps1` captures (`crates/sonicly/src/staging.rs`).
  Not seen on macOS or Linux.
- **In progress:** nothing in the engine. The uncommitted ownership pass waits
  for review and a commit.
- **Not verified:** the app has not been launched since the pass, and no test
  has been run, the new hash-lookup tests included.
- **Next:** first off-screen launch (`scripts/shot.ps1`), Sonicly's own icon and
  wordmark, then M1 (real playback) in `docs/PARITY.md` order.
- **Requested on top of the brief:** download from a link through the user's own
  yt-dlp; a user-styled lyrics page (background image or video, typeface, size);
  a compact floating player; low memory use. See `docs/DESIGN.md` 8-11.
- **Waiting on the owner:** Discord application id, whether ffmpeg may be
  installed for multi-format test files, whether Inno Setup may be installed to
  compile the installer. The GitHub repository exists
  (<https://github.com/FlyShip09/Sonicly>); it has no release yet, so
  `state::RELEASE_REPO` stays `None` and the update check stays off.
- **House rules:** never open tryliner/desktop; no DRM circumvention; no
  credentials in chat or files; windows open only through `scripts/shot.ps1`
  (off-screen, unfocused); the owner prefers building over test runs.

## Release (`m8/release`)

Windows x64 is the only platform built so far. From the repository root:

```sh
# 1. notices, only after a dependency changed (needs `cargo install cargo-about --locked --features cli`)
python scripts/generate-notices.py

# 2. the release exe, about 7 minutes cold
cargo build --release --locked -p sonicly

# 3. the portable zip: dist/Sonicly-<version>-windows-x64-portable.zip, prints its SHA-256
powershell -ExecutionPolicy Bypass -File scripts/windows/package-portable.ps1

# 4. the installer: dist/Sonicly-<version>-windows-x64-setup.exe, needs Inno Setup 6
powershell -ExecutionPolicy Bypass -File scripts/windows/build-installer.ps1

# 5. look at it without a window popping up; prints memory, CPU and time to first window
powershell -ExecutionPolicy Bypass -File scripts/shot.ps1 -Exe target/release/sonicly.exe -Out .targets/shots/release-home.png -Profile release -Size 1080x1080 -Wait 8
```

- **Measured on 2026-11-16, version 0.1.0:** `sonicly.exe` is 82.6 MB (GUI
  subsystem, Sonicly icon, version resource 0.1.0), the zip 30.1 MB. Home at
  1080x1080 after 8 s, YouTube Music guest: 145 MB working set, 150 MB private,
  0.9 s CPU, first window 716 ms after process start. One run, not an average.
- **Not built:** the installer (Inno Setup is not installed, and installing it
  is the owner's call), any macOS or Linux package, an arm64 build. Nothing is
  code-signed, so SmartScreen will warn on first run.
- **Not seen:** the star rows and switch under Settings, About; the star button
  in a narrow window; a settings migration on a real file (there is no step
  yet, only the unit tests).
- **Before the first public release:** set `state::RELEASE_REPO` only if the
  self-updater is wanted, since it downloads and runs an installer, which the
  house rules forbid as it stands. `kakasi` declares `GPL-3.0`, read as GPL-3.0-only, so the binary as a
  whole is distributed under GPL version 3.
