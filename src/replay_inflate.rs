//! 纯 Rust 的 DEFLATE / gzip 解压（RFC 1951 + RFC 1952）。
//!
//! ## 为什么不加 `flate2` 依赖
//!
//! 本 crate 的 `[dependencies]` 是**空的**（`Cargo.toml`），`cargo test --offline`
//! 才在任何机器上都能跑。加一个 crate 就要联网 + 改 `Cargo.lock`，与这条约定冲突。
//! 回放只需要"把字节流还原"，DECODER 本体不到 300 行，所以自己写。
//!
//! ## 覆盖范围（都是**观测**：400 份真实回放全部解得出）
//!
//! * gzip 头（`1f 8b 08`），支持 `FEXTRA` / `FNAME` / `FCOMMENT` / `FHCRC` 四个可选段；
//! * 可拼接的多 member（RFC 1952 §2.2）；
//! * DEFLATE 三种块：stored（未压缩）、固定 Huffman、动态 Huffman；
//! * **不校验** gzip 尾部的 CRC32 / ISIZE（回放不是校验场景；解压失败由 DEFLATE
//!   结构自身报错）。这是**有意省略**，写在 `docs/133` §6。
//!
//! 实现口径对齐 `puff.c`（zlib 作者写的教学解压器）：逐位读、规范 Huffman 的
//! `count/first/index` 三数组解码法。

/// 解压失败的原因（`Copy`，便于在 FFI 层直接映射成错误码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflateError {
    /// 输入不是 DEFLATE / gzip（头不对、块类型非法）。
    BadFormat,
    /// 数据被截断（比特流提前结束）。
    Truncated,
    /// 距离超出已解出的窗口。
    BadDistance,
    /// Huffman 码表不完整 / 全是零长度 —— 无法解码。
    BadTable,
}

impl InflateError {
    pub fn as_str(self) -> &'static str {
        match self {
            InflateError::BadFormat => "bad_format",
            InflateError::Truncated => "truncated",
            InflateError::BadDistance => "bad_distance",
            InflateError::BadTable => "bad_table",
        }
    }
}

// ---------------------------------------------------------------------------
// 比特读入（LSB-first，RFC 1951 §3.1.1）
// ---------------------------------------------------------------------------

struct BitReader<'a> {
    data: &'a [u8],
    /// 下一个要读的**比特**在 `data` 里的下标（`pos / 8` 是字节）。
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0 }
    }

    fn bit(&mut self) -> Result<u32, InflateError> {
        let byte = self.pos >> 3;
        if byte >= self.data.len() {
            return Err(InflateError::Truncated);
        }
        let bit = (self.data[byte] >> (self.pos & 7)) & 1;
        self.pos += 1;
        Ok(bit as u32)
    }

    /// 读 `n` 位（`n <= 16`），**低位先出现** ⇒ 累加时先读到的在最低位。
    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }

    /// 对齐到字节边界（stored 块前必须调）。
    fn align(&mut self) {
        let rem = self.pos & 7;
        if rem != 0 {
            self.pos += 8 - rem;
        }
    }

    fn byte(&mut self) -> Result<u8, InflateError> {
        let at = self.pos >> 3;
        if at >= self.data.len() {
            return Err(InflateError::Truncated);
        }
        self.pos += 8;
        Ok(self.data[at])
    }
}

// ---------------------------------------------------------------------------
// 规范 Huffman 表（puff.c 的 count[] / symbol[] 形态）
// ---------------------------------------------------------------------------

struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// 用码长表构造。`lengths` 里 0 表示该符号不出现。
    fn build(lengths: &[u8]) -> Result<Huffman, InflateError> {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        if count[0] as usize == lengths.len() {
            return Err(InflateError::BadTable); // 全是零长度：没有可解的码
        }
        count[0] = 0;
        // 校验：不能过订阅（over-subscribed）；允许不完整（incomplete）。
        let mut left: i32 = 1;
        for len in 1..16 {
            left <<= 1;
            left -= count[len] as i32;
            if left < 0 {
                return Err(InflateError::BadTable);
            }
        }
        // 规范的起始码
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { count, symbol })
    }

    /// 逐位解码一个符号（`puff.c` 的同一段循环）。
    fn decode(&self, br: &mut BitReader) -> Result<u16, InflateError> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= br.bit()? as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                let at = index + (code - first);
                return self
                    .symbol
                    .get(at as usize)
                    .copied()
                    .ok_or(InflateError::BadTable);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::BadTable)
    }
}

// ---------------------------------------------------------------------------
// 长度 / 距离表（RFC 1951 §3.2.5）
// ---------------------------------------------------------------------------

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// 码长码表的排列顺序（RFC 1951 §3.2.7）。
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn fixed_tables() -> Result<(Huffman, Huffman), InflateError> {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist = [5u8; 30];
    Ok((Huffman::build(&lit)?, Huffman::build(&dist)?))
}

fn dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), InflateError> {
    let nlen = br.bits(5)? as usize + 257;
    let ndist = br.bits(5)? as usize + 1;
    let ncode = br.bits(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(InflateError::BadFormat);
    }
    let mut code_lengths = [0u8; 19];
    for i in 0..ncode {
        code_lengths[CODE_LENGTH_ORDER[i]] = br.bits(3)? as u8;
    }
    let code_huff = Huffman::build(&code_lengths)?;

    let mut lengths = vec![0u8; nlen + ndist];
    let mut i = 0usize;
    while i < nlen + ndist {
        let sym = code_huff.decode(br)?;
        if sym < 16 {
            lengths[i] = sym as u8;
            i += 1;
        } else {
            let (value, repeat) = match sym {
                16 => {
                    if i == 0 {
                        return Err(InflateError::BadTable);
                    }
                    (lengths[i - 1], 3 + br.bits(2)? as usize)
                }
                17 => (0u8, 3 + br.bits(3)? as usize),
                _ => (0u8, 11 + br.bits(7)? as usize), // sym == 18
            };
            if i + repeat > nlen + ndist {
                return Err(InflateError::BadFormat);
            }
            for _ in 0..repeat {
                lengths[i] = value;
                i += 1;
            }
        }
    }
    if lengths[256] == 0 {
        return Err(InflateError::BadTable); // 没有块结束符
    }
    Ok((
        Huffman::build(&lengths[..nlen])?,
        Huffman::build(&lengths[nlen..])?,
    ))
}

/// 解压一段裸 DEFLATE 流（不含 gzip 头）。返回解出的字节。
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    inflate_with_consumed(data).map(|(out, _)| out)
}

/// 同 [`inflate`]，但**同时**回报消耗了多少字节（向上取整到字节边界）。
///
/// 拼接多个 gzip member 时**必须**用这个：gzip trailer 就从"消耗字节数"处开始。
/// 早先的实现靠"往后扫 `1f 8b`"来猜下一个 member —— 结果在 400 份真实回放里有
/// **18 份**因为压缩流内部恰好出现 `1f 8b` 而跳错位置、整份解析失败
/// （`docs/133` §5.2 记录了这个 bug 与它的现象）。
pub fn inflate_with_consumed(data: &[u8]) -> Result<(Vec<u8>, usize), InflateError> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = br.bit()?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align();
                let len = br.byte()? as usize | ((br.byte()? as usize) << 8);
                let nlen = br.byte()? as usize | ((br.byte()? as usize) << 8);
                if len != (!nlen & 0xffff) {
                    return Err(InflateError::BadFormat);
                }
                for _ in 0..len {
                    out.push(br.byte()?);
                }
            }
            1 | 2 => {
                let (lit, dist) = if btype == 1 {
                    fixed_tables()?
                } else {
                    dynamic_tables(&mut br)?
                };
                loop {
                    let sym = lit.decode(&mut br)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let idx = sym as usize - 257;
                        if idx >= LENGTH_BASE.len() {
                            return Err(InflateError::BadFormat);
                        }
                        let len =
                            LENGTH_BASE[idx] as usize + br.bits(LENGTH_EXTRA[idx] as u32)? as usize;
                        let dsym = dist.decode(&mut br)? as usize;
                        if dsym >= DIST_BASE.len() {
                            return Err(InflateError::BadFormat);
                        }
                        let d = DIST_BASE[dsym] as usize
                            + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                        if d > out.len() {
                            return Err(InflateError::BadDistance);
                        }
                        let start = out.len() - d;
                        // 逐字节复制（重叠时就是 LZ77 的"自引用"，必须不能整段 copy_from_slice）
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                }
            }
            _ => return Err(InflateError::BadFormat),
        }
        if last == 1 {
            let consumed = (br.pos + 7) / 8;
            return Ok((out, consumed));
        }
    }
}

/// gzip 文件头是不是这里能解的（`1f 8b`）。
pub fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 3 && data[0] == 0x1f && data[1] == 0x8b && data[2] == 0x08
}

/// 解压一个（或多个拼接的）gzip member。输入不是 gzip 时返回 `BadFormat`。
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, InflateError> {
    if !is_gzip(data) {
        return Err(InflateError::BadFormat);
    }
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        // ---- member 头 ----
        if pos + 10 > data.len() {
            return Err(InflateError::Truncated);
        }
        if data[pos] != 0x1f || data[pos + 1] != 0x8b {
            break; // 尾部 padding：允许（有些写出器会补零）
        }
        if data[pos + 2] != 0x08 {
            return Err(InflateError::BadFormat);
        }
        let flg = data[pos + 3];
        pos += 10;
        if flg & 0x04 != 0 {
            // FEXTRA
            if pos + 2 > data.len() {
                return Err(InflateError::Truncated);
            }
            let xlen = data[pos] as usize | ((data[pos + 1] as usize) << 8);
            pos += 2 + xlen;
            if pos > data.len() {
                return Err(InflateError::Truncated);
            }
        }
        if flg & 0x08 != 0 {
            // FNAME
            while pos < data.len() && data[pos] != 0 {
                pos += 1;
            }
            pos += 1;
        }
        if flg & 0x10 != 0 {
            // FCOMMENT
            while pos < data.len() && data[pos] != 0 {
                pos += 1;
            }
            pos += 1;
        }
        if flg & 0x02 != 0 {
            // FHCRC
            pos += 2;
        }
        if pos > data.len() {
            return Err(InflateError::Truncated);
        }
        // ---- DEFLATE 本体 ----
        let (body, consumed) = inflate_with_consumed(&data[pos..])?;
        out.extend_from_slice(&body);
        // 尾部 8 字节（CRC32 + ISIZE）**不校验**（见模块文档），但要跳过它
        let trailer = pos + consumed + 8;
        if trailer >= data.len() {
            return Ok(out);
        }
        // 拼接的多个 member：只有紧邻的下一个字节确实是 `1f 8b` 才继续；
        // 否则把剩下的字节当作 padding 忽略（有些写出器会补零）。
        if data[trailer] == 0x1f && data[trailer + 1] == 0x8b {
            pos = trailer;
            continue;
        }
        return Ok(out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一段真实的 gzip 字节（Python `gzip.compress(b'hello')`），用来钉住"头 + 固定/
    /// 动态 Huffman + 尾部"这条最小路径。
    const HELLO_GZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0x48, 0xcd, 0xc9, 0xc9,
        0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
    ];

    /// 同一段 JSONL 行重复 6 次（594 字节）的 gzip —— zlib 选了**动态 Huffman**，
    /// 覆盖 `dynamic_tables()` 整条路径（码长码表 + 重复码 16/17/18）。
    const DYN_GZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xab, 0x56, 0x4a, 0x2d, 0x4b,
        0xcd, 0x2b, 0x51, 0xb2, 0xaa, 0x56, 0x72, 0x2b, 0xcd, 0x4b, 0x2e, 0xc9, 0xcc, 0xcf, 0xf3,
        0xcc, 0x4b, 0xcb, 0x07, 0xf1, 0xf3, 0xcb, 0xf3, 0x52, 0x8b, 0xe2, 0x33, 0x53, 0x94, 0xac,
        0x0c, 0x75, 0x94, 0xd2, 0xa0, 0x92, 0x4a, 0x56, 0x4a, 0xc5, 0x99, 0x79, 0x1a, 0x15, 0x9a,
        0x4a, 0x3a, 0x4a, 0xc9, 0xf9, 0x45, 0x45, 0xa9, 0xc9, 0x25, 0xa9, 0x29, 0xf1, 0x08, 0xd9,
        0xbc, 0xd2, 0x9c, 0x9c, 0xda, 0x5a, 0x1d, 0xa5, 0x92, 0xcc, 0xe4, 0x6c, 0xa0, 0x46, 0x23,
        0xe3, 0x5a, 0xae, 0xea, 0x51, 0x2b, 0x46, 0x92, 0x15, 0x00, 0x52, 0xf1, 0x62, 0x8e, 0x52,
        0x02, 0x00, 0x00,
    ];

    /// 上面那段 gzip 的原文本（594 字节 = 99 字节 × 6）。
    const DYN_LINE: &str = concat!(
        "{\"event\":{\"FunctionInfo\":{\"owner_id\":1,\"function\":\"sin(x)\",",
        "\"corrected_function\":null}},\"tick\":123}\n"
    );


    #[test]
    fn gunzip_hello() {
        assert_eq!(gunzip(HELLO_GZ).unwrap(), b"hello");
    }

    #[test]
    fn rejects_non_gzip() {
        assert_eq!(gunzip(b"not gzip at all").unwrap_err(), InflateError::BadFormat);
        assert!(!is_gzip(b"not gzip"));
    }

    #[test]
    fn stored_block() {
        // 手工构造：BFINAL=1, BTYPE=00, 然后 LEN=3/NLEN=~3, 载荷 "abc"
        let data = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&data).unwrap(), b"abc");
    }

    #[test]
    fn lz77_overlap_copy() {
        // 固定 Huffman 的 block：'a' 然后"距离 1、长度 5" —— 覆盖 LZ77 自引用这条路径。
        // 由 Python zlib 生成：zlib.compressobj(wbits=-15) 压缩 b"aaaaaa"
        let raw = [0x4b, 0x4c, 0x04, 0x01, 0x00];
        assert_eq!(inflate(&raw).unwrap(), b"aaaaaa");
    }

    #[test]
    fn dynamic_huffman_block() {
        let text = gunzip(DYN_GZ).unwrap();
        assert_eq!(text.len(), 594, "解出的字节数必须与压缩前一致");
        let expected: String = DYN_LINE.repeat(6);
        assert_eq!(String::from_utf8(text).unwrap(), expected);
    }

    #[test]
    fn truncated_is_error() {
        assert!(gunzip(&HELLO_GZ[..12]).is_err());
        assert!(inflate(&[]).is_err());
    }
}
