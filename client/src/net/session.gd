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
## idle → connecting ──TCP 连上──→ handshaking ──7 条到齐(或静默)──→ joined
##            │                        │                              │
##            └──连不上──→ failed       └──超时──→ failed              ├─ GameStart 数据 → started
##                                                                    ├─ EndingGame     → ended
##                                                                    └─ 对端关闭       → closed
## ```
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
signal ended(reason: String)               ## 对局正常结束（`EndingGame`）
signal failed(reason: String)              ## 一次都没连上 / 握手超时
signal closed(reason: String)              ## 连上之后被对端断开

## 握手（从 TCP 连上开始算）的超时。**本项目选择**：原版没有可观测的超时值。
const HANDSHAKE_TIMEOUT := 5.0
## 7 条状态同步没到齐时的兜底：静默这么久也算握手完成（服务端可能只回一部分）。
const JOIN_IDLE := 0.30
## 收件箱（消息名）上限 —— 只为自检/文档取证，不参与逻辑。
const MAX_RECEIVED := 512

## `docs/33` §1 的 7 条状态同步（顺序即抓包顺序）。
const STATE_SYNC_KINDS := [
	"NewConnection", "TurnModeInfo", "GameStateInfo", "GameNameInfo",
	"AxisModeInfo", "FunctionModeInfo", "TimeModeInfo",
]
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
## 我们的服务端是台"等客户端来开局"的 harness（`src/serverloop.rs`），所以本客户端
## 连上自己的服务端后**自己发**这一条，否则对局永远不开始。关掉它就连真服务器。
var auto_start := true
## 加入时是否发 `NewPlayerRequest`（原版会；我们的服务端忽略它）。
var request_player := true

var link: GwNetLink = null
var room := {}

## 收到的消息名（按到达顺序，有界）。自检/文档用它证明"真的收到了哪些事件"。
var received: Array[String] = []

var _elapsed := 0.0
var _idle := 0.0
var _saw_new_connection := false
var _state_seen := {}
var _joined_once := false
var _started_once := false


func _init() -> void:
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
	}


# ---------------------------------------------------------------- 连接

func start(p_host: String, p_port: int) -> void:
	_reset_room()
	received.clear()
	_elapsed = 0.0
	_idle = 0.0
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
		_idle += delta
		if phase == "handshaking":
			if _can_join_by_idle():
				_mark_joined()
			elif _elapsed >= HANDSHAKE_TIMEOUT:
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
	_idle = 0.0
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
	_idle = 0.0
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


## 把一条事件落到 `room` 上。**只解析抓包里见过的字段**；未知事件只记录不猜。
func _apply(kind: String, body: Dictionary) -> void:
	var changed := true
	match kind:
		"NewConnection":
			my_connection_id = GwNetFrame.body_int(body, "connection_id", -1)
			room["connection_id"] = my_connection_id
			_saw_new_connection = true
		"GameNameInfo":
			room["room_name"] = GwNetFrame.body_str(body, "game_name")
		"GameStateInfo":
			room["game_state"] = GwNetFrame.body_str(body, "game_state")
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
				if cid >= 0 and cid == my_connection_id:
					my_player_id = pid
		"TeamInfo":
			var eid := GwNetFrame.body_int(body, "entity_id", -1)
			if eid >= 0:
				var e := _ensure_entity(eid)
				e["team"] = team_index(GwNetFrame.body_str(body, "team"))
		"ColorInfo":
			pass          # `color{r,g,b}`：颜色由界面侧按队色表分配，这里不覆盖
		"SoldierInfo":
			var sid := GwNetFrame.body_int(body, "soldier_id", -1)
			var owner := GwNetFrame.body_int(body, "player_id", -1)
			if sid >= 0:
				var se := _ensure_entity(sid)
				se["kind"] = "soldier"
				se["player_id"] = owner
				if owner >= 0:
					var sp := _ensure_player(owner)
					if not (sp["soldiers"] as Array).has(sid):
						(sp["soldiers"] as Array).append(sid)
		"PosInfo":
			var peid := GwNetFrame.body_int(body, "entity_id", -1)
			if peid >= 0:
				var pe := _ensure_entity(peid)
				pe["x"] = GwNetFrame.body_float(body, "pos_x")
				pe["y"] = GwNetFrame.body_float(body, "pos_y")
				pe["radius"] = GwNetFrame.body_float(body, "radius", 0.5)
		"LifeInfo":
			var leid := GwNetFrame.body_int(body, "entity_id", -1)
			if leid >= 0:
				_ensure_entity(leid)["alive"] = GwNetFrame.body_bool(body, "alive", true)
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
		"EndingGame":
			room["ending"] = true
			phase = "ended"
			changed = true
			room_changed.emit()
			ended.emit("EndingGame")
			return
		_:
			changed = false      # 结算/特效类（FunctionPoints/EffectInfo/…）只记录名
	if changed:
		room_changed.emit()


func _ensure_player(pid: int) -> Dictionary:
	var players: Dictionary = room["players"]
	if not players.has(pid):
		players[pid] = {"team": 0, "soldiers": [], "connection_id": -1}
	return players[pid]


func _ensure_entity(eid: int) -> Dictionary:
	var entities: Dictionary = room["entities"]
	if not entities.has(eid):
		entities[eid] = {
			"kind": "soldier", "x": 0.0, "y": 0.0, "radius": 0.5,
			"team": 0, "player_id": -1, "alive": true,
		}
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


func _can_join_by_idle() -> bool:
	return _saw_new_connection and _idle >= JOIN_IDLE


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
	# 逐字节 = 抓包原文（`fixtures/protocol.jsonl:2`）。版本号由客户端报给服务端，
	# 原版报 `2.0`/`60`；我们照抄（服务端并不校验，`docs/33` §1）。
	return _send("NewConnectionRequest", '{"major_version":"2.0","minor_version":"60"}')


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
