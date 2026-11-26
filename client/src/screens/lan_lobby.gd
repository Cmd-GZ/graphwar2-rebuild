extends GwScreen
## LanLobby —— 联机大厅（Local / Global 两个房间列表）。
##
## 结构（`docs/92` §4.4，恢复出的 `lan_lobby.tscn` 32 节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer
##     │    ├ ExitButtonContainer/…            (input_text="exit_lan_lobby")  字号 宽/30
##     │    ├ ShowLocalContainer/ShowLocalButton("Local",  input_text="show_local", toggle+checked)
##     │    │        └ 挂 `LanLobbyTitle.gd` ⇒ **字号 = 宽/30**
##     │    └ ShowGlobalContainer/ShowGlobalButton("Global", input_text="show_global", toggle)
##     ├ MarginContainer2/VBoxContainer
##     │    ├ HBoxContainer（表头，`LanLobbyFontResize.gd` 的四个 MarginContainer ⇒ 字号 宽/40）
##     │    │    ├ NameContainer    size_flags_horizontal=3, ratio 1
##     │    │    ├ PlayersContainer size_flags_horizontal=3, ratio **0.1**
##     │    │    ├ ModeContainer    size_flags_horizontal=3, ratio 1
##     │    │    └ JoinButtonContainer（**不** EXPAND）里放一个全透明主题的 " Join "
##     │    └ ScrollContainer/GraphwarLanVBox/<房间条目>   ← game_room_entry.tscn 实例
##     ├ MarginContainer/HBoxContainer（`LobbyButtonContainer.gd`）
##     │    ├ MarginContainer  ┐
##     │    ├ CreateButton     │ 四个子节点的 custom_minimum_size.x 全被设成 **屏宽×0.2**，
##     │    ├ JoinButton       │ 字号 = 宽/30 ⇒ " Join IP " 的宽度由 0.2×1920=384 兜底
##     │    └ MarginContainer2 ┘
##     └ AdContainer（运行 0 高、无文字）
##
## 本轮按真帧纠正的三件事（docs/129 §4）：
##   1. 表头：三列都 `size_flags_horizontal=3`、标签**居中**、字号 宽/40、颜色用默认白。
##      我们以前是 3:1:2 的比例 + 左对齐 + 自选灰 ⇒ 整排左移约 260 px。
##   2. 底部按钮：`LobbyButtonContainer.gd` 给**每个**子节点设 `宽度 = 屏宽×0.2 = 384`，
##      所以 " Join IP " 是 384 而不是文本宽度 286（真帧 1000..1383 = 384，逐像素对得上）。
##   3. 公网列表的 `Players` 是 **0**（真帧四行全是 0），不是 `--`；四个模式图标照
##      `game_room_entry.tscn` 的结构做（AspectRatioContainer 里的 GraphwarButton + expand_icon）。

## 真帧观测（1920×1160、离线、Global 页）：四行，`Players` 全是 0；模式四块分别是
## 网格 / `f(x)=` / 回合箭头 / `01:00` —— 也就是 `axis_option_3` / `fx_option` /
## `turns_simultaneous` / `turn_1m` 四张图（`game_room_entry.tscn` 的静态值）。
## 房间名与在线人数是**服务端数据**，离线拿不到：名字沿用原版真帧里出现过的四个，
## 人数用真帧里读到的 0。
##
## ## 列表的真实来源（docs/134）
##
## 两页的数据现在都来自 `App.net`，占位行只在**没有数据**时兜底：
##
## | 页 | 数据来源 | 依据 |
## |---|---|---|
## | Local | `GwLanDiscovery`：UDP 42968 收到的裸 9 字段 JSON 行 | **观测**（`docs/24` §1.1、`docs/115` §5.3） |
## | Global | `GwBrokerClient`：broker 推的 `GameInfo` 行 | **逐位**（`fixtures/protocol_match.jsonl:83–86`） |
##
## * broker 地址**默认空**（原版默认从 graphwar.com 取；本项目不发外网请求），
##   所以 Global 页在默认配置下仍然是那 4 条占位 —— 与真帧一致；
## * 点某一行右侧的 ` Join ` 会真的去连该行的 `address`（`App.connect_to`），
##   连不上就走 `DisconnectedPopUp`；
## * 占位行**没有地址**，所以它们的 ` Join ` 不接受点击（**原版里没有这种行**：
##   房间表的键就是 `SocketAddr`，见 docs/144 §2）。以前这里会弹一句
##   "这一行没有房间地址"，那是我们自造的、用户实测撞到的提示 —— 已删除。
##
## ## 列表是**每帧重投影**的，不是缓存（docs/144 §1/§3）
##
## 原版 `GraphwarLanVBox::process` 每帧重新取一遍房间表：每一行的名字/人数/mode 图标
## 都从活的数据源现取，取不到就把这一行删掉。所以"人数变了"和"房间没了"都在下一帧生效。
## 本类照做（`_process` → `_sync_list`）：键序列没变就只改文本，变了才重建行。
##
## 自己开的那间房由 `App.local_room` 代表，生命周期 = 原版
## `start_lan_broadcaster()` / `stop_lan_broadcaster()`：`create_game` 时开，
## 离开大厅/对局时收。以前它直接读 `state.players`（对局状态），退出对局不清 ⇒
## 上一把的房间一直挂着。
const GLOBAL_ROOMS := [
	{"name": "US Room 3", "players": "0"},
	{"name": "EU Room 1", "players": "0"},
	{"name": "EU Room 2", "players": "0"},
	{"name": "US Room 2", "players": "0"},
]

## `game_room_entry.tscn` 的四个模式图标（顺序就是节点顺序）。
const MODE_ICONS := ["axis_option_3", "fx_option", "turns_sequential", "turn_1m"]

## Local / Global 两个切换键：**原生 `GraphwarButton`**（原版 `.tscn` 里的节点）。
var _local_button: Button
var _global_button: Button
var _list: VBoxContainer
var _bottom_row: HBoxContainer
## 当前画出来的行：`key` 的序列 + 每行的两个文本标签。
##
## 有了它才能"只改数字不重建行"：原版 `GraphwarLanVBox::process` 每帧重取房间表，
## 人数变了就改那一格的文本，**不重画整行**（docs/144 §3）。行还在、只有内容变的情况下
## 我们照做；键序列变了才重建（增/删房间）。
var _row_keys: Array[String] = []
var _row_labels := {}     # key -> {"name": Label, "players": Label}
## 当前在画哪一页（"local" / "global"）。`_process` 的重投影要用它。
var _mode := "local"

## 字号全部由原版脚本自己算（`client/src/views/`）：表头/房间行 `LanLobbyFontResize.gd`（宽/40）、
## Local/Global `LanLobbyTitle.gd`（宽/30）、底栏 `LobbyButtonContainer.gd`（宽×0.2 + 宽/30）、
## 返回键 `ExitButtonContainer.gd`（宽/30）。所以本屏不再设 `font_divisor`。
func _ready() -> void:
	super()
	set_process(true)

## 节点树来自原版 `.tscn`（`client/scenes/screens/lan_lobby.tscn` 与
## `recovered-src/lan_lobby.tscn` **逐字节相同**，213 行 / 32 节点）。这里只取句柄：
## 表头四格、Local/Global、返回键、Create/Join 的布局与字号**全部由原版脚本在 `_ready` 里算**
## （`LanLobbyFontResize.gd` 宽/40、`LanLobbyTitle.gd` 宽/30、`LobbyButtonContainer.gd` 宽×0.2），
## 我方不再手抄任何一行（docs/06 §92）。
const PATH_LIST := "MarginContainer/VBoxContainer/MarginContainer2/VBoxContainer/ScrollContainer/GraphwarLanVBox"
const PATH_LOCAL := "MarginContainer/VBoxContainer/HBoxContainer/ShowLocalContainer/ShowLocalButton"
const PATH_GLOBAL := "MarginContainer/VBoxContainer/HBoxContainer/ShowGlobalContainer/ShowGlobalButton"
const PATH_BOTTOM := "MarginContainer/VBoxContainer/MarginContainer/HBoxContainer"

func build() -> void:
	_list = get_node_or_null(PATH_LIST) as VBoxContainer
	if _list == null:
		push_error("[gw2] lan_lobby.tscn 里找不到 %s" % PATH_LIST)
	_local_button = get_node_or_null(PATH_LOCAL) as Button
	_global_button = get_node_or_null(PATH_GLOBAL) as Button
	_bottom_row = get_node_or_null(PATH_BOTTOM) as HBoxContainer

func on_enter() -> void:
	var app_node := app()
	var mode := "local"
	if app_node != null:
		mode = String(app_node.room_list_mode)
		if mode == "global" and app_node.net != null:
			# 进 Global 页才开始连 broker（地址为空时它什么都不做）。
			app_node.net.browse_global()
	_local_button.button_pressed = mode == "local"
	_global_button.button_pressed = mode == "global"
	_rebuild_list(mode)

## 每帧重投影（原版 `GraphwarLanVBox::process` 就是这么干的）。
##
## 为什么必须每帧：房间表是**活数据**（人数会变、房间会消失），而"我的房间"的人数
## 更是随时在变（别人加入 = `NewConnection`/`PlayerInfo`）。只在收到
## `rooms_changed` 时刷新的写法，遇到"没人发信号但数字变了"就永远不更新
## （用户实测：别人进了房间，Players 列不动）。
func _process(_delta: float) -> void:
	if not _built or not visible:
		return
	_sync_list(_mode)

## 只改文本、不重建行。键序列变了才回退到 `_rebuild_list()`。
func _sync_list(mode: String) -> void:
	var wanted := _wanted_rows(mode)
	if not _same_keys(wanted):
		_rebuild_list(mode)
		return
	for w in wanted:
		var labels: Dictionary = _row_labels.get(String(w["key"]), {})
		if labels.is_empty():
			continue
		var name_label := labels.get("name") as Label
		var players_label := labels.get("players") as Label
		if name_label != null:
			name_label.text = String(w["name"])
		if players_label != null:
			players_label.text = String(w["players"])

func _same_keys(wanted: Array) -> bool:
	if wanted.size() != _row_keys.size():
		return false
	for i in wanted.size():
		if String((wanted[i] as Dictionary)["key"]) != _row_keys[i]:
			return false
	return true

## 当前页面**应该**画出来的行。
##
## `key` 是这一行的身份：真实房间用它的 `address`（原版的表就是
## `HashMap<SocketAddr, GameInfo>`，地址是键 —— docs/144 §2），占位行用房间名。
## 占位行的 `address` 是空串 ⇒ 这一行没有地址 ⇒ Join 不可点（见 `_room_entry`）。
func _wanted_rows(mode: String) -> Array:
	var app_node := app()
	var net: GwNet = app_node.net if app_node != null else null
	var out: Array = []
	if mode == "global":
		# 原版这一页只有房间条目，**没有任何说明文字**（真帧逐像素对照，docs/129 §4.4）。
		var rooms: Array = net.global_rooms() if net != null else []
		if rooms.is_empty():
			# 没有 broker 数据 ⇒ 沿用占位行（默认配置就是这样：broker 地址为空）。
			# 这四行是**本项目自定**的（离线真帧里那几行来自真实 broker，有地址），
			# 所以它们没有地址、Join 不可点 —— 见 docs/144 §2。
			for room in GLOBAL_ROOMS:
				out.append({
					"key": "placeholder:" + String(room.name),
					"name": String(room.name),
					"players": String(room.players),
					"address": "",
				})
			return out
		for room in rooms:
			var addr := String(room.get("address", ""))
			out.append({
				"key": addr,
				"name": String(room.get("room_name", "")),
				"players": "%d" % int(room.get("current_player_count", 0)),
				"address": addr,
			})
		return out
	# Local 页 = LAN 广播收到的房间 + 我们自己那间（`App.local_room`）。
	# 空列表就**什么都不画**（原版 Local 页在 1920×1160 下就是一片空背景）。
	for room in (net.local_rooms() if net != null else []):
		var addr := String(room.get("address", ""))
		out.append({
			"key": addr,
			"name": String(room.get("room_name", "")),
			"players": "%d" % int(room.get("current_player_count", 0)),
			"address": addr,
		})
	# 自己那间：`App.local_room_row()` 为空 ⇒ 房间已经收了 ⇒ 这一行**不画**。
	# 这就是"上一把的房间不会留在大厅里"的机制（docs/144 §1）。
	var self_row: Dictionary = app_node.local_room_row() if app_node != null else {}
	if not self_row.is_empty() and not (app_node != null and app_node.online):
		out.append({
			"key": "@self",
			"name": String(self_row.get("room_name", "")),
			"players": "%d" % int(self_row.get("players", 0)),
			"address": String(self_row.get("address", "")),
		})
	return out

func _rebuild_list(mode: String) -> void:
	_mode = mode
	for c in _list.get_children():
		_list.remove_child(c)
		c.queue_free()
	_row_keys.clear()
	_row_labels.clear()
	for w in _wanted_rows(mode):
		var row := _room_entry(String(w["name"]), String(w["players"]), String(w["address"]))
		# 行里的四个容器各挂一个原版脚本；它们是在**进树**时由各自的 `_ready` 算字号的，
		# 所以必须在 `add_child()` 之前挂好 —— 这也正是原版 `.tscn` 的时序。
		_list.add_child(row)
		_row_keys.append(String(w["key"]))
		_row_labels[String(w["key"])] = {
			"name": row.find_child("RoomName", true, false),
			"players": row.find_child("PlayerCount", true, false),
		}

## 一条房间条目（`game_room_entry.tscn`，20 节点）：
##   HBoxContainer
##     ├ ScrollContainer/NameContainer/RoomName(居中)
##     ├ PlayersContainer/PlayerCount(居中)          ← ratio 0.1
##     ├ ModeContainer/OptionsContainer/[Margin, 4×AspectRatio(GraphwarButton+icon), Margin]
##     └ JoinButtonContainer/JoinButton(" Join ")
## `address` 为空 = 占位行（没有真实地址，点 Join 只提示）。
func _room_entry(room_name: String, players: String, address: String = "") -> HBoxContainer:
	var row := GwUI.hbox("HBoxContainer", GwUI.SEPARATION)
	row.size_flags_vertical = Control.SIZE_SHRINK_BEGIN

	# 原版把 NameContainer **套在一个 ScrollContainer 里**（`vertical_scroll_mode = 0`）。
	# 这层壳让名字列的最小宽度不随文本增长（ScrollContainer 的最小尺寸很小），照抄。
	var name_scroll := ScrollContainer.new()
	name_scroll.name = "ScrollContainer"
	name_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	name_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	row.add_child(name_scroll)
	var name_m := GwUI.margin("NameContainer", 0, name_scroll)
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	GwViews.attach(name_m, "LanLobbyFontResize.gd")
	var name_label := GwUI.label(room_name, 0, HORIZONTAL_ALIGNMENT_CENTER)
	name_label.name = "RoomName"
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_m.add_child(name_label)

	var players_m := GwUI.margin("PlayersContainer", 0, row)
	players_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	players_m.size_flags_stretch_ratio = 0.1
	# **行内 Players 列有一个结构性最小宽度 ≈111 px**（真帧反解，**来源未知**）。
	#
	# 量到的三件事（1920×1160 真帧，两侧同一次采集；数字见 `docs/06` §75）：
	#   * 行内 `0` / `1` 的墨迹两页都一样（x 876..893）⇒ 本列位置两侧相同；
	#   * 行内 ` Join ` 单元格两侧都从 x=1776 起（144 px 宽）；
	#   * 但四个模式图标的中心，原版是 1151 / 1289 / 1427 / 1566，我们是 1140 / 1281 / 1422 / 1562
	#     ⇒ 原版的 `ModeContainer` 比我们窄、起点比我们靠右 13.5 px。
	# 本列只放一个数字（文本最小宽度 29 px），可原版这一列占 **111 px** 且 `0`/`1` 两种文本
	# **一样宽** ⇒ 它不是文本撑出来的。按 Godot 的分配规则反解：
	#   `(1920 − 3 个 4 px 间隔 − 144) = 1764` 按 1 : 0.1 : 1 分 = 840 / 84 / 840；`84 < 111`
	#   ⇒ 这一列被最小宽度顶到 111，少掉的 27 由另两列各让 13.5 ⇒ **826.5 / 111 / 826.5**，
	#   此时算出的四个图标中心 = 1151.25 / 1289.6 / 1428 / 1566.4 ⇒ 与真帧逐个数吻合。
	# 取整实测：`111` 时四个图标各差 1 px；**`112` 时四个图标逐个精确**（本轮实测），故用 112。
	# ⚠️ 「原版为什么是 111」**没有证据**：`game_room_entry.tscn` 里这一列没有
	#   `custom_minimum_size`（三种 size_flags 都照抄了），行是 Rust `GraphwarLanVBox` 建的
	#   （`docs/144` §1）⇒ **推断**：最小值是它建行时给的，但没反编译到那一行。
	players_m.custom_minimum_size = Vector2(112, 0)
	GwViews.attach(players_m, "LanLobbyFontResize.gd")
	var count := GwUI.label(players, 0, HORIZONTAL_ALIGNMENT_CENTER)
	count.name = "PlayerCount"
	count.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	players_m.add_child(count)

	var mode_m := GwUI.margin("ModeContainer", 0, row)
	mode_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	GwViews.attach(mode_m, "LanLobbyFontResize.gd")
	var options := GwUI.hbox("OptionsContainer", GwUI.SEPARATION, mode_m)
	options.size_flags_stretch_ratio = 0.5
	# 这两个垫片在 `.tscn` 里是 `size_flags_horizontal = 3`（EXPAND）的 MarginContainer，
	# 不是固定宽度的弹簧 —— 4 个模式块各占 OptionsContainer 的 1/6，两侧各留 1/6。
	var left_pad := GwUI.margin("MarginContainer", 0, options)
	left_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for i in MODE_ICONS.size():
		var aspect := AspectRatioContainer.new()
		aspect.name = "AspectRatioContainer" if i == 0 else "AspectRatioContainer%d" % (i + 1)
		aspect.ratio = 1.0
		aspect.stretch_mode = AspectRatioContainer.STRETCH_FIT
		aspect.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var b := GwButton.make("")
		b.name = ["AxisModeButton", "FunctionModeButton", "TurnModeButton", "TimeModeButton"][i]
		b.icon = GwIcons.get_icon(MODE_ICONS[i])
		b.expand_icon = true
		b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
		b.mouse_filter = Control.MOUSE_FILTER_IGNORE
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		# 第 1 格白底、其余三格深绿底（形制各一份 .tres，见 GwThemeFactory.mode_icon_stylebox）
		var sb := GwThemeFactory.mode_icon_stylebox(i == 0)
		for state in ["normal", "hover", "pressed", "disabled"]:
			b.add_theme_stylebox_override(state, sb)
		aspect.add_child(b)
		options.add_child(aspect)
	var right_pad := GwUI.margin("MarginContainer2", 0, options)
	right_pad.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var join_m := GwUI.margin("JoinButtonContainer", 0, row)
	join_m.size_flags_horizontal = Control.SIZE_FILL
	GwViews.attach(join_m, "LanLobbyFontResize.gd")
	var join := GwButton.make(" Join ")
	join.name = "JoinButton"
	join.set_meta("room_address", address)
	if address.is_empty():
		# **没有地址的行在原版里不存在**：房间表的键就是 `SocketAddr`
		# （`GraphwarLanVBox::update_game_list<Iter<SocketAddr, GameInfo>>`），
		# 所以原版永远不会画出一行"点了却没有地址"。我们的占位行是自定的，
		# 因此这里把 Join 画出来（保真帧的形状）但**不接受点击**：
		# 点了什么都不会发生，也不会再弹"这一行没有房间地址"（用户实测的 bug）。
		join.mouse_filter = Control.MOUSE_FILTER_IGNORE
	else:
		join.pressed.connect(_on_join_pressed.bind(address))
	join_m.add_child(join)
	return row

## 点房间条目的 ` Join `。
##
## 原版这一行的 ` Join ` 就是"连这一行自己的地址"：房间表的键是 `SocketAddr`，
## 行上的数据是 `GameInfo`，`GraphwarLanVBox::process` 里按下后走的是
## `GraphwarBackend::start_client_to_remote_server(addr)`（docs/144 §2 的证据链）。
##
## 所以这里**只**做一件事：连这一行的 `address`（`App.connect_to`）。地址来自
## LAN 广播的 `address` 字段 / broker 的 `GameInfo.address`，两处都是同一个
## `host:port` 形状（`docs/24` §1.1、`docs/115` §2.2）。
##
## 空地址走不到这里 —— `_room_entry` 对没有地址的行根本不接 `pressed`
## （原版不存在这种行，见那里的注释）。这一支只留作防御。
func _on_join_pressed(address: String = "") -> void:
	if address.is_empty():
		return
	var app_node := app()
	if app_node == null:
		return
	app_node.call("connect_to", address)

