//! `graphwar_replay_reader`：回放**文件层**——列目录名、读一个文件、gunzip、拆 JSONL。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 它和 `graphwar_replay_player` 的分工（本模块要回答的问题 (b)）
//!
//! | 层 | 模块 | 干什么 | 碰文件？ | 碰时钟？ |
//! |---|---|---|---|---|
//! | 文件层 | **本模块** `graphwar_replay_reader` | `user://replays/` 列名、读字节、解 gzip、逐行 JSON → `Vec<EventInfo>` | **是** | **不碰** |
//! | 播放层 | `graphwar_replay_player` | 拿 `Vec<EventInfo>` 按 tick 还原局面、推进 `tick += dt*60*speed` | 不碰 | **是** |
//!
//! 判据全在 arm64 的**被调者**上（§3.2）：本模块两个函数只调
//! `DirAccess` / `FileAccess` / `flate2` / `serde_json` / `str::pattern::StrSearcher`，
//! **一个时间函数都没有**（无 `Instant`、无 `chrono`、无 tick 运算）；
//! 反过来 `graphwar_replay_player` 侧负责 tick ⇄ 事件的换算。
//!
//! 这也解释了 `graphwar_replay_control`（回放界面）为什么把 `load_games`/`load_game`
//! 放在**后台线程**里：它们是**阻塞文件 I/O**，不是纯计算（见该模块文档 §2）。
//!
//! ## 2. 函数清单
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 比 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `load_games` | `0x3da190` / 480 | `0x3d76f0` / 608 | 1.267 | `DirAccess::get_files_at` → `Vec<String>` | **观测** |
//! | `load_game`  | `0x3da370` / 1612 | `0x3d5740` / 2368 | 1.469 | 打开 → 读字节 → gunzip → 逐行 JSON → `Vec<EventInfo>` | **观测** |
//!
//! 两个尺寸比**都超出** 0.61–1.19 的闸，所以对位**不看尺寸**，靠一条判等：
//!
//! > `"user://replays/"`（15 字节）在**整份** x86 `.so` 里只被 **2 个**函数引用
//! > （`0x3d5740`、`0x3d76f0`），整份 arm64 `.so` 里也只在**本模块这两个函数**里出现
//! > （`.rodata 0x19bdef`，`w1 = 0xf`）。二对二，且长函数↔长函数
//! > （`1612 > 480` / `2368 > 608`）⇒ 配对唯一。
//!
//! 哪一个是哪个还有一条独立证据：`load_games` 建的是**元素 8 字节**的 `Vec`
//! （arm64 `0x3da200 lsl x23, x21, #3`；x86 `0x3d7727 mov $0x8,%r14d`）——
//! 8 字节 = `GString` 的大小；`load_game` 建的是 1 字节元素的字节缓冲
//! （`alloc_zeroed` + `PackedArray<u8>` 的 `ffi_index_const`）。
//!
//! ## 3. 逐位事实
//!
//! | # | 事实 | 证据 |
//! |---|---|---|
//! | 1 | 回放目录字面量 = `"user://replays/"`，**15 字节、自带结尾斜杠**（所以拼名时不插分隔符） | arm64 `.rodata 0x19bdef` + `mov w1,#0xf`（`0x3da1bc`）；x86 `0x1bf7d7` + `mov $0xf,%edx`/`%esi` |
//! | 2 | `load_games` 用 **静态** `DirAccess::get_files_at(path)`（不先 new 一个 DirAccess），返回 `PackedArray<GString>` | arm64 `0x3da1c0` → `godot_core::…DirAccess::get_files_at` |
//! | 3 | 返回 **`Vec<String>`**（名字，不含目录、不含扩展名——目录里本来也没有扩展名） | arm64 `0x3da200` 8 字节步长 + `Vec::from_iter`（`0x3da294`） |
//! | 4 | `load_game` 用 `FileAccess::open(path, mode=1)`；Godot 的 `Mode` 里 **1 = READ** | arm64 `0x3da400 mov w1,#1` + `0x3da404` |
//! | 5 | 读完立刻 `unreference()` 关句柄（`RefCounted`），**先解压后关** | arm64 `0x3da548`（在 `0x3da540 check_rtti` 之后） |
//! | 6 | 解压是 `flate2::gz::bufread::GzDecoder` + `std::io::append_to_string` ⇒ **整份解成一个 `String`**，不是流式 | arm64 `0x3da48c` + `0x3da4e4` |
//! | 7 | 逐行用 `str::pattern::StrSearcher`（即 `str::lines()`/`split('\n')`），**不是** `serde_json::StreamDeserializer` | arm64 `0x3da628 StrSearcher::new` + `0x3da69c next_match` |
//! | 8 | 每行 `serde_json` 反序列化成 **`graphwar_game_recorder::EventInfo`** | arm64 `0x3da714 serde_json::de::from_trait`；`drop_in_place<Result<EventInfo,serde_json::Error>>` @ `0x3ea8c0` |
//! | 9 | 单行失败**不中断**：`Result` 被 `drop` 掉，循环继续（丢一行） | arm64 `0x3da684 drop_in_place<Result<…>>` 后直接跳回循环头（`0x3da688`） |
//! | 10 | 结果用 `RawVec::grow_one` 逐条 push，**不预分配** | arm64 `0x3da734` |
//!
//! ### 3.1 回放文件长什么样（`docs/78` §2 / `docs/133` §2，**观测**）
//!
//! ```text
//! gzip 压缩；解压后每行一条：
//! {"event":{"FunctionInfo":{"owner_id":635,"function":"(-0.35*(-11.40+x))","corrected_function":null}},"tick":1234}
//! ```
//!
//! 内层 `{"<EventName>":{…}}` 与网络协议**同一套名字与字段**，所以逐行解析**复用**
//! `_scaffold::protocol` 的那一层（[`protocol::parse_message`]），不另写一套 JSON 解析器。
//!
//! ### 3.2 被调者（arm64 `bl` 目标穷举，**观测**）
//!
//! `load_games`：`DirAccess::get_files_at`、`PackedElement::{op_is_empty, ffi_index_const,
//! op_size, ffi_destroy}`、`builtin::to_i64/to_usize`、`Vec::from_iter`。
//!
//! `load_game`：`FileAccess::open`、`FileAccess::get_buffer`、`RawGd::check_rtti`、
//! `RefCounted::unreference`、`flate2::gz::bufread::GzDecoder::new`、
//! `std::io::append_to_string`、`StrSearcher::{new,next_match}`、`serde_json::de::from_trait`、
//! `RawVec::grow_one`。**没有任何时间/tick 函数**。
//!
//! ## 4. 未知
//!
//! * `load_games` 是否**过滤**（例如跳过空名、只留时间戳命名的项）：arm64 里能看到
//!   `op_is_empty` + `ffi_index_const(0)`，但那是 Rust 迭代 `PackedArray` 的样板，
//!   **不是**业务过滤 ⇒ 本实现不过滤（目录里本来只有 `<unix 秒>` 这样的名字）；
//! * `load_game` 的**参数类型**：`&str`（名字）还是 `GString`？arm64 里
//!   `ldp x21,x22,[x19,#8]` 取的是 `(ptr,len)` ⇒ **`&str`**（**推断**，因为
//!   `GString` 是 8 字节单字、不会有 `(ptr,len)` 拆分）；
//! * 非 gzip 文件的处理：arm64 无条件建 `GzDecoder`（没有 `is_gzip` 分支）⇒
//!   原版**只认 gzip**。本实现保留 `is_gzip` 以便对拍（`gzip 与否` 的判定在
//!   `_scaffold::replay_inflate` 里），行为等价但多一次判断（**推断**）。
//!
//! ## 5. 复现（不需要 Godot）
//!
//! ```text
//! python3 tools/replay_extract.py <回放文件>      # 打印解压后的 JSONL
//! ```
//!
//! 本模块的 `load_game` 等价于：读文件 → `gunzip` → 按行 → 每行取 `event`/`tick`。

use crate::_scaffold::protocol::{self, RawMessage};
use crate::_scaffold::replay_inflate::{gunzip, is_gzip, InflateError};
use crate::graphwar_game_recorder::EventInfo;

/// 回放目录。原版是**字面量**（不是拼出来的），**自带结尾斜杠**，15 字节。
///
/// 档位：**逐位**（arm64 `.rodata 0x19bdef` + `mov w1,#0xf`；x86 `0x1bf7d7` + `mov $0xf`）。
pub const REPLAY_DIR: &str = "user://replays/";

/// [`REPLAY_DIR`] 的字节数（指令里的立即数 `0xf`）。档位：**观测**（`0x3da1bc`）。
pub const REPLAY_DIR_LEN: usize = 15;

/// `FileAccess::open` 的模式实参。Godot 的 `Mode::READ` = **1**。
///
/// 档位：**观测**（arm64 `0x3da400 mov w1,#1`）；"1 就是 READ"是 Godot 枚举口径（**推断**）。
pub const FILE_MODE_READ: i32 = 1;

/// 读一个回放的失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayReadError {
    /// 文件不存在 / 读不动。
    Io,
    /// gzip 解压失败。
    Inflate(InflateError),
    /// 解压出来不是 UTF-8。
    NotUtf8,
    /// 一行都没解析出来。
    Empty,
}

impl ReplayReadError {
    /// 稳定的短名（测试与日志用）。
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayReadError::Io => "io",
            ReplayReadError::Inflate(_) => "inflate",
            ReplayReadError::NotUtf8 => "not-utf8",
            ReplayReadError::Empty => "empty",
        }
    }
}

/// 回放文件名 → Godot 路径。`REPLAY_DIR` 自带斜杠，所以**不插**分隔符。
///
/// 与 `graphwar_game_recorder::replay_path` 的 `user://replays/<unix 秒>` 同一个口径；
/// 区别是这个函数接受**任意**名字（回放界面拿到的是目录里已有的名字）。
pub fn replay_file(name: &str) -> String {
    let mut s = String::with_capacity(REPLAY_DIR_LEN + name.len());
    s.push_str(REPLAY_DIR);
    s.push_str(name);
    s
}

/// `load_games`（arm64 `0x3da190`）：把目录里已有的名字收成 `Vec<String>`。
///
/// 原版自己调 `DirAccess::get_files_at(REPLAY_DIR)`；这里把**目录列表**作为参数注入
/// （Godot 运行时才有 `DirAccess`），语义等价：原版也只是"遍历 → 逐个 push"。
/// 空名字跳过（见模块文档 §4 的**未知**）。
pub fn load_games<I, S>(files: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    files
        .into_iter()
        .map(Into::into)
        .filter(|n: &String| !n.is_empty())
        .collect()
}

/// `load_game`（arm64 `0x3da370`）：**一个文件的字节** → 一局的事件表。
///
/// 与原版逐条对齐：能 gzip 就 `GzDecoder` 解（原版**无条件**解，见 §4），
/// 解完按行切，每行取 `event` + `tick`，失败的**行**丢掉继续（事实 9）。
pub fn load_game_bytes(name: &str, bytes: &[u8]) -> Result<Vec<EventInfo>, ReplayReadError> {
    let path = replay_file(name);
    load_game_at(&path, bytes)
}

/// 同上，但路径已经拼好（原版 `FileAccess::open` 收到的就是拼好的 GString）。
pub fn load_game_at(_path: &str, bytes: &[u8]) -> Result<Vec<EventInfo>, ReplayReadError> {
    let raw: Vec<u8> = if is_gzip(bytes) {
        gunzip(bytes).map_err(ReplayReadError::Inflate)?
    } else {
        bytes.to_vec()
    };
    let text = String::from_utf8(raw).map_err(|_| ReplayReadError::NotUtf8)?;
    let events = parse_event_lines(&text);
    if events.is_empty() {
        return Err(ReplayReadError::Empty);
    }
    Ok(events)
}

/// 从（已解压的）文本里逐行切事件。**坏行跳过**，与事实 9 一致。
pub fn parse_event_lines(text: &str) -> Vec<EventInfo> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(ev) = parse_event_line(line) {
            out.push(ev);
        }
    }
    out
}

/// 一行 `{"event":{…},"tick":N}` → [`EventInfo`]。
///
/// 复用 [`protocol::parse_message`]：先解外层拿 `event` 的对象原文，再解内层拿事件名
/// 与载荷（`RawMessage.body` 保持原文，**不重排**）。
pub fn parse_event_line(line: &str) -> Option<EventInfo> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let outer = protocol::parse_message(line)?;
    if outer.kind != "event" {
        return None;
    }
    // 事件本体：`outer.body` 是 `"<Name>":{…}`（不含外层花括号，且到第一个对象就截断），
    // 补一对括号就是一条标准单键消息。
    let ev = protocol::parse_message(&format!("{{{}}}", outer.body))?;
    // `tick` 在**整行**里（`outer.body` 已经截断了 `,"tick":N`），所以回到原文找。
    let tick = int_field(&outer.text, "tick")?;
    if tick < 0 {
        return None;
    }
    Some(EventInfo {
        tick: tick as u32,
        event: ev.kind,
        body: ev.body,
    })
}

/// `"<key>":<整数>` —— 与 `_scaffold::protocol::get_int` 同口径，但吃**原文**而不是
/// `RawMessage.body`（`event` 外层解析会把 `,"tick":N` 截掉）。
fn int_field(text: &str, key: &str) -> Option<i64> {
    let pat = format!("\"{}\":", key);
    let i = text.find(&pat)? + pat.len();
    let rest = text[i..].trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// 只判断"这条事件之后 tick 还能往前走"——给播放层判断是否已到末尾用。
pub fn last_tick(events: &[EventInfo]) -> Option<u32> {
    events.iter().map(|e| e.tick).max()
}

/// 事件名清单（回放界面要显示"这一局有什么"时的最小信息）。
pub fn event_names(events: &[EventInfo]) -> Vec<&str> {
    events.iter().map(|e| e.event.as_str()).collect()
}

/// 便于把 `&RawMessage` 的载荷按名字取回来（与 `_scaffold::protocol::get_str` 同口径）。
pub fn payload(msg: &RawMessage) -> &str {
    msg.body.as_str()
}
