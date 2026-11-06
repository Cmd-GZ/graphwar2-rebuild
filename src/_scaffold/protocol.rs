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
        include_str!("../../fixtures/protocol.jsonl")
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
        include_str!("../../fixtures/protocol_match.jsonl")
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
        let line = include_str!("../../fixtures/protocol_match.jsonl")
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
        include_str!("../../fixtures/protocol_death.jsonl")
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
        include_str!("../../fixtures/protocol_match.jsonl")
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
            include_str!("../../fixtures/protocol_match.jsonl"),
            include_str!("../../fixtures/protocol_death.jsonl"),
            include_str!("../../fixtures/protocol.jsonl"),
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
/// JSON 字符串转义（只处理字符串里可能出现的字符）。
///
/// 存在的理由：`docs/136` 在真语料里抓到一例 —— 玩家输入 `sqrt(\y)` 时，
/// 不转义会产出**非法 JSON**（`\y` 不是合法转义）。协议是 JSON，字符串必须转义。
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn emit_function_info(owner_id: i64, function: &str, corrected: Option<&str>) -> String {
    let f = json_escape(function);
    match corrected {
        Some(c) => format!(
            r#"{{"FunctionInfo":{{"owner_id":{},"function":"{}","corrected_function":"{}"}}}}"#,
            owner_id, f, json_escape(c)
        ),
        None => format!(
            r#"{{"FunctionInfo":{{"owner_id":{},"function":"{}","corrected_function":null}}}}"#,
            owner_id, f
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
pub fn split_fields(body: &str) -> Vec<(String, String)> {
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
/// ⚠️ 本轮已把最后 10 个用户（`LockInfo` … `BanConnectionRequest`）升级成强类型
/// ——`serialize_field` 的调用序把字段名与键序都定死了，不再需要透传。
/// 宏本身保留：将来若又出现字段名读不到的事件，它就是正确的降级形态。
#[allow(unused_macros)]
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

// ---------- 线格式规格表：74 个事件的「名字 + 字段序列 + 值类型」----------
//
// ## 这张表是怎么来的（不是猜的）
//
// `#[derive(Serialize)]` 生成的代码**就是**线格式的定义：
//
// ```text
// serialize_struct("XxxEvent", N)   -> 名字只进 serde 的错误串，JSON 里不出现
// serialize_field("字段名", 值)      -> 字段名 = .rodata 里的 &'static str
// ```
//
// `serde_json` 把 `serialize_struct` 的名字丢掉，但 `serialize_field` 的键**逐个写进
// 报文**，而且**调用顺序就是线上键序**。LLVM 把 serde 的 `Compound` 全内联了，于是
// 一次 `serialize_field` 在代码里是三条连续指令：
//
// ```text
// lea  rsi, [rip + 字段名]   ; &'static str.ptr
// mov  edx, LEN              ; &'static str.len（== 该字面量的真实字节数）
// call <serialize_field 单态>
// ```
//
// ⇒ **按地址顺序收 `lea rsi` 那一串，就是线上键序**。复现脚本：`tools/wire_fields.py`
// （`--json` 机器读，`--verify` 自校验）。
//
// 值类型来自 `call` 的目标地址：**同一个单态 = 同一个 Rust 类型**。已认出的单态：
//
// | 单态 | 类型 | 判据 |
// |---|---|---|
// | `0x3cc060` | `i64` | 抓包里是整数 |
// | `0x3cb250` | `String` | 抓包里是字符串 |
// | `0x3cb760` | `f64` | 抓包里是小数 |
// | `0x3cbb80` | `bool` | 该单态体内写 `true` / `false`（观测） |
// | `0x3cc4b0` | `Option<i64>` | 该单态体内写 `null`，且 `rank` 抓到过 `null` |
// | `0x3cb440` | `Option<String>` | 该单态体内写 `null` |
// | `0x3cc330` | `u8` | 与 `GraphwarColor.r/g/b` 同一个单态 |
// | `0x3cbab0` | socket 地址（线上是 JSON 字符串） | `GameInfo.address` 与 `GameRemoved.game_address` 同一单态 |
// | `0x3cb330` / `0x3cb560` / `0x3cb670` / `0x3cbce0` / `0x3cbf70` / `0x3cbe80` / `0x3cc260` | 枚举（线上是字符串） | 单态体内 `{"EffectType" / "AxisMode" / …}` 的变体名表 |
// | `0x3cb9e0` | [`GraphwarColor`] | 结构体 |
// | `0x3cbdb0` | [`GameInfo`] | 结构体 |
//
// ## 与抓包、与 serde 的 `N` 对得上（三道交叉验证）
//
// 1. **`N`**：字段个数与 `.rodata` 里 `struct <Name>Event with N elements` 的 N
//    逐个相等（67/67）；剩下 7 个事件没有这条串，它们的体实测是空 `{}`。
// 2. **反序列化侧**：每个事件 `Deserialize` 的 `visit_map` 里都能找到同名键的字节比较
//    （67/67 全部字段被比到）。注意它常用**合并后的 16 字节窗口**，所以字面量地址与
//    `Serialize` 侧那份不是同一处 —— 这是正常的（`.rodata` 会去重/合并）。
// 3. **抓包**：`fixtures/protocol*.jsonl` 里 49 种事件的键序与这张表**逐个相同**
//    （含嵌套的 `color` / `game_info`）。见 `tests/wire_format.rs`。
//
// ⚠️ **判别值（tag）不上线**：线上是 serde 外部标签 JSON，`{"事件名":{…}}` 里的键是
// **名字**，不是数字。tag 列只用于和 `src/events.rs` 对账（`docs/07` §7）。
// `None` 的两个（`GameHeartbeat` / `VersionInfo`）正是 `docs/07` §7.3 里
// **63 / 65 缺号**的候选——它们有 `Serialize` 实现、有线上名字，但没有已知判别值。

/// 一个字段在线上能取到的值形状。
///
/// 由 `serialize_field` 的**单态地址**定（同单态 = 同类型），见本节开头的表。
/// 名字带 `enum` 语义的（`axis_mode` 等）线上就是字符串，所以统一记 [`ValType::Str`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    /// `i64`（`serialize_u64`/`i64` 一族，单态 `0x3cc060`）。
    I64,
    /// `u8`（单态 `0x3cc330`，只出现在颜色的 `r`/`g`/`b`）。
    U8,
    /// `f64`（单态 `0x3cb760`）。
    F64,
    /// `bool`（单态 `0x3cbb80`）。
    Bool,
    /// 字符串；枚举也走这条（线上就是字符串）。
    Str,
    /// `Option<i64>`（单态 `0x3cc4b0`，可写 `null`）。
    OptI64,
    /// `Option<String>`（单态 `0x3cb440`，可写 `null`）。
    OptStr,
    /// [`GraphwarColor`]（`{"r":..,"g":..,"b":..}`）。
    Color,
    /// [`GameInfo`]（9 个字段，与 LAN 广播的负载同一个结构体）。
    GameInfo,
}

/// 一个线上事件的完整形状。
#[derive(Debug, Clone, Copy)]
pub struct WireEvent {
    /// 线上键名（`{"<name>": …}` 的那个 `<name>`）。
    pub name: &'static str,
    /// `enum Event` 的判别值（**不上线**）；`None` = 判别值未解出。
    pub tag: Option<u16>,
    /// 字段，**按线上顺序**。
    pub fields: &'static [(&'static str, ValType)],
}

/// 74 个事件的线上形状，按事件名升序。**这是对外协议的唯一权威表。**
///
/// 「74」而不是「73」：`docs/07` 早先记的 73 是按 `src/events.rs` 的 72 个
/// **有判别值的**变体 + `GameHeartbeat` 数的，漏了 [`VersionInfo`]——它同样有
/// `Serialize` 实现和线上名字（`{"VersionInfo":{"version":..,"system":..}}`），
/// 只是判别值未知（63 或 65）。本轮从二进制里数出来是 **74**。
pub const WIRE_SPEC: &[WireEvent] = &[
    WireEvent { name: "AddObstacle", tag: Some(40), fields: &[("pos_x", ValType::F64), ("pos_y", ValType::F64), ("radius", ValType::F64)] },
    WireEvent { name: "AddSoldierRequest", tag: Some(16), fields: &[("player_id", ValType::I64)] },
    WireEvent { name: "AxisModeChangeRequest", tag: Some(18), fields: &[] },
    WireEvent { name: "AxisModeInfo", tag: Some(45), fields: &[("axis_mode", ValType::Str)] },
    WireEvent { name: "BanConnectionRequest", tag: Some(72), fields: &[("player_connection_id", ValType::I64), ("leader_connection_id", ValType::I64)] },
    WireEvent { name: "BotAdded", tag: Some(10), fields: &[("connection_id", ValType::I64), ("player_id", ValType::I64), ("level", ValType::I64)] },
    WireEvent { name: "ChangeTeamRequest", tag: Some(11), fields: &[("player_id", ValType::I64)] },
    WireEvent { name: "ChatMessage", tag: Some(71), fields: &[("entity_id", ValType::I64), ("message", ValType::Str)] },
    WireEvent { name: "ChatMessageRequest", tag: Some(70), fields: &[("entity_id", ValType::I64), ("message", ValType::Str)] },
    WireEvent { name: "ClearBansRequest", tag: Some(73), fields: &[] },
    WireEvent { name: "ClearObstacles", tag: Some(39), fields: &[] },
    WireEvent { name: "ColorInfo", tag: Some(61), fields: &[("entity_id", ValType::I64), ("color", ValType::Color)] },
    WireEvent { name: "ConnectedToServer", tag: Some(7), fields: &[("connection_id", ValType::I64)] },
    WireEvent { name: "ConnectionRemoved", tag: Some(6), fields: &[("connection_id", ValType::I64), ("reason", ValType::OptStr)] },
    WireEvent { name: "DestroyObstacle", tag: Some(41), fields: &[("pos_x", ValType::F64), ("pos_y", ValType::F64), ("radius", ValType::F64)] },
    WireEvent { name: "EffectInfo", tag: Some(44), fields: &[("effect_id", ValType::I64), ("pos_x", ValType::F64), ("pos_y", ValType::F64), ("effect_type", ValType::Str), ("start_tick", ValType::I64)] },
    WireEvent { name: "EndOfTurnInfo", tag: Some(49), fields: &[("end_of_turn_tick", ValType::I64), ("time_mode", ValType::Str)] },
    WireEvent { name: "EntityEffectInfo", tag: Some(56), fields: &[("entity_id", ValType::I64), ("effect_type", ValType::Str)] },
    WireEvent { name: "EntityRemoved", tag: Some(14), fields: &[("entity_id", ValType::I64)] },
    WireEvent { name: "FaceGraphicsInfo", tag: Some(53), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "FaceGraphicsRequest", tag: Some(52), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "FunctionActive", tag: Some(36), fields: &[("owner_id", ValType::I64), ("active_tick", ValType::I64)] },
    WireEvent { name: "FunctionFire", tag: Some(33), fields: &[("soldier_id", ValType::I64)] },
    WireEvent { name: "FunctionFireRequest", tag: Some(32), fields: &[("player_id", ValType::I64)] },
    WireEvent { name: "FunctionInfo", tag: Some(35), fields: &[("owner_id", ValType::I64), ("function", ValType::Str), ("corrected_function", ValType::OptStr)] },
    WireEvent { name: "FunctionModeChangeRequest", tag: Some(19), fields: &[] },
    WireEvent { name: "FunctionModeInfo", tag: Some(46), fields: &[("function_mode", ValType::Str)] },
    WireEvent { name: "FunctionPoints", tag: Some(37), fields: &[("owner_id", ValType::I64), ("start_x", ValType::F64), ("start_y", ValType::F64), ("end_x", ValType::F64)] },
    WireEvent { name: "FunctionUpdateRequest", tag: Some(34), fields: &[("player_id", ValType::I64), ("function", ValType::Str)] },
    WireEvent { name: "FunctionsCalculated", tag: Some(38), fields: &[] },
    WireEvent { name: "GameHeartbeat", tag: None, fields: &[] },
    WireEvent { name: "GameInfo", tag: Some(62), fields: &[("game_info", ValType::GameInfo)] },
    WireEvent { name: "GameNameInfo", tag: Some(29), fields: &[("game_name", ValType::Str)] },
    WireEvent { name: "GameNameRequest", tag: Some(22), fields: &[("game_name", ValType::Str)] },
    WireEvent { name: "GameRemoved", tag: Some(64), fields: &[("game_address", ValType::Str)] },
    WireEvent { name: "GameStartRequest", tag: Some(31), fields: &[] },
    WireEvent { name: "GameStateInfo", tag: Some(30), fields: &[("game_state", ValType::Str)] },
    WireEvent { name: "HatGraphicsInfo", tag: Some(55), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "HatGraphicsRequest", tag: Some(54), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "LifeInfo", tag: Some(27), fields: &[("entity_id", ValType::I64), ("alive", ValType::Bool)] },
    WireEvent { name: "LifeQueuedDeath", tag: Some(28), fields: &[("entity_id", ValType::I64), ("tick", ValType::I64)] },
    WireEvent { name: "LockInfo", tag: Some(69), fields: &[("lock", ValType::Bool)] },
    WireEvent { name: "LockRequest", tag: Some(68), fields: &[("lock", ValType::Bool)] },
    WireEvent { name: "NameInfo", tag: Some(60), fields: &[("entity_id", ValType::I64), ("name", ValType::Str)] },
    WireEvent { name: "NameRequest", tag: Some(59), fields: &[("entity_id", ValType::I64), ("name", ValType::Str)] },
    WireEvent { name: "NewBotRequest", tag: Some(9), fields: &[("connection_id", ValType::I64), ("level", ValType::I64)] },
    WireEvent { name: "NewConnection", tag: Some(5), fields: &[("connection_id", ValType::I64)] },
    WireEvent { name: "NewConnectionRequest", tag: Some(4), fields: &[("major_version", ValType::Str), ("minor_version", ValType::Str)] },
    WireEvent { name: "NewPlayerRequest", tag: Some(8), fields: &[("connection_id", ValType::I64)] },
    WireEvent { name: "PlayerInfo", tag: Some(13), fields: &[("player_id", ValType::I64), ("connection_id", ValType::I64)] },
    WireEvent { name: "PosInfo", tag: Some(26), fields: &[("entity_id", ValType::I64), ("pos_x", ValType::F64), ("pos_y", ValType::F64), ("radius", ValType::F64)] },
    WireEvent { name: "QueueDestroyObstacle", tag: Some(42), fields: &[("pos_x", ValType::F64), ("pos_y", ValType::F64), ("radius", ValType::F64), ("tick", ValType::I64)] },
    WireEvent { name: "RankInfo", tag: Some(67), fields: &[("entity_id", ValType::I64), ("rank", ValType::OptI64)] },
    WireEvent { name: "RankRequest", tag: Some(66), fields: &[("entity_id", ValType::I64), ("rank", ValType::OptI64)] },
    WireEvent { name: "RemovePlayerRequest", tag: Some(12), fields: &[("player_id", ValType::I64)] },
    WireEvent { name: "RemoveSoldierRequest", tag: Some(17), fields: &[("player_id", ValType::I64)] },
    WireEvent { name: "RemoveTeam", tag: Some(25), fields: &[("entity_id", ValType::I64)] },
    WireEvent { name: "SkinGraphicsInfo", tag: Some(51), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "SkinGraphicsRequest", tag: Some(50), fields: &[("entity_id", ValType::I64), ("graphics_str", ValType::Str)] },
    WireEvent { name: "SoldierInfo", tag: Some(15), fields: &[("soldier_id", ValType::I64), ("player_id", ValType::I64)] },
    WireEvent { name: "Subscribe", tag: Some(3), fields: &[("subscriber_id", ValType::I64), ("subscribed_event_type", ValType::Str)] },
    WireEvent { name: "TargetInfo", tag: Some(23), fields: &[("target_id", ValType::I64)] },
    WireEvent { name: "TeamInfo", tag: Some(24), fields: &[("entity_id", ValType::I64), ("team", ValType::Str)] },
    WireEvent { name: "Tick", tag: Some(0), fields: &[("current_tick", ValType::I64)] },
    WireEvent { name: "TickReply", tag: Some(2), fields: &[("current_tick", ValType::I64)] },
    WireEvent { name: "TickRequest", tag: Some(1), fields: &[] },
    WireEvent { name: "TimeModeChangeRequest", tag: Some(21), fields: &[] },
    WireEvent { name: "TimeModeInfo", tag: Some(48), fields: &[("time_mode", ValType::Str)] },
    WireEvent { name: "TurnCountInfo", tag: Some(58), fields: &[("turn_count", ValType::I64)] },
    WireEvent { name: "TurnInfo", tag: Some(43), fields: &[("player_id", ValType::I64), ("soldier_id", ValType::I64)] },
    WireEvent { name: "TurnLimitInfo", tag: Some(57), fields: &[("turn_limit", ValType::OptI64)] },
    WireEvent { name: "TurnModeChangeRequest", tag: Some(20), fields: &[] },
    WireEvent { name: "TurnModeInfo", tag: Some(47), fields: &[("turn_mode", ValType::Str)] },
    WireEvent { name: "VersionInfo", tag: None, fields: &[("version", ValType::Str), ("system", ValType::Str)] },
];

/// 按线上键名查规格。
pub fn wire_spec(name: &str) -> Option<&'static WireEvent> {
    WIRE_SPEC.iter().find(|e| e.name == name)
}

/// 事件名的线上字段顺序（`None` = 不在表里）。
pub fn wire_field_order(name: &str) -> Option<Vec<&'static str>> {
    wire_spec(name).map(|e| e.fields.iter().map(|(k, _)| *k).collect())
}

// ---------- 两个负载结构体 ----------

/// `{"r":149,"g":5,"b":62}` —— 颜色。`ColorInfo.color` 的值就是这个结构体。
///
/// **观测**：`tools/wire_fields.py --struct 0x464c00` 读出三个键依次是 `r`、`g`、`b`；
/// 三者的 `serialize_field` 单态都是 `0x3cc330`（`u8`），与抓包里的
/// `{"r":149,"g":5,"b":62}` 一致。⚠️ `docs/07` §3.2 记的字段名 `rgb` 是**错的**
/// （`.rodata` 里确实有 `rgb` 这三个连续字节，但那只是 `r`/`g`/`b` 三个 1 字节
/// 字面量在内存里紧挨着，`strings` 会把它读成一条）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphwarColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl GraphwarColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `{"r":..,"g":..,"b":..}`（键序 = 二进制里的 `serialize_field` 顺序）。
    pub fn to_json(&self) -> String {
        format!(r#"{{"r":{},"g":{},"b":{}}}"#, self.r, self.g, self.b)
    }

    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(Self {
            r: get_int(m, "r")? as u8,
            g: get_int(m, "g")? as u8,
            b: get_int(m, "b")? as u8,
        })
    }
}

/// LAN 广播 / `GameInfoEvent` 的负载：房间信息，**9 个字段**。
///
/// **观测**：`tools/wire_fields.py --struct 0x5060d0` 读出键序
/// `room_name, game_state, current_player_count, address, axis_mode,
/// function_mode, turn_mode, time_mode, locked`，与 `struct GameInfo with 9 elements`
/// 的 N 相等，也与真实 LAN 广播报文完全一致（`docs/07` §1、`protocol.rs` 模块文档）。
///
/// ⚠️ 注意与 [`LockInfo`]/[`LockRequest`] 的字段名**不同**：这里是 `locked`，
/// 那两条是 `lock`。两个名字都在 `.rodata` 里、都实测存在，别互相套用。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GameInfo {
    pub room_name: String,
    pub game_state: String,
    pub current_player_count: i64,
    /// socket 地址（`"[::1]:35245"`）。线上是 JSON 字符串；
    /// 与 `GameRemoved.game_address` 同一个 `serialize_field` 单态 `0x3cbab0`。
    pub address: String,
    pub axis_mode: String,
    pub function_mode: String,
    pub turn_mode: String,
    pub time_mode: String,
    pub locked: bool,
}

impl GameInfo {
    /// 键序 = 二进制里的 `serialize_field` 顺序（9 个，`slot+?` 见文档）。
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                r#"{{"room_name":{},"game_state":{},"current_player_count":{},"#,
                r#""address":{},"axis_mode":{},"function_mode":{},"#,
                r#""turn_mode":{},"time_mode":{},"locked":{}}}"#
            ),
            json_str(&self.room_name),
            json_str(&self.game_state),
            self.current_player_count,
            json_str(&self.address),
            json_str(&self.axis_mode),
            json_str(&self.function_mode),
            json_str(&self.turn_mode),
            json_str(&self.time_mode),
            self.locked,
        )
    }

    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(Self {
            room_name: get_str_unescaped(m, "room_name")?,
            game_state: get_str_unescaped(m, "game_state")?,
            current_player_count: get_int(m, "current_player_count")?,
            address: get_str_unescaped(m, "address")?,
            axis_mode: get_str_unescaped(m, "axis_mode")?,
            function_mode: get_str_unescaped(m, "function_mode")?,
            turn_mode: get_str_unescaped(m, "turn_mode")?,
            time_mode: get_str_unescaped(m, "time_mode")?,
            locked: get_bool(m, "locked")?,
        })
    }
}

// ---------- 强类型 10 个：原先"透传"，本轮从二进制读出字段名与类型 ----------
//
// 这 10 个此前刻意**不声明字段**（`docs/07` §6.2.1：没有抓包就不能猜键名）。
// 本轮改用 `serialize_field` 的**调用序**取证 —— 它比抓包更硬：抓包只能证明
// "见过这种顺序"，而调用序是**产出报文的代码本身**。⇒ 全部升级成强类型。
//
// 每个结构体仍保留 `extra`：将来对端多发字段也不会丢（这是原透传实现的唯一优点，
// 原样保留）。序列化时先按二进制键序发已知字段，再把 `extra` 接在后面。

/// 单布尔字段事件。`lock` 的单态 `0x3cbb80` 体内写 `true`/`false`，故是 `bool`。
macro_rules! bool_field_event {
    ($(#[$meta:meta])* $ty:ident, $field:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            pub $field: bool,
            /// 未识别字段（键 / 原始值文本），按出现顺序保留并原样回发。
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    $field: get_bool(m, stringify!($field))?,
                    extra: extra_fields(&m.body, &[stringify!($field)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[(stringify!($field), self.$field.to_string())],
                    &self.extra,
                )
            }
        }
    };
}

/// 单 `i64` 字段事件。
#[allow(unused_macros)]   // 通用形态，本轮 10 个里没有恰好"单 i64"的事件
macro_rules! i64_field_event {
    ($(#[$meta:meta])* $ty:ident, $field:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            pub $field: i64,
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    $field: get_int(m, stringify!($field))?,
                    extra: extra_fields(&m.body, &[stringify!($field)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[(stringify!($field), json_i64(self.$field))],
                    &self.extra,
                )
            }
        }
    };
}

/// 单 `Option<i64>` 字段事件（可写 `null`）。
macro_rules! opt_i64_field_event {
    ($(#[$meta:meta])* $ty:ident, $field:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            pub $field: Option<i64>,
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    $field: get_int(m, stringify!($field)),
                    extra: extra_fields(&m.body, &[stringify!($field)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[(stringify!($field), json_opt_i64(self.$field))],
                    &self.extra,
                )
            }
        }
    };
}

/// 单字符串字段事件。
macro_rules! str_field_event {
    ($(#[$meta:meta])* $ty:ident, $field:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            pub $field: String,
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    $field: get_str_unescaped(m, stringify!($field))?,
                    extra: extra_fields(&m.body, &[stringify!($field)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[(stringify!($field), json_str(&self.$field))],
                    &self.extra,
                )
            }
        }
    };
}

/// 两个 `i64` 字段事件。
macro_rules! i64_pair_event {
    ($(#[$meta:meta])* $ty:ident, $a:ident, $b:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
            pub $a: i64,
            pub $b: i64,
            pub extra: Vec<(String, String)>,
        }
        impl $ty {
            pub fn parse(m: &RawMessage) -> Option<Self> {
                Some(Self {
                    $a: get_int(m, stringify!($a))?,
                    $b: get_int(m, stringify!($b))?,
                    extra: extra_fields(&m.body, &[stringify!($a), stringify!($b)]),
                })
            }
            pub fn to_msg(&self) -> String {
                emit_object(
                    stringify!($ty),
                    &[(stringify!($a), json_i64(self.$a)),
                      (stringify!($b), json_i64(self.$b))],
                    &self.extra,
                )
            }
        }
    };
}

/// 空体事件：二进制里的 `Serialize` 不写任何字段 ⇒ 体恒为 `{}`。
///
/// 仍然保留 `extra`：万一更高版本加了字段，解析端不会丢、转发端能原样回发。
macro_rules! empty_body_event {
    ($(#[$meta:meta])* $ty:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $ty {
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

bool_field_event! {
    /// `{"LockInfo":{"lock":true}}` —— 房间是否已锁。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x50ca30`（Ghidra `0x60ca30`）；`serialize_struct`
    /// 后只调一次 `serialize_field("lock", ..)`（字段名字面量在 `0x1ba58c`，4 字节，
    /// 紧邻它的是 `mov edx,4`）。值类型 = 单态 `0x3cbb80`（`bool`）。
    /// N=1 与 `struct LockInfoEvent with 1 element` 相等。
    ///
    /// ⚠️ 字段名是 **`lock`**，不是 `locked`。`locked` 是 [`GameInfo`] 的字段
    /// （`LockInfo` 与它不是同一个结构体，别互相套用）。
    LockInfo, lock
}

bool_field_event! {
    /// `{"LockRequest":{"lock":true}}` —— 请求锁房/解锁。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x50e420`；字段名 `lock`（与 [`LockInfo`] 同名字面量），
    /// 值类型同单态 `0x3cbb80`。N=1。
    LockRequest, lock
}

entity_str_event! {
    /// `{"ChatMessage":{"entity_id":229,"message":"gg"}}` —— 聊天广播。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x511320`；两次 `serialize_field` 依次
    /// `entity_id`（单态 `0x3cc060` = `i64`）、`message`（单态 `0x3cb250` = `String`）。
    /// N=2 与 `struct ChatMessageEvent with 2 elements` 相等。
    /// `docs/07` §6.2 只说"`message` 有相邻串证据、第二个字段名未知" —— 本轮把
    /// `entity_id` 也读出来了。
    ChatMessage, message
}

entity_str_event! {
    /// `{"ChatMessageRequest":{"entity_id":232,"message":"hi"}}` —— 发送聊天。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x5124e0`；键序与类型同 [`ChatMessage`]。N=2。
    ChatMessageRequest, message
}

empty_body_event! {
    /// `{"GameHeartbeat":{}}` —— 心跳/保活。
    ///
    /// **观测（升级自"未知"）**：`Serialize` 在 `0x510a90`；`serialize_struct` 之后
    /// **一次 `serialize_field` 都没有**，直接写 `}`。⇒ 体恒为空 `{}`。
    /// 它没有 `struct GameHeartbeatEvent with N elements` 串，
    /// 而空体结构体本来就不需要 `invalid_length` 文案 —— 这与 N=0 自洽。
    /// 判别值仍未知（`docs/07` §7.3 的 63/65 缺号之一）。
    GameHeartbeat
}

str_field_event! {
    /// `{"GameRemoved":{"game_address":"69.164.213.144:35981"}}` —— 房间从列表里消失。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x511de0`；单次 `serialize_field("game_address", ..)`，
    /// 单态 `0x3cbab0` —— **与 `GameInfo.address` 是同一个单态**（观测），
    /// 也就是同一个 socket 地址类型，线上是 JSON 字符串。N=1。
    GameRemoved, game_address
}

opt_i64_field_event! {
    /// `{"TurnLimitInfo":{"turn_limit":20}}` —— 回合数上限。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x50f5d0`；单次 `serialize_field("turn_limit", ..)`，
    /// 单态 `0x3cc4b0` —— **与 `RankInfo.rank` 同一个单态**，也就是 `Option<i64>`
    /// （该单态体内写 `null`，且 `rank` 抓到过 `null`）。N=1。
    TurnLimitInfo, turn_limit
}

empty_body_event! {
    /// `{"ClearObstacles":{}}` —— 清场。
    ///
    /// **观测**：`Serialize` 在 `0x50e7c0`，没有 `serialize_field` ⇒ 体恒为 `{}`
    /// （与真实报文一致）。N 串缺失，同样因为空体。
    ClearObstacles
}

empty_body_event! {
    /// `{"ClearBansRequest":{}}` —— 清空封禁列表。
    ///
    /// **观测（升级自"未知"）**：`Serialize` 在 `0x5137c0`，无 `serialize_field`
    /// ⇒ 体恒为 `{}`。N 串缺失同样因为空体。
    ClearBansRequest
}

i64_pair_event! {
    /// `{"BanConnectionRequest":{"player_connection_id":228,"leader_connection_id":229}}` —— 踢人。
    ///
    /// **逐位/观测**：`Serialize` 在 `0x513cd0`；两次 `serialize_field` 依次
    /// `player_connection_id`、`leader_connection_id`，两者单态都是 `0x3cc060`（`i64`）。
    /// N=2。`docs/07` §6.2 的"候选"现在坐实，且**顺序**也定死了。
    BanConnectionRequest, player_connection_id, leader_connection_id
}

// ---------- 10 个透传事件的 `emit_*`（从零构造报文）----------
//
// 有了字段名与键序就能**主动构造**，不再只能解析转发。键序一律 = 二进制键序。

/// `{"LockInfo":{"lock":true}}`
pub fn emit_lock_info(lock: bool) -> String {
    format!(r#"{{"LockInfo":{{"lock":{}}}}}"#, lock)
}

/// `{"LockRequest":{"lock":true}}`
pub fn emit_lock_request(lock: bool) -> String {
    format!(r#"{{"LockRequest":{{"lock":{}}}}}"#, lock)
}

/// `{"ChatMessage":{"entity_id":229,"message":"gg"}}`
pub fn emit_chat_message(entity_id: i64, message: &str) -> String {
    format!(
        r#"{{"ChatMessage":{{"entity_id":{},"message":"{}"}}}}"#,
        entity_id, json_escape(message)
    )
}

/// `{"ChatMessageRequest":{"entity_id":232,"message":"hi"}}`
pub fn emit_chat_message_request(entity_id: i64, message: &str) -> String {
    format!(
        r#"{{"ChatMessageRequest":{{"entity_id":{},"message":"{}"}}}}"#,
        entity_id, json_escape(message)
    )
}

/// `{"GameHeartbeat":{}}`
pub fn emit_game_heartbeat() -> String {
    r#"{"GameHeartbeat":{}}"#.to_string()
}

/// `{"GameRemoved":{"game_address":"69.164.213.144:35981"}}`
pub fn emit_game_removed(game_address: &str) -> String {
    format!(
        r#"{{"GameRemoved":{{"game_address":"{}"}}}}"#,
        json_escape(game_address)
    )
}

/// `{"TurnLimitInfo":{"turn_limit":20}}`（`None` → `null`）
pub fn emit_turn_limit_info(turn_limit: Option<i64>) -> String {
    format!(
        r#"{{"TurnLimitInfo":{{"turn_limit":{}}}}}"#,
        json_opt_i64(turn_limit)
    )
}

/// `{"ClearObstacles":{}}`
pub fn emit_clear_obstacles() -> String {
    r#"{"ClearObstacles":{}}"#.to_string()
}

/// `{"ClearBansRequest":{}}`
pub fn emit_clear_bans_request() -> String {
    r#"{"ClearBansRequest":{}}"#.to_string()
}

/// `{"BanConnectionRequest":{"player_connection_id":228,"leader_connection_id":229}}`
pub fn emit_ban_connection_request(player_connection_id: i64, leader_connection_id: i64) -> String {
    format!(
        concat!(
            r#"{{"BanConnectionRequest":{{"player_connection_id":{},"#,
            r#""leader_connection_id":{}}}}}"#
        ),
        player_connection_id, leader_connection_id
    )
}

/// `{"GameInfo":{"game_info":{…9 个字段…}}}`
pub fn emit_game_info(info: &GameInfo) -> String {
    format!(r#"{{"GameInfo":{{"game_info":{}}}}}"#, info.to_json())
}

/// `{"ColorInfo":{"entity_id":..,"color":{"r":..,"g":..,"b":..}}}`
pub fn emit_color_info(entity_id: i64, color: GraphwarColor) -> String {
    format!(
        r#"{{"ColorInfo":{{"entity_id":{},"color":{}}}}}"#,
        entity_id,
        color.to_json()
    )
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
        ("LockInfo", r#"{"LockInfo":{"lock":true}}"#),                         // 合成（键名本轮由二进制读出）
        ("LockRequest", r#"{"LockRequest":{"lock":true}}"#),                   // 合成（同上）
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

    /// 原先"透传"的那 10 个：本轮已按二进制的 `serialize_field` 调用序升级成强类型。
    ///
    /// ⚠️ 本测试**替换**了旧断言 `passthrough_events_declare_no_field_names`
    /// （旧断言是"这 10 个不声明任何已知字段，全部落进 `extra`"）。旧断言的前提是
    /// "没有抓包就不能猜键名"；本轮拿到了**比抓包更硬的证据**——产出报文的代码本身
    /// （`serialize_field` 的调用序 + 单态类型），前提已不成立。
    /// 这是本轮**唯一**改动的既有断言，且改后更强：从"不许有字段"变成
    /// "字段名与键序必须逐字等于从二进制读出的值"。
    #[test]
    fn upgraded_events_declare_the_binary_field_names() {
        let upgraded: Vec<&str> = SUPPLEMENTED_EVENT_NAMES
            .iter()
            .copied()
            .filter(|k| !CAPTURED_TYPED_EVENTS.contains(k))
            .collect();
        assert_eq!(upgraded.len(), 10);
        for kind in upgraded {
            let line = sample_of(kind);
            let m = parse_message(line).expect("样本应能解析");
            let ev = AnyEvent::parse(&m).expect("应识别");
            assert!(
                ev.extra_fields().is_empty(),
                "{kind}: 已知字段应全部进强类型槽位，extra 必须为空"
            );
            let order = wire_field_order(kind)
                .unwrap_or_else(|| panic!("{kind} 不在 WIRE_SPEC 里"));
            let out = ev.to_msg();
            assert_eq!(out, line, "{kind} 往返后字节不一致");
            let m2 = parse_message(&out).expect("重新解析");
            let got: Vec<String> =
                split_fields(&m2.body).into_iter().map(|(k, _)| k).collect();
            let want: Vec<String> = order.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, want, "{kind} 的线上键序与 WIRE_SPEC 不一致");
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
            include_str!("../../fixtures/protocol.jsonl"),
            include_str!("../../fixtures/protocol_match.jsonl"),
            include_str!("../../fixtures/protocol_death.jsonl"),
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

    /// **转义与字符串内的逗号**必须一字节不动地保留。
    ///
    /// ⚠️ 本测试原先用 `ChatMessage` 走"透传"路径断言 `extra` 里有两个字段；
    /// `ChatMessage` 本轮升级成强类型后，`message` 落在强类型槽位里。
    /// 断言意图（转义 / 串内逗号不能被当成分隔符）保持不变并**加强**：
    /// ① 强类型字段能解转义读回、再逐字节写回；② 真正未知的字段仍然原样进 `extra`。
    #[test]
    fn passthrough_keeps_escaped_strings_verbatim() {
        let line = r#"{"ChatMessage":{"entity_id":229,"message":"he said \"hi\", ok"}}"#;
        let m = parse_message(line).expect("含转义的帧应能解析");
        let AnyEvent::ChatMessage(chat) = AnyEvent::parse(&m).expect("应识别") else {
            panic!("应解析为 ChatMessage")
        };
        // ① 强类型字段：解转义读回，且能逐字节写回（`json_escape` 只转义 `"` 与 `\`）
        assert_eq!(chat.entity_id, 229);
        assert_eq!(chat.message, r#"he said "hi", ok"#);
        assert!(chat.extra.is_empty(), "已知字段不该再落进 extra");
        assert_eq!(chat.to_msg(), line, "转义与串内逗号没有一字节不动地写回");

        // ② 未知字段（含转义与逗号）仍然原样保留
        let line2 = r#"{"ChatMessage":{"entity_id":1,"message":"a","note":"x \"y\", z"}}"#;
        let m2 = parse_message(line2).unwrap();
        let AnyEvent::ChatMessage(chat2) = AnyEvent::parse(&m2).expect("应识别") else {
            panic!("应解析为 ChatMessage")
        };
        assert_eq!(chat2.extra, vec![("note".to_string(), r#""x \"y\", z""#.to_string())]);
        assert_eq!(chat2.to_msg(), line2);
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
