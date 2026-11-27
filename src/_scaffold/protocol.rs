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

/// **严格版** [`parse_message`]：线上帧的解析入口。
///
/// 判据（与原版 serde 外部标签同口径）：整段文本必须是**恰好一个**顶层键、它的值必须是
/// **对象**、尾部只许空白。多一个键 / 值不是对象 / 尾随垃圾 ⇒ `None`。
///
/// 为什么要两个入口（T76 台账 `fixtures/protocol_parse_audit.tsv` 的 2 号行）：
/// [`parse_message`] 的契约是"**按前缀**取第一个键的对象"—— 回放行是
/// `{"event":{…},"tick":N}`，`graphwar_replay_reader::parse_event_line` 正靠这个前缀语义
/// 取 `event` 再回到原文取 `tick`；把它改严会让**整条回放读取链路**失效。
/// 线上帧没有这个需求（帧本身就是单键 map）⇒ 线上路径（`_scaffold/broker.rs`）用本函数。
pub fn parse_frame_message(text: &str) -> Option<RawMessage> {
    let text = text.trim();
    let inner = match text.strip_prefix('{').and_then(|_| {
        let (v, end) = scan_value_at(text, 0)?;
        if end != text.len() {
            return None;
        }
        Some(&v[1..v.len() - 1])
    }) {
        Some(inner) => inner,
        None => return None,
    };
    let pairs = scan_pairs(inner)?;
    if pairs.len() != 1 {
        return None; // 线上帧 = serde 外部标签的单键 map
    }
    let (kind, value) = pairs[0];
    let body = object_body(value)?;
    Some(RawMessage { kind: kind.to_string(), body: body.to_string(), text: text.to_string() })
}

/// LAN 广播 / `GameInfoEvent` 负载用的类型**别名**：见 [`GameInfo`]。
///
/// 原版**只有一个** `struct GameInfo`（serde 名串 `struct GameInfo with 9 elements`，槽
/// `0x9502f0` → `0x1bc121`）：LAN 广播（裸对象 `{"room_name":…}`）与事件
/// （`{"GameInfo":{"game_info":{…}}}`）用的是**同一个结构体**。
/// ⇒ 本 crate 也只保留**一个** JSON 视图类型，`Broadcast` 只是它的另一个名字
/// （T66 收敛，`docs/06` §111）。为什么没能压到"只剩 1 个结构体定义"：
/// `fixtures/struct_audit_plan3.txt` §2.3 的两条硬约束（`_scaffold/broker.rs` 的 `i64`
/// 测试助手、`tests/wire_format.rs` 的 `"Timers1m"` 错字断言）。
pub use GameInfo as Broadcast;

/// `GameInfo::parse` 的输入形态：**整段 JSON 对象文本**（LAN 广播）或 [`RawMessage`]
/// （事件负载，取它的 `body`）。两种调用点都因此**零改动**：
/// `Broadcast::parse(text)`（广播）与 `GameInfo::parse(&msg)`（事件）。
pub trait JsonObject {
    /// 取"对象文本"（`{…}` 或 `"k":v,…` 的内部体，实现者自己保证）。
    fn json_object_text(&self) -> &str;

    /// 这个输入是**事件负载**还是**线上裸对象原文**？
    ///
    /// 两种形态的严格性不同，所以必须由**输入类型**区分（T76）：
    /// * `&str` / `&String` = LAN 数据报原文。原版受控实验（`docs/archive/115` §5.3）证明
    ///   只有"裸的 9 字段对象"被接受 ⇒ **绝不**下钻，否则变体 B/D 会漏过去；
    /// * `&RawMessage` = 已经由 [`parse_message`] 解出名字的**事件负载**。原版 `GameInfo`
    ///   事件只有一个字段 `game_info`（`WIRE_SPEC`）⇒ 顶层有 `game_info` 才下钻一层。
    fn is_event_payload(&self) -> bool {
        false
    }
}

impl JsonObject for &str {
    fn json_object_text(&self) -> &str {
        self
    }
}

impl JsonObject for &String {
    fn json_object_text(&self) -> &str {
        self.as_str()
    }
}

impl JsonObject for &RawMessage {
    fn json_object_text(&self) -> &str {
        &self.body
    }

    fn is_event_payload(&self) -> bool {
        true
    }
}

/// 从消息体里取一个字符串字段，例如 `get_str(m, "game_name")`。
///
/// **严格（T76）**：只认 `body` 的**顶层**字段（子串式的 `find` 会让嵌套对象里的同名字段
/// 也命中 ⇒ 变体 B 漏网）；值必须是一个完整的 JSON 字符串字面量，返回**内部原文、不解转义**
/// （带转义的值请用 [`get_str_unescaped`]）。
pub fn get_str(msg: &RawMessage, key: &str) -> Option<String> {
    top_str(&msg.body, key)
}

/// 从消息体里取一个整数字段。
///
/// **严格（T76）**：顶层 + JSON 整数文法精确匹配。改前是"取数字前缀" ⇒ `"tick":12abc`
/// 会得到 12、`"tick":1.5` 会得到 1（serde 这两种都是类型错 ⇒ 反序列化失败）。
pub fn get_int(msg: &RawMessage, key: &str) -> Option<i64> {
    top_int(&msg.body, key)
}

/// 取 `{"r":149,"g":5,"b":62}` 这样的颜色字段。
///
/// **严格（T76）**：顶层 + 三个通道都必须是 `u8` 范围（改前 `as u8` 会把 `r:300` 静默
/// 回绕成 44；serde 的 `u8` 越界是反序列化失败）。
pub fn get_color(msg: &RawMessage, key: &str) -> Option<(u8, u8, u8)> {
    top_color(&msg.body, key)
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

// ---------------------------------------------------------------------------
// 严格字段读取（T76）：只认**对象顶层**的字段，值与类型按线上文法精确校验
// ---------------------------------------------------------------------------
//
// 为什么要有这一层（T76 的起点，`docs/06` §118.4 → §120）：
// 改前 6 个 `get_*` 全是 `body.find("\"k\":")` —— 它在**任意深度**都能命中
// （嵌套对象里、甚至别的字符串值内部），于是 `{"GameInfo":{"game_info":{…9 字段…}}}`
// 这种**带事件外壳的整帧**也能被 `Broadcast::parse` "解析成功"（T67 实测：一接上去
// selftest 的变体 B 断言当场变红）。原版丢弃的报文在我们这边"解析成功"= 证据缺口。
//
// ⇒ 字段读取必须是「**顶层 + 精确**」，与 serde 的 `visit_map`（只在自己这一层按名字
// 逐个比字节）同口径。真值依据见 `fixtures/protocol_parse_audit.tsv` 头部：
// `fixtures/protocol_live_frames.jsonl` 的 **1717 条线上真帧**里，**每一帧**的顶层键集
// 与键序都等于 `WIRE_SPEC`（本轮机核 0 mismatch）。

/// 扫一个字符串字面量（`i` 必须指向开头的 `"`），返回 `(内部原文, 闭合引号之后的位置)`。
///
/// 内部原文**不解转义**（保持 [`get_str`] 的旧契约）；`\` + 任意字符按 2 字节跳过，
/// 未闭合 ⇒ `None`。
fn scan_string_at(s: &str, i: usize) -> Option<(&str, usize)> {
    let b = s.as_bytes();
    if i >= b.len() || b[i] != b'"' {
        return None;
    }
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Some((&s[i + 1..j], j + 1)),
            _ => j += 1,
        }
    }
    None
}

/// 扫一个值（对象 / 数组 / 字符串 / 标量原文），返回 `(值原文, 结束位置)`。
///
/// 对象与数组按括号配平（字符串内的括号不算）；标量截到顶层的 `,` / `}` / `]`。
fn scan_value_at(s: &str, i: usize) -> Option<(&str, usize)> {
    let b = s.as_bytes();
    if i >= b.len() {
        return None;
    }
    match b[i] {
        b'"' => {
            let (_, end) = scan_string_at(s, i)?;
            Some((&s[i..end], end))
        }
        b'{' | b'[' => {
            let mut depth = 0i32;
            let mut j = i;
            let mut in_str = false;
            while j < b.len() {
                let c = b[j];
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
                    depth -= 1;
                    if depth == 0 {
                        return Some((&s[i..j + 1], j + 1));
                    }
                    if depth < 0 {
                        return None;
                    }
                }
                j += 1;
            }
            None
        }
        _ => {
            let mut j = i;
            while j < b.len() && b[j] != b',' && b[j] != b'}' && b[j] != b']' {
                j += 1;
            }
            let raw = s[i..j].trim_end();
            if raw.is_empty() {
                return None;
            }
            Some((raw, j))
        }
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i] as char).is_whitespace() {
        i += 1;
    }
    i
}

/// **严格**扫对象体：只接受 `"k":v,"k2":v2,…`；顶层键不许重复（serde 的 `visit_map`
/// 遇重复字段直接报错）、值必须完整、尾部不许有多余字符、不许尾随逗号。
///
/// 与 [`split_fields`] 的分工：那个是**宽容**的（遇畸形即停，`extra` 原样保留靠它），
/// 这个是**严格**的（typed 入口用它，畸形体一律 `None`）。
fn scan_pairs(body: &str) -> Option<Vec<(&str, &str)>> {
    let b = body.as_bytes();
    let mut out: Vec<(&str, &str)> = Vec::new();
    let mut i = skip_ws(b, 0);
    if i == b.len() {
        return Some(out); // `{}` 空体合法
    }
    loop {
        if i >= b.len() || b[i] != b'"' {
            return None;
        }
        let (key, after_key) = scan_string_at(body, i)?;
        if key.contains('\\') {
            return None; // 协议里的键都是纯 ASCII，真帧 0 例（1717 帧机核）
        }
        i = skip_ws(b, after_key);
        if i >= b.len() || b[i] != b':' {
            return None;
        }
        i = skip_ws(b, i + 1);
        let (val, after_val) = scan_value_at(body, i)?;
        if out.iter().any(|(k, _)| *k == key) {
            return None; // serde：重复字段 = 反序列化失败
        }
        out.push((key, val));
        i = skip_ws(b, after_val);
        if i >= b.len() {
            return Some(out);
        }
        if b[i] != b',' {
            return None;
        }
        i = skip_ws(b, i + 1);
        if i >= b.len() {
            return None; // 尾随逗号
        }
    }
}

/// `body` 顶层字段 `key` 的值原文（含引号/括号）；键不在顶层 ⇒ `None`。
///
/// ⚠️ 这是 T76 的**核心**：**不下钻**。嵌套对象里的同名字段一律看不见 ——
/// 这正是"变体 B（事件外壳）不再漏网"的原因。
pub fn top_level_value<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    scan_pairs(body)?.into_iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// 值原文必须是**一个完整的对象字面量**，返回它的内部体（不含两端花括号）。
fn object_body(raw: &str) -> Option<&str> {
    let t = raw.trim();
    if !t.starts_with('{') {
        return None;
    }
    let (v, end) = scan_value_at(t, 0)?;
    if end != t.len() {
        return None;
    }
    Some(&v[1..v.len() - 1])
}

/// 顶层字段 `key` 的对象内部体（不含花括号）。
pub fn top_level_object<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    object_body(top_level_value(body, key)?)
}

/// 变体名闸（T76）：线上事件的键名**就是** serde 外部标签的变体名，`Deserialize` 时
/// serde 先按名字选变体 —— 名字不认识就是 "unknown variant" **解析失败**。
///
/// ⇒ 每个强类型 `parse` 的第一句都必须是它。改前没有这一句：`PosInfo::parse` 之类
/// 拿到**别的**事件的报文也可能"解析成功"（只要字段恰好齐），这是比子串式更外一层
/// 的宽容 —— 名字这一维根本没校验。
fn kind_is(m: &RawMessage, name: &str) -> bool {
    m.kind == name
}

/// 严格版 [`extra_fields`]：体不符合 JSON 对象文法 ⇒ `None`（typed 入口用它，
/// 这样"畸形体"不会因为 `split_fields` 静默截断而变成"少几个 extra 字段"）。
///
/// `known` 里的键不进 `extra`；其余按**出现顺序**原样保留（键 + 值原文），供 [`emit_object`] 回发。
fn strict_extra_fields(body: &str, known: &[&str]) -> Option<Vec<(String, String)>> {
    Some(
        scan_pairs(body)?
            .into_iter()
            .filter(|(k, _)| !known.contains(k))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

/// 值原文必须是**一个完整的字符串字面量**，返回内部原文（不解转义）。
fn as_string_literal(raw: &str) -> Option<&str> {
    let (inner, end) = scan_string_at(raw, 0)?;
    if end != raw.len() {
        return None;
    }
    Some(inner)
}

/// JSON 整数文法：`-?(0|[1-9][0-9]*)`。serde 的 `i64` 只收这个形状
/// （`12abc` / `1.5` / `+1` / `01` 全失败），改前的"取数字前缀"会把它们悄悄放过。
fn as_i64_literal(raw: &str) -> Option<i64> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    raw.parse().ok()
}

/// JSON 数字文法（`-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`），用作 `f64` 字段。
fn as_f64_literal(raw: &str) -> Option<f64> {
    let b = raw.as_bytes();
    let mut i = 0usize;
    if i < b.len() && b[i] == b'-' {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == int_start {
        return None;
    }
    if b[int_start] == b'0' && i - int_start > 1 {
        return None;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let fs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == fs {
            return None;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let es = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == es {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    raw.parse().ok()
}

/// `u8` 字段：整数文法 + 范围 0..=255（serde 的 `u8` 越界 = 错误，改前 `as u8` 会静默回绕）。
fn as_u8_literal(raw: &str) -> Option<u8> {
    u8::try_from(as_i64_literal(raw)?).ok()
}

/// 顶层字段取值：字符串（**不解转义**，与 [`get_str`] 一致）。
fn top_str(body: &str, key: &str) -> Option<String> {
    Some(as_string_literal(top_level_value(body, key)?)?.to_string())
}

/// 顶层字段取值：字符串（解转义）。
fn top_str_unescaped(body: &str, key: &str) -> Option<String> {
    unescape_json_string(as_string_literal(top_level_value(body, key)?)?)
}

/// 顶层字段取值：整数（JSON 整数文法，精确）。
fn top_int(body: &str, key: &str) -> Option<i64> {
    as_i64_literal(top_level_value(body, key)?)
}

/// 顶层字段取值：浮点（JSON 数字文法，精确）。
fn top_f64(body: &str, key: &str) -> Option<f64> {
    as_f64_literal(top_level_value(body, key)?)
}

/// 顶层字段取值：布尔（只认 `true` / `false` 两个字面量）。
fn top_bool(body: &str, key: &str) -> Option<bool> {
    match top_level_value(body, key)? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// 顶层字段取值：`{"r":..,"g":..,"b":..}`（三个通道都走 [`as_u8_literal`]，越界 ⇒ `None`）。
fn top_color(body: &str, key: &str) -> Option<(u8, u8, u8)> {
    let inner = top_level_object(body, key)?;
    Some((
        as_u8_literal(top_level_value(inner, "r")?)?,
        as_u8_literal(top_level_value(inner, "g")?)?,
        as_u8_literal(top_level_value(inner, "b")?)?,
    ))
}

/// 把字符串字面量的**内部原文**按 JSON 规则解转义。
///
/// 局限（未知）：`\uD83D\uDE00` 这类代理对会解成 U+FFFD（没有样本需要）。
fn unescape_json_string(inner: &str) -> Option<String> {
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.char_indices();
    while let Some((_, c)) = chars.next() {
        match c {
            '"' => return None, // 内部原文里不该出现裸引号（扫描阶段已排除）
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
    Some(out)
}


#[cfg(test)]
mod tests {
    use super::*;

    // ===================================================================
    // T73 真值表：`fixtures/test_expectation_truth4.tsv`
    // （生成器 `tools/gen_test_expectations4.py`；`--check` 可复算）
    //
    // 三段和本文件的测试对应：
    //   `F/*` = 线上真 wire 字节（1717 帧里 1715 帧，逐帧核 sha256/frame_sha256/n）
    //   `L/*` = 线上真帧的字段数与键序（含 `GameInfo` 52 帧的内层 9 字段）
    //   `R/*` = arm64 二进制指令（crash reporter 端口，见 `tests/misc_modules.rs`）
    // ===================================================================
    const TRUTH4: &str = include_str!("../../fixtures/test_expectation_truth4.tsv");

    /// 取一条真值（`section` + `key`）。
    pub(super) fn truth4(section: &str, key: &str) -> &'static str {
        for line in TRUTH4.lines() {
            let mut it = line.split('\t');
            if it.next() == Some(section) && it.next() == Some(key) {
                return it.next().unwrap_or("");
            }
        }
        panic!("truth4 里没有 {section}/{key}")
    }

    pub(super) fn truth4_usize(section: &str, key: &str) -> usize {
        truth4(section, key).parse().unwrap_or_else(|_| panic!("{section}/{key} 不是整数"))
    }

    pub(super) fn truth4_u64(section: &str, key: &str) -> u64 {
        let v = truth4(section, key);
        u64::from_str_radix(v.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("{section}/{key} 不是 hex"))
    }

    /// FNV-1a 64（与 `tools/gen_test_expectations4.py::fnv1a64` 同口径）。
    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// 抓包里的每一帧：`(事件名, n, 负载字节, 是否打码)`，按抓包顺序。meta 行跳过。
    fn live_frames() -> Vec<(String, usize, Vec<u8>, bool)> {
        const JSONL: &str = include_str!("../../fixtures/protocol_live_frames.jsonl");
        let mut out = Vec::new();
        for line in JSONL.lines() {
            // 只用最小 JSON 切片：这个夹具每行都是扁平对象，键固定
            let Some(epos) = line.find("\"event\": \"") else { continue };
            let rest = &line[epos + 10..];
            let Some(eend) = rest.find('"') else { continue };
            let event = &rest[..eend];
            if event == "null" || event.is_empty() {
                continue; // meta 行（`"event": null`）
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

    /// **结构 pin（T66）**：`GameInfo` 在本 crate 里**只有一份定义**，且 9 个字段/键序/严格解析都被钉住。
    ///
    /// 为什么要有这条：原版只有**一个** `struct GameInfo`（`0x5060d0`，9 字段），
    /// 而我方在 T66 之前有 **3 处**形态（`events.rs` / 本文件的 `GameInfo` / 本文件的 `Broadcast`）。
    /// 收敛之后剩 **1 处定义**（`crate::events::GameInfo`，本文件 `pub use` 它、`Broadcast` 是它的别名），
    /// 这条测试把「字段集 + 键序 + serde 两方向」钉死：谁加/删/改名一个字段，这里立刻编不过或测红。
    #[test]
    fn game_info_has_one_definition_with_pinned_fields_and_key_order() {
        use crate::graphwar_game_info::GAME_INFO_FIELDS;

        let info = GameInfo {
            room_name: "Graphwar Game".into(),
            game_state: crate::turnstate::GameState::Setup,
            current_player_count: 2,
            address: "[::1]:35245".into(),
            axis_mode: crate::graphwar_mode_manager::AxisMode::EveryUnit,
            function_mode: crate::graphwar_mode_manager::FunctionMode::NormalFunction,
            turn_mode: crate::graphwar_turn_manager::TurnMode::SequentialTurns,
            time_mode: crate::graphwar_mode_manager::TimeMode::Timer1m,
            locked: false,
        };

        // ① 字段集：`let` 解构要求**恰好**这 9 个字段（多一个 / 少一个 / 改名 ⇒ 编译错误）。
        let GameInfo {
            room_name: _,
            game_state: _,
            current_player_count: _,
            address: _,
            axis_mode: _,
            function_mode: _,
            turn_mode: _,
            time_mode: _,
            locked: _,
        } = info.clone();

        // ② 键序：`to_json` 的键必须等于字段表（同一个顺序 = 二进制 `0x5060d0` 的键序），
        //    **且**必须等于线上 52 帧 `GameInfo` 的**真帧内层键序**（T73 / 计划 R04）：
        //    期望值不再是"我方这张表"，而是抓包字节（`truth4` 的 `L/GameInfo/inner_keyorder`
        //    由 `tools/gen_test_expectations4.py` 从每一帧的负载字节解 JSON 得到）。
        let keys: Vec<String> = split_fields(
            info.to_json().trim_start_matches('{').trim_end_matches('}'),
        )
        .into_iter()
        .map(|(k, _)| k)
        .collect();
        assert_eq!(keys, GAME_INFO_FIELDS.map(str::to_string).to_vec());
        let live_order: Vec<String> = truth4("L", "GameInfo/inner_keyorder")
            .split('|')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        assert_eq!(keys, live_order,
                   "GameInfo 的键序与线上真帧不一致（真值见 fixtures/test_expectation_truth4.tsv）");
        assert_eq!(live_order.len(), GAME_INFO_FIELDS.len(), "真帧内层字段数应等于我方字段表");

        // ③ serde 两个方向：`to_json` → `parse` 必须回到原样（枚举按变体名严格往返）。
        let m = parse_message(&format!("{{\"X\":{}}}", info.to_json())).unwrap();
        assert_eq!(GameInfo::parse(&m).unwrap(), info);

        // ④ 严格性：未知枚举名 ⇒ `None`（与 serde「未知变体 = 反序列化失败」同口径）。
        let bad_json = info.to_json().replace("\"Setup\"", "\"Bogus\"");
        let bad = RawMessage {
            kind: "GameInfo".into(),
            body: bad_json.trim_start_matches('{').trim_end_matches('}').to_string(),
            text: bad_json.clone(),
        };
        assert!(GameInfo::parse(&bad).is_none(), "未知枚举名必须解析失败");
    }

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

    /// 帧格式 = `u16 大端长度前缀 + UTF-8 负载`。
    ///
    /// ## 真值来源（T73 / 计划 R01，`逐位`）
    ///
    /// **不再**只做我方 `encode → decode` 往返（那样编码/解码同时错也恒绿）。真值是
    /// **线上抓到的 wire 字节**：`fixtures/protocol_live_frames.jsonl` 的 1717 帧里
    /// **1715 帧**（2 条打码帧除外）每帧自带 `frame_sha256 = sha256(u16be(n) ‖ payload)`
    /// 与 `sha256 = sha256(payload)`（生成器 `tools/gen_test_expectations4.py::verify_live`
    /// 已逐帧核过这两项）。本测试用**我方 `Frame::encode`** 重建每一帧，把 1715 个完整帧
    /// 按抓包顺序拼起来算 FNV-1a 64，必须等于抓包字节的 FNV（`truth4` 的 `F/wire_fnv`）；
    /// 只要前缀字节序反了、把前缀长度算成负载+2、或写成 u32，FNV 立刻不同。
    ///
    /// 复跑：`.reference/venv/bin/python tools/gen_test_expectations4.py --check`
    #[test]
    fn frame_length_prefix_matches_payload_length() {
        let frames = live_frames();
        let masked = frames.iter().filter(|f| f.3).count();
        assert_eq!(frames.len(), truth4_usize("F", "frames_total"),
                   "抓包帧数与真值表不符（夹具被换过）");
        assert_eq!(masked, truth4_usize("F", "frames_masked"), "打码帧数不符");
        let verified: Vec<&(String, usize, Vec<u8>, bool)> =
            frames.iter().filter(|f| !f.3).collect();
        assert_eq!(verified.len(), truth4_usize("F", "frames_verified"));

        let mut wire: Vec<u8> = Vec::new();
        let mut n_checked = 0usize;
        for (event, n, payload, _) in &verified {
            assert_eq!(payload.len(), *n, "{event}: 抓包 n 与负载字节数不符");
            let payload = std::str::from_utf8(payload).expect("线上负载应是 UTF-8");
            let f = Frame { payload: payload.to_string() };
            let w = f.encode();
            assert_eq!(w.len(), n + 2, "{event}: 帧长应为负载 + 2");
            assert_eq!(&w[..2], &(*n as u16).to_be_bytes(),
                       "{event}: 长度前缀必须是大端 u16 且等于负载字节数（不含前缀自身）");
            // 解码方向：同一份 wire 字节必须解回同一帧
            let (back, used) = Frame::decode(&w).expect("整帧应能解码");
            assert_eq!(back.payload, payload, "{event}: 解码回来的负载变了");
            assert_eq!(used, w.len(), "{event}: 解码消耗字节数应为整帧");
            wire.extend_from_slice(&w);
            n_checked += 1;
        }
        assert_eq!(n_checked, verified.len());
        assert_eq!(wire.len(), truth4_usize("F", "wire_len"),
                   "1715 帧完整 wire 字节总数与抓包不符");
        assert_eq!(fnv1a64(&wire), truth4_u64("F", "wire_fnv"),
                   "1715 帧完整 wire 字节与抓包**逐字节**不一致（前缀口径错了）");
    }

    /// 「半个长度字段 / 负载不完整 ⇒ `None`」是**我方解码器的契约**。
    ///
    /// ⚠️ **契约断言，不含语义**（T73 / 计划 R07）：真流量里不会有半帧，原版
    /// `graphwar_stream_buffer` 的半帧行为没有抓到。等价的**真值侧**覆盖是
    /// `frame_length_prefix_matches_payload_length`：抓包 1715 个整帧的 wire 字节
    /// 我方逐帧重建后 FNV 一致（整帧能解、能编）。**不能把这条当复原证据。**
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

    /// 非 JSON 输入必须拒绝。
    ///
    /// ⚠️ **契约断言，不含语义**（T73 / 计划 R08）：真流量里不会有非法帧（1717 帧真帧
    /// 0 条非法，`fixtures/protocol_live_frames.jsonl` 的 `parse_ok` 全为 true）⇒
    /// 原版的失败路径**没有对应物**。这条只证我方解析器不吞垃圾。
    #[test]
    fn rejects_non_json() {
        assert!(parse_message("not json").is_none());
        assert!(parse_message(r#"{"A":{"x":1}"#).is_none(), "不配对的括号应拒绝");
    }

    /// **T76 真值依据的机器化**：`fixtures/protocol_live_frames.jsonl` 的 **1717 条线上真帧**，
    /// **每一帧**的顶层键集与键序都必须等于 `WIRE_SPEC`（本测试是本轮机核，不是抽查）。
    ///
    /// 为什么这条能当"严格解析"的判据：严格化的方向是「字段只认顶层 + 类型精确」，
    /// 它必须**不丢任何一条真帧**。反过来说，只要 1717 帧全部通过，任何"改严把真帧改红"
    /// 的可能都被排除了。台账 `fixtures/protocol_parse_audit.tsv` 头部引的就是这条。
    #[test]
    fn every_live_frame_has_exactly_the_wire_spec_top_level_fields() {
        const LIVE: &str = include_str!("../../fixtures/protocol_live_frames.jsonl");
        let mut checked = 0usize;
        let mut by_kind: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for line in LIVE.lines() {
            // 抓包记录是 JSON 行；负载字节在 `"hex": "<hex>"` 里（与
            // `supplemented_event_tests::replays_every_live_frame_of_the_supplemented_kinds` 同源）。
            let Some(pos) = line.find("\"hex\": \"") else { continue };
            let rest = &line[pos + 8..];
            let Some(end) = rest.find('"') else { continue };
            let hex = &rest[..end];
            let bytes: Vec<u8> = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("抓包 hex"))
                .collect();
            let text = std::str::from_utf8(&bytes).expect("线上负载应是 UTF-8");
            let m = parse_message(text)
                .unwrap_or_else(|| panic!("线上帧应能解出单键外壳：{text}"));
            let spec = wire_spec(&m.kind)
                .unwrap_or_else(|| panic!("{} 不在 WIRE_SPEC 里（1717 真帧应被 74 条规格覆盖）", m.kind));
            let pairs = scan_pairs(&m.body)
                .unwrap_or_else(|| panic!("{} 的负载不是合法 JSON 对象：{text}", m.kind));
            let got: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
            let want: Vec<&str> = spec.fields.iter().map(|(k, _)| *k).collect();
            assert_eq!(got, want, "{} 的顶层键集/键序与 WIRE_SPEC 不符：{text}", m.kind);
            // 严格读取必须能取到**每一个**字段（顶层可见）
            for (k, _) in spec.fields {
                assert!(
                    top_level_value(&m.body, k).is_some(),
                    "{}.{} 在顶层取不到：{text}",
                    m.kind,
                    k
                );
            }
            *by_kind.entry(m.kind.clone()).or_insert(0usize) += 1;
            checked += 1;
        }
        assert_eq!(checked, 1717, "线上真帧总数应为 1717（fixtures/protocol_live_frames.jsonl）");
        assert_eq!(by_kind.len(), 56, "线上真帧覆盖的事件种数变了");
        assert_eq!(by_kind.get("TickReply"), Some(&504usize), "TickReply 帧数变了");
    }

    /// **反面对照（T76）**：字段只认顶层 —— 嵌套对象里的同名字段、以及别的**字符串值内部**
    /// 出现的 `"key":` 都不许被当成顶层字段。改前这 4 条全都能"解析成功"（substring `find`）。
    #[test]
    fn nested_or_quoted_field_names_are_not_top_level_fields() {
        // ① 嵌套一层对象里放齐所有字段
        let m = parse_message(
            r#"{"PosInfo":{"junk":{"entity_id":231,"pos_x":0.0,"pos_y":0.0,"radius":0.5}}}"#,
        )
        .expect("外壳合法");
        assert!(PosInfo::parse(&m).is_none(), "嵌套对象里的字段不是顶层字段");
        // ② 同名字段出现在**字符串值内部**
        let m = parse_message(
            r#"{"PosInfo":{"note":"x \"entity_id\":231","pos_x":0.0,"pos_y":0.0,"radius":0.5}}"#,
        )
        .expect("外壳合法");
        assert!(PosInfo::parse(&m).is_none(), "字符串值内部的 \"key\": 不是字段");
        // ③ 正对照：真帧的同形状必须照旧解得开（真值 = live1717 里的 PosInfo 帧）
        let m = parse_message(
            r#"{"PosInfo":{"entity_id":231,"pos_x":-21.5,"pos_y":5.9,"radius":0.5}}"#,
        )
        .expect("真帧形状");
        assert_eq!(
            PosInfo::parse(&m),
            Some(PosInfo { entity_id: 231, pos_x: -21.5, pos_y: 5.9, radius: 0.5 })
        );
    }

    /// **反面对照（T76）**：数字/字符串字面量按线上文法精确校验。
    ///
    /// 改前 `get_int` 是"取数字前缀" ⇒ `1.5`→1、`12abc`→12 都"解析成功"，
    /// 而 serde 对 `i64` 的这两种输入都是**类型错 = 反序列化失败**。
    #[test]
    fn non_json_number_literals_are_rejected() {
        for (line, why) in [
            (r#"{"TurnInfo":{"player_id":1.5,"soldier_id":2}}"#, "整数位置给浮点"),
            (r#"{"TurnInfo":{"player_id":12abc,"soldier_id":2}}"#, "整数后面跟垃圾"),
            (r#"{"TurnInfo":{"player_id":+1,"soldier_id":2}}"#, "JSON 不允许前导 +"),
            (r#"{"TurnCountInfo":{"turn_count":"3"}}"#, "字符串冒充整数"),
            (r#"{"TurnCountInfo":{"turn_count":01}}"#, "JSON 不允许前导 0"),
        ] {
            let m = parse_message(line).expect("外壳与括号合法");
            assert!(TurnInfo::parse(&m).is_none(), "{why}：TurnInfo 必须拒绝（{line}）");
            assert!(
                TurnCountInfo::parse(&m).is_none(),
                "{why}：TurnCountInfo 必须拒绝（{line}）"
            );
        }
        // 正对照：真帧里的整数/浮点照旧解得开
        let m = parse_message(r#"{"TurnInfo":{"player_id":635,"soldier_id":636}}"#).unwrap();
        assert_eq!(
            TurnInfo::parse(&m),
            Some(TurnInfo { player_id: 635, soldier_id: 636 })
        );
        let m = parse_message(r#"{"PosInfo":{"entity_id":231,"pos_x":-2.15e+1,"pos_y":5.9,"radius":0.5}}"#).unwrap();
        assert_eq!(PosInfo::parse(&m).map(|p| p.pos_x), Some(-21.5));
    }

    /// **反面对照（T76）**：`u8` 通道越界必须拒绝。
    ///
    /// 改前 `get_color` 用 `as u8` ⇒ `r:300` 静默回绕成 44；serde 的 `u8` 越界是
    /// 反序列化失败（`GraphwarColor` 三个通道都是 `u8`，单态 `0x3cc330`）。
    #[test]
    fn color_channel_out_of_u8_range_is_rejected() {
        let m = parse_message(r#"{"ColorInfo":{"entity_id":1,"color":{"r":300,"g":0,"b":0}}}"#)
            .expect("外壳合法");
        assert_eq!(get_color(&m, "color"), None, "r=300 越界（改前会回绕成 44）");
        let m = parse_message(r#"{"ColorInfo":{"entity_id":1,"color":{"r":-1,"g":0,"b":0}}}"#)
            .expect("外壳合法");
        assert_eq!(get_color(&m, "color"), None, "负数不是 u8");
        let m = parse_message(r#"{"ColorInfo":{"entity_id":1,"color":{"r":149,"g":5,"b":62}}}"#)
            .expect("外壳合法");
        assert_eq!(get_color(&m, "color"), Some((149, 5, 62)), "真帧取值照旧解得开");
        // `GraphwarColor::parse`（值结构体入口）同样不许回绕
        let m = parse_message(r#"{"X":{"r":300,"g":0,"b":0}}"#).expect("合成外壳合法");
        assert_eq!(GraphwarColor::parse(&m), None, "GraphwarColor::parse 也不许把 r=300 回绕");
        let m = parse_message(r#"{"X":{"r":149,"g":5,"b":62}}"#).expect("合成外壳合法");
        assert_eq!(GraphwarColor::parse(&m), Some(GraphwarColor::new(149, 5, 62)));
        // 嵌套在别的字段里的 r/g/b 不算顶层字段
        let m = parse_message(r#"{"X":{"c":{"r":149,"g":5,"b":62}}}"#).expect("合成外壳合法");
        assert_eq!(GraphwarColor::parse(&m), None, "嵌套对象里的 r/g/b 不是顶层字段");
    }

    /// **反面对照（T76）**：变体名是 serde 外部标签的"选路键" —— 名字不同就是**别的**类型，
    /// 字段恰好齐也不许当成自己。
    ///
    /// 改前 17 个手写 + 19 个宏生成实例的 `parse` 里**没有一处**比较 `m.kind`：
    /// `PosInfo::parse` 拿到 `{"TurnInfo":{…PosInfo 的字段…}}` 也会"解析成功"。
    #[test]
    fn typed_parse_requires_its_own_variant_name() {
        // ① 字段齐全但变体名是 TurnInfo ⇒ 不是 PosInfo
        let m = RawMessage {
            kind: "TurnInfo".into(),
            body: r#""entity_id":231,"pos_x":-21.5,"pos_y":5.9,"radius":0.5"#.into(),
            text: String::new(),
        };
        assert!(PosInfo::parse(&m).is_none(), "变体名 TurnInfo 不该被当成 PosInfo");
        // ② 两个事件**字段形状完全相同**（entity_id + name），名字也必须能区分它们
        let m = RawMessage {
            kind: "NameInfo".into(),
            body: r#""entity_id":229,"name":"user""#.into(),
            text: String::new(),
        };
        assert!(NameRequest::parse(&m).is_none(), "NameInfo 的体不该被当成 NameRequest");
        // ③ 正对照：同一段体，名字对上就必须解得开
        let m = RawMessage {
            kind: "PosInfo".into(),
            body: r#""entity_id":231,"pos_x":-21.5,"pos_y":5.9,"radius":0.5"#.into(),
            text: String::new(),
        };
        assert_eq!(
            PosInfo::parse(&m),
            Some(PosInfo { entity_id: 231, pos_x: -21.5, pos_y: 5.9, radius: 0.5 })
        );
    }

    /// **反面对照（T76）**：必填的 `String` 字段缺失必须拒绝。
    ///
    /// 改前 `FunctionInfo::parse` / `FunctionUpdateRequest::parse` 用 `unwrap_or_default()`
    /// ⇒ 缺 `function` 时静默变成空串（"静默丢字段"），而 serde 的 `Str` 是必填。
    /// ⚠️ 但 `function` 取**空串**是合法值（真帧 `{"function":"",…}` 出现过，见
    /// `emit_function_info` 的往返测试）—— 这条测的是"缺字段"，不是"空串"。
    #[test]
    fn missing_required_string_field_is_rejected() {
        let m = parse_message(r#"{"FunctionInfo":{"owner_id":231,"corrected_function":null}}"#)
            .expect("外壳合法");
        assert!(FunctionInfo::parse(&m).is_none(), "缺 function ⇒ None（改前静默变空串）");
        let m = parse_message(r#"{"FunctionUpdateRequest":{"player_id":635}}"#)
            .expect("外壳合法");
        assert!(
            FunctionUpdateRequest::parse(&m).is_none(),
            "缺 function ⇒ None（改前静默变空串）"
        );
        // 正对照 1：空串是合法取值
        let m = parse_message(r#"{"FunctionInfo":{"owner_id":231,"function":"","corrected_function":null}}"#)
            .expect("外壳合法");
        assert_eq!(
            FunctionInfo::parse(&m),
            Some(FunctionInfo {
                owner_id: 231,
                function: String::new(),
                corrected_function: None
            })
        );
        // 正对照 2：`corrected_function` 是 `OptStr`，缺失仍是 None（serde 对 Option 字段缺失的行为）
        let m = parse_message(r#"{"FunctionInfo":{"owner_id":231,"function":"x"}}"#)
            .expect("外壳合法");
        assert_eq!(FunctionInfo::parse(&m).map(|f| f.corrected_function), Some(None));
    }

    /// 一条**裸的** 9 字段广播原文（形状与 `fixtures/protocol.jsonl` 里的 LAN 报文逐字段相同；
    /// 受控实验里被接受的就是这个形状，`docs/archive/115` §5.3 的变体 C）。
    const BARE_BROADCAST: &str = concat!(
        r#"{"room_name":"VAR-C-nl","game_state":"Setup","current_player_count":2,"#,
        r#""address":"[::1]:43717","axis_mode":"EveryUnit","function_mode":"NormalFunction","#,
        r#""turn_mode":"SequentialTurns","time_mode":"Timer1m","locked":false}"#
    );

    /// **负向测试（T76 ③）**：带事件外壳的"变体 B"喂给 `Broadcast::parse` 必须返回 `None`。
    ///
    /// 真值（不是自写断言）：`docs/archive/115-broker.md` §5.3 的**原版客户端受控实验** ——
    /// 往 LAN 端口灌 5 个变体，只有"裸的 9 字段对象 + 结尾换行"（C）出现在 UI 里；
    /// A（裸对象无换行）、**B（事件包裹 `{"GameInfo":{"game_info":…}}`）**、
    /// D（`{"game_info":…}`）、E（`time_mode` = `Timers1m`）**全部被原版丢弃**。
    ///
    /// 改前：`Broadcast::parse` 是子串查找 ⇒ 变体 B 在嵌套的 `game_info` 里也能找到
    /// `"room_name":` ⇒ **解析成功**（T67 实测：一接上 selftest 的变体 B 断言当场变红，
    /// `docs/06` §118.4）。改后：字段只认顶层 ⇒ 变体 B/D 缺顶层 `room_name` ⇒ `None`。
    #[test]
    fn broadcast_parse_rejects_the_event_wrapped_variant_b() {
        // ① 正对照：裸对象必须解得开 —— 否则下面那一串 `None` 什么也证明不了。
        let info = Broadcast::parse(BARE_BROADCAST).expect("裸 9 字段对象必须能解析（受控实验变体 C）");
        assert_eq!(info.room_name, "VAR-C-nl");
        assert_eq!(info.current_player_count, 2);

        // ② 变体 B：事件外壳（**这是 T67 只能靠 `is_bare_object()` 兜住的那条**）
        let variant_b = format!(r#"{{"GameInfo":{{"game_info":{BARE_BROADCAST}}}}}"#);
        assert!(
            Broadcast::parse(variant_b.as_str()).is_none(),
            "变体 B（事件外壳）必须被拒：{variant_b}"
        );
        // ③ 变体 D：只包一层 `game_info`（没有事件名）
        let variant_d = format!(r#"{{"game_info":{BARE_BROADCAST}}}"#);
        assert!(Broadcast::parse(variant_d.as_str()).is_none(), "变体 D 必须被拒");
        // ④ 变体 E：枚举值域外（T66 起由 `TimeMode::from_str` 严格拒）
        let variant_e = BARE_BROADCAST.replace(r#""time_mode":"Timer1m""#, r#""time_mode":"Timers1m""#);
        assert!(Broadcast::parse(variant_e.as_str()).is_none(), "变体 E 必须被拒");
        // ⑤ 9 个字段全藏在嵌套对象里（子串式改前一定会"解析成功"）
        let nested = format!(r#"{{"junk":{BARE_BROADCAST}}}"#);
        assert!(Broadcast::parse(nested.as_str()).is_none(), "嵌套对象里的字段不是顶层字段");

        // ⑥ 反面对照的另一半：改前的形状闸 `is_bare_object()` 会**误拒**带 `{` 的房名
        //    （`docs/06` §118.4 记的那条"推断"级缺陷）。删掉它之后这里是**合法**的。
        let brace_room = BARE_BROADCAST.replace("VAR-C-nl", "a{b");
        assert_eq!(
            Broadcast::parse(brace_room.as_str()).map(|i| i.room_name),
            Some("a{b".to_string()),
            "房名里带 `{{` 是合法字符串内容，不该被形状闸误拒"
        );
    }

    /// `GameInfo` 事件的**外壳**（`{"GameInfo":{"game_info":{…}}}`）仍必须解得开 ——
    /// 变体 B 的"拒"不能把**真帧**一起拒掉。真值：`fixtures/protocol_live_frames.jsonl`
    /// 里 52 条 `GameInfo` 帧的体**恒为** `{"game_info":{9 字段}}`。
    #[test]
    fn game_info_event_wrapper_round_trips() {
        let info = Broadcast::parse(BARE_BROADCAST).expect("裸对象");
        let frame = emit_game_info(&info);
        let raw = parse_frame_message(&frame).expect("线上帧 = 单键对象");
        assert_eq!(raw.kind, "GameInfo");
        // 事件负载路径：顶层有 game_info ⇒ 下钻一层
        assert_eq!(Broadcast::parse(&raw), Some(info.clone()));
        // broker 的 `parse_game_info_event` 依赖这条逐字节往返
        assert_eq!(emit_game_info(&Broadcast::parse(&raw).unwrap()), frame);
    }

    /// **严格版帧解析**：多键 / 尾随垃圾 / 值不是对象 都必须拒；
    /// 而**前缀语义**的 `parse_message` 保持原样（回放行 `{"event":{…},"tick":N}` 靠它）。
    #[test]
    fn frame_message_parser_rejects_trailing_and_multiple_keys() {
        assert!(parse_frame_message(r#"{"Tick":{"current_tick":1}}"#).is_some());
        assert_eq!(
            parse_frame_message(r#"{"Tick":{"current_tick":1}}"#).map(|m| m.body),
            Some(r#""current_tick":1"#.to_string())
        );
        assert!(parse_frame_message(r#"{"Tick":{"current_tick":1}}x"#).is_none(), "尾随垃圾");
        assert!(parse_frame_message(r#"{"Tick":{"current_tick":1},"junk":1}"#).is_none(), "多键");
        assert!(parse_frame_message(r#"{"Tick":1}"#).is_none(), "值不是对象");
        assert!(parse_frame_message("{}").is_none(), "空对象没有事件名");
        assert!(parse_frame_message("not json").is_none());
        // 前缀语义（回放读取的载重契约）**不变**
        let replay_line = r#"{"event":{"NameInfo":{"entity_id":229,"name":"user"}},"tick":253}"#;
        let prefix = parse_message(replay_line).expect("回放行靠前缀语义");
        assert_eq!(prefix.kind, "event");
        assert_eq!(prefix.body, r#""NameInfo":{"entity_id":229,"name":"user"}"#);
        // 严格版对同一行必须拒（它不是线上帧）
        assert!(parse_frame_message(replay_line).is_none());
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
///
/// **严格（T76）**：顶层 + JSON 数字文法（改前是"取数字前缀"）。
pub fn get_f64(msg: &RawMessage, key: &str) -> Option<f64> {
    top_f64(&msg.body, key)
}

/// 取布尔字段。
///
/// **严格（T76）**：顶层 + 只认 `true` / `false` 两个字面量（改前 `starts_with("true")`
/// ⇒ `trueX` 也会被当成 `true`）。
pub fn get_bool(msg: &RawMessage, key: &str) -> Option<bool> {
    top_bool(&msg.body, key)
}

impl AddObstacle {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        if !kind_is(msg, "AddObstacle") {
            return None;
        }
        Some(AddObstacle {
            pos_x: get_f64(msg, "pos_x")?,
            pos_y: get_f64(msg, "pos_y")?,
            radius: get_f64(msg, "radius")?,
        })
    }
}

impl PosInfo {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        if !kind_is(msg, "PosInfo") {
            return None;
        }
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
        if !kind_is(msg, "LifeInfo") {
            return None;
        }
        Some(LifeInfo { entity_id: get_int(msg, "entity_id")?, alive: get_bool(msg, "alive")? })
    }
}

impl FunctionInfo {
    pub fn parse(msg: &RawMessage) -> Option<Self> {
        if !kind_is(msg, "FunctionInfo") {
            return None;
        }
        let owner_id = get_int(msg, "owner_id")?;
        // `function` 是 serde 的 `Str`（必填）：缺字段 ⇒ `None`。改前 `unwrap_or_default()`
        // 会把它静默变成空串 —— 那是"静默丢字段"（台账 fixtures/protocol_parse_audit.tsv）。
        // `corrected_function` 是 `OptStr`：缺失与 `null` 都是 `None`（serde 对 Option 字段
        // 缺失的行为就是 `None`），所以这里不用 `?`。
        let function = get_str(msg, "function")?;
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
        if !kind_is(m, "TurnInfo") {
            return None;
        }
        Some(TurnInfo { player_id: get_int(m, "player_id")?, soldier_id: get_int(m, "soldier_id")? })
    }
}
impl TurnCountInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "TurnCountInfo") {
            return None;
        }
        Some(TurnCountInfo { turn_count: get_int(m, "turn_count")? })
    }
}
impl EndOfTurnInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "EndOfTurnInfo") {
            return None;
        }
        Some(EndOfTurnInfo {
            end_of_turn_tick: get_int(m, "end_of_turn_tick")?,
            time_mode: get_str(m, "time_mode")?,
        })
    }
}
impl FunctionFireRequest {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "FunctionFireRequest") {
            return None;
        }
        Some(FunctionFireRequest { player_id: get_int(m, "player_id")? })
    }
}
impl FunctionFire {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "FunctionFire") {
            return None;
        }
        Some(FunctionFire { soldier_id: get_int(m, "soldier_id")? })
    }
}
impl FunctionUpdateRequest {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "FunctionUpdateRequest") {
            return None;
        }
        Some(FunctionUpdateRequest {
            player_id: get_int(m, "player_id")?,
            // `function` 必填（serde `Str`）：缺字段 ⇒ `None`（改前静默变空串）。
            function: get_str(m, "function")?,
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
        if !kind_is(m, "LifeQueuedDeath") {
            return None;
        }
        Some(LifeQueuedDeath { entity_id: get_int(m, "entity_id")?, tick: get_int(m, "tick")? })
    }
}
impl EffectInfo {
    pub fn parse(m: &RawMessage) -> Option<Self> {
        if !kind_is(m, "EffectInfo") {
            return None;
        }
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
        if !kind_is(m, "DestroyObstacle") {
            return None;
        }
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
        assert_eq!(b.time_mode, crate::graphwar_mode_manager::TimeMode::Timer1m,
                   "实测广播与 TCP 用的是同一种拼写");
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
    use super::tests::{truth4, truth4_usize};
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
    ///
    /// ## 真值来源（T73 / 计划 R05）
    ///
    /// 两侧都是抓到的真实字节：
    /// * **LAN 广播**（`fixtures/protocol_match.jsonl` 的 `{"room_name"…}` 行，UDP 侧原文）
    ///   首字节 = `{`（0x7b）⇒ **没有**前缀（若带前缀，首字节会是长度高位 `0x00`）；
    /// * **对局 TCP 帧**（`fixtures/protocol_live_frames.jsonl`）**有**前缀：1715 帧的
    ///   `frame_sha256 = sha256(u16be(n) ‖ payload)`（见 `frame_length_prefix_matches_payload_length`）。
    ///
    /// 两条一起钉死「前缀只在 `Frame` 层，广播载荷里没有」。
    #[test]
    fn frame_prefix_is_not_part_of_broadcast() {
        let line = captured_broadcasts().remove(0);
        assert_eq!(line.as_bytes()[0], 0x7b,
                   "LAN 广播原文首字节应为 `{{`（0x7b）");
        let b = Broadcast::parse(&line).unwrap();
        let json = b.to_json();
        assert!(!json.starts_with('\u{0}'), "广播不应带二进制长度前缀");
        assert_eq!(truth4("F", "lan_broadcast_first_byte"), "0x7b",
                   "真值表里 LAN 广播首字节口径变了");
        // 而 Frame 编码会加 2 字节前缀，长度等于负载字节数
        let f = Frame { payload: json.clone() };
        let wire = f.encode();
        assert_eq!(wire.len(), json.len() + 2);
        // 真 wire 侧：抓包里**确实**存在带前缀的对局帧（数字来自抓包，不是我方假设）
        assert!(truth4_usize("F", "frames_verified") > 1700,
                "抓包里能逐字节重建的对局帧应上千");
    }
}

// ---------------------------------------------------------------------------
// 消息序列化（只覆盖我们会主动生成的那些）
// ---------------------------------------------------------------------------

/// f64 → **serde_json / ryu 语义**的文本。
///
/// 为什么不能用 `format!("{}")`（`Display`）：它对整数值**丢掉 `.0`**（`15` vs `15.0`），
/// 而且**永不**用指数记法——`2.638231208698597e-62` 会被写成 66 位小数。
/// 回放/抓包是 serde_json 写的，所以逐字节比较时这 16 批全是"值同、字节不同"
/// （`docs/06-verification.md` §25.2 / §30.6）。
///
/// 为什么 `{:?}`（`Debug`）几乎就够：它也是**最短往返**十进制，且定/指数分界与 ryu
/// 只差**一档**——十进制点在 `kk == -4`（即 `|v| ∈ [1e-5, 1e-4)`）时 ryu 用**定点**
/// （`-0.000043453289208041886`），而 `Debug` 用指数（`-4.3453289208041886e-5`）。
/// 这一档由 [`fixed_for_small`] 补上；实测 16 批格式差先降到 1 批、补完归零（§30.6）。
///
/// 这不是"看着像"：`fixtures/protocol*.jsonl` 是真实抓包（ryu 写的）原文，
/// 本模块的 `ryu_*` 测试把里面**每一个**浮点字面量重新格式化后逐字节比对。
///
/// 非有限值走 `null`：serde_json 不接受 `NaN`/`inf`，序列化成 `null`
/// （本仓库解析侧也把 `null` 归到 `Kind::Null`）。
pub fn emit_f64(v: f64) -> String {
    if !v.is_finite() {
        return "null".to_string();
    }
    let s = format!("{v:?}");
    fixed_for_small(&s).unwrap_or(s)
}

/// `Debug` 的 `d.ddddde-5` → ryu 的定点写法 `0.0000ddddd`（只处理 `e-5` 这一档）。
fn fixed_for_small(s: &str) -> Option<String> {
    let (mant, exp) = s.split_once('e')?;
    if exp != "-5" {
        return None;
    }
    let (sign, mant) = match mant.strip_prefix('-') {
        Some(m) => ("-", m),
        None => ("", mant),
    };
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    if digits.is_empty() {
        return None;
    }
    Some(format!("{sign}0.0000{digits}"))
}

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
/// 浮点用 [`emit_f64`]（serde_json/ryu 语义），见那里为什么要用 `{:?}` 而不是 `{}`。
pub fn emit_function_points(owner_id: i64, start_x: f64, start_y: f64, end_x: f64) -> String {
    format!(
        r#"{{"FunctionPoints":{{"owner_id":{},"start_x":{},"start_y":{},"end_x":{}}}}}"#,
        owner_id,
        emit_f64(start_x),
        emit_f64(start_y),
        emit_f64(end_x)
    )
}

/// `{"EffectInfo":{"effect_id":..,"pos_x":..,"pos_y":..,"effect_type":"..","start_tick":..}}`
pub fn emit_effect_info(effect_id: i64, x: f64, y: f64, effect_type: &str, start_tick: i64) -> String {
    format!(
        r#"{{"EffectInfo":{{"effect_id":{},"pos_x":{},"pos_y":{},"effect_type":"{}","start_tick":{}}}}}"#,
        effect_id,
        emit_f64(x),
        emit_f64(y),
        effect_type,
        start_tick
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

/// f64 打印的两条护栏（`docs/06` §30）。
///
/// * `ryu_fixtures_roundtrip`：**真实抓包原文**里的每一个浮点字面量，
///   解析成 f64 再用 [`emit_f64`] 打回去，必须**逐字节**相同。
///   （抓包是 serde_json/ryu 写的 ⇒ 这条把"布局规则"钉死在真值上。）
/// * `ryu_known_divergences`：`Display`（`{}`）与 ryu **不一样**的那几类，
///   逐条给出"旧写法 / 新写法"。这 6 条就是 §25.2 里 16 批格式差的原文对。
#[cfg(test)]
mod emit_f64_tests {
    use super::*;

    /// 从一条原始报文里扫出**字符串之外**的浮点字面量（含 `.` 或 `e`）。
    fn float_literals(line: &str) -> Vec<String> {
        let b = line.as_bytes();
        let mut out = Vec::new();
        let mut in_str = false;
        let mut i = 0usize;
        while i < b.len() {
            let c = b[i];
            if in_str {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            if c == b'"' {
                in_str = true;
                i += 1;
                continue;
            }
            if c.is_ascii_digit() || c == b'-' {
                let start = i;
                while i < b.len()
                    && (b[i].is_ascii_digit()
                        || b[i] == b'.'
                        || b[i] == b'-'
                        || b[i] == b'+'
                        || b[i] == b'e'
                        || b[i] == b'E')
                {
                    i += 1;
                }
                let tok = &line[start..i];
                if tok.contains('.') || tok.contains('e') || tok.contains('E') {
                    out.push(tok.to_string());
                }
                continue;
            }
            i += 1;
        }
        out
    }

    #[test]
    fn ryu_fixtures_roundtrip() {
        let mut n = 0usize;
        for text in [
            include_str!("../../fixtures/protocol_match.jsonl"),
            include_str!("../../fixtures/protocol_death.jsonl"),
            include_str!("../../fixtures/protocol.jsonl"),
        ] {
            for line in text.lines() {
                for lit in float_literals(line) {
                    let v: f64 = lit.parse().expect("抓包里的浮点字面量应能解析");
                    assert_eq!(emit_f64(v), lit, "抓包浮点原文 {lit} 往返不一致");
                    n += 1;
                }
            }
        }
        assert!(n >= 20, "抓包夹具里的浮点字面量太少（{n}），这条护栏失去意义");
    }

    /// `ryu` 与 `Display` 的已知差异。
    ///
    /// ⚠️ **契约断言（我方格式化器的行为），不含语义**（T73 / 计划 R09）：期望串是
    /// 「serde_json 会这么写」的**推断**，真帧里只有其中几种形态出现过。真值侧的覆盖是
    /// `ryu_fixtures_roundtrip`（输入 = 抓包里的浮点原文，逐字往返）与
    /// `emit_*_matches_captured`（我方产出的整帧与抓包逐字节）。这条钉的是
    /// 「我方 `emit_f64` 不退化成 `Display`」，**不是**「ryu 已逐位复原」。
    #[test]
    fn ryu_known_divergences() {
        // (值, 旧写法 `{}`, serde_json/ryu 写法)
        let cases: [(f64, &str, &str); 6] = [
            (0.0, "0", "0.0"),
            (-9.0, "-9", "-9.0"),
            (15.0, "15", "15.0"),
            (2.638231208698597e-62, "0.00000000000000000000000000000000000000000000000000000000000002638231208698597", "2.638231208698597e-62"),
            (1.7763568394002505e-15, "0.0000000000000017763568394002505", "1.7763568394002505e-15"),
            (7.606517635480697e-19, "0.0000000000000000007606517635480697", "7.606517635480697e-19"),
        ];
        for (v, display, ryu) in cases {
            assert_eq!(format!("{v}"), display, "旧写法（Display）变了？");
            assert_eq!(emit_f64(v), ryu, "ryu 写法不一致：{v}");
        }
        // -0.0 / 极大值 / 指数阈值两侧
        assert_eq!(emit_f64(-0.0), "-0.0");
        assert_eq!(emit_f64(1e15), "1000000000000000.0");
        assert_eq!(emit_f64(1e16), "1e16");
        assert_eq!(emit_f64(0.0001), "0.0001");
        // ⚠️ 这一档 ryu 用**定点**，Rust `Debug` 用指数（§30.6 唯一剩下的那 1 批）
        assert_eq!(format!("{:?}", 0.00001), "1e-5");
        assert_eq!(emit_f64(0.00001), "0.00001");
        assert_eq!(format!("{:?}", -4.3453289208041886e-5), "-4.3453289208041886e-5");
        assert_eq!(emit_f64(-4.3453289208041886e-5), "-0.000043453289208041886");
        assert_eq!(emit_f64(-1.7763568394002505e-15), "-1.7763568394002505e-15");
        // 非有限值：serde_json 不接受 NaN/inf，写 `null`
        assert_eq!(emit_f64(f64::NAN), "null");
        assert_eq!(emit_f64(f64::INFINITY), "null");
    }

    /// 我方 `emit_*` 对整数值浮点带 `.0`。
    ///
    /// ⚠️ **契约断言**（T73 / 计划 R09）：期望串是我方构造的（`150.0`/`-9.0`/`15.0`），
    /// 不是抓到的字节。真值侧见 `emitted_function_points_matches_captured_floats`
    /// 与 `emit_f64` 对抓包浮点原文的往返（`ryu_fixtures_roundtrip`）。
    #[test]
    fn emit_message_uses_ryu() {
        // 整数值的 `end_x` / `pos_y` 必须带 `.0`
        assert_eq!(
            emit_function_points(7, 150.0, -9.0, 15.0),
            r#"{"FunctionPoints":{"owner_id":7,"start_x":150.0,"start_y":-9.0,"end_x":15.0}}"#
        );
        assert_eq!(
            emit_effect_info(3, 0.0, 15.0, "ExplosionEffect", 40),
            r#"{"EffectInfo":{"effect_id":3,"pos_x":0.0,"pos_y":15.0,"effect_type":"ExplosionEffect","start_tick":40}}"#
        );
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
        let function = get_str(&m, "function").unwrap();
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
/// [`get_str`] 不解转义，遇到 `\"` 会原样返回；昵称与外观名都可能带引号，
/// 所以本节统一用这个。
///
/// **严格（T76）**：顶层 + 必须是完整字符串字面量（改前是 `body.find`，任意深度都命中）。
pub fn get_str_unescaped(msg: &RawMessage, key: &str) -> Option<String> {
    top_str_unescaped(&msg.body, key)
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self { extra: strict_extra_fields(&m.body, &[])? })
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    entity_id: get_int(m, "entity_id")?,
                    $field: get_str_unescaped(m, stringify!($field))?,
                    extra: strict_extra_fields(&m.body, &["entity_id", stringify!($field)])?,
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
        if !kind_is(m, "RankRequest") {
            return None;
        }
        Some(RankRequest {
            entity_id: get_int(m, "entity_id")?,
            rank: get_int(m, "rank"),
            extra: strict_extra_fields(&m.body, &["entity_id", "rank"])?,
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
        if !kind_is(m, "RankInfo") {
            return None;
        }
        Some(RankInfo {
            entity_id: get_int(m, "entity_id")?,
            rank: get_int(m, "rank"),
            extra: strict_extra_fields(&m.body, &["entity_id", "rank"])?,
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
        if !kind_is(m, "VersionInfo") {
            return None;
        }
        Some(VersionInfo {
            version: get_str_unescaped(m, "version")?,
            system: get_str_unescaped(m, "system")?,
            extra: strict_extra_fields(&m.body, &["version", "system"])?,
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
        if !kind_is(m, "BotAdded") {
            return None;
        }
        Some(BotAdded {
            connection_id: get_int(m, "connection_id")?,
            player_id: get_int(m, "player_id")?,
            level: get_int(m, "level")?,
            extra: strict_extra_fields(&m.body, &["connection_id", "player_id", "level"])?,
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
// 1. **`N`**：字段个数与 `.rodata` 里 `struct <Name>Event with N element(s)` 的 N
//    逐个相等（**64/64 有字段事件**）。`.rodata` 里这条串共 **67** 条 = 上面 64 个事件结构体
//    + 3 个**非事件负载**结构体（`GameInfo`9 / `GraphwarColor`3 / `EventInfo`2）；
//    其余 **10** 个事件是空体（`{}`），serde 对 0 字段结构体只产出短形式 `struct <Name>Event`
//    ⇒ 它们**本来就没有**这条串。2026-11-26 复核见 `docs/07` §3.2 的就地更正 与 `docs/06` §66。
// 2. **反序列化侧**：每个事件 `Deserialize` 的 `visit_map` 里都能找到同名键的字节比较
//    （有字段的 64 个事件全部字段被比到）。注意它常用**合并后的 16 字节窗口**，所以字面量地址与
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
/// **线上枚举的完整取值集合**（2026-11-07 穷举，`docs/07` §5.2）。
///
/// 穷尽性判据是**结构性**的：serde 的 `unknown_variant` 会把 `VARIANTS` 表的
/// **地址与长度都写成指令立即数**（`lea reg,[rip+V]; mov reg,N`）；全 `.text`
/// 按这个形状扫，**恰好命中 8 张表**（下面 7 个 + `GraphwarEvent` 的 74 条），
/// **没有第 9 张** ⇒ 这就是全集，不是"已知值"。
///
/// 校准点：`GameState` 那一行与 `.data.rel.ro 0x9586d0` 的 8 个值、字节长
/// `10/5/19/18/20/16/10/12` 逐项相同（`docs/14` §11）。
pub const WIRE_VALUE_ENUMS: &[(&str, &[&str])] = &[
    ("AxisMode", &["NoAxis", "OnlyMain", "EveryFive", "EveryUnit"]),
    ("FunctionMode", &["NormalFunction", "DiffEqFunction"]),
    ("TurnMode", &["SequentialTurns", "SimultaneousTurns"]),
    ("TimeMode", &["Timer30s", "Timer1m", "Timer2m", "Timer3m", "Timer5m", "TimerInf"]),
    ("GameState", &[
        "Connecting", "Setup", "WaitingForFunctions", "CalculatingResults",
        "SynchronizingResults", "ExecutingResults", "EndingGame", "Disconnected",
    ]),
    ("GraphwarTeam", &["Team1", "Team2"]),
    ("EffectType", &["ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"]),
];

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
    WireEvent { name: "GameHeartbeat", tag: Some(63), fields: &[] },
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
    WireEvent { name: "VersionInfo", tag: Some(65), fields: &[("version", ValType::Str), ("system", ValType::Str)] },
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

    /// `{"r":..,"g":..,"b":..}` 的三个通道。
    ///
    /// **严格（T76）**：三个键都必须是**顶层**字段、值都必须是 `u8` 范围（serde 的 `u8`
    /// 越界/负数 = 反序列化失败；改前 `get_int(..)? as u8` 会把 `r:300` 静默回绕成 44）。
    /// 多加的字段忽略（serde derive 默认）。
    ///
    /// ⚠️ 这里**没有**变体名闸：本结构体是 `ColorInfo.color` 的值（不是事件），
    /// `tests/wire_format.rs` 用 `{"X":{"r":…}}` 这种合成外壳解它。
    pub fn parse(m: &RawMessage) -> Option<Self> {
        Some(Self {
            r: as_u8_literal(top_level_value(&m.body, "r")?)?,
            g: as_u8_literal(top_level_value(&m.body, "g")?)?,
            b: as_u8_literal(top_level_value(&m.body, "b")?)?,
        })
    }
}

/// LAN 广播 / `GameInfoEvent` 负载：**房间信息，9 个字段** —— 本 crate 里**只有这一份定义**。
///
/// **原版只有一个** `struct GameInfo`：serde 名串 `struct GameInfo with 9 elements`
/// （槽 `0x9502f0` → `0x1bc121`），`Serialize` 在 **`0x5060d0`**，键序
/// `room_name, game_state, current_player_count, address, axis_mode, function_mode,
/// turn_mode, time_mode, locked`（`tools/wire_fields.py --struct 0x5060d0`；与真实 LAN 广播
/// 报文逐字节一致 —— `docs/07` §1、`docs/06` §109.4）。
///
/// ## 形态归属（T66，`docs/06` §112）
///
/// **权威形态 = [`crate::events::GameInfo`]**（该文件是原版 `graphwar_game_info` 模块的结构体本体，
/// 经 [`crate::graphwar_game_info`] 再导出）。本模块**不再定义第二个 `GameInfo`**：
/// 这里是 `pub use` 的**同一类型**，只额外提供两个**线上进出口**：
/// * [`GameInfo::to_json`] —— serde 的 `Serialize`（键序 = `0x5060d0` 的 `serialize_field` 顺序）；
/// * [`GameInfo::parse`] —— serde 的 `Deserialize`（5 个枚举字段按**变体名**严格解析，
///   未知名字 ⇒ `None`，与 serde「未知变体 = 反序列化失败」同口径）。
///
/// `Broadcast` 是本类型的**别名**（LAN 广播是裸对象、事件负载是 `{"GameInfo":{"game_info":…}}`，
/// 两者在原版里用的是同一个结构体）⇒ 我方 `GameInfo` 的结构体**定义数：3 → 1**。
///
/// **字段类型**（逐位证据见 `docs/06` §109.4）：5 个枚举（`GameState`/`AxisMode`/`FunctionMode`/
/// `TurnMode`/`TimeMode`）+ `current_player_count: u32`（4 字节 + itoa u32 路径）+ `address: String`
/// （原版是 32 字节 / 1 字节 tag / 2 变体的枚举，形状 = 观测、身份 = 推断 ⇒ 仍用 `String` 并登记）。
///
/// ⚠️ 注意与 [`LockInfo`]/[`LockRequest`] 的字段名**不同**：这里是 `locked`，那两条是 `lock`。
pub use crate::events::GameInfo;

impl GameInfo {
    /// 键序 = 二进制里的 `serialize_field` 顺序（9 个，`docs/06` §109.4）。
    ///
    /// 串按 JSON 规则转义（与 serde_json 对 `String` 的表示一致）；枚举写变体名；布尔小写。
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                r#"{{"room_name":{},"game_state":{},"current_player_count":{},"#,
                r#""address":{},"axis_mode":{},"function_mode":{},"#,
                r#""turn_mode":{},"time_mode":{},"locked":{}}}"#
            ),
            json_str(&self.room_name),
            json_str(self.game_state.as_str()),
            self.current_player_count,
            json_str(&self.address),
            json_str(self.axis_mode.as_str()),
            json_str(self.function_mode.as_str()),
            json_str(self.turn_mode.as_str()),
            json_str(self.time_mode.as_str()),
            self.locked,
        )
    }

    /// 解析（两种输入形态见 [`JsonObject`]）：广播 = 整段对象文本，
    /// 事件负载 = [`RawMessage`]（取它的 `body`）。
    ///
    /// 5 个枚举字段**严格**按变体名解析（`GameState::from_str` 等），未知名字 ⇒ `None`：
    /// 与 serde 派生出来的 `Deserialize` 同口径（未知变体 = 反序列化失败）。
    /// ## 严格性（T76：把 T67 记在 `docs/06` §118.4 的那条闸**收进 protocol.rs**）
    ///
    /// 两种入口形态的严格性**不同**，由输入类型定（见 [`JsonObject::is_event_payload`]）：
    ///
    /// * `Broadcast::parse(裸广播原文: &str)` —— 必须**本身就是** 9 字段裸对象。
    ///   字段一律**只在顶层**找（[`top_level_value`] 不下钻），所以
    ///   `{"GameInfo":{"game_info":{…}}}`（受控实验的**变体 B**）与
    ///   `{"game_info":{…}}`（**变体 D**）都**缺顶层 `room_name` ⇒ `None`**。
    ///   这正是原版行为（`docs/archive/115` §5.3：A/B/D/E 全丢，只有"裸对象 + 换行"被接受）。
    ///   ⇒ `_scaffold/lan_v6_class.rs` 里那道具名闸 `is_bare_object()` 不再需要，已删。
    /// * `GameInfo::parse(&RawMessage)` —— 事件负载：顶层有 `game_info` 就下钻**一层**
    ///   （`WIRE_SPEC`：`GameInfo` 事件只有这一个字段），否则把这个体当 9 字段结构体
    ///   （本 crate 两个解码入口共用同一个结构体，T66）。
    ///
    /// 字段级：5 个枚举按变体名严格解析（未知名字 ⇒ `None`）；`current_player_count` 必须
    /// 是 `u32` 范围；缺任何必填字段 ⇒ `None`；**多余字段忽略**（serde derive 默认）。
    pub fn parse(obj: impl JsonObject) -> Option<Self> {
        let text = obj.json_object_text().trim();
        let body = if obj.is_event_payload() {
            // 事件负载：`{"game_info":{…}}` 就下钻一层；否则体本身就是 9 字段结构体。
            match top_level_value(text, "game_info") {
                Some(raw) => object_body(raw)?,
                None => text,
            }
        } else {
            // 线上裸对象原文（LAN 数据报）：必须是完整对象字面量，**绝不下钻**。
            object_body(text)?
        };
        Some(Self {
            room_name: top_str_unescaped(body, "room_name")?,
            game_state: crate::turnstate::GameState::from_str(&top_str_unescaped(body, "game_state")?)?,
            current_player_count: u32::try_from(top_int(body, "current_player_count")?).ok()?,
            address: top_str_unescaped(body, "address")?,
            axis_mode: crate::graphwar_mode_manager::AxisMode::from_str(&top_str_unescaped(
                body, "axis_mode",
            )?)?,
            function_mode: crate::graphwar_mode_manager::FunctionMode::from_str(
                &top_str_unescaped(body, "function_mode")?,
            )?,
            turn_mode: crate::graphwar_turn_manager::TurnMode::from_str(&top_str_unescaped(
                body, "turn_mode",
            )?)?,
            time_mode: crate::graphwar_mode_manager::TimeMode::from_str(&top_str_unescaped(
                body, "time_mode",
            )?)?,
            locked: top_bool(body, "locked")?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    $field: get_bool(m, stringify!($field))?,
                    extra: strict_extra_fields(&m.body, &[stringify!($field)])?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    $field: get_int(m, stringify!($field))?,
                    extra: strict_extra_fields(&m.body, &[stringify!($field)])?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    $field: get_int(m, stringify!($field)),
                    extra: strict_extra_fields(&m.body, &[stringify!($field)])?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    $field: get_str_unescaped(m, stringify!($field))?,
                    extra: strict_extra_fields(&m.body, &[stringify!($field)])?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self {
                    $a: get_int(m, stringify!($a))?,
                    $b: get_int(m, stringify!($b))?,
                    extra: strict_extra_fields(&m.body, &[stringify!($a), stringify!($b)])?,
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
                if !kind_is(m, stringify!($ty)) {
                    return None;
                }
                Some(Self { extra: strict_extra_fields(&m.body, &[])? })
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

    /// 每条事件的样本行。**T64 之后这张表只剩「兜底」作用**：`sample_of` 会优先取
    /// `fixtures/protocol_live_frames.jsonl` 里该事件的**首帧**（18 种事件有真帧），
    /// 这里的串只在这 5 种事件上才会被用到。
    ///
    /// 四种来源，**必须**如实标：
    ///
    /// * `线上真帧` — **T53 连官服/自建局抓包**（1717 帧）里该事件的首帧（**18 种**）：
    ///   `sample_of` 直接返回抓到的字节，`replays_every_live_frame_of_the_supplemented_kinds`
    ///   逐帧回放全部 352 帧；
    /// * `语料真帧` — `TurnLimitInfo`：线上 1717 帧抓不到它，但 403 份冻结回放里有 **21 帧**
    ///   （T53 §110.5 解压重扫；T46 记的「零真实帧」是 gzip-grep 假阴性）；
    /// * `fixture` — 旧抓包（`fixtures/protocol_match.jsonl`）里的真帧：只剩 `VersionInfo`；
    /// * `合成` — 纯构造：**本仓库至今没有任何一本抓包含该事件**（`GameHeartbeat`/
    ///   `ClearBansRequest`/`BanConnectionRequest`），形状只由二进制的 `serialize_field`
    ///   调用序定死（`逐位`），**语义仍是 `推断`** —— 这三条按 `fixtures/test_expectation_plan3.tsv`
    ///   的 P21–P23 **换不了**，只改注释口径，不新增"真值"。
    const SAMPLES: &[(&str, &str)] = &[
        ("NameRequest", r#"{"NameRequest":{"entity_id":229,"name":"user"}}"#), // 线上真帧（3 帧）
        ("NameInfo", r#"{"NameInfo":{"entity_id":232,"name":"Skynet"}}"#),     // 线上真帧（20 帧）
        ("RankRequest", r#"{"RankRequest":{"entity_id":232,"rank":null}}"#),   // 线上真帧（3 帧）
        ("RankInfo", r#"{"RankInfo":{"entity_id":229,"rank":0}}"#),            // 线上真帧（14 帧，含 rank:null）
        ("LockInfo", r#"{"LockInfo":{"lock":true}}"#),                         // 线上真帧（2 帧：自建局锁房/解锁）
        ("LockRequest", r#"{"LockRequest":{"lock":true}}"#),                   // 线上真帧（2 帧，client→server）
        ("ChatMessage", r#"{"ChatMessage":{"entity_id":228,"message":"gg"}}"#), // 线上真帧（1 帧；正文打码成 <len=2>）
        ("ChatMessageRequest", r#"{"ChatMessageRequest":{"entity_id":228,"message":"gg"}}"#), // 线上真帧（1 帧，client→server）
        ("SkinGraphicsRequest", r#"{"SkinGraphicsRequest":{"entity_id":232,"graphics_str":"bot_0"}}"#), // 线上真帧（3 帧）
        ("SkinGraphicsInfo", r#"{"SkinGraphicsInfo":{"entity_id":229,"graphics_str":"yellow"}}"#), // 线上真帧（60 帧）
        ("FaceGraphicsRequest", r#"{"FaceGraphicsRequest":{"entity_id":232,"graphics_str":"no_face"}}"#), // 线上真帧（3 帧）
        ("FaceGraphicsInfo", r#"{"FaceGraphicsInfo":{"entity_id":229,"graphics_str":"regular_eyes"}}"#), // 线上真帧（60 帧）
        ("HatGraphicsRequest", r#"{"HatGraphicsRequest":{"entity_id":232,"graphics_str":"no_hat"}}"#), // 线上真帧（3 帧）
        ("HatGraphicsInfo", r#"{"HatGraphicsInfo":{"entity_id":229,"graphics_str":"soldier_helmet"}}"#), // 线上真帧（60 帧）
        // ⚠️ 换不了（P21）：T53 三会话 1717 帧、官服房间一整局都没有它 ⇒ 形状逐位、语义推断
        ("GameHeartbeat", r#"{"GameHeartbeat":{}}"#),
        ("GameRemoved", r#"{"GameRemoved":{"game_address":"69.164.213.144:42409"}}"#), // 线上真帧（broker 广播，1 帧）
        ("VersionInfo", r#"{"VersionInfo":{"version":"2.0.60","system":"linux"}}"#), // fixture（旧抓包 protocol_match.jsonl，1 帧；线上 1717 帧里没有它）
        ("BotAdded", r#"{"BotAdded":{"connection_id":228,"player_id":232,"level":3}}"#), // 线上真帧（4 帧；真机 level=1000）
        ("TurnLimitInfo", r#"{"TurnLimitInfo":{"turn_limit":5}}"#),             // 语料真帧（21 份回放，值 1/5/6/10；见 turn_limit_samples_are_the_corpus_frames）
        ("EntityEffectInfo", r#"{"EntityEffectInfo":{"entity_id":230,"effect_type":"DeathEffect"}}"#), // 线上真帧（110 帧）
        ("ClearObstacles", r#"{"ClearObstacles":{}}"#),                        // 线上真帧（2 帧）
        // ⚠️ 换不了（P22）：1717 帧 0 帧；客户端 UI 里没有「清空封禁」入口 ⇒ 形状逐位、语义推断
        ("ClearBansRequest", r#"{"ClearBansRequest":{}}"#),
        ("BanConnectionRequest",
         r#"{"BanConnectionRequest":{"player_connection_id":228,"leader_connection_id":229}}"#), // ⚠️ 换不了（P23）：1717 帧 0 帧
    ];

    /// 线上真帧：`fixtures/protocol_live_frames.jsonl`（T53 连官服/自建局抓包，**1717 帧**）。
    ///
    /// 证据链：`tools/live_capture/live_wire.js`（hex 直接来自 `recv` 缓冲）
    /// → `tools/live_capture/parse_live.py`（切帧 + 打码 + 落表）
    /// → `fixtures/protocol_live_frames.jsonl`（每帧自带负载 `sha256` / 带前缀 `frame_sha256` / `n`）
    /// → `tools/gen_test_expectations3.py`（逐帧核 sha256/n，再算每类事件的负载 FNV 与帧数）
    /// → `fixtures/test_expectation_truth3.tsv`（`P/<事件>/fnv1a64`、`P/<事件>/frames`）→ **本测试**。
    ///
    /// ⚠️ 打码帧（只有 `ChatMessage*` 的正文）的 `hex` 是**打码后**文本（`<len=N>`），
    /// 与 `n`/`sha256`（原始负载口径）不同长 —— 这是 T53 的隐私口径（`docs/plan-official-capture.md` §1.3）。
    const LIVE_FRAMES_JSONL: &str = include_str!("../../fixtures/protocol_live_frames.jsonl");

    /// `hex` → 字节。
    fn hex_bytes(hex: &str) -> Vec<u8> {
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("抓包 hex"))
            .collect()
    }

    /// 抓包里某事件的**全部线上负载字节**（按抓包顺序）。没有该事件 ⇒ 空。
    fn live_payloads(kind: &str) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for line in LIVE_FRAMES_JSONL.lines() {
            let Some(pos) = line.find("\"event\": \"") else { continue };
            let rest = &line[pos + 10..];
            let Some(end) = rest.find('"') else { continue };
            if &rest[..end] != kind {
                continue;
            }
            let Some(pos) = line.find("\"hex\": \"") else { continue };
            let rest = &line[pos + 8..];
            let Some(end) = rest.find('"') else { continue };
            out.push(hex_bytes(&rest[..end]));
        }
        out
    }

    /// FNV-1a 64（与 `tools/gen_test_expectations3.py::fnv1a64` 同口径）。
    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// 样本：**优先取线上真帧的首帧**（18 种事件有），否则退回 `SAMPLES`（4 种抓不到 + 语料样本）。
    ///
    /// 这样 23 条 `round_trip_*` 里 19 条的期望值就是抓到的字节本身，而不是手抄串。
    fn sample_of(kind: &str) -> String {
        if let Some(first) = live_payloads(kind).into_iter().next() {
            return String::from_utf8(first).expect("线上负载应是 UTF-8");
        }
        SAMPLES
            .iter()
            .find(|(k, _)| *k == kind)
            .unwrap_or_else(|| panic!("样本表里没有 {kind}"))
            .1
            .to_string()
    }

    /// 每个补齐事件一条往返测试：`解析 → 重新序列化` 必须逐字节相同。
    ///
    /// **期望值来源**（`fixtures/test_expectation_plan3.tsv` 的 P01–P23）：
    /// `sample_of` 优先给**线上真帧的首帧**（18 种事件，352 帧里的第一帧）⇒ 这里比的是
    /// **抓到的字节**，不是手抄串；`TurnLimitInfo` 是冻结语料的真帧文本；
    /// `VersionInfo` 是旧抓包原文；`GameHeartbeat`/`ClearBansRequest`/`BanConnectionRequest`
    /// **换不了**（1717 帧 0 帧）⇒ 仍是 `合成` 样本，形状逐位、**语义推断**。
    macro_rules! round_trip_tests {
        ($($name:ident => $ty:ident),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    let line = sample_of(stringify!($ty));
                    let m = parse_message(&line).expect("样本应能解析成单键消息");
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

    /// 把聊天正文换成 `<len=N>`（与 `fixtures/protocol_live_expected.tsv` 的打码口径一致）。
    ///
    /// 只处理 `"message":"…"` 这一个键：字段形状与长度留着，正文不留
    /// （`docs/plan-official-capture.md` §1.3：`ChatMessage` 只留字段形状与长度分布）。
    fn mask_chat(line: &str) -> String {
        const KEY: &str = r#""message":""#;
        let Some(start) = line.find(KEY) else { return line.to_string() };
        let body = start + KEY.len();
        let rest = &line[body..];
        let Some(end) = rest.find('"') else { return line.to_string() };
        // **幂等**：`sample_of` 现在优先给线上真帧，而抓包夹具里的 `ChatMessage` 正文**已经在
        // T53 落表时打码过**（`<len=N>`）⇒ 再打一次会把 `<len=2>` 变成 `<len=7>`（踩过）。
        if rest[..end].starts_with("<len=") && rest[..end].ends_with('>') {
            return line.to_string();
        }
        let msg_len = rest[..end].chars().count();
        format!("{}{}<len={}>{}", &line[..body], "", msg_len, &rest[end..])
    }

    /// **T53 的硬闸：`SAMPLES` 里标 `真帧` 的那几条，必须与线上负载字节逐字节相同。**
    ///
    /// 换真值的意义全在这条测试里 —— 只把样本串改成"看着像线上"的样子是好不了多少的，
    /// 必须能证明它就是**抓到的字节**。证据链：
    /// `tools/live_capture/live_wire.js`（探针，hex 直接来自 `recv` 的缓冲）
    /// → `tools/live_capture/parse_live.py`（切帧 + 打码 + 落表）
    /// → `fixtures/protocol_live_expected.tsv`（本测试 `include_str!` 进来）
    /// → `SAMPLES`。
    ///
    /// `TurnLimitInfo` 那一行的来源是**冻结语料**（21 份回放里各有 1 帧，见
    /// `fixtures/protocol_zero_frame_events.tsv`），方向列写 `回放`。
    #[test]
    fn captured_samples_are_the_live_wire_bytes() {
        const TABLE: &str = include_str!("../../fixtures/protocol_live_expected.tsv");
        let mut checked = 0usize;
        for line in TABLE.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols.len() < 6 || cols[0] == "event" {
                continue;
            }
            let (event, dir, hexs, text) = (cols[0], cols[2], cols[3], cols[4]);
            // `EndingGame` 不是事件名，是 `GraphwarGameState` 的取值 ⇒ 它没有 SAMPLES 行，
            // 单独判：帧原文必须逐字节等于语料里的那个状态帧。
            if event == "GameStateInfo" {
                assert_eq!(text, r#"{"GameStateInfo":{"game_state":"EndingGame"}}"#,
                           "EndingGame 帧原文与预期不符（{dir}）");
                checked += 1;
                continue;
            }
            let sample = mask_chat(&sample_of(event));
            assert_eq!(sample, text,
                       "{event} 的样本打码后与线上真帧原文不一致（方向 {dir}）");
            let bytes: Vec<u8> = (0..hexs.len() / 2)
                .map(|i| u8::from_str_radix(&hexs[i * 2..i * 2 + 2], 16).expect("hex"))
                .collect();
            assert_eq!(bytes, sample.as_bytes(),
                       "{event} 的样本与线上负载**字节**不一致（方向 {dir}）");
            checked += 1;
        }
        assert_eq!(checked, 7,
                   "真帧条数应为 7：LockInfo/ChatMessage/GameRemoved/TurnLimitInfo/\
                    LockRequest/ChatMessageRequest + EndingGame(GameStateInfo)");
    }

    /// **线上真帧的全量硬闸**：18 种补齐事件 × **352 帧**，逐帧「解析 → 重新序列化」必须逐字节相同。
    ///
    /// 与上面那条（T53 只钉 7 条样本）的分工：那条证「样本串就是抓到的字节」，这条证
    /// 「**抓到的每一帧**我们都能原样回放」——`SkinGraphicsInfo`(60) / `FaceGraphicsInfo`(60) /
    /// `HatGraphicsInfo`(60) / `EntityEffectInfo`(110) 这些取值组合，任何手抄样本都覆盖不到。
    ///
    /// 帧数与负载指纹（`fixtures/test_expectation_truth3.tsv` 的 `P/<事件>/frames` 与 `fnv1a64`）
    /// 钉死在这里 ⇒ 抓包被改一帧、或有人拿新抓包覆盖夹具，这条立刻红。
    #[test]
    fn replays_every_live_frame_of_the_supplemented_kinds() {
        // (事件, 帧数, 负载按抓包顺序拼接的 FNV-1a 64)
        const LIVE: [(&str, usize, u64); 18] = [
            ("NameRequest", 3, 0x93dd_c46f_0175_f6a6),
            ("NameInfo", 20, 0x5600_1fee_ea8d_b908),
            ("RankRequest", 3, 0xc657_f1c4_b285_29f9),
            ("RankInfo", 14, 0x82ed_f648_b9db_3507),
            ("LockInfo", 2, 0x830e_047d_a4ce_0458),
            ("LockRequest", 2, 0x7b7f_99bc_8287_6c0a),
            ("ChatMessage", 1, 0x9a9d_ba88_a7b9_b38c),
            ("ChatMessageRequest", 1, 0x52ef_a3e1_b341_f80f),
            ("SkinGraphicsRequest", 3, 0x5739_accf_b33d_476a),
            ("SkinGraphicsInfo", 60, 0x5910_42a6_7d08_37ed),
            ("FaceGraphicsRequest", 3, 0xb021_6553_994b_7f57),
            ("FaceGraphicsInfo", 60, 0xefd3_25e7_45e8_d117),
            ("HatGraphicsRequest", 3, 0xd726_1229_245a_f66e),
            ("HatGraphicsInfo", 60, 0xe3d8_02b8_3361_7465),
            ("GameRemoved", 1, 0x1b1e_3b53_b3be_9174),
            ("BotAdded", 4, 0x16d2_48d3_c3db_3390),
            ("EntityEffectInfo", 110, 0x4d18_3fe0_fe98_3ab3),
            ("ClearObstacles", 2, 0x8596_1ecf_2132_d5e3),
        ];
        let mut total = 0usize;
        for (kind, want_n, want_fnv) in LIVE {
            let payloads = live_payloads(kind);
            assert_eq!(
                payloads.len(), want_n,
                "{kind}: 线上帧数与 fixtures/test_expectation_truth3.tsv 不符"
            );
            let joined: Vec<u8> = payloads.iter().flatten().copied().collect();
            assert_eq!(
                fnv1a64(&joined), want_fnv,
                "{kind}: 线上负载指纹变了（真帧被改过 / 换了抓包）"
            );
            for p in &payloads {
                let text = std::str::from_utf8(p).expect("线上负载应是 UTF-8");
                let m = parse_message(text)
                    .unwrap_or_else(|| panic!("{kind}: 线上帧解析失败：{text}"));
                assert_eq!(m.kind, kind, "线上帧的事件名与抓包记录不符：{text}");
                let ev = AnyEvent::parse(&m)
                    .unwrap_or_else(|| panic!("{kind}: 应被识别为补齐事件"));
                assert_eq!(ev.kind(), kind);
                assert_eq!(ev.to_msg(), text, "{kind}: 线上真帧回放后字节不一致（实现有偏差）");
                total += 1;
            }
        }
        assert_eq!(
            total, 352,
            "线上补齐事件帧总数应为 352（fixtures/protocol_live_frames.jsonl 1717 帧的子集）"
        );
    }

    /// `TurnLimitInfo` 的真帧在**冻结语料**里（线上 1717 帧抓不到它）：21 帧 / 21 份回放，
    /// 取值 `{1,5,6,10}`。T46 曾把它记成「零真实帧」，是**在 gzip 压缩字节上 grep** 的假阴性
    /// （T53 §110.5 解压重扫纠正）。
    ///
    /// ⚠️ 这一行的档位是 `观测（回放记录）`：文本是「回放记录的 event 对象 → 紧凑 JSON」**重建**的，
    /// 不是线上负载字节（线上负载抓不到它，见 `docs/06` §110.6 的缺触发条件）。
    #[test]
    fn turn_limit_samples_are_the_corpus_frames() {
        // 21 份回放里该事件的取值，**按快照文件名顺序**（truth3 的 `P/TurnLimitInfo` 段就是按这个顺序算 FNV 的）
        const CORPUS_ORDER: [i64; 21] = [
            5, 5, 10, 10, 10, 6, 1, 6, 6, 6, 5, 5, 5, 6, 6, 5, 5, 5, 10, 10, 6,
        ];
        // 21 条重建文本按上面顺序拼接的 FNV-1a 64 = `P/TurnLimitInfo/fnv1a64`（truth3）
        const CORPUS_FNV: u64 = 0x48e2_39f8_596c_54ce;
        let mut joined: Vec<u8> = Vec::new();
        for value in CORPUS_ORDER {
            let text = format!(r#"{{"TurnLimitInfo":{{"turn_limit":{value}}}}}"#);
            let m = parse_message(&text).expect("重建文本应能解析");
            let ev = AnyEvent::parse(&m).expect("应识别为 TurnLimitInfo");
            assert_eq!(ev.to_msg(), text, "TurnLimitInfo({value}) 往返后字节不一致");
            joined.extend_from_slice(text.as_bytes());
        }
        assert_eq!(fnv1a64(&joined), CORPUS_FNV, "语料里的 TurnLimitInfo 重建文本变了");
        // 取值直方图：{1:1, 5:8, 6:7, 10:5}
        let mut hist = std::collections::BTreeMap::new();
        for v in CORPUS_ORDER {
            *hist.entry(v).or_insert(0usize) += 1;
        }
        assert_eq!(hist, [(1, 1), (5, 8), (6, 7), (10, 5)].into_iter().collect());
    }

    /// 23 个名字一个不多一个不少；三张表（名字 / 字段数 / 抓包档）互相一致。
    ///
    /// ⚠️ **契约断言（表结构不变量），不含语义**（T73 / 计划 R06）：这条只证
    /// 「我方三张表彼此不矛盾」，**不构成复原证据** —— 三张表都是我方的代码。
    /// 每个事件的语义/形状真值在别处：字段数与键序见 `typed_events_have_exactly_the_serde_field_count`
    /// 与 `upgraded_events_declare_the_binary_field_names`（期望值来自抓包字节），
    /// 逐帧回放见 `replays_every_live_frame_of_the_supplemented_kinds`（352 帧逐字节）。
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

    /// 强类型那 13 个：**线上真帧的字段数**必须与我方解析出的字段数逐个相等。
    ///
    /// ## 真值来源（T73 / 计划 R02）
    ///
    /// 改前：期望值 = 我方 `SERDE_FIELD_COUNTS`（人工从 `.rodata` 抄进源码的 N）—— 抄错一位、
    /// 或我方解析把嵌套逗号当分隔符/把空体算成 1 个字段，两边同错就恒绿。
    ///
    /// 改后：期望值 = **抓包字节里数出来的字段数**（`truth4` 的 `L/<事件>/fieldcounts`，
    /// 由 `tools/gen_test_expectations4.py` 从每一帧负载解 JSON 后数顶层字段；每种事件的
    /// 直方图只有一个键 ⇒ 逐帧恒定）。`SERDE_FIELD_COUNTS` 降为**交叉核对**：
    /// `.rodata` 的 N 与真帧实测必须一致（这一句仍然有价值：它把两个独立来源对上）。
    ///
    /// `VersionInfo` 线上 1717 帧没有它 ⇒ 用旧抓包 `fixtures/protocol_match.jsonl` 的 1 帧
    /// （`L/VersionInfo/fixture_*`）。
    ///
    /// 复跑：`.reference/venv/bin/python tools/gen_test_expectations4.py --check`
    #[test]
    fn typed_events_have_exactly_the_serde_field_count() {
        use super::tests::{truth4, truth4_usize};

        assert_eq!(CAPTURED_TYPED_EVENTS.len(), 13);
        assert_eq!(truth4_usize("L", "GameInfo/frames"), 52, "GameInfo 真帧数变了");
        for kind in CAPTURED_TYPED_EVENTS {
            // `VersionInfo` 线上 0 帧 ⇒ 真值来自旧抓包夹具的 1 帧（单独一行）
            if kind == "VersionInfo" {
                let want = truth4_usize("L", "VersionInfo/fixture_fieldcount");
                let line = sample_of(kind);
                let m = parse_message(&line).expect("旧抓包 VersionInfo 应能解析");
                let got = split_fields(m.body.trim()).len();
                assert_eq!(got, want, "VersionInfo: 字段数与旧抓包真帧不符");
                assert_eq!(
                    SERDE_FIELD_COUNTS.iter().find(|(k, _)| *k == kind).and_then(|(_, n)| *n),
                    Some(want), "VersionInfo: `.rodata` 的 N 与旧抓包真帧字段数不一致"
                );
                continue;
            }
            // ① 真值：抓包里数出来的字段数（直方图必须只有一个键）
            let hist = truth4("L", &format!("{kind}/fieldcounts"));
            let mut counts: Vec<usize> = Vec::new();
            for part in hist.trim_matches(['{', '}']).split(',') {
                if part.trim().is_empty() {
                    continue;
                }
                let (k, v) = part.split_once(':').expect("直方图格式");
                let n: usize = k.trim().trim_matches('"').parse().expect("字段数是整数");
                let c: usize = v.trim().parse().expect("计数是整数");
                for _ in 0..c {
                    counts.push(n);
                }
            }
            let frames = live_payloads(kind).len();
            assert_eq!(counts.len(), frames,
                       "{kind}: 真值表的字段数样本数与实际帧数不一致（夹具被换过）");
            assert!(
                counts.windows(2).all(|w| w[0] == w[1]),
                "{kind}: 抓包里的字段数不是恒定的（{hist}）⇒ 不能直接钉一个数"
            );
            let want = counts[0];

            // ② 实测：我方在这条事件**每一帧**上数出来的字段数
            for payload in live_payloads(kind) {
                let line = std::str::from_utf8(&payload).expect("线上负载应是 UTF-8");
                let m = parse_message(line).expect("线上帧应能解析");
                let body = m.body.trim();
                let got = if body.is_empty() { 0 } else { split_fields(body).len() };
                assert_eq!(got, want,
                           "{kind}: 线上真帧的字段数 {got} 与抓包真值 {want} 不符");
            }
            let line = sample_of(kind);
            let m = parse_message(&line).expect("样本应能解析");
            let body = m.body.trim();
            let got = if body.is_empty() { 0 } else { split_fields(body).len() };
            assert_eq!(got, want, "{kind}: 样本字段数与真帧不符");

            // ③ 交叉核对：`.rodata` 的 N 与真帧实测要一致（不一致 ⇒ 两个来源打架，必须查）
            let rodata = SERDE_FIELD_COUNTS
                .iter()
                .find(|(k, _)| *k == kind)
                .and_then(|(_, n)| *n);
            assert_eq!(rodata, Some(want),
                       "{kind}: `.rodata` 的 serde N 与线上真帧实测字段数不一致");
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
    ///
    /// ## T73 / 计划 R03：键序的期望值从「我方 WIRE_SPEC」换成「抓包真帧」
    ///
    /// 改前：期望键序 = 我方 `wire_field_order(kind)`（`WIRE_SPEC` 表，人工从指令流抄的）。
    /// 那份表与实现若同时错（键名拼错 / 键序颠倒），往返测试是拿**同一个 `sample_of`** 比的 ⇒
    /// 恒绿。改后：
    ///
    /// * **7 个有真帧的**（LockInfo / LockRequest / ChatMessage / ChatMessageRequest /
    ///   GameRemoved / ClearObstacles + 语料里的 `TurnLimitInfo` 21 帧）：期望键序取
    ///   `truth4` 的 `L/<事件>/keyorder`（`tools/gen_test_expectations4.py` 从**负载字节**解
    ///   JSON 得到；每种事件现场键序唯一）；
    /// * **3 个零真帧的**（`GameHeartbeat` / `ClearBansRequest` / `BanConnectionRequest`）：
    ///   线上 1717 帧 0 帧 ⇒ **仍是 `WIRE_SPEC`（推断）**，逐条注明。
    #[test]
    fn upgraded_events_declare_the_binary_field_names() {
        use super::tests::truth4;

        let upgraded: Vec<&str> = SUPPLEMENTED_EVENT_NAMES
            .iter()
            .copied()
            .filter(|k| !CAPTURED_TYPED_EVENTS.contains(k))
            .collect();
        assert_eq!(upgraded.len(), 10);
        // 三个零真帧事件（T53 三会话 1717 帧全无）⇒ 键序只能来自 WIRE_SPEC（推断）
        const NO_LIVE_FRAME: [&str; 3] =
            ["GameHeartbeat", "ClearBansRequest", "BanConnectionRequest"];
        let mut from_capture = 0usize;
        for kind in upgraded {
            let line = sample_of(kind);
            let m = parse_message(&line).expect("样本应能解析");
            let ev = AnyEvent::parse(&m).expect("应识别");
            assert!(
                ev.extra_fields().is_empty(),
                "{kind}: 已知字段应全部进强类型槽位，extra 必须为空"
            );
            let spec_order = wire_field_order(kind)
                .unwrap_or_else(|| panic!("{kind} 不在 WIRE_SPEC 里"));
            let out = ev.to_msg();
            assert_eq!(out, line, "{kind} 往返后字节不一致");
            let m2 = parse_message(&out).expect("重新解析");
            let got: Vec<String> =
                split_fields(&m2.body).into_iter().map(|(k, _)| k).collect();
            let spec: Vec<String> = spec_order.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, spec, "{kind} 的键序与 WIRE_SPEC 不一致");

            // ★ 真值侧：有真帧的事件，键序必须等于**抓包现场键序**
            if NO_LIVE_FRAME.contains(&kind) {
                // 零真帧：明确标推断（真值表里 frames=0）
                assert_eq!(truth4("L", &format!("{kind}/frames")), "0",
                           "{kind} 原先零真帧，现在有真帧了 ⇒ 该条可以再升一档，请更新本测试");
                continue;
            }
            // `TurnLimitInfo` 线上 0 帧，真帧在**冻结语料**里（21 帧）⇒ 键序取 `T/` 段
            let live_key = if kind == "TurnLimitInfo" {
                "T".to_string()
            } else {
                "L".to_string()
            };
            let live_key2 = if kind == "TurnLimitInfo" {
                "turn_limit_keyorder".to_string()
            } else {
                format!("{kind}/keyorder")
            };
            let live_order: Vec<String> =
                truth4(&live_key, &live_key2)
                    .split('|')
                    .filter(|s| !s.is_empty()) // 空体事件（ClearObstacles）的键序就是空列表
                    .map(str::to_string)
                    .collect();
            assert_eq!(spec, live_order,
                       "{kind} 的键序与**抓包真帧**不一致（WIRE_SPEC 可能抄错了）");
            assert_eq!(got, live_order,
                       "{kind} 序列化出的键序与抓包真帧不一致");
            from_capture += 1;
        }
        assert_eq!(from_capture, 7,
                   "应有 7 个事件的键序被钉在抓包字节上（LockInfo/LockRequest/ChatMessage/\
                    ChatMessageRequest/GameRemoved/ClearObstacles/TurnLimitInfo）");
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
    ///
    /// ⚠️ **契约断言，不含语义**（P26）：真流量里不会有非法帧，原版的失败路径没抓到；
    /// 这条钉的是「我方解析器不许把不懂的事件当成已知事件吞下去」。
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
    ///
    /// ⚠️ **契约断言，不含语义**（P25）：原版 serde 的未知字段行为没有抓到（真流量里不会有
    /// 我们不认识的字段）；这条钉的是**我方 `_scaffold::protocol` 的接缝契约** —— 客户端转发
    /// 一帧它不认识的字段时不能丢。**不能当复原证据。**
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
    /// ⚠️ **契约断言，不含语义**（P27）：1717 帧线上真帧里没有一条带引号/转义字符的
    /// `NameInfo`/`ChatMessage`（`ChatMessage` 正文还被 T53 打码成 `<len=N>`）⇒ 这条的输入是
    /// 构造的，不是观测到的报文。
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

    /// 真帧里机器人的段位是 `null`、人类的段位是整数：`Option` 必须把 `None` 发回成 `null`。
    ///
    /// **真值来源（P28）**：线上真帧 14 条 `RankInfo`（`fixtures/protocol_live_frames.jsonl`），
    /// 既有 `"rank":null`（bot）也有 `"rank":0`（人类）——本测试直接从抓包取这两条原文，
    /// 而不是用构造串。
    #[test]
    fn rank_null_round_trips_as_json_null() {
        let payloads = live_payloads("RankInfo");
        assert_eq!(payloads.len(), 14, "线上 RankInfo 帧数变了");
        let mut seen_null = 0;
        let mut seen_int = 0;
        for p in &payloads {
            let line = std::str::from_utf8(p).expect("UTF-8");
            let m = parse_message(line).expect("线上 RankInfo 应能解析");
            let AnyEvent::RankInfo(r) = AnyEvent::parse(&m).expect("应识别") else {
                panic!("应解析为 RankInfo")
            };
            if line.contains(r#""rank":null"#) {
                assert_eq!(r.rank, None, "线上 rank:null 应解析成 None");
                seen_null += 1;
            } else {
                assert!(r.rank.is_some(), "线上整数段位应解析成 Some：{line}");
                seen_int += 1;
            }
            assert_eq!(r.to_msg(), line, "线上 RankInfo 往返后字节不一致");
        }
        assert!(seen_null > 0, "线上应至少有一条 rank:null（bot）");
        assert!(seen_int > 0, "线上应至少有一条整数 rank（人类）");

        let r = RankInfo { entity_id: 229, rank: Some(0), extra: Vec::new() };
        assert_eq!(r.to_msg(), r#"{"RankInfo":{"entity_id":229,"rank":0}}"#);
    }

    /// 强类型事件的字符串字段：带引号的名字要能转义写出、解转义读回。
    ///
    /// ⚠️ **契约断言，不含语义**（P29）：线上 20 条 `NameInfo` 里没有带引号的名字 ⇒
    /// 输入是构造的；真帧覆盖的是「普通字符串逐字节回放」（见 352 帧硬闸）。
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
    ///
    /// ⚠️ **契约断言，不含语义**：真流量里没有缺字段/类型错的帧。
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
