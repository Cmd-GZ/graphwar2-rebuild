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
//! # ⚠️ 一处**必须说明**的差：本类的 tick 时钟与平面的 tick 时钟是**两个对象**
//!
//! 原版**只有一台** `GraphwarTickHandler`（平面通过它取 `get_fractional_tick`，同名 tick 计数
//! 又驱动 `TickRequest` 的 `%60` 判据）。我方 `GraphwarPlane::drive_frame`（**`0x516fd0` →
//! `0x518400` → `0x518240` → `0x519840`**，T88 已接）里面**也**装了一台 `GraphwarTickHandler`，
//! 但它把发件箱丢掉了（`drive_frame` 里 `self.tick_source.outbox.clear()`，理由见那行注释：
//! 否则每帧一条 `TickEvent` 会无限长）。
//!
//! ⇒ 想做到"一台时钟"必须**改 `src/graphwar_plane.rs`** 把发件箱暴露出来；那不在 T99 的
//! 写范围内（任务书：不动 `src/graphwar_*.rs`）。所以本类自己拿一台，**用同一个 `delta`、
//! 同一条 `NANOS_PER_TICK` 折法**（[`crate::graphwar_tick_handler::fold_dt_into_ticks`]）驱动，
//! 正常帧率下与平面的整数 tick 恒等；只有"一帧补 tick 的上限"不同（平面 `MAX_CATCHUP_TICKS=600`
//! 会丢余数，本类不丢）。**这条差异登记在 `docs/06` §144 的"遗留"里**，不许当成已复原。
//!
//! # 接口
//!
//! ```gdscript
//! var c := GwFlowClock.new()
//! c.reset()                     # 换局/重连：计数器与余数归零
//! c.connected()                 # → 1：模拟 0x519630"连上立刻发一条 TickRequest"
//! c.drive(delta)                # → n：这一帧该发几条 TickRequest（0x519840 的 %60 判据）
//! c.tick()                      # 当前 tick 计数（诊断）
//! c.requests_total()            # 累计发过几条（自检/文档取证）
//! GwFlowClock.state_sync_kinds()          # → ["NewConnection", …]（7 条，顺序 = 抓包顺序）
//! GwFlowClock.new_connection_request_body()# → '{"major_version":"2.0","minor_version":"60"}'
//! ```
//!
//! 前五个是实例方法（要持有那台 `GraphwarTickHandler`），后两个是**关联函数**
//! （与 `GwNetConsts.xxx()` 同一形态）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_client_connection_handler as cch;
use crate::graphwar_server_connection_handler as sch;
use crate::events::Event;
use crate::graphwar_tick_handler::{fold_dt_into_ticks, GraphwarTickHandler};

/// 联机时序的两件真值：`TickRequest` 的**节奏**，以及握手那一批的**名单与版本串**。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwFlowClock {
    base: Base<RefCounted>,
    /// 原版 `GraphwarTickHandler`（`new` arm64 `0x40b528`）。**只当计时/节奏用**。
    handler: GraphwarTickHandler,
    /// 帧内余数纳秒（`fold_dt_into_ticks` 的进位器）。与平面各自持有一份，理由见文件头。
    carry_ns: u64,
    /// 累计发过几条 `TickRequest`（我方计数，验收用）。
    requests_total: i64,
    /// 累计喂过多少帧（我方计数，验收用）。
    frames_total: i64,
}

#[godot_api]
impl IRefCounted for GwFlowClock {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            base,
            handler: GraphwarTickHandler::new(0, 0),
            carry_ns: 0,
            requests_total: 0,
            frames_total: 0,
        }
    }
}

#[godot_api]
impl GwFlowClock {
    /// 换局/重连：tick 计数、余数、累计量全部归零（`GraphwarTickHandler::new` 的语义）。
    #[func]
    fn reset(&mut self) {
        self.handler = GraphwarTickHandler::new(0, 0);
        self.handler.take_outbox();
        self.carry_ns = 0;
        self.requests_total = 0;
        self.frames_total = 0;
    }

    /// `GraphwarTickHandler::handle_connected_to_server_event`（**`0x519630`**）：
    /// **不看**载荷直接发一条 `TickRequest`。
    ///
    /// 返回这一下**该发几条**（恒 `1`；写成计数是为了让 `.gd` 与 [`Self::drive`] 同一形态）。
    #[func]
    fn connected(&mut self) -> i64 {
        self.handler.handle_connected_to_server_event();
        self.drain_requests()
    }

    /// 每帧推进（`GraphwarTickHandler::tick`，**`0x519840`**）：把帧间隔折成整数 tick，
    /// 每 tick 一次 `tick()`，`tick % TICKS_PER_REQUEST(60) == 0` 的那几次会再发一条 `TickRequest`。
    ///
    /// 返回这一帧**该发几条 `TickRequest`**（正常帧率下每 60 帧一条）。
    #[func]
    fn drive(&mut self, delta: f64) -> i64 {
        self.frames_total += 1;
        let n = drive_handler(&mut self.handler, &mut self.carry_ns, delta);
        self.requests_total += n;
        n
    }

    /// 当前 tick 计数（`GraphwarTickHandler` 的 `+0x40`；诊断/自检用）。
    #[func]
    fn tick(&self) -> i64 {
        self.handler.current_tick as i64
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

    /// 只读快照（自检/诊断）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        d.set("tick", self.handler.current_tick as i64);
        d.set("carry_ns", self.carry_ns as i64);
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

    /// 从发件箱里数出 `TickRequest` 的条数（并把发件箱清空 —— 我方没有事件队列消费者）。
    fn drain_requests(&mut self) -> i64 {
        let n = take_requests(&mut self.handler);
        self.requests_total += n;
        n
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

/// 把一帧的 `delta` 折成整数 tick 并逐个 [`GraphwarTickHandler::tick`]，
/// 返回这一帧该发几条 `TickRequest`（纯逻辑，单测直接喂它；类里那两行只是加上计数）。
fn drive_handler(handler: &mut GraphwarTickHandler, carry_ns: &mut u64, delta: f64) -> i64 {
    let n = fold_dt_into_ticks(delta, carry_ns);
    for _ in 0..n {
        handler.tick();
    }
    take_requests(handler)
}

/// 数出发件箱里的 `TickRequest` 并清空（`TickEvent` 我方没有消费者，直接丢）。
fn take_requests(handler: &mut GraphwarTickHandler) -> i64 {
    handler
        .take_outbox()
        .iter()
        .filter(|e| matches!(e, Event::TickRequest))
        .count() as i64
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

    /// 节奏：`60` 个 tick 一条 `TickRequest`（`0x519840` 的 `%60`），
    /// 且连上时那条走 `0x519630`（**不看** tick 计数）。
    #[test]
    fn tick_request_every_sixty_ticks() {
        let mut h = GraphwarTickHandler::new(0, 0);
        let mut carry = 0u64;
        // 0x519630：连上立刻一条。
        h.handle_connected_to_server_event();
        assert_eq!(take_requests(&mut h), 1);
        let mut fired = 0;
        for _ in 0..60 {
            fired += drive_handler(&mut h, &mut carry, 1.0 / 60.0);
        }
        assert_eq!(h.current_tick, 60);
        assert_eq!(fired, 1, "第二次触发在 tick=60，不在 tick=59");
    }

    /// 折法是整数纳秒（`fold_dt_into_ticks`）：120 个半帧 = 60 tick，余数不许攒出多余 tick。
    #[test]
    fn fractional_frames_do_not_drift() {
        let mut h = GraphwarTickHandler::new(0, 0);
        let mut carry = 0u64;
        for _ in 0..120 {
            drive_handler(&mut h, &mut carry, 1.0 / 120.0);
        }
        assert_eq!(h.current_tick, 60);
        assert!(carry < crate::graphwar_tick_handler::NANOS_PER_TICK as u64);
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
