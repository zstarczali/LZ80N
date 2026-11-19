"""Sprites for examples/shooter.lz80n from examples/assets/space-ship.png.

The source is a blurry, upscaled picture of 6 x 4 ships on a gradient. Ship
pixels are found by colour; the hull's bounding box (plus a limited flame
below it) is shrunk to the sprite size, and every sprite pixel takes the
most common of a few fixed colours among the ship pixels it covers, or
stays transparent.

Output: examples/assets/ships.png, 128 x 80, 16x16 patterns numbered left
to right, top to bottom (8 per row):
  0..11   player, 3 frames of 32x32 (row 2, columns 1-3), 4 patterns each:
          top-left, top-right, bottom-left, bottom-right
  12..23  enemy, 3 frames of 32x32 (row 3, columns 4-6), turned upside down
  24..29  missile, 6 frames of 16x16 (row 4), flame at the bottom
  30..33  explosion, 4 frames of 16x16 (drawn here)
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


def hull_box(im, x0, y0, x1, y1):
    xs, ys = [], []
    for y in range(y0, y1):
        for x in range(x0, x1):
            c = classify(im.getpixel((x, y)))
            if c and c not in ("orange", "yellow"):
                xs.append(x)
                ys.append(y)
    return min(xs), min(ys), max(xs) + 1, max(ys) + 1


def shrink(im, box, size, limit):
    bx0, by0, bx1, by1 = box
    side = max(bx1 - bx0, by1 - by0)
    cx, cy = (bx0 + bx1) / 2, (by0 + by1) / 2
    step = side / size
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    lx0, ly0, lx1, ly1 = limit
    for ty in range(size):
        for tx in range(size):
            sx0 = cx - side / 2 + tx * step
            sy0 = cy - side / 2 + ty * step
            votes, tot = {}, 0
            for yy in range(int(sy0), int(sy0 + step) + 1):
                for xx in range(int(sx0), int(sx0 + step) + 1):
                    if not (lx0 <= xx < lx1 and ly0 <= yy < ly1):
                        continue
                    tot += 1
                    c = classify(im.getpixel((xx, yy)))
                    if c:
                        votes[c] = votes.get(c, 0) + 1
            n = sum(votes.values())
            if tot and n / tot >= 0.45:
                best = max(votes, key=votes.get)
                out.putpixel((tx, ty), PALETTE[best] + (255,))
    return out


def ship(im, col, row, size):
    x0, y0, x1, y1 = COLS[col], ROWS[row], COLS[col + 1], ROWS[row + 1]
    hx0, hy0, hx1, hy1 = hull_box(im, x0, y0, x1, y1)
    flame = int((hy1 - hy0) * 0.45)
    # the flame: below the hull (or above it for the missiles of row 4)
    if row == 3:
        box = (hx0, max(y0, hy0 - flame), hx1, hy1)
    else:
        box = (hx0, hy0, hx1, min(y1, hy1 + flame))
    limit = (x0, box[1], x1, box[3])
    return shrink(im, box, size, limit)


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


def put(sheet, tile, pattern):
    sheet.paste(tile, ((pattern % 8) * 16, (pattern // 8) * 16))


def put32(sheet, img, first):
    for k, (qx, qy) in enumerate([(0, 0), (16, 0), (0, 16), (16, 16)]):
        put(sheet, img.crop((qx, qy, qx + 16, qy + 16)), first + k)


def main():
    im = Image.open(SRC).convert("RGB")
    sheet = Image.new("RGBA", (128, 80), (0, 0, 0, 0))
    for f, col in enumerate([0, 1, 2]):
        put32(sheet, ship(im, col, 1, 32), f * 4)
    for f, col in enumerate([3, 4, 5]):
        put32(sheet, ship(im, col, 2, 32).transpose(Image.FLIP_TOP_BOTTOM), 12 + f * 4)
    for f in range(6):
        put(sheet, ship(im, f, 3, 16).transpose(Image.FLIP_TOP_BOTTOM), 24 + f)
    for k in range(4):
        put(sheet, explosion(k), 30 + k)
    sheet.save(OUT)


main()
