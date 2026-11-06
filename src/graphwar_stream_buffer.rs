//! `graphwar_stream_buffer`：**帧层**——把一条 `GraphwarEvent` 变成
//! `u16` 大端长度 + JSON 的字节串，以及反过来从不消费半帧地切出整帧。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`），除非明说 `Ghidra`。
//!
//! ## 1. 它是什么（本模块要回答的问题 (a)）
//!
//! **不是**"TCP 粘包的字节队列"，**也不是**回放流缓冲。它是一对**帧编解码方法**，
//! 缓冲/前瞻全部交给底层 socket 自己：
//!
//! * `write_event`：`serialize` 成 JSON 文本 → 写 2 字节大端长度 → 写负载；
//! * `read_event`：先 `peek` 2 字节拿长度 → 校验 → 再 `peek` `len+2` 字节确认**整帧已到**
//!   → `read_exact(len+2)` 一次性消费 → 把 `buf[2..]` 交给 `serde_json`。
//!
//! 关键在**用 `peek` 而不是 `read`**：整帧没到之前一个字节都不消费，所以不需要用户态
//! 累加缓冲，也就没有"半帧留在 buffer 里"的状态。`GraphwarStreamBuffer` 这个名字里的
//! "buffer" 指的是它对调用方**表现得**像一个事件流，而不是它内部真有一个 `Vec<u8>`——
//! 全模块只有两处 `alloc`，都是给**本次**帧用的临时块，函数返回前就 `dealloc` 了
//! （`0x45c71c`/`0x45c9ec` 等收尾路径）。这一点**推断**（内存语义由指令序列直接给出，
//! 但"所以没有内部缓冲"是解释）。
//!
//! ## 2. 函数清单（arm64 未 strip 构建 = 权威；x86 用字面量 + 被调者 + 尺寸比闸对位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 比 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `read_event`  | `0x45c51c` / 764  | `0x3d86a0` / 832  | 1.089 | peek→校验→读整帧→JSON | **观测** |
//! | `write_event` | `0x45c818` / 1044 | `0x3d89e0` / 1520 | 1.456 | JSON→长度→两段 `write_all` | **观测** |
//!
//! 两处对位的判据（不靠尺寸，尺寸只用来判**不**是别人）：
//!
//! * `read_event`：x86 `0x3d86a0` 里有 `rol $0x8,%ax`（u16 字节序翻转）、
//!   `cmp $0x1000,%ebp`（4096 上限）、`movabs $0x800000000000004b`（错误判别值），
//!   与 arm64 `rev16` / `cmp w24,#0x1000` / `movk #0x8000,lsl #48` 逐条同构；
//!   它引用的串 `"Message too large"` 在整份 x86 `.so` 里**只被这一个函数引用**。
//! * `write_event`：x86 `0x3d89e0` 引用 `"Failed to serialize event."`、
//!   `"Failed to write to tcp_stream."`（两处）、
//!   `"Failed to write to tcp_stream. Message too long."`，且同样有 `cmp $0xffff`
//!   与 `rol $0x8`。**尺寸比 1.456 超出 0.61–1.19 的闸**，这里靠字面量判等钉死；
//!   超闸的原因**未知**（arm64 侧把两条错误构造 out-line 到了不同的冷块。
//!   **推断**）。
//!
//! x86 `0x3d89e0` 就是 `docs/09` §6 里那句"broker 连接用的写入函数就是 `0x3d89e0`
//! （`cmp $0xffff` 限长、`rol $0x8` 转大端）"的那个函数 ⇒ 本模块是**所有** TCP 通道
//! （对局、broker、局域网直连）共用的帧层，不是某个通道的私有实现。
//!
//! ## 3. 逐位事实
//!
//! | # | 事实 | 证据 |
//! |---|---|---|
//! | 1 | 长度前缀 **2 字节、大端、只算负载** | arm64 `0x45c58c rev16`（读）/ `0x45c954 rev`+`lsr #0x10`（写）；x86 `rol $0x8` 两处；抓包 `0x0031`=49 |
//! | 2 | 读侧上限 **`0x1000` = 4096**，超了报 `"Message too large"`（17 B） | arm64 `0x45c590 cmp w24,#1,lsl #12`；x86 `0x3d8713` |
//! | 3 | 写侧上限 **`0xffff`**，超了报 `"Failed to write to tcp_stream. Message too long."`（48 B） | arm64 `0x45c8d0 lsr x8,x23,#0x10` + `cbz`；x86 `0x3d8ab0 cmp $0xffff` |
//! | 4 | peek 到 **0 字节**（对端关闭）⇒ `"Connection Lost"`（15 B） | arm64 `0x45c57c cbz x1` → `0x45c604` |
//! | 5 | peek 到 **不足**（`1` 或 `< len+2`）⇒ **不是错误**，返回"这次没有整帧" | arm64 `0x45c584 b.ne` → `0x45c6fc`（只写判别值、不写负载）；`0x45c6c4` 同 |
//! | 6 | 读侧 `read_exact` 读的是 **`len+2`** 字节（把头部一起消费掉），负载从 `buf+2` 开始 | arm64 `0x45c684 add w0,w24,#2`、`0x45c6d4`、`0x45c734 add x0,x22,#2` |
//! | 7 | 返回类型是 `Result<Option<Event>, io::Error>`：`Ok(None)` 判别值 `0x8000_0000_0000_004a`，`Err` = `0x…4b` | `0x45c6fc` 写 `4a`；`0x45c5f0`/`0x45c66c` 写 `4b` + 负载；`GraphwarEvent` 判别值只有 0..=73（74 个），`4a`/`4b` 是它之后的两个 |
//! | 8 | 自定义 `io::Error` 的 `ErrorKind` 实参是 **`0x28`**（两个错误点都是） | arm64 `0x45c5f8` / `0x45c664` `mov w0,#0x28`；`io::Error::_new` |
//!
//! ### 3.1 顺带结掉 `docs/09` §11.7 的第 4 条未知
//!
//! 那里写的是"`read_event` 的 `0x…4a` 返回值 = EOF 还是'暂时无事件'"。本节给出答案：
//!
//! * `0x…4a` = **"整帧还没到"**（`Ok(None)`）——它只在"peek 到的字节数 > 0 但 < 需要值"
//!   时出现（`0x45c584`、`0x45c6c4` 两个分支）。**不是 EOF**；
//! * EOF 走的是**另一条**路：peek 返回 `Ok(0)` ⇒ `Err("Connection Lost")`（事实 4）。
//!
//! ⚠️ 但 `tick` 的读循环里那个 `cmp w8,#0xd`（`ErrorKind::WouldBlock` = 13）与这里的
//! `0x28` **不是同一个数**：`0xd` 是**底层 `TcpStream::peek` 自己**返回的 `WouldBlock`
//! （非阻塞空读），`read_event` 在 `Err` 分支把它**原样透传**（`0x45c56c`~`0x45c578`
//! 只把 `x1` 从寄存器搬到返回槽，不做转换）。⇒ 上层看到三种"没数据"：
//! `Err(WouldBlock)`（socket 空）、`Ok(None)`（头到了、身子没到）、`Ok(Some(_))`。
//!
//! ## 4. 调用方（谁在用这套帧，**观测**：arm64 `bl` 目标穷举）
//!
//! | 方向 | 调用方 | arm64 | 字节 |
//! |---|---|---|---|
//! | `write_event` | `GraphwarRemoteEventHandler::handle_client_remote_event` | `0x3e3ad0` | 13088 |
//! | `write_event` | `GraphwarClientConnectionHandler::disconnect` | `0x48a8a0` | 948 |
//! | `write_event` | `GraphwarClientConnectionHandler::forward_event` | `0x48ac54` | 464 |
//! | `write_event` | `GraphwarServerConnectionHandler::forward_event` | `0x48badc` | 332 |
//! | `write_event` | `GraphwarServerConnectionHandler::forward_connection_specific_event` | `0x48c02c` | 484 |
//! | `read_event` | `ServerBrokerRequestConnectionClient::process` | `0x3ea0cc` | 688 |
//! | `read_event` | `GraphwarClientConnectionHandler::tick` | `0x48b468` | 1084 |
//! | `read_event` | `GraphwarServerConnectionHandler::tick` | `0x48c644` | 1796 |
//!
//! ⇒ 三条通道（客户端在局、服务端、broker 客户端）**共用**这一个帧层。
//!
//! ## 5. 未知
//!
//! * 读侧 `0x28` 这个 `ErrorKind` 的**枚举名**：按 Rust 1.8x 的 `ErrorKind` 排布
//!   `0x28` 落在 `Other` 附近，但本构建用的是 `library/std` 内部顺序，没有符号可证；
//! * `"Connection Lost"` 在 **x86 构建里搜不到逐字节相同的内容**（整份 `.so` 0 命中，
//!   `"Message too large"` 却 1 命中）⇒ 两版这条消息**不同**，或者 x86 侧走了别的
//!   `ErrorKind`。差异**未知**，不影响帧格式结论。
//!
//! ## 6. 最小复现（线上一帧长什么样）
//!
//! ```text
//! 00 31                                                ← u16 大端 = 49
//! {…49 字节 JSON，形如 {"NewConnectionRequest":{…}}}     ← 负载，无 NUL、无换行
//! ```

use std::io::{self, Write};

/// 长度前缀的字节数（`u16` 大端）。
///
/// 档位：**逐位**（`rev16` / `rol $0x8`）。
pub const LEN_PREFIX_LEN: usize = 2;

/// 读侧单帧负载上限 = `0x1000`。超了整帧**拒收**（不截断、不跳过）。
///
/// 档位：**逐位**（arm64 `0x45c590 cmp w24,#1,lsl #12`；x86 `0x3d8713 cmp $0x1000,%ebp`）。
pub const MAX_MESSAGE_LEN: usize = 0x1000;

/// 写侧单帧负载上限 = `u16::MAX`。超了**不发**，直接报错。
///
/// 档位：**逐位**（arm64 `0x45c8d0 lsr x8,x23,#0x10` + `cbz`；x86 `0x3d8ab0 cmp $0xffff`）。
pub const MAX_FRAME_PAYLOAD: usize = 0xffff;

/// 对端关闭（peek 到 0 字节）时报的消息，15 字节。档位：**逐位**（`.rodata 0x19e385`）。
pub const MSG_CONNECTION_LOST: &str = "Connection Lost";

/// 帧长超过 [`MAX_MESSAGE_LEN`] 时报的消息，17 字节。档位：**逐位**（`.rodata 0x19e374`）。
pub const MSG_TOO_LARGE: &str = "Message too large";

/// `serde_json` 序列化失败时报的消息，26 字节。档位：**逐位**（`.rodata 0x19e3e2`）。
pub const MSG_SERIALIZE_FAILED: &str = "Failed to serialize event.";

/// 写不动时报的消息，30 字节。档位：**逐位**（`.rodata 0x19e394`）。
pub const MSG_WRITE_FAILED: &str = "Failed to write to tcp_stream.";

/// 负载超过 `u16` 时报的消息，48 字节。档位：**逐位**（`.rodata 0x19e3b2`）。
pub const MSG_WRITE_TOO_LONG: &str = "Failed to write to tcp_stream. Message too long.";

/// `Result<Option<GraphwarEvent>, io::Error>` 里 `Ok(None)` 的判别值。
///
/// 74 个 `GraphwarEvent` 变体占 `0x8000_0000_0000_0000..=…0049`，
/// 其后的 `0x4a` = `None`、`0x4b` = `Err`。档位：**观测**
/// （`event_tags_arm64.py` 的 74 条 + `0x45c6fc` / `0x45c5f0` 的写点）。
pub const TAG_OK_NONE: u64 = 0x8000_0000_0000_004a;

/// 见 [`TAG_OK_NONE`]：`Err(io::Error)` 的判别值（错误指针在值槽 `+8`）。
pub const TAG_ERR: u64 = 0x8000_0000_0000_004b;

/// 两个自定义 `io::Error` 传入的 `ErrorKind` 实参。档位：**观测**（`0x45c5f8`/`0x45c664` `mov w0,#0x28`）。
/// 枚举名**未知**（见模块文档 §5）。
pub const CUSTOM_ERROR_KIND: u8 = 0x28;

/// 底层"读"能力：原版只需要 `peek` 与 `read_exact` 两件事。
///
/// `TcpStream` 同时提供两者（`peek(&self)` / `read_exact(&mut self)`），
/// 而 `read_event` 的 `&self` 之所以够用，正是因为**前瞻不算读**。
pub trait PeekRead {
    /// 看一眼不消费；返回实际可看字节数（可能少于请求）。
    fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// 精确读满；不足即错。
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()>;
}

/// 转发一层：`&mut T` 只要 `T` 能 `peek`/`read_exact` 就也能用（调用方手里的游标是可变借用）。
impl<T: PeekRead + ?Sized> PeekRead for &mut T {
    fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (**self).peek(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        (**self).read_exact(buf)
    }
}

impl PeekRead for &[u8] {
    fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.len());
        buf[..n].copy_from_slice(&self[..n]);
        Ok(n)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.len() < buf.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short"));
        }
        let (head, tail) = self.split_at(buf.len());
        buf.copy_from_slice(head);
        *self = tail;
        Ok(())
    }
}

/// 读侧的失败原因（对应原版 `Err(io::Error)` 的四个来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// 底层读/看报错（含非阻塞空读的 `WouldBlock`，**原样透传**）。
    Io,
    /// peek 到 0 字节：对端关了。
    ConnectionLost,
    /// 长度前缀 > [`MAX_MESSAGE_LEN`]（带上是多少）。
    TooLarge(u16),
    /// `read_exact` 没读满。
    Short,
}

impl ReadError {
    /// 原版写进 `io::Error` 的消息；`Io`/`Short` 是纯错误传播，没有自定义消息。
    pub fn message(self) -> Option<&'static str> {
        match self {
            ReadError::ConnectionLost => Some(MSG_CONNECTION_LOST),
            ReadError::TooLarge(_) => Some(MSG_TOO_LARGE),
            ReadError::Io | ReadError::Short => None,
        }
    }
}

/// 写侧的失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    /// `serde_json` 失败（`MSG_SERIALIZE_FAILED`）。
    Serialize,
    /// 负载 > `u16::MAX`（`MSG_WRITE_TOO_LONG`）。
    TooLong,
    /// `write_all` 失败（`MSG_WRITE_FAILED`）。
    Io,
}

impl WriteError {
    /// 原版写进 `io::Error` 的消息（三者都有）。
    pub fn message(self) -> &'static str {
        match self {
            WriteError::Serialize => MSG_SERIALIZE_FAILED,
            WriteError::TooLong => MSG_WRITE_TOO_LONG,
            WriteError::Io => MSG_WRITE_FAILED,
        }
    }
}

/// 一次 `read_event` 的结果。语义与 [`TAG_OK_NONE`]/[`TAG_ERR`] 一一对应：
///
/// * `Ok(Some(bytes))` → [`StreamRead::Event`]
/// * `Ok(None)`        → [`StreamRead::Pending`]（整帧未到）
/// * `Err(e)`          → [`StreamRead::Failed`]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamRead {
    /// 一帧完整负载（**不含** 2 字节前缀）。
    Event(Vec<u8>),
    /// 这一轮没有完整帧：头都没凑齐，或者身子还没到。**不是错误。**
    Pending,
    /// 真错误（见 [`ReadError`]）。
    Failed(ReadError),
}

/// `GraphwarStreamBuffer`：帧层。原版是 `Mutex<GraphwarStreamBuffer>` 里的一个字段对
/// （socket + 状态）；这里把传输层做成泛型参数，行为不变。
#[derive(Debug)]
pub struct GraphwarStreamBuffer<S> {
    /// 底层传输（原版是 `TcpStream`）。`null` 语义（`fd == -1`）由上层用 `Option` 表达。
    pub stream: S,
}

impl<S> GraphwarStreamBuffer<S> {
    /// 包一层。原版没有独立构造点（由 `Mutex` 持有、随连接对象一起构造）⇒ **推断**。
    pub fn new(stream: S) -> Self {
        GraphwarStreamBuffer { stream }
    }
}

impl<S: PeekRead> GraphwarStreamBuffer<S> {
    /// `read_event`（arm64 `0x45c51c`）：`peek` 头 → 校验 → `peek` 整帧 → `read_exact`。
    ///
    /// 返回的 `Vec<u8>` 是 JSON 负载原文，原版紧接着交给
    /// `GraphwarEvent::deserialize`（arm64 `0x4691a4`，调用点 `0x45c73c`）。
    ///
    /// 与 `docs/09` §11 的 `write_event`/`read_event` 命名一致；**不做** JSON 解析，
    /// 因为解析属于 `graphwar_event` 模块。
    pub fn read_event(&mut self) -> StreamRead {
        // ① 头两个字
        let mut head = [0u8; LEN_PREFIX_LEN];
        let got = match self.stream.peek(&mut head) {
            Ok(n) => n,
            Err(_) => return StreamRead::Failed(ReadError::Io),
        };
        if got == 0 {
            return StreamRead::Failed(ReadError::ConnectionLost);
        }
        if got < LEN_PREFIX_LEN {
            // 事实 5：不足 ⇒ "还没到"，不是错
            return StreamRead::Pending;
        }

        // ② 大端长度 + 上限
        let len = u16::from_be_bytes(head) as usize;
        if len > MAX_MESSAGE_LEN {
            return StreamRead::Failed(ReadError::TooLarge(len as u16));
        }

        // ③ 整帧前瞻：len+2 一个字节都不少才算齐
        let want = len + LEN_PREFIX_LEN;
        let mut frame = vec![0u8; want];
        match self.stream.peek(&mut frame) {
            Ok(n) if n == want => {}
            Ok(_) => return StreamRead::Pending,
            Err(_) => return StreamRead::Failed(ReadError::Io),
        }

        // ④ 真消费（连头一起，事实 6）
        if self.stream.read_exact(&mut frame).is_err() {
            return StreamRead::Failed(ReadError::Short);
        }
        StreamRead::Event(frame[LEN_PREFIX_LEN..].to_vec())
    }
}

impl<S: Write> GraphwarStreamBuffer<S> {
    /// `write_event`（arm64 `0x45c818`）：已经序列化好的 JSON 文本 → 两段 `write_all`。
    ///
    /// 原版的入参是 `&GraphwarEvent` 并自己调 `serialize`；这里把"序列化"留在外面，
    /// 因为它是 `graphwar_event` 模块的事（`docs/07`）。错误种类一一对应。
    pub fn write_event(&mut self, payload: &[u8]) -> Result<(), WriteError> {
        if payload.len() > MAX_FRAME_PAYLOAD {
            return Err(WriteError::TooLong);
        }
        let mut prefix = [0u8; LEN_PREFIX_LEN];
        prefix.copy_from_slice(&(payload.len() as u16).to_be_bytes());
        // 两段写，**不**拼成一个大 Vec（arm64 是两次 write_all：0x45c970 / 0x45c9fc）
        self.stream
            .write_all(&prefix)
            .map_err(|_| WriteError::Io)?;
        self.stream
            .write_all(payload)
            .map_err(|_| WriteError::Io)?;
        Ok(())
    }
}

/// 只算前缀：`u16` 大端。原版写侧的 `rev`+`lsr #0x10` 等价物。
pub fn encode_len(len: usize) -> Option<[u8; LEN_PREFIX_LEN]> {
    if len > MAX_FRAME_PAYLOAD {
        return None;
    }
    Some((len as u16).to_be_bytes())
}

/// 只读前缀：大端 → 长度。原版读侧的 `ldrh` + `rev16` 等价物。
pub fn decode_len(prefix: [u8; LEN_PREFIX_LEN]) -> u16 {
    u16::from_be_bytes(prefix)
}

/// 把"到底算不算错误"这件事单列出来，供上层读循环照抄原版的三分派：
/// `Err(WouldBlock)`（socket 空）→ 收工；`Ok(None)` → 收工；`Ok(Some)` → 继续。
pub fn is_idle(outcome: &StreamRead) -> bool {
    matches!(outcome, StreamRead::Pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抓包实测的那一帧：`0x0031` = 49 字节负载（`docs/09` §1.1）。
    #[test]
    fn length_prefix_is_big_endian_and_counts_payload_only() {
        let payload = vec![b'x'; 49];
        let mut buf = Vec::new();
        GraphwarStreamBuffer::new(&mut buf)
            .write_event(&payload)
            .unwrap();
        assert_eq!(&buf[..2], &[0x00, 0x31], "大端 49");
        assert_eq!(buf.len(), 51, "前缀只算负载");
        assert_eq!(decode_len([0x00, 0x31]), 49);
        assert_eq!(encode_len(49), Some([0x00, 0x31]));
        // 0x0102 与 0x0201 必须分得开（小端会写成 [0x02,0x01]）
        assert_eq!(encode_len(0x0102), Some([0x01, 0x02]));
        assert_eq!(encode_len(0x0102).map(decode_len), Some(0x0102));
    }

    #[test]
    fn round_trip_through_the_same_type() {
        let frames: Vec<Vec<u8>> = vec![b"{}".to_vec(), b"{\"TickRequest\":{}}".to_vec()];
        let mut wire = Vec::new();
        for f in &frames {
            GraphwarStreamBuffer::new(&mut wire).write_event(f).unwrap();
        }
        let mut cursor = &wire[..];
        for f in &frames {
            let out = GraphwarStreamBuffer::new(&mut cursor).read_event();
            assert_eq!(out, StreamRead::Event(f.clone()));
        }
        // 消费干净 ⇒ 再来一次就是 "0 字节" = 对端关闭
        assert_eq!(
            GraphwarStreamBuffer::new(&mut cursor).read_event(),
            StreamRead::Failed(ReadError::ConnectionLost)
        );
    }

    #[test]
    fn empty_stream_is_connection_lost_not_pending() {
        let mut empty = &[][..];
        assert_eq!(
            GraphwarStreamBuffer::new(&mut empty).read_event(),
            StreamRead::Failed(ReadError::ConnectionLost)
        );
        assert_eq!(ReadError::ConnectionLost.message(), Some("Connection Lost"));
        assert_eq!(MSG_CONNECTION_LOST.len(), 15);
    }

    #[test]
    fn one_byte_of_header_is_pending_not_error() {
        // 事实 5：头只到 1 字节 ⇒ Ok(None)，且**不消费**
        let mut half = &[0x00u8][..];
        assert!(is_idle(
            &GraphwarStreamBuffer::new(&mut half).read_event()
        ));
        assert_eq!(half.len(), 1, "peek 不消费");
    }

    #[test]
    fn body_short_is_pending_and_never_consumes() {
        // 头说 4 字节，只来了 2 字节
        let wire = [0x00u8, 0x04, b'{', b'}'];
        let mut cursor = &wire[..];
        assert!(is_idle(
            &GraphwarStreamBuffer::new(&mut cursor).read_event()
        ));
        assert_eq!(cursor.len(), 4, "整帧没齐之前一个字节都不消费");
    }

    #[test]
    fn oversized_frame_is_rejected_with_the_real_message() {
        // 0x1001 > 4096
        let mut wire = vec![0x10u8, 0x01];
        wire.extend(std::iter::repeat(b'x').take(0x1001));
        let mut cursor = &wire[..];
        assert_eq!(
            GraphwarStreamBuffer::new(&mut cursor).read_event(),
            StreamRead::Failed(ReadError::TooLarge(0x1001))
        );
        assert_eq!(MSG_TOO_LARGE, "Message too large");
        assert_eq!(MSG_TOO_LARGE.len(), 17);
        // 边界：正好 4096 是合法的
        let mut ok = vec![0x10u8, 0x00];
        ok.extend(std::iter::repeat(b'x').take(0x1000));
        let mut c = &ok[..];
        assert!(matches!(
            GraphwarStreamBuffer::new(&mut c).read_event(),
            StreamRead::Event(ref v) if v.len() == 0x1000
        ));
    }

    #[test]
    fn write_side_rejects_more_than_u16() {
        let payload = vec![0u8; MAX_FRAME_PAYLOAD + 1];
        let mut out = Vec::new();
        assert_eq!(
            GraphwarStreamBuffer::new(&mut out).write_event(&payload),
            Err(WriteError::TooLong)
        );
        assert!(out.is_empty(), "超长一个字节都不写");
        assert_eq!(encode_len(MAX_FRAME_PAYLOAD + 1), None);
        assert_eq!(MSG_WRITE_TOO_LONG.len(), 48);
        assert_eq!(WriteError::TooLong.message(), MSG_WRITE_TOO_LONG);
    }

    #[test]
    fn discriminant_constants_match_the_enum_domain() {
        // GraphwarEvent 的 74 个变体占 0x…00..=0x…49；
        assert_eq!(TAG_OK_NONE, 0x8000_0000_0000_004a);
        assert_eq!(TAG_ERR, TAG_OK_NONE + 1);
        assert_eq!(CUSTOM_ERROR_KIND, 40);
        assert_eq!(MSG_SERIALIZE_FAILED.len(), 26);
        assert_eq!(MSG_WRITE_FAILED.len(), 30);
    }
}
