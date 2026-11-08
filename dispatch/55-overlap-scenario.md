# 55 · `agent/overlap-scenario`：把 **≥6 发回合**的整批崩掉查清（重叠假设）

> **2026-11-09 第三次更新**：样本已经够了（5 份同时回合 replay 在 `dispatch/replays/`），
> 而且上游已经把规律量化到"**≥6 发是分水岭**"。本任务不再"造场景"，直接**查机制**。

**优先级 P0。**

## 0. 上游已测的量化结论（**先读，别重推**）

把手上的同时/顺序回合批按"每回合发数"分组（`docs/06` §33.4f）：

| 每回合发数 | 批数 | 对拍失败 | 含 `>1e-3` 大差 |
|---|---|---|---|
| **≤3 发** | 17 | 6（35%） | **1** |
| 4–5 发 | 1 | 1 | 1 |
| **≥6 发** | **8** | **8（100%）** | **5** |

大差形态：**整条弹道的停点/落点都不同**（`end_x` 7.001 vs 7.377 = 37.5 步；`pos_y` 11.06 vs 10.50），
**不是末位差** —— 与 §25/§33 那类 1 ulp 是两回事。

## 1. 要做什么（三件事，按顺序）

### A. 判"是否串了"（**核心判据**）

对 `dispatch/replays/` 里**每份同时回合 replay**、**每个 ≥6 发的批**：

```
把该批每一发分别用我方采样器跑（各自的 FUNC / x0 / y0 / flip / 障碍/边界快照）
对每一发，检查回放给出的 `FunctionPoints.end_x` / `EffectInfo.pos_x`
是否命中：
  (a) **这一发自己**的点列  → 我方停点判据错（障碍/边界/预算哪一条）
  (b) **同一批别的某一发**的点列 → **串了**（重叠假设成立，并指出"哪一发被哪一发顶掉"）
  (c) 都不命中 → 停点被别的东西改了（转 60 号）
```

⚠️ 用 `examples/replay_endx_probe.rs`（`--file <名> --batch N --tail 6`）就能看"命中我方第几个点"，
但**它只跑 owner 自己的那一发**；要判 (b) 必须**把同一批所有发都跑一遍再互相比对** ——
这是本任务要补的工具。

### B. 并行要求（**必须真的吃满 16 核**）

```
· 5 份 replay × 每个 ≥6 发批 × 每发一次采样 ⇒ 几百次采样 + 交叉比对
· 按 (replay, batch, shot) 切成独立任务，进程池 --jobs ≥ 16
· 汇报必须给 墙钟时间 与 相对单进程的加速比；墙钟接近串行 ⇒ 不算完成
```

### C. 给结论

```
重叠假设：成立 / 不成立 / 部分（按 A 的 (a)(b)(c) 计数）
若成立：给出"谁顶掉谁"的规律（例如后发的数组覆盖前发、或后发的 end_x 借用了前发的缓冲）
若不成立：说明大差还能是什么（并把线索交给 60 号）
```

## 2. 纪律

- **负结论要有正对照**：用 `1794158844`（顺序回合 1 发/回合、7/10 命中）跑同一套流程，
  它应当**不出现**几十步长的大差；
- 样本用完删除（`git rm -r dispatch/replays && rm -rf dispatch/replays`），只把**数值**写进 `docs/06`；
- 数字不许退：`cargo test --offline` **1659**、三轴 `fire 647/1075`、`batch 647/1075`；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/overlap-scenario`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 样本：`dispatch/replays/` 下 5 份（4 份同时 + 1 份顺序作对照）
- 工具：`examples/replay_attrib.rs`、`examples/replay_endx_probe.rs`、`tools/replay_state.py --index N`、
  `tools/frida_point_array.*`（若要运行时直证）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/overlap-scenario`

## 4. 可以改的文件

- `tools/`、`examples/`（只加不改）、`fixtures/`（只入文本摘要）
- `docs/06-verification.md` 追加一节
- ❌ 不许改 `src/` 结算逻辑、`client/`、`dispatch/`（最后删 `dispatch/replays/` 除外）

## 5. 验收

```text
① 覆盖了哪几份 replay、几个 ≥6 发批、共几发
② (a)/(b)/(c) 各多少（逐批列表）
③ 并行：墙钟 + 加速比
④ 重叠假设判决（成立/不成立/部分）+ 证据
⑤ 正对照：顺序回合那份不出现几十步长的差
⑥ dispatch/replays/ 已删除（贴 git status）
⑦ cargo test 1659；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、覆盖量与并行加速比、(a)/(b)/(c) 计数、判决、正对照、是否已删样本、`cargo` 结果。
