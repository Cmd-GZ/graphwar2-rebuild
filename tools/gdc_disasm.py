#!/usr/bin/env python3
"""Disassemble Godot 4 compiled GDScript (.gdc) — token level, no decompiler needed.

Why: there is **no** GDScript decompiler on this machine (docs/92 §2.1), so
`tools/gdc_strings.py` could only recover the string pool. But a `.gdc` is not a
black box: after `GDSC` + zstd the payload is a fully typed token stream with an
identifier table, a typed constant table, a line table and a column table. Parsing
those four sections gives, deterministically:

  * every identifier (function names, signal names, member names, node paths);
  * every constant with its Godot Variant type (int / float / String / StringName /
    bool / null / Vector2 ...), which is where the layout numbers live;
  * the function/method list with parameter counts;
  * a readable pseudo-source (one statement per source line) good enough to see
    what a helper script does, without pretending to be a decompiler.

The payload format below was **verified against a same-version compiler**: this
machine ships Godot 4.7.2 (`tools/Godot_v4.7.2-stable_linux.x86_64`), and
`--export-pack` compiles `.gd` → `.gdc` with the exact bytecode version 101 the
game shipped. Compiling probe scripts and aligning them with their bytes is how the
token-type table in §3 of `docs/101-gdc-contents.md` was derived — every number in
it comes from a compiled probe, not from guesswork. See that doc for the table and
for how to reproduce the probe.

Container (Godot 4.x, verified on 4.7.2 exports — same as `gdc_strings.py`):

    offset 0   "GDSC"
    offset 4   u32   bytecode version (101 here)
    offset 8   u32   decompressed payload size
    offset 12  zstd frame -> payload

Payload (all little-endian):

    u32 identifier_count
    u32 constant_count
    u32 line_count                    # number of "source line" entries
    u32 token_count
    identifiers[identifier_count]     # u32 nchars, then nchars x u32 (c ^ 0xb6b6b6b6)
    constants[constant_count]         # Godot Variant encoding (type|flags<<16, payload)
    lines[line_count]                 # u32 (first_token_index, source_line)
    columns[line_count]               # u32 (first_token_index, column + 1)   # tab counts as 1 char
    tokens[token_count]               # u32 (type | operand << 8, source_line)

Token operand semantics by type (see TOKEN_NAMES): identifiers index the identifier
table, literals index the constant table, annotations index the identifier table
(the identifier text is the full annotation, e.g. "@onready"), everything else has
operand 0.

Usage:
    tools/gdc_disasm.py --all rsc/                 # report: the 5 behavioural scripts + layout table
    tools/gdc_disasm.py --all rsc/ --json out.json # + machine-readable model
    tools/gdc_disasm.py --all rsc/ --tsv layout.tsv
    tools/gdc_disasm.py rsc/ExitButtonContainer.gdc --pseudo
    tools/gdc_disasm.py rsc/ChatBox.gdc --json -   # single script JSON to stdout

Exit codes: 0 ok, 2 missing dependency, 1 at least one file failed to parse.

Only external dependency: `zstandard` (same as gdc_strings.py). Nothing is
executed; `.gdc` bytes are parsed, never run.
"""
import argparse
import json
import os
import struct
import sys

try:
    import zstandard
except ImportError:  # pragma: no cover - environment guard
    sys.exit("zstandard is required (pip install zstandard); nothing was downloaded")

MAGIC = b"GDSC"
BYTECODE_VERSION = 101          # 4.7.2; older/newer versions may renumber tokens
XOR = 0xB6B6B6B6                # per-u32 string obfuscation

# ---------------------------------------------------------------------------
# Token types. Verified by compiling probe scripts with Godot 4.7.2 and reading
# the emitted token stream (docs/101 §3). Types not listed here were never
# emitted by the game or by the probes; they are reported as <Tnnn>.
# ---------------------------------------------------------------------------
IDENT = 130        # operand -> identifier index
LITERAL = 131      # operand -> constant index
ANNOTATION = 129   # operand -> identifier index (text is "@tool", "@export", ...)

TOKEN_NAMES = {
    IDENT: "<ident>", LITERAL: "<literal>", ANNOTATION: "<annotation>",
    132: "<", 133: "<=", 134: ">", 135: ">=", 136: "==", 137: "!=",
    138: "and", 139: "or", 140: "not", 141: "&&", 142: "||", 143: "!",
    144: "&", 145: "|", 146: "~", 147: "^", 148: "<<", 149: ">>",
    150: "+", 151: "-", 152: "*", 153: "**", 154: "/", 155: "%", 156: "=",
    157: "+=", 158: "-=", 159: "*=", 160: "**=", 161: "/=", 162: "%=",
    163: "<<=", 164: ">>=", 165: "&=", 166: "|=", 167: "^=",
    168: "if", 169: "elif", 170: "else", 171: "for", 172: "while",
    173: "break", 174: "continue", 175: "pass", 176: "return", 177: "match",
    178: "when", 179: "as", 180: "assert", 181: "await", 183: "class",
    184: "class_name", 185: "const", 186: "enum", 187: "extends", 188: "func",
    189: "in", 190: "is", 192: "preload", 193: "self", 194: "signal",
    195: "static", 196: "super", 198: "var", 199: "void",
    201: "[", 202: "]", 203: "{", 204: "}", 205: "(", 206: ")", 207: ",",
    208: ";", 209: ".", 212: ":", 213: "$", 214: "->", 215: "_",
    219: "PI", 220: "TAU", 221: "INF", 222: "NAN",
}

# Godot Variant type ids (core/variant/variant.h) used by the constant table.
VARIANT_NAMES = {
    0: "nil", 1: "bool", 2: "int", 3: "float", 4: "String", 21: "StringName",
    22: "NodePath", 5: "Vector2", 6: "Vector2i", 20: "Color",
}


# ---------------------------------------------------------------------------
# Container / payload decoding
# ---------------------------------------------------------------------------
def decode_container(raw):
    """Split a .gdc into (bytecode_version, payload bytes)."""
    if raw[:4] != MAGIC:
        raise ValueError("not a .gdc (bad magic %r)" % raw[:4])
    version, size = struct.unpack_from("<II", raw, 4)
    payload = zstandard.ZstdDecompressor().decompress(raw[12:], max_output_size=64 << 20)
    if size and len(payload) != size:
        raise ValueError("size mismatch: header %d, decoded %d" % (size, len(payload)))
    return version, payload


def _u32(payload, off):
    return struct.unpack_from("<I", payload, off)[0]


def read_identifier(payload, off):
    """One identifier: u32 length, then `length` u32 codepoints XOR 0xb6b6b6b6."""
    n = _u32(payload, off)
    if n > 4096 or off + 4 + 4 * n > len(payload):
        raise ValueError("bad identifier length %d at %d" % (n, off))
    chars = []
    for i in range(n):
        c = _u32(payload, off + 4 + 4 * i) ^ XOR
        if c > 0x10FFFF:
            raise ValueError("identifier char out of range at %d" % off)
        chars.append(chr(c))
    return "".join(chars), off + 4 + 4 * n


def decode_variant(payload, off):
    """Decode one Godot Variant (the constant table). Returns (typed_value, next_off).

    Only the encodings the game and the probes actually use are implemented; an
    unknown type raises instead of silently mis-aligning the whole table.
    """
    word = _u32(payload, off)
    typ, flags = word & 0xFFFF, word >> 16
    o = off + 4
    if typ == 0:                                     # NIL
        return ("nil", None), o
    if typ == 1:                                     # BOOL (u32)
        return ("bool", bool(_u32(payload, o))), o + 4
    if typ == 2:                                     # INT, flags&1 -> 64-bit
        if flags & 1:
            return ("int", struct.unpack_from("<q", payload, o)[0]), o + 8
        return ("int", struct.unpack_from("<i", payload, o)[0]), o + 4
    if typ == 3:                                     # FLOAT, flags&1 -> double
        if flags & 1:
            return ("float", struct.unpack_from("<d", payload, o)[0]), o + 8
        return ("float", struct.unpack_from("<f", payload, o)[0]), o + 4
    if typ in (4, 21):                               # STRING / STRING_NAME
        n = _u32(payload, o)
        text = payload[o + 4:o + 4 + n].rstrip(b"\x00").decode("utf-8", "replace")
        return (("string" if typ == 4 else "stringname"), text), o + 4 + ((n + 3) // 4) * 4
    if typ == 22:                                    # NODE_PATH (observed layout)
        head = _u32(payload, o)
        names = head & 0x7FFFFFFF
        subnames = _u32(payload, o + 4)
        pad = _u32(payload, o + 8)
        if pad != 0:
            raise ValueError("NodePath: unexpected third word %#x at %d" % (pad, off))
        o += 12
        parts = []
        for _ in range(names + subnames):
            n = _u32(payload, o)
            parts.append(payload[o + 4:o + 4 + n].decode("utf-8", "replace"))
            o += 4 + ((n + 3) // 4) * 4
        return ("nodepath", "/".join(parts)), o
    if typ == 5:                                     # Vector2 (2 x float32)
        return ("Vector2", struct.unpack_from("<ff", payload, o)), o + 8
    if typ == 6:                                     # Vector2i
        return ("Vector2i", struct.unpack_from("<ii", payload, o)), o + 8
    raise ValueError("unsupported constant type %d (flags %d) at offset %d"
                     % (typ, flags, off))


# ---------------------------------------------------------------------------
# Script model
# ---------------------------------------------------------------------------
class Script(object):
    """One parsed .gdc."""

    def __init__(self, path):
        self.path = path
        self.name = os.path.basename(path)
        self.version = None
        self.identifiers = []
        self.constants = []          # [(kind, value)] in table order
        self.lines = []              # [(first_token_index, line)]
        self.columns = []            # [(first_token_index, column+1)]
        self.tokens = []             # [(value, line)]
        self.warnings = []
        self.is_plugin = (os.sep + "addons" + os.sep) in os.path.abspath(path)

    # -- tokens ---------------------------------------------------------
    def token_type(self, i):
        return self.tokens[i][0] & 0xFF

    def token_operand(self, i):
        return self.tokens[i][0] >> 8

    def token_text(self, i):
        """Render one token as text (identifier/constant resolved)."""
        t = self.token_type(i)
        op = self.token_operand(i)
        if t == IDENT:
            return self.identifiers[op] if op < len(self.identifiers) else "<id%d?>" % op
        if t == ANNOTATION:
            return self.identifiers[op] if op < len(self.identifiers) else "@<ann%d?>" % op
        if t == LITERAL:
            return literal_repr(self.constants[op]) if op < len(self.constants) \
                else "<lit%d?>" % op
        return TOKEN_NAMES.get(t, "<T%d>" % t)

    # -- derived views --------------------------------------------------
    def functions(self):
        """All `func` declarations: name, parameter count, line, static."""
        out = []
        for i in range(len(self.tokens)):
            if self.token_type(i) != 188:            # 'func'
                continue
            static = i > 0 and self.token_type(i - 1) == 195
            j = i + 1
            if j >= len(self.tokens) or self.token_type(j) != IDENT:
                continue
            name = self.token_text(j)
            line = self.tokens[i][1]
            params = 0
            if j + 1 < len(self.tokens) and self.token_type(j + 1) == 205:
                params = self._count_params(j + 1)
            out.append({"name": name, "params": params, "line": line, "static": static})
        return out

    def _count_params(self, open_idx):
        """Count top-level parameters between '(' and its matching ')'."""
        depth = 0
        commas = 0
        has_content = False
        for k in range(open_idx, len(self.tokens)):
            t = self.token_type(k)
            if t in (205, 201, 203):                 # ( [ {
                depth += 1
                if depth > 1:
                    has_content = True
                continue
            if t in (206, 202, 204):                 # ) ] }
                depth -= 1
                if depth == 0:
                    return (commas + 1) if has_content else 0
                continue
            if depth == 1 and t == 207:              # ','
                commas += 1
                continue
            has_content = True
        return 0

    def signals(self):
        out = []
        for i in range(len(self.tokens) - 1):
            if self.token_type(i) == 194 and self.token_type(i + 1) == IDENT:
                out.append(self.token_text(i + 1))
        return out

    def members(self):
        """Identifiers reached through '.', split into calls and property reads."""
        calls, props = [], []
        for i in range(len(self.tokens) - 1):
            if self.token_type(i) != 209 or self.token_type(i + 1) != IDENT:
                continue
            name = self.token_text(i + 1)
            if i + 2 < len(self.tokens) and self.token_type(i + 2) == 205:
                if name not in calls:
                    calls.append(name)
            elif name not in props:
                props.append(name)
        return {"calls": calls, "properties": props}

    def local_vars(self):
        out = []
        for i in range(len(self.tokens) - 1):
            if self.token_type(i) == 198 and self.token_type(i + 1) == IDENT:
                name = self.token_text(i + 1)
                if name not in out:
                    out.append(name)
        return out

    def source_lines(self):
        """Group tokens by their source line: [(line, [token indices])]."""
        groups = []
        for i, (_v, line) in enumerate(self.tokens):
            if groups and groups[-1][0] == line:
                groups[-1][1].append(i)
            else:
                groups.append((line, [i]))
        return groups

    def render_range(self, indices):
        return render_tokens([self.token_text(i) for i in indices])

    def pseudo_source(self):
        return "\n".join("%4d  %s" % (line, self.render_range(idxs))
                         for line, idxs in self.source_lines())

    def theme_calls(self):
        """add_theme_font_size_override / add_theme_constant_override calls.

        Returns one record per call with the theme key (first literal argument, when
        it is a String) and the rendered expression(s) that follow it.
        """
        out = []
        names = ("add_theme_font_size_override", "add_theme_constant_override",
                 "add_theme_stylebox_override", "add_theme_color_override",
                 "remove_theme_font_size_override", "remove_theme_constant_override")
        for i in range(len(self.tokens)):
            if self.token_type(i) != IDENT or self.token_text(i) not in names:
                continue
            if i + 1 >= len(self.tokens) or self.token_type(i + 1) != 205:
                continue
            end = match_paren(self, i + 1)
            if end is None:
                continue
            args = split_args(self, i + 1, end)
            key = None
            if args and len(args[0]) == 1 and self.token_type(args[0][0]) == LITERAL:
                k = self.constants[self.token_operand(args[0][0])]
                if k[0] in ("string", "stringname"):
                    key = k[1]
            value = self.render_range(args[1]) if len(args) > 1 else ""
            out.append({
                "call": self.token_text(i),
                "key": key,
                "value": value,
                "line": self.tokens[i][1],
                "constant_indices": [self.token_operand(k) for k in
                                     (idx for a in args[1:] for idx in a)
                                     if self.token_type(k) == LITERAL],
            })
        return out

    def numeric_constants(self):
        """All int/float constants in table order."""
        return [(i, k, v) for i, (k, v) in enumerate(self.constants) if k in ("int", "float")]

    def size_expressions(self):
        """Source lines that compute a size: font_size / custom_minimum_size / margins."""
        keys = set(("font_size", "custom_minimum_size", "desired_size", "minimum_size",
                    "margin", "margin_left", "margin_right", "margin_top", "margin_bottom",
                    "separation", "safe_area", "minimum_y", "extra_y", "target_y",
                    "pos", "pos_x_increment", "pos_y_increment", "margin_value",
                    "parent_size", "target_size", "window_size", "ad_size"))
        keys.update(c["key"] for c in self.theme_calls() if c["key"])
        out = []
        for line, idxs in self.source_lines():
            texts = [self.token_text(i) for i in idxs]
            if any(t in keys for t in texts) and any(
                    self.token_type(i) == LITERAL and
                    self.constants[self.token_operand(i)][0] in ("int", "float")
                    for i in idxs):
                out.append({"line": line, "text": self.render_range(idxs)})
        return out


def literal_repr(const):
    kind, value = const
    if kind == "string":
        return '"%s"' % value
    if kind == "stringname":
        return '&"%s"' % value
    if kind == "nodepath":
        return '^"%s"' % value
    if kind == "nil":
        return "null"
    if kind == "bool":
        return "true" if value else "false"
    if kind == "int":
        return str(value)
    if kind == "float":
        return repr(value)
    if kind in ("Vector2", "Vector2i"):
        return "%s(%s, %s)" % (kind, value[0], value[1])
    return str(value)


def render_tokens(texts):
    """Join token texts into readable code (spacing heuristics, not a formatter)."""
    out = ""
    for t in texts:
        if not out:
            out = t
            continue
        prev = out[-1]
        if t in (")", "]", ",", ":", ";", ".", "(") and t != "(":
            out += t
        elif t == "=" and prev == ":":
            out += t                                # ":" + "=" -> ":=" (tokenizer splits it)
        elif t == "}":
            out += t if prev == "{" else " " + t
        elif t == "(":
            out += t if (prev.isalnum() or prev in '_\'"') else " " + t
        elif t in ("[",):
            out += t if (prev.isalnum() or prev in '_\'"') else " " + t
        elif t in ("]", "}"):
            out += t
        elif prev in "([.":
            out += t
        elif t in ("->", "="):
            out += " " + t
        elif t in ("!", "~"):
            out += t if (prev.isalnum() or prev in ')_]\'"') else " " + t
        else:
            out += " " + t
    return out


def match_paren(script, open_idx):
    """Index of the ')' matching tokens[open_idx] == '(' (else None)."""
    depth = 0
    for k in range(open_idx, len(script.tokens)):
        t = script.token_type(k)
        if t in (205, 201, 203):
            depth += 1
        elif t in (206, 202, 204):
            depth -= 1
            if depth == 0:
                return k if t == 206 else None
    return None


def split_args(script, open_idx, close_idx):
    """Split a call argument list into token-index lists."""
    args, cur, depth = [], [], 0
    for k in range(open_idx + 1, close_idx):
        t = script.token_type(k)
        if t in (205, 201, 203):
            depth += 1
        elif t in (206, 202, 204):
            depth -= 1
        if t == 207 and depth == 0:
            args.append(cur)
            cur = []
        else:
            cur.append(k)
    if cur:
        args.append(cur)
    return args


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------
def parse_gdc(path):
    raw = open(path, "rb").read()
    version, payload = decode_container(raw)
    s = Script(path)
    s.version = version
    if version != BYTECODE_VERSION:
        s.warnings.append("bytecode version %d is not the verified %d; token map may be wrong"
                          % (version, BYTECODE_VERSION))
    id_count, const_count, line_count, token_count = struct.unpack_from("<4I", payload, 0)
    off = 16
    for _ in range(id_count):
        text, off = read_identifier(payload, off)
        s.identifiers.append(text)
    for _ in range(const_count):
        value, off = decode_variant(payload, off)
        s.constants.append(value)
    for _ in range(line_count):
        s.lines.append(struct.unpack_from("<2I", payload, off))
        off += 8
    for _ in range(line_count):
        s.columns.append(struct.unpack_from("<2I", payload, off))
        off += 8
    for _ in range(token_count):
        s.tokens.append(struct.unpack_from("<2I", payload, off))
        off += 8
    if off != len(payload):
        raise ValueError("%d trailing bytes after token stream (parsed %d of %d)"
                         % (len(payload) - off, off, len(payload)))
    return s


# ---------------------------------------------------------------------------
# Classification (evidence flags, not guesses)
# ---------------------------------------------------------------------------
# docs/92 §2.3: the only five non-plugin scripts with behaviour beyond layout.
BEHAVIOURAL = [
    ("CampaingLevelsButtons", "creates the 21 campaign level buttons at runtime"),
    ("ShowExtraButtons", "toggles the extra-buttons row and swaps its icon"),
    ("ChatBox", "clicking the chat area focuses ChatLineEdit"),
    ("PlayerTabContainer", "sets the three tab titles"),
    ("OptionsContainer", "resized -> call_deferred(set_minimum_size)"),
]


def classify(script):
    """Coarse, evidence-based bucket used by the report."""
    names = set(script.identifiers)
    has_theme = any(n.startswith(("add_theme_", "remove_theme_")) for n in names)
    has_size = bool({"custom_minimum_size", "minimum_size"} & names)
    if script.name[:-4] in [b[0] for b in BEHAVIOURAL]:
        return "behavioural"
    if not script.tokens and not script.constants:
        return "empty"
    if has_theme:
        return "theme-helper"
    if has_size:
        return "geometry-helper"
    if not script.constants and len(script.tokens) <= 20:
        return "shell"
    return "other"


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
def try_parse(path):
    try:
        return parse_gdc(path), None
    except Exception as exc:                        # keep going: one bad file must not hide the rest
        return None, "!! %s: %s" % (path, exc)


def collect(targets):
    scripts, errors = [], []
    for path in targets:
        s, err = try_parse(path)
        if err:
            errors.append(err)
        else:
            scripts.append(s)
    scripts.sort(key=lambda s: s.name)
    return scripts, errors


def find_gdc(directory):
    out = []
    for root, _dirs, files in os.walk(directory):
        for f in sorted(files):
            if f.endswith(".gdc"):
                out.append(os.path.join(root, f))
    return out


def print_behavioural(scripts):
    by_name = {s.name[:-4]: s for s in scripts}
    print("=" * 78)
    print("A. 有行为的 5 个脚本（名单来自 docs/92 §2.3；下面全部由字节码实测得到）")
    print("=" * 78)
    for name, role in BEHAVIOURAL:
        s = by_name.get(name)
        if s is None:
            print("\n-- %s.gdc: 未在输入中找到" % name)
            continue
        print("\n-- %s.gdc  [%s]" % (name, role))
        print("   字节码 v%d  标识符 %d  常量 %d  token %d" %
              (s.version, len(s.identifiers), len(s.constants), len(s.tokens)))
        funcs = s.functions()
        if funcs:
            print("   函数: " + ", ".join(
                "%s(%d)%s" % (f["name"], f["params"], " [static]" if f["static"] else "")
                for f in funcs))
        if s.signals():
            print("   信号: " + ", ".join(s.signals()))
        mem = s.members()
        if mem["calls"] or mem["properties"]:
            print("   成员调用: " + ", ".join(mem["calls"]))
            print("   成员属性: " + ", ".join(mem["properties"]))
        if s.constants:
            print("   常量: " + ", ".join("[%d] %s" % (i, literal_repr(c))
                                        for i, c in enumerate(s.constants)))
        tc = s.theme_calls()
        if tc:
            print("   主题/布局调用:")
            for c in tc:
                print("     L%-3d %s(%s, %s)" % (c["line"], c["call"],
                                                 '"%s"' % c["key"] if c["key"] else "?", c["value"]))
        print("   伪源码:")
        for line, idxs in s.source_lines():
            print("     %4d  %s" % (line, s.render_range(idxs)))


def layout_rows(scripts):
    rows = []
    for s in scripts:
        if s.is_plugin:
            continue
        cat = classify(s)
        if cat == "behavioural":
            continue
        nums = s.numeric_constants()
        theme = s.theme_calls()
        purpose = []
        for c in theme:
            if c["value"] and c["value"] != c["key"]:
                purpose.append("%s = %s" % (c["key"] or "?", c["value"]))
        purpose.extend(e["text"] for e in s.size_expressions())
        rows.append({
            "script": s.name,
            "kind": cat,
            "funcs": ",".join(f["name"] for f in s.functions()),
            "numbers": "; ".join("%s" % literal_repr((k, v)) for _i, k, v in nums),
            "theme": "; ".join("%s<- %s" % (c["key"] or "?", c["value"]) for c in theme),
            "sizes": " | ".join(e["text"] for e in s.size_expressions()),
            "purpose": " | ".join(purpose),
        })
    return rows


def print_layout_table(rows):
    print()
    print("=" * 78)
    print("B. 布局助手数值表（非插件、非 5 个行为脚本；数值即字节码常量表原文）")
    print("=" * 78)
    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    print("分类计数: " + ", ".join("%s=%d" % kv for kv in sorted(kinds.items()))
          + "   （仅非插件脚本 %d 个）" % len(rows))
    print()
    print("%-31s %-16s %-24s %s" % ("script", "kind", "numbers", "theme keys / size expressions"))
    print("-" * 118)
    for r in rows:
        print("%-31s %-16s %-24s %s" % (r["script"], r["kind"], r["numbers"], r["purpose"]))


def script_to_json(s):
    return {
        "path": s.path,
        "name": s.name,
        "bytecode_version": s.version,
        "identifiers": s.identifiers,
        "constants": [{"index": i, "type": k, "value": v}
                      for i, (k, v) in enumerate(s.constants)],
        "functions": s.functions(),
        "signals": s.signals(),
        "members": s.members(),
        "local_vars": s.local_vars(),
        "theme_calls": s.theme_calls(),
        "size_expressions": s.size_expressions(),
        "category": classify(s),
        "plugin": s.is_plugin,
        "pseudo_source": s.pseudo_source(),
        "warnings": s.warnings,
    }


def write_json(path, scripts):
    data = {
        "generator": "gdc_disasm.py",
        "bytecode_version": BYTECODE_VERSION,
        "token_names": {str(k): v for k, v in sorted(TOKEN_NAMES.items())},
        "scripts": [script_to_json(s) for s in scripts],
    }
    text = json.dumps(data, ensure_ascii=False, indent=1)
    if path == "-":
        print(text)
    else:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text + "\n")
        print("wrote %s (%d scripts)" % (path, len(scripts)))


def write_markdown(path, rows):
    lines = ["| 脚本 | 分类 | 数值常量（常量表原文） | 关键表达式（伪源码原文） |",
             "|---|---|---|---|"]
    for r in rows:
        lines.append("| `%s` | %s | %s | %s |" % (
            r["script"], r["kind"], r["numbers"].replace("|", "\\|") or "—",
            r["purpose"].replace("|", "\\|") or "—"))
    text = "\n".join(lines) + "\n"
    if path == "-":
        print(text)
    else:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        print("wrote %s (%d rows)" % (path, len(rows)))


def write_tsv(path, rows):
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("script\tkind\tfunctions\tnumbers\ttheme_keys\tsize_expressions\n")
        for r in rows:
            fh.write("\t".join(r[k].replace("\t", " ") for k in
                               ("script", "kind", "funcs", "numbers", "theme", "sizes")) + "\n")
    print("wrote %s (%d rows)" % (path, len(rows)))


def main(argv):
    ap = argparse.ArgumentParser(description="Parse Godot 4 .gdc bytecode (see docs/101).")
    ap.add_argument("paths", nargs="*", help=".gdc files")
    ap.add_argument("--all", metavar="DIR", help="every *.gdc under DIR")
    ap.add_argument("--json", metavar="OUT", help="write the full model ('-' = stdout)")
    ap.add_argument("--tsv", metavar="OUT", help="write the layout table as TSV")
    ap.add_argument("--md", metavar="OUT", help="write the layout table as Markdown")
    ap.add_argument("--pseudo", action="store_true", help="print pseudo-source only")
    ap.add_argument("--no-report", action="store_true", help="skip the two report sections")
    a = ap.parse_args(argv)

    targets = list(a.paths)
    if a.all:
        targets.extend(find_gdc(a.all))
    if not targets:
        ap.error("no .gdc given (pass files or --all DIR)")

    scripts, errors = collect(targets)
    if a.pseudo:
        for s in scripts:
            print("== %s  v%d  ids=%d consts=%d tokens=%d" %
                  (s.name, s.version, len(s.identifiers), len(s.constants), len(s.tokens)))
            print(s.pseudo_source())
            print()
    elif not a.no_report:
        print_behavioural(scripts)
        rows = layout_rows(scripts)
        print_layout_table(rows)
        print()
        print("解析: %d 个脚本，失败 %d 个" % (len(scripts), len(errors)))
        for e in errors:
            print("  " + e)
        kinds = {}
        for s in scripts:
            k = classify(s)
            kinds[k] = kinds.get(k, 0) + 1
        print("分类: " + ", ".join("%s=%d" % kv for kv in sorted(kinds.items())))

    if a.json:
        write_json(a.json, scripts)
    if a.tsv:
        write_tsv(a.tsv, layout_rows(scripts))
    if a.md:
        write_markdown(a.md, layout_rows(scripts))
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
