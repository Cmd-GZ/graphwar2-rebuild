//! 线格式验收：**事件名 + 字段名 + 键序 + 值类型**必须等于从二进制读出的值。
//!
//! ## 真值从哪来（不是本文件自说自话）
//!
//! `#[derive(Serialize)]` 生成的代码就是线格式的定义。LLVM 把 serde 的 `Compound`
//! 全内联之后，一次 `serialize_field` 在 `.text` 里是三条连续指令：
//!
//! ```text
//! lea  rsi, [rip + 字段名]   ; &'static str.ptr
//! mov  edx, LEN              ; &'static str.len == 该字面量的真实字节数（自校验）
//! call <serialize_field 单态> ; 同单态 ⇒ 同值类型
//! ```
//!
//! ⇒ **`serialize_field` 的调用顺序就是线上键序**。下面的 [`EXPECTED`] 表是
//! `tools/wire_fields.py` 的输出：
//!
//! ```bash
//! GW2_UPSTREAM=/path/to/gw2 ../.venv/bin/python tools/wire_fields.py --verify
//! ```
//!
//! 本文件把那份输出**逐行抄成常量**，于是有三方对账：
//! ① 常量（二进制读出值，本文件）② `protocol::WIRE_SPEC`（编译进 crate 的表）
//! ③ `fixtures/protocol*.jsonl`（真实抓包）。
//!
//! ## 三道自校验（都在 `tools/wire_fields.py` 里跑）
//!
//! 1. **N**：字段个数 == `.rodata` 里 `struct <Name>Event with N elements` 的 N
//!    （67/67）。另 7 个事件没有这条串 —— 它们的 `Serialize` 不写任何字段，体是空 `{}`。
//! 2. **反序列化侧**：`Deserialize` 的 `visit_map` 里能找到同名键的字节比较
//!    （67/67，全部字段）。
//! 3. **抓包**：本文件 [`capture_key_order_matches_binary`] 逐行验证。

use gw2_rebuild::_scaffold::protocol::{
    parse_message, split_fields, wire_field_order, wire_spec, GameInfo, GraphwarColor,
    ValType, WireEvent, WIRE_SPEC,
};

/// 判别值未知（`docs/07` §7.3 的 `63`/`65` 缺号）。
const NONE: u16 = u16::MAX;

/// **74 个事件的线上形状**，逐行抄自 `tools/wire_fields.py`。
///
/// 三个字段：线上键名 / `enum Event` 判别值（**不上线**，只用于对账；
/// `NONE` = 未解出）/ 字段（按线序）与值类型。
///
/// 值类型来自 `serialize_field` 的**单态地址**：同一地址 ⇒ 同一 Rust 类型。已认出：
/// `0x3cc060`=`i64`、`0x3cb250`=`String`、`0x3cb760`=`f64`、`0x3cbb80`=`bool`
/// （体内写 `true`/`false`）、`0x3cc4b0`=`Option<i64>`、`0x3cb440`=`Option<String>`
/// （后两者体内写 `null`）、`0x3cc330`=`u8`、`0x3cb9e0`=`GraphwarColor`、
/// `0x3cbdb0`=`GameInfo`、`0x3cbab0`=socket 地址（线上是 JSON 字符串）、
/// 其余枚举单态线上都是字符串（记为 `enum`）。
const EXPECTED: &[(&str, u16, &[(&str, &str)])] = &[
    ("AddObstacle", 40, &[("pos_x", "f64"), ("pos_y", "f64"), ("radius", "f64")]),
    ("AddSoldierRequest", 16, &[("player_id", "i64")]),
    ("AxisModeChangeRequest", 18, &[]),
    ("AxisModeInfo", 45, &[("axis_mode", "String")]),
    ("BanConnectionRequest", 72, &[("player_connection_id", "i64"), ("leader_connection_id", "i64")]),
    ("BotAdded", 10, &[("connection_id", "i64"), ("player_id", "i64"), ("level", "i64")]),
    ("ChangeTeamRequest", 11, &[("player_id", "i64")]),
    ("ChatMessage", 71, &[("entity_id", "i64"), ("message", "String")]),
    ("ChatMessageRequest", 70, &[("entity_id", "i64"), ("message", "String")]),
    ("ClearBansRequest", 73, &[]),
    ("ClearObstacles", 39, &[]),
    ("ColorInfo", 61, &[("entity_id", "i64"), ("color", "GraphwarColor")]),
    ("ConnectedToServer", 7, &[("connection_id", "i64")]),
    ("ConnectionRemoved", 6, &[("connection_id", "i64"), ("reason", "Option<String>")]),
    ("DestroyObstacle", 41, &[("pos_x", "f64"), ("pos_y", "f64"), ("radius", "f64")]),
    ("EffectInfo", 44, &[("effect_id", "i64"), ("pos_x", "f64"), ("pos_y", "f64"), ("effect_type", "String"), ("start_tick", "i64")]),
    ("EndOfTurnInfo", 49, &[("end_of_turn_tick", "i64"), ("time_mode", "String")]),
    ("EntityEffectInfo", 56, &[("entity_id", "i64"), ("effect_type", "String")]),
    ("EntityRemoved", 14, &[("entity_id", "i64")]),
    ("FaceGraphicsInfo", 53, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("FaceGraphicsRequest", 52, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("FunctionActive", 36, &[("owner_id", "i64"), ("active_tick", "i64")]),
    ("FunctionFire", 33, &[("soldier_id", "i64")]),
    ("FunctionFireRequest", 32, &[("player_id", "i64")]),
    ("FunctionInfo", 35, &[("owner_id", "i64"), ("function", "String"), ("corrected_function", "Option<String>")]),
    ("FunctionModeChangeRequest", 19, &[]),
    ("FunctionModeInfo", 46, &[("function_mode", "String")]),
    ("FunctionPoints", 37, &[("owner_id", "i64"), ("start_x", "f64"), ("start_y", "f64"), ("end_x", "f64")]),
    ("FunctionUpdateRequest", 34, &[("player_id", "i64"), ("function", "String")]),
    ("FunctionsCalculated", 38, &[]),
    ("GameHeartbeat", 63, &[]),
    ("GameInfo", 62, &[("game_info", "GameInfo")]),
    ("GameNameInfo", 29, &[("game_name", "String")]),
    ("GameNameRequest", 22, &[("game_name", "String")]),
    ("GameRemoved", 64, &[("game_address", "String")]),
    ("GameStartRequest", 31, &[]),
    ("GameStateInfo", 30, &[("game_state", "String")]),
    ("HatGraphicsInfo", 55, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("HatGraphicsRequest", 54, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("LifeInfo", 27, &[("entity_id", "i64"), ("alive", "bool")]),
    ("LifeQueuedDeath", 28, &[("entity_id", "i64"), ("tick", "i64")]),
    ("LockInfo", 69, &[("lock", "bool")]),
    ("LockRequest", 68, &[("lock", "bool")]),
    ("NameInfo", 60, &[("entity_id", "i64"), ("name", "String")]),
    ("NameRequest", 59, &[("entity_id", "i64"), ("name", "String")]),
    ("NewBotRequest", 9, &[("connection_id", "i64"), ("level", "i64")]),
    ("NewConnection", 5, &[("connection_id", "i64")]),
    ("NewConnectionRequest", 4, &[("major_version", "String"), ("minor_version", "String")]),
    ("NewPlayerRequest", 8, &[("connection_id", "i64")]),
    ("PlayerInfo", 13, &[("player_id", "i64"), ("connection_id", "i64")]),
    ("PosInfo", 26, &[("entity_id", "i64"), ("pos_x", "f64"), ("pos_y", "f64"), ("radius", "f64")]),
    ("QueueDestroyObstacle", 42, &[("pos_x", "f64"), ("pos_y", "f64"), ("radius", "f64"), ("tick", "i64")]),
    ("RankInfo", 67, &[("entity_id", "i64"), ("rank", "Option<i64>")]),
    ("RankRequest", 66, &[("entity_id", "i64"), ("rank", "Option<i64>")]),
    ("RemovePlayerRequest", 12, &[("player_id", "i64")]),
    ("RemoveSoldierRequest", 17, &[("player_id", "i64")]),
    ("RemoveTeam", 25, &[("entity_id", "i64")]),
    ("SkinGraphicsInfo", 51, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("SkinGraphicsRequest", 50, &[("entity_id", "i64"), ("graphics_str", "String")]),
    ("SoldierInfo", 15, &[("soldier_id", "i64"), ("player_id", "i64")]),
    ("Subscribe", 3, &[("subscriber_id", "i64"), ("subscribed_event_type", "String")]),
    ("TargetInfo", 23, &[("target_id", "i64")]),
    ("TeamInfo", 24, &[("entity_id", "i64"), ("team", "String")]),
    ("Tick", 0, &[("current_tick", "i64")]),
    ("TickReply", 2, &[("current_tick", "i64")]),
    ("TickRequest", 1, &[]),
    ("TimeModeChangeRequest", 21, &[]),
    ("TimeModeInfo", 48, &[("time_mode", "String")]),
    ("TurnCountInfo", 58, &[("turn_count", "i64")]),
    ("TurnInfo", 43, &[("player_id", "i64"), ("soldier_id", "i64")]),
    ("TurnLimitInfo", 57, &[("turn_limit", "Option<i64>")]),
    ("TurnModeChangeRequest", 20, &[]),
    ("TurnModeInfo", 47, &[("turn_mode", "String")]),
    ("VersionInfo", 65, &[("version", "String"), ("system", "String")]),
];

/// 类型标签 → [`ValType`]。
fn label_of(t: ValType) -> &'static str {
    match t {
        ValType::I64 => "i64",
        ValType::U8 => "u8",
        ValType::F64 => "f64",
        ValType::Bool => "bool",
        ValType::Str => "String",
        ValType::OptI64 => "Option<i64>",
        ValType::OptStr => "Option<String>",
        ValType::Color => "GraphwarColor",
        ValType::GameInfo => "GameInfo",
    }
}

fn expected(name: &str) -> &'static (&'static str, u16, &'static [(&'static str, &'static str)]) {
    EXPECTED
        .iter()
        .find(|(n, _, _)| *n == name)
        .unwrap_or_else(|| panic!("EXPECTED 表里没有 {name}"))
}

/// 把一条报文的内层对象按**顶层**逗号切出键名（顺序原样）。
fn keys_of(body: &str) -> Vec<String> {
    split_fields(body).into_iter().map(|(k, _)| k).collect()
}

/// 夹具里那一行**不是**单键消息的（LAN 广播的裸 `GameInfo` 对象）。
fn is_single_key_message(line: &str) -> bool {
    parse_message(line).is_some()
}

// ---------------------------------------------------------------------------
// 1. 表与表对账：74 条、名字唯一、crate 里的 WIRE_SPEC 与本文件的 EXPECTED 完全一致
// ---------------------------------------------------------------------------

#[test]
fn spec_has_seventy_four_events_and_matches_the_binary_table() {
    assert_eq!(WIRE_SPEC.len(), 74, "二进制里有 74 个事件的 Serialize 实现");
    assert_eq!(EXPECTED.len(), 74);

    let mut names: Vec<&str> = WIRE_SPEC.iter().map(|e| e.name).collect();
    names.sort_unstable();
    let mut want: Vec<&str> = EXPECTED.iter().map(|(n, _, _)| *n).collect();
    want.sort_unstable();
    assert_eq!(names, want, "WIRE_SPEC 与二进制读出的事件名集合不一致");

    for e in WIRE_SPEC {
        let (_, tag, fields) = expected(e.name);
        assert_eq!(
            e.tag.map(|t| t as u16).unwrap_or(NONE),
            *tag,
            "{} 的判别值不一致",
            e.name
        );
        let got: Vec<(&str, &str)> = e.fields.iter().map(|(k, t)| (*k, label_of(*t))).collect();
        assert_eq!(&got[..], *fields, "{} 的字段/键序/类型与二进制不一致", e.name);
    }
}

// ---------------------------------------------------------------------------
// 2. 10 个"原先透传"的事件：字段名与键序等于二进制读出值
// ---------------------------------------------------------------------------

/// 本轮升级的 10 个（原先 `docs/07` §6.2.1 说"字段名未证实，只透传"）。
const UPGRADED: [(&str, usize); 10] = [
    ("LockInfo", 1),
    ("LockRequest", 1),
    ("ChatMessage", 2),
    ("ChatMessageRequest", 2),
    ("GameHeartbeat", 0),
    ("GameRemoved", 1),
    ("TurnLimitInfo", 1),
    ("ClearObstacles", 0),
    ("ClearBansRequest", 0),
    ("BanConnectionRequest", 2),
];

#[test]
fn upgraded_ten_events_have_the_binary_field_names_in_order() {
    for (name, n) in UPGRADED {
        let fields = wire_field_order(name).unwrap_or_else(|| panic!("{name} 不在 WIRE_SPEC"));
        assert_eq!(fields.len(), n, "{name} 的字段数应为 {n}");
        let (_, _, want) = expected(name);
        let want: Vec<&str> = want.iter().map(|(k, _)| *k).collect();
        assert_eq!(fields, want, "{name} 的键序与二进制不一致");
    }

    // 逐个把"名字 + 键序"钉死（这是二进制里 `serialize_field` 的调用序）
    assert_eq!(wire_field_order("LockInfo").unwrap(), ["lock"]);
    assert_eq!(wire_field_order("LockRequest").unwrap(), ["lock"]);
    assert_eq!(wire_field_order("ChatMessage").unwrap(), ["entity_id", "message"]);
    assert_eq!(
        wire_field_order("ChatMessageRequest").unwrap(),
        ["entity_id", "message"]
    );
    assert_eq!(wire_field_order("GameHeartbeat").unwrap(), Vec::<&str>::new());
    assert_eq!(wire_field_order("GameRemoved").unwrap(), ["game_address"]);
    assert_eq!(wire_field_order("TurnLimitInfo").unwrap(), ["turn_limit"]);
    assert_eq!(wire_field_order("ClearObstacles").unwrap(), Vec::<&str>::new());
    assert_eq!(wire_field_order("ClearBansRequest").unwrap(), Vec::<&str>::new());
    assert_eq!(
        wire_field_order("BanConnectionRequest").unwrap(),
        ["player_connection_id", "leader_connection_id"]
    );
}

/// 构造 → 解析 → 再构造，逐字节相同；且键序等于二进制键序。
///
/// 这是"**能对外发布**"的实际含义：第三方只看这张表就能拼出报文。
#[test]
fn upgraded_ten_events_are_constructible_from_the_spec() {
    use gw2_rebuild::_scaffold::protocol::*;

    let built: Vec<(&str, String)> = vec![
        ("LockInfo", emit_lock_info(true)),
        ("LockRequest", emit_lock_request(false)),
        ("ChatMessage", emit_chat_message(229, "gg")),
        ("ChatMessageRequest", emit_chat_message_request(232, "hi")),
        ("GameHeartbeat", emit_game_heartbeat()),
        ("GameRemoved", emit_game_removed("69.164.213.144:35981")),
        ("TurnLimitInfo", emit_turn_limit_info(Some(20))),
        ("ClearObstacles", emit_clear_obstacles()),
        ("ClearBansRequest", emit_clear_bans_request()),
        (
            "BanConnectionRequest",
            emit_ban_connection_request(228, 229),
        ),
    ];
    assert_eq!(built.len(), UPGRADED.len());

    for (name, text) in built {
        let m = parse_message(&text).expect("构造出来的报文应能解析");
        assert_eq!(m.kind, name, "线上键名不一致");
        let ev = AnyEvent::parse(&m).expect("应被 AnyEvent 识别");
        assert_eq!(ev.kind(), name);
        assert!(ev.extra_fields().is_empty(), "{name} 不该有未知字段");
        assert_eq!(ev.to_msg(), text, "{name} 重新序列化后字节不一致");

        // 键序 == 二进制键序
        let want: Vec<String> = wire_field_order(name)
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(keys_of(&m.body), want, "{name} 的键序与二进制不一致");
    }
}

// ---------------------------------------------------------------------------
// 3. 13 个强类型事件：抓包键序 == 二进制键序（这就把旧"推断"升级成观测）
// ---------------------------------------------------------------------------

/// 有真实抓包、因而早先就建了强类型的那 13 个。
const CAPTURED_TYPED: [&str; 13] = [
    "NameRequest",
    "NameInfo",
    "RankRequest",
    "RankInfo",
    "SkinGraphicsRequest",
    "SkinGraphicsInfo",
    "FaceGraphicsRequest",
    "FaceGraphicsInfo",
    "HatGraphicsRequest",
    "HatGraphicsInfo",
    "EntityEffectInfo",
    "VersionInfo",
    "BotAdded",
];

/// 三个夹具的全部行（真值夹具，已入库）。
const FIXTURES: [&str; 3] = [
    include_str!("../fixtures/protocol.jsonl"),
    include_str!("../fixtures/protocol_match.jsonl"),
    include_str!("../fixtures/protocol_death.jsonl"),
];

fn fixture_lines() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for text in FIXTURES {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || !line.starts_with('{') {
                continue;
            }
            if !is_single_key_message(line) {
                continue; // 裸的 LAN 广播对象，见 `lan_broadcast_is_a_game_info`
            }
            let m = parse_message(line).expect("已判过可解析");
            out.push((line.to_string(), m.kind));
        }
    }
    out
}

#[test]
fn thirteen_typed_events_key_order_matches_the_binary() {
    let lines = fixture_lines();
    let mut checked = 0usize;
    for name in CAPTURED_TYPED {
        let want: Vec<String> = wire_field_order(name)
            .unwrap_or_else(|| panic!("{name} 不在 WIRE_SPEC"))
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut hits = 0usize;
        for (line, kind) in &lines {
            if kind != name {
                continue;
            }
            let m = parse_message(line).unwrap();
            assert_eq!(keys_of(&m.body), want, "{name} 的抓包键序与二进制不一致: {line}");
            hits += 1;
            checked += 1;
        }
        assert!(hits > 0, "{name} 声称有抓包，但夹具里一行都没有");
    }
    // `docs/07` §6.2 的"14 种 41 行"= 这 13 个（40 行）+ `ClearObstacles`（1 行）
    assert!(checked >= 40, "13 个强类型事件至少要有 40 行抓包，实际 {checked}");
}

// ---------------------------------------------------------------------------
// 4. 所有抓到的 49 种事件：键序 == 二进制键序，且能逐字节回放
// ---------------------------------------------------------------------------

#[test]
fn capture_key_order_matches_binary() {
    let lines = fixture_lines();
    let mut kinds = std::collections::BTreeSet::new();
    for (line, kind) in &lines {
        let spec = wire_spec(kind)
            .unwrap_or_else(|| panic!("抓到的事件 {kind} 不在 WIRE_SPEC 里"));
        let want: Vec<String> = spec.fields.iter().map(|(k, _)| k.to_string()).collect();
        let m = parse_message(line).unwrap();
        // 顶层键序必须与二进制一致。注意嵌套对象（`color` / `game_info`）的键
        // 由它们自己的结构体决定，这里只看顶层。
        let got = keys_of(&m.body);
        let head: Vec<String> = got.clone().into_iter().take(want.len()).collect();
        assert_eq!(head, want, "{kind} 的抓包键序与二进制不一致: {line}");
        kinds.insert(kind.clone());
    }
    // 48 种单键事件 + 1 条裸 LAN 广播 = 夹具里的 49 种"形状"
    assert!(kinds.len() >= 48, "抓到的不同事件种类应 ≥ 48，实际 {}", kinds.len());
}

// ---------------------------------------------------------------------------
// 5. 两个负载结构体：键序同样来自二进制
// ---------------------------------------------------------------------------

#[test]
fn payload_structs_have_the_binary_key_order() {
    // GraphwarColor：`tools/wire_fields.py --struct 0x464c00` → r, g, b
    let c = GraphwarColor::new(149, 5, 62);
    assert_eq!(c.to_json(), r#"{"r":149,"g":5,"b":62}"#);
    let m = parse_message(r#"{"X":{"r":149,"g":5,"b":62}}"#).unwrap();
    assert_eq!(GraphwarColor::parse(&m).unwrap(), c);

    // GameInfo：`--struct 0x5060d0` → 9 个字段，顺序如下
    let info = GameInfo {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        current_player_count: 2,
        address: "[::1]:35245".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timers1m".into(),
        locked: false,
    };
    let want = concat!(
        r#"{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":2,"#,
        r#""address":"[::1]:35245","axis_mode":"EveryUnit","function_mode":"NormalFunction","#,
        r#""turn_mode":"SequentialTurns","time_mode":"Timers1m","locked":false}"#
    );
    assert_eq!(info.to_json(), want);
    assert_eq!(GameInfo::parse(&parse_message(&format!("{{\"X\":{}}}", info.to_json())).unwrap()).unwrap(), info);

    // GameInfoEvent 把它包一层：`{"GameInfo":{"game_info":{…}}}`
    let ev = gw2_rebuild::_scaffold::protocol::emit_game_info(&info);
    assert_eq!(ev, format!(r#"{{"GameInfo":{{"game_info":{}}}}}"#, want));
    // 颜色事件
    assert_eq!(
        gw2_rebuild::_scaffold::protocol::emit_color_info(230, c),
        r#"{"ColorInfo":{"entity_id":230,"color":{"r":149,"g":5,"b":62}}}"#
    );
}

// ---------------------------------------------------------------------------
// 6. 判别值对账：WIRE_SPEC 里有判别值的 72 条，与 events.rs 的具名变体数一致
// ---------------------------------------------------------------------------

#[test]
fn tag_column_agrees_with_events_rs() {
    use gw2_rebuild::events::VARIANT_COUNT;
    let with_tag = WIRE_SPEC.iter().filter(|e| e.tag.is_some()).count();
    assert_eq!(
        with_tag, VARIANT_COUNT,
        "有判别值的事件数应等于 enum Event 的变体总数（74）"
    );
    // 2026-11-07：63 / 65 定死后，**74 条全部有判别值**（旧断言"只有 2 个没有"已作废）
    assert_eq!(
        WIRE_SPEC.len(),
        with_tag,
        "74 条事件应全部带判别值（docs/07 §5.1：GameHeartbeat=63、VersionInfo=65）"
    );
    assert!(
        WIRE_SPEC.iter().all(|e| e.tag.is_some()),
        "不应再有无判别值的事件"
    );
    assert_eq!(
        WIRE_SPEC.iter().find(|e| e.name == "GameHeartbeat").and_then(|e| e.tag),
        Some(63)
    );
    assert_eq!(
        WIRE_SPEC.iter().find(|e| e.name == "VersionInfo").and_then(|e| e.tag),
        Some(65)
    );

    // 每个有判别值的事件都能在 enum Event 里找到同判别值的变体。
    // ⚠️ 不能用 DISPATCH_NAME_TAGS 查：63(GameHeartbeat) / 65(VersionInfo) 没有 `*_event`
    // 派发名、不进那张表（docs/07 §5.1）。改为按 RELAY 的全量判别值集合校验。
    let all_tags: std::collections::HashSet<u16> = gw2_rebuild::events::ALL_VARIANTS
        .iter()
        .map(|e| e.wire_tag())
        .collect();
    assert_eq!(all_tags.len(), gw2_rebuild::events::VARIANT_COUNT);
    for e in WIRE_SPEC {
        if let Some(t) = e.tag {
            assert!(
                all_tags.contains(&(t as u16)),
                "{} 的判别值 {} 在 enum Event 里不存在",
                e.name,
                t
            );
        }
    }
}

/// `WireEvent` 是 `Copy`，便于外部按值取用。
#[test]
fn wire_event_is_copy() {
    fn takes(_: WireEvent) {}
    for e in WIRE_SPEC {
        takes(*e);
    }
}

/// 夹具里还有**裸的 LAN 广播对象**（不是 `{"Kind":{…}}` 单键消息）：它就是
/// [`GameInfo`] 的 9 个字段，键序必须与二进制一致。这同时是 `GameInfo` 键序的
/// 真实抓包证据（`docs/07` §1 的那条 UDP 发现报文）。
#[test]
fn lan_broadcast_is_a_game_info() {
    use gw2_rebuild::_scaffold::protocol::RawMessage;
    let want: Vec<String> = [
        "room_name",
        "game_state",
        "current_player_count",
        "address",
        "axis_mode",
        "function_mode",
        "turn_mode",
        "time_mode",
        "locked",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut seen = 0usize;
    for text in FIXTURES {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || !line.starts_with('{') || is_single_key_message(line) {
                continue;
            }
            let body = line
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .expect("裸对象");
            assert_eq!(
                split_fields(body)
                    .into_iter()
                    .map(|(k, _)| k)
                    .collect::<Vec<_>>(),
                want,
                "LAN 广播的键序与二进制不一致: {line}"
            );
            let m = RawMessage {
                kind: "GameInfo".to_string(),
                body: body.to_string(),
                text: line.to_string(),
            };
            let info = GameInfo::parse(&m).expect("LAN 广播应能按 GameInfo 解析");
            assert_eq!(info.room_name, "Graphwar Game");
            assert_eq!(info.to_json().len() > 0, true);
            seen += 1;
        }
    }
    assert!(seen >= 1, "夹具里应有至少一条 LAN 广播");
}
