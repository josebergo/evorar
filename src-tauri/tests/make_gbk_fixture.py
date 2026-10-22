"""手工构造 ZIP(store 模式):条目名 GBK 字节、无 UTF-8 flag,
模拟 Windows 资源管理器/老国产工具压出的包。"""
import struct, zlib, os

def make_zip(path, entries):
    """entries: [(gbk_name_bytes, data_bytes)]"""
    local = b""
    central = b""
    for name_b, data in entries:
        crc = zlib.crc32(data) & 0xffffffff
        # local file header: sig, ver, flag=0, method=0(store), time, date, crc, csize, usize, nlen, elen
        local += struct.pack("<IHHHHHIIIHH", 0x04034b50, 20, 0, 0, 0, 0,
                             crc, len(data), len(data), len(name_b), 0) + name_b + data
        off = 0  # filled after
    # rebuild with offsets
    local = b""
    central = b""
    offset = 0
    for name_b, data in entries:
        crc = zlib.crc32(data) & 0xffffffff
        lh = struct.pack("<IHHHHHIIIHH", 0x04034b50, 20, 0, 0, 0, 0,
                         crc, len(data), len(data), len(name_b), 0) + name_b
        ch = struct.pack("<IHHHHHHIIIHHHHHII", 0x02014b50, 20, 20, 0, 0, 0, 0,
                         crc, len(data), len(data), len(name_b), 0, 0, 0, 0, 0, offset) + name_b
        local += lh + data
        central += ch
        offset += len(lh) + len(data)
    eocd = struct.pack("<IHHHHIIH", 0x06054b50, 0, 0, len(entries), len(entries),
                       len(central), len(local), 0)
    with open(path, "wb") as f:
        f.write(local + central + eocd)

out = os.path.join(os.path.dirname(__file__), "fixtures")
os.makedirs(out, exist_ok=True)
make_zip(os.path.join(out, "gbk.zip"), [
    ("文档.txt".encode("gbk"), "GBK 编码文件名".encode("gbk")),
    ("新建文件夹/说明.txt".encode("gbk"), b"nested"),
])
print("fixture written (flag=0, GBK names)")
