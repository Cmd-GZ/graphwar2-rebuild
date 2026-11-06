extends RefCounted
class_name GwBrokerClient
## Broker（房间列表服务器）**浏览方**会话：连上去、报版本、收房间表。
##
## # 观测（`docs/115` §1/§2，逐位）
##
## 分帧与对局连接**同一套**：`u16` 大端长度 + JSON（同一个写入函数 `0x3d89e0`）。
## 一次真实会话（`fixtures/protocol_match.jsonl:83–86`）：
##
## ```text
## -> {"VersionInfo":{"version":"2.0.60","system":"linux"}}
## <- {"GameInfo":{"game_info":{…9 字段…}}}
## <- {"GameInfo":{"game_info":{…}}}
## <- {"GameInfo":{"game_info":{…}}}
## ```
##
## 即：连上先报版本，broker 把当前房间表**逐条**推回来；房间消失时是
## `GameRemoved{game_address}`（**该条从未抓到过真实报文**，见 `docs/115` §2.3）。
##
## # 我们不做的事
##
## 原版的 broker 地址默认来自 `https://graphwar.com/graphwar_II_ip.txt`。
## **本项目不发任何外网请求**：地址只能由 `GW2_BROKER_ADDR` 环境变量或界面侧显式
## 指定（`App.net.set_broker_address()`），默认空 = 不发、不连。
## 于是"Global 页默认显示 4 条占位"这件事仍然成立（`docs/129` §4.4 的真帧口径）。

signal rooms_changed
signal opened                        ## 连上并已发出 VersionInfo
signal failed(reason: String)

## 帮助文本里的示例端口（观测，`docs/115` §6.1 / `src/_scaffold/broker.rs::DEFAULT_PORT`）。
const DEFAULT_PORT := 42970

## "idle" | "connecting" | "browsing" | "failed"
var state := "idle"
var host := ""
var port := 0
var last_error := ""

var _link: GwNetLink = null
var _rooms := {}                     # address -> 9 字段字典


func start(p_host: String, p_port: int) -> void:
	stop()
	host = p_host
	port = p_port
	last_error = ""
	state = "connecting"
	_link = GwNetLink.new()
	_link.connect_timeout = 3.0
	_link.connected.connect(_on_connected)
	_link.frame_received.connect(_on_frame)
	_link.link_failed.connect(_on_link_failed)
	_link.link_closed.connect(_on_link_closed)
	_link.start(host, port)


func stop() -> void:
	if _link != null:
		_link.shutdown()
		_link = null
	state = "idle"
	_rooms.clear()


func is_active() -> bool:
	return state == "browsing" or state == "connecting"


func poll(delta: float) -> void:
	if _link != null:
		_link.poll(delta)


func _on_connected() -> void:
	state = "browsing"
	# 逐字节发抓包原文（含 version="2.0.60"）：broker 侧只把它当"会话第一帧"。
	# ⚠️ 我们自己的客户端版本是 `main.gd::CLIENT_VERSION`，这里**故意**用原版原文，
	# 属"为了与原版/broker 会话形态一致"的有意偏离，记录在 docs/134 §5。
	_link.send_text(GwWireSamples.VERSION_INFO)
	opened.emit()


func _on_frame(text: String) -> void:
	var msg := GwNetFrame.parse_message(text)
	if msg.is_empty():
		return
	var kind := String(msg["kind"])
	match kind:
		"GameInfo":
			var inner := GwNetFrame.as_dict(msg, "game_info")
			var addr := String(inner.get("address", ""))
			if addr.is_empty():
				return
			if _rooms.has(addr) and _same(_rooms[addr], inner):
				return
			_rooms[addr] = inner.duplicate()
			rooms_changed.emit()
		"GameRemoved":
			var gone := GwNetFrame.as_str(msg, "game_address")
			if _rooms.has(gone):
				_rooms.erase(gone)
				rooms_changed.emit()
		_:
			pass        # 其它事件（Subscribe / GameHeartbeat…）不解析字段，只记录不动作


static func _same(a: Dictionary, b: Dictionary) -> bool:
	for k in ["room_name", "game_state", "current_player_count", "axis_mode",
			"function_mode", "turn_mode", "time_mode", "locked"]:
		if a.get(k) != b.get(k):
			return false
	return true


func _on_link_failed(reason: String) -> void:
	state = "failed"
	last_error = reason
	failed.emit(reason)


func _on_link_closed(reason: String) -> void:
	if state == "browsing":
		state = "idle"
		last_error = reason


## 房间表（按 `address` 排序 —— **本项目选择**，原版顺序未知）。
func rooms() -> Array[Dictionary]:
	var keys := _rooms.keys()
	keys.sort()
	var out: Array[Dictionary] = []
	for k in keys:
		out.append((_rooms[k] as Dictionary).duplicate())
	return out


func room_count() -> int:
	return _rooms.size()


# ---------------------------------------------------------------- 房主方向（推断）

## 把一条房间登记到 broker 上（房主方向）。
##
## **推断，未观测**：真实会话只抓到"浏览方"。这里按"房主在自己那条 broker 连接上
## 发 `GameInfo`"实现 —— `docs/115` §4 的理由是除此之外没有任何通道能到达 broker。
## 本轮界面**没有**调用它（我们的服务端还没有"房主注册"这一层），留着是为了让
## 协议的对称性可测。
func announce(info: Dictionary) -> bool:
	if _link == null or not _link.is_open():
		return false
	var addr := String(info.get("address", ""))
	if addr.is_empty():
		return false
	var body := GwWireSamples.broadcast(
		String(info.get("room_name", "")), String(info.get("game_state", "Setup")),
		int(info.get("current_player_count", 0)), addr,
		String(info.get("axis_mode", "EveryUnit")),
		String(info.get("function_mode", "NormalFunction")),
		String(info.get("turn_mode", "SequentialTurns")),
		String(info.get("time_mode", "Timer1m")),
		bool(info.get("locked", false)))
	return _link.send_text(GwNetFrame.message("GameInfo", '{"game_info":%s}' % body))
