//! 回放文件（`user://replays/<unix 时间戳>`）：读取、解析、按 tick 还原局面。
//!
//! ## 格式（**观测**，`docs/78` §2、`docs/133` §2）
//!
//! gzip 压缩 + 每行一个 JSON 事件：
//!
//! ```text
//! {"event":{"FunctionInfo":{"owner_id":635,"function":"(-0.35*(-11.40+x))",
//!            "corrected_function":null}},"tick":1234}
//! ```
//!
//! 内层 `{"<EventName>":{…}}` 与网络协议**同一套名字与字段**
//! （`src/_scaffold/protocol.rs` / `src/events.rs`），所以这里**不重写解析**：
//! 每行拆出 `event` 对象后直接交给 [`protocol::parse_message`]，
//! 字段读取也复用 [`protocol::get_str`] / [`protocol::get_int`] / [`protocol::get_f64`] /
//! [`protocol::get_bool`] / [`protocol::get_color`]。
//! **协议里没有的字段一律不猜**：`ReplayEvent::body` 保留那一段原始 JSON 原文
//! （`docs/133` §3 的"透传"口径）。
//!
//! ## 播放是按 tick 的（**观测**，`docs/35`：60 ticks/秒）
//!
//! 回放里没有任何"墙钟时间"，只有每条事件的 `tick`。所以播放器按
//! `tick = tick + floor(dt * 60 * speed)` 推进，把 `tick <= 当前 tick` 的事件
//! 按**文件顺序**消费掉（[`ReplayPlayer`]）。
//!
//! ⚠️ 400 份真实回放里有 **7 份 tick 有回退**（最多 2 次、单次差 ≤ 19 tick，
//! `docs/133` §2.3）。这里**不排序**：排序会把"同一 tick 内的批内顺序"打乱，
//! 而批内顺序本身有意义（`docs/113`）。游标模型天然容忍回退：
//! 迟到的那条会在一次 `advance` 里被一起消费掉。
//!
//! ## 证据分级
//!
//! * 格式 / 事件名 / 字段名：**观测**（400 份回放 + 协议抓包同构）；
//! * tick ↔ 60 Hz：**观测**（`docs/35` §2，113 个 `TickReply` 样本）；
//! * 局面还原规则（哪条事件改哪个字段）：**推断**——口径照 `tools/replay_state.py`，
//!   两份实现在 `docs/133` §5 用同一份真实回放对拍；
//! * 原版回放界面长什么样：**未知**（`docs/78` 只给出场景名；播放控件是我们自定的）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::_scaffold::protocol::{self, RawMessage};

pub use crate::_scaffold::replay_inflate::{gunzip, is_gzip, InflateError};

/// gzip 解压 / 文件读取层面失败的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayError {
    /// 读不到文件。
    Io,
    /// gzip 解压失败（原因见 [`InflateError`]）。
    Inflate(InflateError),
    /// 解压出来不是 UTF-8。
    NotUtf8,
    /// 一行事件都没解析出来。
    Empty,
}

impl ReplayError {
    pub fn as_str(self) -> &'static str {
        match self {
            ReplayError::Io => "io",
            ReplayError::Inflate(e) => e.as_str(),
            ReplayError::NotUtf8 => "not_utf8",
            ReplayError::Empty => "empty",
        }
    }
}

/// 一条回放事件：`{tick, 事件名, 该事件的原始 JSON 对象}`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayEvent {
    /// 事件的 tick（`docs/35`：60 ticks/秒）。
    pub tick: i64,
    /// 事件名，例如 `FunctionInfo`（与协议里的外部标签一致）。
    pub kind: String,
    /// 该事件载荷的**原始 JSON 内部原文**，例如 `"owner_id":635,"function":"x"`
    /// （不含最外层花括号，与 [`protocol::RawMessage::body`] 同一种切法）。
    /// 协议里没有的字段就靠它透传，不猜键名。
    pub body: String,
}

impl ReplayEvent {
    /// 把这条事件包成协议层的 [`RawMessage`]，好复用 `protocol::get_*`。
    pub fn msg(&self) -> RawMessage {
        RawMessage {
            kind: self.kind.clone(),
            body: self.body.clone(),
            text: self.to_json(),
        }
    }

    /// 重组成线上同构的单键 JSON：`{"Kind":{…}}`。
    pub fn to_json(&self) -> String {
        format!("{{\"{}\":{{{}}}}}", self.kind, self.body)
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        protocol::get_str(&self.msg(), key)
    }

    pub fn get_str_unescaped(&self, key: &str) -> Option<String> {
        protocol::get_str_unescaped(&self.msg(), key)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        protocol::get_int(&self.msg(), key)
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        protocol::get_f64(&self.msg(), key)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        protocol::get_bool(&self.msg(), key)
    }

    pub fn get_color(&self, key: &str) -> Option<(u8, u8, u8)> {
        protocol::get_color(&self.msg(), key)
    }

    /// 载荷里的 `entity_id`（多数实体类事件用它）。
    pub fn entity_id(&self) -> Option<i64> {
        self.get_i64("entity_id")
    }
}

/// 解析出来的一局回放。
#[derive(Debug, Clone, Default)]
pub struct Replay {
    /// 按**文件顺序**（不排序）的事件序列。
    pub events: Vec<ReplayEvent>,
    /// 解析失败被跳过的行数（对齐 `tools/parse_replay.py` 的 `continue`）。
    pub skipped_lines: usize,
    /// 文件是不是 gzip（`false` = 明文 JSONL，两个工具都吃）。
    pub gzipped: bool,
    /// 全局实体表：`player_id → 名字`（`NameInfo.entity_id` 就是 `player_id`，`docs/79` §1）。
    pub names: BTreeMap<i64, String>,
    /// 全局实体表：`soldier_id → player_id`（`FunctionInfo.owner_id` 是**士兵**号）。
    pub soldier_owner: BTreeMap<i64, i64>,
    /// `TargetInfo.target_id`（靶子）。
    pub targets: BTreeSet<i64>,
}

impl Replay {
    /// 从原始字节解析（自动识别 gzip / 明文）。
    pub fn parse_bytes(bytes: &[u8]) -> Result<Replay, ReplayError> {
        let (text, gzipped) = if is_gzip(bytes) {
            let plain = gunzip(bytes).map_err(ReplayError::Inflate)?;
            (plain, true)
        } else {
            (bytes.to_vec(), false)
        };
        let text = String::from_utf8(text).map_err(|_| ReplayError::NotUtf8)?;
        let mut replay = Replay { gzipped, ..Replay::default() };
        for line in text.lines() {
            match parse_line(line) {
                Some(ev) => {
                    // 两张全局表在整份文件上收集（`docs/79` §1 的链路），
                    // 与 `tools/replay_state.py` 的预扫描口径一致。
                    if ev.kind == "NameInfo" {
                        if let (Some(id), Some(name)) =
                            (ev.entity_id(), ev.get_str("name"))
                        {
                            replay.names.insert(id, name);
                        }
                    } else if ev.kind == "SoldierInfo" {
                        if let (Some(sid), Some(pid)) =
                            (ev.get_i64("soldier_id"), ev.get_i64("player_id"))
                        {
                            replay.soldier_owner.insert(sid, pid);
                        }
                    } else if ev.kind == "TargetInfo" {
                        if let Some(tid) = ev.get_i64("target_id") {
                            replay.targets.insert(tid);
                        }
                    }
                    replay.events.push(ev);
                }
                None => {
                    if !line.trim().is_empty() {
                        replay.skipped_lines += 1;
                    }
                }
            }
        }
        if replay.events.is_empty() {
            return Err(ReplayError::Empty);
        }
        Ok(replay)
    }

    /// 从文件读（`user://replays/<时间戳>` 在 Godot 里是普通文件路径）。
    pub fn load(path: &Path) -> Result<Replay, ReplayError> {
        let bytes = std::fs::read(path).map_err(|_| ReplayError::Io)?;
        Replay::parse_bytes(&bytes)
    }

    /// 第一条 / 最后一条事件的 tick（**文件顺序**上的，不排序）。
    pub fn tick_range(&self) -> (i64, i64) {
        let first = self.events.iter().map(|e| e.tick).min().unwrap_or(0);
        let last = self.events.iter().map(|e| e.tick).max().unwrap_or(0);
        (first, last)
    }

    /// 事件名 → 条数（诊断 / 文档用例）。
    pub fn kind_counts(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for ev in &self.events {
            *out.entry(ev.kind.clone()).or_insert(0) += 1;
        }
        out
    }

    /// 把整份回放一直重放到 `tick` 为止的局面。
    ///
    /// 口径：**文件顺序**，只消费 `事件.tick <= tick` 的事件。这就是
    /// `tools/replay_state.py` 的循环，也是客户端播放器的前进方向。
    pub fn state_at(&self, tick: i64) -> ReplayState {
        let mut st = ReplayState::new(&self.names, &self.soldier_owner, &self.targets);
        for ev in &self.events {
            if ev.tick > tick {
                break; // 文件顺序单调时提前结束；不单调时行为同游标模型（见模块文档）
            }
            st.apply(ev);
        }
        st.tick = tick;
        st
    }
}

// ---------------------------------------------------------------------------
// 局面（按 tick 累加）
// ---------------------------------------------------------------------------

/// 一个圆（障碍 / 洞 / 实体位置）。`r` = 半径（棋盘单位）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub x: f64,
    pub y: f64,
    pub r: f64,
}

/// 一次开火（`FunctionPoints`）—— 回放里**没有**完整弹道点序列，只有起点与 `end_x`。
#[derive(Debug, Clone, PartialEq)]
pub struct FiredShot {
    pub tick: i64,
    pub owner: i64,
    pub start: (f64, f64),
    pub end_x: f64,
    /// 开火那一刻该射手最近一次非空 `FunctionInfo.function`（`docs/78` §4 的取法）。
    pub function: String,
    /// 开火那一刻的 `corrected_function`（若有）。
    pub corrected: String,
}

/// 一条特效（`EffectInfo`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub tick: i64,
    pub id: i64,
    pub pos: (f64, f64),
    pub effect_type: String,
    pub start_tick: i64,
}

/// 某一 tick 的局面。
///
/// 字段全部是"由事件推进出来的"（[`ReplayState::apply`]），没有一处靠猜：
/// 哪条事件改哪个字段见本文件的 `apply` 与 `docs/133` §3 的表。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplayState {
    pub tick: i64,
    /// 当前障碍（`ClearObstacles` 清空，`AddObstacle` 追加）。
    pub obstacles: Vec<Circle>,
    /// 被挖掉的洞（`DestroyObstacle`；**不移除**障碍，只记录，`docs/30`）。
    pub holes: Vec<Circle>,
    /// `entity_id → 位置`（`PosInfo`）。
    pub pos: BTreeMap<i64, Circle>,
    /// `entity_id → "Team1"/"Team2"`（`TeamInfo`）。
    pub team: BTreeMap<i64, String>,
    /// `entity_id → 生死`（`LifeInfo`；缺省视为活着，与 `replay_state.py` 一致）。
    pub alive: BTreeMap<i64, bool>,
    /// `owner_id(=soldier_id) → 最近一次非空函数文本`（`FunctionInfo`）。
    /// 玩家复用上一回合的函数时不会再发 `FunctionInfo`，所以这里**不清空**。
    pub functions: BTreeMap<i64, String>,
    /// `owner_id → corrected_function`（`"__null__"` 表示事件里是 `null`）。
    pub corrected: BTreeMap<i64, String>,
    /// 皮肤/脸/帽子（`Skin/Face/HatGraphicsInfo`），键是 `<slot>:<entity_id>`。
    pub graphics: BTreeMap<String, String>,
    /// 玩家颜色（`ColorInfo`，`docs/102`）。
    pub colors: BTreeMap<i64, (u8, u8, u8)>,
    /// 靶子 id 集合（**全局**，来自 `Replay.targets`）。
    pub targets: BTreeSet<i64>,
    /// 被 `EntityRemoved` 移除的实体。
    pub removed: BTreeSet<i64>,
    /// 服务端状态机（`GameStateInfo` 的**最后一个**值）。
    pub game_state: String,
    /// 房间名 / 关卡名（`GameNameInfo`）。
    pub game_name: String,
    /// 四个模式（`*ModeInfo`），保留原始字符串，不映射成枚举（协议里是字符串）。
    pub axis_mode: String,
    pub function_mode: String,
    pub turn_mode: String,
    pub time_mode: String,
    /// `TurnCountInfo.turn_count`。
    pub turn_count: i64,
    /// 当前射手（`TurnInfo.soldier_id`）。
    pub shooter: Option<i64>,
    /// 最近一次 `FunctionActive`（开火后的激活 tick）。
    pub active_tick: Option<i64>,
    /// 特效列表（`EffectInfo`，按出现顺序）。
    pub effects: Vec<Effect>,
    /// 每次开火（`FunctionPoints`）。
    pub shots: Vec<FiredShot>,
    /// `soldier_id → player_id`（全局表，拷贝一份便于直接用）。
    pub soldier_owner: BTreeMap<i64, i64>,
    /// `player_id → 名字`（全局表）。
    pub names: BTreeMap<i64, String>,
}

impl ReplayState {
    pub fn new(
        names: &BTreeMap<i64, String>,
        soldier_owner: &BTreeMap<i64, i64>,
        targets: &BTreeSet<i64>,
    ) -> Self {
        ReplayState {
            names: names.clone(),
            soldier_owner: soldier_owner.clone(),
            targets: targets.clone(),
            ..ReplayState::default()
        }
    }

    /// 应用一条事件。
    ///
    /// 规则表（**推断**，口径 = `tools/replay_state.py`，逐项在 `docs/133` §3 列出）：
    /// 只有这 15 类事件改局面，其余**只进事件流**（不猜副作用）。
    pub fn apply(&mut self, ev: &ReplayEvent) {
        match ev.kind.as_str() {
            "ClearObstacles" => self.obstacles.clear(),
            "AddObstacle" => {
                if let (Some(x), Some(y), Some(r)) = (
                    ev.get_f64("pos_x"),
                    ev.get_f64("pos_y"),
                    ev.get_f64("radius"),
                ) {
                    self.obstacles.push(Circle { x, y, r });
                }
            }
            "DestroyObstacle" => {
                if let (Some(x), Some(y)) = (ev.get_f64("pos_x"), ev.get_f64("pos_y")) {
                    self.holes.push(Circle { x, y, r: ev.get_f64("radius").unwrap_or(0.5) });
                }
            }
            "PosInfo" => {
                if let (Some(id), Some(x), Some(y)) = (
                    ev.entity_id(),
                    ev.get_f64("pos_x"),
                    ev.get_f64("pos_y"),
                ) {
                    self.pos.insert(id, Circle { x, y, r: ev.get_f64("radius").unwrap_or(0.5) });
                }
            }
            "TeamInfo" => {
                if let (Some(id), Some(t)) = (ev.entity_id(), ev.get_str("team")) {
                    self.team.insert(id, t);
                }
            }
            "LifeInfo" => {
                if let (Some(id), Some(a)) = (ev.entity_id(), ev.get_bool("alive")) {
                    self.alive.insert(id, a);
                }
            }
            "FunctionInfo" => {
                if let Some(owner) = ev.get_i64("owner_id") {
                    if let Some(f) = ev.get_str_unescaped("function") {
                        if !f.is_empty() {
                            self.functions.insert(owner, f);
                        }
                    }
                    match ev.get_str_unescaped("corrected_function") {
                        Some(c) => {
                            self.corrected.insert(owner, c);
                        }
                        None => {
                            // `"corrected_function":null` —— 与"字段不存在"区分开
                            self.corrected.insert(owner, "__null__".to_string());
                        }
                    }
                }
            }
            "FunctionPoints" => {
                if let (Some(owner), Some(sx), Some(sy), Some(ex)) = (
                    ev.get_i64("owner_id"),
                    ev.get_f64("start_x"),
                    ev.get_f64("start_y"),
                    ev.get_f64("end_x"),
                ) {
                    self.shots.push(FiredShot {
                        tick: ev.tick,
                        owner,
                        start: (sx, sy),
                        end_x: ex,
                        function: self.functions.get(&owner).cloned().unwrap_or_default(),
                        corrected: match self.corrected.get(&owner) {
                            Some(c) if c != "__null__" => c.clone(),
                            _ => String::new(),
                        },
                    });
                }
            }
            "FunctionActive" => {
                if let Some(t) = ev.get_i64("active_tick") {
                    self.active_tick = Some(t);
                }
            }
            "EffectInfo" => {
                if let (Some(id), Some(x), Some(y)) = (
                    ev.get_i64("effect_id"),
                    ev.get_f64("pos_x"),
                    ev.get_f64("pos_y"),
                ) {
                    self.effects.push(Effect {
                        tick: ev.tick,
                        id,
                        pos: (x, y),
                        effect_type: ev.get_str("effect_type").unwrap_or_default(),
                        start_tick: ev.get_i64("start_tick").unwrap_or(0),
                    });
                }
            }
            "EntityRemoved" => {
                if let Some(id) = ev.entity_id() {
                    self.removed.insert(id);
                }
            }
            "GameStateInfo" => {
                if let Some(s) = ev.get_str("game_state") {
                    self.game_state = s;
                }
            }
            "GameNameInfo" => {
                if let Some(s) = ev.get_str("game_name") {
                    self.game_name = s;
                }
            }
            "AxisModeInfo" => self.axis_mode = ev.get_str("axis_mode").unwrap_or_default(),
            "FunctionModeInfo" => {
                self.function_mode = ev.get_str("function_mode").unwrap_or_default()
            }
            "TurnModeInfo" => self.turn_mode = ev.get_str("turn_mode").unwrap_or_default(),
            "TimeModeInfo" => self.time_mode = ev.get_str("time_mode").unwrap_or_default(),
            "TurnCountInfo" => {
                if let Some(n) = ev.get_i64("turn_count") {
                    self.turn_count = n;
                }
            }
            "TurnInfo" => {
                if let Some(s) = ev.get_i64("soldier_id") {
                    self.shooter = Some(s);
                }
            }
            "SkinGraphicsInfo" | "FaceGraphicsInfo" | "HatGraphicsInfo" => {
                if let (Some(id), Some(g)) = (ev.entity_id(), ev.get_str("graphics_str")) {
                    let slot = match ev.kind.as_str() {
                        "SkinGraphicsInfo" => "skin",
                        "FaceGraphicsInfo" => "face",
                        _ => "hat",
                    };
                    self.graphics.insert(format!("{slot}:{id}"), g);
                }
            }
            "ColorInfo" => {
                if let (Some(id), Some(c)) = (ev.entity_id(), ev.get_color("color")) {
                    self.colors.insert(id, c);
                }
            }
            _ => {}
        }
    }

    /// 有位置、且没被移除的实体（画士兵/靶子用）。
    pub fn visible_entities(&self) -> Vec<(i64, Circle, bool, bool)> {
        // (id, 位置, 是否活着, 是否靶子)
        let ids: BTreeSet<i64> = self
            .pos
            .keys()
            .copied()
            .chain(self.team.keys().copied())
            .chain(self.alive.keys().copied())
            .collect();
        ids.into_iter()
            .filter(|id| !self.removed.contains(id))
            .map(|id| {
                let p = self.pos.get(&id).copied().unwrap_or(Circle { x: 0.0, y: 0.0, r: 0.5 });
                let alive = *self.alive.get(&id).unwrap_or(&true);
                let is_target = self.targets.contains(&id);
                (id, p, alive, is_target)
            })
            .collect()
    }

    /// 士兵的"队伍 → 0/1"（`Team1` = 0，`Team2` = 1，其它/未知 = -1）。
    ///
    /// 依据 `docs/105`：Team1 恒在 x<0、Team2 恒在 x>0（773 个位置 0 例外），
    /// 所以射击方向由**队伍**决定，不用几何猜。
    pub fn team_index(&self, id: i64) -> i32 {
        match self.team.get(&id).map(|s| s.as_str()) {
            Some("Team1") => 0,
            Some("Team2") => 1,
            _ => -1,
        }
    }

    /// 规范性快照串 —— **Python / Rust / GDScript 三端同格式**，用来对拍
    /// （`docs/133` §5）。
    ///
    /// ```text
    /// tick=T|O=x,y,r;…|E=id,team,alive,x,y,r;…|F=owner=text;…
    /// ```
    /// * 数字一律 `%.6f`；`team` 未知写 `?`；`alive` 写 `0/1`（缺省 1）；
    /// * 没有 `PosInfo` 的实体把 `x,y,r` 三项留空；
    /// * 障碍保持**插入顺序**（不排序，顺序本身是事件流的一部分）；
    /// * 实体与函数表按 id 升序。
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("tick={}", self.tick));
        out.push_str("|O=");
        for (i, o) in self.obstacles.iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            out.push_str(&format!("{:.6},{:.6},{:.6}", o.x, o.y, o.r));
        }
        out.push_str("|E=");
        let ids: BTreeSet<i64> = self
            .pos
            .keys()
            .copied()
            .chain(self.team.keys().copied())
            .chain(self.alive.keys().copied())
            .collect();
        for (i, id) in ids.into_iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            let team = self.team.get(&id).map(|s| s.as_str()).unwrap_or("?");
            let alive = if *self.alive.get(&id).unwrap_or(&true) { 1 } else { 0 };
            match self.pos.get(&id) {
                Some(p) => out.push_str(&format!(
                    "{},{},{},{:.6},{:.6},{:.6}",
                    id, team, alive, p.x, p.y, p.r
                )),
                None => out.push_str(&format!("{},{},{},,,", id, team, alive)),
            }
        }
        out.push_str("|F=");
        for (i, (owner, text)) in self.functions.iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            out.push_str(&format!("{}={}", owner, text));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// 行解析
// ---------------------------------------------------------------------------

/// 从 `text` 里找 `"key"` 之后的对象，返回**含花括号原文**的那一段。
///
/// 只做"花括号配平 + 字符串内不算"，不建 JSON 树 —— 因为协议层已经有自己的
/// 逐字段取法（`protocol::get_*`），这里多建一棵树只会多一份可能与真值不一致的解析。
fn object_after<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{}\"", key);
    let at = text.find(&pat)? + pat.len();
    let rest = text[at..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 解析一行：`{"event":{…},"tick":N}` → [`ReplayEvent`]。
///
/// 事件体交给 [`protocol::parse_message`]，也就是**与网络报文同一个解析器**
/// ——回放的事件名 / 字段名与协议同构（`docs/78` §2）。
pub fn parse_line(line: &str) -> Option<ReplayEvent> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let event_obj = object_after(line, "event")?;
    let msg = protocol::parse_message(event_obj)?;
    // tick 在 event 对象**之后**，避免把 payload 里的 `active_tick` / `start_tick` 当成它
    let after = &line[line.find(event_obj)? + event_obj.len()..];
    let tick = object_after(after, "tick").or_else(|| {
        let pat = "\"tick\"";
        let at = after.find(pat)? + pat.len();
        let rest = after[at..].trim_start().strip_prefix(':')?.trim_start();
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+'))
            .unwrap_or(rest.len());
        Some(&rest[..end])
    });
    let tick: i64 = match tick {
        Some(t) => t.trim().parse().ok()?,
        None => return None,
    };
    Some(ReplayEvent { tick, kind: msg.kind, body: msg.body })
}

// ---------------------------------------------------------------------------
// 播放器：按 tick 推进
// ---------------------------------------------------------------------------

/// tick 时钟（`docs/35` §2：**精确 60 ticks/秒**）。
pub const TICKS_PER_SECOND: i64 = 60;

/// 按 tick 推进的播放器：游标 + 累加局面。
///
/// 播放模型（**我们自定**，原版未观测；`docs/133` §4）：
/// * 每帧 `tick += (dt * 60 * speed)` 取整累加（余数留着，不然 1× 会慢 1 tick/帧）；
/// * 把 `事件.tick <= tick` 的事件按**文件顺序**逐条 apply，并记录本帧消费了哪些下标；
/// * 跳到任意 tick = 从头重放（1500 条事件，代价可忽略），换来"状态永远等于
///   [`Replay::state_at`]"，不会有增量重放的漂移。
#[derive(Debug, Clone)]
pub struct ReplayPlayer {
    pub replay: Replay,
    pub state: ReplayState,
    /// 下一条待消费事件在 `replay.events` 里的下标。
    pub cursor: usize,
    /// 当前 tick（浮点保留亚 tick 余数，取整后才算"当前"）。
    ///
    /// T72 之后 [`ReplayPlayer::advance_dt`] 只按**整数 tick** 推进，
    /// 亚 tick 余数改由 [`ReplayPlayer::accum_ns`] 承担（定步 `NANOS_PER_TICK`）。
    pub tick: f64,
    /// **定步累加器**（纳秒，T72 归一）：`advance_dt` 的亚 tick 余数。
    ///
    /// 原版播放器**没有**这个东西 —— 累加器在 `GraphwarTickHandler`
    /// （`elapsed_nanos`，`docs/06` §63.2），播放器只是把它的 `tick` 镜像到自己的 `+0x58`。
    /// 我方 headless 没有墙钟，所以这个替身必须跟着驱动走；档位：**我方投影**。
    pub accum_ns: u64,
    pub playing: bool,
    /// 播放倍速（1.0 = 真实速度 = 60 tick/秒）。
    pub speed: f64,
    first_tick: i64,
    last_tick: i64,
}

impl ReplayPlayer {
    pub fn new(replay: Replay) -> Self {
        let (first, last) = replay.tick_range();
        let mut p = ReplayPlayer {
            state: ReplayState::new(&replay.names, &replay.soldier_owner, &replay.targets),
            replay,
            cursor: 0,
            tick: first as f64,
            accum_ns: 0,
            playing: false,
            speed: 1.0,
            first_tick: first,
            last_tick: last,
        };
        p.state.tick = first;
        p
    }

    pub fn first_tick(&self) -> i64 {
        self.first_tick
    }
    pub fn last_tick(&self) -> i64 {
        self.last_tick
    }
    /// 当前整 tick。
    pub fn current_tick(&self) -> i64 {
        self.tick.floor() as i64
    }
    /// 进度 0..1。
    pub fn progress(&self) -> f64 {
        let span = (self.last_tick - self.first_tick).max(1) as f64;
        ((self.tick - self.first_tick as f64) / span).clamp(0.0, 1.0)
    }
    pub fn finished(&self) -> bool {
        self.cursor >= self.replay.events.len() && self.tick >= self.last_tick as f64
    }

    pub fn play(&mut self) {
        if self.finished() {
            // 放完了再按播放 = 从头放（我们自定的交互）
            self.seek(self.first_tick);
        }
        self.playing = true;
    }
    pub fn pause(&mut self) {
        self.playing = false;
    }
    pub fn toggle(&mut self) {
        if self.playing {
            self.pause()
        } else {
            self.play()
        }
    }

    /// 消费所有 `tick <= 目标` 的事件，返回被消费事件的下标（按消费顺序）。
    pub fn advance_to(&mut self, target: i64) -> Vec<usize> {
        let mut applied = Vec::new();
        while self.cursor < self.replay.events.len()
            && self.replay.events[self.cursor].tick <= target
        {
            let ev = self.replay.events[self.cursor].clone();
            self.state.apply(&ev);
            applied.push(self.cursor);
            self.cursor += 1;
        }
        self.tick = target as f64;
        self.state.tick = target;
        applied
    }

    /// 按墙钟时间推进一帧；返回本帧消费的事件下标。
    ///
    /// # T72 归一（台账 C 档 `replay_driver::drive_frame`）
    ///
    /// 这里原来是**第二套时钟**：`tick += dt × 60 × speed` 的 **f64 累加**（旧
    /// `client/src/play/replay_player.gd::advance_dt` 的模型）。真值是定步
    /// `16 666 666 ns`（`NANOS_PER_TICK` / `0xfe502a`，`docs/06` §63.2），而 `1/60` 与它
    /// 不是同一个数 ⇒ 两套时钟在长序列上必然漂。
    ///
    /// 现在定步折叠只有一份：[`crate::graphwar_tick_handler::fold_dt_into_ticks`]
    /// （余数进 [`ReplayPlayer::accum_ns`]，与 `0x51948f` 的进位分支同源）。
    /// `dt` 仍夹到 1.0 s（掉帧不跳一大截，这是旧 `.gd` 的行为，`docs/16` §56 有登记）。
    pub fn advance_dt(&mut self, dt_seconds: f64) -> Vec<usize> {
        if !self.playing {
            return Vec::new();
        }
        let dt = dt_seconds.max(0.0).min(1.0); // 掉帧时不跳一大截
        let dt = dt * self.speed.max(0.0);
        let n = crate::graphwar_tick_handler::fold_dt_into_ticks(dt, &mut self.accum_ns);
        self.tick += n as f64;
        let target = self.tick.floor() as i64;
        if target > self.last_tick {
            self.tick = self.last_tick as f64;
            self.playing = false;
        }
        self.advance_to(self.tick.floor() as i64)
    }

    /// 跳到某个 tick（从头重放，保证与 [`Replay::state_at`] 一致）。
    pub fn seek(&mut self, tick: i64) {
        let tick = tick.clamp(self.first_tick, self.last_tick);
        self.state = ReplayState::new(
            &self.replay.names,
            &self.replay.soldier_owner,
            &self.replay.targets,
        );
        self.cursor = 0;
        self.tick = tick as f64;
        self.state.tick = tick;
        while self.cursor < self.replay.events.len()
            && self.replay.events[self.cursor].tick <= tick
        {
            let ev = self.replay.events[self.cursor].clone();
            self.state.apply(&ev);
            self.cursor += 1;
        }
        self.state.tick = tick;
    }

    /// 「下一个事件」按钮：跳到下一条**尚未消费**事件的 tick。
    pub fn step_event(&mut self) -> Option<i64> {
        if self.cursor >= self.replay.events.len() {
            return None;
        }
        let t = self.replay.events[self.cursor].tick;
        self.seek(t);
        Some(t)
    }

    /// 「上一个事件」按钮：跳到**上一条**事件的 tick（严格早于当前 tick）。
    pub fn step_back_event(&mut self) -> Option<i64> {
        let now = self.current_tick();
        let prev = self
            .replay
            .events
            .iter()
            .map(|e| e.tick)
            .filter(|&t| t < now)
            .max();
        match prev {
            Some(t) => {
                self.seek(t);
                Some(t)
            }
            None => None,
        }
    }
}

// ---------------------------------------------------------------------------
// 6. 原版 `GraphwarReplayPlayer`：把回放事件**注入**活着的对局（T95 落码）
// ---------------------------------------------------------------------------
//
// ⚠️ 这一节与上面 [`ReplayPlayer`] **不是同一个东西**：
//   * [`ReplayPlayer`] 是我们自造的**纯数据**播放器（`ReplayState` 快照 + cursor），
//     headless 对拍/回放界面用它；
//   * [`GraphwarReplayPlayer`] 是**原版**的播放器：它不维护局面快照，而是把回放里的事件
//     **当成"服务器发来的远端事件"重新喂一遍**（`handle_server_remote_event`），
//     再把 `GraphwarTickHandler` 的 tick 计数对齐到回放时间轴。
//
// ## 函数清单（x86 vaddr = ELF 口径；arm64 = 同一份源码的另一份构建，有 `.symtab` 真名）
//
// | 函数 | x86 vaddr / FDE | arm64 vaddr / size | 证据档 |
// |---|---|---|---|
// | `inject_events` | **`0x3d61a0`** / `0x70b` | `0x3d8d9c` / `0x500` | 观测（逐指令；见下） |
// | `go_to_first_checkpoint` | **`0x3d6a40`** / `0x162` | `0x3d94a0` / `0x1b8` | 观测 |
// | `go_to_next_checkpoint` | **`0x3d68b0`** / `0x186` | `0x3d929c` / `0x204` | 观测 |
// | `go_to_previous_checkpoint` | **`0x3d6bb0`** / `0x1bc` | `0x3d9658` / `0x220` | 观测 |
//
// 名字来源：`fixtures/name_to_addr_v2.tsv` 三向唯一锚（android+win+win_old 三份构建同名），
// 不是本仓库的推断。
//
// ## `GraphwarReplayPlayer` 的字段偏移（观测，从四个函数的读写下夹逼）
//
// ```text
// +0x00  u64/usize   "已开始注入" 标志：`inject_events` 写 1（x86 `0x3d62c5 mov qword [rax],1`）；
//                    `go_to_next_checkpoint` 读它（`0x3d6922 cmp byte [rdi],0`）决定起点下标
// +0x08  usize       最近注入的事件下标（`0x3d62d4 mov [rax+8],r13`）
// +0x10  usize       `events: Vec<GraphwarEvent>` 的 cap（`0x3d6243` 起按 cap/ptr/len 读）
// +0x18  ptr         同上：数据指针（`0x3d6256 mov rax,[rax+0x18]`）
// +0x20  usize       同上：条数（`0x3d6237 mov rax,[rax+0x20]`）
// +0x28  usize       `checkpoints: Vec<u32>` 的 cap
// +0x30  ptr         同上：数据指针（`0x3d68bc mov rax,[rdi+0x30]`）
// +0x38  usize       同上：条数（`0x3d68c0 mov rcx,[rdi+0x38]`；`0x3d6a93` 用它判空）
// +0x58  u32         当前 tick（`0x3d6284 mov [rax+0x58],ecx` 写；`0x3d68cf` 读作二分查找的 key）
// ```
//
// 事件元素 **`0x50` = 80 字节**、tick 在元素 **`+0x48`**（x86 `0x3d629d lea rsi,[r13*4]` +
// `0x3d62a5 add rsi,r13` + `0x3d62a8 shl rsi,4` = `i*0x50`，`0x3d62ac mov ecx,[rax+rsi+0x48]`；
// arm64 `0x3d8e78 mov w9,#0x50` + `0x3d8e80 madd x1,x23,x9,x8` + `0x3d8e84 ldr w28,[x1,#0x48]`
// 逐条对上）。
//
// ## `inject_events` 的一圈到底做了什么（逐指令）
//
// ```text
// 0x3d61c6 / arm64 0x3d8dbc  client+0x40 = Rc<RefCell<GraphwarModuleContainer>>
// 0x3d61e9 call 0x46aa50     ; get_module::<GraphwarTickHandler>()   ← docs/02 §22.3 表
// 0x3d621b call 0x46be70     ; get_module::<GraphwarGameStateManager>()（同一张表）
// 0x3d6243 cmp  r13,[self+0x20]            ; from >= events.len() ⇒ 整个循环跳过
// 0x3d62ac mov  ecx,[events_base + i*0x50 + 0x48]   ; 事件 tick
// 0x3d62b4 cmp  ecx,[rsp]                  ; tick >= until_tick ⇒ 退出（**严格小于**才注入）
// 0x3d62c5 mov  qword [self],1             ; 开始过
// 0x3d62d4 mov  [self+8],r13               ; 记录下标
// 0x3d62e0 call 0x3e8a60                   ; GraphwarEvent::clone（arm64 名：<GraphwarEvent as Clone>::clone）
// 0x3d634d call 0x3e2830                   ; GraphwarRemoteEventHandler::handle_server_remote_event
// 0x3d6376 call 0x3d3630                   ; GraphwarEventHandler::process_events
// 0x3d6395 cmp  byte [gamestate+0x138],2   ; 状态字节 < 2 才同步 tick
// 0x3d63ad mov  [tick_handler+0x58],tick   ; = TickHandler 的 u32 tick（+0x58 = payload+0x40）
// 0x3d6284 mov  [self+0x58],tick           ; 播放器自己的当前 tick
// ```
//
// ⚠️ 两处**没解透**（不许写成结论）：
//   1. `handle_server_remote_event` 的三个实参的**类型**：x86 传 `rdi=&事件副本`、
//      `rsi=client`、`rdx=&Weak<RefCell<…>>`（arm64 `0x3d8eb8 ldr x9,[x26,8]; adds x8,x9,1;
//      str x8,[x26,8]` = 弱引用 +1，调用后 `0x3d8ef4` 起再 -1 并在 0 上 `__rust_dealloc(0x48,8)`）
//      ⇒ 原版是**静态方法**（没有 `&self`），本文件用 [`ReplayInjectTarget`] 把这一步抽出来。
//   2. 为什么 `GameStateManager+0x138` 的字节 `<2` 才同步 tick —— 那个字节的语义**未知**
//      （只知道比较值 2，不猜它是不是 "game_state"）。
//
// ## 检查点按钮的三条路径（`GraphwarRootNode::process_button_pressed` = `0x3b0830` 调它们）
//
// 三条的共同尾巴：**注入完就冻结 `GraphwarTickHandler` 的时间基**（时间基哨兵
// `nanos == 1e9` ⇔ 未冻结，见 `src/graphwar_tick_handler.rs` 的 `NANOS_PER_SECOND`；
// 冻结 = 写 `Instant::now()` 到 `+0x38/+0x40`）。等价源码：`if !paused { pause() }`。

/// 原版 `GraphwarReplayPlayer`（回放**注入器**）。字段偏移见本节开头表。
#[derive(Debug, Clone, Default)]
pub struct GraphwarReplayPlayer {
    /// `+0x00`：是否已经开始注入过（`usize` 口径，`inject_events` 写 1）。
    pub started: u64,
    /// `+0x08`：最近注入的事件下标（`go_to_next_checkpoint` 的起点从它 +1 起）。
    pub cursor: usize,
    /// `+0x10/+0x18/+0x20`：`Vec<GraphwarEvent>`（元素 0x50 字节、tick 在 `+0x48`）。
    ///
    /// 我方用 [`ReplayEvent`] 承载同一个东西（`tick` + 原始 JSON 载荷）；
    /// **元素大小与 tick 偏移是原版的**（有地址），类型名是我方的。
    pub events: Vec<ReplayEvent>,
    /// `+0x28/+0x30/+0x38`：`Vec<u32>`，检查点 tick 表（**升序**——四个函数都拿它做二分）。
    pub checkpoints: Vec<u32>,
    /// `+0x58`：当前 tick（注入时逐条写；三个 checkpoint 函数的查找 key）。
    pub tick: u32,
}

/// 事件元素大小（逐位：`i*0x50`，x86 `0x3d62a8 shl rsi,4` / arm64 `0x3d8e78 mov w9,#0x50`）。
pub const REPLAY_EVENT_STRIDE: usize = 0x50;
/// tick 在事件元素内的偏移（x86 `0x3d62ac` / arm64 `0x3d8e84`：`+0x48`）。
pub const REPLAY_EVENT_TICK_OFFSET: usize = 0x48;
/// `GameStateManager` 里那个"状态字节"的偏移（`0x3d6395 cmp byte [rax+0x138],2`）。
pub const GAME_STATE_GATE_OFFSET: usize = 0x138;
/// 同一处比较的阈值：`< 2` 才把 tick 同步进 `GraphwarTickHandler`（`0x3d639c jae`）。
pub const GAME_STATE_GATE_LIMIT: u8 = 2;
/// `GraphwarTickHandler` 的 `u32 tick` 字段偏移（= payload `+0x40`，见 `src/graphwar_tick_handler.rs`）。
pub const TICK_HANDLER_TICK_OFFSET: usize = 0x58;

/// 注入时需要的那半套对局对象 —— **我方抽象，不是原版类型**。
///
/// 原版 `inject_events` 直接对着 `GraphwarGameClient` 与它的 `Rc<RefCell<GraphwarModuleContainer>>`
/// 操作（`get_module::<GraphwarTickHandler>` = x86 `0x46aa50` / `get_module::<GraphwarGameStateManager>`
/// = `0x46be70`）。本仓库 headless 没有那套 gdext 对象图，所以把"注入目标"抽成 trait；
/// **函数体逻辑逐条照反汇编**，trait 本身是接线层。
pub trait ReplayInjectTarget {
    /// `GraphwarRemoteEventHandler::handle_server_remote_event`（x86 `0x3e2830`）+
    /// `GraphwarEventHandler::process_events`（x86 `0x3d3630`）。
    fn dispatch_replay_event(&mut self, ev: &ReplayEvent);
    /// `GraphwarGameStateManager` 的 `+0x138` 字节（`0x3d6395` 读；语义未知）。
    fn game_state_byte(&self) -> u8;
    /// 写 `GraphwarTickHandler` 的 `u32 tick`（`0x3d63ad mov [tick_handler+0x58],tick`）。
    fn set_tick_handler_tick(&mut self, tick: u32);
    /// `GraphwarTickHandler` 时间基哨兵的 `nanos`（`1e9` ⇔ 未冻结；见 `NANOS_PER_SECOND`）。
    fn tick_handler_base_nanos(&self) -> u32;
    /// 冻结时间基（写 `Instant::now()` 到 `+0x38/+0x40`；`0x3d6395` 判据 + `0x3d63c8` 写入）。
    fn freeze_tick_handler(&mut self);
}

impl GraphwarReplayPlayer {
    /// 空播放器（原版没有"`new` 只写这一节字段"的证据，这里只给 `Default` 同义的构造）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `inject_events`（x86 **`0x3d61a0`** / FDE `0x70b`；arm64 `0x3d8d9c` / `0x500`）。
    ///
    /// 语义（逐条有地址）：从 `from` 号事件起，把 `tick < until_tick` 的事件**按文件顺序**
    /// 逐条注入对局；每注入一条就把播放器的 `cursor`/`tick` 更新成这条事件的值，
    /// 并在 `GameStateManager+0x138 < 2` 时把 `GraphwarTickHandler` 的 tick 也写成这条事件的 tick。
    ///
    /// `from >= events.len()` 时**一圈都不跑**（`0x3d624b jae` 直接跳过循环）；
    /// `until_tick` 是**开区间上界**（`0x3d62b4 cmp ecx,[rsp]` + `0x3d62b7 jae`）。
    pub fn inject_events<T: ReplayInjectTarget>(
        &mut self,
        from: usize,
        until_tick: u32,
        target: &mut T,
    ) {
        // 0x3d61c6/0x3d621b：先各取一次模块（原版 get_module，取不到就 expect panic）。
        // 我方由 target 承担这两次取模块。
        let mut i = from;
        while i < self.events.len() {
            let tick = self.events[i].tick;
            // 0x3d62b4：`event.tick >= until_tick` ⇒ 停（严格小于才注入）。
            if tick < 0 || tick as u64 >= until_tick as u64 {
                break;
            }
            let tick = tick as u32;
            // 0x3d62c5 / 0x3d62d4：记下"开始过"与当前下标。
            self.started = 1;
            self.cursor = i;
            // 0x3d62e0：GraphwarEvent::clone（逐条克隆，后面交出去的是副本）。
            let ev = self.events[i].clone();
            // 0x3d634d / 0x3d6376：远端事件处理器 + 事件泵。
            target.dispatch_replay_event(&ev);
            // 0x3d6395：状态字节 < 2 才同步 tick；阈值 2 的语义**未知**。
            if target.game_state_byte() < GAME_STATE_GATE_LIMIT {
                // 0x3d63ad：写 TickHandler 的 u32 tick。
                target.set_tick_handler_tick(tick);
            }
            // 0x3d6284：播放器自己的当前 tick。
            self.tick = tick;
            i += 1;
        }
    }

    /// `go_to_first_checkpoint`（x86 **`0x3d6a40`** / `0x162`；arm64 `0x3d94a0`）。
    ///
    /// 目标 tick = `checkpoints.first().unwrap_or(0)`（`0x3d6a93` 判空 ⇒ `0x3d6b20 xor edx,edx`；
    /// 否则 `0x3d6aa2 mov edx,[rax]` 取首项），把 TickHandler 的 tick 设成它
    /// （`0x3d6aab mov [rbx+0x58],edx`），再从 **0 号事件**注入到该 tick（`0x3d6ab1 xor esi,esi`）。
    pub fn go_to_first_checkpoint<T: ReplayInjectTarget>(&mut self, target: &mut T) {
        let tick = self.checkpoints.first().copied().unwrap_or(0);
        target.set_tick_handler_tick(tick);
        self.inject_events(0, tick, target);
        self.freeze_tick_clock_if_running(target);
    }

    /// `go_to_next_checkpoint`（x86 **`0x3d68b0`** / `0x186`；arm64 `0x3d929c`）。
    ///
    /// 1. 在 `checkpoints` 上二分找**第一个严格大于当前 tick** 的检查点
    ///    （`0x3d68cf`..`0x3d6930`：二分 + `0x3d6902 je` 命中"相等就往后挪一格"的修正；
    ///    等价于 `upper_bound(tick)`）；找不到（`0x3d6914 jae`）就**不注入**。
    /// 2. 起点 = `if started != 0 { cursor + 1 } else { 0 }`
    ///    （`0x3d6919 inc rax` + `0x3d6922 cmp byte [rdi],0` + `0x3d6925 cmovne`）。
    /// 3. 注入到该检查点的 tick（`0x3d692c call inject_events`）。
    /// 4. 冻结 TickHandler 的时钟（`0x3d697a`..`0x3d69a8`）。
    pub fn go_to_next_checkpoint<T: ReplayInjectTarget>(&mut self, target: &mut T) {
        // upper_bound：第一个 `> self.tick` 的检查点。
        let next = self.checkpoints.iter().position(|&c| c > self.tick);
        if let Some(idx) = next {
            // 0x3d6919/0x3d6922/0x3d6925：起点 = 已注入过则 cursor+1，否则 0。
            let from = if self.started != 0 { self.cursor + 1 } else { 0 };
            let until = self.checkpoints[idx];
            self.inject_events(from, until, target);
        }
        self.freeze_tick_clock_if_running(target);
    }

    /// `go_to_previous_checkpoint`（x86 **`0x3d6bb0`** / `0x1bc`；arm64 `0x3d9658`）。
    ///
    /// 1. 找**最后一个严格小于当前 tick** 的检查点（等价 `rposition(< tick)`；
    ///    二进制里是 `upper_bound` 后再 `cmp rdx,1; adc rdx,-1` = `saturating_sub(1)`，
    ///    见 `0x3d6c4c`..`0x3d6c5b`）；找不到（`0x3d6c60 jae`）就**不注入**。
    /// 2. 把 TickHandler 的 tick 写成 **`checkpoints[0]`**（`0x3d6c78 mov eax,[rax]` +
    ///    `0x3d6c7a mov [rbx+0x58],eax`）—— 注意**不是**目标检查点；
    ///    为什么是首项：**未知**（只观测到"写首项"这一条指令，不编语义）。
    /// 3. 从 **0 号事件**注入到该检查点的 tick（`0x3d6c88 xor esi,esi`）。
    /// 4. 冻结 TickHandler 时钟。
    pub fn go_to_previous_checkpoint<T: ReplayInjectTarget>(&mut self, target: &mut T) {
        let prev = self.checkpoints.iter().rposition(|&c| c < self.tick);
        if let Some(idx) = prev {
            // 0x3d6c78：写的是**首项**（不是 checkpoints[idx]）。
            let first = self.checkpoints[0];
            target.set_tick_handler_tick(first);
            let until = self.checkpoints[idx];
            self.inject_events(0, until, target);
        }
        self.freeze_tick_clock_if_running(target);
    }

    /// 三条 checkpoint 路径的共同尾巴：时间基**未冻结**（哨兵 `nanos == 1e9`）时才冻结。
    ///
    /// 逐指令：x86 `0x3d697a cmp dword [rbx+0x40],0x3b9aca00` / `0x3d69a8`；
    /// arm64 `0x3d939c ldr w9,[x19,#0x40]` + `0x3d93a0/0x3d93a8 mov w10,#0x3b9aca00`
    /// + `0x3d93b0 cmp` + `0x3d93c8 bl <Instant>::now` + `0x3d93d0/0x3d93d4` 写 `+0x38/+0x40`。
    fn freeze_tick_clock_if_running<T: ReplayInjectTarget>(&self, target: &mut T) {
        if target.tick_handler_base_nanos() == crate::graphwar_tick_handler::NANOS_PER_SECOND {
            target.freeze_tick_handler();
        }
    }
}

#[cfg(test)]
mod replay_player_orig_tests {
    use super::*;

    /// 记录"注入了什么、tick 被写成什么"的假目标。
    struct Recorder {
        dispatched: Vec<i64>,
        tick_handler_tick: Vec<u32>,
        state_byte: u8,
        base_nanos: u32,
        freezes: usize,
    }

    impl Recorder {
        fn new(state_byte: u8) -> Self {
            Self {
                dispatched: Vec::new(),
                tick_handler_tick: Vec::new(),
                state_byte,
                // 哨兵 1e9 = 未冻结（`src/graphwar_tick_handler.rs` 的 NANOS_PER_SECOND）
                base_nanos: crate::graphwar_tick_handler::NANOS_PER_SECOND,
                freezes: 0,
            }
        }
    }

    impl ReplayInjectTarget for Recorder {
        fn dispatch_replay_event(&mut self, ev: &ReplayEvent) {
            self.dispatched.push(ev.tick);
        }
        fn game_state_byte(&self) -> u8 {
            self.state_byte
        }
        fn set_tick_handler_tick(&mut self, tick: u32) {
            self.tick_handler_tick.push(tick);
        }
        fn tick_handler_base_nanos(&self) -> u32 {
            self.base_nanos
        }
        fn freeze_tick_handler(&mut self) {
            self.freezes += 1;
            self.base_nanos = 123_456_789; // 非哨兵 = 已冻结
        }
    }

    fn ev(tick: i64) -> ReplayEvent {
        ReplayEvent { tick, kind: "TickEvent".into(), body: String::new() }
    }

    fn player(ticks: &[i64], checkpoints: &[u32]) -> GraphwarReplayPlayer {
        GraphwarReplayPlayer {
            started: 0,
            cursor: 0,
            events: ticks.iter().map(|&t| ev(t)).collect(),
            checkpoints: checkpoints.to_vec(),
            tick: 0,
        }
    }

    /// `inject_events` 的上界是**开区间**：`0x3d62b4 cmp ecx,[rsp]` + `jae` ⇒ `tick == until` 不注入。
    #[test]
    fn inject_events_stops_before_until_tick() {
        let mut p = player(&[10, 20, 30, 40], &[]);
        let mut t = Recorder::new(0);
        p.inject_events(0, 30, &mut t);
        assert_eq!(t.dispatched, vec![10, 20], "tick 30 是上界，不含");
        assert_eq!(p.tick, 20);
        assert_eq!(p.cursor, 1, "cursor = 最后注入的下标");
        assert_eq!(p.started, 1);
    }

    /// `GameStateManager+0x138 < 2` 才同步 TickHandler 的 tick（`0x3d6395` / `0x3d639c jae`）。
    #[test]
    fn tick_handler_sync_is_gated_by_state_byte() {
        let mut p = player(&[10, 20], &[]);
        let mut t = Recorder::new(0); // 0 < 2 ⇒ 同步
        p.inject_events(0, 100, &mut t);
        assert_eq!(t.tick_handler_tick, vec![10, 20]);

        let mut p = player(&[10, 20], &[]);
        let mut t = Recorder::new(2); // 2 不小于 2 ⇒ 不同步
        p.inject_events(0, 100, &mut t);
        assert_eq!(t.dispatched, vec![10, 20]);
        assert!(t.tick_handler_tick.is_empty());
    }

    /// `from >= events.len()` 一圈都不跑（`0x3d624b jae`）。
    #[test]
    fn inject_events_from_past_end_does_nothing() {
        let mut p = player(&[10], &[]);
        let mut t = Recorder::new(0);
        p.inject_events(1, 1000, &mut t);
        assert!(t.dispatched.is_empty());
        assert_eq!(p.started, 0);
    }

    /// 首检查点：空表 ⇒ tick 0（`0x3d6b20 xor edx,edx`）；非空 ⇒ `checkpoints[0]`（`0x3d6aa2`）。
    #[test]
    fn first_checkpoint_uses_head_or_zero() {
        let mut p = player(&[5, 50, 500], &[100, 300]);
        let mut t = Recorder::new(0);
        p.go_to_first_checkpoint(&mut t);
        assert_eq!(t.tick_handler_tick.first(), Some(&100));
        assert_eq!(t.dispatched, vec![5, 50], "注入到 100 之前");
        assert_eq!(t.freezes, 1, "尾部冻结一次");

        let mut p = player(&[5, 50], &[]);
        let mut t = Recorder::new(0);
        p.go_to_first_checkpoint(&mut t);
        assert_eq!(t.tick_handler_tick.first(), Some(&0));
        assert!(t.dispatched.is_empty(), "until=0 ⇒ 一条都不注入");
        assert_eq!(t.freezes, 1);
    }

    /// 「下一个检查点」= 第一个**严格大于**当前 tick 的检查点；起点 = 已注入过则 cursor+1。
    #[test]
    fn next_checkpoint_is_upper_bound_and_resumes_after_cursor() {
        let mut p = player(&[5, 50, 150, 250, 350], &[100, 200, 300]);
        p.tick = 200; // 正好在一个检查点上 ⇒ 目标应是 300（严格大于）
        p.started = 1;
        p.cursor = 1; // 已注入到下标 1（tick 50）
        let mut t = Recorder::new(0);
        p.go_to_next_checkpoint(&mut t);
        assert_eq!(t.dispatched, vec![150, 250], "从 cursor+1=2 起，注入到 300 之前");
        assert_eq!(t.freezes, 1);
    }

    /// 没有更大的检查点：不注入、不写 tick（`0x3d6914 jae`），但仍然冻结时钟。
    #[test]
    fn next_checkpoint_absent_still_freezes_clock() {
        let mut p = player(&[5, 50, 150], &[100, 200]);
        p.tick = 900;
        let mut t = Recorder::new(0);
        p.go_to_next_checkpoint(&mut t);
        assert!(t.dispatched.is_empty());
        assert!(t.tick_handler_tick.is_empty());
        assert_eq!(t.freezes, 1);
    }

    /// 「上一个检查点」= 最后一个**严格小于**当前 tick 的检查点；TickHandler 的 tick 写成**首项**。
    #[test]
    fn previous_checkpoint_writes_head_tick_not_target() {
        let mut p = player(&[50, 150, 250, 350], &[100, 200, 300]);
        p.tick = 250;
        let mut t = Recorder::new(0);
        p.go_to_previous_checkpoint(&mut t);
        // 目标 = 200（严格小于 250 的最后一个）；TickHandler tick 写首项 100（0x3d6c78）
        assert_eq!(t.tick_handler_tick.first(), Some(&100), "写的是 checkpoints[0]");
        assert_eq!(t.dispatched, vec![50, 150], "从 0 号事件注入到 200 之前");
        assert_eq!(t.freezes, 1);
    }

    /// 当前 tick 早于所有检查点：没有"上一个"，不注入，但仍冻结。
    #[test]
    fn previous_checkpoint_absent_still_freezes_clock() {
        let mut p = player(&[5, 50], &[100, 200]);
        p.tick = 10;
        let mut t = Recorder::new(0);
        p.go_to_previous_checkpoint(&mut t);
        assert!(t.dispatched.is_empty());
        assert!(t.tick_handler_tick.is_empty());
        assert_eq!(t.freezes, 1);
    }

    /// 已冻结（`nanos != 1e9`）时**不再**冻结（`0x3d6981 jne` 跳过整段）。
    #[test]
    fn already_frozen_clock_is_not_frozen_again() {
        let mut p = player(&[5], &[100]);
        let mut t = Recorder::new(0);
        t.base_nanos = 42; // 非哨兵 = 已冻结
        p.go_to_first_checkpoint(&mut t);
        assert_eq!(t.freezes, 0);
    }

    /// 元素步长与 tick 偏移是反汇编里的立即数（`0x50` / `0x48`）。
    #[test]
    fn event_stride_and_tick_offset_match_the_binary() {
        assert_eq!(REPLAY_EVENT_STRIDE, 0x50);
        assert_eq!(REPLAY_EVENT_TICK_OFFSET, 0x48);
        assert_eq!(GAME_STATE_GATE_LIMIT, 2);
        assert_eq!(TICK_HANDLER_TICK_OFFSET, 0x58);
    }
}

// ---------------------------------------------------------------------------
// 对外 C ABI（供 ctypes / 离线对拍；GDScript 不走这条路）
// ---------------------------------------------------------------------------

use std::os::raw::c_char;

/// 缓冲区不足（与 `src/_scaffold/capi.rs` 的约定一致）。
pub const E_BUFFER_SMALL: i64 = -1;
/// 参数非法（空指针 / 非 UTF-8 / 文件打不开）。
pub const E_BAD_ARGS: i64 = -2;

fn c_path(path: *const c_char) -> Option<std::path::PathBuf> {
    if path.is_null() {
        return None;
    }
    let s = unsafe { std::ffi::CStr::from_ptr(path) }.to_str().ok()?;
    Some(std::path::PathBuf::from(s))
}

/// 一局回放里的事件条数。`-1` = 文件打不开 / 解压失败；`0` = 文件能解但**一局都没记录**
/// （真实语料里就有 2 份这样的空回放，`docs/133` §2.2）。
///
/// # Safety
/// `path` 必须是有效的 NUL 结尾 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn gw2_replay_event_count(path: *const c_char) -> i64 {
    let Some(path) = c_path(path) else { return -1 };
    match Replay::load(&path) {
        Ok(r) => r.events.len() as i64,
        // 空回放（gzip 有效、0 字节正文）不是错误，本局就是 0 条事件
        Err(ReplayError::Empty) => 0,
        Err(_) => -1,
    }
}

/// 一局回放的规范性状态快照（见 [`ReplayState::canonical`]），写进 `out`。
///
/// 返回写出的字节数；`-1` = 缓冲区不足（需要的长度写在 `out` 的前 8 字节？不——
/// 保持简单：调用方给够 64 KiB，回放快照实测 < 8 KiB），`-2` = 参数/文件非法。
///
/// # Safety
/// `path` 有效 NUL 结尾 C 字符串；`out` 至少 `cap` 字节可写。
#[no_mangle]
pub unsafe extern "C" fn gw2_replay_state_at(
    path: *const c_char,
    tick: i64,
    out: *mut c_char,
    cap: usize,
) -> i64 {
    if out.is_null() || cap == 0 {
        return E_BAD_ARGS;
    }
    let replay = match c_path(path).and_then(|p| Replay::load(&p).ok()) {
        Some(r) => r,
        None => return E_BAD_ARGS,
    };
    let text = replay.state_at(tick).canonical();
    let bytes = text.as_bytes();
    if bytes.len() + 1 > cap {
        return E_BUFFER_SMALL;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    bytes.len() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一段**真实格式**的回放（明文 JSONL，未压缩）：Level 0、NormalFunction、
    /// 1 个士兵 + 1 个靶子、两发、靶子在 tick 2169 被打掉。
    /// 内容取自在场机器上的真实回放 `1793897298`，这里只保留结构所需的事件。
    const SAMPLE: &str = concat!(
        r#"{"event":{"GameStateInfo":{"game_state":"Setup"}},"tick":2}"#,
        "\n",
        r#"{"event":{"FunctionModeInfo":{"function_mode":"NormalFunction"}},"tick":2}"#,
        "\n",
        r#"{"event":{"PlayerInfo":{"player_id":229,"connection_id":228}},"tick":5}"#,
        "\n",
        r#"{"event":{"SoldierInfo":{"soldier_id":230,"player_id":229}},"tick":5}"#,
        "\n",
        r#"{"event":{"TeamInfo":{"entity_id":230,"team":"Team1"}},"tick":5}"#,
        "\n",
        r#"{"event":{"TargetInfo":{"target_id":232}},"tick":6}"#,
        "\n",
        r#"{"event":{"TeamInfo":{"entity_id":232,"team":"Team2"}},"tick":6}"#,
        "\n",
        r#"{"event":{"ClearObstacles":{}},"tick":6}"#,
        "\n",
        r#"{"event":{"AddObstacle":{"pos_x":2.5,"pos_y":-1.5,"radius":1.25}},"tick":6}"#,
        "\n",
        r#"{"event":{"PosInfo":{"entity_id":230,"pos_x":-5.156306989169929,"pos_y":-9.889980889323086,"radius":0.5}},"tick":6}"#,
        "\n",
        r#"{"event":{"PosInfo":{"entity_id":232,"pos_x":9.782072976693868,"pos_y":4.595393963132992,"radius":0.5}},"tick":6}"#,
        "\n",
        r#"{"event":{"LifeInfo":{"entity_id":230,"alive":true}},"tick":6}"#,
        "\n",
        r#"{"event":{"LifeInfo":{"entity_id":232,"alive":true}},"tick":6}"#,
        "\n",
        r#"{"event":{"NameInfo":{"entity_id":229,"name":"cmdgz"}},"tick":6}"#,
        "\n",
        r#"{"event":{"FunctionInfo":{"owner_id":230,"function":"y+0.1","corrected_function":null}},"tick":549}"#,
        "\n",
        r#"{"event":{"FunctionFire":{"soldier_id":230}},"tick":550}"#,
        "\n",
        r#"{"event":{"FunctionPoints":{"owner_id":230,"start_x":-5.156306989169929,"start_y":-9.889980889323086,"end_x":2.5793570733273476}},"tick":550}"#,
        "\n",
        r#"{"event":{"TurnCountInfo":{"turn_count":1}},"tick":1341}"#,
        "\n",
        r#"{"event":{"FunctionInfo":{"owner_id":230,"function":"-y+x","corrected_function":"-y+x"}},"tick":1485}"#,
        "\n",
        r#"{"event":{"FunctionPoints":{"owner_id":230,"start_x":-5.156306989169929,"start_y":-9.889980889323086,"end_x":10.359005510824831}},"tick":1485}"#,
        "\n",
        r#"{"event":{"EffectInfo":{"effect_id":233,"pos_x":10.359005510824831,"pos_y":6.0,"effect_type":"ExplosionEffect","start_tick":1500}},"tick":1490}"#,
        "\n",
        r#"{"event":{"LifeInfo":{"entity_id":232,"alive":false}},"tick":2169}"#,
        "\n",
        r#"{"event":{"TurnCountInfo":{"turn_count":2}},"tick":2276}"#,
        "\n",
        r#"{"event":{"GameStateInfo":{"game_state":"EndingGame"}},"tick":2276}"#,
        "\n",
    );

    fn sample_replay() -> Replay {
        Replay::parse_bytes(SAMPLE.as_bytes()).expect("样例必须能解析")
    }

    #[test]
    fn parses_plain_jsonl() {
        let r = sample_replay();
        assert_eq!(r.events.len(), 24);
        assert!(!r.gzipped);
        assert_eq!(r.skipped_lines, 0);
        assert_eq!(r.events[0].tick, 2);
        assert_eq!(r.events[0].kind, "GameStateInfo");
        assert_eq!(r.events[0].body, r#""game_state":"Setup""#);
        assert_eq!(r.tick_range(), (2, 2276));
    }

    #[test]
    fn global_tables_follow_docs_79() {
        let r = sample_replay();
        // NameInfo.entity_id 就是 player_id；SoldierInfo 把士兵映射到玩家
        assert_eq!(r.names.get(&229).map(|s| s.as_str()), Some("cmdgz"));
        assert_eq!(r.soldier_owner.get(&230), Some(&229));
        assert!(r.targets.contains(&232));
    }

    #[test]
    fn state_before_and_after_death() {
        let r = sample_replay();
        let before = r.state_at(2168);
        assert_eq!(before.alive.get(&232), Some(&true), "2168 靶子还活着");
        assert_eq!(before.tick, 2168);
        assert_eq!(before.obstacles.len(), 1);
        assert_eq!(before.obstacles[0].x, 2.5);
        assert_eq!(before.turn_count, 1);
        assert_eq!(before.shots.len(), 2);
        assert_eq!(before.functions.get(&230).map(|s| s.as_str()), Some("-y+x"));

        let after = r.state_at(2169);
        assert_eq!(after.alive.get(&232), Some(&false), "2169 靶子被打掉");
        assert_eq!(after.game_name, String::new(), "这份样例没有 GameNameInfo");
        assert_eq!(after.turn_count, 1, "turn_count 要到 2276 才变");

        let end = r.state_at(2276);
        assert_eq!(end.turn_count, 2);
        assert_eq!(end.game_state, "EndingGame");
    }

    #[test]
    fn canonical_is_stable_and_formatted() {
        let r = sample_replay();
        let c = r.state_at(2169).canonical();
        assert!(c.starts_with("tick=2169|O=2.500000,-1.500000,1.250000|E="), "实际 {c}");
        assert!(c.contains("230,Team1,1,-5.156307,-9.889981,0.500000"), "实际 {c}");
        assert!(c.contains("232,Team2,0,9.782073,4.595394,0.500000"), "实际 {c}");
        assert!(c.contains("|F=230=-y+x"), "实际 {c}");
        // 三端同格式：字段顺序固定、无多余空白
        assert_eq!(c, r.state_at(2169).canonical());
    }

    #[test]
    fn unknown_event_is_passed_through_verbatim() {
        // 协议里没有的事件（这里是编的）必须原样留在 body 里，不猜字段
        let line = r#"{"event":{"FutureEvent":{"weird_key":1.5}},"tick":7}"#;
        let ev = parse_line(line).expect("未知事件也要解析出来");
        assert_eq!(ev.kind, "FutureEvent");
        assert_eq!(ev.body, r#""weird_key":1.5"#);
        assert_eq!(ev.get_f64("weird_key"), Some(1.5));
        assert_eq!(ev.to_json(), r#"{"FutureEvent":{"weird_key":1.5}}"#);
    }

    #[test]
    fn empty_replay_is_zero_events_not_an_error() {
        // `1f 8b 08 00 00 00 00 00 00 ff 03 00 00 00 00 00 00 00 00 00`：gzip 空正文。
        // 真实语料里有 2 份（1782674520 / 1782761998），`docs/133` §2.2。
        const EMPTY_GZ: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x03, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(Replay::parse_bytes(EMPTY_GZ).unwrap_err(), ReplayError::Empty);
        // ABI 层把它算成 0 条，而不是"读不了"
        let dir = std::env::temp_dir().join("gw2-replay-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.replay");
        std::fs::write(&path, EMPTY_GZ).unwrap();
        let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { gw2_replay_event_count(c.as_ptr()) }, 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bad_lines_are_skipped_not_fatal() {
        let text = format!("{}\nnot json\n\n", SAMPLE);
        let r = Replay::parse_bytes(text.as_bytes()).unwrap();
        assert_eq!(r.skipped_lines, 1);
        assert_eq!(r.events.len(), 24);
        assert!(Replay::parse_bytes(b"").is_err());
        assert!(Replay::parse_bytes(b"\x1f\x8b\x08\x00broken").is_err());
    }

    #[test]
    fn gzip_input_parses() {
        // 同一份样例压成 gzip（Python: gzip.compress(SAMPLE.encode())），
        // 走 gunzip 这条路；解出来必须与明文完全一致。
        let gz = gzip_sample();
        let r = Replay::parse_bytes(&gz).expect("gzip 回放必须能解");
        assert!(r.gzipped);
        assert_eq!(r.events.len(), 24);
        assert_eq!(r.state_at(2169).canonical(), sample_replay().state_at(2169).canonical());
    }

    /// 由 `tools` 之外生成的一段真实 gzip（见 `docs/133` §5.1 的复现命令）。
    fn gzip_sample() -> Vec<u8> {
        GZ_SAMPLE.to_vec()
    }

    /// `gzip.compress(SAMPLE.encode(), mtime=0)` 的字节（1536 字节原文 → 464 字节）。
    const GZ_SAMPLE: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xbd, 0x95, 0x4d, 0x8f, 0xda,
        0x30, 0x10, 0x86, 0xef, 0xfd, 0x19, 0xbe, 0x2e, 0x44, 0xfe, 0x88, 0x13, 0x9b, 0x5b, 0xb5,
        0x62, 0xab, 0x4a, 0xed, 0x16, 0x89, 0xbd, 0xa3, 0x6c, 0xe2, 0xa0, 0xa8, 0xc1, 0x46, 0x89,
        0xb3, 0x25, 0x45, 0xfc, 0xf7, 0x4e, 0xc8, 0x27, 0x5b, 0xbc, 0x82, 0x1e, 0x7a, 0x89, 0xe2,
        0xb1, 0xdf, 0x67, 0xc6, 0x93, 0x79, 0xe1, 0x88, 0xd4, 0x9b, 0xd2, 0x16, 0x2d, 0x8e, 0xe8,
        0x4b, 0xb4, 0x53, 0x6b, 0x1b, 0x59, 0xf5, 0x55, 0xa7, 0xa6, 0x09, 0x6c, 0x21, 0xb0, 0x29,
        0x9b, 0x08, 0x5a, 0xa0, 0xb5, 0xb2, 0xd5, 0x1e, 0x9d, 0x4e, 0x33, 0x64, 0xb3, 0xf8, 0x27,
        0x5a, 0xd0, 0xd3, 0xa7, 0xe3, 0xa8, 0x7d, 0xaa, 0x74, 0x6c, 0x33, 0xa3, 0xbf, 0x9b, 0x64,
        0x90, 0xa7, 0x5d, 0x6c, 0xb3, 0x83, 0x20, 0x10, 0x9e, 0x4d, 0xb1, 0x8b, 0xf2, 0xfe, 0xa4,
        0x13, 0xb5, 0xca, 0xa3, 0x5a, 0x15, 0x3d, 0x64, 0x7f, 0x5e, 0x6d, 0xb2, 0x04, 0x8e, 0x51,
        0x39, 0x43, 0xb1, 0xd1, 0x5a, 0xb5, 0xd8, 0x36, 0x26, 0x46, 0x0e, 0xbf, 0xe0, 0xac, 0x4d,
        0x9e, 0x64, 0x23, 0xa8, 0x6c, 0x97, 0xad, 0x8a, 0xe1, 0xd9, 0x3b, 0xb2, 0x8b, 0xf2, 0xa2,
        0xa2, 0x5d, 0x8f, 0x80, 0x50, 0x66, 0xeb, 0x91, 0x60, 0x61, 0x0f, 0xee, 0xd5, 0x1c, 0x21,
        0xc8, 0x09, 0x88, 0x8a, 0xad, 0xb2, 0x3d, 0xc2, 0x9e, 0x57, 0x1d, 0x82, 0x8e, 0x9a, 0xe0,
        0xb6, 0xa4, 0xf4, 0x22, 0x29, 0x45, 0x2e, 0xc0, 0x63, 0xae, 0xa2, 0xe2, 0xc7, 0x2b, 0x7c,
        0xbb, 0x38, 0x57, 0x25, 0x44, 0x5c, 0x07, 0x3f, 0x27, 0x49, 0x7f, 0xec, 0xdc, 0x6d, 0x53,
        0x6e, 0x0e, 0x90, 0xc8, 0xe3, 0xb3, 0xf3, 0x7b, 0x8d, 0x16, 0x73, 0xd2, 0x2c, 0x8a, 0x28,
        0xc9, 0x2a, 0x00, 0x11, 0x8f, 0x72, 0x17, 0x6b, 0x65, 0x4a, 0x57, 0xa7, 0x3a, 0xee, 0x9c,
        0x7b, 0x84, 0x07, 0x0c, 0x07, 0x52, 0x48, 0x12, 0x48, 0xd9, 0x7c, 0xd0, 0x3e, 0x8d, 0xf4,
        0x84, 0x90, 0x52, 0x60, 0x78, 0x32, 0xd0, 0x88, 0x60, 0x4c, 0x8a, 0xbd, 0xbb, 0x73, 0xd2,
        0x21, 0xa7, 0xf4, 0x42, 0x41, 0x71, 0x48, 0x65, 0x18, 0x04, 0x92, 0x89, 0x40, 0x0c, 0x29,
        0x7d, 0x8f, 0x4b, 0xce, 0x24, 0x93, 0x01, 0x23, 0x8c, 0x42, 0x35, 0x37, 0x65, 0xfc, 0x96,
        0xa5, 0xca, 0x75, 0xcd, 0x28, 0xcf, 0xde, 0xa0, 0x8f, 0xb6, 0xa8, 0xd4, 0xdd, 0x72, 0x7a,
        0x93, 0xfc, 0x19, 0x2c, 0x79, 0x4d, 0xde, 0x74, 0x52, 0xc3, 0x1e, 0x4c, 0x46, 0xbc, 0x4b,
        0xb6, 0xbf, 0x9d, 0x93, 0xd1, 0xdb, 0xaf, 0x87, 0x98, 0x5f, 0x7a, 0x6a, 0x8a, 0xde, 0xb3,
        0xc0, 0xa9, 0x1f, 0xb0, 0x47, 0x50, 0xe3, 0xb8, 0xa2, 0x00, 0xc7, 0xa9, 0x64, 0x33, 0x6e,
        0xea, 0x2a, 0xcf, 0x27, 0x03, 0xef, 0xcb, 0xab, 0x39, 0x9e, 0xb2, 0x42, 0x5d, 0xb1, 0xde,
        0x44, 0xc8, 0xf1, 0x55, 0xe1, 0xca, 0x64, 0xda, 0x96, 0x57, 0xca, 0x83, 0x49, 0x2d, 0xac,
        0x6b, 0x92, 0xda, 0x4d, 0xc7, 0x2c, 0x29, 0x9d, 0x74, 0x93, 0x1d, 0x4a, 0xc6, 0x43, 0x1c,
        0x32, 0x46, 0x43, 0xe6, 0x87, 0x81, 0xbb, 0x9a, 0x97, 0xaa, 0xd0, 0x8f, 0xa6, 0xd2, 0xa3,
        0x79, 0x21, 0xb0, 0x89, 0x9b, 0x08, 0x18, 0x61, 0xd4, 0x11, 0xe6, 0x93, 0x7f, 0xec, 0xf1,
        0xbc, 0x7e, 0x38, 0x38, 0x5a, 0xdc, 0xee, 0x4d, 0xb2, 0xf8, 0x82, 0xff, 0xe7, 0x66, 0x11,
        0xec, 0x31, 0x2e, 0x31, 0xe6, 0x9c, 0x60, 0x41, 0x7d, 0xc1, 0xc8, 0x07, 0xe5, 0x2c, 0xd3,
        0x14, 0xae, 0x30, 0xcc, 0xe6, 0x79, 0xd5, 0xd5, 0xc2, 0x06, 0x33, 0xfe, 0x4d, 0x1c, 0xdc,
        0x18, 0x78, 0x50, 0x72, 0x27, 0xb3, 0xf5, 0xbe, 0x99, 0xe4, 0xe5, 0x61, 0x9f, 0x9b, 0x12,
        0xae, 0xd8, 0xb2, 0x51, 0x5f, 0x75, 0x57, 0x01, 0xc7, 0x78, 0x5a, 0x8f, 0xc4, 0xf7, 0x1a,
        0x2d, 0x8d, 0xf2, 0x72, 0xe2, 0x34, 0x0a, 0xbd, 0xb9, 0x7d, 0x00, 0x26, 0x3f, 0xde, 0x94,
        0x86, 0x97, 0x26, 0xfb, 0xf8, 0x9f, 0x74, 0xa9, 0x93, 0x4c, 0x6f, 0x9b, 0x33, 0xe8, 0x3d,
        0xe3, 0x0f, 0xa6, 0xc0, 0x85, 0x0d, 0x8e, 0x07, 0x00, 0x00,
    ];

    #[test]
    fn player_advances_by_tick_and_seek_is_exact() {
        let mut p = ReplayPlayer::new(sample_replay());
        assert_eq!(p.current_tick(), 2);
        assert_eq!(p.last_tick(), 2276);
        assert!(!p.playing);

        // 逐 tick 推进：2168 前靶子活着，2169 起死
        let applied = p.advance_to(2168);
        assert!(!applied.is_empty());
        assert_eq!(p.state.alive.get(&232), Some(&true));
        p.advance_to(2169);
        assert_eq!(p.state.alive.get(&232), Some(&false));

        // 跳回去必须与 state_at 完全一致（从头重放，没有增量漂移）
        p.seek(1000);
        assert_eq!(p.state.canonical(), p.replay.state_at(1000).canonical());
        assert_eq!(p.current_tick(), 1000);

        // 进度：首 tick 0、末尾 1
        p.seek(2);
        assert!(p.progress() < 0.001);
        p.seek(2276);
        assert!((p.progress() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn player_wall_clock_is_sixty_ticks_per_second() {
        let mut p = ReplayPlayer::new(sample_replay());
        p.play();
        p.seek(100);
        // 0.5 秒 × 60 tick/s = 30 tick
        p.advance_dt(0.5);
        assert_eq!(p.current_tick(), 130);
        // 倍速 2×
        p.speed = 2.0;
        p.advance_dt(0.5);
        assert_eq!(p.current_tick(), 190);
        // 暂停时不动
        p.pause();
        p.advance_dt(1.0);
        assert_eq!(p.current_tick(), 190);
        // 走到底自动停
        p.play();
        p.advance_dt(1.0);
        assert!(p.current_tick() <= p.last_tick());
    }

    #[test]
    fn step_event_buttons() {
        let mut p = ReplayPlayer::new(sample_replay());
        // 从第一个 tick 出发：下一条未消费事件（tick 2 的已被消费？没有，seek 未跑过）
        p.seek(2);
        let next = p.step_event().expect("还有事件");
        assert!(next >= 5, "下一条事件的 tick 是 5（实际 {next}）");
        let back = p.step_back_event().expect("能往回一步");
        assert_eq!(back, 2);
    }

    #[test]
    fn c_abi_roundtrip() {
        // 写到工作区里的临时文件（不用 /tmp：本机 /tmp 是逐命令 tmpfs）
        let dir = std::env::temp_dir().join("gw2-replay-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.jsonl");
        std::fs::write(&path, SAMPLE).unwrap();
        let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).unwrap();

        assert_eq!(unsafe { gw2_replay_event_count(c.as_ptr()) }, 24);
        let mut buf = vec![0u8; 8192];
        let n = unsafe {
            gw2_replay_state_at(c.as_ptr(), 2169, buf.as_mut_ptr() as *mut c_char, buf.len())
        };
        assert!(n > 0);
        let text = std::str::from_utf8(&buf[..n as usize]).unwrap();
        assert_eq!(text, sample_replay().state_at(2169).canonical());

        // 缓冲区太小 → -1；坏参数 → -2
        let mut small = vec![0u8; 4];
        assert_eq!(
            unsafe {
                gw2_replay_state_at(c.as_ptr(), 2169, small.as_mut_ptr() as *mut c_char, 4)
            },
            E_BUFFER_SMALL
        );
        assert_eq!(unsafe { gw2_replay_event_count(std::ptr::null()) }, -1);
        let _ = std::fs::remove_file(&path);
    }
}
