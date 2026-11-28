//! `GraphwarGameStateManager`（296 B）的**事件层**：7 条订阅 + 发件箱 + 处理器的落点。
//!
//! 本文件回答三件事：**这个 manager 订阅什么、每个处理器体内发什么、我方哪几个处理器真的存在**。
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **x86 ELF vaddr**（Ghidra = vaddr + `0x100000`；arm64 另给）。
//!
//! ## 0. 边界：`game.rs` 与这里怎么分工
//!
//! 原版这一个模块（`graphwar_game_state_manager.rs`）在我们的复原里被切成**两半**，
//! 切法是按**职责**、不是按证据缺口：
//!
//! | 哪一半 | 在哪 | 内容 |
//! |---|---|---|
//! | 运行实体（状态 + 结算） | [`crate::game`]（`Match`） | 房间/士兵/靶子/队伍/回合/生命/函数槽 + `fire_functions` / `execute_results` / `begin_next_turn` … |
//! | **事件层（订阅 + 处理器）** | **本文件**（`impl Match`） | `SUBSCRIBED_EVENTS[7]` + [`GsmEvents`]（`subscribed` / `outbox` / `delivered`）+ 3 个已实现的处理器 |
//!
//! ⇒ `Match` 的**类型定义仍在 `crate::game`**（容器里装的还是 `Match`，22 槽的
//! `get_module::<Match>` 判据一个字没动），本文件只放**方法实现**与事件层。这也是
//! `dispatch/86` §2.4 说的"唯一正路"：不加壳、不另起旁表。
//!
//! ## 1. 订阅表（7 条，**观测**；本轮逐条复跑过，与 `docs/161` §2.4 数字一致）
//!
//! `GraphwarGameStateManager::new`（**`0x477bc0`**，992 字节）体内 7 条
//! `call [0x9bac88]`（= `subscribe_to_event`，`0x3d38f0`），名字长度与 `docs/07` §7.2 对上：
//!
//! | # | 事件（长度） | 判别值 | 订阅点 | 处理器 | 处理器体内 `add_event` |
//! |---|---|---|---|---|---|
//! | 1 | `tick_event`（10） | 0 | `0x477c92`（`edx=0xa`） | `0x470840` | **1**（tag 30 `GameStateInfo`） |
//! | 2 | `new_connection_event`（20） | 5 | `0x477ce8`（`edx=0x14`） | `0x477140` | **4**（含 1 处 tag 30；另 3 处载荷形状**未解**） |
//! | 3 | `entity_removed_event`（20） | 14 | `0x477d3e`（`edx=0x14`） | `0x476a20` | **1**（tag 44 `EffectInfo`） |
//! | 4 | `connection_removed_event`（24） | 6 | `0x477d94`（`edx=0x18`） | `0x477540` | **2**（其中一条路径调 `execute_results` `0x472470`） |
//! | 5 | `function_points_event`（21） | 37 | `0x477dea`（`edx=0x15`） | `0x4758b0` | **1**（转发：`docs/161` §2 记"只做转发"） |
//! | 6 | `skin_graphics_info_event`（24） | 51 | `0x477e40`（`edx=0x18`） | `0x4741a0` | **1**（tag 56 `EntityEffectInfo`，载荷 `{id, 3}`=Ash） |
//! | 7 | `chat_message_event`（18） | 71 | `0x477e96`（`edx=0x12`） | `0x473d00` | **0** |
//!
//! 处理器名取自 arm64 未 strip 的 `.symtab`（`fixtures/sym_match.tsv` 的 "arm64 名 → x86 地址" 列：
//! `handle_tick` 0x4a6ad4→`0x470840`、`handle_new_connection_event` 0x4ad3b4→`0x477140`、
//! `handle_entity_removed_event` 0x4acd44→`0x476a20`、`handle_connection_removed_event`
//! 0x4ad808→`0x477540`、`handle_function_points` 0x4abad4→`0x4758b0`、
//! `handle_graphics_info` 0x4aa304→`0x4741a0`、`handle_chat_message` 0x4a9de0→`0x473d00`），
//! 等级 **观测**（名字的符号来源 + 地址的跨 ISA 匹配，不是逐字节对拍）。
//!
//! 合计 **10** 处 `add_event`（= 1+4+1+2+1+1+0）。这十处的分布是本轮自己复跑的
//! （`addr_probe.py dis <f> | grep -c "slot 0x9bac80"`，7 条命令），与 `dispatch/86`
//! 报的 10 处逐条相同 ⇒ **订阅方向 7/7**、**发布方向的清单 10/10**。
//!
//! 三个 tag 是逐指令读出来的（本文件据此写发件箱）：
//!
//! ```text
//! 0x470a49  add r13, 0x1e      ; r13 = 0x8000000000000000 ⇒ tag word = …001e = 30 GameStateInfo
//! 0x470a44  mov byte [rsp+0x28], 1   ; 载荷 +8 = 1（= Setup，docs/14 §11.3 第 ③ 段）
//! 0x476bbe  lea rax, [r13 + 0x2d]    ; r13 = 0x7fffffffffffffff ⇒ …002c = 44 EffectInfo
//! 0x4742b9  add r13, 0x39            ; ⇒ …0038 = 56 EntityEffectInfo（载荷 {id, 3=AshEffect}）
//! ```
//!
//! 判别值名字见 [`crate::events::Event`]（tag 30 / 44 / 56 与枚举一一对应）。
//!
//! ## 2. 本轮搬了什么（**任务 95**；行为零变化，逐个搬、逐个跑三轴）
//!
//! | 处理器 | 原地址 | 原来在 `src/game.rs` | 现在 |
//! |---|---|---|---|
//! | `handle_tick` | `0x470840` / 816 B | `Match::handle_tick` | [`Match::handle_tick`]（本文件） |
//! | `handle_graphics_info` | arm64 `0x4aa304` / x86 处理器 `0x4741a0` | `Match::note_skin_graphics` | [`Match::note_skin_graphics`]（本文件） |
//! | `entity_removed` 的处理 | `0x476a20`（判定值 14，`0x3e9ac4`） | `Match::handle_entity_removed` | [`Match::handle_entity_removed`]（本文件） |
//!
//! 搬过来的函数体**一字未改**（只多了一行"记下这条事件到了哪个处理器"，见 §3），
//! 三轴（`fire` / `batch` / `batch_conv`）在每次搬移后各跑一遍，数字一位没动。
//!
//! ⚠️ **原地址到不了的两件事照实记**：
//!
//! * `handle_tick` 的三段职责我们只落地了"到期 `DestroyObstacle` 走一遍"
//!   （第 ① 段事件转发没有总线、第 ② 段自动开火只落成纯函数
//!   [`crate::turnstate::should_auto_fire`]、第 ③ 段时间上限回 `Setup` 未落地 ——
//!   见 `Match::handle_tick` 的文档与 `docs/14` §11.3）；
//! * `entity_removed` / `skin_graphics_info` 两个处理器的**发布方向没接**：原版体内各 1 处
//!   `add_event`（44 `EffectInfo` / 56 `EntityEffectInfo`），我方这两个入口只写
//!   `GraphwarEntityEffectManager` 的登记表、**一条报文都不发**（广播点不在这些函数里）。
//!   ⇒ 发件箱在这两条上是 **0**，`docs/161` §2.4 的缺口照旧成立。
//!
//! ## 3. 事件层的落地（`subscribed` / `outbox` / `delivered`）
//!
//! * [`GsmEvents::subscribed`]：`new` 里那 7 条订阅的落地（顺序 = 调用序，`0x477c92` →
//!   `0x477e96`）。这是**订阅方向**在代码里的对应物；
//! * [`GsmEvents::delivered`]：**入方向**。处理器被调用时记一条"到了哪个处理器"——
//!   原版是事件队列把事件派进来（`call [vtable+0x20]`），我们没有总线，于是用
//!   "调用方 → 处理器"这条路径本身当证据；
//! * [`GsmEvents::outbox`]：**发方向**。只装**我方真的发出去**的报文类别
//!   （判别值，同 `GraphwarRankManager` / `GraphwarColorManager` 的口径：载荷由业务
//!   方法返回，发件箱只记"发了哪一类"）。当前唯一会写的是 `handle_tick` 里到期的
//!   `DestroyObstacle`(41)。
//!
//! ⚠️ **为什么不把 10 处 `add_event` 都写进 `outbox`**：那样等于把"原版有这处调用"
//! 直接写成"我方发了这条报文"，而 4 个处理器在我方**根本没有实现**（见 §4）。
//! `dispatch/86` §2.4 已经否掉过"另起旁表"这条路 —— 发件箱只记真发出去的。
//!
//! ## 4. 4 个处理器**搬不了**：不是耦合，是**本仓库没有实现**（本轮结论）
//!
//! `dispatch/95` 的前提是"`src/game.rs` 里现有 7 处实现"。复核后**只有 3 处**
//! （`grep -cE "fn .*handle" src/game.rs` = 2 + `note_skin_graphics` 1 处，见上表）。
//! 另外 4 个处理器在 `src/` 里**一行都没有**，它们不是"写在别处的同一个逻辑"：
//!
//! | 处理器 | 原版体内做什么（反汇编直读） | 我方为什么不能"搬" |
//! |---|---|---|
//! | `new_connection` `0x477140` | 先比事件 tag（`cmp [rsi], 0x8000000000000005`）、按 `[self+0x120]==1` / `[self+0x11e]` 两个状态位分两路，每路 1 条 `add_event`；再 1 次 `get_module_wk`（`0x46d8c0`，`0x47720f`）与 2 条 `add_event`（其中一条 **tag 30 `GameStateInfo{1}`**，另一条在 `GET_MODULE` 之后；这两条的载荷形状**未解**） | 这段是**整表重播**：我方没有"把当前局面重播给新连接"的入口，也没有那两个状态位 ⇒ 要写就是**新行为**（`dispatch/95` 红线：不许顺手加） |
//! | `connection_removed` `0x477540` | 删连接相关表项 + **调 `execute_results`（`0x472470`）** + 2 条 `add_event` | 我方 `Match` 没有"连接 → 实体"的映射（连接是 `_scaffold` 的概念），也没有 `connection_removed` 的入口 |
//! | `function_points` `0x4758b0` | 只转发（体内 1 条 `add_event`，`docs/161` §2 已记"无调用，锚 1129/1135"） | `game.rs:1096` 的横幅早已登记：我方**不存提交**（收集阶段在 `graphwar_game_server`），这一条没有落点 |
//! | `chat_message` `0x473d00` | 体内 **0** 条 `add_event`（纯转发给订阅者） | 聊天链在 `graphwar_setup_control.rs` 的节点类（`chat_message` 信号；T63 已从 `_scaffold` 搬回），不在本模块 |
//!
//! ⇒ 这一格**没有升到 `wired`**：订阅方向 7/7 已落地、发方向只有 1 类报文落地，
//! 而 4/7 个处理器缺的是**行为**（要新写，不是搬移）。缺什么写在 `docs/161` §2.4。
//!
//! ## 5. 复跑（每条都能原地跑）
//!
//! ```bash
//! PY=../.venv/bin/python
//! $PY tools/addr_probe.py --so ../.reference/game/libgraphwarrust.so dis 0x477bc0 | grep -c "slot 0x9bac88"  # 7
//! for a in 0x470840 0x477140 0x476a20 0x477540 0x4758b0 0x4741a0 0x473d00; do
//!   echo -n "$a "; $PY tools/addr_probe.py --so ../.reference/game/libgraphwarrust.so dis $a | grep -c "slot 0x9bac80"
//! done   # 1 4 1 2 1 1 0  ⇒ 合计 10
//! ```

use crate::events::Event;
use crate::game::Match;
use crate::graphwar_mode_manager::{AxisMode, FunctionMode, GraphwarModeManager, TimeMode};
use crate::graphwar_player_manager::GraphwarPlayerManager;
use crate::graphwar_soldier_manager::SoldierTable;
use crate::turnstate::GameState;

/// `tick_event` 的判别值（订阅 1；也是 [`Event::TickEvent`]）。
pub const EVENT_TICK: u16 = 0;
/// `new_connection_event` 的判别值（订阅 2）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `connection_removed_event` 的判别值（订阅 4）。
pub const EVENT_CONNECTION_REMOVED: u16 = 6;
/// `entity_removed_event` 的判别值（订阅 3）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;
/// `function_points_event` 的判别值（订阅 5）。
pub const EVENT_FUNCTION_POINTS: u16 = 37;
/// `skin_graphics_info_event` 的判别值（订阅 6）。
pub const EVENT_SKIN_GRAPHICS_INFO: u16 = 51;
/// `chat_message_event` 的判别值（订阅 7）。
pub const EVENT_CHAT_MESSAGE: u16 = 71;

/// 一条订阅（`new` 里一次 `subscribe_to_event`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription {
    /// 事件名（`.rodata` 原文，长度已与 `docs/07` §7.2 对上）。
    pub name: &'static str,
    /// 判别值（`0x3d38f0` 的分派结果）。
    pub tag: u16,
    /// 原版处理器名（arm64 `.symtab` 原文；x86 侧无名）。
    pub handler: &'static str,
    /// 订阅点：`0x477bc0` 体内那条 `call [0x9bac88]`。
    pub site: u64,
    /// 处理器入口（x86 vaddr）。
    pub entry: u64,
    /// 处理器**体内**（不含被调函数）的 `add_event` 处数。
    pub add_event: usize,
    /// 我方是否真的实现了这个处理器（`false` 见模块文档 §4）。
    pub landed: bool,
}

/// `GraphwarGameStateManager::new` 的 7 条订阅（**观测**，见模块文档 §1）。
pub const SUBSCRIBED_EVENTS: [Subscription; 7] = [
    Subscription {
        name: "tick_event",
        tag: EVENT_TICK,
        handler: "GraphwarGameStateManager::handle_tick",
        site: 0x477c92,
        entry: 0x470840,
        add_event: 1,
        landed: true,
    },
    Subscription {
        name: "new_connection_event",
        tag: EVENT_NEW_CONNECTION,
        handler: "GraphwarGameStateManager::handle_new_connection_event",
        site: 0x477ce8,
        entry: 0x477140,
        add_event: 4,
        landed: false,
    },
    Subscription {
        name: "entity_removed_event",
        tag: EVENT_ENTITY_REMOVED,
        handler: "GraphwarGameStateManager::handle_entity_removed_event",
        site: 0x477d3e,
        entry: 0x476a20,
        add_event: 1,
        landed: true,
    },
    Subscription {
        name: "connection_removed_event",
        tag: EVENT_CONNECTION_REMOVED,
        handler: "GraphwarGameStateManager::handle_connection_removed_event",
        site: 0x477d94,
        entry: 0x477540,
        add_event: 2,
        landed: false,
    },
    Subscription {
        name: "function_points_event",
        tag: EVENT_FUNCTION_POINTS,
        handler: "GraphwarGameStateManager::handle_function_points",
        site: 0x477dea,
        entry: 0x4758b0,
        add_event: 1,
        landed: false,
    },
    Subscription {
        name: "skin_graphics_info_event",
        tag: EVENT_SKIN_GRAPHICS_INFO,
        handler: "GraphwarGameStateManager::handle_graphics_info",
        site: 0x477e40,
        entry: 0x4741a0,
        add_event: 1,
        landed: true,
    },
    Subscription {
        name: "chat_message_event",
        tag: EVENT_CHAT_MESSAGE,
        handler: "GraphwarGameStateManager::handle_chat_message",
        site: 0x477e96,
        entry: 0x473d00,
        add_event: 0,
        landed: false,
    },
];

/// 事件层：订阅表 + 发件箱 + 入件记录。
///
/// 原版这三样在 `GraphwarGameStateManager` 里分别是
/// `Rc<SubscriptionVec>`（7 条）、`Rc<RefCell<GraphwarEventQueue>>`（发件箱）与
/// 队列本身（入件）。我方没有事件总线，落成下面三个字段（见模块文档 §3）：
/// **订阅名**（订阅方向）、**发件箱**（真发出去的报文判别值）、**入件**（处理器被调到的证据）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GsmEvents {
    /// `new` 里 7 条 `subscribe_to_event` 的落地（顺序 = 调用序）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱：**我方真的发出去**的报文判别值（载荷由业务方法返回）。
    pub outbox: Vec<Event>,
    /// 入件：处理器被调用时记一条（= 原版"事件到了这个处理器"那一步）。
    pub delivered: Vec<Event>,
}

impl Default for GsmEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl GsmEvents {
    /// 空发件箱 + 7 条订阅（= 原版 `new` 做的同一件事）。
    pub fn new() -> Self {
        Self {
            subscribed: SUBSCRIBED_EVENTS.iter().map(|s| s.name).collect(),
            outbox: Vec::new(),
            delivered: Vec::new(),
        }
    }

    /// 记一条"事件送到处理器"（入方向）。
    pub fn deliver(&mut self, event: Event) {
        self.delivered.push(event);
    }

    /// 记一条"处理器发出去"（发方向）。
    pub fn publish(&mut self, event: Event) {
        self.outbox.push(event);
    }

    /// 取走发件箱（与原版 `GraphwarEventQueue` 的"取空"同口径）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 入件记了几条。
    pub fn delivered_len(&self) -> usize {
        self.delivered.len()
    }
}

impl Match {
    /// 原版 `GraphwarGameStateManager::handle_tick`（**`0x470840`**，816 字节）。
    ///
    /// 真值这一个函数干三件事（反汇编见 `docs/14` §11.3）：
    ///
    /// 1. **把 tick 事件转给订阅者**（`0x470880`–`0x470989`：取订阅者表，逐个
    ///    `call [vtable+0x20]`，然后释放）—— 我们**没有**事件总线，这一段只落成
    ///    [`GsmEvents::deliver`] 那一条记录（"事件确实到了这个处理器"）；
    /// 2. **回合截止 ⇒ 自动开火**（`0x470989`–`0x4709f1`）：`state == WaitingForFunctions`
    ///    ∧ `now_tick >= TurnManager::get_end_of_turn_tick()` ∧ `[self+0x11c] != 0`
    ///    ⇒ `fire_functions`。这条判据已落成纯函数
    ///    [`crate::turnstate::should_auto_fire`]（**未接线**：我们仍由调用方决定开火，
    ///    因为 `Match` 上没有 `[+0x11c]` 那一位）；
    /// 3. **时间上限到期 ⇒ 回 `Setup`**（`0x470a04`–`0x470a61`）：`[self+0xc0] == 1`
    ///    ∧ `now_tick > [self+0xc4]` ∧ `[self+0x11c] != 0` ⇒ 清 `[+0xc0]`、
    ///    `state = 1`（`Setup`）、推一条 **tag 30 `GameStateInfo{1}`**（`0x470a49` 的
    ///    `add r13, 0x1e`）。**未落地**：`[+0xc0]`/`[+0xc4]` 这对
    ///    (标志, 截止 tick) 只在 `set_lock` 一族里见过写点，语义仍是推断。
    ///
    /// 本方法因此只做**已落地的那部分**：把 [`Match::tick`] 的到期动作走一遍，并把
    /// 它们**发出去**（进 [`GsmEvents::outbox`]，判别值 `DestroyObstacle`(41)）。
    pub fn handle_tick(&mut self, now_tick: i64) -> Vec<String> {
        self.events.deliver(Event::TickEvent);
        let due = self.tick(now_tick);
        // 我方这个入口吐出去的**唯一**一类报文：到期的 `{"DestroyObstacle":…}`。
        // 原版体内那 1 处 `add_event` 属第 ③ 段（`GameStateInfo{1}`），本函数未落地。
        for _ in &due {
            self.events.publish(Event::DestroyObstacle);
        }
        due
    }

    /// **迟到的皮肤信息**（原版 `GraphwarGameStateManager::handle_graphics_info`，
    /// arm64 **`0x4aa304`**；x86 订阅表里的处理器入口 `0x4741a0`）：
    /// `SkinGraphicsInfo{graphics_str}` 决定这个实体的死亡特效。
    ///
    /// 只看两条闸门（逐位，见 `graphwar_entity_effect_manager` §6.3）：
    ///
    /// * 事件的 tag 必须是 **51 `SkinGraphicsInfo`**（调用方负责）；
    /// * `graphics_str` **长度 ≥ 4 且以 `bot_` 开头**才改写，否则原版**什么都不做**
    ///   （`0x4aa358` 的 `b.ne`）。
    ///
    /// 返回是否真的改写了登记表。
    ///
    /// ⚠️ **发布方向没接**：原版体内 1 处 `add_event` 是 tag 56 `EntityEffectInfo`
    /// （`0x4742b9` 的 `add r13, 0x39`，载荷 `{entity_id, 3}` = `AshEffect`），
    /// 我方这里只写登记表、一条报文都不发（`docs/161` §2.4 的缺口）。
    pub fn note_skin_graphics(&mut self, id: i64, graphics_str: &str) -> bool {
        self.events.deliver(Event::SkinGraphicsInfo);
        match u32::try_from(id) {
            Ok(eid) => self.entity_effects.apply_skin_graphics(eid, graphics_str),
            Err(_) => false,
        }
    }

    /// 原版 `entity_removed_event` 的处理（判定值 **14**，`0x3e9ac4`；处理器入口
    /// `0x476a20`）：**只删表项**。
    ///
    /// 死亡**不**走这里（`0x4a8cb0` 只查表、不删），所以活着登记的类型能一直用到
    /// 死亡结算那一刻。
    ///
    /// ⚠️ **发布方向没接**：原版体内 1 处 `add_event` 是 tag 44 `EffectInfo`
    /// （`0x476bbe` 的 `lea rax, [r13 + 0x2d]`），我方这里不发（见 `docs/161` §2.4）。
    pub fn handle_entity_removed(&mut self, id: i64) {
        self.events.deliver(Event::EntityRemoved);
        if let Ok(eid) = u32::try_from(id) {
            let _ = self.entity_effects.handle_entity_removed_event(
                crate::graphwar_entity_effect_manager::ENTITY_REMOVED_TAG,
                eid,
            );
        }
    }

    /// 本槽的 7 条订阅名（订阅方向的读口）。
    pub fn subscribed_events(&self) -> &[&'static str] {
        &self.events.subscribed
    }

    /// 取走发件箱（发方向的读口）。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        self.events.take_outbox()
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.events.outbox_len()
    }

    /// 入件记录（"哪些事件到了哪个处理器"）。
    pub fn delivered_events(&self) -> &[Event] {
        &self.events.delivered
    }
}

// ---------------------------------------------------------------------------
// 7. **本轮（T94）落码的 6 条 F3 缺口函数**
//
// 判据 = [`fixtures/gap_gamestate_plan.txt`](../../fixtures/gap_gamestate_plan.txt)（预注册）+ `docs/06` §140。
// 每条都写了 **vaddr**（P2 地址指认）与**真名**（P1 名字指认），函数体逐条按反汇编写，
// 单测的期望值**全部来自反汇编立即数/跳转表/分支**，没有一条来自我方输出。
//
// 这 6 条的共同形状（`观测`+`逐位`，见 §140.4）：**闸门（读 `[self+0x120]` 状态字节）→
// 改动某一格 → 经 `[self+0xd8]` 的事件队列 `add_event` 发一条 Info 报文**。
// 原版这三条 `request_*_mode_change` 走的还是 `get_module_wk::<GraphwarModeManager>()`
// （x86 弱句柄单态 **`0x46e140`**，`docs/02` §22.3 的 `wk` 列）——
// 也就是说"客户端请求改模式"实际是**本地先改、再广播**（服务器侧同名 setter 在
// `graphwar_mode_manager::GraphwarModeManager::set_*_mode`，`0x3e2830` 的 45/46/48 臂）。
// ---------------------------------------------------------------------------

/// 原版 `GraphwarGameStateManager` **自有**的几个控制位（不是实体状态）。
///
/// 我方把这个类的**运行实体**放在 [`crate::game::Match`]（模块文档 §0 的切法），
/// 而下面这几格装的不是实体状态、`Match` 里也没有对应字段 ⇒ 单独成一个纯数据块，
/// **由调用方持有并传入**（既不往 `Match` 里塞壳字段，也不把原版的位丢掉）。
///
/// 每个字段的偏移都由**写点 + 读点**两侧独立钉住（`docs/06` §140.6）：
/// `data` = 对象指针 + `0x18`（`Gd` 的 `{rc 0x0, ? 0x8, borrow 0x10}` 之后就是实例数据）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagerBytes {
    /// 数据 **`+0x11e`**（对象 `+0x136`）：**房间已锁**。
    ///
    /// * 写点**只有一处**：`set_lock` (`0x477fa5` `mov %sil,0x11e(%rdi)`)；
    /// * 读点：`handle_new_connection_event` (`0x477172` `cmpb $0x0,0x11e(%rbx)`)、
    ///   `GamewarGameClient::request_lock_change` (`0x5174f0`，**取反**后发 `LockRequest`)、
    ///   以及经对象口径的两处 `0x3e29c5`（`handle_server_remote_event`，服务端下发的锁位落地）
    ///   与 `0x4520a7`（`game_server_thread`，线程收尾清零）。
    ///
    /// 证据档：**观测**（写点/读点/信号 tag 三齐）。T84 §113.6 第 5 条把它记成"推断"，本轮升档。
    pub lock: bool,
    /// 数据 **`+0x11f`**（对象 `+0x137`）：**结局字节**。
    ///
    /// * 取值语义 = [`crate::turnstate::winner_byte`]（`0` = Team1 赢 / `1` = Team2 赢 /
    ///   `2` = 两边都没人）；`3` = **未定**（`start_game` `0x46f761` `movb $0x3,0x11f(%r12)`）；
    /// * 写点还有 `set_state` (`0x478236` `mov $0x2,%al; sub %dl,%al` ⇒ 写 `2-(x&1)` ∈ {1,2})
    ///   与 `queue_end_turn_trigger` 的闭包 (`0x475c14`)；
    /// * 读点 `check_save_level` (`0x3afd95` `movzbl 0x137(%rax)` → `test bpl,bpl; jne ret`)：
    ///   **非 0 就不写战役存档** ⇒ 战役里只有 Team1（本地玩家）赢的那一局才写进度。
    ///
    /// 证据档：**观测**（三处写点 + 两处读点）。`0x46be70` 的 TypeId 见 §140.6。
    pub winner: u8,
    /// 数据 **`+0x80`**（data ptr）/ **`+0x88`**（len）的一条 `Vec<u32>`：**敲过 `-skip` 的 entity id**。
    ///
    /// 依据 `handle_chat_message` (`0x473d4d`–`0x473daf`)：先在表里线性找 `entity_id`
    /// （4 字节步长），找不到就 `push`（`0x473daf mov %rbx,0x88(%rdi)` 写回长度）。
    /// 容量那一格在 `+0x78`（`0x473d89 cmp %rbx,0x78(%rdi)`）。
    pub skip_votes: Vec<u32>,
    /// 数据 **`+0x20`**（ptr）/ **`+0x28`**（len）的另一条 `Vec<u32>`：**本局玩家 entity id**。
    ///
    /// 依据 `handle_chat_message` (`0x473dbf`–`0x473df6`)：遍历它，逐个查"是否已在
    /// `skip_votes` 里"，**全都在**才往下走（= 所有人都同意跳过）。
    pub players: Vec<u32>,
}

/// 轴档的**换档表**（`request_axis_mode_change`，`0x4764b0`）。
///
/// 逐位：`0x4764f8 movzx ecx,byte [rbx+0x31]` / `0x4764fc shl cl,3` /
/// `0x4764ff mov edx,0x2010003` / `0x476504 shr edx,cl` / `0x47651b mov [rbx+0x31],dl`
/// ⇒ 新值 = `(0x0201_0003 >> (8·旧值)) & 0xff` = 表中的 `[3,0,1,2][旧值]`
/// ⇒ **`EveryUnit → NoAxis → OnlyMain → EveryFive → EveryUnit`**（= 旧值 − 1 mod 4）。
pub const AXIS_MODE_CHANGE: [AxisMode; 4] = [
    AxisMode::EveryUnit, // 0 → 3
    AxisMode::NoAxis,    // 1 → 0
    AxisMode::OnlyMain,  // 2 → 1
    AxisMode::EveryFive, // 3 → 2
];

/// 时间档的**换档表**（`request_time_mode_change`，`0x4765f0`）。
///
/// 逐位：`0x476638 movzx ecx,byte [rbx+0x30]` / `0x47663c shl cl,3` /
/// `0x47663f movabs rdx,0x504030201` / `0x476649 shr rdx,cl` / `0x476661 mov [rbx+0x30],dl`
/// ⇒ 新值 = `(0x0504_0302_01 >> (8·旧值)) & 0xff` = `[1,2,3,4,5][旧值]`，
/// 旧值 5 时那一位读出来是 0 ⇒ **`Timer30s → Timer1m → Timer2m → Timer3m → Timer5m → TimerInf → Timer30s`**（+1 mod 6）。
pub const TIME_MODE_CHANGE: [TimeMode; 6] = [
    TimeMode::Timer1m,
    TimeMode::Timer2m,
    TimeMode::Timer3m,
    TimeMode::Timer5m,
    TimeMode::TimerInf,
    TimeMode::Timer30s,
];

/// `handle_chat_message`（`0x473d00`）的判决。
///
/// 原版这个处理器**没有返回值**：满足全部条件时它是一条 `jmp` 到尾调用
/// `setup_and_start_game`（该函数的 vaddr 见 `docs/06` §140.4 表；**本体本轮没有落码**）
/// （= 尾调用 `setup_and_start_game`，`0x473e22`）。我方把"要不要走那一步"显式化，
/// 因为 `setup_and_start_game` 本体尚未落码（§140.5 遗留 2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipVote {
    /// 不是 `-skip` / 闸门没过 —— 原版在这里 `ret`（`0x473e27` 那一组出口）。
    Ignored,
    /// 记了一票，但还有人没投 ⇒ 原版落到 `0x473e27` `ret`。
    Recorded,
    /// 全员都投过 ⇒ 原版尾跳 `setup_and_start_game`（`0x473e1e`/`0x473e22`）。
    AllVoted,
}

impl Match {
    /// 原版 `GraphwarGameStateManager::set_lock`（**`0x477fa0`**，`0x68` 字节）。
    ///
    /// 全函数只有三步（逐条指令）：
    ///
    /// ```text
    /// 0x477fa5  mov byte [rdi+0x11e], sil    ; ① 写房间锁位
    /// 0x477fac  mov rbx, [rdi+0xd8]          ; ② 取事件队列 Rc
    /// 0x477fcb  movabs rax, 0x8000000000000045 ; ③ tag word ⇒ 判别值 0x45 = 69 LockInfo
    /// 0x477fc6  mov byte [rsp+0x10], sil     ;    载荷 = 1 字节（同一个布尔）
    /// 0x477fdf  call [slot 0x9bac80] -> 0x3d3530   ; add_event（只一条）
    /// ```
    ///
    /// ⇒ **写位 + 广播 `LockInfo{locked}`**，返回 `()`。返回的报文列表与
    /// [`Match::events`] 的发件箱是同一份东西（本文件 §3 的口径）。
    pub fn set_lock(&mut self, bits: &mut ManagerBytes, lock: bool) -> Vec<Event> {
        bits.lock = lock;
        self.events.publish(Event::LockInfo);
        vec![Event::LockInfo]
    }

    /// 原版 `GraphwarGameStateManager::set_name`（**`0x478010`**，`0xae` 字节）。
    ///
    /// ```text
    /// 0x47801d  mov rsi, [rdi] / 0x478025 mov rdi, [rbx+8] / mov edx,1
    /// 0x47802e  call [slot 0x9ba5a0] -> 0x528070   ; ① 先 __rust_dealloc 掉旧的 24 字节缓冲
    /// 0x478034  mov rax,[r14+0x10] / movups …      ; ② 把入参的 24 字节 String 拷进 self+0x00
    /// 0x47807f  movabs rax, 0x800000000000001d     ; ③ tag word ⇒ 判别值 0x1d = 29 GameNameInfo
    /// 0x47805e  mov rsi, rbx / call [slot 0x9ba8f0] -> 0x91e260  ; ④ 载荷 = self+0x00 的克隆
    /// 0x478093  call [slot 0x9bac80] -> 0x3d3530   ; add_event（只一条）
    /// ```
    ///
    /// ⇒ **换名字 + 广播 `GameNameInfo{game_name}`**。我方这一格的真值载体是
    /// [`Match::room`] 的 `room_name`（线上视图，`docs/33` 的 `NewConnection` 串里用的就是它）。
    pub fn set_name(&mut self, name: &str) -> Vec<Event> {
        self.room.room_name = name.to_string();
        self.events.publish(Event::GameNameInfo);
        vec![Event::GameNameInfo]
    }

    /// 原版 `GraphwarGameStateManager::request_axis_mode_change`（**`0x4764b0`**，`0x140` 字节）。
    ///
    /// ```text
    /// 0x4764b7  cmpb $0x1, [rdi+0x120]        ; ① 只在状态 = 1（GameState::Setup）里做
    /// 0x4764c4  add rdi, 0xe0                 ; ② self+0xe0 = 模块容器
    /// 0x4764cb  call 0x46e140                 ;    get_module_wk::<GraphwarModeManager>()（§22.3 的 wk 列）
    /// 0x4764d0  test rax,rax / je panic       ;    None ⇒ unwrap panic
    /// 0x4764f8  … 0x2010003 >> 8·[rbx+0x31]   ; ③ 换档：旧值 − 1 mod 4
    /// 0x476539  add rax, 0x2e (rax=0x7fff…)   ; ④ tag word ⇒ 0x800000000000002d ⇒ 45 AxisModeInfo
    /// 0x476547  call [slot 0x9bac80]          ;    add_event（载荷 1 字节 = 新档）
    /// ```
    ///
    /// ⇒ 轴档存在 **`GraphwarModeManager` 的数据 `+0x19`**（对象 `+0x31`）里，
    /// 与 `docs/02` §22.3 记的 `+0x18` 那一字（`word 0x0301` = `time_mode`/`axis_mode` 两个字节）**对得上**。
    pub fn request_axis_mode_change(
        &mut self,
        state: GameState,
        modes: &mut GraphwarModeManager,
    ) -> Vec<Event> {
        if state != GameState::Setup {
            return Vec::new();
        }
        modes.axis_mode = AXIS_MODE_CHANGE[modes.axis_mode as usize];
        self.events.publish(Event::AxisModeInfo);
        vec![Event::AxisModeInfo]
    }

    /// 原版 `GraphwarGameStateManager::request_time_mode_change`（**`0x4765f0`**，`0x140` 字节）。
    ///
    /// ```text
    /// 0x4765f7  cmpb $0x1, [rdi+0x120]        ; ① 只在 Setup 里做
    /// 0x47660b  call 0x46e140                 ; ② get_module_wk::<GraphwarModeManager>()
    /// 0x476638  … 0x504030201 >> 8·[rbx+0x30] ; ③ 换档：旧值 + 1 mod 6
    /// 0x47667f  add rax, 0x31 (rax=0x7fff…)   ; ④ 0x8000000000000030 ⇒ 48 TimeModeInfo
    /// 0x47668d  call [slot 0x9bac80]          ;    add_event（载荷 1 字节）
    /// ```
    ///
    /// ⇒ 时间档在 `GraphwarModeManager` 的数据 **`+0x18`**（对象 `+0x30`）。
    pub fn request_time_mode_change(
        &mut self,
        state: GameState,
        modes: &mut GraphwarModeManager,
    ) -> Vec<Event> {
        if state != GameState::Setup {
            return Vec::new();
        }
        modes.time_mode = TIME_MODE_CHANGE[modes.time_mode as usize];
        self.events.publish(Event::TimeModeInfo);
        vec![Event::TimeModeInfo]
    }

    /// 原版 `GraphwarGameStateManager::request_function_mode_change`（**`0x477410`**，`0x130` 字节）。
    ///
    /// ```text
    /// 0x477417  cmpb $0x1, [rdi+0x120]        ; ① 只在 Setup 里做
    /// 0x47742b  call 0x46e140                 ; ② get_module_wk::<GraphwarModeManager>()
    /// 0x477461  movzx ecx,byte [rbx+0x32]     ; ③ 函数档 = 数据 +0x1a
    /// 0x477465  xor cl, 1                     ;   ★ **异或 1**（2 态互翻，不是查表）
    /// 0x477470  mov [rbx+0x32], cl
    /// 0x47748e  add rax, 0x2f (rax=0x7fff…)   ; ④ 0x800000000000002e ⇒ 46 FunctionModeInfo
    /// 0x47749c  call [slot 0x9bac80]          ;    add_event（载荷 1 字节）
    /// ```
    ///
    /// ⇒ `NormalFunction ⇄ DiffEqFunction`。注意它比另两条多一道
    /// `0x477458 test rcx,rcx / jne panic`：先把借用标记当"必须空闲"检查（`try_borrow`）。
    pub fn request_function_mode_change(
        &mut self,
        state: GameState,
        modes: &mut GraphwarModeManager,
    ) -> Vec<Event> {
        if state != GameState::Setup {
            return Vec::new();
        }
        modes.function_mode = match modes.function_mode {
            FunctionMode::NormalFunction => FunctionMode::DiffEqFunction,
            FunctionMode::DiffEqFunction => FunctionMode::NormalFunction,
        };
        self.events.publish(Event::FunctionModeInfo);
        vec![Event::FunctionModeInfo]
    }

    /// 原版 `GraphwarGameStateManager::handle_chat_message`（**`0x473d00`**，`0x150` 字节）——
    /// **聊天框里的 `-skip` 是一条"全员同意就开局"的指令**（本轮新解出来的行为）。
    ///
    /// 逐条指令（`0x473d00`–`0x473e22`）：
    ///
    /// ```text
    /// 0x473d12  cmp qword [rsi], 0x8000000000000047   ; ① 判别值 0x47 = 71 ChatMessage
    /// 0x473d1b  cmp qword [rsi+0x18], 5 / jne ret     ; ② 文本长度必须 = 5
    /// 0x473d2f  xor ecx,[rax] vs 0x696b732d           ; ③ 前 4 字节 = "-ski"
    /// 0x473d35  movzx eax,[rax+4] xor 0x70            ;    第 5 字节 = 'p'  ⇒ 文本 == "-skip"
    /// 0x473d40  cmpb $0x0, [rdi+0x11c] / je ret       ; ④ 本局"已武装"位（语义未定，见下）
    /// 0x473d4d  … 线性找 [rdi+0x80][0..0x88] 里有没有 this_id（步长 4）
    /// 0x473da9  mov [rax+rbx*4], ebp / mov [rdi+0x88],rbx+1  ; ⑤ 没有就投票（push 自己的 id）
    /// 0x473dbf  … 遍历 [rdi+0x20][0..0x28]（玩家表），每个都必须已在上面那张表里
    /// 0x473df8  cmpb $0x2, [rdi+0x120] / jne ret      ; ⑥ 状态必须 = 2（GameState::WaitingForFunctions）
    /// 0x473e01  cmp qword [rdi+0x10], 5 / jb 0x473e1e  ; ⑦ 本局名字长度 < 5 ⇒ 直接开局
    /// 0x473e0c  xor ecx,[rax] vs 0x6576654c           ;    否则比前 5 字节 "Level"
    /// 0x473e17  xor eax,0x6c  (= 'l') / je ret        ;    以 "Level" 开头 ⇒ 不在这里开局
    /// 0x473e1e  … 0x473e22 jmp <setup_and_start_game> ; ⑧ 尾调用（0x473e22 是那条 jmp 自己）
    /// ```
    ///
    /// 语义复述（**观测**）：**只有不在战役关卡里（局名不以 `Level` 开头）且所有玩家都发过
    /// `-skip` 时才开局**；战役关卡靠 `setup_and_start_game` 走自己那条路。
    ///
    /// ⚠️ **两处未定，写在这里不许当结论**：
    /// ① 数据 `+0x11c` 那一位的语义（读点 6 处、写点身份未定，见 §140.6）；
    /// ② `setup_and_start_game` 本体尚未落码（见 §140.5 遗留 2）⇒ 本函数只返回判决，
    ///    不真的开局（§140.5 遗留 2）。
    pub fn handle_chat_message(
        &mut self,
        bits: &mut ManagerBytes,
        state: GameState,
        armed: bool,
        entity_id: u32,
        message: &str,
    ) -> SkipVote {
        self.events.deliver(Event::ChatMessage);
        // ②③ 只有 5 个字节且正好是 "-skip" 才算指令（0x473d1b / 0x473d2f / 0x473d35）。
        if message.len() != 5 || message != "-skip" {
            return SkipVote::Ignored;
        }
        // ④ 本局"已武装"位（0x473d40）。
        if !armed {
            return SkipVote::Ignored;
        }
        // ⑤ 投票（幂等：同一个 id 只记一次，0x473d70 的线性查表）。
        if !bits.skip_votes.contains(&entity_id) {
            bits.skip_votes.push(entity_id);
        }
        // ⑥ 玩家表里每一个都投过才继续（0x473dbf–0x473df6；空表 ⇒ 这个循环不执行）。
        if !bits.players.is_empty() && !bits.players.iter().all(|p| bits.skip_votes.contains(p)) {
            return SkipVote::Recorded;
        }
        // ⑦ 状态必须是 WaitingForFunctions（0x473df8）。
        if state != GameState::WaitingForFunctions {
            return SkipVote::Recorded;
        }
        // ⑧ 局名以 "Level" 开头 ⇒ 战役关卡，不从这里开局（0x473e0c–0x473e17）。
        if self.room.room_name.len() >= 5 && self.room.room_name.starts_with("Level") {
            return SkipVote::Recorded;
        }
        SkipVote::AllVoted
    }
}

// ---------------------------------------------------------------------------
// 8. **本轮（T111）落码的 4 条 F3「确证缺函数边界」**
//
// 判据 = [`fixtures/f3_boundary_plan.txt`](../../fixtures/f3_boundary_plan.txt)（**先提交，后落码**）
// + 清单 [`fixtures/f3_boundary_list.tsv`](../../fixtures/f3_boundary_list.tsv)。
// 每条都写 **vaddr**（ELF vaddr 口径）+ 真名（`fixtures/boundary_audit.tsv` 的 `name_src=v2唯一锚`）
// + 关键指令地址注释；单测期望值**全部来自反汇编**（tag word / 字段偏移 / 分支形状），
// 没有一条来自我方输出。
//
// 这 4 条的**共同形状**（与第 7 节那 6 条同源、`观测`+`逐位`）：
// **闸门（`[self+0x120]` 状态字节）→ `get_module_wk::<某 manager>(self+0xe0)` 取弱句柄并 upgrade →
// 在 manager 的表上做一次收集（hashbrown 的单态化，`0x3bea40` 全收 / `0x3bf230` 按 value 过滤）
// → 把 id 推进 `self+0x90` 那个 `Vec<u32>`（[`Match::removed_entities`]）→ 经 `[self+0xd8]`
// 的队列 `add_event`**。
//
// ⚠️ **本文件/本仓库没有的东西（照实记，不拿代码掩盖）**：
// * 原版的 `self+0xe0` 是 `GraphwarModuleContainer` 的弱句柄表，**我方没有这个容器**；
//   所以三条 `get_module_wk` 在下面显式变成**入参**（`&mut SoldierTable` / `&GraphwarPlayerManager`），
//   由接线层持有 —— 这与第 7 节 `request_*_mode_change` 拿 `&mut GraphwarModeManager` 同一手法。
// * 原版 `0x3bea40` / `0x3bf230` 是 **hashbrown 的 `drain`-形状单态化**（`pmovmskb`/`tzcnt` 扫控制字节），
//   它们**不写回**调用者的表（写回的是栈上的临时 view，见 `0x47380b`–`0x47381e` 的
//   `[rsp+0x38..0x58]` 组装）⇒ 这两个函数**只收集、不删表**。我方用
//   [`SoldierTable::get_soldiers_from_owner`] / [`GraphwarPlayerManager::players`] 的只读收集表达。
// ---------------------------------------------------------------------------

impl Match {
    /// 原版 `GraphwarGameStateManager::remove_all_players`（**`0x4737a0`**，532 B）。
    ///
    /// ```text
    /// 0x4737b4  add rdi, 0xe0                 ; ① self+0xe0 = 模块容器
    /// 0x4737bb  call 0x46d8c0                 ;    get_module_wk::<GraphwarPlayerManager>（docs/02 §22.3 的 x86 `wk` 列）
    /// 0x4737c0  test rax,rax / je panic       ;    None ⇒ `Option::unwrap` panic（loc 554:26）
    /// 0x4737db..0x4737eb 是否超 i64::MAX 再加引用计数（loc 486:56 的 overflow panic）
    /// 0x4737ef  rax = [rbx+0x30]              ; ② PlayerManager 的 hashbrown：ctrl 指针
    /// 0x4737f3  rcx = [rbx+0x38]              ;    bucket_mask
    /// 0x4737fd  pmovmskb / not               ;    从第 0 个 bucket 开始的控制字节掩码
    /// 0x473807  rsi = [rbx+0x48]              ;    items 计数
    /// 0x473832  call 0x3bea40                 ; ③ 收集**全部**条目（不带 key ⇒ 全收）→ Vec<u32>
    /// 0x473864  add rbp, 0xf                  ;    rbp = 0x7fff…ff + 0xf ⇒ tag word = 0x800000000000000e = **14 EntityRemoved**
    /// 0x473870  r13d = [rax + r12]            ; ④ 逐个 player id
    /// 0x473879..0x4738a4  push 到 [self+0x90] 那个 Vec<u32>（cap/ptr/len = +0x90/+0x98/+0xa0）
    /// 0x4738c5  [rsp+0x68] = r13d             ;    载荷 = player id（u32）
    /// 0x4738ca  [rsp+0x60] = rbp (tag word)
    /// 0x4738d4  call [slot 0x9bac80] → 0x3d3530  ; add_event，**每个玩家一条**
    /// ```
    ///
    /// ⇒ **把玩家表里每一个人都当成"实体被移除"播一遍**（同时静默推进 `self+0x90` 那张表）。
    /// 返回 `Vec<Event>` = 发出去报文的判别值列表（本层口径同 [`Match::set_lock`]）。
    ///
    /// 证据档：**逐位**（tag word `0x800000000000000e`、四个字段偏移 `+0xe0/+0x30/+0x38/+0x48`、
    /// 三个 push 偏移 `+0x90/+0x98/+0xa0` 都是立即数）+ **观测**（"全部播出"是循环形状读出来的）。
    pub fn remove_all_players(&mut self, players: &GraphwarPlayerManager) -> Vec<Event> {
        // ③ 收集全部键（原版 `0x3bea40` 不带 key ⇒ 一个不滤）。用 BTreeMap 的迭代序，
        //    原版是 hashbrown 的 `RandomState` 序 ⇒ **顺序不可复现**（与 PlayerManager 同款登记）。
        let ids: Vec<u32> = players.players.keys().copied().collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            self.removed_entities.push(id);
            self.events.publish(Event::EntityRemoved);
            out.push(Event::EntityRemoved);
        }
        out
    }

    /// 原版 `GraphwarGameStateManager::remove_soldier_from_player`（**`0x476830`**，487 B）。
    ///
    /// ```text
    /// 0x476841  add rdi, 0xe0 / 0x476848 call 0x46e580   ; ① get_module_wk::<GraphwarSoldierManager>()
    /// 0x476867  rax = [r14+0x10] / cmp i64::MAX / jae    ;    引用计数 +1（loc 559:26 的 overflow panic）
    /// 0x47687f  rax = [r14+0x30] / rcx = [r14+0x38]      ; ② SoldierManager 的 hashbrown 视图
    /// 0x4768ae  rax = [r14+0x48]                         ;    items 计数
    /// 0x4768b7  rax = &[rsp+0xc] = &player_id            ;    ★ key 指针 = 入参 esi（**按 value 过滤**）
    /// 0x4768cb  call 0x3bf230                            ; ③ 收集 **owner == player_id** 的士兵 id → Vec<u32>
    /// 0x4768d4  rax = [rsp+0x20] = 收集到的条数
    /// 0x4768e3  ebp = [r14 + rax*4 - 4]                  ; ④ 取**最后一个**
    /// 0x4768e8..0x476913  push 到 [self+0x90] 那个 Vec<u32>
    /// 0x476936  [rsp+0x30] = ebp；rbp = 0x7fff…ff + 0xf ;    tag word = 0x800000000000000e = **14 EntityRemoved**
    /// 0x47694d  call [slot 0x9bac80] → 0x3d3530          ; add_event **只一条**
    /// ```
    ///
    /// ⚠️ **只播最后一条**（不是每条）—— 这是**逐指令读出来的形状**，不是笔误：
    /// `0x4768e3` 一步就把 `ebp` 定成末元素，`0x47694d` 只有**一处** `add_event`，
    /// 中间没有循环（`0x4768d0`–`0x476953` 是直线代码）。
    /// **未证**：`0x3bf230` 的返回是否真能 >1 元素（我只读了它自己的 FDE
    /// `0x3bf230..0x3bf42a`，它在 `0x3bf2de` 之后确实会继续扫，但实际样本没跑过）
    /// ⇒ "只播最后一条"在 `len > 1` 时的**游戏内后果**是**未知**，这里按指令照写。
    ///
    /// 哪个士兵算"最后"：原版是 hashbrown 迭代序 ⇒ **不可复现**；
    /// 我方 [`SoldierTable::get_soldiers_from_owner`] 返回**升序**，所以"最后"= 最大 id
    /// （**这是顺序上的我方差异，登记在此，不是结论**）。
    ///
    /// 证据档：**逐位**（`+0xe0`/`+0x30`/`+0x38`/`+0x48` 四个偏移、tag word、
    /// `[rsp+0x20]` 的 len 偏移、`-4` 的末元素偏移）+ **观测**（"按 value 过滤"由 key 指针来源钉住）。
    pub fn remove_soldier_from_player(
        &mut self,
        soldiers: &SoldierTable,
        player_id: u32,
    ) -> Vec<Event> {
        let owned = soldiers.get_soldiers_from_owner(player_id);
        let Some(&last) = owned.last() else {
            // 收集为空 ⇒ 原版 `0x4768d9 je 0x476958` 直接跳过整段（**不发事件、不推表**）。
            return Vec::new();
        };
        self.removed_entities.push(last);
        self.events.publish(Event::EntityRemoved);
        vec![Event::EntityRemoved]
    }

    /// 原版 `GraphwarGameStateManager::set_player_num_soldiers`（**`0x476320`**，398 B）。
    ///
    /// ```text
    /// 0x47632e  cmpb $0x1, [rdi+0x120]        ; ① 只在状态 1 = GameState::Setup 里做
    /// 0x47633b  ebp = edx                     ;    入参 3 = 想要的士兵数（u32）
    /// 0x47633d  ebx = esi                     ;    入参 2 = player id
    /// 0x476349  call 0x46e580                 ; ② get_module_wk::<GraphwarSoldierManager>()
    /// 0x476380  rax = [r15+0x30] / rcx = [r15+0x38] / 0x4763af rsi = [r15+0x48]
    /// 0x4763c2  call 0x3bf230                 ; ③ 收集 owner == player_id 的士兵 → Vec<u32>
    /// 0x4763d6  r12 = [rsp+0x20] = **现有士兵数**
    /// 0x4763f8  r15d = ebp (want)
    /// 0x4763fb  cmp r12, r15 / jae 0x476421   ; ④ 现有 < 想要 ⇒ 进"加"循环
    /// 0x476403  r13 = want - have; inc r13    ;    循环计数 = want - have
    /// 0x476410  dec r13 / je；0x47641a call 0x474a10   ;    **add_soldier_to_player**（我方 `Match::register_player` 的接线形态）
    /// 0x476421  sub r12, r15 / jbe 0x476441   ; ⑤ 现有 > 想要 ⇒ 进"删"循环
    /// 0x476426  inc r12；0x476430 dec r12 / je；0x47643a call 0x476830  ; **remove_soldier_from_player**（上面那一条）
    /// ```
    ///
    /// ⇒ 语义 = **把该玩家的士兵数调到 `want`：少了逐个加、多了逐个删、相等什么也不做**。
    /// 两条循环的**次数都逐位来自反汇编**（`0x476403`/`0x476421` 的 `sub` + `inc/dec` 配对）。
    ///
    /// ⚠️ **"加"的一半不在本函数里**：原版加兵是调 [`Self::add_soldier_to_player`]（`0x474a10`，
    /// 已 F1，我方落在 [`crate::game::Match::register_player`] 那条接线里，**发号在调用方**）。
    /// 所以我方本函数**只执行"删"，把"加"的次数返回给接线层**（[`SoldierCountPlan::add`]）。
    /// 这是**架构差异不是证据缺口**：原版 `0x474a10` 自己发号，我方发号在 `_scaffold` 的接线层。
    ///
    /// 证据档：**逐位**（闸门立即数 `1`、两个 `sub` 的循环计数、`call 0x474a10`/`0x476830`
    /// 两个被调者地址）+ **观测**（"调到 want"的语义由两条循环的方向钉住）。
    pub fn set_player_num_soldiers(
        &mut self,
        state: GameState,
        soldiers: &SoldierTable,
        player_id: u32,
        want: u32,
    ) -> SoldierCountPlan {
        // ① 闸门：不在 Setup 就什么也不做（原版 `jne 0x476453` = 直接 ret）。
        if state != GameState::Setup {
            return SoldierCountPlan { have: 0, add: 0, remove: 0, events: Vec::new() };
        }
        // ③ 现有士兵数（原版 `[rsp+0x20]` = `0x3bf230` 收集结果的 len）。
        let have = soldiers.get_soldiers_from_owner(player_id).len() as u32;
        // ④ 加：`0x4763fb` `jae` ⇒ have >= want 时跳过；否则执行 (want - have) 次。
        let add = want.saturating_sub(have);
        // ⑤ 删：`0x476421` `jbe` ⇒ have <= want 时跳过；否则执行 (have - want) 次。
        let remove = have.saturating_sub(want);
        let mut events = Vec::new();
        for _ in 0..remove {
            events.extend(self.remove_soldier_from_player(soldiers, player_id));
        }
        SoldierCountPlan { have, add, remove, events }
    }

    /// 原版 `GraphwarGameStateManager::request_remove_player`（**`0x475590`**，790 B）。
    ///
    /// ```text
    /// 0x47559e  cmpb $0x1, [rdi+0x120]        ; ① 只在状态 1 = Setup 里做
    /// 0x4755b7  call 0x46d8c0                 ; ② get_module_wk::<GraphwarPlayerManager>()
    /// 0x4755f0  rbp = [r15+0x48]              ;    items 计数；**为 0 ⇒ 跳到 ⑦**（`0x4755f7 je 0x47569e`）
    /// 0x475609  call 0x47dec0                 ; ③ 算 hash(player_id)
    /// 0x475666  cmp ebx, [r10-8]              ;    SwissTable 查 key = player_id；命中 ⇒ `0x475692`
    /// 0x475695  eax = [rcx + r8*8 - 4]        ;    eax = 该玩家的**连接 id**（表的 value）
    /// 0x47569a  mov dl, 1                     ;    "找到了"标志
    /// 0x47575e  cmp eax, r8d / setne cl       ; ④ r8d = 全表 value 的**最小值**（`0x47572c cmovb`）
    /// 0x475764  test dl, cl / je 0x4757a8     ;    只在 **找到 且 value ≠ 最小值** 时发下面那条
    /// 0x47578f  lea rax, [r12+0x49]           ;    tag word = 0x8000000000000048 = **72 BanConnectionRequest**
    /// 0x475786  [rsp+0x18] = eax (连接 id)    ; 载荷 +8 = 自己的连接 id
    /// 0x47578a  [rsp+0x1c] = r8d (最小 id)    ; 载荷 +12 = leader 的连接 id
    /// 0x47579e  call [slot 0x9bac80]          ;    add_event（**条件**一条）
    /// 0x4757a8..0x4757d3 push ebx 到 [self+0x90]  ; ⑤ **无条件**推进那张 Vec<u32>
    /// 0x4757f8  r12 += 0xf ⇒ tag 0x800000000000000e = **14 EntityRemoved**
    /// 0x475806  call [slot 0x9bac80]          ; ⑥ **无条件**再发一条 EntityRemoved{player_id}
    /// ```
    ///
    /// ⇒ **"移除这个玩家" = 先判定要不要请服务器踢掉它，再无条件宣布这个实体消失**。
    /// 判定里的"最小值"就是 **leader = 连接 id 最小的那个玩家**（这与
    /// [`crate::graphwar_remote_event_handler`] 里那个**尚未落码**的 `is_leader` 同源，
    /// 见 `docs/06` §157 —— 那条本轮只登记、不落码，所以这里**故意不写它的地址**：
    /// `function_coverage.py` 的 P2 规则会把我方注释里出现的地址当成"地址指认"，
    /// 写上去就会给它造一个假命中（T111 已踩过一次并改正）。
    ///
    /// 证据档：**逐位**（闸门 `1`、tag word `…0048`/`…000e`、两个载荷偏移 `+0x18`/`+0x1c`、
    /// `+0x49`/`+0xf` 两个加数、`+0x90` 三格）+ **观测**（"最小值 = leader"由
    /// `0x47572c cmovb` 的 min 扫描 + `BanConnectionRequest` 的线上字段名 `leader_connection_id` 两条独立对上）。
    pub fn request_remove_player(
        &mut self,
        state: GameState,
        players: &GraphwarPlayerManager,
        player_id: u32,
    ) -> RemovePlayerRequest {
        let mut events = Vec::new();
        let mut ban = None;
        // ① 闸门
        if state != GameState::Setup {
            return RemovePlayerRequest { ban, events };
        }
        // ② 表为空 ⇒ 跳过 ③④（原版 `0x4755f7 je 0x47569e`），但仍会做 ⑤⑥。
        if !players.players.is_empty() {
            // ③ 命中标志 + 该玩家的连接 id
            let found = players.connection_of(player_id);
            // ④ 全表 value 的最小值（`0x47572c cmovb` 的 min 扫描）
            let min_conn = players.players.values().copied().min();
            if let (Some(conn), Some(min)) = (found, min_conn) {
                // `test dl, cl` ⇒ "找到" ∧ "conn != min" 才发
                if conn != min {
                    self.events.publish(Event::BanConnectionRequest);
                    ban = Some((conn, min));
                    events.push(Event::BanConnectionRequest);
                }
            }
        }
        // ⑤ 无条件推进 `self+0x90`；⑥ 无条件发 14。
        self.removed_entities.push(player_id);
        self.events.publish(Event::EntityRemoved);
        events.push(Event::EntityRemoved);
        RemovePlayerRequest { ban, events }
    }
}

/// `set_player_num_soldiers`（`0x476320`）的判决 + 执行结果。
///
/// 三个数都是**反汇编读出来的**：`have` = `0x4763d6 [rsp+0x20]`（收集结果 len），
/// `add` = `0x476403 r13 = want - have`，`remove` = `0x476421 r12 = have - want`。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SoldierCountPlan {
    /// 现有士兵数（`0x4763d6`）。
    pub have: u32,
    /// 要**加**几个（`0x476403`–`0x47641f` 的循环次数）。执行由接线层做（发号在原版 `0x474a10` 里）。
    pub add: u32,
    /// 要**删**几个（`0x476421`–`0x47643f` 的循环次数）。本函数**已经执行**了。
    pub remove: u32,
    /// 删的过程中发出去的报文判别值（每条 [`Event::EntityRemoved`]）。
    pub events: Vec<Event>,
}

/// `request_remove_player`（`0x475590`）的判决 + 载荷。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemovePlayerRequest {
    /// tag 72 的载荷 `(player_connection_id, leader_connection_id)`
    /// （原版 `0x475786 [rsp+0x18]` / `0x47578a [rsp+0x1c]`，**两个 u32**）。
    /// `None` = 这一步没发（闸门没过 / 表空 / 找不到该玩家 / 它就是 leader）。
    pub ban: Option<(u32, u32)>,
    /// 发出去的报文判别值（顺序 = 原版 `add_event` 顺序：条件性的 72 在前，14 在后）。
    pub events: Vec<Event>,
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    use crate::server::RoomState;

    fn room() -> RoomState {
        RoomState {
            room_name: "gw2".into(),
            game_state: "Setup".into(),
            axis_mode: "XY".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    /// 7 条订阅逐条钉死：名字、长度（`docs/07` §7.2 的 10/20/20/24/21/24/18）、判别值、
    /// 处理器入口、体内 `add_event` 处数。
    #[test]
    fn the_seven_subscriptions_are_the_ones_in_the_binary() {
        assert_eq!(SUBSCRIBED_EVENTS.len(), 7);
        let names: Vec<&str> = SUBSCRIBED_EVENTS.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            vec![
                "tick_event",
                "new_connection_event",
                "entity_removed_event",
                "connection_removed_event",
                "function_points_event",
                "skin_graphics_info_event",
                "chat_message_event",
            ]
        );
        // 订阅点处 `mov edx, 长度` 的立即数（`0x477c87` … `0x477e8b`）
        let lens: Vec<usize> = names.iter().map(|n| n.len()).collect();
        assert_eq!(lens, vec![10, 20, 20, 24, 21, 24, 18]);

        let tags: Vec<u16> = SUBSCRIBED_EVENTS.iter().map(|s| s.tag).collect();
        assert_eq!(tags, vec![0, 5, 14, 6, 37, 51, 71]);

        let entries: Vec<u64> = SUBSCRIBED_EVENTS.iter().map(|s| s.entry).collect();
        assert_eq!(
            entries,
            vec![0x470840, 0x477140, 0x476a20, 0x477540, 0x4758b0, 0x4741a0, 0x473d00]
        );

        // 体内 add_event 合计 10 处（`dispatch/86` 与本轮复跑同为 10）
        let adds: usize = SUBSCRIBED_EVENTS.iter().map(|s| s.add_event).sum();
        assert_eq!(adds, 10);
        let per: Vec<usize> = SUBSCRIBED_EVENTS.iter().map(|s| s.add_event).collect();
        assert_eq!(per, vec![1, 4, 1, 2, 1, 1, 0]);

        // 处理器名（arm64 `.symtab` → x86 vaddr，`fixtures/sym_match.tsv`；等级 观测）
        let handlers: Vec<&str> = SUBSCRIBED_EVENTS.iter().map(|s| s.handler).collect();
        assert_eq!(
            handlers,
            vec![
                "GraphwarGameStateManager::handle_tick",
                "GraphwarGameStateManager::handle_new_connection_event",
                "GraphwarGameStateManager::handle_entity_removed_event",
                "GraphwarGameStateManager::handle_connection_removed_event",
                "GraphwarGameStateManager::handle_function_points",
                "GraphwarGameStateManager::handle_graphics_info",
                "GraphwarGameStateManager::handle_chat_message",
            ]
        );

        // 判别值必须与 `events.rs` 的枚举同名同值
        assert_eq!(SUBSCRIBED_EVENTS[0].tag, Event::TickEvent.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[1].tag, Event::NewConnection.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[2].tag, Event::EntityRemoved.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[3].tag, Event::ConnectionRemoved.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[4].tag, Event::FunctionPoints.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[5].tag, Event::SkinGraphicsInfo.wire_tag() as u16);
        assert_eq!(SUBSCRIBED_EVENTS[6].tag, Event::ChatMessage.wire_tag() as u16);
    }

    /// 7 条里我方**真的实现**了 3 个（`handle_tick` / `handle_entity_removed` /
    /// `handle_graphics_info`），另外 4 个是**缺行为**（模块文档 §4）。
    /// 这一条是防止下一轮把"搬完"写成"7 个都搬完了"。
    #[test]
    fn only_three_handlers_have_a_body_in_this_repo() {
        let landed: Vec<&str> = SUBSCRIBED_EVENTS
            .iter()
            .filter(|s| s.landed)
            .map(|s| s.name)
            .collect();
        assert_eq!(
            landed,
            vec!["tick_event", "entity_removed_event", "skin_graphics_info_event"]
        );
        let missing: Vec<&str> = SUBSCRIBED_EVENTS
            .iter()
            .filter(|s| !s.landed)
            .map(|s| s.name)
            .collect();
        assert_eq!(
            missing,
            vec![
                "new_connection_event",
                "connection_removed_event",
                "function_points_event",
                "chat_message_event"
            ]
        );
    }

    /// 事件层一建好就是"7 条订阅 + 两个空箱"。
    #[test]
    fn the_event_layer_starts_with_seven_subscriptions_and_empty_boxes() {
        let events = GsmEvents::new();
        assert_eq!(events.subscribed.len(), 7);
        assert_eq!(events.subscribed[0], "tick_event");
        assert_eq!(events.subscribed[6], "chat_message_event");
        assert!(events.outbox.is_empty());
        assert!(events.delivered.is_empty());
        assert_eq!(events, GsmEvents::default());
    }

    /// `tick` 处理器：**入方向**记一条 `TickEvent`；没有到期对象时**发件箱为空**
    /// （原版体内那 1 处 `add_event` 属未落地的第 ③ 段）。
    #[test]
    fn handle_tick_records_delivery_and_publishes_nothing_when_nothing_is_due() {
        let mut m = Match::new(room(), vec![], vec![], (0, 0));
        assert_eq!(m.subscribed_events().len(), 7);
        assert_eq!(m.outbox_len(), 0);
        let out = m.handle_tick(0);
        assert!(out.is_empty(), "没有排队销毁 ⇒ 没有报文");
        assert_eq!(m.delivered_events(), &[Event::TickEvent]);
        assert_eq!(m.outbox_len(), 0);
        assert!(m.take_outbox().is_empty());
    }

    /// 两个登记表处理器：**只写登记表、不发报文**（发方向缺口照实记成 0）。
    #[test]
    fn the_two_registration_handlers_publish_nothing() {
        let mut m = Match::new(room(), vec![], vec![], (0, 0));
        assert!(m.note_skin_graphics(701, "bot_x"));
        assert_eq!(m.entity_effect_of(701), Some(crate::graphwar_effect_manager::EffectType::Ash));
        m.handle_entity_removed(701);
        assert_eq!(m.entity_effect_of(701), None);
        // 原版这两处体内各有 1 处 add_event（56 / 44），我方一条都不发
        assert_eq!(m.outbox_len(), 0);
        assert_eq!(
            m.delivered_events(),
            &[Event::SkinGraphicsInfo, Event::EntityRemoved]
        );
    }
}

// ---------------------------------------------------------------------------
// 6. 弹道生命周期：交给 **game_state_manager**（2026-11-26 按证据搬家）
// ---------------------------------------------------------------------------
//
// 这一段**原来**在 `graphwar_play_screen_control.rs`。搬家的依据（`docs/02` §17.4 原登记项）：
// 它对应的原版逻辑是 `graphwar_game_state_manager::handle_tick`（`0x470840`）
// + `execute_results`（`0x472470`）的状态推进 —— 两个函数的 `panic::Location` 都落在
// `src/graphwar_game_state_manager.rs`（`tools/class_probe.py func 0x472470` / `0x470840`，
// 输出里两条 `rdi, [rip+…]  'src/graphwar_game_state_manager.rs'`）。
// ⇒ 按 `AGENTS.md` 铁律「文件名就是证据」，它该住在 GSM 里，不该住在播放屏控制器里。
//
// 档位与逐条指令依据见下面每个类型/方法的注释（**观测**为主，标了"推断"的不要当结论）。

/// 一发弹道的阶段。
///
/// 对应原版状态机里的两段（`docs/14` §2）：
/// * [`ShotPhase::Idle`] ≈ `WaitingForFunctions`（没有弹道在飞）；
/// * [`ShotPhase::InFlight`] ≈ `ExecutingResults`（弹道在飞，服务端也不收新开火）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotPhase {
    Idle,
    InFlight,
}

/// 命中类别（原版死亡块里 `+0x18` 那个判别字段的两档之一）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillKind {
    /// 士兵阵亡（`LifeQueuedDeath` + `LifeInfo{alive:false}`）。
    Soldier,
    /// 靶子被打掉（`docs/150` §Q1：靶子也是**生命表**里的实体）。
    Target,
}

/// 一条**已计划、未落地**的命中（原版 `LifeQueuedDeath{entity_id, tick}` 的本地对应物）。
///
/// `point` = **弹道点序号**：弹道揭示到这个点，这条命中才落地。这就是 `0x472814`
/// 那个死亡块记录里的 `+0x10` 字段（`shl $0x4` 后当 16 字节/点的下标用）。
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedKill {
    /// 弹道点序号（从 0 起）。
    pub point: usize,
    pub kind: KillKind,
    /// [`KillKind::Soldier`] 时是**士兵下标**；靶子时无意义（`-1`）。
    pub index: i64,
    /// [`KillKind::Target`] 时是**靶子 id**；士兵时无意义（`-1`）。
    pub target_id: i64,
    /// 死者站位 —— **不是**弹道停点（`docs/15` §1.2 观测）。
    pub pos: (f64, f64),
}

impl PlannedKill {
    /// 士兵阵亡。
    pub fn soldier(point: usize, index: i64, pos: (f64, f64)) -> Self {
        PlannedKill { point, kind: KillKind::Soldier, index, target_id: -1, pos }
    }

    /// 靶子被打掉。
    pub fn target(point: usize, target_id: i64, pos: (f64, f64)) -> Self {
        PlannedKill { point, kind: KillKind::Target, index: -1, target_id, pos }
    }
}

/// 推进一帧（或一次结算）时吐出的事件。**顺序即弹道先后**。
#[derive(Debug, Clone, PartialEq)]
pub enum ShotEvent {
    /// 一名士兵阵亡（视图层据此置 `alive=false` + 摆死亡特效）。
    SoldierKilled { point: usize, index: i64, pos: (f64, f64) },
    /// 一个靶子被打掉（视图层据此销毁实体 + 摆火花）。
    TargetKilled { point: usize, target_id: i64, pos: (f64, f64) },
    /// 弹道飞完（视图层据此在**停止点**炸、清曲线、并在 `settle` 为真时收回合）。
    Arrived { stop: (f64, f64) },
}

/// [`ShotLifecycle::settle`] 的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleOutcome {
    /// 这一次调用**真的**结算了一个回合（`false` = 这一发已经结算过 ⇒ 不重复算）。
    pub settled: bool,
    /// 累计已结算的回合数（`TurnCountInfo.turn_count` 的本地对应物）。
    pub turns_used: i64,
}

/// 一发弹道的生命周期（= 播放屏那 6 个函数的"决定"部分）。
///
/// 视图层只做三件不再判断的事：把数据递进来（[`ShotLifecycle::begin`]）、
/// 把"画到第几个点"递进来（[`ShotLifecycle::advance`]）、执行吐回来的事件。
#[derive(Debug, Clone)]
pub struct ShotLifecycle {
    phase: ShotPhase,
    /// 这条弹道的总点数（`FunctionPoints.points.len()`）。
    total_points: usize,
    /// 已经揭示到的点数（由视图层从真 `Line2D` 读出来喂进来）。
    reveal: usize,
    /// 还没落地的命中，**按点序号升序**。
    pending: std::collections::VecDeque<PlannedKill>,
    /// 弹道停止点（爆炸点，`docs/14` §12.1：503/503 与爆炸点相同）。
    stop: (f64, f64),
    /// 这一发**是否已经结算过**（旧 `.gd` 的 `_logic_settled`）。
    settled: bool,
    turns_used: i64,
    /// 本发已经落地的士兵下标 / 靶子 id（同一发内同一受害者只落一次）。
    hit_soldiers: Vec<i64>,
    hit_targets: Vec<i64>,
    kills_landed: i64,
    targets_landed: i64,
}

impl Default for ShotLifecycle {
    fn default() -> Self {
        ShotLifecycle::new()
    }
}

impl ShotLifecycle {
    /// 空状态：没有弹道在飞、回合数 0。
    pub fn new() -> Self {
        ShotLifecycle {
            phase: ShotPhase::Idle,
            total_points: 0,
            reveal: 0,
            pending: std::collections::VecDeque::new(),
            stop: (0.0, 0.0),
            settled: false,
            turns_used: 0,
            hit_soldiers: Vec::new(),
            hit_targets: Vec::new(),
            kills_landed: 0,
            targets_landed: 0,
        }
    }

    /// 换局 / 退出对局：连回合数一起归零（旧 `.gd` 的 `_reset_shot_state`）。
    pub fn reset(&mut self) {
        *self = ShotLifecycle::new();
    }

    /// 开始放一发。
    ///
    /// * `total_points` —— 这条弹道**画上去的**总点数（截断后的那条，不是采样全量）；
    /// * `kills` / `target_kills` —— 已算好的命中表（顺序无要求，本层按点序号排并合并）；
    /// * `stop` —— 弹道停止点（爆炸点）；
    /// * `already_settled` —— 压平模式（automation）/ 联机：本地不再结算，到达时只收尾。
    pub fn begin(
        &mut self,
        total_points: usize,
        kills: Vec<PlannedKill>,
        target_kills: Vec<PlannedKill>,
        stop: (f64, f64),
        already_settled: bool,
    ) {
        self.phase = ShotPhase::InFlight;
        self.total_points = total_points;
        self.reveal = 0;
        self.stop = stop;
        self.settled = already_settled;
        self.hit_soldiers.clear();
        self.hit_targets.clear();
        self.kills_landed = 0;
        self.targets_landed = 0;
        self.pending = kills.into_iter().chain(target_kills).collect();
        // 按点序号升序 —— 先被打到的先落地（`docs/14` §7.3：106/106 按 `start_tick` 升序）。
        self.pending
            .make_contiguous()
            .sort_by(|a, b| a.point.cmp(&b.point).then(a.kind_index().cmp(&b.kind_index())));
    }

    /// 这一发一共多少点。
    pub fn total_points(&self) -> usize {
        self.total_points
    }

    /// "飞完了" —— 与旧 `.gd` 的 `_curve_finished()` **逐字对应**：
    /// 点数 < 2 直接算完（一条线都没有），否则揭示点数 >= 总点数。
    pub fn arrived(&self) -> bool {
        self.total_points < 2 || self.reveal >= self.total_points
    }

    /// 推进"画到第几个点"，吐回这一帧该执行的事件。
    ///
    /// 已经不在飞（或已飞完）⇒ 空表（幂等，视图层不需要自己判重复）。
    pub fn advance(&mut self, reveal: usize) -> Vec<ShotEvent> {
        let mut out = Vec::new();
        if self.phase != ShotPhase::InFlight {
            return out;
        }
        if reveal > self.reveal {
            self.reveal = reveal;
        }
        // ① 落地"弹道已经走到的那一段"里的命中。
        self.land_until(self.reveal, &mut out);
        // ② 飞完了没有（到达事件只吐一次）。
        if self.arrived() {
            // 到达前先把**剩下的全部**落完 —— 旧 `.gd` 的 `_on_shot_arrived()` 里
            // 那句 `_apply_kills_up_to(1 << 30)`（越界的命中不会因为它排在弹道之外就丢掉）。
            self.reveal = self.reveal.max(self.total_points);
            self.land_until(usize::MAX, &mut out);
            self.phase = ShotPhase::Idle;
            out.push(ShotEvent::Arrived { stop: self.stop });
        }
        out
    }

    /// 把点序号 `<= upto` 的命中落地，按队列顺序（已按点序号升序）。
    fn land_until(&mut self, upto: usize, out: &mut Vec<ShotEvent>) {
        while let Some(k) = self.pending.front() {
            if k.point > upto {
                break;
            }
            let k = self.pending.pop_front().expect("front 已判空");
            // 同一发里同一受害者只落一次（原版 `if alive` 守卫的等价物：
            // 命中表是**开火那一刻**对着存活表算的，所以陈旧只可能来自同一发内部的重复）。
            match k.kind {
                KillKind::Soldier => {
                    if self.hit_soldiers.contains(&k.index) {
                        continue;
                    }
                    self.hit_soldiers.push(k.index);
                    self.kills_landed += 1;
                    out.push(ShotEvent::SoldierKilled { point: k.point, index: k.index, pos: k.pos });
                }
                KillKind::Target => {
                    if self.hit_targets.contains(&k.target_id) {
                        continue;
                    }
                    self.hit_targets.push(k.target_id);
                    self.targets_landed += 1;
                    out.push(ShotEvent::TargetKilled {
                        point: k.point,
                        target_id: k.target_id,
                        pos: k.pos,
                    });
                }
            }
        }
    }

    /// 把在飞的那一发**直接跑完**（旧 `.gd` 的 `flush_shot()`：自检/诊断用）。
    pub fn flush(&mut self) -> Vec<ShotEvent> {
        self.advance(usize::MAX)
    }

    /// 把还没落地的命中**全部**落地，**不吐到达事件**，阶段仍是 InFlight。
    ///
    /// 两条路用它：
    /// * **压平模式**（automation）：开火当刻落地，但曲线照放 —— 到达与收尾仍由
    ///   后面每帧的 [`ShotLifecycle::advance`] 给（旧 `.gd` 的 `_logic_settled` 口径）；
    /// * **整批结算**（同时回合攒齐 / 外部来弹）：那一批里没有单条弹道在飞，
    ///   既没有"到达"也没有爆炸与清线（旧 `.gd` 的 `_resolve_shots()`），
    ///   调用方接 [`ShotLifecycle::abort`] 收尾。
    pub fn land_all(&mut self) -> Vec<ShotEvent> {
        let mut out = Vec::new();
        if self.phase != ShotPhase::InFlight {
            return out;
        }
        self.land_until(usize::MAX, &mut out);
        out
    }

    /// 收一个回合。**同一发只收一次**（`settled` 闩，旧 `.gd` 的 `_logic_settled`）。
    pub fn settle(&mut self) -> SettleOutcome {
        if self.settled {
            return SettleOutcome { settled: false, turns_used: self.turns_used };
        }
        self.settled = true;
        self.turns_used += 1;
        SettleOutcome { settled: true, turns_used: self.turns_used }
    }

    /// 半路丢弃（换局 / 联机断线）。
    pub fn abort(&mut self) {
        self.phase = ShotPhase::Idle;
        self.pending.clear();
    }

    pub fn phase(&self) -> ShotPhase {
        self.phase
    }

    pub fn in_flight(&self) -> bool {
        self.phase == ShotPhase::InFlight
    }

    pub fn revealed(&self) -> usize {
        self.reveal
    }

    /// 还没落地的命中条数（士兵 + 靶子）。
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// 还没落地的**士兵**命中条数（`level_state()["pending_kills"]`）。
    pub fn pending_soldiers(&self) -> usize {
        self.pending.iter().filter(|k| k.kind == KillKind::Soldier).count()
    }

    /// 还没落地的**靶子**命中条数。
    pub fn pending_targets(&self) -> usize {
        self.pending.iter().filter(|k| k.kind == KillKind::Target).count()
    }

    pub fn kills_landed(&self) -> i64 {
        self.kills_landed
    }

    pub fn targets_landed(&self) -> i64 {
        self.targets_landed
    }

    pub fn settled(&self) -> bool {
        self.settled
    }

    pub fn turns_used(&self) -> i64 {
        self.turns_used
    }

    /// 只读快照（自检/诊断）。
    pub fn snapshot(&self) -> LifecycleSnapshot {
        LifecycleSnapshot {
            in_flight: self.in_flight(),
            revealed: self.reveal,
            total_points: self.total_points,
            pending: self.pending.len(),
            kills_landed: self.kills_landed,
            targets_landed: self.targets_landed,
            settled: self.settled,
            turns_used: self.turns_used,
        }
    }
}

impl PlannedKill {
    /// 排序用的类别序号（同点序号时士兵先于靶子 —— 原版死亡块在前、靶子块在后）。
    fn kind_index(&self) -> u8 {
        match self.kind {
            KillKind::Soldier => 0,
            KillKind::Target => 1,
        }
    }
}

/// [`ShotLifecycle::snapshot`] 的只读形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleSnapshot {
    pub in_flight: bool,
    pub revealed: usize,
    pub total_points: usize,
    pub pending: usize,
    pub kills_landed: i64,
    pub targets_landed: i64,
    pub settled: bool,
    pub turns_used: i64,
}

// ---------------------------------------------------------------------------
// 7. 胜负判据：`LevelRules` 家族（`0x473520` + `0x4759f0` + `0x4780c0`）
// ---------------------------------------------------------------------------
//
// # 为什么这一段在 `graphwar_game_state_manager.rs` 里
//
// **2026-11-26 按 `panic::Location` 证据从 `graphwar_play_screen_control.rs` 搬来**
// （本模块就是 `GraphwarGameStateManager`；三个判据函数的 Location 一个都不落播放屏模块）。
// 证据逐条可复跑（`tools/boundary_audit.py`，台账 `fixtures/boundary_audit.tsv` 的 527 行里
// 这三行是 `match`，`loc_main_file` 全是 `src/graphwar_game_state_manager.rs`）：
//
// | 函数 | Location 行号 | 锚点数 | 符号名（三向匹配） |
// |---|---|---|---|
// | `0x473520` | `:867 :868 :869 :875 :876` | 5 | `GraphwarGameStateManager::get_alive_teams` |
// | `0x4759f0` | `:191 :256 :839 :840 :847 :848 :851 :852 :859 :860 :905 :906` | 12 | `GraphwarGameStateManager::queue_end_turn_trigger::{{closure}}` |
// | `0x4780c0` | `:223 :224 :230 :231 :256` | 5 | `GraphwarGameStateManager::set_state` |
//
// 复跑：`$PY tools/addr_probe.py locs 0x473520`（同理 `0x4759f0` / `0x4780c0`）；
//       `$PY tools/boundary_audit.py --check`（台账里这三行的 `loc_main_file`）。
//
// 原版的胜负**不在界面控制器里算**：判据是
//
// ```text
// 0x473520  GraphwarGameStateManager::get_alive_teams  -> (Team1 有活体, Team2 有活体)
// 0x4759f0  queue_end_turn_trigger（回合末）
//   0x475a64  cmp turn_count, turn_limit   ; ① 上限用尽
//   0x475a80  test al,1 / je END           ; ② Team1 没人
//   0x475a88  test dl,1 / je END           ; ③ Team2 没人
//   0x475bac  mov BYTE [self+0x120],6      ; EndingGame
// 0x4780c0  set_state(EndingGame) 分支里算胜方字节
//   0x478133..0x478236                     ; -> [self+0x11f]，表见 turnstate::winner_byte
// ```
//
// 本仓库的**真值实现在 `game.rs`**（`Match::ending` / `Match::team_has_survivor`，
// 与上面两处逐行对应）。本节的 `LevelRules` **不重算那条规则**，只把"胜方字节"那张逐位表
// （[`crate::turnstate::winner_byte`]）与两个结束入口的顺序组合成**一个结论**。
//
// 这一层存在的唯一理由：原版客户端**没有本地权威**——它收服务端的
// `GameStateInfo(EndingGame)` + 胜方字节；我们的客户端没有服务端，所以必须有
// 一个本地权威来说明"这局结束了没有、谁赢"。这段代码就是那个权威，**并且**它的
// 数值来源仍是逐位的那张表，不是我们发明的判据。
//
// 播放屏一侧仍通过 `graphwar_play_screen_control::LevelRules` 这个名字用（那里是一条
// `pub use` 再导出），所以**公开路径没变**、调用方一行没改。
//
// # 档位
//
// | 项 | 档位 | 依据 |
// |---|---|---|
// | 胜方字节 `(Team1 活, Team2 活, 设过上限)` -> `0/1/2` | **逐位** | `0x478133`–`0x478236`，见 [`crate::turnstate::winner_byte`] 的表 |
// | 两个结束入口（上限 / 某队没人）与 `EndingGame` 出口 | **逐位** | `0x475a64` / `0x475a80` / `0x475a88` → `0x475bac` |
// | 靶子算 **Team2** 的存活实体（于是"靶子全清" == "Team2 全灭"） | **观测** | `docs/150` §Q1：111 份回放 / 127 个靶子全是 `TeamInfo{Team2}`、0 个 `SoldierInfo` |
// | **哪个入口先判**（本节把 wipe 放前、上限放后） | **我方选择** | 二进制是上限在前（`0x475a64` < `0x475a80`）；**胜方**由存活位算，与入口顺序无关 ⚠️ 见 [`LevelRules::evaluate`] |
// | `mutual_wipe` 时的 `winner_team` | **我方选择** | 真值写字节 `2`（没人赢），界面需要一个队伍号去排版文案 |
// | 每关的 `turn_limit` / 靶子数 / 有没有 Bot | **逐位 + 运行时** | `graphwar_level_manager::SETUPS`（`docs/118` §7.1，21/21 与 385 份回放一致） |

/// 靶子所在的队伍（`docs/150` §Q1）。**观测**：127/127 个靶子都收到 `TeamInfo{Team2}`。
///
/// 队伍编号用**界面侧**的 0 基约定（`0 = Team1`、`1 = Team2`），与
/// `graphwar_turn_manager::Team::One.id()`（1 基）差一 —— 换算在
/// [`crate::turnstate::winner_byte`] 的调用处做一次，别在这条链上重复换。
pub const TARGET_TEAM: i64 = 1;

/// 关卡类型。**只是标签**（`docs/150` §3：判定只有一条，标签给状态行/文档看）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelKind {
    /// 对战关：有对手、无靶子（8 关：5/9/11/14/15/18/19/20）。
    Versus,
    /// 打靶关：有靶子、无对手（13 关）。
    Targets,
    /// 两者都有：21 关数据里不存在，但规则要定义。
    Both,
}

impl LevelKind {
    /// 状态行/文档里的名字（与删掉的 `level_rules.gd` 逐字一致，便于对照旧断言）。
    pub fn as_str(self) -> &'static str {
        match self {
            LevelKind::Versus => "versus",
            LevelKind::Targets => "targets",
            LevelKind::Both => "both",
        }
    }

    fn from_flags(has_targets: bool, has_opponent: bool) -> Self {
        match (has_targets, has_opponent) {
            (true, true) => LevelKind::Both,
            (true, false) => LevelKind::Targets,
            (false, _) => LevelKind::Versus,
        }
    }
}

/// 判一局要用的**输入**（全部由调用方给：界面/回放各自知道自己的局面）。
///
/// 注意它**不含**"谁的回合"：回合与胜负是两件事（`0x4759f0` 只在回合末判）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchState {
    /// 每队**存活士兵**数（键固定 0/1，缺省 0）。
    pub alive_team0: i64,
    pub alive_team1: i64,
    /// **开局时**就有士兵的队伍（含已全灭的队 —— 否则它一灭就从表里消失，判不出"这队没了"）。
    pub teams_present: Vec<i64>,
    /// 靶子集合的规模与存活数（靶子算 [`TARGET_TEAM`] 的存活实体）。
    pub targets_total: i64,
    pub targets_alive: i64,
    /// 已经结算过的回合数（与 `turn_limit` 比）。
    pub turns_used: i64,
}

impl MatchState {
    /// `alive_by_team` 取某个键（缺省 0）。
    pub fn alive_of(&self, team: i64) -> i64 {
        match team {
            0 => self.alive_team0,
            1 => self.alive_team1,
            _ => 0,
        }
    }

    /// 某队**存活实体**数 = 士兵 + （该队是 [`TARGET_TEAM] 时的）存活靶子。
    ///
    /// 这就是 `0x473520` 数的那个量（它枚举**生命表**：士兵与靶子都有 `LifeInfo`，
    /// 玩家实体没有 ⇒ 不数，`docs/150` §Q2）。
    pub fn survivors_of(&self, team: i64) -> i64 {
        let mut n = self.alive_of(team);
        if team == TARGET_TEAM && self.targets_total > 0 {
            n += self.targets_alive;
        }
        n
    }
}

/// 判定结论（界面只读这四项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    pub over: bool,
    pub human_won: bool,
    /// 界面展示用的胜方队伍号（真值"没人赢"时由本层给一个，见 [`LevelRules::evaluate`]）。
    pub winner_team: i64,
    /// **给状态行/自检看的标签**，不参与任何分支。
    pub reason: &'static str,
}

/// 一局的规则（关卡类型 + 人类队伍 + 回合上限）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelRules {
    pub kind: LevelKind,
    pub human_team: i64,
    /// 回合上限；`0` = **无上限**（原版此时压根不发 `TurnLimitInfo`，`docs/123` §5.1 #4）。
    pub turn_limit: i64,
    /// 关卡号（`-1` = 非战役对局，只为状态行/文档可追溯）。
    pub level_index: i64,
    /// 关卡表里写的目标数（只做一致性自检，判定用实际靶子集合）。
    pub targets_planned: i64,
}

impl LevelRules {
    /// 战役关：类型由"有没有靶子 / 有没有对手"两列决定（`docs/118` §7.1，**只是标签**）。
    pub fn for_campaign(
        targets_planned: i64,
        has_bot: bool,
        turn_limit: i64,
        level_index: i64,
        human_team: i64,
    ) -> Self {
        LevelRules {
            kind: LevelKind::from_flags(targets_planned > 0, has_bot),
            human_team,
            turn_limit: turn_limit.max(0),
            level_index,
            targets_planned,
        }
    }

    /// 非战役对局（LAN / 回放）：没有靶子、没有回合上限。
    pub fn for_versus(human_team: i64) -> Self {
        LevelRules {
            kind: LevelKind::Versus,
            human_team,
            turn_limit: 0,
            level_index: -1,
            targets_planned: 0,
        }
    }

    /// 通用构造（类型由"有没有靶子 / 有没有对手"决定，**只是标签**）。
    pub fn for_match(
        has_targets: bool,
        has_opponent: bool,
        turn_limit: i64,
        human_team: i64,
    ) -> Self {
        LevelRules {
            kind: LevelKind::from_flags(has_targets, has_opponent),
            human_team,
            turn_limit: turn_limit.max(0),
            level_index: -1,
            targets_planned: 0,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind.as_str()
    }

    pub fn has_targets(&self) -> bool {
        matches!(self.kind, LevelKind::Targets | LevelKind::Both)
    }

    pub fn has_opponent_rule(&self) -> bool {
        matches!(self.kind, LevelKind::Versus | LevelKind::Both)
    }

    /// **判一局的胜负。**
    ///
    /// 唯一判据（`0x473520` + `0x4759f0` ②③）：**某队没有存活实体 ⇒ 结束**，
    /// 还有活实体的那队赢；两队都没人 ⇒ 真值写字节 `2`（没人赢）。
    /// 回合上限（`0x475a64`）是**独立的**第二个结束入口，胜方同样由
    /// [`crate::turnstate::winner_byte`] 从存活位算（两边都还有人 + 设过上限 ⇒ Team2）。
    ///
    /// # 与二进制的两处差异（都是**我方选择**，写在这里别当结论）
    ///
    /// 1. **入口顺序**：二进制先判上限（`0x475a64`）、后判 wipe（`0x475a80`/`0x475a88`）；
    ///    本层先判 wipe。**判定结果不变**（胜方由存活位算，与谁先跳无关），
    ///    只有"最后一回合清空靶子"时的 `reason` 标签不同：二进制那条路也会进 END，
    ///    但胜方字节是 `0`（Team1）⇒ 人类照样赢。本层把这个收场标成 `targets_cleared`。
    /// 2. **`mutual_wipe` 的 `winner_team`**：真值写 `2`（没人赢），界面必须有队伍号去
    ///    排版文案，所以本层填"人类以外的那一队"。`human_won` 仍是 `false`。
    pub fn evaluate(&self, st: &MatchState) -> Verdict {
        // 参评的队伍 = 有士兵的队 ∪ 靶子所在的队（靶子有队伍，`docs/150` §Q1）。
        let mut judged: Vec<i64> = Vec::new();
        for t in &st.teams_present {
            if !judged.contains(t) {
                judged.push(*t);
            }
        }
        if st.targets_total > 0 && !judged.contains(&TARGET_TEAM) {
            judged.push(TARGET_TEAM);
        }
        // 什么都没有（例如回放模式没有玩家表）⇒ 明确说"判不了"，**不猜**胜负。
        if judged.is_empty() {
            return Verdict {
                over: false,
                human_won: false,
                winner_team: self.human_team,
                reason: "no_verdict",
            };
        }

        let team1_alive = st.survivors_of(0) > 0;
        let team2_alive = st.survivors_of(1) > 0;
        let wiped: Vec<i64> = judged
            .iter()
            .copied()
            .filter(|t| st.survivors_of(*t) == 0)
            .collect();
        let limit_hit = self.turn_limit > 0 && st.turns_used >= self.turn_limit;

        if wiped.is_empty() && !limit_hit {
            return Verdict {
                over: false,
                human_won: false,
                winner_team: self.human_team,
                reason: "playing",
            };
        }

        // 胜方字节：**逐位**那张表（`0x478133`–`0x478236`）。
        // `turn_limit_set` 是 "set_turn_limit 被调过"（`[+0xc8] == 1`），**不是**"上限用尽"。
        let byte = crate::turnstate::winner_byte(team1_alive, team2_alive, self.turn_limit > 0);
        // 字节 -> 界面队伍号：0 = Team1、1 = Team2、2 = 没人赢。
        let winner_from_byte = match byte {
            0 => Some(0),
            1 => Some(1),
            _ => None,
        };
        let human_won = winner_from_byte == Some(self.human_team) && !wiped.contains(&self.human_team);
        let reason: &'static str = if wiped.contains(&0) && wiped.contains(&1) {
            "mutual_wipe"
        } else if wiped.contains(&self.human_team) {
            "own_wiped"
        } else if !wiped.is_empty() {
            // 对手方（打靶关 = 靶子所在的 Team2）全灭 ⇒ 人类胜（`0x475a88`）。
            // 两队都没有士兵、清空它的只能是靶子 ⇒ 标签换成 targets_cleared。
            // ⚠️ 这是**同一个判据**的两种标签，不是两条规则。
            let w = wiped
                .iter()
                .copied()
                .find(|t| *t != self.human_team)
                .unwrap_or(TARGET_TEAM);
            if w == TARGET_TEAM && st.alive_of(w) == 0 && st.targets_total > 0 {
                "targets_cleared"
            } else {
                "enemy_wiped"
            }
        } else {
            // 上限用尽（`0x475a64`）：两边都还有人 ⇒ 胜方字节写死 `1`（Team2）。
            "turn_limit"
        };

        let winner_team = winner_from_byte.unwrap_or_else(|| {
            // 真值"没人赢"（字节 2）：界面给一个可排版的队伍号（我方选择）。
            if self.human_team == 0 {
                1
            } else {
                0
            }
        });
        Verdict { over: true, human_won, winner_team, reason }
    }
}

// ---------------------------------------------------------------------------
// 8. `docs/06` §140 落码函数的**单测**（期望值全部来自反汇编，不是来自我方输出）
//
// 每条测试的注释都写了"期望值从哪条指令来"。凡是判据只有一半证据的，
// 测试只钉**有证据的那一半**，另一半在函数文档里标 `缺：…`。
// ---------------------------------------------------------------------------
#[cfg(test)]
mod gap_f3_tests {
    use super::*;
    use crate::server::RoomState;

    fn room(name: &str) -> RoomState {
        RoomState {
            room_name: name.into(),
            game_state: "Setup".into(),
            axis_mode: "NoAxis".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer30s".into(),
        }
    }

    fn match_with(name: &str) -> Match {
        Match::new(room(name), vec![], vec![], (0, 0))
    }

    /// `set_lock`（`0x477fa0`）：写锁位 + **只发一条** `LockInfo`（tag 69）。
    ///
    /// 期望值来源：`0x477fcb movabs rax,0x8000000000000045` ⇒ 判别值 `0x45 = 69`；
    /// 体内只有一处 `add_event`（`0x477fdf`）⇒ 报文条数 1。
    #[test]
    fn set_lock_writes_the_bit_and_emits_exactly_one_lock_info() {
        let mut m = match_with("gw2");
        let mut bits = ManagerBytes::default();
        assert!(!bits.lock);
        let out = m.set_lock(&mut bits, true);
        assert_eq!(out, vec![Event::LockInfo]);
        assert!(bits.lock);
        assert_eq!(m.take_outbox(), vec![Event::LockInfo]);
        // 再置一次同一值：原版也照样写位 + 发信（没有"值没变就不发"的分支）
        let out = m.set_lock(&mut bits, true);
        assert_eq!(out, vec![Event::LockInfo]);
        assert_eq!(m.take_outbox(), vec![Event::LockInfo]);
        // 解锁
        let out = m.set_lock(&mut bits, false);
        assert_eq!(out, vec![Event::LockInfo]);
        assert!(!bits.lock);
    }

    /// `set_name`（`0x478010`）：换名字 + 发一条 `GameNameInfo`（tag 29）。
    ///
    /// 期望值来源：`0x47807f movabs rax,0x800000000000001d` ⇒ 判别值 `0x1d = 29`。
    #[test]
    fn set_name_replaces_the_name_and_emits_one_game_name_info() {
        let mut m = match_with("old");
        let out = m.set_name("Level 3");
        assert_eq!(out, vec![Event::GameNameInfo]);
        assert_eq!(m.room.room_name, "Level 3");
        assert_eq!(m.take_outbox(), vec![Event::GameNameInfo]);
    }

    /// `request_axis_mode_change`（`0x4764b0`）：换档表 `[3,0,1,2]`（旧值 − 1 mod 4），
    /// 且**只在 `GameState::Setup`（判别值 1）里生效**。
    ///
    /// 期望值来源：`0x4764ff mov edx,0x2010003` + `0x476504 shr edx,cl`（cl = 8·旧值）
    /// ⇒ `(0x0201_0003 >> 8k) & 0xff` 依次是 `3,0,1,2`；闸门 `0x4764b7 cmpb $0x1,[rdi+0x120]`
    /// 而 `[+0x120]` 的判别值 = `turnstate::GameState::index()`（`Connecting=0, Setup=1, …`）。
    #[test]
    fn request_axis_mode_change_is_the_decrement_table_and_is_gated_on_setup() {
        let mut m = match_with("gw2");
        let mut modes = GraphwarModeManager::default(); // axis = NoAxis(0)
        // 0 → 3（EveryUnit）
        assert_eq!(
            m.request_axis_mode_change(GameState::Setup, &mut modes),
            vec![Event::AxisModeInfo]
        );
        assert_eq!(modes.axis_mode, AxisMode::EveryUnit);
        // 3 → 2（EveryFive）
        m.request_axis_mode_change(GameState::Setup, &mut modes);
        assert_eq!(modes.axis_mode, AxisMode::EveryFive);
        // 2 → 1（OnlyMain）
        m.request_axis_mode_change(GameState::Setup, &mut modes);
        assert_eq!(modes.axis_mode, AxisMode::OnlyMain);
        // 1 → 0（NoAxis）—— 一圈回到起点
        m.request_axis_mode_change(GameState::Setup, &mut modes);
        assert_eq!(modes.axis_mode, AxisMode::NoAxis);
        // 闸门：非 Setup 一个字节都不改、一条都不发
        let before = modes.axis_mode;
        let n = m.outbox_len();
        for st in [GameState::WaitingForFunctions, GameState::EndingGame, GameState::Connecting] {
            assert!(m.request_axis_mode_change(st, &mut modes).is_empty());
        }
        assert_eq!(modes.axis_mode, before);
        assert_eq!(m.outbox_len(), n);
    }

    /// `request_time_mode_change`（`0x4765f0`）：换档表 `[1,2,3,4,5]`（+1 mod 6），闸门同上。
    ///
    /// 期望值来源：`0x47663f movabs rdx,0x504030201` + `0x476649 shr rdx,cl`
    /// ⇒ `(0x0504_0302_01 >> 8k) & 0xff` = `1,2,3,4,5`，k=5 时那一位读出来是 0。
    #[test]
    fn request_time_mode_change_is_the_increment_table_and_wraps_to_the_first() {
        let mut m = match_with("gw2");
        let mut modes = GraphwarModeManager::default(); // Timer30s(0)
        let expect = [
            TimeMode::Timer1m,
            TimeMode::Timer2m,
            TimeMode::Timer3m,
            TimeMode::Timer5m,
            TimeMode::TimerInf,
            TimeMode::Timer30s, // 5 → 0（表末尾读出来的 0 字节）
        ];
        for (i, want) in expect.iter().enumerate() {
            assert_eq!(
                m.request_time_mode_change(GameState::Setup, &mut modes),
                vec![Event::TimeModeInfo],
                "第 {i} 次换档"
            );
            assert_eq!(modes.time_mode, *want, "第 {i} 次换档");
        }
        let n = m.outbox_len();
        assert!(m.request_time_mode_change(GameState::WaitingForFunctions, &mut modes).is_empty());
        assert_eq!(m.outbox_len(), n);
    }

    /// `request_function_mode_change`（`0x477410`）：**异或 1** 的 2 态互翻（不是查表）。
    ///
    /// 期望值来源：`0x477465 xor cl,1`（+ `0x477461 movzx ecx,byte [rbx+0x32]`），
    /// 判别值 46 来自 `0x47748e add rax,0x2f`（rax = `0x7fff…ff`）。
    #[test]
    fn request_function_mode_change_toggles_the_two_state_byte() {
        let mut m = match_with("gw2");
        let mut modes = GraphwarModeManager::default(); // NormalFunction(0)
        m.request_function_mode_change(GameState::Setup, &mut modes);
        assert_eq!(modes.function_mode, FunctionMode::DiffEqFunction);
        m.request_function_mode_change(GameState::Setup, &mut modes);
        assert_eq!(modes.function_mode, FunctionMode::NormalFunction);
        assert_eq!(m.take_outbox(), vec![Event::FunctionModeInfo, Event::FunctionModeInfo]);
        // 闸门
        let n = m.outbox_len();
        assert!(m.request_function_mode_change(GameState::EndingGame, &mut modes).is_empty());
        assert_eq!(m.outbox_len(), n);
    }

    /// `handle_chat_message`（`0x473d00`）：`-skip` 全员投票 ⇒ 尾跳 `setup_and_start_game`。
    ///
    /// 期望值来源（逐条指令见函数文档）：
    /// * `0x473d1b cmp qword [rsi+0x18],5` ⇒ 长度必须 **5**；
    /// * `0x473d2f/…35` 比对 `"-ski"` + `'p'` ⇒ 文本必须 **`-skip`**；
    /// * `0x473df8 cmpb $0x2,[rdi+0x120]` ⇒ 状态必须 **`WaitingForFunctions`**；
    /// * `0x473e0c xor ecx,0x6576654c`（`"Leve"`）+ `xor eax,0x6c`（`'l'`）⇒ 局名以
    ///   **`Level`** 开头就不开局。
    ///
    /// 正/反面对照都在这一条里：每条闸门各给一个"过"和一个"不过"。
    #[test]
    fn chat_skip_starts_the_game_only_when_every_player_voted() {
        let mut m = match_with("gw2");
        let mut bits = ManagerBytes { players: vec![10, 11], ..Default::default() };
        let ready = GameState::WaitingForFunctions;

        // ① 文本不是 "-skip"：长度对但不是这 5 个字节 / 字节对但长度不对，都不算
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 10, "hello"), SkipVote::Ignored);
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 10, "-ski"), SkipVote::Ignored);
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 10, "-skip!"), SkipVote::Ignored);
        assert!(bits.skip_votes.is_empty(), "闸门没过 ⇒ 一票都不记");

        // ② armed 位（+0x11c）为 0 ⇒ 不算
        assert_eq!(m.handle_chat_message(&mut bits, ready, false, 10, "-skip"), SkipVote::Ignored);
        assert!(bits.skip_votes.is_empty());

        // ③ 玩家 10 先投：只记一票，玩家 11 还没投 ⇒ Recorded
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 10, "-skip"), SkipVote::Recorded);
        assert_eq!(bits.skip_votes, vec![10]);
        // 幂等：同一个人再发一次不重复记（0x473d70 的线性查表）
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 10, "-skip"), SkipVote::Recorded);
        assert_eq!(bits.skip_votes, vec![10]);

        // ④ 玩家 11 也投 ⇒ 全票
        assert_eq!(m.handle_chat_message(&mut bits, ready, true, 11, "-skip"), SkipVote::AllVoted);
        assert_eq!(bits.skip_votes, vec![10, 11]);

        // ⑤ 全票但状态不是 WaitingForFunctions ⇒ 不开局
        for st in [GameState::Setup, GameState::CalculatingResults, GameState::EndingGame] {
            assert_eq!(
                m.handle_chat_message(&mut bits, st, true, 10, "-skip"),
                SkipVote::Recorded,
                "{st:?}"
            );
        }

        // ⑥ 全票 + 状态对，但局名以 "Level" 开头 ⇒ 战役关卡，不开局
        let mut lvl = match_with("Level 7");
        let mut b2 = ManagerBytes { players: vec![10], ..Default::default() };
        assert_eq!(
            lvl.handle_chat_message(&mut b2, ready, true, 10, "-skip"),
            SkipVote::Recorded
        );
        // ⑦ 局名短于 5（`0x473e01 jb`）⇒ 直接开局，不比对 "Level"
        let mut short = match_with("gw2");
        let mut b3 = ManagerBytes { players: vec![10], ..Default::default() };
        assert_eq!(
            short.handle_chat_message(&mut b3, ready, true, 10, "-skip"),
            SkipVote::AllVoted
        );
        // 反向对照：把同一个短名字改成 "Level 7" 之后判决翻转
        short.set_name("Level 7");
        let mut b4 = ManagerBytes { players: vec![10], ..Default::default() };
        assert_eq!(
            short.handle_chat_message(&mut b4, ready, true, 10, "-skip"),
            SkipVote::Recorded
        );
    }

    /// 换档表本身钉死（`0x02010003` / `0x0504030201` 两个立即数的**逐字节展开**）。
    ///
    /// 这条是 §139 表里"立即数 ⇒ 表"的独立复算：即使函数的调用形态变了，
    /// 表错了这条也会红。
    #[test]
    fn the_two_change_tables_are_the_little_endian_bytes_of_the_two_immediates() {
        // 0x0201_0003 的 LE 字节 = [0x03, 0x00, 0x01, 0x02]
        let axis_bytes: Vec<u8> = AXIS_MODE_CHANGE.iter().map(|m| *m as u8).collect();
        assert_eq!(axis_bytes, 0x0201_0003u32.to_le_bytes().to_vec());
        // 0x05_0403_0201 的 LE 字节 = [0x01, 0x02, 0x03, 0x04, 0x05, 0x00]
        // （立即数只有 5 个非 0 字节 ⇒ 第 6 次 `shr` 读到的是 0 = Timer30s）
        let time_bytes: Vec<u8> = TIME_MODE_CHANGE.iter().map(|m| *m as u8).collect();
        assert_eq!(time_bytes, 0x0504_0302_01u64.to_le_bytes()[..6].to_vec());
    }
}

/// T111 落码的 4 条 F3 边界函数的单测。
///
/// **期望值来源**（每条都写在断言旁）：全部是反汇编里的立即数 / 表偏移 / 分支形状，
/// 没有一条来自我方输出（`fixtures/f3_boundary_plan.txt` §1 第 4 条）。
#[cfg(test)]
mod f3_boundary_tests {
    use super::*;
    use crate::server::RoomState;

    fn room() -> RoomState {
        RoomState {
            room_name: "gw2".into(),
            game_state: "Setup".into(),
            axis_mode: "XY".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    fn match_new() -> Match {
        Match::new(room(), vec![], vec![], (0, 0))
    }

    /// `remove_all_players`（`0x4737a0`）：玩家表里每人**一条** `EntityRemoved`。
    ///
    /// 期望值来源：
    /// * tag word `0x800000000000000e`（`0x473864 add rbp,0xf`，rbp = `0x7fffffffffffffff`）⇒ `14` = `EntityRemoved`；
    /// * `0x473870` 起是**循环**（`0x4738e2 cmp rbx,r12 / jne 0x473870`，步长 4 = `u32`）⇒ 每个玩家一条，不是只一条。
    #[test]
    fn remove_all_players_emits_one_entity_removed_per_player() {
        let mut m = match_new();
        let mut pm = GraphwarPlayerManager::new();
        pm.add_player(10, 100);
        pm.add_player(20, 50);
        pm.add_player(30, 200);
        let _ = pm.take_outbox();

        let out = m.remove_all_players(&pm);
        // 3 个玩家 ⇒ 3 条（循环形状），且判别值都是 14
        assert_eq!(out, vec![Event::EntityRemoved; 3]);
        assert_eq!(m.take_outbox(), vec![Event::EntityRemoved; 3]);
        // `+0x90` 那张 `Vec<u32>` 也推进了 3 条（`0x473879..0x4738a4` 的 push）
        let mut ids = m.removed_entities.clone();
        ids.sort_unstable();
        assert_eq!(ids, vec![10, 20, 30]);
        assert_eq!(m.removed_entities.len(), 3);

        // 空表 ⇒ 收集长度 0 ⇒ `0x473857 je 0x4738e7` 跳过整段循环（一条都不发）
        let mut m2 = match_new();
        let empty = GraphwarPlayerManager::new();
        assert!(m2.remove_all_players(&empty).is_empty());
        assert!(m2.removed_entities.is_empty());
    }

    /// `remove_soldier_from_player`（`0x476830`）：**只播最后一条**，且收集为空时一条不发。
    ///
    /// 期望值来源：
    /// * `0x4768e3 ebp = [r14 + rax*4 - 4]` ⇒ 取**末**元素（不是第一个、不是全部）；
    /// * `0x4768d9 test rax,rax / je 0x476958` ⇒ 收集长度 0 时整段跳过；
    /// * `0x476948` 只有**一处** `add_event`（`call [slot 0x9bac80]`），中间无循环
    ///   （`0x4768d0`–`0x476953` 是直线代码）⇒ 报文条数 ≤ 1。
    #[test]
    fn remove_soldier_from_player_emits_only_the_last_owned_soldier() {
        let mut m = match_new();
        let mut st = SoldierTable::new();
        st.add_soldier(10, 7);
        st.add_soldier(20, 7);
        st.add_soldier(30, 8);

        // 玩家 7 有两个兵（10、20）⇒ 只播**最后**一个（我方升序 ⇒ 20）
        assert_eq!(m.remove_soldier_from_player(&st, 7), vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![20]);
        assert_eq!(m.take_outbox(), vec![Event::EntityRemoved]);

        // 玩家 8 只有一个兵 ⇒ 就是它
        assert_eq!(m.remove_soldier_from_player(&st, 8), vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![20, 30]);
        assert_eq!(m.take_outbox(), vec![Event::EntityRemoved], "清空发件箱再验下一条");

        // 玩家 9 一个兵都没有 ⇒ 空了：一条不发、`+0x90` 也不推进（`0x4768d9 je`）
        assert!(m.remove_soldier_from_player(&st, 9).is_empty());
        assert_eq!(m.removed_entities, vec![20, 30]);
        assert!(m.take_outbox().is_empty());
    }

    /// `set_player_num_soldiers`（`0x476320`）：闸门是**状态 1 = Setup**，两个方向都要对。
    ///
    /// 期望值来源：`0x47632e cmpb $0x1,[rdi+0x120] / jne 0x476453` ⇒ 非 Setup 直接 ret；
    /// `0x4763fb cmp r12,r15 / jae` + `0x476403 r13 = want - have` ⇒ 加的次数 = `want − have`；
    /// `0x476421 sub r12,r15 / jbe` ⇒ 删的次数 = `have − want`。
    #[test]
    fn set_player_num_soldiers_is_setup_gated_and_counts_both_directions() {
        let mut m = match_new();
        let mut st = SoldierTable::new();
        st.add_soldier(10, 7);
        st.add_soldier(20, 7);

        // 想要 5、现有 2 ⇒ 加 3、删 0、一条报文都不发
        let plan = m.set_player_num_soldiers(GameState::Setup, &st, 7, 5);
        assert_eq!((plan.have, plan.add, plan.remove), (2, 3, 0));
        assert!(plan.events.is_empty());
        assert!(m.take_outbox().is_empty());

        // 想要 1、现有 2 ⇒ 加 0、删 1（这一次真的删，发 1 条 EntityRemoved）
        let plan = m.set_player_num_soldiers(GameState::Setup, &st, 7, 1);
        assert_eq!((plan.have, plan.add, plan.remove), (2, 0, 1));
        assert_eq!(plan.events, vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![20]);

        assert_eq!(m.take_outbox(), vec![Event::EntityRemoved], "清空发件箱再验闸门");
        // 想要 = 现有 ⇒ 两条循环都跳过
        let plan = m.set_player_num_soldiers(GameState::Setup, &st, 7, 2);
        assert_eq!((plan.have, plan.add, plan.remove), (2, 0, 0));

        // 闸门：只要不是 Setup，一律什么都不做（原版 `jne 0x476453` 直接 ret）
        for st_state in [
            GameState::Connecting,
            GameState::WaitingForFunctions,
            GameState::CalculatingResults,
            GameState::SynchronizingResults,
            GameState::ExecutingResults,
            GameState::EndingGame,
            GameState::Disconnected,
        ] {
            let plan = m.set_player_num_soldiers(st_state, &st, 7, 9);
            assert_eq!(plan, SoldierCountPlan::default(), "{st_state:?}");
        }
        assert!(m.take_outbox().is_empty(), "闸门没过 ⇒ 一条报文都不该有");
    }

    /// `request_remove_player`（`0x475590`）：tag 72 只在"找到 ∧ 不是 leader"时发，
    /// tag 14 **无条件**发。
    ///
    /// 期望值来源：
    /// * `0x475764 test dl,cl / je 0x4757a8` ⇒ 72 是**条件**发的（dl = 找到，cl = conn ≠ min）；
    /// * `0x47572c cmovb` 扫的是**最小值** ⇒ leader = 连接 id 最小者；
    /// * `0x47578f lea rax,[r12+0x49]` ⇒ 72（`0x7fffffffffffffff + 0x49 = 0x8000000000000048`）；
    /// * `0x475786/0x47578a` 载荷 = **两个 u32**（`+0x18`/`+0x1c`），顺序 = (自己的 conn, 最小的 conn)；
    /// * `0x4757f8 add r12,0xf` ⇒ 14 且这条在 `0x4757a8` 之后**无条件**执行。
    #[test]
    fn request_remove_player_bans_only_non_leaders_and_always_announces() {
        let mut m = match_new();
        let mut pm = GraphwarPlayerManager::new();
        pm.add_player(1, 100);
        pm.add_player(2, 50); // ← 连接 id 最小 ⇒ leader
        pm.add_player(3, 200);
        let _ = pm.take_outbox();

        // 玩家 1（conn 100，leader 是 50）⇒ 发 72 再从发 14
        let r = m.request_remove_player(GameState::Setup, &pm, 1);
        assert_eq!(r.ban, Some((100, 50)));
        assert_eq!(r.events, vec![Event::BanConnectionRequest, Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![1]);

        // 玩家 2 自己就是 leader（conn = min）⇒ **不发** 72，但仍发 14
        let r = m.request_remove_player(GameState::Setup, &pm, 2);
        assert_eq!(r.ban, None);
        assert_eq!(r.events, vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![1, 2]);

        // 表里没有这个玩家 ⇒ 不发 72，仍发 14（`0x4757a8` 在条件块之后）
        let r = m.request_remove_player(GameState::Setup, &pm, 99);
        assert_eq!(r.ban, None);
        assert_eq!(r.events, vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![1, 2, 99]);

        // 空表 ⇒ 跳过 ③④（`0x4755f7 je 0x47569e`），但 ⑤⑥ 照做
        let empty = GraphwarPlayerManager::new();
        let r = m.request_remove_player(GameState::Setup, &empty, 5);
        assert_eq!(r.ban, None);
        assert_eq!(r.events, vec![Event::EntityRemoved]);
        assert_eq!(m.removed_entities, vec![1, 2, 99, 5]);

        // 闸门（`0x47559e cmpb $0x1`）：非 Setup ⇒ 一条都不发、表也不推进
        let mut m2 = match_new();
        let r = m2.request_remove_player(GameState::WaitingForFunctions, &pm, 1);
        assert_eq!(r, RemovePlayerRequest::default());
        assert!(m2.removed_entities.is_empty());
        assert!(m2.take_outbox().is_empty());
    }
}
