#!/usr/bin/env python3
"""Generate a 1024x1024 app icon for Harness Desktop (pure stdlib)."""
import math
import struct
import sys
import zlib

W = H = 1024
CX = CY = W / 2.0


def hex_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


BG_TOP = hex_rgb("#182340")
BG_BOT = hex_rgb("#0a101f")
ACCENT = hex_rgb("#4d6bfe")
ACCENT_LIGHT = hex_rgb("#7dd3fc")


def sd_box(px, py, cx, cy, hw, hh, r):
    dx = abs(px - cx) - (hw - r)
    dy = abs(py - cy) - (hh - r)
    ox = max(dx, 0.0)
    oy = max(dy, 0.0)
    return math.hypot(ox, oy) + min(max(dx, dy), 0.0) - r


def h_dist(x, y):
    d1 = sd_box(x, y, CX - 210, CY, 85, 255, 62)
    d2 = sd_box(x, y, CX + 210, CY, 85, 255, 62)
    d3 = sd_box(x, y, CX, CY + 55, 215, 52, 48)
    return min(d1, d2, d3)


def lerp(a, b, t):
    return a + (b - a) * t


rows = bytearray()
for yy in range(H):
    rows.append(0)  # filter: none
    t = yy / (H - 1)
    bg = tuple(lerp(BG_TOP[i], BG_BOT[i], t) for i in range(3))
    for xx in range(W):
        d = h_dist(xx + 0.5, yy + 0.5)
        a = max(0.0, min(1.0, 0.5 - d))
        g = (yy / H) * 0.25
        r = int(lerp(ACCENT[0], ACCENT_LIGHT[0], g))
        gg = int(lerp(ACCENT[1], ACCENT_LIGHT[1], g))
        b = int(lerp(ACCENT[2], ACCENT_LIGHT[2], g))
        rows += bytes(
            (
                int(bg[0] * (1 - a) + r * a),
                int(bg[1] * (1 - a) + gg * a),
                int(bg[2] * (1 - a) + b * a),
                255,
            )
        )


def chunk(tag, data):
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def png(path):
    ihdr = struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(rows), 9)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", ihdr))
        f.write(chunk(b"IDAT", idat))
        f.write(chunk(b"IEND", b""))


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "app-icon.png"
    png(out)
    print("wrote", out)
