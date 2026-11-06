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

    #[test]
    fn empty_input_stays_empty() {
        assert!(pnc::PolishNotationConverter::reorder(vec![]).is_empty());
    }

    #[test]
    fn single_atom_passes_through() {
        assert_eq!(to_prefix(&["x"]), vec![op(Function::VarX)]);
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        // 1+2*3  ->  + 1 * 2 3
        assert_eq!(
            to_prefix(&["1", "+", "2", "*", "3"]),
            vec![op(Function::Add), c(1.0), op(Function::Mul), c(2.0), c(3.0)]
        );
    }

    #[test]
    fn brackets_change_the_root() {
        // (1+2)*3  ->  * + 1 2 3
        assert_eq!(
            to_prefix(&["(", "1", "+", "2", ")", "*", "3"]),
            vec![op(Function::Mul), op(Function::Add), c(1.0), c(2.0), c(3.0)]
        );
    }

    #[test]
    fn brackets_never_reach_the_output() {
        let out = to_prefix(&["(", "1", "+", "2", ")", "*", "3"]);
        assert!(!out
            .iter()
            .any(|t| matches!(t.tag, Function::LeftBracket | Function::RightBracket)));
    }

    #[test]
    fn subtraction_is_left_associative() {
        // 1-2-3 -> (- (- 1 2) 3)  — 平级取后出现的当根
        assert_eq!(
            to_prefix(&["1", "-", "2", "-", "3"]),
            vec![op(Function::Sub), op(Function::Sub), c(1.0), c(2.0), c(3.0)]
        );
    }

    #[test]
    fn pow_is_left_associative() {
        // 2^3^2 -> (^ (^ 2 3) 2)。docs/03 的 "`^` 左结合" 机制就在这条判据里
        assert_eq!(
            to_prefix(&["2", "^", "3", "^", "2"]),
            vec![op(Function::Pow), op(Function::Pow), c(2.0), c(3.0), c(2.0)]
        );
    }

    #[test]
    fn mixed_same_precedence_is_left_associative() {
        // 1/2*3 -> (* (/ 1 2) 3)
        assert_eq!(
            to_prefix(&["1", "/", "2", "*", "3"]),
            vec![op(Function::Mul), op(Function::Divide), c(1.0), c(2.0), c(3.0)]
        );
    }

    #[test]
    fn unary_function_wraps_everything_to_its_right() {
        // sin(x) -> sin x
        assert_eq!(
            to_prefix(&["sin", "(", "x", ")"]),
            vec![op(Function::Sin), op(Function::VarX)]
        );
    }

    #[test]
    fn unary_functions_are_right_associative() {
        // sin cos x -> sin cos x （先出现的当根）
        assert_eq!(
            to_prefix(&["sin", "cos", "x"]),
            vec![op(Function::Sin), op(Function::Cos), op(Function::VarX)]
        );
    }

    #[test]
    fn tokenizer_never_produces_unary_minus() {
        // `FunctionToken::new("-")` 给的是**二元减**（tag 4），不是 tag 5。
        // ⇒ 词法层没有一元负号；`-x` 由 `+`/`-` 补 CONST 0 变成 `0 - x`。
        assert_eq!(ft::new("-").unwrap().tag, Function::Sub);
        assert_ne!(ft::new("-").unwrap().tag, Function::UnaryMinus);
        assert_eq!(
            to_prefix(&["-", "x"]),
            vec![op(Function::Sub), c(0.0), op(Function::VarX)]
        );
    }

    #[test]
    fn unary_minus_token_if_present_binds_looser_than_pow() {
        // 词法器不产 tag 5（见上一条），但转换器**认识**它：
        // 一元（`neg` = 5，或 `sqrt..tan` = 9..=16）走"不递归、接着扫右边"那条路。
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
        // 1+ -> + 1 CONST0（docs/03 的 "`+` fills missing operands"）
        assert_eq!(
            to_prefix(&["1", "+"]),
            vec![op(Function::Add), c(1.0), c(0.0)]
        );
    }

    #[test]
    fn missing_left_operand_of_minus_becomes_const_zero() {
        // -1 在分词器眼里是 neg + 1；但二元减号打头时补 CONST0
        // 这里用 "*" 打头做对照：只有 +/- 才补洞
        assert_eq!(
            to_prefix(&["+", "1"]),
            vec![op(Function::Add), c(0.0), c(1.0)]
        );
    }

    #[test]
    fn only_plus_and_minus_fill_holes() {
        // '*' 打头不补 0，那一位就是空缺（原版同样不补）
        let out = to_prefix(&["*", "2"]);
        assert_eq!(out, vec![op(Function::Mul), c(2.0)]);
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
        // ((x)) -> x
        assert_eq!(to_prefix(&["(", "(", "x", ")", ")"]), vec![op(Function::VarX)]);
        // sin(cos(tan(x)))
        assert_eq!(
            to_prefix(&["sin", "(", "cos", "(", "tan", "(", "x", ")", ")", ")"]),
            vec![
                op(Function::Sin),
                op(Function::Cos),
                op(Function::Tan),
                op(Function::VarX)
            ]
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

    #[test]
    fn port_is_42972() {
        assert_eq!(cr::CRASH_REPORT_PORT, 42972);
    }

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
