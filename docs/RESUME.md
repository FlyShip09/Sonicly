# Where things stand

Read this first in a new session.

- **Branch:** `integration`, cut from `m0/setup` with `m1/sources`, `m2/identity`,
  `m6/floating` and `m5/lyrics-style` merged in that order (merge commits), then
  `m8/release`, `m8/onboarding`, `m9/apple` and `m9/motion`. Pushed; pull request 2
  into `main` is open. `main` holds the M0 setup; `archive/v0-native` is the earlier
  from-scratch build, kept for reference.
- **Base:** Sonora at commit `6e3cfb0` (full hash in `docs/UPSTREAM.md`), cloned
  read-only into the git-ignored `reference/sonora`. Do not build or run it.
- **Built:** the ported engine (`docs/DESIGN.md` 13); M1 sources (PLS, XSPF,
  cue sheets, download from a link through the user's yt-dlp, sealed Spotify and
  YouTube sign-ins, `docs/PRIVACY.md`; decisions 14-16); M2 identity (frame on
  the system material, type scale, edged covers, cover washes, shelf reveal,
  sidebar shortcuts; 17-24, captures in `docs/screenshots/m2/`); M6 floating
  mini player, file drops and drawn covers (25-27, `docs/screenshots/m6/`); M5
  lyrics style with a Media Foundation video background (28,
  `docs/screenshots/m5/`); M8 welcome screen, local-only use, Tab focus, seeded
  covers, settings pass (31-36, `docs/screenshots/m8/`) and release work (29-30,
  see Release below); M9 Apple Music through Apple's web player (37) and the
  calm desktop look with its motion (38-42, `docs/screenshots/m9/`). The
  self-updater's download-and-install code is deleted.
- **Direction from the owner (2026-11-16):** clean and macOS-like, detailed, with
  small action-driven animations.
- **Checks:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D
  warnings` and `cargo build -p sonicly` pass on `integration`. `cargo test` has
  not been run.
- **Verified in a running build:** local playback of eight formats, transport
  controls, resume after a relaunch, playlist import and cue sheets. The table
  and the method are in `docs/REPORT.md`; statuses are in `docs/PARITY.md`.
- **Fixed here:** a song opened while Sonicly was restoring that same song from
  the last run stayed paused at 0:00 (`state::Playback`, the `unsent` flag).
- **Captures and checks without input:** every debug launch flag is
  `--debug-<name>[=<value>]`, read in `crates/sonicly/src/debug.rs` and listed
  in its header and in `CLAUDE.md`. `scripts/shot.ps1` takes `-Profile`,
  `-Size`, `-Title`, `-ExeArgs` and prints memory and CPU. The scratch files of
  the integration checks are in the git-ignored `.targets/integration/`.
- **Not verified:** streaming sources, sign-ins (Apple Music's whole-track
  playback among them) and yt-dlp downloads (they need the owner); macOS and
  Linux; anything that needs a pointer or keyboard (hover, focus rings, drag);
  gapless joins, loudness normalization and the equalizer by ear; anything at
  audible volume.
- **Known issues:** see the end of `docs/REPORT.md`.
- **Next:** the owner's pass over streaming, sign-in (Apple Music first) and a
  yt-dlp download; then the motion items listed as not done in `docs/REPORT.md`.
- **Waiting on the owner:** Discord application id, Inter Display (a download),
  Inno Setup for the installer, review of the open pull request.
- **House rules:** never open tryliner/desktop; no DRM circumvention; no
  credentials in chat or files; windows open only through `scripts/shot.ps1`
  (off-screen, unfocused, `--debug-mute` when something plays); the owner
  prefers building over test runs.

## Release

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
- **Before the first public release:** setting `state::RELEASE_REPO` turns on a
  check for a newer release that links to its page. Sonicly never downloads or
  installs an update; that code is deleted. `kakasi` declares `GPL-3.0`, read as
  GPL-3.0-only, so the binary as a whole is distributed under GPL version 3.
