# 22 · `agent/manager-layout`：`FunctionManager` / `ObstacleManager` 的原版字段布局

**优先级 P1。** 现在容器里这两个槽装的是**我方形状**（`FunctionTable` / `ObstacleTable`），
报告里明确标着 `stub` —— 也就是**我们没有原版的字段布局**。

## 要解什么

```text
FunctionManager   32 字节
ObstacleManager   80 字节
```

两个都不大，正好适合"逐字段钉死"。

## 方法（这套在这个仓库已反复验证）

1. **arm64 未 strip 侧**：找这两个类型的 `new` / 构造点、`Drop` glue，
   `readelf -sW` 里带类型名的符号能给出**大小**（已经知道 32/80）；
2. **字段偏移**：从各方法的 `ldr/str [x0+0xNN]` 反推，**同一偏移在不同方法里必须一致**才算数；
3. **字段类型**：看存取宽度（`ldr w` vs `ldr x` vs `ldp`）与相邻偏移推导；
   `Vec` 是三连（ptr/len/cap，24 字节）、`HashMap` 的桶在别处；
4. **交叉验证**：x86 侧同址映射后的指令应有**相同的偏移序列**；不一致就说明配错了函数。
5. **自校验**：把解出的布局写成 Rust `struct`，`size_of` 必须等于 32/80（用 `const_assert` 或测试）。

## 已知

- 容器 = `HashMap<TypeId, Rc<dyn Any>>`，**manager 没有 `+0xNN` 偏移**（它们是独立对象，不是嵌在容器里）；
- 22 条 `add_module` 已全部定名（第三轮 12 号）；
- 5 个 manager 已"接容器 + 接事件流"（第三轮 19 号），其中就有这两个；
- 各 manager 的既有偏移记录见 `src/graphwar_*_manager.rs` 的文档注释（**部分是我方推测，要标明哪些被证实/推翻**）。

## 环境

- 克隆到 `<WS>/.agent-work/mgrlayout/repo`，分支 `agent/manager-layout`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/mgrlayout/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`
- arm64 未 strip `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- 工具：`tools/addr_probe.py`、`tools/loc_xrefs.py`、`tools/symtab_dump.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/manager-layout`

## 可以改的文件

- `src/graphwar_function_manager.rs`、`src/graphwar_obstacle_manager.rs`（**文档注释 + 布局结构体**；
  行为逻辑只在与布局冲突时才改，且要说明）
- `src/_scaffold/root_wiring.rs`（把 stub 换成真布局）
- `tests/`、`docs/16-client.md` 追加一节

❌ 不许碰 `*_class.rs`、`client/`、`game.rs` 的结算逻辑

## 验收

```text
两个 struct 的 size_of 分别 == 32 / 80（测试断言）
每个字段：偏移 / 宽度 / 类型 / 证据（符号 or 指令地址）/ 档位（逐位/观测/推断/未知）
x86 侧的偏移序列与 arm64 一致（不一致的要解释）
三轴护栏不许退；cargo test 全绿；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、两个 struct 的字段数、**各字段证据是否有交叉验证**、
哪些字段只能标推断/未知（缺什么）、`size_of` 断言结果、`cargo`/`--check` 结果。
