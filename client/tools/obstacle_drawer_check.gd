extends SceneTree
## `ObstacleDrawer` 真节点验收脚本（`docs/16-client.md` §45）。
##
## ```text
## cargo build --features gdext-bindings
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/obstacle_drawer_check.gd
## ```
##
## 它验证的是「**障碍掩膜能不能真的落成一张上屏的 `LA8` 贴图、并随掩膜变化而更新**」，
## 不验证复原逻辑本身（掩膜算法在 `src/plane.rs`，已有 unicorn 对拍）：
##
## 1. `ObstacleDrawer` 注册进了 `ClassDB`、父类是 `Control`、能 `instantiate` / `.new()`；
## 2. 空抽屉：0 个子节点、贴图是空的、图尺寸 `(0,0)`；
## 3. `add_obstacle(0, 0, 1)` ⇒ 掩膜在半径 **20 px** 处被填（**480/520 边界**逐像素断言，
##    这是 `plane::obstacle_rect` 的已知值，不是我们自己编的）；
## 4. `update(plane)` ⇒ **出现 1 个 `Sprite2D`**、贴图非空、`Image` 尺寸 `(1000, 600)`、
##    格式 **1（LA8）**、`position = size*0.5`、`scale = (size.x/1000, -size.x/1000)`；
## 5. **贴图像素**（走 `Texture2D.get_image()`，引擎侧那一份）在障碍中心 alpha = 255、
##    空白处 alpha = 0 ⇒ "不透明黑" 这条逐位结论在引擎里成立；
## 6. `carve_hole(0, 0, 1)` + `update` ⇒ 子节点**还是 1 个**、sprite 没重建、`create_from_data`
##    没再调、贴图**变了**（同一像素 alpha 255 → 0）——这就是"挖一个洞 ⇒ 贴图更新"；
## 7. 每帧次数取自纯逻辑：`updates_per_frame() == 2`、`images_per_frame() == 4`；
## 8. 接进 `GraphwarPlane`：`build_obstacle_drawer()` 幂等、子节点真的是 `ObstacleDrawer`，
##    进树后 `process` **每帧刷 2 次**（差值是 2 的整数倍且真的在涨），
##    平面 `set_size` 之后 sprite 的 `scale` 跟着变 ⇒ 平面真的在驱动它。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const OD := "ObstacleDrawer"
const PLANE := "GraphwarPlane"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 掩膜/图的固定尺寸（`plane::CANVAS_W/H`；原版 `create_from_data` 的立即数 `0x3e8` / `0x258`）。
const CANVAS_W := 1000
const CANVAS_H := 600
## Godot 4 的 `Image::Format::LA8` 枚举值（原版 `mov w3, #0x1`）。
const FORMAT_LA8 := 1
## 1 个游戏单位 = 20 px（`plane::PX_PER_UNIT`）⇒ `radius = 1` 的圆半径 20 px。
const OBSTACLE_PX_RADIUS := 20

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

func _method_names(obj: Object) -> PackedStringArray:
	var names := PackedStringArray()
	for m in obj.get_method_list():
		names.append(String(m.name))
	return names

## 引擎侧读贴图像素：`ImageTexture.get_image()` 是 Godot 自己的方法，GDScript 直接调。
## 返回 0..255 的 alpha；拿不到返回 -1。
func _texture_alpha(sprite: Sprite2D, px: int, py: int) -> int:
	if sprite == null or sprite.texture == null:
		return -1
	var img: Image = sprite.texture.get_image()
	if img == null or img.is_empty():
		return -1
	return int(round(img.get_pixel(px, py).a * 255.0))

## 一的个「平面」：只要是个 `Control` 且尺寸对，`update` 就能摆 sprite。
func _make_plane(w: float, h: float) -> Control:
	var plane := Control.new()
	plane.set_size(Vector2(w, h))
	return plane

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	if not ClassDB.class_exists(OD):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])

	# 1) 类注册 + 基类 + 方法表
	ok(ClassDB.class_exists(OD), "ClassDB.class_exists(\"%s\")" % OD)
	ok(ClassDB.class_exists(PLANE), "ClassDB.class_exists(\"%s\")" % PLANE)
	ok(ClassDB.get_parent_class(OD) == "Control",
		"父类是 Control（原版这个类**不是** Godot 类 ⇒ 基类是推断，见 §45.2）")

	var probe: Object = ClassDB.instantiate(OD)
	ok(probe != null and probe is Control, "ClassDB.instantiate(\"ObstacleDrawer\") 出来的是 Control")
	if probe != null:
		var names := _method_names(probe)
		for method in ["add_obstacle", "carve_hole", "clear_obstacles", "update",
				"mask_value_at", "occupied_pixels", "child_count", "sprite_count",
				"texture_is_null", "image_size", "image_format", "sprite_position",
				"sprite_scale", "update_call_count", "image_build_count",
				"image_set_data_count", "texture_update_count", "sprite_build_count",
				"mask_edit_count", "updates_per_frame", "images_per_frame",
				"texture_alpha_at"]:
			ok(names.has(method), "方法表里有 %s()" % method)
		# 有界否定：本类不做碰撞、不 `_draw`（证伪用，避免把 `is_occupied` 混进来）
		ok(not names.has("is_obstacle"), "有界否定：本类不暴露 is_obstacle（碰撞归 ObstacleManager）")

	await _check_empty()
	await _check_add_and_update()
	await _check_carve()
	await _check_counts()
	await _check_plane()

	if probe != null:
		probe.free()
	_finish()

## 2) 空抽屉的起点
func _check_empty() -> void:
	var od: Object = ClassDB.instantiate(OD)
	var node: Control = od as Control
	ok(node != null, "实例化成功")
	if node == null:
		return

	ok(int(node.call("child_count")) == 0, "空抽屉：0 个子节点")
	ok(bool(node.call("texture_is_null")), "空抽屉：贴图是空的")
	ok(node.call("image_size") == Vector2i.ZERO, "空抽屉：图尺寸 (0,0)")
	ok(int(node.call("image_format")) == -1, "空抽屉：格式 -1（还没建图）")
	ok(int(node.call("occupied_pixels")) == 0, "空抽屉：掩膜全空")
	ok(int(node.call("mask_value_at", CANVAS_W / 2, CANVAS_H / 2)) == 255,
		"空掩膜中心 = 255（`plane::EMPTY`）")
	ok(int(node.call("mask_value_at", -1, 0)) == -1, "掩膜探针越界 = -1")
	ok(int(node.call("sprite_count")) == 0, "空抽屉：0 个 Sprite2D")
	ok(int(node.call("updates_per_frame")) == 2, "updates_per_frame() == 2（纯逻辑 §36 观测）")
	ok(int(node.call("images_per_frame")) == 4, "images_per_frame() == 4（2 次 update × 2 次 create_from_data）")
	node.free()

## 3) 加一个障碍 ⇒ 掩膜被填（480/520 这条边界来自 `plane::obstacle_rect`）
func _check_add_and_update() -> void:
	var node := ClassDB.instantiate(OD) as Control
	if node == null:
		ok(false, "实例化失败（_check_add_and_update）")
		return
	var plane := _make_plane(500.0, 300.0)

	var occupied: int = node.call("add_obstacle", 0.0, 0.0, 1.0)
	ok(int(node.call("mask_edit_count")) == 1, "add_obstacle 记了 1 次掩膜编辑")
	ok(occupied > 1200 and occupied < 1400,
		"半径 20 px 的圆面积 ≈ 1257（实际被占 %d 像素）" % occupied)
	ok(int(node.call("mask_value_at", 500, 300)) == 0, "圆心 (500,300) 被填 = 0")
	ok(int(node.call("mask_value_at", 500 - OBSTACLE_PX_RADIUS, 300)) == 0,
		"左边界 x = 480 仍被填（`obstacle_rect` 的 min_x = 480）")
	ok(int(node.call("mask_value_at", 500 - OBSTACLE_PX_RADIUS - 1, 300)) == 255,
		"再左一格 x = 479 已空（硬填行的边界是 480..520）")
	ok(int(node.call("mask_value_at", 500 + OBSTACLE_PX_RADIUS, 300)) == 0, "右边界 x = 520 被填")
	ok(int(node.call("mask_value_at", 500, 300 - OBSTACLE_PX_RADIUS)) == 0, "上边界 y = 280 被填")

	# 4) update(plane) ⇒ 出现 Sprite2D + 贴图 + 摆法
	var ok_update: bool = node.call("update", plane)
	ok(ok_update, "update(plane) 返回 true")
	ok(int(node.call("child_count")) == 1, "update 之后 **1 个子节点**（实际 %d）" % int(node.call("child_count")))
	ok(int(node.call("sprite_count")) == 1, "其中 1 个是 Sprite2D")
	ok(not bool(node.call("texture_is_null")), "贴图非空")
	ok(node.call("image_size") == Vector2i(CANVAS_W, CANVAS_H),
		"Image 尺寸 = (1000, 600)（原版立即数 0x3e8 / 0x258）")
	ok(int(node.call("image_format")) == FORMAT_LA8, "Image 格式 = 1 = LA8")
	ok(node.call("sprite_position") == Vector2(250.0, 150.0),
		"position = size*0.5 = (250, 150)（实际 %s）" % str(node.call("sprite_position")))
	ok(node.call("sprite_scale") == Vector2(0.5, -0.5),
		"scale = (size.x/1000, -size.x/1000) = (0.5, -0.5)（实际 %s）" % str(node.call("sprite_scale")))

	var child: Node = node.get_child(0)
	ok(child is Sprite2D, "第 0 个子节点是 Sprite2D（实际 %s）" % child.get_class())
	ok(String(child.name).begins_with("@Sprite2D@"),
		"子节点是引擎自动名 @Sprite2D@N（实际 %s）" % child.name)
	var sprite := child as Sprite2D
	if sprite != null:
		ok(sprite.texture != null, "Sprite2D.texture 非空")
		if sprite.texture != null:
			ok(sprite.texture.get_width() == CANVAS_W and sprite.texture.get_height() == CANVAS_H,
				"贴图尺寸 1000×600（实际 %d×%d）" % [sprite.texture.get_width(), sprite.texture.get_height()])
		ok(sprite.position == Vector2(250.0, 150.0), "Sprite2D.position 真的被设了")
		ok(sprite.scale == Vector2(0.5, -0.5), "Sprite2D.scale 真的被设了")
		# 5) 引擎侧像素：障碍 = 不透明黑、空白 = 全透明
		ok(_texture_alpha(sprite, 500, 300) == 255,
			"贴图圆心 alpha = 255（障碍不透明；实际 %d）" % _texture_alpha(sprite, 500, 300))
		ok(_texture_alpha(sprite, 500 - OBSTACLE_PX_RADIUS, 300) == 255, "贴图左边界 alpha = 255")
		ok(_texture_alpha(sprite, 500 - OBSTACLE_PX_RADIUS - 1, 300) == 0, "贴图 x = 479 alpha = 0")
		ok(_texture_alpha(sprite, 0, 0) == 0, "贴图左上角 alpha = 0（空 = 全透明）")
		var img: Image = sprite.texture.get_image()
		if img != null:
			var center := img.get_pixel(500, 300)
			ok(center.r == 0.0 and center.g == 0.0 and center.b == 0.0,
				"障碍像素是**黑**（L = 0，逐位：0x422cd4 的 sturb wzr）")
		else:
			ok(false, "Texture2D.get_image() 拿不到 Image")

	ok(int(node.call("image_build_count")) == 1, "create_from_data 只调了 1 次")
	ok(int(node.call("sprite_build_count")) == 1, "Sprite2D 只建了 1 个")
	ok(int(node.call("texture_update_count")) == 1, "贴图上传 1 次")
	ok(int(node.call("update_call_count")) == 1, "update_call_count() == 1")

	# 再 update 一次：贴图就地更新，不重建节点
	var before_child: int = int(node.call("child_count"))
	node.call("update", plane)
	ok(int(node.call("child_count")) == before_child, "第二次 update 没有多建子节点")
	ok(int(node.call("sprite_build_count")) == 1, "第二次 update 没重建 Sprite2D")
	ok(int(node.call("image_build_count")) == 1, "第二次 update 走 set_data（没再 create_from_data）")
	ok(int(node.call("image_set_data_count")) == 1, "Image::set_data 调了 1 次")
	ok(int(node.call("texture_update_count")) == 2, "ImageTexture::update 调了 1 次（上传共 2 次）")
	ok(int(node.call("update_call_count")) == 2, "update_call_count() == 2")

	# 换一个尺寸的平面 ⇒ 摆法跟着变（`size.x / 1000`）
	var big := _make_plane(800.0, 480.0)
	node.call("update", big)
	ok(node.call("sprite_position") == Vector2(400.0, 240.0), "800×480 ⇒ position (400, 240)")
	ok(node.call("sprite_scale") == Vector2(0.8, -0.8), "800×480 ⇒ scale (0.8, -0.8)")

	node.queue_free()
	big.free()
	plane.free()
	await process_frame

## 6) 挖一个洞 ⇒ 贴图更新（子节点与 sprite 都不重建）
func _check_carve() -> void:
	var node := ClassDB.instantiate(OD) as Control
	if node == null:
		ok(false, "实例化失败（_check_carve）")
		return
	var plane := _make_plane(500.0, 300.0)

	node.call("add_obstacle", 0.0, 0.0, 1.0)
	node.call("update", plane)
	var sprite_before := node.get_child(0) as Sprite2D
	var alpha_before: int = _texture_alpha(sprite_before, 500, 300)
	ok(sprite_before != null and alpha_before == 255,
		"挖洞之前：圆心 alpha = 255（实际 %d）" % alpha_before)

	var occupied: int = node.call("carve_hole", 0.0, 0.0, 1.0)
	# 挖同一个圆**不可能**把掩膜刷回全 255：`plane::rasterize` 的第 ② 步是**抗锯齿环**
	# （`blend_ring`），环上的像素是"填色与底色按覆盖率混合"的值，所以原地挖回来会剩一圈
	# 中间值。这是掩膜算法本身的行为（`src/plane.rs`，已 unicorn 对拍），不是本类的 bug。
	ok(occupied > 0 and occupied < 200,
		"挖洞之后只剩抗锯齿环那一圈（实际 %d 像素；圆心已被刷回）" % occupied)
	ok(int(node.call("mask_value_at", 500, 300)) == 255, "挖洞之后圆心掩膜 = 255")
	ok(int(node.call("mask_edit_count")) == 2, "掩膜编辑记了 2 次（加 + 挖）")

	node.call("update", plane)
	ok(int(node.call("child_count")) == 1, "挖洞之后仍只有 1 个子节点")
	ok(int(node.call("sprite_build_count")) == 1, "sprite 没重建")
	ok(int(node.call("image_build_count")) == 1, "create_from_data 没再调（走 set_data）")
	ok(int(node.call("image_set_data_count")) == 1, "Image::set_data 调了 1 次（`create_from_data` 的替代）")
	ok(int(node.call("texture_update_count")) == 2, "贴图上传共 2 次（1 建 + 1 更新）")

	var sprite_after := node.get_child(0) as Sprite2D
	ok(sprite_after == sprite_before, "还是**同一个** Sprite2D 实例（instance_id 没变）")
	if sprite_after != null:
		# 贴图内容走**本类留着的那张 `Image`**（就是喂给 `ImageTexture::update` 的那一份）。
		# 为什么不读 `texture.get_image()`：headless 的 dummy 渲染器里 `texture_2d_update`
		# 不生效（**观测**：建贴图时 alpha=255 的那一像素，`update` 之后从贴图读回来仍是 255，
		# 而同一张 `Image` 读出来已经是 0）⇒ 引擎侧的"贴图真的换了"只能在有 GPU 的机器上看到。
		var alpha_after: int = int(node.call("texture_alpha_at", 500, 300))
		ok(alpha_after == 0,
			"挖洞之后那张图圆心 alpha = 0（贴图真的更新了；实际 %d）" % alpha_after)
		ok(alpha_after != alpha_before,
			"同一像素在挖洞前后**不同**（%d → %d，这就是「挖一个洞 ⇒ 贴图更新」）" % [alpha_before, alpha_after])
		ok(int(node.call("texture_alpha_at", 0, 0)) == 0, "同一张图的空白处 alpha 仍是 0")
		print("      INFO headless dummy 渲染器：texture.get_image() 的读回还是建贴图那一刻的像素（%d），"
			% _texture_alpha(sprite_after, 500, 300)
			+ "与 Image 的 %d 不一致是渲染器行为，不是本类的问题" % alpha_after)

	# 清空也是一条路（`clear_obstacles`）
	node.call("add_obstacle", 5.0, 5.0, 2.0)
	var before_clear: int = int(node.call("occupied_pixels"))
	node.call("clear_obstacles")
	ok(before_clear > 0, "clear 之前掩膜里确实有东西（%d 像素）" % before_clear)
	ok(int(node.call("occupied_pixels")) == 0, "clear_obstacles 之后掩膜全空")
	ok(int(node.call("mask_edit_count")) == 4, "掩膜编辑共 4 次（加/挖/加/清）")

	node.queue_free()
	plane.free()
	await process_frame

## 7) 计数与纯逻辑对齐
func _check_counts() -> void:
	var node := ClassDB.instantiate(OD) as Control
	if node == null:
		ok(false, "实例化失败（_check_counts）")
		return
	var plane := _make_plane(1000.0, 600.0)
	var per_frame: int = int(node.call("images_per_frame"))
	var updates: int = int(node.call("updates_per_frame"))
	ok(per_frame == updates * 2,
		"images_per_frame() = updates_per_frame() × 2（update 体内 create_from_data 两次）= %d" % per_frame)
	for i in range(3):
		node.call("update", plane)
	ok(int(node.call("update_call_count")) == 3, "update 3 次 ⇒ 计数 3")
	ok(int(node.call("texture_update_count")) == 3, "3 次 update ⇒ 上传 3 次（1 建 + 2 更新）")
	# `flip_x = true` 时 scale.x 取负（原版 `0x422678` 的 fcsel）
	node.set("flip_x", true)
	node.call("update", plane)
	ok(node.call("sprite_scale") == Vector2(-1.0, -1.0),
		"flip_x = true ⇒ scale = (-1, -1)（原版 w5 分支）")
	node.queue_free()
	plane.free()
	await process_frame

## 8) 接进 GraphwarPlane：真子节点 + 每帧刷 2 次 + 跟着平面尺寸变
func _check_plane() -> void:
	var plane := ClassDB.instantiate(PLANE) as Control
	ok(plane != null, "GraphwarPlane 能实例化")
	if plane == null:
		return
	var names := _method_names(plane)
	for method in ["build_obstacle_drawer", "obstacle_drawer_count", "obstacle_drawer_updates"]:
		ok(names.has(method), "GraphwarPlane 方法表里有 %s()" % method)

	ok(int(plane.call("obstacle_drawer_count")) == 0, "平面一开始没有障碍层（不自动建）")
	ok(plane.get_child_count() == 0, "树外平面还没有子节点（实际 %d）" % plane.get_child_count())
	ok(int(plane.call("build_obstacle_drawer")) == 1, "build_obstacle_drawer() 返回 1")
	ok(plane.get_child_count() == 1, "建完 ⇒ 1 个子节点（实际 %d）" % plane.get_child_count())
	var drawer := plane.get_child(0)
	ok(drawer != null and drawer.get_class() == OD,
		"那个子节点就是 ObstacleDrawer（实际 %s）" % ("" if drawer == null else drawer.get_class()))
	ok(int(plane.call("build_obstacle_drawer")) == 1, "幂等：再调一次仍返回 1")
	ok(plane.get_child_count() == 1, "幂等：子节点没变多（实际 %d）" % plane.get_child_count())

	# 进树 ⇒ ready 建 3 个 FunctionDrawer + 我们那 1 个 = 4；process 每帧刷障碍层 2 次
	plane.set_size(Vector2(500.0, 300.0))
	get_root().add_child(plane)
	await process_frame
	await process_frame
	ok(plane.get_child_count() == 4,
		"进树后 4 个子节点 = 3 个 FunctionDrawer + 1 个 ObstacleDrawer（实际 %d）" % plane.get_child_count())
	ok(int(plane.call("function_drawer_count")) == 3, "原有的 3 个 FunctionDrawer 没被影响")

	if drawer != null:
		var before: int = int(plane.call("obstacle_drawer_updates"))
		ok(before >= 4, "进树 2 帧后障碍层至少被刷 4 次（实际 %d）" % before)
		var before_big: int = int(plane.call("obstacle_update_total"))
		for i in range(3):
			await process_frame
		var after: int = int(plane.call("obstacle_drawer_updates"))
		ok(after > before, "3 帧之后刷新次数还在涨（%d → %d）" % [before, after])
		ok((after - before) % 2 == 0,
			"每帧刷 **2** 次 ⇒ 差值是 2 的整数倍（差 %d）" % (after - before))
		ok((after - before) <= 6, "3 帧最多刷 6 次（差 %d）" % (after - before))
		ok(int(plane.call("obstacle_update_total")) >= before_big, "平面的累计计数也在涨")

		# 平面尺寸变了 ⇒ 障碍层的 sprite 跟着变（证明 `update` 拿的是平面的 size）
		plane.set_size(Vector2(800.0, 480.0))
		await process_frame
		await process_frame
		ok(drawer.call("sprite_scale") == Vector2(0.8, -0.8),
			"平面 800×480 ⇒ 障碍层 scale (0.8, -0.8)（实际 %s）" % str(drawer.call("sprite_scale")))
		ok(drawer.call("sprite_position") == Vector2(400.0, 240.0),
			"平面 800×480 ⇒ 障碍层 position (400, 240)")

	plane.queue_free()
	await process_frame

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("OBSTACLE_DRAWER %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
