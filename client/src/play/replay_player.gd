extends RefCounted
class_name GwReplayPlayer
## 回放播放器：按 **tick** 推进、逐条消费事件、累加局面。
##
## ## 播放模型（**我们自定**；原版回放界面未观测，`docs/133` §4）
##
## * tick 时钟 = **60 tick/秒**（`docs/35` §2 是真值：`current_tick` 与墙钟严格线性，
##   113 个 `TickReply` 样本、110 个完全吻合）；
## * 每帧 `tick += dt × 60 × speed`，**余数留着**（不留就会 1× 每帧慢 1 tick）；
## * 只消费 `事件.tick <= 当前 tick` 的事件，且**按文件顺序** —— 不排序。
##   排序会把"同一 tick 内的批内顺序"打乱，而批内顺序本身有意义（`docs/113`）；
##   400 份真实回放里有 7 份 tick 会小幅回退，游标模型天然容忍（迟到的那条会在
##   一次推进里被一起消费掉）；
## * 跳到任意 tick = **从头重放**（整局 1500 条事件，代价可忽略）。这样
##   "任意时刻的状态" 恒等于 `state_at(tick)`，不会出现增量重放的漂移。
##
## ## 局面规则（**推断**，口径 = `tools/replay_state.py`，逐项列在 `docs/133` §3）
##
## 只有那 15 类事件改局面，其余只进事件流（不猜副作用）。
## `canonical()` 与内核 `src/graphwar_replay_player.rs::ReplayState::canonical` **逐字符同格式**，
## 用于三端对拍（Python 参考 / Rust 内核 / 本文件）。

## tick 时钟（`docs/35` §2）。
const TICKS_PER_SECOND := 60

var file: GwReplayFile = null
## 当前局面。键与 `src/graphwar_replay_player.rs::ReplayState` 一一对应。
var state := {}
var cursor := 0
var tick := 0.0
var playing := false
var speed := 1.0
var first_tick := 0
var last_tick := 0

## 建立播放器（不自动开始播放）。
func setup(f: GwReplayFile) -> GwReplayPlayer:
	file = f
	var r := f.tick_range()
	first_tick = r.x
	last_tick = r.y
	tick = float(first_tick)
	_reset_state()
	cursor = 0
	state.tick = first_tick
	return self

func _reset_state() -> void:
	state = {
		"tick": 0,
		"obstacles": [],          # [{x,y,r}] 插入顺序
		"holes": [],              # DestroyObstacle 挖的洞（不移除障碍，docs/30）
		"pos": {},                # entity_id -> {x,y,r}
		"team": {},               # entity_id -> "Team1"/"Team2"
		"alive": {},              # entity_id -> bool（缺省视为活着）
		"functions": {},          # owner(=soldier_id) -> 最近一次非空函数文本
		"corrected": {},          # owner -> corrected_function（"__null__" = JSON null）
		"graphics": {},           # "skin:<id>" / "face:<id>" / "hat:<id>" -> 名字
		"colors": {},             # player_id -> Color
		"targets": {},            # 靶子 id（全局表）
		"removed": {},            # EntityRemoved
		"game_state": "",
		"game_name": "",
		"axis_mode": "",
		"function_mode": "",
		"turn_mode": "",
		"time_mode": "",
		"turn_count": 0,
		"shooter": -1,
		"active_tick": -1,
		"effects": [],            # [{tick,id,pos,effect_type,start_tick}]
		"shots": [],              # [{tick,owner,start,end_x,function,corrected}]
	}
	if file != null:
		for t in file.targets.keys():
			state.targets[int(t)] = true

# ---------------------------------------------------------------- 推进

## 当前整 tick。
func current_tick() -> int:
	return int(floor(tick))

## 进度 0..1。
func progress() -> float:
	var span := maxi(1, last_tick - first_tick)
	return clampf((tick - float(first_tick)) / float(span), 0.0, 1.0)

func finished() -> bool:
	return cursor >= (file.events.size() if file != null else 0) and tick >= float(last_tick)

## 播放 / 暂停 / 取反。放完再按播放 = 从头重放（我们自定的交互）。
func play() -> void:
	if finished():
		seek(first_tick)
	playing = true

func pause() -> void:
	playing = false

func toggle() -> void:
	if playing:
		pause()
	else:
		play()

## 按墙钟时间推进一帧；返回本帧消费掉的事件（按消费顺序）。
func advance_dt(dt: float) -> Array[Dictionary]:
	if not playing:
		return []
	var d := clampf(dt, 0.0, 1.0)      # 掉帧时不跳一大截
	tick += d * float(TICKS_PER_SECOND) * maxf(0.0, speed)
	if tick >= float(last_tick):
		tick = float(last_tick)
		playing = false
	return advance_to(current_tick())

## 消费所有 `tick <= 目标` 的事件（文件顺序）。
func advance_to(target: int) -> Array[Dictionary]:
	var applied: Array[Dictionary] = []
	if file == null:
		return applied
	while cursor < file.events.size() and int(file.events[cursor].tick) <= target:
		var ev: Dictionary = file.events[cursor]
		apply_event(ev)
		applied.append(ev)
		cursor += 1
	tick = float(target)
	state.tick = target
	return applied

## 跳到某个 tick：**从头重放**（保证与内核 `state_at` 一致）。
func seek(target: int) -> void:
	var t := clampi(target, first_tick, last_tick)
	_reset_state()
	cursor = 0
	tick = float(t)
	state.tick = t
	if file == null:
		return
	while cursor < file.events.size() and int(file.events[cursor].tick) <= t:
		apply_event(file.events[cursor])
		cursor += 1
	state.tick = t

## 「下一个事件」：跳到下一条尚未消费事件的 tick。没有则返回 -1。
func step_event() -> int:
	if file == null or cursor >= file.events.size():
		return -1
	var t := int(file.events[cursor].tick)
	seek(t)
	return t

## 「上一个事件」：跳到严格早于当前 tick 的最近一条事件的 tick。没有则返回 -1。
func step_back_event() -> int:
	if file == null:
		return -1
	var now := current_tick()
	var best := -1
	for ev in file.events:
		var t := int(ev.tick)
		if t < now:
			best = maxi(best, t)
	if best < 0:
		return -1
	seek(best)
	return best

# ---------------------------------------------------------------- 局面累加

## 应用一条事件到局面。规则表见 `docs/133` §3（与 `src/graphwar_replay_player.rs::ReplayState::apply` 同序）。
func apply_event(ev: Dictionary) -> void:
	var d: Dictionary = ev.data
	match String(ev.kind):
		"ClearObstacles":
			state.obstacles = []
		"AddObstacle":
			if d.has("pos_x") and d.has("pos_y") and d.has("radius"):
				state.obstacles.append({
					"x": float(d.pos_x), "y": float(d.pos_y), "r": float(d.radius)})
		"DestroyObstacle":
			if d.has("pos_x") and d.has("pos_y"):
				(state.holes as Array).append({
					"x": float(d.pos_x), "y": float(d.pos_y),
					"r": float(d.get("radius", 0.5))})
		"PosInfo":
			if d.has("entity_id") and d.has("pos_x") and d.has("pos_y"):
				state.pos[int(d.entity_id)] = {
					"x": float(d.pos_x), "y": float(d.pos_y),
					"r": float(d.get("radius", 0.5))}
		"TeamInfo":
			if d.has("entity_id") and d.has("team"):
				state.team[int(d.entity_id)] = String(d.team)
		"LifeInfo":
			if d.has("entity_id") and d.has("alive"):
				state.alive[int(d.entity_id)] = bool(d.alive)
		"FunctionInfo":
			if d.has("owner_id"):
				var owner := int(d.owner_id)
				var f := String(d.get("function", ""))
				if f != "":
					state.functions[owner] = f
				if d.get("corrected_function") == null:
					state.corrected[owner] = "__null__"
				else:
					state.corrected[owner] = String(d.corrected_function)
		"FunctionPoints":
			if d.has("owner_id") and d.has("start_x") and d.has("start_y") and d.has("end_x"):
				var owner := int(d.owner_id)
				var corrected := String(state.corrected.get(owner, ""))
				(state.shots as Array).append({
					"tick": int(ev.tick),
					"owner": owner,
					"start": Vector2(float(d.start_x), float(d.start_y)),
					"end_x": float(d.end_x),
					"function": String(state.functions.get(owner, "")),
					"corrected": "" if corrected == "__null__" else corrected,
				})
		"FunctionActive":
			if d.has("active_tick"):
				state.active_tick = int(d.active_tick)
		"EffectInfo":
			if d.has("effect_id") and d.has("pos_x") and d.has("pos_y"):
				(state.effects as Array).append({
					"tick": int(ev.tick),
					"id": int(d.effect_id),
					"pos": Vector2(float(d.pos_x), float(d.pos_y)),
					"effect_type": String(d.get("effect_type", "")),
					"start_tick": int(d.get("start_tick", 0)),
				})
		"EntityRemoved":
			if d.has("entity_id"):
				state.removed[int(d.entity_id)] = true
		"GameStateInfo":
			state.game_state = String(d.get("game_state", ""))
		"GameNameInfo":
			state.game_name = String(d.get("game_name", ""))
		"AxisModeInfo":
			state.axis_mode = String(d.get("axis_mode", ""))
		"FunctionModeInfo":
			state.function_mode = String(d.get("function_mode", ""))
		"TurnModeInfo":
			state.turn_mode = String(d.get("turn_mode", ""))
		"TimeModeInfo":
			state.time_mode = String(d.get("time_mode", ""))
		"TurnCountInfo":
			if d.has("turn_count"):
				state.turn_count = int(d.turn_count)
		"TurnInfo":
			if d.has("soldier_id"):
				state.shooter = int(d.soldier_id)
		"SkinGraphicsInfo", "FaceGraphicsInfo", "HatGraphicsInfo":
			if d.has("entity_id") and d.has("graphics_str"):
				var slot := "skin"
				if String(ev.kind) == "FaceGraphicsInfo":
					slot = "face"
				elif String(ev.kind) == "HatGraphicsInfo":
					slot = "hat"
				state.graphics["%s:%d" % [slot, int(d.entity_id)]] = String(d.graphics_str)
		"ColorInfo":
			if d.has("entity_id") and d.has("color"):
				var c: Dictionary = d.color
				state.colors[int(d.entity_id)] = Color8(
					int(c.get("r", 0)), int(c.get("g", 0)), int(c.get("b", 0)))
		_:
			pass

# ---------------------------------------------------------------- 四个模式

## 回放里四个模式事件的**字符串 → `GwModes` 档位下标**。
##
## **观测**：400 份真实回放里这几个字段的全部取值（括号内是出现次数，`docs/133` §3.3）：
##   `AxisModeInfo`     EveryUnit(615) / EveryFive(34) / OnlyMain(54) / NoAxis(144)
##   `FunctionModeInfo` NormalFunction(548) / DiffEqFunction(320)
##   `TurnModeInfo`     SequentialTurns(658) / SimultaneousTurns(168)
##   `TimeModeInfo`     Timer1m(342) / TimerInf(470) / Timer3m(28) / Timer5m(22)
##                      / Timer30s(11) / Timer2m(191)
## 下标与 `GwModes` 的枚举顺序一致；那个顺序本身来自 `docs/92` §6 的连点观测。
## `AxisModeInfo` 的四个名字与 `docs/118` §7.1 从 21 关描述反推出的
## "no axis / main axes only / every 5 units / every unit" 一一对上（3 = EveryUnit）。
static func axis_index(s: String) -> int:
	match s:
		"EveryUnit": return 3
		"EveryFive": return 2
		"OnlyMain": return 1
		"NoAxis": return 0
	return 3

static func function_index(s: String) -> int:
	return 1 if s == "DiffEqFunction" else 0

static func turn_index(s: String) -> int:
	return 1 if s == "SimultaneousTurns" else 0

static func time_index(s: String) -> int:
	match s:
		"Timer30s": return 0
		"Timer1m": return 1
		"Timer2m": return 2
		"Timer3m": return 3
		"Timer5m": return 4
		"TimerInf": return 5
	return 5

## 当前局面的四个模式档位（给 `play_screen` 同步 `state.*` 用）。
func mode_indices() -> Dictionary:
	return {
		"axis": axis_index(String(state.axis_mode)),
		"function": function_index(String(state.function_mode)),
		"turn": turn_index(String(state.turn_mode)),
		"time": time_index(String(state.time_mode)),
	}

# ---------------------------------------------------------------- 查询

## `Team1` → 0、`Team2` → 1、未知 → -1（`docs/105`：队伍决定射击方向，不靠几何）。
func team_index(entity_id: int) -> int:
	match String(state.team.get(entity_id, "")):
		"Team1": return 0
		"Team2": return 1
	return -1

## 有名有位的实体：`[{id, pos, alive, is_target, team, name, color, player_id, removed}]`。
##
## 只包含"有 PosInfo 且没被 EntityRemoved"的实体 —— 没有坐标的东西画不出来。
func actors() -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var ids := _entity_ids()
	for id in ids:
		if state.pos.has(id) == false:
			continue
		if state.removed.has(id):
			continue
		var p: Dictionary = state.pos[id]
		var pid := int(file.soldier_owner.get(id, -1)) if file != null else -1
		var nm := String(file.names.get(pid, "")) if file != null else ""
		out.append({
			"id": id,
			"pos": Vector2(float(p.x), float(p.y)),
			"radius": float(p.get("r", 0.5)),
			"alive": bool(state.alive.get(id, true)),
			"is_target": bool(state.targets.has(id)),
			"team": team_index(id),
			"team_name": String(state.team.get(id, "?")),
			"name": nm,
			"player_id": pid,
			"color": state.colors.get(pid, null),
		})
	return out

## 局面里出现过的实体 id（位置 ∪ 队伍 ∪ 生死），升序。
func _entity_ids() -> Array:
	var seen := {}
	for k in state.pos.keys():
		seen[int(k)] = true
	for k in state.team.keys():
		seen[int(k)] = true
	for k in state.alive.keys():
		seen[int(k)] = true
	var ids := seen.keys()
	ids.sort()
	return ids

## 障碍（`{pos: Vector2, size: 半径}`）—— 直接就是 `GwPlaneView.set_obstacles()` 的入参形状。
func obstacles_for_view() -> Array:
	var out := []
	for o in state.obstacles:
		out.append({"pos": Vector2(float(o.x), float(o.y)), "size": float(o.r)})
	return out

## 靶子（活着且没被移除）的位置。
func target_positions() -> Array:
	var out := []
	for a in actors():
		if bool(a.is_target) and bool(a.alive):
			out.append(a.pos)
	return out

# ---------------------------------------------------------------- 对拍

## 规范性快照串 —— 与内核 `ReplayState::canonical()` **逐字符同格式**：
##
## ```text
## tick=T|O=x,y,r;…|E=id,team,alive,x,y,r;…|F=owner=text;…
## ```
## 数字一律 `%.6f`；`team` 未知写 `?`；`alive` 写 `0/1`；没有位置的实体留空三项。
func canonical() -> String:
	var obstacles := ""
	var first := true
	for o in state.obstacles:
		if not first:
			obstacles += ";"
		first = false
		obstacles += "%.6f,%.6f,%.6f" % [float(o.x), float(o.y), float(o.r)]
	var ent := ""
	first = true
	for id in _entity_ids():
		if not first:
			ent += ";"
		first = false
		var team := String(state.team.get(id, "?"))
		var alive := 1 if bool(state.alive.get(id, true)) else 0
		if state.pos.has(id):
			var p: Dictionary = state.pos[id]
			ent += "%d,%s,%d,%.6f,%.6f,%.6f" % [id, team, alive, float(p.x), float(p.y), float(p.get("r", 0.5))]
		else:
			ent += "%d,%s,%d,,," % [id, team, alive]
	var funcs := ""
	first = true
	var owners: Array = state.functions.keys()
	owners.sort()
	for owner in owners:
		if not first:
			funcs += ";"
		first = false
		funcs += "%d=%s" % [int(owner), String(state.functions[owner])]
	return "tick=%d|O=%s|E=%s|F=%s" % [int(state.tick), obstacles, ent, funcs]

## 播放状态摘要（自检 / 状态行用）。
func summary() -> String:
	var events := file.events.size() if file != null else 0
	return "tick %d/%d ／ 事件 %d/%d ／ %s%s" % [
		current_tick(), last_tick, cursor, events,
		"播放中" if playing else "暂停",
		"" if speed == 1.0 else (" ×%.1f" % speed),
	]
