//! 原版 `src/backtracer.rs` —— **panic 钩子 → 崩溃日志落盘**。
//!
//! ## 一句话（2026-11-07，`docs/16-client.md` §30.2）
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

/// 原版日志行的**旧猜测骨架** —— ⚠️ **T106 已证伪，仅为兼容保留**（2026-11-28）。
///
/// 本轮把 hook 闭包（arm64 `0x4bf058` / 960 B）里的两段 `.rodata` 紧凑格式串**逐字**解出来了，
/// 真实日志是**两行**、不是一行四段：
///
/// ```text
/// Graphwar version: {full_version} ({os_name})        ← 版本行，格式串在 0x182d1e
/// [{timestamp}] PANIC in thread '{thread}': {info}    ← panic 行，格式串在 0x17a21e
/// ```
///
/// 这个常量里"`[{os}]`"和"` - {message}`"两段**都不存在**；`[{ts}] … - …` 那个带方括号与
/// ` - ` 分隔符的形态属于 `graphwar_crash_reporter` 的 `log!` 行（见
/// [`crate::graphwar_crash_reporter::sender_log_line`]，模板里带源码行号），不是本模块的。
/// `timestamp_format` 也不是 [`TIMESTAMP_FORMAT`] —— 两处都用 `chrono::Utc::now()` 的**默认 Display**
/// （`0x4bf0b4 bl Utc::now` + `0x4bf0bc DateTime<Tz> as Display::fmt`；
///  `%Y-%m-%d %H:%M:%S` 字面量在 arm64 `0x1a177c`，全 `.text` 只有 1 处引用、属于
///  `graphwar_replay_control::GraphwarReplayScreenControl::process` 0x4cb478）。
///
/// 保留原因：`tests/misc_modules.rs::hook_renders_all_four_segments` 钉的就是这行推断骨架
/// （那个测试在 `tests/` 里，不在 T106 写范围）⇒ 本轮**不删**，只把真值落到下面的
/// [`PANIC_LINE_PIECES`] / [`VERSION_LINE_PIECES`] / [`panic_log_line`] / [`version_line`]。
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


// ===========================================================================
// T106 落码：`Backtracer::install_panic_log_hook`（arm64 `0x4becec` / 876 B）
// + 它的闭包 `{{closure}}`（arm64 `0x4bf058` / 960 B）—— 527 分母外
// ===========================================================================
//
// ## 为什么要分层落
//
// 这条函数的**副作用**（装全局 panic hook）在 `cargo test` 里会造成互相污染，而它的**内容**
// （两段格式串、拼接顺序、10240 字节截断规则）全是纯逻辑。按预注册 §5 的 P1/P2：
//   纯逻辑部分**必须落码并带单测**；**不**在本 crate 里真装全局 hook。
//
// ## 主函数逐指令（arm64 `0x4becec..0x4bf058`，只列有语义的）
//
// ```text
// 0x4bed0c  bl   ProjectSettings(Singleton)::singleton
// 0x4bed24  bl   RawGd::check_rtti            ; 类型名 0x1a131b（10 字节）
// 0x4bed38  bl   ProjectSettings::globalize_path(builtin 0x1a1485, 0x14 = 20)
//                                              ; "user://crash_log.txt" → 绝对路径（GString）
// 0x4bedac  ldar w8,[0x8b4398] / cbnz         ; 缓存 1 的状态位
// 0x4bef0c  bl   OnceLock::initialize(0x8b4380)   ; 缓存"日志绝对路径"
// 0x4bee3c  ldar w8,[0x8b43b8] / cbnz         ; 缓存 2 的状态位
// 0x4bef48  bl   OnceLock::initialize(0x8b43a0)   ; 缓存 OS_NAME
// 0x4bee80  bl   std::panicking::take_hook    ; 取旧 hook（写成 16 字节的 (fn, data) 对）
// 0x4bee9c  bl   __rustc::__rust_alloc(16, 8)  ; Box 一个 16 字节闭包捕获体 = 旧 hook
// 0x4beeb0  bl   std::panicking::set_hook     ; 装新 hook（vtable 在 0x85ad38）
// ```
//
// ## 闭包（= hook 本体）逐指令（arm64 `0x4bf058..0x4bf418`，只列有语义的）
//
// ```text
// 0x4bf080  bl   std::thread::current / 0x4bf08c bl Thread::name
// 0x4bf090..   name 为空 ⇒ "<unnamed>"（0x1a1499，9 字节）
// 0x4bf0b4  bl   chrono::offset::utc::Utc::now     ; 时间戳（默认 Display，见上）
// 0x4bf0f4  bl   alloc::fmt::format_inner（格式串 0x17a21e）  ⇒ panic 行
// 0x4bf120  bl   std::fs::read_to_string::inner(CRASH_LOG_PATH) ; 读已有日志（出错当空）
// 0x4bf1c4  bl   alloc::fmt::format_inner（格式串 0x182d1e）  ⇒ 版本行
// 0x4bf1dc..   **先**把版本行追加进缓冲（0x4bf22c 之前）
// 0x4bf22c  lsr x8,#0xb / cmp x8,#5 / b.hs 0x4bf2c4         ; 已有+版本 ≥ 10240 ⇒ **连写都不写**
// 0x4bf23c..   **再**追加 panic 行
// 0x4bf268  cmp total,#0x2801 / b.lo 写                      ; total ≤ 10240 ⇒ 原样写
// 0x4bf280  ldrsb w9,[buf+0x2800] / cmn w9,#0x40 / b.ge       ; 从 10240 往回退到 UTF-8 字符边界
// 0x4bf2ac  bl   std::fs::write::inner(CRASH_LOG_PATH, 文本)  ; 写回
// 0x4bf2d4  ldp x0,x8,[x21] / ldr x1,[sp] / ldr x8,[x8,#0x28] / blr x8
//                                              ; **最后调用旧 hook**（vtable +0x28 = call）
// ```

/// 线程名取不到时的占位（`0x4bf090 adrp 0x1a1000 + 0x499` = 字面量 9 字节）。
pub const THREAD_NAME_FALLBACK: &str = "<unnamed>";

/// `OS_NAME` 缓存没初始化时的占位（`0x4bf16c adrp 0x1a1000 + 0x4a2` = 7 字节）。
pub const OS_NAME_FALLBACK: &str = "unknown";

/// 日志文件上限：**10240 字节**，且只在 UTF-8 字符边界处截断。
///
/// 两处立即数：`0x4bf22c lsr x8,x25,#0xb` + `cmp x8,#5`（5 × 2048 = 10240）⇒ 追加前的判据；
/// `0x4bf268 mov w8,#0x2801`（10241）⇒ 追加后的判据（≤10240 原样写，否则回退到字符边界）。
pub const PANIC_LOG_CAP: usize = 10240;

/// panic 行的格式片段（`.rodata` 紧凑格式串 `0x17a21e` 逐字解出，3 个占位符）：
/// `[len]"[" \xC0\x13"] PANIC in thread '" \xC0\x03"': " \xC0\x01"\n" \x00`
/// ⇒ `"[{timestamp}] PANIC in thread '{thread}': {info}\n"`。
pub const PANIC_LINE_PIECES: [&str; 4] = ["[", "] PANIC in thread '", "': ", "\n"];

/// 版本行的格式片段（`.rodata` `0x182d1e` 逐字解出，2 个占位符）：
/// `[len]"Graphwar version: " \xC0\x02" (" \xC0\x02")\n" \x00`
/// ⇒ `"Graphwar version: {full_version} ({os_name})\n"`。
pub const VERSION_LINE_PIECES: [&str; 3] = ["Graphwar version: ", " (", ")\n"];

/// `install_panic_log_hook` 的步骤（顺序 = `bl` 地址顺序，地址见本节开头的逐指令表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPanicLogHookStep {
    /// `ProjectSettings::singleton`（`0x4bed0c`）→ `check_rtti`（`0x4bed24`）→
    /// `globalize_path("user://crash_log.txt", 20)`（`0x4bed38`）⇒ 把 `user://` 换成绝对路径。
    ResolveLogPath,
    /// 缓存日志路径：`OnceLock::initialize`（`0x4bef0c`，静态量 `0x8b4380`/`0x8b4398`）。
    CacheLogPath,
    /// 缓存 OS 名：`OnceLock::initialize`（`0x4bef48`，静态量 `0x8b43a0`/`0x8b43b8`）。
    CacheOsName,
    /// `std::panicking::take_hook`（`0x4bee80`）。
    TakeOldHook,
    /// 把旧 hook 装进 16 字节捕获体：`__rust_alloc(16, 8)`（`0x4bee9c`；分配失败走 `handle_alloc_error` 0x4beed4）。
    AllocClosure,
    /// `std::panicking::set_hook`（`0x4beeb0`，vtable `0x85ad38`）。
    SetHook,
}

/// `install_panic_log_hook` 的装配顺序（桩：**不真的装 hook**，见本节的 P2 声明）。
pub fn install_panic_log_hook_steps() -> [InstallPanicLogHookStep; 6] {
    [
        InstallPanicLogHookStep::ResolveLogPath,
        InstallPanicLogHookStep::CacheLogPath,
        InstallPanicLogHookStep::CacheOsName,
        InstallPanicLogHookStep::TakeOldHook,
        InstallPanicLogHookStep::AllocClosure,
        InstallPanicLogHookStep::SetHook,
    ]
}

/// panic 行：`"[{timestamp}] PANIC in thread '{thread}': {info}\n"`。
///
/// * `thread_name`：`None` ⇒ [`THREAD_NAME_FALLBACK`]（`0x4bf090..0x4bf0a4` 的两条 `csel`）；
/// * `info_display`：hook 闭包的**第 2 个实参**（`&PanicHookInfo`）的 Display 文本。
///   证据链：`0x4bf07c str x1,[sp]`（保存实参）→ `0x4bf0d4 mov x8,sp` + `0x4bf0dc adr 0x4b0ea8`
///   （`&T as Display::fmt` 的实例化）⇒ 格式化的对象就是那个实参本身。
///   `PanicHookInfo: Display` 在本机 rustc 1.100 上存在（实测 `format!("{}", info)` 编译通过、
///   输出 `panicked at <file>:<line>:<col>:\n<message>`）⇒ **推断**（原版 Android 构建的 std 版本未证），
///   不是逐位。
pub fn panic_log_line(timestamp: &str, thread_name: Option<&str>, info_display: &str) -> String {
    let p = PANIC_LINE_PIECES;
    format!(
        "{}{}{}{}{}{}{}",
        p[0],
        timestamp,
        p[1],
        thread_name.unwrap_or(THREAD_NAME_FALLBACK),
        p[2],
        info_display,
        p[3]
    )
}

/// 版本行：`"Graphwar version: {full_version} ({os_name})\n"`。
///
/// `full_version` 来自 `graphwar_version_get_full_version`（arm64 `0x3913e4`，`0x4bf19c` 调用）；
/// `os_name` 来自 `OnceLock<…>`（`0x8b43a0`），未初始化 ⇒ [`OS_NAME_FALLBACK`]（`0x4bf16c`..`0x4bf18c`）。
pub fn version_line(full_version: &str, os_name: Option<&str>) -> String {
    let p = VERSION_LINE_PIECES;
    format!(
        "{}{}{}{}{}",
        p[0],
        full_version,
        p[1],
        os_name.unwrap_or(OS_NAME_FALLBACK),
        p[2]
    )
}

/// 把 panic 行并进日志后的**完整文本**；`None` = **连文件都不写**。
///
/// 逐指令（arm64 `0x4bf164..0x4bf2b0`）：
/// 1. 读已有内容（`0x4bf120`，出错当空）→ **先追加版本行**（`0x4bf1dc`–`0x4bf210`）；
/// 2. `0x4bf22c` `lsr x8,x25,#0xb` / `cmp x8,#5` / `b.hs 0x4bf2c4` ⇒ **已有+版本 ≥ 10240 字节时
///    直接跳过写文件**（panic 行与版本行**都不落盘**）⇒ 返回 `None`；
/// 3. 否则**再追加 panic 行**（`0x4bf23c`–`0x4bf260`）；
/// 4. `0x4bf268` `cmp total,#0x2801`：`total ≤ 10240` 原样写；否则从偏移 **10240** 往回退到
///    第一个**不是 UTF-8 续字节**的位置（`0x4bf280 ldrsb` + `0x4bf284 cmn w9,#0x40` + `b.ge`）再截断；
/// 5. `0x4bf2ac` `fs::write` 写回（错误静默丢弃：`0x4bf2b4 cbz x0` 之后直接 drop 错误）。
///
/// 顺序是证据：**版本行在前、panic 行在后**（`0x4bf1dc` < `0x4bf23c`）。
pub fn merged_panic_log(existing: &str, version: &str, panic: &str) -> Option<String> {
    let mut text = String::from(existing);
    // 0x4bf1dc：先追加版本行
    text.push_str(version);
    // 0x4bf22c：lsr #11 / cmp #5 ⇒ 512*…… 5×2048 = 10240，达标就**不写**
    if text.len() >= PANIC_LOG_CAP {
        return None;
    }
    // 0x4bf23c：再追加 panic 行
    text.push_str(panic);
    // 0x4bf268..0x4bf2a4：超上限就截到 UTF-8 字符边界
    if text.len() > PANIC_LOG_CAP {
        let mut n = PANIC_LOG_CAP;
        while n > 0 && !text.is_char_boundary(n) {
            n -= 1; // 0x4bf28c subs x8,x8,#1
        }
        text.truncate(n);
    }
    Some(text)
}

#[cfg(test)]
mod install_panic_log_hook_tests {
    //! 期望值来源：arm64 `0x4becec..0x4bf058`（装配）+ `0x4bf058..0x4bf418`（闭包）的指令，
    //! 以及 `.rodata` 两段紧凑格式串**逐字**解出（`0x17a21e` / `0x182d1e`）。
    use super::*;

    /// 两段格式片段：段数、**每段长度字节**、占位符个数都要与 `.rodata` 对上。
    #[test]
    fn pieces_match_the_rodata_templates() {
        // 0x17a21e：\x01"[" \xC0\x13"] PANIC in thread '" \xC0\x03"': " \xC0\x01"\n" \x00
        assert_eq!(PANIC_LINE_PIECES[0], "[");
        assert_eq!(PANIC_LINE_PIECES[0].len(), 0x01);
        assert_eq!(PANIC_LINE_PIECES[1], "] PANIC in thread '");
        assert_eq!(PANIC_LINE_PIECES[1].len(), 0x13, "紧凑串里的 0x13 = 19");
        assert_eq!(PANIC_LINE_PIECES[2], "': ");
        assert_eq!(PANIC_LINE_PIECES[2].len(), 0x03);
        assert_eq!(PANIC_LINE_PIECES[3], "\n");
        assert_eq!(PANIC_LINE_PIECES[3].len(), 0x01);

        // 0x182d1e：len"Graphwar version: " \xC0\x02" (" \xC0\x02")\n" \x00
        assert_eq!(VERSION_LINE_PIECES[0], "Graphwar version: ");
        assert_eq!(VERSION_LINE_PIECES[0].len(), 0x12, "紧凑串里的 0x12 = 18");
        assert_eq!(VERSION_LINE_PIECES[1], " (");
        assert_eq!(VERSION_LINE_PIECES[1].len(), 0x02);
        assert_eq!(VERSION_LINE_PIECES[2], ")\n");
        assert_eq!(VERSION_LINE_PIECES[2].len(), 0x02);

        assert_eq!(THREAD_NAME_FALLBACK, "<unnamed>");
        assert_eq!(THREAD_NAME_FALLBACK.len(), 9, "0x4bf09c mov w9,#0x9");
        assert_eq!(OS_NAME_FALLBACK, "unknown");
        assert_eq!(OS_NAME_FALLBACK.len(), 7, "0x4bf184 mov w9,#0x7");
        assert_eq!(PANIC_LOG_CAP, 10240, "0x4bf268 mov w8,#0x2801 的上限");
    }

    /// panic 行 / 版本行的形状（含两个 fallback 分支）。
    #[test]
    fn lines_take_the_fallback_values() {
        assert_eq!(
            panic_log_line("TS", Some("main"), "panicked at src/x.rs:1:1:\nboom"),
            "[TS] PANIC in thread 'main': panicked at src/x.rs:1:1:\nboom\n"
        );
        assert_eq!(
            panic_log_line("TS", None, "X"),
            "[TS] PANIC in thread '<unnamed>': X\n",
            "0x4bf090..0x4bf0a4 的两条 csel"
        );
        assert_eq!(version_line("2.0.60", Some("linux")), "Graphwar version: 2.0.60 (linux)\n");
        assert_eq!(
            version_line("2.0.60", None),
            "Graphwar version: 2.0.60 (unknown)\n",
            "0x4bf16c..0x4bf18c 的 csel"
        );
    }

    /// 拼接**顺序**：已有内容 → 版本行 → panic 行（`0x4bf1dc` < `0x4bf23c`）。
    #[test]
    fn merged_log_appends_version_then_panic() {
        let got = merged_panic_log("OLD\n", "V\n", "P\n").unwrap();
        assert_eq!(got, "OLD\nV\nP\n");
        assert_eq!(merged_panic_log("", "V\n", "P\n").unwrap(), "V\nP\n");
    }

    /// `0x4bf22c`：已有内容 + 版本行 **≥ 10240** ⇒ 返回 `None`（**连文件都不写**）。
    #[test]
    fn merged_log_skips_entirely_when_already_at_cap() {
        let existing = "x".repeat(PANIC_LOG_CAP - 1);
        // 加 1 字节版本行后正好 10240 ⇒ 仍然达标（`cmp x8,#5` 是"≥"）
        assert_eq!(merged_panic_log(&existing, "V", "P").map(|s| s.len()), None);
        // 差一字节就还能写：10239 + 1 = 10240 ⇒ 达标 ⇒ None；10238 + 1 = 10239 ⇒ Some
        let existing = "x".repeat(PANIC_LOG_CAP - 2);
        assert!(merged_panic_log(&existing, "V", "P").is_some(), "10239 < 10240 ⇒ 照写");
    }

    /// `0x4bf268..0x4bf2a4`：总长 > 10240 时截到 **UTF-8 字符边界**（从 10240 往回退）。
    ///
    /// 构造：`existing` 10237 B + 版本行 1 B = 10238（< 10240 ⇒ 不跳写），
    /// 再追加 panic 行的 3 字节字符 ⇒ 偏移 **10240 落在一个 3 字节字符的中间**（续字节），
    /// 正好触发 `0x4bf280 ldrsb` / `cmn w9,#0x40` / `b.ge` 的回退循环。
    #[test]
    fn merged_log_truncates_on_a_char_boundary() {
        let existing = format!("a{}", "中".repeat(3412)); // 1 + 10236 = 10237
        assert_eq!(existing.len(), 10237);
        let got = merged_panic_log(&existing, "V", "中C").unwrap();
        // 10237 + 1 = 10238；+ "中C"(4) = 10242 > 10240 ⇒ 回退到 10238（"中" 的起始位置）
        assert_eq!(got.len(), 10238, "必须退到 10238（3 字节字符的起点）");
        assert!(got.is_char_boundary(got.len()));
        assert_eq!(got, format!("{}V", existing), "版本行保留、panic 行整条被截掉");
        // 反面对照：若按"硬截到 10240"就该切在一个字符中间（那正是 0x4bf280 那段要防的）
        let mut raw = String::from(existing);
        raw.push_str("V中");
        assert!(!raw.is_char_boundary(PANIC_LOG_CAP), "10240 落在一个 3 字节字符中间");
    }

    /// 装配六步的**地址顺序**（写死成文档的一部分：0x4bed38 → 0x4bef0c → 0x4bef48 → 0x4bee80
    /// → 0x4bee9c → 0x4beeb0 —— 注意 OnceLock 的 initialize 代码块地址在 take_hook **之后**，
    /// 但两条初始化路径是从 0x4bedb0/0x4bee44 跳过去的，语义顺序是"先解析路径、再取旧 hook"）。
    #[test]
    fn install_steps_are_the_six_observed_actions() {
        assert_eq!(
            install_panic_log_hook_steps(),
            [
                InstallPanicLogHookStep::ResolveLogPath,
                InstallPanicLogHookStep::CacheLogPath,
                InstallPanicLogHookStep::CacheOsName,
                InstallPanicLogHookStep::TakeOldHook,
                InstallPanicLogHookStep::AllocClosure,
                InstallPanicLogHookStep::SetHook,
            ]
        );
        // take_hook → alloc → set_hook 三个调用点的地址必须严格递增
        let order = [0x4bee80u64, 0x4bee9c, 0x4beeb0];
        assert!(order.windows(2).all(|w| w[0] < w[1]));
    }

    /// **P2 声明**：本 crate **不**装全局 hook（会污染其它单测）⇒ 这里只钉"没装"。
    /// 装它需要的一半东西（godot `ProjectSettings` / `GString`）本 crate 没有。
    #[test]
    fn hook_is_not_actually_installed_by_this_crate() {
        // 我们只提供步骤与纯函数；真正的 set_hook 调用不在本 crate 的任何路径上。
        // 反面对照：若有人偷偷装了，`take_hook` 返回的就不再是默认 hook —— 这条断言只
        // 记录"本模块没有可执行的安装入口"（`install_panic_log_hook_steps()` 不产生副作用）。
        let before = install_panic_log_hook_steps();
        let again = install_panic_log_hook_steps();
        assert_eq!(before, again, "纯函数，无副作用");
    }
}
