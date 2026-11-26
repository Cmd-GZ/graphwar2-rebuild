extends GwScreen
## EndGamePopUp —— 结算。
##
## **节点树全部来自原版 `.tscn`**：`client/scenes/screens/endgame_pop_up.tscn` 与
## `recovered-src/endgame_pop_up.tscn` **逐字节相同**（349 行 / 69 节点）。
## 1920×1160 下量到的矩形（绝对像素，真机真帧）：
##
##   弹窗根   (1,1) 1920×1160    ← 原版根的四个 offset 都是 **1.0**（`.tscn` 里就是）
##   面板     (483,235) 956×691  ← 三列 1:2:1；竖直三条带 **1:3:1**（230 / 691 / 231）
##   返回键   (1,1) 146×96      ← 挂在根节点上的浮层
##
## 原版结构与三个"和别处不一样"的地方（现在全在 `.tscn` 字节里，不再由代码摆）：
##   * **没有 AdContainer**（VBox 只有三条带 ⇒ 分隔只有 2 个 4 px，比例才正好 230/691/231）；
##   * `SelfVictoryLabel` 是**静态** `theme_override_font_sizes/font_size = 70` + `title_theme.tres`
##     （Eraser 手写体），`TeamVictoryLabel` 是静态 `font_size = 40`；两个都**不**跟窗口缩放
##     —— 而基类 `GwScreen.apply_font_scale()` 会把每个 Label 改成 宽/30，所以本类在
##     `super()` 之后把 `.tscn` 的静态值**盖回去**（值在 `build()` 那一刻从节点上读出来，
##     不在这里重抄常数）；
##   * 十个 `VictoryPlayerContainerN` 静态是 `visible = false`；`HBoxVictoryIcon.gd`
##     把行内 `separation` 设成 **-屏幕宽/10 = -192 px**（图标互相叠着画）——
##     那是**原版脚本自己**在 `_ready`/`size_changed` 里做的，本类不再插手。
##
## # 控制器归属：这一屏在 Rust 里（`FN_SHOW_VICTORY 0x3da130`）
##
## 原版这一屏的内容由 `GraphwarPlayScreenControl` 的 **`0x3da130`** 填（写
## `SelfVictoryLabel` / `TeamVictoryLabel` 两条文本 + 胜利者头像三层贴图），它取的节点名
## 是 `.rodata` 里的字面量（**观测**：串 + 取节点调用点）。
##
## 那条接线**以前是死链**（T33/T35 记的"`FN_SHOW_VICTORY 0x3da130` 未接节点层"：
## 节点名表只躺在 `graphwar_play_screen_control::OTHER_NODES` 里，没有任何消费者）。
## **本轮接上（2026-11-26，docs/06 §97）**：
##   1. `src/_scaffold/endgame_class.rs::GwEndGame::show_victory_nodes()` 把那张表
##      （节点名 + 归属地址 `0x3da130`/`0x3d9000` + 是否前缀形式）搬给 GDScript；
##   2. `build()` **只按这张表**去原版 `.tscn` 里取节点：一个取不到就 `push_error`，
##      不许静默退化 —— 于是"我方写的节点"与"原版 `0x3da130` 写的节点"是**同一批同名节点**；
##   3. 之后的每一次写入（`on_enter()`）都走这些句柄。
## ⚠️ 档位：节点名与归属函数 = **观测**；`VictoryPlayerContainer` 是**前缀**（原版
## `.tscn` 里是 `…Container1..10`）= **推断**（缺那处 `format`/下标拼接的指令）；
## "哪种结局显示哪条横幅"见 `endgame_class.rs` 文件头（第一行观测、后两行推断）。
##
## 只有真的有人全灭才会出现（`docs/92` §4.13）。本项目由 `App.report_match_result()` 触发。
##
## 2026-11-26（`docs/06` §77）真值（1920×1160）：标题 `YOU WON` 白字 ink 329×65 @ x796..1124
## y245..309；胜利者头像合并 ink 395×289 @ x754..1148 y435..723；`Team 1 Victory!` @ y882..921。
## 两条文案都是**白字**。

## 原版 `.tscn` 里 10 个胜利者格子（`VictoryPlayerContainer1..10`）。
const VICTORY_SLOTS := 10
## 有人的格子占行高的多少（**推断**，见下面 `on_enter()` 里的注）。
const SLOT_ROW_FRACTION := 0.5

## 原版 `.tscn` 里的节点路径前缀（一字不改）。
const CONTENT_PATH := "MarginContainer/VBoxContainer/HBoxContainer2/MarginContainer2/VBoxContainer"
const ICONS_ROW_PATH := CONTENT_PATH + "/HBoxContainer"

var _self_label: Label
var _team_label: Label
var _icons_row: HBoxContainer
var _slots: Array[Control] = []
## 每个格子三层的句柄：`{"skin":TextureRect, "face":…, "hat":…}`（顺序与 `_slots` 对齐）。
var _slot_layers: Array[Dictionary] = []
## `.tscn` 里两条横幅的**静态**字号（`build()` 从节点上读出来 —— 真值在 `.tscn` 字节里）。
var _self_font_size := 0
var _team_font_size := 0

func _ready() -> void:
	font_divisor = 30
	super()
	_apply_fixed_metrics()
	var root := get_tree().root
	if root != null and not root.size_changed.is_connected(_apply_fixed_metrics):
		# 在 `GwScreen._install_font_scaling()` 之后连，保证自己**后**执行 ⇒ 盖得住
		root.size_changed.connect(_apply_fixed_metrics)

## 节点全部来自原版 `.tscn`；这里只做两件事：① 按 `0x3da130` 的节点表取句柄；
## ② 把 `.tscn` 的静态字号读出来（`super()` 之后会被基类的字号缩放覆盖，要盖回去）。
func build() -> void:
	var bridge = _endgame_math()
	if bridge == null:
		push_error("EndGamePopUp: 缺内核 GwEndGame —— 无法按 0x3da130 的节点表接线")
		return
	var table: Array = bridge.call("show_victory_nodes")
	if table.is_empty():
		push_error("EndGamePopUp: 内核返回的 0x3da130 节点表为空")
		return
	var resolved := _resolve_nodes(table)
	_icons_row = get_node_or_null(ICONS_ROW_PATH) as HBoxContainer
	_self_label = _one(resolved, "SelfVictoryLabel") as Label
	_team_label = _one(resolved, "TeamVictoryLabel") as Label
	if _self_label != null:
		_self_font_size = _self_label.get_theme_font_size("font_size")
	if _team_label != null:
		_team_font_size = _team_label.get_theme_font_size("font_size")
	# 十个格子 + 每格三层贴图（`SkinIcon` / `FaceIcon` / `HatIcon`，名字都来自那张表）
	var slot_nodes: Array = resolved.get("VictoryPlayerContainer", [])
	_slots.clear()
	_slot_layers.clear()
	for i in mini(slot_nodes.size(), VICTORY_SLOTS):
		var slot: Control = slot_nodes[i]
		_slots.append(slot)
		var layer_margin: Node = slot.get_child(0) if slot.get_child_count() > 0 else null
		_slot_layers.append({
			"skin": layer_margin.get_node_or_null("SkinIcon") as TextureRect
				if layer_margin != null else null,
			"face": layer_margin.get_node_or_null("FaceIcon") as TextureRect
				if layer_margin != null else null,
			"hat": layer_margin.get_node_or_null("HatIcon") as TextureRect
				if layer_margin != null else null,
		})

## 把内核那张 `{name, fn, numbered, evidence}` 表**逐个落到节点上**。
## 取不到任何一个 ⇒ `push_error`（接线断了要吵，不许静默）。
func _resolve_nodes(table: Array) -> Dictionary:
	var resolved := {}
	for entry in table:
		var node_name := String(entry.get("name", ""))
		if node_name == "":
			continue
		if bool(entry.get("numbered", false)):
			var list: Array = []
			for i in VICTORY_SLOTS:
				var n := get_node_or_null("%s/%s%d" % [ICONS_ROW_PATH, node_name, i + 1])
				if n == null:
					push_error("EndGamePopUp: 0x3da130 要的节点 %s%d 在原版 .tscn 里不存在"
						% [node_name, i + 1])
					continue
				list.append(n)
			resolved[node_name] = list
		else:
			# 同名节点在 10 个格子里重复出现（`SkinIcon` ×10）⇒ 全部收下
			var all := find_children(node_name, "", true, false)
			if all.is_empty():
				push_error("EndGamePopUp: 0x3da130 要的节点 %s 在原版 .tscn 里不存在" % node_name)
				continue
			resolved[node_name] = all
	return resolved

func _one(resolved: Dictionary, node_name: String) -> Node:
	var arr: Array = resolved.get(node_name, [])
	if arr.is_empty():
		push_error("EndGamePopUp: 缺节点 %s（0x3da130 的节点表成员）" % node_name)
		return null
	return arr[0]

## `.tscn` 的两条横幅字号是静态 override，基类 `apply_font_scale()` 会把它们改成 宽/30，
## 所以在它之后再写一遍；`HBoxVictoryIcon.gd` 的 `separation` 由原版脚本自己管，本类不碰。
func _apply_fixed_metrics() -> void:
	if _self_label != null and _self_font_size > 0:
		_self_label.add_theme_font_size_override("font_size", _self_font_size)
	if _team_label != null and _team_font_size > 0:
		_team_label.add_theme_font_size_override("font_size", _team_font_size)

# ---------------------------------------------------------------- 数据

func on_enter() -> void:
	_apply_fixed_metrics()
	var app_node := app()
	if app_node == null:
		return
	var summary: Dictionary = app_node.match_summary()
	var winner_team := int(summary.get("winner_team", 0))
	var human_team := int(summary.get("human_team", 0))
	# 胜方字节：`0` = Team1、`1` = Team2、`2` = 没人赢（内核四行表，`docs/14` §11.4）。
	# 本地/战役那条路没有字节（`report_match_result()` 只给队伍号）⇒ 用队伍号当字节，
	# 语义相同（字节就是"哪一队赢"）。见 `app.gd::match_summary()`。
	var byte := int(summary.get("winner_byte", winner_team))
	var texts := _label_texts(app_node, byte, human_team)
	if _self_label != null:
		_self_label.text = String(texts.get("self", ""))
		# 原版两条都是**白字**（真帧 ink 里全是 #FFFFFF）；主题默认就是白，所以**不覆盖颜色**。
		_self_label.remove_theme_color_override("font_color")
	if _team_label != null:
		_team_label.text = String(texts.get("team", ""))
		_team_label.remove_theme_color_override("font_color")

	var winners: Array = summary.get("winners", [])
	for i in _slots.size():
		var ar := _slots[i]
		var layers: Dictionary = _slot_layers[i] if i < _slot_layers.size() else {}
		var skin_icon: TextureRect = layers.get("skin")
		var face_icon: TextureRect = layers.get("face")
		var hat_icon: TextureRect = layers.get("hat")
		if i < winners.size():
			ar.visible = true
			# ⚠️ 推断：原版运行时给这 10 个格子定多大**没有观测**（`.tscn` 里没有
			# `custom_minimum_size`，而 `stretch_mode = 1` 会让唯一显示的格子撑到行高）。
			# 真帧的**内容**（三层贴图合并 ink）是 395×289 ⇒ 先用"行高的一半"作正方形
			# （1920×1160 下 ≈ 271 px）。
			var row_h := _icons_row.size.y if _icons_row != null else 0.0
			var side := maxf(64.0, row_h * SLOT_ROW_FRACTION)
			ar.custom_minimum_size = Vector2(side, side)
			var w: Dictionary = winners[i]
			# 三层贴图：值是**资产相对路径**（`skins/face_paint_2` 这种），空串 = 该层不画。
			if skin_icon != null:
				skin_icon.texture = _tex(String(w.get("skin", "")))
			if face_icon != null:
				face_icon.texture = _tex(String(w.get("face", "")))
				face_icon.visible = face_icon.texture != null
			if hat_icon != null:
				hat_icon.texture = _tex(String(w.get("hat", "")))
				hat_icon.visible = hat_icon.texture != null
		else:
			ar.visible = false
			ar.custom_minimum_size = Vector2.ZERO
			if skin_icon != null:
				skin_icon.texture = null
			if face_icon != null:
				face_icon.visible = false
			if hat_icon != null:
				hat_icon.visible = false

## 内核里的结算数据桥（`src/_scaffold/endgame_class.rs::GwEndGame`）。
func _endgame_math():
	var app_node := app()
	if app_node != null and app_node.has_method("endgame_math"):
		return app_node.call("endgame_math")
	return null

## 两条横幅文本：字面量在**内核**（`graphwar_play_screen_control::VICTORY_TEXTS`，
## 原版 `.rodata` 6 条，观测），选哪条由 `GwEndGame.label_texts()` 按胜方字节 + 我方队伍算。
## 缺内核 ⇒ 退化成 `.tscn` 的静态原文（`docs/92` §4.13），**不编新文案**。
func _label_texts(app_node, byte: int, human_team: int) -> Dictionary:
	var m = _endgame_math()
	if m != null:
		return m.call("label_texts", byte, human_team)
	var res: Dictionary = app_node.last_result
	var team := int(res.get("winner_team", 0))
	return {
		"self": "YOU WON" if bool(res.get("human_won", false)) else "YOU lOST",
		"team": "Team %d Victory!" % (team + 1) if team >= 0 else "Tie!",
	}

## 资产相对路径 → 真贴图（`GwAssets` 只返回 pck 里解出来的真资源，找不到是 `null`）。
static func _tex(rel: String) -> Texture2D:
	if rel == "":
		return null
	return GwAssets.texture(rel)
