//! 瞬时特效的**登记 + 超期回收**：`GraphwarEffectManager`。
//!
//! 本模块回答三件事：**特效是怎么被登记的**（注册表 + 广播）、**一条特效活多久**、
//! **`EffectType` 的四个判别值**。**画**不在本模块（那是 `effect_drawer.rs`，
//! `docs/16` §21）；**死亡时发哪个特效也**不在本模块（见 §5 与
//! `docs/16-client.md` §32 的结论）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测**（能指到地址或字节）/ **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。arm64 侧地址来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`（源码级函数名**就是符号名**），
//! x86 侧地址来自 `.eh_frame` FDE 函数边界 + `panic::Location` 锚。
//!
//! ## 1. 函数清单（5 个，全部已定位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 职责 | 等级 |
//! |---|---|---|---|---|
//! | `new` | `0x40bc0c` / 456 | 无独立函数（内联进宿主） | 建表 + 订阅 `tick_event` | 观测 |
//! | `add_effect` | `0x40bb20` / 192 | **无独立函数**：内联进 `0x472470` / `0x476a20`（`:87:26`） | 登记一条特效 + 广播 `EffectInfo` | 观测 |
//! | `tick` | `0x40bdd4` / 424 | `0x519d80` / 448（`:103:105`、`:104:41`） | 扫表、删掉超期的记录 | 逐位 |
//! | `subscribe` 闭包 | `0x40bbe0` / 44 | — | `WeakCaller::call_0` → [`GraphwarEffectManager::tick`] | 观测 |
//! | `EffectType::deserialize` | `0x40b834` / 748 | 无锚 | serde 变体名 → 判别值 | 逐位 |
//!
//! `add_effect` 与 `tick` 的归属靠**同一枚 `panic::Location`** 定死：
//! arm64 `0x40bb20` 引用 `graphwar_effect_manager.rs:87:26`，x86 侧同一枚
//! `:87:26`（`.data.rel.ro 0x962940`）只被 `0x472470` / `0x476a20` 引用 ⇒
//! 那两处是**内联副本**，x86 上没有独立的 `add_effect`。
//! `tick` 的两枚锚（`:103:105` / `:104:41`）在两侧都落在同一个函数上。
//!
//! ## 2. 数据结构（逐位）
//!
//! ```text
//! Rc<RefCell<GraphwarEffectManager>>              ; 分配 0x60 = 0x18 头 + 0x48 值
//!   值 +0x08 : Rc<RefCell<GraphwarEventQueue>>     ; 广播出口（add_event 的 x0 = +0x18）
//!   值 +0x10 : Weak<RefCell<GraphwarModuleContainer>>   ; tick 里取值用
//!   值 +0x18 : HashMap<u32 /*effect_id*/, EffectData>   ; RawTable 0x20 + RandomState 0x10
//! EffectData = { pos_x: f64, pos_y: f64, start_tick: u32, effect_type: u8 }  ; 0x18 字节
//! ```
//!
//! 表项步长 **0x20**：键 `u32` 在 +0、值在 +8（`HashMap::insert` 的
//! `x2 = 值指针` 与 `tick` 里 `asr #5` 的步长两处独立印证）。
//!
//! ## 3. `add_effect`：登记 + 广播（逐位）
//!
//! 形参顺序（ARM64 寄存器实测）：`add_effect(&self, effect_id: u32 /*w1*/,
//! pos: &(f64, f64) /*x2*/, effect_type: u8 /*w3*/, start_tick: u32 /*w4*/)`。
//!
//! 1. `HashMap::insert(&mut self.effects, effect_id, EffectData{…})`（`0x40bb64`）；
//! 2. 组装 `GraphwarEvent`（72 字节，`0x40bb80`–`0x40bb90`）：
//!    `+0x00` = `0x800000000000002C` = **判别值 44 = `EffectInfo`**，
//!    `+0x08/+0x10` = `pos_x/pos_y`（f64），`+0x18` = `effect_id`，
//!    `+0x1c` = `start_tick`，`+0x20` = `effect_type`（u8）；
//! 3. `GraphwarEventQueue::add_event(&queue, event)`（`0x3d7a40`）。
//!
//! 载荷偏移与 `graphwar_audio_control::effect_info_event`（客户端按 `+0x20` 的 u8
//! 选音效，`docs/16` §30）**逐字节吻合** ⇒ 这 72 字节的布局是双向确认的。
//!
//! ## 4. `tick`：**3600 tick** 后回收（逐位）
//!
//! ```text
//! now = get_module_wk(self + 0x10)?.borrow()[+0x58] as u32     ; 当前 tick
//! for (effect_id, data) in effects:
//!     if data.start_tick + 3600 <= now {  删除该项（ctrl 置 EMPTY，items -= 1） }
//! ```
//!
//! * `+3600` 是立即数 `add w0, w0, #0xe10`（`0x40bea4`）；
//! * 被比较的字段是**值里的 `start_tick`**（`ldur w0, [base+0x18]`，键在 `base+0`）——
//!   容易看错成键，`0x40bea0` 那条 `ldur` 的位移是 `值基址 + 0x18`，见 §2 的表项布局；
//! * 删除只动表和 `items` 计数，**不发任何事件**（`tick` 里没有 `add_event`）。
//!
//! 3600 tick ÷ 60 Hz = **60 秒**。这是**登记记录**的寿命（迟到的客户端仍能查表），
//! **不是**绘制时长：绘制固定 60 tick（`effect_drawer.rs`，`docs/16` §21）。
//!
//! ## 5. 死亡时发哪个特效：**不在这里**（观测）
//!
//! `add_effect` 的 `effect_type` 是**入参**，本模块不做任何判定。`EffectInfo` 的
//! 生产点全库只有三处（`movk #0x8000,lsl #48` 的 44 立即数扫描）：
//!
//! | 生产点 | 内容 | 等级 |
//! |---|---|---|
//! | `add_effect` 自己（`0x40bb84`） | 由调用方给类型 | 逐位 |
//! | `GraphwarGameStateManager::execute_results`（**内联** `add_effect`）× 2 | 见下 | 逐位 |
//! | `effect_drawer` / `audio_control` | 只**消费**，不生产 | 观测 |
//!
//! `execute_results` 的两处（arm64 `0x4a89ec`、`0x4a8cb0`）分别给：
//!
//! * **`effect_type = 0`（`ExplosionEffect`），`effect_id` = 一个自增计数器**
//!   （`ldxr/stxr` 原子加一）⇒ 弹道停止/命中的爆炸；
//! * **`effect_type = entity_effect_map[entity_id]`，`effect_id = entity_id`**
//!   ⇒ 死亡特效**照抄这个实体登记过的实体特效**（`GraphwarEntityEffectManager` 的
//!   `HashMap<u32, EffectType>`，键步长 8 的查表在 `0x4a8b8c`–`0x4a8c50`）。
//!   **查不到就一条都不发**（`0x4a8c38` 分支直接跳过 `add_event`）。
//!
//! ⇒ **`AshEffect` 与 `DeathEffect` 的分界不在死亡那一刻**，而在实体出生时登记的
//! `EntityEffectInfo` 上。详见 `graphwar_entity_effect_manager.rs` §5。
//!
//! ## 6. `EffectType` 的四个判别值（逐位）
//!
//! `.rodata 0x19caf8` 起是**首尾相接**的四个变体名 + `tick_event`：
//!
//! ```text
//! 0x19caf8 ExplosionEffect (15)     ← 0
//! 0x19cb07 DeathEffect     (11)     ← 1
//! 0x19cb12 SparkEffect     (11)     ← 2
//! 0x19cb1d AshEffect       ( 9)     ← 3
//! 0x19cb26 tick_event      (10)     ← 订阅名，正好接在 AshEffect 后面
//! ```
//!
//! `deserialize`（`0x40b834`）只做一件事：①跳过空白；②按字节集合闸
//! （`(1<<c) & 0x1_00002600`）确认首字符属于 `{E,D,S,A}`；③对整串做 `memcmp`
//! 式比较，命中后写判别值。名串与判别值的**顺序一致**，且客户端侧
//! `graphwar_audio_control` 用 `0/1/2` 分别选 `explosion/squash/spark.mp3`，
//! 两条独立证据互补 ⇒ 0..3 就是枚举声明顺序。
//!
//! ## 7. 未知
//!
//! * `new` 的两个不透明字段（值 `+0x00` 那 8 字节）没解出来；
//! * `tick` 取的模块（`get_module_wk::h4ae2b3f89e2f8a81`）只知**读 `+0x58` 当 tick**，
//!   具体是哪个模块**推断**为 `GraphwarTickHandler`（同一 monomorphization 被
//!   `game_recorder` / `turn_manager` / `function_manager` / `gsm` 共用，全是需要
//!   「当前 tick」的地方），`module_container` 按 `TypeId` 取，常量不在符号里。

use std::collections::HashMap;

use crate::events::Event;

/// `EffectType` 的变体名（**声明顺序 = 判别值**，见模块文档 §6）。
///
/// 判别值 `0..=3` 与 `.rodata 0x19caf8` 的串顺序一致，也与客户端音效分支
/// （`graphwar_audio_control::effect_info_event`）一致。
pub const EFFECT_TYPE_VARIANTS: [&str; 4] = [
    "ExplosionEffect",
    "DeathEffect",
    "SparkEffect",
    "AshEffect",
];

/// 一条瞬时特效的类型（`docs/07` §5 的 `enum(str)`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EffectType {
    /// 弹道停止 / 命中处的爆炸。`execute_results` 里用自增 id 的那条。
    Explosion = 0,
    /// 士兵死亡。**只有在实体登记过 `DeathEffect` 时才会出现**（见 §5）。
    Death = 1,
    /// 火花（靶子等实体的默认效果）。
    Spark = 2,
    /// 灰烬（`docs/141` 里 19 例「死亡块整块是 AshEffect」的来源）。
    Ash = 3,
}

impl EffectType {
    /// 判别值（`u8`，与 `.rodata` 变体名顺序一致）。
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// 由判别值反查；`>= 4` 返回 `None`（原版 `deserialize` 到这一步就报错了）。
    pub const fn from_code(code: u8) -> Option<EffectType> {
        match code {
            0 => Some(EffectType::Explosion),
            1 => Some(EffectType::Death),
            2 => Some(EffectType::Spark),
            3 => Some(EffectType::Ash),
            _ => None,
        }
    }

    /// `EffectType` 在 serde 里的名字（`"ExplosionEffect"` 等）。
    pub const fn name(self) -> &'static str {
        EFFECT_TYPE_VARIANTS[self as usize]
    }
}

/// 特效记录的寿命：`3600` tick（`add w0, w0, #0xe10`，模块文档 §4）。
pub const EFFECT_LIFETIME_TICKS: u32 = 3600;

/// 一条特效的登记内容（`HashMap` 的值，`0x18` 字节；键是 `effect_id`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectData {
    pub pos_x: f64,
    pub pos_y: f64,
    /// `EffectInfo.start_tick`。`tick` 用它判超期。
    pub start_tick: u32,
    pub effect_type: EffectType,
}

/// `EffectInfo`（判别值 **44**）的载荷，字段顺序与 72 字节事件一致。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectInfo {
    pub pos_x: f64,
    pub pos_y: f64,
    pub effect_id: u32,
    pub start_tick: u32,
    pub effect_type: EffectType,
}

impl EffectInfo {
    /// 事件的判别值（与 `crate::events::Event::EffectInfo` 同一数字，测试会互校）。
    pub const TAG: u16 = Event::EffectInfo.wire_tag();
}

/// 瞬时特效管理器。
///
/// 原版是 `Rc<RefCell<GraphwarEffectManager>>`（arm64 分配 `0x60` 字节），
/// 这里只保留**语义**：一张表 + 一个广播出口。
#[derive(Debug, Default)]
pub struct GraphwarEffectManager {
    effects: HashMap<u32, EffectData>,
    /// 广播出去的事件（对应原版的 `GraphwarEventQueue`；本 crate 用 `Vec` 承接）。
    pub outbox: Vec<Event>,
    /// 本模块订阅的事件名（只有一条）。
    pub subscribed: Vec<&'static str>,
}

impl GraphwarEffectManager {
    /// 订阅名：`tick_event`（长度立即数 `0xa`，`0x19cb26`）。
    pub const SUBSCRIPTION: &'static str = "tick_event";

    /// `new`（arm64 `0x40bc0c`）：建空表、记下订阅名。
    ///
    /// 原版还接两个引用进来：`Rc<RefCell<GraphwarEventQueue>>`（值 `+0x08`）与
    /// `Weak<RefCell<GraphwarModuleContainer>>`（值 `+0x10`）。这里由调用方直接
    /// 传 `outbox` 语义，故构造函数无参。
    pub fn new() -> Self {
        Self {
            effects: HashMap::new(),
            outbox: Vec::new(),
            subscribed: vec![Self::SUBSCRIPTION],
        }
    }

    /// 登记一条特效并广播 `EffectInfo`（arm64 `0x40bb20`，模块文档 §3）。
    ///
    /// 形参顺序照 ARM64 寄存器：`effect_id` → `pos` → `effect_type` → `start_tick`。
    pub fn add_effect(
        &mut self,
        effect_id: u32,
        pos: (f64, f64),
        effect_type: EffectType,
        start_tick: u32,
    ) {
        self.effects.insert(
            effect_id,
            EffectData {
                pos_x: pos.0,
                pos_y: pos.1,
                start_tick,
                effect_type,
            },
        );
        self.outbox.push(Event::EffectInfo);
    }

    /// 最近一次广播的 `EffectInfo` 载荷（测试与上层装配用；原版没有这个函数）。
    pub fn last_effect_info(&self, effect_id: u32) -> Option<EffectInfo> {
        let d = self.effects.get(&effect_id)?;
        Some(EffectInfo {
            pos_x: d.pos_x,
            pos_y: d.pos_y,
            effect_id,
            start_tick: d.start_tick,
            effect_type: d.effect_type,
        })
    }

    /// 查表（`tick` 与测试用）。
    pub fn get(&self, effect_id: u32) -> Option<&EffectData> {
        self.effects.get(&effect_id)
    }

    /// 表长（原版是 `RawTable.items`）。
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// 扫表、删掉 `start_tick + 3600 <= now` 的记录（arm64 `0x40bdd4`，§4）。
    ///
    /// **不发事件**，只回收。返回被删掉的 `effect_id`（按 `HashMap` 迭代序，
    /// 原版同样是 `HashMap` 迭代序 ⇒ 不可复现，别写成排序）。
    pub fn tick(&mut self, now: u32) -> Vec<u32> {
        let mut dead: Vec<u32> = Vec::new();
        for (&id, d) in self.effects.iter() {
            // 原版是 u32 加法（`add w0, w0, #0xe10`）⇒ 用 wrapping，避免调试构建 panic。
            if d.start_tick.wrapping_add(EFFECT_LIFETIME_TICKS) <= now {
                dead.push(id);
            }
        }
        for id in &dead {
            self.effects.remove(id);
        }
        dead
    }
}
