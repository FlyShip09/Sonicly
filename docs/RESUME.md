# Where things stand

Read this first in a new session.

- **Branch:** `integration`, cut from `m0/setup` with `m1/sources`, `m2/identity`,
  `m6/floating` and `m5/lyrics-style` merged in that order (merge commits). Not
  pushed, no pull request. The four branches and their worktrees under
  `.claude/worktrees/` are untouched. `main` holds only the license;
  `archive/v0-native` is the earlier from-scratch build, kept for reference.
- **Base:** Sonora at commit `6e3cfb0` (full hash in `docs/UPSTREAM.md`), cloned
  read-only into the git-ignored `reference/sonora`. Do not build or run it.
- **Built:** the ported engine (`docs/DESIGN.md` 13); M1 sources (PLS, XSPF,
  cue sheets, download from a link through the user's yt-dlp, sealed Spotify and
  YouTube sign-ins, `docs/PRIVACY.md`; decisions 14-16); M2 identity (frame on
  the system material, type scale, edged covers, cover washes, shelf reveal,
  sidebar shortcuts; 17-24, captures in `docs/screenshots/m2/`); M6 floating
  mini player, file drops and drawn covers (25-27, `docs/screenshots/m6/`); M5
  lyrics style with a Media Foundation video background (28,
  `docs/screenshots/m5/`).
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
- **Not verified:** streaming sources, sign-ins and yt-dlp downloads (they need
  the owner); macOS and Linux; a real drag from the system; gapless joins,
  loudness normalization and the equalizer by ear; anything at audible volume.
- **Known issues:** see the end of `docs/REPORT.md`.
- **Next:** the owner's pass over streaming, sign-in and a yt-dlp download; then
  the open items in `docs/REPORT.md`.
- **Waiting on the owner:** GitHub repository (PRs, star count), Discord
  application id, Inter Display (a download), review of `integration`.
- **House rules:** never open tryliner/desktop; no DRM circumvention; no
  credentials in chat or files; windows open only through `scripts/shot.ps1`
  (off-screen, unfocused, `--debug-mute` when something plays); the owner
  prefers building over test runs.
