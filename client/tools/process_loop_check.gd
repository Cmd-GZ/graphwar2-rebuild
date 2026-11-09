extends SceneTree
## **60 Hz 定步 `process` 循环**的验收脚本（`docs/16-client.md` §50，任务 47）。
##
## ```text
## cargo build --offline --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## "$GODOT" --headless --path client --import
## "$GODOT" --headless --path client --script res://tools/process_loop_check.gd ; echo "EXIT=$?"
## ```
##
## 验的是**内核侧定步语义**（真值在反汇编里，见 `docs/16-client.md` §48.3；本脚本只是回归保护）：
##
## 1. 每帧进来一次（`KernelBus::drive_frame` = 原版 `GraphwarGameClient::process`，x86 `0x518400`），
##    里面按 **1/60 s 墙钟**追赶：**喂 1 秒 ⇒ 恰好 60 个 tick**（一次调用里追完，不是一帧一个）；
## 2. **喂 2.5 个 tick ⇒ 转 2 圈、留下 0.5 个 tick**（`get_time_to_tick` 读回 `8 333 333` ns）；
## 3. **先 tick 后泵**（`0x518496` → `0x51849c`）：`tick` 产出的 `TickEvent`（判别值 0）
##    在**同一次**泵里就投递给订阅者 —— 顺序若反了，它要等下一帧；
## 4. 帧率不决定 tick 数（60 帧 × 半 tick = 30 tick），掉帧在下一帧补回来；
## 5. `delta` 参数**不被使用**（原版 x86 不读 `xmm0`、arm64 不读 `d0`）；
## 6. `get_time_to_tick` 被调次数 == 帧数 + due tick 数（实测口径，`§48.4` 的 `112 = 52 + 60`）。
##
## 它**不**验"播放屏每帧调这里"——`client/src/**` 是禁区（见 §50 的差异表）。
## 第 9 组更进一步：**把"不接"这个决策本身钉成断言**（任务 85，`docs/16-client.md` §58）——
## 生产客户端里没有驱动/通道符号、没有收件人时泵只会抽干队列、以及"接上之后 5 个读数会变成什么"。
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const EXTENSION_PATH := "res://gw2rebuild.gdextension"
const HOST := "KernelProcessHost"
## `0xfe502a` = 16 666 666 ns（`GraphwarTickHandler::tick` 的加法立即数）。
const NANOS_PER_TICK := 16_666_666
## `0x3b9aca00` = 1e9（`0x519450` 的"欠一个 tick"哨兵，`0x51848e` 的比较立即数）。
const DUE_SENTINEL_NANOS := 1_000_000_000

const PLAY_SCREEN_SRC := "res://src/screens/play_screen.gd"
const CLIENT_SRC_DIR := "res://src"
## 生产路径里**不许**出现的驱动/通道符号（它们只允许出现在 `tools/*_check.gd` 这类验收脚本里）。
## 出现任何一个 ⇒ 有人把驱动接进了活动客户端 ⇒ 决策（§58.3）必须重做。
const PRODUCTION_FORBIDDEN := [
	"drive_frame", "KernelProcessHost", "kernel_subscribe", "kernel_pump",
]

const METHODS := [
	"reset", "subscribe", "set_elapsed_nanos", "drive_frame",
	"last_probes", "last_ticks", "last_pumps", "ticks_total",
	"tick_handler_tick", "frames", "probes_total", "pumps_total",
	"time_to_tick_nanos", "due_sentinel_nanos", "fractional_tick",
	"queue_len", "delivered_tags", "delivered_count", "subscriber_count",
]

var _checks := 0
var _failures: PackedStringArray = []
## 扫描器这一轮看过几个 `.gd`（负结论的正对照：0 个文件 ⇒ "没命中"毫无意义）。
var _scan_files := 0

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## 新起一台宿主要先把通道拆掉重装：容器、tick handler、基准点都是新的。
func _host() -> Object:
	if not ClassDB.class_exists(HOST):
		print("      load_extension(%s) -> %d" % [EXTENSION_PATH,
			GDExtensionManager.load_extension(EXTENSION_PATH)])
	var host: Object = ClassDB.instantiate(HOST)
	host.call("reset")
	return host

func _method_names(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

## 递归收 `.gd`（`res://src/**`；`client/` 是活动客户端，`tools/` 不在它下面）。
func _gd_files(dir_path: String, out: PackedStringArray) -> void:
	var d := DirAccess.open(dir_path)
	if d == null:
		return
	d.list_dir_begin()
	var entry := d.get_next()
	while entry != "":
		if not entry.begins_with("."):
			var p := dir_path.path_join(entry)
			if d.current_is_dir():
				_gd_files(p, out)
			elif entry.ends_with(".gd"):
				out.append(p)
		entry = d.get_next()
	d.list_dir_end()

## 在 `client/src/**` 里搜一组符号，返回 `文件:符号` 的命中清单（空 = 生产路径干净）。
func _scan_client_sources(symbols: Array) -> PackedStringArray:
	var files := PackedStringArray()
	_gd_files(CLIENT_SRC_DIR, files)
	_scan_files = files.size()
	var hits := PackedStringArray()
	for path in files:
		var text: String = FileAccess.get_file_as_string(path)
		for sym in symbols:
			if text.contains(String(sym)):
				hits.append("%s:%s" % [path, sym])
	return hits

func _initialize() -> void:
	print("=== 60 Hz 定步 process 循环验收（docs/16-client.md §50）")

	# 0) 扩展与类
	if not ClassDB.class_exists(HOST):
		print("      load_extension(%s) -> %d" % [EXTENSION_PATH,
			GDExtensionManager.load_extension(EXTENSION_PATH)])
	ok(ClassDB.class_exists(HOST), "ClassDB 里有验收外壳 %s" % HOST)
	var host: Object = ClassDB.instantiate(HOST)
	ok(host != null, "ClassDB.instantiate(\"%s\") 非空" % HOST)
	if host == null:
		_finish()
		return
	var names: PackedStringArray = _method_names(host)
	var missing: PackedStringArray = PackedStringArray()
	for m in METHODS:
		if not names.has(m):
			missing.append(m)
	ok(missing.is_empty(), "方法表齐（缺：%s）" % [missing])
	ok(int(host.call("due_sentinel_nanos")) == DUE_SENTINEL_NANOS,
		"到期哨兵 == 1e9（`0x51848e` 的 `cmp edx,0x3b9aca00`）")
	ok(int(host.call("ticks_total")) == 0 and int(host.call("frames")) == 0,
		"刚装上 ⇒ 0 tick / 0 帧")

	# 1) 不欠账 ⇒ 一次都不转（循环头只探一次 = `probes == 1`）
	ok(bool(host.call("reset")) == true, "reset() 重装通道")
	ok(bool(host.call("set_elapsed_nanos", 0)) == true, "把墙钟摆到基准 + 0 ns")
	var t0: int = int(host.call("drive_frame", 0.0))
	ok(t0 == 0, "不欠账 ⇒ 本帧 0 个 tick（实际 %d）" % t0)
	ok(int(host.call("last_probes")) == 1, "不欠账 ⇒ `get_time_to_tick` 只调 1 次（循环头）")
	ok(int(host.call("last_pumps")) == 0, "没 tick 就没有泵")
	ok(int(host.call("time_to_tick_nanos")) == NANOS_PER_TICK,
		"离下一个 tick 还有 16 666 666 ns（1/60 s）")

	# 2) 一帧里喂 1/60 s ⇒ 恰好 1 个 tick、1 次泵，探针 2 次（1 帧 + 1 tick）
	ok(bool(host.call("set_elapsed_nanos", NANOS_PER_TICK)) == true, "墙钟推进一个 tick")
	var t1: int = int(host.call("drive_frame", 0.0))
	ok(t1 == 1, "1/60 s ⇒ 1 个 tick（实际 %d）" % t1)
	ok(int(host.call("last_pumps")) == 1, "每个 tick 后恰好一次泵")
	ok(int(host.call("last_probes")) == 2, "探针 = 1 + tick 数 = 2")
	ok(int(host.call("tick_handler_tick")) == 1, "tick handler 的 current_tick == 1")

	# 3) **一次喂 1 秒 ⇒ 恰好 60 个 tick**（追赶循环，不是一帧一个）
	host = _host()
	ok(bool(host.call("set_elapsed_nanos", 1_000_000_000)) == true, "墙钟一次推进 1 秒")
	var t60: int = int(host.call("drive_frame", 0.0))
	ok(t60 == 60, "喂 1 秒 ⇒ **恰好 60 个 tick**（实际 %d）" % t60)
	ok(int(host.call("last_pumps")) == 60, "60 个 tick ⇒ 60 次泵")
	ok(int(host.call("last_probes")) == 61, "探针 = 1 + 60 = 61（§48.4 的 112 = 52 + 60 同口径）")
	ok(int(host.call("tick_handler_tick")) == 60, "tick handler 走到 60")
	ok(int(host.call("time_to_tick_nanos")) == 16_666_626,
		"追完不跨边界：离**下一个** tick 还有 16 666 626 ns（1e9 - 999 999 960 = 40 ns 已过）")
	ok(int(host.call("probes_total")) == int(host.call("frames")) + int(host.call("ticks_total")),
		"累计探针 == 帧数 + due tick 数")

	# 4) **喂 2.5 个 tick ⇒ 转 2 圈、留 0.5**
	host = _host()
	var two_half: int = 2 * NANOS_PER_TICK + 8_333_333
	ok(bool(host.call("set_elapsed_nanos", two_half)) == true, "墙钟推进 2.5 个 tick（41 666 665 ns）")
	var t25: int = int(host.call("drive_frame", 0.0))
	ok(t25 == 2, "2.5 个 tick ⇒ 转 2 圈（实际 %d）" % t25)
	ok(int(host.call("time_to_tick_nanos")) == 8_333_333,
		"留下 0.5 个 tick（8 333 333 ns）")
	# 余数真的会累加：再喂 0.4 tick 仍不转；再喂 0.1 tick 正好到 3 个 tick 的边界 ⇒ 转 1 圈
	var t_04: int = int(host.call("drive_frame", 0.0))
	ok(t_04 == 0, "同一时刻再推一帧 ⇒ 0 个 tick（余数没丢，但也还没到期）")
	ok(bool(host.call("set_elapsed_nanos", two_half + 6_666_666)) == true, "再推进 0.4 个 tick")
	var t_29: int = int(host.call("drive_frame", 0.0))
	ok(t_29 == 0, "2.9 个 tick 的墙钟 ⇒ 仍然 0 个（余数 0.5 + 0.4 还没攒够）")
	ok(bool(host.call("set_elapsed_nanos", 3 * NANOS_PER_TICK)) == true, "正好推进到第 3 个 tick")
	var t3: int = int(host.call("drive_frame", 0.0))
	ok(t3 == 1, "累计到 3 个 tick ⇒ 补 1 圈（实际 %d）" % t3)

	# 5) **先 tick 后泵**：`TickEvent`（判别值 0）在同一次泵里就投递
	host = _host()
	ok(bool(host.call("subscribe", "tick_event")) == true, "订阅 `tick_event`（判别值 0）")
	ok(int(host.call("subscriber_count", "tick_event")) == 1, "订阅数读回 1")
	ok(bool(host.call("set_elapsed_nanos", 2 * NANOS_PER_TICK)) == true, "墙钟推进 2 个 tick")
	var t2: int = int(host.call("drive_frame", 0.0))
	var tags: PackedInt64Array = host.call("delivered_tags")
	ok(t2 == 2, "两个 tick（实际 %d）" % t2)
	ok(tags.size() == 2 and int(tags[0]) == 0 and int(tags[1]) == 0,
		"**本帧内**就投递了 2 条 TickEvent(0)（先 tick 后泵；顺序反了这里会是 0 条）%s" % [tags])
	ok(int(host.call("queue_len")) == 0, "泵把队列取空 ⇒ 没有留给下一帧的残留")
	var before: int = int(host.call("delivered_count"))
	var t_none: int = int(host.call("drive_frame", 0.0))
	ok(t_none == 0 and int(host.call("delivered_count")) == before,
		"不欠账的那一帧不会再投递（迟到的 tick_event 不存在）")

	# 6) **delta 不被使用**：同一个墙钟、0.0 与 1e6 的 delta ⇒ 同样圈数
	host = _host()
	ok(bool(host.call("set_elapsed_nanos", 5 * NANOS_PER_TICK)) == true, "墙钟推进 5 个 tick")
	var d0: int = int(host.call("drive_frame", 0.0))
	host = _host()
	ok(bool(host.call("set_elapsed_nanos", 5 * NANOS_PER_TICK)) == true, "重装后同样推进 5 个 tick")
	var d1: int = int(host.call("drive_frame", 1_000_000.0))
	ok(d0 == 5 and d1 == d0, "delta=0.0 与 delta=1e6 都是 %d 个 tick（原版不读 delta）" % [d0])

	# 7) 帧率不决定 tick 数：60 帧 × 半 tick = 30 tick
	host = _host()
	ok(bool(host.call("subscribe", "tick_event")) == true, "再订阅一次（重装后订阅表是空的）")
	var frames := 60
	for k in range(1, frames + 1):
		host.call("set_elapsed_nanos", k * 8_333_333)  # 1/120 s
		host.call("drive_frame", 1.0 / 120.0)
	ok(int(host.call("tick_handler_tick")) == 30,
		"60 帧 × 半 tick ⇒ 30 个 tick（不随帧率走）实际 %d" % int(host.call("tick_handler_tick")))
	ok(int(host.call("frames")) == 60 and int(host.call("ticks_total")) == 30,
		"帧数 60 / tick 数 30 / 泵数 %d" % int(host.call("pumps_total")))
	ok(int(host.call("probes_total")) == int(host.call("frames")) + int(host.call("ticks_total")),
		"累计探针 %d == 60 帧 + 30 tick" % int(host.call("probes_total")))

	# 8) 掉帧在下一帧补回来（不是丢掉），且 `V` 与 tick 读的是同一台 handler
	host = _host()
	ok(bool(host.call("set_elapsed_nanos", 3 * NANOS_PER_TICK)) == true, "墙钟一次跳 3 个 tick（掉帧）")
	var t_catch: int = int(host.call("drive_frame", 0.0))
	ok(t_catch == 3, "掉掉的 3 个 tick 在下一帧一次补回来（实际 %d）" % t_catch)
	ok(bool(host.call("set_elapsed_nanos", 60 * NANOS_PER_TICK)) == true, "墙钟推进到 60 个 tick")
	host.call("drive_frame", 0.0)
	var frac: float = float(host.call("fractional_tick"))
	ok(absf(frac - 60.0) < 1e-6,
		"`get_fractional_tick()`（EntityDrawer 的 V）== 60.0（同一台 tick handler，实际 %.6f）" % frac)

	# 9) **决策：生产路径不接驱动**（`docs/16-client.md` §58，任务 85）
	#
	# 80 号量到的节奏真值（`docs/06` §44）说明**原版**是"每帧一次 `drive_frame` + 泵跟着 tick"。
	# 本轮**决定不接**：理由与可执行触发条件在 §58.3。这一组把决策变成断言 ——
	#
	#   ① 生产客户端源码里**没有**驱动/通道符号（只有 `tools/*_check.gd` 用）；
	#   ② 没有订阅者时，泵照跑、队列被抽干、**投递 0 条** ⇒ 就算接上也没有收件人；
	#   ③ 接上之后那 5 个读数**会变成什么**（65 帧 / 1 秒，对齐 §44.7.3 的 +65/+60/+125）。
	#
	# ⚠️ ① 是**负结论**，所以先做正对照：本脚本自己就在调 `drive_frame`
	#    （扫描器不是"永远返回空"）。`AGENTS.md` 合并纪律 A.4。
	var play_src: String = FileAccess.get_file_as_string(PLAY_SCREEN_SRC)
	ok(play_src.length() > 20000, "读到播放屏源码（%d 字符）" % play_src.length())
	ok(not play_src.contains("drive_frame"), "播放屏里**没有** `drive_frame` 调用（决策：不接）")
	var hits: PackedStringArray = _scan_client_sources(PRODUCTION_FORBIDDEN)
	ok(_scan_files > 20, "扫描器扫到 `client/src/**` 的 %d 个 .gd（正对照：不是空目录）" % _scan_files)
	ok(hits.is_empty(), "生产客户端里没有驱动/通道符号（命中：%s）" % [hits])
	ok(FileAccess.get_file_as_string("res://tools/process_loop_check.gd").contains("drive_frame"),
		"正对照：**本脚本**在调 `drive_frame` ⇒ 上一条不是\"扫描器永远返回空\"")

	# 9.2 没有收件人时：泵照跑、队列被抽干、一条也投递不出去（读数③的实质）
	host = _host()
	var drawer: Object = ClassDB.instantiate("EntityDrawer")
	ok(drawer != null and drawer is Control,
		"借一个 `EntityDrawer` 造一条真事件（`kernel_push_position` → `set_position` → `PosInfo(26)`）")
	if drawer != null:
		ok(bool(drawer.call("kernel_push_position", 7, 5.0, 5.0, 2.0)) == true,
			"位置表写入一条 ⇒ 共享队列里压 1 条 `PosInfo(26)`")
		ok(int(host.call("queue_len")) == 1, "队列长度 1（还没泵）")
		ok(bool(host.call("set_elapsed_nanos", NANOS_PER_TICK)) == true, "墙钟推进 1 个 tick")
		var one: int = int(host.call("drive_frame", 0.0))
		ok(one == 1 and int(host.call("last_pumps")) == 1,
			"1 个 tick ⇒ 1 次泵（§44.3 的 `pumps ≡ ticks` 同口径）")
		ok(int(host.call("queue_len")) == 0 and int(host.call("delivered_count")) == 0,
			"**泵把队列抽干了，却一条也没投递**（没有订阅者 ⇒ 接线后 `tick_event` 仍无收件人）")
		drawer.free()
	# 9.3 正对照：有人订阅才收得到（同一台 host、同一条 tick 路径）
	host = _host()
	ok(bool(host.call("subscribe", "tick_event")) == true, "订阅 `tick_event`（正对照）")
	host.call("set_elapsed_nanos", 2 * NANOS_PER_TICK)
	var two: int = int(host.call("drive_frame", 0.0))
	ok(two == 2 and int(host.call("delivered_count")) == 2,
		"有订阅者 ⇒ 2 个 tick 投递 2 条（接线要有效果，**先得有收件人**）")

	# 9.4 接上之后那 5 个读数会变成什么：65 帧 / 1 秒（≈ §44.2 的 A 轮 64.90 fps）
	host = _host()
	var zero_tick := 0
	for k in range(1, 66):
		host.call("set_elapsed_nanos", k * 15_384_615)
		if int(host.call("drive_frame", 0.0)) == 0:
			zero_tick += 1
	var frames_n: int = int(host.call("frames"))
	var ticks_n: int = int(host.call("ticks_total"))
	ok(ticks_n == 59 or ticks_n == 60, "1 秒 65 帧 ⇒ **%d 个 tick**（tick 率仍 60/s，不是 65）" % ticks_n)
	ok(int(host.call("pumps_total")) == ticks_n, "泵数 == tick 数（%d）⇒ 不是每帧泵" % ticks_n)
	ok(int(host.call("probes_total")) == frames_n + ticks_n,
		"探针数 == 帧数 + tick 数（%d + %d = %d）⇒ §44.7.3 的 +65/+60/+125" % [frames_n, ticks_n,
			frames_n + ticks_n])
	ok(zero_tick > 0, "**0-tick 帧存在**（65 帧里 %d 帧一个 tick 都没有 ⇒ 这些帧泵 0 次）" % zero_tick)

	host = null
	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("PROCESS_LOOP %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
