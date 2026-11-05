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
## | 准备圈的**存在** | — | **观测** | `docs/140` §3：两张 92×87 的用户裁图（准备前 / 准备完毕）能逐像素量出环的内外径、环带宽度、外凸箭头的方位与尖端半径 |
## | 准备圈的半径 / 线宽 / 颜色 / 段数 / 张开态弧覆盖 / 张开态 alpha | 本文件的 `RING_*` 常数 | **观测**（逐像素测量） | `docs/140` §3：25.5 px = 1 棋盘单位（网格线间隔，与 `docs/125` 的 1920 窗口下 25.58 px/单位互证）；半径 19.21 px、带宽 6.72 px、外径 22.57 px、箭头尖端 31.0 px、箭头方位 60.5°/180.2°（相差 120°）、准备态弧覆盖 44%、准备态 `#7D6800` 精确色 934 px（**不透明**） |
## | 准备圈的颜色来源 | 该士兵的**玩家色** | **观测** | 真帧里环、外凸箭头、名字、弹道是同一个 `#7D6800` = `plane_view.gd::TEAM_COLORS[0][2]` |
## | 段数 | **3 段（120° 一节）**，不是 4 | **观测** | `docs/140` §3.4：准备态缺口在 0–15° / 75–135° / 195–255° / 315–357° 四段；闭合态箭头实测 2 个（60.5°/180.2°），第 3 个在 300° 被名字标签与弹道线压住（那一带形态学开运算后无实心块） |
## | 闭合时长 0.18 s / 线性插值 / 张开态回退 15° | 本文件里的常数 | 时长与插值**自定**；**回退 15°** 是**观测** | `docs/140` §5：静态裁图给不出时长；回退角由两组独立姿态差算出（43.9°→60.5°、164.5°→180.2°，两次都 ≈+15°，半径 27.8→27.3 px 不变） |
## | "准备完毕"的语义 | 函数非空（顺序回合）/ 本轮已开火（同时回合） | **推断**，但 `docs/140` §3.5 加了一条旁证：准备完毕那一帧里**同时出现了弹道预览线**（准备前那一帧没有），说明"闭合"与"函数已敲完、预览已出"是同一时刻 | 顺序回合：`TurnInfo{player_id, soldier_id}` 决定谁开火（`docs/46`）；同时回合：`docs/86` §6 的 `WaitingForFunctions` 语义 = 各自提交。仍**未观测**到"点了 Fire"这一步的像素 |
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

## 准备圈（`docs/140` §3：**几何全部是原版真帧的逐像素测量值**，不再是自定）：
## * 半径 **0.753 棋盘单位**（= 15.05 画布 px）：真帧环中心线 19.21 px ÷ 25.5 px/单位；
## * 线宽 **0.264 单位**（= 5.27 画布 px，整齐的 5 画布 px）：环外径 22.57 / 内径 15.85；
## * 张开态弧覆盖 **0.44**：真帧准备态实测三段各 52.8°，缺口 67.2°；
## * 张开态 alpha **1.0**：真帧准备态的环与闭合态**同样饱和**（`#7D6800` 精确值占 934 px），
##   没有半透明 —— 原来那个 0.55 是凭感觉定的，用户说的"色调不对"有一半出在这里；
## * 颜色取该士兵的**玩家色** —— 真帧里环、箭头、名字、弹道是同一个 `#7D6800`
##   = `plane_view.gd` 的 `TEAM_COLORS[0][2]`（**观测**，不是自定）；
## * 闭合/张开过渡 **0.18 s 线性**：**仍是自定** —— 静态裁图拍不出时长，见 `docs/140` §5。
const RING_RADIUS_UNITS := 0.8576
const RING_WIDTH_UNITS := 0.3
## 张开/闭合态的半张角（度）：原版 `H` 从 π/6=30° 涨到封顶 1.07337748998 rad=61.5°。
const RING_HALF_DEG_MIN := 30.0
const RING_HALF_DEG_MAX := 61.5
## "整圈被盖住的比例" = 3 段 × 2H / 360°：张开 180/360 = 0.5、闭合 369/360 → 钳到 1.0。
## 自检用这个量；画的时候用的是 `half_deg`。
const RING_MIN_COVERAGE := 0.5
const RING_CLOSE_SECONDS := 0.18
const RING_OPEN_ALPHA := 1.0
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
## 已开火、正在等弹道飞完的圈（`docs/140` §5.2：**闭合 = 已发射**，不是"函数非空"）。
## 每项 `{index, pos, color}`；由 `_pin_on_fire()` 在 `play()` 里写入，
## 弹道走完（`shot_finished`）或本回合没人要开火时清空。
var _pinned: Array = []

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
	_pin_on_fire()          # 开火 ⇒ 当场那些圈闭合（docs/140 §5.2）

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
		_pinned = []        # 弹道飞完 ⇒ 闭合圈功成身退（下一帧 _sync_rings 会把它移出）
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
## **"准备完毕"= 已经开火，不是"输入框非空"**（用户实测，`docs/140` §5.2）。
## `state_provider`（`play_screen.gd::fire_fx_state()`，本轮不可改）只提供
## `ready = 函数非空`，那个判据是**错的**，所以这里**不再采信它**：
## 唯一的权威信号是本类自己的 `play()` —— 它在 `PlayScreen.fire()` 里被调用，
## 等价于"这一发已经提交/发射"。开火那一帧把当场在屏幕上的圈**钉住**（`_pinned`）、
## 推到闭合，并一直画到弹道飞完（`shot_finished`）为止。
##
## 已知边界（写在这里，别当成已解决）：同时回合里"每个玩家各自提交"的逐个闭合
## **检测不到** —— `_shot_owners()` 一提交就把该玩家从 `rings` 里拿掉，本类看不到
## "他提交了但还没结算"这个中间态。要修必须改 `fire_fx_state()` 的契约（`docs/140` §7）。
func _sync_rings(delta: float) -> void:
	var want: Array = _desired_rings()
	if want.is_empty():
		_pinned = []                        # 对局结束/本回合没人要开火 ⇒ 钉子一起清掉
	var step := 0.0
	if delta > 0.0:
		step = delta / RING_CLOSE_SECONDS
	var next: Array = []
	# ① 还没开火的圈：**一律开环**（progress 只会朝 0 走）
	for d in want:
		var idx := int(d["index"])
		if _pinned_index(idx) >= 0:
			continue                        # 这一位已经开过火，由下面的钉子负责
		var prev := _find_ring(idx)
		var progress := maxf(0.0, float(prev.get("progress", 0.0)) - step)
		next.append(_ring_entry(idx, d["pos"], d["color"], false, progress))
	# ② 已开火的圈：闭合并钉住（即使它已经不在 `want` 里 —— 换手了也还在飞）
	for p in _pinned:
		var prev := _find_ring(int(p["index"]))
		var progress := minf(1.0, float(prev.get("progress", 0.0)) + step)
		next.append(_ring_entry(int(p["index"]), p["pos"], p["color"], true, progress))
	_rings = next
	if plane != null:
		plane.set_ready_rings(_rings)

func _ring_entry(idx: int, pos: Vector2, color: Color, ready: bool, progress: float) -> Dictionary:
	var p := clampf(progress, 0.0, 1.0)
	var half := RING_HALF_DEG_MIN + (RING_HALF_DEG_MAX - RING_HALF_DEG_MIN) * p
	return {
		"index": idx,
		"pos": pos,
		"color": color,
		"ready": ready,
		"progress": p,
		"radius": RING_RADIUS_UNITS,
		"half_deg": half,
		# 3 段 × 2H / 360°；闭合态 3×123° > 360° ⇒ 钳到 1.0（整圈，且首尾有富余重叠）
		"coverage": minf(1.0, 3.0 * 2.0 * half / 360.0),
		"alpha": RING_OPEN_ALPHA + (RING_READY_ALPHA - RING_OPEN_ALPHA) * p,
	}

## 开火瞬间把当场在屏幕上的圈记下来（`docs/140` §5.2）。顺序回合里"当场那一个"
## 就是射手的圈，语义无歧义；同时回合里会有多个，这时只能全部钉住 —— 已知近似。
func _pin_on_fire() -> void:
	_pinned = []
	for r in _rings:
		_pinned.append({"index": int(r.get("index", -1)), "pos": r.get("pos", Vector2.ZERO),
			"color": r.get("color", Color(1, 1, 1))})

func _pinned_index(idx: int) -> int:
	for i in _pinned.size():
		if int(_pinned[i].get("index", -1)) == idx:
			return i
	return -1

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

## 已开火、正在等弹道飞完的圈（只读快照，自检用）。
func pinned_count() -> int:
	return _pinned.size()
