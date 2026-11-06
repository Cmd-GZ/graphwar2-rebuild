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
