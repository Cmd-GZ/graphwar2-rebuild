extends Control
class_name GwPlaneView
## 战场绘制。原版是 Rust 类 `GraphwarPlane`（自绘，子节点 `@AxisDrawer` / `@EntityDrawer` /
## `@EffectDrawer` / `@Line2D`，见 `docs/92` §4.10、`docs/104` §5.3）。
##
## **配色不是目测的**：全部来自"用原版工程 + 原版 `.so` 在 1920×1160 跑出来的一帧"
## （`.agent-work/playfit/shots/orig_campaign_play.png`，由 `probe.gd` 驱动真原版
## 到 `play_screen` 后 `get_viewport().get_texture().get_image()` 存的），
## 逐像素取样得到下表（`docs/125` §2 有坐标）：
##
## | 元素 | 颜色 | 线宽 | 取样位置 |
## |---|---|---|---|
## | 棋盘底色 | `#FFFFFF` | 填充 | 大面积主色 95610/（3px 采样） |
## | 细网格（每 1 格） | `#E6E6E6` = (230,230,230) | 1 canvas px | 行 y=210，x=325/351/376… |
## | 粗网格（每 5 格） | `#B2B2B2` = (178,178,178) | 2 canvas px | 行 y=210，x=427/428 |
## | 主坐标轴（x=0 / y=0） | `#333333` = (51,51,51) | 2 canvas px | x=939 列、y=578/579 行 |
## | 棋盘外框 | `#000000` | 3 canvas px | x=300..302、y=195..197 |
##
## 网格间距：1279 px 宽 ÷ 50 = **25.58 px/单位**（细线间隔实测 25–26），
## 粗线每 5 单位（实测 427→555 = 128 px ≈ 5×25.58）——与 `docs/08` §4 的
## `px = 20x + 500`、`py = 20y + 300`（画布 1000×600）**一致**，那条换算仍然成立。
##
## 线宽用"画布像素"（1000×600）表达：`1 canvas px = _unit() / 20`，
## 这样在任何窗口尺寸下都与原版同比例（1920 窗口下 1 canvas px = 1 设备像素）。
##
## 不画的东西（诚实标注）：障碍物（需要内核的关卡生成器喂位置）、特效、命中后的洞、
## 士兵手里的小枪（原版由 `entity_drawer` 的 `draw_polygon` 画，未逐点取证）。

const BOARD_HALF_X := 25.0
const BOARD_HALF_Y := 15.0

## 士兵半径（棋盘单位）。`docs/120`：`PosInfo.radius` 恒 0.5；实测原版士兵
## 身体直径 ≈ 26 px @1920 = 1.0 单位（截图 `crop_soldier.png` 量取）。
const SOLDIER_RADIUS := 0.5

## 贴图"内容占比"：`skins/yellow.svg` 与 `skins/target.svg` 的可见内容只占 512 画布的
## **46.5%**（逐像素量取，见 docs/125 §2.3）。所以精灵方块的边长必须是身体直径 / 0.465，
## 否则画出来只有原版的一半大（第一版就是这个毛病）。
const SPRITE_CONTENT := 0.465

## 名字字号 = 1.12 单位。原版名字是 `name_label.tscn` 的 RichTextLabel，
## 字号取主题默认（`graphwar_theme.tres` 的 `default_font_size = 24` @1600×900）；
## 1920 窗口下 = 28.8 设备像素 = 1.12 × 25.58 单位 ✓（截图实测 x-height ≈13 px）。
const NAME_FONT_UNITS := 1.12

## 玩家颜色表（`docs/102` §1.1，逐条从 `.so` 的调色板表解出）：
## Team1 暖色 / Team2 冷色，各 6 个，分配算法是"随机起点 + 线性探测"。
const TEAM_COLORS := [
	[Color8(186, 42, 32), Color8(238, 114, 0), Color8(125, 104, 0),
		Color8(122, 160, 0), Color8(149, 5, 62), Color8(234, 0, 219)],
	[Color8(107, 0, 234), Color8(17, 0, 108), Color8(21, 83, 221),
		Color8(0, 203, 163), Color8(0, 116, 26), Color8(2, 85, 63)],
]

## 棋盘配色的**名字与来源**（见文件头）。
const COL_BOARD := Color(1, 1, 1)                                    # #FFFFFF
const COL_GRID_MINOR := Color(0.90196078, 0.90196078, 0.90196078)    # #E6E6E6
const COL_GRID_MAJOR := Color(0.69803922, 0.69803922, 0.69803922)    # #B2B2B2
const COL_AXIS := Color(0.2, 0.2, 0.2)                               # #333333
const COL_BORDER := Color(0, 0, 0)                                   # #000000

## AxisMode（`docs/118` §4.3 / §7.1）：3 = EveryUnit 最密、2 = EveryFive、1 = OnlyMain、0 = NoAxis。
const AXIS_NONE := 0
const AXIS_MAIN_ONLY := 1
const AXIS_EVERY_FIVE := 2
const AXIS_EVERY_UNIT := 3

var soldiers: Array[Dictionary] = []      # {name, team, pos:Vector2, alive, is_bot, skin, color}
var targets: Array = []                   # 靶子位置（棋盘单位）；战役关的靶子数来自 docs/118 §7.1
var path_points := PackedVector2Array()
var impact := Vector2.ZERO
var has_impact := false
var show_names := true
var axis_mode := AXIS_EVERY_UNIT
var path_color := Color(0.2, 0.2, 0.2)

func _ready() -> void:
	name = "GraphwarPlane"
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL
	# 原版战场是 Rust 控件自绘；这里同样自绘，不需要子节点。
	resized.connect(queue_redraw)
	mouse_filter = Control.MOUSE_FILTER_IGNORE

# ---------------------------------------------------------------- 坐标换算

## 1 个棋盘单位 = 多少像素。原版把 50×30 的棋盘铺满控件，所以取较小的那个比例
## （`docs/08` §4：1000×600 画布 → 20 px/单位，控件按比例放大）。
func _scale() -> float:
	var w := size.x
	var h := size.y
	if w <= 1.0 or h <= 1.0:
		return 1.0
	return minf(w / (BOARD_HALF_X * 2.0), h / (BOARD_HALF_Y * 2.0))

## "画布像素"（1000×600 那张画布）换算成当前控件的像素：1 canvas px = _scale()/20。
func _canvas_px() -> float:
	return _scale() / 20.0

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

func set_targets(list: Array) -> void:
	targets = list
	queue_redraw()

func set_axis_mode(v: int) -> void:
	axis_mode = clampi(v, 0, 3)
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

## 玩家颜色分配（`docs/102` §1.2 的真规则：本队 6 色表里随机起点、线性探测、
## 取第一个没人用的）。这里用确定性 RNG，保证同一局每次配色一致。
static func assign_colors(team_keys: Array[int], seed_value: int) -> Array[Color]:
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_value
	var used: Array[Color] = []
	var out: Array[Color] = []
	for team in team_keys:
		var pool: Array = TEAM_COLORS[clampi(team, 0, 1)]
		var n := pool.size()
		var base := int(rng.randi() % n)
		var picked: Color = pool[base]
		for k in n:
			var c: Color = pool[(base + k) % n]
			if not used.has(c):
				picked = c
				break
		used.append(picked)
		out.append(picked)
	return out

# ---------------------------------------------------------------- 绘制

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), COL_BOARD, true)
	_draw_grid()
	_draw_border()
	_draw_path()
	_draw_soldiers()

## 网格按 AxisMode 的 4 档画（原版 `AxisDrawer`；档位含义见 `docs/118` §7.1 的 21/21 描述对照）。
func _draw_grid() -> void:
	if axis_mode == AXIS_NONE:
		return
	var unit := _canvas_px()
	var one := unit                       # 细线 1 canvas px
	var two := unit * 2.0                 # 粗线/轴 2 canvas px
	var axis := axis_mode >= AXIS_MAIN_ONLY
	var major := axis_mode >= AXIS_EVERY_FIVE
	var minor := axis_mode >= AXIS_EVERY_UNIT

	if minor:
		var x := -BOARD_HALF_X
		while x <= BOARD_HALF_X + 0.001:
			if absf(fmod(x, 5.0)) > 0.001 and absf(x) > 0.001:
				_line_v(x, COL_GRID_MINOR, one)
			x += 1.0
		var y := -BOARD_HALF_Y
		while y <= BOARD_HALF_Y + 0.001:
			if absf(fmod(y, 5.0)) > 0.001 and absf(y) > 0.001:
				_line_h(y, COL_GRID_MINOR, one)
			y += 1.0
	if major:
		var x2 := -BOARD_HALF_X
		while x2 <= BOARD_HALF_X + 0.001:
			if absf(fmod(x2, 5.0)) <= 0.001 and absf(x2) > 0.001:
				_line_v(x2, COL_GRID_MAJOR, two)
			x2 += 1.0
		var y2 := -BOARD_HALF_Y
		while y2 <= BOARD_HALF_Y + 0.001:
			if absf(fmod(y2, 5.0)) <= 0.001 and absf(y2) > 0.001:
				_line_h(y2, COL_GRID_MAJOR, two)
			y2 += 1.0
	if axis:
		_line_v(0.0, COL_AXIS, two)
		_line_h(0.0, COL_AXIS, two)

func _line_v(board_x: float, color: Color, width: float) -> void:
	var top := board_to_screen(Vector2(board_x, BOARD_HALF_Y))
	var bottom := board_to_screen(Vector2(board_x, -BOARD_HALF_Y))
	draw_line(top, bottom, color, width)

func _line_h(board_y: float, color: Color, width: float) -> void:
	var left := board_to_screen(Vector2(-BOARD_HALF_X, board_y))
	var right := board_to_screen(Vector2(BOARD_HALF_X, board_y))
	draw_line(left, right, color, width)

## 外框：原版 3 canvas px 纯黑，画在控件内侧（取样 x=300..302 / y=195..197）。
func _draw_border() -> void:
	var w := _canvas_px() * 3.0
	var half := w * 0.5
	draw_rect(Rect2(Vector2(half, half), size - Vector2(w, w)), COL_BORDER, false, w)

func _draw_soldiers() -> void:
	var unit := _scale()
	# 靶子：原版是 `skins/target.svg`（蓝色靶心），实测直径 24 px @1920 ≈ 0.94 单位
	for t in targets:
		var tp := board_to_screen(t)
		var tex := GwAssets.texture("skins/target")
		var tside := unit * 0.94 / SPRITE_CONTENT
		if tex != null:
			draw_texture_rect(tex, Rect2(tp - Vector2(tside, tside) * 0.5, Vector2(tside, tside)), false)
	var font := GwThemeFactory.font()
	var fsize := maxi(9, int(round(unit * NAME_FONT_UNITS)))
	# 精灵方块边长 = 身体直径 / 内容占比 ⇒ 可见身体正好 1.0 单位（= 26 px @1920）
	var side := unit * 2.0 * SOLDIER_RADIUS / SPRITE_CONTENT
	for soldier in soldiers:
		var p: Vector2 = board_to_screen(soldier.pos)
		var alive := bool(soldier.alive)
		var is_bot := bool(soldier.get("is_bot", false))
		var skin_path: String = (String(GwIcons.BOT_SKINS[clampi(int(soldier.get("bot_tier", 0)), 0,
				GwIcons.BOT_SKINS.size() - 1)]) if is_bot
			else GwIcons.path_for("skin_%d" % int(soldier.get("skin", 0))))
		var face_path: String = GwIcons.BOT_FACE if is_bot else GwIcons.DEFAULT_FACE
		var hat_path: String = GwIcons.BOT_HAT if is_bot else GwIcons.DEFAULT_HAT
		var team_color: Color = soldier.get("color", Color(1, 1, 1))
		var rect := Rect2(p - Vector2(side, side) * 0.5, Vector2(side, side))
		var tint := Color(1, 1, 1, 1) if alive else Color(0.55, 0.55, 0.55, 0.45)
		for wp in [skin_path, face_path, hat_path]:
			if wp == "":
				continue
			var tex := GwAssets.texture(String(wp))
			if tex != null:
				draw_texture_rect(tex, rect, false, tint)
		if not alive:
			draw_line(p + Vector2(-side * 0.3, -side * 0.3), p + Vector2(side * 0.3, side * 0.3),
				Color(0, 0, 0, 0.7), maxf(1.0, unit * 0.08))
			draw_line(p + Vector2(side * 0.3, -side * 0.3), p + Vector2(-side * 0.3, side * 0.3),
				Color(0, 0, 0, 0.7), maxf(1.0, unit * 0.08))
		if show_names and String(soldier.name) != "":
			# 名字：玩家颜色 + 黑描边，画在士兵正上方。
			# 原版是 `name_label.tscn`（RichTextLabel，`[outline_size=4][outline_color=black]`）。
			var label := String(soldier.name)
			var w := font.get_string_size(label, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
			var base := p + Vector2(-w * 0.5, -unit * 0.95)
			var outline := maxi(1, int(round(unit * 0.16)))
			draw_string_outline(font, base, label, HORIZONTAL_ALIGNMENT_LEFT, -1, fsize,
				outline, Color(0, 0, 0))
			draw_string(font, base, label, HORIZONTAL_ALIGNMENT_LEFT, -1.0, fsize, team_color)

func _draw_path() -> void:
	if path_points.size() < 2:
		return
	var mapped := PackedVector2Array()
	for i in path_points.size():
		mapped.append(board_to_screen(path_points[i]))
	# 原版弹道是一条 Line2D（`@Line2D@N`，运行时新建，`docs/92` §4.10），颜色取射手的玩家色。
	draw_polyline(mapped, path_color, maxf(1.5, _canvas_px() * 3.0), true)
	if has_impact:
		var ip := board_to_screen(impact)
		draw_circle(ip, _scale() * 0.2, path_color)
