//! `graphwar_server_broker_handler`：**Global 一路的房间表**——房主侧连 broker、
//! 把 `GameInfo` 累积成表，供大厅读。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（arm64 `.symtab` 权威：`graphwarrust::graphwar_server_broker_handler`）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 比 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `get_game_info` | `0x489b98` / 460  | **无锚** | — | 按地址查表 | **观测**（arm64）+ **推断**（语义） |
//! | `new`           | `0x489d64` / 236  | **无锚** | — | 建表（`RandomState` + hashbrown） | **观测** |
//! | `stop`          | `0x489e50` / 344  | `0x4586d0` / 288  | 0.837 | 锁 + 取走连接 ⇒ 关流 | **观测**（唯一锚 `:90:52`） |
//! | `start`         | `0x489fa8` / 208  | `0x4587f0` / 224  | 1.077 | 起线程（**分离**，不 join） | **推断**（见 §1.1） |
//! | `process`       | `0x48a078` / 2088 | `0x4588d0` / 2560 | 1.226 | 驱动连接 + 维护房间表 | **观测**（4 个锚命中 3 个） |
//! | `start::{{closure}}`（线程体） | `0x48e130` / 1984 | `0x437ce0` / 1344 | 0.677 | 连 broker + 设非阻塞 + 写第一帧 | **观测**（两个文件的 `Location` 同时命中） |
//!
//! ### 1.1 `start` 的 x86 地址为什么是"推断"
//!
//! 它在 arm64 只有一个锚（`src/graphwar_server_broker_handler.rs` 的某一行），而那个锚在
//! x86 侧被 **6 个**函数共享（`fixtures/name_to_addr.tsv` 的候选：`0x4587f0`、`0x45a340`、
//! `0x479520`、`0x4c5fa0`、`0x6d6000`、`0x6de750`）⇒ 锚点法在此**不能判等**。判据换成两条：
//!
//! 1. **相邻性**：x86 侧 `stop`=`0x4586d0`、`process`=`0x4588d0` 都已由唯一锚钉死，
//!    而 `0x4587f0` 正好夹在两者之间；arm64 侧 `stop`/`start`/`process` 也是
//!    `0x489e50`/`0x489fa8`/`0x48a078` 连续三个符号；
//! 2. **尺寸比** 224/208 = 1.077（在闸内），其余 5 个候选的尺寸都对不上这个量级
//!    （336 / 544 / 3552 / 1408 / 320 / 2656）。
//!
//! ### 1.2 `new` / `get_game_info` 为什么**根本**没有 x86 地址
//!
//! 两者都**不引用任何 `src/*.rs` 的 `panic::Location`**（arm64 侧就是 0 个本模块锚），
//! 所以"两侧锚点 join"这条路走不通，而它们又都太短、太通用，没有可判等的字面量
//! ⇒ **不给地址**（这比给一个错地址好）。`new` 的 arm64 形状与
//! `GraphwarLanBroadcasterV6::new` **几乎逐条同构**（`__rust_no_alloc_shim` →
//! `__rust_alloc` → `thread_local::Storage::get` → 32 字节表常量 → `Instant::now`），
//! 那一条**推断**写在 §2 里。
//!
//! ## 2. 结构（**观测**：`new` 的写点 + `get_game_info`/`process` 的读点）
//!
//! | 偏移 | 类型 | 含义 | 证据 |
//! |---|---|---|---|
//! | `+0x00`.. | `HashMap<SocketAddr, GameInfo>` | 房间表（桶步长 `0x78`，与 LAN 那张同构） | `0x489d…` 的 32 字节常量 |
//! | 表内 | hasher | `RandomState`（`thread_local` 取种子） | `0x489dc0` `Storage::get` |
//! | 另一字段 | `Mutex<Option<ServerBrokerRequestConnectionClient>>` | 活着的 broker 连接 | `0x489f38 Mutex::lock_contended` + `drop_in_place<…MutexGuard<Option<…Client>>>` |
//!
//! `GraphwarBackend` 把这个对象整体放在 `+0x228`（`docs/09` §11.5），
//! 而 `start_server_broker_handler`（arm64 `0x4077bc`）的守卫是
//! `if backend+0x228 == 0 { 建 }`（`0x4077d0 cbz x8`）⇒ 那个槽**首个 8 字节为 0 = 没有**。
//! 与 LAN 侧那个"首字 `i64::MIN`"的哨兵**不是同一套编码**（这一点是**观测**，
//! 两处为什么不同 = **未知**）。
//!
//! ## 3. `process` 的一帧（arm64 逐条被调者，**观测**）
//!
//! ```text
//! let _probe = TimeProbe::new();            // 0x48a…，落尾 drop_in_place<time_probe::TimeProbe>
//! let mut guard = self.conn.lock().unwrap();   // Mutex::lock_contended ×2
//! if let Some(client) = &mut *guard {
//!     client.process();                     // 0x48a… → ServerBrokerRequestConnectionClient::process
//! }
//! for ev in … {                             // hash_one ×2 + insert / remove_entry ×2
//!     match ev { GameInfo    => rooms.insert(addr, info),
//!                GameRemoved => rooms.remove(addr) }
//! }
//! // TimeProbe::drop：耗时 > 5 ms 才打印（0.005 = 0x3f747ae147ae147b）
//! ```
//!
//! * `hashbrown::HashMap::insert` 与 `RawTable::remove_entry` **成对出现** ⇒ 表既增也删，
//!   与 `_scaffold::broker` 的 `GameInfo` / `GameRemoved` 两条事件一一对应
//!   （那两条的线上形状见 `docs/09` §6）；
//! * `Instant::now` + `Instant::elapsed` + 常量 **`0.005`**（`0.179fa0`）⇒ `TimeProbe`
//!   的 5 ms 阈值：**这是原版留在里面的计时打印**，不是业务逻辑（**推断**：
//!   `TimeProbe` 是游戏自己的小类型，`drop_in_place<time_probe::TimeProbe>` 有符号）。
//!
//! ## 4. 与 LAN 那一路的分工（`docs/09` §11.5）
//!
//! | | LAN（`graphwar_lan_broadcaster_v6`） | Global（本模块） |
//! |---|---|---|
//! | 传输 | UDP + IPv6 组播 `ff02::1` | **TCP** + `u16` 大端帧（`graphwar_stream_buffer`） |
//! | 谁发 | 自己广播 | 连 **broker**，由它转发 |
//! | 表键 | **发送方** `SocketAddr`（广播里带 `address`） | **房间** `SocketAddr`（`GameInfo` 里带 `address`） |
//! | 过期 | 10 s（本地 TTL） | **未知**（本模块里没找到 TTL 常量；可能有 `GameRemoved`，也可能靠 broker 断线） |
//! | 线程 | 无（每帧跑在 `process` 里） | **有**：`start` 起一条分离线程持有连接 |
//!
//! 两条路最终在 `IVBoxContainer::process`（`0x450c20`）里汇成同一张 9 字段表
//! （`docs/09` §10）。
//!
//! ## 5. 未知
//!
//! * 房间表的**过期**：LAN 侧有 10 s TTL，这里的 `process` 只看到 `insert`/`remove_entry`，
//!   **没有** `Instant::now - entry.instant` 那种扫描 ⇒ 可能完全依赖 broker 的
//!   `GameRemoved`；没观测到 `GameRemoved` 真实报文（`docs/09` §6 也是"从未抓到"）⇒ **未知**；
//! * `get_game_info` 的表偏移：与 LAN 那一路同尺寸（460 B）、同形状
//!   （首条 `ldr …#0x90` 查条目数），但**偏移没逐个读**⇒ **推断**同构；
//! * `start` 起的线程**怎么结束**：`JoinHandle` 一拿到就 `drop`（`0x48a020`）⇒ 分离；
//!   线程退出条件 = 连接读到 `Err`/EOF（`server_broker_request_connection` 的 `process` 里没有
//!   "重连"分支）⇒ 关流即线程结束（**推断**）。

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::_scaffold::broker::{parse_game_info_event, parse_game_removed_event};
use crate::_scaffold::protocol::Broadcast;

/// 模块路径（`.symtab` 的 mangled 前缀）。档位：**观测**。
pub const MODULE_PATH: &str = "graphwarrust::graphwar_server_broker_handler";

/// 类型名。档位：**观测**。
pub const TYPE_NAME: &str = "GraphwarServerBrokerHandler";

/// 在 `GraphwarBackend` 里的偏移（`Option<…>` 的那个槽）。档位：**观测**（`docs/09` §11.5）。
pub const BACKEND_OFFSET: usize = 0x228;

/// 房间表桶步长（与 LAN 那张表同构）。档位：**观测**（`0x78`，两处一致）。
pub const BUCKET_STRIDE: usize = 0x78;

/// `TimeProbe` 的打印阈值（秒）。`0x3f747ae147ae147b` = `0.005`。
///
/// 档位：**逐位**（常量池 `0x179fa0`，`process` 里 `ldr d1,[x8,#0xfa0]`）。
pub const TIME_PROBE_THRESHOLD_SECS: f64 = 0.005;

/// `stop` 的 `Location`（`Mutex::lock().unwrap()` 那一处）。档位：**观测**（唯一锚）。
pub const LOC_STOP: (&str, u32, u32) = ("src/graphwar_server_broker_handler.rs", 90, 52);

/// `process` 的展示锚（它共引用 4 个锚，x86 侧命中 3 个）。档位：**观测**。
pub const LOC_PROCESS: (&str, u32, u32) = ("src/graphwar_server_broker_handler.rs", 98, 105);

/// 连接线程体的 `Location`（与 `server_broker_request_connection.rs:66:42` 在同一个函数里）。
pub const LOC_THREAD_BODY: (&str, u32, u32) = ("src/graphwar_server_broker_handler.rs", 60, 55);

/// 函数地址 / 尺寸表：(名字, arm64 vaddr, arm64 字节, x86 vaddr 或 `None`, x86 字节或 `None`)。
///
/// `None` = **不给地址**（没有可判等的锚，见模块文档 §1.2），不是漏做。
pub const FUNCTIONS: &[(&str, u64, usize, Option<u64>, Option<usize>)] = &[
    ("get_game_info", 0x489b98, 460, None, None),
    ("new", 0x489d64, 236, None, None),
    ("stop", 0x489e50, 344, Some(0x4586d0), Some(288)),
    ("start", 0x489fa8, 208, Some(0x4587f0), Some(224)),
    ("process", 0x48a078, 2088, Some(0x4588d0), Some(2560)),
    ("start::{{closure}}", 0x48e130, 1984, Some(0x437ce0), Some(1344)),
];

/// Global 一路的处理器。
///
/// 原版的两个字段是 `HashMap<SocketAddr, GameInfo>` 与
/// `Mutex<Option<ServerBrokerRequestConnectionClient>>`；这里用 `Option<BrokerConn>`
/// 表达"连接是否活着"（单线程语义，锁在真实现里由调用方提供）。
#[derive(Debug, Default)]
pub struct GraphwarServerBrokerHandler {
    /// 房间表：键 = **房间地址**（`GameInfo.address` 解析出来的那个）。
    pub rooms: HashMap<SocketAddr, Broadcast>,
    /// 解析失败的地址串计数（原版会打日志；这里只记数，便于测试断言）。
    pub bad_address: usize,
    /// 活着的 broker 连接（原版 `Mutex<Option<…>>`）。
    pub conn: Option<BrokerConn>,
}

/// [`GraphwarServerBrokerHandler::apply_event`] 的结果。
///
/// 三个状态必须分开：原版 `match` 的臂是 `GameInfo`（增/覆盖）与 `GameRemoved`（删），
/// 而"删一个不存在的房间"**不是**同一种语义——如果都压成 `bool`，
/// `process` 的 (增, 删) 计数就会把"删成功"算成"增"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// `GameInfo`：插入或覆盖一间房。
    Inserted,
    /// `GameRemoved` 且表里确实有这条。
    Removed,
    /// `GameRemoved` 但这间房本来就不在表里。
    NotFound,
}

/// 一条 broker 连接的最小状态（原版的 `ServerBrokerRequestConnectionClient`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerConn {
    /// broker 地址。
    pub broker: SocketAddr,
    /// 是否已发过 `VersionInfo`（第一帧）。
    pub requested: bool,
    /// 线程是否**分离**（`JoinHandle` 立刻被 drop）。
    pub detached: bool,
}

impl GraphwarServerBrokerHandler {
    /// `new`（arm64 `0x489d64`）：建空表。
    ///
    /// 与 `GraphwarLanBroadcasterV6::new` 同构（`Storage::get` 取 `RandomState` 种子）⇒
    /// 本实现只需 `Default`。
    pub fn new() -> Self {
        Self::default()
    }

    /// `start`（arm64 `0x489fa8`）：起一条**分离**线程连 broker。
    ///
    /// 守卫在 `GraphwarBackend::start_server_broker_handler`（`0x4077d0 cbz x8`）——
    /// 槽里没有才建；这里用"已有连接就返回 `false`"表达同一语义。
    pub fn start(&mut self, broker: SocketAddr) -> bool {
        if self.conn.is_some() {
            return false;
        }
        self.conn = Some(BrokerConn {
            broker,
            requested: false,
            detached: true,
        });
        true
    }

    /// 线程体里发的第一帧（`{"VersionInfo":{…}}`），发完把 `requested` 置位。
    ///
    /// 档位：**推断**（见 `server_broker_request_connection` 模块文档 §5）。
    pub fn request(&mut self) -> Option<String> {
        let conn = self.conn.as_mut()?;
        if conn.requested {
            return None;
        }
        conn.requested = true;
        Some(crate::server_broker_request_connection::version_info_frame(
            crate::server_broker_request_connection::VERSION,
            "linux",
        ))
    }

    /// `stop`（arm64 `0x489e50`）：锁住槽、把连接取走（`Drop` 即关流）。
    ///
    /// 原版 `Mutex::lock().unwrap()`（`Location :90:52`）——中毒就 panic；
    /// 这里返回"原来是否有连接"。
    pub fn stop(&mut self) -> bool {
        self.conn.take().is_some()
    }

    /// `get_game_info`（arm64 `0x489b98`）：按**房间地址**查表。
    ///
    /// 与 LAN 那一路同形（首条指令查条目数）⇒ 空表 `None`。
    pub fn get_game_info(&self, addr: &SocketAddr) -> Option<&Broadcast> {
        self.rooms.get(addr)
    }

    /// 处理一条 broker 推来的事件（`{"GameInfo":{…}}` 或 `{"GameRemoved":{…}}`）。
    ///
    /// 返回 [`Applied::Inserted`] / [`Applied::Removed`] / [`Applied::NotFound`]，
    /// `None` = 不是这两类事件（原版这里是 `match GraphwarEvent`，
    /// 我们复用 `_scaffold::broker` 的同形解析）。
    ///
    /// 键取 `GameInfo.address`（不是发送方地址）——这与 LAN 那一路**不同**，
    /// 见模块文档 §4 的对照表。
    pub fn apply_event(&mut self, text: &str) -> Option<Applied> {
        if let Some(info) = parse_game_info_event(text) {
            match info.address.parse::<SocketAddr>() {
                Ok(a) => {
                    self.rooms.insert(a, info);
                    return Some(Applied::Inserted);
                }
                Err(_) => {
                    self.bad_address += 1;
                    return None;
                }
            }
        }
        if let Some(addr) = parse_game_removed_event(text) {
            match addr.parse::<SocketAddr>() {
                Ok(a) => {
                    return Some(if self.rooms.remove(&a).is_some() {
                        Applied::Removed
                    } else {
                        Applied::NotFound
                    })
                }
                Err(_) => {
                    self.bad_address += 1;
                    return None;
                }
            }
        }
        None
    }

    /// `process`（arm64 `0x48a078`）的纯逻辑：把这一轮的事件全部吃进表。
    ///
    /// 返回 `(插入/覆盖数, 删除数)`。原版还会在这里驱动
    /// `ServerBrokerRequestConnectionClient::process` 拿事件——那一步在调用方。
    pub fn process<'a, I>(&mut self, events: I) -> (usize, usize)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut added = 0;
        let mut removed = 0;
        for text in events {
            match self.apply_event(text) {
                Some(Applied::Inserted) => added += 1,
                Some(Applied::Removed) => removed += 1,
                Some(Applied::NotFound) | None => {}
            }
        }
        (added, removed)
    }

    /// 表里有多少条（大厅要显示的条数）。
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }
}

/// 按名字取 [`FUNCTIONS`] 的一行。
pub fn function_named(name: &str) -> Option<&'static (&'static str, u64, usize, Option<u64>, Option<usize>)> {
    FUNCTIONS.iter().find(|f| f.0 == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn info_event(address: &str) -> String {
        crate::_scaffold::broker::game_info_event(&Broadcast {
            room_name: "Room".into(),
            game_state: "Setup".into(),
            current_player_count: 1,
            address: address.into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
            locked: false,
        })
    }

    #[test]
    fn six_entries_and_two_without_x86_address() {
        assert_eq!(FUNCTIONS.len(), 6);
        assert!(function_named("get_game_info").unwrap().3.is_none());
        assert!(function_named("new").unwrap().3.is_none());
        assert_eq!(function_named("stop").unwrap().3, Some(0x4586d0));
        assert_eq!(function_named("process").unwrap().3, Some(0x4588d0));
        // 尺寸比闸：`process` 是**已记录的例外**（1.226，靠 4 个锚里命中 3 个判等，
        // 见模块文档 §1 的表注）；其余三行必须在闸内。
        for (n, _a, asz, _x, xsz) in FUNCTIONS {
            if let Some(xsz) = xsz {
                let r = *xsz as f64 / *asz as f64;
                if *n == "process" {
                    assert!((1.19..1.30).contains(&r), "{n} 超闸 {r}（已记录）");
                } else {
                    assert!((0.61..=1.19).contains(&r), "{n} ratio={r}");
                }
            }
        }
        assert_eq!(BACKEND_OFFSET, 0x228);
    }

    #[test]
    fn start_is_idempotent_and_detached() {
        let mut h = GraphwarServerBrokerHandler::new();
        assert!(h.start(addr("127.0.0.1:42970")));
        assert!(!h.start(addr("127.0.0.1:42970")), "槽里已有 ⇒ 不重建");
        assert!(h.conn.as_ref().unwrap().detached, "JoinHandle 一拿到就 drop");
        // 第一帧只发一次
        let first = h.request().unwrap();
        assert!(first.starts_with("{\"VersionInfo\":{"));
        assert!(h.request().is_none());
        assert_eq!(h.conn.as_ref().unwrap().requested, true);
    }

    #[test]
    fn stop_takes_the_connection_out() {
        let mut h = GraphwarServerBrokerHandler::new();
        assert!(!h.stop(), "本来就没有");
        h.start(addr("127.0.0.1:42970"));
        assert!(h.stop());
        assert!(h.conn.is_none());
        // 停了还能再起（原版 stop 只 take，不置"永久关闭"位）
        assert!(h.start(addr("127.0.0.1:42970")));
    }

    #[test]
    fn room_table_is_keyed_by_the_room_address_not_the_sender() {
        let mut h = GraphwarServerBrokerHandler::new();
        assert_eq!(h.apply_event(&info_event("[::1]:35245")), Some(Applied::Inserted));
        assert_eq!(h.len(), 1);
        assert_eq!(
            h.get_game_info(&addr("[::1]:35245")).unwrap().room_name,
            "Room"
        );
        // 同一房间地址再来一次 ⇒ 覆盖，条数不变
        assert_eq!(h.apply_event(&info_event("[::1]:35245")), Some(Applied::Inserted));
        assert_eq!(h.len(), 1);
        // GameRemoved ⇒ 删
        assert_eq!(
            h.apply_event(&crate::_scaffold::broker::game_removed_event("[::1]:35245")),
            Some(Applied::Removed)
        );
        assert!(h.is_empty());
        // 删不存在的 ⇒ false
        assert_eq!(
            h.apply_event(&crate::_scaffold::broker::game_removed_event("[::1]:1")),
            Some(Applied::NotFound)
        );
    }

    #[test]
    fn unrelated_events_are_ignored_and_bad_addresses_counted() {
        let mut h = GraphwarServerBrokerHandler::new();
        assert_eq!(h.apply_event("{\"TickRequest\":{}}"), None);
        assert_eq!(h.apply_event(&info_event("not-an-address")), None);
        assert_eq!(h.bad_address, 1);
        assert!(h.is_empty());
        assert_eq!(h.process(["{\"TickRequest\":{}}"].into_iter()), (0, 0));
    }

    #[test]
    fn process_counts_inserts_and_removes() {
        let mut h = GraphwarServerBrokerHandler::new();
        let evs = vec![
            info_event("[::1]:1"),
            info_event("[::1]:2"),
            crate::_scaffold::broker::game_removed_event("[::1]:1"),
        ];
        assert_eq!(h.process(evs.iter().map(|s| s.as_str())), (2, 1));
        assert_eq!(h.len(), 1);
        assert!(h.get_game_info(&addr("[::1]:2")).is_some());
    }

    #[test]
    fn trace_constants() {
        assert_eq!(LOC_STOP.1, 90);
        assert_eq!(LOC_PROCESS.1, 98);
        assert_eq!(LOC_THREAD_BODY.1, 60);
        assert_eq!(BUCKET_STRIDE, 0x78);
        assert!((TIME_PROBE_THRESHOLD_SECS - 0.005).abs() < 1e-12);
        assert_eq!(MODULE_PATH, "graphwarrust::graphwar_server_broker_handler");
        assert_eq!(TYPE_NAME, "GraphwarServerBrokerHandler");
    }
}
