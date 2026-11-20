"""Builds the icons of the lz80nc executable:

* lz80n.ico  - Windows, embedded by compiler/build.rs
* lz80n.icns - macOS, set as the Finder icon of the binary by the release workflow

The small sizes (16, 24, 32) come from the 16x16 pixel drawing below, the large
ones from the glossy purple logo (on macOS with the usual margin around the
tile). Needs Pillow. Run it after changing either:
    python assets/make_icon.py
"""

from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).resolve().parent
LOGO = ASSETS / "Glossy Purple LZ80N Rainbow Logo.png"
SMALL_SIZES = [16, 24, 32]
LARGE_SIZES = [48, 64, 128, 256]

# 16x16: "LZ" over the Spectrum stripe on a purple tile.
# ' ' transparent, '.' tile, '-' tile edge, 'W' the L, 'R' the Z, 'r y g c' the stripe
PIXELS = [
    " -------------- ",
    "-..............-",
    "-..............-",
    "-.WW...RRRRRRR.-",
    "-.WW...RRRRRRR.-",
    "-.WW.......RR..-",
    "-.WW......RR...-",
    "-.WW.....RR....-",
    "-.WW....RR.....-",
    "-.WWWW.RRRRRRR.-",
    "-.WWWW.RRRRRRR.-",
    "-..............-",
    "-...rrryyygggcc-",
    "-..rrryyygggccc-",
    "-..............-",
    " -------------- ",
]
COLOURS = {
    ".": (76, 18, 160, 255),
    "-": (150, 60, 255, 255),
    "W": (255, 255, 255, 255),
    "R": (255, 32, 32, 255),
    "r": (232, 32, 48, 255),
    "y": (255, 214, 0, 255),
    "g": (40, 200, 60, 255),
    "c": (30, 190, 240, 255),
}


def pixel_icon() -> Image.Image:
    grid = Image.new("RGBA", (16, 16), (0, 0, 0, 0))
    for y, row in enumerate(PIXELS):
        assert len(row) == 16, f"row {y} is {len(row)} pixels wide"
        for x, ch in enumerate(row):
            if ch != " ":
                grid.putpixel((x, y), COLOURS[ch])
    return grid


def main() -> None:
    # NEAREST up to 768 (a multiple of 16, 24 and 32), then BOX down keeps the grid sharp
    big_grid = pixel_icon().resize((768, 768), Image.NEAREST)
    images = [big_grid.resize((s, s), Image.BOX) for s in SMALL_SIZES]
    logo = Image.open(LOGO).convert("RGBA")
    images += [logo.resize((s, s), Image.LANCZOS) for s in LARGE_SIZES]
    images[-1].save(
        ASSETS / "lz80n.ico",
        format="ICO",
        sizes=[im.size for im in images],
        append_images=images[:-1],
    )

    # macOS: the tile fills about 80 % of the canvas, like the system's own icons
    mac_tile = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    mac_tile.alpha_composite(logo.resize((824, 824), Image.LANCZOS), (100, 100))
    mac = [big_grid.resize((s, s), Image.BOX) for s in (16, 32)]
    mac += [mac_tile.resize((s, s), Image.LANCZOS) for s in (64, 128, 256, 512)]
    mac_tile.save(ASSETS / "lz80n.icns", format="ICNS", append_images=mac)


if __name__ == "__main__":
    main()
