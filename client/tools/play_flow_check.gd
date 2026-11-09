extends SceneTree
## **播放屏流程迁到 Rust** 的验收脚本（`dispatch/54`，`docs/16-client.md` §52）。
##
## ```text
## cargo build --offline --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## "$GODOT" --headless --path client --import
## "$GODOT" --headless --path client --script res://tools/play_flow_check.gd ; echo "EXIT=$?"
## ```
##
## # 这个脚本证明什么（以及不证明什么）
##
## 它证明的是**"流程决定权在 Rust"**这件事本身，不是"原版行为逐位复原"：
##
## | 组 | 断言什么 | 证据档位 |
## |---|---|---|
## | 0 | 两个流程类在**扩展**里（`ClassDB`），且 `client/src/game/level_rules.gd` / `turn_driver.gd` **已不存在** | **观测**（运行时） |
## | 1 | 胜负判定真相表（清靶/上限/全灭/判不了/同归于尽）由 `GwLevelRules.evaluate()` 给出 | 规则本体 **逐位**（`0x473520`/`0x4759f0`/`0x478133`–`0x478236`，见 `graphwar_play_screen_control::LevelRules`） |
## | 2 | 射手轮转（先自增再取 / 跳过死兵 / 全灭 -1 / 双玩家交替 / `setup_flat` 归组） | **逐位**（`0x463da0`：`0x463f40`–`0x463f58`、`0x463fc7`–`0x463fca`、`0x463fb6`、`0x464016`） |
## | 3 | 同时回合制的收集（公告顺序 / 重复 / 不在表里 / 攒齐 / 关轮） | **观测**（`docs/141` §4：130/130 批按公告顺序） |
## | 4 | 播放屏源码里**不再有**旧流程分支，且确实调的是 Rust 的两个入口 | **观测**（读文件文本，等价于 `grep`） |
## | 5 | `KernelBus::drive_frame` **接不接**：决策 = **不接**（§58.3 给了可执行触发条件）；本节仍验"内核那一侧能驱动"，另加 V 的实测（读数变了、画面没变） | **观测**（真引擎里驱动出来的 tick 数 + 真 `_draw` 的弧实参） |
## | 6 | **弹道生命周期**（`GwShotFlow`）：命中按点序号落地 / 飞完 / 只结算一次 / 整批结算 | 落地时序 **观测**（`0x472814` 死亡块取点序号；`docs/14` §7.3 106/106 按 `start_tick` 升序） |
## | 7 | **回合超时**（`GwTurnClock`）：六档时长 + 三个合取项 + 我方闩 | 谓词 **观测**（`0x470989`–`0x4709ec`）；闩是**我方**兜底 |
## | 8 | **AI 回合闸门**（`GwBotTurn`）：思考倒计时 + 一发直线 | **我方自定**（原版是 `aigen` 进化器，时序未观测） |
## | 9 | 播放屏源码里**不再有**这些流程分支，且确实调的是新入口 | **观测**（读文件文本，等价于 `grep`） |
## | 10 | **回放驱动**（`GwReplayDriver`）的类/方法表在扩展里，旧 `.gd` 局面机不再被播放屏使用 | **观测**（运行时） |
## | 11 | 加载 → 事件数 / tick 范围 / 结算批切分（3 批、边界 tick） | 批窗口形状 **观测**（`docs/136` §2 的 401 份语料口径） |
## | 12 | **按 tick 推进**：`pumps ≡ ticks`、0-tick 帧泵 0 次、60 fps 每帧 1 tick、长帧追赶无上限 | **观测**（`docs/06` §44.2/§44.3/§44.4；5 次运行 12 856 个 tick） |
## | 13 | **结算批在 tick 时钟上的落地**：只在 `ExecutingResults` 那一拍结算、长帧按序跨多批、`seek` 不报结算 | **观测**（同 §44.4 的追赶；批窗口来自 `replaydiff::split_batches`） |
## | 14 | 驱动吐出的事件序列与**本脚本独立扫描**的结果逐条相同 | 交叉验证（两份实现，两份语言） |
## | 15 | 真实回放（本机样本）上的同一批事实 + `canonical`/游标 = `fixtures/replay_player84.txt`（**旧局面机固化**下来的期望值） | 局面 **推断**（`docs/133` §3）+ 三端同格式（§5） |
## | 16 | 播放屏源码：旧回放驱动写法消失、新驱动入口出现 | **观测**（读文件文本） |
## | 17 | **旧局面机整份退役**：`GwReplayPlayer` 类不存在、源文件不存在、`client/src/**` 里 0 处**可执行**引用；4 个查表函数在 `GwReplayDriver` 上逐值复现旧表 | **观测**（运行时 ClassDB + 读文件文本 + 逐值调用；`dispatch/84`/`89`） |
##
## # 为什么没有"端到端打一枪"
##
## `--script` 模式下 **autoload 不加载**（实测 `root.get_node_or_null("/root/App") == null`），
## 播放屏的 `begin_match()` 离不开 `App.state`。端到端那条路在 `tools/selftest.gd`
## 里（`_test_level_rules` / `_test_play_flow`，714 条），那两支现在跑的**就是**这两个
## Rust 类 —— 所以"Rust 真的驱动了播放屏"由 selftest 覆盖，本脚本只钉接口与真相表。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下）。

const EXTENSION_PATH := "res://gw2rebuild.gdextension"
const RULES_CLASS := "GwLevelRules"
const DRIVER_CLASS := "GwTurnDriver"

## 播放屏仍然**必须**走的两个 Rust 入口（少一个就说明流程又回 GDScript 了）。
const FLOW_CALLS := ["_rules.evaluate(", "_turn_driver.next_shooter(", "_turn_driver.setup_flat("]
## 旧流程里已经被删掉的函数/写法（出现任何一个都算回流）。
const GONE_PATTERNS := [
	"func _all_alive_players_shot", "func _shot_owners",
	"GwLevelRules.for_campaign", "GwLevelRules.for_versus", "GwLevelRules.for_match",
	"SOLDIER_ID_NONE", "wiped.append",
]

## 67 号搬走的三块流程（`docs/16-client.md` §53）：出现任何一个都说明流程又回到 GDScript 了。
const FLOW_GONE_67 := [
	"func _apply_kills_up_to", "func _bot_aim_expression",
	"_pending_kills", "_pending_target_kills", "_logic_settled",
	"_kills_this_shot", "_target_kills_this_shot", "_bot_think_left",
	"BOT_AIM_FORMAT", "_turns_used",
]
## 搬完之后播放屏**必须**调的新入口（少一个就等于那块流程没搬）。
const FLOW_CALLS_67 := [
	"_flow.begin(", "_flow.advance(", "_flow.settle(", "_flow.settled(",
	"_flow.land_all(", "_turn_clock.decide(", "_bot.tick(", "_bot.aim_expression(",
]

const RULES_METHODS := [
	"from_campaign", "from_versus", "from_match", "kind_name", "turn_limit",
	"targets_planned", "level_index", "human_team", "has_targets", "has_opponent_rule",
	"evaluate", "snapshot", "target_team",
]
const FLOW_CLASS := "GwShotFlow"
const CLOCK_CLASS := "GwTurnClock"
const BOT_CLASS := "GwBotTurn"
const REPLAY_DRIVER_CLASS := "GwReplayDriver"
## 84 号固化的夹具：**旧 GDScript 局面机**（`GwReplayPlayer`，426 行）算出来的期望值。
## 旧代码退役之后，它就是对拍旧实现的**唯一**对照物（先固化、再删，`docs/16-client.md` §57）。
const REPLAY_FIXTURE := "fixtures/replay_player84.txt"
## 旧局面机的类名与文件（用来钉"它不许把局面/时钟带回来"；89 号起**文件已整份删除**）。
const REPLAY_OLD_PLAYER_CLASS := "GwReplayPlayer"
const REPLAY_OLD_PLAYER_PATH := "res://src/play/replay_player.gd"
## 89 号：`GwReplayPlayer` 那 4 个 `static func` 搬到了 `GwReplayDriver`（静态、纯查表）。
## 左边 = 方法名，中间 = 入参，右边 = **旧实现**给的整数（删文件前逐条抄下来的）。
## 判据不是"能跑通"，而是"删掉的那张表被一模一样地接上了" —— 14 个已知取值 + 4 个兜底。
const REPLAY_MODE_CASES := [
	["axis_index", "NoAxis", 0], ["axis_index", "OnlyMain", 1],
	["axis_index", "EveryFive", 2], ["axis_index", "EveryUnit", 3],
	["function_index", "NormalFunction", 0], ["function_index", "DiffEqFunction", 1],
	["turn_index", "SequentialTurns", 0], ["turn_index", "SimultaneousTurns", 1],
	["time_index", "Timer30s", 0], ["time_index", "Timer1m", 1],
	["time_index", "Timer2m", 2], ["time_index", "Timer3m", 3],
	["time_index", "Timer5m", 4], ["time_index", "TimerInf", 5],
	# 兜底：旧 `match` 的 `return` 与两个三元表达式的默认值（3 / 0 / 0 / 5）
	["axis_index", "???", 3], ["function_index", "???", 0],
	["turn_index", "???", 0], ["time_index", "???", 5],
]
## 旧局面机的方法（84 号删掉的那批）：再出现任何一个都说明它又长回来了。
const REPLAY_STATE_MACHINE_METHODS := [
	"setup", "advance_dt", "advance_to", "apply_event", "canonical", "seek",
	"step_event", "step_back_event", "actors", "obstacles_for_view", "target_positions",
	"current_tick", "progress", "finished", "mode_indices", "team_index",
]
## 84 号的第二条判据（旧源文件 ≤ 80 行、不含 `func apply_event` / `"obstacles": []` 等写法）
## 在 89 号被**更强的判据取代**：文件本身必须不存在、类必须在 `ClassDB` 里不存在。
## 之所以不保留"扫全 `client/src/**` 找局面写法"：那几个模式串在别处有**合法**命中
## （`core_bridge.gd` 的返回形状、`play_screen.gd` 的 `CURVE_TICKS_PER_SECOND`），
## 扫全仓会变成假红机器。
## 本类的方法表（少一个就说明驱动又退回 GDScript 了）。
const REPLAY_METHODS := [
	"load", "load_text", "ok", "error", "first_tick", "last_tick", "tick", "cursor",
	"event_count", "progress", "finished", "playing", "speed", "set_speed", "canonical",
	"summary", "stats", "phase", "snapshot", "actors", "obstacles", "target_positions",
	"last_shot", "team_index", "soldier_owner", "has_color", "color_of", "graphics",
	"functions", "function_of", "batches", "batch_count", "batches_settled", "batch_note",
	"advance_dt", "advance_ticks", "play", "pause", "toggle", "seek", "step_event",
	"step_back_event",
]
## 81 号搬走的回放驱动（`docs/16-client.md` §56）：这些写法**不许**再出现在播放屏里。
const REPLAY_GONE := [
	"GwReplayPlayer.new().setup(", "func _apply_replay_events(applied: Array[Dictionary])",
	"_replay.state.", "_replay.file.", "_replay.obstacles_for_view(",
	"_replay.mode_indices(", "_replay.current_tick(",
]
## 搬完之后播放屏**必须**调的新入口。
##
## 最后 4 条是 89 号加的：模式字符串 → 档位下标原来问 `GwReplayPlayer`（已删），
## 现在问驱动的**静态**查表函数（`docs/16-client.md` §59）。
const REPLAY_CALLS := [
	"GwReplayDriver.new()", "_replay.advance_dt(", "_replay.snapshot()",
	"_replay.last_shot(", "_replay.actors()", "_replay.obstacles()",
	"_replay.phase()", "dirty_actors", "dirty_layout", "dirty_shot",
	"GwReplayDriver.axis_index(", "GwReplayDriver.function_index(",
	"GwReplayDriver.turn_index(", "GwReplayDriver.time_index(",
]
## 本机真实回放样本的**文件名**（与 `selftest.gd` 的 `REPLAY_SAMPLE` 同一份，
## `docs/133` §5 的样本）。
##
## ⚠️ 不能写 `user://replays/…`：`user://` 是**本重建版**自己的目录
## （`app_userdata/Graphwar II — Client Rebuild`），原版存档在
## `app_userdata/Graphwar/replays/`（与 `selftest.gd::_real_replay_dir()` 同一条路）。
const REPLAY_REAL_NAME := "1793897298"

## 合成回放（**不依赖任何外部文件**，所以本组的条数在每台机器上都一样）。
## 形状照 401 份语料：`CalculatingResults` → `FunctionInfo`/`FunctionPoints`
## → `ExecutingResults` → 延迟段里的 `LifeInfo`。3 批 / 19 条事件 / tick 1..120。
const SYNTHETIC_REPLAY := """{\"event\":{\"GameNameInfo\":{\"game_name\":\"T\"}},\"tick\":1}
{\"event\":{\"SoldierInfo\":{\"soldier_id\":11,\"player_id\":1}},\"tick\":1}
{\"event\":{\"PosInfo\":{\"entity_id\":11,\"pos_x\":-8.0,\"pos_y\":0.0,\"radius\":0.5}},\"tick\":2}
{\"event\":{\"TeamInfo\":{\"entity_id\":11,\"team\":\"Team1\"}},\"tick\":2}
{\"event\":{\"PosInfo\":{\"entity_id\":12,\"pos_x\":8.0,\"pos_y\":0.0,\"radius\":0.5}},\"tick\":2}
{\"event\":{\"TeamInfo\":{\"entity_id\":12,\"team\":\"Team2\"}},\"tick\":2}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"CalculatingResults\"}},\"tick\":10}
{\"event\":{\"FunctionInfo\":{\"owner_id\":11,\"function\":\"x\",\"corrected_function\":null}},\"tick\":11}
{\"event\":{\"FunctionPoints\":{\"owner_id\":11,\"start_x\":-8.0,\"start_y\":0.0,\"end_x\":8.0}},\"tick\":12}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"ExecutingResults\"}},\"tick\":20}
{\"event\":{\"LifeInfo\":{\"entity_id\":12,\"alive\":false}},\"tick\":30}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"CalculatingResults\"}},\"tick\":60}
{\"event\":{\"FunctionPoints\":{\"owner_id\":12,\"start_x\":8.0,\"start_y\":0.0,\"end_x\":-8.0}},\"tick\":62}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"ExecutingResults\"}},\"tick\":70}
{\"event\":{\"LifeInfo\":{\"entity_id\":11,\"alive\":false}},\"tick\":80}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"CalculatingResults\"}},\"tick\":100}
{\"event\":{\"FunctionPoints\":{\"owner_id\":11,\"start_x\":-8.0,\"start_y\":0.0,\"end_x\":8.0}},\"tick\":102}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"ExecutingResults\"}},\"tick\":110}
{\"event\":{\"GameStateInfo\":{\"game_state\":\"EndingGame\"}},\"tick\":120}
"""
const FLOW_METHODS := [
	"reset", "begin", "advance", "flush", "land_all", "settle", "abort",
	"in_flight", "arrived", "revealed", "total_points", "pending_kills",
	"pending_targets", "kills_landed", "targets_landed", "settled", "turns_used",
	"snapshot",
]
const CLOCK_METHODS := [
	"set_time_mode", "restart", "advance", "decide", "seconds_left", "has_deadline",
	"duration_ticks", "elapsed_ticks", "auto_fired_total", "snapshot",
]
const BOT_METHODS := [
	"configure", "arm", "disarm", "armed", "think_left", "think_seconds", "tick",
	"aim_expression",
]
const DRIVER_METHODS := [
	"setup", "setup_flat", "first_shooter", "next_shooter", "current_shooter",
	"soldier_id_none", "snapshot", "round_open", "round_submit", "round_complete",
	"round_waiting", "round_reset",
]

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

func _ensure_extension() -> void:
	if ClassDB.class_exists(RULES_CLASS) and ClassDB.class_exists(DRIVER_CLASS):
		return
	print("      load_extension(%s) -> %d" % [EXTENSION_PATH,
		GDExtensionManager.load_extension(EXTENSION_PATH)])

func _methods_of(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

func _missing(obj: Object, wanted: Array) -> PackedStringArray:
	var names := _methods_of(obj)
	var out := PackedStringArray()
	for m in wanted:
		if not names.has(String(m)):
			out.append(String(m))
	return out

func _rules() -> Object:
	return ClassDB.instantiate(RULES_CLASS)

func _driver() -> Object:
	return ClassDB.instantiate(DRIVER_CLASS)

func _initialize() -> void:
	print("=== 播放屏流程迁到 Rust 验收（docs/16-client.md §52）")
	_ensure_extension()

	# ---------------- 0. 两个流程类在**扩展**里，旧 .gd 已经不在 ----------------
	ok(ClassDB.class_exists(RULES_CLASS),
		"扩展里有关卡规则类 %s（胜负判定的权威）" % RULES_CLASS)
	ok(ClassDB.class_exists(DRIVER_CLASS),
		"扩展里有回合轮转类 %s（射手轮转 + 同时回合收集的权威）" % DRIVER_CLASS)
	var rules: Object = _rules()
	var driver: Object = _driver()
	ok(rules != null and driver != null, "两个类都能 ClassDB.instantiate")
	if rules == null or driver == null:
		_finish()
		return
	ok(_missing(rules, RULES_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [RULES_CLASS, _missing(rules, RULES_METHODS)])
	ok(_missing(driver, DRIVER_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [DRIVER_CLASS, _missing(driver, DRIVER_METHODS)])
	# 旧的两份 GDScript 实现必须**真的**消失（否则就是两份逻辑并存，正是本任务要治的病）
	ok(not FileAccess.file_exists("res://src/game/level_rules.gd"),
		"旧的 `src/game/level_rules.gd`（219 行）已删除 —— 判定只剩 Rust 一份")
	ok(not FileAccess.file_exists("res://src/game/turn_driver.gd"),
		"旧的 `src/game/turn_driver.gd`（121 行）已删除 —— 轮转只剩 Rust 一份")

	# ---------------- 1. 胜负判定：真相表由 Rust 给 ----------------
	# `docs/118` §7.1 的关卡列：0 = 1 靶/无上限、6 = 2 靶/上限 6、7 = 1 靶/上限 1、5 = 有 Bot。
	var r0: Object = _rules()
	r0.call("from_campaign", {"index": 0, "targets": 1, "turn_limit": 0, "bot_tier": -1}, 0)
	var r6: Object = _rules()
	r6.call("from_campaign", {"index": 6, "targets": 2, "turn_limit": 6, "bot_tier": -1}, 0)
	var r7: Object = _rules()
	r7.call("from_campaign", {"index": 7, "targets": 1, "turn_limit": 1, "bot_tier": -1}, 0)
	var r5: Object = _rules()
	r5.call("from_campaign", {"index": 5, "targets": 0, "turn_limit": 0, "bot_tier": 0}, 0)
	ok(String(r0.call("kind_name")) == "targets" and int(r0.call("targets_planned")) == 1
		and int(r0.call("turn_limit")) == 0,
		"规则：index 0 = 打靶关 / 1 靶 / 无上限（实际 %s）" % str(r0.call("snapshot")))
	ok(String(r6.call("kind_name")) == "targets" and int(r6.call("turn_limit")) == 6,
		"规则：index 6 = 打靶关 / 上限 6")
	ok(String(r7.call("kind_name")) == "targets" and int(r7.call("turn_limit")) == 1,
		"规则：index 7 = 打靶关 / 上限 **1**（docs/118 §7.1 'first try'）")
	ok(String(r5.call("kind_name")) == "versus", "规则：index 5 = 对战关（有 Bot、无靶子）")
	var rboth: Object = _rules()
	rboth.call("from_match", true, true, 0, 0)
	ok(String(rboth.call("kind_name")) == "both",
		"规则：目标 + 对手 ⇒ both（21 关数据里不存在，docs/131 §2.2）")
	ok(int(r0.call("target_team")) == 1, "靶子属于 Team2（`0x46f310` 的队伍是入参；docs/150 §Q1）")

	var v: Dictionary = r0.call("evaluate",
		{"alive_by_team": {0: 1}, "teams_present": [0],
			"targets_total": 1, "targets_alive": 0, "turns_used": 1})
	ok(bool(v.over) and bool(v.human_won) and String(v.reason) == "targets_cleared",
		"判定：靶子全清 ⇒ 人类通关（`0x475a88`；实际 %s）" % str(v))
	ok(int(v.get("verdict_byte", -1)) == 0,
		"判定：胜方字节 = 0（Team1，`0x478133`–`0x478236` 的表；实际 %s）" % str(v.get("verdict_byte")))
	v = r0.call("evaluate", {"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 1, "targets_alive": 1, "turns_used": 3})
	ok(not bool(v.over) and String(v.reason) == "playing",
		"判定：靶子还在、没到上限 ⇒ 不结束（旧实现这里判人类胜）")
	v = r7.call("evaluate", {"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 1, "targets_alive": 1, "turns_used": 1})
	ok(bool(v.over) and not bool(v.human_won) and String(v.reason) == "turn_limit"
		and int(v.winner_team) == 1,
		"判定：上限用尽 ⇒ 判负、胜方 Team2（`0x475a64`；实际 %s）" % str(v))
	ok(int(v.get("verdict_byte", -1)) == 1, "判定：上限收场的胜方字节 = 1（`adc al,1` 那条）")
	v = r5.call("evaluate", {"alive_by_team": {0: 2, 1: 0}, "teams_present": [0, 1],
		"targets_total": 0, "targets_alive": 0, "turns_used": 3})
	ok(bool(v.over) and bool(v.human_won) and String(v.reason) == "enemy_wiped"
		and int(v.winner_team) == 0, "判定：对战关对方全灭 ⇒ 人类通关（实际 %s）" % str(v))
	v = r5.call("evaluate", {"alive_by_team": {0: 0, 1: 1}, "teams_present": [0, 1],
		"targets_total": 0, "targets_alive": 0, "turns_used": 3})
	ok(bool(v.over) and not bool(v.human_won) and String(v.reason) == "own_wiped",
		"判定：对战关我方全灭 ⇒ 人类失败（实际 %s）" % str(v))
	v = r5.call("evaluate", {"alive_by_team": {0: 0, 1: 0}, "teams_present": [],
		"targets_total": 0, "targets_alive": 0, "turns_used": 0})
	ok(not bool(v.over) and String(v.reason) == "no_verdict",
		"判定：双方都不在场 ⇒ no_verdict（不猜胜负；实际 %s）" % str(v))
	v = r5.call("evaluate", {"alive_by_team": {0: 0, 1: 0}, "teams_present": [0, 1],
		"targets_total": 0, "targets_alive": 0, "turns_used": 3})
	ok(bool(v.over) and String(v.reason) == "mutual_wipe"
		and int(v.get("verdict_byte", -1)) == 2,
		"判定：同一发清空两边 ⇒ 字节 2（没人赢）、人类判负（实际 %s）" % str(v))
	# 靶子的"队伍"真的是 Team2：只有靶子的关，靶子全灭就等于 Team2 全灭
	var r_t: Object = _rules()
	r_t.call("from_campaign", {"index": 8, "targets": 3, "turn_limit": 0, "bot_tier": -1}, 0)
	v = r_t.call("evaluate", {"alive_by_team": {0: 1}, "teams_present": [0],
		"targets_total": 3, "targets_alive": 0, "turns_used": 5})
	ok(bool(v.over) and bool(v.human_won),
		"判定：没有敌方士兵、3 个靶子全清 ⇒ 人类通关（靶子算 Team2 的成员）")

	# ---------------- 2. 回合轮转：规则由 Rust 给 ----------------
	driver.call("setup", [{"player": 0, "soldiers": [0, 1]}])
	var alive_all := [true, true]
	ok(int(driver.call("first_shooter", alive_all)) == 0, "轮转：第一个射手 = 0 号士兵")
	ok(int(driver.call("next_shooter", alive_all)) == 1,
		"轮转：先自增再取 ⇒ 换到 1 号士兵（`0x463f40`–`0x463f58`）")
	ok(int(driver.call("next_shooter", alive_all)) == 0, "轮转：到尾回绕到 0 号")
	ok(int(driver.call("current_shooter")) == 0, "轮转：current_shooter 与上一次结果一致")
	driver.call("setup", [{"player": 0, "soldiers": [0, 1]}])
	driver.call("first_shooter", alive_all)
	ok(int(driver.call("next_shooter", [true, false])) == 0,
		"轮转：跳过死士兵（`0x463fc7` 的 dec/jne，最多试 len 次）")
	driver.call("setup", [{"player": 0, "soldiers": [0, 1]}])
	driver.call("first_shooter", alive_all)
	ok(int(driver.call("next_shooter", [false, false])) == -1,
		"轮转：全灭 ⇒ -1（对应 `Some(SOLDIER_ID_NONE)`，界面侧统一用 -1）")
	ok(int(driver.call("soldier_id_none")) == 0, "轮转：哨兵 soldier_id = 0（`0x464016`）")
	driver.call("setup", [{"player": 0, "soldiers": [0]}, {"player": 1, "soldiers": [1]}])
	ok(int(driver.call("first_shooter", alive_all)) == 0, "轮转：两玩家 ⇒ 从 0 号玩家开始")
	ok(int(driver.call("next_shooter", alive_all)) == 1, "轮转：两玩家 ⇒ 交替到 1 号玩家")
	ok(int(driver.call("next_shooter", alive_all)) == 0, "轮转：两玩家 ⇒ 再交替回来")
	# 归组也交给 Rust（`setup_flat`：入参 = "每个士兵属于谁"）
	driver.call("setup_flat", [0, 0, 1])
	var snap: Dictionary = driver.call("snapshot")
	ok(int(snap.get("count", -1)) == 2,
		"归组：`setup_flat([0,0,1])` ⇒ 2 张表（实际 %s）" % str(snap.get("count")))
	ok((snap.get("players", []) as Array).size() == 2
		and int((snap.get("players", [])[0] as Dictionary).get("player", -1)) == 0
		and ((snap.get("players", [])[0] as Dictionary).get("soldiers", []) as Array).size() == 2,
		"归组：0 号玩家拥有 2 个士兵（实际 %s）" % str(snap.get("players")))
	ok(int(snap.get("cursor", -1)) == 0, "归组后游标回到 0")

	# ---------------- 3. 同时回合制的收集（`RoundCollection`） ----------------
	driver.call("round_open", [0, 1])
	ok(bool(driver.call("round_complete")) == false, "收集：刚开轮 = 没攒齐")
	ok((driver.call("round_waiting") as Array).size() == 2, "收集：还等 2 个玩家")
	var sub: Dictionary = driver.call("round_submit", 0)
	ok(bool(sub.get("accepted", false)) and not bool(sub.get("complete", true)),
		"收集：0 号交上 ⇒ 受理、还没齐（实际 %s）" % str(sub))
	sub = driver.call("round_submit", 0)
	ok(not bool(sub.get("accepted", true)) and String(sub.get("reason", "")) == "duplicate",
		"收集：同一个玩家再交 ⇒ duplicate（`docs/86`：一回合一人一发）")
	sub = driver.call("round_submit", 7)
	ok(not bool(sub.get("accepted", true)) and String(sub.get("reason", "")) == "not_expected",
		"收集：不在公告里的玩家 ⇒ not_expected")
	sub = driver.call("round_submit", 1)
	ok(bool(sub.get("complete", false)), "收集：1 号也交上 ⇒ 攒齐")
	ok(bool(driver.call("round_complete")), "收集：round_complete() == true")
	driver.call("round_reset")
	ok(not bool(driver.call("round_complete")), "收集：关轮后不再算齐")
	sub = driver.call("round_submit", 0)
	ok(String(sub.get("reason", "")) == "no_round", "收集：关轮后再交 ⇒ no_round")

	# ---------------- 4. 播放屏源码：旧流程分支真的没了，调的是 Rust ----------------
	var src := FileAccess.get_file_as_string("res://src/screens/play_screen.gd")
	ok(src.length() > 20000, "读到播放屏源码（%d 字符）" % src.length())
	for pat in GONE_PATTERNS:
		ok(not src.contains(pat), "源码里已无旧流程写法：`%s`" % pat)
	for pat in FLOW_CALLS:
		ok(src.contains(pat), "源码里确实调 Rust 入口：`%s`" % pat)
	ok(src.contains("func _check_match_over") and src.contains("_last_verdict"),
		"播放屏仍保留 `_check_match_over()` —— 但它只**应用** Rust 的结论（排版/结束标志）")

	# ---------------- 5. `drive_frame` 的接线**决策**（§58，任务 85）：**不接** ----------------
	# 下面四条仍然验"内核那一侧能驱动"（= 80 号 §44.7 的方案技术上可执行）；
	# **本轮的决策是不接**，判据与可执行触发条件在 `docs/16-client.md` §58.3。为什么不接，
	# 由 5.1（源码前提）与 5.2（实测：读数变了、画面没变）两组钉住。
	if ClassDB.class_exists("KernelProcessHost"):
		var host: Object = ClassDB.instantiate("KernelProcessHost")
		host.call("reset")
		var t0: int = int(host.call("last_ticks"))
		host.call("set_elapsed_nanos", 16_666_666)
		var t1: int = int(host.call("drive_frame", 0.0))
		ok(t0 == 0 and t1 == 1,
			"drive_frame：摆 1 个 tick 的墙钟 ⇒ 转 1 圈（%d → %d）" % [t0, t1])
		host.call("set_elapsed_nanos", 1_000_000_000)
		var t2: int = int(host.call("drive_frame", 0.0))
		ok(t2 == 59 or t2 == 60,
			"drive_frame：喂 1 秒 ⇒ 追上 60 tick（第二帧补 %d 圈，合计 60）" % t2)
		ok(int(host.call("fractional_tick")) == 60,
			"drive_frame：`V` = 60.0（与 `EntityDrawer` 同一台 handler，实际 %d）"
				% int(host.call("fractional_tick")))
		ok(int(host.call("last_probes")) == 1 + int(host.call("last_ticks")),
			"drive_frame：探针数 == 1 + tick 数（`0x518484` 在循环头）")
		host = null
	else:
		ok(false, "KernelProcessHost 不在扩展里（应随 --features gdext-bindings 注册）")

	# ---------------- 5.1 决策钉死：`V` 的来源 / 前提 / 触发条件（§58.2–§58.3） ----------------
	# 80 号的五条真值（`docs/06` §44）说的是**原版**：每帧一次 `drive_frame`、泵跟着 tick。
	# 我方**现在不接**，因为可见时序的真缺口不在"谁来驱动"，而在"谁来消费"：
	# 曲线的揭示仍按**帧 delta** 累加 `_curve_tick`（`_advance_curve()`），一帧都没读过 tick handler。
	# 下面几条把"现在不接"的**前提**钉住 —— 前提一变，断言就红，指回 §58.3 的落地顺序。
	ok(src.contains("set_fractional_tick\", _curve_tick"),
		"V（喂给 `GraphwarPlane`）的实参仍是 `_curve_tick`（帧 delta 累加）")
	ok(src.contains("SOLDIER_HIT_RADIUS, _curve_tick"),
		"V（喂给 `EntityDrawer.set_ring`）的实参仍是 `_curve_tick`")
	ok(src.contains("seconds * CURVE_TICKS_PER_SECOND"),
		"曲线仍按帧 delta 推（`_advance_curve(seconds)`），不是按 tick ⇒ 此刻接驱动就是两只钟并存")
	ok(not src.contains("drive_frame"), "播放屏里**没有** `drive_frame` 调用（这条就是决策本身）")
	var writers: PackedStringArray = _scan_src_symbols(
		["pos_set_position(", "kernel_push_position", "gate_attach"])
	ok(writers.is_empty(),
		"生产路径**没有**位置表/门的生产者（`client/src/**` 命中：%s）⇒ `EntityDrawer` 的 V 走代偿支、不读内核钟"
			% [writers])
	var t1_reached: bool = (not (src.contains("set_fractional_tick\", _curve_tick")
			and src.contains("SOLDIER_HIT_RADIUS, _curve_tick"))) or not writers.is_empty()
	ok(not t1_reached,
		"**触发条件 T1 未到达**：V 仍由 `_curve_tick` 供给 ⇒ 决策「不接」仍成立（红了先读 §58.3）")

	# ---------------- 5.2 实测：内核钟推 60 格，画面**一位不变**；一旦 V 由它供给就**饱和** ----------------
	# 探针 = 真 `EntityDrawer._draw` 发出去的弧（`last_arc()` 的 12 个实参；[3]/[4] = 起止角）。
	#   生产口径（位置表空 + 门没 attach）⇒ V 取 `prep` 里的 `_curve_tick`，内核钟读数**没人读**；
	#   正对照（填表 + attach 门）⇒ 同一条弧立刻改由内核钟决定，而且**饱和**
	#   （`H = min(π/6 + (V − base)/60, 1.0733775)`，V = 60、base 仍是 `_curve_tick` 的 1.0）
	#   ⇒ "只把 V 接到内核钟"会让准备圈**永远全开**：V 与 base 必须同一支钟，两处一起改。
	if ClassDB.class_exists("KernelProcessHost"):
		var host2: Object = ClassDB.instantiate("KernelProcessHost")
		host2.call("reset")
		var ed: Control = ClassDB.instantiate("EntityDrawer")
		ok(ed != null, "`EntityDrawer` 可用于摆一具准备态（V 的消费者）")
		if ed != null:
			ed.set_size(Vector2(1000.0, 600.0))
			root.add_child(ed)
			await process_frame
			# = `play_screen.gd` 那两处实参（这里用 1.0 代替 `_curve_tick`）
			ed.call("set_ring", 7, 5.0, 5.0, 2.0, 1.0)
			ed.call("set_ring_base", 7, 1.0)
			await process_frame
			await process_frame
			var a0: PackedFloat64Array = ed.call("last_arc", 0)
			ok(a0.size() == 12 and absf((a0[4] - a0[3]) - 2.0 * PI / 6.0) < 1e-6,
				"生产口径：H = π/6 ⇒ 张开 %.9f rad（V = base = 1.0）" % (a0[4] - a0[3]))
			ok(String(ed.call("kernel_clock_source")) == "tick_handler",
				"内核钟读数来自真 tick handler（不是验收用的 override）")
			host2.call("set_elapsed_nanos", 60 * 16_666_666)
			var n60: int = int(host2.call("drive_frame", 0.0))
			await process_frame
			await process_frame
			var a1: PackedFloat64Array = ed.call("last_arc", 0)
			ok(n60 == 60 and absf(float(host2.call("fractional_tick")) - 60.0) < 1e-6,
				"驱动到 60 个 tick（内核钟读数确实变了：%.1f）" % float(host2.call("fractional_tick")))
			ok(a0 == a1,
				"**画出来的弧逐位没变** ⇒ V 不读内核钟（读数变了、画面没变 = §58.2 的结论）")
			# 正对照：把表填上 + attach 门 ⇒ 同一段弧改由内核钟决定
			ok(bool(ed.call("gate_attach")) == true, "正对照：attach Life/Turn 两份 manager")
			ed.call("gate_set_alive", 7, true)
			ed.call("gate_set_turn", 9001, PackedInt64Array([7]))
			ed.call("pos_set_position", 7, 5.0, 5.0, 2.0)
			await process_frame
			await process_frame
			var a2: PackedFloat64Array = ed.call("last_arc", 0)
			var spread2: float = a2[4] - a2[3]
			ok(String(ed.call("gate_source")) == "managers" and spread2 > (a0[4] - a0[3]) + 1.0,
				"正对照：V 改由内核钟（60）供给 ⇒ 张开 %.9f rad（探针有判别力）" % spread2)
			ok(absf(spread2 - 2.0 * 1.0733774899765125) < 1e-5,
				"H **饱和**在 1.0733775 rad（base 仍是 `_curve_tick` 的 1.0）⇒ V 与 base 必须同一支钟")
			ed.queue_free()

	# ---------------- 6. 弹道生命周期（`GwShotFlow` → `ShotLifecycle`） ----------------
	# 这一组钉的是"哪条命中什么时候落地 / 算不算飞完 / 结算几次"——
	# 也就是原来写在 `_apply_kills_up_to` / `_on_shot_arrived` / `_finish_round` 里的判断。
	ok(ClassDB.class_exists(FLOW_CLASS),
		"扩展里有弹道生命周期类 %s（落地的权威）" % FLOW_CLASS)
	ok(ClassDB.class_exists(CLOCK_CLASS), "扩展里有回合时钟类 %s（超时谓词的权威）" % CLOCK_CLASS)
	ok(ClassDB.class_exists(BOT_CLASS), "扩展里有 AI 闸门类 %s（**我方自定**）" % BOT_CLASS)
	var flow: Object = ClassDB.instantiate(FLOW_CLASS) if ClassDB.class_exists(FLOW_CLASS) else null
	if flow == null:
		_finish()
		return
	ok(_missing(flow, FLOW_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [FLOW_CLASS, _missing(flow, FLOW_METHODS)])

	# 6.1 命中**按弹道点序号**落地：没走到那个点，一条都不许落。
	#     指令级证据：`0x472814` 的死亡块 `mov 0x10(%rax),%r13` / `shl $0x4` /
	#     `movdqu -0x10(%rdx,%rcx,1)`（按 16 字节/点取 `points[idx-1]`）。
	flow.call("begin", 100, [
		{"kind": "soldier", "index": 3, "point": 50, "pos": Vector2(1.0, 2.0)},
		{"kind": "soldier", "index": 7, "point": 10, "pos": Vector2(3.0, 4.0)},
	], [], Vector2(24.0, 0.5), false)
	ok(bool(flow.call("in_flight")), "生命周期：开火后进入 in_flight")
	ok(int(flow.call("total_points")) == 100, "生命周期：总点数 = 递进来的 100")
	ok(int(flow.call("pending_kills")) == 2, "生命周期：两条命中都还挂着（%d）"
		% int(flow.call("pending_kills")))
	ok((flow.call("advance", 9) as Array).is_empty(),
		"生命周期：揭示到第 9 点 ⇒ 一条都不落（10 号点还没到）")
	var ev: Array = flow.call("advance", 10)
	ok(ev.size() == 1 and String((ev[0] as Dictionary).get("kind", "")) == "soldier_killed"
		and int((ev[0] as Dictionary).get("index", -1)) == 7,
		"生命周期：揭示到第 10 点 ⇒ **正好**落 7 号士兵（边界是 `<=`；实际 %s）" % str(ev))
	ok(int(flow.call("kills_landed")) == 1, "生命周期：落地计数 = 1")
	ok(int(flow.call("pending_kills")) == 1, "生命周期：还剩 1 条没落")
	ev = flow.call("advance", 49)
	ok(ev.is_empty(), "生命周期：第 49 点还差一点（不许提前落）")
	ev = flow.call("advance", 50)
	ok(ev.size() == 1 and int((ev[0] as Dictionary).get("index", -1)) == 3,
		"生命周期：第 50 点落 3 号")
	ok(int(flow.call("kills_landed")) == 2 and int(flow.call("pending_kills")) == 0,
		"生命周期：两条都落地、队列空了")

	# 6.2 飞完 = 揭示点数 >= 总点数（旧 `.gd` 的 `_curve_finished`），到达事件只吐一次。
	ok(not bool(flow.call("arrived")), "生命周期：揭示 50/100 ⇒ 还没飞完")
	ev = flow.call("advance", 99)
	ok(ev.is_empty() and bool(flow.call("in_flight")), "生命周期：99/100 ⇒ 仍在飞")
	ev = flow.call("advance", 100)
	ok(ev.size() == 1 and String((ev[0] as Dictionary).get("kind", "")) == "arrived",
		"生命周期：100/100 ⇒ 到达事件（实际 %s）" % str(ev))
	ok(not bool(flow.call("in_flight")), "生命周期：到达后离开 in_flight")
	ok((flow.call("advance", 100) as Array).is_empty(), "生命周期：到达事件只吐一次（幂等）")

	# 6.3 结算**每发只收一次**（旧 `.gd` 的 `_logic_settled`）。
	var s1: Dictionary = flow.call("settle")
	ok(bool(s1.get("settled", false)) and int(s1.get("turns_used", -1)) == 1,
		"生命周期：第一次结算 ⇒ settled / 回合 1（实际 %s）" % str(s1))
	var s2: Dictionary = flow.call("settle")
	ok(not bool(s2.get("settled", true)) and int(s2.get("turns_used", -1)) == 1,
		"生命周期：同一发再结算 ⇒ 不算第二个回合（实际 %s）" % str(s2))

	# 6.4 靶子走另一条计数；`land_all` 一次全落地且**不吐到达**（压平模式/整批结算）。
	flow.call("begin", 40, [], [
		{"kind": "target", "target_id": 1000, "point": 4, "pos": Vector2(12.0, -5.1)},
	], Vector2(1.0, 1.0), false)
	ok(int(flow.call("pending_targets")) == 1, "生命周期：靶子命中单独计数")
	var landed_all: Array = flow.call("land_all")
	ok(landed_all.size() == 1
		and String((landed_all[0] as Dictionary).get("kind", "")) == "target_killed"
		and int((landed_all[0] as Dictionary).get("target_id", -1)) == 1000,
		"生命周期：`land_all()` 把靶子一次落地（实际 %s）" % str(landed_all))
	ok(int(flow.call("targets_landed")) == 1, "生命周期：靶子落地计数 = 1")
	ok(not _has_kind(flow.call("land_all"), "arrived"),
		"生命周期：`land_all()` **不吐**到达事件（压平模式还要放动画）")
	ev = flow.call("advance", 40)
	ok(_has_kind(ev, "arrived"), "生命周期：`land_all()` 之后飞完仍会吐到达（%s）" % str(ev))

	# 6.5 `already_settled = true`（压平模式/联机）：到达时不再结算。
	flow.call("reset")
	ok(int(flow.call("turns_used")) == 0, "生命周期：`reset()` 把回合数一起归零")
	flow.call("begin", 10, [], [], Vector2(0.0, 0.0), true)
	ok(not bool(flow.call("settle").get("settled", true)),
		"生命周期：`already_settled = true` ⇒ 到达时不再结算（压平/联机）")
	flow.call("abort")
	ok(not bool(flow.call("in_flight")), "生命周期：`abort()` 清掉在飞状态")

	# ---------------- 7. 回合时钟（`GwTurnClock` → `0x470989` 的三个合取项） ----------------
	var clock: Object = ClassDB.instantiate(CLOCK_CLASS)
	ok(clock != null and _missing(clock, CLOCK_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [CLOCK_CLASS, _missing(clock, CLOCK_METHODS)])
	# 六档时长（`docs/14` §3.3.2 全档实测；`TimerInf` 没有截止时刻）
	var want := [[0, 1800], [1, 3600], [2, 7200], [3, 10800], [4, 18000], [5, -1]]
	var modes_ok := true
	for row in want:
		clock.call("set_time_mode", int(row[0]))
		if int(clock.call("duration_ticks")) != int(row[1]):
			modes_ok = false
	ok(modes_ok, "时钟：六档时长 = 30s/1m/2m/3m/5m/∞（`TimeMode::seconds() × 60`）")
	clock.call("set_time_mode", 5)
	ok(not bool(clock.call("has_deadline")) and float(clock.call("seconds_left")) < 0.0,
		"时钟：`TimerInf` 没有截止时刻、没有倒计时（原版标签是空白）")
	# Timer1m：59.9 s 不超时、60.0 s 超时；同一回合只给一次（我方闩）
	clock.call("set_time_mode", 1)
	clock.call("restart")
	clock.call("advance", 59.9)
	ok(int(clock.call("elapsed_ticks")) == 3594, "时钟：59.9 s = 3594 tick（60 Hz）")
	ok(String(clock.call("decide", false, true)) == "nothing", "时钟：没到点 ⇒ 不自动开火")
	clock.call("advance", 0.1)
	ok(int(clock.call("elapsed_ticks")) == 3600, "时钟：60.0 s = 3600 tick")
	ok(String(clock.call("decide", false, true)) == "auto_fire",
		"时钟：到点 + 在等开火 + 局在跑 ⇒ auto_fire（`0x470989`）")
	ok(String(clock.call("decide", false, true)) == "nothing",
		"时钟：同一回合不再重复给（**我方**闩：原版靠改状态自锁）")
	ok(int(clock.call("auto_fired_total")) == 1, "时钟：累计自动开火 = 1")
	# 三个合取项：状态 / 截止 / 有人在等
	clock.call("restart")
	clock.call("advance", 60.0)
	ok(String(clock.call("decide", true, true)) == "nothing",
		"时钟：弹道在飞（≈ ExecutingResults）⇒ 不超时")
	ok(String(clock.call("decide", false, false)) == "nothing",
		"时钟：局已结束（≈ EndingGame）⇒ 不超时")
	ok(String(clock.call("decide", false, true)) == "auto_fire",
		"时钟：回到等开火才判（三个合取项缺一不可）")
	# 倒计时是"总时长 − 已过"，夹到 0
	clock.call("set_time_mode", 2)
	clock.call("restart")
	ok(absf(float(clock.call("seconds_left")) - 120.0) < 0.001, "时钟：2 分钟档开局 120 s")
	clock.call("advance", 1.5)
	ok(absf(float(clock.call("seconds_left")) - 118.5) < 0.001, "时钟：1.5 s 后 118.5 s")
	clock.call("advance", 200.0)
	ok(absf(float(clock.call("seconds_left"))) < 0.001, "时钟：走完夹到 0（不出现负数）")

	# ---------------- 8. AI 回合闸门（`GwBotTurn`，**我方自定**） ----------------
	var bot: Object = ClassDB.instantiate(BOT_CLASS)
	ok(bot != null and _missing(bot, BOT_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [BOT_CLASS, _missing(bot, BOT_METHODS)])
	bot.call("configure", 0.6)
	ok(not bool(bot.call("armed")) and float(bot.call("think_left")) < 0.0,
		"AI 闸门：没轮到 AI ⇒ 倒计时不跑")
	bot.call("arm")
	ok(bool(bot.call("armed")), "AI 闸门：轮到 AI ⇒ 起表（%.2f s，**自定**值）"
		% float(bot.call("think_left")))
	ok(not bool(bot.call("tick", 0.3, true)), "AI 闸门：0.3 s 还没到")
	ok(not bool(bot.call("tick", 0.4, false)), "AI 闸门：`can_fire = false` 时表不走")
	ok(bool(bot.call("tick", 0.3, true)), "AI 闸门：到点且能开火 ⇒ 放行一次")
	ok(not bool(bot.call("armed")), "AI 闸门：放行后表停掉（不重复开火）")
	# 一发**自定**的直线：m = (y敌 − y我)/(x敌 − x我)，6 位小数
	# 坐标用 **f32 可精确表示**的整数（`Vector2` 在 Godot 里是 f32；用 −5.1 会与
	# 字典里的 f64 字面量差 1e-7，让 `dy/dx` 变成一个非零的极小值）。
	ok(String(bot.call("aim_expression", Vector2(-18.0, -5.0), 0,
		[{"x": 18.0, "y": -5.0, "team": 1, "alive": true}])) == "0.000000*x",
		"AI 瞄准：同一水平线 ⇒ f(x) = 0.000000*x")
	ok(String(bot.call("aim_expression", Vector2(0.0, 0.0), 0,
		[{"x": 2.0, "y": 4.0, "team": 1, "alive": true}])) == "2.000000*x",
		"AI 瞄准：斜率 = 4/2 = 2（6 位小数，与旧 `.gd` 的 `%.6f*x` 同形）")
	ok(String(bot.call("aim_expression", Vector2(0.0, 0.0), 0,
		[{"x": 2.0, "y": 4.0, "team": 1, "alive": false}])) == "0",
		"AI 瞄准：死人不算目标 ⇒ 兜底 \"0\"")
	ok(String(bot.call("aim_expression", Vector2(0.0, 0.0), 0,
		[{"x": 2.0, "y": 4.0, "team": 0, "alive": true}])) == "0",
		"AI 瞄准：同队不算目标")

	# ---------------- 9. 播放屏源码：这三块流程真的搬走了 ----------------
	for pat in FLOW_GONE_67:
		ok(not src.contains(pat), "源码里已无旧流程写法：`%s`" % pat)
	for pat in FLOW_CALLS_67:
		ok(src.contains(pat), "源码里确实调新 Rust 入口：`%s`" % pat)
	ok(src.contains("func _apply_shot_events"),
		"播放屏只剩**事件执行器** `_apply_shot_events()`（按 kind 摆节点）")

	# ---------------- 10. 回放驱动（`GwReplayDriver`，`docs/16-client.md` §56） ----------------
	# 先钉接口：类在扩展里、方法表齐、旧 .gd 局面机不再被播放屏使用。
	ok(ClassDB.class_exists(REPLAY_DRIVER_CLASS),
		"扩展里有回放驱动类 %s（回放推进的权威）" % REPLAY_DRIVER_CLASS)
	var rd: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	ok(rd != null, "`%s` 能 ClassDB.instantiate" % REPLAY_DRIVER_CLASS)
	if rd == null:
		_finish()
		return
	ok(_missing(rd, REPLAY_METHODS).is_empty(),
		"`%s` 方法表齐（缺：%s）" % [REPLAY_DRIVER_CLASS, _missing(rd, REPLAY_METHODS)])

	# ---------------- 11. 加载 + 结算批切分（合成回放，19 条 / 3 批） ----------------
	ok(bool(rd.call("load_text", SYNTHETIC_REPLAY)) and bool(rd.call("ok")),
		"合成回放能加载（error='%s'）" % String(rd.call("error")))
	ok(int(rd.call("event_count")) == 19 and int(rd.call("first_tick")) == 1
		and int(rd.call("last_tick")) == 120,
		"加载后：19 条事件 / tick 1..120（实际 %d / %d..%d）"
		% [int(rd.call("event_count")), int(rd.call("first_tick")), int(rd.call("last_tick"))])
	# 批切分：与**本脚本自己扫回放文本**算出来的边界比（两份实现，两份语言）。
	var want_batches := _expected_batches(SYNTHETIC_REPLAY)
	var got_batches: Array = rd.call("batches")
	ok(int(rd.call("batch_count")) == 3 and String(rd.call("batch_note")) == "",
		"切出 3 个结算批、没有降级原因（实际 %d / '%s'）"
		% [int(rd.call("batch_count")), String(rd.call("batch_note"))])
	ok(_batch_bounds(got_batches) == _batch_bounds(want_batches),
		"批边界与独立扫描一致（驱动 %s ／ 扫描 %s）"
		% [str(_batch_bounds(got_batches)), str(_batch_bounds(want_batches))])
	var shots_ok := true
	for i in got_batches.size():
		if int((got_batches[i] as Dictionary).get("shots", -1)) != 1:
			shots_ok = false
	ok(shots_ok, "每批 1 发（`FunctionPoints` 计数，%s）" % str([1, 1, 1]))

	# ---------------- 12. **按 tick** 推进（`docs/06` §44 的五条真值） ----------------
	var d1: Object = _driver_fresh()
	# 120 帧 × 1/120 s = 60 个 tick；一半的帧一个 tick 都不该推（§44.3 的 0-tick 帧）。
	var bad_pumps := 0
	var ticks_sum := 0
	for i in 120:
		var rep: Dictionary = d1.call("advance_dt", 1.0 / 120.0)
		if int(rep.get("ticks", -1)) != int(rep.get("pumps", -2)):
			bad_pumps += 1
		ticks_sum += int(rep.get("ticks", 0))
	ok(bad_pumps == 0 and ticks_sum == 60,
		"`pumps ≡ ticks` 逐帧相等（§44.3；不符 %d 帧），120 帧 × 1/120 s = 60 tick（实际 %d）"
		% [bad_pumps, ticks_sum])
	var st1: Dictionary = d1.call("stats")
	ok(int(st1.get("frames", 0)) == 120 and int(st1.get("ticks", 0)) == 60
		and int(st1.get("pumps", 0)) == 60,
		"累计：帧 120 / tick 60 / 泵 60（实际 %s）" % str(st1))
	ok(int(st1.get("zero_tick_frames", 0)) == 60,
		"0-tick 帧 60 个、每个泵 0 次（§44.3：默认帧率下这种帧占 10.9%%；实际 %d）"
		% int(st1.get("zero_tick_frames", 0)))
	# 60 fps ⇒ 每帧正好 1 tick（比 §44.2 的 0.924 tick/帧 更严的定步自检）
	var d2: Object = _driver_fresh()
	var one_each := true
	for i in 60:
		if int((d2.call("advance_dt", 1.0 / 60.0) as Dictionary).get("ticks", 0)) != 1:
			one_each = false
	ok(one_each and int(d2.call("tick")) == 61,
		"60 fps 每帧 1 tick、60 帧后 tick 61（实际 %d）" % int(d2.call("tick")))
	# 长帧追赶**无上限**（§44.4：1.5 s 冻结 ⇒ 单帧 91 圈；旧 .gd 夹到 1 s = 60 tick）
	var d3: Object = _driver_fresh()
	var big: Dictionary = d3.call("advance_dt", 1.5)
	ok(int(big.get("ticks", 0)) >= 90 and int(big.get("pumps", 0)) == int(big.get("ticks", 0)),
		"长帧 1.5 s ⇒ 单帧 %d tick（§44.4 实测 91；旧实现夹到 60）" % int(big.get("ticks", 0)))
	ok(int(big.get("ticks", 0)) > 60, "长帧不夹到 1 s（这一条旧 `.gd` 必挂）")
	# 暂停门（原版 `0x515dd1` 的"在对局里"；回放屏用"用户按了播放"当同一件事）
	# 注意：这里**不**调 `play()`（`_driver_fresh()` 会 play，所以手工建一个）
	var d4: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	d4.call("load_text", SYNTHETIC_REPLAY)
	var paused: Dictionary = d4.call("advance_ticks", 10)
	ok(int(paused.get("ticks", -1)) == 0 and int(paused.get("pumps", -1)) == 0,
		"没在播放 ⇒ 推 10 个 tick 也是 0 tick / 0 泵（门在驱动里）")
	# 确定性：同一段 dt 两次独立驱动给出同一批 tick
	var d5: Object = _driver_fresh()
	var d6: Object = _driver_fresh()
	var a5: int = int((d5.call("advance_dt", 0.2) as Dictionary).get("ticks", -1))
	var a6: int = int((d6.call("advance_dt", 0.2) as Dictionary).get("ticks", -1))
	ok(a5 == a6 and a5 == 12, "200 ms = 12 tick、两次驱动完全相同（实际 %d / %d）" % [a5, a6])
	# tick 单调（驱动过程中不许回退）
	var d7: Object = _driver_fresh()
	var mono := true
	var last := int(d7.call("tick"))
	for i in 30:
		d7.call("advance_dt", 0.07)
		if int(d7.call("tick")) < last:
			mono = false
		last = int(d7.call("tick"))
	ok(mono, "推进过程中 tick 单调不减（到 tick %d）" % last)

	# ---------------- 13. 结算批在 tick 时钟上落地 ----------------
	var d8: Object = _driver_fresh()
	ok((d8.call("advance_ticks", 18) as Dictionary).get("settled", [1]).is_empty(),
		"tick 19（批 0 的 ExecutingResults 在 20）⇒ 还没结算")
	var r8: Dictionary = d8.call("advance_ticks", 1)
	ok(str(r8.get("settled", [])) == "[0]",
		"tick 20 = ExecutingResults ⇒ 第 0 批结算（实际 %s）" % str(r8.get("settled", [])))
	ok(int(d8.call("batches_settled")) == 1 and String(d8.call("phase")) == "executing",
		"结算计数 1、相位 executing（实际 %d / %s）"
		% [int(d8.call("batches_settled")), String(d8.call("phase"))])
	var d9: Object = _driver_fresh()
	var r9: Dictionary = d9.call("advance_ticks", 109)
	ok(str(r9.get("settled", [])) == "[0, 1, 2]",
		"长帧跨三批 ⇒ 按序全记、**不跳批**（实际 %s）" % str(r9.get("settled", [])))
	var d10: Object = _driver_fresh()
	d10.call("seek", 105)
	ok(int(d10.call("batches_settled")) == 2
		and (d10.call("advance_ticks", 0) as Dictionary).get("settled", [1]).is_empty(),
		"跳转不算结算（跳到 105 ⇒ 已结算 2 批，但不再报结算）")
	var d11: Object = _driver_fresh()
	var r11: Dictionary = d11.call("advance_ticks", 29)   # 1 → 30（LifeInfo 那一拍）
	ok(_has_kind_arr(r11.get("kinds", []), "LifeInfo"),
		"延迟段里的死亡事件（tick 30 的 `LifeInfo`）由驱动吐出来（批 0 已在 20 结算）")
	# 延迟段的右端点：下一批开始 tick 严格晚于执行拍（除末尾批允许相等）
	var b: Array = rd.call("batches")
	ok(int(b[0].next_start_tick) == 60 and int(b[0].exec_tick) == 20,
		"批 0：执行拍 20、延迟段到 60（下一批开始）")

	# ---------------- 14. 消息序列：驱动 vs 本脚本独立扫描 ----------------
	var d12: Object = _driver_fresh()
	var rep12: Dictionary = d12.call("advance_ticks", 19)   # → tick 20
	# 装载时已经消费了 tick ≤ 1 的两条（`GameNameInfo`/`SoldierInfo`）⇒ 从 1 之后算起
	var want_kinds := _kinds_upto(SYNTHETIC_REPLAY, 1, 20)
	ok(str(rep12.get("kinds", [])) == str(want_kinds),
		"驱动到 tick 20 的事件名序列 = 独立扫描（%d 条：%s）"
		% [want_kinds.size(), str(want_kinds)])
	ok(int(d12.call("cursor")) == want_kinds.size() + 2 and want_kinds.size() == 8,
		"游标 = 8 条 + 装载时消费的 2 拍 = 10（实际 %d / 扫描 %d 条）"
		% [int(d12.call("cursor")), want_kinds.size()])

	# ---------------- 15. 真实回放（有样本才做；`docs/133` §5 的同一份） ----------------
	var real_path := _real_replay_path()
	if real_path != "":
		var real: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
		ok(bool(real.call("load", real_path))
			and int(real.call("event_count")) == 104,
			"真实回放能加载：104 条事件（实际 %d）" % int(real.call("event_count")))
		# 与**旧 GDScript 局面机**对拍的替代品：`fixtures/replay_player84.txt`。
		# 84 号把旧实现（`GwReplayPlayer`，426 行，已退役）算出来的局面**先固化**成这份夹具，
		# 再让 selftest 改打驱动 ⇒ 旧代码删了，对照物还在（`docs/16-client.md` §57）。
		var f := GwReplayFile.load_path(real_path)
		if f.ok():
			var fx := _replay_fixture()
			var same := fx.size() >= 20
			var bad_tick := -1
			for t in [550, 2168, 2169, 2276]:
				real.call("seek", int(t))
				if String(real.call("canonical")) != String(fx.get("canonical_tick_%d" % t, "<缺键>")) \
					or int(real.call("cursor")) != int(String(fx.get("cursor_tick_%d" % t, "-1"))):
					same = false
					bad_tick = int(t)
			ok(same, "回放驱动在 tick 550/2168/2169/2276 上的 canonical + 游标 = 旧局面机固化的夹具（%s，%d 键）"
				% [REPLAY_FIXTURE, fx.size()])
			if not same:
				print("      首个不符的 tick：%d（实际 %s）" % [bad_tick, String(real.call("canonical"))])
			var rb: Array = real.call("batches")
			var wb := _expected_batches_raw(f)
			ok(_batch_bounds(rb) == _batch_bounds(wb) and rb.size() > 0,
				"真实回放的批边界与独立扫描一致（%d 批）" % rb.size())
			# 真实回放上重申 pumps ≡ ticks（合成回放已经查过，这里换成真样本）
			real.call("seek", int(real.call("first_tick")))
			real.call("play")
			var rp_ok := true
			for i in 20:
				var r: Dictionary = real.call("advance_dt", 1.0 / 100.0)
				if int(r.get("ticks", -1)) != int(r.get("pumps", -2)):
					rp_ok = false
			ok(rp_ok, "真实回放上 `pumps ≡ ticks` 仍逐帧相等（100 fps 帧率，多数帧 0 tick）")
	else:
		print("      SKIP 真实回放（%s 不在本机）—— 第 15 组 4 条不计入"
			% REPLAY_REAL_NAME)

	# ---------------- 16. 播放屏源码：回放驱动真的搬走了 ----------------
	ok(not src.contains("GwReplayPlayer.new()"),
		"播放屏不再自己建 GDScript 局面机（`GwReplayPlayer.new()` 已无）")
	for pat in REPLAY_GONE:
		ok(not src.contains(pat), "源码里已无旧回放驱动写法：`%s`" % pat)
	for pat in REPLAY_CALLS:
		ok(src.contains(pat), "源码里确实调新驱动入口：`%s`" % pat)

	# ---------------- 17. 旧局面机**整份**退役（`dispatch/84` + `89`，§57 / §59） ----------------
	# 84 号把那份 426 行的 GDScript 局面机删到只剩 4 个查表函数；89 号把播放屏那 4 行调用
	# 改成问驱动（`GwReplayDriver.axis_index(...)` 等），于是**类与文件都该整份消失**。
	# 本组钉三件事：① 类/文件确实没了（不是"方法表碰巧干净"）；② `client/src/**` 里
	# **可执行**引用 0 处（注释里的历史说明允许留）；③ 被删掉的那张表在 Rust 上**逐值**
	# 复现（14 个已知取值 + 4 个兜底），不是"大概对"。
	if ClassDB.class_exists(REPLAY_OLD_PLAYER_CLASS):
		var old_player: Object = ClassDB.instantiate(REPLAY_OLD_PLAYER_CLASS)
		var back := PackedStringArray()
		for m in REPLAY_STATE_MACHINE_METHODS:
			if _methods_of(old_player).has(String(m)):
				back.append(String(m))
		ok(false, "旧局面机类 `%s` 又回来了（89 号已整份删除；方法表：%s）"
			% [REPLAY_OLD_PLAYER_CLASS, str(back)])
	else:
		ok(true, "旧局面机 `%s` 整个类都不在了（局面只剩 Rust 一份）"
			% REPLAY_OLD_PLAYER_CLASS)
	var old_src := FileAccess.get_file_as_string(REPLAY_OLD_PLAYER_PATH)
	ok(not FileAccess.file_exists(REPLAY_OLD_PLAYER_PATH),
		"`%s` 已**整份删除**（exists=%s，%d 行）"
		% [REPLAY_OLD_PLAYER_PATH, str(FileAccess.file_exists(REPLAY_OLD_PLAYER_PATH)),
			old_src.split("\n").size() if old_src != "" else 0])

	# ② 可执行引用：逐行扫 `client/src/**`，**整行注释跳过**（"这文件删了"写进注释最有价值）。
	var live_hits := _scan_src_identifier(REPLAY_OLD_PLAYER_CLASS)
	ok(live_hits.is_empty(),
		"`client/src/**` 里 `%s` 的**可执行**引用 0 处（命中：%s）"
		% [REPLAY_OLD_PLAYER_CLASS, str(live_hits)])
	# 正对照：同一个扫描器必须扫得到确实存在的符号，否则"0 命中"什么也不能证明（AGENTS.md A.4）。
	var ctrl := _scan_src_identifier(REPLAY_DRIVER_CLASS)
	ok(not ctrl.is_empty(),
		"正对照：同一个扫描器扫到 `%s` %d 处 ⇒ 上一条的 0 命中有效"
		% [REPLAY_DRIVER_CLASS, ctrl.size()])

	# ③ 逐值复现旧表。**期望值来自旧实现**（删文件前抄下来的数），不是新实现自证。
	var mode_wrong := PackedStringArray()
	for case in REPLAY_MODE_CASES:
		var got := _mode_index_of(String(case[0]), String(case[1]))
		if got != int(case[2]):
			mode_wrong.append("%s(%s)=%d≠%d"
				% [String(case[0]), String(case[1]), got, int(case[2])])
	ok(mode_wrong.is_empty(),
		"`%s` 的 4 个静态查表函数逐值复现旧 `%s` 的表（%d 个用例；不符：%s）"
		% [REPLAY_DRIVER_CLASS, REPLAY_OLD_PLAYER_CLASS, REPLAY_MODE_CASES.size(),
			str(mode_wrong)])

	_finish()

## 在 `client/src/**` 里搜一组符号，返回 `文件:符号` 的命中清单（空 = 生产路径里没有这些调用）。
##
## 负结论必须配正对照（`AGENTS.md` 合并纪律 A.4）：调用方先在 `client/src` 里数 `.gd` 的个数，
## 个数为 0 时"没命中"没有意义 —— 调用处断言了扫描面非空。
func _scan_src_symbols(symbols: Array) -> PackedStringArray:
	var files := PackedStringArray()
	_gd_files("res://src", files)
	if files.size() <= 20:
		push_error("[gw2] 源码扫描面只有 %d 个 .gd，负结论不可信" % files.size())
	var hits := PackedStringArray()
	for path in files:
		var text: String = FileAccess.get_file_as_string(path)
		for sym in symbols:
			if text.contains(String(sym)):
				hits.append("%s:%s" % [path, sym])
	return hits

## 扫 `client/src/**` 里的 .gd，返回 `文件:行号` —— **整行注释跳过**（`#` 开头的行不算引用）。
##
## 为什么要跳过注释：`GwReplayPlayer` 这个名字在"它已被删除"的说明里出现最有价值，
## 而那种说明都是注释；把注释也算命中，等于逼后来的人删掉解释。
## 判据仍是严的：任何一行**代码**（含行尾注释里的同名符号）都会命中。
## 正对照由调用方做（同一个扫描器必须能扫到确实存在的符号）。
func _scan_src_identifier(sym: String) -> PackedStringArray:
	var files := PackedStringArray()
	_gd_files("res://src", files)
	if files.size() <= 20:
		push_error("[gw2] 源码扫描面只有 %d 个 .gd，负结论不可信" % files.size())
	var hits := PackedStringArray()
	for path in files:
		var text: String = FileAccess.get_file_as_string(path)
		var line_no := 0
		for line in text.split("\n"):
			line_no += 1
			if line.strip_edges().begins_with("#"):
				continue
			if line.contains(sym):
				hits.append("%s:%d" % [path, line_no])
	return hits

## 调驱动上的**静态**查表函数（`GwReplayDriver.axis_index(s)` 等），返回它给的整数。
##
## 走实例 `call()` 而不是 `GwReplayDriver.axis_index(...)` 的直接写法：直接写在 GDScript 里
## 是**编译期**解析的，方法一旦不在就整脚本 Parse Error（`dispatch/84` §57.3 那次实测），
## 而这里要的是"变红"，不是"检查脚本自己起不来"。调用不到就返回 `-999`（不可能等于期望值）。
func _mode_index_of(method: String, arg: String) -> int:
	var drv: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	if drv == null:
		return -999
	var got: Variant = drv.call(method, arg)
	if got == null:
		return -999
	return int(got)

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

## 本机原版存档目录里的那个样本的**绝对路径**（没有则返回空串）。
##
## 与 `selftest.gd::_real_replay_dir()` 同一条路：`$HOME/.local/share/godot/app_userdata/
## Graphwar/replays/<时间戳>`（原版存档目录，**只读**；仓库里不放任何回放文件）。
func _real_replay_path() -> String:
	var home := OS.get_environment("HOME")
	if home == "":
		return ""
	var p := home.path_join(".local/share/godot/app_userdata/Graphwar/replays")
	p = p.path_join(REPLAY_REAL_NAME)
	return p if FileAccess.file_exists(p) else ""

## 新建一个加载好合成回放的驱动器并 `play()`（每条的起点都干净）。
func _driver_fresh() -> Object:
	var d: Object = ClassDB.instantiate(REPLAY_DRIVER_CLASS)
	d.call("load_text", SYNTHETIC_REPLAY)
	d.call("play")
	return d

## 读 84 号固化的夹具（`key<TAB>value`；`#` 注释、空行忽略）。
##
## `res://` 是 `client/`，运行时读不到仓库根的 `fixtures/` ⇒ 用绝对路径
## （`ProjectSettings.globalize_path("res://")` 的上一级；与 `selftest.gd::_repo_root()` 同一条路）。
func _replay_fixture() -> Dictionary:
	var out := {}
	var root: String = ProjectSettings.globalize_path("res://").path_join("..")
	var f := FileAccess.open(root.path_join(REPLAY_FIXTURE), FileAccess.READ)
	if f == null:
		return out
	while not f.eof_reached():
		var line := f.get_line()
		if line.begins_with("#") or line.strip_edges() == "":
			continue
		var parts := line.split("\t")
		if parts.size() == 2:
			out[parts[0]] = parts[1]
	return out

## 事件名在不在一个**字符串数组**里（驱动报告里的 `kinds`）。
func _has_kind_arr(kinds: Array, kind: String) -> bool:
	for k in kinds:
		if String(k) == kind:
			return true
	return false

## 批的边界三元组（用来把"驱动的批"与"独立扫描的批"逐项比）。
func _batch_bounds(batches: Array) -> Array:
	var out := []
	for b in batches:
		var d: Dictionary = b
		out.append([int(d.get("start_tick", -1)), int(d.get("exec_tick", -2)),
			int(d.get("next_start_tick", -3))])
	return out

## **独立扫描**（本脚本自己实现，不走 Rust、不调 `replaydiff`）：
## 从回放文本重新切结算批 —— `CalculatingResults` → `ExecutingResults` → 下一批开始。
##
## 这份实现存在的唯一理由就是"让驱动的批边界有一份**别的**实现可以对"：
## 两边都错成一样的概率，远小于一边错。
func _expected_batches(text: String) -> Array:
	return _expected_batches_raw(GwReplayFile.parse_text(text))

func _expected_batches_raw(f: GwReplayFile) -> Array:
	var out := []
	if not f.ok():
		return out
	var evs := f.events
	var last_tick := 0
	for e in evs:
		last_tick = int(e.tick)
	var i := 0
	while i < evs.size():
		if _is_state(evs[i], "CalculatingResults"):
			var start := i
			var j := i
			var end := -1
			while j < evs.size():
				if _is_state(evs[j], "ExecutingResults"):
					end = j
					break
				if j > start and _is_state(evs[j], "CalculatingResults"):
					break
				j += 1
			if end >= 0:
				var tail := end + 1
				while tail < evs.size():
					if _is_state(evs[tail], "CalculatingResults"):
						break
					tail += 1
				var next_tick := last_tick
				if tail < evs.size():
					next_tick = int(evs[tail].tick)
				var shots := 0
				for k in range(start, end + 1):
					if String(evs[k].kind) == "FunctionPoints":
						shots += 1
				out.append({
					"start_tick": int(evs[start].tick),
					"exec_tick": int(evs[end].tick),
					"next_start_tick": maxi(next_tick, int(evs[end].tick)),
					"shots": shots,
				})
				i = end + 1
				continue
		i += 1
	return out

func _is_state(ev: Dictionary, state: String) -> bool:
	return String(ev.kind) == "GameStateInfo" \
		and String(ev.data.get("game_state", "")) == state

## 回放文本里 `tick ∈ (lo, hi]` 的事件名（文件顺序）—— 用来与驱动吐出的 `kinds` 比。
func _kinds_upto(text: String, lo: int, hi: int) -> Array:
	var f := GwReplayFile.parse_text(text)
	var out := []
	for e in f.events:
		var t := int(e.tick)
		if t > lo and t <= hi:
			out.append(String(e.kind))
	return out

## 一批事件里有没有某种 `kind`。
func _has_kind(events: Array, kind: String) -> bool:
	for e in events:
		if String((e as Dictionary).get("kind", "")) == kind:
			return true
	return false

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("PLAY_FLOW %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
