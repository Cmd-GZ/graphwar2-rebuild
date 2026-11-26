#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""审计③：`fixtures/` 与测试的**期望值来源**分档（`AGENTS.md` §0.5 / `.agent-work/specs/AUDIT-3-tests.md`）。

产出 `fixtures/source_audit_tests.tsv`，并打印统计。

## 分档（四档，定义写死在这里，跑完不许改）

| 档 | 判据（期望值能否**独立于我方实现**复核） |
|---|---|
| `G-binary` | 期望值来自真二进制/真机/抓包：`tools/gen_fixtures.py` 用 unicorn 直接调 `libgraphwarrust.so`；
`.rodata` 槽 / 指令立即数 / ELF vaddr / arm64 `.symtab` 名 / `panic::Location`（ELF+`.rela.dyn` 扫出）；
frida 真机样本；真实抓包字节；真机真帧坐标。 |
| `G-corpus` | 期望值来自冻结回放语料（403 份快照）的统计量/逐字节内容。 |
| `S-self` | 期望值只能由我方实现给出：把我方公式/常量重抄一遍当期望（镜像）；我方 A 路径 vs 我方 B 路径
（parity）；我方 parse→我方 to_text 往返；在实现里定义的不变量（单调/排序/长度/确定性/no-panic）；
由我方实现在语料上跑出来的基线数字。⇒ **自洽，零复原证据**（有回归保护价值，但不是证据）。 |
| `U-unknown` | 判不出来（写清缺什么证据）。 |

## 判法（三步，全部可复跑）

1. **夹具**：找生成脚本（`tools/*.py`/`examples/*.rs`），读它的真值来源（unicorn/ELF/抓包 vs
   `cargo run`/我方 example）。手写的计划/台账**按它引用的证据**分档。
2. **测试条数**：`cargo test -- --list` 给出每一条测试的**模块路径**，据此把每条测试归到
   **它所在的源文件**（`tests/*.rs` 或 `#[path]` 引进来的 `src/*.rs`）。所以口径不是"二进制名字"，
   而是"测试写在哪个文件里" —— 这样 `#[path]` 混编不会把别人的测试算到本文件头上。
3. **每个源文件**：解析它 `#[test]` 块里的断言，按"期望值有没有**独立物证锚**"（`0x…` 地址、
   `.to_bits()`、`.rodata`/立即数/`.symtab`/Location/夹具路径）计锚定率；再叠加**人工复核表**
   （`MANUAL`，我逐文件读过 `tests/` 全部 70 个二进制；分歧处人工覆盖并写明理由）。

用法：
    python3 tools/audit_tests_scan.py                # 打印统计
    python3 tools/audit_tests_scan.py --write        # 同时写 fixtures/source_audit_tests.tsv
    python3 tools/audit_tests_scan.py --list <file>  # 指定 `cargo test -- --list` 的输出

## 这条判法可能漏什么（照实写）

* **锚定 ≠ 正确**：注里有地址但抄错了，本判法照样记 `G-binary`。本审计抽核了 `tests/` 的
  全部 70 个二进制 + `src/` 的 10 个高密度文件（见 `docs/06` §85 正对照），但不能保证 100%。
* **镜像公式未必带地址**：§58 的 `update_sprite` 就是镜像（`((k/60)*unit)`）而周边常量都带地址 ⇒
  纯自动锚定率会被带偏；这一条靠人工表纠正（`entity_drawer` 里那 1 条）。
* **`#[path]` 混编**：测试写在 `src/a.rs` 却编进 `tests/b.rs`，按模块路径归到 `src/a.rs` —— 这是
  有意的（"测试写在哪个文件"才是证据责任归属），但 `--list` 的路径前缀偶尔有歧义会归错。
* **被跳过的测试**：语料缺失时 `eprintln!("跳过…")` 的测试仍然计测试条数（它绿着），本判法
  按 0 证据但**仍算在 1772 里**，口径见输出里的 `skipped` 列。
"""

import argparse
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)

# ---------------------------------------------------------------------------
# 1. 夹具：名字 -> (类型, 档, 判据)
#
# 类型：expect  测试/客户端检查直接读它当期望值
#       truth   真值/取证数据表（工具用，或测试只拿来做一致性校验）
#       capture 真机/抓包原始样本
#       measure 我方 vs 真值 的对拍/测量结果
#       plan    预注册 / 台账 / 计划（不是期望值本身）
#       input   喂给两边的输入脚本（不是期望值）
# ---------------------------------------------------------------------------

FIXTURES = {}


def fx(name, kind, bucket, why):
    """登记一个夹具。why 里必须给"可复核判据"（生成脚本:行 / 命令 / 文档节）。"""
    FIXTURES[name] = (kind, bucket, why)


# === A. 测试/客户端**直接读它当期望值**的夹具（类型 expect） ===========================
# 判据：grep -rn 'fixtures/' tests/ src/ client/tools/ 命中的那些文件
fx("eval.tsv", "expect", "G-binary",
   "tools/gen_fixtures.py:56-64,136-143 用 analysis/gwemu.py(unicorn) 直接 call 0x4eb9d0 读回 f64 位模式")
fx("path.tsv", "expect", "G-binary",
   "tools/gen_fixtures.py:170-181 用 unicorn call 0x3c2530 读回点数组")
fx("parse.tsv", "expect", "G-binary",
   "tools/gen_fixtures.py:104-109 用 analysis/graphwar_curve.py::parse_function；该参照实现对二进制做过 90 例对拍（间接）")
fx("to_text.tsv", "expect", "G-binary",
   "tools/gen_to_text_fixture.py 头注释：unicorn 直接 call 0x4ec160 读回 String")
fx("to_text_roundtrip.tsv", "expect", "G-binary", "同上：两代文本都取自真二进制 0x4ec160")
fx("parse_matrix_truth.tsv", "expect", "G-binary",
   "tools/parse_matrix.py 头注释：x86 真二进制 0x4ed670/0x4eb9d0 由 unicorn 逐条指令模拟")
fx("protocol.jsonl", "expect", "G-binary", "真实抓包（tools/capture_net*.js）；tests/wire_format.rs:304 include_str!")
fx("protocol_match.jsonl", "expect", "G-binary", "真实抓包（对局中）；tests/wire_format.rs:305、tests/broker.rs:3 引用行号")
fx("protocol_death.jsonl", "expect", "G-binary", "真实抓包（死亡链）；tests/wire_format.rs:306")
fx("name_to_addr.tsv", "expect", "G-binary",
   "tools/name_bridge.py：arm64 .symtab 名 + x86 panic::Location(.data.rel.ro+.rela.dyn) 配出的唯一锚；52 处引用")
fx("module_funcs.tsv", "expect", "G-binary",
   "tools/module_map.py：capstone rip-relative xref + .eh_frame 边界 + src/*.rs 串 ⇒ 函数→文件归堆")
fx("module_map.tsv", "expect", "G-binary", "tools/module_map.py 输出（同上）")
fx("sym_match.tsv", "expect", "G-binary",
   "tools/sym_match.py build：arm64 .symtab 名 × 两份 .so 的字节特征对到 x86（--verify 用 326 条唯一锚交叉）")
fx("boundary_audit.tsv", "expect", "G-binary",
   "tools/boundary_audit.py：527 个可归属函数的 panic::Location 文件分布 + coverage-matrix 对照")
fx("ai_gen_parity.tsv", "expect", "G-binary",
   "tools/ai_gen_parity.py：注入 ChaCha key 后真二进制生成的 token 流（src/graphwar_function_evolutionator.rs:1592 读它）")
fx("ai_src_fix_plan.txt", "expect", "G-binary",
   "8 条差异台账，每条带反汇编地址（docs/06 §78）；src/graphwar_function_evolutionator.rs:618 引用")
fx("levelgen_draws.txt", "expect", "G-binary",
   "文件头『真实抓包：某次运行的随机流前 120 个 64 位值』；src/graphwar_level_generator.rs:367 读它")
fx("levelgen_obstacles.txt", "expect", "G-binary",
   "同一抓包批次的障碍生成结果（docs/11 §275/332）；src/graphwar_level_generator.rs:376 读它")
fx("ticks.tsv", "expect", "G-binary", "文件头『真实抓包：TickReply 的时间戳与 current_tick』（docs/14-turn.md:29）")
fx("net_event_handlers.tsv", "expect", "G-binary",
   "台账：12 条缺处理器事件，『原版收到它之后做什么』列带证据出处（docs/06 §67）")
fx("guardrails.json", "expect", "S-self",
   "三轴基线 647/647/637 是**我方实现**在 403 份语料上的输出（tools/bit_exact_attrib.py --check-baseline）；"
   "语料指纹（corpus 的 403 条 sha256）才是 G-corpus。tests/fire_batch_driver.rs 读它")
fx("replay_player84.txt", "expect", "S-self",
   "文件头明写『来源（全部由旧实现算出，不是新驱动自己给的）』⇒ 我方 GDScript GwReplayPlayer 的期望值")
fx("frame_match_events.txt", "input", "S-self",
   "预注册**输入脚本**（对局屏事件序列）——不是期望值；字段布局来自 docs/07 + 我方 protocol.rs")
fx("frame_endgame_events.txt", "input", "S-self", "同上（结算屏事件序列）")
for n in ("frame_diff_thresholds.txt", "frame_diff_thresholds_amend1.txt"):
    fx(n, "plan", "S-self",
       "预注册**门槛**是我方设定的（基线数字来自真帧对拍，但门槛本身是我方决策）⇒ 不是真值")
for n, why in [
    ("frame_rest_targets.txt", "13 屏元素 bbox 目标，数值来自真机真帧测量（tools/frame_capture.py --side original）"),
    ("replay_screen_targets.txt", "回放屏元素 bbox，来自真机真帧"),
    ("replay_lobby_targets.txt", "回放列表行元素 bbox，来自真机真帧"),
    ("play_screen_final_targets.txt", "对局屏六项 bbox，来自真机真帧"),
    ("static_screens_targets.txt", "静态两屏元素 bbox，来自真机真帧"),
]:
    fx(n, "expect", "G-binary", why)
fx("frame_diff_results_endgame.tsv", "measure", "G-binary", "两侧真帧对拍结果（原版侧=真机真帧）")

# === B. 真二进制/真机/抓包 取证数据（类型 truth/capture） =========================
for n, why in [
    ("name_to_addr_v2.tsv", "tools/name_bridge.py v2 口径（arm64 符号 + x86 Location）"),
    ("ai_params_crosscheck.tsv", "tools/ai_params_crosscheck.py：.rodata 常量逐位对表"),
    ("crossarch_tables.tsv", "三份原版构建 .rodata 数据表逐字节对照（tools/crossarch_tables.py）"),
    ("crossarch_funcs.tsv", "x86-64 与 arm64 真机器码 unicorn 同输入执行（tools/crossarch_funcs.py）"),
    ("bit_exact_sweep.tsv", "439 个观测级函数与真二进制逐位对拍（tools/bitexact_run.py + unicorn）"),
    ("bit_exact_sweep_diffs.md", "上表的真差异清单（真二进制 vs 我方）"),
    ("crossarch_plan.txt", "预注册：两路执行体的函数清单与输入集"),
    ("bit_exact_plan.txt", "预注册：439 函数清单 + 可执行性分类器（地址/Location 来自真二进制）"),
    ("bit_exact_inputs.txt", "输入集（工具生成的输入，不是期望值）"),
    ("parse_matrix.jsonl", "差分矩阵输入（原文 + 输入集）；真值在 parse_matrix_truth.tsv"),
    ("protocol_field_recount.tsv", "74 事件字段数重测（tools/protocol_recount.py 读序列化指令）"),
    ("boundary_audit.tsv", "527 函数 Location 分布 + coverage-matrix 对照"),
    ("unnamed_attrib.tsv", "未定名函数的归属（capstone xref，tools/unnamed_attrib.py）"),
    ("tag_sites83.tsv", "tag 构造点全扫（capstone）"),
    ("tag_sites91.tsv", "tag 构造点重扫"),
    ("tag_counts83.tsv", "tag 计数"),
    ("tag_counts91.tsv", "tag 计数重扫"),
    ("tag37_rescan88.tsv", "tag 37 四形态重扫（capstone）"),
    ("windows_name_to_addr.tsv", "Windows DLL 符号↔地址（tools/pe_dis.py / win_names.py）"),
    ("name_crate_audit.tsv", "arm64 .symtab 真名 × crate 归属审计"),
    ("module_audit.tsv", "模块审计（输入=ELF 符号/xref）"),
    ("module_map_propagated.tsv", "模块归属传播（输入=ELF xref）"),
    ("module_map_audit.tsv", "docs/02 §17 台账审计（真值=coverage-matrix + 归档路径实测）"),
    ("callgraph_indirect.tsv", "真二进制间接调用点（capstone）"),
    ("callgraph_roles.tsv", "真二进制函数角色归堆"),
    ("callgraph_roles_control.tsv", "同上（对照组）"),
    ("callgraph_indirect_bench70.tsv", "间接调用识别基准（真二进制字节）"),
    ("callgraph_indirect_cov70.tsv", "同上（覆盖）"),
    ("callgraph_indirect_discrim70.tsv", "同上（判别力）"),
    ("callgraph_indirect_pairs70.tsv", "同上（配对）"),
    ("unknowns_plot.tsv", "弹道绘图原语扫描（字面量→包装→GOT 槽→调用点）"),
    ("unknowns_levelrules.tsv", "LevelRules 归属（panic::Location 取证）"),
    ("unknowns_mask.tsv", "掩膜生命周期扫描"),
    ("unknowns_rank.tsv", "rank 段位/字段数取证"),
    ("audio_event_map.tsv", "真机 audio 句柄↔事件映射（tools/frida_audio_map.py + audio_replay_align.py）"),
    ("replay_clock_observations.tsv", "frida 真机读数（回放时钟，docs/06 §63）"),
    ("effectinfo_posy_sample.jsonl",
     "tools/posy_batches.py：真解析器 0x4ed670 + 真采样器 0x3c2530 跑出来的末点 vs 回放报文 pos_*"),
    ("nan_batches.tsv", "tools/nan_batches.py：真二进制 / 我方 / 回放 三方逐发对照（docs/06 §38）"),
    ("lan_ai_triple_sample.jsonl", "frida 真机样本（LAN+AI 三连，tools/lan_ai_triple.py）"),
    ("obstacle_destroy_pairs.jsonl", "回放语料里的障碍破坏对（tools/obstacle_room_extract.py）"),
    ("ai_behaviour_runs.tsv", "25 s 预算实测（真二进制侧跑，tools/ai_budget_probe.py）"),
    ("ai_behaviour_plan.txt", "预注册：docs/13 结论档位表 + 判据（地址来自真二进制）"),
    ("audio_event_map_plan.txt", "预注册：音频映射的探针计划"),
    ("view_layer_ledger.tsv", "台账：52 份原版 .gd 的 status/why/need（真值=recovered-src 逐字节）"),
    ("hybrid_names_prereg.md", "预注册：定名器方法与判据"),
    ("hybrid_names_prereg.sha256", "预注册件指纹"),
    ("seq_align_prereg93.md", "预注册：顺序对齐方法"),
    ("seq_align_prereg93.sha256", "预注册件指纹"),
    ("feature_cleanup78_prereg.md", "预注册：特征清理实验"),
    ("protocol_recount_plan.txt", "预注册：§66 先写死方法与口径"),
]:
    kind = "plan" if n.endswith((".txt", ".md", ".sha256")) and ("plan" in n or "prereg" in n) else "truth"
    if n.endswith(".jsonl"):
        kind = "capture"
    fx(n, kind, "G-binary", why)

for n, why in [
    ("frida_array_divergence_sample.jsonl", "frida 挂真游戏抓的采样器两装配点数组（docs/06 §30.1）"),
    ("frida_endx_sample.jsonl", "frida 真机样本（end_x/pos_x 分叉）"),
    ("frida_flip_normal_l05.jsonl", "frida 真机样本（flip=1，docs/06 §31）"),
    ("frida_flip_sample.jsonl", "frida 真机样本（flip）"),
    ("frida_ode_edge_l16.jsonl", "frida 真机样本（Ode 边界 L16）"),
    ("frida_ode_edge_l17.jsonl", "frida 真机样本（Ode 边界 L17）"),
    ("frida_ode_sample.jsonl", "frida 真机样本（Ode 采样器）"),
    ("frida_point_array_sample.jsonl", "frida 真机样本（点数组）"),
    ("frida_ptr_pair_sample.jsonl", "frida 真机样本（函数指针槽配对）"),
    ("frida_token_at_fire_sample.jsonl", "frida 真机样本（开火瞬间 token 流）"),
    ("frida_triple_capture_sample.jsonl", "frida 真机样本（三连抓）"),
]:
    fx(n, "capture", "G-binary", why)

# === C. 两侧真帧对拍结果（measure） =================================================
for n in sorted(FX_DUMMY := []):
    pass
for n in ["frame_diff_results.tsv", "frame_diff_blocks.tsv",
          "frame_diff_results_static.tsv", "frame_diff_blocks_static.tsv",
          "frame_diff_results_match.tsv", "frame_diff_blocks_match.tsv",
          "frame_diff_blocks_endgame.tsv",
          "frame_diff_results_rest.tsv", "frame_diff_blocks_rest.tsv",
          "frame_diff_AvsB_ours472path.tsv", "frame_diff_blocks_AvsB_ours472path.tsv",
          "frame_diff_AvsC_ours470pck.tsv", "frame_diff_blocks_AvsC_ours470pck.tsv",
          "frame_diff_BvsD_form_only.tsv", "frame_diff_CvsD_engine_only.tsv"]:
    fx(n, "measure", "G-binary",
       "两侧真帧对拍（原版侧=真机真帧）⇒ 差异量是实测，不是期望值")

# === D. 我方算法/我方实现在二进制或语料上的**计算结果**（S-self） ====================
for n, why in [
    ("callgraph_fingerprint.tsv", "我方指纹算法在真二进制上的输出（输入 G-binary，数字是我方算法的）"),
    ("callgraph_fingerprint_gpu_ctl348.tsv", "我方指纹 GPU 仿真的对照输出"),
    ("callgraph_fingerprint_gpu_ctl348_scal05.tsv", "同上（0.5 缩放）"),
    ("fingerprint_newev.tsv", "我方指纹法在新事件上的评测"),
    ("fingerprint_newev_coverage.tsv", "同上（覆盖）"),
    ("fingerprint_ablation63.tsv", "指纹消融实验（我方算法）"),
    ("fingerprint_ci63.tsv", "指纹置信区间（我方算法）"),
    ("fingerprint_holdout72.tsv", "指纹 holdout（我方算法）"),
    ("fingerprint_holdout72_samples.tsv", "指纹 holdout 样本（我方算法）"),
    ("fingerprint_range_audit63.tsv", "指纹范围审计（我方算法）"),
    ("feature_cleanup78_configs.tsv", "特征清理实验配置（我方）"),
    ("feature_cleanup78_ladder.tsv", "特征清理阶梯（我方）"),
    ("feature_cleanup78_coverage.tsv", "特征清理覆盖（我方）"),
    ("feature_cleanup78_coverage_pos_tgt.tsv", "特征清理覆盖正/靶集（我方）"),
    ("feature_cleanup78_greedy.tsv", "特征清理贪心（我方）"),
    ("feature_cleanup78_hits.tsv", "特征清理命中（我方）"),
    ("feature_cleanup78_scale.tsv", "特征清理规模（我方）"),
    ("hypothesis_sweep.tsv", "假设扫描结果（我方算法）"),
    ("hybrid_names.tsv", "混合定名器输出（我方算法；输入=两份 .so 字节）"),
    ("hybrid_names_conflicts.tsv", "定名冲突（我方算法）"),
    ("hybrid_names_conflicts_holdout.tsv", "定名冲突 holdout（我方算法）"),
    ("hybrid_names_eval.tsv", "定名评测（我方算法）"),
    ("hybrid_names_holdout_rows.tsv", "定名 holdout 行（我方算法）"),
    ("seq_align.tsv", "顺序对齐结果（我方算法；输入=两份 .so）"),
    ("seq_align_conv93.tsv", "顺序对齐收敛（我方算法）"),
    ("seq_align_truth93.tsv", "顺序对齐真值表（我方算法）"),
    ("campaign_anchors75.tsv", "战役锚点（我方匹配器给出）"),
    ("evidence_sources75.tsv", "证据来源台账（我方整理，内容是真二进制地址）"),
    ("evidence_holdout75.tsv", "定名 holdout（我方算法）"),
    ("sim_tensor71_configs.tsv", "我方仿真配置与命中"),
    ("sim_tensor71_ceiling.tsv", "我方仿真上限"),
    ("sim_tensor71_marginal.tsv", "我方仿真边际"),
    ("sim_tensor71_wallclock.tsv", "我方仿真墙钟"),
    ("settle_order_matrix_65.tsv", "我方结算顺序策略 × 三轴 × 语料（**我方实现**的命中率）"),
    ("batch_cross_probe_55.tsv", "三轴跨驱动 probe（我方驱动在语料上的读数）"),
    ("shots_vs_fail.tsv", "我方 replay_attrib 在语料上的失败率统计"),
]:
    fx(n, "measure", "S-self", why + "｜输入是物证、**数字是我方产出** ⇒ 只作回归保护")

# 兜底：清单里还剩的（没登记的）按"判不出"记，脚本会打印出来逼我补
DEFAULT = ("unknown", "U-unknown", "未登记：本判法没有它的生成链路证据")


# ---------------------------------------------------------------------------
# 2. 人工复核覆盖表（源文件 -> (G-binary, G-corpus, S-self, U, 理由)）
#
# 只写**读过后**改判的；没写的走自动锚定率。
# tests/ 的 70 个二进制我全部逐条读过（tests_dump），分歧在这里落地。
# ---------------------------------------------------------------------------

MANUAL = {
    # 源文件 : (G-corpus, S-self, U-unknown, 判据)   —— G-binary = 该文件总条数 − 这三项
    # tests/ 的 70 个二进制我全部逐条读过（.agent-run/tests_dump.txt），与自动锚定率的分歧落在这里。
    "tests/ai_explain.rs": (0, 4, 0,
        "4 条是确定性/种子敏感性/内部一致性（同境同种子必须同解、换种子必须不同、档位表能映射回分档）；"
        "其余钉难度分档/评分常数（docs/147 地址）"),
    "tests/diffconv_parity.rs": (0, 2, 0,
        "比的不是我方 vs 真值，是**我方收敛层 vs 我方内核**两条路径逐位一致 ⇒ 纯 S-self"),
    "tests/fire_batch_driver.rs": (3, 1, 0,
        "批内 +3 GameStateInfo / 一个批一次广播来自 130 份真回放实测（docs/141 §3）⇒ G-corpus；"
        "criterion_is_the_batch_not_the_file 是内部一致性"),
    "tests/golden.rs": (0, 1, 0,
        "parse/eval/path 三个夹具=unicorn 真二进制；end_to_end_shot_is_stable 只断言点数和弦长（不变量）"),
    "tests/entity_drawer.rs": (0, 1, 0,
        "18 条钉 .rodata 槽/指令立即数；sprite_transform_matches_update_sprite_arithmetic 把公式重抄当期望 "
        "⇒ docs/06 §58 那条活了 557 个 commit 的错断言（现由指令复核背书，形态仍是镜像）"),
    "tests/half_step.rs": (0, 5, 0,
        "probe 与生产逐位相同 / 语料不复检 / 构造用例退回次数 / stats 自洽都是我方自洽；"
        "ode_with_recheck_diverges_at_point_78_on_divx 与 documented_real_case 的期望值来自 path.tsv（真二进制）"),
    "tests/diff_conv.rs": (10, 1, 1,
        "值来自回放（逐位 / 修正文本 / 爆炸点）⇒ G-corpus；strict_policy 是策略自洽；real_corpus_* 缺语料时跳过"),
    "tests/misc_modules.rs": (0, 15, 2,
        "钉 .rodata/立即数的（token 正则 77 字节、优先级表、掩码 0x3fe27…）是 G；"
        "15 条是自洽（我方 parse→reorder→我方输出、round_trip、幂等、FIFO）；2 条判不出"),
    "tests/last_five.rs": (0, 2, 0,
        "arm64 .symtab 真名/地址/`.rodata` 字面量/Location 行号 ⇒ G；2 条是我方表内部一致（合计 8 之类）"),
    "tests/graphwar_game_state_manager.rs": (0, 4, 0,
        "钉 GameState 表/winner_byte/auto_fire 的 ⇒ G；4 条是我方两路径 parity（fire vs batch、纯几何、"
        "execute_results、handle_tick）"),
    "tests/graphwar_color_manager.rs": (0, 2, 0, "两张 6 色表/偏移/载荷来自 .rodata；tables_are_disjoint 是内部一致性"),
    "tests/graphwar_computer_player_manager.rs": (0, 2, 0, "分档边界/种群来自 .rodata；elite=pop/5 与名字池是内部推导"),
    "tests/graphwar_effect_manager.rs": (0, 2, 0, "寿命 3600/类型码/订阅来自立即数与 .rodata；tick_emits_nothing 等是行为自洽"),
    "tests/graphwar_entity_effect_manager.rs": (0, 2, 0, "皮肤前缀/类型映射来自 .rodata 串与表；两条是空表/计数自洽"),
    "tests/graphwar_graphics_manager.rs": (0, 2, 0, "三张表偏移 0x150/0x180/0x1b0 与 tag 位来自二进制"),
    "tests/campaign_data.rs": (0, 4, 0,
        "对 docs/archive/118 §7.1（每格带 C 立即数地址 / R 回放统计）⇒ G；"
        "4 条是『我方 kernel vs 我方 client / level_manager 副本』parity"),
    "tests/broker.rs": (0, 0, 1, "3 条对真实抓包 protocol_match.jsonl:83-86；room_table_expires 是内部语义（判不出档）"),
    "tests/sim_turns.rs": (14, 3, 0, "批内分组/顺序来自 130 份真回放（docs/141）；3 条是收集器不变量"),
    "tests/replay_diff.rs": (8, 2, 1, "回放字节是真值；两条比对器负样本=我方构造；1 条缺语料跳过"),
    "tests/to_text.rs": (0, 2, 0, "5 条对真二进制 0x4ec160 夹具；往返结构/语义两条是自洽"),
    "tests/graphwar_setup_control.rs": (0, 2, 0, "钉 .rodata 图标/名字表/长度立即数/抓包串 ⇒ G；2 条自洽"),
    "tests/graphwar_lan_vbox.rs": (0, 2, 0, "节点名/偏移/Location 锚来自 .rodata/.symtab；2 条自洽"),
    "tests/graphwar_module_container.rs": (0, 2, 0, "25/23/20 计数与块地址来自 .symtab；2 条是容器行为自洽"),
    "tests/player_showcase.rs": (0, 1, 0, "10 函数/21 锚点/偏移全部来自符号表与 Location"),
    "tests/graphwar_function_input.rs": (0, 1, 0, "23 锚/28 折叠表/字节立即数；1 条 unknowns_are_declared 自洽"),
    "tests/graphwar_plane.rs": (0, 0, 1, "变换/槽 0x9bc018…/符号名都是二进制物证；1 条 gdext 入口是推断"),
    "tests/server_match.rs": (0, 4, 0,
        "真 TCP 端到端只证明『我方服务端+我方客户端能跑完一局』⇒ 全是 S-self（判据是能跑通，不是与真值对拍）"),
    # --- src/ 侧（`#[path]` 混编后条数≠文件里的 #[test] 数，所以按实测条数记）---
    "src/_scaffold/protocol.rs": (0, 60, 0,
        "绝大多数对 fixtures/protocol*.jsonl 的**真实帧**逐字节比对 ⇒ G-binary；"
        "我方 encode→我方 decode 往返/未知字段原样回发/未知 kind 拒绝 是自洽"),
    "src/_scaffold/api.rs": (0, 43, 0,
        "契约测试：我方 API 与 core_bridge.gd 逐字一致 + 我方两条管线（api vs shot_points）互相相等 ⇒ S-self"),
    "src/_scaffold/replay_driver.rs": (0, 20, 0, "我方自造的 replay 驱动（_scaffold），期望值只能来自它自己 ⇒ S-self"),
    "src/graphwar_function_evolutionator.rs": (0, 10, 2,
        "主体对 fixtures/ai_gen_parity.tsv（注入 ChaCha key 后真二进制的 token 流）⇒ G；"
        "10 条是生成器内部不变量（长度/修复/代数），2 条判不出"),
    "src/game.rs": (5, 12, 0,
        "抓包协议行 + 三轴语料基线 ⇒ G/G-corpus；12 条是结算流程内部一致性"),
    "src/graphwar_turn_manager.rs": (6, 6, 0,
        "6 条 replay_1782…/1793… 的轮转序列是**真回放**里读出来的 ⇒ G-corpus；6 条是我方游标行为自洽"),
    "src/graphwar_stream_buffer.rs": (0, 4, 0, "帧格式字面量来自 .rodata；4 条是我方 read/write 往返"),
    "src/graphwar_play_screen_control.rs": (0, 18, 0,
        "winner_byte / panic 行 561 / 六档时长 来自二进制 ⇒ G；弹道生命周期与时钟状态机 18 条是"
        "我方重建的行为，期望值只能由我方模型给出"),
    "src/graphwar_team_manager.rs": (0, 4, 0, "少人侧/tag 位型来自二进制；4 条是分配过程自洽"),
    "src/graphwar_shot_calculator.rs": (0, 3, 0, "点数组/停止判据来自真二进制与夹具；3 条是内部一致性"),
    "src/graphwar_nodes.rs": (0, 3, 0, "11 thunk/符号/调用点来自二进制；3 条是表内部一致"),
    "src/graphwar_root_node.rs": (0, 2, 0, "15 屏 .rodata 地址 / init word 来自二进制；2 条是环回自洽"),
    "tests/graphwar_audio_control.rs": (0, 2, 0, "vtable/立即数/步长来自二进制；2 条是 mute/清队列的行为自洽"),
    "tests/graphwar_lan_broadcaster_v6.rs": (0, 1, 0, "组播地址/周期/TTL/结构 0xc0 来自二进制"),
    "tests/graphwar_server_broker_handler.rs": (0, 1, 0, "6 函数/阈值位型 ⇒ G；1 条 start/stop 行为自洽"),
    "tests/server_broker_request_connection.rs": (0, 1, 0, "帧长/身份串/Location ⇒ G；1 条读侧行为自洽"),
    "tests/win_rule.rs": (0, 4, 0,
        "胜负判据的**结构**（一条判据）已由 docs/150 指到二进制，但 4 条是『我方两种关卡走同一条路径』的自洽"),
    "tests/events.rs": (0, 0, 0, "dispatch 名表/判别值来自 0x3d38f0 解出（真二进制）"),
}

# 逐条读过 `tests/` 全部 70 个二进制后的完整人工分档（(G-corpus, S-self, U-unknown, 判据)；其余=G-binary）
MANUAL.update({
    "tests/axis_drawer.rs": (0, 0, 0, "常数/.rodata 颜色四元组/地址区间 0x4e05b0+0x670"),
    "tests/effect_drawer.rs": (0, 0, 0, "48 字节布局/1-600 定步/7 phase/槽 0x1ba860"),
    "tests/events.rs": (0, 0, 0, "dispatch 名表/判别值来自 0x3d38f0 解出（真二进制）"),
    "tests/evidence.rs": (0, 0, 0, "常量位型（.rodata 原始比特）"),
    "tests/graphwar_ad_switch.rs": (0, 0, 0, "字段偏移 0x18 / 38 字节 ad_unit_id / 6 函数"),
    "tests/graphwar_audio_control.rs": (0, 2, 0, "vtable/立即数/元素步长 0x18 来自二进制；2 条 mute/清队列行为自洽"),
    "tests/graphwar_backend.rs": (0, 0, 0, "18 方法/4 锚/字段偏移 0x110/0x228"),
    "tests/graphwar_button.rs": (0, 0, 0, "6 方法地址/信号槽 .rodata/偏移 0x18/0x40/0x50"),
    "tests/graphwar_client_connection_handler.rs": (0, 0, 0, "11 函数 arm64 size / 16 条 Location / 标签位 0x8000…"),
    "tests/graphwar_game_client.rs": (0, 0, 0, "36 函数/8 对请求构造器/命令键立即数 0xa/0x4 来自反汇编"),
    "tests/graphwar_game_recorder.rs": (0, 0, 0, "32 订阅/状态 6/路径字面量/一行 JSON 序"),
    "tests/graphwar_lan_broadcaster_v6.rs": (0, 1, 0, "组播地址/1s 周期/10s TTL/结构 0xc0 来自二进制"),
    "tests/graphwar_level_manager.rs": (0, 0, 0, "21 关权威表（docs/10 §4，C 列立即数）/存档 7 字节"),
    "tests/graphwar_lobby_control.rs": (0, 0, 0, "节点名/回调偏移 0x48/互斥按钮（cmp 结构）"),
    "tests/graphwar_local_player_manager.rs": (0, 0, 0, "5 条请求 tag 59/50/52/54/66 与载荷"),
    "tests/graphwar_long_press_animation.rs": (0, 0, 0, "7 函数/常量 .rodata 位型/几何 10.0/30.0/15.0"),
    "tests/graphwar_name_manager.rs": (0, 0, 0, "订阅/tag/载荷 0x20"),
    "tests/graphwar_nodes.rs": (0, 0, 0, "11 thunk 地址/arm64 符号/调用点"),
    "tests/graphwar_player_manager.rs": (0, 0, 0, "4 方法/载荷 8/级联 (6)"),
    "tests/graphwar_player_screen_control.rs": (0, 0, 0, "8 锚/22 行号/节点访问器（Location+xref）"),
    "tests/graphwar_rank_manager.rs": (0, 0, 0, "载荷 12/tag 66/67/表偏移 0x18"),
    "tests/graphwar_replay_control.rs": (0, 0, 0, "4 模式按钮/场景字面量/按 tick 推进"),
    "tests/graphwar_replay_reader.rs": (0, 0, 0, "字面量/目录名/帧格式来自 .rodata 与 gzip 真流"),
    "tests/graphwar_root_node.rs": (0, 0, 0, "15 屏 .rodata 地址 / 12 命令键长度 / init word 0x08010000"),
    "tests/graphwar_server_broker_handler.rs": (0, 1, 0, "6 函数/阈值位型 0x3f747ae147ae147b；1 条 start/stop 行为自洽"),
    "tests/graphwar_soldier_manager.rs": (0, 0, 0, "两列行/级联 tag 14/载荷 8"),
    "tests/graphwar_stream_buffer.rs": (0, 1, 0, "大端长度前缀 0x00 0x31/4096 上限/错误串 .rodata；1 条我方 read-write 往返"),
    "tests/graphwar_target_manager.rs": (0, 0, 0, "tag 14/23 + queue_end_turn_trigger 的内联判断"),
    "tests/graphwar_team_manager.rs": (0, 0, 0, "少人侧入队/tag 24/25 位型"),
    "tests/graphwar_texture_loader.rs": (0, 0, 0, "三表偏移/23/17/28 条 res:// 串/双斜杠 typo"),
    "tests/graphwar_video_writer.rs": (0, 0, 0, "track 名 13 字节/60 fps/偶数对齐"),
    "tests/live_hits.rs": (0, 0, 0, "士兵/障碍/命中点全部来自 tools/capture_net_raw.js 的一次真实对局抓包"),
    "tests/ode_stop.rs": (0, 0, 0, "停止判据逐指令（±25/±15、NaN、预算 20000）"),
    "tests/parse_stray_paren.rs": (0, 0, 0, "真值来自 unicorn 直接 call 0x4ed670"),
    "tests/plane.rs": (0, 0, 0, "变换式与掩膜语义来自 docs/08 的 Ghidra 伪代码（观测）"),
    "tests/play_control.rs": (0, 0, 0, "5 个 widget 取反/15 个 remote 事件名/行号 561"),
    "tests/print_roundtrip_chain.rs": (0, 0, 0, "输入=回放报文原文/位模式；舍入语义来自真二进制 {:.3}"),
    "tests/residual_fold.rs": (0, 0, 0, "期望值=回放报文原文与位模式（外部数据）"),
    "tests/server_broker_request_connection.rs": (0, 1, 0, "帧长 u16 大端/身份串/Location；1 条读侧行为自洽"),
    "tests/sym_match.rs": (0, 0, 0, "对 sym_match.tsv 的一致性 + 已知锚 0x3dca1c（夹具来自两架构字节）"),
    "tests/weak_caller.rs": (0, 0, 0, "39/35/26 单态化与 Location 行号来自符号表与 .rela.dyn"),
    "tests/wire_format.rs": (0, 0, 0, "三方对账：serialize_field 指令序 + WIRE_SPEC + 真实抓包"),
    "src/_scaffold/broker.rs": (3, 10, 0, "broker 是我方自造的服务端；协议形状来自抓包（G），行为/表语义是我方实现（S）"),
    "src/_scaffold/diffconv.rs": (0, 10, 0, "收敛层是我方实现；测试比的是它与内核是否一致 ⇒ S-self"),
    "src/_scaffold/replay_inflate.rs": (6, 6, 0, "gzip 真流（回放）是外部数据（G-corpus 型）；往返解压是我方实现"),
    # 自动锚定率对"常量从 .rodata 抄、断言里不写地址"的文件偏低，逐个补
    "src/function_token.rs": (0, 2, 0, "TOKEN_PATTERN 77 字节 /.rodata 正则与跳表 ⇒ G；2 条 match 顺序自洽"),
    "src/graphwar_replay_player.rs": (6, 6, 0, "回放驱动的 tick 推进来自真回放（G-corpus）；6 条是我方状态机自洽"),
    "src/graphwar_life_manager.rs": (0, 3, 0, "生命状态两表/位掩码来自 .rodata；3 条是表操作自洽"),
    "src/plane.rs": (0, 2, 0, "20 px/单位、1000×600 画布来自二进制常量；2 条是掩膜自洽"),
    "src/graphwar_mode_manager.rs": (0, 2, 0, "模式判别值/icon 表来自 .rodata；2 条是 setter 自洽"),
})

# `src/` 侧人工覆盖（锚定率不够准的几个大文件）
MANUAL_SRC = {
    "src/math_function.rs": "G-binary：求值器/解析器有 fixtures/eval.tsv|parse.tsv|parse_matrix_truth.tsv（unicorn）",
    "src/graphwar_path_calculator.rs": "G-binary：fixtures/path.tsv 由 unicorn call 0x3c2530 产出",
    "src/game.rs": "G-binary：protocol_match.jsonl 抓包行 + 三轴语料基线（自身另有一部分自洽断言）",
    "src/_scaffold/api.rs": "S-self：契约测试（我方 API 与 core_bridge.gd 逐字一致 + 我方两条管线互相相等）",
    "src/graphwar_function_evolutionator.rs": "G-binary：ai_gen_parity.tsv（真二进制 token 流）+ .rodata 参数表",
    "src/entity_drawer.rs": "G-binary：sprite_transform 的指令段与 .rodata 槽已按 §58 指令复核；单元测试多为常量钉",
    "src/graphwar_tick_handler.rs": "G-binary：16 666 666 ns 定步（立即数）+ frida 真机读数（docs/06 §63）",
}

# ---------------------------------------------------------------------------
# 3. 自动锚定率
# ---------------------------------------------------------------------------

ANCHOR_RE = re.compile(r"0x[0-9a-fA-F]{4,}|to_bits\(\)|\.rodata|rodata|\.symtab|symtab|vaddr|LOCATION|Location|"
                       r"锚|立即数|指令|偏移|跳表|fixtures/|include_str!")
ASSERT_RE = re.compile(r"\bassert(_eq|_ne|!|\()|assert_eq!|assert_ne!")
SELF_RE = re.compile(r"往返|round_?trip|self_consistent|self-consistent|不变量|invariant|parity|逐位相同|"
                     r"两次相同|幂等|idempot|单调|monoton|确定性")


def split_tests(body):
    """把一个源文件切成 [(test_name, body_text)]。"""
    out = []
    lines = body.split("\n")
    idx = [i for i, L in enumerate(lines) if re.match(r"\s*#\[test\]", L)]
    for k, i in enumerate(idx):
        end = idx[k + 1] if k + 1 < len(idx) else len(lines)
        chunk = "\n".join(lines[i:end])
        m = re.search(r"fn (\w+)", chunk)
        out.append((m.group(1) if m else "?", chunk))
    return out


def anchored_ratio(tests):
    """返回 (锚定断言数, 断言总数, 自洽关键词命中数)。"""
    a = t = s = 0
    for _n, chunk in tests:
        for L in chunk.split("\n"):
            if not ASSERT_RE.search(L):
                continue
            t += 1
            if ANCHOR_RE.search(L):
                a += 1
            elif SELF_RE.search(L):
                s += 1
    return a, t, s


DOC_RE = re.compile(r"真二进制|unicorn|抓包|真帧|反汇编|\.rodata|symtab|vaddr|Ghidra|真机|fixtures/")


def doc_provenance(body):
    """文件头（前 60 行 + 模块文档）里声明的物证来源标记数。"""
    head = "\n".join(body.split("\n")[:60])
    return len(DOC_RE.findall(head))


def auto_bucket(a, t, s, doc):
    """自动分档：锚定率优先；锚定率低但文件头自带 >=3 处物证来源声明时记 G-binary。"""
    if t == 0:
        return None
    r = a / t
    if r >= 0.30:
        return "G-binary"
    if doc >= 3 and (not s or s / t < 0.5):
        return "G-binary"
    if r <= 0.15 or (s and s / t >= 0.5):
        return "S-self"
    return "MIXED"


# ---------------------------------------------------------------------------
# 4. 测试条数：从 `cargo test -- --list` 输出按源文件归堆
# ---------------------------------------------------------------------------

def load_list(path):
    """`cargo test -- --list` 输出 -> [(test_name, binary_path)]。

    注意：测试名**会重名**（同一个 `#[path]` 进来的 `src/x.rs` 的测试会在多个二进制里各跑一遍，
    如 `graphwar_stream_buffer::tests::*` 出现在 4 个二进制里）⇒ 必须用 list，不能用 dict。
    """
    out = []
    cur = "?"
    for L in open(path, encoding="utf-8", errors="replace").read().split("\n"):
        m = re.match(r"\s+Running (?:unittests )?(\S+) \(", L)
        if m:
            cur = "src/lib.rs" if "unittests" in L else m.group(1)
            continue
        if L.endswith(": test"):
            out.append((L[:-6], cur))
    return out


def path_map_for(bin_path):
    """integration 测试文件里的 `#[path=...] mod name;` -> 真实源文件。"""
    mp = {}
    try:
        s = open(bin_path, encoding="utf-8", errors="replace").read()
    except OSError:
        return mp
    for m in re.finditer(r'#\[path\s*=\s*"([^"]+)"\]\s*(?:pub )?mod (\w+)', s):
        p = os.path.normpath(os.path.join(os.path.dirname(bin_path), m.group(1)))
        mp[m.group(2)] = p
    return mp


def attribute(rows):
    """把每条测试归到**它写在哪个源文件里**（不是它在哪个二进制里跑）。"""
    pmaps, per_src = {}, {}
    for tname, binpath in rows:
        segs = tname.split("::")
        seg = segs[0]
        if binpath == "src/lib.rs":
            if seg == "_scaffold" and len(segs) > 1:
                src = "src/_scaffold/%s.rs" % segs[1]
            else:
                src = None
                for cand in ("src/%s.rs" % seg, "src/_scaffold/%s.rs" % seg):
                    if os.path.exists(cand):
                        src = cand
                        break
                src = src or "src/<%s>.rs" % seg
        else:
            if binpath not in pmaps:
                pmaps[binpath] = path_map_for(binpath)
            src = pmaps[binpath].get(seg, binpath)
        per_src[src] = per_src.get(src, 0) + 1
    return per_src


# ---------------------------------------------------------------------------
# 5. 主流程
# ---------------------------------------------------------------------------

def controls():
    """正/负对照：3 个我确定是 G-binary 的 + 2 个我确定是 S-self 的，判法必须全判对。

    判据都不是"看起来像"：
      * path.tsv        —— tools/gen_fixtures.py:170-181 里 `g.call_path(...)` 是 unicorn 直调 0x3c2530；
      * protocol.jsonl  —— tools/capture_net.js 在真实对局进程里 hook libc socket 收下来的字节；
      * tests/parse_stray_paren.rs —— 文件头明写『真值来自真二进制 0x4ed670，不是我们自己的模型』；
      * tests/diffconv_parity.rs   —— 文件头明写比的是"我方收敛层 vs 我方内核"两条路径；
      * replay_player84.txt        —— 文件头明写『来源（全部由旧实现算出）』。
    """
    p = os.path.join(REPO, "fixtures", "source_audit_tests.tsv")
    if not os.path.exists(p):
        print("!! 先跑 --write 生成 TSV 再做对照")
        return 2
    rows = [L.split("\t") for L in open(p, encoding="utf-8").read().split("\n") if L.strip()]
    idx = {r[1]: r for r in rows[1:]}
    cases = [
        ("fixtures/path.tsv", "G-binary", "unicorn 直调 0x3c2530（gen_fixtures.py:176）"),
        ("fixtures/protocol.jsonl", "G-binary", "真实抓包字节（capture_net.js）"),
        ("tests/parse_stray_paren.rs", "G-binary", "文件头：真值来自真二进制 0x4ed670"),
        ("tests/diffconv_parity.rs", "S-self", "我方收敛层 vs 我方内核"),
        ("fixtures/replay_player84.txt", "S-self", "文件头：全部由旧实现算出"),
    ]
    bad = 0
    for name, want, why in cases:
        r = idx.get(name) or idx.get(name.replace("fixtures/", ""))
        got = r[3] if r else "<missing>"
        # 期望"整条都是 X"：单值档直接比；拆分档要求 X 的条数 > 0 且其它档为 0
        m = re.match(r"G-binary (\d+) / G-corpus (\d+) / S-self (\d+) / U (\d+)", got)
        if m:
            vals = dict(zip(("G-binary", "G-corpus", "S-self", "U"),
                            [int(x) for x in m.groups()]))
            ok = vals.get(want, 0) > 0 and sum(v for k, v in vals.items() if k != want) == 0
        else:
            ok = got == want
        print("%s  %-32s want=%-9s got=%-40s  [%s]" % ("PASS" if ok else "FAIL", name, want, got, why))
        bad += 0 if ok else 1
    print("\n正对照 %d/%d 通过" % (len(cases) - bad, len(cases)))
    return 0 if bad == 0 else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--list", default=os.path.join(REPO, ".agent-run", "testlist_default.txt"))
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--controls", action="store_true")
    args = ap.parse_args()

    rows = []
    stats = {"fixtures": {}, "tests": {}}

    # ---- 5a. 夹具 ----
    fxdir = os.path.join(REPO, "fixtures")
    names = [n for n in sorted(os.listdir(fxdir)) if n != "source_audit_tests.tsv"]  # 本审计自己的产物
    for n in names:
        kind, bucket, why = FIXTURES.get(n, DEFAULT)
        rows.append(("F", n, kind, bucket, why, 1))
        stats["fixtures"][bucket] = stats["fixtures"].get(bucket, 0) + 1

    # ---- 5b. 测试（按源文件归堆）----
    per_src = attribute(load_list(args.list))
    total = sum(per_src.values())
    for src in sorted(per_src):
        cnt = per_src[src]
        if src.startswith("src/lib.rs<unmapped"):
            rows.append(("T", src, "测试", "U-unknown", "模块路径在 src/ 找不到同名文件", cnt))
            stats["tests"]["U-unknown"] = stats["tests"].get("U-unknown", 0) + cnt
            continue
        if not os.path.exists(src):
            rows.append(("T", src, "测试", "U-unknown", "源文件不存在（可能是宏生成/移动过）", cnt))
            stats["tests"]["U-unknown"] = stats["tests"].get("U-unknown", 0) + cnt
            continue
        body = open(src, encoding="utf-8", errors="replace").read()
        tests = split_tests(body)
        a, t, s = anchored_ratio(tests)
        doc = doc_provenance(body)
        man = MANUAL.get(src)
        if man:
            Gc, S, U, why = man
            Gc, S, U = [min(v, cnt) for v in (Gc, S, U)]
            G = cnt - (Gc + S + U)
        elif src in MANUAL_SRC:
            why = MANUAL_SRC[src]
            G, Gc, S, U = cnt, 0, 0, 0
        else:
            b = auto_bucket(a, t, s, doc)
            if b is None:
                G, Gc, S, U, why = 0, 0, 0, cnt, "无断言（只有构造/打印）"
            elif b == "G-binary":
                G, Gc, S, U, why = cnt, 0, 0, 0, \
                    "自动锚定率 %.0f%%（%d/%d 断言带锚）+ 文件头物证来源标记 %d 处" % (100.0 * a / t, a, t, doc)
            elif b == "S-self":
                G, Gc, S, U, why = 0, 0, cnt, 0, \
                    "自动锚定率 %.0f%% + 自洽关键词 %d 条" % (100.0 * a / t, s)
            else:
                G = round(cnt * a / t)
                S = cnt - G
                Gc = U = 0
                why = "混合：自动锚定率 %.0f%%（%d/%d）⇒ 按比例拆" % (100.0 * a / t, a, t)
        if a or t:
            why += "｜断言 %d 条，带锚 %d 条" % (t, a)
        rows.append(("T", src, "测试", "", why, cnt))
        rows[-1] = ("T", src, "测试", "SPLIT", why, cnt)
        for k, v in (("G-binary", G), ("G-corpus", Gc), ("S-self", S), ("U-unknown", U)):
            stats["tests"][k] = stats["tests"].get(k, 0) + v
        rows[-1] = ("T", src, "测试",
                    "G-binary %d / G-corpus %d / S-self %d / U %d" % (G, Gc, S, U), why, cnt)

    # ---- 5c. 输出 ----
    hdr = ["id", "夹具/测试", "类型", "期望值来源档", "判据", "条数"]
    out = ["\t".join(hdr)]
    for i, r in enumerate(rows, 1):
        out.append("\t".join([r[0] + "%03d" % i, r[1], r[2], r[3], r[4].replace("\t", " "), str(r[5])]))
    text = "\n".join(out) + "\n"
    if args.write:
        p = os.path.join(fxdir, "source_audit_tests.tsv")
        open(p, "w", encoding="utf-8").write(text)
        print("wrote", p, len(rows), "rows")

    if args.controls:
        return controls()

    print("\n== 夹具（按条数）==")
    for k in ("G-binary", "G-corpus", "S-self", "U-unknown"):
        print("  %-10s %d" % (k, stats["fixtures"].get(k, 0)))
    print("  合计 %d" % len(names))
    print("\n== 测试（按测试条数，分母 = --list 总数 %d）==" % total)
    for k in ("G-binary", "G-corpus", "S-self", "U-unknown"):
        print("  %-10s %d" % (k, stats["tests"].get(k, 0)))
    print("  合计 %d" % sum(stats["tests"].values()))
    # 夹具按类型再切一刀
    bykind = {}
    for n in names:
        kind, bucket, _ = FIXTURES.get(n, DEFAULT)
        bykind.setdefault(kind, {}).setdefault(bucket, 0)
        bykind[kind][bucket] += 1
    print("\n== 夹具（按类型×档）==")
    for kind in ("expect", "capture", "truth", "measure", "plan", "input", "unknown"):
        if kind in bykind:
            print("  %-8s %s" % (kind, dict(sorted(bykind[kind].items()))))
    missing = [n for n in names if n not in FIXTURES]
    if missing:
        print("\n!! 未登记的夹具（按 U 记，请补 FIXTURES）：")
        for n in missing:
            print("   ", n)
    return 0


if __name__ == "__main__":
    sys.exit(main())
