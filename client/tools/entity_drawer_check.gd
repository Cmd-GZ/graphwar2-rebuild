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
## 3. 三个 `Sprite2D.texture` 都非空、都是 `Texture2D`、三层互不相同；**贴图链接上了**（§51）：
##    名字（`yellow` / `regular_eyes` / `soldier_helmet`）→ `GraphwarTextureLoader` 三张表 →
##    `res://skins|faces|hats/<名>.svg` → **`res://assets/<dir>/<名>.ctex`**（本机有 assets 时）；
##    **没有 assets 的机器优雅退化成程序生成的纯色 `ImageTexture`**（这一条在任何机器上都要断言）；
## 4. 位置 / 缩放 / 翻面 == `entity_drawer::sprite_transform` 的手算期望（§37.3 那张表）；
## 5. 名字标签的属性 == `recovered-src/name_label.tscn`（`fit_content` / `scroll_active` /
##    `autowrap_mode` / 初始尺寸 25×40），文本与不透明度跟着 `set_entity_name` /
##    `set_entity_dead_ticks` 走；
## 6. 进树之后 `_ready` / `_process` / `resized` **真的被引擎调到**，尺寸变了会重摆；
## 7. **准备圈真的发出去了**（§50）：`set_ring()` 之后每次 `_draw` 发 **3 条 `draw_arc` +
##    3 个 `draw_polygon`**，逐条对 `docs/archive/140-plane-visual.md` §2 的立即数
##    （半径 = `pos_radius×1.5×k`、起止角 `A_i ∓ H`、10 折点、线宽 0.3、关 AA、
##    中分角 2π/3 与 4π/3、箭头尖端在 `r`、底边在 `r+0.5`、张 12°），
##    **关掉准备态（`clear_ring`）之后一条都不发**；
## 8. `remove_entity()` 的 `queue_free` 真的释放了那 4 个节点；
## 9. 贴图三态（第 16 组）：**有 assets** ⇒ `texture.resource_path = res://assets/…ctex`、
##    每层按**自己**贴图的宽度算缩放（`no_face` 是 128、皮肤 512）；**没有 assets**
##    （`assets_root = ""`）⇒ 纯色兜底 + `missing_assets()` 点名缺什么；**名字不在表里** ⇒ 同样兜底。
##
## 与 `client/tools/gdext_check.gd` 的分工：那个测「换绑定层对客户端透明」，这个测
## 「`EntityDrawer` 能不能长出节点树 + 把准星画出来」。
## 与 `client/tools/draw_classes_check.gd` 的分工：那个用计数验"发了几个图元"，
## 这里连**实参**一起钉（`last_arc()` / `last_polygon()`）。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const NODE_CLASS := "EntityDrawer"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 控件尺寸与队伍色：手算期望值的三个输入（§37.3）。
## 纯红 `0xff` ⇒ 打包色低字节是 R、G 位不置顶 ⇒ `entity_drawer::team_mirror` 为 false ⇒ 不翻面。
const W := 500.0
const H := 300.0
const TEAM_COLOR := 0xff

## 准备圈那几条**立即数**（`docs/archive/140-plane-visual.md` §2，逐条 objdump 得来的）。
##
## ```text
## 0x19f620 = 120.0                     A0 = V/120
## 0x19f9e8 = 2.0943951023931953        A1 = A0 + 2π/3
## 0x19fbf8 = 4.1887902047863905        A2 = A0 + 4π/3
## 0x19f2e8 = 0.5235987755982988        H 的下限 π/6
## 0x19f0a8 = 1.0733774899765125        H 的封顶（≈61.5°）
## 0x1b9b98 = 1.5f32                    r = pos_radius × 1.5
## 0x1bb2b4 = 0.3f32                    线宽
## mov $0xa,%esi                        每段 10 折点
## xor %ecx,%ecx                        关抗锯齿
## 0x19f6d0 = 0.5                      箭头外沿 r + 0.5
## 0x19fb68 / 0x19fc80 = ±0.20943951023931953   箭头张角 ±6°
## ```
const HALF_ANGLE_MIN := 0.5235987755982988
const HALF_ANGLE_MAX := 1.0733774899765125
const SEGMENT_STEP := 2.0943951023931953
const SEGMENT_STEP2 := 4.1887902047863905
const RADIUS_SCALE := 1.5
const ARC_WIDTH := 0.3
const ARC_POINTS := 10
const ARROW_RADIUS_DELTA := 0.5
const ARROW_HALF_SPREAD := 0.20943951023931953

## 准备圈的队伍色：`#7D6800` 打包成 `u32`（低字节 = R）⇒ `0x00687d`。
## 这就是 `docs/16-client.md` §49.5 第 1 条点名的那个玩家色，`team_mirror` 判据为 false（不翻面）。
const RING_COLOR := 0x00687d

## `PosInfo` 的第 3 个 f64（**未标定的半径量**）。取 2.0 是为了让画出来的半径
## = 2×1.5×(500/50) = **30 px**，与"箭头外沿 35 px"一起好手算。
const RING_POS_RADIUS := 2.0

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## 浮点比较：`is_equal_approx` 的相对容差随量级变，角度的绝对值又小
## ⇒ 这里用固定的绝对容差（f32 → f64 的误差量级是 1e-8）。
func _near(a: float, b: float, eps: float = 1e-5) -> bool:
	return absf(a - b) <= eps

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
		# 准备圈（`_draw`）的输入口与观测口（§50）
		"set_ring", "set_ring_base", "clear_ring_base", "clear_ring", "ring_count",
		"draw_call_count", "drawn_arc_total", "drawn_polygon_total",
		"last_arc_count", "last_arc", "last_polygon",
		# 贴图链（`GraphwarTextureLoader` 三张表，§51）
		"set_entity_look", "entity_look", "sprite_texture_path",
		"texture_stats", "missing_assets", "assets_available",
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

	# 5) 贴图：**名字 → `GraphwarTextureLoader` 三张表 → res://…svg → client/assets/**.ctex**（§51）
	#
	# 原版 `ready` 订了 skin/face/hat 三个 graphics 事件，名字进 `HashMap<i32, String>`（+0x30），
	# `get_entity_skin_sprite`（x86 0x4bb6b0）拿名字去 loader 的皮肤表（+0x150）查 `Gd<Texture2D>`。
	# 本机有 client/assets/ ⇒ 真 `.ctex`（皮肤/脸/帽子都是 512×512）；没有 ⇒ 纯色兜底。
	var asset_mode: bool = bool(drawer.call("assets_available"))
	print("      assets_available = %s（assets_root = %s）"
		% [asset_mode, String(drawer.get("assets_root"))])
	ok(asset_mode == FileAccess.file_exists("res://assets/MANIFEST.tsv"),
		"assets_available() 与 GwAssets 同判据（<root>/MANIFEST.tsv 在不在）")
	var expected_paths := [
		"res://assets/skins/yellow.ctex",
		"res://assets/faces/regular_eyes.ctex",
		"res://assets/hats/soldier_helmet.ctex",
	]
	var textures: Array = []
	for i in range(3):
		var tex: Variant = kids[i].texture
		ok(tex != null, "子节点 %d 的 texture 非空" % i)
		ok(tex is Texture2D, "子节点 %d 的 texture 是 Texture2D" % i)
		if tex is Texture2D:
			textures.append(tex.get_instance_id())
			ok(String(drawer.call("sprite_texture_path", 0, i)) == String(tex.resource_path),
				"子节点 %d：sprite_texture_path(0, %d) == 贴图的 resource_path" % [i, i])
			if asset_mode:
				ok(String(tex.resource_path) == expected_paths[i],
					"子节点 %d 的贴图来自 %s（实际 \"%s\"）" % [i, expected_paths[i], tex.resource_path])
				ok(int(tex.get_width()) == 512 and int(tex.get_height()) == 512,
					"子节点 %d 的真贴图是 512×512（pck 里的 .ctex 就是 512²）" % i)
			else:
				ok(String(tex.resource_path) == "",
					"子节点 %d 是纯色兜底（ImageTexture 没有 resource_path）" % i)
				ok(int(tex.get_width()) == 8 and int(tex.get_height()) == 8,
					"子节点 %d 的兜底贴图是 8×8（texture_size）" % i)
		else:
			textures.append(-1)
			ok(false, "子节点 %d 的 text 不是 Texture2D，后面几条贴图断言无从验起" % i)
	# 统计口：本机要么 3 张全来自磁盘、要么 3 张全退化（同一个实例只 spawn 过一个实体）
	var stats: PackedInt64Array = drawer.call("texture_stats")
	if asset_mode:
		ok(int(stats[0]) == 3 and int(stats[1]) == 0 and int(stats[2]) == 0,
			"texture_stats() = [3 加载, 0 退化, 0 缺失]（实际 %s）" % [stats])
	else:
		ok(int(stats[0]) == 0 and int(stats[1]) == 3 and int(stats[2]) == 3,
			"没资产 ⇒ texture_stats() = [0, 3, 3]（实际 %s）" % [stats])
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
	#
	# ⚠️ `tex_w` 不是常数：原版 `update_sprite` 读**这一层自己**的 `Texture2D::get_width()`
	# 算 `unit = 128/tex_w`。有 assets 时皮肤是 512（⇒ unit = 0.25），没 assets 时是兜底的 8。
	var tex_w0: int = int(kids[0].texture.get_width())
	print("      皮肤层贴图宽度 tex_w = %d（unit = 128/%d = %f）"
		% [tex_w0, tex_w0, 128.0 / float(tex_w0)])
	var want: Dictionary = expect_sprite(Vector2(W, H), TEAM_COLOR, tex_w0, 5.0, 5.0)
	print("      sprite0 position=%s scale=%s flip_h=%s"
		% [kids[0].position, kids[0].scale, kids[0].flip_h])
	print("      期望   position=%s scale=%s flip_h=%s"
		% [want["position"], want["scale"], want["flip_h"]])
	ok(kids[0].position.is_equal_approx(want["position"]),
		"位置 == (300, 100)（k=10: 250+10·5, 150−10·5；与 tex_w 无关）")
	ok(kids[0].scale.is_equal_approx(want["scale"]),
		"缩放 == ((1/60)·(128/%d), (10/−60)·(128/%d))（读出的是这一层自己的 tex_w）"
		% [tex_w0, tex_w0])
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

	# 13) 准备圈：`_draw` 真的把 3 段弧 + 3 个外凸箭头发出来了（`0x4b2890`，§50）
	#
	# 原版在 `EntityDrawer::draw` 里对**每一具**准备态士兵发 6 条图元：
	# 3 次 `draw_polygon`（箭头，`0x4b30b7`/`0x4b30e8`/`0x4b3119`，都经 `0x4b9a70`）
	# + 3 次 `draw_arc`（弧，`0x4b366c`/`0x4b36fd`/`0x4b378a`，经 GOT 槽 `0x9bc020`）。
	# 这里验的正是"那 6 条真的发到了引擎，而且实参就是那几条立即数"。
	var draws_before: int = int(drawer.call("draw_call_count"))
	var arcs_before: int = int(drawer.call("drawn_arc_total"))
	var polys_before: int = int(drawer.call("drawn_polygon_total"))

	ok(int(drawer.call("ring_count")) == 0, "还没有准备态实体 ⇒ ring_count() == 0")
	await process_frame
	await process_frame
	print("      _draw 累计 %d 次（+%d）" % [int(drawer.call("draw_call_count")),
		int(drawer.call("draw_call_count")) - draws_before])
	ok(int(drawer.call("draw_call_count")) > draws_before, "_draw 被引擎调到（每帧 queue_redraw ⇒ 每帧一次）")
	ok(int(drawer.call("drawn_arc_total")) == arcs_before,
		"**没有准备态 ⇒ 一条 draw_arc 都不发**（`_draw` 只看 prep 表）")
	ok(int(drawer.call("drawn_polygon_total")) == polys_before,
		"没有准备态 ⇒ 一个 draw_polygon 都不发")

	# 13.1 打开准备态：棋盘 (5,5)、pos_radius = 2.0、V = 0、base 查不到（⇒ H 恒 π/6）
	drawer.set("team_color", RING_COLOR)
	drawer.call("set_ring", 0, 5.0, 5.0, RING_POS_RADIUS, 0.0)
	ok(int(drawer.call("ring_count")) == 1, "set_ring(0, 5, 5, 2.0, 0.0) ⇒ ring_count() == 1")

	draws_before = int(drawer.call("draw_call_count"))
	arcs_before = int(drawer.call("drawn_arc_total"))
	polys_before = int(drawer.call("drawn_polygon_total"))
	await process_frame
	await process_frame
	var draws_delta: int = int(drawer.call("draw_call_count")) - draws_before
	var arcs_delta: int = int(drawer.call("drawn_arc_total")) - arcs_before
	var polys_delta: int = int(drawer.call("drawn_polygon_total")) - polys_before
	print("      _draw +%d 次 ⇒ draw_arc +%d、draw_polygon +%d（期望 3 倍 / 3 倍）"
		% [draws_delta, arcs_delta, polys_delta])
	ok(draws_delta >= 1, "准备态下 _draw 被调到（+%d 次）" % draws_delta)
	ok(arcs_delta == 3 * draws_delta, "每次 _draw 发 3 条 draw_arc（一具士兵三段弧）")
	ok(polys_delta == 3 * draws_delta, "每次 _draw 发 3 个 draw_polygon（一具士兵三个外凸箭头）")
	ok(int(drawer.call("last_arc_count")) == 3, "最近一次 _draw 的弧条数 == 3")

	# 13.2 逐条对立即数。控件 500×300 ⇒ k = 500/50 = 10；
	#      圆心 = (250 + 10·5, 150 − 10·5) = (300, 100)（与 sprite 同一条投影）。
	var k: float = W / 50.0
	var center := Vector2(W * 0.5 + k * 5.0, H * 0.5 - k * 5.0)
	print("      期望圆心 %s，半径 %.3f" % [center, RING_POS_RADIUS * RADIUS_SCALE * k])

	var a0: PackedFloat64Array = drawer.call("last_arc", 0)
	print("      arc0 = %s" % [a0])
	ok(a0.size() == 12,
		"last_arc(0) 返回 12 个数：cx, cy, r, start, end, 折点, 线宽, AA, r, g, b, a")
	ok(_near(a0[0], center.x) and _near(a0[1], center.y),
		"第 1 条 draw_arc 的圆心 == (%.1f, %.1f)（棋盘 (5,5) 的投影，k=10）" % [center.x, center.y])
	ok(_near(a0[2], RING_POS_RADIUS * RADIUS_SCALE * k),
		"半径 == pos_radius×1.5×k == 30 px（1.5 = 槽 0x1b9b98）")
	ok(_near(a0[3], -HALF_ANGLE_MIN) and _near(a0[4], HALF_ANGLE_MIN),
		"第 1 条弧 start/end == ∓π/6（A0 − H / A0 + H：subsd 0x4b3620 / addsd 0x4b362c）")
	ok(int(a0[5]) == ARC_POINTS, "point_count == 10（mov $0xa,%esi @0x4b3655/0x4b36e6/0x4b3773）")
	ok(_near(a0[6], ARC_WIDTH), "width == 0.3（f32 0x3e99999a，槽 0x1bb2b4）")
	ok(a0[7] == 0.0, "antialiased == false（xor %ecx,%ecx）")
	ok(_near(a0[8], 0x7d / 255.0) and _near(a0[9], 0x68 / 255.0)
			and _near(a0[10], 0.0) and _near(a0[11], 1.0),
		"颜色 == #7D6800（team_color 0x00687d 按「低字节 = R」展开，alpha 1.0）")

	var a1: PackedFloat64Array = drawer.call("last_arc", 1)
	var a2: PackedFloat64Array = drawer.call("last_arc", 2)
	ok(_near((a1[3] + a1[4]) * 0.5, SEGMENT_STEP),
		"第 2 条弧的中分角 == 2π/3 = 2.0943951023931953（槽 0x19f9e8）")
	ok(_near((a2[3] + a2[4]) * 0.5, SEGMENT_STEP2),
		"第 3 条弧的中分角 == 4π/3 = 4.1887902047863905（槽 0x19fbf8）")
	ok(_near(a0[3] + 2.0 * HALF_ANGLE_MIN, a0[4]),
		"张开态单段张角 == 2×π/6 = 60°（三段合计 180°，画面上是断开的）")
	# 13.3 箭头：3 个顶点，尖端在 r、底边两点在 r+0.5、张 12°
	var p0: PackedVector2Array = drawer.call("last_polygon", 0)
	print("      arrow0 = %s" % [p0])
	ok(p0.size() == 3, "第 1 个箭头是 3 顶点多边形（原版走 draw_polygon）")
	ok(_near(p0[0].x, center.x + k * RING_POS_RADIUS * RADIUS_SCALE)
			and _near(p0[0].y, center.y),
		"尖端 == 圆心 + 30 px（θ = A0 = 0 ⇒ 落在 +x 方向）")
	ok(_near(p0[0].distance_to(center), RING_POS_RADIUS * RADIUS_SCALE * k),
		"尖端在半径 r 上（30 px = 3.0 棋盘单位 × k）")
	ok(_near(p0[1].distance_to(center), (RING_POS_RADIUS * RADIUS_SCALE + ARROW_RADIUS_DELTA) * k),
		"底边两点在 r + 0.5 上（35 px = 3.5 × k；0.5 是槽 0x19f6d0）")
	ok(_near(absf((p0[1] - center).angle_to(p0[2] - center)), 2.0 * ARROW_HALF_SPREAD),
		"底边张角 == 2×6° = 12°（槽 0x19fb68 / 0x19fc80）")

	# 13.4 闭合：base 命中 ⇒ H = min(π/6 + (V−base)/60, 61.5°)，V = 60 ⇒ 封顶
	ok(bool(drawer.call("set_ring_base", 0, 0.0)) == true,
		"set_ring_base(0, 0.0) 命中哈希表（0x4b34e6 的 je 不跳）⇒ true")
	drawer.call("set_ring", 0, 5.0, 5.0, RING_POS_RADIUS, 60.0)
	await process_frame
	await process_frame
	var c0: PackedFloat64Array = drawer.call("last_arc", 0)
	print("      闭合态 arc0 = %s" % [c0])
	ok(_near(c0[4] - c0[3], 2.0 * HALF_ANGLE_MAX),
		"闭合态单段张角 == 2×61.5° = 123°（H 被 minsd 封顶到 1.0733774899765125）")
	ok(_near((c0[3] + c0[4]) * 0.5, 60.0 / 120.0),
		"中分角 == V/120 == 0.5（槽 0x19f620 = 120.0）")
	ok(_near(c0[2], RING_POS_RADIUS * RADIUS_SCALE * k), "半径与 V 无关（仍是 30 px）")

	# 13.5 关掉准备态：一条都不再发（"该有的分支判断"）
	arcs_before = int(drawer.call("drawn_arc_total"))
	polys_before = int(drawer.call("drawn_polygon_total"))
	ok(bool(drawer.call("clear_ring", 0)) == true, "clear_ring(0) 返回 true（之前在准备态）")
	ok(int(drawer.call("ring_count")) == 0, "clear_ring 之后 ring_count() == 0")
	ok(bool(drawer.call("clear_ring", 0)) == false, "再 clear_ring(0) 返回 false（不报错）")
	await process_frame
	await process_frame
	ok(int(drawer.call("drawn_arc_total")) == arcs_before, "**关掉准备态之后不再发 draw_arc**")
	ok(int(drawer.call("drawn_polygon_total")) == polys_before, "关掉准备态之后不再发 draw_polygon")
	ok(int(drawer.call("last_arc_count")) == 0, "last_arc_count() 回到 0")

	# 恢复队色：后面两段按 `TEAM_COLOR` 手算 sprite 位置（这里只影响镜像位，两个值都不翻面）
	drawer.set("team_color", TEAM_COLOR)

	# 14) 尺寸变了 ⇒ `resized` ⇒ 重摆（k 从 10 变 20）
	control.set_size(Vector2(W * 2.0, H * 2.0))
	await process_frame
	var resized_calls: int = int(drawer.call("resized_call_count"))
	ok(resized_calls >= 1, "resized 信号真的连上了（on_resized 被调到 %d 次）" % resized_calls)
	var want3: Dictionary = expect_sprite(Vector2(W * 2.0, H * 2.0), TEAM_COLOR, tex_w0, 5.0, 5.0)
	print("      放大后 sprite0 position=%s（期望 %s）" % [kids[0].position, want3["position"]])
	ok(kids[0].position.is_equal_approx(want3["position"]),
		"控件放大到 1000×600 ⇒ 位置重摆成 (600, 200)")

	# 15) 贴图链的三种取法（§51）——**三种都要验**，因为"有 assets"不是所有机器都成立
	#
	# ```text
	# 16.1 有 assets：名字 → loader 表 → res://assets/**.ctex；每层按**自己**的 tex_w 算缩放
	# 16.2 没有 assets（assets_root = ""）：优雅退化成纯色，不崩
	# 16.3 名字不在 GraphwarTextureLoader 的表里：同样退化，且 missing_assets() 点名
	# ```
	#
	# 这几种都建**新实例**，不碰上面那个 `drawer`（它的子节点索引被前面几组断言钉死了）。

	# 15.1 bot 的外观：真机 dump 里 Bot 用 `skins/bot_2.svg` + `faces/no_face.svg` + `hats/no_hat.svg`
	#      （`docs/16-client.md` §2.1.10，**观测**）。注意三者宽度不同：512 / 128 / 512。
	var bot: Object = EntityDrawer.new()
	ok(bot != null, "EntityDrawer.new() 还能建第二个实例（贴图验收用）")
	if bot != null:
		var bc: Control = bot
		bc.set_size(Vector2(W, H))
		ok(int(bot.call("spawn_entity", 0, 5.0, 5.0)) == 4, "bot 实例 spawn_entity(0, 5, 5) == 4")
		ok(bool(bot.call("set_entity_look", 0, "bot_2", "no_face", "no_hat")) == true,
			"set_entity_look(0, \"bot_2\", \"no_face\", \"no_hat\") == true")
		var look: PackedStringArray = bot.call("entity_look", 0)
		ok(look.size() == 3 and String(look[0]) == "bot_2" and String(look[1]) == "no_face"
				and String(look[2]) == "no_hat",
			"entity_look(0) 往返一致（%s）" % [look])
		if asset_mode:
			ok(String(bot.call("sprite_texture_path", 0, 0)) == "res://assets/skins/bot_2.ctex",
				"bot 的皮肤层 = res://assets/skins/bot_2.ctex")
			ok(String(bot.call("sprite_texture_path", 0, 1)) == "res://assets/faces/no_face.ctex",
				"bot 的脸层 = res://assets/faces/no_face.ctex")
			ok(String(bot.call("sprite_texture_path", 0, 2)) == "res://assets/hats/no_hat.ctex",
				"bot 的帽子层 = res://assets/hats/no_hat.ctex")
			var b0: Sprite2D = bc.get_child(0)
			var b1: Sprite2D = bc.get_child(1)
			ok(int(b0.texture.get_width()) == 512 and int(b1.texture.get_width()) == 128,
				"bot 皮肤 512×512、脸 no_face 128×128（pck 里就是这样，%d/%d）"
				% [b0.texture.get_width(), b1.texture.get_width()])
			# `update_sprite` 的 `unit = 128/tex_w` 是**每层自己的**贴图宽度 ⇒ 两层的 scale 不同
			var want_b0: Dictionary = expect_sprite(Vector2(W, H), TEAM_COLOR, 512, 5.0, 5.0)
			var want_b1: Dictionary = expect_sprite(Vector2(W, H), TEAM_COLOR, 128, 5.0, 5.0)
			ok(b0.scale.is_equal_approx(want_b0["scale"]),
				"皮肤层缩放按 tex_w = 512 算（unit = 128/512）")
			ok(b1.scale.is_equal_approx(want_b1["scale"]),
				"脸层缩放按 tex_w = 128 算（unit = 128/128）⇒ 与皮肤层不同，%s vs %s"
				% [b0.scale, b1.scale])
			ok(not b0.scale.is_equal_approx(b1.scale), "两层 scale 不同（各读自己的贴图宽度）")
		bc.free()

	# 15.2 没有 assets 的机器：`assets_root = ""` 就是同一件事（不读盘）
	var noassets: Object = EntityDrawer.new()
	ok(noassets != null, "第三个实例（模拟没有 assets 的机器）")
	if noassets != null:
		var nc: Control = noassets
		nc.set_size(Vector2(W, H))
		noassets.set("assets_root", "")
		ok(bool(noassets.call("assets_available")) == false,
			"assets_root = \"\" ⇒ assets_available() == false（等价于没有 client/assets/）")
		ok(int(noassets.call("spawn_entity", 0, 5.0, 5.0)) == 4,
			"**没有 assets 也照样 spawn**（4 个子节点，不崩）")
		var fb_paths_ok := true
		var fb_sizes_ok := true
		for i in range(3):
			var sp: Sprite2D = nc.get_child(i)
			var t: Variant = sp.texture
			if t == null or not (t is Texture2D) or String(t.resource_path) != "":
				fb_paths_ok = false
			elif int(t.get_width()) != 8 or int(t.get_height()) != 8:
				fb_sizes_ok = false
		ok(fb_paths_ok, "没资产 ⇒ 三层都是纯色兜底（texture 非空、resource_path 为空）")
		ok(fb_sizes_ok, "没资产 ⇒ 兜底贴图 8×8（texture_size 默认值）")
		var st2: PackedInt64Array = noassets.call("texture_stats")
		ok(int(st2[0]) == 0 and int(st2[1]) == 3,
			"没资产 ⇒ texture_stats() = [0 加载, 3 退化]（实际 %s）" % [st2])
		var miss2: PackedStringArray = noassets.call("missing_assets")
		ok(miss2.size() == 3 and miss2.has("skins/yellow")
				and miss2.has("faces/regular_eyes") and miss2.has("hats/soldier_helmet"),
			"没资产 ⇒ missing_assets() 点名缺哪三个（%s）" % [miss2])
		# 换外观也照样退化，而且不会崩
		ok(bool(noassets.call("set_entity_look", 0, "bot_2", "no_face", "no_hat")) == true,
			"没资产时 set_entity_look 仍然返回 true")
		ok(String(noassets.call("sprite_texture_path", 0, 2)) == "",
			"没资产 + 换外观 ⇒ 帽子层仍是纯色（resource_path 为空）")
		var st3: PackedInt64Array = noassets.call("texture_stats")
		ok(int(st3[1]) == 6, "没资产 + 换外观 ⇒ 退化计数涨到 6（三张默认 + 三张新名字）")
		nc.free()

	# 15.3 名字不在 loader 的表里（不是资产问题，是名字错）
	var odd: Object = EntityDrawer.new()
	ok(odd != null, "第四个实例（验「名字不在表里」）")
	if odd != null:
		var oc: Control = odd
		oc.set_size(Vector2(W, H))
		odd.set("assets_root", String(drawer.get("assets_root")))
		ok(int(odd.call("spawn_entity", 0, 5.0, 5.0)) == 4, "第四个实例也能 spawn")
		ok(bool(odd.call("set_entity_look", 0, "no_such_skin", "regular_eyes", "soldier_helmet")) == true,
			"set_entity_look 接受表里没有的名字（不崩）")
		ok(String(odd.call("sprite_texture_path", 0, 0)) == "",
			"表里没有的名字 ⇒ 皮肤层退化成纯色")
		var miss3: PackedStringArray = odd.call("missing_assets")
		ok(miss3.has("skins/no_such_skin"),
			"missing_assets() 里点名 skins/no_such_skin（%s）" % [miss3])
		if asset_mode:
			ok(String(odd.call("sprite_texture_path", 0, 1)) == "res://assets/faces/regular_eyes.ctex"
					and String(odd.call("sprite_texture_path", 0, 2)) == "res://assets/hats/soldier_helmet.ctex",
				"只有皮肤那一层退化，脸/帽子仍是真贴图")
		ok(bool(odd.call("set_entity_look", 99, "yellow", "no_face", "no_hat")) == false,
			"set_entity_look 对不存在的实体返回 false")
		oc.free()

	# 16) **内核事件通道**（任务 42，`docs/16-client.md` §46）：把两条链路真跑一遍
	#
	# 原版逐环节（地址见 §46）：
	#
	# ```text
	# 生产  GraphwarPlanePosManager::set_position        x86 0x3c64c0
	#         └─ GraphwarEventQueue::add_event(PosInfo=26) x86 0x3d3530
	# 派发  GraphwarEventHandler::process_events         arm64 0x486970
	# 订阅  EntityDrawer::ready 的 6 次 subscribe         x86 0x4b4130 → 0x514b40
	#         └─ 闭包唯一副作用： self[+0x108] = 1         x86 0x4b4990
	# 消费  EntityDrawer::draw / process **直接读位置表** x86 0x4b2890 / 0x4b4b60
	# ```
	#
	# ⇒ 这里验的正是那三件事：**事件只置脏**、**数据是拉出来**的、
	#   拉出来的东西真的进了 `_draw`（而不是靠播放屏按回合状态代填）。
	ok(int(drawer.call("kernel_subscriptions")) == 0, "还没调 kernel_subscribe ⇒ 订阅数 0")
	var subs: int = int(drawer.call("kernel_subscribe"))
	ok(subs == 6, "kernel_subscribe() 订上 6 条（= `ready` 的 6 个事件名，实际 %d）" % subs)
	ok(int(drawer.call("kernel_subscriptions")) == 6, "订阅数读回 6")
	ok(int(drawer.call("ring_count")) == 0, "通道开着但位置表空 ⇒ 环仍是 0（不凭空画）")

	# 16.1 生产端：真调 `GraphwarPlanePosManager::set_position`（x86 0x3c64c0）
	ok(bool(drawer.call("kernel_push_position", 0, 5.0, 5.0, RING_POS_RADIUS)) == true,
		"kernel_push_position(0, 5, 5, 2.0) 真的写进位置表")
	ok(int(drawer.call("ring_count")) == 0,
		"事件还在队列里、没 pump ⇒ 一个环都不画（**原版的事件只是脏信号**）")
	ok(bool(drawer.call("kernel_dirty")) == false, "还没 pump ⇒ 脏标志 +0x108 仍是 false")

	# 16.2 投递 + 拉：pump 之后 `prep` 表由**位置表**填（不是事件载荷，也不是调用方代填）
	var events0: int = int(drawer.call("kernel_events"))
	var delivered: int = int(drawer.call("kernel_pump"))
	ok(delivered >= 1, "kernel_pump() 投递了 ≥1 条订阅事件（实际 %d）" % delivered)
	ok(int(drawer.call("kernel_events")) > events0, "累计投递数涨了")
	ok(bool(drawer.call("kernel_dirty")) == false,
		"拉完清脏（原版 `process` 末尾 x86 0x4b5409 `mov byte [rbx+0x108],0`）")
	var tags: PackedInt64Array = drawer.call("kernel_last_tags")
	ok(tags.size() >= 1 and int(tags[tags.size() - 1]) == 26,
		"投递的最后一条是 PosInfo(26)（`kernel_last_tags` = %s）" % [tags])
	ok(int(drawer.call("ring_count")) == 1, "拉完 ⇒ prep 表里 1 条（ring_count == 1）")
	ok(int(drawer.call("kernel_pulled")) >= 1, "累计拉到 ≥1 条位置")
	ok(int(drawer.call("kernel_pulls")) >= 1, "累计拉过 ≥1 次")

	# 16.3 拉到的东西真的进了 `_draw`：圆心/半径就是位置表那三个 f64
	#
	# 驱动量 `V` 是原版 `draw` 里 `0x4b2e10 call 0x519530` 的返回值。容器里那台
	# `GraphwarTickHandler` 的基准是 `(0,0)`（epoch）⇒ 真读数是 epoch 量级，角度没法手算。
	# 所以先把**虚拟时钟**摆到 0（同 `graphwar_plane_class::set_fractional_tick` 的做法，
	# 只摆时钟、不碰数据），再逐位对角度。
	ok(bool(drawer.call("kernel_set_clock", 0.0)) == true, "kernel_set_clock(0.0) 摆好虚拟时钟")
	ok(String(drawer.call("kernel_clock_source")) == "override", "时钟来源 == override")
	ok(int(drawer.call("kernel_pump")) >= 0, "再 pump 一次（让 V = 0 进 prep 表）")
	await process_frame
	await process_frame
	var k2: float = control.size.x / 50.0
	var c_center := Vector2(control.size.x * 0.5 + k2 * 5.0, control.size.y * 0.5 - k2 * 5.0)
	var g0: PackedFloat64Array = drawer.call("last_arc", 0)
	print("      内核链条 arc0 = %s" % [g0])
	ok(int(drawer.call("last_arc_count")) == 3, "位置表 1 条 ⇒ `_draw` 发 3 段弧")
	ok(g0.size() == 12 and _near(g0[0], c_center.x) and _near(g0[1], c_center.y),
		"圆心 == 位置表的 (pos_x, pos_y) = (5,5) 的投影 (%.1f, %.1f)" % [c_center.x, c_center.y])
	ok(_near(g0[2], RING_POS_RADIUS * RADIUS_SCALE * k2),
		"半径 == 位置表的 pos_radius×1.5×k == 30 px（第 3 个 f64 就是半径）")
	# 驱动量 V 来自 `GraphwarTickHandler::get_fractional_tick`（x86 0x519530）——
	# 本局那个 tick handler 没推进过 ⇒ V = 0 ⇒ 张开态（∓π/6）。
	ok(_near(g0[3], -HALF_ANGLE_MIN) and _near(g0[4], HALF_ANGLE_MIN),
		"V 来自 tick handler 的 get_fractional_tick()（=0）⇒ 张开态 ∓π/6")

	# 16.4 **代偿让位给真链路**：调用方再按老办法 `set_ring(...)` 塞别的坐标，
	#      下一次"事件 → 拉表"会把它**覆盖**成位置表里的值。
	drawer.call("set_ring", 0, -5.0, -5.0, 9.0, 60.0)
	ok(int(drawer.call("ring_count")) == 1, "手工 set_ring 也在同一张表里（1 条）")
	ok(bool(drawer.call("kernel_push_position", 0, 6.0, -3.0, 1.0)) == true,
		"位置表改成 (6, -3, 1.0)")
	ok(int(drawer.call("kernel_pump")) >= 1, "再来一次 pump")
	var c2 := Vector2(control.size.x * 0.5 + k2 * 6.0, control.size.y * 0.5 - k2 * (-3.0))
	await process_frame
	await process_frame
	var g1: PackedFloat64Array = drawer.call("last_arc", 0)
	print("      覆盖之后 arc0 = %s" % [g1])
	ok(_near(g1[0], c2.x) and _near(g1[1], c2.y),
		"拉表把手工 set_ring 的 (-5,-5) 覆盖成位置表的 (6,-3)（%.1f, %.1f）" % [c2.x, c2.y])
	ok(_near(g1[2], 1.0 * RADIUS_SCALE * k2), "半径也跟着变成 1.0×1.5×k == 15 px")

	# 16.5 位置表清空 ⇒ 环自己消失（不需要调用方逐个 `clear_ring`）
	ok(bool(drawer.call("kernel_push_position", 0, 0.0, 0.0, 1.0)) == true, "再写一格")
	ok(int(drawer.call("kernel_pump")) >= 1, "pump 一次")
	ok(int(drawer.call("ring_count")) == 1, "表里还有 1 条")
	# 16.6 外观链（51/53/55）：同样"写表 + 事件 → 拉表 → 换贴图"
	ok(bool(drawer.call("kernel_push_graphics", 0, 0, "target")) == true,
		"set_skin_graphics(0, 'target')（x86 0x468c70）")
	ok(bool(drawer.call("kernel_push_graphics", 0, 1, "no_face")) == true,
		"set_face_graphics(0, 'no_face')（x86 0x468b60）")
	ok(bool(drawer.call("kernel_push_graphics", 0, 2, "no_hat")) == true,
		"set_hat_graphics(0, 'no_hat')（x86 0x468a50）")
	var looks_before: PackedStringArray = drawer.call("entity_look", 0)
	ok(looks_before == PackedStringArray(["yellow", "regular_eyes", "soldier_helmet"]),
		"事件还没投递 ⇒ 外观仍是默认三件套（%s）" % [looks_before])
	ok(int(drawer.call("kernel_pump")) >= 1, "pump（位置 + 三条 graphics 事件一起投递）")
	var gtags: PackedInt64Array = drawer.call("kernel_last_tags")
	ok(gtags.size() >= 3 and int(gtags[gtags.size() - 3]) == 51
			and int(gtags[gtags.size() - 2]) == 53 and int(gtags[gtags.size() - 1]) == 55,
		"投递的三条事件判别值依次是 51 / 53 / 55（%s）" % [gtags])
	ok(int(drawer.call("kernel_graphics_applied")) >= 1, "拉表后至少换过一次外观")
	var looks_after: PackedStringArray = drawer.call("entity_look", 0)
	ok(looks_after == PackedStringArray(["target", "no_face", "no_hat"]),
		"外观表的三个名字进了实体（%s）" % [looks_after])

	# 17) 回收：queue_free 整棵树，子节点跟着走
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
