# 37 · `agent/texture-loader`：士兵/靶子不该是纯色方块 —— `GraphwarTextureLoader` 接上

**优先级 P1。第七轮报告 §6 第 3 条。** 目前画面上**士兵和靶子是纯色方块**，
因为纹理加载那条链没接。

## 0. 现状

```
src/graphwar_texture_loader.rs     纯逻辑已复原（模块在 78 个之内，527 里已归属）
client/ 里                          仿制品用纯色方块代替
```

⇒ 缺的是"把原版的贴图取名 + 加载 + 缓存"这一层接到节点上。

## 1. 要做什么

1. **读清 `graphwar_texture_loader` 的原版语义**（`docs/02` 的归属 + `src/` 里既有注释）：
   它按什么名字取图？（`Skins`/`Faces`/`Hats` 三类？玩家 id？队色？）
2. **确认贴图从哪来**：`client/assets/`（用户自己的 pck 解出）里有哪些是它要的；
   缺的列清单（**不要**去改 pck、不要提交资源）。
3. **接到节点层**：让 `EntityDrawer`（或对应的绘制类）真的用上 `ImageTexture`，而不是纯色方块；
   若原版就是"纯色 + 名字"，那就**给出证据**并把纯色保留（别为了好看而偏离）。
4. **headless 断言**：能取到纹理时 `Sprite2D.texture != null`；取不到时**优雅退化为纯色**
   （这条也要断言，因为很多机器没有 assets）。

## 2. 硬约束

- **不许提交任何图片资源**（`rsc/`、`client/assets/`、pck 都是 gitignore）；
- 缺资产的机器上必须**不崩**：`client/assets` 不齐时退化成纯色，并把"缺什么"打进日志；
- 数字不许退：`cargo test --offline` **1620**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless `entity_drawer 118` / `assemble 213` / `plane_on_screen 96`。

## 3. 环境

- 克隆到 `<WS>/.agent-work/tex/repo`，分支 `agent/texture-loader`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/tex/target`
- Godot `<WS>/tools/Godot_v4.7.2-stable_linux.x86_64`；素材在 `<WS>/gw2-rebuild/client/assets/`（gitignore）
- ⚠️ **gdext 构建三步**：`cargo build --release --features gdext-bindings` → `cp` 进 `client/` → `--import`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/texture-loader`
- 逐文件 `git add`，**禁止 `git add -A`**

## 4. 可以改的文件

- `src/_scaffold/entity_drawer_class.rs`（贴图那段）
- `src/graphwar_texture_loader.rs`（**只在**证据指向它时）
- `client/tools/entity_drawer_check.gd`（加断言）、`docs/16-client.md` 追加

❌ 不许改 `src/game.rs`、`client/src/**` 的其它逻辑、`dispatch/`；**不许提交二进制/图片**

## 5. 验收

```text
① 原版按什么取名（证据：地址或函数），三类贴图各自怎么来
② 接上之后：有 assets 时 texture 非空（断言条数）；无 assets 时退化为纯色（断言）
③ 缺哪些贴图（清单，不修）
④ headless 与三轴数字一个都不许退
⑤ cargo test --offline 1620；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、取名规则（证据）、接上了什么、缺哪些贴图、
headless 条数 before→after、三轴有没有退、`cargo` 结果。