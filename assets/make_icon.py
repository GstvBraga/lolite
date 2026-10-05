"""Generates lolite.ico (exe/taskbar) and icon64.rgba (window icon). Run: python assets/make_icon.py"""
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))


def hexc(h):
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def lerp(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


BG_TOP, BG_BOT = hexc("13304A"), hexc("010A13")
GOLD, GOLD_LIGHT, TEAL = hexc("C8AA6E"), hexc("F0E6D2"), hexc("0AC8B9")


def rounded_inside(x, y, inset, r):
    lo, hi = inset, 1 - inset
    if not (lo <= x <= hi and lo <= y <= hi):
        return False
    cx = min(max(x, lo + r), hi - r)
    cy = min(max(y, lo + r), hi - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def sample(x, y):
    """RGBA (0-1 alpha) of the design at unit coords."""
    if not rounded_inside(x, y, 0.02, 0.22):
        return None
    in_l = (0.32 <= x <= 0.46 and 0.24 <= y <= 0.76) or (0.32 <= x <= 0.70 and 0.62 <= y <= 0.76)
    if in_l:
        return lerp(GOLD_LIGHT, GOLD, (y - 0.24) / 0.52)
    if abs(x - 0.64) + abs(y - 0.36) <= 0.09:
        return TEAL
    if not rounded_inside(x, y, 0.075, 0.18):
        return GOLD
    return lerp(BG_TOP, BG_BOT, y)


def render(size, ss=4):
    px = []
    for j in range(size):
        for i in range(size):
            r = g = b = a = 0.0
            for sj in range(ss):
                for si in range(ss):
                    c = sample((i + (si + 0.5) / ss) / size, (j + (sj + 0.5) / ss) / size)
                    if c:
                        r, g, b, a = r + c[0], g + c[1], b + c[2], a + 1
            n = ss * ss
            px.append((int(r / a) if a else 0, int(g / a) if a else 0, int(b / a) if a else 0, int(255 * a / n)))
    return px


def bmp_entry(size, px):
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    rows = [px[j * size:(j + 1) * size] for j in range(size)]
    data = b"".join(bytes((b, g, r, a)) for row in reversed(rows) for (r, g, b, a) in row)
    mask = b"\0" * (((size + 31) // 32) * 4 * size)
    return header + data + mask


sizes = [16, 24, 32, 48, 64, 256]
images = {s: render(s, 4 if s <= 64 else 2) for s in sizes}
blobs = [bmp_entry(s, images[s]) for s in sizes]
out = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
for s, blob in zip(sizes, blobs):
    out += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(blob), offset)
    offset += len(blob)
out += b"".join(blobs)
open(os.path.join(HERE, "lolite.ico"), "wb").write(out)
open(os.path.join(HERE, "icon64.rgba"), "wb").write(b"".join(bytes(p) for p in images[64]))
print("wrote lolite.ico, icon64.rgba")
