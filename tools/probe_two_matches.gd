extends Node
# Hashmap-determinism harness (agent/hashmap-determinism).
# Based on the stock probe.gd; adds a "play two matches in one process" state
# machine so we can compare intra-process vs cross-process message order.
var frames := 0
var root: Node
var fi: Node
var fired := 0
var start_frame: int = 320

func _init() -> void:
	var sf := OS.get_environment("GW2_START_FRAME")
	if sf != "":
		start_frame = int(sf)

const SETUP = "VBoxContainer/ScreensControl/GameSetup/GraphwarSetupControl/MarginContainer/VBoxContainer"
const POP = "VBoxContainer/ScreensControl/AddBotPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2"
const CONFIRM = "VBoxContainer/ScreensControl/ConfirmPlayExitPopUp"
const PLAY_EXIT = "VBoxContainer/ScreensControl/PlayScreen/MarginContainer/VBoxContainer/PlayMainHBox/LeftControls/LeftTopButtonsContainer/ExitButtonContainer/LongPressButton"
var FIXED_FUNC := OS.get_environment("GW2_FIRE_FUNC")
const FUNCS = ["0", "x/5", "x/2", "x", "2*x", "-x/5", "-x/2", "-x", "-2*x", "x/10", "-x/10"]

var in_match := 0          # how many matches started so far
var exit_at := -1          # frame at which match 1 is force-exited (0 = never)
var start2 := -1           # frame at which match 2 is started
var verbose := false


# --- markers: wall-clock timestamps written to a file so frida captures can be aligned ---
func _mark(tag: String) -> void:
	var t := Time.get_unix_time_from_system()
	var f := FileAccess.open("user://hashdet_markers.txt", FileAccess.READ_WRITE)
	if f == null:
		f = FileAccess.open("user://hashdet_markers.txt", FileAccess.WRITE)
	if f:
		f.seek_end()
		f.store_line("%.3f %s" % [t, tag])
		f.close()
	print("MARK ", tag)

func _press(p: String, tag: String) -> void:
	var b = root.get_node_or_null(p) if root else null
	if b:
		b.emit_signal("pressed")
		print("PRESS ", tag, " ok")
	else:
		print("PRESS ", tag, " MISSING ", p)

func _screens_dump(tag: String) -> void:
	var sc = root.get_node_or_null("VBoxContainer/ScreensControl") if root else null
	if sc == null:
		print("SCREENS ", tag, " MISSING")
		return
	var vis := []
	for c in sc.get_children():
		if c.visible:
			vis.append(String(c.name))
	print("SCREENS ", tag, " visible=", vis)

func _press_text(container_path: String, needle: String, tag: String) -> void:
	var c = root.get_node_or_null(container_path) if root else null
	if c == null:
		print("TEXTBTN ", tag, " container MISSING")
		return
	for n in c.find_children("*", "", true, false):
		var txt := ""
		if n is Label: txt = (n as Label).text
		elif n is Button: txt = (n as Button).text
		if txt.findn(needle) >= 0:
			if n is Button:
				(n as Button).emit_signal("pressed")
				print("TEXTBTN ", tag, " pressed ", n.get_path(), " text=[", txt, "]")
				return
			var b := n.get_parent()
			while b != null and not (b is Button):
				b = b.get_parent()
			if b != null:
				(b as Button).emit_signal("pressed")
				print("TEXTBTN ", tag, " pressed-parent ", b.get_path(), " via text=[", txt, "]")
				return
	print("TEXTBTN ", tag, " no button with text ", needle)

func _fire(func_text: String) -> void:
	if FIXED_FUNC != "":
		func_text = FIXED_FUNC
	if fi == null:
		print("FI_MISSING"); return
	fi.on_text_change(func_text)
	fi.on_text_submitted(func_text)
	print("SET_FUNC=", func_text)
	_press("VBoxContainer/ScreensControl/PlayScreen/MarginContainer/VBoxContainer/PlayMainHBox/RightControls/HBoxContainer/FireContainer/FireButton", "Fire")

func _ready() -> void:
	verbose = OS.get_environment("GW2_VERBOSE") != ""
	var ex := OS.get_environment("GW2_EXIT_AT")
	if ex != "": exit_at = int(ex)
	var s2 := OS.get_environment("GW2_START2")
	if s2 != "": start2 = int(s2)

func _process(_dt: float) -> void:
	frames += 1
	if frames == 60:
		root = get_tree().root.get_node_or_null("GraphwarRootNode")
		_screens_dump("f60")
		_press("VBoxContainer/ScreensControl/MainMenu/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer2/VBoxContainer2/LocalButton", "Battle")
	elif frames == 130:
		_press("VBoxContainer/ScreensControl/LanLobby/MarginContainer/VBoxContainer/MarginContainer/HBoxContainer/CreateButton", "CreateGame")
	elif frames == 190:
		_press(SETUP + "/OptionsContainer/AddBotButton", "OpenAddBot")
	elif frames == 240:
		var opt := OS.get_environment("GW2_BOT_OPT")
		if opt == "":
			opt = "3"
		var paths := {
			"1": "/OptionsContainer/AspectRatioContainer/AddBotButton",
			"2": "/OptionsContainer/AspectRatioContainer2/AddBotButton",
			"3": "/OptionsContainer/AspectRatioContainer4/AddBotButton",
			"4": "/OptionsContainer2/AspectRatioContainer/AddBotButton",
			"5": "/OptionsContainer2/AspectRatioContainer2/AddBotButton",
			"6": "/OptionsContainer2/AspectRatioContainer4/AddBotButton",
		}
		_press(POP + "/VBoxContainer" + paths.get(opt, paths["3"]), "PickBot" + opt)
	elif frames == 260:
		var n := int(OS.get_environment("GW2_TT_PRESSES"))
		for i in range(n):
			_press(SETUP + "/OptionsContainer/TurnTimeButton", "TurnTime%d" % i)
	elif frames == start_frame:
		_mark("match1_setup_start_pressed")
		_press(SETUP + "/MarginContainer/HBoxContainer/StartButton", "Start")
		in_match = 1
		print("MATCH START 1")
		_mark("match1_start")
	elif frames == 500:
		var all := get_tree().root.find_children("*", "GraphwarFunctionInput", true, false)
		fi = all[0] if all.size() > 0 else null
		print("FI=", fi != null)
	elif exit_at > 0 and frames == exit_at:
		_mark("exit_pressed")
		_screens_dump("before_exit")
		_press(PLAY_EXIT, "PlayExitLongPress")
	elif exit_at > 0 and frames == exit_at + 20:
		_screens_dump("confirm_open")
		_press_text(CONFIRM, "Yes", "ConfirmYes")
	elif exit_at > 0 and frames == exit_at + 60:
		_screens_dump("after_exit")
	elif exit_at > 0 and frames == exit_at + 90:
		_screens_dump("after_exit2")
	elif start2 > 0 and frames == start2:
		_mark("match2_nav")
		_screens_dump("second_start")
		_press("VBoxContainer/ScreensControl/MainMenu/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer2/VBoxContainer2/LocalButton", "Battle2")
	elif start2 > 0 and frames == start2 + 20:
		_screens_dump("second_lanlobby")
		_press("VBoxContainer/ScreensControl/LanLobby/MarginContainer/VBoxContainer/MarginContainer/HBoxContainer/CreateButton", "CreateGame2")
	elif start2 > 0 and frames == start2 + 60:
		_screens_dump("second_setup")
		_press(SETUP + "/OptionsContainer/AddBotButton", "OpenAddBot2")
	elif start2 > 0 and frames == start2 + 90:
		var opt := OS.get_environment("GW2_BOT_OPT")
		if opt == "": opt = "3"
		var paths := {
			"1": "/OptionsContainer/AspectRatioContainer/AddBotButton",
			"2": "/OptionsContainer/AspectRatioContainer2/AddBotButton",
			"3": "/OptionsContainer/AspectRatioContainer4/AddBotButton",
			"4": "/OptionsContainer2/AspectRatioContainer/AddBotButton",
			"5": "/OptionsContainer2/AspectRatioContainer2/AddBotButton",
			"6": "/OptionsContainer2/AspectRatioContainer4/AddBotButton",
		}
		_press(POP + "/VBoxContainer" + paths.get(opt, paths["3"]), "PickBot2")
	elif start2 > 0 and frames == start2 + 110:
		var n := int(OS.get_environment("GW2_TT_PRESSES"))
		for i in range(n):
			_press(SETUP + "/OptionsContainer/TurnTimeButton", "TurnTime2_%d" % i)
	elif start2 > 0 and frames == start2 + 170:
		_screens_dump("second_before_start")
		_press(SETUP + "/MarginContainer/HBoxContainer/StartButton", "Start2")
		in_match = 2
		print("MATCH START 2")
		_mark("match2_start")
	elif verbose and frames > 0 and frames % 300 == 0:
		_screens_dump("tick%d" % frames)
	elif frames >= 3300:
		get_tree().quit()
