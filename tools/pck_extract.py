#!/usr/bin/env python3
"""从 Graphwar.pck 还原游戏资源到 rsc/，并生成清单。

只做"按原始路径解包 + 校验"，不做格式转换：
`.ctex`（压缩纹理）、`.scn`（二进制场景）、`.gdc`（GDScript 字节码）、
`.res` 等**本身就是 Godot 可加载的资源**，转成 PNG/文本反而会丢信息。

Godot 4 PCK（v2）布局：
    u32 magic 'GDPC' | u32 pack_version | u32 ver_major | u32 ver_minor | u32 ver_patch
    u32 pack_flags | u64 file_base | 16 * u32 reserved
    u32 file_count | [ u32 path_len | path(padded 4) | u64 offset | u64 size |
                       u8 md5[16] | u32 flags ] * file_count
偏移以 `file_base` 为基准（除非设置了相对标志，这里按实际取值处理）。

用法：
    ../.venv/bin/python tools/pck_extract.py                 # 解到 ./rsc
    ../.venv/bin/python tools/pck_extract.py --out rsc --verify
"""

import argparse
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUB = os.path.dirname(HERE)
ROOT = os.path.dirname(SUB)
DEFAULT_PCK = os.path.join(ROOT, "Graphwar.pck")

MAGIC = 0x43504447  # 'GDPC'


def parse_pck(path):
    """解析 Godot 4 PCK(v4)：文件索引在包尾，头部多一个 directory_offset。"""
    data = open(path, "rb").read()
    mv = memoryview(data)
    magic, pack_ver, vmaj, vmin, vpat = struct.unpack_from("<IIIII", mv, 0)
    if magic != MAGIC:
        raise SystemExit("不是 Godot PCK（magic=0x%08x）" % magic)
    flags, = struct.unpack_from("<I", mv, 0x14)
    file_base, = struct.unpack_from("<Q", mv, 0x18)
    dir_offset, = struct.unpack_from("<Q", mv, 0x20)

    off = dir_offset
    count, = struct.unpack_from("<I", mv, off)
    off += 4
    files = []
    for _ in range(count):
        plen, = struct.unpack_from("<I", mv, off)
        off += 4
        raw = mv[off:off + plen]
        off += (plen + 3) & ~3
        name = bytes(raw).rstrip(b"\x00").decode("utf-8", "replace")
        foff, fsize = struct.unpack_from("<QQ", mv, off)
        off += 16
        md5 = bytes(mv[off:off + 16])
        off += 16
        fflags, = struct.unpack_from("<I", mv, off)
        off += 4
        files.append({
            "path": name.removeprefix("res://").lstrip("/"),  # 不能用 lstrip("res://")：它按字符集合剥离，会吃掉开头的 e/r/s
            "offset": foff,
            "size": fsize,
            "md5": md5.hex(),
            "flags": fflags,
        })
    # 实测：偏移一律相对 file_base（本包的 flags=2 并不改变这一点，
    # 用 base=0 时全部 md5 不符，用 file_base 时全部相符）
    base = file_base
    return data, files, {"pack_version": pack_ver,
                         "godot": "%d.%d.%d" % (vmaj, vmin, vpat),
                         "flags": flags, "file_base": file_base,
                         "dir_offset": dir_offset, "base": base}


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--pck", default=DEFAULT_PCK)
    ap.add_argument("--out", default=os.path.join(SUB, "rsc"))
    ap.add_argument("--verify", action="store_true", help="按 md5 校验每个文件")
    a = ap.parse_args(argv)

    data, files, meta = parse_pck(a.pck)
    print("PCK: %s  godot %s  pack v%d  文件 %d 个  file_base=0x%x"
          % (os.path.basename(a.pck), meta["godot"], meta["pack_version"],
             len(files), meta["base"]))

    written = 0
    total = 0
    bad = 0
    manifest = []
    for f in files:
        dst = os.path.join(a.out, f["path"])
        os.makedirs(os.path.dirname(dst) or a.out, exist_ok=True)
        start = meta["base"] + f["offset"]
        blob = data[start:start + f["size"]]
        if len(blob) != f["size"]:
            print("  截断:", f["path"], len(blob), "!=", f["size"])
            bad += 1
            continue
        if a.verify:
            got = hashlib.md5(blob).hexdigest()
            if got != f["md5"]:
                print("  md5 不符:", f["path"], got, "!=", f["md5"])
                bad += 1
                continue
        with open(dst, "wb") as fh:
            fh.write(blob)
        manifest.append((f["path"], f["size"], hashlib.sha256(blob).hexdigest()))
        written += 1
        total += f["size"]

    man = os.path.join(a.out, "MANIFEST.tsv")
    with open(man, "w", encoding="utf-8") as fh:
        fh.write("path\tsize\tsha256\n")
        for p, s, h in sorted(manifest):
            fh.write("%s\t%d\t%s\n" % (p, s, h))
    print("写出 %d 个文件, 共 %.1f MiB, 清单 %s" % (written, total / 1048576, man))
    if bad:
        print("失败 %d 个" % bad)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
