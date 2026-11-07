# 13 · `agent/curve-on-screen`：让屏幕上**真的画出函数曲线**

**优先级 P1。** 这是"能跑"这条路上**最可见的缺口**：现在棋盘上的弹道曲线**一条都画不出来**。

## 现状（两个断点）

```text
① GraphwarPlane::draw 没有接原版那处绘制（0x456df0）
② FunctionDrawer 已经能增量追加点（02 做完了），但**没有东西喂它点**
```

⇒ 即使把 ① 接上，也只会画出空曲线。

## 要做什么（按顺序，每步可独立验收）

1. **先查 `0x456df0`**：它到底画什么（棋盘框？曲线？），实参从哪来；
   `GraphwarPlane::draw` 现在的实现与它差在哪里（`src/_scaffold/graphwar_plane_class.rs`）；
2. **把点喂进 `FunctionDrawer`**：谁在原版里调用 `update_function`？
   （`docs/16` §36 说它只在 `new_len > 已写入数` 时追加；调用者要查出来）
3. 接上之后，headless 断言：`Line2D.get_point_count() > 0` 且 `== 期望点数`；
4. 如果第 2 步查不出调用者，**如实报告"断点在哪、缺什么证据"**，不要编一个喂点逻辑。

## 允许改的文件

- `src/_scaffold/graphwar_plane_class.rs`、`src/_scaffold/function_drawer_class.rs`
- `client/tools/`（新增/扩展验收脚本）
- `docs/16-client.md` 追加一节

⚠️ **不许改** `src/function_drawer.rs` / `src/graphwar_plane.rs` 的纯逻辑、`src/lib.rs`、`client/src/**`。

## 验收

```text
headless：Plane 里 FunctionDrawer 的 Line2D 点数 > 0（贴实际数字）
若能画出：给出"谁喂点、什么时候喂"的结论（一句话 + 地址）
若画不出：给出断点的确切位置与缺的证据
cargo test --offline 全绿（1512）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、`0x456df0` 是什么、**谁调 `update_function`**（有/没有 + 证据）、
headless 点数、若没接通则断点在哪。
