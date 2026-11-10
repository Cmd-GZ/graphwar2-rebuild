# 95 · `agent/gsm-handlers`：把 7 个 handler 从 `src/game.rs` 搬进 `graphwar_game_state_manager.rs`（**最后一格升档**）

**优先级 P1。它卡住的不是证据，是我们的**文件边界**——而这正是本项目"源码还原"的判据之一。**

## 0. 现状

```
档位 **19 wired / 2 layout / 1 assembled / 0 stub**
最后一格 = `GraphwarGameStateManager`（296 B，`game::Match`）
86 号复核结论：**不升**，因为它的实现位置在 `src/game.rs`（结算核心）+ `client/tools/selftest.gd`
   ⇒ 要升，得先把 **7 个 handler** 搬出来
★ 已确认：`src/game.rs` 里现有 **7 处** `fn ...handler` / `handle_tick` 形态的实现
```

**这一格的特别之处**：它不是"缺证据"，是**"该在 `graphwar_game_state_manager.rs` 的代码写在了 `game.rs` 里"**。
按 `AGENTS.md` §4（文件名就是证据：复原文件用原版名），这属于**必须修的结构问题**。

## 1. 本次**放开禁区**（仅限本任务，且仅限"搬移"）

```
允许改 `src/game.rs`，但**只许搬移**：
  · 把 7 个 handler 的实现**原样**移到 `src/graphwar_game_state_manager.rs`
  · `game.rs` 里只留 `pub use` 或最小调用转发
  · **不许改任何行为**（三轴与 cargo 一条都不许动）
理由：原版这 7 个 handler 就属于 `game_state_manager`（86 号的订阅表已逐条指到指令）
```

## 2. 要做什么

1. **先列清单**：7 个 handler 各自的原版入口地址（86 号的订阅表）+ 现在的实现位置；
2. **搬移**（行为零变化）：
   ```
   · 每次只搬 1 个，搬完立刻 `cargo test` + 三轴
   · 判据：搬完后 `git diff src/game.rs` 只剩删除 + 转发；`git diff graphwar_game_state_manager.rs` 是新增
   ```
3. **升档**：补 `Wired::status()` 臂 / `*_TIER_COUNT` / `assemble_check.gd` 的 `TIER_TALLY` /
   `docs/161` 清单（四处同步），档位 → **20 wired / 2 layout / 0 assembled / 0 stub**
4. **自检里的那一半**（`client/tools/selftest.gd` 里对 Match 的直接操作）：
   若仍必须直接操作 `Match`，**写清为什么**（这是"禁区里的第二个位置"，要单独说明，不许含糊）

## 3. 硬红线

```
· **行为零变化**：三轴必须仍是 647 / 647 / 637；`cargo test` 1756；headless 1930；selftest 717
  （只要有一条变了，就说明搬的是"逻辑"不是"位置"，回退）
· 每一搬单独一次提交（便于回退与审查）
· 不许顺手"优化"或"重命名"；搬完的代码要能一对一映射回原地址
```

## 4. 环境

- 分支 `agent/gsm-handlers`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 参考：`docs/161` 那格、`docs/06` §46（GSM 订阅表）、`src/_scaffold/root_wiring.rs:378`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/gsm-handlers`

## 5. 验收

```text
① 7 个 handler 的清单（原版地址 + 现在的实现位置）
② 每次搬移的 diff 摘要（删/增行数）与当时的 cargo/三轴读数
③ 档位四处同步后的值（19/2/1/0 → ?）
④ selftest 那一半为什么还在（或已搬走）
⑤ 三轴是否**一位未变**（这是本任务的核心判据）
```

## 6. 汇报（≤8 行）

分支+sha、7 个 handler 清单、搬完的 diff 摘要、档位新值、三轴是否一位未变。
