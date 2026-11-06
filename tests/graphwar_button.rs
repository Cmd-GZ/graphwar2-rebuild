//! `src/graphwar_button.rs` 的复原验收：从**外部**用公开 API 复核。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 / 5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值要么是反汇编立即数，要么是 `.rodata` 字节，要么是
//! 2.0.60 arm64 `.symtab` 的真名；出处写在 `src/graphwar_button.rs` 的
//! 模块注释 §1–§5 与 `docs/16-client.md` 新增那一节里。
//!
//! 地址口径：一律 **ELF 虚地址**（`objdump`）；Ghidra/REA 地址 = 本值 + `0x100000`。

use gw2_rebuild::graphwar_button::*;

/// 6 个方法、6 个互不相同的地址、大小单调可核。
#[test]
fn six_methods_with_distinct_addresses() {
    assert_eq!(ORIG_FUNCS.len(), 6);
    let mut addrs: Vec<u32> = ORIG_FUNCS.iter().map(|f| f.vaddr).collect();
    addrs.sort_unstable();
    addrs.dedup();
    assert_eq!(addrs.len(), 6);
    for f in ORIG_FUNCS {
        let got = orig_fn(f.vaddr).expect("必须能按地址查回来");
        assert_eq!(got.name, f.name);
        assert!(f.size > 0);
        // 这 6 个方法**没有**一个 panic 锚 —— 这正是 TSV 钉不住它们的原因。
        assert!(f.anchors.is_empty(), "{} 不该有 Location 锚", f.name);
    }
}

/// 两个「只出现一次」的判据：`button_down`/`button_up` 的地址与大小。
#[test]
fn button_down_and_up_addresses() {
    let down = orig_fn(0x4ef390).expect("button_down @ 0x4ef390");
    assert_eq!(down.name, "GraphwarButton::button_down");
    assert_eq!(down.size, 464);
    let up = orig_fn(0x4ef6a0).expect("button_up @ 0x4ef6a0");
    assert_eq!(up.name, "GraphwarButton::button_up");
    assert_eq!(up.size, 448);
    // 两者都引用 type_name::<GraphwarLongPressAnimation>()（全库仅这两处）。
    // 判据是可复核的：`lea 0x1c1c13` 只出现在 0x4ef390 与 0x4ef6a0。
    assert!(down.evidence.contains("GraphwarLongPressAnimation"));
    assert!(up.evidence.contains("0x3b9aca00"));
}

/// `init`/`ready`/`process` 三个地址（相邻、大小与 arm64 同量级）。
#[test]
fn init_ready_process_addresses() {
    assert_eq!(orig_fn(0x4e3e00).unwrap().name, "GraphwarButton::init");
    assert_eq!(orig_fn(0x4e3f20).unwrap().name, "GraphwarButton::ready");
    assert_eq!(orig_fn(0x4e42a0).unwrap().name, "GraphwarButton::process");
    // 源码顺序 = 生成顺序（arm64 符号表同序：init < ready < process）。
    let a = [0x4e3e00u32, 0x4e3f20, 0x4e42a0];
    assert!(a.windows(2).all(|w| w[0] < w[1]));
    // x86 / arm64 大小比都在 1.03–1.07（实测 6 个值见模块注释 §2）。
    let pairs = [
        (orig_fn(0x4e3e00).unwrap(), arm_sym("IButton>::init").unwrap()),
        (orig_fn(0x4e3f20).unwrap(), arm_sym("IButton>::ready").unwrap()),
        (orig_fn(0x4e42a0).unwrap(), arm_sym("IButton>::process").unwrap()),
        (orig_fn(0x4ef390).unwrap(), arm_sym("::button_down").unwrap()),
        (orig_fn(0x4ef560).unwrap(), arm_sym("::pressed").unwrap()),
        (orig_fn(0x4ef6a0).unwrap(), arm_sym("::button_up").unwrap()),
    ];
    for (x, a) in pairs {
        let ratio = f64::from(x.size) / f64::from(a.size);
        assert!(
            (1.02..=1.07).contains(&ratio),
            "{}: x86 {} / arm64 {} = {ratio}",
            x.name,
            x.size,
            a.size
        );
    }
}

/// arm64 2.0.60 的真名（3 个游戏方法 + 3 个虚函数）。
#[test]
fn arm64_names_are_real() {
    assert_eq!(arm_sym("::button_down").unwrap().addr, 0x448470);
    assert_eq!(arm_sym("::pressed").unwrap().addr, 0x448624);
    assert_eq!(arm_sym("::button_up").unwrap().addr, 0x448750);
    assert_eq!(arm_sym("IButton>::init").unwrap().addr, 0x43eb08);
    assert_eq!(arm_sym("IButton>::ready").unwrap().addr, 0x43ec20);
    assert_eq!(arm_sym("IButton>::process").unwrap().addr, 0x43ef84);
    // demangle 后的名字里必须有 graphwar_button 模块路径
    for s in ARM64_SYMS {
        assert!(
            s.name.contains("GraphwarButton") || s.name.contains("graphwar_button"),
            "{}",
            s.name
        );
    }
    assert!(arm_sym("不存在的符号").is_none());
}

/// 类名 —— `.rodata` 14 字节。
#[test]
fn class_name_string() {
    assert_eq!(CLASS, "GraphwarButton");
    assert_eq!(CLASS.len(), 14);
    assert_eq!(CLASS_NAME_VADDR, 0x1c40dc);
    assert_eq!(CLASS_NAME_FNS, &[0x4ff890, 0x500430]);
    for t in TRAIT_IMPLS {
        assert!(!t.is_empty());
    }
    assert!(TRAIT_IMPLS.contains(&"IButton"));
}

/// `ready` 连的 3 条信号：名字与长度都是 `.rodata` 上的真值。
#[test]
fn signal_links_are_bit_exact() {
    assert_eq!(SIGNAL_LINKS.len(), 3);
    assert_eq!(SIGNAL_LINKS[0].name, "pressed");
    assert_eq!(SIGNAL_LINKS[0].vaddr, 0x1c4258);
    assert_eq!(SIGNAL_LINKS[0].len as usize, "pressed".len());
    assert_eq!(SIGNAL_LINKS[1].name, "button_up");
    assert_eq!(SIGNAL_LINKS[1].vaddr, 0x1c425f);
    assert_eq!(SIGNAL_LINKS[1].len as usize, "button_up".len());
    assert_eq!(SIGNAL_LINKS[2].name, "button_down");
    assert_eq!(SIGNAL_LINKS[2].vaddr, 0x1c4268);
    assert_eq!(SIGNAL_LINKS[2].len as usize, "button_down".len());
    // 三个地址在 .rodata 里首尾相接（7 + 9 + 11 = 27；0x1c4273 - 0x1c4258 = 27）。
    assert_eq!(
        SIGNAL_LINKS[2].vaddr + u32::from(SIGNAL_LINKS[2].len),
        NODE_ANIMATION_RECT_VADDR
    );
}

/// `AnimationRect` 是**节点名**（不是属性），13 字节。
#[test]
fn animation_rect_node_name() {
    assert_eq!(NODE_ANIMATION_RECT, "AnimationRect");
    assert_eq!(NODE_ANIMATION_RECT.len(), 13);
    assert_eq!(NODE_ANIMATION_RECT_VADDR, 0x1c4273);
    let (links, node) = ButtonState::ready_plan();
    assert_eq!(links.len(), 3);
    assert_eq!(node, NODE_ANIMATION_RECT);
}

/// 两个属性的名字 + 4 个 getter/setter 名（`.rodata` 上首尾相接的那一串）。
#[test]
fn properties_are_two() {
    assert_eq!(PROPERTIES.len(), 2);
    let it = property("input_text").unwrap();
    assert_eq!(it.getter, "get_input_text");
    assert_eq!(it.setter, "set_input_text");
    assert_eq!(it.name_vaddr, 0x1c42ce);
    let lp = property("long_press_input_text").unwrap();
    assert_eq!(lp.getter, "get_long_press_input_text");
    assert_eq!(lp.setter, "set_long_press_input_text");
    assert_eq!(lp.name_vaddr, 0x1c42d8);
    assert!(property("nope").is_none());
    // 4 个分派入口地址互不相同。
    let mut v = vec![
        it.getter_vcall.0,
        it.getter_vcall.1,
        it.setter_vcall.0,
        it.setter_vcall.1,
        lp.getter_vcall.0,
        lp.getter_vcall.1,
        lp.setter_vcall.0,
        lp.setter_vcall.1,
    ];
    v.sort_unstable();
    v.dedup();
    assert_eq!(v.len(), 8);
    assert_eq!(FN_REGISTER_EXPORTS, 0x4e4ac0);
    assert_eq!(FN_NAME_DISPATCH, 0x5009f0);
}

/// 哨兵与阈值的算术：`0x3b9aca00` = 1e9 ns，`0x1dcd6500` = 0.5 s。
#[test]
fn sentinel_and_hold_constants() {
    assert_eq!(NANOS_PER_SEC, 1_000_000_000);
    assert_eq!(NANOS_PER_SEC, 0x3b9a_ca00);
    assert_eq!(LONG_PRESS_HOLD_NANOS, 0x1dcd_6500);
    assert_eq!(LONG_PRESS_HOLD_NANOS, 500_000_000);
    assert_eq!(Stamp::RELEASED_NANOS, NANOS_PER_SEC);
    // 500 ms 与长按动画的时长是同一个数（两处独立量到）。
    assert_eq!(
        f64::from(LONG_PRESS_HOLD_NANOS) / f64::from(NANOS_PER_SEC),
        gw2_rebuild::graphwar_long_press_animation::SPIN_DURATION_SECS
    );
}

/// `Stamp` 的哨兵语义：只有 nanos 变，secs 保留。
#[test]
fn stamp_sentinel_semantics() {
    assert!(!Stamp::NONE.is_pressed());
    let t = Stamp::now(12, 345_000_000);
    assert!(t.is_pressed());
    assert_eq!(t.released(), Stamp::now(12, NANOS_PER_SEC));
    assert!(!t.released().is_pressed());
    assert_eq!(Stamp::NONE.released(), Stamp::NONE);
}

/// `init` 写出的初值（`0x4e3e5a`–`0x4e3eb3`）。
#[test]
fn init_state_matches_observed_writes() {
    let s = ButtonState::new();
    assert!(!s.animation_linked, "[self+0x18] = 0");
    assert_eq!(s.press, Stamp::NONE, "[self+0x68] = 0x3b9aca00");
    assert_eq!(s.hold, Stamp::now(0, LONG_PRESS_HOLD_NANOS), "[+0x50]=0, [+0x58]=0x1dcd6500");
    assert_eq!(s.input_text, "", "[self+0x88] = GString(\"\")");
    assert_eq!(s.long_press_input_text, "", "[self+0x90] = GString(\"\")");
    assert_eq!(s.subscribers, 0, "[self+0x48] = 0");
    assert_eq!(ButtonState::default(), s);
}

/// `button_down`：无条件记时间；只有接了动画节点才写第二处 + 请求重绘。
#[test]
fn button_down_effects() {
    let mut s = ButtonState::new();
    let now = Stamp::now(100, 5);
    let again = Stamp::now(100, 9);
    let fx = s.button_down(now, again);
    assert_eq!(fx, vec![Effect::RecordPress(now)]);
    assert_eq!(s.press, now);
    assert!(!fx.contains(&Effect::QueueRedraw));

    s.animation_linked = true;
    let fx = s.button_down(Stamp::now(101, 1), again);
    assert_eq!(
        fx,
        vec![
            Effect::RecordPress(Stamp::now(101, 1)),
            Effect::StartAnimation(again),
            Effect::QueueRedraw
        ]
    );
    // 两次 Instant::now() 是**不同**的调用（0x4ef39d 与 0x4ef3e2）。
    assert_eq!(fx[0], Effect::RecordPress(Stamp::now(101, 1)));
    assert_eq!(fx[1], Effect::StartAnimation(again));
}

/// `button_up`：self 的哨兵写在判别之前，所以任何情况下 `press` 都会释放。
#[test]
fn button_up_always_releases() {
    let mut s = ButtonState::new();
    s.press = Stamp::now(7, 8);
    let fx = s.button_up();
    assert_eq!(fx, vec![]);
    assert_eq!(s.press, Stamp::now(7, NANOS_PER_SEC));

    let mut s = ButtonState::new();
    s.animation_linked = true;
    s.press = Stamp::now(7, 8);
    let fx = s.button_up();
    assert_eq!(fx, vec![Effect::StopAnimation, Effect::QueueRedraw]);
    assert_eq!(s.press.nanos, NANOS_PER_SEC);
    // secs 不动（原版是 `mov dword`，只写 nanos）。
    assert_eq!(s.press.secs, 7);
}

/// `process`：四条判定按原版顺序，且只派发一次。
#[test]
fn process_is_one_shot() {
    let mut s = ButtonState::new();
    // 1) 长按文本为空 → 不派发
    s.press = Stamp::now(1, 2);
    assert_eq!(s.process(), None);
    // 2) 有文本但已经释放 → 不派发
    s.long_press_input_text = "a".into();
    s.press = Stamp::NONE;
    assert_eq!(s.process(), None);
    assert_eq!(s.press, Stamp::NONE, "没按下时不该改状态");
    // 3) 按着但没有订阅者 → 仍然把哨兵写回去（原版 0x4e42d7 在 0x4e42df 之前）
    s.press = Stamp::now(1, 2);
    assert_eq!(s.process(), None);
    assert!(!s.press.is_pressed(), "状态已被消费");
    // 4) 再用一次：状态已释放 → 直接返回
    s.subscribers = 3;
    s.long_press_input_text = "b".into();
    assert_eq!(s.process(), None);
    // 5) 重新按下 → 派发一次长按文本
    s.press = Stamp::now(2, 3);
    assert_eq!(s.process(), Some("b"));
    assert!(!s.press.is_pressed());
    assert_eq!(s.process(), None, "一次性：第二次不再派发");
}

/// `pressed`：发 `input_text`，**不改**状态（与 `process` 的分工）。
#[test]
fn pressed_does_not_mutate() {
    let mut s = ButtonState::new();
    s.input_text = "x".into();
    s.long_press_input_text = "y".into();
    s.press = Stamp::now(1, 2);
    let before = s.clone();
    assert_eq!(s.pressed(), "x");
    assert_eq!(s, before, "pressed 只读");
    // 两个字段是**不同**的偏移：0x88 vs 0x90。
    assert_ne!(OFF_INPUT_TEXT, OFF_LONG_PRESS_TEXT);
    assert_eq!(OFF_INPUT_TEXT, 0x88);
    assert_eq!(OFF_LONG_PRESS_TEXT, 0x90);
}

/// 偏移表本身也是判据（改一个就等于宣称二进制变了）。
#[test]
fn struct_offsets() {
    assert_eq!(OFF_ANIMATION_TAG, 0x18);
    assert_eq!(OFF_SUBSCRIBERS, (0x40, 0x48));
    assert_eq!(OFF_HOLD_DURATION, 0x50);
    assert_eq!(OFF_PRESS_STAMP, 0x60);
}

/// `name_to_addr.tsv` 的候选地址被反汇编否掉（冲突已就地记录）。
#[test]
fn name_bridge_conflicts_are_not_our_functions() {
    assert!(!NAME_BRIDGE_CONFLICTS.is_empty());
    for (name, vaddr, why) in NAME_BRIDGE_CONFLICTS {
        assert!(
            orig_fn(*vaddr).is_none(),
            "{name} 的候选 0x{vaddr:06x} 不该在我们的清单里"
        );
        assert!(!why.is_empty());
        // 候选都在 0x3axxxx（root_node）或 gdext 区，远离本模块的 0x4e3/0x4ef 带。
        assert!(*vaddr < 0x4e3e00 || *vaddr > 0x4efc81);
    }
}

/// 三个 panic 锚：只有 gdext 样板引用，`draw` 式的方法一个都没有。
#[test]
fn source_anchors_are_boilerplate_only() {
    assert_eq!(SRC_ANCHORS.len(), 3);
    for (loc, line, col, fns) in SRC_ANCHORS {
        assert!(loc.starts_with("src/graphwar_button.rs:"));
        assert!(*line > 0 && *col > 0);
        assert!(!fns.is_empty(), "{loc} 必须有引用者");
        for f in *fns {
            assert!(
                orig_fn(*f).is_none(),
                "锚所在函数 0x{f:06x} 是样板，不该出现在 6 个方法里"
            );
        }
    }
    // 83:1 有 4 个函数（两个 #[godot_api] 块各 2 个），111:1 只有 1 个。
    assert_eq!(SRC_ANCHORS[1].3.len(), 4);
    assert_eq!(SRC_ANCHORS[2].3.len(), 1);
}
