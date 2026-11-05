extends Node
# Runtime UI inventory dumper for Graphwar II.
#
# Why this exists: the interface was never enumerated. Decompiling 105 .gdc is not
# needed to learn *what widgets exist* — the shipped game builds the whole node tree
# at boot, so we can just walk it in the live harness and print it.
#
# Driven by `tools/dump_ui.sh`, which installs this file as godot-live/probe.gd for
# the duration of one run. Route selection: env GW2_UI_ROUTE (see ROUTES below).
#
# Output is line oriented and machine parsed (tools/dump_ui.py):
#   @@ROUTE|<route>|<nsteps>
#   @@DUMP|<label>|frame=<f>          start of one snapshot
#   @@SCR|<label>|<name>|<class>|<visible>     direct children of ScreensControl
#   @@N|<label>|<path>|<class>|<vis>|<vistree>|<script>|<text>|<flags>
#   @@C|<label>|<path>|<signal>|<method>      signal connections
#   @@END|<label>
# Text has newlines escaped as \n and '|' replaced by '¦'.

const ROOTNAME := "GraphwarRootNode"
const MENU := "VBoxContainer/ScreensControl/MainMenu/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer"
const MENU2 := "VBoxContainer/ScreensControl/MainMenu/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer2/VBoxContainer2"
const LOBBY := "VBoxContainer/ScreensControl/LanLobby/MarginContainer/VBoxContainer"
const SETUP := "VBoxContainer/ScreensControl/GameSetup/GraphwarSetupControl/MarginContainer/VBoxContainer"
const PLAY := "VBoxContainer/ScreensControl/PlayScreen/MarginContainer/VBoxContainer/PlayMainHBox"
const POPBOT := "VBoxContainer/ScreensControl/AddBotPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer"

# Frames are 1/60 s by default; 30 frames ≈ 0.5 s, enough for popups to settle.
var ROUTES := {
	# Every bot difficulty at once: shows the generated player-list rows.
	"setup_bots": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer/AddBotButton"],
		[200, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[230, "press", POPBOT + "/OptionsContainer/AspectRatioContainer2/AddBotButton"],
		[260, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[290, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[320, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[350, "press", POPBOT + "/OptionsContainer2/AspectRatioContainer/AddBotButton"],
		[380, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[410, "press", POPBOT + "/OptionsContainer2/AspectRatioContainer2/AddBotButton"],
		[440, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[470, "press", POPBOT + "/OptionsContainer2/AspectRatioContainer4/AddBotButton"],
		[540, "dump", "setup_six_bots"],
		[550, "quit"],
	],
	# DiffEq match: the function keyboard and the plane differ from Normal.
	"play_diffeq": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/FunctionModeButton"],
		[160, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[200, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[240, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[270, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[420, "dump", "play_diffeq_t0"],
		[600, "dump", "play_diffeq_t3s"],
		[620, "quit"],
	],
	# Extra option pads in a live match (view functions / show names / mute / chat).
	"play_extra": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[240, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[360, "press", PLAY + "/LeftControls/LeftTopButtonsContainer/MarginContainer/ShowExtraButtons"],
		[430, "dump", "play_extra_buttons"],
		[470, "press", PLAY + "/PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox/ShowNames"],
		[520, "dump", "play_names_toggled"],
		[530, "quit"],
	],
	# boot: nothing pressed. Every screen is instantiated at startup, so this
	# captures the complete static tree of all 15 screens in one shot.
	"boot": [
		[60, "dump", "boot_all_screens"],
		[70, "quit"],
	],
	"campaign": [
		[50, "press", MENU + "/VBoxContainer/PlayButton"],
		[100, "dump", "campaign"],
		[110, "press", "VBoxContainer/ScreensControl/Campaign/MarginContainer/VBoxContainer/Control/level_0"],
		[160, "dump", "campaign_after_level0"],
		[170, "quit"],
	],
	"player": [
		[50, "press", MENU + "/VBoxContainer/CharacterButton"],
		[110, "dump", "player_screen"],
		[120, "quit"],
	],
	"replay": [
		[50, "press", MENU2 + "/ReplayButton"],
		[110, "dump", "replay_lobby"],
		[120, "quit"],
	],
	"lobby": [
		[50, "press", MENU2 + "/LocalButton"],
		[100, "dump", "lan_lobby_local"],
		[110, "press", LOBBY + "/HBoxContainer/ShowGlobalContainer/ShowGlobalButton"],
		[160, "dump", "lan_lobby_global"],
		[170, "quit"],
	],
	"setup": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[150, "dump", "game_setup"],
		[160, "quit"],
	],
	"addbot": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[190, "dump", "add_bot_popup"],
		[200, "quit"],
	],
	"joinip": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/JoinButton"],
		[150, "dump", "join_ip_popup"],
		[160, "quit"],
	],
	# Full match: lobby -> create -> add bot -> short turn time -> start.
	"play": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[240, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[270, "press", SETUP + "/OptionsContainer/TurnModeButton"],
		[300, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[380, "dump", "play_screen_t0"],
		[560, "dump", "play_screen_t3s"],
		[760, "dump", "play_screen_t6s"],
		[780, "quit"],
	],
	"play_chat": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[240, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[270, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[400, "press", PLAY + "/PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox/ChatButtonPlay"],
		[470, "dump", "play_chat_popup"],
		[480, "quit"],
	],
	"play_exit": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[330, "press", PLAY + "/LeftControls/LeftTopButtonsContainer/ExitButtonContainer/ClipButton"],
		[400, "dump", "confirm_exit_after_clip"],
		[430, "press", PLAY + "/LeftControls/LeftTopButtonsContainer/ExitButtonContainer/LongPressButton"],
		[510, "dump", "confirm_exit_after_longpress"],
		[520, "quit"],
	],
	# Enumerate every state of the four GameSetup option buttons.
	"setup_cycle": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "dump", "setup_initial"],
		[140, "cycle", SETUP + "/OptionsContainer/AxisModeButton", 6],
		[300, "cycle", SETUP + "/OptionsContainer/FunctionModeButton", 6],
		[460, "cycle", SETUP + "/OptionsContainer/TurnModeButton", 6],
		[620, "cycle", SETUP + "/OptionsContainer/TurnTimeButton", 8],
		[820, "dump", "setup_after_cycle"],
		[830, "quit"],
	],
	# Connection refused (closed port on loopback): should reach DisconnectedPopUp fast.
	"connect_refused": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/JoinButton"],
		[140, "settext", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer/MarginContainer2/JoinIpIpLineEdit", "127.0.0.1:1"],
		[170, "press", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer3/MarginContainer2/GraphwarButton"],
		[200, "dump", "after_join_click"],
		[220, "watchmany", [["DisconnectedPopUp", "disconnected", "45000"], ["ConnectingPopUp", "connecting", "15000"]]],
	],
	# Level-6 bot (Deep Thought, 1000 generations) so the match ends quickly.
	"endgame6": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer2/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[240, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[380, "dump", "match_start"],
		[400, "watchmany", [["EndGamePopUp", "endgame_popup", "420000"], ["ConfirmPlayExitPopUp", "confirm_exit_late", "10000"]]],
	],
	# Join a public ("Global") room from the room list: exercises the connect path.
	"connect_room": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/HBoxContainer/ShowGlobalContainer/ShowGlobalButton"],
		[200, "press", LOBBY + "/MarginContainer2/VBoxContainer/ScrollContainer/GraphwarLanVBox/HBoxContainer/JoinButtonContainer/JoinButton"],
		[220, "dump", "after_room_join"],
		[240, "watchmany", [["ConnectingPopUp", "connecting", "20000"], ["DisconnectedPopUp", "disconnected", "90000"]]],
	],
	# Same, but the IP popup gets an explicit host:port.
	"connect_ip_port": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/JoinButton"],
		[140, "settext", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer/MarginContainer2/JoinIpIpLineEdit", "10.255.255.1:12345"],
		[170, "press", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer3/MarginContainer2/GraphwarButton"],
		[200, "dump", "after_join_click"],
		[220, "watchmany", [["ConnectingPopUp", "connecting", "20000"], ["DisconnectedPopUp", "disconnected", "90000"]]],
	],
	# Try to provoke ConnectingPopUp / DisconnectedPopUp with an unreachable peer.
	"connect_fail": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/JoinButton"],
		[140, "settext", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer/MarginContainer2/JoinIpIpLineEdit", "10.255.255.1"],
		[170, "press", "VBoxContainer/ScreensControl/JoinIpPopUp/MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer/HBoxContainer3/MarginContainer2/GraphwarButton"],
		[220, "dump", "after_join_click"],
		[400, "watch", "ConnectingPopUp", "connecting", "120000"],
	],
	# Long match: watch for EndGamePopUp on the wall clock (game timers are real time).
	"endgame": [
		[50, "press", MENU2 + "/LocalButton"],
		[90, "press", LOBBY + "/MarginContainer/HBoxContainer/CreateButton"],
		[130, "press", SETUP + "/OptionsContainer/AddBotButton"],
		[170, "press", POPBOT + "/OptionsContainer/AspectRatioContainer4/AddBotButton"],
		[210, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[240, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[270, "press", SETUP + "/OptionsContainer/TurnTimeButton"],
		[300, "press", SETUP + "/MarginContainer/HBoxContainer/StartButton"],
		[400, "dump", "match_start"],
		[420, "watch", "EndGamePopUp", "endgame_popup", "300000"],
	],
}

var frames := 0
var queue: Array = []
var root: Node = null
var route := "boot"
var steps: Array = []
var si := 0
var last_frame := 0


func _ready() -> void:
	var r := OS.get_environment("GW2_UI_ROUTE")
	if r != "":
		route = r
	if not ROUTES.has(route):
		print("@@ERROR|unknown route ", route)
		route = "boot"
	steps = ROUTES[route]
	last_frame = int(steps[steps.size() - 1][0])
	print("@@ROUTE|", route, "|", steps.size())


func _root() -> Node:
	if root == null:
		root = get_tree().root.get_node_or_null(ROOTNAME)
	return root


func _find(screens: Node, rel: String) -> Node:
	# Absolute path from GraphwarRootNode.
	var n := _root().get_node_or_null(rel) if _root() else null
	if n:
		return n
	# Fallback: unique name anywhere under the root (paths shift between builds).
	var hits := _root().find_children(rel.get_file(), "", true, false) if _root() else []
	print("@@FALLBACK|", rel, "|hits=", hits.size())
	for h in hits:
		if h is CanvasItem and (h as CanvasItem).is_visible_in_tree():
			return h
	return hits[0] if hits.size() > 0 else null


func _esc(s: String) -> String:
	return s.replace("\n", "\\n").replace("\r", "").replace("|", "¦")


func _text_of(n: Node) -> String:
	var t = n.get("text")
	var out := ""
	if t != null:
		out = str(t)
	if n is RichTextLabel:
		out = (n as RichTextLabel).text
	if out == "" and n is LineEdit:
		var ph := (n as LineEdit).placeholder_text
		if ph != "":
			out = "‹placeholder:" + ph + "›"
	return _esc(out)


func _flags_of(n: Node) -> String:
	var f: Array[String] = []
	if n is BaseButton:
		var b := n as BaseButton
		if b.disabled:
			f.append("disabled")
		if b.toggle_mode:
			f.append("toggle")
		if b.button_pressed:
			f.append("checked")
	if n is LineEdit:
		var le := n as LineEdit
		if not le.editable:
			f.append("readonly")
		if le.secret:
			f.append("secret")
	if n is OptionButton:
		var ob := n as OptionButton
		var items: Array[String] = []
		for i in ob.item_count:
			items.append(ob.get_item_text(i))
		f.append("items=" + _esc(";".join(items)))
	if n is Control and (n as Control).custom_minimum_size != Vector2.ZERO:
		f.append("minsize=" + str((n as Control).custom_minimum_size))
	# Icon-bearing widgets: the actual label of an option button lives in its texture.
	for prop in ["texture", "texture_normal", "icon", "texture_pressed"]:
		var v = n.get(prop)
		if v is Texture2D:
			var rp := (v as Texture2D).resource_path
			if rp != "":
				f.append(prop + "=" + rp)
				break
	# scene_file_path is set on the root of every instanced sub-scene: this maps each
	# .tscn we extracted to the exact node it was instantiated at.
	if n.scene_file_path != "":
		f.append("scene=" + n.scene_file_path)
	var tt = n.get("tooltip_text")
	if tt is String and String(tt) != "":
		f.append("tip=" + _esc(String(tt)))
	return ";".join(f)


func _script_of(n: Node) -> String:
	var s = n.get_script()
	if s == null:
		return "-"
	if s is Script:
		var p := (s as Script).resource_path
		return p if p != "" else "<script>"
	return "<script>"


func _conns_of(n: Node) -> Array:
	var out := []
	for sig in n.get_signal_list():
		var name := String(sig["name"])
		# Skip the noise: engine layout/notification signals are not UI behaviour.
		if name in ["resized", "size_changed", "sort_children", "draw", "renamed",
				"tree_entered", "tree_exiting", "tree_exited", "ready", "predelete",
				"script_changed", "child_order_changed", "visibility_changed",
				"item_rect_changed", "minimum_size_changed", "focus_entered_pending"]:
			continue
		for c in n.get_signal_connection_list(name):
			var cb: Callable = c["callable"]
			out.append([name, cb.get_method()])
	return out


func _walk(n: Node, path: String, label: String) -> void:
	var vis := "?"
	if n is CanvasItem:
		var ci := n as CanvasItem
		vis = ("1" if ci.visible else "0") + ("1" if ci.is_visible_in_tree() else "0")
	print("@@N|", label, "|", path, "|", n.get_class(), "|", vis, "|",
		_script_of(n), "|", _text_of(n), "|", _flags_of(n))
	for c in _conns_of(n):
		print("@@C|", label, "|", path, "|", c[0], "|", c[1])
	for ch in n.get_children():
		_walk(ch, path + "/" + String(ch.name), label)


func _dump(label: String) -> void:
	var r := _root()
	print("@@DUMP|", label, "|frame=", frames)
	if r == null:
		print("@@ERROR|root_missing")
		return
	var sc := r.get_node_or_null("VBoxContainer/ScreensControl")
	if sc:
		for c in sc.get_children():
			var v := "?"
			if c is CanvasItem:
				v = "1" if (c as CanvasItem).visible else "0"
			print("@@SCR|", label, "|", String(c.name), "|", c.get_class(), "|", v)
	_walk(r, String(r.name), label)
	print("@@END|", label)


var watch_name := ""
var watch_label := ""
var watch_deadline := 0
var watch_dumped := false
var watch_queue: Array = []   # [[screen,label,budget_ms], ...] for "watchmany"

func _advance_watch() -> void:
	if watch_queue.is_empty():
		print("@@WATCHDONE")
		get_tree().quit()
		return
	var item: Array = watch_queue.pop_front()
	watch_name = String(item[0])
	watch_label = String(item[1])
	watch_deadline = Time.get_ticks_msec() + int(item[2])
	watch_dumped = false
	print("@@WATCH|", watch_name, "|", watch_label, "|budget_ms=", item[2])


func _do(st: Array) -> void:
	var kind := String(st[1])
	match kind:
		"dump":
			_dump(String(st[2]))
		"press":
			var n := _find(null, String(st[2]))
			if n == null:
				print("@@MISS|press|", st[2])
			else:
				n.emit_signal("pressed")
				print("@@PRESS|", st[2], "|ok=", n.get_class())
		"settext":
			var rel := String(st[2])
			var val := String(st[3])
			var target: Node = null
			if rel.ends_with("::lineedit::"):
				# Fill the first LineEdit inside the popup at rel.
				var base := _root().get_node_or_null(rel.substr(0, rel.length() - 12))
				if base:
					var les := base.find_children("*", "LineEdit", true, false)
					if les.size() > 0:
						target = les[0]
			else:
				target = _find(null, rel)
			if target == null:
				print("@@MISS|settext|", rel)
			else:
				target.set("text", val)
				if target.has_signal("text_changed"):
					target.emit_signal("text_changed", val)
				print("@@SETTEXT|", rel, "|", val)
		"watchmany":
			watch_queue = []
			for item in st[2]:
				watch_queue.append(item)
			_advance_watch()
		"watch":
			watch_name = String(st[2])
			watch_label = String(st[3])
			watch_deadline = Time.get_ticks_msec() + int(st[4])
			print("@@WATCH|", watch_name, "|", watch_label, "|budget_ms=", st[4])
		"quit":
			print("@@QUIT|frame=", frames)
			get_tree().quit()


func _process(_dt: float) -> void:
	frames += 1
	while si < steps.size() and frames >= int(steps[si][0]):
		var st: Array = steps[si]
		si += 1
		if String(st[1]) == "cycle":
			# Enumerate the states of one option button: press it `count` times and
			# record what the widget shows after every press.
			var rel := String(st[2])
			var count := int(st[3])
			for i in count:
				queue.append([frames + 20 + i * 20, "cyclestep", rel, str(i)])
		elif String(st[1]) == "cyclepair":
			# Two buttons alternating (e.g. ShowLocal / ShowGlobal).
			var rel2 := String(st[2])
			var count2 := int(st[3])
			for i in count2:
				queue.append([frames + 20 + i * 20, "press", rel2 + ("" if i % 2 == 1 else ""), str(i)])
		else:
			_do(st)
	while queue.size() > 0 and frames >= int(queue[0][0]):
		var q: Array = queue.pop_front()
		if String(q[1]) == "cyclestep":
			var node := _find(null, String(q[2]))
			if node == null:
				print("@@MISS|cycle|", q[2])
				continue
			node.emit_signal("pressed")
			print("@@CYCLE|", q[2], "|", q[3], "|text=", _text_of(node), "|", _flags_of(node))
		else:
			_do(q)
	if watch_name != "" and not watch_dumped:
		var sc := _root().get_node_or_null("VBoxContainer/ScreensControl") if _root() else null
		var target := sc.get_node_or_null(watch_name) if sc else null
		if target and (target as CanvasItem).visible:
			watch_dumped = true
			print("@@WATCHHIT|", watch_name, "|ms=", Time.get_ticks_msec())
			_dump(watch_label)
			_advance_watch()
		elif Time.get_ticks_msec() > watch_deadline:
			watch_dumped = true
			print("@@WATCHTIMEOUT|", watch_name, "|ms=", Time.get_ticks_msec())
			_dump(watch_label + "_timeout")
			_advance_watch()
	elif frames > last_frame + 30:
		get_tree().quit()
