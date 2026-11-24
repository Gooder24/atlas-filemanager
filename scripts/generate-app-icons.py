#!/usr/bin/env python3
"""Regenerate the Atlas app icons in packaging/icons.

Contrast strategy (the Windows taskbar can be light or dark, over any wallpaper):
  * atlas.png / atlas.ico   plated: dark rounded tile + white glyph + light inner rim.
                            Self-contained, so it reads on every background (used by the
                            .exe, shortcuts, installer and MSIX plated assets).
  * atlas-unplated.png      white glyph with a dark hairline  -> altform-unplated (dark taskbar).
  * atlas-lightunplated.png dark glyph with a white hairline  -> altform-lightunplated (light taskbar).

Usage: python -I scripts/generate-app-icons.py
"""
import math
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "packaging" / "icons"
S = 2048  # supersampled canvas
FINAL = 512
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

# Glyph geometry (capsules: centre-line endpoints + radius) in the original 500px art.
R = 26.0
CAPSULES = [
    ((238.5, 92.5), (103.5, 326.3)),  # left bar
    ((359.6 - 0.5 * 135, 282.2 - 0.866 * 135), (359.6 + 0.5 * 135, 282.2 + 0.866 * 135)),  # right bar
    ((207 - 135, 401), (207 + 135, 401)),  # bottom bar
]


def glyph_mask(box_size: float, centre: tuple[float, float], grow: float = 0.0) -> Image.Image:
    """Alpha mask of the glyph fitted into `box_size` px, centred on `centre`."""
    xs = [p[0] for c in CAPSULES for p in c]
    ys = [p[1] for c in CAPSULES for p in c]
    minx, maxx, miny, maxy = min(xs) - R, max(xs) + R, min(ys) - R, max(ys) + R
    scale = box_size / max(maxx - minx, maxy - miny)
    ox = centre[0] - (minx + maxx) / 2 * scale
    oy = centre[1] - (miny + maxy) / 2 * scale
    mask = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(mask)
    r = (R + grow) * scale
    for (x1, y1), (x2, y2) in CAPSULES:
        p1 = (x1 * scale + ox, y1 * scale + oy)
        p2 = (x2 * scale + ox, y2 * scale + oy)
        d.line([p1, p2], fill=255, width=round(2 * r))
        for cx, cy in (p1, p2):
            d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=255)
    return mask


def solid(color, mask):
    layer = Image.new("RGBA", (S, S), color)
    layer.putalpha(mask)
    return layer


def shadow(mask, dy, blur, alpha):
    m = ImageChops.offset(mask, 0, dy).filter(ImageFilter.GaussianBlur(blur))
    return solid((0, 0, 0, 255), m.point(lambda v: int(v * alpha)))


def finish(img: Image.Image, size: int) -> Image.Image:
    return img.resize((size, size), Image.LANCZOS)


def plated() -> Image.Image:
    margin = S * 0.04
    tile = Image.new("L", (S, S), 0)
    ImageDraw.Draw(tile).rounded_rectangle(
        [margin, margin, S - margin, S - margin], radius=S * 0.225, fill=255
    )
    # Vertical gradient: slightly lifted top so the tile separates from black surfaces.
    grad = Image.new("RGBA", (S, S))
    top, bottom = (38, 50, 74), (11, 16, 32)
    px = grad.load()
    for y in range(S):
        t = y / (S - 1)
        c = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3)) + (255,)
        for x in range(S):
            px[x, y] = c
    grad.putalpha(tile)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.alpha_composite(grad)
    # Light rim just inside the edge: keeps the tile visible on dark taskbars/wallpapers.
    inner = tile.filter(ImageFilter.MinFilter(2 * round(S * 0.008) + 1))
    rim = ImageChops.subtract(tile, inner)
    img.alpha_composite(solid((255, 255, 255, 255), rim.point(lambda v: int(v * 0.30))))
    glyph = glyph_mask(S * 0.66, (S / 2, S / 2))
    img.alpha_composite(shadow(glyph, round(S * 0.012), S * 0.012, 0.45))
    img.alpha_composite(solid((255, 255, 255, 255), glyph))
    return img


def unplated(glyph_color, edge_color) -> Image.Image:
    glyph = glyph_mask(S * 0.88, (S / 2, S / 2))
    edge = glyph_mask(S * 0.88, (S / 2, S / 2), grow=3.5)
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.alpha_composite(solid(edge_color, edge))
    img.alpha_composite(solid(glyph_color, glyph))
    return img


def main() -> None:
    plate = plated()
    finish(plate, FINAL).save(OUT / "atlas.png")
    sizes = [(s, s) for s in ICO_SIZES]
    finish(plate, 256).save(OUT / "atlas.ico", sizes=sizes)
    finish(unplated((255, 255, 255, 255), (15, 23, 42, 150)), FINAL).save(OUT / "atlas-unplated.png")
    finish(unplated((15, 23, 42, 255), (255, 255, 255, 170)), FINAL).save(OUT / "atlas-lightunplated.png")
    print("wrote icons to", OUT)


if __name__ == "__main__":
    main()
