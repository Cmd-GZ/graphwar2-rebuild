# 55 · `agent/overlap-scenario`：用**现成的同时回合样本**分析「结算与开火重叠」

> **2026-11-09 更新**：用户已新录两把 AI 对决，其中 **`dispatch/replays/1794158991` 就是同时回合**
> （5 发全部 `TICK 22734`），**不需要再去造场景了**。任务目标从"造样本"改成"把这份样本吃透"。
> ⚠️ 用完**必须**删掉 `dispatch/replays/`（见那里的 README）。

**优先级 P0 —— 这是现在唯一还有"大差"的样本。**

## 0. 现有事实（上游已测，都带数）

```
1794158991（同时回合，5 发同 tick）
  · 对拍 **0 / 4** 批全灭（对照：顺序回合那份 7 / 10）
  · 批#1：end_x 我方 7.001101170725498 / 回放 7.376882420725504
         差值 **0.375 = 37.5 个步长（0.01）**，且 `pos_x == end_x`
  · 批#1 另一发：end_x 差 **0.000234**（23.4 个步长），同样 `pos_x == end_x`
  · 其余批是 1 ulp 级末位差（与顺序回合同形态）
```

⇒ 两级问题并存：**① 1 ulp（已证不是我们的错）**、**② 停点差几十个步长（同步发之间的干扰）**。
**本任务只做 ②。**

## 1. 要做什么

1. **先复现**：`--dir dispatch/replays` 跑对拍，确认 0/4 与那两个 0.375 / 0.000234 的差；
2. **逐发对照**：把同一 tick 的 5 发**分别**用我方采样器跑（各自的 `FUNC` / `x0` / `y0` / `flip`），
   看**回放的 `end_x` 是否命中任何一发自己的点列**：
   ```
   · 若命中"自己的"点 ⇒ 我方停点判据错（障碍/边界/预算哪一条）
   · 若命中"别人那一发"的点 ⇒ **串了**（重叠假设成立，方向对）
   · 若都不命中 ⇒ 停点被别的东西改了（转 60 号）
   ```
3. **抓运行时**（可选但强烈建议）：同 tick 多发时，在两处装配点（`0x4569b8` / `0x472865`）
   同时下断，看同一块内存是否被两发交替写；
4. **给结论**：重叠假设 **成立 / 不成立 / 部分**，并写清是哪一发影响了哪一发。

## 2. 纪律

- **负结论要有正对照**：用顺序回合那份（`1794158844`）跑同一套流程，它应当**不出现**几十个步长的差；
- 样本用完删除；只把**数值**写进 `docs/06`；
- 数字不许退：`cargo test --offline` **1659**、三轴 `fire 647/1075`、`batch 647/1075`；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/overlap-scenario`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 样本：`dispatch/replays/{1794158991（同时）,1794158844（顺序，作对照）}`
- 现成工具：`examples/replay_attrib.rs`、`examples/replay_endx_probe.rs`（`--file <名> --batch N --tail 6`）、
  `tools/replay_state.py --index N`（取某发的局面）、`tools/frida_point_array.*`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/overlap-scenario`

## 4. 可以改的文件

- `tools/`、`examples/`（只加不改）、`fixtures/`（**只入文本摘要**）
- `docs/06-verification.md` 追加一节
- ❌ 不许改 `src/` 结算逻辑、`client/`、`dispatch/`（除了最后删 `dispatch/replays/`）

## 5. 验收

```text
① 复现：0/4 与两个大差（0.375 / 0.000234）
② 5 发各自的"回放 end_x 命中谁的点列"结论
③ 重叠假设：成立 / 不成立 / 部分（附证据）
④ 正对照：顺序回合那份不出现几十步长的差
⑤ dispatch/replays/ 已删除（贴 git status 证明）
⑥ cargo test 1659；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、复现结果、5 发的命中归属、重叠假设判决、正对照结果、
`dispatch/replays/` 是否已删、`cargo` 结果。
