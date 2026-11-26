extends RefCounted
class_name GwKeyInput
## 物理键盘 → 函数输入框的命令映射（`docs/135` §1）。
##
## **2026-11-26（T42 ③）本类被削成"输入接线"一层**：原来它还带着"按键 → 字符"的
## **规则**（`PRINTABLE` 白名单、`√ → sqrt(` 替换、大小写折叠），那是把
## `src/graphwar_function_input.rs` 的规则在 GDScript 里又写了一遍。现在：
##
## | 谁负责什么 | 落点 |
## |---|---|
## | 哪几个物理键 = 哪几条**屏上命令**（编辑键 / 回车） | **这里**（本方法，输入接线） |
## | 按键 → 要插入的字符（原版 28 条死键折叠 + 我方白名单） | `GwFunctionEdit.key_text` ← `graphwar_function_input::classify_key`（跳转表 `0x1c3d5c`，198 项里 28 项） |
## | 一条命令怎么改文本/光标（编辑语义） | `GwFunctionEdit.apply_command` ← `graphwar_function_input::Buffer`（`0x4f3210`/`0x4f3910`/`0x4f44c0`） |
##
## 逐段台账见 `fixtures/function_input_migration.tsv`。
##
## ## 原版证据（观测，规则的出处）
##
## 1. **`GraphwarFunctionInput` 是 `LineEdit` 的 Rust 子类**（`recovered-src/play_screen.tscn`
##    上它带的全是 `LineEdit` 专有属性：`alignment = 1`、`caret_blink = true`、
##    `context_menu_enabled = false`…），文字状态与编辑在 Rust 侧。
## 2. **它在自己的 `gui_input` 里处理按键**。`.so` 字符串池里有 `gui_input` 与一批
##    `LineEdit` 专有方法（`set_caret_column` / `get_caret_column` / `delete_char_at_caret` /
##    `insert_text_at_caret` / `clear`）；Godot 的 `gui_input` **只有拿到焦点才收按键**
##    ⇒ 原版是"输入框聚焦时接管键盘"。"会不会自己抢焦点"**未知**。
##
## ## 我们怎么接（**自定**）
##
## 触发点是 `PlayScreen._unhandled_key_input()`（屏幕级接管），因此**不需要先点输入框**。
## 理由与代价：原版是否自动 `grab_focus` 无观测；屏幕级接管对"自动聚焦"和"要点一下"
## 两种情形都成立，代价是它比原版**更宽**（焦点不在输入框时也会吃键）。升级路径见
## `docs/135` §1.3。
##
## ## 没有做的两件事（诚实标注）
##
## * 屏上键 `C`（清空）**没有**映射到物理 `c`——`c` 是 `cos(` 的首字母，抢了就敲不出 `cos(`。
##   原版 `C` 键的真实语义本身也只是**推断**（`docs/108` §7），物理键盘上怎么清空**未观测**。
## * `y'`（死特性：第三个变量 `y'` 在所有 7 个调用点都传 0）没有映射；屏上键盘也没有这个键。

## 提交哨兵：回车在 `PlayScreen._unhandled_key_input()` 里被识别成"提交"，
## 不当作字符插入。用 `\n` 是因为它**不在**可插入字符集里，永远撞不上。
const SUBMIT := "\n"

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
	return _char_of(event)

## 取这次按键**实际打出来的那个字符**（**规则在 Rust**：`GwFunctionEdit.key_text`）。
##
## `unicode` 优先：它已经是"按当前键盘布局 + Shift 换算过"的字符（物理键 `9` 与 `(`
## 是同一个键，只有 unicode 能区分；输入法上屏的 `π` 也只有这条路）。没有 unicode 时
## （某些合成事件/输入法前置事件）按 `keycode` 反推 —— Godot 的字母/数字/标点 Key
## 枚举值就是 ASCII。折叠与过滤全在 Rust，这里只递参数。
static func _char_of(event: InputEventKey) -> String:
	var kc := event.keycode
	if kc == 0:
		kc = event.physical_keycode
	return GwFunctionEdit.key_text(event.unicode, kc)
