extends RefCounted
class_name GwKeyInput
## 物理键盘 → 函数输入框的命令映射（`docs/135` §1）。
##
## ## 原版证据（观测）
##
## 1. **`GraphwarFunctionInput` 是 `LineEdit` 的 Rust 子类。** 恢复出的
##    `play_screen.tscn`（GDRE 从 `play_screen.scn` 反解，`docs/104` §4）在它身上写的属性
##    只有 `LineEdit` 才有（`play_screen.tscn:764-772`）：
##    `alignment = 1`、`caret_blink = true`、`caret_force_displayed = true`、
##    `context_menu_enabled = false`、`virtual_keyboard_enabled = false`。
##    （`docs/92` §4.10 的"Rust 自绘、无 `LineEdit` **子节点**"没错——它是**自己就是**
##     一个 LineEdit，不是"里面装着一个"。）
## 2. **它在自己的 `gui_input` 里处理按键。** `.so` 的字符串池里有
##    `gui_input`（与 `text_submitted`、`size_changed`、panic 位置
##    `src/graphwar_function_input.rs:519` 同一段），以及一批 **`LineEdit` 专有方法**：
##    `set_caret_column` / `get_caret_column` / `delete_char_at_caret` /
##    `insert_text_at_caret` / `clear`。Godot 的 `gui_input`
##    **只有拿到焦点才会收到按键** ⇒ **原版是"输入框聚焦时接管键盘"**，不是全局按键。
##    （`.so` 里也有 `grab_focus`，但看不出它调在哪个节点上，所以"会不会自己抢焦点"**未知**。）
##
## ## 我们怎么接（**自定**）
##
## 映射逻辑在这个类里，触发点是 `PlayScreen._unhandled_key_input()`（屏幕级接管），
## 因此**不需要先点输入框**就能打字。理由与代价：
##
## * 原版到底会不会自动 `grab_focus` **没有观测**（`.so` 里有 `grab_focus` 这个字符串，
##   但看不出调用的是哪个节点；`docs/92` 的 282 个按钮全是 `focus_mode` 默认值）。
##   用户实测"原版可以直接敲键盘输入"，若原版其实要求先点一下输入框，用户多半也会
##   描述成"直接敲"——两种情形都无法从现有证据区分。
## * 屏幕级接管把"聚焦后能打字"包含在内（`LineEdit` 拿到焦点时本来也是这个效果），
##   所以它对两种情形都成立；代价是它比原版**更宽**：理论上原版在焦点不在输入框时
##   按键会被别处吞掉，而我们一律吃进函数串。
## * 要升级成观测：需要原版真机上的按键事件日志（hook `gui_input` 或录屏），看
##   ——① 进 PlayScreen 后不点任何东西是否就能打字；② 焦点是否在 `GraphwarFunctionInput`
##   上。见 `docs/135` §1.3。
##
## ## 键位（**自定**，除标了观测的几条）
##
## | 键 | 效果 | 依据 |
## |---|---|---|
## | 数字 `0`–`9`、`.`、`(`、`)`、`+`、`-`、`*`、`/`、`^` | 插入该字符（按 `unicode`，Shift 自动换算） | 文法（`src/function/mod.rs::lex`） |
## | 字母 `a`–`z` / `A`–`Z` | 插入**小写**字符（`sin(`、`sqrt(`、`pi`、`e`、`x`、`y`… 直接敲即可） | 词法器整串 `to_lowercase()` |
## | `π` | 插入 `π` | 文法里 `π` = π（`lex` 把 `pi`/`π` 换成 `n`） |
## | `√` | 插入 `sqrt(` | 屏上键 `√` 的 `input_text` 就是 `sqrt(`（`docs/92` §4.10）；裸 `√` 会被词法器**跳过** |
## | `Backspace` | `⌫`（删光标左） | 屏上键同名命令 |
## | `Delete` | `⌦`（删光标右） | 同上 |
## | `←` / `→` / `Home` / `End` | `←` / `→` / `⇤` / `⇥`（移光标） | 同上 |
## | `Enter` / 小键盘 `Enter` | **提交**（= `text_submitted` → `fire()`） | `text_submitted` 是 LineEdit 的信号（`docs/104` §6.3 #3） |
## | 空格、`%`、`,`、`=`、`'`、`Tab`、`Esc`… | **不插入** | 文法里没有；词法器会跳过 |
## | `Ctrl` / `Alt` / `Meta` 组合键 | 不接管（留给快捷键） | 自定 |
##
## 没有做的两件事（诚实标注）：
## * 屏上键 `C`（清空）**没有**映射到物理 `c`——因为 `c` 是 `cos(` 的首字母，
##   抢了就敲不出 `cos(`。原版 `C` 键的真实语义本身也只是**推断**（`docs/108` §7），
##   物理键盘上怎么清空 **未观测**；要清空请用 `Home` + 反复 `Delete` 或 `C` 屏键。
## * `y'`（死特性，`AGENTS.md` 硬事实："第三个变量 `y'` 在所有 7 个调用点都传 0"）
##   没有映射；屏上键盘也没有这个键。

## 提交哨兵：回车在 `PlayScreen._unhandled_key_input()` 里被识别成"提交"，
## 不当作字符插入。用 `\n` 是因为它**不在**可插入字符集里，永远撞不上。
const SUBMIT := "\n"

## 允许**直接插入**的字符集（小写字母在表里；大写先 `to_lower()`）。
## 依据：`src/function/mod.rs::lex` 认的记号 = 数字 / `.` / `( ) + - * / ^` /
## `x` / `y` / `y'` / `sqrt log abs sin cos tan ln exp e` / `n`（=π），
## 而整串会先 `to_lowercase()` + 把 `pi`/`π` 换成 `n`。
const PRINTABLE := "0123456789.()+-*/^abcdefghijklmnopqrstuvwxyzπ√"

## 多字符替换（见上表）。`√` 不是文法记号，但屏上键把它当 `sqrt(`。
const REPLACEMENTS := {"√": "sqrt("}

## 一个按键 → 一条**屏上键盘命令**（`GwFunctionInput.key_input(cmd)` 收的那种字符串）。
## 返回 `""` = 这个键不归我们管；返回 `SUBMIT` = 回车提交；其余 = 插入/编辑命令。
static func command_for(event: InputEventKey) -> String:
	if event == null or not event.pressed:
		return ""
	# Ctrl/Alt/Meta 组合留给系统快捷键（Ctrl+S 之类），不抢。
	if event.ctrl_pressed or event.alt_pressed or event.meta_pressed:
		return ""
	# 编辑键/提交键先按 `keycode` 认：这几个键没有可打印的 unicode。
	match event.keycode:
		KEY_BACKSPACE:
			return GwCommands.KEY_BACKSPACE
		KEY_DELETE:
			return GwCommands.KEY_DELETE
		KEY_LEFT:
			return GwCommands.KEY_LEFT
		KEY_RIGHT:
			return GwCommands.KEY_RIGHT
		KEY_HOME:
			return GwCommands.KEY_HOME
		KEY_END:
			return GwCommands.KEY_END
		KEY_ENTER, KEY_KP_ENTER:
			return SUBMIT
	var ch := _char_of(event)
	if ch == "":
		return ""
	if REPLACEMENTS.has(ch):
		return String(REPLACEMENTS[ch])
	return ch

## 取这次按键**实际打出来的那个字符**。
##
## 优先 `unicode`：它已经是"按当前键盘布局 + Shift 换算过"的字符
## （物理键 `9` 与 `(` 是同一个键，只有 unicode 能区分；中文输入法上屏的 `π` 也只有这条路）。
## 没有 unicode 时（某些合成事件/输入法前置事件）按 `keycode` 反推——Godot 的
## 字母/数字/标点 Key 枚举值就是 ASCII 码，所以 `String.chr(kc)` 直接对。
static func _char_of(event: InputEventKey) -> String:
	if event.unicode > 0:
		return filter_char(String.chr(event.unicode))
	var kc := event.keycode
	if kc == 0:
		kc = event.physical_keycode
	if kc >= 32 and kc <= 126:
		return filter_char(String.chr(kc))
	return ""

## 字符过滤：只放行文法认得的字符（大写折成小写），其它一律丢弃。
## **过滤规则本身是自定**：原版是否也过滤、过滤到哪一档，没有观测（见文件头）。
static func filter_char(ch: String) -> String:
	if ch.length() != 1:
		return ""
	if ch == "π" or ch == "√":
		return ch
	var c := ch.to_lower()
	if PRINTABLE.find(c) < 0:
		return ""
	return c
