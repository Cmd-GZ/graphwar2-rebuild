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
## | 5 | `KernelBus::drive_frame` 能不能在播放屏 `_process` 接上（**评估**，本轮**没有**接） | **观测**（真引擎里驱动出来的 tick 数） |
## | 6 | **弹道生命周期**（`GwShotFlow`）：命中按点序号落地 / 飞完 / 只结算一次 / 整批结算 | 落地时序 **观测**（`0x472814` 死亡块取点序号；`docs/14` §7.3 106/106 按 `start_tick` 升序） |
## | 7 | **回合超时**（`GwTurnClock`）：六档时长 + 三个合取项 + 我方闩 | 谓词 **观测**（`0x470989`–`0x4709ec`）；闩是**我方**兜底 |
## | 8 | **AI 回合闸门**（`GwBotTurn`）：思考倒计时 + 一发直线 | **我方自定**（原版是 `aigen` 进化器，时序未观测） |
## | 9 | 播放屏源码里**不再有**这些流程分支，且确实调的是新入口 | **观测**（读文件文本，等价于 `grep`） |
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

	# ---------------- 5. `drive_frame` 能不能接到播放屏 `_process`（**评估**） ----------------
	# 结论写在 `docs/16-client.md` §52.5：**技术上能接**（下面三行就是全部接线），本轮**没接**，
	# 因为 `tick` 在播放屏里还没有消费者（曲线的揭示仍按帧 delta 推）且 `EntityDrawer` 的 `V`
	# 与这条总线是**同一台** tick handler ⇒ 先接会静默改掉它的读数，没有真帧可对。
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

	_finish()

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
