extends Control
class_name GwPlayerShowcase
## 玩家预览。原版是 Rust 类 `PlayerShowcase`（`docs/92` §4.14，自绘）。
## 这里按当前 skin/face/hat 画一个"士兵"：身体 = 皮肤色，脸 = 三种表情，
## 帽子 = 四类形状。**全部是现画的占位美术**（见 docs/108 §6）。

var skin := 0
var face := 0
var hat := 0

const SKIN_COLORS := [
	Color("8fb3d9"), Color("9ad9b3"), Color("d9c48f"),
	Color("d99a8f"), Color("b79ad9"), Color("d98fc4"),
	Color("7fd1a8"), Color("e0b25f"), Color("79a7c9"),
	Color("c98fa0"), Color("a0c98f"), Color("8f9fd9"),
	Color("d9a0c9"), Color("c9c98f"), Color("8fd9c9"),
]

func _ready() -> void:
	custom_minimum_size = Vector2(120, 160)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL

func set_look(s: int, f: int, h: int) -> void:
	skin = s
	face = f
	hat = h
	queue_redraw()

func _draw() -> void:
	var center := Vector2(size.x * 0.5, size.y * 0.62)
	var r := minf(size.x, size.y) * 0.22
	var body: Color = SKIN_COLORS[posmod(skin, SKIN_COLORS.size())]
	# 身体
	draw_circle(center + Vector2(0, r * 1.9), r * 1.25, body)
	# 头
	draw_circle(center, r, body)
	# 脸
	var ink := Color(0.1, 0.1, 0.12)
	draw_circle(center + Vector2(-r * 0.35, -r * 0.15), r * 0.12, ink)
	draw_circle(center + Vector2(r * 0.35, -r * 0.15), r * 0.12, ink)
	match posmod(face, 3):
		0:
			draw_arc(center + Vector2(0, r * 0.25), r * 0.35, 0.35, PI - 0.35, 12, ink, 2.0)
		1:
			draw_line(center + Vector2(-r * 0.35, r * 0.42), center + Vector2(r * 0.35, r * 0.42), ink, 2.0)
		_:
			draw_circle(center + Vector2(0, r * 0.4), r * 0.14, ink)
	# 帽子
	match posmod(hat, 4):
		0:
			draw_rect(Rect2(center + Vector2(-r * 1.05, -r * 0.85), Vector2(r * 2.1, r * 0.3)), Color(0.35, 0.4, 0.5))
		1:
			draw_rect(Rect2(center + Vector2(-r * 0.75, -r * 1.55), Vector2(r * 1.5, r * 0.8)), Color(0.35, 0.4, 0.5))
		2:
			draw_colored_polygon(PackedVector2Array([
				center + Vector2(-r * 0.95, -r * 0.75),
				center + Vector2(0, -r * 1.9),
				center + Vector2(r * 0.95, -r * 0.75),
			]), Color(0.35, 0.4, 0.5))
		_:
			draw_rect(Rect2(center + Vector2(-r * 0.5, -r * 1.8), Vector2(r, r * 1.0)), Color(0.35, 0.4, 0.5))
	if hat < 0:
		return
