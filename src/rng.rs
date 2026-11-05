//! ChaCha12 随机数生成器（与游戏同款算法）。
//!
//! 状态布局照游戏里的实现（`docs/53`）：`[σ(4), key(8), counter_lo, counter_hi, nonce(2)]`，
//! 一次产出 64 字节（8 个 u64）。
//!
//! 服务端用它来生成关卡；种子从 `/dev/urandom` 取（与游戏的种子来源一致，见 `docs/57`）。

use std::io::Read;

const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

fn rotl(x: u32, n: u32) -> u32 {
    x.rotate_left(n)
}

#[inline]
fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = rotl(s[d] ^ s[a], 16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = rotl(s[b] ^ s[c], 12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = rotl(s[d] ^ s[a], 8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = rotl(s[b] ^ s[c], 7);
}

/// ChaCha12（12 轮 = 6 次双轮）。
pub struct ChaCha12 {
    key: [u32; 8],
    nonce: [u32; 2],
    counter: u64,
    buf: [u64; 8],
    idx: usize,
}

impl ChaCha12 {
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let mut key = [0u32; 8];
        for (i, k) in key.iter_mut().enumerate() {
            *k = u32::from_le_bytes([
                seed[i * 4],
                seed[i * 4 + 1],
                seed[i * 4 + 2],
                seed[i * 4 + 3],
            ]);
        }
        ChaCha12 { key, nonce: [0, 0], counter: 0, buf: [0; 8], idx: 8 }
    }

    fn block(&mut self) {
        let mut st = [0u32; 16];
        st[0..4].copy_from_slice(&SIGMA);
        st[4..12].copy_from_slice(&self.key);
        st[12] = self.counter as u32;
        st[13] = (self.counter >> 32) as u32;
        st[14] = self.nonce[0];
        st[15] = self.nonce[1];
        let mut x = st;
        for _ in 0..6 {
            quarter(&mut x, 0, 4, 8, 12);
            quarter(&mut x, 1, 5, 9, 13);
            quarter(&mut x, 2, 6, 10, 14);
            quarter(&mut x, 3, 7, 11, 15);
            quarter(&mut x, 0, 5, 10, 15);
            quarter(&mut x, 1, 6, 11, 12);
            quarter(&mut x, 2, 7, 8, 13);
            quarter(&mut x, 3, 4, 9, 14);
        }
        for i in 0..16 {
            x[i] = x[i].wrapping_add(st[i]);
        }
        for i in 0..8 {
            self.buf[i] = (x[i * 2] as u64) | ((x[i * 2 + 1] as u64) << 32);
        }
        self.idx = 0;
        self.counter = self.counter.wrapping_add(1);
    }

    pub fn next_u64(&mut self) -> u64 {
        if self.idx >= 8 {
            self.block();
        }
        let v = self.buf[self.idx];
        self.idx += 1;
        v
    }
}

/// 从 `/dev/urandom` 取 32 字节种子。失败时退回时间戳（不理想，但至少不 panic）。
pub fn seed_from_os() -> [u8; 32] {
    let mut seed = [0u8; 32];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(&mut seed).is_ok() {
            return seed;
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let mut s = now;
    for chunk in seed.chunks_mut(8) {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        chunk.copy_from_slice(&s.to_le_bytes());
    }
    seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_advance_and_differ() {
        let mut r = ChaCha12::from_seed(&[7u8; 32]);
        let a: Vec<u64> = (0..20).map(|_| r.next_u64()).collect();
        let mut r2 = ChaCha12::from_seed(&[7u8; 32]);
        let b: Vec<u64> = (0..20).map(|_| r2.next_u64()).collect();
        assert_eq!(a, b, "同种子必须同序列");
        assert!(a.windows(2).all(|w| w[0] != w[1]), "不应连续重复");
        let mut r3 = ChaCha12::from_seed(&[9u8; 32]);
        let c: Vec<u64> = (0..5).map(|_| r3.next_u64()).collect();
        assert_ne!(&a[..5], &c[..], "不同种子应给出不同序列");
    }

    /// ziggurat/关卡生成只要求"像样的均匀分布"；这里做基本统计检查。
    #[test]
    fn next_u64_is_roughly_uniform() {
        let mut r = ChaCha12::from_seed(&[1u8; 32]);
        let mut buckets = [0usize; 16];
        for _ in 0..16000 {
            let v = r.next_u64() >> 60;
            buckets[v as usize] += 1;
        }
        let avg = 16000 / 16;
        for (i, b) in buckets.iter().enumerate() {
            assert!(
                (*b as f64) > avg as f64 * 0.7 && (*b as f64) < avg as f64 * 1.3,
                "第 {i} 桶 {b} 偏离均值 {avg} 太多"
            );
        }
    }

    #[test]
    fn os_seed_is_available() {
        let s = seed_from_os();
        assert_eq!(s.len(), 32);
        // 不同次调用应当不同（概率上）
        let t = seed_from_os();
        assert_ne!(s, t, "两次种子不应相同");
    }
}
