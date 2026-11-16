#!/usr/bin/env python3

import json
import math
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MASTER = ROOT / "assets" / "icon.svg"

LINUX_SIZES = [16, 22, 24, 32, 48, 64, 128, 256, 512]
WINDOWS_SIZES = [16, 20, 24, 32, 40, 48, 64, 80, 96, 128, 256]
TRAY_SIZES = [18, 36]

ICNS_TYPES = [
    ("icp4", 16, 1),
    ("icp5", 32, 1),
    ("ic11", 16, 2),
    ("ic12", 32, 2),
    ("ic07", 128, 1),
    ("ic13", 128, 2),
    ("ic08", 256, 1),
    ("ic14", 256, 2),
    ("ic09", 512, 1),
    ("ic10", 512, 2),
]

SQUIRCLE_FILL = 824 / 1024
SQUIRCLE_RADIUS = 185.4 / 824
SQUIRCLE_SMOOTHING = 0.6
WINDOWS_RADIUS = 0.1875

# Hand-picked (bar, gap) widths in whole pixels for the sizes where scaling the master would blur
# the bars. Small sizes get heavier bars and a larger share of the canvas; see docs/BRAND.md.
HINTS = {
    16: (2, 1), 18: (2, 1), 20: (2, 1), 22: (2, 1), 24: (2, 1),
    32: (3, 1), 36: (4, 2), 40: (3, 2), 48: (4, 2), 64: (5, 3),
}
# A 16 px circle has no room for two-pixel bars, which would run into its edge.
ROUND_HINTS = {**HINTS, 16: (1, 1)}
# The macOS tile only covers SQUIRCLE_FILL of the canvas, so its glyph is smaller at each size.
SQUIRCLE_HINTS = {16: (1, 1), 32: (2, 1), 64: (4, 2)}

ICON_CANVAS = 1024
NAMED_COLORS = {"white": "#ffffff", "black": "#000000"}


def num(value):
    return f"{round(value, 4):g}"


class Master:
    """Reads the background and wave bars shared by the platform icons."""

    def __init__(self, path):
        if not path.exists():
            sys.exit(f"icons: {path} is missing")

        svg = ET.parse(path).getroot()
        ns = "{http://www.w3.org/2000/svg}"

        box = svg.get("viewBox")
        if box is None:
            sys.exit("icons: master has no viewBox")

        _, _, width, height = (float(n) for n in box.split())
        if width != height:
            sys.exit("icons: master viewBox is not square")
        self.span = width

        rect = svg.find(f"{ns}rect")
        self.background = rect.get("fill") if rect is not None else "white"

        self.bars = []
        for path_el in svg.iterfind(f"{ns}path"):
            match = re.fullmatch(
                r"M(-?[\d.]+) (-?[\d.]+)V(-?[\d.]+)", path_el.get("d").strip()
            )
            if not match:
                sys.exit(f"icons: unsupported path {path_el.get('d')!r}")
            self.bars.append(tuple(float(n) for n in match.groups()))
            self.foreground = path_el.get("stroke", "black")
            self.stroke = float(path_el.get("stroke-width", 2))
            self.cap = path_el.get("stroke-linecap", "round")

        if not self.bars:
            sys.exit("icons: master has no bars")

        self.bars.sort()
        half = self.stroke / 2
        self.left = self.bars[0][0] - half
        self.top = min(y0 for _, y0, _ in self.bars) - half
        self.width = self.bars[-1][0] + half - self.left
        self.height = max(y1 for _, _, y1 in self.bars) + half - self.top
        self.share = self.width / self.span

    def glyph(self, span, share, pixels=None, hints=HINTS, color=None):
        """Draws the bars `share` of `span` wide and centered, on whole pixels where hinted."""
        color = color or self.foreground
        if pixels in hints:
            return self.hinted(span, pixels, *hints[pixels], color)

        k = span * share / self.width
        cx = span / 2 - (self.left + self.width / 2) * k
        cy = span / 2 - (self.top + self.height / 2) * k
        return "\n".join(
            f'  <path d="M{num(cx + x * k)} {num(cy + y0 * k)}V{num(cy + y1 * k)}"'
            f' stroke="{color}" stroke-width="{num(self.stroke * k)}"'
            f' stroke-linecap="{self.cap}"/>'
            for x, y0, y1 in self.bars
        )

    def hinted(self, span, pixels, bar, gap, color):
        """Redraws the bars as whole-pixel rectangles of the given bar and gap widths."""
        count = len(self.bars)
        width = count * bar + (count - 1) * gap
        scale = width / self.width
        x0 = (pixels - width) // 2
        y0 = (pixels - round(self.height * scale)) // 2
        unit = span / pixels
        # Two-pixel bars stay square-ended, since a rounded end that small only blurs.
        radius = bar / 2 if bar >= 3 else 0
        rects = []
        for i, (_, start, end) in enumerate(self.bars):
            top = round((start - self.stroke / 2 - self.top) * scale)
            bottom = round((end + self.stroke / 2 - self.top) * scale)
            assert bottom - top >= bar, f"icons: bar {i} collapses at {pixels}px"
            assert 0 <= x0 and 0 <= y0 + top and y0 + bottom <= pixels, "icons: glyph overflows"
            rects.append(
                f'  <rect x="{num((x0 + i * (bar + gap)) * unit)}" y="{num((y0 + top) * unit)}"'
                f' width="{num(bar * unit)}" height="{num((bottom - top) * unit)}"'
                f' rx="{num(radius * unit)}" fill="{color}"/>'
            )
        return "\n".join(rects)


def squircle(side, radius, smoothing):
    """Builds a square with smooth corners, keeping each corner within half its side."""
    budget = side / 2
    reach = (1 + smoothing) * radius
    if reach > budget:
        smoothing = budget / radius - 1
        reach = budget

    arc_measure = 90 * (1 - smoothing)
    arc = math.sin(math.radians(arc_measure / 2)) * radius * math.sqrt(2)
    alpha = (90 - arc_measure) / 2
    beta = 45 * smoothing
    span = radius * math.tan(math.radians(alpha / 2))
    c = span * math.cos(math.radians(beta))
    d = c * math.tan(math.radians(beta))
    b = (reach - arc - c - d) / 3
    a = 2 * b
    ab = a + b
    long = a + b + c
    mid = b + c

    def n(value):
        return f"{value:.4f}"

    return " ".join(
        [
            f"M{n(side - reach)} 0",
            f"c{n(a)} 0 {n(ab)} 0 {n(long)} {n(d)}",
            f"a{n(radius)} {n(radius)} 0 0 1 {n(arc)} {n(arc)}",
            f"c{n(d)} {n(c)} {n(d)} {n(mid)} {n(d)} {n(long)}",
            f"L{n(side)} {n(side - reach)}",
            f"c0 {n(a)} 0 {n(ab)} {n(-d)} {n(long)}",
            f"a{n(radius)} {n(radius)} 0 0 1 {n(-arc)} {n(arc)}",
            f"c{n(-c)} {n(d)} {n(-mid)} {n(d)} {n(-long)} {n(d)}",
            f"L{n(reach)} {n(side)}",
            f"c{n(-a)} 0 {n(-ab)} 0 {n(-long)} {n(-d)}",
            f"a{n(radius)} {n(radius)} 0 0 1 {n(-arc)} {n(-arc)}",
            f"c{n(-d)} {n(-c)} {n(-d)} {n(-mid)} {n(-d)} {n(-long)}",
            f"L0 {n(reach)}",
            f"c0 {n(-a)} 0 {n(-ab)} {n(d)} {n(-long)}",
            f"a{n(radius)} {n(radius)} 0 0 1 {n(arc)} {n(-arc)}",
            f"c{n(c)} {n(-d)} {n(mid)} {n(-d)} {n(long)} {n(-d)}",
            "Z",
        ]
    )


def round_svg(master, pixels=None):
    half = master.span / 2
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{master.span:g}"'
        f' height="{master.span:g}" viewBox="0 0 {master.span:g} {master.span:g}">\n'
        f'  <circle cx="{half:g}" cy="{half:g}" r="{half:g}"'
        f' fill="{master.background}"/>\n'
        f"{master.glyph(master.span, master.share, pixels, ROUND_HINTS)}\n"
        f"</svg>\n"
    )


def squircle_svg(master, pixels):
    """Draws the macOS icon with its Dock margin, hinted for the pixel size it renders at."""
    canvas = 1024.0
    side = canvas * SQUIRCLE_FILL
    inset = (canvas - side) / 2
    path = squircle(side, side * SQUIRCLE_RADIUS, SQUIRCLE_SMOOTHING)
    glyph = master.glyph(canvas, master.share * SQUIRCLE_FILL, pixels, SQUIRCLE_HINTS)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{canvas:g}"'
        f' height="{canvas:g}" viewBox="0 0 {canvas:g} {canvas:g}">\n'
        f'  <path transform="translate({inset:g} {inset:g})" d="{path}"'
        f' fill="{master.background}"/>\n'
        f"{glyph}\n"
        f"</svg>\n"
    )


def readme_svg(master):
    """Draws the squircle edge to edge in the master's own viewBox, without the macOS margin."""
    path = squircle(master.span, master.span * SQUIRCLE_RADIUS, SQUIRCLE_SMOOTHING)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{master.span:g}"'
        f' height="{master.span:g}" viewBox="0 0 {master.span:g} {master.span:g}">\n'
        f'  <path d="{path}" fill="{master.background}"/>\n'
        f"{master.glyph(master.span, master.share)}\n"
        f"</svg>\n"
    )


def template_svg(master, pixels):
    """Draws the bars alone in black, the menu bar tints a template image itself."""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{master.span:g}"'
        f' height="{master.span:g}" viewBox="0 0 {master.span:g} {master.span:g}">\n'
        f"{master.glyph(master.span, master.share, pixels, color='black')}\n"
        f"</svg>\n"
    )


def rounded_svg(master, pixels=None):
    radius = master.span * WINDOWS_RADIUS
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{master.span:g}"'
        f' height="{master.span:g}" viewBox="0 0 {master.span:g} {master.span:g}">\n'
        f'  <rect width="{master.span:g}" height="{master.span:g}"'
        f' rx="{radius:g}" ry="{radius:g}" fill="{master.background}"/>\n'
        f"{master.glyph(master.span, master.share, pixels)}\n"
        f"</svg>\n"
    )


def layer_svg(master):
    """Draws the bars alone on the Icon Composer canvas, leaving the background to icon.json."""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{ICON_CANVAS}"'
        f' height="{ICON_CANVAS}" viewBox="0 0 {master.span:g} {master.span:g}">\n'
        f"{master.glyph(master.span, master.share)}\n"
        f"</svg>\n"
    )


def srgb(color):
    """Converts a master color to the opaque sRGB string icon.json expects."""
    hex_color = NAMED_COLORS.get(color, color).lstrip("#")
    if not re.fullmatch(r"[0-9a-fA-F]{6}", hex_color):
        sys.exit(f"icons: unsupported color {color!r}")
    channels = ",".join(
        f"{int(hex_color[i:i + 2], 16) / 255:.5f}" for i in (0, 2, 4)
    )
    return f"srgb:{channels},1.00000"


def write_icon_bundle(master, path):
    """Writes the Icon Composer bundle macOS 26 derives its dark, clear and tinted icons from.

    Returns whether anything changed, since actool never compiles the same Assets.car twice.
    """
    background = {"solid": srgb(master.background)}
    glyph = {"solid": srgb(master.foreground)}
    # actool swaps any dark background for its own system-dark, so only the glyph varies.
    manifest = {
        "fill": background,
        "groups": [
            {
                "layers": [
                    {
                        "image-name": "glyph.svg",
                        "name": "glyph",
                        # Clear and tinted both draw the "tinted" variant, lit by luminance,
                        # so the bars stay the light color there too.
                        "fill-specializations": [
                            {"appearance": appearance, "value": glyph}
                            for appearance in ("dark", "tinted")
                        ],
                    }
                ],
                # The brand is flat: a faint shadow and opaque bars, not frosted glass.
                "shadow": {"kind": "neutral", "opacity": 0.25},
                "translucency": {"enabled": False, "value": 0.5},
            }
        ],
        "supported-platforms": {"squares": ["macOS"]},
    }
    files = {
        path / "icon.json": json.dumps(manifest, indent=2) + "\n",
        path / "Assets" / "glyph.svg": layer_svg(master),
    }
    if all(
        file.exists() and file.read_text(encoding="utf-8") == text
        for file, text in files.items()
    ):
        return False

    shutil.rmtree(path, ignore_errors=True)
    (path / "Assets").mkdir(parents=True)
    for file, text in files.items():
        file.write_text(text, encoding="utf-8", newline="\n")
    return True


def compile_icon_bundle(bundle, workdir, target):
    """Compiles the bundle with actool, which needs macOS and Xcode 26 or later."""
    out = workdir / "actool"
    out.mkdir()
    subprocess.run(
        [
            "xcrun", "actool", str(bundle),
            "--compile", str(out),
            "--app-icon", bundle.stem,
            "--include-all-app-icons",
            "--platform", "macosx",
            "--minimum-deployment-target", "11.0",
            "--output-partial-info-plist", str(out / "partial.plist"),
            "--errors", "--warnings",
            "--output-format", "human-readable-text",
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    shutil.copyfile(out / "Assets.car", target)


def rasterize(svg_text, pixels, workdir, name):
    """Renders directly from vector artwork onto a transparent pixel canvas."""
    source = workdir / f"{name}.svg"
    target = workdir / f"{name}.png"
    source.write_text(svg_text, encoding="utf-8", newline="\n")
    if shutil.which("rsvg-convert"):
        command = [
            "rsvg-convert",
            "--width", str(pixels),
            "--height", str(pixels),
            "--keep-aspect-ratio",
            "--background-color", "none",
            "--output", str(target),
            str(source),
        ]
    else:
        command = [
            "resvg", "--width", str(pixels), "--height", str(pixels), str(source), str(target)
        ]
    subprocess.run(command, check=True)
    return target.read_bytes()


def write_ico(entries, path):
    """Packs RGBA PNGs into a Windows icon family with 32-bit directory entries."""
    header = struct.pack("<HHH", 0, 1, len(entries))
    offset = len(header) + 16 * len(entries)
    directory = b""
    payload = b""
    for pixels, blob in entries:
        directory += struct.pack(
            "<BBBBHHII",
            0 if pixels >= 256 else pixels,
            0 if pixels >= 256 else pixels,
            0,
            0,
            1,
            32,
            len(blob),
            offset,
        )
        payload += blob
        offset += len(blob)
    path.write_bytes(header + directory + payload)


def write_icns(entries, path):
    """Packs standard and Retina PNG representations into a macOS icon family."""
    body = b""
    for kind, blob in entries:
        body += kind.encode("ascii") + struct.pack(">I", len(blob) + 8) + blob
    path.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main():
    if not (shutil.which("rsvg-convert") or shutil.which("resvg")):
        sys.exit(
            "icons: no SVG renderer, run under `nix shell nixpkgs#librsvg`"
            " or `cargo install resvg`"
        )

    master = Master(MASTER)
    assets = ROOT / "assets"

    with tempfile.TemporaryDirectory() as tmp:
        workdir = Path(tmp)

        scalable = assets / "linux" / "sonicly.svg"
        scalable.parent.mkdir(parents=True, exist_ok=True)
        scalable.write_text(round_svg(master), encoding="utf-8", newline="\n")

        (ROOT / ".github").mkdir(exist_ok=True)
        (ROOT / ".github" / "readme-icon.svg").write_text(
            readme_svg(master), encoding="utf-8", newline="\n"
        )
        (assets / "macos").mkdir(parents=True, exist_ok=True)
        (assets / "macos" / "sonicly.svg").write_text(
            readme_svg(master), encoding="utf-8", newline="\n"
        )
        (assets / "windows").mkdir(parents=True, exist_ok=True)
        (assets / "windows" / "sonicly.svg").write_text(
            rounded_svg(master), encoding="utf-8", newline="\n"
        )

        for pixels in LINUX_SIZES:
            blob = rasterize(round_svg(master, pixels), pixels, workdir, f"l{pixels}")
            target = (
                assets
                / "linux"
                / "icons"
                / "hicolor"
                / f"{pixels}x{pixels}"
                / "apps"
                / "sonicly.png"
            )
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(blob)

        icns = []
        for kind, points, scale in ICNS_TYPES:
            pixels = points * scale
            blob = rasterize(squircle_svg(master, pixels), pixels, workdir, kind)
            icns.append((kind, blob))
        write_icns(icns, assets / "macos" / "sonicly.icns")

        ico = [
            (
                pixels,
                rasterize(rounded_svg(master, pixels), pixels, workdir, f"w{pixels}"),
            )
            for pixels in WINDOWS_SIZES
        ]
        windows = assets / "windows"
        windows.mkdir(parents=True, exist_ok=True)
        write_ico(ico, windows / "sonicly.ico")

        tray = assets / "tray"
        tray.mkdir(parents=True, exist_ok=True)
        for pixels in TRAY_SIZES:
            blob = rasterize(template_svg(master, pixels), pixels, workdir, f"t{pixels}")
            (tray / f"template-{pixels}.png").write_bytes(blob)
        for obsolete in ["template-32.png", "template-64.png", "sonicly.png"]:
            (tray / obsolete).unlink(missing_ok=True)

        # Last, so an actool failure leaves every other icon already written.
        bundle = assets / "macos" / "sonicly.icon"
        car = assets / "macos" / "Assets.car"
        compiled = False
        if write_icon_bundle(master, bundle) or not car.exists():
            # A catalog compiled from an older bundle would ship the old artwork.
            car.unlink(missing_ok=True)
            if sys.platform == "darwin":
                compile_icon_bundle(bundle, workdir, car)
                compiled = True
            else:
                print("icons: actool needs macOS, rerun there to compile Assets.car")

    print(
        f"icons: {len(LINUX_SIZES)} linux png, 1 linux svg, 1 icns, 1 ico,"
        f" {len(TRAY_SIZES)} tray png, 1 readme svg, 1 macos svg, 1 windows svg,"
        f" 1 icon bundle{', 1 car' if compiled else ''}"
    )


if __name__ == "__main__":
    main()
