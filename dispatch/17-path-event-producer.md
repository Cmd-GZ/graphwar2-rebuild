# 17 · `agent/path-event-producer`：追 `calculate_paths` 一侧的事件生产者

**优先级 P1。** 第二轮报告 §8 第 3 条：能**同时**解释两个悬着的问题。

## 要解释的两件事

```text
① "撞什么算撞"：弹道撞障碍的判据 —— 是掩膜点测试？是 calculate_paths 里的某次判定？
② "一次挖洞发 2 条"：为什么有些批里 DestroyObstacle 出现两条
```

第二轮实测（`docs/06` §14 附近的延迟段）：

```
延迟段挖洞点命中已从 1/259 提到 139/258
但仍有"我方挖的点在该回放里不存在"与"一条变两条"的批
```

## 已知（别重推）

- `calculate_paths` = ELF `0x467530`（5408 B），**17 个调用者全是 `setup_level_*` / `generate_level`**
  ⇒ 它对**对局**不参与（上一轮裁定），但**它内部**的通视/栅格逻辑可能与对局的"撞什么算撞"共用同一函数；
- `is_obstacle` / `is_occupied` / `rasterize` 在 `src/plane.rs`（掩膜）；
- `QueueDestroyObstacle` / `DestroyObstacle` 的入队点：`0x472d19`（挖洞入队，`docs/06` §24 记过）。

## 要做什么

1. **谁发 `EffectInfo` / `DestroyObstacle`**：全 `.text` 找出这两类报文的**构造点**
   （`serialize_*` 的调用者链，或 `add_event` 的实参构造处），逐个归到模块；
   ⚠️ 注意 `EffectInfo`(tag 44) 上一轮结论是"**生产者查不动**：全 `.text` 只有 2 处 `movabs` 且都是消费者"
   ⇒ 本任务要**换路子**：从 `add_event`（发件箱）的调用点反查，别从名字字面量找；
2. 回答"一次挖洞发 2 条"的成因（是两条不同实参？还是同一条发两次？）；
3. 回答"撞什么算撞"：`blocked` 的判据在二进制里是哪一处（地址 + 指令）。

## 环境

- 克隆到 `<WS>/.agent-work/path-producer/repo`，分支 `agent/path-producer`
- **私有** `CARGO_TARGET_DIR=<WS>/.agent-work/path-producer/target`
- x86 目标：`<WS>/.reference/steam/2026-08-28/libgraphwarrust.so`
- arm64 未 strip：`<WS>/.reference/steam/android-2.0.60/libgraphwarrust.so`
- 工具：`tools/addr_probe.py`（`fde/dis/callers/locs/jt`，带缓存，不读 pkl）、`tools/loc_xrefs.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/path-producer`

## 可以改的文件

- `tools/`（可扩展 `addr_probe.py`）
- `docs/` 对应主题追加（`docs/06` / `docs/14` / `docs/08`）
- 若定位到内核判据与真值不符：`src/plane.rs` 或 `src/game.rs` 的**那一段**（改前先在文档里写清证据）

❌ 不许碰 `*_class.rs`、`client/`。

## 验收

```text
① EffectInfo / DestroyObstacle 的构造点清单（每个给地址 + 模块）
② "一次挖洞发 2 条"的成因（一句 + 证据）
③ "撞什么算撞"的判据（地址 + 指令）
若改动了内核：延迟段挖洞点 139/258 → 新数；两个既有数字（324 / 1074）不许退
cargo test --offline 全绿（基线 1519）
```

## 汇报（≤8 行）

分支+sha、三类问题的结论各一句（带地址）、有没有改内核、数字变化、`cargo` 条数。
