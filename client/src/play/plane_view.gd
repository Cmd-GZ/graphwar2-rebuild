extends Control
class_name GwPlaneView
## 战场绘制。原版是 Rust 类 `GraphwarPlane`（自绘，子节点 `@AxisDrawer` / `@EntityDrawer` /
## `@EffectDrawer` / `@Line2D`，见 `docs/92` §4.10、`docs/104` §5.3）。
##
## 本项目用 GDScript 的 `_draw()` 重画同样的东西：坐标轴 + 网格 + 士兵 + 弹道折线 + 落点。
## 棋盘坐标与边界来自 `docs/08` 与 `AGENTS.md` 的硬事实：`|x| <= 25`、`|y| <= 15`，
## **y 轴向上为正**（Godot 屏幕 y 向下 ⇒ 取反）。
##
## 不画的东西（诚实标注）：掩膜/障碍物（`src/plane.rs` 的规则没向 GDScript 暴露）、
## 特效、命中后的洞。所以这个战场只用于验证"能开一局"，不是物理可视化。

const BOARD_HALF_X := 25.0
const BOARD_HALF_Y := 15.0

## 士兵半径（棋盘单位）。原版士兵的绘制半径未观测；这里按"能看见、点得到"取值。
const SOLDIER_RADIUS := 0.8

var soldiers: Array[Dictionary] = []      # {name, team, pos:Vector2, alive:bool, is_bot:bool, skin:int}
var path_points := PackedVector2Array()
var impact := Vector2.ZERO
var has_impact := false
var show_names := true
var show_function_overlay := true

var _bg := Color("11151a")
var _grid := Color(1, 1, 1, 0.06)
var _axis := Color(1, 1, 1, 0.22)

func _ready() -> void:
	name = "GraphwarPlane"
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL
	# 原版战场是 Rust 控件自绘；这里同样自绘，不需要子节点。
	resized.connect(queue_redraw)

# ---------------------------------------------------------------- 坐标换算

func _scale() -> float:
	var w := size.x
	var h := size.y
	if w <= 1.0 or h <= 1.0:
		return 1.0
	return minf(w / (BOARD_HALF_X * 2.0), h / (BOARD_HALF_Y * 2.0))

func board_to_screen(p: Vector2) -> Vector2:
	var s := _scale()
	return Vector2(size.x * 0.5 + p.x * s, size.y * 0.5 - p.y * s)

func screen_to_board(p: Vector2) -> Vector2:
	var s := _scale()
	if s <= 0.0:
		return Vector2.ZERO
	return Vector2((p.x - size.x * 0.5) / s, (size.y * 0.5 - p.y) / s)

# ---------------------------------------------------------------- 数据

func set_soldiers(list: Array[Dictionary]) -> void:
	soldiers = list
	queue_redraw()

func set_path(points: PackedVector2Array, impact_point: Vector2 = Vector2.ZERO, show: bool = true) -> void:
	path_points = points
	impact = impact_point
	has_impact = show
	queue_redraw()

func clear_path() -> void:
	path_points = PackedVector2Array()
	has_impact = false
	queue_redraw()

func set_show_names(v: bool) -> void:
	show_names = v
	queue_redraw()

# ---------------------------------------------------------------- 绘制

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), _bg, true)
	_draw_grid()
	_draw_border()
	_draw_path()
	_draw_soldiers()

func _draw_grid() -> void:
	var s := _scale()
	# 每 5 个棋盘单位一条细线
	var step := 5.0
	var x := -BOARD_HALF_X
	while x <= BOARD_HALF_X + 0.001:
		var top := board_to_screen(Vector2(x, BOARD_HALF_Y))
		var bottom := board_to_screen(Vector2(x, -BOARD_HALF_Y))
		var is_axis := absf(x) < 0.001
		draw_line(top, bottom, _axis if is_axis else _grid, 2.0 if is_axis else 1.0)
		x += step
	var y := -BOARD_HALF_Y
	while y <= BOARD_HALF_Y + 0.001:
		var left := board_to_screen(Vector2(-BOARD_HALF_X, y))
		var right := board_to_screen(Vector2(BOARD_HALF_X, y))
		var is_axis := absf(y) < 0.001
		draw_line(left, right, _axis if is_axis else _grid, 2.0 if is_axis else 1.0)
		y += step
	# 轴刻度数字（只画 0 和 ±25/±15，避免糊成一片）
	var font := GwThemeFactory.font()
	var fsize := maxi(9, int(s * 1.1))
	for label in [
		{"t": "x", "p": Vector2(BOARD_HALF_X, 0.0)},
		{"t": "y", "p": Vector2(0.0, BOARD_HALF_Y)},
	]:
		draw_string(font, board_to_screen(label.p) + Vector2(4, -4), String(label.t),
			HORIZONTAL_ALIGNMENT_LEFT, -1, fsize, Color(1, 1, 1, 0.35))

func _draw_border() -> void:
	var tl := board_to_screen(Vector2(-BOARD_HALF_X, BOARD_HALF_Y))
	var br := board_to_screen(Vector2(BOARD_HALF_X, -BOARD_HALF_Y))
	draw_rect(Rect2(tl, br - tl), Color(1, 1, 1, 0.18), false, 2.0)

func _draw_soldiers() -> void:
	var s := _scale()
	var font := GwThemeFactory.font()
	var fsize := maxi(10, int(s * 1.3))
	for soldier in soldiers:
		var p: Vector2 = board_to_screen(soldier.pos)
		var r := maxf(6.0, SOLDIER_RADIUS * s)
		var alive := bool(soldier.alive)
		# 士兵 = 皮肤 / 脸 / 帽子 三层贴图，同位置叠加 —— 与原版战场一致
		# （`docs/92` §4.10 观测：每个士兵 3 个 Sprite2D）。图来自 assets/，不再画圆。
		var is_bot := bool(soldier.get("is_bot", false))
		var skin_path: String = (String(GwIcons.BOT_SKINS[clampi(int(soldier.get("bot_tier", 0)), 0,
				GwIcons.BOT_SKINS.size() - 1)]) if is_bot
			else GwIcons.path_for("skin_%d" % int(soldier.get("skin", 0))))
		var face_path: String = GwIcons.BOT_FACE if is_bot else GwIcons.DEFAULT_FACE
		var hat_path: String = GwIcons.BOT_HAT if is_bot else GwIcons.DEFAULT_HAT
		# 贴图里士兵本体只占画布中间一部分，所以显示边长取半径的 5 倍左右（肉眼定，见 docs/121 §7）
		var side := r * 5.0
		var rect := Rect2(p - Vector2(side, side) * 0.5, Vector2(side, side))
		var tint := Color(1, 1, 1, 1) if alive else Color(0.55, 0.55, 0.55, 0.45)
		for wp in [skin_path, face_path, hat_path]:
			if wp == "":
				continue
			var tex := GwAssets.texture(String(wp))
			if tex != null:
				draw_texture_rect(tex, rect, false, tint)
		if not alive:
			# 阵亡：画一个叉（原版的阵亡表现未观测，这里沿用本客户端自己的记号）
			draw_line(p + Vector2(-r * 0.6, -r * 0.6), p + Vector2(r * 0.6, r * 0.6), Color(0, 0, 0, 0.7), 2.0)
			draw_line(p + Vector2(r * 0.6, -r * 0.6), p + Vector2(-r * 0.6, r * 0.6), Color(0, 0, 0, 0.7), 2.0)
		if show_names and String(soldier.name) != "":
			# 名字画在士兵上方：先量宽度再居中（早期固定 80 px 会把名字截断）
			var label := String(soldier.name)
			var w := font.get_string_size(label, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
			var text_pos := p + Vector2(-w * 0.5, -r - 8.0)
			draw_string(font, text_pos, label, HORIZONTAL_ALIGNMENT_LEFT, -1.0, fsize,
				Color(1, 1, 1, 0.8))

func _draw_path() -> void:
	if path_points.size() < 2:
		return
	var mapped := PackedVector2Array()
	for i in path_points.size():
		mapped.append(board_to_screen(path_points[i]))
	# 原版弹道是一条 Line2D（`@Line2D@N`，运行时新建，docs/92 §4.10）
	draw_polyline(mapped, GwThemeFactory.COL_ACCENT, 2.5, true)
	if has_impact:
		var ip := board_to_screen(impact)
		draw_circle(ip, 5.0, GwThemeFactory.COL_BAD)
		draw_arc(ip, 9.0, 0.0, TAU, 20, GwThemeFactory.COL_BAD, 1.5)
