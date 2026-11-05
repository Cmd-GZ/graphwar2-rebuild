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
