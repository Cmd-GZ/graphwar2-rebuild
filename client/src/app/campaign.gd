extends RefCounted
class_name GwCampaign
## 战役：21 关（0..20）。
##
## 界面侧**已证实**的部分（`docs/92` §4.2、`docs/101` §4.1、`docs/104` §5.3）：
##   * 21 个按钮，运行时由 `CampaingLevelsButtons.gd` 用 `GraphwarButton.new()` 造出来；
##   * 命名 `level_0` .. `level_20`，`text` 是 `str(i)`，`input_text` 是 `"level_" + str(i)`；
##   * 布局：8 列 × 4 行，字号 = 容器宽/15，x 到 `9/10` 或 `1/10` 处回弹（见 `layout_buttons`）；
##   * `JoinLevelPopUp` 里显示 `Level N` 标题 + 只读说明 + 四个模式图标 + Cancel/Start。
##
## **未知**（必须明写，不要猜）：
##   * 关卡**数据**本身：每关的障碍布局、玩家/Bot 配置、目标函数、通关条件——全都没有观测。
##     `docs/103` 证明只有两种关卡生成模式（disk / 磁盘式），且生成公式已闭合
##     （`src/levelgen.rs`，13/13 逐位复现），但**哪一关用哪个种子**拿不到；
##   * 锁定判据：本机存档下 21 关全可点（`docs/92` §8），锁定规则未知。
##
## 所以这里给的是**跑得通、说得清**的占位：关卡号 + 一个确定性种子 + 全默认选项，
## 解锁规则我们用"上一关通关"（自定规则，别人家的原版规则未知）。

const LEVEL_COUNT := 21
const PLACEHOLDER := true

## 8 列 × 4 行（观测公式：`screen_size.x / 8`、`screen_size.y / 4`，
## 见 `docs/101` §4.1 的真源码）。21 个按钮按 8 列折行，正好 3 行满 + 1 行 5 个。
const GRID_COLUMNS := 8
const GRID_ROWS := 4


static func level(index: int) -> Dictionary:
	var i := clampi(index, 0, LEVEL_COUNT - 1)
	return {
		"index": i,
		"title": "Level %d" % i,
		"description": _placeholder_description(i),
		"axis": GwModes.AXIS_DEFAULT,
		"function": GwModes.FUNCTION_DEFAULT,
		"turn": GwModes.TURN_DEFAULT,
		"time": GwModes.TIME_DEFAULT,
		# 关卡生成种子：占位。真实的"第 N 关用哪个种子"未知（docs/108 §7）。
		"seed": 0x9E3779B9 * (i + 1) & 0xFFFFFFFF,
		"placeholder": PLACEHOLDER,
	}


static func _placeholder_description(i: int) -> String:
	var lines := [
		"Level %d" % i,
		"",
		"PLACEHOLDER — the real campaign data for this level is unknown.",
		"",
		"What we do know (docs/92 §4.2, docs/103): the campaign has exactly 21 levels,",
		"numbered 0..20, and the level generator has exactly two modes. Neither the",
		"obstacle layout nor the win condition of any individual level was ever observed,",
		"so this screen shows defaults instead of inventing a level.",
		"",
		"What works in this rebuild: pressing Start opens the PlayScreen with this",
		"level's default options, and clearing the board unlocks level %d." % (i + 1),
	]
	return "\n".join(lines)


## 解锁规则不再放在这里：现在读**原版的** `user://campaign.save`
## （`GwStore.is_level_unlocked()`），本文件只保留关卡数据与布局算法。


## 复刻 `CampaingLevelsButtons.gd::place_buttons` 的**布局算法**。
##
## 每个数值都来自恢复出的真源码（`docs/101` §4.1 的伪源码与 GDRE 源码 token 级一致）：
##   字号 = size.x / 15；x 步长 = size.x / 8；y 步长 = size.y / 4；
##   起点 (x_step, y_step/2)；每步 `pos.x += x_step`，**只有**当 x 越过 `9/10` 或 `1/10`
##   才反向并 `pos.y += y_step`（换行）。
##
## ⚠️ 与 `docs/101` §4.1 措辞的差异（值得记一笔）：那里把网格概括成"8 列 4 行"，
## 但真码的 y 步进**在回弹分支里**，所以 16:9 屏上实际是 **7 个一行 × 3 行 = 21**，
## x 从 w/8 走到 7w/8。本项目按真码实现，`tools/selftest.gd` 断言的是"3 行"。
## 返回每个按钮的**锚点**（`anchor`，真码里的 `pos`）、只按宽度居中过的 `position`
## （兼容旧调用方/自检）、应设的 `size_x` 与所在行的 y（`row_y`）。
##
## ⚠️ 高度：真码**只写 `button.size.x`**，高度保持控件最小尺寸（= 字号决定的行高），
## 然后 `position = pos - button.size / 2` —— 所以竖直方向是按**真实高度**居中的。
## 调用方（`screens/campaign.gd::place_buttons`）必须先量出最小高度再用 `anchor` 算位置；
## `position` 只是"高度未知时"的近似（高度 0），自检只用它来数行数。
static func layout_buttons(area: Vector2) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	var font_size := area.x / 15.0
	var pos_x_increment := area.x / 8.0
	var pos_y_increment := area.y / 4.0
	var pos := Vector2(pos_x_increment, pos_y_increment / 2.0)
	for _i in LEVEL_COUNT:
		var w: float = absf(pos_x_increment)
		out.append({
			"font_size": font_size,
			"anchor": pos,
			"position": pos - Vector2(w, 0.0) / 2.0,
			"size_x": w,
			"row_y": pos.y,
		})
		pos.x += pos_x_increment
		if pos.x > area.x * 9.0 / 10.0 or pos.x < area.x / 10.0:
			pos_x_increment = -pos_x_increment
			pos.x += pos_x_increment
			pos.y += pos_y_increment
	return out
