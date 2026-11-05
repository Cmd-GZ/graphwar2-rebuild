extends RefCounted
class_name GwThemeFactory
## 主题与字体。
##
## 字体（`docs/121` §5.4）
## --------------------
## **不打包任何字体文件**，用使用者自己 `Graphwar.pck` 里的原版字体：
##
##   res://button_font.tres  → assets/button_font.res    （原版主字体，MSDF 已烘焙）
##   res://EraserRegular.ttf → assets/EraserRegular.fontdata（原版标题字体）
##
## 依据：恢复出的 `graphwar_theme.tres` 里 `Button/fonts/font = res://button_font.tres`
## （5 处主题都用它），`title_theme.tres` 的 `default_font = res://EraserRegular.ttf`。
## 包里**没有 `.ttf`**（只有 7 个 `.fontdata` 导入产物），所以这两条是"原版字体"的
## 唯一可得形态；找到就用，找不到就 `push_error` 并退回 Godot 内置字体
## （`GwAssets.missing()` 会记下来，selftest 会显式报告跳过）。
##
## 配色（`docs/121` §3）
## -------------------
## 不是我们挑的：全部抄自原版主题 `.tres` / `project.binary` 的**观测值**。
##
##   clear_color / boot_splash bg  = #0E251C  (0.054902, 0.145098, 0.109804)
##   Button/Panel normal           = #1A3B2D  bg + 3px 黑边 + 圆角 10
##   Button pressed                = #0F281D
##   Button disabled               = #383838
##
## 出处：`graphwar_style_box_flat.tres`、`graphwar_theme.tres`、
## `project.binary` 的 `rendering/environment/defaults/default_clear_color`。

## 基准字号。**原版 `graphwar_theme.tres` 的 `default_font_size = 24`，一字不改照抄。**
##
## 来历（docs/124 §2.6）：上一轮为了将就我们自己的 1280×720 视口，把它手工缩成
## 20（24 × 720/900）—— 那是在补偿我们自己写错的 `stretch/mode="canvas_items"`。
## 现在视口已经是原版的 **1600×900**、拉伸是原版的 **disabled**，就没有任何理由再缩放：
## 界面上的字号本来就全部由脚本按"窗口宽 ÷ N"覆盖，这个 24 只给没被脚本覆盖的控件兜底。
const BASE_FONT_SIZE := 24

## 原版主字体 / 标题字体的 `res://` 路径（不含扩展名）。
const FONT_MAIN := "button_font"
const FONT_TITLE := "EraserRegular"

## --- 原版配色（观测值，不是设计选择）---
const COL_BG := Color(0.054902, 0.145098, 0.109804)          # #0E251C
const COL_PANEL := Color(0.101961, 0.231373, 0.176471)       # #1A3B2D
const COL_PANEL_EDGE := Color(0, 0, 0)                       # 原版边框恒为纯黑
const COL_PRESSED := Color(0.0588235, 0.156863, 0.113725)    # #0F281D
const COL_DISABLED := Color(0.219608, 0.219608, 0.219608)    # #383838
const COL_TEXT := Color(1, 1, 1)                             # Button/colors/font_color
const PANEL_RADIUS := 10
const PANEL_BORDER := 3

## --- 以下三项原版没有，是本客户端自用的（状态行 / 提示 / 队伍色）---
const COL_TEXT_DIM := Color(0.78, 0.78, 0.78)
const COL_ACCENT := Color(0.55, 0.85, 0.68)
const COL_WARN := Color(0.91, 0.65, 0.29)
const COL_BAD := Color(0.91, 0.45, 0.29)
const COL_TEAM1 := Color(0.31, 0.55, 0.88)
const COL_TEAM2 := Color(0.88, 0.45, 0.31)


## 缓存：`draw_string` 每次新建 FontFile 会拿不到已光栅化的字形（画出来是空框），
## 所以全局只用同一个 Font 对象（`docs/108` §9.2 坑 6）。
static var _font_cache: Font = null
static var _title_font_cache: Font = null


## 原版主字体。没有资产目录时返回 Godot 内置字体并报错（不静默）。
static func font() -> Font:
	if _font_cache == null:
		_font_cache = GwAssets.font(FONT_MAIN)
	return _font_cache


## 原版标题字体（`title_theme.tres`）。缺失时退回主字体。
static func title_font() -> Font:
	if _title_font_cache == null:
		_title_font_cache = GwAssets.font(FONT_TITLE)
		if _title_font_cache == null:
			_title_font_cache = font()
	return _title_font_cache


## StyleBoxFlat，**不带 content margin**。
##
## 原版 5 个 `graphwar_style_box_flat*.tres` 的属性表只有 9 条：`bg_color` + 4 个
## `border_width_*` + `border_color` + 4 个 `corner_radius_*`，**没有任何 content_margin**。
## 我们以前自己加了 12/6 的内边距，等于把每个按钮的最小尺寸撑大了 24×12 px ——
## 这也是"按钮比原版大一圈"的第二个来源（`docs/124` §2.2）。现在不设，与原版一致。
static func _flat(bg: Color, edge: Color = COL_PANEL_EDGE, radius: int = PANEL_RADIUS,
		width: int = PANEL_BORDER) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.border_color = edge
	sb.set_border_width_all(width)
	sb.set_corner_radius_all(radius)
	# 原版 `.tres` 里**没有** content_margin 字段 ⇒ Godot 用 StyleBoxFlat 的默认值，
	# 即四条边各等于 border width（3）。这里显式写 3：早先自选的 12/6 会让每个按钮
	# 比原版宽 18、高 6，实测把 PlayScreen 的右键盘从 337 px 顶到 426 px（docs/125 §4）。
	sb.content_margin_left = width
	sb.content_margin_right = width
	sb.content_margin_top = width
	sb.content_margin_bottom = width
	return sb


## 原版 `campaign_button_theme_2.tres` —— **Campaign 屏 21 个关卡数字用的主题**。
##
## `campaign.tscn` 把这份主题挂在生成关卡按钮的那个 `Control` 上
## （`[node name="Control" ... theme = ExtResource("8")]`，ExtResource("8") =
## `res://campaign_button_theme_2.tres`），所以运行时 `GraphwarButton.new()` 造出来的
## 21 个按钮全都继承它。它的 `[resource]` 段只有 7 条（逐条照抄）：
##
##   default_font                      = FontFile(font_name = "Eraser")   ← 手写体，和标题同一套
##   Button/styles/{normal,hover,pressed,disabled,focus} = StyleBoxEmpty  ← **没有圆角矩形底**
##   Button/colors/font_disabled_color = (0.87451, 0.87451, 0.87451, **0**) ← alpha 0 ⇒ 锁住的关看不见
##   Button/colors/icon_normal_color   = (1, 1, 1, 0)
##
## **没有** 覆盖 `font_color` ⇒ 白色是从上层 app 主题继承下来的（用户截图里 21 个数字全白，
## 正是这个原因）。旧实现给每个格子画了一个灰色圆角按钮 + 我们自己的无衬线粗体，
## 与原版完全不是一回事（docs/124 §3）。
static func campaign_level_theme() -> Theme:
	var t := Theme.new()
	var f := title_font()
	if f != null:
		t.default_font = f
	for state in ["normal", "hover", "pressed", "disabled", "focus"]:
		t.set_stylebox(state, "Button", StyleBoxEmpty.new())
	t.set_color("font_disabled_color", "Button", Color(0.87451, 0.87451, 0.87451, 0.0))
	t.set_color("icon_normal_color", "Button", Color(1, 1, 1, 0.0))
	return t


## 圆角 + 黑边 + 深绿底：原版 `graphwar_style_box_flat.tres` 的形状。
static func build() -> Theme:
	var t := Theme.new()
	t.default_font = font()
	t.default_font_size = BASE_FONT_SIZE

	# --- Button（原版 graphwar_theme.tres 的五个 stylebox）---
	t.set_stylebox("normal", "Button", _flat(COL_PANEL))
	t.set_stylebox("hover", "Button", _flat(COL_PANEL))
	t.set_stylebox("pressed", "Button", _flat(COL_PRESSED))
	t.set_stylebox("disabled", "Button", _flat(COL_DISABLED))
	t.set_stylebox("focus", "Button", StyleBoxEmpty.new())
	t.set_color("font_color", "Button", COL_TEXT)
	t.set_color("font_hover_color", "Button", COL_TEXT)
	t.set_color("font_pressed_color", "Button", COL_TEXT)
	t.set_color("font_disabled_color", "Button", COL_TEXT_DIM)
	t.set_font_size("font_size", "Button", BASE_FONT_SIZE)

	# --- Label ---
	t.set_color("font_color", "Label", COL_TEXT)
	t.set_font_size("font_size", "Label", BASE_FONT_SIZE)

	# --- LineEdit / TextEdit（原版：透明底 + 3px 黑边）---
	t.set_stylebox("normal", "LineEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_stylebox("focus", "LineEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_color("font_color", "LineEdit", COL_TEXT)
	t.set_color("caret_color", "LineEdit", COL_TEXT)
	t.set_font_size("font_size", "LineEdit", BASE_FONT_SIZE)
	t.set_stylebox("normal", "TextEdit", _flat(Color(0.6, 0.6, 0.6, 0)))
	t.set_color("font_color", "TextEdit", COL_TEXT)
	t.set_font_size("font_size", "TextEdit", BASE_FONT_SIZE)

	# --- Panel / PanelContainer（原版 graphwar_style_box_flat.tres）---
	t.set_stylebox("panel", "Panel", _flat(COL_PANEL))
	t.set_stylebox("panel", "PanelContainer", _flat(COL_PANEL))

	# --- TabContainer（PlayerScreen 的三个页签）---
	t.set_stylebox("panel", "TabContainer", _flat(COL_PANEL))
	t.set_stylebox("tab_selected", "TabContainer", _flat(COL_PANEL))
	t.set_stylebox("tab_unselected", "TabContainer", _flat(Color(0, 0, 0, 0)))
	t.set_color("font_selected_color", "TabContainer", COL_TEXT)
	t.set_color("font_unselected_color", "TabContainer", COL_TEXT_DIM)
	# 原版 graphwar_theme.tres：`TabContainer/font_sizes/font_size = 50`（逐字照抄）。
	t.set_font_size("font_size", "TabContainer", 50)

	# --- RichTextLabel（聊天/函数列表/名字）---
	t.set_color("default_color", "RichTextLabel", COL_TEXT)
	t.set_font_size("normal_font_size", "RichTextLabel", 18)

	return t
