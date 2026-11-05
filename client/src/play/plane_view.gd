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
## * `ready_rings`：准备发射的准备圈（**几何/颜色已由 `docs/140` 从原版真帧逐像素量出**，
##   不再是自定值；见 `_draw_ready_rings` 与上面的 `RING_*` 常数表）；
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

## ---------------------------------------------------------------- 准备圈几何（`docs/140`）
##
## **答案是扒到代码了**：准备圈由 `EntityDrawer::draw`（vaddr `0x4b2890`，Ghidra `0x5b2890`）
## 里的 **3 次 `draw_arc`** 画出，参数与常量全是 objdump 直接读出来的立即数。
## 下面每个 `RING_*` 都标注了它来自哪条指令 / 哪个 rip 槽。
##
## 用户给的两张 92×87 真值裁图（准备前 / 准备完毕）是**同一场景、同一士兵**
## （网格线位置逐像素一致），用来**交叉验证**：
##
## | 量 | 真值图像素 | 二进制 | 一致性 |
## |---|---|---|---|
## | 外径 / 内径 | 22.57 / 15.85 px = **1.4240** | `width = 0.3` + 反解 `R/W = 2.859` ⇒ 1.4240 | ✅ 逐位 |
## | 环带 | 6.72 px | `width = 0.3`（同一量纲） | ✅ 比例 |
## | 段数 / 间隔 | 3 段、缺口 4 处、箭头相距 120° | `A_i = A0 + i·2π/3`，i=0,1,2 | ✅ 独立互证 |
## | 张开态覆盖 | 44%（每段 ≈52.8°） | `H` 默认 π/6 ⇒ 每段 60°（合计 180°） | ⚠ 见 §3.6 |
## | 闭合态覆盖 | ≈100%（有重叠） | `H` 封顶 61.5° ⇒ 每段 123°，3×123° = 369° | ✅ |
## | 颜色 | `#7D6800` = `TEAM_COLORS[0][2]` | 运行时取玩家色（栈上 `Color`） | ✅ |
## | 张开态 alpha | 不透明（精确色 934 px） | — | 我们原来写 0.55，**错** |
## | 转角 | +15°（两次独立测量） | `dA0 = dH/2` = (61.5−30)/2 = **15.75°** | ✅ |
##
## **比例尺有一个未解决的 14%**（`docs/140` §3.6）：网格线 25.5 px/单位 与
## "`width = 0.3` 对应 6.72 px"给出的 22.4 px/单位 不一致。这不影响本文件的
## 任何常数 —— 它们全是**比例/角度量**（R 与 W 同单位），只有"屏幕上多大"整体受它影响。
##
## **段数是 3，不是 4**：准备态缺口在 0–15° / 75–135° / 195–255° / 315–357° 四处
## ⇒ 恰好 3 段、每段 120°；闭合态外凸箭头实测 2 个（60.5° / 180.2°，相距 120°），
## 第 3 个在 300° 被名字标签与弹道线压住（那一带 `#7D6800` 像素形态学开运算后
## 全部消失 ⇒ 只是细线）。用户口述"4 段"，以像素+机器码为准是 3 段（`docs/140` §3.4）。
const RING_SEGMENTS := 3
## **下面这一组是 `EntityDrawer::draw` 里的立即数**（`docs/140` §2，objdump 逐条读出来的）：
##
## ```text
## 0x4b2ead   A_i = A0 + i·2.09439510239        → 三段方位角**正好相差 120°**
## 0x4b2ebc   (i = 0,1,2)
## 0x4b34d8   H ← 0.523598775598               = π/6 = 30°      ← 张开态半张角
## 0x4b3594   H = min(H0 + (V − map[idx])/60, 1.07337748998)     ← 1.07337749 = 61.5° 封顶
## 0x4b359c   minsd 1.07337748998
## 0x4b3662   width = 0.3                     ← 环带宽（float32 @0x1bb2b4 = 0x3e99999a）
## 0x4b3655   point_count = 10                ← 每段只折 10 段（不是 48）
## 0x4b366a   antialiased = false             ← `xor %ecx,%ecx`
## 0x4b360d   start = A − H ,  end = A + H    ← 以 A 为**中点**对称张开
## ```
##
## **旋转不是另一套动画**：`A0 = V/120` 与 `H = H0 + (V−…)/60` 由**同一个 V** 驱动，
## 所以 `dA0 = dH/2`。张开→闭合的 `ΔH = 61.5° − 30° = 31.5°` ⇒ 转角
## `ΔA0 = 15.75°`，与两张真值图实测的 +15°（43.9°→60.5°、164.5°→180.2°）吻合。
const RING_HALF_DEG_MIN := 30.0        ## π/6：张开态每段张角 60°
const RING_HALF_DEG_MAX := 61.5        ## 1.07337748998 rad：闭合态每段张角 123°（3×123° > 360° ⇒ 整圈）
const RING_WIDTH_UNITS := 0.3          ## draw_arc 的 width（立即数 0x3e99999a）
## 环中心线半径。**这一个数没能静态定死**：它来自运行时 `PosInfo` 的第 3 个 f64
## （`0x4b317f` 从 `[rsp+0x90]` 搬过来）。这里取"能让实测比例对上"的值：
## 真帧量到 外径/内径 = 22.57/15.85 = 1.4240，而 (R+W/2)/(R−W/2) = 1.4240 ⇒ R/W = 2.859
## ⇒ R = 2.859 × 0.3 = **0.8576**。几何形状（R 与 W 的比例）因此是**逐位对上**的，
## 只有整体缩放还差 ~14%（见 `docs/140` §3.6 的诚实说明）。
const RING_RADIUS_UNITS := 0.8576
## 闭合时第 0 段的方位角（屏幕坐标：0° = +x，角度增大 = 顺时针）。
## 代码里 A0 = V/120 是个**运行时**量（V 来自 tick/时间），静态定不到相位；
## 这里取真帧实测值 60.5° / 180.2° / 300.5° 的整数值。
const RING_READY_AZIMUTH_DEG := 60.0
## 张开态整组回退角 = (61.5° − 30°)/2 = 15.75°（**由上面的公式推出来**，不是另设的参数）
const RING_SPIN_DEG := 15.75
## 外凸箭头。**这一段只有像素证据**：写它的 `draw_polygon` 调用点已经找到了
## （`EntityDrawer::draw` 三次调用 0x4b9a70，后者经 0x3a01a0 走 `draw_polygon`，
## 名字字面量在 0x1c2bad），但**多边形顶点没能静态解出来**（点集在运行时由
## `sin_cos` 逐点累进，参数块跨 4 个 f64 + 一个 Vec 句柄）—— 所以形状是**量出来的**，
## 档位是"推断"，`docs/140` §3.3 与本文件顶部都标了。
## 量到的：箭头外沿是**等半径圆弧**（左箭头 x=14 那一列在 y=46..57 共 12 行都存在，
## r 稳定在 30.8~31.3 px），尖端/环外径 = 31.0/22.57 = 1.3735，角宽约 ±9°。
const RING_BARB_TIP_UNITS := 1.3839
const RING_BARB_HALF_DEG := 9.0
## `draw_arc` 的折点数：原版就是 10（`mov $0xa,%esi`），开抗锯齿也是关的。
const RING_ARC_POINTS := 10
const RING_ARC_ANTIALIASED := false

## 弹道线宽（画布 px）。实测：真帧里那条斜线在 45° 方向上横向跨 4 px ⇒ 垂直线宽
## `4·sin45° = 2.83 px @22.4px/单位` = **2.2 画布 px**（22.4/20 = 1.12 px 每画布 px）。
## 我们原来画 3.0 画布 px（= 3.8 px），偏粗 1.35 倍 —— 用户说"粗细和原版差别很大"就是这条。
## 注意：弹道的绘制原语**没扒到**（`draw_polyline` 在 `.so` 里根本不存在这个字符串，
## 见 `docs/140` §4），所以线宽只有像素证据。
const PATH_WIDTH_CANVAS_PX := 2.2

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
## `progress` 0 = 张开、1 = 闭合；`half_deg`/`coverage`/`alpha` 由 `GwFireFx` 按 `progress` 算好。
## **本控件只按 `progress` 画**（`half_deg`/`coverage` 是给自检和诊断看的导出量）。
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

## 弹道（`docs/135` §3 的逐 tick 揭示 + `docs/140` §4 的实测线宽/颜色/端点）。
##
## | 量 | 原版实测 | 我们原来 | 现在 |
## |---|---|---|---|
## | 颜色 | 射手的**玩家色**（真帧 `#7D6800` = `TEAM_COLORS[0][2]`，与准备圈、名字同色） | `path_color` = 玩家色 ✓ | 不变 |
## | 线宽 | **2.83 px @25.5px/单位** = 2.2 画布 px（45° 斜线横向跨 4 px ⇒ 4·sin45°） | 3.0 画布 px（粗 1.35×） | `PATH_WIDTH_CANVAS_PX` |
## | 端点 | **没有端点标记**：原版是一条 `Line2D`（`docs/92` §4.10），画不出端点圆点 | 撞点画半径 0.2 单位的实心圆（≈5 px） | 去掉 |
##
## 端点那条是**推断**（真帧里弹道在画面外结束，没拍到端点）：依据只有"原版是单个
## `Line2D` 节点"这一条结构事实。如果以后拍到原版撞点有标记，改回来是一行。
func _draw_path() -> void:
	# `path_reveal >= 0` 时只画前缀（`docs/135` §3：按 tick 逐点揭示）。
	var n := path_points.size()
	if path_reveal >= 0:
		n = clampi(path_reveal, 0, n)
	if n < 2:
		return
	var mapped := PackedVector2Array()
	for i in n:
		mapped.append(board_to_screen(path_points[i]))
	# 原版弹道是一条 Line2D（`@Line2D@N`，运行时新建，`docs/92` §4.10），颜色取射手的玩家色。
	draw_polyline(mapped, path_color, maxf(1.0, _canvas_px() * PATH_WIDTH_CANVAS_PX), true)

## 准备圈（`docs/140`）。**几何与颜色来自原版真帧的逐像素测量**，见文件头的 `RING_*` 表。
##
## 模型（三段同一套参数，`progress` 是唯一的动画量）：
##
## ```text
## 每段半张角 H(p) = 30° + (61.5° − 30°)·p          ← 原版 H 的上下限，立即数
## 每段方位 az_i(p) = 60° + 120°·i − 15.75°·(1−p)   ← 回退角 = ΔH/2，同源驱动
## 环带   = draw_arc(center, R=0.8576, az−H, az+H, 10, 色, width=0.3, aa=false)
## 外凸箭头 = 从环外径 1.0076 到 1.3839、±9° 的扇区（`draw_colored_polygon`）
## ```
##
## `p = 0`：三段各 60°、彼此留 60° 缺口（合计 180°），整组比闭合态回退 15.75°
## ⇒ 真帧里那几块"分开的件"；`p = 1`：三段各 123°、合计 369° > 360° ⇒ **首尾重叠成整圈**，
## 箭头转到 60° / 180° / 300° ⇒ 真帧里的"实心环 + 外凸箭头"。
##
## 画在士兵**下面**（`_draw` 里先画环再画精灵），与 `docs/135` §2.3 的层次一致。
## 颜色 = 玩家的玩家色：真帧里环、箭头、名字、弹道是**同一个** `#7D6800`，
## 正是 `TEAM_COLORS[0][2]`，所以环没有任何独立配色。
func _draw_ready_rings() -> void:
	if ready_rings.is_empty():
		return
	var s := _scale()
	for r in ready_rings:
		var progress: float = clampf(float(r.get("progress", 1.0)), 0.0, 1.0)
		var coverage: float = clampf(float(r.get("coverage", 1.0)), 0.0, 1.0)
		if coverage <= 0.0:
			continue
		var c: Vector2 = board_to_screen(r.get("pos", Vector2.ZERO))
		var col: Color = r.get("color", COL_AXIS)
		col.a = clampf(float(r.get("alpha", 1.0)), 0.0, 1.0)
		var mid_r: float = s * float(r.get("radius", RING_RADIUS_UNITS))
		var band: float = maxf(1.0, s * RING_WIDTH_UNITS)
		var tip_r: float = s * RING_BARB_TIP_UNITS
		var half := ring_half_span(progress)
		for i in RING_SEGMENTS:
			var az := ring_segment_azimuth(i, progress)
			# 原版就是 `draw_arc(center, radius, A−H, A+H, 10, color, 0.3, false)`：
			# 对称张开、10 个折点、**关抗锯齿**（`docs/140` §2 的 objdump 逐条）。
			draw_arc(c, mid_r, az - half, az + half, RING_ARC_POINTS, col, band,
				RING_ARC_ANTIALIASED)
			draw_colored_polygon(ring_barb_polygon(c, az, mid_r, band, tip_r), col)

## 每段的**半张角**（弧度）。原版公式（`docs/140` §2）：
## `H = min(π/6 + (V − map[idx])/60, 1.07337748998)`，默认（查不到时）`H = π/6`。
## 这里用 `progress` 0→1 线性驱动 H 从 30° 走到 61.5°；`V` 本身是运行时量，取不到。
## **抽成公开函数是为了自检能直接验证几何**（而不是只比对常数）。
static func ring_half_span(progress: float) -> float:
	var p := clampf(progress, 0.0, 1.0)
	return deg_to_rad(RING_HALF_DEG_MIN + (RING_HALF_DEG_MAX - RING_HALF_DEG_MIN) * p)

## 第 `i` 段的方位角（弧度，屏幕坐标：0 = +x，增大 = 顺时针）。
## `progress = 1` 时是闭合态（60° + 120°·i，实测 60.5° / 180.2° / 300.5°）；
## `progress = 0` 时整组回退 `RING_SPIN_DEG`（实测 43.9° / 164.5° / 285°）。
## 回退角不是独立参数：原版 `A0 = V/120`、`dH = dV/60` ⇒ `dA0 = dH/2`
## = (61.5° − 30°)/2 = 15.75°，`docs/140` §2 末尾。
static func ring_segment_azimuth(i: int, progress: float) -> float:
	var ready := deg_to_rad(RING_READY_AZIMUTH_DEG + 360.0 / float(RING_SEGMENTS) * float(i))
	return ready - deg_to_rad(RING_SPIN_DEG) * (1.0 - clampf(progress, 0.0, 1.0))

## 外凸箭头：以 `center` 为心、方位 `az`、从 `mid_r − band/2`（环外径）到 `tip_r`（尖端）、
## 角半宽 `RING_BARB_HALF_DEG` 的**扇区**（不是三角形 —— 真帧里它的外沿是等半径圆弧：
## 左箭头 x=14 那一列在 y=46..57 共 12 行都存在，`r` 稳定在 30.8~31.3 px，`docs/140` §3.3）。
static func ring_barb_polygon(center: Vector2, az: float, mid_r: float, band: float,
		tip_r: float) -> PackedVector2Array:
	var half := deg_to_rad(RING_BARB_HALF_DEG)
	var outer := mid_r + band * 0.5
	var steps := 6
	var pts := PackedVector2Array()
	for k in steps + 1:                       # 外弧：az−half → az+half
		var a := az - half + 2.0 * half * float(k) / float(steps)
		pts.append(center + Vector2(cos(a), sin(a)) * tip_r)
	for k in steps + 1:                       # 内弧：az+half → az−half（回到起点闭合）
		var a := az + half - 2.0 * half * float(k) / float(steps)
		pts.append(center + Vector2(cos(a), sin(a)) * outer)
	return pts
