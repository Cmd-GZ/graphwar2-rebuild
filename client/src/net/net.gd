extends Node
class_name GwNet
## 联机层的外观（`App.net`）：连接、LAN 房间发现、broker 房间表。
##
## 这是**唯一**被界面持有的节点；三个通道各自是一个 `RefCounted`：
##
## ```text
## GwNet (Node, 每帧 poll)
##   ├─ lan     : GwLanDiscovery    UDP 42968，收房主的裸 GameInfo 行（`docs/24` §1.1）
##   ├─ broker  : GwBrokerClient    TCP u16+JSON，VersionInfo → GameInfo（`docs/115`）
##   └─ session : GwNetSession      TCP u16+JSON，加入一局（`docs/33`/`docs/62`）
## ```
##
## 为什么放在 `App` 的子节点而不是 autoload：`App` 已经是 autoload，多一个单例就多一个
## 初始化顺序问题；而且 `GwNet` 需要**只在** `App` 活着时轮询。
##
## # 不做的两件事（都写进 docs/134）
##
## 1. **不发任何外网请求**：原版 broker 地址默认来自
##    `https://graphwar.com/graphwar_II_ip.txt`（`docs/115` §4），本客户端只在
##    `GW2_BROKER_ADDR`（或界面显式设置）时才连 broker。
## 2. **不做 UDP 广播**：原版房主的广播循环在 `graphwar_lan_broadcaster_v6.rs`，
##    我们这一侧只有监听方（`src/` 本轮不动）。

signal rooms_changed(mode: String)     ## "local" | "global"
signal joined                          ## 加入成功（握手完成）
signal join_failed(reason: String)     ## 连不上 / 握手超时
signal disconnected(reason: String)    ## 连上之后被断开
signal game_started                    ## 服务端把开局那一批发完了
signal ended(reason: String)           ## 对局正常结束（`EndingGame`）

var session: GwNetSession = null
var lan: GwLanDiscovery = null
var broker: GwBrokerClient = null

## LAN 监听端口。原版是 42968（观测）；自检里换成临时端口以免和别的实例抢。
var lan_port := GwLanDiscovery.DEFAULT_PORT
## broker 地址（"host:port"）。空 = 不连（默认；见文件头第 1 条）。
var broker_address := ""

var _was_joined := false


func _ready() -> void:
	name = "Net"
	lan = GwLanDiscovery.new()
	lan.rooms_changed.connect(func(): rooms_changed.emit("local"))
	lan.failed.connect(func(reason: String):
		push_warning("[gw2/net] LAN 发现不可用：%s" % reason))
	var env := OS.get_environment("GW2_BROKER_ADDR").strip_edges()
	if not env.is_empty():
		broker_address = env
	if not env.is_empty():
		browse_global()


func _process(delta: float) -> void:
	if lan != null:
		lan.poll(int(delta * 1000.0))
	if broker != null:
		broker.poll(delta)
	if session != null:
		session.poll(delta)


# ---------------------------------------------------------------- 加入一局

## 连一台对局服务器。`opts` 可覆盖 `auto_start` / `request_player`（见 `GwNetSession`）。
func join(host: String, port: int, opts := {}) -> bool:
	leave()
	_was_joined = false
	var s := GwNetSession.new()
	s.auto_start = bool(opts.get("auto_start", true))
	s.request_player = bool(opts.get("request_player", true))
	s.joined.connect(_on_session_joined)
	s.game_started.connect(_on_session_game_started)
	s.failed.connect(_on_session_failed)
	s.closed.connect(_on_session_closed)
	s.ended.connect(_on_session_ended)
	session = s
	s.start(host, port)
	return true


## 放弃当前连接（取消 / 离开对局）。不发任何报文，直接关 socket。
func leave() -> void:
	if session != null:
		session.abort()
		session = null
	_was_joined = false


func is_online() -> bool:
	return session != null and session.is_joined()


func _on_session_joined() -> void:
	_was_joined = true
	joined.emit()


func _on_session_game_started() -> void:
	game_started.emit()


func _on_session_failed(reason: String) -> void:
	_was_joined = false
	join_failed.emit(reason)


func _on_session_closed(reason: String) -> void:
	if _was_joined:
		_was_joined = false
		disconnected.emit(reason)
	else:
		join_failed.emit(reason)


func _on_session_ended(reason: String) -> void:
	ended.emit(reason)


# ---------------------------------------------------------------- LAN 房间发现

## 开始监听 LAN 广播。`p_port` 传 0 ⇒ 系统选端口（自检用）。
func lan_listen(p_port: int = GwLanDiscovery.DEFAULT_PORT) -> int:
	if lan == null:
		return ERR_UNCONFIGURED
	var err := lan.listen(p_port)
	if err == OK:
		lan_port = lan.port
	return err


func lan_stop() -> void:
	if lan != null:
		lan.stop()


func local_rooms() -> Array[Dictionary]:
	return lan.rooms() if lan != null else []


func lan_listening() -> bool:
	return lan != null and lan.is_listening()


# ---------------------------------------------------------------- broker（Global 页）

## 设置 broker 地址（"host:port"）。空串 = 停掉。
func set_broker_address(addr: String) -> void:
	broker_address = addr.strip_edges()
	if broker_address.is_empty():
		stop_broker()
		return
	browse_global()


## 连 broker 并取房间表。地址为空时**什么都不做**（Global 页保持占位）。
##
## **幂等**：已经连着同一个地址时直接返回，不重连。`LanLobby.on_enter()` 与
## `App` 的 `show_global` 分支都会调它，重连会把刚收到的房间表清空（并且把 broker
## 那侧的连接数变成 2，真实 broker 会当成两个浏览者）。
func browse_global() -> bool:
	if broker_address.is_empty():
		return false
	var parsed := split_host_port(broker_address)
	if parsed.is_empty():
		push_warning("[gw2/net] broker 地址不合法：%s" % broker_address)
		return false
	if broker != null and broker.is_active() and broker.host == String(parsed["host"]) \
			and broker.port == int(parsed["port"]):
		return true
	if broker != null:
		broker.stop()
		broker = null
	var b := GwBrokerClient.new()
	b.rooms_changed.connect(func(): rooms_changed.emit("global"))
	b.failed.connect(func(reason: String):
		push_warning("[gw2/net] broker 不可用：%s" % reason))
	broker = b
	b.start(String(parsed["host"]), int(parsed["port"]))
	return true


func stop_broker() -> void:
	if broker != null:
		broker.stop()
		broker = null


func global_rooms() -> Array[Dictionary]:
	return broker.rooms() if broker != null else []


func broker_browsing() -> bool:
	return broker != null and broker.is_active()


# ---------------------------------------------------------------- 地址解析

## `host:port` → `{"host": …, "port": …}`；不合法返回 `{}`。
##
## 规则与 `App._looks_like_host_port` 一致（**观测**：原版只写 IP 时毫无反应，
## `docs/92` §4.7）。另外允许 `[::1]:1234` 这种 IPv6 写法 —— 抓包里的对局地址就是
## `[::1]:35245` 形状（`docs/24` §1.1）。
static func split_host_port(s: String) -> Dictionary:
	var t := s.strip_edges()
	if t.is_empty():
		return {}
	var idx := t.rfind(":")
	if idx <= 0 or idx == t.length() - 1:
		return {}
	var host := t.substr(0, idx)
	var port_s := t.substr(idx + 1)
	if not port_s.is_valid_int():
		return {}
	var p := port_s.to_int()
	if p < 1 or p > 65535:
		return {}
	if host.begins_with("[") and host.ends_with("]"):
		host = host.substr(1, host.length() - 2)
	if host.strip_edges().is_empty():
		return {}
	return {"host": host, "port": p}
