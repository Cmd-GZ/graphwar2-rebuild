# 81 · `agent/client-flow-3`：客户端流程继续搬（回放驱动 / 联机 / 关卡坐标）

**优先级 P1（Ａ 档最大的一块；按 67 号的路子继续）。**

## 0. 现状

```
已搬进 Rust：胜负判定 / 射手轮转（54 号）｜弹道生命周期 / 回合超时 / AI 闸门（67 号）
           `play_flow_check` 61 → **135 条**
仍剩：`client/src/screens/*.gd` **≈6,062 行**仿制流程
     其中**联机、回放驱动、关卡坐标**三块**基本没有证据**（`docs/00` §0.1b.2 归为 Ｄ+Ｅ）
```

## 1. 本轮做**回放驱动**（挑它是因为它最可能有真值）

```
回放驱动 = 把 replay 的事件流按 tick 喂给内核/界面（我们现在的 `serve_match`/`selftest` 是另一条路）
真值来源：401 份冻结语料 + 已有的 `replay_attrib` / `replay_state` / `replay_diff`
```

1. 先把这个"回放驱动的状态机"在 Rust 里成形（`ReplayDriver`：加载 → 按 tick 推进 → 派发事件 →
   到结算批触发结算 → 下一批），**接口要能被 headless 直接调用**；
2. 用它替换 `client/src/screens/*.gd` 里对应的驱动代码（只删流程、留视图）；
3. headless 断言（≥20 条）：
   ```
   · 给定一份 replay，驱动到第 N 批时，消息序列与 `replay_attrib` 一致
   · tick 推进单调、不跳批；结算批边界与 `split_batches` 一致
   · 一批结束到下一批开始的间隔（回合末延迟段）与语料一致
   ```

**联机与关卡坐标这一轮不做**（缺证据，归 Ｄ/Ｅ 档，别硬上）。

## 2. 边界（沿用 54/67 号，别走回头路）

```
进 Rust：状态机、批推进、事件派发、结算触发
留 GDScript：节点、锚点/边距/字号、纹理、动画插值
判据：GDScript 里**不许再出现"根据游戏状态决定做什么"的分支**
```

## 3. 硬约束

- 三轴不许退（647/647/637）；`cargo test --offline` 1710；headless 1827 / 0 failed；
  `selftest` 714（有 flake，见 76 号）；
- **headless 只跑 `extends SceneTree` 的脚本**；
- 不许把 Rust 逻辑抄回 GDScript；
- 提交信息用 `-F 文件`。

## 4. 环境

- 分支 `agent/client-flow-3`；**私有** `CARGO_HOME` / `CARGO_TARGET_DIR`
- 冻结语料 `.agent-work/replay-snapshot/`（401 份，**只读**；新样本放别处）
- 参考：`src/_scaffold/replaydiff.rs`（批切分与延迟段口径）、`docs/16-client.md`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/client-flow-3`

## 5. 可以改的文件

- `src/_scaffold/`（新增 `ReplayDriver` 或等价物）、`client/tools/play_flow_check.gd`
- `client/src/screens/*.gd`（只删流程）、`docs/16-client.md`
- ❌ 不许改 `src/game.rs` 结算、`fixtures/`、`dispatch/`

## 6. 验收

```text
① 驱动状态机落在哪（文件/接口），哪些 GDScript 流程被删（多少行）
② 新增断言条数（≥20），以及"与 replay_attrib 一致"是怎么判的
③ play_screen 等文件行数 before → after
④ 三轴/cargo/headless/selftest 是否退
```

## 7. 汇报（≤8 行）

分支+sha、驱动接口、删了多少行 GDScript、新增断言数、一致性判据、三轴是否退。
