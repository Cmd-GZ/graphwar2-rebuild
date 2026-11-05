extends RefCounted
class_name GwIcons
## 图标：**全部现画**，不使用 `rsc/` 里的任何 svg。
##
## 原版用 149 个 svg（游戏资产，不可分发）。本项目用一个小 SVG 生成器画同**语义**的
## 几何占位图：形状是我们自己写的，颜色来自 `theme_factory.gd` 的配色。
## 因此界面上图标位置、按钮尺寸都对得上，但美术观感与原版不同 —— 这是有意的（见 docs/108 §6）。
##
## 缓存：同一个名字只生成一次。

static var _cache := {}

const SIZE := 48

static func get_icon(name: String) -> Texture2D:
	if _cache.has(name):
		return _cache[name]
	var tex := _render(name)
	_cache[name] = tex
	return tex

static func _render(name: String) -> Texture2D:
	var body := _body_for(name)
	var svg := ('<svg width="%d" height="%d" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg">%s</svg>'
		% [SIZE, SIZE, body])
	var img := Image.new()
	var err := img.load_svg_from_string(svg, 1.0)
	if err != OK:
		push_warning("[gw2] 图标 %s 生成失败（%d），用占位方块" % [name, err])
		img = Image.create(SIZE, SIZE, false, Image.FORMAT_RGBA8)
		img.fill(Color(0.5, 0.5, 0.5, 0.6))
	return ImageTexture.create_from_image(img)

# ------------------------------------------------------------------ 形状素材

static func _ink(c: Color) -> String:
	return "#%s" % c.to_html(false)

static func _rect(x: float, y: float, w: float, h: float, fill: Color, r: float = 2.0) -> String:
	return ('<rect x="%.1f" y="%.1f" width="%.1f" height="%.1f" rx="%.1f" fill="%s"/>'
		% [x, y, w, h, r, _ink(fill)])

static func _line(x1: float, y1: float, x2: float, y2: float, c: Color, w: float = 4.0) -> String:
	return ('<line x1="%.1f" y1="%.1f" x2="%.1f" y2="%.1f" stroke="%s" stroke-width="%.1f" stroke-linecap="round"/>'
		% [x1, y1, x2, y2, _ink(c), w])

static func _circle(cx: float, cy: float, r: float, c: Color) -> String:
	return '<circle cx="%.1f" cy="%.1f" r="%.1f" fill="%s"/>' % [cx, cy, r, _ink(c)]

static func _poly(points: Array, c: Color) -> String:
	var s := ""
	for i in range(0, points.size(), 2):
		s += "%.1f,%.1f " % [points[i], points[i + 1]]
	return '<polygon points="%s" fill="%s"/>' % [s.strip_edges(), _ink(c)]

static func _path(d: String, c: Color, w: float = 4.0) -> String:
	return ('<path d="%s" fill="none" stroke="%s" stroke-width="%.1f" stroke-linecap="round" stroke-linejoin="round"/>'
		% [d, _ink(c), w])

# ------------------------------------------------------------------ 具体图标

static func _body_for(name: String) -> String:
	var ink := GwThemeFactory.COL_TEXT
	var acc := GwThemeFactory.COL_ACCENT
	match name:
		"return":
			return _path("M30 10 L16 24 L30 38", ink) + _line(16, 24, 40, 24, ink)
		"plus", "add_soldier":
			return _line(24, 12, 24, 36, acc) + _line(12, 24, 36, 24, acc)
		"minus", "remove_soldier":
			return _line(12, 24, 36, 24, GwThemeFactory.COL_BAD)
		"x", "remove_player":
			return _line(14, 14, 34, 34, GwThemeFactory.COL_BAD) + _line(34, 14, 14, 34, GwThemeFactory.COL_BAD)
		"arrow_left":
			return _path("M28 12 L14 24 L28 36", ink)
		"arrow_right":
			return _path("M20 12 L34 24 L20 36", ink)
		"arrow_left_bar":
			return _line(12, 12, 12, 36, acc) + _path("M34 12 L20 24 L34 36", ink)
		"arrow_right_bar":
			return _line(36, 12, 36, 36, acc) + _path("M14 12 L28 24 L14 36", ink)
		"delete_left":
			return _path("M32 14 L20 24 L32 34", GwThemeFactory.COL_BAD) + _line(20, 24, 8, 24, GwThemeFactory.COL_BAD)
		"delete_right":
			return _path("M16 14 L28 24 L16 34", GwThemeFactory.COL_BAD) + _line(28, 24, 40, 24, GwThemeFactory.COL_BAD)
		"unlock":
			return _rect(12, 22, 24, 18, GwThemeFactory.COL_ACCENT) + _path("M18 22 L18 15 A6 6 0 0 1 30 15", acc, 3.0)
		"lock":
			return _rect(12, 22, 24, 18, GwThemeFactory.COL_ACCENT) + _path("M18 22 L18 15 A6 6 0 0 1 30 15 A6 6 0 0 1 18 15", acc, 3.0)
		"no_chat":
			return _rect(8, 12, 32, 22, GwThemeFactory.COL_PANEL_EDGE, 5.0) + _poly([18, 34, 26, 34, 20, 42], GwThemeFactory.COL_PANEL_EDGE)
		"new_chat":
			return (_rect(8, 12, 32, 22, GwThemeFactory.COL_ACCENT, 5.0)
				+ _poly([18, 34, 26, 34, 20, 42], acc)
				+ _circle(36, 12, 6, GwThemeFactory.COL_BAD))
		"show_extra_buttons", "view_functions":
			return (_line(10, 16, 38, 16, ink) + _line(10, 24, 38, 24, ink)
				+ _line(10, 32, 26, 32, ink) + _circle(34, 32, 5, acc))
		"hide_extra_buttons":
			return (_line(10, 16, 38, 16, ink) + _line(10, 24, 38, 24, ink)
				+ _line(10, 32, 26, 32, GwThemeFactory.COL_TEXT_DIM))
		"view_names":
			return (_circle(18, 20, 7, acc) + _path("M8 38 A10 10 0 0 1 28 38", acc)
				+ _line(32, 18, 42, 18, ink) + _line(32, 26, 42, 26, ink) + _line(32, 34, 42, 34, ink))
		"hide_names":
			return (_circle(18, 20, 7, GwThemeFactory.COL_TEXT_DIM) + _path("M8 38 A10 10 0 0 1 28 38", GwThemeFactory.COL_TEXT_DIM)
				+ _line(32, 24, 42, 24, GwThemeFactory.COL_TEXT_DIM))
		"unmute":
			return (_poly([14, 18, 20, 18, 28, 12, 28, 36, 20, 30, 14, 30], acc)
				+ _path("M32 18 A8 8 0 0 1 32 30", ink, 3.0))
		"mute":
			return (_poly([14, 18, 20, 18, 28, 12, 28, 36, 20, 30, 14, 30], GwThemeFactory.COL_TEXT_DIM)
				+ _line(32, 18, 40, 30, GwThemeFactory.COL_BAD) + _line(40, 18, 32, 30, GwThemeFactory.COL_BAD))
		"add_bot_option":
			return (_circle(24, 18, 9, GwThemeFactory.COL_ACCENT)
				+ _path("M8 42 A16 12 0 0 1 40 42", acc) + _line(36, 12, 44, 12, ink))
		"play_option":
			return _poly([16, 10, 38, 24, 16, 38], acc)
		"play_pause":
			return _rect(14, 12, 6, 24, acc) + _rect(28, 12, 6, 24, acc)
		"prev":
			return _line(12, 12, 12, 36, acc) + _poly([38, 12, 22, 24, 38, 36], ink)
		"next":
			return _line(36, 12, 36, 36, acc) + _poly([10, 12, 26, 24, 10, 36], ink)
		"save_screenshot":
			return _rect(8, 14, 32, 24, GwThemeFactory.COL_PANEL_EDGE) + _circle(24, 26, 7, acc)
		"save_summary":
			return _rect(12, 8, 24, 32, GwThemeFactory.COL_PANEL_EDGE) + _line(18, 18, 30, 18, ink) + _line(18, 26, 30, 26, ink)
		"save_clip":
			return _rect(8, 12, 32, 24, GwThemeFactory.COL_PANEL_EDGE) + _poly([20, 18, 30, 24, 20, 30], acc)
		"turn_inf":
			return _path("M10 24 A14 14 0 1 0 24 10", acc, 3.5) + _poly([10, 6, 10, 16, 20, 16], acc)
		"empty_soldier":
			return _circle(24, 16, 8, Color(1, 1, 1, 0.10)) + _rect(12, 26, 24, 14, Color(1, 1, 1, 0.10), 6.0)
	if name.begins_with("bot_"):
		# Bot 头像：圆脸 + 档位序号点阵。序号只是视觉区分，与难度对应关系未证实。
		var tier := int(name.substr(4))
		var col := _tier_color(tier)
		var s := _circle(24, 24, 18, col) + _circle(18, 20, 3, Color(0.08, 0.09, 0.10)) + _circle(30, 20, 3, Color(0.08, 0.09, 0.10))
		for i in tier:
			s += _circle(14 + i * 6, 33, 2, Color(0.08, 0.09, 0.10))
		return s
	if name.begins_with("skin_"):
		var idx := int(name.substr(5))
		var c2 := _tier_color(idx % 6)
		return (_circle(24, 15, 8, c2) + _rect(13, 25, 22, 15, c2, 7.0))
	if name.begins_with("face_"):
		var idx2 := int(name.substr(5))
		var eyes := _circle(18, 22, 3, Color(0.08, 0.09, 0.10)) + _circle(30, 22, 3, Color(0.08, 0.09, 0.10))
		match idx2 % 3:
			0: return _circle(24, 24, 17, Color(0.85, 0.75, 0.55)) + eyes + _path("M17 32 Q24 37 31 32", Color(0.2, 0.15, 0.1), 2.5)
			1: return _circle(24, 24, 17, Color(0.85, 0.75, 0.55)) + eyes + _line(17, 33, 31, 33, Color(0.2, 0.15, 0.1), 2.5)
			_: return _circle(24, 24, 17, Color(0.85, 0.75, 0.55)) + eyes + _circle(24, 33, 3, Color(0.2, 0.15, 0.1))
	if name.begins_with("hat_"):
		var idx3 := int(name.substr(4))
		var body2 := _circle(24, 28, 15, Color(0.6, 0.6, 0.65))
		match idx3 % 4:
			0: return body2 + _rect(6, 26, 36, 5, Color(0.35, 0.4, 0.5))
			1: return body2 + _rect(10, 14, 28, 14, Color(0.35, 0.4, 0.5), 7.0)
			2: return body2 + _poly([10, 28, 24, 8, 38, 28], Color(0.35, 0.4, 0.5))
			_: return body2 + _rect(16, 10, 16, 16, Color(0.35, 0.4, 0.5), 4.0)
	if name.begins_with("axis_option_"):
		var n := int(name.substr(12))
		var s2 := _line(10, 24, 38, 24, GwThemeFactory.COL_TEXT_DIM, 2.5) + _line(24, 10, 24, 38, GwThemeFactory.COL_TEXT_DIM, 2.5)
		for i in n:
			s2 += _circle(12 + i * 6, 34, 2.2, acc)
		return s2
	if name.begins_with("turn_"):
		# 时限图标：一个钟面 + 刻度数量 = 档位；`turn_inf` 已在上面单独处理
		var idx4 := 0
		match name:
			"turn_30s": idx4 = 1
			"turn_1m": idx4 = 2
			"turn_2m": idx4 = 3
			"turn_3m": idx4 = 4
			"turn_5m": idx4 = 5
		var s3 := _circle(24, 24, 15, GwThemeFactory.COL_PANEL_EDGE) + _line(24, 24, 24, 14, acc, 3.0)
		for i in idx4:
			var ang := -PI / 2.0 + i * (TAU / 6.0)
			s3 += _circle(24 + cos(ang) * 15, 24 + sin(ang) * 15, 2.2, GwThemeFactory.COL_TEXT)
		return s3
	match name:
		"turns_sequential":
			return _rect(8, 12, 12, 10, acc) + _rect(28, 26, 12, 10, GwThemeFactory.COL_TEAM2) + _line(20, 17, 28, 31, GwThemeFactory.COL_TEXT_DIM, 2.5)
		"turns_simultaneous":
			return (_rect(8, 12, 12, 10, acc) + _rect(28, 12, 12, 10, GwThemeFactory.COL_TEAM2)
				+ _rect(8, 28, 12, 10, acc) + _rect(28, 28, 12, 10, GwThemeFactory.COL_TEAM2))
		"fx_option":
			return _path("M8 34 C 16 8, 30 8, 40 34", acc, 4.0)
		"dx_dy_option":
			return (_path("M10 30 C 18 30, 20 18, 24 18 S 30 30, 38 30", acc, 4.0)
				+ _line(8, 38, 40, 38, GwThemeFactory.COL_TEXT_DIM, 2.0))
		"show_extra":
			return _path("M14 34 L24 14 L34 34", acc)
		"gold_coin":
			return _circle(24, 24, 16, Color(0.85, 0.7, 0.25))
	return _circle(24, 24, 14, GwThemeFactory.COL_TEXT_DIM)

static func _tier_color(tier: int) -> Color:
	var palette := [
		Color("8fb3d9"), Color("9ad9b3"), Color("d9c48f"),
		Color("d99a8f"), Color("b79ad9"), Color("d98fc4"),
	]
	return palette[wrapi(tier, 0, palette.size())]
