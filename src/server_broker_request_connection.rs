//! `server_broker_request_connection`：**连 broker 的那条客户端连接**（房主侧）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 它是什么，为什么名字里四个词分别是"服务器 / broker / 请求 / 连接"
//!
//! 模块路径是 `graphwarrust::server_broker_request_connection`（**非** `graphwar_` 前缀，
//! 属于那 12 个"非 `graphwar_` 游戏模块"之一），类型 `ServerBrokerRequestConnectionClient`。
//! 它是**房主进程连到 broker（房间列表服务器）的那一条 TCP 连接**：
//!
//! * 连接由 `GraphwarServerBrokerHandler::start` 起的**线程**建立并持有（§3）；
//! * 之后每帧由 `GraphwarServerBrokerHandler::process` 驱动它 `process()` 收事件；
//! * 它被存在 `Mutex<Option<…>>` 里 —— 这条是**观测**，因为符号表里就有
//!   `drop_in_place<PoisonError<MutexGuard<Option<ServerBrokerRequestConnectionClient>>>>`
//!   （`0x492c04` / `0x4931xx` 一族）。
//!
//! ## 2. 函数清单（**只有 1 个**——这就是"可归属 0"的全部内容）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 比 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `process` | `0x3ea0cc` / 688 | `0x3e5a80` / 816 | 1.186 | 从帧层榨事件 → `Vec<GraphwarEvent>` | **观测** |
//!
//! **"可归属 0" 的成因**：本模块**一个 `src/*.rs` 的 `panic::Location` 都不引用**
//! （`fixtures/name_to_addr.tsv` 里两行都是"引用了 1 个 Location 键（src 0 个）"），
//! 所以 `name_bridge.py` 的锚点法在这里**无锚可用**。x86 对位因此改走**调用指纹**：
//!
//! 1. arm64 `process` 的 `bl` 目标里只有**一个**是本仓库已对位过的
//!    ——`GraphwarStreamBuffer::read_event`（arm64 `0x45c51c`，x86 已知 `0x3d86a0`）；
//! 2. 全 `.text` 里调用 x86 `0x3d86a0` 的函数只有 **3 个**：`0x3e5a80`(816)、
//!    `0x459f20`(1056)、`0x45b020`(2144)；
//! 3. 后两个各有自己的 `src` 锚（`graphwar_client_connection_handler.rs:190…` /
//!    `graphwar_server_connection_handler.rs:206…`）⇒ 排除；
//! 4. 只剩 `0x3e5a80`，且它**一个 `src` 锚都没有**（与 arm64 本模块同相），
//!    尺寸比 816/688 = 1.186 落在闸内。
//!
//! ⇒ **推断**（四级证据齐、但没有"唯一锚"这种硬判等）。这是本模块唯一的对位不确定点。
//!
//! ## 3. `process` 干什么（arm64 逐条被调者）
//!
//! ```text
//! loop {
//!     match read_event() {                   // 0x3ea160 → GraphwarStreamBuffer::read_event
//!         Ok(Some(ev)) => events.push(ev),   // 0x3ea1b0 RawVec::grow_one
//!         Ok(None)     => break,             // 整帧还没到（stream_buffer 的 0x…4a）
//!         Err(e) => {
//!             if e.kind() == WouldBlock { break }   // 0x3ea1c0 Error::kind
//!             else { 打日志（chrono 时间戳 + format! + println!）; break }  // 0x3ea210/0x3ea238/0x3ea274
//!         }
//!     }
//! }
//! ```
//!
//! 顺带把 `GraphwarEvent` 的 `drop_in_place` 也调了（`0x3ea2ac`/`0x3ea320`）——
//! 即**事件被消费掉**，不是转手给别人。⇒ 本对象只负责"把字节变成事件"，
//! 事件之后的语义（改房间表）在 `graphwar_server_broker_handler::process` 里。
//!
//! ## 4. **连接**那半边代码在哪（重要：不在本模块的符号里）
//!
//! 建 socket、`set_nonblocking`、`.unwrap()`、以及**第一帧 `write_event`** 全部**内联**在
//! `GraphwarServerBrokerHandler::start` 起的那个线程体里（arm64 `0x48e130` / x86 `0x437ce0`）：
//!
//! * 该函数里同时出现 `src/graphwar_server_broker_handler.rs:60:55` 与
//!   `src/server_broker_request_connection.rs:66:42` 两个 `Location`
//!   ⇒ 两块代码被**内联进同一个函数**；
//! * 它还引用字面量 `"Cannot set non-blocking"`（x86 `0x1bfa0a`）并调用
//!   `GraphwarStreamBuffer::write_event`（x86 `0x3d89e0`）；
//! * x86 `0x437ce0` = 1344 字节，arm64 `0x48e130` = 1984 字节，比 **0.677**（在闸内）。
//!
//! ⇒ **`ServerBrokerRequestConnectionClient` 的"连上 + 设非阻塞 + 发第一帧"没有独立符号**，
//! 所以本模块只有 `process` 一个方法。这也解释了 `fixtures/module_funcs.tsv` 为什么把
//! `0x437ce0` 记到 `graphwar_server_broker_handler.rs`——它确实同时属于两个文件。
//! ⚠️ 这正是"别用'谁持有它'反推'它在哪个模块'"那条教训的实例。
//!
//! ## 5. 第一帧是什么（**推断**，但三方一致）
//!
//! 真实会话（`fixtures/protocol_match.jsonl` 第 83–86 行，客户端侧抓包）第一条永远是
//!
//! ```text
//! -> {"VersionInfo":{"version":"2.0.60","system":"linux"}}
//! ```
//!
//! 与 `src/_scaffold/broker.rs` 的实现、以及 `make_new_connection_request_event`
//! 里逐字节写 `"2.0"` / `"60"` 的指令都对得上 ⇒ 本模块的 [`version_info_frame`]
//! 生成这一帧。**等级：推断**（版本串是逐位观测的，但"broker 连接的第一帧就是它"
//! 只有抓包的一侧证据）。
//!
//! ## 6. 未知
//!
//! * `process` 的**返回值**：arm64 里 `Vec<GraphwarEvent>` 是出参还是内部缓冲？
//!   末尾有 `drop_in_place<Vec<GraphwarEvent>>`（`0x3eb058`）⇒ 至少有一处是局部的，
//!   但事件被 push 进哪个 `Vec` 没有类型级证据 ⇒ **未知**；
//! * 谁**读**那个 `Vec`：`graphwar_server_broker_handler::process` 里有
//!   `HashMap::insert` / `remove_entry` 与 `hash_one`，**推断**是它消费；
//! * 断线后的重连：**没有**（与客户端侧一致，`docs/09` §11.4）。

use crate::graphwar_stream_buffer::{GraphwarStreamBuffer, PeekRead, ReadError, StreamRead};

/// 模块路径（`.symtab` 的 mangled 前缀），用于交叉查证。
///
/// 档位：**观测**（arm64 `_ZN12graphwarrust32server_broker_request_connection…`）。
pub const MODULE_PATH: &str = "graphwarrust::server_broker_request_connection";

/// 类型名。档位：**观测**（同上）。
pub const TYPE_NAME: &str = "ServerBrokerRequestConnectionClient";

/// 本模块唯一的函数名与其 arm64 地址 / 字节数。档位：**观测**（`.symtab`）。
pub const PROCESS_ARM64: u64 = 0x3ea0cc;
/// `process` 的 arm64 字节数。
pub const PROCESS_ARM64_SIZE: usize = 688;
/// `process` 的 x86 地址。档位：**推断**（§2 的四步指纹）。
pub const PROCESS_X86: u64 = 0x3e5a80;
/// `process` 的 x86 字节数（`.eh_frame` 区间）。
pub const PROCESS_X86_SIZE: usize = 816;

/// `set_nonblocking` 失败时的消息，21 字节。档位：**逐位**（x86 `.rodata 0x1bfa0a`）。
pub const MSG_NONBLOCKING: &str = "Cannot set non-blocking";

/// 连接用的 `Location`：本模块里唯一被引用的一条（`src 0` 个锚即由此而来）。
///
/// 档位：**观测**（arm64 `.data.rel.ro` 槽 `0x84e588`，被 `0x48e130` 引用）。
pub const LOC_CONNECT: (&str, u32, u32) = ("src/server_broker_request_connection.rs", 66, 42);

/// 一次 `process` 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drain {
    /// 榨出 `n` 条事件（已消费）。
    Events(usize),
    /// 这一轮没有数据（`Ok(None)` 或 `Err(WouldBlock)`），**不是错误**。
    Idle,
    /// 真错误（原版会先打一行日志再停）。
    Failed(ReadError),
}

impl Drain {
    /// 让调用方一眼看出"该不该继续下一帧"。
    pub fn is_idle(self) -> bool {
        matches!(self, Drain::Idle)
    }
}

/// `ServerBrokerRequestConnectionClient`：一条 broker 连接的**读侧**。
///
/// 原版持有 `GraphwarStreamBuffer`（帧层）+ 一个事件 `Vec`。这里把传输做成泛型，
/// 并且只保留**可判定的行为**：把已到达的整帧全部榨出来。
#[derive(Debug)]
pub struct ServerBrokerRequestConnectionClient<S> {
    /// 帧层（原版是同一个类型，见 `graphwar_stream_buffer`）。
    pub buffer: GraphwarStreamBuffer<S>,
    /// 已榨出的负载（原版是 `Vec<GraphwarEvent>`；这里保留 JSON 原文，
    /// 解析交给 `graphwar_event` 模块）。
    pub events: Vec<Vec<u8>>,
    /// 是否已发过第一帧（`start` 的线程体里写的那一帧，§5）。
    pub requested: bool,
}

impl<S> ServerBrokerRequestConnectionClient<S> {
    /// 包一条流。原版没有独立构造点（线程体里就地构造）⇒ **推断**。
    pub fn new(stream: S) -> Self {
        ServerBrokerRequestConnectionClient {
            buffer: GraphwarStreamBuffer::new(stream),
            events: Vec::new(),
            requested: false,
        }
    }
}

impl<S: PeekRead> ServerBrokerRequestConnectionClient<S> {
    /// `process`（arm64 `0x3ea0cc`）：把**当前能拿到的**整帧全部榨出来（§3 的循环）。
    ///
    /// 与模块文档 §3 一一对应：`Ok(Some)` 继续、`Ok(None)` 停、`Err(WouldBlock)` 停、
    /// 其它 `Err` 记一笔后停（原版这里会打日志，本实现把原因交回去）。
    pub fn process(&mut self) -> Drain {
        let mut drained = 0usize;
        loop {
            match self.buffer.read_event() {
                StreamRead::Event(payload) => {
                    self.events.push(payload);
                    drained += 1;
                }
                StreamRead::Pending => {
                    return if drained == 0 {
                        Drain::Idle
                    } else {
                        Drain::Events(drained)
                    }
                }
                StreamRead::Failed(e) => {
                    // 原版对 `Err(WouldBlock)` 与其它 IO 错误都只是"这一轮结束"，
                    // 区别只在**要不要打日志**（`if kind != WouldBlock`）。
                    return if drained == 0 {
                        Drain::Failed(e)
                    } else {
                        Drain::Events(drained)
                    };
                }
            }
        }
    }

    /// 取走已榨出的事件（原版把 `Vec<GraphwarEvent>` 交出去，自己留空）。
    pub fn take_events(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.events)
    }
}

/// broker 连接的第一帧：`{"VersionInfo":{"version":"2.0.60","system":"linux"}}`。
///
/// 档位：**推断**（抓包一侧 + `make_new_connection_request_event` 的逐字节版本串）。
/// 系统串取 `linux` 是因为抓包样本就是它（`fixtures/protocol_match.jsonl`）。
pub fn version_info_frame(version: &str, system: &str) -> String {
    format!(
        "{{\"VersionInfo\":{{\"version\":{},\"system\":{}}}}}",
        crate::_scaffold::protocol::json_str(version),
        crate::_scaffold::protocol::json_str(system)
    )
}

/// 默认版本串。档位：**逐位**（`0x4693c0` 里 `mov w9,#0x2e32`=`"2."`、`mov w9,#0x3036`=`"60"`）。
pub const VERSION: &str = "2.0.60";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_stream_buffer::GraphwarStreamBuffer;

    fn framed(payloads: &[&str]) -> Vec<u8> {
        let mut w = Vec::new();
        for p in payloads {
            GraphwarStreamBuffer::new(&mut w)
                .write_event(p.as_bytes())
                .unwrap();
        }
        w
    }

    #[test]
    fn one_reading_pass_drains_every_complete_frame() {
        let wire = framed(&["{\"GameInfo\":{}}", "{\"GameInfo\":{}}", "{\"GameRemoved\":{}}"]);
        let mut c = ServerBrokerRequestConnectionClient::new(&wire[..]);
        assert_eq!(c.process(), Drain::Events(3));
        assert_eq!(c.events.len(), 3);
        assert_eq!(c.take_events().len(), 3);
        assert!(c.events.is_empty());
        // 再跑一次：字节已消费完 ⇒ 对端关闭
        assert_eq!(c.process(), Drain::Failed(ReadError::ConnectionLost));
    }

    #[test]
    fn partial_frame_is_idle_not_error() {
        let wire = framed(&["{\"GameInfo\":{}}"]);
        // 砍掉最后 3 字节 ⇒ 身子没到
        let short = &wire[..wire.len() - 3];
        let mut c = ServerBrokerRequestConnectionClient::new(short);
        assert_eq!(c.process(), Drain::Idle);
        assert!(c.events.is_empty());
        assert!(Drain::Idle.is_idle());
    }

    #[test]
    fn version_info_frame_is_the_observed_one() {
        assert_eq!(
            version_info_frame("2.0.60", "linux"),
            "{\"VersionInfo\":{\"version\":\"2.0.60\",\"system\":\"linux\"}}"
        );
        assert_eq!(VERSION, "2.0.60");
        // 帧层加上去就是线上字节：两字节大端长度
        let f = version_info_frame(VERSION, "linux");
        let mut w = Vec::new();
        GraphwarStreamBuffer::new(&mut w)
            .write_event(f.as_bytes())
            .unwrap();
        assert_eq!(
            u16::from_be_bytes([w[0], w[1]]) as usize,
            f.len(),
            "长度只算负载"
        );
    }

    #[test]
    fn identity_constants() {
        assert_eq!(MODULE_PATH, "graphwarrust::server_broker_request_connection");
        assert_eq!(TYPE_NAME, "ServerBrokerRequestConnectionClient");
        assert_eq!(LOC_CONNECT.0, "src/server_broker_request_connection.rs");
        assert_eq!(LOC_CONNECT.1, 66);
        assert_eq!(MSG_NONBLOCKING, "Cannot set non-blocking");
        assert_eq!(MSG_NONBLOCKING.len(), 23);
        // arm64 / x86 的尺寸比在闸内（0.61–1.19）
        let ratio = PROCESS_X86_SIZE as f64 / PROCESS_ARM64_SIZE as f64;
        assert!((0.61..=1.19).contains(&ratio), "ratio={ratio}");
    }
}
