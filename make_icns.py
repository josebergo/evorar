"""从 design/icon.png 生成 macOS icon.icns(现代 ICNS:容器内嵌 PNG)。

依赖 Pillow(python -m pip install pillow)。
图标源文件更新后重跑本脚本即可,mac 构建会自动使用新的 icns。
用法:在 code/ 目录下执行  python make_icns.py
"""
from io import BytesIO
import os
import struct

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SRC = os.path.join(HERE, "design", "icon.png")
OUT = os.path.join(HERE, "src-tauri", "icons", "icon.icns")

# (ICNS 类型码, 像素尺寸);ic10 兼作 512@2x
ENTRIES = [
    ("ic11", 32),
    ("ic12", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
]

im = Image.open(SRC).convert("RGBA")
blobs = []
for code, size in ENTRIES:
    img = im.resize((size, size), Image.LANCZOS)
    buf = BytesIO()
    img.save(buf, "PNG")
    data = buf.getvalue()
    blobs.append(code.encode("ascii") + struct.pack(">I", len(data) + 8) + data)

icns = b"icns" + struct.pack(">I", 8 + sum(len(b) for b in blobs)) + b"".join(blobs)
with open(OUT, "wb") as f:
    f.write(icns)
print(f"icon.icns written ({len(icns)} bytes)")
