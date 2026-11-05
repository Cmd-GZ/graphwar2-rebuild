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
}

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
        Match { room, obstacles, soldiers, tick_base, mask, next_effect_id: 236 }
    }

    /// 当前时刻的 tick。
    pub fn tick_now(&self, now_ms: i64) -> i64 {
        tick_at(self.tick_base.0, self.tick_base.1, now_ms)
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

        let mut msgs = vec![emit_function_points(
            owner_id,
            x0,
            y0,
            outcome.stop.0,
        )];

        for id in &outcome.killed {
            // 死亡 tick：这里按"当前 tick"下发（弹丸飞行时间模型未还原，见 docs/31）
            msgs.push(emit_life_queued_death(*id, now_tick));
            if let Some(t) = self.soldiers.iter().find(|t| t.id == *id) {
                self.next_effect_id += 1;
                msgs.push(emit_effect_info(
                    self.next_effect_id,
                    t.x,
                    t.y,
                    "DeathEffect",
                    now_tick,
                ));
            }
        }

        // 死掉的士兵从场上移除：否则后续射击会重复计死
        if !outcome.killed.is_empty() {
            self.soldiers.retain(|t| !outcome.killed.contains(&t.id));
        }

        if outcome.blocked {
            msgs.push(format!(
                r#"{{"QueueDestroyObstacle":{{"pos_x":{},"pos_y":{},"radius":0.5,"tick":{}}}}}"#,
                outcome.stop.0, outcome.stop.1, now_tick
            ));
        }

        self.next_effect_id += 1;
        msgs.push(emit_effect_info(
            self.next_effect_id,
            outcome.stop.0,
            outcome.stop.1,
            "ExplosionEffect",
            now_tick,
        ));

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

    #[test]
    fn fire_emits_function_points_first_and_explosion_last() {
        let soldiers = vec![
            Target { id: 10, x: 5.0, y: 0.0, radius: 0.5 },
            Target { id: 11, x: -2.0, y: 0.0, radius: 0.5 },
        ];
        let mut m = Match::new(room(), vec![], soldiers, (5, 0));
        let msgs = m.fire(10, "0.00", 5.0, 0.0, true, 1000);
        assert!(msgs.first().unwrap().starts_with(r#"{"FunctionPoints""#));
        assert!(msgs.last().unwrap().contains("ExplosionEffect"));
        // 途中穿过了 11 号士兵
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
