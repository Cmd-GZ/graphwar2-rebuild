# 21 · `agent/diffeq-fire-path`：给内核加 `Mode` 入参（DiffEq 开火路径）

**优先级 P0。这是目前最大的一块"不可比"**：515 批 / 550 发因为内核写死 `Mode::Normal` 而无法对拍。

## 问题

```text
src/game.rs::fire_functions 里写死了 Mode::Normal
⇒ 微分方程局（DiffEq）的开火走的是 Normal 的采样器
⇒ 515 批 / 550 发**不可比**（不是"不一致"，是根本没比对）
```

我们早就实现了 `Mode::Ode`（`src/graphwar_path_calculator.rs`，RK4 积分），
但**对局路径上没有用它**。

## 要做什么

1. **让 `Match` 知道这一局是哪种模式**：模式来自 `FunctionModeInfo`（tag 46）事件
   （`docs/07` §3 的 74 事件表；取值 `NormalFunction` / `DiffEqFunction`，`docs/07` §5 已穷举）；
2. **开火时按模式选采样器**：`Mode::Normal` → `y = f(x)`；`Mode::Ode` → RK4 积分；
3. **补上 DiffEq 的对拍口径**：`replay_attrib` / `replay_diff` 里把 515 批纳入比较；
4. 量化：这 550 发里能对上多少（**从"不可比"变成"可比"本身就是成果**）。

## 已知（别重推）

- `Mode::Ode` 已实现且有过逐位对拍（搜 `Mode::Ode` / `Ode` 在 `docs/` 的记录）；
- 事件 `FunctionModeInfo`(46) 载荷是 `function_mode: str`；
- 三轴护栏当前值见 `dispatch/README.md`（**改了不许退**）。

## 环境

- 克隆到 `<WS>/.agent-work/diffeq/repo`，分支 `agent/diffeq-fire-path`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/diffeq/target`
- 快照 `<WS>/.agent-work/replay-snapshot/`（401 份，**定格**）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/diffeq-fire-path`

## 可以改的文件

- `src/game.rs`（开火路径 + 模式字段）
- `src/_scaffold/replaydiff.rs`、`src/_scaffold/diffconv.rs`（把 DiffEq 批纳入比较）
- `examples/`（只加不改）、`tests/`
- `docs/06-verification.md` / `docs/14-turn.md` 追加一节

❌ 不许改 `src/graphwar_path_calculator.rs` 的采样算法（`Mode::Ode` 已有实现）、`*_class.rs`、`client/`

## 验收

```text
DiffEq 批（515 批 / 550 发）：从"不可比"变成"可比"，并给出命中数
Normal 轴三轴护栏一个都不许退
cargo test --offline 全绿；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、模式怎么传进 `Match`、DiffEq 550 发的命中数（或为什么仍不可比）、
三轴 before/after、`cargo`/`--check` 结果。
