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

func _ready() -> void:
	theme = GwThemeFactory.build()
	screens_control = $VBoxContainer/ScreensControl
	_build_toast()
	_instantiate_screens()
	App.show_screen("MainMenu")
	App.toast.connect(_on_toast)
	_handle_cmdline()

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
