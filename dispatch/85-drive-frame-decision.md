# 85 · `agent/drive-frame-wire`：`drive_frame` **接不接**的决策与落地

**优先级 P1。80 号给了代价、81 号立了 tick 驱动 ⇒ 现在是纯决策题，别再来一轮"评估"。**

## 0. 已知（不要重测）

```
80 号量到的真值：**60.00 tick/s 恒定**、**pumps ≡ ticks**、**delta 确实没读**（`docs/06` §44.x）
80 号列的代价：接上会让 **5 个读数**变化（报告里逐条列了）
81 号已把 `GwReplayDriver` 做成"按 tick 推进"（不按帧），并有 headless 断言
67 号当初不接的理由："会静默改 `EntityDrawer` 的 `V`"
```

## 1. 先做决策，写成一页（**这部分不许含糊**）

| 方案 | 说明 | 代价 |
|---|---|---|
| **A 不接** | 播放屏继续按帧驱动视图；`drive_frame` 只服务 headless | 与"pumps ≡ ticks"的真值不一致（视图层每帧泵一次） |
| **B 接** | 播放屏的 `_process` 调 `KernelBus::drive_frame` | 5 个读数会变；`EntityDrawer.V` 可能被静默改 |

**决策要求**：给出**判据**（哪个方案更接近原版），并回答三个具体问题：
```
① 原版播放屏是"每帧泵一次"还是"每 tick 泵一次"？（80/81 的证据已经能回答，引它）
② 若接，`V` 到底会不会变？**先写断言再动手**（不许事后补）
③ 5 个读数变化后，新的基线是多少？（必须给出数，否则下一轮无法判断是否退）
```

## 2. 若选 B，落地要求

1. 接线**只加一个调用点**，并加**门**（"对局中/回放中"才泵），不许无条件泵；
2. **先写 5 条读数断言**（改前先让它们红，改后让它们绿——这就是"先写断言"的判据）；
3. headless 里给一条"tick 数 == 泵次数"的断言（把 80 号的真值变成结构约束，
   与 81 号在回放驱动上做的事同口径）；
4. 更新 `docs/06`/`docs/16` 里受影响的读数，**并保留旧值一行**（说明为什么变）。

## 3. 若选 A，验收要求

1. 写清"为什么不接"以及**代价的量化**（5 个读数现在与真值差多少）；
2. 在 `docs/16` 里写一节"此处**故意**不接"，防止下一轮有人再提；
3. 同一条约束在 headless 里写成断言（"播放屏**不许**调 drive_frame"）——用代码把决策钉住。

## 4. 硬约束

- `cargo test --offline` **1753**；三轴 `fire 647 / batch 647 / batch_conv 637`；
- headless 1890 条（14 个 SceneTree 脚本）；`selftest` 714/0/1skip（两种降级见 `docs/18` §10.3.3）；
- headless 只跑 `extends SceneTree` 的脚本；
- 提交信息用 `-F 文件`。

## 5. 环境

- 分支 `agent/drive-frame-wire`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 参考：`src/_scaffold/process_loop*`、`src/_scaffold/replay_driver*`、`client/src/screens/play_screen.gd`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/drive-frame-wire`

## 6. 可以改的文件

- `client/src/screens/play_screen.gd`（只加一个调用点 + 门）
- `src/_scaffold/process_loop*`、`client/tools/process_loop_check.gd`、`client/tools/play_flow_check.gd`
- `docs/06`、`docs/16`
- ❌ 不许改 `src/game.rs`、`dispatch/`

## 7. 验收

```text
① 决策：A 还是 B，判据是什么（引 80/81 的证据）
② 三个具体问题的答案（含 V 会不会变 —— 用断言回答）
③ 若 B：5 条读数断言的"先红后绿"证据 + 新基线数
   若 A：代价量化 + "不许调 drive_frame"的断言
④ cargo/三轴/headless/selftest 是否退
```

## 8. 汇报（≤8 行）

分支+sha、决策与判据、`V` 的结论、5 个读数的新旧值、断言是否先红后绿、三轴是否退。
