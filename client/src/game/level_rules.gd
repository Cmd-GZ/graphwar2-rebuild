extends RefCounted
class_name GwLevelRules
## **关卡胜负规则**：按关卡类型分别判定，而不是"两队存活数都 > 0 就继续"（`docs/131`）。
##
## 之前的问题（`docs/131` §1）：`play_screen.gd::_check_match_over()` 只数两队**士兵**的存活数。
## 打靶关（`targets > 0`、没有对手）里 `Team2` **一个士兵都没有** ⇒ `alive[1] == 0`
## ⇒ 第一发一落地就判"人类胜"。所以现象是"发射一次就通关"——**射击本身跟胜负无关**，
## 是存活统计把打靶关误判了。
##
## 真值依据：
##
## | 规则 | 出处 | 等级 |
## |---|---|---|
## | 对战关胜利 = **对方队伍全灭** | `docs/84` §1 / `docs/123` §4.5：45/45 份回放都是 `Team2` 全灭 ⇒ `EndingGame` | 观测（45/45） |
## | 打靶关的靶子全在 `Team2` | `docs/123` §2.1：`0x46f310` 里发 `TeamInfo{Team2}` | 逐位验证 |
## | 每关**目标数 / 回合上限 / 有没有 Bot** | `docs/118` §7.1（`S`+`C`+`R` 三源对照，21/21） | 逐位 + 运行时 |
## | `Level 7` 的回合上限 = **1**（"in your first try"） | `docs/118` §7.1；`docs/123` §5.1 #2 | 逐位 + 运行时 |
## | 回合上限用尽 ⇒ **失败** | `docs/118` §7.1 有 `turn_limit` 这一列；**用尽后的判决没有样本**（`docs/123` §5.3 #1） | **推断** |
##
## ⇒ "靶子全部摧毁"与"`Team2` 全灭"在原版里是**同一件事**（靶子就是 Team2 的全部实体），
## 这里把它写成显式的"目标全部达成"，因为我们的客户端没有给靶子分配队伍。

## 关卡类型（`docs/118` §7.1 的两列：目标数 / Bot 数）。
enum Kind {
	VERSUS,   ## 对战关：`bot > 0`、`targets == 0`（8 关：5/9/11/14/15/18/19/20）
	TARGETS,  ## 打靶关：`targets > 0`、`bot == 0`（13 关）
	BOTH,     ## 两者都有：**21 关数据里不存在**（`docs/118` §7.1），但规则要定义
}

const KIND_NAMES := {
	Kind.VERSUS: "versus",
	Kind.TARGETS: "targets",
	Kind.BOTH: "both",
}

## 回合上限兜底值：`docs/118` §7.1 的 `0` = **无上限**（原版此时压根不发 `TurnLimitInfo`，
## `docs/123` §5.1 #4 观测）。
const NO_TURN_LIMIT := 0

var kind := Kind.VERSUS
## 人类玩家所在队伍（`app.gd::match_summary()` 用的是同一条规则：第一个非 Bot 的玩家）。
var human_team := 0
## 回合上限；0 = 无上限。
var turn_limit := NO_TURN_LIMIT
## 关卡号（-1 = 非战役对局，只为状态行/文档可追溯）。
var level_index := -1
## 关卡表里写的目标数（只做一致性自检，判定用实际靶子集合）。
var targets_planned := 0

## 战役关：类型由 `docs/118` §7.1 的两列决定。
static func for_campaign(level: Dictionary, human_team_: int) -> GwLevelRules:
	var r := GwLevelRules.new()
	r.level_index = int(level.get("index", -1))
	r.human_team = human_team_
	r.targets_planned = int(level.get("targets", 0))
	r.turn_limit = int(level.get("turn_limit", NO_TURN_LIMIT))
	var has_targets := r.targets_planned > 0
	var has_bot := int(level.get("bot_tier", -1)) >= 0
	if has_targets and has_bot:
		r.kind = Kind.BOTH
	elif has_targets:
		r.kind = Kind.TARGETS
	else:
		r.kind = Kind.VERSUS
	return r

## 非战役对局（LAN / 回放）：没有靶子、没有回合上限，只有"对方全灭"这一条。
## 若调用方另有靶子（将来的自定义关/服务端下发），用 [`for_match`]。
static func for_versus(human_team_: int) -> GwLevelRules:
	var r := GwLevelRules.new()
	r.kind = Kind.VERSUS
	r.human_team = human_team_
	return r

## 通用构造：类型由"有没有靶子 / 有没有对手"两件事决定。
static func for_match(has_targets: bool, has_opponent: bool, turn_limit_: int,
		human_team_: int) -> GwLevelRules:
	var r := GwLevelRules.new()
	r.human_team = human_team_
	r.turn_limit = turn_limit_
	if has_targets and has_opponent:
		r.kind = Kind.BOTH
	elif has_targets:
		r.kind = Kind.TARGETS
	else:
		r.kind = Kind.VERSUS
	return r

func kind_name() -> String:
	return String(KIND_NAMES.get(kind, "?"))

func has_targets() -> bool:
	return kind == Kind.TARGETS or kind == Kind.BOTH

func has_opponent_rule() -> bool:
	return kind == Kind.VERSUS or kind == Kind.BOTH

## 判一局的胜负。
##
## `state` 的键（都可省，缺省按"0 / 空"处理）：
##   * `alive_by_team`：`{team: 存活士兵数}`；
##   * `teams_present`：**开局时**有士兵的队伍（不是"当前还活着的队伍"——否则一队全灭后
##     它就从这个表里消失，`enemy_present` 会永远为假）；
##   * `targets_total` / `targets_alive`：靶子集合的规模与存活数；
##   * `turns_used`：已经结算过的回合数（`docs/118` §7.1 的 `turn_limit` 与它比较）。
##
## 返回 `{over: bool, human_won: bool, winner_team: int, reason: String}`。
## `reason` 的取值（也是状态行/自检里断言的东西）：
##   `playing` / `own_wiped` / `targets_cleared` / `enemy_wiped` / `mutual_wipe` /
##   `turn_limit` / `no_verdict`。
func evaluate(state: Dictionary) -> Dictionary:
	var alive: Dictionary = state.get("alive_by_team", {})
	var teams: Array = state.get("teams_present", [])
	var targets_total := int(state.get("targets_total", 0))
	var targets_alive := int(state.get("targets_alive", 0))
	var turns_used := int(state.get("turns_used", 0))

	var my_alive := int(alive.get(human_team, 0))
	var enemy_present := false
	var enemy_alive := 0
	for t in teams:
		var team := int(t)
		if team == human_team:
			continue
		enemy_present = true
		enemy_alive += int(alive.get(team, 0))

	# ① 失败优先：我方全灭。**只有存在对手时才有意义** —— 打靶关没有敌人，
	#    "我方全灭"不是关卡条件（靶子不会还手）。
	if has_opponent_rule() and enemy_present and my_alive == 0:
		return _verdict(true, false, _other_team(teams), "own_wiped")

	# ② 打靶：全部靶子被摧毁 ⇒ 通关。
	#    与"Team2 全灭"等价（`docs/123` §2.1/§4.5：靶子就生在 Team2，45/45 回放同向）。
	if has_targets() and targets_total > 0 and targets_alive == 0:
		return _verdict(true, true, human_team, "targets_cleared")

	# ③ 对战：对方队伍全灭 ⇒ 通关（`docs/84` §1，45/45 实测）。
	if has_opponent_rule() and enemy_present and enemy_alive == 0:
		if my_alive == 0:
			# 同一发把两边都清空：真值里没有样本（`docs/123` §5.3 #5 同类问题）。
			# 这里判人类失败，并把这条明确登记成"未观测"。
			return _verdict(true, false, _other_team(teams), "mutual_wipe")
		return _verdict(true, true, human_team, "enemy_wiped")

	# ④ 回合上限用尽、靶子还没清完 ⇒ 失败。
	#    `docs/118` §7.1 的 `turn_limit` 是真的（`Level 7` = 1，"in your first try"）；
	#    **用尽之后原版怎么判没有样本**（`docs/123` §5.3 #1），这里取"判负"并标成推断。
	if has_targets() and targets_total > 0 and turn_limit > NO_TURN_LIMIT \
			and turns_used >= turn_limit:
		return _verdict(true, false, _other_team(teams), "turn_limit")

	# ⑤ 什么都没触发。若这局根本没有判定所需的双方（例如回放模式没有玩家表），
	#    明确说"判不了"而不是像旧代码那样"两队都是 0 ⇒ 有人赢"。
	if not enemy_present and not has_targets():
		return _verdict(false, false, human_team, "no_verdict")
	return _verdict(false, false, human_team, "playing")

func _other_team(teams: Array) -> int:
	for t in teams:
		if int(t) != human_team:
			return int(t)
	# 打靶关没有别的玩家队 —— 靶子在 **Team2**（`docs/123` §2.1），
	# 所以"关卡失败"的胜方就是 1（= Team2）。这条让结算框的文案与原版语义一致。
	return 1 - human_team

func _verdict(over: bool, human_won: bool, winner_team: int, reason: String) -> Dictionary:
	return {
		"over": over,
		"human_won": human_won,
		"winner_team": winner_team,
		"reason": reason,
	}

## 诊断快照（状态行与自检用）。
func snapshot() -> Dictionary:
	return {
		"kind": kind_name(),
		"human_team": human_team,
		"turn_limit": turn_limit,
		"level_index": level_index,
		"targets_planned": targets_planned,
	}
