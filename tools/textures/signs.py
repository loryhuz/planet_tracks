"""Draws the lettering the app embeds for the start, checkpoint and finish gates
(crates/app/assets/textures/signs_albedo.png and signs_normal.png, one layer of the surface
texture arrays, see scene.wgsl's L_SIGNS).

The layer holds five rows, each a fifth of its height and five times as wide as tall: the two
halves of "PLANET TRACKS" for the gates' banner (cut in the middle of the space, so the two
words sit side by side on the banner), then the words painted on the deck before the gate line:
"DÉPART", "ARRIVÉE", "CHECKPOINT". The lettering is Saira Stencil One, the game's display face
(crates/app/assets/fonts, under the OFL): stencilled, as the colonists would paint it.

The ink is in the alpha channel (linear, 1 = ink), the colour channels are white; the relief layer
is flat. The rows keep a margin above and below their letters, so the mipmaps do not bleed one
row into the next.

    /usr/bin/python3 tools/textures/signs.py [OUT_DIR]

    (OUT_DIR defaults to crates/app/assets/textures)
"""

import os
import sys

from PIL import Image, ImageDraw, ImageFont

SIZE = 1024
ROWS = 5
ROW = SIZE / ROWS
FONT = os.path.join(os.path.dirname(__file__), "..", "..", "crates", "app", "assets", "fonts", "SairaStencilOne-Regular.ttf")
# Letters fill this share of a row's height, and at most this share of its width.
HEIGHT = 0.66
WIDTH = 0.92
# Rendered this many times larger, then reduced (clean edges).
OVER = 4


def fitted_font(text, width, height):
    """The largest size at which `text` fits `width` x `height` pixels (caps height)."""
    size = int(height)
    while size > 8:
        font = ImageFont.truetype(FONT, size)
        l, t, r, b = font.getbbox(text)
        if r - l <= width and b - t <= height:
            return font
        size -= 2
    return ImageFont.truetype(FONT, 8)


def draw_word(canvas, text, row):
    """Draws `text` centred in `row` of `canvas` (oversampled)."""
    draw = ImageDraw.Draw(canvas)
    w, h = canvas.size[0], ROW * OVER
    top = row * ROW * OVER
    font = fitted_font(text, WIDTH * w, HEIGHT * h)
    l, t, r, b = font.getbbox(text)
    draw.text(((w - (r - l)) / 2 - l, top + (h - (b - t)) / 2 - t), text, font=font, fill=255)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "..", "crates", "app", "assets", "textures")
    ink = Image.new("L", (SIZE * OVER, SIZE * OVER), 0)
    # "PLANET TRACKS" across rows 0 and 1: drawn on a strip twice as wide, then split.
    strip = Image.new("L", (2 * SIZE * OVER, int(ROW * OVER)), 0)
    draw_word_strip(strip, "PLANET TRACKS", 6)
    ink.paste(strip.crop((0, 0, SIZE * OVER, strip.size[1])), (0, 0))
    ink.paste(strip.crop((SIZE * OVER, 0, 2 * SIZE * OVER, strip.size[1])), (0, int(round(ROW * OVER))))
    for row, word in [(2, "DÉPART"), (3, "ARRIVÉE"), (4, "CHECKPOINT")]:
        draw_word(ink, word, row)
    ink = ink.resize((SIZE, SIZE), Image.LANCZOS)
    white = Image.new("L", (SIZE, SIZE), 255)
    Image.merge("RGBA", (white, white, white, ink)).save(os.path.join(out, "signs_albedo.png"))
    flat = Image.new("L", (SIZE, SIZE), 128)
    Image.merge("RGB", (flat, flat, Image.new("L", (SIZE, SIZE), 0))).save(os.path.join(out, "signs_normal.png"))
    print("signs: rows PLANET | TRACKS | DÉPART | ARRIVÉE | CHECKPOINT ->", out)


def draw_word_strip(strip, text, cut):
    """`text` on a strip two rows wide, the space after `cut` characters centred on it."""
    draw = ImageDraw.Draw(strip)
    w, h = strip.size
    first, second = text[:cut], text[cut + 1 :]
    font = fitted_font(text, WIDTH * w, HEIGHT * h)
    space = font.getlength(" ")
    l1, t1, r1, b1 = font.getbbox(first)
    l2, t2, r2, b2 = font.getbbox(second)
    t, b = min(t1, t2), max(b1, b2)
    y = (h - (b - t)) / 2 - t
    mid = w / 2
    draw.text((mid - space / 2 - r1, y), first, font=font, fill=255)
    draw.text((mid + space / 2 - l2, y), second, font=font, fill=255)


main()
