# 135 · 三条开火表现：键盘直输 / 准备光圈 / 弹道按 tick 推进

状态：**现行**。本文消掉 [`126-level-obstacles.md`](126-level-obstacles.md) §6.3 登记的三条
"用户口述、未实现"（父 agent 明确要求另派一轮做）：

| # | 行为 | 本轮结果 |
|---|---|---|
| 1 | 支持**键盘直接输入表达式**（不只是点屏上键盘） | ✅ 做了。原版机制**有观测**（是"输入框聚焦时接管键盘"），我们的接线方式**自定** |
| 2 | **准备发射的士兵有光圈，准备完毕光圈闭合** | ✅ 做了。**存在**是用户口述；**长什么样全部自定**（0 证据） |
| 3 | 函数**不是瞬间抵达对面**，弹道随 tick 推进 | ✅ 做了。tick 频率**观测**、每 tick 弧长**推断**、与结算的先后**是本轮的取舍** |

证据分级沿用 `AGENTS.md` 的四档：**逐位验证** / **观测** / **推断** / **未知**；
凡是"我们定、原版没证据"的量，一律额外标 **自定**（并写清要什么证据才能升级）。

---

## 0. 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 原版键盘是怎么接的 | `GraphwarFunctionInput` **本身就是 `LineEdit` 的 Rust 子类**，并在自己的 `gui_input` 里处理按键 ⇒ **输入框聚焦时接管键盘**，不是全局按键 | **观测**（§1.1：恢复出的 `.tscn` 属性 + `.so` 字符串） |
| 我们的接线 | `PlayScreen._unhandled_key_input()` **屏幕级接管**（不用先点输入框） | **自定**（原版"会不会自动 `grab_focus`"**未知**；§1.3 写了升级路径） |
| 键位表 | 数字/`.`/括号/五则运算符/字母（大写折小写）、`π`、`√`→`sqrt(`、退格、Delete、←/→/Home/End、回车提交 | 字符集合**推断**（文法认得哪些字符是**逐位验证**的）；映射本身**自定** |
| 光圈的存在 | 有 | **用户口述**（`docs/126` §6.3 第 2 条） |
| 光圈谁画 | `plane_view.gd`（棋盘上的一切都由它画，分工不变） | — |
| 光圈的状态源 | 谁该开火 = `TurnInfo{player_id, soldier_id}` 那一个；准备好 = **函数非空** | 谁开火**观测**（`docs/46`/`docs/98`）；"准备好"的口径**推断** |
| 光圈的半径/颜色/线宽/**闭合时长**/插值 | 1.15 单位 / 玩家色 / 2 画布像素 / 0.18 s / 线性 | **自定**（§2.3 写了要什么证据） |
| tick 频率 | **60 ticks/秒** | **观测**（`docs/35` §2，113 个样本） |
| 每 tick 沿弹道前进 | **0.29 棋盘单位**（恒定速度） | **推断**（`docs/86` §6/§7：起飞 tick 与爆炸 tick 之差） |
| 弹道"逐 tick 揭示" | 沿 `shot.points` 前缀推进，撞点（`drawn.impact`）是最后一点 | **推断**（协议按 tick 建模：`FunctionActive{active_tick}` / `EffectInfo{start_tick}` / `QueueDestroyObstacle{tick}`） |
| "一次画完 = 回合结束"有没有写死 | **没有**。绘制推进与回合结算互不相干，`GwFireFx.shot_finished` 是留的口 | — |
| 结算时序 | **没改**：仍在 `fire()` 里同步做完（为了不动既有 245 条断言的时序） | **本轮的取舍**（§3.4 给了改法） |
| `bash client/run.sh selftest` | **245 → 280 passed, 0 failed**（+35 条断言，未改/未删任何既有断言） | — |
| `cargo test --offline` | **296**（与起点一致，`src/` 一行没动） | — |
| `bash client/run.sh dump` | `@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…` | **逐位验证**（内核仍在） |
| 截图 | 光圈开环/闭合、弹道前缀随时间变长（§5.3 有产物与量测） | **观测**（我们自己的截图） |

一句话：**三条都做出来了，且把"哪部分是原版事实、哪部分是我们拍的板"逐条摊开写清了。**

---

## 1. ① 键盘直接输入表达式

### 1.1 原版是"输入框聚焦时接管键盘"（**观测**）

三条独立证据，都指到具体字节/文件：

1. **`GraphwarFunctionInput` 是 `LineEdit` 的子类。** 恢复出的 `play_screen.tscn`
   （GDRE 从 `.godot/exported/.../play_screen.scn` 反解，见 `docs/104` §4）在它身上只写了
   **`LineEdit` 才有的属性**（`play_screen.tscn:764-772`）：

   ```text
   [node name="GraphwarFunctionInput" type="GraphwarFunctionInput" parent=".../HBoxContainer2"]
   size_flags_horizontal = 3
   theme = ExtResource("1")
   alignment = 1
   context_menu_enabled = false
   virtual_keyboard_enabled = false
   caret_blink = true
   caret_force_displayed = true
   ```

   `docs/125` §3.3 的另一条旁证也对得上：它的样式就是主题里 **`LineEdit` 那条 `StyleBoxFlat`**
   （透明底 + 3 px 黑边 + 圆角 10）。

2. **它实现了 `gui_input`。** `.so`（`libgraphwarrust.so`）的字符串池里
   `gui_input` 与 `size_changed`、`text_submitted`、panic 位置 `src/graphwar_function_input.rs:519`
   在同一段连续字符串里：

   ```text
   … gui_input size_changed text_submitted src/graphwar_function_input.rs:519 …
   ```

   Godot 的 `gui_input` **只有控件拿到焦点才会收到按键** ⇒ 键盘是**控件级**的，不是全局的。

3. **它用的是一批 `LineEdit` 专有方法。** 同一份字符串池里还有
   `set_caret_column` / `get_caret_column` / `delete_char_at_caret` /
   `insert_text_at_caret` / `clear` / `grab_focus`（都是 `LineEdit` 的 API 名）。

> 注意别和第 `docs/92` §4.10 的措辞打架：那里写的"Rust 自绘，**无 `LineEdit` 子节点**"是对的
> ——它**本身就是**一个 `LineEdit`，不是"里面装着一个"。`docs/06` §15.3 用
> `find_children("*", "LineEdit")` 找子节点返回 0，也和这条一致。

### 1.2 我们怎么接（**自定**：屏幕级接管）

映射表在新文件 `client/src/play/keyboard_input.gd`（`GwKeyInput`），触发点是
`PlayScreen._unhandled_key_input()`。**为什么不挂在 `GwFunctionInput._gui_input`：**
原版"进对局后会不会自动抢焦点"**没有观测**——`.so` 里虽然有 `grab_focus` 这个字符串，
但看不出它调在哪个节点上；而"直接敲键盘就能输入"是用户实测到的行为，两种情形都可能。
屏幕级接管把"聚焦后能打字"包含在内（`LineEdit` 拿到焦点时本来也是这个效果），
所以它对两种情形都成立。代价写在 §1.3。

键位（`GwKeyInput.command_for()`）：

| 键 | 效果 | 为什么这么定 |
|---|---|---|
| `0`–`9`、`.`、`(`、`)`、`+`、`-`、`*`、`/`、`^` | 插入该字符 | 文法（`src/math_function::lex`）认得这些记号 —— **逐位验证** |
| 字母 `a`–`z` / `A`–`Z` | 插入**小写**字符 | 词法器先 `to_lowercase()` 再切词 —— **逐位验证** |
| `π`（输入法上屏） | 插入 `π` | `lex` 把 `pi`/`π` 都换成 `n`（= π）—— **逐位验证** |
| `√` | 插入 `sqrt(` | 屏上键 `√` 的 `input_text` 就是 `sqrt(`（`docs/92` §4.10）；裸 `√` 会被词法器**跳过** |
| `Backspace` / `Delete` | `⌫` / `⌦` | 与屏上键同一条命令（`GwFunctionInput.key_input`） |
| `←` / `→` / `Home` / `End` | `←` / `→` / `⇤` / `⇥` | 同上 |
| `Enter` / 小键盘 `Enter` | **提交** ⇒ `text_submitted` ⇒ `fire()` | `text_submitted` 是 `LineEdit` 的固有信号（`docs/104` §6.3 的 #3 就是它连到 Rust） |
| 空格、`%`、`,`、`=`、`'`、`Tab`、`Esc` | 不插入 | 文法里没有（词法器会跳过），塞进去只会让输入框变脏 |
| `Ctrl`/`Alt`/`Meta` 组合 | 不接管 | 自定：留给系统快捷键 |

两条**故意没做**的：

* 屏上键 `C`（清空）**没有**映射到物理 `c` —— `c` 是 `cos(` 的首字母，抢了就敲不出 `cos(`；
  原版 `C` 键的真实语义本身也只是**推断**（`docs/108` §7），物理键盘上怎么清空**未观测**。
* `y'`（死特性：`AGENTS.md` 硬事实"第三个变量在所有 7 个调用点都传 0"）没有映射。

### 1.3 自定项，以及"要什么证据才能升级成观测"

| 自定项 | 现状 | 升级需要什么 |
|---|---|---|
| 触发点是**屏幕级**而不是控件级 | 见 §1.2 | 原版真机上的**按键事件日志**：进 PlayScreen 后**不点任何东西**直接打字，看 ① 有没有字符进函数串；② 当时焦点在哪个节点（hook `gui_input` 或 dump `viewport.gui_get_focus_owner()`） |
| 字符过滤规则 | 放行"文法认得的单字符"（`GwKeyInput.PRINTABLE`） | 同上日志里看原版对 `z`、空格、`,` 的反应（原版可能照收，靠格式化器兜底——`.so` 里有 `Formatter should not fail!` 与那条 `[0-9]*\.?…|e` 正则，说明它**会**重排文本） |
| 回车 = 开火 | 沿用仓库既有判断（`text_submitted → fire()`） | 抓一次原版对局：按回车后有没有 `FunctionFire`/`FunctionFireRequest` |
| Ctrl/Alt 不接管 | — | 同上 |

另外两条**已知的行为边界**（不算缺陷，但没有原版依据）：

* 弹窗（例如 `ConfirmPlayExitPopUp`）开着时，PlayScreen 仍然可见 ⇒ 打字照样进函数串。
  聊天框不受影响：`ChatLineEdit` 是**真的** `LineEdit`，拿到焦点时我们直接放行（`_unhandled_key_input`
  里查 `gui_get_focus_owner()`）。
* 输入框自己**不**抢焦点（`GwFunctionInput.focus_mode` 仍是 `NONE`），所以点它只是移光标。

---

## 2. ② 准备发射的光圈 / 准备完毕光圈闭合

### 2.1 分工

* **画**：`client/src/play/plane_view.gd::_draw_ready_rings()`（棋盘上的一切都由它画，
  `docs/126` §4.2 的分工不变）。数据在 `GwPlaneView.ready_rings`。
* **状态 + 动画**：`client/src/play/fire_fx.gd::GwFireFx._sync_rings()`。
* **状态源**：`PlayScreen.fire_fx_state()`（只读快照，每帧被拉一次，不写对局状态）。

### 2.2 谁是"准备发射的士兵"（**推断**，依据都在别处）

| 模式 | 光圈画在谁身上 | 依据 |
|---|---|---|
| 顺序回合 | **恰好一个**：`_active_soldier()`（当前该开火的玩家 + 该玩家的第一个活着的士兵） | `TurnInfo{player_id, soldier_id}` 是"轮到谁"（`docs/46`）；射手轮转的真规则是"每玩家一张士兵表 + 游标，先自增再取、死了跳过"（`docs/98`，**观测**） |
| 同时回合 | 每个**还没提交函数**、且自己还有活着的士兵的玩家，各一个 | `docs/86` §6：一回合里各自提交（`WaitingForFunctions`），`_shot_queue` 就是"已提交" |

"准备好"= **函数非空**（`strip_edges()` 之后）。这是**推断**：原版光圈"闭合"到底对应
"函数非空"，还是"按了 Fire"，还是"服务端已收到 `FunctionFire`"，**没有任何证据**。
`docs/86` 只说明同时回合制里"准备完毕"有语义（各自提交），没有说它怎么显示。

### 2.3 长什么样（**全部自定**，0 证据）

先说明为什么只能自定：

* `.so` 的特效枚举里只有 **`DeathEffect` / `SparkEffect` / `AshEffect`**
  （字符串池：`EffectDeathEffectSparkEffectAshEffect…`，`EffectInfoEvent{effect_id, start_tick}`），
  **没有**"光圈"这种 `EffectInfo` ⇒ 它不是协议事件，只能是客户端本地画的；
* `docs/92` §4.10 的界面台账里没有它；`docs/121`/`docs/125` 的真帧取样也没有它
  （那些帧是"开局刚进"和"开火后"，都不是"准备发射"那一刻）；
* 用户口述是**唯一**来源。

所以我们定了一套（写在 `GwFireFx` 的常数里，改起来是一行）：

| 量 | 值 | 备注 |
|---|---|---|
| 半径 | **1.15 棋盘单位** | = 士兵半径（0.5，`docs/120`）的 2.3 倍；正好落在士兵贴图方块（2.15 单位，见 `plane_view.gd::SPRITE_CONTENT`）之外，不会被精灵的透明边压住 |
| 线宽 | **2 画布像素** | 与粗网格同宽（`_canvas_px()*2`） |
| 颜色 | 该士兵的**玩家色** | `docs/102` §1.1 的 6 色调色板，`plane_view.gd` 已在用 |
| 未准备时的形状 | 露出 **30%** 的弧（从 12 点方向顺时针），alpha 0.55 | "看得见，但明显没合上" |
| 准备完毕 | 覆盖 **100%**（整圈），alpha 1.0 | |
| 闭合/张开动画 | **0.18 s 线性** | `RING_CLOSE_SECONDS` |
| 画在谁之上 | 弹道之下、士兵精灵之下 | 身体压住环的内侧，像"站在光圈里" |

**要升级成观测需要什么**：原版**真帧逐帧**——从"刚轮到某个士兵"到"函数敲完"再到"按 Fire"，
每帧一张 1920×1160 截图（`docs/125` §1 的 `probe.gd` 那套 harness 就能做，只要在
`play_screen` 停住并定时截图）。有了它就能量出：半径（相对士兵身体）、线宽、颜色来源
（玩家色还是固定色）、起始弧度、闭合方向（顺时针/逆时针）、闭合时长与插值曲线
（线性/`ease`/分段），以及"闭合"到底卡在哪个事件上。

---

## 3. ③ 弹道按 tick 逐点推进

### 3.1 时钟与速度：一个**观测**、一个**推断**

| 量 | 值 | 档位 | 依据 |
|---|---|---|---|
| tick 频率 | **60 ticks/秒** | **观测** | `docs/35` §2：113 个 `TickReply` 样本拟合出 `current_tick = base + round((now_ms−base_ms)·60/1000)`，110/113 完全吻合；多点线性拟合 59.90 / 59.98 / 60.00 / 60.00 |
| 每 tick 前进多远 | **0.29 棋盘单位**（沿弧长，恒定速度） | **推断** | `docs/86` §6：`FunctionActive.active_tick`（起飞）与 `EffectInfo(ExplosionEffect).start_tick`（到达）之差 ÷ 我们算出的弧长；82 发里 75 发一对一配上，速度**中位 0.29**、p10~p90 = 0.255~0.300。`docs/86` §7 明写这条是**推断**：缺"逐 tick 位置"的直接观测 |

`docs/35` §4 当年留的那个洞——**"tick 与弹丸飞行时间的关系"**——现在有了一条
（`docs/86` 的 0.29 单位/tick），但仍然是**推断**。协议层是按 tick 建模的这一点没有疑问
（`FunctionActive{active_tick}` / `EffectInfo{start_tick}` / `QueueDestroyObstacle{tick}`，
`docs/46`/`docs/43`），所以"逐 tick 推进"这个**结构**是站得住的，只有**速率**是推的。

**采样步长 ≠ 每 tick 的距离**（这条容易混，单列一段）：

| 量 | 值 | 是什么 | 档位 |
|---|---|---|---|
| 采样步长（弦长） | `0.01` | **空间离散化**：一条曲线被折成多少个点（`0.01` 是 `src/graphwar_path_calculator.rs` 的 `step`，`AGENTS.md` 硬事实 + `cargo test` 逐位验证） | **逐位验证** |
| tick 频率 | `60 /s` | **时间**：门钟走多快 | **观测**（`docs/35`） |
| 每 tick 弧长 | `0.29 单位` | **播放速率**：弹丸一 tick 飞多远 | **推断**（`docs/86`） |

三者只在换算里相遇：**每 tick 约覆盖 `0.29 / 0.01 = 29` 个采样点**。
`docs/35` §4 当年明确记着"tick 与弹丸飞行时间的关系"没定；`docs/86` 补上了 0.29，
但它**不是**从采样步长推出来的（那是空间量），而是拿"起飞 tick / 爆炸 tick"的差反推的时间量。

### 3.2 实现（`GwFireFx`）

```text
play(points)            记下全量点序列 + 每个点的累计弧长 _cum[]（float64）
advance_seconds(delta)  _arc += delta × 60 × 0.29（钳到总弧长）
                        reveal = 前缀里累计弧长 ≤ _arc 的点的个数（单调不减）
                        plane.set_path_reveal(reveal)   ← 只是"画到第几个点"
                        _arc ≥ 总弧长 ⇒ 收尾：reveal = 全量、停表、发 shot_finished
```

* 每帧调一次（`Node._process`）；自检用 `advance_seconds(固定步长)` 走同一条路径，可复现。
* **`shot.points` 不动**：`plane.path_points` 永远是全量，`path_reveal` 只决定画多少
  ——`docs/126` §3.1 留的口原样保留。
* 撞点标记（`drawn.impact` 的小圆点）**只在整条揭示完时**才画：弹丸没到就先画个撞点会骗人。
* 弧长用 **float64** 存：float32 在弧长 50 附近量化误差约 4e-6，比判"末点算不算到了"的
  1e-6 容差还大，实测会掉一个点（4586/4587）——所以收尾时直接给全量，不靠浮点容差。

典型时长：`f(x)=0` 这一发共 4587 点 = 45.86 单位弧长 ⇒ **158 tick ≈ 2.6 s**。

### 3.3 验证（数值 + 像素）

* 自检（§6）：开火瞬间 `reveal` = 1 < 全量；三次 `advance_seconds(0.25)` 后严格单调增；
  0.25 s 的揭示点数与"15 tick × 0.29 ÷ 0.01 弦长 ≈ 435"相差 ≤ 40；
  `plane.path_reveal` 与状态一致；走完后 `reveal` = 全量且 `arc` = `total_arc`。
* 像素（我们自己的截图，640×480 窗口）：同一发 `sin(x)`——
  * 推到 1.1 s：弹道跨度 `x ∈ [135, 316]`，451 个弹道色像素；
  * 推到 3.0 s：跨度 `x ∈ [135, 495]`（到棋盘右边），761 个像素。
  ⇒ 画出来的前缀确实随时间**变长**，不是一次画完。

### 3.4 与"回合结束"的关系（**本轮的取舍，明写**）

原版里死亡发生在弹丸**到达**的 tick（`docs/86` §6/§7，**观测（强）**：151/152 个死亡时刻 =
最早到达时刻，残差中位 −0.4 tick）。我们这一版**没有**把结算拖到到达时刻：

* `PlayScreen.fire()` 仍然在算完弹道后**立即** `_commit_shot(shot)`（判定 + 换手 + 回合数 +1），
  动画只是**视觉层**；
* 原因：既有的 245 条断言里有 10 条是"开火后 `await _frames(1)` 立刻读 `turns_used`/靶子存活/判定"，
  把结算改成"飞到才结算"会让它们全部变成时序测试（要按 tick 等待），而本轮被要求
  **只许新增断言**；
* 所以本轮只保证一点：**"画到哪"与"回合结束"在代码里是两件事**——
  `_plane.set_path()` / `_fire_fx.play()` / `_commit_shot()` 是三行各自独立的调用，
  FX 还留了 `shot_finished(ticks)` 信号。

**要接成"到点才结算"只需两处改动**（写在这里是为了下一次不用重新推导）：

```gdscript
# play_screen.fire()：这一行不再直接调用，改挂到信号上
_fire_fx.shot_finished.connect(func(_t): _commit_shot(shot), CONNECT_ONE_SHOT)
```

并把自检里那 10 条"打完立刻读状态"的断言改成"把动画推完再读"
（`play.find_child("FireFx").advance_seconds(600.0)`，本文 §3.3 已在用这个手法）。
**副作用**：同时回合制下要等多发都飞完才结算，`_shot_queue` 的语义也要跟着调整。

---

## 4. 对 `client/src/screens/play_screen.gd` 的逐行改动清单

该文件被**回放播放器** agent 独占，所以这里**只插入、不改既有行**（`git diff --numstat` =
`85 / 0`）。按改动后的行号：

| # | 位置（新行号） | 行数 | 内容 | 作用 |
|---|---|---|---|---|
| 1 | **60–61**（`var _input` 之后） | +2 | `## 开火表现…` + `var _fire_fx: GwFireFx` | 成员声明 |
| 2 | **195–201**（`_plane_aspect.add_child(_plane)` 之后） | +7 | 空行 + 2 行注释 + `_fire_fx = GwFireFx.new()` / `.plane = _plane` / `.state_provider = fire_fx_state` / `_plane.add_child(_fire_fx)` | 挂载（挂在棋盘下，自己 `_process`） |
| 3 | **609–680**（`key_input()` 与 `_on_function_text_changed()` 之间） | +71 | `_unhandled_key_input()`（610–639）+ `fire_fx_state()`（641–680），各带注释 | 键盘接管 + 光圈状态源 |
| 4 | **718–722**（`_plane.set_path(...)` 之后） | +5 | 3 行注释 + `if _fire_fx != null:` / `_fire_fx.play(drawn.points)` | 起 tick 动画 |

合计 **+85 / −0**。合到主线时若与回放 agent 冲突，只有这四个落点。

`client/src/play/plane_view.gd`（回放 agent 不动它）的改动：
`path_reveal` / `ready_rings` 两个字段、`set_path_reveal()` / `set_ready_rings()` 两个方法、
`set_path()`/`clear_path()` 各加一行复位、`_draw()` 加一行 `_draw_ready_rings()`、
`_draw_path()` 按 `path_reveal` 截前缀（撞点标记改成"揭示完才画"）、
新增 `_draw_ready_rings()` + 一个线宽常数。**没有改动任何既有绘制语义**
（`path_reveal = -1` 时与旧行为逐像素一致）。

新建文件：

* `client/src/play/keyboard_input.gd`（`GwKeyInput`，映射表 + 证据注释）
* `client/src/play/fire_fx.gd`（`GwFireFx`，tick 推进 + 光圈动画）

另外改了一个**工具**（不是游戏逻辑）：`client/tools/screenshots.gd` +5 行 ——
`PlayScreenOffset` 那张基准图是"打一发之后定格"，不推完就只能截到动画第 1 帧（几乎没有弹道），
所以截图前把这一发推到飞完。

---

## 5. 验证记录

```bash
# 环境：干净的 user://（本机 .agent-work 里有别的 agent 留下的回放文件，
#      不清掉会让 docs/121 的"回放列表空"那条断言失败 —— 与本轮改动无关）
export XDG_DATA_HOME=…/.agent-work/firefx/xdg/data
export XDG_CACHE_HOME=…/.agent-work/firefx/xdg/cache
export XDG_CONFIG_HOME=…/.agent-work/firefx/xdg/config

bash client/run.sh selftest     # SELFTEST 280 passed, 0 failed   （起点 245）
bash client/run.sh dump | grep @@KERNEL
                                # available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…
cargo test --offline            # 296（248+4+6+6+4+12+6+3+7），与起点一致；src/ 未改
godot --headless --path client --import    # 注册 GwFireFx / GwKeyInput 成功，无脚本错误
bash client/run.sh shots <dir>  # 17 张；PlayScreen/PlayScreenCampaign/PlayScreenOffset 见 §5.3
```

### 5.1 自检条数

`245 → 280`（**+35**，一条没删、没改既有的）。分布：

| 组 | 断言数 | 内容 |
|---|---|---|
| ① 键盘映射（纯函数） | 9 | 字母、Shift+`9`→`(`、`π`、退格、End、回车、空格、抬起、Ctrl |
| ① 键盘端到端 | 3 | 连敲 `s i n ( x )`→`sin(x)`；`End`+退格→`sin(x`；回车开火后棋盘上真有弹道 |
| ② 光圈 | 10 | 只有一个光圈、画在当前射手身上、空函数=开环、开环也看得见、非空=闭合、闭合覆盖整圈、半程中途、对局结束全灭 |
| ③ tick 推进 | 11 | 60/0.29 两个常数、全量点序列、t=0 只到起点、严格单调增、0.25 s ≈ 435 点、`path_reveal` 一致、零步长不倒退、飞完=全量、`arc`=总弧长、跑完后 `shot.points` 仍是全量 |
| ① 回车提交 | 2 | `turns_used` +1 |

### 5.2 三条各自至少一条断言（对应任务要求）

| 任务要求 | 断言 |
|---|---|
| 键盘输入产生正确表达式 | `键盘：连敲 s i n ( x ) ⇒ 'sin(x)'` |
| 光圈可见性与状态一致 | `光圈：函数为空 ⇒ **开环**` / `光圈：函数非空 ⇒ **闭合**` / `光圈：对局结束 ⇒ 光圈全部消失` |
| tick 推进时点数随时间单调增 | `tick：揭示点数随时间**严格单调增**（[1, …, …, …]）` |

### 5.3 像素证据（我们自己的截图，640×480）

| 文件 | 状态 | 观测 |
|---|---|---|
| `PlayScreen.png` | LAN 1v1，已敲 `sin(x)` | 当前射手脚下**一整圈**闭合光圈（玩家色）；对手（Bot）没有 |
| `PlayScreenCampaign.png` | 战役第 0 关，函数空 | 当前射手脚下只有**约 1/3 圈**弧（开环） |
| `PlayScreenOffset.png` | 打完一发（推到飞完） | 弹道完整 + 撞点；光圈闭合 |

产物在 `.agent-work/firefx/shots*/`、放大裁片在 `.agent-work/firefx/crop/`
（不入库：都是可再生的工作产物）。

---

## 6. 四档标注汇总

| 结论 | 档位 | 缺什么 / 依据 |
|---|---|---|
| 原版键盘 = 输入框聚焦后接管（`LineEdit` 子类 + `gui_input`） | **观测** | 恢复出的 `play_screen.tscn:764-772` 属性 + `.so` 字符串池（`gui_input`、`set_caret_column`、`insert_text_at_caret`、`src/graphwar_function_input.rs:519`） |
| 原版会不会自动 `grab_focus` | **未知** | 真机按键日志/焦点 dump |
| 我们的屏幕级接管、过滤规则、回车=开火 | **自定** | 同上 |
| 文法认得的字符集（`x y y' + - * / ^ sqrt log ln exp abs sin cos tan`、`pi/π/n`=π） | **逐位验证** | `src/math_function::lex` + 上游 `cargo test` |
| 光圈**存在** | **用户口述** | 原版真帧（准备发射那一刻） |
| 光圈半径/颜色/线宽/起始弧度/**闭合时长**/插值 | **自定** | 同上（逐帧真帧能量出这些量） |
| "准备好" = 函数非空 | **推断** | 真帧 + 报文时间线（闭合发生在 `FunctionFire` 之前还是之后） |
| tick 频率 60/s | **观测** | `docs/35` §2（113 个 `TickReply`） |
| 每 tick 0.29 单位 | **推断** | `docs/86` §6/§7；缺逐 tick 位置观测（要 hook） |
| 协议按 tick 建模（`active_tick`/`start_tick`/`tick`） | **逐位验证** | `docs/46`/`docs/43` 的报文序列 + `src/_scaffold/protocol.rs` 的逐字节测试 |
| 死亡发生在到达 tick | **观测（强）** | `docs/86` §6（151/152，残差中位 −0.4 tick） |
| 我们这一版"结算仍在开火时" | **自定（取舍）** | §3.4 给了改法与代价 |

---

## 7. 仍未做 / 仍未定（照实写）

1. **结算时序没改成"飞到才结算"**（§3.4）。视觉上会看到"子弹还在飞，回合已经换手、
   下一个人的光圈开始闭"。接口（`shot_finished`）与改法都写好了，等能改那 10 条时序断言的
   时候一起做。
2. **同时回合制下 `_shot_queue` 的动画**：现在每发 `fire()` 都会 `_fire_fx.play()`，
   也就是说"后一发会覆盖前一发的揭示状态"——同时回合制里前一颗子弹的动画会**被截断**。
   正确做法是每发一个 `GwFireFx` 实例（`docs/86` §6 的"多发并行"），本轮没做。
3. **原版光圈的全部观测量**（§2.3 那张表）。
4. **`y'` 键**没有映射（死特性）；屏上键 `C`（清空）的语义本身仍是推断。
5. **输入长度上限**：`GwFunctionInput.MAX_LENGTH = 128` 仍是自定值（原版未观测，`docs/108` §7）。
