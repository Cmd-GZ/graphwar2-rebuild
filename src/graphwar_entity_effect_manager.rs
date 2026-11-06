//! 实体特效：`GraphwarEntityEffectManager`（**`entity_id → EffectType` 一张表**）。
//!
//! 本模块回答一件对逐位对拍**致命**的事：**死亡时到底发 `DeathEffect` 还是 `AshEffect`**。
//! 答案是：**这里不判定**——它只是把实体出生时登记过的特效存下来，
//! 死亡结算（`GraphwarGameStateManager::execute_results`）**照抄这张表**，
//! 查不到就**一条特效都不发**。详见 §5。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。arm64 侧地址来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`；x86 侧来自 `.eh_frame` FDE +
//! `panic::Location` 锚（两侧以**同一枚 `file:line:col`** 对名）。
//!
//! ## 1. 函数清单（3 个，全部已定位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x3e9f40` / 396 | `0x478490` / 352 | `:35:31` | 建表 + 订阅 2 个事件 | 逐位 |
//! | `subscribe` 闭包（新连接） | `0x3e9c58` / 744 | `0x4782f0` / 256 | `:67:30` | 遍历整表逐条广播 `EntityEffectInfo` | 逐位 |
//! | `subscribe` 闭包（实体移除） | `0x3e9ac4` / 404 | `0x4783f0` / 96 | `:77:18` | 断言判别值 14，删表项 | 逐位 |
//!
//! `new` / 两个闭包在 x86 上都是**同一枚 Location** 唯一命中（`:35:31` / `:67:30` /
//! `:77:18`），所以三个名字都不是猜的。两个闭包的尺寸比（x86 ÷ arm64 = 0.34 / 0.24）
//! **落在闸外**：x86 把「升级 Weak + 借 RefCell + 解引用」都内联进了别处，
//! 只有循环体留在原地——**尺寸闸对闭包不适用**，这里改用 Location 判等。
//!
//! 另有一枚锚 `graphwar_entity_effect_manager.rs:57:26`，在 arm64 上**没有任何函数引用它**
//! ⇒ 那个函数（`add_entity_effect`）在本构建里**被内联掉了**。它在 x86 有四份内联副本
//! （`0x4741a0` / `0x474a10` / `0x46f310` / `0x3e2830`），见 §5。
//!
//! ## 2. 数据结构（逐位）
//!
//! ```text
//! Rc<RefCell<GraphwarEntityEffectManager>>        ; 分配 0x60 = 0x18 头 + 0x48 值
//!   值 +0x18 : HashMap<u32 /*entity_id*/, EffectType /*u8*/>
//! ```
//!
//! 表项步长 **8**（键 `u32` 在 +0、值 `u8` 在 +4）：广播循环里
//! `ldur w9,[base-8]` 取键、`ldurb w8,[base-4]` 取值，且控制组按 **8 字节**推进
//! （`cmge v0.8b` + `sub x22, x22, #0x40`）⇒ 一次 8 个桶、桶距 8 字节。
//! 这同时把它与 `target_manager`（同一位置也是 `HashMap`，但**值类型是 `()`**、桶距 4）
//! 区分开：两个 `remove_entry` 的 monomorphization 不同
//! （`he15425207191a51e` vs `hd6fa74e75b817afe`）。
//!
//! ## 3. 订阅：两条（逐位）
//!
//! `new` 里两次 `Box::new(8)`（闭包环境只装一个 `Weak`）+ 两个 20 字节的事件名：
//!
//! | # | 事件名 | 名字长度立即数 | 判别值 | 处理方法 |
//! |---|---|---|---|---|
//! | 1 | `new_connection_event` | `0x14` = 20 | —（不看判别值） | 遍历整表重播 |
//! | 2 | `entity_removed_event` | `0x14` = 20 | **14**（不符就 `panic!()`） | 删表项 |
//!
//! ## 4. 广播：`EntityEffectInfo`（判别值 **56**，逐位）
//!
//! 循环体（arm64 `0x3e9cd0`–`0x3e9da0`）：
//!
//! ```text
//! x24 = 0x8000000000000038        ; = 56 = EntityEffectInfo
//! for each (entity_id, effect_type) in 表:
//!     event[+0x00] = 0x8000000000000038
//!     event[+0x08] = entity_id      ; u32（str w9, [sp, #0x20]）
//!     event[+0x0c] = effect_type    ; u8 （strb w8, [sp, #0x24]）
//!     event_queue.add_event(event)
//! ```
//!
//! 与 `target_manager` 的重播**同形**（同一套 hashbrown 迭代 + 队列 push），
//! 差别只在事件类型与载荷宽度。⇒ 新连接的客户端靠这两条重播拿到
//! 「谁是目标」「谁带什么特效」。
//!
//! ## 5. **`AshEffect` / `DeathEffect` 的分界：登记时决定，死亡时照抄**（逐位）
//!
//! `execute_results` 里那条死亡特效（arm64 `0x4a8cb0` 一带）在做三件事：
//!
//! ```text
//! let module = get_module_wk(...)?;                    ; 实体特效管理器
//! let ty = module.effects.get(&entity_id)?;            ; ← 查不到就整条 return（不发事件）
//! effect_manager.add_effect(entity_id, (pos_x, pos_y), ty, start_tick);
//! ```
//!
//! 逐条证据：
//!
//! * 查表：`hash_one(+0x50, &entity_id)`（`0x4a8ba8`）→ 桶比较
//!   `subs wzr, w23, w14`（键 = 被结算的实体 id，`0x4a8bec`）→ 命中后
//!   **`ldurb w21, [x13, #-0x4]` 把值（u8）读进 `w21`**（`0x4a8c50`）；
//! * 未命中分支（`0x4a8c38`）直接释放借用、跳过 `add_event`；
//! * `w21` 随后被原样写进 `EffectInfo` 的 `+0x20`（`sturb w21, [x29, #-0x40]`，
//!   `0x4a8cbc`），而 `effect_id` 用的是**实体 id 本身**（`stp w23, w24`，`0x4a8cb8`）。
//!
//! ⇒ 结论（**这是对 `docs/141` 那 19 例的解释**）：死亡块的 `effect_type`
//! **不是**由「怎么死的」决定的，而是由这个实体出生时那条
//! `EntityEffectInfo{entity_id, effect_type}` 决定的：
//!
//! | 实体登记过的类型 | 死亡时发出的 |
//! |---|---|
//! | `DeathEffect`(1) | `DeathEffect` |
//! | `AshEffect`(3) | **`AshEffect`**（整块灰烬） |
//! | （没有登记） | **什么都不发** |
//!
//! 所以「死亡块整块是 `AshEffect`」的成因是**出生登记错了/漏了**，不是死亡结算错了。
//! 关卡开场给靶子登记 `SparkEffect`（`docs/123` §2.1 的回放四条），
//! 士兵登记什么**本轮没有逐个核**（见 [`ENTITY_EFFECT_DEFAULT`] 与 §6）。
//!
//! ## 6. 未知
//!
//! * `add_entity_effect` 的**独立函数体**（arm64 内联、x86 四份副本）没有逐指令解，
//!   只知它往表里写 `(entity_id, effect_type)` 并广播 `EntityEffectInfo`（判别值 56）；
//! * 士兵出生时登记哪种类型：**未知**。回放里 397 份的死亡块分布可以反推，
//!   但那是**数据侧**统计，不是代码证据；本文不给默认值。

use std::collections::HashMap;

use crate::events::Event;
use crate::graphwar_effect_manager::EffectType;

/// 两个订阅名（长度都是 20，见模块文档 §3）。
pub const SUBSCRIPTIONS: [&str; 2] = ["new_connection_event", "entity_removed_event"];

/// `new_connection_event` 的判别值没有在闭包里被断言（只 `entity_removed_event` 断言了 14）。
pub const ENTITY_REMOVED_TAG: u16 = Event::EntityRemoved.wire_tag();

/// `EntityEffectInfo` 的判别值（**56**）。
pub const ENTITY_EFFECT_INFO_TAG: u16 = Event::EntityEffectInfo.wire_tag();

/// 一条 `EntityEffectInfo` 的载荷（事件 `+0x08` = `entity_id` u32，`+0x0c` = `effect_type` u8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityEffectInfo {
    pub entity_id: u32,
    pub effect_type: EffectType,
}

impl EntityEffectInfo {
    /// 事件判别值（56）。
    pub const TAG: u16 = ENTITY_EFFECT_INFO_TAG;
}

/// 实体特效管理器：一张 `HashMap<u32, EffectType>`。
#[derive(Debug, Default)]
pub struct GraphwarEntityEffectManager {
    effects: HashMap<u32, EffectType>,
    pub outbox: Vec<Event>,
    pub subscribed: Vec<&'static str>,
}

impl GraphwarEntityEffectManager {
    /// `new`（arm64 `0x3e9f40` / x86 `0x478490`，Location `:35:31`）。
    pub fn new() -> Self {
        Self {
            effects: HashMap::new(),
            outbox: Vec::new(),
            subscribed: SUBSCRIPTIONS.to_vec(),
        }
    }

    /// 登记一个实体的特效（原版 `add_entity_effect`，arm64 内联；
    /// x86 的内联副本在 `0x4741a0` / `0x474a10` / `0x46f310`，Location `:57:26`）。
    ///
    /// 语义：写表 + 广播 `EntityEffectInfo`（判别值 56）。
    pub fn add_entity_effect(&mut self, entity_id: u32, effect_type: EffectType) {
        self.effects.insert(entity_id, effect_type);
        self.outbox.push(Event::EntityEffectInfo);
    }

    /// 查表：**死亡结算就是查这一下**（§5）。
    pub fn get(&self, entity_id: u32) -> Option<EffectType> {
        self.effects.get(&entity_id).copied()
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// `entity_removed_event` 的处理（arm64 `0x3e9ac4` / x86 `0x4783f0`，`:77:18`）。
    ///
    /// 判别值必须是 14，否则原版 `panic!()`（`explicit panic`，`0x19be09`）。
    pub fn handle_entity_removed_event(&mut self, tag: u16, entity_id: u32) -> Result<(), ()> {
        if tag != ENTITY_REMOVED_TAG {
            return Err(());
        }
        self.effects.remove(&entity_id);
        Ok(())
    }

    /// `new_connection_event` 的处理（arm64 `0x3e9c58` / x86 `0x4782f0`，`:67:30`）：
    /// 遍历整表，逐条把 `EntityEffectInfo` 推出去。
    ///
    /// 返回推出去的载荷（顺序 = `HashMap` 迭代序，**不可复现**，原版同样如此）。
    pub fn send_all_info_events(&mut self) -> Vec<EntityEffectInfo> {
        let mut out = Vec::with_capacity(self.effects.len());
        for (&entity_id, &effect_type) in self.effects.iter() {
            self.outbox.push(Event::EntityEffectInfo);
            out.push(EntityEffectInfo {
                entity_id,
                effect_type,
            });
        }
        out
    }

    /// **死亡结算的投影**：`execute_results` 对某个死者的判定（§5）。
    ///
    /// `None` = 原版在这里**一条特效都不发**。这不是我们的选择，是 `0x4a8c38`
    /// 分支的字面行为。**没有默认值**：`unwrap_or(DeathEffect)` 是错的。
    pub fn death_effect_of(&self, entity_id: u32) -> Option<EffectType> {
        self.get(entity_id)
    }
}
