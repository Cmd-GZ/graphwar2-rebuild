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
