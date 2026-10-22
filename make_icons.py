"""Generate evorar icon: PNG(256) wrapped into ICO — no external deps."""
import struct, zlib, math

W = H = 256

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

C1, C2 = (76, 110, 245), (43, 63, 199)   # accent gradient
BG_TRANS = (0, 0, 0, 0)

px = [[BG_TRANS] * W for _ in range(H)]

def rounded_mask(x, y, w, h, r):
    if x < 0 or y < 0 or x >= w or y >= h:
        return False
    cx = min(max(x, r), w - 1 - r)
    cy = min(max(y, r), h - 1 - r)
    dx, dy = x - cx, y - cy
    return dx * dx + dy * dy <= r * r + 1

# rounded-square background with vertical gradient
for y in range(H):
    for x in range(W):
        if rounded_mask(x, y, W, H, 56):
            c = lerp(C1, C2, y / H)
            px[y][x] = (c[0], c[1], c[2], 255)

def put(x, y, color=(255, 255, 255, 255)):
    if 0 <= x < W and 0 <= y < H:
        px[y][x] = color

def rect(x0, y0, x1, y1, color=(255, 255, 255, 255)):
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            put(x, y, color)

def arrow(cx, top, bot, half=34, shaft=16):
    """downward arrow: shaft + head"""
    rect(cx - shaft // 2, top, cx + shaft // 2, bot - half, )
    # head (triangle)
    for i in range(half):
        wdt = int(half * (1 - i / half))
        for xx in range(cx - wdt, cx + wdt + 1):
            put(xx, bot - half + i)

# archive box outline on top
rect(72, 60, 184, 64)          # top bar
rect(72, 60, 72, 118)
rect(184, 60, 184, 118)
rect(72, 118, 184, 122)
# slot in middle of box
rect(124, 60, 132, 88)
# big down arrow below
arrow(128, 150, 216, half=36, shaft=18)

# encode PNG
raw = b""
for y in range(H):
    raw += b"\x00" + b"".join(bytes(px[y][x]) for x in range(W))

def chunk(tag, data):
    c = tag + data
    return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)

png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw, 9))
       + chunk(b"IEND", b""))

open(r"D:\JCode\evorar\code\src-tauri\icons\icon.png", "wb").write(png)

# ICO (embed PNG for 256x256)
ico = (struct.pack("<HHH", 0, 1, 1)
       + struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png), 6 + 16)
       + png)
open(r"D:\JCode\evorar\code\src-tauri\icons\icon.ico", "wb").write(ico)
print("icons written: icon.png / icon.ico")
