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

use gw2_rebuild::path::Mode;
use gw2_rebuild::plane::{empty_mask, is_occupied, rasterize};
use gw2_rebuild::shot::shot_points;

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
fn first_hit(mask: &[u8], pts: &[(f64, f64)], x0: f64, y0: f64) -> Option<(f64, f64)> {
    for &(px, py) in pts {
        if ((px - x0).powi(2) + (py - y0).powi(2)).sqrt() <= 0.5 {
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
        let pts = shot_points("0.00", x0, y0, 0.5, flip, Mode::Normal, true);
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
