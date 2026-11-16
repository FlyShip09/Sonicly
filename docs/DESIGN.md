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
25. **A compact floating player.** A second window, borderless and always on top,
    in two shapes: a card (the cover over a panel with title, artist, previous,
    play or pause, next, the fold button and the way back) and a bar one list row
    tall. It opens from a button beside Now playing in the player bar (minimize
    beside maximize: a smaller and a larger player), from Ctrl+Shift+M
    (Cmd+Shift+M on macOS, rebindable under Keybinds), from the tray menu, and
    from a command row the quick search lists when the query names it. Asking
    again focuses the open one; the shortcut pressed inside it goes back. Opening
    it minimizes the main window, and going back or closing it restores the main
    window first, so the app never runs out of windows and quits. One surface is
    frosted: the cover is drawn again under the whole window and the panel blurs
    it (popover colour at 72% over a 28 px blur), so the panel takes the cover's
    colour without a palette pass. The window stays opaque and its buttons are
    plain ghosts, so there is never glass on glass; with blurs off the panel is a
    92% flat fill. Nothing in it moves on its own: the progress line redraws only
    when it advances a device pixel, from the position tick, so a paused player
    draws nothing and a playing one never redraws the main window. Folding is
    instant, since a window resize cannot be animated evenly on every platform.
    Position and fold are saved without waking settings observers, so dragging it
    never redraws the main window either. A saved position is used while its
    centre lands on a connected display; the first open tucks it into the bottom
    right corner of the main window's display. Two designed sizes rather than a
    free one: the sizes in between would need a third layout, and the fork gives
    a Windows pop-up no resize frame. Per platform, from the gpui fork at
    `8809748`: Windows uses `WindowKind::PopUp` (`WS_EX_TOPMOST |
    WS_EX_TOOLWINDOW`, so no taskbar button and no Alt+Tab entry) with DWM
    rounding. macOS uses `PopUp`, a non-activating panel at the pop-up level that
    joins every Space and floats over full-screen apps, told here not to hide
    while Sonicly is in the background and with its title bar buttons hidden.
    Linux uses a normal window with client decorations, because `PopUp` there is
    an override-redirect notification no window manager can move, and on X11 asks
    for `_NET_WM_STATE_ABOVE` and `_NET_WM_STATE_SKIP_TASKBAR`. Wayland offers no
    way to stay on top.
26. **Files dragged onto the window go into the library.** While files from the
    system hover, the page between the sidebars gives way to an opaque sheet with
    a dashed edge and one message: how many songs the drag holds and where they
    go ("Add 12 songs to Local Music"), or, in the danger colour, that nothing in
    it plays, with the formats that do. The count is real: the dragged paths are
    walked off the main thread with the local provider's own list of audio
    extensions, folders included, links to folders not followed, capped at
    50,000. The sidebars and both bars stay as they are, because a local playlist
    in the sidebar and the queue take drops of their own, which win over the
    window's. Songs a music folder already holds only need a rescan. Songs from
    elsewhere ask once where they go: their folder joins the music folders as it
    is (named, with its path under it), or they are copied into one of the music
    folders. The answer is kept in `settings.json` (`dropped_files`) and changed
    under Settings, Music folders. A copy target that has left the folder list
    asks again, and with no music folder yet there is nothing to ask, so their
    folder is added. A copy keeps a dropped folder's shape under its own name,
    puts loose files at the top, takes a same-size file already there for the
    same song, numbers any other clash and creates every file with `create_new`,
    so nothing is overwritten. Originals are never moved, renamed or deleted, and
    a whole drive is never added as a folder. A drop on a local playlist brings
    the songs into the library the same way, then adds them to the playlist,
    since a local playlist lists only songs the library holds.
27. **Favorites, Local Music and Recents get drawn covers.** These change with
    every like, scan and play, so the mosaic image on disk that playlists get
    (`state::mosaic`) would be rebuilt constantly. Their covers are drawn instead
    (`ui::GeneratedCover`) from the colours of their first four covers, which
    `mosaic::Swatches` samples with the same extraction the adaptive theme uses,
    and samples again only when those covers change: a field in the first cover's
    colour with the collection's glyph in the top left corner, and along its foot
    a row of up to three square-edged tiles in the next covers' colours.
    Saturation is capped at 0.42 and lightness pulled into a band per theme (0.20
    to 0.36 dark, 0.70 to 0.84 light), so the theme's own foreground always reads
    on it and it sits quietly beside real artwork; each tile steps its lightness
    so neighbours of one hue still part. Below 64 px the foot row goes and the
    glyph centres at half the tile, which still reads in a 32 px row. A
    collection with no covers is a neutral tile, and the same first covers always
    draw the same cover. Tried and dropped: rounded bands stacked like spines,
    which read as skeleton text, and a typeset initial, which says nothing about
    what is inside. The Recents hero uses it, and the Songs hero for Favorites
    and, before any account, for the local files alone, then titled Local Music.
    A pinned Favorites row in the sidebar keeps its glyph for now.
