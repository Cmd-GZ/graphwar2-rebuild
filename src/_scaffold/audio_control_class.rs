//! **不是原版的东西**：把 [`crate::graphwar_audio_control`] 里**已经复原的音频规则**
//! 接成 GDExtension 类 `GwAudioControl`，让 `client/src/audio/gw_audio.gd` 只留
//! "加载资源 / 建 player / 推 generator 缓冲 / 调 `AudioServer`"。
//!
//! # 为什么会有这一份（T58 / `docs/06` §90 的 C 档第 2 段）
//!
//! `client/src/audio/gw_audio.gd`（改前 400 行）里有四类**规则**在 Rust 侧早有逐位复原：
//!
//! | # | `.gd` 改前 | Rust 真值 | 证据 |
//! |---|---|---|---|
//! | 1 | `SLOT_VOLUME_DB` 五个音量 | `SoundSlot::volume_db()`（`-15/-15/-15/0/-5`） | 逐位：`fmov s0,#-15.0`、`movi d0,#0`、`movss 0x1b9ba0`（-5.0f） |
//! | 2 | `GEN_MIX_RATE`/`GEN_BUFFER_LENGTH`/`PCM_NORMALIZE` | `HUM_MIX_RATE`/`HUM_BUFFER_LENGTH`/`PCM_FULL_SCALE` | 逐位：`0x467a0000`=16000f、`0x3dcccccd`=0.1f、`0x46fffe00`=32767f |
//! | 3 | `SOUNDS` 里每个音效落在**哪个槽** | 构造顺序写 `+0x80/+0x90/+0xa0/+0xb0/+0xc0` = `explosion/squash/spark/pop/unpop` | 逐位：模块文档 §3 的"槽 ↔ 资源 ↔ 事件"三列表 |
//! | 4 | 静音 = `AudioServer.set_bus_mute(...)`（单标志） | 有效静音 = `+0xd0 ∥ +0xd1`（`orr w8,w19,w22`） | 逐位：`0x4c13a4`/`0x4c13c7` |
//!
//! ⚠️ **改前 `.gd` 的槽位表是错的**（五条里五条都不对）：
//!
//! | 音效 | `.gd` 改前 | 真值（`SoundSlot`） | 后果 |
//! |---|---|---|---|
//! | `explosion` | `0xa0` | `0x80` | 音量恰好同为 -15 dB ⇒ **听不出**，但槽位语义错 |
//! | `squash` | `0x80` | `0x90` | 同上 |
//! | `spark` | `0x90` | `0xa0` | 同上 |
//! | `pop` | `0xa0` | **`0xb0`** | 真值 `0 dB`，`.gd` 给 -15 dB ⇒ **音量差 15 dB** |
//! | `unpop` | `0x80` | **`0xc0`** | 真值 `-5 dB`，`.gd` 给 -15 dB ⇒ **差 10 dB** |
//! | `humm` | 被写成占 `0xc0` | **不在 5 槽里**（走 `AudioStreamGenerator`） | `0xc0` 真值是 `unpop.mp3` |
//!
//! # 接口
//!
//! ```gdscript
//! GwAudioControl.slot_names()            # ["0x80","0x90","0xa0","0xb0","0xc0"]
//! GwAudioControl.slot_path("0x80")       # "res://sounds/explosion.mp3"
//! GwAudioControl.slot_volume_db("0xb0")  # 0.0
//! GwAudioControl.slot_of_effect_kind(2)  # "0xa0"（0/1/其余 四臂落三槽）
//! GwAudioControl.hum_mix_rate()          # 16000.0
//! GwAudioControl.hum_buffer_length()     # 0.1
//! GwAudioControl.pcm_full_scale()        # 32767.0
//! GwAudioControl.hum_frames_per_buffer() # 1600
//! GwAudioControl.decode_wav(bytes)       # {ok, error, rate, channels, frames: PackedVector2Array}
//! GwAudioControl.effective_mute(user, hidden)   # user or hidden
//! GwAudioControl.provenance()            # 出处/档位（自检、文档复核用）
//! ```

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_audio_control as ac;
use crate::_scaffold::audio_wav;

/// 槽位名（GDScript 侧的字符串口径）→ [`ac::SoundSlot`]。
///
/// 名字取 `+0x80` 这种偏移写法（原版槽位就是**结构体偏移**，`SoundSlot::offset()`）。
/// 这个名字表不是原版的东西 —— 它只是 `.gd` 与 Rust 之间的词汇桥。
fn slot_by_name(name: &str) -> Option<ac::SoundSlot> {
    match name {
        "0x80" => Some(ac::SoundSlot::Explosion),
        "0x90" => Some(ac::SoundSlot::Squash),
        "0xa0" => Some(ac::SoundSlot::Spark),
        "0xb0" => Some(ac::SoundSlot::Pop),
        "0xc0" => Some(ac::SoundSlot::Unpop),
        _ => None,
    }
}

fn slot_name(slot: ac::SoundSlot) -> String {
    format!("0x{:02x}", slot.offset())
}

/// 音频规则出口（scaffold；原版整个结构体是 `GraphwarAudioControl`，0xf0 字节）。
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwAudioControl {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwAudioControl {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwAudioControl {
    /// 五个资源槽的名字（按构造/载入顺序：`explosion, squash, spark, pop, unpop`）。
    #[func]
    fn slot_names() -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for s in ac::SoundSlot::ALL {
            let name = GString::from(slot_name(s).as_str());
            out.push(&name);
        }
        out
    }

    /// 槽位装哪个资源文件（`res://` 路径，`.rodata` 原串）。
    #[func]
    fn slot_path(slot: GString) -> GString {
        match slot_by_name(&slot.to_string()) {
            Some(s) => GString::from(s.path()),
            None => GString::from(""),
        }
    }

    /// 槽位的音量（dB，**写死的立即数**，不是玩家可调项）。
    #[func]
    fn slot_volume_db(slot: GString) -> f64 {
        match slot_by_name(&slot.to_string()) {
            Some(s) => s.volume_db() as f64,
            // 未知槽：与原版"没这个队/没这个槽"的兜底同口径 —— 给特效槽的 -15.0。
            None => ac::EFFECT_VOLUME_DB as f64,
        }
    }

    /// `effect_info_event` 的 `kind` → 槽（四臂落三槽，逐位：`0→0x80`、`1→0x90`、
    /// `2` 与其余 → `0xa0`）。
    #[func]
    fn slot_of_effect_kind(kind: i64) -> GString {
        let k = kind.clamp(0, 255) as u8;
        GString::from(slot_name(ac::slot_for_effect_kind(k)).as_str())
    }

    /// `AudioStreamGenerator::set_mix_rate` 的实参（`16000.0f`，`0x467a0000`）。
    #[func]
    fn hum_mix_rate() -> f64 {
        ac::HUM_MIX_RATE as f64
    }

    /// `set_buffer_length` 的实参（`0.1f`，`0x3dcccccd`）。
    #[func]
    fn hum_buffer_length() -> f64 {
        ac::HUM_BUFFER_LENGTH as f64
    }

    /// i16 满量程归一化常量（`32767.0f`，`0x46fffe00`）。
    #[func]
    fn pcm_full_scale() -> f64 {
        ac::PCM_FULL_SCALE as f64
    }

    /// 一帧样本预算 = `mix_rate × buffer_length` = **1600**。
    #[func]
    fn hum_frames_per_buffer() -> i64 {
        ac::GraphwarAudioControl::hum_frames_per_buffer() as i64
    }

    /// 解析 `humm.wav`：`hound` 的等价物（见 [`crate::_scaffold::audio_wav`]）。
    ///
    /// 返回 `{ok, error, rate, channels, frames}`：`frames` 是**已归一化**的
    /// `PackedVector2Array`（单声道 ⇒ `(v, v)`，与 Godot generator 的帧形状一致）。
    #[func]
    fn decode_wav(bytes: PackedByteArray) -> VarDictionary {
        let mut d = VarDictionary::new();
        match audio_wav::decode_wav(bytes.as_slice()) {
            Ok(pcm) => {
                let mut frames = PackedVector2Array::new();
                frames.resize(pcm.samples.len());
                for (i, v) in pcm.samples.iter().enumerate() {
                    frames[i] = Vector2::new(*v, *v);
                }
                d.set("ok", true);
                d.set("error", &GString::from(""));
                d.set("rate", pcm.rate as i64);
                d.set("channels", pcm.channels as i64);
                d.set("frames", &frames);
            }
            Err(e) => {
                d.set("ok", false);
                d.set("error", &GString::from(e.message().as_str()));
                d.set("rate", 0i64);
                d.set("channels", 0i64);
                d.set("frames", &PackedVector2Array::new());
            }
        }
        d
    }

    /// **有效静音** = 用户静音 ∥ 隐式静音（`orr w8, w19, w22; and w2, w8, #1`）。
    ///
    /// 真正的"写到总线上"是 `.gd` 干（`AudioServer.set_bus_mute(get_bus_index("Master"), …)`，
    /// `0x4c13a4`/`0x4c13c7`）—— 那是引擎 API 调用，原版也调引擎。
    #[func]
    fn effective_mute(user: bool, hidden: bool) -> bool {
        let mut st = ac::GraphwarAudioControl::new();
        st.set_muted(user);
        st.set_hidden_muted(hidden);
        st.effective_mute()
    }

    /// 出处与档位（自检、文档复核用；客户端界面不读）。
    #[func]
    fn provenance() -> VarDictionary {
        let mut d = VarDictionary::new();
        for s in ac::SoundSlot::ALL {
            let key = GString::from(format!("slot_{}", slot_name(s)).as_str());
            d.set(
                &key,
                &GString::from(format!("{} @ {:.1} dB", s.path(), s.volume_db()).as_str()),
            );
        }
        d.set("struct_size", ac::GraphwarAudioControl::SIZE as i64);
        d.set("hum_mix_rate", ac::HUM_MIX_RATE as f64);
        d.set("hum_buffer_length", ac::HUM_BUFFER_LENGTH as f64);
        d.set("pcm_full_scale", ac::PCM_FULL_SCALE as f64);
        d.set("hum_frames_per_buffer", ac::GraphwarAudioControl::hum_frames_per_buffer() as i64);
        d.set("muted_offset", ac::MUTED_OFFSET as i64);
        d.set("hidden_muted_offset", ac::HIDDEN_MUTED_OFFSET as i64);
        d.set("master_bus", &GString::from(ac::MASTER_BUS));
        d.set(
            "evidence",
            &GString::from(
                "逐位：5 次 load() 0x4bf550 的写入顺序 = +0x80..+0xc0；音量 fmov #-15.0/#-5.0/\
                 movi d0,#0；generator 16000.0f/0.1f；归一化 0x4c08f6 = 32767.0f；\
                 静音 0x4c13a4/0x4c13c7 = set_bus_mute(get_bus_index(\"Master\"), arg|另一个标志)",
            ),
        );
        d.set("grade", &GString::from("逐位"));
        d
    }
}
