extends Node
## App —— 全局单例（autoload）。
##
## 职责划分（对照原版，见 docs/108 §2）：
##   * 原版：`GraphwarRootNode`（Rust）持有 15 个界面、按 `GraphwarButton.input_text`
##     分发命令、维护房间/玩家/模式状态。
##   * 本项目：`App` 做同一件事，只不过用 GDScript + 明写的命令表（`commands.gd`）。
##     真正的规则计算仍然交给 Rust 内核（`core_bridge.gd` 是接缝）。
##   * 音频：`GwAudio`（`client/src/audio/gw_audio.gd`）是本节点的子节点，`App.audio`。
##     `App` 负责两件事：**静音的双向同步**（`store.muted` ↔ Master 总线）和
##     **把已知的时机分发给播放层**。事件↔音效的原版映射**仍未确定**（`docs/111` §5.1、
##     `docs/128` §3），所以下面凡是"哪个动作放哪个音"的接线都打了
##     `PENDING_MAPPING` 标记并在 `docs/128` §4 逐条注明"**自定，非原版**"。

signal screen_shown(screen_name: String)
signal toast(text: String)
signal state_changed
## 播放层真的播出了一个音效（自检/证据链用；不代表原版在这里放过这个音）。
signal sfx_played(sound: String, slot: String)

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
## 播放层（`client/src/audio/gw_audio.gd`）。全局只有这一个实例。
var audio: GwAudio

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
## 自检/自动化模式标记（headless 下有些交互要跳过）。**不再用来跳过联机**：
## 连接是真的 TCP（见 `docs/134`），自检里也走同一条路径。
var automation := false

# ---------------------------------------------------------------- 联机层（docs/134）
#
# 原版的联机在 Rust 侧（`graphwar_backend.rs` 15 个方法 + `graphwar_client_connection_handler.rs`），
# 本项目把它做成 `client/src/net/` 里的一组 GDScript（`App.net`）：
#   lan / broker / session 三通道，分帧一律 `u16` 大端 + JSON（`docs/24` §1）。
## 联机层。`_ready` 里建；为空说明没建起来（所有联机分支会走"不可用"）。
var net: GwNet
## 当前是否在**联机对局**里。置真的唯一入口是 `GwNet.joined`。
var online := false
## 联机对局里服务端那份快照（`GwNetSession.snapshot()`）。
##
## ⚠️ PlayScreen 目前**还没有**读它：它的士兵/障碍是按本地规则摆的
## （`play_screen.gd::begin_match` 里"非战役 ⇒ 没有障碍"那条注释）。联机要真正同屏，
## PlayScreen 侧需要改读这个字典 —— 交接内容在 `docs/134` §6。
var online_room := {}
## 最近一次联机错误（人话；自检与排障读它，不显示在界面上）。
var net_error := ""

## 本机开着的房间（`{}` = 没开）。
##
## # 为什么要单独一个对象（docs/144 §1）
##
## 原版的大厅列表**不是缓存**：`GraphwarLanVBox::process` 每帧从两个活的数据源
## （`GraphwarLanBroadcasterV6` / `GraphwarServerBrokerHandler` 各自的 `GameInfo` 表）
## 里重取一次，取不到就把那一行删掉。我们这一侧没有广播器（`src/` 不动，`docs/134`），
## 所以"自己的房间"只能由这个对象代表；它的生命周期就是原版
## `start_lan_broadcaster()` / `stop_lan_broadcaster()` 的生命周期：
## `create_game` 时开、离开大厅或对局时收（`exit_lan_lobby` 那一支会
## `stop_lan_broadcaster() + stop_server_broker_handler()`，见 docs/144 §1）。
##
## **以前没有这个对象**：大厅直接用 `state.players` 拼一行"我的房间"，而 `state.players`
## 是**对局状态**、退出对局时不清 ⇒ 上一把的房间一直挂在列表里（用户实测 bug）。
var local_room := {}

# ---------------------------------------------------------------- 音频接线
#
# ⚠️ **原版的"事件↔音效"映射是未知的**（`docs/111` §5.1）。本轮的补充证据只到
# "`0x4bf090` 是 `effect_info_event`（tag 44）的处理函数，按 `event[+0x20]`（= `effect_type`）
# 四臂分派"（`docs/128` §3），**证不到**哪个 `effect_type` 落哪个音效。
#
# 所以下面这三条是**本客户端自己的接线**（`docs/128` §4 表里逐条标 "自定/推断，非原版"）：
# 它们只为"让音频链路真的跑起来并可被验证"，一旦原版映射解出来就按表替换。
# 任何一条都不许在文档里写成"原版行为"。
const PENDING_MAPPING := true         # 恒为 true：提醒读者这些接线还没有原版依据
const SFX_FIRE := "explosion"         # 开火（一次 fire 内就结算完，命中/爆炸都在里面）
const SFX_WIN := "unpop"
const SFX_LOSE := "squash"


func _ready() -> void:
	store = GwStore.new()
	store.load_all()
	state = GwGameState.new()
	state.reset_lan()
	automation = OS.get_cmdline_user_args().has("--selftest") or OS.get_cmdline_user_args().has("--dump-tree")
	_install_audio()
	_install_net()

# ------------------------------------------------------------------ 联机层

## 建联机层并接三条通道（`client/src/net/`）。
##
## LAN 发现一启动就绑 UDP 42968（原版的观测端口）。**绑定失败不算错**：
## 同一台机器再开一个客户端 / 原版客户端正在跑，都会占住这个端口，
## 那时只是"发现不到房间"，手动 Join IP 仍然可用（`docs/134` §5）。
func _install_net() -> void:
	net = GwNet.new()
	add_child(net)
	net.joined.connect(_on_net_joined)
	net.join_failed.connect(_on_net_join_failed)
	net.disconnected.connect(_on_net_disconnected)
	net.game_started.connect(_on_net_game_started)
	net.ended.connect(_on_net_ended)
	net.rooms_changed.connect(_on_net_rooms_changed)
	var err := net.lan_listen(GwLanDiscovery.DEFAULT_PORT)
	if err != OK:
		print("[gw2/net] LAN 发现未启用：%s" % String(net.lan.last_error))

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
	if screen_name == "PlayScreen":
		_apply_mute_to_screen()
		_align_play_mute_once()

## 用户可见文本里**不许**出现内部标记。这类事故已经出现过三次
## （`advertisement slot — disabled in this build`、`战役第 0 关（占位关卡数据…）`、
## 联机大厅底部那句 `公网房间列表来源未知（docs/92 §8）`），所以做成运行期自检：
## 每条气泡在发出前先扫一遍，命中就**不发**、记进 `internal_text_violations` 并报错。
## 自检里断言这个列表为空（`selftest.gd`），于是"往气泡里写内部说明"会当场 fail。
const INTERNAL_MARKERS := ["docs/", "§", "TODO", "占位", "未观测", "原版"]

## 本次进程里被拦下的气泡（去重、保序）。自检断言为空。
static var internal_text_violations: Array[String] = []

static func internal_markers_in(text: String) -> Array[String]:
	var hits: Array[String] = []
	for m in INTERNAL_MARKERS:
		if text.find(m) >= 0:
			hits.append(m)
	return hits

func notify(text: String) -> void:
	var hits := internal_markers_in(text)
	if not hits.is_empty():
		if not internal_text_violations.has(text):
			internal_text_violations.append(text)
		push_error("[gw2] 气泡文本里出现内部标记 %s：%s" % [str(hits), text])
		return
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
			# 公网列表 = broker 的房间表（`docs/115`）。地址没配就**不连**
			# （原版默认从 graphwar.com 取，本项目不发外网请求），此时页面仍显示占位行。
			if net != null:
				net.browse_global()
			_refresh("LanLobby")
		GwCommands.CREATE_GAME:
			state.reset_lan()
			state.add_human(store.profile.name, store.profile.skin, store.profile.face, store.profile.hat)
			# 原版这一支是 `start_server() + start_lan_broadcaster() + start_server_broker_handler()`
			# （`GraphwarRootNode::process_button_pressed` 的 create_game 分支，docs/144 §1）：
			# 房间从这一刻开始存在，并进 LAN 表/公网表。
			_open_local_room()
			changed()
			_enter("GameSetup")
		GwCommands.JOIN_IP:
			open_popup("JoinIpPopUp")
		GwCommands.JOIN_JOIN_IP:
			_submit_join_ip()
		GwCommands.CANCEL_JOIN_IP:
			close_popup("JoinIpPopUp")
		GwCommands.CANCEL_CONNECTING:
			# 取消 = 真的把那条 TCP 连接放掉（`docs/134` §4）。以前这里只是关窗。
			_cancel_connect()
		GwCommands.OK_DISCONNECTED:
			close_popup("DisconnectedPopUp")
			# 断线后还留在对局界面是没有意义的（那边没有任何服务器数据可显示）：
			# 回主菜单。**本轮自定**，原版断线后停在哪一屏未观测。
			if current_screen == "PlayScreen" and not online:
				_leave_to_main_menu()
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
			toggle_mute()
		GwCommands.FIRE:
			_play_call("fire")
			# PENDING_MAPPING：原版在 `EffectInfo(ExplosionEffect)` 时播某个音效
			# （`0x3a5760` → `0x4bfe10`），但**哪个音**未知；这里先用我们自己的选择。
			_sfx(SFX_FIRE)
		GwCommands.PLAY_REPLAY:
			_play_replay()
		GwCommands.PLAY_PAUSE:
			# 回放已能播（docs/133）；播放/暂停由 GwReplayPlayer 自己接管，
			# 这里不再弹"未实现"的假提示。
			pass
		GwCommands.KEY_PREV, GwCommands.KEY_NEXT:
			notify("回放跳转未实现")
		GwCommands.SAVE_SCREENSHOT, GwCommands.SAVE_SUMMARY, GwCommands.SAVE_CLIP:
			notify("这个按钮还没有接上")
		GwCommands.UNLOCK_SKINS:
			notify("内购在本构建里不可用")
		_:
			notify("这个按钮还没有接上")

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
		# 离开对局 = 放掉那条联机连接（不然服务端会一直等我们的 `FunctionFireRequest`）。
		if net != null:
			net.leave()
		online = false
		online_room = {}
	if current_screen == "LanLobby" and net != null:
		# 出大厅就不该继续占着 broker 连接（LAN 监听留着，代价只是收包）。
		net.stop_broker()
	# 本机房间跟着大厅一起收掉。原版 `exit_lan_lobby` 那一支就是
	# `stop_lan_broadcaster() + stop_server_broker_handler()`（docs/144 §1）：
	# 广播停了 ⇒ 自己那一行从 LAN 表里消失 ⇒ 下一次进大厅列表是空的。
	close_local_room()
	show_screen("MainMenu")

## 开一间本机房间。`create_game` 调它。
func _open_local_room() -> void:
	local_room = {
		"room_name": state.room_name,
		"address": "",
		"game_state": "Setup",
	}

## 收掉本机房间（原版 `stop_lan_broadcaster()` 的等价物）。幂等。
func close_local_room() -> void:
	local_room = {}

## 大厅要画的那一行"我的房间"。**人数是实时读的**（原版每帧从 `GameInfo` 重取，
## 不是建表时拍一张快照 —— docs/144 §3）。返回 `{}` = 没开房间。
func local_room_row() -> Dictionary:
	if local_room.is_empty():
		return {}
	var row := local_room.duplicate()
	row["players"] = state.players.size()
	return row

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
		notify("至少还需要一个对手：先加一个 Bot")
		return
	_begin_match()

func _start_level() -> void:
	var lv := GwCampaign.level(campaign_selected_level)
	if not store.is_level_unlocked(int(lv.index)):
		notify("第 %d 关尚未解锁" % int(lv.index))
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
	_begin_match()

# ------------------------------------------------------------------ 联机：加入一台服务器

## `JoinIpPopUp` 的"Join"：校验 → 关窗 → `ConnectingPopUp` → **真的连**。
##
## 以前这里是模拟的（校验通过后 `await create_timer(1.2)` 再弹断线框）。现在的路径：
##
## ```text
## JoinIpPopUp  →（host:port 校验，观测 docs/92 §4.7）→ ConnectingPopUp
##   → GwNetSession.start() → TCP → NewConnectionRequest
##      → 7 条状态同步到齐 ⇒ _on_net_joined() ⇒ PlayScreen（联机模式）
##      → 连不上 / 握手超时 ⇒ _on_net_join_failed() ⇒ DisconnectedPopUp
## ```
func _submit_join_ip() -> void:
	var popup: Control = screens.get("JoinIpPopUp")
	var raw := pending_join_ip.strip_edges()
	if popup != null and popup.has_method("current_text"):
		raw = String(popup.call("current_text")).strip_edges()
	# 观测（docs/92 §4.7）：地址**必须带端口**，只写 IP 时原版毫无反应。
	if not _looks_like_host_port(raw):
		notify("地址要写成 host:port")
		return
	pending_join_ip = raw
	close_popup("JoinIpPopUp")
	connect_to(raw)


## 连一台对局服务器（`host:port`）。返回 false = 地址不合法或联机层没建起来。
##
## 大厅里点房间条目的 Join 也走这里（那一条的地址来自 LAN 广播）。
func connect_to(address: String) -> bool:
	var parsed := GwNet.split_host_port(address)
	if parsed.is_empty():
		notify("地址要写成 host:port")
		return false
	if net == null:
		notify("联机层不可用")
		return false
	# 去连别人的房间 = 自己不再开房（原版 `stop_server()` + `stop_lan_broadcaster()`
	# 就在 `start_client_to_*` 之前，docs/144 §1）。
	close_local_room()
	pending_join_ip = address
	net_error = ""
	close_popup("DisconnectedPopUp")
	open_popup("ConnectingPopUp")
	net.join(String(parsed["host"]), int(parsed["port"]))
	return true


## 取消连接：关窗 + 放掉 socket。已经加入的对局不会被这个命令关掉
## （那时 `ConnectingPopUp` 早就关了，取消键在该屏不可见）。
func _cancel_connect() -> void:
	if net != null:
		net.leave()
	online = false
	online_room = {}
	close_popup("ConnectingPopUp")
	close_popup("DisconnectedPopUp")


# ---------------------------------------------------------------- 联机：信号处理

## 握手完成。把服务端那份房间状态搬进 `state`，然后照常进对局界面。
##
## 搬什么、为什么：
## * 四个模式串（`EveryUnit` / `NormalFunction` / `SequentialTurns` / `Timer1m` …）
##   是**服务端说了算**（`docs/33` §1 的状态同步就是干这个的）；
## * 玩家表从 `SoldierInfo` + `TeamInfo` 反推（`docs/24` §3 的实体分配）；
## * 名字/皮肤/段位要 `NameInfo`/`SkinGraphicsInfo` 才有 —— 我们的服务端**没实现**
##   这一层（`docs/33` §4），所以名字退化成 `Player <id>`（第一个用本机档案名）。
func _on_net_joined() -> void:
	close_popup("ConnectingPopUp")
	online = true
	net_error = ""
	print("[gw2/net] joined %s (connection_id=%d)" % [
		pending_join_ip, net.session.my_connection_id if net.session != null else -1])
	_sync_state_from_net()
	_begin_match()


func _on_net_game_started() -> void:
	if not online:
		return
	# 开局那一批（障碍/士兵/位置/轮到谁）到齐后再刷一次状态。
	# PlayScreen 不在本轮改动范围内，所以这里用**它已有的** `begin_match()` 重建战场；
	# 服务器下发的坐标要真正同屏，还得 PlayScreen 侧改读 `online_room`（docs/134 §6）。
	_sync_state_from_net()
	if current_screen == "PlayScreen":
		var play: Control = screens.get("PlayScreen")
		if play != null and play.has_method("begin_match"):
			play.call("begin_match")


func _on_net_join_failed(reason: String) -> void:
	online = false
	online_room = {}
	net_error = reason
	print("[gw2/net] join failed: %s" % reason)
	close_popup("ConnectingPopUp")
	open_popup("DisconnectedPopUp")


func _on_net_disconnected(reason: String) -> void:
	online = false
	online_room = {}
	net_error = reason
	print("[gw2/net] disconnected: %s" % reason)
	close_popup("ConnectingPopUp")
	open_popup("DisconnectedPopUp")
	if current_screen == "PlayScreen":
		var play: Control = screens.get("PlayScreen")
		if play != null and play.has_method("end_match"):
			play.call("end_match")


## 服务端发 `EndingGame`：**正常结束**，不是断线（`src/graphwar_game_server.rs` 之后会关连接）。
func _on_net_ended(reason: String) -> void:
	print("[gw2/net] match ended (%s)" % reason)
	online = false


func _on_net_rooms_changed(_mode: String) -> void:
	if current_screen == "LanLobby":
		_refresh("LanLobby")


## 把 `GwNetSession.room` 搬到 `state`（并留一份快照给 PlayScreen 侧）。
func _sync_state_from_net() -> void:
	if net == null or net.session == null:
		return
	var room: Dictionary = net.session.room
	online_room = net.session.snapshot()
	state.reset_lan()
	var room_name := String(room.get("room_name", ""))
	state.room_name = room_name if not room_name.is_empty() else "Graphwar Game"
	state.axis_mode = GwNetSession.axis_mode_index(String(room.get("axis_mode", "")))
	state.function_mode = GwNetSession.function_mode_index(String(room.get("function_mode", "")))
	state.turn_mode = GwNetSession.turn_mode_index(String(room.get("turn_mode", "")))
	state.time_mode = GwNetSession.time_mode_index(String(room.get("time_mode", "")))
	var players: Dictionary = room.get("players", {})
	var ids := players.keys()
	ids.sort()
	var first := true
	for pid in ids:
		var p: Dictionary = players[pid]
		var team := int(p.get("team", 0))
		var soldiers := maxi((p.get("soldiers", []) as Array).size(), 1)
		state.players.append({
			"name": store.profile.name if first else "Player %d" % int(pid),
			"team": team,
			"is_bot": false,
			"bot_tier": -1,
			"soldiers": soldiers,
			"skin": int(team) * 3,
			"face": -1,
			"hat": -1,
			"player_id": int(pid),
			"connection_id": int(p.get("connection_id", -1)),
		})
		first = false
	if state.players.is_empty():
		# 服务端还没告诉我们有谁（没开局）：先放自己一个，保证对局界面能建起来。
		state.add_human(store.profile.name, store.profile.skin, store.profile.face, store.profile.hat)
	changed()

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

# ------------------------------------------------------------------ 音频

## 建播放层，并把**存档里的静音**应用上去（`docs/128` §5）。
##
## 原版启动时也做这件事：`0x3a2e10` → `0x4bf550`（载资源）→ `0x4c11d0`（静音初始化，
## `0x3a4413`）。顺序一样：先建播放层，再设静音。
func _install_audio() -> void:
	audio = GwAudio.new()
	audio.name = "Audio"
	add_child(audio)
	audio.played.connect(_on_sfx_played)
	audio.mute_changed.connect(_on_mute_changed)
	# store → bus（原版语义：静音就是 **Master 总线**静音，不是逐 player 调音量）
	audio.set_muted(bool(store.muted))
	_apply_mute_to_screen()

func _on_sfx_played(sound: String, slot: String) -> void:
	sfx_played.emit(sound, slot)

## 播放一个音效。缺资产时 `GwAudio.play()` 返回 false —— 这里**不**提示用户、
## **不**崩（缺资产的入口在 `main.gd` 的硬检查里已经报过）。
func _sfx(sound: String) -> bool:
	if audio == null:
		return false
	return audio.play(sound)

## 当前是否静音（读的是**总线**，不是某个界面里的私有标志）。
func is_muted() -> bool:
	return audio != null and audio.is_muted()

## 切静音。**总线是唯一真相**：界面可以自己翻图标（原版 `play_screen` 就是这么做的），
## 我们以总线的实际变化为准，然后把新状态同步到 `store.muted` 并落盘。
##
## 这个顺序对两种界面实现都成立：
##   * 界面自己写了 `set_bus_mute`（当前 `play_screen.toggle_mute` 的写法）⇒ 总线变了 ⇒ 采纳；
##   * 界面只翻图标、不碰总线（重构后可能出现）⇒ 总线没变 ⇒ 由 `App` 翻转。
func toggle_mute() -> void:
	if audio == null:
		return
	var before := audio.bus_muted()
	var play: Control = screens.get("PlayScreen")
	if play != null and play.has_method("toggle_mute"):
		play.call("toggle_mute")
	if audio.bus_muted() == before:
		audio.set_muted(not before)
	else:
		audio.adopt_bus()
	_apply_mute_to_screen()

## 是否已经跟 PlayScreen 的静音显示对齐过（见 `_align_play_mute_once()`）。
var _play_mute_aligned := false

## 旧 `play_screen.gd` 只有 `toggle_mute()`、没有 `set_muted()`，它的私有 `_muted` 初值是
## false。所以"存档里 muted=true"时，必须在**第一次进 PlayScreen** 时替它翻一次，
## 否则用户按第一下静音键时总线已经是静音的 —— 看起来像没反应。
##
## 这个对齐**只做一次**，而且只在 `is_muted()` 为真时才翻（翻完总线仍是静音，无副作用）。
## 等 `play_screen` 提供 `set_muted()`（另一个 agent 正在重写那四个开关）之后，
## 这段会自动失效（`_apply_mute_to_screen()` 优先用 setter）。
func _align_play_mute_once() -> void:
	var play: Control = screens.get("PlayScreen")
	if play == null or _play_mute_aligned:
		return
	_play_mute_aligned = true
	if play.has_method("set_muted"):
		play.call("set_muted", is_muted())
	elif is_muted() and play.has_method("toggle_mute"):
		play.call("toggle_mute")

## 把静音状态推给界面（只对**主动提供** `set_muted` 的界面生效；没有就不管，
## 因为不能要求每个界面都实现它 —— 原版也只有 PlayScreen 顶栏有这个开关）。
func _apply_mute_to_screen() -> void:
	var play: Control = screens.get("PlayScreen")
	if play == null:
		return
	if play.has_method("set_muted"):
		play.call("set_muted", is_muted())

## 总线 → store 的**另一半**同步。每帧做一次极便宜的布尔比较：
## 任何人（界面 / 未来的设置面板 / 外部脚本）直接调 `AudioServer.set_bus_mute`，
## 下一帧就会被 `store.muted` 吸收并落盘，`mute_changed` 也会冒出去。
func _process(_delta: float) -> void:
	if audio != null and audio.adopt_bus():
		_apply_mute_to_screen()

func _on_mute_changed(muted: bool) -> void:
	store.muted = muted
	store.save_all()

# ------------------------------------------------------------------ 对局结果

## 由 PlayScreen 在"一方全灭"时调用；战役模式下推进解锁。
func report_match_result(human_won: bool, team_won: int) -> void:
	last_result = {"human_won": human_won, "winner_team": team_won}
	# PENDING_MAPPING：原版的播放层订阅了 `game_state_info_event`（docs/128 §3.3），
	# 说明对局状态变化**会**触发音频；具体哪个音未知。下面两个是我们自己的选择。
	_sfx(SFX_WIN if human_won else SFX_LOSE)
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
