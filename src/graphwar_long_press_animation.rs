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


// ===========================================================================
// 原版方法体：原版 `GraphwarLongPressAnimation` 的 gdext 节点类（`Control` 子类）（T57 从 `src/graphwar_long_press_animation.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `GraphwarLongPressAnimation` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/graphwar_long_press_animation.rs` 搬进 `src/graphwar_long_press_animation.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `graphwar_long_press_animation.rs` 里**，证据：
//! `panic::Location` 证据：`draw 0x4d8100` → `src/graphwar_long_press_animation.rs:52:55`（4 个锚）、
// `_::__init::__inner_init 0x4bd430` → `:10:10`。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::graphwar_long_press_animation::EntityDrawer` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `GraphwarLongPressAnimation` 是一个 `Control` 子类，**唯一的虚函数是 `draw`**
//! （2.0.60 arm64 `.symtab` 里这个模块只剩 `draw`，见纯逻辑模块的 §2）：
//!
//! ```text
//! 读自己的启动时刻(+0x18 secs / +0x20 nanos)
//!   → nanos == 1e9 ⇒ 直接返回（没在动画）
//!   → elapsed = Instant::elapsed()
//!   → angle = elapsed × 4π
//!   → draw_arc 两次（黑底 + 白层）
//!   → angle > 2π ⇒ 把 nanos 写回 1e9（结束）
//! ```
//!
//! `src/graphwar_long_press_animation.rs` 已经把上面每一段算术复原成纯函数
//! （`elapsed_secs` / `spin_angle` / `is_finished` / `geometry` / `draw_plan` / `draw_frame`），
//! 但按约定**不碰引擎**。本文件就是"让那两次绘制真的落到 `CanvasItem::draw_arc` 上"的那层壳：
//!
//! ```text
//! src/graphwar_long_press_animation.rs          纯逻辑：状态 / 角度 / 几何 / 两次绘制的实参
//! src/graphwar_long_press_animation.rs   ← 本文件：建类 + 把实参搬进 draw_arc + 观测计数
//! ```
//!
//! # 一处**就地更正**：被调的 API 是 `draw_arc`，不是"矩形"
//!
//! 纯逻辑模块的 §0 写的是"画**两层矩形**（黑底 + 白层）"，§6 把被调 API 记为**未知**。
//! 本文件按 `docs/16-client.md` 的 GOT 槽表（§23 之后那张「绘制 API → 包装 → 槽」表，
//! 行 `draw_arc | 0x19f9d0 | 0x8abca0 | 0x9bc020 | 5 | EntityDrawer::draw ×3 +
//! GraphwarLongPressAnimation::draw ×2`）把它落成 **`CanvasItem::draw_arc`**：
//!
//! | 证据 | 内容 | 档位 |
//! |---|---|---|
//! | GOT 槽 | `GraphwarLongPressAnimation::draw` 里两处 `call *[0x9bc020]`（`0x4d8485` / `0x4d84ed`）就是全 `.text` 里引用该槽的 5 条中的 2 条 | **观测** |
//! | 槽 → 包装 → API | `0x9bc020` ← `0x8abca0` ← 字面量 `0x19f9d0` = `draw_arc`（`R_X86_64_RELATIVE` 写入） | **观测** |
//! | 形参个数与类型 | 6 个 f32（xmm0..xmm5）+ 1 个 int（`esi=0x1e`）+ 1 个 `Color*`（`rdx`）+ 1 个 bool（`ecx=0`）—— 与 `draw_arc(center, radius, start_angle, end_angle, point_count, color, width, antialiased)` 的形参表**逐槽对齐** | **观测**（个数/类型）/ **推断**（哪一槽是哪个形参） |
//!
//! 这也解释了 §0 那条"两层矩形"为什么站不住：`draw_rect` 的实参是 4 个 f32 + 颜色 + 2 个
//! 标量，装不进 6 个 f32 槽；而**旋转角从 0 走到 2π**这件事本身也只有在"弧"上才有意义。
//!
//! # 与原版的差别（逐条）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 两次绘制 | `draw_arc` ×2：黑 `(-0.2 → angle+0.2)` 宽 `2·0.2·v4c+v4c`；白 `(0 → angle)` 宽 `v4c` | 同（实参整个来自 [`graphwar_long_press_animation::draw_plan`]） | **观测**（算术）+ **推断**（形参对应） |
//! | 点数 / 抗锯齿 | `esi = 0x1e`（30）、`ecx = 0`（false） | 直接用 `DrawCall.esi` / `DrawCall.ecx`，不另写常量 | **观测** |
//! | 颜色 | 黑 `{0,0,0,1}`（`.rodata 0x19cf70`）、白 `{1,1,1,1}`（`0x19ed00`） | 同（`Color::BLACK` / `Color::WHITE`） | **观测**；白与 `PlayerShowcase::draw` **共用同一个 `0x19ed00`** |
//! | 三次取值 | 3 次 GOT 槽调用（`p0,p1` / `s0,s1` / `n,c`），**API 名与语义未知** | 位置取 `Control::get_position()`、尺寸取 `Control::get_size()`、`n`/`c` 做成导出属性 | 原版**未知**；本类的取法是**推断** |
//! | 谁来推进动画 | **没人**：2.0.60 里本类只有 `draw`，重画由别的节点触发 | **本类加了 `_process`**（每帧累加 elapsed + `queue_redraw`），否则 headless 里它永远不动 | **我方选择** |
//! | 启动 / 停止 | `GraphwarButton::button_down` 写 `Instant::now()`、`button_up` 写哨兵 `1e9`（**别人**写 `+0x18/+0x20`） | 公开 [`GraphwarLongPressAnimation::press`] / [`GraphwarLongPressAnimation::release`]，由调用方驱动 | 写入者**观测**；本类的接口是**我方选择** |
//! | 长按阈值 | `GraphwarButton::init` 的 `Duration{0, 0x1dcd6500}` = 500 ms，与本类动画时长 0.5 s **数值相同** | 导出属性 `long_press_ms = 500`，**只做展示，不参与绘制** | **观测**（数值）/ **推断**（同一个常量） |
//!
//! ⇒ 这个类证明的是「**纯逻辑算出的两次 `draw_arc` 实参真的能被引擎接受**」，
//! **不是**"这个动画的用途已复原"（"长按进度环"这句话本身在纯逻辑模块里就标着**推断**）。
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
//! GraphwarLongPressAnimation : Control
//!   @export long_press_ms : int = 500     # 长按阈值（+0x58），只做展示
//!   @export geometry_n   : int = 0        # 第三次取值的 i32（语义未知）
//!   @export geometry_c   : float = 0.0    # 第三次取值的 f32（语义未知）
//!
//!   press() -> void                       # 等价 GraphwarButton::button_down：启动时刻 = 现在
//!   advance(delta: float) -> void         # 本类新增：手动推进 delta 秒（与 _process 同一段算术）
//!   release() -> void                     # 等价 button_up：写哨兵（动画立即停）
//!   is_running() -> bool                  # SpinState::is_running（nanos != 1e9）
//!   elapsed() -> float                    # 本类自己累加的秒数
//!   angle() -> float                      # elapsed × 4π（最近一次算出来的）
//!   spin_duration_secs() -> float         # 2π / 4π = 0.5（SPIN_DURATION_SECS）
//!   draw_call_count() / process_call_count() / arc_total() / finished_count() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用**（`BaseMut<'_, Self>` 握着 `&mut self`）⇒ 画东西那段里
//!    不能写 `self.arc_total += 2`（`E0499`）。本类把计数放在 `base` 作用域**之后**。
//! 2. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 3. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 推进状态只
//!    `queue_redraw()`，真正的 `draw_arc` 只在 `_draw` 里发。
//! 4. **"最后一次绘制"必须发生**：原版是**先画两弧、再判断是否该写哨兵**（`0x4d8485` /
//!    `0x4d84ed` 在前，`0x4d8503` 在后）⇒ 本类的 `_draw`（不是 `_process`）负责写哨兵，
//!    否则超时那一帧会被吞掉。



use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::graphwar_long_press_animation::{
    self as lpa, Color as FxColor, DrawCall, Frame, Geometry, SpinState,
};

/// 把纯逻辑的 `Color`（RGBA 四个 f32）搬成 Godot 的 `Color`。
fn color_of(c: FxColor) -> Color {
    Color::from_rgba(c.r, c.g, c.b, c.a)
}

/// 一次 `draw_arc` 的形参，**逐个**对应 `DrawCall` 的槽位。
///
/// 形参顺序照 `CanvasItem::draw_arc(center, radius, start_angle, end_angle, point_count,
/// color, width, antialiased)`；槽位对应关系是**推断**（见文件头那张表）：
///
/// ```text
/// xmm0, xmm1 = center（x, y）      xmm2 = radius
/// xmm3       = start_angle         xmm4 = end_angle
/// esi        = point_count         rdx  = color（颜色指针）
/// xmm5       = width               ecx  = antialiased
/// ```
fn arc_args(call: &DrawCall) -> ((f32, f32), f32, f32, f32, i32, FxColor, f32, bool) {
    (
        (call.xmm0, call.xmm1),
        call.xmm2,
        call.xmm3,
        call.xmm4,
        call.esi as i32,
        call.color,
        call.xmm5,
        call.ecx != 0,
    )
}

/// 把累加的秒数拆成 `(secs, nanos)`：原版的 `Instant` 是 `(u64 secs, u32 nanos)` 两个字段
/// （`+0x18` / `+0x20`），纯逻辑的 `elapsed_secs` 也按这个形状吃参数。
///
/// 用 `floor` 而不是 `as`：负数 `as u64` 会饱和成 0，而 `floor` 之后再夹一次更直白。
fn split_secs(elapsed: f64) -> (u64, u32) {
    if !(elapsed > 0.0) {
        return (0, 0);
    }
    let secs = elapsed.floor();
    let frac = elapsed - secs;
    let nanos = (frac * f64::from(lpa::NANOS_PER_SEC)).round();
    let nanos = nanos.clamp(0.0, f64::from(lpa::NANOS_PER_SEC - 1)) as u32;
    (secs as u64, nanos)
}

/// `GraphwarLongPressAnimation`：转一圈就停的 `draw_arc` 动画（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarLongPressAnimation {
    base: Base<Control>,

    /// 长按阈值（毫秒）。原版是 `GraphwarButton::init` 里的 `Duration{0, 0x1dcd6500}` = 500 ms，
    /// 与 `SPIN_LIMIT / SPIN_RATE = 0.5 s` **数值相同**（推断：同一个常量）。
    ///
    /// ⚠️ 本类**不用**它做任何判定——绘制的结束判据只有 `angle > 2π`（`0x4d8503`）。
    #[export]
    long_press_ms: i64,

    /// 第三次取值拿到的 i32（纯逻辑里叫 `n`）。语义**未知**，只喂给
    /// [`lpa::geometry`] 的 `branch_positive` 判据。默认 `0` ⇒ `(0/2 as f32) > 0.0` 为假。
    #[export]
    geometry_n: i64,

    /// 第三次取值拿到的 f32（纯逻辑里叫 `c`）。语义**未知**，同上。
    #[export]
    geometry_c: f64,

    /// 动画状态：`+0x18`（secs）/ `+0x20`（nanos）。
    ///
    /// 原版由 `GraphwarButton::button_down` 写 `Instant::now()`、`button_up` 写哨兵 `1e9`；
    /// 本类用 `started(0, 0)`（把"零点"当启动时刻）配自己累加的 `elapsed`，等价地走同一条算术。
    spin: SpinState,

    /// 自己累加的 elapsed 秒（替代 `Instant::elapsed()`：headless 里 `delta` 是引擎给的，
    /// 用真实时钟会让验收脚本的期望值没法手算）。
    elapsed: f64,

    /// 最近一次算出来的角度（`elapsed × 4π`）。观测点。
    last_angle: f64,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// 累计发出过几条 `draw_arc`（每次绘制恒 2 条）。
    arc_total: i64,
    /// 动画**自己**结束过几次（`angle > 2π` 那条分支）。
    finished_count: i64,
}

#[godot_api]
impl IControl for GraphwarLongPressAnimation {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            long_press_ms: 500,
            geometry_n: 0,
            geometry_c: 0.0,
            // 初值 = 哨兵：没在动画（原版 `GraphwarButton::init` 写哨兵）。
            spin: SpinState::STOPPED,
            elapsed: 0.0,
            last_angle: 0.0,
            draw_calls: 0,
            process_calls: 0,
            arc_total: 0,
            finished_count: 0,
        }
    }

    /// `_process`：**本类新增的**（原版 2.0.60 里本模块只有 `draw`）。
    ///
    /// 只做两件事：推进 `elapsed`、要一次重画。**不**在这里判结束——原版是先画两弧、
    /// 再判 `angle > 2π`（`0x4d8485`/`0x4d84ed` 在前，`0x4d8503` 在后），所以哨兵写在
    /// [`IControl::draw`] 里，否则超时那一帧会被吞掉。
    fn process(&mut self, delta: f64) {
        self.process_calls += 1;
        if !self.spin.is_running() {
            return;
        }
        self.step_elapsed(delta);
        self.base_mut().queue_redraw();
    }

    /// `_draw`：原版 `GraphwarLongPressAnimation::draw`（vaddr `0x4d8100`，1232 B）。
    ///
    /// 顺序与指令一致：先判 `is_running`（`0x4d8110`，`nanos == 1e9` 直接返回），
    /// 再算 elapsed/angle 画**两条弧**（`0x4d8485` 黑、`0x4d84ed` 白），
    /// 最后判 `angle > 2π` 并写哨兵（`0x4d8503` / `0x4d850d`）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let size = self.base().get_size();
        let pos = self.base().get_position();
        let geometry: Geometry = lpa::geometry(
            (pos.x, pos.y),
            (size.x, size.y),
            self.geometry_n as i32,
            self.geometry_c as f32,
        );

        let (secs, nanos) = split_secs(self.elapsed);
        let frame = lpa::draw_frame(self.spin, secs, nanos, geometry);
        let finished = matches!(frame, Frame::Finished(_));

        match frame {
            // `0x4d8115`：没在动画 ⇒ 一个图元都不发。
            Frame::Idle => {}
            Frame::Drawing(plan) | Frame::Finished(plan) => {
                for call in &plan {
                    let (center, radius, start, end, points, color, width, antialiased) =
                        arc_args(call);
                    // `base_mut()` 握着 `&mut self` ⇒ 计数只能在作用域外累加（文件头第 1 条）。
                    self.base_mut()
                        .draw_arc_ex(
                            Vector2::new(center.0, center.1),
                            radius,
                            start,
                            end,
                            points,
                            color_of(color),
                        )
                        .width(width)
                        .antialiased(antialiased)
                        .done();
                }
                self.arc_total += plan.len() as i64;
            }
        }

        if finished {
            // `0x4d850d`：只写 nanos = 1e9（secs 原样保留）。
            self.spin = self.spin.stopped();
            self.finished_count += 1;
        }
    }
}

#[godot_api]
impl GraphwarLongPressAnimation {
    /// 启动动画：等价 `GraphwarButton::button_down` 写 `Instant::now()`。
    ///
    /// 本类把"零点"当启动时刻（`started(0, 0)`），`elapsed` 清零后由 `_process` 累加。
    #[func]
    fn press(&mut self) {
        self.spin = SpinState::started(0, 0);
        self.elapsed = 0.0;
        self.last_angle = 0.0;
        self.base_mut().queue_redraw();
    }

    /// 手动推进 `delta` 秒（等价 `_process` 每帧做的那一段），然后要一次重画。
    ///
    /// **本类新增的观测口**：headless 的 `--script` 模式里帧间隔由引擎给，验收脚本没法
    /// 手算出"跑 37 帧正好 0.6 s"，所以把[推进]这一段单独暴露出来。
    /// 它调的是与 `_process` **同一个** `step_elapsed`，没有第二份算术。
    #[func]
    fn advance(&mut self, delta: f64) {
        self.step_elapsed(delta);
        self.base_mut().queue_redraw();
    }

    /// 推进 `elapsed` 并重算角度（`_process` 与 `advance` 共用这一段）。
    ///
    /// 负 `delta` 不倒退（引擎不会给，但别让累加器往回走）。
    fn step_elapsed(&mut self, delta: f64) {
        if !self.spin.is_running() {
            return;
        }
        self.elapsed += delta.max(0.0);
        let (secs, nanos) = split_secs(self.elapsed);
        self.last_angle = lpa::spin_angle(lpa::elapsed_secs(self.spin, secs, nanos));
    }

    /// 停掉动画：等价 `GraphwarButton::button_up` 写哨兵（`nanos = 1e9`）。
    #[func]
    fn release(&mut self) {
        self.spin = self.spin.stopped();
    }

    /// 现在还在动画里吗（`SpinState::is_running`：`nanos != 1e9`）。
    #[func]
    fn is_running(&self) -> bool {
        self.spin.is_running()
    }

    /// 本类自己累加的秒数。
    #[func]
    fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// 最近一次算出来的角度（弧度）；未启动时是 `0.0`。
    #[func]
    fn angle(&self) -> f64 {
        self.last_angle
    }

    /// 动画时长（秒）= `SPIN_LIMIT / SPIN_RATE` = `2π / 4π` = `0.5`。
    #[func]
    fn spin_duration_secs(&self) -> f64 {
        lpa::SPIN_DURATION_SECS
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

    /// 累计发出过几条 `draw_arc`。
    #[func]
    fn arc_total(&self) -> i64 {
        self.arc_total
    }

    /// 动画**自己**结束过几次（`angle > 2π` 那条分支）。
    #[func]
    fn finished_count(&self) -> i64 {
        self.finished_count
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 时长 = `2π / 4π` = 0.5 s，与 500 ms 长按阈值**数值相同**（推断：同一个常量）。
    #[test]
    fn spin_duration_is_half_a_second() {
        assert!((lpa::SPIN_DURATION_SECS - 0.5).abs() < 1e-12);
        assert!((lpa::SPIN_LIMIT / lpa::SPIN_RATE - 0.5).abs() < 1e-12);
        assert!((lpa::SPIN_LIMIT - std::f64::consts::TAU).abs() < 1e-12);
    }

    /// 哨兵是 `nanos = 1e9`：`STOPPED` 不跑，`started` 跑，`stopped()` 只改 nanos。
    #[test]
    fn sentinel_nanos_marks_the_stopped_state() {
        assert!(!SpinState::STOPPED.is_running());
        let s = SpinState::started(7, 123);
        assert!(s.is_running());
        let off = s.stopped();
        assert!(!off.is_running());
        assert_eq!(off.secs, 7, "stopped() 只写 nanos");
        assert_eq!(off.nanos, lpa::SENTINEL_NANOS);
    }

    /// `_draw` 的两条弧：黑 `(-0.2 → angle+0.2)`、白 `(0 → angle)`，点数 30、不抗锯齿。
    ///
    /// 这是验收脚本里「arc_total == 2」那条断言在 Rust 侧的对应物。
    #[test]
    fn draw_plan_is_two_arcs_with_the_documented_arguments() {
        let g = lpa::geometry((10.0, 20.0), (60.0, 90.0), 0, 0.0);
        let plan = lpa::draw_plan(g, 1.5);
        assert_eq!(plan.len(), 2);
        for call in &plan {
            assert_eq!(call.esi, 0x1e, "点数是 0x1e = 30（观测）");
            assert_eq!(call.ecx, 0, "antialiased = 0（观测）");
            // xmm0/xmm1 = center、xmm2 = radius —— 两次绘制共用。
            assert_eq!(call.xmm0, g.v8);
            assert_eq!(call.xmm1, g.y);
            assert_eq!(call.xmm2, g.v10);
        }
        // 黑：角度两端各外扩 0.2，线宽按 0.2 那圈加粗。
        assert!((plan[0].xmm3 - (-0.2)).abs() < 1e-6);
        assert!((plan[0].xmm4 - 1.7).abs() < 1e-5);
        assert_eq!(plan[0].color, FxColor::BLACK);
        // 白：从 0 扫到 angle，线宽是 v4c。
        assert_eq!(plan[1].xmm3, 0.0);
        assert!((plan[1].xmm4 - 1.5).abs() < 1e-6);
        assert!((plan[1].xmm5 - g.v4c).abs() < 1e-6);
        assert_eq!(plan[1].color, FxColor::WHITE);
    }

    /// `arc_args` 是**纯搬运**：`DrawCall` 的 8 个槽一个不丢、一个不错位。
    #[test]
    fn arc_args_maps_every_slot_without_reordering() {
        let call = DrawCall {
            xmm0: 1.0,
            xmm1: 2.0,
            xmm2: 3.0,
            xmm3: 4.0,
            xmm4: 5.0,
            xmm5: 6.0,
            color: FxColor::WHITE,
            esi: 0x1e,
            ecx: 1,
        };
        let (center, radius, start, end, points, color, width, aa) = arc_args(&call);
        assert_eq!(center, (1.0, 2.0));
        assert_eq!(radius, 3.0);
        assert_eq!(start, 4.0);
        assert_eq!(end, 5.0);
        assert_eq!(points, 30);
        assert_eq!(color, FxColor::WHITE);
        assert_eq!(width, 6.0);
        assert!(aa, "ecx != 0 ⇒ antialiased");
    }

    /// `split_secs` 把累加的 f64 拆成 `(secs, nanos)`，且与 `elapsed_secs` 往返一致。
    #[test]
    fn split_secs_round_trips_through_elapsed_secs() {
        for e in [0.0_f64, 0.25, 0.5, 1.0, 1.75] {
            let (secs, nanos) = split_secs(e);
            let back = lpa::elapsed_secs(SpinState::started(0, 0), secs, nanos);
            assert!((back - e).abs() < 1e-6, "e={e} back={back}");
        }
        assert_eq!(split_secs(-1.0), (0, 0), "负值夹到 0");
        // 1e9 ns 是哨兵值，绝不能出现在"正在跑"的状态里。
        let (_, nanos) = split_secs(0.999_999_999_9);
        assert!(nanos < lpa::NANOS_PER_SEC, "nanos={nanos} 不得等于哨兵");
    }

    /// 0.5 s 正好越过终点：`angle = 0.5 × 4π = 2π`，而判据是**严格大于**。
    #[test]
    fn finish_threshold_is_strictly_greater_than_two_pi() {
        assert!(!lpa::is_finished(lpa::spin_angle(lpa::SPIN_DURATION_SECS)));
        assert!(lpa::is_finished(lpa::spin_angle(lpa::SPIN_DURATION_SECS + 1e-6)));
        let g = lpa::geometry((0.0, 0.0), (10.0, 30.0), 0, 0.0);
        assert!(matches!(
            lpa::draw_frame(SpinState::STOPPED, 0, 0, g),
            Frame::Idle
        ));
        assert!(matches!(
            lpa::draw_frame(SpinState::started(0, 0), 1, 0, g),
            Frame::Finished(_)
        ));
        assert!(matches!(
            lpa::draw_frame(SpinState::started(0, 0), 0, 0, g),
            Frame::Drawing(_)
        ));
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
