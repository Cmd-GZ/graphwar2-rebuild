# 32 · `agent/play-screen-evidence-gaps`：补播放屏那 3 个**可解**的证据缺口

**优先级 P1，可与** 31 并行**（它改接线，你补证据；两者改的文件基本不重叠）。**

## 0. 背景

`docs/16` §43.6(d) 列出 7 个缺口。其中 **3 个是"证据缺"而不是"做不了"**，
补上之后播放屏才可能"看起来像原版"（现在棋盘内部是工程底色）：

| # | 缺口 | 现在怎么办的 | 要什么 |
|---|---|---|---|
| 1 | **tick 源** | 场景自己推 | `GraphwarTickHandler::get_fractional_tick`（x86 `0x519530`）+ u32 tick 字段 |
| 2 | **`update_transform` 的三个实参** | 我方选择 `position=(500,300)` / `scale=(20,-20)` | 原版实参 |
| 3 | **棋盘底色 / 网格 / 队色表** | 没接 | `get_team_overview`（`0x4be6e0`）背后那**两张表** |

（另外 4 个：点生产者 `0x456810` 的写入者 `0x3e44d4` 归属未证、`get_function_fade_on_tick` 只有 GOT 槽、
`z_index` 初值未知、`FunctionDrawer::draw` x86 未定位 —— **不在本任务范围**，除非你顺手解决。）

## 1. 要做什么

1. **tick 源**：把 `get_fractional_tick`（`0x519530`）的算式**逐条解出来**，落地成内核函数；
   `GraphwarPlane` 用它取 tick，而不是由场景注入。**要给出算式与证据地址**。
2. **`update_transform` 三实参**：`GraphwarPlane::ready`（`0x4b7a70`）里对 `update_transform` 的调用，
   三个实参怎么来的（常量？视口尺寸？哪个 GOT 槽？）——**逐条指令**。
   拿到之后，把 `plane_on_screen_check.gd` 里我们自选的 `position/scale` 换成原版实参（或明确"原版就是动态算的"）。
3. **棋盘底色两张表**：`get_team_overview`（`0x4be6e0`）返回什么、两张表的形状与内容
   （队色表大概率是固定数组）。落地后让棋盘内部**不再是工程底色**。

## 2. 硬约束

- 每条结论**要么指到指令、要么指到运行时观测**，否则标 `推断/未知`；
- **负结论要有正对照**：说"扫不到"之前，先用已知槽（如 `0x9bac98`）证明扫法有效；
- ⚠️ 函数指针有**三种 materialize 形态**：`movabs` 直写 / `i64::MAX+add` / `i64::MAX+inc`；
- ⚠️ **Windows 2.0.60（同 ISA + 939 个真名）** 查调用关系最快：
  `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`（COFF `Value` 是**节内偏移**，要加 `.text` 的 `VirtualAddress`）。

## 3. 环境

- 克隆到 `<WS>/.agent-work/psgaps/repo`，分支 `agent/play-screen-gaps`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/psgaps/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（Ghidra = +`0x100000`）
- 工具 `tools/addr_probe.py`（fde/dis/callers/locs/jt）、`tools/loc_xrefs.py`、`tools/const_xrefs.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/play-screen-gaps`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- `src/graphwar_tick_handler.rs`、`src/_scaffold/graphwar_plane_class.rs`（只在证据支撑下改）
- `client/tools/plane_on_screen_check.gd`（实参改正）
- `docs/16-client.md`（§43.6(d) 缺口表就地更新）、`docs/14-turn.md`（若 tick 算式落那里）

❌ 不许改 `src/game.rs` 的结算、其它 `*_class.rs` 的行为、`client/src/**`、`dispatch/`

## 5. 验收

```text
① tick 算式（给公式 + 地址），落地在哪
② update_transform 三实参（给来源 + 地址）
③ 队色/底色两张表（形状 + 内容 + 地址），棋盘内部是否已不是工程底色
④ headless：plane_on_screen / obstacle_drawer / assemble 条数不许退
⑤ cargo test --offline 全绿（1569）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤10 行）

分支+sha、三个缺口各一句（有没有解出来 + 地址）、哪些只能标推断/未知、
headless 条数有没有退、`cargo`/`--check` 结果。