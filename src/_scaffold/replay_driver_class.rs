//! **不是原版的东西**：把 [`super::replay_driver::ReplayDriver`] 挂成 GDExtension 类。
//!
//! 类名 `GwReplayDriver`，替掉播放屏里原来那份 **GDScript** 驱动
//! （`client/src/play/replay_player.gd::advance_dt` 的每帧模型 +
//! `play_screen.gd::_replay_tick/_apply_replay_events`）。
//!
//! # 这里没有第二份逻辑
//!
//! | 方法 | 落到的纯逻辑 | 档位 |
//! |---|---|---|
//! | `load()` / `advance_dt()` / `advance_ticks()` | [`ReplayDriver`] 的定步累加器 + 泵 | 节奏 **观测**（`docs/06` §44）；界面时序 **我方自定** |
//! | `canonical()` / `actors()` / `snapshot()` | [`crate::graphwar_replay_player::ReplayState`]（原版模块） | 局面规则 **推断**（`docs/133` §3/§5 三端对拍） |
//! | `batches()` | [`crate::_scaffold::replaydiff::split_batches`] | **观测**（401 份冻结语料，`docs/136`） |
//!
//! # 与旧 `.gd` 的接口对应（播放屏的调用点几乎不用改）
//!
//! ```text
//! 旧： var p := GwReplayPlayer.new().setup(f)   新： var d := GwReplayDriver.new(); d.load(path)
//! 旧： p.advance_dt(dt) -> Array[事件]           新： d.advance_dt(dt) -> {ticks, pumps, kinds, dirty_*, settled}
//! 旧： p.actors() / obstacles_for_view()         新： d.actors() / d.obstacles()
//! 旧： p.state.game_name / turn_count / …        新： d.snapshot()["game_name"] / …
//! 旧： p.canonical() / summary() / progress()    新： 同名
//! ```
//!
//! ⚠️ **两个刻意的差异**（播放屏与自检都要知道）：
//!
//! 1. `advance_dt()` 的返回值从"事件数组"变成**报告字典**：旧实现把事件数组交给
//!    GDScript、由它按 `kind` 决定重画哪层；现在那段分派在 Rust（`dirty_*`），
//!    GDScript 只读三个布尔。`kinds` 仍逐条给出来（自检要拿它跟独立扫描比对）。
//! 2. 旧 `advance_dt()` 把 `dt` **夹到 1.0 s**（`min(dt, 1.0)`）；这里**不夹**
//!    （§44.4：原版长帧追赶无上限，1.5 s 冻结 ⇒ 单帧 91 圈）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_replay_player::ReplayError;

use super::replay_driver::{PumpReport, ReplayDriver};

/// 报告 → GDScript 字典（键名与 `PumpReport` 的字段一一对应）。
fn report_dict(r: &PumpReport) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("frames", r.frames as i64);
    d.set("ticks", r.ticks as i64);
    d.set("pumps", r.pumps as i64);
    d.set("events", r.events as i64);
    let mut kinds = VarArray::new();
    for k in &r.kinds {
        let g = GString::from(k.as_str());
        kinds.push(&g);
    }
    d.set("kinds", &kinds);
    d.set("dirty_actors", r.dirty_actors);
    d.set("dirty_layout", r.dirty_layout);
    d.set("dirty_shot", r.dirty_shot);
    let mut settled = VarArray::new();
    for i in &r.settled {
        settled.push(*i as i64);
    }
    d.set("settled", &settled);
    d.set("tick", r.tick);
    d.set("cursor", r.cursor as i64);
    d.set("batches_settled", r.batches_settled as i64);
    d
}

fn v2(x: f64, y: f64) -> Vector2 {
    Vector2::new(x as f32, y as f32)
}

/// 按 tick 驱动的回放播放器（`ReplayDriver` 的 GDExtension 外壳）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwReplayDriver {
    base: Base<RefCounted>,
    /// `None` = 还没加载（或加载失败，原因在 `error`）。
    driver: Option<ReplayDriver>,
    error: String,
}

#[godot_api]
impl IRefCounted for GwReplayDriver {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base, driver: None, error: String::from("还没加载回放") }
    }
}

#[godot_api]
impl GwReplayDriver {
    // ---------------------------------------------------------------- 加载

    /// 读一个回放文件（`user://` / `res://` 请先用
    /// `ProjectSettings.globalize_path()` 换成真路径 —— 本类不解释 Godot 的路径方案）。
    ///
    /// 返回 `true` = 可以播；`false` 时原因读 [`Self::error`]（**不抛异常**）。
    #[func]
    fn load(&mut self, path: GString) -> bool {
        let p = std::path::PathBuf::from(path.to_string());
        match ReplayDriver::load(&p) {
            Ok(d) => {
                self.driver = Some(d);
                self.error = String::new();
                true
            }
            Err(e) => {
                self.driver = None;
                self.error = format!("{}: {}", e.as_str(), p.display());
                false
            }
        }
    }

    /// 直接吃一段**明文 JSONL**（自检用；等价于 `load()` 的明文分支）。
    #[func]
    fn load_text(&mut self, text: GString) -> bool {
        match ReplayDriver::from_bytes(text.to_string().as_bytes()) {
            Ok(d) => {
                self.driver = Some(d);
                self.error = String::new();
                true
            }
            Err(e) => {
                self.driver = None;
                self.error = String::from(e.as_str());
                false
            }
        }
    }

    #[func]
    fn ok(&self) -> bool {
        self.driver.is_some()
    }

    /// 加载失败的原因（空串 = 成功）。名字与旧 `GwReplayFile.error` 一致。
    #[func]
    fn error(&self) -> GString {
        GString::from(self.error.as_str())
    }

    // ---------------------------------------------------------------- 查询

    #[func]
    fn first_tick(&self) -> i64 {
        self.driver.as_ref().map(|d| d.first_tick()).unwrap_or(0)
    }

    #[func]
    fn last_tick(&self) -> i64 {
        self.driver.as_ref().map(|d| d.last_tick()).unwrap_or(0)
    }

    #[func]
    fn tick(&self) -> i64 {
        self.driver.as_ref().map(|d| d.tick()).unwrap_or(0)
    }

    #[func]
    fn cursor(&self) -> i64 {
        self.driver.as_ref().map(|d| d.cursor() as i64).unwrap_or(0)
    }

    #[func]
    fn event_count(&self) -> i64 {
        self.driver.as_ref().map(|d| d.event_count() as i64).unwrap_or(0)
    }

    #[func]
    fn progress(&self) -> f64 {
        self.driver.as_ref().map(|d| d.progress()).unwrap_or(0.0)
    }

    #[func]
    fn finished(&self) -> bool {
        self.driver.as_ref().map(|d| d.finished()).unwrap_or(false)
    }

    #[func]
    fn playing(&self) -> bool {
        self.driver.as_ref().map(|d| d.playing()).unwrap_or(false)
    }

    #[func]
    fn speed(&self) -> f64 {
        self.driver.as_ref().map(|d| d.speed()).unwrap_or(1.0)
    }

    #[func]
    fn set_speed(&mut self, speed: f64) {
        if let Some(d) = self.driver.as_mut() {
            d.set_speed(speed);
        }
    }

    /// 规范性局面串（与 Python `replay_state.py` / 内核 `ReplayState::canonical` 同格式）。
    #[func]
    fn canonical(&self) -> GString {
        let s = self.driver.as_ref().map(|d| d.canonical()).unwrap_or_default();
        GString::from(s.as_str())
    }

    #[func]
    fn summary(&self) -> GString {
        let s = self.driver.as_ref().map(|d| d.summary()).unwrap_or_default();
        GString::from(s.as_str())
    }

    /// 驱动器的累计计数 `{frames, ticks, pumps, zero_tick_frames}`（§44.3 的同名对应物）。
    #[func]
    fn stats(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let st = self.driver.as_ref().map(|x| x.stats()).unwrap_or_default();
        d.set("frames", st.frames as i64);
        d.set("ticks", st.ticks as i64);
        d.set("pumps", st.pumps as i64);
        d.set("zero_tick_frames", st.zero_tick_frames as i64);
        d
    }

    /// 相位名：`before` / `calculating` / `executing` / `after` / `no_batches`。
    #[func]
    fn phase(&self) -> GString {
        GString::from(self.driver.as_ref().map(|d| d.phase()).unwrap_or("no_batches"))
    }

    // ---------------------------------------------------------------- 局面

    /// 局面快照（键 = 旧 `.gd` 的 `state.*`，值是同一批数）。
    #[func]
    fn snapshot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(drv) = self.driver.as_ref() else {
            return d;
        };
        let st = &drv.player().state;
        d.set("tick", drv.tick());
        d.set("first_tick", drv.first_tick());
        d.set("last_tick", drv.last_tick());
        d.set("cursor", drv.cursor() as i64);
        d.set("events", drv.event_count() as i64);
        d.set("playing", drv.playing());
        d.set("progress", drv.progress());
        d.set("finished", drv.finished());
        d.set("speed", drv.speed());
        d.set("game_name", &GString::from(st.game_name.as_str()));
        d.set("game_state", &GString::from(st.game_state.as_str()));
        d.set("turn_count", st.turn_count);
        d.set("turn_shooter", st.shooter.unwrap_or(-1));
        d.set("axis_mode", &GString::from(st.axis_mode.as_str()));
        d.set("function_mode", &GString::from(st.function_mode.as_str()));
        d.set("turn_mode", &GString::from(st.turn_mode.as_str()));
        d.set("time_mode", &GString::from(st.time_mode.as_str()));
        d.set("obstacles", st.obstacles.len() as i64);
        d.set("holes", st.holes.len() as i64);
        d.set("shots", st.shots.len() as i64);
        d.set("effects", st.effects.len() as i64);
        d.set("canonical", &GString::from(drv.canonical().as_str()));
        d.set("summary", &GString::from(drv.summary().as_str()));
        d.set("phase", &GString::from(drv.phase()));
        d.set("batches", drv.batches().len() as i64);
        d.set("batches_settled", drv.batches_settled() as i64);
        d.set("batch_index", drv.current_batch());
        d.set("batch_note", &GString::from(drv.batch_note()));
        d.set("stats", &self.stats());
        d
    }

    /// 有位置、且没被移除的实体（士兵与靶子都在这张表里，`is_target` 区分）。
    ///
    /// 键与旧 `.gd` 的 `actors()` **完全一致**：`id / pos / radius / alive / is_target /
    /// team / team_name / name / player_id / color`。
    #[func]
    fn actors(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(drv) = self.driver.as_ref() else {
            return out;
        };
        let st = &drv.player().state;
        for (id, c, alive, is_target) in st.visible_entities() {
            // 旧 `.gd` 的口径：**没有 `PosInfo` 的画不出来**（`canonical` 里仍保留空三项）。
            if !st.pos.contains_key(&id) {
                continue;
            }
            let pid = st.soldier_owner.get(&id).copied().unwrap_or(-1);
            let name = st.names.get(&pid).cloned().unwrap_or_default();
            let team_name = st.team.get(&id).cloned().unwrap_or_default();
            let color = st.colors.get(&pid).copied();
            let mut a = VarDictionary::new();
            a.set("id", id);
            a.set("pos", v2(c.x, c.y));
            a.set("radius", c.r);
            a.set("alive", alive);
            a.set("is_target", is_target);
            a.set("team", st.team_index(id) as i64);
            a.set("team_name", &GString::from(team_name.as_str()));
            a.set("name", &GString::from(name.as_str()));
            a.set("player_id", pid);
            // 有 `ColorInfo` 才给 "color" 键；没有就**不写这个键**，调用方 `get("color", null)`
            // 拿到 null ⇒ 退回本队色表（与旧 `.gd` 的 `if c is Color` 同一条判据，
            // 只是把"null"表达成"键不存在"，省掉 Godot 的 Variant 空值搬运）。
            if let Some((r, g, b)) = color {
                a.set(
                    "color",
                    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0),
                );
            }
            out.push(&a);
        }
        out
    }

    /// 障碍（`[{pos: Vector2, size: 半径}]`）—— 直接就是 `_set_obstacles()` 的入参形状。
    #[func]
    fn obstacles(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(drv) = self.driver.as_ref() else {
            return out;
        };
        for o in &drv.player().state.obstacles {
            let mut d = VarDictionary::new();
            d.set("pos", v2(o.x, o.y));
            d.set("size", o.r);
            out.push(&d);
        }
        out
    }

    /// 靶子位置（活着且没被移除的）。
    #[func]
    fn target_positions(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(drv) = self.driver.as_ref() else {
            return out;
        };
        let st = &drv.player().state;
        for (id, c, alive, is_target) in st.visible_entities() {
            if is_target && alive && st.pos.contains_key(&id) {
                out.push(v2(c.x, c.y));
            }
        }
        out
    }

    /// 最近一发（`{tick, owner, start, end_x, function, corrected}`）；没有则空字典。
    #[func]
    fn last_shot(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(drv) = self.driver.as_ref() else {
            return d;
        };
        let Some(s) = drv.player().state.shots.last() else {
            return d;
        };
        d.set("tick", s.tick);
        d.set("owner", s.owner);
        d.set("start", v2(s.start.0, s.start.1));
        d.set("end_x", s.end_x);
        d.set("function", &GString::from(s.function.as_str()));
        d.set("corrected", &GString::from(s.corrected.as_str()));
        d
    }

    /// `Team1` → 0、`Team2` → 1、未知 → -1（射击方向由队伍定，`docs/105`）。
    #[func]
    fn team_index(&self, id: i64) -> i64 {
        self.driver
            .as_ref()
            .map(|d| d.player().state.team_index(id) as i64)
            .unwrap_or(-1)
    }

    /// 士兵 → 玩家（`SoldierInfo`）；不知道 → -1。
    #[func]
    fn soldier_owner(&self, soldier_id: i64) -> i64 {
        self.driver
            .as_ref()
            .map(|d| d.player().state.soldier_owner.get(&soldier_id).copied().unwrap_or(-1))
            .unwrap_or(-1)
    }

    /// 这个玩家有没有 `ColorInfo`（没有就该退回本队色表，别把黑当成"没有颜色"）。
    #[func]
    fn has_color(&self, player_id: i64) -> bool {
        self.driver
            .as_ref()
            .map(|d| d.player().state.colors.contains_key(&player_id))
            .unwrap_or(false)
    }

    /// 玩家颜色（`ColorInfo`）。不知道时返回**黑色**；先问 [`Self::has_color`]。
    #[func]
    fn color_of(&self, player_id: i64) -> Color {
        match self.driver.as_ref().and_then(|d| d.player().state.colors.get(&player_id).copied()) {
            Some((r, g, b)) => Color::from_rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                1.0,
            ),
            None => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
        }
    }

    /// 皮肤/脸/帽子名（`slot` = `skin` / `face` / `hat`）；没有 → 空串。
    #[func]
    fn graphics(&self, slot: GString, entity_id: i64) -> GString {
        let key = format!("{}:{}", slot.to_string(), entity_id);
        let v = self
            .driver
            .as_ref()
            .and_then(|d| d.player().state.graphics.get(&key).cloned())
            .unwrap_or_default();
        GString::from(v.as_str())
    }

    /// 整张函数表 `{owner(soldier_id): 文本}`（`FunctionInfo`，`docs/79` §1）。
    ///
    /// 播放屏的"查看函数"列表在回放模式下读这张表（它不在 `state.players` 里）。
    #[func]
    fn functions(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(drv) = self.driver.as_ref() else {
            return d;
        };
        for (owner, text) in &drv.player().state.functions {
            d.set(*owner, &GString::from(text.as_str()));
        }
        d
    }

    /// 某人最近一次非空函数文本。
    #[func]
    fn function_of(&self, owner: i64) -> GString {
        let v = self
            .driver
            .as_ref()
            .and_then(|d| d.player().state.functions.get(&owner).cloned())
            .unwrap_or_default();
        GString::from(v.as_str())
    }

    // ---------------------------------------------------------------- 结算批

    /// 全部结算批：`[{index, start_tick, exec_tick, next_start_tick, shots}]`。
    ///
    /// 自检拿它与**自己**扫回放文本算出来的边界比 —— 两份实现独立，比的就是这件事。
    #[func]
    fn batches(&self) -> VarArray {
        let mut out = VarArray::new();
        let Some(drv) = self.driver.as_ref() else {
            return out;
        };
        for b in drv.batches() {
            let mut d = VarDictionary::new();
            d.set("index", b.index as i64);
            d.set("start_tick", b.start_tick);
            d.set("exec_tick", b.exec_tick);
            d.set("next_start_tick", b.next_start_tick);
            d.set("shots", b.shots as i64);
            out.push(&d);
        }
        out
    }

    #[func]
    fn batch_count(&self) -> i64 {
        self.driver.as_ref().map(|d| d.batches().len() as i64).unwrap_or(0)
    }

    #[func]
    fn batches_settled(&self) -> i64 {
        self.driver.as_ref().map(|d| d.batches_settled() as i64).unwrap_or(0)
    }

    /// 批切分不可用的原因（空串 = 可用）。**没切出来 ≠ 没有批**。
    #[func]
    fn batch_note(&self) -> GString {
        GString::from(self.driver.as_ref().map(|d| d.batch_note()).unwrap_or(""))
    }

    // ---------------------------------------------------------------- 驱动

    /// 按帧推进（`dt` 当墙钟的替身）。返回报告字典，见文件头。
    #[func]
    fn advance_dt(&mut self, dt: f64) -> VarDictionary {
        match self.driver.as_mut() {
            Some(d) => report_dict(&d.drive_frame(dt)),
            None => report_dict(&PumpReport::default()),
        }
    }

    /// **按 tick** 推进（确定性原语；headless 用这条，**不**依赖帧率）。
    #[func]
    fn advance_ticks(&mut self, count: i64) -> VarDictionary {
        let n = count.max(0) as u64;
        match self.driver.as_mut() {
            Some(d) => report_dict(&d.drive_ticks(n)),
            None => report_dict(&PumpReport::default()),
        }
    }

    #[func]
    fn play(&mut self) {
        if let Some(d) = self.driver.as_mut() {
            d.play();
        }
    }

    #[func]
    fn pause(&mut self) {
        if let Some(d) = self.driver.as_mut() {
            d.pause();
        }
    }

    #[func]
    fn toggle(&mut self) {
        if let Some(d) = self.driver.as_mut() {
            d.toggle();
        }
    }

    /// 跳到某个 tick（从头重放；**不产生结算通知**）。
    #[func]
    fn seek(&mut self, tick: i64) {
        if let Some(d) = self.driver.as_mut() {
            d.seek(tick);
        }
    }

    /// 「下一个事件」：返回跳到的 tick（没有则 -1）。
    #[func]
    fn step_event(&mut self) -> i64 {
        self.driver.as_mut().and_then(|d| d.step_event()).unwrap_or(-1)
    }

    /// 「上一个事件」：返回跳到的 tick（没有则 -1）。
    #[func]
    fn step_back_event(&mut self) -> i64 {
        self.driver.as_mut().and_then(|d| d.step_back_event()).unwrap_or(-1)
    }

    // ------------------------------------------------- 模式档位查表（`dispatch/89`）
    //
    // 回放里记着**这一局真实的选择**（`AxisModeInfo` / `FunctionModeInfo` / `TurnModeInfo` /
    // `TimeModeInfo`），播放屏 `_begin_replay()` 要把这 4 个**字符串**翻成 `App.state.*` 的
    // **档位下标**（`GwModes`，`client/src/app/modes.gd`）。
    //
    // 这四个 `static func` 原来在 `client/src/play/replay_player.gd::GwReplayPlayer` 里
    // （84 号删剩的最后一小块，89 号整份删除）。搬到这里而不是留在 GDScript 的理由：
    // 下标就是**线上枚举的声明顺序**，那张唯一的权威表 `protocol::WIRE_VALUE_ENUMS`
    // 本来就在 Rust（`docs/07` §5.2，**逐位**）—— 视图层再抄一份就等于第二个真相。
    // 纯查表：字符串进、整数出，无状态、不看回放，所以是静态方法。
    //
    // 兜底值（不认识的字符串）与旧 `.gd` 表**逐值相同**：3 / 0 / 0 / 5。

    /// `axis_mode` → `GwModes` 轴档（`0 = NoAxis / 1 = OnlyMain / 2 = EveryFive / 3 = EveryUnit`）。
    #[func]
    fn axis_index(s: GString) -> i64 {
        super::replay_driver::axis_index(s.to_string().as_str())
    }

    /// `function_mode` → 函数档（`0 = NormalFunction / 1 = DiffEqFunction`）。
    #[func]
    fn function_index(s: GString) -> i64 {
        super::replay_driver::function_index(s.to_string().as_str())
    }

    /// `turn_mode` → 回合制档（`0 = SequentialTurns / 1 = SimultaneousTurns`）。
    #[func]
    fn turn_index(s: GString) -> i64 {
        super::replay_driver::turn_index(s.to_string().as_str())
    }

    /// `time_mode` → 时限档（`0 = Timer30s … 4 = Timer5m / 5 = TimerInf`）。
    #[func]
    fn time_index(s: GString) -> i64 {
        super::replay_driver::time_index(s.to_string().as_str())
    }

    /// 一句话说明"这个类是干什么的"（`docs`/自检里用）。
    #[func]
    fn about() -> GString {
        GString::from(
            "回放驱动（我方自定）：定步 60 tick/s、pumps≡ticks、长帧追赶无上限；\
             结算批借 replaydiff::split_batches；模式字符串→档位下标见 4 个静态查表函数。\
             见 docs/16-client.md §56 / §59",
        )
    }
}

/// `ReplayError` → 字符串（诊断用；`as_str()` 是协议里的既有口径）。
#[allow(dead_code)]
fn error_text(e: ReplayError) -> String {
    String::from(e.as_str())
}
