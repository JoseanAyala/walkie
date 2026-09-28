#!/usr/bin/env python3
"""Draws walkie's icons in the Settings window's palette (ui/src/lib/theme.css):
a square cream sheet with the checker trim, and an ink microphone whose grille is
the same checker. Writes icons/icon.png (then `cargo tauri icon` makes the
app icon sizes) and the three 32×32 tray icons.

Run: python3 scripts/make-icons.py  (needs Pillow)
"""

from pathlib import Path

from PIL import Image, ImageDraw

ICONS = Path(__file__).resolve().parent.parent / "crates/walkie-app/icons"

SHEET = (251, 249, 243, 255)  # --sheet
BG = (245, 241, 230, 255)  # --bg
INK = (28, 27, 34, 255)  # --ink
RED = (224, 64, 47, 255)  # --red
CLEAR = (0, 0, 0, 0)

SS = 4  # supersampling for the app icon's curves


def checker(draw, box, cols, rows, a, b):
    """`cols` × `rows` whole squares filling `box` exactly."""
    x0, y0, x1, y1 = box
    w, h = (x1 - x0) / cols, (y1 - y0) / rows
    for j in range(rows):
        for i in range(cols):
            c = a if (i + j) % 2 == 0 else b
            draw.rectangle(
                [round(x0 + i * w), round(y0 + j * h), round(x0 + (i + 1) * w) - 1, round(y0 + (j + 1) * h) - 1],
                fill=c,
            )


def app_icon():
    s = 1024 * SS
    u = lambda v: int(v * SS)  # noqa: E731
    img = Image.new("RGBA", (s, s), CLEAR)

    # the body: macOS's icon grid, 824 of 1024, a square sheet with the
    # checker trim above it, like the Settings window
    body = [u(100), u(100), u(924), u(924)]
    art = Image.new("RGBA", (s, s), CLEAR)
    d = ImageDraw.Draw(art)
    d.rectangle(body, fill=SHEET)
    checker(d, (u(100), u(100), u(924), u(212)), 14, 2, INK, SHEET)
    d.rectangle([u(100), u(212), u(924), u(222)], fill=INK)

    # the microphone: an ink capsule, its top half a checker grille
    cx = 512
    top, bottom, half = 290, 610, 104
    d.rounded_rectangle([u(cx - half), u(top), u(cx + half), u(bottom)], radius=u(half), fill=INK)
    inner = 20
    grille = Image.new("RGBA", (s, s), CLEAR)
    g = ImageDraw.Draw(grille)
    checker(g, (u(cx - half + inner), u(top + inner), u(cx + half - inner), u(452)), 6, 5, SHEET, INK)
    g.rectangle([u(cx - half), u(462), u(cx + half), u(bottom)], fill=SHEET)
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [u(cx - half + inner), u(top + inner), u(cx + half - inner), u(bottom - inner)],
        radius=u(half - inner),
        fill=255,
    )
    art.paste(grille, (0, 0), mask)
    d.rectangle([u(cx - half + inner), u(452), u(cx + half - inner), u(472)], fill=INK)
    # the yoke, the stem and the foot
    w = 24
    d.arc([u(cx - 164), u(372), u(cx + 164), u(700)], 0, 180, fill=INK, width=u(w))
    d.line([u(cx - 164 + w / 2), u(536), u(cx - 164 + w / 2), u(470)], fill=INK, width=u(w))
    d.line([u(cx + 164 - w / 2), u(536), u(cx + 164 - w / 2), u(470)], fill=INK, width=u(w))
    d.rectangle([u(cx - 14), u(698), u(cx + 14), u(770)], fill=INK)
    d.rectangle([u(cx - 110), u(758), u(cx + 110), u(782)], fill=INK)
    # the red dot: on air
    d.ellipse([u(cx + 168), u(270), u(cx + 228), u(330)], fill=RED)

    # the ink edge on top
    img.paste(art, (0, 0), art)
    ImageDraw.Draw(img).rectangle(body, outline=INK, width=u(20))
    img.resize((1024, 1024), Image.LANCZOS).save(ICONS / "icon.png")


# 32×32 (16pt at 2x), drawn on the pixel grid so it stays crisp
MIC = [
    "................................",
    "................................",
    "..........############..........",
    ".........##############.........",
    "........################........",
    "........################........",
    "........################........",
    "........################........",
    "........################........",
    "........################........",
    "........################........",
    "........################........",
    "....##..################..##....",
    "....##..################..##....",
    "....##..################..##....",
    "....##..################..##....",
    "....##..################..##....",
    "....###.################.###....",
    ".....##..##############..##.....",
    ".....###..############..###.....",
    "......####............####......",
    "........##############..........",
    "..........##########............",
    "...............##...............",
    "...............##...............",
    "...............##...............",
    "...............##...............",
    "..........############..........",
    "..........############..........",
    "................................",
    "................................",
    "................................",
]


def fix_mic():
    # keep the drawing symmetric: mirror the left half
    rows = []
    for r in MIC:
        left = r[:16]
        rows.append(left + left[::-1])
    return rows


def tray(name, color, grille):
    img = Image.new("RGBA", (32, 32), CLEAR)
    px = img.load()
    for y, row in enumerate(fix_mic()):
        for x, ch in enumerate(row):
            if ch != "#":
                continue
            # the grille: a 2px checker over the capsule's top, leaving a
            # 1px rim
            in_grille = 9 <= x <= 22 and 3 <= y <= 10
            if grille and in_grille and ((x - 9) // 2 + (y - 3) // 2) % 2 == 1:
                continue
            px[x, y] = color
    img.save(ICONS / f"{name}.png")


if __name__ == "__main__":
    app_icon()
    # idle is a template (macOS tints it): ink with a see-through grille
    tray("tray-idle", INK, grille=True)
    tray("tray-rec", RED, grille=False)
    tray("tray-busy", RED, grille=True)
