#!/usr/bin/env python3
"""`src/_scaffold/` 生产函数的**归属台账**（A/B/C/D）—— 判据、正对照、复算。

任务 T63。目标（`AGENTS.md` §0 第 4 条 + §4「文件名就是证据」）：
`src/_scaffold/` 按定义只该放**原版没有对应物**的东西；装在里面的游戏语义要搬回
同名 `src/<module>.rs`。本工具把剩下的每个生产函数判成四档之一，并把判据写成**可复算**的形式。

    python3 tools/scaffold_triage.py            # 打印汇总（按类计数 + 逐文件分布）
    python3 tools/scaffold_triage.py --tsv      # 打印台账 TSV（= fixtures/scaffold_triage.tsv）
    python3 tools/scaffold_triage.py --control  # 正/反对照（必须全部对上，否则 rc=1）

# 四档判据（互斥；优先级 **B > C > D > A**，理由见下）

## A —— 真自造（原版**没有对应物** ⇒ 留原地是对的）

判据是**文件角色**（下面的 `FILE_ROLE` 表，每行都给出处）+ 函数级特征：

1. 原版没有的原语：GDExtension 入口（`gdext.rs`/`gdext_mod.rs`/`gdext_class.rs`）、
   手写 C ABI（`capi.rs`/`api.rs`/`axis_drawer_class.rs`）、报文装配（`protocol.rs`）、
   ChaCha12（`rng.rs`）、inflate/gzip（`replay_inflate.rs`）、WAV 解码（`audio_wav.rs`）、
   broker 服务端（`broker.rs`）、对拍器（`replaydiff.rs`/`diffconv.rs`）、
   回放驱动与元数据（`replay_driver*.rs`/`replay_meta*.rs`）、我方流程驱动（`play_flow_class.rs`
   的 `Gw*` 类）、我方接线容器（`root_wiring.rs`）、`Gw*` 名前缀的 GDScript 桥接类
   （`audio_control_class.rs` 等 8 个，名字不是原版类名）。
2. 函数级：C ABI 入口/注册/构造器（`sym`/`sn`/`add_method`/`add_property`/`ctor`/
   `read_i64`/`write_int`/`instance_mut`/`create_instance`/`free_instance`）、
   验收钩子（`*_count`/`*_calls`/`provenance`/`*_addrs`/`ping*`/`registered`/`instances`）、
   以及只做**薄转发**（函数体调 `crate::<原版模块>::<同名/对应函数>`，自己不算）的接线。

## B —— 装游戏语义（原版本该在 `src/graphwar_*.rs` 里 ⇒ 搬）

**机判判据**（`b_files()`）—— 三条全中才算 B，缺一条就落到 A/C/D：

1. 文件里有 `#[derive(GodotClass)] pub struct <T>`；
2. `<T>` 是**原版类名**：出现在 `fixtures/name_to_addr_v2.tsv` 第 2 列（三向匹配的 COFF/arm64
   符号名），**并且**该类的某个函数有落 `src/*.rs` 的 `core::panic::Location` 锚；
3. 该文件**不是** GDExtension 入口（`#[gdextension]` 不在这里，入口是 `gdext_mod.rs`）。

理由：原版把 `#[derive(GodotClass)]` 的 struct 与方法体**写在同一个 `graphwar_*.rs` 里**
（`panic::Location` 为证，见 `docs/06-verification.md` §104.1/§104.2）；我们把它拆成
「纯逻辑在模块文件 + 方法体在 `_scaffold/*_class.rs`」是**我方**的分层，不是原版的。
B 按**文件整体**判（同一个 `impl` 块/同一个文件里的 `#[func]` 验收钩子、私有 helper
**与类同生共死**，见 §104.1）—— 所以 B 文件里的钩子函数也记 B，但 `note` 列写明
「我方验收钩子（性质 A，随类搬）」，不要把它们读成原版函数。

## C —— 重复实现（同一条规则/助手两处 ⇒ 指出另一处）

判据两种，逐行登记在 `C_LIST`：

* **逐字相同**：本函数体与别处某函数体在空白归一后**逐字符相同**（`identical_body_groups()`
  机算出候选，人工确认后写进 `C_LIST`）——这是最硬的证据，另一处给 `文件:行`；
* **语义相同**：两处实现同一条规则（例：`diffconv::fire_batch*` 复刻 `game.rs::Match::fire*`
  的结算路径；`ai_explain::evaluate` 复刻 `graphwar_function_evolutionator` 的评分）。

C 与 A/B 不冲突：一个文件整体留原地（A）也可能有个别函数是 C（例 `gdext_mod::triples`
与 `api::triples` 逐字相同）；B 文件里的函数也可能是 C（例 `join_text`）。

## D —— 分不清（缺证据）

判据：**不知道它是原版语义（B）还是我方发明（A）**，并写明缺什么。逐行登记在 `D_LIST`。
典型：`play_flow_class` 的玩家轮转顺序（`docs/06` §86.4 的 B18，原版是 HashMap 迭代序 ⇒
顺序未知）；`replay_meta` 的头部解析（T36 §86.4 判 B16「属 replay_reader 职责」，
T57 §104.0 判 A「我方自造元数据」—— **两处结论冲突**，缺原版"回放列表读关卡名"那段的函数/地址）。

## 执行结果（**后加**，不回填台账：预注册纪律见 `docs/06` §104.1）

* `43896d1` —— 台账本身（793 行，A 707 / B 56 / C 17 / D 13）。
* `03704be` —— **搬完 B 档的两个文件**：`graphwar_setup_control_class.rs`(50) + `graphwar_ad_switch_class.rs`(7)
  → 各自的同名模块（`docs/06` §113.2）。`_scaffold` 生产函数 **793 → 736**。
  注意 **B 56 ≠ 搬 57**：B 按函数判（`join_text` 因是 C 被排除），搬按**文件整体**搬（50 + 7 = 57）。
* 搬之后本工具的分母变 736、`B` 归零；`--control` 仍要全绿（正对照用的是现役 `axis_drawer_class.rs`
  与 `git show cda427c^:…` 的历史版 `entity_drawer_class.rs`，都不受搬迁影响）。

# 正对照（证明这套判法能把「对」的判对，`--control` 跑）

| 对照 | 期望 | 依据 |
|---|---|---|
| `src/_scaffold/axis_drawer_class.rs`（现役，24 fn） | **0 个 B**（A 21 + C 3） | T57 §104.3-4：预注册成 D，实判 A（没有一个游戏语义）。3 个 C 是 `clamp_mode`（T57 §104.7-4 自己登记的 C 档小项）、`line_primitive_count`、`sym` —— 都是与别处逐字相同/同规则的助手 |
| `src/_scaffold/entity_drawer_class.rs`（历史版，`cda427c^`，97 fn） | **全 B** | T57 §104.2 第 1 行：整份搬回 `src/entity_drawer.rs`（`draw 0x4b2890` 的 Location 落 `entity_drawer.rs`） |
| 反面对照：把上面那份 entity_drawer 文件的 `#[derive(GodotClass)]` 换成 `Gw` 前缀类名 | **应翻成 A** | 证明判 B 靠的是"原版类名 + Location 锚"，不是"见 `*_class.rs` 就判 B" |

历史文件用 `git show cda427c^:src/_scaffold/entity_drawer_class.rs` 取（`.agent-work/` 下，
不入库）。正对照失败 ⇒ 本工具 rc=1、台账不许提交。

# 复算命令

```bash
V=$MAIN/.reference/venv/bin/python
$V tools/scaffold_triage.py            # 汇总（793 个生产函数 = 与 audit_src.py scaffold 同口径）
$V tools/scaffold_triage.py --tsv > fixtures/scaffold_triage.tsv
$V tools/audit_src.py scaffold         # 核对分母（prod 列合计 793）
$V tools/scaffold_triage.py --control  # 正/反对照
```

口径：**生产函数** = 文件里第一个 `#[cfg(test)]` **块**之前（与 `tools/audit_src.py` 同口径，
见 `docs/06` §86.7 的 D04）；地址一律 **ELF vaddr（RVA）**（Ghidra/REA = +0x100000）。
"""

import argparse
import collections
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SCAFFOLD = os.path.join(ROOT, "src", "_scaffold")

FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<(]")
CFG_TEST_RE = re.compile(r"^#\[cfg\(test\)\]", re.M)
STRUCT_RE = re.compile(r"#\[derive\(GodotClass\)\]\s*\n\s*(?:#\[[^\]]*\]\s*\n\s*)*pub struct\s+(\w+)")

# ---------------------------------------------------------------------------
# 文件角色表（A 档判据①）：file -> (默认档, 原版模块, 理由/出处)
# 这张表就是"逐文件审计"的结论；函数级例外见 C_LIST / D_LIST / B 机判。
# ---------------------------------------------------------------------------
FILE_ROLE = {
    # —— GDExtension 入口 / 手写 C ABI / 报文装配（原版没有这些层）——
    "gdext.rs": ("A", "-", "GDExtension 入口（`gdext_rust_init`/`proc_address`；原版是 `#[gdextension]` 直接生成）"),
    "gdext_mod.rs": ("A", "-", "`#[gdextension]` 入口类 `GW2Rebuild` + 演示类注册（原版入口由宏生成，无手写层）"),
    "gdext_class.rs": ("A", "-", "手写 C ABI 类壳（`register_class`/`sym`/`create_instance`；原版用 `#[derive(GodotClass)]`）"),
    "capi.rs": ("A", "-", "`extern \"C\"` 导出面 `gw2_*`（原版没有 C ABI）"),
    "api.rs": ("A", "-", "内核→C/GDScript 的 `variant_call` 面 + 薄转发（`docs/06` §86.8 正例 A01：`hit_test` 转发、无第二份实现）"),
    "protocol.rs": ("A", "-", "报文装配：74 事件 `WIRE_SPEC`/编解码（原版是 serde 派生；本仓自造。C03 的 schema 重复见 note）"),
    "rng.rs": ("A", "-", "ChaCha12 实现（原版用 `rand_chacha` crate，二进制里没有这个源文件）"),
    "replay_inflate.rs": ("A", "-", "inflate/gzip 实现（原版用 `flate2` crate）"),
    "audio_wav.rs": ("A", "-", "WAV 解码（原版用 `hound` 这类第三方 crate，`docs/111` §4.4b）"),
    "broker.rs": ("A", "-", "我方对局服务器/LAN broker（原版 broker 是独立进程，不在这个 crate 里）"),
    # —— 对拍器 / 工具（原版没有）——
    "replaydiff.rs": ("A", "-", "回放对拍器（`examples/replay_diff`；原版没有这个工具）"),
    "diffconv.rs": ("A", "-", "第三轴 `batch_conv` 收敛层对拍器（`docs/06` §86.5 C06：工具定位，数字必须带轴名）"),
    "ai_explain.rs": ("A", "-", "攻略向 AI 评分解释工具（`examples/ai_explain`；C07 的第二份评分实现）"),
    # —— 我方自造的回放驱动 / 元数据 / 流程驱动（T57 §104.0 判「留原地是对的」）——
    "replay_driver.rs": ("A", "-", "我方回放驱动 `ReplayDriver`（整数 ns 定步；原版时钟在 `graphwar_replay_player`，C02 见 note）"),
    "replay_driver_class.rs": ("A", "-", "`GwReplayDriver`：上面那份驱动的 GDExtension 壳（类名 `Gw*`，非原版类名）"),
    "replay_meta.rs": ("A", "-", "回放头部元数据解析（我方；T36 判 B16 / T57 判 A 的冲突见 D_LIST）"),
    "replay_meta_class.rs": ("A", "-", "`GwReplayMeta`：回放列表元数据的 GDExtension 壳（类名 `Gw*`）"),
    "play_flow_class.rs": ("A", "-", "播放屏流程驱动的 4 个 `Gw*` 类（T36 §86.4a A03 正对照：`evaluate`/`next_shooter` 都转发；B18 见 D_LIST）"),
    "root_wiring.rs": ("A", "-", "我方接线容器 `RootWiring` + 替身事件通道 `KernelBus`（原版对象图是 节点→Backend→容器，T49 §97.7-1）"),
    # —— `Gw*` 前缀的 GDScript 桥接类（类名不是原版类名）——
    "audio_control_class.rs": ("A", "-", "`GwAudioControl` 桥（壳名 `Gw*`；规则在 `graphwar_audio_control`，T58）"),
    "color_manager_class.rs": ("A", "-", "`GwColorManager` 桥（规则在 `graphwar_color_manager`，T58 修了 `%180` 兜底）"),
    "endgame_class.rs": ("A", "-", "`GwEndGame` 桥（`winner_byte` 转发 `turnstate`；T36 A02 正例）"),
    "function_input_rules_class.rs": ("A", "-", "`GwFunctionEdit` 桥（规则在 `graphwar_function_input`，T42）"),
    "math_expr_class.rs": ("A", "-", "`GwMathExpr` 桥（解析/求值在 `math_function`：`0x4ed670`/`0x4eb9d0`）"),
    "net_constants_class.rs": ("A", "-", "`GwNetConsts` 桥（常量来自 `graphwar_client_connection_handler` / `graphwar_lan_broadcaster_v6`，T42）"),
    "stream_buffer_class.rs": ("A", "-", "`GwStream` 桥（规则在 `graphwar_stream_buffer`，T58）"),
    "net_events_class.rs": ("A", "-", "`GwNetEvents` 的 40 个 `#[func]` = GDScript 侧报文门面 + thread-local 通道（T49 §97.7-4：原版没有，永远留 scaffold）"),
    # —— B：原版类名 + Location 锚（机判复核；理由在这里写清）——
    "graphwar_setup_control_class.rs": ("B", "graphwar_setup_control",
                                        "`GraphwarSetupControl` = 原版类名（`_::__init::__inner_init 0x4bcc60` Location 落 `src/graphwar_setup_control.rs`）⇒ 类层该在模块文件里（T57 §104.0 点名的剩余 2 个之一）"),
    "graphwar_ad_switch_class.rs": ("B", "graphwar_ad_switch",
                                    "`GraphwarAdSwitch` = 原版类名（`_::__init::__inner_init 0x4e9c90` Location 落 `src/graphwar_ad_switch.rs:45:1`）⇒ 类层该在模块文件里（T57 §104.0 点名的剩余 2 个之一）"),
}

# 判定「原版类名」要用的类名清单：从 fixtures/name_to_addr_v2.tsv 第 2 列 + 落 src/*.rs 的 Location 锚
ORIG_STRUCTS = None  # 运行期填

# ---------------------------------------------------------------------------
# C_LIST：(file, fn) -> (另一处 Impl, 判据)
# ---------------------------------------------------------------------------
C_LIST = {
    ("diffconv.rs", "fire"):
        ("src/game.rs:1558 `Match::fire`", "复刻开火入口：自己造 `ShotInput` 再走本文件的 `fire_batch` 链（§86.5 C06）"),
    ("diffconv.rs", "fire_batch"):
        ("src/game.rs:1600 `Match::fire_batch`", "复刻整批结算入口（§86.5 C06）"),
    ("diffconv.rs", "fire_batch_with"):
        ("src/game.rs:1600 `Match::fire_batch` + `fire_functions`/`execute_results`", "结算路径本体第二份（几何并行 + 判定 + 广播；§86.5 C06）"),
    ("diffconv.rs", "tick"):
        ("src/game.rs:1523 `Match::tick`", "**逐字相同**（`identical_body_groups()` 机判）；另一份在 game.rs"),
    ("diffconv.rs", "explosion_point"):
        ("src/game.rs 的爆炸点判定（`0x4741a0` 一带，`Match::entity_effects` 登记链）", "爆炸点规则第二份：直接调 `math_function::parse/eval` 自己算（§86.5 C06 的同一族）"),
    ("diffconv.rs", "death_effect_type"):
        ("src/game.rs:170/820/1238/1246（`SparkEffect`/`AshEffect`/`DeathEffect` 的登记规则）", "皮肤名→特效类型的映射第二份（§86.5 C06）"),
    ("ai_explain.rs", "evaluate"):
        ("src/graphwar_function_evolutionator.rs:1212 `evaluate` / `fitness_eval`", "评分链第二份实现（§86.5 C07）"),
    ("replay_driver.rs", "drive_frame"):
        ("src/graphwar_replay_player.rs:745 `advance_dt`", "回放时钟两份：本文件整数 ns 定步 vs 模块侧 f64 累加（§86.5 C02；取整方式不同 ⇒ 永远差 ±1 tick）"),
    ("axis_drawer_class.rs", "clamp_mode"):
        ("src/_scaffold/gdext_mod.rs `clamp_mode`", "**同一规则两份**（i64 收窄 vs i32 收窄；T57 §104.7-4 登记的 C 档小项）"),
    ("gdext_mod.rs", "clamp_mode"):
        ("src/_scaffold/axis_drawer_class.rs:228 `clamp_mode`", "同上（两份实现）"),
    ("axis_drawer_class.rs", "line_primitive_count"):
        ("src/_scaffold/gdext_mod.rs `line_primitive_count`", "同一条计数规则两份（都经 `axis_drawer::axis_primitives`，但 filter/count 规则分成两处）"),
    ("gdext_mod.rs", "line_primitive_count"):
        ("src/_scaffold/axis_drawer_class.rs `line_primitive_count`", "同上（两份实现）"),
    ("gdext_mod.rs", "triples"):
        ("src/_scaffold/api.rs:1412 `triples`", "**逐字符相同**的 3 元组切分助手两份（机判）"),
    ("api.rs", "triples"):
        ("src/_scaffold/gdext_mod.rs:201 `triples`", "同上（两份实现）"),
    ("axis_drawer_class.rs", "sym"):
        ("src/_scaffold/api.rs:681 `sym`", "**逐字符相同**的符号查找助手两份（机判）"),
    ("api.rs", "sym"):
        ("src/_scaffold/axis_drawer_class.rs:431 `sym`", "同上（两份实现）"),
    ("graphwar_setup_control_class.rs", "join_text"):
        ("src/graphwar_button.rs:1157 `join_text`", "**逐字符相同**的 `a|b|c` 拼接助手两份（机判）；搬类时不要把这第二份带进模块"),
}

# ---------------------------------------------------------------------------
# D_LIST：(file, fn) -> (缺什么)
# ---------------------------------------------------------------------------
D_LIST = {
    ("play_flow_class.rs", "next_shooter"):
        "玩家之间的**轮转顺序**未知（`docs/06` §86.4 B18：原版是 HashMap 迭代序）；本函数把顺序固定成调用方给的序，士兵级那半转发 `PlayerTurns::next_shooter`（有据）。缺：原版玩家轮转的指令/观测证据。",
    ("play_flow_class.rs", "first_shooter"):
        "轮转起点：`cursor` 的初值与语义无原版证据（B18，原版是 HashMap 迭代序）。",
    ("play_flow_class.rs", "current_shooter"):
        "「当前射手」由我方 `cursor` 决定；原版口径未知（B18）。",
    ("play_flow_class.rs", "push_group"):
        "玩家分组表 `groups` 是我方模型（原版是 `HashMap<player, PlayerTurns>`）；缺原版容器形状证据（B18/E01）。",
    ("play_flow_class.rs", "setup"):
        "`GwTurnDriver` 的初始化（谁是第一个射手）缺原版证据（B18：轮转顺序/起点未知）。",
    ("play_flow_class.rs", "setup_flat"):
        "扁平化的分组初始化：分组表形状是我方模型（B18/E01），缺原版容器证据。",
    ("replay_meta.rs", "parse_head"):
        "T36 §86.4 判 B16（回放头部解析属 `graphwar_replay_reader` 职责）、T57 §104.0 判 A（我方自造元数据）—— **两处结论冲突**。缺：原版「回放列表显示关卡名」那段的函数名/Location。",
    ("replay_meta.rs", "parse_with_cap"):
        "解析本体：T36 判 B16 / T57 判 A 冲突；缺原版元数据读取那段的函数名/Location。",
    ("replay_meta.rs", "apply_line"):
        "逐行归约（GameNameInfo + 四种模式字段）：T36 判 B16 / T57 判 A 冲突；缺原版那段的函数名/Location。",
    ("replay_meta.rs", "string_field"):
        "字段抽取：同上冲突（B16 vs A），缺原版证据。",
    ("replay_meta.rs", "first_quoted"):
        "字符串扫描器：同上冲突（B16 vs A），缺原版证据。",
    ("replay_meta.rs", "from_raw"):
        "原始文本 → 元数据：同上冲突（B16 vs A），缺原版证据。",
    ("replay_meta.rs", "from_path"):
        "读盘 + gzip 头：同上冲突（B16 vs A），缺原版证据。",
}

# 逐函数补充证据/遗留（只改 evidence/note 文本，不改档位）
EXTRA_EVIDENCE = {
    ("graphwar_ad_switch_class.rs", "are_ads_enabled"):
        "原版方法 `GraphwarAdSwitch::are_ads_enabled` = `!owned`：arm64 2.0.60 ptrcall 实例 `0x377cf4` 的 `ldrb w22,[obj+0x18]` + `eor w8,w22,#1`（**观测**，arm64；Linux 侧无符号锚）",
    ("graphwar_ad_switch_class.rs", "get_ad_unit_id"):
        "原版方法 `GraphwarAdSwitch::get_ad_unit_id`：返回 `.rodata` 的 38 字节 AdMob 单元 id 字面量（**观测**；Linux 侧无符号锚，见模块文件 `AD_UNIT_ID`）",
    ("graphwar_setup_control_class.rs", "build_stub_children"):
        "⚠️ 本函数的前提已被 T43 推翻：`client/scenes/screens/game_setup.tscn` 已装上（节点 `GraphwarSetupControl/RoomName/RoomNameLineEdit` 由场景提供）⇒ 替身子节点是否还该建 = 待定；本轮只搬不改语义",
    ("graphwar_setup_control_class.rs", "ready"):
        "原版 `GraphwarSetupControl::ready 0x4cf670`：11 个 `get_node` + 3 条 `connect`（`docs/16` §16）",
}

# ABI / 验收钩子的函数名词表（A 档判据②的一部分；只用于 note，不改变档位）
ABI_NAMES = {
    "sym", "sn", "add_method", "add_property", "ctor", "read_i64", "write_int",
    "instance_mut", "create_instance", "free_instance", "register_class", "register_ping_method",
    "register_path_method", "register_path_mode_method", "register_hit_test_method",
    "register_level_obstacles_method", "register_axis_drawer_class", "proc_address",
    "gdext_rust_init", "on_initialize", "on_deinitialize", "request",
}
HOOK_RE = re.compile(
    r"(_count$|_calls$|^provenance$|_addrs$|^pings$|^ping_call$|^registered$|^instances$|"
    r"^model_matches$|^owned_raw$|^owned_field_offset$|^wired_count$|^missing_nodes$|"
    r"^cycle_probe$|^interaction_selftest$|^interaction_selftest_strings$|^installed$|"
    r"^applied$|^applied_of$|^last_calls$|^dump$|^record$|^reset$|^ensure_installed$|"
    r"^dispatch_installed$|^label_texts$|^victory_texts$|^banners_for$|^team_of_byte$|"
    r"^slot_status$|^demo_room$|^demo_targets$|^assemble_demo$|^assemble_demo_with_queue$)"
)


def read(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _strip_test_blocks(text):
    """去掉 `#[cfg(test)]` 包住的**项**（`mod tests {…}` / `#[cfg(test)] use …;`），
    但把被删字符换成等量换行 ⇒ **行号保持不变**。

    ⚠️ 不能取"第一个 `#[cfg(test)]` 之前"：`docs/06` §86.7 D04 的坑
    （`src/graphwar_play_screen_control.rs:324` 是一条 `#[cfg(test)] use`，后面还有 800 行生产代码）。
    本函数与 `tools/audit_src.py::_strip_test_blocks` 同算法，只是保行号。
    """
    out, i, n = [], 0, len(text)
    pat = re.compile(r"#\[cfg\(test\)\]")
    while True:
        m = pat.search(text, i)
        if not m:
            out.append(text[i:])
            break
        out.append(text[i:m.start()])
        j = m.end()
        mod_head = re.compile(r"\s*(?:pub\s+)?mod\s+\w+\s*\{").match(text, j)
        if mod_head:
            k, depth = mod_head.end() - 1, 0
            while k < n:
                if text[k] == "{":
                    depth += 1
                elif text[k] == "}":
                    depth -= 1
                    if depth == 0:
                        k += 1
                        break
                k += 1
            out.append("\n" * text.count("\n", m.start(), k))
            i = k
        else:
            use_head = re.compile(r"\s*(?:pub\s+)?use\s[^;]*;").match(text, j)
            end = use_head.end() if use_head else j
            out.append("\n" * text.count("\n", m.start(), end))
            i = end
    return "".join(out)


def prod_functions(text):
    """生产函数（测试块挖掉、行号保留）；返回 [(name, body, line, ordinal)]。"""
    cut = _strip_test_blocks(text)
    out = []
    seen = collections.Counter()
    for mm in FN_RE.finditer(cut):
        name = mm.group(1)
        i = cut.find("{", mm.end())
        if i < 0:
            continue
        depth = 0
        j = i
        while j < len(cut):
            if cut[j] == "{":
                depth += 1
            elif cut[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        seen[name] += 1
        out.append((name, cut[mm.start():j + 1], cut[:mm.start()].count("\n") + 1, seen[name]))
    return out


def load_orig():
    """fixtures/name_to_addr_v2.tsv → {leaf: [(module, func, vaddr, loc_file, line, col)]}"""
    table = collections.defaultdict(list)
    path = os.path.join(ROOT, "fixtures", "name_to_addr_v2.tsv")
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            c = line.rstrip("\n").split("\t")
            if len(c) < 7 or not c[3].startswith("0x"):
                continue
            mod, stru, func, vaddr, locf, ln, col = c[0], c[1], c[2], c[3], c[4], c[5], c[6]
            leaf = func.split("::")[-1]
            table[leaf].append((mod, func, vaddr, locf, ln, col))
    return table


def orig_struct_names():
    """原版类名 = `name_to_addr_v2.tsv` 第 2 列且该类有落 src/*.rs 的 Location 锚。"""
    names = set()
    path = os.path.join(ROOT, "fixtures", "name_to_addr_v2.tsv")
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            if line.startswith("#"):
                continue
            c = line.rstrip("\n").split("\t")
            if len(c) < 5 or not c[1]:
                continue
            if not c[3].startswith("0x"):
                continue
            if c[4].startswith("src/") and c[4].endswith(".rs") and not c[4].startswith("src/_scaffold/"):
                names.add(c[1])
    return names


def file_original_module(text):
    """从模块文档里取"本文件装的是哪个原版模块"（`[`crate::graphwar_x`]` 或 `super::x`）。"""
    m = re.search(r"crate::(graphwar_[a-z0-9_]+|entity_drawer|player_showcase|math_function|"
                  r"graphwar_plane|effect_drawer|obstacle_drawer|function_drawer|axis_drawer)", text)
    return m.group(1) if m else None


def classify_file(name, text, orig_structs):
    """返回 (默认档, 原版模块, 理由, 机判证据)。"""
    if name in FILE_ROLE:
        cls, mod, why = FILE_ROLE[name]
        return cls, mod, why, "文件角色表"
    # 不在表里 = 新文件：按机判走（B 判据三条 / 否则 A）
    m = STRUCT_RE.search(text)
    if m:
        cls_name = m.group(1)
        if cls_name in orig_structs and "#[gdextension]" not in text:
            return ("B", file_original_module(text) or "-",
                    "机判：`#[derive(GodotClass)] struct %s` = 原版类名且非 GDExtension 入口" % cls_name,
                    "机判 B")
        return ("A", "-", "机判：`#[derive(GodotClass)] struct %s` 不是原版类名（我方类）" % cls_name, "机判 A")
    return ("A", "-", "机判：无 GodotClass 派生的非原版模块文件", "机判 A")


def identical_body_groups(files):
    """跨文件逐字相同的函数体（机判 C 候选；人工确认后写进 C_LIST）。"""
    index = collections.defaultdict(list)
    for fname, text in files.items():
        for name, body, line, _ord in prod_functions(text):
            norm = re.sub(r"\s+", " ", body).strip()
            if len(norm) < 40:
                continue
            index[norm].append((fname, name, line))
    groups = {}
    for norm, rows in index.items():
        if len({r[0] for r in rows}) > 1:
            groups.setdefault((rows[0][0], rows[0][1]), []).append(
                ["%s:%s:%d" % r for r in rows])
    return groups


def build_rows(orig, orig_structs, files):
    rows = []
    rid = 0
    for fname in sorted(files):
        text = files[fname]
        dcls, dmod, why, how = classify_file(fname, text, orig_structs)
        for name, body, line, ordinal in prod_functions(text):
            rid += 1
            cls = dcls
            note = ""
            om, of, va, other = "", "", "", ""
            ev = why
            if (fname, name) in C_LIST:
                cls = "C"
                other, crev = C_LIST[(fname, name)]
                ev = crev
                ev += "（覆盖文件级 %s）" % dcls
            elif (fname, name) in D_LIST:
                cls = "D"
                ev = "缺证据：" + D_LIST[(fname, name)]
                ev += "（覆盖文件级 %s）" % dcls
            if cls == "B":
                om = dmod
                hits = [h for h in orig.get(name, []) if h[0] == dmod]
                if not hits and name == "init":
                    # 类的 `_::__init::__inner_init`：Location 直接落模块文件，是 B 的最硬锚
                    hits = [h for h in orig.get("__inner_init", []) if h[0] == dmod]
                if hits:
                    _m, of, va, locf, ln, col = hits[0]
                    ev += "；名字夹具：`%s` %s → Location `%s:%s:%s`" % (of, va, locf, ln, col)
                else:
                    ev += "；本函数是类层成员（无独立真名锚）"
                if HOOK_RE.search(name) or name in ABI_NAMES:
                    note = "我方验收钩子（性质 A，按 §104.1「与类同生共死」随类搬）"
                if (fname, name) in EXTRA_EVIDENCE:
                    ev += "；" + EXTRA_EVIDENCE[(fname, name)]
            elif cls == "A" and (fname, name) in EXTRA_EVIDENCE:
                note = (note + "；" if note else "") + EXTRA_EVIDENCE[(fname, name)]
            elif cls == "A":
                if name in ABI_NAMES:
                    note = "C ABI/注册入口"
                elif HOOK_RE.search(name):
                    note = "验收钩子"
                elif re.search(r"crate::(graphwar_|entity_drawer|math_function|plane|turnstate|game|events|"
                               r"axis_drawer|effect_drawer|function_|player_showcase|obstacle_drawer)",
                               body):
                    note = "薄转发（规则本体在 crate::<原版模块>）"
                if fname == "protocol.rs":
                    note = (note + "；" if note else "") + "C03：事件 schema 与 events.rs 重复（本任务禁区）"
            elif cls == "C":
                if fname == "protocol.rs":
                    note = "C03（禁区）"
            rows.append({
                "id": "T63-%03d" % rid,
                "file": "src/_scaffold/" + fname,
                "fn_line": line,
                "fn": name + ("#%d" % ordinal if ordinal > 1 else ""),
                "class": cls,
                "orig_module": om,
                "orig_func": of,
                "vaddr": va,
                "other_impl": other,
                "evidence": ev,
                "note": note,
            })
    return rows


def print_summary(rows, files):
    by = collections.Counter(r["class"] for r in rows)
    total = len(rows)
    print("分母（生产函数，与 tools/audit_src.py scaffold 同口径）：%d" % total)
    print("A-真自造 %d ｜ B-装游戏语义 %d ｜ C-重复实现 %d ｜ D-分不清 %d"
          % (by["A"], by["B"], by["C"], by["D"]))
    print()
    print("%-42s %5s %5s %5s %5s %5s  %s" % ("file", "prod", "A", "B", "C", "D", "默认档/理由"))
    for fname in sorted(files):
        sub = [r for r in rows if r["file"].endswith("/" + fname)]
        c = collections.Counter(r["class"] for r in sub)
        dcls, _m, why, _h = classify_file(fname, files[fname], orig_structs_global)
        print("%-42s %5d %5d %5d %5d %5d  %s" % (fname, len(sub), c["A"], c["B"], c["C"], c["D"], dcls))
    print()
    print("对照：explicit-A files=%d / explicit-B files=%d / 机制默认=%d"
          % (len([f for f in files if f in FILE_ROLE and FILE_ROLE[f][0] == "A"]),
             len([f for f in files if f in FILE_ROLE and FILE_ROLE[f][0] == "B"]),
             len([f for f in files if f not in FILE_ROLE])))


def cmd_control(files):
    """正/反对照：axis_drawer_class 全 A；entity_drawer_class（历史版）全 B；改类名后翻 A。"""
    ok = True
    # 正对照 1：axis_drawer_class（现役）全 A
    text = files.get("axis_drawer_class.rs")
    if text is None:
        print("FAIL: 找不到 src/_scaffold/axis_drawer_class.rs"); return 1
    rows = [r for r in build_rows(load_orig(), orig_struct_names(), {"axis_drawer_class.rs": text})]
    # 期望：**0 个 B**（T57 §104.3-4：24 个生产函数没有一个游戏语义）。
    # 允许 3 个 C：`clamp_mode` / `line_primitive_count` / `sym` —— T57 §104.7-4 自己就把
    # `clamp_mode` 登记成"C 档小项（同一规则两份）"，另两个是同一类的逐字相同助手。
    bad = [r for r in rows if r["class"] == "B"]
    cs = [r["fn"] for r in rows if r["class"] == "C"]
    print("[正对照1] axis_drawer_class.rs（现役 %d fn）→ A %d / C %d / B %d  %s（C 允许：%s）"
          % (len(rows), sum(1 for r in rows if r["class"] == "A"), len(cs), len(bad),
             "OK" if not bad else "FAIL", ", ".join(cs) or "-"))
    ok = ok and not bad
    # 正对照 2 / 反面对照：entity_drawer_class（历史版）
    try:
        hist = subprocess.run(
            ["git", "-C", ROOT, "show", "cda427c^:src/_scaffold/entity_drawer_class.rs"],
            capture_output=True, text=True, check=True).stdout
    except (subprocess.CalledProcessError, FileNotFoundError) as exc:
        print("FAIL: 取历史版 entity_drawer_class.rs 失败（%s）" % exc)
        return 1
    rows = build_rows(load_orig(), orig_struct_names(), {"entity_drawer_class.rs": hist})
    bad = [r for r in rows if r["class"] != "B"]
    print("[正对照2] entity_drawer_class.rs（历史版 %d fn）→ B %d / 非 B %d  %s"
          % (len(rows), len(rows) - len(bad), len(bad), "OK" if not bad else "FAIL"))
    ok = ok and not bad
    # 反面对照：把类名改成 Gw 前缀 → 应翻成 A
    fake = hist.replace("pub struct EntityDrawer", "pub struct GwEntityDrawer")
    fake = fake.replace("#[derive(GodotClass)]", "#[derive(GodotClass)]")
    rows = build_rows(load_orig(), orig_struct_names(), {"entity_drawer_class.rs": fake})
    by = collections.Counter(r["class"] for r in rows)
    expect_a = by["A"] == len(rows)
    print("[反面对照] 类名换成 GwEntityDrawer（%d fn）→ A %d / B %d  %s"
          % (len(rows), by["A"], by["B"], "OK（翻成 A 了）" if expect_a else "FAIL（仍判 B）"))
    ok = ok and expect_a
    print()
    print("对照结论：%s" % ("全部对上 —— 判法是「原版类名 + Location 锚」而不是「见 *_class.rs 就判 B」" if ok
                          else "有对不上的 ⇒ 台账不许提交"))
    return 0 if ok else 1


def main(argv=None):
    global orig_structs_global
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--tsv", action="store_true", help="打印台账 TSV")
    ap.add_argument("--control", action="store_true", help="正/反对照")
    ap.add_argument("--out", default=None, help="写 TSV 到文件（默认 stdout）")
    args = ap.parse_args(argv)

    files = {}
    for fn in sorted(os.listdir(SCAFFOLD)):
        if fn.endswith(".rs"):
            files[fn] = read(os.path.join(SCAFFOLD, fn))
    orig_structs = orig_struct_names()
    orig_structs_global = orig_structs

    if args.control:
        return cmd_control(files)

    rows = build_rows(load_orig(), orig_structs, files)
    if args.tsv:
        cols = ["id", "file", "fn_line", "fn", "class", "orig_module", "orig_func", "vaddr",
                "other_impl", "evidence", "note"]
        out = ["\t".join(cols)]
        for r in rows:
            out.append("\t".join(str(r[c]).replace("\t", " ") for c in cols))
        text = "\n".join(out) + "\n"
        if args.out:
            with open(args.out, "w", encoding="utf-8") as fh:
                fh.write(text)
            print("写 %s（%d 行）" % (args.out, len(rows)), file=sys.stderr)
        else:
            sys.stdout.write(text)
        return 0
    print_summary(rows, files)
    return 0


orig_structs_global = None

if __name__ == "__main__":
    sys.exit(main())
