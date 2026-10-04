#!/usr/bin/env python3
"""Render the stopwatch logo (the design in the toggl-linux-stopwatch SVG).

Produces two outputs:
  1. app-icon.png   full-color 1024x1024 tile. Feed to `npm run icons`.
  2. stdout         a 48x48 coverage mask as a Rust array literal for
                    tray.rs. The tray tints this mask per state: accent
                    while running, grey while idle. The chevron and the
                    underscore are carved out as negative space, so the
                    monochrome tray mark stays legible on any panel color.

Geometry mirrors the SVG by hand. All coordinates are 200x200 viewBox
units, multiplied by one scale factor. Colors match styles.css:
body #dd3873 (--accent), glyph white (as on .play-btn).
"""

import sys
from PIL import Image, ImageDraw

BODY = (221, 56, 115, 255)  # --accent
GLYPH = (255, 255, 255, 255)
BG_TILE = (247, 247, 248, 255)  # --bg light, for the full-color tile only
TILE_R = 44  # viewBox units, from the SVG rect rx

# Geometry in viewBox units (see the source SVG).
CROWN = (88, 24, 24, 13, 4)  # x y w h r
STEM = (94, 35, 12, 14)  # x y w h
SIDE = (93, 36, 14, 12, 3, 46, 100, 116)  # x y w h r angle cx cy
CIRCLE = (100, 116, 66)  # cx cy r
CHEVRON = [(80, 98), (106, 116), (80, 134)]  # polyline
UNDER = [(114, 136), (136, 136)]  # segment
STROKE = 13  # glyph stroke width in viewBox units


def draw_polyline(d, pts, width, scale, color):
    """Stroke a polyline with round caps and joins, at the given scale."""
    r = width * scale / 2.0
    wpx = int(round(width * scale))
    sp = [(x * scale, y * scale) for x, y in pts]
    for (x1, y1), (x2, y2) in zip(sp, sp[1:]):
        d.line([x1, y1, x2, y2], fill=color, width=wpx)
    for (x, y) in sp:
        d.ellipse([x - r, y - r, x + r, y + r], fill=color)


def rotated_rounded_rect(size, params, scale, color):
    """The angled side button, drawn on its own layer and rotated."""
    x, y, w, h, r, angle, cx, cy = params
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    d.rounded_rectangle(
        [x * scale, y * scale, (x + w) * scale, (y + h) * scale],
        radius=r * scale,
        fill=color,
    )
    # PIL rotates counterclockwise; SVG rotate is clockwise.
    return layer.rotate(-angle, center=(cx * scale, cy * scale), resample=Image.BICUBIC)


def draw_logo(size, tile):
    """Render the mark at `size` px. tile=True adds the rounded background."""
    scale = size / 200.0
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    if tile:
        d.rounded_rectangle([0, 0, size - 1, size - 1], radius=TILE_R * scale, fill=BG_TILE)

    # Body: crown, stem, main dial.
    x, y, w, h, r = CROWN
    d.rounded_rectangle([x * scale, y * scale, (x + w) * scale, (y + h) * scale], radius=r * scale, fill=BODY)
    x, y, w, h = STEM
    d.rectangle([x * scale, y * scale, (x + w) * scale, (y + h) * scale], fill=BODY)
    cx, cy, rr = CIRCLE
    d.ellipse([(cx - rr) * scale, (cy - rr) * scale, (cx + rr) * scale, (cy + rr) * scale], fill=BODY)

    # Angled side button.
    img.alpha_composite(rotated_rounded_rect((size, size), SIDE, scale, BODY))

    # Glyph on top.
    d = ImageDraw.Draw(img)
    draw_polyline(d, CHEVRON, STROKE, scale, GLYPH)
    draw_polyline(d, UNDER, STROKE, scale, GLYPH)
    return img


def glyph_alpha(size):
    """A grayscale layer with the glyph strokes at full opacity."""
    scale = size / 200.0
    layer = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(layer)
    draw_polyline(d, CHEVRON, STROKE, scale, 255)
    draw_polyline(d, UNDER, STROKE, scale, 255)
    return layer


def make_mask():
    """Body-only coverage at 48x48: dial minus the carved glyph."""
    big = 48 * 8  # supersample
    body = draw_logo(big, tile=False)
    alpha = body.getchannel("A")
    carve = glyph_alpha(big)
    knocked = Image.frombytes(
        "L", alpha.size, bytes(max(a - c, 0) for a, c in zip(alpha.tobytes(), carve.tobytes()))
    )
    return knocked.resize((48, 48), Image.LANCZOS)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "app-icon.png"
    # Full-color mark on a transparent background, matching the source SVG:
    # accent dial and crown with a white chevron and underscore.
    draw_logo(1024, tile=False).save(out)
    print(f"wrote {out}", file=sys.stderr)

    data = make_mask().tobytes()
    print("const MARK: [u8; 48 * 48] = [")
    for row in range(48):
        vals = ",".join(str(v) for v in data[row * 48:(row + 1) * 48])
        print(f"    {vals},")
    print("];")


if __name__ == "__main__":
    main()
