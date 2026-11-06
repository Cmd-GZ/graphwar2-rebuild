//! 音频：`GraphwarAudioControl`（原版节点类名 **`SoundHandler`**）。
//!
//! 本模块回答四件事：**订阅了哪些事件**、**每个事件播哪个音效**、**音量/静音怎么走**、
//! **`humm.wav` 那条流式链路是什么形状**。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测**（能指到地址或字节）/ **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。arm64 侧地址来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`（源码级函数名**就是符号名**），
//! x86 侧地址来自 `.eh_frame` FDE 函数边界 + `panic::Location` 锚（同 `docs/111`）。
//!
//! ## 1. 函数清单（全部已定位）
//!
//! | 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `process` | `0x4bfe10` / 5052 | `0x4c5040` / 4268 | 170 | 每帧驱动：`humm` 的 PCM 泵 | 观测 |
//! | `new` | `0x4bf550` / 2230 | `0x4c4964` / 1756 | 245 | 载 6 个资源 + 订阅 4 个事件 | 逐位 |
//! | `effect_info_event` | `0x4bf090` / 681 | `0x4c442c` / 712 | 328 | 按 `kind` 选槽入队 | 逐位 |
//! | `game_state_event` | `0x4bef20` / 212 | `0x4c42c8` / 212 | 351 | `state == 1` 清空队列 | 逐位 |
//! | `set_muted` | `0x4c11d0` / 578 | `0x4c60ec` / 548 | 362 | 写 `+0xd0`，Master 总线静音 | 观测 |
//! | `set_hidden_muted` | `0x4bf000` / 134 | `0x4c439c` / 144 | — | 写 `+0xd1`，Master 总线静音 | 逐位 |
//! | `soldier_info_event` | `0x4bf340` / 188 | `0x4c46f4` / 216 | — | 入队 `pop.mp3`（0 dB） | 逐位 |
//! | `entity_removed_event` | `0x4bf480` / 193 | `0x4c488c` / 216 | — | 入队 `unpop.mp3`（-5 dB） | 逐位 |
//! | `EffectAudioData::new` | `0x4bed70` / 423 | `0x4c4144` / 388 | — | 建一条待播记录（设 stream + 音量） | 逐位 |
//! | `subscribe_to_events` | 内联进 `new` | 4 个闭包 `0x4c47cc`… | — | 四条订阅的胶合层 | 逐位 |
//!
//! x86 的 5 个锚（`:170` `:245` `:328` `:351` `:362`）来自 `fixtures/name_to_addr.tsv`；
//! 剩下 4 个（`set_hidden_muted` / `soldier_info_event` / `entity_removed_event` /
//! `EffectAudioData::new`）**在 x86 侧无锚**，靠 arm64 符号名 + 尺寸比闸
//! （x86 ÷ arm64 = 1.07 / 1.15 / 1.12 / 1.09，全部落在 0.61–1.19 内）定名，
//! 且每个都有一处**独立字节证据**（见 §3、§4 各表）。
//!
//! > `docs/111` §4.2 把 x86 `0x4bed70` 记成"包住 `set_stream` 的辅助函数、签名含
//! > `player_ref/stream_ref/volume_db`"，**它就是 `EffectAudioData::new`** ——
//! > arm64 侧 6 个调用点（`effect_info_event` ×4、`soldier_info_event` ×1、
//! > `entity_removed_event` ×1）与文档里"6 次 `set_stream`"逐一对上。
//!
//! ## 2. 订阅：4 条（逐位）
//!
//! `new` 里 4 次 `Box::new(8)` + 4 次 `'static` 名串 + 4 个 `dyn FnMut` vtable，
//! 每个 vtable 的第 4 项（`call_mut`）再跳到真正的处理方法，两条独立链都解开了：
//!
//! | # | 事件名 | 长度立即数 | 判别值 | vtable | `call_mut` | 处理方法 |
//! |---|---|---|---|---|---|---|
//! | 1 | `effect_info_event` | `0x11` = 17 | 44 | `0x85b0f8` | `0x4d5434` | `effect_info_event` |
//! | 2 | `soldier_info_event` | `0x12` = 18 | 15 | `0x85b120` | `0x4d632c` | `soldier_info_event` |
//! | 3 | `entity_removed_event` | `0x14` = 20 | 14 | `0x85b148` | `0x4d560c` | `entity_removed_event` |
//! | 4 | `game_state_info_event` | `0x15` = 21 | 30 | `0x85b170` | `0x4d5f58` | `game_state_event` |
//!
//! 名字长度与串本身互相印证（17/18/20/21 字节），判别值与 [`crate`] 之外那份
//! `events.rs` 的 `Event` 表一致（`tests/graphwar_audio_control.rs` 会交叉断言）。
//!
//! ⚠️ **方法名叫 `game_state_event`，订阅的却是 `game_state_info_event`**（30）——
//! 别把方法名当成事件名。
//!
//! ## 3. 事件 → 音效（问题的正面答案）
//!
//! 三个处理方法都走同一个形状：`EffectAudioData::new(self, 玩家槽, 资源槽, 音量, 值)`
//! 再 push 进 `Vec<EffectAudioData>`。**资源槽是 `self` 的 5 个 16 字节字段**，
//! 它们在构造函数里按下表顺序写入（构造函数尾部 `0x5238c0` 的载荷初始化，
//! 源结构体字段 `+0x18/+0x28/+0x38/+0x48/+0x58` 依次搬到 `+0x80/+0x90/+0xa0/+0xb0/+0xc0`）：
//!
//! | 资源槽 | 资源 | 载入顺序 | 触发事件 | 音量立即数 |
//! |---|---|---|---|---|
//! | `+0x80` | `res://sounds/explosion.mp3` | 1 | `effect_info_event`，`kind == 0` | `-15.0f` |
//! | `+0x90` | `res://sounds/squash.mp3` | 2 | `effect_info_event`，`kind == 1` | `-15.0f` |
//! | `+0xa0` | `res://sounds/spark.mp3` | 3 | `effect_info_event`，`kind == 2` 或其余 | `-15.0f` |
//! | `+0xb0` | `res://sounds/pop.mp3` | 4 | `soldier_info_event`（15） | `0.0f` |
//! | `+0xc0` | `res://sounds/unpop.mp3` | 5 | `entity_removed_event`（14） | `-5.0f` |
//!
//! 语义自洽：士兵**出现**响 `pop`、**移除**响 `unpop`，三种命中特效响
//! `explosion`/`squash`/`spark`。`humm.wav`（第 6 个资源）不走这 5 个槽，
//! 它由 `AudioStreamGenerator` 流式播放（§5）。
//!
//! 音量立即数在 arm64 上是 `fmov s0, #-15.0` / `#-5.0` / `movi d0, #0`，
//! 在 x86 上是 `movss 0x1bb818`(-15.0f) / `movss 0x1b9ba0`(-5.0f)（`docs/111` §4.5
//! 记的 `0x1b9ba0` 就在 `entity_removed_event`=x86 `0x4bf480` 里）。
//! **`docs/111` §4.5 把 `-5.0f` 归给 `humm` 那一路是不成立的**：`0xc0` 槽装的是
//! `unpop.mp3`，`humm` 走的是 generator（资源槽之外）。
//!
//! ## 4. `effect_info_event` 的分支（逐位）
//!
//! 先断言事件判别值 `0x800000000000002c` = 44，再按 **payload `+0x20` 的 u8** 分四臂：
//! `0 → +0x80`、`1 → +0x90`、`2 → +0xa0`、其余 → `+0xa0`（四臂落三槽），
//! 载荷的 **`+0x1c` u32** 原样带进 `EffectAudioData` 的值字段。
//!
//! `soldier_info_event` / `entity_removed_event` **不校验判别值**（arm64 与 x86 都是），
//! 值字段固定 `0`，音量分别 `0.0f` / `-5.0f`。
//!
//! ## 5. 队列、静音、`humm` 流
//!
//! * **队列**：`Vec<EffectAudioData>` 在 `self + 0x00`，字段顺序 **cap / ptr / len**
//!   = `+0x00 / +0x08 / +0x10`（`docs/112` §3.1 已定过同一布局）；元素 **0x18 = 24 字节**
//!   （`mov w8,#0x18; madd x8, len, x8, ptr`）：`{ +0x00 Option<Gd> (16), +0x10 u32 值, +0x14 bool 已启动 }`。
//! * **清空**：`game_state_event` 读到 payload `+0x8` 的 u8 **等于 1** 时把 `len` 置 0 并逐个
//!   清理（`str xzr,[self+0x10]` 后按保存的旧 len 遍历）。
//! * **静音**：`+0xd0` = 用户静音（`set_muted` 写），`+0xd1` = 隐式静音
//!   （`set_hidden_muted` 写，`docs/111` §5.3 悬着的那个字节就是它）。
//!   两者都**只做整条 `Master` 总线静音**：`set_bus_mute(get_bus_index("Master"), arg | 另一个标志)`。
//! * **`humm`**：`hound` 解析 → `AudioStreamGenerator`（`set_mix_rate(16000.0f)`、
//!   `set_buffer_length(0.1f)`）→ `push_buffer`；归一化 `32767.0f`（i16 满量程）。
//!   一帧的样本预算 = `16000 * 0.1 = 1600`。
//!
//! ## 6. 仍未确定
//!
//! * `self + 0x60` 那个 16 字节 `Option<Gd<…>>`：`EffectAudioData::new` 用它对
//!   `AudioStreamPlayer` 侧做 `set_stream` / `set_volume_db`（绑定函数的字符串簇是
//!   `AudioStreamPlayer is_playing set_volume_db get_stream_playback`），但它在构造
//!   路径里被写成 `([rsp], rbp)`，第二字为 0 ⇒ **它的具体对象身份标推断**。
//! * `process`（4268 B）里 `EffectAudioData` 记录的**播放时机**（`play()` 的调用点）
//!   没有逐指令闭合：`docs/111` §4.4 把 x86 `0x4c01b9`/`0x4c0d…` 记在 `new` 名下，
//!   实际它们落在 `process`（`0x4bfe10` 起 5052 B）里。**标推断**。
//! * `tear.mp3` / `humm_2` / `humm_b` / `shepard_2` 依旧没有任何引用（`docs/111` §5.2 的负结果仍成立）。

/// `res://sounds/explosion.mp3`（`.rodata` 里无 NUL，长度 26）。
pub const SFX_EXPLOSION: &str = "res://sounds/explosion.mp3";
/// `res://sounds/squash.mp3`（长度 23）。
pub const SFX_SQUASH: &str = "res://sounds/squash.mp3";
/// `res://sounds/spark.mp3`（长度 22）。
pub const SFX_SPARK: &str = "res://sounds/spark.mp3";
/// `res://sounds/pop.mp3`（长度 20）。
pub const SFX_POP: &str = "res://sounds/pop.mp3";
/// `res://sounds/unpop.mp3`（长度 22）。
pub const SFX_UNPOP: &str = "res://sounds/unpop.mp3";
/// `res://sounds/humm.wav`（长度 21，Rust 侧自己读 + hound 解析）。
pub const SFX_HUMM: &str = "res://sounds/humm.wav";

/// 五个一次性音效的播放槽（`self` 内的字节偏移）与属性。
///
/// 判别值就是偏移：构造函数把资源按 `explosion, squash, spark, pop, unpop` 的顺序
/// 写进 `+0x80/+0x90/+0xa0/+0xb0/+0xc0`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SoundSlot {
    /// `effect_info_event` 的 `kind == 0`。
    Explosion = 0x80,
    /// `effect_info_event` 的 `kind == 1`。
    Squash = 0x90,
    /// `effect_info_event` 的 `kind >= 2`（含未知值）。
    Spark = 0xa0,
    /// `soldier_info_event`(15)。
    Pop = 0xb0,
    /// `entity_removed_event`(14)。
    Unpop = 0xc0,
}

impl SoundSlot {
    /// 该槽在 `self` 里的字节偏移（= 判别值）。
    pub const fn offset(self) -> usize {
        self as usize
    }

    /// 该槽装哪个资源文件。
    pub const fn path(self) -> &'static str {
        match self {
            SoundSlot::Explosion => SFX_EXPLOSION,
            SoundSlot::Squash => SFX_SQUASH,
            SoundSlot::Spark => SFX_SPARK,
            SoundSlot::Pop => SFX_POP,
            SoundSlot::Unpop => SFX_UNPOP,
        }
    }

    /// 入队时对该槽设的音量（dB）。
    ///
    /// 三个特效槽 `-15.0`、`pop` 是 `0.0`、`unpop` 是 `-5.0`。
    /// 注意：**不是**玩家可调项，是写死的立即数（`docs/111` §4.5）。
    pub const fn volume_db(self) -> f32 {
        match self {
            SoundSlot::Explosion | SoundSlot::Squash | SoundSlot::Spark => EFFECT_VOLUME_DB,
            SoundSlot::Pop => SOLDIER_VOLUME_DB,
            SoundSlot::Unpop => UNPOP_VOLUME_DB,
        }
    }

    /// 五个槽，按构造顺序（= 载入顺序）。
    pub const ALL: [SoundSlot; 5] = [
        SoundSlot::Explosion,
        SoundSlot::Squash,
        SoundSlot::Spark,
        SoundSlot::Pop,
        SoundSlot::Unpop,
    ];

    /// 相邻两个槽的间距（字节）。
    pub const STRIDE: usize = 0x10;
}

/// 三个特效槽共用的音量：`fmov s0, #-15.0` / x86 `movss 0x1bb818`。
pub const EFFECT_VOLUME_DB: f32 = -15.0;
/// `soldier_info_event` 那一路：arm64 `movi d0, #0`（0 dB）。
pub const SOLDIER_VOLUME_DB: f32 = 0.0;
/// `entity_removed_event` 那一路：`fmov s0, #-5.0` / x86 `movss 0x1b9ba0`。
pub const UNPOP_VOLUME_DB: f32 = -5.0;

/// `AudioStreamGenerator::set_mix_rate` 的实参：`0x467a0000` = `16000.0f`。
pub const HUM_MIX_RATE: f32 = 16000.0;
/// `set_buffer_length` 的实参：`0x3dcccccd` = `0.1f`。
pub const HUM_BUFFER_LENGTH: f32 = 0.1;
/// i16 满量程归一化常量 `0x46fffe00` = `32767.0f`。
pub const PCM_FULL_SCALE: f32 = 32767.0;
/// 一个生成器缓冲里的样本预算：`mix_rate * buffer_length`。
pub const HUM_FRAMES_PER_BUFFER: usize = 1600;

/// 事件判别值：`effect_info_event`（订阅 1）。
pub const EVENT_EFFECT_INFO: u16 = 44;
/// 事件判别值：`soldier_info_event`（订阅 2）。
pub const EVENT_SOLDIER_INFO: u16 = 15;
/// 事件判别值：`entity_removed_event`（订阅 3）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;
/// 事件判别值：`game_state_info_event`（订阅 4）。
pub const EVENT_GAME_STATE_INFO: u16 = 30;

/// 一条订阅（事件名 + 长度 + 判别值 + 处理方法）。
#[derive(Debug, Clone, Copy)]
pub struct Subscription {
    /// `.rodata` 里被比较的事件名（snake_case）。
    pub name: &'static str,
    /// 事件判别值（`0x8000_0000_0000_0000 | tag`）。
    pub tag: u16,
    /// 处理方法的名字（原版 `impl GraphwarAudioControl` 的方法名）。
    pub handler: &'static str,
}

/// `GraphwarAudioControl::new` 里的四条订阅，按**代码顺序**（逐位）。
pub const SUBSCRIPTIONS: [Subscription; 4] = [
    Subscription { name: "effect_info_event", tag: EVENT_EFFECT_INFO, handler: "effect_info_event" },
    Subscription { name: "soldier_info_event", tag: EVENT_SOLDIER_INFO, handler: "soldier_info_event" },
    Subscription { name: "entity_removed_event", tag: EVENT_ENTITY_REMOVED, handler: "entity_removed_event" },
    Subscription { name: "game_state_info_event", tag: EVENT_GAME_STATE_INFO, handler: "game_state_event" },
];

/// `effect_info_event` 的载荷字段偏移：u8 特效种类。
pub const EFFECT_INFO_KIND_OFFSET: usize = 0x20;
/// `effect_info_event` 的载荷字段偏移：u32 值（原样进 [`EffectAudioData::value`]）。
pub const EFFECT_INFO_VALUE_OFFSET: usize = 0x1c;
/// `game_state_info_event` 的载荷字段偏移：u8 状态。
pub const GAME_STATE_OFFSET: usize = 0x08;
/// `game_state_event` 里"清空队列"的那个状态值。
pub const GAME_STATE_CLEAR: u8 = 1;

/// `Vec<EffectAudioData>` 在 `self` 里的字段偏移（cap 在前，`docs/112` §3.1）。
pub const QUEUE_CAP_OFFSET: usize = 0x00;
/// 同上：数据指针。
pub const QUEUE_PTR_OFFSET: usize = 0x08;
/// 同上：长度。
pub const QUEUE_LEN_OFFSET: usize = 0x10;

/// 用户静音标志 `self + 0xd0`（`set_muted` 写）。
pub const MUTED_OFFSET: usize = 0xd0;
/// 隐式静音标志 `self + 0xd1`（`set_hidden_muted` 写）。
pub const HIDDEN_MUTED_OFFSET: usize = 0xd1;

/// 主总线名（`set_bus_mute(get_bus_index("Master"), …)`，长度 6）。
pub const MASTER_BUS: &str = "Master";

/// 一条待播记录：`EffectAudioData`，**24 = 0x18 字节**。
///
/// 内存形状（逐位，`madd x8, len, 0x18, ptr` + `stp q0, [entry]` + `str wX, [entry+0x10]`）：
///
/// ```text
/// +0x00  16 字节  Option<Gd<…>>   播放器/流句柄（play 侧用）
/// +0x10  u32      值               effect_info 的 payload+0x1c；其余两个事件恒 0
/// +0x14  bool     已启动          构造时 false
/// ```
///
/// 句柄本身不进本文件（这里只复原**可判定的数据结构**）；[`EffectAudioData::handle_slot`]
/// 记录它来源自哪个资源槽，足以钉住"这条记录该放哪个音"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectAudioData {
    /// 句柄来源的资源槽（= 该条记录对应的音效）。
    pub slot: SoundSlot,
    /// payload 里的 u32 值（`effect_info` 的 `+0x1c`）。
    pub value: u32,
    /// 是否已启动（`+0x14`，构造时为 `false`）。
    pub started: bool,
}

impl EffectAudioData {
    /// 元素大小（字节），逐位：`mov w8, #0x18; madd x8, len, x8, ptr`。
    pub const SIZE: usize = 0x18;
    /// 值字段偏移。
    pub const VALUE_OFFSET: usize = 0x10;
    /// 已启动标志偏移。
    pub const STARTED_OFFSET: usize = 0x14;

    /// 构造（`EffectAudioData::new`，x86 `0x4bed70` / arm64 `0x4c4144`）。
    ///
    /// 原版这里还会 `set_stream(资源槽)` + `set_volume_db(槽的音量)`；本实现只留
    /// 可判定的部分：槽、值、未启动。
    pub const fn new(slot: SoundSlot, value: u32) -> Self {
        Self { slot, value, started: false }
    }

    /// 该记录对应的资源路径。
    pub const fn path(&self) -> &'static str {
        self.slot.path()
    }

    /// 该记录应设的音量（dB）。
    pub const fn volume_db(&self) -> f32 {
        self.slot.volume_db()
    }
}

/// `effect_info_event` 的 `kind → 槽` 映射（四臂落三槽，逐位）。
///
/// `0 → Explosion`、`1 → Squash`、`2` 与**其余所有值** → `Spark`。
pub const fn slot_for_effect_kind(kind: u8) -> SoundSlot {
    match kind {
        0 => SoundSlot::Explosion,
        1 => SoundSlot::Squash,
        _ => SoundSlot::Spark,
    }
}

/// `GraphwarAudioControl` 的可判定状态。
///
/// 原版整个结构体 **0xf0 = 240 字节**（构造助手 `0x5238c0` 的 `alloc(0xf0, 8)`），
/// 头部是 `Vec<EffectAudioData>`，5 个资源槽在 `+0x80…+0xc0`，两个静音字节在
/// `+0xd0/+0xd1`。这里只保留"能观测到行为"的那几项。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarAudioControl {
    /// 待播队列（`Vec<EffectAudioData>`，`self + 0x00`）。
    pub queue: Vec<EffectAudioData>,
    /// 用户静音（`self + 0xd0`）。
    pub muted: bool,
    /// 隐式静音（`self + 0xd1`）。
    pub hidden_muted: bool,
}

impl GraphwarAudioControl {
    /// 结构体大小（逐位：`alloc(0xf0, 8)`）。
    pub const SIZE: usize = 0xf0;

    /// 空状态；`new` 还会载 6 个资源并订阅 4 条事件（见 [`SUBSCRIPTIONS`]）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 队列里待播的条数。
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// `effect_info_event`：按 `kind` 选槽，值原样带入。
    pub fn on_effect_info(&mut self, kind: u8, value: u32) -> EffectAudioData {
        let data = EffectAudioData::new(slot_for_effect_kind(kind), value);
        self.queue.push(data);
        data
    }

    /// `soldier_info_event`：`pop`，值恒 0，音量 `0.0 dB`。
    pub fn on_soldier_info(&mut self) -> EffectAudioData {
        let data = EffectAudioData::new(SoundSlot::Pop, 0);
        self.queue.push(data);
        data
    }

    /// `entity_removed_event`：`unpop`，值恒 0，音量 `-5.0 dB`。
    pub fn on_entity_removed(&mut self) -> EffectAudioData {
        let data = EffectAudioData::new(SoundSlot::Unpop, 0);
        self.queue.push(data);
        data
    }

    /// `game_state_event`：`state == 1` 时清空队列，返回丢弃条数。
    ///
    /// 其它状态什么都不做（`+0x8` 的 u8 只与 1 比较一次）。
    pub fn on_game_state_info(&mut self, state: u8) -> usize {
        if state != GAME_STATE_CLEAR {
            return 0;
        }
        let dropped = self.queue.len();
        self.queue.clear();
        dropped
    }

    /// 有效静音 = 两个标志相或（`orr w8, w19, w22; and w2, w8, #1`）。
    pub fn effective_mute(&self) -> bool {
        self.muted | self.hidden_muted
    }

    /// `set_muted(mute)`：写 `+0xd0`，再用 `mute | hidden` 设总线。
    pub fn set_muted(&mut self, mute: bool) -> bool {
        self.muted = mute;
        self.effective_mute()
    }

    /// `set_hidden_muted(hidden)`：写 `+0xd1`，再用 `hidden | muted` 设总线。
    pub fn set_hidden_muted(&mut self, hidden: bool) -> bool {
        self.hidden_muted = hidden;
        self.effective_mute()
    }

    /// `humm` 生成器一帧要推的样本数（`mix_rate * buffer_length`）。
    pub const fn hum_frames_per_buffer() -> usize {
        HUM_FRAMES_PER_BUFFER
    }

    /// i16 PCM 样本按满量程归一化到 `[-1, 1]`（原版 `push_buffer` 前的 `32767.0f` 除法）。
    pub fn normalize_pcm(sample: i16) -> f32 {
        f32::from(sample) / PCM_FULL_SCALE
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn slot_offsets_are_the_five_fields() {
        assert_eq!(SoundSlot::ALL.map(|s| s.offset()), [0x80, 0x90, 0xa0, 0xb0, 0xc0]);
        for w in SoundSlot::ALL.windows(2) {
            assert_eq!(w[1].offset() - w[0].offset(), SoundSlot::STRIDE);
        }
    }

    #[test]
    fn queue_element_is_24_bytes() {
        assert_eq!(EffectAudioData::SIZE, 0x18);
        assert_eq!(core::mem::size_of::<u32>() + core::mem::size_of::<bool>(), 5);
    }

    #[test]
    fn kind_to_slot_matches_the_four_arms() {
        assert_eq!(slot_for_effect_kind(0), SoundSlot::Explosion);
        assert_eq!(slot_for_effect_kind(1), SoundSlot::Squash);
        assert_eq!(slot_for_effect_kind(2), SoundSlot::Spark);
        assert_eq!(slot_for_effect_kind(3), SoundSlot::Spark);
        assert_eq!(slot_for_effect_kind(255), SoundSlot::Spark);
    }
}
