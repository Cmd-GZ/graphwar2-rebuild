extends Node
## App —— 全局单例（autoload）。
##
## 职责划分（对照原版，见 docs/108 §2）：
##   * 原版：`GraphwarRootNode`（Rust）持有 15 个界面、按 `GraphwarButton.input_text`
##     分发命令、维护房间/玩家/模式状态。
##   * 本项目：`App` 做同一件事，只不过用 GDScript + 明写的命令表（`commands.gd`）。
##     真正的规则计算仍然交给 Rust 内核（`core_bridge.gd` 是接缝）。

signal screen_shown(screen_name: String)
signal toast(text: String)
signal state_changed

## 7 个"基础界面"：同一时刻只显示一个。
const BASE_SCREENS := [
	"MainMenu", "PlayScreen", "Campaign", "LanLobby", "GameSetup", "PlayerScreen", "ReplayLobby",
]
## 8 个"弹窗"：叠加在基础界面之上（原版里这 15 个也都是 `ScreensControl` 的子节点，
## 靠 visible 切换，见 docs/92 §3）。7 + 8 = 15，与原版界面数一致。
const POPUPS := [
	"ChatPopUp", "JoinIpPopUp", "ConnectingPopUp", "DisconnectedPopUp",
	"JoinLevelPopUp", "AddBotPopUp", "EndGamePopUp", "ConfirmPlayExitPopUp",
]

var store: GwStore
var state: GwGameState

var screens := {}
var current_screen := ""
var open_popups: Array[String] = []

## 联机大厅的"本地/公网"开关（观测：ShowLocalButton 默认选中，docs/92 §4.4）。
var room_list_mode := "local"
## 当前在 JoinLevelPopUp 里预览的关卡号。
var campaign_selected_level := 0
## JoinIpPopUp 里最后一次输入的地址。
var pending_join_ip := ""
## 最近一局的胜负（由 PlayScreen 调 report_match_result 写入，结算框读它）。
var last_result := {}
## 自检/自动化模式下不弹"模拟连接"计时器（避免阻塞 headless 退出）。
var automation := false

const _DISCONNECT_DELAY := 1.2


func _ready() -> void:
	store = GwStore.new()
	store.load_all()
	state = GwGameState.new()
	state.reset_lan()
	automation = OS.get_cmdline_user_args().has("--selftest") or OS.get_cmdline_user_args().has("--dump-tree")

# ------------------------------------------------------------------ 注册与路由

func register_screen(screen_name: String, node: Control) -> void:
	screens[screen_name] = node
	node.visible = false

func is_popup(screen_name: String) -> bool:
	return POPUPS.has(screen_name)

## 切到某个基础界面（原版语义：把其它基础界面的 visible 置 0）。
func show_screen(screen_name: String) -> bool:
	if not screens.has(screen_name):
		push_error("[gw2] 未知界面：%s" % screen_name)
		return false
	if not BASE_SCREENS.has(screen_name):
		push_error("[gw2] %s 不是基础界面，用 open_popup()" % screen_name)
		return false
	for n in BASE_SCREENS:
		var node: Control = screens.get(n)
		if node != null:
			node.visible = (n == screen_name)
	current_screen = screen_name
	# 换基础界面时把弹窗全关掉（原版行为未逐条观测；这里取"不回叠"的简单语义）
	for p in open_popups.duplicate():
		close_popup(p)
	_on_screen_entered(screen_name)
	screen_shown.emit(screen_name)
	return true

func open_popup(screen_name: String) -> void:
	if not screens.has(screen_name):
		push_error("[gw2] 未知弹窗：%s" % screen_name)
		return
	var node: Control = screens[screen_name]
	node.visible = true
	if not open_popups.has(screen_name):
		open_popups.append(screen_name)
	_on_screen_entered(screen_name)

func close_popup(screen_name: String) -> void:
	if screens.has(screen_name):
		(screens[screen_name] as Control).visible = false
	open_popups.erase(screen_name)

func close_all_popups() -> void:
	for p in open_popups.duplicate():
		close_popup(p)

func _on_screen_entered(screen_name: String) -> void:
	var node: Control = screens.get(screen_name)
	if node != null and node.has_method("on_enter"):
		node.call("on_enter")

func notify(text: String) -> void:
	toast.emit(text)

## 供界面读取"我要刷新的数据变了"。
func changed() -> void:
	state_changed.emit()

func goto(screen_name: String) -> bool:
	return show_screen(screen_name)

# ------------------------------------------------------------------ 命令总线

## 唯一的命令入口。所有 `GwButton.input_text` 都灌到这里。
## 命令键清单与实现状态见 `commands.gd`（✅/🔶/❌ 三档）。
func command(cmd: String) -> void:
	if cmd == "":
		return
	if cmd.begins_with(GwCommands.ADD_BOT_PREFIX):
		_add_bot(int(cmd.substr(GwCommands.ADD_BOT_PREFIX.length())))
		return
	if cmd.begins_with(GwCommands.LEVEL_PREFIX):
		_select_level(int(cmd.substr(GwCommands.LEVEL_PREFIX.length())))
		return
	# 键盘：只在 PlayScreen 有意义（原版由 Rust 的 GraphwarFunctionInput 处理）
	if current_screen == "PlayScreen" and _is_keypad_key(cmd):
		var play: Control = screens.get("PlayScreen")
		if play != null and play.has_method("key_input"):
			play.call("key_input", cmd)
		return

	match cmd:
		# --- 主菜单 ---
		GwCommands.CAMPAIGN:
			_enter("Campaign")
		GwCommands.PLAYER_SETTINGS:
			_enter("PlayerScreen")
		GwCommands.MULTIPLAYER:
			_enter("LanLobby")
		GwCommands.REPLAY:
			_enter("ReplayLobby")
		# --- 返回 ---
		GwCommands.EXIT_CAMPAIGN, GwCommands.EXIT_LAN_LOBBY, GwCommands.EXIT_GAME_SETUP, \
		GwCommands.EXIT_PLAYER_SETTINGS, GwCommands.EXIT_REPLAY_LOBBY, GwCommands.EXIT_PLAY:
			_leave_to_main_menu()
		# --- 联机大厅 ---
		GwCommands.SHOW_LOCAL:
			room_list_mode = "local"
			_refresh("LanLobby")
		GwCommands.SHOW_GLOBAL:
			room_list_mode = "global"
			_refresh("LanLobby")
			notify("公网房间列表来源未知（docs/92 §8）：这里显示的是静态占位条目")
		GwCommands.CREATE_GAME:
			state.reset_lan()
			state.add_human(store.profile.name, store.profile.skin, store.profile.face, store.profile.hat)
			changed()
			_enter("GameSetup")
		GwCommands.JOIN_IP:
			open_popup("JoinIpPopUp")
		GwCommands.JOIN_JOIN_IP:
			_submit_join_ip()
		GwCommands.CANCEL_JOIN_IP:
			close_popup("JoinIpPopUp")
		GwCommands.CANCEL_CONNECTING:
			close_popup("ConnectingPopUp")
			close_popup("DisconnectedPopUp")
		GwCommands.OK_DISCONNECTED:
			close_popup("DisconnectedPopUp")
		# --- 对局设置 ---
		GwCommands.LOCK_GAME:
			state.locked = not state.locked
			_refresh("GameSetup")
		GwCommands.ADD_BOT:
			open_popup("AddBotPopUp")
		GwCommands.CANCEL_ADD_BOT:
			close_popup("AddBotPopUp")
		GwCommands.START_GAME:
			_start_game()
		GwCommands.TOGGLE_CHAT:
			if open_popups.has("ChatPopUp"):
				close_popup("ChatPopUp")
			else:
				open_popup("ChatPopUp")
		# --- 关卡 ---
		GwCommands.START_LEVEL:
			_start_level()
		GwCommands.CANCEL_LEVEL:
			close_popup("JoinLevelPopUp")
		# --- 对局中 ---
		GwCommands.ASK_EXIT_PLAY:
			open_popup("ConfirmPlayExitPopUp")
		GwCommands.CANCEL_EXIT_PLAY:
			close_popup("ConfirmPlayExitPopUp")
		GwCommands.TOGGLE_NAMES:
			_play_toggle("toggle_names")
		GwCommands.TOGGLE_VIEW_FUNCTIONS:
			_play_toggle("toggle_view_functions")
		GwCommands.TOGGLE_MUTE:
			_play_toggle("toggle_mute")
			notify("静音只切图标：本项目没有音频资源（docs/108 §7）")
		GwCommands.FIRE:
			_play_call("fire")
		GwCommands.PLAY_REPLAY:
			_play_replay()
		GwCommands.PLAY_PAUSE:
			notify("回放播放未实现（回放文件格式见 docs/78，本项目不解析）")
		GwCommands.KEY_PREV, GwCommands.KEY_NEXT:
			notify("回放跳转未实现")
		GwCommands.SAVE_SCREENSHOT, GwCommands.SAVE_SUMMARY, GwCommands.SAVE_CLIP:
			notify("这三个按钮的原版语义未观测（只拿到键名 %s）" % cmd)
		GwCommands.UNLOCK_SKINS:
			notify("内购未实现：Linux 端没有广告/内购（用户要求）")
		_:
			notify("未接的命令：%s" % cmd)

func _is_keypad_key(cmd: String) -> bool:
	var keys := [
		"C", "π", "e", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", ".", "(", ")",
		"+", "-", "*", "/", "^", "x", "y", "y'", "abs(", "ln(", "sqrt(", "sin(", "cos(",
		"tan(", "exp(", "⌫", "⌦", "←", "→", "⇤", "⇥", "e",
	]
	return keys.has(cmd)

# ------------------------------------------------------------------ 各命令的实现

func _enter(screen_name: String) -> void:
	show_screen(screen_name)

func _leave_to_main_menu() -> void:
	if current_screen == "PlayScreen":
		var play: Control = screens.get("PlayScreen")
		if play != null and play.has_method("end_match"):
			play.call("end_match")
	show_screen("MainMenu")

func _refresh(screen_name: String) -> void:
	var node: Control = screens.get(screen_name)
	if node != null and node.has_method("on_enter"):
		node.call("on_enter")

func _select_level(index: int) -> void:
	campaign_selected_level = clampi(index, 0, GwCampaign.LEVEL_COUNT - 1)
	open_popup("JoinLevelPopUp")

func _add_bot(tier: int) -> void:
	if state.players.size() >= (GwGameState.MAX_PLAYERS_PER_TEAM * 2):
		notify("玩家/机器人已满（每队 %d）" % GwGameState.MAX_PLAYERS_PER_TEAM)
		return
	state.add_bot(tier)
	changed()
	close_popup("AddBotPopUp")
	_refresh("GameSetup")
	notify("加入 %s（档 %d）" % [GwGameState.BOT_NAMES[clampi(tier, 0, 5)], tier])

func _start_game() -> void:
	if not state.can_start():
		# 原版行为：不加 Bot 直接 Start 会**静默无操作**（docs/22 §5.1）。
		# 这里保留"不开始"，但额外给一句提示（有意偏离，写在 docs/108 §7）。
		notify("还需要至少一个对手：先用 AddBot 加一个 Bot（原版此处是静默无操作）")
		return
	_begin_match()

func _start_level() -> void:
	var lv := GwCampaign.level(campaign_selected_level)
	if not store.is_level_unlocked(int(lv.index)):
		notify("第 %d 关尚未解锁（原版存档 user://campaign.save 里没通到这一关）" % int(lv.index))
		return
	state.reset_campaign(int(lv.index))
	state.axis_mode = int(lv.axis)
	state.function_mode = int(lv.function)
	state.turn_mode = int(lv.turn)
	state.time_mode = int(lv.time)
	# 士兵数与 Bot 难度档**来自真数据**（docs/118 §7.1 的 S/C/R 三源对照表）：
	# 前 16 关的对手关（5/9/11/14/15/18/19/20）各 1 个 Bot，档位 0/2/3/3/4/3/3/4；
	# 其余关卡没有对手，只有靶子（靶子由 PlayScreen 绘制）。
	state.add_human(store.profile.name, store.profile.skin, store.profile.face, store.profile.hat)
	state.players[state.players.size() - 1]["soldiers"] = int(lv.soldiers)
	if int(lv.bot_tier) >= 0:
		state.add_bot(int(lv.bot_tier))
	close_popup("JoinLevelPopUp")
	_begin_match()

func _begin_match() -> void:
	show_screen("PlayScreen")
	var play: Control = screens.get("PlayScreen")
	if play != null and play.has_method("begin_match"):
		play.call("begin_match")

func _play_call(method: String) -> void:
	var play: Control = screens.get("PlayScreen")
	if play != null and play.has_method(method):
		play.call(method)
	else:
		notify("当前不在对局里")

func _play_toggle(method: String) -> void:
	_play_call(method)

func _play_replay() -> void:
	var lobby: Control = screens.get("ReplayLobby")
	if lobby != null and lobby.has_method("selected_replay"):
		state.reset_replay(lobby.call("selected_replay"))
	notify("回放播放未实现：只进 PlayScreen 的空回放模式（docs/108 §7）")
	_begin_match()

func _submit_join_ip() -> void:
	var popup: Control = screens.get("JoinIpPopUp")
	var raw := pending_join_ip.strip_edges()
	if popup != null and popup.has_method("current_text"):
		raw = String(popup.call("current_text")).strip_edges()
	# 观测（docs/92 §4.7）：地址**必须带端口**，只写 IP 时原版毫无反应。
	if not _looks_like_host_port(raw):
		notify("地址要写成 host:port（原版只写 IP 时同样没有反应）")
		return
	pending_join_ip = raw
	close_popup("JoinIpPopUp")
	open_popup("ConnectingPopUp")
	if automation:
		return
	await get_tree().create_timer(_DISCONNECT_DELAY).timeout
	if screens.has("ConnectingPopUp") and (screens["ConnectingPopUp"] as Control).visible:
		close_popup("ConnectingPopUp")
		open_popup("DisconnectedPopUp")

static func _looks_like_host_port(s: String) -> bool:
	var idx := s.rfind(":")
	if idx <= 0 or idx == s.length() - 1:
		return false
	var port := s.substr(idx + 1)
	if not port.is_valid_int():
		return false
	var p := port.to_int()
	if p < 1 or p > 65535:
		return false
	return not s.substr(0, idx).strip_edges().is_empty()

# ------------------------------------------------------------------ 对局结果

## 由 PlayScreen 在"一方全灭"时调用；战役模式下推进解锁。
func report_match_result(human_won: bool, team_won: int) -> void:
	last_result = {"human_won": human_won, "winner_team": team_won}
	if state.kind == GwGameState.Kind.CAMPAIGN and human_won:
		store.mark_completed(state.level_index)
		notify("第 %d 关通关，已解锁第 %d 关" % [state.level_index, state.level_index + 1])
	open_popup("EndGamePopUp")

## 结算框里的展示数据（EndGamePopUp 读它）。
func match_summary() -> Dictionary:
	var human_team := 0
	for p in state.players:
		if not p.is_bot:
			human_team = int(p.team)
			break
	return {
		"players": state.players,
		"human_team": human_team,
		"human_won": bool(last_result.get("human_won", false)),
		"winner_team": int(last_result.get("winner_team", 0)),
		"kind": state.kind,
		"level": state.level_index,
		"room_name": state.room_name,
	}
