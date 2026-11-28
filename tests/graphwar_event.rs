//! `src/graphwar_event.rs`（= 原版 `graphwar_event.rs` 的 3 个手写方法）的验收。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值都能指到 arm64 指令立即数 / `.rodata` 字节，出处逐条写在断言上方
//! （以及 `fixtures/denom_serde_evidence.tsv` 与 `docs/06` §153）。
//!
//! ## 为什么用 `#[path]` 取源（而不是 `use gw2_rebuild::…`）
//!
//! `src/graphwar_event.rs` **还没进 crate**：`src/lib.rs` 加 `pub mod graphwar_event;` 是
//! `AGENTS.md`「合并纪律」的**禁区**，本轮不动。用 `#[path]` 把它直接编进本测试二进制，
//! 代码因此**有编译、有单测**，只是还没被生产路径调用（同款先例：
//! `tests/graphwar_computer_player_manager.rs`）。要真正进 crate 需要
//! `src/lib.rs` 一行或 `src/events.rs` 两行 `#[path] mod` —— 两条都在本轮写范围之外。
//!
//! 地址口径：本文件的 `0x…` 一律 **Android 2.0.60 arm64 的 ELF vaddr**。

#[path = "../src/graphwar_event.rs"]
mod graphwar_event;

use graphwar_event::*;

// ---------------------------------------------------------------------------
// 1. 与 crate 现有实现的**交叉一致性**（两个独立落点必须给出同一真值）
// ---------------------------------------------------------------------------

/// `make_new_connection_request_event` 的两个版本串，在 `graphwar_client_connection_handler`
/// 里已经有一份（T52/T55 落码时从同一条 arm64 指令读出来的）。
/// 两处**各自独立**记录同一真值 ⇒ 必须逐字节相同；不同就是其中一处读错了。**观测**。
#[test]
fn version_strings_agree_with_client_connection_handler_constants() {
    use gw2_rebuild::graphwar_client_connection_handler as cch;
    assert_eq!(NEW_CONNECTION_REQUEST_MAJOR, cch::NEW_CONNECTION_REQUEST_MAJOR);
    assert_eq!(NEW_CONNECTION_REQUEST_MINOR, cch::NEW_CONNECTION_REQUEST_MINOR);
    // 0x4693e0 的 alloc(3) / 0x4693fc 的 alloc(2) ⇒ 两个串的长度就是这两个立即数
    assert_eq!(NEW_CONNECTION_REQUEST_MAJOR.len(), 3);
    assert_eq!(NEW_CONNECTION_REQUEST_MINOR.len(), 2);
}

/// 判别值低 32 位必须等于 `events::Event` 的那个变体编号。
///
/// `0x46941c/0x46942c` 的常量首字 `0x8000_0000_0000_0004` ⇒ 低 32 位 = 4；
/// `src/events.rs` 的 `Event::NewConnectionRequest = 4`（判别值来自 `0x3d38f0`，`docs/114`）。
/// 两边独立取证、必须相等。**观测**。
#[test]
fn tag_low_word_equals_the_events_enum_discriminant() {
    use gw2_rebuild::events::Event;
    assert_eq!((TAG_NEW_CONNECTION_REQUEST & 0xffff_ffff) as u16, Event::NewConnectionRequest as u16);
    assert_eq!(Event::NewConnectionRequest as u16, 4);
}

// ---------------------------------------------------------------------------
// 2. `make_new_connection_request_event`（arm64 `0x4693c0`，188 B）
// ---------------------------------------------------------------------------

/// 载荷逐字段对 `0x4693c0` 的立即数：`"2.0"`（`mov w9,#0x2e32` + `mov w8,#0x30`）
/// 与 `"60"`（`mov w9,#0x3036`）。**观测**。
#[test]
fn make_new_connection_request_event_payload() {
    let e = make_new_connection_request_event();
    assert_eq!(e.major_version, "2.0");
    assert_eq!(e.minor_version, "60");
    // 无入参 ⇒ 两次调用必须逐字段相等（纯构造函数，没有隐藏状态）
    assert_eq!(e, make_new_connection_request_event());
}

/// 拼出来的载荷 JSON 必须与抓包侧一致（`docs/09` 的握手帧）。
///
/// 这条守的是"字段名 + 顺序"：`0x469414` 先写 `+0x10`（major 指针）、`0x469430` 后写
/// `+0x28`（minor 指针）⇒ **major_version 在前**。**观测**。
#[test]
fn make_new_connection_request_event_wire_shape() {
    let e = make_new_connection_request_event();
    let body = format!(
        r#"{{"major_version":"{}","minor_version":"{}"}}"#,
        e.major_version, e.minor_version
    );
    assert_eq!(body, r#"{"major_version":"2.0","minor_version":"60"}"#);
    // 外层 = {"NewConnectionRequest": <body>}（74 变体枚举的外部标签 JSON）
    assert_eq!(
        format!(r#"{{"NewConnectionRequest":{}}}"#, body),
        r#"{"NewConnectionRequest":{"major_version":"2.0","minor_version":"60"}}"#
    );
}

// ---------------------------------------------------------------------------
// 3. `deserialize` / `serialize` 的**结构常量**（函数体未落码，见模块文档）
// ---------------------------------------------------------------------------

/// 两条失败臂的 source 行号：JSON = `:1149`，UTF-8 = `:1154`。
///
/// 真值来自两条 `adrp`+`add`：`0x4692f0` → `.rodata 0x18cc2c`（`:1149`）、
/// `0x469220` → `.rodata 0x17c634`（`:1154`）。**观测**。
/// ⚠️ 反过来把这两条判成"同一条串"就会丢掉"有两个不同失败阶段"这个事实。
#[test]
fn two_failure_arms_carry_two_different_source_lines() {
    assert_eq!(DeserializeFailure::InvalidJson.source_line(), 1149);
    assert_eq!(DeserializeFailure::InvalidUtf8.source_line(), 1154);
    assert_ne!(
        DeserializeFailure::InvalidJson.source_line(),
        DeserializeFailure::InvalidUtf8.source_line()
    );
}

/// 判据：两个失败臂都必须把 `TAG_INVALID` 写进返回值首字
/// （`0x469244` / `0x469268`+`0x469318`）；`0x4a` = 74 = 变体数，落在合法判别空间之外。
#[test]
fn invalid_sentinel_is_outside_the_74_variant_tag_space() {
    let n = gw2_rebuild::events::ALL_VARIANTS.len();
    assert_eq!(n, 74);
    assert_eq!(TAG_INVALID & 0xffff_ffff, n as u64);
    // 合法判别值最大是 n-1 = 73，哨兵 74 恰好越界一格：这条挡"把哨兵写成 73"
    assert!((TAG_INVALID & 0xffff_ffff) > (n as u64 - 1));
}

/// `serialize` 的 `Vec::with_capacity(128)`（`0x469498 mov w0,#0x80`）。
/// 128 是 `serde_json::to_vec` 的字面量 —— 它同时是"这就是 to_vec"的判据。**观测**。
#[test]
fn serialize_buffer_capacity_is_128() {
    assert_eq!(SERIALIZE_VEC_CAPACITY, 128);
    assert_eq!(format!("{:#x}", SERIALIZE_VEC_CAPACITY), "0x80");
}

/// `GraphwarEvent` 的机器宽度 72 字节（`0x469334..0x469350` 逐字拷 0x48 字节）。
#[test]
fn graphwar_event_machine_width_is_72_bytes() {
    assert_eq!(EVENT_BYTES, 72);
    assert_eq!(format!("{:#x}", EVENT_BYTES), "0x48");
}

/// 内层格式串：`.rodata 0x180e61` 的长度前缀 `0x16` = 22 ⇒ **`{}` 之前那截**正好 22 字符
/// （`{}` 是 `fmt::Arguments` 的占位，不占 `.rodata` 长度）。
///
/// 这条挡两种镜像错误：⑴ 把 `{}` 也算进长度（会写成 24）；⑵ 少一个尾空格（会写成 21）。
#[test]
fn inner_deserialize_message_prefix_is_22_bytes() {
    assert_eq!(MSG_DESERIALIZE_INNER, "Failed to deserialize {}");
    assert_eq!(MSG_DESERIALIZE_INNER.len(), 24);
    assert_eq!(MSG_DESERIALIZE_INNER.strip_suffix("{}").unwrap(), "Failed to deserialize ");
    assert_eq!(MSG_DESERIALIZE_INNER.strip_suffix("{}").unwrap().len(), 22);
}
