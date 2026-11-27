extends RefCounted
class_name GwNetLink
## 一条 TCP 连接（客户端侧）：分帧、收发、连接超时、可选重连。
##
## 用 Godot 的 `StreamPeerTCP`（非阻塞）：每帧 `poll()` 一次，就绪后把字节读进
## 内部缓冲区、按 `u16` 大端长度切帧、逐帧发 `frame_received`。
##
## # 分工（T67 之后：状态机在 Rust）
#
# | 这一层干什么 | 在哪 |
# |---|---|
# | `StreamPeerTCP` 的建/连/poll/读/写、信号 | **本文件**（引擎侧） |
# | 连接状态机（4 态判别值）、超时阈值、握手时机、失败后的 `GameState` | **Rust**：`GwClientLink` ← `graphwar_client_connection_handler` |
#
# 改前本文件自己维护一套 `"idle"/"connecting"/…` 字符串状态机（第二份实现），
# T67 接回 Rust（台账 `fixtures/controller_migration3_ledger.tsv` 的 L05–L09/L12）。
#
# # 证据
#
# * 内核 4 态 `ConnectionState { Disconnected=0, Connecting=1, Connected=2, Failed=3 }`，
#   判别值来自 4 个写入点的 `strb` 立即数（arm64 `0x48b900`/`0x48de10`/`0x48dd1c`/`disconnect`）**逐位**；
# * `connect_timeout`（30 s）= `CONNECT_TIMEOUT_SECS`，arm64 `0x48dccc mov w1,#0x1e` **逐位**；
#   本文件不写这个数字，判定也交给 Rust（`GwClientLink.step`）；
# * `retries` / `retry_delay`：原版重试策略**完全没有观测**（`docs/132` §3）⇒ **本项目选择**（台账 L04）；
# * **第 5 态 `ST_CLOSED`**（`-1`）是我方多出来的：原版 `disconnect` 直接写 0，会丢掉
#   "曾经连上过"，而 `link_closed` 信号要靠它区分"从没连上"。**自造**（台账 L05/L11）。
#
## # 为什么不在 `_ready` 里等
##
## `GwNet`（`net.gd`）是唯一的驱动者：它每帧 `poll(delta)` 一次所有连接。
## 这样界面线程不会被阻塞，`ConnectingPopUp` 的取消也能立刻生效。

signal connected                        ## TCP 真的连上了（可发第一帧）
signal frame_received(text: String)     ## 收到一条完整消息（原文）
signal link_closed(reason: String)      ## 曾经连上、现在断了
signal link_failed(reason: String)      ## 一次都没连上（重试也用完了）

const READ_CHUNK := 16384

## 内核 4 态（判别值从 Rust 取，见 `GwClientLink`）。**本文件不写 0/1/2/3**。
static var ST_DISCONNECTED: int = GwClientLink.state_disconnected()
static var ST_CONNECTING: int = GwClientLink.state_connecting()
static var ST_CONNECTED: int = GwClientLink.state_connected()
static var ST_FAILED: int = GwClientLink.state_failed()
## 我方多出来的第 5 态（**自造**，见文件头）。用它区分"连上后断开"与"从没连上"。
const ST_CLOSED := -1

## 当前状态（内核判别值，或 `ST_CLOSED`）。
var state := GwClientLink.state_disconnected()
var host := ""
var port := 0
## 单次连接尝试的超时（秒）。**原版真值 30 s**，从 Rust 取。这里只用于**人话提示**，
## 判定在 `GwClientLink.step` 里（超时阈值同一个常量）。
var connect_timeout := GwNetConsts.connect_timeout_secs()
## 失败后的重试次数（**本项目选择**，默认不重试；原版策略未观测 —— 台账 L04）。
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
	# 内核语义：`connect()` 写 1（`strb #1,[x21,#0x15]`，arm64 `0x48b900`）。
	state = GwClientLink.state_connecting()


func is_open() -> bool:
	return GwClientLink.is_open(state)


func is_busy() -> bool:
	return GwClientLink.is_busy(state)


## 每帧调用一次。`delta` 是秒。状态迁移**全部**由 `GwClientLink.step()` 决定
## （内核 `state_after_connect_attempt` + 超时常量）；本函数只做重试倒计时、
## 把决定落到 `state` 并发信号、读字节。
func poll(delta: float) -> void:
	if state == ST_FAILED or state == ST_CLOSED or state == ST_DISCONNECTED:
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
	if state == ST_CONNECTING:
		_elapsed += delta
		var step := GwClientLink.step(state, st, _elapsed)
		match String(step.get("action", "wait")):
			"wait":
				return
			"connected":
				state = int(step.get("next_state", GwClientLink.state_connected()))
				connected.emit()
				if state != ST_CONNECTED:
					return
			_:
				if bool(step.get("timed_out", false)):
					_attempt_failed("连接 %s:%d 超时（%.1f s）" % [host, port, connect_timeout])
				else:
					_attempt_failed("连接 %s:%d 失败（status %d）" % [host, port, st])
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
	# 帧层**真错误**（读侧 4096 上限，原版 `Message too large`）：这一帧永远切不出来，
	# 不断开的话 `rest` 会无限增长。判据与错误原文都来自 Rust（`GwStreamBuffer.decode_one`
	# ← `graphwar_stream_buffer::read_event`，arm64 `0x45c590`）。
	var frame_error := String(decoded.get("error", ""))
	if frame_error != "":
		_peer_closed("帧解码失败：%s" % frame_error)
		return
	_buf = decoded["rest"]
	for text in decoded["frames"]:
		frames_in += 1
		frame_received.emit(String(text))
		if state == ST_CLOSED or state == ST_FAILED:
			return


## 发一条消息原文。返回 false = 没连上或负载超过 u16（原版 `Message too large`）。
func send_text(text: String) -> bool:
	if not is_open() or _peer == null:
		last_error = "未连接时不能发送"
		return false
	var bytes := GwNetFrame.encode(text)
	if bytes.is_empty():
		last_error = "消息超过 %d 字节，无法成帧（原版：Message too long.）" % GwStreamBuffer.max_payload()
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
	if state != ST_FAILED:
		state = ST_CLOSED


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
		state = GwClientLink.state_connecting()
		return
	# 内核：连接线程失败 ⇒ 写 3（`strb #3`，arm64 `0x48dd1c`）。
	state = GwClientLink.after_connect_attempt(false)
	link_failed.emit(reason)


func _peer_closed(reason: String) -> void:
	_drop_socket()
	if state == ST_CLOSED or state == ST_FAILED:
		return
	state = ST_CLOSED
	last_error = reason
	link_closed.emit(reason)
