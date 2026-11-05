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

/// `{"GameStateInfo":{"game_state":".."}}`
pub fn emit_game_state(state: &str) -> String {
    format!(r#"{{"GameStateInfo":{{"game_state":"{}"}}}}"#, state)
}

/// `{"FunctionInfo":{"owner_id":..,"function":"..","corrected_function":..}}`
///
/// `corrected` 为 `None` 时输出 `null`（与抓包一致）。
pub fn emit_function_info(owner_id: i64, function: &str, corrected: Option<&str>) -> String {
    match corrected {
        Some(c) => format!(
            r#"{{"FunctionInfo":{{"owner_id":{},"function":"{}","corrected_function":"{}"}}}}"#,
            owner_id, function, c
        ),
        None => format!(
            r#"{{"FunctionInfo":{{"owner_id":{},"function":"{}","corrected_function":null}}}}"#,
            owner_id, function
        ),
    }
}

/// `{"FunctionActive":{"owner_id":..,"active_tick":..}}`
pub fn emit_function_active(owner_id: i64, active_tick: i64) -> String {
    format!(r#"{{"FunctionActive":{{"owner_id":{},"active_tick":{}}}}}"#, owner_id, active_tick)
}

#[cfg(test)]
mod emit_turn_sequence_tests {
    use super::*;

    #[test]
    fn emitted_game_state_and_function_messages_match_captured() {
        assert_eq!(emit_game_state("CalculatingResults"),
                   r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#);
        assert_eq!(emit_function_active(231, 253),
                   r#"{"FunctionActive":{"owner_id":231,"active_tick":253}}"#);
        assert_eq!(emit_function_info(231, "", Some(" + 10.087")),
                   r#"{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.087"}}"#);
        assert_eq!(emit_function_info(231, "", None),
                   r#"{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}"#);
    }

    /// 抓包里那一回合的 `FunctionInfo`（修正后）必须能逐字节重现。
    #[test]
    fn captured_corrected_function_info_round_trips() {
        let line = r#"{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":" + 10.087"}}"#;
        let m = parse_message(line).unwrap();
        let owner = get_int(&m, "owner_id").unwrap();
        let function = get_str(&m, "function").unwrap_or_default();
        let corrected = get_str(&m, "corrected_function");
        assert_eq!(emit_function_info(owner, &function, corrected.as_deref()), line);
    }
}

/// `{"EndOfTurnInfo":{"end_of_turn_tick":..,"time_mode":".."}}`
pub fn emit_end_of_turn(end_of_turn_tick: i64, time_mode: &str) -> String {
    format!(
        r#"{{"EndOfTurnInfo":{{"end_of_turn_tick":{},"time_mode":"{}"}}}}"#,
        end_of_turn_tick, time_mode
    )
}

#[cfg(test)]
mod emit_end_of_turn_tests {
    use super::*;

    #[test]
    fn matches_captured_end_of_turn() {
        // 抓包样本（docs/45 附近）
        let line = r#"{"EndOfTurnInfo":{"end_of_turn_tick":3685,"time_mode":"Timer1m"}}"#;
        let m = parse_message(line).unwrap();
        assert_eq!(
            emit_end_of_turn(get_int(&m, "end_of_turn_tick").unwrap(),
                             &get_str(&m, "time_mode").unwrap()),
            line
        );
    }
}

// ---------------------------------------------------------------------------
// `docs/96` §3 补齐的 23 个事件
// ---------------------------------------------------------------------------
//
// 这一节只做**协议形状**：事件名 + 字段 + 未知字段原样回发。它不改变任何玩法行为，
// 也不碰 `src/events.rs` 的判别值（线上是外部标签 JSON，判别值不上线，`docs/96` §4）。
//
// ## 为什么分成"强类型"和"透传"两种
//
// 复原目标是**逐位相同**，所以只有**真实抓包证明过键名**的事件才允许建强类型结构体
// ——键名差一个字符就是另一条线格式，猜一个像样的名字比不实现更糟。
//
// | 档 | 数量 | 含义 |
// |---|---|---|
// | 强类型 | **13** | `fixtures/protocol*.jsonl` 里有**真实报文**，键名与键序逐位验证 |
// | 透传 | **10** | 名字已识别、能逐字节回放，但**字段名未证实** ⇒ 不声明字段、不参与序列化 |
//
// 23 个里 14 个有真实报文（共 41 行）：强类型 13 个 + `ClearObstacles`（实测空体，走透传）。
//
// ## 字段数（可信）与字段名（要分档）
//
// `.rodata` 里 serde 的 `struct <Name>Event with N elements` 给出 N，见
// [`SERDE_FIELD_COUNTS`]。**N 与实测是否一致**逐条写在结构体的 doc comment 里；
// 三处与 `docs/96` 上一稿 §3 不同，都是**观测**：
//
// 1. `NameInfo`(2) / `LockRequest`(1) / `TurnLimitInfo`(1) 的 N 在 `.rodata` 里**有**，
//    上一稿记成 `—` 是提取时漏了这三条；
// 2. 23 个里有 **14 个**在 `fixtures/` 里早有真实报文——`docs/96` §7 那句
//    "23 个里没有一个抓到过真实流量"与夹具**不符**；
// 3. `ClearObstacles` 实测体为空 `{}`（与 `docs/24` §4.6 同一句），
//    且它的变体**没有** `with N elements` 串。
//
// `GameHeartbeat` / `ClearObstacles` / `ClearBansRequest` 三个没有 N 串。同类的
// `GameStartRequest` 也没有，而它实测就是 `{}`。其中 `ClearObstacles` 已实测空体，
// `GameHeartbeat` / `ClearBansRequest` "是空体"仍只是**推断**；三者一律按透传实现：
// 不声明字段，来什么字段就原样收进 `extra`、原样发回去。
//
// ## 数值格式
//
// 这 23 个事件**没有浮点字段**（观测：41 行抓包原文里这 14 种只有整数、字符串、
// `null` 和空对象）。所以不存在 serde 的 `1.0` vs `1` 之争；强类型结构体里的数字
// 全是 `i64`，用 `i64::to_string()` 输出，与 serde_json 的整数表示一致。

/// 取字符串字段，并按 JSON 规则解转义。
///
/// [`get_str`] 不解转义，遇到 `\"` 会提前截断；昵称与外观名都可能带引号，
/// 所以本节统一用这个。
///
/// 局限（未知）：`\uD83D\uDE00` 这类代理对会解成 U+FFFD（没有样本需要）。
pub fn get_str_unescaped(msg: &RawMessage, key: &str) -> Option<String> {
    let pat = format!("\"{}\":", key);
    let i = msg.body.find(&pat)? + pat.len();
    let rest = msg.body[i..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((_, c)) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            hex.push(chars.next()?.1);
                        }
                        let v = u32::from_str_radix(&hex, 16).ok()?;
                        out.push(char::from_u32(v).unwrap_or('\u{fffd}'));
                    }
                    _ => return None,
                }
            }
            _ => out.push(c),
        }
    }
    None // 没有闭合引号
}

/// 把字符串编成 JSON 字面量（带引号），与 serde_json 的字符串转义一致。
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 整数编成 JSON 数字（与 serde_json 的 `i64` 表示一致）。
pub fn json_i64(v: i64) -> String {
    v.to_string()
}

/// `Option<i64>` 编成数字或 `null`（抓包里实测出现过 `"rank":null`）。
pub fn json_opt_i64(v: Option<i64>) -> String {
    match v {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    }
}

/// 把对象体切成 `(键, 原始值文本)`，值文本**逐字保留**（含嵌套对象、数组、字符串）。
///
/// 只按**顶层**逗号切分：括号内与字符串内的逗号不切（聊天正文里就有逗号）。
fn split_fields(body: &str) -> Vec<(String, String)> {
    let bytes = body.as_bytes();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0usize;
    loop {
        while i < bytes.len() && (bytes[i] == b',' || (bytes[i] as char).is_whitespace()) {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'"' {
            break;
        }
        let key_start = i + 1;
        let mut j = key_start;
        while j < bytes.len() && bytes[j] != b'"' {
            if bytes[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
        if j >= bytes.len() {
            break;
        }
        let key = body[key_start..j].to_string();
        j += 1;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b':' {
            break;
        }
        j += 1;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        let val_start = j;
        let mut depth = 0i32;
        let mut in_str = false;
        while j < bytes.len() {
            let c = bytes[j];
            if in_str {
                if c == b'\\' {
                    j += 2;
                    continue;
                }
                if c == b'"' {
                    in_str = false;
                }
            } else if c == b'"' {
                in_str = true;
            } else if c == b'{' || c == b'[' {
                depth += 1;
            } else if c == b'}' || c == b']' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if c == b',' && depth == 0 {
                break;
            }
            j += 1;
        }
        out.push((key, body[val_start..j].trim().to_string()));
        i = j;
    }
    out
}

/// 取出 `body` 里**不在** `known` 里的字段，供"原样保留并回发"用。
pub fn extra_fields(body: &str, known: &[&str]) -> Vec<(String, String)> {
    split_fields(body)
        .into_iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .collect()
}

/// 组装 `{"Kind":{…}}`：先写已知字段（声明顺序），再写未知字段（原样、原顺序）。
pub fn emit_object(kind: &str, known: &[(&str, String)], extra: &[(String, String)]) -> String {
    let mut out = String::with_capacity(64);
    out.push('{');
    out.push_str(&json_str(kind));
    out.push_str(":{");
    let mut first = true;
    let pairs = known
        .iter()
        .map(|(k, v)| (*k, v))
        .chain(extra.iter().map(|(k, v)| (k.as_str(), v)));
    for (k, v) in pairs {
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&json_str(k));
        out.push(':');
        out.push_str(v);
    }
    out.push_str("}}");
    out
}

/// 生成"字段名未证实 ⇒ 只透传"的事件。
///
/// 结构体**不声明任何字段**：解析时把整段 `{…}` 的字段按出现顺序原样存下，
/// 序列化时原样拼回。这样既不会产生错误的线格式，也不会丢掉将来的新字段。
macro_rules! passthrough_event {
    ($(#[$meta:meta])* $ty:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            /// `{…}` 里的字段，按出现顺序**原样**保留（本事件不声明任何已知字段）。
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self { extra: split_fields(&m.body) })
            }
            pub fn to_msg(&self) -> String {
                emit_object(stringify!($ty), &[], &self.extra)
            }
        }
    };
}

/// 生成"`entity_id` + 一个字符串"形状的事件（线上键名 = 字段名）。
macro_rules! entity_str_event {
    ($(#[$meta:meta])* $ty:ident, $field:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $ty {
            pub entity_id: i64,
            pub $field: String,
            /// 未识别字段（键 / 原始值文本），按出现顺序保留，[`Self::to_msg`] 原样回发。
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    entity_id: get_int(m, "entity_id")?,
                    $field: get_str_unescaped(m, stringify!($field))?,
                    extra: extra_fields(&m.body, &["entity_id", stringify!($field)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[("entity_id", json_i64(self.entity_id)),
                      (stringify!($field), json_str(&self.$field))],
                    &self.extra,
                )
            }
        }
    };
}

// ---------- 强类型 13 个：键名与键序有真实抓包逐位验证 ----------

entity_str_event! {
    /// `{"NameRequest":{"entity_id":229,"name":"user"}}` —— 请求改名。
    ///
    /// **逐位验证**：真实报文（`fixtures/protocol.jsonl`、`protocol_match.jsonl`）。
    /// N=2（serde 原文）与实测一致。
    NameRequest, name
}

entity_str_event! {
    /// `{"NameInfo":{"entity_id":229,"name":"user"}}` —— 广播某人当前的名字。
    ///
    /// **逐位验证**：真实报文（夹具）。N=2 与实测一致
    /// （`docs/96` §3 把 N 记成 `—`，是提取漏了这条 serde 串）。
    NameInfo, name
}

entity_str_event! {
    /// `{"SkinGraphicsRequest":{"entity_id":229,"graphics_str":"yellow"}}` —— 请求换皮肤。
    ///
    /// **逐位验证**：真实报文（夹具；实测取值 `yellow` / `bot_0`）。N=2 与实测一致。
    SkinGraphicsRequest, graphics_str
}

entity_str_event! {
    /// `{"SkinGraphicsInfo":{"entity_id":229,"graphics_str":"yellow"}}` —— 广播皮肤。
    ///
    /// **逐位验证**：真实报文（夹具）。N=2 与实测一致。
    SkinGraphicsInfo, graphics_str
}

entity_str_event! {
    /// `{"FaceGraphicsRequest":{"entity_id":229,"graphics_str":"regular_eyes"}}` —— 请求换脸。
    ///
    /// **逐位验证**：真实报文（夹具；实测 `regular_eyes` / `no_face`）。N=2 与实测一致。
    FaceGraphicsRequest, graphics_str
}

entity_str_event! {
    /// `{"FaceGraphicsInfo":{"entity_id":229,"graphics_str":"regular_eyes"}}` —— 广播脸。
    ///
    /// **逐位验证**：真实报文（夹具）。N=2 与实测一致。
    FaceGraphicsInfo, graphics_str
}

entity_str_event! {
    /// `{"HatGraphicsRequest":{"entity_id":229,"graphics_str":"soldier_helmet"}}` —— 请求换帽子。
    ///
    /// **逐位验证**：真实报文（夹具；实测 `soldier_helmet` / `no_hat`）。N=2 与实测一致。
    HatGraphicsRequest, graphics_str
}

entity_str_event! {
    /// `{"HatGraphicsInfo":{"entity_id":229,"graphics_str":"soldier_helmet"}}` —— 广播帽子。
    ///
    /// **逐位验证**：真实报文（夹具）。N=2 与实测一致。
    HatGraphicsInfo, graphics_str
}

entity_str_event! {
    /// `{"EntityEffectInfo":{"entity_id":230,"effect_type":"DeathEffect"}}` —— 实体挂了某个效果。
    ///
    /// **逐位验证**：真实报文（夹具；实测 `effect_type` 全是 `DeathEffect`）。N=2 与实测一致。
    EntityEffectInfo, effect_type
}

/// `{"RankRequest":{"entity_id":229,"rank":0}}` —— 请求/汇报段位。
///
/// **逐位验证**：真实报文（夹具）。N=2 与实测一致。
/// `rank` 实测可以是 `null`（机器人 `{"entity_id":232,"rank":null}`），所以是 `Option`。
#[derive(Debug, Clone, PartialEq)]
pub struct RankRequest {
    pub entity_id: i64,
    /// `None` 既表示线路上是 `null`，也表示字段缺失——两者本实现不作区分。
    pub rank: Option<i64>,
    /// 未识别字段，原样保留并回发。
    pub extra: Vec<(String, String)>,
}

impl RankRequest {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(RankRequest {
            entity_id: get_int(m, "entity_id")?,
            rank: get_int(m, "rank"),
            extra: extra_fields(&m.body, &["entity_id", "rank"]),
        })
    }
    pub fn to_msg(&self) -> String {
        emit_object(
            "RankRequest",
            &[("entity_id", json_i64(self.entity_id)), ("rank", json_opt_i64(self.rank))],
            &self.extra,
        )
    }
}

/// `{"RankInfo":{"entity_id":229,"rank":0}}` —— 广播段位（`rank` 可为 `null`）。
///
/// **逐位验证**：真实报文（夹具）。N=2 与实测一致。
#[derive(Debug, Clone, PartialEq)]
pub struct RankInfo {
    pub entity_id: i64,
    pub rank: Option<i64>,
    /// 未识别字段，原样保留并回发。
    pub extra: Vec<(String, String)>,
}

impl RankInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(RankInfo {
            entity_id: get_int(m, "entity_id")?,
            rank: get_int(m, "rank"),
            extra: extra_fields(&m.body, &["entity_id", "rank"]),
        })
    }
    pub fn to_msg(&self) -> String {
        emit_object(
            "RankInfo",
            &[("entity_id", json_i64(self.entity_id)), ("rank", json_opt_i64(self.rank))],
            &self.extra,
        )
    }
}

/// `{"VersionInfo":{"version":"2.0.60","system":"linux"}}` —— 版本握手。
///
/// **逐位验证**：真实报文（`fixtures/protocol_match.jsonl`）。N=2 与实测一致。
#[derive(Debug, Clone, PartialEq)]
pub struct VersionInfo {
    pub version: String,
    pub system: String,
    /// 未识别字段，原样保留并回发。
    pub extra: Vec<(String, String)>,
}

impl VersionInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(VersionInfo {
            version: get_str_unescaped(m, "version")?,
            system: get_str_unescaped(m, "system")?,
            extra: extra_fields(&m.body, &["version", "system"]),
        })
    }
    pub fn to_msg(&self) -> String {
        emit_object(
            "VersionInfo",
            &[("version", json_str(&self.version)), ("system", json_str(&self.system))],
            &self.extra,
        )
    }
}

/// `{"BotAdded":{"connection_id":228,"player_id":232,"level":3}}` —— 机器人已加入。
///
/// **逐位验证**：真实报文（夹具；实测 `level=3`）。N=3 与实测一致。
#[derive(Debug, Clone, PartialEq)]
pub struct BotAdded {
    pub connection_id: i64,
    pub player_id: i64,
    pub level: i64,
    /// 未识别字段，原样保留并回发。
    pub extra: Vec<(String, String)>,
}

impl BotAdded {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(BotAdded {
            connection_id: get_int(m, "connection_id")?,
            player_id: get_int(m, "player_id")?,
            level: get_int(m, "level")?,
            extra: extra_fields(&m.body, &["connection_id", "player_id", "level"]),
        })
    }
    pub fn to_msg(&self) -> String {
        emit_object(
            "BotAdded",
            &[
                ("connection_id", json_i64(self.connection_id)),
                ("player_id", json_i64(self.player_id)),
                ("level", json_i64(self.level)),
            ],
            &self.extra,
        )
    }
}

// ---------- 透传 10 个：名字已识别，字段名未证实 ----------

passthrough_event! {
    /// `{"LockInfo":{…}}` —— 房间是否已锁。
    ///
    /// **推断/透传**：N=1（serde 原文，可信）；`.rodata` 词汇表里有 `locked`
    /// （`GameInfo.locked`、LAN 广播的 `locked` 是同一概念的实测证据），
    /// 但**没有这个事件的一份抓包**能证明它就叫 `locked`、也不是别的编码
    /// ⇒ 不声明字段，原样透传。
    LockInfo
}

passthrough_event! {
    /// `{"LockRequest":{…}}` —— 请求锁房/解锁（房主权限）。
    ///
    /// **推断/透传**：N=1（serde 原文）；与 [`LockInfo`] 同族，但字段名同样未证实。
    LockRequest
}

passthrough_event! {
    /// `{"ChatMessage":{…}}` —— 聊天。
    ///
    /// **推断/透传**：N=2（serde 原文）。`message` 在 `.rodata` 里紧邻该变体首次出现
    /// （与 `version`/`system`、`subscriber_id`、`turn_limit` 同一张表、同一位置规律），
    /// 但**第二个字段名被 serde 去重到别处**，且**没有抓包** ⇒ 整条透传，
    /// 不猜"`entity_id` + `message`"。
    ChatMessage
}

passthrough_event! {
    /// `{"ChatMessageRequest":{…}}` —— 发送聊天。
    ///
    /// **推断/透传**：N=2（serde 原文），`message` 同样紧邻出现；第二个字段名未知。
    ChatMessageRequest
}

passthrough_event! {
    /// `{"GameHeartbeat":{…}}` —— 心跳/保活。
    ///
    /// **未知**：`.rodata` 里**没有** `GameHeartbeatEvent with N elements`，也没有
    /// 相邻字段名 ⇒ N 与字段名都不知道。同类（`ClearObstacles` / `GameStartRequest`）
    /// 实测是空体 `{}`，"大概率为空体"只是**推断**；按透传实现，来了字段也照收照发。
    GameHeartbeat
}

passthrough_event! {
    /// `{"GameRemoved":{…}}` —— 房间从列表里消失。
    ///
    /// **推断/透传**：N=1（serde 原文），`game_address` 在 `.rodata` 里紧邻该变体首次
    /// 出现（`docs/07` §3.1 同）⇒ 归属较可靠，但**没有抓包** ⇒ 仍透传。
    GameRemoved
}

passthrough_event! {
    /// `{"TurnLimitInfo":{…}}` —— 回合数上限。
    ///
    /// **推断/透传**：N=1（serde 原文），`turn_limit` 紧邻该变体首次出现
    /// （`docs/07` §3.3 同）；**没有抓包** ⇒ 仍透传。
    TurnLimitInfo
}

passthrough_event! {
    /// `{"ClearObstacles":{}}` —— 清场。
    ///
    /// **观测**：真实报文就是空体 `{}`（`fixtures/protocol_match.jsonl`、`docs/24` §4.6），
    /// 且该变体没有 `with N elements` 串。没有字段可声明，仍按透传实现
    /// （将来若出现字段也能原样转发）。
    ClearObstacles
}

passthrough_event! {
    /// `{"ClearBansRequest":{…}}` —— 清空封禁列表。
    ///
    /// **未知**：`.rodata` 里**没有** `ClearBansRequestEvent with N elements`。
    /// 与 `ClearObstacles` 同属"管理类清空"是**推断**，且**没有抓包** ⇒ 不声明字段。
    ClearBansRequest
}

passthrough_event! {
    /// `{"BanConnectionRequest":{…}}` —— 踢人。
    ///
    /// **推断/透传**：N=2（serde 原文），`player_connection_id` /
    /// `leader_connection_id` 两个名字首次出现就紧邻该变体、正好两个 ⇒ 归属较可靠，
    /// 但**没有抓包** ⇒ 仍透传。
    BanConnectionRequest
}

/// 补齐的 23 个事件的线上键名（与结构体名一致，供测试与 `docs/96` 对账）。
pub const SUPPLEMENTED_EVENT_NAMES: [&str; 23] = [
    "NameRequest",
    "NameInfo",
    "RankRequest",
    "RankInfo",
    "LockInfo",
    "LockRequest",
    "ChatMessage",
    "ChatMessageRequest",
    "SkinGraphicsRequest",
    "SkinGraphicsInfo",
    "FaceGraphicsRequest",
    "FaceGraphicsInfo",
    "HatGraphicsRequest",
    "HatGraphicsInfo",
    "GameHeartbeat",
    "GameRemoved",
    "VersionInfo",
    "BotAdded",
    "TurnLimitInfo",
    "EntityEffectInfo",
    "ClearObstacles",
    "ClearBansRequest",
    "BanConnectionRequest",
];

/// `.rodata` 里 serde 的 `struct <Name>Event with N elements` 给出的 N。
///
/// `None` = **没有这条串**（`docs/96` §3 里的 `—`），**不是** 0。
/// N 是**观测**；N 是否与抓包一致，见每条事件的 doc comment。
pub const SERDE_FIELD_COUNTS: [(&str, Option<usize>); 23] = [
    ("NameRequest", Some(2)),
    ("NameInfo", Some(2)),
    ("RankRequest", Some(2)),
    ("RankInfo", Some(2)),
    ("LockInfo", Some(1)),
    ("LockRequest", Some(1)),
    ("ChatMessage", Some(2)),
    ("ChatMessageRequest", Some(2)),
    ("SkinGraphicsRequest", Some(2)),
    ("SkinGraphicsInfo", Some(2)),
    ("FaceGraphicsRequest", Some(2)),
    ("FaceGraphicsInfo", Some(2)),
    ("HatGraphicsRequest", Some(2)),
    ("HatGraphicsInfo", Some(2)),
    ("GameHeartbeat", None),
    ("GameRemoved", Some(1)),
    ("VersionInfo", Some(2)),
    ("BotAdded", Some(3)),
    ("TurnLimitInfo", Some(1)),
    ("EntityEffectInfo", Some(2)),
    ("ClearObstacles", None),
    ("ClearBansRequest", None),
    ("BanConnectionRequest", Some(2)),
];

/// 有真实抓包、**字段名逐位验证**过的 13 个事件（因而建了强类型结构体）。
///
/// 其余 10 个（`LockInfo` / `LockRequest` / `ChatMessage` / `ChatMessageRequest` /
/// `GameHeartbeat` / `GameRemoved` / `TurnLimitInfo` / `ClearObstacles` /
/// `ClearBansRequest` / `BanConnectionRequest`）只做透传。
pub const CAPTURED_TYPED_EVENTS: [&str; 13] = [
    "NameRequest",
    "NameInfo",
    "RankRequest",
    "RankInfo",
    "SkinGraphicsRequest",
    "SkinGraphicsInfo",
    "FaceGraphicsRequest",
    "FaceGraphicsInfo",
    "HatGraphicsRequest",
    "HatGraphicsInfo",
    "EntityEffectInfo",
    "VersionInfo",
    "BotAdded",
];

/// 生成 `AnyEvent`：变体名 = 结构体名 = 线上键名（三者必须一致）。
macro_rules! any_event {
    ($($ty:ident),* $(,)?) => {
        /// 这 23 个补齐事件的统一入口：按线上键名分派，未知键返回 `None`。
        #[derive(Debug, Clone, PartialEq)]
        pub enum AnyEvent {
            $($ty($ty),)*
        }

        impl AnyEvent {
            /// 按 `{"键名":{…}}` 的键名分派。
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(match m.kind.as_str() {
                    $(stringify!($ty) => AnyEvent::$ty($ty::parse(m)?),)*
                    _ => return None,
                })
            }

            /// 线上键名。
            pub fn kind(&self) -> &'static str {
                match self {
                    $(AnyEvent::$ty(_) => stringify!($ty),)*
                }
            }

            /// 重新序列化成整帧 JSON 原文。
            pub fn to_msg(&self) -> String {
                match self {
                    $(AnyEvent::$ty(e) => e.to_msg(),)*
                }
            }

            /// 未识别字段（键 + 原始值文本）。
            ///
            /// 透传事件没有声明任何字段，所以这里返回它的**全部**字段；
            /// 强类型事件只返回已知字段之外的那些。
            pub fn extra_fields(&self) -> &[(String, String)] {
                match self {
                    $(AnyEvent::$ty(e) => &e.extra,)*
                }
            }
        }
    };
}

any_event!(
    NameRequest,
    NameInfo,
    RankRequest,
    RankInfo,
    LockInfo,
    LockRequest,
    ChatMessage,
    ChatMessageRequest,
    SkinGraphicsRequest,
    SkinGraphicsInfo,
    FaceGraphicsRequest,
    FaceGraphicsInfo,
    HatGraphicsRequest,
    HatGraphicsInfo,
    GameHeartbeat,
    GameRemoved,
    VersionInfo,
    BotAdded,
    TurnLimitInfo,
    EntityEffectInfo,
    ClearObstacles,
    ClearBansRequest,
    BanConnectionRequest,
);

#[cfg(test)]
mod supplemented_event_tests {
    use super::*;

    /// 每条事件的样本行。标 `fixture` 的是**真实抓包原文**（逐字节回放）；
    /// 标 `合成` 的是为覆盖形状而构造的——本仓库没有任何一本抓包含该事件。
    const SAMPLES: &[(&str, &str)] = &[
        ("NameRequest", r#"{"NameRequest":{"entity_id":229,"name":"user"}}"#), // fixture
        ("NameInfo", r#"{"NameInfo":{"entity_id":232,"name":"Skynet"}}"#),     // fixture
        ("RankRequest", r#"{"RankRequest":{"entity_id":232,"rank":null}}"#),   // fixture
        ("RankInfo", r#"{"RankInfo":{"entity_id":229,"rank":0}}"#),            // fixture
        ("LockInfo", r#"{"LockInfo":{"locked":true}}"#),                       // 合成
        ("LockRequest", r#"{"LockRequest":{"locked":true}}"#),                 // 合成
        ("ChatMessage", r#"{"ChatMessage":{"entity_id":229,"message":"gg"}}"#), // 合成
        ("ChatMessageRequest", r#"{"ChatMessageRequest":{"entity_id":232,"message":"hi"}}"#), // 合成
        ("SkinGraphicsRequest", r#"{"SkinGraphicsRequest":{"entity_id":232,"graphics_str":"bot_0"}}"#), // fixture
        ("SkinGraphicsInfo", r#"{"SkinGraphicsInfo":{"entity_id":229,"graphics_str":"yellow"}}"#), // fixture
        ("FaceGraphicsRequest", r#"{"FaceGraphicsRequest":{"entity_id":232,"graphics_str":"no_face"}}"#), // fixture
        ("FaceGraphicsInfo", r#"{"FaceGraphicsInfo":{"entity_id":229,"graphics_str":"regular_eyes"}}"#), // fixture
        ("HatGraphicsRequest", r#"{"HatGraphicsRequest":{"entity_id":232,"graphics_str":"no_hat"}}"#), // fixture
        ("HatGraphicsInfo", r#"{"HatGraphicsInfo":{"entity_id":229,"graphics_str":"soldier_helmet"}}"#), // fixture
        ("GameHeartbeat", r#"{"GameHeartbeat":{}}"#),                          // 合成
        ("GameRemoved", r#"{"GameRemoved":{"game_address":"69.164.213.144:35981"}}"#), // 合成
        ("VersionInfo", r#"{"VersionInfo":{"version":"2.0.60","system":"linux"}}"#), // fixture
        ("BotAdded", r#"{"BotAdded":{"connection_id":228,"player_id":232,"level":3}}"#), // fixture
        ("TurnLimitInfo", r#"{"TurnLimitInfo":{"turn_limit":20}}"#),           // 合成
        ("EntityEffectInfo", r#"{"EntityEffectInfo":{"entity_id":230,"effect_type":"DeathEffect"}}"#), // fixture
        ("ClearObstacles", r#"{"ClearObstacles":{}}"#),                        // fixture
        ("ClearBansRequest", r#"{"ClearBansRequest":{}}"#),                    // 合成
        ("BanConnectionRequest",
         r#"{"BanConnectionRequest":{"player_connection_id":228,"leader_connection_id":229}}"#), // 合成
    ];

    fn sample_of(kind: &str) -> &'static str {
        SAMPLES
            .iter()
            .find(|(k, _)| *k == kind)
            .unwrap_or_else(|| panic!("样本表里没有 {kind}"))
            .1
    }

    /// 每个补齐事件一条往返测试：`解析 → 重新序列化` 必须逐字节相同。
    macro_rules! round_trip_tests {
        ($($name:ident => $ty:ident),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    let line = sample_of(stringify!($ty));
                    let m = parse_message(line).expect("样本应能解析成单键消息");
                    assert_eq!(m.kind, stringify!($ty));
                    let ev = AnyEvent::parse(&m).expect("应被识别为补齐事件");
                    assert_eq!(ev.kind(), stringify!($ty));
                    assert_eq!(ev.to_msg(), line, "{} 往返后字节不一致", stringify!($ty));
                }
            )*
        };
    }

    round_trip_tests! {
        round_trip_name_request => NameRequest,
        round_trip_name_info => NameInfo,
        round_trip_rank_request => RankRequest,
        round_trip_rank_info => RankInfo,
        round_trip_lock_info => LockInfo,
        round_trip_lock_request => LockRequest,
        round_trip_chat_message => ChatMessage,
        round_trip_chat_message_request => ChatMessageRequest,
        round_trip_skin_graphics_request => SkinGraphicsRequest,
        round_trip_skin_graphics_info => SkinGraphicsInfo,
        round_trip_face_graphics_request => FaceGraphicsRequest,
        round_trip_face_graphics_info => FaceGraphicsInfo,
        round_trip_hat_graphics_request => HatGraphicsRequest,
        round_trip_hat_graphics_info => HatGraphicsInfo,
        round_trip_game_heartbeat => GameHeartbeat,
        round_trip_game_removed => GameRemoved,
        round_trip_version_info => VersionInfo,
        round_trip_bot_added => BotAdded,
        round_trip_turn_limit_info => TurnLimitInfo,
        round_trip_entity_effect_info => EntityEffectInfo,
        round_trip_clear_obstacles => ClearObstacles,
        round_trip_clear_bans_request => ClearBansRequest,
        round_trip_ban_connection_request => BanConnectionRequest,
    }

    /// 23 个名字一个不多一个不少；三张表（名字 / 字段数 / 抓包档）互相一致。
    #[test]
    fn all_twenty_three_names_are_implemented() {
        assert_eq!(SUPPLEMENTED_EVENT_NAMES.len(), 23);
        assert_eq!(SERDE_FIELD_COUNTS.len(), 23);
        assert_eq!(SAMPLES.len(), 23, "样本表必须恰好每条事件一个");

        let from_samples: std::collections::BTreeSet<&str> =
            SAMPLES.iter().map(|(k, _)| *k).collect();
        let from_const: std::collections::BTreeSet<&str> =
            SUPPLEMENTED_EVENT_NAMES.iter().copied().collect();
        let from_counts: std::collections::BTreeSet<&str> =
            SERDE_FIELD_COUNTS.iter().map(|(k, _)| *k).collect();
        assert_eq!(from_samples, from_const, "样本表与常量表的事件名不一致");
        assert_eq!(from_counts, from_const, "字段数表与常量表的事件名不一致");

        // 顺序无关，但都不能有重复
        let mut sorted = SUPPLEMENTED_EVENT_NAMES;
        sorted.sort_unstable();
        for pair in sorted.windows(2) {
            assert_ne!(pair[0], pair[1], "事件名重复");
        }
    }

    /// 强类型那 13 个：样本的字段数必须与 serde 的 N **逐个相等**。
    #[test]
    fn typed_events_have_exactly_the_serde_field_count() {
        assert_eq!(CAPTURED_TYPED_EVENTS.len(), 13);
        for kind in CAPTURED_TYPED_EVENTS {
            let n = SERDE_FIELD_COUNTS
                .iter()
                .find(|(k, _)| *k == kind)
                .and_then(|(_, n)| *n)
                .unwrap_or_else(|| panic!("{kind} 应有 serde 字段数"));
            let line = sample_of(kind);
            let m = parse_message(line).expect("样本应能解析");
            let body = m.body.trim();
            let got = if body.is_empty() { 0 } else { split_fields(body).len() };
            assert_eq!(got, n, "{kind} 的实测字段数 {} 与 serde 的 N={} 不符", got, n);
        }
    }

    /// 透传那 10 个：**没有**在代码里声明任何已知字段（声明了就是猜键名）。
    ///
    /// 判定方式：解析后**全部**字段都必须落在 `extra` 里，并原样回发。
    #[test]
    fn passthrough_events_declare_no_field_names() {
        let passthrough: Vec<&str> = SUPPLEMENTED_EVENT_NAMES
            .iter()
            .copied()
            .filter(|k| !CAPTURED_TYPED_EVENTS.contains(k))
            .collect();
        assert_eq!(passthrough.len(), 10);
        for kind in passthrough {
            let line = sample_of(kind);
            let m = parse_message(line).expect("样本应能解析");
            let ev = AnyEvent::parse(&m).expect("应识别");
            let expected = split_fields(&m.body);
            assert_eq!(
                ev.extra_fields(),
                expected.as_slice(),
                "{kind} 的字段没有全部落进 extra：说明代码里给它声明了已知字段"
            );
            assert_eq!(ev.to_msg(), line);
        }
    }

    /// 夹具里属于这 23 个事件的行，必须能**逐字节**回放。
    /// 每种事件在夹具里的行数（`docs/96` §3 的表引用这些数字，钉住防止两边跑偏）。
    const CAPTURED_LINES: [(&str, usize); 14] = [
        ("NameRequest", 3),
        ("NameInfo", 3),
        ("RankRequest", 3),
        ("RankInfo", 3),
        ("SkinGraphicsRequest", 3),
        ("SkinGraphicsInfo", 4),
        ("FaceGraphicsRequest", 3),
        ("FaceGraphicsInfo", 4),
        ("HatGraphicsRequest", 3),
        ("HatGraphicsInfo", 4),
        ("EntityEffectInfo", 4),
        ("VersionInfo", 1),
        ("BotAdded", 2),
        ("ClearObstacles", 1),
    ];

    #[test]
    fn replays_every_captured_frame_of_the_supplemented_kinds() {
        let mut kinds = std::collections::BTreeMap::new();
        let mut lines = 0usize;
        for text in [
            include_str!("../fixtures/protocol.jsonl"),
            include_str!("../fixtures/protocol_match.jsonl"),
            include_str!("../fixtures/protocol_death.jsonl"),
        ] {
            for line in text.lines() {
                let line = line.trim();
                // LAN 广播不是单键消息，`parse_message` 返回 None
                let Some(m) = parse_message(line) else { continue };
                if !SUPPLEMENTED_EVENT_NAMES.contains(&m.kind.as_str()) {
                    continue;
                }
                let ev = AnyEvent::parse(&m).unwrap_or_else(|| panic!("{line} 应能解析"));
                assert_eq!(ev.kind(), m.kind);
                assert_eq!(ev.to_msg(), line, "{} 回放不一致", m.kind);
                *kinds.entry(m.kind.clone()).or_insert(0usize) += 1;
                lines += 1;
            }
        }
        assert_eq!(lines, 41, "夹具里属于这 23 个事件的行数变了");
        assert_eq!(kinds.len(), 14, "夹具覆盖的补齐事件种数变了");
        // 每种事件的行数与文档表一致
        for (kind, n) in CAPTURED_LINES {
            assert_eq!(kinds.get(kind), Some(&n), "{kind} 的抓包行数与文档不符");
        }
        // 抓到的 14 种里，13 种建了强类型，`ClearObstacles` 只有空体 ⇒ 透传
        for kind in kinds.keys() {
            if kind != "ClearObstacles" {
                assert!(
                    CAPTURED_TYPED_EVENTS.contains(&kind.as_str()),
                    "{kind} 有抓包但没有建强类型"
                );
            }
        }
    }

    /// 未知事件名不能被当成已知事件吞掉。
    #[test]
    fn unknown_kind_is_not_accepted() {
        for line in [
            r#"{"SomeFutureEvent":{"x":1}}"#,
            r#"{"NameInfoExtra":{"entity_id":1}}"#,
            r#"{"TickRequest":{}}"#, // 已实现，但不在本节范围
        ] {
            let m = parse_message(line).unwrap();
            assert!(AnyEvent::parse(&m).is_none(), "{line} 不应被识别");
        }
    }

    /// 未知字段：原样保留（键 + 原始值文本）并原样回发。
    #[test]
    fn unknown_fields_are_preserved_and_replayed() {
        // 强类型事件上的未知标量字段
        let line = r#"{"NameInfo":{"entity_id":229,"name":"user","title":"veteran"}}"#;
        let m = parse_message(line).unwrap();
        let AnyEvent::NameInfo(info) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 NameInfo")
        };
        assert_eq!(info.extra, vec![("title".to_string(), "\"veteran\"".to_string())]);
        assert_eq!(info.to_msg(), line, "未知标量字段没有原样回发");

        // 透传事件上的嵌套对象 + 数组（逗号不能被当成字段分隔）
        let line = r#"{"GameHeartbeat":{"seq":[1,2,3],"nested":{"a":1},"ok":true}}"#;
        let m = parse_message(line).unwrap();
        let AnyEvent::GameHeartbeat(hb) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 GameHeartbeat")
        };
        assert_eq!(
            hb.extra,
            vec![
                ("seq".to_string(), "[1,2,3]".to_string()),
                ("nested".to_string(), "{\"a\":1}".to_string()),
                ("ok".to_string(), "true".to_string()),
            ]
        );
        assert_eq!(hb.to_msg(), line, "未知嵌套字段没有原样回发");

        // 已知字段 + 未知字段混排：已知在前、未知按原顺序在后
        let line =
            r#"{"BotAdded":{"connection_id":228,"player_id":232,"level":3,"difficulty":"hard"}}"#;
        let m = parse_message(line).unwrap();
        let AnyEvent::BotAdded(bot) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 BotAdded")
        };
        assert_eq!(bot.extra, vec![("difficulty".to_string(), "\"hard\"".to_string())]);
        assert_eq!(bot.to_msg(), line);
    }

    /// 透传必须连**转义与字符串内的逗号**都一字节不动地保留。
    #[test]
    fn passthrough_keeps_escaped_strings_verbatim() {
        let line = r#"{"ChatMessage":{"entity_id":229,"message":"he said \"hi\", ok"}}"#;
        let m = parse_message(line).expect("含转义的帧应能解析");
        let AnyEvent::ChatMessage(chat) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 ChatMessage")
        };
        assert_eq!(chat.extra.len(), 2, "两个字段都应原样保留");
        assert_eq!(chat.extra[1].0, "message");
        assert_eq!(chat.extra[1].1, r#""he said \"hi\", ok""#);
        assert_eq!(chat.to_msg(), line);
    }

    /// 抓包里机器人的段位是 `null`：`Option` 必须把 `None` 发回成 `null`。
    #[test]
    fn rank_null_round_trips_as_json_null() {
        let line = r#"{"RankInfo":{"entity_id":232,"rank":null}}"#;
        let m = parse_message(line).unwrap();
        let AnyEvent::RankInfo(r) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 RankInfo")
        };
        assert_eq!(r.rank, None);
        assert_eq!(r.to_msg(), line);

        let r = RankInfo { entity_id: 229, rank: Some(0), extra: Vec::new() };
        assert_eq!(r.to_msg(), r#"{"RankInfo":{"entity_id":229,"rank":0}}"#);
    }

    /// 强类型事件的字符串字段：带引号的名字要能转义写出、解转义读回。
    #[test]
    fn typed_string_field_escapes_quotes_both_ways() {
        let name = r#"a "quoted" name"#;
        let ev = NameInfo { entity_id: 229, name: name.to_string(), extra: Vec::new() };
        let line = ev.to_msg();
        assert_eq!(line, r#"{"NameInfo":{"entity_id":229,"name":"a \"quoted\" name"}}"#);
        let m = parse_message(&line).expect("转义后的帧应能解析");
        let AnyEvent::NameInfo(back) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 NameInfo")
        };
        assert_eq!(back.name, name, "解转义后应还原原文");
        assert_eq!(back.to_msg(), line);
    }

    /// 解析不出来的样本不能被"猜"成默认值。
    #[test]
    fn missing_or_mistyped_required_field_is_rejected() {
        // `BotAdded` 少 `level`
        let m = parse_message(r#"{"BotAdded":{"connection_id":228,"player_id":232}}"#).unwrap();
        assert!(AnyEvent::parse(&m).is_none(), "缺必填字段应拒绝");
        // `NameInfo` 的 `name` 是字符串，不是数字
        let m = parse_message(r#"{"NameInfo":{"entity_id":229,"name":7}}"#).unwrap();
        assert!(AnyEvent::parse(&m).is_none(), "类型不符应拒绝");
    }
}
