# 01 · `CG/assemble-min`：让 `GraphwarRootNode` 真正装配

**优先级 P0。** 这是唯一能把"12 个能跑的节点"变成"一棵能跑的树"的任务。

## 背景

12 个 GDExtension 类**全部**已经能被引擎实例化并驱动（603 条 headless 断言全过），
但它们是**孤立的节点**：`GraphwarRootNode` 里那 25 个模块槽都是**占位**，22 个 manager 一个都没真装配。

所以现在能"画出棋盘"，但画出来的东西之间没有关系——
没有对局状态机、没有回合、没有玩家表。

## 已知证据（直接用，别重推）

`docs/02-module-map.md` §22.4，指令级：

```text
GraphwarGameClient::new    → 22 条 add_module
                             x86 0x517690 / arm64 0x409c70，调用序逐位相同
game_server_thread         → 20 条
GraphwarBackend::start_*   → 1 条 ×2
```

容器真身（同一节）：

```rust
GraphwarModuleContainer { modules: HashMap<TypeId, Rc<dyn Any>> }   // 1 字段 / 48 字节
```

⇒ **不是按偏移持有，是按 `TypeId` 查表。** `get_module::<T>()` 才是取法，别去找 `+0xNN`。

各 manager 的依赖关系（谁在谁的 `new` 里被建）见 `docs/02` §22.3 的 25 型台账。

## 要做什么（**只做 3–5 个，不要 22 个**）

按这个顺序，**每一步都能单独验收**：

1. `GraphwarModuleContainer` 的 Rust 侧装载器（`TypeId` → `Rc<dyn Any>`，行为照 `src/graphwar_module_container.rs`）；
2. 装配 **3 个**：`GameStateManager` → `TurnManager` → `PlayerManager`
   （这三个是"能开一局"的最小集合，且它们的 `new` 我们已有实现）；
3. `GraphwarRootNode` 建出 **Setup 屏 → Game 屏** 两张，切换时 `set_visible`；
4. `Game` 屏里挂 `GraphwarPlane`（已有节点类）+ `EntityDrawer`（已有节点类），
   用 3 个假实体把士兵画出来。

**不要**碰剩下的 19 个 manager——那是下一轮的事。

## 允许改的文件（**只许这些**）

- 新增 `src/_scaffold/root_wiring.rs`（装配器本体）
- `src/_scaffold/graphwar_root_node_class.rs`（把占位槽换成真装配）
- `src/_scaffold/mod.rs`（+1 行，feature 门控）
- 新增 `client/tools/assemble_check.gd`（验收脚本）
- **`docs/16-client.md`** 追加一节（先 `grep '^## '` 看最大编号）

⚠️ **不许改**：`src/lib.rs`、`src/graphwar_*.rs` 的纯逻辑文件、其它 `*_class.rs`、
现有 `client/*.gd`、`docs/INDEX.md`、`docs/HANDOVER.md`。

## 验收（硬要求，写进 headless 脚本）

```text
ClassDB.class_exists("GraphwarRootNode")
装配后：容器里能查到 3 个 manager（按类型断言，不是按数量）
切屏：Setup 屏可见 / Game 屏可见 的状态正确
Game 屏：GraphwarPlane 有 1 个、EntityDrawer 里有 3 个实体 × 4 子节点
每帧：Plane 的 _draw 被调用（计数 ≥ 1）
```

回归：

```text
cargo test --offline                      # lib 457 / 总计 1499，全绿
cargo test --offline --features gdext-bindings
bash client/run.sh selftest                # 694（先 cargo build --example serve_match）
python3 tools/coverage_matrix.py --check   # OK
```

## 汇报（≤8 行）

分支 + sha、装配了哪几个 manager、**容器按类型能查到几项**、
headless 脚本的断言数与结果、`cargo` 两条的条数、哪些仍是占位。
