extends RefCounted
class_name GwLanDiscovery
## LAN 房间发现（**监听方**）：UDP、明文 JSON、**换行**分帧。
##
## # 分工（T67 之后：本类只剩引擎侧）
##
## | 这一层干什么 | 在哪 |
## |---|---|
## | `PacketPeerUDP` 的 bind / `get_packet` / 信号 | **本文件**（引擎侧） |
## | 解析 9 字段 `GameInfo`、按**发送方地址**去重、TTL 10 s 过期、广播周期 1 s | **Rust**：`GwLanV6` ← `graphwar_lan_broadcaster_v6` |
##
## 改前这段流程是 `.gd` 里的第二份实现（`_ingest` / `_expire` / `parse_broadcast`，
## 约 70 行）。真值一直是 Rust 侧那几个方法（`0x3de638 insert` / `0x3de6e4 cmp x0,#0xa`
## / `0x3de7c8`），所以 T67 把它接回来（台账
## `fixtures/controller_migration3_ledger.tsv` 的 D07–D13）。
##
## # 观测（`docs/24` §1.1、`docs/115` §5.3）
##
## * 原版客户端在 LanLobby 绑 **UDP `0.0.0.0:42968`**（`ss -ltnup` 实测）；
## * 房主往这个端口灌的报文是**裸的 9 字段对象 + 结尾 `"\n"`**，**没有**事件外壳
##   （`{"GameInfo":…}` 被丢弃）、**没有**长度前缀（那是 TCP 通道的分帧）；
## * 用原版客户端做过 5 个变体的受控实验：只有"裸对象 + 换行"被接受；
##   带 `"time_mode":"Timers1m"` 的那份**被丢弃** —— 见下面的枚举校验（**推断**）。
##
## # 三个**改前就与真值不符**的点（T67 挖出，`docs/06` §115.4）
##
## 1. **表的键**：真值是**发送方 `SocketAddr`**（`HashMap<SocketAddr,_>` 的 `insert` 在
##    `0x3de638`），改前 `.gd` 用报文里的 `address` 字段当键 ⇒ 两个不同 host 广播同一个
##    `address` 时原版是两行、我方合成一行（反过来同一 host 改 `address` 原版仍是一行、
##    我方变两行）。现在键在 Rust 里，`.gd` 拿不到也不该拿。
## 2. **TTL 判据**：真值 `secs >= 10`（`0x3de6e4 cmp x0,#0xa` + `b.lo` 保留），
##    改前 `.gd` 写 `> room_ttl_ms` ⇒ 每行**多留一帧**才被删。
## 3. **`MAX_ROOMS = 64`** 是我方自造的容量闸（Rust 的表没有上限比较）⇒ 已删。
##
## 仍然**属于我方**的两件（不是复原结论）：
## * `rooms()` 的**顺序** —— 原版 `HashMap` 迭代序未知；现在按发送方地址排序只为可复现；
## * `rooms_changed` 的**节流** —— 表内容没变就不发信号（原版 `GraphwarLanVBox::process`
##   每帧重取，没有这层抑制；这一层是 UI 侧，避免每秒重画）。

signal rooms_changed                 ## 房间表变了（增/删/人数变化）
signal failed(reason: String)        ## 绑定失败等

## 原版 LanLobby 监听的端口（观测，`docs/115` §5.3）。端口冲突是常态
## （同一台机器再开一个原版客户端也会绑它），所以绑定失败**不算致命错**。
##
## ⚠️ 这个数**不在** Rust 侧：`GraphwarLanBroadcasterV6::new(port)` 的端口是入参，
## 42968 是观测到的**调用方常量**（在 `GraphwarBackend`），本轮没解那条调用点
## （台账 D02）。
const DEFAULT_PORT := 42968

## 房间条目的存活时间（毫秒）= **原版 `ENTRY_TTL_SECS = 10` × 1000**，从 Rust 取。
## 判据本身在 Rust（`GwLanV6`）里判，这里只是给自检/排障读的**读数**。
var room_ttl_ms := GwNetConsts.entry_ttl_ms()

var port := 0
var last_error := ""

var _udp: PacketPeerUDP = null
## Rust 侧的"表 + 解析 + TTL"（`src/_scaffold/lan_v6_class.rs`）。
var _v6: GwLanV6 = null
var _clock_ms := 0                   # 由 poll 推进（毫秒），不直接读墙钟 —— 便于测试
var _packets := 0
## 上一帧的整张表（`rooms_changed` 的节流用；比较的是 Rust 给的整表）。
var _last_rows: Array = []


func _init() -> void:
	_v6 = GwLanV6.new()


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
		_v6.clear_bind()
		failed.emit(last_error)
		return err
	port = _udp.get_local_port()
	# 原版 `GraphwarLanBroadcasterV6::start`（`0x3de19c`）的三步：bind → join → 非阻塞。
	# 这里只把"bind 过了"记进 Rust（真 socket 是 Godot 的）。
	_v6.start(port)
	return OK


func stop() -> void:
	if _udp != null:
		_udp.close()
		_udp = null
	_v6.stop()
	_last_rows = []


## 每帧调用。`delta_ms` 用来推进时钟（TTL 判据在 Rust 里用它）。
func poll(delta_ms: int = 0) -> void:
	_clock_ms += maxi(delta_ms, 0)
	if _udp == null:
		return
	while _udp.get_available_packet_count() > 0:
		var pkt := _udp.get_packet()
		_packets += 1
		# **发送方地址**是表的键（原版 `HashMap<SocketAddr,_>`）；不带方括号的 IPv6
		# 由 Rust 侧 `parse_peer` 收。
		var from := "%s:%d" % [_udp.get_packet_ip(), _udp.get_packet_port()]
		_ingest(from, pkt)
	_v6.expire_stale(_clock_ms)
	var rows: Array = _v6.rooms(_clock_ms)
	if rows != _last_rows:
		_last_rows = rows
		rooms_changed.emit()


## 一段 UDP 负载（可能含多行，LAN 通道用换行分帧）→ Rust 的表。
func _ingest(from: String, bytes: PackedByteArray) -> void:
	var text := bytes.get_string_from_utf8()
	for raw in text.split("\n"):
		var line := String(raw).strip_edges()
		if line.is_empty():
			continue
		# 解析 / 去重 / 入表全在 Rust；`time_mode` 形状闸也在那边（推断，见 `GwLanV6`）。
		_v6.ingest(from, line.to_utf8_buffer(), _clock_ms)


## 当前房间表（**按发送方地址排序** —— 本项目选择，原版顺序未知）。
func rooms() -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for row in _v6.rooms(_clock_ms):
		out.append(row)
	return out


func room_count() -> int:
	return _v6.room_count()


func packets_seen() -> int:
	return _packets


# ---------------------------------------------------------------- 解析

## 解一条 LAN 广播行 → 9 个字段的字典（`current_player_count` 是 int）。
## 不合格返回 `{}`（**整条丢弃**，与原版受控实验的现象一致）。
##
## 解析在 Rust（`Broadcast::parse`，`src/_scaffold/protocol.rs:92`；9 字段顺序来自抓包）。
## 本函数只把 `time_mode` 形状闸（**推断**）的判定结果翻译成"整条丢弃"。
static func parse_broadcast(line: String) -> Dictionary:
	var d := GwLanV6.parse_broadcast(line)
	d.erase("time_mode_known")
	return d


## `time_mode` 必须是 `Timer<正整数><s|m>` 形状（**推断**，见 `GwLanV6::is_known_time_mode`）。
##
## 依据：原版客户端**丢弃**了 `"Timers1m"` 那个变体（`docs/115` §5.3 变体 E）。
static func is_known_time_mode(s: String) -> bool:
	return GwLanV6.time_mode_known(s)
