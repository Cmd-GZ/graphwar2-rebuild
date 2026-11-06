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
