//! **不是原版的东西**：`humm.wav` 的 WAV(RIFF/PCM) 解析 + 归一化 —— 原版这一步用的是
//! 第三方 **`hound`**（`docs/111` §4.4(b)：`FileAccess::get_file_as_bytes` `0x3eaa00`
//! @`0x4bf681` → `hound` 解析 `0x50a080` @`0x4bf6c3` → `AudioStreamGenerator`
//! `set_mix_rate(16000.0f)` / `set_buffer_length(0.1f)` → `push_buffer`），
//! 归一化常数 `32767.0f` 在 `0x4c08f6`（**逐位**）。
//!
//! 本 crate **零第三方依赖**（`Cargo.toml` 只有可选的 `godot`），所以这里是 `hound` 的
//! **最小等价物**：只支持原版实际会遇到的形状（RIFF/WAVE + `fmt ` + `data` + PCM 16-bit +
//! 单声道），其余一律**显式报错**，不猜、不降级（原版 `hound` 会自己判错，我方也一样）。
//!
//! 为什么落在 `_scaffold/`：原版这段代码**不在** `graphwar_audio_control` 里（它只是调
//! `hound` 的公开 API），所以照 `AGENTS.md` 第 4 条"指不到原版模块的进 `_scaffold/`"。
//! 纯逻辑、无 gdext，所以 `cargo test` 直接测（`#[cfg(test)]` 在本文件末尾）。
//!
//! # 与 `.gd` 改前那份的关系
//!
//! `client/src/audio/gw_audio.gd:177-226`（改前）是同一件事的 GDScript 版（50 行）。
//! 本轮把它搬到这里，`.gd` 只把 Rust 交回的 `PackedVector2Array` 塞进
//! `AudioStreamGeneratorPlayback`。逐段判定见 `fixtures/controller_migration2_ledger.tsv` A07。

/// 原版用到的 PCM 满量程（`0x4c08f6` = `32767.0f`）。**逐位**。
pub const PCM_FULL_SCALE: f32 = 32767.0;

/// 解析成功的 PCM。
#[derive(Debug, Clone, PartialEq)]
pub struct WavPcm {
    /// 采样率（原版 humm.wav = 16000）。
    pub rate: u32,
    /// 声道数（本实现只接受 1）。
    pub channels: u16,
    /// 逐样本归一化值：`i16 / 32767.0`（**单声道**，所以只有一条轨）。
    pub samples: Vec<f32>,
}

/// 解析失败原因（每条都能直接显示给用户；`.gd` 改前那批措辞照搬）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WavError {
    /// 不是 `RIFF....WAVE`
    NotRiffWave,
    /// 某个块的 `body + size` 越界（带块名与数值）
    ChunkOutOfBounds {
        /// 四字节块名（ASCII，可能带空格/不可打印）
        id: String,
        /// 块声明长度
        size: u32,
        /// 块体起始偏移
        body: usize,
    },
    /// `fmt ` 不是 PCM 或位深不是 16
    UnsupportedFormat {
        /// `wFormatTag`
        format: u16,
        /// 位深
        bits: u16,
    },
    /// 没有 `data` 块（或长度为 0）
    NoData,
    /// 声道数不是 1
    NotMono(u16),
    /// 读 `fmt ` 时块体太短
    ShortFmt,
}

impl WavError {
    /// 人可读的原因（`.gd` 直接打印/塞进 `_humm_error`）。
    pub fn message(&self) -> String {
        match self {
            WavError::NotRiffWave => "不是 RIFF/WAVE".to_string(),
            WavError::ChunkOutOfBounds { id, size, body } => {
                format!("块 {} 越界（{} @{}）", id, size, body)
            }
            WavError::UnsupportedFormat { format, bits } => {
                format!("只支持 PCM 16-bit（format={} bits={}）", format, bits)
            }
            WavError::NoData => "没有 data 块".to_string(),
            WavError::NotMono(ch) => format!("只支持单声道（channels={}）", ch),
            WavError::ShortFmt => "fmt 块太短".to_string(),
        }
    }
}

fn le_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn le_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn chunk_id(b: &[u8], off: usize) -> String {
    String::from_utf8_lossy(&b[off..off + 4]).to_string()
}

/// 解析一段 WAV 字节（原版 `hound::WavReader::new(bytes)` 的等价物）。
///
/// 块遍历照 RIFF 规矩：`[id 4][size 4 LE][body size]`，`size` 为奇数时补 1 字节对齐。
pub fn decode_wav(bytes: &[u8]) -> Result<WavPcm, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotRiffWave);
    }
    let mut off = 12usize;
    let mut format_ok = false;
    let mut channels = 0u16;
    let mut rate = 0u32;
    let mut bits = 0u16;
    let mut data_off: Option<usize> = None;
    let mut data_len = 0usize;

    while off + 8 <= bytes.len() {
        let id = chunk_id(bytes, off);
        let sz = le_u32(bytes, off + 4);
        let body = off + 8;
        if body + sz as usize > bytes.len() {
            return Err(WavError::ChunkOutOfBounds { id, size: sz, body });
        }
        let sz = sz as usize;
        if id == "fmt " {
            if sz < 16 {
                return Err(WavError::ShortFmt);
            }
            let fmt = le_u16(bytes, body);
            channels = le_u16(bytes, body + 2);
            rate = le_u32(bytes, body + 4);
            bits = le_u16(bytes, body + 14);
            format_ok = fmt == 1 && bits == 16;
        } else if id == "data" {
            data_off = Some(body);
            data_len = sz;
        }
        off = body + sz + (sz & 1);
    }

    if !format_ok {
        return Err(WavError::UnsupportedFormat { format: le_u16(bytes, 20), bits });
    }
    let Some(start) = data_off else {
        return Err(WavError::NoData);
    };
    if data_len == 0 {
        return Err(WavError::NoData);
    }
    if channels != 1 {
        return Err(WavError::NotMono(channels));
    }
    let sample_count = data_len / 2;
    let mut samples = Vec::with_capacity(sample_count);
    for i in 0..sample_count {
        let v = i16::from_le_bytes([bytes[start + i * 2], bytes[start + i * 2 + 1]]);
        samples.push(v as f32 / PCM_FULL_SCALE);
    }
    Ok(WavPcm { rate, channels, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一份最小的 PCM16 单声道 WAV。
    fn make_wav(rate: u32, channels: u16, bits: u16, format: u16, pcm: &[i16]) -> Vec<u8> {
        let data_len = (pcm.len() * 2) as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes()); // byte rate
        out.extend_from_slice(&(channels * bits / 8).to_le_bytes()); // block align
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for s in pcm {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    #[test]
    fn decodes_mono_pcm16_and_normalizes_by_32767() {
        // 极值：max / min / 0 三个样本，逐位检查除法结果。
        let wav = make_wav(16000, 1, 16, 1, &[32767, -32768, 0]);
        let got = decode_wav(&wav).expect("应当解析成功");
        assert_eq!(got.rate, 16000);
        assert_eq!(got.channels, 1);
        assert_eq!(got.samples.len(), 3);
        assert_eq!(got.samples[0], 1.0f32); // 32767/32767
        assert_eq!(got.samples[1], -32768.0f32 / 32767.0f32);
        assert_eq!(got.samples[2], 0.0f32);
    }

    #[test]
    fn rejects_non_riff() {
        assert_eq!(decode_wav(b"nope"), Err(WavError::NotRiffWave));
        let mut wav = make_wav(16000, 1, 16, 1, &[0]);
        wav[8] = b'X'; // WAVE -> XAVE
        assert_eq!(decode_wav(&wav), Err(WavError::NotRiffWave));
    }

    #[test]
    fn rejects_non_pcm16() {
        // format = 3 (IEEE float)
        let wav = make_wav(16000, 1, 16, 3, &[0]);
        match decode_wav(&wav) {
            Err(WavError::UnsupportedFormat { format, bits }) => {
                assert_eq!((format, bits), (3, 16));
            }
            other => panic!("应当是 UnsupportedFormat，实际 {:?}", other),
        }
        // 位深 8
        let wav8 = make_wav(16000, 1, 8, 1, &[0]);
        assert!(matches!(decode_wav(&wav8), Err(WavError::UnsupportedFormat { bits: 8, .. })));
    }

    #[test]
    fn rejects_stereo() {
        let wav = make_wav(16000, 2, 16, 1, &[0, 0]);
        assert_eq!(decode_wav(&wav), Err(WavError::NotMono(2)));
    }

    #[test]
    fn rejects_missing_data_and_out_of_bounds_chunk() {
        // 只有 fmt、没有 data
        let full = make_wav(16000, 1, 16, 1, &[0]);
        let no_data = &full[..36];
        assert_eq!(decode_wav(no_data), Err(WavError::NoData));

        // data 块声明长度超过文件尾
        let mut broken = make_wav(16000, 1, 16, 1, &[0, 0]);
        let n = broken.len();
        broken[n - 8] = 0xff;
        broken[n - 7] = 0xff;
        assert!(matches!(decode_wav(&broken), Err(WavError::ChunkOutOfBounds { .. })));
    }

    #[test]
    fn odd_sized_chunk_is_skipped_with_padding() {
        // 在 fmt 之前插一个 3 字节的 LIST 块（奇数长度 ⇒ 后面 1 字节对齐）。
        let base = make_wav(16000, 1, 16, 1, &[100]);
        let mut out = Vec::new();
        out.extend_from_slice(&base[0..12]);
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(b"abc");
        out.push(0); // 对齐字节
        out.extend_from_slice(&base[12..]);
        let got = decode_wav(&out).expect("奇数块应当被跳过");
        assert_eq!(got.samples.len(), 1);
    }
}
