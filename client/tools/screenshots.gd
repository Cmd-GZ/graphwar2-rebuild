extends Node
## 逐界面截图工具：`bash client/tools/shots.sh [输出目录]`
##
## 用途：肉眼核对布局（字号缩放、按钮尺寸、战场比例）。**只用于开发/验收**，不参与运行时逻辑。
## 输出文件名 = 界面名。headless（dummy 渲染）拿不到画面，必须给真实渲染后端（Xvfb + OpenGL）。

const TARGETS := [
	{"kind": "screen", "name": "MainMenu"},
	{"kind": "screen", "name": "Campaign"},
	{"kind": "popup", "name": "JoinLevelPopUp", "after": "Campaign"},
	{"kind": "screen", "name": "LanLobby"},
	{"kind": "screen", "name": "GameSetup", "after": "GameSetup"},
	{"kind": "popup", "name": "AddBotPopUp", "after": "GameSetup"},
	{"kind": "screen", "name": "PlayerScreen"},
	{"kind": "screen", "name": "ReplayLobby"},
	{"kind": "screen", "name": "PlayScreen", "after": "PlayScreen"},
	# 战役第 0 关：**验收基准**（原版真帧 `orig_campaign_play.png` 就是这个状态），
	# 走 App 的真实入口 `_start_level`，数据来自 GwCampaign（docs/118 §7）。
	{"kind": "screen", "name": "PlayScreenCampaign", "after": "PlayScreenCampaign",
		"target": "PlayScreen"},
	{"kind": "screen", "name": "PlayScreenOffset", "after": "PlayScreenOffset",
		"target": "PlayScreen"},
	{"kind": "popup", "name": "ConfirmPlayExitPopUp", "after": "PlayScreen"},
	{"kind": "popup", "name": "ChatPopUp", "after": "PlayScreen"},
	# 补上剩下的 4 个弹窗，让 15 个界面每一个都有一张 1920×1160 的实渲染截图（docs/124 §4）。
	{"kind": "popup", "name": "JoinIpPopUp", "after": "LanLobby"},
	{"kind": "popup", "name": "ConnectingPopUp", "after": "LanLobby"},
	{"kind": "popup", "name": "DisconnectedPopUp", "after": "LanLobby"},
	{"kind": "popup", "name": "EndGamePopUp", "after": "PlayScreen"},
]

func run(_main: Control, out_dir: String) -> int:
	var app_node := get_node_or_null("/root/App")
	if app_node == null:
		return 2
	await _frames(3)
	for t in TARGETS:
		app_node.close_all_popups()
		var name := String(t.name)
		var after := String(t.get("after", ""))
		if after != "":
			_prepare(app_node, after)
		var target := String(t.get("target", name))
		if String(t.kind) == "screen":
			app_node.show_screen(target)
		else:
			app_node.show_screen(String(t.after))
			app_node.open_popup(name)
		await _frames(4)
		# 战场是自绘的：改尺寸后强刷一次再截，否则会截到旧尺寸那一帧
		if target == "PlayScreen":
			var plane = app_node.screens["PlayScreen"].find_child("GraphwarPlane", true, false)
			if plane != null:
				plane.queue_redraw()
				await _frames(2)
		if target == "PlayScreen" or name == "JoinLevelPopUp":
			_print_geometry(app_node, name)
		await RenderingServer.frame_post_draw
		var img := get_viewport().get_texture().get_image()
		var path := out_dir.path_join(name + ".png")
		var err := img.save_png(path)
		print("shot %s -> %s (%d)" % [name, path, err])
	return 0

## 把状态铺到"看得出内容"的样子（空表截出来没意义）。
func _prepare(app_node: Node, screen: String) -> void:
	if screen == "PlayScreen":
		app_node.state.reset_lan()
		app_node.state.add_human(app_node.store.profile.name, 0, 0, 0)
		app_node.state.add_bot(2)
		app_node.show_screen("PlayScreen")
		var play: Control = app_node.screens["PlayScreen"]
		if play.has_method("begin_match"):
			play.call("begin_match")
		if play.has_method("key_input"):
			for cmd in ["sin(", "x", ")"]:
				play.call("key_input", cmd)
	elif screen == "PlayScreenCampaign":
		# 战役第 0 关：1 士兵 / 1 靶子 / 无对手 / 无回合时限（docs/118 §7.1）
		app_node.campaign_selected_level = 0
		app_node.call("_start_level")
	elif screen == "PlayScreenOffset":
		# 打一发，展示右下角的修正常数（`+ 4.638` 形状，见 docs/24 §6.2）
		app_node.campaign_selected_level = 0
		app_node.call("_start_level")
		var play: Control = app_node.screens["PlayScreen"]
		for cmd in ["-", "1", "0", "*", "x", "+", "5"]:
			play.call("key_input", cmd)
		play.call("fire")
		# 弹道现在按 tick 逐点揭示（`docs/135` §3）：截图是"一张定格"，所以把这一发
		# 直接推到飞完，否则截到的是动画第 1 帧（几乎没有弹道），基准图就没法比了。
		var fx = play.find_child("FireFx", true, false)
		if fx != null:
			fx.advance_seconds(600.0)
	elif screen == "GameSetup":
		app_node.state.reset_lan()
		app_node.state.add_human(app_node.store.profile.name, 0, 0, 0)
		app_node.state.add_bot(2)
		app_node.state.add_bot(4)
		app_node.show_screen("GameSetup")

## 打印控件几何：`GEOM|snapshot|class|path|visible|x|y|w|h`。
## 用来和原版真帧的同一批节点逐项比（docs/125 §4 的数值表就是这么生成的）。
func _print_geometry(app_node: Node, snapshot: String) -> void:
	var screen: Node = null
	if snapshot == "JoinLevelPopUp":
		screen = app_node.screens.get("JoinLevelPopUp")
	if screen == null:
		screen = app_node.screens.get("PlayScreen")
	if screen == null:
		return
	var stack: Array = [screen]
	while not stack.is_empty():
		var n: Node = stack.pop_back()
		if n is Control:
			var c := n as Control
			print("GEOM|%s|%s|%s|%d|%.1f|%.1f|%.1f|%.1f" % [snapshot, c.get_class(),
				String(c.get_path()), 1 if c.visible else 0,
				c.global_position.x, c.global_position.y, c.size.x, c.size.y])
		for ch in n.get_children():
			stack.append(ch)

func _frames(n: int) -> void:
	for i in n:
		await get_tree().process_frame
