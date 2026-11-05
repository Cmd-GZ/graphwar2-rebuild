extends GwScreen
## LanLobby —— 联机大厅（Local / Global 两个房间列表）。
##
## 结构（`docs/92` §4.4，恢复出的 lan_lobby.tscn 32 节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer
##     │    ├ ExitButtonContainer/…            (input_text="exit_lan_lobby")
##     │    ├ ShowLocalContainer/ShowLocalButton("Local",  input_text="show_local", toggle+checked)
##     │    └ ShowGlobalContainer/ShowGlobalButton("Global", input_text="show_global", toggle)
##     ├ MarginContainer2/VBoxContainer
##     │    ├ HBoxContainer（表头：Room Name / Players / Game Mode / Join）
##     │    └ ScrollContainer/GraphwarLanVBox/<房间条目>   ← game_room_entry.tscn 实例
##     ├ MarginContainer/HBoxContainer
##     │    ├ CreateButton(" Create Game ", input_text="create_game")
##     │    └ JoinButton(" Join IP ", input_text="join_ip")
##     └ AdContainer(0,60)
##
## 观测要点：
##   * Local 默认选中；Global 列表里是 5 条公网房间（US Room 1/3/4、EU Room 1/2，**没有 US Room 2**）。
##     这些房间从哪来**未知**（`docs/92` §8："无网络，未抓包验证"），所以本项目只做静态占位条目。
##   * 本地房间列表：本项目没有网络层，本地"房间"就是当前进程里的那个房间（有的话）。
##   * Join 按钮在本项目里也是"未实现网络"的明写占位。

## 观测到的公网房间条目（docs/92 §4.4）。名字照抄，人数/模式未知 → 显示 `--`。
const GLOBAL_ROOMS := [
	{"name": "US Room 1", "players": "--"},
	{"name": "US Room 3", "players": "--"},
	{"name": "US Room 4", "players": "--"},
	{"name": "EU Room 1", "players": "--"},
	{"name": "EU Room 2", "players": "--"},
]

var _local_button: GwButton
var _global_button: GwButton
var _list: VBoxContainer

func _ready() -> void:
	font_divisor = 40        # LanLobbyFontResize.gd: 宽/40（标题 LanLobbyTitle 是 /30）
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox

	# --- 顶部：返回 + Local/Global 切换 ---
	var top := GwUI.hbox("HBoxContainer", 8, vbox)
	top.add_child(GwUI.exit_pair(GwCommands.EXIT_LAN_LOBBY))
	# 原版截图（1920×1160 真机/真二进制）：[返回][Local][Global] 三者横向铺满整行，
	# Local 与 Global 各占剩余宽度的一半（实测 Local 146..1030、Global 1047..1913）。
	# 我们以前给它们自然宽度、末尾再塞一个弹簧，于是两个按钮挤在左边（docs/124 §4）。
	var local_holder := GwUI.margin("ShowLocalContainer", 4, top)
	local_holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_local_button = GwButton.make("Local", GwCommands.SHOW_LOCAL)
	_local_button.name = "ShowLocalButton"
	_local_button.toggle_mode = true
	_local_button.button_pressed = true
	local_holder.add_child(_local_button)
	var global_holder := GwUI.margin("ShowGlobalContainer", 4, top)
	global_holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_global_button = GwButton.make("Global", GwCommands.SHOW_GLOBAL)
	_global_button.name = "ShowGlobalButton"
	_global_button.toggle_mode = true
	global_holder.add_child(_global_button)

	# --- 表头（原版这里三个 Label 有两个都叫 RoomName，照抄名字会撞，所以保留命名后缀）---
	var header := GwUI.hbox("HBoxContainer", 8, vbox)
	header.add_child(_header_cell("NameContainer", "RoomName", "Room Name", 3.0))
	header.add_child(_header_cell("PlayersContainer", "RoomName", "Players", 1.0))
	header.add_child(_header_cell("ModeContainer", "GameMode", "Game Mode", 2.0))
	header.add_child(_header_cell("JoinButtonContainer", "", " ", 1.0))

	# --- 房间列表 ---
	var scroll := GwUI.scroll("ScrollContainer")
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list = GwUI.vbox("GraphwarLanVBox", 6)
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(_list)
	vbox.add_child(scroll)

	# --- 底部：建房 / 加 IP ---
	var bottom := GwUI.margin("MarginContainer", 4, vbox)
	var bottom_row := GwUI.hbox("HBoxContainer", 10, bottom)
	# `LobbyButtonContainer.gd`: Create/Join 用的是 **宽/30**（与列表头的 /40 不同）。
	bottom_row.set_meta("font_divisor", 30)
	var pad := GwUI.margin("MarginContainer", 4, bottom_row)
	pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var create := GwButton.make(" Create Game ", GwCommands.CREATE_GAME)
	create.name = "CreateButton"
	bottom_row.add_child(create)
	var join := GwButton.make(" Join IP ", GwCommands.JOIN_IP)
	join.name = "JoinButton"
	bottom_row.add_child(join)
	var pad2 := GwUI.margin("MarginContainer2", 4, bottom_row)
	pad2.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	vbox.add_child(GwUI.ad_bar())

func _header_cell(node_name: String, label_name: String, text_: String, ratio: float) -> MarginContainer:
	var m := GwUI.margin(node_name, 4)
	m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	m.size_flags_stretch_ratio = ratio
	var l := GwUI.label(text_)
	if label_name != "":
		l.name = label_name
	l.add_theme_color_override("font_color", GwThemeFactory.COL_TEXT_DIM)
	m.add_child(l)
	return m

func on_enter() -> void:
	var app_node := app()
	var mode := "local"
	if app_node != null:
		mode = String(app_node.room_list_mode)
	_local_button.button_pressed = mode == "local"
	_global_button.button_pressed = mode == "global"
	_rebuild_list(mode)

func _rebuild_list(mode: String) -> void:
	for c in _list.get_children():
		c.queue_free()
	if mode == "global":
		# 原版这里只有房间条目，**没有任何说明文字**（原版截图逐像素对照，docs/124 §4）。
		for room in GLOBAL_ROOMS:
			_list.add_child(_room_entry(String(room.name), String(room.players), true))
	else:
		var app_node := app()
		var has_room: bool = app_node != null and app_node.state.players.size() > 0
		if has_room:
			_list.add_child(_room_entry(
				String(app_node.state.room_name),
				"%d" % app_node.state.players.size(),
				false))
		# 空列表就**什么都不画**（原版 Local 页在 1920×1160 下就是一片空背景）。

## 一条房间条目（原版是 `game_room_entry.tscn` 实例，20 节点）：
## RoomName / PlayerCount / 四个模式图标 / Join 按钮。
func _room_entry(room_name: String, players: String, remote: bool) -> HBoxContainer:
	var row := GwUI.hbox("Room", 8)
	row.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	var name_m := GwUI.margin("NameContainer", 4, row)
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_m.size_flags_stretch_ratio = 3.0
	var name_label := GwUI.label(room_name)
	name_label.name = "RoomName"
	name_m.add_child(name_label)

	var players_m := GwUI.margin("PlayersContainer", 4, row)
	players_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var count := GwUI.label(players, 0, HORIZONTAL_ALIGNMENT_CENTER)
	count.name = "PlayerCount"
	players_m.add_child(count)

	var mode_m := GwUI.margin("ModeContainer", 4, row)
	mode_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	mode_m.size_flags_stretch_ratio = 2.0
	var options := GwUI.hbox("OptionsContainer", 4, mode_m)
	for icon_name in ["axis_option_3", "fx_option", "turns_sequential", "turn_1m"]:
		var b := GwUI.mode_button(icon_name)
		b.custom_minimum_size = Vector2(34, 34)
		b.disabled = true
		options.add_child(b)

	var join_m := GwUI.margin("JoinButtonContainer", 4, row)
	var join := GwButton.make(" Join ")
	join.name = "JoinButton"
	join.custom_minimum_size = Vector2(96, 40)
	join.input_text = ""     # 点 Join 之后的行为**未观测**（docs/92 §4.4 推断：离线时静默失败）
	join.pressed.connect(_on_join_pressed)
	join_m.add_child(join)
	return row

func _on_join_pressed() -> void:
	notify("联机未实现：本项目没有网络层（协议复原在 Rust 侧 docs/24）")
