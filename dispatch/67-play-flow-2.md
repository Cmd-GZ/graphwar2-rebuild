# 67 · `agent/play-flow-2`：把 `play_screen` 剩下那块**流程**搬进 Rust（54 号的续）

**优先级 P1（用户已放开禁区 `client/src/**`）。**

## 0. 54 号已经做了什么

```
删掉两个仿制 .gd（`level_rules.gd` / `turn_driver.gd`，340 行）
胜负判定 + 射手轮转搬进 Rust
新增 play_flow_check 61 条断言
```

## 1. 还剩下的（报告 §7 列的，按这个顺序做）

```
① **弹道生命周期 ~200 行**（准备态 → 发射 → 飞行 → 命中/越界 → 结算）
   —— 这是最值钱的一块，原版在 Rust 里
② 回合超时（`handle_tick` 时钟谓词，`0x470989`；见 docs/14 §11）
③ AI 回合驱动（Bot 何时开火）
④ 关卡坐标/关卡规则（`level_rules.gd` 已删，逻辑要落到 Rust 侧对应模块）
```

联机与回放驱动这一轮**不做**（缺证据）。

## 2. 边界（沿用 54 号，不要走回头路）

```
进 Rust：状态机、阶段流转、超时判定、AI 触发、胜负时机
留 GDScript：节点层级、锚点/边距/字号、纹理、动画插值
判据：GDScript 里**不许再出现"根据游戏状态决定做什么"的分支**
```

## 3. 硬约束

- **不许把 Rust 逻辑抄回 GDScript**；
- 每搬一块就要有 headless 断言（`play_flow_check` 61 → ?）；
- 数字不许退：`cargo test --offline` **1671**、三轴 `fire 647/1075`、`batch 647/1075`、
  headless 全脚本（entity_drawer 366 / assemble 217 / plane_on_screen 96 / process_loop 49 / play_flow 61）
  一律 0 failed；`selftest` 714 / 0 / 1 skip；
- ⚠️ gdext 构建三步（build → `cp` → `--import`）；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/play-flow-2`
- **私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 证据：`src/graphwar_play_screen_control.rs`（现 324 行）、`docs/14`（回合）、`docs/16`（客户端）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/play-flow-2`

## 5. 可以改的文件

- `client/src/screens/play_screen.gd`（只删流程）、`client/src/**`（相关）
- `src/graphwar_play_screen_control.rs`、`src/_scaffold/*_class.rs`、`src/graphwar_turn_manager.rs`
- `client/tools/play_flow_check.gd`、`docs/16-client.md`
- ❌ 不许改 `src/game.rs` 结算、`fixtures/`、`dispatch/`

## 6. 验收

```text
① 搬了哪几块（逐条）、各删了多少行 GDScript
② play_flow_check 61 → ?
③ 三轴与其它 headless 一个都不许退
④ GDScript 里还剩哪些"根据状态决定做什么"的分支（应为 0 或列出理由）
⑤ cargo test 1671；coverage --check OK；check_doc_refs rc=0
```

## 7. 汇报（≤8 行）

分支+sha、搬了几块/删了多少行、play_flow_check 条数、三轴是否退、剩余流程分支、`cargo` 结果。
