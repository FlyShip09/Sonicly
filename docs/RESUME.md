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
- **In progress:** nothing in the engine. The uncommitted ownership pass waits
  for review and a commit.
- **Not verified:** the app has not been launched since the pass, and no test
  has been run, the new hash-lookup tests included.
- **Next:** first off-screen launch (`scripts/shot.ps1`), Sonicly's own icon and
  wordmark, then M1 (real playback) in `docs/PARITY.md` order.
- **Requested on top of the brief:** download from a link through the user's own
  yt-dlp; a user-styled lyrics page (background image or video, typeface, size);
  a compact floating player; low memory use. See `docs/DESIGN.md` 8-11.
- **Waiting on the owner:** GitHub repository (PRs, star count), Discord
  application id, whether ffmpeg may be installed for multi-format test files.
- **House rules:** never open tryliner/desktop; no DRM circumvention; no
  credentials in chat or files; windows open only through `scripts/shot.ps1`
  (off-screen, unfocused); the owner prefers building over test runs.
