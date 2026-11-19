"""Pictures for examples/images.lz80n (run from the repository root)."""
import math
from PIL import Image, ImageDraw


def landscape():
    w, h = 256, 192
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):  # sky: deep blue to orange
        t = y / 120
        d.line([(0, y), (w, y)], fill=(int(40 + 215 * t), int(30 + 110 * t), int(120 - 60 * t)))
    d.ellipse([150, 70, 220, 140], fill=(255, 220, 60))  # sun
    for layer, (col, base, amp, f) in enumerate([((90, 40, 110), 120, 25, 0.03), ((40, 20, 70), 140, 18, 0.05)]):
        pts = [(x, base - amp * math.sin(x * f + layer * 2) - 8 * math.sin(x * f * 3.1)) for x in range(w + 1)]
        d.polygon(pts + [(w, h), (0, h)], fill=col)
    d.rectangle([0, 160, w, h], fill=(20, 60, 90))  # sea
    for y in range(162, h, 6):
        for x in range((y * 7) % 23, w, 23):
            d.line([(x, y), (x + 8, y)], fill=(255, 200, 80))
    img.save("examples/assets/landscape.png")


def badge():
    w, h = 64, 48
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, w - 1, h - 1], radius=12, fill=(20, 20, 30, 255))
    for i, c in enumerate([(255, 40, 40), (255, 220, 0), (40, 200, 60), (40, 160, 255)]):
        d.rectangle([8 + i * 12, 30 - i * 4, 17 + i * 12, 40], fill=c)
    d.text((10, 6), "LZ80N", fill=(255, 255, 255, 255))
    img.save("examples/assets/badge.png")


landscape()
badge()
