"""Sprites for examples/shooter.lz80n.

* The player comes from examples/assets/space-ship.png (a blurry, upscaled
  picture of ships on a gradient): the first ship of the first row is cut
  out by colour, shrunk to 32x32 (four 16x16 patterns) and given three
  engine flames of different lengths, drawn here.
* The five enemy ships, their animation frames, the missile, the enemy
  bullet and the explosion are drawn here as pixel art. Enemies are 16x16
  and shown at double size by the sprite hardware.

Output: examples/assets/ships.png, 128 x 64, patterns numbered left to
right, top to bottom (8 per row):
  0..11   player, 3 frames x 4 patterns (top-left, top-right, bottom-left,
          bottom-right)
  12..21  enemies: kind k, frame f at 12 + 2k + f
          (0 crab, 1 wasp, 2 arrow, 3 orb, 4 bomber)
  22..23  missile, 2 frames
  24..27  explosion, 4 frames
  28..29  enemy bullet, 2 frames
Run from the repository root.
"""
import math
from PIL import Image

SRC = "examples/assets/space-ship.png"
OUT = "examples/assets/ships.png"
COLS = [190, 385, 580, 780, 975, 1170, 1370]
ROWS = [140, 375, 605, 815, 965]

PALETTE = {
    "white": (240, 240, 240),
    "grey": (160, 168, 190),
    "blue": (50, 60, 220),
    "navy": (20, 22, 70),
    "orange": (240, 110, 20),
    "yellow": (255, 210, 70),
}

# ------------------------------------------------------------------ player


def classify(p):
    r, g, b = p[:3]
    if r > 215 and g > 170 and b < 150:
        return "yellow"
    if r > 200 and r - b > 90:
        return "orange"
    if min(r, g, b) > 195:
        return "white"
    if min(r, g, b) > 150 and b >= r - 10:
        return "grey"
    if b - r > 45 and b > 110:
        return "blue"
    if r + g + b < 210 and b >= r - 5:
        return "navy"
    return None


def player_hull(im):
    """The first ship (no flame) shrunk into the top of a 32x32 image."""
    x0, y0, x1, y1 = COLS[0], ROWS[0], COLS[1], ROWS[1]
    xs, ys = [], []
    for y in range(y0, y1):
        for x in range(x0, x1):
            c = classify(im.getpixel((x, y)))
            if c and c not in ("orange", "yellow"):
                xs.append(x)
                ys.append(y)
    bx0, by0, bx1, by1 = min(xs), min(ys), max(xs) + 1, max(ys) + 1
    # the hull fills 26 of the 32 rows; the flame goes below it
    side = max(bx1 - bx0, (by1 - by0) * 32 / 26)
    cx = (bx0 + bx1) / 2
    step = side / 32
    out = Image.new("RGBA", (32, 32), (0, 0, 0, 0))
    for ty in range(32):
        for tx in range(32):
            sx0 = cx - side / 2 + tx * step
            sy0 = by0 + ty * step
            votes, tot = {}, 0
            for yy in range(int(sy0), int(sy0 + step) + 1):
                for xx in range(int(sx0), int(sx0 + step) + 1):
                    if not (x0 <= xx < x1 and y0 <= yy < by1):
                        continue
                    tot += 1
                    c = classify(im.getpixel((xx, yy)))
                    if c:
                        votes[c] = votes.get(c, 0) + 1
            n = sum(votes.values())
            if tot and n / tot >= 0.45:
                out.putpixel((tx, ty), PALETTE[max(votes, key=votes.get)] + (255,))
    return out


def with_flame(hull, length):
    """A copy of the hull with an engine flame of `length` rows below it."""
    img = hull.copy()
    bottom = max(y for y in range(32) for x in range(32) if img.getpixel((x, y))[3])
    for k in range(length):
        y = bottom + 1 + k
        if y > 31:
            break
        half = 3 if k < length - 3 else (2 if k < length - 1 else 1)
        for dx in range(-half, half):
            x = 16 + dx
            core = abs(dx + 0.5) < half - 1.2 and k < length - 2
            col = (255, 240, 150) if core and k < 2 else PALETTE["yellow"] if core else PALETTE["orange"]
            img.putpixel((x, y), col + (255,))
    return img


# ------------------------------------------------------------------ pixel art
# Left halves, 8 columns x 16 rows; the right half is the mirror image.
# . transparent  a light  b mid  c dark  w highlight  e eye / cockpit
# g accent  f engine glow

ENEMIES = [
    ("crab", [
        "........",
        "....cccc",
        "..ccbbbb",
        ".cbaaaaa",
        "cbaawaaa",
        "cbaeaaaa",
        "cbaaaaab",
        ".cbbbbbc",
        "c.cbccc.",
        "c..cb...",
        "cb..cb..",
        ".cb..c..",
        "..c.....",
        "........",
        "........",
        "........",
    ], {"a": (240, 60, 50), "b": (170, 30, 30), "c": (95, 15, 20), "w": (255, 210, 190),
        "e": (255, 240, 60), "f": (255, 140, 0)},
       {"e": (255, 120, 0)}),
    ("wasp", [
        "........",
        ".gg.....",
        "gwwg...c",
        "gwwwg.cb",
        ".gwwgcba",
        "..ggcbae",
        "....cbaa",
        "...cbaab",
        "...cbabc",
        "....cbbc",
        "....cbac",
        ".....cbc",
        ".....cb.",
        "......c.",
        ".......f",
        "........",
    ], {"a": (110, 230, 70), "b": (40, 160, 40), "c": (20, 85, 30), "w": (200, 250, 255),
        "g": (60, 170, 220), "e": (255, 50, 50), "f": (255, 210, 0)},
       {"w": (120, 200, 240), "g": (200, 250, 255), "f": (255, 120, 0)}),
    ("arrow", [
        "c.....ff",
        "cbc.....",
        ".cbc....",
        ".cbbc..c",
        "..cbbccb",
        "..cbaabb",
        "...cbaaw",
        "...cbaaa",
        "....cbae",
        "....cbaa",
        ".....cba",
        ".....cba",
        "......cb",
        "......cb",
        ".......c",
        "........",
    ], {"a": (210, 100, 245), "b": (135, 45, 185), "c": (70, 20, 105), "w": (255, 225, 255),
        "e": (255, 255, 120), "f": (120, 200, 255)},
       {"f": (230, 250, 255), "e": (255, 160, 60)}),
    ("orb", [
        "........",
        ".....ccc",
        "...ccbbb",
        "..cbbaaa",
        ".cbaaaww",
        ".cbaawwa",
        "cbaaaeee",
        "cbaaeegg",
        "cbaaeegg",
        "cbaaaeee",
        ".cbaaaaa",
        ".cbbaaaa",
        "..cbbbbb",
        ".c.ccccc",
        "c..c..c.",
        "........",
    ], {"a": (80, 220, 240), "b": (30, 140, 195), "c": (15, 70, 115), "w": (230, 255, 255),
        "e": (255, 255, 255), "g": (230, 30, 60)},
       {"g": (255, 160, 40), "e": (255, 220, 220)}),
    ("bomber", [
        "........",
        ".....cff",
        "....cbbb",
        "...cbaaa",
        "cccbaaww",
        "cbbbaaaa",
        "cbaaaaaa",
        "cbabbbaa",
        "cbacccba",
        "cbc..cba",
        "cc...cbe",
        ".....cbe",
        ".....cbb",
        "......cc",
        "........",
        "........",
    ], {"a": (250, 210, 60), "b": (190, 130, 30), "c": (110, 70, 20), "w": (255, 250, 200),
        "e": (255, 80, 40), "f": (255, 140, 0)},
       {"e": (255, 230, 80), "f": (255, 230, 120)}),
]


def art(rows, colours):
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    for y, half in enumerate(rows):
        assert len(half) == 8, half
        full = half + half[::-1]
        for x, ch in enumerate(full):
            if ch != ".":
                img.putpixel((x, y), colours[ch] + (255,))
    return img


def missile(frame):
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    body = {(7, 2): "w", (8, 2): "w", (7, 3): "w", (8, 3): "w", (7, 4): "c", (8, 4): "c",
            (7, 5): "c", (8, 5): "c", (6, 6): "b", (9, 6): "b", (7, 6): "c", (8, 6): "c"}
    cols = {"w": (255, 255, 255), "c": (120, 200, 255), "b": (40, 90, 220)}
    for (x, y), c in body.items():
        img.putpixel((x, y), cols[c] + (255,))
    flame = [(255, 240, 150), (255, 200, 60), (255, 120, 20), (200, 50, 10)]
    for k in range(3 + frame * 2):
        c = flame[min(k, 3)]
        img.putpixel((7, 7 + k), c + (255,))
        img.putpixel((8, 7 + k), c + (255,))
    return img


def bullet(frame):
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    r = 2.6 + 0.6 * frame
    for y in range(16):
        for x in range(16):
            d = math.hypot(x - 7.5, y - 7.5)
            if d < r:
                col = (255, 255, 220) if d < 1.2 else (255, 90, 200) if d < r - 0.8 else (170, 30, 140)
                img.putpixel((x, y), col + (255,))
    return img


def explosion(k):
    img = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    radius = 2.5 + k * 1.6
    for y in range(16):
        for x in range(16):
            d = math.hypot(x - 7.5, y - 7.5)
            wobble = 0.9 * math.sin(x * 2.1 + y * 1.3 + k)
            if d < radius + wobble:
                t = d / (radius + 0.01)
                if k == 3 and t < 0.6:
                    continue  # the last frame is a ring of smoke
                col = PALETTE["yellow"] if t < 0.45 else PALETTE["orange"] if t < 0.8 else (150, 40, 20)
                if k == 3:
                    col = (110, 100, 100)
                img.putpixel((x, y), col + (255,))
    return img


# ------------------------------------------------------------------ sheet


def put(sheet, tile, pattern):
    sheet.paste(tile, ((pattern % 8) * 16, (pattern // 8) * 16))


def put32(sheet, img, first):
    for k, (qx, qy) in enumerate([(0, 0), (16, 0), (0, 16), (16, 16)]):
        put(sheet, img.crop((qx, qy, qx + 16, qy + 16)), first + k)


def main():
    im = Image.open(SRC).convert("RGB")
    sheet = Image.new("RGBA", (128, 64), (0, 0, 0, 0))
    hull = player_hull(im)
    for f, length in enumerate([4, 6, 5]):
        put32(sheet, with_flame(hull, length), f * 4)
    for k, (_, rows, colours, frame2) in enumerate(ENEMIES):
        put(sheet, art(rows, colours), 12 + 2 * k)
        put(sheet, art(rows, {**colours, **frame2}), 13 + 2 * k)
    for f in range(2):
        put(sheet, missile(f), 22 + f)
    for k in range(4):
        put(sheet, explosion(k), 24 + k)
    for f in range(2):
        put(sheet, bullet(f), 28 + f)
    sheet.save(OUT)


main()
