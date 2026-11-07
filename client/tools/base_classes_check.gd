extends SceneTree
## 三个「基类不是 `Control`」的扩展类的验收脚本（`docs/16-client.md` 本轮新增的一节）。
##
## ```text
## cargo build --release --features gdext-bindings
## cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
## godot --headless --path client --import
## godot --headless --path client --script res://tools/base_classes_check.gd
## ```
##
## 它验证的是**三件本批特有的事**，不验证复原出来的算法本身：
##
## 1. **换基类**：`GraphwarRootNode`(`Node`) / `GraphwarLanVBox`(`VBoxContainer`) /
##    `GraphwarAdSwitch`(基类未定，按 `Node` 落) 三个类都能 `class_exists` + `instantiate`，
##    `is_class()` 认对了父类，`#[func]` 方法表与 `@export` 属性在三套基类下**行为一致**；
##    `_ready` / `_process` 在 `Node` 与 `VBoxContainer` 两种基类下都真的被引擎调到
##    （⚠️ 不写 `set_process(true)`：Godot 4.7 自己开，`docs/16` §37.4 第 5 条）。
## 2. **局域网列表不排序**：`GraphwarLanVBox.update_game_list()` 给 3 条**故意打乱**的源表，
##    子节点顺序必须**等于传入顺序**（原版 `process` 里 0 处 `sort*`）；再给一次同样的表 ⇒
##    只刷新不重建（节点实例 id 不变）；少一条 ⇒ 那一条被 `queue_free`。
##    `VBoxContainer` 还会**纵向排列**子节点 ⇒ 进树后 y 必须递增（纯 `Node` 没有这个行为）。
## 3. **根节点只做骨架也能转发**：`GraphwarRootNode` 持有子对象 `Ad_holder`
##    （`GraphwarAdSwitch`），`purchase_confirmed()` → `owned = true` → `are_ads_enabled() = false`
##    （原版 arm64 `0x3a658c` 就是写那个 `+0x18` 字节）；`show_screen()` 把
##    `graphwar_root_node::apply_show_screen` 的决策**转发成真子节点的 `set_visible`**。
##
## 与 `entity_drawer_check.gd` 的分工：那个测「`Control` 基类的模板能不能长出节点树」，
## 这个测「**换基类**之后同一个模板还成不成立」。
##
## 这是**验收工具**，不是游戏逻辑（与 `selftest.gd` 一样放在 `tools/` 下，不进运行时路径）。

const ROOT_CLASS := "GraphwarRootNode"
const VBOX_CLASS := "GraphwarLanVBox"
const AD_CLASS := "GraphwarAdSwitch"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 15 个屏名，顺序 = `docs/16-client.md` §15.2 的表序（`graphwar_root_node::ALL_SCREENS`）。
## 这里**手抄一份**：脚本要独立于 Rust 侧算期望值。
const SCREENS := [
	"MainMenu", "Campaign", "LanLobby", "GameSetup", "PlayScreen",
	"ReplayLobby", "PlayerScreen", "JoinIpPopUp", "AddBotPopUp", "ConfirmPlay",
	"ExitPopUp", "JoinLevelPopUp", "ConnectingPopUp", "EndGamePopUp", "DisconnectedPopUp",
]

## `_ready` 里 `get_node` 过的三个 holder（`graphwar_root_node::READY_NODES`）。
const HOLDERS := ["ScreensControl", "IAP_holder", "Ad_holder"]

## 38 字节的 AdMob 单元 id（`.rodata` `0x1c43ff`）。
const AD_UNIT_ID := "ca-app-pub-3940256099942544/6300978111"

## 三条故意**乱序**的房间地址（.9 → .3 → .1）：用来证明"不排序"。
const ROOM_ADDRS := ["127.0.0.1:35249", "127.0.0.1:35243", "127.0.0.1:35241"]
const ROOM_NAMES := ["Room Nine", "Room Three", "Room One"]
const ROOM_COUNTS := [9, 3, 1]
const ROOM_LOCKED := [0, 1, 0]

var _checks := 0
var _failures: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## `127.0.0.1:35249` → `RoomEntry_127_0_0_1_35249`
## （与 Rust 侧 `graphwar_lan_vbox_class::node_name_for` 同一条规则，**独立重算**）。
func row_node_name(addr: String) -> String:
	var out := ""
	for i in range(addr.length()):
		var c := addr[i]
		if (c >= "0" and c <= "9") or (c >= "a" and c <= "z") or (c >= "A" and c <= "Z"):
			out += c
		else:
			out += "_"
	return "RoomEntry_" + out

## 某节点的全部方法名。
func method_names(obj: Object) -> PackedStringArray:
	var out := PackedStringArray()
	for m in obj.get_method_list():
		out.append(String(m.name))
	return out

## 某节点的全部属性名。
func property_names(obj: Object) -> PackedStringArray:
	var out := PackedStringArray()
	for p in obj.get_property_list():
		out.append(String(p.name))
	return out

func _initialize() -> void:
	# 0) 扩展加载了吗？第一次跑到（没有 `.godot/extension_list.cfg`）时不会，显式补一次。
	for cls in [ROOT_CLASS, VBOX_CLASS, AD_CLASS]:
		if not ClassDB.class_exists(cls):
			var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
			print("      GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])
			break

	# ------------------------------------------------------------------
	# 1) 三个类都注册了、都能实例化，父类分别是 Node / VBoxContainer / Node
	# ------------------------------------------------------------------
	for cls in [ROOT_CLASS, VBOX_CLASS, AD_CLASS]:
		ok(ClassDB.class_exists(cls), "ClassDB.class_exists(\"%s\")" % cls)
	if not (ClassDB.class_exists(ROOT_CLASS) and ClassDB.class_exists(VBOX_CLASS) \
			and ClassDB.class_exists(AD_CLASS)):
		_finish()
		return

	var root_node: Node = ClassDB.instantiate(ROOT_CLASS)
	var vbox: Node = ClassDB.instantiate(VBOX_CLASS)
	var ad: Node = ClassDB.instantiate(AD_CLASS)
	ok(root_node != null and root_node is Node, "%s 能实例化且是 Node" % ROOT_CLASS)
	ok(vbox != null and vbox is VBoxContainer, "%s 能实例化且是 VBoxContainer" % VBOX_CLASS)
	ok(ad != null and ad is Node, "%s 能实例化且是 Node" % AD_CLASS)
	if root_node == null or vbox == null or ad == null:
		_finish()
		return
	print("      root=%s vbox=%s ad=%s"
		% [root_node.get_class(), vbox.get_class(), ad.get_class()])

	# 父类判据（换基类的核心断言）
	ok(root_node.is_class("Node"), "GraphwarRootNode.is_class(\"Node\")")
	ok(vbox.is_class("VBoxContainer"), "GraphwarLanVBox.is_class(\"VBoxContainer\")")
	ok(vbox.is_class("Container"), "GraphwarLanVBox.is_class(\"Container\")（继承链）")
	ok(vbox.is_class("Control"), "GraphwarLanVBox.is_class(\"Control\")（继承链）")
	ok(ad.is_class("Node"), "GraphwarAdSwitch.is_class(\"Node\")")

	# ------------------------------------------------------------------
	# 2) `#[func]` 方法表：同一套宏在三套基类下都生成了方法
	# ------------------------------------------------------------------
	var root_methods := method_names(root_node)
	for wanted in ["build_scene_graph", "show_screen", "screen_count", "screen_names",
			"visible_screens", "set_screen_visible", "deferred_log", "ads_enabled",
			"purchase_confirmed", "purchase_denied", "ready_call_count", "process_call_count"]:
		ok(root_methods.has(wanted), "GraphwarRootNode.%s() 存在" % wanted)

	var vbox_methods := method_names(vbox)
	for wanted in ["update_game_list", "row_count", "row_names", "row_texts",
			"set_row_locked", "source_name", "press_join", "drain_join_requests",
			"pending_join_request_count", "process_call_count"]:
		ok(vbox_methods.has(wanted), "GraphwarLanVBox.%s() 存在" % wanted)

	var ad_methods := method_names(ad)
	for wanted in ["are_ads_enabled", "get_ad_unit_id", "owned_raw", "owned_field_offset"]:
		ok(ad_methods.has(wanted), "GraphwarAdSwitch.%s() 存在" % wanted)
	# 原版只有 2 个导出方法，而且**一个虚方法都不覆写**（这正是它的基类定不出来的原因）
	ok(not ad_methods.has("ready") and not ad_methods.has("process"),
		"GraphwarAdSwitch **不**实现 ready/process（原版 6 个函数全是注册驱动）")
	ok(ad_methods.has("notification") and ad_methods.has("set_owned"),
		"方法表里只有引擎方法 + 2 个 getter（`set_owned` 是 `@export owned` 自动生成的）")

	# ------------------------------------------------------------------
	# 3) 属性机制：三套基类的 `@export` 都能读写
	# ------------------------------------------------------------------
	root_node.set("ads_slot", true)
	ok(bool(root_node.get("ads_slot")), "属性 ads_slot 往返一致（true）")
	root_node.set("ads_slot", false)
	ok(not bool(root_node.get("ads_slot")), "属性 ads_slot 往返一致（false）")

	vbox.set("show_global", true)
	ok(bool(vbox.get("show_global")), "属性 show_global 往返一致（true）")
	ok(String(vbox.call("source_name")) == "Broker", "show_global = true ⇒ 源 = Broker")
	vbox.set("show_global", false)
	ok(String(vbox.call("source_name")) == "Lan", "show_global = false ⇒ 源 = Lan")

	ad.set("owned", false)
	ok(bool(ad.get("owned")) == false, "属性 owned 往返一致（false）")
	ok(bool(ad.call("are_ads_enabled")) == true, "owned = false ⇒ are_ads_enabled() = true")
	ad.set("owned", true)
	ok(bool(ad.call("owned_raw")) == true, "属性 owned 写回字段本身（owned_raw）")
	ok(bool(ad.call("are_ads_enabled")) == false, "owned = true ⇒ are_ads_enabled() = false")
	print("      ad.get(\"owned\")=%s are_ads_enabled()=%s"
		% [ad.get("owned"), ad.call("are_ads_enabled")])

	# 属性表里真的有这三条（`get_property_list` 是引擎口径）
	ok(property_names(root_node).has("ads_slot"), "属性表里有 ads_slot")
	ok(property_names(vbox).has("show_global"), "属性表里有 show_global")
	ok(property_names(ad).has("owned"), "属性表里有 owned（推断：属性可写）")

	# ------------------------------------------------------------------
	# 4) `GraphwarAdSwitch` 的语义（纯 getter）
	# ------------------------------------------------------------------
	var ad_id := String(ad.call("get_ad_unit_id"))
	print("      ad_unit_id = \"%s\"（%d 字节）" % [ad_id, ad_id.length()])
	ok(ad_id == AD_UNIT_ID, "get_ad_unit_id() 返回 38 字节的 AdMob 测试单元 id")
	ok(ad_id.length() == 38, "单元 id 长度 == 38")
	ok(int(ad.call("owned_field_offset")) == 0x18, "owned 的字段偏移 == 0x18（arm64 逐位）")

	# ------------------------------------------------------------------
	# 5) `GraphwarLanVBox`：3 个子项、顺序 = 插入顺序（**无排序**）
	# ------------------------------------------------------------------
	var created: int = int(vbox.call("update_game_list",
		PackedStringArray(ROOM_ADDRS), PackedStringArray(ROOM_NAMES),
		PackedInt32Array(ROOM_COUNTS), PackedByteArray(ROOM_LOCKED)))
	ok(created == 3, "update_game_list(3 条) 返回 3（实际 %d）" % created)
	ok(int(vbox.call("row_count")) == 3, "row_count() == 3")
	ok(vbox.get_child_count() == 3, "子节点个数 == 3（实际 %d）" % vbox.get_child_count())

	# 顺序：必须是**传入顺序**，不是地址字典序（.1 会排到最前）
	var want_names := PackedStringArray()
	for a in ROOM_ADDRS:
		want_names.append(row_node_name(a))
	var got_names := PackedStringArray(vbox.call("row_names"))
	print("      期望顺序 = %s" % want_names)
	print("      实际顺序 = %s" % got_names)
	ok(got_names == want_names, "子节点顺序 == 传入顺序（**没有**按地址排序）")
	ok(int(vbox.call("row_count")) == 3 and String(got_names[0]).ends_with("35249"),
		"第一条就是最后那个地址（.9）—— 排序会把 .1 排到最前，这里没有")

	var got_texts := PackedStringArray(vbox.call("row_texts"))
	ok(got_texts == PackedStringArray(ROOM_NAMES), "每行 RoomName 文本 == 传入的房名")

	# locked → JoinButton.disabled（原版 `set_disabled`）
	ok(bool(vbox.call("row_join_disabled", ROOM_ADDRS[1])) == true,
		"locked 的那一行 JoinButton.disabled == true")
	ok(bool(vbox.call("row_join_disabled", ROOM_ADDRS[0])) == false,
		"没锁的行 JoinButton.disabled == false")
	ok(bool(vbox.call("set_row_locked", ROOM_ADDRS[0], true)) == true,
		"set_row_locked 能改一行")
	ok(bool(vbox.call("row_join_disabled", ROOM_ADDRS[0])) == true, "改完 disabled == true")
	vbox.call("set_row_locked", ROOM_ADDRS[0], false)

	# 复用：同一张表再来一次 ⇒ 只刷新，不重建（实例 id 不变）
	var first_ids: Array = []
	for i in range(vbox.get_child_count()):
		first_ids.append(vbox.get_child(i).get_instance_id())
	var again: int = int(vbox.call("update_game_list",
		PackedStringArray(ROOM_ADDRS), PackedStringArray(["A", "B", "C"]),
		PackedInt32Array([1, 2, 3]), PackedByteArray([0, 0, 0])))
	ok(again == 3, "同一张表再投影一次仍是 3 行")
	var same_ids := true
	for i in range(vbox.get_child_count()):
		if vbox.get_child(i).get_instance_id() != first_ids[i]:
			same_ids = false
	ok(same_ids, "行节点被**复用**（3 个 instance_id 都没变 ⇒ 没有整页重建）")
	ok(PackedStringArray(vbox.call("row_texts")) == PackedStringArray(["A", "B", "C"]),
		"刷新真的改了标签文本（不是建行时的快照）")

	# 少一条 ⇒ 那一条被 queue_free，其余顺序不变
	var doomed: Node = vbox.get_child(2)
	var left: int = int(vbox.call("update_game_list",
		PackedStringArray([ROOM_ADDRS[0], ROOM_ADDRS[1]]),
		PackedStringArray(["A", "B"]), PackedInt32Array([1, 2]), PackedByteArray([0, 0])))
	ok(left == 2, "源表少一条 ⇒ 行数 2")
	await process_frame
	await process_frame
	ok(vbox.get_child_count() == 2, "被清掉的行已经不在子节点里（实际 %d）" % vbox.get_child_count())
	ok(not is_instance_valid(doomed), "被清掉的行真的被 queue_free 释放了")
	var after := PackedStringArray(vbox.call("row_names"))
	ok(after.size() == 2 and after[0] == want_names[0] and after[1] == want_names[1],
		"清理之后剩下的两行顺序不变")

	# Join：投递 ≠ 立刻连（下一帧 `process` 才排空）
	var pending: bool = bool(vbox.call("press_join", ROOM_ADDRS[0]))
	ok(pending, "press_join 接受地址")
	ok(int(vbox.call("pending_join_request_count")) == 1, "按下之后请求还在通道里（1 条）")
	var drained := PackedStringArray(vbox.call("drain_join_requests"))
	ok(drained.size() == 1 and drained[0] == ROOM_ADDRS[0], "排空时拿到那条地址")
	ok(int(vbox.call("pending_join_request_count")) == 0, "排空后通道为空")

	# ------------------------------------------------------------------
	# 6) `GraphwarRootNode`：骨架 + 转发（持有子对象，转发一次调用）
	# ------------------------------------------------------------------
	var built: int = int(root_node.call("build_scene_graph"))
	print("      root 直接子节点 = %d" % built)
	ok(built == 4, "build_scene_graph() 建出 4 个直接子节点（3 个 holder + module 容器）")
	ok(PackedStringArray(root_node.call("holder_names")) == PackedStringArray(HOLDERS),
		"holder_names() == [ScreensControl, IAP_holder, Ad_holder]")
	for h in HOLDERS:
		ok(root_node.has_node(NodePath(h)), "子节点 %s 存在" % h)

	# 15 个屏挂在 ScreensControl 下
	var screens_control: Node = root_node.get_node("ScreensControl")
	ok(screens_control.get_child_count() == 15,
		"ScreensControl 下有 15 个屏节点（实际 %d）" % screens_control.get_child_count())
	var screen_names := PackedStringArray(root_node.call("screen_names"))
	ok(screen_names == PackedStringArray(SCREENS), "screen_names() == 15 个屏名（表序）")
	var all_present := true
	for s in SCREENS:
		if not screens_control.has_node(NodePath(s)):
			all_present = false
	ok(all_present, "15 个屏名都是 ScreensControl 的子节点")
	ok(int(root_node.call("screen_count")) == 15, "screen_count() == 15")
	ok(int(root_node.call("manager_slot_count")) == 25, "manager_slot_count() == 25（模块槽）")
	ok(int(root_node.call("manager_registration_count")) == 22,
		"manager_registration_count() == 22（GraphwarGameClient::new 的注册点）")

	# Ad_holder 是**另一个扩展类**
	var ad_holder: Node = root_node.get_node("Ad_holder")
	ok(bool(root_node.call("ad_holder_present")), "ad_holder_present()")
	ok(ad_holder.is_class("GraphwarAdSwitch"),
		"Ad_holder 的身份是 GraphwarAdSwitch（跨类持有，class_exists 级别的证据）")

	# —— 转发一次调用：purchase_confirmed → 子对象的 owned = true ——
	ok(bool(ad_holder.call("are_ads_enabled")) == false, "初始 owned = true ⇒ 不显示广告")
	ok(bool(root_node.call("purchase_denied")) == true,
		"purchase_confirmed/denied 转发链：purchase_denied() ⇒ ads_enabled() = true")
	ok(bool(ad_holder.call("are_ads_enabled")) == true,
		"**转发真的写进了子对象的字段**（Ad_holder.are_ads_enabled() = true）")
	ok(bool(ad_holder.call("owned_raw")) == false, "Ad_holder.owned == false")
	ok(bool(root_node.call("purchase_confirmed")) == false,
		"purchase_confirmed() ⇒ ads_enabled() = false（买断 ⇒ 关广告）")
	ok(bool(ad_holder.call("owned_raw")) == true, "Ad_holder.owned == true")

	# —— 转发一次调用：show_screen → 真子节点的 set_visible ——
	root_node.set("ads_slot", false)
	var writes: int = int(root_node.call("show_screen", "Campaign"))
	print("      show_screen(\"Campaign\") 写了 %d 次 set_visible" % writes)
	ok(writes == 15, "show_screen 遍历了全部 15 个条目（每个都写一次 set_visible）")
	var visible := PackedStringArray(root_node.call("visible_screens"))
	ok(visible == PackedStringArray(["Campaign"]),
		"只有被点名的那个屏可见（实际 %s）" % visible)
	ok(screens_control.get_node("Campaign").visible == true,
		"**转发到了真节点**：ScreensControl/Campaign.visible == true")
	ok(screens_control.get_node("MainMenu").visible == false,
		"其余屏被隐藏：ScreensControl/MainMenu.visible == false")
	ok(String(root_node.call("last_queried_screen")) == "Campaign",
		"last_queried_screen() == Campaign")

	# PlayScreen 进出口要动广告（`0x3af850` 的两个单列分支）
	# 前置：PlayScreen **先可见**（`0x3af850` 的两个广告分支都要求
	# `ads_slot && play_screen_visible`，见 `graphwar_root_node::ScreenSwap` 的决策表）
	root_node.call("set_screen_visible", "PlayScreen", true)
	root_node.set("ads_slot", true)
	root_node.call("show_screen", "Campaign")
	ok(screens_control.get_node("PlayScreen").visible == false,
		"ads_slot = true 时，离开 PlayScreen ⇒ PlayScreen 被隐藏")
	var deferred := PackedStringArray(root_node.call("deferred_log"))
	print("      deferred_log = %s" % deferred)
	ok(deferred.size() >= 1 and deferred[deferred.size() - 1] == "create_ad_view",
		"离开 PlayScreen ⇒ call_deferred(\"create_ad_view\") 被记下")
	# 再摆一次前置：进入 PlayScreen 时要「它本来就可见」才会拆广告
	root_node.call("set_screen_visible", "PlayScreen", true)
	var before := deferred.size()
	root_node.call("show_screen", "PlayScreen")
	var deferred2 := PackedStringArray(root_node.call("deferred_log"))
	ok(deferred2.size() == before + 1 and deferred2[deferred2.size() - 1] == "destroy_ad_view",
		"进入 PlayScreen ⇒ call_deferred(\"destroy_ad_view\")")
	ok(screens_control.get_node("PlayScreen").visible == true, "PlayScreen 现在可见")
	var visible2 := PackedStringArray(root_node.call("visible_screens"))
	ok(visible2 == PackedStringArray(["PlayScreen"]), "只剩 PlayScreen 可见（实际 %s）" % visible2)

	# 非屏名（原版 `background` 那条单列分支）不写可见性、也不报错
	root_node.set("ads_slot", false)
	var writes_bg: int = int(root_node.call("show_screen", "background"))
	ok(writes_bg == 15, "「background」不在屏表里 ⇒ 15 个真屏各写一次（不 panic）")
	ok(PackedStringArray(root_node.call("visible_screens")).is_empty(),
		"没有屏叫 background ⇒ 一个都不可见")
	var _bad := bool(root_node.call("set_screen_visible", "nonsense", true))
	ok(_bad == false, "set_screen_visible(非法名) → false")

	# ------------------------------------------------------------------
	# 7) 进树：`_ready` / `_process` 在**两种基类**下都真的被引擎调到
	# ------------------------------------------------------------------
	root_node.set("ads_slot", false)
	root.add_child(root_node)
	root.add_child(vbox)
	ok(root_node.is_inside_tree() and vbox.is_inside_tree(), "两个节点都进了场景树")
	await process_frame
	await process_frame
	var rc: int = int(root_node.call("ready_call_count"))
	var pc: int = int(root_node.call("process_call_count"))
	var vpc: int = int(vbox.call("process_call_count"))
	print("      Node 基类回调: ready=%d process=%d；VBoxContainer 基类 process=%d"
		% [rc, pc, vpc])
	ok(rc >= 1, "GraphwarRootNode._ready 被引擎调到（%d 次）" % rc)
	ok(pc >= 1, "GraphwarRootNode._process 被引擎调到（%d 次；不写 set_process）" % pc)
	ok(vpc >= 1, "GraphwarLanVBox._process 被引擎调到（%d 次）" % vpc)

	# `VBoxContainer` 会纵向排列：y 必须随子节点序号递增（纯 Node 没有这个行为）
	var y0: float = vbox.get_child(0).position.y
	var y1: float = vbox.get_child(1).position.y
	print("      VBoxContainer 排列: child0.y=%f child1.y=%f" % [y0, y1])
	ok(y1 > y0, "VBoxContainer 把第 2 行排在 1 行下面（y 递增）—— 基类真的是 VBoxContainer")

	# ------------------------------------------------------------------
	# 8) 回收：`Gd<Node>` 不保活，删节点自己 queue_free
	# ------------------------------------------------------------------
	root_node.queue_free()
	vbox.queue_free()
	await process_frame
	await process_frame
	ok(not is_instance_valid(root_node), "GraphwarRootNode 被 queue_free 回收")
	ok(not is_instance_valid(vbox), "GraphwarLanVBox 被 queue_free 回收")
	ok(not is_instance_valid(ad_holder), "子对象 Ad_holder 随父节点一起被回收（没有单独 free）")
	ok(not is_instance_valid(screens_control), "屏容器随父节点一起被回收")
	# 单独造的那个 `GraphwarAdSwitch` 不在树里 ⇒ 必须自己 free（`Gd<Node>` 不保活）
	ad.free()
	await process_frame
	ok(not is_instance_valid(ad), "单独实例化的 GraphwarAdSwitch 被显式 free")

	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("BASE_CLASSES %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
