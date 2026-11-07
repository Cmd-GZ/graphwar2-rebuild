# 31 · `agent/play-screen-real-nodes`：把 6 个**真 GDExtension 节点**挂进播放屏，删掉两个仿制 `.gd`

**优先级 P0。这是"客户端迁回 Rust"的第一刀，也是唯一已经具备全部前置条件的一刀。**

## 0. 为什么现在能做了

`docs/16` §43.6 那张表写于第四轮之前，其中第 6 行仍写着「`ObstacleDrawer` 类**还不存在**」——
**这条已过期**：第四轮任务 23 已做出该类（headless **116 条**断言全过）。
⇒ **6 个节点位置现在全都已有真类**，缺的只剩"接线"。

## 1. 现状（照 §43.6 执行，但按下面修正过的状态）

**（a）要删的 2 个仿制 `.gd`**

| 文件 | 行数 | 替代者 |
|---|---|---|
| `client/src/play/plane_view.gd`（`GwPlaneView`） | 527 | `GraphwarPlane` + `AxisDrawer` + `EntityDrawer` + `ObstacleDrawer` + `FunctionDrawer` |
| `client/src/play/fire_fx.gd`（`GwFireFx`） | 261 | `GraphwarPlane` 的 `_draw` 喂点循环 + `set_fractional_tick()` |

**（b）要挂的 6 个节点**（现在**都已有真类**）

| # | 节点类 | 现状 |
|---|---|---|
| 1 | `GraphwarPlane` | 已节点化（棋盘框 + 脏矩形 + 3 个 `FunctionDrawer`） |
| 2 | `FunctionDrawer` | 已节点化（每 player 一条 `Line2D`，**增量追加**） |
| 3 | `EntityDrawer` | 已节点化，**还没挂进播放屏** |
| 4 | `AxisDrawer` | 已节点化，**还没挂** |
| 5 | `EffectDrawer` | 已节点化，**还没挂** |
| 6 | `ObstacleDrawer` | **第四轮新建**（116 断言），**还没挂** |

**（c）要改的 3 个文件**

- `client/src/screens/play_screen.gd`（2610 行）：**44 处** `_plane.*` 调用
  （`set_path`×5 / `clear_path`×8 / `set_targets`×6 / `set_obstacles`×4 / `set_soldiers`×2 /
  `set_show_names`×3 / `set_axis_mode`×3 / `path_color`×5 / `path_points`×2 / `queue_redraw`×2 /
  `add_child`×3 / `soldiers`×1）+ 3 处节点创建（第 301 / 306–309 / 311–314 行）；
- `client/src/game/effect_layer.gd`：换成 `EffectDrawer` 后，靠 `plane.plane` 做的坐标映射要去掉；
- `client/tools/selftest.gd`：**18 处**读 `plane.path_points` / `plane.path_reveal` 的断言要改成读
  `GraphwarPlane` 的方法。

## 2. 要做什么

1. **先做"能跑"**：在 `play_screen` 里实例化真节点（`GraphwarPlane` 为主体，其余作为其子节点或兄弟），
   让**棋盘框 + 曲线 + 士兵 + 网格 + 障碍 + 特效**都来自 Rust 类；
2. **再删仿制**：`plane_view.gd` / `fire_fx.gd` 删除，44 处调用改成调真类；
3. **最后改断言**：`selftest.gd` 那 18 条按新 API 改（**不许直接删断言**，要等价替换）；
4. 每条改动都要能在 headless 里验证。

## 3. 验收（**必须给数字**）

```text
① 6 个真节点各自的 headless 断言条数（挂进去之后仍全过）
② 32 个 GDExtension 类仍能加载（gdext_check 24 条不许退）
③ assemble_check 199 条不许退；plane_on_screen 76 条不许退；obstacle_drawer 116 条不许退
④ selftest：改前 / 改后条数与 failed 数（**双口径**：有 assets / 无 assets 各报一次）
   ⚠️ 真 pck 下的标准值是 **625**（旧文档 694 是旧资产读数）
⑤ 删掉的两个 .gd 确实不在树里了（`git ls-files client/src | grep -E "plane_view|fire_fx"` 为空）
⑥ cargo test --offline 全绿（基线 1569）
```

## 4. 环境

- 克隆到 `<WS>/.agent-work/playscreen/repo`，分支 `agent/play-screen-real-nodes`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/playscreen/target`
- Godot `<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`
- ⚠️ **gdext 构建三步缺一不可**：`cargo build --release --features gdext-bindings`
  → **`cp` 进 `client/`** → `godot --headless --path client --import`（今天已因此误报过一次）
- 真 pck 与游戏三件套：`<WS>/.reference/game/`（`libgraphwarrust.so` sha `9a538115…`）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/play-screen-real-nodes`
- 逐文件 `git add`，**禁止 `git add -A`**

## 5. 可以改的文件

- `client/src/screens/play_screen.gd`、`client/src/game/effect_layer.gd`、`client/tools/selftest.gd`
- 删除 `client/src/play/plane_view.gd`、`client/src/play/fire_fx.gd`
- `src/_scaffold/graphwar_plane_class.rs`（**只在必需时**，且说明理由）
- `docs/16-client.md`（把 §43.6 那张表的过期行改掉 + 追加本轮结果）

❌ 不许改 `src/game.rs`、其它 `*_class.rs` 的行为、`dispatch/`

## 6. 汇报（≤10 行）

分支+sha、6 个节点是否都挂上、删了哪两个 .gd、
四个 headless 脚本的条数、selftest 双口径（有/无 assets）、
还剩哪些 §43.6 的缺口没填、`cargo` 结果。