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
//! 落码位置（2026-11-28 T109）：`load_games` → [`load_games`]（目录列表注入）；
//! `load_game` → [`load_game`]（原版语义：入参是**名字**、无条件 gunzip、失败返回空 `Vec`），
//! 另有对拍友好的超集 [`load_game_at`] / [`load_game_bytes`]（接受明文、返回 `Result`）。
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
use crate::graphwar_game_recorder::{EventInfo, GraphwarEvent};

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

/// `FileAccess` 类名字面量：原版对 `FileAccess::open` 的返回值做 `check_rtti` 时用的就是它。
///
/// 档位：**观测**（arm64 `.rodata 0x19bdff` = `"FileAccess"`；`0x3da41c adrp/add` 取址 +
/// `0x3da428 mov w2,#0xa` = 长度 10）。
pub const FILE_ACCESS_CLASS: &str = "FileAccess";

/// `FileAccess::get_buffer` 的实参：**一次性最多读 10 000 000 字节**（`0x989680`）。
///
/// 档位：**观测**（arm64 `0x3da430 mov w1,#0x9680` + `0x3da43c movk w1,#0x98,lsl #16`
/// ⇒ `w1 = 0x0098_9680 = 10_000_000`；紧接 `0x3da440 bl FileAccess::get_buffer`）。
///
/// ⇒ 原版**不是流式读文件**：整份文件一次塞进 `PackedByteArray`（上限 10 MB）。
/// 一个 401 份回放的快照里最大的那份也远小于这个数（`docs/06` §153）。
pub const FILE_READ_CAP: u32 = 10_000_000;

/// 包住**已读字节**的读侧缓冲容量：`0x8000` = 32768 字节。
///
/// 档位：**观测**（arm64 `0x3da464 mov w0,#0x8000` + `0x3da46c bl __rust_alloc_zeroed`；
/// 这块缓冲随后作为 `GzDecoder::new` 的 `R`（`0x3da488 add x8, sp,#0x1b0` 传进去）
/// ⇒ 那是 `BufReader<&[u8]>` 的容量，**不是**解压输出缓冲）。
pub const GZ_READ_BUFFER: usize = 0x8000;

/// 逐行切分的分隔符：**单个 `\n`**（1 字节）。
///
/// 档位：**观测**（arm64 `.rodata 0x19bdfe` 处正是 `0x0a`；`0x3da610 adrp/add` 取址 +
/// `0x3da620 mov w3,#1` 传 `StrSearcher::new` 的 needle 长度 ⇒ `split('\n')`，
/// 不是 `lines()` 的 `\r\n` 兼容版）。
pub const LINE_SEP: &str = "\n";

/// 原版 `EventInfo` 的**机器字大小**：80 字节。
///
/// 档位：**观测**（arm64 `0x3da668 mov w24,#0x50` + `0x3da73c madd x8, x27, x24, x23`
/// = `base + len * 0x50` —— 逐条 push 时针步用的就是它；`0x3da734 bl grow_one` 的
/// 元素类型即 `graphwar_game_recorder::EventInfo`）。
///
/// ⚠️ 我方 `EventInfo` 是 `{GraphwarEvent{name:&str, body:String}, tick:u32}`，尺寸**不同**；
/// 这个常数只用于说明原版的内存形状，不参与我方实现。
pub const EVENT_INFO_SIZE: usize = 0x50;

/// **原版**入口 `GraphwarReplayReader::load_game`（arm64 `0x3da370`，**1612 字节**）。
///
/// 语义链（每一步都有地址，见模块文档 §3 的 10 条事实 + 本轮补的 5 条常数）：
///
/// 1. `0x3da3a8 mov w0,#0xf` / `0x3da3b8 adrp 0x19b000 + add #0xdef` ⇒ 分配 15 字节并拷
///    `"user://replays/"`；`0x3da3e8 add x8, x23, x22` = `15 + name.len()` ⇒ **目录由本函数自己拼**，
///    不插分隔符（字面量自带斜杠）。⇒ 入参是**名字**，不是完整路径。
/// 2. `0x3da400 mov w1,#1` + `0x3da404 bl FileAccess::open` ⇒ 模式 1（= READ）。
/// 3. `0x3da410 cmp x8,#1` / `b.ne 0x3da578` ⇒ `open` 失败就跳到**返回初始空 `Vec`** 的那条臂
///    （`0x3da39c stp xzr, x8` 就是那个 `{cap=0,len=0,ptr=8}` 的空 Vec）。
/// 4. `0x3da430/0x3da43c` ⇒ `get_buffer(FILE_READ_CAP)`；`0x3da448 op_is_empty` 空文件同样走空 Vec。
/// 5. `0x3da48c bl GzDecoder::new`（**无条件**，没有 `is_gzip` 分支）+ `0x3da4e4 bl append_to_string`
///    ⇒ 解压失败 ⇒ `0x3da4f0 drop_in_place<std::io::Error>` 后直接返回**空 Vec**（不是 `Result`）。
/// 6. `0x3da548 bl RefCounted::unreference` ⇒ 解压**之后**才关句柄。
/// 7. `0x3da628 StrSearcher::new`(needle=`"\n"`) + `0x3da69c next_match` ⇒ 按 `\n` 切。
/// 8. `0x3da714 serde_json::de::from_trait` ⇒ 每行一个 `EventInfo`；失败 ⇒ `0x3da684`
///    `drop_in_place<Result<EventInfo, serde_json::Error>>` 后回到循环头（`0x3da690`）⇒ **坏行丢、不中断**。
/// 9. `0x3da734 grow_one` ⇒ 逐条 push，**不预分配**。
/// 10. 返回 `Vec<EventInfo>`（`0x3da578` 起的 sret 写 24 字节）—— **没有 `Result`**。
///
/// 本实现把 Godot 的 `FileAccess::open` + `get_buffer` 两步**注入为 `bytes` 参数**
/// （测试环境没有 Godot；`load_games` 对 `DirAccess` 也是同样的注入口径）。
/// ⇒ `name` → `bytes` 之间的读文件动作是**推断**（缺：真机 `FileAccess` 调用序列的运行时读数），
/// 其余 9 步全部**观测**。
///
/// 与 [`load_game_at`] 的区别（两者都在，别混用）：
/// * 本函数 = **原版语义**：无条件 gunzip、失败返回空 Vec、入参是**名字**；
/// * [`load_game_at`] = 对拍/测试友好的**超集**：接受明文、返回 `Result`、入参是**完整路径**。
pub fn load_game(name: &str, bytes: &[u8]) -> Vec<EventInfo> {
    // ① 目录自己拼（0x3da3a8/0x3da3b8/0x3da3e8）；路径本身只用于错误语境，不参与解码
    let _path = replay_file(name);
    // ② 无条件 gzip：解压失败 ⇒ 空 Vec（0x3da4e4 append_to_string 的 Err 臂）
    let Ok(raw) = gunzip(bytes) else {
        return Vec::new();
    };
    // ③ `append_to_string` 只接受 UTF-8；非 UTF-8 ⇒ 同一 Err 臂 ⇒ 空 Vec
    let Ok(text) = String::from_utf8(raw) else {
        return Vec::new();
    };
    // ④ split('\n') + 每行 serde_json；坏行只丢那一行（0x3da684 → 0x3da690）
    let mut out: Vec<EventInfo> = Vec::new();
    for line in text.split(LINE_SEP) {
        if let Some(ev) = parse_event_line(line) {
            out.push(ev);
        }
    }
    out
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
        // 声明顺序 = 二进制的字段顺序：`event` 在前、`tick` 在后（`docs/06` §103.3）。
        event: GraphwarEvent { name: ev.kind, body: ev.body },
        tick: tick as u32,
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
    events.iter().map(|e| e.name()).collect()
}

/// 便于把 `&RawMessage` 的载荷按名字取回来（与 `_scaffold::protocol::get_str` 同口径）。
pub fn payload(msg: &RawMessage) -> &str {
    msg.body.as_str()
}
