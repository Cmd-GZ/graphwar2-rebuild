#!/usr/bin/env python3
"""Parse Godot 4 text scenes (.tscn) into a control table, and diff against runtime dumps.

Why: `docs/92` enumerated the UI **at runtime** (node path / class / text / script only).
GDRE Tools can now recover the *source* project, which turns the same UI into exact
attributes: anchors/offsets, size flags, custom minimum size, theme overrides
(font size / margin constants / styleboxes) and `[connection]` blocks.

This tool reads the recovered `.tscn` files **outside** the repo (they are game assets and
must never be committed), expands instanced sub-scenes, and emits a per-scene node table.
With `--runtime` it also diffs the static tree against the `@@N` node dumps produced by
`tools/dump_ui.gd` (see docs/92 §9), so "static has it, runtime did not show it" and
"runtime created it dynamically" are separated instead of hand-waved.

Scene format (Godot 4, text): sections `[ext_resource]`, `[sub_resource]`, `[node]`,
`[connection]`, `[editable]`; node properties are `key = value` lines with Godot's
text-serialisation syntax (strings, `Vector2(..)`, `ExtResource("id")`, `SubResource("id")`,
`&"StringName"`, arrays, dictionaries).

Usage:
    tools/tscn_inventory.py --all ../.agent-work/tools/recovered            # summary
    tools/tscn_inventory.py --all <dir> --json out.json                     # full model
    tools/tscn_inventory.py --scene <dir>/play_screen.tscn                  # one scene table
    tools/tscn_inventory.py --all <dir> --md controls.md                    # markdown tables
    tools/tscn_inventory.py --all <dir> --runtime ../.rea-work/ui/boot.log  # diff vs runtime

Notes / limits:
  * the recovered scenes carry **no** `[connection]` block for the game itself (only the
    AdMob addon samples do), so connections are reported as observed, not assumed;
  * Godot auto-renames duplicate sibling names (`MarginContainer` -> `MarginContainer7`);
    the diff matches siblings by order + type + name-with-suffix, never by name alone;
  * values are reported as serialised; nothing is evaluated.
"""
import argparse
import json
import os
import re
import sys

# ---------------------------------------------------------------------------
# Godot text-serialisation scalar/constructor parser
# ---------------------------------------------------------------------------
_STRING_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')


def _unescape(s):
    out = []
    i = 0
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            n = s[i + 1]
            if n == "n":
                out.append("\n")
            elif n == "t":
                out.append("\t")
            elif n == "r":
                out.append("\r")
            elif n == '"':
                out.append('"')
            elif n == "\\":
                out.append("\\")
            elif n == "u" and i + 5 < len(s) + 1:
                try:
                    out.append(chr(int(s[i + 2:i + 6], 16)))
                    i += 4
                except ValueError:
                    out.append(n)
            else:
                out.append(n)
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


class _P(object):
    """Tiny recursive-descent parser for the right-hand side of a .tscn property."""

    def __init__(self, text):
        self.s = text
        self.i = 0

    def ws(self):
        while self.i < len(self.s) and self.s[self.i] in " \t":
            self.i += 1

    def parse(self):
        self.ws()
        v = self.value()
        self.ws()
        return v

    def value(self):
        self.ws()
        if self.i >= len(self.s):
            return None
        c = self.s[self.i]
        if c == '"':
            return self.string()
        if c == "&" and self.s[self.i:self.i + 2] == '&"':
            self.i += 1
            return self.string()
        if c == "[":
            return self.array()
        if c == "{":
            return self.dictionary()
        m = re.match(r'-?\d+\.\d*(?:[eE][-+]?\d+)?', self.s[self.i:])
        if m:
            self.i += m.end()
            return float(m.group(0))
        m = re.match(r'-?\d+', self.s[self.i:])
        if m:
            self.i += m.end()
            return int(m.group(0))
        m = re.match(r'[A-Za-z_][A-Za-z0-9_]*', self.s[self.i:])
        if m:
            name = m.group(0)
            self.i += m.end()
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == "(":
                self.i += 1
                args = []
                self.ws()
                if self.i < len(self.s) and self.s[self.i] == ")":
                    self.i += 1
                else:
                    while True:
                        args.append(self.value())
                        self.ws()
                        if self.i < len(self.s) and self.s[self.i] == ",":
                            self.i += 1
                            continue
                        if self.i < len(self.s) and self.s[self.i] == ")":
                            self.i += 1
                        break
                if name == "ExtResource":
                    return {"__ext__": args[0] if args else None}
                if name == "SubResource":
                    return {"__sub__": args[0] if args else None}
                return {"__ctor__": name, "args": args}
            if name == "true":
                return True
            if name == "false":
                return False
            if name == "null":
                return None
            if name == "inf":
                return float("inf")
            return name
        raise ValueError("cannot parse value at %r" % self.s[self.i:self.i + 30])

    def string(self):
        m = _STRING_RE.match(self.s, self.i)
        if not m:
            raise ValueError("bad string at %r" % self.s[self.i:self.i + 30])
        self.i = m.end()
        return _unescape(m.group(1))

    def array(self):
        self.i += 1                              # '['
        out = []
        self.ws()
        if self.i < len(self.s) and self.s[self.i] == "]":
            self.i += 1
            return out
        while True:
            out.append(self.value())
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ",":
                self.i += 1
                continue
            if self.i < len(self.s) and self.s[self.i] == "]":
                self.i += 1
            break
        return out

    def dictionary(self):
        self.i += 1                              # '{'
        out = {}
        self.ws()
        if self.i < len(self.s) and self.s[self.i] == "}":
            self.i += 1
            return out
        while True:
            k = self.value()
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ":":
                self.i += 1
            v = self.value()
            out[str(k)] = v
            self.ws()
            if self.i < len(self.s) and self.s[self.i] == ",":
                self.i += 1
                continue
            if self.i < len(self.s) and self.s[self.i] == "}":
                self.i += 1
            break
        return out


def parse_value(text):
    return _P(text).parse()


# ---------------------------------------------------------------------------
# Scene parsing
# ---------------------------------------------------------------------------
_SECTION_RE = re.compile(r"^\[(\w+)((?:\s+\w+=(?:\"[^\"]*\"|[^\s\]]+))*)\]\s*$")
_ATTR_RE = re.compile(r'(\w+)=("(?:[^"\\]|\\.)*"|[^\s\]]+)')


def _attrs(s):
    out = {}
    for m in _ATTR_RE.finditer(s):
        v = m.group(2)
        if v.startswith('"'):
            v = _unescape(v[1:-1])
        out[m.group(1)] = v
    return out


def parse_scene(path):
    """Parse one .tscn into {file, ext, sub, nodes, connections}."""
    with open(path, "r", encoding="utf-8") as fh:
        lines = fh.read().split("\n")
    scene = {"file": path, "ext": {}, "sub": {}, "nodes": [], "connections": [], "format": None}
    cur = None          # ('node'|'sub'|None)
    for raw in lines:
        line = raw.rstrip()
        if not line:
            continue
        if line.startswith("["):
            m = _SECTION_RE.match(line)
            if not m:
                continue
            kind, attrs = m.group(1), _attrs(m.group(2))
            if kind == "gd_scene":
                scene["format"] = attrs
                cur = None
            elif kind == "ext_resource":
                scene["ext"][attrs["id"]] = {"type": attrs.get("type"),
                                             "path": attrs.get("path"),
                                             "uid": attrs.get("uid")}
                cur = None
            elif kind == "sub_resource":
                scene["sub"][attrs["id"]] = {"type": attrs.get("type"), "props": {}}
                cur = ("sub", attrs["id"])
            elif kind == "node":
                node = {"name": attrs.get("name"), "type": attrs.get("type"),
                        "parent": attrs.get("parent"), "instance": attrs.get("instance"),
                        "groups": attrs.get("groups"), "props": {}}
                if "instance" in attrs:
                    m2 = re.search(r'ExtResource\("([^"]+)"\)|ExtResource\(\'([^\']+)\'\)',
                                   line)
                    if m2:
                        node["instance"] = m2.group(1) or m2.group(2)
                scene["nodes"].append(node)
                cur = ("node", node)
            elif kind == "connection":
                scene["connections"].append(attrs)
                cur = None
            else:
                cur = None
            continue
        if cur is None or "=" not in line:
            continue
        key, _, val = line.partition("=")
        key = key.strip()
        try:
            parsed = parse_value(val.strip())
        except Exception:
            parsed = val.strip()
        if cur[0] == "node":
            cur[1]["props"][key] = parsed
        else:
            scene["sub"][cur[1]]["props"][key] = parsed
    # resolve `script = ExtResource("id")` here: an instanced node's ext ids belong to
    # the *sub* scene's table, so resolving later against the top scene would be wrong.
    for node in scene["nodes"]:
        ref = node["props"].get("script")
        if isinstance(ref, dict) and "__ext__" in ref:
            ent = scene["ext"].get(ref["__ext__"])
            if ent and ent.get("path"):
                node["props"]["script"] = ent["path"]
    return scene


def build_tree(scene):
    """Attach nodes to parents (local file only, no instance expansion).

    `parent="."` means "child of the scene root", so every stored path is prefixed
    by the root's own name: the map key is the full path, not the file-relative one.
    """
    root = None
    for node in scene["nodes"]:
        if node["parent"] is None:
            root = node
            break
    if root is None:
        return None
    prefix = root["name"]
    by_path = {}
    for node in scene["nodes"]:
        parent = node["parent"]
        if parent is None:
            node["_path"] = prefix
        elif parent == ".":
            node["_path"] = prefix + "/" + node["name"]
        else:
            node["_path"] = prefix + "/" + parent + "/" + node["name"]
        node["children"] = []
        by_path[node["_path"]] = node
    for node in scene["nodes"]:
        parent = node["parent"]
        if parent is None:
            continue
        pkey = prefix if parent == "." else prefix + "/" + parent
        if pkey in by_path:
            by_path[pkey]["children"].append(node)
    return root


def expand(scene, root, scene_dirs, _stack=()):
    """Materialise a node subtree, merging instanced scenes recursively."""
    out = []

    def visit(node, base, override_props):
        props = dict(node["props"])
        props.update(override_props)
        typ = node.get("type")
        children = []
        inst = node.get("instance")
        inst_root = None
        if inst is not None:
            ref = scene["ext"].get(inst)
            if ref and ref.get("type") == "PackedScene" and ref.get("path"):
                target = resolve_scene(ref["path"], scene_dirs)
                if target and target not in _stack:
                    sub = parse_scene(target)
                    inst_root = build_tree(sub)
                    if inst_root is not None:
                        typ = typ or inst_root.get("type")
                        merged = dict(inst_root["props"])
                        merged.update(props)
                        props = merged
                        children = list(inst_root.get("children") or [])
        own_children = list(node.get("children") or [])
        # local children override same-named instanced children
        own_names = {c["name"] for c in own_children}
        children = [c for c in children if c["name"] not in own_names] + own_children
        path = base + node["name"]
        rec = {"path": path, "name": node["name"], "type": typ,
               "instance": (scene["ext"].get(inst, {}).get("path") if inst else None),
               "props": props, "children": []}
        out.append(rec)
        for child in children:
            visit(child, path + "/", {})
        return rec

    if root is not None:
        visit(root, "", {})
    return out


def resolve_scene(res_path, scene_dirs):
    name = res_path.replace("res://", "")
    stem = name[:-5] if name.endswith(".tscn") else name
    for d in scene_dirs:
        cand = os.path.join(d, stem + ".tscn")
        if os.path.exists(cand):
            return cand
    return None


# ---------------------------------------------------------------------------
# Table projection
# ---------------------------------------------------------------------------
ANCHOR_KEYS = ("anchors_preset", "anchor_left", "anchor_top", "anchor_right",
               "anchor_bottom", "offset_left", "offset_top", "offset_right",
               "offset_bottom", "grow_horizontal", "grow_vertical")
SIZE_KEYS = ("size_flags_horizontal", "size_flags_vertical",
             "size_flags_stretch_ratio", "custom_minimum_size", "layout_mode")
TEXT_KEYS = ("text", "placeholder_text", "tooltip_text", "alignment",
             "horizontal_alignment", "vertical_alignment", "autowrap_mode")


def _fmt(v):
    if isinstance(v, dict):
        if "__ext__" in v:
            return "ExtResource(%s)" % v["__ext__"]
        if "__sub__" in v:
            return "SubResource(%s)" % v["__sub__"]
        if "__ctor__" in v:
            return "%s(%s)" % (v["__ctor__"], ", ".join(_fmt(a) for a in v.get("args", [])))
        return str(v)
    if isinstance(v, float) and v.is_integer():
        return str(int(v))
    if isinstance(v, str):
        return v
    if isinstance(v, (list, tuple)):
        return "[" + ", ".join(_fmt(x) for x in v) + "]"
    return str(v)


def project(records, scene):
    """Flatten expanded records into table rows with the attributes docs/92 lacks."""
    rows = []
    for rec in records:
        p = rec["props"]
        anchors = {k: p[k] for k in ANCHOR_KEYS if k in p}
        sizes = {k: p[k] for k in SIZE_KEYS if k in p}
        texts = {k: p[k] for k in TEXT_KEYS if k in p}
        overrides = {k: v for k, v in p.items() if k.startswith("theme_override_")}
        script = p.get("script") if isinstance(p.get("script"), str) else None
        row = {
            "path": rec["path"],
            "type": rec["type"],
            "instance": rec["instance"],
            "script": script,
            "anchors": {k: _fmt(v) for k, v in anchors.items()},
            "size": {k: _fmt(v) for k, v in sizes.items()},
            "text": {k: _fmt(v) for k, v in texts.items()},
            "theme_overrides": {k: _fmt(v) for k, v in overrides.items()},
            "visible": p.get("visible"),
            "unique": p.get("unique_name_in_owner"),
            "groups": p.get("groups"),
        }
        rows.append(row)
    return rows


def resolve_styleboxes(scene):
    """Sub-resource styleboxes -> their content margins/colors (nine-patch info)."""
    out = {}
    for sid, sub in scene["sub"].items():
        if (sub.get("type") or "").startswith("StyleBox"):
            p = sub["props"]
            out[sid] = {
                "type": sub["type"],
                "content_margin_left": _fmt(p.get("content_margin_left")),
                "content_margin_top": _fmt(p.get("content_margin_top")),
                "content_margin_right": _fmt(p.get("content_margin_right")),
                "content_margin_bottom": _fmt(p.get("content_margin_bottom")),
                "bg_color": _fmt(p.get("bg_color")),
            }
    return out


# ---------------------------------------------------------------------------
# Runtime dump parsing / diff (docs/92 §9 format)
# ---------------------------------------------------------------------------
def parse_runtime_log(path):
    """@@N|snapshot|path|class|vis|script|text|flags  -> {snapshot: {path: node}}"""
    snaps = {}
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if not line.startswith("@@N|"):
                continue
            parts = line.rstrip("\n").split("|")
            if len(parts) < 8:
                continue
            _tag, snap, path, cls, vis, script, text, flags = parts[:8]
            snaps.setdefault(snap, {})[path] = {
                "class": cls, "script": script if script != "-" else None,
                "text": text, "flags": flags,
            }
    return snaps


def scene_root_of(runtime_nodes, scene_res):
    """Find the runtime node whose flags declare `scene=res://...`."""
    for path, node in runtime_nodes.items():
        if ("scene=" + scene_res) in (node["flags"] or ""):
            return path
    return None


def _name_matches(static_name, runtime_name):
    if static_name == runtime_name:
        return True
    # Godot appends a collision counter: MarginContainer -> MarginContainer7
    m = re.match(r"^(.*?)(\d+)$", runtime_name)
    return bool(m) and m.group(1) == static_name


def diff_tree(static_rows, static_root, runtime_nodes, runtime_root):
    """Order-sensitive sibling match: (type, name-or-name+suffix)."""
    def children_of(paths, base):
        out = []
        for path in paths:
            if base == "":
                ok = path.count("/") == 1
            else:
                ok = path.startswith(base + "/") and path.count("/") == base.count("/") + 1
            if ok:
                out.append(path)
        return out

    matched, static_only, runtime_only, mismatched = [], [], [], []
    by_path = {r["path"]: r for r in static_rows}

    def walk(spath, rpath):
        srec = by_path.get(spath)
        rnode = runtime_nodes.get(rpath)
        if srec and rnode:
            cls = rnode["class"]
            if srec["type"] and cls and srec["type"] != cls:
                mismatched.append({"path": rpath, "field": "class",
                                   "static": srec["type"], "runtime": cls})
            if srec["script"] and rnode["script"] and srec["script"] != rnode["script"]:
                mismatched.append({"path": rpath, "field": "script",
                                   "static": srec["script"], "runtime": rnode["script"]})
            matched.append(rpath)
        rev = {p: r for r, p in [(r, r["path"]) for r in static_rows]}
        s_children = [r["path"] for r in static_rows
                      if r["path"].startswith(spath + "/") and
                      r["path"].count("/") == spath.count("/") + 1]
        r_children = [p for p in runtime_nodes
                      if p.startswith(rpath + "/") and
                      p.count("/") == rpath.count("/") + 1]
        used = set()
        for sc in s_children:
            sname = sc.rsplit("/", 1)[-1]
            stype = by_path[sc]["type"]
            hit = None
            for rc in r_children:
                if rc in used:
                    continue
                rname = rc.rsplit("/", 1)[-1]
                if _name_matches(sname, rname) and \
                        (stype is None or runtime_nodes[rc]["class"] == stype):
                    hit = rc
                    break
            if hit is None:
                for sp in static_rows:
                    if sp == sc or sp.startswith(sc + "/"):
                        static_only.append(sp)
            else:
                used.add(hit)
                walk(sc, hit)
        for rc in r_children:
            if rc not in used:
                for rp in runtime_nodes:
                    if rp == rc or rp.startswith(rc + "/"):
                        runtime_only.append(rp)
    walk(static_root, runtime_root)
    return matched, static_only, runtime_only, mismatched


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def load_scene(path, scene_dirs):
    scene = parse_scene(path)
    root = build_tree(scene)
    records = expand(scene, root, scene_dirs)
    rows = project(records, scene)
    return scene, root, rows


def main(argv):
    ap = argparse.ArgumentParser(description="Godot .tscn control inventory (docs/104)")
    ap.add_argument("--all", metavar="DIR", help="every *.tscn under DIR (non-recursive)")
    ap.add_argument("--scene", metavar="FILE", help="one .tscn")
    ap.add_argument("--json", metavar="OUT")
    ap.add_argument("--md", metavar="OUT")
    ap.add_argument("--runtime", metavar="LOG", help="@@N dump (e.g. boot.log)")
    ap.add_argument("--snapshot", default="boot_all_screens")
    ap.add_argument("--recursive", action="store_true",
                    help="with --all: walk sub-directories too (e.g. addons/)")
    ap.add_argument("--quiet", action="store_true")
    a = ap.parse_args(argv)

    targets = []
    if a.scene:
        targets.append(a.scene)
    if a.all:
        if a.recursive:
            for root, _dirs, files in os.walk(a.all):
                targets.extend(sorted(os.path.join(root, f) for f in files
                                      if f.endswith(".tscn")))
        else:
            targets.extend(sorted(os.path.join(a.all, f) for f in os.listdir(a.all)
                                  if f.endswith(".tscn")))
    if not targets:
        ap.error("pass --scene FILE or --all DIR")
    scene_dirs = sorted({os.path.dirname(os.path.abspath(t)) for t in targets})
    scene_dirs.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "rsc")))

    model, md_chunks = [], []
    total_nodes = 0
    total_conns = 0
    for path in targets:
        scene, root, rows = load_scene(path, scene_dirs)
        total_nodes += len(rows)
        total_conns += len(scene["connections"])
        name = os.path.basename(path)
        if not a.quiet:
            print("%-34s nodes=%-4d conns=%-3d styleboxes=%d instanced=%d" % (
                name, len(rows), len(scene["connections"]),
                len(resolve_styleboxes(scene)),
                sum(1 for r in rows if r["instance"])))
        model.append({
            "file": path, "name": name, "root": rows[0]["path"] if rows else None,
            "node_count": len(rows),
            "connections": scene["connections"],
            "styleboxes": resolve_styleboxes(scene),
            "nodes": rows,
        })
        md_chunks.append((name, rows))

    if not a.quiet:
        print("\n合计: %d 个场景, %d 个节点, %d 条 [connection]" %
              (len(model), total_nodes, total_conns))

    if a.runtime:
        snaps = parse_runtime_log(a.runtime)
        runtime_nodes = snaps.get(a.snapshot)
        if runtime_nodes is None:
            print("!! snapshot %r not in %s (have: %s)" %
                  (a.snapshot, a.runtime, ", ".join(sorted(snaps)[:5])), file=sys.stderr)
        else:
            print("\n== 对拍 %s ==" % a.snapshot)
            for entry in model:
                scene_res = "res://" + entry["name"]
                rroot = scene_root_of(runtime_nodes, scene_res)
                if not rroot:
                    continue
                srows = entry["nodes"]
                matched, sonly, ronly, mism = diff_tree(
                    srows, srows[0]["path"], runtime_nodes, rroot)
                entry["diff"] = {"runtime_root": rroot, "matched": len(matched),
                                 "static_only": sonly, "runtime_only": ronly,
                                 "mismatched": mism}
                print("%-30s runtime=%d  matched=%-4d static_only=%-4d runtime_only=%-4d mism=%d" % (
                    entry["name"], len([p for p in runtime_nodes if p == rroot or
                                        p.startswith(rroot + "/")]),
                    len(matched), len(sonly), len(ronly), len(mism)))

    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump({"scenes": model}, fh, ensure_ascii=False, indent=1)
        print("wrote %s" % a.json)

    if a.md:
        with open(a.md, "w", encoding="utf-8") as fh:
            for name, rows in md_chunks:
                fh.write("### %s\n\n" % name)
                fh.write("| 节点路径 | 类型 | 脚本 | 锚点/offset | size_flags / min | 文本 | theme_override |\n")
                fh.write("|---|---|---|---|---|---|---|\n")
                for r in rows:
                    fh.write("| `%s` | %s | %s | %s | %s | %s | %s |\n" % (
                        r["path"], r["type"] or "?",
                        (r["script"] or "").replace("res://", ""),
                        ", ".join("%s=%s" % kv for kv in r["anchors"].items()),
                        ", ".join("%s=%s" % kv for kv in r["size"].items()),
                        " / ".join(str(v) for v in r["text"].values()),
                        ", ".join("%s=%s" % kv for kv in r["theme_overrides"].items())))
                fh.write("\n")
        print("wrote %s" % a.md)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
