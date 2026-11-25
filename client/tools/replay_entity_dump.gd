extends SceneTree
## 回放屏"实体落地"的输入表 dump（T4 任务书 §1 第 4 块：先 dump `EntityDrawer` 的输入表，
## 判断"缺实体"是**数据层**还是**渲染层**）。
##
## ```text
## "$GODOT" --headless --path client --script res://tools/replay_entity_dump.gd
## ```
##
## 输出四张表（都是**运行时读出来的数**，不是推断）：
## 1. 回放驱动 `actors()` / `target_positions()` —— 数据层有没有实体、位置多少、is_target；
## 2. `EntityDrawer` 的输入表：spawn 之后每个 id 的 `entity_look()` / `sprite_texture_path()` /
##    `texture_stats()`；子节点（3 sprite + 1 标签）的 position / scale / visible；
## 3. 与棋盘坐标的换算（`entity_drawer::sprite_transform` 的输入是控件尺寸 1000×600）；
## 4. `missing_assets()` / `assets_available()` —— 贴图是不是真资产。
##
## 这是**诊断工具**，不是游戏逻辑。

const DRIVER_CLASS := "GwReplayDriver"
const NODE_CLASS := "EntityDrawer"
const EXTENSION_PATH := "res://gw2rebuild.gdextension"
const CANVAS := Vector2(1000, 600)


func _initialize() -> void:
	if not ClassDB.class_exists(NODE_CLASS):
		GDExtensionManager.load_extension(EXTENSION_PATH)
	var path := _replay_path()
	if path == "":
		print("!! 找不到回放文件（设 GW2_REPLAY=<path>）")
		quit(2)
		return
	print("== 回放文件 %s" % path)

	# ---------------- 1. 数据层 ----------------
	var drv: Object = ClassDB.instantiate(DRIVER_CLASS)
	if drv == null or not bool(drv.call("load", path)):
		print("!! 驱动载入失败")
		quit(2)
		return
	# 停到"载入态"那个 tick（默认第 1 个结算批的 exec_tick = 558；`GW2_TICK` 可覆盖）
	var want_tick := int(OS.get_environment("GW2_TICK"))
	if want_tick == 0:
		var bs: Array = drv.call("batches")
		want_tick = int(bs[0].exec_tick) if bs.size() > 0 else 1
	drv.call("seek", want_tick)
	drv.call("pause")
	print("载入 tick = %d（playing=%s）；结算批 = %s" % [want_tick, str(drv.call("playing")),
		str(drv.call("batches"))])
	var snap: Dictionary = drv.call("snapshot")
	print("snapshot: tick=%d first=%d last=%d events=%d playing=%s" % [
		int(snap.tick), int(snap.first_tick), int(snap.last_tick),
		int(snap.events), str(snap.playing)])
	var actors: Array = drv.call("actors")
	print("actors() = %d 个" % actors.size())
	for a in actors:
		print("   id=%-5d pos=(%8.3f,%8.3f) r=%.3f alive=%s is_target=%s team=%d name='%s' color=%s" % [
			int(a.id), a.pos.x, a.pos.y, float(a.radius), str(a.alive), str(a.is_target),
			int(a.team), String(a.name), str(a.get("color", null))])
	var tps: Array = drv.call("target_positions")
	print("target_positions() = %d 个：%s" % [tps.size(), str(tps)])

	# ---------------- 2. 渲染层：把同一批实体喂给真 EntityDrawer ----------------
	var drawer: Control = ClassDB.instantiate(NODE_CLASS)
	drawer.size = CANVAS
	get_root().add_child(drawer)
	await process_frame
	for a in actors:
		if bool(a.is_target):
			continue
		drawer.call("spawn_entity", int(a.id), a.pos.x, a.pos.y)
		drawer.call("set_entity_name", int(a.id), String(a.name))
	for i in tps.size():
		var p: Vector2 = tps[i]
		drawer.call("spawn_entity", 1000 + i, p.x, p.y)
		drawer.call("set_entity_look", 1000 + i, "target", "no_face", "no_hat")
	await process_frame
	await process_frame
	print("EntityDrawer.entity_count() = %d" % int(drawer.call("entity_count")))
	print("assets_available() = %s   missing_assets() = %s" % [
		str(drawer.call("assets_available")), str(drawer.call("missing_assets"))])
	print("texture_stats() = %s" % str(drawer.call("texture_stats")))
	for ch in drawer.get_children():
		var c := ch as Node
		if c is Sprite2D:
			var sp := c as Sprite2D
			var tex := sp.texture
			print("   Sprite2D pos=(%8.2f,%8.2f) scale=%s visible=%s tex=%s %s" % [
				sp.position.x, sp.position.y, str(sp.scale), str(sp.visible),
				("null" if tex == null else "%dx%d" % [tex.get_width(), tex.get_height()]),
				("" if tex == null else String(tex.resource_path))])
		elif c is RichTextLabel:
			var lbl := c as RichTextLabel
			print("   RichTextLabel pos=(%8.2f,%8.2f) visible=%s text='%s' modulate=%s size=%s" % [
				lbl.position.x, lbl.position.y, str(lbl.visible), lbl.text,
				str(lbl.modulate), str(lbl.size)])
	var ids: Array = []
	for a in actors:
		ids.append(int(a.id))
	for i in tps.size():
		ids.append(1000 + i)
	for id in ids:
		print("   id=%-5d look=%s tex0='%s' tex1='%s' tex2='%s'" % [
			int(id), str(drawer.call("entity_look", int(id))),
			String(drawer.call("sprite_texture_path", int(id), 0)),
			String(drawer.call("sprite_texture_path", int(id), 1)),
			String(drawer.call("sprite_texture_path", int(id), 2))])
	quit(0)


## 回放文件路径：`GW2_REPLAY` 优先；否则扫冻结快照里的 `1793897298`。
func _replay_path() -> String:
	var env := OS.get_environment("GW2_REPLAY")
	if env != "" and FileAccess.file_exists(env):
		return env
	var home := OS.get_environment("HOME")
	for p in [home + "/.local/share/godot/app_userdata/Graphwar/replays/1793897298",
			OS.get_environment("GW2_SNAPSHOT") + "/1793897298"]:
		if p != "/1793897298" and FileAccess.file_exists(p):
			return p
	return ""
