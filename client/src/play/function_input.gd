extends Control
class_name GwFunctionInput
## 函数输入框。原版是 Rust 类 `GraphwarFunctionInput`：**自绘，没有 LineEdit 子节点**
## （`docs/92` §4.10："Rust 自绘，无 LineEdit 子节点"），并且
## `text_changed → on_text_change`、`text_submitted → on_text_submitted` 两条连接指向 Rust
## （`docs/104` §6.3 的 #2/#3）。所以这里也用 `_draw()` 自绘，不摆 LineEdit。
##
## 编辑语义来自键盘按钮的 `input_text`（`docs/92` §4.10 的 18+18 个键）：
##   `C` 清空 · `⌫` 删光标左 · `⌦` 删光标右 · `←/→` 移光标 · `⇤/⇥` 行首/行尾 ·
##   其余按字面插入（`sin(`、`π`、`x`、`3`、`e`…）。
## 原版对这些键的**全部**行为没有逐条观测（例如 `C` 是否等于清空是推断）；
## 推断的依据是键名与函数文法的常识，见 docs/108 §7。

signal text_changed(new_text: String)
signal text_submitted(new_text: String)

## 长度上限：原版上限未知（未观测）。这里取 128，超过就拒绝输入并把边框标红。
const MAX_LENGTH := 128

## 只读态的文字色 = Godot 默认 `LineEdit/colors/font_uneditable_color`
## （`graphwar_theme.tres` 没有覆盖它 ⇒ 就是这个默认值；实测叠色见 `read_only` 的注释）。
const COL_TEXT_UNEDITABLE := Color(0.875, 0.875, 0.875, 0.5)

## 只读态那块 `(17,33,27)` = `#11211B` 的**来源**（T7 定案，`docs/06` §59）：
## **不是**自绘 —— 是 Godot **默认主题** `LineEdit/styles/read_only` 的
## `bg_color = (0.1, 0.1, 0.1, 0.3)` 叠在页面底色 `#0E251C`(14,37,28) 上的结果：
## `14+0.3·(25.5−14) = 17.45`、`37−0.3·11.5 = 33.55`、`28−0.3·2.5 = 27.25` ⇒ (17,33,27)。
## ⇒ 现在由 [`_line_edit_style`] 直接取那个样式盒来画，不再手填这个常数（留作对照/自检）。
const COL_BG_READ_ONLY := Color(17.0 / 255.0, 33.0 / 255.0, 27.0 / 255.0)

var text := "":
	set(value):
		if text == value:
			return
		text = value
		_caret = clampi(_caret, 0, text.length())
		queue_redraw()
		text_changed.emit(text)

var _caret := 0
var _caret_blink := 0.0
var _rejected := false
## 只读态（回放模式）：原版回放屏的函数框是**不可编辑**的，Godot 用
## `LineEdit/colors/font_uneditable_color`（默认 `(0.875, 0.875, 0.875, 0.5)`）画文字
## —— 实测真帧：函数串在 `x822..1181` 上的颜色 = `(120,128,125)`，
## 正好等于 0.875 灰 × α0.5 叠在输入框底色 `(17,33,27)` 上
## （`120=17+(223-17)*0.5`、`128=33+(223-33)*0.5`、`125=27+(223-27)*0.5`）。
var read_only := false:
	set(value):
		if read_only == value:
			return
		read_only = value
		# 只读/可编辑两态的**样式盒不同**（只读落到 Godot 默认主题的 `read_only`，
		# content_margin 4；可编辑用主题的 `normal`，边框 3）⇒ 最小高 98 / 96 也不同。
		update_minimum_size()
		queue_redraw()

func _ready() -> void:
	custom_minimum_size = Vector2(200, 44)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	mouse_filter = Control.MOUSE_FILTER_STOP
	set_process(true)
	focus_mode = Control.FOCUS_NONE

func _process(delta: float) -> void:
	_caret_blink += delta
	if _caret_blink > 0.55:
		_caret_blink = 0.0
		queue_redraw()

func caret_index() -> int:
	return _caret

## 键盘命令。`cmd` 就是按钮的 `input_text`。
func key_input(cmd: String) -> void:
	_rejected = false
	match cmd:
		"C":
			text = ""
			_caret = 0
		"⌫":
			if _caret > 0:
				text = text.substr(0, _caret - 1) + text.substr(_caret)
				_caret -= 1
		"⌦":
			if _caret < text.length():
				text = text.substr(0, _caret) + text.substr(_caret + 1)
		"←":
			_caret = maxi(0, _caret - 1)
		"→":
			_caret = mini(text.length(), _caret + 1)
		"⇤":
			_caret = 0
		"⇥":
			_caret = text.length()
		_:
			if text.length() + cmd.length() > MAX_LENGTH:
				_rejected = true
				return
			text = text.substr(0, _caret) + cmd + text.substr(_caret)
			_caret += cmd.length()
	queue_redraw()

func submit() -> void:
	text_submitted.emit(text)

func _gui_input(event: InputEvent) -> void:
	# 鼠标点一下把光标放到最近的位置（原版是否支持鼠标定位未观测）
	if event is InputEventMouseButton and (event as InputEventMouseButton).pressed:
		_caret = _caret_at_position((event as InputEventMouseButton).position.x)
		queue_redraw()

func _font() -> Font:
	return GwThemeFactory.font()

## 本控件画的是**哪个 LineEdit 样式盒**（原版 `GraphwarFunctionInput` 是 `LineEdit` 的 Rust
## 子类，见 `src/graphwar_function_input.rs` 文件头 ⇒ 它不自绘边框，用 `LineEdit/styles/*`）：
##
## * **可编辑** ⇒ 主题 `LineEdit/styles/normal` = `graphwar_theme.tres` 里那个 StyleBoxFlat
##   （`bg_color` 透明 + **3px 黑边** + 圆角 10）；
## * **只读**（回放屏的函数框是不可编辑的）⇒ `graphwar_theme.tres` **没有**定义
##   `LineEdit/styles/read_only` 这个键 ⇒ 落到 **Godot 默认主题**的那一个。这条能被真帧
##   逐通道验证（**观测**）：默认 `read_only` 的 `bg_color = (0.1, 0.1, 0.1, 0.3)`、
##   只有 **下边框 2 px**、content_margin 4 —— 叠在页面底色 `(14,37,28)` 上得
##   `14+0.3·11.5 = 17.45`、`37−0.3·11.5 = 33.55`、`28−0.3·2.5 = 27.25` ⇒ **(17,33,27)**，
##   与真帧输入框内部的整块颜色**三个通道全对上**；而真帧上这个框**左右上都没有黑边、
##   只有底部 2 行近黑** —— 正是"只有下边框 2 px"。
##
## ⇒ 改前我方在只读态自造了"透明底 + 3px 黑边"（等于可编辑态），于是底部行多出
## ~9 400 px 的假黑框、底色也缺一层（`docs/06` §59 的改前/改后数）。
func _line_edit_style() -> StyleBox:
	if read_only:
		var def := ThemeDB.get_default_theme()
		if def != null:
			var ro := def.get_stylebox("read_only", "LineEdit")
			if ro != null:
				return ro
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0, 0, 0, 0)
	sb.border_color = GwThemeFactory.COL_BAD if _rejected else GwThemeFactory.COL_PANEL_EDGE
	sb.set_border_width_all(GwThemeFactory.PANEL_BORDER)
	sb.set_corner_radius_all(GwThemeFactory.PANEL_RADIUS)
	return sb

## 最小尺寸：**LineEdit 的规则** = `字体高 + 样式盒的上下 content_margin`
## （`LineEdit::get_minimum_size` 的 `font->get_height(font_size) + style minimum size`）。
##
## 这条不是凑出来的：@1920 时字号 = 64、字体 `button_font` 的 `get_height(64) = 90`
## ⇒ 可编辑 90+2·3 = **96**、只读 90+2·4 = **98**。原版回放屏底栏（`HBoxContainer2`）
## 高 **98**（`docs/archive/125` §3.3 的 GEOM 表 + 真帧输入框底 `1062..1157`）——
## 我方改前拿"左上按钮键宽 = 96"当输入框最小高 ⇒ 底栏 96、整个 `PlayMainHBox` 变成 1060、
## 棋盘跟着下移 2 px（`docs/06` §58.3 那条"每条网格线整体错 1 px"的竖直分量）。
func _get_minimum_size() -> Vector2:
	var fs := _font_size()
	var style := _line_edit_style()
	return Vector2(0.0, _font().get_height(fs) + style.get_minimum_size().y)

## 字号变了（viewport resize）或只读开关变了都要让父容器重排。
func refresh_minimum_size() -> void:
	update_minimum_size()

## 字号 = 屏幕宽 / 30（原版一行函数串在 1920 下约 64 px 高，量自 `orig_play_default.png`）。
## 不取主题字号：`GwScreen.apply_font_scale()` 不会遍历到这个自绘控件。
func _font_size() -> int:
	var w := get_viewport_rect().size.x
	if w <= 0.0:
		w = size.x
	return maxi(10, int(w / 30.0))

## 宽度。
func _text_x(_font: Font, _fsize: int) -> float:
	return (size.x - _font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, _fsize).x) * 0.5

func _caret_at_position(local_x: float) -> int:
	var font := _font()
	var fsize := _font_size()
	var best := text.length()
	var origin := _text_x(font, fsize)
	for i in range(text.length() + 1):
		var w := font.get_string_size(text.substr(0, i), HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
		if origin + w >= local_x:
			best = i
			break
	return best

func _draw() -> void:
	var font := _font()
	var fsize := _font_size()
	var sb := _line_edit_style()
	sb.draw(get_canvas_item(), Rect2(Vector2.ZERO, size))

	# 基线：**LineEdit 的规则**（`LineEdit::_notification(NOTIFICATION_DRAW)`：
	# `content_top + (内容高 − 字体高)/2 + font_ascent`），不是"控件高的一半 + 0.35 em"。
	# 后者在 @1920 上比真帧高 3 px（实测：改前函数串墨迹 y1084..1146、原版 y1087..1149）。
	var style_min := sb.get_minimum_size()
	var content_top := sb.get_content_margin(SIDE_TOP)
	var content_h := size.y - style_min.y
	var baseline := content_top + maxf(0.0, (content_h - font.get_height(fsize)) * 0.5) \
		+ font.get_ascent(fsize)
	var origin := _text_x(font, fsize)
	# 原版 `alignment = 1` ⇒ 文本水平居中（实测 "-7.49" 的中心与控件中心重合）。
	draw_string(font, Vector2(origin, baseline), text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, fsize,
		COL_TEXT_UNEDITABLE if read_only else GwThemeFactory.COL_TEXT)
	var caret_x := origin + font.get_string_size(text.substr(0, _caret),
		HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
	if _caret_blink < 0.28:
		# 高度：真帧上原版光标是 3×90（bbox x1182–1184、y1066–1155）⇒ 正好是
		# **内容矩形**（样式盒上下各 content_margin 4 ⇒ 98−8 = 90）；宽度用主题的
		# `LineEdit/constants/caret_width = 3`。
		draw_line(Vector2(caret_x, content_top), Vector2(caret_x, size.y - sb.get_content_margin(SIDE_BOTTOM)),
			GwThemeFactory.COL_TEXT, 3.0)
