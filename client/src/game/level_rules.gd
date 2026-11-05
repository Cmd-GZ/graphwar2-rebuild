extends RefCounted
class_name GwLevelRules
## **关卡胜负规则：一条判据。**（`docs/150-win-rule.md`）
##
## > **某队没有存活实体 ⇒ 这局结束；还有活实体的那队赢。**
##
## 这就是原版的全部胜负逻辑。`docs/150` §Q2 把判定点钉在两条指令上：
##
## ```text
## 0x473520(self) -> (Team1 有活实体, Team2 有活实体)
##     遍历生命表的每个 id：0x454950(判活) → 0x3c2120(查队伍) → 置位
## 0x4759f0（回合末）
##     0x475a64/0x475a77  回合上限用尽 ⇒ END
##     0x475a80  al == 0            ⇒ END   ; Team1 没人
##     0x475a88  dl == 0            ⇒ END   ; Team2 没人
##     0x475bac  mov [self+0x120],6          ; EndingGame
## ```
##
## **"打靶关"与"对战关"不是两条规则**，是同一个判据的两种输入：
##
## | 关卡 | `Team2` 里有什么 | "Team2 全灭" 长什么样 |
## |---|---|---|
## | 打靶关 | **只有靶子**（`docs/150` §Q1：127/127 个靶子 `TeamInfo{Team2}`、0 个 `SoldierInfo`） | 靶子全清 |
## | 对战关 | 只有对手士兵 | 对方全灭 |
##
## ## 这份文件以前错在哪（`docs/148` §3.1 的第 1 条矛盾）
##
## `docs/131` 把胜负写成**按关卡类型两分**（`targets>0` 看靶子、`bot>0` 看队伍），
## 理由是"我们的客户端没有给靶子分配队伍"。那是**客户端自造的两分法**：
## 原版靶子本来就带队伍（`0x46f310` 的队伍是入参），所以一条规则就够，
## 两分法在**我们自己的实现里**反而制造了"两个真相源"。
## 现在：`GwTargetSet.TEAM`（= 1 = Team2）把靶子并进该队的存活统计，
## 判据只剩 [`evaluate`] 里那一段。
##
## 真值依据：
##
## | 规则 | 出处 | 等级 |
## |---|---|---|
## | 靶子是 `Team2` 的实体、且不是士兵 | `docs/150` §Q1（111 份回放 / 127 个靶子：`TeamInfo{Team2}` 127、`SoldierInfo` 0） | **观测** |
## | 造靶子的函数按**入参**写队伍 | `docs/150` §Q1（`0x46f310`：`movzx edx,bpl` → `0x3c17d0`） | **逐位** |
## | 判据 = 数"两队各有没有活实体" | `docs/150` §Q2（`0x473520`） | **逐位** |
## | 结束 = 上限用尽 / 某队没人（同一条出口） | `docs/150` §Q2/§Q3（`0x4759f0`） | **逐位** |
## | 回合上限用尽 ⇒ 胜方 `1`（Team2）——**以前标的是推断，现在是真值** | `docs/150` §Q3（`0x475bfb`–`0x475c14` 写 `[self+0x11f]`）；语料里 `TurnLimitInfo` 21 条全是 `{"turn_limit":N}` | **逐位 + 观测** |
## | 每关**靶子数 / 上限 / 有没有 Bot** | `docs/118` §7.1（`S`+`C`+`R` 三源，21/21） | **逐位 + 运行时** |
## | 217/217 次 `EndingGame` 都有一队"零活实体" | `docs/150` §Q2（401 份回放） | **观测** |

## 关卡类型 —— **现在只是给状态行/文档看的标签，不参与判定**（`docs/150` §3）。
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
## `docs/123` §5.1 #4 观测；`0x472400` 只在真的设上限时才被关卡分支调用）。
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

## 战役关：类型由 `docs/118` §7.1 的两列决定（**只是标签**，见文件头）。
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

## 非战役对局（LAN / 回放）：没有靶子、没有回合上限。
## 判定仍然是同一条 —— 只是 `Team2` 里只有士兵。
static func for_versus(human_team_: int) -> GwLevelRules:
	var r := GwLevelRules.new()
	r.kind = Kind.VERSUS
	r.human_team = human_team_
	return r

## 通用构造：类型由"有没有靶子 / 有没有对手"两件事决定（**只是标签**）。
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

## 判一局的胜负。**只有一条判据**：某队没有存活实体 ⇒ 结束。
##
## `state` 的键（都可省，缺省按"0 / 空"处理）：
##   * `alive_by_team`：`{team: 存活士兵数}`（**只数士兵**；靶子靠 `targets_*` 补）；
##   * `teams_present`：有士兵的队伍（含已全灭的队 —— 否则它一灭就从表里消失，
##     判不出"这队没了"）；
##   * `targets_total` / `targets_alive`：靶子集合的规模与存活数
##     （靶子算 **`GwTargetSet.TEAM` = Team2** 的成员，`docs/150` §Q1）；
##   * `turns_used`：已经结算过的回合数（与 `docs/118` §7.1 的 `turn_limit` 比）。
##
## 返回 `{over: bool, human_won: bool, winner_team: int, reason: String}`。
## `reason` 是**给状态行/自检看的标签**，不是分支：取值
## `playing` / `own_wiped` / `targets_cleared` / `enemy_wiped` / `mutual_wipe` /
## `turn_limit` / `no_verdict`。
##
## 注意 `winner_team` 在这条规则下是**算出来的**（"还有活实体的那队"），
## 不是按关卡类型猜的 —— 打靶关输掉时它仍然是 `1`（Team2 = 靶子所在的队），
## 因为靶子本来就是 Team2（`docs/150`）。
func evaluate(state: Dictionary) -> Dictionary:
	var alive: Dictionary = state.get("alive_by_team", {})
	var teams: Array = state.get("teams_present", [])
	var targets_total := int(state.get("targets_total", 0))
	var targets_alive := int(state.get("targets_alive", 0))
	var turns_used := int(state.get("turns_used", 0))

	# 参评的队伍 = 有士兵的队 ∪ 靶子所在的队（靶子有队伍，`docs/150` §Q1）。
	var judged: Array = []
	for t in teams:
		var team := int(t)
		if not judged.has(team):
			judged.append(team)
	if targets_total > 0 and not judged.has(GwTargetSet.TEAM):
		judged.append(GwTargetSet.TEAM)

	# ★ 唯一判据：逐队数**存活实体**（士兵 + 靶子），为 0 的队算"全灭"。
	var wiped: Array = []
	for team in judged:
		var still_alive := int(alive.get(team, 0))
		if team == GwTargetSet.TEAM:
			still_alive += targets_alive
		if still_alive == 0:
			wiped.append(team)

	if not wiped.is_empty():
		var human_wiped := wiped.has(human_team)
		var others := wiped.filter(func(t): return int(t) != human_team)
		if human_wiped and not others.is_empty():
			# 同一发把两边都清空：真值里没有样本（`docs/150` §6），按"人类失败"处理。
			return _verdict(true, false, _other_team(judged), "mutual_wipe")
		if human_wiped:
			# 我方全灭 ⇒ 对方胜（`0x475a80`：`al == 0` ⇒ END，胜方 = 还有活人的那队）。
			return _verdict(true, false, _other_team(judged), "own_wiped")
		# 对手方（打靶关 = 靶子所在的 Team2）全灭 ⇒ 人类胜（`0x475a88`）。
		var w := int(others[0])
		var reason := "enemy_wiped"
		if w == GwTargetSet.TEAM and int(alive.get(w, 0)) == 0 and targets_total > 0:
			# 这一队**没有士兵**，清空它的只能是靶子 ⇒ 标签用 targets_cleared。
			# ⚠️ 这只是**同一个判据**的两种标签，不是两条规则。
			reason = "targets_cleared"
		return _verdict(true, true, human_team, reason)

	# 回合上限 = **独立的**第二个结束入口（真值 `0x475a64`–`0x475a7a`，在两条 wipe 判定**之前**）。
	# 语料里没有"打满还没分胜负"的样本（`docs/123` §5.3 #1），但**二进制里有判定点**：
	# 上限用尽 ⇒ `EndingGame`，且胜方被写死成 `1`（Team2）—— 见 `docs/150` §Q3。
	if turn_limit > NO_TURN_LIMIT and turns_used >= turn_limit:
		return _verdict(true, false, _other_team(judged), "turn_limit")

	# 什么都没触发。若这局根本没有判定所需的双方（例如回放模式没有玩家表），
	# 明确说"判不了"而不是像旧实现那样"两队都是 0 ⇒ 有人赢"。
	if judged.is_empty():
		return _verdict(false, false, human_team, "no_verdict")
	return _verdict(false, false, human_team, "playing")

func _other_team(teams: Array) -> int:
	for t in teams:
		if int(t) != human_team:
			return int(t)
	# 打靶关没有别的玩家队 —— 靶子在 **Team2**（`docs/150` §Q1），
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
