#!/usr/bin/env python3
"""从一个 replay 里还原"某一发出手前"的完整局面，输出给 Rust 侧的对比工具。

输出格式（纯文本，便于零依赖解析）：

    MODE NormalFunction
    SHOOTER <soldier_id> <x> <y> <team>
    FLIP <0|1>
    FUNC <函数文本>
    O <x> <y> <radius>        # 障碍（多行）
    S <id> <x> <y> <r> <team> # 士兵（多行，不含射手）
    END

用法:
    python3 tools/replay_state.py <replay> [--bot 名称] [--index N] > state.txt
"""
import argparse
import gzip
import json
import sys

BOTS = {"ChatGPT": 1, "Wolfram": 2, "Alpha Zordon": 3,
        "Multivac": 4, "Deep Blue": 5, "Deep Thought": 6}


def load(path):
    with open(path, "rb") as f:
        gz = f.read(2) == b"\x1f\x8b"
    opener = gzip.open if gz else open
    return [json.loads(l) for l in opener(path, "rt", encoding="utf-8", errors="replace") if l.strip()]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("replay")
    ap.add_argument("--bot", help="只取该机器人开的那一发")
    ap.add_argument("--index", type=int, default=0, help="取第几发（默认第一发）")
    args = ap.parse_args()

    evs = load(args.replay)
    mode = None
    names, s2p = {}, {}
    for e in evs:
        ev = e.get("event") or {}
        if "FunctionModeInfo" in ev:
            mode = ev["FunctionModeInfo"].get("function_mode")
        if "NameInfo" in ev:
            names[ev["NameInfo"]["entity_id"]] = ev["NameInfo"]["name"]
        if "SoldierInfo" in ev:
            s2p[ev["SoldierInfo"]["soldier_id"]] = ev["SoldierInfo"]["player_id"]

    # 逐 tick 维护局面
    obstacles = []          # [(x,y,r)]
    pos = {}                # soldier_id -> (x,y,r)
    team = {}               # soldier_id -> "Team1"/"Team2"
    alive = {}              # soldier_id -> bool
    pending_func = None
    last_hole = None
    shots = []

    for e in evs:
        ev = e.get("event") or {}
        tick = e.get("tick")
        if "ClearObstacles" in ev:
            obstacles = []
        if "AddObstacle" in ev:
            a = ev["AddObstacle"]
            obstacles.append((a["pos_x"], a["pos_y"], a["radius"]))
        if "DestroyObstacle" in ev:
            d = ev["DestroyObstacle"]
            obstacles.append(("HOLE", d["pos_x"], d["pos_y"], d.get("radius", 0.5)))
            last_hole = (d["pos_x"], d["pos_y"])
        if "PosInfo" in ev:
            p = ev["PosInfo"]
            pos[p["entity_id"]] = (p["pos_x"], p["pos_y"], p.get("radius", 0.5))
        if "TeamInfo" in ev:
            team[ev["TeamInfo"]["entity_id"]] = ev["TeamInfo"]["team"]
        if "LifeInfo" in ev:
            alive[ev["LifeInfo"]["entity_id"]] = ev["LifeInfo"]["alive"]
        if "FunctionInfo" in ev:
            fi = ev["FunctionInfo"]
            if fi.get("function"):
                pending_func = (fi["function"], fi["owner_id"])
        if "FunctionPoints" in ev:
            fp = ev["FunctionPoints"]
            shots.append({
                "tick": tick, "owner": fp["owner_id"],
                "start": (fp["start_x"], fp["start_y"]), "end_x": fp["end_x"],
                "func": pending_func[0] if pending_func else "",
                "end": last_hole,
                "obstacles": list(obstacles),
                "pos": dict(pos), "team": dict(team), "alive": dict(alive),
                "mode": mode,
            })
            pending_func = None

    picked = None
    if args.bot:
        for s in shots:
            nm = names.get(s2p.get(s["owner"]))
            if nm == args.bot:
                picked = s
                break
    else:
        picked = shots[args.index] if shots else None
    if not picked:
        sys.exit("没找到匹配的开火记录")

    shooter = picked["owner"]
    # FunctionPoints.start_x/start_y 是这一次开火的真实起点（比 PosInfo 可靠）
    sx, sy = picked["start"]
    pos_guess = picked["pos"].get(shooter)
    if pos_guess and (abs(pos_guess[0] - sx) > 0.01 or abs(pos_guess[1] - sy) > 0.01):
        print(f"# 注意: PosInfo=({pos_guess[0]:.2f},{pos_guess[1]:.2f}) 与 "
              f"FunctionPoints.start=({sx:.2f},{sy:.2f}) 不一致，采用后者", file=sys.stderr)
    # 方向：功能点在 x 上向左则为 flip
    flip = 1 if picked["end_x"] < sx else 0

    print(f"MODE {picked['mode']}")
    print(f"TICK {picked['tick']}")
    print(f"SHOOTER {shooter} {sx!r} {sy!r} {picked['team'].get(shooter,'?')}")
    print(f"FLIP {flip}")
    print(f"ENDX {picked['end_x']!r}")
    if picked.get("end"):
        print(f"ENDY {picked['end'][1]!r}")
    print(f"FUNC {picked['func']}")
    for o in picked["obstacles"]:
        if o[0] == "HOLE":
            continue
        print(f"O {o[0]!r} {o[1]!r} {o[2]!r}")
    for o in picked["obstacles"]:
        if o[0] == "HOLE":
            print(f"H {o[1]!r} {o[2]!r} {o[3]!r}")
    for sid, (px, py, r) in picked["pos"].items():
        if sid == shooter:
            continue
        if not picked["alive"].get(sid, True):
            continue
        print(f"S {sid} {px!r} {py!r} {r!r} {picked['team'].get(sid,'?')}")
    print("END")


if __name__ == "__main__":
    main()
