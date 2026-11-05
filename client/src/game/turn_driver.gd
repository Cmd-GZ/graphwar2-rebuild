extends RefCounted
class_name GwTurnDriver
## 回合射手轮转（`docs/143` §6）：**照 `src/turn.rs` 的那条规则**在客户端侧算"下一个该谁开火"。
##
## ## 为什么客户端要有一份
##
## 原版的回合推进在**服务端**：`0x463da0`（`graphwar_turn_manager.rs`）选好下一个
## `soldier_id`，然后广播 `TurnInfo{player_id, soldier_id}`（`docs/98` §1.4、
## `docs/109` §2.2/§2.3 的 objdump 逐条）。我们这一版没有本地服务端，所以
## **必须有一个"谁开火"的唯一权威**；以前的 `_next_turn()` 是 `(_active+1) % n`
## 的**玩家**轮转，与二进制里"每玩家一张士兵表 + 游标"不是一回事
## （多士兵关卡里第二个士兵永远轮不到 —— `docs/143` §6.2）。
##
## 本类只复刻**规则**，不冒充协议：它不生成报文、不判胜负、不碰动画。
##
## ## 证据档位
##
## | 项 | 档位 | 依据 |
## |---|---|---|
## | "先自增再取"（`index = (index+1) % len`） | **逐位** | `0x463f40`–`0x463f58`：`mov -0x8(%rbp),%rax` / `inc %rax` / `div %r15` / `mov %rdx,-0x8(%rbp)`（`docs/109` §2.3 原文照录） |
## | 最多试 `len` 次、跳过死士兵 | **逐位** | `0x463fc7`–`0x463fca` 的 `dec %r14` / `jne`；`0x463fb6` 的 `call 454950` + `test $0x1,%al` |
## | 存活只看生命字节最低位 | **逐位** | 同上（`life::is_alive_byte`，`docs/109` §2.5） |
## | 一个玩家的士兵全灭 ⇒ 该玩家给 `soldier_id = 0` | **逐位** | `0x464016`–`0x464021`（`movl $0x0,0x64(%rsp)`） |
## | **玩家之间**的轮转顺序 | **未知**（原版判为 `HashMap` 迭代序，`docs/47`） | 这里固定用 `GwGameState.players` 的下标序（= 调用方给的顺序），见 `docs/143` §6.3 |
## | 初始游标 | **未知**（`docs/109` §6-A） | 这里取"第一个玩家的第一个士兵"，与既有行为一致 |

## `docs/98` §1.4 / `docs/109` §2.2：全灭时广播的哨兵值。
const SOLDIER_ID_NONE := 0

## 每个玩家一组：`{player: int, soldiers: Array[int], index: int}`。
## `soldiers` 里放的是**调用方的士兵下标**（本类不解释它的含义）。
var _players: Array = []
## 当前玩家游标（`_players` 的下标）。
var _cursor := 0

## 装表。`groups` = `[{player: int, soldiers: Array[int]}, …]`，顺序即玩家顺序。
## 重复调用会重置游标（换局时用）。
func setup(groups: Array) -> void:
	_players = []
	_cursor = 0
	for g in groups:
		var list: Array = []
		for s in (g.get("soldiers", []) as Array):
			list.append(int(s))
		_players.append({
			"player": int(g.get("player", -1)),
			"soldiers": list,
			"index": 0,          # 初始游标：0（**未知**项，见文件头）
		})

## 找下一个射手。`alive` 收**士兵下标**、返回 bool。
##
## 返回士兵下标；`-1` = 一张能开火的表都没有（全灭 / 空表）。
## 语义对齐 `PlayerTurns::next_shooter` 的 `Some(id)` / `Some(SOLDIER_ID_NONE)`：
## 单个玩家全灭 ⇒ 跳过它（客户端不需要"给空表玩家发一条 `TurnInfo`"，
## 那条广播是服务端的义务，见 `docs/109` §2.4）。
func next_shooter(alive: Callable) -> int:
	var n := _players.size()
	if n == 0:
		return -1
	if _cursor >= n:
		_cursor %= n
	for _p in n:
		_cursor = (_cursor + 1) % n            # ① 先自增再取
		var g: Dictionary = _players[_cursor]
		var list: Array = g["soldiers"]
		var m := list.size()
		if m == 0:
			continue                            # 这个玩家没有士兵 ⇒ 换下一个
		var idx := int(g["index"])
		if idx >= m:
			idx %= m
		for _i in m:                            # ② 最多试 len 次
			idx = (idx + 1) % m
			var sid := int(list[idx])
			if bool(alive.call(sid)):           # ③④ 只看最低位算活
				g["index"] = idx
				return sid
		g["index"] = idx                        # 全灭也要把游标写回
	return -1                                    # ⑤ 没有任何活人

## 当前射手（上一次 `next_shooter()` 的结果）。没调过返回 `-1`。
func current_shooter() -> int:
	if _cursor < 0 or _cursor >= _players.size():
		return -1
	var g: Dictionary = _players[_cursor]
	var list: Array = g["soldiers"]
	if list.is_empty():
		return -1
	return int(list[int(g["index"]) % list.size()])

## 开局第一个射手。
##
## 与 `next_shooter()` 不同，这里**不**先自增：`setup()` 把每张表的游标放在 0，
## 语义是"0 号士兵正握着这一回合"，所以第一个射手就是它。
##
## ⚠️ 这**不是**复原出来的结论：原版的**初始游标**未定（`docs/109` §6-A —— 三例吻合 0，
## 但 `1782763022` 首取 `2509` 不符）。此处取 0 只是为了与既有行为（第 0 号士兵先打）一致。
## 0 号不活时退回 `next_shooter()`（最多试 `len` 次，行为与二进制同）。
func first_shooter(alive: Callable) -> int:
	_cursor = 0
	if _players.is_empty():
		return -1
	var g: Dictionary = _players[0]
	var list: Array = g["soldiers"]
	if list.size() > 0:
		var idx := int(g["index"]) % list.size()
		if bool(alive.call(int(list[idx]))):
			return int(list[idx])
	return next_shooter(alive)

## 只读快照（自检/诊断）。
func snapshot() -> Dictionary:
	var rows: Array = []
	for g in _players:
		rows.append({
			"player": int(g["player"]),
			"soldiers": (g["soldiers"] as Array).duplicate(),
			"index": int(g["index"]),
		})
	return {"cursor": _cursor, "players": rows, "count": _players.size()}
