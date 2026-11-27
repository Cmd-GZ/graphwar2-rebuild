//! **不是原版的东西**：把**已经是复原实现**的连接状态机接给 GDScript 的 socket 层。
//!
//! # 为什么会有这一份（T67 / `docs/06` §118 / `docs/06` §90 C 档第 5 段）
//!
//! `client/src/net/link.gd`（改前 224 行）是一条 TCP 连接：非阻塞 `StreamPeerTCP`
//! + 每帧 `poll()` + 状态机（`idle → connecting → connected / failed / closed`）。
//! 其中**状态机**在 Rust 侧早就复原了：
//!
//! | 真值 | 地址 | 档位 |
//! |---|---|---|
//! | `ConnectionState { Disconnected=0, Connecting=1, Connected=2, Failed=3 }` | 4 个写入点 `0x48b900` / `0x48de10` / `0x48dd1c` / `disconnect` | 逐位 |
//! | `tick` 的闩锁 + 分派（`SendNewConnectionRequest` / `SetGameState(7)`） | `0x48b488`–`0x48b518` | 观测 |
//! | 连接线程的结束态（成功 ⇒ 2、失败 ⇒ 3） | `state_after_connect_attempt` | 观测 |
//! | 超时 `CONNECT_TIMEOUT_SECS = 30` | arm64 `0x48dccc mov w1,#0x1e` | 逐位 |
//!
//! `.gd` 里那份状态机是**第二份实现**（同一个 `2` 在两处各写一遍）。本类把它接回来：
//! GDScript 只留**引擎侧**那部分（建 `StreamPeerTCP`、`poll`、读字节、发信号），
//! "下一步该是什么状态"一律问 Rust。
//!
//! # 原版里没有这个类
//!
//! 原版 `libgraphwarrust.so` 里没有 `GwClientLink`：状态机是
//! `GraphwarClientConnectionHandler` 的字段（`Mutex<u8>`，读点在 `get_connection_state`），
//! 连接线程是 `std::thread`。我们的客户端用 Godot 的 `StreamPeerTCP`（单线程非阻塞），
//! 所以这里只搬**判定**，不搬线程。
//!
//! # 框架差异（照实写）
//!
//! 原版的 30 s 是**阻塞在 `TcpStream::connect_timeout` 里**的（超时由 libstd 判），
//! 我方单线程必须每帧自己比 `elapsed`。所以 [`GwClientLink::step`] 里那条
//! `elapsed >= CONNECT_TIMEOUT_SECS ⇒ Failed` 是**等价物**，不是"原版有这么一条比较"。
//!
//! GDScript 用法（`client/src/net/link.gd`）：
//!
//! ```gdscript
//! var link := GwClientLink.new()                  # 或全部用静态函数
//! var nxt := GwClientLink.step(state_id, tcp_status, elapsed_s)
//! # nxt = {next_state: int, action: "wait"|"connected"|"failed", timed_out: bool}
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_client_connection_handler as cch;

/// 连接状态机的**判定出口**（scaffold；原版没有这个类，见模块文档）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwClientLink {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwClientLink {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

/// `Godot StreamPeerTCP` 的 4 个 status 值（**引擎侧常量**，不是原版的数）。
///
/// `NET_STATUS_NONE = 0`（对端关了/还没建）、`CONNECTING = 1`、`CONNECTED = 2`、`ERROR = 3`。
const TCP_STATUS_NONE: i64 = 0;
const TCP_STATUS_CONNECTING: i64 = 1;
const TCP_STATUS_CONNECTED: i64 = 2;
// const TCP_STATUS_ERROR: i64 = 3;  // 其它值一律按"错"处理（见 `step`）

/// `ConnectionState` 的判别值 → GDScript（`0..=3`；值本身是**逐位**的写入点证据）。
fn state_id(s: cch::ConnectionState) -> i64 {
    match s {
        cch::ConnectionState::Disconnected => 0,
        cch::ConnectionState::Connecting => 1,
        cch::ConnectionState::Connected => 2,
        cch::ConnectionState::Failed => 3,
    }
}

/// 判别值 → `ConnectionState`（>3 不存在：原版只有 4 个写入点）。
fn state_of(id: i64) -> Option<cch::ConnectionState> {
    u8::try_from(id).ok().and_then(cch::ConnectionState::from_u8)
}

#[godot_api]
impl GwClientLink {
    /// `Disconnected = 0`（初值；`disconnect()` 之后也是它）。
    #[func]
    fn state_disconnected() -> i64 {
        state_id(cch::ConnectionState::Disconnected)
    }

    /// `Connecting = 1`（`connect()` 里 `strb #1,[x21,#0x15]`，arm64 `0x48b900`）。
    #[func]
    fn state_connecting() -> i64 {
        state_id(cch::ConnectionState::Connecting)
    }

    /// `Connected = 2`（连接线程 `set_nonblocking` 成功之后 `strb #2`，`0x48de10`）。
    #[func]
    fn state_connected() -> i64 {
        state_id(cch::ConnectionState::Connected)
    }

    /// `Failed = 3`（`connect_timeout` 返回 `Err` 时 `strb #3`，`0x48dd1c`）。
    #[func]
    fn state_failed() -> i64 {
        state_id(cch::ConnectionState::Failed)
    }

    /// 4 个状态的判别值（自检/DOM 用；顺序 = 声明序）。
    #[func]
    fn state_ids() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("disconnected", state_id(cch::ConnectionState::Disconnected));
        d.set("connecting", state_id(cch::ConnectionState::Connecting));
        d.set("connected", state_id(cch::ConnectionState::Connected));
        d.set("failed", state_id(cch::ConnectionState::Failed));
        d
    }

    /// 单次连接尝试的超时，**秒**（= [`cch::CONNECT_TIMEOUT_SECS`]，逐位 30）。
    #[func]
    fn timeout_secs() -> f64 {
        cch::CONNECT_TIMEOUT_SECS as f64
    }

    /// 连接线程结束后的状态（原版：成功 ⇒ `Connected`，失败 ⇒ `Failed`）。
    #[func]
    fn after_connect_attempt(tcp_established: bool) -> i64 {
        state_id(cch::state_after_connect_attempt(tcp_established))
    }

    /// **一帧的连接状态机**：`(当前状态, Godot TCP status, 已用秒数)` → 下一步。
    ///
    /// 返回 `{next_state: int, action: String, timed_out: bool}`：
    /// * `action = "wait"` —— 什么都不做（还在连 / 已经不在 `Connecting`）；
    /// * `action = "connected"` —— TCP 建起来了，状态转 `Connected`；
    /// * `action = "failed"` —— 失败（`timed_out` 区分"超时"与"对端报错"）。
    ///
    /// 判据来源：
    /// * `Connected`/`Failed` 两个终态 = [`cch::state_after_connect_attempt`]（观测，两个 `strb`）；
    /// * 超时阈值 = [`cch::CONNECT_TIMEOUT_SECS`]（逐位 30）；
    /// * **框架差异**：原版超时由 `TcpStream::connect_timeout` 内部判（阻塞线程），
    ///   这里改成每帧比 `elapsed` —— 等价物，不是原版的比较指令。
    #[func]
    fn step(state: i64, tcp_status: i64, elapsed_s: f64) -> VarDictionary {
        let mut out = VarDictionary::new();
        let cur = state_of(state);
        // 只有 `Connecting` 这一态会被推进（数字 1；其余态原版也不写）。
        if cur != Some(cch::ConnectionState::Connecting) {
            out.set("next_state", state);
            out.set("action", &GString::from("wait"));
            out.set("timed_out", false);
            return out;
        }
        let (next, action, timed_out) = match tcp_status {
            TCP_STATUS_CONNECTED => (
                cch::state_after_connect_attempt(true),
                "connected",
                false,
            ),
            TCP_STATUS_CONNECTING => {
                if elapsed_s >= cch::CONNECT_TIMEOUT_SECS as f64 {
                    (cch::state_after_connect_attempt(false), "failed", true)
                } else {
                    (cch::ConnectionState::Connecting, "wait", false)
                }
            }
            // `TCP_STATUS_NONE` = 对端关了/没建；其余（含 `ERROR = 3`）都算失败。
            _ => (cch::state_after_connect_attempt(false), "failed", false),
        };
        out.set("next_state", state_id(next));
        out.set("action", &GString::from(action));
        out.set("timed_out", timed_out);
        out
    }

    /// `tick` 的判定（原版 `0x48b488`–`0x48b518`）：这一帧该不该发握手 / 置 GameState。
    ///
    /// 返回 `{send_handshake: bool, game_state: int}`（`game_state = -1` = 不置）。
    ///
    /// * `latched` —— 本模块的**闩锁**（`self+0x68`）：`connect()` 清零、`tick` 置 1；
    ///   没有它，状态 2 会在每一帧重发 `NewConnectionRequest`。
    /// * 状态 2 ⇒ 发握手；状态 3 ⇒ `set_state(7)`（[`cch::GAME_STATE_ON_CONNECT_FAILED`]）。
    #[func]
    fn tick(latched: bool, state: i64) -> VarDictionary {
        let mut out = VarDictionary::new();
        let action = match state_of(state) {
            Some(s) => cch::tick_action(latched, s),
            None => cch::TickAction::Idle,
        };
        match action {
            cch::TickAction::Idle => {
                out.set("send_handshake", false);
                out.set("game_state", -1i64);
            }
            cch::TickAction::SendNewConnectionRequest => {
                out.set("send_handshake", true);
                out.set("game_state", -1i64);
            }
            cch::TickAction::SetGameState(g) => {
                out.set("send_handshake", false);
                out.set("game_state", g as i64);
            }
        }
        out
    }

    /// 连接失败时要置的 `GameState`（原版 `0x48b564 mov w1,#7`）。
    #[func]
    fn game_state_on_connect_failed() -> i64 {
        cch::GAME_STATE_ON_CONNECT_FAILED as i64
    }

    /// `is_open()`：状态是不是 `Connected`（= [`cch::ConnectionState::Connected`]，2）。
    #[func]
    fn is_open(state: i64) -> bool {
        state_of(state) == Some(cch::ConnectionState::Connected)
    }

    /// `is_busy()`：状态是不是 `Connecting`（= 1）。
    #[func]
    fn is_busy(state: i64) -> bool {
        state_of(state) == Some(cch::ConnectionState::Connecting)
    }

    /// 主动断开后应处的状态（原版 `disconnect` 写 0）。
    #[func]
    fn disconnect_state() -> i64 {
        state_id(cch::ConnectionState::Disconnected)
    }

    /// 握手请求的事件名（`NewConnectionRequest`，`docs/07` tag 表；原版
    /// `make_new_connection_request_event` `0x4693c0`）。
    #[func]
    fn handshake_event_name() -> GString {
        GString::from(cch::MINIMAL_CLIENT_HANDSHAKE)
    }

    /// 出处自检：每个值的 Rust 常量/函数 + 指令地址 + 档位。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("module", &GString::from("graphwar_client_connection_handler"));
        d.set("timeout_secs", cch::CONNECT_TIMEOUT_SECS as i64);
        d.set(
            "timeout_evidence",
            &GString::from(
                "arm64 0x48dccc mov w1,#0x1e (Duration.secs=30) + 0x48dcd0 mov w2,wzr (nanos=0) \
                 + 0x48dcd4 bl std::net::tcp::TcpStream::connect_timeout",
            ),
        );
        d.set(
            "state_evidence",
            &GString::from(
                "4 个写入点：0x48b900 strb #1 / 0x48de10 strb #2 / 0x48dd1c strb #3 / disconnect 写 0",
            ),
        );
        d.set(
            "tick_evidence",
            &GString::from(
                "arm64 0x48b488 ldrb w8,[x0,#0x68] (闩锁) / 0x48b4a4 cmp w8,#2 / 0x48b4b4 发握手 / \
                 0x48b518 strb #1,[x19,#0x68]",
            ),
        );
        d.set("game_state_on_failed", cch::GAME_STATE_ON_CONNECT_FAILED as i64);
        d.set("handshake", &GString::from(cch::MINIMAL_CLIENT_HANDSHAKE));
        d.set(
            "framework_note",
            &GString::from(
                "原版 30 s 由 libstd 的 connect_timeout 判（阻塞线程）；我方单线程非阻塞，\
                 每帧比 elapsed 是等价物，不是原版比较指令",
            ),
        );
        d.set("evidence_grade", &GString::from("逐位（4 态/超时/GameState）+ 观测（tick 分派）"));
        d
    }
}
