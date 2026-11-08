#!/usr/bin/env python3
"""posy_corpus.py —— 全语料上量一件事：**报文里的 `EffectInfo(Explosion).pos` 到底在不在曲线上**。

判据只用**报文自己的量** + **真二进制的求值器/解析器**（`0x4ed670` / `0x4eb9d0`）：

    取 `corrected_function` 文本 → 真解析器 → token；
    在报文的 `pos_x`（`flip=1` 的局取 `-pos_x`）上求值 → 得 `f(pos_x)`；
    与报文的 `pos_y` **逐位**比。

* `逐位相同` ⇒ 报文的 `pos` **是曲线上的点**（曲线 = 报文自己那串修正函数）；
* 不同 ⇒ 这个 `pos` **不在这条曲线上**，采样器/求值器给不出来（本轮的 Y 桶就是这一类）。

⚠ 只对 `Mode::Normal` 有意义（`NormalFunction`）；`DiffEq`（Ode）局里 `f` 是**斜率**，
   `f(pos_x)` 不是 y，脚本里单列不计入分母。

用法：
    ../.venv/bin/python tools/posy_corpus.py --dir <解压后的回放目录> --json out.jsonl
    ../.venv/bin/python tools/posy_corpus.py --dir <目录> --limit 20      # 先小跑
回放是 gzip 的，本脚本**直接读 gzip**，不用先解压。
"""

import argparse
import glob
import gzip
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from posy_unicorn import Real, bits, _ulp_diff  # noqa: E402


def read_events(path):
    op = gzip.open if open(path, "rb").read(2) == b"\x1f\x8b" else open
    with op(path, "rt", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                yield json.loads(line)["event"]
            except Exception:
                continue


def batches_of(path):
    teams, soldiers, info, effects = {}, {}, {}, []
    mode = "NormalFunction"
    out = []
    for e in read_events(path):
        if "TeamInfo" in e:
            teams[e["TeamInfo"]["entity_id"]] = e["TeamInfo"]["team"]
        elif "SoldierInfo" in e:
            soldiers[e["SoldierInfo"]["soldier_id"]] = e["SoldierInfo"]["player_id"]
        elif "FunctionModeInfo" in e:
            mode = e["FunctionModeInfo"]["function_mode"]
        elif "FunctionInfo" in e:
            fi = e["FunctionInfo"]
            info[fi["owner_id"]] = fi
        elif "FunctionPoints" in e:
            fp = e["FunctionPoints"]
            fi = info.pop(fp["owner_id"], None)
            out.append({"owner": fp["owner_id"], "corr": fi.get("corrected_function") if fi else None,
                        "mode": mode, "end_x": fp["end_x"],
                        "team": teams.get(soldiers.get(fp["owner_id"], fp["owner_id"]))})
        elif "EffectInfo" in e:
            if e["EffectInfo"]["effect_type"] == "ExplosionEffect":
                effects.append(e["EffectInfo"])
    return out, effects


def match(b, effects):
    for e in effects:
        if bits(e["pos_x"]) == bits(b["end_x"]):
            return e
    if not effects:
        return None
    return min(effects, key=lambda e: abs(e["pos_x"] - b["end_x"]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--json", default=None)
    a = ap.parse_args()
    files = sorted(p for p in glob.glob(os.path.join(a.dir, "*")) if os.path.isfile(p)
                   and not p.endswith((".md", ".tsv", ".txt", ".json")))
    if a.limit:
        files = files[:a.limit]
    R = Real()
    rows = []
    n_shot = n_normal = n_on = n_off = n_skip = 0
    hist = {}
    for path in files:
        try:
            bs, effs = batches_of(path)
        except Exception:
            continue
        for b in bs:
            n_shot += 1
            if not b["corr"]:
                n_skip += 1
                continue
            if b["mode"] != "NormalFunction":
                continue
            w = match(b, effs)
            if w is None:
                continue
            n_normal += 1
            try:
                toks = R.parse(b["corr"])
                x = -w["pos_x"] if b["team"] == "Team2" else w["pos_x"]
                fx = R.eval(toks, x, 0.0, 0.0)
            except Exception:
                n_skip += 1
                continue
            if not (fx == fx and abs(fx) != float("inf")):
                n_skip += 1
                continue
            same = bits(fx) == bits(w["pos_y"])
            if same:
                n_on += 1
            else:
                n_off += 1
                d = _ulp_diff(fx, w["pos_y"])
                hist[d] = hist.get(d, 0) + 1
            rows.append({"file": os.path.basename(path), "owner": b["owner"], "team": b["team"],
                         "corr": b["corr"], "end_x": b["end_x"], "wire_pos_x": w["pos_x"],
                         "wire_pos_y": w["pos_y"], "f_at_pos_x": fx, "on_curve": same,
                         "ulp": 0 if same else _ulp_diff(fx, w["pos_y"])})
    print("回放 %d 份；发 %d（无修正文本 %d）；Normal 且有 Explosion %d" % (len(files), n_shot, n_skip, n_normal))
    print("  pos 在曲线上（逐位）：%d / %d = %.1f%%" % (n_on, n_normal, 100.0 * n_on / max(n_normal, 1)))
    print("  pos 不在曲线上      ：%d" % n_off)
    print("  不在曲线上的 ulp 分布（f(pos_x) − pos_y）：")
    for d, c in sorted(hist.items(), key=lambda kv: -kv[1])[:12]:
        print("     %+6d ulp  ×%d" % (d, c))
    if a.json:
        with open(a.json, "w") as f:
            for r in rows:
                f.write(json.dumps(r, sort_keys=True) + "\n")
        print("  写出 %s（%d 行）" % (a.json, len(rows)))


if __name__ == "__main__":
    main()
