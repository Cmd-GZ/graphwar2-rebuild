extends Control
## 客户端根节点。等价于原版 `main.tscn` 的 `GraphwarRootNode/VBoxContainer/ScreensControl`：
## 把 15 个界面全部实例化好、只切 `visible`（原版就是这么做的，见 `docs/92` §3）。
##
## 命令行（都是 `--` 之后的用户参数，Godot 自身不会吃掉）：
##   godot --headless --path client -- --selftest     跑自检（tools/selftest.gd），退出码 0/1
##   godot --headless --path client -- --dump-tree    打印整棵节点树后退出（格式仿 docs/92 的 @@N 行）
##   xvfb-run -a godot --path client --rendering-driver opengl3 -- --shots=<dir>
##                                                    逐界面截图（需要真实渲染，headless 出不了图）

const SCREEN_ORDER := [
	# 基础界面（顺序 = 原版 ScreensControl 子节点顺序，见 docs/92 §3）
	"MainMenu", "PlayScreen", "Campaign", "LanLobby", "GameSetup", "PlayerScreen", "ReplayLobby",
	# 弹窗（同属 ScreensControl，叠在基础界面之上）
	"ChatPopUp", "JoinIpPopUp", "ConnectingPopUp", "DisconnectedPopUp",
	"JoinLevelPopUp", "AddBotPopUp", "EndGamePopUp", "ConfirmPlayExitPopUp",
]

## 原版把最后一个叫 `ConfirmPlayExitPopUp`（节点名），场景文件是 `confirm_exit_pop_up.tscn`。
## 这里保留原版节点名，方便与 docs/92 的台账逐条对照。
const SCENE_FILES := {
	"MainMenu": "main_menu",
	"PlayScreen": "play_screen",
	"Campaign": "campaign",
	"LanLobby": "lan_lobby",
	"GameSetup": "game_setup",
	"PlayerScreen": "player_screen",
	"ReplayLobby": "replay_lobby",
	"ChatPopUp": "chat_pop_up",
	"JoinIpPopUp": "join_ip_pop_up",
	"ConnectingPopUp": "connecting_pop_up",
	"DisconnectedPopUp": "disconnected_pop_up",
	"JoinLevelPopUp": "join_level_pop_up",
	"AddBotPopUp": "add_bot_pop_up",
	"EndGamePopUp": "endgame_pop_up",
	"ConfirmPlayExitPopUp": "confirm_exit_pop_up",
}

var screens_control: Control
var _toast_label: Label
var _toast_timer := 0.0

## 资产检查结果（`_require_assets()` 填）。selftest 用它决定要不要跳过图标/字体断言。
var assets_ok := false
var missing_assets: Array[String] = []

## 缺资产时给用户看的**可执行**提示。绝不"静默退化"成自绘图标（docs/121 §5）。
const ASSET_HELP := """[gw2] 缺少媒体资源目录：client/assets/
  本仓库**不存放任何媒体文件**（图标 / 字体 / 主题都是原版资产，版权原因不入库）。
  请在你自己的 Graphwar.pck 上跑一次提取：

      cd <仓库根> && python3 tools/extract_assets.py
      # 或显式指定包：python3 tools/extract_assets.py --pck /path/to/Graphwar.pck

  产出的 client/assets/ 已 gitignore。提取完直接重跑本程序即可，不需要重新 --import。
"""

func _ready() -> void:
	if not _require_assets():
		return
	theme = GwThemeFactory.build()
	screens_control = $VBoxContainer/ScreensControl
	_build_toast()
	_instantiate_screens()
	App.show_screen("MainMenu")
	App.toast.connect(_on_toast)
	_handle_cmdline()

## 资产硬检查。返回 true 表示可以继续建房；false 表示已经退出（GUI 模式）。
##
## 规则（`docs/121` §5.3）：
##   * **GUI 模式**：缺 `client/assets/` 就打印可执行的补救命令并 `quit(4)` —— 不静默退化；
##   * **selftest 模式**：允许继续，但把"跳过图标/字体断言"**显式报告**出来（`tools/selftest.gd`）。
func _is_selftest() -> bool:
	return OS.get_cmdline_user_args().has("--selftest")

func _require_assets() -> bool:
	assets_ok = GwAssets.available()
	if assets_ok:
		var need := GwIcons.all_referenced_paths()
		need.append(GwThemeFactory.FONT_MAIN)
		need.append(GwThemeFactory.FONT_TITLE)
		missing_assets = GwAssets.check(need)
		if not missing_assets.is_empty():
			assets_ok = false
	if assets_ok:
		return true
	if missing_assets.is_empty():
		missing_assets = ["（整个 client/assets/ 目录缺失，或没有 MANIFEST.tsv）"]
	print(ASSET_HELP)
	if _is_selftest():
		push_warning("[gw2] 无资产目录：本次 selftest 跳过图标/字体断言（会显式报告）")
		return true
	printerr("[gw2] 缺资产，拒绝启动 —— 不做占位图冒充实物")
	get_tree().quit(4)
	return false

func _instantiate_screens() -> void:
	for screen_name in SCREEN_ORDER:
		var file: String = SCENE_FILES[screen_name]
		var path := "res://scenes/screens/%s.tscn" % file
		if not ResourceLoader.exists(path):
			push_error("[gw2] 缺少界面场景：%s" % path)
			continue
		var scene: PackedScene = load(path)
		var node: Control = scene.instantiate()
		node.name = screen_name
		node.set_anchors_preset(Control.PRESET_FULL_RECT)
		screens_control.add_child(node)
		App.register_screen(screen_name, node)

func _build_toast() -> void:
	_toast_label = Label.new()
	_toast_label.name = "Toast"
	_toast_label.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	_toast_label.offset_top = -36
	_toast_label.offset_bottom = -6
	_toast_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_toast_label.add_theme_color_override("font_color", GwThemeFactory.COL_WARN)
	_toast_label.text = ""
	_toast_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_toast_label)

# ---------------------------------------------------------------- 调试覆盖层（默认关）

## 客户端自己的版本号（**不是**原版的 `2.0.60`；那是 VersionLabel 显示的工程版本）。
const CLIENT_VERSION := "0.1.0"

## 显式开关：`godot --path client -- --debug-overlay`
const DEBUG_OVERLAY_ARG := "--debug-overlay"

var _debug_label: Label

## 状态行默认**不画在界面上**：原版主菜单右下角只有 `2.0.60`，没有内核状态、
## 没有客户端版本（用户截图对比就是这么来的）。要调试时显式加 `--debug-overlay`，
## 状态行出现在左下角，仍然是一眼能看出"这是我们的构建"的自陈（docs/124 §2.4）。
func _install_debug_overlay() -> void:
	if not OS.get_cmdline_user_args().has(DEBUG_OVERLAY_ARG):
		return
	_debug_label = Label.new()
	_debug_label.name = "DebugOverlay"
	_debug_label.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	_debug_label.offset_left = 8.0
	_debug_label.offset_top = -30.0
	_debug_label.offset_right = 720.0
	_debug_label.offset_bottom = -4.0
	_debug_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	_debug_label.add_theme_font_size_override("font_size", 14)
	_debug_label.add_theme_color_override("font_color", GwThemeFactory.COL_TEXT_DIM)
	_debug_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_debug_label.text = debug_status_text()
	add_child(_debug_label)

## 内核状态的**文本形态**。`dump` 与 `--debug-overlay` 共用它，selftest 也从这里读
## ⇒ 界面上不显示 ≠ 状态丢了（docs/124 §2.4）。
static func debug_status_text() -> String:
	return "client %s · kernel %s" % [CLIENT_VERSION,
		"on" if GwCore.kernel_available() else "off"]

## `dump` 通道里的内核状态行（机器可读）。**这是内核状态的权威出口**：
## 断言、脚本、别的 agent 都从这里读，不再依赖界面上那行字。
static func kernel_dump_line() -> String:
	return "@@KERNEL available=%d hit_test=%d methods=%s note=%s" % [
		int(GwCore.kernel_available()),
		int(GwCore.kernel_has_hit_test()),
		",".join(GwCore.kernel_methods()),
		GwCore.kernel_note().replace("\n", " "),
	]

## `dump` 通道里的工程设置行：视口、拉伸模式、版本 —— 都是本轮几何对齐的关键键。
static func project_dump_line() -> String:
	var stretch := String(ProjectSettings.get_setting("display/window/stretch/mode", "disabled"))
	return "@@PROJECT version=%s stretch_mode=%s viewport=%dx%d mode=%d" % [
		String(ProjectSettings.get_setting("application/config/version", "")),
		stretch,
		int(ProjectSettings.get_setting("display/window/size/viewport_width", 0)),
		int(ProjectSettings.get_setting("display/window/size/viewport_height", 0)),
		int(ProjectSettings.get_setting("display/window/size/mode", 0)),
	]

func _on_toast(text: String) -> void:
	_toast_label.text = text
	_toast_timer = 5.0
	print("[toast] %s" % text)

func _process(delta: float) -> void:
	if _toast_timer > 0.0:
		_toast_timer -= delta
		if _toast_timer <= 0.0:
			_toast_label.text = ""

func _handle_cmdline() -> void:
	var args := OS.get_cmdline_user_args()
	_install_debug_overlay()
	if args.has("--dump-tree"):
		_dump_and_quit.call_deferred()
	elif args.has("--selftest"):
		_run_selftest.call_deferred()
	for a in args:
		if a.begins_with("--shots="):
			_take_screenshots.call_deferred(a.substr(8))

func _dump_and_quit() -> void:
	await get_tree().process_frame
	await get_tree().process_frame
	# 机器可读的状态行放最前面：内核状态与工程设置**不再画在界面上**，但必须可查
	# （docs/124 §2.4）。`run.sh dump` 的输出里 grep 这两行即可。
	print(project_dump_line())
	print(kernel_dump_line())
	print("@@TREE_BEGIN")
	_dump_node(self, "")
	print("@@TREE_END")
	get_tree().quit(0)

## 仿 `docs/92` §1 的 dump 格式：`路径|类|visible/is_visible_in_tree|text`。
func _dump_node(node: Node, prefix: String) -> void:
	var path := prefix + "/" + String(node.name) if prefix != "" else String(node.name)
	var kind := node.get_class()
	var vis := "-"
	if node is CanvasItem:
		vis = "1" if (node as CanvasItem).visible else "0"
		if node.is_inside_tree():
			vis += "1" if (node as CanvasItem).is_visible_in_tree() else "0"
	var text := ""
	if "text" in node:
		text = String(node.get("text")).replace("\n", "\\n").replace("|", "¦")
	var extra := ""
	if node is Control:
		var c := node as Control
		extra += " rect=%s+%s" % [str(c.position.round()), str(c.size.round())]
	if node is GwButton:
		var b := node as GwButton
		if b.input_text != "":
			extra += " cmd=%s" % b.input_text
		if b.long_press_input_text != "":
			extra += " long_cmd=%s" % b.long_press_input_text
		if b.disabled:
			extra += " disabled"
	print("@@%s|%s|%s|%s|%s" % [path, kind, vis, text, extra])
	for child in node.get_children():
		_dump_node(child, path)

func _run_selftest() -> void:
	# 看门狗：自检里的 await 万一卡住，也要在 120 s 后带非零码退出（CI 不能挂死）。
	var watchdog := get_tree().create_timer(120.0)
	watchdog.timeout.connect(func():
		print("@@SELFTEST_TIMEOUT")
		get_tree().quit(3)
	)
	var script: GDScript = load("res://tools/selftest.gd")
	if script == null:
		print("@@SELFTEST_LOAD_FAILED")
		get_tree().quit(2)
		return
	var runner: Node = script.new()
	if runner == null or not runner.has_method("run"):
		print("@@SELFTEST_BAD_RUNNER")
		get_tree().quit(2)
		return
	add_child(runner)
	var code: int = await runner.run(self)
	print("@@SELFTEST_DONE code=%d" % code)
	get_tree().quit(code)

## 逐界面截图。只在有渲染后端时可用（headless 的 dummy 后端拿不到画面）。
## 用 `xvfb-run -a ... --rendering-driver opengl3 -- --shots=/abs/dir` 跑，见 tools/shots.sh。
func _take_screenshots(out_dir: String) -> void:
	DirAccess.make_dir_recursive_absolute(out_dir)
	var runner: Node = load("res://tools/screenshots.gd").new()
	add_child(runner)
	var code: int = await runner.call("run", self, out_dir)
	print("@@SHOTS_DONE code=%d dir=%s" % [code, out_dir])
	get_tree().quit(code)
