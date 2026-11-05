extends GwScreen
## PlayScreen —— 对局界面。
##
## 结构（`docs/92` §4.10 + 恢复出的 play_screen.tscn **106 个静态节点**）：
##   MarginContainer/VBoxContainer/PlayMainHBox
##     ├ LeftControls
##     │    ├ LeftTopButtonsContainer/{ExitButtonContainer(ask_exit_play), ShowExtraButtons}
##     │    ├ MarginContainer/LeftButtonGrid        ← 18 键：C π e 1..9 ( 0 . ⇤ ← ⌫
##     │    └ LeftReplaysButtonsGrid                ← 回放专用（⏮︎ / play_pause / ⏭）
##     ├ PlaneContainer/GamePlane
##     │    ├ ExtraOptionPad
##     │    ├ PlaneContainer(AspectRatio)/GraphwarPlane   ← 战场（自绘）
##     │    │    └ VBoxContainer/{ViewFunctionPad, ViewFunctionsLabel}
##     │    └ ExtraOptionsVBox/ExtraOptionsHBox
##     │         {ViewFunctionButton, ShowNames, ChatButtonPlay, MuteButton}
##     └ RightControls
##          ├ HBoxContainer/FireContainer/{ClipButton, FireButton}
##          ├ MarginContainer/RightButtonGrid       ← 18 键：abs ln √ sin cos tan * / exp + - ^ x y )
##          └ RightReplaysButtonsGrid               ← 回放专用（save_screenshot/summary/clip）
##   HBoxContainer2: TimerContainer/TimerLabel · FxContainer/FxLabel+FxTextureButton ·
##                   GraphwarFunctionInput · OffsetContainer/OffsetLabel
##
## 布局数值（`docs/101` §5.2 的常量表）：ButtonGrid 宽/30 字号、每键 = 屏宽×0.15/3；
## FireContainer 宽×0.07；OffsetContainer 宽×0.09（OffsetLabel 最小宽 100）；
## FxContainer 宽/60；ExtraOptionsHBox 宽/30；ViewFunctionPad 最小高 = 展示区宽/1.66667。

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

var _plane: GwPlaneView
var _input: GwFunctionInput
var _timer_label: Label
var _offset_label: Label
var _fx_label: Label
var _fx_button: GwButton
var _view_functions_label: RichTextLabel
var _extra_vbox: VBoxContainer
var _extra_hbox: HBoxContainer
var _show_extra_button: GwButton
var _names_button: GwButton
var _view_func_button: GwButton
var _mute_button: GwButton
var _fire_button: GwButton
var _status_label: Label
var _replay_left: Control
var _replay_right: Control
var _function_pad: MarginContainer

# --- 对局状态（界面侧） ---
var _running := false
var _soldiers: Array[Dictionary] = []          # {name, team, pos, alive, is_bot, skin, player}
var _functions := {}                            # player_index -> String
var _active := 0
var _time_left := 0.0
var _shot_queue: Array[Dictionary] = []         # 同时回合制下本轮的弹道
var _show_names := true
var _show_function_overlay := true
var _extra_visible := false
var _last_result := {}
## 程序化改写输入框（换手时回填）期间不要清 OffsetLabel —— 否则会把刚算出来的修正抹掉。
var _suppress_offset_clear := false

func _ready() -> void:
	font_divisor = 30        # ButtonGrid.gd / FireContainer.gd / ExtraOptionsHBox.gd 都是 /30
	super()

# ---------------------------------------------------------------- 搭界面

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox

	var main_hbox := GwUI.hbox("PlayMainHBox", 6, vbox)
	main_hbox.size_flags_vertical = Control.SIZE_EXPAND_FILL

	# ---------------- 左控制区 ----------------
	var left := GwUI.vbox("LeftControls", 6, main_hbox)
	left.custom_minimum_size = Vector2(220, 0)
	left.size_flags_horizontal = Control.SIZE_FILL    # 不抢水平空间，中间战场优先

	var left_top := GwUI.hbox("LeftTopButtonsContainer", 6, left)
	left_top.add_child(GwUI.exit_pair(GwCommands.ASK_EXIT_PLAY))
	_show_extra_button = GwButton.make("")
	_show_extra_button.name = "ShowExtraButtons"
	_show_extra_button.icon = GwIcons.get_icon("show_extra_buttons")
	_show_extra_button.custom_minimum_size = Vector2(48, 48)
	_show_extra_button.tooltip_text = "显示/隐藏附加按钮（ShowExtraButtons.gd 的行为，docs/101 §4.2）"
	_show_extra_button.pressed.connect(_toggle_extra_buttons)
	var extra_holder := GwUI.margin("MarginContainer", 2, left_top)
	extra_holder.add_child(_show_extra_button)

	var left_grid_margin := GwUI.margin("MarginContainer", 2, left)
	left_grid_margin.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	var left_grid := GridContainer.new()
	left_grid.name = "LeftButtonGrid"
	left_grid.columns = 3
	left_grid.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	left_grid_margin.add_child(left_grid)
	_fill_grid(left_grid, LEFT_KEYS)
	# 多余高度交给尾部 Spacer，避免键盘被拉长把底部信息行挤出屏幕
	var left_filler := Control.new()
	left_filler.name = "Filler"
	left_filler.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left.add_child(left_filler)

	_replay_left = _build_replay_controls("LeftReplaysButtonsGrid", ["prev", "play_pause", "next"])
	(left as VBoxContainer).add_child(_replay_left)

	# ---------------- 战场 ----------------
	var plane_container := GwUI.margin("PlaneContainer", 4, main_hbox)
	plane_container.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	plane_container.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var game_plane := GwUI.vbox("GamePlane", 4, plane_container)

	var extra_pad := GwUI.margin("ExtraOptionPad", 0, game_plane)
	extra_pad.custom_minimum_size = Vector2(0, 0)
	extra_pad.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	# 战场外层用普通 Control 叠两层：下面 AspectRatioContainer（保持 50:30）+ 上面覆盖层。
	# 原版 `PlaneContainer` 是 AspectRatioContainer，GraphwarPlane 与覆盖层是它的两个子节点；
	# AspectRatioContainer 会把空间**分给**每个子节点，叠不住，所以这里换成显式叠放。
	var plane_wrap := Control.new()
	plane_wrap.name = "PlaneContainer"
	plane_wrap.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	plane_wrap.size_flags_vertical = Control.SIZE_EXPAND_FILL
	plane_wrap.custom_minimum_size = Vector2(200, 120)
	game_plane.add_child(plane_wrap)

	var aspect_holder := AspectRatioContainer.new()
	aspect_holder.name = "AspectKeeper"
	aspect_holder.ratio = BOARD_ASPECT     # 50:30 —— 原版 1.66667 = "平面最小高 = 宽/1.66667"
	aspect_holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
	aspect_holder.set_anchors_preset(Control.PRESET_FULL_RECT)
	plane_wrap.add_child(aspect_holder)

	_plane = GwPlaneView.new()
	aspect_holder.add_child(_plane)

	var overlay := GwUI.vbox("VBoxContainer", 2, plane_wrap)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.alignment = BoxContainer.ALIGNMENT_BEGIN
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_function_pad = GwUI.margin("ViewFunctionPad", 2, overlay)
	_function_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label_row := GwUI.hbox("HBoxContainer", 4, _function_pad)
	var lm := GwUI.margin("MarginContainer", 2, label_row)
	lm.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_view_functions_label = GwUI.rich(16)
	_view_functions_label.name = "ViewFunctionsLabel"
	_view_functions_label.custom_minimum_size = Vector2(0, 40)
	_view_functions_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	lm.add_child(_view_functions_label)

	# 附加按钮行（ExtraOptionsVBox/ExtraOptionsHBox）
	_extra_vbox = GwUI.vbox("ExtraOptionsVBox", 2, plane_container)
	_extra_hbox = GwUI.hbox("ExtraOptionsHBox", 6, _extra_vbox)
	_view_func_button = _extra_button("ViewFunctionButton", "toggle_view_functions", "view_functions")
	_names_button = _extra_button("ShowNames", "toggle_names", "view_names")
	var chat_button := _extra_button("ChatButtonPlay", "toggle_chat", "no_chat")
	_mute_button = _extra_button("MuteButton", "toggle_mute", "unmute")
	for b in [_view_func_button, _names_button, chat_button, _mute_button]:
		_extra_hbox.add_child(b)
	_extra_vbox.visible = false

	# ---------------- 右控制区 ----------------
	var right := GwUI.vbox("RightControls", 6, main_hbox)
	right.custom_minimum_size = Vector2(220, 0)
	right.size_flags_horizontal = Control.SIZE_FILL
	var fire_row := GwUI.hbox("HBoxContainer", 4, right)
	var fire_margin := GwUI.margin("FireContainer", 2, fire_row)
	fire_margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# 原版：FireContainer 里 ClipButton（动画层）与 FireButton（真按钮）**叠在一起**，
	# 两者文本都是 " Fire! "。我们用同样形状的 Control 叠放。
	var fire_stack := Control.new()
	fire_stack.name = "FireStack"
	fire_stack.custom_minimum_size = Vector2(0, 60)
	fire_stack.size_flags_horizontal = Control.SIZE_EXPAND_FILL
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
	_fire_button.tooltip_text = "开火（fire）。原版的 FireButton 只带 long_press_input_text=\"fire\""
	fire_stack.add_child(_fire_button)

	var right_grid_margin := GwUI.margin("MarginContainer", 2, right)
	right_grid_margin.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	var right_grid := GridContainer.new()
	right_grid.name = "RightButtonGrid"
	right_grid.columns = 3
	right_grid.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	right_grid_margin.add_child(right_grid)
	_fill_grid(right_grid, RIGHT_KEYS)
	var right_filler := Control.new()
	right_filler.name = "Filler"
	right_filler.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right.add_child(right_filler)
	# YButton(y)：原版两种模式下都恒为 disabled（docs/92 §4.10 观测）——
	# 为什么不知道（docs/92 §8 明确列为未知），这里照抄"不可按"。
	var y_button := right_grid.get_node_or_null("YButton")
	if y_button != null:
		(y_button as GwButton).disabled = true
		(y_button as GwButton).tooltip_text = "原版里这颗键在 Normal/DiffEq 两种模式下都恒为 disabled（原因未知，docs/92 §8）"

	_replay_right = _build_replay_controls("RightReplaysButtonsGrid", ["save_screenshot", "save_summary", "save_clip"])
	(right as VBoxContainer).add_child(_replay_right)

	# ---------------- 底部信息行 ----------------
	var info := GwUI.hbox("HBoxContainer2", 8, vbox)
	var timer_m := GwUI.margin("TimerContainer", 2, info)
	_timer_label = GwUI.label("00:00", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_timer_label.name = "TimerLabel"
	_timer_label.custom_minimum_size = Vector2(96, 0)
	timer_m.add_child(_timer_label)
	var pad := GwUI.margin("MarginContainer", 2, info)
	pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pad.size_flags_stretch_ratio = 0.25

	var fx_m := GwUI.margin("FxContainer", 2, info)
	var fx_row := GwUI.hbox("FxRow", 4, fx_m)
	_fx_label = GwUI.label(" f(x) = ")
	_fx_label.name = "FxLabel"
	fx_row.add_child(_fx_label)
	_fx_button = GwButton.make("")
	_fx_button.name = "FxTextureButton"
	_fx_button.icon = GwIcons.get_icon("fx_option")
	_fx_button.custom_minimum_size = Vector2(44, 44)
	_fx_button.tooltip_text = "函数模式指示（Normal / DiffEq）。模式由对局设置决定"
	fx_row.add_child(_fx_button)

	_input = GwFunctionInput.new()
	_input.name = "GraphwarFunctionInput"
	_input.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_input.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_input.custom_minimum_size = Vector2(320, 44)
	_input.text_changed.connect(_on_function_text_changed)
	_input.text_submitted.connect(_on_function_text_submitted)
	info.add_child(_input)

	var offset_m := GwUI.margin("OffsetContainer", 2, info)
	_offset_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_offset_label.name = "OffsetLabel"
	_offset_label.custom_minimum_size = Vector2(100, 0)   # 观测：唯一一条 (100,0)
	offset_m.add_child(_offset_label)

	_status_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_LEFT)
	_status_label.name = "StatusLabel"
	_status_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	_status_label.clip_text = true
	_status_label.add_theme_font_size_override("font_size", 16)
	_status_label.add_theme_color_override("font_color", GwThemeFactory.COL_TEXT_DIM)
	vbox.add_child(_status_label)

const BOARD_ASPECT := 50.0 / 30.0

func _anim_rect() -> ColorRect:
	var r := ColorRect.new()
	r.name = "AnimationRect"
	r.color = Color(1, 1, 1, 0.0)
	r.mouse_filter = Control.MOUSE_FILTER_IGNORE
	r.set_anchors_preset(Control.PRESET_FULL_RECT)
	return r

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
		b.custom_minimum_size = Vector2(0, 38)
		grid.add_child(b)

func _extra_button(node_name: String, cmd: String, icon_name: String) -> GwButton:
	var b := GwButton.make("")
	b.name = node_name
	b.input_text = cmd
	b.icon = GwIcons.get_icon(icon_name)
	b.custom_minimum_size = Vector2(52, 52)
	b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
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
		b.custom_minimum_size = Vector2(44, 44)
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		box.add_child(b)
	return box

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
	var team_counts := {0: 0, 1: 0}
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
				"player": pi,
			})
	_active = 0
	_time_left = float(GwModes.turn_seconds(st.time_mode))
	_running = true
	_refresh_mode_indicators()
	_refresh_plane()
	_load_active_function()
	_update_timer_label()
	_replay_left.visible = st.kind == GwGameState.Kind.REPLAY
	_replay_right.visible = st.kind == GwGameState.Kind.REPLAY
	_status("对局开始：%s ／ 目标：把对面打完" % st.room_name)
	if st.kind == GwGameState.Kind.CAMPAIGN:
		_status("战役第 %d 关（占位关卡数据，见 docs/108 §7）" % st.level_index)
	if st.kind == GwGameState.Kind.REPLAY:
		_status("回放模式：回放播放未实现，这里只是空对局（docs/108 §7）")

func end_match() -> void:
	_running = false
	_plane.clear_path()

## 队伍固定在 x 两侧（`docs/105`：flip 由队伍决定，不靠几何猜）。
func _spawn_position(team: int, slot: int) -> Vector2:
	var total := 0
	for s in _soldiers:
		if int(s.team) == team:
			total += 1
	var spread := 3.4
	var y := (slot - 1.5) * spread
	return Vector2(-18.0 if team == 0 else 18.0, y)

func _refresh_plane() -> void:
	var list: Array[Dictionary] = []
	for s in _soldiers:
		list.append(s)
	_plane.set_soldiers(list)
	_plane.set_show_names(_show_names)

func _refresh_mode_indicators() -> void:
	var app_node := app()
	if app_node == null:
		return
	var st = app_node.state
	if st.function_mode == 0:
		_fx_label.text = " f(x) = "
		_fx_button.icon = GwIcons.get_icon("fx_option")
		_fx_label.visible = true
	else:
		# DiffEq：同位置显示 dx/dy 图标，文字前缀消失
		_fx_label.text = ""
		_fx_button.icon = GwIcons.get_icon("dx_dy_option")
		_fx_label.visible = false
	_names_button.icon = GwIcons.get_icon("view_names" if _show_names else "hide_names")
	_view_func_button.icon = GwIcons.get_icon("view_functions" if _show_function_overlay else "hide_extra_buttons")

# ---------------------------------------------------------------- 输入与开火

func key_input(cmd: String) -> void:
	if not _running:
		_status("对局未开始")
		return
	_input.key_input(cmd)
	_functions[_active] = _input.text

func _on_function_text_changed(new_text: String) -> void:
	_functions[_active] = new_text
	if not _suppress_offset_clear:
		_offset_label.text = ""   # 手改函数 → 上一发的修正不再对应当前输入
	_refresh_function_list()

func _on_function_text_submitted(_new_text: String) -> void:
	# 原版：GraphwarFunctionInput.text_submitted → on_text_submitted（Rust）
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
	var impact: Vector2 = pts[pts.size() - 1] if pts.size() > 0 else active_soldier.pos
	_plane.set_path(pts, impact, true)
	_offset_label.text = GwCore.format_offset(float(result.offset)) if app_node.state.function_mode == 0 else ""
	_status("弹道来源：%s ／ %s" % [String(result.source), String(result.note)])

	var shot := {"points": pts, "team": team, "shooter": _active, "soldier": active_soldier}
	if int(app_node.state.turn_mode) == GwModes.Turn.SIMULTANEOUS:
		# 同时回合：先全部记下，轮完再结算（真实结算在 src/settle.rs，docs/86）
		_shot_queue.append(shot)
		if _all_alive_players_shot():
			_resolve_shots(_shot_queue)
			_shot_queue.clear()
			_next_turn(true)
		else:
			_next_turn(false)
	else:
		_resolve_shots([shot])
		_next_turn(true)
	_check_match_over()

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
## 障碍光栅化进 1000×600 掩膜决定弹道停在哪；士兵**不**挡弹道，只在
## "弹道点落在半径 0.5 内"时判死（`PosInfo.radius` 恒 0.5，与画出来的 0.8 无关）。
## 射手本人不参与致死（`settle` 的约定：子弹从自己身上出发）。
##
## 内核缺席时 `GwCore.hit_test()` 会退回几何近似，状态行必须把 `source` 写出来 ——
## 不允许把近似当成内核算的。原来的纯近似版本登记在 `docs/108` §7，本函数是它的替代。
func _resolve_shots(shots: Array) -> void:
	var kills := 0
	var sources: Array[String] = []
	var note := ""
	for shot in shots:
		var pts: PackedVector2Array = shot.points
		var targets: Array = []
		for s in _soldiers:
			if not bool(s.alive):
				continue
			if is_same(s, shot.get("soldier", null)):
				continue                      # 射手本人不参与致死判定
			targets.append({"pos": s.pos, "radius": GwCore.SOLDIER_HIT_RADIUS, "soldier": s})
		if targets.is_empty():
			continue
		var judged := GwCore.hit_test(pts, targets, [])
		sources.append(String(judged.source))
		note = String(judged.note)
		for idx in judged.hits:
			var victim: Dictionary = targets[int(idx)].soldier
			if bool(victim.alive):
				victim.alive = false
				kills += 1
	if not sources.is_empty():
		_status("本轮命中 %d 个士兵 ／ 判定来源：%s ／ %s" % [kills, sources[0], note])
	if kills > 0:
		_refresh_plane()

func _active_soldier() -> Dictionary:
	var app_node := app()
	if app_node == null:
		return {}
	var active_player := _active
	var found := {}
	for s in _soldiers:
		if int(s.player) == active_player:
			found = s
			break
	return found

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
	# 注意：**不**在这里清 OffsetLabel —— 原版换手时 OffsetLabel 仍显示上一发的修正
	# （观测：Normal 下一直是 ` + 2.515 ` 这种值；只有 DiffEq 下为空）。

func _check_match_over() -> void:
	var alive := {0: 0, 1: 0}
	for s in _soldiers:
		if bool(s.alive):
			alive[int(s.team)] = int(alive[int(s.team)]) + 1
	if int(alive[0]) > 0 and int(alive[1]) > 0:
		return
	var app_node := app()
	if app_node == null:
		return
	_running = false
	var human_team := 0
	for p in app_node.state.players:
		if not bool(p.is_bot):
			human_team = int(p.team)
			break
	var winner := 0 if int(alive[0]) > 0 else 1
	app_node.report_match_result(winner == human_team, winner)

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

func _update_timer_label() -> void:
	if _timer_label == null:
		return
	var app_node := app()
	if app_node == null:
		return
	var limit := GwModes.turn_seconds(app_node.state.time_mode)
	if limit <= 0:
		_timer_label.text = "∞"
		return
	_timer_label.text = GwModes.format_clock(_time_left)

# ---------------------------------------------------------------- 附加按钮与显示开关

## 复刻 `ShowExtraButtons.gd`（`docs/101` §4.2 有真源码）：
## 按下 → 取反、切换附加按钮区、换图标、并在这个空间不足时把自己置灰。
func _toggle_extra_buttons() -> void:
	_extra_visible = not _extra_visible
	_extra_vbox.visible = _extra_visible
	_show_extra_button.icon = GwIcons.get_icon("hide_extra_buttons" if _extra_visible else "show_extra_buttons")

func toggle_names() -> void:
	_show_names = not _show_names
	_plane.set_show_names(_show_names)
	_names_button.icon = GwIcons.get_icon("view_names" if _show_names else "hide_names")
	_status("名字显示：%s" % ("开" if _show_names else "关"))

func toggle_view_functions() -> void:
	_show_function_overlay = not _show_function_overlay
	_view_functions_label.visible = _show_function_overlay
	_view_func_button.icon = GwIcons.get_icon("view_functions" if _show_function_overlay else "hide_extra_buttons")
	_status("顶部函数列表：%s" % ("开" if _show_function_overlay else "关"))

func toggle_mute() -> void:
	# 本项目没有音频资源（.pck 里有一个损坏文件，其余也不入库）→ 只切图标
	var muted := _mute_button.icon == GwIcons.get_icon("unmute")
	_mute_button.icon = GwIcons.get_icon("mute" if muted else "unmute")

func _refresh_function_list() -> void:
	if _view_functions_label == null:
		return
	var app_node := app()
	if app_node == null:
		return
	var lines: Array[String] = []
	for pi in app_node.state.players.size():
		var p: Dictionary = app_node.state.players[pi]
		var fn := String(_functions.get(pi, ""))
		if fn == "":
			fn = "—"
		var color := "green" if pi == _active else "white"
		lines.append("[color=%s]%s: f(x) = %s[/color]" % [color, String(p.name), fn])
		# 观测到的格式：[outline_size=4][outline_color=black][color=green]player 1: f(x) = 3x+234[/color]
	_view_functions_label.clear()
	_view_functions_label.append_text("[outline_size=4][outline_color=black]%s[/outline_color][/outline_size]"
		% "\n".join(lines))

func _status(text_: String) -> void:
	if _status_label != null:
		_status_label.text = text_

func on_enter() -> void:
	if not _running:
		begin_match()
