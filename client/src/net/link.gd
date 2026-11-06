extends RefCounted
class_name GwNetLink
## 一条 TCP 连接（客户端侧）：分帧、收发、连接超时、可选重连。
##
## 用 Godot 的 `StreamPeerTCP`（非阻塞）：每帧 `poll()` 一次，就绪后把字节读进
## 内部缓冲区、按 `u16` 大端长度切帧、逐帧发 `frame_received`。
##
## # 时间与状态
##
## 状态机：`idle → connecting →(失败, 还有重试次数) connecting … → connected`
##                                              `↘ failed`
## `connected → closed`（对端关闭 / 读错 / 帧非法）。
##
## * `connect_timeout`（默认 3 s）—— 只算**单次**尝试；
## * `retries`（默认 0）—— 失败后再试几次；两次之间等 `retry_delay`。
##   原版的连接重试策略**完全没有观测**（`docs/132` §3 只给了 `start_client_to_*` 的
##   方法名），所以这两个值都是**本项目的选择**，不是复原结论。
##
## # 为什么不在 `_ready` 里等
##
## `GwNet`（`net.gd`）是唯一的驱动者：它每帧 `poll(delta)` 一次所有连接。
## 这样界面线程不会被阻塞，`ConnectingPopUp` 的取消也能立刻生效。

signal connected                        ## TCP 真的连上了（可发第一帧）
signal frame_received(text: String)     ## 收到一条完整消息（原文）
signal link_closed(reason: String)      ## 曾经连上、现在断了
signal link_failed(reason: String)      ## 一次都没连上（重试也用完了）

const READ_CHUNK := 16384

## "idle" | "connecting" | "connected" | "closed" | "failed"
var state := "idle"
var host := ""
var port := 0
## 单次连接尝试的超时（秒）。**本项目选择**，非观测值。
var connect_timeout := 3.0
## 失败后的重试次数（**本项目选择**，默认不重试）。
var retries := 0
## 两次尝试之间的等待（秒）。
var retry_delay := 0.25
## 已经尝试了几次（自检用它断言"重试真的发生了"）。
var attempts := 0
## 最近一次错误的人话描述。
var last_error := ""

var _peer: StreamPeerTCP = null
var _buf := PackedByteArray()
var _elapsed := 0.0
var _retry_left := 0.0
var _waiting_retry := false
## 收到过多少字节 / 多少帧（自检与文档里的端到端记录用）。
var bytes_in := 0
var frames_in := 0


func start(p_host: String, p_port: int) -> void:
	host = p_host
	port = p_port
	last_error = ""
	bytes_in = 0
	frames_in = 0
	_buf = PackedByteArray()
	attempts = 0
	_begin_attempt()


func _begin_attempt() -> void:
	attempts += 1
	_elapsed = 0.0
	_waiting_retry = false
	# 上一次尝试可能留下半帧：新连接上不能接旧的残渣
	_buf = PackedByteArray()
	_peer = StreamPeerTCP.new()
	var err := _peer.connect_to_host(host, port)
	if err != OK:
		_peer = null
		_attempt_failed("连不上 %s:%d（connect_to_host 返回 %d）" % [host, port, err])
		return
	# 关掉 Nagle：抓包里的握手是**一次 write 吐出 7 帧**，延迟合并会让"批"的边界漂。
	# ⚠️ 必须**在 connect 之后**调：socket 还没建时 Godot 会报
	# `_sock.is_null() || !_sock->is_open()`（这条错误本轮实测踩到过）。
	_peer.set_no_delay(true)
	state = "connecting"


func is_open() -> bool:
	return state == "connected"


func is_busy() -> bool:
	return state == "connecting"


## 每帧调用一次。`delta` 是秒。
func poll(delta: float) -> void:
	if state == "failed" or state == "closed" or state == "idle":
		return
	if _waiting_retry:
		_retry_left -= delta
		if _retry_left <= 0.0:
			_begin_attempt()
		return
	if _peer == null:
		return
	_peer.poll()
	var st := _peer.get_status()
	if state == "connecting":
		if st == StreamPeerTCP.STATUS_CONNECTING:
			_elapsed += delta
			if _elapsed >= connect_timeout:
				_attempt_failed("连接 %s:%d 超时（%.1f s）" % [host, port, connect_timeout])
			return
		if st == StreamPeerTCP.STATUS_ERROR or st == StreamPeerTCP.STATUS_NONE:
			_attempt_failed("连接 %s:%d 失败（status %d）" % [host, port, st])
			return
		# STATUS_CONNECTED —— 首次连上，之后同一帧就把可能已经到达的字节读掉。
		state = "connected"
		connected.emit()
		if state != "connected":
			return
	if st == StreamPeerTCP.STATUS_ERROR or st == StreamPeerTCP.STATUS_NONE:
		# 对端关闭。Godot 4 的 `StreamPeerTCP` 一旦 poll 到 EOF/ERROR 就**不再交数据**
		# （`get_available_bytes()` 会打 `!is_open()` 并返回 -1），所以这里只能报关闭；
		# "最后一批数据 + 立刻关闭"会丢掉那一批 —— 记在 docs/134 §7 的已知边界里。
		_peer_closed("对端关闭了连接")
		return
	_read_available()


func _read_available() -> void:
	# 已经断开的 socket 上 `get_available_bytes()` 会打一条 `Condition "!is_open()"` 错误
	# 并返回 -1，所以先看状态。（代价：如果对端"把最后一批数据发完就立刻关"，
	# 那一批可能读不到 —— 与 `src/graphwar_game_server.rs::read_frame` 的 `read_exact` 失败同性质。）
	if _peer == null or _peer.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		return
	while _peer != null and _peer.get_available_bytes() > 0:
		var res: Array = _peer.get_partial_data(READ_CHUNK)
		if int(res[0]) != OK:
			_peer_closed("读取出错（%d）" % int(res[0]))
			return
		var chunk: PackedByteArray = res[1]
		if chunk.is_empty():
			break
		bytes_in += chunk.size()
		_buf.append_array(chunk)
	var decoded := GwNetFrame.decode_buffer(_buf)
	if bool(decoded.get("bad_utf8", false)):
		_peer_closed("收到非法 UTF-8 帧")
		return
	_buf = decoded["rest"]
	for text in decoded["frames"]:
		frames_in += 1
		frame_received.emit(String(text))
		if state == "closed" or state == "failed":
			return


## 发一条消息原文。返回 false = 没连上或负载超过 u16（原版 `Message too large`）。
func send_text(text: String) -> bool:
	if not is_open() or _peer == null:
		last_error = "未连接时不能发送"
		return false
	var bytes := GwNetFrame.encode(text)
	if bytes.is_empty():
		last_error = "消息超过 %d 字节，无法成帧（原版：Message too large）" % GwNetFrame.MAX_PAYLOAD
		return false
	var err := _peer.put_data(bytes)
	if err != OK:
		last_error = "写入失败（%d）" % err
		return false
	return true


## 主动关掉（不触发 `link_closed`：这个动作是**我们自己**发起的）。
func shutdown() -> void:
	_drop_socket()
	_waiting_retry = false
	_buf = PackedByteArray()
	if state != "failed":
		state = "closed"


## 放掉 socket。**只有真的开着才调 `disconnect_from_host()`** ——
## 对没连上的 peer 调它会返回 -1 并打一条 `Condition "!is_open()"` 错误（实测踩到过）。
func _drop_socket() -> void:
	if _peer == null:
		return
	var st := _peer.get_status()
	if st == StreamPeerTCP.STATUS_CONNECTED or st == StreamPeerTCP.STATUS_CONNECTING:
		_peer.disconnect_from_host()
	_peer = null


func _attempt_failed(reason: String) -> void:
	last_error = reason
	_drop_socket()
	if attempts <= retries:
		_waiting_retry = true
		_retry_left = retry_delay
		state = "connecting"
		return
	state = "failed"
	link_failed.emit(reason)


func _peer_closed(reason: String) -> void:
	_drop_socket()
	if state == "closed" or state == "failed":
		return
	state = "closed"
	last_error = reason
	link_closed.emit(reason)
