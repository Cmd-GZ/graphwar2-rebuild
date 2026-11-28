//! **不是原版的东西**：把三处"时序/握手时序"的真值接给 GDScript（本文件是外壳，不是复原物）。
//!
//! # 为什么会有这一份（T99 / `docs/06` §144）
//!
//! `client/src/screens/play_screen.gd` 与 `client/src/net/session.gd` 里有三段**时序逻辑**
//! 在原版 Rust 侧本来就有复原实现，却在 `.gd` 里又写了一遍（同一规则两个真相源）：
//!
//! | 原来在 `.gd` | 改前的实现 | 原版真值（ELF vaddr） | 档位 |
//! |---|---|---|---|
//! | `play_screen.gd:1220` `_online_poll_tick()` 的 **`TickRequest` 节奏** | 墙钟累加器 `_tick_request_acc += delta`，满 1.0 s 就发一条（`fmod` 重置） | [`GraphwarTickHandler::tick`] = **`0x519840`**：`accum += 16_666_666 ns`、`tick += 1`，**`tick % 60 == 0`** 时再发一条 `TickRequest`（幻数 `imul 0xeeeeeeef` + `ror 2` + `cmp 0x4444444` 就是判 60 的整除） | **逐位** |
//! | 同上，`_reset_shot_state()` 里那句 `_tick_request_acc = TICK_REQUEST_PERIOD`（"连上就发一条"） | 把累加器推到门槛上当替身 | [`GraphwarTickHandler::handle_connected_to_server_event`] = **`0x519630`**：**不看**事件载荷，直接发一条 `TickRequest` | **逐位** |
//! | `session.gd:67` `STATE_SYNC_KINDS`（7 条状态同步名单）+ `session.gd:818` 版本串 `"2.0"`/`"60"` | 手抄两份 | [`crate::graphwar_server_connection_handler::on_new_connection`]（7 条的**名单与顺序**，`state_sync_matches_captured_bytes` 与抓包逐字节）；[`crate::graphwar_client_connection_handler::NEW_CONNECTION_REQUEST_MAJOR`] / `…_MINOR`（arm64 `0x4693c0`：`mov w9,#0x2e32` = `"2."`、`mov w8,#0x30` = `'0'`；`mov w9,#0x3036` = `"60"`） | **逐位** |
//!
//! # 这是脚手架，不是原版的类
//!
//! 原版 `libgraphwarrust.so` 里**没有** `GwFlowClock` 这个类：
//!
//! * `GraphwarTickHandler`（72 字节）是 `GraphwarGameClient` 内部的一个字段（槽 `0x40b528` 那一片，
//!   见 `src/_scaffold/root_wiring.rs` 的 `TickHandlerSlot`），它的订阅是
//!   `connected_to_server_event`、发件是 `TickEvent`/`TickRequest`；
//! * 7 条状态同步的名单在原版是**服务端**函数（`on_new_connection` 的发送侧）与客户端
//!   `GraphwarRemoteEventHandler` 的订阅侧，两侧都**没有**一张"名单常量表"。
//!
//! 我们这边因为拿不到原版的 `GraphwarGameClient`/`GraphwarRemoteEventHandler` 函数边界
//! （`docs/02` §17 的待拆表），只能在 `.gd` ↔ Rust 之间放一个取值/判据出口。
//!
//! # 一台时钟（T104 / `docs/06` §150 第③条：把 §144.6 #3 的遗留收掉）
//!
//! 原版**只有一台** `GraphwarTickHandler`（平面通过它取 `get_fractional_tick`；同名 tick 计数
//! 又驱动 `TickRequest` 的 `%60` 判据）。T99 时我方是**两台同源时钟**：平面那台
//! （`GraphwarPlane::drive_frame`，**`0x516fd0` → `0x518400` → `0x518240` → `0x519840`**）
//! 把发件箱 `clear()` 丢掉，于是本类只好自己再握一台复现 `%60` 节奏。
//!
//! **本类不再握时钟**：节奏与计数都回到平面那台（`GraphwarPlane::drive_frame` 现在把
//! `TickEvent`/`TickRequest` **drain 成计数**，由 `take_tick_requests()` 取走 —— 判决依据是
//! `0x518400` 每 tick 之后紧接 `0x51849f call rbp`（`0x3d3630`，`graphwar_event_handler.rs:467`
//! 的事件泵），即原版**确实**把发件箱泵出去）。
//!
//! 本类现在只剩三类**纯**出口：
//!
//! * 联机倒计时的换算 [`GwFlowClock::seconds_left_in_turn`]（`0x3dcd47`–`0x3dcd4c`）；
//! * 握手那批的名单 [`GwFlowClock::state_sync_kinds`] 与版本串
//!   [`GwFlowClock::new_connection_request_body`]（与时钟无关，一直在用）；
//! * 计数/取证（`note_requests` / `note_frame` / `reset` / `snapshot`）—— **不是**时钟。
//!
//! # 接口
//!
//! ```gdscript
//! # 节奏与 tick 计数**只在平面那台时钟里**（一台）：
//! _plane.drive_frame(delta)            # 0x518400 的定步追赶（每帧一次）
//! var due := _plane.take_tick_requests()   # 本帧该发几条 TickRequest（0x519840 的 %60）
//! _plane.tick_connected()              # 连上那一条（0x519630，不看载荷）
//! _plane.integer_tick_now()            # 当前 tick（0x4b6317 的 M+0x40）
//!
//! # 本类只剩纯出口（无状态、无时钟）：
//! GwFlowClock.seconds_left_in_turn(end_of_turn_tick, now_tick)  # → 剩余秒（-1 = 无截止）
//! GwFlowClock.state_sync_kinds()          # → ["NewConnection", …]（7 条，顺序 = 抓包顺序）
//! GwFlowClock.new_connection_request_body()# → '{"major_version":"2.0","minor_version":"60"}'
//! # 取证计数（可选的实例）：
//! var c := GwFlowClock.new(); c.note_requests(n); c.requests_total()
//! ```
//!
//! 全部是**关联函数**（与 `GwNetConsts.xxx()` 同一形态）；实例只剩计数。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_client_connection_handler as cch;
use crate::graphwar_server_connection_handler as sch;

/// 联机时序的**纯**出口：倒计时换算 + 握手那批的名单/版本串（+ 取证计数）。
///
/// ⚠️ **不持有 `GraphwarTickHandler`**（T104 起）：那台时钟只有一个实例，
/// 在 [`crate::graphwar_plane::GraphwarPlane`] 里（见文件头）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwFlowClock {
    base: Base<RefCounted>,
    /// 累计发过几条 `TickRequest`（**消费者侧的镜子**，不是时钟；验收/取证用）。
    requests_total: i64,
    /// 累计喂过多少帧（同上）。
    frames_total: i64,
}

#[godot_api]
impl IRefCounted for GwFlowClock {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            requests_total: 0,
            frames_total: 0,
        }
    }
}

#[godot_api]
impl GwFlowClock {
    /// 换局/重连：**计数**归零（本类不再有时钟，见文件头）。
    #[func]
    fn reset(&mut self) {
        self.requests_total = 0;
        self.frames_total = 0;
    }

    /// **消费者侧的镜子**：把平面那一帧真正发出去的 `TickRequest` 条数记一笔。
    ///
    /// 节奏判据**不在这里**（`0x519840` 的 `%60` 只在平面那台 `GraphwarTickHandler` 里算），
    /// 本方法纯粹给自检/文档取证留数。返回 `n`，方便 `.gd` 写成 `for _i in _flow_clock.note_requests(n)`。
    #[func]
    fn note_requests(&mut self, n: i64) -> i64 {
        self.requests_total += n;
        n
    }

    /// 记一帧（取证用）。
    #[func]
    fn note_frame(&mut self) {
        self.frames_total += 1;
    }

    /// **联机倒计时的换算**：`end_of_turn_tick − now_tick` 的剩余**整秒**；`-1` = 没有截止。
    ///
    /// 判据出处（**逐位**，`docs/06` §150 P4-1）—— 播放屏那台 `process`（宿主 `0x3dcbb0`）里：
    ///
    /// ```text
    /// 0x3dccd6  mov  ebx, dword ptr [r15+0x58]  ; now_tick = GraphwarTickHandler.current_tick
    ///                                           ;   r15 = RefCell 守卫 ⇒ 载荷在 +0x18 ⇒ +0x58 = 载荷+0x40
    /// 0x3dcd2c  call 0x462ec0                   ; TurnManager::get_end_of_turn_tick → Option<u32>
    /// 0x3dcd35  test al, 1 / je                 ; None（TimerInf）⇒ 另一个分支（标签空白）
    /// 0x3dcd47  sub  edx, ebx                   ; end − now（u32 模运算）
    /// 0x3dcd49  cmovb edx, eax(=0)              ; **借位 ⇒ 0**（= saturating_sub，不出负数）
    /// 0x3dcd4c  imul rax, rdx ; shr rax, 0x25   ; / 60 ⇒ 剩余整秒（截断）
    /// ```
    ///
    /// ⇒ **两个操作数在同一根 60 Hz tick 轴上**（`end_of_turn_tick` 是服务端绝对 tick，
    /// `now_tick` 是客户端那台 tick handler 的 `current_tick`，由 `TickReply` 拨到同一条轴）。
    /// **不是**"回合时长 − 本回合已过 tick"（那是本地局的口径，见 `_turn_clock`）。
    /// 关联函数（与 [`GwFlowClock::state_sync_kinds`] 同一形态），GDScript 侧写
    /// `GwFlowClock.seconds_left_in_turn(...)` —— 它**不依赖那台 tick handler**，
    /// 纯换算（`now_tick` 由调用方给，联机路径给平面那台 handler 的 `integer_tick_now()`）。
    #[func]
    fn seconds_left_in_turn(end_of_turn_tick: i64, now_tick: i64) -> i64 {
        seconds_until_turn_end(end_of_turn_tick, now_tick).unwrap_or(-1)
    }


    /// 累计发过几条 `TickRequest`（自检/文档取证）。
    #[func]
    fn requests_total(&self) -> i64 {
        self.requests_total
    }

    /// 累计喂过多少帧（自检/文档取证）。
    #[func]
    fn frames_total(&self) -> i64 {
        self.frames_total
    }

    /// 只读快照（自检/诊断）。**没有 tick**：时钟只有平面那一台，读数问
    /// `GraphwarPlane.integer_tick_now()`（`0x4b6317` 的 `M+0x40`）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("owns_clock", false);
        d.set("requests_total", self.requests_total);
        d.set("frames_total", self.frames_total);
        d.set("ticks_per_request", crate::graphwar_tick_handler::TICKS_PER_REQUEST as i64);
        d.set("nanos_per_tick", crate::graphwar_tick_handler::NANOS_PER_TICK as i64);
        d
    }

    /// 7 条状态同步的**事件名**（顺序 = 真抓包顺序）。
    ///
    /// 真值不是我方编的表：名单与顺序来自
    /// [`crate::graphwar_server_connection_handler::on_new_connection`] 的**输出本身**
    /// （那个函数用 `state_sync_matches_captured_bytes` 与 `fixtures/protocol_match.jsonl` 逐字节钉住），
    /// 这里只是从每条报文里取出最外层的键。
    #[func]
    fn state_sync_kinds() -> VarArray {
        let state = sch::RoomState {
            room_name: "Graphwar Game".to_string(),
            game_state: "Setup".to_string(),
            axis_mode: "EveryUnit".to_string(),
            function_mode: "NormalFunction".to_string(),
            turn_mode: "SequentialTurns".to_string(),
            time_mode: "Timer1m".to_string(),
        };
        let mut out = VarArray::new();
        for line in sch::on_new_connection(&state, 0) {
            if let Some(kind) = wire_kind(&line) {
                out.push(&GString::from(kind));
            }
        }
        out
    }

    /// `NewConnectionRequest` 的**载荷 JSON**（`{"major_version":"2.0","minor_version":"60"}`）。
    ///
    /// 两个串来自 [`crate::graphwar_client_connection_handler::NEW_CONNECTION_REQUEST_MAJOR`] /
    /// `…_MINOR`（arm64 `0x4693c0` 的立即数），`.gd` 里不再出现字面量。
    #[func]
    fn new_connection_request_body() -> GString {
        GString::from(new_connection_request_json().as_str())
    }

}

/// `NewConnectionRequest` 的载荷 JSON（纯逻辑；两个串来自 `graphwar_client_connection_handler`
/// 的常量 —— arm64 `0x4693c0` 的立即数 `"2."`/`'0'`/`"60"`）。
fn new_connection_request_json() -> String {
    format!(
        r#"{{"major_version":"{}","minor_version":"{}"}}"#,
        cch::NEW_CONNECTION_REQUEST_MAJOR,
        cch::NEW_CONNECTION_REQUEST_MINOR
    )
}

/// **联机倒计时**：`end_of_turn_tick − now_tick` 折成整秒（`None` = 这一档没有截止）。
///
/// 逐位对应的指令（`docs/06` §150 P4-1，宿主 `0x3dcbb0`）：
///
/// ```text
/// 0x3dcd47  sub edx, ebx / 0x3dcd49 cmovb edx,0   ; saturating_sub（借位夹 0）
/// 0x3dcd4c  imul rax, rdx ; shr rax, 0x25         ; / 60，截断
/// ```
///
/// `TimerInf` 档原版走 `get_end_of_turn_tick() == None`（`0x3dcd35 test al,1 / je`），
/// 我方线上代理是 `end_of_turn_tick == 0`（`docs/14` §3.3.2 四档实测：TimerInf 线上写 `0`）
/// ⇒ `None` ⇒ 标签空白。
pub fn seconds_until_turn_end(end_of_turn_tick: i64, now_tick: i64) -> Option<i64> {
    if end_of_turn_tick <= 0 {
        return None; // `TimerInf` 哨兵（原版 Option::None；线上 0）
    }
    let left_ticks = (end_of_turn_tick - now_tick).max(0); // 0x3dcd49 cmovb：借位夹 0
    Some(left_ticks / crate::graphwar_tick_handler::TICKS_PER_SECOND as i64) // 0x3dcd4c：/60 截断
}

/// 一条线上报文 `{"Kind":{…}}` 的最外层键（= 事件名）。不是这个形状 ⇒ `None`。
fn wire_kind(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("{\"")?;
    let end = rest.find("\":")?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **联机倒计时的轴**（`docs/06` §150 P4-4）：`(end − now) / 60`，**截断**、下溢夹 0。
    ///
    /// 真值来自 `0x3dcd47`–`0x3dcd55`（`sub edx,ebx` / `cmovb edx,0` / `/60` 幻数
    /// `0x88888889 >> 0x25`）。这些断言钉住**边界**（到点 = `00:00` 而不是负数）与
    /// **`TimerInf` 哨兵**（线上 `0` ⇒ 空白），两处都是原版指令直接给的口径。
    #[test]
    fn turn_countdown_is_end_minus_now_on_the_same_tick_axis() {
        assert_eq!(seconds_until_turn_end(10_800, 0), Some(180));
        // `docs/92` §4.10 观测到"3 分钟档一开局就显示 `02:59`"：那是 `end − now` 在公告那一 tick
        // 已经是 179 秒左右（`end` 在回合起点算，`EndOfTurnInfo` 晚一两 tick 才到）——
        // **不是**这里再减 1。所以本条只钉 180/179 都由同一算式给出，不额外造偏移。
        assert_eq!(seconds_until_turn_end(10_800, 60), Some(179), "过 60 tick ⇒ 179 秒");
        assert_eq!(seconds_until_turn_end(10_800, 10_800), Some(0), "到点 == 00:00（不是负数）");
        assert_eq!(seconds_until_turn_end(10_800, 10_801), Some(0), "过点 ⇒ 夹 0（cmovb）");
        assert_eq!(seconds_until_turn_end(600, 537), Some(1), "63 tick ⇒ 1 秒（截断）");
        assert_eq!(seconds_until_turn_end(59, 0), Some(0), "不足 1 秒 ⇒ 00:00（整数除法）");
        assert_eq!(seconds_until_turn_end(0, 5), None, "TimerInf 哨兵（线上 0）⇒ 无截止");
        assert_eq!(seconds_until_turn_end(-1, 5), None, "负值也当哨兵，不给负数秒");
    }

    /// 版本串来自 `graphwar_client_connection_handler` 的两个常量（`0x4693c0` 的立即数），
    /// 且与抓包原文（`fixtures/protocol.jsonl:2`）逐字节相同。
    #[test]
    fn new_connection_body_is_the_two_constants() {
        assert_eq!(
            new_connection_request_json(),
            r#"{"major_version":"2.0","minor_version":"60"}"#
        );
        assert_eq!(cch::NEW_CONNECTION_REQUEST_MAJOR, "2.0");
        assert_eq!(cch::NEW_CONNECTION_REQUEST_MINOR, "60");
    }

    /// 正对照：7 条状态同步的**事件名与顺序**必须与真抓包一致
    /// （`fixtures/protocol_match.jsonl`；`on_new_connection` 那条 `state_sync_matches_captured_bytes` 钉住）。
    #[test]
    fn state_sync_names_come_from_the_captured_batch() {
        let state = sch::RoomState {
            room_name: "Graphwar Game".to_string(),
            game_state: "Setup".to_string(),
            axis_mode: "EveryUnit".to_string(),
            function_mode: "NormalFunction".to_string(),
            turn_mode: "SequentialTurns".to_string(),
            time_mode: "Timer1m".to_string(),
        };
        let kinds: Vec<String> = sch::on_new_connection(&state, 228)
            .iter()
            .filter_map(|l| wire_kind(l).map(|s| s.to_string()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                "NewConnection",
                "TurnModeInfo",
                "GameStateInfo",
                "GameNameInfo",
                "AxisModeInfo",
                "FunctionModeInfo",
                "TimeModeInfo",
            ]
        );
        // 与 `.gd` 改前那份名单逐个比（改前是第二份拷贝；改后它不在了，这条留作回归）。
        assert_eq!(kinds.len(), 7);
    }

    /// `wire_kind` 取的是"最外层第一个键"：对 `{"Kind":{…}}` 就是事件名，
    /// 对 LAN 那种裸 9 字段对象则会是第一个字段名 —— 所以它的**输入域只有
    /// `on_new_connection` 的输出**（这一点写死在函数注释里，别拿去解 LAN 广播）。
    #[test]
    fn wire_kind_takes_the_outermost_key() {
        assert_eq!(
            wire_kind(r#"{"NewConnection":{"connection_id":228}}"#),
            Some("NewConnection")
        );
        assert_eq!(wire_kind(r#"{"TimeModeInfo":{"time_mode":"Timer1m"}}"#), Some("TimeModeInfo"));
        // 不是 JSON 对象 ⇒ 没有键
        assert_eq!(wire_kind(""), None);
        assert_eq!(wire_kind("[1,2,3]"), None);
    }
}
