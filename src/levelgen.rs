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
//!
//! ## 三个函数的关系（`docs/103` 结论）
//!
//! * [`generate`] —— 随机散布，**真·关卡生成器**（`FUN_003f1e50`）；
//! * [`generate_center`] —— 中心单障碍，**真·关卡生成器**（`FUN_0049f370`，
//!   以函数指针形式挂在 `.data.rel.ro` 槽位 `0x95ad60` 上）；
//! * [`generate_disk_pattern`] —— 圆盘式（`FUN_004ea6dc`），**不是关卡生成器**：
//!   它不调 `place_obstacle`，写出的也不是障碍的 32 字节布局，而是
//!   跳转表分派器 `FUN_004ea690` 的 case 0 产生的 0x30 字节"图案段"。
//!   把它当成第三种关卡模式是 `docs/70` 的误读（`docs/72` 已提出怀疑）。

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

/// 圆盘式生成器的入口（`FUN_004ea6dc`，虚地址 `0x4ea6dc`，Ghidra `0x5ea6dc`）。
///
/// 它是跳转表分派器 `FUN_004ea690` 的 **case 0**（表在虚地址 `0x1c3ba8`），
/// 同一个分派器里还有 27 个以上的兄弟 case（`0x4ea8a0`/`0x4ea9bc`/`0x4eab32`/
/// `0x4eaca6` …），全部是同一族"图案段"生成器。
pub const DISK_PATTERN_KIND: u8 = 0;

/// 图案段：分派器一族写出的 **0x30 字节**记录。
///
/// ⚠️ **这不是障碍**。障碍是 32 字节的 `{tag, x, y, r}`，由 `place_obstacle`
/// （`FUN_003f17a0`）推进 Vec；下面这个结构体没有半径字段，记录条数也多一个量级。
///
/// 布局（观测，逐条对照 `0x4ea710`–`0x4ea75b` 的汇编，见 `docs/103` §3）：
///
/// ```text
/// +0x00  f64  a       // 分派器入参 xmm0（圆盘式：整批 80 条共用同一个 a）
/// +0x08  f64  b       // 分派器入参 xmm1
/// +0x10  f64  c       // 圆盘式：sin(2π·u1) · r
/// +0x18  f64  d       // 圆盘式：cos(2π·u1) · r
/// +0x20  u32  kind    // 分派器第 3 个入参 + 10
/// +0x24  u32  style   // 圆盘式恒为 0xf
/// +0x28  u8   flag    // 前 64 条 = 1，后 16 条 = 0
/// ```
///
/// `(a,b)` 与 `(c,d)` 是否就是一条线段的两个端点属于**推断**（依据：同族的
/// `FUN_004ea8a0` 在 `+0x00` 写的是中心附近的抖动值，而不是常数）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternSegment {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub kind: u32,
    pub style: u32,
    pub flag: u8,
}

/// `(next_u64() >> 11) * 2^-53`——游戏里任何"均匀数"都这么取
/// （常量 `2^-53` 在 `.rodata` 虚地址 `0x19f578`）。
fn uniform(next: &mut dyn FnMut() -> u64) -> f64 {
    ((next() >> 11) as f64) * (1.0 / 9007_199_254_740_992.0)
}

/// 往 Vec 里追加 `n` 条"落在半径 ≤ `max_r` 的圆盘内"的图案段。
///
/// 每条记录**恰好消耗 2 个** `next_u64`，顺序固定：先 `u1`（角度），后 `u2`（半径）。
#[allow(clippy::too_many_arguments)]
fn push_disk_pattern(
    out: &mut Vec<PatternSegment>,
    next: &mut dyn FnMut() -> u64,
    n: usize,
    max_r: f64,
    flag: u8,
    type_code: u32,
    centre: (f64, f64),
) {
    for _ in 0..n {
        let u1 = uniform(next);
        let u2 = uniform(next);
        let r = u2 * max_r;
        // 汇编是 `sincos(x, &sin, &cos)`：`sin` 落 `+0x10`，`cos` 落 `+0x18`。
        let (sin, cos) = ((u1 + u1) * std::f64::consts::PI).sin_cos();
        out.push(PatternSegment {
            a: centre.0,
            b: centre.1,
            c: sin * r,
            d: cos * r,
            kind: type_code.wrapping_add(10),
            style: 0xf,
            flag,
        });
    }
}

/// 圆盘式图案：**80 条记录** = 64 条落在半径 ≤ 7.5 的圆盘内
/// + 16 条落在半径 ≤ 3.0 的圆盘内。总共消耗 **160 个** `next_u64`。
///
/// 与 [`generate`] 的差别不只是参数：散布模式用 ziggurat 正态采样（拒绝采样，
/// 消耗不定个随机数）并且调 `place_obstacle`；这里只用均匀数、定长消耗、
/// 直接写记录结构体。
///
/// `type_code` 是分派器第 3 个入参（`rdx`），落进记录的 `kind` 字段（`+10`）。
/// `centre` 是分派器的两个 f64 入参，整批 80 条原样复制。
pub fn generate_disk_pattern(
    next: &mut dyn FnMut() -> u64,
    type_code: u32,
    centre: (f64, f64),
) -> Vec<PatternSegment> {
    let mut out = Vec::with_capacity(80);
    push_disk_pattern(&mut out, next, 64, 7.5, 1, type_code, centre);
    push_disk_pattern(&mut out, next, 16, 3.0, 0, type_code, centre);
    out
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

    /// 确定性伪随机流（测试用）。
    ///
    /// 真值夹具 `levelgen_draws.txt` 只有 120 个数，而圆盘式一批要 160 个，
    /// 所以形状类测试改用 xorshift 造流；真实随机流的对拍见
    /// `disk_pattern_first_60_follow_real_draw_stream`。
    fn stream(n: usize) -> Vec<u64> {
        let mut s: u64 = 0x243f_6a88_85a3_08d3;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                s
            })
            .collect()
    }

    /// 圆盘式：64 条（半径 ≤7.5）+ 16 条（半径 ≤3.0），全部落在圆盘内。
    #[test]
    fn disk_pattern_is_two_disks_of_64_and_16() {
        let mut d = stream(200).into_iter();
        let mut next = move || d.next().unwrap();
        let segs = generate_disk_pattern(&mut next, 3, (1.5, -2.5));
        assert_eq!(segs.len(), 80, "整批固定 80 条，与随机流无关");

        for (i, s) in segs.iter().enumerate() {
            let (max_r, flag) = if i < 64 { (7.5, 1u8) } else { (3.0, 0u8) };
            assert_eq!((s.a, s.b), (1.5, -2.5), "#{i} 中心坐标应当整批原样复制");
            assert_eq!(s.flag, flag, "#{i} flag");
            assert_eq!(s.style, 0xf, "#{i} style 恒为 0xf");
            assert_eq!(s.kind, 13, "#{i} kind = 入参 + 10");
            let r = (s.c * s.c + s.d * s.d).sqrt();
            assert!(r <= max_r, "#{i} 半径 {r} 超出圆盘上限 {max_r}");
        }

        // 反证"半径不是常数、也不是只贴着外圈"：64 条均匀抽样里最大值应接近外沿。
        let outer = segs[..64]
            .iter()
            .map(|s| (s.c * s.c + s.d * s.d).sqrt())
            .fold(0.0_f64, f64::max);
        assert!(outer > 7.0, "64 条抽样里最大半径只有 {outer}，不像 [0,7.5)");
    }

    /// 每条记录恰好 2 个 `next_u64`：先角度（u1），后半径（u2）；整批 160 个。
    #[test]
    fn disk_pattern_consumes_two_draws_per_segment() {
        let d = stream(200);
        let mut i = 0usize;
        let mut next = || {
            let v = d[i];
            i += 1;
            v
        };
        let segs = generate_disk_pattern(&mut next, 0, (0.0, 0.0));
        assert_eq!(segs.len(), 80);
        assert_eq!(i, 160, "80 条 × 2 个随机数");
    }

    /// 与随机流逐条对齐：第 k 条用的就是第 2k、2k+1 个随机数。
    #[test]
    fn disk_pattern_follows_the_draw_stream_exactly() {
        let d = stream(200);
        let mut i = 0usize;
        let mut next = || {
            let v = d[i];
            i += 1;
            v
        };
        let segs = generate_disk_pattern(&mut next, 7, (0.0, 0.0));
        for (k, s) in segs.iter().enumerate() {
            let u1 = ((d[2 * k] >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
            let u2 = ((d[2 * k + 1] >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
            let max_r = if k < 64 { 7.5 } else { 3.0 };
            let r = u2 * max_r;
            let (sin, cos) = ((u1 + u1) * std::f64::consts::PI).sin_cos();
            assert_eq!(s.c.to_bits(), (sin * r).to_bits(), "#{k} +0x10 应为 sin·r");
            assert_eq!(s.d.to_bits(), (cos * r).to_bits(), "#{k} +0x18 应为 cos·r");
            assert_eq!(s.kind, 17, "#{k} kind");
        }
    }

    /// 用**真实随机流**（夹具前 120 个数 = 60 条记录）驱动，逐位对公式。
    ///
    /// 注意证据等级：这锁的是"实现 ↔ 反出来的公式"，**不是**"实现 ↔ 真实关卡"——
    /// 圆盘式没有真值样本（见 `docs/103` §6）。
    #[test]
    fn disk_pattern_first_60_follow_real_draw_stream() {
        let d = draws();
        let mut i = 0usize;
        let mut next = || {
            let v = d[i];
            i += 1;
            v
        };
        // 只跑外圈那一段的前 60 条：恰好用掉夹具里全部 120 个随机数。
        let mut segs = Vec::new();
        push_disk_pattern(&mut segs, &mut next, 60, 7.5, 1, 0, (-1.0, 2.0));
        assert_eq!(segs.len(), 60);
        assert_eq!(i, 120, "60 条 × 2 个随机数 = 夹具全部");
        for (k, s) in segs.iter().enumerate() {
            let u1 = ((d[2 * k] >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
            let u2 = ((d[2 * k + 1] >> 11) as f64) * (1.0 / 9007_199_254_740_992.0);
            let r = u2 * 7.5;
            let (sin, cos) = ((u1 + u1) * std::f64::consts::PI).sin_cos();
            assert_eq!((s.a, s.b), (-1.0, 2.0), "#{k} 中心");
            assert_eq!(s.c.to_bits(), (sin * r).to_bits(), "#{k} c");
            assert_eq!(s.d.to_bits(), (cos * r).to_bits(), "#{k} d");
        }
    }

    /// 半径在圆盘内**均匀**（而不是落在圆环上）：均值 ≈ 上限的一半。
    ///
    /// 这一条正是"圆盘式"与"环状"的判别依据——同族的 `FUN_004ea8a0` 才是环。
    #[test]
    fn disk_pattern_radii_are_uniform_inside_the_disk() {
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        let segs = generate_disk_pattern(&mut next, 0, (0.0, 0.0));
        let rs: Vec<f64> = segs[..64]
            .iter()
            .map(|s| (s.c * s.c + s.d * s.d).sqrt())
            .collect();
        let mean = rs.iter().sum::<f64>() / rs.len() as f64;
        assert!(
            (mean - 3.75).abs() < 0.6,
            "半径均值 {mean}，不像 [0,7.5) 上的均匀分布（圆环会接近 7.5）"
        );
    }
}
