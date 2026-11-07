extends SceneTree
## `GraphwarRootNode` **真装配**的验收脚本（`docs/16-client.md` §38 → §41 扩到 22 个）。
##
## ```text
## cargo build --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/assemble_check.gd
## ```
##
## 槽位状态查的是**复原档位**（四个词，判据在 `src/_scaffold/root_wiring.rs` 的模块文档
## "槽位复原档位"一节，2026-11 任务 28 定）：
##
## ```text
## wired      已接容器 + 已接事件流 + 有最小交互断言（三件齐）
## layout     原版字段布局已逐字段解出，但没到 wired（事件流没接全 / 无交互断言）
## assembled  容器里是我方复原类型，但没接事件流 / 没有最小交互断言
## stub       我方形状占位：原版字段布局未知
## ```
##
## ⚠️ `stub` 的旧读法（"零行为空占位"）**作废**：它现在只说"结构都没解"，与"能不能交互"无关。
## 这四个词由 `module_slot_status()` 原样返回，与 `Wired::status()` 是同一套（不要各写一套）。
##
## 它验证的是「**容器真的按原版顺序装进了 22 个 manager、两张屏真的挂上了真节点类**」这几句，
## 不验证 manager / 棋盘 / 绘制器的**算术**（那些各自有脚本：`entity_drawer_check.gd`
## §34、`draw_classes_check.gd` §37、`signal_classes_check.gd` §36、`base_classes_check.gd` §35）：
##
## 1. `ClassDB.class_exists("GraphwarRootNode")` + 能实例化成 `Node`；
## 2. **按类型**在容器里查到 **22** 个 manager（顺序 = 原版 `GraphwarGameClient::new`
##    的注册序，`docs/16-client.md` §41.2），**不是按数量**：同一时刻查
##    `GraphwarBanList`（客户端**不**注册的那 3 型之一）必须是 `false`；
##    每个槽还要过 `container_module_live()`（真 downcast 回来）与
##    `manager_slot_size()`（原版 `size_of::<M>()`，22 个数逐字对）；
## 3. 取回容器里的对象**再读一次**（士兵 3 个 / 玩家 1 有 2 兵、玩家 2 有 1 兵 / 玩家表 2 行）
##    —— 证明 `get_module` 拿到的是活对象，不是"表里有个键"；
## 4. **切屏**：`show_screen("GameSetup")` / `show_screen("PlayScreen")` 之后
##    Setup 屏与 Game 屏的 `visible` 状态互斥且正确（走的是 `0x3af850` 的决策表）；
## 5. **Game 屏**：`GraphwarPlane` **恰好 1 个**、`EntityDrawer` **恰好 1 个**、
##    绘制器里 **3 个实体 × 4 个子节点 = 12**（3 个 `Sprite2D` + 1 个 `RichTextLabel` 一组）；
## 6. **每帧**：进树之后 Plane 的 `_draw` 真的被引擎调到（计数 ≥ 1），绘制器的 `_process` 也是。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const ROOT_CLASS := "GraphwarRootNode"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 容器的**类型名**（`graphwar_module_container::MODULES` 的原文，不是我们起的别名）。
##
## 顺序 = **原版 `GraphwarGameClient::new` 的注册序**（arm64 `0x409c70` / x86 `0x517690`
## 的 22 条 `add_module`；`docs/16-client.md` §41.2）。两架构的
## `size_of::<RefCell<M>>` 序列逐位相同，所以这个顺序不是我们排的。
const WIRED := [
	"GraphwarTickHandler", "GraphwarClientConnectionHandler", "GraphwarLocalPlayerManager",
	"GraphwarPlayerManager", "GraphwarSoldierManager", "GraphwarPlanePosManager",
	"GraphwarGameStateManager", "GraphwarFunctionManager", "GraphwarTargetManager",
	"GraphwarTeamManager", "GraphwarLifeManager", "GraphwarObstacleManager",
	"GraphwarTurnManager", "GraphwarEffectManager", "GraphwarModeManager",
	"GraphwarGraphicsManager", "GraphwarEntityEffectManager", "GraphwarComputerPlayerManager",
	"FunctionHistoryRecorder", "GraphwarNameManager", "GraphwarColorManager",
	"GraphwarRankManager",
]
## 每个槽的**原版 `size_of::<M>()`**（vtable size 字 − 8；两架构相同，`docs/02` §22.3）。
## 这一串与注册序配对后就是 §41.2 的"逐位"证据：错一位就会红。
const WIRED_SIZE := [
	72, 112, 64, 72, 72, 24, 296, 32, 72, 32, 72, 80, 32, 72, 32, 168, 72, 72, 48, 72, 120, 72,
]
## 每个槽的**复原档位**（`module_slot_status()` 的原样返回值；顺序与 `WIRED` 一一对应）。
##
## 这是 `root_wiring.rs` 里 `Wired::status()` 的 GDScript 影印版：两边必须逐字相同，
## 那边的单测 `slot_tiers_are_the_four_word_vocabulary` 与本脚本的 22 条逐槽断言互为对照。
## 任务 28 的病根正是"只改了一边"（注释说改了、字符串还是旧的），所以这里**断言**也一起写死。
const WIRED_TIER := [
	# 1  TickHandler          订 1 发 2 与原版一致 + 4 条交互 ⇒ 三件齐
	"wired",
	# 2  ClientConnectionHandler  112 B 的 9 个偏移全解，行为 0 行
	"layout",
	# 3  LocalPlayerManager
	"assembled",
	# 4  PlayerManager        订 2 发 2 + 6 条交互（任务 40）
	"wired",
	# 5  SoldierManager
	"assembled",
	# 6  PlanePosManager      24 B 布局未知（连文件都没有）
	"stub",
	# 7  GameStateManager
	"assembled",
	# 8  FunctionManager      32 B / 4 字段全解，容器 + 6 条交互都在，但**发方向**事件流没接
	"layout",
	# 9  TargetManager        订 2 发 1 + 5 条交互
	"wired",
	# 10 TeamManager         订 2 发 2 + 7 条交互（任务 40）
	"wired",
	# 11 LifeManager          订 3 发 2 + 5 条交互
	"wired",
	# 12 ObstacleManager      订 1 发 4 + 7 条交互
	"wired",
	# 13 TurnManager          订 2 发 1 + 6 条交互（任务 40）
	"wired",
	# 14 EffectManager        订 1 发 1 + 7 条交互（任务 40）
	"wired",
	# 15 ModeManager          订 1 发 3 + 5 条交互（任务 40）
	"wired",
	# 16 GraphicsManager
	"assembled",
	# 17 EntityEffectManager  订 2 发 1 + 7 条交互（任务 40）
	"wired",
	# 18-22 ComputerPlayer / FunctionHistoryRecorder / Name / Color / Rank
	"assembled", "assembled", "assembled", "assembled", "assembled",
]
## 四档现值计数（顺序固定：wired / layout / assembled / stub；合计 = 22）。
const TIER_TALLY := {"wired": 10, "layout": 2, "assembled": 9, "stub": 1}
## 容器里**什么都没有**的两个槽（纯标记）：只有它们的 `manager_impl_type()` 回 `"—"`。
const EMPTY_IMPL := ["GraphwarClientConnectionHandler", "GraphwarPlanePosManager"]
## 容器类型是 `_scaffold` 包装/标记的 5 个（`SlotKind::Stub`，**不是**档位 `stub`）；
## `container_stub_manager_count()` 数的就是这一组（`docs/16` §41.2 的既有口径，17 + 5 = 22）。
const PACKAGED := [
	"GraphwarTickHandler", "GraphwarClientConnectionHandler", "GraphwarPlanePosManager",
	"GraphwarFunctionManager", "GraphwarObstacleManager",
]
## 上面那 5 个的档位（任务 28 逐个重判：**不再清一色 `stub`**）。
const PACKAGED_TIER := {
	"GraphwarTickHandler": "wired",
	"GraphwarClientConnectionHandler": "layout",
	"GraphwarPlanePosManager": "stub",
	"GraphwarFunctionManager": "layout",
	"GraphwarObstacleManager": "wired",
}
## 客户端**不**注册的那 3 型（25 − 22；服务端线程 / backend 才装）：
## 容器里查不到，`module_slot_status` 必须是 `placeholder`。
const NOT_WIRED := ["GraphwarBanList", "GraphwarServerConnectionHandler", "GraphwarGameRecorder"]
## 一个连 25 型台账里都没有的名字。
const NONSENSE := "GraphwarNotARealManager"

const SETUP_SCREEN := "GameSetup"
const GAME_SCREEN := "PlayScreen"
const PLANE_NAME := "GraphwarPlane"

## `GraphwarRootNode` 上属于 §38 的那批方法（容器观测口 + 装配口）。
const CONTAINER_METHODS := [
	"wiring_done", "container_module_count", "container_has_module",
	"container_module_names", "wired_manager_count", "assembled_manager_total",
	"container_game_state_soldiers", "container_turn_manager_soldiers",
	"container_player_manager_players", "module_slot_status",
	# §41 新增的 6 个观测口
	"container_module_live", "container_live_count", "container_real_manager_count",
	"container_stub_manager_count", "manager_slot_size", "manager_impl_type",
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

func _method_names(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

func _children_of(node: Node) -> Array:
	var out: Array = []
	for i in range(node.get_child_count()):
		out.append(node.get_child(i))
	return out

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	if not ClassDB.class_exists(ROOT_CLASS):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists(ROOT_CLASS), "ClassDB.class_exists(\"%s\")" % ROOT_CLASS)
	if not ClassDB.class_exists(ROOT_CLASS):
		_finish()
		return

	# ------------------------------------------------------------------
	# 1) 实例化 + 父类 + §38 的方法表
	# ------------------------------------------------------------------
	var root_node: Node = ClassDB.instantiate(ROOT_CLASS)
	ok(root_node != null and root_node is Node, "%s 能实例化且是 Node" % ROOT_CLASS)
	if root_node == null:
		_finish()
		return
	print("      root.get_class() = %s" % root_node.get_class())
	ok(root_node.is_class("Node"), "is_class(\"Node\") 为真（基类正确）")

	var methods := _method_names(root_node)
	for wanted in CONTAINER_METHODS:
		ok(methods.has(wanted), "方法 %s() 存在" % wanted)

	# ------------------------------------------------------------------
	# 2) 装配（**进树之前**就能查：`build_scene_graph` 是幂等的 self-contained 入口）
	# ------------------------------------------------------------------
	var built: int = int(root_node.call("build_scene_graph"))
	print("      root 直接子节点 = %d" % built)
	ok(built == 4, "build_scene_graph() 仍是 4 个直接子节点（3 holder + module 容器）")
	ok(bool(root_node.call("wiring_done")), "wiring_done() —— 容器已装配")

	var count: int = int(root_node.call("container_module_count"))
	print("      容器模块数 = %d；按类型成立的 = %d；能取回的 = %d；原版模块类型 %d + _scaffold 包装 %d"
		% [count, int(root_node.call("wired_manager_count")),
			int(root_node.call("container_live_count")),
			int(root_node.call("container_real_manager_count")),
			int(root_node.call("container_stub_manager_count"))])
	ok(count == WIRED.size(), "container_module_count() == %d（实际 %d）" % [WIRED.size(), count])
	ok(int(root_node.call("wired_manager_count")) == WIRED.size(),
		"wired_manager_count() == %d（**按类型查**出来的）" % WIRED.size())
	ok(int(root_node.call("container_live_count")) == WIRED.size(),
		"container_live_count() == %d（每个槽都能 `get_module` 取回）" % WIRED.size())
	ok(int(root_node.call("assembled_manager_total")) == WIRED.size(),
		"assembled_manager_total() == %d（= 原版 22 条 add_module）" % WIRED.size())
	# 17 + 5 = 22：17 个槽容器里直接装原版模块类型，5 个槽容器类型是 `_scaffold` 包装/标记
	# （`SlotKind::RealType` / `SlotKind::Stub`，§41.2 的既有口径）。
	# ⚠️ 这**不是档位**：档位是 10 wired + 2 layout + 9 assembled + 1 stub（见 `WIRED_TIER`）。
	# 两个口子的函数名（尤其 `container_stub_manager_count`）是 §41 遗留措辞，函数在
	# `graphwar_root_node_class.rs`（本轮禁区），`docs/16` §47.6 记了这条待改。
	ok(int(root_node.call("container_real_manager_count")) == WIRED.size() - PACKAGED.size(),
		"容器里直接装原版模块类型的槽 == %d（SlotKind::RealType，22 − 5 个包装）"
			% (WIRED.size() - PACKAGED.size()))
	ok(int(root_node.call("container_stub_manager_count")) == PACKAGED.size(),
		("容器类型是 `_scaffold` 包装/标记的槽 == %d（SlotKind::Stub；"
			+ "**不是**档位 stub 的那 1 个）") % PACKAGED.size())

	# —— 档位（四个词）：这里断总数与词表，逐槽在下面那个 22 循环里断 ——
	var tally := {"wired": 0, "layout": 0, "assembled": 0, "stub": 0}
	var outside := []
	for type_name in WIRED:
		var got := String(root_node.call("module_slot_status", type_name))
		if tally.has(got):
			tally[got] = int(tally[got]) + 1
		else:
			outside.append("%s=%s" % [type_name, got])
	print("      四档: %s（越界 %s）" % [tally, outside])
	ok(int(tally["wired"]) == int(TIER_TALLY["wired"])
		and int(tally["layout"]) == int(TIER_TALLY["layout"])
		and int(tally["assembled"]) == int(TIER_TALLY["assembled"])
		and int(tally["stub"]) == int(TIER_TALLY["stub"]),
		"四档计数 == wired 10 / layout 2 / assembled 9 / stub 1（实际 %s）" % tally)
	ok(int(tally["wired"]) + int(tally["layout"]) + int(tally["assembled"]) + int(tally["stub"])
		== WIRED.size(), "四档合计 == %d（每个槽只报一档）" % WIRED.size())
	ok(outside.is_empty(),
		"22 个槽的档位都在四词词表里（越界：%s）" % [outside])

	# —— 核心：**按类型**逐条查（不是数一个总数）——
	#   每个槽四条：键在（contains）/ 取回（get_module downcast）/ **复原档位** / impl_type。
	for i in range(WIRED.size()):
		var type_name: String = WIRED[i]
		ok(bool(root_node.call("container_has_module", type_name)),
			"容器里按类型查得到 %s" % type_name)
		ok(bool(root_node.call("container_module_live", type_name)),
			"容器里能 **取回** %s（get_module downcast 成功）" % type_name)
		var want_status: String = WIRED_TIER[i]
		var got_status := String(root_node.call("module_slot_status", type_name))
		ok(got_status == want_status,
			"%s 的档位 = %s（实际 %s）" % [type_name, want_status, got_status])
		var impl_type := String(root_node.call("manager_impl_type", type_name))
		var want_empty: bool = EMPTY_IMPL.has(type_name)
		ok((impl_type == "—") == want_empty,
			"%s 的 impl_type %s（实际 %s）" % [type_name,
				"应为 —（容器里什么都没有）" if want_empty else "应为复原类型路径", impl_type])

	# —— 注册序（逐位证据的运行时版本）：22 个名字 + 22 个原版 size ——
	var names: PackedStringArray = root_node.call("container_module_names")
	print("      container_module_names() = %s" % names)
	ok(names == PackedStringArray(WIRED),
		"container_module_names() == 原版注册序的 22 个类型名（实际 %s）" % names)
	var sizes := []
	for type_name in WIRED:
		sizes.append(int(root_node.call("manager_slot_size", type_name)))
	ok(sizes == WIRED_SIZE,
		"22 个 size_of::<M>() == 两架构逐位相同的那串数（实际 %s）" % [sizes])
	ok(int(root_node.call("manager_slot_size", NONSENSE)) == -1,
		"胡编的名字没有 size（-1）")

	# —— 反面：客户端不注册的 3 型 / 胡编的名字一律查不到 ——
	for type_name in NOT_WIRED:
		ok(not bool(root_node.call("container_has_module", type_name)),
			"客户端不注册的 %s 查不到（⇒ 判据是类型，不是数量）" % type_name)
		ok(String(root_node.call("module_slot_status", type_name)) == "placeholder",
			"%s 的槽位状态 = placeholder（原版 25 型里有它，客户端这一处不装）" % type_name)
	ok(not bool(root_node.call("container_has_module", NONSENSE)),
		"胡编的名字 %s 查不到" % NONSENSE)

	# 25 型台账 / 22 个注册点 / 22 个已装配 —— 前两个数**不是一回事**
	ok(int(root_node.call("manager_slot_count")) == 25, "manager_slot_count() == 25（原版模块类型数）")
	ok(int(root_node.call("manager_registration_count")) == 22,
		"manager_registration_count() == 22（GraphwarGameClient::new 的注册点）")

	# —— 取回对象再读一次：证明 get_module 拿到的是活对象 ——
	var soldiers: int = int(root_node.call("container_game_state_soldiers"))
	print("      GameStateManager 的士兵数 = %d；TurnManager[1] = %d 兵、[2] = %d 兵；玩家表 = %d 行"
		% [soldiers, int(root_node.call("container_turn_manager_soldiers", 1)),
			int(root_node.call("container_turn_manager_soldiers", 2)),
			int(root_node.call("container_player_manager_players"))])
	ok(soldiers == 3, "取回 GameStateManager ⇒ 里面是 3 个假士兵（实际 %d）" % soldiers)
	ok(int(root_node.call("container_turn_manager_soldiers", 1)) == 2,
		"取回 TurnManager ⇒ 玩家 1 有 2 个士兵")
	ok(int(root_node.call("container_turn_manager_soldiers", 2)) == 1,
		"取回 TurnManager ⇒ 玩家 2 有 1 个士兵")
	ok(int(root_node.call("container_turn_manager_soldiers", 99)) == -1,
		"TurnManager 里没有玩家 99 ⇒ -1（不是 0，也不是 panic）")
	ok(int(root_node.call("container_player_manager_players")) == 2,
		"取回 PlayerManager ⇒ player_id → connection_id 表有 2 行")

	# —— 槽位状态的最后一档：胡编的名字 ——
	ok(String(root_node.call("module_slot_status", NONSENSE)) == "unknown",
		"%s 的槽位状态 = unknown（原版 25 型里也没有）" % NONSENSE)


	# ------------------------------------------------------------------
	# 2b) **一次最小交互后取回正确值**（任务 19 + 任务 40；docs/16-client.md §44 / §50）
	#
	# 上面 2) 那一段只证明"22 个槽都能按类型取回"——**取回来的是活对象**这件事，
	# 得真调一个入口再读回来才算。这里用 `manager_probe_selftest()`（**我方自造**的
	# 观测口，原版没有；理由见 §44）：它在容器里对 11 个 manager 各跑一遍
	# "取回 → 写 → 读回"，返回 65 条 `名字=值`。
	#
	# ⚠️ 期望值**不是**从返回值抄下来的：它们逐条对应 `root_wiring.rs` 的单测
	# `one_minimal_interaction_per_manager_reads_back_the_written_value`，
	# 那边不通这一条也过不了（两边同一个 `interaction_selftest`）。
	# ------------------------------------------------------------------
	const PROBE_METHODS := ["manager_probe_selftest", "manager_probe"]
	for wanted in PROBE_METHODS:
		ok(methods.has(wanted), "方法 %s() 存在（我方观测口，§43）" % wanted)

	var expected_probe := [
		# ① LifeManager：set_alive(0x454b40) → get_alive(0x454950)
		"life.alive=1", "life.death_tick=42", "life.due_at_42=1",
		"life.death_tick_after=none", "life.subs=3",
		# ② TargetManager：add_target → contains；两条订阅各走一次
		"target.contains=true", "target.replayed=1", "target.removed=true",
		"target.contains_after=false", "target.subs=2",
		# ③ ObstacleManager：add_obstacle → is_obstacle → 排队 → 164 不炸 / 165 炸
		"obstacle.hit_after_add=true", "obstacle.hit_far=false", "obstacle.pending=1",
		"obstacle.due_164=0", "obstacle.due_165=1", "obstacle.hit_after_destroy=false",
		"obstacle.subs=1",
		# ④ FunctionManager：set_function → get_function_text；active_tick 三条窗口
		"function.text=x", "function.active_tick=100", "function.active_at_101=1",
		"function.active_at_100=0", "function.active_at_200=0", "function.subs=0",
		# ⑤ TickHandler：tick() 推进计数并发 TickEvent(0)
		"tick.delta=1", "tick.event_tag=0", "tick.at_500ms=30", "tick.subs=1",
		# ⑥ TurnManager（任务 40）：begin_turn → Announce → TurnInfo(43)
		"turn.soldier=9023", "turn.flip=false", "turn.emitted=1", "turn.subs=2",
		"turn.removed=true", "turn.present_after=false",
		# ⑦ PlayerManager：add_player → connection_of；级联 ConnectionRemoved(6)
		"player.conn=9025", "player.emitted=1", "player.subs=2", "player.replayed=3",
		"player.cascade=true", "player.outbox=5",
		# ⑧ TeamManager：add_to_team → get_team；pick_team(0x3c1b40)
		"team.added=Team2", "team.counts=1,2", "team.pick=Team1", "team.subs=2",
		"team.replayed=3", "team.removed=1", "team.outbox=5",
		# ⑨ ModeManager：set_axis_mode(0x497dc8) → AxisModeInfo(45)；末尾复位
		"mode.axis=2", "mode.replayed=3", "mode.restored=true", "mode.outbox=5",
		"mode.subs=1",
		# ⑩ EntityEffectManager：出生登记 → 死亡结算读回同一格
		"entity_effect.kind=1", "entity_effect.emitted=1", "entity_effect.subs=2",
		"entity_effect.replayed=1", "entity_effect.removed=true",
		"entity_effect.after=true", "entity_effect.outbox=2",
		# ⑪ EffectManager：add_effect → EffectInfo(44)；tick_event 到期回收不发声
		"effect.kind=0", "effect.emitted=1", "effect.subs=1", "effect.due_early=0",
		"effect.due_at=1", "effect.gone=true", "effect.outbox=1",
	]
	var probe: PackedStringArray = root_node.call("manager_probe_selftest")
	print("      manager_probe_selftest() 给出 %d 条（期望 %d 条）" % [probe.size(), expected_probe.size()])
	ok(probe == PackedStringArray(expected_probe),
		"11 个 manager 的最小交互结果 == 65 条 名字=值（实际 %s）" % probe)
	ok(probe.size() == expected_probe.size(),
		"交互条数 == 65（11 个 manager：5+5+7+6+4 + 6+6+7+5+7+7）")

	# 逐个 manager 各取一条最关键的值再查一遍（单条查询口）
	ok(String(root_node.call("manager_probe", "life.alive")) == "1",
		"LifeManager：set_alive(id,true) 之后 get_alive(id) == 1（真表读回）")
	ok(String(root_node.call("manager_probe", "target.contains")) == "true",
		"TargetManager：add_target(id) 之后 contains(id) == true")
	ok(String(root_node.call("manager_probe", "obstacle.hit_after_destroy")) == "false",
		"ObstacleManager：queue→165 tick 之后 is_obstacle 变 false（掩膜真的被挖了）")
	ok(String(root_node.call("manager_probe", "function.text")) == "x",
		"FunctionManager：set_function(VarX) 之后 get_function_text == \"x\"")
	ok(String(root_node.call("manager_probe", "tick.delta")) == "1",
		"TickHandler：tick() 让 current_tick 恰好 +1")

	# —— 任务 40 的 6 个：每个一条"交互后取回正确值" + 一条事件流读数 ——
	#
	# ⚠️ 这些值同样是"写进去 → 从被写的那张表读回来"，不是常量：Rust 侧
	# `the_new_read_back_values_track_the_written_state` 换输入读回值跟着变。
	ok(String(root_node.call("manager_probe", "turn.soldier")) == "9023",
		"TurnManager：begin_turn(玩家 9021, 士兵 [9022,9023]) 先自增再取 ⇒ 射手 9023")
	ok(String(root_node.call("manager_probe", "turn.subs")) == "2",
		"TurnManager：订阅 2 条（new_connection_event / entity_removed_event，0x4637fc / 0x46384d）")
	ok(String(root_node.call("manager_probe", "player.conn")) == "9025",
		"PlayerManager：add_player(9024,9025) 之后 connection_of(9024) == 9025")
	ok(String(root_node.call("manager_probe", "player.cascade")) == "true",
		"PlayerManager：该连接最后一人被删 ⇒ 级联 ConnectionRemoved(6)")
	ok(String(root_node.call("manager_probe", "team.added")) == "Team2",
		"TeamManager：add_to_team(9026, Team2) 之后 get_team(9026) == Team2")
	ok(String(root_node.call("manager_probe", "team.pick")) == "Team1",
		"TeamManager：分队规则 0x3c1b40（Σteam < Σ(team^1)）在 (1,2) 上仍是 Team1")
	ok(String(root_node.call("manager_probe", "mode.axis")) == "2",
		"ModeManager：set_axis_mode(EveryFive) 之后 axis_mode == 2（判别值 = 声明序）")
	ok(String(root_node.call("manager_probe", "mode.restored")) == "true",
		"ModeManager：probe 末尾复位，容器里的模式与调用前一致（幂等的前提）")
	ok(String(root_node.call("manager_probe", "entity_effect.kind")) == "1",
		"EntityEffectManager：register_soldier(空皮肤) 登记 DeathEffect(1)，死亡结算读回同一格")
	ok(String(root_node.call("manager_probe", "effect.due_at")) == "1",
		"EffectManager：start_tick=1000 ⇒ 4600 tick 到期回收（3600 是 EFFECT_LIFETIME_TICKS）")

	# 反面对照（任务 28 重判）：这 5 个 `SlotKind::Stub` 槽的档位**跟着事实走**，
	# 不再是清一色 `stub`——"能交互"不等于"复原了原版结构"，两件事分开记（§43 / §47）：
	#   TickHandler / ObstacleManager = wired（三件齐）
	#   ClientConnectionHandler / FunctionManager = layout（布局逐字段解出，行为没做完）
	#   PlanePosManager = stub（24 B 布局未知）
	for type_name in PACKAGED:
		ok(String(root_node.call("module_slot_status", type_name)) == PACKAGED_TIER[type_name],
			"%s 的档位 = %s（不再是清一色 stub）" % [type_name, PACKAGED_TIER[type_name]])
	ok(int(tally["stub"]) == 1,
		"22 个槽里只剩 1 个真 stub（PlanePosManager），不是 5 个")

	# 幂等：连调两次必须给出同一串（probe 不许把状态改歪）
	var probe_again: PackedStringArray = root_node.call("manager_probe_selftest")
	ok(probe_again == probe, "连调两次 manager_probe_selftest() 结果完全相同（幂等）")
	ok(String(root_node.call("manager_probe", "no.such.key")) == "",
		"未知名字 ⇒ 空串（不是 0，也不是 panic）")

	# ------------------------------------------------------------------
	# 3) 两张屏：Setup 屏（GameSetup）→ Game 屏（PlayScreen）
	# ------------------------------------------------------------------
	var screens_control: Node = root_node.get_node("ScreensControl")
	ok(screens_control != null, "ScreensControl 存在")
	if screens_control == null:
		_finish()
		return
	ok(screens_control.get_child_count() == 15,
		"ScreensControl 下仍是 15 个屏节点（实际 %d）" % screens_control.get_child_count())

	var setup_screen: CanvasItem = screens_control.get_node(SETUP_SCREEN) as CanvasItem
	var game_screen: CanvasItem = screens_control.get_node(GAME_SCREEN) as CanvasItem
	ok(setup_screen != null, "Setup 屏节点 %s 存在" % SETUP_SCREEN)
	ok(game_screen != null, "Game 屏节点 %s 存在" % GAME_SCREEN)
	if setup_screen == null or game_screen == null:
		_finish()
		return

	# Setup 屏挂的是**真节点类**（`docs/16` §35 的 `GraphwarSetupControl`）
	ok(setup_screen.is_class("GraphwarSetupControl"),
		"Setup 屏是 GraphwarSetupControl（真节点类，不是空 Control）")
	print("      setup=%s game=%s" % [setup_screen.get_class(), game_screen.get_class()])
	ok(game_screen is Control, "Game 屏是 Control 容器（PlayScreenControl 还没节点化）")

	# —— 切到 Setup 屏 ——
	root_node.set("ads_slot", false)
	var writes_setup: int = int(root_node.call("show_screen", SETUP_SCREEN))
	var visible_setup: PackedStringArray = PackedStringArray(root_node.call("visible_screens"))
	print("      show_screen(%s) 写了 %d 次；可见 = %s" % [SETUP_SCREEN, writes_setup, visible_setup])
	ok(writes_setup == 15, "show_screen(%s) 遍历了 15 个屏（实际 %d）" % [SETUP_SCREEN, writes_setup])
	ok(setup_screen.visible == true, "Setup 屏可见")
	ok(game_screen.visible == false, "Game 屏不可见（切屏互斥）")
	ok(visible_setup == PackedStringArray([SETUP_SCREEN]),
		"visible_screens() == [%s]（实际 %s）" % [SETUP_SCREEN, visible_setup])

	# —— 切到 Game 屏 ——
	var writes_game: int = int(root_node.call("show_screen", GAME_SCREEN))
	var visible_game: PackedStringArray = PackedStringArray(root_node.call("visible_screens"))
	print("      show_screen(%s) 写了 %d 次；可见 = %s" % [GAME_SCREEN, writes_game, visible_game])
	ok(writes_game == 15, "show_screen(%s) 遍历了 15 个屏（实际 %d）" % [GAME_SCREEN, writes_game])
	ok(game_screen.visible == true, "Game 屏可见")
	ok(setup_screen.visible == false, "Setup 屏不可见（切屏互斥）")
	ok(visible_game == PackedStringArray([GAME_SCREEN]),
		"visible_screens() == [%s]（实际 %s）" % [GAME_SCREEN, visible_game])

	# ------------------------------------------------------------------
	# 4) Game 屏的内容：1 个 GraphwarPlane + 它的子节点里的 1 个 EntityDrawer
	# ------------------------------------------------------------------
	var planes: Array = game_screen.find_children("*", "GraphwarPlane", true, false)
	print("      Game 屏里的 GraphwarPlane: %s" % [planes.size()])
	ok(planes.size() == 1, "Game 屏里 GraphwarPlane 恰好 1 个（实际 %d）" % planes.size())
	if planes.size() != 1:
		_finish()
		return
	var plane: Node = planes[0]
	ok(String(plane.name) == PLANE_NAME, "棋盘节点名 == %s（原版 dump 里的名字）" % PLANE_NAME)

	var drawers: Array = plane.find_children("*", "EntityDrawer", true, false)
	print("      棋盘下的 EntityDrawer: %s" % [drawers.size()])
	ok(drawers.size() == 1, "棋盘下 EntityDrawer 恰好 1 个（实际 %d）" % drawers.size())
	if drawers.size() != 1:
		_finish()
		return
	var drawer: Node = drawers[0]
	ok(drawer.get_parent() == plane, "EntityDrawer 是 GraphwarPlane 的**子节点**（原版 dump 如此）")
	ok(String(drawer.name).begins_with("@EntityDrawer@"),
		"EntityDrawer 是引擎自动名 @EntityDrawer@N（原版没 set_name，实际 %s）" % drawer.name)

	# 3 个实体 × 4 个子节点
	var entity_count: int = int(drawer.call("entity_count"))
	print("      EntityDrawer: entity_count=%d 子节点=%d" % [entity_count, drawer.get_child_count()])
	ok(entity_count == 3, "EntityDrawer 里有 3 个实体（实际 %d）" % entity_count)
	ok(drawer.get_child_count() == 12,
		"3 个实体 × 4 个子节点 = 12（实际 %d）" % drawer.get_child_count())

	var kids := _children_of(drawer)
	var demo_names := ["P1-1", "P1-2", "P2-1"]
	if kids.size() == 12:
		for e in range(3):
			var offset: int = e * 4
			var spr := 0
			for k in range(3):
				if kids[offset + k] is Sprite2D:
					spr += 1
			var label_ok: bool = kids[offset + 3] is RichTextLabel
			ok(spr == 3 and label_ok,
				"实体 %d 的 4 个子节点 = 3 Sprite2D + 1 RichTextLabel（实际 %d sprite / label=%s）"
					% [e + 1, spr, label_ok])
			if label_ok:
				var text := String(kids[offset + 3].text)
				ok(text == demo_names[e], "实体 %d 的名字标签文本 == %s（实际 %s）"
					% [e + 1, demo_names[e], text])
		ok(String(kids[0].name).begins_with("@Sprite2D@"),
			"贴图子节点是引擎自动名 @Sprite2D@N（原版 dump 如此，实际 %s）" % kids[0].name)
	else:
		ok(false, "子节点不是 12 个，跳过逐实体类型断言")
	ok(plane.get_child_count() == 1,
		"进树之前棋盘只有我们的 1 个子节点（EntityDrawer 那个；实际 %d）" % plane.get_child_count())

	# ------------------------------------------------------------------
	# 5) 进树：`_ready` / `_process` / **Plane 的 _draw** 真的被引擎调到
	# ------------------------------------------------------------------
	root.add_child(root_node)
	# ⚠️ `--script` 模式下 `_initialize()` 跑在第一帧**之前**：这一刻 `root`（那个
	# `Window`）自己还没 `inside_tree`（实测），所以这里只能断言"父子关系已建立"，
	# `is_inside_tree()` 放到第一帧之后再验（`docs/16` §38 的踩坑清单）。
	ok(root_node.get_parent() == root, "根节点挂到了场景树根下（get_parent() == root）")
	await process_frame
	await process_frame
	ok(root_node.is_inside_tree(), "根节点进了场景树（第一帧之后 is_inside_tree() 才为真）")

	var rc: int = int(root_node.call("ready_call_count"))
	var pc: int = int(root_node.call("process_call_count"))
	print("      引擎回调: root ready=%d process=%d" % [rc, pc])
	ok(rc >= 1, "GraphwarRootNode._ready 被引擎调到（%d 次）" % rc)
	ok(pc >= 1, "GraphwarRootNode._process 被引擎调到（%d 次；不写 set_process）" % pc)

	# `GraphwarPlane::_draw` —— 任务书的硬要求「每帧被调用（计数 ≥ 1）」
	var draws: int = int(plane.call("draw_call_count"))
	var rects: int = int(plane.call("drawn_rect_total"))
	var plane_procs: int = int(plane.call("process_call_count"))
	print("      plane: draw=%d rect=%d process=%d ready=%d"
		% [draws, rects, plane_procs, int(plane.call("ready_call_count"))])
	ok((plane as CanvasItem).is_visible_in_tree(), "棋盘在树里且可见（否则 _draw 不会被调）")
	ok(draws >= 1, "Plane 的 _draw 被引擎调到（%d 次）" % draws)
	ok(rects == draws, "每次 _draw 发 1 条 draw_rect（%d/%d）" % [rects, draws])
	ok(plane_procs >= 1, "Plane 的 _process 被引擎调到（%d 次）" % plane_procs)

	var drawer_procs: int = int(drawer.call("process_call_count"))
	var drawer_ready: int = int(drawer.call("ready_call_count"))
	ok(drawer_ready >= 1, "EntityDrawer._ready 被引擎调到（%d 次）" % drawer_ready)
	ok(drawer_procs >= 1, "EntityDrawer._process 被引擎调到（%d 次）" % drawer_procs)

	# 再等一帧：`_draw` 还在被调（不是只在进树那一次）
	var before: int = draws
	await process_frame
	await process_frame
	var after: int = int(plane.call("draw_call_count"))
	ok(after >= before, "_draw 的计数只增不减（%d → %d）" % [before, after])
	ok(int(plane.call("drawn_rect_total")) == after, "draw_rect 条数 == _draw 次数（%d）" % after)

	# 进树之后 `GraphwarPlane::ready` 会补上 3 个 FunctionDrawer **占位**子节点
	ok(plane.get_child_count() == 4,
		"进树后棋盘有 4 个子节点（3 个 FunctionDrawer 占位 + 我们的 EntityDrawer，实际 %d）"
			% plane.get_child_count())
	ok(drawer.get_child_count() == 12, "进树后绘制器的 12 个子节点没变（实际 %d）" % drawer.get_child_count())

	# 装配是幂等的：再调一次不会重建容器/屏
	var built2: int = int(root_node.call("build_scene_graph"))
	ok(built2 == 4, "二次 build_scene_graph() 还是 4 个直接子节点（幂等）")
	ok(int(root_node.call("container_module_count")) == WIRED.size(),
		"二次装配后容器里仍是 %d 个（没有重复注册）" % WIRED.size())
	ok(screens_control.get_child_count() == 15, "二次装配后屏数仍是 15")

	# ------------------------------------------------------------------
	# 6) 回收：整棵树 queue_free，子节点跟着走（`Gd<Node>` 不保活）
	# ------------------------------------------------------------------
	var plane_id: int = plane.get_instance_id()
	var drawer_id: int = drawer.get_instance_id()
	root_node.queue_free()
	await process_frame
	await process_frame
	ok(not is_instance_valid(root_node), "GraphwarRootNode 被 queue_free 回收")
	ok(not is_instance_valid(plane), "棋盘随父节点一起被回收")
	ok(not is_instance_valid(drawer), "EntityDrawer 随父节点一起被回收")
	ok(not is_instance_valid(setup_screen), "Setup 屏随父节点一起被回收")
	print("      回收前的 instance_id: plane=%d drawer=%d" % [plane_id, drawer_id])

	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("ASSEMBLE %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
