extends RefCounted
class_name GwLanDiscovery
## LAN 房间发现（**监听方**）：UDP、明文 JSON、**换行**分帧。
##
## # 观测（`docs/24` §1.1、`docs/115` §5.3）
##
## * 原版客户端在 LanLobby 绑 **UDP `0.0.0.0:42968`**（`ss -ltnup` 实测）；
## * 房主往这个端口灌的报文是**裸的 9 字段对象 + 结尾 `"\n"`**，**没有**事件外壳
##   （`{"GameInfo":…}` 被丢弃）、**没有**长度前缀（那是 TCP 通道的分帧）；
## * 用原版客户端做过 5 个变体的受控实验：只有"裸对象 + 换行"被接受；
##   带 `"time_mode":"Timers1m"` 的那份**被丢弃** —— 见下面的枚举校验。
##
## # 我们这一侧的分工（重要）
##
## 本类**只负责听**。真正的"广播循环"在原版是 `graphwar_lan_broadcaster_v6.rs`
## （IPv6 `ff02::1`，`docs/24` §7.2），我们**没有**它（`src/` 本轮不动）。
## 所以自检/端到端里由测试自己发一条同形状的报文把"房主"演出来 ——
## 这正是 `docs/115` §5.3 用原版客户端做的实验，反过来做。
##
## # 三个"本项目选择"（都写进 docs/134）
##
## 1. `ROOM_TTL_MS` —— 房间多久没更新就认为没了。原版广播周期/超时**未知**
##    （`GameHeartbeat` 从未抓到，`docs/96` §3）。
## 2. `rooms()` 的顺序 —— 按 `address` 排序。原版列表顺序**未知**；
##    排序只是为了可复现，不作为复原结论（与 `src/_scaffold/broker.rs` 的口径一致）。
## 3. 「枚举值不在已知集合里就整条丢弃」—— 这是从原版受控实验反推的行为
##    （`docs/115` §5.3 变体 E），**推断**而非逐位验证。

signal rooms_changed                 ## 房间表变了（增/删/人数变化）
signal failed(reason: String)        ## 绑定失败等

## 原版 LanLobby 监听的端口（观测，`docs/115` §5.3）。端口冲突是常态
## （同一台机器再开一个原版客户端也会绑它），所以绑定失败**不算致命错**。
const DEFAULT_PORT := 42968
const ROOM_TTL_MS := 12000
const MAX_ROOMS := 64

## 已知的 `time_mode` 取值形状。抓包实际只观测到 `Timer1m` / `Timer3m`
## （`docs/24` §1.1 的更正），其余档位是**按同形状推断**的。
const GAME_STATES := ["Setup", "WaitingForFunctions", "ExecutingResults", "EndingGame"]

var port := 0
var last_error := ""

var _udp: PacketPeerUDP = null
var _rooms := {}                     # address -> {…9 个字段…, "last_seen_ms": int}
var _clock_ms := 0                   # 由 poll 推进（毫秒），不直接读墙钟 —— 便于测试
var _packets := 0


func is_listening() -> bool:
	return _udp != null


## 开始监听。`p_port` 传 0 ⇒ 让系统选一个（自检用，避免抢 42968）。
## 返回 Godot 的 Error 码（OK = 0）。
func listen(p_port: int = DEFAULT_PORT, bind_address: String = "*") -> int:
	stop()
	_udp = PacketPeerUDP.new()
	var err := _udp.bind(p_port, bind_address)
	if err != OK:
		last_error = "UDP 端口 %d 绑定失败（%d）—— 可能已有另一个客户端在跑" % [p_port, err]
		_udp = null
		failed.emit(last_error)
		return err
	port = _udp.get_local_port()
	return OK


func stop() -> void:
	if _udp != null:
		_udp.close()
		_udp = null
	_rooms.clear()


## 每帧调用。`delta_ms` 用来淘汰过期房间。
func poll(delta_ms: int = 0) -> void:
	_clock_ms += maxi(delta_ms, 0)
	if _udp == null:
		return
	var changed := false
	while _udp.get_available_packet_count() > 0:
		var pkt := _udp.get_packet()
		_packets += 1
		var n := _ingest(pkt.get_string_from_utf8())
		if n > 0:
			changed = true
	changed = _expire() or changed
	if changed:
		rooms_changed.emit()


## 处理一段 UDP 负载（可能含多行）。返回新增/更新的条数。
func _ingest(text: String) -> int:
	var n := 0
	for raw in text.split("\n"):
		var line := String(raw).strip_edges()
		if line.is_empty():
			continue
		var info := parse_broadcast(line)
		if info.is_empty():
			continue
		var key := String(info["address"])
		if not _rooms.has(key) and _rooms.size() >= MAX_ROOMS:
			continue
		info["last_seen_ms"] = _clock_ms
		if _rooms.has(key) and _same_row(_rooms[key], info):
			# 内容没变：只刷新时间戳，别把界面刷成"每秒重画"。
			_rooms[key]["last_seen_ms"] = _clock_ms
			continue
		_rooms[key] = info
		n += 1
	return n


static func _same_row(a: Dictionary, b: Dictionary) -> bool:
	for k in ["room_name", "game_state", "current_player_count", "axis_mode",
			"function_mode", "turn_mode", "time_mode", "locked"]:
		if a.get(k) != b.get(k):
			return false
	return true


func _expire() -> bool:
	var dead: Array[String] = []
	for k in _rooms:
		if _clock_ms - int(_rooms[k]["last_seen_ms"]) > ROOM_TTL_MS:
			dead.append(String(k))
	for k in dead:
		_rooms.erase(k)
	return not dead.is_empty()


## 当前房间表（按 `address` 排序 —— **本项目选择**，原版顺序未知）。
func rooms() -> Array[Dictionary]:
	var keys := _rooms.keys()
	keys.sort()
	var out: Array[Dictionary] = []
	for k in keys:
		out.append((_rooms[k] as Dictionary).duplicate())
	return out


func room_count() -> int:
	return _rooms.size()


func packets_seen() -> int:
	return _packets


# ---------------------------------------------------------------- 解析

## 解一条 LAN 广播行。返回 9 个字段的字典（`current_player_count` 是 int），
## 不合格返回 `{}`（**整条丢弃**，与原版受控实验的现象一致）。
static func parse_broadcast(line: String) -> Dictionary:
	var t := line.strip_edges()
	if t.is_empty() or not t.begins_with("{"):
		return {}
	var parsed = JSON.parse_string(t)
	if typeof(parsed) != TYPE_DICTIONARY:
		return {}
	var d: Dictionary = parsed
	# 裸对象：**没有**事件外壳，所以不能是单键的 {"GameInfo":…} 形状。
	if d.has("game_info"):
		return {}
	for k in ["room_name", "game_state", "address", "axis_mode", "function_mode",
			"turn_mode", "time_mode"]:
		if typeof(d.get(k, null)) != TYPE_STRING or String(d[k]).is_empty():
			return {}
	if typeof(d.get("current_player_count", null)) != TYPE_FLOAT \
			and typeof(d.get("current_player_count", null)) != TYPE_INT:
		return {}
	if typeof(d.get("locked", null)) != TYPE_BOOL:
		return {}
	if not is_known_time_mode(String(d["time_mode"])):
		return {}
	return {
		"room_name": String(d["room_name"]),
		"game_state": String(d["game_state"]),
		"current_player_count": int(d["current_player_count"]),
		"address": String(d["address"]),
		"axis_mode": String(d["axis_mode"]),
		"function_mode": String(d["function_mode"]),
		"turn_mode": String(d["turn_mode"]),
		"time_mode": String(d["time_mode"]),
		"locked": bool(d["locked"]),
	}


## `time_mode` 必须是 `Timer<正整数><s|m>` 形状。
##
## 依据：原版客户端**丢弃**了 `"Timers1m"` 那个变体（`docs/115` §5.3 变体 E）。
## 最自然的解释是这个字段在 Rust 侧被反序列化成枚举，非法字符串让整条报文解析失败。
## **推断**——只观测到"被丢弃"，没有观测到判定发生在哪一层。
static func is_known_time_mode(s: String) -> bool:
	if not s.begins_with("Timer"):
		return false
	var rest := s.substr(5)
	if rest.length() < 2:
		return false
	var unit := rest.substr(rest.length() - 1, 1)
	if unit != "s" and unit != "m":
		return false
	var digits := rest.substr(0, rest.length() - 1)
	return digits.is_valid_int() and int(digits) > 0
