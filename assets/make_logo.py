"""Generate assets/logo.png: the LZ80N pixel logo for the README.

Big letters: chunky 2-pixel-stroke block glyphs, slanted like italics.
Spectrum-style rainbow stripes run through them; a soft glow behind.
Tagline: the compiler's own 8x8 text font (compiler/src/font.rs).

    python assets/make_logo.py
"""
import re
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
W, H = 1600, 500
BG = (10, 12, 20)
CELL, BLOCK = 28, 26            # grid step and drawn block size
SHEAR = 0.22                    # blocks of shift per row (italic)
STRIPES = [(255, 45, 45), (255, 213, 0), (34, 221, 34), (0, 200, 255)]
SLOPE = -0.075                   # stripes rise to the right

GLYPHS = {
    "L": ["##....", "##....", "##....", "##....", "##....",
          "##....", "##....", "######", "######"],
    "Z": ["#######", "#######", "....##.", "...##..", "..##...",
          ".##....", "##.....", "#######", "#######"],
    "8": [".#####.", "##...##", "##...##", ".##.##.", ".#####.",
          "##...##", "##...##", "##...##", ".#####."],
    "0": [".#####.", "##...##", "##..###", "##.#.##", "###..##",
          "##...##", "##...##", "##...##", ".#####."],
    "N": ["##...##", "###..##", "####.##", "##.####", "##..###",
          "##...##", "##...##", "##...##", "##...##"],
}


def small_font():
    """Glyphs of the compiler's text font: char -> list of 5-wide rows."""
    src = (ROOT / "compiler/src/font.rs").read_text(encoding="utf-8")
    rows = re.findall(r'/\* .{1,3} \*/ "([.# ]+)"', src)
    return {chr(32 + i): r.split() for i, r in enumerate(rows)}


def draw_text_small(img, text, x, y, scale, colour):
    font = small_font()
    d = ImageDraw.Draw(img)
    for ch in text:
        for r, row in enumerate(font.get(ch, font[" "])):
            for c, p in enumerate(row):
                if p == "#":
                    d.rectangle([x + c * scale, y + r * scale,
                                 x + (c + 1) * scale - 1, y + (r + 1) * scale - 1], fill=colour)
        x += 6 * scale


def text_width_small(text, scale):
    return len(text) * 6 * scale - scale


def main():
    word = "LZ80N"
    cols = sum(len(GLYPHS[c][0]) for c in word) + 2 * (len(word) - 1)
    rows = 9
    text_w = cols * CELL + int(SHEAR * rows * CELL)
    x0 = (W - text_w) // 2 - 70
    y0 = 56
    base = y0 + 5.0 * CELL          # top of the stripe band at x0

    def stripe(cx, cy):
        """index of the rainbow stripe at a point, or None"""
        k = int((cy - (base + SLOPE * (cx - x0))) // CELL)
        return k if 0 <= k < len(STRIPES) else None

    blocks = []                     # (x, y, colour)
    gx = 0
    right = 0
    for ch in word:
        g = GLYPHS[ch]
        for r, row in enumerate(g):
            shift = SHEAR * (rows - 1 - r) * CELL
            for c, p in enumerate(row):
                if p == "#":
                    x = x0 + (gx + c) * CELL + shift
                    y = y0 + r * CELL
                    k = stripe(x + BLOCK / 2, y + BLOCK / 2)
                    blocks.append((x, y, STRIPES[k] if k is not None else (255, 255, 255)))
                    right = max(right, x + CELL)
        gx += len(g[0]) + 2

    # the stripes leave the logo to the right, on the same grid
    x = right + CELL
    while x < W:
        for r in range(rows):
            y = y0 + r * CELL
            k = stripe(x + BLOCK / 2, y + BLOCK / 2)
            if k is not None:
                blocks.append((x, y, STRIPES[k]))
        x += CELL

    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    for gy in range(16, H, 32):     # faint dot grid
        for gxp in range(16, W, 32):
            d.rectangle([gxp, gy, gxp + 1, gy + 1], fill=(26, 30, 44))

    layer = Image.new("RGB", (W, H), BG)
    ld = ImageDraw.Draw(layer)
    for x, y, col in blocks:
        ld.rectangle([x, y, x + BLOCK - 1, y + BLOCK - 1], fill=col)
    img = Image.blend(img, layer.filter(ImageFilter.GaussianBlur(20)), 0.6)
    d = ImageDraw.Draw(img)
    for x, y, col in blocks:
        d.rectangle([x, y, x + BLOCK - 1, y + BLOCK - 1], fill=col)

    t1 = "The simplicity of Lua. The power of Z80N."
    draw_text_small(img, t1, (W - text_width_small(t1, 4)) // 2, y0 + rows * CELL + 50, 4, (235, 238, 245))
    t2 = "A compiled language for the ZX Spectrum Next"
    draw_text_small(img, t2, (W - text_width_small(t2, 3)) // 2, y0 + rows * CELL + 108, 3, (110, 195, 255))

    img.save(ROOT / "assets/logo.png", optimize=True)
    print("wrote assets/logo.png", img.size)


if __name__ == "__main__":
    main()
