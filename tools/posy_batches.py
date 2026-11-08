#!/usr/bin/env python3
"""posy_batches.py —— 一份回放里**每一发**：真二进制算出来的末点 vs 报文里记的 `pos_*`。

`EffectInfo.pos_*` 的写入指令只有一条 16 字节 `movdqu`（`0x4729a0` / `0x472e14`，见
`tools/effect_info_pos.py` 与 `docs/06` §35），源就是点数组的 `pts[len-1]`。所以比
"真二进制末点 vs 报文 pos" 就等于比"数组末点 vs 报文"。

每发做四件事：
  ① 用**真解析器**（`0x4ed670`）把回放自己的 `corrected_function` 变成 token；
  ② 用**真采样器**（`0x3c2530`）跑出整条点数组（`step=0.01`；`flip` 按队伍取，
     `mode` 按 `FunctionModeInfo`）；
  ③ 比 `末点 x` vs `end_x`、`末点 y` vs `EffectInfo(Explosion).pos_y`（**位** + ulp 差）；
  ④ 比 `f(报文 pos_x)` vs 报文 `pos_y`（真求值器 `0x4eb9d0`）——这一条**不需要任何
     我方实现参与**，只用报文自己的 `corrected_function` 和 `pos_x`。

用法：
    ../.venv/bin/python tools/posy_batches.py --replay R.json [--limit N] [--json out.jsonl]
"""

import argparse
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from posy_unicorn import Real, bits, _ulp_diff, STEP  # noqa: E402


def load_all(path):
    """把 JSONL 顺序读一遍，返回 (batches, teams, soldiers, effects)。

    `FunctionInfo(corrected_function != null)` 与 `FunctionPoints` 在**同时回合**里是
    两批事件（先 N 条 FunctionInfo，再 N 条 FunctionPoints），所以按 `owner_id` 配对。
    """
    teams, soldiers, effects = {}, {}, []
    info, batches = {}, []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            e = json.loads(line)["event"]
            if "TeamInfo" in e:
                teams[e["TeamInfo"]["entity_id"]] = e["TeamInfo"]["team"]
            elif "SoldierInfo" in e:
                soldiers[e["SoldierInfo"]["soldier_id"]] = e["SoldierInfo"]["player_id"]
            elif "FunctionModeInfo" in e:
                _SET["mode"] = e["FunctionModeInfo"]["function_mode"]
            elif "FunctionInfo" in e:
                fi = e["FunctionInfo"]
                info[fi["owner_id"]] = fi
            elif "FunctionPoints" in e:
                fp = e["FunctionPoints"]
                fi = info.pop(fp["owner_id"], None)
                batches.append({
                    "owner": fp["owner_id"],
                    "func": fi["function"] if fi else None,
                    "corr": fi["corrected_function"] if fi else None,
                    "mode": _SET.get("mode", "NormalFunction"),
                    "start_x": fp["start_x"], "start_y": fp["start_y"], "end_x": fp["end_x"],
                })
            elif "EffectInfo" in e:
                effects.append(e["EffectInfo"])
    return batches, teams, soldiers, effects


def match_effects(B, effects):
    """把 ExplosionEffect 一对一配到每一发（**贪心 + 唯一占用**）。

    优先级：① `pos_x` 与 `end_x` 逐位相同；② 剩下的按 |pos_x − end_x| 从小到大配。
    这样避免"两发抢同一个 effect"（同时回合里一 tick 有 N 个 ExplosionEffect）。
    """
    ex = [e for e in effects if e["effect_type"] == "ExplosionEffect"]
    used = set()
    out = [None] * len(B)
    for i, b in enumerate(B):
        for j, e in enumerate(ex):
            if j in used:
                continue
            if bits(e["pos_x"]) == bits(b["end_x"]):
                out[i] = (e, "pos_x==end_x")
                used.add(j)
                break
    rest = [i for i in range(len(B)) if out[i] is None]
    for i in rest:
        cand = [(abs(ex[j]["pos_x"] - B[i]["end_x"]), j) for j in range(len(ex)) if j not in used]
        if not cand:
            break
        _, j = min(cand)
        out[i] = (ex[j], "按最近 pos_x 配")
        used.add(j)
    return out


_SET = {}


def team_of(owner, teams, soldiers):
    p = soldiers.get(owner, owner)
    return teams.get(p) or teams.get(owner)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--replay", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--json", default=None)
    ap.add_argument("--flip", type=int, default=-1, help="-1 = 按队伍推")
    a = ap.parse_args()
    B, teams, soldiers, effects = load_all(a.replay)
    R = Real()
    matched = match_effects(B, effects)
    out = []
    print("%-3s %-6s %-5s %-5s %8s %-16s %-16s %-16s" %
          ("批", "owner", "队伍", "flip", "点数", "末点 x vs end_x", "末点 y vs pos_y", "f(pos_x) vs pos_y"))
    for i, b in enumerate(B):
        if a.limit and i >= a.limit:
            break
        team = team_of(b["owner"], teams, soldiers)
        flip = a.flip if a.flip >= 0 else (0 if team == "Team1" else 1)
        mode = 0 if b["mode"] == "NormalFunction" else 1
        we, how = matched[i]
        expl = [we] if we else []
        row = {"batch": i, "owner": b["owner"], "team": team, "flip": flip, "mode": b["mode"],
               "corr": b["corr"], "start_x": b["start_x"], "start_y": b["start_y"],
               "end_x": b["end_x"]}
        if b["corr"]:
            toks = R.parse(b["corr"])
            pts = R.path(toks, b["start_x"], b["start_y"], STEP, flip, mode)
            row.update(n=len(pts), last_x=pts[-1][0], last_y=pts[-1][1])
            dx = "逐位相同" if bits(pts[-1][0]) == bits(b["end_x"]) else \
                 "%+d ulp" % _ulp_diff(pts[-1][0], b["end_x"])
            if expl:
                w = expl[0]
                row.update(wire_pos_x=w["pos_x"], wire_pos_y=w["pos_y"], effect_id=w["effect_id"])
                dy = "逐位相同" if bits(pts[-1][1]) == bits(w["pos_y"]) else \
                     "%+d ulp" % _ulp_diff(pts[-1][1], w["pos_y"])
                # ④ 只用报文自己：f(pos_x) vs pos_y
                fx = R.eval(toks, -w["pos_x"] if flip else w["pos_x"], 0.0, 0.0)
                row["f_at_pos_x"] = fx
                if not (fx == fx and abs(fx) != float("inf")):
                    df = "（f 非有限）"
                else:
                    df = "逐位相同" if bits(fx) == bits(w["pos_y"]) else "%+d ulp" % _ulp_diff(fx, w["pos_y"])
                if bits(w["pos_x"]) != bits(b["end_x"]):
                    df += "（pos_x≠end_x）"
            else:
                dy = df = "（无 Explosion）"
            print("%-3d %-6s %-5s %-5d %8d %-16s %-16s %-16s"
                  % (i, b["owner"], (team or "?")[-1], flip, len(pts), dx, dy, df))
        else:
            print("%-3d %-6s %-5s %-5d %8s %-16s %-16s %-16s"
                  % (i, b["owner"], (team or "?")[-1], flip, "-", "-", "-", "-"))
        out.append(row)
    if a.json:
        with open(a.json, "w") as f:
            for r in out:
                f.write(json.dumps(r, sort_keys=True) + "\n")
        print("\n写出 %s（%d 行）" % (a.json, len(out)))


if __name__ == "__main__":
    main()
