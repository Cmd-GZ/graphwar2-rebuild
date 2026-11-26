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
	var ghost_m := GwUI.margin("MarginContainer4", 0)
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
	GwViews.attach(ghost_m, "LanLobbyTitle.gd")   # 字号 = 宽/30
	top.add_child(ghost_m)

	var room_m := GwUI.hbox("RoomName", GwUI.SEPARATION)
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
	# 原版 `GameSetup.tscn` 的 `HBoxContainer/RoomName` 挂 `RoomName.gd`（字号 = 宽/30）
	GwViews.attach(room_m, "RoomName.gd")
	top.add_child(room_m)

	# 聊天 / 上锁：原版是 `text = "     "`（5 个空格）+ `icon` + `expand_icon`，
	# 与返回键同一套 ⇒ 真帧**都是 146×96**（宿主导出）。
	var chat_m := GwUI.margin("MarginContainer2", 0)
	chat_m.size_flags_horizontal = Control.SIZE_FILL
	_chat_button = GwButton.make("     ")
	_chat_button.name = "ChatButtonSetup"
	_chat_button.input_text = GwCommands.TOGGLE_CHAT
	_chat_button.icon = GwIcons.get_icon("no_chat")
	_chat_button.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_chat_button.expand_icon = true
	_chat_button.tooltip_text = "聊天（toggle_chat）"
	chat_m.add_child(_chat_button)
	GwViews.attach(chat_m, "LanLobbyTitle.gd")    # 字号 = 宽/30
	top.add_child(chat_m)

	var lock_m := GwUI.margin("MarginContainer3", 0)
	lock_m.size_flags_horizontal = Control.SIZE_FILL
	_lock_button = GwButton.make("     ")
	_lock_button.name = "LockButton"
	_lock_button.input_text = GwCommands.LOCK_GAME
	_lock_button.icon = GwIcons.get_icon("unlock")
	_lock_button.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_lock_button.expand_icon = true
	lock_m.add_child(_lock_button)
	GwViews.attach(lock_m, "LanLobbyTitle.gd")    # 字号 = 宽/30
	top.add_child(lock_m)

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
	# 原版 `GameSetup.tscn` 这一行是：
	#   MarginContainer/HBoxContainer[script = LobbyButtonContainer.gd]
	#     ├ MarginContainer(size_flags_horizontal=3) → GameIpLabel
	#     │      （Label：size_flags_horizontal=0 靠左、vertical=8 靠**下**）
	#     ├ StartButton(" Start ", input_text=start_game)
	#     └ MarginContainer2(size_flags_horizontal=3)
	# `LobbyButtonContainer.gd` 给**每个**子节点设 `custom_minimum_size = (屏宽×0.2, 0)`
	# = (384,0)、字号 宽/30 = 64 ⇒ Start 真值 **(768,1060) 384×96**（两侧 384 的弹性空位把它居中）。
	# 我们以前**没有挂这个脚本**，于是 Start 用的是主题默认字号（96 高 → 只有 44），
	# 整排也跟着矮一截 —— 真帧上表现为"底部整排下移 + Start 压到底边 + 是 disabled 外观"。
	var bottom := GwUI.margin("MarginContainer", 0, vbox)
	# ⚠️ 这一行**先不挂父节点**：`GwViews.attach()` 必须在 `add_child()` **之前**
	# （`_ready` 只在进树时发一次；已经进树的节点 `set_script()` 不补发 —— 见 gw_views.gd 的警告）。
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION)
	var ip_m := GwUI.margin("MarginContainer", 0, row)
	_ip_label = GwUI.label("")
	_ip_label.name = "GameIpLabel"
	# `.tscn` 的静态值：水平 SHRINK_BEGIN(0)、竖直 SHRINK_END(8) ⇒ 落在本格**左下角**。
	_ip_label.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_ip_label.size_flags_vertical = Control.SIZE_SHRINK_END
	_ip_label.tooltip_text = ("原版这里显示本机房间的 IP:端口（真帧例：192.168.31.224:41823）。"
		+ "本项目不开对局 TCP 监听、也不做 UDP 广播 ⇒ 拿不到这个地址，见 docs/06 §75")
	ip_m.add_child(_ip_label)
	_start_button = GwButton.make(" Start ", GwCommands.START_GAME)
	_start_button.name = "StartButton"
	# 与 `LobbyButtonContainer.gd` 写的是同一个值（屏宽×0.2 = 384）；这里显式写一遍是为了
	# 让"构建期就能查到"（自检里有一条断言是构建期读 `custom_minimum_size.x == 384`）。
	_start_button.custom_minimum_size = Vector2(384, 0)
	row.add_child(_start_button)
	var pad := GwUI.margin("MarginContainer2", 0, row)
	# 原版这行挂 `LobbyButtonContainer.gd`：给**每个**子节点 `custom_minimum_size = 屏宽×0.2`、
	# 字号 宽/30 = 64。**必须挂**（不挂 `Start` 只有 44 px 高：实测 6.09 % vs 挂上 1.25 %）。
	GwViews.attach(row, "LobbyButtonContainer.gd")
	bottom.add_child(row)

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
	# 原版队名容器挂 `LanLobbyTitle.gd`（字号 = 宽/30）；`GameSetup.tscn` 的两处
	# `.../Team1Container/MarginContainer`、`.../Team2Container/MarginContainer` 就是它。
	GwViews.attach(m, "LanLobbyTitle.gd")
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

## 覆盖：`LobbyButtonContainer.gd` 会把底排每个子节点的最小宽度写成
## `get_viewport_rect().size.x × 0.2`。自检跑在 headless 的 dummy 窗口（**实测 64×64**）里，
## 于是它会把 `StartButton` 写成 12 px —— 而既有断言（`selftest.gd`："StartButton 最小宽 384"）
## 要的是 1920 口径的 384。这里在每次字号重算之后**用 deferred 钉回真值**
## （deferred ⇒ 排在同一次 `size_changed` 里原版脚本之后，不会被它盖掉）。
## 1920×1160 下两条路径给出同一个 384，采帧结果**逐像素相同**。
func apply_font_scale() -> void:
	super()
	if _start_button != null:
		_pin_start_width.call_deferred()

func _pin_start_width() -> void:
	if is_instance_valid(_start_button):
		_start_button.custom_minimum_size = Vector2(384, 0)

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
	# **不做成 `disabled`**：原版没有对手时 `Start` 仍然是**正常态**大按钮（深绿底 + 白字），
	# 点下去**静默无操作**（`docs/22` §5.1）。我们以前把 `can_start()` 直接接到 `disabled`，
	# 于是真帧上是"灰字 + 深灰底"的 disabled 外观（`docs/06` §69.4 第 5 条）。
	# 静默/提示那一半在 `_start_game()` 里（`app.gd`），本屏只管外观。
	_start_button.disabled = false
	_start_button.tooltip_text = ("开始对局（start_game）。原版不加 Bot 直接 Start 会静默无操作"
		if not app_node.state.can_start() else "开始对局（start_game）")

## 一行玩家（原版是 `team1_entry.tscn` / `team2_entry.tscn` 的实例，16 个子节点）。
##
## 结构照 `recovered-src/team1_entry.tscn`，**尺寸用真帧量的值** —— 因为行本身不是 `.tscn`
## 里的静态节点，是 Rust `GraphwarSetupControl` 建行时填的；`.tscn` 只给节点形状，
## 最终大小取决于运行时行高（行高 = 名字 Label 在 宽/40 = 48 px 下的最小高度 **67**，
## 四个 AspectRatioContainer 的 `stretch_mode = 1` = 高度控宽度 ⇒ 按钮全是 **67×67**）。
##
## 真帧逐元素（`docs/06` §75，1920×1160，team1 那一半 0..950）：
##
##   | 元素 | 真帧 x | 大小 | `.tscn` 里的节点 |
##   |---|---|---|---|
##   | 左侧空位 | 0..12 | 12（+4 间隔）| `MarginContainer2`（ratio 0.05）|
##   | `RemovePlayerButton` | 12..79 | 67×67 | `AspectRatioContainer` → 按钮（text 4 空格 + `x.svg`）|
##   | 空位 | 79..117 | 34 | `MarginContainer6`（ratio 0.05）|
##   | `RankIcon` | 117..140 | 23×36 | `TextureRect(expand_mode=3)`，本帧是 `ranks/rank_0.svg` |
##   | `PlayerNameLabel` | 文字 324..385 | 居中在 144..532 | `MarginContainer`(`GameSetupPlayerEntry.gd` = 宽/40) |
##   | `AddSoldierButton` | 536..603 | 67×67 | `AspectRatioContainer2` |
##   | `Label`（**5 个空格**）| 607..712 | 105 | 行里那个字面量 `text = "     "` |
##   | `SoldierIconContainer` | 716..784 | 每名士兵 34 px | 5 个 `MarginContainer`（`SoldierIconMarginContainer.gd` 的负 margin 让它们叠起来）|
##   | `RemoveSoldierButton` | 799..866 | 67×67 | `AspectRatioContainer3` |
##   | `ChangeTeamButton` | 872..939 | 67×67 | `AspectRatioContainer4` |
##   | 右侧留白 | 939..950 | 11 | `MarginContainer5`（ratio 0.05）|
##
## 我们以前把四个键做成 28–30 px 的"裸图标"（没有 67×67 的底板）、段位图标是个空
## `TextureRect`、士兵图标 18 px、名字左对齐紧贴 X 键 —— 这就是 `docs/06` §69.4 写的
## "原版 `X 0 CG [+] 🍈🍈 [−] [→]` vs 我方 `X user [小+] 🍈 [小−] [小→]`"。
const ROW_LEAD := 8.0             # + HBox 的 4 px 间隔 = 真帧 12
const ROW_GAP_AFTER_X := 9.0
const ROW_BUTTON := 67.0          # = 行高（AspectRatioContainer stretch_mode=1）
const ROW_RANK := Vector2(65, 0)   # 墨迹 23×36 ⇒ 盒子 65×(行高，由 svg 留白折算)
const ROW_SOLDIER := 66.0        # svg 留白折算：墨迹 ≈ 34 px + 负 margin ⇒ 两兵墨迹 67 px
const ROW_FIVE_SPACES := 105.0    # 行里那个 `text = "     "` 的 Label（字号 48）
const ROW_GAP_BEFORE_MINUS := 19.0
const ROW_GAP_MINUS_ARROW := 0.0
const ROW_TAIL := 6.0            # 行尾留白（真帧 `→` 右沿 939、行右沿 950）

func _player_row(index: int, p: Dictionary) -> HBoxContainer:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION)
	row.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	row.add_child(GwUI.spacer(Vector2(ROW_LEAD, 0)))

	# 踢出（`RemovePlayerButton`：text = 4 个空格 + `x.svg` + `expand_icon`）
	var remove := _row_button("RemovePlayerButton", "x", "踢出这名玩家")
	remove.pressed.connect(_on_remove_player.bind(index))
	row.add_child(remove)

	row.add_child(GwUI.spacer(Vector2(ROW_GAP_AFTER_X, 0)))

	# 段位图标：原版是 `ranks/rank_N.svg`（玩家）或 `hats/no_hat.svg`（Bot）。
	# 真帧这一格画的是 `ranks/rank_0`（23×36 的黄色徽记）⇒ 默认 rank = 0。
	# ⚠️ 段位系统没有实现（玩家数据里没有 rank 字段）⇒ 一律 rank_0；**推断**：
	# 原版对局设置里这一格就是"当前段位"，新账号是 0。
	var rank := TextureRect.new()
	rank.expand_mode = TextureRect.EXPAND_IGNORE_SIZE   # 图标是 512² 导入产物，不忽略会撑爆最小尺寸
	rank.name = "RankIcon"
	# 盒子尺寸是**按 svg 的透明留白反算**的：`ranks/rank_0.svg` 的画布留白很大，
	# 图标墨迹只占盒子宽的 39%、高的 53%（实测：盒子 23×36 时墨迹只有 9×19）。
	# 真帧那一格墨迹是 23×36 ⇒ 盒子 = (23/0.39, 36/0.53) ≈ (59, 68)，取 (59, 行高)。
	rank.custom_minimum_size = ROW_RANK
	rank.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# 真帧这一格墨迹正好 23×36 ⇒ 图标**铺满**这个盒子（KEEP_ASPECT 会按 svg 自身比例缩成 9 px 宽）
	rank.stretch_mode = TextureRect.STRETCH_SCALE
	var rank_name := "no_hat" if bool(p.get("is_bot", false)) else "rank_%d" % int(p.get("rank", 0))
	rank.texture = GwIcons.get_icon(rank_name)
	rank.tooltip_text = "段位图标（%s；原版 ranks/rank_N.svg，本局一律 rank_0）" % rank_name
	row.add_child(rank)

	# 名字（`GameSetupPlayerEntry.gd` = 字号 宽/40 = 48，标签居中）
	var name_m := GwUI.margin("MarginContainer", 0)
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var name_label := GwUI.label(String(p.name), 0, HORIZONTAL_ALIGNMENT_CENTER)
	name_label.clip_text = true
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_label.name = "PlayerNameLabel"
	name_m.add_child(name_label)
	# 原版 `team1_entry.tscn` / `team2_entry.tscn` 里，**包 `PlayerNameLabel` 的那个**
	# `MarginContainer` 挂的就是它（观测：`[node name="MarginContainer" parent="."]` +
	# `script = ExtResource("3_4xh4q")` = `res://GameSetupPlayerEntry.gd`；它是根
	# `HBoxContainer` 的**第 5 个**孩子）。这个脚本只干一件事：给子节点写
	# `font_size = 视口宽/40` —— 所以节点形状对上就能直接换。
	GwViews.attach(name_m, "GameSetupPlayerEntry.gd")
	row.add_child(name_m)

	# 加士兵（`AddSoldierButton`）
	var add := _row_button("AddSoldierButton", "plus",
		"该玩家加一个士兵（最多 %d）" % GwGameState.MAX_SOLDIERS)
	add.pressed.connect(_on_add_soldier.bind(index))
	row.add_child(add)

	# 行里那个 `text = "     "` 的 Label（5 个空格，字号由父主题给 48 ⇒ 105 px）
	row.add_child(GwUI.spacer(Vector2(ROW_FIVE_SPACES, 0)))

	# 士兵图标（`SoldierIconContainer`）：每名士兵 34 px，**只画真的有士兵的那几格**。
	# 原版是 5 个 `MarginContainer`（`SoldierIconMarginContainer.gd` 给它写
	# `margin_left/right = -屏宽/100 = -19.2`），没兵的那几格画的是 `empty_soldier.svg`
	# （真帧上看不见）且**不占宽**（真帧：2 个兵 = 67 px，兵后到 − 键只有 15 px）。
	var slots := GwUI.hbox("SoldierIconContainer", 0, row)
	slots.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	slots.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var soldiers := int(p.get("soldiers", 0))
	for s in GwGameState.MAX_SOLDIERS:
		if s >= soldiers:
			break
		# 槽 = `MarginContainer` + **原版** `SoldierIconMarginContainer.gd`：它给左右各写
		# `margin = -屏宽/100 = -19.2` ⇒ 相邻两个士兵**叠起来**（真帧：两颗 34 px 的图标
		# 只占 67 px，中间没有缝）。盒子 64×64：`skins/*.svg` 的画布留白同样很大，
		# 墨迹只占 53% ⇒ 64×0.53 ≈ 34 px = 真帧那颗士兵图标的宽度。
		var slot := GwUI.margin("MarginContainer", 0, slots)
		slot.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
		var icon := TextureRect.new()
		icon.custom_minimum_size = Vector2(ROW_SOLDIER, ROW_SOLDIER)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.name = "SkinIcon"
		icon.stretch_mode = TextureRect.STRETCH_SCALE
		icon.texture = GwIcons.get_icon("skin_%d" % int(p.skin)
			if int(p.skin) >= 0 else "bot_%d" % int(p.bot_tier))
		slot.add_child(icon)
		# 原版这一层是 `SoldierIconMarginContainer.gd`（`recovered-src/` 里有，正文一行）：
		#   margin_left/right = -屏宽/100 = **-19.2**
		# 它给相邻两个士兵各让出 19 px ⇒ 图标互相叠起来。我们的
		# `client/src/views/` 只有 27 份原版脚本、里面**没有**这一份（不能自己加第 28 份），
		# 所以这里把那一行**照抄**进来（同样是 宽/100 的负 margin，随视口走）。
		var neg := -int(get_viewport_rect().size.x / 100.0)
		slot.add_theme_constant_override("margin_left", neg)
		slot.add_theme_constant_override("margin_right", neg)

	row.add_child(GwUI.spacer(Vector2(ROW_GAP_BEFORE_MINUS, 0)))

	# 减士兵（`RemoveSoldierButton`）
	var sub := _row_button("RemoveSoldierButton", "minus", "减一个士兵")
	sub.pressed.connect(_on_remove_soldier.bind(index))
	row.add_child(sub)

	row.add_child(GwUI.spacer(Vector2(ROW_GAP_MINUS_ARROW, 0)))

	# 换队（`ChangeTeamButton`）
	var swap := _row_button("ChangeTeamButton",
		"arrow_right" if int(p.team) == 0 else "arrow_left", "换到对面队伍")
	swap.pressed.connect(_on_change_team.bind(index))
	row.add_child(swap)

	# 行尾留白（`team*_entry.tscn` 的 `MarginContainer5`，ratio 0.05）：
	# 真帧行尾到 `→` 键右沿还有 **11 px**（`→` 12..79 之类都靠它定住）。
	row.add_child(GwUI.spacer(Vector2(ROW_TAIL, 0)))

	return row

## 玩家行里那几个方键：`team*_entry.tscn` 里它们是
## `AspectRatioContainer(stretch_mode=1) → GraphwarButton(text = "    ", icon, expand_icon)`
## ⇒ 真帧上就是 **67×67 的深绿底 + 3 px 黑边 + 居中图标**（行高 67 = 名字 Label 的最小高度）。
func _row_button(node_name: String, icon_name: String, tip: String) -> GwButton:
	var b := GwButton.make("")
	b.name = node_name
	b.icon = GwIcons.get_icon(icon_name)
	b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	b.expand_icon = true
	b.custom_minimum_size = Vector2(ROW_BUTTON, ROW_BUTTON)
	b.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	b.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	b.tooltip_text = tip
	return b

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
