"""Exports the demo's look as herdr-panel's art: the full-width pieces as
their own PNGs, the small sprites and the mascot's animations packed into
sprites.png with sprites.yaml naming where each one is.

Usage: python export.py ../assets (needs pypng)
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_src = open(os.path.join(HERE, "demo.py")).read().replace("\nmain()\n", "\n")
d = {"__file__": os.path.join(HERE, "demo.py")}
exec(compile(_src, "demo.py", "exec"), d)
Img, W, H = d["Img"], d["W"], d["H"]
INNER, STOREY, GROUND = d["INNER"], d["STOREY"], d["GROUND"]


def crop(img, x, y, w, h):
    out = Img(w, h)
    for yy in range(h):
        for xx in range(w):
            if 0 <= x + xx < img.w and 0 <= y + yy < img.h:
                out.px[yy][xx] = img.px[y + yy][x + xx]
    return out


def blank():
    return Img(W, H)


def storey_room(paper):
    """A storey's room: walls, shaft, wallpaper, windows and unlit lamp
    fittings, without number, desks or decor."""
    img = blank()
    top = d["storey_top"](1)
    d["shell"](img, top, d["WALLPAPER"][paper], 1, number=False)
    for i in range(d["DESKS"]):
        d["window"](img, d["desk_centre"](i), top, paper, i)
    d["lamps"](img, top, False)
    return crop(img, 0, top, W, STOREY)


# Mascot cells: 30x26, the mascot drawn at (2, 9) so its markers fit above
# and to the right. The feet are at (13, 21).
CELL_W, CELL_H, AT = 30, 26, (2, 9)


def mascot_frame(state, tick):
    img = Img(CELL_W, CELL_H)
    x, y = AT
    m, sign = d["mascot"], d["sign"]
    odd = tick % 2 == 1
    if state == "working":
        pose = m("up", "down", stride=True) if odd else m("down", "up")
    elif state == "idle":
        pose = m(eyes="drowsy")
    elif state == "awake":
        # Idle but focused: eyes open, blinking now and then.
        pose = m(eyes="closed" if tick % 4 == 3 else "open")
    elif state == "blocked":
        pose = m(arm_r="up" if odd else "down")
    elif state == "done":
        pose = m("up", "up", lift=1 if odd else 0)
    else:
        pose = m()
    d["draw_mascot"](img, x, y, pose)
    if state == "idle":
        # One z, then two, then both stay, then none.
        if tick % 4 >= 1:
            sign(img, x + 20, y - 2, d["Z"], d["ZZZ"])
        if tick % 4 >= 2:
            sign(img, x + 23, y - 6, d["Z"], d["ZZZ"])
    elif state == "blocked":
        if tick % 4 < 2:
            sign(img, x + 22, y - 4, d["BANG"], d["ALERT"])
    elif state == "done":
        for k, (sx, sy) in enumerate([(0, -1), (21, 0), (0, 8), (21, 7)]):
            if (tick + k) % 3 != 0:
                sign(img, x + sx, y + sy, ["#", "#"], d["SPARK"])
    elif state == "unknown":
        sign(img, x + 21, y - 3, d["QMARK"], d["QUESTION"])
    return img


FRAMES = {"working": 2, "idle": 4, "awake": 4, "blocked": 4, "done": 6, "unknown": 1}


def sprites():
    """The small sprites, by name."""
    out = {}
    top = d["storey_top"](1)
    floor = top + INNER
    # Desk: drawn for desk 0 with nobody absent, so no props.
    img = blank()
    cx = d["desk_centre"](0)
    d["desk"](img, cx, top, True, 0, True)
    out["desk"] = crop(img, cx - 5, floor - 14, 19, 14)
    # A lit bulb and its light, centred on the fitting.
    img = blank()
    lx = d["DESKS_X"] + d["SPACING"]
    d["lamp_light"](img, lx, top)
    out["lamp-on"] = crop(img, lx - 18, top, 37, INNER)
    # Moonlight through a window onto the floor.
    img = blank()
    for y in range(top + 22, top + INNER):
        s = (y - top - 22) // 2
        for x in range(cx - 7 + s, cx + 8 + s):
            img.blend(x, y, d["h"](0xA0B0F8), 0.12)
    out["moonbeam"] = crop(img, cx - 7, top + 22, 20, INNER - 22)
    # The car.
    img = blank()
    d["car"](img, 1)
    out["car"] = crop(img, d["SHAFT_X"] + 1, top + INNER - 24, d["SHAFT_W"] - 2, 24)
    # Arrows, with their corners cut.
    for name, up in (("arrow-up", True), ("arrow-down", False)):
        img = blank()
        d["arrow"](img, 100, 100, up)
        a = crop(img, 100, 100, 14, 14)
        for (x, y) in [(0, 0), (13, 0), (0, 13), (13, 13)]:
            a.px[y][x] = (0, 0, 0, 0)
        out[name] = a
    # The plumbob.
    img = Img(5, 7)
    img.grid(0, 0, d["PLUMBOB"], {"#": d["h"](0x084018), "L": d["h"](0xB8FFA8), "G": d["h"](0x48D848), "D": d["h"](0x209030)})
    out["plumbob"] = img
    # Digits 0 to 9 in a row, a font cell each with their drop shadow.
    fw, fh = d["FONT_W"], d["FONT_H"]
    img = Img(10 * fw, fh)
    for n in range(10):
        d["text"](img, fw * n, 0, str(n), d["h"](0xF8B040), d["h"](0x582810))
    out["digits"] = img
    # Decor for each storey's right end: bookshelf, water cooler, plant,
    # coffee machine.
    for kind in range(4):
        img = blank()
        d["decor"](img, top, kind)
        out[f"decor-{kind}"] = crop(img, 284, top, 32, INNER)
    # The mascot: a strip of frames per agent status.
    for state, n in FRAMES.items():
        strip = Img(CELL_W * n, CELL_H)
        for t in range(n):
            f = mascot_frame(state, t)
            for y in range(CELL_H):
                for x in range(CELL_W):
                    strip.px[y][t * CELL_W + x] = f.px[y][x]
        out[f"mascot-{state}"] = strip
    return out


def pack(parts, width=200):
    """Places sprites in rows, tallest first, a pixel apart."""
    order = sorted(parts, key=lambda n: (-parts[n].h, n))
    places, x, y, row_h = {}, 0, 0, 0
    for name in order:
        img = parts[name]
        if x + img.w > width:
            x, y, row_h = 0, y + row_h + 1, 0
        places[name] = (x, y, img.w, img.h)
        x += img.w + 1
        row_h = max(row_h, img.h)
    sheet = Img(width, y + row_h)
    for name, (x, y, w, h) in places.items():
        for yy in range(h):
            for xx in range(w):
                sheet.px[y + yy][x + xx] = parts[name].px[yy][xx]
    return sheet, places


def main():
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    img = blank()
    d["backdrop"](img)
    img.save(os.path.join(out, "backdrop.png"))
    img = blank()
    d["street"](img)
    crop(img, 0, GROUND, W, H - GROUND).save(os.path.join(out, "street.png"))
    img = blank()
    d["lobby"](img, number=False)
    crop(img, 0, d["storey_top"](0), W, STOREY).save(os.path.join(out, "lobby.png"))
    for p in range(4):
        storey_room(p).save(os.path.join(out, f"storey-{p}.png"))
    for name, light in (("roof-on", True), ("roof-off", False)):
        img = blank()
        d["roof"](img, 1, light)
        t = d["storey_top"](1) - 13
        crop(img, 0, t - 8, W, 22).save(os.path.join(out, f"{name}.png"))
    sheet, places = pack(sprites())
    sheet.save(os.path.join(out, "sprites.png"))
    names = ["desk", "lamp-on", "moonbeam", "car", "arrow-up", "arrow-down", "plumbob", "digits"]
    names += [f"decor-{k}" for k in range(4)] + [f"mascot-{s}" for s in FRAMES]
    with open(os.path.join(out, "sprites.yaml"), "w") as f:
        f.write("# Where each sprite is in sprites.png: [x, y, width, height], in pixels.\n")
        f.write("# Sizes must match what the layout in scene/src/lib.rs expects; its\n")
        f.write("# tests check them. Mascot strips are frames of 30x26 in a row.\n")
        for n in names:
            x, y, w, h = places[n]
            f.write(f"{n}: [{x}, {y}, {w}, {h}]\n")


main()
