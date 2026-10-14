"""Draws a mock-up of the panel's screen at 1:1 (320x480): the tower with
agents in every state, a dark storey, the lobby, roof and street. It is
where the tower's look is designed; export.py turns its drawings into the
art in ../assets. Writes demo.png, demo-2x.png, demo-3x.png and, with the
touch areas outlined, demo-touch.png and demo-touch-3x.png into OUT_DIR.

Usage: python demo.py OUT_DIR (needs pypng)
"""

import math
import os
import sys

import png

W, H = 320, 480


def q(c):
    """Cuts a colour to 15 bits, as the SNES and GBA store them."""
    return tuple(v & 0xF8 for v in c[:3]) + (c[3] if len(c) > 3 else 255,)


class Img:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[(0, 0, 0, 0)] * w for _ in range(h)]

    def set(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = q(c)

    def get(self, x, y):
        return self.px[y][x]

    def fill(self, x, y, w, h, c):
        for yy in range(y, y + h):
            for xx in range(x, x + w):
                self.set(xx, yy, c)

    def box(self, x, y, w, h, c, line):
        self.fill(x, y, w, h, line)
        self.fill(x + 1, y + 1, w - 2, h - 2, c)

    def blend(self, x, y, c, a):
        if not (0 <= x < self.w and 0 <= y < self.h):
            return
        o = self.px[y][x]
        if o[3] == 0:
            self.px[y][x] = q((c[0], c[1], c[2], int(a * 255)))
            return
        oa = o[3] / 255
        out = a + oa * (1 - a)
        ch = [round((c[i] * a + o[i] * oa * (1 - a)) / out) for i in range(3)]
        self.px[y][x] = q((ch[0], ch[1], ch[2], round(out * 255)))

    def glow(self, cx, cy, r, c, a):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                d = math.hypot(x - cx, y - cy) / r
                if d <= 1:
                    self.blend(x, y, c, a * (3 - math.floor(d * 3)) / 3)

    def grid(self, x, y, rows, palette):
        """Draws a text grid: each character is a pixel, looked up in
        `palette`; '.' and ' ' are transparent."""
        for dy, row in enumerate(rows):
            for dx, ch in enumerate(row):
                if ch in palette:
                    self.set(x + dx, y + dy, palette[ch])

    def save(self, path, scale=1):
        rows = []
        for row in self.px:
            line = []
            for p in row:
                line.extend(list(p) * scale)
            for _ in range(scale):
                rows.append(line)
        with open(path, "wb") as f:
            png.Writer(self.w * scale, self.h * scale, greyscale=False, alpha=True).write(f, rows)


def mix(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3)) + (255,)


def h(c):
    return ((c >> 16) & 255, (c >> 8) & 255, c & 255, 255)


def ramp(l, b, d):
    return {"light": h(l), "base": h(b), "dark": h(d)}


def hsh(a, b):
    x = (a * 0x9E3779B1 ^ b * 0x85EBCA77) & 0xFFFFFFFF
    x ^= x >> 15
    x = (x * 0x2C1B3C6D) & 0xFFFFFFFF
    return x ^ (x >> 12)


INK = h(0x181020)
BRICK = ramp(0xC06048, 0x983828, 0x602018)
STONE = ramp(0xE0D8C8, 0xB0A898, 0x787068)
WOOD = ramp(0xD89860, 0xA86838, 0x704020)
METAL = ramp(0xE8F0F8, 0xB0B8C8, 0x707888)
SHAFT = ramp(0x484058, 0x282030, 0x181020)
LEAF = ramp(0x78D058, 0x389838, 0x186028)
NEON = h(0xFF78C0)
LAMP = h(0xFFF0A0)
WARM = h(0xFFD890)
MOON = h(0xF8F0D0)
MOONLIGHT = h(0x5868B0)
SKY = [h(0x100828), h(0x201848), h(0x382868), h(0x583880), h(0x904878), h(0xD07058)]
WALLPAPER = [
    ramp(0xF8E8C8, 0xF0D0A8, 0xC8A880),
    ramp(0xD0F0E0, 0xB0D8C8, 0x88B0A0),
    ramp(0xE0D0F0, 0xC8B8E0, 0xA090C0),
    ramp(0xC8E0F8, 0xA8C8E8, 0x80A0C8),
]
BODY = h(0xD87858)
OUTLINE = h(0x401818)
ZZZ = h(0x28A8F8)
ALERT = h(0xF80048)
SPARK = h(0xF8E820)
QUESTION = h(0xC0C0C0)

FONT = {
    "0": ["###", "#.#", "#.#", "#.#", "###"],
    "1": [".#.", "##.", ".#.", ".#.", "###"],
    "2": ["###", "..#", "###", "#..", "###"],
    "3": ["###", "..#", ".##", "..#", "###"],
    "4": ["#.#", "#.#", "###", "..#", "..#"],
    "5": ["###", "#..", "###", "..#", "###"],
    "6": ["###", "#..", "###", "#.#", "###"],
    "7": ["###", "..#", "..#", ".#.", ".#."],
    "8": ["###", "#.#", "###", "#.#", "###"],
    "9": ["###", "#.#", "###", "..#", "###"],
    "H": ["#.#", "#.#", "###", "#.#", "#.#"],
    "E": ["###", "#..", "##.", "#..", "###"],
    "R": ["##.", "#.#", "##.", "#.#", "#.#"],
    "D": ["##.", "#.#", "#.#", "#.#", "##."],
    "L": ["#..", "#..", "#..", "#..", "###"],
    "O": ["###", "#.#", "#.#", "#.#", "###"],
    "B": ["##.", "#.#", "##.", "#.#", "##."],
    "Y": ["#.#", "#.#", ".#.", ".#.", ".#."],
    " ": ["...", "...", "...", "...", "..."],
}


def text(img, x, y, s, c, shadow=None):
    for i, ch in enumerate(s):
        for dy, row in enumerate(FONT[ch]):
            for dx, p in enumerate(row):
                if p == "#":
                    if shadow:
                        img.set(x + i * 4 + dx + 1, y + dy + 1, shadow)
                    img.set(x + i * 4 + dx, y + dy, c)


# Layout, as in the firmware at 1:1 with 33x36 per agent.
GROUND = 463
STOREY = 36
SLAB = 3
INNER = STOREY - SLAB
WALL, SHAFT_X, SHAFT_W = 4, 4, 14
ROOMS_X = SHAFT_X + SHAFT_W + 1
ROOMS_END = W - WALL
DESKS_X, SPACING, DESKS = ROOMS_X + 1, 33, 8


def storey_top(level):
    return GROUND - STOREY * (level + 1)


def desk_centre(i):
    return DESKS_X + SPACING * i + SPACING // 2


def sky(y):
    t = min(max((y // 3 * 3) / GROUND, 0), 1) ** 1.3 * (len(SKY) - 1)
    i = min(int(t), len(SKY) - 2)
    return mix(SKY[i], SKY[i + 1], t - i)


def backdrop(img):
    for y in range(H):
        img.fill(0, y, W, 1, sky(y))
    for y in range(0, 200):
        for x in range(W):
            r = hsh(x, y) % 300
            if r == 0:
                img.set(x, y, h(0xF8F8F8))
            elif r < 3:
                img.blend(x, y, h(0xC0C8F8), 0.6)
    for sx, sy in [(30, 40), (150, 22), (110, 120), (230, 150)]:
        img.set(sx, sy, h(0xFFFFFF))
        for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
            img.blend(sx + dx, sy + dy, h(0xC0C8F8), 0.7)
    cx, cy = 268, 62
    img.glow(cx, cy, 22, MOON, 0.16)
    for y in range(-9, 10):
        for x in range(-9, 10):
            d = x * x + y * y
            if d <= 81:
                bite = (x + 5) ** 2 + (y - 3) ** 2 <= 49
                if not bite:
                    img.set(cx + x, cy + y, MOON)
    for x, y in [(cx + 3, cy - 4), (cx + 6, cy + 1), (cx + 2, cy + 5)]:
        img.set(x, y, h(0xD8D0B0))


def bricks(img, x, y, w, hh):
    for yy in range(y, y + hh):
        for xx in range(x, x + w):
            row = yy // 3
            if yy % 3 == 2 or (xx + row * 3) % 6 == 0:
                c = BRICK["dark"]
            elif yy % 3 == 0:
                c = BRICK["light"]
            else:
                c = BRICK["base"]
            img.set(xx, yy, c)


def shell(img, top, paper, level, number=True):
    """Walls, shaft, wallpaper and floor of the storey whose top is `top`,
    with its number on the shaft if `number`."""
    rooms_w = ROOMS_END - ROOMS_X
    img.fill(ROOMS_X, top, rooms_w, INNER, paper["base"])
    img.fill(ROOMS_X, top, rooms_w, 2, paper["dark"])
    img.fill(ROOMS_X, top + INNER - 9, rooms_w, 9, paper["dark"])
    img.fill(ROOMS_X, top + INNER - 9, rooms_w, 1, paper["light"])
    bricks(img, 0, top, WALL, STOREY)
    bricks(img, ROOMS_END, top, WALL, STOREY)
    img.fill(0, top, 1, STOREY, INK)
    img.fill(W - 1, top, 1, STOREY, INK)
    for yy in range(INNER):
        img.fill(SHAFT_X, top + yy, SHAFT_W, 1, mix(SHAFT["base"], SHAFT["dark"], (yy // 2 * 2) / INNER))
    for rail in (SHAFT_X + 1, SHAFT_X + SHAFT_W - 2):
        img.fill(rail, top, 1, INNER, SHAFT["light"])
    img.fill(SHAFT_X + SHAFT_W // 2, top, 1, INNER, h(0x383040))
    img.fill(ROOMS_X - 1, top, 1, INNER, INK)
    img.fill(ROOMS_X - 1, top + INNER, rooms_w + 1, 1, WOOD["light"])
    img.fill(WALL, top + INNER + 1, W - 2 * WALL, 1, STONE["base"])
    img.fill(WALL, top + INNER + 2, W - 2 * WALL, 1, STONE["dark"])
    img.fill(SHAFT_X, top + INNER, SHAFT_W, 1, STONE["light"])
    if number:
        label = str(level)
        text(img, SHAFT_X + SHAFT_W // 2 - 2 * len(label) + 1, top + 2, label, h(0xF8B040), h(0x582810))


def window(img, cx, top, level, i):
    x0, y0, ww, wh = cx - 8, top + 4, 17, 17
    img.box(x0, y0, ww, wh, h(0xF8F0E8), INK)
    for y in range(y0 + 1, y0 + wh - 1):
        img.fill(x0 + 1, y, ww - 2, 1, sky((y - y0) * 9 + level * 7 + i * 5))
    img.fill(cx, y0 + 1, 1, wh - 2, h(0xD0C8C0))
    img.fill(x0 + 1, y0 + 8, ww - 2, 1, h(0xD0C8C0))
    if hsh(level, i) % 3 == 0:
        img.set(cx - 5, y0 + 3, h(0xF8F8F8))
    if hsh(i, level) % 4 == 0:
        img.set(cx + 4, y0 + 11, h(0xF8D060))
    img.fill(x0 - 1, y0 + wh, ww + 2, 1, h(0xE8E0D8))
    img.fill(x0 - 1, y0 + wh + 1, ww + 2, 1, INK)


def lamp_light(img, x, top):
    """A lit bulb at the fitting at `x`, and its cone of light."""
    img.fill(x - 1, top + 2, 3, 1, LAMP)
    for y in range(top + 3, top + INNER):
        half = 2 + (y - top) // 2
        for xx in range(x - half, x + half + 1):
            img.blend(xx, y, WARM, 0.12)


def lamps(img, top, lit):
    for i in range(1, DESKS):
        x = DESKS_X + SPACING * i
        img.set(x, top, INK)
        img.fill(x - 2, top + 1, 5, 1, INK)
        img.fill(x - 1, top + 2, 3, 1, h(0x605868))
        if lit:
            lamp_light(img, x, top)


def blob(img, cx, cy, r, ramp_, line=INK, speckle=0):
    """A round clump lit from the upper left, outlined: drawn back to front,
    overlapping clumps read as a bushy crown."""
    pts = set()
    for y in range(cy - r - 1, cy + r + 2):
        for x in range(cx - r - 1, cx + r + 2):
            if (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r * 0.6:
                pts.add((x, y))
    for (x, y) in pts:
        d = ((x - cx) + (y - cy)) / max(r, 1)
        c = ramp_["light"] if d < -0.6 else ramp_["dark"] if d > 0.5 else ramp_["base"]
        if speckle and hsh(x * 7, y * 13) % speckle == 0:
            c = ramp_["dark"] if c != ramp_["dark"] else ramp_["base"]
        img.set(x, y, c)
    for (x, y) in pts:
        for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
            if (x + dx, y + dy) not in pts:
                img.set(x + dx, y + dy, line)


POT = ramp(0xE08050, 0xB85838, 0x803020)


def pot(img, cx, floor, w, hh):
    """A terracotta pot `w` wide at its rim, standing on `floor`."""
    img.fill(cx - w // 2 - 1, floor - hh - 2, w + 2, 3, INK)
    img.fill(cx - w // 2, floor - hh - 1, w, 1, POT["light"])
    for k in range(hh):
        y = floor - hh + k
        inset = k * 2 // hh
        x0, x1 = cx - w // 2 + 1 + inset, cx + (w + 1) // 2 - 1 - inset
        img.set(x0 - 1, y, INK)
        img.set(x1, y, INK)
        img.fill(x0, y, x1 - x0, 1, POT["base"])
        img.set(x1 - 1, y, POT["dark"])
        img.set(x0, y, POT["light"])
    img.fill(cx - w // 2 + 1, floor - 1, w - 2, 1, INK)
    img.fill(cx - w // 2 + 1, floor - hh - 1, w - 2, 1, h(0x402818))


def plant(img, cx, floor, size=1.0):
    """A potted ficus: a pot, a trunk, and a crown of leaf clumps."""
    s = size
    trunk_top = floor - int(16 * s)
    for y in range(trunk_top, floor - 8):
        img.fill(cx - 1, y, 3, 1, INK)
        img.set(cx, y, WOOD["dark"])
    img.set(cx + 1, trunk_top + 3, INK)
    img.set(cx + 2, trunk_top + 2, INK)
    clumps = [(-5, -20, 4), (5, -21, 4), (0, -25, 5), (-6, -14, 4), (6, -15, 4), (-1, -17, 5), (2, -12, 3)]
    for (dx, dy, r) in clumps:
        blob(img, cx + int(dx * s), floor + int(dy * s), max(2, int(r * s)), LEAF, speckle=9)
    pot(img, cx, floor, int(10 * s) | 1, int(7 * s))


def decor(img, top, kind):
    """Something in the spare room at the storey's right end."""
    x = DESKS_X + SPACING * DESKS + 4
    floor = top + INNER
    if kind == 0:  # bookshelf
        bookshelf(img, x, floor)
    elif kind == 1:
        water_cooler(img, x, floor)
    elif kind == 2:
        plant(img, x + 10, floor)
    else:
        coffee_machine(img, x, floor)


BOOKS = [h(0xC83838), h(0x3870C8), h(0xE8B838), h(0x48A048), h(0x8858C0), h(0xE07830), h(0x2898A0)]


def bookshelf(img, x, floor):
    w, hh = 22, 27
    t = floor - hh
    img.box(x, t, w, hh, h(0x583018), INK)
    img.fill(x + 1, t + 1, 2, hh - 2, WOOD["dark"])
    img.fill(x + w - 3, t + 1, 2, hh - 2, WOOD["dark"])
    img.fill(x + 1, t + 1, w - 2, 1, WOOD["light"])
    shelves = [t + 9, t + 17, floor - 2]
    for sy in shelves:
        img.fill(x + 1, sy, w - 2, 1, WOOD["light"])
        img.fill(x + 1, sy + 1, w - 2, 1, WOOD["base"])
    for n, sy in enumerate(shelves):
        bx = x + 3
        end = x + w - 3 - (4 if n == 1 else 0)
        while bx < end - 1:
            bw = 2 + hsh(bx, n) % 2
            bh = 5 + hsh(n, bx) % 3
            if bx + bw > end:
                break
            c = BOOKS[hsh(bx * 3, sy) % len(BOOKS)]
            dark = tuple(v * 3 // 4 for v in c[:3]) + (255,)
            img.fill(bx, sy - bh, bw, bh, c)
            img.fill(bx, sy - bh, bw, 1, tuple(min(255, v + 48) for v in c[:3]) + (255,))
            img.fill(bx, sy - bh + bh * 2 // 3, bw, 1, dark)
            img.set(bx + bw - 1, sy - bh + 1, dark)
            bx += bw + (1 if hsh(sy, bx) % 5 == 0 else 0)
        if n == 1:
            # A book leaning on a metal bookend.
            for k in range(6):
                img.fill(end - 1 + k // 2, sy - 1 - k, 2, 1, BOOKS[1])
            img.fill(end + 2, sy - 4, 1, 4, METAL["base"])
    # On top: a small plant and a vase.
    blob(img, x + 6, t - 4, 3, LEAF)
    img.box(x + 4, t - 2, 5, 3, POT["base"], INK)
    img.box(x + 14, t - 6, 4, 7, h(0x5878C8), INK)
    img.set(x + 15, t - 5, h(0xA0B8F0))
    img.set(x + 15, t - 8, h(0xF87068))
    img.set(x + 16, t - 9, h(0xF8D048))


def water_cooler(img, x, floor):
    # Cabinet with hot and cold taps and a drip tray.
    cx0, cw, ch = x + 5, 12, 15
    ct = floor - ch
    img.box(cx0, ct, cw, ch, METAL["light"], INK)
    img.fill(cx0 + cw - 2, ct + 1, 1, ch - 2, METAL["base"])
    img.fill(cx0 + 1, ct + 6, cw - 2, 1, METAL["base"])
    img.set(cx0 + 3, ct + 3, h(0xF84838))
    img.set(cx0 + 8, ct + 3, h(0x3870C8))
    img.fill(cx0 + 2, ct + 5, cw - 4, 1, h(0x383040))
    img.fill(cx0 + 2, floor - 3, cw - 4, 1, METAL["base"])
    # The bottle, upside down: neck into the cabinet, water nearly full.
    bx, bw, bh = cx0 + 1, 10, 12
    bt = ct - bh
    img.fill(bx + 3, ct - 1, 4, 2, h(0x3870C8))
    img.box(bx, bt, bw, bh, h(0x70B0E8), INK)
    img.fill(bx + 1, bt + 1, bw - 2, 2, h(0xC0E0F8))
    img.fill(bx + 1, bt + 3, bw - 2, 1, h(0x4890D0))
    img.fill(bx + 1, bt + 4, 1, bh - 6, h(0xD8F0F8))
    img.fill(bx + bw - 2, bt + 3, 1, bh - 4, h(0x4890D0))
    for (px, py) in [(4, 6), (6, 8), (5, 10)]:
        img.set(bx + px, bt + py, h(0xF0F8F8))
    # Paper cups on the side.
    img.box(cx0 + cw, ct + 2, 4, 7, h(0xF8F8F8), INK)
    img.fill(cx0 + cw + 1, ct + 3, 2, 1, h(0xD8D8D0))


def coffee_machine(img, x, floor):
    # Cabinet with two doors.
    img.box(x + 1, floor - 12, 20, 12, WOOD["base"], INK)
    img.fill(x + 2, floor - 11, 18, 1, WOOD["light"])
    img.fill(x + 11, floor - 10, 1, 9, INK)
    img.set(x + 9, floor - 6, METAL["light"])
    img.set(x + 13, floor - 6, METAL["light"])
    # The machine: dark body, display, buttons, a carafe of coffee.
    mx, mt = x + 4, floor - 25
    img.box(mx, mt, 14, 13, h(0x383040), INK)
    img.fill(mx + 1, mt + 1, 12, 1, h(0x585068))
    img.fill(mx + 2, mt + 3, 4, 2, h(0x204828))
    img.fill(mx + 3, mt + 3, 2, 1, LIME_)
    img.set(mx + 9, mt + 3, h(0xF84838))
    img.set(mx + 11, mt + 3, h(0xF8B040))
    img.fill(mx + 5, mt + 6, 4, 1, METAL["base"])
    img.set(mx + 6, mt + 7, METAL["dark"])
    img.box(mx + 4, mt + 8, 6, 5, h(0xD0D8E0), INK)
    img.fill(mx + 5, mt + 10, 4, 2, h(0x603018))
    img.set(mx + 5, mt + 9, h(0xF0F8F8))
    img.set(mx + 10, mt + 9, INK)
    img.set(mx + 10, mt + 10, INK)
    for k, (dx, dy) in enumerate([(7, -2), (8, -3), (7, -5), (8, -6)]):
        img.blend(mx + dx, mt + dy, h(0xFFFFFF), 0.45 - k * 0.08)


LIME_ = h(0x48D848)


# The mascot: the terminal's block-character art, each character cell 2x2
# quadrants and each quadrant a pixel one wide and two tall, as on the
# device (18x12, 20x14 with its outline). Flat colour, dark outline; the eyes
# are gaps the outline fills.
ART = [" \u2590\u259b\u2588\u2588\u2588\u259c\u258c", "\u259d\u259c\u2588\u2588\u2588\u2588\u2588\u259b\u2598", "  \u2598\u2598 \u259d\u259d"]
QUADRANTS = {"\u2598": 1, "\u259d": 2, "\u258c": 5, "\u2590": 10, "\u259b": 7, "\u259c": 11, "\u2588": 15}
ARM_ROW, LEFT_ARM, RIGHT_ARM = 2, (1, 2), (15, 16)
LEG_ROW, LEGS_STANDING, LEGS_STRIDE = 4, (4, 6, 11, 13), (5, 7, 10, 12)
EYE_ROW, EYES_FORWARD = 1, (5, 12)


def cells():
    """The art as an 18x6 grid of booleans."""
    g = [[False] * 18 for _ in range(6)]
    for row, line in enumerate(ART):
        for col, ch in enumerate(line):
            qd = QUADRANTS.get(ch, 0)
            for bit, (dx, dy) in enumerate([(0, 0), (1, 0), (0, 1), (1, 1)]):
                if qd & (1 << bit):
                    g[row * 2 + dy][col * 2 + dx] = True
    return g


def mascot(arm_l="down", arm_r="down", eyes="open", lift=0, stride=False):
    """The mascot's body and eye pixels, as sets of (x, y) in a 22x18 cell:
    grid cell (c, r) of the art covers pixels (c + 2, 2r + 2 - 2 lift) and
    the one below."""
    g = cells()
    for x in LEFT_ARM + RIGHT_ARM:
        g[ARM_ROW][x] = False
    for x in LEGS_STANDING:
        g[LEG_ROW][x] = False
    for pose, xs in ((arm_l, LEFT_ARM), (arm_r, RIGHT_ARM)):
        for x in xs:
            g[ARM_ROW - 1 if pose == "up" else ARM_ROW][x] = True
    for x in LEGS_STRIDE if stride else LEGS_STANDING:
        g[LEG_ROW][x] = True
    body, eye = set(), set()
    for r in range(6):
        for c in range(18):
            px, py = c + 2, 2 * r + 2 - 2 * lift
            is_eye = r == EYE_ROW and c in EYES_FORWARD
            if is_eye and eyes == "closed":
                body.update({(px, py), (px, py + 1)})
            elif is_eye and eyes == "drowsy":
                body.add((px, py))
                eye.add((px, py + 1))
            elif is_eye:
                eye.update({(px, py), (px, py + 1)})
            elif g[r][c]:
                body.update({(px, py), (px, py + 1)})
    return body, eye


def draw_mascot(img, x, y, pose):
    body, eye = pose
    for (px, py) in body:
        img.set(x + px, y + py, BODY)
    for (px, py) in eye:
        img.set(x + px, y + py, OUTLINE)
    for (px, py) in body:
        for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]:
            n = (px + dx, py + dy)
            if n not in body and n not in eye:
                img.set(x + n[0], y + n[1], OUTLINE)


Z = ["###", ".#.", "###"]
BANG = ["#", "#", "#", ".", "#"]
QMARK = ["###", "..#", ".##", "...", ".#."]


def sign(img, x, y, rows, c):
    """A marker (zzz, !, ?, a sparkle), with a dark border all round so it
    stands out against the wall and the windows."""
    pts = {(x + dx, y + dy) for dy, row in enumerate(rows) for dx, p in enumerate(row) if p == "#"}
    for (px, py) in pts:
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                if (px + dx, py + dy) not in pts:
                    img.set(px + dx, py + dy, INK)
    for (px, py) in pts:
        img.set(px, py, c)


def desk(img, cx, top, lit, i, occupied):
    """A workstation in front of the agent's right half: a solid desk in
    oblique projection, receding to the upper left, so its top shows as a
    strip set back that way and its left side shows. On it, a monitor seen
    from behind, and papers or a mug if nobody works there."""
    floor = top + INNER
    depth = 2
    # Front panel from the mascot's middle (plus the side face) to past the
    # monitor: the mascot's left side stays in view.
    dx0, dw = cx - 3, 16
    front = floor - 4
    # Back on the desk: one pixel left per row of the top's depth.
    mx = cx + 3 - depth
    base = front - depth
    # Top surface: each row further back sits one pixel further left.
    for k in range(depth):
        y = front - 1 - k
        x0 = dx0 - k
        img.fill(x0 + 1, y, dw - 1, 1, WOOD["light"])
        img.set(x0, y, INK)
        img.set(x0 + dw, y, INK)
    img.fill(dx0 - depth, front - depth - 1, dw + 1, 1, INK)
    # Left side face, then the front panel down to the floor.
    for k in range(1, depth + 1):
        img.fill(dx0 - k, front - k + 1, 1, floor - front + k - 1, WOOD["dark"])
    img.fill(dx0 - depth, front - depth, 1, floor - front + depth, INK)
    img.box(dx0, front, dw, floor - front, WOOD["base"], INK)
    img.fill(dx0 + 1, front, dw - 2, 1, WOOD["dark"])
    img.fill(dx0 + 5, front + 2, 8, 1, WOOD["dark"])
    # Monitor on a short stand standing on the back row of the top.
    img.box(mx, base - 8, 9, 7, METAL["base"], INK)
    img.fill(mx + 1, base - 7, 7, 1, METAL["light"])
    img.set(mx + 4, base - 4, METAL["dark"])
    img.fill(mx + 3, base - 1, 3, 2, INK)
    if i % 3 == 0 and not occupied:
        img.box(dx0, base - 4, 4, 4, h(0xF8F8F8), INK)
        img.set(dx0 - 1, base - 3, INK)
        img.blend(dx0 + 1, base - 6, h(0xFFFFFF), 0.4)
    elif i % 3 == 1 and not occupied:
        img.fill(dx0, base, 3, 1, h(0xF0F0E8))


def agent(img, cx, top, state, frame=0):
    """An agent at the desk centred on `cx`: the mascot behind the desk, with
    its state's sign."""
    floor = top + INNER
    # Behind the desk, left of the monitor, high enough to see over it.
    x = cx - 15
    y = floor - 16
    poses = {
        "working": [mascot("up", "down", stride=True), mascot("down", "up")],
        "idle": [mascot(eyes="drowsy")],
        "awake": [mascot()],
        "blocked": [mascot(arm_r="up")],
        "done": [mascot("up", "up", lift=2)],
        "unknown": [mascot()],
    }
    pose = poses[state][frame % len(poses[state])]
    draw_mascot(img, x, y, pose)
    if state == "idle":
        sign(img, x + 20, y - 2, Z, ZZZ)
        sign(img, x + 23, y - 6, Z, ZZZ)
    elif state == "blocked":
        sign(img, x + 22, y - 4, BANG, ALERT)
    elif state == "done":
        for sx, sy in [(0, -1), (21, 0), (0, 8), (21, 7)]:
            sign(img, x + sx, y + sy, ["#", "#"], SPARK)
    elif state == "unknown":
        sign(img, x + 21, y - 3, QMARK, QUESTION)


PLUMBOB = ["..#..", ".#L#.", ".LG#.", "#LGD#", ".GD#.", ".#D#.", "..#.."]


def storey(img, level, paper, agents, focus=None):
    top = storey_top(level)
    # The focused agent is awake, and keeps its storey's lights on.
    if focus is not None and agents[focus] == "idle":
        agents = agents[:focus] + ["awake"] + agents[focus + 1 :]
    lit = focus is not None or any(s != "idle" for s in agents)
    p = WALLPAPER[paper % len(WALLPAPER)]
    shell(img, top, p, level)
    for i in range(DESKS):
        window(img, desk_centre(i), top, level, i)
    decor(img, top, level % 4)
    for i, state in enumerate(agents):
        agent(img, desk_centre(i), top, state, frame=i)
    for i in range(DESKS):
        desk(img, desk_centre(i), top, lit, i + level, i < len(agents))
    if lit:
        lamps(img, top, True)
    else:
        for y in range(top, top + INNER + 1):
            for x in range(ROOMS_X, ROOMS_END):
                c = img.get(x, y)
                img.set(x, y, tuple(c[k] * MOONLIGHT[k] // 255 for k in range(3)) + (255,))
        lamps(img, top, False)
        for i in range(DESKS):
            cx = desk_centre(i)
            for y in range(top + 22, top + INNER):
                s = (y - top - 22) // 2
                for x in range(cx - 7 + s, cx + 8 + s):
                    img.blend(x, y, h(0xA0B0F8), 0.12)
    if focus is not None:
        cx = desk_centre(focus)
        img.grid(cx - 6, top + 4, PLUMBOB, {"#": h(0x084018), "L": h(0xB8FFA8), "G": h(0x48D848), "D": h(0x209030)})


def lobby(img, number=True):
    top = storey_top(0)
    p = ramp(0xF0E8D8, 0xD8D0C0, 0xB0A898)
    shell(img, top, p, 0, number)
    floor = top + INNER
    for x in range(ROOMS_X, ROOMS_END):
        img.set(x, floor, h(0xF8F0E8) if (x // 4) % 2 == 0 else h(0xC8C0B8))
    # Directory board in a wooden frame: a title and a line per floor.
    bx, by = ROOMS_X + 6, top + 13
    img.box(bx - 1, by - 1, 26, 15, WOOD["base"], INK)
    img.box(bx + 1, by + 1, 22, 11, h(0x302838), INK)
    img.fill(bx + 3, by + 3, 12, 1, h(0xF8D048))
    for r in range(3):
        img.set(bx + 3, by + 5 + r * 2, BOOKS[r])
        img.fill(bx + 5, by + 5 + r * 2, 8 + hsh(r, 3) % 8, 1, h(0xD0C8B8))
    # Reception: a counter with panels, a bell, a screen and a vase.
    rx, rw, rh = 60, 44, 13
    rt = floor - rh
    img.box(rx - 1, rt - 1, rw + 2, 3, WOOD["light"], INK)
    img.box(rx, rt + 1, rw, rh - 1, WOOD["base"], INK)
    for k in range(3):
        img.box(rx + 3 + k * 14, rt + 4, 11, rh - 7, WOOD["base"], WOOD["dark"])
        img.fill(rx + 4 + k * 14, rt + 5, 9, 1, WOOD["light"])
    img.fill(rx + 1, floor - 2, rw - 2, 1, WOOD["dark"])
    img.box(rx + 6, rt - 9, 9, 8, METAL["base"], INK)
    img.fill(rx + 7, rt - 8, 7, 1, METAL["light"])
    img.fill(rx + 9, rt - 1, 3, 1, INK)
    img.fill(rx + 26, rt - 3, 4, 2, h(0xF8D048))
    img.set(rx + 27, rt - 3, h(0xFFF8C0))
    img.set(rx + 27, rt - 4, h(0xF8D048))
    img.fill(rx + 25, rt - 1, 6, 1, h(0xB89030))
    img.box(rx + 36, rt - 7, 5, 6, h(0x5878C8), INK)
    img.set(rx + 37, rt - 6, h(0xA0B8F0))
    for (fx, fy, fc) in [(37, -9, h(0xF87068)), (39, -10, h(0xF8D048)), (38, -11, h(0xF87068))]:
        img.set(rx + fx, rt + fy, fc)
    img.fill(rx + 38, rt - 8, 1, 1, LEAF["base"])
    # Plants by the reception and the doors.
    plant(img, 120, floor, 1.15)
    plant(img, 232, floor, 1.15)
    # A bench with slats on metal legs.
    bx0, bw = 140, 34
    for k in range(2):
        img.fill(bx0, floor - 9 + k * 2, bw, 1, INK)
        img.fill(bx0 + 1, floor - 8 + k * 2, bw - 2, 1, WOOD["light"] if k == 0 else WOOD["base"])
    img.fill(bx0, floor - 5, bw, 1, INK)
    for lx in (bx0 + 3, bx0 + bw - 5):
        img.fill(lx, floor - 4, 2, 4, METAL["dark"])
        img.fill(lx, floor - 1, 2, 1, INK)
    # Glass doors under a striped awning.
    door_x = 250
    img.box(door_x, top + 9, 40, INNER - 9, METAL["dark"], INK)
    for dx in (2, 21):
        img.box(door_x + dx, top + 11, 17, INNER - 11, h(0x78B8E8), INK)
        for y in range(top + 12, floor - 1):
            img.fill(door_x + dx + 1, y, 15, 1, mix(h(0xA8D8F8), h(0x5890C8), (y - top - 12) / (INNER - 13)))
        for k in range(5):
            img.set(door_x + dx + 3 + k, top + 18 - k, h(0xE0F0F8))
        img.fill(door_x + dx + (14 if dx == 2 else 2), top + 22, 1, 4, METAL["light"])
    for dx in range(46):
        stripe = ramp(0xF87068, 0xD83838, 0x902020) if (dx // 5) % 2 == 0 else ramp(0xFFFFFF, 0xF0E8E0, 0xC0B8B0)
        for k, part in enumerate(("light", "base", "base", "dark")):
            img.set(door_x - 3 + dx, top + 3 + k, stripe[part])
    img.fill(door_x - 3, top + 7, 46, 1, INK)
    lamps(img, top, True)


def street(img):
    img.fill(0, GROUND, W, 1, STONE["light"])
    for y in range(GROUND + 1, GROUND + 5):
        img.fill(0, y, W, 1, mix(STONE["base"], STONE["dark"], (y - GROUND - 1) / 3))
    for x in range(0, W, 8):
        img.fill(x, GROUND + 1, 1, 4, STONE["dark"])
    for y in range(GROUND, GROUND + 5):
        for x in range(244, 296):
            img.blend(x, y, WARM, 0.25)
    img.fill(0, GROUND + 5, W, 1, h(0x383040))
    for y in range(GROUND + 6, H):
        img.fill(0, y, W, 1, mix(h(0x403850), h(0x282030), (y - GROUND - 6) / (H - GROUND - 7)))
    for x in range(2, W, 14):
        img.fill(x, GROUND + 11, 7, 1, h(0xF8D048))
        img.fill(x, GROUND + 12, 7, 1, h(0xB89030))


def roof(img, top_storey, light=True):
    t = storey_top(top_storey) - 13
    hgt = 14
    sign_x = 140
    for y in range(t, t + hgt - 3):
        for x in range(sign_x - 4, sign_x + 30):
            img.blend(x, y, NEON, 0.12)
    img.box(0, t + hgt - 4, W, 4, STONE["base"], INK)
    img.fill(1, t + hgt - 3, W - 2, 1, STONE["light"])
    img.box(14, t, 15, 9, WOOD["base"], INK)
    img.fill(15, t + 1, 3, 7, WOOD["light"])
    img.fill(25, t + 1, 3, 7, WOOD["dark"])
    img.fill(15, t + 3, 13, 1, METAL["dark"])
    img.fill(15, t + 6, 13, 1, METAL["dark"])
    img.fill(16, t + 9, 1, 1, INK)
    img.fill(26, t + 9, 1, 1, INK)
    img.box(60, t + 4, 16, 7, METAL["base"], INK)
    for k in range(3):
        img.fill(62 + k * 4, t + 6, 2, 3, METAL["dark"])
    img.fill(sign_x + 6, t + 9, 1, 1, INK)
    img.fill(sign_x + 20, t + 9, 1, 1, INK)
    img.box(sign_x, t + 1, 27, 8, h(0x281828), INK)
    text(img, sign_x + 4, t + 2, "HERDR", NEON)
    ax = W - 24
    img.fill(ax, t - 6, 1, hgt + 2, METAL["base"])
    img.fill(ax - 2, t - 2, 5, 1, METAL["base"])
    if light:
        img.glow(ax, t - 7, 3, h(0xFF4040), 0.35)
    img.fill(ax - 1, t - 8, 3, 2, h(0xFF4040) if light else h(0x702020))


def car(img, level):
    top = storey_top(level)
    x, w, hh = SHAFT_X + 1, SHAFT_W - 2, 24
    y = top + INNER - hh
    img.box(x, y, w, hh, METAL["base"], INK)
    img.fill(x + 1, y + 1, w - 2, 2, METAL["dark"])
    img.fill(x + w // 2 - 1, y + 1, 2, 1, h(0xF8B040))
    for yy in range(y + 4, y + hh - 1):
        img.fill(x + 2, yy, w - 4, 1, mix(METAL["light"], METAL["dark"], (yy - y - 4) / (hh - 6)))
    img.fill(x + w // 2, y + 4, 1, hh - 5, INK)
    img.fill(x + 3, y + 5, 1, 6, h(0xFFFFFF))


def arrow(img, x, y, up):
    n = 14
    img.box(x, y, n, n, METAL["base"], INK)
    img.fill(x + 1, y + 1, n - 2, 1, METAL["light"])
    img.fill(x + 1, y + 1, 1, n - 2, METAL["light"])
    img.fill(x + 1, y + n - 2, n - 2, 1, METAL["dark"])
    img.fill(x + n - 2, y + 1, 1, n - 2, METAL["dark"])
    for cx, cy in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1)]:
        img.px[y + cy][x + cx] = sky(y)
    for r in range(5):
        yy = y + (4 + r if up else n - 5 - r)
        img.fill(x + n // 2 - 1 - r, yy, 2 + 2 * r, 1, h(0xC07818) if r == 4 else h(0xF8B040))


def touch_areas(img, storeys, arrows):
    """Outlines what each tap hits, as the firmware works it out: a desk's
    whole 33x36 cell on storeys above the lobby, and each arrow button with
    10 pixels around it. Cells alternate two colours so neighbours show."""
    colours = [h(0x00F8F8), h(0xF800F8)]
    for level in range(1, storeys + 1):
        top = storey_top(level)
        for i in range(DESKS):
            x0 = DESKS_X + SPACING * i
            c = colours[(i + level) % 2]
            for y in range(top, top + STOREY):
                for x in range(x0, x0 + SPACING):
                    img.blend(x, y, c, 0.18)
            for x in range(x0, x0 + SPACING):
                img.set(x, top, c)
                img.set(x, top + STOREY - 1, c)
            for y in range(top, top + STOREY):
                img.set(x0, y, c)
                img.set(x0 + SPACING - 1, y, c)
    for (ax, ay) in arrows:
        reach = 10
        x0, y0, n = ax - reach, ay - reach, 14 + 2 * reach
        for y in range(y0, y0 + n):
            for x in range(x0, x0 + n):
                img.blend(x, y, h(0xF8F800), 0.25)
        for k in range(n):
            for (x, y) in [(x0 + k, y0), (x0 + k, y0 + n - 1), (x0, y0 + k), (x0 + n - 1, y0 + k)]:
                img.set(x, y, h(0xF8F800))


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "."
    img = Img(W, H)
    backdrop(img)
    street(img)
    lobby(img)
    storeys = [
        (0, ["working", "working", "blocked", "idle", "working", "done"], 1),
        (1, ["done", "working", "unknown", "working", "idle", "working", "working", "blocked"], None),
        (2, ["idle", "idle", "idle", "idle"], None),
        (3, ["working", "blocked", "working", "working", "idle"], None),
        (0, ["unknown", "working", "done"], None),
        (1, ["working", "idle", "working", "working", "working", "working", "done", "working"], None),
        (2, ["blocked", "working"], None),
        (3, ["working", "idle", "working"], None),
        (0, ["working", "working", "idle", "done", "working"], None),
    ]
    for k, (paper, agents, focus) in enumerate(storeys):
        storey(img, k + 1, paper, agents, focus)
    roof(img, len(storeys))
    car(img, 1)
    arrow(img, SHAFT_X, 6, True)
    img.save(os.path.join(out, "demo.png"))
    img.save(os.path.join(out, "demo-2x.png"), 2)
    img.save(os.path.join(out, "demo-3x.png"), 3)
    touch_areas(img, len(storeys), [(SHAFT_X, 6)])
    img.save(os.path.join(out, "demo-touch.png"))
    img.save(os.path.join(out, "demo-touch-3x.png"), 3)


main()
