//! `graphwar_lan_broadcaster_v6`：局域网房间广播器 —— UDP / IPv6 组播 `ff02::1` /
//! 明文 JSON / **换行分帧**，**同时**收别人的广播。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（arm64 `.symtab` 权威：模块 `graphwarrust::graphwar_lan_broadcaster_v6`）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 比 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `get_game_info` | `0x3ddce8` / 460  | **无锚** | — | 按发送方地址查房间表 | **观测**（arm64）+ **推断**（语义） |
//! | `set_room_info` | `0x3ddeb4` / 432  | `0x4572c0` / 448  | 1.037 | 写本机房间信息（含地址串） | **观测** |
//! | `new`           | `0x3de064` / 312  | `0x457480` / 384  | 1.231 | 解析组播地址、建表、起表 | **观测** |
//! | `start`         | `0x3de19c` / 372  | `0x457600` / 400  | 1.075 | bind `::` → join 组播 → 非阻塞 | **观测** |
//! | `process`       | `0x3de310` / 2040 | `0x457790` / 2208 | 1.082 | 收 → 过期清理 → 到点发 | **观测** |
//!
//! `new` 的尺寸比 1.231 略超 0.61–1.19 的闸，但 `fixtures/name_to_addr.tsv` 给了它
//! **唯一锚** `src/graphwar_lan_broadcaster_v6.rs:35:51`（两侧一对一），所以归属以锚为准，
//! 尺寸不参与判定。`get_game_info` 在**两侧都一个 `panic::Location` 都没有**，
//! 所以 **x86 地址无法落地**（这是全模块唯一的"无 x86 对位"，不是漏做）。
//!
//! ## 2. 结构（字段偏移 **观测**：`new` 的写点 + `process`/`get_game_info` 的读点）
//!
//! | 偏移 | 类型 | 含义 | 证据 |
//! |---|---|---|---|
//! | `+0x00` | `u64` | 房间信息哨兵：`new` 写 `i64::MIN`，`process` 见到它就**不发**广播 | `0x3de130` / `0x3de7b0` |
//! | `+0x48` / `+0x50` | `Instant{secs,nanos}` | **上一次广播的时刻**（`new` 用 `Instant::now()` 初始化） | `0x3de134` / `0x3de7bc` |
//! | `+0x58` | `u16` | 目标 `SocketAddr` 的判别值（`new` 写 `1` = V6） | `0x3de104` |
//! | `+0x5a`..`+0x6a` | 16 B | 目标组播地址（来自 `ff02::1`） | `0x3de0fc` |
//! | `+0x6a` | `u16` | 目标端口 | `0x3de118` |
//! | `+0x6c`/`+0x70` | `u32`×2 | `flowinfo` / `scope_id`（都清 0） | `0x3de120` |
//! | `+0x74` | `u16` | 本机广播端口（`new` 的入参） | `0x3de128` |
//! | `+0x78` | `*mut u8` | 房间表 hashbrown 的 **ctrl 指针** | `0x3ddd18` |
//! | `+0x80` | `usize` | 房间表 `bucket_mask`（`get_game_info` 里 `and` 它取桶） | `0x3ddd3c` |
//! | `+0x88` | `usize` | `growth_left` | 清理循环 `0x3de76c` |
//! | `+0x90` | `usize` | **条目数**（`0` ⇒ `get_game_info` 直接返回 `None`） | `0x3ddce8` 首条指令 |
//! | `+0x98` | 16 B | `RandomState`（表的 hasher） | `0x3ddd00` `hash_one(&self+0x98,…)` |
//! | `+0xa8` | 16 B | `ff02::1` 的副本（join 用） | `0x3de0f8` |
//! | `+0xb8` | `i32` | UDP socket 的 fd；**`-1` = 还没 bind** | `0x3de0f0` / `0x3de38c` |
//! | `+0xbc` | `u16` | 同 `+0x74` 的端口副本（`start` 里 bind 用） | `0x3de10c` |
//!
//! 结构总长 **`0xc0` = 192 字节**：`GraphwarBackend::start_lan_broadcaster` 把它整块
//! 拷进 `backend+0x110`，拷的就是 6 条 `ldp q/stp q` = `6×32`（`0x407258`…`0x407284`）。
//! 房间表桶步长 **`0x78` = 120 B**，与 `graphwar_lan_vbox` 记的常量一致（那是对同一张表的第二处观测）。
//!
//! ## 3. 三个必答结论
//!
//! ### 3.1 广播周期 = **1 秒**（`process` 里的 `elapsed > 1s`）
//!
//! ```text
//! let now = Instant::now();
//! let elapsed = now - self.last_broadcast;     // 0x3de7b8 / 0x3de7c4
//! if elapsed <= Duration::from_secs(1) { return }   // 0x3de7c8/0x3de7cc/0x3de7d4
//! … serialize; send_to
//! self.last_broadcast = now;                  // 0x3de8d4/0x3de8d8
//! ```
//!
//! 逐位形状是"`secs == 0` ⇒ 不发；`secs != 1` ⇒ 发；`secs == 1 && nanos == 0` ⇒ 不发"，
//! 三者合起来正好等于 `elapsed > 1 s`。⇒ **`docs/09` §11 里没写的周期是 1 秒**。
//! 注意它不是"每帧发"：`process` 每帧都会被 `GraphwarBackend::process` 调（`0x408224`），
//! 但内部按这个 1 秒闸限流。
//!
//! ### 3.2 收到的广播怎么去重 = **按"发送方 `SocketAddr`"覆盖 + 10 秒过期**
//!
//! 唯一的键是**发送方地址**（不是房间名、不是房间地址字段）：
//! `HashMap<SocketAddr, (GameInfo, Instant)>` 插在 `self+0x78`（`0x3de638` `insert`）。
//! ⇒ 同一个 host 连发 10 次只留**一条**（后写覆盖前写），这就是去重；
//! 换个 host 发同样房名则是**两条**（所以 LAN 列表里可能出现同名房间）。
//!
//! 过期：`process` 每帧扫一遍表，`Instant::now() - entry.instant` 的
//! **`secs >= 10`** 就删（`0x3de6e4 cmp x0,#0xa` + `b.lo` 保留）。⇒ TTL = **10 秒**。
//! 派生性质（**推断**）：host 掉线后，最长 10 秒它还会留在别人的列表里。
//!
//! ### 3.3 广播里的 `address` 字段恒为 `[::1]:<port>`
//!
//! `set_room_info` 内部 `IpAddr::from_str("::1")`（arm64 `0x3ddf08`；x86 侧字面量
//! `::1` 在 `0x4572ee`），再拼上端口 ⇒ `"[::1]:35245"` 这种形式。
//! 这解释了 `docs/09` §1.1 抓到的 385 个样本里 `address` 全是 `[::1]:…`
//! （当时只记为"观测"，现在有**成因**）。语义上"LAN 房间里显示的是环回地址"是
//! **推断**（依据：字面量是 `::1`，不是本机网卡地址），实现上等价于
//! [`local_broadcast_address`]。
//!
//! ## 4. 帧格式：**换行分帧、无长度前缀**（与 TCP 的 `u16` 前缀是两套）
//!
//! 收侧 `recv_from` 到**一个 1000 字节的栈缓冲**（`0x3de398 mov w2,#0x3e8`，
//! 先整块清零再收），然后：
//!
//! ```text
//! if n >= 1001 { slice_index_fail }          // 0x3de400 cmp x1,#0x3e9
//! str::from_utf8(&buf[..n])  else 丢弃       // 0x3de418
//! serde_json::from_str::<GameInfo>(text)      // 0x3de444
//! rooms.insert(sender_addr, (info, Instant::now()))
//! ```
//!
//! ⇒ **一条 UDP 数据报 = 一条 JSON**（数据报边界就是帧边界），`docs/09` §1.1 记的
//! "无长度前缀、明文 JSON" 与此一致；发侧是 `send_to` 一条序列化好的 JSON（无换行、
//! 无 NUL）——`docs/09` §11.5 写的"换行分帧"应理解为**面向行**的文本协议特性，
//! 本构建里 `process` 并不追加 `\n`（**观测**：`GameInfo::serialize` 之后直接
//! `send_to(buf, len)`，`0x3de808` → `0x3de95c`）。
//!
//! ## 5. 错误与非阻塞语义
//!
//! * `recv_from` 的错误按 `ErrorKind` 分：**`13`（`WouldBlock`）直接吞掉**
//!   （`0x3de3d8` `Error::kind` + `0x3de3e0 cmp w8,#0xd`），其余打日志后继续
//!   （字面量 `"LAN broadcast socket error, closing: "`，x86 `0x1b0285`）；
//! * `fd == -1`（没 `start` 过）⇒ **整段收包跳过**（`0x3de384 cmn w9,#1`）；
//! * 发送侧的目标 `SocketAddr` 判别值若落在 `2`/`3`，走"地址不可用"分支
//!   （`0x3de914` 两处比较），x86 侧对应字面量 `"no addresses to send data to"`。
//!
//! ## 6. 未知
//!
//! * `+0x00` 那个 `i64::MIN` 哨兵**到底是什么类型**：`new` 写它、`process` 拿它当
//!   "别发"的闸，`set_room_info` 会改它；但 `GraphwarBackend::start_lan_broadcaster`
//!   的守卫比的是 `0x8000_0000_0000_0001`（`i64::MIN+1`），**两个值差 1**，
//!   本轮没能把这两处对成同一个枚举 ⇒ **未知**（不影响周期/去重结论）；
//! * 广播端口从**哪来**：`new(port)` 的实参由
//!   `GraphwarBackend::start_lan_broadcaster`（arm64 `0x407220`）透传，
//!   而它自己的实参来自更上层；抓包观测到的目的端口是 **55463**，
//!   该常量不在本模块里（**未知**：在本模块之外赋值）；
//! * `get_game_info` 的**返回类型**：`&GameInfo` 还是拷贝？`0x3ddce8` 走的是
//!   hashbrown `find` 后直接返回桶内指针（**推断** `Option<&GameInfo>`），
//!   没有二次分配的证据，但也没有类型级证据。
//!
//! ## 7. 复现
//!
//! ```text
//! # 抓一条 LAN 广播（需要一台同网段的机器）
//! tcpdump -n -i any udp port 55463 -X
//! ```

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6};
use std::time::{Duration, Instant};

use crate::_scaffold::protocol::Broadcast;

/// 组播组地址字面量，**7 字节**。档位：**逐位**（arm64 `.rodata 0x19be2e` +
/// `mov w1,#7`；x86 `0x1c0f0d`）。
pub const GROUP_ADDR_V6: &str = "ff02::1";

/// `start` 里 bind 的本地地址字面量，**2 字节**。档位：**逐位**
/// （arm64 `.rodata`，`start` 的 `IpAddr::from_str`；x86 `0x1c0f14` 的 `::`）。
/// 绑 `::`（unspecified）才能收到组播。
pub const BIND_ADDR: &str = "::";

/// `set_room_info` 里拼进广播的**本机**地址字面量，**3 字节**。档位：**逐位**
/// （arm64 `0x3ddf08` `IpAddr::from_str`；x86 `0x1c0e35` `::1`）。
pub const LOCAL_ADDR: &str = "::1";

/// 收包缓冲大小 = `0x3e8`。档位：**逐位**（`0x3de398 mov w2,#0x3e8`）。
pub const RECV_BUF: usize = 1000;

/// 一条广播的存活时间（秒）：`secs >= 10` 即删。档位：**逐位**（`0x3de6e4 cmp x0,#0xa`）。
pub const ENTRY_TTL_SECS: u64 = 10;

/// 广播周期（秒）：`elapsed > 1s` 才发。档位：**逐位**
/// （`0x3de7c8`/`0x3de7cc`/`0x3de7d4` 三段比较合成 `> 1s`）。见模块文档 §3.1。
pub const BROADCAST_PERIOD_SECS: u64 = 1;

/// 房间表桶步长（字节）`0x78`。档位：**观测**（`0x3de6bc mov w9,#0x78`；
/// 与 `graphwar_lan_vbox` 的同一常量互相印证）。
pub const BUCKET_STRIDE: usize = 0x78;

/// 房间表 hashbrown 各字段的结构偏移。档位：**观测**（见模块文档 §2 表）。
pub const OFF_ROOMS: usize = 0x78;
/// 条目数偏移。
pub const OFF_ROOMS_LEN: usize = 0x90;
/// hasher 偏移。
pub const OFF_ROOMS_HASHER: usize = 0x98;

/// "socket 还没 bind" 的 fd 值。档位：**观测**（`0x3de0f0 str w8,#-1` / `0x3de384`）。
pub const FD_NONE: i32 = -1;

/// 结构总长（`0xc0`）。档位：**观测**（6×32 字节整块拷贝，`0x407258`…`0x407284`）。
pub const STRUCT_SIZE: usize = 0xc0;

/// `graphwar_lan_vbox` 里的房间表偏移（对同一张表的第二处观测）。
///
/// 那个模块从 `模块容器 +0x128` 取广播器、再读 `+0x1a0`；本模块的表在 `+0x78`。
/// 两者**不是同一个偏移**，说明 vbox 读的是**另一张表**（很可能是"已显示条目"缓存）。
/// 档位：**未知**（本轮只确认本模块的表在 `+0x78`）。
pub const VBOX_ROOM_TABLE_OFF: usize = 0x1a0;

/// 一条被收到的房间记录：广播内容 + 收到它的时刻（TTL 用）。
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    /// 广播里的 9 字段房间信息。
    pub info: Broadcast,
    /// 收到时刻（`process` 里 `Instant::now()`，清理时用来算年龄）。
    pub seen_at: Instant,
}

/// `GraphwarLanBroadcasterV6`：广播器本体。
///
/// 原版把 socket 直接放在 `+0xb8`；这里用 `Option<SocketAddr>` 之外的东西不引进来，
/// 只保留**可判定的状态**：表、组播地址、上次发送时刻、端口、是否 bind 过。
#[derive(Debug)]
pub struct GraphwarLanBroadcasterV6 {
    /// 房间表：键 = **发送方地址**（去重就发生在这里）。
    pub rooms: HashMap<SocketAddr, Room>,
    /// 组播组（`ff02::1`）+ 广播端口。
    pub group: SocketAddr,
    /// bind 过的本地端口（`None` = 还没 `start`，等价于原版 `fd == -1`）。
    pub bound_port: Option<u16>,
    /// 上一次广播的时刻（原版 `+0x48`/`+0x50`）。`None` = `new` 时的"刚起表"状态。
    pub last_broadcast: Option<Instant>,
    /// 本机房间信息（原版 `+0x00` 那个哨兵字段，`None` = 还没 `set_room_info`）。
    pub room_info: Option<Broadcast>,
    /// 发出去 / 收进来的条数（只用于测试与自检，原版没有这个字段）。
    pub seq: u64,
}

impl GraphwarLanBroadcasterV6 {
    /// `new(port)`（arm64 `0x3de064`）：**解析 `ff02::1`** → 建空表 → 记 `Instant::now()`。
    ///
    /// 原版在这里 `Ipv6Addr::from_str("ff02::1").unwrap()`（`.unwrap()` 的
    /// `panic::Location` 是 `:35:51`），并把 fd 置 `-1`（还没 bind）。
    ///
    /// # Panics
    ///
    /// `GROUP_ADDR_V6` 是编译期常量，解析不可能失败；真失败说明常量被改坏了。
    pub fn new(port: u16) -> Self {
        let ip: Ipv6Addr = GROUP_ADDR_V6.parse().expect("ff02::1 解析失败");
        GraphwarLanBroadcasterV6 {
            rooms: HashMap::new(),
            group: SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0)),
            bound_port: None,
            last_broadcast: None,
            room_info: None,
            seq: 0,
        }
    }

    /// `start`（arm64 `0x3de19c`）：bind `::` → `join_multicast_v6(ff02::1)` → `set_nonblocking(true)`。
    ///
    /// 三步各自 `.unwrap()`（`panic::Location` `:59:74`）。这里只记状态——
    /// 真 socket 的创建在调用方（Godot/我们的外壳），本模块不碰网络。
    /// 返回 `true` 表示"从没 bind 到 bind 了"（第二次调用是 no-op，与原版一致：
    /// 原版 `start` 里没有"已 bind 就退出"的守卫，**推断**由调用方保证只调一次）。
    pub fn start(&mut self, port: u16) -> bool {
        if self.bound_port.is_some() {
            return false;
        }
        self.bound_port = Some(port);
        true
    }

    /// 这一帧该不该广播（`process` 的时间闸，模块文档 §3.1）。
    ///
    /// 判据是 `elapsed > Duration::from_secs(1)`；`None`（刚 `new`）按"现在"算。
    pub fn due_to_broadcast(&mut self, now: Instant) -> bool {
        let Some(last) = self.last_broadcast else {
            // 原版 `new` 已经把 `Instant::now()` 写进 +0x48，所以第一次 `process`
            // 的 elapsed 很小 ⇒ 不会立刻发；这里用"从 now 起算"复现同一行为。
            self.last_broadcast = Some(now);
            return false;
        };
        if now.duration_since(last) > Duration::from_secs(BROADCAST_PERIOD_SECS) {
            self.last_broadcast = Some(now);
            true
        } else {
            false
        }
    }

    /// 过期清理（`process` 的扫描段，`0x3de654`…`0x3de7a0`）：返回删掉的条数。
    ///
    /// 判据 `secs >= 10`（`cmp x0,#0xa` + `b.lo` 保留）。
    pub fn expire_stale(&mut self, now: Instant) -> usize {
        let before = self.rooms.len();
        self.rooms.retain(|_, r| {
            now.duration_since(r.seen_at) < Duration::from_secs(ENTRY_TTL_SECS)
        });
        before - self.rooms.len()
    }

    /// 收一条广播（`process` 的收包段）：`from` = 发送方地址，`bytes` = 数据报原文。
    ///
    /// 返回 `true` 表示"解析成功并写入/覆盖了表"。**不给 `from` 打任何标记**——
    /// 原版也不区分"自己的回环广播"，只是覆盖同一键。
    pub fn ingest(&mut self, from: SocketAddr, bytes: &[u8], now: Instant) -> bool {
        if bytes.len() > RECV_BUF {
            return false;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        let Some(info) = Broadcast::parse(text.trim()) else {
            return false;
        };
        self.rooms.insert(from, Room { info, seen_at: now });
        self.seq += 1;
        true
    }

    /// `get_game_info`（arm64 `0x3ddce8`）：按**发送方地址**查表。
    ///
    /// 原版首条指令就是 `ldr x8,[x0,#0x90]`（条目数）+ `cbz` ⇒ 空表直接 `None`。
    pub fn get_game_info(&self, from: &SocketAddr) -> Option<&Broadcast> {
        self.rooms.get(from).map(|r| &r.info)
    }

    /// `set_room_info`（arm64 `0x3ddeb4`）：写本机房间信息。
    ///
    /// 地址串**由本函数生成**（`IpAddr::from_str("::1")` + 端口），不是外部传入，
    /// 见模块文档 §3.3。原版把整份 9 字段写进 `+0x00` 起始的那块。
    pub fn set_room_info(&mut self, mut info: Broadcast, port: u16) {
        info.address = local_broadcast_address(port);
        self.room_info = Some(info);
    }

    /// 到点要发出去的那一条 JSON（`GameInfo::serialize` + `send_to`）。
    ///
    /// 原版没有换行、没有 NUL（模块文档 §4）。
    pub fn broadcast_payload(&self) -> Option<String> {
        let info = self.room_info.as_ref()?;
        Some(info.to_json())
    }

    /// 一条 `process` 的**纯逻辑骨架**：清理 → 收包 → 到点发。真 IO 在调用方。
    ///
    /// 返回 `(删掉的过期条数, 要发出去的负载)`。

    pub fn process(
        &mut self,
        now: Instant,
        incoming: &[(SocketAddr, Vec<u8>)],
    ) -> (usize, Option<String>) {
        let expired = self.expire_stale(now);
        for (from, bytes) in incoming {
            self.ingest(*from, bytes, now);
        }
        if self.due_to_broadcast(now) {
            (expired, self.broadcast_payload())
        } else {
            (expired, None)
        }
    }
}

/// `"[::1]:<port>"` —— 广播 `address` 字段的唯一形态（模块文档 §3.3）。
///
/// 单独抽出来是因为它可测：`SocketAddrV6` 的 `Display` 会给 IPv6 加方括号，
/// 与抓包样本 `"[::1]:35245"` 逐字节一致。
pub fn local_broadcast_address(port: u16) -> String {
    let ip: IpAddr = LOCAL_ADDR.parse().expect("::1 解析失败");
    SocketAddr::new(ip, port).to_string()
}

/// 广播目标是不是"组播 + 期望端口"（`start` 的 join 参数检查）。
pub fn is_group_target(addr: &SocketAddr, port: u16) -> bool {
    match addr {
        SocketAddr::V6(v6) => v6.ip().is_multicast() && v6.port() == port,
        SocketAddr::V4(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(address: &str) -> Broadcast {
        Broadcast {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            current_player_count: 2,
            address: address.into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
            locked: false,
        }
    }

    fn addr(last: u8) -> SocketAddr {
        format!("127.0.0.1:3524{}", last).parse().unwrap()
    }

    #[test]
    fn group_and_bind_literals() {
        assert_eq!(GROUP_ADDR_V6, "ff02::1");
        assert_eq!(GROUP_ADDR_V6.len(), 7);
        assert_eq!(BIND_ADDR, "::");
        assert_eq!(LOCAL_ADDR, "::1");
        let b = GraphwarLanBroadcasterV6::new(55463);
        assert_eq!(b.group.port(), 55463);
        assert!(is_group_target(&b.group, 55463));
        assert!(!is_group_target(&b.group, 55464));
        assert!(!is_group_target(&addr(5), 55463));
    }

    #[test]
    fn broadcast_period_is_one_second() {
        let t0 = Instant::now();
        let mut b = GraphwarLanBroadcasterV6::new(55463);
        b.set_room_info(info("[::1]:35245"), 35245);
        // 第一次：记下起点，不发
        assert!(!b.due_to_broadcast(t0));
        // 0.9 s：不发
        assert!(!b.due_to_broadcast(t0 + Duration::from_millis(900)));
        // 正好 1.0 s：**不发**（判据是 > 1s，不是 >=）
        assert!(!b.due_to_broadcast(t0 + Duration::from_secs(1)));
        // 1.001 s：发
        assert!(b.due_to_broadcast(t0 + Duration::from_millis(1001)));
        // 发完立刻再问：不发
        assert!(!b.due_to_broadcast(t0 + Duration::from_millis(1001)));
    }

    #[test]
    fn entry_ttl_is_ten_seconds() {
        let t0 = Instant::now();
        let mut b = GraphwarLanBroadcasterV6::new(55463);
        assert!(b.ingest(addr(1), info("[::1]:1").to_json().as_bytes(), t0));
        assert_eq!(b.rooms.len(), 1);
        assert_eq!(b.expire_stale(t0 + Duration::from_secs(9)), 0);
        assert_eq!(b.rooms.len(), 1);
        assert_eq!(b.expire_stale(t0 + Duration::from_secs(10)), 1);
        assert!(b.rooms.is_empty());
    }

    #[test]
    fn dedup_is_by_sender_address() {
        let t0 = Instant::now();
        let mut b = GraphwarLanBroadcasterV6::new(55463);
        let mut a = info("[::1]:1");
        let mut c = info("[::1]:2");
        a.room_name = "A".into();
        c.room_name = "C".into();
        assert!(b.ingest(addr(1), a.to_json().as_bytes(), t0));
        assert!(b.ingest(addr(1), c.to_json().as_bytes(), t0));
        assert_eq!(b.rooms.len(), 1, "同一发送方覆盖，不新增");
        assert_eq!(b.get_game_info(&addr(1)).unwrap().room_name, "C");
        // 换个发送方、同样房名 ⇒ 两条
        assert!(b.ingest(addr(2), c.to_json().as_bytes(), t0));
        assert_eq!(b.rooms.len(), 2);
        assert!(b.get_game_info(&addr(3)).is_none());
    }

    #[test]
    fn broadcast_address_is_loopback_with_port() {
        assert_eq!(local_broadcast_address(35245), "[::1]:35245");
        let mut b = GraphwarLanBroadcasterV6::new(55463);
        b.set_room_info(info("ignored"), 35245);
        let payload = b.broadcast_payload().unwrap();
        assert!(payload.contains("\"address\":\"[::1]:35245\""), "{payload}");
        assert!(!payload.contains('\n'), "无换行");
    }

    #[test]
    fn process_orders_expire_then_ingest_then_send() {
        let t0 = Instant::now();
        let mut b = GraphwarLanBroadcasterV6::new(55463);
        b.set_room_info(info("x"), 35245);
        assert!(b.ingest(addr(1), info("[::1]:1").to_json().as_bytes(), t0));
        // 0.5 s：还没到点
        let (exp, out) = b.process(t0 + Duration::from_millis(500), &[]);
        assert_eq!((exp, out.is_none()), (0, true));
        // 2 s 后：过期清理 0 条（离 10 s 还早）、到点发
        let (exp, out) = b.process(t0 + Duration::from_secs(2), &[]);
        assert_eq!(exp, 0);
        assert!(out.is_some());
        // 11 s 后：过期 1 条
        let (exp, _) = b.process(t0 + Duration::from_secs(11), &[]);
        assert_eq!(exp, 1);
    }

    #[test]
    fn struct_layout_constants() {
        assert_eq!(STRUCT_SIZE, 0xc0);
        assert_eq!(BUCKET_STRIDE, 0x78);
        assert_eq!(OFF_ROOMS, 0x78);
        assert_eq!(OFF_ROOMS_LEN, 0x90);
        assert_eq!(OFF_ROOMS_HASHER, 0x98);
        assert_eq!(RECV_BUF, 1000);
        assert_eq!(FD_NONE, -1);
    }
}
