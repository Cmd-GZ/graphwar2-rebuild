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
## 当前每行的 `GameName` 标签（自检/对拍用；也是 `_rescale()` 要改 `custom_minimum_size` 的那批）。
var _game_labels: Array[Label] = []
## 本屏的"条目元数据"缓存：`path → GwReplayMeta`（重进屏时复用，不重复解压）。
var _meta_cache := {}

## 原版 `ReplayName` / `GameName` 的字号除数（`LanLobbyFontResize.gd` / `GameNameContainer.gd`:
## 两者都是 `屏宽 / 40`；GameNameContainer 还额外给子节点钉 `custom_minimum_size.x = 屏宽 × 0.24`）。
const FONT_DIVISOR_ROW := 40
const GAME_NAME_WIDTH_RATIO := 0.24

## 四个模式图标的节点名（`replay_entry.tscn` 的顺序，不带 `2/3/4` 后缀的那个是第一格）。
const MODE_NODE_NAMES := ["AxisModeButton", "FunctionModeButton", "TurnModeButton", "TimeModeButton"]

func _ready() -> void:
	font_divisor = 30        # 标题/返回键的除数由各自的视图脚本管；这里只兜底
	super()
	get_tree().root.size_changed.connect(_rescale)

func build() -> void:
	var f := GwUI.frame(self)
	var vbox: VBoxContainer = f.vbox
	var hdr := build_header(vbox, "REPLAYS", GwCommands.EXIT_REPLAY_LOBBY, true)
	# 返回键的圆角：原版 `replay_lobby.tscn` 的 `ClipButton` 带 `clip_children = 2`
	# （CLIP_CHILDREN_ONLY），所以它那个面板色的 `AnimationRect` 子节点被**裁到圆角形状内**，
	# 四个弯角露出的是底色。少了这一条，弯角处会多出 152 个面板色像素（max|Δch|=22）。
	# 真帧证据：原版 (0,0)=(14,37,28)=底色，我方 (26,59,45)=面板色（docs/06 §56.5）。
	# ⚠️ 这是 `GwUI.exit_pair` 的**既有缺陷**（影响每一屏的返回键），但 `client/src/ui/ui.gd`
	# 不在本任务允许改的文件里 ⇒ 这里就地补一条，并把根因写进 docs/16 §62.6 建议统一修。
	var exit_holder: Node = hdr.exit
	var clip_btn: Node = exit_holder.get_node_or_null("ClipButton") if exit_holder != null else null
	if clip_btn is CanvasItem:
		(clip_btn as CanvasItem).clip_children = CanvasItem.CLIP_CHILDREN_ONLY

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
	_game_labels.clear()
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
	# 关卡名另走 `_scale_game_label()`（原版 `GameNameContainer.gd` 的 宽/40 + 宽×0.24），
	# 这里这一句是给本屏其它晚建的文本兜底。
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
	# 原版 `GameNameContainer.gd` 的两条（本客户端没有这个视图脚本文件，行为照抄，
	# 见 docs/16 §62.5）：字号 = 屏宽/40；子节点 `custom_minimum_size = (屏宽×0.24, 0)`。
	# ⚠️ 必须**显式**写字号：本屏的行是 `on_enter()` 里建的，那时 `GwScreen` 的
	# `apply_font_scale()` 早已跑过（它只在 `_ready` / 窗口尺寸变化时跑），
	# 靠 `meta("font_divisor")` 会停在主题默认 24（实测：标签高 34 而不是 67）。
	_scale_game_label(game_label)
	game_m.add_child(game_label)
	_game_labels.append(game_label)
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

## 把原版 `GameNameContainer.gd` 的两条规则应用到关卡名标签上。
func _scale_game_label(lbl: Label) -> void:
	var w := _viewport_width()
	lbl.custom_minimum_size = Vector2(w * GAME_NAME_WIDTH_RATIO, 0)
	lbl.add_theme_font_size_override("font_size", maxi(10, int(w / float(FONT_DIVISOR_ROW))))

func _viewport_width() -> float:
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		w = 1920.0
	return w

## 窗口尺寸变了：原版 `GameNameContainer.gd` 按屏宽重算**字号**与
## `custom_minimum_size`（两条都是 屏宽 的函数）⇒ 这里照做。
func _rescale() -> void:
	for lbl in _game_labels:
		if is_instance_valid(lbl):
			_scale_game_label(lbl)

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
