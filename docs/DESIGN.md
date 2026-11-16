# Design decisions

Newest last. One entry per decision: what was chosen, and why.

1. **Built on Sonora's source, not beside it.** The engine crates are vendored
   unmodified where possible and the interface crates are ported. A second
   engine would have to re-solve streaming, gapless playback, and device
   handling that already work.
2. **The vendored crates keep upstream's internal names.** Only string literals
   and assets are renamed (`scripts/rebrand.py`), so `vendor/` keeps merging
   cleanly. A user never sees an identifier.
3. **No Widevine code in the tree.** Upstream's `widevine` crate is not
   imported. `vendor/music/src/drm.rs` is a stub that reports "unsupported", so
   the related settings hide themselves. The Apple Music and Deezer providers
   sit behind Cargo features that are off by default; `apple-music` cannot be
   built from this repository.
4. **Nothing phones home.** The install counter, the update check against
   upstream's releases, the official shared-listening server, and upstream's
   Discord application are removed. Each is a single constant to set once
   Sonicly has its own.
5. **App id `io.github.flyship09.sonicly`, scheme `sonicly://`.** Reverse-DNS
   on the account that owns the code, since Sonicly has no domain.
6. **Reference screenshots of the upstream app are not taken.** The owner asked
   that upstream not be run; screens are judged against upstream's source and
   the checklist in the project brief.
7. **Liner is never consulted.** Features described as Liner-style are designed
   here from the behavior a user would see, and each gets an entry in this log.
8. **Downloads from a link use the user's own yt-dlp.** Sonicly runs a `yt-dlp`
   the user has installed (found on `PATH` or set in Settings) and never
   downloads or bundles an executable itself. It asks for the native audio
   stream, so no ffmpeg or re-encoding is needed, then tags the file and adds it
   to Local Music. yt-dlp does not handle DRM-protected media, and Sonicly adds
   nothing that would.
9. **Apple Music is not playable.** Its streams are DRM-protected and need a
   Widevine module, which this project does not ship (decision 3). The provider
   architecture still supports adding services that allow third-party playback.
10. **The lyrics page is user-styled.** Background (cover-derived, a chosen
    image, or a chosen video), typeface, size, alignment, and blur are settings
    with live preview. A video background is decoded at reduced resolution and
    paused whenever the lyrics are not visible, to keep memory and CPU low.
11. **Memory is a budget, not an afterthought.** Artwork is decoded at display
    size and evicted least-recently-used, long lists are virtualized, and
    nothing animates while idle. Idle and playing memory are measured and
    recorded in `docs/REPORT.md` before any release.
12. **Hardening after a source audit.** A read-only audit found nothing
    malicious but six things worth removing or tightening, all done before the
    first launch: the helper that downloaded and ran Spotify's player binary
    unverified is compiled out; a personal third-party server consulted for
    Spotify query hashes is no longer contacted; covers are never uploaded to an
    image host; saved logins are sealed with the Windows user's key instead of
    stored as plain text; a third-party lyrics proxy is off by default and named
    for what it is; self-hosted servers default to HTTPS. Left in place and
    disclosed: YouTube playback loads a hidden YouTube page to satisfy its bot
    check, and Tidal sign-in uses client ids taken from Tidal's own apps.
13. **The engine is Sonicly's own, modeled 1:1 on Sonora's.** Supersedes the
    "vendored, kept in sync" approach of decisions 1-2. The owner wants an engine
    that works exactly as Sonora's does but is Sonicly's code: so Sonora's engine
    source is ported into Sonicly's own crates under `crates/`, given Sonicly's
    names throughout, and reshaped freely. Features Sonicly does not ship (Apple
    Music, Deezer, the downloaded Spotify helper, Widevine screens) are deleted
    rather than feature-gated. `docs/UPSTREAM.md` becomes the provenance record
    (which commit the port started from, what was removed and why) instead of a
    merge guide. Behavior is checked against the source in `reference/sonora`.
14. **A cue sheet track is a stretch of its file, named in the track id.** A
    sheet beside the one audio file it describes replaces that file in the
    library with the sheet's tracks. Each track's id is the file's id followed
    by `#cue=<start>-<end>`, counted in the sheet's own 1/75 s frames (`end`
    empty for the last track). Nothing else carries the range: the local
    provider turns the id into the existing `Trim` (skip, take) and wraps its
    decoder in the existing `Trimmed` source, so the engine, the queue and the
    seek bar see an ordinary track whose position starts at zero. Chosen over
    a new field on `Track` or the queue entry because an id already travels
    everywhere a track does (queue, history, favorites, restore) and nothing
    outside `music::local` had to change. The scan index still holds one row
    per real file and the sheet is applied on every scan, so editing a sheet
    needs no rescan of the audio; the list of sheets is remembered beside the
    index, because a folder listed from memory is never read from disk. A
    sheet that names several files changes nothing (those files are already a
    track each). A cue track's tags cannot be edited and its file cannot be
    deleted from the app, since both would act on the whole album, and it gets
    no lyrics from a file beside the audio, which would be timed for the whole
    file.
15. **How the user's yt-dlp is run.** Extends decision 8. yt-dlp is started
    directly with a fixed argument list, never through a shell, in the chosen
    music folder, with `--ignore-config` and `--no-exec` so no configuration
    of the user's can run a program or move the file, and with the link after
    `--`. Only `http`/`https` links with a host are passed on. Progress and
    file facts are asked for as prefixed lines (`--progress-template`,
    `--print`) with every text field JSON-encoded by yt-dlp, so a title cannot
    pose as another field; all of it is treated as data, cleaned of control
    characters and cut to length, and a reported file name is used only by its
    last part inside the chosen folder. The format is
    `bestaudio[ext=m4a]/bestaudio`: the stream as served, so no ffmpeg; a site
    with no audio-only stream is refused with that explanation rather than
    downloaded as video. The thumbnail is fetched only to be embedded: Sonicly
    crops it square, re-encodes it as JPEG itself and writes it with the tags
    through `lofty`, then deletes the image file. WebM, which `lofty` cannot
    tag, is kept untagged. One download runs at a time, and it outlives its
    dialog.
16. **Spotify's and YouTube's sign-ins are sealed around the libraries that
    write them.** Neither goes through `credentials.rs` by itself. librespot
    (the pinned fork) can keep the credential in memory only, so Sonicly reads
    and writes Spotify's `credentials.json` itself, sealed; no plain copy is
    written at any point. `ytmusic` only loads and saves its cookie store as a
    plain file and exposes neither the store nor a hook, and it saves whenever
    Google sets a cookie. Without patching it, the lasting copy
    (`cookies.json`) is kept sealed and the crate is pointed at
    `cookies.live.json`, which Sonicly writes for the instant a client loads
    it and deletes at once, then seals and deletes again after every rotation
    it asks for and on a one-minute tick. That leaves a window of at most a
    minute (or until the next launch after a crash) in which rotated cookies
    are on disk unsealed. It is documented in `docs/PRIVACY.md`; closing it
    needs a load/save hook in `ytmusic`.
17. **Sonicly's own look, after an audit of the port (M2).** The ported shell
    read as its source: one flat near-black (or near-white) sheet, a rule under
    the title bar and another beside the sidebar, 24 px bold shelf titles, Quick
    picks boxed in a bordered card, and a fixed blue for selected rows and text
    selection. The audit covered `crates/ui` (theme, metrics, glass, motion) and
    captures of Home, Search, Library, an album, an artist, Settings, the quick
    search and a context menu in both themes (`docs/screenshots/m2/before-*`).
    - **Kept:** neutral, hue-free greys; colour only from covers (adaptive tint,
      cover-coloured play buttons); Inter as the one face; the `Room` ladder and
      metrics derived from the base size; the motion system (springs, the page
      entrance, presses, reduced motion, no frames while idle); the frosted
      player bar as the one glass layer over the page; the error states, which
      already say what happened and offer one retry.
    - **Changed, and why:** listed in 18-23.
18. **Frame and page.** The title bar and the sidebar are one surface, the
    frame (`Theme::sidebar`, a step off the page: `#161616` on dark, `#efefef`
    on light), with no rule between them. The page is a single opaque surface
    set into it, edged by one hairline (`sidebar_border`, now a 7-9% white or
    black veil) along the sides it shares with the frame. Depth comes from value,
    not borders. The adaptive tint leaves the frame alone, so a cover's colour
    reaches the page and never the window's chrome. The untinted selection and
    active row are neutral greys; blue was a fixed accent the brand rules out.
19. **The frame sits on the platform's material.** With the Blur setting on
    and an opaque window, the frame paints its colour thin (`ui::frame_fill`)
    over the system material and the page paints over it, so it is never glass
    on glass:
    - Windows 11 (22H2, build 22621+): Mica (`WindowBackgroundAppearance::
      MicaBackdrop`). Sonicly probes DWM for a system backdrop when the window
      opens (`ui::WindowMaterial`) and sets `DWMWA_USE_IMMERSIVE_DARK_MODE` from
      its own theme after every system change, since GPUI follows the system's.
      Older Windows keeps the solid frame. An inactive window shows Mica's flat
      fallback, which is also what the off-screen captures show.
    - macOS: the window's blurred backdrop (GPUI's `NSVisualEffectView`) under a
      thicker fill, since a raw blur can show any wallpaper. Not verified.
    - Linux and FreeBSD: the compositor's blur when `WindowBlur` reports it,
      solid otherwise. Not verified.
    A sidebar laid over the page in a narrow window stays solid with its own
    edge, since a thin fill there would show the page and not the material.
20. **One type scale.** `ui::Text` is 11, 12, 13, 14, 16, 19, 24 and 32 px at
    the default 14 (Display moved from 30 to 32), with steps that widen as they
    climb so a page title, a section and a row never read as neighbours. Every
    title is semibold; bold is gone from headings. Large steps set tighter
    leading (`Text::leading`: 1.1 for Display, 1.15 for Title, 1.2 for Large).
    `ui::heading` is a page title, `ui::section_heading` a section, and
    `spoken_heading` picks between them by its level. Shelves, Quick picks,
    library groups and an artist's Popular and Releases moved from 24 px bold to
    19 px semibold, so the artwork leads. The space between page sections is a
    metric (`Metrics::section`). Inter stays: it is OFL and already bundled.
    Inter Display would suit the 24 and 32 px titles, but adding it means
    downloading a file, which waits on the owner.
21. **Artwork is the hero.** Covers in cards, rows, tables and page heroes wear
    a hairline (`Artwork::edged`) so a white sleeve keeps its edge on a light
    page and a black one on a dark page. The placeholder is a quiet tile with
    the kind's glyph at a third of its width. Hover shows the play button and
    nothing grows. Quick picks lost its bordered box and stands like a shelf,
    its rows reaching into the margin so their covers line up with the heading;
    shelf arrows are ghost buttons. Album, playlist and song pages wash the top
    of the page with the cover's own hue (`Theme::header_wash`: dim on dark,
    pale on light, so the theme's text keeps its contrast), fading to nothing
    at the hero's foot. A cover with no hue leaves the page neutral.
22. **Motion with a cause.** The page entrance stays as it was. The header wash
    fades in over the slow span when its colour arrives, keyed by its cover.
    Home's shelves surface one after another (`ui::Cascade`) when the feed
    replaces its skeletons; a shelf already shown never plays again, scrolled
    back or revisited. With motion reduced both appear at once, and loading
    skeletons hold still instead of pulsing.
23. **The sidebar from the keyboard.** Ctrl+1 to Ctrl+5 (Cmd on macOS) open
    Home, Songs, Albums, Artists and Playlists, and Ctrl+Alt+S (Ctrl+Cmd+S on
    macOS) shows or hides the sidebar. All six are rebindable under Settings,
    Keybinds. Buttons are not Tab stops anywhere in the app yet, the sidebar
    included.
24. **Empty and error copy.** States keep one action each and say what to do:
    an unset local library reads "Add a folder of music to start your library"
    over Choose folder, and a page that failed reads "This page didn't load"
    over the reason and Try again.
