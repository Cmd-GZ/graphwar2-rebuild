//! `player_showcase.rs` —— **玩家名片预览**（原版 `PlayerShowcase`）。
//!
//! # 这个模块是什么
//!
//! 一个 `Control` 派生类，做三件事，每件都能指到指令：
//!
//! | 方法 | 干什么 | 档位 |
//! |---|---|---|
//! | `ready` | 把玩家档案里的**名字**灌进 `PlayerNameLineEdit`，再把该控件的 `text_changed` 连到自己的 `on_text_change` | **观测** |
//! | `on_text_change` | 名字改回来：`name = 新文本` → `save()` → **变更计数器 +1** | **观测** |
//! | `process` | 可见性闸门 + **变更计数器轮询**：变了才 `queue_redraw()` | **观测** |
//! | `draw` | 依次画 **皮肤 → 脸 → 帽子** 三层贴图，铺满自身矩形 | **观测** |
//!
//! 所以它是**角色外观预览**：显示本地玩家的名字与三层外观。
//! 它画的是三张 `Texture2D`——不是多边形、不是 `draw_circle`、不是 `draw_rect`；
//! 三次调用全是 `CanvasItem::draw_texture_rect_full`，`modulate` 恒为 `Color(1,1,1,1)`。
//!
//! # 它跟 `graphwar_color_manager` 没关系（有边界的否定）
//!
//! 任务是"查它跟 `GraphwarColor` / 两张 6 色队色表的关系"。答案：**没有关系**。
//! 本模块 10 个函数（= 引用了 `src/player_showcase.rs` 这个 Location 文件的**全部**函数，
//! 见 [`LOCATIONS`] 的 21 条指令级引用点）的具名被调者**穷举**在 [`CALLEES`] 里，
//! 其中没有任何 `GraphwarColor` / 颜色表 / 队色 / 段位函数；三次绘制的 `modulate`
//! 是 `.rodata 0x19ed00` 的**纯白**常量。
//! 玩家颜色与段位在这条链上的落点是别的模块（`graphwar_player_screen_control` 等），
//! **不在** `player_showcase` 里。
//!
//! # 地址约定
//!
//! * 本文件所有地址是 **ELF 虚地址（vaddr，`objdump` 口径）**；
//! * Ghidra / REA 地址 = 本值 + `0x100000`（**只加一次**，见 [`ghidra`]）；
//! * **arm64** 地址只用于**定名**（2.0.60 的 Android 构建没 strip），绝不当地址用；
//! * `.rodata` 的 vaddr == 文件偏移（本文件用到的两个构建都是），所以字面量可直接按偏移读。
//!
//! # 名字与地址：`name_to_addr.tsv` 的 7 行
//!
//! ```text
//! awk -F'\t' '$1=="player_showcase"' fixtures/name_to_addr.tsv
//! ```
//!
//! | 方法 | x86 | TSV 判定 | 本节处理 |
//! |---|---|---|---|
//! | `draw` | `0x4e5400` | 唯一锚（7 个双向唯一 Location） | **采用**（[`FUNCTIONS`]） |
//! | `ready` | `0x4e6480` | 唯一锚（4 个） | **采用** |
//! | `process` | `0x4e68f0` | 唯一锚（2 个） | **采用** |
//! | `on_text_change` | `0x4efd70` | 唯一锚（2 个） | **采用** |
//! | `__register_constants` | 4 候选 | 歧义 | 定到 `0x4e12e0`，见 [`NAME_BRIDGE_CONFLICTS`]（**推断**：两个 `__register_*` 大小相同，只能按 arm64 符号表顺序对应） |
//! | `__default_virtual_call` | — | 未知 | **不落地**（无 Location ⇒ 无锚） |
//! | `class_id` | — | 未知 | **不落地**，同上 |
//!
//! 那 4 条"唯一锚"是硬的：`src/player_showcase.rs` 一共有 **18 个** `panic::Location`，
//! 两个架构**逐条相同**（[`LOCATIONS`]；`only-arm` / `only-x86` 都为空），
//! 共被 **21 条指令**引用、归属到 **10 个函数**（[`FUNCTIONS`]）——
//! 正好等于 `fixtures/module_map.tsv` 给本模块的 `functions=10 / refs=21`。
//! 两架构的函数集合因此是**双射**，不是"函数大小相近"猜的。
//!
//! ⚠️ 反过来说：**不要**用"大小比"当判据。本模块 `draw` 的 x86/arm64 比值是 1.59，
//! 远高于别处记的 1.03–1.07——原因是 **x86 把三个取贴图函数内联进了 draw**
//! （能直接看到三段 inlined SwissTable 查表），arm64 则保留成 `bl`。
//! 这正好说明"大小相近"既不是必要条件也不是充分条件。
//!
//! # 本章最强的一条证据：`(line, col)` 就是源码结构
//!
//! `Location` 是编译期常量（`{file, line: u32, col: u32}`，24 字节），与指令集无关。
//! 每个 `(line, col)` 通向一个**具体的 panic 出口**，而 panic 的种类能直接从被调符号
//! 读出来（`core::option::unwrap_failed` / `core::cell::panic_already_borrowed` /
//! `core::cell::panic_already_mutably_borrowed` / `core::result::unwrap_failed`）。
//! 于是 `(line, col)` → "这一行在干什么" 是**可证**的：
//!
//! ```text
//!  9:10    #[derive(GodotClass)]（col 10 = `GodotClass` 的 G）
//! 18:129   ready：取 "GraphwarRootNode" 的 Option unwrap
//! 21:132   ready：取 "PlayerNameLineEdit" 的 Option unwrap
//! 22:47    ready：**共享**借用（panic_already_mutably_borrowed）
//! 27:1     #[godot_api] 块 A（__register_methods + __register_constants + 2×__inner_init）
//! 31:129   on_text_change：取 root 的 Option unwrap
//! 33:23    on_text_change：**独占**借用（panic_already_borrowed，计数被置 -1）
//! 37:1     #[godot_api] 块 B（1×__inner_init）
//! 52:132   ready：**第二次**取 "PlayerNameLineEdit" 的 Option unwrap（连线用）
//! 59:133   process：取 root 的 Option unwrap（比另外三处多 4 列 ⇒ 同形语句的另一种前缀）
//! 62:41    process：共享借用
//! 72:129   draw：取 root 的 Option unwrap
//! 76:38 / 77:38 / 78:37   draw：三个共享借用（皮肤 / 脸 / 帽子）
//! 80:39 / 81:39 / 82:38   draw：三个共享借用（三个取贴图调用）
//! ```
//!
//! **76/77/78 与 80/81/82 里"第三行比同组少 1 列"是可证的标识符长度差**：
//! `hat` 比 `skin`/`face` 少一个字符，`get_hat_texture` 比
//! `get_skin_texture`/`get_face_texture` 也少一个字符。
//! 这条判别式被显式编码成 [`borrow_column_signature`] / [`group_column_shortfall`]，
//! `tests/player_showcase.rs` 逐条对拍 —— 于是"78 行是帽子、82 行是取帽子贴图"
//! **不是猜的**。
//!
//! # 10 个函数怎么分
//!
//! 4 个**有真名的方法体**（arm64 `.symtab` 直接给名字与边界，见 [`ARM64_SYMS`]）
//! + 6 个 **gdext 惰性注册驱动**（`#[derive]` / `#[godot_api]` 生成物）。
//! 后者**不实现**：它们不在游戏逻辑里，逐条归属已足够（[`Role`]）。
//!
//! # 落码清单
//!
//! | 原版 | 落在 | 档位 |
//! |---|---|---|
//! | 10 个函数的名 + 地址 + 跨度 | [`OrigFn`] / [`FUNCTIONS`] / [`function_at`] / [`total_anchor_sites`] | 观测 |
//! | arm64 真名与真边界 | [`ARM64_SYMS`] / [`arm_sym`] / [`arm_size_of`] / [`arm_of`] | 观测 |
//! | 18 个 `(line,col)` → panic 种类 | [`LocAnchor`] / [`LOCATIONS`] / [`locations_of`] / [`borrow_sites_of`] | 观测 |
//! | 节点名 / 方法名 / 信号名 | [`NODE_ROOT`] … [`SIGNAL_TEXT_CHANGED_LEN`] + [`literal_pair_ok`] | 观测（`(ptr,len)` 自校验） |
//! | 根节点的两个持有单元 | [`ROOT_PLAYER_CONFIG`] / [`ROOT_TEXTURE_LOADER`] / [`root_unit_writer`] | 观测（写者唯一） |
//! | 玩家档案布局 | [`CFG_NAME`] / [`CFG_SKIN`] / [`CFG_FACE`] / [`CFG_HAT`] / [`CFG_CHANGE_COUNTER`] | 观测（**字段名是推断**） |
//! | `String` 三字布局 | [`StringLayout`] | 观测（`__rust_dealloc` 实参序 + `set_text` 实参序） |
//! | 三次绘制 | [`DrawCall`] / [`draw_plan`] / [`DRAW_ORDER`] / [`DRAW_COLOR_RODATA`] | 观测 |
//! | 变更计数器两条链 | [`Showcase`] / [`ProcessOutcome`] / [`on_text_change_effect`] | 观测 |
//! | 与颜色表无关 | [`CALLEES`] / [`touches_color_manager`] | 观测（有界否定） |
//! | 未解出的 | [`UNKNOWNS`] | 未知 |
//!
//! Godot 运行时调用一律**只写意图结构**：本模块不 `use` 任何 `godot_*` crate，
//! 也不在 Rust 里调引擎 API（`client/` 下那份 `player_showcase.gd` 是**改写**，不是原版源码）。

// ---------------------------------------------------------------------------
// 0. 文件级常量
// ---------------------------------------------------------------------------

/// 原版源文件名（`panic::Location` 的 `file` 段）。档位：**观测**。
pub const SOURCE_FILE: &str = "src/player_showcase.rs";

/// 类名（arm64 `.symtab` 的模块路径 `<graphwarrust::player_showcase::PlayerShowcase …>`）。
///
/// 档位：**观测**。⚠️ 类名**字面量**在 `.rodata` 的地址**未解**（见 [`UNKNOWNS`]）。
pub const CLASS_NAME: &str = "PlayerShowcase";

/// x86 构建（Steam 2026-08-28，sha256 `9a538115…`）。档位：**观测**。
pub const X86_TARGET: &str = ".reference/steam/2026-08-28/libgraphwarrust.so";

/// arm64 构建（Android 2.0.60，**未 strip**，只用于定名）。档位：**观测**。
pub const ARM64_TARGET: &str = ".rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so";

/// Ghidra / REA 地址与 ELF 虚地址的固定差。档位：**观测**（`AGENTS.md` Hard facts）。
pub const GHIDRA_BIAS: u64 = 0x10_0000;

/// ELF 虚地址 → Ghidra / REA 地址。
pub const fn ghidra(vaddr: u64) -> u64 {
    vaddr + GHIDRA_BIAS
}

// ---------------------------------------------------------------------------
// 1. 函数表：10 个函数（= 引用本文件 Location 的全部函数）
// ---------------------------------------------------------------------------

/// 一个函数在**本模块**里的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `IControl::draw` —— 画三层贴图。
    Draw,
    /// `IControl::ready` —— 灌名字 + 连信号。
    Ready,
    /// `IControl::process` —— 可见性闸门 + 计数器轮询。
    Process,
    /// `PlayerShowcase::on_text_change` —— `#[func]`，写回名字。
    OnTextChange,
    /// `#[derive(GodotClass)]` 生成的类注册闭包（Location `9:10`）。
    DeriveInit,
    /// `#[godot_api]` 生成的 `__register_methods` / `__register_constants`（Location `27:1`）。
    RegisterFns,
    /// `#[godot_api]` 生成的 `__inner_init`（每个注册项一个）。
    InnerInit,
}

impl Role {
    /// 中文短名（文档/测试用）。
    pub const fn label(self) -> &'static str {
        match self {
            Role::Draw => "draw",
            Role::Ready => "ready",
            Role::Process => "process",
            Role::OnTextChange => "on_text_change",
            Role::DeriveInit => "derive-init",
            Role::RegisterFns => "register-fns",
            Role::InnerInit => "inner-init",
        }
    }

    /// 是不是游戏逻辑（只有这 4 个有方法体可读；其余是 gdext 样板）。
    pub const fn is_game_logic(self) -> bool {
        matches!(
            self,
            Role::Draw | Role::Ready | Role::Process | Role::OnTextChange
        )
    }
}

/// 一条函数记录。
///
/// * `x86` / `span` —— Linux 2.0.60 的 vaddr 与"下一个 Ghidra 起点 − 本起点"
///   （起点表是 `analysis/funcstarts_ghidra_va.pkl`，**不是**那份错的 `.eh_frame`）；
/// * `arm64` / `arm_size` —— Android 2.0.60 `.symtab` 给的真边界，**只用于定名与比对**；
/// * `body` —— 真实机器码字节数（能指到收尾 `ret` 时才有值；`None` = 未逐指令定界）。
#[derive(Debug, Clone, Copy)]
pub struct OrigFn {
    /// 原版真名（arm64 `.symtab` 的去 hash 短名；样板函数带 `_` 前缀）。
    pub name: &'static str,
    /// ELF 虚地址。
    pub x86: u64,
    /// Ghidra 起点表跨度（`下一个起点 − 本起点`）。
    pub span: u32,
    /// 真实机器码字节数（`None` = 未逐指令定界）。
    pub body: Option<u32>,
    /// arm64 地址（只用于定名）。
    pub arm64: u64,
    /// arm64 `.symtab` 的 `st_size`。
    pub arm_size: u32,
    /// 本模块里的角色。
    pub role: Role,
    /// 它引用的 `(line, col)`（穷举；来自 `.data.rel.ro` 的 Location 结构体）。
    pub anchors: &'static [(u32, u32)],
}

impl OrigFn {
    /// 真实字节数：优先 [`OrigFn::body`]，否则退回 Ghidra 跨度。
    pub const fn effective_len(&self) -> u32 {
        match self.body {
            Some(b) => b,
            None => self.span,
        }
    }

    /// 引用了几条 Location 指令级引用点。
    pub const fn anchor_count(&self) -> usize {
        self.anchors.len()
    }

    /// x86 与 arm64 的字节数比。
    ///
    /// **不是判据**，只作记录：本模块 `draw` 的比值偏大，原因是 x86 把三个
    /// 取贴图函数**内联**了（能直接看到三段 inlined SwissTable 查表）。
    pub fn size_ratio(&self) -> f64 {
        f64::from(self.span) / f64::from(self.arm_size)
    }
}

/// **10 个函数**，按 x86 地址升序（= `fixtures/module_map.tsv` 的 `functions=10`）。
///
/// 档位：**观测**（地址由 21 条 Location 引用点归属；arm64 名与边界来自 `.symtab`）。
pub const FUNCTIONS: [OrigFn; 10] = [
    OrigFn {
        name: "_::register_methods",
        x86: 0x4e1160,
        span: 384,
        body: None,
        arm64: 0x43c1bc,
        arm_size: 400,
        role: Role::RegisterFns,
        anchors: &[(27, 1)],
    },
    OrigFn {
        name: "_::register_constants",
        x86: 0x4e12e0,
        span: 384,
        body: None,
        arm64: 0x43c34c,
        arm_size: 400,
        role: Role::RegisterFns,
        anchors: &[(27, 1)],
    },
    OrigFn {
        name: "IControl::draw",
        x86: 0x4e5400,
        span: 4224,
        // 收尾 `ret` 在 `0x4e60be`；`0x4e60bf..0x4e6476` 是**外置冷路径**
        // （`lea "Upcast failed…"(47)` + `core::result::unwrap_failed` 等），
        // Ghidra 因为没有新起点把它并进了 draw 的跨度。
        body: Some(0xcc0),
        arm64: 0x43fe7c,
        arm_size: 2652,
        role: Role::Draw,
        anchors: &[
            (72, 129),
            (76, 38),
            (77, 38),
            (78, 37),
            (80, 39),
            (81, 39),
            (82, 38),
        ],
    },
    OrigFn {
        name: "IControl::ready",
        x86: 0x4e6480,
        span: 1136,
        body: None,
        arm64: 0x4408d8,
        arm_size: 1108,
        role: Role::Ready,
        anchors: &[(18, 129), (21, 132), (22, 47), (52, 132)],
    },
    OrigFn {
        name: "IControl::process",
        x86: 0x4e68f0,
        span: 656,
        body: None,
        arm64: 0x440d2c,
        arm_size: 656,
        role: Role::Process,
        anchors: &[(59, 133), (62, 41)],
    },
    OrigFn {
        name: "_::inner_init",
        x86: 0x4e87d0,
        span: 416,
        body: None,
        arm64: 0x442adc,
        arm_size: 428,
        role: Role::InnerInit,
        anchors: &[(27, 1)],
    },
    OrigFn {
        name: "_::inner_init",
        x86: 0x4e8970,
        span: 432,
        body: None,
        arm64: 0x442e80,
        arm_size: 448,
        role: Role::InnerInit,
        anchors: &[(27, 1)],
    },
    OrigFn {
        name: "_::inner_init",
        x86: 0x4e8b20,
        span: 592,
        body: None,
        arm64: 0x443040,
        arm_size: 512,
        role: Role::InnerInit,
        anchors: &[(37, 1)],
    },
    OrigFn {
        name: "_::init",
        x86: 0x4e8d70,
        span: 528,
        body: None,
        arm64: 0x442c88,
        arm_size: 504,
        role: Role::DeriveInit,
        anchors: &[(9, 10)],
    },
    OrigFn {
        name: "on_text_change",
        x86: 0x4efd70,
        span: 672,
        body: None,
        arm64: 0x448d10,
        arm_size: 704,
        role: Role::OnTextChange,
        anchors: &[(31, 129), (33, 23)],
    },
];

/// 按 vaddr 查记录。档位：**观测**。
pub fn function_at(vaddr: u64) -> Option<&'static OrigFn> {
    FUNCTIONS.iter().find(|f| f.x86 == vaddr)
}

/// 按角色查记录（`RegisterFns`/`InnerInit` 有多条，返回第一条）。
pub fn by_role(role: Role) -> Option<&'static OrigFn> {
    FUNCTIONS.iter().find(|f| f.role == role)
}

/// 游戏逻辑的 4 个函数（顺序 = [`FUNCTIONS`] 里出现顺序）。
pub fn method_bodies() -> impl Iterator<Item = &'static OrigFn> {
    FUNCTIONS.iter().filter(|f| f.role.is_game_logic())
}

/// 21 条 Location 指令级引用点 = 10 个函数 `anchors` 的总长。
///
/// 对拍口径：`fixtures/module_map.tsv` 给本模块 `refs=21`。
pub const fn total_anchor_sites() -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < FUNCTIONS.len() {
        n += FUNCTIONS[i].anchors.len();
        i += 1;
    }
    n
}

// ---------------------------------------------------------------------------
// 2. arm64 真名（2.0.60 的 Android 构建没 strip）
// ---------------------------------------------------------------------------

/// arm64 `.symtab` 里属于本模块的条目（`(地址, 去 hash 名)`）。
///
/// 档位：**观测**（`readelf -sW … | c++filt | grep player_showcase`）。
pub const ARM64_SYMS: [(u64, &str); 12] = [
    (
        0x43c1bc,
        "<PlayerShowcase as ImplementsGodotApi>::__register_methods",
    ),
    (
        0x43c34c,
        "<PlayerShowcase as ImplementsGodotApi>::__register_constants",
    ),
    (
        0x43f988,
        "<PlayerShowcase as ImplementsGodotVirtual>::__virtual_call::virtual_fn",
    ),
    (
        0x43fa24,
        "<PlayerShowcase as ImplementsGodotVirtual>::__virtual_call::virtual_fn",
    ),
    (
        0x43fac0,
        "<PlayerShowcase as ImplementsGodotVirtual>::__virtual_call::virtual_fn",
    ),
    (0x43fe7c, "<PlayerShowcase as IControl>::draw"),
    (0x4408d8, "<PlayerShowcase as IControl>::ready"),
    (0x440d2c, "<PlayerShowcase as IControl>::process"),
    (0x442adc, "player_showcase::_::__init::__inner_init"),
    (0x442e80, "player_showcase::_::__init::__inner_init"),
    (0x443040, "player_showcase::_::__init::__inner_init"),
    (0x448d10, "PlayerShowcase::on_text_change"),
];

/// `class_id` 的 arm64 符号地址（**不在** [`ARM64_SYMS`] 里：56 字节、无 Location）。
/// 档位：**观测**（`.symtab`）；x86 地址**未知**（见 [`UNKNOWNS`]）。
pub const ARM64_CLASS_ID: u64 = 0x438088;

/// `class_id` 的缓存槽（`.data.rel.ro`，8 字节 `CLASS_ID`）。档位：**观测**。
pub const ARM64_CLASS_ID_SLOT: u64 = 0x8b40b0;

/// `__registration_storage::STORAGE`（arm64，56 字节对象）。档位：**观测**。
pub const ARM64_REGISTRATION_STORAGE: u64 = 0x8b4178;

/// arm64 地址 → 去 hash 名。档位：**观测**。
pub fn arm_sym(addr: u64) -> Option<&'static str> {
    ARM64_SYMS
        .iter()
        .find(|(a, _)| *a == addr)
        .map(|(_, n)| *n)
}

/// arm64 地址 → `.symtab` 的 `st_size`（从 [`FUNCTIONS`] 的配对里取）。档位：**观测**。
pub fn arm_size_of(addr: u64) -> Option<u32> {
    FUNCTIONS
        .iter()
        .find(|f| f.arm64 == addr)
        .map(|f| f.arm_size)
}

/// x86 地址 → arm64 地址。档位：**观测**（`RegisterFns` 那一对的左右顺序是**推断**，
/// 见 [`NAME_BRIDGE_CONFLICTS`]）。
pub fn arm_of(x86: u64) -> Option<u64> {
    function_at(x86).map(|f| f.arm64)
}

/// arm64 三个 `__virtual_call::virtual_fn` 薄壳（每个虚函数一个）。
///
/// 档位：**观测**（`.symtab` 三个 156 字节函数，且**都不引用任何 Location**）。
/// ⇒ "虚函数 3 个" 与 [`FUNCTIONS`] 里 `ready`/`process`/`draw` 三条**互相独立印证**。
pub const ARM64_VIRTUAL_FN: [u64; 3] = [0x43f988, 0x43fa24, 0x43fac0];

// ---------------------------------------------------------------------------
// 3. 18 个 Location：`(line, col)` → panic 种类
// ---------------------------------------------------------------------------

/// panic 出口的种类 —— **直接从被调符号读出来**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanicKind {
    /// `core::option::unwrap_failed`（arm64 `0x836d74`；x86 `[0x9ba870]`）。
    OptionUnwrap,
    /// `core::result::unwrap_failed`，消息 47 字节 =
    /// `"Upcast failed. This is a bug; please report it."`（gdext `check_rtti`）。
    ResultUnwrapUpcast,
    /// `core::cell::panic_already_mutably_borrowed`（**共享**借用撞上独占借用）。
    SharedBorrowConflict,
    /// `core::cell::panic_already_borrowed`（**独占**借用撞上任何借用）。
    MutBorrowConflict,
    /// gdext 样板：`#[derive]`/`#[godot_api]` 生成物自带的 `.unwrap()`。
    BoilerplateUnwrap,
}

impl PanicKind {
    /// 中文短名。
    pub const fn label(self) -> &'static str {
        match self {
            PanicKind::OptionUnwrap => "Option::unwrap",
            PanicKind::ResultUnwrapUpcast => "Result::unwrap (Upcast failed)",
            PanicKind::SharedBorrowConflict => "shared-borrow conflict",
            PanicKind::MutBorrowConflict => "mut-borrow conflict",
            PanicKind::BoilerplateUnwrap => "boilerplate unwrap",
        }
    }

    /// 是不是借用冲突（⇒ 这一行在借东西，而不是在取节点）。
    pub const fn is_borrow(self) -> bool {
        matches!(
            self,
            PanicKind::SharedBorrowConflict | PanicKind::MutBorrowConflict
        )
    }
}

/// 一条 Location 记录：两个架构**逐字段相同**的编译期常量 + 它的 x86 引用点。
#[derive(Debug, Clone, Copy)]
pub struct LocAnchor {
    /// `Location.line`。
    pub line: u32,
    /// `Location.col`。
    pub col: u32,
    /// 引用它的 x86 指令（装载 Location 指针那条）。
    pub x86_insn: u64,
    /// 该指令所属函数的 x86 vaddr。
    pub owner: u64,
    /// 该 Location 通向的 panic 种类。
    pub kind: PanicKind,
}

/// **21 条指令级引用点**（去重后是 **18 个** `(line, col)`；`27:1` 被 4 个注册闭包
/// 各引用一次），按 `(line, col)` 升序。
///
/// 档位：**观测**（两侧 `(file,line,col)` 集合完全相同，`only-arm`/`only-x86` 皆空）。
pub const LOCATIONS: [LocAnchor; 21] = [
    LocAnchor {
        line: 9,
        col: 10,
        x86_insn: 0x4e8f00,
        owner: 0x4e8d70,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 18,
        col: 129,
        x86_insn: 0x4e67d7,
        owner: 0x4e6480,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 21,
        col: 132,
        x86_insn: 0x4e684d,
        owner: 0x4e6480,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 22,
        col: 47,
        x86_insn: 0x4e685c,
        owner: 0x4e6480,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 27,
        col: 1,
        x86_insn: 0x4e125b,
        owner: 0x4e1160,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 27,
        col: 1,
        x86_insn: 0x4e13db,
        owner: 0x4e12e0,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 27,
        col: 1,
        x86_insn: 0x4e88ed,
        owner: 0x4e87d0,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 27,
        col: 1,
        x86_insn: 0x4e8aa1,
        owner: 0x4e8970,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 31,
        col: 129,
        x86_insn: 0x4eff14,
        owner: 0x4efd70,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 33,
        col: 23,
        x86_insn: 0x4eff61,
        owner: 0x4efd70,
        kind: PanicKind::MutBorrowConflict,
    },
    LocAnchor {
        line: 37,
        col: 1,
        x86_insn: 0x4e8ce6,
        owner: 0x4e8b20,
        kind: PanicKind::BoilerplateUnwrap,
    },
    LocAnchor {
        line: 52,
        col: 132,
        x86_insn: 0x4e686b,
        owner: 0x4e6480,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 59,
        col: 133,
        x86_insn: 0x4e6ad8,
        owner: 0x4e68f0,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 62,
        col: 41,
        x86_insn: 0x4e6b1d,
        owner: 0x4e68f0,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 72,
        col: 129,
        x86_insn: 0x4e60f4,
        owner: 0x4e5400,
        kind: PanicKind::OptionUnwrap,
    },
    LocAnchor {
        line: 76,
        col: 38,
        x86_insn: 0x4e618e,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 77,
        col: 38,
        x86_insn: 0x4e55f0,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 78,
        col: 37,
        x86_insn: 0x4e619d,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 80,
        col: 39,
        x86_insn: 0x4e61ac,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 81,
        col: 39,
        x86_insn: 0x4e61bb,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
    LocAnchor {
        line: 82,
        col: 38,
        x86_insn: 0x4e61ca,
        owner: 0x4e5400,
        kind: PanicKind::SharedBorrowConflict,
    },
];

/// 某函数引用的全部 Location。档位：**观测**。
///
/// ⚠️ 这是**指令级**列表：`27:1` 出现 4 次（4 个注册闭包各一条），
/// 所以条数和是 21，**不等于**去重后的 Location 个数 18。
pub fn locations_of(owner: u64) -> impl Iterator<Item = &'static LocAnchor> {
    LOCATIONS.iter().filter(move |a| a.owner == owner)
}

/// 去重后的 `(line, col)` 个数 = **18**。
pub fn distinct_location_count() -> usize {
    let mut seen: [Option<(u32, u32)>; 21] = [None; 21];
    let mut n = 0;
    for a in LOCATIONS.iter() {
        let key = (a.line, a.col);
        if !seen[..n].iter().any(|s| *s == Some(key)) {
            seen[n] = Some(key);
            n += 1;
        }
    }
    n
}

/// 某函数里**借用冲突**的 Location（= 该函数在借什么东西）。
pub fn borrow_sites_of(owner: u64) -> impl Iterator<Item = &'static LocAnchor> {
    locations_of(owner).filter(|a| a.kind.is_borrow())
}

/// `draw` 里 6 个借用点的列号签名 `(76, 77, 78, 80, 81, 82) → (38, 38, 37, 39, 39, 38)`。
///
/// **本节最漂亮的一条判别式**：每组第三行比同组另两行**少 1 列** ——
/// 因为 `hat` 比 `skin`/`face` 少一个字符、`get_hat_texture` 比
/// `get_skin_texture`/`get_face_texture` 少一个字符。
/// ⇒ "78 行 = 帽子、82 行 = 取帽子贴图" 是**推出来的**，不是猜的。
/// 档位：**观测**（列号来自 `.data.rel.ro` 的 `Location.col`）。
pub const fn borrow_column_signature() -> [(u32, u32); 6] {
    [(76, 38), (77, 38), (78, 37), (80, 39), (81, 39), (82, 38)]
}

/// 两组"第三行少几列"；正常应为 `(1, 1)`。
pub fn group_column_shortfall() -> (u32, u32) {
    let s = borrow_column_signature();
    (s[0].1 - s[2].1, s[3].1 - s[5].1)
}

// ---------------------------------------------------------------------------
// 4. `.rodata` 字面量：节点名 / 方法名 / 信号名
// ---------------------------------------------------------------------------

/// 根节点名。`.rodata 0x19e490`（x86）/ `0x198660`（arm64）。档位：**观测**。
///
/// 四个方法**全都**用它取同一个节点：`draw`（`0x4e5488`）、`ready`（`0x4e64f6`）、
/// `process`（`0x4e69ac`）、`on_text_change`（`0x4efdf1`）。
pub const NODE_ROOT: &str = "GraphwarRootNode";
/// 上面的长度（指令立即数 `mov w3,#0x10` / `mov edx,0x10`）。档位：**观测**。
pub const NODE_ROOT_LEN: u32 = 16;

/// 名字输入框节点名。`.rodata 0x1c42ed`（x86）/ `0x19dc5a`（arm64）。档位：**观测**。
pub const NODE_NAME_EDIT: &str = "PlayerNameLineEdit";
/// 上面的长度。档位：**观测**。
pub const NODE_NAME_EDIT_LEN: u32 = 18;

/// 连到自己的方法名。`.rodata 0x1c42ff`（紧接上一个字面量）。档位：**观测**。
pub const METHOD_ON_TEXT_CHANGE: &str = "on_text_change";
/// 上面的长度。档位：**观测**。
pub const METHOD_ON_TEXT_CHANGE_LEN: u32 = 14;

/// 信号名。`.rodata 0x1c430d`（紧接上一个）。档位：**观测**。
pub const SIGNAL_TEXT_CHANGED: &str = "text_changed";
/// 上面的长度。档位：**观测**。
pub const SIGNAL_TEXT_CHANGED_LEN: u32 = 12;

/// 三条字面量在 `.rodata` 里**首尾相接**：`0x1c42ed` → `0x1c42ff` → `0x1c430d`。
///
/// 档位：**观测**（三处 `lea` 的地址差恰好等于前一条的长度：18 / 14）。
pub const LITERAL_RUN: [(u64, &str); 3] = [
    (0x1c42ed, NODE_NAME_EDIT),
    (0x1c42ff, METHOD_ON_TEXT_CHANGE),
    (0x1c430d, SIGNAL_TEXT_CHANGED),
];

/// `str` 的字节长度（= `.rodata` 那个 `len` 立即数）。
pub const fn str_len(s: &str) -> u32 {
    s.len() as u32
}

/// 一条 `(地址, 长度)` 立即数对能不能自洽：长度对上，且地址对得上 [`LITERAL_RUN`]。
pub fn literal_pair_ok(addr: u64, len: u32, s: &str) -> bool {
    str_len(s) == len && LITERAL_RUN.iter().all(|(a, t)| *a != addr || *t == s)
}

/// 本模块四个 `(ptr,len)` 立即数对**全部自校验通过**（`node_name_ok()` 必须为 `true`）。
///
/// 这是名字类结论唯一的硬约束：Godot 侧按 `(类名, 方法名)` 分派，长度错一位就找不到。
pub fn node_name_ok() -> bool {
    literal_pair_ok(0x19e490, NODE_ROOT_LEN, NODE_ROOT)
        && literal_pair_ok(0x1c42ed, NODE_NAME_EDIT_LEN, NODE_NAME_EDIT)
        && literal_pair_ok(0x1c42ff, METHOD_ON_TEXT_CHANGE_LEN, METHOD_ON_TEXT_CHANGE)
        && literal_pair_ok(0x1c430d, SIGNAL_TEXT_CHANGED_LEN, SIGNAL_TEXT_CHANGED)
}

// ---------------------------------------------------------------------------
// 5. 它读谁：根节点的两个"持有单元"
// ---------------------------------------------------------------------------

/// 根节点 `+0x16a8` = `Rc<RefCell<GraphwarPlayerConfig>>`（玩家档案）。档位：**观测**。
///
/// 三条互相独立的判据：
/// 1. 全 `.text` 里 `str [+0x16a8]` **只有一条**（arm64 `0x39a450`），
///    在 `<GraphwarRootNode as INode>::init`（`0x399dd0`）里；写者唯一 ⇒ 这个槽由根节点拥有；
/// 2. 单元形状 = `[0]` 强引用计数（用到前 `+1`）、`[+0x10]` 借用计数
///    （共享借用 `+1`、独占借用被置 `-1`）、`[+0x18]` 载荷 —— 正是
///    `RcInner { strong, weak, RefCell { borrow, value } }`；
/// 3. 载荷上的 `save()` / `set_skin()` / `lock()` 都是
///    `graphwarrust::graphwar_player_config::GraphwarPlayerConfig` 的方法（arm64 `.symtab`）。
pub const ROOT_PLAYER_CONFIG: usize = 0x16a8;

/// 根节点 `+0x16a0` = `Rc<RefCell<GraphwarTextureLoader>>`（贴图缓存）。档位：**观测**。
///
/// 判据：`str [+0x16a0]` 全 `.text` 也只有一条（arm64 `0x39a44c`，同一个 `INode::init`）；
/// 本模块 `draw` 从它取出 `GraphwarTextureLoader` 并调三个 `get_*_texture`；
/// 另外 `EntityDrawer::draw`、`GraphwarSetupControl`、`GraphwarFunctionInput`、
/// `GraphwarLanVBox`、`GraphwarPlayerScreenControl::skin_button_pressed` 也读它
/// ⇒ 它是几个绘制类**共享**的贴图缓存（不是本模块私有）。
pub const ROOT_TEXTURE_LOADER: usize = 0x16a0;

/// 根节点这两个单元的**唯一写入者**。
///
/// 档位：**观测**（全 `.text` 扫 `mem.disp ∈ {0x16a0,0x16a8}` 的写指令，各 1 条；
/// 两条都属于 arm64 `0x399dd0` = `<GraphwarRootNode as INode>::init`）。
///
/// ⚠️ 一条**就地更正**：本模块第一版把这两条 `str` 记成 `GraphwarRootNode::ready`
/// ——那是因为扫描脚本把过长的 trait impl 符号名截断到 95 字符，`…::re_expor` 后面
/// 看不见了。按符号边界复核后是 **`INode::init`**（构造函数里建两个 `Rc<RefCell<…>>`），
/// 语义更合理：两个单元在节点构造时就存在，`ready` 不该是唯一写者。
pub const ROOT_UNIT_WRITERS: [(u64, &str, u64); 2] = [
    (
        ROOT_TEXTURE_LOADER as u64,
        "<GraphwarRootNode as INode>::init",
        0x39a44c,
    ),
    (
        ROOT_PLAYER_CONFIG as u64,
        "<GraphwarRootNode as INode>::init",
        0x39a450,
    ),
];

/// 偏移 → 单元写入者名（`None` = 不是这两个单元）。
pub fn root_unit_writer(off: usize) -> Option<&'static str> {
    ROOT_UNIT_WRITERS
        .iter()
        .find(|(o, _, _)| *o == off as u64)
        .map(|(_, name, _)| *name)
}

/// 玩家档案里**名字** `String` 的偏移。档位：**观测**
/// （`on_text_change` 覆盖 `载荷+0x00..0x18`；`ready` 把 `载荷+0x08`/`+0x10` 当 `(ptr,len)` 用）。
pub const CFG_NAME: usize = 0x00;
/// 皮肤 `String`。`set_skin` 在 arm64 `0x392c54` 用 `ldr x1,[x21,#0x60]!` 取旧值的
/// `cap`、`str q0,[x21]` 写新值 ⇒ 这个偏移上的字段就是 `set_skin` 写的那个。档位：**观测**。
pub const CFG_SKIN: usize = 0x60;
/// 脸 `String`（= `CFG_SKIN + 0x18`）。档位：**观测**（`draw` 第二个 `(ptr,len)` 对）。
pub const CFG_FACE: usize = 0x78;
/// 帽子 `String`（= `CFG_FACE + 0x18`）。档位：**观测**（`draw` 第三个 `(ptr,len)` 对）。
pub const CFG_HAT: usize = 0x90;
/// 变更计数器（`u32`）：`on_text_change` 里 `+1`，`process` 里轮询它。档位：**观测**。
///
/// 真值：arm64 `0x448e40..0x448e54` `ldr w8,[cell+0xc0]`（= 载荷 `+0xa8`）
/// → `add w8,w8,#1` → `str`；`new` 里 `str wzr,[sp,#0xa8]` 清 0；
/// `set_skin`/`set_face`/`set_hat` 三个方法也都 `+1`。
pub const CFG_CHANGE_COUNTER: usize = 0xa8;
/// 锁定位（`bool`，单字节）：`lock()` 写 `0`，`set_skin` 读它决定要不要把新值
/// 同时写进"原始"槽。**本模块不读它**。档位：**观测**。
pub const CFG_LOCK_FLAG: usize = 0xac;

/// 单元头 → 载荷的固定偏移（`RcInner` 的 `strong`/`weak` + `RefCell` 的 `borrow`）。
///
/// 于是"单元 + 偏移"与"载荷 + 偏移"差 `0x18`：`draw` 读 `cell+0x80` 就是
/// `CFG_SKIN` 的 `ptr` 位（`0x60 + 0x08 + 0x18 = 0x80`）。
pub const UNIT_PAYLOAD: usize = 0x18;
/// 单元内的借用计数偏移（共享 `+1` / 独占 `-1`）。档位：**观测**。
pub const UNIT_BORROW: usize = 0x10;
/// `bind_mut()` 把借用计数写成这个值（`-1`）。档位：**观测**（arm64 `0x448dfc` `mov x8,#-1`）。
pub const EXCLUSIVE_BORROW_MARK: i64 = -1;

/// 档案的三个默认外观名（`GraphwarPlayerConfig::new` 写进去的字面量）。
///
/// 档位：**观测**（arm64 `0x39152c..0x3915a4`：6 字节 `yellow`、
/// 12 字节 `regular_eyes`、14 字节 `soldier_helmet`；两组 ⇒ "原始"三槽 + "当前"三槽）。
pub const DEFAULT_SKIN: &str = "yellow";
/// 默认脸。见 [`DEFAULT_SKIN`]。
pub const DEFAULT_FACE: &str = "regular_eyes";
/// 默认帽。见 [`DEFAULT_SKIN`]。
pub const DEFAULT_HAT: &str = "soldier_helmet";

/// 三个默认值的长度（`new` 里的 `mov w0,#0x6 / #0xc / #0xe`）。档位：**观测**。
pub const DEFAULT_LENS: (u32, u32, u32) = (
    str_len(DEFAULT_SKIN),
    str_len(DEFAULT_FACE),
    str_len(DEFAULT_HAT),
);

/// Rust `String` 在本二进制里的三字布局：**(cap, ptr, len)**，24 字节。
///
/// 档位：**观测**，两条独立判据：
/// * **释放序**：`on_text_change` 的 `0x448e14`–`0x448e24` 先
///   `ldr x1,[x19,#0x18]!`（非零才释放）再 `ldr x0,[x20,#0x20]`，然后
///   `__rust_dealloc(x0, x1, 1)` ⇒ `x1` 是 **size/cap**、`x0` 是 **ptr**；
/// * **使用序**：`ready` 把 `[cell+0x20]`,`[cell+0x28]`（= 载荷 `+0x08`/`+0x10`）
///   当 `(&str)` 的 `(ptr, len)` 传给 `LineEdit::set_text`。
///
/// ⇒ 载荷 `+0x00` = cap、`+0x08` = ptr、`+0x10` = len。
/// ⚠️ `Vec`/`String` 的字段序在 `repr(Rust)` 下**不保证**；若照搬常见的
/// "ptr, cap, len" 会把 `len` 读成 `cap`（这个二进制的 `cap` 恰好在前）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringLayout;

impl StringLayout {
    /// `cap` 在字段内的偏移。
    pub const CAP: usize = 0x00;
    /// `ptr` 在字段内的偏移。
    pub const PTR: usize = 0x08;
    /// `len` 在字段内的偏移。
    pub const LEN: usize = 0x10;
    /// 一个 `String` 占几个字节。
    pub const SIZE: usize = 0x18;

    /// 从载荷基点 `base` 起，一个 `String` 三字的位置。
    pub const fn slots(base: usize) -> (usize, usize, usize) {
        (base + Self::CAP, base + Self::PTR, base + Self::LEN)
    }

    /// 当这个字段被当 `&str` 用时，`(ptr, len)` 的实际偏移。
    pub const fn ptr_len(base: usize) -> (usize, usize) {
        (base + Self::PTR, base + Self::LEN)
    }
}

// ---------------------------------------------------------------------------
// 6. 贴图加载器：三张表（皮肤 / 脸 / 帽子各一张）
// ---------------------------------------------------------------------------

/// 一张贴图表属于哪一类外观。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureKind {
    /// 皮肤。
    Skin,
    /// 脸。
    Face,
    /// 帽子。
    Hat,
}

impl TextureKind {
    /// 取图方法名（**原版真名**，arm64 `.symtab`）。档位：**观测**。
    pub const fn getter(self) -> &'static str {
        match self {
            TextureKind::Skin => "get_skin_texture",
            TextureKind::Face => "get_face_texture",
            TextureKind::Hat => "get_hat_texture",
        }
    }

    /// getter 的 arm64 地址。档位：**观测**。
    pub const fn arm_getter(self) -> u64 {
        match self {
            TextureKind::Skin => 0x40c1cc,
            TextureKind::Face => 0x40c0a4,
            TextureKind::Hat => 0x40bf7c,
        }
    }

    /// getter 读的那张 `HashMap` 的控制字节指针偏移（`0x168`/`0x198`/`0x1c8`）。
    /// 三张表**间距 `0x30`**。档位：**观测**（arm64 三个 getter 各读一处）。
    pub const fn map_ctrl(self) -> usize {
        match self {
            TextureKind::Skin => 0x168,
            TextureKind::Face => 0x198,
            TextureKind::Hat => 0x1c8,
        }
    }

    /// 该表的 hasher 字段偏移。档位：**观测**。
    pub const fn map_hasher(self) -> usize {
        match self {
            TextureKind::Skin => 0x170,
            TextureKind::Face => 0x1a0,
            TextureKind::Hat => 0x1d0,
        }
    }

    /// 该表桶指针/掩码的偏移。档位：**观测**。
    pub const fn map_buckets(self) -> usize {
        match self {
            TextureKind::Skin => 0x150,
            TextureKind::Face => 0x180,
            TextureKind::Hat => 0x1b0,
        }
    }

    /// 中文名。
    pub const fn label(self) -> &'static str {
        match self {
            TextureKind::Skin => "皮肤",
            TextureKind::Face => "脸",
            TextureKind::Hat => "帽子",
        }
    }
}

/// 三张表按绘制分层顺序的排列。
pub const TEXTURE_KINDS: [TextureKind; 3] = [
    TextureKind::Skin,
    TextureKind::Face,
    TextureKind::Hat,
];

/// 偏移 → 是哪张表（`None` = 不是三张表之一的 ctrl 槽）。
pub fn texture_kind_of_map(off: usize) -> Option<TextureKind> {
    TEXTURE_KINDS
        .iter()
        .copied()
        .find(|k| k.map_ctrl() == off)
}

// ---------------------------------------------------------------------------
// 7. `ready`：两取节点 + 一灌名字 + 一连接
// ---------------------------------------------------------------------------

/// `ready` 的一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyStep {
    /// `GraphwarNodes::get_node_by_name(self, "GraphwarRootNode")` + `.expect()`（`18:129`）。
    LookupRoot,
    /// 从根节点 `+0x16a8` 的载荷上取名字（共享借用，`22:47`）。
    ReadName,
    /// `get_node_by_name::<LineEdit>(self, "PlayerNameLineEdit")` + `.expect()`（`21:132`）。
    LookupNameEdit,
    /// `LineEdit::set_text(名字)`。
    SetEditText,
    /// **再**取一次 `PlayerNameLineEdit` + `.expect()`（`52:132`）。
    LookupNameEditAgain,
    /// `Callable::from_object_method(self, "on_text_change")` → `connect("text_changed", …)`。
    ConnectTextChanged,
}

/// `ready` 的意图结构（**观测**：每一步都对应 arm64 `0x4408d8..0x440ba8` 的一段）。
///
/// 两次取同一个 `PlayerNameLineEdit` 是**事实**（两个不同的
/// `get_node_by_name::<LineEdit>` 调用 + 两个不同的 Option unwrap：`21:132` / `52:132`），
/// 不是抄错。**为什么**取两次未知（见 [`UNKNOWNS`]）。
pub const READY_PLAN: [ReadyStep; 6] = [
    ReadyStep::LookupRoot,
    ReadyStep::ReadName,
    ReadyStep::LookupNameEdit,
    ReadyStep::SetEditText,
    ReadyStep::LookupNameEditAgain,
    ReadyStep::ConnectTextChanged,
];

/// 连线的三元组：`edit.connect("text_changed", Callable::from_object_method(self, "on_text_change"))`。
///
/// 档位：**观测**（arm64 `0x440b2c` 取 `"on_text_change"`(14) →
/// `Callable::from_object_method` → `0x440b58` 取 `"text_changed"`(12) → `Object::connect`）。
pub const TEXT_CHANGED_LINK: (&str, &str) = (SIGNAL_TEXT_CHANGED, METHOD_ON_TEXT_CHANGE);

/// 一步的中文说明（文档/测试用）。
pub const fn ready_step_label(step: ReadyStep) -> &'static str {
    match step {
        ReadyStep::LookupRoot => "取 GraphwarRootNode",
        ReadyStep::ReadName => "从档案读名字（共享借用）",
        ReadyStep::LookupNameEdit => "取 PlayerNameLineEdit",
        ReadyStep::SetEditText => "LineEdit::set_text(名字)",
        ReadyStep::LookupNameEditAgain => "再取 PlayerNameLineEdit",
        ReadyStep::ConnectTextChanged => "text_changed → on_text_change",
    }
}

// ---------------------------------------------------------------------------
// 8. `process`：可见性闸门 + 变更计数器
// ---------------------------------------------------------------------------

/// 本类的状态：**只有一个 `u32`** —— 上一次看到的变更计数器，偏移 `+0x18`。
///
/// 档位：**观测**（`process` 里 `ldr w8,[x19,#0x18]` 与 `str w21,[x19,#0x18]`；
/// 初值 `0` 与 `GraphwarPlayerConfig` 的 `+0xa8 = 0` 一致 ⇒ "刚进场景不重画"）。
/// 字段名 `last_counter` 是**推断**（代码里只能看到"存这里再比"）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Showcase {
    /// 上一次触发重画的计数器值。
    pub last_counter: u32,
}

impl Showcase {
    /// 实例里这个字段的偏移。档位：**观测**。
    pub const LAST_COUNTER_OFFSET: usize = 0x18;

    /// 初值 `0`（= gdext 的零初始化）。
    pub const fn new() -> Self {
        Self { last_counter: 0 }
    }

    /// `process` 的第一道闸门：`CanvasItem::is_visible_in_tree()`。
    ///
    /// 档位：**观测**（arm64 `0x440d6c` `bl …::is_visible_in_tree` + `cbz w0,<ret>`；
    /// x86 `0x4e692f` `call [0x9bc168]` + `test al,al` + `je <ret>`）。
    /// ⇒ **不可见时 `process` 直接返回**，连档案都不碰。
    pub const fn gate(visible_in_tree: bool) -> bool {
        visible_in_tree
    }

    /// 计数器变了要不要重画。档位：**观测**（arm64 `0x440e48`：`ldr w21,[cell+0xc0]`
    /// = 载荷 `+0xa8`；`ldr w8,[x19,#0x18]`；`cmp`；`b.eq <跳过重画>`）。
    pub const fn should_redraw(&self, counter: u32) -> bool {
        self.last_counter != counter
    }

    /// 重画之后把计数器记下来。
    ///
    /// **顺序是观测的**：`base_mut()` + `queue_redraw()` 在前，
    /// `str w21,[x19+0x18]` 在 `0x440e8c` 之后。
    pub fn note_redrawn(&mut self, counter: u32) {
        self.last_counter = counter;
    }

    /// `process` 一帧的完整意图：可见性 → 读计数器 → 变了就要求 `queue_redraw`。
    ///
    /// `counter` 由调用方从档案里读出（[`CFG_CHANGE_COUNTER`]）；
    /// 真正的 `queue_redraw()` 属于 Godot 运行时，本模块只返回 [`ProcessOutcome`]。
    pub fn tick(&mut self, visible_in_tree: bool, counter: u32) -> ProcessOutcome {
        if !Self::gate(visible_in_tree) {
            return ProcessOutcome::NotVisible;
        }
        if !self.should_redraw(counter) {
            return ProcessOutcome::Unchanged;
        }
        self.note_redrawn(counter);
        ProcessOutcome::QueueRedraw
    }
}

/// `process` 一帧的三种结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// 不在树上可见 ⇒ 直接返回（不读档案）。
    NotVisible,
    /// 计数器没变 ⇒ 什么都不做。
    Unchanged,
    /// 计数器变了 ⇒ `queue_redraw()`。
    QueueRedraw,
}

// ---------------------------------------------------------------------------
// 9. `draw`：三层贴图
// ---------------------------------------------------------------------------

/// 一个矩形（Godot `Rect2`，4 个 `f32`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect2 {
    /// 左上角 x。
    pub x: f32,
    /// 左上角 y。
    pub y: f32,
    /// 宽。
    pub w: f32,
    /// 高。
    pub h: f32,
}

impl Rect2 {
    /// 从控件尺寸造矩形：**原点恒为 `(0,0)`**，尺寸就是 `Control::get_size()`。
    ///
    /// 档位：**观测**（x86 `0x4e5cc3` `mov QWORD [rsp+0x108],0x0` = 原点清零；
    /// `0x4e5ccf`/`0x4e5cd8` 把 `get_size()` 的 `xmm0`/`xmm1` 存到 `+0x110`/`+0x114`
    /// = `size.x`/`size.y`；arm64 `0x440248` 同形）。
    pub const fn from_size(w: f32, h: f32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w,
            h,
        }
    }
}

/// 绘制用的颜色（Godot `Color`，RGBA 四个 `f32`）。本模块只用**纯白**。
pub type Color = [f32; 4];

/// 纯白：`.rodata 0x19ed00`。档位：**观测**（x86 `0x4e5d2d`
/// `movdqa xmm0,[rip…] # 19ed00`；arm64 `0x440284` `fmov v0.4s,#1.0`）。
///
/// ⚠️ 与 `docs/16-client.md` §19.7 里 `GraphwarLongPressAnimation::draw` 第 2 次绘制
/// **同一个地址**（`0x19ed00`）—— 两个模块共用一个"白"。
pub const DRAW_COLOR_RODATA: u64 = 0x19ed00;

/// 纯白颜色值。见 [`DRAW_COLOR_RODATA`]。
pub const WHITE: Color = [1.0, 1.0, 1.0, 1.0];

/// 一次 `CanvasItem::draw_texture_rect_full`。
///
/// 档位：**观测**。五个实参逐个可指：
///
/// | 实参 | 值 | 证据 |
/// |---|---|---|
/// | `texture` | `Some(t)` 才画 | `Option` 判别（arm64 `cmp x24,#1`） |
/// | `rect` | [`Rect2::from_size`] | `get_size()` 的两个 `f32` |
/// | `tile` | `false` | `mov w3,wzr` / `xor ecx,ecx` |
/// | `modulate` | [`WHITE`] | `.rodata 0x19ed00` / `fmov 1.0` |
/// | `transpose` | `false` | `mov w5,wzr` / `xor r9d,r9d` |
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawCall {
    /// 画哪一层。
    pub kind: TextureKind,
    /// 铺开的矩形。
    pub rect: Rect2,
    /// Godot 的 `tile` 实参（恒 `false`）。
    pub tile: bool,
    /// Godot 的 `modulate` 实参（恒 [`WHITE`]）。
    pub modulate: Color,
    /// Godot 的 `transpose` 实参（恒 `false`）。
    pub transpose: bool,
}

/// 三层的绘制顺序：**皮肤 → 脸 → 帽子**（后画的盖住先画的）。档位：**观测**。
///
/// 判据：arm64 三次 `bl draw_texture_rect_full` 在 `0x4402b0`/`0x44037c`/`0x440444`，
/// 与之配对的取图调用分别是 `get_skin_texture`(`0x440174`) /
/// `get_face_texture`(`0x4401b0`) / `get_hat_texture`(`0x4401ec`)。
/// x86 侧同序（`0x4e5d77` / `0x4e5e91` / `0x4e5f97` 三个 GOT 绘制调用）。
pub const DRAW_ORDER: [TextureKind; 3] = [
    TextureKind::Skin,
    TextureKind::Face,
    TextureKind::Hat,
];

/// 给定控件尺寸与"哪几层有贴图"，算出这一帧要发的绘制调用。
///
/// `available[i]` 对应 [`DRAW_ORDER`][i]；`false` 的层**不画**（`if let Some(t)`）。
/// 没有贴图时返回空表：**本模块不画任何"缺省占位"**——`draw` 里没有
/// `draw_rect`/`draw_circle`/`draw_string`，见 [`CALLEES`]。
pub fn draw_plan(w: f32, h: f32, available: [bool; 3]) -> Vec<DrawCall> {
    let rect = Rect2::from_size(w, h);
    let mut out = Vec::new();
    let mut i = 0;
    while i < DRAW_ORDER.len() {
        if available[i] {
            out.push(DrawCall {
                kind: DRAW_ORDER[i],
                rect,
                tile: false,
                modulate: WHITE,
                transpose: false,
            });
        }
        i += 1;
    }
    out
}

/// 本模块**没有**读玩家颜色 / 段位 / 队色：绘制只有三次贴图 + 一个白 modulate。
///
/// 档位：**观测**（有界否定：10 个函数的具名被调者穷举在 [`CALLEES`] 里）。
pub const fn touches_color_manager() -> bool {
    false
}

// ---------------------------------------------------------------------------
// 10. `on_text_change`：写回 + 存档 + 计数 +1
// ---------------------------------------------------------------------------

/// `on_text_change(new_text)` 造成的效果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChangeEffect {
    /// 写进档案的名字（`String::from(GString)`，覆盖 [`CFG_NAME`] 那 24 字节）。
    pub new_name: String,
    /// 是否调了 `GraphwarPlayerConfig::save()`（**是**）。
    pub saved: bool,
    /// 变更计数器的增量（**+1**）。
    pub counter_delta: u32,
}

/// `on_text_change` 的语义：`name = 新文本` → `save()` → `counter += 1`。
///
/// 档位：**观测**（arm64 `0x448e04` `String::from(GString)` →
/// `0x448e28..0x448e34` 覆盖载荷 `+0x00..0x18` →
/// `0x448e3c` `GraphwarPlayerConfig::save(&载荷.名字)` →
/// `0x448e40..0x448e54` `[载荷+0xa8] += 1`）。
///
/// ⚠️ 顺序是"**先存档、再加计数**"：`save()` 拿到的已经是新名字，
/// 但计数器在**返回之后**才变 ⇒ 同一帧里 `process` 还看不到变化。
pub fn on_text_change_effect(new_text: &str) -> TextChangeEffect {
    TextChangeEffect {
        new_name: new_text.to_string(),
        saved: true,
        counter_delta: 1,
    }
}

// ---------------------------------------------------------------------------
// 11. 具名被调者（有界否定用）
// ---------------------------------------------------------------------------

/// 本模块 10 个函数**全部**具名被调者（arm64 `.symtab` 逐条给出）。
///
/// 档位：**观测**。这张表就是"它跟颜色表无关"的边界：
/// 没有任何 `graphwar_color_manager` 条目，也没有"段位 / 队色"函数。
pub const CALLEES: [&str; 12] = [
    "GraphwarNodes::get_node_by_name",
    "RawGd::bind",
    "RawGd::bind_mut",
    "Gd::from_obj_sys_weak",
    "Gd::owned_cast",
    "RawGd::check_rtti",
    "Control::get_size",
    "CanvasItem::draw_texture_rect_full",
    "CanvasItem::is_visible_in_tree",
    "CanvasItem::queue_redraw",
    "GraphwarTextureLoader::get_skin_texture",
    "GraphwarPlayerConfig::save",
];

/// 本模块用到的三个取图方法（`GraphwarTextureLoader` 另外三个方法不在本模块）。
pub const TEXTURE_GETTERS_USED: [&str; 3] = [
    "get_skin_texture",
    "get_face_texture",
    "get_hat_texture",
];

// ---------------------------------------------------------------------------
// 12. `name_to_addr.tsv` 的冲突登记
// ---------------------------------------------------------------------------

/// `fixtures/name_to_addr.tsv` 三条"非唯一锚"的处置。
///
/// 档位：**观测（前两条）/ 推断（第三条的左右顺序）**。
pub const NAME_BRIDGE_CONFLICTS: [(&str, &str, &str); 3] = [
    (
        "__default_virtual_call",
        "未知",
        "该函数不引用任何 panic::Location ⇒ 无锚。arm64 侧有真名 \
         (`<PlayerShowcase as UserClass>::__default_virtual_call::virtual_fn`，156 B)，\
         x86 地址未定（疑似被内联进派生初始化）⇒ 不落地。",
    ),
    (
        "class_id",
        "未知",
        "同样无 Location。arm64 有真名（56 B `0x438088`）+ 8 字节 `CLASS_ID` 槽 \
         (`0x8b40b0`)；x86 侧应是同形状的 56 字节函数，但**没有**任何锚能指认\
         是哪一个 ⇒ 不落地。",
    ),
    (
        "__register_constants",
        "歧义",
        "4 个候选 0x4e1160 / 0x4e12e0 / 0x4e87d0 / 0x4e8970 **全部真实**：它们是一个 \
         `#[godot_api]` 块的 4 个生成物（arm64: __register_methods 400 B、\
         __register_constants 400 B、__inner_init 428/448 B）。\
         本节把 __register_constants 记为 0x4e12e0：按 arm64 符号表 + 两侧地址序对应。\
         两个 __register_* 的 x86 跨度都是 384、arm64 都是 400 ⇒ **大小区分不开**，\
         所以这一条只能算推断（缺：能区分这两个闭包的独立指纹）。",
    ),
];

/// 按方法名查冲突登记。
pub fn conflict(name: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    NAME_BRIDGE_CONFLICTS.iter().find(|(n, _, _)| *n == name)
}

// ---------------------------------------------------------------------------
// 13. 未解出的（照实写）
// ---------------------------------------------------------------------------

/// 本节**没有**解出来的东西。档位：**未知**（每条都写清缺哪份证据）。
pub const UNKNOWNS: [&str; 6] = [
    "`__default_virtual_call` / `class_id` 的 x86 地址：两条都没有 panic::Location，\
     只能靠跨架构指纹落；`class_id` 的 `CLASS_ID` 静态槽（arm64 `0x8b40b0`）在 x86 的地址未解。",
    "两个 `#[godot_api]` 块（行 27 / 行 37）各注册了什么：块 27 有 4 个生成物\
     （2 个 __register_* + 2 个 __inner_init），块 37 只有 1 个 __inner_init；\
     哪一个是 `#[func]`、哪一个是 `#[signal]`/属性，静态分不出\
     （缺：注册闭包写进 `.data.rel.ro` 的 `(名字,长度)` 表项）。",
    "`ready` 为什么**取两次** `PlayerNameLineEdit`（21:132 与 52:132 两个不同的 unwrap）：\
     可能是「先取来 set_text、再取来 connect」，也可能第一次的守卫在第二次之前已释放。\
     缺：两次取节点之间那 30 行源码（行 23–51）里到底是什么。",
    "`draw` 里 6 个借用点与源码行的**一一对应**：能证明 76/77/78 与 80/81/82 两组各三次借用、\
     且每组第三行是帽子（列号少 1），但「哪一次借用配哪一行」缺：\
     x86 侧 `RawGd::bind` 被内联后没有独立调用点。",
    "类名字面量 `PlayerShowcase` 在 `.rodata` 的地址与长度：注册闭包里没有可自校验的\
     `(类名ptr, 类名len)` 立即数对（`check_rtti` 拿到的 10 字节串是 gdext 固定的\
     `\"upcast_ref\"`，不是类名）。",
    "`CFG_NAME`/`CFG_SKIN`/`CFG_FACE`/`CFG_HAT` 四个字段的**源码标识符**：\
     偏移与读写指令都钉住了，名字是按 `set_skin/set_face/set_hat` 与默认字面量推的。\
     另外 `+0x18`/`+0x30`/`+0x48` 三槽（`lock()` 会写默认值）的语义未定。",
];

/// 本模块依赖的文档小节（**不要另抄一份**）。
pub const DOC_SECTIONS: [&str; 3] = [
    "docs/16-client.md §22（本模块逐函数规格；本节）",
    "docs/02-module-map.md §18（模块权威清单 / GDExtension 类表）",
    "docs/16-client.md §19.7（`0x19ed00` = Color{1,1,1,1}，与长按动画共用）",
];

// ---------------------------------------------------------------------------
// 14. 复现（不依赖本节结论，任何人可重跑）
// ---------------------------------------------------------------------------

/// 复现命令（写成常量，避免 `docs` 与代码各写一份而走样）。
///
/// 档位：**观测**（`readelf`/`objdump`/`llvm-objdump` 都是只读工具）。
///
/// ```text
/// SO=.reference/steam/2026-08-28/libgraphwarrust.so
/// ARM=.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so
///
/// # 1) 真名与真边界（x86 被 strip，arm64 没有）
/// readelf -sW $ARM | c++filt | grep player_showcase
///
/// # 2) 10 个函数 + 18 个 Location（工具会做两侧双射）
/// ../.venv/bin/python tools/name_bridge.py module player_showcase
///
/// # 3) 四个方法体逐条（x86 用 objdump；arm64 要 llvm-objdump 才有符号名）
/// objdump -d -M intel --start-address=0x4e5400 --stop-address=0x4e60bf $SO
/// llvm-objdump -d --no-show-raw-insn --demangle \
///   --start-address=0x43fe7c --stop-address=0x4408d8 $ARM
///
/// # 4) 「写者唯一」：扫 .text 里 disp == 0x16a0 / 0x16a8 的写指令
/// #    → 各 1 条，都在 GraphwarRootNode::ready（arm64 0x39a44c / 0x39a450）
/// ```
pub const REPRO: &str = "readelf -sW $ARM | c++filt | grep player_showcase; \
                         ../.venv/bin/python tools/name_bridge.py module player_showcase; \
                         objdump -d -M intel --start-address=0x4e5400 --stop-address=0x4e60bf $SO";


// ===========================================================================
// 原版方法体：原版 `PlayerShowcase` 的 gdext 节点类（`Control` 子类）（T57 从 `src/player_showcase.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `PlayerShowcase` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/player_showcase.rs` 搬进 `src/player_showcase.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `player_showcase.rs` 里**，证据：
//! `panic::Location` 证据：`ready 0x4e6480` → `src/player_showcase.rs:18:129`、`process 0x4e68f0` → `:59:133`、`draw 0x4e5400` → `:77:38`、`on_text_change 0x4efd70` → `:31:129`（`fixtures/name_to_addr_v2.tsv`）。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::player_showcase::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `PlayerShowcase` 是**玩家名片预览**（`docs/16-client.md` §22）：四个方法
//! `ready` / `on_text_change` / `process` / `draw`，每件都能指到指令：
//!
//! | 方法 | 原版干什么 | 档位 |
//! |---|---|---|
//! | `ready` | 取 `GraphwarRootNode` → 从 `GraphwarPlayerConfig`（根节点 `+0x16a8`）读**名字** → 灌进 `PlayerNameLineEdit` → **再取一次同一个控件** → `text_changed` 连到自己的 `on_text_change` | **观测**（6 步、两次 `get_node_by_name`） |
//! | `on_text_change` | 名字改回档案（`String::from`）→ `save()` → 变更计数器 **+1** | **观测** |
//! | `process` | `is_visible_in_tree()` 闸门 → 读 `+0xa8` 计数器 → 变了才 `queue_redraw()` | **观测** |
//! | `draw` | 依次画 **皮肤 → 脸 → 帽子** 三层贴图，`draw_texture_rect_full` ×3、`modulate` 恒纯白 | **观测** |
//!
//! `src/player_showcase.rs` 已经把这些复原成纯逻辑（`Showcase::tick` / `draw_plan` /
//! `on_text_change_effect` / `READY_PLAN` / `CFG_*`），但按约定**不碰引擎**。本文件是那层壳：
//!
//! ```text
//! src/player_showcase.rs               纯逻辑：档案字段偏移 / 6 步 ready / 闸门 / 三次绘制
//! src/graphwar_player_config.rs        纯逻辑：GraphwarPlayerConfig（名字 + 三个外观名 + 计数）
//! src/player_showcase.rs ← 本文件：持档案、建控件、真的 draw_texture_rect
//! ```
//!
//! # ⚠️ 一处与任务书不同的地方（按证据改，并说明改了什么）
//!
//! 任务书要求「读配置 + 建 **3 个 `Sprite2D` + 1 个标签**」。按 `docs/16` §22 与
//! `src/player_showcase.rs` 的观测，**原版没有这三个 `Sprite2D`**：它的 `draw` 是
//! `draw_texture_rect_full` ×3 画在**自己**身上（`0x4402b0` / `0x44037c` / `0x440444`），
//! 子节点只有 `ready` 里**取**（不是建）的那个 `PlayerNameLineEdit`。
//! 「三个 `Sprite2D` + 一个名字标签」是**兄弟类 `EntityDrawer`** 的形状（§24），两者被记混了。
//!
//! ⇒ 本类的做法：
//!
//! * **默认路径 = 证据**：`_draw` 里三次 `draw_texture_rect`（`draw_plan` 给的顺序与实参）；
//! * 任务书要的 3 个 `Sprite2D` + 1 个标签**仍然提供**，但作为**显式 opt-in 的演示口**
//!   [`PlayerShowcase::build_demo_children`]（默认不建），并在文件与文档里标明**非原版**。
//!   这样"默认跑起来 == 原版形状"，而"任务书要的那个形状"也有一个可断言的方法。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 三层贴图 | `GraphwarTextureLoader`（根节点 `+0x16a0`）的三张 `HashMap`，按**外观名**取 | **程序生成的 8×8 纯色 `ImageTexture`**，颜色按层固定（我方选择）；名字**只**决定"这一层有没有"（空名 ⇒ 无贴图 ⇒ 不画，与 `draw_plan` 的 `if let Some(t)` 同构） | 原版**观测**；纯色是**我方选择** |
//! | 档案从哪来 | 根节点 `+0x16a8` 的 `Rc<RefCell<GraphwarPlayerConfig>>`（`INode::init` 建） | 本类直接持一个 [`GraphwarPlayerConfig`] 值（没有 `GraphwarRootNode` 这个节点类） | 原版**观测**；本类**化简** |
//! | `ready` 第 1 步 | `get_node_by_name::<RootNode>("GraphwarRootNode")` + `.expect()` | 真的 `get_node_or_null("GraphwarRootNode")` ⇒ **拿不到**（没有那个节点），记到 `root_lookup_failures` | 原版**观测**；本类**照形状失败** |
//! | `ready` 第 3/5 步 | 两次 `get_node_by_name::<LineEdit>("PlayerNameLineEdit")` + `.expect()`（**为什么两次未知**，`UNKNOWNS`） | 真的查两次；控件由本类在 `ready` 开头**自己建**（场景本来该提供它） | 两次查找**观测**；"自己建"是**我方选择** |
//! | `ready` 第 6 步 | `text_changed` → `on_text_change` | 真连（`TEXT_CHANGED_LINK` 那对名字） | **观测** |
//! | `on_text_change` 的 `save()` | 真写盘（`FileAccess::open` + `store_string` + `close`） | **不写盘**（headless 验收不该有副作用）；只更新内存里的档案 + 计数 | 原版**逐位**；本类**故意省略** |
//! | `process` 闸门 | `is_visible_in_tree()` 为假 ⇒ 直接返回，**连档案都不碰** | 同（闸门在 `Showcase::tick` 里） | **观测** |
//! | 是否读段位 / 队色 | **不读**（有界否定：10 个函数的具名被调者穷举在 `CALLEES`，无颜色表、无段位） | 同（本类**没有**任何段位 / 队色 / 颜色表的东西），见 [`PlayerShowcase::touches_rank`] | **观测**（有界否定） |
//! | 那 3 个 `Sprite2D` + 1 个标签 | **不存在** | 只在 [`PlayerShowcase::build_demo_children`] 里建，**默认不建** | **非原版**（演示） |
//!
//! ⇒ 这个类证明的是「**档案 → 三层贴图 + 计数器闸门**这条链能真的跑起来」，
//! **不是**"名片外观已复原"（贴图是纯色、档案没有磁盘后端、根节点不存在）。
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
//! PlayerShowcase : Control
//!   @export texture_size : int = 8        # 程序生成贴图的边长
//!
//!   player_name() -> String               # 档案 +0x00
//!   skin() / face() / hat() -> String     # 档案 +0x60 / +0x78 / +0x90
//!   change_counter() -> int               # 档案 +0xa8
//!   ready_steps() -> PackedStringArray     # 原版 ready 的 6 步（READY_PLAN，观测）
//!   name_edit_path() -> String             # "PlayerNameLineEdit"（原版字面量）
//!   on_text_change(text: String) -> int    # 对应原版；返回新的 change_counter
//!   set_player_name(text: String) -> int   # 只改档案（不经过控件、不算 on_text_change）
//!   layer_count() -> int                   # 这一帧 draw_plan 会画几层（0..=3）
//!   available_layers() -> PackedInt32Array # 哪几层有贴图（0/1 各一项）
//!   build_demo_children() -> int           # **非原版**：3 个 Sprite2D + 1 个 Label，返回 4
//!   demo_child_count() -> int
//!   touches_rank() -> bool                 # 恒 false（有界否定）
//!   draw_call_count() / drawn_texture_rect_total() / redraw_request_count() -> int
//!   process_call_count() / ready_call_count() / root_lookup_failures() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`Gd<Node>` 不保活**：`LineEdit` 子节点归**父节点**所有。本类在 `ready` 里建完就
//!    先 `add_child`、再在同一个 `ready` 里**重新查找**（这也是原版第 5 步的形状），
//!    句柄与节点同生共死。
//! 2. **`base_mut()` 是独占借用** ⇒ `_draw` 里画东西那段不能同时写 `self.rect_total`
//!    （`E0499`）；本类在 `base` 的作用域**之后**累加。
//! 3. **`resized` 之类只在 `ready` 里连**（本类不连信号，但 `text_changed` 是 `ready` 里连的）
//!    ⇒ 树外 `set_text` 也会触发（信号连上就行，不要求进树）；这一点验收脚本会区分。
//! 4. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 5. **`draw_*` 只能在 `NOTIFICATION_DRAW` 里调**（引擎侧 `ERR_DRAW_GUARD`）⇒ 改档案之后
//!    只 `queue_redraw()`。



use godot::classes::image::Format;
use godot::classes::{Control, IControl, Image, ImageTexture, LineEdit, Node, Sprite2D, Texture2D};
use godot::prelude::*;

use crate::graphwar_player_config::GraphwarPlayerConfig;
use crate::player_showcase::{self as ps, TextureKind};

/// 程序生成贴图的默认边长（像素）。**我方选择**（原版贴图在 pck 里）。
pub const DEFAULT_TEXTURE_SIZE: i32 = 8;

/// 三层贴图的纯色（RGBA，`0..=1`）。**我方选择**：原版是 `GraphwarTextureLoader` 里的真贴图。
///
/// 顺序与 [`ps::DRAW_ORDER`]（皮肤 → 脸 → 帽子）一致。
const LAYER_COLORS: [(f32, f32, f32, f32); 3] = [
    (0.93, 0.76, 0.62, 1.0), // 皮肤
    (1.00, 1.00, 1.00, 1.0), // 脸
    (0.20, 0.32, 0.62, 1.0), // 帽子
];

/// 把 `(r, g, b, a)` 填成 `edge × edge` 的纯色 `ImageTexture`。
///
/// 与 `entity_drawer.rs::solid_texture` 是同一个做法（没有抽公共函数：
/// 那个文件不在本批的可改清单里，抽出去就得动它）。
fn solid_texture(edge: i32, rgba: (f32, f32, f32, f32)) -> Option<Gd<Texture2D>> {
    let edge = edge.max(1);
    let bytes_per_pixel = 4usize;
    let pixel_count = (edge as usize) * (edge as usize);

    let channel = |v: f32| -> u8 { (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 };
    let pixel = [
        channel(rgba.0),
        channel(rgba.1),
        channel(rgba.2),
        channel(rgba.3),
    ];

    let mut data = PackedByteArray::new();
    data.resize(pixel_count * bytes_per_pixel);
    for i in 0..pixel_count {
        for (k, &b) in pixel.iter().enumerate() {
            data[i * bytes_per_pixel + k] = b;
        }
    }

    let image = Image::create_from_data(edge, edge, false, Format::RGBA8, &data)?;
    let texture = ImageTexture::create_from_image(&image)?;
    Some(texture.upcast::<Texture2D>())
}

/// `TextureKind` → [`LAYER_COLORS`] 的下标（按 [`ps::DRAW_ORDER`] 的位置，不按判别值）。
fn layer_index(kind: TextureKind) -> usize {
    ps::DRAW_ORDER
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0)
}

/// 档案里的某个外观名（皮肤 / 脸 / 帽子）。
fn appearance_name(config: &GraphwarPlayerConfig, kind: TextureKind) -> &str {
    match kind {
        TextureKind::Skin => &config.skin,
        TextureKind::Face => &config.face,
        TextureKind::Hat => &config.hat,
    }
}

/// `PlayerShowcase`：玩家名片预览（`Control` 子类）。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct PlayerShowcase {
    base: Base<Control>,

    /// 程序生成贴图的边长（像素）。
    #[export]
    texture_size: i64,

    /// 玩家外观档案（原版在根节点 `+0x16a8` 的 `Rc<RefCell<…>>` 里）。
    ///
    /// 默认值 = [`ps::DEFAULT_SKIN`] / [`ps::DEFAULT_FACE`] / [`ps::DEFAULT_HAT`]
    /// （`GraphwarPlayerConfig::new` 写进去的三条字面量，**观测**），名字留空。
    config: GraphwarPlayerConfig,

    /// `process` 的闸门状态：只有**一个** `u32`（原版 `+0x18`）。
    showcase: ps::Showcase,

    /// 本次实例有没有已经建过 `PlayerNameLineEdit`（`ready` 幂等）。
    edit_created: bool,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// 累计发出过几条 `draw_texture_rect`。
    rect_total: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// `_ready` 被引擎调用过几次。
    ready_calls: i64,
    /// 计数器变了 ⇒ `queue_redraw()` 的次数。
    redraw_requests: i64,
    /// `ready` 第 1 步（找 `GraphwarRootNode`）失败过几次——本类**一定**失败（没有那个节点）。
    root_lookup_failures: i64,
    /// `build_demo_children` 建了几个（0 = 没建）。
    demo_children: i64,
}

#[godot_api]
impl IControl for PlayerShowcase {
    fn init(base: Base<Control>) -> Self {
        // 档案默认值 = `GraphwarPlayerConfig::new(load())` 的兜底形状：三个默认外观名 + 计数 0。
        let config = GraphwarPlayerConfig {
            name: String::new(),
            skin: ps::DEFAULT_SKIN.to_string(),
            face: ps::DEFAULT_FACE.to_string(),
            hat: ps::DEFAULT_HAT.to_string(),
            unlocked: Vec::new(),
            change_counter: 0,
        };
        Self {
            base,
            texture_size: DEFAULT_TEXTURE_SIZE as i64,
            config,
            showcase: ps::Showcase::new(),
            edit_created: false,
            draw_calls: 0,
            rect_total: 0,
            process_calls: 0,
            ready_calls: 0,
            redraw_requests: 0,
            root_lookup_failures: 0,
            demo_children: 0,
        }
    }

    /// `_ready`：照 [`ps::READY_PLAN`] 的 6 步走（顺序**观测**）。
    ///
    /// 第 1 步真的会失败（本类没有 `GraphwarRootNode`）；第 3/5 步真的查两次
    /// `PlayerNameLineEdit`；第 6 步真的连 `text_changed`。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // 场景本来该提供 `PlayerNameLineEdit`；本类没有场景 ⇒ 自己建一个（我方选择）。
        // 必须在第 3 步**之前**建，否则那一步查不到东西。
        self.ensure_name_edit();

        // ① LookupRoot：`get_node_by_name::<RootNode>("GraphwarRootNode")` + `.expect()`。
        //    本类没有那个节点 ⇒ 真的拿不到。原版在这里 `.expect()` 会 panic，本类记数后继续
        //    （headless 验收不该因为"整棵场景树还没做出来"而挂）。
        let root = self
            .base()
            .get_node_or_null(&NodePath::from(ps::NODE_ROOT));
        if root.is_none() {
            self.root_lookup_failures += 1;
        }

        // ② ReadName：原版从根节点单元 `+0x16a8` 的载荷上共享借一次取名字。
        //    本类直接读自己持有的档案（化简，见文件头对照表）。
        let name = self.config.name.clone();

        // ③ LookupNameEdit + ④ SetEditText：`LineEdit::set_text(名字)`。
        let first = self.lookup_name_edit();
        if let Some(mut edit) = first {
            edit.set_text(&GString::from(name.as_str()));
        }

        // ⑤ LookupNameEditAgain：原版**再取一次**同一个控件（为什么两次未知，`UNKNOWNS`）。
        let second = self.lookup_name_edit();

        // ⑥ ConnectTextChanged：`text_changed` → `on_text_change`（名字见 `TEXT_CHANGED_LINK`）。
        if let Some(mut edit) = second {
            let (signal, handler) = ps::TEXT_CHANGED_LINK;
            let myself = self.to_gd();
            let callable = myself.callable(handler);
            let err = edit.connect(signal, &callable);
            if err != godot::global::Error::OK {
                godot_warn!("PlayerShowcase::ready 连接 {signal} 失败: {err:?}");
            }
        }
    }

    /// `_process`：原版 `PlayerShowcase::process`（`0x4e68f0`）。
    ///
    /// 闸门 + 计数器轮询整个在 [`ps::Showcase::tick`] 里（不可见 ⇒ `NotVisible`，
    /// 计数器没变 ⇒ `Unchanged`，变了 ⇒ `QueueRedraw`）。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;

        let visible = self.base().is_visible_in_tree();
        let counter = self.config.change_counter;
        if self.showcase.tick(visible, counter) == ps::ProcessOutcome::QueueRedraw {
            self.redraw_requests += 1;
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `PlayerShowcase::draw`（`0x4e5400`）——皮肤 → 脸 → 帽子**三层贴图**。
    ///
    /// 三次 `draw_texture_rect_full` 的实参逐个来自 [`ps::draw_plan`]：
    /// `rect = Rect2((0,0), get_size())`、`tile = false`、`modulate = 纯白`、`transpose = false`。
    /// 没有贴图的层**不画**（`if let Some(t)`），本类也不画任何占位矩形（原版就没有）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let size = self.base().get_size();
        let available = self.available_flags();
        let plan = ps::draw_plan(size.x, size.y, available);

        let mut drawn = 0i64;
        // 贴图必须在 `base_mut()` **之前**全部取好：`BaseMut<'_, Self>` 握着 `&mut self`，
        // 那段里连 `self.texture_for(..)`（`&self`）都借不到（`E0502`，文件头第 2 条）。
        let textures: Vec<Option<Gd<Texture2D>>> =
            plan.iter().map(|call| self.texture_for(call.kind)).collect();
        {
            let mut base = self.base_mut();
            for (call, texture) in plan.iter().zip(textures.iter()) {
                let Some(texture) = texture else {
                    continue;
                };
                let rect = Rect2::new(
                    Vector2::new(call.rect.x, call.rect.y),
                    Vector2::new(call.rect.w, call.rect.h),
                );
                let modulate = Color::from_rgba(
                    call.modulate[0],
                    call.modulate[1],
                    call.modulate[2],
                    call.modulate[3],
                );
                base.draw_texture_rect_ex(texture, rect, call.tile)
                    .modulate(modulate)
                    .transpose(call.transpose)
                    .done();
                drawn += 1;
            }
        }
        self.rect_total += drawn;
    }
}

#[godot_api]
impl PlayerShowcase {
    /// 建 / 复用名字输入框 `PlayerNameLineEdit`（原版是场景提供的，本类自己建）。
    ///
    /// 名字**取原版字面量** [`ps::NODE_NAME_EDIT`]，长度也是原版记的 18。
    fn ensure_name_edit(&mut self) {
        if self.edit_created {
            return;
        }
        let mut edit = LineEdit::new_alloc();
        edit.set_name(&StringName::from(ps::NODE_NAME_EDIT));
        edit.set_text(&GString::from(self.config.name.as_str()));
        let node = edit.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        self.edit_created = true;
    }

    /// 按名字查一次 `PlayerNameLineEdit`（原版 `get_node_by_name::<LineEdit>`）。
    fn lookup_name_edit(&self) -> Option<Gd<LineEdit>> {
        let node = self
            .base()
            .get_node_or_null(&NodePath::from(ps::NODE_NAME_EDIT))?;
        node.try_cast::<LineEdit>().ok()
    }

    /// 三层里哪几层有贴图（空外观名 ⇒ 没贴图 ⇒ `draw_plan` 不画那一层）。
    fn available_flags(&self) -> [bool; 3] {
        let mut out = [false; 3];
        for (i, kind) in ps::DRAW_ORDER.iter().enumerate() {
            out[i] = !appearance_name(&self.config, *kind).is_empty();
        }
        out
    }

    /// 懒建某一层的程序生成贴图（`None` = 没有贴图，那一层不画）。
    fn texture_for(&self, kind: TextureKind) -> Option<Gd<Texture2D>> {
        let name = appearance_name(&self.config, kind);
        if name.is_empty() {
            return None;
        }
        solid_texture(self.texture_size as i32, LAYER_COLORS[layer_index(kind)])
    }

    /// 玩家名字（档案 `+0x00`）。
    #[func]
    fn player_name(&self) -> GString {
        GString::from(self.config.name.as_str())
    }

    /// 皮肤名（档案 `+0x60`）。
    #[func]
    fn skin(&self) -> GString {
        GString::from(self.config.skin.as_str())
    }

    /// 脸名（档案 `+0x78`）。
    #[func]
    fn face(&self) -> GString {
        GString::from(self.config.face.as_str())
    }

    /// 帽子名（档案 `+0x90`）。
    #[func]
    fn hat(&self) -> GString {
        GString::from(self.config.hat.as_str())
    }

    /// 变更计数器（档案 `+0xa8`）。
    #[func]
    fn change_counter(&self) -> i64 {
        self.config.change_counter as i64
    }

    /// 原版 `ready` 的 6 步（[`ps::READY_PLAN`]），给验收脚本对着看。
    #[func]
    fn ready_steps(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for step in ps::READY_PLAN {
            out.push(&GString::from(ps::ready_step_label(step)));
        }
        out
    }

    /// 原版那个控件的名字（`"PlayerNameLineEdit"`，字面量长 18）。
    #[func]
    fn name_edit_path(&self) -> GString {
        GString::from(ps::NODE_NAME_EDIT)
    }

    /// 等价原版 `on_text_change`：`name = 新文本` → `save()` → **计数器 +1**。
    ///
    /// `save()` 真的会写盘（`FileAccess` + `store_string`），本类**故意不写**
    /// （headless 验收不该产生副作用）；计数器与名字照原版的顺序更新。
    ///
    /// 返回新的 `change_counter()`。
    #[func]
    fn on_text_change(&mut self, text: GString) -> i64 {
        let effect = ps::on_text_change_effect(&text.to_string());
        self.config.name = effect.new_name;
        if effect.saved {
            // 原版在这里 `GraphwarPlayerConfig::save(&载荷.名字)`。本类不写盘 —— 见文件头。
        }
        // 原版是 `[载荷+0xa8] += 1`：`add w8,w8,#1` 是**回绕**加法。
        self.config.change_counter = self
            .config
            .change_counter
            .wrapping_add(effect.counter_delta);
        self.base_mut().queue_redraw();
        self.config.change_counter as i64
    }

    /// 只改档案里的名字（不过控件、不算 `on_text_change` 的计数 +1）。
    ///
    /// 验收脚本用它准备初始状态；游戏里没有这条路径（名字只能从 `LineEdit` 来）。
    #[func]
    fn set_player_name(&mut self, text: GString) -> i64 {
        self.config.name = text.to_string();
        self.base_mut().queue_redraw();
        self.config.change_counter as i64
    }

    /// 这一帧 `draw_plan` 会画几层（`0..=3`）。
    #[func]
    fn layer_count(&self) -> i64 {
        let flags = self.available_flags();
        ps::draw_plan(0.0, 0.0, flags).len() as i64
    }

    /// 哪几层有贴图（按 [`ps::DRAW_ORDER`] 的顺序，`0` / `1` 各一项）。
    #[func]
    fn available_layers(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for flag in self.available_flags() {
            out.push(i32::from(flag));
        }
        out
    }

    /// **非原版**：建 3 个 `Sprite2D`（皮肤 / 脸 / 帽子，各挂程序生成贴图、名字 = 配置里的
    /// 外观名）+ 1 个 `Label`（玩家名字）。
    ///
    /// 原版**没有**这些子节点（见文件头的对照表），它们只是"读配置 → 建节点"这条链路的演示。
    /// 幂等：建过就不再建。返回这次创建了几个（恒 `4`）。
    #[func]
    fn build_demo_children(&mut self) -> i64 {
        if self.demo_children > 0 {
            return self.demo_children;
        }
        let mut made = 0i64;
        for kind in ps::DRAW_ORDER {
            let name = appearance_name(&self.config, kind).to_string();
            let Some(texture) = solid_texture(self.texture_size as i32, LAYER_COLORS[layer_index(kind)])
            else {
                continue;
            };
            let mut sprite = Sprite2D::new_alloc();
            // 名字 = **档案里的外观名**（这就是"读配置"最直白的演示）。
            sprite.set_name(&StringName::from(name.as_str()));
            sprite.set_texture(&texture);
            let node = sprite.clone().upcast::<Node>();
            self.base_mut().add_child(&node);
            made += 1;
        }
        let mut label = godot::classes::Label::new_alloc();
        label.set_text(&GString::from(self.config.name.as_str()));
        let node = label.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        made += 1;
        self.demo_children = made;
        made
    }

    /// `build_demo_children` 建了几个（0 = 没建）。
    #[func]
    fn demo_child_count(&self) -> i64 {
        self.demo_children
    }

    /// 本类读不读段位 / 队色 —— 恒 `false`。
    ///
    /// 有界否定：原版本模块 10 个函数的具名被调者穷举在 [`ps::CALLEES`]，
    /// 没有 `graphwar_color_manager`、没有段位函数（[`ps::touches_color_manager`] 也是 `false`）。
    #[func]
    fn touches_rank(&self) -> bool {
        ps::touches_color_manager()
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_texture_rect`。
    #[func]
    fn drawn_texture_rect_total(&self) -> i64 {
        self.rect_total
    }

    /// 计数器变了 ⇒ `queue_redraw()` 的次数。
    #[func]
    fn redraw_request_count(&self) -> i64 {
        self.redraw_requests
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

    /// `ready` 第 1 步（找 `GraphwarRootNode`）失败过几次。
    #[func]
    fn root_lookup_failures(&self) -> i64 {
        self.root_lookup_failures
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 绘制顺序是**皮肤 → 脸 → 帽子**（后画的盖住先画的），三层颜色互不相同。
    #[test]
    fn draw_order_is_skin_face_hat_and_colors_are_distinct() {
        assert_eq!(
            ps::DRAW_ORDER,
            [TextureKind::Skin, TextureKind::Face, TextureKind::Hat]
        );
        assert_eq!(layer_index(TextureKind::Skin), 0);
        assert_eq!(layer_index(TextureKind::Face), 1);
        assert_eq!(layer_index(TextureKind::Hat), 2);
        assert_ne!(LAYER_COLORS[0], LAYER_COLORS[1]);
        assert_ne!(LAYER_COLORS[1], LAYER_COLORS[2]);
        assert_ne!(LAYER_COLORS[0], LAYER_COLORS[2]);
    }

    /// 三次绘制铺满自身矩形、`tile` / `transpose` 恒 false、`modulate` 恒纯白（`0x19ed00`）。
    #[test]
    fn draw_plan_is_three_white_rects_filling_the_control() {
        let plan = ps::draw_plan(120.0, 160.0, [true, true, true]);
        assert_eq!(plan.len(), 3);
        for (i, call) in plan.iter().enumerate() {
            assert_eq!(call.kind, ps::DRAW_ORDER[i]);
            assert_eq!((call.rect.x, call.rect.y), (0.0, 0.0), "原点恒 (0,0)");
            assert_eq!((call.rect.w, call.rect.h), (120.0, 160.0));
            assert!(!call.tile);
            assert!(!call.transpose);
            assert_eq!(call.modulate, ps::WHITE);
        }
        // 缺一层就不画那一层（`if let Some(t)`）。
        let only_hat = ps::draw_plan(1.0, 1.0, [false, false, true]);
        assert_eq!(only_hat.len(), 1);
        assert_eq!(only_hat[0].kind, TextureKind::Hat);
        assert!(ps::draw_plan(1.0, 1.0, [false, false, false]).is_empty());
    }

    /// `process` 的三道闸门：不可见 ⇒ 不碰档案；没变 ⇒ 不动；变了 ⇒ 要重画。
    #[test]
    fn process_outcome_follows_the_visibility_gate_and_counter() {
        let mut s = ps::Showcase::new();
        assert_eq!(s.tick(false, 5), ps::ProcessOutcome::NotVisible);
        assert_eq!(s.last_counter, 0, "不可见时计数器都不该被记下来");
        assert_eq!(s.tick(true, 0), ps::ProcessOutcome::Unchanged);
        assert_eq!(s.tick(true, 5), ps::ProcessOutcome::QueueRedraw);
        assert_eq!(s.last_counter, 5, "重画之后才记计数器");
        assert_eq!(s.tick(true, 5), ps::ProcessOutcome::Unchanged);
    }

    /// `on_text_change` 的语义：名字改了、存档**真的**发生、计数器 +1（顺序：先存档再加计数）。
    #[test]
    fn text_change_updates_name_saves_and_increments() {
        let e = ps::on_text_change_effect("Alice");
        assert_eq!(e.new_name, "Alice");
        assert!(e.saved);
        assert_eq!(e.counter_delta, 1);
        // 本类用回绕加法（原版 `add w8,w8,#1`）。
        assert_eq!(u32::MAX.wrapping_add(1), 0);
    }

    /// 档案三个默认外观名就是 `GraphwarPlayerConfig::new` 写进去的那三条字面量。
    #[test]
    fn default_appearance_names_come_from_the_pure_module() {
        assert_eq!(ps::DEFAULT_SKIN, "yellow");
        assert_eq!(ps::DEFAULT_FACE, "regular_eyes");
        assert_eq!(ps::DEFAULT_HAT, "soldier_helmet");
        assert_eq!(ps::DEFAULT_LENS, (6, 12, 14));
        let mut cfg = GraphwarPlayerConfig::default();
        cfg.skin = ps::DEFAULT_SKIN.to_string();
        cfg.face = ps::DEFAULT_FACE.to_string();
        cfg.hat = ps::DEFAULT_HAT.to_string();
        assert_eq!(appearance_name(&cfg, TextureKind::Skin), "yellow");
        assert_eq!(appearance_name(&cfg, TextureKind::Face), "regular_eyes");
        assert_eq!(appearance_name(&cfg, TextureKind::Hat), "soldier_helmet");
    }

    /// 有界否定：本模块**不读**段位 / 队色。本类的 `touches_rank()` 就是这一条的直接映射。
    #[test]
    fn the_module_never_touches_rank_or_team_colors() {
        assert!(!ps::touches_color_manager());
        assert!(!ps::CALLEES.iter().any(|c| c.contains("color_manager")));
        assert!(!ps::CALLEES.iter().any(|c| c.contains("rank")));
    }

    /// `ready` 的 6 步与两个节点名都是原版字面量（长度也照原版记的）。
    #[test]
    fn ready_plan_and_node_names_are_the_original_literals() {
        assert_eq!(ps::READY_PLAN.len(), 6);
        assert_eq!(ps::NODE_ROOT, "GraphwarRootNode");
        assert_eq!(ps::NODE_NAME_EDIT, "PlayerNameLineEdit");
        assert_eq!(ps::NODE_ROOT_LEN as usize, ps::NODE_ROOT.len());
        assert_eq!(ps::NODE_NAME_EDIT_LEN as usize, ps::NODE_NAME_EDIT.len());
        assert_eq!(ps::TEXT_CHANGED_LINK, ("text_changed", "on_text_change"));
        assert_eq!(ps::METHOD_ON_TEXT_CHANGE_LEN as usize, "on_text_change".len());
        assert_eq!(ps::SIGNAL_TEXT_CHANGED_LEN as usize, "text_changed".len());
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
