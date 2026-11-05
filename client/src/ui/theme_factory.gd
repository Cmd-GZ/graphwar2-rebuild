extends RefCounted
class_name GwThemeFactory
## 主题与字体。
##
## 字体策略（见 docs/108 §6）：**不打包任何字体文件**。
## 原版用 7 个 `.ttf`（游戏资产，不可分发），这里改用 Godot 的 `SystemFont`：
## 只写"字体族名"，运行时由 fontconfig 解析本机已装的字体。因此
##   * 仓库里没有字体 → 没有许可证问题；
##   * 换机器时按下面的候选链逐个回退；
##   * 想用指定的（已授权的）字体时，设 `GW2_FONT_PATH=/abs/path/font.ttf` 覆盖。
##
## 界面是英文的（原版没有多语言支持，见 docs/108），因此不需要 CJK 字体。

## fontconfig 上任意 Linux 桌面都至少有一个；顺序 = 观感优先级。
const FONT_CANDIDATES := [
	"Noto Sans",
	"DejaVu Sans",
	"Liberation Sans",
	"Adwaita Sans",
	"FreeSans",
]

const BASE_FONT_SIZE := 20

## 配色（本项目自己的配色，不是从游戏里取的）。
const COL_BG := Color("16191d")
const COL_PANEL := Color("22272e")
const COL_PANEL_EDGE := Color("3a4250")
const COL_TEXT := Color("e6e6e6")
const COL_TEXT_DIM := Color("a0a8b4")
const COL_ACCENT := Color("7fd1a8")
const COL_WARN := Color("e8a54a")
const COL_BAD := Color("e8734a")
const COL_TEAM1 := Color("4f8ce0")
const COL_TEAM2 := Color("e0734f")


## 缓存：`draw_string` 每次新建 FontFile 会拿不到已光栅化的字形（画出来是空框），
## 所以全局只用同一个 Font 对象。
static var _font_cache: Font = null

static func font() -> Font:
	if _font_cache != null:
		return _font_cache
	_font_cache = _load_font()
	return _font_cache

static func _load_font() -> Font:
	# 1) 用户显式指定的（已授权）字体文件
	var override := OS.get_environment("GW2_FONT_PATH")
	if override != "" and FileAccess.file_exists(override):
		var f := FontFile.new()
		var err := f.load_dynamic_font(override)
		if err == OK:
			return f
		push_warning("[gw2] GW2_FONT_PATH 无法加载，回退到系统字体：%s" % override)

	# 2) 本机已装字体：**用 FontFile 按路径加载**，而不是 SystemFont。
	#    原因（实测）：`SystemFont` 在控件里正常，但 `CanvasItem.draw_string` 画出来是缺字方块
	#    （自绘的函数输入框、战场上的士兵名字都会坏）。FontFile 没有这个问题。
	for name_ in FONT_CANDIDATES:
		var path := OS.get_system_font_path(name_, 400, 100, false)
		if path != "" and FileAccess.file_exists(path):
			var ff := FontFile.new()
			if ff.load_dynamic_font(path) == OK:
				ff.antialiasing = TextServer.FONT_ANTIALIASING_GRAY
				ff.subpixel_positioning = TextServer.SUBPIXEL_POSITIONING_AUTO
				return ff

	# 3) 兜底：交给 fontconfig 自己挑（自绘文本在这种情况下可能缺字，见上面注释）
	push_warning("[gw2] 没找到候选系统字体，改用 SystemFont 兜底")
	var sys := SystemFont.new()
	sys.font_names = PackedStringArray(FONT_CANDIDATES)
	sys.allow_system_fallback = true
	sys.subpixel_positioning = TextServer.SUBPIXEL_POSITIONING_AUTO
	return sys


static func _flat(bg: Color, edge: Color, radius: int = 6, width: int = 1) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.border_color = edge
	sb.set_border_width_all(width)
	sb.set_corner_radius_all(radius)
	sb.content_margin_left = 12
	sb.content_margin_right = 12
	sb.content_margin_top = 6
	sb.content_margin_bottom = 6
	return sb


static func build() -> Theme:
	var t := Theme.new()
	t.default_font = font()
	t.default_font_size = BASE_FONT_SIZE

	# --- Button ---
	t.set_stylebox("normal", "Button", _flat(COL_PANEL, COL_PANEL_EDGE))
	t.set_stylebox("hover", "Button", _flat(Color("2c333c"), COL_ACCENT))
	t.set_stylebox("pressed", "Button", _flat(Color("1b2027"), COL_ACCENT))
	t.set_stylebox("disabled", "Button", _flat(Color("1b1f24"), Color("2a3038")))
	t.set_stylebox("focus", "Button", _flat(Color(0, 0, 0, 0), COL_ACCENT))
	t.set_color("font_color", "Button", COL_TEXT)
	t.set_color("font_hover_color", "Button", Color.WHITE)
	t.set_color("font_pressed_color", "Button", COL_ACCENT)
	t.set_color("font_disabled_color", "Button", Color("6b7480"))
	t.set_font_size("font_size", "Button", BASE_FONT_SIZE)

	# --- Label ---
	t.set_color("font_color", "Label", COL_TEXT)
	t.set_font_size("font_size", "Label", BASE_FONT_SIZE)

	# --- LineEdit / TextEdit ---
	t.set_stylebox("normal", "LineEdit", _flat(Color("101418"), COL_PANEL_EDGE))
	t.set_stylebox("focus", "LineEdit", _flat(Color("101418"), COL_ACCENT))
	t.set_color("font_color", "LineEdit", COL_TEXT)
	t.set_font_size("font_size", "LineEdit", BASE_FONT_SIZE)
	t.set_stylebox("normal", "TextEdit", _flat(Color("101418"), COL_PANEL_EDGE))
	t.set_color("font_color", "TextEdit", COL_TEXT)

	# --- Panel / PanelContainer ---
	t.set_stylebox("panel", "Panel", _flat(COL_PANEL, COL_PANEL_EDGE, 10))
	t.set_stylebox("panel", "PanelContainer", _flat(COL_PANEL, COL_PANEL_EDGE, 10))

	# --- TabContainer（PlayerScreen 的三个页签）---
	t.set_color("font_selected_color", "TabContainer", COL_ACCENT)
	t.set_color("font_unselected_color", "TabContainer", COL_TEXT_DIM)
	t.set_font_size("font_size", "TabContainer", BASE_FONT_SIZE)

	# --- RichTextLabel（聊天/函数列表/名字）---
	t.set_color("default_color", "RichTextLabel", COL_TEXT)
	t.set_font_size("normal_font_size", "RichTextLabel", 18)

	return t
