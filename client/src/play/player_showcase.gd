extends Control
class_name GwPlayerShowcase
## 玩家预览。原版是 Rust 类 `PlayerShowcase`（`docs/92` §4.14，自绘）。
##
## 合成方式照原版战场里的做法（`docs/92` §4.10 观测）：**三层贴图叠在同一个矩形上**
## —— 皮肤 / 脸 / 帽子，三张图共用 512×512 的画布，直接同位置叠画就是原版的合成结果。
##
## 图不在这里画：`GwIcons` 从使用者自己的 `Graphwar.pck` 解出来的资产里取
## （`docs/121` §5）。缺资产时什么都不画并报错 —— 不再用几何图形冒充原版美术。
##
## **尺寸规则（观测，docs/129 §1.3）**：原版把 512² 的画布铺满 `PlayerShowcase` 的整个矩形，
## 所以画布里的实心圆（`skins/*.ctex` 里直径 240/512 = 46.875%）在屏幕上就是矩形的 46.875%。
## 1920×1160 真帧实测：预览矩形 ≈ 933×933 → 圆 437 px；格子按钮的内容区 297 px → 圆 139 px。
## 两处都是同一条 46.875%，可见原版只是"把整张 512² 贴满控件矩形"。
## 我们以前乘了个 0.92 的魔法系数，于是预览比原版小一圈。

var skin := 0
var face := 0
var hat := 0

func _ready() -> void:
	# 不给最小尺寸：原版这个 Rust 节点不自带尺寸，矩形完全由外层 `AspectRatioContainer`
	# 决定（`player_screen.tscn`）。给了最小尺寸会把右列撑变形。
	custom_minimum_size = Vector2.ZERO
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL

func set_look(s: int, f: int, h: int) -> void:
	skin = s
	face = f
	hat = h
	queue_redraw()

func _draw() -> void:
	# 画布 = 控件矩形里居中的正方形（外层是比例 1 的 AspectRatioContainer；这里只是不让
	# 非方形时把图画扁）。
	var side := minf(size.x, size.y)
	if side <= 0.0:
		return
	var rect := Rect2((size - Vector2(side, side)) * 0.5, Vector2(side, side))
	# 顺序 = 层序：皮肤 → 脸 → 帽子（后画压在上面）。`hats/no_hat` 是全透明的空图，
	# 所以"没有帽子"与默认外观走同一条路径。
	for icon_name in ["skin_%d" % skin, "face_%d" % face, "hat_%d" % hat]:
		var tex := GwIcons.get_icon(icon_name)
		if tex != null:
			draw_texture_rect(tex, rect, false)
