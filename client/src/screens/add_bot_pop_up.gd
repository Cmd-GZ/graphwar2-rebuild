extends GwScreen
## AddBotPopUp —— 加 Bot（六档难度）。
##
## **节点树来自原版 `.tscn`**：`client/scenes/screens/add_bot_pop_up.tscn` 与
## `recovered-src/add_bot_pop_up.tscn` **逐字节相同**（254 行 / 45 节点）。原版结构：
##
##   MarginContainer/VBoxContainer
##     ├ MarginContainer [v=3]           ← 挂原版 `MarginContainerPopUpBack.gd` → ExitButtonContainer
##     ├ HBoxContainer2 [v=3, ratio 10]  ← 三列 1 : 6 : 1
##     │   └ MarginContainer2 → Panel + VBoxContainer
##     │        ├ MarginContainer(ratio .1)  顶部内留白
##     │        ├ OptionsContainer  ← M(1) AR AR2 M4(.2) AR4 M3(.2) AR4 M5(1) ⇒ 三格 add_bot_0/1/2
##     │        ├ OptionsContainer2 ← 同上                                  ⇒ 三格 add_bot_3/4/5
##     │        └ HBoxContainer3 → M3 · IpPopUpMargin(Cancel) · M4
##     ├ HBoxContainer3 [v=3]            空底带
##     └ AdContainer（运行时 0 高）
##
## 六个格子是 `GraphwarButton`（`input_text = add_bot_N`、`expand_icon = true`、只有图标），
## 图标 = 原版 `skins/bot_0..5.svg`（别名层 `.import` → `assets/skins/bot_N.ctex`）。
## 原版**没有标题**、格子也**没有** `custom_minimum_size`（尺寸由 1:.2 空隙比例 + 行高决定）。
##
## **`bot_N` ↔ 六档难度的对应关系没有观测**（`docs/92` §4.6）⇒ 只按顺序摆放，并在 tooltip 里
## 明写"对应关系未证实"。tooltip 是**我方加的**（原版没有 tooltip），纯提示、不参与像素。
##
## 我方这一层现在只剩补 tooltip；布局/字号全由原版 `.tscn` 与其挂的脚本决定（docs/06 §92）。

func _ready() -> void:
	font_divisor = 40        # `IpPopUpMargin.gd`（Cancel 那一层）
	super()

func build() -> void:
	for b in GwButton.native_buttons(self):
		var cmd := String(b.get("input_text"))
		if not cmd.begins_with(GwCommands.ADD_BOT_PREFIX):
			continue
		var tier := int(cmd.substr(GwCommands.ADD_BOT_PREFIX.length()))
		if tier < 0 or tier >= GwGameState.BOT_TIER_LABELS.size():
			continue
		b.tooltip_text = "%s\nbot_%d 与难度的对应关系**未观测**（docs/92 §4.6），此处只按顺序摆放" % [
			GwGameState.BOT_TIER_LABELS[tier], tier]
