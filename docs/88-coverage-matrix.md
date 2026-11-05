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

## 3. 覆盖矩阵（诚实版 · 本轮更新）

| FunctionMode | TurnMode | 物理 | AI 生成器 | 证据 |
|---|---|---|---|---|
| Normal | Sequential | ✅ | ✅ | 224 例 golden（修复后与二进制 **24/24 逐点一致**）；停点差 <0.5；击杀 12 发一致 |
| Normal | **Simultaneous** | ✅ | 🔶 未单独验（口述"AI 只优化自己那一发"，§14） | 结算 = **几何并行 + 事件按飞行时间**；修复采样器后 82 发并集 **152/152**、多报 0 / 漏报 0、时刻判决 **38:0**（`docs/86` §14） |
| DiffEq | Sequential | ✅ 采样 | 🔶 只验了 `y` 权重 | `path.tsv` 112 例 Ode |
| DiffEq | Simultaneous | ⬜ | ⬜ | 完全没碰过 |

### 3.1 本轮的关键变化

| 项 | 变化 |
|---|---|
| **采样器缺陷已修** | 半步复检漏了 `hi = mid2`（反汇编 `0x3c2a0a`）。修复前"约 23% 弹道被截断"，修复后 24/24 逐点一致（`docs/91`） |
| **同时回合制重验** | 漏报从 23 → **0**，时刻判决从 23:3 → **38:0**（`docs/86` §14） |
| **实况步长** | 确认 **0.01**（`SERVER_STEP`）；此前多处误用 0.5 |
| **AI 文档** | 12 份逐轮记录合并为 **`docs/AI.md`**（唯一权威） |

### 3.2 仍开着的问题

| 项 | 状态 |
|---|---|
| ~~`tests/live_hits.rs` 用 0.01 时 4 个测试失败~~ | ✅ **已结案**（`docs/89` §33）：不是 `(-46.06/(7.94+x))` 的极点问题，而是测试 `first_hit()` 跳过射手身体的半径固定 0.5、撞上掩膜像素量化抖动（1 px = 0.05）；改 `SHOOTER_SKIP_R = 0.56` 后 **12 条全过，全程 0.01** |
| `events.rs` 的 33 个判别值 | **缺独立验证**（`docs/07` §2 的顺序假设在 `0x1bc000` 那段上不成立，`docs/96` §7） |
| 协议事件 | 实现 33 个，**缺 24 个名字**（`docs/96`），其中四个 `*ModeInfo` 广播最要紧 |
| ~~AI 时间预算 / 精英 5~~ | ✅ **已解**：终止 = 代数到顶 **∨ 25 s 墙钟**（`docs/93`，每代开头查一次、截断无标记）；精英 = 每代前 **20%**，字面量 5 = 每槽 1 克隆 + 4 工厂子代（`docs/95`）。⚠️ 仍未做：把 25 s 上限写进 `aigen::evolve` |
| 界面与脚本 | 子 agent 做台账中（`docs/92`） |

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
- 已知待补：DiffEq × Sequential 的 AI 统计、DiffEq × Simultaneous（零覆盖）、
  `AxisMode`/`TimeMode` 是否参与判定（§14 只有口述，§16.3 自评把握低）。
  原先列的"Simultaneous 结算顺序（进行中）"**已结案**（`docs/86` §14）。

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
| `SimultaneousTurns` 结算 | **高**（影响物理） | ✅ **已判定**（`docs/86` §14：几何并行 + 事件按飞行时间；0.01 下 152/152、38:0） |
| `AxisMode` 四种变体的绘制/判定参与程度 | 中 | 未验证（推测仅影响绘制） |
| `TimeMode` 六种是否影响 AI 预算 | 低 | 未验证（推测仅影响玩家） |

## 9. 正确的做法（已定为规矩）

1. 每新增一块复原，**先从二进制推导该块的完整变体集**（字符串表 / 跳转表 / 协议枚举）；
2. 把变体集写进本文件，逐项标注「已实现 / 未实现 / 不适用」；
3. 只有**全部变体都有归属**时，才允许在 README 上写 ✅；
4. 任何"用户玩出来才知道"的模式，都算**枚举失误**，要回头补第 1 步。

## 10. 界面/功能清单（从 `.pck` 场景推导，**不是靠记忆**）

> **2026-11 补正**：本节当时只能"从场景名推导"。界面已被**运行时枚举**取代——
> 15/15 界面、283 个按钮、每个控件的文本/图标/作用都在 [`docs/92`](92-ui-inventory.md) 里，
> 本节几处"未查"（Player / 房间 IP / ShowGlobal）已在那份文档结案。以 `92` 为准。

用户指出：即使核心逻辑全绿，只要功能清单是"我记得查过的"，产物仍残次。
`Player`、`Replays` 这两个界面我此前**从未查过**（`Replays` 还是用户告诉我的）。

从 `rsc/*.tscn.remap` 可枚举出**全部 31 个场景**（2026-11 核对：
`find rsc -name '*.tscn.remap' | wc -l` = **31**；本文件旧版写"27 个"、列表只有 24 个名字，是漏枚举）：

```
main_menu campaign GameSetup lan_lobby play_screen player_screen
replay_lobby replay_entry shop_button_entry game_room_entry
team1_entry team2_entry name_label function_buttons function_buttons_no_control
line_2d main Main Ump Banner
Interstitial Rewarded RewardedInterstitial
add_bot_pop_up chat_pop_up confirm_exit_pop_up connecting_pop_up
disconnected_pop_up endgame_pop_up join_ip_pop_up join_level_pop_up
```

二进制里的控件类名另有一批（`ReplayLobby`、`PlayerScreen`、`ReplaysContainer`、
`PlayReplayButton`、`SkinBackgroundButton`、`ShowGlobalButton`、`ShowLocalButton`、
`JoinIpPopUp`、`AddBotPopUp`、`ConfirmPlayExitPopUp`、`TurnTimeButton` …）。

### 与用户口述对照

| 用户说的 | 对应场景 | 我们查过吗 |
|---|---|---|
| 主界面 4 个按钮（Campaign / Battle / Player / Replays） | `main_menu` | ❓ 未逐项核对 |
| Campaign 21 关（0–20，逐关解锁） | `campaign` / `join_level_pop_up` | ✅ 关卡生成器已逆（`docs/60`+） |
| Battle：Local/Global、Create/Join IP、四模式、加 AI（6 级） | `lan_lobby` / `GameSetup` / `join_ip_pop_up` / `add_bot_pop_up` | 🔶 四模式已枚举；**6 个 AI 等级**已逆 |
| Player：改名 + Skins/Faces/Hats | `player_screen` / `shop_button_entry` / `skin_background_button` | ❌ **从未查过** |
| Replays：回放存储 | `replay_lobby` / `replay_entry` | ✅ 用户在 `docs/78` 指出后才查 |
| 房间 IP/端口显示 | `game_ip_label` | ❓ 未查 |

### 判定

**当前产物是残次品**，缺口按严重度：

| 缺口 | 严重度 |
|---|---|
| `SimultaneousTurns` 结算 | 高 |
| `Player` 界面 / 皮肤系统（Skins/Faces/Hats） | 中（不影响对局物理，但属功能缺失） |
| `Replays` 回放播放器 | 中 |
| `AxisMode`/`TimeMode` 对判定的参与度 | 中/低 |

### 规矩补充

**功能清单的权威来源是 `.pck` 的场景与控件名，不是任何人的记忆。**
每轮开工前先列这份清单，逐项标状态；"用户玩出来才知道"的功能一律算枚举失误。

## 11. 决定性发现：**有 105 个编译脚本 + 31 个编译场景**

> **2026-11 补正**：数字需修正——`.tscn.remap` 只有 **25** 个，31 个 `.scn` 里 6 个是
> AdMob 插件场景；且"界面/信号/处理器全在 `.gdc` 里"**不成立**：51 个非插件脚本基本是
> 字号/间距助手，处理器在 Rust。见 [`docs/92`](92-ui-inventory.md) §2.3、§5、§7。

用户指出「这游戏根本没做混淆，连壳都没有」——核对 `rsc/`（已解包的 `.pck`）：

| 类型 | 数量 | 含义 |
|---|---|---|
| `.gdc` | **105** | **编译后的 GDScript**（可反编译回接近源码） |
| `.scn` | **31** | **编译后的场景**（对应那 31 个 `.tscn.remap`） |
| `.ctex` / `.res` / `.import` | 167 / 22 / 184 | 贴图与资源 |

⇒ **界面、按钮、信号、处理器全都在 `.gdc` 里，可以系统地一次性拿全。**

## 12. 因此我的路线错了

我一直在用**反汇编 + 运行时 hook 去猜**行为（AI 参数、适应度、装配规则…），
而**编译脚本本身就能反编译出接近原始源码**。

在"没有混淆"的前提下，正确顺序应当是：

1. 反编译全部 105 个 `.gdc` → 得到全部界面逻辑与处理器；
2. 解出 31 个 `.scn` → 得到完整的控件树（每个界面有哪些按钮/选项）；
3. 剩下的**原生 Rust 部分**（弹道/掩膜/AI 进化）才需要 REA + 反汇编。

我把顺序做反了：先啃最难的二进制，却放着现成的脚本没动。

## 13. 下一步（明确）

- 装一个 GDScript 反编译器（Godot RE Tools / gdsdecomp 的 `.gdc` 反编译，或按 GDScript
  字节码格式自解析），把 105 个脚本全部还原；
- 用 `Godot RE Tools` 把 31 个 `.scn` 转回 `.tscn`，得到完整控件树；
- 据此产出**完整功能台账**（界面 × 控件 × 信号 × 处理器），逐项标实现状态。

## 14. 用户口述的简化假设（**未验证，按口述记录**）

用户说明（"大概可以想当然"）：

> **坐标格子粗细（`AxisMode`）与回合模式（`TurnMode`）都不会影响 AI 行为。**

若成立，则覆盖面大幅收窄：

| 维度 | 对 AI 的影响 | 对结算的影响 |
|---|---|---|
| `AxisMode`（EveryUnit/EveryFive/OnlyMain/NoAxis） | **无**（纯绘制） | 无 |
| `TurnMode`（Sequential/Simultaneous） | **无**（AI 只优化自己那一发） | **有**（同 tick 多发如何结算） |

⇒ 我之前列为"高严重度"的 `SimultaneousTurns`，**对 AI 部分其实不构成缺口**；
它只影响"多发同时命中"的结算语义，属物理/结算层，不是 AI 层。

**状态：口述证据**。可在成本低时用一局对照验证（同一局面下 AI 选出的函数是否与模式无关），
但优先级低于补齐脚本反编译。

## 15. 待办顺序（按用户指示）

1. **先把 AI 部分收尾**（装配规则已解 `docs/87`；剩：精英 5 的用途、统计验收）；
2. 再回头处理界面/脚本这一大块（105 个 `.gdc` + 31 个 `.scn` 的反编译）；
3. `SimultaneousTurns` 结算顺序作为物理层缺口单独跟踪。

## 16. 用户提出的两个待查点（按口述记录，置信度不同）

### 16.1 有无"时间限制"（AI 每回合的预算）

**不确定**。需要查：进化循环里是否有墙钟/帧数上限，还是只按代数终止。
若存在，难度 6（1000 代）在实际回合时间内可能跑不完 ⇒ 样本会是"被截断"的，
这会影响我们按代数复现的正确性。

### 16.2 `FunctionMode` 的实现方式：生成后再裁剪 `y`？

用户转述 **GW1** 的做法：**随机生成函数后，按模式决定是否裁掉 `y` 项**。

GW2 是否同构**未定**，需要自己查。两种可能：

- **A（我们现在的实现）**：分派表本身就是模式相关的（Normal 的 10 个变量权重全给 `x`，
  DiffEq 拆成 `x`6+`y`4）——`docs/71` §5.0 实测支持这一点：
  在分派器 `0x4ef2b0` 的**输出端**直接看到 tag 只在 DiffEq 下出现。
- **B（用户转述的 GW1 式）**：先生成含 `y` 的流，再按模式把 `y` 映射/删除。

**关键判断**：我们在**分派器出口**就观察到了模式差异，说明到那一步为止模式已经生效。
但无法区分"表不同"与"出口后立刻改写"。

**行为影响**：
- 若是 `y → x` 的**同位置替换**：与我们的双表实现**完全等价**（`y`/`x` 都是零元叶子，
  不影响 arity 计数与 RNG 消耗），无需改动；
- 若是**删除** `y` token：会改变流长度与 arity，需要重新装配 ⇒ 与现实现不等价，必须改。

⇒ 待查：二进制里是否存在"按模式改写 token 流"的独立 pass（找把 tag 1 改成 0 的循环）。

### 16.3 `TurnMode` 不影响 AI —— **用户自评把握较低**

比 `AxisMode` 那条更没把握。因此 `docs/88` §14 的简化**不能当作已确认**，
需用一局对照实验（同一局面、两种回合模式，看 AI 选的函数是否一致）来验证。
