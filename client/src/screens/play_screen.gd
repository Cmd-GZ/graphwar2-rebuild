extends GwScreen
## PlayScreen —— 对局界面。
##
## 几何与配色**不是目测**：本文的每个比例都抄自恢复出的原版脚本
## （`.agent-work/tools/recovered/*.gd`），并用"原版工程 + 原版 `.so` 在 1920×1160
## 跑出来的真帧"逐项量过（`.agent-work/playfit/shots/orig_campaign_play.png`）。
## 对照表见 `docs/125-playscreen.md` §4。
##
## 原版布局规则（全部是 `screen_size.x` 的比例，所以任何分辨率同比例）：
##   `ButtonGrid.gd`           每键 `min = (x*0.15/3, x*0.15/3)`、字号 `x/30`
##   `ExtraOptionsHBox.gd`     每键同样 `x*0.15/3`、字号 `x/30`、`min.y = 左上按钮行高`
##   `FireContainer.gd`        子节点 `min.x = x*0.07`、字号 `x/30`
##   `OffsetContainer.gd`      子节点 `min = (x*0.09, 0)`、字号 `x/60`
##   `FxContainer.gd`          子节点字号 `x/60`
##   `TimerContainer.gd`       子节点字号 `x/30`
##   `ExtraOptionPad.gd`       `min.y = 左上按钮行高`（当 `plane.h - plane.w/1.66667 >= 行高`）
##
## 结构（原版 `play_screen.tscn` 106 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ PlayMainHBox
##     │    ├ LeftControls/{LeftTopButtonsContainer, LeftButtonGrid, LeftReplaysButtonsGrid}
##     │    ├ PlaneContainer
##     │    │    ├ GamePlane/{ExtraOptionPad, PlaneContainer(GraphwarPlane + 覆盖层)}
##     │    │    └ ExtraOptionsVBox/ExtraOptionsHBox{4 个开关}
##     │    └ RightControls/{FireContainer/FireButton, RightButtonGrid, RightReplaysButtonsGrid}
##     └ HBoxContainer2{TimerContainer · 弹性间隔 · FxContainer · GraphwarFunctionInput · OffsetContainer}

## 键盘（`docs/92` §4.10，顺序即原版 GridContainer 里子节点的顺序）
const LEFT_KEYS := [
	{"cmd": "C", "text": "C"}, {"cmd": "π", "text": "π"}, {"cmd": "e", "text": "e"},
	{"cmd": "1", "text": "1"}, {"cmd": "2", "text": "2"}, {"cmd": "3", "text": "3"},
	{"cmd": "4", "text": "4"}, {"cmd": "5", "text": "5"}, {"cmd": "6", "text": "6"},
	{"cmd": "7", "text": "7"}, {"cmd": "8", "text": "8"}, {"cmd": "9", "text": "9"},
	{"cmd": "(", "text": "("}, {"cmd": "0", "text": "0"}, {"cmd": ".", "text": "."},
	{"cmd": GwCommands.KEY_HOME, "icon": "arrow_left_bar"},
	{"cmd": GwCommands.KEY_LEFT, "icon": "arrow_left"},
	{"cmd": GwCommands.KEY_BACKSPACE, "icon": "delete_left"},
]

const RIGHT_KEYS := [
	{"cmd": "abs(", "text": "abs"}, {"cmd": "ln(", "text": "ln"}, {"cmd": "sqrt(", "text": "√"},
	{"cmd": "sin(", "text": "sin"}, {"cmd": "cos(", "text": "cos"}, {"cmd": "tan(", "text": "tan"},
	{"cmd": "*", "text": "*"}, {"cmd": "/", "text": "/"}, {"cmd": "exp(", "text": "exp"},
	{"cmd": "+", "text": "+"}, {"cmd": "-", "text": "-"}, {"cmd": "^", "text": "^"},
	{"cmd": "x", "text": "x"}, {"cmd": "y", "text": "y"}, {"cmd": ")", "text": ")"},
	{"cmd": GwCommands.KEY_DELETE, "icon": "delete_right"},
	{"cmd": GwCommands.KEY_RIGHT, "icon": "arrow_right"},
	{"cmd": GwCommands.KEY_END, "icon": "arrow_right_bar"},
]

## 原版 `PlaneContainer/AspectRatioContainer.ratio = 1.6667`（= 50:30）。
const BOARD_ASPECT := 50.0 / 30.0

# ---------------------------------------------------------------- 真节点（6 个 Rust 扩展类，docs/16 §48）
#
# 战场**不再由 GDScript 自绘**：`client/src/play/plane_view.gd`（`GwPlaneView`）与
# `client/src/play/fire_fx.gd`（`GwFireFx`）已删除，六个节点位置换成了 `.so` 里的
# GDExtension 类（`ClassDB.instantiate`，与 `client/src/core/core_bridge.gd` 同一条路径）：
#
# ```text
# PlaneCanvas (Control, 1000×600 画布)
#   └ GraphwarPlane            ← 棋盘框（`_draw`：draw_set_transform + draw_rect，§43）
#       ├ FunctionDrawer ×3     ← 曲线（每 player 一条 Line2D；平面 `_draw` 里的喂点循环）
#       ├ AxisDrawer            ← 白底 + 网格（`axis_primitives`，§35）
#       ├ ObstacleDrawer        ← 障碍掩膜贴图（`ObstacleDrawer::update`，§45）
#       ├ EntityDrawer          ← 士兵 / 名字（每实体 3 sprite + 1 标签，§33）
#       ├ EffectDrawer          ← 爆炸/火花（`frame_primitives`，§34）
#       └ EffectLayer           ← 只剩"按种类记账 + 派给 EffectDrawer"的适配层
# ```
#
# **画布模型是我方选择**（§43.6(d) 第 3 条：原版那次 `draw_set_transform` 的三个实参没解出来）：
# 六个类各自按"自己控件的尺寸"换算（`size.x/50`、`size/2`、`size.x/1000`），所以本屏把它们
# 统一摆在 **1000×600 的固定画布**上（`graphwar_plane::CANVAS_W/H` = 内核常数），再由
# `Control.scale` 缩放到 `AspectRatioContainer` 给的那块 5:3 矩形。这样六个类的换算落在
# **同一个坐标系**里，且与 `client/tools/plane_on_screen.tscn` 的摆法逐值相同。

## **参考画布**尺寸（`src/plane.rs::CANVAS_W/CANVAS_H`，逐位；= 50×30 棋盘 × 20 px/单位）。
##
## ⚠️ T44 起它**不再是渲染坐标系**：原版 `.tscn` 的 `AspectRatioContainer` 直接把
## `GraphwarPlane` 铺成棋盘矩形（@1920×1160 = 1279×767.4），绘制层拿**那块矩形**的 `size`。
## 这个常量现在只用于 ① 等效缩放（`plane_state()["plane_scale"]`）② 当 `src/plane.rs`
## 定标关系的对照。等价性证明见 `fixtures/board_geometry_equiv.tsv`。
const CANVAS_SIZE := Vector2(1000.0, 600.0)
## `graphwar_plane::PX_PER_UNIT`（= CANVAS_W / BOARD_W = 1000/50）。
const PX_PER_UNIT := 20.0
## `graphwar_plane::PIXEL_OFFSET`（`to_pixel` 的加法项，逐位）：`px = 20x + 500`、`py = 300 - 20y`。
const PIXEL_OFFSET := Vector2(500.0, 300.0)
## `FunctionDrawer::update_transform` 的两个实参里我们自己选的那一对（§42.5 第 4 条：原版实参未解）。
## `scale = (20, -20)` 就是 `to_pixel` 的换算；`set_width(0.125)`（逐位）在它下面 = 2.5 px 宽。
const CURVE_SCALE := Vector2(20.0, -20.0)
## 每 tick 露几个点（`graphwar_plane_class::REVEAL_POINTS_PER_TICK` = `.rodata 0x19ef90` = 30.0）。
## 这里只用来算"几 tick 揭完"；**判定**在 Rust 侧（`revealed_points()`）。
const CURVE_POINTS_PER_TICK := 30.0
## tick 频率（**观测**，`docs/35` §2：113 个 `TickReply` 样本拟合出 60 ticks/秒）。
## 原版这个数由 `GraphwarTickHandler::get_fractional_tick`（`0x519530`）给，那个模块还没接 ⇒
## 本屏按帧间隔自己推（`advance_shot_for_test()` 是给自检的确定性入口）。
const CURVE_TICKS_PER_SECOND := 60.0

## 靶子的实体 id 从 1000 起（士兵用它在 `_soldiers` 里的下标）。`EntityDrawer` 只认
## i32 id，没有"种类"概念 ⇒ 两个编号空间靠这个基数分开。
const TARGET_ID_BASE := 1000
## 阵亡士兵的"死了多少刻"（`EntityDrawer.set_entity_dead_ticks` → `death_alpha = 1 - t/60`）。
## 45 刻 = alpha 0.25。**自定值**：原版淡出曲线逐位在 `entity_drawer::death_alpha` 里，
## 但"画面上死人是多淡"取决于 `LifeQueuedDeath.tick` 与实际时刻之差，本屏没有那个 tick 源。
const ENTITY_DEAD_TICKS := 45

## 图层顺序（`CanvasItem.z_index`，子节点按 z 从小到大画）。同 z 时按加入顺序。
const Z_AXIS := -1          # AxisDrawer 的白底 + 网格在棋盘框**之下**（框是平面的 `_draw`）
const Z_OBSTACLE := 1
const Z_CURVE := 2
const Z_ENTITY := 3
const Z_FX := 4

## 玩家颜色表（`docs/102` §1.1，逐条从 `.so` 的调色板表解出）：
## Team1 暖色 / Team2 冷色，各 6 个，分配算法是"随机起点 + 线性探测"。
## 它原来住在 `GwPlaneView` 里；那个类删掉之后**唯一**的两个消费者（`assign_colors()` 与
## 回放的缺省色）都在本文件，数字一个字没动（`docs/16` §48.4）。
const TEAM_COLORS := [
	[Color8(186, 42, 32), Color8(238, 114, 0), Color8(125, 104, 0),
		Color8(122, 160, 0), Color8(149, 5, 62), Color8(234, 0, 219)],
	[Color8(107, 0, 234), Color8(17, 0, 108), Color8(21, 83, 221),
		Color8(0, 203, 163), Color8(0, 116, 26), Color8(2, 85, 63)],
]

## 玩家颜色分配（`docs/102` §1.2 的真规则：本队 6 色表里随机起点、线性探测、取第一个没人用的）。
## 确定性 RNG：同一局每次配色一致（原版是真 RNG 抽的，我们这一版没有那条 Rng）。
static func assign_colors(team_keys: Array[int], seed_value: int) -> Array[Color]:
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var used: Array[Color] = []
	var out: Array[Color] = []
	for team in team_keys:
		var pool: Array = TEAM_COLORS[clampi(team, 0, 1)]
		var n := pool.size()
		var base := int(rng.randi() % n)
		var picked: Color = pool[base]
		for k in n:
			var c: Color = pool[(base + k) % n]
			if not used.has(c):
				picked = c
				break
		used.append(picked)
		out.append(picked)
	return out

## 顶栏三个「查看函数」档位（原版三张图，`docs/121` §4.3：`view_single_function` 在
## `.tscn`/`.so` 里都没有引用点，但图标确实存在；顺序按语义排 single→recent→all）。
const VIEW_MODES := ["view_single_function", "view_recent_functions", "view_all_functions"]

## 真 `GraphwarPlane`（`src/_scaffold/graphwar_plane_class.rs`）。类型是 `Control`：
## 它是**运行时注册**的扩展类，GDScript 里写 `GraphwarPlane` 会在解析期就要求扩展已加载。
var _plane: Control = null
var _axis: Control = null            ## 真 `AxisDrawer`（网格 + 白底）
var _entities: Control = null        ## 真 `EntityDrawer`（士兵 / 名字 / 靶子）
var _fx: Control = null              ## 真 `EffectDrawer`（特效）
var _obstacle_layer: Control = null  ## 真 `ObstacleDrawer`（障碍掩膜贴图）
## 已经 spawn 过的实体 id（`EntityDrawer` 没有"列出 id"的接口 ⇒ 这边记一份）。
var _drawn_entities := {}

## 准备圈的"起始量"（`EntityDrawer.set_ring_base` 的 `base`）。原版那两张哈希表里
## `map[i]` 记的是"这具士兵开始准备时的时钟"，用来算 `H = min(π/6 + (V−base)/60, 61.5°)`。
## 我方按"第一次把某具士兵标成准备态时的 `_curve_tick`"填，**这是接线选择、不是观测**（`docs/16` §50.6）。
var _ring_base := {}
## 装不上真节点时的原因（headless 与 `--dump` 要能分清"扩展没加载"和"逻辑没走"）。
var _nodes_error := ""
## 当前那条曲线（= 原版 `GraphwarFunctionManager` 里那条 `FunctionEntry`）。
var _curve_player := -1
var _curve_total := 0
var _curve_tick := 0.0
var _curve_final_tick := 0.0
var _input: GwFunctionInput
var _timer_label: Label
var _offset_label: Label
var _fx_button: Button
var _view_functions_label: RichTextLabel
var _extra_vbox: VBoxContainer
var _extra_hbox: HBoxContainer
var _show_extra_button: Button
var _names_button: Button
var _view_func_button: Button
var _mute_button: Button
var _chat_button: Button
var _fire_button: Button
var _status_label: Label
var _replay_left: Control
var _replay_right: Control
var _plane_container: Control
var _extra_pad: MarginContainer
var _plane_aspect: AspectRatioContainer
var _left_grid: GridContainer
var _right_grid: GridContainer
var _keys: Array[Button] = []
var _view_mode := 1
var _muted := false

# --- 对局状态（界面侧） ---
var _running := false
var _soldiers: Array[Dictionary] = []          # {name, team, pos, alive, is_bot, skin, player, color}
## 本局的障碍：每项 `{pos: Vector2, size: float}`（`size` = 半径，棋盘单位）。
## 来自内核 `level_obstacles()`（`docs/126`）；**同一个数组**既给 `_set_obstacles()`
## 画，也给 `GwCore.hit_test()` 判 —— 画的和判的必须是同一批圆。
var _obstacles: Array = []
## 本局的靶子集合（`docs/131`）。**同一个集合**既给 `_sync_entities()` 画，
## 也给 `GwCore.hit_test()` 判 —— 画的和判的必须是同一批靶子。
## 之前这里只有靶子绘制（纯绘制），靶子**根本没进命中判定**，
## 于是打靶关的胜负永远由"两队士兵存活数"决定 ⇒ 第一发就通关（`docs/131` §1）。
var _targets := GwTargetSet.new()
## 联机靶子的 **`GwTargetSet` 下标 → 线上实体 id** 映射（`_build_online_targets()` 填）。
##
## 为什么要这张表：`GwTargetSet` 的 `target_id` 是**下标**（它只存位置 + 生死），
## 而线上事件与 22 个复原 manager 说的是**实体 id**（本样本靶子 = 232）。
## 只在下标与 id 一一对应时才翻译（`_view_targets()` 会先比长度，对不上就退回自造编号）。
var _target_entity_ids: Array[int] = []
## 本局的胜负规则（`docs/131` §2 / `docs/150`）：**判定在 Rust**（`GwLevelRules` =
## `src/_scaffold/play_flow_class.rs` → `graphwar_play_screen_control::LevelRules`），
## 本屏只读它的结论去排版文案（`docs/16` §52）。
var _rules: GwLevelRules = null
## 本次结算里被摧毁的靶子 id（保序）。用于"打不全就全部复原"这条推断规则。
## （**已结算的回合数**不在本屏：`_flow.turns_used()`，见 `docs/16` §53。）
var _destroyed_this_shot: Array[int] = []
## 上一次胜负判定结果（诊断/自检用；不参与逻辑）。
var _last_verdict := {}
## 自检用：注入确定的靶子/障碍布局。原版这两样都是**每局随机**的
## （`docs/118` §8 #16："本来就没有每关固定坐标"），所以这不是篡改关卡数据，
## 只是把随机量钉住，好让端到端用例可复现。空字典 = 用本关自己的值。
var _layout_override := {}
var _functions := {}                            # player_index -> String
var _active := 0
## 联机 / 回放共用的倒计时秒数（**只给排版与联机那条路用**）。
## 本地回合的时钟在 Rust：`GwTurnClock`（`0x470989` 的三个合取项，`docs/16` §53）。
var _time_left := 0.0
var _shot_queue: Array[Dictionary] = []         # 同时回合制下本轮的弹道
var _show_names := true
var _show_function_overlay := false
var _last_result := {}
## 状态行的"本轮"部分（这一行**不显示**，只给 selftest / `--dump` 当诊断，`docs/125` §5）。
var _shot_status := ""
## 状态行当前文本（`_status()` 写入；诊断用）。
var _status_text := ""

# ---------------------------------------------------------------- 一发弹道的生命周期（docs/143）
#
# 时序：开火（点序列 + 命中表）→ 逐 tick 揭示 → 走到死者那个点才落地
# （`LifeQueuedDeath{tick}`，docs/65）→ 飞完在停止点炸 → 清曲线 → 收回合。
# **流程与判据在 Rust**（`GwShotFlow` → `ShotLifecycle`，docs/16 §53）；
# 本屏只递数据、读揭示点数、按事件摆节点。报文顺序见 docs/43 §1。

## 有弹道正在飞。发射期间**不能再发**（`docs/143` §4：原版要等 `TurnEnded`）。
## 状态本体在 `_flow`（Rust）；这个字段只是它的镜像，供本屏早退判断。
var _shot_in_flight := false
## 在飞那一发的**视图数据**（画线 / 报点数 / 联机标记）；流程状态全在 `_flow`。
var _pending_shot: Dictionary = {}
## 这一发的命中表（`_judge_shots()` 的产物），给状态行与自检看。
var _judged: Dictionary = {}
## 弹道生命周期（`docs/16` §53）：`GwShotFlow` → `ShotLifecycle`。
var _flow := GwShotFlow.new()
## 本地回合时钟（超时谓词 `0x470989`–`0x4709ec`）。
var _turn_clock := GwTurnClock.new()
## **对局状态机本体**（8 个 `GameState`，原版是 `[self+0x120]` 那一个字节）。
##
## 迁移表在 Rust（`GwTurnState` → `src/turnstate.rs::next_state`），本屏只**发事件、问状态**。
## 在此之前本屏没有这台机器 —— 只有 `_shot_in_flight` 一列布尔当投影，而 `fire()` 的早退
## 判断（"上一发还在飞"）就是那条投影的读点。现在那个判断改成 `can_fire()`
## = `state == WaitingForFunctions`（`Fire` 只从这一态出发；同一个字节也是
## `handle_tick` 超时判据的第一项，`0x470989`）。
##
## ⚠️ 联机局不驱动它：那边状态由**服务端** `GameStateInfo` 说了算（`docs/134` §7.1）。
var _match_state := GwTurnState.new()
## AI 回合闸门（**我方自定**：原版 AI 时序未观测）。
var _bot := GwBotTurn.new()
## 效果层（`docs/143` §1）。由 `build()` 挂到棋盘上。
var _effects: GwEffectLayer = null
## 本轮收到/发出的效果事件流水（诊断与自检用；不影响绘制）。
var _fx_log: Array = []
## 回合轮转（`docs/143` §6）。**规则在 Rust**：`GwTurnDriver`（扩展类）→
## `graphwar_turn_manager::PlayerTurns::next_shooter`（`0x463da0`，逐位）。
## 本屏只读"下一个射手是谁"（`docs/16` §52）。
var _turn_driver := GwTurnDriver.new()
## 当前射手在 `_soldiers` 里的下标（`_active` 是**玩家**下标，两个编号都在用）。
var _active_soldier_idx := 0

## AI 开火前的等待（秒）—— **自定**。原版是 25 s 预算内的进化（`docs/93`），
## 我们这一版**还没有** AI 生成器接进客户端（`docs/AI.md` 的线），只用一条自定直线。
## 闸门本体在 Rust（`GwBotTurn` → `graphwar_play_screen_control::BotTurn`）。
const BOT_THINK_SECONDS := 0.6

## 效果 → 音效。**槽位有逐位证据**（`0x4bf090` 的 4 臂跳表：0→0x80、1→0x90、
## 2/3→0xa0，见 `docs/143` §4.1），**音效身份一条证据都没有**（`docs/128` §4.1）
## ⇒ 音是**自定**的，只有槽位是复原的。
const SLOT_FOR_EFFECT := {
	"ExplosionEffect": "0x80",
	"DeathEffect": "0x90",
	"SparkEffect": "0xa0",
	"AshEffect": "0xa0",
}
const SFX_FOR_EFFECT := {
	"ExplosionEffect": "explosion",
	"DeathEffect": "squash",
	"SparkEffect": "spark",
	"AshEffect": "pop",
}

# --- 回放模式（`docs/133`；这是本文件里唯一为回放新增的分叉） ---
## 回放模式下：不接管输入（键盘/开火无效），由 tick 时钟消费事件流。
var _replay_mode := false
## **驱动在 Rust**：`GwReplayDriver` → `src/_scaffold/replay_driver.rs`（`docs/16` §56）。
## 旧实现在 `client/src/play/replay_player.gd`（那份 `.gd` 已在 89 号**整份删除**：
## 局面机归 `src/graphwar_replay_player.rs`，模式字符串 → 档位那张表归驱动，见 §59）。
var _replay: GwReplayDriver = null
var _replay_path := ""
var _replay_bar: HBoxContainer = null
var _replay_progress: ProgressBar = null
var _replay_label: Label = null
## 回放里的"当前这一发"（诊断 / 状态行用）。
var _replay_shot_status := ""
## 回放版式是否已应用（`_set_replay_layout()`）。
var _replay_layout := false
## 左侧 ⏮▶⏭ / 右侧三键所在的容器（只在回放模式可见）。
var _replay_play_btn: Button = null
## 键盘 `RightButtonGrid` 的**天然最小宽**（@1920 = 337）。回放模式把它藏起来之后，右列宽度
## 就靠这个数钉住（真帧右三键 x1583..1919 = 337）—— **机制未知 + 我方补丁**，
## 见 `_set_replay_layout()` 里那段注。
var _right_grid_natural_w := 0.0
## `RightControls/HBoxContainer`（`Fire!` 那一**行**）与其中的 `FireContainer`。
##
## 回放屏藏的是 **`FireContainer` 这一个 `MarginContainer`**，**不是**那一行、也**不是**行里的
## 两颗按钮 —— 这条有指令级证据：原版 `0x3d9b50`（回放视图）按名字表
## `LeftButtonGrid|RightButtonGrid|FireContainer|LeftReplaysButtonsGrid|RightReplaysButtonsGrid`
## 取 5 个节点、依次 `CanvasItem::set_visible(false,false,false,true,true)`：
## `FireContainer` 那一条是 `0x3d9ce6 xor %esi,%esi` → `0x3d9ce8 call *[0x9ba978]`，
## 而 `0x9ba978 → 0x8ab200` 正是 `set_visible` 的 ptrcall 包装（class 串 `0x29ef2d`
## 'CanvasItem' 长 `0xa`、method 串 `0x29ef4c` 'set_visible' 长 `0xb`）。见 `docs/06` §103。
##
## ⇒ 那一行的最小高变成 **0**（VBox 里仍占一个 separation），右三键组的居中量因此与
## 原版一致（真帧 y=433/533/633）。**不要**给这一行 `custom_minimum_size.y`：
## T4 时代手搭树时补过 `(0, 96)`，装真 `.tscn` 之后它正好把右三键列压下 48 px（= 96/2，
## 两个 `size_flags_vertical = 3` 的间隔把余量均分）。
var _fire_row: Control = null
var _fire_container: Control = null

# --- 联机模式（`docs/139`；落实 `docs/134` §7.1 的交接清单） ---
#
# 判据是 `App.online`（它的唯一置真入口是 `GwNet.joined`），**不是** `state.kind`：
# 联机局在 `state` 里就是一次 `reset_lan()`（`kind == Kind.LAN`），与"本机 LAN + Bot"
# 同一种 kind —— 两者的区别只有"战场是谁摆的"。
#
# 联机为真时，本节的数据是**服务端那一份**：
#   * 士兵 = `room.entities` 里 `kind == "soldier"` 的那些（坐标/队伍/生死都来自 `PosInfo`
#     /`TeamInfo`/`LifeInfo`），**不再**调 `_spawn_position()`（`docs/134` §7.1 第 1 条）；
#   * 障碍 = `room.obstacles`（`AddObstacle` 的顺序），**不再**用内核的关卡生成器；
#   * 靶子 = 没有（服务端的 `Match` 里只有士兵，`docs/117`）。
var _online_mode := false
## 正在订阅的会话。`begin_match()` 会被 App 调用**两次**（`joined` 一次、开局批到齐一次），
## 所以订阅必须可重入（先断后连）。
var _online_session: GwNetSession = null
## 服务端的 `PlayerInfo` 给的 player_id；`-1` = 不知道（降级口径见 `_online_can_fire()`）。
var _my_player_id := -1
## 服务端 `player_id` → `state.players` 的下标。
##
## `App._sync_state_from_net()` 是按 **id 排序**建表的，这里用同一个顺序，
## 于是 `_functions` / `_active` 这套既有的"按下标存函数"逻辑在联机下也能用。
var _pid_index := {}
## 本回合的射手实体 id（`TurnInfo.soldier_id`）；`-1` = 服务端还没说。
var _online_soldier := -1
## 本回合轮到哪个玩家（`TurnInfo.player_id`）。
var _online_turn_player := -1
## 最近一发复算出来的弹道与诊断（`online_state()` 读；不参与逻辑）。
var _online_shot := {}
var _online_shot_status := ""
## 服务端广播过的特效（`EffectInfo`）：`[{x, y, type, tick}]`。**只记录不绘制** ——
## `GwPlaneView` 没有特效图层，本轮不新造一个（`docs/139` §7）。
var _online_effects: Array = []
## 服务端已经宣布结束（`EndingGame`）。等当前这一发落地后才算收尾（`docs/143` §5）。
var _online_ending := false
## 服务端广播过的生死（`LifeInfo`）：`{entity_id: alive}`。
var _online_lives := {}

func _ready() -> void:
	font_divisor = 30        # ButtonGrid.gd / FireContainer.gd / ExtraOptionsHBox.gd 都是 /30
	super()


# ---------------------------------------------------------------- 搭界面

## 节点树 = **原版 `play_screen.tscn`**（106 个静态节点；`client/scenes/screens/play_screen.tscn`
## 与 `recovered-src/play_screen.tscn` **逐字节相同**，`cmp` 过）。
##
## 所以本函数只剩三件事：① 取句柄 ② 装 6 个真 GDExtension 画布节点 ③ 接线。
## **布局/字号/最小尺寸一个字都不在这里**：它们全在 `.tscn` 挂的**原版脚本**里
## （`ButtonGrid.gd` / `HBoxContainer.gd` / `ExtraOptionsHBox.gd` / `ExtraOptionPad.gd` /
## `ViewFunctionPad.gd` / `ViewFunctionsLabel.gd` / `FireContainer.gd` / `FxContainer.gd` /
## `TimerContainer.gd` / `OffsetContainer.gd` / `ExitButtonContainer.gd` /
## `MarginContainer.gd` / `ShowExtraButtons.gd`），**58** 个原生按钮的 `text`/`input_text`/`icon`
## 也全在 `.tscn` 里 ⇒ 手搭键盘/回放键/顶栏开关/顶底栏的那四段代码整份删除。
##
## 证据档：树=**逐字节**（`cmp` 与 `recovered-src/` 相同）；三个 `_scaffold` 类节点由
## 原版 Rust 控制器创建、我方还没复原那条路径 ⇒ 由本函数按 `.tscn` 的形状补（**我方接线**）。
func build() -> void:
	var P := "MarginContainer/VBoxContainer"
	var PL := P + "/PlayMainHBox/PlaneContainer"
	var L := P + "/PlayMainHBox/LeftControls"
	var R := P + "/PlayMainHBox/RightControls"
	var B := P + "/HBoxContainer2"
	var EO := PL + "/ExtraOptionsVBox/ExtraOptionsHBox"

	_plane_container = _node(PL)
	_extra_pad = _node(PL + "/GamePlane/ExtraOptionPad")
	_plane_aspect = _node(PL + "/GamePlane/PlaneContainer")
	_plane = _node(PL + "/GamePlane/PlaneContainer/GraphwarPlane")
	_view_functions_label = _node(PL + "/GamePlane/PlaneContainer/VBoxContainer/HBoxContainer/ViewFunctionsLabel")
	_extra_vbox = _node(PL + "/ExtraOptionsVBox")
	_extra_hbox = _node(EO)
	_view_func_button = _node(EO + "/ViewFunctionButton")
	_names_button = _node(EO + "/ShowNames")
	_chat_button = _node(EO + "/ChatButtonPlay")
	_mute_button = _node(EO + "/MuteButton")
	_show_extra_button = _node(L + "/LeftTopButtonsContainer/MarginContainer/ShowExtraButtons")
	_left_grid = _node(L + "/LeftButtonGrid")
	_replay_left = _node(L + "/LeftReplaysButtonsGrid")
	_replay_play_btn = _node(L + "/LeftReplaysButtonsGrid/AspectRatioContainer/PlayPauseButton")
	_fire_row = _node(R + "/HBoxContainer")
	_fire_container = _node(R + "/HBoxContainer/FireContainer")
	_fire_button = _node(R + "/HBoxContainer/FireContainer/FireButton")
	_right_grid = _node(R + "/RightButtonGrid")
	_replay_right = _node(R + "/RightReplaysButtonsGrid")
	_timer_label = _node(B + "/TimerContainer/TimerLabel")
	_fx_button = _node(B + "/FxContainer/AspectRatioContainer/FxTextureButton")
	_offset_label = _node(B + "/OffsetContainer/OffsetLabel")

	# `ViewFunctionsLabel` 在 `.tscn` 里带着一段**示例文本**（原版控制器运行时会改）。
	# 原版对局屏默认不显示它（`_show_function_overlay == false`）；实例化后必须自己关掉，
	# 否则棋盘上会印着 "player 1: f(x) = 3x+234"（那是 `.tscn` 里的占位串）。
	if _view_functions_label != null:
		_view_functions_label.visible = false

	# 函数输入框：`.tscn` 里这个节点是**原版类型** `GraphwarFunctionInput`（Rust 类，自绘）。
	# ⚠️ **遗留**：`client/tools/selftest.gd:1352` 对它有既有断言（`as GwFunctionInput` +
	# `.text`），断言不在本任务可改范围 ⇒ 这里**原位**换成我方同名控件（节点名/路径不变）。
	# 这条是"半原版"的一处，已登记在 docs/06 §94.7。
	var native_input := _node(B + "/GraphwarFunctionInput")
	_input = GwFunctionInput.new()
	_input.name = "GraphwarFunctionInput"
	_input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_input.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	if native_input != null and native_input.get_parent() != null:
		var holder := native_input.get_parent()
		var idx := native_input.get_index()
		_input.size_flags_horizontal = native_input.size_flags_horizontal
		_input.size_flags_vertical = native_input.size_flags_vertical
		holder.remove_child(native_input)
		native_input.queue_free()
		holder.add_child(_input)
		holder.move_child(_input, idx)
	elif _node(B) != null:
		(_node(B) as Control).add_child(_input)
	_input.text_changed.connect(_on_function_text_changed)
	_input.text_submitted.connect(_on_function_text_submitted)

	# 键盘/回放键的句柄（禁用态、回放版式用）。它们都是 `.tscn` 的**原生 `GraphwarButton`**。
	_keys.clear()
	for g in [_left_grid, _right_grid]:
		if g == null:
			continue
		for c in g.get_children():
			if c is Button:
				_keys.append(c as Button)
	# `YButton(y)`：原版 Normal/DiffEq 两种模式下都恒为 disabled（`docs/92` §4.10 观测）。
	if _right_grid != null:
		var y := _right_grid.get_node_or_null("YButton") as Button
		if y != null:
			y.disabled = true
			y.tooltip_text = "原版里这颗键在 Normal/DiffEq 两种模式下都恒为 disabled（原因未知，docs/92 §8）"

	# 装 6 个真节点（平面本身是 `.tscn` 里的节点 ⇒ 这里只装它下面的绘制层）。
	_install_plane_nodes()
	# 效果层：**只剩适配**（按种类记账 + 派给真 `EffectDrawer`）。名字保留 `EffectLayer`，
	# 因为自检按名字找它（`docs/143` §1）。
	_effects = GwEffectLayer.new()
	_effects.name = "EffectLayer"
	_effects.bind(_fx)
	if _plane != null:
		_plane.add_child(_effects)
	if _plane != null:
		_plane.resized.connect(_layout_plane_children)
	if _plane_container != null:
		_plane_container.resized.connect(_update_view_geometry)
	_isolate_extra_options_feedback()

	# ---------------- 接线 ----------------
	# ① 54 颗原生按钮的 `input_text` → `App.command`（过渡桥，`client/src/ui/gw_button.gd` 的
	#    `bind_native()`：`input_selected` / `long_press_finished` 两条自造信号）。
	GwButton.bind_native(self)
	# ② `Fire!`：原版只有 `long_press_input_text = "fire"`（长按派发，已由 ① 接上）；
	#    单击也要能开火（`GwButton._on_pressed` 的同一条约定）。
	#    ⚠️ 必须用**字符串形** `connect("pressed", …)`：原生类的 `#[func] pressed()` 与
	#    `BaseButton` 的 `pressed` 信号同名，`.pressed.connect()` 会解析到那个方法上。
	if _fire_button != null:
		_fire_button.connect("pressed", Callable(self, "_on_fire_pressed"))
	# ③ 回放三颗播放键：`.tscn` 的 `input_text` 就是原版的命令字面量（`⏮︎` / `play_pause` / `⏭`）。
	#    这三条**不走命令总线**（`app.gd` 对它们是占位提示）⇒ 直接接本屏播放器。
	var replay_cmds := {
		"⏮︎": GwCommands.KEY_PREV, "play_pause": GwCommands.PLAY_PAUSE, "⏭": GwCommands.KEY_NEXT,
	}
	if _replay_left != null:
		for n in _replay_left.find_children("*", "GraphwarButton", true, false):
			var txt := String(n.get("input_text"))
			if replay_cmds.has(txt):
				n.connect("pressed", Callable(self, "_on_replay_button").bind(replay_cmds[txt]))

	# ---------------- 我方自造的两块（原版没有，见各自的注释） ----------------
	# 回放进度条：原版回放界面没有这条（`docs/133` §4.2）。永不显示。
	_replay_bar = GwUI.hbox("ReplayStatusBar", 6, _node(P))
	_replay_bar.visible = false
	_replay_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_LEFT)
	_replay_label.name = "ReplayTickLabel"
	_replay_bar.add_child(_replay_label)
	_replay_progress = ProgressBar.new()
	_replay_progress.name = "ReplayProgress"
	_replay_progress.min_value = 0.0
	_replay_progress.max_value = 1.0
	_replay_progress.step = 0.0
	_replay_progress.show_percentage = false
	_replay_progress.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_replay_progress.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_replay_progress.custom_minimum_size = Vector2(0, 10)
	_replay_bar.add_child(_replay_progress)
	# 状态行：原版**没有**这行字（用户实测：底部多了一行说明）。保留节点但永不显示，
	# 只给 selftest / `--dump` 当诊断用（`docs/125` §5 登记）。
	_status_label = GwUI.label("", 12, HORIZONTAL_ALIGNMENT_LEFT)
	_status_label.name = "StatusLabel"
	_status_label.visible = false
	_status_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_status_label)

## 从 `.tscn` 里按路径取一个节点（取不到就报错并返回 `null` —— 宁可不画也不静默降级）。
func _node(path: String) -> Control:
	var n := get_node_or_null(path)
	if n == null:
		push_error("[gw2] play_screen.tscn 里找不到节点 " + path)
		return null
	return n as Control

## 把 `ExtraOptionPad.gd` / `ExtraOptionsHBox.gd` / `ViewFunctionPad.gd` / `ShowExtraButtons.gd`
## 挂在 `item_rect_changed` 上的处理器**摘掉**，改由本屏 `_update_view_geometry()` 驱动同一套判据。
##
## 为什么（**实测**，`docs/06` §94.6）：这四份原版脚本的判据是
##   `extra_y = plane_container.size.y − plane_container.size.x/1.66667`，然后回写自己的
##   `custom_minimum_size.y`/`visible`。`custom_minimum_size` 又会通过 VBox 反向改变
##   `plane_container` 的矩形 ⇒ **正反馈**。在真实窗口（1920×1160，`extra_y = 190 ≫ target 96`）
##   这个不动点稳定；但 **headless 自检的视口是 64×64**，`extra_y` 恰好落在阈值附近 ⇒
##   每帧在 `pc=(56,72)` 与 `pc=(13,72)` 两个状态间来回跳，一帧内排进几万条排布/重绘消息，
##   最后 `Message queue out of memory` → SIGSEGV（本轮实测 29 000+ 次 `resized`）。
##
## ⇒ 判据**照抄**（同一个式子、同一个 `target_y`），只把"触发源"从 `item_rect_changed`
## 换成"本屏尺寸变化时算一次"，反馈回路因此断开。语义等价性：1920×1160 下 `show = true`、
## `pad.min.y = 96`，与原版脚本给的值逐项相同（T26/T29 的真帧读数就是这么来的）。
func _isolate_extra_options_feedback() -> void:
	for nm in [_plane_container_path(), "MarginContainer/VBoxContainer/PlayMainHBox/LeftControls/LeftTopButtonsContainer"]:
		var nd := get_node_or_null(nm)
		if nd == null:
			continue
		for c in nd.get_signal_connection_list("item_rect_changed"):
			var cb: Callable = c["callable"]
			var obj: Object = cb.get_object()
			if obj == null or obj == nd:
				continue            # 引擎自己的 `Control::_size_changed` 不能摘
			if obj.get_script() == null:
				continue
			nd.disconnect("item_rect_changed", cb)

func _plane_container_path() -> String:
	return "MarginContainer/VBoxContainer/PlayMainHBox/PlaneContainer"

## `Fire!` 单击 → 开火（原版契约：`FireButton` 只有 `long_press_input_text`，
## 长按那条由 `GwButton.bind_native()` 接；单击这条是我方补的，见 `build()` ②）。
func _on_fire_pressed() -> void:
	fire()

## 回放版式：**原版回放屏长什么样，就摆成什么样**（`fixtures/replay_screen_targets.txt` §三）。
##
## 原版 `play_screen.tscn` 里这几块是**静态节点**，靠运行时 `visible` 切换
## （`LeftButtonGrid` / `RightButtonGrid` / `FireContainer` 那一行 / `*ReplaysButtonsGrid`）；
## 底部的 `TimerLabel` 不隐藏而是**空文本**（真帧上 `f(x) =` 仍在 x169..249，
## 说明 TimerContainer 那 140 px 还在，只是没有字符）。
##
## 我方自造的 `ReplayStatusBar` 一律不显示：原版底部只有 `f(x) =` + 大号函数文本 +
## 修正常数**一行**（§53.4 ③），读数改走 `replay_state()`（不画任何东西）。
func _set_replay_layout(on: bool) -> void:
	_replay_layout = on
	# 原版这几块都是 `.tscn` 里的**静态节点**，靠运行时 `visible` 切（`docs/06` §53.4）：
	# 对局屏 = 键盘 + Fire + 隐藏的回放键组；回放屏 = 反过来。
	if _left_grid != null:
		_left_grid.visible = not on
	if _right_grid != null and _replay_right != null:
		# ⚠️ **机制未知 + 我方补丁**：把 `RightButtonGrid` 藏掉之后，Godot 的 `Container`
		# 会把整根子节点从最小宽里剔掉 ⇒ 右列缩到回放组自己的天然宽（@1920 = 230），
		# 棋盘列因此变宽、整块版式崩塌（本机实测过）。原版回放屏右列仍是 **337**
		# （= 键盘 grid 的天然最小宽），但"宽度还在、高度不在"这一步**没有源码证据**。
		# 这里如实复现观测：藏键盘之前量一次它的天然最小宽，回放时把同一个宽度钉在
		# **可见的**回放组上。
		if on:
			if _right_grid_natural_w <= 0.0:
				_right_grid_natural_w = _right_grid.get_combined_minimum_size().x
			_right_grid.visible = false
			_replay_right.custom_minimum_size = Vector2(_right_grid_natural_w, 0.0)
		else:
			_right_grid.visible = true
			_replay_right.custom_minimum_size = Vector2.ZERO
	# Fire 行：**原版藏的是 `FireContainer` 这个 `MarginContainer`**（`0x3d9b50` 的第 3 个节点，
	# `esi=0` @ `0x3d9ce6`；见 `_fire_container` 的注），不是整行、也不是行里那两颗按钮。
	# ⇒ 那一行的最小高 = 0（`FireContainer` 没有可见子节点 ⇒ 自己 min = (0,0)），而它**仍然
	# 是 VBox 的一个可见子节点**（占掉一个 separation）—— 这一点决定右三键组的居中量：
	#   三键组 y = f + sep + (H − f − G − 3·sep)/2 + sep   （f = Fire 行高、H = 列高 1058、G = 396）
	#   f = 0   ⇒ y = 333（组顶）⇒ 三颗 433/533/633  ← 原版真帧
	#   f = 96  ⇒ y = 381（组顶）⇒ 三颗 481/581/681  ← 我方改前（差 48 = 96/2）★ 已修
	# 所以这里**不许**再写 `custom_minimum_size.y`（那正是 +96 的来源；`docs/06` §103）。
	if _fire_container != null:
		_fire_container.visible = not on
	if _fire_row != null:
		_fire_row.visible = true
		_fire_row.custom_minimum_size = Vector2.ZERO
	if _replay_left != null:
		_replay_left.visible = on        # `.tscn` 里默认 `visible = false`（对局屏不显示）
	if _replay_right != null:
		_replay_right.visible = on
		_replay_right.size_flags_vertical = Control.SIZE_FILL
	if _replay_bar != null:
		_replay_bar.visible = false      # 原版没有这条（我方自造，永不显示）
	# 回放模式下这两颗顶栏键是**禁用态**（观测：真帧上它们是灰面 `#383838` + 暗图标
	# `#888888`，而另外两颗是绿面 `#1A3B2D` + 白图标 ⇒ 只有这两颗 disabled）。
	# 语义也说得通（**推断**）：回放里没有聊天会话、也没有"附加按钮"可展开。
	if _show_extra_button != null:
		_show_extra_button.disabled = on
	if _chat_button != null:
		_chat_button.disabled = on
	if _timer_label != null and on:
		_timer_label.text = ""             # 原版回放屏底部没有 00:00
	if _input != null:
		# 回放屏的函数框是**只读**的（原版按不可编辑态画文字：灰 0.875×α0.5）
		_input.read_only = on
	_update_replay_pause_icon()
	# 布局诊断（`docs/16` §86）：`GW2_REPLAY_LAYOUT_DUMP=1` 时把右列那 5 个节点的真实矩形 +
	# 最小尺寸打到 stdout（→ `game.log`）。**默认关闭、不参与任何判定**，只给"量版式"用。
	# 为什么要它：`--headless --script` 的根窗口不是 1920×1160（量不到真布局），
	# 走 `tools/frame_capture.py --flow replay --seed-replays` 这条**真窗口**路径才与真帧同口径；
	# 本轮的定案数据就是这么来的（存成 `fixtures/replay_layout_dump.tsv`）。
	if OS.get_environment("GW2_REPLAY_LAYOUT_DUMP") == "1" and is_inside_tree():
		_dump_replay_layout(on)

## 布局诊断（见 `_set_replay_layout()` 末尾）。等 2 帧让容器 sort 完再打。
func _dump_replay_layout(on: bool) -> void:
	await get_tree().process_frame
	await get_tree().process_frame
	var R := "MarginContainer/VBoxContainer/PlayMainHBox/RightControls"
	var L := "MarginContainer/VBoxContainer/PlayMainHBox/LeftControls"
	var paths := [
		R, R + "/HBoxContainer", R + "/HBoxContainer/FireContainer",
		R + "/HBoxContainer/FireContainer/ClipButton",
		R + "/HBoxContainer/FireContainer/FireButton",
		R + "/MarginContainer", R + "/RightButtonGrid", R + "/RightReplaysButtonsGrid",
		R + "/RightReplaysButtonsGrid/HBoxContainer",
		R + "/RightReplaysButtonsGrid/GraphwarButton",
		R + "/RightReplaysButtonsGrid/GraphwarButton3",
		R + "/RightReplaysButtonsGrid/GraphwarButton4",
		R + "/MarginContainer2",
		L, L + "/LeftTopButtonsContainer", L + "/LeftButtonGrid", L + "/LeftReplaysButtonsGrid",
		"MarginContainer/VBoxContainer/PlayMainHBox",
		"MarginContainer/VBoxContainer",
	]
	print("@@LAYOUT on=%s viewport=%s win=%s" % [str(on), str(get_viewport_rect().size), str(get_window().size)])
	for p in paths:
		var n := get_node_or_null(p) as Control
		if n == null:
			print("@@LAYOUT %s || MISSING" % p)
			continue
		var gr := n.get_global_rect()
		var mn := n.get_combined_minimum_size()
		print("@@LAYOUT %s || vis=%s || pos=(%.1f,%.1f) size=(%.1f,%.1f) || gpos=(%.1f,%.1f) || min=(%.1f,%.1f) || cmin=(%.1f,%.1f) || vflag=%d hflag=%d" % [
			p, str(n.visible), n.position.x, n.position.y, n.size.x, n.size.y,
			gr.position.x, gr.position.y, mn.x, mn.y,
			n.custom_minimum_size.x, n.custom_minimum_size.y,
			n.size_flags_vertical, n.size_flags_horizontal])

## 播放/暂停键的图标：原版 `PlayPauseButton` 的 `icon` 在 `play_option`（▶）与
## `pause_option`（⏸）之间切（`docs/121` §4 的资产名映射；`GwIcons.FIXED` 里
## `play_pause → pause_option`、`play_option → play_option`）。
func _update_replay_pause_icon() -> void:
	if _replay_play_btn == null:
		return
	var playing := (_replay != null and bool(_replay.playing()))
	_replay_play_btn.icon = GwIcons.get_icon("play_pause" if playing else "play_option")

# ---------------------------------------------------------------- 尺寸（照原版脚本的比例）

## 字号：**几乎全在原版脚本里**（`ButtonGrid.gd` = 宽/30、`ExtraOptionsHBox.gd` = 宽/30、
## `FireContainer.gd` = 宽/30、`FxContainer.gd` = 宽/60、`TimerContainer.gd` = 宽/30、
## `OffsetContainer.gd` = 宽/60、`ExitButtonContainer.gd` = 宽/30 …，都是 `.tscn` 挂上去的）。
## 基类 `apply_font_scale()` 会把 `res://src/views/` 与 `res://<Name>.gd` 别名两种形态都认成
## "原版脚本"并整棵跳过 ⇒ 这里只剩**我方控件**的三处。
func apply_font_scale() -> void:
	super.apply_font_scale()
	# `EntityDrawer` 的名字标签**不走本屏的字号表**：原版那一层是 `name_label.tscn` 的
	# `RichTextLabel`，字号取**主题默认**（`graphwar_theme.tres` 的 `default_font_size = 24`，
	# 真帧上 `cmdgz` 的墨迹高 ~22 px 就是它）。基类 `_apply_size_recursive()` 会遍历整棵树给
	# **所有** `RichTextLabel` 写 `normal_font_size = 宽/N × 0.85`；本屏 N 小 ⇒ 那串被放大到
	# ~68 px（实测我方名字墨迹 202 px 宽 vs 原版 72 px）。这里把那层覆盖撤掉。
	if _entities != null:
		for child in _entities.get_children():
			if child is RichTextLabel:
				(child as RichTextLabel).remove_theme_font_size_override("normal_font_size")
	if not _built:
		return
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		return
	var key := w * 0.15 / 3.0
	if _input != null:
		# **只给宽度**：高度必须由 `GwFunctionInput._get_minimum_size()` 按 LineEdit 的规则
		# 算（字体高 + 样式盒上下 content_margin ⇒ 只读 98 / 可编辑 96）。改前这里写死 `key`
		# (=96)，于是回放屏底栏比原版矮 2 px、`PlayMainHBox` 高 1060 而不是 1058、
		# 棋盘整体下移 2 px（`docs/06` §58.3 的竖直分量）。
		_input.custom_minimum_size = Vector2(key * 2.0, 0.0)
		_input.call("refresh_minimum_size")
	_update_view_geometry()
	if _timer_label != null:
		# 观测：原版**无限时限**时 TimerLabel 也占 140 px @1920（0.0729·x）宽度，只是没有字符
		# （`orig_campaign_play.png` 左下角一片空白，但 GEOM 里 TimerContainer = 140 px）。
		# 这里同样保留宽度，否则整条底栏会整体左移 ~140 px（docs/125 §4）。
		_timer_label.custom_minimum_size = Vector2(w * 0.0729, 0)

## `ExtraOptionPad.gd` / `ExtraOptionsHBox.gd` / `ViewFunctionPad.gd` / `ShowExtraButtons.gd`
## 的**同一个判据**（`docs/archive/125` §4）：
##   余高 `extra_y = plane_container.size.y − plane_container.size.x/1.66667`；
##   余高 ≥ 左上按钮行高 `target_y` ⇒ 附加按钮区展开（`ExtraOptionPad` 撑 `target_y` 高）。
## 触发源换成"本屏尺寸变化"，见 [`_isolate_extra_options_feedback`] 的理由。
func _update_view_geometry() -> void:
	if _plane_container == null or _extra_hbox == null:
		return
	var pc := _plane_container.size
	if pc.x <= 1.0 or pc.y <= 1.0:
		return
	var w := get_viewport_rect().size.x
	var target_y := w * 0.15 / 3.0          # = `LeftTopButtonsContainer.size.y`（左上按钮行高）
	var extra_y := pc.y - pc.x / BOARD_ASPECT
	var show := extra_y >= target_y
	_extra_hbox.visible = show
	if _extra_pad != null:
		_extra_pad.custom_minimum_size = Vector2(0, target_y if show else 0)
		_extra_pad.visible = show
	# `ShowExtraButtons` 的 disabled 是原版脚本自己算的（`ShowExtraButtons.gd::_on_size_changed`）：
	# 竖直余量够（附加按钮区已自动展开）时这颗开关是灰的。判据同上。
	if _show_extra_button != null:
		_show_extra_button.disabled = show

# ---------------------------------------------------------------- 对局流程

## 由 App 在切到本界面后调用。
func begin_match() -> void:
	var app_node := app()
	if app_node == null:
		return
	var st = app_node.state
	# 回放分支：`app.gd` 只做 `state.reset_replay(path)` + `begin_match()`（它不在本轮
	# 可改范围里），所以分叉必须在这里。其余字段（玩家表）在回放里是空的 ——
	# 真实局面由**回放自己的事件**给出（PlayerInfo/SoldierInfo/PosInfo/…）。
	if st.kind == GwGameState.Kind.REPLAY and String(st.replay_file) != "":
		_begin_replay(String(st.replay_file))
		return
	# 联机分支：**战场来自服务端**（`docs/139`；`docs/134` §7.1 第 1 条）。
	# 判 `App.online` 而不是 `st.kind`：联机局的 kind 也是 LAN（见成员区注释）。
	if bool(app_node.online):
		_replay_mode = false
		_replay = null
		if _replay_bar != null:
			_replay_bar.visible = false
		_set_replay_layout(false)
		_set_replay_input_enabled(true)
		_begin_online(app_node)
		return
	_online_teardown()          # 从联机局退回本地局时把订阅放掉（幂等）
	_replay_mode = false
	_replay = null
	_replay_bar.visible = false
	_set_replay_layout(false)
	_set_replay_input_enabled(true)
	_functions.clear()
	_shot_queue.clear()
	_last_result = {}
	_soldiers.clear()
	_targets = GwTargetSet.new()
	_destroyed_this_shot.clear()
	_last_verdict = {}
	_reset_shot_state()
	var level := GwCampaign.level(int(st.level_index)) if st.kind == GwGameState.Kind.CAMPAIGN else {}
	var team_counts := {0: 0, 1: 0}
	# 玩家颜色（`docs/102` §1.2：本队 6 色表 + 随机起点 + 线性探测）。真值是 RNG 抽的，
	# 这里用确定性种子，保证同一局每次配色一致。
	var teams: Array[int] = []
	for p in st.players:
		teams.append(int(p.team))
	var colors := assign_colors(teams, int(level.get("seed", 1)) + 1)
	for pi in st.players.size():
		var p: Dictionary = st.players[pi]
		_functions[pi] = ""
		# 士兵数（`docs/118` §7.1 的「我方士兵」列）：**战役关由关卡数据定**
		# （立即数 `edx=N` → `0x476320(mgr, player, N)`，21/21 与 385 份 replay 对齐），
		# 不是设置界面的默认值。以前这里无条件用 `p.soldiers`（默认 1），
		# 于是 `Level 3/5/9/11/14/18/19/20`（2~5 个士兵）在界面上全都只摆 1 个。
		# 联机/局域网局仍用玩家表自己的数（服务端才说得清）。
		var want := int(p.soldiers)
		if st.kind == GwGameState.Kind.CAMPAIGN and not bool(p.is_bot):
			want = int(level.get("soldiers", want))
		for s in want:
			var team := int(p.team)
			var slot := int(team_counts[team])
			team_counts[team] = slot + 1
			_soldiers.append({
				"name": String(p.name),
				"team": team,
				"pos": _spawn_position(team, slot),
				"alive": true,
				"is_bot": bool(p.is_bot),
				"skin": int(p.skin),
				"bot_tier": int(p.get("bot_tier", 0)),
				"player": pi,
				"color": colors[pi],
			})
	# 回合轮转：每玩家一张士兵表（`docs/143` §6，规则同 `src/graphwar_turn_manager.rs`）。
	_reset_turn_driver()
	_active = 0
	if _active_soldier_idx >= 0 and _active_soldier_idx < _soldiers.size():
		_active = int(_soldiers[_active_soldier_idx].player)
	_time_left = float(GwModes.turn_seconds(st.time_mode))
	_running = true
	_set_axis_mode(int(st.axis_mode))
	_refresh_mode_indicators()
	_refresh_plane()
	_load_active_function()
	_update_timer_label()
	_apply_layout_sizes()
	_replay_left.visible = st.kind == GwGameState.Kind.REPLAY
	_replay_right.visible = st.kind == GwGameState.Kind.REPLAY
	# 靶子：**数目**来自 `docs/118` §7.1（13 个打靶关），**位置是我们的自定值** ——
	# 原版由关卡生成器随机放置，§8 #16 明确"本来就没有每关固定坐标"这回事。
	# 障碍：**模式 + min_count 来自 `docs/118` §7.1，坐标/半径由内核的生成器算**（`docs/126`）。
	var positions: Array = []
	if st.kind == GwGameState.Kind.CAMPAIGN:
		positions = _target_positions(int(st.level_index), int(level.get("targets", 0)))
		_obstacles = _level_obstacles(int(st.level_index))
	else:
		# 非战役（LAN / 回放）：目前服务端还没有广播 AddObstacle，先按"没有障碍"处理。
		# 真接上之后这里应当用**服务端下发的那批**，不要用本地生成（见 docs/126 §5）。
		_obstacles = []
	if _layout_override.has("targets"):
		positions = (_layout_override["targets"] as Array).duplicate()
	if _layout_override.has("obstacles"):
		_obstacles = (_layout_override["obstacles"] as Array).duplicate()
	_targets.setup(positions)
	_sync_entities()
	_set_obstacles(_obstacles)
	# 胜负规则（`docs/131` §2）：战役关按 `docs/118` §7.1 的"目标数 / Bot 数"两列分类；
	# 非战役对局没有靶子也没有回合上限，只有"对方全灭"这一条。
	_rules = GwLevelRules.new()
	if st.kind == GwGameState.Kind.CAMPAIGN:
		_rules.from_campaign(level, _human_team())
	else:
		_rules.from_versus(_human_team())
	# 回合时钟（Rust）：档位 = 原版 `TimeMode` 的判别值（`0..5`，与 `GwModes.TimeMode` 同一套）。
	_turn_clock.set_time_mode(int(st.time_mode))
	_bot.configure(BOT_THINK_SECONDS)
	_bot.disarm()
	# 开局：`Setup --GameStarted--> WaitingForFunctions`（`turnstate::next_state`）。
	# 原版这一步在 `root_node` 那侧把状态字节写成 2（`0x475bac` 是同族的写点，
	# 收场写 6）；我们没有那台 root_node，开局点就是这里。
	_match_state.apply("game_started")
	_status("关卡规则：%s ／ 靶子 %d ／ 回合上限 %s" % [_rules.kind_name(), _targets.total(),
		"∞" if _rules.turn_limit() <= 0 else str(_rules.turn_limit())])

func end_match() -> void:
	_running = false
	_replay_mode = false
	_replay = null
	if _replay_bar != null:
		_replay_bar.visible = false
	_set_replay_layout(false)
	_set_replay_input_enabled(true)
	_clear_curve()
	_reset_shot_state()
	_online_teardown()

func _apply_layout_sizes() -> void:
	apply_font_scale()

## 队伍固定在 x 两侧（`docs/105`：flip 由队伍决定，不靠几何猜）。
func _spawn_position(team: int, slot: int) -> Vector2:
	var spread := 3.4
	var y := (slot - 1.5) * spread
	return Vector2(-18.0 if team == 0 else 18.0, y)

## 靶子位置：**我们的自定值**。原版由关卡生成器随机放置（`docs/118` §7.2 只给了障碍公式，
## 靶子坐标未解），所以这里用关卡种子的确定性伪随机，落在右侧半区。
func _target_positions(level_index: int, count: int) -> Array:
	var out := []
	if count <= 0:
		return out
	var rng := RandomNumberGenerator.new()
	rng.seed = GwCampaign.level_seed(level_index) + 7
	for i in count:
		var x := rng.randf_range(12.0, 22.0)
		var y := rng.randf_range(-10.0, 10.0)
		out.append(Vector2(x, y))
	return out

## 画面上"靶子在哪"的**唯一**来源。
##
## 三条路的 id 口径不一样，别混：
## * **联机**：`_target_entity_ids[i]` = 线上 `TargetInfo{target_id}` 的**实体 id**（本样本 232）。
##   靶子的位置/生死仍由 `GwTargetSet` 存（`target_id` 是它的**下标**），
##   所以这里做一次 `下标 → 实体 id` 的翻译；这样 `EntityDrawer` 拿到的 id 与
##   名字/外观/颜色 manager 拿到的是同一个（= id 空间一致）。
## * **回放**：`target_positions()` 是一张**纯位置表**（没有 id）⇒ 用下标当 id，
##   落在 `TARGET_ID_BASE` 之上，与士兵 id 不撞。
## * **本地战役/关卡**：`GwTargetSet` 的 `target_id` 由关卡生成器给（小整数）⇒ 同样
##   `TARGET_ID_BASE + target_id`，理由同上。
func _view_targets() -> Array:
	var out := []
	if _replay_mode:
		var positions: Array = (_replay.target_positions() if _replay != null else [])
		for i in positions.size():
			out.append({"id": TARGET_ID_BASE + i, "pos": positions[i]})
		return out
	if _online_mode and _target_entity_ids.size() == _targets.total():
		for t in _targets.entities():
			var idx := int(t["target_id"])
			out.append({"id": _target_entity_ids[idx], "pos": t["pos"]})
		return out
	for t in _targets.entities():
		out.append({"id": TARGET_ID_BASE + int(t["target_id"]), "pos": t["pos"]})
	return out

## 局面 → 画面。旧实现在这里调仿制控件 `set_soldiers()/set_show_names()`；
## 现在整棵树由真 `EntityDrawer` 持有（见 `_sync_entities()`），本函数只做同步 + 重画。
func _refresh_plane() -> void:
	_sync_entities()
	if _plane != null:
		_plane.queue_redraw()

# ---------------------------------------------------------------- 六个真节点（docs/16 §48）

## 装 5 个绘制层节点到**原版 `.tscn` 的那个 `GraphwarPlane`** 下面。
##
## 平面本身不再是 `ClassDB.instantiate` 出来的：它就是 `.tscn` 的静态节点
## （`.../GamePlane/PlaneContainer/GraphwarPlane`），`AspectRatioContainer` 直接把它铺成
## 棋盘矩形（1279×767.4 @1920×1160，与真帧黑框逐项相同）。
## 三个 `FunctionDrawer` 由平面自己在 `_ready` 里建（原版 `init_children`），这里只 `build_children()`
## 补调一次；`AxisDrawer` / `EntityDrawer` / `EffectDrawer` / `ObstacleDrawer` 原版是控制器建的
## （`docs/16` §48.5），我方还没复原那条路径 ⇒ 由本函数造，**按平面矩形铺满**。
##
## 换算为什么成立（T44 §2① 的等价性证明，`fixtures/board_geometry_equiv.tsv`）：
## 六个类都是**按自己控件的 `size` 换算**（`unit = size.x/50`、中心 `size/2`）。以前它们统一
## 摆在 1000×600 画布上、由父节点 `Control.scale = size.x/1000` 缩放；现在直接把 `size` 给成
## 平面矩形。两条链在 `size = 1279×767.4` 上**逐值相同**（差 ≤0.02 px = `ratio 1.6667` 的取整）。
func _install_plane_nodes() -> void:
	if _plane == null:
		return
	# `#[export]` 属性：原版 `GraphwarPlane` 的默认值就是这两个（`board_scale` 只在
	# `size.x/50` 算不出来时当兜底；`flip_y` = 原版"棋盘 y 向上"那条永远取负）。
	_plane.set("board_scale", PX_PER_UNIT)
	_plane.set("flip_y", true)
	# 棋盘黑框的**半边在矩形外面**（`draw_rect` 的 0.2 单位描边以边线为中心，@1279 宽 = 5.12 px）
	# ⇒ 真帧上只看见矩形内侧那 ~3 px（`docs/06` §94.5 的两侧逐像素扫描：原版黑框 300..302、
	# 外侧 297..299 是背景色）。剪到平面矩形上就是把外半边去掉，与真帧同形。
	_plane.clip_contents = true
	# 3 个 `FunctionDrawer`：原版在 `init` 里建，本类在 `ready`；显式调一次（幂等），
	# 这样 `set_function_points()` 当场就有节点可喂（§40）。
	# ⚠️ **不给它们设 size**：原版这 3 个节点的 `size` 就是 `(0,0)`（T44 实测：原版 tscn
	# 实例化后 `@FunctionDrawer@N size=(0,0)`），变换由平面把 `(size×0.5, size.x/50, flip)`
	# 喂给 `update_transform()`（`0x4b6147`）。
	_plane.call("build_children")

	# ① `AxisDrawer`：白底 + 网格。`z_as_relative = false` + `z_index = -1` ⇒ 画在
	#    棋盘框**之下**（框是平面自己的 `_draw`，引擎先画父再画子）。
	_axis = _make_node("AxisDrawer")
	if _axis != null:
		_axis.name = "AxisDrawer"
		_plane.add_child(_axis)
		_axis.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_axis.z_as_relative = false
		_axis.z_index = Z_AXIS
		_axis.set("axis_mode", int(GwModes.AXIS_DEFAULT))

	# ② `ObstacleDrawer`：由平面建（原版这一层是平面的字段，`§45.5` 说明为什么不自动建）。
	_plane.call("build_obstacle_drawer")
	for ch in _plane.get_children():
		if String(ch.get_class()) == "ObstacleDrawer":
			_obstacle_layer = ch as Control
	if _obstacle_layer != null:
		_obstacle_layer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_obstacle_layer.z_index = Z_OBSTACLE

	# ③ `EntityDrawer`：士兵 / 名字 / 靶子。
	_entities = _make_node("EntityDrawer")
	if _entities != null:
		_entities.name = "EntityDrawer"
		_plane.add_child(_entities)
		_entities.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_entities.z_index = Z_ENTITY

	# ④ `EffectDrawer`：特效。`flip_y = true` 与棋盘 y 向上一致
	#    （`effect_drawer::draw_transform` 的第二个实参就是它）。
	_fx = _make_node("EffectDrawer")
	if _fx != null:
		_fx.name = "EffectDrawer"
		_plane.add_child(_fx)
		_fx.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_fx.z_index = Z_FX
		_fx.set("flip_y", true)
	_layout_plane_children()

## 按类名造一个扩展类节点；扩展没加载时返回 `null` 并记下原因。
func _make_node(cls: String) -> Control:
	if not ClassDB.class_exists(cls):
		_nodes_error = "%s 不在 ClassDB（GDExtension 没加载：先 cargo build --features gdext-bindings → cp 覆盖 client/*.so → --import）" % cls
		push_error("[gw2] " + _nodes_error)
		return null
	var node: Object = ClassDB.instantiate(cls)
	if node == null:
		_nodes_error = "ClassDB.instantiate(%s) 返回空" % cls
		push_error("[gw2] " + _nodes_error)
		return null
	return node as Control

## 把四个绘制层铺成**平面矩形**（`GraphwarPlane` 的 `size`）。
##
## 以前这里是 `_fit_canvas()`：把 1000×600 的画布用 `Control.scale` 缩进
## `AspectRatioContainer` 给的矩形。装原版 `.tscn` 之后不需要缩放了 —— 平面自己就是那块矩形，
## 绘制层跟着它走同一组 `size`（各类的换算式都是 `size.x/50`、`size/2`，见 `docs/16` §48.2）。
## 判据：`fixtures/board_geometry_equiv.tsv`（两套公式在同一块矩形上逐值相同）。
func _layout_plane_children() -> void:
	if _plane == null:
		return
	var sz := _plane.size
	if sz.x <= 1.0 or sz.y <= 1.0:
		return
	for n in [_axis, _obstacle_layer, _entities, _fx]:
		if n != null:
			(n as Control).position = Vector2.ZERO
			(n as Control).size = sz

## 棋盘绘制层铺出来的矩形（诊断 / 自检用）。
func plane_rect() -> Vector2:
	if _plane == null:
		return Vector2.ZERO
	return _plane.size

## 真节点装上了没有（`null` = 扩展没加载；诊断/自检用）。
func plane_nodes_ready() -> bool:
	return _plane != null and _axis != null and _entities != null and _fx != null

func plane_nodes_error() -> String:
	return _nodes_error

# ---------------------------------------------------------------- 曲线（真 FunctionDrawer）

## 把一条弹道交给**真平面**：`set_function_points()`（原版 `GraphwarFunctionManager::
## set_function_points`，`0x456810`）+ `set_fractional_tick()`（原版 `GraphwarTickHandler::
## get_fractional_tick`，`0x519530`）+ `feed_functions()`（原版 `GraphwarPlane::draw` 尾部
## 那段循环，`0x4b6339`–`0x4b6a13`）。
##
## `points` 是**棋盘坐标**（`x ∈ [-25,25]`）。颜色的打包口径是 `0x00RRGGBB`（低字节 = R，
## `FunctionDrawer::color_from_packed` 的三条 shift）。
func _set_curve(points: PackedVector2Array, color: Color, player_id: int) -> void:
	_curve_player = player_id
	_curve_total = points.size()
	_curve_tick = 0.0
	_curve_final_tick = ceilf(float(_curve_total) / CURVE_POINTS_PER_TICK)
	if _plane == null or _curve_total < 1:
		return
	var flat := PackedFloat64Array()
	flat.resize(_curve_total * 2)
	for i in _curve_total:
		flat[i * 2] = points[i].x
		flat[i * 2 + 1] = points[i].y
	# 起飞 tick = **原版 `FunctionState` 的 `value+0x74`**（联机时来自
	# `FunctionActive{active_tick}`，本样本 615；见 `_curve_active_tick()`）。
	# 它决定这条弹道**开不开始画**：`GraphwarPlane::draw` 的在场谓词是
	# `active_tick < tick`（严格，`0x3be5f6` 的 `jbe`）。
	#
	# 改前：这里恒传 **0**（注释写着"我方选择"）⇒ 只要本地 tick ≥ 1 就开始长，
	# 而原版要等服务端那条 `FunctionActive` 的 615 被本地 tick 追上。
	var active_tick := _curve_active_tick(player_id)
	_plane.call("set_function_points", player_id, flat, active_tick, _packed_color(color))
	#
	# **回放不走墙钟**：曲线的揭示量是 `回放 tick − active_tick`（原版 `GraphwarPlane::draw`
	# 把 `GraphwarTickHandler::get_fractional_tick` 喂给喂点循环，而 `is_active_function`
	# 拿它和 `active_tick` 比）。以前这里给"揭完"⇒ 载入态就把整条弹道画了出来，
	# 而真帧上原版载入态**没有弹道**（`docs/06` §53.4 ④）。
	_curve_tick = (_replay_curve_tick() if _replay_mode else 1.0)
	_plane.call("set_fractional_tick", _curve_tick)
	_feed_curve()

## 这条弹道的**起飞 tick**（原版 `GraphwarFunctionManager` 的 `FunctionState.value+0x74`）。
##
## 三条路径口径不同，各自有出处：
## * **联机**：服务端 `FunctionActive{active_tick}` 是唯一真值（本样本 = **615**，
##   `fixtures/frame_match_events.txt` [phase2]）。没收到那条事件前记 0 —— 原版
##   `FunctionState` 的默认值也是 0，且那时本地 tick 已经远超窗口，照样不画。
## * **回放**：留 **0**。回放喂给平面的 `fractional_tick` 本身就是"已揭示多少 tick"
##   （`_replay_curve_tick()` = 回放 tick − 事件里的 active_tick），再减一次就负了。
## * **本地局**：留 0（时钟从 1.0 起，等价于第 1 个 tick 开始长）。
func _curve_active_tick(player_id: int) -> int:
	if _online_mode:
		var ticks: Dictionary = _online_room().get("active_ticks", {})
		return int(ticks.get(player_id, 0))
	return 0

## 回放模式下"这一条弹道已经揭到第几个 tick" = `回放 tick − active_tick`（夹到 0..揭完）。
##
## `active_tick` 来自 `FunctionActive` 事件（本样本 = 615）；没有那条事件 ⇒ 退回"已揭完"
## （只在回放确实缺 `FunctionActive` 时出现，不编造 0）。
func _replay_curve_tick() -> float:
	if _replay == null:
		return 0.0
	var snap: Dictionary = _replay.snapshot()
	var active := int(snap.get("active_tick", -1))
	if active < 0:
		return _curve_final_tick
	var elapsed := float(int(snap.get("tick", 0)) - active)
	return clampf(elapsed, 0.0, _curve_final_tick)

## 回放每走一个 tick 就把曲线往前揭一格（暂停时**不动**）。
func _replay_advance_curve() -> void:
	if _plane == null or _curve_total < 2:
		return
	var t := _replay_curve_tick()
	if is_equal_approx(t, _curve_tick):
		return
	_curve_tick = t
	_plane.call("set_fractional_tick", _curve_tick)
	_feed_curve()

## 喂一次点 + 摆一次 `Line2D` 的变换。
##
## `feed_functions()` 平时由平面的 `_draw` 调（原版就是这样），这里**再显式调一次**：
## headless 下不保证每帧都进 `_draw`，自检要的是确定的揭示点数。
## `update_transform()` 只对**已经建出来**的 `Line2D` 生效 ⇒ 每次喂完都要补一次。
func _feed_curve() -> void:
	if _plane == null:
		return
	_plane.call("feed_functions")
	# 变换的四个实参**从平面矩形派生**（原版 `update_transform` `0x4b6147`：`x = size.x×0.5`、
	# `y = size.y×0.5`、`s = size.x/50.0`、`flip`），不再写死 `(500, 300, 20, -20)`
	# —— 那组数只在参考画布 1000×600 上成立。
	var sz := _plane.size
	if sz.x <= 1.0:
		return
	var unit := sz.x / 50.0
	for fd in _plane.get_children():
		if String(fd.get_class()) == "FunctionDrawer":
			fd.call("update_transform", sz.x * 0.5, sz.y * 0.5, Vector2(unit, -unit), false)
	_plane.queue_redraw()

## 按秒推进曲线（`ticks = 秒 × 60`），到"揭完"那个 tick 就停住（不越过后面的淡出窗口）。
func _advance_curve(seconds: float) -> void:
	if _plane == null or _curve_total < 2:
		return
	if _replay_mode:
		return        # 回放的揭示量由 `_replay_advance_curve()` 按 tick 推（不走墙钟）
	if _curve_tick >= _curve_final_tick:
		return
	_curve_tick = minf(_curve_tick + seconds * CURVE_TICKS_PER_SECOND, _curve_final_tick)
	_plane.call("set_fractional_tick", _curve_tick)
	_feed_curve()

## 画到第几个点 —— **读 `Line2D` 本身**（原版 `FunctionDrawer` 的 value 里那个"已写入数"，
## `docs/16` §40.3a），不是我们这边另记的账。
func _curve_reveal() -> int:
	if _plane == null or _curve_player < 0:
		return 0
	for fd in _plane.get_children():
		if String(fd.get_class()) == "FunctionDrawer":
			return maxi(0, int(fd.call("point_count", _curve_player)))
	return 0

## 已经揭到末端（= 旧 `GwFireFx.shot_state()["finished"]`）。
func _curve_finished() -> bool:
	if _curve_total < 2:
		return true
	return _curve_reveal() >= _curve_total

## 清掉曲线（一发结算完 / 换局）。
##
## **两件事都要做**（实测踩过）：
## 1. `clear_function_points()` 只清平面里那张登记表，**不**删 `Line2D`；
## 2. `Line2D` 是 `FunctionDrawer` 的私有表（`lines: BTreeMap`）里的，只有
##    `remove_function`（原版 `0x498b70`：`queue_free` + 删表）能删它。
##
## 只做第 1 步的后果不是"看着没删"，而是**下一条曲线画不出来**：同一 player id 再
## `update_function` 时那一侧还记着"已写入 N 点"，`append_window` 判 `new_len <= N`
## 直接 return（`0x498d5a` 的 `ja`）⇒ 新曲线一个点都不加，旧线还留在屏幕上。
## 平面的 `_draw` 里那条 `remove_function`（`0x4b6418`）只在"这条函数已经离开活跃窗口"
## 时才跑，而这里登记表已经空了 ⇒ 必须自己调。
func _clear_curve() -> void:
	var player_id := _curve_player
	_curve_total = 0
	_curve_player = -1
	_curve_tick = 0.0
	_curve_final_tick = 0.0
	if _plane == null:
		return
	_plane.call("clear_function_points")
	if player_id >= 0:
		for fd in _plane.get_children():
			if String(fd.get_class()) == "FunctionDrawer":
				fd.call("remove_function", player_id)
	_plane.queue_redraw()

## 棋盘上那条曲线**实际画出来的点**（读 `Line2D`，棋盘坐标）。诊断/自检用。
func plane_points() -> PackedVector2Array:
	var out := PackedVector2Array()
	if _plane == null or _curve_player < 0:
		return out
	for fd in _plane.get_children():
		if String(fd.get_class()) != "FunctionDrawer":
			continue
		var lid := int(fd.call("line_instance_id", _curve_player))
		if lid == 0:
			continue
		var line: Object = instance_from_id(lid)
		if line == null:
			continue
		var n := int(line.call("get_point_count"))
		for i in n:
			out.append(line.call("get_point_position", i))
		return out
	return out

## `Color` → `0x00RRGGBB`（`FunctionDrawer::color_from_packed` 的逆：低字节 = R）。
static func _packed_color(c: Color) -> int:
	return (clampi(int(round(c.r * 255.0)), 0, 255)
		| (clampi(int(round(c.g * 255.0)), 0, 255) << 8)
		| (clampi(int(round(c.b * 255.0)), 0, 255) << 16))

# ---------------------------------------------------------------- 网格 / 障碍 / 名字

## 网格档位（`AxisMode` 的 4 档，`docs/118` §7.1）：0 = 没有、1 = 只主轴、2 = 每 5、3 = 每 1。
func _set_axis_mode(v: int) -> void:
	if _axis == null:
		return
	_axis.set("axis_mode", clampi(v, 0, 3))
	# `#[export]` 字段改值不会自己重画（gdext 不替我们发 `queue_redraw`）。
	_axis.queue_redraw()

## 障碍：清空掩膜再一个个光栅化（`plane::rasterize`，与 `GraphwarObstacleManager::
## add_obstacle`（`0x3f17a0`）同构，已 unicorn 对拍）。贴图由平面每帧刷 2 次
## （`ObstacleDrawer::update`，`0x3ee190`）。
##
## ⚠️ **坐标要传棋盘单位**，不要自己换成像素：`plane::raster_params()`
## （`0x3f17a0` 那条光栅化路径的入口）自己就做那次换算 ——
## `px = x*1000/50 + 500`、`py = y*1000/50 + 300`、`r = size*1000/50`。
## （本轮踩过：先换成像素再传，等于换了两次 ⇒ 圆心被推到 (10500, 6300)、半径 800，
## 一个像素都画不出来。自检里"注入 1 个障碍 ⇒ 掩膜里有像素"那条就是为它加的。）
func _set_obstacles(list: Array) -> void:
	_obstacles = list
	if _obstacle_layer == null:
		return
	_obstacle_layer.call("clear_obstacles")
	for o in list:
		var p: Vector2 = o.get("pos", Vector2.ZERO)
		var r: float = float(o.get("size", 0.0))
		_obstacle_layer.call("add_obstacle", float(p.x), float(p.y), r)

## 名字开关（`toggle_names`）。真 `EntityDrawer` 没有这个开关 ⇒ 用**空文本**表达"不显示"：
## 它每帧 `relayout()` 会把标签的 `visible` 按死亡 alpha 重写，从外面改 `visible` 会被覆盖。
func _set_show_names(v: bool) -> void:
	_show_names = v
	_sync_entities()

## 一条**只读**的战场快照：每个字段都从真节点读回来（自检 / `--dump` 诊断用）。
##
## 键名沿用旧 `GwPlaneView` 的那几个（`path_points` / `soldiers` / `targets` / `obstacles`），
## 值改成真节点的口径：`soldiers`/`targets` 是 `EntityDrawer` 里的**实体数**，
## `path_points` 是 `Line2D` 上**已经画出来**的点数，另外附上各自的"原样"计数。
func plane_state() -> Dictionary:
	var curve := plane_points()
	var entity_count := (int(_entities.call("entity_count")) if _entities != null else -1)
	var occupied := (int(_obstacle_layer.call("occupied_pixels")) if _obstacle_layer != null else -1)
	return {
		"nodes_ready": plane_nodes_ready(),
		# 棋盘矩形（`AspectRatioContainer` 铺给 `GraphwarPlane` 的那块）与**等效缩放**
		# （矩形 / 参考画布 1000×600）。T44 之前这里读 `_canvas.size` 与 `Control.scale`；
		# 装原版 `.tscn` 之后平面自己就是那块矩形、不再有 `Control.scale` ⇒ 用同一组量纲的
		# 等价量（两个键的语义与自检断言 `canvas>1 且 scale>0` 一致）。
		"canvas_size": (_plane.size if _plane != null else Vector2.ZERO),
		"plane_scale": (Vector2(_plane.size.x / CANVAS_SIZE.x, _plane.size.y / CANVAS_SIZE.y) if _plane != null else Vector2.ZERO),
		"nodes_error": _nodes_error,
		"path_points": curve.size(),
		"path_reveal": _curve_reveal(),
		"path_total": _curve_total,
		"path_player": _curve_player,
		"path_tick": _curve_tick,
		"soldiers": entity_count,
		"soldier_count": _soldiers.size(),
		"targets": _view_targets().size(),
		"entities": entity_count,
		"obstacles": _obstacles.size(),
		"obstacle_pixels": occupied,
		"axis_mode": (int(_axis.get("axis_mode")) if _axis != null else -1),
		"axis_lines": (int(_axis.call("axis_line_count")) if _axis != null else -1),
		"effects": (_fx.call("effect_count") if _fx != null else -1),
		"fx_draws": (_fx.call("draw_call_count") if _fx != null else -1),
		"plane_draws": (_plane.call("draw_call_count") if _plane != null else -1),
		"obstacle_updates": (_plane.call("obstacle_drawer_updates") if _plane != null else -1),
		"function_drawers": (_plane.call("function_drawer_count") if _plane != null else -1),
		"show_names": _show_names,
	}

##
## 实体的 id 约定（`EntityDrawer` 只认 i32 id，没有"种类"）：士兵 = 它在 `_soldiers` 里的
## 下标；靶子 = `TARGET_ID_BASE + target_id`。名字标签 = 士兵名（`_show_names` 为假时给空串
## —— `EntityDrawer` 没有"隐藏名字"的开关，空文本是唯一不打架的做法：
## 它每帧 `relayout()` 会把 `visible` 按死亡 alpha 重写，从外面改 `visible` 会被覆盖）。
##
## 阵亡：`set_entity_dead_ticks(id, ENTITY_DEAD_TICKS)` ⇒ 标签与名字按 `death_alpha` 淡出；
## 联机的死者**整个撤掉**（服务端 `Match::fire` 就把死者 retain 掉了，`docs/139`）。
## 局面 → `EntityDrawer` 的实体表。
##
## ## 2026-11-26（T26）**id 空间**修正（这是"事件全接上也画不对"的总根因）
##
## 旧实现交给 `EntityDrawer` 的 id 是**士兵下标** `i`（0,1,2…），而线上事件与 22 个复原
## manager（位置/外观/名字/颜色/队伍/生命）用的都是**实体 id**（本样本：玩家 229、士兵 230、
## 被删的 231、靶子 232）。两套编号互不相干 ⇒ T17 喂进 manager 的外观/名字/颜色
## **一个消费者都没有**（`docs/06` §67.8 第 3 条就是这么记的）。
## 现在：有实体 id 就用实体 id（联机 `entity_id`、回放 `id`），只有**本地战役局**
## （士兵是界面自己摆的、线上没有实体 id）才退回下标 —— 那是唯一没有实体 id 的一条路。
##
## 靶子那条也一样：联机靶子的 id 就是 `TargetInfo{target_id}` 里的**实体 id**（232），
## 不再映射到 `TARGET_ID_BASE + target_id`（那是本地战役/回放"靶子只有位置表、没有 id"时
## 的自造编号）。`_view_targets()` 负责给出这个 id，本函数只管用。
func _sync_entities() -> void:
	if _entities == null:
		return
	var want := {}
	for i in _soldiers.size():
		var s: Dictionary = _soldiers[i]
		var alive := bool(s.get("alive", true))
		if _online_mode and not alive:
			continue
		# 实体 id 的三条来源（优先级从上到下）：联机 `entity_id` → 回放 `id` → 本地局下标。
		var eid := int(s.get("entity_id", s.get("id", i)))
		want[eid] = {
			"pos": s.pos,
			# 名字：**`room["names"]` 的真值优先**（`NameInfo` 的实体 id 可能只指玩家、
			# 也可能指士兵 ⇒ 先实体再 `player_id`，见 `_online_entity_name()`）。
			# 旧实现读本地 profile 名（`app_node.state.players[pi].name`，默认 `"user"`），
			# 那不是这一局的真值 —— 真值是 `NameInfo{229:"cmdgz"}`（`docs/06` §67.8 第 2 条）。
			# **现读**（不是建屏时缓存）：`NameInfo` 比 `begin_match()` 晚到（本剧本事件 36 vs 35），
			# 每次 `_sync_entities()` 都按 `room` 的当前值算，晚到就能自己补上。
			"name": (_online_entity_name(eid, int(s.get("player_id", -1)),
				int(s.get("player", 0)), app()) if _online_mode and _show_names
				else (String(s.name) if _show_names else "")),
			"dead": (-1 if alive else ENTITY_DEAD_TICKS),
			"actor_id": eid,                   # 外观表按实体 id 索引（回放/联机同一个口径）
			# 颜色：**现读** `room["colors"]`（`ColorInfo` 同样比 `begin_match()` 晚到）。
			# 这个值同时喂 ①名字标签的笔画色（`set_entity_name_color`）②准备圈的环色
			# （`EntityDrawer` 的 `team_color`，下面逐实体设）⇒ 两项的原文都要求"= 该实体 ColorInfo"：
			# 真帧上名字笔画色与环色**逐值相同** = `(125,104,0)`（`docs/06` §60.5 / §71.1-B 两处量过）。
			"color": (_online_entity_color(eid, int(s.get("player_id", -1)),
				s.get("color", Color.WHITE)) if _online_mode
				else s.get("color", Color.WHITE)),
			"kind": "soldier",
		}
	for t in _view_targets():
		want[int(t["id"])] = {"pos": t["pos"], "name": "", "dead": -1, "kind": "target"}
	# 先删：`EntityDrawer` 的句柄与节点同生共死（`remove_entity` 里 `queue_free`）。
	for id in _drawn_entities.keys():
		if not want.has(id):
			_entities.call("remove_entity", id)
			_drawn_entities.erase(id)
	# 再建/重摆：`spawn_entity` 幂等（同 id 只重摆位置），名字与死亡刻跟着更新。
	for id in want:
		var e: Dictionary = want[id]
		var p: Vector2 = e["pos"]
		_entities.call("spawn_entity", id, p.x, p.y)
		_entities.call("set_entity_name", id, String(e["name"]))
		# 名字的颜色 = **这个实体自己的队色**（真帧：士兵名 `cmdgz` 笔画色 (125,104,0) =
		# 它 `ColorInfo` 的颜色，逐值相同）。**我方接线**：原版这条颜色的来源未解
		# （见 `EntityDrawer::set_entity_name_color` 的文档）。
		if String(e["name"]) != "" and e.has("color"):
			_entities.call("set_entity_name_color", id, e["color"])
		_entities.call("set_entity_dead_ticks", id, int(e["dead"]))
		_drawn_entities[id] = true
		# ---- 外观（`docs/16` §44）：原版由 `set_{skin,face,hat}_graphics` 的 51/53/55 事件喂
		# `EntityDrawer` 的 `HashMap<i32,String>`；那条链路（`GraphwarGraphicsManager`）还没接，
		# 所以这里按"线上事件已经落在 `room` 里的那份事实"代填（`session.gd` 的
		# `Skin/Face/HatGraphicsInfo` → `room["look"][entity_id]`）。**按实体 id 索引** ——
		# 这与 manager 收的是同一批 id（见本函数头的 id 空间说明）。
		# 没有 look 事实时才退回默认三件套 `yellow/regular_eyes/soldier_helmet`
		# （= `GraphwarPlayerConfig::new` 的字面量，arm64 `0x39152c..0x3915a4`）；
		# 靶子退回 `target/no_face/no_hat`（`res://skins/target.svg` 在表里）。
		# ⚠️ 这是**我方接线**，原版是 graphics 事件驱动的（`docs/16` §44.0）。
		var look := _look_for(id, String(e.get("kind", "")))
		if not look.is_empty():
			_entities.call("set_entity_look", id, String(look.get("skin", "")),
				String(look.get("face", "")), String(look.get("hat", "")))
		elif String(e.get("kind", "")) == "target":
			_entities.call("set_entity_look", id, "target", "no_face", "no_hat")
	# ---- 准备圈：**填 `prep` 表**（`docs/16` §50.6）----
	#
	# 原版 `EntityDrawer::draw`（`0x4b2890`）只画 `prep` 哈希表里的实体，而填表的是
	# `GraphwarBackend` 的 `pos_info_event` —— 那条链路还没接。这里**由播放屏按回合状态代填**。
	# ⚠️ 这是**我方接线**，不是观测到的原版路径。
	#
	# ## 2026-11-26（T21）：回放屏**要**画准备圈 —— 旧注释"回放不画"是被证伪的
	#
	# 旧注释写"原版真帧：棋盘上 0 个红箭头（§53.4 ③/§55.3）⇒ 回放模式不画准备圈"。真帧复测
	# （`replay_play_1.png`，本文件 §复跑命令）把话说准了：**红箭头确实没有，但三段弧有**。
	# 尺子：队色 `(125,104,0)` 的全部像素做圆拟合 ⇒ 环心 `(807.09, 830.93)`、半径 `19.28 px`
	# （内 15.30 / 外 23.25）、三段弧各张 `59.1°–60.1°`（⇒ H = π/6，`base` 未命中那条分支）、
	# 方位 `{26.5°, 146.5°, 266.5°}`（⇒ `A0 = V/120`、`V ≈ 558` = 载入 tick）。
	# **径向直方图在 r ∈ [15.3, 23.3] px 之外一个像素都没有** ⇒ 那 3 个外凸箭头（外沿到
	# `r+0.5` 单位 ≈ 32 px）真的没画。⇒ 本屏：开环（`set_ring`）、**关箭头**、
	# `V` = 回放播放头（= `GraphwarTickHandler::get_fractional_tick`，原版 `0x4b2e10`）、
	# `base` = 同一个 `V`（⇒ `V−base = 0` ⇒ `H = π/6`，与真帧 60° 张角逐值一致）。
	#
	# 原版弧的门是 `get_alive` + `is_on_turn`（§46/§48 观测，`0x454950`/`0x462b50`）；回放侧
	# 没有 turn 状态源（`_turn_driver` 那条链只在本地局走）⇒ 我方按"回放里**所有活着的士兵**"
	# 画环。本样本只有 1 具士兵 ⇒ 与真帧一致；多士兵回放可能多画，**登记为遗留**（不是观测）。
	# ⚠️ 环的键必须是**实体 id**（与 `want` 同一口径），不是士兵下标 —— 下标那条在
	# 联机局是 0/1/2…，`want` 里根本没有那个键 ⇒ 环一个都画不出来（改前的现象）。
	var ring_ids := {}
	if _replay_mode:
		for i in _soldiers.size():
			var s: Dictionary = _soldiers[i]
			var seid := int(s.get("entity_id", s.get("id", i)))
			if bool(s.get("alive", true)) and want.has(seid):
				ring_ids[seid] = true
	elif _active_soldier_idx >= 0 and _active_soldier_idx < _soldiers.size():
		var acting: Dictionary = _soldiers[_active_soldier_idx]
		var aeid := int(acting.get("entity_id", acting.get("id", _active_soldier_idx)))
		if bool(acting.get("alive", true)) and want.has(aeid):
			ring_ids[aeid] = true
	# 箭头开不开：回放关（真帧径向直方图，见上）；本地局开（`match_ready` 真帧那份"带准备圈"
	# 的形态里有箭头，`docs/06` §60.1 —— 那一屏还没对齐，开关先保持上一轮的行为）。
	_entities.set("show_ring_arrows", not _replay_mode)
	# 回放：`V` = 播放头（`ReplayDriver::tick()`，载入态 = 第 1 批 `exec_tick` = 558）；
	# 本地局：沿用 `_curve_tick`（我方时钟，不是原版的 `get_fractional_tick`）。
	var ring_v: float = float(_replay.tick()) if (_replay_mode and _replay != null) else _curve_tick
	for id in want:
		if ring_ids.has(id):
			var rp: Vector2 = want[id]["pos"]
			# 环色 = **这具士兵自己的队色**（真帧：环的 377 个像素全是 `(125,104,0)`，
			# 与这名士兵 `ColorInfo` 的颜色逐值相同）。原版从 `draw` 的两张哈希表里
			# **逐实体**取打包 RGB；本类的 `team_color` 只有一个 ⇒ 多士兵异色时环会同色
			# （**登记遗留**：本样本 1 具士兵，够用）。
			# 打包口径：低字节 = R（`entity_drawer::Color::from_packed_rgb`）。
			if want[id].has("color"):
				var rc: Color = want[id]["color"]
				_entities.set("team_color", (rc.r8 | (rc.g8 << 8) | (rc.b8 << 16)))
			_entities.call("set_ring", id, rp.x, rp.y, GwCore.SOLDIER_HIT_RADIUS, ring_v)
			# `base` = 同一个 `V` ⇒ `H` 取到下限 π/6（真帧 60° 张角）。原版 `map[i]` 的来源
			# 仍是未知（§50.6）；这里给"起始量 = 现在"，张开动画从 0 开始。
			_entities.call("set_ring_base", id, ring_v)
			_ring_base[id] = ring_v
		else:
			_entities.call("clear_ring", id)
			_ring_base.erase(id)

## 联机的靶子图层（**原版是独立图层**，`docs/131`）。
##
## 数据来源全是 T17 已经落进 `room` 的事实（`client/src/net/session.gd`）：
##   `TargetInfo{target_id}`      → `room["targets"]`（**实体 id** 列表，按到达顺序）
##   `PosInfo{entity_id,pos_x…}`  → `room["entities"][id].x/.y`
## ⇒ 这里只做"搬进 `GwTargetSet`"这一件事，**不新增语义**。
##
## `GwTargetSet` 的 `target_id` 是**下标**（它只存位置 + 生死，`docs/131` §3），
## 所以同时留住 `下标 → 实体 id`：`_view_targets()` 用它把实体 id 交给 `EntityDrawer`，
## 这样靶子的位置/外观/名字（manager 按实体 id 说的）与绘制是同一个 id 空间。
func _build_online_targets(room: Dictionary) -> void:
	var ids: Array = room.get("targets", [])
	var ents: Dictionary = room.get("entities", {})
	var positions: Array = []
	_target_entity_ids.clear()
	for tid in ids:
		var e: Dictionary = ents.get(int(tid), {})
		positions.append(Vector2(float(e.get("x", 0.0)), float(e.get("y", 0.0))))
		_target_entity_ids.append(int(tid))
	_targets.setup(positions)
	if not ids.is_empty():
		_status("联机靶子 %d 个（实体 id %s）" % [ids.size(), str(_target_entity_ids)])

# ---------------------------------------------------------------- 障碍（docs/126）

## 本局的障碍列表。**生成在内核里**（`src/graphwar_level_generator.rs` 的既有实现，公式逐位验证过），
## 本界面只负责把结果存起来、画出来、并原样交给 `hit_test`。
##
## 种子语义（`docs/126` §2.2）：
## * 原版**每局重新抽种子**（`docs/118` §7.1 的 `R` 列：同一关出现过 10…29 个障碍），
##   所以这里默认每局给一个不同的种子（`randi()` 混进关卡种子）；
## * 但 `--selftest` / `--dump-tree` / `--shots=` 这三种"要可复现"的运行**固定**用
##   `GwCampaign.level_seed()` —— 否则自检与截图基准每跑一次变一张图。
func _level_obstacles(level_index: int) -> Array:
	var base := int(GwCampaign.level_seed(level_index))
	var seed_value := base if _reproducible_run() else (base ^ int(randi()))
	var gen := GwCore.level_obstacles(level_index, seed_value)
	if not bool(gen.get("ok", false)):
		# 内核没有这个方法（或参数被拒）：**不**退回"界面自己撒点"——原版没有这种行为。
		_status("障碍取不到（%s），本局按无障碍处理" % String(gen.get("error", "?")))
		return []
	var list: Array = gen.get("obstacles", [])
	_status("障碍 %d 个（内核生成，种子 %d）" % [list.size(), seed_value])
	return list

## 本次运行是否要"可复现"（自检 / dump / 截图基准）。
func _reproducible_run() -> bool:
	for a in OS.get_cmdline_user_args():
		if a == "--selftest" or a == "--dump-tree" or a.begins_with("--shots="):
			return true
	return false

func _refresh_mode_indicators() -> void:
	var app_node := app()
	if app_node == null:
		return
	var st = app_node.state
	if int(st.function_mode) == 0:
		_fx_button.icon = GwIcons.get_icon("fx_option")
	else:
		# DiffEq：同位置换成 dy/dx 图标（原版 FxTextureButton 的图标切换，docs/92 §4.10）
		_fx_button.icon = GwIcons.get_icon("dx_dy_option")
	_names_button.icon = GwIcons.get_icon("view_names" if _show_names else "hide_names")

# ---------------------------------------------------------------- 输入与开火

func key_input(cmd: String) -> void:
	if not _running:
		return
	if _replay_mode:
		return          # 回放不接管输入（docs/133 §4.1）
	_input.key_input(cmd)
	_functions[_active] = _input.text

## 物理键盘（`docs/135` §1）—— **屏幕级接管**，不用先点输入框。
##
## 映射表在 `GwKeyInput`（那张表里逐条写了哪几条是观测、哪几条是自定）；
## 这里只做三件事：挡住"不该接管"的情形、把命令喂给屏上键盘用的同一条 `key_input()`、
## 把事件标记为已处理（否则同一次按键会被别的控件再吃一遍）。
##
## 为什么不挂在 `GwFunctionInput._gui_input`：原版确实是"控件聚焦后接管"
## （`GraphwarFunctionInput` 是 `LineEdit` 子类且实现了 `gui_input`，见 `GwKeyInput` 文件头），
## 但"进对局后会不会自动抢焦点"**没有观测**；屏幕级接管对"自动聚焦"和"要点一下"
## 两种情形都成立。代价与升级路径写在 `docs/135` §1.3。
func _unhandled_key_input(event: InputEvent) -> void:
	if not _running or not is_visible_in_tree() or not (event is InputEventKey):
		return
	# 聊天框之类的**真**文本控件拿到焦点时不抢键（原版这时会由 LineEdit 自己处理）。
	var focused := get_viewport().gui_get_focus_owner() if get_viewport() != null else null
	if focused != null and (focused is LineEdit or focused is TextEdit):
		return
	var cmd := GwKeyInput.command_for(event as InputEventKey)
	if cmd == "":
		return
	if cmd == GwKeyInput.SUBMIT:
		_input.submit()          # 与 Fire 按钮同一条链：text_submitted → fire()
	else:
		key_input(cmd)
	get_viewport().set_input_as_handled()

# ---------------------------------------------------------------- 准备圈：几何已复原，**触发还没接**
##
## 原版的准备圈是 **`EntityDrawer::draw`**（vaddr `0x4b2890`）里 3 次 `draw_arc` +
## 3 个外凸箭头画的（`docs/140` §2 逐条立即数）。**这一段已经落地**：
## `src/_scaffold/entity_drawer_class.rs` 的 `_draw` 把 `entity_drawer::entity_primitives`
## 的 6 条图元逐条发成 `draw_arc` / `draw_polygon`（`docs/16` §50；
## `client/tools/entity_drawer_check.gd` 有 46 条断言钉住半径 / 起止角 / 线宽 / 折点 / 箭头顶点）。
##
## **但画面上暂时仍然看不到它**，卡的是**触发**而不是绘制：
##
## * `_draw` 只画 **`prep` 表里**的实体（原版也只遍历自己那两张 `HashMap`）。这张表要由
##   调用方填 —— `EntityDrawer.set_ring(id, x, y, pos_radius, v)`（配 `set_ring_base`）。
##   原版填它的是 `ready` 里订的 `pos_info_event`（`entity_drawer::READY_EVENTS` 第 1 个），
##   而 `GraphwarBackend` 还没接到节点层 ⇒ **本文件里也没有任何一处调 `set_ring`**。
## * 补它的最小改法（**下一轮**，本轮任务书不许动本文件的逻辑）：在 `_sync_entities()`
##   的循环里只对"当前行动的那具士兵"调
##   `_entities.call("set_ring", id, p.x, p.y, radius, v)`，其余实体
##   `_entities.call("clear_ring", id)`；`v` 取 tick 源（`get_fractional_tick`，§48 已接）。
##   `pos_radius` 是 `PosInfo` 的第 3 个 f64，**语义仍未标定**（`docs/140` §3.1 那 14% 的缺口）。
## * 先删掉的 `fire_fx.gd` 那条"假圈"路**不要再造一遍**：圈必须由 `EntityDrawer::draw` 画。
##
## `docs/16` §49.5 第 1 条与 §50 是这条缺口的正式记录。
func _on_function_text_changed(new_text: String) -> void:
	_functions[_active] = new_text
	# 右下修正常数**跟着当前输入重算**（旧实现是"手改就清空"，那样 S7 空函数一开始就永远是空的，
	# 而原版 S7 常显 `- 9.890` = `y0 − f(x0)` 在 `f ≡ 0` 时的值）。
	# 旧实现里那个 `_suppress_offset_clear` 开关（"程序化改写时别把刚算出来的修正抹掉"）
	# 随之一并删掉：现在**唯一**来源是"当前输入 + 当前射手"，重算不会抹掉任何东西。
	_refresh_offset_label()
	_refresh_function_list()

func _on_function_text_submitted(_new_text: String) -> void:
	fire()

## 修正文本的**显示串**：`" - 9.890"` —— **串首一个空格**。
##
## 那个空格不是我们挑的：内核自己的格式函数 `src/graphwar_shot_calculator.rs::correction_text()`
## 返回的就是 `format!(" - {:.3}", c)` / `format!(" + {:.3}", c)`；同一个函数喂
## `corrected_text()` 才拼得出 `x+2sin(20x) - 3.694` 这种"函数 + 空格 + 修正"的形态
## （`docs/24` §6.2、`docs/127` §2.1）。
##
## 而 `GwCore.format_offset()` 是 scaffold 侧的**数值**格式（`"- 9.890"`，不带那个空格），
## 所以显示前在这里补。真帧判据（**观测**，`docs/06` §79.4(a)）：原版右下修正文本墨迹
## `x1764..1883`（1200 ink px）；`Label` 内容矩形左边界 1745.4、fs=32 一个空格 advance = 14、
## `-` 的左侧 bearing ≈ 4.6 ⇒ 四个候选里只有"**LEFT 对齐 + 前导空格**"落在 **1764.0**
## （CENTER+空格 1780 / CENTER 无空格 1773 / LEFT 无空格 1750）。
static func _offset_display(offset: float) -> String:
	var txt := GwCore.format_offset(offset)
	return (" " + txt) if txt != "" else ""

## 右下 `OffsetLabel` = **当前输入对应的修正常数** `c`。常显（不是"开火后才写"、也不是"手改就清空"）。
##
## ## 公式（有真值）
## `c = y0 − f(x0)`：`(x0, y0)` = 射手位置，`f` = 当前输入框里的函数。
## 来源是内核自己的实现 `src/graphwar_shot_calculator.rs::correction_text`
## （`docs/24` §6.2 的格式样本 + 抓包帧 143/144 的空函数负样本 `- 3.420` = 当时那个 `y0`），
## 并且在本样本上能用**三个数**独立复核（全部逐位）：
##   * 空函数（S7）：`f ≡ 0` ⇒ `c = y0 = −9.889980889…` ⇒ `format_offset(c)` = **`- 9.890`**
##     —— 正是原版 S7 真帧上那一串；
##   * 函数 `x+2sin(20x)`（S8 服务端下发的第一发）：`c = y0 − f(x0) = −3.694413988…`
##     ⇒ **`- 3.694`**，与服务端 `corrected_function = "x+2sin(20x) - 3.694"` 的常数项一致；
##   * 同一发的落点：`f(end_x) + c = 0.8236080576`，与 `EffectInfo{pos_y: 0.8236080576517795}`
##     逐位相同 ⇒ 说明 `c` 的定义就是"让曲线过射手"的那个平移量。
##
## ## 为什么原版 S8 显示的仍是 `- 9.890`
## 原版的输入框里**没有人打字**（线上事件不回填输入框）⇒ `f ≡ 0` ⇒ `c = y0`。
## 旧实现是"开火后把内核按**收到的**函数串算出来的修正写进标签" ⇒ S8 显示 `- 3.694`、S7 空，
## 数值与出现时机都与原版不同（`docs/06` §60.5 表最后一行）。现在只有**一个**来源。
##
## 回放屏不在这里：那边的修正文本来自录像（`_replay_draw_shot()`，还带一个前导空格，
## 是"从 `corrected_function` 里切出来的后缀"，见那段注释）。
func _refresh_offset_label() -> void:
	if _offset_label == null or _replay_mode:
		return
	var app_node := app()
	if app_node != null and int(app_node.state.function_mode) != 0:
		_offset_label.text = ""     # DiffEq 局没有修正常数（观测：原版那一格为空）
		return
	var shooter := _active_soldier()
	if shooter.is_empty():
		return
	var c := float(shooter.pos.y)   # 空函数 ⇒ f ≡ 0 ⇒ c = y0
	var expr := String(_input.text).strip_edges() if _input != null else ""
	if expr != "":
		var direction := 1.0 if int(shooter.get("team", 0)) == 0 else -1.0
		var res := GwCore.calculate_path(expr, shooter.pos, direction)
		if not bool(res.get("ok", false)):
			return                  # 半截表达式（如 `sin(`）算不出来：保持上一次的值，不写 0
		c = float(res.get("offset", c))
	_offset_label.text = _offset_display(c)

## 开火：算一条弹道并**开始播放**它。结算在弹道飞完时（`_on_shot_arrived()`）。
##
## 三条时序护栏（`docs/143` §3–§5）：
## * **发射期间不能再发**（`_shot_in_flight`）—— 原版靠 `GameStateInfo` 状态机拦住：
##   `Fire` 只在 `WaitingForFunctions` 下有意义（`src/turnstate.rs`，13 次迁移来自真抓包）；
## * 不是我的回合不能发（本地：射手必须是人类玩家的士兵）；
## * 结算**延后到弹道飞完**，判胜负在结算之后。
## 返回**这一发是否真的交出去了**（超时自动开火要用它决定要不要兜底换手）。
func fire() -> bool:
	var app_node := app()
	if app_node == null or not _running:
		return false
	if _replay_mode:
		_status("回放模式不接管输入（这一局的每一发都来自录像）")
		return false
	# ① 发射期间不能再发（docs/143 §4）。原版的状态机里 `Fire` 只从
	# `WaitingForFunctions` 出发（`turnstate::next_state`），弹道在飞时服务端根本不收
	# `FunctionFire`。⇒ 这条早退**问 Rust 的状态机**（`GwTurnState.can_fire()`），
	# 不再读 `_shot_in_flight` 那条投影。联机局不驱动本地状态机 ⇒ 仍旧读投影。
	var can_fire := _match_state.can_fire() if not _online_mode else not _shot_in_flight
	if not can_fire:
		_status("上一发还在飞（服务端此时是 ExecutingResults）⇒ 不能再发")
		return false
	var active_soldier := _active_soldier()
	if active_soldier.is_empty() or not bool(active_soldier.alive):
		_status("当前射手已阵亡")
		return false
	if not _online_mode and bool(active_soldier.get("is_bot", false)):
		_status("现在是 AI 的回合（玩家 %s）⇒ 本地不接受人类开火"
			% String(active_soldier.get("name", "?")))
		return false
	var expr := String(_input.text).strip_edges()
	if expr == "":
		_status("先输入一个函数")
		return false
	# 联机：开火不再走本地结算（`docs/134` §7.1 第 3 条）。判定、生死、回合全在服务端；
	# 本界面只发两条请求，然后等 `FunctionPoints` 把弹道带回来（`_draw_online_shot()`）。
	if _online_mode:
		_fire_online(expr, active_soldier)
		return true                      # 联机：请求发出去了就算"交出去"
	var team := int(active_soldier.team)
	var direction := 1.0 if team == 0 else -1.0     # 队伍固定在两侧（docs/105）
	var result := GwCore.calculate_path(expr, active_soldier.pos, direction)
	if not bool(result.ok):
		_status("弹道算不出来：%s" % String(result.note))
		return false
	var pts: PackedVector2Array = result.points
	# 障碍挡弹道（`docs/30` §2：障碍让弹道**停下**，士兵只是被穿过）。
	# 这里只决定"**画**到哪一点"：`shot.points` 仍保留整条点序列 ——
	# 将来的 tick 动画（父 agent 另派）应当沿 `shot.points` 逐点推进、到
	# `drawn.impact` 停住，**不要**在这里写死"一次画完整条 = 本回合结束"。
	var drawn := _clip_to_obstacle(pts)
	var impact: Vector2 = drawn.impact
	# 曲线交给真平面：原版 `GraphwarFunctionManager::set_function_points` + `FunctionDrawer`。
	# 逐 tick 揭示**由 Rust 侧算**（`revealed_points()`：30 点/tick），本屏只推 tick。
	# `shot.points` 仍是全量 ⇒ "画到哪"与"飞到哪"是两件事，与回合结算没有耦合。
	_set_curve(drawn.points, active_soldier.get("color", GwThemeFactory.COL_ACCENT), _active)
	# 修正常数（`docs/24` §6.2 的样本格式：`+ 4.638` / `- 8.315`，三位小数、符号后一个空格）。
	# DiffEq 模式下原版 OffsetLabel 为空（观测）。
	# 修正常数：与"当前输入 + 当前射手"同一个来源（`_refresh_offset_label()`）——
	# 内核这一发算出的 `result.offset` 就是 `y0 − f(x0)`，两处同值，这里不再各写一份。
	_refresh_offset_label()
	if bool(drawn.blocked):
		_status("弹道撞到障碍，停在 (%.2f, %.2f) ／ 本局障碍 %d 个 ／ 点序列 %d 点（未截断）"
			% [impact.x, impact.y, _obstacles.size(), pts.size()])
	else:
		_status("弹道来源：%s ／ %s ／ 本局障碍 %d 个" %
			[String(result.source), String(result.note), _obstacles.size()])

	var shot := {"points": pts, "drawn": drawn.points, "team": team,
		"shooter": _active, "soldier": active_soldier, "phase": "local"}
	_commit_shot(shot)
	return true

## 把一发（或同时回合的一批）弹道交给结算，然后走**回合结算**（`docs/131` §2、§4）。
##
## 同时回合制下只有"所有还活着的人都打过"才结算一次（`docs/86`）——那一批算**一个**回合。
## 顺序与原版语料一致的那部分：`docs/84`/`docs/86` 的结算本体仍在 `src/graphwar_shot_calculator/settle.rs`。
func _commit_shot(shot: Dictionary) -> void:
	var app_node := app()
	if app_node == null:
		return
	# 状态机：`WaitingForFunctions --Fire--> CalculatingResults`（`turnstate::next_state`）。
	# 原版这条迁移就是 `set_state`（`0x4780cc`）写 `[self+0x120] = 3`。
	_match_state.apply("fire")
	# 命中表在**开火这一刻**就算好（= 原版的 `SynchronizingResults`：服务端立刻算完），
	# 但**不立刻生效**：哪一发什么时候落地由弹道自己的 tick 决定（`docs/65`）。
	shot["judged"] = _judge_shots([shot])
	# `CalculatingResults --ResultsComputed--> SynchronizingResults`（= 4）。
	_match_state.apply("results_computed")
	if int(app_node.state.turn_mode) == GwModes.Turn.SIMULTANEOUS:
		# 同时回合：先全部记下，轮完再结算（真实结算在 src/graphwar_shot_calculator/settle.rs，docs/86）。
		# **"本回合齐了没有"是流程** ⇒ 由 Rust 的 `RoundCollection` 判（`docs/141` §1：
		# 一回合 N 发一个事务，按公告顺序收）。本屏只递"这一发是谁打的"。
		# ⚠️ 这一支**没有**接逐 tick 时序（见 `docs/143` §9 "未做"）：整批一落地就算完。
		if _shot_queue.is_empty():
			_turn_driver.round_open(_simultaneous_announced())
		_shot_queue.append(shot)
		var collected := _turn_driver.round_submit(_active)
		if bool(collected.get("complete", false)):
			_resolve_shots(_shot_queue)
			_shot_queue.clear()
			_turn_driver.round_reset()
			_next_turn(true)
			_settle_round()
			_match_state.apply("turn_ended")   # 整批结算完 ⇒ 回 WaitingForFunctions
		else:
			_next_turn(false)
		return
	# 顺序回合：弹道先飞，**飞完才结算**（`docs/143` §3）。
	_begin_shot(shot)
	if _fast_forward():
		# 自动化运行（`--selftest` / `--dump-tree`）：把时序**压平** —— 逻辑当刻落地，
		# 动画照放（自检既要在下一帧读到结算结果，也要能看"弹道还在飞"）。
		# 落地在 Rust 侧、**只落命中不吐到达**（`ShotLifecycle::land_all`）：曲线照放，
		# 到站与收尾仍等 `_advance_presentation`（命中先落地、动画照放）。
		_apply_shot_events(_flow.land_all())
		_finish_round()

# ---------------------------------------------------------------- 一发弹道的播放与落地（docs/143）

## 自动化运行（`--selftest` / `--dump-tree`）把"发射 → 结算"的时序**压平**。
##
## 压平的只有**触发时机**：判定规则走的是同一条代码路径（`_judge_shots` /
## `_apply_shot_events` / `_settle_round`）。真机（返回 `false`）走逐 tick：
## 死亡在弹道到达那个点时落地、胜负在弹道飞完之后才判（`docs/143` §3）。
func _fast_forward() -> bool:
	var a := app()
	return a != null and bool(a.get("automation"))

## 清掉"一发"的所有中间状态（换局 / 退出对局时调；幂等）。
##
## 状态本体（阶段 / 待落地队列 / 已结算闩 / 回合数）在 Rust 的 `_flow`：
## 这里只剩视图层自己的东西（曲线、特效、视角）。
func _reset_shot_state() -> void:
	_shot_in_flight = false
	_pending_shot = {}
	_judged = {}
	_flow.reset()                     # 生命周期 + 回合计数一起归零（Rust）
	_match_state.reset()              # 对局状态机回 `Setup`（Rust）
	_bot.disarm()                     # AI 闸门（Rust）
	_turn_driver.round_reset()        # 同时回合制的收集器（Rust 侧）
	_fx_log.clear()
	if _effects != null:
		_effects.clear()
	_clear_curve()                    # 曲线 + tick 一起归零（幂等）

## 把 `_soldiers` 摊成"每玩家一张士兵表"，交给回合轮转（`docs/143` §6）。
##
## **归组在 Rust**（`GwTurnDriver.setup_flat`：入参是"每个士兵属于哪个玩家"这一列，
## 玩家顺序 = 首次出现顺序，与旧实现逐项一致）。本屏只剩"给数据、读结果"。
func _reset_turn_driver() -> void:
	var owners: Array = []
	for s in _soldiers:
		owners.append(int(s.player))
	_turn_driver.setup_flat(owners)
	_active_soldier_idx = _turn_driver.first_shooter(_alive_flags()) if not _soldiers.is_empty() else -1

## 存活表（按士兵下标）—— 交给 Rust 侧的轮转/收集器判活（`0x454950` 的 `& 1` 那一位）。
func _alive_flags() -> Array:
	var out: Array = []
	for s in _soldiers:
		out.append(bool(s.alive))
	return out

## 开始放一发：命中表已经算好，但**一条都没落地**（顺序回合走这条）。
## `already_settled = true`：压平模式（automation）与联机（本地不结算）。
func _begin_shot(shot: Dictionary, already_settled := false) -> void:
	_pending_shot = shot
	_judged = shot.get("judged", {})
	_destroyed_this_shot.clear()
	_shot_status = ""
	_shot_in_flight = true
	# 总点数取**画上去的那条**（`drawn`，撞障碍会被截断）—— 与 `_set_curve()` 设的
	# `_curve_total` 是同一个量；取 `points`（全量、未截断）会让"揭完"永远等不到。
	_flow.begin(
		(_pending_shot.get("drawn", _pending_shot.get("points", PackedVector2Array()))
			as PackedVector2Array).size(),
		_judged.get("kills", []),
		_judged.get("target_kills", []),
		_judged.get("stop", Vector2.ZERO),
		already_settled)
	# `SynchronizingResults --Activate--> ExecutingResults`（= 5）：弹道开始按 tick 执行。
	_match_state.apply("activate")

## 每帧推进：把"画到第几个点"递给 Rust，执行它吐回来的事件。
##
## 揭示点数**读 `Line2D`**（= 测量，留在视图层）；"该落哪些命中 / 算不算飞完"
## 是**判断**，在 `ShotLifecycle::advance`（`docs/16` §53）。
func _advance_presentation() -> void:
	if _pending_shot.is_empty():
		return
	if _plane == null:
		# 没有平面可读揭示点数 ⇒ 直接按"已揭完"推进（与原实现同一条兜底）。
		_apply_shot_events(_flow.flush())
		return
	_apply_shot_events(_flow.advance(_curve_reveal()))

## 执行 Rust 吐回来的一批事件：**只摆节点、只播声音，不再判断**。
##
## 三种事件 `soldier_killed` / `target_killed` / `arrived` 的形状见 `GwShotFlow`。
## "哪个点该落哪条命中"由 `ShotLifecycle::advance` 定（点序号语义 = 死亡 tick，
## `docs/65` §1）；这里只按 `kind` 分派。
func _apply_shot_events(events: Array) -> void:
	if events.is_empty():
		return
	var changed := false
	for e in events:
		match String(e.get("kind", "")):
			"soldier_killed":
				var idx := int(e.get("index", -1))
				if idx < 0 or idx >= _soldiers.size():
					continue
				var victim: Dictionary = _soldiers[idx]
				victim.alive = false
				changed = true
				# 死亡特效的坐标是**死者站位**（`docs/31` §3 逐位），不是弹道停点。
				_spawn_effect("DeathEffect", victim.pos)
				_spawn_effect("AshEffect", victim.pos)      # 推断（灰烬），见 GwEffectLayer 文件头
			"target_killed":
				var tid := int(e.get("target_id", -1))
				if not _targets.destroy(tid):
					continue
				_destroyed_this_shot.append(tid)
				changed = true
				# 靶子只有 `SparkEffect` 这一条线索（`docs/123` §90 是**造**靶子时发的），
				# 用在这里是**推断**：靶子被打掉与靶子出现共用同一个效果名。
				_spawn_effect("SparkEffect", e.get("pos", Vector2.ZERO))
			"arrived":
				_on_shot_arrived(String(_pending_shot.get("phase", "local")),
					e.get("stop", Vector2.ZERO))
	if changed:
		_refresh_plane()
		_refresh_targets()
		_note_shot_status()

## 弹道飞完：在**停止点**炸 → 清曲线 → 判回合（要不要结算问 `_flow.settled()`）。
## 进来时 Rust 已把剩余命中全部落完（`advance` 到达前会连越界的一起落地）。
func _on_shot_arrived(phase: String, stop: Vector2) -> void:
	if _pending_shot.is_empty():
		return
	# `EffectInfo(ExplosionEffect)` 的位置 = 弹道停止点（`docs/31` §2/§3 逐位），
	# 时机 = 弹道执行时（`docs/43` §1 的报文顺序里紧跟 `FunctionActive`）。
	# 联机不合成：那一条由**服务端**发过来（已经在 `EffectInfo` 分支里画了）。
	if phase != "online":
		_spawn_effect("ExplosionEffect", stop)
	_note_shot_status()
	# 打完就**清掉曲线**（`docs/143` §2：原版一发结算完不再留线，用户实测）。
	_clear_curve()
	_pending_shot = {}
	_shot_in_flight = false
	if phase == "online":
		# 联机：生死与回合全在服务端，本地**不判**胜负（`_check_match_over()` 也直接 return）。
		# 但服务端已经宣布收场（`_online_ending`）时，这一发落地就是本屏的最后一帧
		# ⇒ 停掉本屏自己的流程；结算框由 `App._on_net_ended()` 开（`docs/06` §77）。
		# `_online_ending` 以前**只被赋值、从不被读**，这里是它的读点。
		if _online_ending:
			_running = false
			_status("%s ／ 本局已结束（服务端 `EndingGame`）" % _shot_status)
		return
	if _flow.settled():
		# 压平模式：这一发在开火那一刻已经结算过了 ⇒ 这里只补"回合结束"这一条迁移。
		_match_state.apply("turn_ended")
		return
	_finish_round()
	# `ExecutingResults --TurnEnded--> WaitingForFunctions`（= 2）：弹道执行完、回合结束。
	# 放在 `_finish_round()` **之后**：`_settle_round()` 里的 `_check_match_over()` 可能已经
	# 把状态推到 `EndingGame`（`ExecutingResults --GameEnded--> EndingGame`），那时这条是空操作。
	_match_state.apply("turn_ended")

## 一个回合的收尾：回合轮转 → 回合结算（`_settle_round` 里含判胜负）。
## "这一发结算过没有"那个闩在 Rust（`ShotLifecycle::settle`）。
func _finish_round() -> void:
	_next_turn(true)
	_settle_round()

## 生成一条效果 + 播它那个槽位的音效（`docs/143` §4）。
func _spawn_effect(kind: String, pos: Vector2) -> void:
	if _effects != null:
		_effects.spawn(kind, pos)
	_fx_log.append({"kind": kind, "x": float(pos.x), "y": float(pos.y)})
	var sound := String(SFX_FOR_EFFECT.get(kind, ""))
	if sound == "":
		return
	var a := app()
	if a != null and a.audio != null:
		a.audio.play(sound, String(SLOT_FOR_EFFECT.get(kind, "")))

## 把在飞的那一发**直接跑完**（生产里由"弹道到点"触发；自检/诊断用它把时序走完）。
## 返回是否真的推进了一发。
func flush_shot() -> bool:
	if _pending_shot.is_empty():
		return false
	_advance_curve(1000000.0)          # 推到"揭完"那个 tick（Rust 侧夹住上限，不会越界）
	_apply_shot_events(_flow.flush())
	return true

## 自检入口：按**秒**推进弹道，走与 `_process` 完全相同的那条路（可复现）。
func advance_shot_for_test(seconds: float) -> void:
	if _plane == null:
		return
	_advance_curve(seconds)
	_advance_presentation()

# ---------------------------------------------------------------- AI 的回合（docs/143 §6）

## 每帧问一次"是不是该 AI 动手了"。
##
## 闸门本体在 Rust（`GwBotTurn` → `graphwar_play_screen_control::BotTurn`）：
## 思考倒计时、`can_fire` 的三个前置、"到点该开火"那个判断都不在 GDScript。
## 自动化运行里**不**让 AI 自动开火：自检用 `apply_external_shot()` 构造外部来弹
## （否则 AI 的弹道会插进断言中间，`docs/143` §6.5）。
func _tick_bot_turn(delta: float) -> void:
	if _fast_forward():
		return
	# `can_fire` 的判据是**状态机**（`Fire` 只从 `WaitingForFunctions` 出发），
	# 不再是"弹道在飞没有"那条投影 —— 两处在真机时序下等价，但只有前者的语义是原版的。
	if _bot.tick(delta, _running and _match_state.can_fire()):
		bot_take_turn()

## AI 回合：由**它自己**开一发。
##
## 旧实现根本没有这一步 —— 回合轮转到了 Bot，界面上什么都不发生，要等人类再按一次
## Fire 才会"动一下"（用户实测的"卡住"，`docs/143` §6.1）。返回是否真的开了火。
##
## 函数串由 Rust 给（`GwBotTurn.aim_expression`：一条过射手、指向最近敌人的直线，
## **不是原版 AI** —— 原版是 `aigen` 进化器，`docs/AI.md`/`docs/143` §6.4）。
func bot_take_turn() -> bool:
	if not _running or _online_mode or _replay_mode:
		return false
	if not _match_state.can_fire():
		return false            # 状态机不是 `WaitingForFunctions` ⇒ 这一发开不出去
	var shooter := _active_soldier()
	if shooter.is_empty() or not bool(shooter.alive) or not bool(shooter.get("is_bot", false)):
		return false
	_bot.disarm()
	var targets: Array = []
	for s in _soldiers:
		targets.append({"x": float(s.pos.x), "y": float(s.pos.y),
			"team": int(s.team), "alive": bool(s.alive)})
	var expr := _bot.aim_expression(shooter.pos, int(shooter.team), targets)
	var direction := 1.0 if int(shooter.team) == 0 else -1.0
	var result := GwCore.calculate_path(expr, shooter.pos, direction)
	if not bool(result.ok):
		_status("AI 的弹道算不出来：%s" % String(result.note))
		return false
	var pts: PackedVector2Array = result.points
	var drawn := _clip_to_obstacle(pts)
	_set_curve(drawn.points, shooter.get("color", GwThemeFactory.COL_ACCENT), int(shooter.player))
	_status("AI（%s）开火：f(x) = %s ／ **自定**的一发（原版 AI 是进化器，docs/143 §6.4）"
		% [String(shooter.get("name", "Bot")), expr])
	var shot := {"points": pts, "drawn": drawn.points, "team": int(shooter.team),
		"shooter": int(shooter.player), "soldier": shooter, "phase": "local"}
	_commit_shot(shot)
	return true

## 一次射击 / 一轮同时回合的**结算**：① 回合数 +1（Rust）；② "打不全就全部复原"；③ 判胜负。
##
## 回合数为什么在结算时才 +1：战役关 `TimeModeInfo = TimerInf`（`docs/123` §5.1 #9），
## 没有超时路径，一个回合只能由"打完一发"推进（`docs/131` §4.3）。
func _settle_round() -> void:
	if _replay_mode:
		return          # 回放的胜负由录像自己的 GameStateInfo 决定，不在这里判
	# 回合 +1 在 Rust（`ShotLifecycle::settle`，**同一发只收一次**）；这里只读结果。
	var outcome := _flow.settle()
	if not bool(outcome.get("settled", false)):
		return
	var restored := _apply_target_reset()
	var extra := ""
	if not restored.is_empty():
		extra = " ／ 本发没打全 ⇒ 靶子全部复原（推断，docs/131 §4）"
	_status("%s ／ 回合 %d/%s%s" % [_shot_status, int(outcome.get("turns_used", 0)),
		"∞" if _rules == null or _rules.turn_limit() <= 0 else str(_rules.turn_limit()), extra])
	_check_match_over()

## 一次射击结束时的"**打不全就全部复原**"（`docs/131` §4）。
##
## 等级：**推断（依据用户实测 + `docs/123` §3.2 的形状结论）**。二进制里没找到判定点，
## 语料 397 份回放 **0 次部分命中**（`docs/123` §4.3）⇒ 没有真值可以对。
## 形状是硬约束（`docs/123` §2.2/§4.3 逐位验证 + 观测）：**只翻 alive 位**，
## 不重建实体、不发 `TargetInfo`、不碰掩膜（洞留着）。
## 返回被复原的靶子 id。
func _apply_target_reset() -> Array:
	if _destroyed_this_shot.is_empty() or _targets.total() == 0:
		_destroyed_this_shot.clear()
		return []
	var plan := _targets.settle_shot(_destroyed_this_shot)
	_destroyed_this_shot.clear()
	if not bool(plan.get("reset", false)):
		return []
	var back := _targets.restore(plan.get("ids", []))
	if not back.is_empty():
		_refresh_targets()
	return back

## 弹道在障碍处**截断**（只为绘制）。返回
## `{points: PackedVector2Array, impact: Vector2, blocked: bool}`。
##
## 用内核的 `hit_test(points, [], obstacles)` 问"撞没撞、撞在第几点"——
## 判定本体是 `src/plane.rs` 的掩膜，与 `_resolve_shots` 里那次调用同一条规则；
## 两次调用只差"有没有士兵"，不会给出矛盾的撞点。
##
## ⚠️ 这里**不**改 `shot.points`（整条点序列）：
## 将来的 tick 动画要沿整条序列推进、到 `impact` 收尾，接口留在原处。
func _clip_to_obstacle(pts: PackedVector2Array) -> Dictionary:
	var fallback := {
		"points": pts,
		"impact": pts[pts.size() - 1] if pts.size() > 0 else Vector2.ZERO,
		"blocked": false,
	}
	if _obstacles.is_empty() or pts.size() < 2:
		return fallback
	var judged := GwCore.hit_test(pts, [], _obstacles)
	if not bool(judged.get("ok", false)):
		return fallback
	var first := int(judged.get("first_hit", -1))
	if not bool(judged.get("blocked", false)) or first < 0 or first + 1 > pts.size():
		return fallback
	var cut := pts.slice(0, first + 1)
	return {
		"points": cut,
		"impact": judged.get("stop", cut[cut.size() - 1]),
		"blocked": true,
	}

## 同时回合制：本回合**该交**的玩家 = 每个还有活士兵的玩家（`docs/86`：一回合一人一发）。
##
## 这只是一列数据（给 Rust 的 `round_open()` 当"公告顺序"）；"齐了没有"不在这里判。
func _simultaneous_announced() -> Array:
	var app_node := app()
	var out: Array = []
	if app_node == null:
		return out
	for pi in app_node.state.players.size():
		for s in _soldiers:
			if int(s.player) == int(pi) and bool(s.alive):
				out.append(int(pi))
				break
	return out

## 命中表（= 原版 **`SynchronizingResults`** 那一步的结果：服务端在开火那一刻就把谁死算完）。
##
## **判定本体在 Rust**：`GwShotFlow.judge()` → `src/graphwar_shot_calculator/settle.rs::resolve_points_owned`
## —— 与内核 `hit_test`、以及 `settle::resolve` **同一个函数**（`docs/120` 的逐位等价性测试钉住）。
## 掩膜规则（障碍决定停点、士兵/靶子都不挡弹道、点落进半径即死）、射手排除
## （`0x3c4452` 有弹道点 / `0x3c3e81` 空弹道，两处比的都是开火记录 `+0x18` 的射手 **id**）、
## 以及"第几点打中的"（`Hit.index`）全部由它给。
##
## 本函数只**递数据、收表**：`_soldiers` 原表（含 `alive`，死者由 Rust 排掉）、
## `GwTargetSet.entities()`、`_obstacles`；射手用**士兵下标**（-1 = 不排除任何人）。
## 旧实现里那张 `owners` 表与 `_first_point_within()` 的第二次半径扫描都删了 ——
## 它们是同一条规则的重复实现（`docs/06` §81）。
func _judge_shots(shots: Array) -> Dictionary:
	var rows: Array = []
	for shot in shots:
		rows.append({"points": shot.get("points", PackedVector2Array()),
			"shooter": _soldiers.find(shot.get("soldier", null))})
	return _flow.judge(rows, _soldier_rows(), _targets.entities(), _obstacles,
		GwCore.SOLDIER_HIT_RADIUS)

## 命中表 → 状态行（`_shot_status` 那一行**不显示**，只给自检/诊断，`docs/125` §5）。
func _note_shot_status() -> void:
	var sources: Array = _judged.get("sources", [])
	if sources.is_empty():
		return
	_shot_status = "本轮命中 %d 个士兵、%d 个靶子 ／ 判定来源：%s ／ %s%s" % [
		_flow.kills_landed(), _flow.targets_landed(), String(sources[0]),
		String(_judged.get("note", "")),
		"（弹道被障碍拦下）" if bool(_judged.get("blocked", false)) else ""]
	_status(_shot_status)

## 一次性结算一批弹道（同时回合、外部来弹、压平模式都走这条）。
## 与逐 tick 那条路径共用 `_judge_shots()` + Rust 的 `ShotLifecycle`：**判定只有一份**。
func _resolve_shots(shots: Array) -> void:
	_judged = _judge_shots(shots)
	_destroyed_this_shot.clear()
	# 这一批**没有单条弹道在飞**（同时回合攒齐 / 外部来弹）⇒ 命中一次全落地，
	# 既没有"到达"，也没有爆炸与清线（旧实现同口径，`docs/16` §53.6 登记过）。
	_flow.begin(0, _judged.get("kills", []), _judged.get("target_kills", []),
		_judged.get("stop", Vector2.ZERO), false)
	_apply_shot_events(_flow.land_all())
	_flow.abort()                     # 这一批没有单条弹道在飞 ⇒ 不收尾、不判到达
	# 整批也走一次 `Activate`（这一批没有逐 tick 的 `_begin_shot()`）：
	# `SynchronizingResults --Activate--> ExecutingResults`。之后由调用方换手 + 结算。
	_match_state.apply("activate")
	_note_shot_status()

## 靶子的绘制入口：真 `EntityDrawer` 里的实体表是**唯一**的画布来源，
## 所以"靶子被打掉 / 被复原"在界面上表现为那个实体被 `remove_entity` / 重新 `spawn`。
func _refresh_targets() -> void:
	_sync_entities()

func _active_soldier() -> Dictionary:
	var app_node := app()
	if app_node == null:
		return {}
	# 联机：射手是**服务端 `TurnInfo.soldier_id` 指的那个实体**，不是"这个玩家的第一个
	# 活着的士兵"（本地那条规则在联机里没有依据：服务端的游标在 `TurnManager` 里）。
	if _online_mode and _online_soldier >= 0:
		for s2 in _soldiers:
			if int(s2.get("entity_id", -1)) == _online_soldier:
				return s2
		return {}
	# 本地：射手由**Rust 的轮转**（`GwTurnDriver` → `PlayerTurns::next_shooter`，`0x463da0` 逐位）
	# 指定到具体士兵。旧实现是"该玩家的第一个活着的士兵"，多士兵关卡里第二个士兵永远轮不到
	# （docs/143 §6.2）—— 那个规则现在只有 Rust 一份。
	var idx := _turn_driver.current_shooter()
	if idx >= 0 and idx < _soldiers.size() and int(_soldiers[idx].player) == _active:
		_active_soldier_idx = idx
		return _soldiers[idx]
	# 兜底：游标还没跑过（**联机局**的 `_active` 来自服务端 `TurnInfo`，本地轮转不参与）。
	for s in _soldiers:
		if int(s.player) == _active:
			return s
	return {}

## 回合推进（**只在结算完成之后**调，`docs/143` §6）。
##
## 旧实现是 `(_active + 1) % n` 的**玩家**轮转 —— 两处不对：
## ① 多士兵关卡里那个玩家永远只用第一个士兵（`Level 3` 的描述明写
##    "At each turn you will shoot from a different one"，**观测**）；
## ② 轮到 Bot 之后什么都不发生，要人类再按一次 Fire 才"动一下"（用户实测的"卡住"）。
## 现在改用 `GwTurnDriver`（规则同 `src/graphwar_turn_manager.rs`，`0x463da0` 逐位），
## 轮到 AI 就给它一个思考倒计时（`_tick_bot_turn()`），到点由 AI 自己开火。
func _next_turn(advance: bool) -> void:
	if not advance:
		return
	var app_node := app()
	if app_node == null:
		return
	# 联机：换手是**服务端 `TurnInfo`** 的事（`_apply_online_turn()`），本地不许抢。
	if _online_mode:
		return
	if app_node.state.players.size() <= 0 or _soldiers.is_empty():
		return
	# 轮转本体在 Rust（`0x463f40`–`0x463fca`：先自增再取、判活、最多试 len 次）。
	var idx := _turn_driver.next_shooter(_alive_flags())
	if idx >= 0 and idx < _soldiers.size():
		_active_soldier_idx = idx
		_active = int(_soldiers[idx].player)
	# 回合时钟归零（Rust：`GwTurnClock.restart`），AI 闸门先取消。
	_turn_clock.restart()
	_bot.disarm()
	var shooter := _active_soldier()
	if not shooter.is_empty() and bool(shooter.get("is_bot", false)):
		_bot.arm()
	_load_active_function()
	_update_timer_label()
	_refresh_function_list()

func _load_active_function() -> void:
	if _input == null:
		return
	_input.text = String(_functions.get(_active, ""))
	# 换手 = 换射手 ⇒ 修正常数也要重算（原版换手时右下那串**不消失**，它跟着"当前输入 + 当前射手"变）。
	_refresh_offset_label()

## 判一局结束没有 —— **按关卡类型**（`docs/131` §2），不再只看两队士兵的存活数。
##
## 旧写法（`alive = {0:n, 1:m}`，任一为 0 就结束）在**打靶关**上是必然误判：
## 那些关没有敌方士兵 ⇒ `alive[1] == 0` ⇒ 第一发一落地就"人类胜"（`docs/131` §1）。
## 现在：打靶关要**靶子全清**，对战关要**对方队伍全灭**（`docs/84`：45/45 实测），
## 回合上限用尽仍未清完 ⇒ 失败（`docs/118` §7.1，`Level 7` = 1）。
func _check_match_over() -> void:
	if _replay_mode:
		return
	# 联机：胜负由**服务端**判（它发 `EndingGame`，`src/graphwar_game_server.rs`；本界面只收）。
	# 本地这套 `GwLevelRules` 是"单机/战役"的规则，拿去判联机局会出现两个真相源。
	if _online_mode:
		return
	var app_node := app()
	if app_node == null:
		return
	if _rules == null:
		_rules = GwLevelRules.new()
		_rules.from_versus(_human_team())
	# 入参是**实体表**，不是"已经数好的两队人数"：`0x473520 get_alive_teams` 的原版行为就是
	# 遍历生命表 → `0x454950` 判活 → `0x3c2120` 查队伍，只数"有没有"（`docs/150` §2.1）。
	# 旧实现里的 `_alive_by_team()` / `_teams_present()` 两个循环是同一条规则的第二次实现。
	var verdict := _rules.evaluate_soldiers(_soldier_rows(), _targets.total(),
		_targets.alive_count(), _flow.turns_used())
	_last_verdict = verdict
	if not bool(verdict.get("over", false)):
		return
	# 状态机：`ExecutingResults --GameEnded--> EndingGame`（= 6；`0x475bac` 那条
	# `mov BYTE [self+0x120], 6` 就是收场写点）。只从 `ExecutingResults` 出发 ⇒
	# 在 `WaitingForFunctions` 时施加会被忽略（迁移表的 `None`），那正是原版的行为。
	_match_state.apply("game_ended")
	_running = false
	var human_won := bool(verdict.get("human_won", false))
	var winner := int(verdict.get("winner_team", _human_team()))
	_status("%s ／ 判定：%s（%s）" % [_status_text, "人类胜" if human_won else "人类负",
		String(verdict.get("reason", "?"))])
	app_node.report_match_result(human_won, winner)

## `_soldiers` 的**无类型副本**：扩展方法收的是无类型 `Array`，而 `_soldiers` 是
## `Array[Dictionary]`（godot-rust 在边界上拒绝"有类型数组塞进无类型形参"，
## 0.5.5 `array.rs:1225` 的 `copied array should have same type`）。
func _soldier_rows() -> Array:
	var rows: Array = []
	rows.assign(_soldiers)
	return rows

## 人类玩家所在队伍。与 `app.gd::match_summary()` 用同一条规则（第一个非 Bot 玩家），
## 这样"谁赢了"与结算框的文案永远一致。
##
## 联机分支：队伍取**我自己那个玩家**的队（`PlayerInfo` → 实体上的 `team`），
## 而不是"state 里的第一个非 Bot" —— 那个人可能是对手（`docs/139` §4）。
func _human_team() -> int:
	if _online_mode and _my_player_id >= 0:
		for s in _soldiers:
			if int(s.get("player_id", -1)) == _my_player_id:
				return int(s.get("team", 0))
	var app_node := app()
	if app_node == null:
		return 0
	for p in app_node.state.players:
		if not bool(p.is_bot):
			return int(p.team)
	return 0

## 关卡规则的**只读**快照（自检 / `--dump` 诊断用，不改任何状态）。
func level_state() -> Dictionary:
	var shooter := _active_soldier()
	return {
		"running": _running,
		"rules": (_rules.snapshot() if _rules != null else {}),
		"targets": _targets.snapshot(),
		"turns_used": _flow.turns_used(),
		"verdict": (_last_verdict.duplicate(true) if not _last_verdict.is_empty() else {}),
		"shot_status": _shot_status,
		"obstacles": _obstacles.size(),
		# --- 弹道生命周期（docs/143）：这些字段只读，不参与逻辑，给自检/诊断看 ---
		# **全部来自 Rust**（`GwShotFlow.snapshot()` 与它的分项读数）。
		"in_flight": _shot_in_flight,
		"logic_settled": _flow.settled(),
		"pending_kills": _flow.pending_kills(),
		"pending_target_kills": _flow.pending_targets(),
		"shot_points": (_pending_shot.get("points", PackedVector2Array()) as PackedVector2Array).size(),
		"soldiers_landed": _flow.kills_landed(),
		"targets_landed": _flow.targets_landed(),
		"lifecycle": _flow.snapshot(),
		"turn_clock": _turn_clock.snapshot(),
		"effects": (_effects.count() if _effects != null else -1),
		"effects_spawned": (_effects.spawned() if _effects != null else {}),
		"fx_log": _fx_log.duplicate(true),
		"active_player": _active,
		"active_soldier": ({} if shooter.is_empty() else {
			"player": int(shooter.get("player", -1)),
			"is_bot": bool(shooter.get("is_bot", false)),
			"x": float(shooter.pos.x), "y": float(shooter.pos.y),
			"alive": bool(shooter.get("alive", false)),
		}),
		"soldiers": _soldiers.size(),
		"alive_soldiers": _alive_soldier_count(),
		"turn_driver": _turn_driver.snapshot(),
		# --- 对局状态机（T31）：**来自 Rust**（`GwTurnState.snapshot()`），只读 ---
		"game_state": _match_state.snapshot(),
		"bot_think_left": _bot.think_left(),
		"status": _status_text,
		"path_points": _curve_reveal(),
	}

## 自检用：把**随机**布局钉住。原版靶子与障碍都是每局随机生成的
## （`docs/118` §8 #16："本来就没有每关固定坐标"），所以这不是篡改关卡数据，
## 只是把随机量固定下来好让端到端用例可复现。**在 `begin_match()` 之前调用。**
##
## `layout` 可含 `targets`（`Array[Vector2]`）与 `obstacles`（`Array[{pos, size}]`），
## 缺哪个就不覆盖哪个。
func inject_layout(layout: Dictionary) -> void:
	_layout_override = layout.duplicate(true)

## 清掉注入的布局（自检结束时调，正常游戏里永远是空的）。
func clear_layout_override() -> void:
	_layout_override = {}

## **外部来弹**：把"别人打过来的一发"送进**同一条**结算路径。
##
## `animate = false`（默认，既有签名）：立刻结算 —— 自检用它构造"我方全灭 ⇒ 失败"
## （`docs/131` §5.2 ③）。
## `animate = true`：与人类开火一样先放弹道、飞完再结算（`docs/143` §3）。
## 游戏里"别人打过来的一发"应当走 `animate = true`；AI 的回合现在走 `bot_take_turn()`。
func apply_external_shot(points: PackedVector2Array, shooter_player: int,
		animate := false) -> bool:
	if not _running or points.size() < 2:
		return false
	var shooter := _soldier_of(shooter_player)
	if shooter.is_empty():
		return false
	if animate:
		var drawn := _clip_to_obstacle(points)
		_set_curve(drawn.points, shooter.get("color", GwThemeFactory.COL_ACCENT), shooter_player)
		var shot := {"points": points, "drawn": drawn.points, "team": int(shooter.team),
			"shooter": shooter_player, "soldier": shooter, "phase": "local"}
		# 外部来弹同样是"别人的一次 Fire" ⇒ 同一条迁移链（这里没有 `_commit_shot()`）。
		_match_state.apply("fire")
		shot["judged"] = _judge_shots([shot])
		_match_state.apply("results_computed")
		_begin_shot(shot)
		return true
	_resolve_shots([{
		"points": points, "team": int(shooter.team),
		"shooter": shooter_player, "soldier": shooter,
	}])
	_next_turn(true)
	_settle_round()
	_match_state.apply("turn_ended")   # 整批结算完 ⇒ 回 WaitingForFunctions
	return true

## 某个玩家的（第一个）还活着的士兵；没有就返回空字典。
func _soldier_of(player_index: int) -> Dictionary:
	for s in _soldiers:
		if int(s.player) == player_index and bool(s.alive):
			return s
	return {}

# ---------------------------------------------------------------- 回放模式（docs/133）

## 进入回放模式：把一局真实回放接到**现有的绘制与结算路径**上。
##
## 与原版的对应：原版这一块是 `graphwar_replay_player.rs` + `graphwar_replay_control.rs`
## 两个 Rust 模块（`docs/78` §1 从 `.so` 里读到的模块名），界面是 `res://replay_lobby.tscn`。
## 本重建的 `app.gd` 只负责 `state.reset_replay(path)` 然后调 `begin_match()`，
## 所以**分叉点在这里** —— 这也是本文件里唯一为回放新增的分支（`docs/133` §4.1）。
##
## **流程（tick 时钟 + 事件泵 + 结算批）在 Rust**：`GwReplayDriver`
## → `src/_scaffold/replay_driver.rs`（`docs/16` §56）。本函数只剩"递路径 / 读快照 / 摆节点"：
## 旧实现在这里建一份 426 行的 GDScript 局面机（`GwReplayPlayer`，89 号整份删除），再每帧 `advance_dt(dt)`
## —— 那是**每帧泵一次**，与原版量到的 `pumps ≡ ticks`（`docs/06` §44.3）不符。
##
## 喂给绘制/结算的东西一律**不绕过内核**：
## * 弹道：回放只给 `FunctionPoints{start_x, start_y, end_x}`（**没有点序列**），
##   所以用 `GwCore.calculate_path(函数文本, 起点, 方向)` 重算 —— 那条路径就是
##   内核的采样器（`docs/116`）；算不出来就**不画**并说明原因，不拿预览折线冒充；
## * 障碍：`AddObstacle` 的精确坐标/半径直接进 `_obstacles`（画与判定同一份数组）；
## * 生死：`LifeInfo{alive:false}` 直接翻 `_soldiers[i].alive`；
## * 位置：`PosInfo` 直接写坐标。
func _begin_replay(path: String) -> void:
	_replay_path = path
	_replay = null
	_replay_mode = false
	_running = false
	_replay_shot_status = ""
	_functions.clear()
	_shot_queue.clear()
	_last_result = {}
	_soldiers.clear()
	_targets = GwTargetSet.new()
	_destroyed_this_shot.clear()
	_last_verdict = {}
	_reset_shot_state()
	_clear_curve()
	_set_replay_input_enabled(false)
	_replay_left.visible = true
	_replay_right.visible = true
	_replay_bar.visible = true

	var drv := GwReplayDriver.new()
	# `user://` / `res://` 是 **Godot** 的路径方案，内核只认真路径 ⇒ 换算留在视图层
	# （`docs/16` §56.5 第 3 条）。
	if not drv.load(ProjectSettings.globalize_path(path)):
		_status("回放打不开：%s" % String(drv.error()))
		# `app.gd::_play_replay()` 在调 `begin_match()` **之前**先弹了一句
		# `回放播放未实现`（那是它还没接回放时的占位）。这里用一句准确的覆盖掉它 ——
		# 否则用户会一边看着回放在播、一边读到"未实现"。气泡文本不能含内部标记
		# （`app.gd::INTERNAL_MARKERS`），所以只写事实。
		notify("回放打不开：%s" % String(drv.error()))
		_replay_label.text = "回放读取失败：%s" % String(drv.error())
		_replay_progress.value = 0.0
		_running = false
		return
	_replay = drv
	_replay_mode = true
	_running = true
	var snap: Dictionary = _replay.snapshot()
	# 四个模式：回放里记着**这一局真实的选择**（`AxisModeInfo` 等）。字符串 → 档位下标的
	# 对照表在 **Rust 侧**（`GwReplayDriver` 的 4 个**静态**查表函数，纯查表不是流程）：
	# 源表是 `src/_scaffold/protocol.rs::WIRE_VALUE_ENUMS`（`docs/07` §5.2，**逐位**），
	# 下标 = 线上枚举的声明顺序。GDScript 里**没有**第二份表，本屏只问一句
	# （`docs/16-client.md` §59；84 号那份 `.gd` 查表已在 89 号整份删除）。
	var app_node := app()
	if app_node != null:
		app_node.state.axis_mode = GwReplayDriver.axis_index(String(snap.axis_mode))
		app_node.state.function_mode = GwReplayDriver.function_index(String(snap.function_mode))
		app_node.state.turn_mode = GwReplayDriver.turn_index(String(snap.turn_mode))
		app_node.state.time_mode = GwReplayDriver.time_index(String(snap.time_mode))
	_set_axis_mode(int(app_node.state.axis_mode) if app_node != null else GwModes.AXIS_DEFAULT)
	_refresh_mode_indicators()
	# 回放不做胜负判定（录像自己会发 `GameStateInfo{EndingGame}`），这里只占位。
	_rules = GwLevelRules.new()
	_rules.from_versus(0)
	# ---- 载入即**暂停**（原版行为，`docs/06` §53.4 ④ 观测：进屏 45 s 只变 270 px）----
	#
	# 两件事：
	# ① **不调 `play()`** —— `ReplayPlayer::new` 的 `playing` 本来就是 false，
	#    以前是本函数末尾一句 `_replay.play()` 把它打开的（我方自定交互）。
	# ② 时钟放到**载入态**那个 tick：`ReplayDriver::with_replay` 停的是 `first_tick`（=1），
	#    而 `PosInfo` 全部在 tick 6 ⇒ tick 1 时 `actors()` 是空的（士兵/靶子一个都没有）。
	#    真帧上原版载入态**有**士兵、靶子、名字标签，且底部函数文本 = 第 1 发
	#    （`x+2sin(20x)` / `- 3.694`，tick 550 的 `FunctionInfo`），而**没有弹道**
	#    （`FunctionActive.active_tick` = 615）⇒ 载入 tick ∈ [550, 615)。取"第 1 个结算批的
	#    `exec_tick`"（本样本 = 558）是**推断**，缺 frida 冻 tick 的证据（§53.5 第 2 条）。
	var load_tick := _replay_load_tick()
	if load_tick != int(_replay.tick()):
		_replay.seek(load_tick)
	_replay.pause()
	_realize_replay_state()
	_set_replay_layout(true)
	_replay_sync_bottom_text()
	_update_replay_ui()
	_status(_replay_status_text())
	# **不弹气泡**：原版进回放屏什么都不弹（真帧底部只有 `f(x) =` 那一行）；以前这句
	# 自造的 `notify()` 会在底部中央留一条 5 s 的橙色提示（实测 6117 px 的白/橙像素），
	# 属于"多出来的元素"。诊断信息仍走隐藏的 `_status()`（`_status_label` 永不显示）。

## 载入态那个 tick（见 `_begin_replay()` 的注释）。
##
## 有结算批 ⇒ 取第 1 批的 `exec_tick`（"第一批执行完"这一拍：布局/外观/位置都已落地，
## 弹道还没开始揭示）；切不出批 ⇒ 退回 `first_tick`（不假装知道）。
func _replay_load_tick() -> int:
	if _replay == null:
		return 1
	var fits: Array = _replay.batches()
	if fits.is_empty():
		return int(_replay.first_tick())
	var b: Dictionary = fits[0]
	return int(b.get("exec_tick", b.get("start_tick", _replay.first_tick())))

## 底部那一行：**大号函数文本**（原版回放屏底部中间那一串）+ 修正常数（右下角）。
##
## 文本取**这一发**的函数文本（`last_shot()["function"]`，本样本 `x+2sin(20x)`）；
## 不取 `corrected`：真帧上 `- 3.694` 是**另起一列**（OffsetContainer，x1764..1883），
## 函数串里没有它（`fixtures/replay_screen_targets.txt` 的 `func_text_bbox` / `offset_text_bbox`）。
## 光标停在末尾 —— 原版载入态那个 270 px 的变化就是它（§53.4 ④：bbox x1182–1184）。
func _replay_sync_bottom_text() -> void:
	if _input == null:
		return
	var shot: Dictionary = _replay.last_shot() if _replay != null else {}
	var text := String(shot.get("function", ""))
	if text == "" and _replay != null:
		text = String(_replay.function_of(int(shot.get("owner", -1))))
	if _input.text != text:
		_input.text = text
	_input.key_input(GwCommands.KEY_END)      # 光标到末尾（原版载入态光标就在末尾）

## 主循环里的回放推进（`_process` 的回放分支）。
##
## 一行调用：**推进在 Rust**（定步累加器 + 每 tick 一次泵，`docs/16` §56）。
func _replay_tick(delta: float) -> void:
	if _replay == null:
		return
	var report: Dictionary = _replay.advance_dt(delta)
	_apply_replay_events(report)
	_replay_advance_curve()
	_update_replay_ui()

## 执行一次驱动报告：**只看 Rust 给的"该重画哪层"三个标志摆节点，不再看事件名**。
##
## 旧实现自己遍历事件数组、按 `kind` 决定要不要重摆实体/障碍/弹道（24 行 match）。
## 那张表现在在 `ReplayDriver::mark_dirty()`（`docs/16` §56.4）——"事件类型 → 视图动作"
## 本身**不是**游戏流程，但它与"谁消费事件"绑在一起，绑在 Rust 里就不会再出现
## "GDScript 也有一份消费顺序"这种双份真理。
func _apply_replay_events(report: Dictionary) -> void:
	if bool(report.get("dirty_layout", false)):
		_replay_sync_layout()
	if bool(report.get("dirty_actors", false)):
		_replay_sync_actors()
	if bool(report.get("dirty_shot", false)):
		_replay_draw_last_shot()
	# 结算批落地（`ExecutingResults` 那一拍）：**只更新状态行，不动节点** ——
	# 生死/特效是延迟段里那些 `LifeInfo` 事件带来的（`docs/136` §2 的批窗口形状）。
	# 长帧一次跨多批时 `settled` 里会有多个下标（`docs/06` §44.4 的追赶），按序记下即可。
	var settled: Array = report.get("settled", [])
	if int(report.get("events", 0)) > 0 or not settled.is_empty():
		_status(_replay_status_text())

## 从**当前局面**整体重建画面（跳转/进入时用；逐事件路径只做增量）。
func _realize_replay_state() -> void:
	if _replay == null:
		return
	_replay_sync_layout()
	_replay_sync_actors()
	_replay_draw_last_shot()
	_update_replay_ui()

## 画"当前局面里最近的一发"。
##
## 回放里没有点序列（`FunctionPoints` 只有 `start_x/start_y/end_x`），所以这条弹道只能
## **用内核按（起点 + 函数文本 + 队伍方向）重算** —— 那就是唯一的真值路径；算不出来
## （例如微分方程局，`GwCore.calculate_path` 只跑 `Mode::Normal`）就清空并说明原因，
## 不拿界面预览折线冒充（`docs/133` §4.5）。
func _replay_draw_last_shot() -> void:
	var shot: Dictionary = _replay.last_shot() if _replay != null else {}
	if shot.is_empty():
		_clear_curve()
		_replay_shot_status = "还没有开火记录"
		return
	_replay_draw_shot(shot)

func _replay_sync_layout() -> void:
	_obstacles = _replay.obstacles()
	_set_obstacles(_obstacles)

## 士兵与靶子：**同一个局面对象**既给 `_plane` 画，也给"谁死了"用。
func _replay_sync_actors() -> void:
	var list: Array[Dictionary] = []
	for a in _replay.actors():
		if bool(a.is_target):
			continue            # 靶子走 `set_targets()`（与原版一样是独立图层，docs/131）
		list.append({
			# ⚠️ **回放的实体 id**（本样本士兵 = 230）。以前这里没带出去，`_sync_entities()`
			# 拿**下标**（= 0）去问 `_replay.graphics("skin", 0)` ⇒ 永远空串 ⇒ 士兵一直用
			# `spawn_entity` 的默认三件套（yellow/regular_eyes/soldier_helmet），而回放录下来的
			# 是 `face_paint_2/asian/asian_hat`。真帧对拍差的是**整个精灵**（改前士兵彩色
			# 521 px / 26×26，原版 1811 px / 73×72）。
			"id": int(a.id),
			"name": String(a.name),
			"pos": a.pos,
			"alive": bool(a.alive),
			"team": maxi(0, int(a.team)),
			# 回放**没说**谁是 bot（`NameInfo` 只给名字，`docs/79` §1 说完这一步没做完）
			# ⇒ 一律按真人画，皮肤从 `SkinGraphicsInfo` 的字符串反查下标（查不到用 0）。
			"is_bot": false,
			"bot_tier": 0,
			"skin": _replay_skin_index(int(a.id)),
			"color": _replay_color(a),
			"player": 0,
		})
	_soldiers = list
	_sync_entities()

## `SkinGraphicsInfo.graphics_str`（如 `face_paint_2`）→ `GwIcons.SKINS` 的下标。
## 名字对不上就退回 0（不编造皮肤）。**观测**：这三类字符串就是原版资产的文件名
## （`rsc/skins/*.svg`、`faces/*.svg`、`hats/*.svg`，`docs/121` §5）。
func _replay_skin_index(entity_id: int) -> int:
	if _replay == null:
		return 0
	var nm := String(_replay.graphics("skin", entity_id))
	if nm == "":
		return 0
	return maxi(0, GwIcons.SKINS.find("skins/" + nm))

## 玩家颜色：优先用回放里 `ColorInfo` 的**真实颜色**（`docs/102` 的队色表就是这样解出来的），
## 没有就退回本队色表第一色。
func _replay_color(a: Dictionary) -> Color:
	var c = a.get("color", null)
	if c is Color:
		return c
	var team := clampi(int(a.get("team", 0)), 0, 1)
	return TEAM_COLORS[team][0]

## 画一发的弹道：**用内核按（起点 + 函数文本 + 队伍方向）重算**。
##
## 回放里没有点序列（`FunctionPoints` 只有 `start_x/start_y/end_x`），所以这是唯一
## 不编造的做法；内核算不出来（例如微分方程局——`GwCore.calculate_path` 只跑
## `Mode::Normal`，见 `docs/116` §6）就清空并说明，不画假线。
func _replay_draw_shot(d: Dictionary) -> void:
	if _replay == null:
		return
	var owner := int(d.get("owner", -1))
	var start: Vector2 = d.get("start", Vector2.ZERO)
	var expr := String(d.get("function", ""))
	if expr == "":
		expr = String(_replay.function_of(owner))
	if expr == "":
		_clear_curve()
		_replay_shot_status = "这一发没有函数文本（owner %d）" % owner
		return
	var team := int(_replay.team_index(owner))
	var direction := 1.0
	if team == 1:
		direction = -1.0
	elif team < 0:
		# 队伍未知才退回几何：起点在左就往右（`docs/105`：Team1 恒在 x<0）
		direction = 1.0 if start.x < 0.0 else -1.0
	# 模式**取自这一局的录像**（`FunctionModeInfo`），不是界面上的开关：
	# `DiffEqFunction` 的 `f` 是 `dy/dx`，必须走 ODE（一阶 RK4），否则画出来是错的线
	# —— 契约与实测见 `docs/138`（内核 `path_calculate_mode`，`mode` 是字符串
	# `"normal"` / `"ode"`；`GwCore.calculate_path()` 的第 4 个参数就是它）。
	var snap: Dictionary = _replay.snapshot()
	var mode := "ode" if String(snap.function_mode) == "DiffEqFunction" else "normal"
	var result := GwCore.calculate_path(expr, start, direction, mode)
	var replay_corrected := String(d.get("corrected", ""))
	if not bool(result.ok):
		_clear_curve()
		_replay_shot_status = "内核算不出这一发（%s）／ 函数 %s" % [String(result.note), expr]
		return
	var drawn := _clip_to_obstacle(result.points)
	# 颜色：`ColorInfo` 是按**玩家**记的，函数表的 owner 是**士兵** ⇒ 先过一次
	# `soldier_owner`（`SoldierInfo` 那张全局表）。没有 ColorInfo 就退回本队色表。
	var pid := int(_replay.soldier_owner(owner))
	var col: Color = TEAM_COLORS[clampi(maxi(0, team), 0, 1)][0]
	if _replay.has_color(pid):
		col = _replay.color_of(pid)
	_set_curve(drawn.points, col, owner)
	# 修正常数：只有 Normal 有。ODE 下内核给的是 `NaN` ⇒ `format_offset()` 出空串，
	# 正好是原版 DiffEq 局右下角不显示修正文本的样子（`docs/92` §4.10、`docs/138`）。
	# 修正常数文本的前导空格由 `_offset_display()` 给（= 内核 `correction_text` 的格式
	# `" - {:.3}"`），这里**不再**手工加第二个。
	#
	# ⚠️ **2026-11-26（T29）更正**：上一轮在这里记的是"加一个前导空格（居中 ⇒ 左移 7 px）
	# 后与真帧差 1.9 px，档位推断"。**不是**——`Label` 还该是 **LEFT 对齐**（原版 `.tscn`
	# 没有写 `horizontal_alignment` ⇒ Label 默认 LEFT）。四个候选一起算只有
	# "LEFT + 前导空格" 落在真帧的 1764（其余 1750 / 1773 / 1780），改后整串墨迹
	# `(1764,1100)-(1883,1125)`、1200 ink px，与原版**逐像素相同**（`docs/06` §79.4）。
	_offset_label.text = _offset_display(float(result.offset))
	var mode_name := "Ode(dy/dx)" if mode == "ode" else "Normal(y=f(x))"
	_replay_shot_status = "内核重算（%s）：函数 %s ／ 修正 %s（回放记录：%s）／ %d 点%s" % [
		mode_name, expr, GwCore.format_offset(float(result.offset)),
		replay_corrected if replay_corrected != "" else "—",
		drawn.points.size(),
		"（撞障碍截断）" if bool(drawn.blocked) else "",
	]

## 状态行（只在隐藏的 StatusLabel 上，不显示给用户；`docs/125` §5）。
func _replay_status_text() -> String:
	if _replay == null:
		return "回放未载入"
	var snap: Dictionary = _replay.snapshot()
	var dead := 0
	var total := 0
	for a in _replay.actors():
		total += 1
		if not bool(a.alive):
			dead += 1
	return "回放 %s ／ %s ／ 状态 %s ／ 回合 %d ／ 实体 %d（阵亡 %d）／ %s ／ 批 %d/%d（%s）／ %s" % [
		_replay_path.get_file(), String(snap.game_name), String(snap.game_state),
		int(snap.turn_count), total, dead, String(snap.summary),
		int(snap.batches_settled), int(snap.batches), String(snap.phase), _replay_shot_status]

## 进度条 / tick 标签刷新。
func _update_replay_ui() -> void:
	if _replay == null or _replay_progress == null:
		return
	_replay_progress.value = _replay.progress()
	var snap: Dictionary = _replay.snapshot()
	_replay_label.text = "回放 %s ／ %s ／ %s" % [
		String(snap.game_name), String(snap.summary),
		"（已到结尾）" if bool(snap.finished) else "",
	]
	_update_replay_pause_icon()

## 左侧三颗播放键（prev / play_pause / next）。**我们自定的接线**，见 `docs/133` §4.3。
##
## 播放控制也在 Rust（`ReplayDriver::toggle/step_event/step_back_event`）：
## 跳转 = 从头重放（保证局面恒等于 `state_at(tick)`，不留增量漂移）。
func _on_replay_button(cmd: String) -> void:
	if not _replay_mode or _replay == null:
		_status("当前不是回放模式")
		return
	match cmd:
		GwCommands.PLAY_PAUSE:
			_replay.toggle()
		GwCommands.KEY_PREV:
			_replay.pause()          # 步进 = 停在那一拍（原版 pause 图标 stays ▶）
			_replay.step_back_event()
			_realize_replay_state()
		GwCommands.KEY_NEXT:
			_replay.pause()
			_replay.step_event()
			_realize_replay_state()
	_replay_sync_bottom_text()
	_update_replay_ui()

## 回放模式下把输入类控件**显式禁用**（而不是"按了没反应"）。
func _set_replay_input_enabled(enabled: bool) -> void:
	for b in _keys:
		if b == null:
			continue
		# YButton(y) 在原版**两种模式下都恒为 disabled**（docs/92 §4.10 观测），
		# 恢复输入时不能把它一起开回来（自检有这条断言）。
		if String(b.name) == "YButton":
			continue
		b.disabled = not enabled
	if _fire_button != null:
		_fire_button.disabled = not enabled
	if _input != null and not enabled:
		# `GwFunctionInput` 不是 LineEdit（自绘，没有 `editable`），禁用的做法是清空 +
		# `key_input()`/`fire()` 两个入口直接 return。
		_input.text = ""

# ---- 自检 / 诊断用的只读入口（不参与游戏逻辑） ----

## 当前回放驱动器的**规范性局面串**（与 Kernel / Python 参考同格式，`docs/133` §5）。
func replay_canonical() -> String:
	return String(_replay.canonical()) if _replay != null else ""

func replay_state() -> Dictionary:
	if _replay == null:
		return {}
	var snap: Dictionary = _replay.snapshot()
	return {
		"mode": _replay_mode,
		"tick": int(snap.tick),
		"first_tick": int(snap.first_tick),
		"last_tick": int(snap.last_tick),
		"cursor": int(snap.cursor),
		"events": int(snap.events),
		"playing": bool(snap.playing),
		"progress": float(snap.progress),
		"game_name": String(snap.game_name),
		"game_state": String(snap.game_state),
		"turn_count": int(snap.turn_count),
		"obstacles": int(snap.obstacles),
		"soldiers": _soldiers.size(),
		"canonical": String(snap.canonical),
		"shot_status": _replay_shot_status,
		# 载入态 / 弹道揭示（`docs/06` §55）：`active_tick` 来自 `FunctionActive`，
		# `curve_tick` / `curve_reveal` 是"这条弹道已经揭到第几 tick / 几个点"
		# （回放里由 tick 推，暂停时不动 —— 与墙钟无关）。
		"active_tick": int(snap.get("active_tick", -1)),
		"curve_tick": _curve_tick,
		"curve_total": _curve_total,
		"curve_reveal": _curve_reveal(),
		"layout": _replay_layout,
		# 新增（`docs/16` §56）：驱动侧的事实，给自检与状态行看。
		"phase": String(_replay.phase()),
		"batches": int(snap.batches),
		"batches_settled": int(snap.batches_settled),
		"batch_note": String(snap.batch_note),
		"stats": snap.stats,
	}

## 自检用：把回放**推进** n 秒墙钟（等价于主循环跑 n 秒）。返回本次消费的事件条数。
func replay_advance_for_test(seconds: float) -> int:
	if _replay == null:
		return 0
	var report: Dictionary = _replay.advance_dt(seconds)
	_apply_replay_events(report)
	_update_replay_ui()
	return int(report.get("events", 0))

## 自检用：直接跳到某个 tick（走与按键跳转**同一条**路径）。
func replay_seek_for_test(target_tick: int) -> void:
	if _replay == null:
		return
	_replay.seek(target_tick)
	_realize_replay_state()

## 自检用：按一下播放键（走 `_on_replay_button`）。
func replay_press_for_test(cmd: String) -> void:
	_on_replay_button(cmd)

# ---------------------------------------------------------------- 联机模式（docs/139）
#
# `docs/134` §7.1 留下的交接清单，本节逐条落实（编号沿用那份清单）：
#
# | # | 清单 | 本文件的落点 |
# |---|---|---|
# | ① | 士兵/障碍来源 | `_build_online_soldiers()` / `_build_online_obstacles()` —— **不调** `_spawn_position()` |
# | ② | 轮到谁 | `_apply_online_turn()`（`TurnInfo`）+ `_active_soldier()` 的联机分支 |
# | ③ | 开火 | `_fire_online()` → `session.submit_function()` + `session.fire()`；弹道在 `_draw_online_shot()` 复算 |
# | ④ | 随事件刷屏 | `_on_online_message()`（`PosInfo`/`LifeInfo`/`EffectInfo`/`TurnInfo`/`EndOfTurnInfo`/`FunctionPoints`/…） |
#
# 谁是"我"：`session.my_player_id`（服务端 `PlayerInfo` 给的 —— 本仓库已补，`docs/139` §4）。
# 拿不到时（旧服务端 / 自检里的假服务器）降级成"按 `TurnInfo` 里的玩家假定"，
# 并在状态行里写明"不知道我是谁"。

## 联机开局：把**服务端那一份**战场搬进界面。
##
## `App._on_net_joined()` 与 `App._on_net_game_started()` 都会调 `begin_match()`，
## 所以本函数必须**幂等**：每次都从 `room` 整体重建，不做增量。
func _begin_online(app_node) -> void:
	_online_mode = true
	_online_subscribe(app_node.net.session if app_node.net != null else null)
	var room := _online_room()
	_my_player_id = int(_online_session.my_player_id) if _online_session != null else -1

	_functions.clear()
	_shot_queue.clear()
	_last_result = {}
	_soldiers.clear()
	_targets = GwTargetSet.new()
	_target_entity_ids.clear()
	_destroyed_this_shot.clear()
	_last_verdict = {}
	_reset_shot_state()
	_time_left = 0.0
	_clear_curve()
	_sync_entities()                  # 服务端的 `Match` 里只有士兵，没有靶子（docs/117）
	_rebuild_pid_index(room)
	_build_online_soldiers(room, app_node)
	_build_online_obstacles(room)
	# 靶子图层：**原版是独立图层**（`docs/131`），而 `GwTargetSet` 只能装"位置 + 生死"。
	# 旧实现到这里只有 `_targets = GwTargetSet.new()` 就再也没填过 ⇒ 联机局**一个靶子都画不出来**
	# （T17 把这条记成 §67.8 第 1 条：数据已经在 `room["targets"]`，缺的是界面侧接线）。
	_build_online_targets(room)
	_apply_online_turn(room)
	# 胜负规则只做占位：联机局由服务端判（`_check_match_over()` 的联机分支直接返回）。
	_rules = GwLevelRules.new()
	_rules.from_versus(_human_team())
	_set_axis_mode(int(app_node.state.axis_mode))
	_refresh_mode_indicators()
	_refresh_plane()
	_load_active_function()
	_update_timer_label()
	_apply_layout_sizes()
	_replay_left.visible = false
	_replay_right.visible = false
	_running = true
	_refresh_offset_label()
	_status(_online_status_text())

## 退出联机局（`end_match()` 调用；断线时 App 也会调 `end_match()`）。幂等。
func _online_teardown() -> void:
	if _online_session != null and _online_session.message_received.is_connected(_on_online_message):
		_online_session.message_received.disconnect(_on_online_message)
	_online_session = null
	_online_mode = false
	_online_soldier = -1
	_online_turn_player = -1
	_online_shot = {}
	_online_lives.clear()
	_online_effects.clear()
	_online_ending = false

## 订阅会话的 `message_received`。**先断后连**，所以连着调多次也只会有一个订阅。
func _online_subscribe(session: GwNetSession) -> void:
	var changed := _online_session != session
	if _online_session != null and changed \
			and _online_session.message_received.is_connected(_on_online_message):
		_online_session.message_received.disconnect(_on_online_message)
	_online_session = session
	if changed:
		# 换了一条连接 = 换了一局：上一局的生死/特效/弹道记录一律作废。
		# （同一条连接上 `begin_match()` 会被调两次，那时这些记录要留着 —— 不然
		#  第一次重建之后收到的 `LifeQueuedDeath` 会在第二次重建时被抹掉。）
		_online_lives.clear()
		_online_effects.clear()
		_online_shot = {}
		_online_shot_status = ""
	if session != null and not session.message_received.is_connected(_on_online_message):
		session.message_received.connect(_on_online_message)

## 服务端那份房间模型。
##
## 会话没了（断线 / 被 `App._cancel_connect()` 放掉）就返回空字典 —— **不**拿
## `App.online_room` 顶替：那一份是 `GwNetSession.snapshot()` 的**摘要**（只有条数，
## 没有坐标），拿它摆只会摆出一个空场。`App.online_room` 只在本文件的
## `online_state()` 里当"App 侧看到的条数"用（诊断 / 自检对拍）。
func _online_room() -> Dictionary:
	if _online_session != null:
		return _online_session.room
	return {}

## 服务端 `player_id` → `state.players` 下标。**顺序必须与 `App._sync_state_from_net()`
## 一致**（那里也是 `ids.sort()`），否则 `_functions` / `_active` 会错位。
func _rebuild_pid_index(room: Dictionary) -> void:
	_pid_index = {}
	var players: Dictionary = room.get("players", {})
	var ids := players.keys()
	ids.sort()
	for i in ids.size():
		_pid_index[int(ids[i])] = i

## 士兵：**服务端 `entities` 里 `kind == "soldier"` 的那些**（`PosInfo` 给坐标、
## `TeamInfo` 给队伍、`SoldierInfo` 给归属、`LifeInfo` 给生死）。
##
## 这与本地局的三处不同，正是 `docs/134` §7.1 第 1 条要的：
## * 坐标来自服务端（本地 `_spawn_position()` 在联机分支里**不再被调用**）；
## * 一名玩家可以有多个士兵（本地是"每个玩家的第一个活着的士兵"）；
## * 每个士兵带 `entity_id` —— 服务端的 `TurnInfo/FunctionPoints/LifeInfo` 都用实体 id 说话。
func _build_online_soldiers(room: Dictionary, app_node) -> void:
	var players: Dictionary = room.get("players", {})
	# 颜色：一支队伍一张 6 色表（`docs/102` §1.2）。联机下用**确定性**种子 ——
	# 原版的颜色是服务端用 `ColorInfo` 发的（`docs/33` §4 没实现），所以"同一个士兵
	# 每局都同色"是自定做法，不是复原结论（`docs/139` §6）。
	var teams: Array[int] = []
	for pid in _pid_index:
		var p: Dictionary = players.get(pid, {})
		teams.append(int(p.get("team", 0)))
	var colors := assign_colors(teams, 1)
	var entities: Dictionary = room.get("entities", {})
	var ids := entities.keys()
	ids.sort()                  # 服务端的下发顺序不保证；固定顺序让自检可复现
	for eid in ids:
		var e: Dictionary = entities[eid]
		if String(e.get("kind", "")) != "soldier":
			continue
		var pid := int(e.get("player_id", -1))
		if not _pid_index.has(pid):
			# 玩家表里还没这个人（`SoldierInfo` 可能比 `PlayerInfo` 先到）：补一个下标，
			# 让"下标"这件事永远有定义（`docs/139` §4）。
			_pid_index[pid] = _pid_index.size()
			teams.append(int(e.get("team", 0)))
			colors = assign_colors(teams, 1)
		var pi := int(_pid_index[pid])
		# 生死：会话模型里的 `alive` 优先；但 `LifeQueuedDeath` 不会被 `GwNetSession` 落进
		# `entities`（它只认 `LifeInfo`），所以再用本界面记下的那份广播覆盖一次
		# （否则同一条连接上第二次 `begin_match()` 会把刚死的人复活，`docs/139` §4）。
		var alive := bool(e.get("alive", true))
		if _online_lives.has(int(eid)):
			alive = bool(_online_lives[int(eid)])
		_soldiers.append({
			"name": _online_player_name(pi, pid, app_node),
			"team": int(e.get("team", 0)),
			"pos": Vector2(float(e.get("x", 0.0)), float(e.get("y", 0.0))),
			"alive": alive,
			"is_bot": false,        # 服务端不说谁是谁；一律按真人画（未知，docs/139 §7）
			"bot_tier": 0,
			"skin": int(e.get("team", 0)) * 3,   # 与 `App._sync_state_from_net()` 同一取值
			"player": pi,
			"player_id": pid,
			"entity_id": int(eid),
			# 颜色：**`ColorInfo` 的真值优先**（同样先实体 id、再 `player_id`；缺席才用本队色表）。
			"color": _online_entity_color(int(eid), pid,
				colors[pi] if pi < colors.size() else GwThemeFactory.COL_ACCENT),
		})

## 玩家的**本地 profile 名**（`App.state.players[pi].name` ⇒ `store.profile.name`）。
##
## ⚠️ 这不是联机局的真值：真值在 `room["names"]`（`NameInfo`）。本函数只当**兜底**
## （线上没给 `NameInfo` 时才有意义），联机士兵的名字走 `_online_entity_name()`。
func _online_player_name(pi: int, pid: int, app_node) -> String:
	if app_node != null and pi >= 0 and pi < app_node.state.players.size():
		return String(app_node.state.players[pi].name)
	return "Player %d" % pid

## 联机实体的名字：**线上 `NameInfo` 优先**，其次它的 `player_id` 那条，最后才退回本地 profile 名。
##
## 为什么要"先实体再 player_id"：本样本的 `NameInfo` 只有一条
## `NameInfo{entity_id: 229, name: "cmdgz"}`，而**射手是玩家 229 名下的士兵 230**
## （`SoldierInfo{230, player_id: 229}`）⇒ 只查 `names[eid]` 会查空。
## 这条链就是真帧上射手头顶那个 `cmdgz` 的来源（`docs/06` §60.5 的"名字标签"那一行）。
func _online_entity_name(eid: int, pid: int, pi: int, app_node) -> String:
	var names: Dictionary = _online_room().get("names", {})
	if names.has(eid) and String(names[eid]) != "":
		return String(names[eid])
	if names.has(pid) and String(names[pid]) != "":
		return String(names[pid])
	return _online_player_name(pi, pid, app_node)

## 联机实体的颜色：**线上 `ColorInfo` 优先**，同样先实体 id、再它的 `player_id`。
##
## 本样本只有 `ColorInfo{229: (125,104,0)}`（玩家实体），射手 230 与靶子 232 都没有自己那条
## ⇒ 士兵的队色/环色由它所属玩家那条 `ColorInfo` 决定，这正是真帧上的橄榄色 `(125,104,0)`
## （名字笔画色与准备环色**逐值相同**，`docs/06` §60.5 / §71.1-B 两处独立量过）。
## 真值缺席才退回本队色表 —— 那是**我方自定**（`assign_colors()`，`docs/139` §6 已登记）。
func _online_entity_color(eid: int, pid: int, fallback: Color) -> Color:
	var colors: Dictionary = _online_room().get("colors", {})
	var c: Variant = colors.get(eid, colors.get(pid, null))
	if c is Dictionary:
		return Color(float(c.get("r", 0)) / 255.0, float(c.get("g", 0)) / 255.0,
			float(c.get("b", 0)) / 255.0)
	return fallback

## 联机事件的"外观三件套"（`room["look"][entity_id]`，由 `Skin/Face/HatGraphicsInfo` 落进去）。
## 回放那边同一件事由 `_replay.graphics(kind, entity_id)` 提供；本地局没有这条事实 ⇒ 空字典。
##
## `_sync_entities()` 只用它来决定"要不要显式 `set_entity_look`"：空字典 ⇒ 不调，
## 保持 `spawn_entity` 的默认三件套（原版 `GraphwarPlayerConfig::new` 的字面量）。
func _look_for(eid: int, kind: String = "") -> Dictionary:
	if _replay_mode and _replay != null:
		var sk := String(_replay.graphics("skin", eid))
		if sk == "":
			return {}
		return {
			"skin": sk,
			"face": String(_replay.graphics("face", eid)),
			"hat": String(_replay.graphics("hat", eid)),
		}
	var got := {}
	if _online_mode:
		var look: Dictionary = _online_room().get("look", {})
		if look.has(eid):
			got = look[eid]
	# **缺项按种类补默认值**：`EntityDrawer::spawn_entity` 的默认三件套是
	# `yellow/regular_eyes/soldier_helmet`（`GraphwarPlayerConfig::new` 的字面量），而靶子
	# 在原版里是 `target/no_face/no_hat`（`res://skins/target.svg` 在表里）。
	# 这一步**必须**补齐：`set_entity_look()` 是三件一起换的，传进去的空串会让那两层精灵
	# 落到"纯色兜底"上 —— 实测靶子会变成一个 55×55 的蓝色方块（2 970 px），而原版是 25×25 的环。
	var out := {
		"skin": "target" if kind == "target" else "yellow",
		"face": "no_face" if kind == "target" else "regular_eyes",
		"hat": "no_hat" if kind == "target" else "soldier_helmet",
	}
	for k in ["skin", "face", "hat"]:
		if String(got.get(k, "")) != "":
			out[k] = String(got[k])
	return out

## 障碍：`AddObstacle` 下发的 `{pos_x, pos_y, radius}`，顺序即服务端顺序。
## 形状与 `GwCore.level_obstacles()` 的产物一致（`{pos: Vector2, size: 半径}`），
## 因为 `_set_obstacles()` 与 `GwCore.hit_test()` 都吃这一种。
func _build_online_obstacles(room: Dictionary) -> void:
	var list: Array = []
	for o in (room.get("obstacles", []) as Array):
		list.append({
			"pos": Vector2(float(o.get("x", 0.0)), float(o.get("y", 0.0))),
			"size": float(o.get("r", 0.0)),
		})
	_obstacles = list
	_set_obstacles(_obstacles)

## 回合：`turn{player_id, soldier_id}` 决定**谁是射手**（`_active` 是玩家下标，
## `_online_soldier` 是实体 id —— 两套编号都在用，别混）。
func _apply_online_turn(room: Dictionary) -> void:
	var turn: Dictionary = room.get("turn", {})
	_online_turn_player = int(turn.get("player_id", -1))
	_online_soldier = int(turn.get("soldier_id", -1))
	_active = int(_pid_index.get(_online_turn_player, 0))
	var app_node := app()
	if app_node != null:
		_time_left = float(GwModes.turn_seconds(app_node.state.time_mode))
	# 服务端已经记着的函数原文（`FunctionInfo.owner_id` = **soldier_id**，`docs/24` §4.5）
	var fns: Dictionary = room.get("functions", {})
	for owner in fns:
		var s := _online_soldier_by_entity(int(owner))
		if not s.is_empty():
			_functions[int(s.player)] = String(fns[owner].get("function", ""))
	_load_active_function()
	_update_timer_label()
	_refresh_function_list()

## 这一发能不能由我来发。
##
## * `_online_soldier < 0`：服务端还没说轮到谁（开局批还没到）⇒ 不能发；
## * `_my_player_id < 0`：**降级** —— 不知道我是谁，就按"当前回合的玩家"打
##   （真服务端不发 `PlayerInfo` 时只有这样；`docs/134` §7.1 的临时办法）；
## * 否则严格比对：`TurnInfo.player_id == 我`。
func _online_can_fire() -> bool:
	if not _online_mode:
		return true
	if _online_soldier < 0:
		return false
	if _my_player_id < 0:
		return true
	return _online_turn_player == _my_player_id

## 联机下的时钟：只走数、不换手。换手是服务端 `TurnInfo` 的事 ——
## 本地那条"到点自动换手"（`_process` 的另一半）在这里会让两个真相源打架。
func _online_tick(delta: float) -> void:
	var app_node := app()
	if app_node == null:
		return
	var limit := GwModes.turn_seconds(app_node.state.time_mode)
	if limit <= 0:
		_update_timer_label()
		return
	_time_left = maxf(0.0, _time_left - delta)
	_update_timer_label()

## 联机开火：**只发两条请求**，弹道与服务端算。判定/生死/回合全在服务端
## （`src/graphwar_game_server.rs`；`docs/134` §7.1 第 3 条）。
func _fire_online(expr: String, soldier: Dictionary) -> void:
	if _online_session == null:
		_status("联机会话已断（socket 关了），这一发没有发出去")
		return
	if _online_soldier >= 0 and int(soldier.get("entity_id", -1)) != _online_soldier:
		_status("服务端说的射手是实体 %d，不是 %d —— 这一发不发" % [
			_online_soldier, int(soldier.get("entity_id", -1))])
		return
	if not _online_can_fire():
		_status("还没轮到我（服务端：玩家 %d ／ 我：%s）" % [_online_turn_player,
			"不知道（服务端没发 PlayerInfo）" if _my_player_id < 0 else str(_my_player_id)])
		return
	var pid := _my_player_id if _my_player_id >= 0 else _online_turn_player
	_functions[_active] = expr
	var sent_submit := _online_session.submit_function(pid, expr)
	var sent_fire := _online_session.fire(pid)
	_online_shot_status = "已请求开火：player %d ／ 函数 %s ／ 提交 %s ／ 开火 %s" % [
		pid, expr, "已发" if sent_submit else "没发出去", "已发" if sent_fire else "没发出去"]
	_status("联机：弹道等服务端的 FunctionPoints 回来再画（%s）" % _online_shot_status)

## 服务端每广播一条消息就到这里（`GwNetSession.message_received`）。
##
## 注意**顺序**：`GwNetSession._on_frame()` 先 `_apply()` 落到 `room`，再发这个信号 ——
## 所以这里读 `room` 拿到的是**已更新**的模型（`PosInfo`/`LifeInfo`/`AddObstacle` 都是）。
func _on_online_message(kind: String, body: Dictionary) -> void:
	if not _online_mode:
		return
	match kind:
		"NameInfo", "SkinGraphicsInfo", "FaceGraphicsInfo", "HatGraphicsInfo", "ColorInfo":
			# 这五条改的是"实体的外观事实"（名字 / 三张图 / 颜色），消费者是 `_sync_entities()`。
			# 它们**晚于** `begin_match()` 到（见本函数下面 PosInfo 的处理同理）⇒ 收到就重摆一次。
			# 原版对应的是 `EntityDrawer` 订阅 graphics/name/color 事件后自己重画（`docs/16` §44）。
			_refresh_plane()
		"PlayerInfo":
			# 会话侧已经判过"这条是不是给我的"（`GwNetSession._apply` 比对 connection_id），
			# 这里只做镜像，不再自己判一次（免得出现两个真相源）。
			if _online_session != null and int(_online_session.my_player_id) >= 0:
				_my_player_id = int(_online_session.my_player_id)
				_status(_online_status_text())
		"PosInfo":
			var eid := GwNetFrame.body_int(body, "entity_id", -1)
			var s := _online_soldier_by_entity(eid)
			if not s.is_empty():
				s["pos"] = Vector2(GwNetFrame.body_float(body, "pos_x"),
					GwNetFrame.body_float(body, "pos_y"))
				_refresh_plane()
				_status("联机：实体 %d 移动到 (%.2f, %.2f)"
					% [eid, float(s.pos.x), float(s.pos.y)])
		"LifeInfo":
			# `LifeInfo{alive:false}` 是**回放**里的死亡记录（`docs/96` §4 的真相样本），
			# 也是"复活"（`docs/123` §3.2）唯一能表达的形态 ⇒ 两种取值都认。
			_online_apply_life(GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_bool(body, "alive", true))
		"LifeQueuedDeath":
			# 而**线上**的致死那一批是 `LifeQueuedDeath{entity_id, tick}` + `EffectInfo(DeathEffect)`
			# （`docs/31` §1、`docs/43`）：我们的服务端照这个发（`src/game.rs::fire`），
			# 它**不**发 `LifeInfo{alive:false}`。本客户端没有 tick 队列（收尸时刻未还原，
			# `docs/30` §5），所以收到就按"已死"处理 —— **推断/自定**，写进 `docs/139` §5。
			_online_apply_life(GwNetFrame.body_int(body, "entity_id", -1), false)
		"EffectInfo":
			# 现在**真的画**（`docs/143` §1）：`GwEffectLayer` 是本轮新增的特效层。
			# 位置与种类都来自服务端，本地只负责画与出声（音效自定，`docs/128` §4）。
			var fx_type := GwNetFrame.body_str(body, "effect_type")
			var fx_pos := Vector2(GwNetFrame.body_float(body, "pos_x"),
				GwNetFrame.body_float(body, "pos_y"))
			_online_effects.append({
				"x": fx_pos.x, "y": fx_pos.y, "type": fx_type,
				"tick": GwNetFrame.body_int(body, "start_tick", 0),
			})
			_spawn_effect(fx_type, fx_pos)
		"TargetInfo", "EntityRemoved":
			# 靶子层是 `GwTargetSet`（只存位置 + 生死）：`TargetInfo` 增、`EntityRemoved` 减，
			# 两条都要求重建一次（原版 `GraphwarTargetManager` 收到就改表，`docs/131`）。
			_build_online_targets(_online_room())
			_refresh_plane()
		"TurnInfo":
			_apply_online_turn(_online_room())
			_status(_online_status_text())
		"EndOfTurnInfo":
			# 本客户端还没发 `TickRequest`（`room.current_tick` 恒 0），所以只记下来，
			# 倒计时仍按墙钟走（`docs/139` §7 记为未做）。
			_status("联机：本回合截止 tick %d（%s）／当前 tick %d" % [
				GwNetFrame.body_int(body, "end_of_turn_tick", 0),
				GwNetFrame.body_str(body, "time_mode"),
				int(_online_room().get("current_tick", 0))])
		"FunctionInfo":
			var owner_id := GwNetFrame.body_int(body, "owner_id", -1)
			var so := _online_soldier_by_entity(owner_id)
			if not so.is_empty():
				_functions[int(so.player)] = GwNetFrame.body_str(body, "function")
				_refresh_function_list()
		"FunctionPoints":
			# **弹道的唯一来源**：服务端只广播起点与终点 x（`docs/24` §6.1）。
			_draw_online_shot(GwNetFrame.body_int(body, "owner_id", -1),
				Vector2(GwNetFrame.body_float(body, "start_x"),
					GwNetFrame.body_float(body, "start_y")),
				GwNetFrame.body_float(body, "end_x"))
		"FunctionActive":
			# **起飞 tick 的服务端真值**（原版 `set_function_active_tick`，`0x456b80`）：
			# 写进 `FunctionState.value+0x74`，之后 `GraphwarPlane::draw` 的在场谓词
			# `active_tick < tick`（`0x3be5f6`）与揭示量 `30·(fractional_tick − active_tick)`
			# （`0x4b67a0`）都用它。**这条迟到也没关系**：原版就是两个独立的 setter，
			# 顺序无关（`FunctionPoints` 先到、`FunctionActive` 后到是常态）。
			var aoid := GwNetFrame.body_int(body, "owner_id", -1)
			var atick := GwNetFrame.body_int(body, "active_tick", 0)
			if aoid >= 0 and _plane != null:
				_plane.call("set_function_active_tick", aoid, atick)
				_feed_curve()
		"AddObstacle", "ClearObstacles", "QueueDestroyObstacle", "DestroyObstacle":
			_build_online_obstacles(_online_room())
			_refresh_plane()
		"GameStateInfo":
			# 服务端判完了胜负：**线上原文是 `GameStateInfo{game_state:"EndingGame"}`**，
			# 不是一条叫 `EndingGame` 的报文（`fixtures/frame_endgame_events.txt` 的
			# `[phase3]` 末行；`docs/06` §77 更正 —— 这一支以前按事件名匹配，**从来没命中过**）。
			# **当前这一发可能还在飞** ⇒ 只记下来，收尾在弹道落地之后（`_on_shot_arrived()`）。
			# 这正好是 `docs/143` §5 要的"结算完成前不结束"。
			if GwNetFrame.body_str(body, "game_state") == "EndingGame":
				_online_ending = true
				# 弹道还在飞 ⇒ 等它落地（`_on_shot_arrived()`）；已经不飞 ⇒ 立刻停本屏流程
				# （`_running=false` 只挡输入/回合推进/时钟，**不动**画面：`_process` 里
				#  `_advance_presentation()` 在它之前）。
				if not _shot_in_flight:
					_running = false
				_status("联机：服务端宣布本局结束（`GameStateInfo{EndingGame}`）／当前弹道%s"
					% ("还在飞，落地后收尾" if _shot_in_flight else "已落地"))
		_:
			pass

## 一条生死广播落到界面：翻 `_soldiers` 的 alive 位、刷绘制、记进 `_online_lives`。
##
## **只翻 alive 位**（与 `docs/123` §2.2 的形状结论一致：不重建实体、不碰掩膜）。
## 联机下 `_refresh_plane()` 会把阵亡者从**绘制列表**里去掉（服务端在
## `src/game.rs::fire` 里也把它从 `Soldiers` 里 retain 掉了 —— 两边是同一件事）。
func _online_apply_life(eid: int, alive: bool) -> void:
	if eid < 0:
		return
	_online_lives[eid] = alive
	var s := _online_soldier_by_entity(eid)
	if not s.is_empty():
		s["alive"] = alive
		_refresh_plane()
	_status("联机：实体 %d %s（场上还剩 %d 名士兵）" % [
		eid, "存活" if alive else "阵亡", _alive_soldier_count()])

func _online_soldier_by_entity(eid: int) -> Dictionary:
	if eid < 0:
		return {}
	for s in _soldiers:
		if int(s.get("entity_id", -1)) == eid:
			return s
	return {}

func _alive_soldier_count() -> int:
	var n := 0
	for s in _soldiers:
		if bool(s.get("alive", true)):
			n += 1
	return n

## 用**内核**复算服务端那一发弹道并画出来（`docs/134` §7.1 第 3 条）。
##
## 为什么能复算：服务端只发 `start_x/start_y/end_x`（`docs/24` §6.1），轨迹本身不发 ——
## 但采样器是确定性的（`docs/04`/`docs/116`），所以"同一条函数 + 同一个起点 + 同一个方向"
## 必然给出同一条曲线。**方向由服务端数据定**：`end_x` 落在起点哪一侧就往哪一侧飞
## （不是按队伍猜 —— 抓包里 `docs/24` §6.3 那两个样本的方向正是这样定下来的）。
##
## 修正（`f(x) + c`）由内核按 `c = y0 - f(x0)` 自己算（`src/_scaffold/api.rs::calculate_path`），
## 与 `Match::fire` 用的是同一个 `correction()`。`corrected_function`（三位小数的文本）
## 只拿来当**交叉证据**写进诊断，不参与计算：它被四舍五入过，用它反而会引入误差。
func _draw_online_shot(owner_id: int, start: Vector2, end_x: float) -> void:
	var shooter := _online_soldier_by_entity(owner_id)
	var expr := ""
	var corrected := ""
	if _online_session != null:
		var fns: Dictionary = _online_session.room.get("functions", {})
		if fns.has(owner_id):
			expr = String(fns[owner_id].get("function", ""))
			corrected = String(fns[owner_id].get("corrected", ""))
	if expr == "":
		_online_shot_status = ("收到实体 %d 的 FunctionPoints（起点 (%.3f, %.3f)、终点 x %.3f），"
			+ "但这一发没有函数原文，画不出来") % [owner_id, start.x, start.y, end_x]
		_status(_online_shot_status)
		return
	var direction := 1.0
	if end_x < start.x:
		direction = -1.0
	var result := GwCore.calculate_path(expr, start, direction)
	if not bool(result.ok):
		_clear_curve()
		_online_shot_status = "内核算不出这一发（%s）／ 函数 %s" % [String(result.note), expr]
		_status(_online_shot_status)
		return
	var pts: PackedVector2Array = result.points
	# 只画到服务端说的 `end_x`：服务端的停点 = 它那条点序列的最后一点
	# （`src/graphwar_shot_calculator/settle.rs::resolve_points`），而采样器确定性 ⇒ 到同一个 x 之前的点序列
	# 就是服务端那条轨迹。服务端按 0.5 的弦长采样、内核按 0.01，所以停点最多差一个弦长
	# （`docs/139` §4 记了这一条）。
	var cut := _truncate_path_at_x(pts, end_x, direction)
	if cut.size() < 2:
		cut = pts
	var impact: Vector2 = cut[cut.size() - 1]
	_set_curve(cut, (shooter.get("color", GwThemeFactory.COL_ACCENT)
		if not shooter.is_empty() else GwThemeFactory.COL_ACCENT), owner_id)
	# 这一发也要走"在飞"状态（`docs/143` §4）：服务端在 `ExecutingResults` 期间同样不收
	# 新的 `FunctionFire`，本地不能比它更宽松。**不**做本地结算（生死/回合在服务端）。
	_pending_shot = {"phase": "online", "owner": owner_id, "stop": impact,
		"judged": {"stop": impact, "sources": []}}
	# 周期本体在 Rust：点数为 `cut.size()`、命中队列空、`already_settled = true`
	# （联机下生死与回合都归服务端，本地只负责"飞到哪就收尾"）。
	_flow.begin(cut.size(), [], [], impact, true)
	_shot_in_flight = true
	# ⚠️ **不**在这里用 `result.offset` 覆盖右下标签：那一发用的是**服务端收到的函数串**，
	# 而原版的标签跟的是**本地输入框**（本样本输入框是空的 ⇒ 原版 S8 仍显示 `y0` = `- 9.890`）。
	# `docs/06` §60.5 那行"S7 不显示 / S8 显示 - 3.694 ⇒ 数值与出现时机都不同"就是这条。
	_refresh_offset_label()
	_online_shot = {
		"owner": owner_id, "function": expr, "start": start, "end_x": end_x,
		"direction": direction, "points": cut.size(), "offset": float(result.offset),
		"last_x": float(impact.x), "last_y": float(impact.y),
		"corrected": corrected, "blocked_at_end": bool(_stop_before_end(pts, end_x, direction)),
	}
	_online_shot_status = ("服务端弹道：实体 %d ／ 函数 %s ／ 起点 (%.3f, %.3f) ／ 终点 x %.3f ／ "
		+ "方向 %s ／ %d 点 ／ 修正 %s（服务端文本 %s）") % [
		owner_id, expr, start.x, start.y, end_x, "+x" if direction > 0.0 else "-x",
		cut.size(), GwCore.format_offset(float(result.offset)),
		corrected if corrected != "" else "—"]
	_status(_online_shot_status)

## 把点序列截到 `end_x` 为止（含越过它的第一个点）。
func _truncate_path_at_x(pts: PackedVector2Array, end_x: float, direction: float) -> PackedVector2Array:
	var out := PackedVector2Array()
	for p in pts:
		out.append(p)
		if (p.x - end_x) * direction >= -1e-9:
			break
	return out

## 内核算出来的整条点序列是否**已经越过** `end_x`（= 服务端提前停下，通常是被障碍挡住）。
func _stop_before_end(pts: PackedVector2Array, end_x: float, direction: float) -> bool:
	if pts.is_empty():
		return true
	return (pts[pts.size() - 1].x - end_x) * direction < -1e-6

func _online_status_text() -> String:
	var where := "（会话已断）"
	if _online_session != null:
		where = "%s:%d" % [String(_online_session.host), int(_online_session.port)]
	var who := "不知道（服务端没发 PlayerInfo）" if _my_player_id < 0 else str(_my_player_id)
	return ("联机 %s ／ 我 = %s ／ 轮到玩家 %d（射手实体 %d）／ 士兵 %d（活 %d）／ 障碍 %d ／ "
		+ "可开火 %s ／ 弹道 %s") % [
		where, who, _online_turn_player, _online_soldier, _soldiers.size(),
		_alive_soldier_count(), _obstacles.size(), "是" if _online_can_fire() else "否",
		_online_shot_status if _online_shot_status != "" else "等 FunctionPoints"]

## 联机局的**只读**快照（自检 / `--dump` 诊断用；不改任何状态）。
##
## 与 `level_state()` / `replay_state()` 同一性质：都是"把内部状态摊平成字典"，
## 让自检可以断言"画面上的士兵/障碍确实来自服务端"。
func online_state() -> Dictionary:
	var soldiers: Array = []
	for s in _soldiers:
		soldiers.append({
			"entity_id": int(s.get("entity_id", -1)),
			"player_id": int(s.get("player_id", -1)),
			"player": int(s.get("player", -1)),
			"team": int(s.get("team", 0)),
			"alive": bool(s.get("alive", true)),
			"x": float(s.pos.x),
			"y": float(s.pos.y),
		})
	var obstacles: Array = []
	for o in _obstacles:
		obstacles.append({
			"x": float(o["pos"].x), "y": float(o["pos"].y), "r": float(o.get("size", 0.0)),
		})
	var shooter := _active_soldier()
	var app_node := app()
	return {
		"mode": _online_mode,
		"my_player_id": _my_player_id,
		"turn_player": _online_turn_player,
		"turn_soldier": _online_soldier,
		"can_fire": _online_can_fire(),
		"shooter": {} if shooter.is_empty() else {
			"entity_id": int(shooter.get("entity_id", -1)),
			"x": float(shooter.pos.x), "y": float(shooter.pos.y),
			"team": int(shooter.get("team", 0)),
		},
		"players": _pid_index.size(),
		"soldiers": soldiers,
		"alive_soldiers": _alive_soldier_count(),
		# `drawn_soldiers` = `EntityDrawer` 里的实体数；`drawn_points` = **交给平面的**点数
		# （= 这一发复算出来的全量，不是"已经揭示到第几个"——揭示另有 `revealed_points`）。
		"drawn_soldiers": (int(_entities.call("entity_count")) if _entities != null else -1),
		"drawn_points": _curve_total,
		"revealed_points": _curve_reveal(),
		"obstacles": obstacles,
		"effects": _online_effects.size(),
		"lives": _online_lives.duplicate(),
		"shot": _online_shot.duplicate(true),
		"shot_status": _online_shot_status,
		"status": _status_text,
		"received": _online_session.received_kinds() if _online_session != null else [],
		"room_snapshot": (app_node.online_room.duplicate(true) if app_node != null else {}),
	}

# ---------------------------------------------------------------- 每帧

func _process(delta: float) -> void:
	# 曲线的 tick 在**任何模式**下都推进：原版是 `GraphwarTickHandler::get_fractional_tick`
	# （`0x519530`）每帧喂给 `GraphwarPlane::draw` 的喂点循环，那个模块还没接 ⇒ 本屏按帧间隔推
	# （`docs/16` §43.6(d) 第 2 条就是这条缺口；`advance_shot_for_test()` 是自检的确定性入口）。
	_advance_curve(delta)
	if _replay_mode:
		_replay_tick(delta)
		return
	# 弹道的表现**任何状态下都继续推进**（`docs/143` §3）：一发打完之后还要把爆炸放完、
	# 把曲线清掉；对局已经在 `_on_shot_arrived()` 里结束了也照样收尾。
	_advance_presentation()
	if not _running:
		return
	# 联机：时钟照走，但**回合推进不由本地做**（服务端的 `TurnInfo` 说了算）。
	# 本地那条"到点自动换手"在联机里会与服务端打架 —— 于是分成独立分支，互不干扰。
	if _online_mode:
		_online_tick(delta)
		return
	var app_node := app()
	if app_node == null:
		return
	# 轮到 AI ⇒ 到点由**它自己**开火（旧实现没有这一步，界面上就一直等着人类再按 Fire）。
	_tick_bot_turn(delta)
	# 回合时钟 + 超时谓词都在 Rust（`GwTurnClock` → `handle_tick` 的三个合取项，
	# `0x470989`–`0x4709ec`）：状态（在等开火）、截止 tick、"有人在等"。
	if not _turn_clock.has_deadline():
		_update_timer_label()      # `TimerInf`：没有截止时刻（战役关 21/21 都是这一档）
		return
	_turn_clock.advance(delta)
	if String(_turn_clock.decide(_shot_in_flight, _running)) == "auto_fire":
		# 原版到点做的**不是**换手，而是 `call fire_functions`（`0x4709ec`）：
		# 把已提交的那个函数打出去。这里照做；万一打不出去（输入框空 / 轮到 AI），
		# 才退回换手 —— **我方兜底**，见 `docs/16-client.md` §53.4。
		if not fire():
			_status("回合时限到，没有可提交的函数 ⇒ 换手")
			_next_turn(true)
	_update_timer_label()

## 倒计时。**无限时限时原版是空白**（观测：`orig_campaign_play.png` 左下角没有任何字符，
## 而 LAN 局同样位置显示 `00:58`）—— 所以这里给空串，不给 `∞`。
func _update_timer_label() -> void:
	if _timer_label == null:
		return
	var app_node := app()
	if app_node == null:
		return
	# 联机：倒计时是本地那条 `_time_left`（换手与否归服务端，本屏只显示）。
	if _online_mode:
		var limit := GwModes.turn_seconds(app_node.state.time_mode)
		_timer_label.text = "" if limit <= 0 else GwModes.format_clock(_time_left)
		return
	# 本地：秒数由 Rust 的回合时钟给（`-1.0` = 无限时档 ⇒ 空白）。
	var left := _turn_clock.seconds_left()
	_timer_label.text = "" if left < 0.0 else GwModes.format_clock(left)

# ---------------------------------------------------------------- 附加按钮与显示开关

## 复刻 `ShowExtraButtons.gd`：按下 → 取反 force_show、切图标、重排顶上留白。
func toggle_names() -> void:
	_show_names = not _show_names
	_set_show_names(_show_names)
	_names_button.icon = GwIcons.get_icon("view_names" if _show_names else "hide_names")

## 原版 `toggle_view_functions`：三张图标 = 三种「查看函数」档位。
func toggle_view_functions() -> void:
	_view_mode = (_view_mode + 1) % VIEW_MODES.size()
	_view_func_button.icon = GwIcons.get_icon(VIEW_MODES[_view_mode])
	_show_function_overlay = _view_mode != 0
	_view_functions_label.visible = _show_function_overlay
	_refresh_function_list()

## 原版 `toggle_mute`：`AudioServer.set_bus_mute`（`docs/111`/`docs/118` §2 的字段集里
## 有 `muted`，与这条调用对得上）。本项目没有音频资源，但总线静音是真实生效的。
func toggle_mute() -> void:
	_muted = not _muted
	_mute_button.icon = GwIcons.get_icon("mute" if _muted else "unmute")
	var bus := AudioServer.get_bus_index("Master")
	if bus >= 0:
		AudioServer.set_bus_mute(bus, _muted)

func _refresh_function_list() -> void:
	if _view_functions_label == null:
		return
	var app_node := app()
	if app_node == null:
		return
	var lines: Array[String] = []
	if _replay_mode and _replay != null:
		# 回放里的函数表在事件里（`FunctionInfo.owner_id` = 士兵号，`docs/79` §1），
		# 不在 `state.players` 里 —— 那张表在回放模式下是空的。
		var funcs: Dictionary = _replay.functions()
		for owner in funcs.keys():
			lines.append("[color=white]soldier %d: f(x) = %s[/color]"
				% [int(owner), String(funcs[owner])])
		_view_functions_label.clear()
		_view_functions_label.append_text(
			"[outline_size=4][outline_color=black]%s[/outline_color][/outline_size]"
			% "\n".join(lines))
		return
	for pi in app_node.state.players.size():
		var p: Dictionary = app_node.state.players[pi]
		if _view_mode == 0 and int(pi) != int(_active):
			continue
		var fn := String(_functions.get(pi, ""))
		if fn == "":
			fn = "—"
		var color := "green" if int(pi) == int(_active) else "white"
		lines.append("[color=%s]%s: f(x) = %s[/color]" % [color, String(p.name), fn])
	_view_functions_label.clear()
	_view_functions_label.append_text(
		"[outline_size=4][outline_color=black]%s[/outline_color][/outline_size]" % "\n".join(lines))

## 诊断用（**不显示**）。原版没有这行字，用户实测明确指出多了一行说明。
func _status(text_: String) -> void:
	_status_text = text_
	if _status_label != null:
		_status_label.text = text_

func on_enter() -> void:
	if not _running:
		begin_match()
