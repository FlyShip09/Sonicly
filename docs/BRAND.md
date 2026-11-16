# Brand

Sonicly's interface takes its color from the cover that is playing, so the brand
itself stays neutral: one mark, two colors, no gradient, glow or glass.

## The mark

Five rounded bars sampled from one period of a sine wave. The wave rises at the
second bar, crosses the middle at the third and bottoms out at the fourth; the
outer bars turn back toward the middle like the ends of an S. The shape has
180° rotational symmetry, the same as the letter.

Geometry on a 14 × 14 grid (1 unit = 1/14 of the mark's height). Every bar is
2 units wide with round ends (radius 1); bars sit 1 unit apart.

| Bar | x  | top | bottom | height |
| --- | -- | --- | ------ | ------ |
| 1   | 0  | 3   | 8      | 5      |
| 2   | 3  | 0   | 8      | 8      |
| 3   | 6  | 2   | 12     | 10     |
| 4   | 9  | 6   | 14     | 8      |
| 5   | 12 | 6   | 11     | 5      |

The same in a normalized 0–1 box, for animation (left edge, width, top,
height, vertical center):

| Bar | left   | width  | top    | height | center |
| --- | ------ | ------ | ------ | ------ | ------ |
| 1   | 0      | 0.1429 | 0.2143 | 0.3571 | 0.3929 |
| 2   | 0.2143 | 0.1429 | 0      | 0.5714 | 0.2857 |
| 3   | 0.4286 | 0.1429 | 0.1429 | 0.7143 | 0.5    |
| 4   | 0.6429 | 0.1429 | 0.4286 | 0.5714 | 0.7143 |
| 5   | 0.8571 | 0.1429 | 0.4286 | 0.3571 | 0.6071 |

Bars 1 and 2 share a bottom edge, bars 4 and 5 share a top edge. Animating each
bar's center and height around these values keeps the mark recognizable; keep
the rotational symmetry at rest.

`assets/icon.svg` is the master: the mark on a 24 × 24 canvas at 5–19 on both
axes (58.3% of the canvas), drawn as stroked lines with round caps.
`assets/icons/common/sonicly.svg` is the same geometry in `currentColor` for
use inside the app.

## Color

| Name     | Value     | Use                                      |
| -------- | --------- | ---------------------------------------- |
| Graphite | `#1C1B1A` | Icon tile, the mark on light grounds     |
| Paper    | `#F3F0E8` | The mark on the tile and on dark grounds |

Both are warm grays, a few values off neutral, so the icon reads as an object
next to the app's neutral grays (`#0a0a0a`, `#fafafa`). Contrast between them
is 15.1:1. There is no signature color: a fixed accent would compete with the
album art the interface is built around.

## Clear space and minimum size

- Keep empty space of at least a quarter of the mark's height (3.5 units) on
  every side of the bare mark and of the wordmark.
- The bare mark goes down to 14 px tall (2 px bars, 1 px gaps). Below that,
  use the app icon, which is hand-hinted to 16 px.
- Use the wordmark where the mark is at least 20 px tall; below that, set the
  mark alone.

## Wordmark

`assets/brand/sonicly-wordmark.svg` (Graphite, for light grounds) and
`assets/brand/sonicly-wordmark-inverse.svg` (Paper, for dark grounds). The name
is Inter SemiBold from `assets/fonts` (SIL OFL 1.1, see
`assets/fonts/LICENSE.txt`), converted to outlines so it needs no installed
font, tracked −0.01 em. The mark is 1.4 × the cap height, centered on it, with
a gap of 0.4 × the mark's height before the S. The files are static; rebuild
them by hand from these numbers if the mark changes.

## Platform icons

`scripts/generate-icons.py` generates all of them from `assets/icon.svg`.

- **Windows** (`assets/windows/sonicly.ico`, `sonicly.svg`): a full-bleed
  rounded square, corner radius 18.75%. The `.ico` holds 16, 20, 24, 32, 40,
  48, 64, 80, 96, 128 and 256 px; the 16–32 entries also serve as the tray
  icon. On a dark taskbar the tile merges with the bar and the Paper bars carry
  the icon; on a light one the tile reads as a dark object.
- **macOS** (`assets/macos/sonicly.icns`, `sonicly.svg`, `sonicly.icon`): the
  Big Sur grid, a smooth-cornered tile 824/1024 of the canvas, with the mark at
  58.3% of the tile. The Icon Composer bundle (`sonicly.icon`) keeps the bars
  opaque (translucency off, faint neutral shadow) so macOS 26 renders it flat.
- **Linux** (`assets/linux/icons/hicolor/*/apps/sonicly.png`, `sonicly.svg`):
  a full circle in the hicolor sizes 16, 22, 24, 32, 48, 64, 128, 256 and 512
  px, plus the scalable SVG.
- **Menu bar** (`assets/tray/template-18.png`, `template-36.png`): macOS
  template images, black bars on transparent, which the system tints.

The script draws small sizes on whole pixels instead of scaling them, with bar
and gap widths picked per size (`HINTS`, `ROUND_HINTS`, `SQUIRCLE_HINTS` in the
script):

| Pixels | Bar | Gap | Notes |
| ------ | --- | --- | ----- |
| 16     | 2   | 1   | Mark fills 14 of 16 px, square ends. Circle and macOS tile: 1 px bars |
| 18–24  | 2   | 1   | Square ends; at 24 px this is the master |
| 32     | 3   | 1   | Heavier bars; macOS tile: 2 px bars |
| 36     | 4   | 2   | Menu bar @2x |
| 40     | 3   | 2   | |
| 48     | 4   | 2   | |
| 64     | 5   | 3   | macOS tile: 4 px bars, 2 px gaps |

Larger sizes scale the master.

## Regenerating

```sh
python scripts/generate-icons.py
```

It needs `rsvg-convert` (`nix shell nixpkgs#librsvg`) or `resvg`
(`cargo install resvg`). On macOS with Xcode 26 it also compiles
`assets/macos/Assets.car` from `sonicly.icon` with `actool`; elsewhere it
writes everything else and deletes `Assets.car` whenever the bundle changes,
so a catalog of older artwork never ships.

## Pending on macOS

`assets/macos/Assets.car` is not in the tree: it held the previous project's
artwork, and only `actool` on a Mac can compile it. Until someone runs the
script there, macOS 26 has no adaptive (dark, clear, tinted) icon and should
fall back to `sonicly.icns` through `CFBundleIconFile` (not verified). Every other macOS file,
including `sonicly.icns` and the `sonicly.icon` bundle, is already the new
artwork.
