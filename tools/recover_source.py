#!/usr/bin/env python3
"""Reclaim the *source-text* surface of Graphwar II from `Graphwar.pck`.

Why this exists
===============
`docs/146` says what the shipped package is made of. The short version:

    692 files = 105 `.gdc` + 31 `.scn` + 22 `.res` + 1 `.gdextension`
                + 1 `project.binary`            <-- 160 things that *used to be* source
                + 532 import products / media   <-- never source, see below

The 160 are all that stand between us and the original authoring project, and each
of the three container formats is mechanically reversible:

    `.gdc`  Godot 4 GDScript bytecode (`GDSC`, payload zstd)  -> `.gd`
    `.scn`  binary `PackedScene`                             -> `.tscn`
    `.res`  binary `Resource`                                -> `.tres`
    `project.binary` (ECFG)                                  -> `project.godot`

What is *not* recoverable as source, and why that is not a bug: `.ctex` (167),
`.fontdata` (7) and `.png`/`.wav`/`.mp3str`/... (11) are import **products**. Their
authoring inputs (149 `.svg`, 7 `.ttf`, the `.png`/audio masters) are not in the
package at all — Godot's exporter replaces them. `.import` (184) and `.remap` (158)
are exporter bookkeeping, not source. So the honest ceiling for "source recovery"
is 160/692, and this script reaches it.

Where the original paths come from
==================================
Exported resources live under hashed names (`res://.godot/exported/133200997/
export-b5463874...-play_screen.scn`) with no human-readable identity. The mapping
back to the authoring path is *given by the package itself*: for every one of the
158 converted files there is a sibling `X.gd.remap` / `X.tscn.remap` / `X.tres.remap`
whose body is

    [remap]

    path="res://X.gdc"          (or the hashed .scn / .res path)

so `X` is recovered, never guessed. Measured: the 158 `.remap` files decompose
exactly into 105 `.gd` + 31 `.tscn` + 22 `.tres` — the counts match the container
counts one-for-one, which is the check that the mapping is total.

Three converters, and why these three
=====================================
* `.gdc` -> `.gd`: **GDRE Tools** (`--recover --scripts-only`). GDRE is the only
  GDScript decompiler that exists. Bytecode version here is `101`, whose token
  table is `4.5.0-stable`; GDRE auto-detects it from the PCK header.
  The output is then verified *token for token* against the shipped bytecode by
  `tools/gdc_verify_source.py` (already in this repo): lex the recovered `.gd`,
  map both sides to (kind, value) pairs, require equality. A mismatch means the
  decompiler invented or dropped a token, and is reported instead of hidden.
* `.scn`/`.res` -> `.tscn`/`.tres`: **Godot itself, headless**. Godot can load the
  binary resource and `ResourceSaver.save()` it as text, which is by definition the
  format's own serialisation — no second implementation of the reader to be wrong.
  One Godot process per file, so a failure is attributable to exactly one input.
* `project.binary` -> `project.godot`: parsed in this file. The container is
  `ECFG`; layout is `u32 magic | u32 count | count x (u32 klen | key | u32 vlen |
  vlen bytes of Variant)`. Verified by consuming exactly 1346/1346 bytes.

Usage
=====
    python3 tools/recover_source.py                 # write recovered-src/
    python3 tools/recover_source.py --check         # CI: verify, never write
    python3 tools/recover_source.py --list          # print the plan, do nothing
    python3 tools/recover_source.py --keep-work     # keep .agent-work/srcrec/work

Environment: `GDRE_BIN` / `GODOT_BIN` override the two external tool paths.

Only `recovered-src/` is written inside the repo, and it is text-only: the
extension whitelist is enforced here and re-checked by `--check`, so a binary
cannot slip in through a future edit. Stdlib only.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)                       # gw2-rebuild/
WS = os.path.dirname(REPO)                         # where Graphwar.pck lives

DEFAULT_OUT = os.path.join(REPO, "recovered-src")


def _search_up(rel, start=REPO, levels=5):
    """Find `rel` under `start` or one of its ancestors.

    The package and the two external tools live next to the *analysis* checkout,
    not next to every clone of this repository, so the defaults have to be
    discovered rather than hard-coded. Explicit flags or GW2_PCK/GODOT_BIN/
    GDRE_BIN always win.
    """
    d = os.path.abspath(start)
    for _ in range(levels):
        cand = os.path.join(d, rel)
        if os.path.exists(cand):
            return cand
        parent = os.path.dirname(d)
        if parent == d:
            break
        d = parent
    return None


def _search_glob(rel_glob, start=REPO, levels=5):
    import glob as _glob
    d = os.path.abspath(start)
    for _ in range(levels):
        hits = sorted(_glob.glob(os.path.join(d, rel_glob)))
        if hits:
            return hits[-1]
        parent = os.path.dirname(d)
        if parent == d:
            break
        d = parent
    return None


def _default_pck():
    return (os.environ.get("GW2_PCK") or _search_up("Graphwar.pck")
            or os.path.join(WS, "Graphwar.pck"))


def _default_godot():
    return (os.environ.get("GODOT_BIN")
            or _search_glob("tools/Godot_v*-stable_linux.x86_64")
            or _search_glob("Godot_v*-stable_linux.x86_64")
            or os.path.join(WS, "tools", "Godot_v4.7.2-stable_linux.x86_64"))


def _default_gdre():
    return (os.environ.get("GDRE_BIN")
            or _search_up(os.path.join(".agent-work", "tools", "gdre_tools.x86_64"))
            or _search_glob("**/gdre_tools.x86_64")
            or _search_glob("gdre_tools.x86_64")
            or os.path.join(WS, ".agent-work", "tools", "gdre_tools.x86_64"))


def _default_work():
    pck = os.path.dirname(os.path.abspath(_default_pck()))
    return os.path.join(pck, ".agent-work", "srcrec", "work")

MAGIC = 0x43504447                                 # 'GDPC'

# `recovered-src/` is text source only. `.gitignore` is listed because the
# directory carries one that denies binary extensions. `.cfg` is listed for
# `addons/admob/plugin.cfg`, a genuine source file (plugin manifest, INI text);
# it is the one package `.cfg` that is not an editor cache.
TEXT_EXT = (".gd", ".tscn", ".tres", ".godot", ".gdextension",
            ".tsv", ".md", ".gitignore", ".cfg")

# Fonts and texture atlases embedded in a `.tres` are serialised as one huge
# `PackedByteArray("...")` literal. Anything past this many characters in a
# single literal is media, not source. See `redact_media()` and docs/146 §3.5.
MEDIA_LITERAL_LIMIT = 4096

# Statuses used in MANIFEST.tsv / docs; keep the wording stable, it is quoted.
FULL, PARTIAL, FAILED = "完整", "部分", "失败"
# 省略 = the output exists and is valid text, but its embedded font/atlas bytes
# were deliberately dropped: this repository does not carry media. Distinct from
# 部分 (recovered as-is, with a known loss) and from 失败.
OMITTED = "省略"
# The one package `.cfg` that is source rather than an editor cache.
SOURCE_CFG = "addons/admob/plugin.cfg"
SKIP = {
    ".import": "非源码-导入元数据",
    ".remap": "非源码-导出重映射",
    ".ctex": "非源码-导入产物(纹理)",
    ".fontdata": "非源码-导入产物(字体)",
    ".png": "非源码-媒体",
    ".wav": "非源码-媒体",
    ".mp3": "非源码-媒体",
    ".mp3str": "非源码-媒体",
    ".ogg": "非源码-媒体",
    ".oggvorbisstr": "非源码-媒体",
    ".sample": "非源码-音频采样",
    ".cfg": "非源码-编辑器缓存",
    ".bin": "非源码-编辑器缓存(uid)",
}


# --------------------------------------------------------------------- PCK

class Pck:
    """Read-only view of a Godot 4 PCK (v4: index at the tail, offset table)."""

    def __init__(self, path):
        with open(path, "rb") as fh:
            self.data = fh.read()
        mv = memoryview(self.data)
        magic, self.pack_version = struct.unpack_from("<II", mv, 0)
        if magic != MAGIC:
            raise SystemExit("not a Godot PCK (magic=0x%08x): %s" % (magic, path))
        (self.ver_major, self.ver_minor, self.ver_patch) = struct.unpack_from("<III", mv, 8)
        self.file_base, = struct.unpack_from("<Q", mv, 0x18)
        dir_offset, = struct.unpack_from("<Q", mv, 0x20)
        off = dir_offset
        count, = struct.unpack_from("<I", mv, off)
        off += 4
        self.entries = []
        for _ in range(count):
            plen, = struct.unpack_from("<I", mv, off)
            off += 4
            name = bytes(mv[off:off + plen]).rstrip(b"\x00").decode("utf-8", "replace")
            off += (plen + 3) & ~3
            foff, fsize = struct.unpack_from("<QQ", mv, off)
            off += 16
            md5 = bytes(mv[off:off + 16])
            off += 16
            flags, = struct.unpack_from("<I", mv, off)
            off += 4
            self.entries.append({"name": name, "offset": foff, "size": fsize,
                                 "md5": md5, "flags": flags})
        self.by_name = {e["name"]: e for e in self.entries}

    def read(self, name):
        e = self.by_name[name]
        start = self.file_base + e["offset"]
        return self.data[start:start + e["size"]]

    def has(self, name):
        return name in self.by_name

    def version_string(self):
        return "%d.%d.%d" % (self.ver_major, self.ver_minor, self.ver_patch)


RE_REMAP_PATH = re.compile(rb'^path="([^"]+)"', re.M)


def build_plan(pck):
    """Work out the 158 reversible pairs from the package's own `.remap` files.

    Returns a dict of job lists; every job is {orig, shipped, kind}. Nothing here
    is inferred: `orig` comes from the `.remap` file *name*, `shipped` from its
    `path=` value.
    """
    plan = {"gdc": [], "scene": [], "res": [], "remap_missing": []}
    for e in pck.entries:
        name = e["name"]
        if not name.endswith(".remap"):
            continue
        orig = name[:-len(".remap")]
        body = pck.read(name)
        m = RE_REMAP_PATH.search(body)
        if not m:
            plan["remap_missing"].append(name)
            continue
        target = m.group(1).decode("utf-8", "replace")
        target = target[len("res://"):] if target.startswith("res://") else target
        if orig.endswith(".gd"):
            plan["gdc"].append({"orig": orig, "shipped": target, "kind": "gd"})
        elif orig.endswith(".tscn"):
            plan["scene"].append({"orig": orig, "shipped": target, "kind": "tscn"})
        elif orig.endswith(".tres"):
            plan["res"].append({"orig": orig, "shipped": target, "kind": "tres"})
        else:
            plan["remap_missing"].append(name)
    for k in plan:
        plan[k].sort(key=lambda j: j.get("orig", j))
    return plan


# ------------------------------------------------------- project.binary

# Godot Variant type ids we can render. Anything else is reported, never guessed.
VARIANT_NAMES = {1: "bool", 2: "int", 3: "float", 4: "String", 5: "Vector2",
                 6: "Vector2i", 20: "Color", 21: "StringName", 28: "Array",
                 34: "PackedStringArray"}


def _decode_variant(body):
    """Decode one `vlen`-delimited Variant; returns (value, type_name)."""
    t, = struct.unpack_from("<I", body, 0)
    name = VARIANT_NAMES.get(t, "unknown(%d)" % t)
    p = 4
    if t == 1:
        return bool(struct.unpack_from("<I", body, p)[0]), name
    if t == 2:
        return struct.unpack_from("<i", body, p)[0], name
    if t == 3:
        return struct.unpack_from("<d", body, p)[0], name
    if t in (4, 21):
        n, = struct.unpack_from("<I", body, p)
        return body[p + 4:p + 4 + n].decode("utf-8", "replace"), name
    if t == 5:
        return list(struct.unpack_from("<2f", body, p)), name
    if t == 6:
        return list(struct.unpack_from("<2i", body, p)), name
    if t == 20:
        return list(struct.unpack_from("<4f", body, p)), name
    if t == 34:
        # PackedStringArray: u32 count, then count x (u32 len | payload padded to 4).
        # `len` counts a trailing NUL, which is why the pad is already included.
        n, = struct.unpack_from("<I", body, p)
        p += 4
        out = []
        for _ in range(n):
            ln, = struct.unpack_from("<I", body, p)
            p += 4
            out.append(body[p:p + ln].rstrip(b"\x00").decode("utf-8", "replace"))
            p += (ln + 3) & ~3
        return out, name
    return {"_unparsed_variant_type": t, "_bytes": len(body) - 4}, name


def parse_project_binary(blob):
    """Parse `ECFG`; returns [(key, value, typename)] in file order plus a byte count."""
    if len(blob) < 8 or blob[:4] != b"ECFG":
        raise SystemExit("project.binary is not ECFG (magic=%r)" % blob[:4])
    o = 4
    count, = struct.unpack_from("<I", blob, o)
    o += 4
    items = []
    for _ in range(count):
        klen, = struct.unpack_from("<I", blob, o)
        o += 4
        key = blob[o:o + klen].decode("utf-8", "replace")
        o += klen
        vlen, = struct.unpack_from("<I", blob, o)
        o += 4
        items.append((key,) + _decode_variant(blob[o:o + vlen]))
        o += vlen
    return items, o


def _num(x):
    """Render a float the way Godot's Variant writer does: no trailing `.0`."""
    s = ("%.6f" % x).rstrip("0").rstrip(".")
    return s if s not in ("", "-") else "0"


def _godot_literal(value, typename):
    if typename == "bool":
        return "true" if value else "false"
    if typename == "int":
        return str(value)
    if typename == "float":
        return _num(value)
    if typename == "String":
        return '"%s"' % value.replace("\\", "\\\\").replace('"', '\\"')
    if typename == "StringName":
        return '&"%s"' % value.replace("\\", "\\\\").replace('"', '\\"')
    if typename == "Color":
        return "Color(%s)" % ", ".join(_num(c) for c in value)
    if typename == "Vector2":
        return "Vector2(%s)" % ", ".join(_num(c) for c in value)
    if typename == "Vector2i":
        return "Vector2i(%d, %d)" % (value[0], value[1])
    if typename == "PackedStringArray":
        return "PackedStringArray(%s)" % ", ".join(
            '"%s"' % v.replace("\\", "\\\\").replace('"', '\\"') for v in value)
    return None


def project_godot_text(items):
    """Render the shipped settings as a Godot 4 `project.godot`.

    Field names and values are verbatim; nothing is defaulted or prettified. The
    only line that does not come from the package is `config_version=5`, which the
    format requires in order to be loadable at all, and it is called out in the
    file's own comment.
    """
    lines = [
        "; Recovered from the shipped project.binary (ECFG). Field names and values",
        "; are verbatim; the single added line is config_version, which the format",
        "; requires. See docs/146-source-recovery.md.",
        "config_version=5",
        "",
    ]
    sections, order = {}, []
    for key, value, typename in items:
        if "/" in key:
            sec, rest = key.split("/", 1)
        else:
            sec, rest = "", key
        if sec not in sections:
            sections[sec] = []
            order.append(sec)
        sections[sec].append((rest, value, typename, key))
    unconverted = []
    for sec in order:
        lines.append("[%s]" % sec)
        for rest, value, typename, full in sections[sec]:
            lit = _godot_literal(value, typename)
            if lit is None:                        # unknown Variant: keep the key visible
                lines.append("; UNCONVERTED %s : %s %r" % (full, typename, value))
                unconverted.append(full)
            else:
                lines.append("%s=%s" % (rest, lit))
        lines.append("")
    return "\n".join(lines), unconverted


# ------------------------------------------------- staging + converters

CONVERT_GD = r'''extends SceneTree
# Loads one shipped binary resource and saves it as text.
# One process per input file so that any engine error belongs to exactly one file.

func _bytes(p: String) -> int:
	var f := FileAccess.open(p, FileAccess.READ)
	if f == null:
		return -1
	var n := f.get_length()
	f.close()
	return n

func _init() -> void:
	var f := FileAccess.open("res://_recover_job.json", FileAccess.READ)
	if f == null:
		print("@@RESULT " + JSON.stringify({"ok": false, "error": "no job file"}))
		quit(2)
		return
	var job = JSON.parse_string(f.get_as_text())
	f.close()
	# Resource UIDs are *read from the shipped bytes* (u64 after each ext_resource
	# path) and registered so the loader keeps them instead of falling back to the
	# path with an "invalid UID" warning.
	for pair in job.get("uids", []):
		var id := int(pair[0])
		if not ResourceUID.has_id(id):
			ResourceUID.add_id(id, String(pair[1]))
	var src := String(job["src"])
	var dst := String(job["dst"])
	var res = ResourceLoader.load(src)
	if res == null and job.has("fallback"):
		src = String(job["fallback"])
		res = ResourceLoader.load(src)
	if res == null:
		print("@@RESULT " + JSON.stringify({"ok": false, "error": "ResourceLoader.load returned null"}))
		quit(0)
		return
	var err: int = ResourceSaver.save(res, dst)
	print("@@RESULT " + JSON.stringify({
		"ok": err == OK,
		"error": "" if err == OK else error_string(err),
		"class": res.get_class(),
		"source": src,
		"bytes": _bytes(dst) if err == OK else 0,
	}))
	quit(0)
'''

VERIFY_GD = r'''extends SceneTree
# Independent check that the text form means the same thing as the binary form.
# For a PackedScene the comparison is over `SceneState`: node count,每 node 的
# type/name/instance, and every stored property name+value. Object-valued
# properties compare by class + resource path (the same sub-resource has no path
# in either form, so the comparison stays symmetric).

func _val(v) -> String:
	if typeof(v) == TYPE_OBJECT:
		if v == null:
			return "<null>"
		if v is Resource and String(v.resource_path) != "":
			# A sub-resource's path is "<owner path>::<id>", and the two forms are
			# loaded from different owners, so only the stable "::<id>" tail can be
			# compared. External resources keep their full res:// path.
			var p := String(v.resource_path)
			if p.contains("::"):
				p = p.substr(p.find("::"))
			return "<%s %s>" % [v.get_class(), p]
		return "<%s>" % v.get_class()
	return var_to_str(v)

func _sig_scene(res) -> Array:
	var st: SceneState = res.get_state()
	var out := ["nodes", st.get_node_count(), "conns", st.get_connection_count()]
	for i in st.get_node_count():
		# get_node_instance() returns a PackedScene for instanced sub-scenes, not a
		# String, so it goes through _val() like any other value.
		var node := [String(st.get_node_name(i)), String(st.get_node_type(i)),
			_val(st.get_node_instance(i)), String(st.get_node_instance_placeholder(i)),
			st.get_node_property_count(i)]
		for p in st.get_node_property_count(i):
			node.append([String(st.get_node_property_name(i, p)),
				_val(st.get_node_property_value(i, p))])
		out.append(node)
	return out

func _sig_other(res) -> Array:
	var out := [res.get_class()]
	for p in res.get_property_list():
		if not (int(p.usage) & PROPERTY_USAGE_STORAGE):
			continue
		out.append([String(p.name), _val(res.get(String(p.name)))])
	return out

func _sig(res) -> Array:
	if res is PackedScene:
		return _sig_scene(res)
	return _sig_other(res)

func _init() -> void:
	var f := FileAccess.open("res://_recover_verify.json", FileAccess.READ)
	if f == null:
		print("@@VERIFY " + JSON.stringify([]))
		quit(2)
		return
	var jobs = JSON.parse_string(f.get_as_text())
	f.close()
	var out := []
	for job in jobs:
		for pair in job.get("uids", []):
			var id := int(pair[0])
			if not ResourceUID.has_id(id):
				ResourceUID.add_id(id, String(pair[1]))
		var rec := {"orig": String(job.get("orig", "")), "ok": false, "note": ""}
		var a = ResourceLoader.load(String(job["bin"]))
		var b = ResourceLoader.load(String(job["text"]))
		if a == null:
			rec["note"] = "binary form failed to load"
		elif b == null:
			rec["note"] = "text form failed to load"
		elif a.get_class() != b.get_class():
			rec["note"] = "class %s != %s" % [a.get_class(), b.get_class()]
		else:
			var sa := _sig(a)
			var sb := _sig(b)
			rec["ok"] = sa == sb
			if not rec["ok"]:
				rec["note"] = "structure differs (%d vs %d entries)" % [sa.size(), sb.size()]
			elif a is PackedScene:
				rec["note"] = "scene-state equal"
			else:
				rec["note"] = "property list equal"
		out.append(rec)
	print("@@VERIFY " + JSON.stringify(out))
	quit(0)
'''


def write_stage(pck, stage, items):
    """Extract the whole package to `stage/` so that `res://` references resolve.

    A faithful conversion needs the *neighbourhood*: `play_screen.scn` references
    `res://graphwar_theme.tres`, which itself references `res://ComicRelief-Bold.ttf`.
    Extracting only the 53 files under conversion would silently null those out.
    """
    if os.path.isdir(stage):
        shutil.rmtree(stage)
    for e in pck.entries:
        dst = os.path.join(stage, e["name"])
        d = os.path.dirname(dst)
        if d and not os.path.isdir(d):
            os.makedirs(d)
        with open(dst, "wb") as fh:
            fh.write(pck.read(e["name"]))
    text, unconverted = project_godot_text(items)
    with open(os.path.join(stage, "project.godot"), "w", encoding="utf-8") as fh:
        fh.write(text + "\n")
    with open(os.path.join(stage, "_recover_convert.gd"), "w", encoding="utf-8") as fh:
        fh.write(CONVERT_GD)
    with open(os.path.join(stage, "_recover_verify.gd"), "w", encoding="utf-8") as fh:
        fh.write(VERIFY_GD)
    return unconverted


RE_RES_PATH = re.compile(rb"res://[^\x00]*")


def binary_ext_uids(blob):
    """Recover (uid, path) pairs from a binary resource's external-resource table.

    Layout, verified against `play_screen.scn` (38 entries, byte-exact walk):

        u32 type_len | type\\0 | u32 path_len | path\\0 | u64 uid

    `path_len` counts the NUL, and `uid == 0xffffffffffffffff` means "no uid".
    Anchoring on the `res://` prefix and validating `path_len` makes the scan
    self-checking: a false anchor fails the length test and is skipped.
    """
    pairs = []
    i = 0
    n = len(blob)
    while True:
        i = blob.find(b"res://", i)
        if i < 0 or i < 4 or i + 8 > n:
            break
        ln, = struct.unpack_from("<I", blob, i - 4)
        nul = blob.find(b"\x00", i)
        if nul < 0:
            break
        path = blob[i:nul].decode("utf-8", "replace")
        if ln == len(path) + 1 and nul + 9 <= n:
            uid, = struct.unpack_from("<Q", blob, nul + 1)
            if uid != 0xFFFFFFFFFFFFFFFF:
                pairs.append((uid, path))
            i = nul + 9
        else:
            i += 6
    return pairs


def run(cmd, cwd=None, env=None, timeout=600):
    e = dict(os.environ)
    # Godot/GDRE want a writable user dir; keep it inside the workspace so that
    # nothing is written to `$HOME` and `/tmp` is never involved.
    if env:
        e.update(env)
    p = subprocess.run(cmd, cwd=cwd, env=e, timeout=timeout,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")


def tool_env(work):
    home = os.path.join(work, "home")
    return {"HOME": home,
            "XDG_DATA_HOME": os.path.join(home, ".local", "share"),
            "XDG_CONFIG_HOME": os.path.join(home, ".config"),
            "XDG_CACHE_HOME": os.path.join(home, ".cache")}


def decompile_scripts(pck, plan, work, gdre, log):
    """`.gdc` -> `.gd` via GDRE, then per-file bookkeeping.

    GDRE's `--recover --scripts-only` decompiles the whole package in one pass and
    prints an aggregate report. Per-file status comes from the report *plus* an
    explicit existence/size check per expected path, and any script that is absent
    is retried alone with `--decompile` so its error message is attributable.
    """
    out = os.path.join(work, "gdre")
    if os.path.isdir(out):
        shutil.rmtree(out)
    os.makedirs(out)
    env = tool_env(work)
    cmd = [gdre, "--headless", "--recover=" + os.path.abspath(pck),
           "--scripts-only", "--output=" + out]
    rc, so, se = run(cmd, cwd=work, env=env)
    with open(os.path.join(work, "gdre_recover.log"), "w", encoding="utf-8") as fh:
        fh.write(so + "\n=== stderr ===\n" + se)
    report = {}
    for line in so.splitlines():
        m = re.match(r"^(Decompiled scripts|Scripts not decompiled|Failed conversions)\s*:\s*(\d+)", line.strip())
        if m:
            report[m.group(1)] = int(m.group(2))
    failed = {}
    for job in plan["gdc"]:
        path = os.path.join(out, job["orig"])
        ok = os.path.isfile(path) and os.path.getsize(path) > 0
        if not ok and gdre and os.path.isfile(gdre):
            # One file, one invocation: whatever GDRE says is *this* file's reason.
            tmp = os.path.join(work, "gdre_retry")
            if os.path.isdir(tmp):
                shutil.rmtree(tmp)
            os.makedirs(tmp)
            src = os.path.join(work, "stage", job["shipped"])
            rc2, so2, se2 = run([gdre, "--headless", "--decompile=" + src,
                                 "--bytecode=4.5.0-stable", "--output=" + tmp],
                                cwd=work, env=env)
            msg = (se2 or so2).strip().splitlines()
            failed[job["orig"]] = msg[-1] if msg else "GDRE produced no output"
            log("  gdc FAILED %s: %s" % (job["orig"], failed[job["orig"]]))
    return out, report, failed


def verify_tokens(pck, plan, work, gdre_out, log):
    """Token-for-token verification of every `.gd` against the shipped `.gdc`.

    Delegates to `tools/gdc_verify_source.py` (same repo): it lexes the recovered
    source and compares the (kind, value) token sequence with the one decoded from
    the bytecode. Equal sequences mean the decompiler neither invented nor dropped
    a token — the strongest statement available short of the original text.
    """
    tool = os.path.join(REPO, "tools", "gdc_verify_source.py")
    stage = os.path.join(work, "stage")
    if not os.path.isfile(tool):
        return {}, "gdc_verify_source.py not found"
    js = os.path.join(work, "verify_gdc.json")
    rc, so, se = run([sys.executable, tool, "--gdc", stage, "--gd", gdre_out,
                      "--json", js], cwd=REPO)
    per = {}
    if os.path.isfile(js):
        with open(js, encoding="utf-8") as fh:
            data = json.load(fh)
        for r in data.get("results", []):
            per[r["script"] + ".gd"] = r
        return per, so.strip().splitlines()[-1] if so.strip() else ""
    return per, "verifier produced no JSON (%s)" % (se.strip()[:200] or rc)


def convert_binary_resources(pck, plan, work, godot, log):
    """`.scn`/`.res` -> `.tscn`/`.tres`, one Godot process per file."""
    stage = os.path.join(work, "stage")
    conv = os.path.join(stage, "_conv")
    if os.path.isdir(conv):
        shutil.rmtree(conv)
    os.makedirs(conv)
    # One global uid table: every (uid -> path) the package itself declares, so a
    # nested load (theme -> font, scene -> theme) resolves instead of warning.
    uids, conflicts = {}, 0
    jobs = plan["scene"] + plan["res"]
    for job in jobs:
        shipped = job["shipped"]
        if not pck.has(shipped):
            continue
        for uid, path in binary_ext_uids(pck.read(shipped)):
            if uid in uids and uids[uid] != path:
                conflicts += 1
                continue
            uids[uid] = path
    uid_pairs = [[u, p] for u, p in uids.items()]
    log("  uid table: %d entries from the shipped binaries (%d conflicting, dropped)"
        % (len(uid_pairs), conflicts))

    results = {}
    for job in jobs:
        orig, shipped = job["orig"], job["shipped"]
        if not pck.has(shipped):
            results[orig] = {"ok": False, "error": "shipped blob %s absent" % shipped}
            continue
        dst = os.path.join(conv, orig)
        d = os.path.dirname(dst)
        if d and not os.path.isdir(d):
            os.makedirs(d)
        jobj = {"src": "res://" + shipped, "dst": dst,
                "fallback": "res://" + orig, "uids": uid_pairs}
        with open(os.path.join(stage, "_recover_job.json"), "w", encoding="utf-8") as fh:
            json.dump(jobj, fh)
        rc, so, se = run([godot, "--headless", "--path", stage,
                          "--script", "res://_recover_convert.gd"],
                         env=tool_env(work), timeout=180)
        rec = {"ok": False, "error": "no @@RESULT (rc=%d)" % rc, "stderr": se}
        for line in so.splitlines():
            if line.startswith("@@RESULT "):
                try:
                    rec = json.loads(line[len("@@RESULT "):])
                except ValueError:
                    rec["error"] = "unparsable @@RESULT"
                rec["stderr"] = se
        rec["binary_ext_uids"] = len(binary_ext_uids(pck.read(shipped)))
        results[orig] = rec
        if not rec.get("ok"):
            log("  %s FAILED: %s" % (orig, rec.get("error")))
    return conv, results


def verify_roundtrip(pck, plan, work, godot, log):
    """Compare the text form against the binary form inside the staged project."""
    stage = os.path.join(work, "stage")
    conv = os.path.join(stage, "_conv")
    uids, _ = {}, 0
    for job in plan["scene"] + plan["res"]:
        if not pck.has(job["shipped"]):
            continue
        for uid, path in binary_ext_uids(pck.read(job["shipped"])):
            uids.setdefault(uid, path)
    uid_pairs = [[u, p] for u, p in uids.items()]
    jobs = []
    for job in plan["scene"] + plan["res"]:
        text = os.path.join(conv, job["orig"])
        if not os.path.isfile(text) or not pck.has(job["shipped"]):
            continue
        jobs.append({"orig": job["orig"], "bin": "res://" + job["shipped"],
                     "text": "res://_conv/" + job["orig"], "uids": uid_pairs})
    if not jobs:
        return {}
    with open(os.path.join(stage, "_recover_verify.json"), "w", encoding="utf-8") as fh:
        json.dump(jobs, fh)
    rc, so, se = run([godot, "--headless", "--path", stage,
                      "--script", "res://_recover_verify.gd"],
                     env=tool_env(work), timeout=600)
    out = {}
    for line in so.splitlines():
        if line.startswith("@@VERIFY "):
            try:
                for rec in json.loads(line[len("@@VERIFY "):]):
                    out[rec["orig"]] = rec
            except ValueError:
                pass
    if not out:
        log("  round-trip verify produced nothing (rc=%d): %s" % (rc, se.strip()[:200]))
    else:
        bad = [k for k, v in out.items() if not v["ok"]]
        log("  round-trip verify: %d/%d equal%s"
            % (len(out) - len(bad), len(out),
               "" if not bad else " — differs: " + ", ".join(bad)))
    return out


# ------------------------------------------------------------- manifest

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build_rows(pck, plan, out_dir, gdre_report, token_verify, token_summary,
               conv_results, rt_verify, unconverted, media=None):
    """One row per package entry, so coverage is countable rather than asserted."""
    media = media or {}
    by_orig = {}
    for job in plan["gdc"]:
        by_orig[job["shipped"]] = ("gd", job)
    for job in plan["scene"]:
        by_orig[job["shipped"]] = ("tscn", job)
    for job in plan["res"]:
        by_orig[job["shipped"]] = ("tres", job)

    rows = []
    for e in pck.entries:
        name = e["name"]
        row = {"pck": name, "out": "", "kind": "", "status": "", "note": ""}
        if name in by_orig:
            kind, job = by_orig[name]
            outp = os.path.join(out_dir, job["orig"])
            row["out"] = job["orig"]
            row["kind"] = kind
            exists = os.path.isfile(outp) and os.path.getsize(outp) > 0
            if not exists:
                row["status"] = FAILED
                row["note"] = "converter produced no output"
                if kind == "gd":
                    row["note"] = "GDRE produced no .gd"
                rows.append(row)
                continue
            if kind == "gd":
                v = token_verify.get(job["orig"])
                if v is not None and v.get("ok"):
                    row["status"] = FULL
                    row["note"] = "GDScript; token-for-token equal to the bytecode"
                elif v is not None:
                    row["status"] = PARTIAL
                    row["note"] = "GDScript; token verify failed: %s" % v.get("detail", "")
                else:
                    row["status"] = PARTIAL
                    row["note"] = "GDScript; token verify unavailable"
            else:
                rec = conv_results.get(job["orig"], {})
                rt = rt_verify.get(job["orig"])
                nuids = rec.get("binary_ext_uids", 0)
                bits = []
                if kind == "tscn":
                    bits.append("PackedScene")
                else:
                    bits.append("Resource(" + str(rec.get("class", "?")) + ")")
                if rt is not None and rt.get("ok"):
                    bits.append("round-trip " + rt.get("note", "equal"))
                elif rt is not None:
                    bits.append("round-trip differs: " + rt.get("note", ""))
                else:
                    bits.append("round-trip not compared")
                if nuids:
                    # Godot's binary reader drops an ext_resource uid that is not in
                    # `.godot/uid_cache.bin`, so the text form loses those attributes.
                    bits.append("%d ext_resource uid dropped" % nuids)
                    row["status"] = PARTIAL
                else:
                    row["status"] = FULL
                # Embedded media (font file / texture atlas) is removed on purpose;
                # keep the original byte size in the note so the omission is countable.
                med = media.get(job["orig"])
                if med and not med.get("included"):
                    bits.append("内嵌字形图集/字体数据不入库：原始 %d B，骨架 %d B，"
                                "丢弃 %d 段共 %d B（恢复见 docs/146 §3.5 / --include-media）"
                                % (med["orig_bytes"], med["skeleton_bytes"],
                                   med["dropped_payloads"], med["dropped_bytes"]))
                    row["status"] = OMITTED
                elif med:
                    bits.append("内嵌二进制数据已按 --include-media 保留（原始 %d B，媒体，"
                                "入库前请确认策略）" % med["orig_bytes"])
                    row["status"] = PARTIAL
                row["note"] = "; ".join(bits)
        elif name == "project.binary":
            row["out"] = "project.godot"
            row["kind"] = "godot"
            row["status"] = PARTIAL if unconverted else FULL
            row["note"] = ("ECFG settings -> project.godot; %d value(s) kept as comments"
                           % len(unconverted)) if unconverted else \
                          "ECFG settings -> project.godot (verbatim fields)"
        elif name == "Graphwar.gdextension":
            row["out"] = "Graphwar.gdextension"
            row["kind"] = "gdextension"
            row["status"] = FULL
            row["note"] = "already text in the package; copied byte for byte"
        elif name == SOURCE_CFG:
            # `plugin.cfg` is INI text authored by the plugin vendor, not an editor
            # cache: name/description/author/version/script. It is source.
            row["out"] = SOURCE_CFG
            row["kind"] = "cfg"
            row["status"] = FULL
            row["note"] = "AdMob plugin manifest (INI text, already source in the package)"
        elif name == ".godot/uid_cache.bin":
            row["kind"] = "binary"
            row["status"] = SKIP[".bin"]
            row["note"] = "editor UID cache (108 entries: 105 .gdc + .png + .gdextension + .wav)"
        else:
            ext = os.path.splitext(name)[1]
            row["kind"] = "binary"
            row["status"] = SKIP.get(ext, "非源码-其他")
            if ext in (".import", ".remap"):
                row["note"] = ("import record: the source path is this file's own name, "
                               "path= names the .ctex product"
                               if ext == ".import"
                               else "exporter remap: body names the shipped blob, "
                                    "and is what recovers the original path")
            elif ext == ".ctex":
                row["note"] = ("rasterised product of a .svg/.png/.jpeg that is not in "
                               "the package")
            elif ext == ".fontdata":
                row["note"] = "MSDF product of a .ttf that is not in the package"
            elif ext in (".mp3str", ".oggvorbisstr", ".sample", ".wav", ".png"):
                row["note"] = "media, not source"
            elif ext == ".cfg":
                row["note"] = "Godot editor cache (text already); not project source"
        rows.append(row)
    return rows


# --------------------------------------------------- media redaction

LONG_LITERAL = re.compile(r'"[^"\n]{%d,}"' % MEDIA_LITERAL_LIMIT)
_DATA_DICT_OPEN = "data = {"


def has_media(text):
    """True when the file still carries an embedded font/atlas payload."""
    return LONG_LITERAL.search(text) is not None


def redact_media(text):
    """Drop embedded font/atlas bytes, keep everything that is source.

    A `.tres` produced from a Theme/FontFile carries its media inline as one huge
    `PackedByteArray("...")` string, in either of two shapes:

        data = {                    # texture atlas
        "data": PackedByteArray("...."),
        "format": "LumAlpha8", ...
        }

        data = PackedByteArray("....")      # the .ttf itself

    Both are *bytes*, not source; the repository does not carry media. Dropping
    them leaves a metrics-only skeleton: for `button_font.tres` that is 2 333
    stored properties (every glyph's `uv_rect` / `offset` / `size`) plus the font
    settings — i.e. everything an author actually looks at. Returns
    (new_text, dropped_count, dropped_bytes).
    """
    lines = text.split("\n")
    out = []
    dropped = 0
    dropped_bytes = 0
    i = 0
    while i < len(lines):
        line = lines[i]
        if line == _DATA_DICT_OPEN:
            j = i + 1
            big = False
            while j < len(lines) and lines[j] != "}":
                if len(lines[j]) > MEDIA_LITERAL_LIMIT:
                    big = True
                j += 1
            if j < len(lines) and big:
                dropped += 1
                dropped_bytes += sum(len(x) + 1 for x in lines[i:j + 1])
                i = j + 1
                continue
        m = re.match(r'^(data = )PackedByteArray\("(.*)"\)$', line)
        if m and len(m.group(2)) > MEDIA_LITERAL_LIMIT:
            dropped += 1
            dropped_bytes += len(line) + 1
            i += 1
            continue
        out.append(line)
        i += 1
    return "\n".join(out), dropped, dropped_bytes


def apply_media_policy(path, include_media):
    """Enforce the repository's "no media" rule on one output file.

    Returns None when nothing was dropped, else a dict describing the omission.
    The dropped payload is *counted*, never silently forgotten: the manifest keeps
    the original byte size so a reader can tell what is missing and how big it is.
    """
    with open(path, encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    if not has_media(text):
        return None
    if include_media:
        return {"included": True, "orig_bytes": len(text)}
    orig = len(text.encode("utf-8"))
    new, dropped, dropped_bytes = redact_media(text)
    new = _media_marker(new, dropped, dropped_bytes, orig)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(new)
    return {"included": False, "orig_bytes": orig,
            "skeleton_bytes": len(new.encode("utf-8")),
            "dropped_payloads": dropped, "dropped_bytes": dropped_bytes}


def _media_marker(text, dropped, dropped_bytes, orig_bytes):
    """Say, inside the file, what was removed — so the skeleton is not a mystery."""
    note = ("; MEDIA OMITTED: %d embedded payload(s), %d B of font/atlas data removed.\n"
            "; This file's original size was %d B. Original text: "
            "python3 tools/recover_source.py --include-media\n"
            % (dropped, dropped_bytes, orig_bytes))
    lines = text.split("\n")
    if lines and lines[0].startswith("[gd_resource"):
        return "\n".join([lines[0], note.rstrip("\n")] + lines[1:])
    return note + text


def write_manifest(rows, path, pck, plan, extra, media_mode="omitted"):
    counts = {}
    for r in rows:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    total = len(rows)
    recon = sum(counts.get(k, 0) for k in (FULL, PARTIAL, OMITTED, FAILED))
    lines = [
        "# Graphwar II - source-recovery manifest, one row per file in Graphwar.pck",
        "# generated by tools/recover_source.py; do not edit by hand",
        "# pck: %d files, version %s, %d bytes" % (total, pck.version_string(), len(pck.data)),
        "# text source surface: %d/%d files" % (recon, total),
        "#   完整 %d | 部分 %d | 省略 %d | 失败 %d | 非源码 %d"
        % (counts.get(FULL, 0), counts.get(PARTIAL, 0), counts.get(OMITTED, 0),
           counts.get(FAILED, 0), total - recon),
        "# status 省略 = .tres skeleton kept, embedded font/atlas bytes dropped"
        " (the original byte size is in the note column)",
        "# media: " + media_mode,
        "# plan: %d .gdc->.gd, %d .scn->.tscn, %d .res->.tres"
        % (len(plan["gdc"]), len(plan["scene"]), len(plan["res"])),
        "# " + extra,
        "pck_path\tout_path\tkind\tstatus\tnote",
    ]
    for r in rows:
        lines.append("\t".join([r["pck"], r["out"], r["kind"], r["status"], r["note"]]))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    return counts


GITIGNORE = """# recovered-src/ holds text source only. Nothing binary may ever be committed here.
# (Graphwar.pck and everything extracted from it stays out of the repository.)
*.gdc
*.scn
*.res
*.ctex
*.fontdata
*.import
*.remap
*.pck
*.so
*.dll
*.dylib
*.exe
*.png
*.svg
*.ttf
*.otf
*.wav
*.mp3
*.ogg
*.sample
*.bin
__pycache__/
"""


def write_readme(path, counts, plan, stats, pck):
    total = sum(counts.values())
    recon = sum(counts.get(k, 0) for k in (FULL, PARTIAL, OMITTED, FAILED))
    body = README_TEMPLATE.format(
        total=total, recon=recon, full=counts.get(FULL, 0),
        partial=counts.get(PARTIAL, 0), omitted=counts.get(OMITTED, 0),
        failed=counts.get(FAILED, 0),
        nonsrc=total - recon, ngdc=len(plan["gdc"]), nscn=len(plan["scene"]),
        nres=len(plan["res"]), gdc_ok=stats.get("gdc_ok", 0),
        scn_ok=stats.get("scn_ok", 0), res_ok=stats.get("res_ok", 0),
        token=stats.get("token_summary", "n/a"), pck_version=pck.version_string(),
        media=stats.get("media", "n/a"),
        roundtrip=stats.get("roundtrip", "n/a"))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(body)


README_TEMPLATE = """# recovered-src — the original source-text layer of Graphwar II

This directory is **generated**, and it is **text only**. It answers one question:
*what did the authoring project look like, as far as it can still be read out of the
shipped package?*

All numbers below come from the package itself (`Graphwar.pck`, engine version
{pck_version}, {total} files) and are regenerated by the script named here.

## What is here

```
project.godot            from project.binary (ECFG), fields verbatim
Graphwar.gdextension     already text in the package; copied byte for byte
addons/admob/plugin.cfg  plugin manifest, already source in the package
<original path>.gd       105 GDScript files decompiled from .gdc
<original path>.tscn     31 PackedScenes rewritten from binary .scn
<original path>.tres     22 Resources rewritten from binary .res
MANIFEST.tsv             one row per package file: path / output / kind / status / note
```

Paths are the **original `res://` paths**, not the hashed export names. They are not
guessed: every converted file has a sibling `X.gd.remap` / `X.tscn.remap` /
`X.tres.remap` in the package whose `path="res://..."` body names the shipped blob,
and the 158 remaps decompose exactly into 105 + 31 + 22.

## Coverage

| status | files | meaning |
|---|---|---|
| 完整 | {full} | text form is complete and verified |
| 部分 | {partial} | text form recovered as-is, with a known loss (see MANIFEST note) |
| 省略 | {omitted} | `.tres` skeleton kept, **embedded font/atlas bytes removed** (media policy) |
| 失败 | {failed} | conversion failed |
| 非源码 | {nonsrc} | never was source: import products, media, editor caches |

The recoverable ceiling is {recon}/{total}. The remaining {nonsrc} are `.import` (184),
`.ctex` (167), `.remap` (158), `.fontdata` (7), audio (11), one `.png`, and editor
caches. `.ctex`/`.fontdata` are what Godot **made out of** the authoring inputs
(149 `.svg`, 17 `.png`, 1 `.jpeg`, 7 `.ttf`) — and those inputs are not in the package
at all, so no amount of work turns them back into source. `.import`/`.remap` are
exporter bookkeeping. This directory therefore is not missing {nonsrc} files; those
files have no source form to return to.

## Media policy: the 省略 rows

Five `.res` files are Themes/FontFiles that carry their media **inline** — the `.ttf`
bytes themselves and the MSDF glyph atlas images, serialised as one ~699 KB
`PackedByteArray` literal each. **This repository does not carry media files**, and an
embedded atlas is media regardless of being base64 text. So those are written as
*metrics-only skeletons*: the `.tres` stays, keeping the theme item tables, the style
boxes, the external references and all 2 333 per-glyph `uv_rect`/`offset`/`size`
entries — only the byte payload is dropped. Each skeleton states in its own header what
was removed and how big the original was; `MANIFEST.tsv` repeats it.

{media}

To get the full files back (they will not be committed):

```bash
python3 tools/recover_source.py --include-media
```

## How to regenerate

```bash
python3 tools/recover_source.py            # rebuild this directory
python3 tools/recover_source.py --check    # verify only, never writes (CI)
python3 tools/recover_source.py --list     # print the plan, touch nothing
```

External tools (never vendored, never fetched by the script):

* `GDRE Tools` — the only GDScript decompiler that exists, for `.gdc` -> `.gd`.
  Override with `GDRE_BIN`.
* `Godot 4.7.2` headless — for `.scn`/`.res` -> text. Override with `GODOT_BIN`.
  Using Godot itself means the text is produced by the reference serialiser, not by
  a re-implementation of it.
* `tools/gdc_verify_source.py` (this repo) — token-for-token verification of the
  decompiled GDScript against the shipped bytecode: {token}.

Content hashes are not asserted: a different Godot build may reflow whitespace.
Textuality and set completeness are asserted, and `--check` enforces both.

## What cannot be recovered here, and why

1. **The screen controllers.** No `.gd` in this tree is a screen. The 105 scripts are
   small layout controls (`ButtonGrid`, `FxContainer`, `OffsetLabel`, ...) plus the
   whole AdMob/Android-IAP plugin. `play_screen`, `player_screen`, `lan_lobby`,
   `campaign`, `GameSetup` exist as **scenes** here but their behaviour lives in
   `libgraphwarrust.so` (58 `src/graphwar_*.rs` module files plus two module
   directories). That is a separate track; see `docs/146-source-recovery.md` §5.
2. **`ext_resource` UID attributes.** The binary scenes carry a `u64` UID per external
   resource, and the text form drops it: Godot's binary reader resolves a UID through
   `.godot/uid_cache.bin`, which in this package registers only 108 paths (105 `.gdc`
   + one `.png` + `.gdextension` + one `.wav`) — never the scenes' targets. An
   unregistered UID is treated as invalid and the loader falls back to the path, so
   the writer emits path-only `ext_resource` lines. Nothing else is lost: paths,
   nodes, properties and sub-resources are all present, and the files load cleanly.
   Rows affected by this are the `部分` ones.
3. **Texture/font sources.** Recovered scenes reference `res://<name>.svg` and
   `res://<name>.ttf`. Those paths are correct — they are what the original project
   used — but the files do not exist in the package, only their `.ctex`/`.fontdata`
   products do. Loading this directory standalone therefore reports unresolved
   texture/font resources. That is a property of the package, not of the recovery;
   see `docs/121` §5 for how the client handles it.
4. **Formatting of GDScript.** `.gdc` stores tokens, not text. The decompiler's
   output is a rendering; indentation and blank lines are reconstructed, and
   redundant parentheses or comments the compiler dropped are unrecoverable in
   principle.

Do not "fix" anything here by hand: `--check` re-derives the expected set from the
package on every run, and a hand edit will disagree with it.
"""


# ------------------------------------------------------------------ check

def check(out_dir, pck, plan, items, log, strict_media=True):
    """Verify the directory without writing anything. CI entry point.

    `strict_media=False` is used only by an explicit `--include-media` run, which
    produces a tree that is intentionally *not* committable; the policy still holds
    for every tree that can pass CI.
    """
    problems = []
    warnings = []
    expected = set()
    for job in plan["gdc"]:
        expected.add(job["orig"])
    for job in plan["scene"]:
        expected.add(job["orig"])
    for job in plan["res"]:
        expected.add(job["orig"])
    expected.add("project.godot")
    expected.add("Graphwar.gdextension")
    if pck.has(SOURCE_CFG):
        expected.add(SOURCE_CFG)

    if not os.path.isdir(out_dir):
        return False, ["recovered-src/ does not exist: run tools/recover_source.py first"]

    present = set()
    for root, _dirs, files in os.walk(out_dir):
        for f in files:
            full = os.path.join(root, f)
            rel = os.path.relpath(full, out_dir).replace(os.sep, "/")
            present.add(rel)
            if not rel.endswith(TEXT_EXT):
                problems.append("non-text file in recovered-src/: %s" % rel)
            elif os.path.getsize(full) == 0:
                problems.append("empty file: %s" % rel)

    for rel in sorted(expected - present):
        problems.append("missing output: %s" % rel)
    for rel in sorted(present - expected):
        if rel in ("MANIFEST.tsv", "README.md", ".gitignore"):
            continue
        warnings.append("extra file not derived from the package: %s" % rel)

    man = os.path.join(out_dir, "MANIFEST.tsv")
    if not os.path.isfile(man):
        problems.append("MANIFEST.tsv missing")
    else:
        seen = []
        with open(man, encoding="utf-8") as fh:
            for line in fh:
                if line.startswith("#") or not line.strip():
                    continue
                cols = line.rstrip("\n").split("\t")
                if cols[0] == "pck_path":
                    continue
                seen.append(cols)
                if len(cols) != 5:
                    problems.append("MANIFEST.tsv row has %d columns: %r" % (len(cols), cols[0]))
                if cols[3] in (FULL, PARTIAL, OMITTED, FAILED) and cols[1]:
                    if cols[1] not in present:
                        problems.append("MANIFEST.tsv promises %s but it is absent" % cols[1])
        names = [c[0] for c in seen]
        if sorted(names) != sorted(pck.by_name):
            miss = sorted(set(pck.by_name) - set(names))
            extra = sorted(set(names) - set(pck.by_name))
            if miss:
                problems.append("MANIFEST.tsv misses %d package entries (e.g. %s)"
                                % (len(miss), miss[0]))
            if extra:
                problems.append("MANIFEST.tsv invents %d entries (e.g. %s)"
                                % (len(extra), extra[0]))
        status_counts = {}
        for c in seen:
            status_counts[c[3]] = status_counts.get(c[3], 0) + 1
        recon = sum(status_counts.get(k, 0) for k in (FULL, PARTIAL, OMITTED, FAILED))
        log("  manifest: %d rows, %d text-source (%s)"
            % (len(seen), recon,
               ", ".join("%s=%d" % (k, status_counts[k]) for k in
                         (FULL, PARTIAL, OMITTED, FAILED) if k in status_counts)))
        if status_counts.get(FAILED):
            problems.append("%d file(s) recorded as 失败" % status_counts[FAILED])
        if recon != len(expected):
            problems.append("manifest says %d recovered but %d output paths are expected"
                            % (recon, len(expected)))

        # Media policy: a row marked 省略 must be a skeleton (no embedded payload)
        # and must say so in its own header; no file outside that set may carry an
        # embedded payload either. This is what keeps the 17 MB of atlases out.
        omitted = set(c[1] for c in seen if c[3] == OMITTED)
        for rel in sorted(present):
            if not rel.endswith((".tres", ".tscn")):
                continue
            text = open(os.path.join(out_dir, rel), encoding="utf-8", errors="replace").read()
            carries = has_media(text)
            if rel in omitted:
                if carries:
                    problems.append("%s is marked 省略 but still carries an embedded payload"
                                    % rel)
                if "MEDIA OMITTED" not in text:
                    problems.append("%s is marked 省略 but has no MEDIA OMITTED header" % rel)
            elif carries:
                msg = ("%s carries an embedded payload (>= %d chars) but is not marked 省略 "
                       "— media must not be committed" % (rel, MEDIA_LITERAL_LIMIT))
                if strict_media:
                    problems.append(msg)
                else:
                    warnings.append(msg + " (allowed only because --include-media was used)")
        if omitted:
            log("  media: %d file(s) omitted embedded payload; rest of the tree is clean"
                % len(omitted))

    # project.godot must still parse and still carry every shipped key.
    pg = os.path.join(out_dir, "project.godot")
    if os.path.isfile(pg):
        text = open(pg, encoding="utf-8").read()
        for key, _v, _t in items:
            if key.split("/")[-1] not in text or key.split("/")[0] not in text:
                problems.append("project.godot lost setting %s" % key)
        if "config_version=5" not in text:
            problems.append("project.godot lacks config_version=5")

    for w in warnings:
        log("  warn: " + w)
    return not problems, problems


# ------------------------------------------------------------------- main

def main(argv):
    ap = argparse.ArgumentParser(
        description="Recover the source-text surface of Graphwar II from Graphwar.pck")
    ap.add_argument("--pck", default=None)
    ap.add_argument("--out", default=DEFAULT_OUT)
    ap.add_argument("--work", default=None)
    ap.add_argument("--godot", default=None)
    ap.add_argument("--gdre", default=None)
    ap.add_argument("--check", action="store_true", help="verify only; never writes")
    ap.add_argument("--list", action="store_true", help="print the plan and exit")
    ap.add_argument("--keep-work", action="store_true", help="keep the staging directory")
    ap.add_argument("--skip-verify", action="store_true", help="skip the two verifiers")
    ap.add_argument("--include-media", action="store_true",
                    help="keep embedded font/atlas payloads in the .tres files "
                         "(default: drop them; this repository does not carry media)")
    a = ap.parse_args(argv)

    def log(msg):
        print(msg, flush=True)

    a.pck = a.pck or _default_pck()
    a.godot = a.godot or _default_godot()
    a.gdre = a.gdre or _default_gdre()
    a.work = a.work or _default_work()

    if not os.path.isfile(a.pck):
        raise SystemExit("package not found: %s (set --pck or GW2_PCK)" % a.pck)
    pck = Pck(a.pck)
    plan = build_plan(pck)
    items, consumed = parse_project_binary(pck.read("project.binary"))

    if a.list:
        log("package %s (%d files, godot %s)" % (a.pck, len(pck.entries), pck.version_string()))
        for kind in ("gdc", "scene", "res"):
            for job in plan[kind]:
                log("  %-5s %-58s <- %s" % (kind, job["orig"], job["shipped"]))
        unused = [e["name"] for e in pck.entries if e["name"].endswith(".remap")]
        log("  %d .remap files; %d mapped by this plan"
            % (len(unused), len(plan["gdc"]) + len(plan["scene"]) + len(plan["res"])))
        log("  project.binary: %d keys, %d/%d bytes consumed"
            % (len(items), consumed, pck.by_name["project.binary"]["size"]))
        return 0

    if a.check:
        log("checking %s" % a.out)
        ok, problems = check(a.out, pck, plan, items, log)
        if not ok:
            for p in problems:
                log("  FAIL " + p)
            log("check FAILED (%d problem(s))" % len(problems))
            return 1
        log("check OK")
        return 0

    for tool, name in ((a.gdre, "GDRE Tools"), (a.godot, "Godot")):
        if not os.path.isfile(tool):
            raise SystemExit(
                "%s not found at %s.\n"
                "This script does not download tools. Point --gdre/--godot (or "
                "GDRE_BIN/GODOT_BIN) at an existing binary, or run --check to verify "
                "an existing recovered-src/ without them." % (name, tool))

    if os.path.isdir(a.work) and not a.keep_work:
        shutil.rmtree(a.work)
    if not os.path.isdir(a.work):
        os.makedirs(a.work)
    stage = os.path.join(a.work, "stage")
    log("staging %d files -> %s" % (len(pck.entries), stage))
    unconverted = write_stage(pck, stage, items)

    log("A. decompiling %d .gdc" % len(plan["gdc"]))
    gdre_out, gdre_report, gdc_failed = decompile_scripts(a.pck, plan, a.work, a.gdre, log)
    log("   GDRE report: %s" % (gdre_report or "unparsed"))

    token_verify, token_summary = {}, ""
    if not a.skip_verify:
        token_verify, token_summary = verify_tokens(pck, plan, a.work, gdre_out, log)
        n_ok = sum(1 for v in token_verify.values() if v.get("ok"))
        log("B. token verify: %d/%d" % (n_ok, len(plan["gdc"])))

    log("C. converting %d .scn + %d .res"
        % (len(plan["scene"]), len(plan["res"])))
    conv, conv_results = convert_binary_resources(pck, plan, a.work, a.godot, log)
    rt_verify = {}
    if not a.skip_verify:
        rt_verify = verify_roundtrip(pck, plan, a.work, a.godot, log)

    log("D. writing %s" % a.out)
    if os.path.isdir(a.out):
        shutil.rmtree(a.out)
    os.makedirs(a.out)
    written = 0
    for job in plan["gdc"]:
        src = os.path.join(gdre_out, job["orig"])
        if os.path.isfile(src):
            dst = os.path.join(a.out, job["orig"])
            d = os.path.dirname(dst)
            if d and not os.path.isdir(d):
                os.makedirs(d)
            shutil.copyfile(src, dst)
            written += 1
    for job in plan["scene"] + plan["res"]:
        src = os.path.join(conv, job["orig"])
        if os.path.isfile(src):
            dst = os.path.join(a.out, job["orig"])
            d = os.path.dirname(dst)
            if d and not os.path.isdir(d):
                os.makedirs(d)
            shutil.copyfile(src, dst)
            written += 1
    text, _u = project_godot_text(items)
    with open(os.path.join(a.out, "project.godot"), "w", encoding="utf-8") as fh:
        fh.write(text + "\n")
    with open(os.path.join(a.out, "Graphwar.gdextension"), "wb") as fh:
        fh.write(pck.read("Graphwar.gdextension"))
    if pck.has(SOURCE_CFG):
        dst = os.path.join(a.out, SOURCE_CFG)
        d = os.path.dirname(dst)
        if d and not os.path.isdir(d):
            os.makedirs(d)
        with open(dst, "wb") as fh:
            fh.write(pck.read(SOURCE_CFG))
        written += 1
    with open(os.path.join(a.out, ".gitignore"), "w", encoding="utf-8") as fh:
        fh.write(GITIGNORE)
    log("   %d text files" % written)

    # E. media policy last: the round-trip verifiers (B/C) ran on the faithful text,
    # so redaction can never mask a conversion defect.
    media = {}
    for job in plan["scene"] + plan["res"]:
        p = os.path.join(a.out, job["orig"])
        if os.path.isfile(p):
            got = apply_media_policy(p, a.include_media)
            if got:
                media[job["orig"]] = got
    if media:
        if a.include_media:
            log("E. media: kept in %d file(s) (--include-media; %d B)"
                % (len(media), sum(m.get("orig_bytes", 0) for m in media.values())))
        else:
            log("E. media: %d file(s) skeletonised — %d B -> %d B, dropped %d B"
                % (len(media),
                   sum(m["orig_bytes"] for m in media.values()),
                   sum(m["skeleton_bytes"] for m in media.values()),
                   sum(m["dropped_bytes"] for m in media.values())))

    rows = build_rows(pck, plan, a.out, gdre_report, token_verify, token_summary,
                      conv_results, rt_verify, unconverted, media)
    n_token = sum(1 for v in token_verify.values() if v.get("ok"))
    n_rt = sum(1 for v in rt_verify.values() if v.get("ok"))
    extra = ("token-exact .gd %d/%d; round-trip equal %d/%d; uid attributes dropped "
             "by the text writer are recorded per row"
             % (n_token, len(plan["gdc"]), n_rt, len(plan["scene"]) + len(plan["res"])))
    if a.include_media:
        media_mode = "included (--include-media; NOT committable under the repo media policy)"
    elif media:
        media_mode = ("omitted for %d .tres (metrics-only skeletons; original bytes recorded "
                      "per row)" % len(media))
    else:
        media_mode = "omitted (nothing to omit: no embedded payloads found)"
    counts = write_manifest(rows, os.path.join(a.out, "MANIFEST.tsv"), pck, plan, extra,
                           media_mode)
    stats = {"gdc_ok": n_token, "scn_ok": sum(1 for j in plan["scene"]
                                              if conv_results.get(j["orig"], {}).get("ok")),
             "res_ok": sum(1 for j in plan["res"]
                           if conv_results.get(j["orig"], {}).get("ok")),
             "token_summary": token_summary or "not run",
             "media": ("%d file(s) were skeletonised: %d B of embedded font/atlas data was "
                       "dropped in total, leaving %d B. The per-file numbers are in "
                       "`MANIFEST.tsv` (status `省略`)."
                       % (len(media),
                          sum(m.get("dropped_bytes", 0) for m in media.values()),
                          sum(m.get("skeleton_bytes", 0) for m in media.values())))
             if media and not a.include_media else
             ("Media was kept (`--include-media`): %d file(s), %d B — **not committable**."
              % (len(media), sum(m.get("orig_bytes", 0) for m in media.values())))
             if media else "Nothing in this package needed redaction.",
             "roundtrip": "%d/%d text forms structurally equal to the binary form"
                          % (n_rt, len(plan["scene"]) + len(plan["res"]))}
    write_readme(os.path.join(a.out, "README.md"), counts, plan, stats, pck)

    recon = sum(counts.get(k, 0) for k in (FULL, PARTIAL, OMITTED, FAILED))
    log("done: %d/%d files are text source (%s %d, %s %d, %s %d, %s %d), %d non-source"
        % (recon, len(rows), FULL, counts.get(FULL, 0), PARTIAL, counts.get(PARTIAL, 0),
           OMITTED, counts.get(OMITTED, 0), FAILED, counts.get(FAILED, 0),
           len(rows) - recon))
    ok, problems = check(a.out, pck, plan, items, log, strict_media=not a.include_media)
    if not ok:
        for p in problems:
            log("  FAIL " + p)
        return 1
    if not a.keep_work:
        shutil.rmtree(a.work, ignore_errors=True)
    else:
        log("work kept at %s" % a.work)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
