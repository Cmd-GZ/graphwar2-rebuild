# 143 · 对局流程的六条硬伤：证据、修法、新旧行为

状态：**现行**。本文只回答一件事：用户实测的"看起来能玩，但流程是错的"那 6 条
（+2 条附带项）**各自凭什么改**、**改成了什么**、**哪些仍然是推断/未知**。

> 前置：`docs/131`（胜负判定）、`docs/135`（弹道 tick 动画）、`docs/128`（音频）、
> `docs/109`（`src/turn.rs` 与 `aigen`）、`docs/41`/`docs/43`（回合状态机与报文顺序）、
> `docs/118` §7.1（关卡参数）、`docs/65`（`LifeQueuedDeath` 的 tick）、`docs/31`（结算→事件）。
> 本文**没有**重新挖二进制，除 §4.1 那一张跳表是本次新读的（原文在 §4.1 给出）。
>
> 证据分级沿用 `AGENTS.md`：**逐位验证** / **观测** / **推断** / **未知**。
> 地址一律 **ELF 虚地址**（Ghidra 地址 = vaddr + `0x100000`，只加一次）。

---

## 0. 一页结论

| # | 用户现象 | 根因（本次定的） | 修法 | 档位 |
|---|---|---|---|---|
| 1 | 函数碰壁没有爆炸效果 | `PlayScreen` **根本没有效果层**：`EffectInfo` 在本地局不存在，联机局只记录不绘制（`docs/139` §7 的旧账） | 新增 `GwEffectLayer`（`client/src/game/effect_layer.gd`）+ 在**弹道停止点**发 `ExplosionEffect`、在**死者站位**发 `DeathEffect`/`AshEffect` | 效果名/位置/时机 **逐位+观测**；画法 **自定** |
| 2 | 函数一直留到下一次发射 | 曲线没有任何清除点（`fire()` 只会覆盖它） | 弹道**飞到尽头**就 `_plane.clear_path()` | **用户实测**（无二进制判定点，见 §3.3） |
| 3 | 音效缺失 | 事件↔音效映射 0 条确定（`docs/128` §4.1），本地局连"什么时候该响"都没有 | 按效果事件出声；**槽位**用二进制跳表，**音效身份仍是自定** | 槽位 **逐位**、音效 **自定** |
| 4 | 运动过程中再次发射会打断当前函数 | `fire()` 没有任何"在飞"判断 | `_shot_in_flight` 拦截，并写清拒绝理由 | **观测**（状态机）+ 客户端自定实现 |
| 5 | **还没击中目标就判胜利** | `fire()` 里**同步**做了 `_resolve_shots → _settle_round → _check_match_over`：开火当刻就判胜负、当刻就弹结算框 | 结算**延后到弹道飞完**；死亡在**弹道走到那个点**才落地 | 顺序 **观测**（`docs/41`/`docs/43`）+ tick 语义 **观测**（`docs/65`） |
| 6 | 对面是 AI 时卡住不动 | ① 回合推进靠"人类又按了一次 Fire"（`_next_turn()` 只在自己 `fire()` 之后调）；② **轮到 AI 之后没有任何人开火** | 回合推进改为**结算完成时**由 `GwTurnDriver`（规则 = `src/turn.rs`）驱动；轮到 AI 就倒计时后由 AI 自己开火 | 轮转规则 **逐位**；AI 的那一发 **自定** |
| 附 1 | 默认士兵数是 1 | `begin_match()` 无条件用玩家表的 `soldiers`（默认 1），**忽略关卡数据** | 战役关按 `docs/118` §7.1 的「我方士兵」列装人 | **逐位 + 运行时**（21/21） |
| 附 2 | 成就/队伍图标过小 | 定位到 `client/src/screens/game_setup.gd:316` 的 `RankIcon` : `custom_minimum_size = Vector2(24, 24)` | **没改**（不在本任务的可改文件里，且原版尺寸无证据）——见 §8 | **未知** |

**新增文件**：`client/src/game/effect_layer.gd`、`client/src/game/turn_driver.gd`、本文。
**改动文件**：`client/src/screens/play_screen.gd`、`client/tools/selftest.gd`（**只加断言**）。
**没动**：`client/src/play/fire_fx.gd`、`client/src/play/plane_view.gd`、`src/`（Rust 一行没改）、
`client/src/app/*`、其它界面。

验证：`cargo test --offline` **333 passed**（一条没变，本轮没动 Rust）；
`bash client/run.sh selftest` **635 passed, 0 failed**（基线 581 → +54，既有断言一条没删没改）。

---

## 1. 硬伤 ①：函数碰壁没有爆炸效果

### 1.1 原版什么时候发哪一种效果

| 效果 | 什么时候发 | 位置 | 档位 / 出处 |
|---|---|---|---|
| `ExplosionEffect` | **每一条弹道执行到尽头时**（撞墙、或飞出平面） | **弹道停止点** | **逐位 + 观测**：`docs/31` §2/§3（`tests/live_hits.rs` 里与抓包对拍）、`fixtures/protocol_death.jsonl` 的三条坐标在 ±25（= 平面边界）；报文顺序见 `docs/43` §1：`FunctionActive` 之后紧跟 `EffectInfo(ExplosionEffect)` |
| `DeathEffect` | 某个士兵被判死的那一 tick | **死者的站位**（不是弹道停点） | **逐位**：`docs/31` §3 的坐标与抓包逐位相同（`1e-9`）；`docs/65` §1 两例证明 `DeathEffect.start_tick == LifeQueuedDeath.tick`（238/238、953/953） |
| `SparkEffect` | 观测到的只有**造靶子**时（`EntityEffectInfo{effect_id, effect_type: SparkEffect}`） | 靶子位置 | **观测**（`docs/123` §90）。**靶子被打掉**也用它 —— **推断** |
| `AshEffect` | 与 `DeathEffect` 同源（每人恰好 1 条，坐标同为站位） | 死者站位 | **观测**（`docs/86` §264：152 个死亡 152/152）+ 与 Death 的先后 **推断** |

效果名只有这 4 个：`.rodata` `0x1c64e5 ExplosionEffect(15)` / `0x1c64f4 DeathEffect(11)` /
`0x1c64ff SparkEffect(11)` / `0x1c650a AshEffect(9)`（**观测**，`docs/128` §3.2 原文）。

### 1.2 我们在哪里：以前一直没画

`PlayScreen` 里**没有**任何效果绘制；联机分支收到 `EffectInfo` 只 push 进 `_online_effects`
（`docs/139` §7 记为未做）。本地局连"发不发"都没有这一层。

### 1.3 修法

* 新增 `client/src/game/effect_layer.gd`（`GwEffectLayer extends Control`），挂在棋盘上
  **`GwFireFx` 之后**（子节点按加入顺序绘制 ⇒ 特效压在弹道之上）。
  它只做三件事：`spawn(kind, pos)`、按时间老化、`_draw()`。**不碰判定、不碰曲线**。
  （放在 `client/src/game/` 是为了不碰 `plane_view.gd` 与 `fire_fx.gd` —— 后者由另一位
  agent 在改准备圈与弹道。）
* `PlayScreen._spawn_effect(kind, pos)`：效果 + 音效（§4）+ 诊断流水 `_fx_log`。
* **发在哪一刻**（这是 ⑤ 的一部分）：
  * 死亡：`_apply_kills_up_to(reveal)` —— 弹道揭示到"死者那个点"时才发（见 §5）；
  * 爆炸：`_on_shot_arrived()` —— 弹道飞完时，位置取 `hit_test` 回来的 `stop`。
* 联机：`_on_online_message("EffectInfo")` 现在**真的画**（以前只记录）。

### 1.4 仍是自定/未知

* **画法**（半径、时长、颜色、粒子形状）全是自定 —— 原版由 Rust `entity_drawer` 画
  （`0x4b2890` 是它的 `draw`，见 `AGENTS.md` 的"间接调用"一节），**没有**任何像素或
  二进制证据给出这些常数。本层**不声称**与原版同形（`docs/121` §7 的老账照旧）。
* `AshEffect` 与 `DeathEffect` 的**先后**、`SparkEffect` 用来表示"靶子被打掉"：
  **推断**。

---

## 2. 硬伤 ②：函数不会消失

### 2.1 以前

`fire()` 里 `_plane.set_path(...)` 之后**再也没有任何地方清它**；下一次发射只是把它覆盖掉。
于是"打完这一发，曲线一直挂在那里，直到下一次发射"。

### 2.2 现在

`_on_shot_arrived()`（弹道飞完）里 `_plane.clear_path()`。真机时序下就是"火球炸开、
线消失"；自检断言 `path_points == 0`。

### 2.3 证据（这一条最弱，必须写清楚）

* **用户实测**：原版"发射完就清掉"。
* **没找到**二进制里的判定点：`FunctionActive{owner_id, active_tick}` 是"这一发开始执行"，
  但"执行结束把线擦掉"这一步在 `play_screen_control` / `entity_drawer` 里**没有定位到具体函数**。
* ⇒ 档位：**用户实测 + 推断（时机取"弹道飞完"）**。缺的证据：一次带时间戳的原版真帧序列
  （或 `entity_drawer` 里那条 `draw_line` 的调用条件）。

---

## 3. 硬伤 ③：音效

### 3.1 以前

`docs/128` §4.3 的三条 `PENDING_MAPPING` 接线：`Fire!` → `explosion@0xa0`、胜负 → `unpop`/`squash`。
**事件↔音效映射 0 条确定**，本地局里"命中/死亡完全没有声音"。

### 3.2 本轮新读的一条证据（`0x4bf090` 的 4 臂跳表）

`docs/128` §3.2 只读到"按 `event+0x20` 一个字节分派 4 臂"，没解出**哪一臂对应哪个效果**。
本次把跳表解了（`objdump` + 直接读 4 个 `int32`）：

```text
0x4bf0b4  movzbl 0x20(%rsi),%eax          ; 分派依据：effect_type 的判别值 0..3
0x4bf0bc  lea    -0x2fc907(%rip),%rcx     ; 0x1c27bc = 跳转表
0x4bf0c3  movslq (%rcx,%rax,4),%rax
0x4bf0ca  jmp    *%rax
```

| `effect_type` | 表项（相对 0x1c27bc） | 落点 | 落到的 player 槽 |
|---|---|---|---|
| 0 | `+0x2fc910` | `0x4bf0cc` → `0x4bf21d` | **`0x80`** |
| 1 | `+0x2fc992` | `0x4bf14e` → `0x4bf263` | **`0x90`** |
| 2 | `+0x2fc93f` | `0x4bf0fb` → `0x4bf183` | **`0xa0`** |
| 3 | `+0x2fc963` | `0x4bf11f` → `0x4bf1d0` | **`0xa0`** |

（4 臂的落点原文都是 `lea 0x80/0x90/0xa0(%rbx),%rdi` + `movss 1bb818`= −15.0 +
`call 0x4bed70`，参数 `r9d = 0x1c(%r14)`。）

**这是一个负结果，但它把一条路封死了**：4 个臂读的都是**同一个流字段**
（`mov (%rdi),%r12` / `mov 0x68(%rbx),%r15`，`rdi = self+0x60`）并 `clone` 它，
**只有槽位不同**。⇒ 从 `effect_info_event` 这条链上**拿不到"哪个效果播哪个音"**
（流身份是更早、在 `0x4bfe10` 里写进 `self+0x60` 的，45 KB 的 Variant 机械；
`docs/128` §8 第 1 条就是它）。

⇒ 本轮的取舍：**槽位照二进制（逐位），音效身份仍标自定**，并把这条负结果写进代码注释
（`SLOT_FOR_EFFECT` / `SFX_FOR_EFFECT`），避免下一个人再从跳表上做无用的推断。

### 3.3 现在的接线

| 事件 | 槽位（逐位） | 音效（自定） | 出现时刻 |
|---|---|---|---|
| `ExplosionEffect` | `0x80` | `explosion` | 弹道飞完（停止点） |
| `DeathEffect` | `0x90` | `squash` | 弹道走到死者那个点 |
| `SparkEffect` | `0xa0` | `spark` | 靶子被打掉 |
| `AshEffect` | `0xa0` | `pop` | 与 `DeathEffect` 同时 |

⚠️ **与 `app.gd` 的冲突（需父 agent 处理）**：`App` 里那条 `Fire!` → `explosion` 的
`PENDING_MAPPING` 接线仍在，于是真机上按 Fire 会响一次 explosion、2.5 s 后弹道落地再响一次。
建议把 `app.gd` 的 `SFX_FIRE` 那一条**删掉**（`app.gd` 不在本任务可改范围）。

---

## 4. 硬伤 ④：发射期间不能再发

### 4.1 证据

* 真对局的 `GameStateInfo` 序列是**状态机**（`docs/41` §1：3 个回合的 14 个状态完全一致）；
  `src/turnstate.rs` 的 `next_state()` 里 `Fire` **只**从 `WaitingForFunctions` 出发
  （13 次迁移来自同一份抓包，`reproduces_captured_state_sequence` 逐条对拍）。**观测**。
* "服务端在 `ExecutingResults` 期间不收 `FunctionFire`"是**推断**（状态机语义，没有
  "玩家在弹道飞行中再发一次"的抓包样本）。

### 4.2 现在

`fire()` 第一道闸：`if _shot_in_flight: 拒绝，并在状态行写"上一发还在飞（服务端此时是
ExecutingResults）⇒ 不能再发"`。另外两道：不是人类回合（`active_soldier.is_bot`）也拒。

自检断言：第二发（**另一条长度不同的曲线**）不会替换在飞的记录、不消耗回合。

---

## 5. 硬伤 ⑤（最严重）：还没击中就判胜利

### 5.1 根因（三步都发生在 `fire()` 这一次调用里）

```text
_resolve_shots([shot])   → 立刻翻 alive（死者当场死）
_next_turn(true)         → 立刻换手
_settle_round()          → 立刻 turns_used+1 → 立刻 _check_match_over() → 立刻弹结算框
```

⇒ 用户按下 Fire 之后**同一次调用**里：靶子死了、胜负判了、结算框弹了 —— 而画面上的火球
**还没出发**（`GwFireFx` 只是把点序列的揭示长度从 0 开始推）。

### 5.2 原版的顺序（证据）

| 步 | 原版 | 档位 / 出处 |
|---|---|---|
| 算结果 | `CalculatingResults → SynchronizingResults`：服务端**立刻**算完并广播 `FunctionPoints` + `LifeQueuedDeath` + `EffectInfo` | **观测**（`docs/43` §1 的报文顺序；`docs/41` §1） |
| 死亡落地 | `LifeQueuedDeath{tick}` 的 tick 才死；`EffectInfo(DeathEffect).start_tick` 与之相等 | **观测**（`docs/65` §1 两例） |
| 结束一个回合 | `ExecutingResults → TurnCountInfo → WaitingForFunctions` | **观测**（`docs/43` §1、`docs/41` §1） |
| 判胜负 | `EndingGame` 出现在 `ExecutingResults` **之后** | **观测**（`docs/41` §1 的 14 状态序列） |

### 5.3 现在的实现（`play_screen.gd`）

```text
fire()                 → 算全量点序列；_judge_shots() **算出命中表**（但不落地）
                       → GwFireFx.play()（弹道开始飞）；_shot_in_flight = true
每帧 _advance_presentation()
                       → 揭示到第 n 点 ⇒ 把 point ≤ n 的击杀落地（死亡/靶子/特效/音效）
弹道 finished          → _on_shot_arrived():
                         ├ 剩余击杀落地
                         ├ 在 stop 点发 ExplosionEffect
                         ├ _plane.clear_path()            ← ② 的修法
                         └ _finish_round() = _next_turn(true) + _settle_round()
                                             └ _settle_round 里才 _check_match_over()
```

**判定只有一份代码**：`_judge_shots()`（纯计算）+ `_apply_kills_up_to()`（落地）+
`_settle_round()`（回合/复原/胜负）。逐 tick 与"一次性结算"两条路径都调它们。

### 5.4 关掉 `automation` 才看得到这条

`App.automation`（`--selftest`/`--dump-tree`）下**把时序压平**：逻辑在开火当刻落地，动画照放。
理由：既有 6 组端到端断言都是"开火 → `_frames(1~2)` → 读结算结果"，压平后它们仍然成立
（**一条没改**）；不压平就得改那些断言，而任务不许。

⚠️ 代价要写清：**既有断言不再覆盖真机时序**。所以本轮新增的 54 条断言里，
`_test_play_flow()` 整组**把 `automation` 置为 false** 跑真机时序，逐条断言
"开火那一刻：靶子还活着 / 没判胜负 / 不弹结算框 / 回合数还是 0"，然后
`advance_shot_for_test()` 走完整条弹道再断言结果 —— 这样"先后顺序"这条才算被验证过。

### 5.5 仍未定

* **tick ↔ 时间的换算**：弹道按 60 tick/s、0.29 单位/tick 推进（`docs/35` 观测 +
  `docs/86` §6 推断）；原版 `active_tick`/`end_of_turn_tick` 用的是**另一套时间基准**
  （`docs/41` §3 已记对不上）⇒ 我们的"飞行时长"是**近似**，不是复原值。
* 同时回合制（`Level 11` / `Level 14`）**没有**接逐 tick 时序，仍是"整批一次落地"
  （见 §9）。

---

## 6. 硬伤 ⑥：对面是 AI 时卡住

### 6.1 根因：两个都要修

1. **推进点错了**：`_next_turn()` 只在 `fire()` / `apply_external_shot()` 之后被调用 ——
   也就是"人类又按了一次 Fire"才推进一次。原版是**服务端**在 `TurnCountInfo` 之后广播
   `TurnInfo{player_id, soldier_id}`（`docs/98` §1.4、`docs/109` §2.2）。
2. **轮到 AI 之后没有任何人开火**：客户端根本没有 AI 回合这一步（`docs/131` §6 #9 记的
   就是这条旧账），于是回合停在 Bot 身上，界面看起来"卡住"。

### 6.2 修法一：回合轮转按 `src/turn.rs` 的规则

新增 `client/src/game/turn_driver.gd`（`GwTurnDriver`）：**每玩家一张士兵表 + 游标**，
`next_shooter()` 先自增再取、最多试 `len` 次、跳过死士兵、全灭返回 `SOLDIER_ID_NONE`。

```text
0x463f40  mov -0x8(%rbp),%rax ; inc %rax ; div %r15 ; mov %rdx,-0x8(%rbp)   ; index = (index+1) % len
0x463f7?  mov (%rcx,%rdx,4),%ebx                                            ; soldiers[index]
0x463fb6  call 454950 ; test $0x1,%al ; jne 464050                          ; 存活 = 生命字节最低位
0x463fc7  dec %r14 ; jne 463f40                                             ; 最多 len 次
0x464016  movl $0x0,0x64(%rsp)                                              ; 全灭 ⇒ soldier_id = 0
```

（以上原文照录 `docs/109` §2.3 的 objdump；本轮**没有**重跑 objdump —— 见 §10 的"未复核"。）

**这顺带修掉一个既有 bug**：旧实现是 `(_active + 1) % n` 的**玩家**轮转 +
"该玩家的第一个活着的士兵"，于是多士兵关卡（`Level 3` = 2、`Level 5` = 2、`Level 9` = 3、
`Level 11` = 3、`Level 14` = 4、`Level 18`/`Level 19` = 3~4、`Level 20` = 5）里
**第二个以后的士兵永远轮不到**。而 `Level 3` 的描述原文是
"At each turn you will shoot from a different one."（**观测**，`docs/118` §7.1 的 `R` 列 2/2）。
现在轮到的是**具体士兵**，自检断言"第二回合的射手是 1 号槽（y = −1.7）"。

### 6.3 修法二：AI 的回合由 AI 自己开火

`_next_turn()` 发现新射手 `is_bot` ⇒ 起 `BOT_THINK_SECONDS`（0.6 s，**自定**）倒计时；
`_process` 里 `_tick_bot_turn()` 到点调 `bot_take_turn()`：
它自己算弹道（`GwCore.calculate_path`）、自己 `_commit_shot` —— 与人类走**同一条**
结算路径（`_judge_shots` → 逐 tick → `_finish_round`）。

AI 的那条函数是**自定**的：一条过射手、指向最近敌人的直线
（`m = (y敌 − y我)/(x敌 − x我)`；修正常数 `c = y0 − f(x0)` 会把这条线钉在射手身上，
见 `docs/24` §6.2）。
**这不是原版 AI**：原版是 `aigen` 的进化器（`docs/109` §4、`docs/AI.md`），
**还没接进客户端** —— 那属于另一条线。这里只保证"AI 的回合真的会有人开火"。

自检断言：LAN 1v1 里人打偏 ⇒ 结算后射手是 Bot、`bot_think_left > 0`、
`bot_take_turn()` 返回 true、AI 弹道的起点在**它自己**的士兵处（x ≈ 18）、
AI 那发也照常结算、回合数 +1。

### 6.4 仍是未知

| # | 未知 | 缺什么 |
|---|---|---|
| 1 | **初始游标**（谁先开火） | `docs/109` §6-A：三例吻合 0，但 `1782763022` 首取 `2509` 不符。本实现取"0 号玩家的 0 号士兵"（与既有行为一致） |
| 2 | **玩家之间**的轮转顺序 | `docs/47` 判为 `HashMap` 迭代序 ⇒ **不可复现**；本实现固定用 `state.players` 的下标序 |
| 3 | 同时回合制的**发射时序** | `docs/98` §6-D。本实现只做单次判决 |
| 4 | AI 的函数质量 | 原版进化器未接（`aigen` 只到模块层，`docs/109` §6.1） |
| 5 | 超时的三件事（是否推进游标/是否已广播/算不算一次） | `docs/109` §2.7，只有用户口述 |

---

## 7. 附 1：默认士兵数

### 7.1 根因

`PlayScreen.begin_match()` 以前无条件 `for s in int(p.soldiers)`，
而 `GwGameState.add_human()` / `add_bot()` 的默认 `soldiers = 1`（`client/src/app/game_state.gd:64,79`）。
⇒ **战役关**里 `Level 3/5/9/11/14/18/19/20` 该有 2~5 个士兵，界面上只摆了 1 个。
用户说的"默认是 1 个士兵"就是这条（不是设置界面的默认值）。

### 7.2 证据

`docs/118` §7.1 的「我方士兵」列：**逐位**（立即数 `edx=N` → `0x476320(mgr, player_id, N)`）
+ **运行时**（385 份 replay，21/21 对齐）。`GwCampaign.LEVELS` 里已有这一列。

### 7.3 修法

战役关：`want = int(level.get("soldiers", p.soldiers))`（**关卡数据赢**）；
局域网/联机：仍用玩家表自己的数（那一份由设置界面/服务端给）。
`game_state.gd` 的默认值**没动**（不在本任务可改范围；而且"设置界面默认 1 个"没有反证）。

自检断言：`Level 3` 开一局 ⇒ `level_state().soldiers == 2`（**不**注入玩家表），
第一个射手在 y = −5.1（0 号槽）。

---

## 8. 附 2：成就/队伍图标过小（**只定位，没改**）

### 8.1 定位

`docs/92` §4 的台账里，`GameSetup` 玩家条目的一行是：

```text
└ .../HBoxContainer/RankIcon   TextureRect   玩家 ranks/rank_0.svg；Bot hats/no_hat.svg
```

我们这边是 `client/src/screens/game_setup.gd:311-319`：

```gdscript
var rank := TextureRect.new()
rank.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
rank.name = "RankIcon"
rank.custom_minimum_size = Vector2(24, 24)     # ← 24×24，硬编码
```

⇒ **"成就图标"= 这个 `RankIcon`（段位/成就）**，24×24 是**我们自定的常数**，没有任何证据。

### 8.2 为什么没改

1. 原版这个图标的**尺寸**没有证据：`GameSetup` 的玩家条目是**运行时**由
   `graphwar_setup_control.rs` 装配的（`docs/92` §4.5 的条目也只说"有 RankIcon"，
   没有尺寸），要拿尺寸得去反编译 `setup_control` 的 `custom_minimum_size` 立即数
   （或量一次原版真帧）。
2. `client/src/screens/game_setup.gd` **不在本任务的可改文件清单**里
   （清单只到 `play_screen.gd` / `client/src/game/*` / `src/`）。
3. "队伍图标"若指 `ChangeTeamButton`（`arrow_left/right.svg`）或 `AddSoldierButton`，
   它们在同一个文件里，同样越界。

⇒ 交回父 agent：**(a)** 是否把 `game_setup.gd` 纳入范围；**(b)** 尺寸仍需一次真帧或
`setup_control` 的反编译（本轮没做）。

---

## 9. 明确**没做**的（别读成已复原）

| 项 | 状态 | 缺什么 |
|---|---|---|
| **同时回合制**的逐 tick 时序 | 未做：整批仍一次落地（`_commit_shot()` 的 `SIMULTANEOUS` 分支） | 一份同时回合的真报文时序 + 双方的 `FunctionActive` 交错 |
| 特效的**像素形态** | 自定 | `entity_drawer::draw`（`0x4b2890`）的绘制细节 |
| **音效身份**（哪个效果配哪个音） | 自定 | `0x4bfe10` 的 match 臂（45 KB）或运行时 hook `0x8ae4c0`=`play()` |
| `app.gd` 的 `Fire!`→explosion 重复 | 未改（越界） | 父 agent 删一行 |
| 曲线上限/多玩家同时显示 | 未做 | 原版是否同时画多条函数，未观测 |
| 失败/胜利音的时机 | 仍是 `app.gd` 在 `report_match_result()` 里响 | 原版 `game_state_info_event` 驱动的具体音未知（`docs/128` §3.1） |

---

## 10. 本文的**未复核**项（诚实标注）

* `0x463da0` 那几条 `turn_manager` 指令**本轮没有重跑 objdump**，原文照录
  `docs/109` §2.3（那一节标注为「本轮新读（objdump）」）。若要逐位复核：
  `objdump -d --start-address=0x463f27 --stop-address=0x464030 libgraphwarrust.so`。
* `0x4bf090` 的**跳表**是本轮新读的（§3.2），可复现：
  `python3 -c` 读 `.rodata` 的 `0x1c27bc` 起 4 个 `int32`（相对表基址），
  原文指令见 §3.2。**没有**复核 `event+0x20` 确实是 `effect_type` 字段
  （`docs/128` §3.2 已标**推断**，本文沿用）。
* **`effect_type` 判别值 ↔ 效果名**的对应（0=Explosion、1=Death、2=Spark、3=Ash）是
  **推断**：依据是 `.rodata` 里 4 个名字首尾相接且顺序如此（`docs/128` §3.2 的**观测**），
  以及 serde 枚举按声明序编号这一通用事实。**没有**独立证据钉死这张对应表。
  ⇒ 我们按这张表接的槽位（§3.3）因此也是"**表已解、名字映射推断**"。

---

## 11. 复现

```bash
cd .agent-work/playfix/repo
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.agent-work/playfix/target
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
export GODOT=/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64

cargo test --offline                       # 333 passed
cargo build --offline --example serve_match # 联机两组断言要用（selftest 读 $CARGO_TARGET_DIR）
cargo build --offline && cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/
python3 tools/extract_assets.py --pck /home/user/MyProject/gw2/Graphwar.pck
"$GODOT" --headless --path client --import  # 新 class_name（GwEffectLayer/GwTurnDriver）必须重新扫描
bash client/run.sh selftest                # 635 passed, 0 failed
```

只跑本文这一组：

```bash
bash client/run.sh selftest | grep -E "轮转|①|②|③|④|⑤|⑥|士兵数|特效层"
```

### 11.1 回归反证（本轮**实际跑过**，不是建议）

自写断言只能证明自洽（`AGENTS.md` §0 第 5 条），所以两条改动各自做了一次反证：

| 反证 | 操作 | 结果 |
|---|---|---|
| ② 曲线清除 | 删掉 `_on_shot_arrived()` 里的 `_plane.clear_path()` | `SELFTEST 634 passed, 1 failed` ——**唯一**红的就是「② 飞完 ⇒ 曲线被清掉（path_points = 4587）」 |
| ⑤ 结算后置 | 把 `_commit_shot()` 里的 `if _fast_forward():` 改成 `if true:`（即**退回"开火当刻就结算"**） | `SELFTEST 628 passed, 7 failed`：<br>`① 还没结算 ⇒ 回合数仍是 0`（实际 1）<br>`① 开火那一刻没有胜负判定`（实际 `reason: playing`）<br>`① 弹道只飞了一小段 ⇒ 仍在飞、仍未结算`（turns_used = 1）<br>`⑤ 开火那一刻靶子还没被打掉`（alive = 0）<br>`⑤ 开火那一刻局还没结束`<br>`⑤ 开火那一刻没有胜负判定`（实际 `targets_cleared`）<br>`⑤ 开火那一刻不弹结算框` |

原状（改回来之后）：`SELFTEST 635 passed, 0 failed`。

### 11.2 这些断言能证明什么、不能证明什么

* **能**证明：改动的**先后顺序**真的变了（反证一红就是证据），且既有行为没被碰坏。
* **不能**证明：原版就是这么做的。原版行为的证据永远是 §1–§7 里那些
  地址/报文/夹具坐标/用户实测；**selftest 条数只是回归保护，不是复原证据**
  （`AGENTS.md` §0 第 2 条）。
* 特别是 §5 的"结算在弹道飞完之后"：**顺序**有 `docs/41`/`docs/43` 的观测支撑，
  但"飞多久"是我们的 tick 近似（§5.5），**不是**复原值。
