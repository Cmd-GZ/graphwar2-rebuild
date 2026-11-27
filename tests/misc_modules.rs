//! 最后一批杂项小模块的验收（`docs/16-client.md` §33）。
//!
//! 这 11 个模块**还没有**在 `src/lib.rs` 里 `pub mod`（父 agent 待加，见 §33.11），
//! 所以本文件用 `#[path]` 直接编译源码，并把它们依赖的 `crate::` 项在测试 crate
//! 根上重导出 —— 这样**不接线也能跑**，接线之后同一份断言仍然成立。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 条：这些断言是**回归保护**，不是复原证据。
//! 复原证据是 `docs/16-client.md` §33 里逐条标的地址与字节。

// ---- 让 `#[path]` 进来的源码能解析 `crate::…` ----
pub use gw2_rebuild::{Function, Token, E, PI};

pub mod events {
    pub use gw2_rebuild::events::{Event, GameInfo, GAME_INFO_FIELD_COUNT};
}

#[path = "../src/function_token.rs"]
pub mod function_token;
#[path = "../src/polish_notation_converter.rs"]
pub mod polish_notation_converter;
#[path = "../src/graphwar_mode_manager.rs"]
pub mod graphwar_mode_manager;
#[path = "../src/graphwar_event_queue.rs"]
pub mod graphwar_event_queue;
#[path = "../src/graphwar_crash_reporter.rs"]
pub mod graphwar_crash_reporter;
#[path = "../src/backtracer.rs"]
pub mod backtracer;
#[path = "../src/graphwar_logger.rs"]
pub mod graphwar_logger;
#[path = "../src/graphwar_draw_util.rs"]
pub mod graphwar_draw_util;
#[path = "../src/graphwar_player_config.rs"]
pub mod graphwar_player_config;
#[path = "../src/graphwar_campaign_control.rs"]
pub mod graphwar_campaign_control;
#[path = "../src/graphwar_game_info.rs"]
pub mod graphwar_game_info;

// ===========================================================================
// §33.10 function_token：字符串 → token
// ===========================================================================
mod function_token_tests {
    use super::function_token::{self as ft, FunctionToken};
    use super::{Function, Token, E, PI};

    fn tok(s: &str) -> FunctionToken {
        ft::new(s).unwrap_or_else(|| panic!("{s:?} 应该能切出 token"))
    }

    #[test]
    fn regex_pattern_is_the_byte_exact_original() {
        // .rodata arm64 0x19dcb4 / x86 0x1c4347，两份逐字节相同（77 字节）
        assert_eq!(ft::TOKEN_PATTERN.len(), 77);
        assert!(ft::TOKEN_PATTERN.starts_with(r"[0-9]*\.?[0-9]+|"));
        assert!(ft::TOKEN_PATTERN.ends_with("|exp|e"));
        // 交替顺序即优先级：y' 必须在 y 之前、exp 必须在 e 之前
        let yp = ft::TOKEN_PATTERN.find("y'").unwrap();
        let y = ft::TOKEN_PATTERN.find("|y|").unwrap();
        assert!(yp < y, "`y'` 必须在 `y` 之前，否则会被切成 y + 残渣");
        let exp = ft::TOKEN_PATTERN.find("|exp|").unwrap();
        let e = ft::TOKEN_PATTERN.rfind("|e").unwrap();
        assert!(exp < e);
    }

    #[test]
    fn numbers_win_over_symbol_table() {
        assert_eq!(tok("0.5"), Token::constant(0.5));
        assert_eq!(tok("7"), Token::constant(7.0));
    }

    #[test]
    fn brackets_are_18_and_19() {
        assert_eq!(tok("(").tag as u64, 18);
        assert_eq!(tok(")").tag as u64, 19);
        assert_eq!(tok("(").tag, Function::LeftBracket);
        assert_eq!(tok(")").tag, Function::RightBracket);
    }

    #[test]
    fn every_operator_and_function_maps_like_the_jump_table() {
        let cases = [
            ("x", Function::VarX),
            ("y", Function::VarY),
            ("y'", Function::VarDy),
            ("+", Function::Add),
            ("-", Function::Sub),
            ("*", Function::Mul),
            ("/", Function::Divide),
            ("^", Function::Pow),
            ("sqrt", Function::Sqrt),
            ("log", Function::Log),
            ("ln", Function::Ln),
            ("exp", Function::Exp),
            ("abs", Function::Abs),
            ("sin", Function::Sin),
            ("cos", Function::Cos),
            ("tan", Function::Tan),
        ];
        for (s, f) in cases {
            assert_eq!(tok(s), Token::op(f), "{s:?}");
        }
    }

    #[test]
    fn n_and_e_are_pi_and_e() {
        assert_eq!(tok("n"), Token::constant(PI));
        assert_eq!(tok("e"), Token::constant(E));
        // "pi" 在 MathFunction::new 里已被换成 "n"，但这一臂原版仍在（死臂），行为一致
        assert_eq!(tok(ft::PI_ALIAS_UNREACHABLE), Token::constant(PI));
    }

    #[test]
    fn caret_alias_is_tag_8() {
        assert_eq!(ft::new_with_caret_alias("\u{02c6}"), Some(Token::op(Function::Pow)));
        // 普通 ^ 也还是 8
        assert_eq!(ft::new_with_caret_alias("^"), Some(Token::op(Function::Pow)));
    }

    #[test]
    fn unrecognised_fragment_has_no_token() {
        // 原版返回 tag 20；本 crate 的 Function 只到 19，所以映射成 None
        assert_eq!(ft::new("%"), None);
        assert_eq!(ft::new("xyz"), None);
    }
}

// ===========================================================================
// §33.10 polish_notation_converter：中缀 → 前缀
// ===========================================================================
mod polish_tests {
    //! `polish_notation_converter` 的 `reorder_recursive`：**真值全部来自 unicorn 直调**。
    //!
    //! 真值来源（`fixtures/test_expectation_plan3.tsv` 的 M01–M16）：
    //! `tools/gen_test_expectations3.py --section reorder` 把**每一行测试里那份中缀 token 序列**
    //! 直接喂原版 `PolishNotationConverter::reorder_recursive`（ELF vaddr **`0x4f5450`**，
    //! 见 `src/polish_notation_converter.rs` 文件头），把返回的前缀 token 记进
    //! `fixtures/test_expectation_truth3.tsv`（`M/M0x/expected`）。
    //!
    //! ⚠️ 两条口径：
    //! * 只钉 `tag` 序列 + `CONST` 的 `value` **位型**：真函数只给 CONST 写 value，
    //!   非 CONST token 的 value 槽是堆上残留（实测，见脚本文件头口径第 1 条）。
    //! * **不能**改用 `MathFunction::new 0x4ed670` 的输出当真值：那条路径是「文本 → 分词 → reorder」，
    //!   多了一趟分词（`-x` 在解析器路径上分词成 tag 5 `UnaryMinus`，而这里的输入是
    //!   `ft::new("-")` = tag 4 `Sub`）——两份口径不同，混用会把分词器的行为记成转换器的行为。

    use super::function_token as ft;
    use super::polish_notation_converter::{self as pnc, PolishNotationConverter, PRECEDENCE};
    use super::{Function, Token};

    fn infix(s: &[&str]) -> Vec<Token> {
        s.iter().map(|t| ft::new(t).expect("token")).collect()
    }

    fn to_prefix(s: &[&str]) -> Vec<Token> {
        PolishNotationConverter::reorder(infix(s))
    }

    fn op(f: Function) -> Token {
        Token::op(f)
    }

    fn c(v: f64) -> Token {
        Token::constant(v)
    }

    /// 与我方输出逐 token 对拍**真二进制 `0x4f5450` 的结果**。
    ///
    /// `real` 每一对是 `(tag, CONST 的 value)`；非 CONST 的 value 不参与比较
    /// （真侧那一位是堆残留）。
    fn assert_reorder(infix_tokens: &[&str], real: &[(Function, f64)]) {
        let got = to_prefix(infix_tokens);
        assert_eq!(
            got.len(),
            real.len(),
            "reorder({infix_tokens:?}) 的 token 个数与真二进制 0x4f5450 不一致"
        );
        for (i, (g, (tag, val))) in got.iter().zip(real).enumerate() {
            assert_eq!(
                g.tag, *tag,
                "reorder({infix_tokens:?}) 第 {i} 个 token 的 tag 与真二进制 0x4f5450 不一致"
            );
            if *tag == Function::Const {
                assert_eq!(
                    g.value.to_bits(),
                    val.to_bits(),
                    "reorder({infix_tokens:?}) 第 {i} 个 CONST 的 value 位型与真二进制不一致"
                );
            }
        }
    }

    #[test]
    fn empty_input_stays_empty() {
        // 真值 M01：真 reorder 拿到空 Vec 时输出也是空（`len==0` 连 call 都不做）
        assert!(pnc::PolishNotationConverter::reorder(vec![]).is_empty());
    }

    #[test]
    fn single_atom_passes_through() {
        // 真值 M02：[VarX] → [VarX]
        assert_reorder(&["x"], &[(Function::VarX, 0.0)]);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        // 真值 M03：+ 1 * 2 3
        assert_reorder(
            &["1", "+", "2", "*", "3"],
            &[
                (Function::Add, 0.0),
                (Function::Const, 1.0),
                (Function::Mul, 0.0),
                (Function::Const, 2.0),
                (Function::Const, 3.0),
            ],
        );
    }

    #[test]
    fn brackets_change_the_root() {
        // 真值 M04：* + 1 2 3
        assert_reorder(
            &["(", "1", "+", "2", ")", "*", "3"],
            &[
                (Function::Mul, 0.0),
                (Function::Add, 0.0),
                (Function::Const, 1.0),
                (Function::Const, 2.0),
                (Function::Const, 3.0),
            ],
        );
    }

    #[test]
    fn brackets_never_reach_the_output() {
        // 真值 M05：真 reorder 的 5 个输出 token 里没有 18/19
        let out = to_prefix(&["(", "1", "+", "2", ")", "*", "3"]);
        assert_eq!(out.len(), 5, "真值 M05：输出 5 个 token");
        assert!(!out
            .iter()
            .any(|t| matches!(t.tag, Function::LeftBracket | Function::RightBracket)));
    }

    #[test]
    fn subtraction_is_left_associative() {
        // 真值 M06：- - 1 2 3 （平级取后出现的当根 = 左结合）
        assert_reorder(
            &["1", "-", "2", "-", "3"],
            &[
                (Function::Sub, 0.0),
                (Function::Sub, 0.0),
                (Function::Const, 1.0),
                (Function::Const, 2.0),
                (Function::Const, 3.0),
            ],
        );
    }

    #[test]
    fn pow_is_left_associative() {
        // 真值 M07：^ ^ 2 3 2（docs/03 的「`^` 左结合」机制就在这条判据里）
        assert_reorder(
            &["2", "^", "3", "^", "2"],
            &[
                (Function::Pow, 0.0),
                (Function::Pow, 0.0),
                (Function::Const, 2.0),
                (Function::Const, 3.0),
                (Function::Const, 2.0),
            ],
        );
    }

    #[test]
    fn mixed_same_precedence_is_left_associative() {
        // 真值 M08：* / 1 2 3
        assert_reorder(
            &["1", "/", "2", "*", "3"],
            &[
                (Function::Mul, 0.0),
                (Function::Divide, 0.0),
                (Function::Const, 1.0),
                (Function::Const, 2.0),
                (Function::Const, 3.0),
            ],
        );
    }

    #[test]
    fn unary_function_wraps_everything_to_its_right() {
        // 真值 M09：sin x（括号在转换器里被消化掉）
        assert_reorder(
            &["sin", "(", "x", ")"],
            &[(Function::Sin, 0.0), (Function::VarX, 0.0)],
        );
    }

    #[test]
    fn unary_functions_are_right_associative() {
        // 真值 M10：sin cos x（先出现的当根 = 右结合）
        assert_reorder(
            &["sin", "cos", "x"],
            &[
                (Function::Sin, 0.0),
                (Function::Cos, 0.0),
                (Function::VarX, 0.0),
            ],
        );
    }

    /// ⚠️ 这条的名称有歧义，**读清楚再改**：
    ///
    /// * `FunctionToken::new("-")`（跳跃表 `0x1c3d5c` 那条）给的确实是**二元减 tag 4**
    ///   ⇒ 「**词法器不产 tag 5**」这句只说对了一半；
    /// * 但 `MathFunction::new 0x4ed670` 这条**完整路径**上，前缀位置的 `-` 最终是
    ///   **tag 5 `UnaryMinus`**（`fixtures/parse.tsv` 的真值：`-` → `[5]`、`--x` → `[5,5,0]`、
    ///   `-x^2` → `[5,8,0,17:2.0]`，全部 unicorn 直调产物）⇒ 产生点在**分词那一趟**，
    ///   不在转换器里（`src/polish_notation_converter.rs` 文件头说「产生点未定位」，本轮把它定位到
    ///   分词层，见 `docs/06` §112.5）。
    /// * **本测试钉的是转换器**：拿 tag 4 的中缀输入喂 `0x4f5450`，真值 M11 = `[Sub, CONST0, VarX]`
    ///   （补洞的是转换器，不是分词器）。
    #[test]
    fn tokenizer_never_produces_unary_minus() {
        assert_eq!(ft::new("-").unwrap().tag, Function::Sub);
        assert_ne!(ft::new("-").unwrap().tag, Function::UnaryMinus);
        // 真值 M11：4 17:0 0
        assert_reorder(
            &["-", "x"],
            &[
                (Function::Sub, 0.0),
                (Function::Const, 0.0),
                (Function::VarX, 0.0),
            ],
        );
    }

    #[test]
    fn unary_minus_token_if_present_binds_looser_than_pow() {
        // 词法器不产 tag 5（见上一条），但转换器**认识**它：
        // 一元（`neg` = 5，或 `sqrt..tan` = 9..=16）走"不递归、接着扫右边"那条路。
        //
        // 真值 M12：把 `[UnaryMinus, VarX, Pow, Const(2)]` 这份**中缀** token 喂 `0x4f5450`
        // ⇒ `[UnaryMinus, Pow, VarX, Const(2)]`（即 `-(x^2)`：tag 5 比 `^` 松）。
        let input = vec![op(Function::UnaryMinus), op(Function::VarX), op(Function::Pow), c(2.0)];
        assert_eq!(
            PolishNotationConverter::reorder(input),
            vec![
                op(Function::UnaryMinus),
                op(Function::Pow),
                op(Function::VarX),
                c(2.0)
            ]
        );
    }

    #[test]
    fn missing_right_operand_of_plus_becomes_const_zero() {
        // 真值 M13：[Const1, Add] → `+ 1 CONST0`（补洞在转换器里）
        assert_reorder(
            &["1", "+"],
            &[
                (Function::Add, 0.0),
                (Function::Const, 1.0),
                (Function::Const, 0.0),
            ],
        );
    }

    #[test]
    fn missing_left_operand_of_minus_becomes_const_zero() {
        // 真值 M14：[Add, Const1] → `+ CONST0 1`
        assert_reorder(
            &["+", "1"],
            &[
                (Function::Add, 0.0),
                (Function::Const, 0.0),
                (Function::Const, 1.0),
            ],
        );
    }

    #[test]
    fn only_plus_and_minus_fill_holes() {
        // 真值 M15：`*` 打头**不补** 0（输出 2 个 token：Mul, Const2）
        assert_reorder(&["*", "2"], &[(Function::Mul, 0.0), (Function::Const, 2.0)]);
        // 正对照：同一个位置换成 `+` 就补（真值 M14 已给），这里只钉"不补"
        assert_eq!(to_prefix(&["*", "2"]).len(), 2, "真值 M15：只有 2 个 token");
    }

    #[test]
    fn precedence_table_is_the_binary_copy() {
        // .rodata 0x1c4838 / 0x1c4888 两份拷贝，值逐项相同
        assert_eq!(
            &PRECEDENCE[..18],
            &[2, 2, 2, 7, 7, 4, 6, 6, 4, 3, 3, 3, 3, 3, 3, 3, 3, 2]
        );
        // 数值越大 = 结合越松
        assert!(PRECEDENCE[Function::Add as usize] > PRECEDENCE[Function::Mul as usize]);
        assert!(PRECEDENCE[Function::Mul as usize] > PRECEDENCE[Function::Pow as usize]);
        assert!(PRECEDENCE[Function::Pow as usize] > PRECEDENCE[Function::Sin as usize]);
        assert!(PRECEDENCE[Function::Sin as usize] > PRECEDENCE[Function::VarX as usize]);
    }

    #[test]
    fn masks_are_the_three_bt_constants() {
        assert_eq!(pnc::MASK_NOT_BINARY, 0x3fe27);
        assert_eq!(pnc::MASK_NOT_BINARY_OR_BRACKET, 0xffe27);
        assert_eq!(pnc::MASK_LEAF, 0xe0007);
        // 掩码 1：二元操作符不在里面
        for f in [
            Function::Add,
            Function::Sub,
            Function::Mul,
            Function::Divide,
            Function::Pow,
        ] {
            assert_eq!(pnc::MASK_NOT_BINARY & (1u64 << (f as u64)), 0, "{f:?}");
        }
        // 一元与原子在里面
        for f in [Function::VarX, Function::UnaryMinus, Function::Sin, Function::Const] {
            assert_ne!(pnc::MASK_NOT_BINARY & (1u64 << (f as u64)), 0, "{f:?}");
        }
        // 掩码 2 = 掩码 1 + 括号
        assert_ne!(
            pnc::MASK_NOT_BINARY_OR_BRACKET & (1u64 << (Function::LeftBracket as u64)),
            0
        );
    }

    #[test]
    fn deep_nesting_still_reaches_atoms() {
        // 真值 M16（顺带）：`((x))` → `[VarX]`（真 reorder 输出 1 个 token）
        assert_reorder(&["(", "(", "x", ")", ")"], &[(Function::VarX, 0.0)]);
        // sin(cos(tan(x))) → sin cos tan x
        assert_reorder(
            &["sin", "(", "cos", "(", "tan", "(", "x", ")", ")", ")"],
            &[
                (Function::Sin, 0.0),
                (Function::Cos, 0.0),
                (Function::Tan, 0.0),
                (Function::VarX, 0.0),
            ],
        );
    }
}

// ===========================================================================
// §33.4 mode_manager
// ===========================================================================
mod mode_manager_tests {
    use super::graphwar_mode_manager::*;

    #[test]
    fn discriminants_are_the_wire_values() {
        assert_eq!(AxisMode::NoAxis as u8, 0);
        assert_eq!(AxisMode::OnlyMain as u8, 1);
        assert_eq!(AxisMode::EveryFive as u8, 2);
        assert_eq!(AxisMode::EveryUnit as u8, 3);
        assert_eq!(FunctionMode::NormalFunction as u8, 0);
        assert_eq!(FunctionMode::DiffEqFunction as u8, 1);
        assert_eq!(TimeMode::Timer30s as u8, 0);
        assert_eq!(TimeMode::TimerInf as u8, 5);
    }

    #[test]
    fn time_mode_seconds_and_icons() {
        assert_eq!(TimeMode::Timer30s.seconds(), Some(30));
        assert_eq!(TimeMode::Timer5m.seconds(), Some(300));
        assert_eq!(TimeMode::TimerInf.seconds(), None);
        assert_eq!(TimeMode::TimerInf.icon(), "res://turn_inf.svg");
        assert_eq!(TimeMode::Timer1m.icon(), "res://turn_1m.svg");
    }

    #[test]
    fn setters_return_the_event_to_broadcast() {
        let mut m = GraphwarModeManager::default();
        assert_eq!(m.set_axis_mode(AxisMode::EveryUnit), ModeInfoEvent::AxisModeInfo(AxisMode::EveryUnit));
        assert_eq!(m.axis_mode, AxisMode::EveryUnit);
        assert_eq!(
            m.set_function_mode(FunctionMode::DiffEqFunction),
            ModeInfoEvent::FunctionModeInfo(FunctionMode::DiffEqFunction)
        );
        assert_eq!(m.set_time_mode(TimeMode::Timer3m), ModeInfoEvent::TimeModeInfo(TimeMode::Timer3m));
    }

    #[test]
    fn send_all_info_events_is_three_and_in_order() {
        let m = GraphwarModeManager::new(AxisMode::OnlyMain, FunctionMode::NormalFunction, TimeMode::Timer2m);
        let ev = m.send_all_info_events();
        assert_eq!(ev.len(), 3);
        assert!(matches!(ev[0], ModeInfoEvent::AxisModeInfo(_)));
        assert!(matches!(ev[1], ModeInfoEvent::FunctionModeInfo(_)));
        // TurnMode 不在这个管理器里 —— 这是本模块最重要的一条结论
        assert!(matches!(ev[2], ModeInfoEvent::TimeModeInfo(_)));
    }
}

// ===========================================================================
// §33.3 event_queue
// ===========================================================================
/// ## 口径（T73 / 计划 D02）：**契约断言，不含语义**
///
/// 这两条测的是我方 [`GraphwarEventQueue`] 的队列语义（FIFO / 空队列 no-op）。
/// 原版对应物（arm64）是：
/// * `GraphwarEventQueue::add_event` = **`0x3d7a40`**（276 字节）—— 函数体就是
///   `push_back`（`VecDeque::grow` 在调用链里，`graphwar_event_queue.rs` 文件头记了这条）；
/// * `GraphwarEventQueue::send_buffer` = **`0x3d793c`**（260 字节）—— `drain` + 逐条写 socket。
///
/// 但「FIFO 顺序」在**语料/抓包里不可观测**（回放不记队列内部顺序；真流量里
/// 同一帧的多个事件本来就是按队列顺序发出去的，无法反推队列结构），而 `push_back`/`drain`
/// 的语义只有读到 `VecDeque` 的**方法体**才能算逐位 ⇒ 本轮**换不了**。
/// 本测试只证「我方队列不退化成 LIFO/丢事件」，**不构成复原证据**。
mod event_queue_tests {
    use super::events::Event;
    use super::graphwar_event_queue::GraphwarEventQueue;

    #[test]
    fn add_then_drain_is_fifo() {
        let mut q = GraphwarEventQueue::new();
        assert!(q.is_empty());
        q.add_event(Event::TickEvent);
        q.add_event(Event::TickRequest);
        assert_eq!(q.len(), 2);
        let batch = q.send_buffer();
        assert_eq!(batch, vec![Event::TickEvent, Event::TickRequest]);
        assert!(q.is_empty());
    }

    #[test]
    fn send_buffer_on_empty_queue_is_a_noop() {
        let mut q = GraphwarEventQueue::new();
        assert!(q.send_buffer().is_empty());
    }
}

// ===========================================================================
// §33.2 crash_reporter + backtracer
// ===========================================================================
mod crash_tests {
    use super::backtracer::PanicLogHook;
    use super::graphwar_crash_reporter::{self as cr, CrashReportTarget};

    // ===================================================================
    // T73 真值表：`fixtures/test_expectation_truth4.tsv` 的 `R/*` 段
    // （生成器 `tools/gen_test_expectations4.py`，`--check` 可复算）
    // ===================================================================
    const TRUTH4: &str = include_str!("../fixtures/test_expectation_truth4.tsv");

    fn truth4(section: &str, key: &str) -> &'static str {
        for line in TRUTH4.lines() {
            let mut it = line.split('\t');
            if it.next() == Some(section) && it.next() == Some(key) {
                return it.next().unwrap_or("");
            }
        }
        panic!("truth4 里没有 {section}/{key}")
    }

    /// 从 AArch64 `MOVZ Wd, #imm16, LSL #hw<<4` 的 32 位字里解出立即数。
    ///
    /// 编码（ARM ARM C6.2.196）：`sf(1)=0 | opc(2)=10 | 100101(6) | hw(2) | imm16(16) | Rd(5)`。
    fn decode_movz(word: u32) -> Option<u64> {
        // MOVZ 32-bit：bit31..23 = 0b0_10_100101 ⇒ 与 0x7F80_0000 掩码后等于 0x5280_0000
        if word & 0x7F80_0000 != 0x5280_0000 {
            return None;
        }
        let hw = (word >> 21) & 0b11;
        let imm16 = ((word >> 5) & 0xFFFF) as u64;
        Some(imm16 << (hw * 16))
    }

    /// `CRASH_REPORT_PORT` 的真值来自**真二进制指令**，不是我方常量。
    ///
    /// ## 真值来源（T73 / 计划 D01，`逐位`）
    ///
    /// arm64 **Android 2.0.60**（未 strip，`.symtab` 有真名；sha256
    /// `8a612e954a030f86184667e88ec25797c9af538c2ab28b63549fe2b46b9554df`）里
    /// `GraphwarCrashReporter::resolve_default_address`（ELF vaddr **`0x4ca8b4`**，980 字节）
    /// 有两条把端口materialize 的 `mov w8/w10,#imm`：
    /// * `0x4cab30: 5294fb88`
    /// * `0x4cab64: 5294fb8a`
    ///
    /// 本测试把这两个 32 位字**解码**成 MOVZ 立即数，再与 `CRASH_REPORT_PORT` 比 ——
    /// 常量抄错一位（42972 → 42973/49272…）这里立刻红。
    ///
    /// 复跑（取指）：`llvm-objdump -d --start-address=0x4ca8b4 --stop-address=0x4cac8c <arm64 .so>`
    /// 复跑（真值表）：`.reference/venv/bin/python tools/gen_test_expectations4.py --check`
    #[test]
    fn port_is_42972() {
        assert_eq!(cr::CRASH_REPORT_PORT, 42972);
        // 二进制身份：整份 arm64 .so 的 sha256（换构建 ⇒ 立刻发现）
        assert_eq!(truth4("R", "binary_sha256"),
                   "8a612e954a030f86184667e88ec25797c9af538c2ab28b63549fe2b46b9554df");
        assert_eq!(truth4("R", "port_function"),
                   "GraphwarCrashReporter::resolve_default_address");
        assert_eq!(truth4("R", "port_function_addr"), "0x4ca8b4");
        assert_eq!(truth4("R", "port_function_len"), "980");
        let func_addr = u64::from_str_radix(
            truth4("R", "port_function_addr").trim_start_matches("0x"), 16).unwrap();
        let func_len: u64 = truth4("R", "port_function_len").parse().unwrap();

        let words = truth4("R", "port_words");
        let mut decoded = Vec::new();
        for part in words.split(',') {
            let (addr, word) = part.split_once(':').expect("addr:word");
            let addr = u64::from_str_radix(addr.trim_start_matches("0x"), 16).unwrap();
            let word = u32::from_str_radix(word.trim_start_matches("0x"), 16).unwrap();
            assert!((func_addr..func_addr + func_len).contains(&addr),
                    "0x{addr:x} 应落在 resolve_default_address 内");
            let value = decode_movz(word)
                .unwrap_or_else(|| panic!("0x{word:08x} 不是 MOVZ"));
            decoded.push(value);
        }
        assert_eq!(decoded.len(), 2, "函数体里应有两条端口立即数");
        assert_eq!(truth4("R", "port_value"), "42972");
        for (i, v) in decoded.iter().enumerate() {
            assert_eq!(*v, cr::CRASH_REPORT_PORT as u64,
                       "第 {} 条 MOVZ 解出的端口 {v} 与我方常量不一致", i + 1);
        }
        assert_eq!(cr::CRASH_REPORT_PORT as u64, 0xa7dc, "端口位型（mov w8,#0xa7dc）");
    }

    /// ## 口径（T73 / 计划 D03）：下面六条是**推断，不是控制流证据**
    ///
    /// 六条断言的是我方 `resolve_default_address` 的**优先级顺序**（CLI > env > 全局地址 >
    /// 待发）。真值侧的物证只有三样：①`port_is_42972` 钉住的**端口立即数**（两条
    /// `mov w8,#0xa7dc`，`逐位`）；②`.rodata` 里 clap 的三条帮助串与三条失败串；
    /// ③ 函数体里 `reqwest::blocking::get`（拉 `graphwar_ii_ip.txt`）与
    /// `chrono::Utc::now`（时间戳）的调用。**优先级顺序本身**没有逐条读成证据
    /// ⇒ 本组是**推断**，缺的是 `resolve_default_address` 里三条 `if`/`Option::or_else`
    /// 链的逐条控制流（`0x4ca8b4..0x4cac88` 只读到了两条端口立即数附近的片段）。
    #[test]
    fn cli_wins_over_env_and_global() {
        assert_eq!(
            cr::resolve_default_address(Some("1.2.3.4:9000"), Some("5.6.7.8:9001"), Some("9.9.9.9")),
            CrashReportTarget::Address("1.2.3.4:9000".into())
        );
    }

    #[test]
    fn env_is_second() {
        assert_eq!(
            cr::resolve_default_address(None, Some("5.6.7.8:9001"), Some("9.9.9.9")),
            CrashReportTarget::Address("5.6.7.8:9001".into())
        );
    }

    #[test]
    fn global_address_gets_the_crash_port() {
        // 帮助文本：the game's global server address is reused with CRASH_REPORT_PORT
        assert_eq!(
            cr::resolve_default_address(None, None, Some("9.9.9.9")),
            CrashReportTarget::Address("9.9.9.9:42972".into())
        );
    }

    #[test]
    fn nothing_available_means_pending() {
        assert_eq!(
            cr::resolve_default_address(None, None, None),
            CrashReportTarget::PendingWithoutAddress
        );
        assert_eq!(
            cr::resolve_default_address(Some("  "), Some(""), None),
            CrashReportTarget::PendingWithoutAddress
        );
    }

    #[test]
    fn address_without_port_gets_the_default() {
        assert_eq!(
            cr::resolve_default_address(Some("127.0.0.1"), None, None),
            CrashReportTarget::Address("127.0.0.1:42972".into())
        );
    }

    #[test]
    fn messages_are_the_ones_in_rodata() {
        assert!(cr::MESSAGES
            .contains(&"Crash report pending but no crash report server address is available."));
        assert!(cr::MESSAGES.contains(&"Crash report sent to "));
        assert_eq!(cr::CRASH_LOG_PATH, "user://crash_log.txt");
        assert_eq!(cr::TIMESTAMP_FORMAT, "%Y-%m-%d %H:%M:%S");
    }

    #[test]
    fn hook_renders_all_four_segments() {
        let hook = PanicLogHook::default();
        let line = hook.render("linux", "src/x.rs:1:1", "boom", "2026-11-07 03:00:00");
        assert!(line.contains("2026-11-07 03:00:00"));
        assert!(line.contains("linux"));
        assert!(line.contains("src/x.rs:1:1"));
        assert!(line.contains("boom"));
    }
}

// ===========================================================================
// §33.5 logger / §33.6 draw_util
// ===========================================================================
mod logger_draw_tests {
    use super::graphwar_draw_util::{get_team_overview, TeamOverviewEntry};
    use super::graphwar_logger::GraphwarLogger;

    #[test]
    fn logger_queues_then_serves() {
        let mut l = GraphwarLogger::new(0);
        assert_eq!(l.pending(), 0);
        l.log("a");
        l.log("b");
        assert_eq!(l.pending(), 2);
        assert_eq!(l.process(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(l.pending(), 0);
    }

    #[test]
    fn team_overview_maps_team_to_color_index() {
        // team_colors = [1, 0]：先登记 Team2 再登记 Team1
        let got = get_team_overview(&[(10, 0), (11, 1)], &[1, 0]);
        assert_eq!(
            got,
            vec![
                TeamOverviewEntry { player_id: 10, team: 0, color_index: 1 },
                TeamOverviewEntry { player_id: 11, team: 1, color_index: 0 },
            ]
        );
    }
}

// ===========================================================================
// §33.7 player_config / §33.8 campaign_control / §33.9 game_info
// ===========================================================================
mod config_campaign_tests {
    use super::graphwar_campaign_control::{get_level_as_int, CampaignProgress, CAMPAIGN_LEVEL_COUNT};
    use super::graphwar_game_info::{GAME_INFO_FIELD_COUNT, GAME_INFO_FIELDS, GAME_INFO_EVENT_TAG};
    use super::graphwar_player_config::GraphwarPlayerConfig;

    #[test]
    fn setters_bump_the_change_counter() {
        let mut cfg = GraphwarPlayerConfig::default();
        cfg.set_skin("blue");
        cfg.set_face("cat_eyes");
        cfg.set_hat("crown");
        assert_eq!(cfg.change_counter, 3);
        assert_eq!(cfg.skin, "blue");
        assert_eq!(cfg.face, "cat_eyes");
        assert_eq!(cfg.hat, "crown");
    }

    #[test]
    fn lock_and_unlock_are_idempotent() {
        let mut cfg = GraphwarPlayerConfig::default();
        cfg.unlock("crown");
        cfg.unlock("crown");
        assert_eq!(cfg.unlocked, vec!["crown".to_string()]);
        assert!(cfg.lock("crown"));
        assert!(!cfg.lock("crown"));
        assert!(cfg.unlocked.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let mut cfg = GraphwarPlayerConfig::default();
        cfg.name = "lucas".into();
        cfg.set_skin("red");
        cfg.unlock("top_hat");
        let text = cfg.save();
        let back = GraphwarPlayerConfig::load(&text);
        assert_eq!(back.name, "lucas");
        assert_eq!(back.skin, "red");
        assert_eq!(back.unlocked, vec!["top_hat".to_string()]);
    }

    #[test]
    fn campaign_level_number_is_scraped_from_text() {
        assert_eq!(get_level_as_int("level_7"), Some(7));
        assert_eq!(get_level_as_int("12"), Some(12));
        assert_eq!(get_level_as_int("no digits here"), None);
        assert_eq!(CAMPAIGN_LEVEL_COUNT, 21);
    }

    #[test]
    fn campaign_visibility_follows_progress() {
        let p = CampaignProgress { unlocked_through: 4 };
        assert!(p.is_level_visible(1));
        assert!(p.is_level_visible(5)); // 下一关也放出来
        assert!(!p.is_level_visible(6));
        assert!(!p.is_level_visible(0));
        assert!(!p.is_level_visible(22));
    }

    #[test]
    fn game_info_has_nine_fields_and_the_lan_tag() {
        assert_eq!(GAME_INFO_FIELD_COUNT, 9);
        assert_eq!(GAME_INFO_FIELDS.len(), 9);
        assert_eq!(GAME_INFO_FIELDS[0], "room_name");
        assert_eq!(GAME_INFO_EVENT_TAG, 62);
    }
}
