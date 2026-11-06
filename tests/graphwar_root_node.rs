//! `graphwar_root_node.rs` 的复原验收：从**外部**用公开 API 复核 `docs/16-client.md` §15 + §15.10。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 / 5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值都能指到反汇编地址或 `.rodata` 字节；地址口径与规格见
//! `docs/16-client.md` §15（证据）与 §15.10（实现侧补充 + 四条更正）。
//!
//! 本文件只碰公开 API，故意**不**依赖 `src/` 里的 `mod tests`，这样内部重构不会顺手
//! 把验收断言一起改掉。

use gw2_rebuild::graphwar_root_node::*;

/// §15.2：15 个屏名，**地址与长度逐条**。这些数是从 `.rodata` 读回的字节 + 使用点的
/// 长度立即数两路互证的（`EndGamePopUp = 12`，旧表的 11 是错的）。
#[test]
fn fifteen_screens_with_verified_rodata_addresses() {
    let want: [(&str, u64, u32); 15] = [
        ("MainMenu", 0x19fc70, 8),
        ("Campaign", 0x19fcc0, 8),
        ("LanLobby", 0x19f380, 8),
        ("GameSetup", 0x1bda32, 9),
        ("PlayScreen", 0x1bd92b, 10),
        ("ReplayLobby", 0x1bdaec, 11),
        ("PlayerScreen", 0x1bdb2f, 12),
        ("JoinIpPopUp", 0x1bdaf7, 11),
        ("AddBotPopUp", 0x1bdb02, 11),
        ("ConfirmPlay", 0x1bdb0d, 11),
        ("ExitPopUp", 0x1bdb18, 9),
        ("JoinLevelPopUp", 0x1bdb21, 14),
        ("ConnectingPopUp", 0x1bda23, 15),
        ("EndGamePopUp", 0x1bda3b, 12),
        ("DisconnectedPopUp", 0x1bda73, 17),
    ];
    assert_eq!(ALL_SCREENS.len(), want.len());
    for (s, (name, addr, len)) in ALL_SCREENS.iter().zip(want.iter()) {
        assert_eq!(s.name(), *name);
        assert_eq!(s.addr(), *addr, "{} 的 .rodata 地址", name);
        assert_eq!(s.len(), *len, "{} 的字节长度", name);
    }
}

/// 屏名 ↔ 枚举**双向**一致：`from_name(name(s)) == s` 且 `name(from_name(n)) == n`。
#[test]
fn screen_names_round_trip_in_both_directions() {
    for s in ALL_SCREENS {
        assert_eq!(Screen::from_name(s.name()), Ok(s));
    }
    for (name, _, _) in [
        ("MainMenu", 0u64, 0u32),
        ("PlayScreen", 0, 0),
        ("DisconnectedPopUp", 0, 0),
    ] {
        let s = Screen::from_name(name).expect(name);
        assert_eq!(s.name(), name);
        assert_eq!(s.addr(), Screen::from_name(s.name()).unwrap().addr());
    }
}

/// 非法屏名是**错误**，不是 panic（用 `catch_unwind` 显式证明不 panic）。
#[test]
fn illegal_screen_name_returns_an_error_and_does_not_panic() {
    let bad = ["", "nope", "playscreen", "PlayScreen\n", "\u{0}", "背景"];
    for b in bad {
        let got = std::panic::catch_unwind(|| Screen::from_name(b));
        let r = got.expect("Screen::from_name 不许 panic");
        assert_eq!(r, Err(UnknownScreenName), "{b:?}");
    }
}

/// §15.6：12 条命令键 + 长度。第 7 条是 `ask_exit_play`（旧表写成 `exit_ask_play`）。
#[test]
fn twelve_command_keys_with_verified_lengths() {
    let want: [(&str, u8); 12] = [
        ("player_settings", 15),
        ("start_level", 11),
        ("multiplayer", 11),
        ("create_game", 11),
        ("toggle_mute", 11),
        ("exit_campaign", 13),
        ("ask_exit_play", 13),
        ("cancel_add_bot", 14),
        ("exit_lan_lobby", 14),
        ("cancel_join_ip", 14),
        ("add_bot", 7),
        ("join_ip", 7),
    ];
    assert_eq!(ALL_COMMANDS.len(), want.len());
    for (c, (k, l)) in ALL_COMMANDS.iter().zip(want.iter()) {
        assert_eq!(c.as_str(), *k);
        assert_eq!(c.len(), *l);
        assert_eq!(Command::from_name(k), Some(*c));
    }
    // 两条 8 字节窗口的并集必须覆盖整条键（键长由第二窗口偏移钉死）
    for c in ALL_COMMANDS {
        let ev = c.evidence();
        let width = if c.len() >= 8 { 8usize } else { 4 };
        assert_eq!(
            ev.second_offset as usize,
            c.as_str().len() - width,
            "{} 的第二窗口",
            c.as_str()
        );
    }
}

/// §15.3 / 构造期初值：`0x3a2b30` 的 `movl $0x8010000,0x1702(%r15)`
/// ⇒ `00 / 00 / 01 / 08`。
#[test]
fn constructor_initial_values_are_00_00_01_08() {
    assert_eq!(INIT_WORD_1702, 0x0801_0000);
    let f = constructor_init_fields();
    assert!(!f.gate_a, "[+0x1702] = 0");
    assert!(!f.gate_b, "[+0x1703] = 0");
    assert_eq!(f.current_screen, 1, "[+0x1704] = 1");
    assert_eq!(f.game_state, 8, "[+0x1705] = 8");
    assert_eq!(INIT_WORD_1702.to_le_bytes(), [0u8, 0, 1, 8]);
}

/// `decode_init_word` 的四个字节必须按**小端**落到 0x1702..0x1705（错位会整体错一格）。
#[test]
fn init_word_bytes_land_in_the_right_fields() {
    assert!(decode_init_word(0x0000_0001).gate_a);
    assert!(decode_init_word(0x0000_0100).gate_b);
    assert_eq!(decode_init_word(0x0000_0100).current_screen, 0);
    assert_eq!(decode_init_word(0x0100_0000).current_screen, 0);
    assert_eq!(decode_init_word(0x0100_0000).game_state, 1);
}

/// §15.4：`_ready` 取的三个节点 —— `ScreensControl` / `IAP_holder` / `Ad_holder`，
/// 长度 14 / 10 / 9，三个都**不是**屏。
#[test]
fn ready_takes_three_holder_nodes_that_are_not_screens() {
    assert_eq!(READY_NODES.len(), 3);
    let names: Vec<&str> = READY_NODES.iter().map(|n| n.name).collect();
    assert_eq!(names, vec!["ScreensControl", "IAP_holder", "Ad_holder"]);
    let lens: Vec<u32> = READY_NODES.iter().map(|n| n.len).collect();
    assert_eq!(lens, vec![14, 10, 9]);
    for n in READY_NODES {
        assert_eq!(n.name.len(), n.len as usize);
        assert!(Screen::from_name(n.name).is_err());
    }
}

/// §15.5 + §15.10 更正 ③：`0x3af850` 的决策表 —— 只有被点名的那个屏可见。
#[test]
fn show_screen_makes_exactly_one_screen_visible() {
    let mut visible: Vec<(&str, bool)> = Vec::new();
    let entries: Vec<&str> = ALL_SCREENS.iter().map(|s| s.name()).collect();
    for entry in &entries {
        let swap = screen_swap(entry, "Campaign", false, false);
        if let Some(v) = swap.visible() {
            visible.push((entry, v));
        }
    }
    // 15 个屏条目全都写了可见性，且恰有一个 true
    assert_eq!(visible.len(), 15);
    assert_eq!(visible.iter().filter(|(_, v)| *v).count(), 1);
    assert_eq!(
        visible.iter().find(|(_, v)| *v).map(|(n, _)| *n),
        Some("Campaign")
    );
}

/// `PlayScreen` 被单列：进出对局要点不同的广告动作；`background` 完全不动。
#[test]
fn play_screen_and_background_are_special_cased() {
    // 进入对局：拆广告
    assert_eq!(
        screen_swap("PlayScreen", "PlayScreen", true, true),
        ScreenSwap::ShowAndDestroyAdView
    );
    assert_eq!(
        screen_swap("PlayScreen", "PlayScreen", true, true).deferred_method(),
        Some("destroy_ad_view")
    );
    // 离开对局：建广告
    assert_eq!(
        screen_swap("PlayScreen", "MainMenu", true, true),
        ScreenSwap::HideAndCreateAdView
    );
    assert_eq!(
        screen_swap("PlayScreen", "MainMenu", true, true).deferred_method(),
        Some("create_ad_view")
    );
    // 闸门没开就只做显隐
    assert_eq!(
        screen_swap("PlayScreen", "PlayScreen", false, true),
        ScreenSwap::Show
    );
    assert_eq!(
        screen_swap("PlayScreen", "MainMenu", true, false),
        ScreenSwap::Hide
    );
    // background 不动
    assert_eq!(
        screen_swap("background", "MainMenu", true, true),
        ScreenSwap::Leave
    );
    assert_eq!(ScreenSwap::Leave.visible(), None);
    assert_eq!(ScreenSwap::Leave.deferred_method(), None);
}

/// 动作顺序：`call_deferred` 在 `set_visible` **之前**（`0x3afa1e`/`0x3afad5` → `0x3af8a7`）。
#[test]
fn deferred_call_happens_before_the_visibility_write() {
    struct H(Vec<&'static str>);
    impl ScreenHost for H {
        fn set_visible(&mut self, _s: &str, _v: bool) {
            self.0.push("visible");
        }
        fn call_deferred(&mut self, _m: &str) {
            self.0.push("deferred");
        }
        fn play_screen_visible(&self) -> bool {
            true
        }
    }
    let mut h = H(vec![]);
    apply_screen_swap(&mut h, "PlayScreen", ScreenSwap::HideAndCreateAdView);
    assert_eq!(h.0, vec!["deferred", "visible"]);
}

/// 地址锚点：23 个可归属函数 + 构造器不在其中；`0x3af850`/`0x3af700` 没有 panic 锚。
#[test]
fn address_anchors_are_consistent_with_the_module_list() {
    assert_eq!(MODULE_FUNCS.len(), 23);
    assert!(!MODULE_FUNCS.contains(&FN_CONSTRUCT));
    assert!(!MODULE_FUNCS.contains(&FN_SHOW_SCREEN));
    assert_eq!(FN_CONSTRUCT, 0x3a22f0);
    assert_eq!(FN_READY, 0x3a2e10);
    assert_eq!(FN_STATE_MACHINE, 0x3a5760);
    assert_eq!(FN_SCREEN_GET, 0x3af700);
    assert_eq!(FN_SHOW_SCREEN, 0x3af850);
    assert_eq!(FN_BUSY_CHECK, 0x3afcf0);
    assert_eq!(FN_COMMAND_BUS, 0x3b0830);
}
