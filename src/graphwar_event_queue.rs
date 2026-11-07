//! 原版 `src/graphwar_event_queue.rs` —— **全游戏的发件箱**（"为什么它不在 25 个
//! manager 的注册表里"的答案）。
//!
//! ## 定案（2026-11-07，`docs/16-client.md` §30.3）
//!
//! 它**不是 manager**，所以不在 `GraphwarModuleContainer` 那张
//! `HashMap<TypeId, Rc<dyn Any>>` 里（`docs/02-module-map.md` §22）。
//! 它的持有者是 **`GraphwarGameClient`**：
//!
//! ```text
//! graphwarrust::graphwar_game_client::GraphwarGameClient::new +0xcdc
//!     → core::ptr::drop_in_place<core::cell::RefCell<GraphwarEventQueue>>
//!     （drop 路径落在 new 的恐慌清理段里 ⇒ 说明 `Rc<RefCell<GraphwarEventQueue>>`
//!       是 GameClient::new 里构造的局部量）
//! ```
//!
//! 其余所有 manager 拿的是 `Weak<RefCell<GraphwarEventQueue>>`
//! （`drop_in_place<alloc::rc::Weak<RefCell<GraphwarEventQueue>>>` 在
//! `Rc::drop_slow` 里被调到）。**一个 `Rc` 主人 + 一堆 `Weak` 用户**。
//!
//! ## 它是"唯一出口"：`add_event` 的调用面
//!
//! arm64 2.0.60 的 `bl` 图里，`GraphwarEventQueue::add_event`（`0x3d7a40`，276 字节）
//! 有 **200 余个**直接调用点，覆盖几乎每个 manager 的每个 `set_*` / `handle_*` /
//! `send_all_info_events`：
//! `graphwar_game_state_manager`（`start_game` / `set_state` / `request_*_mode_change` /
//! `add_soldier_to_player` / `execute_results` / `fire_functions` …）、
//! `graphwar_function_manager`、`graphwar_life_manager`、`graphwar_team_manager`、
//! `graphwar_player_manager`、`graphwar_soldier_manager`、`graphwar_turn_manager`、
//! `graphwar_obstacle_manager`、`graphwar_mode_manager`、`graphwar_color_manager`、
//! `graphwar_graphics_manager`、`graphwar_game_client`、`graphwar_game_server`、
//! `graphwar_remote_event_handler`、`graphwar_setup_control` …
//!
//! 结论：**"事件队列"是所有管理器共同的出站缓冲**，写进来到处都是，
//! 真正把它发到 socket 的只有一处。
//!
//! ## `send_buffer` 只有一个直接调用者（这条**反常**，如实记录）
//!
//! `GraphwarEventQueue::send_buffer`（`0x3d793c`，260 字节）在 arm64 `bl` 图里
//! **只有 1 个直接调用者**：`GraphwarLevelGenerator::generate_level_custom +0x424`。
//!
//! 三条可能的解释，本仓库**不选定**（标 `推断`）：
//!
//! 1. 其余冲刷点被**内联**进了 `GraphwarClientConnectionHandler::tick` /
//!    `GraphwarServerConnectionHandler::tick`（这两个 tick 在本版里都被大量内联，
//!    `bl` 图看不到）；
//! 2. 冲刷走**函数指针槽**（`call *slot`）——本仓库的 `bl` 图不覆盖这种情况；
//! 3. 真的只有关卡生成会主动冲刷，平时靠 tick 里的别处路径。
//!
//! 要定死这一条需要 `tools/ptr_slots.py` 这类间接派发扫描，**本轮未做**。
//!
//! ## 函数清单（arm64 符号，`观测`）
//!
//! `GraphwarEventQueue`: `add_event`、`send_buffer`。没有 `new`、没有 `tick`、
//! 没有 `subscribe_to_events_event_handler` —— 与"manager"的形态（`new` +
//! `subscribe_to_events_event_handler` + `send_all_info_events`）**完全不同**，
//! 这是它不在注册表里的结构性原因。

use std::collections::VecDeque;

use crate::events::Event;

/// 出站事件缓冲。原版字段只有一个 `VecDeque<GraphwarEvent>` 加"往哪发"的句柄；
/// 本 crate 只还原**队列语义**，发送端抽象成 [`GraphwarEventQueue::send_buffer`]
/// 把整批取走（原版是同名方法直接在 `Rc<RefCell<Self>>` 上做 socket 写）。
///
/// 证据：`send_buffer` 的函数体里出现 `VecDeque::grow`、
/// `drop_in_place<GraphwarEvent>`、`GraphwarEvent::clone`
/// ⇒ 队列是 `VecDeque<GraphwarEvent>`，且冲刷时**先 clone 再发**
/// （发出去了但队列要等克隆成功才清空 —— 失败重试的形态）。
#[derive(Debug, Default)]
pub struct GraphwarEventQueue {
    buffer: VecDeque<Event>,
}

impl GraphwarEventQueue {
    pub fn new() -> Self {
        Self { buffer: VecDeque::new() }
    }

    /// 对应 `add_event`（arm64 `0x3d7a40`）。原版就是 `push_back`，无去重、无排序。
    pub fn add_event(&mut self, event: Event) {
        self.buffer.push_back(event);
    }

    /// 对应 `send_buffer`（arm64 `0x3d793c`）：把当前积压**整批取走**。
    ///
    /// 原版在这里做的是 `drain` + clone 后逐条写 socket；写失败的行为本轮
    /// **未观测**（标 `未知`），所以这里只保证"取走"这一半语义。
    pub fn send_buffer(&mut self) -> Vec<Event> {
        self.buffer.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}
