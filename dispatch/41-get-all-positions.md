# 41 · `agent/get-all-positions`：把 `get_all_positions` 接进 `EntityDrawer`

**优先级 P1。第八轮报告 §8 第 3 条。**

## 0. 现状

```
原版 EntityDrawer::draw（0x4b2890）读 `get_all_positions`（位置表）来画实体
我们：EntityDrawer 的 _draw 已经会画准备圈（第八轮 33 号），但**不读位置表**
      现在的位置入口是 _sync_entities 代 pos_info_event 填的
```

而 `PlanePosManager` 在第八轮 36 号解出来了 —— **24 B / 3 字段，它就是那张位置表**。

## 1. 要做什么

1. 把 `GraphwarPlanePosManager` 的位置表接进 `EntityDrawer` 的 `_draw`：
   - 读表 → 逐实体画（士兵/名字/准备圈）；
   - **顺序不可依赖**：`get_all_positions` 是**散列序**（第八轮 §7 明确标未知）⇒
     画之前要**排序**（按 id），否则帧间顺序会抖；
2. 保持 `_sync_entities` 那条代偿路径**可回退**（开关或注释说明），别一把删掉；
3. headless 断言：
   ```
   位置表有 N 个实体 → 画了 N 个
   两个不同顺序插入 → 画出来的结果一致（证明排序有效）
   ```

## 2. 硬约束

- **不许改 `src/entity_drawer.rs` 的算术**（那是已复原部分）；
- 数字不许退：`cargo test --offline` **1628**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；
  headless 基线 `entity_drawer 118 / assemble 213 / plane_on_screen 96 / obstacle 116 / function_drawer 132`；
  `selftest` **712 / 0 failed**（有 serve_match）。
- ⚠️ gdext 构建三步（build `--features gdext-bindings` → `cp` 进 `client/` → `--import`）。

## 3. 环境

- 克隆到 `<WS>/.agent-work/getpos/repo`，分支 `agent/get-all-positions`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/getpos/target`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/get-all-positions`

## 4. 可以改的文件

- `src/_scaffold/entity_drawer_class.rs`、`src/graphwar_plane_pos_manager.rs`（读表接口）
- `client/tools/entity_drawer_check.gd`（加断言）
- `docs/16-client.md`（追加一节）

❌ 不许改 `src/entity_drawer.rs` 的算术、`src/game.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 位置表怎么接的（几行、读什么）
② 排序前后"两顺序插入 → 结果一致"的断言（条数）
③ headless 条数 before→after；其余脚本不许退
④ 代偿路径是否保留（是/否 + 理由）
⑤ cargo test 1628；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、接入方式、断言条数、headless before→after、
代偿路径去向、`cargo`/`--check` 结果。