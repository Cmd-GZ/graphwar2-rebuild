extends Control
class_name GwEffectLayer
## 命中/死亡特效的**适配层**（`docs/16` §48.3）：把 `EffectInfo` 那四种名字派给**真**
## `EffectDrawer` 节点（`src/_scaffold/effect_drawer_class.rs`），自己**一个图元都不画**。
##
## 旧版本（本轮之前）是 GDScript 自绘：靠 `plane.board_to_screen()` 把棋盘坐标换成像素，
## 然后 `draw_arc` / `draw_circle` / `draw_line` 手画四种形状。换成真节点之后那段坐标映射
## **整段删掉** —— 因为 `EffectDrawer` 自己就会 `draw_set_transform(size/2, 0, (size.x/50, ∓))`
## （`effect_drawer::draw_transform`，逐位），棋盘坐标进去就是对的像素。
##
## ## 证据与自定（逐条，别当成已复原）
##
## | 项 | 档位 | 依据 |
## |---|---|---|
## | 效果**种类**只有 4 个：`ExplosionEffect` / `DeathEffect` / `SparkEffect` / `AshEffect` | **观测** | `docs/128` §3.2 / `docs/135` §155：`.rodata` 字符串池 |
## | 四种效果**怎么画** | **观测**（现在是真的） | `effect_drawer::frame_primitives`：`draw_set_transform` ×1 + `draw_circle` / `draw_polygon` / `draw_line`，半径 0.05/0.06、四组基色、`max(0,(1−t)·0.9)` 全是 `.so` 里的立即数（§34） |
## | **种类名 → 相位字节**的对应 | **我方选择** | 原版是 `EffectInfo` 的判别值（0..3 的跳表，`0x4bf090`）进 `EffectData`，而 `Phase` 有 **7** 档（`effect_drawer::Phase`）——两者是不是同一个编号**没有证据**。下表是我方按语义挑的，改一行就能换 |
## | 时长（tick） | **我方选择** | 旧版本是自定秒数 × 60；原版 `EffectInfo` 只带 `start_tick`，持续多久取决于那个被 `retain` 的判据（`effect_drawer` 模块文档：判据**推断**） |
## | 多边形顶点 / 线段端点 | **未知** | `effect_drawer` 模块文档 §5：顶点公式未复原、`from == to` ⇒ 这两类图元在画面上是退化/被跳过的 |
##
## 旧版本里那套"半径/颜色/时长"常数（`LIFETIME` / `RADIUS_UNITS` / `COLORS`）**全部作废**：
## 它们是自定的，现在画出来的东西由 Rust 侧的立即数决定。
##
## ## 这个类还剩什么
##
## ① 按种类记账（`spawned()` / `alive_of()`，自检与 `level_state()` 用）；
## ② 把名字映射成 `EffectDrawer.spawn_effect(x, y, phase, duration_ticks)`；
## ③ 每帧 `retain_alive()` —— 原版那个 `Vec::retain` 的判据是**推断**，所以节点类故意
##    没在它自己的 `_process` 里跑（见 `effect_drawer_class.rs` 的 `retain_alive` 文档），
##    由调用方（这里）显式跑。

## 4 个效果名（`docs/128` §3.2 的 `.rodata` 顺序，**观测**）。
## 顺序同时是协议枚举的判别值顺序（`0x4bf090` 的跳表索引 0..3，**逐位**见 `docs/143` §4.1）。
const KINDS := ["ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"]

## 种类 → `EffectDrawer` 的相位字节（`effect_drawer::Phase`）。**我方选择**，见文件头。
##
## | 名字 | 相位 | 那个相位画什么（`effect_drawer::Phase`） |
## |---|---|---|
## | `ExplosionEffect` | 2 = `CircleLargeA` | 大圆 + `(0.7,0.7,0)` —— 爆炸最像它 |
## | `DeathEffect` | 6 = `CircleDark` | 大圆 + `(0.1,0.1,0.1)`、alpha 固定 0.7 —— "死亡的暗圈" |
## | `SparkEffect` | 0 = `CircleSmall` | 小圆 + `(0.6,0.6,0.1)` |
## | `AshEffect` | 4 = `LineA` | 线段 + `(0.9,0.9,0.1)`（端点未复原 ⇒ 退化线段） |
const PHASE_FOR_KIND := {
	"ExplosionEffect": 2,
	"DeathEffect": 6,
	"SparkEffect": 0,
	"AshEffect": 4,
}

## 每种效果活多少 tick —— **我方选择**（旧版本是自定秒数，这里 ×60 换算过来，
## 好让"多久消失"与旧版本一致；原版的时长没有证据，见文件头）。
const LIFETIME_TICKS := {
	"ExplosionEffect": 33,
	"DeathEffect": 27,
	"SparkEffect": 18,
	"AshEffect": 54,
}

## 真 `EffectDrawer` 节点（`bind()` 推进来；`null` = 扩展没加载 / 还没 bind）。
var drawer: Control = null

## 累计**生成过**多少条（自检看"发没发"，与"现在还剩几条"是两件事）。
var _spawned := 0
## 每个种类各生成过多少条。
var _spawned_by_kind := {}
## 已生成但还没被 `retain_alive()` 清掉的条数（诊断用；真值以 `drawer.effect_count()` 为准）。
var _pending := 0

func _ready() -> void:
	name = "EffectLayer"
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_anchors_preset(Control.PRESET_FULL_RECT)
	set_process(true)

## 绑定真节点（`PlayScreen.build()` 里调一次）。绑不上时本层退化成"只记账"。
func bind(d: Control) -> void:
	drawer = d

func _process(_delta: float) -> void:
	# ① 推进在最右边那个节点里（它自己的 `_process` 会 `step(delta)`）；
	# ② 只在这里做那个**推断**判据的清理（`retain_alive`），见文件头 ③。
	if drawer != null and _pending > 0:
		_pending = int(drawer.call("retain_alive"))

## 生成一条特效。`kind` 必须是 [`KINDS`] 之一（否则只记日志不画）。
## 返回那条记录的副本（自检用；`phase` / `duration` 是派给真节点的实参）。
func spawn(kind: String, pos: Vector2, _opts: Dictionary = {}) -> Dictionary:
	if not KINDS.has(kind):
		return {}
	var phase := int(PHASE_FOR_KIND.get(kind, -1))
	var duration := int(LIFETIME_TICKS.get(kind, 30))
	if drawer != null:
		drawer.call("spawn_effect", float(pos.x), float(pos.y), phase, duration)
	_pending += 1
	_spawned += 1
	_spawned_by_kind[kind] = int(_spawned_by_kind.get(kind, 0)) + 1
	return {"kind": kind, "x": float(pos.x), "y": float(pos.y),
		"phase": phase, "duration_ticks": duration}

## 清空（换局/退出对局时调）。
func clear() -> void:
	if drawer != null:
		drawer.call("clear_effects")
	_pending = 0
	_spawned = 0
	_spawned_by_kind.clear()

# ---------------------------------------------------------------- 只读快照

## 当前还活着的特效。真值读 `EffectDrawer`（它有相位与透明度两个探针），
## 名字只是"我们是按哪个名字派过去的"——那条对应关系是**我方选择**（文件头第 3 行）。
func snapshot() -> Array:
	var out: Array = []
	if drawer == null:
		return out
	var n := int(drawer.call("effect_count"))
	for i in n:
		out.append({
			"phase": int(drawer.call("effect_phase", i)),
			"alpha": float(drawer.call("effect_alpha", i)),
		})
	return out

func count() -> int:
	return (int(drawer.call("effect_count")) if drawer != null else 0)

## 累计生成过的条数（按种类）。
func spawned() -> Dictionary:
	var d := {"total": _spawned}
	for k in KINDS:
		d[k] = int(_spawned_by_kind.get(k, 0))
	return d

func alive_of(kind: String) -> int:
	return _alive_of(kind)

## 现在还在场上、且当初是按 `kind` 派过去的那几条。
##
## ⚠️ `EffectDrawer` 没有"按名字查"的接口（它只存相位字节），所以这条只能按相位数：
## 相位相同的两种名字会互相算进去。**这是缺口的直接后果**（名字 → 相位的对应本身是
## 我方选择），只给诊断用，不进任何判定。
func _alive_of(kind: String) -> int:
	if drawer == null:
		return 0
	var want := int(PHASE_FOR_KIND.get(kind, -1))
	var n := 0
	for i in int(drawer.call("effect_count")):
		if int(drawer.call("effect_phase", i)) == want:
			n += 1
	return n
