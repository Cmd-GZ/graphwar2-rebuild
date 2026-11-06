//! `graphwar_local_player_manager` 的验收：**谁是我 / 5 条请求 / `RankRequest`(66) 的发起侧**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `three_subscriptions` | 7/13/14 → 三个处理方法 | **逐位**（三个闭包的 `adr` 目标） |
//! | `connected_records_connection_and_clears` | `+0x18:=1`、`+0x1c:=id`、`+0x10:=0`、发 (8) | **逐位**（`stp w9,w8,[x0,#0x18]` + `str xzr,[x19,#0x10]`） |
//! | `player_info_is_filtered_by_connection` | 只收 `payload+0xc == self+0x1c` 的那些 | **逐位**（两处 `b.ne` 提前返回） |
//! | `five_requests_on_first_entry_only` | 首次入列发 59/50/52/54/66 各一条 | **逐位**（`add x8, x25, #imm`，13+imm） |
//! | `rank_request_has_three_dwords` | (66) 载荷 `{1, level, player_id}`，level 来自关卡 | **逐位**（`stp w9(1),w8` + `csel` 于 `get_current_level`） |
//! | `entity_removed_retains` | `Vec::retain` 掉**全部**相等项 | **逐位**（x86 与 arm64 两边同形；纠 `docs/112` §9.3） |
//! | `module_does_not_emit_function_info` | 35 不在本模块的 5 条里 | **观测**（tag 35 全扫只有 function_manager 两处） |
#![allow(dead_code)]

#[path = "../src/events.rs"]
mod events;
#[path = "../src/graphwar_local_player_manager.rs"]
mod graphwar_local_player_manager;

use graphwar_local_player_manager::*;

fn mgr() -> GraphwarLocalPlayerManager {
    GraphwarLocalPlayerManager::new(LocalPlayerProfile {
        name: "Ann".into(),
        skin: "skin_1".into(),
        face: "face_2".into(),
        hat: "hat_3".into(),
    })
}

/// 三条订阅，判别值 7 / 13 / 14。
#[test]
fn three_subscriptions() {
    assert_eq!(SUBSCRIPTIONS.len(), 3);
    assert_eq!(SUBSCRIPTIONS[0].name, "connected_to_server_event");
    assert_eq!(SUBSCRIPTIONS[0].handler, "handle_connected_to_server");
    assert_eq!(SUBSCRIPTIONS[1].name, "player_info_event");
    assert_eq!(SUBSCRIPTIONS[1].handler, "handle_player_info");
    assert_eq!(SUBSCRIPTIONS[2].name, "entity_removed_event");
    assert_eq!(SUBSCRIPTIONS[2].handler, "handle_entity_removed_event");
    // 三个闭包都是 48 字节的同一形状（`adr x1, <handler>` + `b WeakCaller::call`）。
    assert_eq!(SUBSCRIPTIONS[0].tag, 7);
    assert_eq!(SUBSCRIPTIONS[1].tag, 13);
    assert_eq!(SUBSCRIPTIONS[2].tag, 14);
}

/// 收到 `connected_to_server`：记下连接号、请求建玩家（8）、把 id 列表清空。
#[test]
fn connected_records_connection_and_clears() {
    let mut m = mgr();
    m.player_ids.push(99); // 上一次会话残留
    let payload = m.handle_connected_to_server(4242);
    assert_eq!(payload, 4242, "(8) 的载荷就是那个连接号");
    assert_eq!((m.connected, m.connection_id), (1, 4242));
    assert!(m.player_ids.is_empty(), "+0x10 必须清零");
    assert!(m.is_connected());
}

/// `player_info_event` 只在"已连上 + 连接号相同"时才入列。
#[test]
fn player_info_is_filtered_by_connection() {
    let mut m = mgr();
    // 没连上：忽略。
    assert!(m.handle_player_info(1, 7, None).is_empty());
    assert!(m.player_ids.is_empty());
    m.handle_connected_to_server(7);
    // 连接号不符：忽略。
    assert!(m.handle_player_info(1, 8, None).is_empty());
    assert!(m.player_ids.is_empty());
    // 符了：入列。
    assert_eq!(m.handle_player_info(1, 7, None).len(), 5);
    assert_eq!(m.my_player_id(), Some(1));
}

/// 5 条请求在**第一次**入列时一次发全，顺序固定；第二条 `player_info` 不再发。
#[test]
fn five_requests_on_first_entry_only() {
    let mut m = mgr();
    m.handle_connected_to_server(7);
    let reqs = m.handle_player_info(11, 7, Some(3));
    let tags: Vec<u16> = reqs.iter().map(|r| r.tag()).collect();
    assert_eq!(tags, vec![59, 50, 52, 54, 66]);
    assert_eq!(reqs.iter().map(|r| r.player_id()).collect::<Vec<_>>(), vec![11; 5]);
    // 前四条带本机配置里的字符串；第五条带三个 dword。
    match (&reqs[0], &reqs[1], &reqs[2], &reqs[3]) {
        (
            MetadataRequest::Name { name, .. },
            MetadataRequest::Skin { skin, .. },
            MetadataRequest::Face { face, .. },
            MetadataRequest::Hat { hat, .. },
        ) => {
            assert_eq!(name, "Ann");
            assert_eq!(skin, "skin_1");
            assert_eq!(face, "face_2");
            assert_eq!(hat, "hat_3");
        }
        other => panic!("前四条顺序必须是 名字/皮肤/脸/帽子，得到 {other:?}"),
    }
    assert_eq!(reqs[0].payload_size(), 0x20);
    assert_eq!(reqs[4].payload_size(), 12);

    // 第二次入列：不再发。
    assert!(m.handle_player_info(12, 7, Some(3)).is_empty());
    assert_eq!(m.player_ids, vec![11, 12]);
}

/// (66) 的载荷是三个 dword：`+0x08` 恒 `1`、`+0x0c` = 当前关卡（没有就 0）、`+0x10` = id。
#[test]
fn rank_request_has_three_dwords() {
    assert_eq!(
        (
            RANK_REQUEST_FLAG_OFFSET,
            RANK_REQUEST_LEVEL_OFFSET,
            RANK_REQUEST_PLAYER_OFFSET
        ),
        (0x08, 0x0c, 0x10)
    );
    assert_eq!(RANK_REQUEST_FLAG, 1);

    let mut m = mgr();
    m.handle_connected_to_server(7);
    let reqs = m.handle_player_info(42, 7, Some(9));
    match &reqs[4] {
        MetadataRequest::Rank { flag, level, player_id } => {
            assert_eq!((*flag, *level, *player_id), (1, 9, 42));
        }
        other => panic!("第五条必须是 Rank，得到 {other:?}"),
    }

    // 没有当前关卡 ⇒ level = 0（`csel w8, w1, wzr, ne`）。
    let mut m2 = mgr();
    m2.handle_connected_to_server(7);
    let reqs2 = m2.handle_player_info(42, 7, None);
    match &reqs2[4] {
        MetadataRequest::Rank { level, .. } => assert_eq!(*level, 0),
        other => panic!("第五条必须是 Rank，得到 {other:?}"),
    }
}

/// `entity_removed_event` 把该 id 从 `Vec` 里**全部**删掉（`Vec::retain`）。
///
/// ⚠️ `docs/112` §9.3 说这个函数"只有一条 `panic_fmt` 出口"（空处理）——**错**：
/// x86 `0x51a610` 与 arm64 `0x411c9c` 两边都是 retain 循环（arm64 那一侧
/// `str w13,[x9, x8, lsl #2]` 的写回索引就是 `retain` 的紧凑化）。
#[test]
fn entity_removed_retains() {
    let mut m = mgr();
    m.handle_connected_to_server(7);
    m.player_ids.extend_from_slice(&[5, 6, 5]);
    assert_eq!(m.handle_entity_removed_event(5), 2);
    assert_eq!(m.player_ids, vec![6]);
    assert_eq!(m.handle_entity_removed_event(5), 0, "已经没了 ⇒ 删掉 0 个");
}

/// 本模块的 5 条请求里**没有** `function_info_event`(35)：上行不归它管。
#[test]
fn module_does_not_emit_function_info() {
    let mut m = mgr();
    m.handle_connected_to_server(7);
    let reqs = m.handle_player_info(1, 7, None);
    assert!(reqs.iter().all(|r| r.tag() != 35), "35 必须不在本模块的输出里");
    // 与事件表交叉核对：35 与 66 / 59 是三个不同的变体。
    assert_eq!(events::Event::FunctionInfo as u16, 35);
    assert_ne!(EVENT_RANK_REQUEST, 35);
    assert_ne!(EVENT_NAME_REQUEST, 35);
    // 结构偏移逐条对上。
    assert_eq!(
        (
            PLAYER_IDS_OFFSET,
            CONNECTED_OFFSET,
            CONNECTION_ID_OFFSET,
            EVENT_CHANNEL_OFFSET,
            MODULE_CONTAINER_OFFSET,
            PLAYER_CONFIG_OFFSET
        ),
        (0x00, 0x18, 0x1c, 0x28, 0x30, 0x38)
    );
    assert_eq!(
        (
            CONFIG_NAME_OFFSET,
            CONFIG_SKIN_OFFSET,
            CONFIG_FACE_OFFSET,
            CONFIG_HAT_OFFSET
        ),
        (0x18, 0x30, 0x48, 0x60)
    );
    // `player_info` 的两个 u32 位置（与 `player_manager` 的 (13) 载荷一致）。
    assert_eq!(
        (PLAYER_INFO_PLAYER_OFFSET, PLAYER_INFO_CONNECTION_OFFSET),
        (0x08, 0x0c)
    );
    assert_eq!(
        (STRING_REQUEST_STRING_OFFSET, STRING_REQUEST_PLAYER_OFFSET),
        (0x00, 0x18)
    );
}
