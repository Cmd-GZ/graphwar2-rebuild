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
//! | `chat_message` `0x473d00` | 体内 **0** 条 `add_event`（纯转发给订阅者） | 聊天链在 `_scaffold/graphwar_setup_control_class.rs`（`chat_message` 信号），不在本模块 |
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
