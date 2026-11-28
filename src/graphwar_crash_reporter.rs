//! 原版 `src/graphwar_crash_reporter.rs` —— **崩溃日志的上报端**（2026 版才有的模块）。
//!
//! ## 为什么 2024 版里找不到它（`观测`）
//!
//! 2024-07-30 首发周那一版（本仓库旧文档里的 `6eee7bb8…`）**0 个符号、0 个字符串**；
//! 2026 版有 **8 处** `src/graphwar_crash_reporter.rs:NN` 字面量，
//! 而其它模块各只有 1 处 —— 这是**唯一**一个"两份二进制字符串计数不同"的游戏模块，
//! 所以它确实是**后来加的功能**，不是我们上一轮漏扫。
//!
//! ## 三个函数（arm64 2.0.60 符号，`观测`）
//!
//! | 符号 | arm64 | 字节 | 干什么 |
//! |---|---|---|---|
//! | `GraphwarCrashReporter::send_pending_report` | `0x4ca444` | 1136 | 起一个**线程**把待发崩溃报告送出去 |
//! | `GraphwarCrashReporter::resolve_default_address` | `0x4ca8b4` | 980 | 决定"发到哪" |
//! | `GraphwarCrashReporter::clear_report` | `0x4ca340` | 260 | 把崩溃日志清空 |
//!
//! 结构体本身没有 `new`：它是**自由函数集合**，不持有状态（日志在磁盘上）。
//!
//! ## 发到哪（`逐位` + `观测`）
//!
//! `resolve_default_address` 里出现 **`mov w8,#0xa7dc`（= 42972）两次** ⇒
//! **`CRASH_REPORT_PORT = 42972`**，与 CLI 帮助文本里的
//! `e.g. 127.0.0.1:42972` 完全对得上（`.rodata` 里的 clap 帮助串）。
//!
//! 解析顺序（三条字符串各自对应一条失败路径，`观测`）：
//!
//! 1. CLI / 环境变量 `--crash_report_address` / `CRASH_REPORT_ADDRESS`
//!    （clap 参数名 `crash_report_address`，命令行风格 `crash-report-address`）。
//!    解析失败 → `"Failed to parse --crash_report_address: "`。
//! 2. 否则用**全局服务器地址** + 端口 `42972`
//!    （帮助文本：*"If not specified, the game's global server address is reused
//!    with `CRASH_REPORT_PORT`"*）。
//!    全局地址来自 `https://graphwar.com/graphwar_II_ip.txt`
//!    —— 这个 URL 在哪都出现：`BROKER_ADDRESS` 的帮助文本、
//!    `"Failed to get global server address for crash reporting."`。
//!    拉不到 / 解析不了 → `"Failed to parse global server address for crash reporting: "`。
//! 3. 两条都没有 ⇒ `send_pending_report` 记一条
//!    `"Crash report pending but no crash report server address is available."`
//!    然后**什么都不发**（日志留在磁盘上等下次）。
//!
//! ## 什么时候装钩子（`观测`，arm64 `bl` 图）
//!
//! **`GraphwarRootNode::ready`** 里连着调两个（`ready` 是 7932 字节的启动函数）：
//!
//! ```text
//! <GraphwarRootNode as INode>::ready +0x30  →  backtracer::Backtracer::install_panic_log_hook
//! <GraphwarRootNode as INode>::ready +0x44  →  graphwar_crash_reporter::GraphwarCrashReporter::send_pending_report
//! ```
//!
//! 顺序很重要：**先装钩子，再补发上一次的**。⇒ 游戏**每次进主场景**都会装一次，
//! 而不是 `main()` 里一次性的（`ready` 会被 Godot 反复回调）。
//!
//! ## 一条日志的生命周期
//!
//! ```text
//! 进程 panic
//!   └─ backtracer 装的 hook：往 user://crash_log.txt 追加一行
//!       （`%Y-%m-%d %H:%M:%S` + OS 名 + 位置 + 消息；见 crate::backtracer）
//! 下次进主场景
//!   └─ ready → send_pending_report
//!        ├─ 日志为空 / 不存在 → 直接返回
//!        ├─ 解析地址失败      → log 一条，返回
//!        ├─ spawn 线程：连 TCP 发出去
//!        │    ├─ 连不上 → "Failed to connect to crash report server at "
//!        │    ├─ 发失败 → "Failed to send crash report to "
//!        │    └─ 成功   → "Crash report sent to "
//!        └─ clear_report 清空日志（失败 → "Failed to clear crash log at "）
//! ```

/// 崩溃日志落盘位置。`逐位`：`.rodata` 字面量 `user://crash_log.txt`；
/// arm64 `0x1c2b89` 被 **3** 条指令引用，分别落在 `backtracer` 的 hook 与
/// `clear_report` 里（x86 目标：`0x4b8bd0` ×1、`0x4c5fa0` ×2）。
pub const CRASH_LOG_PATH: &str = "user://crash_log.txt";

/// 上报端口。`逐位`：`resolve_default_address` 里 `mov w8,#0xa7dc`（42972）。
pub const CRASH_REPORT_PORT: u16 = 42972;

/// 全局服务器地址的来源（`BROKER_ADDRESS` 与崩溃上报共用）。`逐位`：`.rodata` 39 字节字面量。
pub const GLOBAL_SERVER_URL: &str = "https://graphwar.com/graphwar_II_ip.txt";

/// clap 参数名（`逐位`：`.rodata` 的 clap 定义块，`0x1bdaaf` 起）。
pub const CLI_ARG_SNAKE: &str = "crash_report_address";
/// 环境变量名（同块）。
pub const ENV_VAR: &str = "CRASH_REPORT_ADDRESS";
/// 命令行风格写法（同块）。
pub const CLI_ARG_KEBAB: &str = "crash-report-address";

/// 时间戳格式串 `%Y-%m-%d %H:%M:%S`（`逐位`：`.rodata` 字面量在 arm64 `0x1a177c`，17 字节）。
///
/// ⚠️ **T106 更正（2026-11-28）**：旧注释写"被本模块与 `backtracer` 共用"—— **不成立**。
/// 全 `.text` 里指向 `0x1a177c` 的 `adrp+add` 只有 **1 处**（`0x4cb478`/`0x4cb47c`），
/// 所属函数是 `graphwar_replay_control::GraphwarReplayScreenControl::process`
/// （`0x4cb138`，回放播放时钟那一段），**不是**本模块、也不是 `backtracer`。
/// 本模块与 `backtracer` 的时间戳都是 **`chrono::Utc::now()` 的默认 Display**
/// （`clear_report` `0x4ca36c bl Utc::now` + `0x4ca3b8 DateTime<Tz> as Display::fmt`；
///  hook 闭包 `0x4bf0b4 bl Utc::now` + `0x4bf0bc` 同一 fmt）。
/// 复跑：`llvm-objdump -d --no-show-raw-insn <.so> | grep -B1 '#0x77c'`（只该有一处 `adrp …0x1a1000`）。
/// ⇒ 这个常量留在本模块只因为它确实是同一份构建里的字面量，**不被本模块任何函数引用**。
pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// `clear_report` 的动作：**把日志truncate 成空**。`逐位`：
/// arm64 `0x4ca35c` `bl std::fs::write::inner`，随后紧跟
/// `chrono::offset::utc::Utc::now` + `format_inner` ⇒ 写失败才拼错误串。
pub const CLEAR_WRITES_EMPTY: bool = true;

/// 上报地址的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashReportTarget {
    /// 有了：`host:port`
    Address(String),
    /// 待发但没有地址（原版只是记一条日志）
    PendingWithoutAddress,
}

/// `resolve_default_address` 的**语义**还原：给 CLI / 环境变量 / 全局地址，决定发到哪。
///
/// 原版只用 `ToSocketAddrs`（`resolve_default_address` 里 `to_socket_addrs` 的
/// 单态化在 `GraphwarLogger::new` 里也能看到）把字符串变成 `SocketAddr`，
/// 所以这里同样**只做字符串拼接**，不引入 DNS。
///
/// * `cli`/`env` 里若已经带 `:port`，原样用；不带则补 [`CRASH_REPORT_PORT`]
///   （`推断`：帮助文本写的是 "Address of the crash report collector server
///   (e.g. `127.0.0.1:42972`)"，示例本身带端口，但补端口的代码路径本轮未逐条读出）。
/// * `global` 是 `GLOBAL_SERVER_URL` 拉回来的裸 IP；此时一定用 [`CRASH_REPORT_PORT`]
///   （这一条 `观测`：帮助文本 + 函数里的两次 `42972` 立即数）。
pub fn resolve_default_address(
    cli: Option<&str>,
    env: Option<&str>,
    global: Option<&str>,
) -> CrashReportTarget {
    if let Some(a) = cli.or(env) {
        let a = a.trim();
        if !a.is_empty() {
            return CrashReportTarget::Address(with_default_port(a));
        }
    }
    if let Some(g) = global {
        let g = g.trim();
        if !g.is_empty() {
            return CrashReportTarget::Address(format!("{g}:{CRASH_REPORT_PORT}"));
        }
    }
    CrashReportTarget::PendingWithoutAddress
}

/// 补默认端口的规则：已经是 `host:port` 就原样返回。
fn with_default_port(addr: &str) -> String {
    // IPv6 字面量 `[::1]` 里也有 `:`，先排掉这种形态。
    let has_port = if let Some(rest) = addr.strip_prefix('[') {
        rest.split_once(']')
            .map(|(_, tail)| tail.starts_with(':'))
            .unwrap_or(false)
    } else {
        addr.rsplit_once(':').is_some()
    };
    if has_port {
        addr.to_string()
    } else {
        format!("{addr}:{CRASH_REPORT_PORT}")
    }
}

/// 上报流程的**五种结局**，每种对应 `.rodata` 里一条可指认的字面量（`观测`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// `"Crash report pending but no crash report server address is available."`
    NoAddress,
    /// `"Failed to connect to crash report server at "`
    ConnectFailed(String),
    /// `"Failed to send crash report to "`
    SendFailed(String),
    /// `"Crash report sent to "`
    Sent(String),
    /// `"Failed to clear crash log at "`（发成功了但清不掉，日志会重复上报）
    ClearFailed(String),
}

/// `.rodata` 里这几条消息的**原文**，逐条可 grep 求证。
pub const MESSAGES: [&str; 5] = [
    "Crash report pending but no crash report server address is available.",
    "Failed to connect to crash report server at ",
    "Failed to send crash report to ",
    "Crash report sent to ",
    "Failed to clear crash log at ",
];

/// 另外两条属于 `resolve_default_address` 前置解析。
pub const PARSE_MESSAGES: [&str; 2] = [
    "Failed to parse --crash_report_address: ",
    "Failed to parse global server address for crash reporting: ",
];


// ===========================================================================
// T106 落码：`clear_report`（arm64 `0x4ca340` / 260 B，527 分母外）
// ===========================================================================

/// `clear_report` 的动作：`std::fs::write(path, &[])` —— 把日志 **truncate 成空**。
///
/// 逐指令（Android 2.0.60 arm64 `0x4ca340..0x4ca444`）：
///
/// ```text
/// 0x4ca350  mov  w2,#0x1 / 0x4ca354 mov x3,xzr    ; contents = &[]（**指针 1 = 悬垂、长度 0**）
/// 0x4ca358  stp  x0,x1,[sp]                       ; (x0,x1) = path 的 (ptr,len) —— **没有 &self**
/// 0x4ca35c  bl   std::fs::write::inner            ; fs::write::<&Path, &[u8]>(path, &[])
/// 0x4ca360  cbz  x0,0x4ca400                      ; Ok ⇒ **静默返回**（连日志都不打）
///   ── 失败才走下面 ──
/// 0x4ca36c  bl   chrono::offset::utc::Utc::now    ; 时间戳（**默认 Display**，不是 TIMESTAMP_FORMAT）
/// 0x4ca3a0  bl   alloc::fmt::format_inner         ; format! 串在 0x1805a0（见 CLEAR_FAILED_PIECES）
/// 0x4ca3e0  bl   std::io::stdio::_print           ; 串在 0x18769c（见 CLEAR_LOG_PIECES）⇒ **stdout**
/// 0x4ca3f8  bl   drop_in_place<std::io::Error>
/// 0x4ca400  ldp/ret
/// ```
///
/// 两个不显然的点：
/// 1. **`contents` 是空切片**：`x2 = 1`（`NonNull::dangling()` 形式的合法空切片指针）、`x3 = 0`（长度）
///    ⇒ 不是"写一行提示"，是**清空文件**；
/// 2. 时间戳被 **格式化两次**：先作为"错误消息"的一部分、再作为 `_print` 行的前缀 —— 但
///    `Utc::now()` **只调一次**（`0x4ca36c`），两次显示的**是同一个时刻**。
///
/// ⚠️ 档位：**观测**（逐指令 + `.rodata` 逐字解出的两段格式串）。
/// `GraphwarCrashReporter` 是**无 `self` 的关联函数/自由函数集合**（`x0/x1` 直接就是 path 的
/// (ptr,len)，没有 self 寄存器）⇒ 原版类型大概率是 ZST（`推断`：没读 `new`，因为它没有）。
pub struct GraphwarCrashReporter;

/// `clear_report` **失败消息**的格式片段（`.rodata` 紧凑格式串 `0x1805a0` 逐字解出）。
///
/// 编码方式（本轮实测）：`[len][piece]` 交错 `0xC0<下一段长度>` 的占位符`，`0x00` 收尾：
/// `\x1d` + `"Failed to clear crash log at "` + `\xC0\x02` + `": "` + `\xC0\x01` + `"."` + `\x00`
/// ⇒ 2 个占位符、3 段文本、**结尾一个句点**。
pub const CLEAR_FAILED_PIECES: [&str; 3] =
    ["Failed to clear crash log at ", ": ", "."];

/// `clear_report` **打印行**的格式片段（`.rodata` `0x18769c` 逐字解出）：
/// `\x01` + `"["` + `\xC0\x27` + `"] src/graphwar_crash_reporter.rs:100 - "` + `\xC0\x01` + `"\n"`
/// ⇒ 2 个占位符，第二段里那条硬编码的 **`src/graphwar_crash_reporter.rs:100`**
/// 是原版这行 `log!`/`println!` 的**源码行号**（⇒ 该打印语句在原版文件第 100 行）。
pub const CLEAR_LOG_PIECES: [&str; 3] =
    ["[", "] src/graphwar_crash_reporter.rs:100 - ", "\n"];

impl GraphwarCrashReporter {
    /// `clear_report` —— **原版 arm64 `0x4ca340` / 260 字节**（x86 侧无 Location 锚，527 分母外）。
    ///
    /// 语义：`std::fs::write(path, &[])`；**成功什么都不做**，失败打印一行后返回。
    ///
    /// * 参数 `timestamp`：原版在**体内**取 `chrono::Utc::now()` 的默认 Display（`0x4ca36c`）。
    ///   本 crate 没有 `chrono` 依赖（`Cargo.toml` 只有 gdext/serde 那几个）⇒ 由调用方给，
    ///   **这是与真二进制的已知差异**：时间戳的取法不在本函数里。要补：给 crate 加 chrono 后
    ///   在函数内取 `Utc::now()`（缺的是依赖，不是逻辑）。
    /// * 返回值：失败时返回那行**已经打印出去的文本**（便于单测；原版无返回值 —— `0x4ca40c ret`）。
    pub fn clear_report(path: &std::path::Path, timestamp: &str) -> Result<(), String> {
        // 0x4ca35c：fs::write(path, &[])（空内容 = 指针 1/长度 0）
        match std::fs::write(path, []) {
            // 0x4ca360 cbz x0 → 静默返回
            Ok(()) => Ok(()),
            Err(e) => {
                // 0x4ca3a0：format!("Failed to clear crash log at {ts}: {err}.")
                let message = clear_report_error_message(timestamp, &e.to_string());
                // 0x4ca3e0：_print（**stdout**）"[{ts}] src/graphwar_crash_reporter.rs:100 - {msg}\n"
                let line = clear_report_log_line(timestamp, &message);
                print!("{}", line);
                Err(line)
            }
        }
    }
}

/// `"Failed to clear crash log at {timestamp}: {error}."`（片段见 [`CLEAR_FAILED_PIECES`]）。
pub fn clear_report_error_message(timestamp: &str, error: &str) -> String {
    let p = CLEAR_FAILED_PIECES;
    format!("{}{}{}{}{}", p[0], timestamp, p[1], error, p[2])
}

/// `"[{timestamp}] src/graphwar_crash_reporter.rs:100 - {message}\n"`（片段见 [`CLEAR_LOG_PIECES`]）。
pub fn clear_report_log_line(timestamp: &str, message: &str) -> String {
    let p = CLEAR_LOG_PIECES;
    format!("{}{}{}{}{}", p[0], timestamp, p[1], message, p[2])
}

#[cfg(test)]
mod clear_report_tests {
    //! 期望值来源：arm64 `0x4ca340..0x4ca444` 的指令 + `.rodata` 两段格式串**逐字**解出
    //! （`0x1805a0` / `0x18769c`），不来自我方实现的输出（AGENTS.md §0.5）。
    use super::*;

    /// 两段格式片段是**逐字**从 `.rodata` 读出来的：段数、占位符个数、结尾标点都要对上。
    #[test]
    fn pieces_match_the_rodata_templates() {
        // 0x1805a0：\x1d"Failed to clear crash log at "\xC0\x02": "\xC0\x01"."\x00
        assert_eq!(CLEAR_FAILED_PIECES[0], "Failed to clear crash log at ");
        assert_eq!(CLEAR_FAILED_PIECES[0].len(), 0x1d, "紧凑串第一字节就是 0x1d = 29");
        assert_eq!(CLEAR_FAILED_PIECES[1], ": ");
        assert_eq!(CLEAR_FAILED_PIECES[1].len(), 0x02, "\\xC0\\x02 里的 0x02");
        assert_eq!(CLEAR_FAILED_PIECES[2], ".", "结尾句点（\\xC0\\x01 后的 1 字节段）");
        assert_eq!(CLEAR_FAILED_PIECES[2].len(), 0x01);

        // 0x18769c：\x01"["\xC0\x27"] src/graphwar_crash_reporter.rs:100 - "\xC0\x01"\n"
        assert_eq!(CLEAR_LOG_PIECES[0], "[");
        assert_eq!(CLEAR_LOG_PIECES[0].len(), 0x01);
        assert_eq!(CLEAR_LOG_PIECES[1], "] src/graphwar_crash_reporter.rs:100 - ");
        assert_eq!(CLEAR_LOG_PIECES[1].len(), 0x27, "紧凑串里的 0x27 = 39");
        assert_eq!(CLEAR_LOG_PIECES[2], "\n");
        assert_eq!(CLEAR_LOG_PIECES[2].len(), 0x01);
    }

    /// 失败行的形状：`[ts] src/…:100 - Failed to clear crash log at ts: err.\n`
    /// （时间戳出现两次 —— `0x4ca36c` 只取一次 `Utc::now()`，两处显示同一时刻）。
    #[test]
    fn failure_line_has_timestamp_twice_and_the_line_100_prefix() {
        let ts = "2026-11-28 22:15:48 UTC";
        let msg = clear_report_error_message(ts, "No such file or directory (os error 2)");
        assert_eq!(msg, format!("Failed to clear crash log at {ts}: No such file or directory (os error 2)."));
        let line = clear_report_log_line(ts, &msg);
        assert!(line.starts_with(&format!("[{ts}] src/graphwar_crash_reporter.rs:100 - Failed")), "{line}");
        assert!(line.ends_with(".\n"), "{line}");
        assert_eq!(line.matches(ts).count(), 2, "时间戳在打印行里出现两次（前缀 + 消息内）");
        assert_eq!(line.matches("src/graphwar_crash_reporter.rs:100").count(), 1);
    }

    /// 真行为：**已存在的文件被清空**（0x4ca354 `x3 = 0` ⇒ 长度 0 的空切片），不是"写一行提示"。
    #[test]
    fn clear_report_truncates_an_existing_file() {
        let mut path = std::env::temp_dir();
        path.push(format!("gw2_t106_clear_report_{}.txt", std::process::id()));
        std::fs::write(&path, b"2026-11-28 00:00:00 boom\nsecond line\n").unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() > 0);

        let r = GraphwarCrashReporter::clear_report(&path, "2026-11-28 22:15:48 UTC");
        assert!(r.is_ok(), "写成功 ⇒ 0x4ca360 cbz 走 0x4ca400 静默返回：{r:?}");
        assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new(), "文件必须变空");
        let _ = std::fs::remove_file(&path);
    }

    /// 反面对照：路径不存在 ⇒ 返回那行（并且已经 print 出去），行里带 `io::Error` 的 Display 文本。
    #[test]
    fn clear_report_reports_the_io_error_on_failure() {
        let mut path = std::env::temp_dir();
        path.push("gw2_t106_no_such_dir/definitely_missing/gw2_crash_log.txt");
        let line = GraphwarCrashReporter::clear_report(&path, "TS").unwrap_err();
        assert!(line.starts_with("[TS] src/graphwar_crash_reporter.rs:100 - Failed to clear crash log at TS: "));
        assert!(line.ends_with(".\n"), "{line}");
    }
}
