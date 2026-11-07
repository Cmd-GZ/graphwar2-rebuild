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
//! **2026-11-07 补**：arm64 侧的内联副本一共 **6 处**、分布在 **4 个**宿主函数里，
//! 逐个实参都解出来了（§6）——其中 3 处在 `add_soldier_to_player`。arm64 的 6 处
//! 靠「谁 `bl` 那个值类型是 `u8` 的 `HashMap::insert`」定死，比 x86 的分片扫描更硬。
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
//!   `0x4a8cbc`）。
//!
//! **`effect_id` 不是实体 id**（这一条 2026-11-07 复核时改正了旧稿）：`0x4a8cb8` 的
//! `stp w23, w24, [x29, #-0x48]` 写的是 `EffectInfo` 的 `+0x18`/`+0x1c`，即
//! `effect_id` 与 `start_tick` 两个**相邻**字段（布局见 `graphwar_effect_manager` §3），
//! `w23` 来自主循环的 `next_effect_id` 自增。replay 侧也直接否掉「= 实体 id」：
//! `1782640836` 一批里死者是 877/881/880，而三条死亡 `EffectInfo` 的
//! `effect_id` 是 **884/885/886**（连号），与爆炸的 887 共用同一个计数器。
//!
//! ⇒ 结论（**这是对 `docs/141` 那 19 例的解释**）：死亡块的 `effect_type`
//! **不是**由「怎么死的」决定的，而是由这个实体出生时那条
//! `EntityEffectInfo{entity_id, effect_type}` 决定的：
//!
//! | 实体登记过的类型 | 死亡时发出的 |
//! |---|---|
//! | `DeathEffect`(1) | `DeathEffect` |
//! | `SparkEffect`(2) | `SparkEffect` |
//! | `AshEffect`(3) | **`AshEffect`**（整块灰烬） |
//! | （没有登记） | **什么都不发** |
//!
//! ## 6. 谁在什么时候登记：三个登记点（**逐位**，2026-11-07 新增）
//!
//! 表全库只有 **6 个写入点**，靠"谁调 `HashMap::insert`"定死：
//! `HashMap<u32, EffectType>::insert` 的 monomorphization 是
//! `hashbrown::map::HashMap::insert::hd46922447724ee7`（arm64 **`0x466f14`**，
//! 值类型 `u8` ⇒ 桶距 8、和 `target_manager` 那个值类型是 `()` 的
//! `he15425207191a51e` 不同），扫 `.text` 的全部 `bl 0x466f14` 得 6 个调用点：
//!
//! | # | 所在函数 | 调用点 | 登记的 `effect_type` | 触发时机 |
//! |---|---|---|---|---|
//! | 1 | `GraphwarGameStateManager::add_soldier_to_player`（`0x4aaab4`） | `0x4ab0d0` | **1 `DeathEffect`**（皮肤名为空/取不到） | 士兵出生 |
//! | 2 | 同上 | `0x4ab154` | **1 `DeathEffect`**（皮肤名不以 `bot_` 开头，或长度 ≤ 3） | 士兵出生 |
//! | 3 | 同上 | `0x4ab2a4` | **3 `AshEffect`**（皮肤名以 `bot_` 开头） | 士兵出生 |
//! | 4 | `GraphwarGameStateManager::add_target`（`0x4a5704`） | `0x4a58d4` | **2 `SparkEffect`**（立即数 `mov w2, #0x2`） | 靶子出生 |
//! | 5 | `GraphwarGameStateManager::handle_graphics_info`（`0x4aa304`） | `0x4aa464` | **3 `AshEffect`**（循环，逐 id） | 收到 `SkinGraphicsInfo{bot_*}` |
//! | 6 | `GraphwarRemoteEventHandler::handle_server_remote_event`（`0x3e6df0`） | `0x3e8554` | 照抄收到的 `EntityEffectInfo` 载荷 | 收到远端的 `EntityEffectInfo` |
//!
//! `add_entity_effect` 的**独立函数体不存在**（arm64 上 `graphwar_entity_effect_manager.rs:57:26`
//! 那枚 `panic::Location` 没有任何函数引用它 ⇒ 全部内联）：6 个写入点各自把
//! 「写表 + 组 56 号事件 + `add_event`」整段展开，三次在 `add_soldier_to_player` 里、
//! 一次在 `add_target` 里、一次在 `handle_graphics_info` 里、一次在远端处理器里。
//!
//! ### 6.1 士兵：`bot_` 前缀决定 `AshEffect`（逐位）
//!
//! `add_soldier_to_player` 取的是**这个玩家的 `SkinGraphicsInfo.graphics_str`**
//! （`GraphicsManager` 里 `HashMap<u32, String>`，桶距 `0x20`，`0x4aac8c`–`0x4aad48`
//! 查表；命中就 `String::clone` 进 `Cow<str>` 槽 `[sp+0x20]`，未命中写
//! `0x8000000000000000` = `Cow::Borrowed("")`）。随后是**同一段判定**的两个入口：
//!
//! ```text
//! 0x4ab060: ldr  x20, [sp, #0x20]      ; Cow 的 capacity 字段（含 BORROWED_MASK）
//! 0x4ab068: cmp  x20, #0x8000000000000000
//! 0x4ab06c: b.ne 0x4ab0e4              ; 有皮肤名 → 走"长度 + 前缀"判定
//!           ⇒ 空皮肤：登记 DeathEffect（写点 1）
//!
//! 0x4ab0e4: ldp  x21, x8, [sp, #0x28]  ; x21 = ptr, x8 = len
//! 0x4ab0e8: cmp  x8, #0x3
//! 0x4ab0ec: b.hi 0x4ab218              ; len > 3 才去比前缀
//!           ⇒ len ≤ 3：登记 DeathEffect（写点 2）
//!
//! 0x4ab218: ldr  w9,  [x21]            ; 前 4 个字节
//! 0x4ab220: mov  w10, #0x6f62          ; 'b','o'
//! 0x4ab224: movk w10, #0x5f74, lsl #16 ; 't','_'
//! 0x4ab22c: cmp  w9, w10
//! 0x4ab234: b.eq 0x4ab24c              ; "bot_" ⇒ 登记 AshEffect（写点 3，立即数 3）
//!           ⇒ 不以 "bot_" 开头：登记 DeathEffect（写点 2）
//! ```
//!
//! 写点的立即数是**两条独立编码**（事件载荷字节 `strb w8,[sp,#0x54]` 与
//! `HashMap::insert` 的值 `mov w2, #0x1/#0x3`），同一分支里两者一致 ⇒ 不是看错。
//!
//! ### 6.2 靶子：`SparkEffect`（逐位）
//!
//! `add_target` 在 `0x4a589c` 写 `mov w8, #0x2`（载荷 `+0x0c`），
//! `0x4a58d0` 写 `mov w2, #0x2`（表值）。同一函数在 `0x4a583c`–`0x4a5858` 造出
//! `"target"` 六个字节并交给 `set_skin_graphics` ⇒ 靶子的皮肤名就是 `target`，
//! 与 `docs/16` §29 的「靶子」判定一致。
//!
//! ### 6.3 迟到的皮肤：`handle_graphics_info` 改写已登记的类型（逐位）
//!
//! `0x4aa324` 先闸门：事件的 tag 必须是 **51 `SkinGraphicsInfo`**
//! （`mov x20, #0x33; movk x20, #0x8000, lsl #48`），且载荷里的 `graphics_str`
//! **长度 ≥ 4 且前 4 字节 = `bot_`**（`0x4aa338`–`0x4aa358`，与 6.1 同一对立即数），
//! 否则整个函数什么都不做。命中后对**一批 entity id** 逐个
//! `add_entity_effect(id, 3)`（`0x4aa458` 起是循环体，表值 `mov w2, #0x3`）。
//!
//! ⇒ 一个玩家如果开局时还没选皮肤（此时登记 `DeathEffect`），等 `bot_0` 的
//! `SkinGraphicsInfo` 到了，他名下实体的登记会被**改写**成 `AshEffect`。
//! 回放侧逐条印证：`1782640836` 第 108 行 `SkinGraphicsInfo{881,"bot_0"}` 之后
//! 立刻是四条 `EntityEffectInfo{880|881,"AshEffect"}`（两趟，每趟两个 id），
//! 而这两人后来死时发的正是 `AshEffect`。
//!
//! ### 6.4 谁把它删掉（逐位）
//!
//! 只有 `entity_removed_event`（判别值 **14**，见 §3）会 `remove_entry` ⇒
//! **死亡本身不删表项**（不然 6.3 的改写、以及死亡时那次查表都没有意义）。
//!
//! ## 7. 未知
//!
//! * `handle_graphics_info` 里"一批 entity id"的来源只解到形状（一张
//!   `HashMap` 查表 + `SpecFromIterNested::from_iter`，`0x4aa398`–`0x4aa3e8`），
//!   没有逐条确认它是"该玩家名下的实体"还是"名下士兵"；两种读法在本仓库里
//!   **行为相同**（我们只有 `Match::soldiers`），所以不再往下追。
//! * 写点 6（远端处理器）的取舍策略没解：收到 `EntityEffectInfo` 是**无条件覆盖**
//!   还是先比较，没有逐指令核（不影响本地局，也不影响本模块的对外语义）。

use std::collections::HashMap;

use crate::events::Event;
use crate::graphwar_effect_manager::EffectType;

/// 两个订阅名（长度都是 20，见模块文档 §3）。
pub const SUBSCRIPTIONS: [&str; 2] = ["new_connection_event", "entity_removed_event"];

/// `new_connection_event` 的判别值没有在闭包里被断言（只 `entity_removed_event` 断言了 14）。
pub const ENTITY_REMOVED_TAG: u16 = Event::EntityRemoved.wire_tag();

/// `EntityEffectInfo` 的判别值（**56**）。
pub const ENTITY_EFFECT_INFO_TAG: u16 = Event::EntityEffectInfo.wire_tag();

/// 电脑玩家的皮肤名前缀（`add_soldier_to_player` 的 `0x4ab220`/`0x4ab224` 两个立即数
/// `0x6f62`/`0x5f74`，拼起来是 `"bot_"`；`handle_graphics_info` 的 `0x4aa348` 同一对）。
///
/// **这不是猜的**：`0x4ab22c` 的 `cmp w9, w10` 之后 `b.eq` 的分支写 `effect_type = 3`。
pub const BOT_SKIN_PREFIX: &str = "bot_";

/// 靶子的皮肤名（`add_target` 在 `0x4a583c`–`0x4a5858` 现场拼出来的那 6 个字节）。
pub const TARGET_SKIN: &str = "target";

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

    /// 士兵出生登记（`add_soldier_to_player`，§6.1）：皮肤名决定登记什么。
    ///
    /// 空皮肤（还没收到 `SkinGraphicsInfo`）⇒ `DeathEffect`（写点 1，`0x4ab09c`）；
    /// 以 `bot_` 开头 ⇒ `AshEffect`（写点 3，`0x4ab270`）；其余 ⇒ `DeathEffect`
    /// （写点 2，`0x4ab120`）。
    pub fn register_soldier(&mut self, entity_id: u32, skin: Option<&str>) {
        let ty = match skin {
            Some(s) if s.starts_with(BOT_SKIN_PREFIX) => EffectType::Ash,
            _ => EffectType::Death,
        };
        self.add_entity_effect(entity_id, ty);
    }

    /// 靶子出生登记（`add_target`，§6.2）：恒 `SparkEffect`（立即数 `mov w2, #0x2`）。
    pub fn register_target(&mut self, entity_id: u32) {
        self.add_entity_effect(entity_id, EffectType::Spark);
    }

    /// 迟到的皮肤信息（`handle_graphics_info`，§6.3）：**只有** `bot_` 前缀会改写登记，
    /// 其余名字原版什么都不做（`0x4aa358` 的 `b.ne` 直接跳出）。
    ///
    /// 返回是否改写了表。
    pub fn apply_skin_graphics(&mut self, entity_id: u32, graphics_str: &str) -> bool {
        if !graphics_str.starts_with(BOT_SKIN_PREFIX) {
            return false;
        }
        self.add_entity_effect(entity_id, EffectType::Ash);
        true
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
