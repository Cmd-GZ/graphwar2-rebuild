//! 一局对战的服务端对象：把已有的零件装起来。
//!
//! 组成：
//!
//! * 房间状态（`server::RoomState`）
//! * 关卡障碍（用于建掩膜）
//! * 士兵（致死判定）
//! * tick 基准（`tick::tick_at`）
//!
//! `Match::fire` 处理一次开火，产出**这条链路上要广播的消息**：
//!
//! ```text
//! FunctionPoints -> [LifeQueuedDeath -> EffectInfo(DeathEffect)]*
//!                -> [QueueDestroyObstacle] -> EffectInfo(ExplosionEffect)
//! ```
//!
//! 与 `settle::events` 的区别：这里直接产出**协议报文文本**（带 tick），
//! 可以直接写进 socket。

use crate::plane::{empty_mask, rasterize};
use crate::protocol::{emit_effect_info, emit_function_points, emit_life_queued_death};
use crate::server::RoomState;
use crate::settle::{resolve, Target};
use crate::tick::tick_at;

/// 一局对战的服务端状态。
pub struct Match {
    pub room: RoomState,
    pub obstacles: Vec<(f64, f64, f64)>,
    pub soldiers: Vec<Target>,
    /// tick 基准：`(基准 tick, 基准毫秒)`
    pub tick_base: (i64, i64),
    /// 障碍光栅化后的掩膜（决定弹道停点）
    mask: Vec<u8>,
    /// 已经用掉的效果 id（抓包里从 236 开始递增）
    next_effect_id: i64,
    /// 已排队但还没到期的 `DestroyObstacle`：`(到期 tick, 报文原文)`。
    ///
    /// 实测延迟 ≈165 tick（2.75 秒），见 `docs/44`。
    pending_destroy: Vec<(i64, String)>,
}

/// `QueueDestroyObstacle` 到 `DestroyObstacle` 的实测延迟（tick）。
///
/// 三次配对样本为 163.9 / 164.0 / 167.0 tick，取 165。
pub const DESTROY_DELAY_TICKS: i64 = 165;

impl Match {
    pub fn new(
        room: RoomState,
        obstacles: Vec<(f64, f64, f64)>,
        soldiers: Vec<Target>,
        tick_base: (i64, i64),
    ) -> Self {
        let mut mask = empty_mask();
        for (x, y, r) in &obstacles {
            rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        Match { room, obstacles, soldiers, tick_base, mask, next_effect_id: 236, pending_destroy: Vec::new() }
    }

    /// 当前时刻的 tick。
    pub fn tick_now(&self, now_ms: i64) -> i64 {
        tick_at(self.tick_base.0, self.tick_base.1, now_ms)
    }

    /// 推进到 `now_tick`，返回所有到期的 `DestroyObstacle`。
    pub fn tick(&mut self, now_tick: i64) -> Vec<String> {
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.pending_destroy.len() {
            if self.pending_destroy[i].0 <= now_tick {
                due.push(self.pending_destroy.remove(i).1);
            } else {
                i += 1;
            }
        }
        due
    }

    /// 处理一次开火，返回要广播的消息（顺序见模块注释）。
    ///
    /// `owner_id` 是开火实体；`function_text` 是**未修正**的函数原文
    /// （修正由 `resolve` 内部完成）。
    pub fn fire(
        &mut self,
        owner_id: i64,
        function_text: &str,
        x0: f64,
        y0: f64,
        flip: bool,
        now_ms: i64,
    ) -> Vec<String> {
        // 射手本人不参与致死判定
        let others: Vec<Target> = self
            .soldiers
            .iter()
            .filter(|t| {
                !(t.id == owner_id)
                    && !((t.x - x0).abs() < 1e-12 && (t.y - y0).abs() < 1e-12)
            })
            .copied()
            .collect();

        let outcome = resolve(function_text, x0, y0, 0.5, flip, crate::path::Mode::Normal, &self.mask, &others);
        let now_tick = self.tick_now(now_ms);

        // 报文顺序照真实抓包（docs/43）：
        //   CalculatingResults -> FunctionInfo(修正后) -> FunctionPoints
        //   -> SynchronizingResults -> [死者] -> [销毁] -> FunctionActive
        //   -> ExplosionEffect -> ExecutingResults
        let mut msgs = vec![
            crate::protocol::emit_game_state("CalculatingResults"),
            crate::protocol::emit_function_info(
                owner_id,
                function_text,
                Some(&crate::shot::corrected_text(function_text, x0, y0, flip)),
            ),
            emit_function_points(owner_id, x0, y0, outcome.stop.0),
            crate::protocol::emit_game_state("SynchronizingResults"),
        ];

        for id in &outcome.killed {
            msgs.push(emit_life_queued_death(*id, now_tick));
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                msgs.push(emit_effect_info(self.next_effect_id, t.x, t.y, "DeathEffect", now_tick));
            }
        }
        if !outcome.killed.is_empty() {
            self.soldiers.retain(|t| !outcome.killed.contains(&t.id));
        }

        if outcome.blocked {
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                outcome.stop.0, outcome.stop.1, now_tick
            ));
            // 真实游戏：在命中点用 fill=0xFF、半径 0.5 挖洞（docs/39）
            rasterize(&mut self.mask, outcome.stop.0, outcome.stop.1, 0.5, 0xFF);
            // 真正的删除要延后：实测 ≈165 tick 之后才发 DestroyObstacle
            self.pending_destroy.push((
                now_tick + DESTROY_DELAY_TICKS,
                format!(
                    r#"{{"DestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5}}}}"#,
                    outcome.stop.0, outcome.stop.1
                ),
            ));
        }

        msgs.push(crate::protocol::emit_function_active(owner_id, now_tick));
        self.next_effect_id += 1;
        msgs.push(emit_effect_info(self.next_effect_id, outcome.stop.0, outcome.stop.1, "ExplosionEffect", now_tick));
        msgs.push(crate::protocol::emit_game_state("ExecutingResults"));

        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> RoomState {
        RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    fn kind(msg: &str) -> &str {
        &msg[2..msg.find('"').unwrap_or(2)] // 简易取键名
    }

    fn kinds(msgs: &[String]) -> Vec<String> {
        msgs.iter()
            .map(|m| {
                let end = m[2..].find('"').unwrap() + 2;
                m[2..end].to_string()
            })
            .collect()
    }

    /// 报文顺序必须与真实抓包一致（docs/43）。
    #[test]
    fn fire_emits_captured_turn_sequence() {
        // 一个"被障碍挡住、没打死人"的回合（与抓包里那一回合同形）
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (5, 0));
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert_eq!(
            kinds(&msgs),
            vec![
                "GameStateInfo",
                "FunctionInfo",
                "FunctionPoints",
                "GameStateInfo",
                "QueueDestroyObstacle",
                "FunctionActive",
                "EffectInfo",
                "GameStateInfo",
            ],
            "回合内报文顺序与抓包不符"
        );
        // 状态名也要对
        assert!(msgs[0].contains("CalculatingResults"));
        assert!(msgs[3].contains("SynchronizingResults"));
        assert!(msgs[7].contains("ExecutingResults"));
    }

    #[test]
    fn killed_soldier_appears_between_sync_and_active() {
        let soldiers = vec![
            Target { id: 10, x: 5.0, y: 0.0, radius: 0.5 },
            Target { id: 11, x: -2.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (5, 0));
        let msgs = m.fire(10, "0.00", 5.0, 0.0, true, 1000);
        let ks = kinds(&msgs);
        let death = ks.iter().position(|k| k == "LifeQueuedDeath").expect("应有死亡消息");
        let active = ks.iter().position(|k| k == "FunctionActive").unwrap();
        assert!(death < active, "死亡消息应出现在 FunctionActive 之前");
        assert!(msgs.iter().any(|s| s.contains(r#""entity_id":11"#)), "应记录 11 号死亡");
    }

    #[test]
    fn tick_comes_from_the_clock() {
        let mut m = Match::new(room(), vec![], vec![], (5, 0));
        let msgs = m.fire(99, "0.00", 10.0, 0.0, true, 1000);
        // 基准 tick=5，1000ms 后 = 5 + 60 = 65
        assert!(msgs.iter().any(|s| s.contains(r#""start_tick":65"#)), "tick 应为 65: {msgs:?}");
    }

    #[test]
    fn blocked_shot_also_queues_obstacle_destruction() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 0);
        assert!(msgs.iter().any(|s| s.starts_with(r#"{"QueueDestroyObstacle""#)), "应排队销毁障碍");
    }

    #[test]
    fn destroy_is_delayed_by_measured_ticks() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        // 基准 (tick=0, ms=0)，fire 在 1000ms 处 -> 当时 tick = 60
        let msgs = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert!(msgs.iter().any(|s| s.starts_with(r#"{"QueueDestroyObstacle""#)));
        let queued_at = 60; // tick_at(0, 0, 1000)
        assert!(m.tick(queued_at).is_empty(), "刚排队时不该立刻删除");
        assert!(m.tick(queued_at + DESTROY_DELAY_TICKS - 1).is_empty(), "差一个 tick 时仍不该删除");
        let due = m.tick(queued_at + DESTROY_DELAY_TICKS);
        assert_eq!(due.len(), 1, "到期应恰好发出一条");
        assert!(due[0].starts_with(r#"{"DestroyObstacle""#));
        assert!(due[0].contains(r#""radius":0.5"#));
        assert!(m.tick(queued_at + DESTROY_DELAY_TICKS + 100).is_empty(), "不应重复发出");
    }

    #[test]
    fn blocked_shot_punches_a_hole_in_the_mask() {
        let mut m = Match::new(room(), vec![(-5.0, 0.0, 1.0)], vec![], (0, 0));
        let first = m.fire(1, "0.00", 10.0, 0.0, true, 0);
        let stop_x = first
            .iter()
            .find_map(|s| s.strip_prefix(r#"{"FunctionPoints":{"owner_id":1,"start_x":10,"start_y":0,"end_x":"#))
            .and_then(|r| r.split('}').next())
            .and_then(|v| v.parse::<f64>().ok())
            .expect("应能解析终点");
        // 同一发再打一次：洞口已经挖开，应飞得更远
        let second = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        let stop2 = second
            .iter()
            .find_map(|s| s.strip_prefix(r#"{"FunctionPoints":{"owner_id":1,"start_x":10,"start_y":0,"end_x":"#))
            .and_then(|r| r.split('}').next())
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap();
        assert!(stop2 < stop_x, "挖洞后应飞得更远（{stop2} 应小于 {stop_x}）");
    }

    #[test]
    fn killed_soldiers_leave_the_field() {
        // 第二个射手再打同一位置，不应重复记录同一个人
        let soldiers = vec![
            Target { id: 1, x: 10.0, y: 0.0, radius: 0.5 },
            Target { id: 2, x: 0.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        m.fire(1, "0.00", 10.0, 0.0, true, 0);
        assert_eq!(m.soldiers.len(), 1, "死者应被移出");
        let second = m.fire(1, "0.00", 10.0, 0.0, true, 1000);
        assert!(!second.iter().any(|s| s.contains("LifeQueuedDeath")), "不该重复计死");
    }

    #[test]
    fn shooter_never_kills_itself() {
        let soldiers = vec![Target { id: 7, x: 0.0, y: 0.0, radius: 0.5 }];
        let mut m = Match::new(room(), vec![], soldiers, (0, 0));
        let msgs = m.fire(7, "0.00", 0.0, 0.0, true, 0);
        assert!(!msgs.iter().any(|s| s.contains(r#"{"LifeQueuedDeath""#)), "射手不该判定自己死亡");
    }
}
