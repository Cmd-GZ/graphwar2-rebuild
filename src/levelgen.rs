//! 关卡生成：把 `docs/55`/`docs/56` 反出来的公式实现成代码。
//!
//! 公式（全部由真实抓包验证，见 `docs/56`）：
//!
//! ```text
//! n = trunc(normal() * 7.0 + 15.0)         // 障碍个数
//! 每个障碍：
//!   r = max(normal() * 1.6 + 2.6, 0.8)     // 半径，下限 0.8
//!   u = (next_u64() >> 11) * 2^-53
//!   x = (2r + 25.0) * (2u - 1.0)
//!   v = (next_u64() >> 11) * 2^-53
//!   y = (2r + 15.0) * (2v - 1.0)
//! ```
//!
//! `normal()` 是 ziggurat 标准正态采样器，表从 `.rodata` 原样导出
//! （`levelgen_tables.rs`）。采样器会**消耗不定数量**的随机数（拒绝采样），
//! 这也是为什么不能用"固定偏移"去猜半径的来源。

use crate::levelgen_tables::{ZIG_NORM_F, ZIG_NORM_X};

/// 一个障碍。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstacle {
    pub x: f64,
    pub y: f64,
    pub r: f64,
}

/// 把 64 位整数当作 `f64` 的位模式来解释。
fn from_bits(b: u64) -> f64 {
    f64::from_bits(b)
}

/// ziggurat 标准正态采样（与游戏内 `FUN_005625b0` 等价）。
///
/// `next` 每次返回一个 64 位随机数（与 `next_u64` 相同）。
pub fn normal(next: &mut dyn FnMut() -> u64) -> f64 {
    loop {
        let u = next();
        let i = (u & 0xff) as usize;
        // [2,4) 的位模式减去 3 → [-1,1)，再乘上 X[i]
        let x = ZIG_NORM_X[i] * (from_bits((u >> 12) | 0x4000_0000_0000_0000) - 3.0);
        if x.abs() < ZIG_NORM_X[i + 1] {
            return x;
        }
        if i == 0 {
            // 尾部：对数采样（R = ZIG_NORM_X[0]）
            let r = ZIG_NORM_X[0];
            loop {
                let a = next();
                let b = next();
                let la = (from_bits((a >> 12) | 0x3FF0_0000_0000_0000) - 1.0).max(f64::MIN_POSITIVE).ln();
                let lb = (from_bits((b >> 12) | 0x3FF0_0000_0000_0000) - 1.0).max(f64::MIN_POSITIVE).ln();
                if lb * -2.0 >= (la / r) * (la / r) {
                    return -la / r;
                }
            }
        }
        // 楔形区域：用下一个均匀数在 F[i+1..i+2] 之间插值，与密度比较。
        // i == 255 时 F[i+2] 越界——这一桶的真实取值我们没有观测到（夹具也没触发），
        // 这里直接重抽，宁可慢一点也不要凭空造一个值。
        if i + 2 >= ZIG_NORM_F.len() {
            continue;
        }
        let u2 = next();
        let f2 = ((u2 >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
        if ZIG_NORM_F[i + 2] + f2 * (ZIG_NORM_F[i + 1] - ZIG_NORM_F[i + 2])
            < (-0.5 * x * x).exp()
        {
            return x;
        }
    }
}

/// 生成一关。`min_count` 是个数下限（游戏里由调用方传入）。
pub fn generate(next: &mut dyn FnMut() -> u64, min_count: i32) -> Vec<Obstacle> {
    let mut n = (normal(next) * 7.0 + 15.0) as i32;
    if n <= min_count {
        n = min_count;
    }
    let mut out = Vec::with_capacity(n.max(0) as usize);
    for _ in 0..n {
        let mut r = normal(next) * 1.6 + 2.6;
        if r <= 0.8 {
            r = 0.8;
        }
        let u = ((next() >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
        let x = (2.0 * r + 25.0) * (2.0 * u - 1.0);
        let v = ((next() >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
        let y = (2.0 * r + 15.0) * (2.0 * v - 1.0);
        out.push(Obstacle { x, y, r });
    }
    out
}

/// "中心单障碍"模式（`FUN_0059f370`，见 `docs/72`）。
///
/// 整关**只有 1 个障碍**，位置固定在原点，半径 `u*5.0 + 3.0 ∈ [3, 8)`。
/// 只消耗 1 次随机数——比散布模式省得多，这也是它容易认出来的特征。
pub fn generate_center(next: &mut dyn FnMut() -> u64) -> Vec<Obstacle> {
    let u = ((next() >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
    vec![Obstacle {
        x: 0.0,
        y: 0.0,
        r: u * 5.0 + 3.0,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws() -> Vec<u64> {
        include_str!("../fixtures/levelgen_draws.txt")
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| u64::from_str_radix(l, 16).expect("十六进制"))
            .collect()
    }

    fn obstacles() -> Vec<Obstacle> {
        include_str!("../fixtures/levelgen_obstacles.txt")
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| {
                let mut it = l.split('\t');
                Obstacle {
                    x: it.next().unwrap().parse().unwrap(),
                    y: it.next().unwrap().parse().unwrap(),
                    r: it.next().unwrap().parse().unwrap(),
                }
            })
            .collect()
    }

    /// 真实随机流 → 真实障碍，必须逐位一致。
    #[test]
    fn replays_captured_level_exactly() {
        let d = draws();
        let want = obstacles();
        assert!(d.len() > 60, "夹具里的随机数太少: {}", d.len());
        assert_eq!(want.len(), 13, "夹具里的障碍数不对");

        let mut i = 3usize; // 关卡生成从第 3 个随机数开始（docs/56）
        let mut next = || {
            let v = d[i];
            i += 1;
            v
        };
        let got = generate(&mut next, 0);

        assert_eq!(got.len(), want.len(), "障碍个数与抓包不符");
        for (k, (a, b)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(a.x.to_bits(), b.x.to_bits(), "#{k} x: {} vs {}", a.x, b.x);
            assert_eq!(a.y.to_bits(), b.y.to_bits(), "#{k} y: {} vs {}", a.y, b.y);
            assert_eq!(a.r.to_bits(), b.r.to_bits(), "#{k} r: {} vs {}", a.r, b.r);
        }
    }

    #[test]
    fn normal_is_reasonably_distributed() {
        let mut s: u64 = 0x243f_6a88_85a3_08d3;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let (mut sum, mut sum2, n) = (0.0, 0.0, 20000);
        for _ in 0..n {
            let v = normal(&mut next);
            sum += v;
            sum2 += v * v;
        }
        let mean = sum / n as f64;
        let var = sum2 / n as f64 - mean * mean;
        assert!(mean.abs() < 0.1, "均值 {mean} 偏离太大");
        assert!((var - 1.0).abs() < 0.1, "方差 {var} 偏离太大");
    }

    #[test]
    fn center_mode_is_one_obstacle_at_origin() {
        let mut d = draws().into_iter();
        let mut next = move || d.next().unwrap();
        let obs = generate_center(&mut next);
        assert_eq!(obs.len(), 1, "中心模式整关只有 1 个障碍");
        assert_eq!((obs[0].x, obs[0].y), (0.0, 0.0), "位置固定在原点");
        assert!(
            (3.0..8.0).contains(&obs[0].r),
            "半径应在 [3,8) 内，实测 {}",
            obs[0].r
        );
    }

}
