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

use crate::_scaffold::protocol::{parse_message, Broadcast, Frame, VersionInfo};

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
pub fn parse_game_info_event(text: &str) -> Option<Broadcast> {
    let raw = parse_message(text)?;
    if raw.kind != "GameInfo" {
        return None;
    }
    let inner = inner_object(&raw.body, "game_info")?;
    Broadcast::parse(inner)
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
    let raw = parse_message(text)?;
    if raw.kind != "GameRemoved" {
        return None;
    }
    crate::_scaffold::protocol::get_str(&raw, "game_address")
}

/// 在 `{"game_info":{…}}` 里取出内层对象（带花括号配平，非对象返回 `None`）。
fn inner_object<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\":", key);
    let start = body.find(&pat)? + pat.len();
    let rest = body[start..].trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, ch) in rest.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..=i]);
                }
            }
            _ => {}
        }
    }
    None
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
                if let Some(raw) = parse_message(&msg) {
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
            let raw = match parse_message(&msg) {
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

    fn sample(addr: &str, players: i64) -> Broadcast {
        Broadcast {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            current_player_count: players,
            address: addr.into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
            locked: false,
        }
    }

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
        let raw = parse_message(&line).unwrap();
        assert_eq!(raw.kind, "VersionInfo");
        let v = VersionInfo::parse(&raw).unwrap();
        assert_eq!(v.version, "2.0.60");
        assert_eq!(v.system, "linux");
        assert_eq!(v.to_msg(), line);
    }

    #[test]
    fn game_info_event_shape_matches_the_serializer() {
        // 序列化代码 0x50ff30：'{' "GameInfo" ':' '{' "game_info" ':' <struct> '}' '}'
        let s = game_info_event(&sample("[::1]:35245", 2));
        assert!(s.starts_with("{\"GameInfo\":{\"game_info\":{"));
        assert!(s.ends_with("}}"));
        let info = parse_game_info_event(&s).unwrap();
        assert_eq!(info, sample("[::1]:35245", 2));
    }

    #[test]
    fn game_removed_round_trips() {
        let s = game_removed_event("1.2.3.4:35981");
        assert_eq!(s, "{\"GameRemoved\":{\"game_address\":\"1.2.3.4:35981\"}}");
        assert_eq!(parse_game_removed_event(&s).as_deref(), Some("1.2.3.4:35981"));
        // 别的消息不能被当成移除
        assert_eq!(parse_game_removed_event("{\"GameInfo\":{}}"), None);
    }

    #[test]
    fn frame_is_u16_big_endian_prefixed() {
        // 0x0031 = 49 的那一帧是 docs/24 §1 的实测样本
        let msg = "{\"TickReply\":{\"current_tick\":5}}";
        let f = Frame { payload: msg.into() }.encode();
        assert_eq!(&f[..2], &(msg.len() as u16).to_be_bytes());
        assert_eq!(&f[2..], msg.as_bytes());
        let (back, used) = Frame::decode(&f).unwrap();
        assert_eq!(back.payload, msg);
        assert_eq!(used, f.len());
        // 太少的数据不算一帧
        assert!(Frame::decode(&f[..1]).is_none());
        assert!(Frame::decode(&f[..f.len() - 1]).is_none());
    }

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

    #[test]
    fn snapshot_is_sorted_and_stable() {
        let mut t = RoomTable::new();
        t.upsert(sample("z:9", 0), 1);
        t.upsert(sample("a:1", 0), 1);
        let s = t.snapshot();
        assert_eq!(s.iter().map(|i| i.address.as_str()).collect::<Vec<_>>(), vec!["a:1", "z:9"]);
    }
}
