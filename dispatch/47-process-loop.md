# 47 · `agent/process-loop`：把 **60 Hz 定步 `process` 循环**落进内核

**优先级 P0。第十轮报告 §8 第 1 条 —— "每帧驱动 pump"的最后一环。**

## 0. 现状（43 号已给出全部证据，只差落地）

```
GraphwarGameClient::process = x86 **0x518400**
事件泵挂在 **60 Hz 定步循环**里，**先 tick 后泵**（不是"每帧一次"）
43 号给了 3 处改造清单：graphwar_game_client.rs / root_wiring.rs / client
```

⚠️ **上一轮没做的原因**：动 `src/` 要在**同一个提交**里附上两份 coverage 生成物
（`docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md`），否则 `--check` 必红。

## 1. 要做什么

1. 在 `src/graphwar_game_client.rs` 落地定步循环（**60 Hz、先 tick 后泵**）；
2. `src/_scaffold/root_wiring.rs` 接上（谁调 `process`）；
3. **同一个提交**里重跑 `python3 tools/coverage_matrix.py`（两份生成物）；
4. headless 断言（≥3 条）：
   ```
   一次 process(1/60) → tick 计数 +1、泵调用 +1
   累积 1 秒（60 次）→ 计数 == 60（定步，不随帧率变）
   传入 delta=1/30 → 仍然 tick 2 次（追赶），不是 1 次
   ```

## 2. 硬约束

- **先 tick 后泵** 的顺序不许反（43 号实测）；
- 数字不许退：`cargo test --offline` **1638**、三轴 `fire 647/1075`、`batch 647/1075`、
  死亡 `366/385`、挖洞 `207/294`；headless `entity_drawer 292 / assemble 217 / plane_on_screen 96`；
- ⚠️ gdext 构建三步（build → `cp` 进 `client/` → `--import`）；
- 提交信息**用 `-F 文件`**。

## 3. 环境

- 克隆到 `<WS>/.agent-work/procloop/repo`，分支 `agent/process-loop`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/procloop/target`
- 证据：`docs/16-client.md` 里 43 号那节（含 `0x518400` 的调用链）
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/process-loop`

## 4. 可以改的文件

- `src/graphwar_game_client.rs`、`src/_scaffold/root_wiring.rs`
- `docs/coverage-matrix.tsv`、`docs/149-coverage-matrix.md`（**重跑生成物**）
- `client/tools/*.gd`（加断言）、`docs/16-client.md`（追加）
- ❌ 不许改 `src/game.rs` 的结算、`client/src/**`、`dispatch/`

## 5. 验收

```text
① 循环落在哪（文件/函数/行）
② 三个 headless 断言的实测结果（60 次 / delta=1/30 的追赶）
③ 顺序是"先 tick 后泵"吗（怎么证的）
④ 两份 coverage 生成物是否同提交
⑤ cargo test 1638；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、循环位置、三个断言结果、顺序怎么证的、生成物是否同提交、`cargo`/`--check` 结果。
