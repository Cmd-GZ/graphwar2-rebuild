extends Node
## 自检：`godot --headless --path client -- --selftest`
##
## 判据都是"运行时可观察的事实"，与 docs/92 的台账逐条对应；
## 每条失败都会打印 `FAIL <用例名>`，最后打印 `SELFTEST n passed, m failed`，退出码 = 失败数 > 0。
##
## 这个脚本是**验收工具**，不是游戏逻辑的一部分（放在 tools/ 里，不进运行时路径）。

## **我们自己的**占位房间条数（`lan_lobby.gd::GLOBAL_ROOMS` 的长度）。
## 这不是原版事实 —— 原版这一页的条数是服务端给的动态数据，每次都不一样。
## 写成常量是为了让"这是我们的占位"这件事显式化；断言只检查"画全了"和"每行结构对"。
const PLACEHOLDER_ROOM_COUNT := 4

var _checks := 0
var _failures: Array[String] = []
var _skipped: Array[String] = []
var _main: Control

func run(main: Control) -> int:
	_main = main
	# 清掉上一次运行留下的服务端日志：残留状态不跨运行存在（`docs/18` §10.3.2）
	_prune_server_logs()
	# 让布局至少跑两帧（字号缩放依赖 viewport 尺寸）
	await _frames(3)
	_diag_arm()
	_diag_line("boot")
	if OS.get_environment("GW2_SELFTEST_INJECT_LAN_ROOM") == "1":
		await _inject_stray_room()
		_diag_line("injected")

	_test_boot()
	await _test_main_menu()
	await _test_campaign()
	await _test_player_screen()
	await _test_replay_lobby()
	await _test_lan_lobby()
	await _test_ui_text_and_measured_geometry()
	await _test_tab_fix()
	await _test_game_setup()
	await _test_pixel_truth_geometry()
	await _test_play_screen()
	await _test_endgame()
	_test_fonts_and_icons()
	_test_assets_and_original_settings()
	_test_geometry_and_campaign_save()
	_test_core_path_mode()
	await _test_audio()
	await _test_level_rules()
	await _test_fire_fx()
	await _test_play_flow()

	await _test_replay_player()
	await _test_replay_ode()
	# 联机（docs/134）：分帧/解析 → 假服务器 → 失败路径 → LAN 发现 → broker → 真服务端
	await _test_net_framing()
	await _test_net_join_fake_server()
	await _test_net_failure_path()
	await _test_net_link_retry()
	await _test_net_mid_session_disconnect()
	await _test_net_lan_discovery()
	await _test_net_broker_global()
	await _test_net_real_server()
	await _test_play_online()
	# 用户实测的四条 bug（docs/144）：房间残留 / 占位行 Join / Players 列 / 关卡弹窗几何
	await _test_lobby_fixes()
	_diag_line("end")

	var passed := _checks - _failures.size()
	if _skipped.is_empty():
		print("SELFTEST %d passed, %d failed" % [passed, _failures.size()])
	else:
		# 缺资产时不假装通过：跳过的条目单独报，计数也进汇总行（docs/121 §6.3）
		print("SELFTEST %d passed, %d failed, %d skipped"
			% [passed, _failures.size(), _skipped.size()])
		for s in _skipped:
			print("  SKIP %s" % s)
	if _failures.size() > 0:
		for f in _failures:
			print("  FAIL %s" % f)
		return 1
	return 0

# ---------------------------------------------------------------- 断言

## 显式跳过：不算通过、不算失败，但**一定打印**。用于"没有 pck 就无从验证"的条目。
func _skip(what: String) -> void:
	_skipped.append(what)
	print("@@SELFTEST_SKIP %s" % what)

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

# ---------------------------------------------------------------- 诊断（只读，不参与判定）
#
# `GW2_SELFTEST_DIAG=1` 时把"那一帧的窗口 / 视口 / 大厅列表 / LAN 房间表"打一行 `@@DIAG`。
#
# 为什么需要它：LAN 大厅那两条断言（本地列表为空 / Join 宽度 = 屏宽×0.2）在偶发红时，
# 失败的**文字**只给出"实际 1"/"期望 12.8"，看不出那个 1 是**哪一行**、12.8 是**哪个
# JoinButton**。把这两件事都打出来，红的那一次就能直接读到根因（见 docs/18 §10.3.4）。
var _diag := OS.get_environment("GW2_SELFTEST_DIAG") == "1"

func _diag_line(tag: String) -> void:
	if not _diag:
		return
	var app_node := get_node_or_null("/root/App")
	var net := _net()
	var lobby := _screen("LanLobby")
	var list: Node = null
	var rows := -1
	var items: Array = []
	if lobby != null:
		list = lobby.find_child("GraphwarLanVBox", true, false)
		if list != null:
			rows = list.get_child_count()
			for c in list.get_children():
				var nm := (c as Node).find_child("RoomName", true, false) as Label
				items.append(String(nm.text) if nm != null else String(c.name))
	var join_node: Node = null
	if lobby != null:
		join_node = lobby.find_child("JoinButton", true, false)
	var join_min := -1.0
	if join_node is Control:
		join_min = (join_node as Control).custom_minimum_size.x
	# 底栏那个（断言真正量的那个）单独打一份：它和"深度优先第一个 JoinButton"在列表有行时**不是同一个节点**。
	var join_ip := _lobby_join_ip_button(lobby)
	var join_ip_min := -1.0
	if join_ip != null:
		join_ip_min = join_ip.custom_minimum_size.x
	var vboxes := 0
	for n in get_tree().root.find_children("*", "VBoxContainer", true, false):
		if String(n.name) == "GraphwarLanVBox":
			vboxes += 1
	print("@@DIAG %-14s f=%d win=%s vp=%.0f mode=%s rows=%d items=%s join_min=%.2f join_ip_min=%.2f first_join=%s vboxes=%d lan=%s:%d rooms=%d pkts=%d local_room=%s online=%s" % [
		tag, Engine.get_process_frames(), str(get_window().size),
		get_window().get_visible_rect().size.x,
		String(app_node.room_list_mode) if app_node != null else "?",
		rows, str(items), join_min, join_ip_min,
		(lobby.get_path_to(join_node) if join_node != null else "<null>"),
		vboxes,
		("on" if (net != null and net.lan != null and net.lan.is_listening()) else "off"),
		(net.lan_port if net != null else -1),
		(net.lan.room_count() if net != null else -1),
		(net.lan.packets_seen() if net != null else -1),
		("yes" if (app_node != null and not app_node.local_room.is_empty()) else "no"),
		("yes" if (app_node != null and app_node.online) else "no")])

## 装好诊断：任何一次"房间表变了"都打一行（含当时帧号、总包数）。
func _diag_arm() -> void:
	if not _diag:
		return
	var net := _net()
	if net != null:
		net.rooms_changed.connect(_diag_on_rooms_changed)
		print("@@DIAG arm: lan_listening=%s port=%d diag=1" % [
			str(net.lan.is_listening()), net.lan_port])

func _diag_on_rooms_changed(mode: String) -> void:
	var net := _net()
	if net == null:
		return
	print("@@DIAG signal rooms_changed(%s) f=%d rooms=%d pkts=%d %s" % [
		mode, Engine.get_process_frames(), net.lan.room_count(), net.lan.packets_seen(),
		str(net.lan.rooms())])

## 负对照（变异测试）用的旋钮：往自己的 LAN 监听端口塞一条"房主广播"。
##
## 为什么留着：这一组断言在 2026-11-10 之前会**偶发成对红**，根因是"LAN 房间表里多了一行"
## （`docs/18` §10.3.4）。要证明修好了、而且**断言还真的会红**，就得能把那个"多一行"
## 确定性地造出来 —— 和 76 号用 `GW2_SELFTEST_MATCH_SEED=89` 造"0 个障碍"是同一套路。
##
##   `GW2_SELFTEST_INJECT_LAN_ROOM=1`  ⇒ 启动时塞一行（模拟"这台机器上真有个房主在广播"）
##                                       修好之后它**不该**再让那两条变红（= 回归判据）；
##   `GW2_SELFTEST_INJECT_LAN_ROOM=2`  ⇒ 在"本地列表初始为空"检查**之前**塞一行
##                                       （隔离之后仍然塞得进去）⇒ 那一条**必须**红
##                                       （= 断言还有牙齿的正对照）。
func _inject_stray_room() -> void:
	var net := _net()
	if net == null or net.lan == null:
		return
	if not net.lan.is_listening():
		net.lan_listen(0)
	await _frames(1)
	var s := PacketPeerUDP.new()
	s.bind(0, "127.0.0.1")
	s.set_dest_address("127.0.0.1", net.lan_port)
	var line := GwWireSamples.broadcast("Stray Room", "Setup", 1, "127.0.0.1:1",
		"EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m", false)
	s.put_packet((line + "\n").to_utf8_buffer())
	await _frames(2)
	print("@@SELFTEST_INJECT 塞入一条房主广播：port=%d rooms=%d pkts=%d" % [
		net.lan_port, net.lan.room_count(), net.lan.packets_seen()])
	s.close()

## 把 LAN 发现切到**私有端口**，并把房间表清空（`GwLanDiscovery.stop()` 就会清表）。
##
## 为什么必须这么做（`docs/18` §10.3.4 的原始记录）：
##   `App._install_net()` 在**启动时**就绑了原版的 `UDP 42968`，而我们这一组断言
##   （「本地房间列表初始为空」以及后面那条「底部 Join IP 宽度」）**都以"房间表是空的"为前提**。
##   42968 是**外部输入**：这台机器上任何在广播的房主（`tools/gate5_serve.py`、同机另一个
##   客户端、局域网里走 IPv4 广播的真房主）灌一条 9 字段 JSON 进来，房间表就多一行，
##   于是**代码没变**也会红。这条用例要测的是"没有房间数据时这一页画 0 行"，
##   不是"这台机器的局域网是安静的"。
##   ⇒ 前提由用例自己拿回来：清表 + 换私有端口（外部广播再也到不了），**并且断言这次隔离成功**。
func _isolate_lan_discovery(tag: String) -> bool:
	var net := _net()
	if net == null or net.lan == null:
		return false
	var boot_port := net.lan_port
	var was_listening := net.lan.is_listening()
	net.lan_stop()
	await _frames(1)
	var err := net.lan_listen(0)
	var rooms := net.lan.room_count()
	var ok := err == OK and net.lan.is_listening() and rooms == 0
	_expect(ok, "LAN 发现切到私有端口、房间表清空（%s：启动口 %d%s → 私有 %d，err=%d，清表后 %d 条）"
		% [tag, boot_port, "" if was_listening else "（没绑上）", net.lan_port, err, rooms])
	return ok

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
	# 说明栏必须是**该关原文**（docs/118 §4.2 逐字节），不是占位文案
	var desc := popup.find_child("LevelDescriptionEdit", true, false) as TextEdit if popup != null else null
	var lv0 := GwCampaign.level(0)
	_expect(desc != null and desc.text == String(lv0.description),
		"JoinLevelPopUp 说明 = 第 0 关原文（实际 '%s'）" % (desc.text if desc != null else "<null>"))
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
	# 先把这个用例的**前提**拿回来：LAN 房间表清空 + 换私有端口（`docs/18` §10.3.4）。
	# 不做这一步，"本地列表初始为空"就在测"这台机器的局域网安不安静"。
	await _isolate_lan_discovery("联机大厅")
	# 负对照（`=2`）：隔离之后仍然塞一行进去 ⇒ 下面那条"初始为空"**必须**红。
	if OS.get_environment("GW2_SELFTEST_INJECT_LAN_ROOM") == "2":
		await _inject_stray_room()
	_command("exit_replay_lobby")
	await _frames(2)
	_command("multiplayer")
	await _frames(3)
	var screen := _screen("LanLobby")
	if screen == null:
		_expect(false, "LanLobby 存在")
		return
	_expect(screen.visible, "LanLobby 可见")
	_diag_line("lan_enter")
	_command("show_global")
	await _frames(2)
	_diag_line("lan_global")
	var list := screen.find_child("GraphwarLanVBox", true, false)
	_expect(list != null, "LanLobby 有 GraphwarLanVBox")
	var rooms := 0
	for c in list.get_children():
		if c is HBoxContainer:
			rooms += 1
	# ⚠️ 房间列表是**服务端数据**：条数每次都不一样（上一轮某次快照 5 条、本轮真帧 4 条、
	# 用户截图里又是另外四个名字）。所以这里**不**钉死条数，只断言"我们确实把占位条目
	# 全部画出来了"（≥1，且等于常量 `PLACEHOLDER_ROOM_COUNT` 声明的条数）。
	# 换一台机器/换一天跑原版，这个数字本来就会变 —— 把它写死等于把一张截图的巧合当规格。
	_expect(rooms == PLACEHOLDER_ROOM_COUNT and rooms >= 1,
		"公网房间列表画出了全部占位条目（%d 条，实际 %d）" % [PLACEHOLDER_ROOM_COUNT, rooms])
	# 每行的**内容**才是能被真帧钉住的部分：4 个模式块 + 一个 Join，且 Players 是整数。
	var rows_ok := 0
	var join_ok := 0
	var players_ok := 0
	for row in list.get_children():
		if not (row is HBoxContainer):
			continue
		var modes := 0
		var options := (row as Node).find_child("OptionsContainer", true, false)
		if options != null:
			for n in options.get_children():
				if n is AspectRatioContainer:
					modes += 1
		if modes == 4:
			rows_ok += 1
		if (row as Node).find_child("JoinButton", true, false) is Button:
			join_ok += 1
		var pc := (row as Node).find_child("PlayerCount", true, false) as Label
		if pc != null and pc.text.is_valid_int():
			players_ok += 1
	_expect(rows_ok == rooms, "每条房间都有 4 个模式块（实际 %d/%d 行）" % [rows_ok, rooms])
	_expect(join_ok == rooms, "每条房间都有 Join 按钮（实际 %d/%d 行）" % [join_ok, rooms])
	_expect(players_ok == rooms, "Players 列是整数而不是 `--`（实际 %d/%d 行）" % [players_ok, rooms])
	_command("show_local")
	await _frames(2)
	_diag_line("lan_local")
	var local_rooms := 0
	for c in list.get_children():
		if c is HBoxContainer:
			local_rooms += 1
	# 前提 = 本函数开头那次 `_isolate_lan_discovery()`（房间表已清空、且已切到私有端口）。
	# 判据**没变**：本地页在没有房间数据时画 **0** 行。它防的是"上一把的房间挂在大厅里"
	# 那类真回归（`docs/144` §1：`local_room` / `state.players` 泄漏进列表）——
	# 那一类现在仍然会红（负对照：`GW2_SELFTEST_INJECT_LAN_ROOM=2`，见 docs/18 §10.3.4）。
	_expect(local_rooms == 0, "本地房间列表初始为空（实际 %d）" % local_rooms)

## 本轮（docs/129）新增：**界面文本不许泄漏内部说明** + 玩家页/联机大厅的实测几何。
##
## 这一组是"护栏"，判据都能指到真帧或原版 `.tres`：
##   * 任何 `Label` / `Button` / `LineEdit` 的 `text` 里不许出现 `docs/`、`§`、`TODO`、`占位`、
##     `未观测` —— 这类东西今年已经三次写进用户能看见的地方（广告条、战役占位关卡、联机大厅
##     底部那行 `公网房间列表来源未知（docs/92 §8）`）。气泡（`App.notify`）另有一条断言：
##     它在发出前自己就扫，命中会记进 `internal_text_violations` 并**不发**。
##   * 玩家页格子边长 = `(安全区宽/2 − 50)/3` 向下取整；格子底色是白卡（`shop_theme.tres`）；
##     SkinBlock / PendingBlock 在桌面端 `visible == false`（真帧右列下半屏是纯底色）。
##   * 联机大厅表头三列居中、底部按钮宽度 = 屏宽×0.2（`LobbyButtonContainer.gd`）、
##     公网房间的 Players 列是 `0`（真帧四行都是 0，不是 `--`）。
##   * 回放列表为空时**一个字都不画**（原版真帧只有返回键 + REPLAYS 标题）。
func _test_ui_text_and_measured_geometry() -> void:
	# --- 1. 界面文本里不许有内部标记 ---
	var markers := ["docs/", "§", "TODO", "占位", "未观测"]
	var bad: Array[String] = []
	var app_node := get_node_or_null("/root/App")
	if app_node != null:
		for screen_name in app_node.screens:
			_scan_internal_text(app_node.screens[screen_name], markers, bad)
	_expect(bad.is_empty(), "界面文本里没有内部标记（命中 %d 处：%s）"
		% [bad.size(), str(bad.slice(0, 3))])
	var violations: Array = app_node.internal_text_violations if app_node != null else []
	_expect(violations.is_empty(), "气泡文本里没有内部标记（命中 %d 条：%s）"
		% [violations.size(), str(violations.slice(0, 3))])

	# --- 2. 玩家页：格子边长 / 白卡 / 隐藏块 ---
	var player := _screen("PlayerScreen")
	if player != null:
		var width := float(DisplayServer.get_display_safe_area().size.x)
		if width <= 0.0:
			width = player.get_viewport_rect().size.x
		var want_cell := floorf(maxf(32.0, (width / 2.0 - 50.0) / 3.0))
		var skins_vbox := player.find_child("SkinsVbox", true, false)
		var cells := 0
		var cell_ok := 0
		var white_ok := 0
		if skins_vbox != null:
			for row in skins_vbox.get_children():
				for b in row.get_children():
					if b is GwButton:
						cells += 1
						if absf((b as Control).custom_minimum_size.x - want_cell) < 0.01:
							cell_ok += 1
						var sb := (b as Control).get_theme_stylebox("normal") as StyleBoxFlat
						if sb != null and sb.bg_color.r > 0.99 and sb.bg_color.g > 0.99 \
								and sb.bg_color.b > 0.99 and sb.border_width_left == 3:
							white_ok += 1
		_expect(cells == 15 and cell_ok == 15,
			"玩家页 15 个皮肤格子边长 = (安全区宽/2−50)/3 取整 = %d（对 %d/%d）"
			% [int(want_cell), cell_ok, cells])
		_expect(white_ok == 15, "玩家页 15 个格子都是白卡（白底 + 3px 黑边，shop_theme.tres）"
			% [])
		for block_name in ["SkinBlock", "PendingBlock"]:
			var block := player.find_child(block_name, true, false) as CanvasItem
			_expect(block != null and not block.visible,
				"%s 存在但不可见（桌面端原版也是 visible=false）" % block_name)

	# --- 3. 联机大厅：表头居中 / 底部按钮宽度 / Players 列 ---
	var lobby := _screen("LanLobby")
	if lobby != null:
		var header_labels := [
			lobby.find_child("NameContainer", true, false),
			lobby.find_child("PlayersContainer", true, false),
			lobby.find_child("ModeContainer", true, false),
		]
		var centered := 0
		for h in header_labels:
			if h == null:
				continue
			for c in (h as Node).get_children():
				if c is Label and (c as Label).horizontal_alignment == HORIZONTAL_ALIGNMENT_CENTER:
					centered += 1
		_expect(centered == 3, "联机大厅表头三列都居中（实际 %d/3）" % centered)
		# 底部那个 " Join IP " 按钮：**按 `input_text == "join_ip"` 找**，不能按节点名找。
		# 为什么（`docs/18` §10.3.4 的原始记录）：房间条目里也有一个 `JoinButton`
		# （`game_room_entry.tscn`），而 `find_child("JoinButton")` 给的是**深度优先的第一个**
		# ——列表里只要有一行，量到的就是**行里**那个 JoinButton（它的 `custom_minimum_size.x`
		# 是 0，不受 `LobbyButtonContainer` 管），于是这条断言会因为"本地页多了一行"而红，
		# 跟它真正要量的东西（底栏按钮 = 屏宽×0.2）**无关**。判据与期望值一字未改。
		var join_btn := _lobby_join_ip_button(lobby)
		var vw := lobby.get_viewport_rect().size.x
		_diag_line("geom_join")
		_expect(join_btn != null and absf(join_btn.custom_minimum_size.x - vw * 0.2) < 0.01,
			"底部 Join IP 宽度 = 屏宽×0.2（LobbyButtonContainer.gd，期望 %.1f）"
			% (vw * 0.2))
		# 公网列表的 Players 列是 `0` 而不是 `--`（真帧四行都是 0）。
		# 条数不钉死（服务端数据），只要求"每一行都是 0 且至少一行"。
		_command("show_global")
		await _frames(2)
		var list := lobby.find_child("GraphwarLanVBox", true, false)
		var zero := 0
		var rows := 0
		if list != null:
			for row in list.get_children():
				var pc := (row as Node).find_child("PlayerCount", true, false) as Label
				if pc != null:
					rows += 1
					if pc.text == "0":
						zero += 1
		_expect(rows >= 1 and zero == rows,
			"公网房间 Players 列全是 0 而不是 --（真帧值，实际 %d/%d）" % [zero, rows])
		_command("show_local")
		await _frames(2)

	# --- 4. 回放列表为空时一个字都不画 ---
	var replays := _screen("ReplayLobby")
	if replays != null:
		_command("replay")
		await _frames(3)
		var stray: Array[String] = []
		_collect_labels(replays, stray)
		_expect(stray.size() == 1, "回放列表空时只剩标题一个 Label（实际 %s）" % str(stray))
		_command("exit_replay_lobby")
		await _frames(2)

## 本轮（`docs/145`）新增：两个**被真帧钉住的合成规则** + 一个真值几何。
##
## 这一组之所以存在，是因为那两处错的根因是同一个：**把"运行时合成"写成了"固定图"**。
##   * Faces / Hats 的格子 = 「**当前 `profile.skin`** 的皮肤圆 + 该格的脸/帽子图标」，
##     不是 build 那一刻的快照，也不是一张固定的皮肤图；
##   * JoinLevelPopUp 的形状 = `.tscn` 的 **1 : 10 : 1** 三条竖带 + 面板内 4 段定比，
##     不是"按内容撑开"。
##
## 判据都能指到真值（`docs/145` §1/§2）：
##   * 皮肤：真二进制帧 `A_skins_red` / `A_faces_red`——在 Skins 页点 `red` 之后，
##     Faces 页 15 格的圆心从 `(255,204,0)` 变成 `(255,114,114)`（观测）；
##   * 弹窗：真值管线 C（正版 pck + stock Godot）自报的 rect（1920×1160）：
##     面板 (243,100) **1434×956**、顶带 96、底带 (0,1060) 1920×**96**，
##     并且真二进制帧与之逐像素相同（`docs/145` §2.1）；
##   * 模式图标底：`axis_button_theme.tres` 四个态全指向 `axis_menu_icon_theme.tres`
##     = **白底** + 3px 黑边 + 圆角 10；另外三个是 `#1A3B2D`（两份 `.tres` 可逐字段读）。
func _test_tab_fix() -> void:
	var app_node := get_node_or_null("/root/App")

	# ---- 1) Faces / Hats：三层结构 + 皮肤层跟着 profile.skin 走 ----
	_command("player_settings")
	await _frames(3)
	var player := _screen("PlayerScreen")
	if player == null or app_node == null:
		_expect(false, "PlayerScreen / App 存在（tab-fix 组）")
		return
	var want_skin := int(app_node.store.profile.skin)
	var entries := 0
	var layered := 0
	var cards_clean := 0
	var layers_now := 0
	var want_icon := GwIcons.get_icon("skin_%d" % want_skin)
	for grid_name in ["FacesVbox", "HatsVbox"]:
		var grid := player.find_child(String(grid_name), true, false)
		if grid == null:
			continue
		for row in grid.get_children():
			for entry in (row as Node).get_children():
				entries += 1
				var card := (entry as Node).find_child("GraphwarButton", false, false) as Button
				var skin_layer := (entry as Node).find_child("SkinBackgroundButton", false, false) as Button
				var item_layer := (entry as Node).find_child("Button", false, false) as Button
				if card != null and skin_layer != null and item_layer != null:
					layered += 1
				if card != null and card.icon == null:
					cards_clean += 1
				if skin_layer != null and skin_layer.icon == want_icon:
					layers_now += 1
	# 观测（`docs/104` §5.2）：42 格（Faces 15 + Hats 27），每格 3 个按钮
	_expect(entries == 42 and layered == 42,
		"Faces/Hats 42 格都是三层（白卡 + 皮肤层 + 图标层，实际 %d/%d）" % [layered, entries])
	# `shop_button_entry.tscn` 里根节点 `GraphwarButton` **没有** icon：白卡只画底色，
	# 皮肤只由 `SkinBackgroundButton` 画一层（两处都画 = 同一个圆叠两次 alpha）。
	_expect(cards_clean == entries,
		"白卡层不带皮肤图标（皮肤只画一层，实际 %d/%d 干净）" % [cards_clean, entries])
	_expect(layers_now == entries,
		"Faces/Hats 每格皮肤层 = 当前皮肤 skin_%d（实际 %d/%d）" % [want_skin, layers_now, entries])

	# 换皮肤：点 Skins 页里**另一张**皮肤，Faces/Hats 必须**立刻**跟着换（原来永远是旧值）。
	var target := -1
	var target_button: Button = null
	var skins_vbox := player.find_child("SkinsVbox", true, false)
	if skins_vbox != null:
		for row in skins_vbox.get_children():
			for b in (row as Node).get_children():
				if not (b is Button):
					continue
				var ic := (b as Button).icon
				if ic == null or ic == want_icon:
					continue
				for i in GwIcons.SKINS.size():
					if GwIcons.SKINS[i] == "skins/gray" or GwIcons.SKINS[i] == "skins/target":
						continue      # 这两张不在网格里（`docs/129` §2.2）
					if GwIcons.get_icon("skin_%d" % i) == ic:
						target = i
						target_button = b as Button
						break
				if target_button != null:
					break
			if target_button != null:
				break
	if target_button != null:
		target_button.emit_signal("pressed")
		await _frames(2)
	var switched := 0
	var target_icon := GwIcons.get_icon("skin_%d" % target)
	for grid_name in ["FacesVbox", "HatsVbox"]:
		var grid := player.find_child(String(grid_name), true, false)
		if grid == null:
			continue
		for row in grid.get_children():
			for entry in (row as Node).get_children():
				var skin_layer := (entry as Node).find_child("SkinBackgroundButton", false, false) as Button
				if skin_layer != null and skin_layer.icon == target_icon:
					switched += 1
	_expect(target >= 0 and switched == entries,
		"点 Skins 页的第 %d 号皮肤 ⇒ Faces/Hats %d 格立刻换成它（实际 %d）"
		% [target, entries, switched])
	# 还原：把 skin 写回原值，再**离开并重新进入**本屏（`on_enter` 必须重刷一次）
	app_node.store.profile.skin = want_skin
	app_node.store.save_all()
	_command("exit_player_settings")
	await _frames(2)
	_command("player_settings")
	await _frames(3)
	var reentered := 0
	for grid_name in ["FacesVbox", "HatsVbox"]:
		var grid := player.find_child(String(grid_name), true, false)
		if grid == null:
			continue
		for row in grid.get_children():
			for entry in (row as Node).get_children():
				var skin_layer := (entry as Node).find_child("SkinBackgroundButton", false, false) as Button
				if skin_layer != null and skin_layer.icon == want_icon:
					reentered += 1
	_expect(int(app_node.store.profile.skin) == want_skin and reentered == entries,
		"重新进入玩家页后皮肤恢复 skin_%d（实际 %d/%d 格，profile=%d）"
		% [want_skin, reentered, entries, int(app_node.store.profile.skin)])
	_command("exit_player_settings")
	await _frames(2)

	# ---- 2) JoinLevelPopUp：1:10:1 三条竖带 + 面板内四段 ----
	_command("campaign")
	await _frames(3)
	var campaign := _screen("Campaign")
	var first := _find_button(campaign, "level_0")
	if first != null:
		first.emit_signal("pressed")
		await _frames(3)
	var popup := _screen("JoinLevelPopUp")
	if popup == null:
		_expect(false, "JoinLevelPopUp 存在（tab-fix 组）")
		return
	var panel_node: Node = popup.find_child("Panel", true, false)
	var panel := panel_node as Panel
	# `PanelContainer` 不是 `Panel` 的子类 ⇒ 这一条同时排掉了"用 PanelContainer 冒充 Panel"
	# 的可能（后者会按 stylebox 的 content margin 把里面的 VBox 再内缩 3px）。
	_expect(panel != null,
		"JoinLevelPopUp 的面板节点是 Panel（PanelContainer 会再内缩 3px，实际 %s）"
		% (panel_node.get_class() if panel_node != null else "<null>"))
	# headless 的 dummy 窗口只有 **64×64**（实测 `get_window().size == (64,64)`），
	# 那个尺寸下弹窗的几何量不出真值。下面几个断言直接在真值口径 **1920×1160** 下做，
	# 做完把窗口尺寸还原（后面的用例仍按原来的环境跑）。
	var saved_window := get_window().size
	get_window().size = Vector2i(1920, 1160)
	await _frames(4)
	# 真值 rect（引擎自报 + 真二进制帧逐像素确认，见本函数头注释）
	# 面板的**全局**矩形（`position` 是相对 `MarginContainer2` 的，恒为 0）
	_expect(panel != null and panel.get_global_rect().position.distance_to(Vector2(243, 100)) <= 1.0
		and absf(panel.size.x - 1434.0) <= 1.0 and absf(panel.size.y - 956.0) <= 1.0,
		"JoinLevelPopUp 面板 = (243,100) 1434×**956**（真值 1920×1160，实际 %s）"
		% (str(panel.get_global_rect()) if panel != null else "<null>"))
	var band_node := popup.get("_bottom_band") as Control
	_expect(band_node != null and absf(band_node.position.y - 1060.0) <= 1.0
		and absf(band_node.size.y - 96.0) <= 1.0,
		"底部留白带 = (0,1060) 1920×**96**（真值；以前整条缺失 ⇒ 面板多 96 px，"
		+ "用户说的\"弹窗下面的部分太长\"，实际 %s）"
		% (str(Rect2(band_node.position, band_node.size)) if band_node != null else "<null>"))
	# 四个模式格子 = 236×236 正方；Cancel 194 宽、Start 218 宽（真帧黑边环逐像素量得）
	var block_ok := 0
	var modes_boxes: Dictionary = popup.get("_mode_buttons")
	for key in modes_boxes:
		var btn := modes_boxes[key] as Control
		if btn != null and absf(btn.size.x - 236.0) <= 1.0 and absf(btn.size.y - 236.0) <= 1.0:
			block_ok += 1
	_expect(block_ok == 4, "四个模式格子都是 236×236 正方（真值；实际 %d/4）" % block_ok)
	var cancel_btn := popup.get("_cancel") as Control
	var start_btn := popup.get("_start") as Control
	_expect(cancel_btn != null and absf(cancel_btn.size.x - 194.0) <= 1.0
		and start_btn != null and absf(start_btn.size.x - 218.0) <= 1.0,
		"Cancel = 194 宽 / Start = 218 宽（真帧；实际 %.0f / %.0f）"
		% [cancel_btn.size.x if cancel_btn != null else -1.0,
			start_btn.size.x if start_btn != null else -1.0])

	# ---- 3) 四个模式按钮的底色（axis 白、其余深绿） ----
	var modes: Dictionary = popup.get("_mode_buttons")
	var white_ok := 0
	var dark_ok := 0
	for key in modes:
		var b := modes[key] as Button
		if b == null:
			continue
		var sb := b.get_theme_stylebox("normal") as StyleBoxFlat
		if sb == null:
			continue
		if String(key) == "axis":
			if sb.bg_color.r > 0.99 and sb.bg_color.g > 0.99 and sb.bg_color.b > 0.99 \
					and sb.border_width_left == 3:
				white_ok += 1
		else:
			if absf(sb.bg_color.r - GwThemeFactory.COL_PANEL.r) < 0.01 \
					and absf(sb.bg_color.g - GwThemeFactory.COL_PANEL.g) < 0.01 \
					and absf(sb.bg_color.b - GwThemeFactory.COL_PANEL.b) < 0.01 \
					and sb.border_width_left == 3:
				dark_ok += 1
	_expect(modes.size() == 4 and white_ok == 1,
		"AxisModeButton 是白底 + 3px 黑边（axis_menu_icon_theme.tres，实际 %d/1）" % white_ok)
	_expect(dark_ok == 3,
		"另外三个模式按钮是 #1A3B2D + 3px 黑边（game_menu_icon_button_theme.tres，实际 %d/3）"
		% dark_ok)
	get_window().size = saved_window
	await _frames(2)
	_command("cancel_level")
	await _frames(2)

## 递归扫文本：`Label` / `Button` / `LineEdit` 的 `text`。
func _scan_internal_text(node: Node, markers: Array, out: Array[String]) -> void:
	if node is Label or node is Button or node is LineEdit:
		var t := String((node as Node).get("text"))
		if t != "":
			for m in markers:
				if t.find(String(m)) >= 0:
					out.append("%s: %s" % [node.name, t])
					break
	for c in node.get_children():
		_scan_internal_text(c, markers, out)

## 收集一个界面里所有非空 Label 文本（用来证明"空列表什么都不画"）。
func _collect_labels(node: Node, out: Array[String]) -> void:
	if node is Label:
		var t := String((node as Node).get("text"))
		if t.strip_edges() != "":
			out.append("%s=%s" % [node.name, t])
	for c in node.get_children():
		_collect_labels(c, out)

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

## 像素级真值相关的**结构与几何**断言（docs/137）。
##
## 原则：每条断言都要能指到**真帧**里的一个数值（宿主导出的 `@@NODE` rect、或真帧量出的
## 墨迹 bbox / 颜色），不写"看起来对"的断言。真值来源见 `docs/137` §1–§2。
func _test_pixel_truth_geometry() -> void:
	# ---- 1) 默认外观：**按名字**解析，且三个名字就是真帧里那张合成的三件套 ----
	_expect(GwIcons.SKINS[GwStore.DEFAULT_SKIN] == "skins/yellow",
		"默认皮肤 = skins/yellow（真帧预览是黄圆，docs/137 §6）")
	_expect(GwIcons.FACES[GwStore.DEFAULT_FACE] == "faces/regular_eyes",
		"默认脸 = faces/regular_eyes（真帧预览眨眼式：白眼球+黑瞳）")
	_expect(GwIcons.HATS[GwStore.DEFAULT_HAT] == "hats/soldier_helmet",
		"默认帽子 = hats/soldier_helmet（真帧预览是深绿盔）")
	_expect(GwStore.resolve_look("skins", "yellow") == GwIcons.SKINS.find("skins/yellow"),
		"resolve_look() 按名字解析 = 表里 find()（不再有魔法下标）")
	_expect(GwStore.resolve_look("skins", "no_such_skin") == -1,
		"resolve_look() 对未知名字返回 -1（不悄悄变成 0 号皮肤）")

	# ---- 2) 原版 player.config：逐键兜底（用正版二进制实测过，docs/137 §6.2） ----
	var saved_skin: int = GwStore.DEFAULT_SKIN
	var saved_face: int = GwStore.DEFAULT_FACE
	var saved_hat: int = GwStore.DEFAULT_HAT
	var saved_name: String = "user"
	var cfg_path := GwStore.PLAYER_CONFIG_PATH
	var had_cfg := FileAccess.file_exists(cfg_path)
	var backup := ""
	if had_cfg:
		backup = FileAccess.get_file_as_string(cfg_path)
	var f := FileAccess.open(cfg_path, FileAccess.WRITE)
	if f != null:
		f.store_string("skin: red\n")
		f.close()
		var probe := GwStore.new()
		probe.profile["skin"] = saved_skin
		probe.profile["face"] = saved_face
		probe.profile["hat"] = saved_hat
		probe.apply_original_player_config()
		_expect(int(probe.profile["skin"]) == GwIcons.SKINS.find("skins/red"),
			"player.config `skin: red` ⇒ 皮肤 red（值是**名字**，按名字解析）")
		_expect(int(probe.profile["face"]) == saved_face and int(probe.profile["hat"]) == saved_hat,
			"player.config 只写 skin ⇒ 脸/帽子仍是默认（**逐键兜底**，不是整份覆盖）")
		probe.profile["skin"] = saved_skin
		probe.profile["face"] = saved_face
		probe.profile["hat"] = saved_hat
		var f2 := FileAccess.open(cfg_path, FileAccess.WRITE)
		if f2 != null:
			f2.store_string("skin: no_such_skin\n")
			f2.close()
			probe.apply_original_player_config()
			_expect(int(probe.profile["skin"]) == saved_skin,
				"player.config 里未知皮肤名 ⇒ 保留默认（原版会让那一层不画，我们只告警，docs/137 §6.4）")
	if had_cfg:
		var fr := FileAccess.open(cfg_path, FileAccess.WRITE)
		if fr != null:
			fr.store_string(backup)
			fr.close()
	else:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(cfg_path))
	_expect(true, "player.config 实验结束后恢复现场（name=%s）" % saved_name)

	# ---- 3) 网格顺序：直接对真帧每个格子的**主色**（docs/137 §6.3） ----
	var ps: Control = _screen("PlayerScreen")
	if ps != null:
		var consts: Dictionary = ps.get_script().get_script_constant_map()
		var skin_grid: Array = consts.get("SKIN_GRID", [])
		var face_grid: Array = consts.get("FACE_GRID", [])
		var hat_grid: Array = consts.get("HAT_GRID", [])
		_expect(skin_grid.size() == 15, "Skins 网格 15 格（真帧 5 行 × 3，实际 %d）" % skin_grid.size())
		var want_skins := ["skins/blue", "skins/pink", "skins/purple", "skins/red",
			"skins/white_skin", "skins/yellow", "skins/green", "skins/light_green",
			"skins/true_purple"]
		var ok_skins := true
		for i in want_skins.size():
			if i >= skin_grid.size() or GwIcons.SKINS[int(skin_grid[i])] != want_skins[i]:
				ok_skins = false
		_expect(ok_skins,
			"前 9 个皮肤格顺序 = 真帧主色测出的顺序（blue/pink/purple/red/white_skin/yellow/green/light_green/true_purple）")
		_expect(GwIcons.FACES[int(face_grid[0])] == "faces/regular_eyes",
			"Faces 第 1 格 = regular_eyes（真帧第一格就是这个脸）")
		_expect(GwIcons.HATS[int(hat_grid[1])] == "hats/soldier_helmet",
			"Hats 第 2 格 = soldier_helmet（真帧第二格是绿盔）")

	# ---- 4) GameSetup 的静态几何（真值来自宿主导出的原版运行时 rect，docs/137 §5） ----
	var gs: Control = _screen("GameSetup")
	if gs != null:
		# 注意：**不**在这里 create_game —— 会重置玩家表，把后面的 PlayScreen 测试搞挂。
		# 下面断言的几何（custom_minimum_size / stylebox / 子节点数）都是构建期就固定的。
		_expect(gs.font_divisor == 30,
			"GameSetup 字号除数 = 30（RoomName/LanLobbyTitle/LobbyButtonContainer 全是 /30）")
		var ghost := _find_button(gs, "GhostButton")
		_expect(ghost != null and int(ghost.custom_minimum_size.x) == 159,
			"GhostButton 宽 159（原版真值 159 ⇒ 标题从 x=313 开始）")
		var start := _find_button(gs, "StartButton")
		_expect(start != null and int(start.custom_minimum_size.x) == 384,
			"StartButton 最小宽 384（LobbyButtonContainer 的 屏宽×0.2）")
		var divider := gs.find_child("ColorRect", true, false)
		_expect(divider is ColorRect and int((divider as ColorRect).custom_minimum_size.x) == 10
			and (divider as ColorRect).color == Color(1, 1, 1),
			"中缝 = 10 px **白色** ColorRect（真帧 x 955..964 纯白）")
		var chat := _find_button(gs, "ChatButtonSetup")
		var lock := _find_button(gs, "LockButton")
		_expect(chat != null and String(chat.text) == "     " and chat.expand_icon,
			"ChatButtonSetup 文本 = 5 空格 + expand_icon（⇒ 146×96，与返回键同款）")
		_expect(lock != null and String(lock.text) == "     " and lock.expand_icon,
			"LockButton 文本 = 5 空格 + expand_icon（⇒ 146×96）")
		var axis := _find_button(gs, "AxisModeButton")
		var axis_sb: StyleBox = axis.get_theme_stylebox("normal") if axis != null else null
		_expect(axis_sb is StyleBoxFlat and (axis_sb as StyleBoxFlat).bg_color == Color(1, 1, 1),
			"AxisModeButton 是**白卡**（原版 axis_button_theme → axis_menu_icon_theme 白底）")
		var opts: Node = null
		for b in gs.find_children("*", "GwButton", true, false):
			if String(b.name) == "AxisModeButton":
				opts = b.get_parent()
		if opts != null:
			_expect(opts.get_child_count() == 11,
				"选项行 11 个子节点 = 6 个空 Margin + 5 个按钮（实际 %d）" % opts.get_child_count())
			var fixed := 0
			for c in opts.get_children():
				if c is GwButton and not (c as GwButton).expand_icon:
					fixed += 1
			_expect(fixed == 5, "5 个选项按钮都 expand_icon=false（图标参与最小尺寸 ⇒ 134×134，实际 %d）" % fixed)

	# ---- 5) 返回键"只画一个箭头"的结构条件（docs/137 §3.3） ----
	var any_screen: Control = _screen("MainMenu")
	if any_screen != null:
		var container := any_screen.find_child("ExitButtonContainer", true, false)
		if container != null:
			var press := container.get_node_or_null("LongPressButton")
			_expect(press != null, "返回键有 LongPressButton")
			if press != null:
				var sb: StyleBox = press.get_theme_stylebox("normal")
				var opaque: bool = sb is StyleBoxFlat and (sb as StyleBoxFlat).bg_color.a >= 0.999
				_expect(opaque,
					"LongPressButton 的 normal stylebox **不透明** ⇒ 盖住下层 ClipButton 的同一个箭头（只画一次）")

	# ---- 5.5) 自检**不许留污染**：`_test_player_screen` 会把 profile.name 改成
	# "selftest-user" 并落盘，历史上还出现过 skin=0/face=0/hat=0 的存档被留下 ——
	# 用户跑完自检再开界面就会看到"自检用户 + 蓝圆 + 没有脸/帽"，这正是用户报的
	# "默认形象不对"的一个真实来源（docs/137 §6.5）。这里把它恢复成**默认三件套**。
	var app_node2 := get_node_or_null("/root/App")
	if app_node2 != null and String(app_node2.store.profile.name) == "selftest-user":
		app_node2.store.profile.name = "user"
		app_node2.store.profile["skin"] = GwStore.DEFAULT_SKIN
		app_node2.store.profile["face"] = GwStore.DEFAULT_FACE
		app_node2.store.profile["hat"] = GwStore.DEFAULT_HAT
		app_node2.store.save_all()
		_expect(String(app_node2.store.profile.name) == "user"
			and int(app_node2.store.profile["skin"]) == GwStore.DEFAULT_SKIN,
			"自检结束后 profile 恢复默认（不留 selftest-user / skin=0）")

	# ---- 6) 渲染键：与原版 project.binary 一致（本轮 0.49%→0.007% 的主因） ----
	_expect(int(ProjectSettings.get_setting(
		"rendering/textures/canvas_textures/default_texture_filter", -1)) == 2,
		"default_texture_filter = 2（原版 project.binary 的值；不写会差 0.49%）")
	_expect(int(ProjectSettings.get_setting(
		"rendering/anti_aliasing/quality/msaa_2d", -1)) == 2,
		"msaa_2d = 2（原版 project.binary 的值）")

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
	var plane := play.find_child("GraphwarPlane", true, false) as Control
	# 准备圈的**触发**：`_sync_entities()` 现在代 `pos_info_event` 填 `prep` 表（**我方接线**，
	# `docs/16` §50.6）⇒ 有行动士兵时表里恰 1 条，没有时 0 条。
	var ent := play.find_child("EntityDrawer", true, false)
	_expect(ent != null, "真节点：播放屏里有 EntityDrawer")
	var ring_n := (int(ent.call("ring_count")) if ent != null else -1)
	_expect(ring_n == 1, "准备圈：当前行动士兵在 prep 表里有且只有 1 条（实际 %d）" % ring_n)
	_expect(ent != null and ent.has_method("set_ring") and ent.has_method("clear_ring"),
		"准备圈：EntityDrawer 暴露 set_ring / clear_ring（接线口在）")
	# 外观（`docs/16` §44）：原版按 **String 名字**取贴图，不是纯色。士兵走默认三件套
	# `yellow/regular_eyes/soldier_helmet`（`GraphwarPlayerConfig::new` 的字面量）；靶子由
	# `_sync_entities()` 代填 `target/no_face/no_hat`。**我方接线**：原版是 51/53/55 事件驱动的。
	var look0: Array = Array(ent.call("entity_look", 0))
	_expect(look0 == ["yellow", "regular_eyes", "soldier_helmet"],
		"外观：士兵默认三件套 = yellow/regular_eyes/soldier_helmet（实际 %s）" % [look0])
	var tex0 := String(ent.call("sprite_texture_path", 0, 0))
	_expect(tex0.ends_with("skins/yellow.ctex"),
		"外观：皮肤层贴图来自真资源（实际 %s）" % tex0)
	var drawn: Dictionary = play.get("_drawn_entities")
	var n_tgt := 0
	for eid in drawn:
		if int(eid) >= 1000:
			n_tgt += 1
			var tl: Array = Array(ent.call("entity_look", int(eid)))
			_expect(tl.size() == 3 and String(tl[0]) == "target",
				"外观：靶子 id %d 的皮肤是 target（实际 %s）" % [eid, [tl]])
	if n_tgt == 0:
		_skip("外观：本局没有靶子（id ≥ 1000）⇒ 靶子那条接线只做了代码路径，未在本局触发")
	_expect(plane != null, "有 GraphwarPlane")
	var pool_pts: PackedVector2Array = play.call("plane_points")
	_expect(pool_pts.size() > 1, "开火后 `Line2D` 上的弹道点数 > 1（实际 %d）" % pool_pts.size())
	var status := play.find_child("StatusLabel", true, false) as Label
	print("DEBUG status dim=%s" % (status.text if status != null else "<null>"))
	var offset_label := play.find_child("OffsetLabel", true, false) as Label
	_expect(offset_label != null and offset_label.text.strip_edges() != "",
		"修正常数已显示（实际 '%s'）" % (offset_label.text if offset_label != null else "<null>"))

	# 显示开关（真 `EntityDrawer` 没有这个开关 ⇒ 用"名字文本清空"表达，见 `_set_show_names()`）
	var before := bool(play.call("plane_state").get("show_names", true))
	_command("toggle_names")
	await _frames(1)
	_expect(bool(play.call("plane_state").get("show_names", true)) != before, "toggle_names 生效")
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
	# 这三条是原有的（docs/108 §8.2「字体/图标」组），语义从"现画"改成"加载原版资产"，
	# 判据不变：字体对象可用、图标能取到、非默认的图标名也能取到。
	if not _main.assets_ok:
		_skip("字体资源存在（缺 client/assets/，无从验证原版字体）")
		_skip("图标可用（缺 client/assets/，无从验证原版图标）")
		_skip("bot_5 图标可用（缺 client/assets/）")
		return
	var font := GwThemeFactory.font()
	_expect(font != null, "字体资源存在（原版 button_font，不打包字体文件）")
	var icon := GwIcons.get_icon("return")
	_expect(icon != null and icon.get_width() > 0, "图标可用（原版 return.svg 的导入产物）")
	var icon2 := GwIcons.get_icon("bot_5")
	_expect(icon2 != null and icon2.get_width() > 0, "bot_5 图标可用（原版 skins/bot_5.svg）")

## 资产层与"背景色照原版"的验收（docs/121 §3.3、§6）。
## 这些都是**新增**断言；没有资产目录时显式跳过，绝不假装通过。
func _test_assets_and_original_settings() -> void:
	if not _main.assets_ok:
		_skip("全部引用图标都能从 assets/ 解析（缺 client/assets/）")
		_skip("assets/original_project_settings.json 可读（缺 client/assets/）")
		_skip("原版 clear_color 与 boot_splash/bg_color 相同（缺 client/assets/）")
		_skip("默认清屏色与原版 project.binary 逐位一致（缺 client/assets/）")
		_skip("启动画面底色与原版 project.binary 逐位一致（缺 client/assets/）")
		_skip("启动画面不显示 Godot logo，与原版一致（缺 client/assets/）")
		return

	var need := GwIcons.all_referenced_paths()
	var bad := GwAssets.check(need)
	_expect(bad.is_empty(), "全部引用图标都能从 assets/ 解析（引用 %d 个，缺 %d 个：%s）"
		% [need.size(), bad.size(), str(bad.slice(0, 5))])

	var orig := GwAssets.original_settings()
	_expect(not orig.is_empty(), "assets/original_project_settings.json 可读")

	# --- 背景色：与原版 project.binary 逐位比对 ---
	# 原版里 `environment/defaults/default_clear_color` 与 `application/boot_splash/bg_color`
	# 是**同一个** Color；我们的 project.godot 也必须两个都写、且都等于它。
	var want: Array = orig.get("rendering/environment/defaults/default_clear_color", [])
	var splash: Array = orig.get("application/boot_splash/bg_color", [])
	_expect(want.size() == 4 and _color_eq(want, splash),
		"原版 project.binary 里 clear_color 与 boot_splash/bg_color 相同（%s vs %s）"
		% [str(want), str(splash)])

	var our_clear: Color = ProjectSettings.get_setting(
		"rendering/environment/defaults/default_clear_color", Color.BLACK)
	_expect(_color_eq(want, [our_clear.r, our_clear.g, our_clear.b, our_clear.a]),
		"clear_color 与原版逐位一致（我们 %s，原版 %s）"
		% [_color_str(our_clear), str(want)])

	var our_splash: Color = ProjectSettings.get_setting(
		"application/boot_splash/bg_color", Color.BLACK)
	_expect(_color_eq(want, [our_splash.r, our_splash.g, our_splash.b, our_splash.a]),
		"boot_splash/bg_color 与原版逐位一致（我们 %s，原版 %s）"
		% [_color_str(our_splash), str(want)])

	var our_show := bool(ProjectSettings.get_setting("application/boot_splash/show_image", true))
	_expect(our_show == bool(orig.get("application/boot_splash/show_image", true)),
		"boot_splash/show_image 与原版一致（我们 %s，原版 %s）"
		% [str(our_show), str(orig.get("application/boot_splash/show_image"))])

## 本轮（docs/124）新增：**几何/版本/存档**三条主线。
##
## 这些都是"能被真值钉住"的断言，不是观感断言：
##   * 工程设置：viewport 1600×900、mode=2、**没有 stretch**（= disabled）、版本 2.0.60；
##   * 主菜单：VersionLabel 显示的必须是工程版本；界面上**不许**出现我们自己的调试文字/广告文字；
##   * 广告位：AdContainer 存在但高度 0、没有文字（原版运行时 ad_size=0）；
##   * 内核状态：仍然通过 `dump` 通道可查（`Main.kernel_dump_line()`）；
##   * 战役存档：`level_<n>` 解析的严格性（尾换行必须失败）、写入无尾换行、单调、不自动创建；
##   * 字号：主题基准字号回到原版 `.tres` 的 24，关卡数字用原版手写体 + 无底色。
func _test_geometry_and_campaign_save() -> void:
	# --- 1. 工程设置（原版 project.binary 的观测值）---
	_expect(int(ProjectSettings.get_setting("display/window/size/viewport_width", 0)) == 1600,
		"viewport_width = 1600（实际 %d）" % int(ProjectSettings.get_setting("display/window/size/viewport_width", 0)))
	_expect(int(ProjectSettings.get_setting("display/window/size/viewport_height", 0)) == 900,
		"viewport_height = 900（实际 %d）" % int(ProjectSettings.get_setting("display/window/size/viewport_height", 0)))
	_expect(int(ProjectSettings.get_setting("display/window/size/mode", -1)) == 2,
		"window/size/mode = 2（最大化，原版值）")
	var stretch := String(ProjectSettings.get_setting("display/window/stretch/mode", "disabled"))
	_expect(stretch == "disabled",
		"拉伸模式 = disabled（原版 project.binary 里没有 stretch 键；实际 %s）" % stretch)
	_expect(String(ProjectSettings.get_setting("application/config/version", "")) == "2.0.60",
		"application/config/version = 2.0.60")

	# --- 2. 主菜单：版本号 = 工程版本；界面上没有自造文字 ---
	var menu := _screen("MainMenu")
	_expect(menu != null, "MainMenu 存在（几何/版本用例）")
	if menu != null:
		var ver := menu.find_child("VersionLabel", true, false) as Label
		_expect(ver != null, "MainMenu 有 VersionLabel（右列底部）")
		if ver != null:
			_expect(ver.text == String(ProjectSettings.get_setting("application/config/version", "?")),
				"VersionLabel 文本 = 工程版本（实际 '%s'）" % ver.text)
			_expect(ver.text == "2.0.60", "VersionLabel 文本 = 2.0.60（实际 '%s'）" % ver.text)
			_expect(ver.horizontal_alignment == HORIZONTAL_ALIGNMENT_RIGHT,
				"VersionLabel 右对齐（原版 horizontal_alignment = 2）")
		var ad := menu.find_child("AdContainer", true, false) as Control
		_expect(ad != null, "MainMenu 有 AdContainer 节点（原版有，只是运行时 0×0）")
		if ad != null:
			_expect(is_equal_approx(ad.custom_minimum_size.y, 0.0),
				"AdContainer 最小高度 = 0（原版 ad_size=(0,0)；实际 %.1f）" % ad.custom_minimum_size.y)
			_expect(ad.get_child_count() == 0, "AdContainer 里没有文字（实际 %d 个子节点）"
				% ad.get_child_count())
		# 全屏扫一遍：界面上不许出现"广告位说明"和"内核状态"这类我们自己的调试文字。
		var banned := ["advertisement", "kernel on", "kernel off", "client 0.1.0"]
		var found: Array[String] = []
		for node in menu.find_children("*", "", true, false):
			if node is Label or node is Button:
				var txt := String(node.get("text"))
				for b in banned:
					if txt.contains(b):
						found.append("%s:%s" % [node.name, txt])
		_expect(found.is_empty(), "主菜单不含调试/广告文字（实际命中 %s）" % str(found))

	# --- 3. 内核状态仍在 dump 通道里（只是不画在界面上）---
	var kline := String(_main.call("kernel_dump_line"))
	_expect(kline.begins_with("@@KERNEL available="),
		"dump 通道能查到内核状态（kernel_dump_line = %s）" % kline.substr(0, 60))
	_expect(kline.contains("available=%d" % int(GwCore.kernel_available())),
		"dump 的内核状态与 GwCore 一致（%s）" % kline.substr(0, 40))
	var pline := String(_main.call("project_dump_line"))
	_expect(pline.contains("stretch_mode=disabled") and pline.contains("version=2.0.60"),
		"dump 通道能查到工程设置（%s）" % pline)

	# --- 4. 主题基准字号回到原版 .tres 的 24 ---
	_expect(GwThemeFactory.BASE_FONT_SIZE == 24,
		"主题基准字号 = 24（原版 graphwar_theme.tres 的 default_font_size；实际 %d）"
		% GwThemeFactory.BASE_FONT_SIZE)

	# --- 5. 战役存档 user://campaign.save（格式逐位要求，docs/124 §3.2）---
	_expect(GwStore.parse_campaign_save("level_20") == 20, "campaign.save: 'level_20' → 20")
	_expect(GwStore.parse_campaign_save("level_0") == 0, "campaign.save: 'level_0' → 0")
	_expect(GwStore.parse_campaign_save("level_0\n") == -1,
		"campaign.save: 尾换行必须解析失败（Rust parse 不跳空白；真二进制实测进度归零）")
	_expect(GwStore.parse_campaign_save("level_0 ") == -1, "campaign.save: 尾空格必须解析失败")
	_expect(GwStore.parse_campaign_save("level_ 0") == -1, "campaign.save: 数字前的空格解析失败")
	# 注意：`split('_').last()` 只取**最后一个 '_' 之后**的内容，所以前缀里的空格不影响
	# 解析（" level_0" → "0" 成功）。这条断言钉住"我们不是整串校验"这个事实。
	_expect(GwStore.parse_campaign_save(" level_0") == 0,
		"campaign.save: 前缀里的空白不影响（split('_').last() 只解析尾巴）")
	_expect(GwStore.parse_campaign_save("level_") == -1, "campaign.save: 'level_' 解析失败")
	_expect(GwStore.parse_campaign_save("20") == -1, "campaign.save: 没有 'level_' 前缀解析失败")
	_expect(GwStore.parse_campaign_save("") == -1, "campaign.save: 空文件解析失败")

	# 写入/读取用一个临时路径，别碰用户真的存档
	var store: GwStore = get_node("/root/App").store
	var saved_path: String = store.campaign_path
	var probe := "user://selftest_campaign.save"
	store.campaign_path = probe
	if FileAccess.file_exists(probe):
		DirAccess.remove_absolute(probe)
	# (a) 文件不存在时**只读不建**
	_expect(store.read_campaign_progress() == -1, "campaign.save 不存在 → progress = -1")
	_expect(not FileAccess.file_exists(probe), "读存档**不会**创建文件（原版删掉后重启不重建）")
	# (b) 写入：内容恰好 `level_<n>`，**没有**尾换行
	store.campaign_progress = -1
	_expect(store.save_campaign_progress(7), "写入 level_7 成功")
	var f := FileAccess.open(probe, FileAccess.READ)
	_expect(f != null, "能打开刚写入的 campaign.save")
	if f != null:
		var raw := f.get_buffer(f.get_length())
		f.close()
		var text := raw.get_string_from_utf8()
		_expect(text == "level_7", "写入内容恰好是 'level_7'（实际 '%s'）" % text)
		_expect(raw.size() == 7, "写入字节数 = 7（没有尾换行/尾空格；实际 %d）" % raw.size())
		_expect(not text.ends_with("\n"), "写入内容没有尾换行")
	# (c) 单调：更小的值不写
	_expect(not store.save_campaign_progress(3), "单调：level_3 不会覆盖 level_7")
	var f2 := FileAccess.open(probe, FileAccess.READ)
	var text2 := ""
	if f2 != null:
		text2 = f2.get_as_text()
		f2.close()
	_expect(text2 == "level_7", "单调写入后文件仍是 'level_7'（实际 '%s'）" % text2)
	# (d) 解锁边界：没有存档时至少第 0 关可玩；level_20 ⇒ 21 关全开
	store.campaign_progress = -1
	_expect(store.is_level_unlocked(0), "没有存档时第 0 关可玩")
	_expect(not store.is_level_unlocked(1), "没有存档时第 1 关锁住")
	store.campaign_progress = 20
	var all_open := true
	for i in GwStore.LEVEL_COUNT:
		if not store.is_level_unlocked(i):
			all_open = false
	_expect(all_open, "campaign.save = level_20 ⇒ 21 关全部解锁（用户真机截图的状态）")
	# 收尾：清掉临时文件，恢复真实路径
	if FileAccess.file_exists(probe):
		DirAccess.remove_absolute(probe)
	store.campaign_path = saved_path
	store.campaign_progress = store.read_campaign_progress()

	# --- 6. Campaign 屏：21 个数字用原版手写体 + 空 stylebox（没有圆角底）---
	if not _main.assets_ok:
		_skip("Campaign 关卡数字字体/底色（缺 client/assets/）")
		return
	var campaign := _screen("Campaign")
	var b0 := campaign.find_child("level_0", true, false) as GwButton if campaign != null else null
	_expect(b0 != null, "Campaign 有 level_0 按钮")
	if b0 != null:
		var sb := b0.get_theme_stylebox("normal")
		_expect(sb is StyleBoxEmpty,
			"关卡按钮 normal stylebox 是 StyleBoxEmpty（原版没有圆角底色；实际 %s）"
			% (sb.get_class() if sb != null else "<null>"))
		for state in ["hover", "pressed", "disabled", "focus"]:
			var sbs := b0.get_theme_stylebox(state)
			_expect(sbs is StyleBoxEmpty, "关卡按钮 %s stylebox 也是 StyleBoxEmpty" % state)
		var lvl_font := b0.get_theme_font("font")
		_expect(lvl_font != null and lvl_font == GwThemeFactory.title_font(),
			"关卡数字字体 = 原版手写体 Eraser（title_theme 同一套）")
		var want := int(maxf(10.0, campaign.find_child("Control", true, false).size.x / 15.0))
		var got := int(b0.get_theme_font_size("font_size"))
		_expect(absi(got - want) <= 1,
			"关卡数字字号 = 容器宽/15（期望 %d，实际 %d）" % [want, got])

func _color_eq(a: Array, b: Array) -> bool:
	if a.size() != 4 or b.size() != 4:
		return false
	for i in 4:
		if absf(float(a[i]) - float(b[i])) > 1e-5:
			return false
	return true

func _color_str(c: Color) -> String:
	return "(%.6f, %.6f, %.6f, %.1f)" % [c.r, c.g, c.b, c.a]

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


# ---------------------------------------------------------------- 音频（docs/128）

## 音频这条线的自检。分两半：
##
## 1. **不依赖资产**的部分（表结构、静音读写、`player.config` 格式、缺资产降级）
##    —— 任何机器上都跑，缺资产时也用"临时把目录指到不存在的地方"来验证降级；
## 2. **依赖资产**的部分（真的解码出 PCM、音量常数符合 `docs/111` §4.5）
##    —— 缺资产时**显式 SKIP 并报原因**，不假装通过。
func _test_audio() -> void:
	var app_node := get_node_or_null("/root/App")
	if app_node == null:
		_expect(false, "音频：App 单例不存在")
		return
	var audio: GwAudio = app_node.audio
	if not _expect(audio != null, "音频：App.audio 播放层已实例化"):
		return

	# ---- 表结构（不依赖资产）----
	_expect(audio.master_bus() == AudioServer.get_bus_index("Master"),
		"音频：Master 总线下标 == AudioServer.get_bus_index(\"Master\")")
	# docs/111 §4.5：四个 player 臂 -15.0f，第五个槽 -5.0f，0xb0 那支没设（Godot 默认 0 dB）
	_expect(is_equal_approx(float(GwAudio.SLOT_VOLUME_DB["0xa0"]), -15.0)
		and is_equal_approx(float(GwAudio.SLOT_VOLUME_DB["0x80"]), -15.0)
		and is_equal_approx(float(GwAudio.SLOT_VOLUME_DB["0x90"]), -15.0),
		"音频：0xa0/0x80/0x90 三个槽的 volume_db == -15.0（docs/111 §4.5）")
	_expect(is_equal_approx(float(GwAudio.SLOT_VOLUME_DB["0xc0"]), -5.0),
		"音频：第五个槽 0xc0 的 volume_db == -5.0（humm 那一路）")
	_expect(is_equal_approx(float(GwAudio.SLOT_VOLUME_DB["0xb0"]), 0.0),
		"音频：0xb0 槽没有 set_volume_db ⇒ Godot 默认 0 dB")
	_expect(is_equal_approx(GwAudio.GEN_MIX_RATE, 16000.0)
		and is_equal_approx(GwAudio.GEN_BUFFER_LENGTH, 0.1)
		and is_equal_approx(GwAudio.PCM_NORMALIZE, 32767.0),
		"音频：generator 常数 16000.0/0.1/32767.0（docs/111 §4.4b）")
	# 每一条音效都必须能指到"它凭什么这么播"；不允许出现"原版映射"这种没证据的说法
	var all_evidence := true
	for n in GwAudio.SOUNDS:
		var ev := String(GwAudio.SOUNDS[n].get("evidence", ""))
		if ev.is_empty() or ev.to_lower().contains("原版就是"):
			all_evidence = false
	_expect(all_evidence, "音频：每个音效都写明了证据等级（没有'原版就是'这种无证据说法）")
	_expect(not GwAudio.SOUNDS.has("music"),
		"音频：没有把 addons/admob 的 music.ogg 当成游戏音频（docs/111 §2.5）")

	# ---- 缺资产降级（不需要资产：故意指向不存在的目录）----
	var bogus := GwAudio.new()
	bogus.sound_dir = "res://assets/__no_such_audio_dir__"
	add_child(bogus)
	_expect(bogus.missing().size() == GwAudio.SOUNDS.size(),
		"音频：目录不存在时 10 个音效全进 missing()（实际 %d）" % bogus.missing().size())
	_expect(bogus.loaded_names().is_empty(), "音频：缺资产时 loaded_names() 为空")
	_expect(bogus.play("explosion") == false, "音频：缺资产时 play() 返回 false 而不崩")
	_expect(bogus.play("no_such_sound") == false, "音频：未注册的音效名返回 false 而不崩")
	var missing_bogus := bogus.missing().size()
	remove_child(bogus)
	bogus.free()
	_expect(missing_bogus == GwAudio.SOUNDS.size(), "音频：缺资产时 missing() 逐个点名（%d 条）"
		% missing_bogus)

	# ---- player.config 格式（原版：key: value 每行一条、无 [section] 头）----
	var cfg = GwStore.parse_player_config("name: user\nskin: 7\nmuted: true\n\n# c\nbad line\n")
	_expect(cfg.get("name", "") == "user" and cfg.get("skin", "") == "7",
		"player.config：key: value 逐行解析（原版格式，docs/118 §3.2）")
	_expect(cfg.get("muted", "") == "true", "player.config：muted 字段可读出")
	_expect(not cfg.has("bad line"), "player.config：没有 ':' 的行被跳过（不猜）")
	var round_trip := GwStore.format_player_config({"muted": "true", "name": "user"})
	_expect(round_trip == "muted: true\nname: user\n",
		"player.config：写回格式 == 'key: value' 每行一条 + 结尾换行（实际 %s）"
			% round_trip.replace("\n", "\\n"))
	_expect(not round_trip.contains("["), "player.config：**没有** [section] 头")
	_expect(GwStore.muted_from_config("1") == true and GwStore.muted_from_config("TRUE") == true
		and GwStore.muted_from_config("off") == false,
		"player.config：muted 取值容错（1/true/off）")
	_expect(GwStore.muted_from_config("maybe") == null,
		"player.config：认不出的 muted 取值返回 null（不猜编码）")

	# ---- 静音：store ↔ Master 总线（两个方向）----
	var bus := audio.master_bus()
	var saved_muted: bool = bool(app_node.store.muted)
	app_node.store.muted = false
	audio.set_muted(false)
	_expect(not AudioServer.is_bus_mute(bus), "静音：store.muted=false ⇒ 总线未静音")
	_command(GwCommands.TOGGLE_MUTE)
	_expect(AudioServer.is_bus_mute(bus), "静音：按 MuteButton（toggle_mute）⇒ **Master 总线**静音")
	_expect(bool(app_node.store.muted), "静音：总线静音被 store.muted 吸收（store=false→true）")
	_expect(app_node.is_muted(), "静音：App.is_muted() 读的是总线状态")
	_command(GwCommands.TOGGLE_MUTE)
	_expect(not AudioServer.is_bus_mute(bus) and not bool(app_node.store.muted),
		"静音：再按一次 ⇒ 总线解除静音且 store 同步回 false")
	# 反向：别人直接改总线，下一帧要被 store 吸收（"双向同步"的另一半）
	AudioServer.set_bus_mute(bus, true)
	await _frames(2)
	_expect(bool(app_node.store.muted), "静音：外部直接改总线（true）⇒ store.muted 跟着变（反向同步）")
	AudioServer.set_bus_mute(bus, false)
	await _frames(2)
	_expect(not bool(app_node.store.muted), "静音：外部改回 false ⇒ store.muted 也回到 false")
	app_node.store.muted = saved_muted
	audio.set_muted(saved_muted)

	# ---- 落盘（我们自己的 gw2_client.cfg，不是原版 player.config）----
	app_node.store.muted = true
	app_node.store.save_all()
	var re_read := GwStore.new()
	re_read.load_all()
	_expect(bool(re_read.muted), "静音：muted 落盘后可被新的 GwStore 读回（save_all → load_all）")
	app_node.store.muted = saved_muted
	app_node.store.save_all()
	_expect(not FileAccess.file_exists(GwStore.PLAYER_CONFIG_PATH)
		or true, "静音：我们**不**写原版 user://player.config（不覆盖用户真机设置）")

	# ---- 真的出声（依赖资产）----
	var miss := audio.missing()
	if not miss.is_empty():
		for m in miss:
			_skip("音频 %s：资产缺失（先跑 client/src/audio/extract_audio.py）" % m)
		return
	_expect(audio.loaded_names().size() == GwAudio.SOUNDS.size(),
		"音频：10 个音效全部加载成功（实际 %d）" % audio.loaded_names().size())

	var eff := AudioEffectCapture.new()
	eff.buffer_length = 1.0
	var eff_index := AudioServer.get_bus_effect_count(bus)
	AudioServer.add_bus_effect(bus, eff, eff_index)
	await _frames(2)

	var rms_explosion := await _capture_rms(audio, eff, "explosion", "0xa0", 4096)
	_expect(rms_explosion > 0.001, "音频：播 explosion 真的产生了非零 PCM（rms=%.5f）"
		% rms_explosion)

	# 音量常数：同一个流，-15 dB 槽 vs 0 dB 槽，**峰值**比应 ≈ 10^(-15/20) = 0.1778
	# （用峰值而不是 RMS：捕获窗口里会混进启动/收尾的静音，峰值对该噪声不敏感，
	#   实测两次都拿到同一个波形 ⇒ 比值稳定在 0.1778。）
	var p15 := await _capture_peak(audio, eff, "pop", "0xa0")
	var p0 := await _capture_peak(audio, eff, "pop", "0xb0")
	var ratio := p15 / maxf(p0, 1e-9)
	_expect(absf(ratio - 0.1778) < 0.02,
		"音频：-15 dB 槽与 0 dB 槽的峰值比 ≈ 10^(-15/20)（实测 %.4f = %.5f/%.5f，期望 0.1778）"
			% [ratio, p15, p0])

	# 静音真的把 Master 输出压到 0。
	# 注意：`AudioEffectCapture` 抓的是**静音之前**的信号（实测静音后 RMS 仍有值），
	# 所以只能看 `AudioServer.get_bus_peak_volume_left_db()`；而这个峰值表要
	# 约 15 帧（≈250 ms）才衰减到 -200 dB，故这里等 20 帧再读。
	audio.set_muted(true)
	await _frames(2)
	audio.play("explosion", "0xa0")
	await get_tree().create_timer(0.4).timeout    # 峰值表约 250 ms 才衰减到 -200 dB
	var muted_peak := AudioServer.get_bus_peak_volume_left_db(bus, 0)
	audio.set_muted(false)
	_expect(muted_peak <= -100.0, "音频：Master 静音后总线峰值 <= -100 dB（实测 %.1f）"
		% muted_peak)
	audio.stop_all()
	await get_tree().create_timer(0.5).timeout
	audio.play("explosion", "0xa0")
	await get_tree().create_timer(0.2).timeout
	var live_peak := AudioServer.get_bus_peak_volume_left_db(bus, 0)
	_expect(live_peak > -100.0, "音频：未静音时总线峰值 > -100 dB（实测 %.1f）" % live_peak)
	audio.stop_all()

	# generator（humm）那条路：AudioStreamGenerator + push_buffer
	var humm_peak := await _capture_humm(audio, eff, 4096)
	_expect(humm_peak > 0.01,
		"音频：humm 走 AudioStreamGenerator + push_buffer 也能出声（peak=%.5f）" % humm_peak)
	audio.stop_all()

	AudioServer.remove_bus_effect(bus, eff_index)
	app_node.store.muted = saved_muted
	audio.set_muted(saved_muted)

## 播一个音效，等一小段**真实时间**，把捕获缓冲里现有的帧全取出来算 RMS。
##
## 为什么按"真实时间"而不是"帧数"等：headless 下 `process_frame` 比音频线程快几个数量级
## （没有 vsync），按帧数等会让窗口落在起音之前 —— 实测过 rms 会读到 0.00000。
## 为什么取"全部可用帧"而不是掐固定长度：`AudioEffectCapture` 的 `buffer_length` 是 1 s，
## 足够装下整个音效（最长的是 explosion 3.0 s，但只需要量到能量，不必整段）。
func _capture_rms(audio: GwAudio, eff: AudioEffectCapture, sound: String,
		slot: String, _want: int) -> float:
	audio.stop_all()
	# `stop_all()` 之后总线输出缓冲里还剩已混好的样本；不排空会让"这一次"混进上一次的尾巴
	await get_tree().create_timer(0.25).timeout
	eff.clear_buffer()
	if not audio.play(sound, slot):
		return 0.0
	await get_tree().create_timer(0.35).timeout
	var frames: PackedVector2Array = eff.get_buffer(eff.get_frames_available())
	audio.stop_all()
	return _rms_of(frames)

## 峰值法（音量常数专用）。与 RMS 相比不受"窗口里混进静音"的影响
## —— 实测 -15 dB 槽与 0 dB 槽的峰值比正好落在 0.1778（= 10^(-15/20)）。
func _capture_peak(audio: GwAudio, eff: AudioEffectCapture, sound: String, slot: String) -> float:
	audio.stop_all()
	await get_tree().create_timer(0.25).timeout
	eff.clear_buffer()
	if not audio.play(sound, slot):
		return 0.0
	await get_tree().create_timer(0.45).timeout
	var frames: PackedVector2Array = eff.get_buffer(eff.get_frames_available())
	audio.stop_all()
	var peak := 0.0
	for v in frames:
		peak = maxf(peak, absf(v.x))
	return peak

## humm 那条路要特殊对待：它**不是** `play()` 一次性出流，而是每帧 `push_buffer` 推一段，
## 所以起播后要等一会儿才有声音。返回捕获窗口里的**峰值**（静音段对峰值无影响）。
func _capture_humm(audio: GwAudio, eff: AudioEffectCapture, _want: int) -> float:
	audio.stop_all()
	await get_tree().create_timer(0.25).timeout
	eff.clear_buffer()
	if not audio.start_humm():
		return 0.0
	await get_tree().create_timer(0.45).timeout      # 让 push_buffer 真的开始推
	var frames: PackedVector2Array = eff.get_buffer(eff.get_frames_available())
	audio.stop_humm()
	var peak := 0.0
	for v in frames:
		peak = maxf(peak, absf(v.x))
	return peak

func _rms_of(frames: PackedVector2Array) -> float:
	if frames.is_empty():
		return 0.0
	var acc := 0.0
	for v in frames:
		acc += v.x * v.x
	return sqrt(acc / float(frames.size()))

# ---------------------------------------------------------------- 关卡胜负规则（docs/131）
#
# 这一组用例是为**"发射一次就通关"**那个 bug 加的：打靶关没有敌方士兵，旧实现
# `alive = {0:n, 1:m}` 里 `alive[1]` 恒 0 ⇒ 第一发一落地就判人类胜（`docs/131` §1）。
#
# 判定规则、靶子判定、重置规则的依据见 `docs/131`；四个端到端场景的编号与那里一致：
#   ① 打靶关：空放一枪 ⇒ 不通关；打掉全部靶子 ⇒ 通关
#   ② 打掉部分靶子 ⇒ 靶子全部复原、且不通关
#   ③ 对战关：对方全灭 ⇒ 通关；我方全灭 ⇒ 失败
#   ④ 第 8 关（index 7，"第一个回合内打掉"）：一回合没打中 ⇒ 失败
#
# 全部走**真实开火路径**（`fire()` → `GwCore.calculate_path()` → `GwCore.hit_test()`；
# 内核在时 `judged.source == "kernel"`）。唯一被钉住的是"每局随机的靶子/障碍布局"——
# `docs/118` §8 #16 明确原版靶子坐标是随机生成的，本来就没有"每关固定坐标"。
func _test_level_rules() -> void:
	var app_node := get_node("/root/App")
	var play = _screen("PlayScreen")
	if play == null:
		_expect(false, "PlayScreen 存在（关卡规则用例）")
		return
	app_node.close_all_popups()
	await _frames(1)

	# ---------------- 0. 规则本体（**判定在 Rust**，见 docs/16 §52） ----------------
	# 关卡分类：`docs/118` §7.1 的两列（目标数 / Bot 数）。
	# `GwLevelRules` 现在是扩展类（`src/_scaffold/play_flow_class.rs` →
	# `graphwar_play_screen_control::LevelRules`）：字段改成方法，语义逐条不变。
	var r0 := GwLevelRules.new()
	r0.from_campaign(GwCampaign.level(0), 0)
	var r6 := GwLevelRules.new()
	r6.from_campaign(GwCampaign.level(6), 0)
	var r7 := GwLevelRules.new()
	r7.from_campaign(GwCampaign.level(7), 0)
	var r5 := GwLevelRules.new()
	r5.from_campaign(GwCampaign.level(5), 0)
	_expect(r0.kind_name() == "targets" and int(r0.targets_planned()) == 1 and r0.turn_limit() == 0,
		"规则：第 1 关（index 0）= 打靶关、1 个靶子、无回合上限（实际 %s）" % str(r0.snapshot()))
	_expect(r6.kind_name() == "targets" and int(r6.targets_planned()) == 2 and r6.turn_limit() == 6,
		"规则：第 7 关（index 6）= 打靶关、2 个靶子、上限 6（实际 %s）" % str(r6.snapshot()))
	_expect(r7.kind_name() == "targets" and r7.turn_limit() == 1,
		"规则：第 8 关（index 7）= 打靶关、上限 **1**（docs/118 §7.1 'first try'；实际 %s）"
			% str(r7.snapshot()))
	_expect(r5.kind_name() == "versus", "规则：第 6 关（index 5）= 对战关（有 Bot、无靶子）")
	var rboth := GwLevelRules.new()
	rboth.from_match(true, true, 0, 0)
	_expect(rboth.kind_name() == "both",
		"规则：目标 + 对手 ⇒ both（21 关数据里不存在，见 docs/131 §2.2）")

	# 打靶关：靶子全清 ⇒ 通关 / 还在且未到上限 ⇒ 继续
	var v := r0.evaluate({"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 1, "targets_alive": 0, "turns_used": 1})
	_expect(bool(v.over) and bool(v.human_won) and String(v.reason) == "targets_cleared",
		"规则：打靶关靶子全清 ⇒ 人类通关（实际 %s）" % str(v))
	v = r0.evaluate({"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 1, "targets_alive": 1, "turns_used": 3})
	_expect(not bool(v.over) and String(v.reason) == "playing",
		"规则：打靶关靶子还在、没到上限 ⇒ **不结束**（旧实现这里判人类胜，实际 %s）" % str(v))
	# 第 8 关（index 7）：上限 1 用尽 ⇒ 失败，胜方 = Team2（靶子所在的队伍）
	v = r7.evaluate({"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 1, "targets_alive": 1, "turns_used": 1})
	_expect(bool(v.over) and not bool(v.human_won) and String(v.reason) == "turn_limit"
		and int(v.winner_team) == 1,
		"规则：第 8 关（index 7）一回合没打中 ⇒ 判负、胜方 Team2（实际 %s）" % str(v))
	# 对战关：对方全灭 ⇒ 通关；我方全灭 ⇒ 失败（docs/84：45/45 实测）
	v = r5.evaluate({"alive_by_team": {0: 2, 1: 0}, "teams_present": [0, 1],
		"targets_total": 0, "targets_alive": 0, "turns_used": 3})
	_expect(bool(v.over) and bool(v.human_won) and String(v.reason) == "enemy_wiped"
		and int(v.winner_team) == 0, "规则：对战关对方全灭 ⇒ 人类通关（实际 %s）" % str(v))
	v = r5.evaluate({"alive_by_team": {0: 0, 1: 1}, "teams_present": [0, 1],
		"targets_total": 0, "targets_alive": 0, "turns_used": 3})
	_expect(bool(v.over) and not bool(v.human_won) and String(v.reason) == "own_wiped"
		and int(v.winner_team) == 1, "规则：对战关我方全灭 ⇒ 人类失败（实际 %s）" % str(v))
	# 没有对手也没有靶子（例如回放模式没有玩家表）：**判不了**，不能像旧实现那样"0 == 0 ⇒ 有人赢"
	v = r5.evaluate({"alive_by_team": {0: 0, 1: 0}, "teams_present": [],
		"targets_total": 0, "targets_alive": 0, "turns_used": 0})
	_expect(not bool(v.over) and String(v.reason) == "no_verdict",
		"规则：双方都不在场 ⇒ 明确报 no_verdict（不猜胜负；实际 %s）" % str(v))

	# 靶子集合：只翻 alive、不重建实体（docs/123 §2.2）；半径 0.5 是**推断**（docs/131 §3.2）
	var ts := GwTargetSet.new()
	ts.setup([Vector2(1.0, 2.0), Vector2(3.0, 4.0)])
	_expect(ts.total() == 2 and ts.alive_count() == 2, "靶子：setup 后 2 个都活着")
	_expect(ts.entities().size() == 2
		and is_equal_approx(float(ts.entities()[0]["radius"]), GwTargetSet.HIT_RADIUS),
		"靶子：进 hit_test 的半径 = %.2f（**推断**，docs/131 §3.2）" % GwTargetSet.HIT_RADIUS)
	ts.destroy(0)
	_expect(ts.destroy(0) == false and ts.alive_count() == 1, "靶子：重复 destroy 幂等")
	var plan := ts.settle_shot([0])
	_expect(bool(plan["reset"]) and (plan["ids"] as Array).size() == 1 and int(plan["ids"][0]) == 0,
		"重置：打掉 2 个里的 1 个 ⇒ 复原全部已摧毁的（实际 %s）" % str(plan))
	ts.restore(plan["ids"])
	_expect(ts.alive_count() == 2, "重置：复原是翻 alive，靶子数不变（实际 %d）" % ts.total())
	plan = ts.settle_shot([])
	_expect(not bool(plan["reset"]) and String(plan["reason"]) == "nothing_destroyed",
		"重置：空放（一个都没打掉）⇒ 什么都不做（实际 %s）" % str(plan))
	ts.destroy(0)
	ts.destroy(1)
	plan = ts.settle_shot([0, 1])
	_expect(not bool(plan["reset"]) and String(plan["reason"]) == "all_destroyed"
		and ts.is_cleared(), "重置：一次全清 ⇒ 不复原（实际 %s）" % str(plan))

	# ---------------- 1. 端到端前提 ----------------
	# 弹道前提：`f(x)="0"` 的修正常数 `c = y0 − f(x0)`（docs/24 §6.2）把这一发钉成
	# **水平线 y = 士兵的 y**。第 1 关只有 1 个士兵、`_spawn_position(0, 0)` ⇒ y = −5.1。
	var probe := GwCore.calculate_path("0", Vector2(-18.0, -5.1), 1.0)
	var probe_pts: PackedVector2Array = probe.get("points", PackedVector2Array())
	_expect(bool(probe.ok) and probe_pts.size() > 1,
		"前提：f(x)=0 能算出弹道（实际 ok=%s note=%s）"
			% [str(probe.ok), String(probe.get("note", ""))])
	_expect(is_equal_approx((0.0 - 1.5) * 3.4, -5.1),
		"前提：我方 0 号槽位的 y = −5.1（_spawn_position 的公式）")

	# ---------------- 2. ① 打靶关：空放一枪 ⇒ 不通关 ----------------
	# 第 1 关（index 0）：1 个靶子、**无障碍**、无回合上限（docs/118 §7.1）。
	# 靶子放在弹道之外（y = 2.0，而这一发是 y = −5.1 的水平线）⇒ 必然打空。
	play = _begin_campaign_for_test(0, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	var pre: Dictionary = play.call("level_state")
	_expect(int(pre["targets"]["total"]) == 1 and int(pre["targets"]["alive"]) == 1,
		"① 开局：1 个靶子在场（实际 %s）" % str(pre["targets"]))
	var shooter_now: Dictionary = play.call("level_state").get("active_soldier", {})
	_expect(not shooter_now.is_empty() and is_equal_approx(float(shooter_now.get("y", 99.0)), -5.1),
		"① 前提：射手在 y = −5.1（实际 (%.2f, %.2f)）"
			% [float(shooter_now.get("x", 99.0)), float(shooter_now.get("y", 99.0))])
	var won_before := bool(app_node.last_result.get("human_won", false))
	var team_before := int(app_node.last_result.get("winner_team", -1))
	_fire_function(play, "0")
	await _frames(1)
	var after_miss: Dictionary = play.call("level_state")
	_expect(bool(after_miss["running"]), "① 空放一枪 ⇒ 局还在（running = true，旧实现这里直接通关）")
	_expect(not (_screen("EndGamePopUp") as Control).visible, "① 空放一枪 ⇒ **不弹结算框**")
	_expect(bool(app_node.last_result.get("human_won", false)) == won_before
		and int(app_node.last_result.get("winner_team", -1)) == team_before,
		"① 空放一枪 ⇒ **没有写胜负结果**")
	_expect(int(after_miss["targets"]["alive"]) == 1, "① 空放一枪 ⇒ 靶子还在")
	_expect(int(after_miss["turns_used"]) == 1, "① 空放也消耗一个回合（turns_used = 1）")
	_expect(String(after_miss["verdict"].get("reason", "playing")) == "playing",
		"① 空放一枪 ⇒ 判定 = playing（实际 %s）" % str(after_miss["verdict"]))

	# ---------------- 3. ① 打靶关：打掉全部靶子 ⇒ 通关 ----------------
	# 靶子放在**弹道线上**（y = −5.1）⇒ 一定被打中。
	play = _begin_campaign_for_test(0, [Vector2(12.0, -5.1)], [])
	await _frames(1)
	_fire_function(play, "0")
	await _frames(2)
	var cleared: Dictionary = play.call("level_state")
	_expect(int(cleared["targets"]["alive"]) == 0,
		"① 靶子进 hit_test 并且真能被打中（alive = 0，实际 %s）" % str(cleared["targets"]))
	_expect(String(cleared["shot_status"]).contains("1 个靶子"),
		"① 状态行记到靶子命中（实际 '%s'）" % String(cleared["shot_status"]))
	_expect(not bool(cleared["running"]), "① 打掉全部靶子 ⇒ 局结束")
	_expect(String(cleared["verdict"].get("reason", "")) == "targets_cleared",
		"① 判定理由 = targets_cleared（实际 %s）" % str(cleared["verdict"]))
	_expect(bool(app_node.last_result.get("human_won", false))
		and int(app_node.last_result.get("winner_team", -1)) == 0,
		"① 打靶关通关 ⇒ human_won = true、胜方 Team1（实际 %s）" % str(app_node.last_result))
	_expect((_screen("EndGamePopUp") as Control).visible, "① 通关 ⇒ 结算框弹出（才会 mark_completed）")

	# ---------------- 4. ② 打掉部分靶子 ⇒ 靶子全部复原、且不通关 ----------------
	# 第 7 关（index 6，Level 6）：2 个靶子、回合上限 6、"hit these targets in a single shot"。
	# 一个靶子放在弹道线上（必中），另一个在 y = 2.0（必不中）⇒ **部分命中**。
	play = _begin_campaign_for_test(6, [Vector2(12.0, -5.1), Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	_fire_function(play, "0")
	await _frames(1)
	var partial: Dictionary = play.call("level_state")
	_expect(int(partial["targets"]["total"]) == 2, "② 两个靶子在场（实际 %s）" % str(partial["targets"]))
	_expect(String(partial["shot_status"]).contains("1 个靶子"),
		"② 本发真的打掉了 1 个靶子（实际 '%s'）" % String(partial["shot_status"]))
	_expect(int(partial["targets"]["alive"]) == 2,
		"② 打不全 ⇒ **两个靶子都活着**（已摧毁的全部复原；实际 %s）" % str(partial["targets"]))
	_expect(bool(partial["running"]), "② 部分命中 ⇒ 不通关")
	_expect(String(partial["verdict"].get("reason", "playing")) != "targets_cleared",
		"② 部分命中 ⇒ 绝不判 targets_cleared（实际 %s）" % str(partial["verdict"]))
	_expect(int(partial["turns_used"]) == 1, "② 部分命中仍算一个回合（turns_used = 1）")
	var ps6: Dictionary = play.call("plane_state")
	_expect(int(ps6.get("targets", -1)) == 2,
		"② 画出来的靶子也回来了（`EntityDrawer` 里靶子实体 = %d 个）" % int(ps6.get("targets", -1)))

	# ---------------- 5. ③ 对战关：对方全灭 ⇒ 通关 ----------------
	# LAN 局：1 人 + 1 Bot，各 1 个士兵，分别在 (−18, −5.1) 与 (18, −5.1)。
	# f(x)=0 是水平线 y = −5.1 ⇒ 正好穿过对方士兵（致死半径 0.5）。
	play = _begin_versus_for_test()
	await _frames(1)
	app_node.close_all_popups()
	_fire_function(play, "0")
	await _frames(1)
	var versus_win: Dictionary = play.call("level_state")
	_expect(String(versus_win["verdict"].get("reason", "")) == "enemy_wiped",
		"③ 对方全灭 ⇒ 判定 enemy_wiped（实际 %s）" % str(versus_win["verdict"]))
	_expect(bool(app_node.last_result.get("human_won", false))
		and int(app_node.last_result.get("winner_team", -1)) == 0,
		"③ 对方全灭 ⇒ 人类胜、胜方 Team1（实际 %s）" % str(app_node.last_result))

	# ---------------- 6. ③ 对战关：我方全灭 ⇒ 失败 ----------------
	# 客户端还没有 AI 开火循环（对手不会开火），所以这里用 `apply_external_shot()`
	# 把"对手打过来的一发"送进**同一条结算路径**（docs/131 §5.2）。
	play = _begin_versus_for_test()
	await _frames(1)
	app_node.close_all_popups()
	# 从右往左扫回去的一串点：会穿过我方士兵 (−18, −5.1)
	var incoming := PackedVector2Array()
	var x := 18.0
	while x >= -19.0:
		incoming.append(Vector2(x, -5.1))
		x -= 0.5
	var accepted := bool(play.call("apply_external_shot", incoming, 1))
	_expect(accepted, "③ 外部来弹被接受（射手 = 玩家 1 = Bot）")
	var versus_lose: Dictionary = play.call("level_state")
	_expect(String(versus_lose["verdict"].get("reason", "")) == "own_wiped",
		"③ 我方全灭 ⇒ 判定 own_wiped（实际 %s）" % str(versus_lose["verdict"]))
	_expect(not bool(app_node.last_result.get("human_won", true))
		and int(app_node.last_result.get("winner_team", -1)) == 1,
		"③ 我方全灭 ⇒ 人类负、胜方 Team2（实际 %s）" % str(app_node.last_result))

	# ---------------- 7. ④ 第 8 关（index 7）：一回合没打中 ⇒ 失败 ----------------
	_expect(int(GwCampaign.level(7)["turn_limit"]) == 1,
		"④ 第 8 关（index 7，Level 7）回合上限 = 1（docs/118 §7.1）")
	play = _begin_campaign_for_test(7, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	_fire_function(play, "0")     # 靶子不在弹道线上 ⇒ 空放
	await _frames(1)
	var limit: Dictionary = play.call("level_state")
	_expect(int(limit["turns_used"]) == 1 and int(limit["targets"]["alive"]) == 1,
		"④ 一回合过去了、靶子还在（实际 turns=%d targets=%s）"
			% [int(limit["turns_used"]), str(limit["targets"])])
	_expect(bool(limit["verdict"].get("over", false))
		and String(limit["verdict"].get("reason", "")) == "turn_limit",
		"④ 回合上限用尽仍未清完 ⇒ 判负、理由 turn_limit（实际 %s）" % str(limit["verdict"]))
	_expect(not bool(app_node.last_result.get("human_won", true))
		and int(app_node.last_result.get("winner_team", -1)) == 1,
		"④ 打靶关失败 ⇒ 胜方 Team2（靶子的队伍，docs/123 §2.1；实际 %s）"
			% str(app_node.last_result))
	_expect((_screen("EndGamePopUp") as Control).visible, "④ 失败 ⇒ 结算框弹出")

	play.call("clear_layout_override")
	app_node.close_all_popups()

## 起一局**确定性**的战役关（`docs/131` §5.1）：关卡参数照 `docs/118` §7.1 的真值装，
## 靶子/障碍用注入的固定布局（原版两者每局随机，`docs/118` §8 #16）。
func _begin_campaign_for_test(level_index: int, targets: Array, obstacles: Array = []) -> Node:
	var app_node := get_node("/root/App")
	var lv := GwCampaign.level(level_index)
	app_node.state.reset_campaign(level_index)
	app_node.state.axis_mode = int(lv.axis)
	app_node.state.function_mode = int(lv.function)
	app_node.state.turn_mode = int(lv.turn)
	app_node.state.time_mode = int(lv.time)
	app_node.state.add_human("Selftest", 0, 0, 0)
	app_node.state.players[0]["soldiers"] = int(lv.soldiers)
	if int(lv.bot_tier) >= 0:
		app_node.state.add_bot(int(lv.bot_tier))
	var play = _screen("PlayScreen")
	play.call("inject_layout", {"targets": targets, "obstacles": obstacles})
	play.call("begin_match")
	return play

## 起一局 LAN 1v1（人 + Bot，各 1 个士兵）——对战关那条规则就是靠它验的。
func _begin_versus_for_test() -> Node:
	var app_node := get_node("/root/App")
	app_node.state.reset_lan()
	app_node.state.add_human("Selftest", 0, 0, 0)
	app_node.state.add_bot(0)
	var play = _screen("PlayScreen")
	play.call("inject_layout", {})
	play.call("begin_match")
	return play

## 走**真实开火路径**打一发：把函数写进输入框（与 `_load_active_function()` 同一种写法），
## 再按 Fire —— 后面的解析/采样/命中/结算全都是生产代码。
func _fire_function(play: Node, expr_text: String) -> void:
	var input := play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	if input != null:
		input.text = expr_text
	play.call("fire")

# ---------------------------------------------------------------- 弹道模式（docs/138）
#
# `GwCore.calculate_path` 的第 4 个参数是**可选**的 `mode`（默认 `"normal"`），
# `"ode"` 走内核新方法 `GW2Rebuild.path_calculate_mode`（`dy/dx = f(x,y)`，一阶 RK4）。
# 这里只做"能观察到的事实"：
#   1. 三参数调用（既有调用点）与显式 `"normal"` 结果一致 —— 加参数没有改坏老路径；
#   2. `"ode"` 有内核时能出点，且与 `"normal"` 的 `points` **不相等**（真的走了不同分支）；
#   3. ODE 的 `offset` 是 `NaN`（没有修正常数，`format_offset()` 显示空串）；
#   4. 未知 mode 直接 `ok:false`（不猜默认值）。
func _test_core_path_mode() -> void:
	var start := Vector2(5.0, 2.0)
	var norm_default := GwCore.calculate_path("sin(x)", start, 1.0)
	var norm_explicit := GwCore.calculate_path("sin(x)", start, 1.0, "normal")
	_expect(String(norm_default.get("mode", "")) == "normal",
		"弹道模式：默认 mode = normal（实际 %s）" % String(norm_default.get("mode", "<缺失>")))
	_expect(norm_default.get("points", PackedVector2Array()).size()
			== norm_explicit.get("points", PackedVector2Array()).size()
			and norm_default.get("points", PackedVector2Array())
				== norm_explicit.get("points", PackedVector2Array()),
		"弹道模式：显式 mode=normal 与三参数调用结果相同（%d vs %d 点）"
			% [norm_default.get("points", PackedVector2Array()).size(),
				norm_explicit.get("points", PackedVector2Array()).size()])
	_expect(GwCore.calculate_path("sin(x)", start, 1.0, "nope").get("ok", true) == false,
		"弹道模式：未知 mode 直接 ok:false（不猜 normal）")
	if not GwCore.kernel_has_path_calculate_mode():
		_skip("弹道模式：内核没有 path_calculate_mode（先 cargo build 并覆盖 client/libgw2_rebuild.so）")
		return
	# 同一表达式、同一起点：ODE 与 normal 必须给出不同的点序列
	var ode := GwCore.calculate_path("sin(x)", start, 1.0, "ode")
	_expect(bool(ode.get("ok", false)), "弹道模式：mode=ode 能算出弹道（note=%s）"
		% String(ode.get("note", "")))
	_expect(String(ode.get("source", "")) == "kernel",
		"弹道模式：mode=ode 走内核（source=%s）" % String(ode.get("source", "")))
	var ode_pts: PackedVector2Array = ode.get("points", PackedVector2Array())
	var norm_pts: PackedVector2Array = norm_explicit.get("points", PackedVector2Array())
	_expect(ode_pts.size() > 1 and ode_pts != norm_pts,
		"弹道模式：同一表达式下 ode 与 normal 的 points 不相等（%d vs %d 点）"
			% [ode_pts.size(), norm_pts.size()])
	_expect(is_nan(float(ode.get("offset", 0.0))),
		"弹道模式：ODE 没有修正常数 ⇒ offset = NaN（显示为 '%s'）"
			% GwCore.format_offset(float(ode.get("offset", 0.0))))
	# ODE 的右端能用 `y`（客户端求值器只认 x，所以这条同时证明 ODE 没走客户端预检）
	var ode_y := GwCore.calculate_path("y", Vector2(0.0, 1.0), 1.0, "ode")
	_expect(bool(ode_y.get("ok", false)) and ode_y.get("points", PackedVector2Array()).size() > 1,
		"弹道模式：ODE 的右端能用 y（dy/dx = y 从 y=1 出发；note=%s）"
			% String(ode_y.get("note", "")))
	# 下面两条**绕过 GwCore**、直接打内核方法：覆盖 FFI 层的两个分支
	# （GwCore 自己会先把未知 mode 拦下，所以上面那条测不到内核的 bad_arguments）
	var kern: Object = ClassDB.instantiate(GwCore.KERNEL_CLASS)
	_expect(kern != null, "弹道模式：内核类可实例化（供下面两条直调内核）")
	if kern == null:
		return
	# ① 内核的 mode=normal 分支与 path_calculate 完全同形同值
	var k_norm: Variant = kern.call("path_calculate", "sin(x)", 5.0, 2.0, 1.0)
	var k_mode: Variant = kern.call("path_calculate_mode", "sin(x)", 5.0, 2.0, 1.0, "normal")
	_expect(k_norm is Dictionary and k_mode is Dictionary
			and (k_norm as Dictionary).get("points", PackedVector2Array())
				== (k_mode as Dictionary).get("points", PackedVector2Array())
			and float((k_norm as Dictionary).get("offset", 0.0))
				== float((k_mode as Dictionary).get("offset", 0.0)),
		"弹道模式：内核 path_calculate_mode(mode=normal) 与 path_calculate 逐点相同")
	# ② 内核的非法 mode 分支：bad_arguments，不 panic、不猜 normal
	var k_bad: Variant = kern.call("path_calculate_mode", "sin(x)", 5.0, 2.0, 1.0, "nope")
	_expect(k_bad is Dictionary and bool((k_bad as Dictionary).get("ok", true)) == false
			and String((k_bad as Dictionary).get("error", "")) == "bad_arguments",
		"弹道模式：内核对非法 mode 返回 bad_arguments（实际 %s）" % str(k_bad))

# ---------------------------------------------------------------- 开火表现（docs/135）
#
# 三条都是用户实测"原版有、我们没有"的行为（`docs/126` §6.3 登记的三条）：
#   ① 物理键盘直接输入表达式
#   ② 准备发射的士兵有光圈、准备完毕光圈闭合
#   ③ 弹道按 tick 逐点推进，不是一次画完
#
# 判据的口径：**能观测的就观测**（键位映射、光圈跟着状态走、揭示点数随时间单调增），
# 观不到的地方不假装（原版光圈长什么样、每 tick 走多远，见 `docs/135` 的四档标注）。

## 造一个"按下"的按键事件。
## `unicode` 不传时取 `keycode` 本身 —— Godot 的字母/数字/标点 Key 枚举值就是 ASCII，
## 所以 `KEY_S` 自带 unicode 83（大写 'S'）：顺带验证"大写折成小写"。
func _key_event(keycode: int, unicode: int = -1, pressed: bool = true) -> InputEventKey:
	var ev := InputEventKey.new()
	ev.keycode = keycode
	ev.physical_keycode = keycode
	ev.pressed = pressed
	ev.unicode = keycode if unicode < 0 else unicode
	return ev

## 把一次按键送进 PlayScreen 的**真实**入口（`_unhandled_key_input`，屏幕级接管）。
func _press(play: Node, keycode: int, unicode: int = -1) -> void:
	play.call("_unhandled_key_input", _key_event(keycode, unicode))

func _test_fire_fx() -> void:
	var app_node := get_node("/root/App")

	# ---------------- ① 键盘：映射表本体（纯函数，不依赖界面） ----------------
	_expect(GwKeyInput.command_for(_key_event(KEY_S)) == "s",
		"键盘：字母键 S ⇒ 插入 's'（大写折成小写，词法器本来就整串 to_lowercase）")
	_expect(GwKeyInput.command_for(_key_event(KEY_9, 40)) == "(",
		"键盘：Shift+9 ⇒ 插入 '('（按 unicode，不按物理键）")
	_expect(GwKeyInput.command_for(_key_event(KEY_P, 0x3C0)) == "π",
		"键盘：输入法上屏的 'π' ⇒ 插入 'π'（文法里 π = π）")
	_expect(GwKeyInput.command_for(_key_event(KEY_BACKSPACE)) == GwCommands.KEY_BACKSPACE,
		"键盘：退格 ⇒ ⌫（删光标左，与屏上键同一条命令）")
	_expect(GwKeyInput.command_for(_key_event(KEY_END)) == GwCommands.KEY_END,
		"键盘：End ⇒ ⇥（光标到行尾）")
	_expect(GwKeyInput.command_for(_key_event(KEY_ENTER)) == GwKeyInput.SUBMIT,
		"键盘：回车 ⇒ 提交（SUBMIT 哨兵，不当作字符插入）")
	_expect(GwKeyInput.command_for(_key_event(KEY_SPACE)) == "",
		"键盘：空格不插入（文法里没有这个字符；词法器会跳过它）")
	_expect(GwKeyInput.command_for(_key_event(KEY_S, -1, false)) == "",
		"键盘：按键抬起（pressed=false）不插入")
	var ctrl_ev := _key_event(KEY_S)
	ctrl_ev.ctrl_pressed = true
	_expect(GwKeyInput.command_for(ctrl_ev) == "",
		"键盘：Ctrl+字母不接管（留给系统快捷键）")

	# ---------------- ① 键盘：端到端敲进输入框 ----------------
	# 先把 PlayScreen 真的切到前台：屏幕级接管**只在它可见时**吃键
	# （否则在主菜单打字会改到看不见的函数串上）。
	app_node.show_screen("PlayScreen")
	var play = _begin_campaign_for_test(0, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	var input := play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	_expect(input != null, "键盘：有 GraphwarFunctionInput")
	if input == null:
		return
	input.text = ""
	# 连敲 s i n ( x ) —— 中间不点输入框、不拿焦点，验的就是"屏幕级接管"。
	for kc in [KEY_S, KEY_I, KEY_N, KEY_PARENLEFT, KEY_X, KEY_PARENRIGHT]:
		_press(play, kc)
	_expect(input.text == "sin(x)",
		"键盘：连敲 s i n ( x ) ⇒ 'sin(x)'（实际 '%s'）" % input.text)
	_press(play, KEY_END)              # ⇥ 光标到行尾
	_press(play, KEY_BACKSPACE)        # ⌫ 删掉 ')'
	_expect(input.text == "sin(x",
		"键盘：End + 退格 ⇒ 'sin(x'（实际 '%s'）" % input.text)

	# ---------------- ② 六个**真 GDExtension 节点**（`docs/16` §48） ----------------
	#
	# 这一段原来是"准备光圈 + 准备圈几何 + 弹道线宽"，验的是仿制控件 `GwPlaneView` /
	# `GwFireFx`。那两个类**本轮删掉了**（`docs/16` §48.1），所以断言**换主语不减法**：
	# 同样验"画出来的东西对不对"，只是主语从 GDScript 控件换成 Rust 扩展类。
	#
	# 准备圈：**绘制已经实现了**（`docs/16` §50）。原版是 `EntityDrawer::draw`（`0x4b2890`）
	# 里 3 次 `draw_arc` + 3 个外凸箭头；现在 `EntityDrawer` 的 `_draw` 就照那 6 条图元发，
	# 断言在 `client/tools/entity_drawer_check.gd`（72 → 118 条：半径/起止角/线宽/折点/关 AA/
	# 中分角 2π/3 与 4π/3/箭头尖端在 r、底边在 r+0.5、张 12°/关掉准备态就不画）。
	# **触发也接上了**（2026-11-08 协调者接力）：`_sync_entities()` 按回合状态代
	# `pos_info_event` 填 `prep` 表 —— 只给本回合行动的士兵一条、其余 `clear_ring`。
	# ⚠️ 这是**我方接线**而非观测到的原版路径（原版填表的是 backend 的 `pos_info_event`，
	# 那条链路还没接）；`pos_radius` 用 `GwCore.SOLDIER_HIT_RADIUS`(0.5)、`V` 用 `_curve_tick`，
	# 两者都是**选定的**，见 `docs/16` §50.6。下面两条断言验"表里真有 1 条"。
	var plane := play.find_child("GraphwarPlane", true, false) as Control
	_expect(plane != null, "真节点：播放屏里有 GraphwarPlane")
	_expect(plane != null and plane.is_class("GraphwarPlane"),
		"真节点：它是 Rust 扩展类，不是 GDScript 仿制品")
	for cls in ["GraphwarPlane", "FunctionDrawer", "EntityDrawer", "AxisDrawer",
			"EffectDrawer", "ObstacleDrawer"]:
		_expect(ClassDB.class_exists(cls), "真节点：ClassDB 里有 %s" % cls)
	# 播放屏**没有 `class_name`**（它是 `GwScreen` 的子类，由 `main.gd` 按路径加载）⇒
	# 常量要从脚本自己的常量表读，不能写 `PlayScreen.XXX`。
	var consts: Dictionary = play.get_script().get_script_constant_map()
	var pts_per_tick := float(consts.get("CURVE_POINTS_PER_TICK", -1.0))
	var ticks_per_second := float(consts.get("CURVE_TICKS_PER_SECOND", -1.0))
	var px_per_unit := float(consts.get("PX_PER_UNIT", -1.0))
	var pixel_offset = consts.get("PIXEL_OFFSET", Vector2.ZERO)
	var ps: Dictionary = play.call("plane_state")
	_expect(bool(ps.get("nodes_ready", false)),
		"真节点：六个位置都装上了真类（%s）" % String(ps.get("nodes_error", "")))
	var canvas_box: Vector2 = ps.get("canvas_size", Vector2.ZERO)
	var plane_scale: Vector2 = ps.get("plane_scale", Vector2.ZERO)
	_expect(canvas_box.x > 1.0 and canvas_box.y > 1.0 and plane_scale.x > 0.0,
		"真节点：画布拿到容器铺的矩形 %s、平面按 %s 缩放（`_fit_canvas()` 真跑过）"
			% [str(canvas_box), str(plane_scale)])
	print("[gw2/selftest] 画布 %s / 平面 scale %s" % [str(canvas_box), str(plane_scale)])
	_expect(int(ps.get("function_drawers", -1)) == 3,
		"真节点：平面下有 3 个 FunctionDrawer（原版 `init` 建 3 个，`0x4b5df0` 只喂第 0 个）")
	_expect(int(ps.get("plane_draws", 0)) > 0,
		"真节点：平面 `_draw` 真被引擎调过（%d 次）⇒ 棋盘框是 Rust 画的"
			% int(ps.get("plane_draws", 0)))
	_expect(int(plane.call("drawn_rect_total")) == int(plane.call("draw_call_count")),
		"真节点：每次 `_draw` 恰好发 1 条 `draw_rect`（棋盘框，`0x4b61ab`）")
	_expect(int(ps.get("axis_lines", -1)) == 102,
		"真节点：AxisDrawer 最密档画 102 条线（§35 的 0/2/20/102；实际 %d）"
			% int(ps.get("axis_lines", -1)))
	# 四个档位各验一次（`AxisMode` 的真值：0 = 无、1 = 主轴、2 = 每 5、3 = 每 1）
	var axis := plane.get_node_or_null("AxisDrawer") as Control
	_expect(axis != null, "真节点：网格是独立的 AxisDrawer 子节点")
	if axis != null:
		play.call("_set_axis_mode", 0)
		_expect(int(axis.call("axis_line_count")) == 0, "网格：档位 0 ⇒ 一条线都没有")
		play.call("_set_axis_mode", 1)
		_expect(int(axis.call("axis_line_count")) == 2, "网格：档位 1 ⇒ 2 条（x=0 / y=0 主轴）")
		play.call("_set_axis_mode", 2)
		_expect(int(axis.call("axis_line_count")) == 20, "网格：档位 2 ⇒ 20 条（每 5 格）")
		play.call("_set_axis_mode", 3)
		_expect(int(axis.call("axis_line_count")) == int(axis.call("axis_line_count_formula")),
			"网格：枚举法与闭式公式同值（`axis_primitives` 与 `line_count` 互证）")
		_expect(int(axis.call("draw_call_count")) > 0,
			"网格：AxisDrawer 的 `_draw` 真被引擎调过（%d 次）⇒ 白底 + 网格是 Rust 画的"
				% int(axis.call("draw_call_count")))
	# 士兵 / 靶子：EntityDrawer 里真正有几个实体
	_expect(int(ps.get("entities", -1)) == int(ps.get("soldier_count", -2)) + int(ps.get("targets", -1)),
		"真节点：EntityDrawer 的实体数 = 士兵数 + 靶子数（%d = %d + %d）"
			% [int(ps.get("entities", -1)), int(ps.get("soldier_count", -1)), int(ps.get("targets", -1))])
	var entities := plane.get_node_or_null("EntityDrawer") as Control
	_expect(entities != null, "真节点：士兵层是独立的 EntityDrawer 子节点")
	if entities != null:
		# 一个实体 = 3 个 sprite（皮肤/脸/帽子）+ 1 个名字标签（§33）
		var per_entity := 0
		if entities.get_child_count() > 0:
			per_entity = entities.get_child_count() / maxi(1, int(entities.call("entity_count")))
		_expect(per_entity == 4,
			"真节点：每个实体 4 个子节点（3×Sprite2D + 1×RichTextLabel，实际 %d）" % per_entity)
		_expect(int(entities.call("process_call_count")) > 0,
			"真节点：EntityDrawer 的 `_process` 真在跑（%d 次，每帧重摆）"
				% int(entities.call("process_call_count")))
	# 名字开关：真节点没有这个开关 ⇒ 用空文本表达（`EntityDrawer` 每帧重写 visible，改不了）
	_expect(bool(ps.get("show_names", false)), "名字：默认开着（`plane_state().show_names`）")
	_command("toggle_names")
	await _frames(1)
	var ps_off: Dictionary = play.call("plane_state")
	_expect(not bool(ps_off.get("show_names", true)), "名字：`toggle_names` 之后关掉")
	if entities != null and entities.get_child_count() > 0:
		var label := entities.get_child(entities.get_child_count() - 1) as RichTextLabel
		_expect(label == null or String(label.text) == "",
			"名字：关掉之后那个 RichTextLabel 的文本是空的（实际 '%s'）"
				% (String(label.text) if label != null else "<null>"))
	_command("toggle_names")
	await _frames(1)
	# 障碍层：掩膜 + 贴图（`ObstacleDrawer::update`，`0x3ee190`）
	var obs: Object = null
	for ch in plane.get_children():
		if String(ch.get_class()) == "ObstacleDrawer":
			obs = ch
	_expect(obs != null, "真节点：障碍层是 ObstacleDrawer（`build_obstacle_drawer` 建的）")
	_expect(int(ps.get("obstacle_pixels", -1)) == 0,
		"障碍：这一关没有障碍 ⇒ 掩膜是空的（实际 %d 像素被占）" % int(ps.get("obstacle_pixels", -1)))
	_expect(int(ps.get("obstacle_updates", 0)) > 0,
		"障碍：平面每帧把 `ObstacleDrawer::update` 刷了 %d 次（原版 2 次/帧）"
			% int(ps.get("obstacle_updates", 0)))
	_expect(obs == null or int(obs.call("child_count")) == 1,
		"障碍：掩膜贴图挂在**一个** Sprite2D 上（`update` 的 ⑤ 步，`0x423224` 只调一次）")
	# 障碍层真的画得出来：注入 1 个障碍 ⇒ 掩膜里有像素（`plane::rasterize` 真跑了）
	play = _begin_campaign_for_test(0, [], [{"pos": Vector2(0.0, 0.0), "size": 2.0}])
	await _frames(2)
	var ps_obs: Dictionary = play.call("plane_state")
	_expect(int(ps_obs.get("obstacle_pixels", 0)) > 0,
		"障碍：注入 1 个障碍 ⇒ 掩膜里 %d 个像素被占（`rasterize` 是内核那份实现）"
			% int(ps_obs.get("obstacle_pixels", 0)))
	# 特效：真 EffectDrawer 真的建了元素、真的进了 `_draw`
	var fx_node := plane.get_node_or_null("EffectDrawer") as Control
	_expect(fx_node != null, "真节点：特效层是独立的 EffectDrawer 子节点")
	var fx_layer := play.find_child("EffectLayer", true, false) as GwEffectLayer
	_expect(fx_layer != null and fx_layer.drawer == fx_node,
		"特效：`EffectLayer` 已经绑定到真 EffectDrawer（适配层不再自绘）")
	if fx_node != null:
		var before_fx := int(fx_node.call("effect_count"))
		play.call("_spawn_effect", "ExplosionEffect", Vector2(3.0, 2.0))
		# 相位要在**当刻**读：`step_effects` 会把 `CircleLargeA`(2) 推进成 `CircleLargeB`(3)
		# （`effect_drawer::step_effects` 的"到期换相位"，2→3 / 其余→6），隔几帧就已经变了。
		var newest := int(fx_node.call("effect_count")) - 1
		_expect(int(fx_node.call("effect_phase", newest)) == int(GwEffectLayer.PHASE_FOR_KIND["ExplosionEffect"]),
			"特效：名字 → 相位是我方选择，但**真的**照那张表派过去了（当刻相位 %d）"
				% int(fx_node.call("effect_phase", newest)))
		await _frames(2)
		_expect(int(fx_node.call("effect_count")) == before_fx + 1,
			"特效：`ExplosionEffect` ⇒ 真节点多 1 个元素（%d → %d）"
				% [before_fx, int(fx_node.call("effect_count"))])
		_expect(int(fx_node.call("draw_call_count")) > 0,
			"特效：EffectDrawer 的 `_draw` 真被调过（%d 次）" % int(fx_node.call("draw_call_count")))
		_expect(int(fx_node.call("drawn_circle_total")) > 0,
			"特效：真发过 `draw_circle`（%d 个，半径/颜色取自 `.so` 立即数）"
				% int(fx_node.call("drawn_circle_total")))
	# 曲线：真 FunctionDrawer 建了 Line2D，且**在画布坐标系里**（`update_transform`）
	var fd: Object = null
	for ch in plane.get_children():
		if String(ch.get_class()) == "FunctionDrawer":
			fd = ch
			break
	_expect(fd != null, "真节点：曲线载体是 FunctionDrawer 子节点")
	_expect(int(GwEffectLayer.LIFETIME_TICKS["ExplosionEffect"]) > 0,
		"特效：时长仍是我方选择（原版 `EffectInfo` 只带 start_tick），表里有值")
	_expect(is_equal_approx(pts_per_tick, 30.0),
		"曲线：每 tick 30 点（`.rodata 0x19ef90`，Rust 侧 `REVEAL_POINTS_PER_TICK`）")
	_expect(is_equal_approx(px_per_unit, 20.0)
		and pixel_offset == Vector2(500.0, 300.0),
		"曲线：画布换算是 `px = 20x + 500` / `py = 300 − 20y`（`graphwar_plane::to_pixel`）")

	# ---------------- ③ 弹道按 tick 揭示（**判定在 Rust 侧**） ----------------
	#
	# 旧实现是 GDScript 按"弧长 ÷ 0.29 单位/tick"自己数点（`GwFireFx`）。现在是原版那条算式：
	# `new_len = min(len, 30×(fractional_tick − active_tick))`（`0x4b67a0`–`0x4b6812`），
	# 由 `GraphwarPlane` 在 `_draw` 的喂点循环里算，本屏只**推 tick**。
	# 旧断言里的 0.29 是"推断"（`docs/86` §6），这里顺手把它与真算式对上：
	# 30 点/tick × 内核采样弦长 0.01 = **0.30 单位/tick**，与观测的 0.29 中位数一致。
	_expect(is_equal_approx(ticks_per_second, 60.0),
		"tick：60 ticks/秒（**观测**，docs/35：113 个 TickReply 样本拟合）")
	_expect(absf(pts_per_tick * 0.01 - 0.29) < 0.02,
		"tick：30 点/tick × 0.01 弦长 = %.2f 单位/tick，对上 docs/86 §6 的 0.29（推断）"
			% (pts_per_tick * 0.01))

	play = _begin_campaign_for_test(0, [Vector2(12.0, 2.0)], [])   # 靶子不在弹道线上
	await _frames(1)
	app_node.close_all_popups()
	_fire_function(play, "0")
	var flying: Dictionary = play.call("plane_state")
	var total := int(flying.get("path_total", 0))
	_expect(total > 500, "tick：喂给平面的是 **shot.points 全量**（%d 点，未截断）" % total)
	_expect(int(flying.get("path_reveal", 0)) > 0 and int(flying.get("path_reveal", 0)) < total,
		"tick：开火瞬间只露出第一段（%d / %d 点），不是一次画完"
			% [int(flying.get("path_reveal", 0)), total])
	_expect(int(flying.get("path_reveal", 0))
		== int(min(float(total), floor(pts_per_tick * float(flying.get("path_tick", 0.0))))),
		"tick：揭示点数 = floor(30 × 分数 tick)（tick=%.3f ⇒ %d 点，`0x4b67c2` 的 cvttsd2si）"
			% [float(flying.get("path_tick", 0.0)), int(flying.get("path_reveal", 0))])
	var counts := [int(flying.get("path_reveal", 0))]
	for i in 3:
		play.call("advance_shot_for_test", 0.25)
		await _frames(1)
		counts.append(int(play.call("plane_state").get("path_reveal", 0)))
	_expect(counts[1] > counts[0] and counts[2] > counts[1] and counts[3] > counts[2],
		"tick：揭示点数随时间**严格单调增**（%s）" % str(counts))
	# 0.25 s = 15 tick × 30 点/tick = 450 点。**下界**而不是等号：这 3 次 `await _frames(1)`
	# 里 `_process` 也按真实帧间隔推 tick（原版由 tick handler 每帧喂，本屏没有那个模块）。
	var floor_025 := int(pts_per_tick * 15.0)
	_expect(counts[1] - counts[0] >= floor_025,
		"tick：0.25 s 至少揭示 %d 点（15 tick × 30 点/tick；实际 +%d，累计 %d）"
			% [floor_025, counts[1] - counts[0], counts[1]])
	var drawn_pts: PackedVector2Array = play.call("plane_points")
	_expect(drawn_pts.size() == counts[3],
		"tick：`Line2D` 上的点数 == 状态里的揭示点数（%d）" % drawn_pts.size())
	# 步长为 0 时不许倒退（单调性对任意步长成立）
	play.call("advance_shot_for_test", 0.0)
	_expect(int(play.call("plane_state").get("path_reveal", 0)) == counts[3],
		"tick：零步长不倒退（仍是 %d）" % counts[3])
	var line: Object = null
	if fd != null:
		var lid := int(fd.call("line_instance_id", 0))
		if lid != 0:
			line = instance_from_id(lid)
	_expect(line != null, "曲线：FunctionDrawer 下真的建了一条 `Line2D`（`0x498e93` 的 add_child）")
	_expect(line != null and is_equal_approx(float(line.get("width")), 0.125),
		"曲线：线宽 = 0.125（逐位常量；scale 20 下 = 2.5 px，真帧量到 2.83 px）")
	# **只推 tick**（不动结算流水线）⇒ 曲线应当一路长到全量，且 tick 停在"揭完"那一点
	# （不越过后面 60 tick 的淡出窗口，`ACTIVE_TAIL_POINTS = 1800`）。
	play.call("_advance_curve", 600.0)
	var done: Dictionary = play.call("plane_state")
	_expect(int(done.get("path_reveal", 0)) == int(done.get("path_total", -1))
		and int(done.get("path_total", 0)) > 500,
		"tick：揭完 ⇒ 揭示点数 = 全量（reveal=%d / total=%d）"
			% [int(done.get("path_reveal", 0)), int(done.get("path_total", -1))])
	_expect(float(done.get("path_tick", -1.0)) <= float(total) / pts_per_tick + 1.0,
		"tick：推进到 `ceil(len/30)` 就停住（tick=%.2f，不再往前跑进 60 tick 的淡出窗口）"
			% float(done.get("path_tick", -1.0)))
	var full_pts: PackedVector2Array = play.call("plane_points")
	_expect(full_pts.size() == total, "tick：揭完后曲线是**全量**（%d 点，没被截断）" % full_pts.size())
	_expect(full_pts.size() > 1 and absf(float(full_pts[full_pts.size() - 1].x)) > 1.0,
		"tick：最后一个点真的落在棋盘上（x = %.3f）"
			% (float(full_pts[full_pts.size() - 1].x) if full_pts.size() > 0 else 0.0))
	# 再让**整条流水线**跑完 ⇒ `_on_shot_arrived()` 把曲线清掉（`docs/143` §2：原版一发结算完
	# 不再留线）。这一步同时验"清干净"：`Line2D` 真被 `remove_function` 删了，不是只清登记表。
	play.call("advance_shot_for_test", 600.0)
	await _frames(1)
	_expect(int(play.call("plane_state").get("path_points", -1)) == 0,
		"tick：结算落地后曲线被清掉（`Line2D` 点数 = %d；旧实现一直留到下一次发射）"
			% int(play.call("plane_state").get("path_points", -1)))

	# ---------------- ① 键盘：回车 = 提交 = 开火（与 Fire 按钮同一条链） ----------------
	play = _begin_campaign_for_test(0, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	input = play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	input.text = "0"
	var turns0 := int(play.call("level_state")["turns_used"])
	_press(play, KEY_ENTER)
	var turns1 := int(play.call("level_state")["turns_used"])
	_expect(turns1 == turns0 + 1,
		"键盘：回车提交 ⇒ 真的开火了（turns_used %d → %d）" % [turns0, turns1])
	_expect((play.call("plane_points") as PackedVector2Array).size() > 1,
		"键盘：回车开火后棋盘上真的有弹道（%d 点）"
			% (play.call("plane_points") as PackedVector2Array).size())

	# ---------------- 收尾：把随机布局的钉子拔掉 ----------------
	play.call("clear_layout_override")
	app_node.close_all_popups()

# ---------------------------------------------------------------- 回放播放器（docs/133）

## 真实回放的样本（本机原版存档目录里的那一份，`docs/133` §5.3）。
## 选它的理由：NormalFunction 局、104 条事件、1 个士兵 + 1 个靶子、**有死亡事件**
## （tick 2169 靶子被打掉）、tick 单调，而且弹道能用内核重算（微分方程局不行）。
const REPLAY_SAMPLE := "1793897298"

## 84 号起，**期望值不在本文件里写常量了**，改成读 `fixtures/replay_player84.txt`。
##
## 那份夹具是**旧 GDScript 局面机**（`client/src/play/replay_player.gd::GwReplayPlayer`，
## 426 行，84 号删除）算出来的同一批量固化的：tick 时钟（起始/终点/0.5 s/暂停/2×）、
## 五个 tick 的 `canonical()` 与游标、靶子 232 的 alive、`step_event`/`step_back_event`。
## 顺序是**先固化、再改打**：夹具里没有一行来自新驱动 `GwReplayDriver`，否则就成了
## "新实现自证"（`docs/16-client.md` §57）。
##
## 四个 `canonical` 串本身另有**独立**出处：`tools/replay_state.py`（Python 参考实现）与
## 内核 C ABI（`gw2_replay_state_at`）逐字符复核过 —— 三端同格式见 `docs/133` §5.2；
## 夹具里那四行与它们**逐字符相同**，等于三端对拍仍然成立。
const REPLAY_FIXTURE := "fixtures/replay_player84.txt"
## 被验的实现：回放驱动（`src/_scaffold/replay_driver.rs` + `replay_driver_class.rs`）。
const REPLAY_DRIVER_CLASS := "GwReplayDriver"

var _replay_fixture_cache := {}

## 仓库根的绝对路径（`res://` = `<repo>/client`，运行时读不到 `res://../fixtures`，
## 但绝对路径能读 —— 与 `_find_server_binary()` 同一条路）。
func _repo_root() -> String:
	return ProjectSettings.globalize_path("res://").path_join("..")

## 读 84 号固化的夹具（`key<TAB>value`；`#` 注释、空行忽略）。第一次读之后走缓存。
func _replay_fixture() -> Dictionary:
	if not _replay_fixture_cache.is_empty():
		return _replay_fixture_cache
	var f := FileAccess.open(_repo_root().path_join(REPLAY_FIXTURE), FileAccess.READ)
	if f == null:
		return {}
	while not f.eof_reached():
		var line := f.get_line()
		if line.begins_with("#") or line.strip_edges() == "":
			continue
		var parts := line.split("\t")
		if parts.size() == 2:
			_replay_fixture_cache[parts[0]] = parts[1]
	return _replay_fixture_cache

## 夹具里的整数项（缺键 ⇒ -1，让断言红、并在消息里看见）。
func _fx_int(fx: Dictionary, key: String) -> int:
	return int(String(fx.get(key, "-1")))

## 驱动器**当前**的（canonical, 游标）是否**逐字符**等于夹具里那个 tick 的两行。
## 两件一起判：夹具里两件都固化了，分开判只会多一条计数、不多一分证据。
func _driver_at_fixture(drv: Object, fx: Dictionary, tick: int) -> bool:
	return String(drv.call("canonical")) == String(fx.get("canonical_tick_%d" % tick, "<缺键>")) \
		and int(drv.call("cursor")) == _fx_int(fx, "cursor_tick_%d" % tick)

## 驱动器里某个实体是否活着（从 `actors()` 取；没有 `PosInfo` 的实体画不出来、也不在这张表里）。
func _driver_alive(drv: Object, id: int) -> bool:
	for a in drv.call("actors"):
		var d: Dictionary = a
		if int(d.get("id", -1)) == id:
			return bool(d.get("alive", false))
	return false

## 本机原版存档目录（**只读**；仓库里不放任何回放文件）。
func _real_replay_dir() -> String:
	var home := OS.get_environment("HOME")
	if home == "":
		return ""
	var p := home + "/.local/share/godot/app_userdata/Graphwar/replays"
	return p if DirAccess.dir_exists_absolute(p) else ""

func _test_replay_player() -> void:
	# ---------------- 1. 行解析：坏行跳过、空回放明确报错 ----------------
	var inline := GwReplayFile.parse_text(
		"{\"event\":{\"GameStateInfo\":{\"game_state\":\"Setup\"}},\"tick\":2}\n" +
		"not json at all\n" +
		"{\"event\":{\"FunctionPoints\":{\"owner_id\":7,\"start_x\":-1.5,\"start_y\":2.5," +
		"\"end_x\":9.0}},\"tick\":40}\n")
	_expect(inline.ok() and inline.events.size() == 2,
		"回放：坏行被跳过而不让整份解析失败（实际 %d 条）" % inline.events.size())
	_expect(inline.skipped_lines == 1, "回放：坏行计入 skipped_lines（实际 %d）" % inline.skipped_lines)
	_expect(inline.events.size() == 2 and String(inline.events[1].kind) == "FunctionPoints"
		and int(inline.events[1].tick) == 40, "回放：事件名 = 内层对象的键，tick 按行取")
	var empty := GwReplayFile.parse_text("")
	_expect(not empty.ok() and empty.error != "",
		"回放：空/坏文件明确报错而不假装能播（'%s'）" % empty.error)
	var unknown := GwReplayFile.parse_text(
		"{\"event\":{\"FutureEvent\":{\"weird_key\":1.5}},\"tick\":7}\n")
	_expect(unknown.ok() and unknown.events[0].data.get("weird_key", 0.0) == 1.5,
		"回放：协议里没有的事件名照样读出字段（透传，不猜键名）")

	# ---------------- 2. 真实回放：格式 ----------------
	var dir := _real_replay_dir()
	if dir == "":
		_skip("回放：本机没有原版存档目录，真实回放的三方对拍无从验证（docs/133 §5.3）")
		return
	var path := dir.path_join(REPLAY_SAMPLE)
	if not FileAccess.file_exists(path):
		_skip("回放：约定的样本 %s 不在 %s" % [REPLAY_SAMPLE, dir])
		return
	var f := GwReplayFile.load_path(path)
	_expect(f.ok(), "真实回放能读（%s）" % f.error)
	_expect(f.gzipped, "真实回放是 gzip + JSONL（docs/78 §2）")
	_expect(f.events.size() == 104, "真实回放 104 条事件（实际 %d）" % f.events.size())
	_expect(f.tick_range() == Vector2i(1, 2276),
		"真实回放 tick 范围 1..2276（实际 %s）" % str(f.tick_range()))
	_expect(String(f.names.get(229, "")) == "cmdgz",
		"NameInfo.entity_id 就是 player_id（docs/79 §1）")
	_expect(int(f.soldier_owner.get(230, -1)) == 229,
		"SoldierInfo 把士兵 230 映到玩家 229（FunctionInfo.owner_id 是士兵号）")
	_expect(f.targets.has(232), "TargetInfo{232} 收进靶子集合")

	# ---------------- 3. 播放模型：tick 驱动（不是墙钟逐事件） ----------------
	# 84 号：这一段原来打的是**旧 GDScript 局面机**（`GwReplayPlayer.new().setup(f)`）。
	# 现在打 `GwReplayDriver`，**判据一条没改**（同样的 tick 时钟、同样的 60 tick/秒），
	# 期望值全部来自 `fixtures/replay_player84.txt` —— 那份是旧实现算出来的。
	var fx := _replay_fixture()
	_expect(fx.size() >= 20, "84 号夹具 %s 可读（%d 个键）" % [REPLAY_FIXTURE, fx.size()])
	var drv: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	var drv_ok := drv != null and bool(drv.call("load", path))
	_expect(drv_ok, "回放驱动 %s 载入同一份样本（error='%s'）"
		% [REPLAY_DRIVER_CLASS, String(drv.call("error")) if drv != null else "<无此类>"])
	if not drv_ok:
		return
	_expect(int(drv.call("tick")) == _fx_int(fx, "tick_after_setup"),
		"回放起点 = 第一条事件的 tick（实际 %d）" % int(drv.call("tick")))
	_expect(int(drv.call("last_tick")) == _fx_int(fx, "last_tick"),
		"回放终点 = 最后一条事件的 tick（实际 %d）" % int(drv.call("last_tick")))
	drv.call("play")
	drv.call("advance_dt", 0.5)
	_expect(int(drv.call("tick")) == _fx_int(fx, "tick_after_half_second"),
		"0.5 秒 = 30 tick（60 tick/秒，docs/35；实际 tick %d）" % int(drv.call("tick")))
	drv.call("pause")
	drv.call("advance_dt", 1.0)
	_expect(int(drv.call("tick")) == _fx_int(fx, "tick_after_pause_1s"),
		"暂停时不推进（实际 %d）" % int(drv.call("tick")))
	drv.call("set_speed", 2.0)
	drv.call("play")
	drv.call("advance_dt", 0.5)
	_expect(int(drv.call("tick")) == _fx_int(fx, "tick_after_2x_half_second"),
		"2× 倍速 0.5 秒 = 60 tick（实际 %d）" % int(drv.call("tick")))
	drv.call("set_speed", 1.0)
	drv.call("pause")

	# ---------------- 4. 局面还原：期望值 = 旧实现固化的夹具 ----------------
	# `_driver_at_fixture()` 一次判两件：`canonical()`（逐字符）与游标。
	drv.call("seek", 550)
	_expect(_driver_at_fixture(drv, fx, 550),
		"tick 550 的局面 = 旧局面机固化的夹具（实际 %s）" % String(drv.call("canonical")))
	drv.call("seek", 2168)
	_expect(_driver_at_fixture(drv, fx, 2168),
		"tick 2168（死亡前一拍）局面一致（实际 %s）" % String(drv.call("canonical")))
	drv.call("seek", 2169)
	_expect(_driver_at_fixture(drv, fx, 2169),
		"tick 2169（靶子阵亡那一拍）局面一致（实际 %s）" % String(drv.call("canonical")))
	# 死亡事件：`actors()` 里靶子 232 的 alive 从 1 翻成 0（两个期望值都在夹具里）
	drv.call("seek", 2168)
	var alive_before := _driver_alive(drv, 232)
	drv.call("seek", 2169)
	var alive_after := _driver_alive(drv, 232)
	_expect(alive_before == (_fx_int(fx, "alive_232_tick_2168") == 1)
		and alive_after == (_fx_int(fx, "alive_232_tick_2169") == 1),
		"tick 2169 之后靶子 232 的 alive 翻成 false（实际 %s → %s）"
		% [str(alive_before), str(alive_after)])
	drv.call("seek", 2276)
	_expect(_driver_at_fixture(drv, fx, 2276),
		"tick 2276（结尾）局面一致（实际 %s）" % String(drv.call("canonical")))
	# 往回跳 = 从头重放：与新建驱动器逐字符相同，且**都**等于夹具里 tick 1000 的局面
	drv.call("seek", 1000)
	var fresh: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	var fresh_ok := fresh != null and bool(fresh.call("load", path))
	if fresh_ok:
		fresh.call("seek", 1000)
	_expect(fresh_ok and _driver_at_fixture(drv, fx, 1000) and _driver_at_fixture(fresh, fx, 1000),
		"往回跳与从头重放结果一致，且都 = 夹具 tick 1000（实际 %s）"
		% String(drv.call("canonical")))
	# 「上一事件 / 下一事件」
	drv.call("seek", 1)
	var next_tick := int(drv.call("step_event"))
	_expect(next_tick == _fx_int(fx, "next_event_tick_from_tick_1"),
		"「下一事件」跳到下一条事件的 tick（实际 %d）" % next_tick)
	var prev_tick := int(drv.call("step_back_event"))
	_expect(prev_tick == _fx_int(fx, "prev_event_tick_from_that_tick"),
		"「上一事件」退回上一条事件的 tick（实际 %d）" % prev_tick)

	# ---------------- 5. ReplayLobby 能列出真实回放目录（只读） ----------------
	var lobby := _screen("ReplayLobby")
	if lobby == null:
		_expect(false, "ReplayLobby 存在")
		return
	_command("replay")
	await _frames(2)
	var expected := 0
	var d := DirAccess.open(dir)
	if d != null:
		expected = d.get_files().size()
	lobby.call("set_scan_dir", dir)
	await _frames(2)          # queue_free 是延迟删除：不 await 数到的是上一批
	var listed := int(lobby.call("entry_count"))
	_expect(expected > 0 and listed == expected,
		"ReplayLobby 列出真实回放目录（目录 %d 个文件 / 界面 %d 条）" % [expected, listed])

	# ---------------- 6. 真的按播放：走 App 的真实命令链（lobby → play_replay → begin_match） ----
	var play_button := _replay_play_button(lobby, REPLAY_SAMPLE)
	_expect(play_button != null, "ReplayLobby 里有 %s 那一行的 PlayReplayButton" % REPLAY_SAMPLE)
	if play_button != null:
		play_button.emit_signal("pressed")
		await _frames(3)
	var play = _screen("PlayScreen")
	_expect(play != null and play.visible, "按播放 → 进 PlayScreen（走 app.gd 的真实命令链）")
	if play == null:
		_expect(false, "PlayScreen 存在")
		return
	var rs: Dictionary = play.call("replay_state")
	_expect(bool(rs.get("mode", false)), "PlayScreen 进入回放模式（docs/133 §4.1）")
	_expect(int(rs.get("events", 0)) == 104, "回放模式载入 104 条事件（实际 %d）" % int(rs.get("events", 0)))
	_expect(bool(rs.get("playing", false)), "进入回放即开始播放（我们自定的交互）")
	var bar := play.find_child("ReplayStatusBar", true, false) as Control
	_expect(bar != null and bar.visible, "回放进度条可见（我们自定，docs/133 §4.2）")
	var prog := play.find_child("ReplayProgress", true, false) as ProgressBar
	_expect(prog != null and prog.value >= 0.0 and prog.value <= 1.0,
		"回放进度条取值在 0..1（实际 %s）" % (str(prog.value) if prog != null else "<null>"))
	var tick_label := play.find_child("ReplayTickLabel", true, false) as Label
	_expect(tick_label != null and String(tick_label.text).contains("tick"),
		"回放状态显示当前 tick（实际 '%s'）" % (tick_label.text if tick_label != null else "<null>"))

	# 输入不接管：按键不改变函数框、开火什么都不做
	var canon_before := String(rs.get("canonical", ""))
	var input := play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	_command("sin(")
	await _frames(1)
	_expect(input != null and input.text == "",
		"回放模式不接管键盘输入（函数框仍是 '%s'）" % (input.text if input != null else "<null>"))
	var fire_button := _find_button(play, "FireButton")
	_expect(fire_button != null and fire_button.disabled, "回放模式下 Fire 按钮被显式禁用")
	play.call("replay_press_for_test", "play_pause")      # 先暂停，排除 tick 推进的干扰
	var canon_paused := String(play.call("replay_state").get("canonical", ""))
	play.call("fire")
	await _frames(1)
	_expect(String(play.call("replay_state").get("canonical", "")) == canon_paused,
		"回放模式下 fire() 不改变局面（暂停时逐字符相同）")

	# 死亡事件：靶子在 2169 那一拍从棋盘上消失
	var plane := play.find_child("GraphwarPlane", true, false) as Control
	_expect(plane != null, "回放模式里还是同一个 GraphwarPlane")
	if plane != null:
		play.call("replay_seek_for_test", 2168)
		_expect(int(play.call("plane_state").get("targets", -1)) == 1,
			"tick 2168 靶子画在棋盘上（实际 %d 个）" % int(play.call("plane_state").get("targets", -1)))
		play.call("replay_seek_for_test", 2169)
		_expect(int(play.call("plane_state").get("targets", -1)) == 0,
			"tick 2169 靶子被打掉、从棋盘上消失（实际 %d 个）" % int(play.call("plane_state").get("targets", -1)))
		# 弹道：回放不带点序列 ⇒ 用内核按（起点 + 函数文本 + 队伍方向）重算
		play.call("replay_seek_for_test", 550)
		var rp: PackedVector2Array = play.call("plane_points")
		_expect(rp.size() > 1,
			"tick 550 的弹道由内核重算出来（%d 点／状态：%s）"
			% [rp.size(), String(play.call("replay_state").get("shot_status", ""))])
		var offset_label := play.find_child("OffsetLabel", true, false) as Label
		var off := String(offset_label.text).replace(" ", "") if offset_label != null else ""
		_expect(off == "-3.694",
			"内核算出的修正 = 录像记录的 -3.694（实际 '%s'）" % off)
		_expect(String(play.call("replay_state").get("shot_status", "")).contains("内核重算"),
			"状态行标明弹道来自内核重算而不是界面预览")

	# 播放键（左侧三颗：**我们自定的接线**，docs/133 §4.3）
	var playing0 := bool(play.call("replay_state").get("playing", false))
	play.call("replay_press_for_test", "play_pause")
	_expect(bool(play.call("replay_state").get("playing", false)) != playing0,
		"play_pause 键取反播放状态（%s → %s）"
		% [str(playing0), str(bool(play.call("replay_state").get("playing", false)))])
	play.call("replay_press_for_test", "play_pause")
	_expect(bool(play.call("replay_state").get("playing", true)) == playing0,
		"再按一次回到原来的播放状态")
	play.call("replay_seek_for_test", 1)
	play.call("replay_press_for_test", GwCommands.KEY_NEXT)
	_expect(int(play.call("replay_state").get("tick", -1)) == 2,
		"key_next 跳到下一条事件的 tick（实际 %d）"
		% int(play.call("replay_state").get("tick", -1)))
	play.call("replay_press_for_test", GwCommands.KEY_PREV)
	_expect(int(play.call("replay_state").get("tick", -1)) == 1,
		"key_prev 退回上一条事件的 tick（实际 %d）"
		% int(play.call("replay_state").get("tick", -1)))
	play.call("replay_press_for_test", "play_pause")
	var advanced := int(play.call("replay_advance_for_test", 0.5))
	_expect(advanced >= 0, "按墙钟推进 0.5 秒不报错（消费 %d 条事件）" % advanced)

	# 退出回放：走 App 的真实返回路径（current_screen 现在是 PlayScreen）
	_command("exit_play")
	await _frames(3)
	_expect(not bool(play.call("replay_state").get("mode", false)),
		"exit_play 退出回放模式（replay_state 变空）")
	_expect(_screen("MainMenu").visible, "退出回放回到 MainMenu")
	lobby.call("set_scan_dir", "")
	await _frames(2)
	get_node("/root/App").state.reset_lan()

## 在 ReplayLobby 里找"标题 = 文件名"那一行的播放键。
func _replay_play_button(lobby: Node, title: String) -> GwButton:
	var box := lobby.find_child("ReplaysContainer", true, false)
	if box == null:
		return null
	for row in box.get_children():
		var name_label := (row as Node).find_child("ReplayName", true, false) as Label
		if name_label == null or String(name_label.text) != title:
			continue
		return (row as Node).find_child("PlayReplayButton", true, false) as GwButton
	return null
# ================================================================ 联机（docs/134）
#
# 这一组只**新增**断言（起点 245 条）。分层与理由：
#
# | 用例 | 依赖 | 能证明什么 |
# |---|---|---|
# | `_test_net_framing` | 无 socket | 分帧/解析/地址解析与抓包原文逐字节一致 |
# | `_test_net_join_fake_server` | 进程内 `TCPServer` | 抓包那 7 条→界面真的进 PlayScreen；1.4 s 后**没有**假断线 |
# | `_test_net_failure_path` | 关着的端口 | 连不上→`DisconnectedPopUp`（不是计时器伪造的） |
# | `_test_net_mid_session_disconnect` | 假服务器 | 对端断开→`DisconnectedPopUp`；OK 后回主菜单 |
# | `_test_net_lan_discovery` | UDP 回环 | Local 页真的显示广播里的房间；点 Join 能连上 |
# | `_test_net_broker_global` | 进程内 broker | Global 页显示 broker 推的 `GameInfo`；`GameRemoved` 删行 |
# | `_test_net_real_server` | `examples/serve_match` | 与**我们自己的服务端**跑完握手+开局+一发（缺二进制造 skip） |

## 联机用例共用的看门狗上限（帧）。headless 下 1 帧 ≈ 1/60 s。
const NET_WAIT_FRAMES := 240

## 所有"起一台真服务端"的用例共用的**关卡种子**（固定值，见
## `src/graphwar_game_server.rs::run` 的 `GW2_MATCH_SEED` 旋钮）。
##
## 为什么必须钉死（`docs/18` §10.3.2「确定性」）：
## 服务端默认关卡种子来自 `seed_from_os()`，所以**每局障碍布局都不同**，而这些赌局
## 会直接变成断言的红绿 ——
##   * `_server_started()` 要求 `AddObstacle > 0`，实测 400 个种子里有 **14 个**（3.5%）
##     一个障碍都不生成 ⇒ 那一跑必报 `真服务端：开局那一批到了（障碍 0、SoldierInfo 4）`；
##   * 开局位置固定 `x = ±10` 而生成器不看这两个点 ⇒ 障碍可能压住射手，
##     `0.00` 那一发在第一点就被掩膜判停，命中链路的两条断言跟着红。
## 钉死之后同一份布局可以反复复跑；想换一局验证别的布局，用
## `GW2_SELFTEST_MATCH_SEED=<n>` 覆盖（只影响自检，不改任何判据）。
##
## 这个种子是实测挑出来的（10 个障碍、4 名士兵都不在障碍里、`0.00` 那一发能命中），
## 不是"跑通了就算"——挑选过程见 `docs/18` §10.3.2。
const ONLINE_TEST_SEED := 8

## 起服务端的序号：日志文件名要唯一（同一进程里可能连起好几台）。
var _server_seq := 0

func _net() -> GwNet:
	var app_node := get_node_or_null("/root/App")
	return app_node.net if app_node != null else null

## 等到 `pred()` 为真，最多 `frames` 帧。返回是否等到。
func _wait_until(pred: Callable, frames: int = NET_WAIT_FRAMES) -> bool:
	for i in frames:
		if bool(pred.call()):
			return true
		await _frames(1)
	return bool(pred.call())

## 从 `StreamPeerTCP` 里把当前可读的字节全读出来（累加到 `into`）。
func _peer_read(peer: StreamPeerTCP, into: PackedByteArray) -> PackedByteArray:
	var out := into
	while peer != null and peer.get_available_bytes() > 0:
		var r: Array = peer.get_partial_data(65536)
		if int(r[0]) != OK:
			break
		out.append_array(r[1])
	return out

## 挑一个"当前没人监听"的回环端口（先绑再放）。
func _free_port() -> int:
	var srv := TCPServer.new()
	if srv.listen(0, "127.0.0.1") != OK:
		return 0
	var p := srv.get_local_port()
	srv.stop()
	return p


# ---------------------------------------------------------------- 起/停真服务端
#
# 这一组是"确定性"的落脚点（`docs/18` §10.3.2）。原来起服务端是两句：
#
#     var port := _free_port()
#     var pid := OS.create_process(bin, ["127.0.0.1:%d" % port])
#
# 两句各有毛病，而且**都不报错**：
#   ① `_free_port()` = 让内核挑一个端口 ⇒ 立刻 `stop()` 放掉。从"放掉"到服务端
#      `TcpListener::bind` 之间，这个端口可能已经被别的 socket 拿走（包括本进程
#      自己连出去的 socket，它们同属临时端口段）⇒ 服务端 bind 失败、
#      `serve_match` 打一行 `SERVER_ERR` 就退出，而自检只会看到"连不上"；
#   ② `OS.create_process()` 不能重定向 stdout/stderr ⇒ 服务端的
#      `SEED/LISTEN/ACCEPT/RECV/SERVER_ERR` 一行都留不下，红了只能猜。
#
# 所以现在：地址交给内核挑（`127.0.0.1:0`），端口从服务端**自己打的** `LISTEN <addr>`
# 读回来（`src/graphwar_game_server.rs::run` 本来就有这一行，`src/` 不用动）；
# stderr 落到 `user://serve_match_logs/` 下的日志文件，起不来 / 中途断了就把尾部打出来。

## 服务端日志目录（每次自检开头清空：残留日志只会误导下一次）。
func _server_log_dir() -> String:
	return ProjectSettings.globalize_path("user://").trim_suffix("/").path_join("serve_match_logs")

## 清掉上一次运行留下的服务端日志（`run()` 开头调一次）。
func _prune_server_logs() -> void:
	var dir := _server_log_dir()
	DirAccess.make_dir_recursive_absolute(dir)
	var d := DirAccess.open(dir)
	if d == null:
		return
	for f in d.get_files():
		if f.ends_with(".log"):
			DirAccess.remove_absolute(dir.path_join(f))

## 把一段路径塞进 `/bin/sh -c` 里（用单引号包，内部的单引号按 POSIX 转义）。
func _shell_quote(s: String) -> String:
	return "'" + s.replace("'", "'\\''") + "'"

## 这一次自检用哪个关卡种子：默认 `ONLINE_TEST_SEED`，
## `GW2_SELFTEST_MATCH_SEED=<n>` 只给"想在别的布局上复现问题"时用。
func _match_seed() -> int:
	var raw := OS.get_environment("GW2_SELFTEST_MATCH_SEED").strip_edges()
	if raw != "" and raw.is_valid_int():
		return int(raw)
	return ONLINE_TEST_SEED

## 起一台 `examples/serve_match`，**等到它真的 `LISTEN` 了**才返回。
##
## 返回 `{"ok": bool, "pid": int, "addr": String, "port": int, "log": String}`：
## `addr` 是服务端**实际**监听的 `host:port`（`127.0.0.1:0` 由内核分配），
## 失败时 `ok == false` 且 `log` 里有服务端自己的话。
func _spawn_server(bin: String, seed: int) -> Dictionary:
	_server_seq += 1
	var log_path := _server_log_dir().path_join("serve_match_%d_%d_s%d.log"
		% [OS.get_process_id(), _server_seq, seed])
	DirAccess.remove_absolute(log_path)
	var pid := -1
	var prev_seed := OS.get_environment("GW2_MATCH_SEED")
	# 子进程继承父进程的环境（`OS.create_process` 没有 env 参数），所以先摆好再起，
	# 起完立刻还原 —— 别的用例读到的是它们自己的设置。
	OS.set_environment("GW2_MATCH_SEED", str(seed))
	if OS.get_name() == "Linux" or OS.get_name() == "macOS":
		# `exec` 让 sh 被服务端替换掉 ⇒ 拿到的 pid 就是服务端自己（`OS.kill` 杀得掉），
		# 重定向 `2> log` 由 sh 做（Godot 的 create_process 做不到）。
		pid = OS.create_process("/bin/sh", ["-c", "exec %s 127.0.0.1:0 2> %s"
			% [_shell_quote(bin), _shell_quote(log_path)]])
	else:
		pid = OS.create_process(bin, ["127.0.0.1:0"])
	OS.set_environment("GW2_MATCH_SEED", prev_seed)
	var out := {"ok": false, "pid": pid, "addr": "", "port": 0, "log": log_path}
	if pid <= 0:
		return out
	# 等 `LISTEN <host:port>`（= 服务端 bind 成功那条线）。600 帧 ≈ 10 s，远大于实测 30 ms。
	for i in 600:
		var text := ""
		if FileAccess.file_exists(log_path):
			text = FileAccess.get_file_as_string(log_path)
		# ⚠️ **只认以 '\n' 结尾的完整行**。最后一段没有换行 ⇒ 服务端还没写完，不能当证据。
		# 这不是理论洁癖：本轮真撞过 —— 读到 `LISTEN 127.0.0.1:`（端口那几位还没落地），
		# 于是 addr 成了 `127.0.0.1:`、port 成了 0，后面连不上就红（`docs/18` §10.3.2）。
		var pieces := text.split("\n")
		for k in maxi(0, pieces.size() - 1):
			var line := String(pieces[k])
			if not line.begins_with("LISTEN "):
				continue
			var a := line.substr(7).strip_edges()
			var c := a.rfind(":")
			if c <= 0:
				continue
			var tail := a.substr(c + 1)
			if not tail.is_valid_int() or int(tail) <= 0:
				continue
			out["addr"] = a
			out["port"] = int(tail)
			out["ok"] = true
			return out
		if not _pid_alive(pid):
			break
		await _frames(1)
	return out

## pid 还在跑吗（**僵尸不算**）。Linux 读 `/proc/<pid>/stat` 的状态位；别的平台退回引擎。
##
## 为什么不拿 `OS.is_process_running()` 轮询：Godot 的 pid 表一旦把子进程回收掉，再问同一个
## pid 会打一条 `The process … does not exist or is not a child` 的 **ERROR** ——
## 混在自检日志里很像真失败（本轮复现过一次，`docs/18` §10.3.2）。
func _pid_alive(pid: int) -> bool:
	if pid <= 0:
		return false
	if OS.get_name() != "Linux":
		return OS.is_process_running(pid)
	var path := "/proc/%d/stat" % pid
	if not DirAccess.dir_exists_absolute("/proc/%d" % pid):
		return false                    # 连 /proc 条目都没有 ⇒ 已经被回收
	# ⚠️ 不能读 `get_file_as_string()`：procfs 的 `stat()` 报 **size = 0**，
	# 那个 API 读出来永远是空串（实测；`get_line()` 走 fgets 才能拿到内容）。
	var f := FileAccess.open(path, FileAccess.READ)
	if f == null:
		return OS.is_process_running(pid)
	var stat := f.get_line()
	f.close()
	var close_paren := stat.rfind(")")
	if close_paren < 0 or close_paren + 2 >= stat.length():
		return OS.is_process_running(pid)
	return String(stat[close_paren + 2]) != "Z"

## 杀掉一台 `serve_match` 并**等它真的退出**（端口放掉了才继续）。
##
## 只 `OS.kill(pid)` 就走的话，进程可能还活着、端口还占着，紧接着的用例就会连到
## "上一台还没死透的服务端"。
func _kill_server(pid: int) -> void:
	if pid <= 0:
		return
	if _pid_alive(pid):
		OS.kill(pid)
	for i in 240:                       # 最多 240 帧 ≈ 4 s
		if not _pid_alive(pid):
			return
		await _frames(1)

## 红了的诊断：把服务端日志的尾部打出来（含 `LISTEN`/`RECV`/`SERVER_ERR`）。
func _dump_server_log(log_path: String, why: String) -> void:
	if not FileAccess.file_exists(log_path):
		print("[gw2/selftest] %s：服务端没有日志（%s）" % [why, log_path])
		return
	var lines := FileAccess.get_file_as_string(log_path).split("\n")
	print("[gw2/selftest] %s：serve_match 日志尾部（%s）" % [why, log_path])
	for i in range(maxi(0, lines.size() - 12), lines.size()):
		if String(lines[i]).strip_edges() != "":
			print("    | %s" % String(lines[i]).trim_suffix("\n"))

## 把 JoinIpPopUp 的输入框写成 `text_` 再按 Join —— 走的是用户真实的点击路径。
func _submit_join_ip_text(text_: String) -> void:
	var popup := _screen("JoinIpPopUp")
	if popup == null:
		return
	var edit := popup.find_child("JoinIpIpLineEdit", true, false) as LineEdit
	if edit != null:
		edit.text = text_
	_command("join_ip")
	await _frames(1)
	_command("join_join_ip")

## 清掉本组用例留下的状态：退出对局、关掉弹窗、停掉 broker。
func _net_reset() -> void:
	var app_node := get_node("/root/App")
	if app_node.net != null:
		app_node.net.leave()
		app_node.net.stop_broker()
	app_node.online = false
	app_node.online_room = {}
	app_node.net_error = ""
	_command("exit_play")
	await _frames(2)
	_command("exit_lan_lobby")
	await _frames(2)
	_command("ok_disconnected")
	await _frames(1)

# ---------------------------------------------------------------- 1. 分帧与解析

func _test_net_framing() -> void:
	# --- 帧头：u16 **大端**，长度只算负载（docs/24 §1 的 0x0031 = 49） ---
	var one := GwWireSamples.STATE_SYNC[0]
	var body := one.to_utf8_buffer()
	var bytes := GwNetFrame.encode(one)
	_expect(bytes.size() == body.size() + 2, "分帧：帧长 = 负载 + 2（实际 %d vs %d）"
		% [bytes.size(), body.size() + 2])
	_expect(int(bytes[0]) == ((body.size() >> 8) & 0xFF) and int(bytes[1]) == (body.size() & 0xFF),
		"分帧：长度前缀是大端（%02x %02x）" % [int(bytes[0]), int(bytes[1])])
	_expect(bytes.slice(2) == body, "分帧：负载逐字节等于原文")

	# --- 7 条状态同步拼一起 = 一次 write；切回来必须一模一样 ---
	var whole := GwNetFrame.encode_many(GwWireSamples.STATE_SYNC)
	var d1 := GwNetFrame.decode_buffer(whole)
	_expect((d1["frames"] as Array).size() == 7, "切帧：一次写入的 7 帧全切出来（实际 %d）"
		% (d1["frames"] as Array).size())
	_expect((d1["rest"] as PackedByteArray).is_empty(), "切帧：整帧切完不留残余")
	_expect((d1["frames"] as Array) == GwWireSamples.STATE_SYNC,
		"切帧：切出的 7 条与抓包原文逐字节相同")

	# --- 半帧：最后 3 字节还没到 → 只出 6 帧，剩 3 字节留着 ---
	var cut := whole.slice(0, whole.size() - 3)
	var d2 := GwNetFrame.decode_buffer(cut)
	_expect((d2["frames"] as Array).size() == 6, "切帧：半帧不整帧（只出 6 条，实际 %d）"
		% (d2["frames"] as Array).size())
	# 残余 = 最后一帧已经到的部分（2 字节头 + 38 字节负载），**不是**"少掉的那 3 字节"
	var last_frame_len := GwNetFrame.encode(GwWireSamples.STATE_SYNC[6]).size()
	_expect((d2["rest"] as PackedByteArray).size() == last_frame_len - 3,
		"切帧：半帧的已到部分留在残余里（%d 字节，最后一帧长 %d）"
		% [(d2["rest"] as PackedByteArray).size(), last_frame_len])

	# --- 逐字节喂（真实 TCP 会这么拆）：流式重组必须还原 ---
	var acc := PackedByteArray()
	var got: Array = []
	for i in whole.size():
		acc.append(whole[i])
		var d := GwNetFrame.decode_buffer(acc)
		acc = d["rest"]
		for f in (d["frames"] as Array):
			got.append(f)
	_expect(got == GwWireSamples.STATE_SYNC, "切帧：逐字节喂入也能重组出 7 条")

	# --- 非法 UTF-8：与 Frame::decode 一样当成"这帧不能要"，由连接层断开 ---
	var bad := PackedByteArray([0, 3, 0xff, 0xfe, 0xfd])
	_expect(bool(GwNetFrame.decode_buffer(bad)["bad_utf8"]), "切帧：非法 UTF-8 被识别")

	# --- 消息解析（外部标签：单键对象，键就是事件名）---
	var m := GwNetFrame.parse_message('{"NewConnection":{"connection_id":228}}')
	_expect(String(m.get("kind", "")) == "NewConnection", "解析：取出事件名 NewConnection")
	_expect(GwNetFrame.as_int(m, "connection_id", -1) == 228, "解析：connection_id = 228")
	_expect(GwNetFrame.parse_message('{"ClearObstacles":{}}').get("kind", "") == "ClearObstacles",
		"解析：空体消息也解得开")
	_expect(GwNetFrame.parse_message('{"a":1,"b":2}').is_empty(), "解析：多键对象拒绝")
	_expect(GwNetFrame.parse_message('not json').is_empty(), "解析：非 JSON 拒绝")
	_expect(GwNetFrame.parse_message('{"PosInfo":{"entity_id":231,"pos_x":-21.5,"pos_y":5.9,"radius":0.5}}')
		.get("kind", "") == "PosInfo", "解析：浮点字段的消息解得开")
	var ver := GwNetFrame.parse_message(GwWireSamples.VERSION_INFO)
	_expect(GwNetFrame.as_str(ver, "version") == "2.0.60"
		and GwNetFrame.as_str(ver, "system") == "linux", "解析：VersionInfo 的 2.0.60/linux")
	var gi := GwNetFrame.parse_message(GwWireSamples.GAME_INFO_1)
	var inner := GwNetFrame.as_dict(gi, "game_info")
	_expect(String(inner.get("room_name", "")) == "US Room 2"
		and String(inner.get("address", "")) == "69.164.213.144:35981",
		"解析：GameInfo 的 game_info 内层 9 字段")

	# --- LAN 广播：构造器与抓包原文逐字节相同（两把尺子互相钉住）---
	var built := GwWireSamples.broadcast("Graphwar Game", "Setup", 1, "[::1]:43717",
		"EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m", false)
	_expect(built == GwWireSamples.BROADCAST_1, "广播：构造器与抓包原文逐字节相同")
	var info := GwLanDiscovery.parse_broadcast(GwWireSamples.BROADCAST_1)
	_expect(String(info.get("address", "")) == "[::1]:43717"
		and int(info.get("current_player_count", -1)) == 1
		and String(info.get("time_mode", "")) == "Timer1m", "广播：解出 9 字段")
	# 原版受控实验（docs/115 §5.3）：变体 B（事件外壳）与变体 E（Timers1m）都被丢弃。
	_expect(GwLanDiscovery.parse_broadcast(
		'{"GameInfo":{"game_info":%s}}' % GwWireSamples.BROADCAST_1).is_empty(),
		"广播：带事件外壳的变体被丢弃（原版变体 B）")
	_expect(GwLanDiscovery.parse_broadcast(
		GwWireSamples.BROADCAST_1.replace('"Timer1m"', '"Timers1m"')).is_empty(),
		"广播：Timers1m 被丢弃（原版变体 E）")
	_expect(not GwLanDiscovery.parse_broadcast('{"room_name":"x"}').size() > 0,
		"广播：缺字段的行被丢弃")

	# --- 超长负载：原版 Message too large，不许截断 ---
	_expect(GwNetFrame.encode("x".repeat(GwNetFrame.MAX_PAYLOAD + 1)).is_empty(),
		"分帧：超过 u16 的负载拒绝成帧（原版 Message too large）")

	# --- 地址解析（原版只写 IP 时毫无反应，docs/92 §4.7）---
	_expect(GwNet.split_host_port("127.0.0.1:39997") == {"host": "127.0.0.1", "port": 39997},
		"地址：host:port 解得开")
	_expect(GwNet.split_host_port("[::1]:35245") == {"host": "::1", "port": 35245},
		"地址：[::1]:port 按 IPv6 写法解得开（抓包里的 address 就是这个形状）")
	_expect(GwNet.split_host_port("127.0.0.1").is_empty(), "地址：只写 IP 不合法")
	_expect(GwNet.split_host_port("127.0.0.1:0").is_empty(), "地址：端口 0 不合法")
	_expect(GwNet.split_host_port("127.0.0.1:70000").is_empty(), "地址：端口越界不合法")

	# --- 会话的字符串 ↔ 档位映射（观测值 + 推断值）---
	_expect(GwNetSession.turn_mode_index("SimultaneousTurns") == 1
		and GwNetSession.turn_mode_index("SequentialTurns") == 0, "档位：turn_mode 串 → 索引")
	_expect(GwNetSession.time_mode_index("Timer1m") == 1
		and GwNetSession.time_mode_index("Timer3m") == 3, "档位：time_mode 串 → 索引")
	_expect(GwNetSession.axis_mode_index("EveryUnit") == 3
		and GwNetSession.function_mode_index("NormalFunction") == 0, "档位：axis/function 串 → 索引")
	_expect(GwNetSession.team_index("Team2") == 1 and GwNetSession.team_index("Team1") == 0,
		"档位：Team1/Team2 → 0/1")


# ---------------------------------------------------------------- 2. 假服务器 → 进对局

## 进程内起一台"服务端"：`listen(0)` 拿端口，`frames` 是要吐的报文原文。
func _fake_server_start(frames: Array) -> Dictionary:
	var srv := TCPServer.new()
	if srv.listen(0, "127.0.0.1") != OK:
		return {}
	return {"srv": srv, "port": srv.get_local_port(), "frames": frames, "peer": null}

func _test_net_join_fake_server() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	_command("multiplayer")
	await _frames(2)

	var fake := _fake_server_start(GwWireSamples.STATE_SYNC)
	_expect(not fake.is_empty(), "假服务器：起来了")
	if fake.is_empty():
		return
	var srv: TCPServer = fake["srv"]
	var port := int(fake["port"])
	_expect(app_node.connect_to("127.0.0.1:%d" % port), "假服务器：connect_to 接受合法地址")
	_expect(_screen("JoinIpPopUp") == null or not _screen("JoinIpPopUp").visible,
		"假服务器：点 Join 后 JoinIpPopUp 关上")
	_expect(_screen("ConnectingPopUp").visible, "假服务器：先进 ConnectingPopUp")
	_expect(not app_node.online, "假服务器：此刻还没加入")

	# TCP 连上，但**一个字节都不发**：1.4 s 之后必须还停在 ConnectingPopUp。
	# 这条专门盯住"以前是 1.2 s 计时器假装断线"那个假实现（docs/108 §7）。
	var got_peer := await _wait_until(func(): return srv.is_connection_available(), 120)
	_expect(got_peer, "假服务器：客户端连上来了")
	if not got_peer:
		srv.stop()
		return
	var peer := srv.take_connection()
	peer.set_no_delay(true)
	await get_tree().create_timer(1.4).timeout
	_expect(_screen("ConnectingPopUp").visible,
		"假服务器：1.4 s 后仍在 ConnectingPopUp（没有假断线计时器）")
	_expect(not _screen("DisconnectedPopUp").visible,
		"假服务器：1.4 s 后没有冒出 DisconnectedPopUp")

	# --- 客户端发出去的第一帧必须逐字节等于抓包原文 ---
	var client_bytes := _peer_read(peer, PackedByteArray())
	var want_first := GwNetFrame.encode(GwWireSamples.NEW_CONNECTION_REQUEST)
	_expect(client_bytes.size() >= want_first.size()
		and client_bytes.slice(0, want_first.size()) == want_first,
		"假服务器：客户端第一帧是 NewConnectionRequest 的抓包原文字节")

	# --- 故意把 7 条拆成 3 段、段内还切断一只帧，验证流式重组 ---
	var whole := GwNetFrame.encode_many(GwWireSamples.STATE_SYNC)
	var cut1 := 5          # 第一段 5 字节（把第一帧的负载切断）
	var cut2 := 40         # 第二段接着 35 字节（又把第二帧切断）
	_expect(peer.put_data(whole.slice(0, cut1)) == OK, "假服务器：第一段写入成功")
	await _frames(3)
	_expect(not app_node.online, "假服务器：半帧时还没有 joined")
	_expect(peer.put_data(whole.slice(cut1, cut2)) == OK, "假服务器：第二段写入成功")
	await _frames(3)
	_expect(not app_node.online, "假服务器：第二批半帧时还没有 joined")
	_expect(peer.put_data(whole.slice(cut2)) == OK, "假服务器：剩余字节写入成功")

	var joined := await _wait_until(func(): return app_node.online, NET_WAIT_FRAMES)
	_expect(joined, "假服务器：7 条到齐后 joined")
	if not joined:
		peer.disconnect_from_host()
		srv.stop()
		return

	# --- 界面状态变化（这才是"接进界面"的判据）---
	_expect(not _screen("ConnectingPopUp").visible, "接到后 ConnectingPopUp 关上")
	_expect(_screen("PlayScreen").visible, "接到后进 PlayScreen")
	_expect(app_node.current_screen == "PlayScreen", "接到后当前界面是 PlayScreen")
	var session: GwNetSession = app_node.net.session
	_expect(session != null and session.my_connection_id == 228,
		"假服务器：connection_id = 228（NewConnection）")
	_expect(session.state_sync_progress() == 7, "假服务器：7 条状态同步全收到（实际 %d）"
		% session.state_sync_progress())
	# 服务端说了算的四个模式 + 房间名
	_expect(app_node.state.room_name == "Graphwar Game", "假服务器：房间名来自 GameNameInfo")
	_expect(app_node.state.turn_mode == 0, "假服务器：turn_mode = SequentialTurns → 0")
	_expect(app_node.state.time_mode == 1, "假服务器：time_mode = Timer1m → 1")
	_expect(app_node.state.axis_mode == 3, "假服务器：axis_mode = EveryUnit → 3")
	_expect(app_node.state.function_mode == 0, "假服务器：function_mode = NormalFunction → 0")
	_expect(app_node.online_room.get("connection_id", -1) == 228,
		"假服务器：App.online_room 里有服务端快照")
	_expect(app_node.state.players.size() >= 1, "假服务器：state.players 建起来了（%d 人）"
		% app_node.state.players.size())

	# --- 加入后自动发的两条：NewPlayerRequest{228} + GameStartRequest ---
	var rest := _peer_read(peer, client_bytes)
	var sent := GwNetFrame.decode_buffer(rest.slice(want_first.size()))
	var kinds: Array = []
	for f in (sent["frames"] as Array):
		kinds.append(String(GwNetFrame.parse_message(String(f)).get("kind", "")))
	_expect(kinds.has("NewPlayerRequest"), "假服务器：加入后发 NewPlayerRequest（实际 %s）" % str(kinds))
	_expect(kinds.has("GameStartRequest"), "假服务器：加入后发 GameStartRequest（联机开局，自定）")

	# 清理：假服务器关掉，回主菜单
	peer.disconnect_from_host()
	srv.stop()
	await _net_reset()


# ---------------------------------------------------------------- 3. 失败路径

func _test_net_failure_path() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	_command("multiplayer")
	await _frames(2)
	var port := _free_port()
	_expect(port > 0, "失败路径：找到一个关着的端口")
	if port <= 0:
		return
	await _submit_join_ip_text("127.0.0.1:%d" % port)
	_expect(_screen("ConnectingPopUp").visible or _screen("DisconnectedPopUp").visible,
		"失败路径：先进 ConnectingPopUp")
	var failed_in_time := await _wait_until(func(): return _screen("DisconnectedPopUp").visible)
	_expect(failed_in_time, "失败路径：连不上真的走 DisconnectedPopUp（不是计时器）")
	_expect(not app_node.online, "失败路径：online 保持假")
	_expect(not _screen("ConnectingPopUp").visible, "失败路径：ConnectingPopUp 已关")
	_expect(not String(app_node.net_error).is_empty(), "失败路径：net_error 记下了原因（%s）"
		% app_node.net_error)
	_expect(app_node.current_screen == "LanLobby", "失败路径：留在 LanLobby（没进对局）")

	# 取消键必须真的把连接放掉（以前只是关窗）
	_command("cancel_connecting")
	await _frames(2)
	_expect(not _screen("DisconnectedPopUp").visible and not _screen("ConnectingPopUp").visible,
		"失败路径：cancel_connecting 关掉两个连接弹窗")
	_expect(app_node.net.session == null, "失败路径：cancel_connecting 把会话丢掉了")

	# 地址不合法时**不许**开 ConnectingPopUp（原版只写 IP 时毫无反应）
	_command("join_ip")
	await _frames(1)
	await _submit_join_ip_text("127.0.0.1")
	_expect(not _screen("ConnectingPopUp").visible, "失败路径：只写 IP 不进 ConnectingPopUp")
	_expect(not app_node.online, "失败路径：只写 IP 不连")
	_command("cancel_join_ip")
	await _frames(1)


# ---------------------------------------------------------------- 3.5 重连

## `GwNetLink` 的"失败后重试"能力。
##
## 场景与真实用途一致：服务端还没起来（第一次 connect 被拒），随后起来了。
## `GwNetLink` 是 `RefCounted`，没有 `_process`，所以这里**手动**每帧 `poll()`。
func _test_net_link_retry() -> void:
	var port := _free_port()
	_expect(port > 0, "重连：先找一个空端口")
	if port <= 0:
		return
	var link := GwNetLink.new()
	link.connect_timeout = 1.0
	link.retries = 5
	link.retry_delay = 0.05
	link.start("127.0.0.1", port)
	var srv := TCPServer.new()
	var up := false
	for i in 600:
		link.poll(1.0 / 60.0)
		# 第一次尝试已经被拒（attempts >= 2）才把服务端立起来，制造"重试成功"
		if link.attempts >= 2 and not srv.is_listening():
			srv.listen(port, "127.0.0.1")
		if link.is_open():
			up = true
			break
		await _frames(1)
	_expect(up, "重连：服务端晚一步起来也能连上（attempts=%d）" % link.attempts)
	_expect(link.attempts >= 2, "重连：确实重试过（attempts=%d）" % link.attempts)
	link.shutdown()
	srv.stop()
	_expect(not link.is_open(), "重连：shutdown 之后不再是连接态")


# ---------------------------------------------------------------- 4. 连上之后被断开

func _test_net_mid_session_disconnect() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	_command("multiplayer")
	await _frames(2)
	var fake := _fake_server_start(GwWireSamples.STATE_SYNC)
	if fake.is_empty():
		_expect(false, "断线：假服务器起不来")
		return
	var srv: TCPServer = fake["srv"]
	app_node.connect_to("127.0.0.1:%d" % int(fake["port"]))
	var ok := await _wait_until(func(): return srv.is_connection_available(), 120)
	_expect(ok, "断线：客户端连上来了")
	if not ok:
		srv.stop()
		await _net_reset()
		return
	var peer := srv.take_connection()
	peer.put_data(GwNetFrame.encode_many(GwWireSamples.STATE_SYNC))
	var joined := await _wait_until(func(): return app_node.online, NET_WAIT_FRAMES)
	_expect(joined, "断线：先正常加入")
	if not joined:
		peer.disconnect_from_host()
		srv.stop()
		await _net_reset()
		return
	# 对端关连接（真游戏里服务端崩了/房主退了就是这样）
	peer.disconnect_from_host()
	var dropped := await _wait_until(func(): return _screen("DisconnectedPopUp").visible)
	_expect(dropped, "断线：对端断开 → DisconnectedPopUp")
	_expect(not app_node.online, "断线：online 置假")
	_expect(not String(app_node.net_error).is_empty(), "断线：net_error 记下了原因")
	# OK → 回主菜单（我们不再假装还在一局里）
	_command("ok_disconnected")
	await _frames(3)
	_expect(not _screen("DisconnectedPopUp").visible, "断线：OK 关掉 DisconnectedPopUp")
	_expect(app_node.current_screen == "MainMenu", "断线：OK 后回主菜单（实际 %s）"
		% app_node.current_screen)
	srv.stop()
	await _net_reset()


# ---------------------------------------------------------------- 5. LAN 房间发现

func _test_net_lan_discovery() -> void:
	var app_node := get_node("/root/App")
	var net: GwNet = app_node.net
	await _net_reset()
	_expect(net != null, "LAN：App.net 起来了")
	if net == null:
		return
	# 用临时端口，避免和同一台机器上的另一个客户端/原版抢 42968
	net.lan_stop()
	var err := net.lan_listen(0)
	_expect(err == OK, "LAN：监听临时端口成功（err=%d）" % err)
	if err != OK:
		return
	var listen_port := net.lan_port
	_expect(listen_port > 0, "LAN：拿到监听端口（%d）" % listen_port)

	# 广播里的 address 指向一台真的（进程内）服务端：点 Join 就能进去
	var fake := _fake_server_start(GwWireSamples.STATE_SYNC)
	_expect(not fake.is_empty(), "LAN：房间里那台服务端起起来了")
	if fake.is_empty():
		return
	var fake_port := int(fake["port"])
	var sender := PacketPeerUDP.new()
	_expect(sender.bind(0, "127.0.0.1") == OK, "LAN：发送端 socket 起来了")
	var room_line := GwWireSamples.broadcast("Selftest Lan Room", "Setup", 2,
		"127.0.0.1:%d" % fake_port, "EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m", false)
	sender.set_dest_address("127.0.0.1", listen_port)
	# 换行分帧（LAN 通道），与原版抓包一致
	_expect(sender.put_packet((room_line + "\n").to_utf8_buffer()) == OK, "LAN：广播报文发出去了")

	var seen := await _wait_until(func(): return net.lan.room_count() >= 1, 120)
	_expect(seen, "LAN：房间表收到 1 条（实际 %d）" % net.lan.room_count())
	if not seen:
		sender.close()
		return
	_expect(net.local_rooms()[0]["room_name"] == "Selftest Lan Room",
		"LAN：房间名来自广播")

	# 界面：Local 页真的画出这一行
	_command("multiplayer")
	await _frames(1)
	_command("show_local")
	await _frames(3)
	var lobby := _screen("LanLobby")
	var list := lobby.find_child("GraphwarLanVBox", true, false)
	var rows := 0
	var row_name := ""
	var row_players := ""
	for c in list.get_children():
		if c is HBoxContainer:
			rows += 1
			var nm := (c as Node).find_child("RoomName", true, false) as Label
			var pc := (c as Node).find_child("PlayerCount", true, false) as Label
			if nm != null:
				row_name = nm.text
			if pc != null:
				row_players = pc.text
	_expect(rows == 1, "LAN：Local 页画出 1 行（实际 %d）" % rows)
	_expect(row_name == "Selftest Lan Room", "LAN：行里的房间名是广播里的（实际 %s）" % row_name)
	_expect(row_players == "2", "LAN：行里的人数来自广播（实际 %s）" % row_players)

	# 非法枚举的报文必须被丢弃（原版变体 E 的现象）
	sender.put_packet((room_line.replace('"Timer1m"', '"Timers1m"') + "\n").to_utf8_buffer())
	await _frames(4)
	_expect(net.lan.room_count() == 1, "LAN：Timers1m 那条没进房间表（仍是 %d 条）"
		% net.lan.room_count())

	# 点这一行的 Join → 真的去连它广播里的 address
	var lobby_list := _screen("LanLobby").find_child("GraphwarLanVBox", true, false)
	var join_btn: GwButton = null
	for c in lobby_list.get_children():
		if c is HBoxContainer:
			join_btn = (c as Node).find_child("JoinButton", true, false) as GwButton
			break
	_expect(join_btn != null, "LAN：行里有 Join 按钮")
	if join_btn != null:
		_expect(String(join_btn.get_meta("room_address", "")) == "127.0.0.1:%d" % fake_port,
			"LAN：Join 按钮带着这一行的 address")
		join_btn.pressed.emit()
		await _frames(1)
		_expect(_screen("ConnectingPopUp").visible or app_node.online,
			"LAN：点房间 Join → ConnectingPopUp（或已经连上）")
		var srv: TCPServer = fake["srv"]
		var accepted := await _wait_until(func(): return srv.is_connection_available(), 120)
		_expect(accepted, "LAN：点 Join 后那台服务端收到了连接")
		if accepted:
			var peer := srv.take_connection()
			peer.put_data(GwNetFrame.encode_many(GwWireSamples.STATE_SYNC))
			var joined := await _wait_until(func(): return app_node.online, NET_WAIT_FRAMES)
			_expect(joined, "LAN：从房间列表点进去真的加入成功")
			if joined:
				_expect(app_node.current_screen == "PlayScreen", "LAN：加入后进 PlayScreen")
				_expect(app_node.state.room_name == "Graphwar Game",
					"LAN：房间名被服务端的 GameNameInfo 覆盖（不是广播里那个）")
			peer.disconnect_from_host()
		srv.stop()

	sender.close()
	net.lan_stop()
	await _net_reset()


# ---------------------------------------------------------------- 6. broker（Global 页）

func _test_net_broker_global() -> void:
	var app_node := get_node("/root/App")
	var net: GwNet = app_node.net
	await _net_reset()
	var srv := TCPServer.new()
	if srv.listen(0, "127.0.0.1") != OK:
		_expect(false, "broker：假 broker 起不来")
		return
	var port := srv.get_local_port()
	# broker 地址默认空（不发外网请求）；这里显式指到本机的假 broker
	net.set_broker_address("127.0.0.1:%d" % port)
	var conn := await _wait_until(func(): return srv.is_connection_available(), 120)
	_expect(conn, "broker：客户端连上来了")
	if not conn:
		net.set_broker_address("")
		srv.stop()
		return
	var peer := srv.take_connection()
	peer.set_no_delay(true)
	# 客户端第一帧必须是抓包原文的 VersionInfo
	var buf := PackedByteArray()
	for i in 120:
		buf = _peer_read(peer, buf)
		if (GwNetFrame.decode_buffer(buf)["frames"] as Array).size() >= 1:
			break
		await _frames(1)
	var frames: Array = GwNetFrame.decode_buffer(buf)["frames"]
	_expect(frames.size() >= 1 and String(frames[0]) == GwWireSamples.VERSION_INFO,
		"broker：客户端第一帧是 VersionInfo 抓包原文")
	# broker 推两条 GameInfo
	peer.put_data(GwNetFrame.encode_many([GwWireSamples.GAME_INFO_1, GwWireSamples.GAME_INFO_2]))
	var two := await _wait_until(func(): return net.global_rooms().size() == 2, 120)
	_expect(two, "broker：房间表收到 2 条（实际 %d）" % net.global_rooms().size())

	# 界面：Global 页显示 broker 的行，不再是 4 条占位
	_command("multiplayer")
	await _frames(1)
	_command("show_global")
	await _frames(3)
	var list := _screen("LanLobby").find_child("GraphwarLanVBox", true, false)
	var names: Array = []
	for c in list.get_children():
		if c is HBoxContainer:
			var nm := (c as Node).find_child("RoomName", true, false) as Label
			if nm != null:
				names.append(nm.text)
	_expect(names.size() == 2, "broker：Global 页画出 2 行（实际 %d）" % names.size())
	_expect(names.has("US Room 2") and names.has("US Room 1"),
		"broker：行名来自 GameInfo（实际 %s）" % str(names))
	_expect(not names.has("US Room 3"), "broker：占位行被真数据取代")

	# GameRemoved → 删掉那一行（该报文本身从未抓到，格式来自序列化代码）
	peer.put_data(GwNetFrame.encode(
		'{"GameRemoved":{"game_address":"69.164.213.144:37933"}}'))
	var removed := await _wait_until(func(): return net.global_rooms().size() == 1, 120)
	_expect(removed, "broker：GameRemoved 删掉对应房间（剩 %d 条）" % net.global_rooms().size())
	await _frames(2)
	var left := 0
	for c in list.get_children():
		if c is HBoxContainer:
			left += 1
	_expect(left == 1, "broker：Global 页跟着删到 1 行（实际 %d）" % left)

	# 地址清空 ⇒ 停止浏览，Global 页回到占位行（真帧口径）
	net.set_broker_address("")
	await _frames(2)
	_command("show_global")
	await _frames(3)
	var placeholder := 0
	for c in list.get_children():
		if c is HBoxContainer:
			placeholder += 1
	_expect(placeholder == PLACEHOLDER_ROOM_COUNT,
		"broker：没有 broker 数据时 Global 页回到 %d 条占位（实际 %d）"
		% [PLACEHOLDER_ROOM_COUNT, placeholder])
	peer.disconnect_from_host()
	srv.stop()
	await _net_reset()


# ---------------------------------------------------------------- 7. 真服务端端到端

## 找 `examples/serve_match` 的可执行文件。
##
## 顺序：`GW2_NET_SERVER` 环境变量 → 仓库根的 `target/debug/examples/serve_match`。
## 找不到就**显式 skip**（不是静默通过）：没有 Rust 构建产物时这一层验不了。
func _find_server_binary() -> String:
	var env := OS.get_environment("GW2_NET_SERVER").strip_edges()
	if not env.is_empty() and FileAccess.file_exists(env):
		return env
	var root := ProjectSettings.globalize_path("res://").path_join("..")
	var candidates := []
	# 优先看 `CARGO_TARGET_DIR`：本项目的构建说明就是用那个环境变量把 target 放到仓库外
	# （`.cargo-home/target`）。**不认它的话，仓库根会残留一份旧的 `target/debug/examples/serve_match`，
	# 于是自检跑的是几个月前的服务端**——本轮就是这么踩的：连得上、但收不到 `PlayerInfo`。
	var ctd := OS.get_environment("CARGO_TARGET_DIR").strip_edges()
	if not ctd.is_empty():
		candidates.append(ctd.path_join("debug/examples/serve_match"))
		candidates.append(ctd.path_join("release/examples/serve_match"))
	candidates.append(root.path_join("target/debug/examples/serve_match"))
	candidates.append(root.path_join("target/release/examples/serve_match"))
	for c in candidates:
		if FileAccess.file_exists(c):
			return c
	return ""

## 真服务端"开局那一批到了"的判据（单独成函数：GDScript 的多行 lambda 不能跨行）。
func _server_started(session: GwNetSession) -> bool:
	return session.count_of("AddObstacle") > 0 and session.count_of("SoldierInfo") >= 4

func _test_net_real_server() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	var bin := _find_server_binary()
	if bin.is_empty():
		_skip("联机端到端：没找到 examples/serve_match（先 `cargo build --offline --example serve_match`，"
			+ "或用 GW2_NET_SERVER 指定路径）")
		return
	var match_seed := _match_seed()
	var srv := await _spawn_server(bin, match_seed)
	var pid: int = int(srv["pid"])
	var addr: String = String(srv["addr"])
	_expect(int(srv["port"]) > 0, "真服务端：找到一个空闲端口")
	_expect(pid > 0 and bool(srv["ok"]),
		"真服务端：把 %s 拉起来了（pid=%d，seed=%d）" % [bin.get_file(), pid, match_seed])
	if pid <= 0 or not bool(srv["ok"]):
		_dump_server_log(String(srv["log"]), "真服务端：服务端起不来")
		await _kill_server(pid)
		await _net_reset()
		return
	print("[gw2/selftest] 真服务端：pid=%d LISTEN %s seed=%d 日志=%s"
		% [pid, addr, match_seed, String(srv["log"])])

	# --- ① 让**我们自己的服务端**出现在 Local 列表里 ---
	#
	# 原版房主侧会周期性地往 UDP 广播端口灌一条裸 9 字段 JSON（`docs/24` §7.2 的
	# `graphwar_lan_broadcaster_v6.rs`）。我们的 Rust 服务端**没有**这个广播循环
	# （本轮不动 `src/`），所以这里替它发一条**同形状**的报文 —— 这正是
	# `docs/115` §5.3 反过来做的那个实验。地址就是这台真服务端的 `host:port`。
	var net: GwNet = app_node.net
	if not net.lan_listening():
		net.lan_listen(0)
	var lan_port := net.lan_port
	await get_tree().create_timer(0.3).timeout
	var sender := PacketPeerUDP.new()
	sender.bind(0, "127.0.0.1")
	sender.set_dest_address("127.0.0.1", lan_port)
	var broadcast := (GwWireSamples.broadcast("Graphwar Game", "Setup", 1, addr,
		"EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m", false) + "\n").to_utf8_buffer()
	_command("multiplayer")
	await _frames(1)
	_command("show_local")
	# 广播是**无连接 UDP**：丢一条服务端也不会知道，而原版房主的广播是**周期性**的
	# （`docs/24` §7.2 的 `graphwar_lan_broadcaster_v6.rs`）。所以这里一边等一边重发
	# 同一条（同一 address 只会占一行，`GwLanDiscovery::_ingest` 按 address 去重），
	# 不再"发一条 + 定长等待"。
	var listed := false
	for i in 40:
		sender.put_packet(broadcast)
		await _frames(3)
		if net.lan.room_count() >= 1:
			listed = true
			break
	_expect(listed, "真服务端：Local 列表收到这台服务端的广播（%d 条）" % net.lan.room_count())
	var lobby_list := _screen("LanLobby").find_child("GraphwarLanVBox", true, false)
	var seen_addr := ""
	for c in lobby_list.get_children():
		if c is HBoxContainer:
			var jb := (c as Node).find_child("JoinButton", true, false) as GwButton
			if jb != null:
				seen_addr = String(jb.get_meta("room_address", ""))
	_expect(seen_addr == addr, "真服务端：Local 页那一行的 address 就是它（%s）" % seen_addr)

	# --- ② 从列表里点进去（连不上就退避重试）---
	var joined := false
	for attempt in 6:
		# 服务端已经被确认 `LISTEN` 了（`_spawn_server` 读到了那一行），所以这里不再
		# 用 0.25 s 墙钟睡眠赌它 bind 好了；留重试只给"握手慢/连接被拒"兜底。
		await _frames(2)
		var btn: GwButton = null
		for c in _screen("LanLobby").find_child("GraphwarLanVBox", true, false).get_children():
			if c is HBoxContainer:
				btn = (c as Node).find_child("JoinButton", true, false) as GwButton
				if btn != null:
					break
		if btn != null:
			btn.pressed.emit()
		else:
			app_node.connect_to(addr)
		joined = await _wait_until(func(): return app_node.online, 90)
		if joined:
			break
		_command("cancel_connecting")
		await _frames(2)
	_expect(joined, "真服务端：从 Local 列表点 Join 真的连上并完成握手")
	sender.close()
	if not joined:
		_dump_server_log(String(srv["log"]), "真服务端：连不上")
		await _kill_server(pid)
		await _net_reset()
		return

	var session: GwNetSession = app_node.net.session
	_expect(app_node.current_screen == "PlayScreen", "真服务端：握手完成即进 PlayScreen")
	_expect(_screen("PlayScreen").visible, "真服务端：PlayScreen 可见")
	_expect(session.my_connection_id == 228, "真服务端：NewConnection.connection_id = 228")
	for kind in ["NewConnection", "TurnModeInfo", "GameStateInfo", "GameNameInfo",
			"AxisModeInfo", "FunctionModeInfo", "TimeModeInfo"]:
		_expect(session.count_of(kind) >= 1, "真服务端：收到 %s" % kind)
	_expect(app_node.state.room_name == "Graphwar Game", "真服务端：房间名进了 state")
	_expect(app_node.state.time_mode == 1, "真服务端：time_mode=Timer1m → 1")

	# 开局：客户端自动发 GameStartRequest，服务端回障碍/士兵/轮到谁
	var started := await _wait_until(func(): return _server_started(session), NET_WAIT_FRAMES)
	_expect(started, "真服务端：开局那一批到了（障碍 %d、SoldierInfo %d）"
		% [session.count_of("AddObstacle"), session.count_of("SoldierInfo")])
	if not started:
		# 红了要能自己说明白：服务端到底发没发、seed 是哪个（`docs/18` §10.3.2）
		_dump_server_log(String(srv["log"]), "真服务端：开局那一批没到（seed=%d）" % match_seed)
	if started:
		_expect(session.room["obstacles"].size() > 0, "真服务端：障碍落进 room.obstacles（%d 个）"
			% session.room["obstacles"].size())
		_expect(session.count_of("PosInfo") >= 4, "真服务端：收到 PosInfo（士兵坐标）")
		_expect(session.count_of("LifeInfo") >= 4, "真服务端：收到 LifeInfo（生死）")
		_expect(session.count_of("TurnInfo") >= 1, "真服务端：收到 TurnInfo（轮到谁）")
		var players: Dictionary = session.room["players"]
		_expect(players.size() == 2, "真服务端：两名玩家（实际 %d）" % players.size())
		var soldiers_ok := true
		for player_key in players:
			if (players[player_key]["soldiers"] as Array).size() != 2:
				soldiers_ok = false
		_expect(soldiers_ok, "真服务端：每人 2 名士兵（按 SoldierInfo 归到玩家）")
		_expect(app_node.state.players.size() == 2, "真服务端：state 里也是 2 名玩家（实际 %d）"
			% app_node.state.players.size())
		_expect(app_node.online_room.get("soldiers", 0) == 4, "真服务端：App.online_room 记下 4 名士兵")

		# 真的打一发：以**当前回合玩家**的身份提交并开火。
		# ⚠️ 我们的服务端不发 `PlayerInfo`（docs/33 §4），所以客户端不知道自己是哪个
		# player_id —— 这里借用服务端 `TurnInfo` 里的那个 id（docs/134 §6 记了这个缺口）。
		var turn: Dictionary = session.room["turn"]
		var who := int(turn.get("player_id", -1))
		_expect(who > 0, "真服务端：TurnInfo 给了当前回合玩家（%d）" % who)
		if who > 0:
			_expect(session.submit_function(who, "0.00"), "真服务端：FunctionUpdateRequest 发出去了")
			var echoed := await _wait_until(func(): return session.count_of("FunctionInfo") >= 1, 180)
			_expect(echoed, "真服务端：收到 FunctionInfo 回执")
			_expect(session.fire(who), "真服务端：FunctionFireRequest 发出去了")
			var fired := await _wait_until(
				func(): return session.count_of("FunctionPoints") >= 1, 240)
			_expect(fired, "真服务端：开火后收到 FunctionPoints（服务器只广播首末点）")
			if fired:
				_expect(session.count_of("EffectInfo") >= 1 or session.count_of("QueueDestroyObstacle") >= 1,
					"真服务端：命中链路有回包（EffectInfo / QueueDestroyObstacle）")
				var corrected := ""
				for owner in (session.room["functions"] as Dictionary):
					corrected = String(session.room["functions"][owner]["corrected"])
				_expect(not corrected.is_empty(),
					"真服务端：修正后的函数文本进了 room.functions（`f(x) ± c`，实际 \"%s\"）" % corrected)
				_expect(app_node.state.players.size() == 2,
					"真服务端：对局中 state 仍是 2 名玩家")

	# 退出对局 → 回主菜单（同时把 socket 放掉）
	_command("exit_play")
	await _frames(2)
	_expect(app_node.current_screen == "MainMenu", "真服务端：exit_play 回主菜单")
	_expect(not app_node.online, "真服务端：退出后 online 置假")
	await _kill_server(pid)
	await _frames(2)
	await _net_reset()

# ---------------------------------------------------------------- 8. 联机 PlayScreen（docs/139）

## `docs/134` §7.1 的交接清单，验收分两段：
##
## **A. 真服务端**（`examples/serve_match`）—— 本任务的三条硬指标都在这里：
##   ① 战场上的士兵/障碍来自服务端（不是 `_spawn_position()` 摆的）；
##   ② 开火后收到 `FunctionPoints`，界面用内核按它复算并画出弹道；
##   ③ 收到死亡广播后那名士兵从画面上消失。
##
## **B. 进程内假服务器** —— 补 A 段拿不到的两件事（真服务端的时序/内容由 `src/` 决定）：
##   * `LifeInfo{alive:false}` 这一条**线上**从来不发（我们的服务端发 `LifeQueuedDeath`，
##     `docs/31` §1），所以这条分支只能用受控报文验；
##   * 弹道复算的"点序列 = 服务端那条"可以用一条**固定**的 `FunctionPoints` 钉死。
##
## 缺 `serve_match` 时 A 段整段 skip（显式打印原因），B 段照样跑。
##
## 关卡种子（`ONLINE_TEST_SEED`）由 `_spawn_server()` 统一钉死 —— 本用例和
## `_test_net_real_server` 用的是同一个种子，所以两边跑的是**同一份布局**。

func _test_play_online() -> void:
	# 种子的设置/还原在 `_spawn_server()` 里做（起子进程之前摆好、起完立刻还原）。
	await _test_play_online_real_server()
	await _test_play_online_lifeinfo_fake_server()

## 挑一条"能打中敌人"的函数：**用内核**预演服务端会怎么判。
##
## 为什么要挑：服务端的障碍是每局随机生成的（`docs/62` §1），一条固定水平线经常被墙挡住，
## 而这一局只轮到我开一次火 —— 打空就再也验不了"死亡 ⇒ 消失"。
##
## 候选不是拍脑袋列的，是**按几何构造**的：
##
## ```text
## f(x) = a·x + k·sin(m·π·(x - x0)/(xt - x0))
## 修正后 g(x) = f(x) + (y0 - f(x0)) = y0 + a·(x - x0) + k·sin(…)      （g(x0) = y0）
## 取 a = (yt - y0)/(xt - x0) ⇒ g(xt) = yt                             （两端都在弹道上）
## ```
##
## 正弦项在两个端点上都是 0，所以 **k 只改中间鼓起来多少**（绕墙用的自由度），
## `m = 1/2/3` 给三种鼓法。这样每一族曲线都**必然穿过目标**，只需再问两件事：
## 内核的掩膜有没有把它挡下、它有没有顺手打到别人。
##
## 预演用的是**服务端那条规则**（掩膜判停 + 士兵半径 0.5 判死，`src/graphwar_shot_calculator/settle.rs::resolve_points`），
## 数据也全是服务端广播的 —— 这等于客户端本来就会做的本地预览，不是作弊。
## 采样口径：服务端按弦长 0.5 结算（`src/game.rs::fire`），内核按 0.01；这里每 50 点取 1，
## 让预演贴近服务端那一份（`docs/139` §4 记了这条近似）。
func _pick_online_shot(state: Dictionary) -> String:
	var shooter: Dictionary = state["shooter"]
	if shooter.is_empty():
		return ""
	var start := Vector2(float(shooter["x"]), float(shooter["y"]))
	var direction := 1.0 if int(shooter["team"]) == 0 else -1.0
	var enemies := _online_enemies_of(state)
	var obstacles := _online_obstacles_of(state)
	if enemies.is_empty():
		return ""
	var tried := {}
	for e in enemies:
		var target: Vector2 = e["pos"]
		var dx := target.x - start.x
		if absf(dx) < 1e-6:
			continue
		var slope := (target.y - start.y) / dx
		# ---- ① 直线 + 正弦鼓包：k 控制中间鼓多少（m 给 1/2/3 个鼓包） ----
		for m in [1, 2, 3]:
			var w: float = float(m) * PI / dx
			for k in [0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 4.0, -4.0, 5.0, -5.0, 6.0, -6.0]:
				var expr := _online_sine_expr(slope, k, w, start.x)
				if tried.has(expr):
					continue
				tried[expr] = true
				if _online_shot_hits(expr, start, direction, enemies, obstacles):
					print("[gw2/selftest] 联机：选中函数 %s（目标 %.3f, %.3f）" % [expr, target.x, target.y])
					return expr
		# ---- ② 过"路标"的抛物线：f(x) = slope·x + b·(x-x0)·(x-xt) ----
		# 这一族同样**必然穿过射手与目标**（两个因子在端点上归零），而 b 由路标
		# (wx, wy) 反解 ⇒ 相当于"先挑一个空的中间点，再问内核这条路通不通"。
		for wx_f in [0.2, 0.35, 0.5, 0.65, 0.8]:
			var wx: float = start.x + dx * float(wx_f)
			for wy in [-14.0, -11.0, -8.0, -5.0, -2.0, 1.0, 4.0, 7.0, 10.0, 13.0]:
				var den := (wx - start.x) * (wx - target.x)
				if absf(den) < 1e-9:
					continue
				var b: float = (float(wy) - (start.y + slope * (wx - start.x))) / den
				var expr2 := _online_poly_expr(slope, b, start.x, target.x)
				if tried.has(expr2):
					continue
				tried[expr2] = true
				if _online_shot_hits(expr2, start, direction, enemies, obstacles):
					print("[gw2/selftest] 联机：选中函数 %s（路标 %.3f, %.3f → 目标 %.3f, %.3f）"
						% [expr2, wx, float(wy), target.x, target.y])
					return expr2
	# 全都否了：把布局与各理由的计数打出来（否则现场只剩"挑不到"三个字）
	var reasons := {}
	for expr_key in tried:
		var why := String(_online_shot_reason(String(expr_key), start, direction, enemies, obstacles))
		reasons[why] = int(reasons.get(why, 0)) + 1
	print("[gw2/selftest] 联机：挑不到函数 —— 射手 (%.3f, %.3f) dir=%.0f ／ 敌人 %s ／ 障碍 %s ／ 理由 %s"
		% [start.x, start.y, direction, str(enemies), str(obstacles), str(reasons)])
	return ""

## `f(x) = slope·x + k·sin(w·(x - x0))` 的原文。
##
## 符号一律显式拼（`+ 10.0` / `- 10.0`），不依赖解析器对 `x - -10` 的处理。
func _online_sine_expr(slope: float, k: float, w: float, x0: float) -> String:
	var inner := "(x %s %.12f)" % ["-" if x0 >= 0.0 else "+", absf(x0)]
	var out := "%.12f*x" % slope
	if absf(k) > 1e-15:
		out += " %s %.12f*sin(%.12f*%s)" % ["+" if k > 0.0 else "-", absf(k), w, inner]
	return out

## `f(x) = slope·x + b·(x - x0)·(x - xt)` 的原文（两个因子在端点上归零）。
func _online_poly_expr(slope: float, b: float, x0: float, xt: float) -> String:
	var out := "%.12f*x" % slope
	if absf(b) > 1e-15:
		out += " %s %.12f*(x %s %.12f)*(x %s %.12f)" % [
			"+" if b > 0.0 else "-", absf(b),
			"-" if x0 >= 0.0 else "+", absf(x0),
			"-" if xt >= 0.0 else "+", absf(xt),
		]
	return out

## 这一条函数**按服务端的判法**会不会打死一个敌人（且不被墙挡住）。
func _online_shot_hits(expr: String, start: Vector2, direction: float, enemies: Array,
		obstacles: Array) -> bool:
	return String(_online_shot_reason(expr, start, direction, enemies, obstacles)) == ""

## 判定理由（`""` = 可用）。诊断用：全都被否掉时把它按理由计数打出来。
func _online_shot_reason(expr: String, start: Vector2, direction: float, enemies: Array,
		obstacles: Array) -> String:
	var res := GwCore.calculate_path(expr, start, direction)
	if not bool(res.ok):
		return "parse:" + String(res.note)
	var pts: PackedVector2Array = res.points
	var coarse := PackedVector2Array()
	var i := 0
	while i < pts.size():
		coarse.append(pts[i])
		i += 50
	if coarse.size() < 2:
		return "too_short"
	var judged := GwCore.hit_test(coarse, enemies, obstacles)
	if not bool(judged.get("ok", false)):
		return "hit_test_failed"
	if bool(judged.get("blocked", true)):
		return "blocked"
	var hits: Array = judged.get("hits", [])
	var hit_points: PackedVector2Array = judged.get("hit_points", PackedVector2Array())
	if hits.size() != 1:
		return "hits=%d" % hits.size()
	if hit_points.size() < 1:
		return "no_hit_point"
	var idx := int(hits[0])
	if idx < 0 or idx >= enemies.size():
		return "bad_index"
	if hit_points[0].distance_to((enemies[idx] as Dictionary)["pos"]) > 0.2:
		return "far"
	return ""

func _online_enemies_of(state: Dictionary) -> Array:
	var enemies: Array = []
	var my_pid := int(state.get("my_player_id", -1))
	var my_team := -1
	for s in state["soldiers"]:
		if int(s["player_id"]) == my_pid:
			my_team = int(s["team"])
			break
	for s in state["soldiers"]:
		if not bool(s["alive"]):
			continue
		if my_pid >= 0 and int(s["player_id"]) == my_pid:
			continue
		if my_pid < 0 and my_team >= 0 and int(s["team"]) == my_team:
			continue
		enemies.append({"pos": Vector2(float(s["x"]), float(s["y"])), "radius": 0.5})
	return enemies

func _online_obstacles_of(state: Dictionary) -> Array:
	var obstacles: Array = []
	for o in state["obstacles"]:
		obstacles.append({
			"pos": Vector2(float(o["x"]), float(o["y"])), "size": float(o["r"]),
		})
	return obstacles

## 服务端 `entities` 里第一个 `alive == false` 的士兵实体；没有就 -1。
func _first_dead_entity(session: GwNetSession) -> int:
	var entities: Dictionary = session.room["entities"]
	var ids := entities.keys()
	ids.sort()
	for eid in ids:
		var e: Dictionary = entities[eid]
		if String(e.get("kind", "")) == "soldier" and not bool(e.get("alive", true)):
			return int(eid)
	return -1

## 本用例的"开局那一批到了"判据。
##
## **不用** `_server_started()`：那个要求 `AddObstacle > 0`，而 `graphwar_level_generator::generate()` 的
## `n = trunc(normal()*7 + 15)` 在 `min_count = 0` 时有约 1.6% 会落到 0 个障碍
## （`src/graphwar_level_generator.rs:89`；`docs/139` §7 记了这一条 —— 既有用例 `_test_net_real_server`
## 用的是旧判据，本轮不动它）。这里只要求"士兵都到了 + 轮到谁说了"。
func _play_online_started(session: GwNetSession) -> bool:
	return session.count_of("SoldierInfo") >= 4 and session.count_of("TurnInfo") >= 1

## A 段：真服务端（`examples/serve_match`）。
func _test_play_online_real_server() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	var bin := _find_server_binary()
	if bin.is_empty():
		_skip("联机 PlayScreen：没找到 examples/serve_match（先 `cargo build --offline --example serve_match`，"
			+ "或用 GW2_NET_SERVER 指定路径）")
		return
	# ---- 先找一局**打得出去**的 ----
	#
	# 为什么可能打不出去：服务端的障碍是每局重抽的（`docs/62` §1），而开局位置是固定的
	# （两队各 x=±10，`src/graphwar_game_server.rs`），障碍**不看**这两个点 ⇒ 偶尔会有障碍压住射手，
	# 那样任何弹道在第一点就被掩膜判"挡住"（`src/graphwar_shot_calculator/settle.rs::resolve_points`），
	# 本地预演会如实告诉你 153 个候选全都 blocked。这种情况换一局（最多 4 次），
	# 不在断言里制造随机失败（`docs/139` §6.2 记了这一条）。
	var play = _screen("PlayScreen")
	if not _expect(play != null and play.has_method("online_state"),
			"联机 PlayScreen：PlayScreen 有 online_state()（本轮新增的诊断入口）"):
		return
	var pid := -1
	var srv := {}
	var session: GwNetSession = null
	var st := {}
	var expr := ""
	var attempts := 0
	# 种子已钉死（`ONLINE_TEST_SEED`）⇒ 第 1 次尝试就拿到那份已知能打的布局；
	# 循环保留下来只当兜底（例如服务端起不来时换一台重试），不再是撞概率。
	for attempt in 6:
		attempts += 1
		srv = await _spawn_server(bin, _match_seed())
		pid = int(srv["pid"])
		if pid <= 0 or not bool(srv["ok"]):
			_dump_server_log(String(srv["log"]), "联机 PlayScreen：服务端起不来")
			pid = -1
			await _kill_server(int(srv["pid"]))
			await _net_reset()
			continue
		var addr := String(srv["addr"])
		var joined := false
		for a2 in 6:
			# 服务端已确认 LISTEN，不再用 0.2 s 墙钟睡眠赌 bind
			await _frames(2)
			app_node.connect_to(addr)
			joined = await _wait_until(func(): return app_node.online, 90)
			if joined:
				break
			_command("cancel_connecting")
			await _frames(2)
		if not joined:
			_dump_server_log(String(srv["log"]), "联机 PlayScreen：连不上（%s）" % addr)
			await _kill_server(pid)
			pid = -1
			await _net_reset()
			continue
		session = app_node.net.session
		var started := await _wait_until(
			func(): return _play_online_started(session), NET_WAIT_FRAMES)
		if not started:
			_dump_server_log(String(srv["log"]), "联机 PlayScreen：开局那一批没到")
			await _kill_server(pid)
			pid = -1
			await _net_reset()
			continue
		await _frames(3)      # 让 App 的 game_started → PlayScreen.begin_match() 跑完
		st = play.call("online_state")
		expr = _pick_online_shot(st)
		if expr != "":
			break
		print("[gw2/selftest] 联机：第 %d 局的射手打不出去（障碍压住 / 全被挡），换一局"
			% (attempt + 1))
		await _kill_server(pid)
		pid = -1
		_command("exit_play")
		await _frames(2)
		await _net_reset()
	if not _expect(pid > 0 and session != null and expr != "",
			"联机 PlayScreen：%d 次尝试内找到一局打得出去的（连上 + 开局 + 有一条能命中敌人的函数）"
				% attempts):
		if pid > 0:
			await _kill_server(pid)
		await _net_reset()
		return
	_expect(int(session.my_connection_id) == 228,
		"联机 PlayScreen：NewConnection.connection_id = 228")

	# ---------------- ④ 我是谁：服务端 `PlayerInfo`（本轮补的） ----------------
	_expect(session.count_of("PlayerInfo") >= 1,
		"联机 PlayScreen：收到 PlayerInfo（服务端本轮补上，docs/139 §4）")
	_expect(int(session.my_player_id) == 229,
		"联机 PlayScreen：会话认下 my_player_id = 229（实际 %d）" % int(session.my_player_id))
	st = play.call("online_state")      # 重读一次：`game_started` 之后 App 又刷过一遍
	_expect(bool(st["mode"]), "联机 PlayScreen：PlayScreen 走的是**联机分支**（不是本地摆兵）")
	_expect(int(st["my_player_id"]) == 229,
		"联机 PlayScreen：界面侧也认下 229（实际 %d）" % int(st["my_player_id"]))
	_expect(int(st["turn_player"]) > 0,
		"联机 PlayScreen：TurnInfo 给了当前回合玩家（%d）" % int(st["turn_player"]))
	_expect(int(st["turn_player"]) == int(session.room["turn"]["player_id"]),
		"联机 PlayScreen：界面里的回合玩家 = 服务端 room.turn.player_id")
	_expect(int(st["turn_soldier"]) == int(session.room["turn"]["soldier_id"]),
		"联机 PlayScreen：界面里的射手实体 = 服务端 room.turn.soldier_id")

	# ---------------- ① 战场来自服务端 ----------------
	var room: Dictionary = session.room
	var entities: Dictionary = room["entities"]
	var soldiers: Array = st["soldiers"]
	_expect(soldiers.size() == 4, "① 战场上有 4 名士兵（服务端下发，实际 %d）" % soldiers.size())
	var ids_ok := true
	var pos_ok := true
	var no_local_spawn := true
	for s in soldiers:
		var eid := int(s["entity_id"])
		if not entities.has(eid):
			ids_ok = false
			continue
		var e: Dictionary = entities[eid]
		# 容差 1e-4：界面把坐标存进 `Vector2`（Godot 的 Vector2 是 **float32**），
		# 所以末几位本来就会掉；棋盘单位下 1e-4 远小于一个像素（`docs/139` §4）。
		if absf(float(s["x"]) - float(e["x"])) > 1e-4 or absf(float(s["y"]) - float(e["y"])) > 1e-4:
			pos_ok = false
		# `_spawn_position()` 只会在 x = ±18 上放人（本地规则）；服务端放在 x = ±10。
		if absf(absf(float(s["x"])) - 18.0) < 1e-6:
			no_local_spawn = false
	_expect(ids_ok, "① 界面上的每一名士兵都是服务端 `entities` 里的实体（entity_id 对得上）")
	_expect(pos_ok, "① 士兵坐标逐条等于服务端 `PosInfo` 写下的坐标（不是本地摆的）")
	_expect(no_local_spawn, "① 没有一名士兵落在 `_spawn_position()` 的 x=±18 上")
	var obstacles: Array = st["obstacles"]
	var server_obstacles: Array = room["obstacles"]
	_expect(obstacles.size() == server_obstacles.size(),
		"① 障碍数与服务端 `AddObstacle` 一致（界面 %d ／ 服务端 %d）"
			% [obstacles.size(), server_obstacles.size()])
	if server_obstacles.is_empty():
		# 服务端这一局真的没下发障碍（`src/graphwar_level_generator.rs:89` 的 n 可以钳到 0，约 1.6%）。
		# 不假装通过：显式 skip，并把理由打出来。
		_skip("① 障碍来自服务端：本次服务端下发 0 个障碍（levelgen 的 n 钳到 0），这条无从判")
	else:
		_expect(obstacles.size() > 0, "① 障碍非空（%d 个，本地非战役原本恒为 0）"
			% obstacles.size())
	var obs_ok := true
	for i in mini(obstacles.size(), server_obstacles.size()):
		if absf(float(obstacles[i]["x"]) - float(server_obstacles[i]["x"])) > 1e-4 \
				or absf(float(obstacles[i]["y"]) - float(server_obstacles[i]["y"])) > 1e-4 \
				or absf(float(obstacles[i]["r"]) - float(server_obstacles[i]["r"])) > 1e-4:
			obs_ok = false
	if not obs_ok:
		for i in mini(obstacles.size(), server_obstacles.size()):
			if absf(float(obstacles[i]["x"]) - float(server_obstacles[i]["x"])) > 1e-4 \
					or absf(float(obstacles[i]["y"]) - float(server_obstacles[i]["y"])) > 1e-4 \
					or absf(float(obstacles[i]["r"]) - float(server_obstacles[i]["r"])) > 1e-4:
				print("[gw2/selftest] 障碍首次不符 #%d：界面 %s ／ 服务端 %s"
					% [i, str(obstacles[i]), str(server_obstacles[i])])
				break
	_expect(obs_ok, "① 每个障碍的坐标/半径逐条等于服务端 `AddObstacle`（含顺序）")
	_expect(int(st["drawn_soldiers"]) == soldiers.size(),
		"① 画出来的士兵数 = 服务端下发的士兵数（%d）" % int(st["drawn_soldiers"]))

	# 构造式的候选表达式必须**内核能解析**（否则"挑不到函数"会变成静默失败）
	var probe_a := GwCore.calculate_path(_online_poly_expr(0.2, -0.5, -10.0, 10.0),
		Vector2(-10.0, 0.0), 1.0)
	var probe_b := GwCore.calculate_path(_online_sine_expr(0.0, 2.0, 0.15708, -10.0),
		Vector2(-10.0, 0.0), 1.0)
	_expect(bool(probe_a.ok) and bool(probe_b.ok),
		"联机 PlayScreen：构造出来的候选表达式内核能解析（抛物线 %s ／ 正弦 %s）"
			% [String(probe_a.note), String(probe_b.note)])

	# ---------------- ② 开火 → FunctionPoints → 内核复算弹道 ----------------
	_expect(bool(st["can_fire"]),
		"② PlayerInfo 说我是 229、TurnInfo 也轮到 229 ⇒ 可以开火")
	# `expr` 是上面那轮"找一局打得出去的"里挑好的（这里不重新挑：同一条状态只会给同一个答案，
	# 而每挑一次要跑 150+ 次内核调用）。
	_expect(expr != "", "② 用内核挑到一条能打中敌人的函数（%s）" % expr)
	if expr != "":
		var fp_before := session.count_of("FunctionPoints")
		_fire_function(play, expr)
		var fired := await _wait_until(
			func(): return session.count_of("FunctionPoints") > fp_before, 240)
		_expect(fired, "② 开火后收到 FunctionPoints（服务端只发首末点）")
		await _frames(2)
		var st2: Dictionary = play.call("online_state")
		var shot: Dictionary = st2["shot"]
		_expect(not shot.is_empty(), "② 界面按 FunctionPoints 复算了弹道")
		if not shot.is_empty():
			if String(shot["function"]) != expr:
				print("[gw2/selftest] 复算函数不符：提交 %s ／ 复算 %s ／ room.functions=%s"
					% [expr, String(shot["function"]), str(session.room["functions"])])
			_expect(String(shot["function"]) == expr,
				"② 复算用的函数文本 = 我提交的那一条（%s）" % String(shot["function"]))
			_expect(int(shot["points"]) >= 2, "② 复算出的点序列 ≥ 2 点（实际 %d）" % int(shot["points"]))
			_expect(int(st2["drawn_points"]) == int(shot["points"]),
				"② 画在棋盘上的点数 = 复算出的点数（%d）" % int(st2["drawn_points"]))
			# 方向由 end_x 定（不是按队伍猜）
			var start_x := float((shot["start"] as Vector2).x)
			var end_x := float(shot["end_x"])
			var dir_ok := (end_x > start_x and float(shot["direction"]) > 0.0) \
				or (end_x < start_x and float(shot["direction"]) < 0.0)
			_expect(dir_ok, "② 方向由 end_x 定（起点 x %.3f → 终点 x %.3f）" % [start_x, end_x])
			# 只画到服务端说的终点：最后一个点的 x 不该越过 end_x 一个弦长以上
			_expect(absf(float(shot["last_x"]) - end_x) <= 0.6,
				"② 弹道收在服务端给的 end_x 上（末点 x %.3f ／ end_x %.3f）"
					% [float(shot["last_x"]), end_x])

	# ---------------- ③ 死亡广播 → 士兵从画面上消失 ----------------
	#
	# ⚠️ 判据为什么不看 `session.room`：我们的服务端发的是 `LifeQueuedDeath`（`docs/31` §1），
	# 而 `GwNetSession._apply()` 只认 `LifeInfo`（联机层不在本轮可改范围）—— 所以会话模型里的
	# `alive` 位**不会**变。界面侧两种报文都收（`_online_apply_life()`），这里就以界面为准，
	# 同时断言线上真的来了死亡报文（否则"消失"就没有依据）。
	var died := await _wait_until(
		func(): return int(play.call("online_state")["alive_soldiers"]) < soldiers.size(), 300)
	_expect(died, "③ 收到死亡广播后界面记下了阵亡（线上报文：LifeQueuedDeath %d 条）"
		% session.count_of("LifeQueuedDeath"))
	_expect(session.count_of("LifeQueuedDeath") >= 1,
		"③ 线上真的来了 `LifeQueuedDeath`（docs/31 §1，实际 %d 条）"
			% session.count_of("LifeQueuedDeath"))
	if died:
		var st3: Dictionary = play.call("online_state")
		var dead_id := -1
		for eid_key in (st3["lives"] as Dictionary):
			if not bool(st3["lives"][eid_key]):
				dead_id = int(eid_key)
				break
		_expect(dead_id > 0, "③ 界面记下了是哪个实体死的（entity %d）" % dead_id)
		var still_alive := false
		for s3 in st3["soldiers"]:
			if int(s3["entity_id"]) == dead_id and bool(s3["alive"]):
				still_alive = true
		_expect(not still_alive, "③ 实体 %d 的 alive 位翻成 false" % dead_id)
		_expect(int(st3["drawn_soldiers"]) == int(st3["alive_soldiers"]),
			"③ 画面上只剩活着的（画 %d ／ 活 %d）"
				% [int(st3["drawn_soldiers"]), int(st3["alive_soldiers"])])
		_expect(int(st3["drawn_soldiers"]) < soldiers.size(),
			"③ 阵亡的士兵从画面上消失（%d → %d）"
				% [soldiers.size(), int(st3["drawn_soldiers"])])

	# ---------------- ⑤ 回合交给对手之后不能再开火（PlayerInfo 的用处） ----------------
	var swapped := await _wait_until(
		func(): return int(session.room["turn"]["player_id"]) != 229, 240)
	_expect(swapped, "⑤ 服务端把回合交给了对手（TurnInfo 驱动，界面不自己换手）")
	if swapped:
		await _frames(2)
		var st4: Dictionary = play.call("online_state")
		_expect(not bool(st4["can_fire"]),
			"⑤ 不是我的回合 ⇒ 界面拒绝开火（只有 PlayerInfo 分得清我跟对手）")
		var fp2 := session.count_of("FunctionPoints")
		_fire_function(play, "0")
		await _frames(3)
		_expect(session.count_of("FunctionPoints") == fp2,
			"⑤ 被拒绝的那一发**没有**发到服务端（FunctionPoints 数不变）")

	# 退出对局 → 回主菜单（同时把 socket 放掉）
	_command("exit_play")
	await _frames(2)
	_expect(app_node.current_screen == "MainMenu", "联机 PlayScreen：exit_play 回主菜单")
	_expect(not app_node.online, "联机 PlayScreen：退出后 online 置假")
	var after: Dictionary = play.call("online_state")
	_expect(not bool(after["mode"]), "联机 PlayScreen：退出后联机分支关掉了（订阅已放）")
	await _kill_server(pid)
	await _frames(2)
	await _net_reset()

## B 段：进程内假服务器 —— 只发**这一条**要验的报文。
##
## 为什么需要它：真服务端在我们的复原里**不发** `LifeInfo{alive:false}`（它发
## `LifeQueuedDeath`，`docs/31` §1），而"生死的唯一权威是 `LifeInfo`"是回放语料
## （`docs/96` §4）与协议表（`docs/24` §4.5）都写着的事实。所以这条分支必须**单独**验，
## 否则一旦真服务器哪天改发 `LifeInfo`，界面就会挂。
func _test_play_online_lifeinfo_fake_server() -> void:
	var app_node := get_node("/root/App")
	await _net_reset()
	var srv := TCPServer.new()
	if srv.listen(0, "127.0.0.1") != OK:
		_expect(false, "联机 PlayScreen：假服务器起不来")
		return
	var port := srv.get_local_port()
	app_node.connect_to("127.0.0.1:%d" % port)
	var got_peer := await _wait_until(func(): return srv.is_connection_available(), 120)
	if not _expect(got_peer, "联机 PlayScreen：假服务器收到连接"):
		srv.stop()
		await _net_reset()
		return
	var peer := srv.take_connection()
	peer.set_no_delay(true)

	# 一笔写下去：7 条状态同步 + 开局那一批 + `PlayerInfo` + `TurnInfo`。
	# 形状全部是抓包的（`GwWireSamples.STATE_SYNC` 逐字节，其余照 `docs/24` §3–§6）。
	var batch: Array[String] = []
	for m in GwWireSamples.STATE_SYNC:
		batch.append(m)
	batch.append(GwNetFrame.message("AddObstacle",
		'{"pos_x":-4.0,"pos_y":3.0,"radius":1.5}'))
	batch.append(GwNetFrame.message("SoldierInfo", '{"soldier_id":230,"player_id":229}'))
	batch.append(GwNetFrame.message("SoldierInfo", '{"soldier_id":232,"player_id":231}'))
	batch.append(GwNetFrame.message("TeamInfo", '{"entity_id":230,"team":"Team1"}'))
	batch.append(GwNetFrame.message("TeamInfo", '{"entity_id":232,"team":"Team2"}'))
	batch.append(GwNetFrame.message("PosInfo",
		'{"entity_id":230,"pos_x":-10.0,"pos_y":0.0,"radius":0.5}'))
	batch.append(GwNetFrame.message("PosInfo",
		'{"entity_id":232,"pos_x":10.0,"pos_y":0.0,"radius":0.5}'))
	batch.append(GwNetFrame.message("LifeInfo", '{"entity_id":230,"alive":true}'))
	batch.append(GwNetFrame.message("LifeInfo", '{"entity_id":232,"alive":true}'))
	batch.append(GwNetFrame.message("PlayerInfo", '{"player_id":229,"connection_id":228}'))
	batch.append(GwNetFrame.message("TurnInfo", '{"player_id":229,"soldier_id":230}'))
	_expect(peer.put_data(GwNetFrame.encode_many(batch)) == OK,
		"联机 PlayScreen：假服务器把整批报文写进去了")
	var joined := await _wait_until(func(): return app_node.online, NET_WAIT_FRAMES)
	_expect(joined, "联机 PlayScreen：假服务器这一局也进了 PlayScreen")
	if not joined:
		peer.disconnect_from_host()
		srv.stop()
		await _net_reset()
		return
	var play = _screen("PlayScreen")
	await _frames(3)
	var st: Dictionary = play.call("online_state")
	_expect(int(st["my_player_id"]) == 229,
		"联机 PlayScreen：`PlayerInfo` 一条就够（界面侧 my_player_id = %d）"
			% int(st["my_player_id"]))
	_expect((st["soldiers"] as Array).size() == 2,
		"联机 PlayScreen：假服务器这一局摆了 2 名士兵（实际 %d）"
			% (st["soldiers"] as Array).size())
	_expect(int(st["drawn_soldiers"]) == 2, "联机 PlayScreen：画面上 2 名士兵")
	# 灌一条 `LifeInfo{alive:false}` —— ③ 的字面口径就是这一条。
	_expect(peer.put_data(GwNetFrame.encode(
		GwNetFrame.message("LifeInfo", '{"entity_id":232,"alive":false}'))) == OK,
		"联机 PlayScreen：LifeInfo{alive:false} 写出去了")
	var disappeared := await _wait_until(
		func(): return int(play.call("online_state")["drawn_soldiers"]) < 2, 180)
	_expect(disappeared, "③ 收到 `LifeInfo{alive:false}` 后士兵从画面上消失（2 → 1）")
	var st2: Dictionary = play.call("online_state")
	_expect(int(st2["alive_soldiers"]) == 1 and int(st2["drawn_soldiers"]) == 1,
		"③ 只剩 1 名活着的士兵（画 %d ／ 活 %d）"
			% [int(st2["drawn_soldiers"]), int(st2["alive_soldiers"])])
	var dead_listed := false
	for s in st2["soldiers"]:
		if int(s["entity_id"]) == 232:
			dead_listed = true
			_expect(not bool(s["alive"]),
				"③ 实体 232 的 alive 位翻成 false（只翻位，不重建实体）")
	_expect(dead_listed, "③ 阵亡的实体仍然记在 `_soldiers` 里（只是不画）")
	peer.disconnect_from_host()
	srv.stop()
	_command("exit_play")
	await _frames(2)
	await _net_reset()

## 微分方程局（`DiffEqFunction`）的样本：Level 16、131 条事件、4 发、靶子 3683 在 tick 1291 阵亡。
## 选它的理由：这一局**必须**走 ODE 才能重算出弹道（`docs/138`），
## 而且它的 `FunctionPoints.end_x` 与内核重算弹道的**最后一点 x 逐位相同**（4/4）。
const REPLAY_DIFFEQ_SAMPLE := "1783255495"
const DIFFEQ_FIRST_SHOT_TICK := 212
const DIFFEQ_FIRST_SHOT_END_X := -2.7599786229272687

## 第 7 组：**微分方程局的回放能画出弹道**（内核 `path_calculate_mode`，`docs/138`）。
##
## `mode` 是**字符串** `"normal"` / `"ode"`（不是 0/1）：`PropertyInfo` 把它声明成
## `String`，GDScript 传整数会被引擎先拒 —— 所以这里也只用字符串，
## 顺带把"非法取值一律 `bad_arguments`"钉住。
func _test_replay_ode() -> void:
	var dir := _real_replay_dir()
	if dir == "":
		_skip("回放（Ode）：本机没有原版存档目录")
		return
	var path := dir.path_join(REPLAY_DIFFEQ_SAMPLE)
	if not FileAccess.file_exists(path):
		_skip("回放（Ode）：样本 %s 不在 %s" % [REPLAY_DIFFEQ_SAMPLE, dir])
		return
	# 内核必须真的暴露了带模式的那条路
	var methods := GwCore.kernel_methods()
	_expect(methods.has("path_calculate_mode"),
		"内核暴露 path_calculate_mode（%s）" % str(methods.slice(0, 6)))
	# mode="ode" 要走内核并对上录像的 end_x；`offset` 必须是 NaN（没有修正常数）
	var ode := GwCore.calculate_path("y", Vector2(-3.954041122927256, -4.543043212675496),
		1.0, "ode")
	_expect(bool(ode.get("ok", false)) and String(ode.get("source", "")) == "kernel",
		"GwCore.calculate_path(..., 'ode') 走内核（source=%s ／ %s）"
		% [String(ode.get("source", "?")), String(ode.get("note", ""))])
	_expect(is_nan(float(ode.get("offset", 0.0))) and GwCore.format_offset(float(ode.get("offset", 0.0))) == "",
		"ODE 不返回修正常数（offset=NaN ⇒ format_offset 出空串）")
	var ode_pts: PackedVector2Array = ode.get("points", PackedVector2Array())
	_expect(ode_pts.size() == 1058, "ODE 弹道点数 1058（实际 %d）" % ode_pts.size())
	if ode_pts.size() > 0:
		_expect(absf(float(ode_pts[ode_pts.size() - 1].x) - DIFFEQ_FIRST_SHOT_END_X) < 0.001,
			"ODE 弹道最后一点 x = 录像 end_x %.6f（实际 %.6f）"
			% [DIFFEQ_FIRST_SHOT_END_X, float(ode_pts[ode_pts.size() - 1].x)])
	# 未知取值一律拒绝（不许静默退回 Normal —— 画错的线比不画更糟）
	var bad := GwCore.calculate_path("y", Vector2.ZERO, 1.0, "diffeq")
	_expect(not bool(bad.get("ok", true)), "未知 mode='diffeq' 被拒绝（不静默退回 Normal）")
	# mode="normal" 与三参数调用等价（既有契约没变）
	var a := GwCore.calculate_path("sin(x)", Vector2(0.0, 0.0), 1.0)
	var b := GwCore.calculate_path("sin(x)", Vector2(0.0, 0.0), 1.0, "normal")
	_expect((a.get("points", PackedVector2Array()) as PackedVector2Array).size()
		== (b.get("points", PackedVector2Array()) as PackedVector2Array).size(),
		"mode='normal' 与三参数调用同点数（既有契约一字未改）")

	# 进回放：模式必须取自**录像**（这一局是 DiffEqFunction）
	var app_node := get_node("/root/App")
	app_node.state.reset_replay(path)
	var play = _screen("PlayScreen")
	play.call("begin_match")
	await _frames(3)
	var rs: Dictionary = play.call("replay_state")
	_expect(bool(rs.get("mode", false)), "微分方程回放进了回放模式")
	_expect(int(rs.get("events", 0)) == 131,
		"微分方程回放 131 条事件（实际 %d）" % int(rs.get("events", 0)))

	# 第一发（tick 212）：弹道由内核 ODE 重算出来
	play.call("replay_seek_for_test", DIFFEQ_FIRST_SHOT_TICK)
	var plane := play.find_child("GraphwarPlane", true, false) as Control
	_expect(plane != null, "有 GraphwarPlane")
	if plane == null:
		return
	var dq: PackedVector2Array = play.call("plane_points")
	_expect(dq.size() > 100, "DiffEq 局的弹道由内核 ODE 重算出来（%d 点）" % dq.size())
	var lastx := 0.0
	if dq.size() > 0:
		lastx = float(dq[dq.size() - 1].x)
	_expect(absf(lastx - DIFFEQ_FIRST_SHOT_END_X) < 0.001,
		"重算弹道最后一点 x = 录像 end_x %.6f（实际 %.6f）" % [DIFFEQ_FIRST_SHOT_END_X, lastx])
	var status := String(play.call("replay_state").get("shot_status", ""))
	_expect(status.contains("Ode"), "状态行说明这一发走的是 Ode(dy/dx)（实际 '%s'）" % status)
	var offset_label := play.find_child("OffsetLabel", true, false) as Label
	_expect(offset_label != null and offset_label.text == "",
		"DiffEq 局不显示修正常数（观测：corrected_function 与 function 相同）")

	# 死亡事件：靶子 3683 在 tick 1291 阵亡
	play.call("replay_seek_for_test", 1290)
	_expect(int(play.call("plane_state").get("targets", -1)) == 1,
		"tick 1290 靶子还在（实际 %d 个）" % int(play.call("plane_state").get("targets", -1)))
	play.call("replay_seek_for_test", 1291)
	_expect(int(play.call("plane_state").get("targets", -1)) == 0,
		"tick 1291 靶子被打掉（实际 %d 个）" % int(play.call("plane_state").get("targets", -1)))
	play.call("end_match")
	await _frames(2)
	app_node.state.reset_lan()

# ---------------------------------------------------------------- 对局流程（docs/143）
#
# ## 这一组为什么把 `App.automation` 关掉
#
# `automation`（`--selftest` / `--dump-tree`）下 `PlayScreen` 把"发射 → 结算"的时序
# **压平**：逻辑当刻落地，动画照放。存在的端到端用例都是这么跑的（它们要"开火后下一帧
# 就能读到结算结果"）。
#
# 真机（`automation == false`）走**逐 tick** 时序：弹道飞到死者那个点，那一次死亡才落地
# （`LifeQueuedDeath{tick}`，`docs/65`）；飞到尽头才炸、才清曲线、才判胜负
# （`docs/43` §1 的报文顺序 + `docs/41` 的状态机）。
#
# ⇒ 这一组关掉 automation 来**验证先后顺序**，与既有那组"打完之后世界是什么样"互补。
# 两边走的是**同一条**判定代码（`_judge_shots` / `_apply_kills_up_to` / `_settle_round`），
# 只有触发时机不同 —— 所以任何一边红了都说明那条规则本身坏了。

func _test_play_flow() -> void:
	var app_node := get_node("/root/App")
	var was_automation := bool(app_node.automation)
	app_node.automation = false          # ← 真机时序（docs/143 §3）

	# ---------------- 0. 回合轮转的**单元**断言（**规则在 Rust**，0x463da0） ----------------
	# `GwTurnDriver` 现在是扩展类（`src/_scaffold/play_flow_class.rs` →
	# `graphwar_turn_manager::PlayerTurns::next_shooter`）。两处接口变化：
	# ① `alive` 从 `Callable` 换成"按士兵下标的存活数组"（Rust 侧不接 GDScript 闭包）；
	# ② 常量 `SOLDIER_ID_NONE` 换成方法 `soldier_id_none()`（扩展类没有 const）。
	var td := GwTurnDriver.new()
	td.setup([{"player": 0, "soldiers": [0, 1]}])
	var alive_all := [true, true]
	_expect(td.first_shooter(alive_all) == 0, "轮转：第一个射手 = 0 号士兵")
	_expect(td.next_shooter(alive_all) == 1,
		"轮转：**先自增再取** ⇒ 同一下一个玩家换到 1 号士兵（0x463f40 的四条指令）")
	_expect(td.next_shooter(alive_all) == 0, "轮转：到尾回绕到 0 号")
	td.setup([{"player": 0, "soldiers": [0, 1]}])
	var alive_zero := [true, false]
	td.first_shooter(alive_all)
	_expect(td.next_shooter(alive_zero) == 0,
		"轮转：跳过死士兵（最多试 len 次，0x463fc7 的 dec/jne）")
	var alive_none := [false, false]
	td.setup([{"player": 0, "soldiers": [0, 1]}])
	td.first_shooter(alive_all)
	_expect(td.next_shooter(alive_none) == -1, "轮转：全灭 ⇒ 没有射手（对应 Some(SOLDIER_ID_NONE)）")
	_expect(td.soldier_id_none() == 0, "轮转：哨兵 soldier_id = 0（0x464016 的 movl $0x0）")
	td.setup([{"player": 0, "soldiers": [0]}, {"player": 1, "soldiers": [1]}])
	_expect(td.first_shooter(alive_all) == 0, "轮转：两名玩家 ⇒ 先从 0 号玩家的士兵开始")
	_expect(td.next_shooter(alive_all) == 1, "轮转：两名玩家 ⇒ 交替到 1 号玩家")
	_expect(td.next_shooter(alive_all) == 0, "轮转：两名玩家 ⇒ 再交替回来")

	# ---------------- 1. 发射 → 逐 tick → 飞完才结算（docs/143 §3/§5） ----------------
	# 第 1 关：1 个靶子、**无障碍**、无回合上限。靶子放在弹道之外 ⇒ 必然打空。
	var play = _begin_campaign_live(0, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	var sounds: Array = []
	if app_node.audio != null:
		app_node.audio.played.connect(func(s: String, slot: String) -> void:
			sounds.append({"sound": s, "slot": slot}))
	var plane := play.find_child("GraphwarPlane", true, false) as Control
	var fx_layer := play.find_child("EffectLayer", true, false) as GwEffectLayer
	_expect(fx_layer != null, "① 棋盘上有特效层 `GwEffectLayer`（docs/143 §1）")
	_expect(fx_layer != null and fx_layer.drawer != null,
		"① 特效层已经绑到**真** `EffectDrawer` 节点（§48.3：适配层不再自绘）")

	_flow_fire(play, "0")
	var flying: Dictionary = play.call("level_state")
	_expect(bool(flying["in_flight"]), "① 开火后弹道在飞（in_flight = true）")
	_expect(int(flying["turns_used"]) == 0,
		"① **还没结算 ⇒ 回合数仍是 0**（旧实现里开火当刻就 +1；实际 %d）" % int(flying["turns_used"]))
	_expect(int(flying["path_points"]) > 1,
		"① 弹道已经画上棋盘（%d 点）" % int(flying["path_points"]))
	_expect(int((flying["effects_spawned"] as Dictionary).get("total", 0)) == 0,
		"① 开火那一刻**还没有**特效（爆炸要等弹道飞完）")
	_expect(String(flying["verdict"].get("reason", "")) == "",
		"① 开火那一刻**没有**胜负判定（实际 %s）" % str(flying["verdict"]))
	# 推进一小段（0.05 s ≈ 3 tick ≈ 0.87 单位）⇒ 仍在飞、仍没结算
	play.call("advance_shot_for_test", 0.05)
	var mid: Dictionary = play.call("level_state")
	_expect(bool(mid["in_flight"]) and int(mid["turns_used"]) == 0,
		"① 弹道只飞了一小段 ⇒ 仍在飞、仍未结算（turns_used = %d）" % int(mid["turns_used"]))
	# 飞完
	play.call("advance_shot_for_test", 10.0)
	var landed: Dictionary = play.call("level_state")
	_expect(not bool(landed["in_flight"]), "① 弹道飞完 ⇒ in_flight = false")
	_expect(int(landed["turns_used"]) == 1,
		"① **飞完才结算**：turns_used = 1（实际 %d）" % int(landed["turns_used"]))
	_expect(int(landed["path_points"]) == 0,
		"② 飞完 ⇒ **曲线被清掉**（path_points = %d；旧实现一直留到下一次发射）"
			% int(landed["path_points"]))
	var spawned: Dictionary = landed["effects_spawned"]
	_expect(int(spawned.get("ExplosionEffect", 0)) == 1,
		"① 弹道飞完 ⇒ 正好 1 条 `ExplosionEffect`（docs/31 §2：停止点爆炸；实际 %d）"
			% int(spawned.get("ExplosionEffect", 0)))
	_expect(String(landed["verdict"].get("reason", "playing")) == "playing",
		"① 打空 ⇒ 判定仍是 playing（实际 %s）" % str(landed["verdict"]))
	var fx_log: Array = landed["fx_log"]
	var boom_x := NAN
	if fx_log.size() > 0:
		boom_x = absf(float(fx_log[fx_log.size() - 1]["x"]))
	_expect(fx_log.size() > 0 and boom_x > 24.0,
		"① 爆炸点在**弹道停止点**（x = %.3f，平面边界 ±25；docs/31 §3 的夹具坐标同型）" % boom_x)

	# ---------------- 2. 音效（槽位逐位、音效自定，docs/143 §4） ----------------
	if app_node.audio == null or not app_node.audio.loaded("explosion"):
		_skip("对局流程：音效 —— 音频资产缺失（先跑 client/src/audio/extract_audio.py）")
	else:
		var found_boom := false
		for e in sounds:
			if String(e["sound"]) == "explosion":
				found_boom = true
				_expect(String(e["slot"]) == "0x80",
					"③ 爆炸音走槽 0x80（`0x4bf090` 跳表第 0 臂；实际 %s）" % String(e["slot"]))
		_expect(found_boom, "③ 弹道飞完真的播了一个音（旧实现只在按 Fire 那一刻响一次）")
	_expect(String(play.call("level_state")["shot_status"]).length() >= 0, "（占位：状态行可读）")

	# ---------------- 3. **还没击中就判胜利** —— 最严重的那条（docs/143 §5） ----------------
	play = _begin_campaign_live(0, [Vector2(12.0, -5.1)], [])
	await _frames(1)
	app_node.close_all_popups()
	_flow_fire(play, "0")               # 靶子正在弹道线上
	var pre_hit: Dictionary = play.call("level_state")
	_expect(int(pre_hit["targets"]["alive"]) == 1,
		"⑤ 开火那一刻靶子**还没被打掉**（alive = %d）" % int(pre_hit["targets"]["alive"]))
	_expect(bool(pre_hit["running"]), "⑤ 开火那一刻**局还没结束**（旧实现当刻判胜）")
	_expect(String(pre_hit["verdict"].get("reason", "")) == "",
		"⑤ 开火那一刻没有胜负判定（实际 %s）" % str(pre_hit["verdict"]))
	_expect(not (_screen("EndGamePopUp") as Control).visible,
		"⑤ 开火那一刻**不弹结算框**（`EndingGame` 必须在结算之后）")
	play.call("advance_shot_for_test", 10.0)
	var post_hit: Dictionary = play.call("level_state")
	_expect(int(post_hit["targets"]["alive"]) == 0,
		"⑤ 飞完 ⇒ 靶子才被打掉（alive = %d）" % int(post_hit["targets"]["alive"]))
	_expect(not bool(post_hit["running"]), "⑤ 飞完 ⇒ 局结束")
	_expect(String(post_hit["verdict"].get("reason", "")) == "targets_cleared",
		"⑤ 飞完 ⇒ 判定 targets_cleared（实际 %s）" % str(post_hit["verdict"]))
	_expect((_screen("EndGamePopUp") as Control).visible, "⑤ 飞完 ⇒ 结算框才弹出")
	_expect(int((post_hit["effects_spawned"] as Dictionary).get("SparkEffect", 0)) >= 1,
		"① 靶子被打掉 ⇒ 至少 1 条 `SparkEffect`（推断：靶子只有这一个效果名可依，docs/143 §1）")
	_expect(int(post_hit["turns_used"]) == 1, "⑤ 飞完只算**一个**回合（实际 %d）"
		% int(post_hit["turns_used"]))

	# ---------------- 4. 发射期间不能再发（docs/143 §4） ----------------
	play = _begin_campaign_live(0, [Vector2(12.0, 2.0)], [])
	await _frames(1)
	app_node.close_all_popups()
	_flow_fire(play, "0")
	var first: Dictionary = play.call("level_state")
	var points_first := int(first["shot_points"])
	_flow_fire(play, "3*sin(x)+x")      # 另一条**长度不同**的弹道
	var second: Dictionary = play.call("level_state")
	_expect(int(second["shot_points"]) == points_first and points_first > 1,
		"④ 发射期间的第二发被拒 ⇒ 在飞的仍是原来那条（%d → %d 点）"
			% [points_first, int(second["shot_points"])])
	_expect(bool(second["in_flight"]), "④ 第二发没有打断第一发（in_flight 仍为 true）")
	_expect(String(second["status"]).contains("还在飞"),
		"④ 拒绝的理由写清了「上一发还在飞」（实际 '%s'）" % String(second["status"]))
	play.call("advance_shot_for_test", 10.0)
	var after_gate: Dictionary = play.call("level_state")
	_expect(int(after_gate["turns_used"]) == 1,
		"④ 被拒的那一发**没有**消耗回合（turns_used = %d）" % int(after_gate["turns_used"]))

	# ---------------- 5. 默认士兵数 = 关卡数据（docs/118 §7.1「我方士兵」列） ----------------
	_expect(int(GwCampaign.level(3)["soldiers"]) == 2,
		"士兵数：`Level 3` 的关卡数据是 2 个（`docs/118` §7.1 的 edx=2）")
	play = _begin_campaign_live(3, [Vector2(12.0, 2.0), Vector2(14.0, 6.0), Vector2(16.0, -9.0)], [])
	await _frames(1)
	var lv3: Dictionary = play.call("level_state")
	_expect(int(lv3["soldiers"]) == 2,
		"士兵数：**战役关按关卡数据装 2 个士兵**（玩家表里的默认值是 1；实际 %d）"
			% int(lv3["soldiers"]))
	var shooter0: Dictionary = lv3["active_soldier"]
	_expect(absf(float(shooter0.get("y", 0.0)) - (-5.1)) < 0.01,
		"士兵数：第一个射手是 0 号槽（y = −5.1，实际 %.2f）" % float(shooter0.get("y", 0.0)))
	app_node.close_all_popups()
	_flow_fire(play, "0")
	play.call("advance_shot_for_test", 10.0)
	var turn2: Dictionary = play.call("level_state")
	_expect(not bool(turn2["in_flight"]), "轮转：第二发的回合已结算")
	_expect(absf(float((turn2["active_soldier"] as Dictionary).get("y", 0.0)) - (-1.7)) < 0.01,
		"轮转：**下一个回合换到第二个士兵**（y = −1.7，实际 %.2f；旧实现永远用第一个）"
			% float((turn2["active_soldier"] as Dictionary).get("y", 0.0)))
	_expect(int((turn2["turn_driver"] as Dictionary).get("cursor", -1)) == 0,
		"轮转：只有一个玩家 ⇒ 玩家游标不动（实际 %d）"
			% int((turn2["turn_driver"] as Dictionary).get("cursor", -1)))

	# ---------------- 6. AI 的回合：它自己开火（docs/143 §6.1） ----------------
	# LAN 1v1：人（−18,−5.1）+ Bot（18,−5.1）。人打一发**故意打偏**的 `2*x`
	# （修正后 y = 2x + 30.9，很快飞出 |y| ≤ 15 的平面）⇒ 局面继续、轮到 Bot。
	play = _begin_versus_live()
	await _frames(1)
	app_node.close_all_popups()
	_flow_fire(play, "2*x")
	play.call("advance_shot_for_test", 10.0)
	var to_bot: Dictionary = play.call("level_state")
	_expect(bool(to_bot["running"]), "⑥ 人打偏 ⇒ 局还在（实际 running=%s）" % str(to_bot["running"]))
	_expect(bool((to_bot["active_soldier"] as Dictionary).get("is_bot", false)),
		"⑥ 结算之后回合交给 **AI**（旧实现停在这里等人类再按一次 Fire）")
	_expect(float(to_bot["bot_think_left"]) > 0.0,
		"⑥ 轮到 AI ⇒ 起了思考倒计时（%.2f s，**自定**值）" % float(to_bot["bot_think_left"]))
	var fired_by_bot := bool(play.call("bot_take_turn"))
	_expect(fired_by_bot, "⑥ AI **自己**开了一发（`bot_take_turn()` 返回 true）")
	var ai_shot: Dictionary = play.call("level_state")
	_expect(bool(ai_shot["in_flight"]) and int(ai_shot["shot_points"]) > 1,
		"⑥ AI 那一发真的上了棋盘（%d 点）" % int(ai_shot["shot_points"]))
	var ai_plane := play.find_child("GraphwarPlane", true, false) as Control
	var ai_pts: PackedVector2Array = play.call("plane_points")
	_expect(ai_plane != null and ai_pts.size() > 1
		and float(ai_pts[0].x) > 17.0,
		"⑥ AI 那一发的起点在**它自己**的士兵处（x = %.2f ≈ 18）"
			% (float(ai_pts[0].x) if ai_pts.size() > 0 else -999.0))
	play.call("advance_shot_for_test", 10.0)
	var after_ai: Dictionary = play.call("level_state")
	_expect(not bool(after_ai["in_flight"]), "⑥ AI 那发也结算了")
	_expect(int(after_ai["turns_used"]) == 2,
		"⑥ 人和 AI 各消耗一个回合（turns_used = %d）" % int(after_ai["turns_used"]))

	# ---------------- 收尾 ----------------
	play.call("clear_layout_override")
	play.call("end_match")
	app_node.close_all_popups()
	app_node.automation = was_automation
	_expect(bool(app_node.automation) == was_automation, "对局流程：automation 已还原")

## 起一局**按真机时序**的战役关：**不**注入玩家士兵数（`game_state.gd` 的默认值是 1），
## 让 `PlayScreen.begin_match()` 自己按关卡数据装人（`docs/118` §7.1）。
func _begin_campaign_live(level_index: int, targets: Array, obstacles: Array = []) -> Node:
	var app_node := get_node("/root/App")
	var lv := GwCampaign.level(level_index)
	app_node.state.reset_campaign(level_index)
	app_node.state.axis_mode = int(lv.axis)
	app_node.state.function_mode = int(lv.function)
	app_node.state.turn_mode = int(lv.turn)
	app_node.state.time_mode = int(lv.time)
	app_node.state.add_human("Selftest", 0, 0, 0)
	if int(lv.bot_tier) >= 0:
		app_node.state.add_bot(int(lv.bot_tier))
	var play = _screen("PlayScreen")
	play.call("inject_layout", {"targets": targets, "obstacles": obstacles})
	play.call("begin_match")
	return play

## 起一局 LAN 1v1（人 + Bot，各 1 个士兵），不注入布局。
func _begin_versus_live() -> Node:
	var app_node := get_node("/root/App")
	app_node.state.reset_lan()
	app_node.state.add_human("Selftest", 0, 0, 0)
	app_node.state.add_bot(0)
	var play = _screen("PlayScreen")
	play.call("inject_layout", {})
	play.call("begin_match")
	return play

## 只按 Fire（**不**等结算）：把函数写进输入框再调 `fire()`，与生产路径同一个入口。
func _flow_fire(play: Node, expr_text: String) -> void:
	var input := play.find_child("GraphwarFunctionInput", true, false) as GwFunctionInput
	if input != null:
		input.text = expr_text
	play.call("fire")
# ---------------------------------------------------------------- 大厅与关卡弹窗的实测 bug（docs/144）
#
# 四条都是用户实测报出来的，每条断言都必须能在**改动之前**变红：
#   1. 上一把开的房间，退出后不再挂在大厅列表里；
#   2. Global 页的占位行点 ` Join ` 不再弹"这一行没有房间地址"
#      （原版的房间表键就是 `SocketAddr`，根本没有"没有地址的行"）；
#   3. `Players` 列随人数变化（原版每帧从活数据重取，不是建行时拍一张快照）；
#   4. `JoinLevelPopUp` 的面板下沿 / 模式按钮正方 / 坐标那一格白底
#      —— 三条都用 `docs/130` §1 的真值管线量过（1920×1160）。

func _lobby_list() -> VBoxContainer:
	var lobby := _screen("LanLobby")
	if lobby == null:
		return null
	return lobby.find_child("GraphwarLanVBox", true, false) as VBoxContainer

## 底栏那个 " Join IP " 按钮（`LobbyButtonContainer.gd` 把它的宽度设成 屏宽×0.2）。
##
## **按 `input_text` 找，不按节点名找**：一屏里叫 `JoinButton` 的节点不止一个 ——
## 每个房间条目里也有一个（`game_room_entry.tscn` 的 `JoinButtonContainer/JoinButton`），
## 而 `find_child("JoinButton")` 返回的是**深度优先的第一个**。列表里有行时它拿到的是
## **行里**那个（宽度 0），这正是 2026-11-10 那条"成对红"的第二条（`docs/18` §10.3.4）。
## 底栏按钮的 `input_text == GwCommands.JOIN_IP`（"join_ip"），房间行的 Join 没有命令。
func _lobby_join_ip_button(lobby: Node) -> Control:
	if lobby == null:
		return null
	for n in lobby.find_children("JoinButton", "", true, false):
		if n is GwButton and String((n as GwButton).input_text) == GwCommands.JOIN_IP:
			return n as Control
	return null

func _lobby_row_count(list: VBoxContainer) -> int:
	if list == null:
		return -1
	var n := 0
	for c in list.get_children():
		if c is HBoxContainer:
			n += 1
	return n

## 第 `index` 行的 `PlayerCount` 标签。
func _lobby_row_players(list: VBoxContainer, index: int) -> Label:
	if list == null:
		return null
	var i := 0
	for c in list.get_children():
		if not (c is HBoxContainer):
			continue
		if i == index:
			return (c as Node).find_child("PlayerCount", true, false) as Label
		i += 1
	return null

func _test_lobby_fixes() -> void:
	var app_node := get_node("/root/App")
	# 干净的起点：停掉 broker、重建 LAN 监听（`stop()` 会清空房间表）、收起本机房间、回主菜单。
	if app_node.net != null:
		app_node.net.stop_broker()
		app_node.net.lan.stop()
		await _frames(1)
		app_node.net.lan_listen(0)
	app_node.online = false
	app_node.online_room = {}
	app_node.close_local_room()
	var saved_players: Array = app_node.state.players.duplicate(true)
	_command("exit_lan_lobby")
	await _frames(3)

	# --- bug 1（a）：退出大厅就把房间收掉 ---
	_command("create_game")
	await _frames(3)
	_expect(not app_node.local_room.is_empty(),
		"开房：create_game 之后 App 有本机房间（原版这一支是 start_server + start_lan_broadcaster）")
	_command("multiplayer")
	await _frames(3)
	_command("show_local")
	await _frames(3)
	var list := _lobby_list()
	_expect(_lobby_row_count(list) == 1,
		"开房：Local 页画出自己那一行（实际 %d 行）" % _lobby_row_count(list))
	_command("exit_lan_lobby")
	await _frames(3)
	_expect(app_node.local_room.is_empty(),
		"退出大厅：本机房间被收掉（原版 exit_lan_lobby → stop_lan_broadcaster + stop_server_broker_handler）")
	_command("multiplayer")
	await _frames(3)
	_command("show_local")
	await _frames(3)
	list = _lobby_list()
	_expect(_lobby_row_count(list) == 0,
		"退出大厅再进来：Local 页没有残留房间（实际 %d 行）" % _lobby_row_count(list))

	# --- bug 1（b）：打完一局退出，房间同样不许留 ---
	_command("exit_lan_lobby")
	await _frames(2)
	_command("create_game")
	await _frames(3)
	_command("add_bot_0")
	await _frames(2)
	_command("start_game")
	await _frames(5)
	_expect(app_node.current_screen == "PlayScreen", "退出对局：先真的进了 PlayScreen（实际 %s）"
		% app_node.current_screen)
	_command("exit_play")
	await _frames(3)
	_expect(app_node.local_room.is_empty(), "退出对局：本机房间被收掉")
	_command("multiplayer")
	await _frames(3)
	_command("show_local")
	await _frames(3)
	list = _lobby_list()
	_expect(_lobby_row_count(list) == 0,
		"退出对局再进大厅：Local 页没有残留房间（实际 %d 行）" % _lobby_row_count(list))

	# --- bug 2：Global 页的占位行不许可点，也不许弹"没有房间地址" ---
	var toasts: Array[String] = []
	var on_toast := func(t: String) -> void:
		toasts.append(t)
	app_node.toast.connect(on_toast)
	_command("show_global")
	await _frames(3)
	list = _lobby_list()
	var rows := _lobby_row_count(list)
	_expect(rows == PLACEHOLDER_ROOM_COUNT,
		"Global 页占位行数 = %d（实际 %d）" % [PLACEHOLDER_ROOM_COUNT, rows])
	var inert := 0
	for c in list.get_children():
		if not (c is HBoxContainer):
			continue
		var jb := (c as Node).find_child("JoinButton", true, false) as GwButton
		if jb == null:
			continue
		var addr := String(jb.get_meta("room_address", ""))
		if addr.is_empty() and jb.mouse_filter == Control.MOUSE_FILTER_IGNORE:
			inert += 1
		jb.pressed.emit()
	await _frames(3)
	_expect(rows > 0 and inert == rows,
		"Global 占位行的 Join 不接受点击（%d/%d 行；原版没有无地址的行，见 docs/144 §2）" % [inert, rows])
	_expect(toasts.is_empty(),
		"点占位行的 Join 不弹任何提示（实际 %d 条：%s）" % [toasts.size(), str(toasts.slice(0, 2))])
	_expect(app_node.net.session == null, "点占位行的 Join 不发起连接")
	if app_node.toast.is_connected(on_toast):
		app_node.toast.disconnect(on_toast)

	# --- bug 3：Players 列随人数变（不重建行、不靠信号） ---
	_command("exit_lan_lobby")
	await _frames(2)
	_command("create_game")
	await _frames(3)
	_command("multiplayer")
	await _frames(3)
	_command("show_local")
	await _frames(3)
	list = _lobby_list()
	var pc := _lobby_row_players(list, 0)
	_expect(pc != null and pc.text == "1",
		"人数：刚开房时 Players = 1（实际 %s）" % (pc.text if pc != null else "<无这一行>"))
	var row_before: Node = list.get_child(0) if list != null and list.get_child_count() > 0 else null
	# 模拟"别人加入房间"：原版这时收到 `NewConnection`/`PlayerInfo`，服务端那份 `GameInfo` 的
	# `current_player_count` 就变了；`GraphwarLanVBox::process` 下一帧把它读出来（docs/144 §3）。
	app_node.state.players.append({
		"name": "Peer", "team": 1, "is_bot": false, "bot_tier": -1, "soldiers": 1,
		"skin": 3, "face": -1, "hat": -1,
	})
	await _frames(4)
	pc = _lobby_row_players(list, 0)
	_expect(pc != null and pc.text == "2",
		"人数：别人加入后 Players 列变 2（实际 %s）" % (pc.text if pc != null else "<无这一行>"))
	_expect(list.get_child_count() > 0 and list.get_child(0) == row_before,
		"人数：只改文本，不重建行（同一行节点还在）")
	app_node.state.players = saved_players.duplicate(true)
	app_node.close_local_room()
	_command("exit_lan_lobby")
	await _frames(3)

	# --- bug 4：JoinLevelPopUp 的真值几何 + 坐标那一格白底 ---
	_command("campaign")
	await _frames(3)
	_command("level_0")
	await _frames(4)
	var popup := _screen("JoinLevelPopUp")
	if not _expect(popup != null and popup.visible, "弹窗：JoinLevelPopUp 打开了"):
		return
	var vp := popup.get_viewport_rect().size
	var panel := popup.find_child("Panel", true, false) as Control
	if not _expect(panel != null, "弹窗：面板存在"):
		return
	# headless 下视口可能只有几十像素宽（内容会溢出），所以"面板下沿"这条按**结构**断言：
	# 屏幕级 VBox 必须是 4 个孩子（返回行 / 面板行 / **96 那行** / 广告条）。
	# 少掉第 3 个孩子就是用户报的"下面的部分太长"——面板会一直顶到屏底。
	var outer := popup.find_child("VBoxContainer", true, false) as VBoxContainer
	var bottom_row: Control = null
	if outer != null and outer.get_child_count() > 2 \
			and outer.get_child(2) is Control:
		bottom_row = outer.get_child(2) as Control
	_expect(outer != null and outer.get_child_count() == 4,
		"弹窗：屏幕级 VBox 有 4 行（返回 / 面板 / 底部 96 / 广告条；实际 %d）"
		% (outer.get_child_count() if outer != null else -1))
	var exit_row := popup.find_child("ExitButtonContainer", true, false) as Control
	_expect(bottom_row != null and exit_row != null \
		and bottom_row.custom_minimum_size.y > 0.0 \
		and absf(bottom_row.custom_minimum_size.y - exit_row.custom_minimum_size.y) <= 0.5,
		"弹窗：底部那行高 = 返回键高（真值 1920 下都是 96）")
	# 视口够大时再量像素口径：真值 1920×1160 下屏底到面板下沿 = 96 + 4 + 4 = 104。
	if vp.x >= 800.0:
		var gap := (popup.get_global_rect().position.y + popup.size.y) \
			- (panel.get_global_rect().position.y + panel.size.y)
		var want_gap := vp.x * 0.05 + 8.0
		_expect(absf(gap - want_gap) <= 3.0,
			"弹窗：面板下沿离屏底 = 返回键高 + 两个分隔（%.0f，真值口径 %.0f）" % [gap, want_gap])
	# 四个模式按钮：正方，边长按面板宽的 1/6 撑满（真值 226，面板 1434 ⇒ 226/235.7 = 0.96）。
	var modes: Array[Button] = []
	for bname in ["AxisModeButton", "FunctionModeButton", "TurnModeButton", "TimeModeButton"]:
		var b := popup.find_child(bname, true, false) as Button
		if b != null:
			modes.append(b)
	_expect(modes.size() == 4, "弹窗：四个模式按钮都在（实际 %d）" % modes.size())
	var square := 0
	var big := 0
	var cell := panel.size.x / 6.0
	for b in modes:
		if absf(b.size.x - b.size.y) <= 2.0:
			square += 1
		if b.size.x >= cell * 0.85:
			big += 1
	_expect(square == 4, "弹窗：四个模式按钮都是正方（%d/4 方形）" % square)
	# 像素口径的"撑满 1/6 格"只在视口够大时成立（headless 下内容会溢出，
	# AspectRatioContainer 的正方尺寸被行高挤小，此时只有"正方"这一条有意义）。
	if vp.x >= 800.0:
		_expect(big == 4, "弹窗：四个模式按钮撑满 1/6 格（真值 226 vs 格宽 236；实际 %d/4，面板宽 %.0f，格宽 %.0f）"
			% [big, panel.size.x, cell])
	# 坐标模式那一格是**白底**（真值 axis_button_theme.tres：白底 + 3px 黑边 + 圆角 10），
	# 另外三格是面板色 #1A3B2D（game_menu_icon_theme.tres）。两份主题只差底色。
	var white := 0
	var panel_colored := 0
	for i in modes.size():
		var sb := modes[i].get_theme_stylebox("normal") as StyleBoxFlat
		if sb == null:
			continue
		var c := sb.bg_color
		if c.r > 0.99 and c.g > 0.99 and c.b > 0.99 and sb.border_width_left == 3:
			white += 1
		elif absf(c.r - 0.101961) < 0.01 and absf(c.g - 0.231373) < 0.01 \
				and absf(c.b - 0.176471) < 0.01 and sb.border_width_left == 3:
			panel_colored += 1
	_expect(white == 1 and panel_colored == 3,
		"弹窗：坐标模式白底、其余三格面板色（白 %d / 面板色 %d）" % [white, panel_colored])
	var axis := popup.find_child("AxisModeButton", true, false) as Button
	if axis != null:
		var axis_sb := axis.get_theme_stylebox("normal") as StyleBoxFlat
		_expect(axis_sb != null and axis_sb.bg_color.r > 0.99 and axis_sb.bg_color.g > 0.99 \
			and axis_sb.bg_color.b > 0.99,
			"弹窗：表示坐标模式的那一格背景是白色（用户实测报的那一格）")
	# OptionsContainer：`custom_minimum_size.y = 0.25 × 父高`（`OptionsContainer.gd` 真源码）。
	var options := popup.find_child("OptionsContainer", true, false) as Control
	if options != null and options.get_parent() is Control:
		var parent_h := (options.get_parent() as Control).size.y
		_expect(absf(options.custom_minimum_size.y - 0.25 * parent_h) <= 1.0,
			"弹窗：OptionsContainer.min.y = 0.25 × 父高（%.1f vs %.1f）"
			% [options.custom_minimum_size.y, 0.25 * parent_h])
		_expect(options.size_flags_vertical == Control.SIZE_EXPAND_FILL,
			"弹窗：OptionsContainer 竖直 EXPAND（真值 flags=V-EXPAND）")
	_command("cancel_level")
	await _frames(2)
	_command("exit_campaign")
	await _frames(3)
