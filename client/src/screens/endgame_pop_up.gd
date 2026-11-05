extends GwScreen
## EndGamePopUp —— 结算。
##
## 结构（`docs/92` §4.13，恢复出的 endgame_pop_up.tscn 69 节点）：
##   Panel/VBoxContainer
##     ├ SelfVictoryLabel（原版文本 **"YOU lOST"** —— 小写 l，是原版原文，照抄）
##     │      theme_override_font_sizes/font_size = 70（全项目最大的两条之一）
##     ├ HBoxContainer/VictoryPlayerContainer1..10
##     │      每槽 3 个 TextureRect：SkinIcon / FaceIcon / HatIcon（最多展示 10 个士兵）
##     ├ TeamVictoryLabel（"Team 2 Victory!"）  font_size = 40
##     └ ExitButtonContainer（input_text="exit_play"）
##
## 只有真的有人全灭才会出现（`docs/92` §4.13：档 5 的 Bot 31.7 s 后自然出现；
## 等时间不会出现）。本项目由 `App.report_match_result()` 触发。

const VICTORY_SLOTS := 10

var _self_label: Label
var _team_label: Label
var _slots: Array[MarginContainer] = []

func _ready() -> void:
	font_divisor = 30
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(760, 320))
	var content: VBoxContainer = d.content

	_self_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_self_label.name = "SelfVictoryLabel"
	_self_label.add_theme_font_size_override("font_size", 70)   # 静态场景里的 override 值
	content.add_child(_self_label)

	var slots_row := GwUI.hbox("HBoxContainer", 6, content)
	for i in VICTORY_SLOTS:
		var holder := AspectRatioContainer.new()
		holder.name = "VictoryPlayerContainer%d" % (i + 1)
		holder.ratio = 1.0
		holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
		holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		holder.custom_minimum_size = Vector2(48, 48)
		var m := GwUI.margin("MarginContainer", 2, holder)
		var skin_icon := TextureRect.new()
		skin_icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		skin_icon.name = "SkinIcon"
		skin_icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		skin_icon.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		m.add_child(skin_icon)
		var face_icon := TextureRect.new()
		face_icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		face_icon.name = "FaceIcon"
		face_icon.visible = false
		m.add_child(face_icon)
		var hat_icon := TextureRect.new()
		hat_icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		hat_icon.name = "HatIcon"
		hat_icon.visible = false
		m.add_child(hat_icon)
		slots_row.add_child(holder)
		_slots.append(m)

	_team_label = GwUI.label("", 0, HORIZONTAL_ALIGNMENT_CENTER)
	_team_label.name = "TeamVictoryLabel"
	_team_label.add_theme_font_size_override("font_size", 40)
	content.add_child(_team_label)

	var actions := GwUI.hbox("HBoxContainer", 8, content)
	actions.add_child(GwUI.exit_pair(GwCommands.EXIT_PLAY))
	var spacer := GwUI.spacer(Vector2(0, 0))
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(spacer)

func on_enter() -> void:
	var app_node := app()
	if app_node == null:
		return
	var summary: Dictionary = app_node.match_summary()
	var human_won := bool(summary.get("human_won", false))
	var winner_team := int(summary.get("winner_team", 0))
	var players: Array = summary.players

	# 原版只观测到失败一侧的文案 "YOU lOST"（小写 l，docs/92 §4.13）。
	# 胜利一侧的原文**没有观测**，所以赢的时候用一句明确标注的说明，不编造原文。
	if human_won:
		_self_label.text = "YOU WON"
		_self_label.add_theme_color_override("font_color", GwThemeFactory.COL_ACCENT)
		_self_label.tooltip_text = ("胜利一侧的原文未观测（只观测到 \"YOU lOST\"，docs/92 §4.13）；"
			+ "这里用英文占位并明确标注")
	else:
		_self_label.text = "YOU lOST"    # 原版原文，含它的小写 l
		_self_label.add_theme_color_override("font_color", GwThemeFactory.COL_BAD)
		_self_label.tooltip_text = "原版原文如此（docs/92 §4.13：小写 l）"

	_team_label.text = "Team %d Victory!" % (winner_team + 1)

	var winners: Array = []
	for p in players:
		if int(p.team) == winner_team:
			for i in int(p.soldiers):
				winners.append(p)
	for i in _slots.size():
		var m := _slots[i]
		var skin_icon := m.get_node_or_null("SkinIcon") as TextureRect
		var face_icon := m.get_node_or_null("FaceIcon") as TextureRect
		var hat_icon := m.get_node_or_null("HatIcon") as TextureRect
		if i < winners.size():
			var p: Dictionary = winners[i]
			var skin_idx := int(p.skin)
			if skin_idx >= 0:
				skin_icon.texture = GwIcons.get_icon("skin_%d" % skin_idx)
			else:
				skin_icon.texture = GwIcons.get_icon("bot_%d" % int(p.bot_tier))
			# 观测：Bot 的脸/帽子分别是 faces/no_face.svg、hats/no_hat.svg（这里留空 = 无脸/无帽）
			face_icon.texture = GwIcons.get_icon("face_%d" % int(p.face)) if int(p.face) >= 0 else null
			hat_icon.texture = GwIcons.get_icon("hat_%d" % int(p.hat)) if int(p.hat) >= 0 else null
			face_icon.visible = int(p.face) >= 0
			hat_icon.visible = int(p.hat) >= 0
		else:
			skin_icon.texture = null
			face_icon.visible = false
			hat_icon.visible = false
