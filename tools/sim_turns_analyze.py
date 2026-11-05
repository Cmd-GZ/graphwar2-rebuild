#!/usr/bin/env python3
"""同时回合制（SimultaneousTurns）击杀结算顺序的分析流水线。

做三件事：

1. 把 replay 按**回合**切开（`GameStateInfo = WaitingForFunctions` 是回合边界；
   老 replay 里同一回合的多个 `FunctionPoints` 可能相差十几 tick，按 tick 切会切错）；
2. 每回合还原局面（障碍 / 洞 / 存活士兵 / 各发的函数与起点），写成 state 文件，
   交给 `examples/sim_turns.rs` 用**已验证的物理**逐发算弹道与命中；
3. 把结果与 replay 里观测到的死亡集合 / 死亡时刻对照，输出：
   * 逐回合 观测死亡 vs 我们并集（多报 / 漏报）
   * 「被 >=2 发穿过」的士兵：最早到达 vs FunctionPoints 顺序，谁的解释更接近观测时刻
   * 偏序约束（胜者必须先于其他命中者）是否有环 ⇒ 是否存在任何固定串行顺序

用法：
    python3 tools/sim_turns_analyze.py --newest 5
    python3 tools/sim_turns_analyze.py 1793884817 1793884702 ...
    python3 tools/sim_turns_analyze.py --newest 5 --quiet

state 文件格式见 `examples/sim_turns.rs` 的头注释。
"""
import argparse
import collections
import gzip
import json
import os
import subprocess
import sys

REPLAY_DIR = os.path.expanduser("~/.local/share/godot/app_userdata/Graphwar/replays")
DEFAULT_EXE = os.path.join(os.path.dirname(__file__), "..", "..",
                           ".cargo-home/target/release/examples/sim_turns")


def load(path):
    with open(path, "rb") as f:
        gz = f.read(2) == b"\x1f\x8b"
    op = gzip.open if gz else open
    return [json.loads(l) for l in op(path, "rt", encoding="utf-8", errors="replace") if l.strip()]


def build(path):
    """把 replay 解析成「回合 -> 各发局面快照」+ 死亡时刻表。"""
    evs = load(path)
    pos, team, s2p, alive = {}, {}, {}, {}
    obstacles, holes = [], []
    funcs, corr = {}, {}
    mode, turn_mode = "NormalFunction", None
    rounds, cur = [], None
    deaths, death_msgs = collections.OrderedDict(), []
    effects, active_list = [], []

    for e in evs:
        t = e.get("tick", 0)
        ev = e.get("event") or {}
        if "FunctionModeInfo" in ev:
            mode = ev["FunctionModeInfo"].get("function_mode")
        elif "TurnModeInfo" in ev:
            turn_mode = ev["TurnModeInfo"].get("turn_mode")
        elif "ClearObstacles" in ev:
            obstacles, holes = [], []
        elif "AddObstacle" in ev:
            a = ev["AddObstacle"]
            obstacles.append((a["pos_x"], a["pos_y"], a["radius"]))
        elif "DestroyObstacle" in ev:
            d = ev["DestroyObstacle"]
            holes.append((d["pos_x"], d["pos_y"], d.get("radius", 0.5)))
        elif "PosInfo" in ev:
            p = ev["PosInfo"]
            pos[p["entity_id"]] = (p["pos_x"], p["pos_y"], p.get("radius", 0.5))
        elif "TeamInfo" in ev:
            team[ev["TeamInfo"]["entity_id"]] = ev["TeamInfo"]["team"]
        elif "SoldierInfo" in ev:
            s2p[ev["SoldierInfo"]["soldier_id"]] = ev["SoldierInfo"]["player_id"]
        elif "LifeInfo" in ev:
            li = ev["LifeInfo"]
            eid = li["entity_id"]
            if li["alive"]:
                alive[eid] = True
            else:
                # 只有 "活着 -> 死" 才算一次死亡：
                #  重复播报的第二条不计；开局 Sync 里本来就写 false 的（无前置状态）也不计
                if alive.get(eid) is True:
                    deaths.setdefault(eid, t)
                alive[eid] = False
                death_msgs.append((t, eid))
        elif "FunctionInfo" in ev:
            fi = ev["FunctionInfo"]
            if fi.get("function"):
                funcs[fi["owner_id"]] = fi["function"]
            if fi.get("corrected_function"):
                corr[fi["owner_id"]] = fi["corrected_function"]
        elif "FunctionActive" in ev:
            fa = ev["FunctionActive"]
            active_list.append((fa["active_tick"], fa["owner_id"]))
        elif "EffectInfo" in ev:
            ei = ev["EffectInfo"]
            effects.append((ei.get("start_tick", t), ei["effect_type"], ei["pos_x"], ei["pos_y"]))
        elif "GameStateInfo" in ev:
            if ev["GameStateInfo"]["game_state"] == "WaitingForFunctions":
                cur = {"start": t, "shots": []}
                rounds.append(cur)
        elif "FunctionPoints" in ev:
            fp = ev["FunctionPoints"]
            if cur is None:
                cur = {"start": t, "shots": []}
                rounds.append(cur)
            cur["shots"].append(dict(
                tick=t, owner=fp["owner_id"], sx=fp["start_x"], sy=fp["start_y"],
                end_x=fp["end_x"], func=funcs.get(fp["owner_id"], ""),
                corr=corr.get(fp["owner_id"]),
                obstacles=list(obstacles), holes=list(holes), alive=dict(alive)))

    return dict(pos=pos, team=team, s2p=s2p, alive=alive, mode=mode, turn_mode=turn_mode,
                rounds=rounds, deaths=deaths, death_msgs=death_msgs,
                effects=effects, active_list=active_list)


def write_state(path, d, shots):
    alive = shots[0]["alive"]
    lines = ["MODE {}".format(d["mode"]), "STEP 0.5"]
    for i, s in enumerate(shots):
        flip = 1 if s["end_x"] < s["sx"] else 0
        s["flip"] = flip
        lines.append("SHOT {} {} {} {} {} {}".format(i, s["owner"], repr(s["sx"]),
                                                     repr(s["sy"]), flip, s["func"]))
    for sid, (x, y, r) in sorted(d["pos"].items()):
        if not alive.get(sid, True):
            continue
        lines.append("T {} {} {} {}".format(sid, repr(x), repr(y), repr(r)))
    for (x, y, r) in shots[0]["obstacles"]:
        lines.append("O {} {} {}".format(repr(x), repr(y), repr(r)))
    for (x, y, r) in shots[0]["holes"]:
        lines.append("H {} {} {}".format(repr(x), repr(y), repr(r)))
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")


def run_state(exe, path):
    out = subprocess.run([exe, path], capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit("sim_turns failed: " + out.stderr)
    shots, hits = {}, {}
    for line in out.stdout.splitlines():
        p = line.split()
        if p[0] == "SHOT":
            shots[int(p[1])] = dict(owner=int(p[2]), arclen=float(p[6]), stop=float(p[7]))
            hits[int(p[1])] = []
        elif p[0] == "HITC":          # 连续（折线段）命中；点采样另见 HITP
            hits[int(p[1])].append(dict(tid=int(p[2]), arc=float(p[4])))
    return shots, hits


def assign_booms(group, shots, active, effects):
    """爆炸事件 <-> 各发，一对一分配（|Δx| <= 0.05 且隐含速度合理）。"""
    cands = [(t, x, y) for (t, k, x, y) in effects if k == "ExplosionEffect"]
    pairs = sorted((abs(x - group["shots"][i]["end_x"]), i, j)
                   for i in shots for j, (t, x, y) in enumerate(cands))
    used_i, used_j, out = set(), set(), {}
    for cost, i, j in pairs:
        if i in used_i or j in used_j or cost > 0.05:
            continue
        t, x, y = cands[j]
        dt = t - active
        if dt <= 0 or shots[i]["arclen"] <= 0:
            continue
        if not (0.15 < shots[i]["arclen"] / dt < 0.45):
            continue
        used_i.add(i); used_j.add(j)
        out[i] = cands[j]
    return out


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("replays", nargs="*", help="replay 文件名（默认按 mtime 取最新）")
    ap.add_argument("--newest", type=int, default=5)
    ap.add_argument("--workdir", default=".sim-work", help="临时 state 文件目录")
    ap.add_argument("--exe", default=DEFAULT_EXE)
    ap.add_argument("--quiet", action="store_true", help="只打汇总，不逐发/逐人")
    a = ap.parse_args(argv)

    reps = a.replays
    if not reps:
        allf = sorted(os.listdir(REPLAY_DIR),
                      key=lambda f: os.path.getmtime(os.path.join(REPLAY_DIR, f)), reverse=True)
        reps = allf[:a.newest]
    exe = os.path.abspath(a.exe)
    os.makedirs(a.workdir, exist_ok=True)

    tot = collections.Counter()
    cases = []
    for rep in reps:
        d = build(os.path.join(REPLAY_DIR, rep))
        starts = [r["start"] for r in d["rounds"]]
        print("=" * 96)
        print("# {}  turn_mode={}  function_mode={}  回合={}  发={}  死亡(去重)={}".format(
            rep, d["turn_mode"], d["mode"], len(d["rounds"]),
            sum(len(r["shots"]) for r in d["rounds"]), len(d["deaths"])))
        for gi, r in enumerate(d["rounds"]):
            shots_raw = r["shots"]
            if not shots_raw:
                continue
            state = os.path.join(a.workdir, "{}_{}.state".format(rep, r["start"]))
            write_state(state, d, shots_raw)
            shots, hits = run_state(exe, state)
            nxt = starts[gi + 1] if gi + 1 < len(starts) else 10 ** 9
            obs = {e: t for e, t in d["deaths"].items() if r["start"] <= t < nxt}
            eff = [(t, k, x, y) for (t, k, x, y) in d["effects"] if r["start"] <= t < nxt]
            act = sorted(set(x for x, o in d["active_list"] if r["start"] <= x < nxt))
            active = act[0] if act else r["start"]
            union = {}
            for i in hits:
                for h in hits[i]:
                    union.setdefault(h["tid"], []).append((i, h["arc"]))
            extra = sorted(set(union) - set(obs))
            miss = sorted(set(obs) - set(union))
            booms = assign_booms(r, shots, active, eff)
            vg = [shots[i]["arclen"] / (booms[i][0] - active) for i in booms]
            vdef = sorted(vg)[len(vg) // 2] if vg else 0.28

            def arrive(i, arc):
                b = booms.get(i)
                if b:
                    return active + arc * (b[0] - active) / shots[i]["arclen"]
                return active + arc / vdef

            print("\n回合{} 起 tick={} {}发 active={} 观测死亡{} 我们并集{} 多报{} 漏报{}".format(
                gi, r["start"], len(shots), active, len(obs), len(union), len(extra), len(miss)))
            if extra:
                print("   多报: {}".format(extra))
            if miss:
                print("   漏报: {}".format(miss))
            if not a.quiet:
                for i, s in shots.items():
                    sh = shots_raw[i]
                    print("   发{} 射手{:6d} 弧长{:7.2f} 停x={:8.3f} 游戏end_x={:8.3f} Δ={:6.3f} "
                          "boom={} 命中={}".format(
                              i, sh["owner"], s["arclen"], s["stop"], sh["end_x"],
                              abs(s["stop"] - sh["end_x"]),
                              booms[i][0] if i in booms else None, [h["tid"] for h in hits[i]]))
            for tid, lst in sorted(union.items()):
                if len(lst) < 2:
                    continue
                tot["multi"] += 1
                fa = min(lst, key=lambda z: z[1])      # 最早到达
                fi = min(lst, key=lambda z: z[0])      # FunctionPoints 顺序优先
                line = "   * {} 被{}发穿过 {}".format(
                    tid, len(lst), ", ".join("发{}@{:.1f}".format(i, x) for i, x in lst))
                if tid in obs:
                    pe, po = arrive(*fa), arrive(*fi)
                    ee, eo = abs(pe - obs[tid]), abs(po - obs[tid])
                    if abs(pe - po) > 5:
                        tot["disc"] += 1
                        tot["earliest" if ee < eo else ("order" if eo < ee else "tie")] += 1
                        cases.append(dict(rep=rep, round=gi, tid=tid, n=len(lst),
                                          obs=obs[tid], p_e=round(pe), p_o=round(po),
                                          e_e=round(ee, 1), e_o=round(eo, 1),
                                          winner=("earliest" if ee < eo else
                                                  ("order" if eo < ee else "tie"))))
                    line += " -> 观测 {} | 最早到达(发{}) {:.0f} Δ{:.0f} | 顺序优先(发{}) {:.0f} Δ{:.0f}{}".format(
                        obs[tid], fa[0], pe, ee, fi[0], po, eo,
                        "  [可判别]" if abs(pe - po) > 5 else "  [不可判别]")
                else:
                    line += " -> 实际未死"
                if not a.quiet:
                    print(line)
            tot["shots"] += len(shots)
            tot["rounds"] += 1
            tot["deaths"] += len(obs)
            tot["unions"] += len(union)
            tot["extra"] += len(extra)
            tot["miss"] += len(miss)

    print("\n" + "=" * 96)
    print("合计: {} 局 / {} 回合 / {} 发 / 观测死亡 {} / 我们并集 {} / 多报 {} / 漏报 {}".format(
        len(reps), tot["rounds"], tot["shots"], tot["deaths"], tot["unions"],
        tot["extra"], tot["miss"]))
    print("被 >=2 发穿过的士兵 {} 人；可判别（两假设预测差 >5 tick）{} 例；"
          "「最早到达」对 {}，「FunctionPoints 顺序优先」对 {}，平 {}".format(
              tot["multi"], tot["disc"], tot["earliest"], tot["order"], tot["tie"]))
    return cases


if __name__ == "__main__":
    main()
