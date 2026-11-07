//! 回放**消息级**对拍：把一份回放的消息流喂给我们的实现，逐条比"我方本该发出的
//! 下一条消息"与回放里的下一条（`docs/06-verification.md` §17.3 第 3 步）。
//!
//! 本模块是**独立**的（不碰 `game.rs` / `graphwar_game_server.rs` / `_scaffold/protocol.rs` / `_scaffold/api.rs`），
//! 只用它们的公开入口：`game::Match::fire` 产报文、`protocol::emit_*` 定字节、
//! `graphwar_shot_calculator::settle::resolve` 定几何。口径写在 [`Policy`]，语料统计与第一处分歧见
//! `docs/136-replay-diff.md`。
//!
//! 三个子层：
//!
//! 1. [`gz`] —— 纯 std 的 gzip/DEFLATE 解压（`Cargo.toml` 零依赖，不许引 flate2）；
//! 2. [`wire`] —— 保**原始文本**的 JSON 读取（`1.0` 与 `1` 是两种字节，必须区分）；
//! 3. [`cmp`] —— 口径 + 逐字段/逐块比对 + 第一处分歧定位。
//!
//! 语料在 `~/.local/share/godot/app_userdata/Graphwar/replays/`（**只读**，378+ 份）。

// ===========================================================================
// 1. gzip / DEFLATE（纯 std）
// ===========================================================================

/// gzip 解压。支持**多成员**（一个文件里串多个 gzip 流），校验 CRC32 + ISIZE。
///
/// 为什么自己写：本 crate 零依赖（`Cargo.toml` 的 `[dependencies]` 是空的），
/// 而回放是 gzip（`docs/78` §2）。引 `flate2` 会让 `cargo test --offline`
/// 依赖 registry；调外部 `gzip` 会把"能不能跑"绑在机器上。
pub mod gz {
    /// 单个成员解压后的上限（防止畸形流把内存吃光）。真实回放解压后约 160 KB。
    pub const MAX_MEMBER_BYTES: usize = 64 << 20;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GzError(pub String);

    impl std::fmt::Display for GzError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "gzip: {}", self.0)
        }
    }

    fn err<T>(msg: &str) -> Result<T, GzError> {
        Err(GzError(msg.to_string()))
    }

    // ---- CRC32（gzip 尾部的校验值；顺带当作解压器的自检） ----

    fn crc32_table() -> &'static [u32; 256] {
        static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
        TABLE.get_or_init(|| {
            let mut t = [0u32; 256];
            for (i, slot) in t.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *slot = c;
            }
            t
        })
    }

    /// 标准 CRC-32（IEEE，反射版）。
    pub fn crc32(data: &[u8]) -> u32 {
        let t = crc32_table();
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c = t[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }

    // ---- 位读取（DEFLATE 是 LSB-first） ----

    struct Bits<'a> {
        data: &'a [u8],
        pos: usize,
        acc: u64,
        n: u32,
    }

    impl<'a> Bits<'a> {
        fn new(data: &'a [u8], pos: usize) -> Self {
            Bits { data, pos, acc: 0, n: 0 }
        }

        fn fill(&mut self) {
            while self.n <= 56 {
                let b = if self.pos < self.data.len() { self.data[self.pos] } else { 0 };
                self.pos += 1;
                self.acc |= (b as u64) << self.n;
                self.n += 8;
            }
        }

        fn bits(&mut self, need: u32) -> Result<u32, GzError> {
            if need == 0 {
                return Ok(0);
            }
            if self.n < need {
                self.fill();
            }
            if self.n < need {
                return err("DEFLATE 比特流提前结束");
            }
            let v = (self.acc & ((1u64 << need) - 1)) as u32;
            self.acc >>= need;
            self.n -= need;
            Ok(v)
        }

        /// 丢弃到下一个字节边界（`BTYPE=00` 前必须对齐）。
        fn align(&mut self) {
            let drop = self.n % 8;
            self.acc >>= drop;
            self.n -= drop;
        }
    }

    // ---- 规范 Huffman ----

    struct Huff {
        counts: [u16; 16],
        symbols: Vec<u16>,
    }

    impl Huff {
        fn new(lengths: &[u8]) -> Result<Self, GzError> {
            let mut counts = [0u16; 16];
            for &l in lengths {
                counts[l as usize] += 1;
            }
            counts[0] = 0;
            let mut left: i32 = 1;
            for len in 1..16 {
                left <<= 1;
                left -= counts[len] as i32;
                if left < 0 {
                    return err("Huffman 码长过订阅（over-subscribed）");
                }
            }
            let mut offs = [0u16; 16];
            for len in 1..15 {
                offs[len + 1] = offs[len] + counts[len];
            }
            let mut symbols = vec![0u16; lengths.len()];
            for (sym, &l) in lengths.iter().enumerate() {
                if l != 0 {
                    symbols[offs[l as usize] as usize] = sym as u16;
                    offs[l as usize] += 1;
                }
            }
            Ok(Huff { counts, symbols })
        }

        fn decode(&self, br: &mut Bits<'_>) -> Result<u16, GzError> {
            let mut code: i32 = 0;
            let mut first: i32 = 0;
            let mut index: i32 = 0;
            for len in 1..16 {
                code |= br.bits(1)? as i32;
                let count = self.counts[len] as i32;
                if code - first < count {
                    return Ok(self.symbols[(index + (code - first)) as usize]);
                }
                index += count;
                first = (first + count) << 1;
                code <<= 1;
            }
            err("Huffman 解码失败（无匹配码）")
        }
    }

    const LEN_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LEN_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DIST_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DIST_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];
    /// 动态块的码长字母表顺序（RFC 1951 §3.2.7）。
    const CL_ORDER: [usize; 19] =
        [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

    fn inflate(br: &mut Bits<'_>, out: &mut Vec<u8>) -> Result<(), GzError> {
        loop {
            let last = br.bits(1)?;
            let btype = br.bits(2)?;
            match btype {
                0 => {
                    br.align();
                    let len = br.bits(16)? as usize;
                    let nlen = br.bits(16)? as usize;
                    if len != (!nlen & 0xFFFF) {
                        return err("stored 块 LEN/NLEN 不自洽");
                    }
                    for _ in 0..len {
                        out.push(br.bits(8)? as u8);
                    }
                }
                1 => {
                    let mut lit = [0u8; 288];
                    for (i, slot) in lit.iter_mut().enumerate() {
                        *slot = match i {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    let lit = Huff::new(&lit)?;
                    let dist = Huff::new(&[5u8; 30])?;
                    inflate_block(br, out, &lit, &dist)?;
                }
                2 => {
                    let hlit = br.bits(5)? as usize + 257;
                    let hdist = br.bits(5)? as usize + 1;
                    let hclen = br.bits(4)? as usize + 4;
                    let mut cl = [0u8; 19];
                    for &pos in CL_ORDER.iter().take(hclen) {
                        cl[pos] = br.bits(3)? as u8;
                    }
                    let clh = Huff::new(&cl)?;
                    let total = hlit + hdist;
                    let mut lens = vec![0u8; total];
                    let mut i = 0;
                    while i < total {
                        let sym = clh.decode(br)?;
                        match sym {
                            0..=15 => {
                                lens[i] = sym as u8;
                                i += 1;
                            }
                            16 => {
                                if i == 0 {
                                    return err("重复码 16 出现在第一个码长");
                                }
                                let prev = lens[i - 1];
                                let n = 3 + br.bits(2)? as usize;
                                for _ in 0..n {
                                    if i >= total {
                                        return err("重复码 16 越过码长表末尾");
                                    }
                                    lens[i] = prev;
                                    i += 1;
                                }
                            }
                            17 => {
                                let n = 3 + br.bits(3)? as usize;
                                i += n;
                                if i > total {
                                    return err("重复码 17 越过码长表末尾");
                                }
                            }
                            18 => {
                                let n = 11 + br.bits(7)? as usize;
                                i += n;
                                if i > total {
                                    return err("重复码 18 越过码长表末尾");
                                }
                            }
                            _ => return err("非法码长符号"),
                        }
                    }
                    let lit = Huff::new(&lens[..hlit])?;
                    let dist = Huff::new(&lens[hlit..])?;
                    inflate_block(br, out, &lit, &dist)?;
                }
                _ => return err("非法块类型 BTYPE=11"),
            }
            if out.len() > MAX_MEMBER_BYTES {
                return err("解压结果超过上限");
            }
            if last == 1 {
                return Ok(());
            }
        }
    }

    fn inflate_block(
        br: &mut Bits<'_>,
        out: &mut Vec<u8>,
        lit: &Huff,
        dist: &Huff,
    ) -> Result<(), GzError> {
        loop {
            let sym = lit.decode(br)?;
            if sym < 256 {
                out.push(sym as u8);
                continue;
            }
            if sym == 256 {
                return Ok(());
            }
            let li = (sym - 257) as usize;
            if li >= LEN_BASE.len() {
                return err("非法长度码");
            }
            let len = LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
            let dsym = dist.decode(br)? as usize;
            if dsym >= DIST_BASE.len() {
                return err("非法距离码");
            }
            let d = DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
            if d > out.len() {
                return err("回溯距离越过输出起点");
            }
            let start = out.len() - d;
            for k in 0..len {
                let b = out[start + k];
                out.push(b);
            }
        }
    }

    /// 解一个 gzip 成员，返回 `(解压结果, 成员结束后的下一个字节)`。
    fn member(data: &[u8], off: usize) -> Result<(Vec<u8>, usize), GzError> {
        if data.len() < off + 18 {
            return err("头/尾不足");
        }
        if data[off] != 0x1F || data[off + 1] != 0x8B {
            return err("不是 gzip 魔数 1f 8b");
        }
        if data[off + 2] != 8 {
            return err("压缩方法不是 DEFLATE");
        }
        let flg = data[off + 3];
        let mut p = off + 10; // 魔数2 + CM + FLG + MTIME4 + XFL + OS
        if flg & 0x04 != 0 {
            // FEXTRA
            if p + 2 > data.len() {
                return err("FEXTRA 越界");
            }
            let xlen = u16::from_le_bytes([data[p], data[p + 1]]) as usize;
            p += 2 + xlen;
        }
        for flag in [0x08u8, 0x10u8] {
            // FNAME / FCOMMENT：以 NUL 结尾
            if flg & flag != 0 {
                while p < data.len() && data[p] != 0 {
                    p += 1;
                }
                p += 1;
            }
        }
        if flg & 0x02 != 0 {
            p += 2; // FHCRC
        }
        if p > data.len() {
            return err("头部越界");
        }

        let mut br = Bits::new(data, p);
        let mut out = Vec::new();
        inflate(&mut br, &mut out)?;
        br.align();
        // 位缓冲会预读：`pos` 可能已经越过真正的流末尾。真正消费掉的比特数
        // = pos*8 - n；对齐后必然是整字节。
        let consumed = (br.pos * 8 - br.n as usize) / 8;
        let mut q = consumed;
        if q + 8 > data.len() {
            return err("gzip 尾部不完整");
        }
        let crc = u32::from_le_bytes([data[q], data[q + 1], data[q + 2], data[q + 3]]);
        let isize = u32::from_le_bytes([data[q + 4], data[q + 5], data[q + 6], data[q + 7]]) as usize;
        q += 8;
        if crc != crc32(&out) {
            return err("CRC32 不匹配");
        }
        if isize != (out.len() as u32) as usize {
            return err("ISIZE 不匹配");
        }
        Ok((out, q))
    }

    /// 解压任意 gzip 字节串（支持多成员、尾随空白）。
    pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, GzError> {
        let mut out = Vec::new();
        let mut off = 0;
        let mut members = 0;
        loop {
            match member(data, off) {
                Ok((bytes, next)) => {
                    out.extend_from_slice(&bytes);
                    off = next;
                    members += 1;
                }
                Err(e) => {
                    if members > 0 && data[off..].iter().all(|b| b.is_ascii_whitespace()) {
                        break;
                    }
                    return Err(e);
                }
            }
            // 末尾可能还有空白/填充字节
            if data[off..].iter().all(|b| b.is_ascii_whitespace()) {
                break;
            }
        }
        if members == 0 {
            return err("没有任何 gzip 成员");
        }
        if out.len() > MAX_MEMBER_BYTES * 4 {
            return err("累计解压结果过大");
        }
        Ok(out)
    }

    /// 是一个 gzip 字节串吗（回放也可能是未压缩 JSONL，`docs/78`）。
    pub fn is_gzip(data: &[u8]) -> bool {
        data.len() >= 2 && data[0] == 0x1F && data[1] == 0x8B
    }
}

// ===========================================================================
// 2. wire：保留原始文本的 JSON 读取
// ===========================================================================

/// 一份回放里的一行，或我方产出的**一条报文**。
///
/// 关键设计：每个值都留着**原始文本**（`Val::raw`）。`docs/06` §17.4 点名过
/// "`1.0` 写成 `1`"这类偏差，parse 成 f64 再比就抓不到了。
pub mod wire {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Val {
        /// 源文本切片（数字保留原样：`1.0`、`25`、`1e-7` 各不相同）。
        pub raw: String,
        pub kind: Kind,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Kind {
        Int(i64),
        Num(f64),
        Str(String),
        Bool(bool),
        Null,
        Obj(Vec<Field>),
        Arr(Vec<Val>),
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Field {
        pub name: String,
        pub val: Val,
    }

    impl Val {
        pub fn as_i64(&self) -> Option<i64> {
            match self.kind {
                Kind::Int(v) => Some(v),
                Kind::Num(v) if v.fract() == 0.0 && v.abs() < 9e15 => Some(v as i64),
                _ => None,
            }
        }
        pub fn as_f64(&self) -> Option<f64> {
            match self.kind {
                Kind::Int(v) => Some(v as f64),
                Kind::Num(v) => Some(v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match &self.kind {
                Kind::Str(s) => Some(s),
                _ => None,
            }
        }
        /// 重新拼回原始文本（对象的键序就是源顺序）。
        pub fn text(&self) -> String {
            match &self.kind {
                Kind::Obj(fields) => {
                    let body: Vec<String> =
                        fields.iter().map(|f| format!("{}:{}", quote(&f.name), f.val.raw)).collect();
                    format!("{{{}}}", body.join(","))
                }
                Kind::Arr(items) => {
                    let body: Vec<String> = items.iter().map(|v| v.raw.clone()).collect();
                    format!("[{}]", body.join(","))
                }
                _ => self.raw.clone(),
            }
        }
        /// 数字的类型名（报告用）。
        pub fn type_name(&self) -> &'static str {
            match self.kind {
                Kind::Int(_) => "int",
                Kind::Num(_) => "float",
                Kind::Str(_) => "string",
                Kind::Bool(_) => "bool",
                Kind::Null => "null",
                Kind::Obj(_) => "object",
                Kind::Arr(_) => "array",
            }
        }
    }

    /// JSON 字符串字面量（不转义斜杠，与 serde_json 一致）。
    pub fn quote(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for ch in s.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct JsonError(pub String);

    impl std::fmt::Display for JsonError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "json: {}", self.0)
        }
    }

    struct P<'a> {
        s: &'a [u8],
        src: &'a str,
        i: usize,
    }

    impl<'a> P<'a> {
        fn new(src: &'a str) -> Self {
            P { s: src.as_bytes(), src, i: 0 }
        }
        fn peek(&self) -> u8 {
            *self.s.get(self.i).unwrap_or(&0)
        }
        fn skip_ws(&mut self) {
            while matches!(self.peek(), b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            }
        }
        fn eat(&mut self, b: u8) -> Result<(), JsonError> {
            self.skip_ws();
            if self.peek() != b {
                return Err(JsonError(format!(
                    "第 {} 字节处期望 `{}`，实际 `{}`",
                    self.i,
                    b as char,
                    self.peek() as char
                )));
            }
            self.i += 1;
            Ok(())
        }
        fn value(&mut self) -> Result<Val, JsonError> {
            self.skip_ws();
            let start = self.i;
            let v = match self.peek() {
                b'{' => {
                    self.i += 1;
                    let mut fields = Vec::new();
                    self.skip_ws();
                    if self.peek() == b'}' {
                        self.i += 1;
                    } else {
                        loop {
                            self.skip_ws();
                            let name = self.string()?;
                            self.eat(b':')?;
                            let val = self.value()?;
                            fields.push(Field { name, val });
                            self.skip_ws();
                            match self.peek() {
                                b',' => self.i += 1,
                                b'}' => {
                                    self.i += 1;
                                    break;
                                }
                                c => {
                                    return Err(JsonError(format!(
                                        "对象里第 {} 字节处出现 `{}`",
                                        self.i,
                                        c as char
                                    )))
                                }
                            }
                        }
                    }
                    Val { raw: self.src[start..self.i].to_string(), kind: Kind::Obj(fields) }
                }
                b'[' => {
                    self.i += 1;
                    let mut items = Vec::new();
                    self.skip_ws();
                    if self.peek() == b']' {
                        self.i += 1;
                    } else {
                        loop {
                            items.push(self.value()?);
                            self.skip_ws();
                            match self.peek() {
                                b',' => self.i += 1,
                                b']' => {
                                    self.i += 1;
                                    break;
                                }
                                c => {
                                    return Err(JsonError(format!(
                                        "数组里第 {} 字节处出现 `{}`",
                                        self.i,
                                        c as char
                                    )))
                                }
                            }
                        }
                    }
                    Val { raw: self.src[start..self.i].to_string(), kind: Kind::Arr(items) }
                }
                b'"' => {
                    let s = self.string()?;
                    Val { raw: self.src[start..self.i].to_string(), kind: Kind::Str(s) }
                }
                b't' => {
                    self.literal("true")?;
                    Val { raw: "true".into(), kind: Kind::Bool(true) }
                }
                b'f' => {
                    self.literal("false")?;
                    Val { raw: "false".into(), kind: Kind::Bool(false) }
                }
                b'n' => {
                    self.literal("null")?;
                    Val { raw: "null".into(), kind: Kind::Null }
                }
                c if c == b'-' || c.is_ascii_digit() => {
                    let raw = self.number()?;
                    let kind = if raw.contains(['.', 'e', 'E']) {
                        Kind::Num(raw.parse::<f64>().map_err(|_| {
                            JsonError(format!("数字无法解析: {raw}"))
                        })?)
                    } else {
                        match raw.parse::<i64>() {
                            Ok(v) => Kind::Int(v),
                            // 超出 i64 的整数按 f64 处理（回放里没有这么大的值）
                            Err(_) => Kind::Num(raw.parse::<f64>().map_err(|_| {
                                JsonError(format!("数字无法解析: {raw}"))
                            })?),
                        }
                    };
                    Val { raw, kind }
                }
                c => {
                    return Err(JsonError(format!(
                        "第 {} 字节处出现未知值起始符 `{}`",
                        self.i,
                        c as char
                    )))
                }
            };
            Ok(v)
        }
        fn literal(&mut self, lit: &str) -> Result<(), JsonError> {
            if self.src[self.i..].starts_with(lit) {
                self.i += lit.len();
                Ok(())
            } else {
                Err(JsonError(format!("第 {} 字节处期望 `{lit}`", self.i)))
            }
        }
        fn number(&mut self) -> Result<String, JsonError> {
            let start = self.i;
            if self.peek() == b'-' {
                self.i += 1;
            }
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
            if self.peek() == b'.' {
                self.i += 1;
                while self.peek().is_ascii_digit() {
                    self.i += 1;
                }
            }
            if matches!(self.peek(), b'e' | b'E') {
                self.i += 1;
                if matches!(self.peek(), b'+' | b'-') {
                    self.i += 1;
                }
                while self.peek().is_ascii_digit() {
                    self.i += 1;
                }
            }
            if self.i == start {
                return Err(JsonError("空数字".into()));
            }
            Ok(self.src[start..self.i].to_string())
        }
        fn string(&mut self) -> Result<String, JsonError> {
            self.skip_ws();
            if self.peek() != b'"' {
                return Err(JsonError(format!("第 {} 字节处期望字符串", self.i)));
            }
            self.i += 1;
            let mut out = String::new();
            loop {
                let c = self.peek();
                match c {
                    0 => return Err(JsonError("字符串未闭合".into())),
                    b'"' => {
                        self.i += 1;
                        return Ok(out);
                    }
                    b'\\' => {
                        self.i += 1;
                        let e = self.peek();
                        self.i += 1;
                        match e {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'b' => out.push('\u{8}'),
                            b'f' => out.push('\u{c}'),
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'u' => {
                                let hex = self
                                    .src
                                    .get(self.i..self.i + 4)
                                    .ok_or_else(|| JsonError("\\u 越界".into()))?;
                                let v = u32::from_str_radix(hex, 16)
                                    .map_err(|_| JsonError(format!("非法 \\u: {hex}")))?;
                                self.i += 4;
                                // 代理对
                                if (0xD800..0xDC00).contains(&v)
                                    && self.src[self.i..].starts_with("\\u")
                                {
                                    let lo_hex = self.src[self.i + 2..self.i + 6].to_string();
                                    let lo = u32::from_str_radix(&lo_hex, 16)
                                        .map_err(|_| JsonError(format!("非法 \\u: {lo_hex}")))?;
                                    self.i += 6;
                                    let c = 0x10000
                                        + ((v - 0xD800) << 10)
                                        + (lo.wrapping_sub(0xDC00) & 0x3FF);
                                    out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                                } else {
                                    out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                                }
                            }
                            other => {
                                return Err(JsonError(format!("非法转义 \\{}", other as char)))
                            }
                        }
                    }
                    _ => {
                        // 多字节 UTF-8：按字符推进
                        let ch = self.src[self.i..].chars().next().unwrap();
                        out.push(ch);
                        self.i += ch.len_utf8();
                    }
                }
            }
        }
    }

    /// 一条规范化后的报文：`{"Kind":{...}}`，外加回放里的信封 tick。
    #[derive(Debug, Clone, PartialEq)]
    pub struct Msg {
        /// 回放信封里的 tick（我方产出的报文没有信封，用 `-1`）。
        pub tick: i64,
        pub kind: String,
        pub body: Vec<Field>,
        /// 内层对象的**原始文本**。
        pub body_raw: String,
    }

    impl Msg {
        /// `{"Kind":{...}}` —— 我方 emitter 产出的形状。
        pub fn parse_wire(wire: &str) -> Result<Msg, JsonError> {
            let wire = wire.trim();
            let v = P::new(wire).value()?;
            let Kind::Obj(fields) = v.kind else {
                return Err(JsonError("顶层不是对象".into()));
            };
            if fields.len() != 1 {
                return Err(JsonError(format!("顶层对象有 {} 个键，应为 1", fields.len())));
            }
            let f = &fields[0];
            let body_raw = match &f.val.kind {
                Kind::Obj(_) => f.val.raw.clone(),
                _ => f.val.raw.clone(),
            };
            let body = match &f.val.kind {
                Kind::Obj(inner) => inner.clone(),
                _ => Vec::new(),
            };
            Ok(Msg { tick: -1, kind: f.name.clone(), body, body_raw })
        }

        /// 回放的一行：`{"event":{"Kind":{...}},"tick":N}`。
        ///
        /// 键序不假设：`event` / `tick` 谁先都行（实测都是 `event` 先）。
        pub fn parse_line(line: &str) -> Result<Msg, JsonError> {
            let v = P::new(line.trim()).value()?;
            let Kind::Obj(fields) = v.kind else {
                return Err(JsonError("行顶层不是对象".into()));
            };
            let mut event: Option<Val> = None;
            let mut tick: i64 = 0;
            for f in fields {
                match f.name.as_str() {
                    "event" => event = Some(f.val),
                    "tick" => tick = f.val.as_i64().unwrap_or(0),
                    other => {
                        return Err(JsonError(format!("行里出现未知键 `{other}`")));
                    }
                }
            }
            let event = event.ok_or_else(|| JsonError("行里没有 `event`".into()))?;
            let Kind::Obj(mut evs) = event.kind else {
                return Err(JsonError("`event` 不是对象".into()));
            };
            if evs.len() != 1 {
                return Err(JsonError(format!("`event` 有 {} 个变体，应为 1", evs.len())));
            }
            let f = evs.remove(0);
            let body = match &f.val.kind {
                Kind::Obj(inner) => inner.clone(),
                _ => Vec::new(),
            };
            Ok(Msg { tick, kind: f.name, body, body_raw: f.val.raw })
        }

        /// 规范化文本：`{"Kind":<body_raw>}`。两侧用同一条路径生成，
        /// 所以**逐字节比**就是比这条串。
        pub fn canonical(&self) -> String {
            format!("{{\"{}\":{}}}", self.kind, self.body_raw)
        }

        pub fn get(&self, name: &str) -> Option<&Val> {
            self.body.iter().find(|f| f.name == name).map(|f| &f.val)
        }

        pub fn get_i64(&self, name: &str) -> Option<i64> {
            self.get(name).and_then(|v| v.as_i64())
        }

        pub fn get_f64(&self, name: &str) -> Option<f64> {
            self.get(name).and_then(|v| v.as_f64())
        }

        pub fn get_str(&self, name: &str) -> Option<String> {
            self.get(name).and_then(|v| v.as_str().map(|s| s.to_string()))
        }

        /// 归一化后的"块签名"：同 kind 的消息按内容排序后比较，
        /// 用来实现"批内顺序是 A 类"的口径（`docs/113`）。
        pub fn signature(&self) -> String {
            self.canonical()
        }
    }
}

// ===========================================================================
// 3. 回放文件读取
// ===========================================================================

pub use wire::{Field, Kind, Msg, Val};

/// 一份回放的内容：有序报文流 + 元信息。
#[derive(Debug, Clone, Default)]
pub struct Replay {
    pub msgs: Vec<Msg>,
    /// 解压后字节数
    pub plain_bytes: usize,
    /// 是不是 gzip（否则是裸 JSONL）
    pub gzipped: bool,
}

impl Replay {
    /// 从磁盘读一份回放（**只读**，不解压到磁盘）。
    pub fn load(path: &std::path::Path) -> Result<Replay, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Replay::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, String> {
        let gzipped = gz::is_gzip(bytes);
        let plain: Vec<u8> = if gzipped {
            gz::gunzip(bytes).map_err(|e| e.to_string())?
        } else {
            bytes.to_vec()
        };
        let text = String::from_utf8(plain.clone())
            .map_err(|e| format!("回放不是 UTF-8: {e}"))?;
        let mut msgs = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let m = Msg::parse_line(line).map_err(|e| format!("第 {} 行: {e}", n + 1))?;
            msgs.push(m);
        }
        Ok(Replay { msgs, plain_bytes: plain.len(), gzipped })
    }

    /// 语料里出现过的消息种类（按出现次数降序）。
    pub fn kind_histogram(&self) -> std::collections::BTreeMap<String, usize> {
        let mut h = std::collections::BTreeMap::new();
        for m in &self.msgs {
            *h.entry(m.kind.clone()).or_insert(0) += 1;
        }
        h
    }
}

// ===========================================================================
// 4. 口径：比什么、不比什么、为什么
// ===========================================================================

/// 比对口径。**这是本模块最重要的产出**，每一条都在 `docs/136` §1 有理由。
///
/// 三条轴（`docs/06` §17.1）：状态 / 消息字节及其批内顺序 / 几何。
/// 但 §17.2 已经按"结果是否由输入决定"排除了三类，所以口径必须先排除它们，
/// 否则永远不可能全绿、也就失去意义（这一点写进 `docs/136` §1.3）。
#[derive(Debug, Clone)]
pub struct Policy {
    /// 值不比（B 类：墙钟）。字段名匹配即跳过**值**比较，位置与类型仍比。
    pub tick_fields: &'static [&'static str],
    /// 绝对值不比、只比"同批内是否连续递增"（会话级计数器，回放里没有它的初值）。
    pub counter_fields: &'static [&'static str],
    /// 我方**能产出**的消息种类（kind）。
    pub produced_kinds: &'static [&'static str],
    /// 我方**能产出**的 `EffectInfo.effect_type`（其余 effect_type 记为"未实现"）。
    pub produced_effects: &'static [&'static str],
    /// 我方产出但**回放里永远不记**的消息（回放的记录域，不是我们的缺口）。
    pub unobserved_kinds: &'static [&'static str],
    /// 同一个 tick 里连续的完全重复报文折叠成一个（回放记录器会重复记，见 `docs/136` §2.4）。
    pub collapse_duplicates: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            // `docs/06` §17.2 B 类：墙钟。证据：`docs/113` §5.2 钉住两份熵后
            // 只有 tick 字段不同（`end_of_turn_tick` 3688 vs 3687 等）。
            tick_fields: &[
                "tick",
                "active_tick",
                "start_tick",
                "end_of_turn_tick",
                "current_tick",
            ],
            // `effect_id` 是**会话级**自增计数器（抓包里从 236 起，
            // 回放里从 6xx 起）——回放不给它的初值映射，绝对值不可判定；
            // 但"同一批内每来一条 effect 就 +1"是可判定的结构。
            counter_fields: &["effect_id"],
            produced_kinds: &[
                "TurnInfo",
                "EndOfTurnInfo",
                "TurnCountInfo",
                "GameStateInfo",
                "FunctionInfo",
                "FunctionPoints",
                "FunctionActive",
                "EffectInfo",
                "LifeQueuedDeath",
                "QueueDestroyObstacle",
                "DestroyObstacle",
            ],
            produced_effects: &["ExplosionEffect", "DeathEffect", "SparkEffect", "AshEffect"],
            unobserved_kinds: &["LifeQueuedDeath", "QueueDestroyObstacle"],
            collapse_duplicates: true,
        }
    }
}

impl Policy {
    /// 回放里的这条消息，我方**根本不产**（不是分歧，是我们的缺口）。
    pub fn is_unproduced(&self, m: &Msg) -> bool {
        if !self.produced_kinds.contains(&m.kind.as_str()) {
            return true;
        }
        if m.kind == "EffectInfo" {
            let et = m.get_str("effect_type").unwrap_or_default();
            return !self.produced_effects.contains(&et.as_str());
        }
        false
    }

    /// 我方产出、但回放永远不记录的消息（比较时直接丢）。
    pub fn is_unobserved(&self, m: &Msg) -> bool {
        self.unobserved_kinds.contains(&m.kind.as_str())
    }
}

/// 一处字段/结构差异的**成因分类**（决定它是不是真失败）。
#[derive(Debug, Clone, PartialEq)]
pub enum DiffClass {
    /// 数值语义相同、**字节不同**（`25` vs `25.0`、`1e-7` vs `0.0000001`）
    /// —— `docs/06` §17.4 点名的那类偏差。
    NumericFormat,
    /// 数值不同（附 `ulp` 与绝对差）。
    NumericValue { ulp: i64, delta: f64 },
    /// 字符串/枚举值不同。
    Value,
    /// 字段缺失或多余。
    Presence,
    /// 字段顺序不同（`docs/06` §17.1 的"字段顺序"，serde 结构体顺序）。
    FieldOrder,
    /// 批内消息**顺序**不同，但内容（多重集）相同 —— `docs/113` 的 A 类。
    MessageOrder,
    /// 批内消息种类序列不同（少一类/多一类/顺序换了位置）。
    KindSequence,
    /// 同批内计数器不连续（`effect_id`）。
    CounterGap,
    /// 超过 `ulp` 阈值但仍是"同一个点"的差（几何量用，报告里分开统计）。
    NumericValueFar,
}

/// 一条字段级差异。
#[derive(Debug, Clone)]
pub struct FieldDiff {
    /// 消息种类
    pub kind: String,
    /// 路径，如 `FunctionPoints.end_x`
    pub path: String,
    /// 我方预测值（原文）
    pub ours: String,
    /// 回放观测值（原文）
    pub observed: String,
    pub class: DiffClass,
}

impl std::fmt::Display for FieldDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: ours={} observed={} ({:?})",
            self.path, self.ours, self.observed, self.class
        )
    }
}

// ===========================================================================
// 5. 结算批（batch）切分
// ===========================================================================

/// 一次"结算批"：`GameStateInfo(CalculatingResults)` … `GameStateInfo(ExecutingResults)`。
///
/// 为什么以批为单位而不是以"一发"为单位：实测（`docs/136` §2.2）
/// 同时回合制一次结算会发 **N 个 `FunctionInfo` + N 个 `FunctionPoints`**，
/// 全部在同一个 `CalcalatingResults`…`ExecutingResults` 窗口里，
/// 且**按消息类型分组**（不是按射手分组）。
pub struct Batch {
    /// 窗口首条（含）在 `Replay::msgs` 里的下标
    pub start: usize,
    /// 窗口末条（`ExecutingResults`，含）
    pub end: usize,
    /// 延迟段：`end+1` 直到下一批开始（`LifeInfo`/`DestroyObstacle` 落在这里）
    pub tail_end: usize,
    /// 批内 `FunctionPoints` 的顺序（= 我方的开火顺序）
    pub shots: Vec<ShotInput>,
}

/// 一次开火的**输入**（全部来自回放，是"已知量"而不是预测目标）。
#[derive(Debug, Clone)]
pub struct ShotInput {
    pub owner: i64,
    pub x0: f64,
    pub y0: f64,
    /// true = 向 x 减小方向（`docs/105`：Team1 在 x<0 向右，Team2 在 x>0 向左）
    pub flip: bool,
    /// 开火用的函数原文（`FunctionInfo.function`，可能是空串）
    pub text: String,
}

/// 从回放里切出所有结算批。
pub fn split_batches(r: &Replay) -> Vec<Batch> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < r.msgs.len() {
        let is_calc = r.msgs[i].kind == "GameStateInfo"
            && r.msgs[i].get_str("game_state").as_deref() == Some("CalculatingResults");
        if !is_calc {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i;
        let mut end = None;
        while j < r.msgs.len() {
            if r.msgs[j].kind == "GameStateInfo"
                && r.msgs[j].get_str("game_state").as_deref() == Some("ExecutingResults")
            {
                end = Some(j);
                break;
            }
            // 防御：下一批开始了还没见到 Executing（畸形回放）
            if j > start
                && r.msgs[j].kind == "GameStateInfo"
                && r.msgs[j].get_str("game_state").as_deref() == Some("CalculatingResults")
            {
                break;
            }
            j += 1;
        }
        let Some(end) = end else {
            i += 1;
            continue;
        };
        // 延迟段到下一批开始为止
        let mut tail_end = end + 1;
        while tail_end < r.msgs.len() {
            let m = &r.msgs[tail_end];
            if m.kind == "GameStateInfo"
                && m.get_str("game_state").as_deref() == Some("CalculatingResults")
            {
                break;
            }
            tail_end += 1;
        }

        // 批内每一发：从 `FunctionPoints` 拿 owner / 起点；从同批的修正 `FunctionInfo`
        // 拿函数原文；朝向从 `TeamInfo` 推。
        let mut shots = Vec::new();
        for m in &r.msgs[start..=end] {
            if m.kind != "FunctionPoints" {
                continue;
            }
            let owner = m.get_i64("owner_id").unwrap_or(0);
            let text = r.msgs[start..=end]
                .iter()
                .filter(|x| x.kind == "FunctionInfo" && x.get_i64("owner_id") == Some(owner))
                .filter_map(|x| x.get_str("function"))
                .next()
                .or_else(|| last_function_before(r, start, owner))
                .unwrap_or_default();
            let team = last_team_before(r, start, owner);
            shots.push(ShotInput {
                owner,
                x0: m.get_f64("start_x").unwrap_or(0.0),
                y0: m.get_f64("start_y").unwrap_or(0.0),
                flip: team.as_deref() == Some("Team2"),
                text,
            });
        }
        out.push(Batch { start, end, tail_end, shots });
        i = end + 1;
    }
    out
}

fn last_function_before(r: &Replay, upto: usize, owner: i64) -> Option<String> {
    let mut found = None;
    for m in &r.msgs[..upto] {
        if m.kind == "FunctionInfo" && m.get_i64("owner_id") == Some(owner) {
            found = m.get_str("function");
        }
    }
    found
}

fn last_team_before(r: &Replay, upto: usize, entity: i64) -> Option<String> {
    let mut found = None;
    for m in &r.msgs[..upto] {
        if m.kind == "TeamInfo" && m.get_i64("entity_id") == Some(entity) {
            found = m.get_str("team");
        }
    }
    found
}

// ===========================================================================
// 6. 我方预测：用 `game::Match::fire` 产报文
// ===========================================================================

use crate::game::{Match, Submission};
use crate::server::RoomState;
use crate::graphwar_shot_calculator::settle::Target;

/// 从回放里攒出来的**输入态**（几何与身份）；不含 tick。
pub struct Inputs {
    pub room: RoomState,
    /// 关卡障碍（`ClearObstacles` 清空 + `AddObstacle` 追加），按回放顺序
    pub obstacles: Vec<(f64, f64, f64)>,
    /// 每个士兵的坐标（`PosInfo`；回放里恰好等于 `FunctionPoints.start`，实测 1290/1290）
    pub pos: std::collections::BTreeMap<i64, (f64, f64)>,
    pub team: std::collections::BTreeMap<i64, String>,
    pub alive: std::collections::BTreeMap<i64, bool>,
    /// 靶子（`TargetInfo`，tag 23）⇒ 出生登记 `SparkEffect`
    pub targets: Vec<i64>,
    /// 皮肤名（`SkinGraphicsInfo`，tag 51，`entity_id -> graphics_str`）⇒ `bot_` 前缀改成 `AshEffect`
    pub skins: Vec<(i64, String)>,
}

impl Inputs {
    pub fn scan(r: &Replay, upto: usize) -> Inputs {
        let mut room = RoomState {
            room_name: "Graphwar Game".into(),
            game_state: "Setup".into(),
            axis_mode: "EveryUnit".into(),
            function_mode: "NormalFunction".into(),
            turn_mode: "SequentialTurns".into(),
            time_mode: "Timer1m".into(),
        };
        let mut obstacles = Vec::new();
        let mut pos = std::collections::BTreeMap::new();
        let mut team = std::collections::BTreeMap::new();
        let mut alive = std::collections::BTreeMap::new();
        let mut targets: Vec<i64> = Vec::new();
        let mut skins: Vec<(i64, String)> = Vec::new();
        for m in &r.msgs[..upto.min(r.msgs.len())] {
            match m.kind.as_str() {
                "AxisModeInfo" => room.axis_mode = m.get_str("axis_mode").unwrap_or(room.axis_mode),
                "FunctionModeInfo" => {
                    room.function_mode = m.get_str("function_mode").unwrap_or(room.function_mode)
                }
                "TurnModeInfo" => room.turn_mode = m.get_str("turn_mode").unwrap_or(room.turn_mode),
                "TimeModeInfo" => room.time_mode = m.get_str("time_mode").unwrap_or(room.time_mode),
                "GameNameInfo" => room.room_name = m.get_str("game_name").unwrap_or(room.room_name),
                "ClearObstacles" => obstacles.clear(),
                "AddObstacle" => obstacles.push((
                    m.get_f64("pos_x").unwrap_or(0.0),
                    m.get_f64("pos_y").unwrap_or(0.0),
                    m.get_f64("radius").unwrap_or(0.0),
                )),
                "PosInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        pos.insert(id, (m.get_f64("pos_x").unwrap_or(0.0), m.get_f64("pos_y").unwrap_or(0.0)));
                    }
                }
                "TeamInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        team.insert(id, m.get_str("team").unwrap_or_default());
                    }
                }
                "TargetInfo" => {
                    if let Some(id) = m.get_i64("target_id") {
                        targets.push(id);
                    }
                }
                "SkinGraphicsInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        skins.push((id, m.get_str("graphics_str").unwrap_or_default()));
                    }
                }
                "LifeInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        alive.insert(id, m.get("alive").and_then(|v| match v.kind {
                            Kind::Bool(b) => Some(b),
                            _ => None,
                        }).unwrap_or(true));
                    }
                }
                _ => {}
            }
        }
        Inputs { room, obstacles, pos, team, alive, targets, skins }
    }
}

/// 我方预测器：**一个回放一个 `Match`**，状态自带（含我们自己在 `fire` 里挖的洞）。
///
/// 为什么状态要自带而不是每批从回放重建：`Match::mask` 是私有的、且"挖洞"只发生在
/// `fire` 里（`DESTROY_DELAY_TICKS` 之后才发 `DestroyObstacle`）。自带状态才是
/// "把输入流喂进我们的实现"（`docs/06` §17.3 第 1 步），重建则等于把答案抄进题目。
pub struct Predictor {
    model: Match,
    pub policy: Policy,
    /// 回放里的 `FunctionModeInfo` 时刻表：`(报文下标, 模式)`，按出现顺序。
    ///
    /// 为什么不是"建模型时读一次"：**模式是可以在局中改的**（`FunctionModeInfo` 是设置广播，
    /// 语料里 125 份回放在同一文件里出现过两种取值）。二进制里写它的是 46 号事件的处理器
    /// （`0x3e2b1b`），读它的是**每次开火**（`0x4717b8`）⇒ 输入流喂到哪一批，
    /// 开火就该用那一刻的模式（`docs/06` §33.3）。
    mode_schedule: Vec<(usize, crate::graphwar_mode_manager::FunctionMode)>,
    /// 已经喂进模型的表项游标。
    mode_cursor: usize,
}

/// 一批的预测结果。
pub struct BatchPrediction {
    /// 我方产出的全部报文（已按 `FunctionPoints` 顺序拼接）
    pub msgs: Vec<Msg>,
    /// 我方 emitter 产出的**不是合法 JSON** 的报文原文。
    ///
    /// 这不是"比对器坏了"，而是被测实现的问题：`protocol::emit_function_info`
    /// 把函数原文直接塞进字符串字面量、不转义（`protocol::json_str` 存在却没被调用）。
    /// 回放里真有玩家输入 `sqrt(\y)`（反斜杠），于是它发出的是非法 JSON。
    pub invalid: Vec<String>,
    /// 我方判定的死者（`LifeQueuedDeath.entity_id`）
    pub killed: Vec<i64>,
    /// 我方判定的阻挡点（`QueueDestroyObstacle` 的 `(x,y)`）
    pub blocked_at: Option<(f64, f64)>,
}

impl Predictor {
    /// 用回放**第一批之前**的输入态建模型。
    pub fn new(r: &Replay, first_batch_start: usize, policy: Policy) -> Result<Self, String> {
        let inputs = Inputs::scan(r, first_batch_start);
        let soldiers: Vec<Target> = inputs
            .pos
            .iter()
            .filter(|(id, _)| inputs.alive.get(id).copied().unwrap_or(true))
            .map(|(id, (x, y))| Target { id: *id, x: *x, y: *y, radius: 0.5 })
            .collect();
        if soldiers.is_empty() {
            return Err("回放里没有 PosInfo，建不出局面".into());
        }
        let base_tick = r
            .msgs
            .get(first_batch_start)
            .map(|m| m.tick)
            .unwrap_or(1);
        let mut model = Match::new(
            inputs.room.clone(),
            inputs.obstacles.clone(),
            soldiers,
            (base_tick, 0),
        );
        // ── 把"出生登记"喂进去（2026-11-07）：死亡特效照抄登记表，查不到就一条都不发。
        // 不喂这两样，回放对拍器里每个实体都走"出生时还没有皮肤"⇒ 全 DeathEffect，
        // 与旧实现逐字节相同，问题问不出来。见 `docs/16-client.md` §33.5。
        // 靶子是 **Team2**（`docs/150`：打靶关的胜负 = Team2 全灭），u8 编码 1。
        for id in inputs.targets.iter() {
            model.add_target(*id, 1);
        }
        for (id, g) in inputs.skins.iter() {
            model.note_skin_graphics(*id, g);
        }
        // `FunctionModeInfo` 的**时刻表**（全文件扫一遍）。模型建好时用的是
        // "第一批之前"的那个值（`Inputs::scan` 已经把它放进 `room.function_mode`），
        // 之后的每一条按报文下标在 [`Predictor::predict`] 里补齐。
        let mut mode_schedule = Vec::new();
        for (i, m) in r.msgs.iter().enumerate() {
            if m.kind == "FunctionModeInfo" {
                if let Some(s) = m.get_str("function_mode") {
                    let fm = crate::game::function_mode_of(&s);
                    // 同值连续出现不必重复入表（只是省几条比较）
                    if mode_schedule.last().map(|e: &(usize, _)| e.1) != Some(fm) {
                        mode_schedule.push((i, fm));
                    }
                }
            }
        }
        Ok(Predictor { model, policy, mode_schedule, mode_cursor: 0 })
    }

    /// 把模型的状态对齐到"第 `upto` 条报文之前"：目前只有函数模式这一项会随报文变。
    fn sync_mode(&mut self, upto: usize) {
        while let Some((idx, fm)) = self.mode_schedule.get(self.mode_cursor).copied() {
            if idx >= upto {
                break;
            }
            self.model.set_function_mode(fm);
            self.mode_cursor += 1;
        }
    }

    /// 预测一批：**批内 ≥2 发**时一次 `Match::fire_batch`，单发批仍走 `Match::fire`。
    ///
    /// 判据是**批内的发数**（`batch.shots.len()`），**不是**"整份回放里有没有多发射击"：
    /// 一个结算批就是一次收集 →**一次**结算 →**一次**广播（`docs/141` §3），窗口里
    /// `GameStateInfo` 只有 3 条（`CalculatingResults` / `SynchronizingResults` /
    /// `ExecutingResults`），前缀 `FunctionInfo`/`FunctionPoints` 按类型连排，
    /// 后缀 `FunctionActive + EffectInfo(爆炸)` 每发一对。
    ///
    /// 旧实现**逐发**调 `fire`，等于把同一个批拆成 N 个"单发批"：多发批凭空多出
    /// `3×(N−1)` 条 `GameStateInfo`，前后缀的分组/交替也不成形。`tools/bit_exact_attrib.py`
    /// 里 `S`（多发批结构）与 `SD` 两桶就是这么来的（2026-11-08 前：126 + 4 批）——
    /// 那是**驱动口径**的产物，不是内核错：内核只有 `fire_batch` 一条结算路径，
    /// `Match::fire` 本身就是它的单元素特例（`src/game.rs`）。
    ///
    /// 单发批的行为**逐位不变**（`fire(f)` ≡ `fire_batch(&[f])`），所以这条分支只影响
    /// `shots ≥ 2` 的批。`--drive batch` 走的是 `Match` 的另一套接线，不受影响。
    pub fn predict(&mut self, batch: &Batch, obs_start_tick: i64) -> BatchPrediction {
        // 本批之前到达的 `FunctionModeInfo` 先喂进去（模式是输入、不是常量）
        self.sync_mode(batch.start);
        // 把 tick 基准挪到本批，令 `now_tick ≈ 回放信封 tick`（tick 值本身不比）
        self.model.tick_base = (obs_start_tick, 0);
        // ★ 口径开关：按**批内发数**选驱动。`>= 2` ⇒ 一次 `fire_batch`（一个批一次广播）。
        let wire: Vec<String> = match batch.shots.len() {
            0 => Vec::new(),
            1 => {
                let s = &batch.shots[0];
                self.model.fire(s.owner, &s.text, s.x0, s.y0, s.flip, 0)
            }
            _ => {
                let subs: Vec<Submission<'_>> = batch
                    .shots
                    .iter()
                    .map(|s| Submission {
                        owner_id: s.owner,
                        function_text: &s.text,
                        x0: s.x0,
                        y0: s.y0,
                        flip: s.flip,
                    })
                    .collect();
                self.model.fire_batch(&subs, 0)
            }
        };
        let mut msgs = Vec::new();
        let mut invalid = Vec::new();
        let mut killed = Vec::new();
        let mut blocked_at = None;
        for w in &wire {
            match Msg::parse_wire(w) {
                Ok(m) => {
                    match m.kind.as_str() {
                        "LifeQueuedDeath" => {
                            if let Some(id) = m.get_i64("entity_id") {
                                killed.push(id);
                            }
                        }
                        "QueueDestroyObstacle" => {
                            blocked_at = Some((
                                m.get_f64("pos_x").unwrap_or(f64::NAN),
                                m.get_f64("pos_y").unwrap_or(f64::NAN),
                            ));
                        }
                        _ => {}
                    }
                    msgs.push(m);
                }
                Err(_) => invalid.push(w.clone()),
            }
        }
        BatchPrediction { msgs, invalid, killed, blocked_at }
    }
}

// ===========================================================================
// 7. 逐条比对
// ===========================================================================

/// 一批的比对结论。
#[derive(Debug, Clone, Default)]
pub struct BatchVerdict {
    /// 回放这一批里有几发
    pub shots: usize,
    /// 内容命中：**投影后、块内按多重集**逐字节相等（tick / 计数器已排除）
    pub content_ok: bool,
    /// 顺序命中：块内**有序**序列也相等
    pub order_ok: bool,
    /// 块种类序列是否相同
    pub kind_seq_ok: bool,
    /// 我方 effect_id / 回放 effect_id 是否都严格递增（结构不变量）
    pub counter_ok: bool,
    /// 回放里有、我方不产 ⇒ 排除并计数（不是分歧）
    pub unproduced: usize,
    /// 我方产出、回放不记 ⇒ 排除并计数
    pub unobserved: usize,
    /// 回放里连续完全重复而被折叠的条数
    pub collapsed: usize,
    /// 字段级差异（最多 `max_diffs` 条）
    pub diffs: Vec<FieldDiff>,
    /// 块内匹配不上的消息（`ours` / `observed` 摘要）
    pub missing: Vec<String>,
    pub extra: Vec<String>,
    /// 首个分歧（`diffs` 的第一条，或结构分歧）
    pub first: Option<FieldDiff>,
    /// effect_id 偏移（回放首个 - 我方首个）
    pub effect_offset: Option<i64>,
}

/// 排除字段归一化：把不比值的字段替换成 `#`。
fn normalize(m: &Msg, policy: &Policy) -> String {
    let mut parts = Vec::with_capacity(m.body.len());
    for f in &m.body {
        let skip = policy.tick_fields.contains(&f.name.as_str())
            || policy.counter_fields.contains(&f.name.as_str());
        if skip {
            parts.push(format!("\"{}\":#", f.name));
        } else {
            parts.push(format!("\"{}\":{}", f.name, f.val.text()));
        }
    }
    format!("{{\"{}\":{{{}}}}}", m.kind, parts.join(","))
}

fn ulp_diff(a: f64, b: f64) -> i64 {
    fn ord(x: f64) -> i64 {
        let bits = x.to_bits() as i64;
        if bits < 0 {
            i64::MIN - bits
        } else {
            bits
        }
    }
    ord(a).saturating_sub(ord(b)).saturating_abs()
}

/// 两条同 kind 消息的字段级比对（含字段顺序/存在性）。
pub fn compare_msg(our: &Msg, obs: &Msg, policy: &Policy) -> Vec<FieldDiff> {
    let mut out = Vec::new();
    let ours_names: Vec<&str> = our.body.iter().map(|f| f.name.as_str()).collect();
    let obs_names: Vec<&str> = obs.body.iter().map(|f| f.name.as_str()).collect();
    if ours_names != obs_names {
        out.push(FieldDiff {
            kind: obs.kind.clone(),
            path: format!("{}.<fields>", obs.kind),
            ours: ours_names.join(","),
            observed: obs_names.join(","),
            class: if ours_names.len() == obs_names.len() {
                DiffClass::FieldOrder
            } else {
                DiffClass::Presence
            },
        });
    }
    for of in &our.body {
        let Some(ob) = obs.body.iter().find(|f| f.name == of.name) else {
            continue; // 上面已经报过 Presence
        };
        if policy.tick_fields.contains(&of.name.as_str())
            || policy.counter_fields.contains(&of.name.as_str())
        {
            continue; // 不比值（口径已声明）
        }
        if of.val.raw == ob.val.raw {
            continue;
        }
        let class = match (of.val.as_f64(), ob.val.as_f64()) {
            (Some(a), Some(b)) => {
                if a == b {
                    DiffClass::NumericFormat
                } else {
                    let delta = (a - b).abs();
                    let ulp = ulp_diff(a, b);
                    if delta > 1e-3 {
                        DiffClass::NumericValueFar
                    } else {
                        DiffClass::NumericValue { ulp, delta }
                    }
                }
            }
            _ => DiffClass::Value,
        };
        out.push(FieldDiff {
            kind: obs.kind.clone(),
            path: format!("{}.{}", obs.kind, of.name),
            ours: of.val.text(),
            observed: ob.val.text(),
            class,
        });
    }
    out
}

fn kind_of(m: &Msg) -> String {
    if m.kind == "EffectInfo" {
        format!("EffectInfo/{}", m.get_str("effect_type").unwrap_or_default())
    } else {
        m.kind.clone()
    }
}

fn brief(m: &Msg) -> String {
    let body: Vec<String> = m
        .body
        .iter()
        .map(|f| format!("{}={}", f.name, f.val.text()))
        .collect();
    format!("{}({})", m.kind, body.join(","))
}

/// 一批的完整比对。`ours` 是我方预测，`observed` 是回放里的那一批。
pub fn compare_batch(ours: &[Msg], observed: &[Msg], policy: &Policy, max_diffs: usize) -> BatchVerdict {
    let mut v = BatchVerdict { content_ok: true, order_ok: true, kind_seq_ok: true, counter_ok: true, ..Default::default() };

    // ---- 投影 1：回放里有、我方不产 ⇒ 排除 ----
    let mut obs: Vec<&Msg> = Vec::new();
    for m in observed {
        if policy.is_unproduced(m) {
            v.unproduced += 1;
        } else {
            obs.push(m);
        }
    }
    // ---- 投影 2：我方产出、回放不记 ⇒ 排除 ----
    let mut own: Vec<&Msg> = Vec::new();
    for m in ours {
        if policy.is_unobserved(m) {
            v.unobserved += 1;
        } else {
            own.push(m);
        }
    }
    // ---- 折叠同一 tick 里连续完全相同的报文 ----
    if policy.collapse_duplicates {
        let mut deduped: Vec<&Msg> = Vec::new();
        for m in obs {
            if let Some(prev) = deduped.last() {
                if prev.canonical() == m.canonical() {
                    v.collapsed += 1;
                    continue;
                }
            }
            deduped.push(m);
        }
        obs = deduped;
    }

    // ---- 计数器结构不变量 ----
    let counter_seq = |ms: &[&Msg]| -> Vec<i64> {
        ms.iter().filter_map(|m| m.get_i64("effect_id")).collect()
    };
    let cs_obs = counter_seq(&obs);
    let cs_our = counter_seq(&own);
    let strict_inc = |s: &[i64]| s.windows(2).all(|w| w[1] > w[0]);
    if !strict_inc(&cs_obs) || !strict_inc(&cs_our) {
        v.counter_ok = false;
    }
    if let (Some(a), Some(b)) = (cs_obs.first(), cs_our.first()) {
        v.effect_offset = Some(a - b);
    }

    // ---- 分块（连续同 kind 为一块） ----
    fn blocks<'a>(ms: &[&'a Msg]) -> Vec<Vec<&'a Msg>> {
        let mut out: Vec<Vec<&Msg>> = Vec::new();
        for m in ms {
            let same = out
                .last()
                .map(|b| kind_of(b[0]) == kind_of(m))
                .unwrap_or(false);
            if same {
                out.last_mut().unwrap().push(m);
            } else {
                out.push(vec![*m]);
            }
        }
        out
    }
    let bo = blocks(&own);
    let bx = blocks(&obs);
    let bkind = |b: &Vec<&Msg>| kind_of(b[0]);

    // ---- 种类序列 ----
    let kinds_ours: Vec<String> = bo.iter().map(bkind).collect();
    let kinds_obs: Vec<String> = bx.iter().map(bkind).collect();
    if kinds_ours != kinds_obs {
        v.kind_seq_ok = false;
        v.content_ok = false;
        v.order_ok = false;
        let at = kinds_ours
            .iter()
            .zip(kinds_obs.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(kinds_ours.len().min(kinds_obs.len()));
        let first = FieldDiff {
            kind: kinds_obs.get(at).cloned().unwrap_or_else(|| "<末尾>".into()),
            path: format!("<批内第 {at} 块的种类>"),
            ours: kinds_ours.join(" | "),
            observed: kinds_obs.join(" | "),
            class: DiffClass::KindSequence,
        };
        v.first = Some(first.clone());
        v.diffs.push(first);
        // 仍然做块内字段比对（只对同 kind 的块），把"值对不对"与"结构对不对"分开
        for (a, b) in bo.iter().zip(bx.iter()) {
            if bkind(a) != bkind(b) {
                continue;
            }
            collect_block_diffs(a, b, policy, max_diffs, &mut v);
        }
        if v.diffs.len() > max_diffs {
            v.diffs.truncate(max_diffs);
        }
        if v.first.is_none() {
            v.first = v.diffs.first().cloned();
        }
        return v;
    }

    // ---- 块内比对 ----
    for (a, b) in bo.iter().zip(bx.iter()) {
        collect_block_diffs(a, b, policy, max_diffs, &mut v);
        if v.diffs.len() >= max_diffs {
            break;
        }
    }
    if v.diffs.len() > max_diffs {
        v.diffs.truncate(max_diffs);
    }
    v.first = v.diffs.first().cloned();
    v
}

/// 一块（同 kind 的连续消息）内部比对：先多重集，再顺序。
fn collect_block_diffs(
    own: &[&Msg],
    obs: &[&Msg],
    policy: &Policy,
    max_diffs: usize,
    v: &mut BatchVerdict,
) {
    let norm_own: Vec<String> = own.iter().map(|m| normalize(m, policy)).collect();
    let norm_obs: Vec<String> = obs.iter().map(|m| normalize(m, policy)).collect();

    let multiset = |xs: &[String]| {
        let mut m = std::collections::BTreeMap::new();
        for x in xs {
            *m.entry(x.clone()).or_insert(0usize) += 1;
        }
        m
    };
    let mo = multiset(&norm_own);
    let mx = multiset(&norm_obs);

    if mo == mx {
        // 内容（多重集）命中
        if norm_own != norm_obs {
            v.order_ok = false;
            if v.diffs.len() < max_diffs {
                v.diffs.push(FieldDiff {
                    kind: kind_of(obs[0]),
                    path: format!("{}.<块内顺序>", kind_of(obs[0])),
                    ours: norm_own.join(" | "),
                    observed: norm_obs.join(" | "),
                    class: DiffClass::MessageOrder,
                });
            }
        }
        return;
    }

    // 多重集不等 ⇒ 逐条对齐，报字段差异。
    // 口径关系：内容（多重集）相等是顺序相等的前提，所以这里同时把 order_ok 拉下来。
    v.content_ok = false;
    v.order_ok = false;
    for (a, b) in own.iter().zip(obs.iter()) {
        if &normalize(a, policy) == &normalize(b, policy) {
            continue;
        }
        for d in compare_msg(a, b, policy) {
            if v.diffs.len() >= max_diffs {
                return;
            }
            v.diffs.push(d);
        }
        if v.diffs.len() >= max_diffs {
            return;
        }
        // 归一化后仍不同、却没有字段差异 ⇒ 结构差异（例如字段集合不同已被报过）
        if v.diffs.is_empty() {
            v.diffs.push(FieldDiff {
                kind: kind_of(b),
                path: format!("{}.<未知>", kind_of(b)),
                ours: a.canonical(),
                observed: b.canonical(),
                class: DiffClass::Value,
            });
        }
    }
    // 数量不等：多/少
    if own.len() < obs.len() {
        for m in &obs[own.len()..] {
            v.missing.push(brief(m));
        }
        v.content_ok = false;
        if v.diffs.len() < max_diffs {
            v.diffs.push(FieldDiff {
                kind: kind_of(obs[0]),
                path: format!("{}.<缺失>", kind_of(obs[0])),
                ours: format!("(共 {} 条)", own.len()),
                observed: obs.iter().map(|m| brief(m)).collect::<Vec<_>>().join(" ; "),
                class: DiffClass::Presence,
            });
        }
    } else if own.len() > obs.len() {
        for m in &own[obs.len()..] {
            v.extra.push(brief(m));
        }
        v.content_ok = false;
        if v.diffs.len() < max_diffs {
            v.diffs.push(FieldDiff {
                kind: kind_of(obs.first().map(|m| *m).unwrap_or(own[0])),
                path: format!("{}.<多余>", kind_of(obs.first().map(|m| *m).unwrap_or(own[0]))),
                ours: own.iter().map(|m| brief(m)).collect::<Vec<_>>().join(" ; "),
                observed: format!("(共 {} 条)", obs.len()),
                class: DiffClass::Presence,
            });
        }
    }
}

// ===========================================================================
// 8. 报告：逐文件 + 语料汇总
// ===========================================================================

/// 一处"第一分歧"。
#[derive(Debug, Clone)]
pub struct FirstDivergence {
    pub batch_index: usize,
    /// 本批的信封 tick（回放给的）
    pub tick: i64,
    pub shots: usize,
    pub diff: FieldDiff,
}

/// 单份回放的结论。
#[derive(Debug, Clone, Default)]
pub struct FileReport {
    pub name: String,
    pub msgs: usize,
    pub plain_bytes: usize,
    pub batches: usize,
    /// 有至少一发的批
    pub scored_batches: usize,
    pub shots: usize,
    pub content_ok: usize,
    pub order_ok: usize,
    pub kind_seq_ok: usize,
    pub counter_ok: usize,
    /// 回放里有、我方不产的消息条数（按 kind/effect_type 分）
    pub unproduced: std::collections::BTreeMap<String, usize>,
    pub collapsed: usize,
    /// 我方 emitter 产出非法 JSON 的条数
    pub invalid_json: usize,
    /// **所有**字段级差异的计数：`"kind|path|class" -> 次数`。
    ///
    /// 与 `first` 的区别：`first` 是每份回放的**第一处**；这里是全部（用于
    /// "分歧集中在哪些消息类型/哪些字段"的统计，`docs/136` §4）。
    pub diff_counts: std::collections::BTreeMap<String, usize>,
    pub first: Option<FirstDivergence>,
    /// 延迟段：死亡集合是否一致 / 挖洞点是否一致（`None` = 该批不涉及）
    pub deaths_checked: usize,
    pub deaths_ok: usize,
    /// 我方多报 / 漏报 / 报错人（同数量但 id 不同）
    pub deaths_extra: usize,
    pub deaths_missing: usize,
    pub deaths_wrong: usize,
    pub destroy_checked: usize,
    pub destroy_ok: usize,
    /// 挖洞点坐标偏差分档（我方预测 vs 回放声明）
    pub destroy_dx: DeltaBucket,
    pub destroy_dy: DeltaBucket,
    /// 按回合模式分的批数
    pub batches_normal: usize,
    pub batches_diffeq: usize,
    /// 几何探针
    pub geom: GeomReport,
    pub error: Option<String>,
}

/// 语料汇总。
#[derive(Debug, Clone, Default)]
pub struct CorpusReport {
    pub files: Vec<FileReport>,
    pub dir: String,
}

impl CorpusReport {
    pub fn total(&self) -> Totals {
        let mut t = Totals::default();
        for f in &self.files {
            t.files += 1;
            t.msgs += f.msgs;
            t.plain_bytes += f.plain_bytes;
            t.batches += f.batches;
            t.scored_batches += f.scored_batches;
            t.shots += f.shots;
            t.content_ok += f.content_ok;
            t.order_ok += f.order_ok;
            t.kind_seq_ok += f.kind_seq_ok;
            t.counter_ok += f.counter_ok;
            t.collapsed += f.collapsed;
            t.invalid_json += f.invalid_json;
            t.deaths_checked += f.deaths_checked;
            t.deaths_ok += f.deaths_ok;
            t.deaths_extra += f.deaths_extra;
            t.deaths_missing += f.deaths_missing;
            t.deaths_wrong += f.deaths_wrong;
            t.destroy_checked += f.destroy_checked;
            t.destroy_ok += f.destroy_ok;
            if f.error.is_some() {
                t.files_with_error += 1;
            }
            if f.first.is_some() {
                t.files_with_divergence += 1;
            } else if f.scored_batches > 0 {
                t.files_clean += 1;
            }
            for (k, v) in &f.unproduced {
                *t.unproduced.entry(k.clone()).or_insert(0) += v;
            }
            for (k, v) in &f.diff_counts {
                *t.diff_counts.entry(k.clone()).or_insert(0) += v;
            }
            t.destroy_dx.merge(&f.destroy_dx);
            t.destroy_dy.merge(&f.destroy_dy);
            t.batches_normal += f.batches_normal;
            t.batches_diffeq += f.batches_diffeq;
            for (k, v) in &f.geom.end_x {
                t.geom.end_x.entry(k.clone()).or_default().merge(v);
            }
            for (k, v) in &f.geom.stop_y {
                t.geom.stop_y.entry(k.clone()).or_default().merge(v);
            }
            for (k, v) in &f.geom.end_x_blocked {
                t.geom.end_x_blocked.entry(k.clone()).or_default().merge(v);
            }
            for (k, v) in &f.geom.end_x_free {
                t.geom.end_x_free.entry(k.clone()).or_default().merge(v);
            }
            for (k, v) in &f.geom.deaths {
                let e = t.geom.deaths.entry(k.clone()).or_insert((0, 0));
                e.0 += v.0;
                e.1 += v.1;
            }
        }
        t
    }

    /// 第一分歧按字段路径汇总（`FunctionPoints.end_x` 这类）。
    pub fn first_by_path(&self) -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for f in &self.files {
            if let Some(fd) = &f.first {
                *m.entry(fd.diff.path.clone()).or_insert(0) += 1;
            }
        }
        m
    }

    /// 第一分歧按消息种类汇总。
    pub fn first_by_kind(&self) -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for f in &self.files {
            if let Some(fd) = &f.first {
                *m.entry(fd.diff.kind.clone()).or_insert(0) += 1;
            }
        }
        m
    }

    /// 第一分歧按成因分类汇总。
    pub fn first_by_class(&self) -> std::collections::BTreeMap<String, usize> {
        let mut m = std::collections::BTreeMap::new();
        for f in &self.files {
            if let Some(fd) = &f.first {
                *m.entry(class_name(&fd.diff.class)).or_insert(0) += 1;
            }
        }
        m
    }
}

pub fn class_name(c: &DiffClass) -> String {
    match c {
        DiffClass::NumericFormat => "数值格式（值同、字节不同）".into(),
        DiffClass::NumericValue { .. } => "数值（末位差）".into(),
        DiffClass::NumericValueFar => "数值（差 >1e-3）".into(),
        DiffClass::Value => "字符串/枚举值".into(),
        DiffClass::Presence => "字段/消息缺失或多余".into(),
        DiffClass::FieldOrder => "字段顺序".into(),
        DiffClass::MessageOrder => "批内顺序（A 类）".into(),
        DiffClass::KindSequence => "批内消息种类序列".into(),
        DiffClass::CounterGap => "计数器不连续".into(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Totals {
    pub files: usize,
    pub files_with_error: usize,
    pub files_clean: usize,
    pub files_with_divergence: usize,
    pub msgs: usize,
    pub plain_bytes: usize,
    pub batches: usize,
    pub scored_batches: usize,
    pub shots: usize,
    pub content_ok: usize,
    pub order_ok: usize,
    pub kind_seq_ok: usize,
    pub counter_ok: usize,
    pub collapsed: usize,
    pub invalid_json: usize,
    pub deaths_checked: usize,
    pub deaths_ok: usize,
    /// 我方多报 / 漏报 / 报错人（同数量但 id 不同）
    pub deaths_extra: usize,
    pub deaths_missing: usize,
    pub deaths_wrong: usize,
    pub destroy_checked: usize,
    pub destroy_ok: usize,
    pub unproduced: std::collections::BTreeMap<String, usize>,
    pub diff_counts: std::collections::BTreeMap<String, usize>,
    pub destroy_dx: DeltaBucket,
    pub destroy_dy: DeltaBucket,
    pub batches_normal: usize,
    pub batches_diffeq: usize,
    pub geom: GeomReport,
}

/// 回放目录的默认位置（`docs/78` §1）。
///
/// 优先级：`GW2_REPLAY_DIR`（跨平台都能用）→ 平台默认的 Godot `user://` 宿主目录。
pub fn default_corpus_dir() -> std::path::PathBuf {
    if let Ok(d) = std::env::var("GW2_REPLAY_DIR") {
        return std::path::PathBuf::from(d);
    }
    godot_user_replays()
}

/// Godot 4 的 `user://` 宿主目录（Linux 实测口径）：
/// `$HOME/.local/share/godot/app_userdata/Graphwar/replays`。
///
/// Android 上 `HOME` 不设，且 Godot 把 `user://` 放在应用私有目录里；
/// 具体路径 **未知**（要真机跑一次 `OS.get_user_data_dir()` 才有证据），
/// 所以这条路在 Android 上退化成相对路径 —— 用 `GW2_REPLAY_DIR` 显式指定。
#[cfg(not(windows))]
fn godot_user_replays() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join(".local/share/godot/app_userdata/Graphwar/replays")
}

/// Windows 上 `HOME` 通常不设，`%APPDATA%\Godot\app_userdata\Graphwar` 才是 Godot 的 `user://`。
///
/// 证据档位：**推断** —— 取自 Godot 4 文档的 `user://` 口径，本机没有 Windows 桌面对拍；
/// 缺的证据是"在 Windows 上跑一次 `OS.get_user_data_dir()` 看到的实际路径"。
/// 拿不准时用 `GW2_REPLAY_DIR` 覆盖。
#[cfg(windows)]
fn godot_user_replays() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(base)
        .join("Godot")
        .join("app_userdata")
        .join("Graphwar")
        .join("replays")
}

/// 跑一份回放：切批 → 逐批预测 → 逐条比 → 第一分歧。
///
/// 同时跑**几何探针**（§9）：同一发、同一起点、两个步长下我们的采样器给出什么停点，
/// 与回放声明的 `end_x` / 爆炸点 / 死亡集合比。这样"消息不对"能立刻拆成
/// "物理不对"还是"序列化/口径不对"。
pub fn run_file(path: &std::path::Path, policy: &Policy, max_diffs: usize) -> FileReport {
    let mut rep = FileReport {
        name: path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        ..Default::default()
    };
    let replay = match Replay::load(path) {
        Ok(r) => r,
        Err(e) => {
            rep.error = Some(e);
            return rep;
        }
    };
    rep.msgs = replay.msgs.len();
    rep.plain_bytes = replay.plain_bytes;

    for m in &replay.msgs {
        if policy.is_unproduced(m) {
            *rep.unproduced.entry(kind_of(m)).or_insert(0) += 1;
        }
    }

    let batches = split_batches(&replay);
    rep.batches = batches.len();
    if batches.is_empty() {
        return rep;
    }
    let inputs = Inputs::scan(&replay, batches[0].start);
    let mut pred = match Predictor::new(&replay, batches[0].start, policy.clone()) {
        Ok(p) => p,
        Err(e) => {
            rep.error = Some(e);
            return rep;
        }
    };

    // ---- 几何探针的状态：回放声明的障碍/洞 + 存活士兵 ----
    let mut mask = StateMask::new();
    mask.apply(&replay.msgs[..batches[0].start]);
    // 对照实验：同样的状态，但不画抗锯齿环
    let mut mask_noring = StateMask::without_ring();
    mask_noring.apply(&replay.msgs[..batches[0].start]);
    let mut alive: std::collections::BTreeSet<i64> = inputs
        .alive
        .iter()
        .filter(|(_, v)| **v)
        .map(|(k, _)| *k)
        .collect();
    // 位置也会在文件中途出现（战役回放一份里有多关、目标实体后加），
    // 所以不能只用"第一批之前的快照"。
    let mut pos: std::collections::BTreeMap<i64, (f64, f64)> = inputs.pos.clone();
    for m in &replay.msgs[..batches[0].start] {
        if m.kind == "PosInfo" {
            if let Some(id) = m.get_i64("entity_id") {
                alive.insert(id);
            }
        }
    }
    let mut mode = if inputs.room.function_mode == "DiffEqFunction" {
        crate::graphwar_path_calculator::Mode::Ode
    } else {
        crate::graphwar_path_calculator::Mode::Normal
    };
    let mut cursor = batches[0].start;
    let mut attributed_deaths: std::collections::BTreeSet<i64> = Default::default();
    // 两个步长都跑：0.5 = `Match::fire` 现在的取值；0.01 = 真服务端的取值（`AGENTS.md` 硬事实）
    const PROBE_STEPS: [f64; 2] = [0.5, 0.01];

    for (idx, b) in batches.iter().enumerate() {
        // 把上一批之后到本批之前的报文应用进探针状态
        for m in &replay.msgs[cursor..b.start] {
            match m.kind.as_str() {
                "FunctionModeInfo" => {
                    mode = if m.get_str("function_mode").as_deref() == Some("DiffEqFunction") {
                        crate::graphwar_path_calculator::Mode::Ode
                    } else {
                        crate::graphwar_path_calculator::Mode::Normal
                    }
                }
                "LifeInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        let dead = matches!(m.get("alive").map(|v| v.kind.clone()), Some(Kind::Bool(false)));
                        if dead {
                            alive.remove(&id);
                        } else {
                            alive.insert(id);
                        }
                    }
                }
                "EntityRemoved" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        alive.remove(&id);
                        pos.remove(&id);
                    }
                }
                "PosInfo" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        pos.insert(
                            id,
                            (m.get_f64("pos_x").unwrap_or(0.0), m.get_f64("pos_y").unwrap_or(0.0)),
                        );
                        alive.insert(id);
                    }
                }
                "RemoveTeam" => {
                    if let Some(id) = m.get_i64("entity_id") {
                        alive.remove(&id);
                    }
                }
                _ => {}
            }
        }
        mask.apply(&replay.msgs[cursor..b.start]);
        mask_noring.apply(&replay.msgs[cursor..b.start]);
        cursor = b.start;

        if b.shots.is_empty() {
            continue;
        }
        rep.scored_batches += 1;
        rep.shots += b.shots.len();
        let tick = replay.msgs[b.start].tick;
        let p = pred.predict(b, tick);
        let observed: Vec<Msg> = replay.msgs[b.start..=b.end].to_vec();
        let mut v = compare_batch(&p.msgs, &observed, policy, max_diffs);
        v.shots = b.shots.len();
        for d in &v.diffs {
            *rep.diff_counts
                .entry(format!("{}|{}|{}", d.kind, d.path, class_name(&d.class)))
                .or_insert(0) += 1;
        }
        if !p.invalid.is_empty() {
            rep.invalid_json += p.invalid.len();
            v.content_ok = false;
            v.order_ok = false;
            if v.first.is_none() {
                v.first = Some(FieldDiff {
                    kind: "<我方 emitter>".into(),
                    path: "<我方产出的报文不是合法 JSON>".into(),
                    ours: p.invalid[0].clone(),
                    observed: observed.first().map(brief).unwrap_or_default(),
                    class: DiffClass::Value,
                });
            }
            if let Some(f) = &v.first {
                v.diffs.push(f.clone());
            }
        }
        if v.collapsed > 0 {
            rep.collapsed += v.collapsed;
        }
        if v.content_ok {
            rep.content_ok += 1;
        }
        if v.order_ok {
            rep.order_ok += 1;
        }
        if v.kind_seq_ok {
            rep.kind_seq_ok += 1;
        }
        if v.counter_ok {
            rep.counter_ok += 1;
        }
        if mode == crate::graphwar_path_calculator::Mode::Ode {
            rep.batches_diffeq += 1;
        } else {
            rep.batches_normal += 1;
        }

        // ---- 延迟段：死亡集合 / 挖洞点（我方预测 vs 回放声明）----
        // 死亡/挖洞的延迟段：从本批 `ExecutingResults` 到**下一批 `ExecutingResults`**。
        // 为什么要跨到下一批：实测 `LifeInfo{alive:false}` 比开火晚 ~165 tick，常落在
        // 下一个回合里（回放信封 tick 是我的唯一时间依据）。已经归属过的 id 不再算第二次。
        let next_end = batches.get(idx + 1).map(|n| n.end + 1).unwrap_or(replay.msgs.len());
        let (mut obs_deaths, obs_destroy) = tail_state(&replay.msgs[b.end + 1..next_end]);
        obs_deaths.retain(|id| !attributed_deaths.contains(id));
        for id in &obs_deaths {
            attributed_deaths.insert(*id);
        }
        if !obs_deaths.is_empty() || !p.killed.is_empty() {
            rep.deaths_checked += 1;
            let mut a = p.killed.clone();
            let mut c = obs_deaths.clone();
            a.sort_unstable();
            c.sort_unstable();
            if a == c {
                rep.deaths_ok += 1;
            } else {
                if a.len() > c.len() {
                    rep.deaths_extra += 1;
                } else if a.len() < c.len() {
                    rep.deaths_missing += 1;
                } else {
                    rep.deaths_wrong += 1;
                }
            }
            if a != c && v.first.is_none() {
                v.first = Some(FieldDiff {
                    kind: "LifeInfo".into(),
                    path: "LifeInfo.<死亡集合>".into(),
                    ours: format!("{a:?}"),
                    observed: format!("{c:?}"),
                    class: DiffClass::Value,
                });
            }
        }
        if let Some((bx, by)) = p.blocked_at {
            if !obs_destroy.is_empty() {
                rep.destroy_checked += 1;
                let ours = (fmt_f64(bx), fmt_f64(by));
                if obs_destroy.contains(&ours) {
                    rep.destroy_ok += 1;
                }
                for (ox, oy) in &obs_destroy {
                    rep.destroy_dx.add(bx, ox.parse().unwrap_or(f64::NAN));
                    rep.destroy_dy.add(by, oy.parse().unwrap_or(f64::NAN));
                }
                if !obs_destroy.contains(&ours) && v.first.is_none() {
                    v.first = Some(FieldDiff {
                        kind: "DestroyObstacle".into(),
                        path: "DestroyObstacle.<挖洞点>".into(),
                        ours: fmt_stop(bx, by),
                        observed: format!("{obs_destroy:?}"),
                        class: DiffClass::Value,
                    });
                }
            }
        }

        // ================= 几何探针 =================
        let obs_end_x: std::collections::BTreeMap<i64, f64> = observed
            .iter()
            .filter(|m| m.kind == "FunctionPoints")
            .filter_map(|m| Some((m.get_i64("owner_id")?, m.get_f64("end_x")?)))
            .collect();
        // 爆炸点按 `FunctionActive` 的先后配对到射手（真实回放就是这个顺序）
        let mut obs_expl: std::collections::BTreeMap<i64, (f64, f64)> = Default::default();
        {
            let mut pending: Option<i64> = None;
            for m in &observed {
                if m.kind == "FunctionActive" {
                    pending = m.get_i64("owner_id");
                } else if m.kind == "EffectInfo"
                    && m.get_str("effect_type").as_deref() == Some("ExplosionEffect")
                {
                    if let Some(o) = pending.take() {
                        obs_expl.insert(o, (m.get_f64("pos_x").unwrap_or(f64::NAN), m.get_f64("pos_y").unwrap_or(f64::NAN)));
                    }
                }
            }
        }
        let mode_name = if mode == crate::graphwar_path_calculator::Mode::Ode { "DiffEq" } else { "Normal" };
        for (mask_name, mref) in [("ring", &mask), ("fill", &mask_noring)] {
        for step in PROBE_STEPS {
            let key = format!("{mode_name} step={step} mask={mask_name}");
            for shot in &b.shots {
                let targets: Vec<Target> = alive
                    .iter()
                    .filter(|id| **id != shot.owner)
                    .filter_map(|id| {
                        pos.get(id).map(|(x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
                    })
                    .collect();
                let out = crate::graphwar_shot_calculator::settle::resolve(
                    &shot.text,
                    shot.x0,
                    shot.y0,
                    step,
                    shot.flip,
                    mode,
                    mref.mask(),
                    &targets,
                );
                if let Some(&ex) = obs_end_x.get(&shot.owner) {
                    rep.geom.end_x.entry(key.clone()).or_default().add(out.stop.0, ex);
                    if out.blocked {
                        rep.geom.end_x_blocked.entry(key.clone()).or_default().add(out.stop.0, ex);
                    } else {
                        rep.geom.end_x_free.entry(key.clone()).or_default().add(out.stop.0, ex);
                    }
                }
                if let Some(&(_ex, ey)) = obs_expl.get(&shot.owner) {
                    rep.geom.stop_y.entry(key.clone()).or_default().add(out.stop.1, ey);
                }
                let d = rep.geom.deaths.entry(key.clone()).or_insert((0, 0));
                if !obs_deaths.is_empty() || !out.killed.is_empty() {
                    let mut a = out.killed.clone();
                    let mut c = obs_deaths.clone();
                    a.sort_unstable();
                    c.sort_unstable();
                    d.1 += 1;
                    if a == c {
                        d.0 += 1;
                    }
                }
            }
        }
        }

        if rep.first.is_none() {
            if let Some(fd) = v.first.clone() {
                rep.first = Some(FirstDivergence {
                    batch_index: idx,
                    tick,
                    shots: b.shots.len(),
                    diff: fd,
                });
            }
        }
    }
    rep
}

fn fmt_f64(v: f64) -> String {
    let mut s = String::new();
    use std::fmt::Write;
    let _ = write!(s, "{v}");
    s
}

fn fmt_stop(x: f64, y: f64) -> String {
    format!("({}, {})", fmt_f64(x), fmt_f64(y))
}

/// 延迟段里回放声明的死亡 id 与挖洞点（各自去重、保序）。
fn tail_state(msgs: &[Msg]) -> (Vec<i64>, Vec<(String, String)>) {
    let mut deaths = Vec::new();
    let mut destroy: Vec<(String, String)> = Vec::new();
    for m in msgs {
        match m.kind.as_str() {
            "LifeInfo" => {
                let alive = matches!(m.get("alive").map(|v| v.kind.clone()), Some(Kind::Bool(true)));
                if !alive {
                    if let Some(id) = m.get_i64("entity_id") {
                        if !deaths.contains(&id) {
                            deaths.push(id);
                        }
                    }
                }
            }
            "DestroyObstacle" => {
                let key = (
                    m.get_f64("pos_x").map(fmt_f64).unwrap_or_default(),
                    m.get_f64("pos_y").map(fmt_f64).unwrap_or_default(),
                );
                if !destroy.contains(&key) {
                    destroy.push(key);
                }
            }
            _ => {}
        }
    }
    (deaths, destroy)
}

/// 跑整个目录（**只读**；按文件名排序保证可重复）。
pub fn run_dir(dir: &std::path::Path, policy: &Policy, max_diffs: usize) -> CorpusReport {
    let mut names: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    let mut rep = CorpusReport { dir: dir.display().to_string(), ..Default::default() };
    for p in names {
        rep.files.push(run_file(&p, policy, max_diffs));
    }
    rep
}

// ===========================================================================
// 9. 几何探针：把"消息对不对"拆成"物理对不对 + 序列化对不对"
// ===========================================================================

/// 用**回放声明**的状态（障碍 + 挖洞）建的掩膜，供几何探针使用。
///
/// 与 [`Predictor`] 的区别：`Predictor` 用**我们自己的**演变状态（含自己挖的洞），
/// 那是"整机跑一遍"；本探针用回放声明的状态，只回答一个问题：
/// **同一条函数、同一个起点、同一个步长，我们的采样器给不给同一个停点。**
pub struct StateMask {
    mask: Vec<u8>,
    pub obstacles: usize,
    pub holes: usize,
    /// 是否画抗锯齿环（`plane::rasterize` 第 ② 步）。
    ///
    /// 置 `false` 时只做硬填（`dx²+dy² <= r²` 的方形近似），用来做**对照实验**：
    /// 如果"停点差"里有相当一部分来自抗锯齿环（环里混出的中间值 `!= 0xFF`，
    /// 于是 `is_occupied` 判成阻挡），关掉环应当明显好转。这只是探针，
    /// **不是**对 `plane.rs` 的改动。
    pub ring: bool,
}

impl StateMask {
    pub fn new() -> Self {
        StateMask { mask: crate::plane::empty_mask(), obstacles: 0, holes: 0, ring: true }
    }

    pub fn without_ring() -> Self {
        StateMask { ring: false, ..Self::new() }
    }

    fn draw(&mut self, x: f64, y: f64, size: f64, fill: u8) {
        if self.ring {
            crate::plane::rasterize(&mut self.mask, x, y, size, fill);
            return;
        }
        // 硬填：像素中心落在圆内才填（与 `plane::rasterize` 第 ① 步同构，但没有环）
        let (px, py, r) = crate::plane::raster_params(x, y, size);
        let r2 = r * r;
        for dy in -r..=r {
            let row = py + dy;
            if !(0..crate::plane::CANVAS_H as i32).contains(&row) {
                continue;
            }
            let inside = r2 - dy * dy;
            let hw = (inside as f64).sqrt() as i32;
            let base = row as usize * crate::plane::CANVAS_W as usize;
            for dx in -hw..=hw {
                let col = px + dx;
                if (0..crate::plane::CANVAS_W as i32).contains(&col) {
                    self.mask[base + col as usize] = fill;
                }
            }
        }
    }

    /// 把一批报文里的状态声明应用进掩膜。
    pub fn apply(&mut self, msgs: &[Msg]) {
        for m in msgs {
            match m.kind.as_str() {
                "ClearObstacles" => {
                    self.mask = crate::plane::empty_mask();
                    self.obstacles = 0;
                    self.holes = 0;
                }
                "AddObstacle" => {
                    let x = m.get_f64("pos_x").unwrap_or(0.0);
                    let y = m.get_f64("pos_y").unwrap_or(0.0);
                    let r = m.get_f64("radius").unwrap_or(0.0);
                    self.draw(x, y, r, 0x00);
                    self.obstacles += 1;
                }
                "DestroyObstacle" => {
                    // 挖洞点 = 弹道停点，半径 0.5、填充 0xFF（`docs/39`；回放的
                    // `DestroyObstacle.radius` 恒为 0.5，503/503）。
                    let x = m.get_f64("pos_x").unwrap_or(0.0);
                    let y = m.get_f64("pos_y").unwrap_or(0.0);
                    self.draw(x, y, 0.5, 0xFF);
                    self.holes += 1;
                }
                _ => {}
            }
        }
    }

    pub fn mask(&self) -> &[u8] {
        &self.mask
    }
}

/// 几何探针的一次测量。
#[derive(Debug, Clone)]
pub struct GeomMeasure {
    pub step: f64,
    pub mode: crate::graphwar_path_calculator::Mode,
    /// 我方停点
    pub stop: (f64, f64),
    /// 我方判定的死者
    pub killed: Vec<i64>,
    /// 是否撞障碍
    pub blocked: bool,
}

/// 观测量与几何量的差：按"逐位 / 1e-9 / 1e-3 / 0.05 / 更大"分档。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeltaBucket {
    pub exact: usize,
    pub tiny: usize,
    pub small: usize,
    pub mid: usize,
    pub far: usize,
}

impl DeltaBucket {
    pub fn add(&mut self, a: f64, b: f64) {
        if a.is_nan() || b.is_nan() {
            self.far += 1;
            return;
        }
        let d = (a - b).abs();
        if d == 0.0 {
            self.exact += 1;
        } else if d <= 1e-9 {
            self.tiny += 1;
        } else if d <= 1e-3 {
            self.small += 1;
        } else if d <= 0.05 {
            self.mid += 1;
        } else {
            self.far += 1;
        }
    }

    pub fn total(&self) -> usize {
        self.exact + self.tiny + self.small + self.mid + self.far
    }

    pub fn merge(&mut self, o: &DeltaBucket) {
        self.exact += o.exact;
        self.tiny += o.tiny;
        self.small += o.small;
        self.mid += o.mid;
        self.far += o.far;
    }

    /// `|Δ| <= 1e-3` 的占比（"几何上同一点"的宽松口径，用于对比步长）
    pub fn within_small(&self) -> usize {
        self.exact + self.tiny + self.small
    }
}

impl std::fmt::Display for DeltaBucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "0:{:>4} <=1e-9:{:>4} <=1e-3:{:>4} <=5e-2:{:>4} >5e-2:{:>4}",
            self.exact, self.tiny, self.small, self.mid, self.far
        )
    }
}

/// 几何探针的汇总（按模式 × 步长分开）。
#[derive(Debug, Clone, Default)]
pub struct GeomReport {
    /// `(模式名, 步长) -> end_x 偏差分档`
    pub end_x: std::collections::BTreeMap<String, DeltaBucket>,
    /// `(模式名, 步长) -> 停点 y 偏差分档`
    pub stop_y: std::collections::BTreeMap<String, DeltaBucket>,
    /// `(模式名, 步长) -> 死亡集合命中 / 计数`
    pub deaths: std::collections::BTreeMap<String, (usize, usize)>,
    /// 只看"我方判定撞到障碍"的那些发 —— 用来分辨"没障碍的自由飞行"与"碰撞"两类
    pub end_x_blocked: std::collections::BTreeMap<String, DeltaBucket>,
    /// 只看"我方判定没撞障碍"的那些发
    pub end_x_free: std::collections::BTreeMap<String, DeltaBucket>,
}
