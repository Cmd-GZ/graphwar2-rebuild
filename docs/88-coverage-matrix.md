# 88 · 覆盖面矩阵：我的"绿"到底盖住了多大一块

## 0. 起因

用户问：「你之前是不是不知道有 `SimultaneousTurns` 模式？如果是的话，那 README 全绿也复原不了游戏吧。」

**答案是：不知道，而且他说得对。** `SimultaneousTurns` 是他实际去玩才暴露出来的。
我的 ✅ 是在**我采样到的那个表面上**绿，而那个表面本身从来没有被枚举过。

## 1. 游戏模式的实际空间（从 replay 实测取值）

| 维度 | 取值 | 种类 |
|---|---|---|
| `TurnMode` | `SequentialTurns`、`SimultaneousTurns` | 2 |
| `FunctionMode` | `NormalFunction`、`DiffEqFunction` | 2 |
| `TimeMode` | `Timer30s/1m/2m/3m/5m/Inf` | 6 |
| `AxisMode` | `EveryUnit`、`EveryFive`、`OnlyMain`、`NoAxis` | 4 |

组合空间 = 2×2×6×4 = **96**。

## 2. 哪些维度真的影响"我们复现的东西"

| 维度 | 影响物理？ | 依据 |
|---|---|---|
| `FunctionMode` | **是** | Normal 用 `y=f(x)`；DiffEq 用 **RK4 积分**（`path::calculate` 的 `Mode::Ode`）；生成器变量槽也变（`docs/71` §5.0） |
| `TurnMode` | **是** | Sequential 每回合一发；Simultaneous 同 tick 多发同时结算 |
| `TimeMode` | 否 | 只影响玩家思考时间 |
| `AxisMode` | 否 | 只影响坐标轴绘制 |

⇒ 真正需要覆盖的物理维度只有 **2×2 = 4 格**。

## 3. 覆盖矩阵（诚实版）

| FunctionMode | TurnMode | 物理 | AI 生成器 | 证据 |
|---|---|---|---|---|
| Normal | Sequential | ✅ | ✅ | 224 例 golden；21 发终点差 <0.5；20 发击杀一致 |
| Normal | **Simultaneous** | ⬜ **进行中** | ⬜ 未验 | 子 agent 正在判定结算顺序（`docs/86`） |
| DiffEq | Sequential | ✅ 采样 | 🔶 只验了 y 权重 | `path.tsv` 112 例 Ode；`docs/71` §5.0 |
| DiffEq | Simultaneous | ⬜ | ⬜ | 完全没碰过 |

## 4. 我错在哪（方法论）

1. **把"采样到的"当成了"全部"**：`README` 的 ✅ 写得像是全称判断，
   实际只是"在我见过的模式下成立"。
2. **没有主动去枚举枚举值**：二进制里的模式枚举完全可以直接读出来，
   我应该先枚举再验证，而不是等着被对局撞见。
3. **"绿"的表述没有限定域**：正确写法应该是
   「在 SequentialTurns × NormalFunction 下，物理与 AI 已验证」，
   而不是笼统一行 ✅。

## 5. 改进

- 本文件作为覆盖面的**唯一清单**，README 的每一行都必须指向这里的某一格；
- 新增模式时先补这一格，再谈"绿"；
- 已知待补：Simultaneous 结算顺序（进行中）、DiffEq × Sequential 的 AI 统计、
  DiffEq × Simultaneous（零覆盖）。

## 6. 仍然成立的结论

`docs/83`/`84` 那些**逐位验证**的项不受影响——它们比对的 oracle 是二进制/游戏自身，
不依赖"我有没有见过某个模式"：

- 采样器（含 ODE）224 例 golden
- 掩膜光栅化逐字节
- 修正常数 6 位一致
- 适应度公式反汇编

受影响的是**"观测类"结论**（终点差、击杀数），它们只在那几局覆盖到的模式里成立。
