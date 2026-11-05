extends GwScreen
## ReplayLobby —— 回放列表。
##
## 结构（`docs/92` §4.15，恢复出的 replay_lobby.tscn 只有 13 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer(exit_replay_lobby), MarginContainer/Label("REPLAYS")}
##     ├ ScrollContainer/ReplaysContainer/<条目>   ← replay_entry.tscn 实例（21 节点/条）
##     └ AdContainer(0,60)
##
## 运行时观测：本机回放目录里 59 个文件 ⇒ 渲染 59 条、2118 个节点。
## 条目结构（`replay_entry.tscn`）：`ReplayName`（= 录制时间戳）、`GameName`（房间名）、
## 四个模式图标、`PlayReplayButton`（input_text="play_replay"）。
##
## 本项目：扫 `user://replays/`（`replay_index.gd`），不读原版存档目录。
## 回放头解析未做 ⇒ 房间名/模式都是 `--`（不假装知道）。

var _list: VBoxContainer
var _selected := ""

func _ready() -> void:
	font_divisor = 40        # GameNameContainer.gd: 宽/40
	super()

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	build_header(vbox, "REPLAYS", GwCommands.EXIT_REPLAY_LOBBY, true)

	var scroll := GwUI.scroll("ScrollContainer")
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list = GwUI.vbox("ReplaysContainer", 4)
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(_list)
	vbox.add_child(scroll)

	vbox.add_child(GwUI.ad_bar())

func on_enter() -> void:
	_rebuild()

func _rebuild() -> void:
	for c in _list.get_children():
		c.queue_free()
	GwReplayIndex.ensure_dir()
	var entries := GwReplayIndex.scan()
	_selected = ""
	# 空列表就**什么都不画**：原版 1920×1160 真帧里，没有回放时这一屏只有返回键和
	# "REPLAYS" 标题，其余是纯底色（逐像素确认）。我们以前在这里加了两行自造提示，
	# 其中一行还写着内部文档编号（docs/129 §5）。
	for e in entries:
		_list.add_child(_entry(e))

## 一条回放（原版 `replay_entry.tscn`，21 节点）。
func _entry(entry: Dictionary) -> HBoxContainer:
	var row := GwUI.hbox("ReplayEntry", 8)
	row.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	var name_m := GwUI.margin("NameContainer", 2, row)
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_m.size_flags_stretch_ratio = 2.0
	var name_label := GwUI.label(String(entry.title))
	name_label.name = "ReplayName"
	name_m.add_child(name_label)

	var game_m := GwUI.margin("GameNameContainer", 2, row)
	game_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	game_m.size_flags_stretch_ratio = 2.0
	var game_label := GwUI.label(String(entry.game_name))
	game_label.name = "GameName"
	game_m.add_child(game_label)

	var mode_m := GwUI.margin("ModeContainer", 2, row)
	mode_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	mode_m.size_flags_stretch_ratio = 2.0
	var options := GwUI.hbox("OptionsContainer", 4, mode_m)
	for icon_name in ["axis_option_3", "fx_option", "turns_sequential", "turn_1m"]:
		var b := GwUI.mode_button(icon_name)
		b.custom_minimum_size = Vector2(34, 34)
		b.disabled = true
		b.modulate = Color(1, 1, 1, 0.5)
		options.add_child(b)

	var play_holder := AspectRatioContainer.new()
	play_holder.name = "AspectRatioContainer"
	play_holder.ratio = 1.0
	play_holder.custom_minimum_size = Vector2(52, 52)
	var play := GwButton.make("")
	play.name = "PlayReplayButton"
	play.icon = GwIcons.get_icon("play_option")
	play.pressed.connect(_on_play.bind(String(entry.path)))
	play_holder.add_child(play)
	row.add_child(play_holder)

	var info_m := GwUI.margin("MarginContainer", 2, row)
	var info := GwUI.label("%d B" % int(entry.size), 0, HORIZONTAL_ALIGNMENT_RIGHT)
	info.add_theme_color_override("font_color", GwThemeFactory.COL_TEXT_DIM)
	info_m.add_child(info)

	return row

func _on_play(path: String) -> void:
	_selected = path
	var app_node := app()
	if app_node != null:
		app_node.command(GwCommands.PLAY_REPLAY)

func selected_replay() -> String:
	return _selected
