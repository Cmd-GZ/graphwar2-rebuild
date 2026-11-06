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

/// 崩溃日志里时间戳的格式（`逐位`：`.rodata` 字面量，被本模块与 `backtracer` 共用）。
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
