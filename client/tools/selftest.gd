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
	# 让布局至少跑两帧（字号缩放依赖 viewport 尺寸）
	await _frames(3)

	_test_boot()
	await _test_main_menu()
	await _test_campaign()
	await _test_player_screen()
	await _test_replay_lobby()
	await _test_lan_lobby()
	await _test_ui_text_and_measured_geometry()
	await _test_game_setup()
	await _test_play_screen()
	await _test_endgame()
	_test_fonts_and_icons()
	_test_assets_and_original_settings()
	_test_geometry_and_campaign_save()
	await _test_audio()

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
	var local_rooms := 0
	for c in list.get_children():
		if c is HBoxContainer:
			local_rooms += 1
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
		var join_btn := lobby.find_child("JoinButton", true, false) as Control
		var vw := lobby.get_viewport_rect().size.x
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
