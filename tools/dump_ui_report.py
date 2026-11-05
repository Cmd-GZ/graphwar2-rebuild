#!/usr/bin/env python3
"""Turn dump_ui.sh logs into a compact, reviewable control inventory.

Why: the raw dumps are ~850 nodes per snapshot (Godot auto-names hundreds of
runtime-generated children as @HBoxContainer@32 etc.). Reading them by eye does not
scale, and "which widgets exist on screen X" is exactly the question docs/92 answers.

Usage:
    tools/dump_ui_report.py [--out DIR] [log ...]
Defaults to every .rea-work/ui/*.log produced by tools/dump_ui.sh.

Prints, per snapshot, one TSV-ish line per *control* node:
    screen <TAB> path-relative-to-screen <TAB> class <TAB> text <TAB> script <TAB> flags
plus a node/control count per snapshot so coverage is auditable.
"""
import argparse
import glob
import os
import re
import sys

# Widgets that carry UI meaning; plain layout containers are counted, not listed.
CONTROL_CLASSES = {
    "GraphwarButton", "Button", "Label", "RichTextLabel", "LineEdit", "TextEdit",
    "OptionButton", "CheckBox", "CheckButton", "TabContainer", "TabBar", "ItemList",
    "TextureButton", "TextureRect", "PlayerShowcase", "GraphwarFunctionInput",
    "GraphwarSetupControl", "GraphwarPlane", "ProgressBar", "HSlider", "VSlider",
    "VideoStreamPlayer", "GraphwarRootNode",
}
AUTO_NAME = re.compile(r"^@?[A-Za-z0-9_]+@\d+$")


def parse(path):
    """Yield (snapshot_label, rows) where rows are node dicts, in file order."""
    label = None
    rows = []
    conns = {}
    for line in open(path, encoding="utf-8", errors="replace"):
        if not line.startswith("@@"):
            continue
        p = line.rstrip("\n").split("|")
        tag = p[0]
        if tag == "@@N":
            _, lab, path_, cls, vis, script, text, flags = p
            rows.append(dict(label=lab, path=path_, cls=cls, vis=vis, script=script,
                             text=text, flags=flags))
        elif tag == "@@C":
            _, lab, path_, sig, meth = p
            conns.setdefault((lab, path_), []).append(f"{sig}->{meth}")
        elif tag == "@@DUMP":
            if label is not None:
                yield label, rows, conns
            label, rows, conns = p[1], [], {}
        elif tag == "@@END":
            if label is not None:
                yield label, rows, conns
            label, rows, conns = None, [], {}
    if label is not None:
        yield label, rows, conns


def screen_of(path):
    m = re.search(r"ScreensControl/([^/]+)", path)
    return m.group(1) if m else "<root>"


def rel(path):
    m = re.search(r"ScreensControl/(.*)$", path)
    return m.group(1) if m else path.split("/", 1)[-1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("logs", nargs="*")
    a = ap.parse_args()

    ws = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    logs = a.logs or sorted(glob.glob(os.path.join(ws, ".rea-work", "ui", "*.log")))
    if not logs:
        sys.exit("no logs; run tools/dump_ui.sh first")

    outdir = a.out or os.path.join(os.path.dirname(logs[0]), "summary")
    os.makedirs(outdir, exist_ok=True)
    index = []
    for log in logs:
        if os.path.basename(log).startswith("_"):
            continue
        seen = set()
        for label, rows, conns in parse(log):
            if label in seen:
                continue
            seen.add(label)
            lines = []
            nctl = 0
            for r in rows:
                s = screen_of(r["path"])
                if r["cls"] not in CONTROL_CLASSES:
                    continue
                nctl += 1
                c = conns.get((label, r["path"]))
                lines.append("\t".join([
                    s, rel(r["path"]), r["cls"], r["text"], r["script"], r["flags"],
                    ";".join(c) if c else "-",
                ]))
            out = os.path.join(outdir, f"{os.path.basename(log)[:-4]}__{label}.tsv")
            with open(out, "w", encoding="utf-8") as fh:
                fh.write("screen\tpath\tclass\ttext\tscript\tflags\tconnections\n")
                fh.write("\n".join(lines) + "\n")
            index.append((os.path.basename(log), label, len(rows), nctl, out))
    print(f"{'log':<18} {'snapshot':<26} {'nodes':>6} {'controls':>8}  file")
    for log, label, n, nctl, out in index:
        print(f"{log:<18} {label:<26} {n:>6} {nctl:>8}  {os.path.basename(out)}")
    print(f"\ntotal snapshots: {len(index)}")


if __name__ == "__main__":
    main()
