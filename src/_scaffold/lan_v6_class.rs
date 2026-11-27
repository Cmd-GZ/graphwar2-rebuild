//! **不是原版的东西**：把已经是复原实现的 **LAN 广播器流程**接给 GDScript 的 socket 层。
//!
//! # 为什么会有这一份（T67 / `docs/06` §118 / `docs/06` §90 C 档第 6 段）
//!
//! `client/src/net/discovery.gd`（改前 215 行）是"**监听方**"：UDP 收包 → 解析 9 字段
//! `GameInfo` → 按房间去重 → TTL 过期 → 给界面一行一条。其中"解析/去重/TTL"这三件
//! 在 Rust 侧**早就复原了**（`src/graphwar_lan_broadcaster_v6.rs` 的
//! `ingest` / `expire_stale` / `due_to_broadcast` / `process`，都是原版
//! `GraphwarLanBroadcasterV6` 的同名方法），`.gd` 里那一份是**第二份实现**。
//!
//! 本类只做**接线**：把"收 → 过期 → 到点发"这条流程交给 Rust 的
//! [`GraphwarLanBroadcasterV6`](crate::graphwar_lan_broadcaster_v6::GraphwarLanBroadcasterV6)，
//! GDScript 只留**引擎侧**那部分（`PacketPeerUDP` 的 bind / `get_packet` / 信号）。
//!
//! # 原版里没有这个类
//!
//! 原版 `libgraphwarrust.so` 里没有 `GwLanV6`：`GraphwarLanBroadcasterV6` 自己持有
//! `socket2` 的 UDP socket（字段 `+0xb8` = fd），收包就在它的 `process` 里。
//! 我们的客户端不能用 Rust 侧 socket（`AGENTS.md` §0.4 的仿制品形态），
//! 所以这里把"**表 + 时间闸 + 解析**"留在 Rust，socket 由 Godot 侧持有。
//!
//! # 键是谁（这是改前的一个**语义错**）
//!
//! 房间表的键是**发送方 `SocketAddr`**（原版 `0x3de638 insert`，模块文档 §3.2），
//! **不是**报文里的 `address` 字段。`.gd` 改前用 `String(info["address"])` 当键 ⇒
//! 两个不同 host 广播同一个 `address` 时原版是**两行**、我方合成一行。
//! 本类的 `ingest(from, …)` 就是按发送方地址入表。
//!
//! # 接口（`client/src/net/discovery.gd` 用）
//!
//! ```gdscript
//! var v6 := GwLanV6.new()
//! v6.start(42968)                                  # 记 bind（真 socket 在 .gd）
//! v6.ingest("127.0.0.1:5353", bytes, now_ms)       # 收一条（返回是否入表）
//! v6.expire_stale(now_ms)                          # TTL 10 s，返回删掉几条
//! v6.due_to_broadcast(now_ms)                      # 周期 1 s 的闸
//! v6.rooms()                                       # 按发送方地址排序的行
//! v6.set_room_info(...) / v6.broadcast_payload()   # 本机房间 → 广播负载
//! v6.provenance()                                  # 出处/证据（自检与文档复核）
//! ```
//!
//! `now_ms` 是**调用方推进的毫秒时钟**（`.gd` 的 `_clock_ms`，由 `poll(delta_ms)` 累加，
//! 不读墙钟）—— 这样自检与截图基准可复现。本类把它映射到 `Instant`（`t0 + now_ms`）。

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, Instant};

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::_scaffold::protocol::Broadcast;
use crate::graphwar_lan_broadcaster_v6 as lan;

/// LAN 房间表的**流程出口**（scaffold；原版没有这个类，见模块文档）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwLanV6 {
    base: Base<RefCounted>,
    inner: lan::GraphwarLanBroadcasterV6,
    /// `now_ms` 的零点（`init` 时的 `Instant`）。
    t0: Instant,
    /// 收进来多少条（诊断口；`ingest` 成功后 +1）。
    ingests: i64,
    /// 过期删掉多少条（诊断口）。
    expired_total: i64,
}

#[godot_api]
impl IRefCounted for GwLanV6 {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            // 端口 0 = 还没 bind（与原版 fd = -1 同义）；真端口由 `start()` 写。
            inner: lan::GraphwarLanBroadcasterV6::new(0),
            t0: Instant::now(),
            ingests: 0,
            expired_total: 0,
        }
    }
}

/// `now_ms` → `Instant`（`t0 + ms`；负数/溢出一律夹到 `t0`）。
fn at(t0: Instant, now_ms: i64) -> Instant {
    let ms = now_ms.max(0) as u64;
    t0.checked_add(Duration::from_millis(ms)).unwrap_or(t0)
}

/// `"ip:port"` / `"1.2.3.4:9"` / `"[::1]:9"` / `"::1:9"` → `SocketAddr`。
///
/// Godot 的 `PacketPeerUDP.get_packet_ip()` 对 IPv6 给不带方括号的原文（`::1`），
/// 直接拼 `":port"` 会得到 `::1:9` —— 也在这里收。
fn parse_peer(s: &str) -> Option<SocketAddr> {
    let t = s.trim();
    if let Ok(a) = SocketAddr::from_str(t) {
        return Some(a);
    }
    // 不带方括号的 IPv6：`::1` + `:9` → 拆最后一节端口，主机部分补方括号。
    let (host, port) = t.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = IpAddr::from_str(host).ok()?;
    Some(SocketAddr::new(ip, port))
}

/// `time_mode` 必须是 `Timer<正整数><s|m>` 形状 —— 否则**整条丢弃**。
///
/// ⚠️ **推断**（不是逐位）：`docs/115` §5.3 的变体 E 观测到原版客户端**丢弃**了
/// `"Timers1m"` 那条广播（合法形状的 5 个变体里只有它被丢）。最自然的解释是
/// `time_mode` 在 Rust 侧反序列化成枚举、非法字符串让整条解析失败；但**判定发生在
/// 哪一层没有观测到**。这条规则改前写在 `client/src/net/discovery.gd`，
/// T67 原样搬到这里（只有一处，且注释里标着它是推断）。
///
/// 缺的证据：`game_info` 反序列化点的 `deserialize_enum` 调用与错误分支。
fn is_known_time_mode(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("Timer") else {
        return false;
    };
    let mut chars = rest.chars();
    let Some(unit) = rest.chars().last() else {
        return false;
    };
    if unit != 's' && unit != 'm' {
        return false;
    }
    let digits: String = rest.chars().take(rest.chars().count() - 1).collect();
    if digits.is_empty() || chars.next().is_none() {
        return false;
    }
    digits.chars().all(|c| c.is_ascii_digit()) && digits.parse::<u32>().map(|n| n > 0).unwrap_or(false)
}

/// 一条 `Broadcast` → GDScript 字典（**键名与改前 `.gd` 的行逐字一致**，调用方零改动）。
fn row_dict(from: &SocketAddr, info: &Broadcast, now_ms: i64, seen_ms: i64) -> VarDictionary {
    let mut d = VarDictionary::new();
    let from_s = from.to_string();
    d.set("from", &GString::from(from_s.as_str()));
    d.set("room_name", &GString::from(info.room_name.as_str()));
    d.set("game_state", &GString::from(info.game_state.as_str()));
    d.set("current_player_count", info.current_player_count);
    d.set("address", &GString::from(info.address.as_str()));
    d.set("axis_mode", &GString::from(info.axis_mode.as_str()));
    d.set("function_mode", &GString::from(info.function_mode.as_str()));
    d.set("turn_mode", &GString::from(info.turn_mode.as_str()));
    d.set("time_mode", &GString::from(info.time_mode.as_str()));
    d.set("locked", info.locked);
    // 改前 `.gd` 的行有 `last_seen_ms`（TTL 判据的输入）。现在 TTL 在 Rust 里算，
    // 但界面/自检还在读这个键，所以照旧给：`last_seen_ms` = 入表时刻，`age_ms` = 现在多老。
    d.set("last_seen_ms", seen_ms);
    d.set("age_ms", now_ms.saturating_sub(seen_ms));
    d
}

#[godot_api]
impl GwLanV6 {
    /// 记一次 bind（真 socket 在 `.gd` 建；这一步只复现原版 `start` 的状态迁移）。
    ///
    /// 返回 `true` = 从"没 bind"变成"bind 了"（第二次调用 no-op，与原版 `start` 一致）。
    #[func]
    fn start(&mut self, port: i64) -> bool {
        let p = u16::try_from(port.max(0)).unwrap_or(0);
        self.inner.start(p)
    }

    /// 收下 bind 状态（等价于 `stop()`：原版在这一层没有 stop，socket 关掉即结束）。
    #[func]
    fn stop(&mut self) {
        self.inner.rooms.clear();
        self.inner.bound_port = None;
        self.inner.last_broadcast = None;
    }

    /// 端口冲突时也要能"假装没 bind"（`.gd` 的 bind 失败分支用）。
    #[func]
    fn clear_bind(&mut self) {
        self.inner.bound_port = None;
    }

    /// bind 过没有（`false` ⇒ 原版 `fd == -1`）。
    #[func]
    fn is_bound(&self) -> bool {
        self.inner.bound_port.is_some()
    }

    /// 收一条广播（原版 `process` 的收包段 = `0x3de5xx` 一组 + `insert` `0x3de638`）。
    ///
    /// * `from` —— **发送方地址** `"ip:port"`（Godot 的 `get_packet_ip()` + `get_packet_port()`）；
    /// * `bytes` —— 数据报原文（一条，不需要换行）；
    /// * `now_ms` —— 调用方的毫秒时钟（入表时刻）。
    ///
    /// 返回 `true` = 解析成功并写入/覆盖了表。
    #[func]
    fn ingest(&mut self, from: GString, bytes: PackedByteArray, now_ms: i64) -> bool {
        let Some(addr) = parse_peer(&from.to_string()) else {
            return false;
        };
        let raw = bytes.to_vec();
        // 受控实验（`docs/archive/115` §5.3）的两条结论都落在**解析层**：
        // ① 变体 B（事件外壳）/ 变体 D（`{"game_info":…}`）—— `Broadcast::parse` 在 T75 之后
        //    只认**顶层** 9 字段（字段读取不下钻）⇒ 这两条直接 `None`。T67 曾在这里补一道
        //    具名闸 `is_bare_object()`（"裸对象里没有 `{`"），T75 把它删了：正确修法在
        //    `protocol.rs`，而且那道形状闸会**误拒**房名里带 `{` 的合法广播（推断级缺陷）。
        // ② 变体 E（`"Timers1m"`）—— `TimeMode::from_str` 严格（T66）⇒ 也是 `None`。
        // 判据发生在**入表之前**，所以被丢的报文既不建行、也不刷新已有行的 `seen_at`。
        let Ok(text) = std::str::from_utf8(&raw) else {
            return false;
        };
        let Some(info) = Broadcast::parse(text.trim()) else {
            return false;
        };
        // `is_known_time_mode` 保留为**第二道形状闸**：它的档位是"推断"（判定发生在哪一层
        // 没有观测到），但行为与原版受控实验一致（`docs/06` §118.3 末段）。T66 之后它对
        // 已知枚举名恒真 —— 即恒不改变行为，只是把推断写在一处。
        if !is_known_time_mode(info.time_mode.as_str()) {
            return false;
        }
        let t = at(self.t0, now_ms);
        let ok = self.inner.ingest(addr, &raw, t);
        if ok {
            self.ingests += 1;
        }
        ok
    }

    /// TTL 过期清理（原版 `0x3de654`…`0x3de7a0`；判据 `secs >= 10`，`0x3de6e4 cmp x0,#0xa`）。
    ///
    /// 返回删掉的条数。
    #[func]
    fn expire_stale(&mut self, now_ms: i64) -> i64 {
        let t = at(self.t0, now_ms);
        let n = self.inner.expire_stale(t) as i64;
        self.expired_total += n;
        n
    }

    /// 这一帧该不该广播（原版时间闸 `elapsed > 1 s`，`0x3de7c8`–`0x3de7d4`）。
    #[func]
    fn due_to_broadcast(&mut self, now_ms: i64) -> bool {
        let t = at(self.t0, now_ms);
        self.inner.due_to_broadcast(t)
    }

    /// 当前房间表（**按发送方地址排序** —— 顺序只为可复现；原版 `HashMap` 迭代序未知）。
    #[func]
    fn rooms(&self, now_ms: i64) -> VarArray {
        let now = at(self.t0, now_ms);
        let t0 = self.t0;
        let mut keys: Vec<&SocketAddr> = self.inner.rooms.keys().collect();
        keys.sort_by_key(|a| a.to_string());
        let mut out = VarArray::new();
        for k in keys {
            let room = &self.inner.rooms[k];
            // seen_at → now_ms 口径的毫秒（两个 Instant 相减，不读墙钟）
            let seen_ms = room
                .seen_at
                .checked_duration_since(t0)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(&row_dict(k, &room.info, now_ms, seen_ms));
        }
        let _ = now;
        out
    }

    /// 房间表条数（原版 `+0x90`，`get_game_info` 的首条读点）。
    #[func]
    fn room_count(&self) -> i64 {
        self.inner.rooms.len() as i64
    }

    /// 按**发送方地址**查一条（原版 `get_game_info` `0x3ddce8`）。
    #[func]
    fn get_game_info(&self, from: GString, now_ms: i64) -> VarDictionary {
        let Some(addr) = parse_peer(&from.to_string()) else {
            return VarDictionary::new();
        };
        let t0 = self.t0;
        match self.inner.rooms.get(&addr) {
            Some(room) => {
                let seen_ms = room
                    .seen_at
                    .checked_duration_since(t0)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                row_dict(&addr, &room.info, now_ms, seen_ms)
            }
            None => VarDictionary::new(),
        }
    }

    /// 写本机房间信息（原版 `set_room_info` `0x3ddeb4`）。
    ///
    /// `address` 字段**由 Rust 生成**：`"[::1]:<port>"`（原版 `IpAddr::from_str("::1")`，
    /// 模块文档 §3.3）—— 调用方不许自己拼。
    #[func]
    fn set_room_info(
        &mut self,
        room_name: GString,
        game_state: GString,
        current_player_count: i64,
        axis_mode: GString,
        function_mode: GString,
        turn_mode: GString,
        time_mode: GString,
        locked: bool,
        port: i64,
    ) {
        // T66：`Broadcast` = 原版 `struct GameInfo`（5 个真枚举字段），GDScript 传进来的是**线上名字**
        // ⇒ 按变体名解析；**任一名字非法就整条不写**（与 serde「未知变体 = 反序列化失败」同口径，
        // 也正是 `docs/115` §5.3 变体 E 观测到的行为）。
        let (Some(game_state), Some(axis_mode), Some(function_mode), Some(turn_mode), Some(time_mode)) = (
            crate::turnstate::GameState::from_str(&game_state.to_string()),
            crate::graphwar_mode_manager::AxisMode::from_str(&axis_mode.to_string()),
            crate::graphwar_mode_manager::FunctionMode::from_str(&function_mode.to_string()),
            crate::graphwar_turn_manager::TurnMode::from_str(&turn_mode.to_string()),
            crate::graphwar_mode_manager::TimeMode::from_str(&time_mode.to_string()),
        ) else {
            return;
        };
        let info = Broadcast {
            room_name: room_name.to_string(),
            game_state,
            current_player_count: u32::try_from(current_player_count.max(0)).unwrap_or(0),
            // 稍后由 `set_room_info` 覆盖成 `[::1]:port`
            address: String::new(),
            axis_mode,
            function_mode,
            turn_mode,
            time_mode,
            locked,
        };
        let p = u16::try_from(port.max(0)).unwrap_or(0);
        self.inner.set_room_info(info, p);
    }

    /// 到点要发出去的那条 JSON（原版 `GameInfo::serialize` + `send_to`；**没有换行**）。
    ///
    /// 没写过本机房间 ⇒ 空串。
    #[func]
    fn broadcast_payload(&self) -> GString {
        match self.inner.broadcast_payload() {
            Some(s) => GString::from(s.as_str()),
            None => GString::from(""),
        }
    }

    /// `"[::1]:<port>"` —— 广播 `address` 字段的唯一形态（模块文档 §3.3）。
    #[func]
    fn local_address(port: i64) -> GString {
        let p = u16::try_from(port.max(0)).unwrap_or(0);
        GString::from(lan::local_broadcast_address(p).as_str())
    }

    /// `time_mode` 是不是已知形状（`Timer<正整数><s|m>`）。
    ///
    /// ⚠️ **推断**：见 [`is_known_time_mode`] 的文档（依据是 `docs/115` §5.3 变体 E
    /// "带 `Timers1m` 的报文被原版丢弃"）。
    #[func]
    fn time_mode_known(s: GString) -> bool {
        is_known_time_mode(s.to_string().as_str())
    }

    /// 解析一条广播原文（不动表；给自检/排障用）。不合格 ⇒ `{}`。
    ///
    /// 与 [`GwLanV6::ingest`] **同一套判据**：变体 B/D（事件外壳）⇒ 空（T75 起由
    /// `Broadcast::parse` 的「只认顶层 9 字段」直接拒）；`time_mode` 形状不对（变体 E）⇒ 空。
    /// 这样"能被解析"就等价于"能进房间表"。
    #[func]
    fn parse_broadcast(text: GString) -> VarDictionary {
        let t = text.to_string();
        let t = t.trim();
        // 与 `ingest` 同一套判据（T75 起全部在 `Broadcast::parse` 里）：
        // 变体 B/D 由"只认顶层 9 字段"拒，变体 E 由枚举严格解析拒。
        match Broadcast::parse(t) {
            Some(info) if is_known_time_mode(info.time_mode.as_str()) => {
                let mut d = VarDictionary::new();
                d.set("room_name", &GString::from(info.room_name.as_str()));
                d.set("game_state", &GString::from(info.game_state.as_str()));
                d.set("current_player_count", info.current_player_count);
                d.set("address", &GString::from(info.address.as_str()));
                d.set("axis_mode", &GString::from(info.axis_mode.as_str()));
                d.set("function_mode", &GString::from(info.function_mode.as_str()));
                d.set("turn_mode", &GString::from(info.turn_mode.as_str()));
                d.set("time_mode", &GString::from(info.time_mode.as_str()));
                d.set("locked", info.locked);
                d.set("time_mode_known", true);
                d
            }
            _ => VarDictionary::new(),
        }
    }

    /// 诊断计数：`ingests` / `expired_total`。
    #[func]
    fn counts(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("ingests", self.ingests);
        d.set("expired_total", self.expired_total);
        d.set("rooms", self.inner.rooms.len() as i64);
        d
    }

    /// 出处自检：每个值的 Rust 常量/函数 + 指令地址 + 档位。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("module", &GString::from("graphwar_lan_broadcaster_v6"));
        d.set("entry_ttl_secs", lan::ENTRY_TTL_SECS as i64);
        d.set(
            "entry_ttl_evidence",
            &GString::from(
                "arm64 0x3de6d4 bl Instant::now + 0x3de6e0 bl Sub for Instant + \
                 0x3de6e4 cmp x0,#0xa + 0x3de6e8 b.lo => secs >= 10 删",
            ),
        );
        d.set("broadcast_period_secs", lan::BROADCAST_PERIOD_SECS as i64);
        d.set(
            "broadcast_period_evidence",
            &GString::from(
                "arm64 0x3de7c8 cbz x0 / 0x3de7cc cmp x0,#1 / 0x3de7d0 b.ne / 0x3de7d4 cbz w1 \
                 => elapsed > 1 s",
            ),
        );
        d.set("recv_buf", lan::RECV_BUF as i64);
        d.set(
            "key_is_sender_addr",
            &GString::from("HashMap<SocketAddr,_> @+0x78；insert 0x3de638（不是报文 address 字段）"),
        );
        d.set(
            "group_addr",
            &GString::from(lan::GROUP_ADDR_V6),
        );
        d.set("local_addr", &GString::from(lan::LOCAL_ADDR));
        d.set("evidence_grade", &GString::from("逐位（TTL/周期/组播地址）+ 观测（表键、9 字段形状）"));
        d
    }
}
