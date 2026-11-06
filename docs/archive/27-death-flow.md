> **已归档（2026-11 文档归并）**
>
> 本文内容已并入 [`65-life-rules-confirmed.md`](../65-life-rules-confirmed.md) §1。
> 本文件只作**过程档案**保留：原始证据地址、命令输出都还在，但**不要引用其结论**。
> 逐份判定与归并映射见 [`107-docs-consolidation.md`](../107-docs-consolidation.md)。

# 27 · 死亡语义与回合状态机（真实对局抓包）

终于抓到 `alive:false`。数据来自一次**高难度 Bot**（`level:25`）的对局，
样本存 `fixtures/protocol_death.jsonl`，测试在 `src/_scaffold/protocol.rs::death_tests`。

## 1. 死亡流程（观测）

一条死亡在协议上留下三条记录，**tick 对齐**：

```json
{"LifeQueuedDeath":{"entity_id":231,"tick":238}}
{"EffectInfo":{"effect_id":236,"pos_x":-1.4658…,"pos_y":2.5807…,"effect_type":"DeathEffect","start_tick":238}}
{"LifeInfo":{"entity_id":231,"alive":false}}
```

- `LifeQueuedDeath{entity_id,tick}`：死亡的**计划**时刻（不是立刻生效）；
- `EffectInfo{... "DeathEffect", start_tick}`：**`start_tick` 与计划的 tick 相同**；
- `LifeInfo{alive:false}`：最终状态确认。

第二例同理：`entity_id=230, tick=953` ↔ `DeathEffect.start_tick=953`。

⇒ 判定模型是**二值**的：士兵要么活着、要么被排入死亡，
**没有观测到任何伤害数值/血量字段**。这与"一发命中即死"一致，但
"是否真的只有 1 点血"仍是推断（样本只覆盖一击致死的情形）。

## 2. 回合状态机（观测）

同一局里出现过的 `GameStateInfo`：

```
Setup → WaitingForFunctions → CalculatingResults → SynchronizingResults → ExecutingResults
```

这些状态名与"提交函数 → 服务端算结果 → 同步 → 执行"的流程吻合，
但**状态迁移的触发条件**（哪些 tick、谁驱动）还没验证。

## 3. 障碍销毁的两条消息

`QueueDestroyObstacle`（排队）与 `DestroyObstacle`（已销毁）是**同一件事的两个阶段**：

```json
{"QueueDestroyObstacle":{"pos_x":-21.9547…,"pos_y":6.6797…,"radius":0.5,"tick":…}}
{"DestroyObstacle":{"pos_x":-21.9547…,"pos_y":6.6797…,"radius":0.5}}
```

坐标完全一致；`radius` 恒为 `0.5`。→ 又一次印证 0.5 是**销毁半径**而非障碍半径。

## 4. AI 的函数形态（按难度）

| Bot 难度 | 提交过的函数 |
|---|---|
| 低（默认按钮） | `abs(x)`、`0.00`、`sin(x)` —— 像是在固定候选里挑 |
| 高（`level:25`） | `tan(sin((-0.12*x)))`、`abs((-3.49+x))` —— 明显是演化/搜索出来的 |

`abs((-3.49+x))` 里的 `-3.49` 很像是**目标 x 坐标的偏移**，
与 `function_evolutionator`（函数演化）的存在吻合。
斜率/偏移是否由目标位置算出**尚未验证**，因此这一条仍是观测。

## 5. 还缺什么

- 伤害数值（如果有血量系统）——样本只覆盖一击致死；
- 状态机的迁移条件；
- AI 打分函数的具体形式（候选集、变异、选择）。
