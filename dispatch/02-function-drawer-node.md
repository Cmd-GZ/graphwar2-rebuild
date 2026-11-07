# 02 · `agent/function-drawer-node`：`FunctionDrawer` 节点化

**优先级 P0。** 补上"12 个类之外、但棋盘必须有"的那一块。

## 背景

`FunctionDrawer` **不是** 12 个 GDExtension 类之一（它是 5 个可归属函数的小模块），
但 `GraphwarPlane` 要持 **3 个**它当子节点——现在那 3 个是**占位**（`@Control@N`）。

⇒ 棋盘上**现在画不出函数曲线**。这个任务就是把它补上。

## 已知事实（`docs/16-client.md` §36，指令级）

```text
持 HashMap<i32(player id), (Gd<Line2D>, 已写入点数)>
GraphwarPlane::init 建 3 个它并 add_child
update_function 只在 new_len > 已写入数 时，对 points[written..new_len] 逐点 Line2D::add_point
    每点 16 B（两个 f64 x/y），第三个参数 esi = 0xffffffff
变短时一点不加
全二进制唯一 add_point 调用点 0x498fab
```

**关键：增量追加，不是整条重画。** 验收要能区分这两者。

## 要做什么

1. 新增 `FunctionDrawer` 节点类（`#[derive(GodotClass)] #[class(base=Control)]`，gdext 路径）；
2. 每个 player id 一个 `Line2D` 子节点；`update_function(player_id, points)` 按上面的增量语义追加；
3. 把 `GraphwarPlane` 的 3 个占位子节点换成真的 `FunctionDrawer`（**只改 Plane 的节点类，不动纯逻辑**）；
4. 颜色/宽度取自现有常量（`src/function_drawer.rs` 里已有）。

## 允许改的文件（**只许这些**）

- 新增 `src/_scaffold/function_drawer_class.rs`
- `src/_scaffold/mod.rs`（+1 行，feature 门控）
- `src/_scaffold/graphwar_plane_class.rs`（把占位换成真子节点；**只改节点层**）
- 新增 `client/tools/function_drawer_check.gd`
- **`docs/16-client.md`** 追加一节

⚠️ **不许改**：`src/function_drawer.rs`（纯逻辑，一行都不许动）、`src/lib.rs`、
其它 `*_class.rs`、现有 `client/*.gd`。

## 验收（硬要求）

```text
ClassDB.class_exists("FunctionDrawer")
update_function 喂 10 点 → Line2D.get_point_count() == 10
再喂 12 点 → 点数 == 12（只加了 2 点，不是重画 12 点）
喂 5 点（变短）→ 点数仍 == 12，一个点都没加        ← 这条是区分"增量"与"重画"的关键
GraphwarPlane 的 3 个子节点类型 == FunctionDrawer（不再是 @Control@N）
```

如果能量到引擎侧调用次数，再加一条：**`Line2D.add_point` 的调用次数 == 增量点数**。

回归同 `README.md`（cargo 两条 + selftest + coverage --check）。

## 汇报（≤8 行）

分支 + sha、`add_point` 的增量语义怎么验的（贴断言）、
Plane 的子节点类型变化、`cargo` 条数、哪些没做到。
