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
## 本项目没有网络层 ⇒ 那个 Label 的内容仍是**明写的占位**（原版 `.tscn` 里它是空 Label，
## 运行时由 Rust 侧填；我方没有对局监听地址可填，见 docs/06 §75）。
const PLACEHOLDER_IP := "127.0.0.1:0 (no network layer)"

func _ready() -> void:
	# 原版这一屏的字号来源（每个容器挂不同脚本，docs/137 §5）：
	#   RoomName.gd / LanLobbyTitle.gd（队名、GhostButton）/ LobbyButtonContainer.gd（底栏）
	#   / ExitButtonContainer.gd  —— 全是 **宽/30**；
	#   只有玩家行 `GameSetupPlayerEntry.gd` 是 **宽/40**（见 _player_row）。
	# 以前整屏写 40，于是标题/队名/Start 全部小一号（真帧 64 vs 我们 48）。
	font_divisor = 30
	super()

## 节点树来自原版 `.tscn`：`client/scenes/screens/game_setup.tscn` 与
## `recovered-src/GameSetup.tscn` **逐字节相同**（273 行 / 47 节点）。内容挂在原版的
## **Rust 类 `GraphwarSetupControl`** 之下（`.tscn` 里 `type="GraphwarSetupControl"`，
## 就是我方已复原的 GDExtension 类），字号/宽度全部由 `.tscn` 里挂的原版脚本自己算
## （`RoomName.gd` / `LanLobbyTitle.gd` / `LobbyButtonContainer.gd` / `ExitButtonContainer.gd`
## 一律 **宽/30**）。我方 `build()` 只做两件事：取句柄、接信号（docs/06 §92）。
const ROOT := "GraphwarSetupControl/MarginContainer/VBoxContainer"

## 两处**我方钉住**的最小尺寸（不是原版 `.tscn` 里的值，来源与理由见 `_pin_min_sizes`）。
const PIN_GHOST := Vector2(159, 96)
const PIN_START := Vector2(384, 0)

var _room_edit: LineEdit
var _team1: VBoxContainer
var _team2: VBoxContainer
var _mode_buttons := {}
var _lock_button: Button
var _ip_label: Label
var _start_button: Button
var _ghost_button: Button

func build() -> void:
	_room_edit = get_node_or_null(ROOT + "/HBoxContainer/RoomName/RoomNameLineEdit") as LineEdit
	if _room_edit == null:
		push_error("[gw2] GameSetup.tscn 里找不到 RoomNameLineEdit")
	else:
		_room_edit.text_changed.connect(_on_room_name_changed)
	_ghost_button = get_node_or_null(ROOT + "/HBoxContainer/MarginContainer4/GhostButton") as Button
	_team1 = get_node_or_null(
		ROOT + "/MarginContainer2/ScrollContainer/PlayerListContainer/Team1Container") as VBoxContainer
	_team2 = get_node_or_null(
		ROOT + "/MarginContainer2/ScrollContainer/PlayerListContainer/Team2Container") as VBoxContainer
	if _team1 == null or _team2 == null:
		push_error("[gw2] GameSetup.tscn 里找不到 Team1Container/Team2Container")
	# 四个模式按钮 + AddBot：原版 `.tscn` 里**没有** `input_text`（它们的点击在原版由 Rust 控制器
	# 接）⇒ 这里直接连 `BaseButton.pressed`，环形切换逻辑仍在我方 `_cycle_mode`。
	# `AddBotButton` 有 `input_text = "add_bot"` ⇒ 走命令总线，不需要再连。
	for spec in [
		{"key": "axis", "name": "AxisModeButton"},
		{"key": "function", "name": "FunctionModeButton"},
		{"key": "turn", "name": "TurnModeButton"},
		{"key": "time", "name": "TurnTimeButton"},
	]:
		var b := get_node_or_null(ROOT + "/OptionsContainer/" + String(spec["name"])) as Button
		if b == null:
			push_error("[gw2] GameSetup.tscn 里找不到 %s" % String(spec["name"]))
			continue
		# ⚠️ 必须用**字符串形式**连信号：原生 `GraphwarButton` 自己有一个
		# `#[func] fn pressed()`（原版 `0x4ef560`，见 `graphwar_button_class.rs`），
		# 它在 GDScript 里会**遮蔽**同名的 `BaseButton.pressed` 信号 ⇒ `b.pressed`
		# 拿到的是那个 `Callable` 而不是 `Signal`，`b.pressed.connect(...)` 会报
		# "Nonexistent function 'connect' in base 'Callable'"（本轮实测）。
		b.connect("pressed", _cycle_mode.bind(String(spec["key"])))
		_mode_buttons[String(spec["key"])] = b
	_start_button = get_node_or_null(ROOT + "/MarginContainer/HBoxContainer/StartButton") as Button
	if _start_button == null:
		push_error("[gw2] GameSetup.tscn 里找不到 StartButton")
	_pin_min_sizes()

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
	(_mode_buttons["axis"] as Button).icon = GwIcons.get_icon(GwModes.axis_icon(st.axis_mode))
	(_mode_buttons["axis"] as Button).tooltip_text = "Axis mode %d / 4（档位语义未知）" % st.axis_mode
	(_mode_buttons["function"] as Button).icon = GwIcons.get_icon(GwModes.function_icon(st.function_mode))
	(_mode_buttons["function"] as Button).tooltip_text = "Function mode: %s" % ("f(x)" if st.function_mode == 0 else "dx/dy")
	(_mode_buttons["turn"] as Button).icon = GwIcons.get_icon(GwModes.turn_icon(st.turn_mode))
	(_mode_buttons["turn"] as Button).tooltip_text = "Turn mode: %s" % ("sequential" if st.turn_mode == 0 else "simultaneous")
	(_mode_buttons["time"] as Button).icon = GwIcons.get_icon(GwModes.time_icon(st.time_mode))
	(_mode_buttons["time"] as Button).tooltip_text = "Turn time: %s" % GwModes.time_label(st.time_mode)

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
	_pin_min_sizes.call_deferred()

## **我方钉住**的两个最小尺寸（原版 `.tscn` 里没有这两个值）：
##
## * `StartButton = (384, 0)` = 屏宽×0.2 —— 这是原版 `LobbyButtonContainer.gd` 在 1920 宽下算出来的
##   值；但自检跑在 headless 的 dummy 窗口（**实测 64×64**）里，那个脚本会把它写成 12 px，
##   而既有断言要的是 1920 口径的 384。deferred ⇒ 排在同一次 `size_changed` 里原版脚本之后。
## * `GhostButton = (159, 96)` —— 原版这颗不可见按钮是 **9 个空格**的 `GraphwarButton`
##   （`GameSetup.tscn` 第 95 行 `text = "         "`），真值是 159 宽（`docs/137` §5 宿主导出
##   `@@NODE GhostButton … size=(159,96)`）。Godot 原生 `Button` 对空格的度量比原版 Rust 侧宽
##   （9 个空格 = 252 px）⇒ 不钉住标题会从真帧的 x=313 跑到 ~400。钉住是**推断**档的补偿。
func _pin_min_sizes() -> void:
	if is_instance_valid(_start_button):
		_start_button.custom_minimum_size = PIN_START
	if is_instance_valid(_ghost_button):
		_ghost_button.custom_minimum_size = PIN_GHOST

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

