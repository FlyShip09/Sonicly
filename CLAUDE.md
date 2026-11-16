# Sonicly

A native desktop music player. The engine was ported from Sonora and is now Sonicly's own code
(see `docs/UPSTREAM.md`, `docs/DESIGN.md`). Read `docs/RESUME.md` first.

Built with Rust and [GPUI](https://github.com/zed-industries/zed), Zed's UI framework.
Cargo workspace, edition 2024, resolver 3.

## Crate layout

```
crates/
  sonicly/     binary: main, window, actions, asset registry, HTTP client shim, tray, dock
  views/      screens, shells (Workspace, FullscreenView) and app chrome
  state/      GPUI entities holding app state, and all async orchestration
  music/      provider traits and models in the root, one submodule per provider (no GPUI)
  storage/    the state.sqlite and cache.sqlite paths, schema and connection setup
  ui/         design system: theme, metrics, layout ladder and reusable elements
  router/     Destination enum, navigation history, Link trait
  input/      text input element, global actions and keybindings
  i18n/       Fluent localization: the `t!` macro, locale selection, embedded .ftl
  icons/      the icon packs: registry, active pack, path resolution, AssetSource
  embed/      build-script helper that walks a folder and writes include_bytes! literals
  webview/    a native browser window over a throwaway session, for cookie sign-ins and PO tokens
```

Dependency direction is strict. Do not create a back edge:

```
sonicly → views → state → music
         state, music → storage
         state → webview
         all ui-side crates → ui, router, input → ui → gpui
         every ui-side crate → i18n, icons → gpui
```

- `state` and `views` see only the traits and models in the root of `music`, never a provider
  module. Only `sonicly/src/main.rs` names a concrete provider.
- `music` never depends on `gpui`. `ui` never knows about `music`, `state` or playback. Widgets
  that need app state live in `views/src/chrome/`.
- `music::engine` is the one playback engine and `music::stream` the one progressive download. A
  provider implements `engine::Fetch` and `stream::Body` rather than writing its own. The engine
  applies loudness normalisation, so a provider only reports a track's level through
  `Fetch::loudness`. Spotify is the one exception, because librespot decodes and normalises
  inside its own player.

## Rules

- **Reuse before you build.** Check `crates/ui/src/lib.rs`, `crates/views/src/shared/` and
  `crates/views/src/chrome/` first, and extend an element with a builder method rather than
  writing a sibling. New elements copy the shape of `ui/src/button.rs`, including the
  `mem::take` and `refine` of caller styles.
- **Never call the Spotify Web API.** No `reqwest` to `api.spotify.com`, no client secret, no
  `rspotify`. Spotify data comes from librespot's `spclient`, and auth uses Spotify's own client id
  in `auth.rs`.
- **Never hardcode a color, radius or size.** Read them from `cx.theme()` and `theme.metrics`. A
  literal `px(…)` is allowed only for a local, non-scaling detail declared as a module `const`. A
  new color token goes into `Theme`, every `Theme::*()` constructor, `ThemeOverrides` and
  `apply_color!`.
- **Never write a bare breakpoint.** Use the `ui::Room` ladder, and classify width with
  `Chrome::room` or `Chrome::content`, never the raw viewport. A view whose layout depends on
  width observes `Chrome::entity(cx)`.
- **Every user-facing string comes from Fluent.** Add the key to `assets/i18n/en-US/main.ftl`. Other
  locales may lag. Never call `t!` in a constructor: store the key and resolve it in `render`.
  Counts use Fluent selectors. Log messages and `.context(…)` stay in English.
- **Every interactive element is announced by screen readers.** A node reaches the
  accessibility tree only with both `.id()` and `.role()`; pair every role with an
  `aria_label` resolved from Fluent in `render` (icon-only buttons take their name from the
  tooltip). Keep ids stable across frames and unique within a list: wrap rows in
  `div().id(("kind", stable_id))` with `aria_position_in_set`/`aria_size_of_set`, never a bare
  row index. Extend `crates/ui/tests/a11y_tree.rs` with every new primitive.
- **Every interactive element is announced by screen readers.** A node reaches the
  accessibility tree only with both `.id()` and `.role()`; pair every role with an
  `aria_label` resolved from Fluent in `render` (icon-only buttons take their name from the
  tooltip). Keep ids stable across frames and unique within a list: wrap rows in
  `div().id(("kind", stable_id))` with `aria_position_in_set`/`aria_size_of_set`, never a bare
  row index. Extend `crates/ui/tests/a11y_tree.rs` with every new primitive.
- **Network work runs on tokio.** Anything touching `MusicApi`, librespot or sockets goes inside
  `Io::global(cx).spawn`, awaited from `cx.spawn` and applied in `this.update` ending with
  `cx.notify()`. Store the returned `Task` in a field and never detach a data load. Network-backed
  features belong in a `state` entity, not a view. An entity that caches provider data clears on
  `SessionEvent::SignedOut`.
- **Never drive a player from a view.** Go through `state::Playback`.
- **Assets are picked up from their folder.** Drop an icon into `assets/icons/<pack>/` or a face
  into `assets/fonts/`. Call sites spell `"icons/<name>.svg"` and resolve it with `icons::path` at
  render.
- **Generated files are regenerated, never edited.** App icons come from
  `scripts/generate-icons.py`, `THIRD-PARTY.md` from `scripts/generate-notices.py`, the README
  translation table from `scripts/i18n-coverage.py`.
- **Do not add tests unless asked.** Offering is fine. Existing tests are `#[cfg(test)] mod tests`
  at the bottom of the file they cover. Accessibility regression tests in
  `crates/ui/tests/a11y_tree.rs` are the exception: extend them with every new primitive. Accessibility regression tests in
  `crates/ui/tests/a11y_tree.rs` are the exception: extend them with every new primitive.

## Building and checks

```sh
nix develop                    # or direnv allow
cargo run --locked --package sonicly
RUSTC_WRAPPER= dx serve --hot-patch --package sonicly --features hot-reload   # hot reload
cargo fmt
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

The devShell has no `cargo` or `cargo-clippy`; those come from the system profile. The flake's
default package compiles the working tree. CI caches the `stable` branch, the `nightly` tag and
main on sonicly.cachix.org.
`.cargo/config.toml` links with mold on `x86_64-unknown-linux-gnu`, so mold must be on PATH, or
build with `RUSTFLAGS=""`. `dx serve --hot-patch` patches a running debug build in about a second on every save. The first
build is a full one into its own `desktop-dev` profile. `RUSTC_WRAPPER=` keeps sccache out, since a
sccache server of another version fails the build. A changed struct layout, a dependency, gpui or
`Cargo.toml` needs a restart, `r` in the dx console.

Clippy is clean, `--all-targets` included, and stays that way. Boxed callback fields get a
module-local `type` alias rather than a `type_complexity` warning. The one `#[allow]` is
`reversed_empty_ranges` on the `clamp_range("abc", &(2..1))` test in `crates/ui/src/input/mod.rs`.

## Patching gpui

`gpui` comes from the `sonorahq/gpui` fork, pinned by `rev`. To change it, clone the fork into
the ignored `vendor/gpui` (never a submodule), branch off `master`, and point Cargo at it while
you work:

```toml
[patch."https://github.com/sonorahq/gpui"]
gpui = { path = "vendor/gpui/crates/gpui" }
gpui_platform = { path = "vendor/gpui/crates/gpui_platform" }
```

Run `cargo fmt` and `cargo clippy` inside `vendor/gpui` too. Once the fork's change is merged,
drop the `[patch]`, move both `rev`s to the merge commit and let `Cargo.lock` follow. A `[patch]`
is never committed.

## Code style

- Every type and every function whose behaviour is not obvious from its name carries a `///`
  comment saying what it is for and what a caller has to know. One or two plain sentences.
- `use gpui::prelude::*;` then explicit imports. Traits are imported anonymously
  (`use ui::ActiveTheme as _;`).
- Module order: `use` block, `const`s, types, impls, private free helpers at the bottom.
- In render, prefer `.when()`, `.when_some()` and `.map()` over branching. Two-arm boolean choices
  use `match flag { true => …, false => … }`. Use let-else and return early.
- `anyhow::Result` at boundaries with lowercase `.context("cannot …")`. Logs are prefixed by
  subsystem (`"playback: …"`).
- Dependencies go in the root `[workspace.dependencies]`. `gpui` and `gpui_platform` are pinned to
  one git rev and move together.

## Commits

Conventional Commits: `type(scope): description`, imperative, lowercase, no trailing period, no
body. Scopes in use: `views`, `ui`, `music`, `state`, `playback`, `player`, `settings`, `router`,
`local`, `sonicly`, `nix`.

## Pull requests

One heading, one list and, for a visual change, screenshots. Nothing else:

```markdown
## Summary

- add playlist create, rename, delete, visibility, and track mutation support
- add album and playlist queue actions
```

Bullets are lowercase, imperative, one line each, and describe behaviour rather than files.

A pull request that noticeably changes what the user sees gets screenshots of the result. That
means a new screen or element, a reworked layout, a theme change, or anything else a reviewer
would only judge by looking at it. When the change alters something that already exists, show it
before and after. The images go below the list with no heading. A small spacing nudge or a change
nobody can see needs none.

## Sonicly house rules

- **Never open, clone, fetch or read github.com/tryliner/desktop**, and never call a Liner API.
  Liner-style features are designed here and logged in `docs/DESIGN.md`.
- **No DRM circumvention**, nothing that downloads and runs an executable, no telemetry, no
  calls to servers of our own. Apple Music, Deezer and the Widevine host were removed on purpose.
- **Never ask for or store credentials** in chat or files. Account sign-in happens inside the app.
- **Windows never pop up in front of the owner.** Launch the app only through
  `scripts/shot.ps1` (`-Exe target/debug/sonicly.exe -Out <png> [-Wait s] [-Size WxH] [-ExeArgs ..]`):
  it opens unfocused, off-screen, in a separate `sonicly-shot` profile, and prints memory use.
  Use `-Profile <name>` for your own profile when several sessions capture at once. A handful of
  purposeful captures, never loops.
- **The owner prefers building over test runs.** Verify with `cargo fmt`, `cargo check`,
  `cargo clippy --workspace --all-targets -- -D warnings` and `cargo build -p sonicly`. Write unit
  tests for new non-UI logic; run `cargo test` only when asked.
- **No AI slop.** Neutral surfaces; colour comes from the playing cover. No gradient text, glows,
  particles, emoji, sparkle icons, purple-blue gradients, cards inside cards, or glass on glass.
  Sentence case, plain verbs, errors that say what happened and how to fix it. Motion answers an
  action or shows where something went; ambient motion only in Now Playing and playing indicators.
  Before calling a screen done, remove one decoration.
- **Memory is a budget.** Home idles at about 128 MB (debug). Decode artwork at display size,
  virtualize long lists, release what is off-screen, and report the number `scripts/shot.ps1`
  prints for any screen you add.
- Add Fluent keys beside related keys in `assets/i18n/en-US/main.ftl`, never at the end of the
  file; other locales fall back to English.
- Commits follow Conventional Commits. Do not push; the lead merges.
