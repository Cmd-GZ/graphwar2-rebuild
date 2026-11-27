#!/usr/bin/env python3
"""评分核心 `0x51a750`（`GraphwarFunctionEvolutionator::score_function`）的**值级对拍**。

## 为什么要有这个脚本

`fitness_eval`（`src/graphwar_function_evolutionator.rs`）与 `ai_explain::evaluate`
（`src/_scaffold/ai_explain.rs`）是同一套评分的两份实现，`docs/06` §126.5 登记过它们与
评分核心**读法不一致**（真值末尾 `0x51b0d9 subsd xmm1,[rsp+8]` 减 dist、命中项带符号
`0x51ae51 dec`/`0x51ae62 inc`）。谁对谁错只能由真二进制裁定 —— 本脚本**直调 `0x51a750`**
拿返回值（unicorn x86-64，`.reference/analysis-repo` 的 `gwemu`），再与我方两份并列。

## 调用形状（`docs/06` §128.2 给的是逐条指令依据）

```text
0x51a750(sret rdi, &ctx rsi, &Vec<Token> rdx, owner_id ecx, step xmm0)
```

* `sret` = 24 字节 4 字段 `{score f64@0, dist f64@8, hits u32@0x10, esi u32@0x14}`
  （写回点 `0x51b10d` / `0x51b11d` / `0x51b111` / `0x51b114`）；
* `&ctx` 只用到几处：`[+0]` = 射手表（自查队伍）、`[+8]` = 实体表（查每个命中的队伍）、
  `+0x18` = step、`+0x58` = limit；两张表都是 `Lazy<HashMap<u32, u8>>` 形状，
  查表哈希走 `0x47dec0`。

## 两段用例

**① `SYNTH`（合成结果项）** —— **不管**弹道，直接把评分核心要吃的"开火结果项"铺好，
证明"分数怎么算"。这是判定公式的**硬证据**（真值 = 二进制算出来的位型）。

结果项（32 字节一条，`kind` 决定走哪个分支；kind 见 `0x51ab88`–`0x51abb1` 的跳表）：

| kind | 跳表项 | 评分核心怎么用它 |
|---|---|---|
| `i64::MIN+0` | `0x51aef0` | 忽略（障碍事件） |
| `i64::MIN+1` | `0x51abb3` | 命中：`payload` 低 32 位 = 实体 id，查队伍后 `+1`/`−1` |
| `i64::MIN+2` | `0x51acd1` | 距离：`item+0x14` = 实体 id，`payload` = f64 距离，**只对敌人取 min** |
| `i64::MIN+3` | `0x51acc1` | `esi += payload>>32`（唯一累加点 `0x51acc8`） |

**② `SCEN`（局面）** —— 真链路：我方 `examples/score_probe` 先跑弹道（与两份实现同源的
`correction`/`shifted_tokens`/`calculate`），脚本按**同一份点列**算出命中，再按上面的
读法把结果项铺给真二进制。于是"真值 vs 两份实现"是同一个局面的三个读数。

⚠️ 结果项的**产生规则**（哪种事件由谁发出）本身是 `docs/06` §78.7 遗留 2 的未决项，
本脚本用的是**声明过的模型**（见 `derive_items`）：敌人/无主实体命中 ⇒ `MIN+1`（进命中数）
**且** `MIN+2`（进距离项）；自己人命中 ⇒ 只有 `MIN+1`；`kind=t` 的靶 ⇒ 只有 `MIN+2`。
模型只影响 `SCEN` 那一段（`SYNTH` 段直接给结果项，不经过模型），`docs/06` §128.4 写明了
"换成别的模型会怎么变"。

## 用法

```bash
P=$PWD/.reference/venv/bin/python
$P tools/score_truth.py                 # 跑全部用例，打印对拍表（不写盘）
$P tools/score_truth.py --write         # 写 fixtures/score_truth.tsv + score_truth_synth.tsv
$P tools/score_truth.py --check         # 只对拍：任一行 truth ≠ 我方实现 ⇒ rc=1
```

`--check` 是**反面对照**的入口：把 `fitness_eval` 故意改坏，它必须变红（§128.5）。
"""

import argparse
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)

from ai_rng_inject import load_emu, EMU_REPO  # noqa: E402  复用 T23 的 unicorn 装载器
from unicorn import UcError  # noqa: E402
from unicorn.x86_const import UC_X86_REG_RDI, UC_X86_REG_RSI  # noqa: E402

SO = os.path.join(ROOT, ".reference/game/libgraphwarrust.so")

# ---------------------------------------------------------------------------
# 真二进制侧常量（全部来自反汇编，见 docs/06 §128.2）
# ---------------------------------------------------------------------------
A_SCORE = 0x51A750        # GraphwarFunctionEvolutionator::score_function
A_SHOT_RESULTS = 0x3C2BC0 # calculate_shot_results（本脚本**替换**它，见 README 段）
A_HASH_U32 = 0x47DEC0     # SipHash13(u32) → 表索引用的哈希；本脚本替换它（等价于喂一张自定义表）
A_PRE_CALL = 0x4413B0     # 取结果项之前的那次临时构造（与评分公式无关）
A_DROP = 0x45FD30         # 上面那个临时值的 drop
A_INIT_CLOSURE = 0x497410 # Lazy 表的初始化闭包（表由本脚本铺好，跳到它没有意义）

I64_MIN = 0x8000000000000000
KIND_OBSTACLE = I64_MIN + 0
KIND_HIT = I64_MIN + 1
KIND_DIST = I64_MIN + 2
KIND_ESI = I64_MIN + 3

DIST_SENTINEL = 1_000_000.0
PENALTY = 50_000_000.0
HIT_WEIGHT = 2_000_000.0
TEAM_UNKNOWN = 2

# ---------------------------------------------------------------------------
# ② SCEN 用例：局面
#
# 字段：case / func / x0 / y0 / flip / step / limit / shooter_id / shooter_team /
#       entities=[(id, x, y, radius, team, kind)]，kind = "s"（士兵，进命中数）/ "t"（靶，进距离项）
#
# 点列的取法：`func="0"` 从 (0,0) 出发是一条 y=0 的水平线（x: 0→20，见
# `examples/score_probe` 的点列 dump），所以实体摆在 (x, dy) 上就能精确造出
# `d = |dy|` 的命中；`d <= radius` 才算命中（与两份实现同一条判据）。
# ---------------------------------------------------------------------------
SCEN = [
    # case, func, x0, y0, flip, step, limit, shooter_id, shooter_team, entities
    ("S01_nohit_v1", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0, []),
    ("S02_nohit_v3", "abs(sin(x))", 0.0, 0.0, 0, 0.5, 25, 101, 0, []),
    ("S03_over_limit", "abs(sin(x))", 0.0, 0.0, 0, 0.5, 1, 101, 0, []),
    ("S04_enemy_hit", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(201, 10.0, 0.0, 0.5, 1, "s")]),
    ("S05_friendly_hit", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(202, 10.0, 0.0, 0.5, 0, "s")]),
    ("S06_enemy_plus_friendly", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(201, 6.0, 0.0, 0.5, 1, "s"), (202, 12.0, 0.0, 0.5, 0, "s")]),
    ("S07_two_enemies_min_dist", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(201, 5.0, 0.4, 0.5, 1, "s"), (203, 10.0, 0.1, 0.5, 1, "s")]),
    ("S08_target_only", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(301, 10.0, 0.2, 0.5, 1, "t")]),
    ("S09_friendly_target", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(302, 10.0, 0.2, 0.5, 0, "t")]),
    ("S10_unknown_team_hit", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(204, 10.0, 0.0, 0.5, 2, "s")]),
    ("S11_both_unknown", "0", 0.0, 0.0, 0, 0.5, 25, 101, 2,
     [(205, 10.0, 0.0, 0.5, 2, "s")]),
    ("S12_hit_plus_target", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(201, 8.0, 0.0, 0.5, 1, "s"), (301, 14.0, 0.3, 0.5, 1, "t")]),
    ("S13_fine_step", "0", 0.0, 0.0, 0, 0.01, 25, 101, 0,
     [(201, 10.0, 0.2, 0.5, 1, "s")]),
    ("S14_hit_over_limit", "0", 0.0, 0.0, 0, 0.5, 1, 101, 0,
     [(201, 10.0, 0.1, 0.5, 1, "s")]),
    ("S15_flip_enemy", "0", 0.0, 0.0, 1, 0.5, 25, 101, 0,
     [(206, 10.0, 0.0, 0.5, 1, "s")]),
    ("S16_miss_far", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(207, 10.0, 2.0, 0.5, 1, "s")]),
    ("S17_mixed_all", "0", 0.0, 0.0, 0, 0.5, 25, 101, 0,
     [(201, 4.0, 0.1, 0.5, 1, "s"), (202, 8.0, 0.2, 0.5, 0, "s"),
      (301, 12.0, 0.3, 0.5, 1, "t"), (302, 16.0, 0.1, 0.5, 0, "t")]),
]

# ---------------------------------------------------------------------------
# ① SYNTH 用例：直接给结果项
#
# item = ("hit"|"dist"|"esi"|"obstacle", 参数)
#   hit  → (id)                         ⇒ kind MIN+1，payload = id
#   dist → (id, 距离)                    ⇒ kind MIN+2，item+0x14 = id，payload = f64
#   esi  → (n)                          ⇒ kind MIN+3，payload = n<<32
#   obstacle                            ⇒ kind MIN+0（被忽略）
#
# 队伍表：`teams` = {id: team}；`owner` = 射手（第 4 个参数 ecx）的 id。
# 期望值由 `formula()` 在 Python 里**按指令顺序重算**再与真值比位型 —— 两边独立。
# ---------------------------------------------------------------------------
SYNTH = [
    # case, v, step, limit, owner_id, teams, items
    ("Y01_empty_v1", 1, 0.5, 25, 101, {}, []),
    ("Y02_empty_v10", 10, 0.5, 25, 101, {}, []),
    ("Y03_empty_over_limit", 30, 0.5, 25, 101, {}, []),
    ("Y04_one_enemy_hit", 2, 0.5, 25, 101, {101: 0, 201: 1}, [("hit", 201)]),
    ("Y05_one_friendly_hit", 2, 0.5, 25, 101, {101: 0, 202: 0}, [("hit", 202)]),
    ("Y06_two_enemy_hits", 2, 0.5, 25, 101, {101: 0, 201: 1, 203: 1},
     [("hit", 201), ("hit", 203)]),
    ("Y07_enemy_minus_friendly", 2, 0.5, 25, 101, {101: 0, 201: 1, 202: 0},
     [("hit", 201), ("hit", 202)]),
    ("Y08_hit_plus_dist", 2, 0.5, 25, 101, {101: 0, 201: 1},
     [("hit", 201), ("dist", 201, 0.25)]),
    ("Y09_dist_only", 2, 0.5, 25, 101, {101: 0, 201: 1}, [("dist", 201, 0.125)]),
    ("Y10_friendly_dist_skipped", 2, 0.5, 25, 101, {101: 0, 202: 0},
     [("dist", 202, 0.125)]),
    ("Y11_unknown_team_hit", 2, 0.5, 25, 101, {101: 0}, [("hit", 201)]),
    ("Y12_unknown_both", 2, 0.5, 25, 101, {101: 2, 201: 2}, [("hit", 201)]),
    ("Y13_min_dist_two_items", 2, 0.5, 25, 101, {101: 0, 201: 1, 203: 1},
     [("dist", 201, 0.4), ("dist", 203, 0.1)]),
    ("Y14_esi_plus_hit", 2, 0.5, 25, 101, {101: 0, 201: 1},
     [("hit", 201), ("esi", 3)]),
    ("Y15_esi_no_hit", 2, 0.5, 25, 101, {101: 0}, [("esi", 3)]),
    ("Y16_esi_negative_total", 2, 0.5, 25, 101, {101: 0, 202: 0},
     [("hit", 202), ("esi", 3)]),
    ("Y17_esi_fine_step", 2, 0.01, 25, 101, {101: 0, 201: 1},
     [("hit", 201), ("esi", 7)]),
    ("Y18_obstacle_ignored", 2, 0.5, 25, 101, {101: 0}, [("obstacle",)]),
    ("Y19_all_kinds", 4, 0.5, 25, 101, {101: 0, 201: 1, 202: 0, 203: 1},
     [("obstacle",), ("hit", 201), ("hit", 202), ("dist", 203, 0.05), ("esi", 2)]),
    ("Y20_penalty_and_hit", 30, 0.5, 25, 101, {101: 0, 201: 1},
     [("hit", 201), ("dist", 201, 0.3)]),
    ("Y21_two_hits_plus_esi", 2, 0.5, 25, 101, {101: 0, 201: 1, 203: 1},
     [("hit", 201), ("hit", 203), ("esi", 5)]),
    ("Y22_dist_over_radius_value", 2, 0.5, 25, 101, {101: 0, 201: 1},
     [("dist", 201, 3.5)]),
]


# ---------------------------------------------------------------------------
# 真二进制侧：unicorn 直调 0x51a750
# ---------------------------------------------------------------------------
class Truth:
    """把 `0x51a750` 需要的三样东西铺好，只留"结果项"由调用方给。"""

    def __init__(self):
        Gw = load_emu()
        self.em = Gw()
        em = self.em
        em.hook(A_PRE_CALL, lambda uc: None)
        em.hook(A_INIT_CLOSURE, lambda uc: None)
        em.hook(A_DROP, lambda uc: None)
        em.hook(0x907300, lambda uc: 1)   # Once 慢路径的"线程状态"查询：恒"没被别的线程占"
        em.hook(0x8F5CC0, lambda uc: 0)   # Once 慢路径加锁
        em.hook(0x8F5DB0, lambda uc: 0)   # Once 慢路径解锁
        self.items = []
        self.teams = {}
        self.h = 0x11 << 57               # h2 = 0x11，与下面 ctrl[0] 对齐
        self.ctrl = None

    # -- 两张表（射手表 / 实体表）：都是 hashbrown 形状，桶数 = 1 --
    def _table(self, key_value):
        em = self.em
        obj = em.alloc(0x100)
        em.wr(obj, b"\x00" * 0x100)
        ctrl = em.alloc(0x40)
        data = ctrl - 8
        em.wr(ctrl, b"\xff" * 0x20)
        if key_value is not None:
            ctrl_byte = bytes([(self.h >> 57) & 0x7F])
            em.wr(ctrl, ctrl_byte + b"\xff" * 0x1F)
            em.wq(data, key_value[0])          # 桶键（u32，用 qword 铺，高 4 字节不读）
            em.wr(data + 4, bytes([key_value[1]]))  # 桶值 = 队伍字节
        em.wq(obj + 0x18, ctrl)   # ctrl
        em.wq(obj + 0x20, 0)      # bucket_mask = 0（单桶）
        em.wq(obj + 0x28, 1)      # growth_left
        em.wq(obj + 0x30, 1)      # items（!=0 才走查表）
        em.wq(obj + 0x38, 0x0123456789ABCDEF)  # hash_builder（0x47dec0 会读它）
        em.wq(obj + 0x40, 0xFEDCBA9876543210)
        return obj

    def build(self, owner_id, teams, step, limit):
        em = self.em
        self.teams = teams
        # 两张表：射手表 = 单条 (owner_id → 队伍)，实体表同形（本 harness 里两张都只放一条，
        # 未命中就是"查不到"⇒ 队伍 = 2）；细节见 docs/06 §128.3 的"表怎么铺"。
        self.t_owner = self._table((owner_id, teams.get(owner_id, TEAM_UNKNOWN)))
        self.t_ent = self._table(self._ent_entry)
        ctx = em.alloc(0x100)
        em.wr(ctx, b"\x00" * 0x100)
        em.wq(ctx + 0x00, self.t_owner)
        em.wq(ctx + 0x08, self.t_ent)
        em.wr_f64(ctx + 0x18, step)
        em.wr(ctx + 0x58, struct.pack("<I", limit))
        self.ctx = ctx
        self.owner_id = owner_id

    def run(self, v, step, limit, owner_id, teams, item_ids):
        """跑一次评分。`item_ids` = 结果项里所有会被查表的实体 id（顺序无关）。

        实体表的实现细节：桶只有一个 ⇒ 只能存一条。为了让每个 id 都查得到，
        这里**按查表顺序重铺 ctrl/桶**——`_h_hash` 每次被调时把桶内容换成"当前要查的
        那个 id"的值。这是 harness 自身的实现选择，不改变被侧代码的任何一条指令。
        """
        em = self.em
        self.teams = dict(teams)
        self.ent_ids = list(item_ids)
        self.t_owner = self._table((owner_id, teams.get(owner_id, TEAM_UNKNOWN)))
        self.t_ent = self._table(None)
        ctx = em.alloc(0x100)
        em.wr(ctx, b"\x00" * 0x100)
        em.wq(ctx + 0x00, self.t_owner)
        em.wq(ctx + 0x08, self.t_ent)
        em.wr_f64(ctx + 0x18, step)
        em.wr(ctx + 0x58, struct.pack("<I", limit))
        self.owner_id = owner_id

        # 结果项铺进堆里，sret 由 stub 写
        n = len(self.items)
        buf = em.alloc(max(n, 1) * 32 + 32)
        for i, it in enumerate(self.items):
            base = buf + i * 32
            kind, payload, f10, f14 = it
            em.wq(base, kind)
            em.wq(base + 8, payload & 0xFFFFFFFFFFFFFFFF)
            em.wr(base + 0x10, struct.pack("<II", f10 & 0xFFFFFFFF, f14 & 0xFFFFFFFF))
        self.results = (buf, n)

        vec = em.make_vec([(0, 0.0)] * v)  # 只要长度对（真值只读 len）
        sret = em.alloc(0x20)
        em.wr(sret, b"\x00" * 0x20)
        try:
            em.call(A_SCORE, [sret, ctx, vec, owner_id], [step])
        except UcError:
            # `emu.call` 收尾时会用返回值 `rax` 去 `mem_read` —— 评分核心返回的是
            # 24 字节 sret（走 rdi），rax 的内容没有语义，可能指向不可读地址。
            # 那时仿真已经跑完、sret 已写回，忽略即可。
            pass
        score = em.rd_f64(sret)
        dist = em.rd_f64(sret + 8)
        hits = struct.unpack("<I", em.rd(sret + 0x10, 4))[0]
        esi = struct.unpack("<I", em.rd(sret + 0x14, 4))[0]
        return dict(score=score, dist=dist, hits=hits, esi=esi)


def _make_truth():
    """构造 Truth 实例（把两个 hook 装好；`_h_shot_results` 在 run 里读 self.results）。"""
    t = Truth()

    def h_shot_results(uc):
        em = t.em
        rdi = uc.reg_read(UC_X86_REG_RDI)
        buf, n = t.results
        em.wq(rdi, max(n, 1))
        em.wq(rdi + 8, buf)
        em.wq(rdi + 0x10, n)
        return None

    def h_hash(uc):
        em = t.em
        key_ptr = uc.reg_read(UC_X86_REG_RSI)
        key = struct.unpack("<I", em.rd(key_ptr, 4))[0]
        # 用"当前要查的 id"重铺实体表，使这次查表命中且拿到正确队伍
        ent = t.em.rq(t.t_ent + 0x18)   # ctrl
        data = ent - 8
        em.wq(data, key)
        em.wr(data + 4, bytes([t.teams.get(key, TEAM_UNKNOWN)]))
        em.wr(ent, bytes([(0x11 << 0)]) + b"\xff" * 0x1F)
        return t.h

    t.em.hook(A_SHOT_RESULTS, h_shot_results)
    t.em.hook(A_HASH_U32, h_hash)
    return t


def formula(bits):
    """按 `0x51b063`–`0x51b108` 的指令顺序，把结果项折算成分数（**独立于二进制**重算）。"""
    v, step, limit, net, dist, esi = bits
    # 0x51b063 test r13d,r13d / mov eax,1 / cmovle eax,r13d ⇒ eax = (net<=0 ? net : 1)
    eax = net if net <= 0 else 1
    # 0x51b073 test esi,esi / cmove eax,r13d        ⇒ esi==0 时回到 net
    if esi == 0:
        eax = net
    acc = 0.0
    if eax > 0:
        acc = 0.0 - ((float(esi) * (step * 100.0)))
    total = float(eax) * HIT_WEIGHT + acc
    total = total - dist
    vv = float((v * v) & 0xFFFFFFFFFFFFFFFF)  # 0x51b0bd imul（64 位）
    total = total - vv / 100.0
    if v > limit:
        total = total + (-PENALTY)
    return total


def synth_items(spec):
    """SYNTH 用例的 items → 二进制侧的 32 字节记录列表。"""
    out = []
    for it in spec:
        if it[0] == "hit":
            out.append((KIND_HIT, it[1], 0, 0))
        elif it[0] == "dist":
            out.append((KIND_DIST, struct.unpack("<Q", struct.pack("<d", it[2]))[0], 0, it[1]))
        elif it[0] == "esi":
            out.append((KIND_ESI, (it[1] & 0xFFFFFFFF) << 32, 0, 0))
        elif it[0] == "obstacle":
            out.append((KIND_OBSTACLE, 0, 0, 0))
        else:
            raise SystemExit("未知 item：%r" % (it,))
    return out


def derive_items_ai(order, teams, owner_id):
    """`ai_explain` 的实体映射（`Entity::soldier` / `Entity::target`）：士兵只进命中数、
    靶只进距离项。用它铺真值，才能把"它的**公式**对不对"与"模型对不对"分开。"""
    spec = []
    for eid, d, team, kind in order:
        if kind == "s":
            spec.append(("hit", eid))
        elif not friendly(team, teams.get(owner_id, TEAM_UNKNOWN)):
            spec.append(("dist", eid, d))
    return spec


def net_and_dist(spec, teams, owner_id):
    """按结果项算出 (带符号命中数, dist)：与 `formula` 分开写，避免同一处算两遍。"""
    net = 0
    dist = DIST_SENTINEL
    for it in spec:
        if it[0] == "hit":
            net += -1 if friendly(teams.get(it[1], TEAM_UNKNOWN), teams.get(owner_id, TEAM_UNKNOWN)) else 1
        elif it[0] == "dist":
            d = it[2]
            if not friendly(teams.get(it[1], TEAM_UNKNOWN), teams.get(owner_id, TEAM_UNKNOWN)):
                dist = min(dist, d)
    return net, dist


def expected_eax(net, esi):
    """sret `+0x10` 写的是 `%eax`（`0x51b111`），esi != 0 且 net > 0 时被夹成 1。"""
    if esi != 0 and net > 0:
        return 1
    return net


def friendly(entity_team, shooter_team):
    """`0x51ae21`–`0x51ae4a`：实体队 2 / 射手队 2 是特例，其余比最低位。"""
    if entity_team == TEAM_UNKNOWN:
        return shooter_team == TEAM_UNKNOWN
    if shooter_team == TEAM_UNKNOWN:
        return False
    return (entity_team ^ shooter_team) & 1 == 0


# ---------------------------------------------------------------------------
# SCEN：跑 Rust 侧的局面执行器，取点列；再按模型铺结果项
# ---------------------------------------------------------------------------
def write_cases(path, cases):
    with open(path, "w") as f:
        f.write("case\tfunc\tx0\ty0\tflip\tstep\tlimit\tshooter_id\tshooter_team\tentities\n")
        for c in cases:
            name, func, x0, y0, flip, step, limit, sid, steam, ents = c
            es = ";".join("%d,%r,%r,%r,%d,%s" % (e[0], e[1], e[2], e[3], e[4], e[5]) for e in ents)
            f.write("%s\t%s\t%r\t%r\t%d\t%r\t%d\t%d\t%d\t%s\n"
                    % (name, func, x0, y0, flip, step, limit, sid, steam, es))


def run_rust(path):
    import subprocess
    env = dict(os.environ)
    env.setdefault("CARGO_HOME", os.path.join(ROOT, ".reference/cargo-home"))
    env.setdefault("CARGO_TARGET_DIR", os.path.join(ROOT, ".cargo-target"))
    cmd = ["cargo", "run", "--release", "--offline", "-q", "--example", "score_probe", "--", path]
    proc = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
        raise SystemExit("cargo run --example score_probe 失败（rc=%d）" % proc.returncode)
    rows = {}
    for line in proc.stdout.splitlines():
        f = line.split("\t")
        if f[0] == "case":
            continue
        rows[f[0]] = dict(
            v=int(f[1]),
            points=[tuple(map(float, p.split(","))) for p in f[2].split(";") if p],
            fit_score_bits=int(f[3], 16),
            fit_dist_bits=int(f[4], 16),
            fit_hits=int(f[5]),
            fit_esi=int(f[6]),
            ai_score_bits=int(f[7], 16),
            ai_dist_bits=int(f[8], 16),
            ai_kills=int(f[9]),
            ai_friendly=int(f[10]),
        )
    return rows


def hits_of(points, ents):
    """与两份实现同一条命中判据：点外层、实体内层、每个实体只记一次。"""
    killed = []
    order = []
    for px, py in points:
        for e in ents:
            eid, ex, ey, r, team, kind = e
            if eid in killed:
                continue
            d = ((px - ex) ** 2 + (py - ey) ** 2) ** 0.5
            if d <= r:
                killed.append(eid)
                order.append((eid, d, team, kind))
    return order


def fit_expr(bits, net, dist, esi):
    """**转写**（读当前 `fitness_eval` 源码手抄，不是运行）：
    `2e6·net − dist − V²/100`（`esi` 在实现里恒 0，所以没有 `−esi·step·100` 那一项）。

    ⚠️ 改前那一版是 `−1e6 − V²/100 + 2e6·(命中项个数)`（不减 dist、命中数不分敌我），
    它在下面每条用例上的读数见 `docs/06` §128.5（本列只反映**当前**源码）。
    """
    v, step, limit = bits
    total = HIT_WEIGHT * float(net)
    total = total - dist
    total = total - (float((v * v) & 0xFFFFFFFFFFFFFFFF) / 100.0)
    if v > limit:
        total -= PENALTY
    return total


def ai_expr(bits, net, dist, esi):
    """**转写**（读 `ai_explain::evaluate` 手抄，不是运行）：它的表达式与真值同形。

        score = 2e6·(敌人−自己人) − dist − V²/100 − (V>limit ? 5e7 : 0)
    """
    v, step, limit = bits
    total = HIT_WEIGHT * float(net)
    total = total - dist
    total = total - (float((v * v) & 0xFFFFFFFFFFFFFFFF) / 100.0)
    if v > limit:
        total -= PENALTY
    return total


def derive_items(order, teams, owner_id):
    """`fitness_eval` 的实体模型（`docs/06` §128.4）：`ctx.targets` 里每一条都是**会进命中数**
    的实体，所以命中 → `MIN+1`；敌人/无主的命中再加一条 `MIN+2`（距离项）；自己人只进命中数。
    """
    spec = []
    for eid, d, team, kind in order:
        spec.append(("hit", eid))
        if not friendly(team, teams.get(owner_id, TEAM_UNKNOWN)):
            spec.append(("dist", eid, d))
    return spec


def _bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def _from_bits(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="写 fixtures/*.tsv")
    ap.add_argument("--check", action="store_true", help="任一行不吻合 ⇒ rc=1")
    ap.add_argument("--cases", default=os.path.join(ROOT, ".agent-work/ev/cases.tsv"))
    args = ap.parse_args(argv)

    truth = _make_truth()
    synth_rows = []

    # ---------- ① SYNTH ----------
    print("=" * 132)
    print("① SYNTH：直接把开火结果项喂给真二进制（不经弹道、不经模型）—— 判定**公式**")
    print("=" * 132)
    print("%-26s %-20s %-20s %-20s %-20s %-6s %-9s %s"
          % ("case", "truth(0x51a750)", "指令顺序重算", "fitness_eval(转写)", "ai_explain(转写)",
             "hits", "dist", "判定"))
    synth_bad_formula = 0
    synth_bad_fit = 0
    synth_bad_ai = 0
    for name, v, step, limit, owner_id, teams, spec in SYNTH:
        truth.items = synth_items(spec)
        ids = [it[1] for it in spec if it[0] in ("hit", "dist")]
        got = truth.run(v, step, limit, owner_id, teams, ids)
        net, dist = net_and_dist(spec, teams, owner_id)
        ref = formula((v, step, limit, net, dist, got["esi"]))
        fx = fit_expr((v, step, limit), net, dist, got["esi"])
        ax = ai_expr((v, step, limit), net, dist, got["esi"])
        comparable = got["esi"] == 0
        ok_formula = _bits(got["score"]) == _bits(ref) and got["dist"] == dist \
            and got["hits"] == (expected_eax(net, got["esi"]) & 0xFFFFFFFF)
        ok_fit = comparable and _bits(got["score"]) == _bits(fx)
        ok_ai = comparable and _bits(got["score"]) == _bits(ax)
        synth_bad_formula += 0 if ok_formula else 1
        synth_bad_fit += 0 if (ok_fit or not comparable) else 1
        synth_bad_ai += 0 if (ok_ai or not comparable) else 1
        mark = "不可比" if not comparable else ("fit✓" if ok_fit else "fit✗") + "/" + ("ai✓" if ok_ai else "ai✗")
        print("%-26s %-20r %-20r %-20r %-20r %-6d %-9r %s"
              % (name, got["score"], ref, fx, ax, got["hits"], got["dist"],
                 ("公式✓ " if ok_formula else "公式✗ ") + mark))
        synth_rows.append((name, v, step, limit, owner_id, teams, spec, got))
    n = len(SYNTH)
    n_cmp = sum(1 for r in synth_rows if r[7]["esi"] == 0)
    print("\nSYNTH %d 条：真值 = 指令顺序重算 %d/%d ｜ 可比 %d 条里 fitness_eval(当前转写) 吻合 %d/%d、"
          "ai_explain 吻合 %d/%d（余 %d 条真值 esi≠0，两份实现都不产 esi 项 ⇒ 不可比）"
          % (n, n - synth_bad_formula, n, n_cmp, n_cmp - synth_bad_fit, n_cmp,
             n_cmp - synth_bad_ai, n_cmp, n - n_cmp))

    # ---------- ② SCEN ----------
    write_cases(args.cases, SCEN)
    rust = run_rust(args.cases)
    print("\n" + "=" * 132)
    print("② SCEN：同一局面（弹道同源），真值 vs fitness_eval vs ai_explain")
    print("   truth(fit模型)=敌人/无主的士兵命中额外铺 MIN+2 距离项；truth(ai模型)=士兵只铺 MIN+1、靶只铺 MIN+2")
    print("=" * 132)
    print("%-25s %-19s %-19s %-19s %-19s %-19s %s"
          % ("case", "truth(fit模型)", "fitness_eval", "truth(ai模型)", "ai_explain", "Δ(fit−truth)", "判定"))
    scen_rows = []
    scen_bad_fit = 0
    scen_bad_ai = 0
    for name, func, x0, y0, flip, step, limit, sid, steam, ents in SCEN:
        r = rust[name]
        teams = {sid: steam}
        for e in ents:
            teams[e[0]] = e[4]
        order = hits_of(r["points"], ents)
        spec_fit = derive_items(order, teams, sid)
        spec_ai = derive_items_ai(order, teams, sid)
        net_f, dist_f = net_and_dist(spec_fit, teams, sid)
        net_a, dist_a = net_and_dist(spec_ai, teams, sid)
        truth.items = synth_items(spec_fit)
        got_f = truth.run(r["v"], step, limit, sid, teams,
                          [it[1] for it in spec_fit if it[0] in ("hit", "dist")])
        truth.items = synth_items(spec_ai)
        got_a = truth.run(r["v"], step, limit, sid, teams,
                          [it[1] for it in spec_ai if it[0] in ("hit", "dist")])
        # 真值公式自检（与 ① 同一份重算）
        for got, (net, dist) in ((got_f, (net_f, dist_f)), (got_a, (net_a, dist_a))):
            ref = formula((r["v"], step, limit, net, dist, got["esi"]))
            assert _bits(got["score"]) == _bits(ref), (name, got["score"], ref)
        fit_ok = r["fit_score_bits"] == _bits(got_f["score"])
        ai_ok = r["ai_score_bits"] == _bits(got_a["score"])
        scen_bad_fit += 0 if fit_ok else 1
        scen_bad_ai += 0 if ai_ok else 1
        delta = _from_bits(r["fit_score_bits"]) - got_f["score"]
        print("%-25s %-19r %-19r %-19r %-19r %-19.6g %s"
              % (name, got_f["score"], _from_bits(r["fit_score_bits"]), got_a["score"],
                 _from_bits(r["ai_score_bits"]), delta,
                 ("fit✓" if fit_ok else "fit✗") + "/" + ("ai✓" if ai_ok else "ai✗")))
        scen_rows.append(dict(name=name, v=r["v"], func=func, x0=x0, y0=y0, flip=flip,
                              step=step, limit=limit, sid=sid, steam=steam, ents=ents,
                              order=order, spec=spec_fit, got=got_f,
                              fit=r["fit_score_bits"], ai=r["ai_score_bits"],
                              ai_truth=got_a["score"], ai_ok=ai_ok))
    print("\nSCEN %d 条：fitness_eval 吻合 %d/%d ｜ ai_explain 吻合 %d/%d"
          % (len(SCEN), len(SCEN) - scen_bad_fit, len(SCEN), len(SCEN) - scen_bad_ai, len(SCEN)))

    if args.write:
        p1 = os.path.join(ROOT, "fixtures/score_truth.tsv")
        with open(p1, "w") as f:
            f.write("# 真值 = unicorn 直调评分核心 0x51a750 的返回值（sret 24 字节 4 字段）。\n")
            f.write("# 生成：.reference/venv/bin/python tools/score_truth.py --write\n")
            f.write("# 判据：src/graphwar_function_evolutionator.rs 的 fitness_eval 必须与 truth_score_bits 逐位相同。\n")
            f.write("# 结果项模型（== fitness_eval 的实体模型）：每个命中铺 MIN+1（进命中数），\n")
            f.write("#   敌人/无主的命中再铺 MIN+2（进距离项），自己人只铺 MIN+1。模型是未决项，见 docs/06 §128.4。\n")
            f.write("# entities = id,x,y,radius,team,kind 用 ; 串；items = 喂给真值的结果项（人类可读）\n")
            f.write("case\tv\tfunc\tx0\ty0\tflip\tstep\tlimit\tshooter_id\tshooter_team\tentities\t"
                    "items\ttruth_score_bits\ttruth_dist_bits\ttruth_hits\ttruth_esi\n")
            for r in scen_rows:
                es = ";".join("%d,%r,%r,%r,%d,%s" % e for e in r["ents"])
                spec = " ".join(_fmt_item(it) for it in r["spec"])
                got = r["got"]
                f.write("%s\t%d\t%s\t%r\t%r\t%d\t%r\t%d\t%d\t%d\t%s\t%s\t%016x\t%016x\t%d\t%d\n"
                        % (r["name"], r["v"], r["func"], r["x0"], r["y0"], r["flip"],
                           r["step"], r["limit"], r["sid"], r["steam"], es, spec,
                           _bits(got["score"]), _bits(got["dist"]), got["hits"], got["esi"]))
        p2 = os.path.join(ROOT, "fixtures/score_truth_synth.tsv")
        with open(p2, "w") as f:
            f.write("# ① 合成结果项：不弹道、不模型，直接把 0x51a750 要吃的开火结果项铺进去。\n")
            f.write("# 生成：.reference/venv/bin/python tools/score_truth.py --write\n")
            f.write("# 判据：score/dist/hits 必须与「按 0x51b063–0x51b108 指令顺序重算」逐位相同。\n")
            f.write("# net/dist 两列是脚本按结果项汇总出来的三项（带符号命中数 / 距离项 / esi 累加），\n")
            f.write("# 用来让 Rust 侧单测直接喂 `score_from_terms`（分数算术只有那一份实现）。\n")
            f.write("case\tv\tstep\tlimit\towner_id\tteams\titems\tnet\ttruth_score_bits\ttruth_dist_bits\ttruth_hits\ttruth_esi\n")
            for name, v, step, limit, owner_id, teams, spec, got in synth_rows:
                tstr = ",".join("%d:%d" % kv for kv in sorted(teams.items()))
                istr = " ".join(_fmt_item(it) for it in spec)
                net, _dist = net_and_dist(spec, teams, owner_id)
                f.write("%s\t%d\t%r\t%d\t%d\t%s\t%s\t%d\t%016x\t%016x\t%d\t%d\n"
                        % (name, v, step, limit, owner_id, tstr, istr, net,
                           _bits(got["score"]), _bits(got["dist"]), got["hits"], got["esi"]))
        print("\n已写 %s（%d 行）与 %s（%d 行）" % (p1, len(scen_rows), p2, len(synth_rows)))

    if args.check:
        bad = synth_bad_formula + scen_bad_fit + scen_bad_ai
        if bad:
            print("\n--check：SYNTH 公式不一致 %d、SCEN fitness_eval %d、SCEN ai_explain %d ⇒ rc=1"
                  % (synth_bad_formula, scen_bad_fit, scen_bad_ai))
            return 1
        print("\n--check 全绿：SYNTH 公式 %d/%d、SCEN fitness_eval %d/%d、SCEN ai_explain %d/%d ⇒ rc=0"
              % (n - synth_bad_formula, n, len(SCEN) - scen_bad_fit, len(SCEN),
                 len(SCEN) - scen_bad_ai, len(SCEN)))
        return 0
    return 0


def _fmt_item(it):
    if it[0] == "hit":
        return "hit(%d)" % it[1]
    if it[0] == "dist":
        return "dist(%d,%.6g)" % (it[1], it[2])
    if it[0] == "esi":
        return "esi(%d)" % it[1]
    return "obstacle()"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
