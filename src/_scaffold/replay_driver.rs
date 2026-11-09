//! **不是原版的东西**：回放的**驱动**（load → 按 tick 推进 → 派发事件 → 结算批落地 → 下一批）。
//!
//! # 为什么会有这一份（`dispatch/81`）
//!
//! 在它之前，"回放怎么往前走"这件事写在 **GDScript** 里三处：
//! `client/src/play/replay_player.gd::advance_dt`（`tick += dt × 60`，**每帧一次**）、
//! `client/src/screens/play_screen.gd::_replay_tick`（每帧调它）、
//! `play_screen.gd::_apply_replay_events`（按 `kind` 决定重画哪一层）。
//! 那份 `.gd` 还是 `src/graphwar_replay_player.rs`（原版模块，1182 行）的**第二份实现**
//! —— 连 `canonical()` 的字符串格式都是照抄的（`docs/133` §5 三端对拍）。
//!
//! 本轮把**驱动**搬进 Rust，理由不是"Rust 更好"，而是 `docs/06` §44 量到的一条真值：
//! **原版的事件泵只在 tick 那一圈里跑**（`pumps ≡ ticks`），**不是每帧一次**。
//! 按帧写的驱动天然是错的（默认帧率下 **10.9% 的帧一个事件都不该派发**），
//! 而"每帧一次"这个错**看不出来**：它只是让回放跑快一点、把 0-tick 帧也泵一遍。
//!
//! # 节奏真值（`docs/06` §44，5 次运行 / 12 856 个 tick，**观测**）
//!
//! | 真值 | 数 | 出处 |
//! |---|---|---|
//! | tick 率 | **60.00 /s**（帧率 30→67 五档全中，跨 70 s 零漂移） | §44.2 |
//! | 每帧几个 tick | 默认 64.9 fps 下 `0:507(10.9%) 1:4116 2:15` | §44.3 |
//! | 泵的节奏 | **`pumps ≡ ticks` 逐帧严格相等**（五轮全部相等）⇒ 0-tick 帧泵 0 次 | §44.3 |
//! | 顺序 | 恒 `TP`（先 tick `0x518499`、后泵 `0x51849f`），`first-event-is-pump` 五轮全 0 | §44.3 |
//! | 追赶 | **无上限**：200/600/1500 ms 冻结 ⇒ 13/37/91 圈 | §44.4 |
//! | `delta` | **不读**（指令层 0 处 `xmm` + 行为层 `--fixed-fps 10` 下 delta 恒 0.1 s 而 tick 不动） | §44.5 |
//!
//! ⇒ 本文件的循环体就是**一个定步累加器**：`accum += Δt`；`n = accum / 16.667ms`；
//! 余数留下；然后 `for _ in 0..n { tick(); pump(); }`。泵在 tick 里面，所以
//! `pumps ≡ ticks` 是**结构性的**，不是一句承诺。
//!
//! # 两处诚实的偏离（都登记在 `docs/16-client.md` §56.6）
//!
//! 1. **`delta` 我们读了**。原版不读（§44.5 双证），但原版读的是
//!    `GraphwarTickHandler::get_time_to_tick` 的**墙钟**；headless 里没有墙钟，
//!    读墙钟会让每条断言变成竞态。所以这里把 `drive_frame(dt)` 的 `dt` 当墙钟的**替身**，
//!    并且提供确定性原语 [`ReplayDriver::drive_ticks`]（按 tick 数直接推进）。
//!    节奏仍服从上面五条真值。档位：**我方自定**。
//! 2. **"对局中"门我们用的是 `playing`**，不是 `[backend+0x1e0]`。原版那条门
//!    （`0x515dd1`，菜单 28 个 1 s 窗口实测 0/s）挡的是"不在对局里就不 tick"；
//!    回放屏没有"对局"概念，我们拿"用户按了播放"当同一件事。档位：**我方投影**。
//!
//! # 结算批（`docs/16-client.md` §56.2）
//!
//! "一批"= `GameStateInfo{CalculatingResults}` → `GameStateInfo{ExecutingResults}`
//! 那一窗，后面跟着"回合末延迟段"（`LifeInfo` / `DestroyObstacle` 落在这里）。
//! 切分逻辑**不在这里**：直接调 [`crate::_scaffold::replaydiff::split_batches`]
//! （`docs/136`，401 份冻结语料上量过的同一个函数），本文件只把它的**消息下标**
//! 换成**tick**，好在 tick 时钟上判边界 —— **没有第二份批切分**。
//!
//! 换 tick 的原因：内核播放器的游标是"内核解析器"的下标，批切分是"replaydiff 解析器"
//! 的下标，两套解析器**不保证下标相同**（一个有 `skipped_lines`、一个直接报错）。
//! tick 是两份解析器里**同一个数**（回放信封里的 `"tick"`），所以按 tick 对齐。
//!
//! # 证据等级
//!
//! * 节奏（60/s、`pumps ≡ ticks`、0-tick 帧、追赶无上限、顺序 `TP`）：**观测**（§44）；
//! * tick ↔ 60 Hz：**观测**（`docs/35` §2，113 个 `TickReply` 样本）；
//! * 局面累加规则：**推断**（口径 = `tools/replay_state.py`，`docs/133` §3/§5 三端对拍）；
//! * **回放界面自己长什么样、按什么节奏播**：**未知**（`docs/78` 只给出场景名；
//!   `docs/133` §4 明说"播放控件是我们自定的"）⇒ 本文件是**我方自定**的驱动，
//!   只是**节奏**这一层借了 §44 的真值。

use std::path::Path;

use crate::graphwar_replay_player::{Replay, ReplayError, ReplayPlayer, TICKS_PER_SECOND};

/// 定步周期（纳秒）。`1e9 / 60 = 16 666 666.67`，取整数部分。
///
/// 用整数纳秒而不是 `1.0/60.0` 的 f64 累加：后者在 70 秒上会攒出可见的漂移
/// （§44.4 实测"70 秒零漂移"，原版是纳秒判决点 `cmp edx,0x3b9aca00`）。
/// 取整带来的速率是 `1e9 / 16 666 666 = 60.0000024 /s`，即每分钟多 0.00015 个 tick。
pub const TICK_PERIOD_NS: u64 = 16_666_666;

/// 一帧最多认多长的 `dt`（纳秒）。**不是节奏上限**，只是防 f64 溢出/恶意入参：
/// 3600 s = 216 000 个 tick，已经远超 §44.4 那条"1.5 s 冻结 ⇒ 91 圈"的实测范围。
pub const MAX_FRAME_NS: u64 = 3_600_000_000_000;

/// 一个**结算批**（`CalculatingResults` → `ExecutingResults` + 延迟段），按 tick 表达。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementBatch {
    /// 批下标（0 起，从文件头的方向数）。
    pub index: usize,
    /// `GameStateInfo{CalculatingResults}` 的 tick（窗口首条）。
    pub start_tick: i64,
    /// `GameStateInfo{ExecutingResults}` 的 tick（窗口末条 = **结算落地的那一拍**）。
    pub exec_tick: i64,
    /// 下一批 `CalculatingResults` 的 tick（= 延迟段的右端点）；没有下一批时是
    /// 最后一条事件的 tick。**注意它可能等于** `exec_tick`（回放末尾没有延迟段）。
    pub next_start_tick: i64,
    /// 批内 `FunctionPoints` 的条数（= 这一批几个人开火）。
    pub shots: usize,
}

/// 一次"泵"产出的报告（给界面用；**只报事实，不替界面决定画什么之外的东西**）。
///
/// 键名与旧 GDScript `_apply_replay_events()` 的入参意义一一对应：
/// 旧实现自己遍历事件数组判 `kind`，现在那段分派在 Rust（`docs/16` §56.4）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PumpReport {
    /// 本次调用算几帧（`drive_frame` = 1；`drive_ticks` = 0）。
    pub frames: u32,
    /// 本次推进的 tick 数。
    pub ticks: u32,
    /// 本次的泵次数。**恒等于 `ticks`**（泵在 tick 循环里）。
    pub pumps: u32,
    /// 本次派发的事件条数。
    pub events: usize,
    /// 本次派发的事件名（**文件顺序**，逐条）。自检拿它与独立扫描比对消息序列。
    pub kinds: Vec<String>,
    /// 这一泵里出现了"要重摆实体"的事件（`PosInfo` / `LifeInfo` / `TargetInfo` / …）。
    pub dirty_actors: bool,
    /// 这一泵里出现了"要重铺障碍"的事件（`AddObstacle` / `ClearObstacles` / `DestroyObstacle`）。
    pub dirty_layout: bool,
    /// 这一泵里出现了 `FunctionPoints`（要重画弹道）。
    pub dirty_shot: bool,
    /// 本次跨过的结算批下标（升序；长帧一次跨多批时全在里面，**不跳批**）。
    pub settled: Vec<usize>,
    /// 泵完之后的 tick。
    pub tick: i64,
    /// 泵完之后的游标（已消费事件数）。
    pub cursor: usize,
    /// 泵完之后**已经结算过**的批数。
    pub batches_settled: usize,
}

/// 驱动器的累计计数（`docs/06` §44.3 的三条读数在我们这一侧的同名对应物）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriverStats {
    /// 驱动过的**帧**数（`drive_frame` 被调用且正在播放）。
    pub frames: u64,
    /// 推进过的 tick 数。
    pub ticks: u64,
    /// 泵次数（结构性恒等于 `ticks`）。
    pub pumps: u64,
    /// 一个 tick 都没推的帧数（`§44.3` 的 `0-tick frames`）。
    pub zero_tick_frames: u64,
}

/// 按 tick 驱动的回放播放器。
///
/// ```text
/// let mut d = ReplayDriver::load(path)?;        // 加载 = 内核解析 + 批切分
/// d.play();
/// let r = d.drive_frame(1.0 / 60.0);            // 一帧 ⇒ 1 个 tick ⇒ 1 次泵
/// assert_eq!(r.ticks, r.pumps);                 // 结构性成立
/// let r = d.drive_ticks(600);                   // 确定性原语：直接走 600 个 tick
/// r.settled                                     // 这 600 个 tick 跨过了哪些结算批
/// ```
#[derive(Debug, Clone)]
pub struct ReplayDriver {
    player: ReplayPlayer,
    /// 结算批（按 `start_tick` 升序；由 `replaydiff::split_batches` 换 tick 而来）。
    batches: Vec<SettlementBatch>,
    /// 批切分失败/为空时的原因（空串 = 切成功）。**不静默**。
    batch_note: String,
    /// 已经结算过的批数（= 下一个待结算批的下标）。
    batch_cursor: usize,
    /// 定步累加器的余数（纳秒）。**余数必须留**，否则 1× 每帧慢一个 tick。
    accum_ns: u64,
    stats: DriverStats,
}

impl ReplayDriver {
    /// 从原始字节加载（自动识别 gzip / 明文，与内核播放器同一条路）。
    ///
    /// 解析**一次**，两个消费者共用：内核 [`Replay`]（局面）+ `replaydiff::Replay`（批切分）。
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReplayError> {
        let replay = Replay::parse_bytes(bytes)?;
        Ok(Self::with_replay(replay, bytes))
    }

    /// 从文件加载（`user://replays/<时间戳>` 在 Godot 里就是普通路径）。
    pub fn load(path: &Path) -> Result<Self, ReplayError> {
        let bytes = std::fs::read(path).map_err(|_| ReplayError::Io)?;
        Self::from_bytes(&bytes)
    }

    /// 用已经解析好的回放建驱动器（`bytes` 只用来切结算批）。
    pub fn with_replay(replay: Replay, bytes: &[u8]) -> Self {
        let first = replay.tick_range().0;
        let (batches, batch_note) = split_settlement_batches(bytes);
        let mut d = ReplayDriver {
            player: ReplayPlayer::new(replay),
            batches,
            batch_note,
            batch_cursor: 0,
            accum_ns: 0,
            stats: DriverStats::default(),
        };
        // 进局 = 消费到"第一条事件的 tick"，与旧 `.gd` 的 `seek(first_tick)` 同一条路
        // （`ReplayPlayer::new` 只把时钟放到 first_tick，一条事件都没应用）。
        d.player.seek(first);
        d.resync_batches();
        d
    }

    // ---------------------------------------------------------------- 查询

    pub fn player(&self) -> &ReplayPlayer {
        &self.player
    }

    pub fn first_tick(&self) -> i64 {
        self.player.first_tick()
    }

    pub fn last_tick(&self) -> i64 {
        self.player.last_tick()
    }

    /// 当前 tick（整 tick；浮点余数只给 `progress()` 用）。
    pub fn tick(&self) -> i64 {
        self.player.current_tick()
    }

    /// 已消费事件数（游标）。
    pub fn cursor(&self) -> usize {
        self.player.cursor
    }

    /// 事件总条数。
    pub fn event_count(&self) -> usize {
        self.player.replay.events.len()
    }

    pub fn progress(&self) -> f64 {
        self.player.progress()
    }

    pub fn finished(&self) -> bool {
        self.player.finished()
    }

    pub fn playing(&self) -> bool {
        self.player.playing
    }

    pub fn speed(&self) -> f64 {
        self.player.speed
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.player.speed = if speed.is_finite() { speed.max(0.0) } else { 1.0 };
    }

    pub fn stats(&self) -> DriverStats {
        self.stats
    }

    /// 结算批表（切分失败时是空的，原因见 [`ReplayDriver::batch_note`]）。
    pub fn batches(&self) -> &[SettlementBatch] {
        &self.batches
    }

    /// 批切分不可用的原因（空串 = 可用）。**不把"没切出来"说成"没有批"**。
    pub fn batch_note(&self) -> &str {
        &self.batch_note
    }

    /// 已经结算过的批数。
    pub fn batches_settled(&self) -> usize {
        self.batch_cursor
    }

    /// 当前落在哪个批里（-1 = 第一批开始之前 / 没有批 / 已经过了最后一批）。
    pub fn current_batch(&self) -> i64 {
        let t = self.tick();
        for b in &self.batches {
            if t >= b.start_tick && t <= b.next_start_tick {
                return b.index as i64;
            }
        }
        -1
    }

    /// 当前相位的名字（诊断 / 状态行用；**不参与驱动**）。
    ///
    /// * `no_batches`：这份回放切不出批（见 [`ReplayDriver::batch_note`]）；
    /// * `before`：还没进第一批；
    /// * `calculating`：在 `CalculatingResults…ExecutingResults` 之间；
    /// * `executing`：`ExecutingResults…下一批开始`（回合末延迟段）；
    /// * `after`：过了最后一批。
    pub fn phase(&self) -> &'static str {
        if self.batches.is_empty() {
            return "no_batches";
        }
        let t = self.tick();
        for b in &self.batches {
            if t < b.start_tick {
                return "before";
            }
            if t < b.exec_tick {
                return "calculating";
            }
            if t <= b.next_start_tick {
                return "executing";
            }
        }
        "after"
    }

    /// 规范性局面串（与内核 `ReplayState::canonical` / Python `replay_state.py` 同格式）。
    pub fn canonical(&self) -> String {
        self.player.state.canonical()
    }

    /// 播放状态摘要（状态行用）。
    pub fn summary(&self) -> String {
        format!(
            "tick {}/{} ／ 事件 {}/{} ／ {}",
            self.tick(),
            self.last_tick(),
            self.cursor(),
            self.event_count(),
            if self.playing() { "播放中" } else { "暂停" },
        )
    }

    // ---------------------------------------------------------------- 播放控制

    pub fn play(&mut self) {
        self.player.play();
    }

    pub fn pause(&mut self) {
        self.player.pause();
    }

    pub fn toggle(&mut self) {
        self.player.toggle();
    }

    /// 跳到某个 tick：**从头重放**（保证局面恒等于 `state_at(tick)`，不留增量漂移）。
    /// 跳转**不算结算**：批簿记跟着重算，但不产生 `settled` 报告。
    pub fn seek(&mut self, tick: i64) {
        self.player.seek(tick);
        self.accum_ns = 0;
        self.resync_batches();
    }

    /// 「下一个事件」：跳到下一条尚未消费事件的 tick。
    pub fn step_event(&mut self) -> Option<i64> {
        let t = self.player.step_event();
        self.accum_ns = 0;
        self.resync_batches();
        t
    }

    /// 「上一个事件」：退到严格早于当前 tick 的最近一条事件的 tick。
    pub fn step_back_event(&mut self) -> Option<i64> {
        let t = self.player.step_back_event();
        self.accum_ns = 0;
        self.resync_batches();
        t
    }

    // ---------------------------------------------------------------- 驱动

    /// **按帧**推进（`dt` 当墙钟的替身，见文件头"两处诚实的偏离"）。
    ///
    /// 一帧推 `n = floor((余数 + dt × speed) / 16.667 ms)` 个 tick，余数留下；
    /// 每个 tick 里面泵一次 ⇒ `pumps ≡ ticks`；`n = 0` 的帧记一次 `zero_tick_frames`
    /// 并且**一次都不泵**（§44.3 的真值：默认帧率下这种帧占 10.9%）。
    pub fn drive_frame(&mut self, dt_seconds: f64) -> PumpReport {
        let mut report = PumpReport::default();
        if !self.player.playing {
            self.fill_tail(&mut report);
            return report;
        }
        let speed = self.player.speed;
        let dt = if dt_seconds.is_finite() { dt_seconds.max(0.0) * speed.max(0.0) } else { 0.0 };
        let ns = ((dt * 1e9).round() as u64).min(MAX_FRAME_NS);
        self.stats.frames += 1;
        report.frames = 1;
        self.accum_ns = self.accum_ns.saturating_add(ns);
        let mut n = self.accum_ns / TICK_PERIOD_NS;
        self.accum_ns %= TICK_PERIOD_NS;
        if n == 0 {
            self.stats.zero_tick_frames += 1;
        }
        // 到结尾就停：多余的时间不攒着（`finished()` 之后 `play()` 会从头放）
        let remain = (self.last_tick() - self.tick()).max(0) as u64;
        if n > remain {
            n = remain;
            self.accum_ns = 0;
        }
        self.drive_ticks_inner(n, &mut report);
        if self.tick() >= self.last_tick() {
            self.player.pause();
        }
        self.fill_tail(&mut report);
        report
    }

    /// **按 tick** 推进（确定性原语：不给时间、只给 tick 数）。
    ///
    /// 这是 headless 断言用的入口，也是"驱动到底按什么节奏"这件事的**唯一**定义：
    /// 每圈先 tick 再泵。`speed` 不参与（它是时间轴的倍率，不是 tick 数的倍率）。
    pub fn drive_ticks(&mut self, count: u64) -> PumpReport {
        let mut report = PumpReport::default();
        if !self.player.playing {
            self.fill_tail(&mut report);
            return report;
        }
        let remain = (self.last_tick() - self.tick()).max(0) as u64;
        let n = count.min(remain);
        self.drive_ticks_inner(n, &mut report);
        if self.tick() >= self.last_tick() {
            self.player.pause();
        }
        self.fill_tail(&mut report);
        report
    }

    /// 循环体：`for _ in 0..n { tick(); pump(); }`。
    ///
    /// `0x518499 call r13`（tick）→ `0x51849f call rbp`（泵），**每圈一对**（§44.3）。
    /// 内核的 [`ReplayPlayer::advance_to`] 把"设时钟"和"派发事件"合成一次调用，
    /// 这里仍把两者**分开计数**，因为 §44.3 的 `pumps ≡ ticks` 是两条独立的调用计数，
    /// 我们要能用同一个口径自查（`stats().pumps == stats().ticks`）。
    fn drive_ticks_inner(&mut self, n: u64, report: &mut PumpReport) {
        for _ in 0..n {
            let target = self.tick() + 1;
            // ① `0x518499 call r13` —— tick：只推进时钟
            self.stats.ticks += 1;
            report.ticks = report.ticks.saturating_add(1);
            // ② `0x51849f call rbp` —— 泵：把 `事件.tick <= 当前 tick` 的按文件顺序派发
            self.stats.pumps += 1;
            report.pumps = report.pumps.saturating_add(1);
            let applied = self.player.advance_to(target);
            for i in applied {
                let kind = self.player.replay.events[i].kind.clone();
                self.mark_dirty(&kind, report);
                report.kinds.push(kind);
                report.events += 1;
            }
            // ③ 结算批：这一拍是否跨过了某一批的 `ExecutingResults`
            self.settle_due(report);
        }
    }

    /// `kind` → 重画哪一层。**表照抄旧 `.gd` 的 `_apply_replay_events()` 分派**
    /// （`docs/16` §56.4：这不是"游戏状态 → 做什么"，是"事件类型 → 视图动作"）。
    fn mark_dirty(&self, kind: &str, report: &mut PumpReport) {
        match kind {
            "PosInfo" | "TeamInfo" | "LifeInfo" | "SoldierInfo" | "EntityRemoved"
            | "SkinGraphicsInfo" | "FaceGraphicsInfo" | "HatGraphicsInfo" | "TargetInfo"
            | "ColorInfo" => report.dirty_actors = true,
            "AddObstacle" | "ClearObstacles" | "DestroyObstacle" => report.dirty_layout = true,
            "FunctionPoints" => {
                report.dirty_actors = true;
                report.dirty_shot = true;
            }
            _ => {}
        }
    }

    /// 把 `exec_tick <= 当前 tick` 的批全部记上（长帧一次跨多批时**按序全记**）。
    fn settle_due(&mut self, report: &mut PumpReport) {
        let t = self.tick();
        while self.batch_cursor < self.batches.len()
            && self.batches[self.batch_cursor].exec_tick <= t
        {
            report.settled.push(self.batch_cursor);
            self.batch_cursor += 1;
        }
    }

    /// 把"当前状态"填进报告尾部（每条返回路径都要填，免得界面读到旧值）。
    fn fill_tail(&self, report: &mut PumpReport) {
        report.tick = self.tick();
        report.cursor = self.cursor();
        report.batches_settled = self.batch_cursor;
    }

    /// 跳转之后重算批簿记：`exec_tick <= 当前 tick` 的批**直接算已结算**，
    /// 但不写进任何报告（跳转不是"驱动"，不该产生结算通知）。
    fn resync_batches(&mut self) {
        let t = self.tick();
        self.batch_cursor = self.batches.iter().filter(|b| b.exec_tick <= t).count();
    }
}

/// 从回放字节切出结算批（**借用** `replaydiff::split_batches`，不重写规则）。
fn split_settlement_batches(bytes: &[u8]) -> (Vec<SettlementBatch>, String) {
    use crate::_scaffold::replaydiff;
    let parsed = match replaydiff::Replay::from_bytes(bytes) {
        Ok(r) => r,
        // 客户端**不**因为"验证用的解析器不认"就拒绝播放：批切分不可用而已。
        Err(e) => return (Vec::new(), format!("批切分不可用（replaydiff 解析失败：{e}）")),
    };
    let windows = replaydiff::split_batches(&parsed);
    if windows.is_empty() {
        return (
            Vec::new(),
            "这份回放里没有结算批（没有 CalculatingResults→ExecutingResults 窗口）".to_string(),
        );
    }
    let last_tick = parsed.msgs.last().map(|m| m.tick).unwrap_or(0);
    let mut out = Vec::with_capacity(windows.len());
    for (i, w) in windows.iter().enumerate() {
        let start_tick = parsed.msgs[w.start].tick;
        let exec_tick = parsed.msgs[w.end].tick;
        let next_start_tick = parsed.msgs.get(w.tail_end).map(|m| m.tick).unwrap_or(last_tick);
        out.push(SettlementBatch {
            index: i,
            start_tick,
            exec_tick,
            next_start_tick: next_start_tick.max(exec_tick),
            shots: w.shots.len(),
        });
    }
    (out, String::new())
}

/// `TICKS_PER_SECOND` 的公开别名（界面/自检要按 60 Hz 算秒数）。
pub const TICKS_PER_SECOND_I64: i64 = TICKS_PER_SECOND;

// ===========================================================================
// 断言（纯逻辑；不依赖任何外部文件 —— 语料是 `.agent-work/` 里的易失物）
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一份**合成回放**：3 个结算批，批内 1 发，批后带延迟段的死亡。
    ///
    /// 形状照 401 份语料里的真实形状（`docs/136` §2）：
    /// `CalculatingResults` → `FunctionInfo` / `FunctionPoints` → `ExecutingResults`
    /// → `LifeInfo`（死亡，落在延迟段）。
    fn synthetic() -> String {
        let mut s = String::new();
        let mut push = |tick: i64, ev: &str| {
            s.push_str(&format!("{{\"event\":{},\"tick\":{}}}\n", ev, tick));
        };
        push(1, r#"{"GameNameInfo":{"game_name":"T"}}"#);
        push(1, r#"{"SoldierInfo":{"soldier_id":11,"player_id":1}}"#);
        push(2, r#"{"PosInfo":{"entity_id":11,"pos_x":-8.0,"pos_y":0.0,"radius":0.5}}"#);
        push(2, r#"{"TeamInfo":{"entity_id":11,"team":"Team1"}}"#);
        push(2, r#"{"PosInfo":{"entity_id":12,"pos_x":8.0,"pos_y":0.0,"radius":0.5}}"#);
        push(2, r#"{"TeamInfo":{"entity_id":12,"team":"Team2"}}"#);
        // 批 0：10 → 20
        push(10, r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#);
        push(11, r#"{"FunctionInfo":{"owner_id":11,"function":"x","corrected_function":null}}"#);
        push(12, r#"{"FunctionPoints":{"owner_id":11,"start_x":-8.0,"start_y":0.0,"end_x":8.0}}"#);
        push(20, r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#);
        push(30, r#"{"LifeInfo":{"entity_id":12,"alive":false}}"#);
        // 批 1：60 → 70
        push(60, r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#);
        push(62, r#"{"FunctionPoints":{"owner_id":12,"start_x":8.0,"start_y":0.0,"end_x":-8.0}}"#);
        push(70, r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#);
        push(80, r#"{"LifeInfo":{"entity_id":11,"alive":false}}"#);
        // 批 2：100 → 110
        push(100, r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#);
        push(102, r#"{"FunctionPoints":{"owner_id":11,"start_x":-8.0,"start_y":0.0,"end_x":8.0}}"#);
        push(110, r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#);
        push(120, r#"{"GameStateInfo":{"game_state":"EndingGame"}}"#);
        s
    }

    fn driver() -> ReplayDriver {
        ReplayDriver::from_bytes(synthetic().as_bytes()).expect("合成回放要能解析")
    }

    #[test]
    fn loads_and_starts_at_first_tick() {
        let d = driver();
        assert_eq!(d.first_tick(), 1);
        assert_eq!(d.last_tick(), 120);
        assert_eq!(d.tick(), 1);
        // seek(first_tick) 之后 tick-1 的事件已经消费掉（与旧 `.gd` 的 seek 同一条路）
        assert_eq!(d.cursor(), 2);
        assert_eq!(d.event_count(), 19);
    }

    #[test]
    fn batches_come_from_split_batches() {
        let d = driver();
        assert_eq!(d.batch_note(), "");
        let b = d.batches();
        assert_eq!(b.len(), 3);
        assert_eq!(
            b.iter().map(|x| (x.start_tick, x.exec_tick, x.next_start_tick)).collect::<Vec<_>>(),
            vec![(10, 20, 60), (60, 70, 100), (100, 110, 120)]
        );
        assert_eq!(b.iter().map(|x| x.shots).collect::<Vec<_>>(), vec![1, 1, 1]);
    }

    #[test]
    fn pumps_equal_ticks_and_zero_tick_frames_pump_nothing() {
        let mut d = driver();
        d.play();
        // 120 帧 × 1/120 s = 60 个 tick，其中 60 帧一个 tick 都没推
        let mut ticks = 0u32;
        let mut pumps = 0u32;
        for _ in 0..120 {
            let r = d.drive_frame(1.0 / 120.0);
            assert_eq!(r.ticks, r.pumps, "泵必须跟着 tick 走");
            ticks += r.ticks;
            pumps += r.pumps;
        }
        assert_eq!(ticks, 60);
        assert_eq!(pumps, 60);
        let st = d.stats();
        assert_eq!(st.frames, 120);
        assert_eq!(st.ticks, 60);
        assert_eq!(st.pumps, 60);
        // 1/120 s = 8.333 ms，两帧才攒满 16.667 ms ⇒ 一半的帧是 0-tick 帧
        assert_eq!(st.zero_tick_frames, 60, "§44.3：0-tick 帧一次都不泵");
    }

    #[test]
    fn sixty_hz_frames_give_one_tick_each() {
        let mut d = driver();
        d.play();
        for _ in 0..60 {
            let r = d.drive_frame(1.0 / 60.0);
            assert_eq!(r.ticks, 1, "60 fps ⇒ 每帧 1 个 tick");
            assert_eq!(r.pumps, 1);
        }
        assert_eq!(d.stats().zero_tick_frames, 0);
        assert_eq!(d.tick(), 61);
    }

    #[test]
    fn long_frame_catches_up_without_cap() {
        // §44.4：SIGSTOP 1500 ms ⇒ 单帧 91 圈，**没有上限**。
        let mut d = driver();
        d.play();
        let r = d.drive_frame(1.5);
        assert_eq!(r.ticks, 90, "1.5 s / 16.667 ms = 90 圈（§44.4 实测 91，差的是累加器余数）");
        assert_eq!(r.pumps, 90);
        // 旧 `.gd` 的 `advance_dt` 把 dt 夹到 1.0 s（= 60 tick）⇒ 这一条它会挂
        assert!(r.ticks > 60, "旧实现夹到 1 s；我们不许夹");
    }

    #[test]
    fn delta_is_used_only_as_a_wall_clock_stand_in() {
        // §44.5：原版**不读** delta。我们读它，是因为 headless 没有墙钟（文件头偏离 1）。
        let mut d = driver();
        d.play();
        let a = d.drive_frame(0.2).ticks;
        let mut e = driver();
        e.play();
        let b = e.drive_frame(0.2).ticks;
        assert_eq!(a, b, "同一段 dt ⇒ 同一批 tick（确定性）");
        assert_eq!(a, 12, "200 ms = 12 个 16.667 ms");
    }

    #[test]
    fn drive_ticks_is_the_deterministic_primitive() {
        let mut d = driver();
        d.play();
        let r = d.drive_ticks(9); // 1 → 10
        assert_eq!(r.ticks, 9);
        assert_eq!(r.pumps, 9);
        assert_eq!(d.tick(), 10);
        assert_eq!(r.frames, 0, "按 tick 推进不记帧数");
        assert!(r.kinds.iter().any(|k| k == "GameStateInfo"));
        assert!(r.settled.is_empty(), "还没到 20 那一拍");
        assert_eq!(d.phase(), "calculating");
    }

    #[test]
    fn settlement_fires_exactly_on_the_executing_tick() {
        let mut d = driver();
        d.play();
        let r = d.drive_ticks(18); // → tick 19
        assert_eq!(d.tick(), 19);
        assert!(r.settled.is_empty(), "tick 19 还没结算");
        let r = d.drive_ticks(1); // → tick 20
        assert_eq!(r.settled, vec![0], "tick 20 = ExecutingResults ⇒ 第 0 批结算");
        assert_eq!(r.batches_settled, 1);
        assert_eq!(d.phase(), "executing");
    }

    #[test]
    fn long_frame_settles_every_batch_it_crosses_in_order() {
        // 一帧跨三批（§44.4 的"长帧追赶"在结算侧的样子）：**按序全记，不跳批**。
        let mut d = driver();
        d.play();
        let r = d.drive_ticks(109); // 1 → 110
        assert_eq!(r.settled, vec![0, 1, 2]);
        assert_eq!(r.batches_settled, 3);
        assert_eq!(d.phase(), "executing");
    }

    #[test]
    fn tick_is_monotonic_while_driving() {
        let mut d = driver();
        d.play();
        let mut last = d.tick();
        for _ in 0..30 {
            d.drive_frame(0.07);
            let t = d.tick();
            assert!(t >= last, "驱动过程中 tick 不许回退：{last} → {t}");
            last = t;
        }
    }

    #[test]
    fn seek_does_not_report_settlements() {
        let mut d = driver();
        d.seek(105);
        assert_eq!(d.tick(), 105);
        assert_eq!(d.batches_settled(), 2, "跳过去的两批直接算已结算");
        let r = d.drive_ticks(0);
        assert!(r.settled.is_empty(), "跳转不产生结算通知");
        d.play();
        let r = d.drive_ticks(5); // 105 → 110
        assert_eq!(r.settled, vec![2]);
    }

    #[test]
    fn paused_driver_drives_nothing() {
        let mut d = driver();
        let r = d.drive_frame(1.0);
        assert_eq!((r.ticks, r.pumps, r.events), (0, 0, 0));
        assert_eq!(d.stats().frames, 0, "暂停的帧不算驱动帧");
        d.play();
        d.pause();
        assert_eq!(d.drive_ticks(10).ticks, 0);
    }

    #[test]
    fn stops_at_last_tick_and_clears_playing() {
        let mut d = driver();
        d.play();
        let r = d.drive_ticks(1000);
        assert_eq!(d.tick(), 120);
        assert_eq!(r.ticks, 119, "1 → 120 只有 119 个 tick");
        assert!(!d.playing(), "到结尾自动停");
        assert!(d.finished());
    }

    #[test]
    fn dirty_flags_follow_the_old_gd_dispatch_table() {
        let mut d = driver();
        d.play();
        // 2 → 10：这一段里有 PosInfo/TeamInfo（要重摆实体）
        let r = d.drive_ticks(8);
        assert!(r.dirty_actors);
        assert!(!r.dirty_layout);
        assert!(!r.dirty_shot);
        // 30 那一拍是 LifeInfo（死亡）⇒ 只重摆实体
        let r = d.drive_ticks(21); // 9 → 30
        assert!(r.kinds.iter().any(|k| k == "LifeInfo"));
        assert!(r.dirty_actors);
        // 12 那一拍有 FunctionPoints ⇒ 重摆 + 重画弹道
        let mut e = driver();
        e.play();
        let r = e.drive_ticks(11); // → 12
        assert!(r.dirty_shot, "FunctionPoints ⇒ 要重画弹道");
    }

    #[test]
    fn canonical_matches_the_kernel_state_at_the_same_tick() {
        let mut d = driver();
        d.play();
        d.drive_ticks(19); // → 20
        let want = d.player().replay.state_at(20).canonical();
        assert_eq!(d.canonical(), want);
        assert!(d.canonical().starts_with("tick=20|"));
    }

    #[test]
    fn replay_without_batches_still_plays() {
        // 没有 CalculatingResults→ExecutingResults 的回放（畸形/极短）：不拒绝播放，
        // 只把原因写进 `batch_note()`，结算批为空。**不静默**。
        let text = "{\"event\":{\"GameNameInfo\":{\"game_name\":\"X\"}},\"tick\":1}\n\
                    {\"event\":{\"GameStateInfo\":{\"game_state\":\"EndingGame\"}},\"tick\":5}\n";
        let mut d = ReplayDriver::from_bytes(text.as_bytes()).expect("要能解析");
        assert!(d.batches().is_empty());
        assert!(d.batch_note().contains("没有结算批"), "原因：{}", d.batch_note());
        d.play();
        assert_eq!(d.drive_ticks(4).ticks, 4);
        assert_eq!(d.phase(), "no_batches");
    }

    /// 一份**够长**的合成回放（跑 60 s 墙钟用：`drive` 到结尾就停了）。
    fn long_synthetic() -> String {
        "{\"event\":{\"GameNameInfo\":{\"game_name\":\"L\"}},\"tick\":1}\n\
         {\"event\":{\"GameStateInfo\":{\"game_state\":\"EndingGame\"}},\"tick\":100000}\n"
            .to_string()
    }

    #[test]
    fn period_gives_sixty_ticks_per_second() {
        // 60 s 墙钟 ⇒ 3600 个 tick（±1），与 §44.2 的 59.94–60.01 /s 同口径
        let mut d = ReplayDriver::from_bytes(long_synthetic().as_bytes()).unwrap();
        d.play();
        let mut ticks = 0u64;
        for _ in 0..3600 {
            ticks += d.drive_frame(1.0 / 60.0).ticks as u64;
        }
        assert!((3599..=3600).contains(&ticks), "60 s 推了 {ticks} 个 tick");
    }
}
