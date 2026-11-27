//! `graphwar_button` —— 原版 `src/graphwar_button.rs` 的还原（**纯逻辑层**）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。每一个条目都能指到
//! **反汇编地址**或**符号表真名**；指不到的写 `未知` 并说明缺什么证据。
//!
//! **地址口径**：本节所有地址一律是 **ELF 虚地址**（`objdump` 口径）；
//! Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。
//!
//! ## 0. 这个模块是什么（一句话 + 证据档）
//!
//! `GraphwarButton` 是 `Button` 的子类（实现 `IButton`，见 [`TRAIT_IMPLS`]），
//! 长得像**虚拟键盘上的一个键**：点一下 → 把自己的 `input_text` 发给订阅者；
//! 按住不放 → 把 `long_press_input_text` 发出去（只发一次）；
//! 同时驱动一个**长按动画节点**（`GraphwarLongPressAnimation`）。
//! 〔类型与接口 = 观测（arm64 `.symtab` + x86 `.rodata`）；
//! `ready`/`process`/`pressed` 的字段选择 = 观测；
//! 「虚拟键盘」这个**用途** = 推断，缺的证据是订阅者注册点〕
//!
//! ## 1. 名字从哪来（**本轮最大的杠杆**：2.0.60 的 arm64 构建没 strip）
//!
//! `fixtures/name_to_addr.tsv` 里本模块的方法名全是 `歧义` 或 `未知`；
//! 它的候选地址经反汇编核对**全是错的**（见 [`NAME_BRIDGE_CONFLICTS`]——
//! 那些函数体内引用的 panic 锚是 `src/graphwar_root_node.rs` 的行号）。
//!
//! 真名来自**同一版本（2.0.60）的 arm64 `.so` 的 `.symtab`**：
//!
//! ```text
//! .reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so
//! readelf -sW libgraphwarrust.so | c++filt      # 见 ARM64_SYMS
//! ```
//!
//! 两份 2.0.60 构建同源，所以**函数个数、名字、调用结构**可比；
//! arm64 地址只用来**定名**，绝不当作 x86 地址使用。
//!
//! ## 2. x86 侧怎么定位（**观测**；每条都给判据）
//!
//! 六个方法的 x86 位置。判据列里的「大小比」= x86 字节数 / arm64 字节数，
//! 六个都在 **1.03–1.07**（实测 1.029/1.032/1.048/1.064/1.067/1.047），没有例外：
//!
//! ```text
//! 方法          x86 vaddr   大小   arm64  判据
//! init          0x4e3e00    288    280   逐字段写初值，含 [self+0x68]=0x3b9aca00、
//!                                       [self+0x58]=0x1dcd6500；紧挨 ready/process
//! ready         0x4e3f20    896    868   唯一同时 lea "pressed"/"button_up"/
//!                                       "button_down"/"AnimationRect" 的函数，
//!                                       并含 "Upcast failed. This is a bug; please report it."
//! process       0x4e42a0    352    336   cmp/mov [r+0x68],0x3b9aca00 + GString::is_empty(+0x90)
//! button_down   0x4ef390    464    436   全库只有它与 button_up 引用
//!                                       type_name::<GraphwarLongPressAnimation>()（arm64 同样只有这两个）
//! pressed       0x4ef560    320    300   [self+0x48] 当长度、[self+0x40] 当指针、步长 0x10
//!                                       的循环 + GString::fmt(self+0x88)
//! button_up     0x4ef6a0    448    428   开头 mov dword [r+0x68],0x3b9aca00，其余与 button_down 同形
//! ```
//!
//! ## 3. 方法名 ↔ 分派表（**观测**）
//!
//! gdext 的 `#[func]` 与属性存取器**不产生 Rust 符号**，而是「按名字分派」：
//! 每个可调用入口是一个小函数，体内 `lea` 出 `(类名, 方法名)` 两对 `&str`
//! （`(ptr,len)` 全部自校验过）。本模块 16 个入口实测如下：
//!
//! ```text
//! x86 vaddr              大小  类名              方法名
//! 0x4e4400 / 0x4e47f0    192/240  GraphwarButton  get_input_text
//! 0x4e44c0 / 0x4e4700    192/240  GraphwarButton  set_long_press_input_text
//! 0x4e4580 / 0x4e49d0    192/240  GraphwarButton  set_input_text
//! 0x4e4640 / 0x4e48e0    192/240  GraphwarButton  get_long_press_input_text
//! 0x4e4d40               192      GraphwarButton  process
//! 0x4e4e00               192      GraphwarButton  ready
//! 0x4ef860 / 0x4efaa0    192/240  GraphwarButton  button_down
//! 0x4ef920 / 0x4efc81    192/239  GraphwarButton  button_up
//! 0x4ef9e0 / 0x4efb90    192/240  GraphwarButton  pressed
//! ```
//!
//! 成对的 192/240 字节是同一模板的两个实例化（ptrcall / varcall），
//! 差别只在尾部（多一次 drop）⇒ **推断**：接收者所有权不同（`&Self` vs `Self`）；
//! 缺的证据：两份调用点的 Rust 签名。
//!
//! `0x4e4ac0`（640 B）是**属性表**（[`FN_REGISTER_EXPORTS`]）；
//! `0x5009f0`（368 B）按名字分派 `pressed`/`button_up`/`button_down`
//! （长度关 + 前 8 字节立即数比较，[`FN_NAME_DISPATCH`]）。
//!
//! ## 4. 状态机（**观测**：偏移 + 立即数；字段名是推断）
//!
//! `init`（`0x4e3e00`，写初值的那一段在 `0x4e3e5a`–`0x4e3eb3`）逐字段写出：
//!
//! ```text
//! [self+0x38] = {0, 8}          Vec<Box<dyn FnMut(&str)>>（cap=0，悬垂指针 8）
//! [self+0x48] = 0                 长度 0
//! [self+0x50] = 0              ┐ Duration
//! [self+0x58] = 0x1dcd6500     ┘ = 500_000_000 ns = 0.5 s      ← 长按阈值
//! [self+0x68] = 0x3b9aca00       Instant.nanos = 1e9           ← "没按下"哨兵
//! [self+0x88] = GString("")      input_text
//! [self+0x90] = GString("")      long_press_input_text
//! [self+0x18] = 0                Option<Gd<GraphwarLongPressAnimation>> = None
//! ```
//!
//! 四个动作（逐句对拍 arm64 同名函数）：
//!
//! ```text
//! button_down  0x4ef39d  call Instant::now → [self+0x60]=secs,[self+0x68]=nanos
//!              0x4ef3aa  cmp dword [self+0x18],1 ; jne 返回       ; 没接动画节点就到此为止
//!              0x4ef3e2  call Instant::now → [lpa+0x18]=secs,[lpa+0x20]=nanos
//!              0x4ef420  base_mut() → queue_redraw()
//! button_up    0x4ef6aa  mov dword [self+0x68],0x3b9aca00          ; 哨兵（在判别**之前**）
//!              0x4ef6e9  mov dword [lpa +0x20],0x3b9aca00
//!              同上 base_mut() → queue_redraw()
//! process      0x4e42bb  GString::is_empty(self+0x90) → 空则返回
//!              0x4e42c9  cmp dword [self+0x68],0x3b9aca00 → 已释放则返回
//!              0x4e42d7  mov dword [self+0x68],0x3b9aca00         ; 一次性（在派发**之前**）
//!              0x4e42df  循环 self+0x40/+0x48，用 GString(self+0x90) 逐个回调
//! pressed      0x4ef560  同形循环，但格式化的字段是 self+0x88（不改状态）
//! ```
//!
//! ⇒ **`pressed` 发 `input_text`；`process` 发 `long_press_input_text`，且只发一次。**
//! `0.5 s` 这个阈值 `process` **没有读**（它只判「还按着」）；
//! 阈值的消费者是长按动画（`draw` 里 `elapsed > 0.5`，见
//! `graphwar_long_press_animation::SPIN_DURATION_SECS`）〔推断：两者是同一个 0.5 s；
//! 缺的证据：把 0.5 s 交给动画节点的那条指令〕
//!
//! ## 5. `ready`：3 条信号 + 1 次按名找节点（**观测**）
//!
//! ```text
//! 0x4e3f4e  lea "pressed"       + 0x4e3f5d  len 7   → Callable::from_object_method → connect
//! 0x4e3fc7  lea "button_up"     + 0x4e3fd6  len 9   → 同上
//! 0x4e4040  lea "button_down"   + 0x4e404f  len 11  → 同上
//! 0x4e410d  lea "AnimationRect" (13 B)              → GraphwarNodes::get_node_by_name_rec
//! ```
//!
//! 信号名与「被连的方法名」在原版里是**同一个 **`&str`（同一地址，见 [`SIGNAL_LINKS`]）
//! ⇒ `connect("pressed", Callable::from_object_method(self, "pressed"))`。
//! 随后在 `get_parent()` 下按名字找 `"AnimationRect"` 并 cast 成
//! `GraphwarLongPressAnimation`（arm64 `ready` 的 `check_rtti` 目标类型名 = 71 字节的
//! `graphwarrust::graphwar_long_press_animation::GraphwarLongPressAnimation`，
//! 在 x86 侧对应 `.rodata` `0x1c1c13`）。
//!
//! ## 6. 本节没解出来的（照实写）
//!
//! * 4 个属性存取器的**函数体**：它们只经 `(类名,方法名)` 分派，静态拿不到函数指针
//!   ⇒ [`PROPERTIES`] 只记名字与入口地址，**没有**记实现。
//! * `[self+0x00]`、`[self+0x10..0x18]`、`[self+0x28..0x38]`、`[self+0x70..0x88]`
//!   这几段在 `init` 里有写入、本模块 6 个方法里没读到 ⇒ 字段名与用途**未知**。
//! * `0.5 s` 阈值（`+0x50/+0x58`）在本模块 6 个方法里**没有任何读者**
//!   （已按立即数与偏移全函数搜索）⇒ 推断它经属性/参数传给动画节点；缺的证据：那个传递点。
//! * 订阅者 `Vec<Box<dyn FnMut(&str)>>` 由谁 `push` **未知**（6 个方法里没有 push）。

/// 原版函数表的一行。
///
/// 地址一律 **ELF 虚地址**（`objdump` 口径）；Ghidra/REA 地址 = 本值 + `0x100000`。
#[derive(Debug, Clone, Copy)]
pub struct OrigFn {
    /// 源码级名字（`GraphwarButton::x` 或 `<GraphwarButton as IButton>::x`）。
    pub name: &'static str,
    /// x86-64 2.0.60 的 ELF 虚地址。
    pub vaddr: u32,
    /// 函数体字节数（相邻 `funcstart` 之差）。
    pub size: u32,
    /// 该函数引用的 `src/graphwar_button.rs` panic 锚（`line:col`）。
    ///
    /// 这 6 个方法**一个都没有** —— 它们没有 panic 点，这正是
    /// `name_to_addr.tsv` 无法把它们钉住的原因。
    pub anchors: &'static [(u32, u32)],
    /// 定名依据（一条一句，可复核）。
    pub evidence: &'static str,
}

/// **本模块 6 个游戏方法**：名字 = arm64 2.0.60 `.symtab`；地址 = x86 `objdump`。
pub const ORIG_FUNCS: &[OrigFn] = &[
    OrigFn {
        name: "GraphwarButton::init",
        vaddr: 0x4e3e00,
        size: 288,
        anchors: &[],
        evidence: "写 [self+0x68]=0x3b9aca00 / [self+0x58]=0x1dcd6500；288 B vs arm64 280 B",
    },
    OrigFn {
        name: "GraphwarButton::ready",
        vaddr: 0x4e3f20,
        size: 896,
        anchors: &[],
        evidence: "lea \"pressed\"/\"button_up\"/\"button_down\"/\"AnimationRect\"；896 B vs arm64 868 B",
    },
    OrigFn {
        name: "GraphwarButton::process",
        vaddr: 0x4e42a0,
        size: 352,
        anchors: &[],
        evidence: "cmp/mov [self+0x68],0x3b9aca00 + GString::is_empty(self+0x90)；352 B vs arm64 336 B",
    },
    OrigFn {
        name: "GraphwarButton::button_down",
        vaddr: 0x4ef390,
        size: 464,
        anchors: &[],
        evidence: "引 type_name::<GraphwarLongPressAnimation>()（全库仅 2 处）；Instant::now 写 self+0x60",
    },
    OrigFn {
        name: "GraphwarButton::pressed",
        vaddr: 0x4ef560,
        size: 320,
        anchors: &[],
        evidence: "[self+0x48] 当长度 / [self+0x40] 当指针、步长 0x10 的循环 + GString::fmt(self+0x88)",
    },
    OrigFn {
        name: "GraphwarButton::button_up",
        vaddr: 0x4ef6a0,
        size: 448,
        anchors: &[],
        evidence: "开头 mov dword [r+0x68],0x3b9aca00；其余与 button_down 同形；448 B vs arm64 428 B",
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

/// 本模块在 arm64 2.0.60 里的符号：3 个游戏方法 + 3 个虚函数 + 注册数据。
pub const ARM64_SYMS: &[ArmSym] = &[
    ArmSym {
        name: "graphwarrust::graphwar_button::GraphwarButton::button_down",
        addr: 0x448470,
        size: 436,
    },
    ArmSym {
        name: "graphwarrust::graphwar_button::GraphwarButton::pressed",
        addr: 0x448624,
        size: 300,
    },
    ArmSym {
        name: "graphwarrust::graphwar_button::GraphwarButton::button_up",
        addr: 0x448750,
        size: 428,
    },
    ArmSym {
        name: "<GraphwarButton as IButton>::init",
        addr: 0x43eb08,
        size: 280,
    },
    ArmSym {
        name: "<GraphwarButton as IButton>::ready",
        addr: 0x43ec20,
        size: 868,
    },
    ArmSym {
        name: "<GraphwarButton as IButton>::process",
        addr: 0x43ef84,
        size: 336,
    },
    ArmSym {
        name: "<GraphwarButton as GodotClass>::class_id",
        addr: 0x438050,
        size: 56,
    },
    ArmSym {
        name: "GraphwarButton::__registration_storage::STORAGE",
        addr: 0x8b4140,
        size: 56,
    },
];

/// 按名字查 [`ARM64_SYMS`]（后缀匹配，便于写短名）。
pub fn arm_sym(name: &str) -> Option<&'static ArmSym> {
    ARM64_SYMS.iter().find(|s| s.name.ends_with(name))
}

/// `GraphwarButton` 实现的 gdext trait。
///
/// 依据：`docs/02-module-map.md` §18.3 A 表的最后一列 + arm64 符号路径逐条相符。
pub const TRAIT_IMPLS: &[&str] = &[
    "IButton",
    "GodotClass",
    "ImplementsGodotApi",
    "ImplementsGodotExports",
    "ImplementsGodotVirtual",
];

/// 类名（Godot 侧注册名）。
pub const CLASS: &str = "GraphwarButton";

/// `.rodata` 中 `"GraphwarButton"` 的地址（14 字节，`lea` 立即数逐条量到）。
pub const CLASS_NAME_VADDR: u32 = 0x1c40dc;

/// 返回类名 `&str` 的两个小函数（55 / 64 字节，体内 `lea 0x1c40dc` + `len 14`）。
pub const CLASS_NAME_FNS: &[u32] = &[0x4ff890, 0x500430];

/// 本模块的 3 个 panic 锚：`(源码位置, line, col, 引用它的 x86 函数)`。
///
/// **x86 与 arm64 取值完全相同** —— `core::panic::Location` 是编译期常量，
/// 与指令集无关。只有 gdext 样板引用它们，游戏侧 6 个方法一个都不引用。
pub const SRC_ANCHORS: &[(&str, u32, u32, &[u32])] = &[
    ("src/graphwar_button.rs:14:10", 14, 10, &[0x4e8020]),
    (
        "src/graphwar_button.rs:83:1",
        83,
        1,
        &[0x4e0e60, 0x4e0fe0, 0x4e8230, 0x4e83d0],
    ),
    ("src/graphwar_button.rs:111:1", 111, 1, &[0x4e8580]),
];

/// `fixtures/name_to_addr.tsv` 里本模块 `歧义` 候选，**逐条被反汇编否掉**。
///
/// 三元组 = `(方法名, 候选地址, 否掉的理由)`。判据都是「该候选函数体引用的
/// panic 锚属于**别的源文件**」⇒ 它不可能是 `src/graphwar_button.rs` 里的方法。
pub const NAME_BRIDGE_CONFLICTS: &[(&str, u32, &str)] = &[
    (
        "GraphwarButton::button_down",
        0x3afb00,
        "引 src/graphwar_root_node.rs:892:35 与 894:28",
    ),
    (
        "GraphwarButton::button_down",
        0x3afe90,
        "引 src/graphwar_root_node.rs:868:35 与 870:28",
    ),
    (
        "GraphwarButton::button_up",
        0x3afb00,
        "同上（root_node 的两个帧回调）",
    ),
    (
        "GraphwarButton::ready",
        0x3a2e10,
        "graphwar_root_node 的 _ready（docs/16-client.md §15.4）",
    ),
    (
        "GraphwarButton::process",
        0x3a5760,
        "graphwar_root_node 的状态机（docs/16-client.md §15.7）",
    ),
    (
        "GraphwarButton::init",
        0x920850,
        "gdext 的通用初始化样板，非本类",
    ),
];

/// 1 秒 = `1_000_000_000` ns。二进制里的立即数是 `0x3b9aca00`
/// （`0x4e3e8e` / `0x4e42c9` / `0x4e42d7` / `0x4ef6aa` / `0x4ef6e9` 五处）。
pub const NANOS_PER_SEC: u32 = 1_000_000_000;

/// 长按阈值 = `0.5 s`。
///
/// `init` 在 `0x4e3e95`/`0x4e3e9d` 写 `[self+0x50]=0`、`[self+0x58]=0x1dcd6500`
/// ⇒ `Duration { secs: 0, nanos: 500_000_000 }`。
pub const LONG_PRESS_HOLD_NANOS: u32 = 0x1dcd_6500;

/// `Instant` 的两个半区（`+0x60` secs / `+0x68` nanos）。
///
/// 「没按下」用 **nanos = 1e9** 表示（超出合法纳秒范围），这是原版的哨兵写法，
/// **不是** `Option`：`init` 先写哨兵，`button_up`/`process` 再写回哨兵。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stamp {
    /// 秒（`[+0x60]`，x86 上是 `mov qword`）。
    pub secs: u64,
    /// 纳秒（`[+0x68]`，x86 上是 `mov dword`）。
    pub nanos: u32,
}

impl Stamp {
    /// 哨兵值：`nanos = 1e9`（`0x3b9aca00`）。
    pub const RELEASED_NANOS: u32 = NANOS_PER_SEC;

    /// 初始值（`init` 写进 `[self+0x68]` 的那个）。
    pub const NONE: Stamp = Stamp {
        secs: 0,
        nanos: Self::RELEASED_NANOS,
    };

    /// 记录一次 `Instant::now()`（`button_down` 的 `0x4ef3a3`/`0x4ef3a7`）。
    pub const fn now(secs: u64, nanos: u32) -> Stamp {
        Stamp { secs, nanos }
    }

    /// 是否处于「按着」状态（`cmp dword [r+0x68],0x3b9aca00` 的否定）。
    pub const fn is_pressed(self) -> bool {
        self.nanos != Self::RELEASED_NANOS
    }

    /// 释放：**只改 nanos**（`0x4ef6aa` / `0x4e42d7` 都是 `mov dword`，secs 不动）。
    pub const fn released(self) -> Stamp {
        Stamp {
            secs: self.secs,
            nanos: Self::RELEASED_NANOS,
        }
    }
}

/// 一条 `connect(signal, Callable::from_object_method(self, method))`。
///
/// 原版里 `signal` 与 `method` 是**同一个 **`&str`（同一 `.rodata` 地址）。
#[derive(Debug, Clone, Copy)]
pub struct SignalLink {
    /// 名字（既是信号名，也是被连的方法名）。
    pub name: &'static str,
    /// `.rodata` 地址。
    pub vaddr: u32,
    /// 长度（自校验：`vaddr` 处正好这么多字节）。
    pub len: u8,
}

/// `ready` 里连的 3 条信号（**观测**，顺序即指令顺序）。
///
/// 三个 `lea` 在 `0x4e3f4e`/`0x4e3fc7`/`0x4e4040`，长度立即数在
/// `0x4e3f5d`/`0x4e3fd6`/`0x4e404f`。
pub const SIGNAL_LINKS: &[SignalLink] = &[
    SignalLink {
        name: "pressed",
        vaddr: 0x1c4258,
        len: 7,
    },
    SignalLink {
        name: "button_up",
        vaddr: 0x1c425f,
        len: 9,
    },
    SignalLink {
        name: "button_down",
        vaddr: 0x1c4268,
        len: 11,
    },
];

/// `ready` 里按名字找的**节点名**（`lea` 在 `0x4e410d`）。
pub const NODE_ANIMATION_RECT: &str = "AnimationRect";

/// 节点名的 `.rodata` 地址（13 字节）。
pub const NODE_ANIMATION_RECT_VADDR: u32 = 0x1c4273;

/// 一个导出的属性（gdext `#[var]`），带 getter/setter 名与它们的**分派入口**。
#[derive(Debug, Clone, Copy)]
pub struct Property {
    /// 属性名。
    pub name: &'static str,
    /// 属性名 `.rodata` 地址。
    pub name_vaddr: u32,
    /// getter 名。
    pub getter: &'static str,
    /// setter 名。
    pub setter: &'static str,
    /// getter 的两个分派入口（ptrcall / varcall）。
    pub getter_vcall: (u32, u32),
    /// setter 的两个分派入口（ptrcall / varcall）。
    pub setter_vcall: (u32, u32),
}

/// 两个导出属性（`0x4e4ac0` 的 24 个 `(str,len)` 立即数逐对还原）。
pub const PROPERTIES: &[Property] = &[
    Property {
        name: "input_text",
        name_vaddr: 0x1c42ce,
        getter: "get_input_text",
        setter: "set_input_text",
        getter_vcall: (0x4e4400, 0x4e47f0),
        setter_vcall: (0x4e4580, 0x4e49d0),
    },
    Property {
        name: "long_press_input_text",
        name_vaddr: 0x1c42d8,
        getter: "get_long_press_input_text",
        setter: "set_long_press_input_text",
        getter_vcall: (0x4e4640, 0x4e48e0),
        setter_vcall: (0x4e44c0, 0x4e4700),
    },
];

/// 按名字查 [`PROPERTIES`]。
pub fn property(name: &str) -> Option<&'static Property> {
    PROPERTIES.iter().find(|p| p.name == name)
}

/// 属性表构造器（`__register_exports` 的位置）：640 B。
pub const FN_REGISTER_EXPORTS: u32 = 0x4e4ac0;

/// 3 个信号方法名的分派器：368 B（长度关 + 前 8 字节立即数比较）。
pub const FN_NAME_DISPATCH: u32 = 0x5009f0;

/// `long_press_input_text` 字段偏移（`process` 的 `lea rbx,[rdi+0x90]`）。
pub const OFF_LONG_PRESS_TEXT: u32 = 0x90;

/// `input_text` 字段偏移（`pressed` 里的 `add r?,0x88`）。
pub const OFF_INPUT_TEXT: u32 = 0x88;

/// 订阅者 `Vec` 的 `(指针, 长度)` 偏移（`[r+0x40]` / `[r+0x48]`）。
pub const OFF_SUBSCRIBERS: (u32, u32) = (0x40, 0x48);

/// `Option<Gd<GraphwarLongPressAnimation>>` 的判别位偏移（`cmp dword [r+0x18],1`）。
pub const OFF_ANIMATION_TAG: u32 = 0x18;

/// `Instant` 的偏移（`+0x60` secs / `+0x68` nanos）。
pub const OFF_PRESS_STAMP: u32 = 0x60;

/// `Duration`（长按阈值）的偏移（`+0x50` secs / `+0x58` nanos）。
pub const OFF_HOLD_DURATION: u32 = 0x50;

/// 一次按下/抬起会产生的可观察效果。
///
/// 刻意写成**纯数据**：原版的动作都是「写自己的字段 + 写动画节点的字段 +
/// 请求重绘」，交给 Godot 的那部分（`bind_mut`/`queue_redraw`）只作为**意图**出现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `[self+0x60..0x68] = now`（`0x4ef3a3`）。
    RecordPress(Stamp),
    /// `[lpa+0x18..0x20] = now`（`0x4ef3e8`）。
    StartAnimation(Stamp),
    /// `[lpa+0x20] = 1e9`（`0x4ef6e9`）。
    StopAnimation,
    /// `base_mut()` 之后的一次重绘请求（arm64 侧内联成
    /// `CanvasItem::queue_redraw`，`0x4484fc`）。
    QueueRedraw,
}

/// `GraphwarButton` 的状态（偏移取自反汇编，字段名是**推断**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonState {
    /// `[self+0x18]`：是否已经拿到动画节点（arm64 上是 `Some(Gd<…>)`）。
    pub animation_linked: bool,
    /// `[self+0x60..0x68]`：最近一次按下时刻。
    pub press: Stamp,
    /// `[self+0x50..0x58]`：长按阈值（`Duration`，装着 [`LONG_PRESS_HOLD_NANOS`]）。
    pub hold: Stamp,
    /// `[self+0x88]`。
    pub input_text: String,
    /// `[self+0x90]`。
    pub long_press_input_text: String,
    /// `[self+0x40..0x48]`：订阅者个数（原版是 `Vec<Box<dyn FnMut(&str)>>`）。
    pub subscribers: usize,
}

impl ButtonState {
    /// `init`（`0x4e3e00`）写出的状态，逐字段对应 §4 那张表。
    pub fn new() -> ButtonState {
        ButtonState {
            animation_linked: false,
            press: Stamp::NONE,
            hold: Stamp {
                secs: 0,
                nanos: LONG_PRESS_HOLD_NANOS,
            },
            input_text: String::new(),
            long_press_input_text: String::new(),
            subscribers: 0,
        }
    }

    /// `button_down`（`0x4ef390`）：**先**记时间，**再**看有没有动画节点。
    ///
    /// `now` = 写进 `[self+0x60..0x68]` 的那次 `Instant::now()`；
    /// `again` = 写进动画节点 `[lpa+0x18..0x20]` 的**第二次** `Instant::now()`
    /// （原版真的调了两次，`0x4ef39d` 与 `0x4ef3e2`）。
    pub fn button_down(&mut self, now: Stamp, again: Stamp) -> Vec<Effect> {
        self.press = now;
        let mut out = vec![Effect::RecordPress(now)];
        if self.animation_linked {
            out.push(Effect::StartAnimation(again));
            out.push(Effect::QueueRedraw);
        }
        out
    }

    /// `button_up`（`0x4ef6a0`）：把两处 nanos 写成哨兵，再请求重绘。
    ///
    /// 对 `self` 的那次写**在**判别之前（`0x4ef6aa` `jne` 之前），
    /// 所以即使没接动画节点，`[self+0x68]` 也会变成 1e9。
    pub fn button_up(&mut self) -> Vec<Effect> {
        self.press = self.press.released();
        let mut out = Vec::new();
        if self.animation_linked {
            out.push(Effect::StopAnimation);
            out.push(Effect::QueueRedraw);
        }
        out
    }

    /// `process`（`0x4e42a0`）：长按文本的**一次性**派发。
    ///
    /// 返回 `Some(text)` 表示这一帧应把 `text` 交给每个订阅者；
    /// 状态在返回前已经改成「已释放」（`0x4e42d7` 在派发之前）。
    /// 判定顺序**逐条照抄**原版：文本空 → 已释放 → 置哨兵 → 无订阅者。
    pub fn process(&mut self) -> Option<&str> {
        if self.long_press_input_text.is_empty() {
            return None;
        }
        if !self.press.is_pressed() {
            return None;
        }
        self.press = self.press.released();
        if self.subscribers == 0 {
            return None;
        }
        Some(&self.long_press_input_text)
    }

    /// `pressed`（`0x4ef560`）：把 `input_text` 交给每个订阅者，**不改**状态。
    pub fn pressed(&self) -> &str {
        &self.input_text
    }

    /// `ready`（`0x4e3f20`）要做的事，用纯数据表达：
    /// 3 条 `connect` + 1 次按名找节点 + 1 次 cast。
    ///
    /// 返回 `(信号表, 节点名)`；真正调 Godot 的地方不在这里（见 §5）。
    pub fn ready_plan() -> (&'static [SignalLink], &'static str) {
        (SIGNAL_LINKS, NODE_ANIMATION_RECT)
    }
}

impl Default for ButtonState {
    fn default() -> ButtonState {
        ButtonState::new()
    }
}


// ===========================================================================
// 原版方法体：原版 `GraphwarButton` 的 gdext 节点类（`Button` 子类）（T57 从 `src/graphwar_button.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `GraphwarButton` 的 gdext 节点类（`Button` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/graphwar_button.rs` 搬进 `src/graphwar_button.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `graphwar_button.rs` 里**，证据：
//! 名字证据：`src/graphwar_button.rs` 的 6 方法全解（init/ready/process/button_down/pressed/button_up），模块归属靠 `fixtures/name_to_addr_v2.tsv` 的 `_::__init::__inner_init 0x4e8020` + arm64 `.symtab` 序；⚠️ `locs 0x4e4040`(ready) / `0x4e42a0`(process) 无 Location 锚 —— 这一点记在台账 `bt-00`。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::graphwar_button::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarButton` 是 **`Button` 的子类**（`impl IButton`，见
//! [`graphwar_button::TRAIT_IMPLS`]）：长得像虚拟键盘上的一个键，点一下把自己的
//! `input_text` 发给**一张外部注册的回调表**，按住时把 `long_press_input_text` 发一次。
//! `src/graphwar_button.rs` 已经把这 6 个方法的**状态机**复原出来了，但它按约定**不碰引擎**
//! （一行 Godot 调用都没有）。本文件是"让那个状态机真的挂到节点上"的那层薄壳：
//!
//! ```text
//! src/graphwar_button.rs            纯逻辑：SIGNAL_LINKS / ButtonState / Stamp / Effect
//! src/graphwar_button.rs   ← 本文件：connect 信号、持有回调表、发信号
//! ```
//!
//! 状态机本体（`button_down` / `button_up` / `process` / `pressed` 的判定顺序）**只有一份**，
//! 仍然在 `src/graphwar_button.rs` 里；本文件不复制任何判定。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §35）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无（原版 6 个方法里没有 `emit_signal`） | `#[signal] long_press_finished()` / `input_selected(text)`，用**类型安全生成器** `self.signals().…().emit()` |
//! | 2 | **连引擎原生信号** | `ready` 里 3 条 `connect`（[`graphwar_button::SIGNAL_LINKS`]） | `self.base_mut().connect("pressed", callable(self,"pressed"))` 等 3 条 |
//! | 3 | **外部注册的闭包表** | `Vec<Box<dyn FnMut(&str)>>`（`+0x38/+0x40/+0x48`） | 字段就写这个类型（原样）；另存一份 `Vec<Callable>` 给 GDScript |
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `Button` + `IButton`（`TRAIT_IMPLS`） | 同 | **观测** |
//! | `ready` 的 3 条连接 | `pressed`/`button_up`/`button_down` → 同名方法 | 同（信号名与处理函数名是同一个 `&str`，见 [`graphwar_button::SIGNAL_LINKS`]） | **观测** |
//! | `ready` 的按名找节点 | `get_node_by_name_rec` 找 `"AnimationRect"` 并 cast 成 `GraphwarLongPressAnimation` | 用 `find_child` 找同名节点，**只记 bool**；那个类没复原，所以**不 cast、不持句柄** | 名字**观测**；行为**未做** |
//! | 订阅者表 | `Vec<Box<dyn FnMut(&str)>>`，由谁 `push` **未知** | 同一个类型；`subscribe_native_probe()` 由 Rust 侧塞一个探针闭包，`subscribe(Callable)` 给 GDScript | 类型**观测**；注册点是**我方选择** |
//! | 长按阈值 500 ms | `+0x50/+0x58` = `Duration{0, 0x1dcd6500}`，6 个方法里**没有读者** | 照抄进 `ButtonState::hold`，同样**不读**（`process` 只看"还按着"） | **观测** |
//! | 时间戳 | `Instant::now()`（`+0x60` secs / `+0x68` nanos），哨兵 = nanos 1e9 | 本实例构造时锚一个 `std::time::Instant`，之后用 `elapsed()` 造 [`Stamp`] | 语义**推断**（原版是绝对时刻，我们只要"与哨兵可比"） |
//! | 动画节点上的两个 `Instant` | `button_down`/`button_up` 会写 `[lpa+0x18..+0x20]` | **没写**（那个类没有节点实现） | **未做** |
//! | `queue_redraw` | `base_mut() → queue_redraw()` | 同（`Effect::QueueRedraw` 的消费者） | **观测** |
//! | 自造信号 | **原版没有**（`pressed`/`button_up`/`button_down` 都是 `BaseButton` 的） | 本文件加了 2 个，供宿主观察 | 我方**新增** |
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarButton : Button
//!   @export input_text : String              # 原版 +0x88
//!   @export long_press_input_text : String   # 原版 +0x90
//!
//!   signal long_press_finished()             # 自造：长按文本派发完了
//!   signal input_selected(text: String)      # 自造：点一下派发的文本
//!
//!   pressed() / button_down() / button_up()  # 原版同名方法（被 ready 里的 connect 指着）
//!   subscribe(cb: Callable) -> int           # 替代方案：把 GDScript 的 Callable 塞进表里
//!   subscribe_native_probe() -> int          # 原版形态：Rust 侧 push 一个 Box<dyn FnMut(&str)>
//!   unsubscribe_all() -> void
//!   pressed_call_count() / button_down_call_count() / button_up_call_count() -> int
//!   ready_call_count() / process_call_count() -> int
//!   long_press_emit_count() / input_selected_emit_count() -> int
//!   subscriber_count() -> int                # = Vec<Box<dyn FnMut>>.len()
//!   callable_count() -> int
//!   native_captured() / callable_captured() / dispatch_log() -> String
//!   press_stamp_is_pressed() -> bool
//!   animation_linked() -> bool
//!   reentrancy_probe() -> String             # 见 §"坑 3"
//! ```
//!
//! # 写这类"带信号"的类必须知道的五条（本轮实测）
//!
//! 1. **`Gd<Node>` 不保活**（`entity_drawer.rs` §坑 1 的同一件事）。本文件**故意不**
//!    缓存 `AnimationRect` 的句柄：我们拿它没办法（那个类没实现），缓存一个随时可能被
//!    `free()` 的野句柄只会换来 `upcast_ref: access to instance … after it has been freed`
//!    + `error code 1337` + 进程卡死。只存 `bool`。
//! 2. **`#[signal]` 的生成器要 `&mut self`**（`WithUserSignals::signals(&mut self)`）⇒
//!    发信号时**不能再借 `self` 的字段**。所有要发的值必须先算成局部变量。
//! 3. **闭包表不能捕获 `Gd<Self>`**：在 `#[func](&mut self)` 里调 `self.to_gd().bind_mut()`
//!    会撞"已经可变借用"。本文件用 `Rc<RefCell<Vec<String>>>` 这一层间接来记录，
//!    [`GraphwarButton::reentrancy_probe`] 把那条 panic **抓成返回值**（不炸进程）。
//! 4. **信号名与处理函数名可以同名**（原版 `pressed` 既是被连的信号、也是方法名），
//!    但**自造信号名不能与 `BaseButton` 的已有信号同名**（`pressed`/`button_up`/
//!    `button_down`/`toggled` 都是基类的），所以本文件自造的两个信号叫
//!    `long_press_finished` / `input_selected`。
//! 5. **`_ready` 里才连信号** ⇒ 树外 `emit_signal("pressed")` 不会被我们收到
//!    （连接还没建立）。验收脚本因此都先 `root.add_child` 再发信号。



use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use godot::classes::{Button, IButton};
use godot::obj::WithUserSignals;
use godot::prelude::*;

use crate::graphwar_button::{self, ButtonState, Stamp};

/// 一次派发之后，`dispatch_log` 最多留多少条（防止长按连发把内存吃光）。
const DISPATCH_LOG_LIMIT: usize = 32;

/// `GraphwarButton`：虚拟键盘上的一个键。
///
/// 状态机的**唯一**持有者是 [`graphwar_button::ButtonState`]；本结构只往里塞
/// `Stamp`、把 `Effect` 落到引擎调用上、以及在派发时驱动两张回调表。
#[derive(GodotClass)]
#[class(base = Button)]
pub struct GraphwarButton {
    base: Base<Button>,

    /// 原版 `[self+0x88]`（`pressed` 里 `add r?,0x88` 那个字段）。
    ///
    /// ⚠️ `#[export]` 是**直接读写字段**，没有 setter 钩子 ⇒ 每次派发之前
    /// [`GraphwarButton::sync_from_properties`] 把它抄进状态机。
    #[export]
    input_text: GString,

    /// 原版 `[self+0x90]`（`process` 里 `lea rbx,[rdi+0x90]` 那个字段）。
    #[export]
    long_press_input_text: GString,

    /// 原版的状态机（6 个方法的全部判定）。
    state: ButtonState,

    /// **机制 3 的原版形态**：`Vec<Box<dyn FnMut(&str)>>`（`+0x38/+0x40/+0x48`）。
    ///
    /// 这是原版逐字段复原出来的类型，本文件**原样**用它。见
    /// [`GraphwarButton::subscribe_native_probe`]。
    subscribers: Vec<Box<dyn FnMut(&str)>>,

    /// 探针闭包往这里写：`Rc<RefCell<_>>` 是"闭包不能借 `self`"的绕法（§坑 3）。
    native_capture: Rc<RefCell<Vec<String>>>,

    /// **机制 3 的替代方案**：GDScript 能给过来的只有 `Callable`
    /// （`Variant` 的类型表里没有"Rust 闭包"这一项）。
    callables: Vec<Callable>,

    /// GDScript 侧 Callable 的到货记录。
    callable_capture: Rc<RefCell<Vec<String>>>,

    /// `ready` 里按名找到 `"AnimationRect"` 了吗（原版 `[self+0x18]` 的判别位）。
    animation_linked: bool,

    /// 时间戳的锚点：原版用绝对 `Instant::now()`，我们只要"与哨兵可比"。
    epoch: Instant,

    /// 派发过的文本（按顺序，最多 [`DISPATCH_LOG_LIMIT`] 条）。
    dispatch_log: Vec<String>,

    ready_calls: i64,
    process_calls: i64,
    pressed_calls: i64,
    button_down_calls: i64,
    button_up_calls: i64,
    long_press_emits: i64,
    input_selected_emits: i64,
}

#[godot_api]
impl IButton for GraphwarButton {
    fn init(base: Base<Button>) -> Self {
        Self {
            base,
            input_text: GString::new(),
            long_press_input_text: GString::new(),
            state: ButtonState::new(),
            subscribers: Vec::new(),
            native_capture: Rc::new(RefCell::new(Vec::new())),
            callables: Vec::new(),
            callable_capture: Rc::new(RefCell::new(Vec::new())),
            animation_linked: false,
            epoch: Instant::now(),
            dispatch_log: Vec::new(),
            ready_calls: 0,
            process_calls: 0,
            pressed_calls: 0,
            button_down_calls: 0,
            button_up_calls: 0,
            long_press_emits: 0,
            input_selected_emits: 0,
        }
    }

    /// `_ready`：**机制 2** —— 连 3 条 `BaseButton` 原生信号（顺序即
    /// [`graphwar_button::SIGNAL_LINKS`] 的指令顺序），再按名找 `"AnimationRect"`。
    ///
    /// 原版两件事的证据都在 `0x4e3f20`：三个 `lea "pressed"/"button_up"/"button_down"`
    /// （`0x4e3f4e`/`0x4e3fc7`/`0x4e4040`）各配一次 `connect`，然后
    /// `lea "AnimationRect"`（`0x4e410d`）走 `get_node_by_name_rec` + cast。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // `Callable::from_object_method(self, name)` 的 gdext 写法：先拿 `Gd<Self>`。
        let myself = self.to_gd();
        for link in graphwar_button::SIGNAL_LINKS {
            let callable = myself.callable(link.name);
            // 同一个 `&str` 既是信号名、也是处理函数名 —— 这是原版的形态（§5）。
            let err = self.base_mut().connect(link.name, &callable);
            if err != godot::global::Error::OK {
                godot_warn!("GraphwarButton::ready 连接 {} 失败: {err:?}", link.name);
            }
        }

        // 按名找动画节点。**递归**查找 = 原版 `get_node_by_name_rec` 的语义；
        // 只记 bool，不持句柄（§坑 1）。
        // ⚠️ `find_child` 的 `owned` 默认是 **true**（Godot 4.6 的默认参数），而代码里
        // `add_child` 建出来的节点 `owner` 是 null ⇒ 必须显式 `owned(false)`，否则永远找不到。
        let found = self
            .base()
            .find_child_ex(graphwar_button::NODE_ANIMATION_RECT)
            .recursive(true)
            .owned(false)
            .done();
        self.animation_linked = found.is_some();
        // 状态机里那个 `[self+0x18]` 判别位（`cmp dword [r+0x18],1`）跟着走：
        // 没接动画节点 ⇒ `button_down`/`button_up` 不产生动画效果（原版同一分支）。
        self.state.animation_linked = self.animation_linked;
    }

    /// `_process`：原版 `0x4e42a0` 的**长按一次性派发**。
    ///
    /// 判定顺序逐条照抄 [`GraphwarButton::process`] 的调用方：
    /// 文本空 → 已释放 → 置哨兵 → 无订阅者（前三步都在 `ButtonState::process` 里）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        // `#[export]` 字段是**直接读写**的、没有 setter 钩子（实测），而状态机读的是自己那份
        // 拷贝 ⇒ 每帧读之前必须同步一次，否则 GDScript 改了 `long_press_input_text` 也不生效。
        self.sync_from_properties();

        // 先把借用结束掉再动 `self`（`ButtonState::process` 返回 `&str`，借的是 `self.state`）。
        let dispatched: Option<String> = self
            .state
            .process()
            .map(|text| text.to_owned());
        let Some(text) = dispatched else {
            return;
        };

        self.dispatch(&text);
        self.emit_long_press_finished();
    }
}

#[godot_api]
impl GraphwarButton {
    /// **机制 1**：自造的、无参信号。
    ///
    /// 原版**没有**这条信号（6 个方法里一个 `emit_signal` 都没有）—— 它存在的唯一理由是
    /// 让宿主（GDScript / 别的节点）能观察到"长按派发完了"，而不必去订阅那张闭包表。
    #[signal]
    fn long_press_finished();

    /// **机制 1**：自造的、带参数（`String`）的信号。
    #[signal]
    fn input_selected(text: GString);

    /// 原版 `GraphwarButton::pressed`（`0x4ef560`）：把 `input_text` 交给每个订阅者。
    ///
    /// 原版**不改状态**（只读 `+0x88`），这里也一样。它是 `#[func]` 而不是
    /// `IButton::pressed` 虚方法 —— 原版那条 `connect("pressed", …)` 指的就是一个能按名字
    /// 找到的方法（`docs/16` §19 的 9 个名字分派入口里有 `GraphwarButton pressed`）。
    ///
    /// ⚠️ 注意它与 `BaseButton` 的**虚方法** `pressed`（Godot 侧 `_pressed`）是两回事：
    /// 本类**没有**实现那个虚方法，只连信号。
    #[func]
    fn pressed(&mut self) {
        self.pressed_calls += 1;
        self.sync_from_properties();
        let text = self.state.pressed().to_owned();
        self.dispatch(&text);
        self.emit_input_selected(&text);
    }

    /// 原版 `GraphwarButton::button_down`（`0x4ef390`）：**先**记时间，**再**看有没有动画节点。
    ///
    /// 原版真的调了两次 `Instant::now()`（`0x4ef39d` 写自己、`0x4ef3e2` 写动画节点）
    /// ⇒ 这里也取两次 [`Stamp`]。
    #[func]
    fn button_down(&mut self) {
        self.button_down_calls += 1;
        let now = self.stamp();
        let again = self.stamp();
        let effects = self.state.button_down(now, again);
        self.apply_effects(&effects);
    }

    /// 原版 `GraphwarButton::button_up`（`0x4ef6a0`）：把两处 nanos 写成哨兵，再请求重绘。
    #[func]
    fn button_up(&mut self) {
        self.button_up_calls += 1;
        let effects = self.state.button_up();
        self.apply_effects(&effects);
    }

    /// **机制 3 的替代方案**：把 GDScript 的 `Callable` 注册进来，返回新的订阅者个数。
    ///
    /// 原版的表装的是 `Box<dyn FnMut(&str)>`；`Variant` 的类型表里**没有**这一项
    /// （见 `docs/16-client.md` §35.4），所以 GDScript 只能给 `Callable`。
    #[func]
    fn subscribe(&mut self, cb: Callable) -> i64 {
        if cb.is_valid() {
            self.callables.push(cb);
        }
        self.state.subscribers = self.subscribers.len() + self.callables.len();
        self.state.subscribers as i64
    }

    /// **机制 3 的原版形态**：在 Rust 侧往 `Vec<Box<dyn FnMut(&str)>>` 里 push 一个探针闭包。
    ///
    /// 闭包捕获的是 [`GraphwarButton::native_capture`]（`Rc<RefCell<Vec<String>>>`），
    /// **不是** `self` —— 见文件头 §坑 3。
    #[func]
    fn subscribe_native_probe(&mut self) -> i64 {
        let capture = self.native_capture.clone();
        self.subscribers.push(Box::new(move |text: &str| {
            capture.borrow_mut().push(text.to_owned());
        }));
        self.state.subscribers = self.subscribers.len() + self.callables.len();
        self.state.subscribers as i64
    }

    /// 清空两张表（`Vec<Box<dyn FnMut>>` 与 `Vec<Callable>`）。
    #[func]
    fn unsubscribe_all(&mut self) {
        self.subscribers.clear();
        self.callables.clear();
        self.state.subscribers = 0;
    }

    /// `pressed` 被调用过几次。
    #[func]
    fn pressed_call_count(&self) -> i64 {
        self.pressed_calls
    }

    /// `button_down` 被调用过几次。
    #[func]
    fn button_down_call_count(&self) -> i64 {
        self.button_down_calls
    }

    /// `button_up` 被调用过几次。
    #[func]
    fn button_up_call_count(&self) -> i64 {
        self.button_up_calls
    }

    /// `_ready` 被调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `long_press_finished` 发了几次（**我们发**的次数，不是别人收到的次数）。
    #[func]
    fn long_press_emit_count(&self) -> i64 {
        self.long_press_emits
    }

    /// `input_selected` 发了几次。
    #[func]
    fn input_selected_emit_count(&self) -> i64 {
        self.input_selected_emits
    }

    /// `Vec<Box<dyn FnMut(&str)>>` 里有几个（原版 `[self+0x48]` 那个长度）。
    #[func]
    fn subscriber_count(&self) -> i64 {
        self.subscribers.len() as i64
    }

    /// `Vec<Callable>` 里有几个。
    #[func]
    fn callable_count(&self) -> i64 {
        self.callables.len() as i64
    }

    /// 探针闭包收到过的文本（`|` 分隔）—— **非空就证明 `Box<dyn FnMut(&str)>` 真的被调到了**。
    #[func]
    fn native_captured(&self) -> GString {
        GString::from(join_text(&self.native_capture.borrow()).as_str())
    }

    /// GDScript 的 `Callable` 收到过的文本（`|` 分隔）。
    #[func]
    fn callable_captured(&self) -> GString {
        GString::from(join_text(&self.callable_capture.borrow()).as_str())
    }

    /// 派发流水（`|` 分隔，最多 [`DISPATCH_LOG_LIMIT`] 条）。
    #[func]
    fn dispatch_log(&self) -> GString {
        GString::from(join_text(&self.dispatch_log).as_str())
    }

    /// 状态机现在认为"按着"吗（`cmp dword [r+0x68],0x3b9aca00` 的否定）。
    #[func]
    fn press_stamp_is_pressed(&self) -> bool {
        self.state.press.is_pressed()
    }

    /// `ready` 按名找到 `"AnimationRect"` 了吗。
    #[func]
    fn animation_linked(&self) -> bool {
        self.animation_linked
    }

    /// **把"闭包捕获 `Gd<Self>` 会怎样"抓成返回值**（§坑 3）。
    ///
    /// 这里**故意**在 `#[func](&mut self)` 里对同一个实例再要一次 `bind_mut()`；
    /// gdext 的 balanced safeguard 会 panic。`catch_unwind` 把它接住，于是
    /// 验收脚本能拿到一条字符串证据，而不是让整个 headless 进程卡死。
    ///
    /// 返回 `"no panic: …"` 或 `"panic: …"`。
    #[func]
    fn reentrancy_probe(&mut self) -> GString {
        let mut myself = self.to_gd();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _second = myself.bind_mut();
        }));
        match caught {
            Ok(()) => GString::from("no panic: second bind_mut() succeeded"),
            Err(payload) => {
                let text = panic_text(&payload);
                GString::from(format!("panic: {text}").as_str())
            }
        }
    }

    // -----------------------------------------------------------------------
    // 内部：状态机 ↔ 引擎
    // -----------------------------------------------------------------------

    /// 从两个导出属性把 `input_text` / `long_press_input_text` 抄进状态机。
    ///
    /// `#[export]` 字段是直接访问的（没有 setter 钩子），所以每次派发前同步一次。
    fn sync_from_properties(&mut self) {
        self.state.input_text = self.input_text.to_string();
        self.state.long_press_input_text = self.long_press_input_text.to_string();
        self.state.subscribers = self.subscribers.len() + self.callables.len();
    }

    /// `Duration` 的当前值（原版 `Instant::now()` 的两个半区）。
    fn stamp(&self) -> Stamp {
        let elapsed = self.epoch.elapsed();
        Stamp::now(elapsed.as_secs(), elapsed.subsec_nanos())
    }

    /// 把 `ButtonState` 算出来的 [`graphwar_button::Effect`] 落到引擎调用上。
    ///
    /// 状态改动**已经在** `ButtonState` 里做完了；这里只剩"请求重绘"这一件真事
    /// （`RecordPress`/`StartAnimation`/`StopAnimation` 记的是被写进字段的时刻，
    /// 而动画节点那个类还没有节点实现，见文件头对照表最后两行）。
    fn apply_effects(&mut self, effects: &[graphwar_button::Effect]) {
        for effect in effects {
            if matches!(effect, graphwar_button::Effect::QueueRedraw) {
                self.base_mut().queue_redraw();
            }
        }
    }

    /// 把 `text` 交给**两张**回调表（原版只有第一张；第二张是给 GDScript 的替代方案）。
    fn dispatch(&mut self, text: &str) {
        if self.dispatch_log.len() >= DISPATCH_LOG_LIMIT {
            self.dispatch_log.remove(0);
        }
        self.dispatch_log.push(text.to_owned());

        // 机制 3 原版形态：`for f in self.subscribers { f(text) }`
        for callback in self.subscribers.iter_mut() {
            callback(text);
        }

        // 机制 3 替代方案：`Callable::call(&[Variant])`
        let arg = text.to_variant();
        for callable in self.callables.iter() {
            callable.call(&[arg.clone()]);
        }
    }

    /// **机制 1**：用 gdext 的类型安全生成器发无参信号。
    fn emit_long_press_finished(&mut self) {
        self.long_press_emits += 1;
        self.signals().long_press_finished().emit();
    }

    /// **机制 1**：发带参数的信号（`GString` 参数在 GDScript 侧就是 `String`）。
    fn emit_input_selected(&mut self, text: &str) {
        self.input_selected_emits += 1;
        let payload = GString::from(text);
        self.signals().input_selected().emit(&payload);
    }
}

/// 把一串字符串接成 `a|b|c`（空表 → `""`）。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic（`single_threaded.rs:170`），所以纯 Rust 的那一半必须留在 `String` 上，
/// 到 `#[func]` 边界才 `GString::from`。本文件初版就在这里栽过一次（`cargo test` 红）。
fn join_text<S: AsRef<str>>(items: &[S]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('|');
        }
        out.push_str(item.as_ref());
    }
    out
}

/// `catch_unwind` 的 payload → 可读文本（`panic!` 的两种载荷都照顾到）。
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉"这个类用的是哪份常量/哪份状态机"）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/signal_classes_check.gd`（headless，真引擎）。
// 这里只保证：① 类里没有第二份状态机；② 三个原生信号的名字就是原版那三个。

#[cfg(test)]
mod tests {
    use super::*;

    /// `ready` 连的必须是原版那 3 条，且信号名 = 处理函数名（同一个 `&str`）。
    #[test]
    fn ready_connects_exactly_the_three_original_signals() {
        let names: Vec<&str> = graphwar_button::SIGNAL_LINKS.iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["pressed", "button_up", "button_down"]);
        for link in graphwar_button::SIGNAL_LINKS {
            // 原版是"信号名与处理函数名同一地址"⇒ 名字相同。
            assert_eq!(link.name.len() as u8, link.len, "{}", link.name);
        }
    }

    /// 自造信号**不能**用基类已有的信号名（`BaseButton` 有 `pressed`/`button_up`/
    /// `button_down`/`toggled`）⇒ 本条同时锁住"我们加了哪两个"。
    #[test]
    fn custom_signals_do_not_shadow_base_class_signals() {
        let base_signals = ["pressed", "button_up", "button_down", "toggled"];
        for name in ["long_press_finished", "input_selected"] {
            assert!(
                !base_signals.contains(&name),
                "{name} 与 BaseButton 的信号同名，Godot 侧会打架"
            );
        }
    }

    /// 初始状态 = `init`（`0x4e3e00`）写出来的那 7 个值。
    #[test]
    fn initial_state_matches_the_original_init() {
        let state = ButtonState::new();
        assert_eq!(state.press, Stamp::NONE);
        assert_eq!(state.press.nanos, graphwar_button::NANOS_PER_SEC);
        assert_eq!(state.hold.nanos, graphwar_button::LONG_PRESS_HOLD_NANOS);
        assert!(!state.animation_linked);
        assert_eq!(state.subscribers, 0);
        assert!(!state.press.is_pressed());
    }

    /// **没有第二份状态机**：一次"按下 → 派发 → 抬起"的判定全来自 `ButtonState`。
    ///
    /// 这也顺便钉住了"长按只发一次"（`process` 第二次返回 `None`）。
    #[test]
    fn long_press_dispatch_comes_from_the_pure_state_machine() {
        let mut state = ButtonState::new();
        state.long_press_input_text = "S".to_owned();
        // 原版 `process` 的第 4 步是"无订阅者就返回"（`+0x48` 当长度）⇒ 先得有 1 个。
        state.subscribers = 1;

        // 还没按下 ⇒ 不发。
        assert_eq!(state.process(), None);

        let now = Stamp::now(7, 123);
        state.button_down(now, now);
        assert!(state.press.is_pressed());
        assert_eq!(state.process(), Some("S"));
        // 一次性：第二次已经是哨兵。
        assert_eq!(state.process(), None);
        assert!(!state.press.is_pressed());
    }

    /// `Stamp` 的哨兵语义：`elapsed()` 的 nanos 永远 < 1e9，所以「按着」判定永假不了。
    #[test]
    fn a_fresh_elapsed_stamp_is_always_pressed() {
        for nanos in [0u32, 1, 999_999_999] {
            assert!(Stamp::now(0, nanos).is_pressed());
        }
        assert!(!Stamp::NONE.released().is_pressed());
    }

    /// `join_text` 的分隔符与空表行为（验收脚本按 `|` 切串）。
    #[test]
    fn join_text_uses_pipe_and_handles_empty() {
        assert_eq!(join_text::<&str>(&[]), "");
        assert_eq!(join_text(&["a"]), "a");
        assert_eq!(join_text(&["a", "b"]), "a|b");
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
