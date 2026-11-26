extends GwScreen
## ReplayLobby —— 回放列表。
##
## 结构（`docs/92` §4.15，恢复出的 replay_lobby.tscn 只有 13 个静态节点）：
##   MarginContainer/VBoxContainer
##     ├ HBoxContainer/{ExitButtonContainer(exit_replay_lobby), MarginContainer/Label("REPLAYS")}
##     ├ ScrollContainer/ReplaysContainer/<条目>   ← replay_entry.tscn 实例（21 节点/条）
##     └ AdContainer(0,60)
##
## ## 一行是什么（**观测**：原版真帧 + 恢复出的 `replay_entry.tscn`，`docs/06` §56）
##
## 原版一行 = 5 段，**逐段与原版 `.tscn` 的节点一一对应**（`recovered-src/replay_entry.tscn`）：
##
## ```text
## HBoxContainer                                    ← 一行（separation 4）
## ├ ScrollContainer / MarginContainer(NameContainer) / Label(ReplayName)
## │     录制时间 **带秒**、按 **UTC**：`2026-11-05 16:48:18`（字号 = 宽/40）
## ├ MarginContainer(GameNameContainer) / Label(GameName)
## │     **关卡名**：`Level 0`（来自回放内的 `GameNameInfo.game_name`，见下）
## ├ MarginContainer(ModeContainer) / HBoxContainer(OptionsContainer)
## │     [空占位][4 个模式图标][空占位] —— 每格是 AspectRatioContainer(1:1) +
## │     GraphwarButton(expand_icon)。**图标随回放内容变**（坐标模式 / 函数模式 /
## │     回合模式 / 时限），第 1 格白底（`axis_button_theme`），其余是面板色。
## ├ AspectRatioContainer / GraphwarButton(PlayReplayButton)
## │     ▶ —— `play_option.svg`，**67×67**
## └ MarginContainer（尾部等宽占位）
## ```
##
## ## 三件本轮纠正的事（都有真帧证据，`docs/06` §56）
##
## 1. **旧注释说"房间名/四个模式不在列表里解析：那要把每份 gzip 整个解压一遍（400 份 ≈ 64 MB）"**
##    —— 这与观测矛盾，已删。实测语料 403 份 gzip 合计 **1.09 MB**、明文合计约 **24 MB**，
##    旧注释的 64 MB 数字本身就偏大；更关键的是**原版 400 份列表照样显示关卡名**，
##    所以"所以不显示"这个结论是错的。现在由 Rust 侧 `GwReplayMeta`
##    （`src/_scaffold/replay_meta.rs`）逐文件读一次，只解析头部 64 KiB。
## 2. **录制时间用文件名（unix 时间戳）而不是 `mtime`**：原版真帧 `1793897298` 显示
##    `2026-11-05 16:48:18`（= 该时间戳的 UTC），并**带秒**；`mtime` 是拷贝时间。
## 3. **去掉文件名列与重复的字节数列**：原版没有这两列（真帧逐像素：行 `y∈[100,166]`
##    只有上面 5 段，左右两端的空白一直到底色）。
##
## ## 本项目：扫 `user://replays/`
##
## 用 `GwReplayFile.list_dir` 列举，不读原版存档目录。
## **文件名就是 unix 时间戳、没有扩展名**（`docs/78` §1 观测），所以这里**不按扩展名过滤**
## —— `replay_index.gd` 的 `EXTENSIONS` 白名单会把**所有真实回放**都筛掉，这正是
## 之前"回放列表永远空着"的原因（`docs/133` §4.4）。
##
## 关卡名/模式读不到时（空回放、坏文件）**不猜**：关卡名留空、四个图标不画。

var _list: VBoxContainer
var _selected := ""
## 扫哪个目录。默认 `user://replays`；自检可以指向真实存档目录做**只读**列举。
var _scan_dir := "user://replays"
## 本屏的"条目元数据"缓存：`path → GwReplayMeta`（重进屏时复用，不重复解压）。
var _meta_cache := {}

## 四个模式图标的节点名（`replay_entry.tscn` 的顺序，不带 `2/3/4` 后缀的那个是第一格）。
const MODE_NODE_NAMES := ["AxisModeButton", "FunctionModeButton", "TurnModeButton", "TimeModeButton"]

func _ready() -> void:
	font_divisor = 30        # 标题/返回键的除数由各自的视图脚本管；这里只兜底
	super()

func build() -> void:
	# **节点树来自原版 `recovered-src/replay_lobby.tscn`（逐字节装进 `client/scenes/screens/`）**：
	# MarginContainer(VBoxContainer → HBoxContainer(ExitButtonContainer/{ClipButton,LongPressButton},
	# MarginContainer/Label("REPLAYS")) + ScrollContainer/ReplaysContainer + AdContainer)。
	# 这里只做接线：取列表容器 + 把那 152 个弯角像素的裁切条件按 .tscn 的写法补上。
	var vbox: VBoxContainer = get_node("MarginContainer/VBoxContainer")
	_list = vbox.get_node("ScrollContainer/ReplaysContainer") as VBoxContainer
	# 原版 `ClipButton` 的 `clip_children = 2`（CLIP_CHILDREN_ONLY）**已经写在 .tscn 里**了，
	# 见 `docs/06` §56.5 / §62 那条弯角证据 —— 不再由代码补。


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
	# 行是**运行时**建的（不在 `_ready` 那个时点），所以字号要自己补一次：
	# `GwScreen.apply_font_scale()` 只在 `_ready` / 窗口尺寸变化时跑，晚建出来的标签
	# 它会漏掉（实测：关卡名停在主题默认 24 ⇒ 行高 34 而不是 67）。
	# 这里这一句只给本屏其它**晚建的文本**兜底（关卡名现在由原版 `GameNameContainer.gd`
	# 自己算，见 `_entry()`）。
	apply_font_scale()

# ---------------------------------------------------------------- 一行

## 一条回放（原版 `replay_entry.tscn`，21 节点；节点名/层数都照它）。
func _entry(entry: Dictionary) -> HBoxContainer:
	var meta := _meta_for(String(entry.path))

	var row := HBoxContainer.new()
	row.name = "HBoxContainer"
	row.add_theme_constant_override("separation", GwUI.SEPARATION)
	# 行的身份挂在元数据上：`ReplayName` 标签里现在是**录制时间**（原版如此，不再是文件名），
	# 所以"这一行是哪份回放"只能从元数据读（自检 `_replay_play_button` 用它，见 docs/16 §62.7）。
	row.set_meta("replay_path", String(entry.path))
	row.set_meta("replay_ts", int(entry.get("ts", -1)))
	# 原版这一层挂 `graphwar_theme.tres`（默认字体 Comic Relief Bold）⇒ 我们自己不设字体，
	# 由 app 主题给（`GwThemeFactory` 的 default_font = 原版 `button_font`），见 docs/06 §56.4。

	# ---- 第 1 段：录制时间（带秒，UTC）------------------------------------
	var name_scroll := ScrollContainer.new()
	name_scroll.name = "ScrollContainer"
	name_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# 原版只关了**竖直**滚动（`vertical_scroll_mode = 0`）；水平仍是默认 auto。
	name_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	var name_m := MarginContainer.new()
	name_m.name = "NameContainer"
	name_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# 原版这一层挂 `LanLobbyFontResize.gd`（字号 = 屏宽/40）⇒ 直接挂同一份原版脚本。
	GwViews.attach(name_m, "LanLobbyFontResize.gd")
	var name_label := GwUI.label(_recorded_at(entry), 0, HORIZONTAL_ALIGNMENT_CENTER)
	name_label.name = "ReplayName"
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_m.add_child(name_label)
	name_scroll.add_child(name_m)
	row.add_child(name_scroll)

	# ---- 第 2 段：关卡名 --------------------------------------------------
	var game_m := MarginContainer.new()
	game_m.name = "GameNameContainer"
	game_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	game_m.size_flags_stretch_ratio = 0.1        # 原版：size_flags_stretch_ratio = 0.1
	var game_label := GwUI.label(String(meta.game_name()), 0, HORIZONTAL_ALIGNMENT_CENTER)
	game_label.name = "GameName"
	game_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	# **原版字节**：`recovered-src/GameNameContainer.gd` 挂在 `replay_entry.scn` 的这个
	# `MarginContainer(GameNameContainer)` 上（观测：31 份 .scn 里 `res://GameNameContainer.gd`
	# 只出现在 replay_entry.scn）。它自己算两条：子节点 `min.x = 屏宽×0.24`、字号 `屏宽/40`
	# —— 我方原来的 `_scale_game_label()`（含自加的 `maxi(10, …)` 下限）已整份删除。
	# 挂法：建 → 填子节点 → attach → 再 add_child（原版 `_ready` 只遍历当次的 get_children()）。
	game_m.add_child(game_label)
	GwViews.attach(game_m, "GameNameContainer.gd")
	row.add_child(game_m)

	# ---- 第 3 段：四个模式图标 --------------------------------------------
	var mode_m := MarginContainer.new()
	mode_m.name = "ModeContainer"
	mode_m.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var options := HBoxContainer.new()
	options.name = "OptionsContainer"
	options.size_flags_stretch_ratio = 0.5       # 原版：size_flags_stretch_ratio = 0.5
	options.add_theme_constant_override("separation", GwUI.SEPARATION)
	mode_m.add_child(options)
	# 原版 `OptionsContainer` 是 **6 个槽**：首尾两个空 MarginContainer 与 4 个
	# AspectRatioContainer 各占 1 份 stretch ⇒ 图标块左右对称留白。
	var lead := MarginContainer.new()
	lead.name = "MarginContainer"
	lead.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	options.add_child(lead)
	var icons := meta.mode_icons()
	for i in 4:
		var holder := AspectRatioContainer.new()
		holder.name = "AspectRatioContainer" if i == 0 else "AspectRatioContainer%d" % (i + 1)
		holder.ratio = 1.0                       # 原版：ratio = 1.0（默认）
		holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		# 原版**没有**写 stretch_mode ⇒ 默认 STRETCH_FIT（等比塞进格子）。
		holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
		var b := GwButton.make("")
		b.name = String(MODE_NODE_NAMES[i])
		b.icon = GwIcons.get_icon(icons[i]) if String(icons[i]) != "" else null
		b.expand_icon = true                     # 原版：expand_icon = true
		b.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		# 底色：原版第一格挂 `axis_button_theme.tres` = **白底**，其余三格挂
		# `game_menu_icon_theme.tres` = 面板色；两者除底色逐字段相同（docs/129 §4.5）。
		var sb := GwThemeFactory.mode_icon_stylebox(i == 0)
		for state in ["normal", "hover", "pressed", "disabled"]:
			b.add_theme_stylebox_override(state, sb)
		holder.add_child(b)
		options.add_child(holder)
	var trail := MarginContainer.new()
	trail.name = "MarginContainer2"
	trail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	options.add_child(trail)
	row.add_child(mode_m)

	# ---- 第 4 段：▶ -------------------------------------------------------
	var play_holder := AspectRatioContainer.new()
	play_holder.name = "AspectRatioContainer"
	play_holder.ratio = 1.0
	# 原版 `replay_entry.tscn`：`stretch_mode = 1` = **高度定宽度**（默认是 2 = FIT，
	# 所以 `.tscn` 才会把它显式写出来）、`alignment_horizontal = 2` = 靠右。
	# 靠"高度定宽度" + 行高 67 ⇒ 子节点 67×67；靠右 ⇒ 贴住行右缘（真帧 1780..1846）。
	play_holder.stretch_mode = AspectRatioContainer.STRETCH_HEIGHT_CONTROLS_WIDTH
	play_holder.alignment_horizontal = AspectRatioContainer.ALIGNMENT_END
	var play := GwButton.make("")
	play.name = "PlayReplayButton"
	play.icon = GwIcons.get_icon("play_option")
	play.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	play.expand_icon = true
	play.pressed.connect(_on_play.bind(String(entry.path)))
	play_holder.add_child(play)
	row.add_child(play_holder)

	# ---- 第 5 段：尾部等宽占位（原版 `replay_entry.tscn` 最后一个 MarginContainer）----
	var tail := MarginContainer.new()
	tail.name = "MarginContainer"
	tail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tail.size_flags_stretch_ratio = 0.1
	row.add_child(tail)

	return row

## 录制时间：**文件名**是 unix 时间戳（`docs/78` §1），按 **UTC** 显示到秒。
##
## 证据（`docs/06` §56.1）：原版真帧 `1793897298 → 2026-11-05 16:48:18`，而同一份文件的
## `mtime` 是拷贝时间 ⇒ 原版读的是**文件名**。`mtime` 只在该行没有时间戳文件名时兜底。
func _recorded_at(entry: Dictionary) -> String:
	var ts := int(entry.get("ts", -1))
	if ts < 0:
		ts = int(entry.get("mtime", 0))
	if ts <= 0:
		return ""
	return GwReplayMeta.new().format_utc(ts)

## 一行要的元数据（关卡名 + 四个模式）。**一次文件读、只解析头部 64 KiB**。
## 读不到时返回 `ReplayMeta{ok=false}`，界面留空 —— 不猜默认档。
func _meta_for(path: String) -> GwReplayMeta:
	if _meta_cache.has(path):
		return _meta_cache[path]
	var m := GwReplayMeta.new()
	m.read(ProjectSettings.globalize_path(path))
	_meta_cache[path] = m
	return m

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
