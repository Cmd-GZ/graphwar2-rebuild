extends RefCounted
class_name GwModes
## 四个对局选项的状态机。
##
## 全部来自**运行时观测**（`docs/92` §6，连点 `@@CYCLE` 枚举）：
##   AxisMode    4 态，初值 3，按一次 3→2→1→0→3
##   FunctionMode 2 态，初值 fx，fx ⇄ dx_dy
##   TurnMode    2 态，初值 sequential，sequential ⇄ simultaneous
##   TimeMode    6 态，初值 1m，1m→2m→3m→5m→∞→30s→1m
##
## 交叉验证：`docs/92` §6 的"按 2 次 ⇒ 开局 02:59（3 分钟档）"在本项目里成立
## （见 tools/selftest.gd 的 `time_mode_cycle` 用例）。
##
## ⚠️ 语义未知的部分：AxisMode 的 4 个档各是什么坐标轴，**没有任何观测**
## （`docs/88` §17 只证明它不参与任何判定）。这里原样保留 4 个档位与图标名，
## 不编造含义。

enum Axis { AXIS_0, AXIS_1, AXIS_2, AXIS_3 }
enum Function { NORMAL, DIFFEQ }
enum Turn { SEQUENTIAL, SIMULTANEOUS }
enum TimeLimit { T30S, T1M, T2M, T3M, T5M, INF }

const AXIS_COUNT := 4
const FUNCTION_COUNT := 2
const TURN_COUNT := 2
const TIME_COUNT := 6

## 各档的图标名（原版 `rsc/` 里的 svg 文件名；我们只借用**名字**，
## 图标本身由 `icon_factory.gd` 现画，见 docs/108 §6）。
const AXIS_ICONS := ["axis_option_0", "axis_option_1", "axis_option_2", "axis_option_3"]
const FUNCTION_ICONS := ["fx_option", "dx_dy_option"]
const TURN_ICONS := ["turns_sequential", "turns_simultaneous"]
const TIME_ICONS := ["turn_30s", "turn_1m", "turn_2m", "turn_3m", "turn_5m", "turn_inf"]

## 回合时限（秒）。0 = 无限。时长本身是推断：由 `TimerLabel` 的 `02:59/01:59` 反推
## （`docs/92` §4.10、§6），且 `docs/35` 已证 tick 时钟 60 Hz。
const TIME_SECONDS := [30, 60, 120, 180, 300, 0]

## 档位的中文说明只用于 tooltip；界面文字保持原版英文（原版无多语言支持）。
const TIME_LABELS := ["30 s", "1 min", "2 min", "3 min", "5 min", "∞"]

const AXIS_DEFAULT := 3
const FUNCTION_DEFAULT := 0
const TURN_DEFAULT := 0
const TIME_DEFAULT := 1


static func axis_next(v: int) -> int:
	return wrapi(v - 1, 0, AXIS_COUNT)      # 观测顺序 3→2→1→0→3

static func function_next(v: int) -> int:
	return wrapi(v + 1, 0, FUNCTION_COUNT)

static func turn_next(v: int) -> int:
	return wrapi(v + 1, 0, TURN_COUNT)

static func time_next(v: int) -> int:
	return wrapi(v + 1, 0, TIME_COUNT)      # 1m→2m→3m→5m→∞→30s→1m

static func axis_icon(v: int) -> String:
	return AXIS_ICONS[wrapi(v, 0, AXIS_COUNT)]

static func function_icon(v: int) -> String:
	return FUNCTION_ICONS[wrapi(v, 0, FUNCTION_COUNT)]

static func turn_icon(v: int) -> String:
	return TURN_ICONS[wrapi(v, 0, TURN_COUNT)]

static func time_icon(v: int) -> String:
	return TIME_ICONS[wrapi(v, 0, TIME_COUNT)]

static func turn_seconds(v: int) -> int:
	return TIME_SECONDS[wrapi(v, 0, TIME_COUNT)]

static func time_label(v: int) -> String:
	return TIME_LABELS[wrapi(v, 0, TIME_COUNT)]

## `02:59` 形状。原版显示 `MM:SS`，且**开局就少 1 秒**（3 分钟档显示 02:59，
## 2 分钟档显示 01:59，见 `docs/92` §4.10 / §6）—— 所以这里取整是"向下取整"。
static func format_clock(seconds_left: float) -> String:
	var total := int(floor(maxf(seconds_left, 0.0)))
	return "%02d:%02d" % [total / 60, total % 60]
