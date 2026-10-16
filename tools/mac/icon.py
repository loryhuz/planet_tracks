"""Draws the macOS apps' icons (art/icons/*.png, 1024 px) from frames of the menu's film.

    /usr/bin/python3 tools/mac/icon.py [FILM] [OUT_DIR]

FILM is the wide menu film (crates/app/assets/video/menu-wide.mp4 of the main checkout by
default: it is a generated file, kept out of git), OUT_DIR art/icons/. Each icon is a square cut
from one frame (ffmpeg extracts it), on macOS's icon grid: an 824 px rounded square inside the
1024 px canvas, with a soft drop shadow. The editor's gets a blueprint grid over its circuit.
tools/mac/bundle.sh turns them into each bundle's AppIcon.icns; rerun this after refilming the
menu only if the icons should follow the new film.
"""

import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

# name: (seconds into the film, centre of the square in the 1920 × 1080 frame, its side, grid)
ICONS = {
    "game": (6.0, (880, 560), 760, False),
    "editor": (3.7, (900, 640), 820, True),
}

CANVAS = 1024
SIDE = 824  # the rounded square, on Apple's icon grid
INSET = (CANVAS - SIDE) // 2
RADIUS = 185


def frame(film, seconds, out):
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", str(seconds), "-i", str(film), "-frames:v", "1", str(out)], check=True)
    return Image.open(out).convert("RGB")


def icon(im, centre, side, grid):
    cx, cy = centre
    crop = im.crop((cx - side // 2, cy - side // 2, cx + side // 2, cy + side // 2)).resize((SIDE, SIDE), Image.LANCZOS)
    if grid:
        # The editor: a blueprint grid over the circuit.
        over = Image.new("RGBA", crop.size, (20, 60, 110, 70))
        d = ImageDraw.Draw(over)
        step = SIDE // 8
        for i in range(1, 8):
            d.line([(i * step, 0), (i * step, SIDE)], fill=(220, 240, 255, 120), width=3)
            d.line([(0, i * step), (SIDE, i * step)], fill=(220, 240, 255, 120), width=3)
        crop = Image.alpha_composite(crop.convert("RGBA"), over)
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    mask = Image.new("L", (SIDE, SIDE), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, SIDE - 1, SIDE - 1), radius=RADIUS, fill=255)
    # The shadow, a little lower than the square.
    shadow = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    alpha = Image.new("L", (CANVAS, CANVAS), 0)
    ImageDraw.Draw(alpha).rounded_rectangle((INSET, INSET + 12, INSET + SIDE - 1, INSET + 12 + SIDE - 1), radius=RADIUS, fill=110)
    shadow.putalpha(alpha.filter(ImageFilter.GaussianBlur(14)))
    canvas = Image.alpha_composite(canvas, shadow)
    canvas.paste(crop.convert("RGBA"), (INSET, INSET), mask)
    return canvas


def main():
    repo = Path(__file__).resolve().parents[2]
    main_checkout = Path(subprocess.run(["git", "-C", str(repo), "rev-parse", "--path-format=absolute", "--git-common-dir"], check=True, capture_output=True, text=True).stdout.strip()).parent
    film = Path(sys.argv[1]) if len(sys.argv) > 1 else main_checkout / "crates/app/assets/video/menu-wide.mp4"
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else repo / "art/icons"
    if not film.is_file():
        sys.exit(f"{film} is missing: make it with tools/video/menu_montage.sh")
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        for name, (seconds, centre, side, grid) in ICONS.items():
            im = frame(film, seconds, Path(tmp) / f"{name}.png")
            icon(im, centre, side, grid).save(out / f"{name}.png", optimize=True)
            print(out / f"{name}.png")


if __name__ == "__main__":
    main()
