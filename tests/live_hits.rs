//! 用**真实对局**的数据验证"碰撞判定"整条链路。
//!
//! 数据来源：`tools/capture_net_raw.js` 抓的真实报文（240 秒对局）：
//!
//! * 关卡障碍来自 `AddObstacle{pos_x,pos_y,radius}`；
//! * 士兵位置来自 `PosInfo{entity_id,pos_x,pos_y,radius=0.5}`；
//! * 命中点来自 `QueueDestroyObstacle{pos_x,pos_y}`，与 `EffectInfo` 的
//!   `ExplosionEffect.start_tick` 是同一次命中。
//!
//! 服务器**不广播轨迹**，只广播起点与终点，所以这里做的事正是客户端要做的：
//! 用同一套采样器重算轨迹，再沿轨迹查掩膜，看第一个被占据的点是不是抓到的命中点。

use gw2_rebuild::graphwar_path_calculator::Mode;
use gw2_rebuild::plane::{empty_mask, is_occupied, rasterize};
use gw2_rebuild::graphwar_shot_calculator::shot_points;

/// 真实对局里的 4 名士兵（`PosInfo`）。
const SOLDIERS: [(f64, f64); 4] = [
    (-8.549630150684965, -11.540337805946324),
    (-19.512760846799154, 10.99018276610812),
    (18.36314344687766, 1.2251556390381353),
    (1.651220991201045, 4.638020825690482),
];

/// 真实关卡的前 17 个障碍（`AddObstacle`）。
const OBSTACLES: [(f64, f64, f64); 17] = [
    (25.071378402490392, 9.716751913103266, 3.2852074351444402),
    (5.99118158962943, 3.6562415629381655, 2.192838347386053),
    (-17.629657259944548, 6.321131621950006, 2.095820466550916),
    (28.382113938215582, -16.780836940074124, 2.588626080740486),
    (28.28536839382893, 13.248063685913687, 3.7251675416225107),
    (7.4740586531371465, -3.8088431175675415, 2.625875145419125),
    (-11.204252855909106, 14.916695643016213, 1.7215512249365994),
    (-25.000046177746338, 2.461641646847163, 3.3466873599459124),
    (-9.005369433938938, 10.482090211951576, 3.0277415889568022),
    (7.270955936353686, 4.096054443978932, 2.920397146394815),
    (1.5722759494530327, -7.4916048913195805, 4.176219520137395),
    (17.206140520606073, -4.112930411385913, 0.8),
    (13.558249333233528, -6.202216381136294, 3.7605101561610867),
    (-19.10459179787006, -8.720569834623932, 4.790334090199217),
    (7.593392076442233, 4.885484185657761, 2.8546461919075408),
    (4.018092880202865, 17.90703412455993, 2.0843021894056526),
    (0.0, 0.0, 0.0),
];

fn build_mask() -> Vec<u8> {
    let mut m = empty_mask();
    for (x, y, r) in OBSTACLES {
        if r > 0.0 {
            rasterize(&mut m, x, y, r, 0x00);
        }
    }
    for (x, y) in SOLDIERS {
        rasterize(&mut m, x, y, 0.5, 0x00);
    }
    m
}

/// 沿轨迹查掩膜，跳过开火者自身身体范围。
/// 跳过射手自己身体时必须留的余量。
///
/// 士兵标称半径 0.5（= 10 px），但掩膜是**像素量化**的圆盘，边界会抖动约 1 px
/// = 0.05 世界单位。因此判据必须略大于 0.5；否则采样点变密时
/// （实况 `step = 0.01`），会有点落在 0.507 处——既没被跳过、又落在被量化成
/// 占用的像素上，误判为撞到射手自己。0.56 留足了这一余量。
const SHOOTER_SKIP_R: f64 = 0.56;

fn first_hit(mask: &[u8], pts: &[(f64, f64)], x0: f64, y0: f64) -> Option<(f64, f64)> {
    for &(px, py) in pts {
        if ((px - x0).powi(2) + (py - y0).powi(2)).sqrt() <= SHOOTER_SKIP_R {
            continue;
        }
        if is_occupied(mask, px, py) {
            return Some((px, py));
        }
    }
    None
}

#[test]
fn captured_hits_are_reproduced_by_our_collision_chain() {
    let mask = build_mask();
    // (起点, flip, 抓到的命中点)
    let cases = [
        ((-19.512760846799154, 10.99018276610812), false, (-12.040885846800286, 10.99)),
        ((1.651220991201045, 4.638020825690482), true, (-16.405029008797804, 4.638)),
    ];
    for ((x0, y0), flip, hit) in cases {
        let pts = shot_points("0.00", x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal, true);
        let got = first_hit(&mask, &pts, x0, y0).unwrap_or_else(|| panic!("轨迹没有撞到任何实体: {hit:?}"));
        let d = ((got.0 - hit.0).powi(2) + (got.1 - hit.1).powi(2)).sqrt();
        // 采样步长是 0.5，命中发生在两次采样之间，所以误差必然小于一个步长
        assert!(d < 0.5, "首次碰撞点 {got:?} 与抓到的命中点 {hit:?} 相距 {d}");
        // 抓到的命中点必须正好落在某个障碍圆的边缘（栅格化误差 ≤ 1 像素 = 0.05 单位）
        let mut best = f64::INFINITY;
        for (ox, oy, r) in OBSTACLES {
            if r <= 0.0 {
                continue;
            }
            let dist = ((hit.0 - ox).powi(2) + (hit.1 - oy).powi(2)).sqrt();
            best = best.min((dist - r).abs());
        }
        assert!(best < 0.06, "命中点不在障碍边缘上（最小偏差 {best}）");
    }
}

#[test]
fn mask_from_real_level_blocks_the_documented_points() {
    let mask = build_mask();
    // 抓到的两个命中点本身必须被判为"被占据"
    assert!(is_occupied(&mask, -12.040885846800286, 10.99));
    assert!(is_occupied(&mask, -16.405029008797804, 4.638));
    // 而两名士兵的初始位置当然也是占据
    for (x, y) in SOLDIERS {
        assert!(is_occupied(&mask, x, y), "士兵位置应被占据: ({x},{y})");
    }
}

// ---------------------------------------------------------------------------
// 第二组：高难度 Bot 对局（曲线弹道）
// ---------------------------------------------------------------------------
//
// 这一关抓到 20 个障碍、4 名士兵、4 次射击。其中两发的函数是演化出来的曲线：
//   `234`: `(-46.06/(7.94+x)) + 15.508`
//   `233`: `cos((0.73*x)) + 1.643`
// 命中点与终点 x 都来自 `FunctionPoints` / `QueueDestroyObstacle`。

const LEVEL2_OBSTACLES: [(f64, f64, f64); 20] = [
    (19.511625079799717, -14.015085500163506, 4.975198844808007),
    (14.172086498289255, -4.301470400387449, 0.8),
    (12.202420249301309, 2.8639969474772085, 3.5924178561047007),
    (16.04299851707694, 7.779079076197709, 5.289563828805489),
    (-10.352897229374758, 21.355890865678457, 3.2497236211918676),
    (-6.1594294417603574, 21.076294483448645, 5.707207519421768),
    (-28.931365368945162, 6.9911030730749255, 3.3381734343192235),
    (14.148440051052289, -0.6687197184906855, 4.959365730233035),
    (4.728689356147907, 12.824719329935842, 4.838617514148936),
    (-6.535729912888458, 8.808675504149576, 0.8),
    (12.643089107419478, -11.011232702347025, 2.363779912570086),
    (-16.260879015666447, 3.2791900456290657, 1.428155893736604),
    (-13.62783428839345, -10.774161312484654, 0.8),
    (-21.858799221570962, -19.955382399502682, 2.9876904901921653),
    (19.078736440195044, 13.444136938111521, 0.8016219870793275),
    (-20.415603453145458, 14.046939272852828, 1.2449638043029094),
    (-3.6457614609797515, 0.47628033188496643, 5.5328318331859165),
    (-21.95566195304651, 16.573509494097767, 0.8),
    (-5.775576073208802, -11.266191210703688, 5.562594589934369),
    (-0.1792709317152293, 3.5724514393987437, 3.9422467232454226),
];

const LEVEL2_SOLDIERS: [(f64, f64); 4] = [
    (22.093339229702053, 0.7301541058520371),
    (-7.12554866468165, 6.212956280205123),
    (-11.346008187591371, 12.212311109990708),
    (6.01835415786761, -8.461015900864648),
];

fn build_mask2() -> Vec<u8> {
    let mut m = empty_mask();
    for (x, y, r) in LEVEL2_OBSTACLES {
        rasterize(&mut m, x, y, r, 0x00);
    }
    for (x, y) in LEVEL2_SOLDIERS {
        rasterize(&mut m, x, y, 0.5, 0x00);
    }
    m
}

#[test]
fn captured_curved_shots_are_reproduced() {
    let mask = build_mask2();
    // (函数原文, 起点 x, 起点 y, 抓到的终点 x, 抓到的命中 y, flip)
    let cases = [
        ("(-46.06/(7.94+x))", 6.018_f64, -8.461_f64, -19.202_f64, 13.811_f64, true),
        ("cos((0.73*x))", 22.093_f64, 0.730_f64, 17.995_f64, 2.485_f64, true),
    ];
    for (text, x0, y0, end_x, hit_y, flip) in cases {
        let pts = shot_points(text, x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal, true);
        let got = first_hit(&mask, &pts, x0, y0)
            .unwrap_or_else(|| panic!("轨迹没有撞到任何实体: {text}"));
        let d = ((got.0 - end_x).powi(2) + (got.1 - hit_y).powi(2)).sqrt();
        assert!(d < 0.5, "{text} 首个碰撞 {got:?} 与抓到的 ({end_x}, {hit_y}) 相距 {d}");
    }
}

#[test]
fn captured_correction_constants_match() {
    // 抓到的 corrected_function 分别是 "(-46.06/(7.94+x)) + 15.508" 与 "cos((0.73*x)) + 1.643"。
    // 必须用**全精度**起点，否则常数差千分之几（用三位小数会得到 15.504）。
    assert_eq!(
        gw2_rebuild::graphwar_shot_calculator::corrected_text("(-46.06/(7.94+x))", 6.01835415786761, -8.461015900864648, true),
        "(-46.06/(7.94+x)) + 15.508"
    );
    assert_eq!(
        gw2_rebuild::graphwar_shot_calculator::corrected_text("cos((0.73*x))", 22.093339229702053, 0.7301541058520371, true),
        "cos((0.73*x)) + 1.643"
    );
}

#[test]
fn full_level_replay_of_all_four_captured_shots() {
    // 同一关的 4 发全部复算：函数取自各实体抓到的 corrected_function
    // （230/231 未提交函数，服务端按 `0.00` 处理）。
    let mask = build_mask2();
    let cases: [(&str, f64, f64, f64, f64, bool); 4] = [
        ("0.00", -11.346008187591371, 12.212311109990708, -0.09600818759175, 12.212, false),
        ("(-46.06/(7.94+x))", 6.01835415786761, -8.461015900864648, -19.202270842130353, 13.811015975785434, true),
        ("0.00", -7.12554866468165, 6.212956280205123, -3.0942986646815407, 6.213, false),
        ("cos((0.73*x))", 22.093339229702053, 0.7301541058520371, 17.994589229702342, 2.4850738714109544, true),
    ];
    let mut worst = 0.0_f64;
    for (text, x0, y0, hit_x, hit_y, flip) in cases {
        let pts = shot_points(text, x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal, true);
        let got = first_hit(&mask, &pts, x0, y0)
            .unwrap_or_else(|| panic!("{text}: 轨迹没有撞到任何实体"));
        let d = ((got.0 - hit_x).powi(2) + (got.1 - hit_y).powi(2)).sqrt();
        assert!(d < 0.5, "{text}: 首个碰撞 {got:?} 与抓到的 ({hit_x}, {hit_y}) 相距 {d}");
        worst = worst.max(d);
    }
    assert!(worst > 0.0, "至少应有一发有偏差（采样步长导致）");
}

// ---------------------------------------------------------------------------
// 致死判定：弹道"穿过"士兵即杀死，但**不会停下**
// ---------------------------------------------------------------------------
//
// 数据来自一局有死亡的对局（14 个障碍、6 次射击、死亡 230 与 231）。
// 关键观察：
//   * `LifeQueuedDeath` 的 `EffectInfo` 位置 = 死者的**站立位置**，
//     而不是碰撞点；
//   * 该发最终爆炸点在 `x=-24.99`，远在死者（x≈-10.45）之后
//     => 弹道穿过士兵后继续飞。
// 所以：掩膜里**只放障碍**，士兵单独做"经过即死"判定。

const KILL_OBSTACLES: [(f64, f64, f64); 14] = [
    (-21.5884800661654, -7.3800965520851, 1.1902918644742717),
    (-0.0043405309383072535, 8.031053141932013, 4.03260643486631),
    (-27.485404156862483, -2.607348073571232, 3.0563754620559624),
    (19.549097387425583, 12.244367460469363, 0.8),
    (5.664280020587462, -1.4520102469532084, 5.530115811710694),
    (-3.0987736677523, -0.41426820089414934, 2.7894043084418967),
    (-7.271546855545921, 7.407667665483476, 2.1546288278638563),
    (2.6337852096200587, -16.364006888151255, 4.150737650686631),
    (-33.029423617493215, 15.326196716300089, 4.120993686035668),
    (-3.5817439411020002, 15.61255105748268, 2.5176156833383114),
    (31.181023534936582, -16.99000672897561, 3.187013128939303),
    (19.295935788332006, 10.214361424896895, 2.6420553382885146),
    (28.267826384274272, -9.996620908589026, 3.6649732987672836),
    (-8.732270571729165, 12.312386915630467, 3.662220619519833),
];

/// (实体 id, x, y)
const KILL_SOLDIERS: [(i64, f64, f64); 4] = [
    (233, 18.434132014691514, 0.8654732803815559),
    (231, -11.75347210076399, -5.862016522961236),
    (234, 17.627629369923454, -12.391816789113197),
    (230, -10.452779035793856, -2.651563472451839),
];

fn build_mask_no_soldiers() -> Vec<u8> {
    let mut m = empty_mask();
    for (x, y, r) in KILL_OBSTACLES {
        rasterize(&mut m, x, y, r, 0x00);
    }
    m
}

#[test]
fn captured_kills_happen_when_trajectory_passes_through_soldier() {
    // (函数, 起点 x, 起点 y, 死者 id, 抓到的修正常数)
    let cases = [
        ("(x/3.02)", 17.627629369923454_f64, -12.391816789113197_f64, 230_i64, -6.555_f64),
        ("(-1.65/(-8.04/x))", 17.627629369923454, -12.391816789113197, 231, -8.774),
    ];
    for (text, x0, y0, victim, captured_c) in cases {
        let toks = gw2_rebuild::math_function::parse(text);
        let c = gw2_rebuild::graphwar_shot_calculator::correction(&toks, x0, y0, true);
        assert!((c - captured_c).abs() < 5e-4, "{text}: 修正 {c} 与抓到的 {captured_c} 不符");

        let pts = shot_points(text, x0, y0, gw2_rebuild::SERVER_STEP, true, Mode::Normal, true);
        let (_, vx, vy) = KILL_SOLDIERS.iter().find(|(id, _, _)| *id == victim).unwrap();
        let d = pts
            .iter()
            .map(|(px, py)| ((px - vx).powi(2) + (py - vy).powi(2)).sqrt())
            .fold(f64::INFINITY, f64::min);
        assert!(d <= 0.5, "{text}: 轨迹到死者距离 {d} 大于士兵半径 0.5，不该致死");
    }
}

#[test]
fn soldiers_do_not_block_the_trajectory() {
    // 掩膜里只有障碍：致死那发不会停在士兵处，而是继续飞到更远的地方
    let mask = build_mask_no_soldiers();
    let (x0, y0) = (17.627629369923454_f64, -12.391816789113197_f64);
    let pts = shot_points("(x/3.02)", x0, y0, gw2_rebuild::SERVER_STEP, true, Mode::Normal, true);
    // 这一发没有撞到任何障碍：它一路飞到棋盘边界（|x|<=25）
    let max_left = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    assert!(max_left < -10.45, "轨迹没有越过死者 x≈-10.45（最左只到 {max_left}）");
    assert!((max_left - (-24.99362)).abs() < 0.6,
            "轨迹最左端 {max_left} 与抓到的终点 x=-24.9936 不符");
    // 障碍掩膜在这条路径上确实没有阻挡
    assert!(first_hit(&mask, &pts, x0, y0).is_none(), "不该撞到障碍");
}

#[test]
fn resolve_reproduces_captured_kills_and_endpoints() {
    use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};

    let mask = build_mask_no_soldiers();
    // 两发都是 Bot 的 234 打出的；射手本人不参与致死判定
    let targets: Vec<Target> = KILL_SOLDIERS
        .iter()
        .filter(|(id, _, _)| *id != 234)
        .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();

    // (函数, 抓到的终点 x, 抓到的死者)
    let cases = [
        ("(x/3.02)", -24.99362063007402_f64, 230_i64),
        ("(-1.65/(-8.04/x))", -24.55049563007283, 231),
    ];
    for (text, end_x, victim) in cases {
        let r = resolve(
            text,
            17.627629369923454,
            -12.391816789113197,
            0.5,
            true,
            Mode::Normal,
            &mask,
            &targets,
        );
        assert_eq!(r.killed, vec![victim], "{text}: 致死对象不符");
        assert!(!r.blocked, "{text}: 这一发没有撞到障碍，不该被阻挡");
        assert!(
            (r.stop.0 - end_x).abs() < 1.0,
            "{text}: 终点 x={} 与抓到的 {end_x} 相差过大",
            r.stop.0
        );
    }
}

#[test]
fn generated_events_follow_captured_protocol_sequence() {
    use gw2_rebuild::graphwar_shot_calculator::settle::{events, resolve, ShotEvent, Target};

    let mask = build_mask_no_soldiers();
    let targets: Vec<Target> = KILL_SOLDIERS
        .iter()
        .filter(|(id, _, _)| *id != 234)
        .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();

    let (x0, y0) = (17.627629369923454_f64, -12.391816789113197_f64);
    let r = resolve("(x/3.02)", x0, y0, 0.5, true, Mode::Normal, &mask, &targets);
    let ev = events(234, x0, y0, &r, &targets);

    // 抓包里的顺序（Tick / FunctionActive / GameStateInfo 属于状态机，不在本测试范围）：
    //   FunctionPoints -> LifeQueuedDeath -> EffectInfo(DeathEffect) -> EffectInfo(ExplosionEffect)
    assert!(matches!(ev[0], ShotEvent::FunctionPoints { owner_id: 234, .. }), "首条应是 FunctionPoints");
    assert!(matches!(ev[1], ShotEvent::LifeQueuedDeath { entity_id: 230 }), "其次应是 230 的死亡");
    match &ev[2] {
        ShotEvent::DeathEffect { entity_id, x, y } => {
            assert_eq!(*entity_id, 230);
            // 死亡效果点在士兵站立处（抓包：-10.452779035793856, -2.651563472451839）
            assert!((x - (-10.452779035793856)).abs() < 1e-9, "死亡效果 x 不符: {x}");
            assert!((y - (-2.651563472451839)).abs() < 1e-9, "死亡效果 y 不符: {y}");
        }
        other => panic!("第三条应是 DeathEffect，实际 {other:?}"),
    }
    // 这一发没有撞到障碍，所以不产生 QueueDestroyObstacle
    assert!(!ev.iter().any(|e| matches!(e, ShotEvent::QueueDestroyObstacle { .. })),
            "未被阻挡时不应产生 QueueDestroyObstacle");
    match ev.last().unwrap() {
        ShotEvent::ExplosionEffect { x, .. } => {
            // 抓到的爆炸点 x = -24.99362063007402
            assert!((x - (-24.99362063007402)).abs() < 1.0, "爆炸点 x 不符: {x}");
        }
        other => panic!("最后应是 ExplosionEffect，实际 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 整关 6 发回放（死亡关卡）
// ---------------------------------------------------------------------------
//
// 同一关的**全部** 6 次射击，函数取自各实体抓到的 corrected_function 的原文
// （230/231 未提交函数，按 `0.00` 处理），终点 x 取自 `FunctionPoints`。

const REPLAY_OBSTACLES: [(f64, f64, f64); 14] = [
    (-21.5884800661654, -7.3800965520851, 1.1902918644742717),
    (-0.0043405309383072535, 8.031053141932013, 4.03260643486631),
    (-27.485404156862483, -2.607348073571232, 3.0563754620559624),
    (19.549097387425583, 12.244367460469363, 0.8),
    (5.664280020587462, -1.4520102469532084, 5.530115811710694),
    (-3.0987736677523, -0.41426820089414934, 2.7894043084418967),
    (-7.271546855545921, 7.407667665483476, 2.1546288278638563),
    (2.6337852096200587, -16.364006888151255, 4.150737650686631),
    (-33.029423617493215, 15.326196716300089, 4.120993686035668),
    (-3.5817439411020002, 15.61255105748268, 2.5176156833383114),
    (31.181023534936582, -16.99000672897561, 3.187013128939303),
    (19.295935788332006, 10.214361424896895, 2.6420553382885146),
    (28.267826384274272, -9.996620908589026, 3.6649732987672836),
    (-8.732270571729165, 12.312386915630467, 3.662220619519833),
];

const REPLAY_SOLDIERS: [(i64, f64, f64); 4] = [
    (233, 18.434132014691514, 0.8654732803815559),
    (231, -11.75347210076399, -5.862016522961236),
    (234, 17.627629369923454, -12.391816789113197),
    (230, -10.452779035793856, -2.651563472451839),
];

#[test]
fn replays_every_shot_of_the_death_level() {
    use gw2_rebuild::graphwar_shot_calculator::settle::{resolve, Target};
    let mut mask = empty_mask();
    for (x, y, r) in REPLAY_OBSTACLES {
        rasterize(&mut mask, x, y, r, 0x00);
    }
    let targets: Vec<Target> = REPLAY_SOLDIERS
        .iter()
        .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();

    // (函数原文, 起点 x, 起点 y, 抓到的终点 x, flip)
    let shots: [(&str, f64, f64, f64, bool); 6] = [
        ("0.00", -10.452779035793856, -2.651563472451839, -4.743404035794105, false),
        ("(-1.65/(-8.04/x))", 17.627629369923454, -12.391816789113197, -24.99362063007402, true),
        ("0.00", -11.75347210076399, -5.862016522961236, 2.309027899235557, false),
        ("abs((5.09+x))", 18.434132014691514, 0.8654732803815559, -5.094617985308956, true),
        ("0.00", -11.75347210076399, -5.862016522961236, 2.852777899235552, false),
        ("(-1.65/(-8.04/x))", 17.627629369923454, -12.391816789113197, -24.55049563007283, true),
    ];

    let mut worst = 0.0_f64;
    for (text, x0, y0, end_x, flip) in shots {
        let others: Vec<Target> = targets.iter().filter(|t| (t.x - x0).abs() > 1e-9 || (t.y - y0).abs() > 1e-9).copied().collect();
        let r = resolve(text, x0, y0, 0.5, flip, Mode::Normal, &mask, &others);
        let d = (r.stop.0 - end_x).abs();
        assert!(d < 0.5, "{text}: 停止点 x={} 与抓到的 {end_x} 相差 {d}", r.stop.0);
        worst = worst.max(d);
    }
    assert!(worst > 0.0, "至少应有一发有偏差（采样步长所致）");
}

#[test]
fn match_replays_the_death_level_through_the_protocol_layer() {
    use gw2_rebuild::game::Match;
    use gw2_rebuild::_scaffold::protocol::{get_f64, get_int, get_str, parse_message};
    use gw2_rebuild::server::RoomState;
    use gw2_rebuild::graphwar_shot_calculator::settle::Target;

    let room = RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    };
    let soldiers: Vec<Target> = REPLAY_SOLDIERS
        .iter()
        .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    let mut m = Match::new(room, REPLAY_OBSTACLES.to_vec(), soldiers, (5, 0));

    // 与 replays_every_shot_of_the_death_level 相同的输入序列
    let shots: [(&str, i64, f64, f64, f64, bool); 6] = [
        ("0.00", 230, -10.453270554921136, -2.651563472451839, -4.743, false),
        ("(-1.65/(-8.04/x))", 234, 17.627629369923454, -12.391816789113197, -24.994, true),
        ("0.00", 231, -11.75347210076399, -5.862016522961236, 2.309, false),
        ("abs((5.09+x))", 233, 18.434029032407364, 0.8649132574204391, -5.095, true),
        ("0.00", 231, -11.75347210076399, -5.862016522961236, 2.853, false),
        ("(-1.65/(-8.04/x))", 234, 17.629369923454 * 0.0 + 17.627629369923454, -12.391816789113197, -24.550, true),
    ];

    let mut killed_all: Vec<i64> = Vec::new();
    for (i, (text, owner, x0, y0, end_x, flip)) in shots.iter().enumerate() {
        let msgs = m.fire(*owner, text, *x0, *y0, *flip, (i as i64 + 1) * 1000);
        // 首条必须是 FunctionPoints，且终点与抓包相差 < 0.5
        let points = msgs.iter().filter_map(|m| parse_message(m))
            .find(|p| p.kind == "FunctionPoints")
            .unwrap_or_else(|| panic!("第 {i} 发没有 FunctionPoints"));
        let got_end = get_f64(&points, "end_x").unwrap();
        // 多发射击会累积误差：每发在**我们的停止点**挖洞（真实游戏在真实命中点挖洞），
        // 我们的停止点本身有 ~0.4 的采样误差，洞就偏了，后一发随之偏移。
        // 因此这里用 1.0 的容差；单发验证（docs/25）仍用 0.5。
        assert!((got_end - end_x).abs() < 1.0, "第 {i} 发终点 x={got_end} 与抓到的 {end_x} 相差过大");
        // 每条都必须是可解析的合法消息
        for msg in &msgs {
            assert!(parse_message(msg).is_some(), "第 {i} 发产出非法消息: {msg}");
        }
        // 收集致死对象
        for msg in &msgs {
            if let Some(parsed) = parse_message(msg) {
                if parsed.kind == "LifeQueuedDeath" {
                    killed_all.push(get_int(&parsed, "entity_id").unwrap());
                }
                if parsed.kind == "EffectInfo" {
                    assert!(!get_str(&parsed, "effect_type").unwrap().is_empty());
                }
            }
        }
    }
    // 抓包里这局死的是 230 与 231。这里**不能**断言"恰好kill这两个"：
    // `FunctionInfo` 会随回合更新，而提取时只保留了每个实体的最后一条，
    // 杀死 230 的那发函数原文没有留存，所以复算出的致死集合只能做"不越界"检查。
    killed_all.sort_unstable();
    killed_all.dedup();
    for id in &killed_all {
        assert!(*id == 230 || *id == 231, "复算出抓包里没死过的实体 {id}，说明模型有误");
    }
}

// ---------------------------------------------------------------------------
// 按回合配对的精确回放（函数原文取自开火时刻，见 tools/replay_extract.py）
// ---------------------------------------------------------------------------

const PAIRED_OBSTACLES: [(f64, f64, f64); 8] = [
    (14.046310675855437, -14.924477914559672, 2.352930364790025),
    (29.696182564548714, -12.163323118078395, 2.6766182869779684),
    (-13.680667750408213, 17.948543431250446, 4.749150775191251),
    (4.49738461075144, -11.137813176563517, 3.449605100302236),
    (8.009478662892661, -4.329916910826, 3.165704264535919),
    (-1.4935534327929743, -8.121366608231103, 1.7039845170941725),
    (-17.75327174189561, 2.071423010597829, 3.7050938867242724),
    (11.655733033805847, -18.264933522033925, 4.756526761470473),
];

const PAIRED_SOLDIERS: [(i64, f64, f64); 4] = [
    (234, 2.7158137579070476, -5.845221853870052),
    (231, -14.352153074285424, -11.263853493977628),
    (230, -3.046663662665346, -10.672775349227418),
    (233, 7.366983680391016, 4.8970427823140135),
];

#[test]
fn paired_replay_reproduces_exact_kill_sets() {
    use gw2_rebuild::game::Match;
    use gw2_rebuild::_scaffold::protocol::{get_f64, get_int, parse_message};
    use gw2_rebuild::server::RoomState;
    use gw2_rebuild::graphwar_shot_calculator::settle::Target;

    let room = RoomState {
        room_name: "Graphwar Game".into(), game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(), function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(), time_mode: "Timer1m".into(),
    };
    let soldiers: Vec<Target> = PAIRED_SOLDIERS.iter()
        .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
        .collect();
    let mut m = Match::new(room, PAIRED_OBSTACLES.to_vec(), soldiers, (5, 0));

    // (函数原文, 射手, 起点 x, 起点 y, 抓到的终点 x, flip, 抓到的致死集合)
    let shots: [(&str, i64, f64, f64, f64, bool, &[i64]); 4] = [
        ("0.00", 230, -3.046663662665346, -10.672775349227418, 1.0502113373346385, false, &[]),
        ("abs((11.66-x))", 233, 7.366983680391016, 4.8970427823140135, -24.995516319609788, true, &[231]),
        ("0.00", 230, -3.046663662665346, -10.672775349227418, 1.6033363373346332, false, &[]),
        ("(exp(x)*(3.34-x))", 234, 2.7158137579070476, -5.845221853870052, -3.58317061709248, true, &[230]),
    ];

    for (i, (text, owner, x0, y0, end_x, flip, expect_kills)) in shots.iter().enumerate() {
        let msgs = m.fire(*owner, text, *x0, *y0, *flip, (i as i64 + 1) * 1000);
        let points = msgs.iter().filter_map(|m| parse_message(m))
            .find(|p| p.kind == "FunctionPoints")
            .unwrap_or_else(|| panic!("第 {i} 发没有 FunctionPoints"));
        let got_end = get_f64(&points, "end_x").unwrap();
        // 同上：多发射击的挖洞位置误差会累积，容差放到 1.0
        assert!((got_end - end_x).abs() < 1.0,
                "第 {i} 发终点 x={got_end} 与抓到的 {end_x} 相差过大");

        let mut kills: Vec<i64> = msgs.iter()
            .filter_map(|m| parse_message(m))
            .filter(|p| p.kind == "LifeQueuedDeath")
            .map(|p| get_int(&p, "entity_id").unwrap())
            .collect();
        kills.sort_unstable();
        assert_eq!(&kills[..], *expect_kills, "第 {i} 发致死集合与抓包不符（函数 {text}）");
    }
}

// ---------------------------------------------------------------------------
// `end_x` 的取值点（`docs/06` §27）
// ---------------------------------------------------------------------------

/// **`FunctionPoints.end_x` 与 `EffectInfo(Explosion).pos_x` 是同一个采样点**：
/// 两条报文在二进制里都取**同一条点序列的最后一个元素**
/// （arm64 `0x489428` `ldur d1,[x8,#-0x10]` / `0x4a891c` `ldp d9,d8,[x8,#-0x10]`，
/// 同一个 `len` = 队列项 `+0x10`）。
///
/// 撞障碍时那条点序列的末元素**就是阻挡点**，不是阻挡点之前那个 ——
/// 改成"退一格"会让整局对拍 324 → 229（`docs/06` §27.4），所以这里把两点都钉住。
#[test]
fn end_x_and_explosion_x_are_the_same_sample_point() {
    use gw2_rebuild::game::Match;
    use gw2_rebuild::graphwar_path_calculator::Mode;
    use gw2_rebuild::graphwar_shot_calculator::settle::Target;
    use gw2_rebuild::graphwar_shot_calculator::{corrected_text, shot_points};
    use gw2_rebuild::server::RoomState;
    use gw2_rebuild::_scaffold::protocol::{get_f64, get_str, parse_message};

    fn room() -> RoomState {
        RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        }
    }

    // (障碍, 是否该被挡)：障碍放在修正后曲线 `y = 0.4x + 4.8` 上（x=0 处 y=4.8）
    let cases: [(Vec<(f64, f64, f64)>, bool); 2] =
        [(vec![], false), (vec![(0.0, 4.8, 2.0)], true)];

    for (obstacles, must_block) in cases {
        let mut mask = empty_mask();
        for (x, y, r) in &obstacles {
            rasterize(&mut mask, *x, *y, *r, 0x00);
        }
        let soldiers = vec![Target { id: 7, x: -12.0, y: 0.0, radius: 0.5 }];
        let mut m = Match::new(room(), obstacles.clone(), soldiers, (5, 0));
        let (text, x0, y0, flip) = ("0.4x", -12.0, 0.0, false);
        let msgs = m.fire(7, text, x0, y0, flip, 1000);

        let mut end_x = None;
        let mut expl = None;
        let mut destroyed = false;
        for msg in &msgs {
            let Some(p) = parse_message(msg) else { continue };
            match p.kind.as_str() {
                "FunctionPoints" => end_x = Some(get_f64(&p, "end_x").unwrap()),
                "QueueDestroyObstacle" => destroyed = true,
                "EffectInfo" if get_str(&p, "effect_type").as_deref() == Some("ExplosionEffect") => {
                    expl = Some(get_f64(&p, "pos_x").unwrap());
                }
                _ => {}
            }
        }
        assert_eq!(destroyed, must_block, "这一发的阻挡结果与夹具不符（障碍 {obstacles:?}）");
        let end_x = end_x.expect("缺 FunctionPoints");
        let expl = expl.expect("缺 ExplosionEffect");
        assert_eq!(
            end_x.to_bits(),
            expl.to_bits(),
            "end_x({end_x}) 与爆炸点 pos_x({expl}) 必须是同一个采样点",
        );

        // 同一个采样点 = 点序列里**第一个被障碍占据的那个点**（没撞障碍时就是最后一个点）。
        let corrected = corrected_text(text, x0, y0, flip);
        let pts = shot_points(&corrected, x0, y0, gw2_rebuild::SERVER_STEP, flip, Mode::Normal, false);
        let want = pts
            .iter()
            .find(|&&(px, py)| is_occupied(&mask, px, py))
            .map(|p| p.0)
            .unwrap_or_else(|| pts.last().expect("点序列不能为空").0);
        assert_eq!(
            end_x.to_bits(),
            want.to_bits(),
            "end_x 必须取「阻挡点」本身（{want}），不是它前一个采样点",
        );
    }
}
