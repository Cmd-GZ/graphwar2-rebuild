//! 靶子（目标）：`GraphwarTargetManager` —— **只是一张 `HashSet<u32>`**，
//! 外加「新连接重播」与「实体移除时删号」两件事。
//!
//! 本模块回答两个问题：**`target_manager` 里有没有「部分命中 ⇒ 全部复原」的分支**
//! （**没有**，§3），以及**那条规则到底在哪儿**（在 `GameStateManager::queue_end_turn_trigger`
//! 的闭包里，§6 —— 本轮新定位的判定点，`docs/123` §3.2 三条候选全部出局）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。arm64 侧地址来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`；x86 侧来自 `.eh_frame` FDE +
//! `panic::Location` 锚（两侧靠**同一枚 `file:line:col`** 对名）。
//!
//! ## 1. 函数清单
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x3dae34` / 396 | `0x465910` / 352 | `:35:24` | 建集合 + 订阅 2 个事件 | 逐位 |
//! | 闭包（新连接） | `0x3da9bc` / 740 | `0x465780` / 240 | `:74:30` | 遍历集合逐条广播 `TargetInfo` | 逐位 |
//! | 闭包（实体移除） | `0x3daca0` / 404 | `0x465870` / 96 | `:84:18` | 断言判别值 14，删号 | 逐位 |
//! | `add_target` | **内联**（无符号） | 内联进 `0x46f310`（`:65:26`） | `:65:26` | 登记一个靶子 id | 观测 |
//!
//! x86 的 4 枚锚与 arm64 的 4 枚锚**逐枚对上**（`:35:24` `:65:26` `:74:30` `:84:18`）。
//! ⚠️ **`docs/123` §2.2 把 `0x46f310` 记成「造一个目标」的函数，它其实是关卡开场里
//! 「生一个靶子实体」的整段代码**：`graphwar_target_manager.rs:65`、
//! `graphwar_entity_effect_manager.rs:57`、`graphwar_game_state_manager.rs` 三枚
//! `panic::Location` 同时出现在它体内 ⇒ `add_target` / `add_entity_effect` 在那里是
//! **内联副本**。所以「13 个调用点全部落在关卡开场」的结论不变，但它的身份不是
//! 「target_manager 的成员函数」，而是**调用方**。
//!
//! ## 2. 数据结构：`HashSet<u32>`（逐位）
//!
//! ```text
//! Rc<RefCell<GraphwarTargetManager>>     ; 分配 0x60 = 0x18 头 + 0x48 值
//!   值 +0x08 : Rc<RefCell<GraphwarEventQueue>>      ; 广播出口
//!   值 +0x18 : HashSet<u32>                          ; RawTable 0x20 + RandomState 0x10
//! ```
//!
//! 三条独立证据说明「值类型是 `()`」：
//!
//! 1. 广播循环取键用 `ldur w8,[base-0x4]`、桶距 **4**（`and x8,x8,#0x3c`）；
//! 2. `remove_entry` 的 monomorphization 是 `hd6fa74e75b817afe`，与
//!    `entity_effect_manager` 那条（`he15425207191a51e`，值 `u8`、桶距 8）**不是同一个**；
//! 3. 管理器值总长 0x48 = 8(队列) + 8(?) + 0x30(表) ⇒ 表后面**没有值**要放。
//!
//! ## 3. 为什么「部分命中 ⇒ 全部复原」**不可能**在这个模块里
//!
//! 一个模块要能复原靶子，至少要**知道靶子此刻是死是活**。本模块的表只有 id：
//!
//! * 没有 `alive` 字段（桶距 4 = 只有 `u32` 键）；
//! * 三个可归属函数里没有任何一处写 `LifeInfo`（判别值 27）——
//!   全库构造 `LifeInfo` 的位置**只有两处**，都在 `graphwar_life_manager`
//!   （`send_all_info_events` `0x497240`、`set_alive` `0x497cb0`）；
//! * 唯一的写操作是 `entity_removed_event` 里的 `remove(&entity_id)`。
//!
//! ⇒ **`docs/123` §2.5 的判断（「不在目标管理器里」）在指令级成立**，而且现在知道
//! 它连数据都没有。复原只能表现为 `life_manager.set_alive(id, true)`，见 §6。
//!
//! ## 4. 订阅：两条（逐位）
//!
//! | # | 事件名 | 长度立即数 | 判别值 | 处理方法 |
//! |---|---|---|---|---|
//! | 1 | `new_connection_event` | `0x14` = 20 | 不看 | 遍历集合重播 `TargetInfo` |
//! | 2 | `entity_removed_event` | `0x14` = 20 | **14**（不符 `panic!()`） | 删号 |
//!
//! 两个闭包都在 `Weak::upgrade` → `RefCell::borrow_mut` 之后干活（`docs/02` §13.1
//! 记的 `mode == 1` 那套计数是 `RcInner` 的 strong/weak 计数，不是游戏状态）。
//!
//! ## 5. 广播：`TargetInfo`（判别值 **23**，逐位）
//!
//! ```text
//! for id in 集合(迭代序) :
//!     event[+0x00] = 0x8000000000000017   ; 23 = TargetInfo
//!     event[+0x08] = id                    ; u32
//!     event_queue.add_event(event)         ; 元素 0x48 字节
//! ```
//!
//! 与 `docs/123` §2.3 对 `0x465780` 的逐指令分析一致（hashbrown 迭代器，
//! `pmovmskb`/`tzcnt` 那套），本轮只是把它的 x86 地址与 arm64 名字对上了。
//!
//! ## 6. **「打不全就全部复原」的判定点（本轮新增，观测）**
//!
//! 反查「谁会写 `LifeInfo{alive:true}`」：`set_alive`（arm64 `0x497bb0`）的**全部**
//! 调用点是 5 个，其中唯一带条件的是
//! `GraphwarGameStateManager::queue_end_turn_trigger::{{closure}}`（arm64 `0x4abc00`，
//! 0x58c 字节，x86 侧对应 `0x472470` 一带的内联体）。闭包的关键路径：
//!
//! ```text
//! turn_count += 1 ; add_event(TurnCountInfo{58, turn_count})            ; 0x4abc20-0x4abc5c
//! w23 = if turn_count < self[+0xcc] { 0 } else { self[+0xc8] }          ; 0x4abc60-0x4abc78
//! (t1, t2) = gsm.get_alive_teams()                                      ; 0x4abc7c
//! if  (w23 & 1) == 0  &&  t1  &&  t2  &&  self[+0x11d] == 1:            ; 0x4abc80-0x4abc94
//!     targets = get_module_wk(...)          ; ← 桶距 4 ⇒ 就是本模块那张 HashSet
//!     life    = get_module_wk(...)
//!     for id in targets { life.set_alive(id, /**alive=*/true, 0) }      ; 0x4abea8-0x4abef8
//! ```
//!
//! ⇒ **规则的形状**：回合结束、**两队都还有人活着**（还没分出胜负）、
//! 且该关开了 `self[+0x11d] == 1`（**推断**：就是 5 个「一発で全部」关卡的标志位）
//! 时，把**靶子集合里的每一个 id 重新置为 alive**（各自发 `LifeInfo{alive:true}`）。
//! 与 `docs/123` §0 的①③④⑤完全吻合：翻 alive + 广播 + 不重建实体 + 不恢复障碍洞。
//!
//! **仍未定的三处**（都给到了地址，下一步可直接挖）：
//!
//! * `self[+0x11d]`（u8）：只知「必须 == 1 才复原」，是哪个关卡字段**未知**；
//! * `self[+0xc8]` / `self[+0xcc]`（两个 i32）与 `(w23 & 1) == 0`：**未知**
//!   （`+0xcc` 参与「`turn_count < +0xcc`」的比较，像回合上限；`+0xc8` 在超限后
//!   提供 w23，但为何要求它偶数没有解释）；
//! * `set_alive` 的第三个参数（`w3 = 0`）：**未知**（第二参数是 `alive`）。

use std::collections::HashSet;

use crate::events::Event;

/// 两条订阅名（长度都是 20）。
pub const SUBSCRIPTIONS: [&str; 2] = ["new_connection_event", "entity_removed_event"];

/// `entity_removed_event` 的判别值 = **14**（不符就 `panic!()`）。
pub const ENTITY_REMOVED_TAG: u16 = Event::EntityRemoved.wire_tag();

/// `new_connection_event` 的判别值 = **5**（`docs/07` §7.2 的表；闭包里没断言它）。
pub const NEW_CONNECTION_EVENT_TAG: u16 = Event::NewConnection.wire_tag();

/// `TargetInfo` 的判别值 = **23**。
pub const TARGET_INFO_TAG: u16 = Event::TargetInfo.wire_tag();

/// 目标管理器：一张 `HashSet<u32>`（**没有 alive 字段**，见模块文档 §2/§3）。
#[derive(Debug, Default)]
pub struct GraphwarTargetManager {
    targets: HashSet<u32>,
    pub outbox: Vec<Event>,
    pub subscribed: Vec<&'static str>,
}

impl GraphwarTargetManager {
    /// `new`（arm64 `0x3dae34` / x86 `0x465910`，Location `:35:24`）。
    pub fn new() -> Self {
        Self {
            targets: HashSet::new(),
            outbox: Vec::new(),
            subscribed: SUBSCRIPTIONS.to_vec(),
        }
    }

    /// 登记一个靶子（原版 `add_target`，arm64 内联；x86 内联副本在 `0x46f310`，
    /// Location `:65:26`）。语义：把 id 塞进集合。
    pub fn add_target(&mut self, target_id: u32) {
        self.targets.insert(target_id);
    }

    /// 集合内容（迭代序不可复现，原版同样如此）。
    pub fn targets(&self) -> impl Iterator<Item = u32> + '_ {
        self.targets.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn contains(&self, target_id: u32) -> bool {
        self.targets.contains(&target_id)
    }

    /// `new_connection_event` 的处理（arm64 `0x3da9bc` / x86 `0x465780`，`:74:30`）：
    /// 遍历集合，每个 id 发一条 `TargetInfo`。
    pub fn send_all_info_events(&mut self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.targets.len());
        for &id in self.targets.iter() {
            self.outbox.push(Event::TargetInfo);
            out.push(id);
        }
        out
    }

    /// `entity_removed_event` 的处理（arm64 `0x3daca0` / x86 `0x465870`，`:84:18`）。
    ///
    /// 判别值必须是 14，否则原版走 `panic!()`（串 `0x19be09` = `explicit panic`，长度 14）。
    pub fn handle_entity_removed_event(&mut self, tag: u16, entity_id: u32) -> Result<(), ()> {
        if tag != ENTITY_REMOVED_TAG {
            return Err(());
        }
        self.targets.remove(&entity_id);
        Ok(())
    }

    // ---- 接线层（2026-11 任务 19）：两条订阅各自的入口 + 发件箱 ----

    /// `new_connection_event` 的入口：先按判别值 **5** 过闸，再走
    /// [`GraphwarTargetManager::send_all_info_events`]。
    ///
    /// 闭包本身**没有**在 `new_connection_event` 上做判别值断言（只有
    /// `entity_removed_event` 那条断言了 14，见模块文档 §4），这里按同一形态补一道闸：
    /// 名字→判别值的对应来自 `docs/07` §7 的表，**不是我方发明的**。
    pub fn handle_new_connection_event(&mut self, tag: u16) -> Result<Vec<u32>, ()> {
        if tag != NEW_CONNECTION_EVENT_TAG {
            return Err(());
        }
        Ok(self.send_all_info_events())
    }

    /// 取走发件箱（验收用；原版直接把事件塞进 `GraphwarEventQueue`）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }
}

/// 「打不全就全部复原」的**判定条件**（模块文档 §6，**观测**）。
///
/// 原版不是函数而是一段内联在 `queue_end_turn_trigger` 闭包里的判断；
/// 这里把它抽成纯函数，好让测试把三个条件钉住。
///
/// **运行时背书（2026-11-09，`docs/06` §34b）**：用户现场打了两把多靶子关，逐 tick 原文显示
/// 「部分命中 ⇒ 回合末整组靶子广播 `alive:true`（**死的那一个和活着的那一个都重播**）」，
/// 而「全清 ⇒ 没有任何 `true`，直接 `EndingGame`」。
/// ⇒ 本条不再只是"读指令推出来的"，`reset_mode_flag` 的方向也与"一发全清模式"一致。
///
/// * `turn_clamped` = 闭包里那个 `w23`（`turn_count < +0xcc ? 0 : +0xc8`）；
/// * `team1_alive` / `team2_alive` = `get_alive_teams()` 的两个 bool；
/// * `reset_mode_flag` = `self[+0x11d]`（**推断**：一発で全部关卡的标志位）。
pub fn should_reset_targets(
    turn_clamped: i32,
    team1_alive: bool,
    team2_alive: bool,
    reset_mode_flag: u8,
) -> bool {
    (turn_clamped & 1) == 0 && team1_alive && team2_alive && reset_mode_flag == 1
}
