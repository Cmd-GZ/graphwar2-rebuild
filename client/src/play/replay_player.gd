extends RefCounted
class_name GwReplayPlayer
## 回放里的**模式字符串 → `GwModes` 档位下标**（`dispatch/84` 之后只剩这张查表）。
##
## ## 这个文件原来是什么（84 号删掉 388 行、净少 361 行）
##
## 它原来是一份 **426 行**的 GDScript **局面机**：tick 时钟（`advance_dt` 每帧
## `tick += dt × 60`）、`apply_event()` 的 20 个 `match` 分支（覆盖 23 类事件）、`canonical()` 的三端对拍串、
## `actors()` / `obstacles_for_view()` / `target_positions()` 的取数，以及
## `seek` / `step_event` / `step_back_event`。这份局面机与内核
## `src/graphwar_replay_player.rs` **重复**（同一套规则两处实现），只有 `selftest` 在用。
##
## 84 号的退役顺序（`docs/16-client.md` §57）：
##   1. 先把旧实现算出来的期望值固化成 `fixtures/replay_player84.txt`（21 个键）；
##   2. `selftest` 那 13 条改打 `GwReplayDriver`（`src/_scaffold/replay_driver.rs`），
##      期望值全部取自那份夹具（**不是**新驱动自己给的）；
##   3. 局面机那些行删除（`git diff --numstat` = `+27 / −388`）—— 现在这里**没有**时钟、
##      没有事件规则、没有局面字典。
##
## ## 为什么还留着这四个函数（而不是整份删掉）
##
## `client/src/screens/play_screen.gd::_begin_replay()` 里还有 4 处
## `GwReplayPlayer.axis_index(...)` / `function_index` / `turn_index` / `time_index`：
## 它把回放里记着的模式字符串翻译成 `App.state.*` 的档位下标。那个文件在 84 号是**禁区**
## （`dispatch/84` §6：不许改 `client/src/screens/**`），所以类名先留在这里。
##
## ⚠️ **这不是"可以留在视图层的流程"**：下面四个函数是**纯查表**（字符串进、整数出，
## 没有状态、没有副作用、不看回放）。84 号实测：把整份文件删掉，`play_screen.gd`
## 会 **4 条 `Identifier "GwReplayPlayer" not declared`** 而整屏解析失败
## ⇒ 完全删除必须同时改播放屏那 4 行（改成调驱动侧的同一张表），那是**另一个任务**。
##
## ## 这张表本身的证据（`docs/133` §3.3，**观测**）
##
## 400 份真实回放里这几个字段的全部取值（括号内是出现次数）：
##   `AxisModeInfo`     EveryUnit(615) / EveryFive(34) / OnlyMain(54) / NoAxis(144)
##   `FunctionModeInfo` NormalFunction(548) / DiffEqFunction(320)
##   `TurnModeInfo`     SequentialTurns(658) / SimultaneousTurns(168)
##   `TimeModeInfo`     Timer1m(342) / TimerInf(470) / Timer3m(28) / Timer5m(22)
##                      / Timer30s(11) / Timer2m(191)
## 下标与 `GwModes` 的枚举顺序一致；那个顺序本身来自 `docs/92` §6 的连点观测。
## `AxisModeInfo` 的四个名字与 `docs/118` §7.1 从 21 关描述反推出的
## "no axis / main axes only / every 5 units / every unit" 一一对上（3 = EveryUnit）。

static func axis_index(s: String) -> int:
	match s:
		"EveryUnit": return 3
		"EveryFive": return 2
		"OnlyMain": return 1
		"NoAxis": return 0
	return 3

static func function_index(s: String) -> int:
	return 1 if s == "DiffEqFunction" else 0

static func turn_index(s: String) -> int:
	return 1 if s == "SimultaneousTurns" else 0

static func time_index(s: String) -> int:
	match s:
		"Timer30s": return 0
		"Timer1m": return 1
		"Timer2m": return 2
		"Timer3m": return 3
		"Timer5m": return 4
		"TimerInf": return 5
	return 5
