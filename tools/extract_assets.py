#!/usr/bin/env python3
"""把正版 `Graphwar.pck` 里的**媒体资源**解到 `client/assets/`，并生成清单。

为什么需要这个脚本
==================
仓库**不存放任何媒体文件**（`.svg` / `.ttf` / `.png` / `.tres` / `.tscn` / `.gd` / `.pck`），
理由是版权（见 `docs/108` §11、`AGENTS.md`）。所以客户端里所有图标与字体都必须是
"运行时从使用者自己的 `Graphwar.pck` 里解出来的原件" —— 本脚本就是那条提取管线。

从包里能解出什么（**这是观测结论，不是设计选择**）
=================================================
对 `Graphwar.pck` 做一次全量清单（692 个文件）得到的扩展名分布：

    .import 184 | .ctex 167 | .remap 158 | .gdc 105 | .scn 31 | .res 22
    .fontdata 7 | 音频 11 | .png 1 | .binary 1 | .gdextension 1

也就是说：

* 包里**没有 `.svg`**，也**没有 `.ttf`**。原版的 149 个 svg 与 7 个 ttf 在导出时被
  Godot 的导入流程吃掉了，只剩导入产物：
    - 149 个 svg → `*.ctex`（`CompressedTexture2D`，导入时按 512×512 光栅化）
    - 7 个 ttf  → `*.fontdata`（`FontFile`，MSDF）
* 每个 `.import` 文件里写着 `source_file="res://hats/no_hat.svg"` 与
  `dest_files=["res://.godot/imported/no_hat.svg-<md5>.ctex"]` —— **源路径↔产物的映射是
  包自己给的**，不需要猜，也不需要拿恢复工程做参照（但两者对得上，见 docs/121 §4）。

于是本脚本产出的是**原始资产本身**（只是容器格式是 Godot 的导入产物）：

    client/assets/hats/no_hat.ctex        <- res://hats/no_hat.svg
    client/assets/ComicRelief-Bold.fontdata <- res://ComicRelief-Bold.ttf
    client/assets/button_font.res         <- res://button_font.tres
    client/assets/basic_soldier.ctex      <- res://basic_soldier.png

客户端按**原版路径**引用它们（`GwAssets.texture("hats/no_hat")`），并兼容
`.ctex/.png/.svg/.webp` 与 `.fontdata/.ttf/.otf`：手上有完整工程（能拿到真 svg/ttf）的人
把同名文件放进去也能直接用，代码不需要改。

顺带产出
========
* `client/assets/original_project_settings.json` —— 从包里的 `project.binary` 解出的
  **原版工程设置**（ECFG 格式，见 `parse_project_binary`）。客户端用它**校验**
  `client/project.godot` 里的 `default_clear_color` / `boot_splash/*` 是否与原版逐位一致
  （`docs/121` §3），而不是把颜色当成"我们觉得好看"的常量。
* `client/assets/MANIFEST.tsv` —— 每条 `源路径 / 落盘路径 / 字节数 / sha256`。

用法
====
    python3 tools/extract_assets.py                    # 默认 <repo>/Graphwar.pck → client/assets
    python3 tools/extract_assets.py --pck /path/to/Graphwar.pck
    python3 tools/extract_assets.py --list             # 只打印会解出什么
    python3 tools/extract_assets.py --check            # 只检查 client/assets 是否齐备（CI 用）

只用标准库；不执行包里的任何代码。
"""

import argparse
import hashlib
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)              # gw2-rebuild/
WS = os.path.dirname(REPO)                # 上一级：Graphwar.pck 与 libgraphwarrust.so 所在处
DEFAULT_PCK = os.path.join(WS, "Graphwar.pck")
DEFAULT_OUT = os.path.join(REPO, "client", "assets")

MAGIC = 0x43504447  # 'GDPC'

# 我们需要的资源：原版 `res://` 路径（不含扩展名）由这三类规则决定。
TEXTURE_DIRS = ("faces/", "hats/", "skins/", "ranks/")
TEXTURE_ROOT_EXTRA = ("basic_soldier", "empty_soldier", "empty_set")
# `button_font.tres` 是一个**自带字形缓存**的 FontFile（2.8 MB），能独立加载；
# 其余 21 个 `.res`（主题 / StyleBox）在导出包里靠 `res://xxx.tres` 的路径互相引用，
# 单独解出来加载会失败（实测：`Failed loading resource: res://graphwar_theme.res`），
# 所以默认不解 —— 见 docs/121 §5.2。
RESOURCE_ALLOW = ("button_font",)


# --------------------------------------------------------------------- PCK

def iter_pck(path):
    """逐条产出 (文件名, 绝对偏移, 长度, md5)。Godot 4 的索引在包尾。"""
    with open(path, "rb") as f:
        data = f.read()
    mv = memoryview(data)
    magic, pack_ver, vmaj, vmin, vpat = struct.unpack_from("<IIIII", mv, 0)
    if magic != MAGIC:
        raise SystemExit("不是 Godot PCK（magic=0x%08x）：%s" % (magic, path))
    file_base, = struct.unpack_from("<Q", mv, 0x18)
    dir_offset, = struct.unpack_from("<Q", mv, 0x20)
    off = dir_offset
    count, = struct.unpack_from("<I", mv, off)
    off += 4
    for _ in range(count):
        plen, = struct.unpack_from("<I", mv, off)
        off += 4
        raw = mv[off:off + plen]
        off += (plen + 3) & ~3
        name = bytes(raw).rstrip(b"\x00").decode("utf-8", "replace")
        foff, fsize = struct.unpack_from("<QQ", mv, off)
        off += 16
        md5 = bytes(mv[off:off + 16])
        off += 16 + 4
        yield name, file_base + foff, fsize, md5, data


def read_pck_file(entry):
    _, off, size, _, data = entry
    return data[off:off + size]


def import_map(entries):
    """{导入产物路径: 源路径}。

    包里的 `<源路径>.import` / `<源路径>.remap` 是**导出后的 remap 存根**（不是编辑器那份
    `.import` 边车），形如：

        [remap]
        importer="font_data_dynamic"
        type="FontFile"
        uid="uid://3tk7wevjww0c"
        path="res://.godot/imported/ComicRelief-Bold.ttf-fc0760a4....fontdata"

    ⇒ **源路径就是存根自己的文件名**（去掉 `.import`/`.remap`），产物在 `path=`。
    这是"哪个 .ctex 是哪个 svg 变的"的唯一权威来源 —— 目录信息只在这里
    （`res://skins/bot_0.svg` 与 `res://faces/bot_0.svg` 的产物都叫 `bot_0.svg-<md5>.ctex`，
    只有 md5 不同，光看产物文件名是分不出目录的）。
    """
    out = {}
    for e in entries:
        name = e[0]
        for suf in (".import", ".remap"):
            if not name.endswith(suf):
                continue
            text = read_pck_file(e).decode("utf-8", "replace")
            src = dest = None
            for line in text.splitlines():
                line = line.strip()
                if line.startswith("source_file="):
                    src = line.split("=", 1)[1].strip().strip('"')
                elif line.startswith("path="):
                    dest = line.split("=", 1)[1].strip().strip('"')
            if src is None:
                src = "res://" + name[:-len(suf)]
            if dest:
                out[_clean_res_path(dest)] = _clean_res_path(src)
            break
    return out


def _clean_res_path(p):
    p = p.strip().strip('"')
    return p[len("res://"):] if p.startswith("res://") else p


# ------------------------------------------------------- project.binary

def parse_project_binary(blob):
    """解 Godot 的 `project.binary`（ECFG）。

    逐位观测到的布局（1346 字节、21 条记录、解完恰好到 EOF）：

        u32 magic 'ECFG' | u32 count |
        count × ( u32 key_len | key | u32 value_len | value_len 字节的 Variant )

    Variant = `u32 type` + 载荷；本项目里出现的 type 有
    `1=BOOL(4) 2=INT(4) 3=FLOAT(8) 4=STRING(4+len) 5=VECTOR2(8) 20=COLOR(16) 21=STRING_NAME(4+len) 28=ARRAY`。
    未知类型只保留长度、不猜内容（`value_len` 让跳过永远安全）。
    """
    if len(blob) < 8 or blob[:4] != b"ECFG":
        raise SystemExit("project.binary 不是 ECFG（magic=%r）" % blob[:4])
    o = 4
    count, = struct.unpack_from("<I", blob, o)
    o += 4
    out = {}
    for _ in range(count):
        klen, = struct.unpack_from("<I", blob, o)
        o += 4
        key = blob[o:o + klen].decode("utf-8", "replace")
        o += klen
        vlen, = struct.unpack_from("<I", blob, o)
        o += 4
        body = blob[o:o + vlen]
        o += vlen
        out[key] = _decode_variant(body)
    return out


def _decode_variant(body):
    t, = struct.unpack_from("<I", body, 0)
    p = 4
    if t == 1:
        return bool(struct.unpack_from("<I", body, p)[0])
    if t == 2:
        return struct.unpack_from("<i", body, p)[0]
    if t == 3:
        return struct.unpack_from("<d", body, p)[0]
    if t in (4, 21):
        n, = struct.unpack_from("<I", body, p)
        return body[p + 4:p + 4 + n].decode("utf-8", "replace")
    if t == 5:
        return list(struct.unpack_from("<2f", body, p))
    if t == 6:
        return list(struct.unpack_from("<2i", body, p))
    if t == 20:
        return [round(x, 6) for x in struct.unpack_from("<4f", body, p)]
    if t == 28:
        et, n = struct.unpack_from("<II", body, p)
        return {"_array_elem_type": et, "_array_len": n}
    return {"_unknown_variant_type": t, "_bytes": len(body) - 4}


# ------------------------------------------------------------------ 计划

def plan(entries):
    """决定要解哪些文件。返回 [(源路径, 落盘相对路径, 条目)]。

    落盘规则：**原版 `res://` 路径 + 实际容器扩展名**。这样客户端按原版名字找，
    同一个查找路径既能命中 `.ctex`（从 pck 解出来的），也能命中真 `.svg`
    （手上有完整工程的人放进去即可，代码不用改）。

    同一个源路径若同时有 `.svg` 与 `.png` 的导入产物（`basic_soldier` 就是这样），
    取 `.svg` 那份 —— 与恢复出的 `.tscn` 引用的是同一张图，且不是 png 的二次光栅化。
    """
    imap = import_map(entries)
    prefer = (".svg", ".png", ".webp", ".jpg")
    best = {}
    for e in entries:
        name = e[0]
        ext = name.rsplit(".", 1)[-1] if "." in name else ""
        if ext not in ("ctex", "fontdata", "res"):
            continue
        src = imap.get(name)
        if src is None or src.startswith("addons/"):
            continue
        stem, sext = src.rsplit(".", 1) if "." in src else (src, "")
        keep = False
        if ext == "ctex":
            keep = (src.startswith(TEXTURE_DIRS) or stem in TEXTURE_ROOT_EXTRA
                    or (os.path.dirname(src) == "" and sext in ("svg", "png")))
        elif ext == "fontdata":
            keep = sext in ("ttf", "otf")
        elif ext == "res":
            keep = os.path.basename(stem) in RESOURCE_ALLOW
        if not keep:
            continue
        rank = prefer.index("." + sext) if "." + sext in prefer else len(prefer)
        cur = best.get(stem)
        if cur is None or rank < cur[0]:
            best[stem] = (rank, src, stem + "." + ext, e)
    todo = [(src, dst, e) for _, src, dst, e in best.values()]
    todo.sort(key=lambda t: t[1])
    return todo


def check_ready(out_dir):
    """只靠 `MANIFEST.tsv` 判断产出目录是否齐备（不需要 pck）。"""
    man = os.path.join(out_dir, "MANIFEST.tsv")
    if not os.path.isfile(man):
        print("资产不齐：%s 不存在" % man, file=sys.stderr)
        return 1
    rows, missing = 0, []
    with open(man, encoding="utf-8") as f:
        for line in f:
            if line.startswith("#") or not line.strip():
                continue
            cols = line.rstrip("\n").split("\t")
            if len(cols) < 2:
                continue
            rows += 1
            if not os.path.isfile(os.path.join(out_dir, cols[1])):
                missing.append(cols[1])
    if rows == 0:
        print("资产不齐：MANIFEST.tsv 是空的", file=sys.stderr)
        return 1
    if missing:
        print("资产不齐：缺 %d/%d 个（前 5 个：%s）"
              % (len(missing), rows, ", ".join(missing[:5])), file=sys.stderr)
        return 1
    if not os.path.isfile(os.path.join(out_dir, "original_project_settings.json")):
        print("资产不齐：缺 original_project_settings.json", file=sys.stderr)
        return 1
    print("资产齐备：%d 个文件在 %s" % (rows, out_dir))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description="从 Graphwar.pck 解出客户端需要的媒体资源")
    ap.add_argument("--pck", default=DEFAULT_PCK)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--list", action="store_true", help="只列出会解出什么")
    ap.add_argument("--check", action="store_true",
                    help="只检查 --out 是否已齐备（缺就非零退出，CI/run.sh 用）")
    args = ap.parse_args(argv)

    # `--check` 只看产出目录，**不需要 pck**：这样 CI / 已经解过资产的机器也能用它做门禁，
    # 而且不会因为"pck 不在默认位置"把一份完好的 assets/ 判成没就绪。
    if args.check:
        return check_ready(args.out)

    if not os.path.isfile(args.pck):
        raise SystemExit(
            "找不到 %s\n"
            "  把正版 Graphwar.pck 放在这里，或显式传 --pck /path/to/Graphwar.pck" % args.pck)

    entries = list(iter_pck(args.pck))
    todo = plan(entries)

    if args.list:
        for src, dst, ent in todo:
            print("  %-46s <- res://%s  (%d B)" % (dst, src, ent[2]))
        print("共 %d 个" % len(todo))
        return 0

    os.makedirs(args.out, exist_ok=True)
    rows = []
    total = 0
    for src, dst, ent in todo:
        blob = read_pck_file(ent)
        path = os.path.join(args.out, dst)
        os.makedirs(os.path.dirname(path) or args.out, exist_ok=True)
        with open(path, "wb") as f:
            f.write(blob)
        total += len(blob)
        rows.append((src, dst, len(blob), hashlib.sha256(blob).hexdigest()))

    # project.binary → 原版工程设置（客户端用它校验自己的 project.godot，见 docs/121 §3）
    pb = None
    for e in entries:
        if e[0] == "project.binary":
            pb = e
            break
    if pb is None:
        raise SystemExit("包里没有 project.binary —— 这个 pck 不完整")
    settings = parse_project_binary(read_pck_file(pb))
    with open(os.path.join(args.out, "original_project_settings.json"), "w",
              encoding="utf-8") as f:
        json.dump(settings, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")

    with open(os.path.join(args.out, "MANIFEST.tsv"), "w", encoding="utf-8") as f:
        f.write("# source\tfile\tbytes\tsha256\n")
        for src, dst, n, h in rows:
            f.write("res://%s\t%s\t%d\t%s\n" % (src, dst, n, h))

    print("解出 %d 个文件（%.1f MB）到 %s" % (len(rows), total / 1e6, args.out))
    print("原版工程设置 %d 条 -> original_project_settings.json" % len(settings))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
