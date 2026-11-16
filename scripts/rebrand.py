#!/usr/bin/env python3
"""Renames the upstream app's identity to Sonicly's across the tree.

Run from the repo root after merging upstream changes into `vendor/` or
porting files into `crates/`:  python scripts/rebrand.py

Rust sources are changed only inside string literals, so internal type names
shared with the vendored crates stay as upstream wrote them and `vendor/`
keeps merging cleanly. Every other text file is rewritten outright. Lines
that point at upstream's own hosts are left alone: those are removed or
re-pointed by hand (see docs/UPSTREAM.md), never silently renamed.
"""

import pathlib
import re
import sys

ROOTS = ["crates", "vendor", "assets", "scripts", ".cargo"]
SKIP_DIRS = {"gpui", "target", "__pycache__"}
SKIP_FILES = {"rebrand.py", "SONORA_REV"}
TEXT = {".ftl", ".js", ".py", ".iss", ".plist", ".desktop", ".toml", ".ps1", ".sh", ".yaml",
        ".yml", ".xml", ".json", ".svg", ".md", ".txt", ".wgsl", ".rc"}
# Upstream's servers, org, and maintainers: handled by hand.
KEEP = re.compile(r"sonorahq|nolight|sonora-stats", re.I)
PAIRS = [("Sonora", "Sonicly"), ("sonora", "sonicly"), ("SONORA", "SONICLY")]
# Normal and raw Rust string literals.
LITERAL = re.compile(r'r(#*)"[\s\S]*?"\1|"(?:[^"\\\n]|\\.)*"')


def rename(text: str) -> str:
    for old, new in PAIRS:
        text = text.replace(old, new)
    return text


def in_literals(source: str) -> str:
    return LITERAL.sub(lambda m: m.group(0) if KEEP.search(m.group(0)) else rename(m.group(0)), source)


def by_line(source: str) -> str:
    return "".join(line if KEEP.search(line) else rename(line) for line in source.splitlines(keepends=True))


def main() -> int:
    changed = 0
    paths = [p for root in ROOTS for p in pathlib.Path(root).rglob("*")]
    for path in paths:
        if not path.is_file() or path.name in SKIP_FILES or SKIP_DIRS & set(path.parts):
            continue
        if path.suffix == ".rs":
            convert = in_literals
        elif path.suffix in TEXT:
            convert = by_line
        else:
            continue
        before = path.read_text(encoding="utf-8")
        after = convert(before)
        if after != before:
            path.write_text(after, encoding="utf-8", newline="")
            changed += 1
    # Deepest first, so a renamed folder does not invalidate its children's paths.
    renamed = 0
    for path in sorted(paths, key=lambda p: len(p.parts), reverse=True):
        if path.exists() and "sonora" in path.name.lower() and not SKIP_DIRS & set(path.parts):
            if path.name not in SKIP_FILES:
                path.rename(path.with_name(rename(path.name)))
                renamed += 1
    print(f"rewrote {changed} files, renamed {renamed} paths")
    return 0


if __name__ == "__main__":
    sys.exit(main())
