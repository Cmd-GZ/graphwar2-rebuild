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

## 原版**行场景**（`replay_entry.tscn`）。行是运行时按回放条数实例化的，不是本屏 `.tscn` 的子节点。
const ENTRY_SCENE_PATH := "res://scenes/screens/replay_entry.tscn"
var _entry_packed: PackedScene

func _entry_scene() -> PackedScene:
	if _entry_packed == null:
		_entry_packed = load(ENTRY_SCENE_PATH) as PackedScene
	return _entry_packed

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

## 一条回放 —— **直接把原版 `replay_entry.tscn` 实例化**（`client/scenes/screens/replay_entry.tscn`
## 与 `recovered-src/replay_entry.tscn` **逐字节相同**，130 行 / 21 节点）。我方只填这一行要显示的
## 数据、接 ▶ 的线；节点与布局**一行都不手抄**（原来这里约 120 行 `HBoxContainer.new()` 手抄树）。
##
## 原版这一行的形状（全部来自 `.tscn`，不在这里）：
##   HBoxContainer[graphwar_theme]
##     ├ ScrollContainer/NameContainer(`LanLobbyFontResize.gd` 宽/40)/ReplayName  ← 录制时间
##     ├ GameNameContainer[`GameNameContainer.gd`]/GameName                       ← 关卡名
##     ├ ModeContainer/OptionsContainer/[M, 4×(AR→模式按钮), M2]                  ← 四个模式图标
##     ├ AspectRatioContainer(stretch_mode=1,align=2)/PlayReplayButton(`play_option`)
##     └ MarginContainer（尾部等宽占位，ratio 0.1）
##
## 行的身份仍挂在**元数据**上（`replay_path` / `replay_ts`）：`ReplayName` 里是录制时间，不是
## 文件名，所以"这一行是哪份回放"只能从元数据读（自检 `_replay_play_button` 用它，docs/16 §62.7）。
func _entry(entry: Dictionary) -> HBoxContainer:
	var meta := _meta_for(String(entry.path))
	var row := _entry_scene().instantiate() as HBoxContainer
	if row == null:
		push_error("[gw2] 实例化 %s 失败" % ENTRY_SCENE_PATH)
		return null
	row.set_meta("replay_path", String(entry.path))
	row.set_meta("replay_ts", int(entry.get("ts", -1)))

	var name_label := row.find_child("ReplayName", true, false) as Label
	if name_label != null:
		name_label.text = _recorded_at(entry)
	var game_label := row.find_child("GameName", true, false) as Label
	if game_label != null:
		game_label.text = String(meta.game_name())

	# 四个模式图标随回放内容变（坐标 / 函数 / 回合 / 时限）。读不到就**不画**（置空），
	# 不拿 `.tscn` 里的默认图标冒充（原版对读不到的回放也不会画）。
	var icons := meta.mode_icons()
	for i in MODE_NODE_NAMES.size():
		var b := row.find_child(String(MODE_NODE_NAMES[i]), true, false) as Button
		if b == null:
			continue
		var want := "" if i >= icons.size() else String(icons[i])
		b.icon = GwIcons.get_icon(want) if want != "" else null

	# ▶：原版这一格 `input_text = "play_replay"`（由 Rust 控制器按行派发）；我方客户端的
	# 命令总线**不知道是哪一行** ⇒ 这里按行绑一个闭包，只走 `_on_play(path)`。
	# ⚠️ 用字符串形式连信号：原生 `GraphwarButton` 自己有个 `#[func] pressed()`，
	# 会遮蔽 `BaseButton.pressed` 信号（docs/06 §92.2）。
	var play := row.find_child("PlayReplayButton", true, false) as Button
	if play != null:
		play.connect("pressed", _on_play.bind(String(entry.path)))
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

