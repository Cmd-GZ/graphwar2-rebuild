#!/usr/bin/env python3
"""把正版 `Graphwar.pck` 里的**音频**导入产物解到 `client/assets/sounds/`。

为什么需要这个脚本（而不是直接 `load("res://sounds/xxx.mp3")`）
==============================================================
原版播放链路（`docs/111` §4.3/§4.4）分两条：

* 5 个 mp3 走 Godot `load("res://sounds/explosion.mp3")` —— 在**导出的包**里这条路径被
  `explosion.mp3.remap` 改写到 `res://.godot/imported/explosion.mp3-<md5>.mp3str`；
* `humm.wav` 走 **Rust `FileAccess::get_file_as_bytes` + hound 解析**，根本不进导入系统。

我们的客户端没有"导出 remap"这一层，包里也没有源 `.mp3`（只有 `.mp3str` 导入产物），
所以要在**资产准备阶段**把导入产物还原成 Godot 能直接 `load()` 的文件：

    .mp3str  → 抽出内层 mp3 字节 → assets/sounds/<name>.mp3    （AudioStreamMP3，与 load() 等价）
    .sample  → 抽出 PCM + 参数    → assets/sounds/<name>.wav    （AudioStreamWAV，RIFF 16-bit PCM）
    .wav(源) → 原样复制           → assets/sounds/humm.wav      （原版就是读源文件，见上）

**不做**的事：不改包、不执行包里的代码、不猜资源语义；每个产物都按字节记账
（解析必须恰好停在文件末尾的 `RSRC` 魔数之后），解析失败就报错退出。

Godot 4 二进制资源（`RSRC`）的布局（对照 `core/io/resource_format_binary.cpp`，
`FORMAT_VERSION = 5`，这些产物是 4.2.5 写的；`docs/111` §2.2 已逐字节验过）：

    "RSRC" | u32 big_endian | u32 use_real64 | u32 ver_major/minor/format
    ustring type | u64 importmd_ofs | u32 flags |
    [u64 uid if flags&2] [ustring script_class if flags&8] | 11 × u32 reserved |
    u32 string_count | ustring × string_count |
    u32 ext_count   | (ustring type, ustring path, u64 uid) × ext_count |
    u32 int_count   | (ustring path, u64 offset) × int_count |
    每个内部资源（在它的 offset 处）: ustring type | u32 prop_count |
        (u32 name_id, u32 variant_tag, payload) × prop_count
    "RSRC"

变体标签用的是**资源格式自己的枚举**（`NIL=1, BOOL=2, INT=3, FLOAT=4, STRING=5,
COLOR=20, OBJECT=24, DICTIONARY=26, ARRAY=30, PACKED_BYTE_ARRAY=31, ...`），
不是 `Variant::Type` —— `NIL` 是 1 不是 0，这一点弄错会整份解析错位。

用法
====
    python3 client/src/audio/extract_audio.py --pck /path/to/Graphwar.pck
    python3 client/src/audio/extract_audio.py --pck ... --out client/assets/sounds
    python3 client/src/audio/extract_audio.py --list      # 只打印会解出什么
    python3 client/src/audio/extract_audio.py --check     # 只看产物是否齐备（缺就非零退出）

产出目录已 gitignore（`client/assets/` 整目录），**不会**进仓库。

> 位置说明：为什么不是 `tools/extract_assets.py`？那个脚本目前只解图标/字体
> （`docs/121` §5），音频这条线是本轮新加的；按任务边界本 agent 只被允许新建
> `client/src/audio/`，所以把提取器放在它**输出**的模块旁边。若要合并进
> `tools/extract_assets.py`，是父 agent 一行 `plan()` 的事（见 `docs/128` §8）。
"""

import argparse
import hashlib
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CLIENT = os.path.dirname(os.path.dirname(HERE))       # client/
REPO = os.path.dirname(CLIENT)                        # gw2-rebuild/
WS = os.path.dirname(REPO)                            # 上一级（Graphwar.pck 所在处）
DEFAULT_PCK = os.path.join(WS, "Graphwar.pck")
DEFAULT_OUT = os.path.join(CLIENT, "assets", "sounds")

MAGIC = 0x43504447  # 'GDPC'

# 要解的 10 个导入产物：源路径（不含扩展名）→ 落盘名。
# 6 个 mp3 + 1 个 wav 源是**原版真的会播放/读取**的（docs/111 §4.4 的 6 条路径）；
# tear/humm_2/humm_b/shepard_2 在原版代码里没有引用（docs/111 §5.2 的负结果），
# 但仍然解出来 —— 客户端把它们作为"可用但未接线"的资源（docs/128 §4）。
MP3 = ["explosion", "squash", "spark", "pop", "unpop", "tear"]
SAMPLE = ["humm_2", "humm_b", "shepard_2"]
RAW_WAV = ["humm"]                 # 包里直接有源文件（原版自己用 hound 解析）

# AudioStreamWAV 的 format 枚举（Godot 4）：0 = 8-bit, 1 = 16-bit, 2 = IMA-ADPCM, 3 = QOA
WAV_FORMAT = {0: 8, 1: 16}


# --------------------------------------------------------------------- PCK

def iter_pck(path):
    """逐条产出 (文件名, 绝对偏移, 长度, md5)。Godot 4 的索引在包尾。"""
    with open(path, "rb") as f:
        data = f.read()
    mv = memoryview(data)
    magic, _pack_ver, _vmaj, _vmin, _vpat = struct.unpack_from("<IIIII", mv, 0)
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


def read_entry(entry):
    _, off, size, _, data = entry
    return data[off:off + size]


def import_map(entries):
    """{导入产物路径: 源路径}，来源是每条 `<源路径>.import` 存根里的 `path=`。"""
    out = {}
    for e in entries:
        name = e[0]
        if not name.endswith(".import"):
            continue
        text = read_entry(e).decode("utf-8", "replace")
        src = dest = None
        for line in text.splitlines():
            line = line.strip()
            if line.startswith("source_file="):
                src = line.split("=", 1)[1].strip().strip('"')
            elif line.startswith("path="):
                dest = line.split("=", 1)[1].strip().strip('"')
        if src is None:
            src = "res://" + name[:-len(".import")]
        if dest:
            out[_clean(dest)] = _clean(src)
    return out


def _clean(p):
    p = p.strip().strip('"')
    return p[len("res://"):] if p.startswith("res://") else p


# ------------------------------------------------------- RSRC 容器解析

class Blob:
    """带边界检查的顺序读取器：越界就是损坏，绝不静默补零。"""

    def __init__(self, data):
        self.d = data
        self.p = 0

    def left(self):
        return len(self.d) - self.p

    def u32(self, what="u32"):
        if self.left() < 4:
            raise ValueError("截断：%s @+%d（剩 %d 字节）" % (what, self.p, self.left()))
        v = struct.unpack_from("<I", self.d, self.p)[0]
        self.p += 4
        return v

    def u64(self, what="u64"):
        if self.left() < 8:
            raise ValueError("截断：%s @+%d（剩 %d 字节）" % (what, self.p, self.left()))
        v = struct.unpack_from("<Q", self.d, self.p)[0]
        self.p += 8
        return v

    def take(self, n, what="bytes"):
        if self.left() < n:
            raise ValueError("截断：%s 需要 %d @+%d（剩 %d）" % (what, n, self.p, self.left()))
        b = self.d[self.p:self.p + n]
        self.p += n
        return b

    def ustring(self, what="ustring"):
        n = self.u32(what + ".len")
        raw = self.take(n, what)
        if raw[-1:] != b"\x00":
            raise ValueError("%s 不是 NUL 结尾：%r" % (what, raw[:32]))
        return raw[:-1].decode("utf-8", "replace")


# 资源格式的变体标签（resource_format_binary.cpp；NIL 是 1，不是 0）
T_NIL, T_BOOL, T_INT, T_FLOAT, T_STRING = 1, 2, 3, 4, 5
T_VEC2, T_RECT2 = 10, 11
T_COLOR = 20
T_OBJECT, T_DICT, T_ARRAY = 24, 26, 30
T_PBA = 31
T_PACKED = {32: 4, 33: 4, 34: -1, 35: 12, 36: 16, 37: 8, 38: 8, 48: 8}  # tag -> 元素字节数


def read_variant(b, real64):
    tag = b.u32("variant.tag")
    if tag == T_NIL:
        return tag, None
    if tag == T_BOOL:
        return tag, bool(b.u32())
    if tag == T_INT:
        return tag, struct.unpack("<i", struct.pack("<I", b.u32()))[0]
    if tag == T_FLOAT:
        v = struct.unpack("<d", b.take(8, "float"))[0] if real64 \
            else struct.unpack("<f", b.take(4, "float"))[0]
        return tag, v
    if tag == T_STRING:
        return tag, b.ustring("string")
    if tag == T_PBA:
        n = b.u32("pba.len")
        raw = b.take(n, "pba")
        # 对齐：PBA 的载荷按 4 字节零填充（save 侧 _pad_buffer）
        pad = (-n) % 4
        if pad:
            filler = b.take(pad, "pba.pad")
            if any(filler):
                raise ValueError("PBA 填充非零：%r" % (filler,))
        return tag, raw
    if tag == T_VEC2:
        return tag, struct.unpack("<2f", b.take(8, "vec2"))
    if tag == T_RECT2:
        return tag, struct.unpack("<4f", b.take(16, "rect2"))
    if tag == T_COLOR:
        return tag, struct.unpack("<4f", b.take(16, "color"))
    if tag == T_DICT:
        n = b.u32("dict.len")
        for _ in range(n):
            read_variant(b, real64)
            read_variant(b, real64)
        return tag, None
    if tag == T_ARRAY:
        n = b.u32("array.len")
        for _ in range(n):
            read_variant(b, real64)
        return tag, None
    if tag in T_PACKED:
        n = b.u32("packed.len")
        if tag == 34:  # PackedStringArray 是 ustring 序列
            for _ in range(n):
                b.ustring("packed.str")
        else:
            b.take(n * T_PACKED[tag], "packed")
        return tag, None
    raise ValueError("无法跳过的变体标签 %d @+%d（没有证据时宁可失败）" % (tag, b.p))


def parse_rsrc(data):
    """解析一个 RSRC 文件 → (header_type, {prop 名: 值}, 主资源类型名)。

    严格性：解析结束后必须正好停在末尾 `RSRC` 魔数之后（`slack == 0`）。
    """
    if data[:4] != b"RSRC":
        raise ValueError("不是 RSRC 资源：%r" % (data[:4],))
    b = Blob(data)
    b.p = 4
    big_endian = b.u32("big_endian")
    real64 = bool(b.u32("use_real64"))
    if big_endian:
        raise ValueError("大端资源（本项目全是小端）")
    b.u32("ver_major")
    b.u32("ver_minor")
    b.u32("ver_format")
    header_type = b.ustring("type")
    b.u64("importmd_ofs")
    flags = b.u32("flags")
    if flags & 2:            # FLAG_UIDS
        b.u64("uid")
    if flags & 8:            # FLAG_HAS_SCRIPT_CLASS
        b.ustring("script_class")
    for _ in range(11):
        b.u32("reserved")
    str_count = b.u32("string_count")
    strings = [b.ustring("strtab") for _ in range(str_count)]
    ext_count = b.u32("ext_count")
    for _ in range(ext_count):
        b.ustring("ext.type")
        b.ustring("ext.path")
        b.u64("ext.uid")
    int_count = b.u32("int_count")
    internal = []
    for _ in range(int_count):
        path = b.ustring("int.path")
        offset = b.u64("int.offset")
        internal.append((path, offset))
    if not internal:
        raise ValueError("没有内部资源（%s）" % header_type)

    props_all = []
    for path, offset in internal:
        b.p = offset
        type_name = b.ustring("res.type")
        prop_count = b.u32("res.prop_count")
        props = {}
        for _ in range(prop_count):
            name_id = b.u32("prop.name_id")
            if name_id & 0x80000000:      # 内联字符串（低 31 位 = 字节数）
                n = name_id & 0x7FFFFFFF
                name = b.take(n, "prop.name").rstrip(b"\x00").decode("utf-8", "replace")
            else:
                name = strings[name_id]
            _tag, value = read_variant(b, real64)
            props[name] = value
        props_all.append((path, type_name, props, b.p))

    # 走到最后一个内部资源之后应当正好是结束魔数
    tail = data[props_all[-1][3]:props_all[-1][3] + 4]
    if tail != b"RSRC":
        raise ValueError("结束魔数不是 'RSRC'（在 +%d 读到 %r）" % (props_all[-1][3], tail))
    slack = len(data) - (props_all[-1][3] + 4)
    if slack != 0:
        raise ValueError("末尾还有 %d 字节未解释（容器账本不平）" % slack)
    return header_type, props_all[0][2], props_all[0][1]


# ------------------------------------------------------------ 落盘格式

def wav_from_pcm(pcm, rate, channels, bits):
    """套一个 44 字节 RIFF 头（16-bit 小端 PCM）。godot 的 AudioStreamWAV 直接吃这个。"""
    bits_per_sample = WAV_FORMAT.get(bits)
    if bits_per_sample is None:
        raise ValueError("不支持的 AudioStreamWAV.format=%r" % (bits,))
    if bits_per_sample == 16:
        block_align = channels * 2
        fmt_tag, byte_rate = 1, rate * block_align
        body = pcm
    else:                    # 8-bit：Godot 存的是无符号 8-bit
        block_align = channels
        fmt_tag, byte_rate = 1, rate * block_align
        body = pcm
    if len(body) % block_align != 0:
        raise ValueError("PCM 长度 %d 不是帧长 %d 的整数倍" % (len(body), block_align))
    hdr = b"RIFF" + struct.pack("<I", 36 + len(body)) + b"WAVE"
    hdr += b"fmt " + struct.pack("<IHHIIHH", 16, fmt_tag, channels, rate,
                                 byte_rate, block_align, bits_per_sample)
    hdr += b"data" + struct.pack("<I", len(body))
    return hdr + body


def mp3_from_container(blob):
    header_type, props, _kind = parse_rsrc(blob)
    if header_type != "AudioStreamMP3":
        raise ValueError("期望 AudioStreamMP3，实际 %s" % header_type)
    data = props.get("data")
    if not isinstance(data, (bytes, bytearray)) or not data:
        raise ValueError("AudioStreamMP3 没有 data 属性")
    return bytes(data)


def wav_from_container(blob):
    header_type, props, _kind = parse_rsrc(blob)
    if header_type != "AudioStreamWAV":
        raise ValueError("期望 AudioStreamWAV，实际 %s" % header_type)
    data = props.get("data")
    if not isinstance(data, (bytes, bytearray)) or not data:
        raise ValueError("AudioStreamWAV 没有 data 属性")
    rate = int(props.get("mix_rate", 0) or 0)
    if rate <= 0:
        raise ValueError("AudioStreamWAV 的 mix_rate 非法：%r" % (props.get("mix_rate"),))
    stereo = bool(props.get("stereo", False))
    fmt = int(props.get("format", 1))
    if fmt not in WAV_FORMAT:
        raise ValueError("AudioStreamWAV.format=%d 不在 8/16-bit 范围内" % fmt)
    return wav_from_pcm(bytes(data), rate, 2 if stereo else 1, fmt)


# ------------------------------------------------------------------ 计划

def plan(entries):
    """返回 [(落盘名, 目标相对路径, 条目, 种类)]。"""
    imap = import_map(entries)          # 产物 -> 源
    # 按**文件名含扩展名**索引（`explosion.mp3`），因为 mp3 与 wav 可能同名不同类。
    by_stem = {}
    for prod, src in imap.items():
        by_stem.setdefault(os.path.basename(src), []).append((prod, src))
    index = {}
    for e in entries:
        index[e[0]] = e

    todo = []
    for name in MP3:
        for prod, _src in by_stem.get(name + ".mp3", []):
            if prod.endswith(".mp3str") and prod in index:
                todo.append((name, name + ".mp3", index[prod], "mp3str"))
    for name in SAMPLE:
        for prod, _src in by_stem.get(name + ".wav", []):
            if prod.endswith(".sample") and prod in index:
                todo.append((name, name + ".wav", index[prod], "sample"))
    for name in RAW_WAV:
        for cand in (name + ".wav", "sounds/" + name + ".wav"):
            if cand in index:
                todo.append((name, name + ".wav", index[cand], "raw"))
                break
    todo.sort(key=lambda t: t[1])
    return todo


def check_ready(out_dir):
    missing = [n for n in [m + ".mp3" for m in MP3] + [s + ".wav" for s in SAMPLE]
               + [w + ".wav" for w in RAW_WAV]
               if not os.path.isfile(os.path.join(out_dir, n))]
    if missing:
        print("音频资产不齐（%s）：缺 %d 个：%s"
              % (out_dir, len(missing), ", ".join(missing)), file=sys.stderr)
        return 1
    print("音频资产齐备：%d 个文件在 %s" % (len(MP3) + len(SAMPLE) + len(RAW_WAV), out_dir))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description="从 Graphwar.pck 解出客户端音频")
    ap.add_argument("--pck", default=DEFAULT_PCK)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--list", action="store_true", help="只列出会解出什么")
    ap.add_argument("--check", action="store_true", help="只检查产物是否齐备")
    args = ap.parse_args(argv)

    if args.check:
        return check_ready(args.out)

    if not os.path.isfile(args.pck):
        raise SystemExit("找不到 %s\n  用 --pck /path/to/Graphwar.pck 指定" % args.pck)

    entries = list(iter_pck(args.pck))
    todo = plan(entries)
    got = {t[0] for t in todo}
    for n in MP3 + SAMPLE + RAW_WAV:
        if n not in got:
            print("[warn] 包里找不到 %s 的导入产物（跳过）" % n, file=sys.stderr)

    if args.list:
        for name, dst, ent, kind in todo:
            print("  %-22s <- %-46s (%s, %d B)" % (dst, ent[0], kind, ent[2]))
        print("共 %d 个" % len(todo))
        return 0

    os.makedirs(args.out, exist_ok=True)
    rows = []
    for name, dst, ent, kind in todo:
        blob = read_entry(ent)
        if kind == "mp3str":
            out = mp3_from_container(blob)
        elif kind == "sample":
            out = wav_from_container(blob)
        else:
            if blob[:4] != b"RIFF":
                raise SystemExit("源文件 %s 不是 RIFF/WAV（%r）" % (ent[0], blob[:4]))
            out = blob
        path = os.path.join(args.out, dst)
        with open(path, "wb") as f:
            f.write(out)
        rows.append((dst, ent[0], len(out), hashlib.sha256(out).hexdigest()))
        print("  %-22s %7d B  (%s → %s)" % (dst, len(out), kind, ent[0]))

    man = os.path.join(args.out, "AUDIO_MANIFEST.tsv")
    with open(man, "w", encoding="utf-8") as f:
        f.write("# file\tsource\tbytes\tsha256\n")
        for dst, src, n, h in rows:
            f.write("%s\t%s\t%d\t%s\n" % (dst, src, n, h))
    print("解出 %d 个音频文件到 %s（%d B）" % (len(rows), args.out,
                                              sum(r[2] for r in rows)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
