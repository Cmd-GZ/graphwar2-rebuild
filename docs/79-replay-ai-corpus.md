# 79 · 从 replay 里按等级分离 AI 函数（真实数据验证）

## 1. 实体关联规则（弄清楚了）

replay 里的 id 有三种，之前搞混过：

| 事件 | 字段 | 含义 |
|---|---|---|
| `PlayerInfo` | `player_id`、`connection_id` | 玩家 |
| `NameInfo` | `entity_id`、`name` | **`entity_id` 就是 `player_id`**（同一个号） |
| `SoldierInfo` | `soldier_id`、`player_id` | 士兵 → 所属玩家 |
| `FunctionInfo` | `owner_id` | **是 `soldier_id`**，不是 player_id |
| `FunctionFire` | `soldier_id` | 开火的士兵 |

所以正确链路是：

```
FunctionInfo.owner_id (soldier_id)
   → SoldierInfo{ soldier_id → player_id }
   → NameInfo{ entity_id == player_id → name }
```

**之前失败的教训**：我直接拿 `soldier_id` 去 `NameInfo` 里查，当然查不到——
`NameInfo.entity_id` 与 `soldier_id` 是两个不同号段。

## 2. 两局样本（用户指定）

`1793874543`（1471 事件、56 回合）与 `1793873534`（882 事件），**覆盖全部 6 个机器人**：

| 机器人 | 等级 | 该局 player_id |
|---|---|---|
| ChatGPT | 1 | 630 |
| Wolfram | 2 | 633 |
| Alpha Zordon | 3 | 636 |
| Multivac | 4 | 639 |
| Deep Blue | 5 | 642 |
| Deep Thought | 6 | 651 |

（两局里 id 有偏移，但名字→等级的对应是固定的。）

## 3. 结果：**复杂度上限被真实数据验证**

按机器人分离后的开火函数：

| 机器人 | 等级 | 开火数 | 去重 | token 范围 | 推断的上限 | **超限数** |
|---|---|---|---|---|---|---|
| ChatGPT | 1 | 12 | 8 | 1–4 | 5 | **0** |
| Wolfram | 2 | 8 | 7 | 1–4 | 5 | **0** |
| Alpha Zordon | 3 | 9 | 9 | 2–8 | 10 | **0** |
| Multivac | 4 | 11 | 10 | 2–10 | 15 | **0** |
| Deep Blue | 5 | 10 | 9 | 2–23 | 25 | **0** |
| Deep Thought | 6 | 10 | 10 | 2–18 | 200 | **0** |

**47 个真实函数，没有一例越过上限**，而且**最大 token 数随等级递增**
（4 → 4 → 8 → 10 → 23 → 18，最后两档样本少所以有交叉）。

⇒ `docs/74` §4 那张"难度 → `limit` = 5/5/10/15/25/200"的表，
从**运行时读内存**和**真实对局函数长度**两条独立路径都得到了支持。

## 4. 各等级的样本函数（真实 AI 输出）

```
[1] ChatGPT       1  1770417.48        ← 那个靠微调漂移出来的超大常数
                  2  sin(x)
                  3  sin(abs(x))
[2] Wolfram       1  0.00
                  4  (x/-2.95)
[3] Alpha Zordon  2  cos(3.13)
                  7  abs((0.33*(-6.80+x)))
                  7  abs((0.33*(-6.95+x)))   ← 同族微调漂移
[4] Multivac      5  (x/(1.17+x))
                  8  (cos((-5.02+x))/-1.47)
[5] Deep Blue     6  (sin((0.16*x))*7.57)
                  8  (cos((ln(abs(x))+x))+8.65)   ← ln 确实会出现
[6] Deep Thought  9  (-9.17/(0.84+(-11.16+x)))
                 10  tan((sin(((14.95+x)/-2.84))+0.46))
```

几个有意思的观察：

- **难度 1/2 的函数极短**（1~4 个 token），经常就是一个常数或 `sin(x)`——
  和"上限只有 5"完全吻合；
- `(0.33*(-6.68+x))` 与 `(0.33*(-6.95+x))` 这种**同族只差常数**的组合，
  正是 `const *= (1+N(0,1))` 微调漂移（`docs/73`）的直接证据；
- `1770417.48` 出现在**难度 1**——低难度代数少（3 代）却有这么夸张的常数，
  说明漂移主要发生在**初始种群随机**（`N(0,10.2)` 的极端值）而不是长期累积；
- `ln` 在难度 5 的样本里出现了 ⇒ 印证 `docs/71` 的权重表（`ln` 权重 1/85 但仍会被抽到）。

## 5. 工具

`tools/parse_replay.py` 已支持批量抽语料；按机器人分离需要本地做一次
`soldier_id → player_id → name` 的映射（见 §1）。
