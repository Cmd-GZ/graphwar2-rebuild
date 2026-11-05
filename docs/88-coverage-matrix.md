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

## 7. 从二进制**推导**出的变体清单（不是靠记忆）

用户的关键批评：**就算 README 全绿，只要清单本身是"我记得查过的"，复原出来的二进制仍是残次品。**

所以清单必须从二进制推导。`.rodata` 里枚举变体名是**连在一起存的**：

```
0x1c18d5  SequentialTurns SimultaneousTurns rgb Connecting Setup WaitingForFunctions ...
          ↑ 回合模式：两者相邻，后面直接接游戏状态 ⇒ **只有 2 个**
0x1c63b4  ... NormalFunction DiffEqFunction NoAxis EveryFive EveryUnit Timer1m Timer2m Timer3m Timer5m ...
0x19f3a0  OnlyMain        （与其它变体不在同一段，容易漏）
0x19f768  Timer30s        0x19f1b0 TimerInf     （同样分散）
```

| 枚举 | 变体（二进制推导） | 数量 | 我们实现了吗 |
|---|---|---|---|
| `TurnMode` | SequentialTurns、SimultaneousTurns | **2** | 🔶 Sequential ✅ / **Simultaneous ❌** |
| `FunctionMode` | NormalFunction、DiffEqFunction | **2** | ✅ 两者（采样器 112+112 例；生成器 VarMode） |
| `AxisMode` | EveryUnit、EveryFive、OnlyMain、NoAxis | **4** | ❓ 只影响绘制，未单独验证 |
| `TimeMode` | Timer30s、Timer1m、Timer2m、Timer3m、Timer5m、TimerInf | **6** | ❓ 只影响思考时间 |

## 8. 结论：残次品的判定标准

> **一个复原产物是否残次，不取决于"已验证项的比例"，而取决于"是否覆盖了二进制声明过的全部变体"。**

按这个标准，当前的产物**确实是残次品**，缺口已经被点名：

| 缺口 | 严重度 | 状态 |
|---|---|---|
| `SimultaneousTurns` 结算 | **高**（影响物理） | 子 agent 判定中 |
| `AxisMode` 四种变体的绘制/判定参与程度 | 中 | 未验证（推测仅影响绘制） |
| `TimeMode` 六种是否影响 AI 预算 | 低 | 未验证（推测仅影响玩家） |

## 9. 正确的做法（已定为规矩）

1. 每新增一块复原，**先从二进制推导该块的完整变体集**（字符串表 / 跳转表 / 协议枚举）；
2. 把变体集写进本文件，逐项标注「已实现 / 未实现 / 不适用」；
3. 只有**全部变体都有归属**时，才允许在 README 上写 ✅；
4. 任何"用户玩出来才知道"的模式，都算**枚举失误**，要回头补第 1 步。
