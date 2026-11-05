extends Node
class_name GwFireFx
## 开火表现（`docs/135`）：② 准备发射的士兵有光圈、准备完毕光圈闭合；③ 弹道按 tick 逐点推进。
##
## 本类只做两件事：**推进状态** + 把结果推给 `GwPlaneView`（棋盘上的东西都由它画，
## `docs/126` §4.2 的分工不变）。它不碰结算、不碰 `shot.points`、不碰回合。
##
## ## 证据（哪些是观测、哪些是自定）
##
## | 量 | 值 | 档位 | 依据 |
## |---|---|---|---|
## | tick 频率 | **60 ticks/秒** | **观测** | `docs/35` §2：113 个 `TickReply` 样本拟合出 `current_tick = base + round((now_ms−base_ms)*60/1000)`，110/113 完全吻合，多点线性拟合 59.90~60.00 |
## | 每 tick 沿弹道走多远 | **0.29 棋盘单位** | **推断** | `docs/86` §6：用 `FunctionActive.active_tick`（起飞）与 `EffectInfo(ExplosionEffect).start_tick`（到达）之差 ÷ 弹道弧长，82 发里 75 发一对一配上，速度中位 **0.29**、p10~p90 = 0.255~0.300。`docs/86` §7 明写"弹丸按恒定速度沿弹道飞"是**推断**：缺逐 tick 位置的直接观测 |
## | 弹道是"逐 tick 揭示"而不是一次画完 | — | **推断** | 同上；协议本身按 tick 建模（`FunctionActive{active_tick}` / `EffectInfo{start_tick}` / `QueueDestroyObstacle{tick}`，`docs/46`/`docs/43`） |
## | 光圈的**存在** | — | **用户口述** | `docs/126` §6.3 第 2 条：用户实测原版有、我们没有任何像素/二进制证据 |
## | 光圈半径/颜色/线宽/闭合时长/插值 | 本文件里的常数 | **自定** | 同上；`.so` 的特效枚举只有 `DeathEffect` / `SparkEffect` / `AshEffect`（字符串池），**没有**"光圈"这种 `EffectInfo` ⇒ 它不是协议事件，只能是客户端自己画的；怎么画没有证据 |
## | "准备完毕"的语义 | 函数非空（顺序回合）/ 本轮已开火（同时回合） | **推断** | 顺序回合：`TurnInfo{player_id, soldier_id}` 决定谁开火（`docs/46`）；同时回合：`docs/86` §6 的 `WaitingForFunctions` 语义 = 各自提交。原版"闭合成整圈"到底对应"函数非空"还是"点了 Fire"**未观测** |
##
## ## 为什么推的是"揭示前缀"而不是"画一条新线"
##
## `shot.points` 保留**全量**点序列、撞点单独记在 `drawn.impact`（`docs/126` §3.1 留的口）。
## 本类只把"当前该画到第几个点"（`GwPlaneView.path_reveal`）往上顶；
## 一次画完与回合结束之间**没有**耦合（见 `shot_finished` 信号与 `docs/135` §3.4）。

## tick 频率（**观测**，`docs/35`）。
const TICKS_PER_SECOND := 60.0

## 每 tick 前进的弧长（棋盘单位）（**推断**，`docs/86` §6 的中位数）。
const SPEED_UNITS_PER_TICK := 0.29

## 光圈（全部**自定**，见文件头）：
## 半径 1.15 棋盘单位 —— 比士兵半径（`docs/120`：`PosInfo.radius` 恒 0.5）的 2 倍还大一圈，
## 这样圆环正好落在士兵**贴图方块**（2.15 单位宽，见 `plane_view.gd` 的 `SPRITE_CONTENT`）之外，
## 不会被精灵的透明边压住；
## 线宽 2 "画布像素"（与粗网格同宽，`plane_view.gd` 的 `_canvas_px()*2`）；
## 未准备时先露出 30% 的弧（看得见、但明显没合上），准备好后闭合成整圈；
## 闭合/张开过渡 0.18 s；颜色取该士兵的**玩家色**（`docs/102` §1.1 的调色板）。
const RING_RADIUS_UNITS := 1.15
const RING_WIDTH_CANVAS_PX := 2.0
const RING_MIN_COVERAGE := 0.30
const RING_CLOSE_SECONDS := 0.18
const RING_OPEN_ALPHA := 0.55
const RING_READY_ALPHA := 1.0

## 弹道走完（到达 `drawn.impact`）时发一次。参数 = 实际用掉的 tick 数。
##
## **这是留给"到点才结算"的接口**：现在结算仍在 `PlayScreen.fire()` 里同步做完
## （`_commit_shot`），动画只是视觉层；要改成"飞到才结算"，把这个信号接到
## `_commit_shot(shot)` 上即可（`docs/135` §3.4 写了为什么这一轮没改）。
signal shot_finished(ticks: float)

## 要画到哪块棋盘上。为 `null` 时本类只推进状态、什么都不画。
var plane: GwPlaneView = null

## 每帧拉一次的**只读**对局状态快照，由 `PlayScreen.fire_fx_state()` 提供：
## `{running: bool, rings: [{index, pos, color, ready}]}`。
var state_provider: Callable = Callable()

var _points := PackedVector2Array()
## 每个点的累计弧长（棋盘单位）。用 **float64** 存：float32 在弧长 50 附近的量化误差
## 约 4e-6，比下面判"最后一个点算不算到了"的 1e-6 容差还大，会把末点漏掉（实测 4586/4587）。
var _cum := PackedFloat64Array()
var _total_arc := 0.0
var _arc := 0.0                      ## 已揭示到的弧长
var _ticks := 0.0                    ## 本发已经过的 tick
var _reveal := 0                     ## 已揭示的点数（单调不减）
var _playing := false
var _finished := false
var _rings: Array = []               ## 当前光圈状态 [{index, pos, color, progress, ready}]

func _ready() -> void:
	name = "FireFx"
	set_process(true)

func _process(delta: float) -> void:
	advance_seconds(delta)

# ---------------------------------------------------------------- ③ tick 推进

## 开一发新的弹道动画。`points` 必须是**全量**点序列（`shot.points`），
## 不是截断到撞点的那一段——撞点是这条序列的最后一点（`drawn.points`）。
func play(points: PackedVector2Array) -> void:
	_points = points
	_cum = PackedFloat64Array()
	_cum.resize(points.size())
	var arc := 0.0
	for i in points.size():
		if i > 0:
			arc += points[i].distance_to(points[i - 1])
		_cum[i] = arc
	_total_arc = arc
	_arc = 0.0
	_ticks = 0.0
	_reveal = mini(1, points.size())
	_finished = false
	# 少于 2 个点 / 零弧长 ⇒ 没有可推进的东西，直接算"已到达"。
	_playing = points.size() >= 2 and _total_arc > 0.0
	if not _playing:
		_finished = true
	_push_reveal()

## 时间是唯一输入：`_process` 传真实帧间隔；自检传固定步长（可复现）。
## 一次调用同时推进"弹道"与"光圈动画"，两者互不影响。
func advance_seconds(delta: float) -> void:
	if delta > 0.0:
		_advance_shot(delta * TICKS_PER_SECOND)
	_sync_rings(delta)

## 按 tick 推进（`ticks` 可以是小数：一帧 = delta × 60 tick）。
func _advance_shot(ticks: float) -> void:
	if not _playing:
		return
	_ticks += ticks
	_arc = minf(_total_arc, _arc + ticks * SPEED_UNITS_PER_TICK)
	if _arc >= _total_arc - 0.000001:
		# 到达末端 = 到达 `drawn.impact`（撞点就是序列最后一点）。
		_arc = _total_arc
		_playing = false
		_finished = true
		_push_reveal(true)
		shot_finished.emit(_ticks)
	else:
		_push_reveal()

## 把"画到第几个点"推给棋盘。**只增不减**（单调），且不动 `path_points`。
## `force_all` 只在"已经走到末端"时用：末点的累计弧长是浮点累加出来的，
## 与其指望容差，不如在收尾时直接给全量。
func _push_reveal(force_all: bool = false) -> void:
	if force_all:
		_reveal = _points.size()
	else:
		var n := _reveal
		while n < _points.size() and _cum[n] <= _arc + 0.000001:
			n += 1
		_reveal = n
	if plane != null and _points.size() > 1:
		plane.set_path_reveal(_reveal)

## 已经揭示的点数。
func reveal_count() -> int:
	return _reveal

## 只读快照（自检用；也方便诊断）。
func shot_state() -> Dictionary:
	return {
		"playing": _playing,
		"finished": _finished,
		"ticks": _ticks,
		"arc": _arc,
		"total_arc": _total_arc,
		"reveal": _reveal,
		"total_points": _points.size(),
		"speed_units_per_tick": SPEED_UNITS_PER_TICK,
		"ticks_per_second": TICKS_PER_SECOND,
	}

# ---------------------------------------------------------------- ② 准备光圈

## 按 `state_provider` 的目标状态推进光圈，并推给棋盘。
##
## 目标状态只决定"朝 0 还是朝 1 走"；`progress` 是**动画量**，所以这一帧拉一次就够了。
## 新出现的光圈从 0（开环）开始，消失的直接移除。
func _sync_rings(delta: float) -> void:
	var want: Array = _desired_rings()
	var step := 0.0
	if delta > 0.0:
		step = delta / RING_CLOSE_SECONDS
	var next: Array = []
	for d in want:
		var prev := _find_ring(int(d["index"]))
		var progress := float(prev.get("progress", 0.0))
		var target := 1.0 if bool(d["ready"]) else 0.0
		if progress < target:
			progress = minf(target, progress + step)
		elif progress > target:
			progress = maxf(target, progress - step)
		next.append({
			"index": int(d["index"]),
			"pos": d["pos"],
			"color": d["color"],
			"ready": bool(d["ready"]),
			"progress": progress,
			"radius": RING_RADIUS_UNITS,
			"coverage": RING_MIN_COVERAGE + (1.0 - RING_MIN_COVERAGE) * progress,
			"alpha": RING_OPEN_ALPHA + (RING_READY_ALPHA - RING_OPEN_ALPHA) * progress,
		})
	_rings = next
	if plane != null:
		plane.set_ready_rings(_rings)

func _desired_rings() -> Array:
	if state_provider.is_valid():
		var state: Dictionary = state_provider.call()
		if bool(state.get("running", false)):
			return state.get("rings", [])
	return []

func _find_ring(index: int) -> Dictionary:
	for r in _rings:
		if int(r.get("index", -1)) == index:
			return r
	return {}

## 当前光圈（只读快照，自检用）。棋盘上那份是 `GwPlaneView.ready_rings`。
func ring_state() -> Array:
	return _rings.duplicate(true)
