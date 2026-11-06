//! `graphwar_long_press_animation` —— 原版 `src/graphwar_long_press_animation.rs` 的还原（**纯逻辑层**）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。每个条目都能指到
//! **反汇编地址**或**符号表真名**；指不到的写 `未知` 并说明缺什么证据。
//!
//! **地址口径**：本节所有地址一律是 **ELF 虚地址**（`objdump` 口径）；
//! Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。
//!
//! ## 0. 这个模块是什么（一句话 + 证据档）
//!
//! `GraphwarLongPressAnimation` 是一个 `Control` 子类（`IControl`，见
//! [`TRAIT_IMPLS`]），**只有一个虚函数 `draw`**：它读自己的启动时刻，
//! 算出一个旋转角，画**两层矩形**（黑底 + 白层），转满一圈（0.5 s）后
//! 把自己的启动时刻写成哨兵，动画结束。
//! 〔绘制结构 = 观测（`0x4d8100` 逐指令）；「长按进度环」这类**用途**描述 = 推断〕
//!
//! ## 1. 名字从哪来
//!
//! 只有 `draw` 在 `fixtures/name_to_addr.tsv` 里是 `唯一锚`
//! （`0x4d8100`，锚 `src/graphwar_long_press_animation.rs:52:55`，4 个双向唯一锚）。
//! 其余名字来自**同版本 arm64 2.0.60 的 `.symtab`**：
//!
//! ```text
//! .reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so
//! <GraphwarLongPressAnimation as godot_core::…::IControl>::draw   @0x4d44f4 (980 B)
//! <GraphwarLongPressAnimation as GodotClass>::class_id             @0x4be98c (64 B)
//! ```
//!
//! ## 2. **2.0.60 只剩 `draw`**（更正 `docs/02-module-map.md` §18.5 C 表）
//!
//! §18.5 的 C 表（用 **2024** 未 strip 构建的符号表生成）给本模块记了
//! `draw, process, ready, start, stop` 五个方法。**2.0.60 的 arm64 `.symtab` 里
//! 只有 `draw`**（按 `graphwar_long_press_animation` 全长搜索，`start`/`stop`/
//! `ready`/`process` 一个都没有）。本节一律以 2.0.60 为准。
//!
//! ## 3. panic 锚（x86 与 arm64 **同值**；Location 是编译期常量）
//!
//! ```text
//! 位置                                          x86 引用函数
//! src/graphwar_long_press_animation.rs:10:10     0x4bd430
//!                                    :17:1       0x4bd650 / 0x4bda50 / 0x4d7bc0 / 0x4d7d40
//!                                    :31:1       0x4bd800
//!                                    :52:55      0x4d8100  ← draw
//!                                    :52:86      0x4d8100
//!                                    :53:56      0x4d8100
//!                                    :53:87      0x4d8100
//! ```
//!
//! `:10:10`/`:17:1`/`:31:1` 与 `graphwar_button` 的 `:14:10`/`:83:1`/`:111:1`
//! 同构：都是 gdext 样板（derive / 两个 `#[godot_api]` 块）的 `expect`。
//! **真正的 `draw` 有 4 个锚，两两成对**（`:52:55`+`:52:86`、`:53:56`+`:53:87`）
//! ⇒ 源码第 52、53 行各有**两个** `.expect()`（`Option`/`Result` 连续解包）。
//! 二进制里对应 4 条 panic 出口：`0x4d8522` / `0x4d852f` / `0x4d855e` / `0x4d856b`。
//!
//! ## 4. `draw` 的完整结构（**观测**：`0x4d8100`，1232 B）
//!
//! ```text
//! 4d810d  eax = [rdi+0x20]                  ; 启动时刻的 nanos
//! 4d8110  cmp eax, 0x3b9aca00 ; je 4d8514   ; == 1e9 ⇒ 没在动画，直接返回
//! 4d811e  [rsp+0x78] = [rdi+0x18] ; [rsp+0x80] = eax     ; 组一个 Instant 到栈上
//! 4d812e  … 3 次 godot 取值（经 GOT 槽，静态不可解）⇒ a0,a1 / s0,s1 / n,c
//!         （每次取值前后各有一对 Option/Result 解包，即 §3 的 4 个锚）
//! 4d82b2  … 见下面的算术
//! 4d83a0  call *[0x9bafe8]  on [rsp+0x78]    ; = Instant::elapsed()（只吃启动时刻）
//! 4d83fb  (double)nanos / 1e9 + (double)secs ; = elapsed（秒，f64）
//! 4d8407  ×2 ; ×2 ; ×π                       ; = elapsed * 4π  ← 角度
//! 4d8485  draw #1  (rdx = Color{0,0,0,1})     ; 黑
//! 4d84ed  draw #2  (rdx = Color{1,1,1,1})     ; 白
//! 4d8503  if (angle > 2π) [rdi+0x20] = 1e9    ; 结束（下一次进来就走 4d8115 返回）
//! ```
//!
//! 算术部分（`p0,p1` = 第一次取值的一对，`s0,s1` = 第二次的一对，
//! `n` = 第三次取值的 i32，`c` = 第三次取值的 f32）：
//!
//! ```text
//! v48 = s0 / 10.0                       ; 0x1b9530 = 10.0
//! v4c = s1 / 3.0                        ; 0x1bb2b8 = 3.0
//! v44 = 0.5 * v4c                       ; 0x1ba128 = 0.5
//! v10 = s1 * 0.5 - v44                  ; 代数上 == s1/3（编译期没消掉那次除法）
//! y   = p1 + s1 * 0.5
//! v8  = if (float)(n/2) > c { p0 + v48 + v44 + s0 + v10 }
//!     else                  { p0 - v48 - v44 - v10 }
//! ```
//!
//! 两次绘制各自带 6 个 f32 + 一个 `Color*` + `esi=0x1e` + `ecx=0`：
//!
//! ```text
//! #1  xmm0=v8  xmm1=y  xmm2=v10  xmm3=-0.2  xmm4=angle+0.2  xmm5=2*0.2*v4c+v4c
//! #2  xmm0=v8  xmm1=y  xmm2=v10  xmm3= 0.0  xmm4=angle      xmm5=v4c
//! ```
//!
//! `0.2`（`0x1ba598`）与 `-0.0`（`0x19e880`，用来取负）都是就地量到的常量。
//! **被调用的 godot API 名未知**（`call *[0x9bc020]`，GOT 槽，静态解析不到符号）；
//! 所以本节只说「两次绘制 + 这些实参」，不猜它们是 `draw_rect` 还是别的。
//!
//! ## 5. 状态与跨模块接口（观测 + 一处**推断**）
//!
//! 本模块的动画状态是**两个字段**：`+0x18`（`Instant` secs）、
//! `+0x20`（`Instant` nanos）。写它的是**别人**：
//! `GraphwarButton::button_down`（`0x4ef3e8`/`0x4ef3ec`）写 `now`，
//! `GraphwarButton::button_up`（`0x4ef6e9`）写哨兵 `1e9`。
//! 〔观测：两侧指令都已逐条核过〕
//!
//! 场景里这个节点的**名字是 `"AnimationRect"`**：`GraphwarButton::ready`
//! （`0x4e410d`）按这个名字找节点再 cast 成本类型。
//! 〔观测；「场景里它就是这么命名的」= 推断，缺的证据是 `.tscn` 文本〕
//!
//! 动画时长由 `draw` 自己判：`angle = elapsed * 4π > 2π ⇔ elapsed > 0.5 s`
//! ⇒ [`SPIN_DURATION_SECS`] = 0.5，与 `GraphwarButton::init` 里那个
//! `Duration{0, 0x1dcd6500}`（500 ms）**数值相同**〔推断：同一个常量；
//! 缺的证据：把 0.5 s 传给本节点的指令〕
//!
//! ## 6. 本节没解出来的（照实写）
//!
//! * 3 次取值的 **API 名**与**语义**（哪一对是位置、哪一对是尺寸）：全是 GOT 槽调用，
//!   静态解析不到符号 ⇒ 本文件只把它们记成 `p0,p1 / s0,s1 / n,c`。
//! * 两次绘制的 **API 名**（同上）与 `esi=0x1e` 的含义。
//! * `+0x00..0x18`、`+0x28..` 的字段：本模块唯一的方法 `draw` 只读 `+0x18/+0x20`，
//!   其余字段由 gdext 的 4 个 `__inner_init` 写，未读。
//! * `0x4bd430`/`0x4bd650`/`0x4bda50`/`0x4d7bc0`/`0x4d7d40`/`0x4bd800` 六个
//!   样板函数的**具体身份**（derive / `#[godot_api]` 两组）——只到「它们是样板」这一档。

/// 原版函数表的一行（地址一律 **ELF 虚地址**）。
#[derive(Debug, Clone, Copy)]
pub struct OrigFn {
    /// 名字。游戏侧用真名，样板用「样板 + 锚行」描述。
    pub name: &'static str,
    /// x86-64 2.0.60 的 ELF 虚地址。
    pub vaddr: u32,
    /// 函数体字节数。
    pub size: u32,
    /// 该函数引用的 `src/graphwar_long_press_animation.rs` 锚（`line:col`）。
    pub anchors: &'static [(u32, u32)],
    /// 定名依据。
    pub evidence: &'static str,
}

/// `draw` 的地址（**唯一锚**：`src/graphwar_long_press_animation.rs:52:55`）。
pub const DRAW: OrigFn = OrigFn {
    name: "<GraphwarLongPressAnimation as IControl>::draw",
    vaddr: 0x4d8100,
    size: 1232,
    anchors: &[(52, 55), (52, 86), (53, 56), (53, 87)],
    evidence: "name_to_addr.tsv 唯一锚 + 本仓库 Location 复算；arm64 同名符号 980 B",
};

/// 本模块 7 个引用 panic 锚的 x86 函数（1 个是 `draw`，6 个是 gdext 样板）。
pub const ORIG_FUNCS: &[OrigFn] = &[
    DRAW,
    OrigFn {
        name: "样板：derive(GodotClass) 生成（`class_id`/`__registration_storage` 一类）",
        vaddr: 0x4bd430,
        size: 544,
        anchors: &[(10, 10)],
        evidence: "锚 10:10 与 graphwar_button 的 14:10 同构（结构体定义行）",
    },
    OrigFn {
        name: "样板：`#[godot_api]` 生成（17:1 组，4 个之一）",
        vaddr: 0x4bd650,
        size: 432,
        anchors: &[(17, 1)],
        evidence: "与 graphwar_button 的 83:1 四个函数同构",
    },
    OrigFn {
        name: "样板：`#[godot_api]` 生成（31:1，虚函数块）",
        vaddr: 0x4bd800,
        size: 592,
        anchors: &[(31, 1)],
        evidence: "与 graphwar_button 的 111:1 同构（592 B 也相同）",
    },
    OrigFn {
        name: "样板：`#[godot_api]` 生成（17:1 组，4 个之一）",
        vaddr: 0x4bda50,
        size: 432,
        anchors: &[(17, 1)],
        evidence: "同上",
    },
    OrigFn {
        name: "样板：`#[godot_api]` 生成（17:1 组，4 个之一）",
        vaddr: 0x4d7bc0,
        size: 384,
        anchors: &[(17, 1)],
        evidence: "同上",
    },
    OrigFn {
        name: "样板：`#[godot_api]` 生成（17:1 组，4 个之一）",
        vaddr: 0x4d7d40,
        size: 384,
        anchors: &[(17, 1)],
        evidence: "同上",
    },
];

/// 按地址查 [`ORIG_FUNCS`]。
pub fn orig_fn(vaddr: u32) -> Option<&'static OrigFn> {
    ORIG_FUNCS.iter().find(|f| f.vaddr == vaddr)
}

/// 同版本 arm64 2.0.60 `.symtab` 里属于本模块的条目（**只用于定名**）。
#[derive(Debug, Clone, Copy)]
pub struct ArmSym {
    /// demangle 后的完整符号名。
    pub name: &'static str,
    /// arm64 虚地址（**不是** x86 地址）。
    pub addr: u32,
    /// arm64 函数体字节数。
    pub size: u32,
}

/// 本模块在 arm64 2.0.60 里的符号。
pub const ARM64_SYMS: &[ArmSym] = &[
    ArmSym {
        name: "<GraphwarLongPressAnimation as IControl>::draw",
        addr: 0x4d44f4,
        size: 980,
    },
    ArmSym {
        name: "<GraphwarLongPressAnimation as GodotClass>::class_id",
        addr: 0x4be98c,
        size: 64,
    },
    ArmSym {
        name: "<GraphwarLongPressAnimation as UserClass>::__default_virtual_call::virtual_fn",
        addr: 0x4be8f0,
        size: 156,
    },
    ArmSym {
        name: "<GraphwarLongPressAnimation as ImplementsGodotApi>::__register_methods",
        addr: 0x4d4000,
        size: 400,
    },
    ArmSym {
        name: "<GraphwarLongPressAnimation as ImplementsGodotApi>::__register_constants",
        addr: 0x4d4190,
        size: 400,
    },
    ArmSym {
        name: "GraphwarLongPressAnimation::__registration_storage::STORAGE",
        addr: 0x8b4330,
        size: 56,
    },
];

/// 按名字查 [`ARM64_SYMS`]（后缀匹配）。
pub fn arm_sym(name: &str) -> Option<&'static ArmSym> {
    ARM64_SYMS.iter().find(|s| s.name.ends_with(name))
}

/// `GraphwarLongPressAnimation` 实现的 gdext trait。
///
/// 只有 `IControl`（**不是** `IButton`）⇒ 它是 `Control` 的直接子类，
/// 不继承 `BaseButton` 的虚函数。
pub const TRAIT_IMPLS: &[&str] = &["IControl", "GodotClass", "ImplementsGodotVirtual"];

/// 类名（Godot 侧注册名）。
pub const CLASS: &str = "GraphwarLongPressAnimation";

/// `.rodata` 中类名的地址。
pub const CLASS_NAME_VADDR: u32 = 0x1c4319;

/// 类名的字节长度（`(ptr,len)` 自校验：`0x4ff520` 的 `mov len,26`）。
pub const CLASS_NAME_LEN: u8 = 26;

/// 返回类名 `&str` 的两个小函数（各 64 字节）。
pub const CLASS_NAME_FNS: &[u32] = &[0x4ff520, 0x4ffeb0];

/// 本模块的 7 个 panic 锚：`(源码位置, line, col, 引用它的 x86 函数)`。
///
/// `draw` 的 4 个锚**两两成对**：`:52:55`+`:52:86`、`:53:56`+`:53:87`
/// ⇒ 源码第 52、53 行各有两个 `.expect()`。
pub const SRC_ANCHORS: &[(&str, u32, u32, &[u32])] = &[
    ("src/graphwar_long_press_animation.rs:10:10", 10, 10, &[0x4bd430]),
    (
        "src/graphwar_long_press_animation.rs:17:1",
        17,
        1,
        &[0x4bd650, 0x4bda50, 0x4d7bc0, 0x4d7d40],
    ),
    ("src/graphwar_long_press_animation.rs:31:1", 31, 1, &[0x4bd800]),
    ("src/graphwar_long_press_animation.rs:52:55", 52, 55, &[0x4d8100]),
    ("src/graphwar_long_press_animation.rs:52:86", 52, 86, &[0x4d8100]),
    ("src/graphwar_long_press_animation.rs:53:56", 53, 56, &[0x4d8100]),
    ("src/graphwar_long_press_animation.rs:53:87", 53, 87, &[0x4d8100]),
];

/// 1 秒 = `1_000_000_000` ns（`divsd 1e9` @ `0x19eff8`；哨兵 `0x3b9aca00`）。
pub const NANOS_PER_SEC: u32 = 1_000_000_000;

/// 「没在动画」哨兵：`nanos = 1e9`（`0x4d8110` 的 `cmp eax,0x3b9aca00`）。
pub const SENTINEL_NANOS: u32 = NANOS_PER_SEC;

/// 角速度：`elapsed * 4π`（`0x4d8407`/`0x4d840b`/`0x4d840f` 的 ×2、×2、×π）。
pub const SPIN_RATE: f64 = 4.0 * std::f64::consts::PI;

/// 结束阈值：`2π`（`0x19fab8`，`0x4d8503` 的 `ucomisd`）。
pub const SPIN_LIMIT: f64 = std::f64::consts::TAU;

/// 动画时长（秒）：`SPIN_LIMIT / SPIN_RATE = 0.5`。
///
/// 与 `GraphwarButton` 的长按阈值 500 ms 数值相同〔推断：同一个常量〕。
pub const SPIN_DURATION_SECS: f64 = 0.5;

/// `.rodata` 里被 `draw` 读到的常量：`(值, 地址, 指令)`。
///
/// 前四个在 `.rodata` 里是 **f32**（`divss`/`mulss` 用），所以这里记的是
/// 把该 f32 **原样加宽**到 f64 的值（`0.2f32` ≠ `0.2f64`，差 3e-9）；
/// 后三个是 f64。改一个就等于宣称二进制变了。
pub const DRAW_CONSTS: &[(f64, u32, &str)] = &[
    (3.0, 0x1bb2b8, "0x4d82b2 `divss`：s1 / 3.0"),
    (10.0, 0x1b9530, "0x4d82c0 `divss`：s0 / 10.0"),
    (0.5, 0x1ba128, "0x4d82d6 `mulss`：s1 * 0.5"),
    (0.200_000_002_980_232_24, 0x1ba598, "0x4d8423 `movss`：0.2f32（相位差）"),
    (1e9, 0x19eff8, "0x4d83fb `divsd`：nanos → 秒"),
    (std::f64::consts::PI, 0x19fdc8, "0x4d840f `mulsd`"),
    (std::f64::consts::TAU, 0x19fab8, "0x4d8503 `ucomisd`：结束判定"),
];

/// 一个 `Color`（Godot 的 RGBA 四 float）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    /// 红。
    pub r: f32,
    /// 绿。
    pub g: f32,
    /// 蓝。
    pub b: f32,
    /// 透明度。
    pub a: f32,
}

impl Color {
    /// 第 1 次绘制的颜色：`.rodata` `0x19cf70` = `{0,0,0,1}`（黑）。
    pub const BLACK: Color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    /// 第 2 次绘制的颜色：`.rodata` `0x19ed00` = `{1,1,1,1}`（白）。
    pub const WHITE: Color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
}

/// 第 1 次绘制的颜色常量地址。
pub const COLOR_BLACK_VADDR: u32 = 0x19cf70;
/// 第 2 次绘制的颜色常量地址。
pub const COLOR_WHITE_VADDR: u32 = 0x19ed00;

/// 一次绘制调用的实参（`call *[0x9bc020]`；被调 API 名**未知**，见 §6）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCall {
    /// `xmm0`。
    pub xmm0: f32,
    /// `xmm1`。
    pub xmm1: f32,
    /// `xmm2`。
    pub xmm2: f32,
    /// `xmm3`。
    pub xmm3: f32,
    /// `xmm4`。
    pub xmm4: f32,
    /// `xmm5`。
    pub xmm5: f32,
    /// `rdx` 指向的颜色。
    pub color: Color,
    /// `esi`（两次都是 `0x1e`）。
    pub esi: u32,
    /// `ecx`（两次都是 `0`）。
    pub ecx: u32,
}

/// 本模块的动画状态：`+0x18`（secs）/ `+0x20`（nanos）。
///
/// 初值由 `GraphwarButton::init`/`button_up` 写哨兵；
/// `GraphwarButton::button_down` 写 `Instant::now()`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpinState {
    /// `[+0x18]`。
    pub secs: u64,
    /// `[+0x20]`。
    pub nanos: u32,
}

impl SpinState {
    /// 没在动画时的状态（`nanos == 1e9`）。
    pub const STOPPED: SpinState = SpinState {
        secs: 0,
        nanos: SENTINEL_NANOS,
    };

    /// 由 `button_down` 写入的启动时刻。
    pub const fn started(secs: u64, nanos: u32) -> SpinState {
        SpinState { secs, nanos }
    }

    /// `draw` 的第一条判定（`0x4d8110`）：`nanos == 1e9` ⇒ 直接返回。
    pub const fn is_running(self) -> bool {
        self.nanos != SENTINEL_NANOS
    }

    /// `draw` 的收尾（`0x4d850d`）：只写 nanos。
    pub const fn stopped(self) -> SpinState {
        SpinState {
            secs: self.secs,
            nanos: SENTINEL_NANOS,
        }
    }
}

/// `Instant::elapsed()`（`call *[0x9bafe8]` @ `0x4d83a0`）的等价算术。
///
/// 原版把 `(double)nanos / 1e9 + (double)secs` 算成秒（`0x4d83fb`–`0x4d8403`）。
pub fn elapsed_secs(start: SpinState, now_secs: u64, now_nanos: u32) -> f64 {
    let d_secs = now_secs.wrapping_sub(start.secs) as f64;
    let d_nanos = now_nanos as f64 - start.nanos as f64;
    d_secs + d_nanos / f64::from(NANOS_PER_SEC)
}

/// 旋转角：`elapsed * 4π`（`0x4d8407` ×2、`0x4d840b` ×2、`0x4d840f` ×π）。
pub fn spin_angle(elapsed: f64) -> f64 {
    elapsed * SPIN_RATE
}

/// 是否该结束（`0x4d8503` 的 `ucomisd angle, 2π` + `jbe`）。
pub fn is_finished(angle: f64) -> bool {
    angle > SPIN_LIMIT
}

/// `draw` 的几何（§4 的算式，逐条对照指令）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// 两次绘制共用的 `xmm0`（`v8`，带方向的那一项）。
    pub v8: f32,
    /// 两次绘制共用的 `xmm1`（`y = p1 + s1 * 0.5`）。
    pub y: f32,
    /// `v10 = s1 * 0.5 - 0.5 * (s1 / 3)`（两次绘制共用 `xmm2`）。
    pub v10: f32,
    /// `s1 / 3`（第 2 次绘制的 `xmm5`）。
    pub v4c: f32,
    /// `0.5 * (s1 / 3)`（`v44`）。
    pub v44: f32,
    /// `s0 / 10`（`v48`）。
    pub v48: f32,
    /// `(float)(n / 2) > c` 这个分支条件本身。
    pub branch_positive: bool,
}

/// 按 §4 的算式算几何。`n` 是第三次取值拿到的 i32，`c` 是同一处的 f32。
pub fn geometry(p: (f32, f32), s: (f32, f32), n: i32, c: f32) -> Geometry {
    let (p0, p1) = p;
    let (s0, s1) = s;
    let v48 = s0 / 10.0;
    let v4c = s1 / 3.0;
    let v44 = 0.5 * v4c;
    let v10 = s1 * 0.5 - v44;
    let half_n = n / 2;
    let branch_positive = (half_n as f32) > c;
    let v8 = if branch_positive {
        p0 + v48 + v44 + s0 + v10
    } else {
        p0 - v48 - v44 - v10
    };
    Geometry {
        v8,
        y: p1 + s1 * 0.5,
        v10,
        v4c,
        v44,
        v48,
        branch_positive,
    }
}

/// 一帧的绘制计划：`draw` 在「running」时恰画 2 次。
///
/// 直接对应 `0x4d8485`（黑）与 `0x4d84ed`（白）两处调用。
pub fn draw_plan(g: Geometry, angle: f32) -> [DrawCall; 2] {
    [
        DrawCall {
            xmm0: g.v8,
            xmm1: g.y,
            xmm2: g.v10,
            xmm3: -0.2,
            xmm4: angle + 0.2,
            xmm5: 2.0 * 0.2 * g.v4c + g.v4c,
            color: Color::BLACK,
            esi: 0x1e,
            ecx: 0,
        },
        DrawCall {
            xmm0: g.v8,
            xmm1: g.y,
            xmm2: g.v10,
            xmm3: 0.0,
            xmm4: angle,
            xmm5: g.v4c,
            color: Color::WHITE,
            esi: 0x1e,
            ecx: 0,
        },
    ]
}

/// 一帧 `draw` 的结果。
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// `nanos == 1e9` ⇒ 什么都没画（`0x4d8115`）。
    Idle,
    /// 画了 2 次，且没到终点。
    Drawing([DrawCall; 2]),
    /// 画完这一帧后超时 ⇒ 调用方要把 `[+0x20]` 写成 1e9（`0x4d850d`）。
    Finished([DrawCall; 2]),
}

/// `draw` 的完整判据（纯逻辑版）。
///
/// 顺序与原版一致：先判 running（**在算任何东西之前**，`0x4d8110`），
/// 再算 elapsed/angle 并画 2 次，最后判是否该写哨兵（`0x4d8503`）。
pub fn draw_frame(state: SpinState, now_secs: u64, now_nanos: u32, g: Geometry) -> Frame {
    if !state.is_running() {
        return Frame::Idle;
    }
    let elapsed = elapsed_secs(state, now_secs, now_nanos);
    let plan = draw_plan(g, spin_angle(elapsed) as f32);
    if is_finished(spin_angle(elapsed)) {
        Frame::Finished(plan)
    } else {
        Frame::Drawing(plan)
    }
}
