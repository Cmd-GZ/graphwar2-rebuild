# 46 · `agent/entity-drawer-gating`：`EntityDrawer` 的门控从**近似**换成**真查询**

**优先级 P1。第九轮报告 §8 第 4 条。**

## 0. 现状

```
原版 EntityDrawer::draw(0x4b2890) 画谁不画谁，取决于两个门控：
  · get_alive      —— 这个实体活着吗
  · is_on_turn     —— 轮到它了吗
我们现在：用 `prep`（准备表）里的成员**近似** —— 报告明确标着 **推断**
```

⇒ 接上真查询之后，准备圈/士兵的显示条件才从"推断"变成"观测"。

## 1. 要做什么

1. **找这两个门控的原版实现**（`get_alive` / `is_on_turn`）
   —— 大概率在 `LifeManager` / `TurnManager` 里（第八轮 36 号已把位置表接上，可参考）；
   - **Windows 2.0.60 有 939 个真名**，查 `is_alive` / `on_turn` 相关最快
     （COFF `Value` 是节内偏移，要加 `.text` 的 VirtualAddress）；
2. **确认门控语义**：
   ```
   get_alive   —— 是"活着"还是"未被排队死亡"？（`LifeQueuedDeath` 那条事件的语义）
   is_on_turn  —— 当前回合方，还是"本回合已开火"？
   ```
   每条给地址 + 证据档；
3. **替换近似**：`EntityDrawer` 里改用真 manager 查询；
4. headless 断言（**至少三条**）：
   ```
   活着的实体 → 画；死掉的 → 不画
   非当前回合方 → 画法不同（或按原版规则）
   门控输入变化后 → 下一帧绘制跟着变
   ```

## 2. 硬约束

- 不许把语义写成猜测：查不到就保留近似并标"未知 + 缺什么"；
- 数字不许退：`cargo test --offline` **1631**、三轴 `fire 647/1075`、`batch 647/1075`、
  headless `entity_drawer 236 / assemble 217 / plane_on_screen 96`；
- ⚠️ gdext 构建三步（build → `cp` 进 `client/` → `--import`）。

## 3. 环境

- 克隆到 `<WS>/.agent-work/edgate/repo`，分支 `agent/entity-drawer-gating`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/edgate/target`
- 工具 `tools/addr_probe.py`、`tools/loc_xrefs.py`、`tools/rust_sym.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/entity-drawer-gating`

## 4. 可以改的文件

- `src/_scaffold/entity_drawer_class.rs`
- `src/graphwar_life_manager.rs` / `src/graphwar_turn_manager.rs`（**只加查询接口**）
- `client/tools/entity_drawer_check.gd`（加断言）
- `docs/16-client.md` 追加一节

❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 两个门控的地址与语义（各一句 + 证据档）
② 近似是否被替换（逐条）
③ 新增 headless 断言条数（≥3）
④ entity_drawer_check 条数 before 236 → after ?
⑤ cargo test 1631；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、两个门控的地址与语义、近似替换了几处、新增断言数、
entity_drawer_check 条数、`cargo`/`--check` 结果。