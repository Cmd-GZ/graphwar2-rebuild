//! T49：`src/_scaffold/` 里装着的**游戏语义**搬回原版模块后的行为闸。
//!
//! 每个断言都指到**指令**（不是指到我方实现）：期望值来自 `tools/addr_probe.py dis <vaddr>`
//! 读出的立即数/分支，不是"我方跑出来是多少就是多少"。
//!
//! 覆盖三处（第 ④ 处 `net_events_class` 的派发层在 `tests/wire_format.rs` 与
//! `src/_scaffold/net_events_class.rs` 的既有 8 条单测里，这里不重复）：
//!
//! | 处 | 函数 | vaddr | 本轮校正的语义 |
//! |---|---|---|---|
//! | ① | `GraphwarObstacleManager::{add_obstacle, destroy_obstacle, is_obstacle, queue_destroy_obstacle, handle_tick_event}` | `0x3f17a0` / `0x3f1b30` / `0x3f15b0` / `0x3f20f0` / `0x3f1d30` | 排队键 = 实参 tick（无 `now+delay`）；`handle_tick_event` 按键**等值**删；fill 0x00/0xff |
//! | ② | `GraphwarFunctionManager::{set_function, set_function_active_tick, set_function_points}` | `0x455e30` / `0x456b80` / `0x456810` | 三个 `set_*` **各发一条**事件：tag **35 / 36 / 37** |
//! | ③ | `GraphwarEventHandler::{subscribe_to_event, process_events}` | `0x3d38f0` / `0x3d3630` | 名字查表 + 无人订阅则丢 |

// `_scaffold::root_wiring` 只在 `gdext-bindings` 下编译（与它照旧是客户端接线层相一致），
// 所以"老路径仍指到同一类型"这条闸只在那个特性下跑；其余断言与特性无关。
#![cfg(feature = "gdext-bindings")]

use gw2_rebuild::events::Event;
use gw2_rebuild::graphwar_event_handler::GraphwarEventHandler;
use gw2_rebuild::graphwar_function_manager::FunctionTable;
use gw2_rebuild::graphwar_obstacle_manager::ObstacleTable;
use gw2_rebuild::graphwar_tick_handler::TICKS_PER_SECOND;
use gw2_rebuild::_scaffold::root_wiring::ObstacleTable as ScaffoldObstacleTable;
use gw2_rebuild::_scaffold::root_wiring::FunctionTable as ScaffoldFunctionTable;

/// 搬完之后**老路径**仍然指到同一个类型（调用方零改动的机器证据）。
#[test]
fn scaffold_paths_alias_the_original_module_types() {
    // 这一句能编译就是证据：`_scaffold::root_wiring::ObstacleTable` 与
    // `graphwar_obstacle_manager::ObstacleTable` 是**同一个类型**（`pub use`，不是第二份实现）。
    let a: ObstacleTable = ScaffoldObstacleTable::new();
    assert_eq!(a.mask().len(), 1000 * 600, "网格 1000×600（0x3f11e0 的 memset 长度 600000）");
    let b: FunctionTable = ScaffoldFunctionTable::new();
    assert!(b.is_empty());
}

/// ① `GraphwarObstacleData::draw_circle` 的 fill 是**第 5 实参**，两个调用点各传常量：
/// `add_obstacle` `0x3f1815 xor edx,edx` = `0x00`；`destroy_obstacle` `0x3f1ba5 mov edx,0xff`。
///
/// 判据用**网格里的字节**：画完圆之后圆心那格必须是 `0x00`；挖掉之后回到 `0xff`。
#[test]
fn obstacle_fill_values_come_from_the_two_call_sites() {
    let mut t = ObstacleTable::new();
    // 空网格全是 0xff（`new` 里 memset(0xff, 600000)）
    assert!(t.mask().iter().all(|b| *b == 0xff));

    t.add_obstacle(0.0, 0.0, 1.0);
    // 圆心格：x = 1000*0/50+500 = 500，y = 1000*0/50+300 = 300 ⇒ idx = 300*1000+500
    assert_eq!(t.mask()[300 * 1000 + 500], 0x00, "add_obstacle 的 fill = 0x00（0x3f1815）");
    assert!(t.is_obstacle(0.0, 0.0), "画上之后该点被挡（grid != 0xff）");

    t.destroy_obstacle(0.0, 0.0, 1.0);
    assert_eq!(t.mask()[300 * 1000 + 500], 0xff, "destroy 的 fill = 0xff（0x3f1ba5）");
    assert!(!t.is_obstacle(0.0, 0.0), "挖掉之后不挡");

    t.clear_obstacles();
    assert!(t.mask().iter().all(|b| *b == 0xff), "clear_obstacles 的 memset 值 = 0xff（0x3f19fa）");
}

/// ① `is_obstacle`（`0x3f15b0`）：**越界算命中**（`0x3f1669 mov al,1` 在两条 `cmp; ja` 之前）。
#[test]
fn is_obstacle_out_of_board_counts_as_blocked() {
    let t = ObstacleTable::new();
    assert!(!t.is_obstacle(0.0, 0.0), "空格子不挡");
    // 界内两端：x = -25 ⇒ 0、x = 24.95 ⇒ 999（都是合法下标，但格子里是 0xff ⇒ 不挡）
    assert!(!t.is_obstacle(-25.0, 0.0), "x = 0 是界内");
    assert!(!t.is_obstacle(24.95, 0.0), "x = 999 是界内（`cmp ecx,0x3e7; ja` 的上界）");
    // 越界：x = 25.04 ⇒ 1000（> 999）；y = 15 ⇒ 600（> 599）
    assert!(t.is_obstacle(25.04, 0.0), "x = 1000 ⇒ 越界 ⇒ 真（`0x3f1669 mov al,1` 在比较之前）");
    assert!(t.is_obstacle(0.0, 15.0), "y = 600 ⇒ 越界 ⇒ 真（`cmp edx,0x257; ja`）");
}

/// ① `queue_destroy_obstacle`（`0x3f20f0`）：**键就是实参 tick**，函数体内没有加法；
/// `handle_tick_event`（`0x3f1d30`）按**等值** tick 删整条。
///
/// 这条正是本轮从"我方自造 `now + 165`"改回原版语义的那一处：若还留着加法，
/// `queue(165)` 之后 `handle(165)` 什么也删不到。
#[test]
fn queue_and_fire_use_the_same_tick_key_without_any_delay() {
    let mut t = ObstacleTable::new();
    t.add_obstacle(0.0, 0.0, 1.0);
    t.queue_destroy_obstacle(165, 0.0, 0.0, 1.0);
    assert_eq!(t.pending_ticks(), vec![165], "键 = 实参 tick（0x3f20fe mov ebx,edx）");
    assert_eq!(t.pending_len(), 1);

    // 相邻 tick 不触发（等值删，不是 due <= now）
    assert_eq!(t.handle_tick_event(0, 164).unwrap().len(), 0);
    assert_eq!(t.pending_ticks(), vec![165]);
    let hit = t.handle_tick_event(0, 165).unwrap();
    assert_eq!(hit, vec![(0.0, 0.0)], "到点销毁，返回被销毁的 (x, y)");
    assert_eq!(t.pending_len(), 0, "整条被 remove_entry 拿走");
    // 判别值不是 TickEvent(0) ⇒ 原版 `0x3f1d40 jno` 走 panic；我方 Err
    assert!(t.handle_tick_event(1, 165).is_err());
}

/// ① 四个 `add/destroy/clear/queue` 各发一条事件，tag 与 `docs/02` 的事件表一致：
/// `AddObstacle` 40（`0x3f1876 add r13,0x29`）、`DestroyObstacle` 41（`0x3f1c09 add r13,0x2a`）、
/// `QueueDestroyObstacle` 42（`0x3f22b2 movabs 0x800000000000002a`）、`ClearObstacles` 39。
#[test]
fn obstacle_methods_emit_the_four_events_in_order() {
    let mut t = ObstacleTable::new();
    t.add_obstacle(0.0, 0.0, 1.0);
    t.queue_destroy_obstacle(7, 0.0, 0.0, 1.0);
    t.destroy_obstacle(0.0, 0.0, 1.0);
    t.clear_obstacles();
    assert_eq!(
        t.take_outbox(),
        vec![
            Event::AddObstacle,
            Event::QueueDestroyObstacle,
            Event::DestroyObstacle,
            Event::ClearObstacles
        ]
    );
    assert_eq!(Event::AddObstacle.wire_tag(), 40);
    assert_eq!(Event::DestroyObstacle.wire_tag(), 41);
    assert_eq!(Event::QueueDestroyObstacle.wire_tag(), 42);
    assert_eq!(Event::ClearObstacles.wire_tag(), 39);
}

/// ② 三个 `set_*` **各发一条**事件，tag 从立即数读死：
/// `set_function` `0x456377 movabs 0x7fff…ffff` + `0x456381 add rax,0x24` ⇒ `0x8000…0023` = **35**；
/// `set_function_active_tick` `0x456cd3 lea rax,[r9+0x25]` ⇒ **36**；
/// `set_function_points` `0x4569d3 lea rax,[r8+0x26]` ⇒ **37**。
///
/// （搬之前我方的 `outbox` 一条都不写 —— 这是本轮唯一改变可观察行为的地方，见 `docs/06` §99。）
#[test]
fn function_setters_emit_function_info_active_points() {
    let mut t = FunctionTable::new();
    assert_eq!(t.outbox_len(), 0);
    t.set_function(9003, vec![]);
    assert_eq!(t.take_outbox(), vec![Event::FunctionInfo], "set_function ⇒ tag 35");
    t.set_function_active_tick(9003, 100);
    assert_eq!(t.take_outbox(), vec![Event::FunctionActive], "set_function_active_tick ⇒ tag 36");
    t.set_function_points(9003, 1.0, 2.0);
    assert_eq!(t.take_outbox(), vec![Event::FunctionPoints], "set_function_points ⇒ tag 37");
    // tag 与 `Event::wire_tag()` 对得上（wire 侧 35/36/37）
    assert_eq!(Event::FunctionInfo.wire_tag(), 35);
    assert_eq!(Event::FunctionActive.wire_tag(), 36);
    assert_eq!(Event::FunctionPoints.wire_tag(), 37);
}

/// ② `get_active_functions_on_tick`（`0x456df0`）的在场判据：`tick > active_tick` **严格**，
/// 且 `30*(tick-active) < 1800`（本表不存整条曲线 ⇒ 窗口 60 tick，见函数文档的差异登记）。
#[test]
fn active_functions_predicate_is_strict_and_windowed() {
    let mut t = FunctionTable::new();
    t.set_function_active_tick(9003, 100);
    assert_eq!(t.get_active_functions_on_tick(100), Vec::<u32>::new(), "同 tick 不算在场（严格 >）");
    assert_eq!(t.get_active_functions_on_tick(101), vec![9003]);
    assert_eq!(t.get_active_functions_on_tick(160), vec![9003], "30*60 = 1800 ⇒ 仍 <= 1800");
    assert_eq!(t.get_active_functions_on_tick(161), Vec::<u32>::new(), "30*61 > 1800 ⇒ 不在场");
}

/// ③ `subscribe_to_event`（`0x3d38f0`）：只认那张 72 条名字表里的名字；`process_events`
/// （`0x3d3630`）**按名字查订阅表**，没人订阅的事件丢掉。
#[test]
fn event_handler_subscribes_by_name_and_drops_unsubscribed() {
    let mut h = GraphwarEventHandler::new();
    assert!(!h.subscribe_to_event("no_such_event"), "名字不在 0x3d38f0 的表里 ⇒ false");
    assert!(h.subscribe_to_event("tick_event"));
    assert!(h.subscribe_to_event("tick_event"), "同一个名字可以登记多次（原版是 Vec<闭包>）");
    assert_eq!(h.subscriber_count("tick_event"), 2);
    assert_eq!(h.subscriber_count("pos_info_event"), 0);

    let delivered = h.process_events(vec![Event::TickEvent, Event::PosInfo]);
    assert_eq!(delivered, vec![Event::TickEvent], "没人订阅的 PosInfo 被丢掉");
    assert_eq!(h.delivered().len(), 1);
    assert_eq!(h.pumps(), 1);
    // `TICKS_PER_SECOND` 只是把它带进编译，确认 tick handler 模块仍可依赖（无关断言）
    assert_eq!(TICKS_PER_SECOND, 60);
}
