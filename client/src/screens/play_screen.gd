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

## 顶栏三个「查看函数」档位（原版三张图，`docs/121` §4.3：`view_single_function` 在
## `.tscn`/`.so` 里都没有引用点，但图标确实存在；顺序按语义排 single→recent→all）。
const VIEW_MODES := ["view_single_function", "view_recent_functions", "view_all_functions"]

var _plane: GwPlaneView
var _input: GwFunctionInput
## 开火表现（`docs/135`）：准备光圈 + 弹道按 tick 推进。挂在棋盘上，自己 `_process`。
var _fire_fx: GwFireFx
var _timer_label: Label
var _offset_label: Label
var _fx_button: GwButton
var _view_functions_label: RichTextLabel
var _extra_vbox: VBoxContainer
var _extra_hbox: HBoxContainer
var _show_extra_button: GwButton
var _names_button: GwButton
var _view_func_button: GwButton
var _mute_button: GwButton
var _chat_button: GwButton
var _fire_button: GwButton
var _status_label: Label
var _replay_left: Control
var _replay_right: Control
var _plane_container: Control
var _extra_pad: MarginContainer
var _plane_aspect: AspectRatioContainer
var _left_grid: GridContainer
var _right_grid: GridContainer
var _keys: Array[GwButton] = []
var _view_mode := 1
var _muted := false

# --- 对局状态（界面侧） ---
var _running := false
var _soldiers: Array[Dictionary] = []          # {name, team, pos, alive, is_bot, skin, player, color}
## 本局的障碍：每项 `{pos: Vector2, size: float}`（`size` = 半径，棋盘单位）。
## 来自内核 `level_obstacles()`（`docs/126`）；**同一个数组**既给 `_plane.set_obstacles()`
## 画，也给 `GwCore.hit_test()` 判 —— 画的和判的必须是同一批圆。
var _obstacles: Array = []
## 本局的靶子集合（`docs/131`）。**同一个集合**既给 `_plane.set_targets()` 画，
## 也给 `GwCore.hit_test()` 判 —— 画的和判的必须是同一批靶子。
## 之前这里只有 `_plane.set_targets(...)`（纯绘制），靶子**根本没进命中判定**，
## 于是打靶关的胜负永远由"两队士兵存活数"决定 ⇒ 第一发就通关（`docs/131` §1）。
var _targets := GwTargetSet.new()
## 本局的胜负规则（`docs/131` §2）：打靶关 / 对战关 / 两者都有。
var _rules: GwLevelRules = null
## 已结算的回合数：`docs/118` §7.1 的 `turn_limit` 与它比较（`Level 7` = 1）。
var _turns_used := 0
## 本次结算里被摧毁的靶子 id（保序）。用于"打不全就全部复原"这条推断规则。
var _destroyed_this_shot: Array[int] = []
## 上一次胜负判定结果（诊断/自检用；不参与逻辑）。
var _last_verdict := {}
## 自检用：注入确定的靶子/障碍布局。原版这两样都是**每局随机**的
## （`docs/118` §8 #16："本来就没有每关固定坐标"），所以这不是篡改关卡数据，
## 只是把随机量钉住，好让端到端用例可复现。空字典 = 用本关自己的值。
var _layout_override := {}
var _functions := {}                            # player_index -> String
var _active := 0
var _time_left := 0.0
var _shot_queue: Array[Dictionary] = []         # 同时回合制下本轮的弹道
var _show_names := true
var _show_function_overlay := false
var _extra_forced := false
var _last_result := {}
## 状态行的"本轮"部分（这一行**不显示**，只给 selftest / `--dump` 当诊断，`docs/125` §5）。
var _shot_status := ""
## 状态行当前文本（`_status()` 写入；诊断用）。
var _status_text := ""
## 程序化改写输入框（换手时回填）期间不要清 OffsetLabel —— 否则会把刚算出来的修正抹掉。
var _suppress_offset_clear := false

# --- 回放模式（`docs/133`；这是本文件里唯一为回放新增的分叉） ---
## 回放模式下：不接管输入（键盘/开火无效），由 tick 时钟消费事件流。
var _replay_mode := false
var _replay: GwReplayPlayer = null
var _replay_path := ""
var _replay_bar: HBoxContainer = null
var _replay_progress: ProgressBar = null
var _replay_label: Label = null
## 回放里的"当前这一发"（诊断 / 状态行用）。
var _replay_shot_status := ""

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
## 服务端广播过的生死（`LifeInfo`）：`{entity_id: alive}`。
var _online_lives := {}

func _ready() -> void:
	font_divisor = 30        # ButtonGrid.gd / FireContainer.gd / ExtraOptionsHBox.gd 都是 /30
	super()

# ---------------------------------------------------------------- 搭界面

func build() -> void:
	# 原版 MarginContainer 的 margin 由安全区脚本给（桌面 = 0），VBox separation = 4
	# （实测：PlayMainHBox y=0..1058、HBoxContainer2 y=1062..1160 ⇒ 间隔 4）。
	var margin := MarginContainer.new()
	margin.name = "MarginContainer"
	margin.set_anchors_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 0)
	add_child(margin)
	var vbox := GwUI.vbox("VBoxContainer", 4, margin)

	var main_hbox := GwUI.hbox("PlayMainHBox", 4, vbox)
	main_hbox.size_flags_vertical = Control.SIZE_EXPAND_FILL

	# ---------------- 左控制区 ----------------
	var left := GwUI.vbox("LeftControls", 4, main_hbox)
	left.size_flags_horizontal = Control.SIZE_FILL    # 宽度由内容决定（3 键 + 2 个 4px 间隔）
	var left_top := GwUI.hbox("LeftTopButtonsContainer", 4, left)
	var exit_pair := GwUI.exit_pair(GwCommands.ASK_EXIT_PLAY)
	left_top.add_child(exit_pair)
	_show_extra_button = GwButton.make("")
	_show_extra_button.name = "ShowExtraButtons"
	_show_extra_button.icon = GwIcons.get_icon("show_extra_buttons")
	_show_extra_button.tooltip_text = "显示/隐藏顶栏附加按钮（ShowExtraButtons.gd）"
	_show_extra_button.pressed.connect(_toggle_extra_buttons)
	left_top.add_child(_show_extra_button)

	# 原版 `LeftControls` 里有两个 `size_flags_vertical = 3` 的 MarginContainer：
	# 一个在键盘**前**、一个在键盘**后** ⇒ 键盘在竖直方向居中（实测 grid y=281 @1920，
	# 不是紧贴左上角）。这里照抄同一结构。
	var left_top_pad := GwUI.margin("MarginContainer", 0, left)
	left_top_pad.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_left_grid = _make_grid("LeftButtonGrid", left)
	_fill_grid(_left_grid, LEFT_KEYS)
	var left_filler := Control.new()
	left_filler.name = "Filler"
	left_filler.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left.add_child(left_filler)
	_replay_left = _build_replay_controls("LeftReplaysButtonsGrid", ["prev", "play_pause", "next"])
	(left as VBoxContainer).add_child(_replay_left)

	# ---------------- 战场 ----------------
	_plane_container = GwUI.margin("PlaneContainer", 0, main_hbox)
	_plane_container.name = "PlaneContainer"
	_plane_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_plane_container.size_flags_vertical = Control.SIZE_EXPAND_FILL

	# 原版：MarginContainer 会把**所有**子节点铺成同一块矩形 ⇒ GamePlane 与
	# ExtraOptionsVBox 天然叠放；AspectRatioContainer 同样把每个子节点铺成同一块矩形 ⇒
	# GraphwarPlane 与函数列表覆盖层叠放。这里照原版结构搭，不另造叠放容器。
	var game_plane := GwUI.vbox("GamePlane", 4, _plane_container)
	_extra_pad = GwUI.margin("ExtraOptionPad", 0, game_plane)
	_extra_pad.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	_plane_aspect = AspectRatioContainer.new()
	_plane_aspect.name = "PlaneContainer"
	_plane_aspect.ratio = BOARD_ASPECT
	_plane_aspect.stretch_mode = AspectRatioContainer.STRETCH_FIT
	_plane_aspect.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_plane_aspect.size_flags_vertical = Control.SIZE_EXPAND_FILL
	game_plane.add_child(_plane_aspect)

	_plane = GwPlaneView.new()
	_plane_aspect.add_child(_plane)

	# 开火表现（`docs/135`）：光圈 + 弹道逐 tick 揭示。**挂在棋盘上当子节点**，
	# 自己 `_process`；棋盘上的东西仍由 `GwPlaneView` 画，分工不变。
	_fire_fx = GwFireFx.new()
	_fire_fx.plane = _plane
	_fire_fx.state_provider = fire_fx_state
	_plane.add_child(_fire_fx)

	var overlay := GwUI.vbox("VBoxContainer", 2, _plane_aspect)
	overlay.alignment = BoxContainer.ALIGNMENT_BEGIN
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var function_pad := GwUI.margin("ViewFunctionPad", 2, overlay)
	function_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label_row := GwUI.hbox("HBoxContainer", 4, function_pad)
	var lm := GwUI.margin("MarginContainer", 2, label_row)
	lm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_view_functions_label = GwUI.rich(16)
	_view_functions_label.name = "ViewFunctionsLabel"
	_view_functions_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_view_functions_label.visible = false
	lm.add_child(_view_functions_label)

	# 顶栏 4 个开关（ExtraOptionsVBox/ExtraOptionsHBox，居中、z_index=10）
	_extra_vbox = GwUI.vbox("ExtraOptionsVBox", 0, _plane_container)
	_extra_vbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
	(_extra_vbox as VBoxContainer).size_flags_vertical = Control.SIZE_FILL
	_extra_hbox = GwUI.hbox("ExtraOptionsHBox", 4, _extra_vbox)
	_extra_hbox.alignment = BoxContainer.ALIGNMENT_CENTER
	_extra_hbox.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_extra_hbox.z_index = 10
	_view_func_button = _extra_button("ViewFunctionButton", "toggle_view_functions",
		VIEW_MODES[_view_mode])
	_names_button = _extra_button("ShowNames", "toggle_names", "view_names")
	_chat_button = _extra_button("ChatButtonPlay", "toggle_chat", "no_chat")
	_mute_button = _extra_button("MuteButton", "toggle_mute", "unmute")
	for b in [_view_func_button, _names_button, _chat_button, _mute_button]:
		_extra_hbox.add_child(b)
	_extra_vbox.visible = true          # 原版默认可见（`play_screen.tscn` 里 ExtraOptionsVBox 没有 visible=false）

	# ---------------- 右控制区 ----------------
	var right := GwUI.vbox("RightControls", 4, main_hbox)
	right.size_flags_horizontal = Control.SIZE_FILL
	var fire_row := GwUI.hbox("HBoxContainer", 4, right)
	var fire_margin := GwUI.margin("FireContainer", 0, fire_row)
	fire_margin.name = "FireContainer"
	fire_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var fire_stack := Control.new()
	fire_stack.name = "FireStack"
	fire_stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# 原版 FireContainer 的 `custom_minimum_size.x = x*0.07`，高度由文字撑到与左上按钮行同高
	# （实测 FireButton = 337×96 @1920）。这里把高度也钉到同一档，右侧键盘才能像原版那样居中。
	fire_margin.add_child(fire_stack)
	var clip := GwButton.make(" Fire! ")
	clip.name = "ClipButton"
	clip.flat = true
	clip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	clip.set_anchors_preset(Control.PRESET_FULL_RECT)
	clip.add_child(_anim_rect())
	fire_stack.add_child(clip)
	_fire_button = GwButton.make(" Fire! ")
	_fire_button.name = "FireButton"
	_fire_button.long_press_input_text = GwCommands.FIRE   # 原版：FireButton 只有 long_press_input_text
	_fire_button.set_anchors_preset(Control.PRESET_FULL_RECT)
	_fire_button.tooltip_text = "开火（fire）"
	fire_stack.add_child(_fire_button)

	var right_top_pad := GwUI.margin("MarginContainer", 0, right)
	right_top_pad.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_right_grid = _make_grid("RightButtonGrid", right)
	_fill_grid(_right_grid, RIGHT_KEYS)
	var right_filler := Control.new()
	right_filler.name = "Filler"
	right_filler.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(right_filler)
	# YButton(y)：原版两种模式下都恒为 disabled（docs/92 §4.10 观测）。
	var y_button := _right_grid.get_node_or_null("YButton")
	if y_button != null:
		(y_button as GwButton).disabled = true
		(y_button as GwButton).tooltip_text = "原版里这颗键在 Normal/DiffEq 两种模式下都恒为 disabled（原因未知，docs/92 §8）"
	_replay_right = _build_replay_controls("RightReplaysButtonsGrid", ["save_screenshot", "save_summary", "save_clip"])
	(right as VBoxContainer).add_child(_replay_right)

	# ---------------- 底部信息行 ----------------
	var info := GwUI.hbox("HBoxContainer2", 4, vbox)
	var timer_m := GwUI.margin("TimerContainer", 0, info)
	timer_m.size_flags_horizontal = Control.SIZE_FILL     # 原版：只有函数输入框抢水平空间
	_timer_label = GwUI.label("00:00", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_timer_label.name = "TimerLabel"
	timer_m.add_child(_timer_label)
	var pad := GwUI.margin("MarginContainer", 0, info)
	pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pad.size_flags_stretch_ratio = 0.01       # OffsetContainer.gd 的兄弟间隔：实测 14 px @1920

	# f(x) = 是一个**图标**（`fx_option.svg` 里就画着 "f(x) ="，DiffEq 用 dy/dx 那张），
	# 不是一个 Label + 一个小图标 —— 原版 FxLabel 实测 visible=0。
	var fx_m := GwUI.margin("FxContainer", 0, info)
	fx_m.size_flags_horizontal = Control.SIZE_FILL
	_fx_button = GwButton.make("")
	_fx_button.name = "FxTextureButton"
	_fx_button.flat = true
	_fx_button.icon = GwIcons.get_icon("fx_option")
	_fx_button.tooltip_text = "函数模式指示（Normal / DiffEq）"
	_fx_button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	fx_m.add_child(_fx_button)

	_input = GwFunctionInput.new()
	_input.name = "GraphwarFunctionInput"
	_input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_input.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_input.text_changed.connect(_on_function_text_changed)
	_input.text_submitted.connect(_on_function_text_submitted)
	info.add_child(_input)

	var offset_m := GwUI.margin("OffsetContainer", 0, info)
	offset_m.size_flags_horizontal = Control.SIZE_FILL
	_offset_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_offset_label.name = "OffsetLabel"
	_offset_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	offset_m.add_child(_offset_label)

	# ---------------- 回放进度（**我们自定**，原版回放界面未观测；`docs/133` §4.2） ----------------
	# 原版 `play_screen.tscn` 里**没有**这条：左侧 `LeftReplaysButtonsGrid` 只有
	# prev/play_pause/next 三颗键（`docs/92` §4.10）。进度条与 tick 计数是我们加的，
	# 因为"放到第几个 tick"是回放唯一的时间轴（`docs/35`：60 tick/秒），没有它无法定位。
	_replay_bar = GwUI.hbox("ReplayStatusBar", 6, vbox)
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

	# 原版**没有**这行状态字（用户实测：底部多了一行说明）。保留节点但永不显示，
	# 只给 selftest / --dump 当诊断用（`docs/125` §5 登记）。
	_status_label = GwUI.label("", 12, HORIZONTAL_ALIGNMENT_LEFT)
	_status_label.name = "StatusLabel"
	_status_label.visible = false
	_status_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_status_label)

	_plane_container.resized.connect(_update_view_geometry)


func _anim_rect() -> ColorRect:
	var r := ColorRect.new()
	r.name = "AnimationRect"
	r.color = Color(0, 0, 0, 0)
	r.mouse_filter = Control.MOUSE_FILTER_IGNORE
	r.set_anchors_preset(Control.PRESET_FULL_RECT)
	return r

func _make_grid(node_name: String, parent: Node) -> GridContainer:
	var holder := GwUI.margin("MarginContainer", 0, parent)
	holder.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	var grid := GridContainer.new()
	grid.name = node_name
	grid.columns = 3
	grid.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	holder.add_child(grid)
	return grid

func _fill_grid(grid: GridContainer, keys: Array) -> void:
	for i in keys.size():
		var spec: Dictionary = keys[i]
		var b: GwButton
		if spec.has("text"):
			b = GwButton.make(String(spec.text), String(spec.cmd))
		else:
			b = GwButton.make("", String(spec.cmd))
			b.icon = GwIcons.get_icon(String(spec.icon))
		b.name = "GraphwarButton%d" % (i + 1)
		if String(spec.cmd) == "y":
			b.name = "YButton"
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		grid.add_child(b)
		_keys.append(b)

func _extra_button(node_name: String, cmd: String, icon_name: String) -> GwButton:
	var b := GwButton.make("     ")      # 原版的 5 个空格 —— 它决定了按钮的最小宽度（实测 146px @1920）
	b.name = node_name
	b.input_text = cmd
	b.icon = GwIcons.get_icon(icon_name)
	b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	b.expand_icon = true
	b.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	return b

## 回放专用按钮组（原版两处 `*ReplaysButtonsGrid`，只在回放模式显示）。
func _build_replay_controls(node_name: String, icons: Array) -> Control:
	var box := GwUI.hbox(node_name, 4)
	box.visible = false
	for icon_name in icons:
		var cmd := ""
		match String(icon_name):
			"prev": cmd = GwCommands.KEY_PREV
			"play_pause": cmd = GwCommands.PLAY_PAUSE
			"next": cmd = GwCommands.KEY_NEXT
			"save_screenshot": cmd = GwCommands.SAVE_SCREENSHOT
			"save_summary": cmd = GwCommands.SAVE_SUMMARY
			"save_clip": cmd = GwCommands.SAVE_CLIP
		# ⚠️ **实现偏离（`docs/133` §4.3）**：左侧三颗播放键**不走 App 命令总线**。
		# 原版命令键（`play_pause` / `key_prev` / `key_next`）记在 `meta` 里；但 `app.gd`
		# 对这三个键的分支仍是 `notify("回放播放未实现")` 的占位，而 `app.gd` 不在
		# 本轮可改范围 ⇒ 直接接本界面的播放器，免得按一下弹一个假提示。
		# 右侧三颗（存截图/存摘要/存片段）保持原样，它们的语义本来就**未观测**。
		var playback := cmd == GwCommands.PLAY_PAUSE or cmd == GwCommands.KEY_PREV \
			or cmd == GwCommands.KEY_NEXT
		var b := GwButton.make("", "" if playback else cmd)
		b.name = "GraphwarButton"
		b.set_meta("observed_input_text", cmd)
		b.icon = GwIcons.get_icon(String(icon_name))
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.tooltip_text = "原版命令键：%s" % cmd
		if playback:
			b.pressed.connect(_on_replay_button.bind(cmd))
		box.add_child(b)
	return box

# ---------------------------------------------------------------- 尺寸（照原版脚本的比例）

## 原版各脚本的规则都写成 `screen_size.x / N`，所以这里在每次 resize 时重算。
## `_apply_font_scale` 由基类在 viewport resize 时调用，这里接在后面。
func apply_font_scale() -> void:
	super.apply_font_scale()
	if not _built:
		return
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		return
	var key := w * 0.15 / 3.0                       # ButtonGrid.gd / ExtraOptionsHBox.gd
	for b in _keys:
		b.custom_minimum_size = Vector2(key, key)
	for b in [_show_extra_button, _view_func_button, _names_button, _chat_button, _mute_button]:
		if b != null:
			b.custom_minimum_size = Vector2(key, key)
	var exit_pair := find_child("ExitButtonContainer", true, false)
	if exit_pair != null:
		(exit_pair as Control).custom_minimum_size = Vector2(key, key)
	if _fx_button != null:
		_fx_button.custom_minimum_size = Vector2(key, key)
	if _input != null:
		_input.custom_minimum_size = Vector2(key * 2.0, key)
	if _timer_label != null:
		# 观测：原版**无限时限**时 TimerLabel 也占 140 px @1920（0.0729·x）宽度，只是没有字符
		# （`orig_campaign_play.png` 左下角一片空白，但 GEOM 里 TimerContainer = 140 px）。
		# 这里同样保留宽度，否则整条底栏会整体左移 ~140 px（docs/125 §4）。
		_timer_label.custom_minimum_size = Vector2(w * 0.0729, 0)
	if _offset_label != null:
		# OffsetContainer.gd：`desired_size = (screen.x * 0.09, 0)`、字号 `screen.x / 60`
		_offset_label.custom_minimum_size = Vector2(w * 0.09, 0)
		_offset_label.add_theme_font_size_override("font_size", maxi(8, int(w / 60.0)))
	var fire_stack := find_child("FireStack", true, false)
	if fire_stack != null:
		(fire_stack as Control).custom_minimum_size = Vector2(key * 1.4, key)
	if _view_functions_label != null:
		_view_functions_label.add_theme_font_size_override("normal_font_size", maxi(8, int(w / 60.0)))
	_update_view_geometry()

## `ExtraOptionPad.gd` / `ExtraOptionsHBox.gd` 的同一条判据：
##   余高 = plane.h - plane.w / 1.66667；余高 >= 工具栏高 ⇒ 工具栏显示，并给顶上留出同高空位。
func _update_view_geometry() -> void:
	if _plane_container == null or _extra_hbox == null:
		return
	var pc := _plane_container.size
	if pc.x <= 1.0 or pc.y <= 1.0:
		return
	var w := get_viewport_rect().size.x
	var toolbar_h := w * 0.15 / 3.0
	var extra_y := pc.y - pc.x / BOARD_ASPECT
	var show := extra_y >= toolbar_h or _extra_forced
	_extra_hbox.visible = show
	if _extra_pad != null:
		_extra_pad.custom_minimum_size = Vector2(0, toolbar_h if show else 0)
		_extra_pad.visible = show

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
		_set_replay_input_enabled(true)
		_begin_online(app_node)
		return
	_online_teardown()          # 从联机局退回本地局时把订阅放掉（幂等）
	_replay_mode = false
	_replay = null
	_replay_bar.visible = false
	_set_replay_input_enabled(true)
	_functions.clear()
	_shot_queue.clear()
	_last_result = {}
	_soldiers.clear()
	_targets = GwTargetSet.new()
	_destroyed_this_shot.clear()
	_turns_used = 0
	_last_verdict = {}
	var level := GwCampaign.level(int(st.level_index)) if st.kind == GwGameState.Kind.CAMPAIGN else {}
	var team_counts := {0: 0, 1: 0}
	# 玩家颜色（`docs/102` §1.2：本队 6 色表 + 随机起点 + 线性探测）。真值是 RNG 抽的，
	# 这里用确定性种子，保证同一局每次配色一致。
	var teams: Array[int] = []
	for p in st.players:
		teams.append(int(p.team))
	var colors := GwPlaneView.assign_colors(teams, int(level.get("seed", 1)) + 1)
	for pi in st.players.size():
		var p: Dictionary = st.players[pi]
		_functions[pi] = ""
		for s in int(p.soldiers):
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
	_active = 0
	_time_left = float(GwModes.turn_seconds(st.time_mode))
	_running = true
	_plane.set_axis_mode(int(st.axis_mode))
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
	_plane.set_targets(_targets.alive_positions())
	_plane.set_obstacles(_obstacles)
	# 胜负规则（`docs/131` §2）：战役关按 `docs/118` §7.1 的"目标数 / Bot 数"两列分类；
	# 非战役对局没有靶子也没有回合上限，只有"对方全灭"这一条。
	_rules = (GwLevelRules.for_campaign(level, _human_team())
		if st.kind == GwGameState.Kind.CAMPAIGN else GwLevelRules.for_versus(_human_team()))
	_status("关卡规则：%s ／ 靶子 %d ／ 回合上限 %s" % [_rules.kind_name(), _targets.total(),
		"∞" if _rules.turn_limit <= 0 else str(_rules.turn_limit)])

func end_match() -> void:
	_running = false
	_replay_mode = false
	_replay = null
	if _replay_bar != null:
		_replay_bar.visible = false
	_set_replay_input_enabled(true)
	_plane.clear_path()
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

func _refresh_plane() -> void:
	var list: Array[Dictionary] = []
	for s in _soldiers:
		# 联机：阵亡的士兵**不上场**。服务端开火时就把死者从 `Soldiers` 里 retain 掉
		# （`src/game.rs::fire`），所以"服务端还有这个人"与"画面上还有这个人"是同一件事；
		# 本地局仍按老样子画成灰 + 叉（那套行为一个字没动）。
		if _online_mode and not bool(s.get("alive", true)):
			continue
		list.append(s)
	_plane.set_soldiers(list)
	_plane.set_show_names(_show_names)
	_plane.queue_redraw()

# ---------------------------------------------------------------- 障碍（docs/126）

## 本局的障碍列表。**生成在内核里**（`src/levelgen.rs` 的既有实现，公式逐位验证过），
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

## 开火表现的状态源（`docs/135` §2.2）：**谁该开火、他准备好没有**。
##
## `GwFireFx` 每帧拉一次，本函数不写任何对局状态。两个模式的口径：
## * **顺序回合**（`TurnInfo{player_id, soldier_id}`，`docs/46`）：只有当前该开火的那个
##   士兵有光圈 —— 也就是 `_active_soldier()`（射手轮转的真规则在
##   `docs/98`：每玩家一张士兵表 + 游标，我们这一版取"该玩家的第一个活着的士兵"）。
## * **同时回合**（`docs/86` §6）：每个还没提交函数、且自己还有活着的士兵的玩家，
##   各显示一个光圈；提交过的（已经在 `_shot_queue` 里）熄灭。
##
## "准备好"= **函数非空**（`strip_edges()` 后）。这条是**推断**：原版光圈"闭合"到底
## 对应"函数非空"还是"按了 Fire"，没有观测（`docs/135` §2.3）。
func fire_fx_state() -> Dictionary:
	var state := {"running": _running, "rings": []}
	if not _running:
		return state
	# 联机：光圈只跟**服务端的 `TurnInfo`** 走（一个回合只有一个射手，
	# 而且"该不该我开火"由服务端说，不由本地队伍算）。同时回合制在联机里没接，
	# 所以这里不做 `_shot_owners()` 那一套（`docs/139` §7 记为未做）。
	if _online_mode:
		var shooter_o := _active_soldier()
		if not shooter_o.is_empty() and bool(shooter_o.get("alive", true)):
			state["rings"] = [{
				"index": _soldiers.find(shooter_o),
				"pos": shooter_o.pos,
				"color": shooter_o.get("color", GwThemeFactory.COL_ACCENT),
				"ready": _online_can_fire()
					and String(_functions.get(_active, "")).strip_edges() != "",
			}]
		return state
	var app_node := app()
	if app_node == null:
		return state
	var simultaneous := int(app_node.state.turn_mode) == GwModes.Turn.SIMULTANEOUS
	var rings: Array = []
	if simultaneous:
		var fired := _shot_owners()
		for i in _soldiers.size():
			var s: Dictionary = _soldiers[i]
			if not bool(s.alive) or fired.has(int(s.player)):
				continue
			rings.append({
				"index": i,
				"pos": s.pos,
				"color": s.get("color", GwThemeFactory.COL_ACCENT),
				"ready": String(_functions.get(int(s.player), "")).strip_edges() != "",
			})
	else:
		var shooter := _active_soldier()
		if not shooter.is_empty() and bool(shooter.alive):
			var i2 := _soldiers.find(shooter)
			rings.append({
				"index": i2,
				"pos": shooter.pos,
				"color": shooter.get("color", GwThemeFactory.COL_ACCENT),
				"ready": String(_functions.get(int(shooter.player), "")).strip_edges() != "",
			})
	state["rings"] = rings
	return state

func _on_function_text_changed(new_text: String) -> void:
	_functions[_active] = new_text
	if not _suppress_offset_clear:
		_offset_label.text = ""   # 手改函数 → 上一发的修正不再对应当前输入
	_refresh_function_list()

func _on_function_text_submitted(_new_text: String) -> void:
	fire()

## 开火：算一条弹道并画出来。
func fire() -> void:
	var app_node := app()
	if app_node == null or not _running:
		return
	if _replay_mode:
		_status("回放模式不接管输入（这一局的每一发都来自录像）")
		return
	var active_soldier := _active_soldier()
	if active_soldier.is_empty() or not bool(active_soldier.alive):
		_status("当前射手已阵亡")
		return
	var expr := String(_input.text).strip_edges()
	if expr == "":
		_status("先输入一个函数")
		return
	# 联机：开火不再走本地结算（`docs/134` §7.1 第 3 条）。判定、生死、回合全在服务端；
	# 本界面只发两条请求，然后等 `FunctionPoints` 把弹道带回来（`_draw_online_shot()`）。
	if _online_mode:
		_fire_online(expr, active_soldier)
		return
	var team := int(active_soldier.team)
	var direction := 1.0 if team == 0 else -1.0     # 队伍固定在两侧（docs/105）
	var result := GwCore.calculate_path(expr, active_soldier.pos, direction)
	if not bool(result.ok):
		_status("弹道算不出来：%s" % String(result.note))
		return
	var pts: PackedVector2Array = result.points
	# 障碍挡弹道（`docs/30` §2：障碍让弹道**停下**，士兵只是被穿过）。
	# 这里只决定"**画**到哪一点"：`shot.points` 仍保留整条点序列 ——
	# 将来的 tick 动画（父 agent 另派）应当沿 `shot.points` 逐点推进、到
	# `drawn.impact` 停住，**不要**在这里写死"一次画完整条 = 本回合结束"。
	var drawn := _clip_to_obstacle(pts)
	var impact: Vector2 = drawn.impact
	_plane.path_color = active_soldier.get("color", GwThemeFactory.COL_ACCENT)
	_plane.set_path(drawn.points, impact, true)
	# tick 动画（`docs/135` §3）：沿**同一条** `drawn.points` 逐 tick 揭示，
	# 撞点就是它的最后一点。`shot.points` 仍是全量 ⇒ "画到哪"与"飞到哪"是两件事，
	# 与回合结算**没有**耦合（结算仍在下面 `_commit_shot`，见 §3.4 的取舍说明）。
	if _fire_fx != null:
		_fire_fx.play(drawn.points)
	# 修正常数（`docs/24` §6.2 的样本格式：`+ 4.638` / `- 8.315`，三位小数、符号后一个空格）。
	# DiffEq 模式下原版 OffsetLabel 为空（观测）。
	_offset_label.text = GwCore.format_offset(float(result.offset)) if int(app_node.state.function_mode) == 0 else ""
	if bool(drawn.blocked):
		_status("弹道撞到障碍，停在 (%.2f, %.2f) ／ 本局障碍 %d 个 ／ 点序列 %d 点（未截断）"
			% [impact.x, impact.y, _obstacles.size(), pts.size()])
	else:
		_status("弹道来源：%s ／ %s ／ 本局障碍 %d 个" %
			[String(result.source), String(result.note), _obstacles.size()])

	var shot := {"points": pts, "team": team, "shooter": _active, "soldier": active_soldier}
	_commit_shot(shot)

## 把一发（或同时回合的一批）弹道交给结算，然后走**回合结算**（`docs/131` §2、§4）。
##
## 同时回合制下只有"所有还活着的人都打过"才结算一次（`docs/86`）——那一批算**一个**回合。
## 顺序与原版语料一致的那部分：`docs/84`/`docs/86` 的结算本体仍在 `src/settle.rs`。
func _commit_shot(shot: Dictionary) -> void:
	var app_node := app()
	if app_node == null:
		return
	if int(app_node.state.turn_mode) == GwModes.Turn.SIMULTANEOUS:
		# 同时回合：先全部记下，轮完再结算（真实结算在 src/settle.rs，docs/86）
		_shot_queue.append(shot)
		if _all_alive_players_shot():
			_resolve_shots(_shot_queue)
			_shot_queue.clear()
			_next_turn(true)
			_settle_round()
		else:
			_next_turn(false)
	else:
		_resolve_shots([shot])
		_next_turn(true)
		_settle_round()

## 一次射击 / 一轮同时回合的**结算**：① 回合数 +1；② "打不全就全部复原"；③ 判胜负。
##
## 回合数为什么在这里 +1：战役关 `TimeModeInfo = TimerInf`（`docs/123` §5.1 #9），
## 根本没有超时路径，一个回合只能由"打完一发"来推进。LAN 的 3 分钟超时不计回合
## （那些对局的 `turn_limit` 恒为 0，`docs/118` §7.1）。这条口径见 `docs/131` §4.3。
func _settle_round() -> void:
	if _replay_mode:
		return          # 回放的胜负由录像自己的 GameStateInfo 决定，不在这里判
	_turns_used += 1
	var restored := _apply_target_reset()
	var extra := ""
	if not restored.is_empty():
		extra = " ／ 本发没打全 ⇒ 靶子全部复原（推断，docs/131 §4）"
	_status("%s ／ 回合 %d/%s%s" % [_shot_status, _turns_used,
		"∞" if _rules == null or _rules.turn_limit <= 0 else str(_rules.turn_limit), extra])
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

func _all_alive_players_shot() -> bool:
	var app_node := app()
	if app_node == null:
		return true
	for pi in app_node.state.players.size():
		for s in _soldiers:
			if int(s.player) == pi and bool(s.alive) and int(pi) not in _shot_owners():
				return false
	return true

func _shot_owners() -> Array:
	var out := []
	for shot in _shot_queue:
		out.append(int(shot.shooter))
	return out

## 命中判定 —— 走**内核的掩膜规则**（`docs/120`）。
##
## 真实规则在 `src/settle.rs::resolve_points`（`docs/30` 的真对局回放）：
## 障碍光栅化进 1000×600 掩膜决定弹道停在哪；士兵与**靶子**都**不**挡弹道，只在
## "弹道点落在半径内"时判死（`PosInfo.radius` 恒 0.5）。
## 射手本人不参与致死（`settle` 的约定：子弹从自己身上出发）。
##
## ⚠️ 靶子与士兵塞进 **同一个** `hit_test` 数组（借用关系，见 `GwTargetSet.entities()`
## 与 `docs/131` §3.1）：内核第 3 个入参只当"半径实体"用，回的 `hits` 是**下标**，
## 所以下标 → 实体的映射由本函数的 `owners` 表负责。**靶子必须在这里**，
## 否则打靶关的靶子只是"画着玩的"（那就是 `docs/131` §1 的 bug）。
func _resolve_shots(shots: Array) -> void:
	var kills := 0
	var target_kills := 0
	var sources: Array[String] = []
	var note := ""
	var blocked := false
	_destroyed_this_shot.clear()
	for shot in shots:
		var pts: PackedVector2Array = shot.points
		var entities: Array = []        # hit_test 第 3 个入参：[{pos, radius}, …]
		var owners: Array = []          # 与 entities 同下标：{kind: "soldier"/"target", …}
		for s in _soldiers:
			if not bool(s.alive):
				continue
			if is_same(s, shot.get("soldier", null)):
				continue                      # 射手本人不参与致死判定
			entities.append({"pos": s.pos, "radius": GwCore.SOLDIER_HIT_RADIUS})
			owners.append({"kind": "soldier", "soldier": s})
		for t in _targets.entities():
			entities.append(t)
			owners.append({"kind": "target", "target_id": int(t["target_id"])})
		# 没有实体但**有障碍**也要判：墙会把弹道拦住，实体在墙后就是安全的
		# （`docs/30`：掩膜决定停点，士兵不挡弹道）。`_obstacles` 与画的是同一个数组。
		if entities.is_empty() and _obstacles.is_empty():
			continue
		var judged := GwCore.hit_test(pts, entities, _obstacles)
		sources.append(String(judged.source))
		note = String(judged.note)
		blocked = blocked or bool(judged.get("blocked", false))
		for idx in judged.hits:
			var i := int(idx)
			if i < 0 or i >= owners.size():
				continue
			var owner: Dictionary = owners[i]
			if String(owner["kind"]) == "soldier":
				var victim: Dictionary = owner["soldier"]
				if bool(victim.alive):
					victim.alive = false
					kills += 1
			else:
				# 靶子：只翻 alive（对应 LifeInfo{alive:false}），不销毁实体（docs/123 §2.2）。
				if _targets.destroy(int(owner["target_id"])):
					target_kills += 1
					_destroyed_this_shot.append(int(owner["target_id"]))
	if not sources.is_empty():
		_shot_status = "本轮命中 %d 个士兵、%d 个靶子 ／ 判定来源：%s ／ %s%s" % [
			kills, target_kills, sources[0], note, "（弹道被障碍拦下）" if blocked else ""]
		_status(_shot_status)
	if kills > 0 or target_kills > 0:
		_refresh_plane()
		_refresh_targets()

## 靶子的绘制入口：`GwPlaneView.set_targets()` 只收 `Array[Vector2]`（它只管画），
## 所以"靶子被打掉 / 被复原"在界面上表现为这个数组少一项 / 多一项。
func _refresh_targets() -> void:
	_plane.set_targets(_targets.alive_positions())

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
	for s in _soldiers:
		if int(s.player) == _active:
			return s
	return {}

func _next_turn(advance: bool) -> void:
	if not advance:
		return
	var app_node := app()
	if app_node == null:
		return
	var n: int = app_node.state.players.size()
	if n <= 0:
		return
	var guard := 0
	while guard <= n:
		_active = (_active + 1) % n
		guard += 1
		var has_alive := false
		for s in _soldiers:
			if int(s.player) == _active and bool(s.alive):
				has_alive = true
				break
		if has_alive:
			break
	_time_left = float(GwModes.turn_seconds(app_node.state.time_mode))
	_load_active_function()
	_update_timer_label()
	_refresh_function_list()

func _load_active_function() -> void:
	if _input == null:
		return
	_suppress_offset_clear = true
	_input.text = String(_functions.get(_active, ""))
	_suppress_offset_clear = false
	# 注意：**不**在这里清 OffsetLabel —— 原版换手时 OffsetLabel 仍显示上一发的修正。

## 判一局结束没有 —— **按关卡类型**（`docs/131` §2），不再只看两队士兵的存活数。
##
## 旧写法（`alive = {0:n, 1:m}`，任一为 0 就结束）在**打靶关**上是必然误判：
## 那些关没有敌方士兵 ⇒ `alive[1] == 0` ⇒ 第一发一落地就"人类胜"（`docs/131` §1）。
## 现在：打靶关要**靶子全清**，对战关要**对方队伍全灭**（`docs/84`：45/45 实测），
## 回合上限用尽仍未清完 ⇒ 失败（`docs/118` §7.1，`Level 7` = 1）。
func _check_match_over() -> void:
	if _replay_mode:
		return
	# 联机：胜负由**服务端**判（它发 `EndingGame`，`src/serverloop.rs`；本界面只收）。
	# 本地这套 `GwLevelRules` 是"单机/战役"的规则，拿去判联机局会出现两个真相源。
	if _online_mode:
		return
	var app_node := app()
	if app_node == null:
		return
	if _rules == null:
		_rules = GwLevelRules.for_versus(_human_team())
	var verdict := _rules.evaluate({
		"alive_by_team": _alive_by_team(),
		"teams_present": _teams_present(),
		"targets_total": _targets.total(),
		"targets_alive": _targets.alive_count(),
		"turns_used": _turns_used,
	})
	_last_verdict = verdict
	if not bool(verdict.get("over", false)):
		return
	_running = false
	var human_won := bool(verdict.get("human_won", false))
	var winner := int(verdict.get("winner_team", _human_team()))
	_status("%s ／ 判定：%s（%s）" % [_status_text, "人类胜" if human_won else "人类负",
		String(verdict.get("reason", "?"))])
	app_node.report_match_result(human_won, winner)

## 每队的**存活士兵数**（键固定 0/1，与旧实现的形状一致）。
func _alive_by_team() -> Dictionary:
	var alive := {0: 0, 1: 0}
	for s in _soldiers:
		if bool(s.alive):
			alive[int(s.team)] = int(alive[int(s.team)]) + 1
	return alive

## **开局时**就有士兵的队伍（不是"当前还活着的队伍" —— 一队全灭后它必须仍在表里，
## 否则 `enemy_present` 会永远为假，对战关就永远判不出胜负）。
func _teams_present() -> Array:
	var out := []
	for s in _soldiers:
		if not out.has(int(s.team)):
			out.append(int(s.team))
	return out

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
	return {
		"running": _running,
		"rules": (_rules.snapshot() if _rules != null else {}),
		"targets": _targets.snapshot(),
		"turns_used": _turns_used,
		"verdict": (_last_verdict.duplicate(true) if not _last_verdict.is_empty() else {}),
		"shot_status": _shot_status,
		"obstacles": _obstacles.size(),
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

## **外部来弹**：把"别人打过来的一发"走与 `fire()` **完全相同**的结算路径
## （`_resolve_shots` → `_next_turn` → `_settle_round`）。
##
## 现在客户端还没有 AI 开火循环（对手不会开火）、服务端也还没接，所以这条目前只被
## 自检用来构造"我方全灭 ⇒ 失败"（`docs/131` §5.2 ③）。接上 AI/服务端之后，
## 它就是这个回调本身，语义不用改。
func apply_external_shot(points: PackedVector2Array, shooter_player: int) -> bool:
	if not _running or points.size() < 2:
		return false
	var shooter := _soldier_of(shooter_player)
	if shooter.is_empty():
		return false
	_resolve_shots([{
		"points": points, "team": int(shooter.team),
		"shooter": shooter_player, "soldier": shooter,
	}])
	_next_turn(true)
	_settle_round()
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
	_turns_used = 0
	_last_verdict = {}
	_plane.clear_path()
	_set_replay_input_enabled(false)
	_replay_left.visible = true
	_replay_right.visible = true
	_replay_bar.visible = true

	var f := GwReplayFile.load_path(path)
	if not f.ok():
		_status("回放打不开：%s" % f.error)
		# `app.gd::_play_replay()` 在调 `begin_match()` **之前**先弹了一句
		# `回放播放未实现`（那是它还没接回放时的占位）。这里用一句准确的覆盖掉它 ——
		# 否则用户会一边看着回放在播、一边读到"未实现"。气泡文本不能含内部标记
		# （`app.gd::INTERNAL_MARKERS`），所以只写事实。
		notify("回放打不开：%s" % f.error)
		_replay_label.text = "回放读取失败：%s" % f.error
		_replay_progress.value = 0.0
		_running = false
		return
	_replay = GwReplayPlayer.new().setup(f)
	_replay_mode = true
	_running = true
	# 四个模式：回放里记着**这一局真实的选择**（`AxisModeInfo` 等），按观测到的字符串
	# 映射到 `GwModes` 的档位（对照表见 GwReplayPlayer.mode_indices 的注释）。
	var app_node := app()
	if app_node != null:
		var mi := _replay.mode_indices()
		app_node.state.axis_mode = int(mi.axis)
		app_node.state.function_mode = int(mi.function)
		app_node.state.turn_mode = int(mi.turn)
		app_node.state.time_mode = int(mi.time)
	_plane.set_axis_mode(int(app_node.state.axis_mode) if app_node != null else GwModes.AXIS_DEFAULT)
	_refresh_mode_indicators()
	# 回放不做胜负判定（录像自己会发 `GameStateInfo{EndingGame}`），这里只占位。
	_rules = GwLevelRules.for_versus(0)
	_replay.seek(_replay.first_tick)
	_realize_replay_state()
	_replay.play()
	_update_replay_ui()
	_status(_replay_status_text())
	# 同上：覆盖 `app.gd` 里那句占位气泡（`回放播放未实现`）。
	notify("回放 %s ／ %d 条事件 ／ tick %d–%d" % [
		_replay.state.game_name if _replay.state.game_name != "" else _replay_path.get_file(),
		_replay.file.events.size(), _replay.first_tick, _replay.last_tick])

## 主循环里的回放推进（`_process` 的回放分支）。
func _replay_tick(delta: float) -> void:
	if _replay == null:
		return
	var applied := _replay.advance_dt(delta)
	if not applied.is_empty():
		_apply_replay_events(applied)
	_update_replay_ui()

## 把一批刚被消费掉的事件喂给绘制/状态（**按事件类型分派，不猜副作用**）。
func _apply_replay_events(applied: Array[Dictionary]) -> void:
	var need_actors := false
	var need_layout := false
	var need_shot := false
	for ev in applied:
		match String(ev.kind):
			"PosInfo", "TeamInfo", "LifeInfo", "SoldierInfo", "EntityRemoved", \
			"SkinGraphicsInfo", "FaceGraphicsInfo", "HatGraphicsInfo", "TargetInfo", "ColorInfo":
				need_actors = true
			"AddObstacle", "ClearObstacles", "DestroyObstacle":
				need_layout = true
			"FunctionPoints":
				need_actors = true
				need_shot = true
			_:
				pass
	if need_layout:
		_replay_sync_layout()
	if need_actors:
		_replay_sync_actors()
	if need_shot:
		_replay_draw_last_shot()
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
	var shots: Array = _replay.state.shots if _replay != null else []
	if shots.is_empty():
		_plane.clear_path()
		_replay_shot_status = "还没有开火记录"
		return
	_replay_draw_shot(shots[shots.size() - 1])

func _replay_sync_layout() -> void:
	_obstacles = _replay.obstacles_for_view()
	_plane.set_obstacles(_obstacles)

## 士兵与靶子：**同一个局面对象**既给 `_plane` 画，也给"谁死了"用。
func _replay_sync_actors() -> void:
	var list: Array[Dictionary] = []
	for a in _replay.actors():
		if bool(a.is_target):
			continue            # 靶子走 `set_targets()`（与原版一样是独立图层，docs/131）
		list.append({
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
	_plane.set_soldiers(list)
	_plane.set_targets(_replay.target_positions())
	_plane.set_show_names(_show_names)
	_plane.queue_redraw()

## `SkinGraphicsInfo.graphics_str`（如 `face_paint_2`）→ `GwIcons.SKINS` 的下标。
## 名字对不上就退回 0（不编造皮肤）。**观测**：这三类字符串就是原版资产的文件名
## （`rsc/skins/*.svg`、`faces/*.svg`、`hats/*.svg`，`docs/121` §5）。
func _replay_skin_index(entity_id: int) -> int:
	if _replay == null:
		return 0
	var nm := String(_replay.state.graphics.get("skin:%d" % entity_id, ""))
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
	return GwPlaneView.TEAM_COLORS[team][0]

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
		expr = String(_replay.state.functions.get(owner, ""))
	if expr == "":
		_plane.clear_path()
		_replay_shot_status = "这一发没有函数文本（owner %d）" % owner
		return
	var team := _replay.team_index(owner)
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
	var mode := "ode" if String(_replay.state.function_mode) == "DiffEqFunction" else "normal"
	var result := GwCore.calculate_path(expr, start, direction, mode)
	var replay_corrected := String(d.get("corrected", ""))
	if not bool(result.ok):
		_plane.clear_path()
		_replay_shot_status = "内核算不出这一发（%s）／ 函数 %s" % [String(result.note), expr]
		return
	var drawn := _clip_to_obstacle(result.points)
	_plane.path_color = _replay_color({
		"color": _replay.state.colors.get(int(_replay.file.soldier_owner.get(owner, -1)), null),
		"team": maxi(0, team),
	})
	_plane.set_path(drawn.points, drawn.impact, true)
	# 修正常数：只有 Normal 有。ODE 下内核给的是 `NaN` ⇒ `format_offset()` 出空串，
	# 正好是原版 DiffEq 局右下角不显示修正文本的样子（`docs/92` §4.10、`docs/138`）。
	_offset_label.text = GwCore.format_offset(float(result.offset))
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
	var dead := 0
	var total := 0
	for a in _replay.actors():
		total += 1
		if not bool(a.alive):
			dead += 1
	return "回放 %s ／ %s ／ 状态 %s ／ 回合 %d ／ 实体 %d（阵亡 %d）／ %s ／ %s" % [
		_replay_path.get_file(), _replay.state.game_name, _replay.state.game_state,
		_replay.state.turn_count, total, dead, _replay.summary(), _replay_shot_status]

## 进度条 / tick 标签刷新。
func _update_replay_ui() -> void:
	if _replay == null or _replay_progress == null:
		return
	_replay_progress.value = _replay.progress()
	_replay_label.text = "回放 %s ／ %s ／ %s" % [
		_replay.state.game_name, _replay.summary(),
		"（已到结尾）" if _replay.finished() else "",
	]

## 左侧三颗播放键（prev / play_pause / next）。**我们自定的接线**，见 `docs/133` §4.3。
func _on_replay_button(cmd: String) -> void:
	if not _replay_mode or _replay == null:
		_status("当前不是回放模式")
		return
	match cmd:
		GwCommands.PLAY_PAUSE:
			_replay.toggle()
		GwCommands.KEY_PREV:
			_replay.step_back_event()
			_realize_replay_state()
		GwCommands.KEY_NEXT:
			_replay.step_event()
			_realize_replay_state()
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
		_suppress_offset_clear = true
		_input.text = ""
		_suppress_offset_clear = false

# ---- 自检 / 诊断用的只读入口（不参与游戏逻辑） ----

## 当前回放播放器的**规范性局面串**（与 Kernel / Python 参考同格式，`docs/133` §5）。
func replay_canonical() -> String:
	return _replay.canonical() if _replay != null else ""

func replay_state() -> Dictionary:
	if _replay == null:
		return {}
	return {
		"mode": _replay_mode,
		"tick": _replay.current_tick(),
		"first_tick": _replay.first_tick,
		"last_tick": _replay.last_tick,
		"cursor": _replay.cursor,
		"events": _replay.file.events.size(),
		"playing": _replay.playing,
		"progress": _replay.progress(),
		"game_name": _replay.state.game_name,
		"game_state": _replay.state.game_state,
		"turn_count": _replay.state.turn_count,
		"obstacles": _replay.state.obstacles.size(),
		"soldiers": _soldiers.size(),
		"canonical": _replay.canonical(),
		"shot_status": _replay_shot_status,
	}

## 自检用：把回放**推进** n 秒墙钟（等价于主循环跑 n 秒）。
func replay_advance_for_test(seconds: float) -> int:
	if _replay == null:
		return 0
	var applied := _replay.advance_dt(seconds)
	if not applied.is_empty():
		_apply_replay_events(applied)
	_update_replay_ui()
	return applied.size()

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
	_destroyed_this_shot.clear()
	_turns_used = 0
	_last_verdict = {}
	_time_left = 0.0
	_plane.clear_path()
	_plane.set_targets([])            # 服务端的 `Match` 里只有士兵，没有靶子（docs/117）
	_rebuild_pid_index(room)
	_build_online_soldiers(room, app_node)
	_build_online_obstacles(room)
	_apply_online_turn(room)
	# 胜负规则只做占位：联机局由服务端判（`_check_match_over()` 的联机分支直接返回）。
	_rules = GwLevelRules.for_versus(_human_team())
	_plane.set_axis_mode(int(app_node.state.axis_mode))
	_refresh_mode_indicators()
	_refresh_plane()
	_load_active_function()
	_update_timer_label()
	_apply_layout_sizes()
	_replay_left.visible = false
	_replay_right.visible = false
	_running = true
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
	var colors := GwPlaneView.assign_colors(teams, 1)
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
			colors = GwPlaneView.assign_colors(teams, 1)
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
			"color": colors[pi] if pi < colors.size() else GwThemeFactory.COL_ACCENT,
		})

func _online_player_name(pi: int, pid: int, app_node) -> String:
	if app_node != null and pi >= 0 and pi < app_node.state.players.size():
		return String(app_node.state.players[pi].name)
	return "Player %d" % pid

## 障碍：`AddObstacle` 下发的 `{pos_x, pos_y, radius}`，顺序即服务端顺序。
## 形状与 `GwCore.level_obstacles()` 的产物一致（`{pos: Vector2, size: 半径}`），
## 因为 `GwPlaneView.set_obstacles()` 与 `GwCore.hit_test()` 都吃这一种。
func _build_online_obstacles(room: Dictionary) -> void:
	var list: Array = []
	for o in (room.get("obstacles", []) as Array):
		list.append({
			"pos": Vector2(float(o.get("x", 0.0)), float(o.get("y", 0.0))),
			"size": float(o.get("r", 0.0)),
		})
	_obstacles = list
	_plane.set_obstacles(_obstacles)

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
## （`src/serverloop.rs`；`docs/134` §7.1 第 3 条）。
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
			# 只记录不绘制：`GwPlaneView` 没有特效图层，本轮不新造一个（`docs/139` §7）。
			_online_effects.append({
				"x": GwNetFrame.body_float(body, "pos_x"),
				"y": GwNetFrame.body_float(body, "pos_y"),
				"type": GwNetFrame.body_str(body, "effect_type"),
				"tick": GwNetFrame.body_int(body, "start_tick", 0),
			})
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
		"AddObstacle", "ClearObstacles", "QueueDestroyObstacle", "DestroyObstacle":
			_build_online_obstacles(_online_room())
			_refresh_plane()
		"EndingGame":
			_status("联机：服务端宣布本局结束（`EndingGame`）")
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
## 修正（`f(x) + c`）由内核按 `c = y0 - f(x0)` 自己算（`src/api.rs::calculate_path`），
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
		_plane.clear_path()
		_online_shot_status = "内核算不出这一发（%s）／ 函数 %s" % [String(result.note), expr]
		_status(_online_shot_status)
		return
	var pts: PackedVector2Array = result.points
	# 只画到服务端说的 `end_x`：服务端的停点 = 它那条点序列的最后一点
	# （`src/settle.rs::resolve_points`），而采样器确定性 ⇒ 到同一个 x 之前的点序列
	# 就是服务端那条轨迹。服务端按 0.5 的弦长采样、内核按 0.01，所以停点最多差一个弦长
	# （`docs/139` §4 记了这一条）。
	var cut := _truncate_path_at_x(pts, end_x, direction)
	if cut.size() < 2:
		cut = pts
	var impact: Vector2 = cut[cut.size() - 1]
	_plane.path_color = (shooter.get("color", GwThemeFactory.COL_ACCENT)
		if not shooter.is_empty() else GwThemeFactory.COL_ACCENT)
	_plane.set_path(cut, impact, true)
	if _fire_fx != null:
		_fire_fx.play(cut)          # 与本地开火同一条 tick 动画（docs/135）
	var app_node := app()
	if app_node != null and int(app_node.state.function_mode) == 0:
		_offset_label.text = GwCore.format_offset(float(result.offset))
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
		"drawn_soldiers": _plane.soldiers.size() if _plane != null else -1,
		"drawn_points": _plane.path_points.size() if _plane != null else -1,
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
	if _replay_mode:
		_replay_tick(delta)
		return
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
	var limit := GwModes.turn_seconds(app_node.state.time_mode)
	if limit <= 0:
		_update_timer_label()
		return
	_time_left -= delta
	if _time_left <= 0.0:
		_status("回合时限到，自动换手")
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
	var limit := GwModes.turn_seconds(app_node.state.time_mode)
	if limit <= 0:
		_timer_label.text = ""
		return
	_timer_label.text = GwModes.format_clock(_time_left)

# ---------------------------------------------------------------- 附加按钮与显示开关

## 复刻 `ShowExtraButtons.gd`：按下 → 取反 force_show、切图标、重排顶上留白。
func _toggle_extra_buttons() -> void:
	_extra_forced = not _extra_forced
	_show_extra_button.icon = GwIcons.get_icon(
		"hide_extra_buttons" if _extra_forced else "show_extra_buttons")
	_update_view_geometry()

func toggle_names() -> void:
	_show_names = not _show_names
	_plane.set_show_names(_show_names)
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
		for owner in _replay.state.functions.keys():
			lines.append("[color=white]soldier %d: f(x) = %s[/color]"
				% [int(owner), String(_replay.state.functions[owner])])
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
