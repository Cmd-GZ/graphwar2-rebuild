# 36 · `agent/plane-pos-manager`：解出 `PlanePosManager`(24 B) —— 最后一个 `stub` 槽

**优先级 P1。第七轮报告 §6 第 4 条；它是 22 个 manager 里**唯一连 `src/` 文件都没有的那个**。

## 0. 现状

```
容器里 22 个 manager：10 wired / 2 layout / 9 assembled / **1 stub**
那个 stub = GraphwarPlanePosManager（24 B，构造点 0x3b89a0 已知）
```

`src/` 下**没有** `graphwar_plane_pos_manager.rs` ⇒ 它连"我方形状"都还没有。

## 1. 已知（别重推）

- 24 字节的构造点在 **`0x3b89a0`**（arm64 侧先确认，`docs/02` 的槽表里记着）；
- 它的**用途**其实从别处已知：`PlanePos` 的坐标换算（世界坐标 ↔ 像素）——`GraphwarPlane` 的
  `update_transform` 三实参就是它在管（第七轮 32 号解过 `0x4b7a70` 那条链）；
- 容器里别的 manager 有 32/48/72/80 字节的，24 是**最小的一个**（3 个 f64？或 1 个 Vec？）。

## 2. 要做什么

1. **字段布局**：24 B ⇒ 逐字段解（宽度 + 偏移 + 类型），要求：
   - **x86 与 arm64 两侧偏移序列一致**（不一致说明配错函数）；
   - `size_of` 断言 == 24；
2. **方法清单**：从这个类型的符号（Windows 2.0.60 有 939 个真名，查 `PlanePos` 相关）
   找出它的方法，逐个归到 `docs/02` 的函数表；
3. **建 `src/graphwar_plane_pos_manager.rs`**（**原版模块名**，见 `AGENTS.md` 硬规矩 4），
   把布局与已复原的换算逻辑放进去；
4. **接进容器**：档位从 `stub` 提到 `layout`（布局解出）或 `wired`（+ 事件 + 交互断言）；
5. 若发现 `GraphwarPlane::update_transform` 的三实参**就是这个 manager 提供的**，把它接上
   （这会让 32 号那三个"我方选择"的实参变成原版值）。

## 3. 环境

- 克隆到 `<WS>/.agent-work/ppman/repo`，分支 `agent/plane-pos-manager`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/ppman/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`（Ghidra = +`0x100000`）
- arm64 `<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- **Windows 2.0.60（939 个真名，查类型/方法最快）** `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`
  注意 COFF `Value` 是**节内偏移**，要加 `.text` 的 `VirtualAddress`
- 工具 `tools/addr_probe.py`、`tools/loc_xrefs.py`、`tools/rust_sym.py`、`tools/three_way_match.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/plane-pos-manager`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- 新增 `src/graphwar_plane_pos_manager.rs`（+ `src/lib.rs` 一行 mod）
- `src/_scaffold/root_wiring.rs`（接手容器 + 档位）
- `src/_scaffold/graphwar_plane_class.rs`（**只在**"三实参来自它"被证实时改）
- `client/tools/assemble_check.gd`（加断言）、`docs/02` / `docs/16` 追加

❌ 不许改 `src/game.rs`、别的 manager 的行为、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 24 B 的字段表（偏移/宽度/类型/证据地址/档位）
② `size_of` 断言 == 24；x86 与 arm64 偏移一致
③ 方法清单（几个、各归到哪）
④ 档位：stub → layout/wired；assemble_check 条数 before 213 → after ?
⑤ 三实参是否来自它（若是，32 号的自选值改成原版）
⑥ cargo test --offline 全绿（基线 1620）；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、24 B 字段表（几字段）、方法几个、档位变化、三实参结论、
assemble_check 条数、`cargo`/`--check` 结果。