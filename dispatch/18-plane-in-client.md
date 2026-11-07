# 18 · `agent/plane-in-client`：把真 `GraphwarPlane` 接进客户端播放屏

**优先级 P2。** 第二轮 13 号任务已经把曲线画出来了，但**只在 headless 脚本里**；
客户端里看到的仍是仿制品。

## 现状

```text
src/_scaffold/graphwar_plane_class.rs   真节点（棋盘框 + 脏矩形 + resized）
src/_scaffold/function_drawer_class.rs  真节点（Line2D 增量追加）
client/                                   仿制品（GDScript 自己画）
⇒ 两边各有一套，真节点没接进客户端
```

## 要做什么

1. 在 `client/` 里**加一个场景**（不改现有界面），把 `GraphwarPlane` + `FunctionDrawer`
   挂上去，喂一条真实弹道的点序列，**让曲线出现在屏幕上**；
2. 截图（`--headless` 下用 viewport 抓帧存 png）作为证据，放到 `docs/img/` 或 `rsc/`（**看仓库规矩**，
   二进制衍生内容不许入库 ⇒ 放 `docs/` 之外的 gitignored 目录，或只报数字）；
3. 给出"替换现有仿制品需要什么"的清单（哪些 `.gd` 要删、哪些节点要换）。

## 环境

- 克隆到 `<WS>/.agent-work/plane-client/repo`，分支 `agent/plane-in-client`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/plane-client/target`
- Godot：`<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`
- 构建：`cargo build --release --features gdext-bindings`，产物 `cp` 进 `client/`
- ⚠️ 新增/改动后**必须** `godot --headless --path client --import`，否则扩展静默不加载
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/plane-in-client`

## 可以改的文件

- `client/tools/**`（新增场景与脚本）
- `docs/16-client.md` 追加一节
- `src/_scaffold/graphwar_plane_class.rs` / `function_drawer_class.rs`（**只在必需时**，且说明理由）

❌ 不许改 `client/src/**` 的现有界面、`src/game.rs`。

## 验收

```text
headless 能加载该场景并报出"Line2D 上的点数 == 喂进去的点数"（贴数字）
曲线真的被绘制：给出 `_draw` 调用次数或 viewport 截图的存在性证据
"替换清单"：要动几个 .gd / 几个节点
cargo test --offline 全绿（基线 1519）；selftest 不许退
```

## 汇报（≤8 行）

分支+sha、headless 点数、绘制证据形式、替换清单条数、
有没有动现有 client、`cargo`/selftest 结果。
