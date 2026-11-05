extends Node
## 自检：`godot --headless --path client -- --selftest`
##
## 判据都是"运行时可观察的事实"，与 docs/92 的台账逐条对应；
## 每条失败都会打印 `FAIL <用例名>`，最后打印 `SELFTEST n passed, m failed`，退出码 = 失败数 > 0。
##
## 这个脚本是**验收工具**，不是游戏逻辑的一部分（放在 tools/ 里，不进运行时路径）。

var _checks := 0
var _failures: Array[String] = []
var _main: Control

func run(main: Control) -> int:
	_main = main
	# 让布局至少跑两帧（字号缩放依赖 viewport 尺寸）
	await _frames(3)

	_test_boot()
	await _test_main_menu()
	await _test_campaign()
	await _test_player_screen()
	await _test_replay_lobby()
	await _test_lan_lobby()
	await _test_game_setup()
	await _test_play_screen()
	await _test_endgame()
	_test_fonts_and_icons()

	print("SELFTEST %d passed, %d failed" % [_checks - _failures.size(), _failures.size()])
	if _failures.size() > 0:
		for f in _failures:
			print("  FAIL %s" % f)
		return 1
	return 0

# ---------------------------------------------------------------- 断言

func _frames(n: int) -> void:
	for i in n:
		await get_tree().process_frame

func _expect(cond: bool, what: String) -> bool:
	_checks += 1
	if not cond:
		_failures.append(what)
	return cond

func _screen(name: String) -> Control:
	var app_node := get_node_or_null("/root/App")
	if app_node == null:
		return null
	return app_node.screens.get(name)

func _command(cmd: String) -> void:
	get_node("/root/App").command(cmd)

func _find_button(root: Node, node_name: String) -> GwButton:
	var n := root.find_child(node_name, true, false)
	return n as GwButton

# ---------------------------------------------------------------- 用例

func _test_boot() -> void:
	var expected := [
		"MainMenu", "PlayScreen", "Campaign", "LanLobby", "GameSetup", "PlayerScreen", "ReplayLobby",
		"ChatPopUp", "JoinIpPopUp", "ConnectingPopUp", "DisconnectedPopUp",
		"JoinLevelPopUp", "AddBotPopUp", "EndGamePopUp", "ConfirmPlayExitPopUp",
	]
	var found := 0
	for n in expected:
		if _screen(n) != null:
			found += 1
	_expect(found == 15, "启动：15 个界面全部实例化（实际 %d）" % found)

	var visible_now: Array[String] = []
	for n in expected:
		var s := _screen(n)
		if s != null and s.visible:
			visible_now.append(n)
	_expect(visible_now == ["MainMenu"], "启动：只有 MainMenu 可见（实际 %s）" % str(visible_now))

func _test_main_menu() -> void:
	var menu := _screen("MainMenu")
	_expect(menu != null, "MainMenu 存在")
	if menu == null:
		return
	var specs := {
		"PlayButton": ["Campaign", "campaign"],
		"CharacterButton": ["Player\n", "player_settings"],
		"LocalButton": ["Battle", "multiplayer"],
		"ReplayButton": ["Replays", "replay"],
	}
	for node_name in specs:
		var b := _find_button(menu, node_name)
		_expect(b != null, "MainMenu 有 %s" % node_name)
		if b == null:
			continue
		_expect(b.text == String(specs[node_name][0]),
			"MainMenu/%s 文本 = %s（实际 '%s'）" % [node_name, specs[node_name][0], b.text])
		_expect(b.input_text == String(specs[node_name][1]),
			"MainMenu/%s input_text = %s（实际 %s）" % [node_name, specs[node_name][1], b.input_text])
	# 真的"按"一下 Campaign 按钮（走 GwButton→App.command 的真实路径）
	var play := _find_button(menu, "PlayButton")
	if play != null:
		play.emit_signal("pressed")
		await _frames(2)
	_expect(_screen("Campaign") != null and _screen("Campaign").visible, "点 Campaign → Campaign 可见")

func _test_campaign() -> void:
	var campaign := _screen("Campaign")
	if campaign == null:
		_expect(false, "Campaign 存在")
		return
	var area := campaign.find_child("Control", true, false)
	_expect(area != null, "Campaign 有按钮容器 Control")
	var level_buttons := 0
	for child in area.get_children() if area != null else []:
		if child is GwButton and String(child.name).begins_with("level_"):
			level_buttons += 1
	_expect(level_buttons == 21, "Campaign 生成 21 个关卡按钮（实际 %d）" % level_buttons)
	for i in [0, 7, 20]:
		var b := _find_button(campaign, "level_%d" % i)
		_expect(b != null and b.text == str(i), "Campaign level_%d 的 text = %d" % [i, i])
		_expect(b != null and b.input_text == "level_%d" % i, "Campaign level_%d 的 input_text" % i)

	# 布局算法（docs/101 §4.1）：8 列 4 行、字号 = 宽/15、碰边回弹
	var layout := GwCampaign.layout_buttons(Vector2(1600, 800))
	_expect(layout.size() == 21, "layout_buttons 返回 21 项")
	_expect(is_equal_approx(float(layout[0].font_size), 1600.0 / 15.0), "关卡按钮字号 = 宽/15")
	var ys := {}
	var per_row := {}
	for item in layout:
		var y := snappedf(float(item.row_y), 0.01)
		ys[y] = true
		per_row[y] = int(per_row.get(y, 0)) + 1
	_expect(ys.size() == 3, "关卡按钮排成 3 行（16:9 下真码是 7+7+7，实际 %d 行）" % ys.size())
	_expect(per_row.size() > 0 and int(per_row.values()[0]) == 7,
		"每行 7 个按钮（实际 %s）" % str(per_row.values()))

	# 点 level_0 → JoinLevelPopUp，标题应为 "Level 0"
	var first := _find_button(campaign, "level_0")
	if first != null:
		first.emit_signal("pressed")
		await _frames(2)
	var popup := _screen("JoinLevelPopUp")
	_expect(popup != null and popup.visible, "点 level_0 → JoinLevelPopUp 可见")
	var title := popup.find_child("LevelTitleLabel", true, false) as Label if popup != null else null
	_expect(title != null and title.text == "Level 0", "JoinLevelPopUp 标题 = 'Level 0'（实际 '%s'）"
		% (title.text if title != null else "<null>"))
	var desc := popup.find_child("LevelDescriptionEdit", true, false) as TextEdit if popup != null else null
	_expect(desc != null and desc.text.contains("PLACEHOLDER"),
		"JoinLevelPopUp 说明里明写占位（关卡数据未知）")
	_command("cancel_level")
	await _frames(2)
	_expect(not popup.visible, "cancel_level 关掉 JoinLevelPopUp")

func _test_player_screen() -> void:
	_command("exit_campaign")
	await _frames(2)
	_command("player_settings")
	await _frames(3)
	var screen := _screen("PlayerScreen")
	if screen == null:
		_expect(false, "PlayerScreen 存在")
		return
	_expect(screen.visible, "PlayerScreen 可见")
	var tabs := screen.find_child("TabContainer", true, false) as TabContainer
	_expect(tabs != null, "PlayerScreen 有 TabContainer")
	if tabs != null:
		_expect(tabs.get_tab_count() == 3, "三个页签（实际 %d）" % tabs.get_tab_count())
		_expect(tabs.get_tab_title(0) == " \u2009 Skins  \u2009",
			"页签 0 标题含 U+2009（实际 %s）" % tabs.get_tab_title(0))
	var skins_vbox := screen.find_child("SkinsVbox", true, false)
	var faces_vbox := screen.find_child("FacesVbox", true, false)
	var hats_vbox := screen.find_child("HatsVbox", true, false)
	_expect(_count_buttons(skins_vbox) == GwStore.SKIN_COUNT,
		"皮肤按钮 15 个（实际 %d）" % _count_buttons(skins_vbox))
	_expect(_count_entries(faces_vbox) == GwStore.FACE_COUNT,
		"脸格子 15 个（实际 %d）" % _count_entries(faces_vbox))
	_expect(_count_entries(hats_vbox) == GwStore.HAT_COUNT,
		"帽子格子 27 个（实际 %d）" % _count_entries(hats_vbox))
	# 观测（docs/104 §5.2）：PlayerScreen 运行时比静态多 141 个 GraphwarButton
	# = 皮肤 15 + Faces 15×3 + Hats 27×3；再加静态的 3 个（返回对 2 + Unlock 1）= 144。
	var total_buttons := _count_buttons(screen)
	var grid_buttons := _count_buttons(skins_vbox) + _count_buttons(faces_vbox) + _count_buttons(hats_vbox)
	_expect(grid_buttons == 141, "皮肤/脸/帽子网格共 141 个按钮（观测值，实际 %d）" % grid_buttons)
	_expect(total_buttons == 144, "PlayerScreen 共 144 个按钮（= 141 动态 + 3 静态，实际 %d）" % total_buttons)

	var name_edit := screen.find_child("PlayerNameLineEdit", true, false) as LineEdit
	_expect(name_edit != null, "PlayerScreen 有改名输入框")
	if name_edit != null:
		name_edit.text = "selftest-user"
		name_edit.emit_signal("text_changed", "selftest-user")
		await _frames(1)
		_expect(String(get_node("/root/App").store.profile.name) == "selftest-user", "改名写进存档")

func _test_replay_lobby() -> void:
	_command("exit_player_settings")
	await _frames(2)
	_command("replay")
	await _frames(3)
	var screen := _screen("ReplayLobby")
	_expect(screen != null and screen.visible, "ReplayLobby 可见")
	_expect(screen.find_child("ReplaysContainer", true, false) != null, "ReplayLobby 有 ReplaysContainer")

func _test_lan_lobby() -> void:
	_command("exit_replay_lobby")
	await _frames(2)
	_command("multiplayer")
	await _frames(3)
	var screen := _screen("LanLobby")
	if screen == null:
		_expect(false, "LanLobby 存在")
		return
	_expect(screen.visible, "LanLobby 可见")
	_command("show_global")
	await _frames(2)
	var list := screen.find_child("GraphwarLanVBox", true, false)
	_expect(list != null, "LanLobby 有 GraphwarLanVBox")
	var rooms := 0
	for c in list.get_children():
		if c is HBoxContainer:
			rooms += 1
	_expect(rooms == 5, "公网房间列表 5 条（观测值，实际 %d）" % rooms)
	_command("show_local")
	await _frames(2)
	var local_rooms := 0
	for c in list.get_children():
		if c is HBoxContainer:
			local_rooms += 1
	_expect(local_rooms == 0, "本地房间列表初始为空（实际 %d）" % local_rooms)

func _test_game_setup() -> void:
	_command("create_game")
	await _frames(3)
	var screen := _screen("GameSetup")
	if screen == null:
		_expect(false, "GameSetup 存在")
		return
	_expect(screen.visible, "GameSetup 可见")
	var app_node := get_node("/root/App")
	_expect(app_node.state.players.size() == 1, "建房后玩家表 1 人（实际 %d）" % app_node.state.players.size())

	# 四个选项按钮的循环（数值来自 docs/92 §6 的连点枚举）
	var t0: int = app_node.state.time_mode
	_expect(t0 == GwModes.TIME_DEFAULT, "时间档初值 = 1m（实际 index %d）" % t0)
	_command("")  # no-op，确认空命令不炸
	var time_button := _find_button(screen, "TurnTimeButton")
	if time_button != null:
		time_button.emit_signal("pressed")
		time_button.emit_signal("pressed")
		await _frames(1)
	_expect(app_node.state.time_mode == GwModes.TimeLimit.T3M,
		"时间档按 2 次到 3 分钟档（docs/92 §6，实际 index %d）" % app_node.state.time_mode)
	_expect(GwModes.turn_seconds(app_node.state.time_mode) == 180, "3 分钟档 = 180 秒")
	_expect(GwModes.format_clock(180.0) == "03:00", "时钟格式 MM:SS")
	var axis_button := _find_button(screen, "AxisModeButton")
	var axis_start: int = app_node.state.axis_mode
	if axis_button != null:
		for i in 4:
			axis_button.emit_signal("pressed")
		await _frames(1)
	_expect(app_node.state.axis_mode == axis_start, "Axis 4 档循环回到初值（docs/92 §6）")

	# 没有对手时 Start 不开始（原版静默；我们额外给提示）
	_command("start_game")
	await _frames(2)
	_expect(not _screen("PlayScreen").visible, "无 Bot 时 Start 不开始（原版行为）")

	# 加 Bot → 六档
	_command("add_bot")
	await _frames(2)
	var bot_popup := _screen("AddBotPopUp")
	_expect(bot_popup != null and bot_popup.visible, "add_bot → AddBotPopUp 可见")
	var tier_buttons := 0
	for t in 6:
		if _find_button(bot_popup, "AddBotButton") != null:
			tier_buttons += 1
			break
	for t in 6:
		var any_of_tier := false
		for b in bot_popup.find_children("*", "GwButton", true, false):
			if String((b as GwButton).input_text) == "add_bot_%d" % t:
				any_of_tier = true
		_expect(any_of_tier, "AddBotPopUp 有 add_bot_%d 按钮" % t)
	var tier2 = null
	for b in bot_popup.find_children("*", "GwButton", true, false):
		if String((b as GwButton).input_text) == "add_bot_2":
			tier2 = b
			break
	if tier2 != null:
		tier2.emit_signal("pressed")
		await _frames(2)
	_expect(app_node.state.players.size() == 2, "加 Bot 后玩家表 2 人（实际 %d）" % app_node.state.players.size())
	_expect(not bot_popup.visible, "加完 Bot 关掉 AddBotPopUp")
	_expect(app_node.state.can_start(), "有对手后 can_start() 为真")

	# JoinIpPopUp：地址必须带端口（docs/92 §4.7）
	_command("join_ip")
	await _frames(2)
	var join_popup := _screen("JoinIpPopUp")
	_expect(join_popup != null and join_popup.visible, "join_ip → JoinIpPopUp 可见")
	join_popup.call("current_text")
	_command("join_join_ip")
	await _frames(2)
	_expect(not _screen("ConnectingPopUp").visible, "空地址不会进 ConnectingPopUp")

func _test_play_screen() -> void:
	_command("start_game")
	await _frames(3)
	var play := _screen("PlayScreen")
	if play == null:
		_expect(false, "PlayScreen 存在")
		return
	_expect(play.visible, "PlayScreen 可见")

	# 键盘：18 + 18 个键
	var left_keys := 0
	var right_keys := 0
	for b in play.find_children("*", "GwButton", true, false):
		var grid := (b as GwButton).get_parent()
		if grid != null and String(grid.name) == "LeftButtonGrid":
			left_keys += 1
		elif grid != null and String(grid.name) == "RightButtonGrid":
			right_keys += 1
	_expect(left_keys == 18, "左键盘 18 键（实际 %d）" % left_keys)
	_expect(right_keys == 18, "右键盘 18 键（实际 %d）" % right_keys)
	var y_button := _find_button(play, "YButton")
	_expect(y_button != null and y_button.disabled, "YButton(y) 恒为 disabled（docs/92 §4.10 观测）")

	# 用键盘敲 sin(x)
	for cmd in ["sin(", "x", ")"]:
		_command(cmd)
		await _frames(1)
	var input := play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	_expect(input != null, "有 GraphwarFunctionInput")
	_expect(input != null and input.text == "sin(x)", "键盘敲出 'sin(x)'（实际 '%s'）"
		% (input.text if input != null else "<null>"))

	# 秒表：3 分钟档 ⇒ 开局读到 02:xx（**要在开火之前查**，因为 1v1 一炮就可能打完）
	var timer_label := play.find_child("TimerLabel", true, false) as Label
	_expect(timer_label != null, "有 TimerLabel")
	_expect(timer_label != null and String(timer_label.text).begins_with("02:"),
		"3 分钟档的时刻是 02:xx（实际 %s）" % (timer_label.text if timer_label != null else "<null>"))

	# 开火
	var fire_button := _find_button(play, "FireButton")
	_expect(fire_button != null, "有 FireButton")
	_expect(fire_button != null and fire_button.long_press_input_text == "fire",
		"FireButton 的 long_press_input_text = fire（原版契约）")
	if fire_button != null:
		fire_button.emit_signal("pressed")
		await _frames(2)
	var plane := play.find_child("GraphwarPlane", true, false) as GwPlaneView
	_expect(plane != null, "有 GraphwarPlane")
	_expect(plane != null and plane.path_points.size() > 1,
		"开火后弹道点数 > 1（实际 %d）" % (plane.path_points.size() if plane != null else -1))
	var status := play.find_child("StatusLabel", true, false) as Label
	print("DEBUG status dim=%s" % (status.text if status != null else "<null>"))
	var offset_label := play.find_child("OffsetLabel", true, false) as Label
	_expect(offset_label != null and offset_label.text.strip_edges() != "",
		"修正常数已显示（实际 '%s'）" % (offset_label.text if offset_label != null else "<null>"))

	# 显示开关
	var before := plane.show_names
	_command("toggle_names")
	await _frames(1)
	_expect(plane.show_names != before, "toggle_names 生效")
	_command("toggle_names")

	# 退出确认链
	_command("ask_exit_play")
	await _frames(2)
	_expect(_screen("ConfirmPlayExitPopUp").visible, "ask_exit_play → ConfirmPlayExitPopUp 可见")
	_command("cancel_exit_play")
	await _frames(2)
	_expect(not _screen("ConfirmPlayExitPopUp").visible, "cancel_exit_play 关掉确认框")

func _test_endgame() -> void:
	var app_node := get_node("/root/App")
	app_node.report_match_result(false, 1)   # 玩家输、Team 2 赢（结算框据此取文案）
	await _frames(3)
	var popup := _screen("EndGamePopUp")
	_expect(popup != null and popup.visible, "report_match_result → EndGamePopUp 可见")
	if popup != null:
		var self_label := popup.find_child("SelfVictoryLabel", true, false) as Label
		_expect(self_label != null and self_label.text == "YOU lOST",
			"结算文案照抄原版 'YOU lOST'（实际 '%s'）" % (self_label.text if self_label != null else "<null>"))
		var team_label := popup.find_child("TeamVictoryLabel", true, false) as Label
		_expect(team_label != null and String(team_label.text).contains("Victory"),
			"有胜方文案（实际 '%s'）" % (team_label.text if team_label != null else "<null>"))
		var slots := 0
		for c in popup.find_children("VictoryPlayerContainer*", "", true, false):
			slots += 1
		_expect(slots == 10, "结算有 10 个展示槽（实际 %d）" % slots)
	_command("exit_play")
	await _frames(2)
	_expect(_screen("MainMenu").visible, "exit_play 回到 MainMenu")

func _test_fonts_and_icons() -> void:
	var font := GwThemeFactory.font()
	_expect(font != null, "字体资源存在（SystemFont，不打包字体文件）")
	var icon := GwIcons.get_icon("return")
	_expect(icon != null and icon.get_width() > 0, "图标生成成功（现画的 SVG）")
	var icon2 := GwIcons.get_icon("bot_5")
	_expect(icon2 != null and icon2.get_width() > 0, "bot_5 图标生成成功")

func _count_buttons(node: Node) -> int:
	if node == null:
		return 0
	var n := 0
	for b in node.find_children("*", "GwButton", true, false):
		n += 1
	return n

func _count_entries(node: Node) -> int:
	if node == null:
		return 0
	var n := 0
	for c in node.find_children("ShopButtonEntry*", "", true, false):
		n += 1
	return n
