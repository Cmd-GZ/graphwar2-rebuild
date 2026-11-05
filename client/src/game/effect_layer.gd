extends Control
class_name GwEffectLayer
## 命中/死亡特效层（`docs/143` §1）：把 `EffectInfo` 那条协议事件**画出来**。
##
## 这一层只做"表现"，不参与判定：谁死谁活由 `PlayScreen._resolve_shots()` 决定，
## 本层只接收 `(effect_type, 棋盘坐标)` 并计时、画。**放在 `client/src/game/`** 是为了
## 不碰 `plane_view.gd` 与 `fire_fx.gd`（后者的准备圈/弹道由另一位 agent 负责）。
##
## ## 哪些是证据、哪些是自定
##
## | 项 | 档位 | 依据 |
## |---|---|---|
## | 效果**种类**只有 4 个：`ExplosionEffect` / `DeathEffect` / `SparkEffect` / `AshEffect` | **观测** | `docs/128` §3.2 / `docs/135` §155：`.rodata` 字符串池 |
## | `ExplosionEffect` 的**位置** = 弹道停止点、**时机** = 弹道执行时 | **逐位 + 观测** | `docs/31` §2/§3（`tests/live_hits.rs` 逐字节）、`docs/43` §1 的报文顺序（`FunctionActive` 之后紧跟 `EffectInfo(ExplosionEffect)`）；夹具 `fixtures/protocol_death.jsonl` 的坐标在 ±25 = 平面边界 ⇒ 就是"弹道飞完的地方" |
## | `DeathEffect` 的**位置** = 死者**站位**（不是弹道停点） | **逐位** | `docs/31` §3（`DeathEffect{230, -10.452779035793856, -2.651563472451839}` 与抓包逐位相同）、`docs/65` §1 |
## | `DeathEffect.start_tick` = `LifeQueuedDeath.tick`（死亡在**弹道到达该士兵的那个 tick**才落地） | **观测** | `docs/65` §1（两例对齐：238/238、953/953） |
## | 每种效果**怎么画**（半径/时长/颜色/粒子数） | **自定** | 原版是 Rust `entity_drawer` 画的，**没有**任何像素或二进制证据能给出这些常数（`docs/135` §155 已记"特效只有名字"） |
## | `AshEffect` 是"死亡之后的灰烬"（比 `DeathEffect` 长一点、更暗） | **推断** | `docs/86` §264：152 个死亡里每人**恰好 1 个** `DeathEffect`/`AshEffect`，坐标都是站位；哪一个对应什么时机未观测 ⇒ 本层把 Death 当"爆一下"、Ash 当"留下的灰"，**顺序是我们的选择** |
##
## 本层**不做**像素保真（`docs/121` §7 的老账），也不声称与原版同形。

## 4 个效果名（`docs/128` §3.2 的 `.rodata` 顺序，**观测**）。
## 顺序同时是协议枚举的判别值顺序（`0x4bf090` 的跳表索引 0..3，**逐位**见 `docs/143` §4.1）。
const KINDS := ["ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"]

## 每种效果的存活时长（秒）—— **自定**。
const LIFETIME := {
	"ExplosionEffect": 0.55,
	"DeathEffect": 0.45,
	"SparkEffect": 0.30,
	"AshEffect": 0.90,
}

## 半径（棋盘单位）—— **自定**。爆炸最大（子弹炸开），死亡中等，火花最小。
const RADIUS_UNITS := {
	"ExplosionEffect": 1.6,
	"DeathEffect": 1.1,
	"SparkEffect": 0.7,
	"AshEffect": 0.9,
}

## 颜色 —— **自定**（原版调色板未观测；取 `docs/102` 队伍色之外的暖色）。
const COLORS := {
	"ExplosionEffect": Color(1.0, 0.62, 0.10),
	"DeathEffect": Color(0.85, 0.22, 0.18),
	"SparkEffect": Color(1.0, 0.94, 0.55),
	"AshEffect": Color(0.42, 0.42, 0.46),
}

## 要画在哪块棋盘上（坐标换算用它的 `board_to_screen()`）。可以为 `null`：
## 那时本层只计时、什么都不画（自检/无棋盘时）。
var plane: GwPlaneView = null

## 当前活着的特效：`{kind, pos, age, lifetime, radius, color}`。
var _fx: Array = []

## 累计**生成过**多少条（自检看"发没发"，与"现在还剩几条"是两件事）。
var _spawned := 0
## 每个种类各生成过多少条。
var _spawned_by_kind := {}

func _ready() -> void:
	name = "EffectLayer"
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_anchors_preset(Control.PRESET_FULL_RECT)
	set_process(true)

func _process(delta: float) -> void:
	advance_seconds(delta)

## 时间唯一入口：`_process` 传真实帧间隔，自检传固定步长（可复现）。
func advance_seconds(delta: float) -> void:
	if delta <= 0.0:
		return
	var keep: Array = []
	var changed := false
	for e in _fx:
		e["age"] = float(e["age"]) + delta
		if float(e["age"]) < float(e["lifetime"]):
			keep.append(e)
		else:
			changed = true
	if changed or keep.size() != _fx.size():
		_fx = keep
		queue_redraw()

## 生成一条特效。`kind` 必须是 [`KINDS`] 之一（否则只记日志不画）。
## 返回那条记录的副本（自检用）。
func spawn(kind: String, pos: Vector2, _opts: Dictionary = {}) -> Dictionary:
	if not KINDS.has(kind):
		return {}
	var e := {
		"kind": kind,
		"pos": pos,
		"age": 0.0,
		"lifetime": float(LIFETIME.get(kind, 0.4)),
		"radius": float(RADIUS_UNITS.get(kind, 1.0)),
		"color": COLORS.get(kind, Color.WHITE),
	}
	_fx.append(e)
	_spawned += 1
	_spawned_by_kind[kind] = int(_spawned_by_kind.get(kind, 0)) + 1
	queue_redraw()
	return e.duplicate()

## 清空（换局/退出对局时调）。
func clear() -> void:
	_fx.clear()
	_spawned = 0
	_spawned_by_kind.clear()
	queue_redraw()

# ---------------------------------------------------------------- 只读快照

## 当前还活着的特效（自检断言"发出来了、还在动"）。
func snapshot() -> Array:
	var out: Array = []
	for e in _fx:
		out.append({
			"kind": String(e["kind"]),
			"x": float(e["pos"].x), "y": float(e["pos"].y),
			"age": float(e["age"]), "lifetime": float(e["lifetime"]),
		})
	return out

func count() -> int:
	return _fx.size()

## 累计生成过的条数（按种类）。
func spawned() -> Dictionary:
	var d := {"total": _spawned}
	for k in KINDS:
		d[k] = int(_spawned_by_kind.get(k, 0))
	return d

func alive_of(kind: String) -> int:
	var n := 0
	for e in _fx:
		if String(e["kind"]) == kind:
			n += 1
	return n

# ---------------------------------------------------------------- 绘制

## 一个棋盘单位 = 多少画布像素（不依赖 `plane` 的私有方法：用两次换算的差）。
func _unit_px() -> float:
	if plane == null:
		return 1.0
	var a := plane.board_to_screen(Vector2.ZERO)
	var b := plane.board_to_screen(Vector2(1.0, 0.0))
	var d := b.distance_to(a)
	return d if d > 0.0 else 1.0

func _draw() -> void:
	if plane == null:
		return
	var unit := _unit_px()
	for e in _fx:
		var p: Vector2 = plane.board_to_screen(e["pos"])
		var t := clampf(float(e["age"]) / maxf(0.001, float(e["lifetime"])), 0.0, 1.0)
		var c: Color = e["color"]
		match String(e["kind"]):
			"ExplosionEffect":
				# 扩散的环 + 四条火舌（自定）。
				var r := float(e["radius"]) * unit * (0.25 + 0.75 * t)
				var a := 1.0 - t
				draw_arc(p, r, 0.0, TAU, 48, Color(c.r, c.g, c.b, a), maxf(1.0, unit * 0.18))
				for k in 4:
					var ang := TAU * float(k) / 4.0 + t * 0.8
					draw_line(p, p + Vector2(cos(ang), sin(ang)) * r * 1.5,
						Color(1.0, 0.85, 0.35, a), maxf(1.0, unit * 0.10))
			"DeathEffect":
				# 向内塌缩的实心圆（自定）：先在死者站位闪一下，再缩没。
				var r2 := float(e["radius"]) * unit * (1.0 - 0.7 * t)
				draw_circle(p, maxf(0.5, r2), Color(c.r, c.g, c.b, 1.0 - t))
			"SparkEffect":
				# 十字火花（自定）。
				var l := float(e["radius"]) * unit * (0.6 + 0.6 * t)
				var a3 := 1.0 - t
				draw_line(p - Vector2(l, 0), p + Vector2(l, 0), Color(c.r, c.g, c.b, a3),
					maxf(1.0, unit * 0.08))
				draw_line(p - Vector2(0, l), p + Vector2(0, l), Color(c.r, c.g, c.b, a3),
					maxf(1.0, unit * 0.08))
			"AshEffect":
				# 留下的灰：小方点，慢慢淡出（自定）。
				var s := maxf(1.0, float(e["radius"]) * unit * 0.35)
				draw_rect(Rect2(p - Vector2(s, s) * 0.5, Vector2(s, s)),
					Color(c.r, c.g, c.b, 0.75 * (1.0 - t)), true)
