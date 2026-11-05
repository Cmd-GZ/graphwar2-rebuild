extends GwScreen
## GameSetup —— 对局设置（建房）。
##
## 结构（`docs/92` §4.5，恢复出的 GameSetup.tscn 47 节点；根节点是 `Control`，
## 里面套一个 Rust 控件 `GraphwarSetupControl`）：
##   GraphwarSetupControl/MarginContainer/VBoxContainer
##     ├ HBoxContainer
##     │    ├ ExitButtonContainer (exit_game_setup)
##     │    ├ MarginContainer4/GhostButton("         ")      ← 标题占位（LanLobbyTitle.gd）
##     │    ├ RoomName/RoomNameLineEdit("Graphwar Game")     ← Rust 类，text_changed→on_text_change
##     │    ├ MarginContainer2/ChatButtonSetup (toggle_chat, icon no_chat)
##     │    └ MarginContainer3/LockButton (lock_game, icon unlock/lock)
##     ├ MarginContainer2/ScrollContainer/PlayerListContainer
##     │    ├ Team1Container ("Team 1") + 若干玩家行
##     │    ├ MarginContainer/ColorRect(10,0)
##     │    └ Team2Container ("Team 2") + 若干玩家行
##     ├ OptionsContainer（Axis/Function/Turn/TurnTime/AddBot 五个按钮）
##     ├ MarginContainer/HBoxContainer/{GameIpLabel, StartButton(" Start "), …}
##     └ AdContainer(0,60)
##
## 玩家行（原版 `team1_entry.tscn` 48 节点 / `team2_entry.tscn` 46 节点的差别只在图标方向）：
##   RemovePlayerButton(x) · RankIcon · PlayerNameLabel · AddSoldierButton(+) ·
##   5 个士兵槽（每槽 Skin/Face/Hat/Empty 四个 TextureRect）·
##   RemoveSoldierButton(−) · ChangeTeamButton(→ / ←)

## 观测：端口号每个进程不同（`docs/92` §4.5 `GameIpLabel = 198.18.0.1:36071`）。
## 本项目没有网络层，所以这里显示的是**明写的占位**，不是真实监听地址。
const PLACEHOLDER_IP := "127.0.0.1:0 (no network layer)"

var _room_edit: LineEdit
var _chat_button: GwButton
var _lock_button: GwButton
var _team1: VBoxContainer
var _team2: VBoxContainer
var _mode_buttons := {}
var _ip_label: Label
var _start_button: GwButton

func _ready() -> void:
	# 原版这一屏的字号来源（每个容器挂不同脚本，docs/137 §5）：
	#   RoomName.gd / LanLobbyTitle.gd（队名、GhostButton）/ LobbyButtonContainer.gd（底栏）
	#   / ExitButtonContainer.gd  —— 全是 **宽/30**；
	#   只有玩家行 `GameSetupPlayerEntry.gd` 是 **宽/40**（见 _player_row）。
	# 以前整屏写 40，于是标题/队名/Start 全部小一号（真帧 64 vs 我们 48）。
	font_divisor = 30
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var rust_root := Control.new()
	rust_root.name = "GraphwarSetupControl"
	rust_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	rust_root.size_flags_vertical = Control.SIZE_EXPAND_FILL
	f.vbox.add_child(rust_root)
	var vbox := GwUI.vbox("VBoxContainer", GwUI.SEPARATION, rust_root)
	# rust_root 是普通 Control（不是容器），子节点得自己铺满 —— 否则整个内容会缩成最小尺寸。
	vbox.set_anchors_preset(Control.PRESET_FULL_RECT)

	# ---------------- 顶栏 ----------------
	var top := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, vbox)
	top.add_child(GwUI.exit_pair(GwCommands.EXIT_GAME_SETUP))

	# GhostButton：原版是 `invisible_theme` 的 GraphwarButton，9 个空格 ⇒ 真帧宽 **159**
	# （`docs/137` §5，宿主导出的 `@@NODE GhostButton ... size=(159,96)`）。它不画任何东西，
	# 唯一作用就是把标题往右顶 159 px。
	var ghost_m := GwUI.margin("MarginContainer4", 0, top)
	ghost_m.size_flags_horizontal = Control.SIZE_FILL
	# 宽度直接钉成真值 159：`GraphwarButton`（Rust）对"9 个空格"的度量与我们 Godot
	# `Button` 不同（我们的空格是 28 px/个 ⇒ 252），只有钉住才能让标题落在真帧的 x=313。
	var ghost := GwButton.make("")
	ghost.name = "GhostButton"
	ghost.custom_minimum_size = Vector2(159, 96)
	ghost.disabled = true
	ghost.flat = true
	ghost.tooltip_text = "原版这里是空白占位按钮（LanLobbyTitle.gd 只调字号）"
	ghost_m.add_child(ghost)

	var room_m := GwUI.hbox("RoomName", GwUI.SEPARATION, top)
	room_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_room_edit = LineEdit.new()
	_room_edit.name = "RoomNameLineEdit"
	_room_edit.text = "Graphwar Game"
	# 原版这个 LineEdit 挂 `title_theme.tres`：`default_font = EraserRegular`（手写体）+
	# 三个 stylebox 全是 `StyleBoxEmpty`（没有框）。字号由 `RoomName.gd` 设成 宽/30 = 64。
	# 手写体的字形只有大写 ⇒ 真帧上显示成 `GRAPHWAR GAME`（不是 .tscn 里那个大小写）。
	var tf := GwThemeFactory.title_font()
	if tf != null:
		_room_edit.add_theme_font_override("font", tf)
	_room_edit.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	_room_edit.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	_room_edit.alignment = HORIZONTAL_ALIGNMENT_CENTER
	_room_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_room_edit.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_room_edit.tooltip_text = "房间名。原版这个 LineEdit 是 Rust 类，text_changed→on_text_change（docs/104 §6.3）"
	_room_edit.text_changed.connect(_on_room_name_changed)
	room_m.add_child(_room_edit)

	# 聊天 / 上锁：原版是 `text = "     "`（5 个空格）+ `icon` + `expand_icon`，
	# 与返回键同一套 ⇒ 真帧**都是 146×96**（宿主导出）。
	var chat_m := GwUI.margin("MarginContainer2", 0, top)
	chat_m.size_flags_horizontal = Control.SIZE_FILL
	_chat_button = GwButton.make("     ")
	_chat_button.name = "ChatButtonSetup"
	_chat_button.input_text = GwCommands.TOGGLE_CHAT
	_chat_button.icon = GwIcons.get_icon("no_chat")
	_chat_button.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_chat_button.expand_icon = true
	_chat_button.tooltip_text = "聊天（toggle_chat）"
	chat_m.add_child(_chat_button)

	var lock_m := GwUI.margin("MarginContainer3", 0, top)
	lock_m.size_flags_horizontal = Control.SIZE_FILL
	_lock_button = GwButton.make("     ")
	_lock_button.name = "LockButton"
	_lock_button.input_text = GwCommands.LOCK_GAME
	_lock_button.icon = GwIcons.get_icon("unlock")
	_lock_button.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_lock_button.expand_icon = true
	lock_m.add_child(_lock_button)

	# ---------------- 玩家列表 ----------------
	var list_m := GwUI.margin("MarginContainer2", 0, vbox)
	list_m.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# 原版这里套了一层 `ScrollContainer`；本项目**不用**它：ScrollContainer 的最小尺寸等于
	# 内容最小尺寸，两列玩家表会把整屏撑爆（实测：整个 GameSetup 被推出 1280×720）。
	# 代价是玩家很多时不会滚动（下面 ROW_LIMIT 之外的行只能靠缩小窗口看）。见 docs/108 §7。
	var players_row := GwUI.hbox("PlayerListContainer", GwUI.SEPARATION, list_m)
	players_row.size_flags_vertical = Control.SIZE_EXPAND_FILL

	_team1 = GwUI.vbox("Team1Container", 6, players_row)
	_team1.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_team1.size_flags_stretch_ratio = 1.0
	_team1.add_child(_team_title("Team 1"))
	# 中缝：原版是 `MarginContainer/ColorRect(custom_minimum_size = (10,0))`，
	# ColorRect **没写 color** ⇒ 默认白。真帧量到 x 955..964 共 **10 px 纯白**（docs/137 §5）。
	# 我们以前放的是一个不可见的 Control ⇒ 真帧上那道白线整条没有。
	var divider_m := GwUI.margin("MarginContainer", 0, players_row)
	divider_m.size_flags_horizontal = Control.SIZE_FILL
	var divider := ColorRect.new()
	divider.name = "ColorRect"
	divider.color = Color(1, 1, 1)
	divider.custom_minimum_size = Vector2(10, 0)
	divider_m.add_child(divider)
	_team2 = GwUI.vbox("Team2Container", 6, players_row)
	_team2.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_team2.size_flags_stretch_ratio = 1.0
	_team2.add_child(_team_title("Team 2"))

	# ---------------- 选项按钮 ----------------
	# 原版 `OptionsContainer` 的 11 个子节点是**交错**的：6 个空 `MarginContainer`(h=3)
	# 夹 5 个按钮 —— 按钮是 Margin 的**兄弟**，不是子节点（宿主导出的真值：
	# Margin 201/202、按钮 134/134，两者相加 + 10×4 分隔 = 1920）。
	# 我们以前把按钮塞进 Margin 里，于是按钮撑满那一格（实测 316 宽）。
	var options := GwUI.hbox("OptionsContainer", GwUI.SEPARATION, vbox)
	var slot := 0
	for spec in [
		{"key": "axis", "name": "AxisModeButton"},
		{"key": "function", "name": "FunctionModeButton"},
		{"key": "turn", "name": "TurnModeButton"},
		{"key": "time", "name": "TurnTimeButton"},
	]:
		_gap(options, "MarginContainer" if slot == 0 else "MarginContainer%d" % slot)
		slot += 1
		var b := GwButton.make("")
		b.name = String(spec.name)
		b.size_flags_horizontal = Control.SIZE_FILL
		# 原版这五个按钮**没有** `expand_icon` ⇒ 图标参与最小尺寸：
		# 128 px 图标 + stylebox 两侧 3 px = **134×134**（宿主导出的真值）。
		b.expand_icon = false
		b.icon = GwIcons.get_icon("axis_option_3")
		if String(spec.key) == "axis":
			# 原版这个按钮单独挂 `axis_button_theme.tres`：normal/hover/pressed/disabled
			# 四个 stylebox 全是 `axis_menu_icon_theme.tres` = **白底** + 3 px 黑边 + 圆角 10。
			# 于是"轴模式"那块在真帧上是**白卡**（其余四个选项是深绿卡）。
			var white_sb := GwThemeFactory._flat(Color(1, 1, 1))
			for st in ["normal", "hover", "pressed", "disabled"]:
				b.add_theme_stylebox_override(st, white_sb)
		b.pressed.connect(_cycle_mode.bind(String(spec.key)))
		options.add_child(b)
		_mode_buttons[String(spec.key)] = b

	_gap(options, "MarginContainer5")
	var add_bot := GwButton.make("")
	add_bot.name = "AddBotButton"
	add_bot.input_text = GwCommands.ADD_BOT
	add_bot.expand_icon = false
	add_bot.icon = GwIcons.get_icon("add_bot_option")
	add_bot.tooltip_text = "加 Bot（add_bot → AddBotPopUp，六档）"
	options.add_child(add_bot)
	# 选项行最后一个**空的**弹性 MarginContainer
	_gap(options, "MarginContainer6")

	# ---------------- 底部 ----------------
	# 原版 `LobbyButtonContainer.gd`：**每个**子节点 custom_minimum_size = (屏宽×0.2, 0)
	# = (384, 0)，字号 宽/30。⇒ Start 真帧 (768,0) 384×96；左右两个 384 的空位把居中撑出来。
	var bottom := GwUI.margin("MarginContainer", 0, vbox)
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION, bottom)
	var ip_m := GwUI.margin("MarginContainer", 0, row)
	ip_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	ip_m.custom_minimum_size = Vector2(384, 0)
	_ip_label = GwUI.label("")
	_ip_label.name = "GameIpLabel"
	_ip_label.tooltip_text = ("原版这里显示本机房间的 IP:端口（docs/92 §4.5 观测到 198.18.0.1:36071）。"
		+ "本项目没有网络层，故显示占位")
	ip_m.add_child(_ip_label)
	_start_button = GwButton.make(" Start ", GwCommands.START_GAME)
	_start_button.name = "StartButton"
	_start_button.custom_minimum_size = Vector2(384, 0)
	row.add_child(_start_button)
	var pad := GwUI.margin("MarginContainer2", 0, row)
	pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pad.custom_minimum_size = Vector2(384, 0)

	vbox.add_child(GwUI.ad_bar())

## 选项行里的弹性空格（原版就是空的 `MarginContainer`，没有子节点）
func _gap(parent: HBoxContainer, node_name: String) -> MarginContainer:
	var m := GwUI.margin(node_name, 0, parent)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return m

func _team_title(text_: String) -> MarginContainer:
	# 原版队名：`MarginContainer(LanLobbyTitle.gd)` → `Label(theme = title_theme.tres)`，
	# 字号 宽/30 = 64，手写体（→ 渲染成大写 TEAM 1 / TEAM 2）。真帧墨迹 Team1 x 354..594。
	var m := GwUI.margin("MarginContainer", 0)
	var l := GwUI.label(text_, 0, HORIZONTAL_ALIGNMENT_CENTER)
	l.name = "Label"
	var tf := GwThemeFactory.title_font()
	if tf != null:
		l.add_theme_font_override("font", tf)
	m.add_child(l)
	return m

func _on_room_name_changed(new_text: String) -> void:
	var app_node := app()
	if app_node != null:
		app_node.state.room_name = new_text

## 四个选项按钮的循环（顺序见 `modes.gd`，数值来自 `docs/92` §6 的连点枚举）。
func _cycle_mode(key: String) -> void:
	var app_node := app()
	if app_node == null:
		return
	var st = app_node.state
	match key:
		"axis": st.axis_mode = GwModes.axis_next(st.axis_mode)
		"function": st.function_mode = GwModes.function_next(st.function_mode)
		"turn": st.turn_mode = GwModes.turn_next(st.turn_mode)
		"time": st.time_mode = GwModes.time_next(st.time_mode)
	_refresh_modes()

func _refresh_modes() -> void:
	var app_node := app()
	if app_node == null:
		return
	var st = app_node.state
	(_mode_buttons["axis"] as GwButton).icon = GwIcons.get_icon(GwModes.axis_icon(st.axis_mode))
	(_mode_buttons["axis"] as GwButton).tooltip_text = "Axis mode %d / 4（档位语义未知）" % st.axis_mode
	(_mode_buttons["function"] as GwButton).icon = GwIcons.get_icon(GwModes.function_icon(st.function_mode))
	(_mode_buttons["function"] as GwButton).tooltip_text = "Function mode: %s" % ("f(x)" if st.function_mode == 0 else "dx/dy")
	(_mode_buttons["turn"] as GwButton).icon = GwIcons.get_icon(GwModes.turn_icon(st.turn_mode))
	(_mode_buttons["turn"] as GwButton).tooltip_text = "Turn mode: %s" % ("sequential" if st.turn_mode == 0 else "simultaneous")
	(_mode_buttons["time"] as GwButton).icon = GwIcons.get_icon(GwModes.time_icon(st.time_mode))
	(_mode_buttons["time"] as GwButton).tooltip_text = "Turn time: %s" % GwModes.time_label(st.time_mode)

func on_enter() -> void:
	var app_node := app()
	if app_node == null:
		return
	_room_edit.text = app_node.state.room_name
	_lock_button.icon = GwIcons.get_icon("lock" if app_node.state.locked else "unlock")
	_lock_button.tooltip_text = "房间%s（lock_game）" % ("已上锁" if app_node.state.locked else "未上锁")
	_refresh_modes()
	_refresh_players()

func _refresh_players() -> void:
	for child in _team1.get_children():
		if child.name != "MarginContainer":
			child.queue_free()
	for child in _team2.get_children():
		if child.name != "MarginContainer":
			child.queue_free()
	var app_node := app()
	if app_node == null:
		return
	var index := 0
	for p in app_node.state.players:
		var row := _player_row(index, p)
		if int(p.team) == 0:
			_team1.add_child(row)
		else:
			_team2.add_child(row)
		index += 1
	_start_button.disabled = not app_node.state.can_start()
	_start_button.tooltip_text = ("开始对局（start_game）。原版不加 Bot 直接 Start 会静默无操作"
		if app_node.state.can_start() else "原版行为：没有对手时 Start 静默无操作（docs/22 §5.1）")

## 一行玩家（原版是 `team1_entry.tscn` / `team2_entry.tscn` 的实例）。
func _player_row(index: int, p: Dictionary) -> HBoxContainer:
	var row := GwUI.hbox("HBoxContainer", 4)
	row.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	# 踢出
	var remove := GwButton.make("")
	remove.name = "RemovePlayerButton"
	remove.icon = GwIcons.get_icon("x")
	remove.tooltip_text = "踢出这名玩家"
	remove.pressed.connect(_on_remove_player.bind(index))
	row.add_child(_icon_box("AspectRatioContainer", remove, 30.0))

	# 段位图标：原版是 ranks/rank_N.svg（玩家）或 hats/no_hat.svg（Bot）。
	# 段位数据未知 → 本项目用一个中性的圆点占位。
	var rank := TextureRect.new()
	rank.expand_mode = TextureRect.EXPAND_IGNORE_SIZE   # 图标是 512×512 导入产物，不忽略会撑爆最小尺寸
	rank.name = "RankIcon"
	rank.custom_minimum_size = Vector2(24, 24)
	rank.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	rank.tooltip_text = "段位图标（原版 ranks/rank_N.svg；段位系统未实现）"
	row.add_child(rank)

	var name_m := GwUI.margin("MarginContainer", 4, row)
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_m.custom_minimum_size = Vector2(80, 0)
	var name_label := GwUI.label(String(p.name), 0, HORIZONTAL_ALIGNMENT_LEFT)
	name_label.clip_text = true
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.name = "PlayerNameLabel"
	name_m.add_child(name_label)

	# 加士兵
	var add := GwButton.make("")
	add.name = "AddSoldierButton"
	add.icon = GwIcons.get_icon("plus")
	add.tooltip_text = "该玩家加一个士兵（最多 %d）" % GwGameState.MAX_SOLDIERS
	add.pressed.connect(_on_add_soldier.bind(index))
	row.add_child(_icon_box("AspectRatioContainer2", add, 28.0))

	# 5 个士兵槽（观测：team1_entry.tscn 里正是 5 个 MarginContainer）
	var slots := GwUI.hbox("SoldierIconContainer", 2, row)
	slots.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	slots.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for s in GwGameState.MAX_SOLDIERS:
		var slot := GwUI.margin("MarginContainer", 1, slots)
		# 40×40：原版 svg 的画布留白很大（士兵本体只占中间 ~45%），
		# 用 24 会缩成一个小点（实测对比，见 docs/121 §7.2）。
		slot.custom_minimum_size = Vector2(40, 40)
		slot.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
		var icon := TextureRect.new()
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.name = "SkinIcon" if s < int(p.soldiers) else "EmptyIcon"
		icon.custom_minimum_size = Vector2(38, 38)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		if s < int(p.soldiers):
			icon.texture = GwIcons.get_icon("skin_%d" % int(p.skin) if int(p.skin) >= 0 else "bot_%d" % int(p.bot_tier))
		else:
			icon.texture = GwIcons.get_icon("empty_soldier")
		slot.add_child(icon)

	# 减士兵
	var sub := GwButton.make("")
	sub.name = "RemoveSoldierButton"
	sub.icon = GwIcons.get_icon("minus")
	sub.tooltip_text = "减一个士兵"
	sub.pressed.connect(_on_remove_soldier.bind(index))
	row.add_child(_icon_box("AspectRatioContainer3", sub, 28.0))

	# 换队
	var swap := GwButton.make("")
	swap.name = "ChangeTeamButton"
	swap.icon = GwIcons.get_icon("arrow_right" if int(p.team) == 0 else "arrow_left")
	swap.tooltip_text = "换到对面队伍"
	swap.pressed.connect(_on_change_team.bind(index))
	row.add_child(_icon_box("AspectRatioContainer4", swap, 28.0))

	return row

## 原版用 AspectRatioContainer 包这些小方按钮；AspectRatioContainer 会把宽度吃掉
## （宽度 = 行高），两列玩家表就撑爆了。这里换成定尺寸的 Control，按钮铺满它。
func _icon_box(box_name: String, button: Control, px: float) -> Control:
	var box := Control.new()
	box.name = box_name
	box.custom_minimum_size = Vector2(px, px)
	box.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	box.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	button.set_anchors_preset(Control.PRESET_FULL_RECT)
	box.add_child(button)
	return box

func _on_remove_player(index: int) -> void:
	var app_node := app()
	if app_node == null:
		return
	app_node.state.remove_player(index)
	app_node.changed()
	_refresh_players()

func _on_add_soldier(index: int) -> void:
	var app_node := app()
	if app_node == null or index >= app_node.state.players.size():
		return
	var p: Dictionary = app_node.state.players[index]
	if int(p.soldiers) < GwGameState.MAX_SOLDIERS:
		p.soldiers = int(p.soldiers) + 1
		app_node.changed()
		_refresh_players()

func _on_remove_soldier(index: int) -> void:
	var app_node := app()
	if app_node == null or index >= app_node.state.players.size():
		return
	var p: Dictionary = app_node.state.players[index]
	if int(p.soldiers) > 1:
		p.soldiers = int(p.soldiers) - 1
		app_node.changed()
		_refresh_players()

func _on_change_team(index: int) -> void:
	var app_node := app()
	if app_node == null or index >= app_node.state.players.size():
		return
	var p: Dictionary = app_node.state.players[index]
	p.team = 1 - int(p.team)
	app_node.changed()
	_refresh_players()
