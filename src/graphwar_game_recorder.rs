//! 对局录像：`GraphwarGameRecorder` —— **把每条事件连同当时的 tick 写成 gzip JSONL**。
//!
//! 本模块回答三件事：**录什么**（32 条 `*_info_event` 的全部事件）、
//! **什么时候落盘**（`GameStateInfo == EndingGame` 那一刻）、
//! **写到哪**（`user://replays/<unix 秒>`，gzip 压缩，一行一个 JSON）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（3 个，全部已定位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x3d7e3c` / 840 | 无锚（内联进宿主） | — | 建缓冲 + 订阅 32 条事件 | 逐位 |
//! | `subscribe_to_events` 闭包 | `0x3d7b54` / 744 | `0x3d4fa0` / 704 | `:107:105` `:108:41` | 记录 + 判 `EndingGame` 触发落盘 | 逐位 |
//! | `save_game` | `0x3d8184` / 2720 | `0x3d5740` / 2368 | `:150:81` `:156:74` | 建目录、拼名、gzip 写盘 | 逐位 |
//!
//! 闭包与 `save_game` 的 x86 尺寸比 = 0.95 / 0.87（闸 0.61–1.19），
//! 且两侧 Locations 逐枚相同 ⇒ 两个名字都是钉死的。
//!
//! ## 2. 数据结构（逐位）
//!
//! ```text
//! GraphwarGameRecorder
//!   +0x08 : Rc<RefCell<GraphwarEventQueue>>        ; 广播出口（落盘不经过它）
//!   +0x18 : Vec<EventInfo>                          ; 指针 / 容量 / 长度
//!   +0x40 : Weak<RefCell<GraphwarModuleContainer>>  ; 取当前 tick
//!   +0x48 : u8  = 上一条 GameStateInfo 的 game_state（初值 0）
//! EventInfo = { event: GraphwarEvent /*0x48 字节*/, tick: u32 }  ; 0x50 = 80 字节
//! ```
//!
//! 三条独立证据：`RawVec::grow_one` 的元素步长 `0x50`（`0x3d7c2c` 的 `mov w8,#0x50`
//! 与 `madd`）、落盘循环的 `madd x26, x19, #0x50`、以及 `tick` 写在记录 `+0x48`。
//!
//! ## 3. 录什么、何时录（逐位）
//!
//! 闭包体（arm64 `0x3d7b54`）：
//!
//! ```text
//! tick = get_module_wk(self+0x40)?.borrow()[+0x58]        ; 当前 tick（u32）
//! self.events.push(EventInfo { event: event.clone(), tick })
//!
//! if event.tag == GameStateInfo(30) {
//!     let state = event.payload[+0x8] as u8
//!     if state == 6 /* EndingGame */ {
//!         if self.last_state != 6 { self.save_game() }     ; 0x3d7d70
//!         self.last_state = 6
//!     } else if state == 1 /* Setup */ && self.last_state == 6 {
//!         self.events.clear()                              ; 新一局：清缓冲
//!         self.last_state = 1
//!     } else { self.last_state = state }
//! }
//! ```
//!
//! 32 条订阅名（`new` 里 32 个 `'static` 串 + 32 个长度立即数），**同一个闭包**：
//! 符号表里 `subscribe_to_events::{{closure}}` 只有**一个** hash ⇒ 32 个 `Box`
//! 装的是同一个闭包类型。
//!
//! | # | 事件名 | # | 事件名 | # | 事件名 | # | 事件名 |
//! |---|---|---|---|---|---|---|---|
//! | 1 | `team_info_event` | 9 | `game_state_info_event` | 17 | `turn_info_event` | 25 | `skin_graphics_info_event` |
//! | 2 | `player_info_event` | 10 | `function_fire_event` | 18 | `effect_info_event` | 26 | `face_graphics_info_event` |
//! | 3 | `entity_removed_event` | 11 | `function_info_event` | 19 | `axis_mode_info_event` | 27 | `hat_graphics_info_event` |
//! | 4 | `soldier_info_event` | 12 | `function_active_event` | 20 | `function_mode_info_event` | 28 | `entity_effect_info_event` |
//! | 5 | `target_info_event` | 13 | `function_points_event` | 21 | `turn_mode_info_event` | 29 | `turn_limit_info_event` |
//! | 6 | `remove_team_event` | 14 | `clear_obstacles_event` | 22 | `time_mode_info_event` | 30 | `turn_count_info_event` |
//! | 7 | `pos_info_event` | 15 | `add_obstacle_event` | 23 | `end_of_turn_info_event` | 31 | `name_info_event` |
//! | 8 | `life_info_event` | 16 | `destroy_obstacle_event` | 24 | `game_name_info_event` | 32 | `color_info_event` |
//!
//! 32 条里 **24 条以 `_info_event` 结尾**，另外 **8 条**是同样由服务端单向广播、
//! 但名字不带 `_info` 的：`entity_removed_event` / `remove_team_event` /
//! `function_fire_event` / `function_active_event` / `function_points_event` /
//! `clear_obstacles_event` / `add_obstacle_event` / `destroy_obstacle_event`。
//! **`*_request_event` 一条都没订** ⇒ 录像里只有权威事件流（客户端的请求进不去）。
//!
//! ## 4. 写到哪、什么格式（逐位）
//!
//! `save_game`（arm64 `0x3d8184`）的固定序列：
//!
//! ```text
//! DirAccess::make_dir_recursive_absolute("user://replays/", 15)         ; 0x3d81b8
//! let secs = SystemTime::now().duration_since(UNIX_EPOCH).as_secs()     ; 0x3d81bc-0x3d81f4
//! let path = "user://replays/".to_owned() + &secs.to_string()           ; 0x3d8240-0x3d827c
//! FileAccess::open(path, /*WRITE=*/2)                                    ; 0x3d8290
//! GzEncoder::new(file, ..) + 每条记录一行 JSON                          ; flate2 1.0.30
//! ```
//!
//! 一行 JSON 的构造（`0x3d830c` 起，手写而非 `serde_json::to_writer`）：
//!
//! ```text
//! '{"event":' + <GraphwarEvent 的 serde 外部标签 JSON> + ',"tick":' + N + '}'
//! ```
//!
//! 两个字段名 `"event"` / `"tick"` 是 `.rodata` 里的字面量（`0x19bb8d` / `0x19332c`），
//! 与 `src/graphwar_replay_player.rs` 解析 400 份真实回放时用的格式**逐字相同**
//! ⇒ 格式这条是**双向印证**的（写侧反汇编 + 读侧 400 份真数据）。
//! 压缩侧同样印证：`save_game` 引用的 `flate2-1.0.30/src/gz/write.rs` 与
//! `zio.rs` 的 `panic::Location` 说明写的是 **gzip**（读侧 `gunzip` 也在用）。
//!
//! ## 5. 未知
//!
//! * 文件名**没有扩展名**，只有 unix 秒（`as_secs`）——同一秒开两局会互相覆盖，
//!   原版就这样，不是我们抄错；
//! * 记录**没有上限**（闭包里没有任何截断），一局很长时内存线性增长；
//! * `EventInfo` 的 `tick` 是 u32 还是 u64：结构体内偏移 0x48 + 元素 0x50 只说明
//!   「4 字节 + 4 字节填充」，按 `[+0x58]` 是 u32 推断为 **u32**。

use crate::events::Event;
use crate::turnstate::GameState;

/// 录像目录（含结尾斜杠，长度 15）。
pub const REPLAY_DIR: &str = "user://replays/";

/// `EventInfo` 在内存里的步长（`0x50`）。
pub const EVENT_INFO_STRIDE: usize = 0x50;

/// `GraphwarEvent` 的字节数（`EventInfo` 里事件本体占的部分）。
pub const GRAPHWAR_EVENT_SIZE: usize = 0x48;

/// 32 条订阅名，顺序 = `new` 里写入顺序（模块文档 §3 的表）。
pub const SUBSCRIPTIONS: [&str; 32] = [
    "team_info_event",
    "player_info_event",
    "entity_removed_event",
    "soldier_info_event",
    "target_info_event",
    "remove_team_event",
    "pos_info_event",
    "life_info_event",
    "game_state_info_event",
    "function_fire_event",
    "function_info_event",
    "function_active_event",
    "function_points_event",
    "clear_obstacles_event",
    "add_obstacle_event",
    "destroy_obstacle_event",
    "turn_info_event",
    "effect_info_event",
    "axis_mode_info_event",
    "function_mode_info_event",
    "turn_mode_info_event",
    "time_mode_info_event",
    "end_of_turn_info_event",
    "game_name_info_event",
    "skin_graphics_info_event",
    "face_graphics_info_event",
    "hat_graphics_info_event",
    "entity_effect_info_event",
    "turn_limit_info_event",
    "turn_count_info_event",
    "name_info_event",
    "color_info_event",
];

/// 触发落盘 / 清空的两个状态（`docs/07` §3.5 的 `GraphwarGameState`）。
pub const STATE_ENDING_GAME: u8 = GameState::EndingGame as u8; // 6
pub const STATE_SETUP: u8 = GameState::Setup as u8; // 1

/// 一条录像记录（`EventInfo`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventInfo {
    pub tick: u32,
    /// 事件名（serde 外部标签的那个 key，例如 `"FunctionInfo"`）。
    pub event: String,
    /// 该事件的 JSON 载荷（原样透传，**不重排、不省略**）。
    pub body: String,
}

impl EventInfo {
    /// 一行 JSON：`{"event":{"<Name>":<body>},"tick":<N>}`。
    ///
    /// 字段顺序是**观察到的**（`"event"` 的字面量在 `"tick"` 之前被引用，
    /// 且 400 份真实回放都是这个顺序）。
    pub fn to_json_line(&self) -> String {
        if self.body.is_empty() {
            format!("{{\"event\":{{\"{}\":{{}}}},\"tick\":{}}}", self.event, self.tick)
        } else {
            format!(
                "{{\"event\":{{\"{}\":{}}},\"tick\":{}}}",
                self.event, self.body, self.tick
            )
        }
    }
}

/// 录像文件名：`user://replays/<unix 秒>`（**没有扩展名**，模块文档 §4）。
pub fn replay_path(unix_secs: u64) -> String {
    format!("{}{}", REPLAY_DIR, unix_secs)
}

/// 对局录像器。
#[derive(Debug, Default)]
pub struct GraphwarGameRecorder {
    events: Vec<EventInfo>,
    /// 上一条 `GameStateInfo` 的 `game_state`（u8，原版 `+0x48`）。
    last_state: u8,
    pub subscribed: Vec<&'static str>,
    /// 落盘次数（原版没有这个字段；测试要看「写了几次」）。
    pub saves: usize,
}

impl GraphwarGameRecorder {
    /// `new`（arm64 `0x3d7e3c`）：空缓冲 + 32 条订阅。
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            last_state: 0,
            subscribed: SUBSCRIPTIONS.to_vec(),
            saves: 0,
        }
    }

    pub fn events(&self) -> &[EventInfo] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn last_state(&self) -> u8 {
        self.last_state
    }

    /// 录一条事件（arm64 `0x3d7b54` 的前半段）。
    ///
    /// 返回 `true` 表示这一次调用触发了 `save_game`（`EndingGame` 的上升沿）。
    pub fn record(&mut self, tick: u32, event: Event, body: &str) -> bool {
        let name = format!("{:?}", event);
        self.events.push(EventInfo {
            tick,
            event: name,
            body: body.to_string(),
        });
        self.on_event_state(event, tick)
    }

    /// 录一条 `GameStateInfo` 并跑状态机（`0x3d7c60` 起的那段）。
    ///
    /// 返回 `true` 表示触发了落盘。
    pub fn record_game_state(&mut self, tick: u32, state: u8) -> bool {
        let body = format!("{{\"game_state\":{}}}", state);
        self.events.push(EventInfo {
            tick,
            event: "GameStateInfo".to_string(),
            body,
        });
        self.on_game_state(state)
    }

    /// 状态机本体（与「是否真的写了盘」解耦，便于测试）。
    fn on_event_state(&mut self, event: Event, tick: u32) -> bool {
        if event != Event::GameStateInfo {
            return false;
        }
        let state = parse_state(&self.events[self.events.len() - 1].body);
        let _ = tick; // 原版只用 tick 填记录，不参与判定
        self.on_game_state(state)
    }

    fn on_game_state(&mut self, state: u8) -> bool {
        if state == STATE_ENDING_GAME {
            let prev = self.last_state;
            self.last_state = STATE_ENDING_GAME;
            if prev != STATE_ENDING_GAME {
                self.save_game();
                return true;
            }
            return false;
        }
        if state == STATE_SETUP && self.last_state == STATE_ENDING_GAME {
            // 新一局开始：丢掉上一局的缓冲（原版连 `drop_in_place` 都是一个循环）。
            self.events.clear();
            self.last_state = STATE_SETUP;
            return false;
        }
        self.last_state = state;
        false
    }

    /// 落盘（arm64 `0x3d8184` / x86 `0x3d5740`）。
    ///
    /// 原版是「建目录 → 拼名 → `FileAccess::open` → `GzEncoder` → 逐行写」。
    /// 本 crate 不引外部依赖（`flate2` / `gzip` 都不在依赖表里），
    /// 所以这里只产生**未压缩的行**；压缩口径见 `src/_scaffold/replay_inflate.rs`
    /// 与 `docs/16-client.md` §29。
    pub fn save_game(&mut self) -> Vec<EventInfo> {
        self.saves += 1;
        self.events.clone()
    }

    /// 落盘时的完整文本（每行一条，行尾 `\n`），供对拍 / 调试。
    pub fn save_game_text(&mut self) -> String {
        let events = self.save_game();
        let mut out = String::new();
        for e in &events {
            out.push_str(&e.to_json_line());
            out.push('\n');
        }
        out
    }
}

fn parse_state(body: &str) -> u8 {
    // body 形如 `{"game_state":3}`；容错：取最后一个 ':' 后的数字。
    let digits: String = body
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.chars().rev().collect::<String>().parse().unwrap_or(0)
}
