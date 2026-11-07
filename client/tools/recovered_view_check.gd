extends SceneTree
## 原版视图脚本（`recovered-src/*.gd`）**原样**挂进本项目的空壳里能不能跑 —— headless 验收。
##
## ```text
## cargo build --features gdext-bindings --offline
## cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
## export XDG_DATA_HOME=... XDG_CACHE_HOME=... XDG_CONFIG_HOME=...
## godot --headless --path client --import                 # 新增 .gd 后必须先 import
## godot --headless --path client --script res://tools/recovered_view_check.gd
## ```
##
## 它回答的是 `dispatch/08-viewport.md` 的三个问题：
##
## | 段 | 问题 | 判据 |
## |---|---|---|
## | A | 副本是不是**逐字节**等于 `recovered-src/`？ | 绝对路径读原文件、与 `res://` 副本逐字节比对 |
## | B | 52 个原版脚本里，哪些**能解析**？ | `load()` 非 null；失败把 `get_error_line()` 打出来 |
## | C | 能解析的，**挂进树**之后 `_ready` 有没有跑出效果？ | 逐文件断言可观测副作用（字号 / 最小尺寸 / 子节点数 / 标签标题） |
## | D | 被"缺成员"挡住的那一个，**缺的到底是哪几个成员**？ | `AdContainer.gd` 三步：原样 → 有节点无成员 → 补 2 个成员 |
## | E | 插件类能不能用？ | `ClassDB` 里有没有 AdMob 原生类 + 包里有没有插件 `.so` |
##
## ⚠️ 本脚本**不改** `recovered-src/` 一个字节：副本由 `cp` 产生，A 段每次都验。
## ⚠️ GDScript 的运行时错误（`Invalid call` / `Node not found`）只会打到 stderr，脚本自己
##    看不见。所以"有没有报错"由 stderr 归属（错误行带脚本路径），本脚本只断言**效果**。
## ⚠️ headless 的视口是 **64×64**（实测：`root.size = (1600,900)` 设下去，下一帧被 dummy
##    DisplayServer 改回 64×64）⇒ 所有 `screen_size.x / N` 算出来都是 1~8 这种小数字。
##    所以 C 段断言的是**公式**（`font_size == int(视口宽 / N)`），不是那个绝对字号。
##
## 这是**验收工具**，不是游戏逻辑（同 `selftest.gd` / `draw_classes_check.gd`，不进运行时路径）。

const REC := "res://tools/recovered/"
const SRC := "../recovered-src/"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"

## 原版 `GraphwarAdSwitch`（我们注册的类）**没有**的两个成员 —— 由 `AdContainer.gd` 自己点名：
## `ad_loaded`（第 8 行 `ad_holder.ad_loaded.connect`）与 `ad_size`（第 12-13 行读 `.x` / `.y`）。
const AD_STUB_SCRIPT := "res://tools/recovered_ad_holder_stub.gd"

var _checks := 0
var _failures: PackedStringArray = []

## B 段挂过的宿主，最后统一释放（见 `_attach_all` 的注）。
var _sweep_hosts: Array = []

## 日志里出现 `SCRIPT ERROR` 的文件名（B/C 段跑完、stderr 由人/外壳核对时用；
## 脚本自己拿不到 stderr，这里只是把"挂过哪些"打印成可核对的顺序）。
var _attached: PackedStringArray = []

func ok(cond: bool, what: String) -> void:
	_checks += 1
	if cond:
		print("ok   %s" % what)
	else:
		_failures.append(what)
		print("FAIL %s" % what)

## 建一个"宿主"：一个满屏的 Control，里面塞上指定类型的子节点（原版脚本靠 `get_children()` 找它们）。
func _host(children: Array = []) -> Control:
	var host := Control.new()
	host.size = Vector2(1600.0, 900.0)
	for c in children:
		host.add_child(c)
	return host

## 把脚本挂到一个新节点上、连宿主一起进树，等两帧让 `_ready` / `call_deferred` 跑完。
## `children` 挂在**脚本节点自己**身上（原版这些脚本都是 `for node in get_children()`）。
func _attach(script: Script, children: Array = []) -> Node:
	var inst: Node = script.new()
	for c in children:
		inst.add_child(c)
	var host: Control = _host()
	host.add_child(inst)
	root.add_child(host)
	await process_frame
	await process_frame
	inst.set_meta("probe_host", host)
	return inst

func _drop(inst: Node) -> void:
	var host: Variant = inst.get_meta("probe_host", null)
	if host != null and is_instance_valid(host):
		host.queue_free()
	elif is_instance_valid(inst):
		inst.queue_free()
	await process_frame

func _label() -> Label:
	var l := Label.new()
	l.name = "probe_child"
	return l

func _has_font_override(n: Object) -> bool:
	return n is Control and (n as Control).has_theme_font_size_override("font_size")

# ---------------------------------------------------------------------------
# A) 副本与原文件逐字节相同
# ---------------------------------------------------------------------------

func _check_copies(files: PackedStringArray) -> int:
	var base: String = ProjectSettings.globalize_path("res://")
	var originals := base.path_join(SRC).simplify_path()
	var same := 0
	for f in files:
		var orig_path: String = originals.path_join(f)
		var a := FileAccess.get_file_as_bytes(orig_path)
		var b := FileAccess.get_file_as_bytes(REC + f)
		if a.size() > 0 and a == b:
			same += 1
		else:
			print("     %s 与 %s 不一致（%d vs %d 字节）" % [f, orig_path, a.size(), b.size()])
	ok(same == files.size(), "A：%d/%d 个副本与 recovered-src 原文件**逐字节相同**" % [same, files.size()])
	return same

# ---------------------------------------------------------------------------
# B) 52 个脚本能不能解析 + 挂进树
# ---------------------------------------------------------------------------

func _parse_all(files: PackedStringArray) -> Dictionary:
	var scripts := {}
	var parsed := 0
	var failed: PackedStringArray = []
	for f in files:
		var s: Script = load(REC + f)
		if s == null or not s.can_instantiate():
			failed.append(f)
			print("     ✗ %s 解析失败" % f)
			continue
		scripts[f] = s
		parsed += 1
	ok(parsed == files.size() - 1, "B：%d/%d 个原版脚本能在 Godot 4.7.2 里**解析**" % [parsed, files.size()])
	ok(failed.size() == 1 and failed[0] == "Ad_holder.gd",
		"B：唯一解析失败的是 Ad_holder.gd —— 它 `extends Control` 却用 `AdView` / `MobileAds` / `AdSize` / `AdPosition` / `AdRequest` / `AdListener` / `LoadAdError` 这 7 个 AdMob 插件类型")
	if not failed.is_empty():
		print("     解析失败：%s" % ", ".join(failed))
	return scripts

## 挂树：能不能 `new()` 出一个 Node 并进树（不判 `_ready` 的语义）。
## ⚠️ 宿主**最后统一释放**：`android_iap.gd` 的 `_ready` 里 `await create_timer(1)`，
## 提前 free 会在日志里造出「Resumed function after Object freed」这种**测试自身的**噪声。
func _attach_all(scripts: Dictionary) -> void:
	var as_node := 0
	var not_node: PackedStringArray = []
	var in_tree := 0
	for f in scripts:
		var inst: Variant = (scripts[f] as Script).new()
		if not (inst is Node):
			not_node.append(f)
			continue
		as_node += 1
		var host := _host([_label()])
		host.add_child(inst)
		root.add_child(host)
		_attached.append(f)
		in_tree += 1
		_sweep_hosts.append(host)
	print("     B：%d 个能 new() 成 Node 并进树，%d 个不是 Node 脚本（%s）"
		% [as_node, not_node.size(), ", ".join(not_node)])
	ok(in_tree + not_node.size() == scripts.size(),
		"B：每个解析成功的脚本要么进树、要么被判为非 Node（%d + %d == %d）"
			% [in_tree, not_node.size(), scripts.size()])
	ok(not_node.size() == 1 and not_node[0] == "ExtraOptionsVbox.gd",
		"B：唯一的非 Node 脚本是**空文件** ExtraOptionsVbox.gd（原版 1 字节，只有换行）")

# ---------------------------------------------------------------------------
# C) 逐文件的效果断言（只用原版脚本自己的副作用当判据）
# ---------------------------------------------------------------------------

## 通用形状：`_ready` 里 `for node in get_children(): node.add_theme_font_size_override(...)`。
## `divisor` 是该文件源码里写死的那个除数（`font_size = screen_size.x / N`）—— 断言的是**公式**
## 而不是"有个覆盖"，因为 headless 的视口是 **64×64**（见文件头的注），绝对值没有意义。
func _check_font_child(file: String, divisor: int) -> bool:
	var s: Script = load(REC + file)
	if s == null:
		ok(false, "C：%s 能加载" % file)
		return false
	var child := _label()
	var inst: Node = await _attach(s, [child])
	var got: bool = _has_font_override(child)
	var size: int = child.get_theme_font_size("font_size") if got else -1
	var want: int = int(float(root.get_visible_rect().size.x) / float(divisor))
	ok(got and size == want,
		"C：%s 的子节点 font_size = 视口宽/%d = %d（实际 %d）" % [file, divisor, want, size])
	await _drop(inst)
	return got

func _check_custom_minimum_size(file: String) -> bool:
	var s: Script = load(REC + file)
	if s == null:
		ok(false, "C：%s 能加载" % file)
		return false
	var child := _label()
	var inst: Node = await _attach(s, [child])
	var sx: float = float(root.get_visible_rect().size.x)
	var want := Vector2(sx * 0.15 / 3.0, sx * 0.15 / 3.0)
	ok(child.custom_minimum_size.is_equal_approx(want) and want.x > 0.0,
		"C：%s 给子节点设的 custom_minimum_size = %s（期望 %s）"
			% [file, child.custom_minimum_size, want])
	await _drop(inst)
	return true

func _check_margin(file: String, key: String, expr: String) -> void:
	var s: Script = load(REC + file)
	if s == null:
		ok(false, "C：%s 能加载" % file)
		return
	var inst: Node = await _attach(s)
	var sx: float = float(root.get_visible_rect().size.x)
	var want: int = int(-sx / 10.0) if expr == "/10" else int(-sx / 100.0)
	var got: int = (inst as Control).get_theme_constant(key) if (inst as Control).has_theme_constant_override(key) else 999999
	ok(got == want, "C：%s 把 `%s` 设成 -视口宽%s = %d（实际 %d）" % [file, key, expr, want, got])
	await _drop(inst)

# ---------------------------------------------------------------------------
# D) `AdContainer.gd` 三步：原样 → 有节点无成员 → 补 2 个成员
# ---------------------------------------------------------------------------

func _check_ad_container() -> void:
	var s: Script = load(REC + "AdContainer.gd")
	ok(s != null, "D：AdContainer.gd 能加载")
	if s == null:
		return

	# ① 原样：`/root/GraphwarRootNode/Ad_holder` 不存在 ⇒ `_ready` 第 3 行就该炸（stderr）。
	var bare: Node = await _attach(s)
	ok(bare.custom_minimum_size == Vector2.ZERO,
		"D①：没有 `/root/GraphwarRootNode` 时 custom_minimum_size 保持 0（实际 %s）"
			% bare.custom_minimum_size)
	await _drop(bare)

	# ② 用**我们注册的** GraphwarRootNode 建骨架（它自己会造 Ad_holder = GraphwarAdSwitch）。
	var rn: Node = GraphwarRootNode.new()
	rn.name = "GraphwarRootNode"
	root.add_child(rn)
	await process_frame
	var ad := rn.get_node_or_null("Ad_holder")
	ok(ad != null, "D②：GraphwarRootNode 建出了 Ad_holder（类 %s）"
		% (ad.get_class() if ad != null else "无"))
	ok(ad is GraphwarAdSwitch, "D②：Ad_holder 就是我们注册的 GraphwarAdSwitch")
	ok(ad != null and ad.has_method("are_ads_enabled"),
		"D②：它**有** `are_ads_enabled()`（原版 `Ad_holder.gd` 用的那个）")
	ok(ad != null and not _has_member(ad, "ad_loaded"),
		"D②：它**没有** `ad_loaded` 信号 —— 这正是 `AdContainer.gd` 第 8 行要的")
	ok(ad != null and not _has_member(ad, "ad_size"),
		"D②：它**没有** `ad_size` 属性 —— 这正是 `AdContainer.gd` 第 12-13 行要的")

	var mid: Node = await _attach(s)
	ok(mid.custom_minimum_size == Vector2.ZERO,
		"D②：有节点但缺这 2 个成员时，仍然没效果（custom_minimum_size = %s）"
			% mid.custom_minimum_size)
	await _drop(mid)
	rn.queue_free()
	await process_frame

	# ③ 补齐 2 个成员（`client/tools/recovered_ad_holder_stub.gd`，试验专用，不是原版）
	var stub_script: Script = load(AD_STUB_SCRIPT)
	ok(stub_script != null, "D③：试验用 stub 脚本能加载（%s）" % AD_STUB_SCRIPT)
	if stub_script == null:
		return
	var rn2: Node = GraphwarRootNode.new()
	rn2.name = "GraphwarRootNode"
	root.add_child(rn2)
	await process_frame
	var old_ad := rn2.get_node_or_null("Ad_holder")
	var holder: Node = stub_script.new()
	holder.name = "Ad_holder"
	if old_ad != null:
		rn2.remove_child(old_ad)
		old_ad.queue_free()
	rn2.add_child(holder)
	await process_frame
	ok(rn2.get_node_or_null("Ad_holder") == holder, "D③：stub 顶替成了 `/root/GraphwarRootNode/Ad_holder`")

	var inst: Node = await _attach(s)
	await process_frame
	# 原版链路：`ad_loaded` 一发 ⇒ `_update_size()` ⇒ `custom_minimum_size = ad_size`
	holder.set("ad_size", Vector2i(320, 50))
	holder.emit_signal("ad_loaded")
	await process_frame
	await process_frame
	ok(inst.custom_minimum_size == Vector2(320.0, 50.0),
		"D③：`ad_loaded` 抬起 ⇒ 原版 `_update_size()` 把 custom_minimum_size 写成 %s（期望 (320, 50)）"
			% inst.custom_minimum_size)
	# `visibility_changed` 那条线（`_ready` 第 1 行连的）
	holder.set("ad_size", Vector2i(640, 100))
	(inst as Control).visible = false
	await process_frame
	await process_frame
	ok(inst.custom_minimum_size == Vector2(640.0, 100.0),
		"D③：`visibility_changed` 抬起 ⇒ 也走同一条 `_update_size()`（实际 %s）"
			% inst.custom_minimum_size)

	await _drop(inst)
	rn2.queue_free()
	await process_frame

func _has_member(obj: Object, name: String) -> bool:
	for p in obj.get_property_list():
		if String(p.name) == name:
			return true
	for s in obj.get_signal_list():
		if String(s.name) == name:
			return true
	return false

# ---------------------------------------------------------------------------
# E) 插件类：原生类不在 ClassDB 里
# ---------------------------------------------------------------------------

func _check_plugins() -> void:
	var natives := ["MobileAds", "AdView", "AdSize", "AdPosition", "AdRequest", "AdListener",
		"AndroidIAPP", "InAppStore"]
	var present: PackedStringArray = []
	for n in natives:
		if ClassDB.class_exists(n):
			present.append(n)
	ok(present.is_empty(),
		"E：AdMob / IAP 的 8 个原生类一个都不在 ClassDB 里（实际在：%s）" % ", ".join(present))
	ok(not Engine.has_singleton("AndroidIAPP"), "E：没有 `AndroidIAPP` 单例")
	ok(not Engine.has_singleton("InAppStore"), "E：没有 `InAppStore` 单例")
	ok(not Engine.has_singleton("AlbumIOS"), "E：没有 `AlbumIOS` 单例")
	ok(not Engine.has_singleton("GodotAndroidMediaStorePlugin"),
		"E：没有 `GodotAndroidMediaStorePlugin` 单例")

# ---------------------------------------------------------------------------

func _initialize() -> void:
	if not ClassDB.class_exists("GraphwarButton"):
		var err: int = GDExtensionManager.load_extension(EXTENSION_PATH)
		print("     GDExtensionManager.load_extension(%s) -> %d" % [EXTENSION_PATH, err])
	ok(ClassDB.class_exists("GraphwarButton"), "前置：GraphwarButton（原版 `CampaingLevelsButtons.gd` 要 new 它）")
	ok(ClassDB.class_exists("GraphwarRootNode"), "前置：GraphwarRootNode（原版 `AdContainer.gd` 要 get_node 它）")
	ok(ClassDB.class_exists("GraphwarAdSwitch"), "前置：GraphwarAdSwitch（`Ad_holder` 的类）")

	var files := PackedStringArray()
	for f in DirAccess.get_files_at(REC):
		if f.ends_with(".gd"):
			files.append(f)
	files.sort()
	print("     副本目录 %s 里 %d 个 .gd" % [REC, files.size()])
	ok(files.size() == 52, "A：`client/tools/recovered/` 里正好 52 个原版游戏脚本（实际 %d）" % files.size())

	_check_copies(files)
	var scripts := _parse_all(files)
	await _attach_all(scripts)

	print("     ---- C 段：逐文件效果断言（`font_size = 视口宽 / N`，N 是原版源码里的除数）----")
	for pair in [["VersionLabel.gd", 60], ["LanLobbyTitle.gd", 30], ["MainMenuButtonResize.gd", 30],
			["UnlockButtonSize.gd", 30], ["GameSetupPlayerEntry.gd", 40], ["RoomName.gd", 30],
			["ExitButtonContainer.gd", 30], ["TimerContainer.gd", 30], ["FireContainer.gd", 30],
			["FxContainer.gd", 60], ["OffsetContainer.gd", 60], ["LobbyButtonContainer.gd", 30],
			["TitleSize.gd", 8], ["CampaingButtonSize.gd", 15], ["ChatContainer.gd", 50],
			["LevelDescriptionScrollContainer.gd", 40], ["TurnLimitText.gd", 20],
			["IpPopUpMargin.gd", 40], ["HBoxContainer.gd", 30], ["ViewFunctionsLabel.gd", 10]]:
		await _check_font_child(pair[0], pair[1])
	await _check_custom_minimum_size("ButtonGrid.gd")
	await _check_margin("SoldierIconMarginContainer.gd", "margin_left", "/100")
	await _check_margin("HBoxVictoryIcon.gd", "separation", "/10")

	# `ResizeContainer.gd`：`custom_minimum_size = 整个视口`
	var rc: Node = await _attach(load(REC + "ResizeContainer.gd"))
	ok((rc as Control).custom_minimum_size.is_equal_approx(root.get_visible_rect().size)
			and (rc as Control).custom_minimum_size.x > 0.0,
		"C：ResizeContainer.gd 把 custom_minimum_size 设成整个视口（%s）" % (rc as Control).custom_minimum_size)
	await _drop(rc)

	# `OptionsContainer.gd`：`custom_minimum_size.y = 0.25 * 父尺寸`
	# （它只在 `resized` 里算，所以必须让父容器真的改一次尺寸，否则信号不会发）
	var oc_host: Control = _host()
	oc_host.size = Vector2(800.0, 400.0)
	var oc: Node = (load(REC + "OptionsContainer.gd") as Script).new()
	oc_host.add_child(oc)
	root.add_child(oc_host)
	await process_frame
	(oc as Control).size = Vector2(400.0, 200.0)
	await process_frame
	await process_frame
	ok((oc as Control).custom_minimum_size.y == 100.0,
		"C：OptionsContainer.gd 把 custom_minimum_size.y 设成父高 400 的 1/4（实际 %s）"
			% (oc as Control).custom_minimum_size)
	oc_host.queue_free()
	await process_frame

	# `PlayerTabContainer.gd`：三个页签标题（原文含两侧空格）
	var tc: TabContainer = (load(REC + "PlayerTabContainer.gd") as Script).new()
	for i in range(3):
		var tab := Control.new()
		tab.name = "Tab%d" % i
		tc.add_child(tab)
	var tc_host: Control = _host()
	tc_host.add_child(tc)
	root.add_child(tc_host)
	await process_frame
	# 原版字面量（含 U+2009 THIN SPACE，已按 `recovered-src/PlayerTabContainer.gd` 逐字符核对）
	ok(String(tc.get_tab_title(0)) == " \u2009 Skins  \u2009"
			and String(tc.get_tab_title(2)) == "  \u2009Hats\u2009  \u2009",
		"C：PlayerTabContainer.gd 写了三个页签标题（0=%s，2=%s）"
			% [tc.get_tab_title(0).c_escape(), tc.get_tab_title(2).c_escape()])
	tc_host.queue_free()
	await process_frame

	# ★ 旗舰：原版 `CampaingLevelsButtons.gd` 用 `GraphwarButton.new()` 造 21 个按钮
	var clb: Node = await _attach(load(REC + "CampaingLevelsButtons.gd"))
	await process_frame
	var buttons: Array = clb.get("buttons")
	var all_gwb := true
	var names := PackedStringArray()
	var inputs := PackedStringArray()
	for b in buttons:
		if not (b is GraphwarButton):
			all_gwb = false
		else:
			names.append(String(b.name))
			inputs.append(String(b.get("input_text")))
	ok(buttons.size() == 21, "★ CampaingLevelsButtons.gd 造了 21 个按钮（实际 %d）" % buttons.size())
	ok(all_gwb and buttons.size() == 21, "★ 21 个全是**我们注册的** GraphwarButton")
	ok(clb.get_child_count() == 21, "★ 21 个都进了场景树（child_count = %d）" % clb.get_child_count())
	ok(names.size() == 21 and names[0] == "level_0" and names[20] == "level_20",
		"★ 名字是原版的 `level_0` … `level_20`（首 %s / 末 %s）"
			% [names[0] if names.size() > 0 else "", names[-1] if names.size() > 0 else ""])
	ok(inputs.size() == 21 and inputs[0] == "level_0" and inputs[20] == "level_20",
		"★ `input_text` 也照原样写进去了（首 %s / 末 %s）"
			% [inputs[0] if inputs.size() > 0 else "", inputs[-1] if inputs.size() > 0 else ""])
	await _drop(clb)

	print("     ---- D 段：AdContainer 三步 ----")
	await _check_ad_container()

	print("     ---- E 段：插件 ----")
	_check_plugins()

	# B 段挂过的东西最后统一释放；先等 1.5 s，让 `android_iap.gd` 的 `await create_timer(1)`
	# 真的跑到 `run_iapp_billing()`（否则它的错误会被"提前 free"掩盖）
	await create_timer(1.5).timeout
	for h in _sweep_hosts:
		if is_instance_valid(h):
			h.queue_free()
	_sweep_hosts.clear()
	await process_frame

	_finish()

func _finish() -> void:
	var passed: int = _checks - _failures.size()
	print("RECOVERED_VIEW %d passed, %d failed" % [passed, _failures.size()])
	for f in _failures:
		print("  FAILED: %s" % f)
	quit(1 if _failures.size() > 0 else 0)
