# 50 · `agent/turndata-ordering`：`TurnData[+0x58]` 的顺序回合分支

**优先级 P1。第十轮报告 §8 第 5 条 —— 现在 `is_on_turn` 是原版**超集**，画面上可能多画。**

## 0. 现状

```
46 号把 EntityDrawer 的门控换成真查询（is_on_turn = 0x462b50）
但它自己的遗留写着：
  · `TurnData[+0x58]` 顺序回合分支 **未建模** —— 本层无 mode 字段与"当前玩家游标"
    ⇒ 我方 is_on_turn 是原版**超集**（会比原版多画出"能画"的实体）
  · `get_alive == 2` 为什么算可画 —— **未知**（draw 用整字节判据，2 也过）
```

## 1. 要做什么

1. **解 `TurnData[+0x58]`**：那个 u32/u64 是什么（顺序回合的游标？模式位？），
   逐条从 `0x462b50` 与它的调用者读出来；
2. **把顺序回合分支建模**：`is_on_turn` 在"顺序回合 + 某人正在行动"时要返回什么；
3. **解 `get_alive == 2`**：`2` 是什么状态（排队死亡？刚复活？）——
   为什么 `draw` 用整字节判据把它算作可画；
4. headless 断言：
   ```
   顺序回合、轮到 A → 只有 A 的实体画
   同时回合 → 所有活着的都画（不许比原版多）
   get_alive == 2 的实体 → 按原版规则（画/不画）
   ```

## 2. 硬约束

- 每条结论要么指指令、要么指运行时观测，否则标 `推断/未知`；
- 数字不许退：`cargo test --offline` **1638**、三轴 `fire 647/1075`、`batch 647/1075`、
  headless `entity_drawer 292 / assemble 217 / plane_on_screen 96`；
- ⚠️ gdext 构建三步。

## 3. 环境

- 克隆到 `<WS>/.agent-work/turndata/repo`，分支 `agent/turndata-ordering`
- **私有** `CARGO_HOME=<WS>/.cargo-home`、`CARGO_TARGET_DIR=<WS>/.agent-work/turndata/target`
- 证据：x86 目标；**Windows 2.0.60（939 个真名）**查调用关系最快
- 工具 `tools/addr_probe.py`、`tools/loc_xrefs.py`
- push：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin HEAD:refs/heads/agent/turndata-ordering`

## 4. 可以改的文件

- `src/graphwar_turn_manager.rs`、`src/_scaffold/entity_drawer_class.rs`
- `client/tools/entity_drawer_check.gd`、`docs/16-client.md`
- ❌ 不许改 `src/game.rs`、`client/src/**`、`dispatch/`

## 5. 验收

```text
① `TurnData[+0x58]` 是什么（地址 + 语义 + 证据档）
② 顺序回合分支怎么建模的
③ `get_alive == 2` 是什么状态，为什么可画
④ 新增 headless 断言条数；`is_on_turn` 还会不会比原版多画
⑤ cargo test 1638；coverage --check OK；check_doc_refs rc=0
```

## 6. 汇报（≤8 行）

分支+sha、`+0x58` 的语义、顺序分支建模、`get_alive==2` 结论、新增断言数、会不会多画、`cargo` 结果。
