# 19 · `agent/manager-behaviors`：22 个 manager 的行为（第一批，只做 5 个）

**优先级 P2。这是"能跑的原版形状"的下一道门槛**，但一次做 22 个不现实 ⇒ **只做 5 个**。

## 现状

```
12 assembly-22：22 条 add_module **全部定名**，容器 3 → 22
但容器里都是**空表** —— 22 个 manager 的**行为**一个都没实现
```

## 本轮只做这 5 个（按"对玩法可见度"选）

| manager | 为什么选它 | 已知入口 |
|---|---|---|
| `graphwar_life_manager` | 生死、目标"打不全就复原"的落点 | `set_alive` / `set_queued_death` |
| `graphwar_target_manager` | 靶子集合（`HashSet<u32>`，无 alive 字段） | `add_target` / `remove` |
| `graphwar_obstacle_manager` | 掩膜、挖洞、延迟销毁（**165 tick**） | `queue_destroy_obstacle` / `is_obstacle` |
| `graphwar_function_manager` | 函数槽、`active_tick` | `set_function` / `set_function_active_tick` |
| `graphwar_tick_handler` | 时钟，回合超时的判据在它下游 | `tick` |

⚠️ 每个 manager 的**函数清单已经解出来了**（`docs/coverage-matrix.tsv` + `docs/02` §17），
**照表实现，不要重新逆向**；需要补的证据逐条标出来。

## 要做什么（每个 manager 三步）

1. 把已有纯逻辑（`src/graphwar_*_manager.rs`）**接进容器**：构造 + 注册 + 让别的 manager 能取到；
2. 接上**事件流**：它订阅哪些事件（`docs/07` §3 的 74 事件表）、往发件箱发什么；
3. headless 断言：装配后能通过容器取到它，且**一次最小交互**（例如 `set_alive` 后 `get` 回来）正确。

**只要"能装配 + 能交互"这两件事，不要求把行为全实现。**

## 环境

- 克隆到 `<WS>/.agent-work/mgr-behav/repo`，分支 `agent/manager-behaviors`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/mgr-behav/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`；arm64 未 strip 见 `.reference/steam/android-2.0.60/`
- 快照：`<WS>/.agent-work/replay-snapshot/`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/manager-behaviors`

## 可以改的文件

- `src/_scaffold/root_wiring.rs`
- 上述 5 个 `src/graphwar_*_manager.rs`（**纯逻辑不许改行为**，只加"接容器/接事件"的那层）
- `client/tools/assemble_check.gd`（扩展断言）
- `docs/16-client.md` 追加一节

❌ 不许改 `src/game.rs`、别的 manager、`client/src/**`。

## 验收

```text
5 个 manager 都能从容器按类型取到（逐型断言）
每个至少一条"交互后取回正确值"的断言
既有数字不许退：内容命中 324、种类序列 1074
cargo test --offline 全绿（基线 1519）
```

## 汇报（≤8 行）

分支+sha、5 个各做到什么程度（装配/交互/事件）、
哪些只能标推断（缺什么证据）、既有数字有没有退、`cargo` 条数。
