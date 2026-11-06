//! `graphwar_lan_vbox.rs` 的复验收据（逐条对拍 `src/graphwar_lan_vbox.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到 `.symtab` 记录、反汇编地址或 `.rodata` 字节，逐条写明出处。
//!
//! 地址口径：本文件的 **x86 地址是 ELF 虚地址**（`objdump` 口径，Ghidra = `+0x100000`）；
//! **arm64 地址一律标明**，来自 Android 2.0.60 未 strip 的 `libgraphwarrust.so`
//! （`readelf -sW | c++filt | grep lan_vbox`）。
//!
//! 证据总表见 `docs/09-network.md` §10（本轮追加）。

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use gw2_rebuild::graphwar_lan_vbox as lv;

fn v4(last: u8, port: u16) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, last), port))
}

fn v6() -> SocketAddr {
    SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 46868, 0, 0))
}

fn row(name: &str, players: u32, locked: bool) -> lv::RoomEntry {
    lv::RoomEntry {
        room_name: name.to_string(),
        player_count: players,
        locked,
        axis_mode: "EveryUnit",
        function_mode: "NormalFunction",
        turn_mode: "SequentialTurns",
        time_mode: "Timer1m",
    }
}

/// 判据 1：本模块在 arm64 `.symtab` 里是 **13 个 `FUNC`**，其中**只有 `process` 与两个
/// `__inner_init`** 有跨架构一对一锚。
///
/// 真值：`readelf -sW libgraphwarrust.so | c++filt | grep graphwar_lan_vbox` 给出 13 条
/// `FUNC` + 6 个 `drop_in_place` 胶水 + 4 个 `_1_6__init` 槽；
/// `fixtures/name_to_addr.tsv` 里 `process` 是**唯一锚**（x86 `0x4f90b0`，16 个
/// `(file,line,col)`），`__register_constants` / `update_game_list` / `init` 是**歧义**。
#[test]
fn thirteen_symbols_and_only_three_have_one_to_one_anchors() {
    assert_eq!(lv::FUNCTIONS.len(), 13);
    let unique: Vec<&str> = lv::FUNCTIONS
        .iter()
        .filter(|f| f.link == lv::Link::UniqueAnchor)
        .map(|f| f.name)
        .collect();
    assert_eq!(
        unique,
        vec![
            "__inner_init", // 145:1  → x86 0x4e8f80
            "__inner_init", // 30:10  → x86 0x4e9380
            "<GraphwarLanVBox as IVBoxContainer>::process",
        ]
    );
    // 歧义项必须**没有** x86 地址（不替原版挑候选）
    assert_eq!(
        lv::FUNCTIONS
            .iter()
            .filter(|f| f.link == lv::Link::Ambiguous && f.x86.is_some())
            .count(),
        0
    );
    // `init` 有名字但没有落点（x86 侧被归到别的模块）
    let init = lv::function_named("<GraphwarLanVBox as IVBoxContainer>::init").unwrap();
    assert_eq!(init.link, lv::Link::Unattributed);
    assert_eq!(init.size, 400);
    assert_eq!(init.anchor, None);
    // 2024 版有 `get_join_address`，2.0.60 没有这个符号 ⇒ 本表里也不许出现
    assert!(lv::function_named("get_join_address").is_none());
}

/// 判据 2：`process` 的 x86 落点是 `0x4f90b0`（Ghidra `0x5f90b0`），arm64 是 `0x450c20`。
///
/// 真值：`fixtures/name_to_addr.tsv` 的唯一锚；arm64 `.symtab` 的 size = **14524**；
/// x86 到下一个 Ghidra 函数起点（`analysis/funcstarts_ghidra_va.pkl` 的 `0x4fd1b0`）
/// 是 **16640** —— 那是**上界**，不是大小（`docs/02` §19：这一版函数被整体重排）。
#[test]
fn process_addresses_and_the_size_upper_bound() {
    assert_eq!(lv::PROCESS_X86, 0x4f90b0);
    assert_eq!(lv::ghidra(lv::PROCESS_X86), 0x5f90b0);
    assert_eq!(lv::PROCESS_X86 + u64::from(lv::PROCESS_X86_SPAN), 0x4fd1b0);
    assert_eq!(lv::PROCESS_ARM64_SIZE, 14524);
    let p = lv::function_at(0x450c20).unwrap();
    assert_eq!(p.x86, Some(lv::PROCESS_X86));
    assert_eq!(p.size, lv::PROCESS_ARM64_SIZE);
    // 14 524 B 的原因：同一段行更新代码在两条分支里各一份（arm64 0x450f70 / 0x45218c）
    assert!(lv::PROCESS_ARM64_SIZE > 14_000);
}

/// 判据 3：`process` 引用的 15 个 `(line, col)` 锚，且其中 8 个成 4 对（4 个模式按钮）。
///
/// 真值：解 `.data.rel.ro` 的 `panic::Location`（24 B：file 指针 + len + line:u32 + col:u32），
/// 按 `analysis/funcstarts_ghidra_va.pkl` 归到函数后，`process` 拿到
/// 90:140 / 110:136 / 113:124 / 116:135 / 122:135 / 123:56 / 127:143 / 128:60 /
/// 132:135 / 133:56 / 137:135 / 138:56 / 167:141 / 172:137 / 174:62。
/// `fixtures/name_to_addr.tsv` 记的"16 个双向唯一锚"里，第 16 个落在
/// `src/graphwar_root_node.rs:630:22`（`process` 内联了根节点取 backend 的那一句）。
#[test]
fn process_anchors_are_fifteen_lines_and_four_pairs() {
    assert_eq!(lv::PROCESS_ANCHORS.len(), 15);
    assert_eq!(lv::PROCESS_ANCHORS[0], (90, 140), "name_to_addr 展示的锚");
    assert_eq!(lv::PROCESS_ANCHORS[14], (174, 62));
    // 3 个字段锚 + 8 个模式按钮锚 = 11 个；剩下 4 个在 90/167/172/174
    let mut field_lines = 0;
    for (a, b) in lv::MODE_BUTTON_ANCHOR_PAIRS.iter() {
        assert_eq!(*b, a + 1);
        field_lines += 2;
    }
    assert_eq!(field_lines + lv::ROW_FIELD_ANCHOR_LINES.len(), 11);
    // `init` 在 145 行 ⇒ 2.0.60 的这个文件比 docs/02 §18 A 表写的 28–135 长
    assert!(lv::SOURCE_LAST_ANCHOR_LINE >= 174);
}

/// 判据 4：实例布局 `0x70` 字节 —— Sender / Receiver / base / 行缓存 / show_global。
///
/// 真值（arm64）：
/// * `Sender<SocketAddr>` @`+0x00`（16 B）：`init` `0x450b54` 写 `{1, ptr}`；闭包里
///   `0x4491d8` 调 `mpmc::Sender::send`，`&self` 就是它；拷贝侧 `0x4522a8` 用 `ldxr/stxr`。
/// * `Receiver<SocketAddr>` @`+0x10`：`try_recv(&self+0x10)`（`0x450c54` / `0x450d98`）。
/// * `base` @`+0x20`（24 B）：`assert_constructed(self+0x20)` + `Gd::from_obj_sys_weak([self+0x28])`。
/// * 行缓存 `HashMap<SocketAddr, Gd>` @`+0x38`（48 B，桶 48 B = 32+16）：清理循环 `0x452018`。
/// * `show_global` @`+0x68`：`init` `strb wzr`（默认 false）；`process` `0x450ed4` 读。
/// * 通道载荷类型是硬证据：`init` 的 unwind 引 `drop_in_place<…Channel<SocketAddr>>`（`0x433b54`）。
#[test]
fn instance_layout_matches_the_observed_offsets() {
    assert_eq!(lv::INSTANCE_SIZE, 0x70);
    assert_eq!(lv::SENDER_OFFSET, 0x00);
    assert_eq!(lv::RECEIVER_OFFSET, 0x10);
    assert_eq!(lv::BASE_OFFSET, 0x20);
    assert_eq!(lv::ROWS_OFFSET, 0x38);
    assert_eq!(lv::SHOW_GLOBAL_OFFSET, 0x68);
    assert!(lv::BASE_OFFSET + 24 == lv::ROWS_OFFSET);
    assert!(lv::ROWS_OFFSET + 48 == lv::SHOW_GLOBAL_OFFSET);
    assert_eq!(lv::ROW_BUCKET_BYTES, std::mem::size_of::<SocketAddr>() + 16);
    assert_eq!(lv::DEFAULT_SHOW_GLOBAL, false);
}

/// 判据 5：Local / Global = 一个 bool 选两套表；两套表的槽与表偏移都不同。
///
/// 真值（arm64）：`process` `0x450ed4` `ldrb w8,[x19,#0x68]`；`0x450edc` 走
/// `container+0x240`（broker），`0x450f1c` 走 `container+0x128`（LAN）；
/// 表分别在 `+0x248` 与 `+0x1a0`。逐行取值的函数分别是
/// `GraphwarServerBrokerHandler::get_game_info`（`0x489b98`）与
/// `GraphwarLanBroadcasterV6::get_game_info`（`0x3ddce8`）——两条分支的清理循环各调一个，
/// 这是"两条分支各一套表"的直接证据。
#[test]
fn local_and_global_are_two_sources_behind_one_flag() {
    assert_eq!(lv::Source::from_show_global(false), lv::Source::Lan);
    assert_eq!(lv::Source::from_show_global(true), lv::Source::Broker);
    assert_eq!(lv::Source::Lan.slot_offset(), 0x128);
    assert_eq!(lv::Source::Broker.slot_offset(), 0x240);
    assert_eq!(lv::Source::Lan.table_offset(), 0x1a0);
    assert_eq!(lv::Source::Broker.table_offset(), 0x248);
    assert_eq!(lv::LAN_BUCKET_BYTES, 120, "docs/144 §1.1 的 0x78");
    assert_eq!(lv::ROOT_MODULE_CONTAINER_OFFSET, 0x1668);
    assert_eq!(lv::ROOT_TEXTURE_LOADER_OFFSET, 0x16a0);

    let mut vb = lv::LanVBox::new();
    assert_eq!(vb.source(), lv::Source::Lan, "init 之后是 Local 页");
    vb.set_show_global(true);
    assert_eq!(vb.source(), lv::Source::Broker);
}

/// 判据 6：行是"按键缓存复用"的 —— 同一地址的行**不重建**，源表里没有的行被删。
///
/// 真值（arm64）：`0x451108` / `0x452320` 的 `hashbrown::rustc_entry`（每分支一次 =
/// 逐行 `self.rows.entry(addr)`）；清理循环 `0x452018`–`0x452110` 对每条
/// `get_game_info(addr) == null` 的记录调 `Node::queue_free`（`0x7c9338`）。
#[test]
fn rows_are_cached_by_address_and_reused() {
    let mut vb = lv::LanVBox::new();
    let a = v4(1, 35245);
    let b = v4(2, 35245);

    let plan = vb.sync_rows(&[a, b]);
    assert_eq!(
        plan.actions,
        vec![lv::RowAction::Create, lv::RowAction::Create]
    );
    assert_eq!(vb.row_count(), 2);

    // 第二帧：两个都命中缓存 ⇒ Refresh，行 id 不变
    let id_a = vb.row_of(&a).unwrap();
    let plan = vb.sync_rows(&[a, b]);
    assert_eq!(
        plan.actions,
        vec![lv::RowAction::Refresh, lv::RowAction::Refresh]
    );
    assert_eq!(vb.row_of(&a), Some(id_a));

    // 源里少了 b ⇒ b 的行被 queue_free，a 留着
    let plan = vb.sync_rows(&[a]);
    assert_eq!(plan.stale, vec![b]);
    assert_eq!(vb.row_count(), 1);
    assert!(vb.row_of(&b).is_none());

    // 同一地址重复出现也不会建第二行（键就是地址 ⇒ 天然去重）
    let plan = vb.sync_rows(&[a]);
    assert_eq!(plan.actions, vec![lv::RowAction::Refresh]);
    assert_eq!(vb.row_count(), 1);

    // IPv6 地址同样是合法键（LAN 广播实测是 IPv6 多播，docs/07 §1 / docs/09 §1.1）
    let plan = vb.sync_rows(&[a, v6()]);
    assert_eq!(plan.actions.last(), Some(&lv::RowAction::Create));
}

/// 判据 7：**不排序** —— 显示顺序 = 源表迭代顺序，逐字保留。
///
/// 真值：`process`（x86 跨度 16 640 B / arm64 14 524 B）里 **0 处** `sort*` 调用，
/// 也没有 `BTreeMap`；行的插入顺序 = `HashMap<SocketAddr, …>` 的迭代顺序 =
/// `add_child` 的顺序（`0x45126c`）。⇒ 顺序是 hash 顺序，**不是**按房名/人数/状态。
#[test]
fn no_sorting_the_frame_order_is_the_source_order() {
    let existing: HashMap<SocketAddr, u64> =
        [(v4(3, 1), 30u64), (v4(1, 1), 10u64)].into_iter().collect();
    // 传入一个"看起来该被排掉"的顺序：必须原样保留
    let source = vec![v4(9, 1), v4(1, 1), v4(3, 1), v4(2, 1)];
    let plan = lv::plan_frame(source.iter(), &existing);
    assert_eq!(plan.order, source);
    assert_eq!(
        plan.actions,
        vec![
            lv::RowAction::Create,
            lv::RowAction::Refresh,
            lv::RowAction::Refresh,
            lv::RowAction::Create,
        ]
    );
    // 名字/人数完全不参与排序：plan_frame 根本不看 `RoomEntry`
    assert!(lv::ORDER_IS_HASH_ORDER);
}

/// 判据 8：人数文本是 `"  {n}  "`（前后各 2 个空格）。
///
/// 真值：`0x4517bc` `mov w9,#0x2020` + `strh w9,[x8,x1]`（两个 `' '` 字节）然后
/// `Label::set_text`（`0x42008c`）；真机 dump 的 `PlayersContainer/PlayerCount` 字段值
/// 就是 `"  0  "` / `"  1  "`（`docs/16-client.md` §160）。
#[test]
fn player_count_is_padded_with_two_spaces_on_each_side() {
    assert_eq!(row("US Room 3", 0, false).player_count_text(), "  0  ");
    assert_eq!(row("EU Room 4", 5, false).player_count_text(), "  5  ");
    assert_eq!(row("x", 12, false).player_count_text(), "  12  ");
    assert_eq!(lv::PLAYER_COUNT_PAD, 2);
    // 去掉两侧空格就是线上字段 `current_player_count` 的十进制写法
    assert_eq!(row("x", 12, false).player_count_text().trim(), "12");
}

/// 判据 9：`locked` → `JoinButton.disabled`；9 字段里只有一个布尔。
///
/// 真值：`0x4515a4` `ldurb w1,[x8,#-0xa]`（从记录里取一个字节）→ `0x4515ac`
/// `BaseButton::set_disabled`（`0x7b8ac0`）。9 字段 `GameInfo`（`docs/09` §1.1.1：
/// `room_name`/`game_state`/`current_player_count`/`address`/`axis_mode`/`function_mode`/
/// `turn_mode`/`time_mode`/`locked`）里唯一的布尔是 `locked` ⇒ 语义是**推断**。
#[test]
fn join_button_is_disabled_for_locked_rooms() {
    assert!(row("a", 1, false).join_enabled());
    assert!(!row("b", 2, true).join_enabled());
    assert_eq!(lv::CONSUMED_FIELDS, 7, "9 字段里本模块读 7 个");
}

/// 判据 10：Join 走自己的通道，**下一帧**才连（不是点一下就直连）。
///
/// 真值：`init` 建 `mpmc` 通道（载荷类型见 `drop_in_place<…Channel<SocketAddr>>`，`0x433b54`）；
/// 点 Join 的闭包（arm64 `0x449124`，56 B 的 boxed 闭包，环境 = Sender16 + addr32 + ptr8）
/// 在 `0x4491d8` 调 `Sender::send`；`process` ①（`0x450c54` 循环）排空后调
/// `GraphwarBackend::start_client_to_remote_server`（`0x407b9c`）。
/// ⇒ 通道**不去重**（连点两次就是两条），但同一帧最多发两次连接竞态由 ① 的顺序决定。
#[test]
fn join_requests_are_queued_and_drained_next_frame() {
    let mut vb = lv::LanVBox::new();
    let a = v4(7, 35245);
    vb.press_join(a);
    assert_eq!(vb.pending_join_requests(), 1);
    vb.press_join(v6());
    assert_eq!(vb.pending_join_requests(), 2);
    let drained = vb.drain_join_requests();
    assert_eq!(drained, vec![a, v6()], "顺序 = 投递顺序");
    assert_eq!(vb.pending_join_requests(), 0);
    assert!(vb.drain_join_requests().is_empty());
}

/// 判据 11：不可见 ⇒ 整帧不刷（`is_visible_in_tree` 闸门）。
///
/// 真值：`0x450dd8` `CanvasItem::is_visible_in_tree` → `0x450ddc` `tbz w0,#0,0x4533d4`
/// （直接返回，一行都不碰）。LanLobby 的 Local/Global 两页是同一个控件，切页时靠这道闸门。
#[test]
fn invisible_vbox_skips_the_whole_refresh() {
    let mut vb = lv::LanVBox::new();
    vb.set_visible(false);
    let plan = vb.sync_rows(&[v4(1, 1), v4(2, 1)]);
    assert!(plan.order.is_empty() && plan.actions.is_empty() && plan.stale.is_empty());
    assert_eq!(vb.row_count(), 0);
    vb.set_visible(true);
    let plan = vb.sync_rows(&[v4(1, 1)]);
    assert_eq!(plan.actions, vec![lv::RowAction::Create]);
    assert_eq!(vb.row_count(), 1);
}

/// 判据 12：行场景与 7 个节点名逐字对上，且 x86 侧引用它们的指令**全在 `process` 里**。
///
/// 真值：arm64 `.rodata` —— `res://game_room_entry.tscn` `0x19dd52`(26 B)、
/// `JoinButton` `0x19dd01`(10)、`PlayerCount` `0x19dd0b`(11)、`AxisModeButton` `0x19dd16`(14)、
/// `FunctionModeButton` `0x19dd24`(18)、`TurnModeButton` `0x19dd36`(14)、
/// `TimeModeButton` `0x19dd44`(14)、`RoomName` `0x179000`(8)。
/// x86 `.rodata` —— 场景 `0x1c43e5`、`JoinButton` `0x1c4394`、`PlayerCount` `0x1c439e`、
/// `RoomName` `0x19ef98`。
/// x86 全 `.text` 的交叉引用：场景 **2** 条（`0x4f97ed`/`0x4fac67`，两分支各一）、
/// `JoinButton` **4** 条（`0x4f9942`/`0x4f9cef`/`0x4fadc1`/`0x4fb183`，每分支两处）、
/// `RoomName` 2 条（`0x4f9db9`/`0x4fb24f`）、`PlayerCount` 2 条（`0x4f9e7c`/`0x4fb30a`）——
/// **全部落在 `process`（`0x4f90b0`–`0x4fd1b0`）内**，这是"两条分支各一套"的独立证据，
/// 与 arm64 的两份副本逐条相符。
#[test]
fn row_scene_and_node_names_are_the_observed_literals() {
    assert_eq!(lv::ROW_SCENE, "res://game_room_entry.tscn");
    assert_eq!(lv::ROW_SCENE.len() as u32, lv::ROW_SCENE_LEN);
    assert_eq!(lv::ROW_SCENE_LEN, 0x1a);
    let got: Vec<&str> = lv::ROW_NODE_NAMES.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        got,
        vec![
            "JoinButton",
            "RoomName",
            "PlayerCount",
            "AxisModeButton",
            "FunctionModeButton",
            "TurnModeButton",
            "TimeModeButton",
        ]
    );
    for (name, len) in lv::ROW_NODE_NAMES.iter() {
        assert_eq!(name.len() as u32, *len, "{name}");
    }
    // 行场景的两条 load 与 JoinButton 的四条引用都在 process 里，且不越界
    let lo = lv::PROCESS_X86;
    let hi = lv::PROCESS_X86 + u64::from(lv::PROCESS_X86_SPAN);
    for a in lv::ROW_SCENE_X86_REFS.iter().chain(lv::JOIN_BUTTON_X86_REFS.iter()) {
        assert!(*a > lo && *a < hi, "{a:#x} 不在 process 里");
    }
    assert_eq!(lv::ROW_SCENE_X86_REFS, [0x4f97ed, 0x4fac67]);
    assert_eq!(
        lv::JOIN_BUTTON_X86_REFS,
        [0x4f9942, 0x4f9cef, 0x4fadc1, 0x4fb183]
    );
    // 两条分支 ⇒ 场景字面量正好 2 条引用
    assert_eq!(lv::ROW_SCENE_X86_REFS.len(), 2);
}

/// 判据 13：本模块**不消费事件** —— 74 个事件里一个都不在这里。
///
/// 真值：`process` 的 `bl` 目标里没有 `EventHandler` / `subscribe_to_events*` / `Event` 判别值
/// 比较；`.symtab` 里也**没有** `ready`（`IVBoxContainer` 只实现了 `init` 与 `process`）。
/// 房间数据由 broadcaster / broker 各自维护，本模块每帧直接读它们的表
/// （`docs/144` §1.1 的运行期结论 + 本文的两处 `get_game_info`）。
#[test]
fn this_module_consumes_no_events_only_two_maps() {
    // 只有一个 ready-less 的实现：init + process
    assert!(lv::function_named("<GraphwarLanVBox as IVBoxContainer>::init").is_some());
    assert!(lv::function_named("<GraphwarLanVBox as IVBoxContainer>::process").is_some());
    assert!(lv::function_named("<GraphwarLanVBox as IControl>::ready").is_none());
    assert!(lv::function_named("<GraphwarLanVBox as ILineEdit>::ready").is_none());
    // 表的取法：每帧现取（不是缓存快照）
    let mut vb = lv::LanVBox::new();
    let a = v4(1, 1);
    vb.sync_rows(&[a]);
    // 源表把这一行删掉 ⇒ 下一帧就没了（原版 ⑤ 步再查一次源表）
    assert_eq!(vb.sync_rows(&[]).stale, vec![a]);
    assert_eq!(vb.row_count(), 0);
}

/// 判据 14：未知项与"与 client 的出入"都被登记，且没有被写成结论。
#[test]
fn unknowns_and_client_diffs_are_registered() {
    assert_eq!(lv::UNKNOWNS.len(), 6);
    assert_eq!(lv::CLIENT_DIFF_COUNT, 6);
    // 每条未知都必须带档位词（未知 / 未定 / 推断），不许写成陈述句
    for u in lv::UNKNOWNS.iter() {
        assert!(
            u.contains("未知") || u.contains("未定") || u.contains("推断"),
            "未标档位：{u}"
        );
    }
    assert_eq!(lv::CLASS, "GraphwarLanVBox");
    assert_eq!(lv::BASE_CLASS, "VBoxContainer");
    assert_eq!(lv::SOURCE_FILE, "src/graphwar_lan_vbox.rs");
    assert_eq!(lv::ROOT_NODE_NAME, "GraphwarRootNode");
}
