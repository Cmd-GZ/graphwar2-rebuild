//! 原版 `src/backtracer.rs` —— **panic 钩子 → 崩溃日志落盘**。
//!
//! ## 一句话（2026-11-07，`docs/16-client.md` §33.2）
//!
//! 全模块只有 **1 个可归属函数**（+ 它的闭包）：
//! `Backtracer::install_panic_log_hook`（arm64 `0x4becec`，876 字节）与
//! `install_panic_log_hook::{{closure}}`（arm64 `0x4bf058`，960 字节，就是**钩子本体**）。
//! 另有两个静态量 `CRASH_LOG_PATH`（arm64 `0x8b4380`）与 `OS_NAME`（`0x8b43a0`）。
//!
//! 它**不上报**任何东西 —— 上报是 [`crate::graphwar_crash_reporter`] 的事。
//! 分工：
//!
//! ```text
//! backtracer            : panic 时 → 往 user://crash_log.txt 追加
//! graphwar_crash_reporter: 启动时 → 把 user://crash_log.txt 发走并清空
//! ```
//!
//! ## 装在哪（`观测`，arm64 `bl` 图）
//!
//! 唯一调用者是 `<GraphwarRootNode as INode>::ready +0x30`
//! （紧跟其后 `+0x44` 就是 `GraphwarCrashReporter::send_pending_report`）。
//! ⇒ **每次进主场景装一次**，不是 `main()` 一次性。
//!
//! godot 那边之所以能这样写，是因为 `panic::set_hook` 在本 crate 里是**替换**
//! 而不是叠加：`GraphwarRootNode::ready` 被反复回调时，钩子也跟着被反复替换，
//! 不会堆出 N 层（本条 `推断`：没有真机观测"进两次主场景后钩子层数"）。
//!
//! ## 为什么模块名叫 backtracer 却看不到 backtrace 调用
//!
//! arm64 版 `install_panic_log_hook` 的 `bl` 目标里没有
//! `std::backtrace::Backtrace::capture`（`rustc` 的 `__rust_begin_short_backtrace`
//! 那个符号是**线程**用的，不是本模块）。所以本模块的实际名字有点误导：
//! 它只做**日志**，不做栈回溯。本轮**未**在两条架构的 `.text` 里找到任何
//! 由本模块发出的 `Backtrace::capture` 调用（`观测`，bounded negative）。
//!
//! ## `OS_NAME`
//!
//! 一个 `&'static str` 静态量，32 字节的 `d` 符号（`LazyLock`/`OnceLock` 壳的尺寸）。
//! 它的**取值**本轮未读出（字符串可能由 `cfg!` 编译期选定、也可能在 `.data` 里），
//! 标 `未知`。日志格式里它出现在时间戳附近，用途是区分 Android / Linux / Windows
//! 三份构建的崩溃（`推断`）。

pub use crate::graphwar_crash_reporter::{CRASH_LOG_PATH, TIMESTAMP_FORMAT};

/// 原版日志行的**骨架**（`推断`：由 `.rodata` 里相邻字面量 + hook 闭包里的
/// `format_inner` 参数个数推出 4 段，逐字符未验证）。
///
/// 可确证的只有两件事：**时间戳格式**是 [`TIMESTAMP_FORMAT`]（`逐位`，字面量在
/// `.rodata` 且被 `clear_report` 的同一段 `format_inner` 使用），
/// 以及**写入目标是** [`CRASH_LOG_PATH`]（`逐位`）。
pub const LOG_LINE_SKELETON: &str = "{timestamp} [{os}] {location} - {message}";

/// 钩子要做的事，本 crate 只用字符串把它描述出来 —— 真正的
/// `std::panic::set_hook` 需要 godot 运行时，本 crate 不引入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicLogHook {
    /// 日志落盘路径（`user://` 是 Godot 的虚拟路径，由引擎映射到用户目录）。
    pub path: &'static str,
    /// 时间戳格式。
    pub timestamp_format: &'static str,
}

impl Default for PanicLogHook {
    fn default() -> Self {
        Self { path: CRASH_LOG_PATH, timestamp_format: TIMESTAMP_FORMAT }
    }
}

impl PanicLogHook {
    /// 渲染一行。`os` 传原版的 `OS_NAME`（本轮未知，见模块文档）。
    ///
    /// 注意这是**形态**而非逐字节复刻（标 `推断`）——能确证的是"里面有时间戳、
    /// 有 OS、有位置、有消息"四段。
    pub fn render(&self, os: &str, location: &str, message: &str, timestamp: &str) -> String {
        LOG_LINE_SKELETON
            .replace("{timestamp}", timestamp)
            .replace("{os}", os)
            .replace("{location}", location)
            .replace("{message}", message)
    }
}
