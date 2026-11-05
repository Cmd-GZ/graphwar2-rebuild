//! 网络协议：帧格式与消息模型。
//!
//! 来源：在真实对局进程里 hook libc 的 socket 收发，抓到的**真实报文**
//! （`tools/capture_net.js`）。所有样本原样存在 `fixtures/protocol.jsonl`，
//! 单元测试直接吃这些真实帧。
//!
//! ## 传输层（观测）
//!
//! * **发现（LAN 广播）**：UDP，明文 JSON，无长度前缀。发往广播端口，
//!   内容形如
//!   `{"room_name":"Graphwar Game","game_state":"Setup","current_player_count":2,
//!     "address":"[::1]:35245","axis_mode":"EveryUnit","function_mode":"NormalFunction",
//!     "turn_mode":"SequentialTurns","time_mode":"Timers1m","locked":false}`
//! * **对局（TCP）**：每帧 = `u16 大端长度` + JSON 负载。长度**只算负载**，
//!   不含长度字段自身（实测 `0x0031` = 49 对应 49 字节 JSON）。
//!
//! ## 消息（观测到的 29 种）
//!
//! 全部是单键对象 `{"TypeName":{...字段...}}`，字段要么是字符串、要么是整数、
//! 要么是嵌套的小对象（如 `color` 的 `{r,g,b}`）。没有数组、没有转义字符串的实际样本。

/// 一帧：长度前缀 + JSON 负载。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub payload: String,
}

impl Frame {
    /// 编码成线上字节：`u16 大端长度` + UTF-8 负载。
    pub fn encode(&self) -> Vec<u8> {
        let body = self.payload.as_bytes();
        let mut out = Vec::with_capacity(body.len() + 2);
        out.extend_from_slice(&(body.len() as u16).to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// 从缓冲区头部解出一帧，返回 `(帧, 消耗字节数)`。
    ///
    /// 长度不足时返回 `None`（调用方应继续等数据）。
    pub fn decode(buf: &[u8]) -> Option<(Frame, usize)> {
        if buf.len() < 2 {
            return None;
        }
        let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
        if buf.len() < 2 + len {
            return None;
        }
        let payload = std::str::from_utf8(&buf[2..2 + len]).ok()?.to_string();
        Some((Frame { payload }, 2 + len))
    }
}

/// 解析结果：消息类型名 + 原始负载。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMessage {
    /// 例如 `NewConnectionRequest`
    pub kind: String,
    /// 负载里该类型对应的那一段原文（`{...}` 的内部），便于按需取字段。
    pub body: String,
    /// 整帧 JSON 原文。
    pub text: String,
}

/// 解出消息类型名与内层对象。
pub fn parse_message(text: &str) -> Option<RawMessage> {
    let text = text.trim();
    // 外层是 {"类型": {...}}：花括号必须配平（字符串内的不算）
    if !braces_balanced(text) {
        return None;
    }
    let inner = text.strip_prefix('{')?;
    let (key, rest) = read_key(inner)?;
    let body = read_object(&rest)?;
    Some(RawMessage { kind: key, body, text: text.to_string() })
}

/// LAN 广播的房间信息（**不是**单键消息，独立格式）。
#[derive(Debug, Clone, PartialEq)]
pub struct Broadcast {
    pub room_name: String,
    pub game_state: String,
    pub current_player_count: i64,
    pub address: String,
    pub axis_mode: String,
    pub function_mode: String,
    pub turn_mode: String,
    pub time_mode: String,
    pub locked: bool,
}

impl Broadcast {
    /// 广播是 `{"room_name":..,"game_state":..,...}`，直接当普通对象取字段。
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if !braces_balanced(text) {
            return None;
        }
        let body = read_object(text)?;
        let m = RawMessage { kind: "Broadcast".into(), body, text: text.to_string() };
        Some(Broadcast {
            room_name: get_str(&m, "room_name")?,
            game_state: get_str(&m, "game_state")?,
            current_player_count: get_int(&m, "current_player_count")?,
            address: get_str(&m, "address")?,
            axis_mode: get_str(&m, "axis_mode")?,
            function_mode: get_str(&m, "function_mode")?,
            turn_mode: get_str(&m, "turn_mode")?,
            time_mode: get_str(&m, "time_mode")?,
            locked: get_bool(&m, "locked")?,
        })
    }
}

/// 从消息体里取一个字符串字段，例如 `get_str(m, "game_name")`。
pub fn get_str(msg: &RawMessage, key: &str) -> Option<String> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 从消息体里取一个整数字段。
pub fn get_int(msg: &RawMessage, key: &str) -> Option<i64> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// 取 `{"r":149,"g":5,"b":62}` 这样的颜色字段。
pub fn get_color(msg: &RawMessage, key: &str) -> Option<(u8, u8, u8)> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    let obj = read_object(rest)?;
    let sub = RawMessage { kind: key.to_string(), body: obj, text: String::new() };
    Some((
        get_int(&sub, "r")? as u8,
        get_int(&sub, "g")? as u8,
        get_int(&sub, "b")? as u8,
    ))
}

// ---------------------------------------------------------------------------
// 最小 JSON 读取（只覆盖本协议实际用到的形状）
// ---------------------------------------------------------------------------

/// 花括号是否配平（跳过字符串字面量）。不配平的输入视为不完整帧。
fn braces_balanced(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
        i += 1;
    }
    depth == 0 && !in_str
}

/// 读开头的 `"key"` 并返回 (key, 冒号之后的剩余部分)。
fn read_key(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find('"')?;
    let key = rest[..end].to_string();
    let after = rest[end + 1..].trim_start();
    let after = after.strip_prefix(':')?;
    Some((key, after.trim_start()))
}

/// 从 `{...}` 里取出内部文本（按大括号配对，跳过字符串内的括号）。
fn read_object(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('{') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(s[1..i].to_string());
            }
        }
        i += 1;
    }
    None
}


impl Broadcast {
    /// 按抓包观察到的**字段顺序**与格式重新序列化。
    ///
    /// 依据：抓到的广播报文形如
    /// `{"room_name":"…","game_state":"…","current_player_count":N,"address":"…",
    ///   "axis_mode":"…","function_mode":"…","turn_mode":"…","time_mode":"…","locked":false}`
    /// —— 键序固定、无空格、布尔小写。
    pub fn to_json(&self) -> String {
        format!(
            "{{\"room_name\":\"{}\",\"game_state\":\"{}\",\"current_player_count\":{},\"address\":\"{}\",\"axis_mode\":\"{}\",\"function_mode\":\"{}\",\"turn_mode\":\"{}\",\"time_mode\":\"{}\",\"locked\":{}}}",
            self.room_name,
            self.game_state,
            self.current_player_count,
            self.address,
            self.axis_mode,
            self.function_mode,
            self.turn_mode,
            self.time_mode,
            if self.locked { "true" } else { "false" },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 夹具每行就是一条**原始报文**（抓包时去掉了 2 字节长度前缀）。
    fn fixtures() -> Vec<RawMessage> {
        include_str!("../fixtures/protocol.jsonl")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| parse_message(l).expect("夹具里的帧都应能解析"))
            .collect()
    }

    #[test]
    fn parses_every_real_captured_frame() {
        let msgs = fixtures();
        assert_eq!(msgs.len(), 29, "真实报文夹具条数不符");
        assert_eq!(msgs[0].kind, "GameNameRequest");
        assert_eq!(get_str(&msgs[0], "game_name").as_deref(), Some("Graphwar Game"));
    }

    #[test]
    fn frame_length_prefix_matches_payload_length() {
        let f = Frame { payload: "{\"TickRequest\":{}}".into() };
        let wire = f.encode();
        assert_eq!(wire.len(), f.payload.len() + 2);
        assert_eq!(&wire[..2], &(f.payload.len() as u16).to_be_bytes());
        let (back, used) = Frame::decode(&wire).unwrap();
        assert_eq!(back, f);
        assert_eq!(used, wire.len());
    }

    #[test]
    fn decode_waits_for_complete_frame() {
        let f = Frame { payload: "{\"TickRequest\":{}}".into() };
        let wire = f.encode();
        assert!(Frame::decode(&wire[..1]).is_none(), "半个长度字段不应解析");
        assert!(Frame::decode(&wire[..wire.len() - 1]).is_none(), "负载不完整不应解析");
    }

    #[test]
    fn extracts_ints_and_colors() {
        let m = parse_message(r#"{"NewConnection":{"connection_id":228}}"#).unwrap();
        assert_eq!(get_int(&m, "connection_id"), Some(228));
        let c = parse_message(r#"{"ColorInfo":{"entity_id":229,"color":{"r":149,"g":5,"b":62}}}"#).unwrap();
        assert_eq!(get_color(&c, "color"), Some((149, 5, 62)));
        assert_eq!(get_int(&c, "entity_id"), Some(229));
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse_message("not json").is_none());
        assert!(parse_message(r#"{"A":{"x":1}"#).is_none(), "不配对的括号应拒绝");
    }
}

// ---------------------------------------------------------------------------
// 对局内消息（第二轮抓包新增）
// ---------------------------------------------------------------------------
//
// 样本来自 `fixtures/protocol_match.jsonl`（真实对局抓包，109 条去重）。
// 这些消息把玩法语义直接暴露在线上，是回合/结算/生命最可靠的真值来源。

/// 关卡障碍：`{"AddObstacle":{"pos_x":..,"pos_y":..,"radius":..}}`
#[derive(Debug, Clone, PartialEq)]
pub struct AddObstacle {
    pub pos_x: f64,
    pub pos_y: f64,
    pub radius: f64,
}

/// 实体位置：`{"PosInfo":{"entity_id":..,"pos_x":..,"pos_y":..,"radius":..}}`
#[derive(Debug, Clone, PartialEq)]
pub struct PosInfo {
    pub entity_id: i64,
    pub pos_x: f64,
    pub pos_y: f64,
    pub radius: f64,
}

/// 生死：`{"LifeInfo":{"entity_id":..,"alive":true}}`
#[derive(Debug, Clone, PartialEq)]
pub struct LifeInfo {
    pub entity_id: i64,
    pub alive: bool,
}

/// 玩家函数：`{"FunctionInfo":{"owner_id":..,"function":"..","corrected_function":null}}`
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionInfo {
    pub owner_id: i64,
    pub function: String,
    /// 原程序在函数未提交时给 `null`；这里用 `None` 表示。
    pub corrected_function: Option<String>,
}

/// 取一个浮点字段。
pub fn get_f64(msg: &RawMessage, key: &str) -> Option<f64> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.' || c == 'e' || c == 'E' || c == '+'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// 取布尔字段。
pub fn get_bool(msg: &RawMessage, key: &str) -> Option<bool> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

impl AddObstacle {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        Some(AddObstacle {
            pos_x: get_f64(msg, "pos_x")?,
            pos_y: get_f64(msg, "pos_y")?,
            radius: get_f64(msg, "radius")?,
        })
    }
}

impl PosInfo {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        Some(PosInfo {
            entity_id: get_int(msg, "entity_id")?,
            pos_x: get_f64(msg, "pos_x")?,
            pos_y: get_f64(msg, "pos_y")?,
            radius: get_f64(msg, "radius")?,
        })
    }
}

impl LifeInfo {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        Some(LifeInfo { entity_id: get_int(msg, "entity_id")?, alive: get_bool(msg, "alive")? })
    }
}

impl FunctionInfo {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        let owner_id = get_int(msg, "owner_id")?;
        let function = get_str(msg, "function").unwrap_or_default();
        let corrected_function = get_str(msg, "corrected_function");
        Some(FunctionInfo { owner_id, function, corrected_function })
    }
}


// ---------------------------------------------------------------------------
// 回合与开火（第三轮抓包新增，46 种消息里最关键的几个）
// ---------------------------------------------------------------------------

/// `{"TurnInfo":{"player_id":..,"soldier_id":..}}` —— 轮到谁
#[derive(Debug, Clone, PartialEq)]
pub struct TurnInfo {
    pub player_id: i64,
    pub soldier_id: i64,
}

/// `{"TurnCountInfo":{"turn_count":..}}` —— 第几回合
#[derive(Debug, Clone, PartialEq)]
pub struct TurnCountInfo {
    pub turn_count: i64,
}

/// `{"EndOfTurnInfo":{"end_of_turn_tick":..,"time_mode":".."}}` —— 回合结束时间点
#[derive(Debug, Clone, PartialEq)]
pub struct EndOfTurnInfo {
    pub end_of_turn_tick: i64,
    pub time_mode: String,
}

/// `{"FunctionFireRequest":{"player_id":..}}` —— 客户端请求开火
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionFireRequest {
    pub player_id: i64,
}

/// `{"FunctionFire":{"soldier_id":..}}` —— 服务端宣布某士兵开火
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionFire {
    pub soldier_id: i64,
}

/// `{"FunctionUpdateRequest":{"player_id":..,"function":".."}}` —— 提交/更新函数
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionUpdateRequest {
    pub player_id: i64,
    pub function: String,
}

impl TurnInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(TurnInfo { player_id: get_int(m, "player_id")?, soldier_id: get_int(m, "soldier_id")? })
    }
}
impl TurnCountInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(TurnCountInfo { turn_count: get_int(m, "turn_count")? })
    }
}
impl EndOfTurnInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(EndOfTurnInfo {
            end_of_turn_tick: get_int(m, "end_of_turn_tick")?,
            time_mode: get_str(m, "time_mode")?,
        })
    }
}
impl FunctionFireRequest {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(FunctionFireRequest { player_id: get_int(m, "player_id")? })
    }
}
impl FunctionFire {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(FunctionFire { soldier_id: get_int(m, "soldier_id")? })
    }
}
impl FunctionUpdateRequest {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(FunctionUpdateRequest {
            player_id: get_int(m, "player_id")?,
            function: get_str(m, "function").unwrap_or_default(),
        })
    }
}


// ---------------------------------------------------------------------------
// 死亡与效果（第五轮抓包新增，终于拿到 alive:false）
// ---------------------------------------------------------------------------

/// `{"LifeQueuedDeath":{"entity_id":..,"tick":..}}` —— 死亡被排入某个 tick
#[derive(Debug, Clone, PartialEq)]
pub struct LifeQueuedDeath {
    pub entity_id: i64,
    pub tick: i64,
}

/// `{"EffectInfo":{"effect_id":..,"pos_x":..,"pos_y":..,"effect_type":"..","start_tick":..}}`
#[derive(Debug, Clone, PartialEq)]
pub struct EffectInfo {
    pub effect_id: i64,
    pub pos_x: f64,
    pub pos_y: f64,
    pub effect_type: String,
    pub start_tick: i64,
}

/// `{"DestroyObstacle":{"pos_x":..,"pos_y":..,"radius":..}}` —— 障碍已销毁
#[derive(Debug, Clone, PartialEq)]
pub struct DestroyObstacle {
    pub pos_x: f64,
    pub pos_y: f64,
    pub radius: f64,
}

impl LifeQueuedDeath {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(LifeQueuedDeath { entity_id: get_int(m, "entity_id")?, tick: get_int(m, "tick")? })
    }
}
impl EffectInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(EffectInfo {
            effect_id: get_int(m, "effect_id")?,
            pos_x: get_f64(m, "pos_x")?,
            pos_y: get_f64(m, "pos_y")?,
            effect_type: get_str(m, "effect_type")?,
            start_tick: get_int(m, "start_tick")?,
        })
    }
}
impl DestroyObstacle {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(DestroyObstacle {
            pos_x: get_f64(m, "pos_x")?,
            pos_y: get_f64(m, "pos_y")?,
            radius: get_f64(m, "radius")?,
        })
    }
}

#[cfg(test)]
mod match_tests {
    use super::*;

    fn match_msgs() -> Vec<RawMessage> {
        include_str!("../fixtures/protocol_match.jsonl")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(parse_message)   // 广播那种非单键 JSON 不在此列
            .collect()
    }

    #[test]
    fn parses_all_match_frames() {
        let msgs = match_msgs();
        assert_eq!(msgs.len(), 84, "86 行夹具里有 2 行是 LAN 广播（非单键）");
        let kinds: std::collections::HashSet<&str> = msgs.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(kinds.len(), 45, "应覆盖 45 种单键消息（广播单独解析）");
    }

    #[test]
    fn parses_lan_broadcast() {
        let line = include_str!("../fixtures/protocol_match.jsonl")
            .lines()
            .find(|l| l.trim_start().starts_with("{\"room_name\""))
            .expect("夹具里应有广播");
        let b = Broadcast::parse(line).expect("广播应能解析");
        assert_eq!(b.room_name, "Graphwar Game");
        assert!(b.address.contains(':'));
        assert_eq!(b.time_mode, "Timer1m", "实测广播与 TCP 用的是同一种拼写");
    }

    #[test]
    fn parses_turn_and_fire_messages() {
        let msgs = match_msgs();
        let turn: Vec<TurnInfo> = msgs.iter().filter(|m| m.kind == "TurnInfo").filter_map(TurnInfo::parse).collect();
        assert!(!turn.is_empty(), "应抓到 TurnInfo");
        assert!(turn.iter().any(|t| t.player_id == 229 && t.soldier_id == 231), "实测样本");

        let eot: Vec<EndOfTurnInfo> = msgs.iter().filter(|m| m.kind == "EndOfTurnInfo").filter_map(EndOfTurnInfo::parse).collect();
        assert!(!eot.is_empty());
        assert!(eot.iter().any(|e| e.time_mode == "Timer3m"), "实测出现 Timer3m");

        let fires: Vec<FunctionFire> = msgs.iter().filter(|m| m.kind == "FunctionFire").filter_map(FunctionFire::parse).collect();
        assert!(!fires.is_empty(), "应抓到 FunctionFire");
        assert!(fires.iter().any(|f| f.soldier_id == 233));

        let cnt: Vec<TurnCountInfo> = msgs.iter().filter(|m| m.kind == "TurnCountInfo").filter_map(TurnCountInfo::parse).collect();
        assert!(cnt.iter().any(|c| c.turn_count == 0));
    }

    #[test]
    fn parses_obstacles_from_real_level() {
        let obstacles: Vec<AddObstacle> = match_msgs()
            .iter()
            .filter(|m| m.kind == "AddObstacle")
            .filter_map(AddObstacle::parse)
            .collect();
        assert_eq!(obstacles.len(), 3, "夹具里最多保留 3 条障碍样本");
        // 夹具里保留的 3 条实测样本
        let o = &obstacles[0];
        assert!((o.pos_x - 10.349758594817281).abs() < 1e-12);
        assert!((o.pos_y - (-15.026524279418366)).abs() < 1e-12);
        assert!((o.radius - 2.26759028710403).abs() < 1e-12);
        // 半径出现过 0.8（两次，像最小值）；坐标可超出棋盘（|y|=16.27 > 15）
        assert!(obstacles.iter().filter(|o| (o.radius - 0.8).abs() < 1e-12).count() == 2);
        assert!(obstacles.iter().any(|o| o.pos_y.abs() > 15.0), "障碍允许落在场外");
    }

    #[test]
    fn parses_soldier_positions() {
        let pos: Vec<PosInfo> = match_msgs()
            .iter()
            .filter(|m| m.kind == "PosInfo")
            .filter_map(PosInfo::parse)
            .collect();
        assert!(!pos.is_empty());
        assert!(pos.iter().all(|p| (p.radius - 0.5).abs() < 1e-12), "士兵半径实测恒为 0.5");
        assert!(pos.iter().any(|p| p.entity_id == 231));
    }

    #[test]
    fn parses_life_and_functions() {
        let lives: Vec<LifeInfo> = match_msgs()
            .iter()
            .filter(|m| m.kind == "LifeInfo")
            .filter_map(LifeInfo::parse)
            .collect();
        assert!(!lives.is_empty());
        assert!(lives.iter().all(|l| l.alive), "这一局的样本都是存活");

        let funcs: Vec<FunctionInfo> = match_msgs()
            .iter()
            .filter(|m| m.kind == "FunctionInfo")
            .filter_map(FunctionInfo::parse)
            .collect();
        // 夹具里只保留了 3 条，都是"尚未提交"的空串形态；
        // 实抓里还出现过 "exp(sin(x))"（见 docs/24 §4.5）
        assert!(funcs.iter().all(|f| f.function.is_empty()));
        assert!(funcs.iter().all(|f| f.corrected_function.is_none()));
    }
}


#[cfg(test)]
mod death_tests {
    use super::*;

    fn msgs() -> Vec<RawMessage> {
        include_str!("../fixtures/protocol_death.jsonl")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(parse_message)
            .collect()
    }

    #[test]
    fn captured_death_flow_is_coherent() {
        let all = msgs();
        assert!(!all.is_empty());

        let deaths: Vec<LifeQueuedDeath> = all.iter().filter(|m| m.kind == "LifeQueuedDeath").filter_map(LifeQueuedDeath::parse).collect();
        assert_eq!(deaths.len(), 2, "抓到两个被排入的死亡");
        // 实测：231 在 tick 238，230 在 tick 953
        assert!(deaths.iter().any(|d| d.entity_id == 231 && d.tick == 238));
        assert!(deaths.iter().any(|d| d.entity_id == 230 && d.tick == 953));

        let effects: Vec<EffectInfo> = all.iter().filter(|m| m.kind == "EffectInfo").filter_map(EffectInfo::parse).collect();
        // 死亡效果与死亡 tick 对齐：DeathEffect.start_tick == LifeQueuedDeath.tick
        for d in &deaths {
            let matched = effects.iter().any(|e| e.effect_type == "DeathEffect" && e.start_tick == d.tick);
            assert!(matched, "死亡 tick {} 没有对应的 DeathEffect", d.tick);
        }
        assert!(effects.iter().any(|e| e.effect_type == "ExplosionEffect"));

        let dead: Vec<LifeInfo> = all.iter().filter(|m| m.kind == "LifeInfo").filter_map(LifeInfo::parse).collect();
        assert!(dead.iter().any(|l| l.entity_id == 230 && !l.alive), "应有 230 的死亡记录");
    }

    #[test]
    fn captured_game_states() {
        let states: Vec<String> = msgs()
            .iter()
            .filter(|m| m.kind == "GameStateInfo")
            .filter_map(|m| get_str(m, "game_state"))
            .collect();
        // 实测出现过的状态
        assert!(states.iter().any(|s| s == "Setup"));
        assert!(states.iter().any(|s| s == "WaitingForFunctions"));
    }
}


#[cfg(test)]
mod broadcast_tests {
    use super::*;

    fn captured_broadcasts() -> Vec<String> {
        include_str!("../fixtures/protocol_match.jsonl")
            .lines()
            .filter(|l| l.trim_start().starts_with("{\"room_name\""))
            .map(|l| l.trim().to_string())
            .collect()
    }

    /// 核心测试：解析抓到的广播再重新序列化，必须**逐字节**还原。
    #[test]
    fn reserializes_captured_broadcasts_byte_for_byte() {
        let lines = captured_broadcasts();
        assert!(!lines.is_empty(), "夹具里应有 LAN 广播样本");
        for line in lines {
            let b = Broadcast::parse(&line).expect("广播应能解析");
            assert_eq!(b.to_json(), line, "重新序列化后与原报文不一致");
        }
    }

    /// 广播本身不带长度前缀；对局 TCP 帧才带 `u16` 大端长度。
    #[test]
    fn frame_prefix_is_not_part_of_broadcast() {
        let line = captured_broadcasts().remove(0);
        let b = Broadcast::parse(&line).unwrap();
        let json = b.to_json();
        assert!(!json.starts_with('\u{0}'), "广播不应带二进制长度前缀");
        // 而 Frame 编码会加 2 字节前缀，长度等于负载字节数
        let f = Frame { payload: json.clone() };
        let wire = f.encode();
        assert_eq!(wire.len(), json.len() + 2);
    }
}

// ---------------------------------------------------------------------------
// 消息序列化（只覆盖我们会主动生成的那些）
// ---------------------------------------------------------------------------

/// `{"TurnInfo":{"player_id":..,"soldier_id":..}}`
pub fn emit_turn_info(player_id: i64, soldier_id: i64) -> String {
    format!(r#"{{"TurnInfo":{{"player_id":{},"soldier_id":{}}}}}"#, player_id, soldier_id)
}

/// `{"LifeQueuedDeath":{"entity_id":..,"tick":..}}`
pub fn emit_life_queued_death(entity_id: i64, tick: i64) -> String {
    format!(r#"{{"LifeQueuedDeath":{{"entity_id":{},"tick":{}}}}}"#, entity_id, tick)
}

/// `{"FunctionPoints":{"owner_id":..,"start_x":..,"start_y":..,"end_x":..}}`
///
/// 浮点用 Rust 的最短往返表示（与抓包里的 `-16.405029008797804` 同形）。
pub fn emit_function_points(owner_id: i64, start_x: f64, start_y: f64, end_x: f64) -> String {
    format!(
        r#"{{"FunctionPoints":{{"owner_id":{},"start_x":{},"start_y":{},"end_x":{}}}}}"#,
        owner_id, start_x, start_y, end_x
    )
}

/// `{"EffectInfo":{"effect_id":..,"pos_x":..,"pos_y":..,"effect_type":"..","start_tick":..}}`
pub fn emit_effect_info(effect_id: i64, x: f64, y: f64, effect_type: &str, start_tick: i64) -> String {
    format!(
        r#"{{"EffectInfo":{{"effect_id":{},"pos_x":{},"pos_y":{},"effect_type":"{}","start_tick":{}}}}}"#,
        effect_id, x, y, effect_type, start_tick
    )
}

#[cfg(test)]
mod emit_tests {
    use super::*;

    /// 从夹具里找某类消息的全部样本。
    fn samples(kind: &str) -> Vec<String> {
        let mut out = Vec::new();
        for text in [
            include_str!("../fixtures/protocol_match.jsonl"),
            include_str!("../fixtures/protocol_death.jsonl"),
            include_str!("../fixtures/protocol.jsonl"),
        ] {
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with(&format!("{{\"{}\"", kind)) {
                    out.push(line.to_string());
                }
            }
        }
        out.dedup();
        out
    }

    #[test]
    fn emitted_turn_info_matches_captured() {
        let all = samples("TurnInfo");
        assert!(!all.is_empty());
        for line in all {
            let m = parse_message(&line).unwrap();
            let got = emit_turn_info(get_int(&m, "player_id").unwrap(), get_int(&m, "soldier_id").unwrap());
            assert_eq!(got, line, "TurnInfo 序列化与抓包不一致");
        }
    }

    #[test]
    fn emitted_life_queued_death_matches_captured() {
        for line in samples("LifeQueuedDeath") {
            let m = parse_message(&line).unwrap();
            let got = emit_life_queued_death(get_int(&m, "entity_id").unwrap(), get_int(&m, "tick").unwrap());
            assert_eq!(got, line, "LifeQueuedDeath 序列化与抓包不一致");
        }
    }

    /// 浮点格式：Rust 的最短往返表示必须与游戏一致。
    #[test]
    fn emitted_function_points_matches_captured_floats() {
        for line in samples("FunctionPoints") {
            let m = parse_message(&line).unwrap();
            let got = emit_function_points(
                get_int(&m, "owner_id").unwrap(),
                get_f64(&m, "start_x").unwrap(),
                get_f64(&m, "start_y").unwrap(),
                get_f64(&m, "end_x").unwrap(),
            );
            assert_eq!(got, line, "FunctionPoints 序列化与抓包不一致（浮点格式）");
        }
    }

    #[test]
    fn emitted_effect_info_matches_captured() {
        for line in samples("EffectInfo") {
            let m = parse_message(&line).unwrap();
            let got = emit_effect_info(
                get_int(&m, "effect_id").unwrap(),
                get_f64(&m, "pos_x").unwrap(),
                get_f64(&m, "pos_y").unwrap(),
                &get_str(&m, "effect_type").unwrap(),
                get_int(&m, "start_tick").unwrap(),
            );
            assert_eq!(got, line, "EffectInfo 序列化与抓包不一致");
        }
    }
}
