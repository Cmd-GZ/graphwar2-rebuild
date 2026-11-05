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
## **障碍（`docs/126`）**：原版是**纯黑实心圆**，半径 = 关卡生成器给的 `r`（棋盘单位）。
## 取样来自 LAN 真帧 `.agent-work/playfit/shots/orig_play_default.png`（1920×1160）：
##
## | 项 | 实测 |
## |---|---|
## | 颜色 | `#000000`（圆心逐像素 `(0,0,0)`；5 个圆里最大的 9289 个采样点全黑） |
## | 边缘 | 每个圆最外圈有 1 个**半覆盖**像素（灰度 154 / 242 / 43），即原版开抗锯齿 |
## | 半径（棋盘单位） | 0.80 / 0.81 / 1.66 / 2.13 / 3.42（最大那个被棋盘下边裁掉） |
## | 落在棋盘外 | **允许**：圆心可以到 `±(25+r)`、`±(15+r)`，超出部分被棋盘边**裁掉** |
##
## 其中"半径 0.80"这一颗是**独立的旁证**：关卡生成器的半径下限正好是
## `maxsd 0.8`（`docs/55` §2），而真帧里最小的圆量出来就是 0.80 单位 ⇒
## "画多大 = 生成器给的 r"（这条是**推断**：那一局真实的 `r` 值我们没有，
## 但 `docs/56` §3 抓到的 13 个障碍里确实出现过 `r = 0.800`）。
##
## 不画的东西（诚实标注）：障碍被摧毁的洞（`QueueDestroyObstacle` 的语义仍未定，
## `docs/38`）、特效、士兵手里的小枪（原版由 `entity_drawer` 的 `draw_polygon` 画，
## 未逐点取证）。
##
## **`docs/135` 的两条开火表现也在这里画**：
## * `ready_rings`：准备发射的光圈（**绘制细节全部自定**，原版无观测；见 `_draw_ready_rings`）；
## * `path_reveal`：弹道只画前 N 个点，由 `GwFireFx` 按 tick 往上顶，
##   `path_points` 仍是**全量**（`docs/126` §3.1 的留口）。

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
## 障碍：与棋盘外框同色（纯黑）。出处见文件头表格：LAN 真帧 5 个圆的圆心全是 `(0,0,0)`。
const COL_OBSTACLE := Color(0, 0, 0)                                 # #000000

## 准备光圈的线宽（画布像素）。与 `GwFireFx.RING_WIDTH_CANVAS_PX` 同值 —— 两边各留一份常数，
## 是为了**避免两个脚本互相引用**（GDScript 的类名循环依赖会让 `--import` 报错）。
const RING_WIDTH_CANVAS_PX := 2.0

## AxisMode（`docs/118` §4.3 / §7.1）：3 = EveryUnit 最密、2 = EveryFive、1 = OnlyMain、0 = NoAxis。
const AXIS_NONE := 0
const AXIS_MAIN_ONLY := 1
const AXIS_EVERY_FIVE := 2
const AXIS_EVERY_UNIT := 3

var soldiers: Array[Dictionary] = []      # {name, team, pos:Vector2, alive, is_bot, skin, color}
var targets: Array = []                   # 靶子位置（棋盘单位）；战役关的靶子数来自 docs/118 §7.1
## 障碍：每项 `{pos: Vector2, size: float}`，`size` = **半径**（棋盘单位）。
## 数据来自内核 `GW2Rebuild.level_obstacles(level_index, seed)`（`docs/126`），
## 本控件只负责画；**同一份数组**还必须传给 `GwCore.hit_test()`，否则子弹会穿墙。
var obstacles: Array = []
var path_points := PackedVector2Array()
var impact := Vector2.ZERO
var has_impact := false
## 只画前 N 个点（`docs/135` §3：弹道按 tick 逐点揭示）。`-1` = 全画（默认）。
## **`path_points` 永远是全量**：这个变量只影响"画多少"，不影响"这一发到底飞了多长"。
var path_reveal := -1
## 准备发射的光圈（`docs/135` §2）。每项
## `{index, pos: Vector2, color: Color, ready, progress, radius, coverage, alpha}`：
## `progress` 0 = 开环、1 = 闭合；`coverage`/`alpha` 由 `GwFireFx` 按 `progress` 插值算好。
## 数据源是 `PlayScreen.fire_fx_state()` → `GwFireFx`；本控件只负责画。
var ready_rings: Array = []
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
	# 障碍允许落在棋盘外（圆心可到 ±(25+r) / ±(15+r)，`docs/118` §7.2 的生成公式），
	# 原版真帧里超出棋盘的部分被**裁掉**（`orig_play_default.png` 左下那颗圆心 y=−12.44、
	# r=3.42，下缘 −15.86 被切在 y=−15）。本控件的矩形恰好就是棋盘，所以开 clip_contents
	# 即等价于原版的裁剪（实测：它连**自己** `_draw` 画出去的东西也裁，`docs/126` §4.2）。
	clip_contents = true

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

## 设障碍（`docs/126`）。`list` 每项 `{pos: Vector2, size: float}`，`size` = 半径（棋盘单位）。
## **同一份数组要原样传给 `GwCore.hit_test(pts, soldiers, obstacles)`** —— 画的和判的
## 必须是同一批圆，否则就会出现"看得见墙、子弹却穿过去"。
func set_obstacles(list: Array) -> void:
	obstacles = list
	queue_redraw()

func set_axis_mode(v: int) -> void:
	axis_mode = clampi(v, 0, 3)
	queue_redraw()

func set_path(points: PackedVector2Array, impact_point: Vector2 = Vector2.ZERO, show: bool = true) -> void:
	path_points = points
	impact = impact_point
	has_impact = show
	path_reveal = -1          # 默认全画；要逐 tick 揭示由 GwFireFx 之后设成具体点数
	queue_redraw()

func clear_path() -> void:
	path_points = PackedVector2Array()
	has_impact = false
	path_reveal = -1
	queue_redraw()

## 只画前 `n` 个点（`-1` = 全画）。`GwFireFx` 每帧往上顶，**单调不减**。
func set_path_reveal(n: int) -> void:
	var v := -1 if n < 0 else clampi(n, 0, maxi(path_points.size(), 1))
	if v == path_reveal:
		return
	path_reveal = v
	queue_redraw()

## 设置准备光圈（`GwFireFx` 每帧算好后推进来）。传空数组 = 全部熄灭。
func set_ready_rings(list: Array) -> void:
	ready_rings = list
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
	_draw_obstacles()
	_draw_path()
	_draw_ready_rings()
	_draw_soldiers()

## 障碍：纯黑实心圆，半径 = 生成器给的 `r`（棋盘单位 × _scale()）。
##
## 画在弹道与士兵**之下**：原版真帧里弹道不会与障碍重叠（弹道在撞点停下，`docs/30`），
## 士兵站在障碍旁边也是画在上面的（`EntityDrawer` 在障碍之后绘制）。
func _draw_obstacles() -> void:
	if obstacles.is_empty():
		return
	var s := _scale()
	for o in obstacles:
		var c: Vector2 = board_to_screen(o.pos)
		# 抗锯齿：原版真帧每个圆最外圈有 1 个半覆盖像素（灰度 154/242/43），
		# 这里用 `antialiased=true` 复刻（实测同样给出半覆盖边缘，`docs/126` §4.2）。
		draw_circle(c, s * float(o.get("size", 0.0)), COL_OBSTACLE, true, -1.0, true)

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
	# `path_reveal >= 0` 时只画前缀（`docs/135` §3：按 tick 逐点揭示）。
	# 撞点标记只在"整条都揭示完"时画 —— 弹丸没到就先画个撞点会骗人。
	var n := path_points.size()
	if path_reveal >= 0:
		n = clampi(path_reveal, 0, n)
	if n < 2:
		return
	var mapped := PackedVector2Array()
	for i in n:
		mapped.append(board_to_screen(path_points[i]))
	# 原版弹道是一条 Line2D（`@Line2D@N`，运行时新建，`docs/92` §4.10），颜色取射手的玩家色。
	draw_polyline(mapped, path_color, maxf(1.5, _canvas_px() * 3.0), true)
	if has_impact and (path_reveal < 0 or path_reveal >= path_points.size()):
		var ip := board_to_screen(impact)
		draw_circle(ip, _scale() * 0.2, path_color)

## 准备发射的光圈（`docs/135` §2）。**所有绘制细节都是自定**：
## 半径 1.15 单位（= 士兵半径 0.5 的 2.3 倍，正好在精灵方块之外）、线宽 2 画布像素、按玩家色、
## 从 12 点方向**顺时针**画 `coverage` 圈（`progress`=1 时正好闭合）。
##
## 画在士兵**下面**（`_draw` 里先画环再画精灵）：身体压住环的内侧，看起来像"站在光圈里"。
## 原版光圈长什么样**没有任何证据**（`docs/126` §6.3 第 2 条：用户口述），见 `docs/135` §2.3。
func _draw_ready_rings() -> void:
	if ready_rings.is_empty():
		return
	var s := _scale()
	var width := maxf(1.0, _canvas_px() * RING_WIDTH_CANVAS_PX)
	for r in ready_rings:
		var coverage: float = clampf(float(r.get("coverage", 1.0)), 0.0, 1.0)
		if coverage <= 0.0:
			continue
		var c: Vector2 = board_to_screen(r.get("pos", Vector2.ZERO))
		var radius: float = s * float(r.get("radius", 1.15))
		var col: Color = r.get("color", COL_AXIS)
		col.a = clampf(float(r.get("alpha", 1.0)), 0.0, 1.0)
		var start := -PI * 0.5
		var segments := maxi(8, int(64.0 * coverage))
		draw_arc(c, radius, start, start + TAU * coverage, segments, col, width, true)
