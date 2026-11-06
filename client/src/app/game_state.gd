extends RefCounted
class_name GwGameState
## 一局对战的界面侧状态（**不是**规则引擎）。
##
## 规则引擎在 Rust 内核 `src/` 里：开火/采样 `src/graphwar_path_calculator.rs`、修正 `src/graphwar_shot_calculator.rs`、
## 杀伤 `src/graphwar_life_manager.rs`、回合与结算 `src/graphwar_turn_manager.rs`/`src/graphwar_shot_calculator/settle.rs`、关卡生成 `src/graphwar_level_generator.rs`。
## 本类只保存"界面上看得见的选择"，并通过 `core_bridge.gd` 把要算的东西递过去。

enum Kind { LAN, CAMPAIGN, REPLAY }

## 单人最多 5 个士兵（观测：`team1_entry.tscn` 里 `SoldierIconContainer` 有 5 个槽，
## 见 `docs/92` §4.5）。
const MAX_SOLDIERS := 5
const MAX_PLAYERS_PER_TEAM := 8

## 每档 Bot 的名字。**这是占位**：`docs/92` §4.5 观测到名字池与难度不是 1:1
## （同一档两次加进来可能叫不同名字），且 `bot_N` 图标 ↔ 六档难度的对应关系是
## 推断（`docs/92` §4.6 明确写了"没有直接观测"）。这里取观测到的名字池固定分配，
## 位置对应关系未证实。
const BOT_NAMES := ["ChatGPT", "Wolfram", "Alpha Zordon", "Multivac", "Deep Blue", "Deep Thought"]
## 六档难度（`AI.md` §1 的分档；界面侧只当索引）。
const BOT_TIER_LABELS := ["tier 0 (weakest)", "tier 1", "tier 2", "tier 3", "tier 4", "tier 5 (strongest)"]

var kind: Kind = Kind.LAN
var room_name := "Graphwar Game"
var locked := false

## 四个选项（见 `modes.gd`）。
var axis_mode := GwModes.AXIS_DEFAULT
var function_mode := GwModes.FUNCTION_DEFAULT
var turn_mode := GwModes.TURN_DEFAULT
var time_mode := GwModes.TIME_DEFAULT

## 玩家表。每项：{name, team(0/1), is_bot, bot_tier, soldiers, skin, face, hat}
var players: Array[Dictionary] = []

## 战役专属
var level_index := -1
## 回放专属
var replay_file := ""

func reset_lan() -> void:
	kind = Kind.LAN
	players.clear()
	level_index = -1
	replay_file = ""

func reset_campaign(index: int) -> void:
	kind = Kind.CAMPAIGN
	players.clear()
	level_index = index

func reset_replay(file: String) -> void:
	kind = Kind.REPLAY
	players.clear()
	replay_file = file

func add_human(player_name: String, skin: int, face: int, hat: int) -> void:
	players.append({
		"name": player_name,
		"team": 0,
		"is_bot": false,
		"bot_tier": -1,
		"soldiers": 1,
		"skin": skin,
		"face": face,
		"hat": hat,
	})

func add_bot(tier: int) -> void:
	# 分队规则（**本项目自定**，原版未观测）：第 1 个 Bot 一定去对面（team 1），
	# 之后按人数奇偶交替。没有这条，1 人 + 1 Bot 会分到同一边，永远打不完。
	var team := players.size() % 2
	players.append({
		"name": BOT_NAMES[clampi(tier, 0, BOT_NAMES.size() - 1)],
		"team": team,
		"is_bot": true,
		"bot_tier": clampi(tier, 0, 5),
		"soldiers": 1,
		"skin": clampi(tier, 0, 5),
		"face": -1,
		"hat": -1,
	})

func remove_player(index: int) -> void:
	if index >= 0 and index < players.size():
		players.remove_at(index)

func bot_count() -> int:
	var n := 0
	for p in players:
		if p.is_bot:
			n += 1
	return n

func human_count() -> int:
	return players.size() - bot_count()

func total_soldiers(team: int) -> int:
	var n := 0
	for p in players:
		if p.team == team:
			n += p.soldiers
	return n

## `docs/22` §5.1 观测：不加 Bot 直接 Start 会静默无操作。本项目照抄这个行为
## （`App.command("start_game")` 里判断），并在状态栏给一句提示——
## 原版是纯静默，我们给提示属于**有意偏离**，写在 docs/108 §7。
func can_start() -> bool:
	if players.is_empty():
		return false
	if bot_count() == 0 and human_count() < 2:
		return false
	return true
