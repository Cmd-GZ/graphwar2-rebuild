//! 生命状态的最小表示。
//!
//! 证据（`docs/10`）：`graphwar_life_manager.rs` 的每个入口都在 `RwLock` 内
//! 操作**两张表**，其中第二张**仅当 `mode == 1`** 时写入：
//!
//! ```text
//! lock();
//!   map_a.insert(id, v1);              // 无条件
//!   if mode == 1 { map_b.insert(id, v2); }
//! unlock();
//! ```
//!
//! **这里的字段语义是推断**：`alive` / `queued_death` 这两个名字来自
//! "第二张表 + LifeQueuedDeath 事件存在"的合理对应，**没有**参数级证据。
//! 之所以还是写出来，是因为"两张表 + 一个 mode 分支"这个**形状**是观测，
//! 把它写成代码比写成散文更不易走形。

use std::collections::HashMap;

/// 一个士兵的存活状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Alive,
    Dead,
}

/// 生命状态表。形状来自观测，字段语义为推断。
#[derive(Debug, Default)]
pub struct LifeState {
    /// 第一张表：无条件写入。推断为"是否存活"。
    pub alive: HashMap<u32, Status>,
    /// 第二张表：仅 `mode == 1` 时写入。推断为"排队中的死亡"。
    pub queued_death: HashMap<u32, u32>,
}

impl LifeState {
    /// 对应伪代码里的 `mode == 1` 分支：写第一张表，并（可能）写第二张。
    ///
    /// `queue_tick` 为 `Some` 时写第二张表——这是把"仅 mode==1 才写"
    /// 这条观测直接编码进类型。
    pub fn set(&mut self, soldier_id: u32, status: Status, queue_tick: Option<u32>) {
        self.alive.insert(soldier_id, status);
        if let Some(tick) = queue_tick {
            self.queued_death.insert(soldier_id, tick);
        }
    }

    /// 第一张表里没有记录时按"未知"处理，不默认成存活或死亡。
    pub fn status(&self, soldier_id: u32) -> Option<Status> {
        self.alive.get(&soldier_id).copied()
    }

    /// 取出并移除一条排队死亡（对应"处理排队的死亡"这一动作）。
    pub fn take_queued_death(&mut self, soldier_id: u32) -> Option<u32> {
        self.queued_death.remove(&soldier_id)
    }

    /// 两张表的规模，便于外部观察状态。
    pub fn lens(&self) -> (usize, usize) {
        (self.alive.len(), self.queued_death.len())
    }
}

// ---------------------------------------------------------------------------
// 状态字节（`docs/98` §1.3 的观测落成代码）
// ---------------------------------------------------------------------------
//
// `0x454950`（`graphwar_life_manager.rs`）是 "HashMap<i32, 8 字节值> 查找"，
// 返回**值里的那个 byte**；**查不到时返回 2**（反编译里 `uVar15 = CONCAT71(...,2)`）。
// 调用方 `0x463da0` 只看最低位：`test $0x1,%al`（`0x463fbb`）。
// ⇒ `1`（活）能开火；`0`（死）与 `2`（查不到 / 未上场）都不能。

/// `life_manager` 查不到该士兵时返回的状态字节（`docs/98` §1.3）。
pub const MISSING_STATE: u8 = 2;

/// "能开火吗"：只有最低位为 1 才算活（`0x454950` 的返回值 → `0x463fbb` 的 `& 1`）。
///
/// 任意奇数都算活——这是二进制里 `test $0x1` 的实际语义，不是"只有 1"。
pub const fn is_alive_byte(state: u8) -> bool {
    state & 1 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn life_table_round_trip() {
        let mut life = LifeState::default();
        assert_eq!(life.status(1), None, "没记录就是未知，不默认成任何一边");
        life.set(1, Status::Alive, None);
        assert_eq!(life.status(1), Some(Status::Alive));
        assert_eq!(life.lens(), (1, 0));
        // mode == 1 的那条路：同时写第二张表
        life.set(2, Status::Dead, Some(77));
        assert_eq!(life.take_queued_death(2), Some(77));
        assert_eq!(life.take_queued_death(2), None);
    }

    /// `docs/98` §1.3：`& 1` 为真才算活，查不到（2）与死（0）同属"不可开火"。
    #[test]
    fn only_the_low_bit_means_alive() {
        assert_eq!(MISSING_STATE, 2, "反编译里查不到时返回的字面量就是 2");
        assert!(!is_alive_byte(0));
        assert!(is_alive_byte(1));
        assert!(!is_alive_byte(2));
        assert!(is_alive_byte(3));
        assert!(!is_alive_byte(0xFE));
        assert!(is_alive_byte(0xFF));
    }
}
