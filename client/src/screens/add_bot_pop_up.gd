extends GwScreen
## AddBotPopUp —— 加 Bot（六档难度）。
##
## 结构（`docs/92` §4.6，恢复出的 add_bot_pop_up.tscn 45 节点）：
##   MarginContainer/VBoxContainer
##     ├ MarginContainer/HBoxContainer/ExitButtonContainer（input_text="cancel_add_bot"）
##     ├ HBoxContainer2/MarginContainer2/Panel/VBoxContainer
##     │    ├ OptionsContainer/AspectRatioContainer{1,2,4}/AddBotButton  (add_bot_0/1/2)
##     │    ├ OptionsContainer2/AspectRatioContainer{1,2,4}/AddBotButton (add_bot_3/4/5)
##     │    └ HBoxContainer3/…/GraphwarButton(" Cancel ", cancel_add_bot)
##     └ AdContainer(0,60)
##
## 注意原版命名不整齐：`OptionsContainer` 里只有 `AspectRatioContainer` /
## `AspectRatioContainer2` / `AspectRatioContainer4`（没有 3），照抄。
##
## 图标：原版 `skins/bot_0..5.svg`；本项目现画 `bot_0..5`。
## **`bot_N` ↔ 六档难度的对应关系没有观测**（`docs/92` §4.6 明确写了），
## 所以 tooltip 里只用档位序号 + "对应关系未证实"。

func _ready() -> void:
	font_divisor = 30
	super()

func build() -> void:
	var d := GwUI.dialog(self, Vector2(680, 360))
	var content: VBoxContainer = d.content

	var top := GwUI.hbox("HBoxContainer", 8, content)
	top.add_child(GwUI.exit_pair(GwCommands.CANCEL_ADD_BOT))
	var title := GwUI.label("ADD BOT", 0, HORIZONTAL_ALIGNMENT_CENTER)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top.add_child(title)
	var pad := GwUI.spacer(Vector2(64, 0))
	top.add_child(pad)

	var row1 := GwUI.hbox("OptionsContainer", 10, content)
	for tier in [0, 1, 2]:
		row1.add_child(_tier_button(tier))
	var row2 := GwUI.hbox("OptionsContainer2", 10, content)
	for tier in [3, 4, 5]:
		row2.add_child(_tier_button(tier))

	var actions := GwUI.hbox("HBoxContainer3", 8, content)
	var spacer := GwUI.spacer(Vector2(0, 0))
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_child(spacer)
	var cancel := GwButton.make(" Cancel ", GwCommands.CANCEL_ADD_BOT)
	cancel.custom_minimum_size = Vector2(120, 42)
	actions.add_child(cancel)

	(d.vbox as VBoxContainer).add_child(GwUI.ad_bar())

func _tier_button(tier: int) -> Control:
	var holder := AspectRatioContainer.new()
	holder.name = "AspectRatioContainer" if tier == 0 else "AspectRatioContainer%d" % tier
	holder.ratio = 1.0
	holder.stretch_mode = AspectRatioContainer.STRETCH_FIT
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	holder.custom_minimum_size = Vector2(120, 120)
	var b := GwButton.make("")
	b.name = "AddBotButton"
	b.input_text = "%s%d" % [GwCommands.ADD_BOT_PREFIX, tier]
	b.icon = GwIcons.get_icon("bot_%d" % tier)
	b.expand_icon = true
	b.custom_minimum_size = Vector2(96, 96)
	b.tooltip_text = "%s\n名字与难度的对应关系**未观测**（docs/92 §4.6），此处只按档位排序" % GwGameState.BOT_TIER_LABELS[tier]
	holder.add_child(b)
	return holder
