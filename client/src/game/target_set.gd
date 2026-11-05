extends RefCounted
class_name GwTargetSet
## 一局里的**靶子集合**（`docs/131`）。
##
## 真值形状（`docs/123` §2.1）：靶子**不是**独立对象，而是 `Team2` 里一个带
## `SkinGraphicsInfo="target"` 的普通实体。它只有两种状态事件 —— `TargetInfo`(23) 与
## `LifeInfo`(27) —— 且造目标的 `0x46f310` **运行期没有调用点**（13 个调用点全在
## 关卡开场）⇒ "打不全就复原"只能**翻 alive 位 + 广播 `LifeInfo{alive:true}`**，
## 不重建实体、不发 `TargetInfo`、不碰掩膜（`docs/123` §2.2、§4.3）。
##
## 本类只持有界面侧需要的三样：位置、半径、alive。**判定本体不在这里** ——
## 谁被打中仍由 `GwCore.hit_test()`（内核 `src/api.rs::hit_test` → `settle::resolve_points`）
## 决定，本类只接受它的结果并维护状态。
##
## 复原规则（`docs/131` §4，**推断**）：一次射击结算结束时，若"本发摧毁的靶子"
## ≠ "全部靶子"，则把所有已摧毁的靶子翻回 alive。依据是**用户实测量 + `docs/123` 的
## 形状结论**，判定点（哪一刻、拿什么条件）**没有在二进制里找到** —— 见 `settle_shot()`。

## 靶子在真值里属于 **Team2**（`docs/150` §Q1）。
##
## 证据：111 份含靶子的回放里，**127/127** 个靶子 id 都收到 `TeamInfo{team:"Team2"}`
## （同时 127/127 **没有** `SoldierInfo`、`SkinGraphicsInfo` 恒为 `"target"`）；
## 造靶子的 `0x46f310` 里队伍是**入参**（`movzx edx,bpl` → `TeamInfo` 的构造点 `0x3c17d0`）。
##
## ⇒ 这个常量不是"我们的约定"，而是把真值里那条**实体属性**写进模型。
## 胜负判定（`GwLevelRules.evaluate()`）靠它把靶子并进 `Team2` 的存活统计，
## 于是"靶子全清"与"Team2 全灭"是**同一件事**，不需要第二条规则（`docs/150` §3）。
const TEAM := 1

## 靶子的致死半径（棋盘单位）。
##
## **推断**：`docs/122` §1 已证明致死半径是**士兵记录里的数据字段**
## （条目 `+0x18`，`PosInfo.radius` 在 5300 条回放里恒 `0.5`），**不是**编译期常量；
## 靶子的半径**本来是观测到了的**：127 个靶子 id 的 302 条 `PosInfo.radius` **全是 0.5**
## （`docs/150` §Q1）。⇒ 这个 0.5 已经从"推断"变成"观测"。
const HIT_RADIUS := 0.5

## 全部靶子的位置（棋盘坐标），下标 = 靶子 id。
var _pos: Array[Vector2] = []
## 与 `_pos` 同下标的存活位 —— 对应实体记录里的那个 alive 字节（`docs/123` §3.3）。
var _alive: Array[bool] = []

## 致死半径；默认 [`HIT_RADIUS`]，留成字段只为将来"靶子有自己的半径"时不用改签名。
var hit_radius := HIT_RADIUS

## 装一批靶子（开局一次）。`positions` 是 `Array[Vector2]`（棋盘坐标），下标即靶子 id。
func setup(positions: Array) -> void:
	_pos.clear()
	_alive.clear()
	for p in positions:
		var v: Vector2 = p
		_pos.append(v)
		_alive.append(true)

func total() -> int:
	return _pos.size()

func alive_count() -> int:
	var n := 0
	for a in _alive:
		if a:
			n += 1
	return n

func destroyed_count() -> int:
	return total() - alive_count()

func is_alive(id: int) -> bool:
	return id >= 0 and id < _alive.size() and _alive[id]

## 全部靶子都没了（空集合也返回 true —— 调用方要自己先看 `total() > 0`）。
func is_cleared() -> bool:
	return alive_count() == 0

## 画用的位置表：**只给还活着的**。`GwPlaneView.set_targets()` 收的就是
## `Array[Vector2]`（它只负责画，参数名 `targets`），所以"靶子消失/复原"在界面上
## 就是"这个数组少一项/多一项"。
func alive_positions() -> Array:
	var out := []
	for i in _pos.size():
		if _alive[i]:
			out.append(_pos[i])
	return out

## 判命中用的实体表：`[{pos: Vector2, radius: float, target_id: int}, …]`，只含活着的。
##
## ⚠️ **借用关系**（`docs/131` §3.1）：内核 `hit_test` 的第 3 个入参在签名里叫
## `soldiers`，但它的实现只是把每个三元组变成 `settle::Target{id: 下标, x, y, radius}`
## （`src/api.rs`），**没有任何"士兵"语义过滤**。所以靶子与士兵可以塞进**同一个数组**：
## 内核只回"哪几个下标被判死"，由调用方按下标映射回"是士兵还是靶子"。
func entities() -> Array:
	var out := []
	for i in _pos.size():
		if _alive[i]:
			out.append({"pos": _pos[i], "radius": hit_radius, "target_id": i})
	return out

## 标记一个靶子被摧毁。返回 `true` 表示这次真的从 alive 翻成了 dead（幂等）。
func destroy(id: int) -> bool:
	if not is_alive(id):
		return false
	_alive[id] = false
	return true

## 翻回 alive（**不重建实体**）。返回真正被翻回来的 id。
func restore(ids: Array) -> Array:
	var back := []
	for id in ids:
		var i := int(id)
		if i >= 0 and i < _alive.size() and not _alive[i]:
			_alive[i] = true
			back.append(i)
	return back

## 一次射击的结算：**打不全就全部复原**。
##
## `destroyed_this_shot`：本发摧毁的靶子 id（保序，由调用方按弹道经过顺序收集）。
## 返回 `{reset: bool, ids: Array[int], reason: String}`：
##   * 本发一个都没打掉 → `reset:false`（走火/空放什么都不发生）；
##   * 本发把靶子清空 → `reset:false`（全部达成，不复原）；
##   * 否则 → `reset:true`，`ids` = **全部已摧毁的靶子**（= "把所有靶子恢复"）。
##
## 等级：**推断（依据用户实测 + `docs/123` §3.2 的形状结论）**。
## 未证实的部分（都写在 `docs/131` §4.2）：判定时刻（射击结算 / end_turn / 下次开火前）、
## "全部复原"还是"只复原本发打掉的"（单发场景下两者等价）、
## 是否顺带重置回合计数、有无视觉效果。语料里 397 份回放 **0 次部分命中**（`docs/123` §4.3），
## 所以这条规则**没有真值可对**。
func settle_shot(destroyed_this_shot: Array) -> Dictionary:
	if total() == 0:
		return {"reset": false, "ids": [], "reason": "no_targets"}
	if destroyed_this_shot.is_empty():
		return {"reset": false, "ids": [], "reason": "nothing_destroyed"}
	if is_cleared():
		return {"reset": false, "ids": [], "reason": "all_destroyed"}
	var ids := []
	for i in _pos.size():
		if not _alive[i]:
			ids.append(i)
	return {"reset": true, "ids": ids, "reason": "partial"}

## 诊断快照（自检与状态行用）。
func snapshot() -> Dictionary:
	return {
		"total": total(),
		"alive": alive_count(),
		"destroyed": destroyed_count(),
		"radius": hit_radius,
		"positions": alive_positions(),
	}
