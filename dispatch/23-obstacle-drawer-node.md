# 23 · `agent/obstacle-drawer-node`：补上 `ObstacleDrawer` 类（客户端替换的关键缺口）

**优先级 P1。** 第三轮 18 号已经把真 `GraphwarPlane` 接进场景（5314 点喂进 `Line2D`），
但客户端替换清单里明确的缺口是：**`ObstacleDrawer` 类还不存在**。

## 现状

```text
src/obstacle_drawer.rs          纯逻辑（7 个 Location、2 个可归属函数）
                                 —— 它把障碍光栅化成 Image → ImageTexture → Sprite2D
src/_scaffold/                  **没有** obstacle_drawer_class.rs
GraphwarPlane 的障碍层          目前没有真节点承载
```

## 要做什么

1. **做 `ObstacleDrawer` 节点类**（照 `entity_drawer_class.rs` / `draw_classes_check` 的模板）：
   - `#[derive(GodotClass)] #[class(base=Control)]`（或按原版基类定，见 `docs/16` §39.3 的基类对照表）；
   - 把 `src/obstacle_drawer.rs` 的纯逻辑接上：光栅化 → `ImageTexture` → `Sprite2D` 子节点；
   - 演示"加一个障碍 → 出现一个 Sprite2D"，以及"挖一个洞 → 贴图更新"。
2. **接进 `GraphwarPlane`**：把障碍层从"没有"变成"真节点"（Plane 侧改动要最小）。
3. **验收脚本**（新增 `client/tools/obstacle_drawer_check.gd`）：
   断言子节点数、贴图非空、`Image` 尺寸、加/挖各一次后的变化。

## 已知（别重推）

- `ObstacleDrawer::update` **连矩形参数都没有** —— 脏矩形由 `graphwar_plane` 决定并持有；
  它每帧被 `GraphwarPlane::process` 调 **2 次**（`docs/16` §36）；
- 障碍数据来自 `Mutex<GraphwarObstacleData>`（掩膜）；
- 现成模板：`src/_scaffold/entity_drawer_class.rs`（677 行，真实代码约 155 行）
  与 `src/_scaffold/{effect,long_press,plane,showcase}_class.rs`；
- 12 个 GDExtension 类的**基类对照表**在 `docs/16` §39.3（11 个可定，`GraphwarAdSwitch` 未知）；
  `ObstacleDrawer` **不是**那 12 个之一 ⇒ 基类要自己定（很可能也是 `Control`，但要有证据）。
- 已知坑（`entity-node` 实测）：`Gd<Node>` 不保活；野句柄 `upcast_ref` 会 **1337 并卡死进程**；
  树外 `add_child` 不触发 `_ready`；**不要**写 `set_process(true)`。

## 环境

- 克隆到 `<WS>/.agent-work/obstaclenode/repo`，分支 `agent/obstacle-drawer-node`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/obstaclenode/target`
- Godot：`<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`
- 构建：`cargo build --release --features gdext-bindings`，**拷进 `client/`**，再
  `godot --headless --path client --import`（**三步缺一步就会用旧 .so，今天刚踩过**）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/obstacle-drawer-node`

## 可以改的文件

- 新增 `src/_scaffold/obstacle_drawer_class.rs`、`client/tools/obstacle_drawer_check.gd`
- `src/_scaffold/mod.rs`（+1 行）、`src/_scaffold/graphwar_plane_class.rs`（接障碍层，最小改动）
- `docs/16-client.md` 追加一节

❌ 不许改 `src/obstacle_drawer.rs` 的纯逻辑、`src/lib.rs`、`client/src/**`、既有 `client/tools/*.gd`

## 验收

```text
headless：ClassDB.class_exists / 子节点数 / Image 尺寸 / 加障碍后 Sprite2D 出现 / 挖洞后贴图变化
模板对照：真实代码行数（不含注释/单测）报告出来
cargo test --offline 全绿；--features gdext-bindings 也绿
client/run.sh selftest 694（前置：assets + `cargo build --example serve_match` + 导出 CARGO_TARGET_DIR）
```

## 汇报（≤8 行）

分支+sha、headless 断言数与结果、**基类怎么定的（证据）**、真实代码行数、
Plane 侧改了几行、`cargo`/selftest 结果。
