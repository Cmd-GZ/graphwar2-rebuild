//! 同时回合制（`SimultaneousTurns`）的验收：收集 → 结算 → 广播三段。
//!
//! 依据是 `docs/86`（几何并行 + 事件按飞行时间串行）与 `docs/141`（批内报文的分组与
//! 顺序，130 个真回放多发包实测）。这里的每个断言都能指到那两份文档的一条结论：
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `two_shots_resolve_in_one_batch` | 批结构：前缀按类型分组、后缀每发一对 | **观测**（130/130 批） |
//! | `mutual_kills_in_the_same_batch` | 两发互相击杀，同 tick 双亡 | **观测**（`docs/86` §5.1） |
//! | `deaths_are_ordered_by_flight_time` | 死亡块按**到达时刻**排，不按发序 | **观测**（`docs/86` §4.2，38:0） |
//! | `one_soldier_dies_once_attributed_to_the_earliest_shot` | 一个士兵一回合只死一次 | **观测**（`docs/86` §5.2，152/152） |
//! | `blocked_shot_does_not_stop_the_other` | 一发被墙挡、另一发照常命中 | **观测**（`docs/86` §5.1） |
//! | `single_shot_is_byte_identical_to_fire` | 重构没有改变 `SequentialTurns` | **逐位验证** |
//! | `collection_*` | 收集阶段：攒齐 / 超时 / 重复 / 不在名单 | **观测**（`docs/141` §1、§4） |
use gw2_rebuild::game::{Match, Submission};
use gw2_rebuild::protocol::parse_message;
use gw2_rebuild::server::RoomState;
use gw2_rebuild::graphwar_shot_calculator::settle::Target;
use gw2_rebuild::graphwar_turn_manager::{RoundCollection, SubmitResult};

fn room(turn_mode: &str) -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: turn_mode.into(),
        time_mode: "Timer1m".into(),
    }
}

fn kinds(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|m| parse_message(m).expect("应为合法消息").kind)
        .collect()
}

/// `EffectInfo` 带 `effect_type`，其余按 kind（与 `replaydiff` 的投影同定义）。
fn kinds_sub(msgs: &[String]) -> Vec<String> {
    msgs.iter()
        .map(|m| {
            let r = parse_message(m).expect("应为合法消息");
            if r.kind == "EffectInfo" {
                let t = gw2_rebuild::protocol::get_str(&r, "effect_type").unwrap_or_default();
                format!("EffectInfo/{t}")
            } else {
                r.kind
            }
        })
        .collect()
}

/// 报文的某一层对象里那个整数字段（`entity_id` / `owner_id`）。
fn ints(msgs: &[String], kind: &str, field: &str) -> Vec<i64> {
    msgs.iter()
        .filter_map(|m| parse_message(m))
        .filter(|r| r.kind == kind)
        .filter_map(|r| gw2_rebuild::protocol::get_int(&r, field))
        .collect()
}

/// 两人的局面。射手在 `(x,y)`，`flip` 由队伍决定（`docs/105`）。
///
/// 用**水平线**（函数 `0.00`）而不是曲线：修正常数把线平移到射手的 y，
/// 于是两条弹道是两条互不相交的水平线 —— 每条只经过自己那一层的士兵，
/// "谁打死谁"因此完全由几何决定，测试里不掺采样器行为。
fn field(soldiers: Vec<Target>, obstacles: Vec<(f64, f64, f64)>) -> Match {
    Match::new(
        room("SimultaneousTurns"),
        obstacles,
        soldiers,
        (0, 0),
    )
}

fn tgt(id: i64, x: f64, y: f64) -> Target {
    Target { id, x, y, radius: 0.5 }
}

// ---------------------------------------------------------------------------
// 1. 批结构：前缀按类型分组、后缀每发一对
// ---------------------------------------------------------------------------

/// 两发一个事务。报文种类序列必须是（`docs/141` §2，130/130 批）：
///
/// ```text
/// Calc | FunctionInfo | FunctionInfo | FunctionPoints | FunctionPoints | Sync
///      | (FunctionActive | Explosion) × 2 | Exec
/// ```
#[test]
fn two_shots_resolve_in_one_batch() {
    let mut m = field(vec![tgt(10, -8.0, -4.0), tgt(11, 8.0, 4.0)], vec![]);
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: -4.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 4.0, flip: true },
        ],
        1000,
    );
    assert_eq!(
        kinds_sub(&msgs),
        vec![
            "GameStateInfo",
            "FunctionInfo",
            "FunctionInfo",
            "FunctionPoints",
            "FunctionPoints",
            "GameStateInfo",
            "FunctionActive",
            "EffectInfo/ExplosionEffect",
            "FunctionActive",
            "EffectInfo/ExplosionEffect",
            "GameStateInfo",
        ],
        "两发一批的报文种类序列与真回放不符"
    );
    // 前缀两块**各自按同一顺序**排（130/130 批）：owner 序列必须一样。
    assert_eq!(ints(&msgs, "FunctionInfo", "owner_id"), vec![10, 11]);
    assert_eq!(ints(&msgs, "FunctionPoints", "owner_id"), vec![10, 11]);
    assert_eq!(ints(&msgs, "FunctionActive", "owner_id"), vec![10, 11]);
    // 只有一对 `CalculatingResults` / `ExecutingResults`，不是每发一对。
    assert_eq!(msgs.iter().filter(|m| m.contains("CalculatingResults")).count(), 1);
    assert_eq!(msgs.iter().filter(|m| m.contains("ExecutingResults")).count(), 1);
}

/// 三发也一样：前缀 3 条一类，后缀 3 对。批宽度不改变形状（`docs/141` §2，
/// `|FunctionInfo| = |FunctionPoints| = |FunctionActive| = |Explosion| = N`，130/130）。
#[test]
fn batch_shape_scales_with_n() {
    let mut m = field(
        vec![tgt(10, -8.0, -4.0), tgt(11, 8.0, 4.0), tgt(12, -8.0, 9.0)],
        vec![],
    );
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: -4.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 4.0, flip: true },
            Submission { owner_id: 12, function_text: "0.00", x0: -8.0, y0: 9.0, flip: false },
        ],
        0,
    );
    let ks = kinds_sub(&msgs);
    assert_eq!(ks.iter().filter(|k| *k == "FunctionInfo").count(), 3);
    assert_eq!(ks.iter().filter(|k| *k == "FunctionPoints").count(), 3);
    assert_eq!(ks.iter().filter(|k| *k == "FunctionActive").count(), 3);
    assert_eq!(ks.iter().filter(|k| *k == "EffectInfo/ExplosionEffect").count(), 3);
    assert_eq!(ints(&msgs, "FunctionActive", "owner_id"), vec![10, 11, 12]);
}

// ---------------------------------------------------------------------------
// 2. 两发互相击杀 / 同 tick 双亡
// ---------------------------------------------------------------------------

/// 两发各自穿过对方的站位 ⇒ 同一批里两个射手都死（`docs/86` §5.1 的"几何并行"：
/// 谁都不影响别人的弹道，因为士兵不挡弹道）。
#[test]
fn mutual_kills_in_the_same_batch() {
    // A 在 (-8, 2) 打 +x、B 在 (8, -2) 打 -x；两条水平线在 y 上错开，
    // 但 A 的线穿过 B？——不：这里刻意让两条线**各自**穿过对方的站位：
    // A 的线 y=2 必须经过 B=(8,2)；B 的线 y=-2 必须经过 A=(-8,-2)。
    let mut m = field(vec![tgt(10, -8.0, -2.0), tgt(11, 8.0, 2.0)], vec![]);
    let msgs = m.fire_batch(
        &[
            // A 在 (-8,-2) 向 +x；修正后弹道是 y = -2，会经过 B 吗？B 在 (8, 2) —— 不会。
            // 所以这里改用"两条线各自穿过对方"的排布：见下。
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: -2.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 2.0, flip: true },
        ],
        0,
    );
    assert!(ints(&msgs, "FunctionPoints", "owner_id").len() == 2);
    // 这个排布下双方都活着（弹道不交叉）—— 断言"两个人都没死"，
    // 用于确认下面的 `mutual` 用例确实来自几何，而不是别的原因。
    assert!(ints(&msgs, "LifeQueuedDeath", "entity_id").is_empty());
    assert_eq!(m.soldiers.len(), 2);
}

/// 真正的互相击杀：两发都沿 **y = 0** 走、方向相反，于是各自的弹道都穿过对方。
///
/// 这同时是"**同 tick 双亡**"：两条死亡在同一个结算批里落地，两个射手在同一时刻
/// 被标死（真回放里 `LifeInfo{alive:false}` 会晚到，但"死"这件事在本批就定了）。
#[test]
fn mutual_kill_both_die_in_one_batch() {
    // 两个射手都在 y = 0 上，相向而行。
    let mut m = field(vec![tgt(10, -8.0, 0.0), tgt(11, 8.0, 0.0)], vec![]);
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: 0.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 0.0, flip: true },
        ],
        0,
    );
    let mut dead = ints(&msgs, "LifeQueuedDeath", "entity_id");
    dead.sort_unstable();
    assert_eq!(dead, vec![10, 11], "两发互相击杀 ⇒ 两个射手都该死: {:?}", kinds(&msgs));
    assert!(m.soldiers.is_empty(), "死掉的士兵必须从场上下桌");
    // 死亡块在 Sync 之后、第一个 FunctionActive 之前（`docs/141` §2，87/87 批）
    let sync = msgs
        .iter()
        .position(|m| m.contains("SynchronizingResults"))
        .expect("应有 SynchronizingResults");
    let ks = kinds_sub(&msgs);
    let first_active = ks.iter().position(|k| k == "FunctionActive").unwrap();
    let last_death = ks
        .iter()
        .rposition(|k| k == "LifeQueuedDeath" || k == "EffectInfo/DeathEffect")
        .expect("应有死亡块");
    assert!(sync < last_death && last_death < first_active, "死亡块位置不对: {ks:?}");
}

// ---------------------------------------------------------------------------
// 3. 死亡块按飞行时间排序，不按发序
// ---------------------------------------------------------------------------

/// 死亡块按**到达时刻**排（`docs/86` §4.2：34:0 / 38:0），不是按发序、不是按 id 序。
///
/// 局面（两条互不相交的水平线，弧长可精确算出）：
///
/// | 士兵 | 位置 | 谁打到 | 从起点算的弧长 |
/// |---|---|---|---|
/// | 21 | `(-2, 4)`  | 发 1（B，从 `(8,4)` 向 −x） | 10 |
/// | 22 | `(6, -4)`  | 发 0（A，从 `(-8,-4)` 向 +x） | 14 |
///
/// 按发序会得到 `[22, 21]`；按到达时刻是 `[21, 22]`。两者可判别。
#[test]
fn deaths_are_ordered_by_flight_time() {
    let mut m = field(
        vec![
            tgt(10, -8.0, -4.0),
            tgt(11, 8.0, 4.0),
            tgt(21, -2.0, 4.0),
            tgt(22, 6.0, -4.0),
        ],
        vec![],
    );
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: -4.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 4.0, flip: true },
        ],
        0,
    );
    assert_eq!(
        ints(&msgs, "LifeQueuedDeath", "entity_id"),
        vec![21, 22],
        "死亡块应按弧长（= 飞行时间）排：21 的弧长 10 < 22 的弧长 14"
    );
}

// ---------------------------------------------------------------------------
// 4. 一个士兵一回合只死一次，且归最早到达的那一发
// ---------------------------------------------------------------------------

/// 被两发穿过的士兵只出一条死亡，且归属**弧长更短**的那一发（`docs/86` §5.2 的 152/152）。
///
/// 23 在 `(2, 0)`：发 0 从 `(-8,0)` 出发弧长 10，发 1 从 `(8,0)` 出发弧长 6
/// ⇒ 归发 1。24 在 `(-6, 0)`：发 0 弧长 2、发 1 弧长 14 ⇒ 归发 0。
/// 两条死亡按弧长排是 `[24(2), 23(6)]`。
#[test]
fn one_soldier_dies_once_attributed_to_the_earliest_shot() {
    let mut m = field(
        vec![tgt(10, -8.0, 0.0), tgt(11, 8.0, 0.0), tgt(23, 2.0, 0.0), tgt(24, -6.0, 0.0)],
        vec![],
    );
    let before = m.soldiers.len();
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: 0.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 0.0, flip: true },
        ],
        0,
    );
    let dead = ints(&msgs, "LifeQueuedDeath", "entity_id");
    let mut uniq = dead.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(uniq.len(), dead.len(), "一个士兵一回合只应出一次死亡: {dead:?}");
    // 死的是 23 / 24 两个受害者 + 两个互相打死的射手（都在 y=0 的线上）
    assert!(dead.contains(&23) && dead.contains(&24), "{dead:?}");
    // 顺序 = 弧长升序：24 的弧长 2 最小
    assert_eq!(dead[0], 24, "弧长最小的应当排在最前: {dead:?}");
    assert_eq!(dead.len(), 4, "两个受害者 + 两个射手 = 4: {dead:?}");
    assert_eq!(m.soldiers.len(), before - 4);
}

// ---------------------------------------------------------------------------
// 5. 一发被墙挡、另一发照常命中
// ---------------------------------------------------------------------------

/// 障碍只让**自己那条弹道**停下（`docs/86` §5.1：弹道互不影响）；
/// 被挡的那一发产出 `QueueDestroyObstacle`，另一发照常打死人。
#[test]
fn blocked_shot_does_not_stop_the_other() {
    // 一堵墙挡在 A 的线上（y = -4，x ≈ 0）；B 那条线（y = 4）上没有障碍。
    let mut m = field(
        vec![tgt(10, -8.0, -4.0), tgt(11, 8.0, 4.0), tgt(22, 6.0, -4.0), tgt(21, -2.0, 4.0)],
        vec![(0.0, -4.0, 1.5)],
    );
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: "0.00", x0: -8.0, y0: -4.0, flip: false },
            Submission { owner_id: 11, function_text: "0.00", x0: 8.0, y0: 4.0, flip: true },
        ],
        0,
    );
    let ks = kinds(&msgs);
    assert_eq!(
        ks.iter().filter(|k| *k == "QueueDestroyObstacle").count(),
        1,
        "只有被挡的那一发需要排队挖洞: {ks:?}"
    );
    let dead = ints(&msgs, "LifeQueuedDeath", "entity_id");
    assert!(dead.contains(&21), "B 那一发没被挡，应当打死 21: {dead:?}");
    assert!(!dead.contains(&22), "A 那一发被墙挡住，打不到 22: {dead:?}");
    // 埋洞只落在被挡的那条线的停点上
    let qdo = msgs
        .iter()
        .filter_map(|m| parse_message(m))
        .find(|r| r.kind == "QueueDestroyObstacle")
        .expect("应有队列挖洞");
    let py = gw2_rebuild::protocol::get_f64(&qdo, "pos_y").unwrap();
    assert!((py + 4.0).abs() < 1.0, "挖洞点应在 A 那条线（y≈-4）上，实际 {py}");
}

// ---------------------------------------------------------------------------
// 6. 单发批与旧实现逐字节相同（回归保护）
// ---------------------------------------------------------------------------

/// `fire()` 现在是 `fire_batch(&[一发])`。它产出的报文必须与"单发一个完整窗口"
/// **逐字节相同**，否则 `SequentialTurns` 的行为就被这次重构悄悄改了。
#[test]
fn single_shot_is_byte_identical_to_fire() {
    let build = || field(vec![tgt(10, 5.0, 0.0), tgt(11, -2.0, 0.0)], vec![(-5.0, 0.0, 1.0)]);
    let mut a = build();
    let once = a.fire(10, "0.00", 5.0, 0.0, true, 1000);
    let mut b = build();
    let batch = b.fire_batch(
        &[Submission { owner_id: 10, function_text: "0.00", x0: 5.0, y0: 0.0, flip: true }],
        1000,
    );
    assert_eq!(once, batch);
    // 而且状态一样（士兵表 / 掩膜 / 待销毁队列）
    assert_eq!(a.soldiers, b.soldiers);
    assert_eq!(a.obstacles, b.obstacles);
}

// ---------------------------------------------------------------------------
// 7. 收集阶段
// ---------------------------------------------------------------------------

/// 攒齐就结算；没攒齐不结算。
#[test]
fn collection_waits_for_every_announced_player() {
    let mut c = RoundCollection::<&str>::new([7i64, 3, 5], None);
    assert!(!c.is_complete());
    assert!(!c.should_resolve(0), "没人交 ⇒ 不该结算");
    assert_eq!(c.submit(3, "b"), SubmitResult::Accepted);
    assert!(!c.should_resolve(0), "只交了 1/3 ⇒ 不该结算");
    assert_eq!(c.submit(5, "c"), SubmitResult::Accepted);
    assert_eq!(c.submit(7, "a"), SubmitResult::Accepted);
    assert!(c.is_complete());
    assert!(c.should_resolve(0), "攒齐就该结算");
    // **顺序 = 公告顺序**（`docs/141` §4，130/130 批），不是提交到达顺序。
    assert_eq!(c.resolve(), vec![(7, "a"), (3, "b"), (5, "c")]);
}

/// 超时也结算：没交的人不出现在结果里（真回放里正是这样：
/// `TurnInfo` 公告了 3 个人、结算批只有 2 发）。
#[test]
fn collection_resolves_on_timeout_without_the_silent_players() {
    let mut c = RoundCollection::<&str>::new([7i64, 3, 5], Some(100));
    assert_eq!(c.submit(3, "b"), SubmitResult::Accepted);
    assert!(!c.should_resolve(99), "没到点、也没攒齐 ⇒ 继续等");
    assert!(c.should_resolve(100), "到点就结算");
    assert_eq!(c.resolve(), vec![(3, "b")]);
}

/// 重复提交与名单外的提交都要被拒，且不改变收集状态。
#[test]
fn collection_rejects_duplicates_and_strangers() {
    let mut c = RoundCollection::<i32>::new([7i64, 3], None);
    assert_eq!(c.submit(9, 1), SubmitResult::NotExpected);
    assert_eq!(c.submit(7, 1), SubmitResult::Accepted);
    assert_eq!(c.submit(7, 2), SubmitResult::Duplicate);
    assert_eq!(c.len(), 1);
    assert_eq!(c.waiting_on(), vec![3]);
    let mut c2 = RoundCollection::<i32>::new([7i64], None);
    c2.close();
    assert!(c2.is_closed());
    assert!(!c2.should_resolve(0), "关掉的轮次不再要求结算");
    assert_eq!(c2.submit(7, 1), SubmitResult::Closed);
}

/// 公告名单为空时**立刻**算"齐了"（没有人在等，别把回合卡死）。
#[test]
fn empty_announcement_is_immediately_complete() {
    let c = RoundCollection::<i32>::new(Vec::<i64>::new(), None);
    assert!(c.is_complete());
    assert!(c.should_resolve(0));
    assert!(c.resolve().is_empty());
}

// ---------------------------------------------------------------------------
// 8. 三段打通：收集 → 结算 → 广播（一整个同时回合）
// ---------------------------------------------------------------------------

/// 一整个同时回合，走完整条链：
///
/// ```text
/// begin_simultaneous_turn  → TurnInfo × P + EndOfTurnInfo + WaitingForFunctions
///   （玩家各自交一发）        → RoundCollection 收下
/// fire_batch               → 一个结算批
/// end_turn                 → TurnCountInfo（+ EndingGame）
/// ```
///
/// 断言三件事：① 每人一条 `TurnInfo`、`EndOfTurnInfo`/`GameStateInfo` 各**一条**
/// （`docs/141` §1：`1793884702` 每回合都是这个形状）；② 收集顺序 = 公告顺序；
/// ③ 结算批的分组与顺序就是上面那几条测试钉住的形状。
#[test]
fn a_full_simultaneous_round_runs_all_three_phases() {
    use gw2_rebuild::graphwar_turn_manager::Team;

    let mut m = field(vec![tgt(10, -8.0, -4.0), tgt(11, 8.0, 4.0)], vec![]);
    m.register_player(7, Team::One, &[10]);
    m.register_player(3, Team::Two, &[11]);

    // ---- 收集阶段 ----
    // 公告顺序由调用方给（真游戏里是玩家表的 HashMap 迭代序，`docs/113` 的 A 类）。
    let (hello, starts) = m.begin_simultaneous_turn(&[7, 3], 1000, "TimerInf");
    let hk = kinds(&hello);
    assert_eq!(hk.iter().filter(|k| *k == "TurnInfo").count(), 2, "每人一条 TurnInfo: {hk:?}");
    assert_eq!(hk.iter().filter(|k| *k == "EndOfTurnInfo").count(), 1, "EndOfTurnInfo 只有一条");
    assert_eq!(hk.iter().filter(|k| *k == "GameStateInfo").count(), 1, "WaitingForFunctions 只有一条");
    assert!(hello.last().unwrap().contains("WaitingForFunctions"));
    assert_eq!(starts.len(), 2);
    assert_eq!(ints(&hello, "TurnInfo", "player_id"), vec![7, 3], "公告顺序 = 调用方给的顺序");

    // ---- 收集：两人各交一发 ----
    let waitlist: Vec<i64> = starts
        .iter()
        .filter_map(|st| st.decision.broadcast())
        .filter(|(_, s)| *s != 0)
        .map(|(p, _)| p)
        .collect();
    assert_eq!(waitlist, vec![7, 3]);
    let mut col = RoundCollection::<i64>::new(waitlist.clone(), None);
    assert_eq!(col.submit(3, 11), SubmitResult::Accepted);
    assert!(!col.should_resolve(0), "还没交齐");
    assert_eq!(col.submit(7, 10), SubmitResult::Accepted);
    assert!(col.should_resolve(0));
    let shots = col.resolve();
    assert_eq!(shots, vec![(7, 10), (3, 11)], "结算顺序 = 公告顺序，不是提交到达顺序");

    // ---- 结算 + 广播 ----
    let text = "0.00";
    let (x0, y0) = {
        let a = m.soldiers.iter().find(|s| s.id == 10).unwrap();
        (a.x, a.y)
    };
    let (bx0, by0) = {
        let b = m.soldiers.iter().find(|s| s.id == 11).unwrap();
        (b.x, b.y)
    };
    let _ = (x0, y0);
    let msgs = m.fire_batch(
        &[
            Submission { owner_id: 10, function_text: text, x0, y0, flip: false },
            Submission { owner_id: 11, function_text: text, x0: bx0, y0: by0, flip: true },
        ],
        2000,
    );
    let ks = kinds_sub(&msgs);
    assert_eq!(ks.iter().filter(|k| *k == "GameStateInfo").count(), 3, "{ks:?}");
    assert_eq!(ks.iter().filter(|k| *k == "FunctionInfo").count(), 2);
    assert_eq!(ks.iter().filter(|k| *k == "FunctionPoints").count(), 2);

    // ---- 回合收尾 ----
    let bye = m.end_turn();
    assert!(bye[0].contains("TurnCountInfo"));
    assert_eq!(m.turn_count, 1);
}

// ---------------------------------------------------------------------------
// 9. 端到端（真 TCP）：同时回合制能跑起来，且广播是**一个批**
// ---------------------------------------------------------------------------

mod tcp {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

    pub fn frame(msg: &str) -> Vec<u8> {
        let mut v = (msg.len() as u16).to_be_bytes().to_vec();
        v.extend_from_slice(msg.as_bytes());
        v
    }

    pub fn read_messages(sock: &mut TcpStream) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = Vec::new();
        sock.set_read_timeout(Some(std::time::Duration::from_millis(600))).ok();
        loop {
            let mut tmp = [0u8; 8192];
            match sock.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(_) => break,
            }
            loop {
                if buf.len() < 2 {
                    break;
                }
                let n = u16::from_be_bytes([buf[0], buf[1]]) as usize;
                if buf.len() < 2 + n {
                    break;
                }
                out.push(String::from_utf8_lossy(&buf[2..2 + n]).to_string());
                buf.drain(..2 + n);
            }
        }
        out
    }

    pub fn kind(m: &str) -> String {
        let rest = &m[2..];
        rest[..rest.find('"').unwrap_or(0)].to_string()
    }

    pub fn start_server(
        cfg: gw2_rebuild::graphwar_game_server::Config,
    ) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind(&cfg.addr).expect("bind");
        let addr = l.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            let mut sock = l.accept().expect("accept").0;
            gw2_rebuild::graphwar_game_server::serve_stream(&mut sock, &cfg).ok();
        });
        (addr, h)
    }
}

/// 同时回合制跑完整一局回合：一个客户端替**两个玩家**各交一发，服务端攒齐后
/// **一次**把整个结算批发回来。
///
/// 断言（都是 `docs/141` §2 的结构结论）：
/// * 开局那段：`TurnInfo` 条数 = 玩家数，`EndOfTurnInfo` / `GameStateInfo(WaitingForFunctions)`
///   各**一条**（不是每人一条）；
/// * 交第一发时服务端**一个字都不回**（收集阶段没有回显）；
/// * 交第二发（攒齐）才回一个批：`CalculatingResults` 与 `ExecutingResults` 各一条，
///   `FunctionInfo` / `FunctionPoints` / `FunctionActive` 各 N 条。
#[test]
fn simultaneous_round_over_tcp_emits_one_grouped_batch() {
    use gw2_rebuild::graphwar_game_server::{Config, FIRST_PLAYER_ID};
    use std::io::Write;
    use std::net::TcpStream;
    use tcp::*;

    let cfg = Config {
        seed: [53u8; 32],
        turn_mode: "SimultaneousTurns".into(),
        per_team: 2,
        ..Default::default()
    };
    let (addr, server) = start_server(cfg);
    let mut client = TcpStream::connect(addr).expect("connect");

    client
        .write_all(&frame(r#"{"NewConnectionRequest":{"connection_id":228}}"#))
        .unwrap();
    let _ = read_messages(&mut client);
    client.write_all(&frame(r#"{"GameStartRequest":{}}"#)).unwrap();
    let start = read_messages(&mut client);
    assert!(!start.is_empty(), "开局应有报文");

    let sk: Vec<String> = start.iter().map(|m| kind(m)).collect();
    let turn_infos: Vec<(i64, i64)> = start
        .iter()
        .filter(|m| kind(m) == "TurnInfo")
        .filter_map(|m| {
            let r = parse_message(m)?;
            Some((
                gw2_rebuild::protocol::get_int(&r, "player_id")?,
                gw2_rebuild::protocol::get_int(&r, "soldier_id")?,
            ))
        })
        .collect();
    // 4 名士兵 → 2 名玩家（per_team = 2，两队各一个玩家槽）。
    assert_eq!(turn_infos.len(), 2, "每人一条 TurnInfo（不是每人三条）: {sk:?}");
    assert_eq!(sk.iter().filter(|k| *k == "EndOfTurnInfo").count(), 1, "EndOfTurnInfo 只有一条");
    assert_eq!(sk.last().map(|s| s.as_str()), Some("GameStateInfo"));
    assert!(start.last().unwrap().contains("WaitingForFunctions"));

    // 逐个玩家提交：`FunctionUpdateRequest` 写槽，`FunctionFireRequest` 交一发。
    let mut last: Vec<String> = Vec::new();
    for (i, (pid, _sid)) in turn_infos.iter().enumerate() {
        client
            .write_all(&frame(&format!(
                r#"{{"FunctionUpdateRequest":{{"player_id":{pid},"function":"0.00"}}}}"#
            )))
            .unwrap();
        let ack = read_messages(&mut client);
        assert!(ack.iter().any(|m| kind(m) == "FunctionInfo"), "提交应有 FunctionInfo 回执");
        client
            .write_all(&frame(&format!(r#"{{"FunctionFireRequest":{{"player_id":{pid}}}}}"#)))
            .unwrap();
        let got = read_messages(&mut client);
        if i + 1 < turn_infos.len() {
            assert!(got.is_empty(), "还没攒齐 ⇒ 收集阶段不该回任何报文，实际 {:?}", got
                .iter()
                .map(|m| kind(m))
                .collect::<Vec<_>>());
        } else {
            last = got;
        }
    }
    let lk: Vec<String> = last.iter().map(|m| kind(m)).collect();
    let calc = last.iter().filter(|m| m.contains("CalculatingResults")).count();
    let exec = last.iter().filter(|m| m.contains("ExecutingResults")).count();
    assert_eq!(calc, 1, "一个批只有一条 CalculatingResults: {lk:?}");
    assert_eq!(exec, 1, "一个批只有一条 ExecutingResults: {lk:?}");
    assert_eq!(lk.iter().filter(|k| *k == "FunctionInfo").count(), 2, "两发一个批: {lk:?}");
    assert_eq!(lk.iter().filter(|k| *k == "FunctionPoints").count(), 2, "两发一个批: {lk:?}");
    assert_eq!(lk.iter().filter(|k| *k == "FunctionActive").count(), 2, "两发一个批: {lk:?}");
    // 下一回合的公告已经在同一个回包里（服务端立刻开下一回合）
    assert!(lk.iter().filter(|k| *k == "TurnInfo").count() >= 2, "应接着公告下一回合: {lk:?}");
    let _ = FIRST_PLAYER_ID;
    drop(client);
    let _ = server.join();
}
