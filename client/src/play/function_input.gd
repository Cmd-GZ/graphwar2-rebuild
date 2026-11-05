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
	# 原版输入框 = 主题的 LineEdit 样式：**透明底** + 3px 黑边 + 圆角 10
	# （`graphwar_theme.tres` 的 StyleBoxFlat；实测边界 x=262..1742 内只有 3px 黑框，
	#  框内颜色与 clear_color 相同 ⇒ 没有自己的底色）。
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0, 0, 0, 0)
	sb.border_color = GwThemeFactory.COL_BAD if _rejected else GwThemeFactory.COL_PANEL_EDGE
	sb.set_border_width_all(GwThemeFactory.PANEL_BORDER)
	sb.set_corner_radius_all(GwThemeFactory.PANEL_RADIUS)
	sb.draw(get_canvas_item(), Rect2(Vector2.ZERO, size))

	var baseline := size.y * 0.5 + fsize * 0.35
	var origin := _text_x(font, fsize)
	# 原版 `alignment = 1` ⇒ 文本水平居中（实测 "-7.49" 的中心与控件中心重合）。
	draw_string(font, Vector2(origin, baseline), text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, fsize,
		GwThemeFactory.COL_TEXT)
	var caret_x := origin + font.get_string_size(text.substr(0, _caret),
		HORIZONTAL_ALIGNMENT_LEFT, -1, fsize).x
	if _caret_blink < 0.28:
		draw_line(Vector2(caret_x, size.y * 0.18), Vector2(caret_x, size.y * 0.82),
			GwThemeFactory.COL_TEXT, maxf(1.5, size.y * 0.04))
