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
		var b := GwButton.make("", cmd)
		b.name = "GraphwarButton"
		b.icon = GwIcons.get_icon(String(icon_name))
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
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
	_plane.clear_path()

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
	_input.key_input(cmd)
	_functions[_active] = _input.text

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
	var active_soldier := _active_soldier()
	if active_soldier.is_empty() or not bool(active_soldier.alive):
		_status("当前射手已阵亡")
		return
	var expr := String(_input.text).strip_edges()
	if expr == "":
		_status("先输入一个函数")
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
func _human_team() -> int:
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

# ---------------------------------------------------------------- 每帧

func _process(delta: float) -> void:
	if not _running:
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
