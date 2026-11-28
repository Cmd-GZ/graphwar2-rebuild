//! `effect_drawer.rs` — 原版命中 / 爆炸特效（`EffectDrawer`）的**图元 + 状态机**级复原。
//!
//! 本模块**不调用任何 Godot API**：它把二进制里读出来的几何 / 颜色 / 线宽 /
//! 时间常数表达成"要画什么 + 什么时刻推进成什么"，由客户端或 GDExtension 照着做。
//!
//! # 1. 函数清单（2.0.60 两侧都有物证）
//!
//! x86 是 strip 的，名字来自 **arm64 2.0.60 的 `.symtab`**（`readelf -sW` 全量）：
//!
//! | 原版符号（arm64） | arm64 vaddr / size | x86 vaddr（本模块引用） | 档位 |
//! |---|---|---|---|
//! | `EffectData::new` | `0x44486c` / 1216 | **未知**（见 §5） | 观测（只有 arm64 名字） |
//! | `EffectDrawer::step_effect` | `0x444d2c` / 992 | **`0x4eb450`** | 观测（4 条常量对拍，见 §3） |
//! | `EffectDrawer::draw` | `0x43c578` / 3356 | **`0x4e1520`**（`0x4e1520..=0x4e25af`） | 观测 |
//! | `EffectDrawer::ready` | `0x43d294` / 1204 | **`0x4e25b0`**（`0x4e25b0..=0x4e2a2f`） | 观测 |
//! | `ready::{{closure}}`（`tick_event`） | `0x43d930` / 708 | **`0x4e2a30`**（`0x4e2a30..=0x4e2d4f`） | 观测 |
//! | `ready::{{closure}}`（`effect_info_event`） | `0x43d748` / 488 | **`0x4e2d50`**（`0x4e2d50..=0x4e2f7f`） | 观测 |
//! | `EffectDrawer::process` | `0x43dbf4` / 3004 | **`0x4e2f80`**（`0x4e2f80..=0x4e3acf`） | 观测 |
//!
//! `fixtures/name_to_addr.tsv` 里 `effect_drawer` 有 3 行同名的 `ready`——那不是
//! 三个 `ready` 方法，而是 `ready` **本体 + 它注册的两个闭包**（桥表把
//! `{{closure}}` 后缀剥掉了）：arm64 `.symtab` 明确给出
//! `ready`(1204) + `ready::{{closure}}`(488) + `ready::{{closure}}`(708)。
//!
//! # 2. 它画什么（`draw`，x86 `0x4e1520`）
//!
//! 一个 `draw` = 1 次 `draw_set_transform` + 每个元素按"相位字节"选一种图元：
//!
//! | 原语 | 调用点 | 档位 |
//! |---|---|---|
//! | `draw_set_transform(pos = size/2, rot = 0, scale = (W/50, ±W/50))` | `0x4e17c0` | 观测 |
//! | `draw_circle` ×4 | `0x4e1a23` `0x4e1ef2` `0x4e1fd2` `0x4e2313` | 观测 |
//! | `draw_polygon` ×1 | `0x4e1d8f` | 观测 |
//! | `draw_line` ×2 | `0x4e2104` `0x4e2234` | 观测 |
//!
//! **不含** `draw_texture_rect`（士兵贴图 / 脸 / 帽子）与 `draw_arc`（准备圈）——
//! 全 `.text` 扫描里本函数对这两者的调用点数 = 0，它们分别是
//! `entity_drawer`（`0x4b2890` / `0x4e5400`）与 `0x4b2890` 的事。
//!
//! 相位字节 `[elem+0x28]` 经 `.rodata` 跳表 `0x1c3b8c`（`movsxd rax,[r13+rax*4];
//! add rax,r13; jmp rax` @ `0x4e1932`）分发到 **7 个 case**（表项 0..=6 都落在
//! 本函数体内；7 项以后落到别的函数，说明枚举只有 7 个值）：
//!
//! | 相位 | case 入口 | 图元 | 半径 | 颜色 | 证据 |
//! |---|---|---|---|---|---|
//! | 0 | `0x4e1950` | `draw_circle` | `0.05`（`0x1ba860`） | `(0.6, 0.6, 0.1, α)`（`0x19c930` + 立即数 `0x3dcccccd`） | `0x4e19e1..0x4e1a23` |
//! | 1 | `0x4e1a30` | `draw_polygon` | — | 顶点由 `×0.2` 的两组点构成 | `0x4e1ab8`/`0x4e1aed`/`0x4e1d8f` |
//! | 2 | `0x4e1e20` | `draw_circle` | `0.06`（`0x1b9e98`） | `(0.7, 0.7, 0.0, α)`（`0x1bb820`） | `0x4e1eb1..0x4e1ef2` |
//! | 3 | `0x4e1f00` | `draw_circle` | `0.06` | `(0.7, 0.7, 0.0, α)` | `0x4e1f91..0x4e1fd2` |
//! | 4 | `0x4e1fe0` | `draw_line` | — | `(0.9, 0.9, 0.1, α)`（`0x19c450`） | `0x4e20c9..0x4e2104` |
//! | 5 | `0x4e2110` | `draw_line` | — | `(0.9, 0.9, 0.1, α)` | `0x4e21f9..0x4e2234` |
//! | 6 | `0x4e2240` | `draw_circle` | `0.06` | `(0.1, 0.1, 0.1, 0.7)`（`0x19c440`） | `0x4e22d1..0x4e2313` |
//!
//! 圆 / 线的 `filled = true`、`width = -1.0`、`antialiased = false`；
//! 线的宽度参数 `xmm4 = -1.0`（槽 `0x1b9d08`）。
//!
//! # 3. 时长与时间单位（**本轮最硬的新结论**）
//!
//! 特效**不用毫秒、不用秒，用 u32 tick**，而且 tick 与秒的换算是 **60 tick = 1 秒**：
//!
//! | 量 | 值 | 证据 |
//! |---|---|---|
//! | tick/秒 | `60.0f64`（槽 `0x19f918`） | `mulsd` @ `0x4eb481`（`step_effect`）、`0x4eb5b4` |
//! | 固定子步长 | `1/600 s`（`0x3f5b4e81b4e81b4f`，槽 `0x19ed20`/`0x19ed28`/`0x19f9f0`） | `mulpd` @ `0x4eb56f`、`mulsd` @ `0x4eb69e`；**全二进制只有 `step_effect` 用这个常量** |
//! | 相位推进量 | `0x258` = **600 tick** | `add wN,wN,#0x258` @ arm64 `0x444da4`/`0x444e44`/`0x444f84`/`0x444fc4`/`0x4450d4`；x86 `add eax,0x258` @ `0x4eb42a`/`0x4eb530` 等 |
//! | tick 事件里的 delta | `1/60 s`（`f64 0.016666666666666666`，槽 `0x19f2a8`） | `0x4e2a30`（`tick_event` 闭包）引用它，然后调 `step_effect` |
//! | 元素字段 | `+0x20` = `start_tick: u32`、`+0x24` = `duration_ticks: u32` | `0x4e1950`：`ecx=[elem+0x20]; eax=[elem+0x24]; edx=eax+ecx; cmp edx,r15d`（`r15d` = 当前 tick，`[rax+0x58]`） |
//!
//! ⇒ **一个特效的存活期由 `EffectData` 自己的 `duration_ticks` 决定**（`draw` 里
//! `now > start+duration` 就整个不画），`step_effect` 推进的 600 tick 是**相位**节拍。
//! `EffectData::new` 里那两个字段被填成多少 = **未知**（见 §5）。
//!
//! # 4. `ready` / `process`（生命周期接线）
//!
//! `ready`（`0x4e25b0`）只做两件事（`0x4e25b0` + 两次 `__rust_alloc(0x18, 8)`）：
//!
//! ```text
//! 0x4e2802  lea rsi,[0x1c4225]  mov edx,0x11  ; "effect_info_event"（17 字节）
//! 0x4e2809  lea r8,[0x95fb50]                 ; 该闭包的 vtable
//! 0x4e281b  call *0x9bb030                    ; GraphwarBackend::subscribe_to_event
//! 0x4e2893  lea rsi,[0x1c4236]  mov edx,0xa   ; "tick_event"（10 字节）
//! 0x4e289a  lea r8,[0x95fb78]                 ; 另一个 vtable
//! 0x4e28ac  call *0x9bb030
//! ```
//!
//! 两张 vtable 的实参函数指针（`.rela.dyn` 的 `R_X86_64_RELATIVE` addend）：
//! `0x95fb70 → 0x4e2d50`（`effect_info_event` 处理）、`0x95fb98 → 0x4e2a30`
//! （`tick_event` 处理 → 调 `step_effect`）。**观测**。
//!
//! `process`（`0x4e2f80`）里出现的是
//! `std::time::Instant::now` / `elapsed`、`std::io::stdio::_print`、
//! `graphwarrust::time_probe::TimeProbe` 的 drop、`GraphwarModuleContainer::get_module`、
//! `Vec::retain`、`CanvasItem::queue_redraw`，以及一段 `cmp dword ptr [r12+0x40], 0x3b9aca00`
//! （`0x346f`，`0x3b9aca00` = **1e9 ns = 1 s**，是"这一段慢于 1 秒才打印"的探针阈值，
//! 释放时打印的位置串是 `.rodata` `0x1c4240` 的 `"src/effect_drawer.rs:507"`，长 24）。
//!
//! # 5. 明确未知（不许写成结论）
//!
//! * `EffectData::new` 的 x86 地址：**未知**。arm64 里它只有 1 个调用者
//!   （`effect_info_event` 闭包）且已被内联，x86 侧没有 `call rel32` 指向它；
//!   arm64 版用了 `rand::random` 与 π，说明它是**带随机初值**的构造函数。
//! * 相位 1（`draw_polygon`）的**顶点公式**：只读到两组点各乘 `0.2`（`0x19c460`）
//!   后进 `PackedVector2Array`，逐点的三角函数/排布**未知**。
//! * 相位 4/5 的**线段端点**：颜色与线宽已定，端点的几何公式未逐条读。
//! * `draw` 里 `self+0x1f8` 的位掩码（`and 0x101` / `test 0x10000` / `cmp 0x1000000`）
//!   决定 y 方向缩放的正负；三个标志的**语义未知**（推断：与 y 轴朝向 / 函数模式有关）。
//! * `EffectData` 的 `+0x2c` 起的尾部 4 字节（元素 0x30 字节，已用 0x2c）**未知**。

// ---------------------------------------------------------------------------
// 地址表
// ---------------------------------------------------------------------------

/// `EffectDrawer::draw` 的 x86 虚地址（Ghidra = `0x5e1520`）。体长 `0x1090`。
pub const DRAW_VADDR: u64 = 0x4e1520;
/// `EffectDrawer::ready` 的 x86 虚地址（Ghidra = `0x5e25b0`）。体长 `0x480`。
pub const READY_VADDR: u64 = 0x4e25b0;
/// `tick_event` 闭包（`ready::{{closure}}`，arm64 `0x43d930`）。体长 `0x320`。
pub const TICK_CLOSURE_VADDR: u64 = 0x4e2a30;
/// `effect_info_event` 闭包（`ready::{{closure}}`，arm64 `0x43d748`）。体长 `0x230`。
pub const INFO_CLOSURE_VADDR: u64 = 0x4e2d50;
/// `EffectDrawer::process` 的 x86 虚地址（Ghidra = `0x5e2f80`）。体长 `0xb50`。
pub const PROCESS_VADDR: u64 = 0x4e2f80;
/// `EffectDrawer::step_effect` 的 x86 虚地址（Ghidra = `0x5eb450`）。
/// Ghidra 把它的跳表分支切成了 0x4eb52e/0x4eb559/… 等"函数"，**那些都是本函数体内**。
pub const STEP_EFFECT_VADDR: u64 = 0x4eb450;
/// `step_effect` 的真实体长（到 `0x4eb81c` 的共同出口为止）。
pub const STEP_EFFECT_LEN: u64 = 0x3cc;

/// arm64 侧同名函数地址（`.symtab` 有名字，可用来复核）。
pub const DRAW_ARM64_VADDR: u64 = 0x43c578;
/// arm64 `step_effect`。
pub const STEP_EFFECT_ARM64_VADDR: u64 = 0x444d2c;
/// arm64 `EffectData::new`（x86 侧未知，见模块文档 §5）。
pub const EFFECT_DATA_NEW_ARM64_VADDR: u64 = 0x44486c;

/// 类名字面量（`downcast` 用）：槽 `0x1c1cf5`，长 **41**（`0x4e2e56` 的 `movq $0x29`）。
pub const CLASS_NAME: &str = "graphwarrust::effect_drawer::EffectDrawer";

/// `ready` 里订阅的两个事件名（**观测**：`0x1c4225` 长 17、`0x1c4236` 长 10）。
pub const SUBSCRIBED_EVENTS: [&str; 2] = ["effect_info_event", "tick_event"];

/// `process` 里 `TimeProbe` 的慢路径阈值：`0x3b9aca00` ns = 1 秒（`0x4e346f`）。**观测**
pub const TIME_PROBE_THRESHOLD_NS: u32 = 1_000_000_000;
/// `process` 打印的探针位置串：`.rodata` `0x1c4240`，长 **24**（`0x4e2fdf` 的 `movq $0x18`）。**观测**
pub const TIME_PROBE_LOCATION: &str = "src/effect_drawer.rs:507";

// ---------------------------------------------------------------------------
// 时间常数（§3）
// ---------------------------------------------------------------------------

/// tick / 秒。槽 `0x19f918`（`00 00 00 00 00 80 4e 40`），`mulsd` @ `0x4eb481` / `0x4eb5b4`。**逐位**
pub const TICKS_PER_SECOND: f64 = 60.0;

/// 物理子步长（秒）。`0x3f5b4e81b4e81b4f` = **1/600**；三个副本 `0x19ed20`、`0x19ed28`、
/// `0x19f9f0`。`mulpd`（两条 f64 一起乘）@ `0x4eb56f`，`mulsd` @ `0x4eb69e`。
///
/// 三步同时在用：`mulsd 60.0`（把 delta 换成 tick）、`mulsd 1/600`（积分子步）、
/// 两者之比 = 每 tick 10 个子步 ⇒ **600 Hz 的固定步长积分**。**逐位**
pub const FIXED_STEP_SECONDS: f64 = 0.001_666_666_666_666_666_8;

/// 相位推进的 tick 数 `0x258` = 600。**逐位**
pub const PHASE_TICKS: u32 = 660_u32 - 60; // 写成算式以强调它不是十进制圆整值

/// 状态字节的"终态位掩码"：`mov ecx,0x5b`（`0x4eb4bb`）/ `mov w11,#0x5b`（arm64 `0x444d94`）。
///
/// `bt ecx, edx` 后 `jb` 跳过的相位 = `{0, 1, 3, 4, 6}`；能推进的是 `{2, 5, 7…}`。
/// **观测（指令）/ 语义为推断**：为什么这 5 个是终态，缺的是原始 `match` 的分支名。
pub const STATE_TERMINAL_MASK: u32 = 0x5b;

/// `tick_event` 闭包交给 `step_effect` 的 delta（秒）= `1/60`。槽 `0x19f2a8`。**逐位**
pub const TICK_DELTA_SECONDS: f64 = 0.016_666_666_666_666_666;

/// 透明度上限 / 衰减系数 `0.9f32`：槽 `0x1b9534`（`66 66 66 3f`），
/// `mulss` @ `0x4e19a6`（`alpha = (1-t) * 0.9`）、`0x4e1bdd`（`alpha = (1 - t/0.9) * 0.9`）。
pub const ALPHA_GAIN: f32 = 0.9;
/// 相位 1 的淡出分母 `0.9`：`divss xmm0, [0x1b9534]` @ `0x4e1bcd`。**逐位**
pub const POLYGON_FADE_DIVISOR: f32 = 0.9;

// ---------------------------------------------------------------------------
// 图元常数
// ---------------------------------------------------------------------------

/// 相位 0 的圆半径 `0.05f32`：槽 `0x1ba860`，`movss` @ `0x4e1a07`。
pub const RADIUS_SMALL: f32 = 0.05;
/// 相位 2/3/6 的圆半径 `0.06f32`：槽 `0x1b9e98`，`movss` @ `0x4e1ed6` / `0x4e1fb6` / `0x4e22f7`。
pub const RADIUS_LARGE: f32 = 0.06;
/// `draw_*` 的 `width` 参数 `-1.0f32`（槽 `0x1b9d08`），每条圆/线都传。
pub const WIDTH_UNUSED: f32 = -1.0;
/// 所有 `draw_*` 的 `antialiased` 恒为 `false`（`xor %ecx,%ecx`）。
pub const ANTIALIASED: bool = false;
/// 绘制变换的缩放分母 `50.0f32`（槽 `0x1b9fc8`，`divss` @ `0x4e1743`）——与 `axis_drawer` 同一个常量。
pub const SCALE_DIVISOR: f32 = 50.0;
/// 居中系数 `0.5f32`（槽 `0x1ba128`，`mulss` @ `0x4e1788` / `0x4e1795`）。
pub const HALF: f32 = 0.5;
/// y 取反用的 `-0.0`（槽 `0x19e880`，`xorps` @ `0x4e175a`）。
pub const Y_FLIP: f32 = -0.0;

// ---------------------------------------------------------------------------
// `EffectData`：元素的**逐字节**布局（stride 0x30 = 48）
// ---------------------------------------------------------------------------

/// 元素步长：`lea r15,[rax+rax*2]` 其中 `rax = len << 4`（`0x4eb471`、`0x4eb475`）⇒ `len × 48`。
pub const EFFECT_STRIDE: u32 = 0x30;

/// `EffectData` 的字段偏移（全部**观测**，见表）。
///
/// | 偏移 | 类型 | 依据 |
/// |---|---|---|
/// | `+0x00` | `f64 × 2`（位置） | `movupd xmm0,[rbx]` @ `0x4eb566`；`movsd [r14+rbp]`/`[r14+rbp+8]` @ `0x4e19cc` |
/// | `+0x10` | `f64 × 2`（速度） | `movupd xmm1,[rbx+0x10]` @ `0x4eb56a`；撞障碍后整体清零 `movupd [r13],xmm0`（`r13 = rbx+0x10`）@ `0x4eb592` |
/// | `+0x20` | `u32`（起始 tick） | `mov ecx,[r14+rbp+0x20]` @ `0x4e1950`；`add eax,0x258; mov [rbx],eax` @ `0x4eb530` |
/// | `+0x24` | `u32`（持续 tick） | `mov eax,[r14+rbp+0x24]` @ `0x4e1955` |
/// | `+0x28` | `u8`（相位） | `movzx eax, byte [r14+rax+0x28]` @ `0x4e1932`（跳表 7 个 case） |
/// | `+0x2c` | 未知（4 字节） | 0x30−0x2c = 4；未见任何访问 |
pub mod effect_data_offset {
    /// 位置 `(x, y)`，两个 `f64`。
    pub const POS: u32 = 0x00;
    /// 速度 `(vx, vy)`，两个 `f64`。
    pub const VEL: u32 = 0x10;
    /// 起始 tick（`u32`）。
    pub const START_TICK: u32 = 0x20;
    /// 持续 tick（`u32`）：`draw` 用它判"这一帧还画不画"。
    pub const DURATION_TICKS: u32 = 0x24;
    /// 相位（`u8`，0..=6）。
    pub const PHASE: u32 = 0x28;
}

/// 二维点（`f64`，与原版 `+0x00`/`+0x10` 两对 `f64` 同域）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2D {
    pub x: f64,
    pub y: f64,
}

impl Vec2D {
    pub const ZERO: Vec2D = Vec2D { x: 0.0, y: 0.0 };
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// 逐分量乘（原版用 `mulpd` 一次乘两个分量）。
    pub fn scale(self, k: f64) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
        }
    }
    /// 逐分量加（原版用 `addpd`）。
    pub fn add(self, o: Vec2D) -> Self {
        Self {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }
}

/// RGBA（`f32`，与 Godot `Color` 同域）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    /// 只换 alpha。
    pub fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }
}

/// 相位 0 的基色 `(0.6, 0.6, 0.1)`：槽 `0x19c930`（两个 `0.6f32`）+ 立即数 `0x3dcccccd`（`0.1f32`）。
pub const COLOR_PHASE0: Color = Color::new(0.6, 0.6, 0.1, 1.0);
/// 相位 2/3 的基色 `(0.7, 0.7, 0.0)`：槽 `0x1bb820` + `mov dword [rsp+0x28], 0`。
pub const COLOR_PHASE23: Color = Color::new(0.7, 0.7, 0.0, 1.0);
/// 相位 4/5 的基色 `(0.9, 0.9, 0.1)`：槽 `0x19c450`（两个 `0.9f32`）+ `0.1f32`。
pub const COLOR_PHASE45: Color = Color::new(0.9, 0.9, 0.1, 1.0);
/// 相位 6 的基色 `(0.1, 0.1, 0.1)`：槽 `0x19c440`（两个 `0.1f32`）+ `0.1f32`。
pub const COLOR_PHASE6: Color = Color::new(0.1, 0.1, 0.1, 1.0);
/// 相位 6 的固定透明度 `0.7f32`：`movss xmm1,[0x1b952c=1.0]; mulss xmm1,[0x1bb820=0.7]` @ `0x4e228a`。
pub const ALPHA_PHASE6: f32 = 0.7;

/// 相位（`+0x28` 那个字节）。7 个值来自跳表 `0x1c3b8c` 的有效项数（0..=6）。**观测**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// 小圆 + `(0.6,0.6,0.1)`。
    CircleSmall = 0,
    /// 多边形（顶点公式未知）。
    Polygon = 1,
    /// 大圆 + `(0.7,0.7,0)`。
    CircleLargeA = 2,
    /// 大圆 + `(0.7,0.7,0)`。
    CircleLargeB = 3,
    /// 线段 + `(0.9,0.9,0.1)`。
    LineA = 4,
    /// 线段 + `(0.9,0.9,0.1)`。
    LineB = 5,
    /// 大圆 + `(0.1,0.1,0.1)`，alpha 固定 0.7。
    CircleDark = 6,
}

impl Phase {
    /// 跳表里有效的 case 数。
    pub const COUNT: usize = 7;

    /// 相位字节 → 变体；`>= 7` 时返回 `None`（原版跳表越界会跳到别的函数，属未定义）。
    pub fn from_byte(v: u8) -> Option<Phase> {
        const ALL: [Phase; Phase::COUNT] = [
            Phase::CircleSmall,
            Phase::Polygon,
            Phase::CircleLargeA,
            Phase::CircleLargeB,
            Phase::LineA,
            Phase::LineB,
            Phase::CircleDark,
        ];
        ALL.iter().copied().find(|p| *p as u8 == v)
    }

    /// `bt 0x5b, phase`：命中的相位在 `step_effect` 里**不推进**。
    pub fn is_terminal(self) -> bool {
        (STATE_TERMINAL_MASK >> (self as u8)) & 1 != 0
    }

    /// 该相位画的圆半径（只有圆相位有值）。
    pub fn circle_radius(self) -> Option<f32> {
        match self {
            Phase::CircleSmall => Some(RADIUS_SMALL),
            Phase::CircleLargeA | Phase::CircleLargeB | Phase::CircleDark => Some(RADIUS_LARGE),
            Phase::Polygon | Phase::LineA | Phase::LineB => None,
        }
    }

    /// 该相位的基色（alpha 由 [`alpha_at`] 决定，相位 6 例外：固定 0.7）。
    pub fn base_color(self) -> Color {
        match self {
            Phase::CircleSmall => COLOR_PHASE0,
            Phase::Polygon => COLOR_PHASE0, // 推断：多边形 case 没有独立的颜色槽，沿用 0x19c930/0x3dcccccd
            Phase::CircleLargeA | Phase::CircleLargeB => COLOR_PHASE23,
            Phase::LineA | Phase::LineB => COLOR_PHASE45,
            Phase::CircleDark => COLOR_PHASE6,
        }
    }
}

// ---------------------------------------------------------------------------
// 一个特效元素
// ---------------------------------------------------------------------------

/// `EffectData` 的字段子集（**已证的部分**）。
///
/// 未知字段（`+0x2c` 的 4 字节）有意**不放进来**：宁可少一个字段，
/// 也不要编一个没人验证过的字段名。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectData {
    /// `+0x00`：位置（棋盘坐标，`f64`）。
    pub pos: Vec2D,
    /// `+0x10`：速度（棋盘单位 / 秒，`f64`）。
    pub vel: Vec2D,
    /// `+0x20`：起始 tick。
    pub start_tick: u32,
    /// `+0x24`：持续 tick（`draw` 判活；`0` 表示只活一帧）。
    pub duration_ticks: u32,
    /// `+0x28`：相位。
    pub phase: Phase,
}

impl EffectData {
    /// 构造。**注意**：原版 `EffectData::new` 的字段初值**未知**（arm64 里它用
    /// `rand::random` 与 π，x86 侧没有可定位的独立函数，见模块文档 §5），
    /// 所以这里只提供"把已知字段填进去"的构造器，不假装知道初值。
    pub fn new(pos: Vec2D, start_tick: u32, duration_ticks: u32, phase: Phase) -> Self {
        Self {
            pos,
            vel: Vec2D::ZERO,
            start_tick,
            duration_ticks,
            phase,
        }
    }

    /// 这一帧还画不画：`now <= start + duration`。
    ///
    /// 指令 @ `0x4e1950..0x4e1960`：`ecx=[elem+0x20]; eax=[elem+0x24]; edx=eax+ecx;
    /// cmp edx,r15d; jb <跳过绘制>`（`r15d` = 当前 tick）。无符号比较。
    pub fn is_alive_at(&self, now_tick: u32) -> bool {
        self.start_tick.wrapping_add(self.duration_ticks) >= now_tick
    }

    /// 进度 `t = (now - start) / duration`，截到 `[0, 1]`；`duration == 0` 时返回 1.0。
    ///
    /// 指令 @ `0x4e1980..0x4e19b1`：
    /// `edx = now - start; jbe → alpha 保持 0.9; t = (f32)edx / (f32)duration;
    /// alpha = max(0, (1 - t) * 0.9)`。
    pub fn progress(&self, now_tick: u32) -> f32 {
        if self.duration_ticks == 0 {
            return 1.0;
        }
        let elapsed = now_tick.saturating_sub(self.start_tick) as f32;
        (elapsed / self.duration_ticks as f32).clamp(0.0, 1.0)
    }
}

/// 相位 0 / 2 / 3 的透明度：`max(0, (1 - t) * 0.9)`，`t = 0` 之前恒为 `0.9`。
///
/// 完整指令序列（`0x4e1970..0x4e19b5`）：
/// ```text
/// movss xmm0, [0x1b9534 = 0.9]      ; 默认 0.9
/// sub   edx, ecx                    ; edx = now - start
/// jbe   .keep                       ; now <= start ⇒ 0.9
/// cvtsi2ss xmm0, edx ; cvtsi2ss xmm1, eax(duration)
/// divss  xmm0, xmm1                 ; t
/// movss  xmm1, [0x1b952c = 1.0] ; subss xmm1, xmm0   ; 1 - t
/// mulss  xmm1, [0x1b9534 = 0.9]  ; ×0.9
/// maxss  xmm1, 0                     ; 夹到 >= 0
/// ```
pub fn alpha_at(now_tick: u32, start_tick: u32, duration_ticks: u32) -> f32 {
    if now_tick <= start_tick || duration_ticks == 0 {
        return ALPHA_GAIN;
    }
    let t = (now_tick - start_tick) as f32 / duration_ticks as f32;
    ((1.0 - t) * ALPHA_GAIN).max(0.0)
}

/// 相位 1（多边形）的透明度：`max(0, (1 - t/0.9) * 0.9)`。
/// 指令 @ `0x4e1ba9..0x4e1be4`（分母是 `0.9` 而不是 duration）。**逐位**
pub fn polygon_alpha(t: f32) -> f32 {
    ((1.0 - t / POLYGON_FADE_DIVISOR) * ALPHA_GAIN).max(0.0)
}

// ---------------------------------------------------------------------------
// `step_effect`
// ---------------------------------------------------------------------------

/// `delta`（秒）→ tick 数：`clamp(60 * delta, 0, u32::MAX)`，`cvttsd2si` 截断取整。
///
/// 指令 @ `0x4eb479..0x4eb4a3`：
/// `movsd xmm2,[0x19f918 = 60.0]; mulsd xmm2,xmm0; maxsd xmm1(0),xmm2;
/// movsd xmm10,[0x19f1a0 = 4294967295.0]; minsd; cvttsd2si rax,xmm2`。**逐位**
pub fn ticks_for_delta(delta_seconds: f64) -> u32 {
    let t = TICKS_PER_SECOND * delta_seconds;
    let t = t.max(0.0);
    let t = if t > u32::MAX as f64 { u32::MAX as f64 } else { t };
    t as u32
}

/// 一个「特效元素集合」按 `delta` 推进一帧。
///
/// 复原的是 `EffectDrawer::step_effect`（x86 `0x4eb450`）的**可验证骨架**：
///
/// ```text
/// ticks   = clamp(60 * delta, 0, u32::MAX)        0x4eb479
/// elapsed = now_tick.saturating_sub(ticks)        0x4eb4aa (sub + cmovb 0)
/// for e in effects:
///     快路径（delta <= 0，0x4eb4b7 ja 反向）：
///         若 phase 不是终态（bt 0x5b）且 e.start_tick <= elapsed：
///             phase = 下一相位；e.start_tick += 600     0x4eb4c2 / 0x4eb530
///     慢路径（delta > 0）：按 1/600 s 固定子步长积分——
///         pos += vel * (1/600)                        0x4eb56f mulpd + addpd
///         若 is_obstacle(pos)：vel = (0, 0)            0x4eb585 call + 0x4eb592
///         acc += 1/600 ; 剩余 tick = clamp((delta-acc)*60)   0x4eb59e..0x4eb5d0
/// ```
///
/// `is_obstacle` 是原版对 `graphwar_obstacle_manager` 的**唯一**外部调用
/// （`0x4eb585`，arm64 那侧符号名 = `GraphwarObstacleManager::is_obstacle`）：
/// 这里做成参数，避免本模块依赖别的模块。
///
/// **推断**部分：快路径里"下一相位"的落点（`2 → 3`、其余 `→ 6`）是按
/// `0x4eb4f6`（`mov byte [rbx+rax+0x28],3`）与 `0x4eb507`（`... ,6`）写的；
/// 慢路径里每个相位自己的分支（跳表 `0x1c3bb8`）没有逐条读完，这里统一按
/// "撞了就停 + 到期换相位"表达。
pub fn step_effects<F>(effects: &mut [EffectData], now_tick: u32, delta_seconds: f64, mut is_obstacle: F)
where
    F: FnMut(Vec2D) -> bool,
{
    let ticks = ticks_for_delta(delta_seconds);
    let elapsed = now_tick.saturating_sub(ticks);

    if delta_seconds <= 0.0 {
        // 快路径：不积分，只推进到期相位。
        for e in effects.iter_mut() {
            if e.phase.is_terminal() {
                continue;
            }
            if e.start_tick > elapsed {
                continue;
            }
            e.phase = match e.phase {
                Phase::CircleLargeA => Phase::CircleLargeB, // 2 → 3
                _ => Phase::CircleDark,                     // 其余 → 6
            };
            e.start_tick = e.start_tick.wrapping_add(PHASE_TICKS);
        }
        return;
    }

    // 慢路径：固定 1/600 s 子步。
    let mut acc = 0.0_f64;
    for e in effects.iter_mut() {
        loop {
            e.pos = e.pos.add(e.vel.scale(FIXED_STEP_SECONDS));
            if is_obstacle(e.pos) {
                e.vel = Vec2D::ZERO;
            }
            acc += FIXED_STEP_SECONDS;
            // 循环条件是 **f64 比较**（`ucomisd xmm0(delta), xmm3(acc); ja` @ `0x4eb628`），
            // 不是 tick 计数：一帧 1/60 s 正好 10 个子步。
            if !(delta_seconds > acc) {
                break;
            }
        }
        // 子步结束后再按"到期换相位"处理一次（快路径同款）。
        if !e.phase.is_terminal() && e.start_tick <= elapsed {
            e.phase = match e.phase {
                Phase::CircleLargeA => Phase::CircleLargeB,
                _ => Phase::CircleDark,
            };
            e.start_tick = e.start_tick.wrapping_add(PHASE_TICKS);
        }
    }
}

// ---------------------------------------------------------------------------
// T108（docs/06 §153）：`EffectDrawer` 的状态内核 + `step_effect` 真名边界
// ---------------------------------------------------------------------------
//
// | 原版函数 | Android 2.0.60 arm64 `.symtab` | Linux x86-64 `.eh_frame`（addr_probe fde 现取） |
// |---|---|---|
// | `EffectDrawer::step_effect` | `0x444d2c`（992 B） | **`0x4eb450`**..`0x4eb82b`（0x3db） |
//
// 体内 `panic::Location` = 0（T105 观测）⇒ 结构上进不了 527 分母；本轮补的是**函数边界**
// （语义此前已由 T57 按指令复原成 [`step_effects`] 自由函数）。
//
// ABI / 字段偏移（**观测**，逐条带指令地址）：
//   `rdi = &mut self`、`rsi = 障碍管理器`、`edx = now_tick`、`xmm0 = delta_seconds`
//   * `0x4eb45e mov rax,[rdi+0x10]` / `0x4eb46d mov rbx,[rdi+8]` ⇒ `self+0x08` = 元素表 ptr、
//     `self+0x10` = len（`self+0x18` 应是 cap，本函数**没读到**）
//   * `0x4eb46b mov ebp,edx` ⇒ 第 2 个整数参是 `now_tick`（不是 delta 的整数部分）
//   * `0x4eb479 movsd xmm2,[0x19f918]` = `60.0`；`0x4eb481 mulsd xmm2,xmm0` ⇒ `ticks = delta*60`；
//     `0x4eb48d..0x4eb4a4` 用 `0x19f1a0`（= `2147483647.0`）做饱和转换 ⇒ 与 [`ticks_for_delta`] 同一算式
//   * `0x4eb4b3 ucomisd xmm0,0.0` + `ja 0x4eb50e` ⇒ `delta > 0` 才走积分慢路径，否则只推进相位
//   * `0x4eb585 call 0x3f15b0` = **单点碰撞** `is_obstacle(&self, &[f64;2])`
//     （`rdi=r14`= 原 rsi 的障碍管理器、`rsi=rbx`= 元素地址，x/y 就在 `+0x00/+0x08`）
//     ⇒ 这个函数**自己不做障碍判定**，判定是外部管理器的方法
//   * `0x4eb4c2 add edx,0x258` + `0x4eb4c8 mov [rbx+rax+0x20],edx` ⇒ 相位推进一次 +600 tick
//     （`0x258` = 600，与 [`PHASE_TICKS`] 同一个立即数）
//
// ⚠️ **未解**（K3，不编）：`0x4eb511 cmp dword ptr [rdi+0x28], ebp` + `0x4eb514 jae 0x4eb687`
//   ⇒ `self+0x28` 是另一个 u32 字段，慢路径按 `now_tick <= self[+0x28]` 分成**两份结构相同、
//   跳表不同**的循环（`0x1c3bec` vs `0x1c3bb8`）。两份循环的语义差异本轮**没解出来**，
//   所以下面的结构体**不编**这个字段，[`step_effects`] 只实现其中一份（登记在 docs/06 §153.6）。
pub struct EffectDrawer {
    /// `self+0x08`/`+0x10`/`+0x18`：特效元素表。原版 `Vec<EffectData>`（元素 0x30 字节）。
    pub effects: Vec<EffectData>,
}

impl EffectDrawer {
    /// 空状态（原版构造点/初值**未知**，见模块文档 §5；这里只给"没有元素"的初态）。
    pub fn new() -> Self {
        Self { effects: Vec::new() }
    }

    /// `EffectDrawer::step_effect`（Linux **`0x4eb450`** / arm64 `0x444d2c`，992 B）
    ///
    /// 每 tick 推进所有特效元素：`delta > 0` 时按 **1/600 s 固定子步**积分速度
    /// （150 步/相位？不 — 见 [`FIXED_STEP_SECONDS`]：一帧 `1/60 s` = 10 个子步），
    /// 撞障碍把速度清零；随后按 `start_tick + duration` 到期把相位推进 600 tick。
    ///
    /// 语义本体在 [`step_effects`]（T57 按指令复原的自由函数）；本方法是**真名边界**
    /// （预注册 `fixtures/denom_drawers_math_plan.txt` §2.5 K1）。
    pub fn step_effect<F>(&mut self, now_tick: u32, delta_seconds: f64, is_obstacle: F)
    where
        F: FnMut(Vec2D) -> bool,
    {
        step_effects(&mut self.effects, now_tick, delta_seconds, is_obstacle)
    }
}

impl Default for EffectDrawer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 绘制计划
// ---------------------------------------------------------------------------

/// `EffectDrawer::draw` 的变换（`0x4e17c0`，槽 `0x9bc018`）。
///
/// `position = size * 0.5`、`rotation = 0.0`、`scale = (W/50, ±W/50)`；
/// y 的符号由 `self+0x1f8` 的位掩码决定（`and 0x101` / `test 0x10000` /
/// `cmp 0x1000000` @ `0x4e175f..0x4e1783`），**三个标志的语义未知**（推断：
/// 与 y 轴朝向有关），所以这里把 `flip_y` 作为参数。
pub fn draw_transform(size_w: f32, size_h: f32, flip_y: bool) -> Transform {
    let unit = size_w / SCALE_DIVISOR;
    Transform {
        position: (size_w * HALF, size_h * HALF),
        rotation: 0.0,
        scale: (unit, if flip_y { -unit } else { unit }),
    }
}

/// `draw_set_transform` 的三个参数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: (f32, f32),
    pub rotation: f32,
    pub scale: (f32, f32),
}

/// 本模块会发出的原语。只有四种，一一对应二进制里的四种引擎调用。
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// `CanvasItem::draw_set_transform(position, rotation, scale)`。
    SetTransform(Transform),
    /// `CanvasItem::draw_circle(position, radius, color, filled, width, antialiased)`。
    Circle {
        center: (f32, f32),
        radius: f32,
        color: Color,
        filled: bool,
        width: f32,
        antialiased: bool,
    },
    /// `CanvasItem::draw_line(from, to, color, width, antialiased)`。
    Line {
        from: (f32, f32),
        to: (f32, f32),
        color: Color,
        width: f32,
        antialiased: bool,
    },
    /// `CanvasItem::draw_polygon(points, colors)`。
    ///
    /// ⚠️ 顶点公式**未知**（见模块文档 §5）：这里只保留"确实调了 draw_polygon"这个事实
    /// 与颜色，顶点一律留空，避免把猜的几何当成观测。
    Polygon {
        points: Vec<(f32, f32)>,
        colors: Vec<Color>,
    },
}

/// 相位 → 图元。返回 `None` 表示"这一帧不画"（已过期）。
///
/// 与 `0x4e1950` 起 7 个 case 的对应关系见模块文档 §2 的表。
pub fn effect_primitive(e: &EffectData, now_tick: u32, flip_y: bool, size: (f32, f32)) -> Option<Primitive> {
    if !e.is_alive_at(now_tick) {
        return None;
    }
    let center = (e.pos.x as f32, e.pos.y as f32);
    let alpha = alpha_at(now_tick, e.start_tick, e.duration_ticks);
    let _ = (flip_y, size); // 变换由调用方先发一次，这里不重复

    match e.phase {
        Phase::CircleSmall | Phase::CircleLargeA | Phase::CircleLargeB => Some(Primitive::Circle {
            center,
            radius: e.phase.circle_radius().unwrap(),
            color: e.phase.base_color().with_alpha(alpha),
            filled: true,
            width: WIDTH_UNUSED,
            antialiased: ANTIALIASED,
        }),
        Phase::CircleDark => Some(Primitive::Circle {
            center,
            radius: RADIUS_LARGE,
            color: COLOR_PHASE6.with_alpha(ALPHA_PHASE6),
            filled: true,
            width: WIDTH_UNUSED,
            antialiased: ANTIALIASED,
        }),
        Phase::Polygon => Some(Primitive::Polygon {
            points: Vec::new(), // 未知：顶点公式未复原（§5）
            colors: vec![COLOR_PHASE0.with_alpha(polygon_alpha(e.progress(now_tick)))],
        }),
        Phase::LineA | Phase::LineB => Some(Primitive::Line {
            from: center,
            to: center, // 未知：端点公式未复原（§5），先保留"画在 center 的线"这一事实
            color: COLOR_PHASE45.with_alpha(alpha),
            width: WIDTH_UNUSED,
            antialiased: ANTIALIASED,
        }),
    }
}

/// 整帧的图元清单：1 条变换 + 每个存活元素 1 条图元（顺序同二进制：HashMap 迭代顺序，
/// 原版是 `std::collections::HashMap` 的 RawTable 遍历，`0x4e1830..0x4e1900` 的
/// `pmovmskb`/`tzcnt`/`shl 6` 就是 hashbrown 的控制字节扫描——**迭代顺序不可依赖**）。
pub fn frame_primitives(
    effects: &[EffectData],
    now_tick: u32,
    flip_y: bool,
    size: (f32, f32),
) -> Vec<Primitive> {
    let mut out = vec![Primitive::SetTransform(draw_transform(size.0, size.1, flip_y))];
    for e in effects {
        if let Some(p) = effect_primitive(e, now_tick, flip_y, size) {
            out.push(p);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// `process` / `ready` 的可验证事实
// ---------------------------------------------------------------------------

/// `EffectDrawer::process` 做的四件事（`0x4e2f80`）。**观测**（顺序见调用点）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStep {
    /// `std::time::Instant::now` — 起一个 `TimeProbe`（`0x4e2f94` 起）。
    StartTimeProbe,
    /// `GraphwarModuleContainer::get_module` ×3 + `[module+0x58]` 取当前 tick（`0x4e30f1` 起）。
    ReadTickFromModule,
    /// `Vec::retain` 丢掉过期元素（`0x4e3397` 的 `call 0x3cb080` 前后）。
    RetainAlive,
    /// `CanvasItem::queue_redraw`（`0x4e35ec`，槽 `0x9baa60`）。
    QueueRedraw,
    /// `Instant::elapsed` + 若 `> 1e9 ns` 则 `_print` 打位置串 `src/effect_drawer.rs:507`（`0x4e346f` 起）。
    FinishTimeProbe,
}

/// `process` 的步骤表（**观测**：每个步骤都能指到上面的调用点）。
pub const PROCESS_STEPS: [ProcessStep; 5] = [
    ProcessStep::StartTimeProbe,
    ProcessStep::ReadTickFromModule,
    ProcessStep::RetainAlive,
    ProcessStep::QueueRedraw,
    ProcessStep::FinishTimeProbe,
];

/// `ready` 注册的两个事件（顺序 = 订阅顺序，`effect_info_event` 先）。
pub const READY_SUBSCRIPTIONS: [(&str, u64, u64); 2] = [
    // (事件名, 名字字面量槽, 闭包 vtable 槽)
    ("effect_info_event", 0x1c4225, 0x95fb50),
    ("tick_event", 0x1c4236, 0x95fb78),
];


// ===========================================================================
// 原版方法体：原版 `EffectDrawer` 的 gdext 节点类（`Control` 子类）（T57 从 `src/effect_drawer.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `EffectDrawer` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/effect_drawer.rs` 搬进 `src/effect_drawer.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `effect_drawer.rs` 里**，证据：
//! `panic::Location` 证据：`draw 0x4e1520` → `src/effect_drawer.rs:530:125`、`ready 0x4e25b0` → `:479:129`、`process 0x4e2f80` → `:509:133`、`ready::{{closure}} 0x4e2a30`（`fixtures/name_to_addr_v2.tsv`）。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::effect_drawer::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `EffectDrawer` 是一个 `Control` 子类（`graphwarrust::effect_drawer::EffectDrawer`，
//! 类名字面量在 `effect_drawer::CLASS_NAME`），它自己**画**特效：`draw` 里发
//! `draw_set_transform` ×1 + `draw_circle` / `draw_polygon` / `draw_line`。
//!
//! `src/effect_drawer.rs` 已经把那套图元算术复原出来了（`frame_primitives` /
//! `effect_primitive` / `alpha_at` / `step_effects`），但它按约定**不碰引擎**（一行 Godot
//! 调用都没有）。本文件是「让那份图元清单**真的落到 `CanvasItem::draw_*` 上**」的那层薄壳：
//!
//! ```text
//! src/effect_drawer.rs                 纯逻辑：常量 / 相位跳表 / 变换 / 图元清单 / 子步积分
//! src/effect_drawer.rs ← 本文件：把 Primitive 翻译成 draw_* 并计数
//! ```
//!
//! 算术本体**只有一份**，仍在 `src/effect_drawer.rs` 里；本文件不复制任何算式
//! （唯一"新"的算术是 `as f32` 的窄化）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 图元的**种类与顺序** | `draw_set_transform` ×1、`draw_circle` ×4、`polygon` ×1、`draw_line` ×2（`docs/16` §18） | 同：直接遍历 [`effect_drawer::frame_primitives`] | **观测**（GOT 槽 + 全 `.text` 扫描） |
//! | 半径 / 颜色 / 透明度 | 0.05 / 0.06、四组基色、`max(0,(1−t)·0.9)` | 同（取自 `effect_drawer` 的常量与函数） | **观测** |
//! | 多边形**顶点** | 公式**未复原**（`effect_drawer` 模块文档 §5） | **不发明**顶点：纯逻辑给的 `points` 是空的 ⇒ 默认跳过；调用方用 [`EffectDrawer::set_polygon_placeholder`] 给才画 | 原版**未知**；给点才画是**我方选择** |
//! | 多边形颜色份数 | 每个顶点一份、**3 份同色**（`docs/16` §18 的「3 份同色」） | 纯逻辑给 1 个颜色 ⇒ 按**顶点数**复制 N 份（`N` 由调用方定，不硬编码 3） | **观测**（3 份）/ 复制规则**推断** |
//! | `draw_line` 的端点 | 公式**未复原**（`from == to == center`） | 照 `effect_primitive` 给的（退化线段：`draw_line` 照样被调到） | 端点**未知** |
//! | 当前 tick | `process` 从 `GraphwarModuleContainer` 的模块读 `[module+0x58]` | **自己数**：`step(delta)` 用 `effect_drawer::ticks_for_delta` 推进 | 推进规则**逐位**；"谁来喂 tick"**未接** |
//! | 撞障碍 | `0x4eb585` 调 `GraphwarObstacleManager::is_obstacle`；撞了 `vel = (0,0)` | 传 `|_| false`（= 场上没有障碍）——**没有任何障碍管理器** | 原版**观测**；本切片**缺失** |
//! | `process` 的 5 步 | 时间探针 → 读 tick → `retain` → `queue_redraw` → 收尾探针（`PROCESS_STEPS`） | 只做「推进 + `queue_redraw`」；探针与 `retain` 见 [`EffectDrawer::retain_alive`] | 步骤表**观测**；`retain` 判据**推断** |
//! | `ready` 的 2 个订阅 | `effect_info_event` / `tick_event`（`READY_SUBSCRIPTIONS`） | **没接**（`GraphwarBackend` 还没到节点层） | 名字与顺序**观测**；订阅**未做** |
//! | 元素容器 | `std::collections::HashMap`（迭代顺序**不可依赖**，`0x4e1830` 的 hashbrown 扫描） | `Vec`（顺序 = 插入顺序，可复现） | **化简**，不是原版形态 |
//!
//! ⇒ 这个类解决的是「**图元清单能不能真的变成 `draw_*` 调用**」，
//! **不是**"`EffectDrawer` 已经复原完了"（多边形顶点、线段端点、tick 来源都还缺）。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/draw_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! EffectDrawer : Control
//!   @export flip_y : bool = false        # 原版读 self+0x1f8 的位掩码（语义未知），这里由调用方给
//!
//!   spawn_effect(x, y, phase, duration_ticks) -> int   # 建一个 EffectData，返回 effect_count()
//!   clear_effects() -> void
//!   effect_count() -> int
//!   now_tick() -> int
//!   set_now_tick(t: int) -> void
//!   step(delta_seconds: float) -> int                  # 推进 tick + 物理子步（step_effects）
//!   retain_alive() -> int                              # 丢掉过期元素（推断判据），返回剩下几个
//!   effect_phase(i: int) -> int
//!   effect_alpha(i: int) -> float                      # 走 effect_drawer::alpha_at
//!   set_polygon_placeholder(points: PackedVector2Array) -> void
//!   draw_call_count() / process_call_count() / ready_call_count() -> int
//!   drawn_transform_total() / drawn_circle_total() / drawn_polygon_total() / drawn_line_total() -> int
//!   polygon_skipped_total() -> int                     # "这一帧本来要画多边形但没顶点"的次数
//! ```
//!
//! 后面那串计数是**观测点**：没有它们，headless 里分不清"引擎没调 `_draw`"和"调了但一个图元都没发"。
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用**（`WithBaseField::base_mut` 返回 `BaseMut<'_, Self>`，握着
//!    `&mut self`）⇒ 循环里既画东西又改 `self.xxx` 编译不过（`E0499`）。本类的做法：
//!    先在**局部变量**里累计计数，`base` 的作用域结束后再写回字段。
//! 2. **不用写 `set_process(true)`**：Godot 4.7 对实现了 `_process` 虚方法的扩展类自己开；
//!    原版 `process` 的证据里也没有这一句。
//! 3. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 本类绝不从
//!    `#[func]` 里发图元，只发 `queue_redraw()`，等引擎回调 `_draw`。
//! 4. **`draw_polygon` 少于 3 个顶点会打印引擎错误**（`CanvasItem::draw_polygon` 的
//!    `ERR_FAIL_COND_MSG`）⇒ 顶点不足时本类**不调**（记到 `polygon_skipped_total`），
//!    而不是"调一下看它抱不抱怨"。



use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::effect_drawer::{self, Color as FxColor, EffectData, Phase, Primitive, Vec2D};

/// 把 `effect_drawer::Color`（RGBA 四个 f32）搬成 Godot 的 `Color`。
fn color_of(c: FxColor) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// 把 `(f32, f32)` 搬成 Godot 的 `Vector2`（Godot 4 默认单精度构建，不会掉精度）。
fn v2(p: (f32, f32)) -> Vector2 {
    Vector2::new(p.0, p.1)
}

/// 象限 1 的相位字节上限：`Phase::COUNT - 1`。越界的相位在原版里是跳表越界（未定义），
/// 本类**拒绝**而不是猜一个分支。
const MAX_PHASE_BYTE: i64 = (Phase::COUNT - 1) as i64;

/// `EffectDrawer`：把 [`effect_drawer::frame_primitives`] 的图元清单真的画出来的 `Control`。
///
/// 元素容器用 `Vec`（原版是 `HashMap`，迭代顺序不可依赖）；顺序 = `spawn_effect` 的调用顺序。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct EffectDrawer {
    base: Base<Control>,

    /// `draw` 时 y 轴是否翻转。
    ///
    /// 原版是 `self+0x1f8` 的位掩码（`and 0x101` / `test 0x10000` / `cmp 0x1000000`，
    /// 三个标志的**语义未知**，见 [`effect_drawer::draw_transform`] 的文档）⇒ 这里由调用方给。
    #[export]
    flip_y: bool,

    /// 特效元素。原版是 `HashMap`（`EffectDrawer::process` 用 `Vec::retain` 清理它）。
    effects: Vec<EffectData>,

    /// 当前 tick。原版从 `GraphwarModuleContainer` 里那个模块的 `+0x58` 读；本类自己数。
    now_tick: u32,

    /// 多边形占位顶点（`effect_drawer` 模块文档 §5：顶点公式未复原）。
    ///
    /// **顺序无关的调试口**：空 = 不画多边形（原版的几何我们不知道）；给了 ≥3 个点才画。
    polygon_placeholder: Vec<(f32, f32)>,

    /// `_draw` 被引擎调用过几次（观测点）。
    draw_calls: i64,
    /// `_process` 被引擎调用过几次（观测点）。
    process_calls: i64,
    /// `_ready` 被引擎调用过几次（观测点）。
    ready_calls: i64,

    /// 累计发出的 `draw_set_transform` 条数。
    transform_total: i64,
    /// 累计发出的 `draw_circle` 个数。
    circle_total: i64,
    /// 累计发出的 `draw_polygon` 个数。
    polygon_total: i64,
    /// 累计发出的 `draw_line` 条数。
    line_total: i64,
    /// 「这一帧本来要画多边形、但没有顶点可用」的次数（原版这里**会**画一个三角形）。
    polygon_skipped: i64,
}

#[godot_api]
impl IControl for EffectDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            // 原版那三个标志的语义未知 ⇒ 默认不翻（y 轴朝下，与 `axis_drawer` 的默认一致）。
            flip_y: false,
            effects: Vec::new(),
            now_tick: 0,
            polygon_placeholder: Vec::new(),
            draw_calls: 0,
            process_calls: 0,
            ready_calls: 0,
            transform_total: 0,
            circle_total: 0,
            polygon_total: 0,
            line_total: 0,
            polygon_skipped: 0,
        }
    }

    /// `_ready`：原版在这里订 2 个后端事件（[`effect_drawer::READY_SUBSCRIPTIONS`]：
    /// `effect_info_event` 先、`tick_event` 后）。
    ///
    /// 本切片**只记数**：要订事件得有 `GraphwarBackend`，那个模块还没接到节点层。
    fn ready(&mut self) {
        self.ready_calls += 1;
    }

    /// `_process`：原版做 5 步（[`effect_drawer::PROCESS_STEPS`]）；本类做「推进 + 要重画」。
    ///
    /// 少了的三步各自有理由：两个时间探针只是 `_print` 超时（`TIME_PROBE_*`），
    /// `RetainAlive` 的判据是**推断**（见 [`EffectDrawer::retain_alive`]，单独暴露成方法）。
    fn process(&mut self, delta: f64) {
        self.process_calls += 1;
        self.step(delta);
        if !self.effects.is_empty() {
            // 原版 `0x4e35ec` 调 `CanvasItem::queue_redraw`（槽 `0x9baa60`）。
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `EffectDrawer::draw`（vaddr `0x4e1520`）。
    ///
    /// 顺序与二进制一致：**先一条** `draw_set_transform`（`0x4e17c0`），然后每个存活元素一条图元。
    /// 图元清单整个来自 [`effect_drawer::frame_primitives`]，本函数只做「Rust 图元 → `draw_*`」。
    fn draw(&mut self) {
        self.draw_calls += 1;

        // 尺寸先在**语句内**取完（`base()` 借的是 `&self`，出语句就结束）。
        let size = self.base().get_size();
        let mut primitives = effect_drawer::frame_primitives(
            &self.effects,
            self.now_tick,
            self.flip_y,
            (size.x, size.y),
        );

        // 顶点公式未复原 ⇒ 纯逻辑给的是空 `points`。调用方给了占位顶点才补上（见文件头对照表）。
        let placeholder = self.polygon_placeholder.clone();
        for prim in primitives.iter_mut() {
            if let Primitive::Polygon { points, .. } = prim {
                if points.is_empty() {
                    *points = placeholder.clone();
                }
            }
        }

        // 计数先在局部变量里累计：`base_mut()` 握着 `&mut self`，那段里碰不到 `self.xxx`
        // （`E0499`，见文件头第 1 条）。
        let mut transform = 0i64;
        let mut circle = 0i64;
        let mut polygon = 0i64;
        let mut line = 0i64;
        let mut skipped = 0i64;

        {
            let mut base = self.base_mut();
            for prim in &primitives {
                match prim {
                    Primitive::SetTransform(t) => {
                        transform += 1;
                        // Godot 4 的后两个参数有默认值 ⇒ gdext 生成 `*_ex(...)` builder。
                        base.draw_set_transform_ex(v2(t.position))
                            .rotation(t.rotation)
                            .scale(v2(t.scale))
                            .done();
                    }
                    Primitive::Circle {
                        center,
                        radius,
                        color,
                        filled,
                        width,
                        antialiased,
                    } => {
                        circle += 1;
                        base.draw_circle_ex(v2(*center), *radius, color_of(*color))
                            .filled(*filled)
                            .width(*width)
                            .antialiased(*antialiased)
                            .done();
                    }
                    Primitive::Line {
                        from,
                        to,
                        color,
                        width,
                        antialiased,
                    } => {
                        line += 1;
                        // 端点公式未复原 ⇒ `from == to == center`（退化线段）。原版**照样**
                        // 调一次 `draw_line`（`0x4e1520` 的 2 个 `0x9bc228` 调用点），不跳过。
                        base.draw_line_ex(v2(*from), v2(*to), color_of(*color))
                            .width(*width)
                            .antialiased(*antialiased)
                            .done();
                    }
                    Primitive::Polygon { points, colors } => {
                        // `CanvasItem::draw_polygon` 要求 ≥3 个点，否则打印引擎错误
                        // ⇒ 顶点没复原时**不调**（文件头第 4 条）。
                        if points.len() < 3 || colors.is_empty() {
                            skipped += 1;
                            continue;
                        }
                        polygon += 1;
                        let mut pts = PackedVector2Array::new();
                        for p in points {
                            pts.push(v2(*p));
                        }
                        // 单个颜色 → 每个顶点一份（原版观测到「3 份同色」；份数 = 顶点数，
                        // 是**推断**，不硬编码 3）。见文件头对照表。
                        let mut cols = PackedColorArray::new();
                        for i in 0..points.len() {
                            let c = colors[i % colors.len()];
                            cols.push(color_of(c));
                        }
                        base.draw_polygon(&pts, &cols);
                    }
                }
            }
        }

        self.transform_total += transform;
        self.circle_total += circle;
        self.polygon_total += polygon;
        self.line_total += line;
        self.polygon_skipped += skipped;
    }
}

#[godot_api]
impl EffectDrawer {
    /// 建一个特效元素（`EffectData::new`：位置 + 起始 tick + 持续 tick + 相位）。
    ///
    /// * `start_tick` 取**当前** tick（原版的起始 tick 从 tick 模块来，本类自己数）；
    /// * `phase` 取 `0..=6`（[`Phase::COUNT`]），越界返回 `-1` 且**不建**——原版跳表越界是
    ///   未定义行为，本类不猜分支。
    ///
    /// 返回建完之后 `effect_count()`。
    #[func]
    fn spawn_effect(&mut self, x: f64, y: f64, phase: i64, duration_ticks: i64) -> i64 {
        if !(0..=MAX_PHASE_BYTE).contains(&phase) {
            godot_warn!("EffectDrawer::spawn_effect 相位越界 {phase}（有效 0..=6）");
            return -1;
        }
        let Some(phase) = Phase::from_byte(phase as u8) else {
            return -1;
        };
        let duration_ticks = duration_ticks.clamp(0, u32::MAX as i64) as u32;
        let data = EffectData::new(
            Vec2D::new(x, y),
            self.now_tick,
            duration_ticks,
            phase,
        );
        // 原版 `EffectData` 的 `vel` 初值**未知**（`EffectData::new` 的字段初值未复原），
        // 纯逻辑给的是 (0,0) ⇒ 物理子步不会移动它。这里**照抄纯逻辑**，不加自己的初值。
        self.effects.push(data);
        self.base_mut().queue_redraw();
        self.effects.len() as i64
    }

    /// 清空所有特效元素。
    #[func]
    fn clear_effects(&mut self) {
        self.effects.clear();
        self.base_mut().queue_redraw();
    }

    /// 当前持有几个特效元素。
    #[func]
    fn effect_count(&self) -> i64 {
        self.effects.len() as i64
    }

    /// 当前 tick（原版从模块 `+0x58` 读，本类自己数）。
    #[func]
    fn now_tick(&self) -> i64 {
        self.now_tick as i64
    }

    /// 直接把当前 tick 设成某个值（验收脚本用它对齐期望值）。
    #[func]
    fn set_now_tick(&mut self, tick: i64) {
        self.now_tick = tick.clamp(0, u32::MAX as i64) as u32;
        self.base_mut().queue_redraw();
    }

    /// 推进一帧：`ticks = clamp(60·delta, 0, u32::MAX)`，然后跑
    /// [`effect_drawer::step_effects`]（快路径 / 1/600 s 子步两分支都在那个函数里）。
    ///
    /// **没有障碍管理器** ⇒ `is_obstacle` 恒 `false`（文件头对照表）。
    /// 返回推进后的 `now_tick()`。
    #[func]
    fn step(&mut self, delta_seconds: f64) -> i64 {
        self.now_tick = self
            .now_tick
            .saturating_add(effect_drawer::ticks_for_delta(delta_seconds));
        let now = self.now_tick;
        effect_drawer::step_effects(&mut self.effects, now, delta_seconds, |_pos| false);
        now as i64
    }

    /// 丢掉过期元素，返回剩下几个。
    ///
    /// 原版 `process` 的第 3 步是 `Vec::retain`（`0x4e3397` 前后的 `call 0x3cb080`），
    /// 但**判据没读到**（`effect_drawer` 模块文档只记了"丢掉过期元素"）。
    /// 本类用 [`EffectData::is_alive_at`]（= `draw` 判活的同一条判据）⇒ **推断**，
    /// 所以它**不**在 `process` 里自动跑，而是单独暴露给调用方。
    #[func]
    fn retain_alive(&mut self) -> i64 {
        let now = self.now_tick;
        self.effects.retain(|e| e.is_alive_at(now));
        self.effects.len() as i64
    }

    /// 第 `index` 个元素现在的相位字节（越界返回 `-1`）。
    #[func]
    fn effect_phase(&self, index: i64) -> i64 {
        match self.effects.get(index.max(0) as usize) {
            Some(e) => e.phase as i64,
            None => -1,
        }
    }

    /// 第 `index` 个元素此刻的透明度（走 [`effect_drawer::alpha_at`]；越界返回 `-1.0`）。
    ///
    /// 相位 6 是特例（固定 `ALPHA_PHASE6`），相位 1 走 `polygon_alpha` ⇒ 这两种情况
    /// 这里返回 `-1.0`，让调用方知道"这个相位不看 `alpha_at`"。
    #[func]
    fn effect_alpha(&self, index: i64) -> f64 {
        let Some(e) = self.effects.get(index.max(0) as usize) else {
            return -1.0;
        };
        match e.phase {
            Phase::CircleDark | Phase::Polygon => -1.0,
            _ => effect_drawer::alpha_at(self.now_tick, e.start_tick, e.duration_ticks) as f64,
        }
    }

    /// 给多边形相位一组**占位顶点**（棋盘坐标）。
    ///
    /// 原版的顶点公式**没复原**（`effect_drawer` 模块文档 §5：只读到两组点各乘 `0.2`）。
    /// 本类绝不发明几何 ⇒ 默认不画；调用方（验收脚本 / 未来的几何复原）给了点才画。
    /// 少于 3 个点等价于清空。
    #[func]
    fn set_polygon_placeholder(&mut self, points: PackedVector2Array) {
        self.polygon_placeholder = if points.len() < 3 {
            Vec::new()
        } else {
            points.as_slice().iter().map(|p| (p.x, p.y)).collect()
        };
        self.base_mut().queue_redraw();
    }

    /// 当前有几条占位顶点（0 = 不画多边形）。
    #[func]
    fn polygon_placeholder_count(&self) -> i64 {
        self.polygon_placeholder.len() as i64
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// 累计发出过几条 `draw_set_transform`。
    #[func]
    fn drawn_transform_total(&self) -> i64 {
        self.transform_total
    }

    /// 累计发出过几个 `draw_circle`。
    #[func]
    fn drawn_circle_total(&self) -> i64 {
        self.circle_total
    }

    /// 累计发出过几个 `draw_polygon`。
    #[func]
    fn drawn_polygon_total(&self) -> i64 {
        self.polygon_total
    }

    /// 累计发出过几条 `draw_line`。
    #[func]
    fn drawn_line_total(&self) -> i64 {
        self.line_total
    }

    /// 「本来要画多边形、但没有顶点」的次数。
    #[func]
    fn polygon_skipped_total(&self) -> i64 {
        self.polygon_skipped
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」，不发明第二份）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/draw_classes_check.gd`（headless，真引擎）。
// 这里钉的是三件容易悄悄跑偏的事：相位表、alpha 的分档、几何常量的来源。

#[cfg(test)]
mod tests {
    use super::*;

    /// 相位表就是 `effect_drawer::Phase` 的 7 个值，`MAX_PHASE_BYTE` 与它同步。
    #[test]
    fn phase_range_matches_the_pure_module() {
        assert_eq!(Phase::COUNT, 7);
        assert_eq!(MAX_PHASE_BYTE, 6);
        for b in 0..=MAX_PHASE_BYTE as u8 {
            assert!(Phase::from_byte(b).is_some(), "相位 {b} 应该有效");
        }
        assert!(Phase::from_byte(7).is_none(), "相位 7 在跳表外");
        assert!(Phase::from_byte(255).is_none());
    }

    /// 半径只有 0.05 / 0.06 两个值，且**圆相位**才有半径（多边形 / 线段没有）。
    #[test]
    fn circle_radii_come_from_the_two_constants() {
        assert_eq!(
            Phase::CircleSmall.circle_radius(),
            Some(effect_drawer::RADIUS_SMALL)
        );
        for p in [Phase::CircleLargeA, Phase::CircleLargeB, Phase::CircleDark] {
            assert_eq!(p.circle_radius(), Some(effect_drawer::RADIUS_LARGE));
        }
        for p in [Phase::Polygon, Phase::LineA, Phase::LineB] {
            assert_eq!(p.circle_radius(), None);
        }
        assert!((effect_drawer::RADIUS_SMALL - 0.05).abs() < 1e-9);
        assert!((effect_drawer::RADIUS_LARGE - 0.06).abs() < 1e-9);
    }

    /// `_draw` 真的是「1 条变换 + 每个存活元素 1 条图元」，而且相位 0 给的是圆。
    ///
    /// 这是验收脚本里「circle_total == 1」那条断言在 Rust 侧的对应物。
    #[test]
    fn frame_primitives_is_one_transform_plus_one_per_effect() {
        let e = EffectData::new(Vec2D::new(5.0, 5.0), 0, 60, Phase::CircleSmall);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        assert_eq!(prims.len(), 2, "1 条 SetTransform + 1 条图元");
        assert!(matches!(prims[0], Primitive::SetTransform(_)));
        let Primitive::Circle { center, radius, color, .. } = prims[1] else {
            panic!("相位 0 应该是圆，实际 {:?}", prims[1]);
        };
        assert_eq!(center, (5.0, 5.0));
        assert_eq!(radius, effect_drawer::RADIUS_SMALL);
        // `alpha_at(0, 0, 60)` = 0.9（`now <= start` 那条快路）
        assert!((color.a - 0.9).abs() < 1e-6, "a={}", color.a);
    }

    /// 过期元素不再进图元清单（`is_alive_at` = `now <= start + duration`）。
    #[test]
    fn expired_effects_do_not_emit_primitives() {
        let e = EffectData::new(Vec2D::new(0.0, 0.0), 0, 10, Phase::CircleSmall);
        assert!(!effect_drawer::frame_primitives(&[e], 11, false, (500.0, 300.0))
            .iter()
            .any(|p| matches!(p, Primitive::Circle { .. })));
        assert!(effect_drawer::frame_primitives(&[e], 10, false, (500.0, 300.0))
            .iter()
            .any(|p| matches!(p, Primitive::Circle { .. })));
    }

    /// 多边形相位在纯逻辑里**故意没有顶点**（本类据此记 `polygon_skipped`）。
    #[test]
    fn polygon_phase_has_no_vertices_to_draw() {
        let e = EffectData::new(Vec2D::new(5.0, 5.0), 0, 60, Phase::Polygon);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        let Primitive::Polygon { points, colors } = &prims[1] else {
            panic!("相位 1 应该是多边形，实际 {:?}", prims[1]);
        };
        assert!(points.is_empty(), "顶点公式未复原 ⇒ 故意留空");
        assert_eq!(colors.len(), 1, "纯逻辑给 1 个颜色，本类按顶点数复制");
    }

    /// 线段相位给的是**退化**线段（`from == to == center`）—— 原版端点未复原。
    #[test]
    fn line_phase_is_a_degenerate_segment() {
        let e = EffectData::new(Vec2D::new(3.0, -4.0), 0, 60, Phase::LineA);
        let prims = effect_drawer::frame_primitives(&[e], 0, false, (500.0, 300.0));
        let Primitive::Line { from, to, .. } = &prims[1] else {
            panic!("相位 4 应该是线段，实际 {:?}", prims[1]);
        };
        assert_eq!(from, to, "端点公式未复原 ⇒ 两端同点");
        assert_eq!(*from, (3.0, -4.0));
    }

    /// `ticks_for_delta`：60 tick = 1 秒，1/60 s 一帧 = 1 tick（`60 × 0.016666…` 截断后是 0 还是 1，
    /// 由纯逻辑决定，别在这里"修"它）。
    #[test]
    fn step_advances_ticks_by_the_pure_rule() {
        assert_eq!(effect_drawer::ticks_for_delta(1.0), 60);
        assert_eq!(effect_drawer::ticks_for_delta(0.5), 30);
        assert_eq!(effect_drawer::ticks_for_delta(-1.0), 0, "负数夹到 0");
        // ⚠️ `60.0 × (1.0/60.0)` 在 f64 里**不是**精确的 1（实测截断后得 **1**），所以
        // "一帧 = 几 tick"由浮点舍入决定，靠整除猜会错。这里只钉住它落在 `0..=1` 里。
        let per_frame = effect_drawer::ticks_for_delta(1.0 / 60.0);
        assert!(per_frame <= 1, "per_frame={per_frame}");
        // 1/600 s（物理子步）比一帧还小一个数量级 ⇒ 60·(1/600) = 0.1 ⇒ 截断成 0。
        assert_eq!(
            effect_drawer::ticks_for_delta(effect_drawer::FIXED_STEP_SECONDS),
            0
        );
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;

#[cfg(test)]
mod denom_step_effect_tests {
    //! T108：`EffectDrawer::step_effect`（linux `0x4eb450` / arm64 `0x444d2c`）的边界与分支。
    //!
    //! 期望值来源 = **反汇编读出的立即数/分支**（不是"我方输出"）：
    //!  * `0x4eb4c2 add edx,0x258` ⇒ 相位推进 **600** tick
    //!  * `0x4eb4bb mov ecx,0x5b` + `bt ecx,edx` ⇒ 终态相位 = 位图 `0x5b` = `{0,1,3,4,6}`
    //!  * `0x4eb4e7 cmp edx,2` + `0x4eb4f6 mov byte [..],3` ⇒ 相位 **2 → 3**
    //!  * `0x4eb507 mov byte [..],6` ⇒ 其余可推进相位 **→ 6**
    //!  * `0x19f918` = `60.0`（`mulsd` @ `0x4eb481`）、`0x19ed20`/`0x19f9f0` = `1/600`（`mulpd` @ `0x4eb56f`）
    use super::*;

    fn elem(phase: Phase, start_tick: u32) -> EffectData {
        let mut e = EffectData::new(Vec2D::new(1.0, 2.0), start_tick, 600, phase);
        e.vel = Vec2D::new(60.0, 0.0);
        e
    }

    #[test]
    fn phase_advance_is_600_ticks_and_only_for_non_terminal_phases() {
        // 常量本身就是证据：0x258 / 0x5b / 60.0 / 1/600
        assert_eq!(PHASE_TICKS, 0x258, "相位推进量应为 0x258");
        assert_eq!(STATE_TERMINAL_MASK, 0x5b, "终态位图应为 0x5b");
        assert_eq!(TICKS_PER_SECOND, 60.0);
        // 位图 0x5b = 0b1011011 ⇒ 终态 {0,1,3,4,6}、可推进 {2,5}
        for p in [Phase::CircleSmall, Phase::Polygon, Phase::CircleLargeB, Phase::LineA, Phase::CircleDark] {
            assert!(p.is_terminal(), "{p:?} 应被 `bt 0x5b` 判成终态");
        }
        for p in [Phase::CircleLargeA, Phase::LineB] {
            assert!(!p.is_terminal(), "{p:?} 应可推进");
        }

        // 相位 2 → 3（`cmp edx,2` / `mov byte ..,3`），start_tick += 600
        let mut fx = EffectDrawer::new();
        fx.effects.push(elem(Phase::CircleLargeA, 0));
        fx.step_effect(600, 0.0, |_| false); // delta=0 ⇒ 快路径：只推进相位
        assert_eq!(fx.effects[0].phase, Phase::CircleLargeB);
        assert_eq!(fx.effects[0].start_tick, 600, "`add eax,0x258` 的 600");

        // 相位 5 → 6（`_ => 6` 那一臂）
        let mut fx2 = EffectDrawer::new();
        fx2.effects.push(elem(Phase::LineB, 0));
        fx2.step_effect(600, 0.0, |_| false);
        assert_eq!(fx2.effects[0].phase, Phase::CircleDark);
        assert_eq!(fx2.effects[0].start_tick, 600);

        // 终态相位一律不动（`bt` + `jb` 跳过）
        let mut fx3 = EffectDrawer::new();
        fx3.effects.push(elem(Phase::CircleSmall, 0));
        fx3.step_effect(600, 0.0, |_| false);
        assert_eq!(fx3.effects[0].phase, Phase::CircleSmall);
        assert_eq!(fx3.effects[0].start_tick, 0);

        // `start_tick > elapsed` 不推进（`0x4eb4f0 cmp edx,[rsp+4]` + `ja`）
        let mut fx4 = EffectDrawer::new();
        fx4.effects.push(elem(Phase::CircleLargeA, 1000));
        fx4.step_effect(600, 0.0, |_| false);
        assert_eq!(fx4.effects[0].phase, Phase::CircleLargeA);
    }

    #[test]
    fn integration_is_ten_fixed_substeps_per_sixtieth() {
        // `delta = 1/60 s`、子步 `1/600 s` ⇒ **10 步**；每步 `pos += vel × (1/600)`
        // ⇒ `vel.x = 60` 时一步 0.1、十步 1.0（浮点累加，允许 1 ulp 量级误差）
        let mut fx = EffectDrawer::new();
        fx.effects.push(elem(Phase::CircleSmall, 0));
        fx.step_effect(0, TICK_DELTA_SECONDS, |_| false);
        // 初值 pos.x = 1.0 ⇒ 十步之后应是 2.0（位移 1.0）
        assert!(
            (fx.effects[0].pos.x - 2.0).abs() < 1e-15,
            "十步 60×(1/600) 应位移 1.0（1.0 → 2.0），实际 {}",
            fx.effects[0].pos.x
        );
        assert_eq!(fx.effects[0].pos.y, 2.0, "vel.y = 0 ⇒ y 不动");

        // `delta = 0` ⇒ 快路径，**不积分**（`0x4eb4b3 ucomisd/ja`：只有 >0 才进慢路径）
        let mut fx2 = EffectDrawer::new();
        fx2.effects.push(elem(Phase::CircleSmall, 0));
        fx2.step_effect(0, 0.0, |_| false);
        assert_eq!(fx2.effects[0].pos, Vec2D::new(1.0, 2.0), "delta=0 不应移动");

        // 撞障碍 ⇒ 速度清零（`0x4eb58a test al,al` / `0x4eb592 movupd [r13],xmm0`）
        // 障碍判定在 `0x4eb585 call 0x3f15b0`，这里用闭包给同一语义。
        let mut fx3 = EffectDrawer::new();
        fx3.effects.push(elem(Phase::CircleSmall, 0));
        fx3.step_effect(0, TICK_DELTA_SECONDS, |p| p.x >= 1.05);
        assert_eq!(fx3.effects[0].vel, Vec2D::ZERO, "撞障碍后速度必须清零");
        // 撞上之后虽然速度清零，位置停在**第一次**越界那一步（0.1 的整数倍）
        assert!(fx3.effects[0].pos.x > 1.0 && fx3.effects[0].pos.x <= 1.1 + 1e-15);
    }

    /// 空元素表：`0x4eb462 test rax,rax; je 0x4eb81c`（len==0 直接返回）。
    #[test]
    fn empty_effect_table_returns_early() {
        let mut fx = EffectDrawer::new();
        fx.step_effect(12345, TICK_DELTA_SECONDS, |_| {
            panic!("len=0 时不该调障碍判定（`0x4eb462 je 0x4eb81c`）")
        });
        assert!(fx.effects.is_empty());
    }
}
