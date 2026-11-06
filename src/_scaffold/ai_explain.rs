//! 把"**局面 → bot 选哪条函数**"这条链变成能读、能复算的东西（`docs/147`）。
//!
//! 这个模块**不引入新行为**：它把 `src/graphwar_function_evolutionator.rs` 里已经逐条对过地址的零件
//! （权重表 / 装配 / 变异 / 单父亲交叉 / 两阶段评分 / 截断）按**原版的实际顺序**跑一遍，
//! 并在每一步留下可打印的记录。用途只有一个：回答"为什么它选了这条函数"。
//!
//! # 决策链（vaddr；Ghidra = +0x100000）
//!
//! ```text
//! 0x479520  回合体：按 soldier key 去表里**取走**上次留下的种群（没有就是空 Vec），
//!    ↓       并从同一条表项的参数区读出 (代数, 种群/5, 每槽个体数, 复杂度上限)
//!    ↓       表项由 0x478fc0(self, key, 难度, 代数, 每槽=5, 上限) 写入（docs/147 §2）
//! 0x47a300  分派：function_mode==0 → A(0x51b440)，否则 B(0x51d080)（docs/110）
//! 0x51b440  变体 A 主循环（每代开头查一次 25 s 墙钟，0x51b7cf）
//!    ├─ 每槽：克隆上一个存活者；槽号 ≥ 上一代长度 ⇒ 随机新生（0x51b82c）
//!    ├─ 每槽：再来 per_slot-1 个工厂子代，**共用同一个父代**（0x4ee3a0 只有三个参数）
//!    ├─ 每个个体都**粗评** step=0.5（0x51bc8b / 0x51c0c1）
//!    ├─ 代末降序排序（0x51cb6f）→ 截断到 s = 种群/5（0x51c208）
//!    └─ 前 3 名精评 step=0.01（0x51c3b3 / 0x51c6ee / 0x51c9a8）
//!         判据不一致 ⇒ 整条换成"随机 + 补齐 + 变异"，**分数留旧值**
//! ```
//!
//! # 一个反直觉的事实：它的"估算"不是估算
//!
//! 适应度函数 `0x51a750` 里**没有**自己写一套近似：它 `call [0x9bacb0]` → `0x3c2bc0`，
//! 也就是**真正开火时用的那个结算函数**（`graphwar_shot_calculator`；全 `.so` 只有两个
//! 调用点：`0x471825` 真开火、`0x51a9a1` 这里）。那个函数内部 `call [0x9bac98]` →
//! `0x3c2530`（同一个弦长采样器），并按 config+0x38 把障碍光栅化进掩膜
//! （`0x3f11e0`，调用点 `0x3c3957`）。唯一的降级是**步长**：粗评 0.5、精评/真开火 0.01。
//!
//! # 证据等级
//!
//! * 公式、顺序、参数搬运：**观测**（逐条反汇编，见各函数注释里的地址）；
//! * 事件标签 0..4 的语义：**观测**（跳转表 `0x1c63a0` 只有 5 项 + 各分支指令）；
//! * "AI 的模拟沿用真开火那条路" ⇒ 必然也吃障碍掩膜：**推断（强）**
//!   —— 缺的直接证据是"AI 那份 config 的 +0x38 字节也是 1"（只在真开火侧读到过）；
//! * 本模块的输出是**回归保护**，不是复原证据（见 `AGENTS.md` §0.2）。

use crate::graphwar_function_evolutionator::{
    self, cross_from, mutate, random_function, Params, VarMode, COARSE_STEP, DIST_SENTINEL,
    FINE_RANKS, FINE_STEP,
};
use crate::math_function::to_text;
use crate::graphwar_path_calculator::{calculate, Mode};
use crate::plane::{is_occupied, rasterize, EMPTY};
use crate::_scaffold::rng::ChaCha12;
use crate::graphwar_shot_calculator::{correction, shifted_tokens};
use crate::{Function, Token};

/// 队伍字节"查不到"时的哨兵。
///
/// 反汇编 `0x51a9ec mov bpl,0x2`（射手侧）与 `0x51ac06 mov r15b,0x2`（被命中实体侧）：
/// 两边都先填 2，再去 `ctx+0x08` 那张 `HashMap<u32, u8>` 里查；查不到就保持 2。
pub const TEAM_UNKNOWN: u8 = 2;

// ---------------------------------------------------------------------------
// 局面
// ---------------------------------------------------------------------------

/// 场上的一个可命中实体。
///
/// `team` 的语义照搬 `ctx+0x08` 那张表的值域：`0`/`1` = 两方，`2` = 查不到（无主）。
/// 士兵与"靶"共用这个结构，用 [`Entity::is_target`] 区分 —— 依据是适应度的事件标签：
/// tag 2 记命中数（`0x51ae4c dec` / `0x51ae5d inc`），tag 3 只更新距离项
/// （`0x51aed6`–`0x51aee2` 取 min），两者分开。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entity {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    /// 0/1 = 两方；[`TEAM_UNKNOWN`] = 不属于任何人（训练靶、中立物）。
    pub team: u8,
    /// `true` ⇒ 命中它只更新距离项（tag 3）；`false` ⇒ 记进命中数（tag 2）。
    pub is_target: bool,
}

impl Entity {
    /// 一个士兵（会进命中数）。
    pub fn soldier(id: i64, x: f64, y: f64, radius: f64, team: u8) -> Self {
        Self { id, x, y, radius, team, is_target: false }
    }

    /// 一个靶（只影响距离项，不影响命中数）。
    pub fn target(id: i64, x: f64, y: f64, radius: f64, team: u8) -> Self {
        Self { id, x, y, radius, team, is_target: true }
    }
}

/// 一个局面：射手位置 + 障碍掩膜 + 场上实体 + 复杂度上限。
#[derive(Debug, Clone)]
pub struct Situation {
    pub x0: f64,
    pub y0: f64,
    /// 是否左右翻转（队 2 的射击方向；`docs/127` §2.1 的 `flip`）。
    pub flip: bool,
    /// 射手自己的队伍字节（进 `ctx+0x00` 那张表的那一份）。
    pub shooter_team: u8,
    /// 障碍掩膜（`plane::empty_mask()` 的 1000×600，`0xFF` = 空）。
    pub mask: Vec<u8>,
    pub entities: Vec<Entity>,
    /// 复杂度硬上限（`ctx+0x58`），来自难度档。
    pub limit: u32,
}

impl Situation {
    /// 一个没有任何障碍的空局面。
    pub fn empty(x0: f64, y0: f64, flip: bool, shooter_team: u8, limit: u32) -> Self {
        Self {
            x0,
            y0,
            flip,
            shooter_team,
            mask: crate::plane::empty_mask(),
            entities: Vec::new(),
            limit,
        }
    }

    /// 按 `(x, y, size)` 把障碍烧进掩膜（与真开火同一个光栅化器 `0x3f11e0`）。
    pub fn add_obstacle(&mut self, x: f64, y: f64, size: f64) {
        rasterize(&mut self.mask, x, y, size, 0x00);
    }

    /// 被障碍占掉的像素数（打印局面时用来确认掩膜真的非空）。
    pub fn occupied_pixels(&self) -> usize {
        self.mask.iter().filter(|b| **b != EMPTY).count()
    }
}

// ---------------------------------------------------------------------------
// 评分（照着 0x51a750 的尾部逐条实现）
// ---------------------------------------------------------------------------

/// 一次评分的完整结果。
///
/// 字段与评分核心 sret 的对应（`0x51b10d` / `0x51b11d` / `0x51b111`）：
/// `score` = `+0x00`、`dist` = `+0x08`、命中数 = `+0x10`。
/// 唯一的扩展是这里把**击杀**与**误伤**分开记，因为 `+0x10` 存的是两者之差。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    /// 排序键 = `hits*2e6 − dist − V²/100 − (V>limit ? 5e7 : 0)`。
    pub score: f64,
    /// 距离项：初值 [`DIST_SENTINEL`]，被 tag 3 事件取 min（`0x51aee2`）。
    pub dist: f64,
    /// 打死的**敌人**数。
    pub kills: u32,
    /// 打死的**自己人**数（`0x51ae4c` 的 `dec`）。
    pub friendly: u32,
}

impl Score {
    /// `+0x10` 那个整数 = 敌人 − 友军（可为负）。
    ///
    /// ⚠️ 这条与 `docs/AI.md` §4 的"不分敌我、友军项是死代码"**相反**：
    /// 反汇编 `0x51ae21`–`0x51ae5a` 明确分了两支；`docs/97` §3.1 的记法才对。
    pub fn net_hits(self) -> i64 {
        self.kills as i64 - self.friendly as i64
    }

    /// 什么也没打到的基准分（`−1e6 − V²/100 − 惩罚`）。
    pub fn is_empty_hit(self) -> bool {
        self.kills == 0 && self.friendly == 0 && self.dist >= DIST_SENTINEL
    }
}

/// 命中一个实体算"自己人"还是"敌人"。
///
/// 逐条对应 `0x51ae21`–`0x51ae4a`：
///
/// ```text
/// 51ae21  cmp  r15b,0x2          ; 实体侧队伍字节 == 2 ?
/// 51ae27  cmp  [rsp+0x18],0x2    ; 射手侧 == 2 ?
/// 51ae31  je   51ae4c            ; 都是 2  ⇒ 友军（dec）
/// 51ae33  jmp  51ae5d            ; 否则    ⇒ 敌人（inc）
/// 51ae41  xor  r15b,[rsp+0x18]   ; 两边都查得到 ⇒ 比最低位
/// 51ae46  test r15b,0x1
/// 51ae4a  jne  51ae5d            ; 不同 ⇒ 敌人
/// 51ae4c  dec                    ; 相同 ⇒ 友军
/// ```
pub fn is_friendly(entity_team: u8, shooter_team: u8) -> bool {
    if entity_team == TEAM_UNKNOWN {
        return shooter_team == TEAM_UNKNOWN;
    }
    if shooter_team == TEAM_UNKNOWN {
        return false;
    }
    (entity_team ^ shooter_team) & 1 == 0
}

/// 按给定步长评一次分（`step` = [`COARSE_STEP`] 或 [`FINE_STEP`]）。
pub fn evaluate(tokens: &[Token], sit: &Situation, step: f64) -> Score {
    let v = tokens.len() as u64;
    // V² 是整数平方再除 100（`0x51b0bd imul` + `0x51b0eb divsd`），−1e6 由 dist 的初值提供
    let mut base = -((v * v) as f64) / 100.0;
    if v as u32 > sit.limit {
        base -= 50_000_000.0; // 0x19f478
    }
    if tokens.is_empty() {
        return Score { score: base - DIST_SENTINEL, dist: DIST_SENTINEL, kills: 0, friendly: 0 };
    }

    let c = correction(tokens, sit.x0, sit.y0, sit.flip);
    let shifted = shifted_tokens(tokens, c);
    let points = calculate(&shifted, sit.x0, sit.y0, step, sit.flip, Mode::Normal);

    let mut dist = DIST_SENTINEL;
    let mut kills = 0u32;
    let mut friendly = 0u32;
    let mut done: Vec<i64> = Vec::new();

    for &(px, py) in &points {
        for e in &sit.entities {
            if done.contains(&e.id) {
                continue;
            }
            let d = ((px - e.x).powi(2) + (py - e.y).powi(2)).sqrt();
            if d > e.radius {
                continue;
            }
            done.push(e.id);
            if e.is_target {
                // tag 3：只有"不是自己人"的目标才更新距离项（`0x51aec3`–`0x51aed4`）
                if !is_friendly(e.team, sit.shooter_team) && d < dist {
                    dist = d;
                }
            } else if is_friendly(e.team, sit.shooter_team) {
                friendly += 1;
            } else {
                kills += 1;
            }
        }
        if is_occupied(&sit.mask, px, py) {
            break; // `0x3c3957` 光栅化进来的障碍把弹道截断
        }
    }

    let hits = kills as i64 - friendly as i64;
    Score { score: hits as f64 * 2_000_000.0 + base - dist, dist, kills, friendly }
}

// ---------------------------------------------------------------------------
// 进化过程的可读记录
// ---------------------------------------------------------------------------

/// 一个个体是怎么来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// 上一个存活者的精确克隆（`0x51b8c5` 的 `memcpy`）。
    Clone,
    /// 工厂子代：从**同一个父代**取一棵子树再变异（`0x4ee3a0`）。
    Factory,
    /// 槽号 ≥ 上一代长度 ⇒ 随机新生（`0x51b82c jbe 0x51b900`）。
    RandomParent,
    /// 工厂子代，但父代太短（`len < 20`）⇒ 直接随机重生（`0x4ee470`）。
    FactoryRebirth,
    /// 代末前 3 名精评不一致 ⇒ 整条换新（`0x51c3ea`）。
    Replaced,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Clone => "克隆",
            Origin::Factory => "工厂(交叉+变异)",
            Origin::RandomParent => "随机父代",
            Origin::FactoryRebirth => "工厂->随机重生",
            Origin::Replaced => "精评不一致->换新",
        }
    }
}

/// 一个候选函数的完整画像。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub tokens: Vec<Token>,
    /// `math_function::to_text` 的产物（`0x4ec160`，`docs/119` 逐字符对拍 1032/1032）。
    pub text: String,
    pub score: Score,
    pub origin: Origin,
    /// 属于哪个槽（0..s）。
    pub slot: usize,
}

impl Candidate {
    fn new(tokens: Vec<Token>, score: Score, origin: Origin, slot: usize) -> Self {
        let text = to_text(&tokens);
        Self { tokens, text, score, origin, slot }
    }

    /// 是否超复杂度上限（吃 −5e7）。
    pub fn over_limit(&self, limit: u32) -> bool {
        self.tokens.len() as u32 > limit
    }
}

/// 一代的记录。
#[derive(Debug, Clone)]
pub struct Generation {
    pub index: u32,
    /// 本代造了多少个个体（= 种群）。
    pub born: usize,
    /// 本代截断后的存活者数（= s）。
    pub survivors: usize,
    /// 本代冠军（截断后第一名）。
    pub champion: Candidate,
    /// 本代存活者的完整名单（降序，`len == survivors`）。
    pub ranking: Vec<Candidate>,
    /// 前 3 名里被"精评不一致"顶掉的槽位（升序）。
    pub replaced: Vec<usize>,
    /// 粗评 vs 精评的分歧细节（`(名次, 粗净命中, 精净命中, 粗 dist, 精 dist)`）。
    pub fine_disagreements: Vec<(usize, i64, i64, f64, f64)>,
}

/// 一次"思考"的完整记录。
#[derive(Debug, Clone)]
pub struct Report {
    pub params: Params,
    /// 若来源是关卡难度值，记下它（见 [`graphwar_function_evolutionator::difficulty_params`]）。
    pub difficulty: Option<u32>,
    pub mode: VarMode,
    pub generations: Vec<Generation>,
    /// 最终交出去的那条函数（= 最后一代的第一个存活者）。
    pub winner: Candidate,
    /// 总评分次数（原版每代 `种群 + 3` 次，`docs/97` §6）。
    pub evaluations: u64,
    /// 是否撞到墙钟预算（原版调用方无法得知这一点，`docs/93` §2.2）。
    pub truncated: bool,
}

impl Report {
    /// 一句话结论：它是靠什么赢的。
    pub fn why(&self) -> String {
        let w = &self.winner;
        let net = w.score.net_hits();
        let over = if w.over_limit(self.params.limit) { "（超上限，吃了 −5e7）" } else { "" };
        if net > 0 {
            format!(
                "打死 {} 名敌人、误伤 {} 名自己人（净 {}），距离项 {:.3}，{} 个 token{}",
                w.score.kills,
                w.score.friendly,
                net,
                w.score.dist,
                w.tokens.len(),
                over
            )
        } else if w.score.dist < DIST_SENTINEL {
            format!("没打死人，但擦到了靶（距离项 {:.3}）", w.score.dist)
        } else {
            format!(
                "谁都没打到（净命中 {}，距离项还是哨兵 1e6），只是同批里最不差的那条",
                net
            )
        }
    }
}

/// 进化的设置。
#[derive(Debug, Clone)]
pub struct Settings {
    pub params: Params,
    pub mode: VarMode,
    pub difficulty: Option<u32>,
    /// 只跑前 `n` 代（`None` = 按 `params.generations`）。
    /// 用途：难度 6 在真机上会被 25 s 截断，本地复算时可以只跑一段做对照。
    pub max_generations: Option<u32>,
    /// 墙钟预算（秒）。`None` = 不限时（测试与可复现演示用）。
    pub budget_secs: Option<f64>,
}

impl Settings {
    /// 按**关卡难度值**建设置（真链路：`bot_added_event` 载荷 +0x10）。
    pub fn from_difficulty(difficulty: u32, mode: VarMode) -> Self {
        Self {
            params: graphwar_function_evolutionator::difficulty_params(difficulty),
            mode,
            difficulty: Some(difficulty),
            max_generations: None,
            budget_secs: None,
        }
    }

    /// 按 `docs/AI.md` 的六档建设置（那是**实测**的一套参数组合，不是关卡难度值）。
    pub fn from_level(level: u8, mode: VarMode) -> Self {
        Self {
            params: graphwar_function_evolutionator::params(level),
            mode,
            difficulty: None,
            max_generations: None,
            budget_secs: None,
        }
    }
}

fn rng_from_seed(seed: u64) -> ChaCha12 {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&seed.to_le_bytes());
    ChaCha12::from_seed(&key)
}

/// 跑一次"思考"并记录全过程。
///
/// 与 `graphwar_function_evolutionator::evolve_with_budget` 的差别**只有"记录"**：循环顺序、每代个体数、
/// 截断、精评换新都照原版（`0x51b440`）。
pub fn explain(sit: &Situation, settings: &Settings, seed: u64) -> Report {
    let p = settings.params;
    let mode = settings.mode;
    let mut rng = rng_from_seed(seed);
    let mut next = move || rng.next_u64();

    let mut evaluations = 0u64;
    let per_slot = p.per_slot.max(1) as usize;
    let survivors = (p.population as usize / per_slot).max(1);
    let gens = settings.max_generations.unwrap_or(p.generations);
    let started = std::time::Instant::now();

    // 第 0 代的上一次 = 空（`0x479c9b` 的未命中分支）
    let mut prev: Vec<Candidate> = Vec::new();
    let mut history = Vec::new();
    let mut truncated = false;

    for g in 0..gens {
        // 原版每代顶部查一次墙钟（`0x51b7cf cmp $0x19`）；超时与跑满共用出口
        if let Some(limit) = settings.budget_secs {
            if started.elapsed().as_secs_f64() > limit {
                truncated = true;
                break;
            }
        }

        let mut born: Vec<Candidate> = Vec::with_capacity(p.population as usize);
        for slot in 0..survivors {
            // ① 槽号 < 上一代长度 ⇒ 克隆；否则随机新生（`0x51b82c`）
            let (parent_tokens, parent_origin) = if slot < prev.len() {
                (prev[slot].tokens.clone(), Origin::Clone)
            } else {
                // `0x51b90f` → `0x51b923` → `0x51b95c`：随机函数 → 补齐 → 再变异一次
                let mut t = random_function(&mut next, mode);
                mutate(&mut t, &mut next);
                (t, Origin::RandomParent)
            };

            let score = evaluate(&parent_tokens, sit, COARSE_STEP);
            evaluations += 1;
            born.push(Candidate::new(parent_tokens.clone(), score, parent_origin, slot));

            // ② per_slot − 1 个工厂子代，**共用同一个父代**（`0x4ee3a0` 只有三个参数）
            for _ in 1..per_slot {
                let (tokens, origin) = if parent_tokens.len() < 20 {
                    // `0x4ee470`：父代太短 ⇒ 直接随机重生
                    (random_function(&mut next, mode), Origin::FactoryRebirth)
                } else {
                    let mut child = cross_from(&parent_tokens, mode, &mut next);
                    mutate(&mut child, &mut next);
                    (child, Origin::Factory)
                };
                let score = evaluate(&tokens, sit, COARSE_STEP);
                evaluations += 1;
                born.push(Candidate::new(tokens, score, origin, slot));
            }
        }
        let born_count = born.len();

        // ③ 代末降序排序 + 截断到 s（`0x51cb6f` / `0x51c208`）
        born.sort_by(|a, b| {
            b.score.score.partial_cmp(&a.score.score).unwrap_or(std::cmp::Ordering::Equal)
        });
        born.truncate(survivors);

        // ④⑤ 前 3 名精评；判据 = 净命中不等 ∨ |Δdist| > 0.5（`0x51c3b8`–`0x51c3e4`）
        let mut replaced = Vec::new();
        let mut fine_disagreements = Vec::new();
        for rank in 0..FINE_RANKS.min(born.len()) {
            let fine = evaluate(&born[rank].tokens, sit, FINE_STEP);
            evaluations += 1;
            let coarse = born[rank].score;
            let ok_hits = fine.net_hits() == coarse.net_hits();
            let ok_dist = !((fine.dist - coarse.dist).abs() > graphwar_function_evolutionator::CONSISTENCY_TOL);
            if ok_hits && ok_dist {
                continue;
            }
            fine_disagreements.push((
                rank,
                coarse.net_hits(),
                fine.net_hits(),
                coarse.dist,
                fine.dist,
            ));
            // 换新 = 随机 → 补齐 → 变异（`0x51c3f6`/`0x51c40a`/`0x51c44c`），分数留旧值
            let mut t = random_function(&mut next, mode);
            mutate(&mut t, &mut next);
            let slot = born[rank].slot;
            born[rank] = Candidate::new(t, coarse, Origin::Replaced, slot);
            replaced.push(rank);
        }

        history.push(Generation {
            index: g,
            born: born_count,
            survivors: born.len(),
            champion: born[0].clone(),
            ranking: born.clone(),
            replaced,
            fine_disagreements,
        });
        prev = born;
    }

    let winner = prev.first().cloned().unwrap_or_else(|| {
        Candidate::new(
            Vec::new(),
            Score { score: 0.0, dist: DIST_SENTINEL, kills: 0, friendly: 0 },
            Origin::RandomParent,
            0,
        )
    });
    Report {
        params: p,
        difficulty: settings.difficulty,
        mode,
        generations: history,
        winner,
        evaluations,
        truncated,
    }
}

// ---------------------------------------------------------------------------
// 文本渲染
// ---------------------------------------------------------------------------

/// 把一次"思考"渲染成给人看的文本。
pub fn render(sit: &Situation, report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    let p = report.params;
    out.push_str("=== 局面 ===\n");
    out.push_str(&format!(
        "射手 ({:.2}, {:.2}) flip={} 队伍={} · 障碍占用 {} 像素 · 场上实体 {} 个\n",
        sit.x0,
        sit.y0,
        sit.flip,
        sit.shooter_team,
        sit.occupied_pixels(),
        sit.entities.len()
    ));
    for e in &sit.entities {
        out.push_str(&format!(
            "  - id={} ({:.2}, {:.2}) r={:.2} 队伍={}{}\n",
            e.id,
            e.x,
            e.y,
            e.radius,
            e.team,
            if e.is_target { " [靶：只影响距离项]" } else { " [士兵：进命中数]" }
        ));
    }

    out.push_str("\n=== 参数 ===\n");
    match report.difficulty {
        Some(d) => out.push_str(&format!(
            "关卡难度值 {d}（`bot_added_event` 载荷 +0x10）\n"
        )),
        None => out.push_str("六档难度（`docs/AI.md` §1 的实测参数组合）\n"),
    }
    out.push_str(&format!(
        "代数 {} · 种群 {}（s = 种群/{} = {}，每代只留这 {} 个）· 复杂度上限 {}\n",
        p.generations,
        p.population,
        p.per_slot,
        p.population as usize / p.per_slot.max(1) as usize,
        p.population as usize / p.per_slot.max(1) as usize,
        p.limit
    ));
    out.push_str(&format!(
        "模式 {:?} · 粗评步长 {COARSE_STEP} · 精评步长 {FINE_STEP} · 前 {FINE_RANKS} 名精评\n",
        report.mode
    ));
    out.push_str(&format!(
        "评分次数 {}（原版每代 种群+3）· 撞墙钟={}\n",
        report.evaluations, report.truncated
    ));

    out.push_str("\n=== 逐代 ===\n");
    for g in &report.generations {
        let c = &g.champion;
        out.push_str(&format!(
            "第 {:4} 代：造 {} → 留 {} | 冠军 {:>12.1} 分 净命中 {:>3} 长 {:>2} | {}\n",
            g.index,
            g.born,
            g.survivors,
            c.score.score,
            c.score.net_hits(),
            c.tokens.len(),
            truncate(&c.text, 58)
        ));
        if !g.replaced.is_empty() {
            out.push_str(&format!(
                "            精评不一致 → 换掉名次 {:?}（名次,粗净,精净,粗dist,精dist = {:?}）\n",
                g.replaced, g.fine_disagreements
            ));
        }
        if verbose {
            for (rank, cand) in g.ranking.iter().enumerate().take(3) {
                out.push_str(&format!(
                    "            #{rank} {:>12.1} 净{:>3} 长{:>2} {:<10} {}\n",
                    cand.score.score,
                    cand.score.net_hits(),
                    cand.tokens.len(),
                    cand.origin.label(),
                    truncate(&cand.text, 50)
                ));
            }
        }
    }

    out.push_str("\n=== 最终选择 ===\n");
    let w = &report.winner;
    out.push_str(&format!("函数：{}\n", w.text));
    out.push_str(&format!(
        "分数 {:.1}（净命中 {} = 击杀 {} − 误伤 {}；距离项 {:.3}；长度 {}）\n",
        w.score.score,
        w.score.net_hits(),
        w.score.kills,
        w.score.friendly,
        w.score.dist,
        w.tokens.len()
    ));
    out.push_str(&format!("为什么是它：{}\n", report.why()));
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}

/// 把 token 序列变成函数文本（薄包装，方便 example 里少一个 import）。
pub fn text_of(tokens: &[Token]) -> String {
    to_text(tokens)
}

/// 该序列是否含某个 tag（粗筛用）。
pub fn contains_tag(tokens: &[Token], tag: Function) -> bool {
    tokens.iter().any(|t| t.tag == tag)
}

/// 一个最小的示范局面：射手在原点，两个敌人在斜前方，一个自己在近处。
pub fn demo_situation() -> Situation {
    let mut sit = Situation::empty(0.0, 0.0, false, 0, 15);
    sit.entities.push(Entity::soldier(101, -10.0, 2.0, 0.5, 1));
    sit.entities.push(Entity::soldier(102, 12.0, -3.0, 0.5, 1));
    sit.entities.push(Entity::soldier(103, 5.0, 1.5, 0.5, 0));
    sit
}
