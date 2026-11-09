# 84 · `agent/retire-replay-player`：**删掉 `replay_player.gd`**，selftest 那 6 条改打 `GwReplayDriver`

**优先级 P1 —— 内核里现在有两份局面机，是真实的维护负担。**

## 0. 现状

```
81 号已经把回放**驱动**搬进 Rust：`GwReplayDriver`（`src/_scaffold/replay_driver.rs`，1357 行），
   `play_screen.gd` 已改用它（只删流程：106 增 / 13 删分支）
但 `client/src/play/replay_player.gd`（**426 行**旧 GDScript 局面机）**还在**，
  `selftest` 里那 6 条仍用它做 canonical 对拍
⇒ 两份局面机并存：任何一处改动都要同步两边
```

## 1. 要做什么

1. 把 `selftest` 里那 **6 条**改成直接打 `GwReplayDriver`（保持同样的**三端同格式**判据：
   Rust 驱动 / 旧 GDScript / replay_attrib 三者对同一份样本的结果一致）；
2. **保留对拍价值**：改完必须仍能证明"新驱动与旧实现同解"——
   建议把"旧实现的期望值"**固化成夹具**（几份样本的事件序列摘要），这样删掉旧代码也不丢对照；
3. **删** `client/src/play/replay_player.gd`，并 `grep` 全仓确认无残留引用
   （`GwReplayPlayer` 的类名/静态枚举映射仍有引用的话，写一份"哪些是纯枚举映射、可以留在视图层"的说明）；
4. 加一条 headless 断言：**旧类名已不存在**（防止有人又把它加回来）。

## 2. 硬约束

- `cargo test --offline` **1753**；三轴 `fire 647 / batch 647 / batch_conv 637` 不许退；
- headless 合计 **1890 条**（14 个 SceneTree 脚本）**只许多不许少**；
- `selftest` 714 / 0 / 1 skip（两种已知降级见 `docs/18` §10.3.3）；
- headless 只跑 `extends SceneTree` 的脚本；
- 提交信息用 `-F 文件`。

## 3. 环境

- 分支 `agent/retire-replay-player`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 相关：`client/src/play/replay_player.gd`、`client/tools/selftest.gd`、
  `client/tools/play_flow_check.gd`、`src/_scaffold/replay_driver*.rs`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/retire-replay-player`

## 4. 可以改的文件

- `client/tools/selftest.gd`、`client/tools/play_flow_check.gd`、`client/src/play/**`（删除）
- `src/_scaffold/replay_driver*.rs`（只加断言/接口，不改语义）
- `fixtures/`（固化的期望值）、`docs/16-client.md`
- ❌ 不许改 `src/game.rs`、`client/src/screens/**`、`dispatch/`

## 5. 验收

```text
① 6 条改成什么样（判据还是"三端同格式"吗）
② 期望值固化在哪（夹具），删掉旧实现后怎么证明同解
③ 删了多少行；全仓残留引用 grep 结果
④ 新增的"旧类名不存在"断言
⑤ cargo 1753 / 三轴 / headless 1890+ / selftest 714
```

## 6. 汇报（≤8 行）

分支+sha、6 条改法、夹具位置、删了 426 行还是多少、残留引用、headless 与三轴是否退。
