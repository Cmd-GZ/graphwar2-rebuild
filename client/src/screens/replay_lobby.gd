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
## 本项目：扫 `user://replays/`（`GwReplayFile.list_dir`），不读原版存档目录。
## **文件名就是 unix 时间戳、没有扩展名**（`docs/78` §1 观测），所以这里**不按扩展名过滤**
## —— `replay_index.gd` 的 `EXTENSIONS` 白名单会把**所有真实回放**都筛掉，这正是
## 之前"回放列表永远空着"的原因（`docs/133` §4.4）。
##
## 房间名/四个模式**不在列表里解析**：那要把每份 gzip 整个解压一遍（400 份 ≈ 64 MB），
## 打开回放时才有那个信息（`play_screen.gd::_replay_status_text()` 会显示）。
## 列表里只给文件名 + 字节数 + 录制时间，`--` 表示"这一项我们没有的字段"。

var _list: VBoxContainer
var _selected := ""
## 扫哪个目录。默认 `user://replays`；自检可以指向真实存档目录做**只读**列举。
var _scan_dir := "user://replays"

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
	_ensure_dir()
	var entries := GwReplayFile.list_dir(_scan_dir)
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
	# 房间名要解压整份回放才知道（列表里不解，见文件头）；录制时间倒是有现成的。
	var game_label := GwUI.label(_entry_subtitle(entry))
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

## 副标题：录制时间（本地时区）+ 字节数。没有房间名时**不写 `--`**，写"没有的东西"更糟。
func _entry_subtitle(entry: Dictionary) -> String:
	var t := int(entry.get("mtime", 0))
	var when := "—"
	if t > 0:
		var d := Time.get_datetime_dict_from_unix_time(t)
		when = "%04d-%02d-%02d %02d:%02d" % [int(d.year), int(d.month), int(d.day),
			int(d.hour), int(d.minute)]
	return "%s ／ %d B" % [when, int(entry.get("size", 0))]

func _ensure_dir() -> void:
	if not DirAccess.dir_exists_absolute(_scan_dir):
		DirAccess.make_dir_recursive_absolute(_scan_dir)

## 自检/诊断用：换一个目录列举（**只读**，不改任何文件）。
## 传空串恢复默认的 `user://replays`。
func set_scan_dir(dir: String) -> void:
	_scan_dir = "user://replays" if dir == "" else dir
	_rebuild()

func scan_dir() -> String:
	return _scan_dir

## 条目数（自检用）。
func entry_count() -> int:
	return _list.get_child_count() if _list != null else -1

func _on_play(path: String) -> void:
	_selected = path
	var app_node := app()
	if app_node != null:
		app_node.command(GwCommands.PLAY_REPLAY)

func selected_replay() -> String:
	return _selected
