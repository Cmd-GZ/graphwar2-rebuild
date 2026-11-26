//! **不是原版的东西**：把 [`crate::graphwar_function_input`] 里**已经复原的编辑规则**
//! 接成 GDExtension 类 `GwFunctionEdit`，让 `client/src/play/*.gd` 只剩视图。
//!
//! # 为什么会有这一份（T42 ③）
//!
//! 原版函数输入框是 **`LineEdit` 的 Rust 子类**（`GraphwarFunctionInput`）：文本状态、
//! 按键分发、编辑操作全在 Rust 侧（我方复原在 `src/graphwar_function_input.rs`）。
//! 而 `client/src/play/function_input.gd`（273 行）曾经把**编辑规则**又用 GDScript 写了一遍
//! （`key_input()` 的 8 个 `match` 分支）、`client/src/play/keyboard_input.gd`（130 行）
//! 把**按键→字符规则**又写了一遍，两处都不引用 Rust 侧同一批函数 ⇒ 同一规则两个真相源
//! （`AGENTS.md` §0.4 说的"仿制控制器"就是这个形态）。
//!
//! 本文件把**规则**收成一处，`.gd` 只留"画键盘 / 读焦点 / 贴图 / 发信号"。
//! 逐段台账见 `fixtures/function_input_migration.tsv`（含每段为什么搬/不搬）。
//!
//! # 规则表（每条都指到地址；地址一律 ELF vaddr）
//!
//! | 命令（屏上键 `input_text`） | 落到的 Rust 操作 | 原版证据 | 档位 |
//! |---|---|---|---|
//! | `C`（清空） | `Buffer::new("")` | `LineEdit::clear` 在 `.so` 字符串池里；**"C = 清空"本身没有逐条观测** | **推断** |
//! | `⌫`（删光标左） | `Buffer::delete_chars_at_caret(1)` = 1 × `delete_char_at_caret` | 字面量 `0x1c44a2`（长 20）+ 循环体 `0x4f4410`（`delete_token_at_caret` `0x4f44c0` 里调它 4 次） | **观测** |
//! | `⌦`（删光标右） | `move_caret_right(1)` → `delete_chars_at_caret(1)` | `LineEdit` 只有"删光标左"一个 API（`delete_char_at_caret`）；右删 = 跨过去再左删 | **推断**（缺"原版右删走哪条路径"的指令证据） |
//! | `←` / `→`（移光标） | `Buffer::move_caret_left(1)` / `move_caret_right(1)` | `move_caret_left` `0x4f3210`、`move_caret_right` `0x4f3910` | **观测** |
//! | `⇤` / `⇥`（行首/行尾） | `Buffer::move_caret_home()` / `move_caret_end()` | 2.0.60 里**没有独立函数体**（`graphwar_function_input::UNKNOWNS` 第 4 条：内联或只以 Callable 存在） | **推断** |
//! | 其余（`sin(`、`π`、`x`、`3`…） | `Buffer::insert_text_at_caret(&cmd)`（原样插入） | 字面量 `insert_text_at_caret`（`0x1c44??` 一族的 `LineEdit` 专有方法，见 `keyboard_input.gd` 原注释） | **观测** |
//!
//! # 按键 → 字符：走**原版 `gui_input` 的 28 条折叠表**，再走我们的可打印集过滤
//!
//! 原版 `gui_input`（`0x4feee3`…`0x4ff1e1`）只看 `InputEventKey.unicode` 一个 `char`，跳转表
//! `0x1c3d5c` 198 项里**只有 28 项**不落默认分支 —— 那 28 项是 AZERTY/QWERTZ 死键合成的
//! `â ê î ô û` / `Ĉ Ĝ Ĥ Ĵ Ŝ Ŵ Ŷ` 与上标 `² ³ ¹`、`·`，被折回 ASCII 后插入
//! （表在 [`crate::graphwar_function_input::COMPOSED_FOLDS`]，档位**观测**）。**ASCII 字符不在表里**
//! ⇒ 原版把它们交给基类 `LineEdit`（任何可打印字符都进得去，文法检查留到解析器）。
//!
//! 改前我方 `.gd` **完全没有这张折叠表**（`filter_char()` 只查自己的 `PRINTABLE` 串），
//! 于是 `â` 这类字符被**丢掉**，而原版会插入 `a` —— 这是本轮真正升上去的那条保真度。
//!
//! ## 键位表（从 `client/src/play/keyboard_input.gd` 搬过来的那张，别丢）
//!
//! | 键 | 效果 | 依据 |
//! |---|---|---|
//! | 数字 `0`–`9`、`.`、`(`、`)`、`+`、`-`、`*`、`/`、`^` | 插入该字符（按 `unicode`，Shift 自动换算） | 文法（`src/math_function.rs::lex`） |
//! | 字母 `a`–`z` / `A`–`Z` | 插入**小写**字符（`sin(`、`sqrt(`、`pi`、`e`、`x`、`y`…） | **我们的选择**：原版在解析器里 `to_lowercase()`，输入框原样收 |
//! | `π` | 插入 `π` | 文法里 `π` = π（`lex` 把 `pi`/`π` 换成 `n`） |
//! | `√` | 插入 `sqrt(` | **我们的选择**（屏上键 `√` 的 `input_text` 就是 `sqrt(`，`docs/92` §4.10；物理键未观测） |
//! | `Backspace` | `⌫`（删光标左） | 屏上键同名命令；`delete_char_at_caret`（字面量 `0x1c44a2`） |
//! | `Delete` | `⌦`（删光标右） | 同上（右删是推断：跨过去再左删） |
//! | `←` / `→` / `Home` / `End` | `←` / `→` / `⇤` / `⇥`（移光标） | `move_caret_left` `0x4f3210` / `move_caret_right` `0x4f3910`；`⇤/⇥` 是推断 |
//! | `Enter` / 小键盘 `Enter` | **提交**（= `text_submitted` → `fire()`） | `text_submitted` 是 LineEdit 的信号（`docs/104` §6.3 #3） |
//! | 空格、`%`、`,`、`=`、`'`、`Tab`、`Esc`… | **不插入** | **我们的选择**：白名单里没有（原版会收下再被词法器跳过） |
//! | `Ctrl` / `Alt` / `Meta` 组合键 | 不接管（留给快捷键） | **我们的选择** |
//!
//! 剩下的 `PRINTABLE` 白名单与 `√ → sqrt(` 替换是**我们自己的选择**（原版没有：它靠
//! `LineEdit` 基类 + 解析器兜底），保留在 Rust 只是为了让规则只有一处；见
//! [`PRINTABLE`] / [`REPLACEMENTS`] 的档位说明。
//!
//! # 接口（全部是**关联函数**，GDScript 直接 `GwFunctionEdit.xxx()`）
//!
//! ```gdscript
//! GwFunctionEdit.apply_command(text, caret, cmd, max_length)  # -> {text, caret, rejected}
//! GwFunctionEdit.key_text(unicode, keycode)                   # -> 要插入的串（"" = 不插入）
//! GwFunctionEdit.provenance()                                 # -> 上表的地址与档位（自检/文档用）
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_function_input::{self as finput, Buffer, KeyAction};

// ---------------------------------------------------------------------------
// 1. 命令字面量（观测：`.tscn` 的 `GraphwarButton.input_text`）
// ---------------------------------------------------------------------------

/// 屏上键盘的命令字面量：**恢复出的 `.tscn` 里 `GraphwarButton.input_text` 的原值**
/// （`docs/92` §4.10 的 18+18 个键；与 `client/src/app/commands.gd` 同一份清单）。
///
/// 档位：**观测**（字面量逐字节来自 `.tscn`）。这里的**语义**（哪条命令做什么编辑）
/// 见文件头表：`⌫`/`←`/`→` 有地址，`C`/`⌦`/`⇤`/`⇥` 是推断。
const CMD_CLEAR: &str = "C";
const CMD_BACKSPACE: &str = "⌫";
const CMD_DELETE: &str = "⌦";
const CMD_LEFT: &str = "←";
const CMD_RIGHT: &str = "→";
const CMD_HOME: &str = "⇤";
const CMD_END: &str = "⇥";

// ---------------------------------------------------------------------------
// 2. 按键 → 字符（原版 28 折叠 + 我们的可打印集）
// ---------------------------------------------------------------------------

/// **我们自己的**可插入字符集（大写先折成小写）。
///
/// ⚠️ **这不是原版的规则**：原版把 ASCII 交给基类 `LineEdit`，**不过滤**；文法检查在
/// 解析器（`MathFunction::new`，`0x4ed670`）里做。这条白名单是我方为了让自绘控件
/// 行为接近 `LineEdit` 而选的（`client/tools/selftest.gd` 已经按它写了断言：
/// 空格不插入、大写折小写）。保留在 Rust 只是为了让"一条规则只有一处"。
///
/// 档位：**本项目选择**（不是复原结论）。
const PRINTABLE: &str = "0123456789.()+-*/^abcdefghijklmnopqrstuvwxyzπ√";

/// **我们自己的**多字符替换：`√ → sqrt(`。
///
/// 依据只有"屏上键 `√` 的 `input_text` 就是 `sqrt(`"（`docs/92` §4.10，观测）；
/// 物理键盘敲 `√` 时原版会插入裸 `√` 再被解析器跳过（`lex` 不认它）——**没有观测**。
/// 档位：**本项目选择**。
const REPLACEMENTS: [(&str, &str); 1] = [("√", "sqrt(")];

/// 一个 `char` 按键 → 要插入的串（空串 = 不插入）。
///
/// 顺序：**先**走原版折叠表（[`finput::classify_key`]，`0x1c3d5c` 跳转表，观测），
/// **再**走我们的白名单/替换（本项目选择）。
///
/// `'^'` 那条在 [`finput::classify_key`] 里有个**语义未知**的附加条件
/// （`0x4fee94`/`0x4feea5`/`0x4feee3` 三次引擎属性读取）；这里按"条件不成立"传
/// `caret_condition = false`（与 `graphwar_function_input_class.rs` 的默认值一致）。
fn key_text_impl(unicode: u32, keycode: i64) -> String {
    // `unicode > 0` 优先：它已经是"按当前键盘布局 + Shift 换算过"的字符。
    let ch = if unicode > 0 {
        char::from_u32(unicode)
    } else if (32..=126).contains(&keycode) {
        char::from_u32(keycode as u32)
    } else {
        None
    };
    let Some(ch) = ch else { return String::new() };
    // ① 原版 `gui_input`：折叠表命中就插入折出来的 ASCII，否则交给基类（这里等价于"原样可插"）。
    let folded = match finput::classify_key(ch, false) {
        KeyAction::Insert(c) => c,
        KeyAction::Delegated => ch,
    };
    // ② 我们的选择：多字符替换 → 白名单（大写折小写）。
    let folded_str = folded.to_string();
    for (from, to) in REPLACEMENTS {
        if folded_str == from {
            return to.to_string();
        }
    }
    let lower = folded.to_lowercase().next().unwrap_or(folded);
    if PRINTABLE.contains(lower) {
        lower.to_string()
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// 3. 一条编辑命令 → 一次 `Buffer` 操作（规则本体）
// ---------------------------------------------------------------------------

/// `apply_command` 的纯逻辑部分：`(新文本, 新光标, 是否被长度上限拒绝)`。
///
/// **不碰引擎**（可 `cargo test` 直接测）；gdext 外壳只做 `GString`/`Dictionary` 搬运。
fn apply_command_impl(text: &str, caret: i64, command: &str, max_length: i64) -> (String, i64, bool) {
    let mut buf = Buffer::new(text);
    // `Buffer::new` 光标在 0；`.gd` 侧的光标是 char 下标，右移到位（夹到末尾）。
    buf.move_caret_right(caret.max(0) as usize);
    match command {
        CMD_CLEAR => {
            buf = Buffer::new("");
        }
        CMD_BACKSPACE => {
            buf.delete_chars_at_caret(1);
        }
        CMD_DELETE => {
            // 右删 = 跨过那个字符再左删（`LineEdit` 只有左删 API）。已在末尾 ⇒ 什么都不做。
            if buf.caret() < buf.len() {
                buf.move_caret_right(1);
                buf.delete_chars_at_caret(1);
            }
        }
        CMD_LEFT => buf.move_caret_left(1),
        CMD_RIGHT => buf.move_caret_right(1),
        CMD_HOME => buf.move_caret_home(),
        CMD_END => buf.move_caret_end(),
        _ => {
            // 其余：按字面插入（`insert_text_at_caret`，原样）。
            // 长度上限是**我们的值**（`MAX_LENGTH`，由 `.gd` 传进来；原版上限未观测）。
            let n: i64 = command.chars().count() as i64;
            if max_length > 0 && buf.len() as i64 + n > max_length {
                return (buf.text(), buf.caret() as i64, true);
            }
            buf.insert_text_at_caret(command);
        }
    }
    (buf.text(), buf.caret() as i64, false)
}

// ---------------------------------------------------------------------------
// 4. GDExtension 外壳
// ---------------------------------------------------------------------------

/// 函数输入框的**规则出口**（scaffold；原版这些规则在 `GraphwarFunctionInput` 自己身上）。
///
/// 所有方法都是**关联函数**（无 `self`）：GDScript 侧 `GwFunctionEdit.apply_command(...)`。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwFunctionEdit {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwFunctionEdit {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwFunctionEdit {
    /// 一条命令落到文本上：**编辑规则本体**（`client/src/play/function_input.gd::key_input`
    /// 曾经用 8 个 `match` 分支写的那些判断）。
    ///
    /// 参数：`text` 当前文本、`caret` 当前光标（char 下标）、`command` 命令字面量
    /// （屏上键的 `input_text` 或物理键折出来的同名字符串）、`max_length` 长度上限
    /// （**我方选择**，传 `0`/负数 = 不限制）。
    ///
    /// 返回 `{text: String, caret: int, rejected: bool}`；`rejected = true` 时文本/光标不变
    /// （只有"插入会超上限"这一条会拒绝）。规则与档位见文件头表。
    #[func]
    fn apply_command(text: GString, caret: i64, command: GString, max_length: i64) -> VarDictionary {
        let (new_text, new_caret, rejected) =
            apply_command_impl(&text.to_string(), caret, &command.to_string(), max_length);
        let mut d = VarDictionary::new();
        d.set("text", &GString::from(new_text.as_str()));
        d.set("caret", new_caret);
        d.set("rejected", rejected);
        d
    }

    /// 一次按键 → 要插入的串（`""` = 这个键不归输入框管）。
    ///
    /// `unicode` 是 `InputEventKey.unicode`（0 = 没有）；`keycode` 在 `unicode == 0` 时兜底
    /// （Godot 的字母/数字/标点 Key 枚举值就是 ASCII）。
    ///
    /// 顺序：原版 28 条折叠表（[`finput::COMPOSED_FOLDS`]，观测）→ 我们的白名单/替换。
    #[func]
    fn key_text(unicode: i64, keycode: i64) -> GString {
        let u = if unicode > 0 { unicode as u32 } else { 0 };
        GString::from(key_text_impl(u, keycode).as_str())
    }

    /// 出处自检：每条规则的原版地址 + 档位（客户端界面不读它；自检/文档复核用）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("move_caret_left", &GString::from("0x4f3210 GraphwarFunctionInput::move_caret_left"));
        d.set(
            "move_caret_right",
            &GString::from("0x4f3910 GraphwarFunctionInput::move_caret_right"),
        );
        d.set(
            "delete_token_at_caret",
            &GString::from("0x4f44c0（内部调 delete_char_at_caret，字面量 0x1c44a2/长 20）"),
        );
        d.set(
            "delete_token_after_caret",
            &GString::from("0x4f4a60（同样落到 delete_char_at_caret）"),
        );
        d.set(
            "composed_fold_table",
            &GString::from("0x1c3d5c 跳转表 198 项 → 28 条折叠（gui_input 0x4feee3…0x4ff1e1）"),
        );
        d.set("composed_fold_count", finput::COMPOSED_FOLDS.len() as i64);
        d.set(
            "strip_spaces",
            &GString::from("0x4f28be…0x4f28dc str::replace(' ',\"\")（force_update_cb 0x4f2200）"),
        );
        d.set(
            "grade",
            &GString::from(
                "⌫/←/→/插入 = 观测；C/⌦/⇤/⇥ = 推断；PRINTABLE/REPLACEMENTS/长度上限 = 本项目选择",
            ),
        );
        d
    }
}

// ---------------------------------------------------------------------------
// 5. 单测（纯逻辑，不碰引擎）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_function_input::COMPOSED_FOLDS;

    /// 期望值构造器：`(文本, 光标, 是否拒绝)` —— 省掉每个断言里的 `String::from`。
    fn r(text: &str, caret: i64, rejected: bool) -> (String, i64, bool) {
        (text.to_string(), caret, rejected)
    }

    /// 插入：光标在中间，插完光标落在插入内容之后（原版 `insert_text_at_caret` 的语义）。
    #[test]
    fn insert_goes_at_caret_and_moves_it() {
        assert_eq!(apply_command_impl("sin(x)", 4, "(", 128), r("sin((x)", 5, false));
        // 多字符插入（屏上键 `sqrt(` / `sin(`）同样原样插、光标跨过整段。
        assert_eq!(apply_command_impl("x", 0, "sqrt(", 128), r("sqrt(x", 5, false));
    }

    /// 左删 / 右删：`⌫` 删光标左，`⌦` 跨过去再删（且**在末尾时什么都不做**）。
    ///
    /// 右删是**推断**（`LineEdit` 只有左删 API）：跨过去 + 左删，光标回到原处。
    #[test]
    fn backspace_and_delete_are_directional() {
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_BACKSPACE, 128), r("sinx)", 3, false));
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_DELETE, 128), r("sin()", 4, false));
        // 光标在末尾：右删不动（否则会误删左边那个字符）。
        assert_eq!(apply_command_impl("sin(x)", 6, CMD_DELETE, 128), r("sin(x)", 6, false));
        // 光标在开头：左删不动。
        assert_eq!(apply_command_impl("sin(x)", 0, CMD_BACKSPACE, 128), r("sin(x)", 0, false));
    }

    /// 光标四键：`C` 清空、`←/→` 各一步、`⇤/⇥` 两端。
    #[test]
    fn caret_and_clear_commands() {
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_CLEAR, 128), r("", 0, false));
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_LEFT, 128), r("sin(x)", 3, false));
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_RIGHT, 128), r("sin(x)", 5, false));
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_HOME, 128), r("sin(x)", 0, false));
        assert_eq!(apply_command_impl("sin(x)", 4, CMD_END, 128), r("sin(x)", 6, false));
    }

    /// 长度上限（**我方值**）：插入会超上限 ⇒ `rejected = true` 且文本不动。
    #[test]
    fn max_length_rejects_insert_only() {
        assert_eq!(apply_command_impl("abc", 3, "d", 3), r("abc", 3, true));
        // 上限只管插入：编辑命令从来不被拒。
        assert_eq!(apply_command_impl("abc", 3, CMD_BACKSPACE, 3), r("ab", 2, false));
        // 0/负 = 不限制。
        assert_eq!(apply_command_impl("abc", 3, "d", 0), r("abcd", 4, false));
    }

    /// **原版折叠表真的被用上了**（改前 `.gd` 没有这一步）：28 条全都折成 ASCII 后进白名单。
    #[test]
    fn every_composed_key_folds_to_ascii() {
        assert_eq!(COMPOSED_FOLDS.len(), 28);
        for (from, to) in COMPOSED_FOLDS {
            let got = key_text_impl(from as u32, 0);
            assert_eq!(got, to.to_lowercase().to_string(), "折叠 {from:?} → 期望 {to:?}");
        }
        // 折叠后的字符必须落进白名单（否则折叠白做）—— `·` 折成 `.` 这条也一样。
        for (_, to) in COMPOSED_FOLDS {
            assert!(PRINTABLE.contains(to.to_ascii_lowercase()), "{to:?} 不在白名单");
        }
    }

    /// ASCII / `π` / `√` 的行为与改前 `.gd` **逐条相同**（`selftest.gd` 的 9 条断言口径）。
    #[test]
    fn ascii_pi_and_sqrt_keep_previous_behaviour() {
        assert_eq!(key_text_impl('S' as u32, 0), "s"); // 大写折小写（我们的选择）
        assert_eq!(key_text_impl('(' as u32, 0), "(");
        assert_eq!(key_text_impl(0x3C0, 0), "π"); // π 原样（解析器再转 n）
        assert_eq!(key_text_impl('√' as u32, 0), "sqrt("); // 我们的替换
        assert_eq!(key_text_impl(' ' as u32, 0), ""); // 空格不插入（我们的白名单）
        assert_eq!(key_text_impl(0, 83), "s"); // unicode 缺失 ⇒ 按 keycode 兜底
        assert_eq!(key_text_impl(0, 4194308), ""); // 功能键没有 unicode ⇒ 不插入
    }
}
