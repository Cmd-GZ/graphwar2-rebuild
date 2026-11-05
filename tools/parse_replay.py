#!/usr/bin/env python3
"""解析 Graphwar II 的 replay 文件（gzip + JSONL）。

replay 存在 `user://replays/<unix 时间戳>`，`user://` 在 Godot 4 里映射到
`$XDG_DATA_HOME/godot/app_userdata/<项目名>/`。

每行是一个事件：`{"event": {"<EventName>": {...}}, "tick": N}`。
"""
import collections
import gzip
import json
import sys


def load(path):
    """读出所有事件；自动处理 gzip。"""
    with open(path, "rb") as f:
        head = f.read(2)
    opener = gzip.open if head == b"\x1f\x8b" else open
    events = []
    with opener(path, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                continue
    return events


def summarize(events):
    kinds = collections.Counter()
    for e in events:
        ev = e.get("event") or {}
        for k in ev:
            kinds[k] += 1
    ticks = [e.get("tick", 0) for e in events]
    print(f"事件总数 {len(events)}，tick 范围 {min(ticks) if ticks else 0} … {max(ticks) if ticks else 0}")
    for k, v in kinds.most_common():
        print(f"  {k:28s} {v}")


def functions(events):
    """抽出 AI/玩家发出的所有函数文本。"""
    out = []
    for e in events:
        ev = e.get("event") or {}
        for name in ("FunctionFire", "FunctionInfo", "FunctionPoints"):
            if name in ev:
                payload = ev[name]
                text = payload.get("function") or payload.get("function_text")
                if text:
                    out.append((e.get("tick"), name, text,
                                payload.get("owner_id"), payload.get("soldier_id")))
    return out


def fired_functions(events):
    """每回合真正**开火时**用的那个函数。

    replay 里 `FunctionInfo` 会在玩家修改函数时不断广播（打字过程也会发），
    所以取"每个 FunctionFire 之前最后一条 FunctionInfo"才是实际发出的式子。
    """
    last = None
    out = []
    for e in events:
        ev = e.get("event") or {}
        if "FunctionInfo" in ev:
            fi = ev["FunctionInfo"]
            text = fi.get("function") or ""
            if text:
                last = (e.get("tick"), text, fi.get("owner_id"))
        elif "FunctionFire" in ev and last:
            out.append({
                "tick": last[0],
                "owner_id": last[2],
                "function": last[1],
                "soldier_id": ev["FunctionFire"].get("soldier_id"),
            })
            last = None
    return out


if __name__ == "__main__":
    import argparse
    import glob
    import os

    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+", help="replay 文件或目录")
    ap.add_argument("--functions", action="store_true", help="打印实际开火用的函数")
    ap.add_argument("--corpus", metavar="OUT", help="把各 replay 的函数汇总写成 JSON")
    args = ap.parse_args()

    files = []
    for p in args.paths:
        files.extend(sorted(glob.glob(os.path.join(p, "*"))) if os.path.isdir(p) else [p])

    corpus = {}
    for f in files:
        if not os.path.isfile(f):
            continue
        try:
            evs = load(f)
        except Exception as e:  # 跳过坏文件
            print(f"跳过 {f}: {e}", file=sys.stderr)
            continue
        shots = fired_functions(evs)
        corpus[os.path.basename(f)] = shots
        print(f"\n=== {os.path.basename(f)}：{len(evs)} 个事件、{len(shots)} 次开火 ===")
        if args.functions:
            summarize(evs)
            for s in shots:
                print(f"  tick {s['tick']:6d} owner={s['owner_id']}  {s['function'][:80]}")

    if args.corpus:
        with open(args.corpus, "w", encoding="utf-8") as f:
            json.dump(corpus, f, ensure_ascii=False, indent=1)
        n = sum(len(v) for v in corpus.values())
        print(f"\n汇总 {len(corpus)} 局、{n} 个开火函数 → {args.corpus}")
