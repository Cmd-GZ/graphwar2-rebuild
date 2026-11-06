//! 录像导出：`GraphwarVideoWriter` —— **真的是 MP4 视频**（不是别的东西被我们猜错了名字）。
//!
//! 本模块回答四件事：**它到底写不写视频**（写：openh264 编码 + minimp4 封装）、
//! **分辨率/帧率**（构造入参，尺寸向上取偶；`60` fps）、**写到哪里**
//! （Godot `FileAccess`，`WRITE` 模式，路径由调用方给）、**轨道叫什么**（`Graphwar Clip`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（3 个，全部已定位）
//!
//! | 函数 | arm64 vaddr / 字节 | x86 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x449964` / 528 | `0x4f0bb0` / 624 | `:61:37` | 建 openh264 编码器 + 尺寸取偶 | 逐位 |
//! | `add_frame` | `0x449b74` / 1116 | `0x4f0e20` / 1392 | `:125:41` | RGB(A) → YUV → H.264 编码 → 收 NAL | 逐位 |
//! | `write_video` | `0x4494e4` / 1152 | `0x4f0610` / 1440 | `:154:56` `:154:93` | 封装 MP4 → 一次性写盘 | 逐位 |
//!
//! 三个名字都由**同名的 `panic::Location` 锚**在两侧对上（`:61:37` / `:125:41` /
//! `:154:56`+`:154:93`）。尺寸比 1.18 / 1.25 / 1.25 —— `add_frame` / `write_video`
//! **略超** 0.61–1.19 的闸（x86 侧内联了更多 `Gd` 胶水），但 Location 判等不看尺寸，
//! 所以归属仍然是钉死的。
//!
//! ## 2. 它用什么写的（逐位，靠被调者符号名）
//!
//! | 证据 | 地址 | 说明 |
//! |---|---|---|
//! | `openh264::encoder::Encoder::new` | `0x4499b0` | 编码器（H.264），配置结构 5264 字节压在栈上 |
//! | `openh264::formats::yuv::YUVBuffer::new` | `0x449a70` | 按「取偶后的宽高」建 YUV 缓冲 |
//! | `openh264::formats::rgb::RgbSliceU8::new` / `RgbaSliceU8::new` | `0x449be8` / `0x449c68` | 两种输入格式 |
//! | `openh264::formats::yuv::YUVBuffer::read_rgb` | `0x449bf4` / `0x449c74` | RGB→YUV |
//! | `openh264::encoder::Encoder::encode_at` | `0x449cb0` / `0x449d18` | 带时间戳编码 |
//! | `openh264::encoder::EncodedBitStream::write_vec` | `0x449cd8` / `0x449d48` | 取 Annex-B 码流 |
//! | `<openh264::time::Timestamp as Add<Duration>>::add` | `0x449ce8` / `0x449d58` | 帧时间戳累加 |
//! | `minimp4::Mp4Muxer::<W>::init_video` | `0x449550` | MP4 封装器（`W = &mut Cursor<Vec<u8>>`） |
//! | `minimp4::writer::write_mp4` | `0x449564` | **fps 立即数 `mov w1,#0x3c` = 60** |
//! | `MP4E_close` | `0x44956c` | 收尾 |
//! | `FileAccess::open` + `store_buffer` | `0x449594` / `0x4495e0` | 一次性把整段 MP4 写进 Godot 文件 |
//!
//! `minimp4` 的 `panic::Location`（`minimp4.rs` 仓库 `.../minimp4/src/lib.rs:86:57`）
//! 直接出现在 `write_video` 引用的 Location 表里 ⇒ 依赖关系是**函数级**确认的，
//! 不是靠字符串猜的。
//!
//! ## 3. 数据结构与尺寸（逐位）
//!
//! ```text
//! GraphwarVideoWriter（GDExtension 类实例）
//!   +0x14e8 : i32  构造入参 width
//!   +0x14ec : i32  构造入参 height
//!   +0x14f0 : i32  取偶后 width   = width  + (width  & 1)
//!   +0x14f4 : i32  取偶后 height  = height + (height & 1)
//!   +0x14f8 : u8   构造入参的第三个（bool）
//!   +0x08/+0x10 : Vec<u8>  已编码的码流（`Vec` 的指针/长度）
//! ```
//!
//! `add w23, w8, w22`（`w8 = w22 & 1`）与 `add w24, w9, w21` 是**向上取偶**：
//! H.264 不接受奇数宽高，这一步是硬需求，不是巧合。
//! 落盘前 `write_video` 用 `tbnz w1, #0x1f` 检查两个尺寸非负（i32→u32 转换的
//! panic 前置检查），所以取偶后的值就是封装用的尺寸。
//!
//! ## 4. 语义链条（逐位）
//!
//! ```text
//! new(w, h, flag)        → Encoder + YUVBuffer(取偶 w, 取偶 h)
//! add_frame(image)       → YUVBuffer::read_rgb → Encoder::encode_at(ts) → write_vec → frames.push
//!                          ts += Duration(一帧)
//! write_video(path)      → Mp4Muxer::init_video(取偶 w, 取偶 h, .., "Graphwar Clip")
//!                          minimp4::writer::write_mp4(&mut muxer, 60, &frames)
//!                          MP4E_close → FileAccess::open(path, WRITE) → store_buffer(mp4)
//! ```
//!
//! 轨道名 `Graphwar Clip`（13 字节，`.rodata 0x19ddaf`）是 `init_video` 的第 5 个参数
//! （`mov w5,#0xd` = 长度 13）。
//!
//! ## 5. 本 crate 里的模型与**不做什么**
//!
//! `gw2-rebuild` 的依赖表是**空的**（纯 `std`），openh264 / minimp4 都不在。
//! ⇒ 这里实现的是**可验证的那一半**：
//!
//! * 尺寸取偶、帧计数、时间戳步进、fps=60、轨道名、目标路径 —— 全部可断言；
//! * **H.264 码流本身不生成**：那是 openh264 的算法实现，我们不复刻，也不假装
//!   `write_video` 产出了能播放的 MP4。[`GraphwarVideoWriter::write_video`] 返回一个
//!   [`Mp4Job`]（把「封装器会收到什么」写清楚），由上层决定是否接真实编码器。
//!
//! **未知**：第三个人参（`+0x14f8` 的 bool）语义没定（`subs wzr, w19, #0` 之后分两条
//! 路设置 `EncoderConfig`，但具体设哪个字段没逐条解）；`EncoderConfig` 的码率/关键帧
//! 间隔也没有逐字段读（栈上 5264 字节的初始化只用 `stp q` 搬，没跟进字段名）。

/// 轨道名（`init_video` 的第 5 个参数，长度 13）。
pub const TRACK_NAME: &str = "Graphwar Clip";

/// `write_mp4` 的 fps 立即数（`mov w1, #0x3c`）。
pub const FPS: u32 = 60;

/// `add_frame` 支持的两条输入支路（`RgbSliceU8` / `RgbaSliceU8`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    /// 3 字节/像素。
    Rgb,
    /// 4 字节/像素。
    Rgba,
}

impl FrameFormat {
    /// 每像素字节数。
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            FrameFormat::Rgb => 3,
            FrameFormat::Rgba => 4,
        }
    }
}

/// 一帧（原版是 YUV420 缓冲 + 编码后的 NAL；这里保留**原始像素**，
/// 编码不在本 crate 的范围内，见模块文档 §5）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub format: FrameFormat,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    /// 该帧的时间戳（毫秒，原版是 `openh264::time::Timestamp`）。
    pub timestamp_ms: u64,
}

/// `add_frame` 的失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// 宽高乘积 × 每像素字节数 与 `pixels.len()` 不符。
    SizeMismatch,
    /// 宽或高为 0。
    Empty,
}

/// `write_video` 交给封装器的活（原版是「就地调 minimp4」，这里显式描述）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp4Job {
    pub path: String,
    pub width: i32,
    pub height: i32,
    pub fps: u32,
    pub track_name: &'static str,
    pub frame_count: usize,
    /// 逐帧时间戳（毫秒）。
    pub timestamps_ms: Vec<u64>,
}

/// 视频导出器。
#[derive(Debug, Default)]
pub struct GraphwarVideoWriter {
    /// 构造入参（原版 `+0x14e8` / `+0x14ec`）。
    pub width: i32,
    pub height: i32,
    /// 取偶后的编码尺寸（原版 `+0x14f0` / `+0x14f4`）。
    pub frame_width: i32,
    pub frame_height: i32,
    /// 第三个人参（原版 `+0x14f8`，语义未知，见 §5）。
    pub flag: bool,
    frames: Vec<Frame>,
    next_timestamp_ms: u64,
}

impl GraphwarVideoWriter {
    /// `new(width, height, flag)`（arm64 `0x449964`）。
    ///
    /// 取偶规则是 `w + (w & 1)`，**只对正数**成立（负数会向上取偶到更小绝对值，
    /// 原版同样如此；`write_video` 会先拦 `>= 0`）。
    pub fn new(width: i32, height: i32, flag: bool) -> Self {
        Self {
            width,
            height,
            frame_width: width + (width & 1),
            frame_height: height + (height & 1),
            flag,
            frames: Vec::new(),
            next_timestamp_ms: 0,
        }
    }

    /// 取偶尺寸（`i32`，与入参同宽）。
    pub fn even_dims(w: i32, h: i32) -> (i32, i32) {
        (w + (w & 1), h + (h & 1))
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// `add_frame`（arm64 `0x449b74`）：收一帧并推进时间戳。
    ///
    /// 原版顺序：`Rgb/RgbaSliceU8::new` → `YUVBuffer::read_rgb` → `Encoder::encode_at`
    /// → `EncodedBitStream::write_vec` → 收进 `Vec<u8>` → `Timestamp += 一帧`。
    /// 我们做到「校验 + 收帧 + 时间戳步进」，编码留给真实编码器（§5）。
    pub fn add_frame(
        &mut self,
        format: FrameFormat,
        width: usize,
        height: usize,
        pixels: &[u8],
    ) -> Result<(), FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::Empty);
        }
        if pixels.len() != width * height * format.bytes_per_pixel() {
            return Err(FrameError::SizeMismatch);
        }
        self.frames.push(Frame {
            format,
            width,
            height,
            pixels: pixels.to_vec(),
            timestamp_ms: self.next_timestamp_ms,
        });
        // 一帧的时长，60 fps ⇒ 16.67 ms。原版是 `Timestamp + Duration`，
        // Duration 由编码器配置里的帧率算；这里固定用 1000/60 的整数版（16 ms）。
        self.next_timestamp_ms += 1000 / FPS as u64;
        Ok(())
    }

    /// `write_video(path)`（arm64 `0x4494e4`）：把所有帧封成 MP4 写进 `path`。
    ///
    /// 返回 `None` = 没有帧（原版 `writer::write_mp4` 拿到空码流时会写一个
    /// 空轨道；这里显式区分，避免上层误以为写成功）。
    pub fn write_video(&self, path: &str) -> Option<Mp4Job> {
        if self.frames.is_empty() {
            return None;
        }
        Some(Mp4Job {
            path: path.to_string(),
            width: self.frame_width,
            height: self.frame_height,
            fps: FPS,
            track_name: TRACK_NAME,
            frame_count: self.frames.len(),
            timestamps_ms: self.frames.iter().map(|f| f.timestamp_ms).collect(),
        })
    }
}
