extends RefCounted
class_name GwCampaign
## 战役：21 关（0..20）—— 数据**全部来自 Rust 内核**，本文件不再存表。
##
## ⚠️ 改前（T67 之前）这里有一张 `const LEVELS`（21 条全参数）—— 那是内核
## `src/graphwar_campaign_control.rs::LEVEL_INFO`（= 原版 `level_info()`，vaddr `0x4ad380`）
## 的**第二份拷贝**：改内核不改 `.gd`，界面就显示旧数据，两份只靠一条 `cargo test` 拉着。
## 现在一律问 `GwCampaignControl`（`src/_scaffold/campaign_control_class.rs`）要，
## 本文件只剩两样**不属于内核**的东西（台账 `fixtures/controller_migration3_ledger.tsv` C06/C08）：
##
##   1. `level_seed()` —— **我方自造**的确定性种子（原版由运行时 ChaCha 抽，`docs/102` §1）；
##   2. `layout_buttons()` —— 原版**视图**算法（真值在 `recovered-src/CampaingLevelsButtons.gd`）。
##
## 关卡数据每字段的证据档（`.rodata` 逐字节 / 立即数 / 385 份 replay 运行时三源对照）
## 在 `graphwar_campaign_control.rs` 的 `LEVEL_INFO` 文档里，21/21 逐项可核。

## 关卡数：**问内核**（`LEVEL_INFO` 的数组长度），本文件不写 21。
##
## ⚠️ 用静态函数而不是 `const`：GDScript 的 `const` 要求**常量表达式**，跨语言调
## GDExtension 的静态方法不是常量表达式（实测：`const LEVEL_COUNT := GwCampaignControl
## .level_count()` 会让引用它的脚本在**解析期**报 "Could not resolve external class member"）。
static func level_count() -> int:
	return GwCampaignControl.level_count()

## 8 列 × 4 行（观测公式：`screen_size.x / 8`、`screen_size.y / 4`，
## 见 `docs/101` §4.1 的真源码）。21 个按钮按 8 列折行，16:9 下实际 7+7+7。
const GRID_COLUMNS := 8
const GRID_ROWS := 4


## 关卡生成种子：**我们的自定值**。原版的种子由运行时 ChaCha 抽（`docs/102` §1），
## 哪一关用哪个种子拿不到，所以这里只保证"同一关每次一样"。
static func level_seed(index: int) -> int:
	return (0x9E3779B9 * (clampi(index, 0, level_count() - 1) + 1)) & 0xFFFFFFFF


## 一关的全部字段（**内核给的那一份**）+ 本文件补的 `index` / `seed`。
##
## 返回的键与改前 `LEVELS` 的行**逐字一致**（调用方零改动）：
##   `title / description / axis / function / turn / time`（四分模式）
##   `soldiers / targets / bot_tier / turn_limit / obstacles / obstacle_min`
##
## `bot_tier` = 敌方 Bot 的 `bot_N` 皮肤号，`-1` = 该关没有对手（内核是 `Option<BotOpponent>`）；
## `turn_limit` = 回合上限，`0` = 无上限（内核是 `Option<u32>`）；
## `obstacles` = 内核 `ObstacleMode` 的判别值（`None=0/Center=1/Scatter=2`，
## 名字表问 `GwCampaignControl.obstacle_modes()`）。
static func level(index: int) -> Dictionary:
	var i := clampi(index, 0, level_count() - 1)
	var out: Dictionary = GwCampaignControl.level(i)
	if out.is_empty():
		push_error("[gw2] GwCampaignControl.level(%d) 返回空：内核类没注册上？" % i)
		return {}
	out["index"] = i
	out["seed"] = level_seed(i)
	return out


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
	for _i in level_count():
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
