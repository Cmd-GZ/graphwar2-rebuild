//! `graphwar_video_writer` 的验收：**它是真的在录 MP4**（H.264 + minimp4）。
//!
//! ⚠️ 用 `#[path]` 把 `src/graphwar_video_writer.rs` 编进本测试二进制，
//! **不需要** `src/lib.rs` 加 `pub mod`。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `track_is_graphwar_clip_at_60fps` | 轨道名 13 字节、fps 60 | **逐位**（`mov w5,#0xd` / `mov w1,#0x3c`） |
//! | `dimensions_round_up_to_even` | `w + (w & 1)` | **逐位**（`add w23,w8,w22`） |
//! | `add_frame_validates_pixel_count` | 两种输入格式的字节数 | **逐位**（`RgbSliceU8` / `RgbaSliceU8` 两条支路） |
//! | `timestamps_step_by_one_frame` | 时间戳步进 `Timestamp + Duration` | **观测** |
//! | `write_video_needs_frames` | 空 -> `None` | **推断**（原版 `write_mp4` 拿到空表的行为未逐指令解） |
//! | `write_video_describes_the_muxer_input` | 尺寸/帧数/时间戳进 `Mp4Job` | **逐位**（`0x449550`–`0x449564` 的入参） |
#![allow(dead_code)]

#[path = "../src/graphwar_video_writer.rs"]
mod graphwar_video_writer;

use graphwar_video_writer::*;

#[test]
fn track_is_graphwar_clip_at_60fps() {
    assert_eq!(TRACK_NAME, "Graphwar Clip");
    assert_eq!(TRACK_NAME.len(), 13, "init_video 的第 5 个参数长度立即数");
    assert_eq!(FPS, 60);
}

#[test]
fn dimensions_round_up_to_even() {
    assert_eq!(GraphwarVideoWriter::even_dims(1280, 720), (1280, 720));
    assert_eq!(GraphwarVideoWriter::even_dims(1281, 721), (1282, 722));
    // 奇数 +1，偶数不动
    for w in 0..20i32 {
        let (ew, eh) = GraphwarVideoWriter::even_dims(w, w + 1);
        assert_eq!(ew % 2, 0, "width {}", w);
        assert_eq!(eh % 2, 0, "height {}", w);
        assert!(ew >= w && ew <= w + 1);
        assert!(eh >= w + 1 && eh <= w + 2);
    }

    let vw = GraphwarVideoWriter::new(1281, 721, true);
    assert_eq!(vw.width, 1281);
    assert_eq!(vw.height, 721);
    assert_eq!(vw.frame_width, 1282);
    assert_eq!(vw.frame_height, 722);
    assert!(vw.flag);
}

#[test]
fn add_frame_validates_pixel_count() {
    let mut w = GraphwarVideoWriter::new(4, 4, false);

    // RGB：4*4*3 = 48
    assert_eq!(w.add_frame(FrameFormat::Rgb, 4, 4, &[0u8; 48]), Ok(()));
    // RGBA：4*4*4 = 64
    assert_eq!(w.add_frame(FrameFormat::Rgba, 4, 4, &[0u8; 64]), Ok(()));
    assert_eq!(w.len(), 2);

    assert_eq!(
        w.add_frame(FrameFormat::Rgb, 4, 4, &[0u8; 47]),
        Err(FrameError::SizeMismatch)
    );
    assert_eq!(
        w.add_frame(FrameFormat::Rgba, 0, 4, &[]),
        Err(FrameError::Empty)
    );
    assert_eq!(w.len(), 2);

    assert_eq!(FrameFormat::Rgb.bytes_per_pixel(), 3);
    assert_eq!(FrameFormat::Rgba.bytes_per_pixel(), 4);
}

#[test]
fn timestamps_step_by_one_frame() {
    let mut w = GraphwarVideoWriter::new(2, 2, false);
    for _ in 0..3 {
        w.add_frame(FrameFormat::Rgb, 2, 2, &[1u8; 12]).unwrap();
    }
    let ts: Vec<u64> = w.frames().iter().map(|f| f.timestamp_ms).collect();
    assert_eq!(ts, vec![0, 16, 32]);
}

#[test]
fn write_video_needs_frames() {
    let w = GraphwarVideoWriter::new(8, 8, false);
    assert!(w.write_video("user://clip.mp4").is_none());
}

#[test]
fn write_video_describes_the_muxer_input() {
    let mut w = GraphwarVideoWriter::new(9, 7, false); // -> 10 x 8
    w.add_frame(FrameFormat::Rgb, 9, 7, &[0u8; 9 * 7 * 3]).unwrap();
    w.add_frame(FrameFormat::Rgb, 9, 7, &[0u8; 9 * 7 * 3]).unwrap();

    let job = w.write_video("user://replays/clip.mp4").unwrap();
    assert_eq!(job.path, "user://replays/clip.mp4");
    assert_eq!(job.width, 10);
    assert_eq!(job.height, 8);
    assert_eq!(job.fps, 60);
    assert_eq!(job.track_name, "Graphwar Clip");
    assert_eq!(job.frame_count, 2);
    assert_eq!(job.timestamps_ms, vec![0, 16]);
}
