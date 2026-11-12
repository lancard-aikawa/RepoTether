"""Draw the Offstage source icon (1024x1024 PNG) with the standard library only.

A stage (lit rectangle) with a single item standing just outside it, on a dark rounded tile.
Run: python -I apps/offstage/tools/make_icon.py <out.png>   then   pnpm tauri icon <out.png> -o apps/offstage/src-tauri/icons
"""
import struct
import sys
import zlib

N = 1024
BG = (0x1F, 0x2A, 0x3D)
STAGE = (0xF2, 0xC9, 0x4C)
ITEM = (0xE8, 0xEE, 0xF5)


def inside_round_rect(x, y, x0, y0, x1, y1, r):
    if x < x0 or x > x1 or y < y0 or y > y1:
        return False
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


rows = []
for y in range(N):
    row = bytearray([0])  # filter: none
    for x in range(N):
        px = (0, 0, 0, 0)
        if inside_round_rect(x, y, 32, 32, N - 33, N - 33, 190):
            px = BG + (255,)
            if inside_round_rect(x, y, 170, 300, 640, 760, 40):
                px = STAGE + (255,)
            if inside_round_rect(x, y, 720, 470, 860, 760, 36):
                px = ITEM + (255,)
        row.extend(px)
    rows.append(bytes(row))

raw = zlib.compress(b"".join(rows), 9)


def chunk(tag, data):
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0)) + chunk(b"IDAT", raw) + chunk(b"IEND", b"")
with open(sys.argv[1], "wb") as f:
    f.write(png)
