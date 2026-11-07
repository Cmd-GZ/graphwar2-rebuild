# 09 · `agent/fx-explosion-y`：爆炸点 `pos_y` 末位差（**单项收益最大**）

**优先级 P0。** 归因表里占失败 **170 / 512 批 = 33.2%**，预计修好可把对拍从 56.3% 推到 **68.9%**。

## 现象

`EffectInfo` 的 `pos_y` 与回放差**最后几位**（不是量级差、不是符号错）：

```text
我方 = 6.11832353472057
回放 = 6.118000000000002
```

`pos_x` 多数对得上，`pos_y` 系统性差一点 ⇒ 怀疑**爆炸点的 y 不是弹道停点的 y**，
而是经过了某次变换（投影/取整/翻转/修正常数）。

## 已知（别重推）

- 爆炸点是 `execute_results` 里两处 `EffectManager::add_effect`（arm64 `0x4a89ec` / `0x4a8cb0`）；
- 坐标来自 `PlanePosManager::get_position`（`docs/14` §11 的直接调用表）；
- `end_x` 已实测 "回放命中我方 `pts[i-1]`、爆炸 `pos_x` 命中 `pts[i]`" ⇒ **两者不是同一个点**；
- 归因表在 `dispatch/reports/CG-2026-11-07.md` §2，工具是 `tools/bit_exact_attrib.py`。

## 要做什么

1. 用 `tools/bit_exact_attrib.py` 拉出这 170 批，**按差值形态分组**（差几个 ulp？是否都是"回放更整齐"？）；
2. 挑 **5 个代表样本**，把回放里的 `pos_y`、我方 `pts[i].y`、`pts[i-1].y`、`end_y` 并列，找规律；
3. **回到二进制**：`0x4a89ec` / `0x4a8cb0` 两处 `add_effect` 的 `pos_y` 实参是怎么算的
   （逐条指令，别读伪代码；`objdump` 为准）。
4. 落地修正，并在同一份快照上量化 before/after。

## 允许改的文件

- `src/game.rs`（结算/爆炸点坐标那一段）
- `src/graphwar_effect_manager.rs` / `src/graphwar_entity_effect_manager.rs`（若坐标在那里）
- `tests/`、`examples/`（可加，不改既有）
- `docs/` 对应主题追加一节

⚠️ 不许改别人的 `*_class.rs`、`client/`。

## 验收

```text
同一份 401 份定格快照：
  before: 818 / 1080（种类序列）、21 / 1080（内容）
  after : 必须给出数字；若"内容"没涨而"种类"涨了，说明只修了位置没修值 —— 如实说明
cargo test --offline 全绿（基线 1512）
python3 tools/coverage_matrix.py --check OK
```

## 汇报（≤8 行）

分支+sha、**差值形态分组结果**、二进制里 `pos_y` 的算法（一句话 + 地址）、
before/after 两组数字、剩下多少批仍差。
