# 28 · `agent/stub-semantics`：把 `stub` 语义**一次性**改对（别再让状态字符串落后于事实）

**优先级 P1。** 第四轮报告 §8 第 3 条。这是**文档/状态一致性**问题，但已经误导过两轮。

## 问题

`root_wiring.rs` 与 `client/tools/assemble_check.gd` 里的 `stub` 标记，代表
"这个 manager 现在装的是**我方形状**，不是原版布局"。但事实已经变了：

```
任务 12（第三轮）：22 条 add_module 全部定名，容器 3 → 22
任务 19（第三轮）：5 个 manager 接容器 + 接事件流 + 最小交互
任务 22（第四轮）：FunctionManager 32B/4 字段、ObstacleManager 80B/5 字段**已解出并挂回 crate 根**
```

⇒ 原来标 `stub` 的那几个，现在**有些已经不是 stub 了**，但标记没跟。这会让人（和 agent）
用错误的现状去判断下一步。

## 要做什么

1. **列出全部 `stub` 标记**（`root_wiring.rs` + `assemble_check.gd` + 文档里的说法），
   逐条对照**事实**（字段布局是否已解、事件流是否已接、是否有真布局）：
   - 确实是 stub 的 ⇒ 保留，并写清"缺什么才能不 stub"；
   - 已经不是 stub 的 ⇒ 改成准确描述（例如 `layout=resolved(32B/4f)`、`events=wired`）。
2. **定一套三等判据**（建议）：
   ```
   stub      我方形状（字段布局未知）
   layout    字段布局已逐字段解出（含证据档）
   wired     已接容器 + 已接事件流 + 有最小交互断言
   ```
   把它写进 `src/_scaffold/root_wiring.rs` 的模块文档注释，**代码与断言共用同一套词**。
3. **让 `assemble_check.gd` 的断言跟着改**（不许只改注释、断言还查旧字符串）。
4. 文档同步：`docs/02` / `docs/16` 里凡是引用 `stub` 的地方，一并按新判据说清。

## 已知

- 22 条 `add_module` 的类型清单在 `docs/02`（第三轮 12 号的产出）；
- 5 个已接的 manager 见 `docs/16 §43`；
- 22 号产出的两个布局在 `src/graphwar_function_manager.rs` / `src/graphwar_obstacle_manager.rs`。

## 环境

- 克隆到 `<WS>/.agent-work/stub/repo`，分支 `agent/stub-semantics`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/stub/target`
- ⚠️ 改完 `client/tools/*.gd` 要跑 headless：**`cargo build --release --features gdext-bindings`
  → `cp` 进 `client/` → `godot --headless --path client --import` → 跑脚本**（三步缺一步就用旧 .so，
  今天已经因此误报过一次）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/stub-semantics`

## 可以改的文件

- `src/_scaffold/root_wiring.rs`（**只改文档注释与状态字符串**，不改装配逻辑）
- `client/tools/assemble_check.gd`
- `docs/02-module-map.md` / `docs/16-client.md` 相关段落

❌ 不许改 `src/lib.rs`、manager 的实现、`client/src/**`

## 验收

```text
① stub 标记总数、改了几条、理由（逐条）
② 新判据（stub/layout/wired）写进了哪个文件
③ assemble_check 断言数（改前 199 → 改后 ?），全过
④ 其它 headless 脚本不许退（obstacle_drawer 116 / plane_on_screen 76 / function_drawer 132）
cargo test --offline 全绿（基线 1569）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、stub 标记数、改了几条、新判据落在哪个文件、
assemble_check 断言数（改前/改后）、其它脚本有没有退、`cargo`/`--check` 结果。