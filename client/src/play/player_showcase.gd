extends Control
class_name GwPlayerShowcase
## 玩家预览。原版是 Rust 类 `PlayerShowcase`（`docs/92` §4.14，自绘）。
##
## 合成方式照原版战场里的做法（`docs/92` §4.10 观测）：**三层贴图叠在同一个矩形上**
## —— 皮肤 / 脸 / 帽子，三张 svg 共用 512×512 的画布，直接同位置叠画就是原版的合成结果。
##
## 图不在这里画：`GwIcons` 从使用者自己的 `Graphwar.pck` 解出来的资产里取
## （`docs/121` §5）。缺资产时什么都不画并报错 —— 不再用几何图形冒充原版美术。

var skin := 0
var face := 0
var hat := 0

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
	var side := minf(size.x, size.y) * 0.92
	if side <= 0.0:
		return
	var rect := Rect2((size - Vector2(side, side)) * 0.5, Vector2(side, side))
	for name in ["skin_%d" % skin, "face_%d" % face, "hat_%d" % hat]:
		var tex := GwIcons.get_icon(name)
		if tex != null:
			draw_texture_rect(tex, rect, false)
