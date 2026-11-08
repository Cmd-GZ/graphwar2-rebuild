# 42 · `agent/real-inputs`：把两个"输入端"从**代偿**换成**真链路**

**优先级 P1。第八轮报告 §8 第 4 条。** 现在准备圈与实体外观都能看见，但**输入端是我们自己接的**。

## 0. 现状（报告 §7 原文）

```
准备圈的输入端   我方接线（代 pos_info_event）—— 缺 GraphwarBackend → 节点层那条链路
实体外观的输入端 我方接线（代 51/53/55 事件）—— 同上；且自检那局没有靶子
                 ⇒ 靶子分支只有代码路径
```

⇒ 这两条不接上，`AGENTS.md` §0.5 那条（"每屏每行为要么指到地址、要么指到真报文"）就还不满足。

## 1. 要做什么

1. **`pos_info_event`（26）→ 节点层**：找出原版里这条链路
   （`GameHeartbeat`/事件队列 → 某个 manager → `PlanePosManager` 的位置表 → `EntityDrawer`）；
   给出**逐环节的地址**，然后把我们的代偿换掉；
2. **graphics 51/53/55（`SkinGraphicsInfo` / `FaceGraphicsInfo` / `HatGraphicsInfo`）→ 外观**：
   同样给出链路，并接上；**特别是靶子分支**（现在自检里没有靶子，只有代码路径）；
3. **每条链路都要有"证据档"**：`逐位`/`观测`/`推断`/`未知`；
   接不上的部分**明确标未知**，不要用"应该就是这样"含糊过去；
4. headless 断言：事件进来 → 位置表/外观表变化 → 绘制跟着变。

## 2. 硬约束

- **不许把行为写成猜测**：没有证据的环节宁可保留代偿并写清缺什么；
- 数字不许退（同 41 号任务书的基线）；
- ⚠️ 若需要跑 `serve_match` 造靶子，用 `examples/serve_match`（已入库）。

## 3. 环境

- 克隆到 `<WS>/.agent-work/realin/repo`，分支 `agent/real-inputs`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/realin/target`
- 证据来源：x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`；
  **Windows 2.0.60（939 个真名）** `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll` 查调用关系最快
  （COFF `Value` 是节内偏移，要加 `.text` 的 VirtualAddress）
- 事件表：`docs/07` §3（74 事件 + 字段）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/real-inputs`

## 4. 可以改的文件

- `src/_scaffold/root_wiring.rs`、`src/_scaffold/{entity_drawer,graphwar_plane}_class.rs`
- 相关 manager（`graphwar_plane_pos_manager.rs`、`graphwar_player_manager.rs` 等）
- `client/tools/*.gd`（加断言）、`docs/16-client.md`（追加）

❌ 不许改 `src/game.rs` 的结算、`client/src/**`、`dispatch/`

## 5. 验收

```text
① `pos_info_event` 链路的逐环节地址（或"缺什么"）
② graphics 51/53/55 链路的逐环节地址（或"缺什么"）
③ 代偿是否被替换（逐条）；替换不了的写清原因
④ 靶子分支是否真的被验证过（怎么造的靶子）
⑤ headless 条数 before→after；三轴与 cargo 不许退（1628）
```

## 6. 汇报（≤10 行）

分支+sha、两条链路各一句（地址或缺什么）、代偿替换了几处、
靶子怎么验的、headless before→after、`cargo`/`--check` 结果。