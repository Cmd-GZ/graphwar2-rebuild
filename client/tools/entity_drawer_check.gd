extends SceneTree
## `EntityDrawer` 真节点类的验收脚本（`docs/16-client.md` §37）。
##
## ```text
## cargo build --release --features gdext-bindings
## cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/entity_drawer_check.gd
## ```
##
## 它验证的是「**复原的纯逻辑能不能真的变成一棵节点树**」这一句，不验证复原逻辑本身：
##
## 1. 类注册成 `Control` 子类，9 个方法都在，两个导出属性可读写；
## 2. `spawn_entity()` 之后**子节点个数 == 4**，类型是 3 个 `Sprite2D` + 1 个 `RichTextLabel`；
## 3. 三个 `Sprite2D.texture` 都非空、都是 `Texture2D`、三层互不相同（程序生成的 `ImageTexture`）；
## 4. 位置 / 缩放 / 翻面 == `entity_drawer::sprite_transform` 的手算期望（§37.3 那张表）；
## 5. 名字标签的属性 == `recovered-src/name_label.tscn`（`fit_content` / `scroll_active` /
##    `autowrap_mode` / 初始尺寸 25×40），文本与不透明度跟着 `set_entity_name` /
##    `set_entity_dead_ticks` 走；
## 6. 进树之后 `_ready` / `_process` / `resized` **真的被引擎调到**，尺寸变了会重摆；
## 7. `remove_entity()` 的 `queue_free` 真的释放了那 4 个节点。
##
## 与 `client/tools/gdext_check.gd` 的分工：那个测「换绑定层对客户端透明」，这个测
## 「`EntityDrawer` 能不能长出节点树」。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const NODE_CLASS := "EntityDrawer"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 控件尺寸与队伍色：手算期望值的三个输入（§37.3）。
## 纯红 `0xff` ⇒ 打包色低字节是 R、G 位不置顶 ⇒ `entity_drawer::team_mirror` 为 false ⇒ 不翻面。
const W := 500.0
const H := 300.0
const TEAM_COLOR := 0xff

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## `entity_drawer::sprite_transform` 的 GDScript 版（**独立重算，不调 Rust**）。
##
## ```text
## k = size.x / 50
## position = ( size.x/2 + s·k·x ,  size.y/2 − k·y )
## scale    = ( (s/60)·(128/tex_w) ,  (k/−60)·(128/tex_w) )
## flip_h   = x > 0
## ```
## 它与 `src/entity_drawer.rs::sprite_transform` 是**同一条算式**——两边算出来不一样就是有一边错了。
func expect_sprite(size: Vector2, packed_color: int, tex_width: int, x: float, y: float) -> Dictionary:
	var k: float = size.x / 50.0
	var s: float = -1.0 if _team_mirror(packed_color) else 1.0
	var unit: float = 128.0 / float(tex_width)
	return {
		"position": Vector2(size.x * 0.5 + s * k * x, size.y * 0.5 - k * y),
		"scale": Vector2((s / 60.0) * unit, (k / -60.0) * unit),
		"flip_h": x > 0.0,
	}

## `entity_drawer::team_mirror` 的 GDScript 版（同一条判据）。
func _team_mirror(packed_color: int) -> bool:
	var green_bit: bool = (packed_color & 0x101) == 0x100
	var alpha: int = 0 if (packed_color & 0x10000) != 0 else (packed_color >> 24)
	return green_bit or (alpha & 1) != 0

## `entity_drawer::name_label_layout` 的 GDScript 版（**独立重算**）。
##
## 入参 `label` 只用来读两个引擎值：`size`（= 原版的 `label_size`）与 `get_minimum_size()`
## （= 原版的 `mw/mh`，`fit_content = true` 时它跟着文本走）。
func expect_label_position(label: RichTextLabel, packed_color: int, x: float, y: float) -> Vector2:
	var w: float = label.size.x
	var h: float = label.size.y
	var mw: float = label.get_minimum_size().x
	var mh: float = label.get_minimum_size().y
	var k: float = w / 50.0
	var s: float = -1.0 if _team_mirror(packed_color) else 1.0

	var cx: float = w * 0.5 + s * k * x - mw * 0.5
	cx = maxf(cx, 0.0)
	var px: float = cx
	if mw + cx > w:
		px = w - mw

	var base_y: float = (h * 0.5 - k * y) + k * (-1.5) - mh * 0.5
	var py: float = base_y
	if base_y < 0.0:
		py = base_y + (k * 1.75 + mh * 0.5)
	return Vector2(px, py)

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	if not ClassDB.class_exists(NODE_CLASS):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	ok(ClassDB.class_exists(NODE_CLASS), "ClassDB.class_exists(\"%s\")" % NODE_CLASS)
	if not ClassDB.class_exists(NODE_CLASS):
		_finish()
		return

	# 1) 实例 + 属性 + 方法表
	var drawer: Object = ClassDB.instantiate(NODE_CLASS)
	ok(drawer != null and drawer is Control, "%s 能实例化且是 Control" % NODE_CLASS)
	if drawer == null:
		_finish()
		return
	print("      drawer.get_class() = %s" % drawer.get_class())
	ok(drawer.is_class("Control"), "is_class(\"Control\") 为真（父类正确）")

	var wanted := [
		"spawn_entity", "remove_entity", "entity_count", "relayout",
		"set_entity_name", "set_entity_dead_ticks",
		"ready_call_count", "process_call_count", "resized_call_count",
	]
	var names := PackedStringArray()
	for m in drawer.get_method_list():
		names.append(String(m.name))
	for w in wanted:
		ok(names.has(w), "方法 %s() 存在" % w)

	drawer.set("team_color", TEAM_COLOR)
	ok(int(drawer.get("team_color")) == TEAM_COLOR, "属性 team_color 往返一致（%d）" % TEAM_COLOR)
	drawer.set("texture_size", 8)
	ok(int(drawer.get("texture_size")) == 8, "属性 texture_size 往返一致（8）")

	# 2) 摆位置用的输入：控件尺寸（原版 `process` 每帧读的就是它）
	var control: Control = drawer
	control.set_size(Vector2(W, H))
	ok(control.size.is_equal_approx(Vector2(W, H)), "控件尺寸设为 %s" % Vector2(W, H))

	# 3) spawn：一个实体 = 4 个子节点
	var created: int = int(drawer.call("spawn_entity", 0, 5.0, 5.0))
	ok(created == 4, "spawn_entity(0, 5, 5) 返回 4（建了几个子节点）")
	ok(control.get_child_count() == 4, "子节点个数 == 4（实际 %d）" % control.get_child_count())

	var kids: Array = []
	for i in range(control.get_child_count()):
		kids.append(control.get_child(i))
	ok(kids.size() == 4, "能取到 4 个子节点")
	if kids.size() != 4:
		_finish()
		return

	# 4) 类型：前三个 Sprite2D、第四个 RichTextLabel（顺序 = `entity_drawer::ENTITY_CHILD_NODES`）
	ok(kids[0] is Sprite2D, "子节点 0 是 Sprite2D（皮肤）")
	ok(kids[1] is Sprite2D, "子节点 1 是 Sprite2D（脸）")
	ok(kids[2] is Sprite2D, "子节点 2 是 Sprite2D（帽子）")
	ok(kids[3] is RichTextLabel, "子节点 3 是 RichTextLabel（名字）")
	print("      子节点名: %s / %s / %s / %s"
		% [kids[0].name, kids[1].name, kids[2].name, kids[3].name])
	# 命名也照原版的节点树 dump（`docs/16` §2.1.10）：三个 sprite 是**引擎自动名**
	# `@Sprite2D@N`，名字标签是场景根名 `RichTextLabel`。
	ok(String(kids[0].name).begins_with("@Sprite2D@"), "子节点 0 是引擎自动名 @Sprite2D@N（原版 dump 如此）")
	ok(String(kids[1].name).begins_with("@Sprite2D@"), "子节点 1 是引擎自动名 @Sprite2D@N")
	ok(String(kids[2].name).begins_with("@Sprite2D@"), "子节点 2 是引擎自动名 @Sprite2D@N")
	ok(String(kids[3].name).begins_with("RichTextLabel"),
		"子节点 3 叫 RichTextLabel*（= name_label.tscn 的根节点名；同名冲突时引擎会加 @N）")

	# 5) 贴图：程序生成的 ImageTexture，非空、是 Texture2D、三层不同
	var textures: Array = []
	for i in range(3):
		var tex: Variant = kids[i].texture
		ok(tex != null, "子节点 %d 的 texture 非空" % i)
		ok(tex is Texture2D, "子节点 %d 的 texture 是 Texture2D" % i)
		if tex is Texture2D:
			ok(int(tex.get_width()) == 8 and int(tex.get_height()) == 8,
				"子节点 %d 的贴图是 8×8（%d×%d）" % [i, tex.get_width(), tex.get_height()])
			textures.append(tex.get_instance_id())
		else:
			textures.append(-1)
	# 三个 sprite 的两个原版常量
	ok(kids[0].flip_v == true and kids[1].flip_v == true and kids[2].flip_v == true,
		"三个 Sprite2D 都 flip_v = true（entity_drawer::SPRITE_FLIP_V）")
	ok(int(kids[0].texture_filter) == 4 and int(kids[1].texture_filter) == 4
			and int(kids[2].texture_filter) == 4,
		"三个 Sprite2D 都 texture_filter = 4（entity_drawer::SPRITE_TEXTURE_FILTER）")
	ok(textures.size() == 3 and textures[0] != textures[1] and textures[1] != textures[2]
			and textures[0] != textures[2],
		"三层贴图互不相同（皮肤 / 脸 / 帽子）")

	# 6) 位置 / 缩放 / 翻面 == 手算期望（§37.3）
	var want: Dictionary = expect_sprite(Vector2(W, H), TEAM_COLOR, 8, 5.0, 5.0)
	print("      sprite0 position=%s scale=%s flip_h=%s"
		% [kids[0].position, kids[0].scale, kids[0].flip_h])
	print("      期望   position=%s scale=%s flip_h=%s"
		% [want["position"], want["scale"], want["flip_h"]])
	ok(kids[0].position.is_equal_approx(want["position"]),
		"位置 == (300, 100)（k=10: 250+10·5, 150−10·5）")
	ok(kids[0].scale.is_equal_approx(want["scale"]),
		"缩放 == (16/60, −160/60)（unit = 128/8）")
	ok(kids[0].flip_h == true, "x > 0 ⇒ flip_h = true")
	ok(kids[1].position.is_equal_approx(kids[0].position)
			and kids[2].position.is_equal_approx(kids[0].position),
		"三层 sprite 位置相同（同一具士兵）")

	# 7) 名字标签：属性照 `recovered-src/name_label.tscn`
	var label: RichTextLabel = kids[3]
	ok(label.fit_content == true, "fit_content = true（name_label.tscn）")
	ok(label.scroll_active == false, "scroll_active = false（name_label.tscn）")
	ok(int(label.autowrap_mode) == 0, "autowrap_mode = 0 = OFF（name_label.tscn）")
	ok(label.size.is_equal_approx(Vector2(25.0, 40.0)),
		"初始尺寸 25×40（offset_right / offset_bottom，实际 %s）" % label.size)

	drawer.call("set_entity_name", 0, "Bob")
	ok(String(label.text) == "Bob", "set_entity_name 后 text == \"Bob\"（实际 \"%s\"）" % label.text)
	var want_label: Vector2 = expect_label_position(label, TEAM_COLOR, 5.0, 5.0)
	print("      label position=%s（期望 %s）min_size=%s"
		% [label.position, want_label, label.get_minimum_size()])
	ok(label.position.is_equal_approx(want_label), "名字标签位置 == name_label_layout 的手算结果")

	# 8) 死亡淡出：alpha 来自 `entity_drawer::death_alpha`（0→1.0 / 30→0.5 / ≥60→0）
	var a30: float = float(drawer.call("set_entity_dead_ticks", 0, 30))
	ok(is_equal_approx(a30, 0.5), "死了 30 刻 ⇒ alpha = 0.5（实际 %f）" % a30)
	ok(is_equal_approx(label.modulate.a, 0.5), "label.modulate.a == 0.5（实际 %f）" % label.modulate.a)
	var a60: float = float(drawer.call("set_entity_dead_ticks", 0, 60))
	ok(is_equal_approx(a60, 0.0) and label.visible == false, "死了 ≥60 刻 ⇒ alpha = 0 且标签隐藏")
	var a_back: float = float(drawer.call("set_entity_dead_ticks", 0, -1))
	ok(is_equal_approx(a_back, 1.0) and label.visible == true, "复活（ticks = −1）⇒ alpha = 1 且标签可见")

	# 9) 第二个实体：独立布局（x < 0 ⇒ 不翻面）
	var created2: int = int(drawer.call("spawn_entity", 1, -5.0, 3.5))
	ok(created2 == 4, "第二个实体也建 4 个子节点")
	ok(int(drawer.call("entity_count")) == 2, "entity_count() == 2")
	ok(control.get_child_count() == 8, "两个实体 ⇒ 8 个子节点（实际 %d）" % control.get_child_count())
	var s1: Sprite2D = control.get_child(4)
	var want2: Dictionary = expect_sprite(Vector2(W, H), TEAM_COLOR, 8, -5.0, 3.5)
	print("      sprite4 position=%s flip_h=%s" % [s1.position, s1.flip_h])
	ok(s1.position.is_equal_approx(want2["position"]), "第二个实体位置 == (200, 115)")
	ok(s1.flip_h == false, "x < 0 ⇒ flip_h = false")

	# 10) remove_entity：queue_free 真的释放（要等一帧，延迟释放）
	var doomed: Node = control.get_child(7)
	ok(drawer.call("remove_entity", 1) == true, "remove_entity(1) 返回 true")
	ok(drawer.call("remove_entity", 1) == false, "remove_entity(1) 再调一次返回 false（不报错）")
	ok(int(drawer.call("entity_count")) == 1, "entity_count() 回到 1")
	await process_frame
	await process_frame
	ok(control.get_child_count() == 4,
		"queue_free 之后回到 4 个子节点（实际 %d）" % control.get_child_count())
	ok(not is_instance_valid(doomed), "被移除实体的子节点已经真的被释放（is_instance_valid == false）")

	# 11) GDScript 的 `EntityDrawer.new()`：第三条实例化路径
	var fresh: Object = EntityDrawer.new()
	ok(fresh != null and fresh is Control, "EntityDrawer.new() 出来的是 Control")
	if fresh != null:
		var fresh_control: Control = fresh
		fresh_control.set_size(Vector2(W, H))
		ok(int(fresh.call("spawn_entity", 7, 1.0, 2.0)) == 4, "新实例也能 spawn_entity")
		ok(fresh_control.get_child_count() == 4, "新实例有 4 个子节点")
		fresh_control.free()

	# 12) 进树：`_ready` / `_process` / `resized` 真的被引擎调到
	root.add_child(control)
	ok(control.is_inside_tree(), "节点能挂进场景树")
	await process_frame
	await process_frame
	var ready_calls: int = int(drawer.call("ready_call_count"))
	var process_calls: int = int(drawer.call("process_call_count"))
	print("      引擎回调: ready=%d process=%d resized=%d"
		% [ready_calls, process_calls, int(drawer.call("resized_call_count"))])
	ok(ready_calls >= 1, "_ready 被引擎调到（%d 次）" % ready_calls)
	ok(process_calls >= 1, "_process 被引擎调到（%d 次；实测不用 set_process，Godot 4.7 自己开）" % process_calls)

	# 13) 尺寸变了 ⇒ `resized` ⇒ 重摆（k 从 10 变 20）
	control.set_size(Vector2(W * 2.0, H * 2.0))
	await process_frame
	var resized_calls: int = int(drawer.call("resized_call_count"))
	ok(resized_calls >= 1, "resized 信号真的连上了（on_resized 被调到 %d 次）" % resized_calls)
	var want3: Dictionary = expect_sprite(Vector2(W * 2.0, H * 2.0), TEAM_COLOR, 8, 5.0, 5.0)
	print("      放大后 sprite0 position=%s（期望 %s）" % [kids[0].position, want3["position"]])
	ok(kids[0].position.is_equal_approx(want3["position"]),
		"控件放大到 1000×600 ⇒ 位置重摆成 (600, 200)")

	# 14) 回收：queue_free 整棵树，子节点跟着走
	var child_of_doomed: Node = kids[0]
	control.queue_free()
	await process_frame
	await process_frame
	ok(not is_instance_valid(drawer), "整棵 EntityDrawer 被 queue_free 回收")
	ok(not is_instance_valid(child_of_doomed), "子节点随父节点一起被回收（没有单独 free）")

	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("ENTITY_DRAWER %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
