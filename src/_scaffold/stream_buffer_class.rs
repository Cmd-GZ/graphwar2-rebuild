//! **不是原版的东西**：把 [`crate::graphwar_stream_buffer`]（原版 `GraphwarStreamBuffer`：
//! `read_event` arm64 `0x45c51c` / x86 `0x3d86a0`、`write_event` arm64 `0x45c818` /
//! x86 `0x3d89e0`）的**帧层规则**接成 GDExtension 类 `GwStreamBuffer`，
//! 让 `client/src/net/frame.gd` 只剩"循环 + 把 JSON 解成 Dictionary"。
//!
//! # 为什么会有这一份（T58 / `docs/06` §90 的 C 档第 8 段）
//!
//! `client/src/net/frame.gd`（改前 175 行）把帧层又用 GDScript 写了一遍：`u16` 大端前缀、
//! 超长判据、切帧循环、半帧保留。真值在 Rust 侧**逐位**复原（见
//! `src/graphwar_stream_buffer.rs` 模块文档 §3 的八条事实），而且 `.gd` 那份少了一条：
//!
//! | # | 事实 | 真值（Rust） | `.gd` 改前 |
//! |---|---|---|---|
//! | 1 | 前缀 = **2 字节大端、只算负载** | `LEN_PREFIX_LEN` / `encode_len` | 一致 |
//! | 2 | **写侧**上限 `0xffff`（`Message too long.`） | `MAX_FRAME_PAYLOAD` | 一致 |
//! | 3 | **读侧**上限 `0x1000` = 4096（`Message too large`） | `MAX_MESSAGE_LEN`（arm64 `0x45c590 cmp w24,#1,lsl #12`） | **缺**（无损读到任意长） |
//! | 4 | peek 0 字节 = 对端关了（`Connection Lost`） | `ReadError::ConnectionLost` | 无对应判据 |
//! | 5 | peek 不足 = **不是错误**（`Ok(None)`） | `StreamRead::Pending` | 一致（`break`） |
//! | 6 | 整帧到齐才消费，消费 `len+2` 字节 | `read_event` ④ | 一致 |
//!
//! ⚠️ GDScript 侧手里是**一整块缓冲**（Godot `StreamPeerTCP` 没有 `peek`），所以这里把
//! `read_event` 的"peek ⇒ 判断 ⇒ 消费"改写成**带游标的纯函数** [`GwStreamBuffer::decode_one`]：
//! 判据（2 字节大端 / 4096 / `len+2`）全部来自 Rust，**游标推进由调用方按 `consumed` 做**。
//! "用 peek 不消费"这一点在 `.gd` 侧靠"没切走的部分留在 `rest`"表达，语义等价。
//!
//! # 接口
//!
//! ```gdscript
//! GwStreamBuffer.length_prefix_len()   # 2
//! GwStreamBuffer.max_payload()         # 65535  （写侧，u16）
//! GwStreamBuffer.max_read_len()        # 4096   （读侧，0x1000）
//! GwStreamBuffer.encode_frame(payload) # PackedByteArray（超 0xffff ⇒ 空）
//! GwStreamBuffer.encode_frames(arr)    # 多条拼一段流
//! GwStreamBuffer.decode_one(buf, offset)
//!     # -> {status: "event"|"pending"|"failed", frame: String, consumed: int,
//!     #     reason: String, message: String}
//! GwStreamBuffer.error_messages()      # 原版那 5 条错误原文（逐位）
//! GwStreamBuffer.provenance()          # 出处/档位
//! ```
//!
//! `status` 三档与 [`crate::graphwar_stream_buffer::StreamRead`] 一一对应：
//! `event` ↔ `Ok(Some)`、`pending` ↔ `Ok(None)`（**不是错误**）、`failed` ↔ `Err`。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_stream_buffer as sb;

/// 帧层规则出口（scaffold；原版是 `GraphwarStreamBuffer<S>` 上的两个方法）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwStreamBuffer {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwStreamBuffer {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwStreamBuffer {
    /// 长度前缀的字节数 = **2**（`LEN_PREFIX_LEN`；arm64 `0x45c58c rev16`）。
    #[func]
    fn length_prefix_len() -> i64 {
        sb::LEN_PREFIX_LEN as i64
    }

    /// **写侧**单帧负载上限 = `0xffff`（`MAX_FRAME_PAYLOAD`；x86 `0x3d8ab0 cmp $0xffff`）。
    #[func]
    fn max_payload() -> i64 {
        sb::MAX_FRAME_PAYLOAD as i64
    }

    /// **读侧**单帧负载上限 = `0x1000` = 4096（`MAX_MESSAGE_LEN`；arm64 `0x45c590 cmp w24,#1,lsl #12`）。
    ///
    /// 这一条 `.gd` 改前**没有** —— 见文件头表第 3 行。
    #[func]
    fn max_read_len() -> i64 {
        sb::MAX_MESSAGE_LEN as i64
    }

    /// 一帧 = `[hi, lo] + UTF-8 负载`。负载超过 `0xffff` 返回**空**数组
    /// （调用方必须把空当错误 —— 原版在这里报 `Message too long.`）。
    #[func]
    fn encode_frame(payload: GString) -> PackedByteArray {
        let body = payload.to_string();
        let bytes = body.as_bytes();
        let Some(prefix) = sb::encode_len(bytes.len()) else {
            return PackedByteArray::new(); // 超长 ⇒ 空（与原版错误路径同语义）
        };
        let mut out = Vec::with_capacity(bytes.len() + sb::LEN_PREFIX_LEN);
        out.extend_from_slice(&prefix);
        out.extend_from_slice(bytes);
        PackedByteArray::from(out)
    }

    /// 多条消息拼成一段字节流（每条各自成帧）。
    #[func]
    fn encode_frames(payloads: PackedStringArray) -> PackedByteArray {
        let mut out: Vec<u8> = Vec::new();
        for p in payloads.as_slice() {
            let body = p.to_string();
            let bytes = body.as_bytes();
            let Some(prefix) = sb::encode_len(bytes.len()) else {
                return PackedByteArray::new(); // 任一条超长 ⇒ 整段失败（不半发）
            };
            out.extend_from_slice(&prefix);
            out.extend_from_slice(bytes);
        }
        PackedByteArray::from(out)
    }

    /// 从 `buf[offset..]` 尝试切**一帧**（不修改 `buf`，由调用方按 `consumed` 推进）。
    ///
    /// 返回字典的键：
    /// * `status` —— `"event"` / `"pending"` / `"failed"`（对应 `StreamRead` 三档）；
    /// * `frame` —— 负载的 UTF-8 文本（只有 `event` 有意义）；
    /// * `consumed` —— 该推进多少字节（只有 `event` 非 0：`len + 2`）；
    /// * `reason` —— `""` / `"connection_lost"` / `"too_large"` / `"bad_utf8"`；
    /// * `message` —— 原版写进 `io::Error` 的原文（`Connection Lost` / `Message too large`；
    ///   `connection_lost`/`too_large` 之外为空）。
    #[func]
    fn decode_one(buf: PackedByteArray, offset: i64) -> VarDictionary {
        let bytes = buf.as_slice();
        let off = if offset < 0 { 0usize } else { offset as usize };

        let mut d = VarDictionary::new();
        d.set("status", &GString::from("pending"));
        d.set("frame", &GString::from(""));
        d.set("consumed", 0i64);
        d.set("reason", &GString::from(""));
        d.set("message", &GString::from(""));

        // peek 到 0 字节 ⇒ 对端关了（事实 4）。调用方只在 `off < buf.size()` 时才会进来，
        // 所以这条分支只表达"确确实实没数据了"。
        if off >= bytes.len() {
            d.set("status", &GString::from("failed"));
            d.set("reason", &GString::from("connection_lost"));
            d.set("message", &GString::from(sb::MSG_CONNECTION_LOST));
            return d;
        }
        // 头都没凑齐 ⇒ `Ok(None)`，**不是错误**（事实 5）。
        if bytes.len() - off < sb::LEN_PREFIX_LEN {
            return d;
        }
        let mut head = [0u8; sb::LEN_PREFIX_LEN];
        head.copy_from_slice(&bytes[off..off + sb::LEN_PREFIX_LEN]);
        let len = sb::decode_len(head) as usize;
        // 读侧 4096 上限（事实 2/3：超了整帧拒收，不截断、不跳过）。
        if len > sb::MAX_MESSAGE_LEN {
            d.set("status", &GString::from("failed"));
            d.set("reason", &GString::from("too_large"));
            d.set("message", &GString::from(sb::MSG_TOO_LARGE));
            return d;
        }
        let want = len + sb::LEN_PREFIX_LEN;
        // 整帧没到齐 ⇒ `Ok(None)`（事实 5/6：一个字节都不消费）。
        if bytes.len() - off < want {
            return d;
        }
        let payload = &bytes[off + sb::LEN_PREFIX_LEN..off + want];
        let Ok(text) = std::str::from_utf8(payload) else {
            // 原版这里是 `serde_json` 反序列化失败 ⇒ `Frame::decode` 返回 `None`，
            // 调用方当"对端关了"。没有固定错误原文，所以 `message` 留空，由连接层措辞。
            d.set("status", &GString::from("failed"));
            d.set("reason", &GString::from("bad_utf8"));
            return d;
        };
        d.set("status", &GString::from("event"));
        d.set("frame", &GString::from(text));
        d.set("consumed", want as i64);
        d
    }

    /// 原版那 5 条错误原文（逐位：它们都是 `.rodata` 里的字面量，见
    /// `graphwar_stream_buffer` 模块文档 §2/§3 与被引用点）。
    #[func]
    fn error_messages() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("connection_lost", &GString::from(sb::MSG_CONNECTION_LOST));
        d.set("too_large", &GString::from(sb::MSG_TOO_LARGE));
        d.set("serialize_failed", &GString::from(sb::MSG_SERIALIZE_FAILED));
        d.set("write_failed", &GString::from(sb::MSG_WRITE_FAILED));
        d.set("write_too_long", &GString::from(sb::MSG_WRITE_TOO_LONG));
        d
    }

    /// 出处与档位（自检、文档复核用）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("read_event", &GString::from("graphwar_stream_buffer::read_event"));
        d.set("read_event_addr", &GString::from("0x45c51c (arm64) / 0x3d86a0 (x86)"));
        d.set("write_event", &GString::from("graphwar_stream_buffer::write_event"));
        d.set("write_event_addr", &GString::from("0x45c818 (arm64) / 0x3d89e0 (x86)"));
        d.set("len_prefix_len", sb::LEN_PREFIX_LEN as i64);
        d.set("max_read_len", sb::MAX_MESSAGE_LEN as i64);
        d.set("max_payload", sb::MAX_FRAME_PAYLOAD as i64);
        d.set(
            "evidence",
            &GString::from(
                "逐位：rev16/rol $0x8 两处、cmp $0x1000 (读侧)、cmp $0xffff (写侧)、\
                 peek 0 ⇒ Connection Lost、不足 ⇒ Ok(None)（0x800000000000004a）",
            ),
        );
        d.set("grade", &GString::from("逐位"));
        d
    }
}
