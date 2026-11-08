"""Diwali: fireworks over water, written in letters.

rajeet-04, 2026. Free for personal projects, see LICENSE.txt.

    uv run --with-requirements requirements.txt diwali.py

Keys: Space pause, R replay, S save a still, Esc quit.
"""

import sys
import time
from pathlib import Path

import numpy as np
import pygame
from PIL import Image, ImageDraw, ImageFont

TAU = np.pi * 2
LOOP = 26.0         # length of one run (s)
G_UP = 4.0          # gravity on a climbing shell
G = 1.1             # gravity on burning stars
DRAG = 0.85         # air drag on stars (1/s)

# glyph tiers from faint to dense; a cell picks its tier by brightness
TIERS = (".,'`:;", "~-=+<>^\"", "ocaesuzrv", "#%&@$MW8")
GLYPHS = "".join(dict.fromkeys(" " + "".join(TIERS) + "*|/\\_-:.~+"))
CODE = {g: i for i, g in enumerate(GLYPHS)}
TIER_LUT = np.array([[CODE[tier[k % len(tier)]] for k in range(8)] for tier in TIERS])

STAR, CITY, WINDOW, GOLD, SAFFRON, RED, MAGENTA, GREEN, BLUE, WHITE, SILVER, HAPPY, DIWALI = range(13)
POINT, STREAK, WATER = range(3)
PEONY, CHRYS, RING, WILLOW, PALM = range(5)


def ease(x):
    # cosine ease, 0 -> 1 over x in [0, 1]
    return 0.5 - 0.5 * np.cos(np.pi * np.clip(x, 0.0, 1.0))


# flight

def drag_path(c, v, tau, k=DRAG, g=G):
    # exact flight under linear drag and gravity:  dv/dt = -k v - g y
    tau = np.maximum(tau, 0.0)[:, None]
    e = np.exp(-k * tau)
    p = c + v * (1 - e) / k
    vel = v * e
    p[:, 1] -= (g / k * tau - g / k ** 2 * (1 - e))[:, 0]
    vel[:, 1] -= g / k * (1 - e)[:, 0]
    return p, vel


def fib_sphere(n):
    # n evenly spread directions: golden-angle spiral down a sphere
    i = np.arange(n) + 0.5
    y = 1 - 2 * i / n
    r = np.sqrt(1 - y * y)
    a = i * np.pi * (3 - np.sqrt(5))
    return np.stack([np.cos(a) * r, y, np.sin(a) * r], 1)


def ring_dirs(n, normal):
    normal = normal / np.linalg.norm(normal)
    u = np.cross(normal, [0.3, 1.0, 0.2])
    u /= np.linalg.norm(u)
    w = np.cross(normal, u)
    a = np.linspace(0, TAU, n, endpoint=False)
    return np.cos(a)[:, None] * u + np.sin(a)[:, None] * w


# the show: every shell is planned once, then evaluated at any time t

KIND = {            # life, trail samples, trail spacing, drag, gravity scale
    PEONY: (2.0, 2, 0.05, 0.85, 1.0),
    CHRYS: (2.3, 6, 0.055, 0.85, 1.0),
    RING: (1.9, 2, 0.05, 0.9, 1.0),
    WILLOW: (3.6, 10, 0.10, 1.6, 0.7),
    PALM: (2.6, 7, 0.07, 1.1, 1.0),
}
PALETTE = [(GOLD, SAFFRON), (MAGENTA, WHITE), (GREEN, GOLD), (BLUE, SILVER),
           (RED, GOLD), (SAFFRON, RED), (WHITE, MAGENTA)]


def stars_of(kind, rng, tilt, pal):
    col, col2 = pal
    if kind == RING:
        ring, core = ring_dirs(120, tilt), 0.42 * fib_sphere(70)
        d = np.concatenate([ring, core])
        base = np.r_[np.full(120, col), np.full(70, col2)]
        late = np.r_[np.full(120, col2), np.full(70, col)]
        return d, base, late
    if kind == PALM:
        arms = fib_sphere(14)
        arms = arms[arms[:, 1] > -0.2]
        d = np.repeat(arms, 18, 0) * np.tile(np.linspace(0.55, 1.0, 18), len(arms))[:, None]
    else:
        d = fib_sphere({PEONY: 240, CHRYS: 170, WILLOW: 150}[kind])
    d = d * (1 + 0.10 * rng.normal(size=(len(d), 1)))
    return d, np.full(len(d), col), np.full(len(d), SILVER if kind == WILLOW else col2)


def make_shell(t0, x, z, h, kind, pal, speed, rng):
    vy = np.sqrt(2 * G_UP * h)                  # just enough to stop at height h
    s = dict(t0=t0, x=x, z=z, kind=kind, pal=pal, speed=speed, vy=vy, drift=rng.uniform(-0.35, 0.35),
             shed=rng.random((16, 4)), tr=vy / G_UP)
    s["tb"] = t0 + s["tr"]
    s["top"] = np.array([x + s["drift"] * s["tr"], h, z])
    if kind is not None:
        s["dirs"], s["base"], s["late"] = stars_of(kind, rng, rng.normal(size=3), pal)
        s["rand"] = rng.random(len(s["dirs"]))
    return s


def program():
    rng = np.random.default_rng(7)
    shells, t = [], 0.9
    while t < 14.4:
        busy = 1 + 1.6 * ease((t - 6) / 8)    # the show builds up
        shells.append(make_shell(t, rng.uniform(-4.5, 4.5), rng.uniform(-5.0, 1.5), rng.uniform(3.6, 6.2),
                                 int(rng.integers(5)), PALETTE[rng.integers(len(PALETTE))],
                                 rng.uniform(1.6, 2.4), rng))
        t += rng.uniform(0.55, 1.3) / busy
    for i in range(9):                          # finale: a fan of shells together
        shells.append(make_shell(14.6 + 0.11 * i, -5 + 10 * i / 8, -3.5 + rng.uniform(-1, 1), 4.6 + 1.2 * np.sin(i),
                                 (CHRYS, PEONY, WILLOW)[i % 3], PALETTE[i % len(PALETTE)], 2.2, rng))
    for i in range(6):                          # side shots while the greeting hangs
        side = -1 if i % 2 else 1
        shells.append(make_shell(19.0 + 0.55 * i, side * rng.uniform(4.2, 5.6), rng.uniform(-4, -1),
                                 rng.uniform(2.8, 3.6), (PEONY, RING)[i % 2], PALETTE[(i * 3) % len(PALETTE)],
                                 1.5, rng))
    return shells


def climb(s, u):
    p = np.stack([s["x"] + s["drift"] * u, s["vy"] * u - 0.5 * G_UP * u ** 2, np.full_like(u, s["z"])], -1)
    v = np.stack([np.full_like(u, s["drift"]), s["vy"] - G_UP * u, np.zeros_like(u)], -1)
    return p, v


def rocket(s, a, out):
    # the climbing shell: a hot head, a fading gold tail, sparks shed behind
    j = np.arange(14)
    u = a - j * 0.03
    keep = u > 0
    p, v = climb(s, u[keep])
    out.add(p[:1], v[:1], 1.2, WHITE, POINT)
    out.add(p[1:], v[1:], (1 - j[keep][1:] / 14) ** 1.3, GOLD, STREAK)
    r = s["shed"]
    age = ((a * 2.5 + r[:, 0]) % 1.0) * 0.6
    born = a - age
    ok = born > 0
    p0, _ = climb(s, born[ok])
    q, vel = drag_path(p0, (r[ok, 1:] - 0.5) * [0.5, 0.3, 0.5], age[ok], 2.0, 1.4)
    out.add(q, vel, 0.6 * (1 - age[ok] / 0.6), SAFFRON, POINT)


def burst(s, b, out):
    life, nt, dt, k, gk = KIND[s["kind"]]
    v0, r = s["dirs"] * s["speed"], s["rand"]
    span = life * (0.75 + 0.5 * r)
    for j in range(nt):
        tau = b - j * dt
        if tau <= 0:
            break
        p, vel = drag_path(s["top"], v0, np.full(len(v0), tau), k, G * gk)
        fade = np.clip(1 - tau / span, 0, 1) ** 1.4
        if j == 0:
            late = tau > 0.7 * span
            strobe = ((r * 53 + tau * 23) % 1) > 0.45          # crackle as the stars burn out
            shade = fade * (1 + 1.8 * np.exp(-tau * 9)) * np.where(late, strobe * 1.4, 1)
            out.add(p, vel, shade, np.where(tau > 0.55 * span, s["late"], s["base"]), POINT)
        else:
            out.add(p, vel, 1.05 * fade * (1 - j / nt) ** 1.1, s["base"], STREAK)


SHELLS = program()


# the greeting: written by the stars of one large shell

TEXT_C = np.array([0.0, 4.3, -1.0])
T_BURST, T_FLY, T_FALL, T_GONE = 16.9, 1.1, 22.3, 2.6
SCRIPT = ["georgiaz.ttf", "C:/Windows/Fonts/georgiaz.ttf", "/System/Library/Fonts/Supplemental/Georgia Bold Italic.ttf",
          "/Library/Fonts/Georgia Bold Italic.ttf", "DejaVuSerif-BoldItalic.ttf"]
CAPS = ["georgiab.ttf", "C:/Windows/Fonts/georgiab.ttf", "/System/Library/Fonts/Supplemental/Georgia Bold.ttf",
        "/Library/Fonts/Georgia Bold.ttf", "DejaVuSerif-Bold.ttf"]


def font(names, px):
    for name in names:
        try:
            return ImageFont.truetype(name, px)
        except OSError:
            pass
    return ImageFont.load_default(px)


def lettering():
    # "Happy" in italics over spaced capitals, a flourish between, sampled into 3D points
    im = Image.new("L", (1200, 540), 0)
    d = ImageDraw.Draw(im)
    d.text((600, 140), "Happy", font=font(SCRIPT, 205), fill=255, anchor="mm", stroke_width=2, stroke_fill=255)
    caps, gap = font(CAPS, 205), 30
    w = [caps.getlength(ch) for ch in "DIWALI"]
    x = 600 - (sum(w) + gap * 5) / 2
    for ch, wi in zip("DIWALI", w):
        d.text((x, 400), ch, font=caps, fill=255, anchor="lm", stroke_width=3, stroke_fill=255)
        x += wi + gap
    a = np.linspace(-1, 1, 300)
    d.line(list(zip(600 + 270 * a, 262 + 9 * np.sin(a * 3 * np.pi) * (1 - a * a))), fill=255, width=5)
    d.regular_polygon((600, 262, 14), 4, fill=255)
    py, px = np.nonzero(np.asarray(im)[::4, ::4] > 110)
    py, px = py * 4.0, px * 4.0
    s = 7.8 / 1200
    P = np.stack([(px - 600) * s, TEXT_C[1] + (270 - py) * s, TEXT_C[2] + 0.02 * np.sin(px * 0.05)], 1)
    return P, np.where(py < 230, HAPPY, DIWALI)


TXT_P, TXT_TAG = lettering()
XN = (TXT_P[:, 0] - TXT_P[:, 0].min()) / np.ptp(TXT_P[:, 0])
TXT_R = np.random.default_rng(17).random((len(TXT_P), 3))
TXT_SHELL = make_shell(0.0, 0.0, TEXT_C[2], TEXT_C[1], None, None, 0, np.random.default_rng(5))
TXT_SHELL.update(t0=T_BURST - TXT_SHELL["tr"], tb=T_BURST, drift=0.0, top=TEXT_C.copy())


def greeting(t, out):
    # returns which letter points currently sit in place as solid lettering
    u = np.clip((t - T_BURST - 0.8 * XN) / T_FLY, 0, 1)        # written left to right
    tau = t - T_FALL - 0.9 * XN                                 # dissolves left to right
    flying, falling = (u > 0) & (u < 1), tau > 0
    if flying.any():
        # quadratic Bezier through an overshoot point: the burst opens, then settles
        e, de = 1 - (1 - u) ** 3, 3 * (1 - u) ** 2
        ctrl = TEXT_C + 1.7 * (TXT_P - TEXT_C)
        a, b = (1 - e)[:, None], e[:, None]
        p = a * a * TEXT_C + 2 * a * b * ctrl + b * b * TXT_P
        v = (2 * a * (ctrl - TEXT_C) + 2 * b * (TXT_P - ctrl)) * (de / T_FLY)[:, None]
        f = flying
        out.add(p[f], v[f], 1.1 - 0.3 * u[f], np.where(u[f] < 0.5, WHITE, TXT_TAG[f]), STREAK)
    f = falling & (tau < T_GONE)
    if f.any():
        q, vel = drag_path(TXT_P[f], (TXT_R[f] - 0.5) * [0.6, 0.4, 0.6], tau[f], 1.4, 0.9)
        out.add(q, vel, (1 - tau[f] / T_GONE) ** 1.2, np.where(tau[f] > 1.0, SAFFRON, TXT_TAG[f]), STREAK)
    return (u >= 1) & ~falling


# the city across the water: towers, two temple domes with spires, lit windows

def skyline():
    rng = np.random.default_rng(3)
    P, K, W = [], [], []
    for layer, (z, hmax, step) in enumerate(((-16.0, 1.9, 0.09), (-11.5, 1.2, 0.07))):
        edges = np.cumsum(rng.uniform(0.6, 1.9, 60)) - 32
        tops = rng.uniform(0.3, 1.0, 60) * hmax
        x = np.arange(-24, 24, step)
        idx = np.searchsorted(edges, x).clip(0, 59)
        h = tops[idx]
        if layer == 1:
            for n, (cx, r) in enumerate(((-3.2, 0.55), (4.1, 0.42))):
                q = np.clip(1 - ((x - cx) / r) ** 2, 0, None)
                h = np.where(q > 0, np.maximum(h, 0.9 + r * 1.15 * np.sqrt(q)), h)
                h = np.where(np.abs(x - cx) < step * 0.6, 0.9 + r * 1.15 + 0.55, h)     # spire
                idx = np.where(q > 0, 100 + n, idx)
        X, Y = np.meshgrid(x, np.arange(0, h.max(), step))
        inside = Y < h[None, :]
        X, Y = X[inside], Y[inside]
        I = np.broadcast_to(idx, inside.shape)[inside]
        P.append(np.stack([X, Y, np.full_like(X, z)], 1))
        K.append(2 + I + 200 * layer)
        floor, bay = np.floor(Y / 0.16), np.floor(X / 0.2)
        lit = ((floor * 7 + bay * 13 + I * 3) % 11 < 3) & (Y < h[np.searchsorted(x, X).clip(0, len(x) - 1)] - 0.12)
        W.append(lit & (I < 100))
    roof_x = np.arange(-12, 12, 0.11)
    return np.concatenate(P), np.concatenate(K), np.concatenate(W), roof_x, x, h


CITY_P, CITY_K, CITY_W, ROOF_X, FRONT_X, FRONT_H = skyline()
WIN_RAND = np.random.default_rng(9).random(len(CITY_P))
SKY = np.random.default_rng(23).random((420, 3))


def city_lights(t, out):
    # strings of little lamps along the near rooftops, a chase running through them
    h = np.interp(ROOF_X, FRONT_X, FRONT_H) + 0.04
    p = np.stack([ROOF_X, h, np.full_like(ROOF_X, -11.4)], 1)
    chase = np.clip(np.sin(ROOF_X * 2.6 - t * 3.5), 0, 1) ** 3
    tags = np.array([GOLD, RED, GREEN, SAFFRON, MAGENTA])[(np.arange(len(p)) // 3) % 5]
    out.add(p, np.zeros_like(p), (0.35 + 0.65 * chase) * ease(t / 1.5), tags, POINT)


def sky_stars(t, out):
    az, el = (SKY[:, 0] - 0.5) * 1.6, 0.04 + 0.8 * SKY[:, 1] ** 1.3
    p = np.stack([np.sin(az) * np.cos(el), np.sin(el), -np.cos(az) * np.cos(el)], 1) * 60
    tw = 0.5 + 0.5 * np.sin(t * (0.8 + 2 * SKY[:, 2]) + SKY[:, 0] * 50)
    out.add(p, np.zeros_like(p), (0.10 + 0.30 * tw ** 4) * ease(t / 2.0), STAR, POINT)


# camera: low by the water, looking up into the sky, drifting slowly

EYE, AIM, F = np.array([0.0, 0.55, 8.0]), np.array([0.0, 2.7, 0.0]), 2.2


def camera(t):
    drift = np.array([0.6 * np.sin(t * 0.11), 0.05 * np.sin(t * 0.37), 0.0])
    lift = 0.45 * ease((t - 16.0) / 2.5) * (1 - ease((t - 23.5) / 2.0))     # look up at the greeting
    eye, aim = EYE + drift, AIM + drift * 0.4 + [0, lift, 0]
    fwd = (aim - eye) / np.linalg.norm(aim - eye)
    right = np.cross(fwd, [0, 1.0, 0])
    right /= np.linalg.norm(right)
    up = np.cross(right, fwd)

    def look(P):
        rel = P - eye
        return rel @ right, rel @ up, rel @ fwd
    return look


class Sparks:
    # everything that glows this frame: position, velocity, brightness, colour, look
    def __init__(self):
        self.items = []

    def add(self, p, v, shade, tag, mode):
        n = len(p)
        if n:
            self.items.append((p, v, np.broadcast_to(shade, n), np.broadcast_to(tag, n), np.broadcast_to(mode, n)))

    def arrays(self):
        if not self.items:
            return np.zeros((0, 3)), np.zeros((0, 3)), np.zeros(0), np.zeros(0, int), np.zeros(0, int)
        return [np.concatenate(x) for x in zip(*self.items)]


def show(t):
    out, flash, cores, active = Sparks(), 0.0, [], 0
    for s in SHELLS + [TXT_SHELL]:
        a, b = t - s["t0"], t - s["tb"]
        if 0 < a and b < 0:
            rocket(s, a, out)
            active += 1
        elif b >= 0:
            if b < 0.3:
                cores.append((s["top"], np.exp(-b * 8)))
            size = 2.5 if s["kind"] is None else len(s["dirs"]) / 200
            flash += size * np.exp(-b * 7)
            if s["kind"] is not None and b < KIND[s["kind"]][0] * 1.25 + 1.0:
                burst(s, b, out)
                active += 1
    held = greeting(t, out)
    city_lights(t, out)
    sky_stars(t, out)
    return out, min(flash, 1.6), cores, held, active


# letters

def boundary_glyph(gx, gy):
    # a stroke along the boundary, i.e. across the direction toward open sky
    bins = (np.round((np.arctan2(gy, gx) % np.pi) / (np.pi / 4)) % 4).astype(int)
    marks = np.array([CODE["|"], CODE["/"], CODE["-"], CODE["\\"]])[bins]
    return np.where((gx == 0) & (gy == 0), CODE["+"], marks)


def motion_glyph(dx, dy):
    # a moving spark becomes a short stroke along its motion on screen
    bins = (np.round((np.arctan2(dy * CH, dx * CW) % np.pi) / (np.pi / 4)) % 4).astype(int)
    return np.array([CODE["-"], CODE["\\"], CODE["|"], CODE["/"]])[bins]


SPOKES = (((0, 2), "-"), ((0, -2), "-"), ((1, 0), "|"), ((-1, 0), "|"),
          ((1, 1), "\\"), ((-1, -1), "\\"), ((1, -1), "/"), ((-1, 1), "/"))


def flare(char, lum, tag, cy, cx, reach, colour, power=1.0):
    # an eight-spoke flare; the diagonal spokes are half as long and dimmer
    rows, cols = char.shape
    for (dy, dx), mark in SPOKES:
        diag = abs(dx) == 1
        n = max(reach // 2, 1) if diag else reach
        k = np.arange(1, n + 1)
        y, x = cy + dy * k, cx + dx * k
        val = power * (1 - k / (n + 1)) * (0.65 if diag else 1.0)
        m = (y >= 0) & (y < rows) & (x >= 0) & (x < cols)
        y, x, val = y[m], x[m], val[m]
        m = lum[y, x] < val
        char[y[m], x[m]], lum[y[m], x[m]], tag[y[m], x[m]] = CODE[mark], val[m], colour
    char[cy, cx], lum[cy, cx], tag[cy, cx] = CODE["*"], 1.0, WHITE


def neighbour(a, dy, dx, fill):
    # a[y + dy, x + dx], with `fill` beyond the border
    b = np.roll(a, (-dy, -dx), (0, 1))
    if dy:
        b[-1 if dy > 0 else 0, :] = fill
    if dx:
        b[:, -1 if dx > 0 else 0] = fill
    return b


def cell_noise(rows, cols):
    r, c = np.mgrid[0:rows, 0:cols]
    return (np.sin(r * 12.9898 + c * 78.233) * 43758.5453) % 1.0


def frame(t, cols, rows, aspect):
    out, flash, cores, held, active = show(t)
    look = camera(t)

    def screen(P):
        x, y, z = look(P)
        zz = np.maximum(z, 0.2)
        return (0.5 + x / zz * F * 0.5) * cols, (0.5 - y / zz * F * 0.5 * aspect) * rows, z

    # solid things: the city and the held lettering, nearest point per cell
    win = CITY_W & (WIN_RAND < 0.85 + 0.15 * np.sin(t * 0.7 + WIN_RAND * 40))
    shimmer = 0.78 + 0.22 * np.sin(t * 3.2 - TXT_P[held, 0] * 1.5)
    P = np.concatenate([CITY_P, TXT_P[held]])
    shade = np.concatenate([np.where(win, 0.55, 0.09 + 0.22 * flash) * ease(t / 1.5), shimmer])
    T = np.concatenate([np.where(win, WINDOW, CITY), TXT_TAG[held]])
    K = np.concatenate([CITY_K, np.ones(held.sum(), int)])
    sx, sy, z = screen(P)
    ix, iy = sx.astype(int), sy.astype(int)
    ok = np.flatnonzero((ix >= 0) & (ix < cols) & (iy >= 0) & (iy < rows) & (z > 0.2))
    cell = iy[ok] * cols + ix[ok]
    o = np.lexsort((z[ok], cell))                               # by cell, nearest first
    o = ok[o[np.r_[True, cell[o][1:] != cell[o][:-1]]]]         # z-buffer: keep the nearest
    part = np.full(rows * cols, -1)
    depth = np.full(rows * cols, 999.0)
    lum = np.zeros(rows * cols)
    tag = np.zeros(rows * cols, int)
    at = iy[o] * cols + ix[o]
    part[at], depth[at], lum[at], tag[at] = K[o], z[o], shade[o], T[o]
    part, depth, lum, tag = (a.reshape(rows, cols) for a in (part, depth, lum, tag))
    body = part >= 0

    # boundaries: a cell whose neighbour is open sky or a different part well behind it
    gx, gy = np.zeros((rows, cols)), np.zeros((rows, cols))
    edge = np.zeros_like(body)
    for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0)):
        np_, nd = neighbour(part, dy, dx, -1), neighbour(depth, dy, dx, 999.0)
        open_ = body & ((np_ < 0) | ((np_ != part) & (nd > depth + 0.5)))
        gx, gy, edge = gx + dx * open_, gy + dy * open_, edge | open_

    noise = cell_noise(rows, cols)
    tier = np.digitize(lum, (0.3, 0.55, 0.8))
    step = (np.floor(noise * 64) + np.floor(t * 4 + noise * 3)).astype(int) % 8
    words = np.isin(tag, (HAPPY, DIWALI))
    tier = np.where(words, 3, tier)                             # lettering stays dense
    char = np.where(body, np.where(edge & ~words, boundary_glyph(gx, gy), TIER_LUT[tier, step]), 0)
    char = np.where(body & (tag == WINDOW), np.where(noise < 0.33, CODE[":"], CODE["."]), char)
    city = body & (tag == CITY)
    char = np.where(city & ~edge, np.where(noise < 0.25, CODE["."], CODE[" "]), char)
    lum = np.where(body & edge, np.maximum(lum, np.where(tag == CITY, 0.22 + 0.3 * flash, 0.9)), lum)

    # sparks, and their reflections rippling on the water
    p, v, s, st, sm = out.arrays()
    up = p[:, 1] > 0.02
    wet = p[up].copy()
    wet[:, 1] *= -1
    wet[:, 0] += 0.05 * np.sin(wet[:, 1] * 9 + t * 2.1 + wet[:, 2])
    lamp = CITY_P[win][::2].copy()
    lamp[:, 1] *= -1
    p = np.concatenate([p, wet, lamp])
    v = np.concatenate([v, v[up] * [1, -1, 1], np.zeros_like(lamp)])
    s = np.concatenate([s, s[up] * 0.3 * (1 - ease(-wet[:, 1] / 9)), np.full(len(lamp), 0.16)])
    st = np.concatenate([st, st[up], np.full(len(lamp), WINDOW)])
    sm = np.concatenate([sm, np.full(up.sum() + len(lamp), WATER)])
    fx, fy, fz = screen(p)
    dx, dy, _ = screen(p + v * 0.06)
    ix, iy = fx.astype(int), fy.astype(int)
    ok = (ix >= 0) & (ix < cols) & (iy >= 0) & (iy < rows) & (fz > 0.2) & (s > 0.03)
    ok[ok] &= ~(body[iy[ok], ix[ok]] & (depth[iy[ok], ix[ok]] < fz[ok]))
    o = np.flatnonzero(ok)[np.argsort(s[ok])]                   # brightest spark wins its cell
    ix, iy, s, st, sm = ix[o], iy[o], s[o], st[o], sm[o]
    dot = np.array([CODE["."], CODE[":"], CODE["+"], CODE["*"]])[np.digitize(s, (0.22, 0.5, 0.9))]
    ripple = np.where((iy + ix) % 3 == 0, CODE["-"], CODE["~"])
    char[iy, ix] = np.where(sm == WATER, ripple, np.where(sm == STREAK, motion_glyph(dx[o] - fx[o], dy[o] - fy[o]), dot))
    lum[iy, ix], tag[iy, ix] = np.clip(s, 0, 1), st

    # a flare the instant a shell opens, and glints wandering over the lettering
    for pos, k in cores:
        cx, cy, cz = screen(pos[None])
        if cz[0] > 0.2 and 0 <= int(cy[0]) < rows and 0 <= int(cx[0]) < cols:
            flare(char, lum, tag, int(cy[0]), int(cx[0]), int(3 + 8 * k), WHITE)
    lit = np.argwhere(body & words)
    for n in range(3):
        if len(lit):
            phase = t * 1.6 + n * 0.37
            gy_, gx_ = lit[int(abs(np.sin(np.floor(phase) * 91.7 + n * 13.1)) * 1e4) % len(lit)]
            flare(char, lum, tag, int(gy_), int(gx_), int(2 + 4 * np.sin(phase % 1 * np.pi)), GOLD, 0.9)
    return char, lum, tag, {"shells": active, "sparks": len(o), "flash": flash}


# pixels

TITLE = "Happy Diwali"
SIGN = "rajeet-04 · 2026"
CW, CH, FONT_PX = 9, 15, 14
W, H = 1152, 960
COLS, ROWS = W // CW, H // CH
ASPECT = W / H
AFTERGLOW = 0.6     # how much of the previous frame survives (trails)
COLORS = np.array([
    [190, 205, 255], [86, 96, 136], [255, 186, 104],                    # sky star, city, window
    [255, 204, 84], [255, 140, 40], [255, 62, 72], [255, 84, 200],      # gold, saffron, red, magenta
    [90, 232, 150], [96, 156, 255], [255, 250, 240], [214, 226, 255],   # green, blue, white, silver
    [255, 132, 196], [255, 212, 120],                                   # Happy, DIWALI
], np.float32) / 255
MONO = "menlo,consolas,dejavusansmono,couriernew,monospace"


class Film:
    # turns letter grids into pixels and keeps a fading afterimage for trails
    def __init__(self):
        font_ = pygame.font.SysFont(MONO, FONT_PX)
        strip = pygame.Surface((CW * len(GLYPHS), CH))
        for i, g in enumerate(GLYPHS):
            img = font_.render(g, True, (255, 255, 255))
            strip.blit(img, (i * CW + (CW - img.get_width()) // 2, (CH - img.get_height()) // 2))
        ink = pygame.surfarray.array_red(strip).T.astype(np.float32) / 255     # CH x (CW * n)
        self.ink = ink.reshape(CH, len(GLYPHS), CW).transpose(1, 0, 2)          # n x CH x CW
        self.after = np.zeros((ROWS * CH, COLS * CW, 3), np.float32)

    def clear(self):
        self.after[:] = 0

    def expose(self, char, lum, tag):
        rgb = COLORS[tag] * np.clip(lum, 0, 1)[..., None]                       # rows x cols x 3
        img = self.ink[char][..., None] * rgb[:, :, None, None, :]             # rows x cols x CH x CW x 3
        img = img.transpose(0, 2, 1, 3, 4).reshape(ROWS * CH, COLS * CW, 3)
        np.maximum(self.after * AFTERGLOW, img, out=self.after)
        return pygame.surfarray.make_surface((self.after * 255).astype(np.uint8).transpose(1, 0, 2))


def bloom(surface):
    # soft halo: shrink, enlarge and add back, at two widths
    out = surface.copy()
    for k, gain in ((5, 0.75), (14, 0.45)):
        halo = pygame.transform.smoothscale(surface, (W // k, H // k))
        halo = pygame.transform.smoothscale(halo, (W, H))
        halo.fill((int(255 * gain),) * 3, special_flags=pygame.BLEND_MULT)
        out.blit(halo, (0, 0), special_flags=pygame.BLEND_ADD)
    return out


def picture(film, label, t):
    char, lum, tag, info = frame(t, COLS, ROWS, ASPECT)
    out = bloom(film.expose(char, lum, tag))
    lines = (f"{t:5.2f} s", f"shells {info['shells']:2d}", f"sparks {info['sparks']:5d}", SIGN)
    for n, text in enumerate(lines):
        img = label.render(text, True, (150, 120, 70) if n < 3 else (120, 110, 130))
        out.blit(img, (W - 28 - img.get_width(), 22 + 20 * n))
    return out


def main():
    pygame.init()
    shown = pygame.display.Info()
    k = min(1.0, 0.9 * shown.current_h / H, 0.95 * shown.current_w / W) if shown.current_h > 0 else 1.0
    size = (int(W * k), int(H * k))
    window = pygame.display.set_mode(size)
    pygame.display.set_caption(TITLE)
    film, label, clock = Film(), pygame.font.SysFont(MONO, 15), pygame.time.Clock()
    t, running, last = 0.0, True, None
    while True:
        for e in pygame.event.get():
            key = e.key if e.type == pygame.KEYDOWN else None
            if e.type == pygame.QUIT or key == pygame.K_ESCAPE:
                pygame.quit()
                return 0
            if key == pygame.K_SPACE:
                running = not running
            elif key == pygame.K_r:
                t = 0.0
                film.clear()
            elif key == pygame.K_s and last is not None:
                pygame.image.save(last, str(Path(__file__).with_name(f"diwali_{int(time.time())}.png")))
        step = clock.tick(30) / 1000
        if running or last is None:
            t = t + min(step, 1 / 15) if running else t
            if t > LOOP:
                t = 0.0
                film.clear()
            last = picture(film, label, t)
        window.blit(last if size == (W, H) else pygame.transform.smoothscale(last, size), (0, 0))
        pygame.display.flip()


if __name__ == "__main__":
    sys.exit(main())
