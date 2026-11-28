extends RefCounted
class_name GwNetSession
## 一局**连到别人服务器**的会话：握手 → 房间状态 → 开局 → 收广播。
##
## # 线上顺序（观测）
##
## `docs/33` §1 —— 客户端连上后发 `NewConnectionRequest`，服务端**按固定顺序**回 7 条：
##
## ```text
## NewConnection{connection_id} → TurnModeInfo → GameStateInfo → GameNameInfo
##   → AxisModeInfo → FunctionModeInfo → TimeModeInfo
## ```
##
## `docs/62` §1 —— 之后 `GameStartRequest` 触发开局（`AddObstacle` × N、每名士兵
## `SoldierInfo`/`TeamInfo`/`PosInfo`/`LifeInfo`、`TurnInfo`、`GameStateInfo`），
## `FunctionUpdateRequest` / `FunctionFireRequest` 提交与开火，服务端广播结算。
##
## # 状态机
##
## ```text
## idle → connecting ──TCP 连上──→ handshaking ──7 条到齐──→ joined
##            │                        │                       │
##            └──连不上──→ failed       └──超时──→ failed       ├─ GameStart 数据 → started
##                                                             ├─ GameStateInfo{EndingGame} → ended
##                                                             └─ 对端关闭       → closed
## ```
##
## **2026-11-27（T84）删掉了"静默也算到齐"那条兜底**（原来 `JOIN_IDLE = 0.30`：
## 见过 `NewConnection` 之后再静默 0.3 s 就 `joined`）。它不是原版行为，是我们自己的宽容，
## 而且会造出**半成品对局屏**：
##
## * `joined` 一旦提前触发，`App._on_net_joined()` 立刻进 PlayScreen 并 `_sync_state_from_net()`，
##   而那时 `room` 里还没有 `function_mode` / `time_mode` / 实体 ⇒ `function_mode_index("") = 1`、
##   `time_mode_index("") = 1` ⇒ 底栏画出 **`dy/dx` 图标 + `00:5x` 倒计时**，棋盘**空**，
##   直到 `TurnInfo` 到达（`game_started`）才修正；
## * 帧长短时这条兜底必然踩中：`poll()` 里 `_idle += delta` 在**每帧**都加，
##   llvmpipe 下 `delta` 常有 0.3 s+ ⇒ 第一个 poll 只要没把 47 帧全读完就立刻"静默达成"。
##
## 证据（**观测**，可复跑）：`docs/06` §131.2 —— T76 那三轮 `settle 1.5 s` 的采集里
## `round1/round2 = 12 823 px（0.5757 %）`、`round3 = 13 390 px（0.6012 %）`，全部落在
## 底栏左块（`00:5x` + `dy/dx`）与空棋盘上；同一时刻**原版**已是成品态。
## 缺的证据：**原版在"对端只发一半状态同步"时怎么处理**（未观测，也没有可指的反汇编地址）。
## 所以这里只做"去掉我方自造"：状态同步没到齐就**不许进屏**，到 `HANDSHAKE_TIMEOUT`（5.0 s）
## 显式报"握手超时"，而不是静默进一块拼错的屏。
##
## # 谁是"我"
##
## **观测缺口**：真实客户端用 `NewPlayerRequest` 换来 `PlayerInfo{player_id,connection_id}`，
## 但**我们的服务端没实现这一段**（`docs/33` §4）。所以 `my_player_id` 在连我们的服务端
## 时恒为 -1，界面侧只能退化成"第一个玩家 / 当前回合的玩家"。这一条写进 `docs/134` §6。

signal joined                              ## 握手完成，可以进对局界面了
signal game_started                        ## 收到开局那一批（关卡/士兵/轮到谁）
signal room_changed                        ## 房间状态有任何变化
signal message_received(kind: String, body: Dictionary)
signal ended(reason: String)               ## 对局正常结束（线上原文 = `GameStateInfo{game_state:"EndingGame"}`）
signal failed(reason: String)              ## 一次都没连上 / 握手超时
signal closed(reason: String)              ## 连上之后被对端断开

## 握手（从 TCP 连上开始算）的超时。**本项目选择**：原版没有可观测的超时值。
const HANDSHAKE_TIMEOUT := 5.0
## ⚠️ 2026-11-27（T84）：`JOIN_IDLE = 0.30` 那条"静默也算到齐"的兜底**已删除**（见文件头）。
## 收件箱（消息名）上限 —— 只为自检/文档取证，不参与逻辑。
const MAX_RECEIVED := 512

## `docs/33` §1 的 7 条状态同步（顺序即抓包顺序）。**名单与顺序从 Rust 取**（T99 / `docs/06` §144）。
##
## 改前这里是手抄的一份常量表 —— 那是**第三份拷贝**（`wire_samples.gd::STATE_SYNC` 是抓包原文、
## `fixtures/protocol_match.jsonl` 是原始报文）。真值在
## `graphwar_server_connection_handler::on_new_connection()`：那个函数的**输出**就是这 7 条，
## 并被 `state_sync_matches_captured_bytes` 与真实抓包**逐字节**钉住（`docs/archive/33`）。
##
## GDScript 的 `const` 不能调扩展类的方法 ⇒ 用 `_init()` 填一次的只读 `var`（同 `link.gd:54`
## 用 `GwNetConsts.connect_timeout_secs()` 的形态）。
var STATE_SYNC_KINDS: Array = []
## 抓到过的 `game_state` 取值（`docs/96`）。别的一律当"未知"字符串原样留着。
const GAME_STATES := ["Setup", "WaitingForFunctions", "ExecutingResults", "EndingGame"]

## "idle" | "connecting" | "handshaking" | "joined" | "started" | "ended" | "closed" | "failed"
var phase := "idle"
var host := ""
var port := 0
var last_error := ""
## 我们这条连接的 `connection_id`（`NewConnection` 给的）。
var my_connection_id := -1
## 我们的 `player_id`（需要服务端发 `PlayerInfo` 才能填；我们的服务端不发 ⇒ -1）。
var my_player_id := -1
## 加入后是否立刻发 `GameStartRequest`。
##
## **偏离，已记录**：真游戏里开局是**房主**按 Start 触发的（房主的客户端发这条）。
## 我们的服务端是台"等客户端来开局"的 harness（`src/graphwar_game_server.rs`），所以本客户端
## 连上自己的服务端后**自己发**这一条，否则对局永远不开始。关掉它就连真服务器。
var auto_start := true
## 加入时是否发 `NewPlayerRequest`（原版会；我们的服务端忽略它）。
var request_player := true

var link: GwNetLink = null
var room := {}

## 收到的消息名（按到达顺序，有界）。自检/文档用它证明"真的收到了哪些事件"。
var received: Array[String] = []

var _elapsed := 0.0
var _saw_new_connection := false
var _state_seen := {}
var _joined_once := false
var _started_once := false


func _init() -> void:
	STATE_SYNC_KINDS = GwFlowClock.state_sync_kinds()
	_reset_room()


func _reset_room() -> void:
	room = {
		"connection_id": -1,
		"room_name": "",
		"game_state": "",
		"axis_mode": "",
		"function_mode": "",
		"turn_mode": "",
		"time_mode": "",
		# player_id -> {"team": 0/1, "soldiers": Array[int], "connection_id": int}
		"players": {},
		# entity_id -> {"kind": "soldier"|"obstacle", "x": float, "y": float,
		#               "radius": float, "team": 0/1, "player_id": int, "alive": bool}
		"entities": {},
		"obstacles": [],          # [{x, y, r}]（`AddObstacle` 的顺序）
		"turn": {"player_id": -1, "soldier_id": -1},
		"turn_count": 0,
		"end_of_turn_tick": 0,
		"current_tick": 0,
		# owner_id(=soldier_id) -> {"function": String, "corrected": String}
		"functions": {},
		"ending": false,
		# ---- 以下边表是 T17 新增（`docs/06` §64）：`TeamInfo`/`ColorInfo`/`NameInfo` 可能
		# 比 `SoldierInfo`/`PosInfo` **先到**（原版也用同一批事件描述"玩家"与"士兵"两种实体），
		# 所以"实体还没建出来"时先把事实记在这里，等实体建好时 `_ensure_entity()` 再回填。
		# target_id 列表（`TargetInfo`；原版靶子是独立图层，`docs/131`）
		"targets": [],
		"teams": {},            # entity_id -> 0/1（实体还没建出来时的队伍）
		"names": {},            # entity_id -> String（`NameInfo`；玩家名/士兵名）
		"look": {},             # entity_id -> {"skin": String, "face": String, "hat": String}
		"colors": {},           # entity_id -> {"r": int, "g": int, "b": int}
		"entity_effects": {},   # entity_id -> String（`EntityEffectInfo.effect_type`）
		"effects": [],          # [{effect_id, x, y, type, start_tick}]（`EffectInfo`）
		"fired": {},            # soldier_id -> true（`FunctionFire`；只是记账，见 §64 遗留）
		"shots": {},            # owner_id -> {start_x, start_y, end_x}（`FunctionPoints`）
		"active_ticks": {},     # owner_id -> int（`FunctionActive`）
	}


# ---------------------------------------------------------------- 连接

func start(p_host: String, p_port: int) -> void:
	_reset_room()
	received.clear()
	_elapsed = 0.0
	_saw_new_connection = false
	_state_seen.clear()
	_joined_once = false
	_started_once = false
	my_connection_id = -1
	my_player_id = -1
	host = p_host
	port = p_port
	phase = "connecting"
	link = GwNetLink.new()
	link.connected.connect(_on_connected)
	link.frame_received.connect(_on_frame)
	link.link_failed.connect(_on_link_failed)
	link.link_closed.connect(_on_link_closed)
	link.start(host, port)


func poll(delta: float) -> void:
	if link != null:
		link.poll(delta)
	if phase == "connecting" or phase == "handshaking":
		_elapsed += delta
		if phase == "handshaking" and _elapsed >= HANDSHAKE_TIMEOUT:
			_fail("握手超时（%.1f s 内没收到完整状态同步）" % HANDSHAKE_TIMEOUT)


func abort() -> void:
	if link != null:
		link.shutdown()
		link = null
	phase = "closed"


func is_joined() -> bool:
	return phase == "joined" or phase == "started"


func is_live() -> bool:
	return phase == "joined" or phase == "started" or phase == "handshaking" or phase == "connecting"


func _on_connected() -> void:
	phase = "handshaking"
	_elapsed = 0.0
	send_new_connection_request()


func _on_link_failed(reason: String) -> void:
	_fail(reason)


func _on_link_closed(reason: String) -> void:
	if phase == "idle" or phase == "closed" or phase == "failed" or phase == "ended":
		return
	last_error = reason
	phase = "closed"
	closed.emit(reason)


func _fail(reason: String) -> void:
	if phase == "failed" or phase == "closed":
		return
	last_error = reason
	phase = "failed"
	if link != null:
		link.shutdown()
		link = null
	failed.emit(reason)


# ---------------------------------------------------------------- 收帧

func _on_frame(text: String) -> void:
	var msg := GwNetFrame.parse_message(text)
	if msg.is_empty():
		return
	var kind := String(msg["kind"])
	var body: Dictionary = msg["body"]
	_note(kind)
	_apply(kind, body)
	message_received.emit(kind, body)
	if phase == "handshaking" and _can_join_by_state_sync():
		_mark_joined()


func _note(kind: String) -> void:
	if received.size() < MAX_RECEIVED:
		received.append(kind)
	if STATE_SYNC_KINDS.has(kind):
		_state_seen[kind] = true


## 把一条事件落到 `room` 上，并**同时喂进 Rust 侧的复原 manager**（T17，`docs/06` §64）。
##
## 两层分工，别混：
##
## * **本层** = 线上事件 → 我方模型（字段解析、`room` 里的那几张表）。只解析抓包里见过的字段，
##   未知事件只记录不猜。
## * **manager 层** = `GwNetEvents`（`src/_scaffold/net_events_class.rs`）：按
##   「线上事件 → 复原 manager 的哪个方法」那张**证据表**派发（原版那条
##   `GraphwarRemoteEventHandler::handle_server_remote_event` x86 `0x3e6df0` 我方没复元，
##   这里是它的接线桥）。**这里没有第二套游戏语义**：凡是能"喂给 manager"的都不在 GDScript 里重算。
##
## 一个事件的两步顺序：先 `_feed_managers()`（把原始字段交给 Rust），再 `match` 落 `room`。
## 反过来也行 —— 但先喂 Rust 能把"事件先到、实体后建"这类顺序问题留在 Rust 侧，
## GDScript 只负责它自己那张表。
func _apply(kind: String, body: Dictionary) -> void:
	var changed := true
	_feed_managers(kind, body)
	match kind:
		"NewConnection":
			my_connection_id = GwNetFrame.body_int(body, "connection_id", -1)
			room["connection_id"] = my_connection_id
			_saw_new_connection = true
		"GameNameInfo":
			room["room_name"] = GwNetFrame.body_str(body, "game_name")
		"GameStateInfo":
			room["game_state"] = GwNetFrame.body_str(body, "game_state")
			# 收场那一条的**线上原文就是它**，不是事件名 `EndingGame`
			# （`fixtures/frame_endgame_events.txt` 的 `[phase3]` 末行 =
			#  `{"GameStateInfo":{"game_state":"EndingGame"}}`；`docs/06` §77 复采一致）。
			# ⚠️ 2026-11-26 更正：这里以前是**只**写 `room["game_state"]`，而下面
			# `"EndingGame"` 那一支按**事件名**匹配 ⇒ 全网没有任何一条报文叫这个名字，
			# 那一支**从来没执行过** ⇒ `ended` 信号从未发出、`phase` 从未变成 `"ended"`。
			if String(room["game_state"]) == "EndingGame":
				_end_session()
				return
		"AxisModeInfo":
			room["axis_mode"] = GwNetFrame.body_str(body, "axis_mode")
		"FunctionModeInfo":
			room["function_mode"] = GwNetFrame.body_str(body, "function_mode")
		"TurnModeInfo":
			room["turn_mode"] = GwNetFrame.body_str(body, "turn_mode")
		"TimeModeInfo":
			room["time_mode"] = GwNetFrame.body_str(body, "time_mode")
		"PlayerInfo":
			var pid := GwNetFrame.body_int(body, "player_id", -1)
			var cid := GwNetFrame.body_int(body, "connection_id", -1)
			if pid >= 0:
				var p := _ensure_player(pid)
				p["connection_id"] = cid
				if (room["teams"] as Dictionary).has(pid):
					p["team"] = int((room["teams"] as Dictionary)[pid])
				if cid >= 0 and cid == my_connection_id:
					my_player_id = pid
		"TeamInfo":
			# ⚠️ **不建实体**：`TeamInfo{entity_id}` 的 `entity_id` 既可能是士兵、也可能是
			# "玩家自己"这个实体（本剧本的 229）。以前无条件 `_ensure_entity()` 会把它变成
			# 一具停在 (0,0) 的士兵（`docs/06` §60.5）。
			var teid := GwNetFrame.body_int(body, "entity_id", -1)
			if teid >= 0:
				var team := team_index(GwNetFrame.body_str(body, "team"))
				var entities: Dictionary = room["entities"]
				if entities.has(teid):
					(entities[teid] as Dictionary)["team"] = team
				elif (room["players"] as Dictionary).has(teid):
					(room["players"] as Dictionary)[teid]["team"] = team
				elif (room["targets"] as Array).has(teid):
					# 靶子先到（`TargetInfo` → `TeamInfo` 的顺序在真回放里就是这样）：
					_ensure_entity(teid, "target")["team"] = team
				else:
					(room["teams"] as Dictionary)[teid] = team
		"RemoveTeam":
			# `RemoveTeam{entity_id}`：原版 `remove_from_team` **删到了才**广播它
			# （`graphwar_team_manager` §1，观测）⇒ 收到就删本地那一格。
			var reid := GwNetFrame.body_int(body, "entity_id", -1)
			if reid >= 0:
				(room["teams"] as Dictionary).erase(reid)
				var rentities: Dictionary = room["entities"]
				if rentities.has(reid):
					(rentities[reid] as Dictionary)["team"] = 0
				if (room["players"] as Dictionary).has(reid):
					(room["players"] as Dictionary)[reid]["team"] = 0
		"ColorInfo":
			# 服务端颜色**直接采纳**（原版 `color_manager::set_color`，观测 `0x3e7e9c`）；
			# 以前这里 `pass`，颜色全靠界面自定 ⇒ 准备圈的颜色跟原版对不上（`docs/06` §64）。
			var ceid := GwNetFrame.body_int(body, "entity_id", -1)
			var col: Dictionary = body.get("color", {})
			if ceid >= 0 and col is Dictionary:
				(room["colors"] as Dictionary)[ceid] = {
					"r": int(col.get("r", 0)), "g": int(col.get("g", 0)), "b": int(col.get("b", 0)),
				}
		"SoldierInfo":
			var sid := GwNetFrame.body_int(body, "soldier_id", -1)
			var owner := GwNetFrame.body_int(body, "player_id", -1)
			if sid >= 0:
				var se := _ensure_entity(sid, "soldier")
				se["kind"] = "soldier"
				se["player_id"] = owner
				if owner >= 0:
					var sp := _ensure_player(owner)
					if not (sp["soldiers"] as Array).has(sid):
						(sp["soldiers"] as Array).append(sid)
		"TargetInfo":
			# `TargetInfo{target_id}`：原版靶子是**独立图层**（`docs/131`），不跟士兵混。
			# ⚠️ 缺口：`play_screen.gd` 的 `_targets`（`GwTargetSet`）在联机分支里从不填
			# （`_begin_online()` 里是 `_targets = GwTargetSet.new()`）⇒ 本客户端**还画不出**
			# 靶子那一层；这里先把事实落进 `room.targets` / 实体种类，等界面侧接线。
			var tid := GwNetFrame.body_int(body, "target_id", -1)
			if tid >= 0:
				if not (room["targets"] as Array).has(tid):
					(room["targets"] as Array).append(tid)
				_ensure_entity(tid, "target")["kind"] = "target"
		"EntityRemoved":
			# `EntityRemoved{entity_id}`：原版每个 manager 都订了这条（逐 manager 证据见
			# `net_events_class.rs` 文件头）⇒ 这里把 `room` 里那份也删干净。
			_remove_entity(GwNetFrame.body_int(body, "entity_id", -1))
		"PosInfo":
			var peid := GwNetFrame.body_int(body, "entity_id", -1)
			if peid >= 0:
				var pe := _ensure_entity(peid, "soldier" if _is_soldier(peid) else "")
				pe["x"] = GwNetFrame.body_float(body, "pos_x")
				pe["y"] = GwNetFrame.body_float(body, "pos_y")
				pe["radius"] = GwNetFrame.body_float(body, "radius", 0.5)
		"LifeInfo":
			var leid := GwNetFrame.body_int(body, "entity_id", -1)
			if leid >= 0:
				_ensure_entity(leid)["alive"] = GwNetFrame.body_bool(body, "alive", true)
		"LifeQueuedDeath":
			# 线上致死那一批是 `LifeQueuedDeath{entity_id, tick}` + `EffectInfo(DeathEffect)`
			# （`docs/31` §1、`docs/43`）—— 它**不**带 `alive` 字段，语义是"到期才收尸"，
			# 本客户端没有 tick 队列（`docs/30` §5）⇒ 只记账，不在这里判死。
			pass
		"NameInfo":
			var neid := GwNetFrame.body_int(body, "entity_id", -1)
			var nm := GwNetFrame.body_str(body, "name")
			if neid >= 0:
				(room["names"] as Dictionary)[neid] = nm
				var nentities: Dictionary = room["entities"]
				if nentities.has(neid):
					(nentities[neid] as Dictionary)["name"] = nm
		"SkinGraphicsInfo":
			_set_look(GwNetFrame.body_int(body, "entity_id", -1), "skin",
				GwNetFrame.body_str(body, "graphics_str"))
		"FaceGraphicsInfo":
			_set_look(GwNetFrame.body_int(body, "entity_id", -1), "face",
				GwNetFrame.body_str(body, "graphics_str"))
		"HatGraphicsInfo":
			_set_look(GwNetFrame.body_int(body, "entity_id", -1), "hat",
				GwNetFrame.body_str(body, "graphics_str"))
		"EntityEffectInfo":
			# 原版：`entity_id → effect kind` 那张表（`graphwar_entity_effect_manager`），
			# 士兵出生登记 `DeathEffect`/`AshEffect`、靶子 `SparkEffect`。
			var xeeid := GwNetFrame.body_int(body, "entity_id", -1)
			if xeeid >= 0:
				(room["entity_effects"] as Dictionary)[xeeid] = GwNetFrame.body_str(body, "effect_type")
		"FunctionFire":
			# `FunctionFire{soldier_id}`：开火公告。**客户端收到它之后原版做什么没有证据**
			# （不是 `GameStateManager` 订阅的 7 条之一；弹道由 FunctionPoints/FunctionActive/
			# EffectInfo 三条驱动）⇒ 只记账，不在这里推任何状态（`docs/06` §64 遗留 §1）。
			var fsid := GwNetFrame.body_int(body, "soldier_id", -1)
			if fsid >= 0:
				(room["fired"] as Dictionary)[fsid] = true
		"FunctionPoints":
			var foid := GwNetFrame.body_int(body, "owner_id", -1)
			if foid >= 0:
				(room["shots"] as Dictionary)[foid] = {
					"start_x": GwNetFrame.body_float(body, "start_x"),
					"start_y": GwNetFrame.body_float(body, "start_y"),
					"end_x": GwNetFrame.body_float(body, "end_x"),
				}
		"FunctionActive":
			var aoid := GwNetFrame.body_int(body, "owner_id", -1)
			if aoid >= 0:
				(room["active_ticks"] as Dictionary)[aoid] = \
					GwNetFrame.body_int(body, "active_tick", 0)
		"EffectInfo":
			(room["effects"] as Array).append({
				"effect_id": GwNetFrame.body_int(body, "effect_id", -1),
				"x": GwNetFrame.body_float(body, "pos_x"),
				"y": GwNetFrame.body_float(body, "pos_y"),
				"type": GwNetFrame.body_str(body, "effect_type"),
				"start_tick": GwNetFrame.body_int(body, "start_tick", 0),
			})
		"AddObstacle":
			var obs := {
				"x": GwNetFrame.body_float(body, "pos_x"),
				"y": GwNetFrame.body_float(body, "pos_y"),
				"r": GwNetFrame.body_float(body, "radius"),
			}
			(room["obstacles"] as Array).append(obs)
		"ClearObstacles":
			room["obstacles"] = []
		"QueueDestroyObstacle", "DestroyObstacle":
			_remove_obstacle(GwNetFrame.body_float(body, "pos_x"),
				GwNetFrame.body_float(body, "pos_y"))
		"TurnInfo":
			room["turn"] = {
				"player_id": GwNetFrame.body_int(body, "player_id", -1),
				"soldier_id": GwNetFrame.body_int(body, "soldier_id", -1),
			}
			if not _started_once:
				_mark_started()
		"TurnCountInfo":
			room["turn_count"] = GwNetFrame.body_int(body, "turn_count", 0)
		"EndOfTurnInfo":
			room["end_of_turn_tick"] = GwNetFrame.body_int(body, "end_of_turn_tick", 0)
			if not _started_once:
				_mark_started()
		"TickReply":
			room["current_tick"] = GwNetFrame.body_int(body, "current_tick", 0)
		"FunctionInfo":
			var fid := GwNetFrame.body_int(body, "owner_id", -1)
			if fid >= 0:
				(room["functions"] as Dictionary)[fid] = {
					"function": GwNetFrame.body_str(body, "function"),
					"corrected": GwNetFrame.body_str(body, "corrected_function"),
				}
		_:
			changed = false      # 其余（本局用不到的事件）只记录名
	if changed:
		room_changed.emit()

## 收场：**一局正常结束**（原版 `GraphwarGameStateManager::set_state(EndingGame)`）。
##
## 三条事实，各自有出处，别混：
##
## * **线上原文**：`GameStateInfo{game_state:"EndingGame"}`（`fixtures/frame_endgame_events.txt`
##   的 `[phase3]` 末行；`docs/07` §3.3/WIRE_VALUE_ENUMS 的 `GameState` 取值表）——
##   **不是**一条叫 `EndingGame` 的报文。所以触发点在 `"GameStateInfo"` 那一支（见上）。
## * **不是断线**：`src/graphwar_game_server.rs` 在`EndingGame` 之后才关连接，所以
##   `phase = "ended"` 让 [`_on_link_closed`] 直接 return ⇒ 不弹"你已断开"（`docs/06` §77 两侧各验一次）。
## * **只发生一次**：重入直接 return（否则 `ended` 会重复触发 ⇒ 结算框重开、音效重复）。
func _end_session() -> void:
	if phase == "ended":
		return
	room["ending"] = true
	phase = "ended"
	room_changed.emit()
	ended.emit("EndingGame")


# ---------------------------------------------------------------- 喂给 Rust 侧（T17）
#
# 这一段的**唯一职责**是把线上字段按 `src/_scaffold/protocol.rs::WIRE_SPEC` 的**字段顺序**
# 传给 `GwNetEvents`（`src/_scaffold/net_events_class.rs`）。那个类里是"线上事件 → 哪个
# manager 的哪个方法"的**证据表**；本文件这里不解释任何游戏语义。
#
# 为什么必须在 GDScript 里做这一层：内核那条 `GraphwarRemoteEventHandler` 我方没复元
# （`docs/02-module-map.md` §17），而事件是 JSON —— 解析在 `GwNetFrame` 已经有了，
# 所以"解析在 GDScript、语义在 Rust"是当前唯一不重复造语义的接法。

## 懒建的 `GwNetEvents` 实例（跨局复用；`_reset_room()` 不销毁它，通道是幂等的）。
var net_events: RefCounted = null


## 取（必要时建）派发桥。内核没加载 ⇒ 返回 `null`，整条链路静默降级（界面照旧跑）。
func _net_events() -> RefCounted:
	if net_events == null:
		if not ClassDB.class_exists("GwNetEvents"):
			return null
		net_events = ClassDB.instantiate("GwNetEvents") as RefCounted
		if net_events != null and net_events.has_method("reset"):
			net_events.call("reset")
	return net_events


## 内核派发桥装上了没有（界面/自检可用来区分"链路断了"与"事件没来"）。
func net_events_ready() -> bool:
	var ev := _net_events()
	return ev != null and bool(ev.call("installed"))


## 已经喂进 Rust manager 的事件条数。
func net_events_applied() -> int:
	var ev := _net_events()
	return int(ev.call("applied")) if ev != null else 0


## manager 表的快照（自检/取证用；键见 `net_events_class.rs::dump`）。
func net_events_dump() -> Dictionary:
	var ev := _net_events()
	return ev.call("dump") if ev != null else {}


## 一条事件 → `GwNetEvents` 的对应方法（**一事件一行**，参数顺序 = `WIRE_SPEC`）。
func _feed_managers(kind: String, body: Dictionary) -> void:
	var ev := _net_events()
	if ev == null:
		return
	match kind:
		"NewConnection":
			ev.call("new_connection", GwNetFrame.body_int(body, "connection_id", -1))
		"PlayerInfo":
			ev.call("player_info", GwNetFrame.body_int(body, "player_id", -1),
				GwNetFrame.body_int(body, "connection_id", -1))
		"SoldierInfo":
			ev.call("soldier_info", GwNetFrame.body_int(body, "soldier_id", -1),
				GwNetFrame.body_int(body, "player_id", -1))
		"TargetInfo":
			ev.call("target_info", GwNetFrame.body_int(body, "target_id", -1))
		"TeamInfo":
			ev.call("team_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "team"))
		"RemoveTeam":
			ev.call("remove_team", GwNetFrame.body_int(body, "entity_id", -1))
		"EntityRemoved":
			ev.call("entity_removed", GwNetFrame.body_int(body, "entity_id", -1))
		"PosInfo":
			ev.call("pos_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_float(body, "pos_x"), GwNetFrame.body_float(body, "pos_y"),
				GwNetFrame.body_float(body, "radius", 0.5))
		"LifeInfo":
			ev.call("life_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_bool(body, "alive", true))
		"LifeQueuedDeath":
			ev.call("life_queued_death", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_int(body, "tick", 0))
		"AddObstacle":
			ev.call("add_obstacle", GwNetFrame.body_float(body, "pos_x"),
				GwNetFrame.body_float(body, "pos_y"), GwNetFrame.body_float(body, "radius"))
		"ClearObstacles":
			ev.call("clear_obstacles")
		"QueueDestroyObstacle":
			ev.call("queue_destroy_obstacle", GwNetFrame.body_float(body, "pos_x"),
				GwNetFrame.body_float(body, "pos_y"), GwNetFrame.body_float(body, "radius"),
				GwNetFrame.body_int(body, "tick", 0))
		"DestroyObstacle":
			ev.call("destroy_obstacle", GwNetFrame.body_float(body, "pos_x"),
				GwNetFrame.body_float(body, "pos_y"), GwNetFrame.body_float(body, "radius"))
		"TurnInfo":
			ev.call("turn_info", GwNetFrame.body_int(body, "player_id", -1),
				GwNetFrame.body_int(body, "soldier_id", -1))
		"TurnModeInfo":
			ev.call("turn_mode_info", GwNetFrame.body_str(body, "turn_mode"))
		"AxisModeInfo":
			ev.call("axis_mode_info", GwNetFrame.body_str(body, "axis_mode"))
		"FunctionModeInfo":
			ev.call("function_mode_info", GwNetFrame.body_str(body, "function_mode"))
		"TimeModeInfo":
			ev.call("time_mode_info", GwNetFrame.body_str(body, "time_mode"))
		"SkinGraphicsInfo":
			ev.call("skin_graphics_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "graphics_str"))
		"FaceGraphicsInfo":
			ev.call("face_graphics_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "graphics_str"))
		"HatGraphicsInfo":
			ev.call("hat_graphics_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "graphics_str"))
		"EntityEffectInfo":
			ev.call("entity_effect_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "effect_type"))
		"NameInfo":
			ev.call("name_info", GwNetFrame.body_int(body, "entity_id", -1),
				GwNetFrame.body_str(body, "name"))
		"ColorInfo":
			var col: Dictionary = body.get("color", {})
			ev.call("color_info", GwNetFrame.body_int(body, "entity_id", -1),
				int(col.get("r", 0)) if col is Dictionary else 0,
				int(col.get("g", 0)) if col is Dictionary else 0,
				int(col.get("b", 0)) if col is Dictionary else 0)
		"FunctionFire":
			ev.call("function_fire", GwNetFrame.body_int(body, "soldier_id", -1))
		"FunctionInfo":
			ev.call("function_info", GwNetFrame.body_int(body, "owner_id", -1),
				GwNetFrame.body_str(body, "function"),
				GwNetFrame.body_str(body, "corrected_function"))
		"FunctionActive":
			ev.call("function_active", GwNetFrame.body_int(body, "owner_id", -1),
				GwNetFrame.body_int(body, "active_tick", 0))
		"FunctionPoints":
			ev.call("function_points", GwNetFrame.body_int(body, "owner_id", -1),
				GwNetFrame.body_float(body, "start_x"), GwNetFrame.body_float(body, "start_y"),
				GwNetFrame.body_float(body, "end_x"))
		"EffectInfo":
			ev.call("effect_info", GwNetFrame.body_int(body, "effect_id", -1),
				GwNetFrame.body_float(body, "pos_x"), GwNetFrame.body_float(body, "pos_y"),
				GwNetFrame.body_str(body, "effect_type"),
				GwNetFrame.body_int(body, "start_tick", 0))
		_:
			pass


## 实体被删：从 `entities`、玩家的 `soldiers` 列表、以及那几张边表里一起清掉。
func _remove_entity(eid: int) -> void:
	if eid < 0:
		return
	(room["entities"] as Dictionary).erase(eid)
	(room["teams"] as Dictionary).erase(eid)
	(room["names"] as Dictionary).erase(eid)
	(room["look"] as Dictionary).erase(eid)
	(room["colors"] as Dictionary).erase(eid)
	(room["entity_effects"] as Dictionary).erase(eid)
	var targets: Array = room["targets"]
	var ti := targets.find(eid)
	if ti >= 0:
		targets.remove_at(ti)
	var players: Dictionary = room["players"]
	for pid in players:
		var lst: Array = (players[pid] as Dictionary)["soldiers"]
		var idx := lst.find(eid)
		if idx >= 0:
			lst.remove_at(idx)


## 记账：某实体的外观三件套之一（原版由 graphics 事件写进 `EntityDrawer` 的三张表）。
func _set_look(eid: int, layer: String, value: String) -> void:
	if eid < 0:
		return
	var looks: Dictionary = room["look"]
	if not looks.has(eid):
		looks[eid] = {"skin": "", "face": "", "hat": ""}
	(looks[eid] as Dictionary)[layer] = value


## 这个实体是不是士兵（看已有事实：`SoldierInfo` 建过、或玩家表里挂着它）。
func _is_soldier(eid: int) -> bool:
	var entities: Dictionary = room["entities"]
	if entities.has(eid) and String((entities[eid] as Dictionary).get("kind", "")) == "soldier":
		return true
	var players: Dictionary = room["players"]
	for pid in players:
		if (players[pid] as Dictionary).get("soldiers", []).has(eid):
			return true
	return false


func _ensure_player(pid: int) -> Dictionary:
	var players: Dictionary = room["players"]
	if not players.has(pid):
		players[pid] = {"team": 0, "soldiers": [], "connection_id": -1}
	return players[pid]


## 建/取一个实体。
##
## `kind` 缺席时**不给默认种类** —— T17 改：以前默认 `"soldier"` 会把"只被 `TeamInfo`
## 提到过的玩家实体"（本剧本的 229）与"已删除的士兵"（231）都变成一具停在 (0,0) 的士兵，
## 于是准备圈画在棋盘正中、棋盘上多两个假精灵（`docs/06` §60.5 的三处差异就来自这里）。
## 现在只有 `SoldierInfo` 说它是 soldier、`TargetInfo` 说它是 target，其余一律 `""`（未知）。
func _ensure_entity(eid: int, p_kind := "") -> Dictionary:
	var entities: Dictionary = room["entities"]
	if not entities.has(eid):
		var e := {
			"kind": "", "x": 0.0, "y": 0.0, "radius": 0.5,
			"team": 0, "player_id": -1, "alive": true, "name": "",
		}
		# 边表里先到的那些事实（队伍/名字/外观/颜色/实体特效）在这里回填一次。
		var teams: Dictionary = room["teams"]
		if teams.has(eid):
			e["team"] = int(teams[eid])
		var names: Dictionary = room["names"]
		if names.has(eid):
			e["name"] = String(names[eid])
		entities[eid] = e
	if p_kind != "" and String(entities[eid]["kind"]) != p_kind:
		entities[eid]["kind"] = p_kind
	return entities[eid]


func _remove_obstacle(x: float, y: float) -> void:
	var list: Array = room["obstacles"]
	for i in range(list.size() - 1, -1, -1):
		var o: Dictionary = list[i]
		if absf(float(o["x"]) - x) < 0.01 and absf(float(o["y"]) - y) < 0.01:
			list.remove_at(i)
			return


# ---------------------------------------------------------------- 握手完成判定

func _can_join_by_state_sync() -> bool:
	if not _saw_new_connection:
		return false
	for k in STATE_SYNC_KINDS:
		if not _state_seen.has(k):
			return false
	return true


func _mark_joined() -> void:
	if _joined_once:
		return
	_joined_once = true
	phase = "joined"
	# `NewPlayerRequest{connection_id}`：原版加入时发（`docs/24` §3）。
	# 我们的服务端**不实现**它（`docs/33` §4），发了也只会被忽略 —— 所以默认发，
	# 保持线格式与原版一致；不想发就关 `request_player`。
	if request_player:
		send_new_player_request()
	joined.emit()
	if auto_start:
		send_start_game()


func _mark_started() -> void:
	if _started_once:
		return
	_started_once = true
	phase = "started"
	game_started.emit()


## 7 条状态同步里已经收到几条（自检/文档取证）。
func state_sync_progress() -> int:
	var n := 0
	for k in STATE_SYNC_KINDS:
		if _state_seen.has(k):
			n += 1
	return n


## 收到过的消息名（去重、保序）。
func received_kinds() -> Array[String]:
	var out: Array[String] = []
	for k in received:
		if not out.has(k):
			out.append(k)
	return out


func count_of(kind: String) -> int:
	var n := 0
	for k in received:
		if k == kind:
			n += 1
	return n


## 一局结束时的一份快照（自检断言用；也是将来 PlayScreen 该读的东西）。
func snapshot() -> Dictionary:
	return {
		"phase": phase,
		"host": host,
		"port": port,
		"connection_id": my_connection_id,
		"my_player_id": my_player_id,
		"room_name": room["room_name"],
		"game_state": room["game_state"],
		"turn_mode": room["turn_mode"],
		"time_mode": room["time_mode"],
		"axis_mode": room["axis_mode"],
		"function_mode": room["function_mode"],
		"players": (room["players"] as Dictionary).size(),
		"soldiers": _count_entities("soldier"),
		"obstacles": (room["obstacles"] as Array).size(),
		"turn": (room["turn"] as Dictionary).duplicate(),
		"turn_count": room["turn_count"],
		"received": received.size(),
	}


func _count_entities(kind: String) -> int:
	var n := 0
	var entities: Dictionary = room["entities"]
	for eid in entities:
		var e: Dictionary = entities[eid]
		if String(e.get("kind", "")) == kind:
			n += 1
	return n


# ---------------------------------------------------------------- 发送

func _send(kind: String, body_json: String) -> bool:
	if link == null or not link.is_open():
		return false
	return link.send_text(GwNetFrame.message(kind, body_json))


func send_new_connection_request() -> bool:
	# 逐字节 = 抓包原文（`fixtures/protocol.jsonl:2`）。两个版本串**从 Rust 取**（T99）：
	# `graphwar_client_connection_handler::{NEW_CONNECTION_REQUEST_MAJOR, _MINOR}` ——
	# arm64 `0x4693c0`（`GraphwarEvent::make_new_connection_request_event`）的立即数
	# `mov w9,#0x2e32` = `"2."`、`mov w8,#0x30` = `'0'`、`mov w9,#0x3036` = `"60"`。
	# 改前这里是 `.gd` 手写的字面量（第二份拷贝）。
	return _send("NewConnectionRequest", String(GwFlowClock.new_connection_request_body()))


func send_new_player_request() -> bool:
	if my_connection_id < 0:
		return false
	return _send("NewPlayerRequest", '{"connection_id":%d}' % my_connection_id)


func send_start_game() -> bool:
	return _send("GameStartRequest", "{}")


func send_tick_request() -> bool:
	return _send("TickRequest", "{}")


func submit_function(player_id: int, function_text: String) -> bool:
	return _send("FunctionUpdateRequest",
		'{"player_id":%d,"function":%s}' % [player_id, GwNetFrame.json_string(function_text)])


func fire(player_id: int) -> bool:
	return _send("FunctionFireRequest", '{"player_id":%d}' % player_id)


# ---------------------------------------------------------------- 字符串 ↔ 档位
#
# 线上是**枚举名字符串**（`docs/96` §4：serde 外部标签，判别值不上线）。
# 已观测到的取值只有：`EveryUnit` / `NormalFunction` / `SequentialTurns` /
# `SimultaneousTurns` / `Timer1m` / `Timer3m` / `Team1` / `Team2`。其余映射是**推断**
# （按 `docs/92` §6 的档位循环顺序），写在 `docs/134` §5 的表里。

static func team_index(name: String) -> int:
	return 1 if name == "Team2" else 0


static func team_name(index: int) -> String:
	return "Team2" if index == 1 else "Team1"


static func turn_mode_index(name: String) -> int:
	return 1 if name.begins_with("Simultaneous") else 0


static func function_mode_index(name: String) -> int:
	return 0 if name.begins_with("Normal") else 1


## `GwModes` 的顺序是 30s / 1m / 2m / 3m / 5m / ∞（`docs/92` §6 的循环）。
static func time_mode_index(name: String) -> int:
	match name:
		"Timer30s": return 0
		"Timer1m": return 1
		"Timer2m": return 2
		"Timer3m": return 3
		"Timer5m": return 4
		_: return 5 if name.begins_with("TimerInf") or name == "Infinite" else 1


## `GwModes` 的顺序是 0=无轴 / 1=主轴 / 2=每 5 单位 / 3=每单位。
## 只有 `EveryUnit` 是观测值，其余按 `docs/118` §7.1 的描述文字对齐。
static func axis_mode_index(name: String) -> int:
	match name:
		"EveryUnit": return 3
		"Every5Units", "Every5": return 2
		"MainAxes", "MainAxis": return 1
		"NoAxis": return 0
		_: return 3
