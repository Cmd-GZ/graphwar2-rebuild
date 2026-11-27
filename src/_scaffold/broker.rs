//! Broker（房间列表服务器）：我们自己的那一台。
//!
//! # 为什么放在 `src/` 而不是 `tools/`
//!
//! 它不是在逆向时用一次的脚本，而是"能被 `cargo test` 直接测的协议实现"：
//! 它要复用 [`crate::_scaffold::protocol`] 里**已经被抓包钉死**的 `Frame` / `Broadcast` /
//! `VersionInfo`，并且要能用两个普通 TCP 连接做端到端断言。放 `src/` 才能进
//! `cargo test`，放 `tools/` 只能靠人工跑。
//!
//! # 协议（证据见 `docs/115-broker.md`）
//!
//! 线上是**和客户端↔游戏服务器同一套**的帧：`u16` 大端长度 + JSON。
//! 依据：broker 连接用的写入函数就是 `0x3d89e0`（里面 `cmp $0xffff` 限长、
//! `rol $0x8` 转大端），和对局连接是同一个函数。
//!
//! 一次真实会话（`fixtures/protocol_match.jsonl` 第 83–86 行，客户端侧）：
//!
//! ```text
//! -> {"VersionInfo":{"version":"2.0.60","system":"linux"}}
//! <- {"GameInfo":{"game_info":{"room_name":"US Room 2",…}}}
//! <- {"GameInfo":{"game_info":{"room_name":"US Room 1",…}}}
//! <- {"GameInfo":{"game_info":{"room_name":"US Room 3",…}}}
//! ```
//!
//! 即：连上先报版本，broker 把当前房间表**逐条用 `GameInfo` 推回来**。
//!
//! # 推断（未观测）
//!
//! * 房主**怎么**把房间登记到 broker：真会话只抓到"浏览方"，没抓到"房主方"。
//!   这里按"房主在自己那条 broker 连接上发 `GameInfo`"实现——除此之外没有任何
//!   别的通道能到达 broker（UDP 广播是另一套，见 `docs/24` §1）。
//! * 房间消失的通知 `GameRemoved{game_address}`：**从未抓到**，键名来自序列化
//!   代码（`game_address` 紧跟 `GameRemoved` 变体名），语义是"这个地址的房间没了"。
//! * 心跳/超时：`GameHeartbeat` 从未抓到。这里做成"连接在 = 房间在"，另加一个
//!   可配的空闲超时兜底。

use crate::_scaffold::protocol::{parse_frame_message, Broadcast, Frame, VersionInfo};

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 帮助文本里的示例端口：`--broker_address … (e.g. 127.0.0.1:42970)`（观测）。
pub const DEFAULT_PORT: u16 = 42970;

// ---------------------------------------------------------------------------
// 消息：房间表项
// ---------------------------------------------------------------------------

/// 一条 `GameInfo` 事件：`{"GameInfo":{"game_info":{…9 个字段…}}}`。
///
/// 外壳（变体名 `GameInfo` + 单字段 `game_info`）来自序列化代码
/// `0x50ff30`（写 `{`、`GameInfo`、`:`、`{`、`game_info`、`:`、结构体、`}`、`}`），
/// 结构体本身复用 [`Broadcast`]（9 字段、键序、无空格，已有真实报文对拍）。
pub fn game_info_event(info: &Broadcast) -> String {
    format!("{{\"GameInfo\":{{\"game_info\":{}}}}}", info.to_json())
}

/// 反向：从一条线上报文里取出房间表项。
///
/// **T76 收严**：帧用 [`parse_frame_message`]（单键 + 值必须是对象 + 尾部只许空白），
/// 外壳下钻交给 `GameInfo::parse` 自己（它按 `JsonObject::is_event_payload` 区分
/// "事件负载"与"裸广播原文"两种形态）。改前这里靠本文件的 `inner_object()`（又一次
/// `body.find("\"game_info\":")` 子串查找），现在整条链上**没有**第二处子串解析。
pub fn parse_game_info_event(text: &str) -> Option<Broadcast> {
    let raw = parse_frame_message(text)?;
    if raw.kind != "GameInfo" {
        return None;
    }
    Broadcast::parse(&raw)
}

/// `{"GameRemoved":{"game_address":"<host:port>"}}` —— 房间从列表里消失。
///
/// **推断**：从未抓到过真实报文；变体名与字段名来自序列化代码
/// （`0x511de0` 写 `{`、`GameRemoved`、`:`、`{`、`game_address`、`:`、值、`}`、`}`）。
pub fn game_removed_event(game_address: &str) -> String {
    format!(
        "{{\"GameRemoved\":{{\"game_address\":{}}}}}",
        crate::_scaffold::protocol::json_str(game_address)
    )
}

/// 反向：取出被移除的房间地址。
pub fn parse_game_removed_event(text: &str) -> Option<String> {
    let raw = parse_frame_message(text)?;
    if raw.kind != "GameRemoved" {
        return None;
    }
    crate::_scaffold::protocol::get_str(&raw, "game_address")
}

// ---------------------------------------------------------------------------
// 房间表
// ---------------------------------------------------------------------------

/// 房间表里的一项。
#[derive(Debug, Clone)]
pub struct Room {
    pub info: Broadcast,
    /// 哪条 broker 连接登记了它（连接断了就跟着消失）。
    pub owner: u64,
    pub last_seen: Instant,
}

/// 房间表。**键 = `GameInfo.address`**（和 `GameRemoved.game_address` 是同一个值）。
#[derive(Debug, Default)]
pub struct RoomTable {
    rooms: HashMap<String, Room>,
}

impl RoomTable {
    pub fn new() -> Self {
        RoomTable { rooms: HashMap::new() }
    }

    /// 登记或刷新一个房间。返回 `true` 表示这是一个新地址。
    pub fn upsert(&mut self, info: Broadcast, owner: u64) -> bool {
        let is_new = !self.rooms.contains_key(&info.address);
        self.rooms.insert(
            info.address.clone(),
            Room { info, owner, last_seen: Instant::now() },
        );
        is_new
    }

    /// 只刷新已存在房间的时间戳（`GameHeartbeat` / 重复上报用）。
    pub fn touch(&mut self, address: &str) -> bool {
        match self.rooms.get_mut(address) {
            Some(r) => {
                r.last_seen = Instant::now();
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, address: &str) -> Option<Room> {
        self.rooms.remove(address)
    }

    /// 掉线：把这条连接登记的房间全删掉，返回被删的地址。
    pub fn remove_owner(&mut self, owner: u64) -> Vec<String> {
        let gone: Vec<String> = self
            .rooms
            .values()
            .filter(|r| r.owner == owner)
            .map(|r| r.info.address.clone())
            .collect();
        for a in &gone {
            self.rooms.remove(a);
        }
        gone
    }

    /// 超过 `ttl` 没刷新的房间（返回被删的地址）。
    pub fn expire(&mut self, ttl: Duration) -> Vec<String> {
        let now = Instant::now();
        let gone: Vec<String> = self
            .rooms
            .values()
            .filter(|r| now.duration_since(r.last_seen) > ttl)
            .map(|r| r.info.address.clone())
            .collect();
        for a in &gone {
            self.rooms.remove(a);
        }
        gone
    }

    /// 当前房间表快照，按地址排序（**顺序未观测**，排序只为可复现）。
    pub fn snapshot(&self) -> Vec<Broadcast> {
        let mut v: Vec<Broadcast> = self.rooms.values().map(|r| r.info.clone()).collect();
        v.sort_by(|a, b| a.address.cmp(&b.address));
        v
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    pub fn get(&self, address: &str) -> Option<&Room> {
        self.rooms.get(address)
    }
}

// ---------------------------------------------------------------------------
// 服务器
// ---------------------------------------------------------------------------

/// broker 可调项。
#[derive(Debug, Clone)]
pub struct Config {
    /// 监听地址，例如 `127.0.0.1:42970`；`:0` 由系统分配。
    pub addr: String,
    /// 多久没更新就认为房间过期（**推断值**，真实 TTL 未知）。
    pub room_ttl: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: format!("127.0.0.1:{}", DEFAULT_PORT),
            room_ttl: Duration::from_secs(120),
        }
    }
}

struct State {
    table: RoomTable,
    /// 每条连接的写端（`try_clone` 出来的同一 socket）。
    peers: HashMap<u64, TcpStream>,
}

impl State {
    fn broadcast(&mut self, msg: &str, skip: Option<u64>) {
        let frame = Frame { payload: msg.to_string() }.encode();
        let mut dead = Vec::new();
        for (id, sock) in self.peers.iter_mut() {
            if Some(*id) == skip {
                continue;
            }
            if sock.write_all(&frame).is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            self.peers.remove(&id);
        }
    }

    fn send_to(&mut self, id: u64, msg: &str) {
        if let Some(sock) = self.peers.get_mut(&id) {
            let frame = Frame { payload: msg.to_string() }.encode();
            let _ = sock.write_all(&frame);
        }
    }
}

/// 跑起来的 broker。`addr` 是**实际**监听地址（`addr` 用 `:0` 时由系统分配）。
pub struct Broker {
    pub addr: SocketAddr,
    listener: Option<TcpListener>,
}

impl Broker {
    /// 绑定端口但不开始服务（测试用 `:0` 拿随机端口）。
    pub fn bind(cfg: &Config) -> std::io::Result<Broker> {
        let listener = TcpListener::bind(&cfg.addr)?;
        let addr = listener.local_addr()?;
        Ok(Broker { addr, listener: Some(listener) })
    }

    /// 在当前线程阻塞地把 broker 服务跑起来（只在 `listener` 里 accept）。
    pub fn serve(self, cfg: Config) -> std::io::Result<()> {
        self.serve_with(cfg, None)
    }

    /// 同上，但把成功绑定后的地址通知给调用方（`serve` 会阻塞，测不了端口）。
    pub fn serve_with(
        mut self,
        cfg: Config,
        ready: Option<std::sync::mpsc::Sender<SocketAddr>>,
    ) -> std::io::Result<()> {
        let listener = self.listener.take().expect("bind() 之后才能 serve()");
        if let Some(tx) = ready {
            let _ = tx.send(self.addr);
        }
        let state = Arc::new(Mutex::new(State {
            table: RoomTable::new(),
            peers: HashMap::new(),
        }));
        let next_id = Arc::new(AtomicU64::new(1));

        // 过期清扫线程（唯一一个；连接线程只做自己的事）。
        {
            let state = Arc::clone(&state);
            let ttl = cfg.room_ttl;
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                let gone = {
                    let mut st = state.lock().unwrap();
                    let gone = st.table.expire(ttl);
                    for a in &gone {
                        st.broadcast(&game_removed_event(a), None);
                    }
                    gone
                };
                if !gone.is_empty() {
                    eprintln!("BROKER expired {} room(s)", gone.len());
                }
            });
        }

        for conn in listener.incoming() {
            let sock = match conn {
                Ok(s) => s,
                Err(_) => continue,
            };
            let id = next_id.fetch_add(1, Ordering::SeqCst);
            let writer = match sock.try_clone() {
                Ok(w) => w,
                Err(_) => continue,
            };
            state.lock().unwrap().peers.insert(id, writer);
            let state = Arc::clone(&state);
            std::thread::spawn(move || {
                let _ = serve_conn(sock, id, state.clone());
                // 掉线：这条连接登记的房间全部消失，并通知其它人。
                let mut st = state.lock().unwrap();
                st.peers.remove(&id);
                let gone = st.table.remove_owner(id);
                for a in gone {
                    st.broadcast(&game_removed_event(&a), None);
                }
            });
        }
        Ok(())
    }
}

/// 在后台线程里起一台 broker，返回句柄。
pub fn spawn(cfg: Config) -> std::io::Result<Broker> {
    let broker = Broker::bind(&cfg)?;
    let addr = broker.addr;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = broker.serve_with(cfg, Some(tx));
    });
    // 等监听真正起来（serve_with 会先发地址再进 accept 循环）。
    let _ = rx.recv_timeout(Duration::from_secs(5));
    Ok(Broker { addr, listener: None })
}

/// 一条连接的生命周期。
fn serve_conn(mut sock: TcpStream, id: u64, state: Arc<Mutex<State>>) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut handshaked = false;
    let mut chunk = [0u8; 8192];
    loop {
        let n = sock.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        loop {
            let frame = match Frame::decode(&buf) {
                Some((f, used)) => {
                    buf.drain(..used);
                    f
                }
                None => break,
            };
            let msg = frame.payload;
            // 1) 首帧（真实会话里是 VersionInfo）：回整张房间表。
            if !handshaked {
                handshaked = true;
                if let Some(raw) = parse_frame_message(&msg) {
                    if raw.kind == "VersionInfo" {
                        if let Some(v) = VersionInfo::parse(&raw) {
                            eprintln!(
                                "BROKER conn {} version={} system={}",
                                id, v.version, v.system
                            );
                        }
                    }
                }
                let snap = {
                    let st = state.lock().unwrap();
                    st.table.snapshot()
                };
                let mut st = state.lock().unwrap();
                for info in snap {
                    st.send_to(id, &game_info_event(&info));
                }
                continue;
            }
            let raw = match parse_frame_message(&msg) {
                Some(r) => r,
                None => continue,
            };
            match raw.kind.as_str() {
                // 房主登记/刷新房间（**推断**：真会话只抓到浏览方）。
                "GameInfo" => {
                    if let Some(info) = parse_game_info_event(&msg) {
                        let addr = info.address.clone();
                        let mut st = state.lock().unwrap();
                        st.table.upsert(info.clone(), id);
                        let out = game_info_event(&info);
                        st.broadcast(&out, Some(id));
                        eprintln!("BROKER room {} ({})", addr, st.table.len());
                    }
                }
                "GameRemoved" => {
                    if let Some(addr) = parse_game_removed_event(&msg) {
                        let mut st = state.lock().unwrap();
                        st.table.remove(&addr);
                        st.broadcast(&game_removed_event(&addr), Some(id));
                    }
                }
                // 从未抓到过；收到就当作"这个房间还活着"。
                "GameHeartbeat" => {
                    let mut st = state.lock().unwrap();
                    let addrs: Vec<String> =
                        st.table.snapshot().iter().map(|i| i.address.clone()).collect();
                    for a in addrs {
                        if st.table.get(&a).map(|r| r.owner) == Some(id) {
                            st.table.touch(&a);
                        }
                    }
                }
                // 客户端显式订阅：把当前表再推一次（键名来自序列化代码，方向未观测）。
                "Subscribe" => {
                    let snap = {
                        let st = state.lock().unwrap();
                        st.table.snapshot()
                    };
                    let mut st = state.lock().unwrap();
                    for info in snap {
                        st.send_to(id, &game_info_event(&info));
                    }
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // T73 真值表：`fixtures/test_expectation_truth4.tsv`
    // （生成器 `tools/gen_test_expectations4.py`，`--check` 可复算）
    //
    // `F/*` = 线上真 wire 字节：`fixtures/protocol_live_frames.jsonl` 的 1717 帧里
    //         **1715 帧**逐帧核过 `sha256`(负载) / `frame_sha256`(u16 前缀+负载) / `n`。
    // `L/*` = 线上真帧的字段数与键序（含 `GameInfo` 52 帧）。
    // ===================================================================
    const TRUTH4: &str = include_str!("../../fixtures/test_expectation_truth4.tsv");

    fn truth4(section: &str, key: &str) -> &'static str {
        for line in TRUTH4.lines() {
            let mut it = line.split('\t');
            if it.next() == Some(section) && it.next() == Some(key) {
                return it.next().unwrap_or("");
            }
        }
        panic!("truth4 里没有 {section}/{key}")
    }

    fn truth4_usize(section: &str, key: &str) -> usize {
        truth4(section, key).parse().unwrap_or_else(|_| panic!("{section}/{key} 不是整数"))
    }

    fn truth4_u64(section: &str, key: &str) -> u64 {
        u64::from_str_radix(truth4(section, key).trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("{section}/{key} 不是 hex"))
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// 抓包里的每一帧：`(事件名, n, 负载字节, 是否打码)`，按抓包顺序；meta 行跳过。
    fn live_frames() -> Vec<(String, usize, Vec<u8>, bool)> {
        const JSONL: &str = include_str!("../../fixtures/protocol_live_frames.jsonl");
        let mut out = Vec::new();
        for line in JSONL.lines() {
            let Some(epos) = line.find("\"event\": \"") else { continue };
            let rest = &line[epos + 10..];
            let Some(eend) = rest.find('"') else { continue };
            let event = &rest[..eend];
            if event == "null" || event.is_empty() {
                continue;
            }
            let Some(hpos) = line.find("\"hex\": \"") else { continue };
            let hrest = &line[hpos + 8..];
            let Some(hend) = hrest.find('"') else { continue };
            let hexs = &hrest[..hend];
            let npos = line.find("\"n\": ").expect("每帧都有 n");
            let nrest = &line[npos + 5..];
            let nend = nrest.find(|c: char| !c.is_ascii_digit()).unwrap_or(nrest.len());
            let n: usize = nrest[..nend].parse().expect("n 是整数");
            let masked = line.contains("\"hex_masked\": true");
            let bytes: Vec<u8> = (0..hexs.len() / 2)
                .map(|i| u8::from_str_radix(&hexs[i * 2..i * 2 + 2], 16).expect("抓包 hex"))
                .collect();
            out.push((event.to_string(), n, bytes, masked));
        }
        out
    }

    /// 抓包里某事件的**全部线上负载字节**（按抓包顺序）。
    fn live_payloads(kind: &str) -> Vec<Vec<u8>> {
        live_frames()
            .into_iter()
            .filter(|(e, _, _, _)| e == kind)
            .map(|(_, _, p, _)| p)
            .collect()
    }

    fn sample(addr: &str, players: u32) -> Broadcast {
        Broadcast {
            room_name: "Graphwar Game".into(),
            game_state: crate::turnstate::GameState::Setup,
            current_player_count: players,
            address: addr.into(),
            axis_mode: crate::graphwar_mode_manager::AxisMode::EveryUnit,
            function_mode: crate::graphwar_mode_manager::FunctionMode::NormalFunction,
            turn_mode: crate::graphwar_turn_manager::TurnMode::SequentialTurns,
            time_mode: crate::graphwar_mode_manager::TimeMode::Timer1m,
            locked: false,
        }
    }

    // ===================================================================
    // T59 真值：**真实抓包** `fixtures/protocol_match.jsonl`（sha256
    // `97d1a0b97c3539139ccdc284ff98b171325c8f1bbb30b27ecc77764c95feba35`，86 帧）。
    // 复算：`.reference/venv/bin/python tools/gen_test_expectations2.py` → truth2 表 `B/*` 段。
    // 这些期望值来自**真机上抓到的字节**，不是我方序列化器的往返（唯一例外是 B05：仓库里只有
    // 解析后的文本、没有原始帧字节 ⇒ 那一条如实降为推断）。
    // ===================================================================

    /// 真抓包里的房间（广播顺序）：`US Room 2` / `US Room 1` / `US Room 3`，
    /// 地址 `69.164.213.144:35981 / :37933 / :38347`，版本 `2.0.60` / `linux`。
    const REAL_ROOMS: [(&str, &str); 3] = [
        ("US Room 2", "69.164.213.144:35981"),
        ("US Room 1", "69.164.213.144:37933"),
        ("US Room 3", "69.164.213.144:38347"),
    ];
    const REAL_VERSION: (&str, &str) = ("2.0.60", "linux");
    /// 抓包会话前 6 帧的事件名（顺序即真值）。
    const REAL_SESSION_HEAD: [&str; 6] = [
        "GameNameRequest",
        "NewConnectionRequest",
        "NewPlayerRequest",
        "TickRequest",
        "NameRequest",
        "NameRequest",
    ];

    /// `fixtures/protocol_match.jsonl` 里的真实 broker 报文（第 83–86 行）。
    fn broker_fixture() -> Vec<String> {
        include_str!("../../fixtures/protocol_match.jsonl")
            .lines()
            .filter(|l| l.contains("\"GameInfo\"") || l.contains("\"VersionInfo\""))
            .map(|l| l.trim().to_string())
            .collect()
    }

    #[test]
    fn fixture_has_the_captured_broker_session() {
        let f = broker_fixture();
        assert_eq!(f.len(), 4, "抓包里 broker 段共 4 条（1 条版本 + 3 条房间）");
        assert!(f[0].starts_with("{\"VersionInfo\":"));
        for line in &f[1..] {
            assert!(line.starts_with("{\"GameInfo\":{\"game_info\":{\"room_name\":"));
        }
        // T59 真值：真实抓包里那三个房间（顺序即真值）、版本串
        let mut got: Vec<(String, String)> = Vec::new();
        for line in f.iter().filter(|l| l.contains("GameInfo")) {
            let info = parse_game_info_event(line).expect("真报文可解析");
            got.push((info.room_name.clone(), info.address.clone()));
        }
        // 借用 REAL_ROOMS 的 'static 串做逐项比对
        for (k, (room, addr)) in REAL_ROOMS.iter().enumerate() {
            assert_eq!(got[k].0, *room, "第 {k} 个房间名");
            assert_eq!(got[k].1, *addr, "第 {k} 个房间地址");
        }
    }

    /// 逐位：抓到的 `GameInfo` 报文 → 解析 → 重新序列化，字节相等。
    #[test]
    fn game_info_event_round_trips_byte_exactly() {
        for line in broker_fixture().iter().filter(|l| l.contains("GameInfo")) {
            let info = parse_game_info_event(line).expect("真实报文应能解析");
            assert_eq!(&game_info_event(&info), line, "重新序列化必须逐字节等于抓包");
        }
    }

    #[test]
    fn version_info_fixture_parses() {
        let line = broker_fixture()
            .into_iter()
            .find(|l| l.contains("VersionInfo"))
            .unwrap();
        let raw = parse_frame_message(&line).unwrap();
        assert_eq!(raw.kind, "VersionInfo");
        let v = VersionInfo::parse(&raw).unwrap();
        assert_eq!(v.version, "2.0.60");
        assert_eq!(v.system, "linux");
        assert_eq!(v.to_msg(), line);
        // T59 真值：版本/系统串来自真抓包（不是我方常量）
        assert_eq!((v.version.as_str(), v.system.as_str()), REAL_VERSION);
        // 真抓包会话的前 6 帧顺序（真值）
        let kinds: Vec<&str> = include_str!("../../fixtures/protocol_match.jsonl")
            .lines()
            .filter(|l| l.starts_with('{'))
            .take(6)
            .map(|l| {
                let k = l.trim_start_matches('{').trim_start_matches('"');
                &k[..k.find('"').unwrap()]
            })
            .collect();
        assert_eq!(kinds.as_slice(), REAL_SESSION_HEAD.as_slice(), "真抓包会话前 6 帧");
    }

    /// 外壳形状（`{"GameInfo":{"game_info":…}}`）+ 键序。
    ///
    /// ## T73 / 计划 B02：键序期望值再加一道**抓包真帧**的核对
    ///
    /// 原先的期望是下面这张 hardcode 键名表（人工从真帧抄的）。本轮另加一条：我方
    /// `Broadcast::to_json()` 的键序必须等于 `truth4` 的 `L/GameInfo/inner_keyorder`
    /// —— 那是 `tools/gen_test_expectations4.py` 从 **52 帧线上 `GameInfo` 的负载字节**
    /// 解出来的键序（不是我方抄的表）。
    #[test]
    fn game_info_event_shape_matches_the_serializer() {
        // 序列化代码 0x50ff30：'{' "GameInfo" ':' '{' "game_info" ':' <struct> '}' '}'
        let s = game_info_event(&sample("[::1]:35245", 2));
        assert!(s.starts_with("{\"GameInfo\":{\"game_info\":{"));
        assert!(s.ends_with("}}"));
        let info = parse_game_info_event(&s).unwrap();
        assert_eq!(info, sample("[::1]:35245", 2));
        // T59 真值：真抓包那一帧的字段**顺序**（我方序列化器必须与真帧同序，字段集合也一样）
        let real = broker_fixture()
            .into_iter()
            .find(|l| l.contains("GameInfo"))
            .unwrap();
        let keys_real: Vec<&str> = [
            "\"room_name\"", "\"game_state\"", "\"current_player_count\"", "\"address\"",
            "\"axis_mode\"", "\"function_mode\"", "\"turn_mode\"", "\"time_mode\"", "\"locked\"",
        ]
        .to_vec();
        let mut last = 0usize;
        for k in &keys_real {
            let at = real.find(k).unwrap_or_else(|| panic!("真帧里没有 {k}"));
            assert!(at >= last, "真帧字段顺序被改了：{k}");
            last = at;
            assert!(s.contains(k), "我方序列化缺字段 {k}");
        }

        // T73：与**线上 52 帧真帧**的键序对齐（真值表 `L/GameInfo/inner_keyorder`）
        let live_keys: Vec<String> = truth4("L", "GameInfo/inner_keyorder")
            .split('|')
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect();
        let own_keys: Vec<String> = crate::_scaffold::protocol::split_fields(
            sample("[::1]:35245", 2).to_json().trim_start_matches('{').trim_end_matches('}'),
        )
        .into_iter()
        .map(|(k, _)| k)
        .collect();
        assert_eq!(own_keys, live_keys, "我方 GameInfo 键序与线上真帧不一致");
        assert_eq!(truth4_usize("L", "GameInfo/frames"), 52, "线上 GameInfo 帧数变了");
    }

    /// `GameRemoved` 往返。
    ///
    /// ## 真值来源（T73）—— **T59 的「零帧」前提被证伪**
    ///
    /// T59 记：「`GameRemoved` 在 403 份语料 + 现有抓包里**零帧** ⇒ 只能是契约断言」。
    /// T53 的线上抓包里**有 1 帧** `GameRemoved`（broker 广播，`69.164.213.144:42409`）⇒
    /// 这条不再是纯契约：期望值取**抓到的字节**，我方解析后重新序列化必须逐字节相同。
    #[test]
    fn game_removed_round_trips() {
        let payloads = live_payloads("GameRemoved");
        assert_eq!(payloads.len(), truth4_usize("L", "GameRemoved/frames"),
                   "线上 GameRemoved 帧数与真值表不符");
        assert_eq!(payloads.len(), 1, "T53 抓包里 GameRemoved 只有 1 帧");
        for payload in &payloads {
            let line = std::str::from_utf8(payload).expect("UTF-8");
            let addr = parse_game_removed_event(line).expect("线上 GameRemoved 应能解析");
            assert_eq!(game_removed_event(&addr), line,
                       "线上 GameRemoved 帧回放后字节不一致");
        }
        // 我方构造的方向（契约）：地址是 host:port 字符串
        let s = game_removed_event("1.2.3.4:35981");
        assert_eq!(s, "{\"GameRemoved\":{\"game_address\":\"1.2.3.4:35981\"}}");
        assert_eq!(parse_game_removed_event(&s).as_deref(), Some("1.2.3.4:35981"));
        // 别的消息不能被当成移除
        assert_eq!(parse_game_removed_event("{\"GameInfo\":{}}"), None);
    }

    /// 帧格式 = `u16 大端长度前缀 + UTF-8 负载`。
    ///
    /// ## 真值来源（T73 / 计划 B01）—— **T59 的「换不了」前提被证伪**
    ///
    /// T59 记：「仓库里的抓包是解析后文本、没有原始 socket 字节 ⇒ 要升档需要一份原始字节抓包」。
    /// **这份抓包已经有了**：T53 的 `fixtures/protocol_live_frames.jsonl` 里 1717 帧中
    /// **1715 帧**自带 `frame_sha256 = sha256(u16be(n) ‖ payload)` 与 `sha256 = sha256(payload)`
    /// （`tools/gen_test_expectations4.py::verify_live` 逐帧核过）。
    ///
    /// 本测试用**我方 `Frame::encode`** 重建 1715 个完整帧，拼接后 FNV-1a 64 必须等于抓包
    /// 字节的 FNV（`truth4` 的 `F/wire_fnv`，`F/wire_len` 字节）⇒ 前缀口径（大端 u16、
    /// 等于负载字节数、不含前缀自身）与**真 wire 字节逐字节一致**。
    ///
    /// 复跑：`.reference/venv/bin/python tools/gen_test_expectations4.py --check`
    #[test]
    fn frame_is_u16_big_endian_prefixed() {
        let frames = live_frames();
        assert_eq!(frames.len(), truth4_usize("F", "frames_total"));
        assert_eq!(frames.iter().filter(|f| f.3).count(), truth4_usize("F", "frames_masked"));
        let mut wire: Vec<u8> = Vec::new();
        for (event, n, payload, masked) in &frames {
            if *masked {
                continue; // 打码帧的 hex 是打码后文本，长度 ≠ n ⇒ 不进 wire 聚合
            }
            assert_eq!(payload.len(), *n, "{event}: n 与负载字节数不符");
            let f = Frame { payload: String::from_utf8(payload.clone()).expect("UTF-8") };
            let w = f.encode();
            assert_eq!(&w[..2], &(*n as u16).to_be_bytes(),
                       "{event}: 长度前缀必须是大端 u16 且等于负载字节数");
            let (back, used) = Frame::decode(&w).expect("整帧应能解码");
            assert_eq!(back, f, "{event}: 解码回来的帧变了");
            assert_eq!(used, w.len());
            wire.extend_from_slice(&w);
        }
        assert_eq!(wire.len(), truth4_usize("F", "wire_len"),
                   "1715 帧完整 wire 字节数应按抓包算出来");
        assert_eq!(fnv1a64(&wire), truth4_u64("F", "wire_fnv"),
                   "我方 encode 出的 wire 字节与抓包不一致（前缀口径错了）");

        // 解码边界：不够一帧就必须等（契约，不是真值 —— 真流量里没有半帧）
        let f = Frame { payload: "{\"TickReply\":{\"current_tick\":5}}".to_string() };
        let w = f.encode();
        assert!(Frame::decode(&w[..1]).is_none());
        assert!(Frame::decode(&w[..w.len() - 1]).is_none());
    }

    /// **线上 `GameInfo` 全量回放**：52 帧逐帧「解析 → 重新序列化」必须与抓包**逐字节**相同。
    ///
    /// ## 真值来源（T73 / 计划 B02）
    ///
    /// T59 用的是 `fixtures/protocol_match.jsonl` 里 **3 条**房间（而且是解析后文本、无原始字节）。
    /// 本轮用 T53 的线上抓包：**52 帧** `GameInfo`（官服 broker 大堂广播，覆盖
    /// `locked` / `current_player_count` / 各 mode 的真实组合），负载字节就是期望值。
    /// 键序另外钉在 `truth4` 的 `L/GameInfo/inner_keyorder`（从抓包字节解出来的 9 个键）。
    #[test]
    fn replays_every_live_game_info_frame() {
        let payloads = live_payloads("GameInfo");
        assert_eq!(payloads.len(), truth4_usize("L", "GameInfo/frames"),
                   "线上 GameInfo 帧数与真值表不符（夹具被换过）");
        let joined: Vec<u8> = payloads.iter().flatten().copied().collect();
        assert_eq!(fnv1a64(&joined), truth4_u64("L", "GameInfo/payload_fnv"),
                   "线上 GameInfo 负载指纹变了");
        let truth_order: Vec<&str> =
            truth4("L", "GameInfo/inner_keyorder").split('|').collect();
        for payload in &payloads {
            let line = std::str::from_utf8(payload).expect("线上负载应是 UTF-8");
            let info = parse_game_info_event(line)
                .unwrap_or_else(|| panic!("线上 GameInfo 应能解析：{line}"));
            assert_eq!(game_info_event(&info), line,
                       "线上 GameInfo 帧回放后字节不一致：{line}");
            // 内层键序（我方序列化器必须与抓包同序）：期望值来自抓包字节（truth4）
            let own_keys: Vec<String> =
                crate::_scaffold::protocol::split_fields(
                    info.to_json().trim_start_matches('{').trim_end_matches('}'),
                )
                .into_iter()
                .map(|(k, _)| k)
                .collect();
            assert_eq!(own_keys, truth_order,
                       "我方 GameInfo 键序与抓包真帧不一致：{line}");
        }
    }

    /// 房间表语义：同地址是更新、`remove_owner` 清掉整条连接、`expire` 按 TTL 过期。
    ///
    /// ⚠️ **契约断言（我方自造服务端的语义），不含复原证据**（T73 / 计划 B04）：
    /// broker **服务端**在原版里没有对应物（`AGENTS.md` §4 把 broker 服务端归 `src/_scaffold/`），
    /// 客户端只有「连 broker → 收 `GameInfo`/`GameRemoved`」。所以这些表语义只能是我方选择。
    ///
    /// 真值侧的**旁证**（不是证据）：线上抓包 52 帧 `GameInfo` **每帧恰好一个房间**
    /// （`game_info` 是单个对象，不是数组 ⇒ 原版协议里没有「房间列表」这个容器），
    /// 房间消失走 `GameRemoved`（1 帧）。复算见 `fixtures/test_expectation_truth4.tsv`
    /// 的 `L/GameInfo/*`。
    #[test]
    fn room_table_upsert_remove_expire() {
        let mut t = RoomTable::new();
        assert!(t.upsert(sample("a:1", 0), 7));
        assert!(!t.upsert(sample("a:1", 2), 7), "同一地址是更新不是新房间");
        assert_eq!(t.len(), 1);
        assert_eq!(t.snapshot()[0].current_player_count, 2, "更新覆盖旧值");

        assert!(t.upsert(sample("b:2", 1), 8));
        assert_eq!(t.remove_owner(7), vec!["a:1".to_string()]);
        assert_eq!(t.len(), 1);

        assert!(t.touch("b:2"));
        assert!(!t.touch("nope:0"));
        assert_eq!(t.expire(Duration::from_secs(3600)), Vec::<String>::new());
        assert_eq!(t.remove("b:2").unwrap().info.address, "b:2");
        assert!(t.is_empty());
    }

    /// `snapshot()` 按地址排序（确定性）。
    ///
    /// ⚠️ **契约断言**（T73 / 计划 B04）：排序是我方服务端的**自造**确定性选择，不是复原证据。
    /// 原版 broker 的广播顺序**无从观测**：线上 52 帧 `GameInfo` 每帧只带一个房间
    /// （没有列表容器），而旧抓包 `fixtures/protocol_match.jsonl` 里三个房间的顺序是
    /// `US Room 2 → US Room 1 → US Room 3`（既非名字序也非地址序）⇒ 原版对房间列表
    /// **没有可观测的不变量**。真帧见 [`REAL_ROOMS`]。
    #[test]
    fn snapshot_is_sorted_and_stable() {
        let mut t = RoomTable::new();
        t.upsert(sample("z:9", 0), 1);
        t.upsert(sample("a:1", 0), 1);
        let s = t.snapshot();
        assert_eq!(s.iter().map(|i| i.address.as_str()).collect::<Vec<_>>(), vec!["a:1", "z:9"]);
        // T59：**换不了**（如实登记）。真抓包里 broker 广播房间的顺序是
        // US Room 2 → US Room 1 → US Room 3（既不是按名字也不是按地址排序）⇒ 原版对房间列表
        // 的顺序没有可观测的不变量；本表"按地址排序"是我方**自造服务端**的确定性选择，
        // 不是复原证据。真值见 `REAL_ROOMS`。
        assert_eq!(REAL_ROOMS[0].0, "US Room 2");
        assert_eq!(REAL_ROOMS[1].0, "US Room 1");
        assert_eq!(REAL_ROOMS[2].0, "US Room 3");
    }
}
