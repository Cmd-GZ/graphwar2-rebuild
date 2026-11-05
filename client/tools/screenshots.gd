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
		if String(t.kind) == "screen":
			app_node.show_screen(name)
		else:
			app_node.show_screen(String(t.after))
			app_node.open_popup(name)
		await _frames(4)
		# 战场是自绘的：改尺寸后强刷一次再截，否则会截到旧尺寸那一帧
		if name == "PlayScreen":
			var plane = app_node.screens["PlayScreen"].find_child("GraphwarPlane", true, false)
			if plane != null:
				plane.queue_redraw()
				await _frames(2)
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
	elif screen == "GameSetup":
		app_node.state.reset_lan()
		app_node.state.add_human(app_node.store.profile.name, 0, 0, 0)
		app_node.state.add_bot(2)
		app_node.state.add_bot(4)
		app_node.show_screen("GameSetup")

func _frames(n: int) -> void:
	for i in n:
		await get_tree().process_frame
