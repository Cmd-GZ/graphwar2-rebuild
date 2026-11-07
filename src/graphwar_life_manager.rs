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

use crate::events::Event;

/// 一个士兵的存活状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Alive,
    Dead,
}

// ---------------------------------------------------------------------------
// 事件层（2026-11 任务 19 的接线层；纯逻辑在上面，**行为一行没改**）
// ---------------------------------------------------------------------------
//
// 证据一律 **x86 2026-08-28**（`.reference/steam/2026-08-28/libgraphwarrust.so`），
// 复算命令：`../.venv/bin/python tools/addr_probe.py dis <vaddr>`。
//
// **订阅 3 个**（`观测`）：`GraphwarLifeManager::new` = **`0x454790`**，体内 3 条
// `EventHandler::subscribe`（`0x3d38f0`，GOT 槽 `0x9bac88`），**按调用序**是
//
// ```text
// 0x4547f9  lea rsi,'new_connection_event'      len 0x14  handler 0x45eea0
// 0x45484e  lea rsi,'entity_removed_event'      len 0x14  handler 0x45eea0
// 0x45489f  lea rsi,'tick_event'                len 0x0a  handler 0x45eea0
// ```
//
// 三个处理器是**同一个闭包** `0x45eea0`（内部按判别值分派）——这也解释了
// `fixtures/name_to_addr.tsv` 里 3 条 `subscribe_to_events_event_handler`
// **都没有独立落点**：它们被合并了。
//
// **发出去的事件**（`观测`；tag 的写法是 `0x7fff_ffff_ffff_ffff + n`，
// 结果等于 `0x8000000000000000 | 判别值`，与 `events.rs` 的 `wire_tag()` 同一编码）：
//
// ```text
// set_alive           0x454b40  0x454c52  +0x1c → tag 0x…1b = 27 LifeInfo
//                               payload [+8]=soldier_id(u32) [+0xc]=alive(u8)
// send_all_info_events 0x454120 0x4542af  +0x1c → tag 27 LifeInfo（每实体一条）
// set_queued_death    0x453de0  0x453f6d  movabs 0x800000000000001c = 28 LifeQueuedDeath
// ```
//
// **`handle_tick_event`（`0x454020`）**：读 `event[+8]`（`TickEvent{tick}`，u32），
// 用 `0x47dec0` + `0x4863a0` 取出"到期"的条目，再逐个调 `set_alive`（`0x4540aa`）。
// ⚠️ **`set_alive` 的两个实参没有逐参解**（缺的证据）⇒ 下面只把到期项**取出来返回**，
// 不替它猜 alive 值。

/// 本模块订阅的 3 个事件名（长度立即数 0x14 / 0x14 / 0x0a，见上面的文档块）。
pub const SUBSCRIPTIONS: [&str; 3] = ["new_connection_event", "entity_removed_event", "tick_event"];

/// `new_connection_event` 的判别值（5）。
pub const NEW_CONNECTION_EVENT_TAG: u16 = Event::NewConnection.wire_tag();
/// `entity_removed_event` 的判别值（14，闭包里不符就 `panic!()`）。
pub const ENTITY_REMOVED_EVENT_TAG: u16 = Event::EntityRemoved.wire_tag();
/// `tick_event` 的判别值（0）。
pub const TICK_EVENT_TAG: u16 = Event::TickEvent.wire_tag();
/// 发出去的 `LifeInfo` 判别值（27）。
pub const LIFE_INFO_TAG: u16 = Event::LifeInfo.wire_tag();
/// 发出去的 `LifeQueuedDeath` 判别值（28）。
pub const LIFE_QUEUED_DEATH_TAG: u16 = Event::LifeQueuedDeath.wire_tag();

/// 生命状态表。形状来自观测，字段语义为推断。
///
/// 后两个字段是**接线层**（`AGENTS.md` 允许"接容器/接事件"那一层）：
/// 原版这两个东西不在 `LifeState` 里，而在 `new` 的实参里
/// （`Rc<RefCell<GraphwarEventQueue>>` + 那张订阅名表）。放进结构体只是为了
/// **容器里取回来的那一个对象自己能发事件**（原版同理：容器里的 manager 持有出口）。
#[derive(Debug, Default)]
pub struct LifeState {
    /// 第一张表：无条件写入。推断为"是否存活"。
    pub alive: HashMap<u32, Status>,
    /// 第二张表：仅 `mode == 1` 时写入。推断为"排队中的死亡"。
    pub queued_death: HashMap<u32, u32>,
    /// 发件箱（原版 `GraphwarEventQueue`；本 crate 用 `Vec` 承接，与别的 manager 同一口径）。
    pub outbox: Vec<Event>,
    /// 本模块订阅的事件名（`SUBSCRIPTIONS` 的副本；`new` 里写进去的那三个）。
    pub subscribed: Vec<&'static str>,
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

    // ---- 接线层：原版那几个入口的落地（都是对上面 `set` / `status` 的一层包装）----

    /// `new`（x86 `0x454790`）：空表 + 三个订阅名。
    pub fn new() -> Self {
        Self { subscribed: SUBSCRIPTIONS.to_vec(), ..Self::default() }
    }

    /// `set_alive`（x86 `0x454b40`）：写第一张表（`mode != 1` 时**不写**第二张），
    /// 并发一条 `LifeInfo`（判别值 27，`0x454c52`）。
    ///
    /// 原版签名是 `(soldier_id, alive: bool, mode)`，`mode == 1` 才动第二张表；
    /// 本入口固定走 `mode != 1` 那条路（`set(.., None)`），第二张表由
    /// [`LifeState::set_queued_death`] 负责——这是**我方拆分**，不是原版形状。
    pub fn set_alive(&mut self, soldier_id: u32, alive: bool) {
        self.set(soldier_id, if alive { Status::Alive } else { Status::Dead }, None);
        self.outbox.push(Event::LifeInfo);
    }

    /// `set_queued_death`（x86 `0x453de0`）：写两张表，并发 `LifeQueuedDeath`（判别值 28）。
    pub fn set_queued_death(&mut self, soldier_id: u32, death_tick: u32) {
        self.set(soldier_id, Status::Dead, Some(death_tick));
        self.outbox.push(Event::LifeQueuedDeath);
    }

    /// `get_alive`（x86 `0x454950`）返回的那个**状态字节**：活 `1` / 死 `0` / 查不到 `2`。
    ///
    /// 这与 `docs/98` §1.3 的观测一致（查不到时返回字面量 2），调用方只看最低位。
    pub fn get_alive(&self, soldier_id: u32) -> u8 {
        match self.status(soldier_id) {
            Some(Status::Alive) => 1,
            Some(Status::Dead) => 0,
            None => MISSING_STATE,
        }
    }

    /// `get_death_tick`（x86 `0x453a60`）：第二张表里的那个 tick。
    pub fn get_death_tick(&self, soldier_id: u32) -> Option<u32> {
        self.queued_death.get(&soldier_id).copied()
    }

    /// `get_all_entities`（x86 `0x453c60`）：第一张表的键（排序后返回，
    /// 因为原版的 `HashMap` 迭代序不可复现，排序只影响我方观察，不影响语义）。
    pub fn get_all_entities(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.alive.keys().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// `handle_entity_removed_event`（x86 `0x454420`）：判别值必须是 **14**，删两张表的项。
    pub fn handle_entity_removed_event(&mut self, tag: u16, entity_id: u32) -> Result<(), ()> {
        if tag != ENTITY_REMOVED_EVENT_TAG {
            return Err(());
        }
        self.alive.remove(&entity_id);
        self.queued_death.remove(&entity_id);
        Ok(())
    }

    /// `handle_tick_event`（x86 `0x454020`）：判别值必须是 **0**（`tick_event`），
    /// 取出并**移除**所有 `death_tick <= now_tick` 的排队项，按 id 升序返回。
    ///
    /// ⚠️ 原版随后逐个调 `set_alive`（`0x4540aa`），但**实参没有逐参解**；
    /// 本函数因此只做"取出来"，不改第一张表（缺的证据写在模块文档里）。
    pub fn handle_tick_event(&mut self, tag: u16, now_tick: u32) -> Result<Vec<(u32, u32)>, ()> {
        if tag != TICK_EVENT_TAG {
            return Err(());
        }
        let mut due: Vec<(u32, u32)> = self
            .queued_death
            .iter()
            .filter(|(_, &tick)| tick <= now_tick)
            .map(|(&id, &tick)| (id, tick))
            .collect();
        due.sort_unstable();
        for (id, _) in &due {
            self.queued_death.remove(id);
        }
        Ok(due)
    }

    /// `send_all_info_events`（x86 `0x454120`）：第一张表里**每个实体**发一条 `LifeInfo`，
    /// 返回 `(soldier_id, 状态字节)`（升序）。
    pub fn send_all_info_events(&mut self) -> Vec<(u32, u8)> {
        let mut out: Vec<(u32, u8)> =
            self.alive.iter().map(|(&id, _)| (id, self.get_alive(id))).collect();
        out.sort_unstable();
        for _ in 0..out.len() {
            self.outbox.push(Event::LifeInfo);
        }
        out
    }

    /// 取走发件箱（验收用；原版没有这个函数，事件是直接塞进 `GraphwarEventQueue` 的）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里现在有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
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

#[cfg(test)]
mod wiring_tests {
    use super::*;

    /// 订阅名与判别值的自校：名字表就是 `new` 里那三条 `subscribe` 的原文，
    /// 判别值必须与 `events.rs` 的 `wire_tag()` 同一个数（27 / 28 / 14 / 0 / 5）。
    #[test]
    fn subscriptions_and_tags_are_the_binary_ones() {
        assert_eq!(SUBSCRIPTIONS, ["new_connection_event", "entity_removed_event", "tick_event"]);
        assert_eq!(LIFE_INFO_TAG, 27);
        assert_eq!(LIFE_QUEUED_DEATH_TAG, 28);
        assert_eq!(ENTITY_REMOVED_EVENT_TAG, 14);
        assert_eq!(TICK_EVENT_TAG, 0);
        assert_eq!(NEW_CONNECTION_EVENT_TAG, 5);
        let life = LifeState::new();
        assert_eq!(life.subscribed, SUBSCRIPTIONS.to_vec());
        assert_eq!(life.lens(), (0, 0));
    }

    /// 一次最小交互：`set_alive` → `get_alive` 读回来；事件同时进发件箱。
    #[test]
    fn set_alive_then_get_alive_round_trips() {
        let mut life = LifeState::new();
        assert_eq!(life.get_alive(7), MISSING_STATE, "没记录 ⇒ 2（原版 0x454950 的返回值）");
        life.set_alive(7, true);
        assert_eq!(life.get_alive(7), 1, "活了就是第一张表里的 1");
        assert_eq!(life.get_all_entities(), vec![7]);
        assert_eq!(life.take_outbox(), vec![Event::LifeInfo], "set_alive 发一条 LifeInfo(27)");
        // 死：0 而不是"查不到"
        life.set_alive(7, false);
        assert_eq!(life.get_alive(7), 0);
        assert!(!is_alive_byte(life.get_alive(7)), "0 不能开火（test $0x1 为假）");
        assert_eq!(life.outbox_len(), 1);
    }

    /// `set_queued_death` 写两张表 + 发 `LifeQueuedDeath`(28)；
    /// `handle_tick_event` 只在 tick 到点后把它取出来。
    #[test]
    fn queued_death_is_taken_only_after_its_tick() {
        let mut life = LifeState::new();
        life.set_queued_death(3, 165);
        assert_eq!(life.get_alive(3), 0, "排队死亡已经把第一张表写成死");
        assert_eq!(life.get_death_tick(3), Some(165));
        assert_eq!(life.take_outbox(), vec![Event::LifeQueuedDeath]);
        // tick 还没到：什么都不取
        assert_eq!(life.handle_tick_event(TICK_EVENT_TAG, 164), Ok(vec![]));
        assert_eq!(life.get_death_tick(3), Some(165));
        // 到点：取出来并移除
        assert_eq!(life.handle_tick_event(TICK_EVENT_TAG, 165), Ok(vec![(3, 165)]));
        assert_eq!(life.get_death_tick(3), None);
        assert_eq!(life.handle_tick_event(TICK_EVENT_TAG, 165), Ok(vec![]), "取过一次就没了");
    }

    /// 判别值不对时必须报错（原版那三条闭包都带断言，`entity_removed_event` 里是 `panic!()`）。
    #[test]
    fn wrong_tags_are_rejected() {
        let mut life = LifeState::new();
        life.set_alive(9, true);
        assert_eq!(life.handle_tick_event(ENTITY_REMOVED_EVENT_TAG, 0), Err(()));
        assert_eq!(life.handle_entity_removed_event(TICK_EVENT_TAG, 9), Err(()));
        assert_eq!(life.get_alive(9), 1, "判别值不对时不能动表");
        assert_eq!(life.handle_entity_removed_event(ENTITY_REMOVED_EVENT_TAG, 9), Ok(()));
        assert_eq!(life.get_alive(9), MISSING_STATE);
    }

    /// `send_all_info_events`：每个实体一条 `LifeInfo`，返回值是真读表算出来的。
    #[test]
    fn send_all_info_events_broadcasts_one_per_entity() {
        let mut life = LifeState::new();
        life.set_alive(2, true);
        life.set_alive(1, false);
        let _ = life.take_outbox();
        assert_eq!(life.send_all_info_events(), vec![(1, 0), (2, 1)]);
        assert_eq!(life.outbox_len(), 2);
        assert_eq!(life.take_outbox(), vec![Event::LifeInfo, Event::LifeInfo]);
    }
}
