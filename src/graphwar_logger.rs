//! 原版 `src/graphwar_logger.rs` —— **进程内的调试日志 HTTP 端点**（不是 `log` crate 的宏）。
//!
//! ## 定性（2026-11-07，`docs/16-client.md` §33.5，`观测`）
//!
//! arm64 2.0.60 的符号表给出三个方法 + 一个静态量：
//!
//! | 符号 | arm64 | 字节 | 判据 |
//! |---|---|---|---|
//! | `GraphwarLogger::new` | `0x45bce0` | 1320 | `bl` 到 **`tiny_http::Server::new` ×2**、`ToSocketAddrs::to_socket_addrs` ×2、`String` 迭代收集 ×2 |
//! | `GraphwarLogger::log` | `0x45bc5c` | 132 | `bl` 到 `<RawVec<OsString>>::grow_one` ⇒ 往一个 `Vec<OsString>` 里塞一行 |
//! | `GraphwarLogger::process` | `0x45c208` | 788 | 每帧/每 tick 的驱动（`process` 是 Godot 虚函数名） |
//! | `GLOBAL_LOGGER` | `0x8b4268` | 16 | `__RUST_STD_INTERNAL_VAL` 壳 |
//!
//! **`tiny_http::Server::new` 出现两次**是这个模块最硬的指纹：它说明
//! `GraphwarLogger` 起的不是一个玩具 socket，而是一个真的 HTTP 服务器
//! （`tiny_http` 是纯 Rust 的单线程 HTTP server crate）。
//! 端口来自 clap 选项 **`LOGGER_PORT` / `logger_port` / `logger-port`**
//! （`.rodata` clap 帮助：*"Port used for the debug logger"*）。
//!
//! ## 谁在用它
//!
//! * `GraphwarLogger::new` ← `<GraphwarRootNode as INode>::init`（一次性，`观测`）
//! * `GraphwarLogger::log` ← `GraphwarRootNode::process_button_pressed`（`观测`）
//! * 其余 `log` 调用点大概率被内联进各 manager（`推断`：`bl` 图只看到 1 个）
//!
//! `GLOBAL_LOGGER` 是 `Rc<RefCell<GraphwarLogger>>` 的全局壳
//! （`drop_in_place<alloc::rc::RcInner<RefCell<GraphwarLogger>>>` 在
//! `GraphwarRootNode::init` 的清理段里），所以各模块拿的是这把 `Rc` 的克隆。
//!
//! ## 为什么它不是"通用日志"
//!
//! `log()` 收的是 **`OsString`**（不是 `String`）——那是
//! `std::env::args_os` 的产物类型，说明它记录的原始用途是**把启动参数与
//! 玩家操作序列落进环形缓冲**，然后由 `process` 通过 HTTP 吐给外部调试工具
//! （这个项目的作者显然在开发期用一个浏览器页面读这些日志）。
//!
//! HTTP 的**报文格式**本轮未读出（响应头/正文模板都在 `process` 里，
//! 需要逐条解 `tiny_http::Response` 的构造），标 `未知`。

/// clap 里的端口选项名（`逐位`：`.rodata` 的 clap 定义块）。
pub const CLI_ARG_SNAKE: &str = "logger_port";
/// 环境变量名（同块）。
pub const ENV_VAR: &str = "LOGGER_PORT";
/// 命令行风格写法（同块）。
pub const CLI_ARG_KEBAB: &str = "logger-port";
/// clap 帮助原文（`逐位`）。
pub const CLI_HELP: &str = "Port used for the debug logger";

/// 日志端点。原版持有 `Rc<RefCell<GraphwarLogger>>`（全局 `GLOBAL_LOGGER`），
/// 本 crate 只还原**数据面**（`log` 塞一行 / `process` 取走），
/// 不引 `tiny_http`（本 crate 无第三方依赖，见 `Cargo.toml`）。
///
/// `逐位`：`log` 的目标是 `Vec<OsString>`（`RawVec<OsString>::grow_one`）。
/// 这里用 `Vec<String>`：语义相同（一行一条），类型差一档并已在文档标注。
#[derive(Debug, Default)]
pub struct GraphwarLogger {
    pending: Vec<String>,
    /// 监听端口；原版由 clap 的 `logger_port` 决定（未给则系统挑空闲端口，
    /// 见帮助文本里的 `server-port` 同理表述）。
    pub port: u16,
}

impl GraphwarLogger {
    pub fn new(port: u16) -> Self {
        Self { pending: Vec::new(), port }
    }

    /// 对应 `0x45bc5c`：**只入队，不发送**。原版没有任何过滤/分级 ——
    /// 没有 `log::Level`，没有 `env_logger`，就是一行一行塞进去。
    pub fn log(&mut self, line: impl Into<String>) {
        self.pending.push(line.into());
    }

    /// 对应 `0x45c208`：把积压交给 HTTP 端点。原版在这里做 `tiny_http` 的
    /// `recv`/`respond`；本 crate 只保证"取走并清空"这一半语义
    /// （报文格式未知，见模块文档末段）。
    pub fn process(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}
