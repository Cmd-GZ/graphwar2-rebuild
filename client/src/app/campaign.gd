extends RefCounted
class_name GwCampaign
## 战役：21 关（0..20）——**全部来自二进制的真数据**。
##
## ⚠️ 这张表是**内核同名表的客户端副本**：权威定义在
## `src/graphwar_campaign_control.rs::LEVEL_INFO`（= 原版 `level_info()`，`0x4ad380`）。
## 两份由 `cargo test --test campaign_data` 的
## `client_campaign_table_matches_the_kernel_table` **逐字节**钉住（标题 21、描述 21、
## 模式 4 项、士兵/目标/Bot 档/回合上限/障碍模式与 `min_count` 全部逐项比对）——
## 改这里而不改内核，测试会立刻红。21 关数据已全部落地（`docs/118` §7），无占位项。
##
## 数据来源（逐条可核对，详见 `docs/10-campaign.md` §16 与 `docs/archive/118-campaign-data.md`）：
##
## | 字段 | 来源 | 证据等级 |
## |---|---|---|
## | `title` | `docs/118` §4.1：21/21 都是 `Level 0`..`Level 20`（`.rodata` 两种编码） | 逐位 |
## | `description` | `docs/118` §4.2：`.rodata` 逐字节 21 段（含原版的拼写错误 `writting`） | 逐位 |
## | `axis/function/turn/time` | `docs/118` §4.3：每关 `mov dword [rsp+0x58], imm32`，4 字节按 `+0x38..+0x3b` 拆分 | 逐位 |
## | `soldiers / targets / turn_limit / obstacles / obstacle_min` | `docs/118` §7.1 全参数表（`S`=立即数、`C`=闭包预设、`R`=385 份 replay 运行时真值） | 逐位 + 运行时 |
## | `bot_tier` | `docs/118` §7.1 的 `R` 列 = 运行时 `bot_N` **皮肤号**（0..4） | 逐位 + 运行时 |
##
## `time` 恒为 5 ⇒ `GwModes.TimeLimit.INF`（∞）。这**同时解释**了三件事：
## `join_level_pop_up` 第 4 个按钮用 `turn_inf.svg`、战役对局里 `TimerLabel` 一片空白
## （观测：`orig_campaign_play.png` 左下角无任何字符）、以及 `docs/118` §4.3 的
## `byte3 = 0x05` 21/21 恒定。
##
## 仍属我们的自定项（原版未观测）：`seed`（每关生成器的随机种子由原版在运行时
## 用 ChaCha 抽，见 `docs/102` §1；这里取确定性值，只为"同一关每次一样"）。

const LEVEL_COUNT := 21

## 8 列 × 4 行（观测公式：`screen_size.x / 8`、`screen_size.y / 4`，
## 见 `docs/101` §4.1 的真源码）。21 个按钮按 8 列折行，16:9 下实际 7+7+7。
const GRID_COLUMNS := 8
const GRID_ROWS := 4

## 障碍生成模式（`docs/118` §7.2 的公式，常量地址可指认）。
enum Obstacles { NONE, CENTER, SCATTER }

## 21 关全表。顺序 = `level_0`..`level_20`。
##
## 每项：
##   `axis`     0=NoAxis 1=OnlyMain 2=EveryFive 3=EveryUnit（§7.1 脚注，21/21 与描述同向）
##   `function` 0=f(x) 1=微分方程
##   `turn`     0=sequential 1=simultaneous
##   `time`     恒 5 = INF（= `turn_inf.svg`）
##   `soldiers` 我方士兵数；`targets` 靶子数；`bot_tier` 敌方 Bot 的 `bot_N` 皮肤号（-1 = 无对手）
##   `turn_limit` 回合上限，0 = 无上限
##   `obstacles`/`obstacle_min` 障碍模式与散布式的个数下界 `min_count`
##              （非散布式一律 0：`CENTER` 的"恰好 1 颗"由生成本体保证，
##               与 `graphwar_level_generator::ObstacleParams::min_count` 的约定一致）
##
## ⚠️ 这里**没有** Bot 的"难度值"（3/25/50/100/250）：那是 `bot_added_event` 载荷里给
## 电脑玩家管理器用的立即数，只有内核需要（`graphwar_campaign_control::BotOpponent::difficulty`）。
## 界面只用到皮肤号，所以客户端不多存一份。
const LEVELS := [
	{"title": "Level 0", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 0, "obstacles": Obstacles.NONE, "obstacle_min": 0,
		"description": "Write a function of x to hit the target."},
	{"title": "Level 1", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 5, "obstacles": Obstacles.NONE, "obstacle_min": 0,
		"description": "A constant is always added to your function. You can see it on the bottom right corner of the screen. It makes sure the function is shot by your soldier."},
	{"title": "Level 2", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 5, "obstacles": Obstacles.CENTER, "obstacle_min": 0,
		"description": "There are obstacles in the cartesian plane that don't let your function go through. Try to make a function that goes around them."},
	{"title": "Level 3", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 2, "targets": 3, "bot_tier": -1, "turn_limit": 10, "obstacles": Obstacles.CENTER, "obstacle_min": 0,
		"description": "You now have multiple soldiers in your team. At each turn you will shoot from a different one."},
	{"title": "Level 4", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 10, "obstacles": Obstacles.SCATTER, "obstacle_min": 15,
		"description": "Try going around a plane with more obstacles."},
	{"title": "Level 5", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 2, "targets": 0, "bot_tier": 0, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 5,
		"description": "Play against an opponent that shoots back."},
	{"title": "Level 6", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 2, "bot_tier": -1, "turn_limit": 6, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "You must hit these targets in a single shot to destroy them."},
	{"title": "Level 7", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 1, "obstacles": Obstacles.SCATTER, "obstacle_min": 6,
		"description": "Destroy your target in your first try."},
	{"title": "Level 8", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 3, "bot_tier": -1, "turn_limit": 6, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "Hit all targets in a single shot to destroy them."},
	{"title": "Level 9", "axis": 3, "function": 0, "turn": 0, "time": 5, "soldiers": 3, "targets": 0, "bot_tier": 2, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 5,
		"description": "Play against a smarter opponent."},
	{"title": "Level 10", "axis": 2, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 2, "bot_tier": -1, "turn_limit": 6, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "Hit all targets in a single shot to destroy them. The finer grid lines are gone."},
	{"title": "Level 11", "axis": 2, "function": 0, "turn": 1, "time": 5, "soldiers": 3, "targets": 0, "bot_tier": 3, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 5,
		"description": "Play against an opponent on a plane with fewer grid lines. You and the opponent are also firing simultaneously."},
	{"title": "Level 12", "axis": 1, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 5, "obstacles": Obstacles.CENTER, "obstacle_min": 0,
		"description": "Most grid lines are gone. Try hitting a target."},
	{"title": "Level 13", "axis": 1, "function": 0, "turn": 0, "time": 5, "soldiers": 1, "targets": 2, "bot_tier": -1, "turn_limit": 5, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "Destroy two targets in a single shot."},
	{"title": "Level 14", "axis": 1, "function": 0, "turn": 1, "time": 5, "soldiers": 4, "targets": 0, "bot_tier": 3, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 5,
		"description": "Play against an opponent with almost no grids."},
	{"title": "Level 15", "axis": 0, "function": 0, "turn": 0, "time": 5, "soldiers": 5, "targets": 0, "bot_tier": 4, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "The main axes are gone. The opponent is smarter than ever."},
	{"title": "Level 16", "axis": 3, "function": 1, "turn": 0, "time": 5, "soldiers": 1, "targets": 1, "bot_tier": -1, "turn_limit": 0, "obstacles": Obstacles.NONE, "obstacle_min": 0,
		"description": "You are now writting a differential equation instead of a function. Write a differential equation in terms of x and y. Your shot will be the solution to the differential equation using the position of the soldier that shot it as the initial condition."},
	{"title": "Level 17", "axis": 3, "function": 1, "turn": 0, "time": 5, "soldiers": 1, "targets": 2, "bot_tier": -1, "turn_limit": 6, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "You must hit both targets in a single shot with a differential equation to destroy them."},
	{"title": "Level 18", "axis": 3, "function": 1, "turn": 0, "time": 5, "soldiers": 3, "targets": 0, "bot_tier": 3, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "Fight an opponent with differential equations."},
	{"title": "Level 19", "axis": 1, "function": 1, "turn": 0, "time": 5, "soldiers": 4, "targets": 0, "bot_tier": 3, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 10,
		"description": "Fight an opponent with differential equations with less grid lines."},
	{"title": "Level 20", "axis": 0, "function": 1, "turn": 0, "time": 5, "soldiers": 5, "targets": 0, "bot_tier": 4, "turn_limit": 0, "obstacles": Obstacles.SCATTER, "obstacle_min": 15,
		"description": "Fight an opponent with differential equations and no grid lines."},
]

## 关卡生成种子：**我们的自定值**。原版的种子由运行时 ChaCha 抽（`docs/102` §1），
## 哪一关用哪个种子拿不到，所以这里只保证"同一关每次一样"。
static func level_seed(index: int) -> int:
	return (0x9E3779B9 * (clampi(index, 0, LEVEL_COUNT - 1) + 1)) & 0xFFFFFFFF


static func level(index: int) -> Dictionary:
	var i := clampi(index, 0, LEVEL_COUNT - 1)
	var row: Dictionary = LEVELS[i]
	var out := row.duplicate(true)
	out["index"] = i
	out["seed"] = level_seed(i)
	return out


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
