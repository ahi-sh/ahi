#!/usr/bin/env python3
"""Generate font-independent GitHub avatar assets from the TUI wordmark.

Run from any directory: python3 scripts/generate-avatar.py
Uses only the Python standard library. Does not upload anything.
"""
import re
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIZE = 512
BACKGROUND = (13, 17, 23)  # Chosen dark canvas; TUI background is terminal-owned.


def main():
    header = (ROOT / "src/chat/header.rs").read_text()
    theme = (ROOT / "src/chat/transcript/theme.rs").read_text()
    match = re.search(r'const WORDMARK:.*?= \["(.*?)", "(.*?)"\];', header)
    if match is None:
        raise ValueError("Cannot find the two-row TUI wordmark")
    rows = match.groups()
    color = re.search(r'const ACCENT: Color = Color::Rgb\((\d+), (\d+), (\d+)\)', theme)
    if color is None:
        raise ValueError("Cannot find TUI accent RGB")
    accent = tuple(map(int, color.groups()))
    cell_w, cell_h = 40, 80
    x0 = (SIZE - max(map(len, rows)) * cell_w) // 2
    y0 = (SIZE - len(rows) * cell_h) // 2
    rectangles = []
    for row, text in enumerate(rows):
        for col, glyph in enumerate(text):
            x, y = x0 + col * cell_w, y0 + row * cell_h
            if glyph == "█":
                rectangles.append((x, y, cell_w, cell_h))
            elif glyph == "▀":
                rectangles.append((x, y, cell_w, cell_h // 2))
            elif glyph == "▄":
                rectangles.append((x, y + cell_h // 2, cell_w, cell_h // 2))
            elif glyph == "░":
                # Sparse quarter-density stipple, like a terminal shade glyph.
                for dy in range(0, cell_h, 8):
                    for dx in range(0, cell_w, 8):
                        rectangles.append((x + dx, y + dy, 4, 4))
            elif glyph != " ":
                raise ValueError(f"Unsupported wordmark glyph: {glyph}")
    pixels = bytearray(bytes(BACKGROUND) * SIZE * SIZE)
    for x, y, w, h in rectangles:
        assert 0 <= x < x + w <= SIZE and 0 <= y < y + h <= SIZE
        for yy in range(y, y + h):
            start = (yy * SIZE + x) * 3
            pixels[start:start + w * 3] = bytes(accent) * w
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    scanlines = b"".join(b"\0" + pixels[y * SIZE * 3:(y + 1) * SIZE * 3] for y in range(SIZE))
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(scanlines, 9)) + chunk(b"IEND", b"")
    hexcolor = lambda rgb: "#" + "".join(f"{c:02x}" for c in rgb)
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512" role="img" aria-labelledby="title">',
           '<title id="title">ahi terminal wordmark</title>',
           f'<rect width="512" height="512" fill="{hexcolor(BACKGROUND)}"/>',
           f'<g fill="{hexcolor(accent)}">']
    svg.extend(f'<rect x="{x}" y="{y}" width="{w}" height="{h}"/>' for x, y, w, h in rectangles)
    svg.extend(['</g>', '</svg>'])
    out = ROOT / "assets/branding"
    out.mkdir(parents=True, exist_ok=True)
    (out / "ahi-avatar.png").write_bytes(png)
    (out / "ahi-avatar.svg").write_text("\n".join(svg) + "\n")
    print(f"Generated {out}/ahi-avatar.{{png,svg}} (512 × 512)")


if __name__ == "__main__":
    main()
