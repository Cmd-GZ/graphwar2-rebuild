#!/usr/bin/env python3
"""整局对拍**剩余失败**的归因分桶（`docs/06-verification.md` §24 的那张表就是本脚本的输出）。

输入是 `examples/replay_attrib.rs` 吐的逐批 JSONL（**每一批一行**），
输出是一张**互斥、可加总**的表：各行批数之和 == 失败批数。

为什么分桶不写在 Rust 里：分桶规则要能随时改而不重新编译；
而且规则本身是**结论**，写在文档 §24 里、由本脚本执行，两者对得上才算数。

## 用法

```bash
export CARGO_HOME=... CARGO_TARGET_DIR=$CARGO_HOME/target
R=.reference/replays/gw2-replays-snapshot/replays
# 1) 任务书基线（旧驱动：每发一次 Match::fire，默认口径）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive fire --policy default --json /tmp/fire.jsonl
python3 tools/bit_exact_attrib.py --json /tmp/fire.jsonl --check-baseline --axis fire --snapshot $R

# 2) 收敛层（模式感知 + 修正文本 + 爆炸点回代 + 皮肤特效，严格口径）
cargo run --release --offline --example replay_attrib -- \
    --dir $R --drive conv --policy strict --json /tmp/conv.jsonl
python3 tools/bit_exact_attrib.py --json /tmp/conv.jsonl
```

`--check-baseline` 会**先比语料指纹、再比数字**：语料（快照目录的文件名集合与逐文件
sha256）登记在 `fixtures/guardrails.json` 的 `corpus` 里，与 `--snapshot` 指向的目录比对，
不一致就打印**多了/少了/内容变了哪些文件**并非 0。语料一变，三轴数字全部不可比
（401→403 那次的 647→654 就是这么来的）⇒ **新样本另开目录，不许混进快照**。
真要换语料：`--update-corpus --snapshot <目录>` 重登记，并在 `docs/06` 写明。

## 分桶规则（**按优先级取第一个命中**，所以互斥）

| 序 | 桶 | 判据 |
|---|---|---|
| 1 | `C` corrected_function 文本 | 首个 diff 的路径是 `*.corrected_function` |
| 2 | `D` 死效块（死亡集合多/少） | 批内**消息种类计数**只差 `EffectInfo/Death|Ash|Spark`（±），不含 `GameStateInfo` |
| 3 | `S` 多发批结构 | ≥2 发，且种类计数只差多出来的 `GameStateInfo`（每发一个窗口 ⇒ +3/发） |
| 4 | `SD` 多发批结构 + 死效块 | ≥2 发，且种类计数同时差 `GameStateInfo`(+) 与死效块(±) |
| 5 | `E1` `end_x` 差一个采样点 | `end_x` 差 ≤ 0.02 且 `pos_x` 与回放逐位/1e-9 内相同 |
| 6 | `E2` `end_x` 真分歧 | 其余 `end_x` 差 |
| 7 | `X` 爆炸点 x（`pos_x ≠ end_x`） | 无 `end_x` 差、有 `pos_x` 差（全部 ≤1e-9 量级） |
| 8 | `Y` 爆炸点 y 末位差 | 无 `end_x`/`pos_x` 差、`pos_y` 差 ≤ 1e-9 |
| 9 | `Y2` 爆炸点 y 中等差 | 同上但 1e-9 < 差 ≤ 1e-3 |
| 10 | `K` 其它（未定性） | 以上全不命中 |

`独占` 列 = **把这一类修好、别的一动不动就会变干净的批数**
（= 该批的 diff 全部落在本类的字段上）。它才是"预计能加多少批"。

## 延迟段（死亡集合 / 挖洞点）是**另一根轴**

它不属于上面那张表：分母是"有死亡/挖洞可比的批"（379/259），不是全部结算批。
本脚本按 `replaydiff::run_file` 的**逐文件**去重规则复算它，用来核对任务书的
`211/379`、`1/259` 两个数。
"""

import argparse
import collections
import hashlib
import json
import os
import sys

DEATH_EFFECTS = ("EffectInfo/DeathEffect", "EffectInfo/AshEffect", "EffectInfo/SparkEffect")
E1_MAX_DELTA = 0.02   # 一个采样步的量级上界（step=0.01，实测最大 0.009375）


def load(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            if r.get("batch", -1) >= 0:
                rows.append(r)
    rows.sort(key=lambda r: (r["file"], r["batch"]))
    return rows


def num(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def get_diffs(r, field):
    return [d for d in r["diffs"] if d["path"].split(".")[-1] == field]


def delta(d):
    a, b = num(d["ours"]), num(d["observed"])
    return abs(a - b) if a is not None and b is not None else None


def kind_deltas(r):
    """批内消息种类计数的差（我方 − 回放）。"""
    a = collections.Counter(r["our_kinds"])
    b = collections.Counter(r["obs_kinds"])
    return {k: a.get(k, 0) - b.get(k, 0) for k in set(a) | set(b) if a.get(k, 0) != b.get(k, 0)}


def classify(r):
    """返回 (桶, 是否独占)。规则见表头，按优先级。"""
    if r["content_ok"]:
        return ("PASS", False)
    dl = kind_deltas(r)
    only_state = dl and all(k == "GameStateInfo" and v > 0 for k, v in dl.items())
    only_death = dl and all(k in DEATH_EFFECTS for k in dl)
    state_and_death = dl and any(k in DEATH_EFFECTS for k in dl) and any(
        k == "GameStateInfo" for k in dl
    )
    multi = r["shots"] > 1

    if not r["kind_seq_ok"]:
        if only_death:
            return ("D", True)
        if multi and only_state:
            return ("S", r["loo"]["state"])
        if multi and state_and_death:
            return ("SD", False)
        if multi:
            return ("S", False)
        return ("K", not r["diffs"])

    cf = get_diffs(r, "corrected_function")
    if cf:
        rest = [d for d in r["diffs"] if d["path"].split(".")[-1] != "corrected_function"]
        return ("C", not rest)

    ex, px, py = get_diffs(r, "end_x"), get_diffs(r, "pos_x"), get_diffs(r, "pos_y")
    if ex:
        de = max((delta(d) or 0.0) for d in ex)
        px_tight = (not px) or max((delta(d) or 0.0) for d in px) <= 1e-9
        if de <= E1_MAX_DELTA and px_tight:
            return ("E1", not px and not py)
        return ("E2", not px and not py and de > E1_MAX_DELTA)
    if px:
        return ("X", not py)
    if py:
        dp = max((delta(d) or 0.0) for d in py)
        if dp <= 1e-9:
            return ("Y", True)
        return ("Y2", True)
    return ("K", False)


NAME = {
    "PASS": "完全一致",
    "C": "corrected_function 文本",
    "D": "死效块数量（死亡集合多/少）",
    "S": "多发批结构（**fire 驱动的口径产物**：batch 轴 128/130 已 OK）",
    "SD": "多发批结构 + 死效块",
    "E1": "end_x 小差（≤0.02）且爆炸点与我们一致 —— 含 Ode 局几何差，勿按字面理解",
    "E2": "end_x 真分歧（几何）",
    "X": "爆炸点 x 末位差（**回放自身 pos_x ≠ 其 end_x**，见 docs/06 §39）",
    "Y": "爆炸点 y 末位差（≤1e-9；求值器已逐位排除，见 §35/§36）",
    "Y2": "爆炸点 y 中等差（3 位小数常数回代）",
    "K": "其它（未定性）",
}


def delayed_stats(rows):
    """按 `run_file` 的口径复算延迟段：死亡集合 / 挖洞点。`attributed` **每份回放**重置。"""
    attributed = set()
    cur = None
    d = collections.Counter()
    for r in rows:
        if r["file"] != cur:
            attributed = set()
            cur = r["file"]
        obs = [i for i in r["obs_deaths"] if i not in attributed]
        for i in obs:
            attributed.add(i)
        ours = sorted(r["killed"])
        if obs or ours:
            d["deaths_checked"] += 1
            if ours == sorted(obs):
                d["deaths_ok"] += 1
            elif len(ours) > len(obs):
                d["deaths_extra"] += 1
            elif len(ours) < len(obs):
                d["deaths_missing"] += 1
            else:
                d["deaths_wrong"] += 1
        if r["blocked"] is not None and r["obs_destroy"]:
            d["destroy_checked"] += 1
            ours_pt = (repr(r["blocked"][0]), repr(r["blocked"][1]))
            if ours_pt in {(repr(x), repr(y)) for x, y in r["obs_destroy"]}:
                d["destroy_ok"] += 1
    return d


def pct(a, b):
    return f"{100.0 * a / b:.1f}%" if b else "-"


# ---------------------------------------------------------------- 语料指纹（冻结纪律）

GUARDRAILS = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "fixtures",
                                          "guardrails.json"))


def corpus_scan(snapshot):
    """扫快照目录，返回 ``(files, names_sha256, digest_sha256, replays)``。

    * ``files``：``{文件名: 内容 sha256}``（只看目录里的普通文件，**不递归**）
    * ``names_sha256``：**文件名集合**的指纹 = ``sha256("\\n".join(sorted(names)) + "\\n")``
    * ``digest_sha256``：名字与内容一起的指纹 = ``sha256(每行 "<sha>  <name>" 按名字排序 + "\\n")``
    * ``replays``：名字是纯数字的文件数（快照里的回放都是 epoch 时间戳名）

    只记**文件名**（不记绝对路径），所以快照搬到哪台机器都能复算；内容 sha256 是为了
    连"原地改一份回放"也拦住 —— 只比文件名集合的话，覆盖同名的样本会静默溜过去。
    """
    names = sorted(n for n in os.listdir(snapshot)
                   if os.path.isfile(os.path.join(snapshot, n)))
    files = {}
    for name in names:
        h = hashlib.sha256()
        with open(os.path.join(snapshot, name), "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        files[name] = h.hexdigest()
    names_blob = "\n".join(names) + "\n" if names else ""
    lines = ["%s  %s" % (files[n], n) for n in names]
    digest_blob = "\n".join(lines) + "\n" if lines else ""
    replays = sum(1 for n in names if n.isdigit())
    return (files,
            hashlib.sha256(names_blob.encode("utf-8")).hexdigest(),
            hashlib.sha256(digest_blob.encode("utf-8")).hexdigest(),
            replays)


def corpus_record(snapshot):
    """``--update-corpus`` 写进 guardrails.json 的那一块（纯文本、可 diff、人也能读）。"""
    files, names_sha, digest, replays = corpus_scan(snapshot)
    dir_hint = "/".join(os.path.abspath(snapshot).split(os.sep)[-2:])
    return {
        "what": "护栏语料（冻结快照）的指纹。--check-baseline 先比这里，再比三轴数字。",
        "note": [
            "语料一变，三轴数字**全部不可比**（401→403 那份让 647 变 654 就是这么来的）。",
            "新样本另开目录（如 .agent-work/replay-new/），**不许**拷进快照目录。",
            "改这里 = 承认换了语料：必须 --update-corpus 重登记，并在 docs/06 与提交信息里写明多了/少了哪些文件。",
            "files 每行是 '<sha256>  <文件名>'（与 sha256sum 输出同格式，按文件名排序）；只记文件名不记路径。",
            "names_sha256 只覆盖文件名集合，digest_sha256 连内容一起覆盖。",
        ],
        "dir_hint": dir_hint,
        "count": len(files),
        "replays": replays,
        "names_sha256": names_sha,
        "digest_sha256": digest,
        "files": ["%s  %s" % (files[n], n) for n in sorted(files)],
    }


def check_corpus(g, snapshot):
    """比对快照目录与 ``guardrails.json`` 的 ``corpus``，返回**不一致项数**（0 = 一致）。

    报错要能直接回答"多了/少了哪几个文件"，否则人只会看见 7 项数字不一致。
    """
    want = g.get("corpus") or {}
    if not os.path.isdir(snapshot):
        print(f"  ❗ 快照目录不存在：{snapshot}")
        return 1
    files, names_sha, digest, replays = corpus_scan(snapshot)
    print(f"  快照：{snapshot}")
    if not want.get("files"):
        print("  ❗ guardrails.json 里没有 corpus 指纹（老文件）—— 先跑 --update-corpus 登记")
        return 1
    exp = {}
    for line in want["files"]:
        sha, _sep, name = line.partition("  ")
        if name.strip():
            exp[name.strip()] = sha.strip()
    added = sorted(set(files) - set(exp))
    missing = sorted(set(exp) - set(files))
    changed = sorted(n for n in set(exp) & set(files) if exp[n] != files[n])

    bad = 0
    def _row(label, got, want_s):
        nonlocal bad
        same = got == want_s
        bad += 0 if same else 1
        shown_got = got if not isinstance(got, str) or len(got) <= 16 else got[:16] + "…"
        shown_want = want_s if not isinstance(want_s, str) or len(want_s) <= 16 else want_s[:16] + "…"
        print(f"  {label:<15} 实测 {str(shown_got):>12}   登记 {str(shown_want):>12}   "
              f"{'OK' if same else '❌ 不一致'}")

    _row("文件数", len(files), want.get("count"))
    _row("回放文件数", replays, want.get("replays"))
    _row("文件名集 sha256", names_sha, want.get("names_sha256"))
    _row("内容 digest", digest, want.get("digest_sha256"))

    if added or missing or changed:
        for label, names in (("多出", added), ("少了", missing), ("内容变了", changed)):
            if not names:
                continue
            bad += len(names)
            print(f"  ❗ {label} {len(names)} 个：{ '，'.join(names[:20]) }"
                  + (f" ……（还有 {len(names) - 20} 个）" if len(names) > 20 else ""))
        print("  ⇒ 语料被动过：三轴数字全部作废（fire/batch/batch_conv 的 647/637 不可比）。")
        print("     先把快照恢复原状（快照目录里的 SHA256SUMS 可以逐份校验）；")
        print("     真要换语料：把新样本放进**另一个**目录，"
              "--update-corpus 重登记，并在 docs/06 写明多了/少了哪些文件。")
    else:
        print("  ⇒ 语料一致（冻结）。")
    return bad


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", required=True)
    ap.add_argument("--check-baseline", action="store_true",
                    help="核对护栏值（读 fixtures/guardrails.json；用 --axis 指定哪条轴）")
    ap.add_argument("--snapshot", metavar="DIR",
                    help="语料快照目录（--check-baseline 的**语料指纹检查**要用它；"
                         "--update-corpus 必填）。给了就逐文件 sha256 比一遍。")
    ap.add_argument("--axis", default="fire", choices=["fire", "batch", "batch_conv"],
                    help="这条 JSONL 是哪条驱动轴（默认 fire）。轴不能混，混了必红。")
    ap.add_argument("--update-baseline", action="store_true",
                    help="把当前实测写回 fixtures/guardrails.json 对应轴（改基线必须在 docs/06 说明）")
    ap.add_argument("--update-corpus", action="store_true",
                    help="把 --snapshot 的逐文件 sha256 写回 guardrails.json 的 corpus "
                         "（= 承认换了语料；必须在 docs/06 写明多了/少了哪些文件）")
    args = ap.parse_args(argv)

    rows = load(args.json)
    total = len(rows)
    fails = [r for r in rows if not r["content_ok"]]
    cats = collections.Counter()
    sole = collections.Counter()
    for r in fails:
        c, s = classify(r)
        cats[c] += 1
        if s:
            sole[c] += 1

    print(f"== 语料：{len({r['file'] for r in rows})} 份有结算批的回放 / {total} 个结算批 ==")
    print(f"内容命中 {total - len(fails)} / {total} ({pct(total - len(fails), total)})"
          f"；失败 {len(fails)} 批\n")
    print(f"{'类别':<44}{'批数':>6}{'占失败':>8}{'独占':>6}")
    print("-" * 66)
    for c, n in cats.most_common():
        print(f"{NAME[c] + f' [{c}]':<44}{n:>6}{pct(n, len(fails)):>8}{sole[c]:>6}")
    print("-" * 66)
    print(f"{'合计':<44}{sum(cats.values()):>6}{'100.0%':>8}{sum(sole.values()):>6}")

    ok = sum(cats.values()) == len(fails)
    print(f"\n加总校验：{sum(cats.values())} == 失败 {len(fails)} ⇒ {'OK' if ok else '不相等！'}")

    d = delayed_stats(rows)
    print("\n== 延迟段（另一根轴，分母是有可比对象的批）==")
    print(f"死亡集合 {d['deaths_ok']} / {d['deaths_checked']} ({pct(d['deaths_ok'], d['deaths_checked'])})"
          f"  多报 {d['deaths_extra']} / 漏报 {d['deaths_missing']} / 错人 {d['deaths_wrong']}")
    print(f"挖洞点   {d['destroy_ok']} / {d['destroy_checked']}"
          f" ({pct(d['destroy_ok'], d['destroy_checked'])})")

    if args.update_baseline or args.check_baseline or args.update_corpus:
        gp = GUARDRAILS
        cur = {
            "batches": total,
            "content_ok": total - len(fails),
            "kind_seq_ok": sum(1 for r in rows if r["kind_seq_ok"]),
            "deaths_ok": d["deaths_ok"],
            "deaths_checked": d["deaths_checked"],
            "destroy_ok": d["destroy_ok"],
            "destroy_checked": d["destroy_checked"],
        }
        if args.update_corpus:
            if not args.snapshot:
                print("\n--update-corpus 需要 --snapshot <快照目录>")
                return 2
            g = json.load(open(gp, encoding="utf-8"))
            rec = corpus_record(args.snapshot)
            old = (g.get("corpus") or {}).get("count")
            g["corpus"] = rec
            open(gp, "w", encoding="utf-8").write(json.dumps(g, ensure_ascii=False, indent=2) + "\n")
            print(f"\n已把语料指纹写回 {gp}")
            print(f"  快照 {args.snapshot}：{rec['count']} 个文件（其中 {rec['replays']} 份回放）"
                  + ("" if old is None else f"；原登记 {old} 个文件"))
            print("  ⚠️ 改语料 = 三轴数字全部要重算。docs/06 里必须写明多了/少了哪些文件，"
                  "提交信息里也要写。")
            return 0
        if args.update_baseline:
            g = json.load(open(gp, encoding="utf-8"))
            g["axes"][args.axis].update(cur)
            open(gp, "w", encoding="utf-8").write(json.dumps(g, ensure_ascii=False, indent=2) + "\n")
            print(f"\n已把 {args.axis} 轴的实测写回 {gp}")
            return 0
        g = json.load(open(gp, encoding="utf-8"))
        want = g["axes"][args.axis]
        print(f"\n== 对护栏基线的校验（轴 = {args.axis}；值来自 fixtures/guardrails.json）==")
        bad = 0
        # 语料指纹**先**比：语料变了的话，下面那些数字全都没有意义（任务书 68 §0-B）
        print("  ── 语料指纹（冻结纪律：新样本另开 replay-new/，不许混进快照）──")
        if args.snapshot:
            bad += check_corpus(g, args.snapshot)
        else:
            print("  ⚠️ 未校验：没给 --snapshot <快照目录> —— 冻结语料纪律要求带上它")
            print("     （merge_check.py 跑护栏时会自动带上）")
        print("\n  ── 三轴数字 ──")
        for k, label in [("batches", "结算批数"), ("content_ok", "内容命中"),
                         ("kind_seq_ok", "种类序列命中"), ("deaths_ok", "死亡集合命中"),
                         ("deaths_checked", "死亡集合分母"), ("destroy_ok", "挖洞点命中"),
                         ("destroy_checked", "挖洞点分母")]:
            exp = want.get(k)
            if exp is None:
                print(f"  {label:<16} 实测 {cur[k]:>6}   基线 未设置（该轴未登记）")
                continue
            same = cur[k] == exp
            bad += 0 if same else 1
            print(f"  {label:<16} 实测 {cur[k]:>6}   基线 {exp:>6}   {'OK' if same else '不一致'}")
        print(f"  校验结果： {'全部一致' if bad == 0 else f'{bad} 项不一致'}")
        return 0 if bad == 0 else 1

    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
