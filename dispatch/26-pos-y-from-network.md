# 26 · `agent/pos-y-from-network`：证伪或证实「爆炸点 `pos_y` 来自网络」

**优先级 P0。** 第四轮报告 §8 第 1 条：**如果成立，1 ulp 残差的性质会从"bug"变成"不可复现字段"。**

## 为什么可能是这样

第四轮已经排除了三层：

```
① 求值器        —— 逐位一致（eval_differ = 0，1287/1287）
② 采样器写点数组 —— 逐位相同（真机 frida 抓的 3588/3588、ulp1 = 0）
③ 取点/常数顺序  —— IEEE 下四种结合逐位相同，且 Sterbenz 反证
```

⇒ **三层都对，残差却还在**。那么第 4 个可能：**我们算的 `pos_y` 与回放里的 `pos_y` 根本不是同一个来源**——
回放里那个值可能是服务端下发的（网络字段），而不是客户端算的。

## 要做什么

1. **从协议侧查**：`EffectInfo`(tag 44) / `ExplosionEffect` 的字段里，`pos_x/pos_y` 是
   **客户端算的**还是**服务端给的**？依据：
   - `GameHeartbeat`(63) / `EffectInfo` 的 wire 顺序与产生者（`docs/07` §3 的 74 事件表 + 第四轮 17 号任务
     已把 `EffectInfo` 的**生产者全部定位**）；
   - 谁在**发送** `EffectInfo`：客户端发还是服务端发？（`docs/09` §11-12 有 broker/room 的角色划分）
2. **从数值侧查**：那一批 1 ulp 的 `pos_y`，如果按"服务端下发"解释，**两边是否本来就该不同**？
   （即：回放里的值不是"真值"，而是"另一台机器算出来的值"——那么逐位相等本来就不该期待）
3. **做一次判别实验**（关键）：
   - 若 `pos_y` 由服务端产生 ⇒ **同一发的 `pos_x/pos_y` 精度应当同源**（都来自服务端）；
     而我们的 `pos_x` **多数逐位相同**、`pos_y` 差 1 ulp ⇒ 这与"整条来自服务端"矛盾，**要解释**；
   - 若 `pos_y` 由客户端产生 ⇒ 那 1 ulp 只能来自"客户端那台机器的浮点路径"，
     这就要解释为什么同一份代码会差（提示：**x87/SSE 混用、FMA 收缩、编译器重排**）。
4. **结论要能落地**：给出"下一步该改什么"或"为什么不该改"。

## 已知（别重推）

- `EffectInfo` 的两个生产点已定位（`docs/06` 第四轮 §31 / 任务 17）；
- 爆炸点 `pos` 取**点数组末元素**（arm64 `0x4a891c ldp`），另一处走 `0x3b7f58` 返回的 `Option<(f64,f64)>`；
- `docs/141` 记着同时回合制的三段（收集/结算/广播）；
- 三轴护栏见 `dispatch/README.md`（**改了不许退**）。

## 环境

- 克隆到 `<WS>/.agent-work/posynet/repo`，分支 `agent/pos-y-network`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/posynet/target`
- x86 目标 `<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`；arm64 `<WS>/.reference/steam/android-2.0.60/`
- Windows 参考 `<WS>/.reference/steam/windows-2.0.60/graphwarrust.dll`（939 名，同 ISA，查调用关系最快）
- 快照 `<WS>/.agent-work/replay-snapshot/`；工具 `tools/addr_probe.py`、`tools/loc_xrefs.py`、`tools/posy_attrib.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/pos-y-network`

## 可以改的文件

- `tools/`、`examples/`（只加不改）、`tests/`
- `docs/06-verification.md` / `docs/07-protocol.md` / `docs/09-network.md` 追加一节
- 若定位到实现错：`src/game.rs` 那一小段（改动最小）

❌ 不许改 `*_class.rs`、`client/`、`dispatch/`

## 验收

```text
① 结论：pos_y 来自客户端 / 服务端（两条独立证据）
② 若"服务端"：解释为什么 pos_x 多数能逐位相同
③ 若"客户端"：解释同一份代码为什么差 1 ulp（给出可检验的机制候选）
④ 三轴护栏一个都不许退
cargo test --offline 全绿（基线 1569）；coverage --check OK
```

## 汇报（≤8 行）

分支+sha、**结论（客户端/服务端）**、两条独立证据各一句、对 ②/③ 的解释、
护栏数字、`cargo`/`--check` 结果。
