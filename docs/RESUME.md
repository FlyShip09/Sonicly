# Where things stand

Read this first in a new session.

- **Branch:** `m0/setup`. `main` holds only the license. `archive/v0-native` is
  the earlier from-scratch build (gpui-kit, own engine), kept for reference.
- **Base:** Sonora at the commit in `vendor/SONORA_REV`, cloned read-only into
  the git-ignored `reference/sonora`. Do not build or run it.
- **Done:** import (`vendor/` engine crates, `crates/` interface crates),
  scripted rename (`scripts/rebrand.py`), docs skeleton.
- **In progress:** compiling out Apple Music and Deezer, removing upstream's
  servers, first clean build. See `docs/UPSTREAM.md`.
- **Direction (owner, latest):** the engine is to be Sonicly's own code that works
  1:1 like Sonora's. After the first clean build, move `vendor/*` into `crates/`,
  rename internal identifiers, and delete (not gate) removed features. See
  `docs/DESIGN.md` 13.
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
