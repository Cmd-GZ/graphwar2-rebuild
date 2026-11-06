//! `graphwar_game_client.rs` 的复原验收：从**外部**用公开 API 复核
//! `docs/16-client.md` §16（本模块规格）。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2 / 5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值都能指到**反汇编立即数**或 `.rodata` 字节；地址口径与判据写在
//! `docs/16-client.md` §16。地址一律是 **ELF 虚地址**
//! （Ghidra/REA 地址 = 本值 + `0x100000`，只加一次）。
//!
//! 本文件只碰公开 API，故意**不**依赖 `src/` 里的 `mod tests`。

use gw2_rebuild::graphwar_game_client::*;

/// 36 = `tools/coverage_matrix.py` 的口径（`docs/coverage-matrix.tsv` 里本模块的 `orig_funcs`）。
///
/// ⚠️ 这是**可归属函数**的条数，不是模块规模；`fixtures/module_funcs.tsv` 的 68 行
/// 是另一种（panic 串启发式）计数，**不能**拿来断言。
#[test]
fn thirty_six_attributable_functions() {
    assert_eq!(ORIG_FUNCS.len(), 36);
    // 每一条都能按地址查回来（不允许重复地址）。
    for f in ORIG_FUNCS {
        let got = orig_fn(f.vaddr).expect("必须在清单里");
        assert_eq!(got.vaddr, f.vaddr);
    }
    let mut addrs: Vec<u32> = ORIG_FUNCS.iter().map(|f| f.vaddr).collect();
    addrs.sort_unstable();
    addrs.dedup();
    assert_eq!(addrs.len(), 36);
}

/// 分类是真划分：8 个类别加起来正好 36，且每类至少 1 个。
#[test]
fn classification_partitions_all_36() {
    let classes = [
        Class::RequestConstruction,
        Class::CommandEntry,
        Class::MatchAction,
        Class::Connection,
        Class::EventWiring,
        Class::StateQuery,
        Class::SetupCallback,
        Class::ReplayRemote,
    ];
    let total: usize = classes.iter().map(|c| class_count(*c)).sum();
    assert_eq!(total, 36);
    for c in classes {
        assert!(class_count(c) >= 1, "{c:?} 必须非空");
    }
    // 各类的条数（`docs/16-client.md` §16.1 的表）。
    assert_eq!(class_count(Class::RequestConstruction), 16);
    assert_eq!(class_count(Class::CommandEntry), 1);
    assert_eq!(class_count(Class::EventWiring), 1);
    assert_eq!(class_count(Class::MatchAction), 3);
    assert_eq!(class_count(Class::Connection), 5);
    assert_eq!(class_count(Class::StateQuery), 4);
    assert_eq!(class_count(Class::SetupCallback), 3);
    assert_eq!(class_count(Class::ReplayRemote), 3);
}

/// 函数体区间不许互相重叠（`size` 是到下一个入口的距离），且本模块没有 0 长度条目。
#[test]
fn function_ranges_do_not_overlap() {
    let mut v: Vec<(u32, u32)> = ORIG_FUNCS.iter().map(|f| (f.vaddr, f.size)).collect();
    v.sort_unstable();
    for w in v.windows(2) {
        assert!(w[0].1 > 0, "size 不能是 0");
        assert!(
            w[0].0 + w[0].1 <= w[1].0,
            "0x{:x}+0x{:x} 越过了 0x{:x}",
            w[0].0,
            w[0].1,
            w[1].0
        );
    }
}

/// 每个函数都要能指到证据：要么解到角色一级，要么在调用图里有登记（`ENTRY_POINTS`）。
///
/// 这条是 `AGENTS.md` §0 第 5 条的可执行版本：**不许有没有出处的条目**。
#[test]
fn every_function_has_an_evidence_anchor() {
    for f in ORIG_FUNCS {
        let solved = f.role != Role::Undetermined;
        let graphed = entry_point(f.vaddr).is_some();
        assert!(
            solved || graphed,
            "0x{:x} 既没有角色也没有调用图记录",
            f.vaddr
        );
    }
}

/// §16.3：8 种请求 × 2 份实例化 = 16 个函数；两份共用一个 `.data.rel.ro` 双槽。
#[test]
fn eight_request_constructors_each_have_two_instances() {
    assert_eq!(REQUEST_CONSTRUCTORS.len(), 8);
    let mut all: Vec<u32> = Vec::new();
    for row in REQUEST_CONSTRUCTORS {
        assert_ne!(row.instance_a, row.instance_b);
        // 双槽相邻 8 字节 ⇒ 同一张「2 方法」表。
        assert_eq!(row.vtable_slots[1] - row.vtable_slots[0], 8);
        // 两份实例化都在 36 条清单里，且都被分成 RequestConstruction。
        for a in [row.instance_a, row.instance_b] {
            let f = orig_fn(a).expect("实例化必须在本模块清单里");
            assert_eq!(f.class, Class::RequestConstruction);
            assert_eq!(f.role, row.kind_role());
        }
        all.push(row.instance_a);
        all.push(row.instance_b);
    }
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 16);
}

/// 同一请求的两份实例化**共享同一个 panic 锚行**（= 同一个源函数的两份代码），
/// 而 8 条请求的行号是 190..218 步长 4（源文件里的书写顺序）。
#[test]
fn paired_instances_share_the_panic_line() {
    for row in REQUEST_CONSTRUCTORS {
        let a = orig_fn(row.instance_a).unwrap();
        let b = orig_fn(row.instance_b).unwrap();
        assert_eq!(a.anchors, b.anchors, "{:?} 两份锚不一致", row.kind);
        assert_eq!(a.anchors.len(), 1);
        assert_eq!(a.anchors[0], (row.source_line, 26));
    }
    let mut lines: Vec<u32> = REQUEST_CONSTRUCTORS.iter().map(|r| r.source_line).collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![190, 194, 198, 202, 206, 210, 214, 218]);
}

/// §16.3：8 条构造器写进去的判别值**逐条等于** `src/events.rs` 的 `Event` 判别值。
///
/// 这是本模块最强的一条交叉验证：左边的 11/12/16/17/18/19/20/21 是从
/// `movabs $0x80000000000000NN` 的**指令立即数**读出的，右边的名字来自
/// `docs/07` §7.2 那张用 `0x3d38f0` 的相等测试解出来的表。
#[test]
fn request_tags_equal_event_discriminants() {
    let want: [(RequestKind, u16); 8] = [
        (RequestKind::ChangeTeam, 11),
        (RequestKind::RemovePlayer, 12),
        (RequestKind::AddSoldier, 16),
        (RequestKind::RemoveSoldier, 17),
        (RequestKind::AxisMode, 18),
        (RequestKind::FunctionMode, 19),
        (RequestKind::TurnMode, 20),
        (RequestKind::TimeMode, 21),
    ];
    for (kind, tag) in want {
        assert_eq!(kind.event().wire_tag(), tag, "{kind:?} 判别值");
        let rec = request_event(kind, 7);
        assert_eq!(rec.tag_word(), 0x8000_0000_0000_0000 | tag as u64);
        assert_eq!(rec.arg, 7);
    }
}

/// §16.3：`Event` 记录的形状（0x48 字节、tag 在高位、i32 在 `+0x08`）。
#[test]
fn event_record_shape() {
    assert_eq!(EVENT_RECORD_SIZE, 0x48);
    let rec = request_event(RequestKind::ChangeTeam, -3);
    assert_eq!(rec.tag_word() >> 48, 0x8000);
    assert_eq!(rec.arg, -3);
    // 判别值只在低位：0x8000…|11，不会碰到别的位。
    assert_eq!(rec.tag_word() & !EVENT_TAG_BASE, 11);
}

/// §16.2：`0x514a10` 的两条立即数窗口拼起来正好是 `start_game`，4 字节立即数是 `fire`。
///
/// 判据是两条独立的：字符串层面（`CMD_*`）与指令层面（`*_WINDOW*` / `FIRE_IMM32`）。
#[test]
fn command_keys_and_their_immediates_agree() {
    assert_eq!(CMD_START_GAME.len(), 10); // 0x514a17 的 cmp $0xa
    assert_eq!(CMD_FIRE.len(), 4); // 0x514a1d 的 cmp $0x4

    let b = CMD_START_GAME.as_bytes();
    assert_eq!(u64::from_le_bytes(b[0..8].try_into().unwrap()), START_GAME_WINDOW0);
    assert_eq!(u16::from_le_bytes(b[8..10].try_into().unwrap()), START_GAME_WINDOW1);

    let f = CMD_FIRE.as_bytes();
    assert_eq!(u32::from_le_bytes(f[0..4].try_into().unwrap()), FIRE_IMM32);
}

/// §16.2：`0x514a10` 的真值表。
#[test]
fn command_entry_truth_table() {
    assert_eq!(
        on_command("start_game"),
        CommandAction::Queue(gw2_rebuild::events::Event::GameStartRequest)
    );
    assert_eq!(on_command("fire"), CommandAction::FireRequest);

    // 二进制只认「长度 + 字节」两关，任何一个不满足就返回（0x514a2b 的公共出口）。
    for other in [
        "",
        "fir",
        "fire ",
        "firee",
        "Fire",
        "FIRE",
        "start_gam",
        "start_game_", // 11 字节
        "Start_game",
        "start_Game",
        "campaign",
        "toggle_names",
        "add_bot_3",
        "player_settings",
    ] {
        assert_eq!(on_command(other), CommandAction::Ignored, "{other:?} 不该被认");
    }
}

/// §16.2：`start_game` 写进去的 tag word 是 `0x800000000000001f`，低位 31 =
/// `GameStartRequest`（`docs/07` §7.2 第 31 行）。
#[test]
fn start_game_tag_equals_event_discriminant() {
    match on_command("start_game") {
        CommandAction::Queue(e) => {
            assert_eq!(e.wire_tag(), 0x1f);
            assert_eq!(e, gw2_rebuild::events::Event::GameStartRequest);
        }
        _ => panic!("start_game 必须投一条事件"),
    }
}

/// §16.4：六条 `add_bot_N` 键 → `0x516970` 收到的六个立即数。
///
/// 键的判据是 `movabs $0x5f746f625f646461`（`"add_bot_"`）+ 第 9 字节与 `'0'..'5'` 异或。
#[test]
fn add_bot_keys_carry_six_immediates() {
    assert_eq!(ADD_BOT_KEYS.len(), 6);
    let prefix = "add_bot_".as_bytes();
    assert_eq!(
        u64::from_le_bytes(prefix[0..8].try_into().unwrap()),
        ADD_BOT_PREFIX
    );

    let mut values = Vec::new();
    for (i, (key, value)) in ADD_BOT_KEYS.iter().enumerate() {
        assert_eq!(*key, format!("add_bot_{i}"));
        assert_eq!(key.len(), 9); // 8 字节窗口 + 1 字节异或
        assert_eq!(add_bot_value(key), Some(*value));
        values.push(*value);
    }
    assert_eq!(values, vec![3, 10, 25, 100, 250, 1000]);
    // 单调递增 ⇒ 「档位」，不是「序号」（推断，见 docs §16.4）。
    assert!(values.windows(2).all(|w| w[0] < w[1]));

    // 前缀之外的一律不认。
    for other in ["add_bot_6", "add_bot", "add_bot_", "add_bot_00", "ADD_BOT_0"] {
        assert_eq!(add_bot_value(other), None, "{other:?} 不该被认");
    }
}

/// §16.5：`0x517690` 订阅的 3 个事件名 —— 名字必须能在 `events.rs` 的名字表里反查到
/// **同一个**变体（两套独立证据相交），长度必须等于 `mov $len,%edx` 的立即数。
#[test]
fn subscribed_event_names_match_the_event_enum() {
    assert_eq!(SUBSCRIBED_EVENTS.len(), 3);
    // 长度立即数：0x5179cc `mov $0x19,%edx`、0x517a1f `mov $0x11,%edx`、
    // 0x517a6f `mov $0x14,%edx`（都在 0x517690 体内）。
    let lens = [25u32, 17, 20];
    let mut rodata: Vec<u32> = Vec::new();
    for ((name, ev, rodata_addr, lea_at), want_len) in SUBSCRIBED_EVENTS.iter().zip(lens) {
        assert_eq!(
            gw2_rebuild::events::Event::from_dispatch_name(name),
            Some(*ev),
            "{name} 反查不到 {ev:?}"
        );
        assert_eq!(name.len() as u32, want_len, "{name} 的长度立即数");
        let f = orig_fn(0x517690).unwrap();
        assert!(*lea_at >= f.vaddr && *lea_at < f.vaddr + f.size);
        rodata.push(*rodata_addr);
    }
    // `.rodata` 地址互不相同，且都落在 `.rodata`（0x19c000..0x19c000+0x113c0c）。
    rodata.sort_unstable();
    rodata.dedup();
    assert_eq!(rodata.len(), 3);
    for a in rodata {
        assert!((0x19c000..0x19c000 + 0x113c0c).contains(&a), "0x{a:x} 不在 .rodata");
    }
}

/// §16.5：`0x517690` 逐个装配 14 个 manager，模块名不重复。
#[test]
fn fourteen_managers_are_wired() {
    assert_eq!(WIRED_MANAGERS.len(), 14);
    let mut names: Vec<&str> = WIRED_MANAGERS.iter().map(|(n, _)| *n).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 14);
    // 前 6 个已经定位到具体 `call` 地址（观测），其余只到模块级（未知）。
    let located = WIRED_MANAGERS.iter().filter(|(_, a)| *a != 0).count();
    assert_eq!(located, 6);
    assert_eq!(FN_EVENT_SUBSCRIBE, 0x3d38f0);
}

/// §16.6：入口表只指向本模块清单里的函数；调用者地址**不在**被调函数的体内。
#[test]
fn entry_points_are_consistent() {
    let mut targets: Vec<u32> = ENTRY_POINTS.iter().map(|e| e.target).collect();
    targets.sort_unstable();
    targets.dedup();
    assert_eq!(targets.len(), ENTRY_POINTS.len(), "入口目标不许重复");

    for e in ENTRY_POINTS {
        let f = orig_fn(e.target).expect("入口目标必须在本模块清单里");
        for c in e.callers {
            assert!(
                *c < f.vaddr || *c >= f.vaddr + f.size,
                "0x{c:x} 落在 0x{:x} 自己体内，不可能是调用者",
                f.vaddr
            );
        }
    }
}

/// `0x514a10` 的 5 个调用者逐条（其中 `0x3b2266` 是 root_node 命令总线的兜底分支）。
#[test]
fn command_entry_callers_are_known() {
    let e = entry_point(0x514a10).expect("0x514a10 必须登记");
    assert_eq!(e.callers, &[0x3b2266, 0x3a55f7, 0x3a5637, 0x3b6e1e, 0x3b703e]);
    // 兜底分支的转发点：`0x3b2242`-`0x3b2266`。
    assert!(e.callers.contains(&0x3b2266));
}

/// `add_bot_N` 的六个 `0x516970` 调用点（`call *0x9baaa0`）逐条。
#[test]
fn add_bot_call_sites_are_the_documented_six() {
    let e = entry_point(0x516970).expect("0x516970 必须登记");
    assert_eq!(
        e.callers,
        &[0x3b31bb, 0x3b3679, 0x3b397a, 0x3b3eb9, 0x3b40d9, 0x3b41f1]
    );
    // 调用者在 root_node 命令总线 0x3b0830..0x3b4ec0 内。
    for c in e.callers {
        assert!((0x3b0830..0x3b4ec0).contains(c), "0x{c:x} 不在命令总线下");
    }
}

/// 3 个与 `graphwar_replay_player.rs` 共用 Location 的函数必须**标出来**
/// （归属有歧义，`docs/16-client.md` §16.5 的诚实清单）。
#[test]
fn contested_functions_are_marked() {
    let contested: Vec<u32> = ORIG_FUNCS
        .iter()
        .filter(|f| !f.also_in.is_empty())
        .map(|f| f.vaddr)
        .collect();
    // 16 个请求构造器与 setup_control 共用 + 3 个与 replay_player 共用 = 19。
    assert_eq!(contested.len(), 19);
    assert_eq!(class_count(Class::ReplayRemote), 3);
    for f in ORIG_FUNCS.iter().filter(|f| f.class == Class::ReplayRemote) {
        assert_eq!(f.also_in, &["graphwar_replay_player.rs"]);
    }
    for f in ORIG_FUNCS.iter().filter(|f| f.class == Class::RequestConstruction) {
        assert_eq!(f.also_in, &["graphwar_setup_control.rs"]);
    }
}

/// §16.3/§16.7：跨模块调用点必须落在**本模块某个函数的体内**，且目标/模块名自洽。
///
/// 这条把「`0x517690` 订阅 3 个事件」「`0x516370`/`0x518320` 调连接处理器」
/// 从"我记得"变成可执行的不变式。
#[test]
fn cross_module_call_sites_are_inside_this_module() {
    let inside = |addr: u32| {
        ORIG_FUNCS
            .iter()
            .any(|f| addr >= f.vaddr && addr < f.vaddr + f.size)
    };

    assert_eq!(DIRECT_CALLS_TO_OTHER_MODULES.len(), 4);
    for (at, target, module) in DIRECT_CALLS_TO_OTHER_MODULES {
        assert!(inside(*at), "0x{at:x} 不在本模块任何函数体内");
        assert!(!module.is_empty());
        assert_ne!(*target, 0);
    }

    assert_eq!(INDIRECT_CALLS_TO_OTHER_MODULES.len(), 4);
    for (at, slot, target, module) in INDIRECT_CALLS_TO_OTHER_MODULES {
        assert!(inside(*at), "0x{at:x} 不在本模块任何函数体内");
        // 三个订阅点用的是同一个槽、同一个目标。
        if module.contains("event_handler") {
            assert_eq!(*slot, 0x9bac88);
            assert_eq!(*target, FN_EVENT_SUBSCRIBE);
        }
        // GOT 槽在 .got（0x9ba548..0x9ba548+0x5620）。
        assert!((0x9ba548..0x9ba548 + 0x5620).contains(slot), "0x{slot:x} 不在 .got");
    }

    // 订阅的 3 个 `lea` 地址都必须在 0x517690 体内（与 SUBSCRIBED_EVENTS 一致）。
    let f = orig_fn(0x517690).unwrap();
    for (_, _, _, lea_at) in SUBSCRIBED_EVENTS {
        assert!(*lea_at >= f.vaddr && *lea_at < f.vaddr + f.size);
    }
}
