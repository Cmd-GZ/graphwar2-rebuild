//! `graphwar_player_screen_control.rs` —— **玩家页（自定义名字 / 皮肤 / 脸 / 帽子）控制器**
//! （8 个可归属函数）。`docs/coverage-matrix.tsv` 里这一格原来是 `未知`
//! （"只有 docs/02 一行，无地址级观测"）；本轮定名 + 定位 8 个函数 + 解出全部直接调用关系。
//!
//! # 一页结论
//!
//! 1. **它就是写 `GraphwarPlayerConfig` 的地方。** 三个按钮回调分别调
//!    `GraphwarPlayerConfig::set_skin` / `set_face` / `set_hat`（arm64 `.symtab`
//!    具名 `bl`，一一对应，见 [`BUTTON_ACTIONS`]）。这直接回答了"玩家页改外观时谁改档案"。
//! 2. 另三个函数 `lock()` / `pending()` / `unlock()` 只做**可见性**：
//!    取 `SkinBlock` 与 `PendingBlock` 两个节点、各调一次 `CanvasItem::set_visible`。
//! 3. `load_items()` 是**造按钮的那一个**：`PackedScene::instantiate_as` ×2 +
//!    `godot_core::tools::save_load::load` ×2 + `Button::set_button_icon` /
//!    `set_expand_icon` / `Control::set_mouse_filter` / `set_force_pass_scroll_events`。
//!    它把 `load_skins` / `load_faces` / `load_hats` **全部内联**进来
//!    （arm64 侧只剩三个 `::{{closure}}` 符号，见 [`INLINED_LOADERS`]）。
//! 4. `update_buttons_background()` 取 `FacesVbox` / `HatsVbox`，
//!    再用 `GraphwarTextureLoader::get_skin_texture` 给每个按钮刷底色
//!    —— 这就是 `docs/16` §7.5 "Faces / Hats 的底色 = 当前皮肤"的**实现位置**。
//!
//! # 8 个可归属函数（`fixtures/name_to_addr.tsv` 的 8 条 `唯一锚`）
//!
//! 每条的 join 都是**双向唯一**的 `(file, line, col)` 锚，所以"名字 ↔ x86 地址"这一跳可信。
//!
//! | # | 函数 | x86 vaddr | size | 锚（`src/graphwar_player_screen_control.rs:line:col`） |
//! |---|---|---|---|---|
//! | 1 | `load_items` | `0x4f57d0` | `0x1fa0` | `113:39` + 8 条内联锚（135/139/156/159/165/183/186/192） |
//! | 2 | `skin_button_pressed` | `0x4f7940` | `0x270` | `118:28`、`120:118`、`123:68` |
//! | 3 | `face_button_pressed` | `0x4f7870` | `0xd0` | `127:28` |
//! | 4 | `hat_button_pressed` | `0x4f77a0` | `0xd0` | `131:28` |
//! | 5 | `update_buttons_background` | `0x4f8050` | `0x2e0` | `230:113`、`233:111` |
//! | 6 | `lock` | `0x4f8850` | `0x140` | `252:107`、`255:113` |
//! | 7 | `pending` | `0x4f8ad0` | `0x140` | `260:107`、`263:113` |
//! | 8 | `unlock` | `0x4f8990` | `0x140` | `268:107`、`271:113` |
//!
//! 另外 arm64 `.symtab` 还有 3 个**无锚**的同模块符号（`fixtures` 里标 `歧义`）：
//! `insert_button_in_vbox`（`0x44fa48`，1012 B）、`update_buttons_background_in_vbox`
//! （`0x450020`，1180 B）、以及三个 `load_*::{{closure}}`。
//! 它们在 x86 侧的地址**未定**（[`UNANCHORED_ARM64`]），本节不猜。
//!
//! # 结构体（偏移 = 观测，字段名 = 推断）
//!
//! 8 个函数的第一个实参 `%rdi` 都指同一个结构体，两个字段区被用到：
//!
//! | 偏移 | 内容 | 判据 |
//! |---|---|---|
//! | `+0x00` | `Rc<RefCell<GraphwarPlayerConfig>>` | `mov rbp,[rdi]` → `[rbp+0x10]` 借用计数检查 / `[rbp+0x18]` 载荷；三个 `set_*` 与 `lock`/`pending` 都直接对它操作 |
//! | `+0x10` / `+0x18` | 一个**两字宽**句柄，两半各自做非零检查，作为 `get_node_by_name` 的查找起点 | `lock`/`unlock`/`pending` 只读这两处；`skin_button_pressed` 用它找 `GraphwarRootNode` |
//!
//! ⇒ 与 `docs/16` §25.9 的根节点字段表**不冲突**：根节点 `+0x16a8` 持有**同一个** `Rc` 的
//! 一份克隆，而本模块把这个 `Rc` 克隆**缓存在自己身上**（`+0x00`）。所以"写档案"这一步
//! 既有 `.text` 里唯一的 `str [+0x16a8]`（在 `GraphwarRootNode::INode::init`），
//! 也有本模块 `+0x00` 这条读路径。
//!
//! # 地址约定
//!
//! 本文件一律用 **ELF 虚地址（vaddr）**；Ghidra / REA 地址 = 本文件地址 + `0x100000`。
//! arm64 侧的地址（`0x44xxxx`）来自 2.0.60 arm64 的 `.symtab`，**与 x86 地址不是同一空间**。
//!
//! # 证据档位
//!
//! `观测` = 能指到具体指令 / 具体符号；`推断` = 有依据但没证；`未知` = 没解出来。
//! 本文件**没有**"逐位"级结论（界面控制器没有可与真二进制逐字节对拍的对象）。
//! 末尾 [`unknowns`] 照实列出没解出来的部分。

// ---------------------------------------------------------------------------
// 1. 8 个可归属函数
// ---------------------------------------------------------------------------

/// 本模块的源文件名（与原版 `panic::Location` 的 `file` 字段逐字节相同）。
pub const MODULE_PATH: &str = "src/graphwar_player_screen_control.rs";

/// `load_items`：**造按钮**的主函数。`0x1fa0` 字节，内联了三个 `load_*`。**观测**。
pub const FN_LOAD_ITEMS: u64 = 0x4f_57d0;

/// `hat_button_pressed`：帽子按钮回调。**观测**（锚 `131:28`）。
pub const FN_HAT_BUTTON_PRESSED: u64 = 0x4f_77a0;

/// `face_button_pressed`：脸按钮回调。**观测**（锚 `127:28`）。
pub const FN_FACE_BUTTON_PRESSED: u64 = 0x4f_7870;

/// `skin_button_pressed`：皮肤按钮回调。**观测**（锚 `118:28`）。
pub const FN_SKIN_BUTTON_PRESSED: u64 = 0x4f_7940;

/// `update_buttons_background`：按当前皮肤刷按钮底色。**观测**（锚 `230:113`）。
pub const FN_UPDATE_BUTTONS_BACKGROUND: u64 = 0x4f_8050;

/// `lock`：锁住外观（`SkinBlock` 显示、`PendingBlock` 隐藏）。**观测**（锚 `252:107`）。
pub const FN_LOCK: u64 = 0x4f_8850;

/// `unlock`：解锁外观（两个块都隐藏）。**观测**（锚 `268:107`）。
pub const FN_UNLOCK: u64 = 0x4f_8990;

/// `pending`：内购挂起（`SkinBlock` 隐藏、`PendingBlock` 显示）。**观测**（锚 `260:107`）。
pub const FN_PENDING: u64 = 0x4f_8ad0;

/// 8 个可归属函数的 `(名字, x86 vaddr, size)`，**按 x86 地址升序**。
pub const ATTRIBUTED: [(&str, u64, u32); 8] = [
    ("load_items", FN_LOAD_ITEMS, 0x1fa0),
    ("hat_button_pressed", FN_HAT_BUTTON_PRESSED, 0xd0),
    ("face_button_pressed", FN_FACE_BUTTON_PRESSED, 0xd0),
    ("skin_button_pressed", FN_SKIN_BUTTON_PRESSED, 0x270),
    ("update_buttons_background", FN_UPDATE_BUTTONS_BACKGROUND, 0x2e0),
    ("lock", FN_LOCK, 0x140),
    ("unlock", FN_UNLOCK, 0x140),
    ("pending", FN_PENDING, 0x140),
];

/// 归属本模块的函数数（= `coverage_matrix` 的 `orig_funcs`）。**观测**。
pub const ATTRIBUTED_FUNCS: usize = 8;

/// 本模块 22 个 `panic::Location` 结构体覆盖的源码行列（去重、升序）。
///
/// 档位：**观测**（`tools/loc_xrefs.py` 从 `.rela.dyn` 的 addend 解出，逐条可复算）。
/// 这些行列是**同一份源文件内的行号**，因此可以用来反推源文件的分节：
/// 113 → 118 → 120 → 123 → 127 → 131 → 135…192 → 230/233 → 252/255 → 260/263 → 268/271。
pub const ANCHOR_LINES: [(u32, u32); 22] = [
    (113, 39),
    (118, 28),
    (120, 118),
    (123, 68),
    (127, 28),
    (131, 28),
    (135, 103),
    (139, 74),
    (156, 103),
    (159, 74),
    (165, 64),
    (183, 102),
    (186, 73),
    (192, 64),
    (230, 113),
    (233, 111),
    (252, 107),
    (255, 113),
    (260, 107),
    (263, 113),
    (268, 107),
    (271, 113),
];

/// 某一行出现的次数（用来判断"哪几行是同一段内联代码在各调用点的重复实例"）。
///
/// 例：`118:28` / `120:118` / `123:68` 是 `skin_button_pressed` 里
/// `borrow_mut()` / `unwrap()` / 借用计数溢出检查三处 panic；`135/139`、`156/159`、
/// `183/186` 三对则分别是 `load_skins` / `load_faces` / `load_hats` 的对应三处，
/// 被内联进 `load_items` 后各留一份 ⇒ `load_items` 的锚点最多（9 条）。
pub fn lines_of(col: u32) -> Vec<u32> {
    ANCHOR_LINES
        .iter()
        .filter(|(_, c)| *c == col)
        .map(|(l, _)| *l)
        .collect()
}

// ---------------------------------------------------------------------------
// 2. 三个按钮回调 → 三个 `GraphwarPlayerConfig` setter（本节最强的一条证据）
// ---------------------------------------------------------------------------

/// 三个按钮回调各自改档案里的哪个字段。
///
/// 档位：**观测**。判据有两条，互相独立：
///
/// * **arm64 具名被调者**（`bl` 解析到符号名）：`skin_button_pressed` 的唯一业务调用是
///   `graphwarrust::graphwar_player_config::GraphwarPlayerConfig::set_skin`，
///   `face_button_pressed` → `set_face`，`hat_button_pressed` → `set_hat`；
/// * **x86 调用点**（三处都是 `call *[.data.rel.ro 槽]`，槽的 `R_X86_64_RELATIVE`
///   addend 解出真实入口）：
///   `0x9bb6e0 → 0x417c70`、`0x9bb6e8 → 0x417d50`、`0x9bb6f0 → 0x417e20`，
///   三个函数体内分别写 `+0x90`（帽）/ `+0x78`（脸）/ `+0x60`（皮肤）并
///   `inc dword [rbx+0xa8]`（变更计数器 +1）—— 与 `docs/16` §25.9 的
///   `GraphwarPlayerConfig` 字段表**逐字段吻合**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    /// 皮肤（`+0x60`，默认 `yellow`）。
    Skin,
    /// 脸（`+0x78`，默认 `regular_eyes`）。
    Face,
    /// 帽子（`+0x90`，默认 `soldier_helmet`）。
    Hat,
}

impl ConfigField {
    /// 该字段在 `GraphwarPlayerConfig` 里的字节偏移（**观测**，见 `docs/16` §25.9）。
    pub fn offset(self) -> u32 {
        match self {
            ConfigField::Skin => 0x60,
            ConfigField::Face => 0x78,
            ConfigField::Hat => 0x90,
        }
    }

    /// 该字段的默认值（`GraphwarPlayerConfig::new` 写的字面量）。**观测**。
    pub fn default_value(self) -> &'static str {
        match self {
            ConfigField::Skin => "yellow",
            ConfigField::Face => "regular_eyes",
            ConfigField::Hat => "soldier_helmet",
        }
    }

    /// 对应的 setter 名（arm64 符号名最后一段）。**观测**。
    pub fn setter(self) -> &'static str {
        match self {
            ConfigField::Skin => "set_skin",
            ConfigField::Face => "set_face",
            ConfigField::Hat => "set_hat",
        }
    }
}

/// 一次"点按钮 → 改档案"的入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonAction {
    /// 回调函数名。
    pub callback: &'static str,
    /// 回调的 x86 vaddr。
    pub fn_addr: u64,
    /// 它写的档案字段。
    pub field: ConfigField,
    /// x86 侧那三个 setter 的入口（`.data.rel.ro` 槽的 addend）。
    pub setter_addr: u64,
    /// 承载 setter 地址的 `.data.rel.ro` 槽。
    pub setter_slot: u64,
}

/// 三个按钮回调 → 三个 setter。见 [`ButtonAction`] 的档位说明。
///
/// ⚠️ **推断**（不是观测）：x86 侧这三个 setter 是**经槽间接调用**的，不是 `call rel32`。
/// 槽被 `R_X86_64_RELATIVE` 静态写入，所以 addend 就是真实入口——这一步是观测；
/// 但"为什么走间接调用"（`#[godot_api]` 方法表？`OnceLock` 初始化？）**未知**。
pub const BUTTON_ACTIONS: [ButtonAction; 3] = [
    ButtonAction {
        callback: "skin_button_pressed",
        fn_addr: FN_SKIN_BUTTON_PRESSED,
        field: ConfigField::Skin,
        setter_addr: 0x41_7e20,
        setter_slot: 0x9b_b6f0,
    },
    ButtonAction {
        callback: "face_button_pressed",
        fn_addr: FN_FACE_BUTTON_PRESSED,
        field: ConfigField::Face,
        setter_addr: 0x41_7d50,
        setter_slot: 0x9b_b6e8,
    },
    ButtonAction {
        callback: "hat_button_pressed",
        fn_addr: FN_HAT_BUTTON_PRESSED,
        field: ConfigField::Hat,
        setter_addr: 0x41_7c70,
        setter_slot: 0x9b_b6e0,
    },
];

/// 按回调地址查它改哪个字段。
pub fn field_of_callback(fn_addr: u64) -> Option<ConfigField> {
    BUTTON_ACTIONS
        .iter()
        .find(|a| a.fn_addr == fn_addr)
        .map(|a| a.field)
}

/// 按 setter 入口反查字段。
pub fn field_of_setter(setter_addr: u64) -> Option<ConfigField> {
    BUTTON_ACTIONS
        .iter()
        .find(|a| a.setter_addr == setter_addr)
        .map(|a| a.field)
}

// ---------------------------------------------------------------------------
// 3. lock / pending / unlock：只动可见性
// ---------------------------------------------------------------------------

/// 玩家页右列下半屏的两个"块"（`docs/16` §2.1 的 `.tscn` 台账里已登记）。
///
/// `SkinBlock` = "Support the game / Unlocking skins also removes all ads / Unlock"；
/// `PendingBlock` = "Pending…"。原版**桌面端两个都不显示**（真帧那一格是纯底色）。
pub const BLOCKS: [&str; 2] = ["SkinBlock", "PendingBlock"];

/// 三个状态函数各自把两个块置成什么可见性。
///
/// 档位：**观测**。判据是 x86 里那 6 条 `call *0x9ba978`，其 `%esi` 取自
/// `xor %esi,%esi`（= 0）或 `mov $0x1,%esi`（= 1）：
///
/// | 函数 | `SkinBlock` | `PendingBlock` | 指令地址（`%esi` 的来源） |
/// |---|---|---|---|
/// | `lock` `0x4f8850` | `1` | `0` | `0x4f88da` / `0x4f8955`（`xor`） |
/// | `pending` `0x4f8ad0` | `0`（`xor`） | `1` | `0x4f8b62` / `0x4f8bdd` |
/// | `unlock` `0x4f8990` | `0`（`xor`） | `0`（`xor`） | `0x4f8a22` / `0x4f8a9d` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockState {
    /// 未解锁：显示"买断"块，隐藏"挂起"块。
    Locked,
    /// 内购挂起：隐藏"买断"块，显示"挂起"块。
    Pending,
    /// 已解锁：两个块都隐藏。
    Unlocked,
}

impl BlockState {
    /// 该状态下两个块的可见性，**顺序与二进制里的调用顺序一致**（先 `SkinBlock` 后 `PendingBlock`）。
    pub fn visibility(self) -> [bool; 2] {
        match self {
            // 0x4f8850: SkinBlock=1, PendingBlock=0
            BlockState::Locked => [true, false],
            // 0x4f8ad0: SkinBlock=0, PendingBlock=1
            BlockState::Pending => [false, true],
            // 0x4f8990: SkinBlock=0, PendingBlock=0
            BlockState::Unlocked => [false, false],
        }
    }

    /// `(块名, 可见)` 对，便于直接对拍。
    pub fn blocks(self) -> [(&'static str, bool); 2] {
        let v = self.visibility();
        [(BLOCKS[0], v[0]), (BLOCKS[1], v[1])]
    }

    /// 该状态对应的函数地址。
    pub fn fn_addr(self) -> u64 {
        match self {
            BlockState::Locked => FN_LOCK,
            BlockState::Pending => FN_PENDING,
            BlockState::Unlocked => FN_UNLOCK,
        }
    }

    /// 由函数地址反查状态。
    pub fn from_fn_addr(addr: u64) -> Option<BlockState> {
        [BlockState::Locked, BlockState::Pending, BlockState::Unlocked]
            .iter()
            .copied()
            .find(|s| s.fn_addr() == addr)
    }
}

/// `call *0x9ba978(node, bool)` 里那个 GOT 槽。**观测（地址）+ 推断（语义）**。
///
/// **就地更正一条旧结论**：`docs/14-turn.md` §9.3 / `docs/16-client.md` §15.9 曾把
/// `0x9ba978`、`0x9baa10` 登记为"引擎侧成员名未知，需要运行时读 GOT"。
/// 本轮不需要运行时了——
///
/// * 槽的 `R_X86_64_RELATIVE` addend = `0x8ab200`（一个 gdext 生成的方法包装，
///   与 `draw_arc` 的包装 `0x8abca0` 同一段），**观测**；
/// * 本模块 `lock`/`unlock`/`pending` 是**唯一**一类"取 2 个节点 + 各调一次
///   `0x9ba978` + 只传 bool"的函数；同一批函数在 arm64 侧具名 `bl` 全部是
///   `CanvasItem::set_visible`（×2，逐条解出）⇒ `0x9ba978` = `CanvasItem::set_visible`
///   的 GOT 槽。**推断（强）**，缺的证据：`0x8ab200` 体内那条
///   `GDExtensionInterfaceObjectMethodBindPtrcall` 的 method bind 名字。
pub const SET_VISIBLE_SLOT: u64 = 0x9b_a978;

/// `0x9ba978` 槽指向的包装函数地址（`R_X86_64_RELATIVE` addend）。**观测**。
pub const SET_VISIBLE_WRAPPER: u64 = 0x8a_b200;

// ---------------------------------------------------------------------------
// 4. 节点名（观测：`.rodata` 字面量 + 长度立即数）
// ---------------------------------------------------------------------------

/// 本模块 8 个函数里 `get_node_by_name` 用过的节点名与长度。
///
/// 档位：**观测**（`lea rcx` + 紧随的 `mov r8d,<len>`）。
/// 全部经 `graphwar_nodes` 的 `get_node_by_name::<T>` 取得——见 `src/graphwar_nodes.rs`
/// 的 `NAMES_BY_ACCESSOR`（这里只重复本模块真正用到的那几条，便于自校验）。
pub const NODE_NAMES: [(&str, usize); 6] = [
    ("SkinsVbox", 9),
    ("FacesVbox", 9),
    ("HatsVbox", 8),
    ("GraphwarRootNode", 16),
    ("SkinBlock", 9),
    ("PendingBlock", 12),
];

/// 某个函数用到的节点名（**观测**）。
pub const NODES_BY_FUNC: [(&str, &[&str]); 6] = [
    ("load_items", &["SkinsVbox", "FacesVbox", "HatsVbox"]),
    ("update_buttons_background", &["FacesVbox", "HatsVbox"]),
    ("skin_button_pressed", &["GraphwarRootNode"]),
    ("face_button_pressed", &[]),
    ("hat_button_pressed", &[]),
    ("lock_/unlock_/pending", &["SkinBlock", "PendingBlock"]),
];

/// 本模块用的 `graphwar_nodes` 单态化（x86 vaddr）。
///
/// 档位：**观测**（`call rel32`）。三个不同的 `T`：
/// `0x3aafa0`（容器类，实参全是 `*Vbox` + 容器）、`0x3ab190`（控件类，实参是
/// `*Grid`/`*Block`/`FireContainer`）、`0x3ab590`（`GraphwarRootNode`）。
/// ⚠️ `T` 的确切类型**未知**（见 `src/graphwar_nodes.rs` 的 unknowns 第 1 条）。
pub const NODE_ACCESSORS_USED: [u64; 3] = [0x3a_afa0, 0x3a_b190, 0x3a_b590];

// ---------------------------------------------------------------------------
// 5. 被内联的三个 loader + 两个无锚函数
// ---------------------------------------------------------------------------

/// 三个 `load_*` 只剩 `::{{closure}}` 符号 ⇒ 外层已被内联进 [`FN_LOAD_ITEMS`]。
///
/// 档位：**观测**。arm64 `.symtab` 里 `GraphwarPlayerScreenControl` 的 14 个符号中，
/// `load_skins` / `load_faces` / `load_hats` 只有 `28_$u7b$$u7b$closure$u7d$$u7d$`
/// （即 `::{{closure}}`）各一份、都 52 字节。这也解释了为什么
/// `fixtures/name_to_addr.tsv` 给它们标"未知 — 该函数不引用任何 panic::Location"：
/// **它们没有独立函数体了**。
pub const INLINED_LOADERS: [(&str, u64, u32); 3] = [
    ("load_skins::{{closure}}", 0x0044_f5c8, 52),
    ("load_faces::{{closure}}", 0x0044_de9c, 52),
    ("load_hats::{{closure}}", 0x0045_0888, 52),
];

/// 三个 `load_*` 的**代码行**（由内联后残留的锚点反推）。
///
/// 档位：**推断**。依据：`load_items` 里 9 条锚点分成 4 组——
/// `113`（自己）+ `135/139`、`156/159`、`183/186`，每组都是"借方 + `unwrap`"两条，
/// 与三个 `load_*` 的 `borrow_mut()` + `unwrap()` 结构一致；且 x86 侧这三组紧邻
/// 三次 `get_node_by_name("SkinsVbox"/"FacesVbox"/"HatsVbox")`。
/// 缺的证据：行号顺序（135/156/183）与 `SkinsVbox`/`FacesVbox`/`HatsVbox` 的**先后**
/// 需要逐段对齐才能定死（本轮只做到"三个块各占一段"）。
pub const LOADER_LINES: [(&str, u32); 3] = [
    ("load_skins", 135),
    ("load_faces", 156),
    ("load_hats", 183),
];

/// arm64 侧 3 个**有符号但 x86 地址未定**的同模块函数。
///
/// 档位：**观测**（arm64 符号名 + 大小）/ x86 地址 **未知**。
/// 它们在 `fixtures/name_to_addr.tsv` 里只拿到 `歧义`（候选里混着 `graphwar_nodes`
/// 的 11 个单态化 —— 因为那些函数也引用同一个 `src/graphwar_nodes.rs:8:62`）。
pub const UNANCHORED_ARM64: [(&str, u64, u32); 3] = [
    ("insert_button_in_vbox", 0x0044_fa48, 1012),
    ("update_buttons_background_in_vbox", 0x0045_0020, 1180),
    ("load_items", 0x0044_ded0, 5880),
];

// ---------------------------------------------------------------------------
// 6. 具名被调者（arm64 `.symtab`，逐条解出）
// ---------------------------------------------------------------------------

/// 某个函数调用过的**游戏内**函数（arm64 `bl` 解析到 `graphwarrust::…` 符号）。
///
/// 档位：**观测**。godot/gdext/alloc/panic 那些基础被调者已滤掉（它们只说明"形状"，
/// 不说明业务）。完整清单见 `docs/16-client.md` §29.3。
pub const GAME_CALLEES: [(&str, &[&str]); 6] = [
    (
        "load_items",
        &[
            "GraphwarNodes::get_node_by_name",
            "GraphwarNodes::get_node_by_name_rec",
            "GraphwarPlayerScreenControl::insert_button_in_vbox",
            "GraphwarTextureLoader::get_skin_texture",
        ],
    ),
    (
        "skin_button_pressed",
        &[
            "GraphwarNodes::get_node_by_name",
            "GraphwarPlayerConfig::set_skin",
            "GraphwarPlayerScreenControl::update_buttons_background",
        ],
    ),
    (
        "face_button_pressed",
        &["GraphwarPlayerConfig::set_face"],
    ),
    ("hat_button_pressed", &["GraphwarPlayerConfig::set_hat"]),
    (
        "update_buttons_background",
        &[
            "GraphwarNodes::get_node_by_name",
            "GraphwarPlayerScreenControl::update_buttons_background_in_vbox",
            "GraphwarTextureLoader::get_skin_texture",
        ],
    ),
    (
        "lock / unlock / pending",
        &[
            "GraphwarNodes::get_node_by_name",
            "CanvasItem::set_visible",
        ],
    ),
];

/// `load_items` 用到的 gdext 侧"造控件"调用（**观测**，说明它是**造按钮**的那一个）。
///
/// 这些不是游戏自己的模块，但决定了这个函数在源码里的角色，所以单列。
pub const LOAD_ITEMS_GODOT_CALLS: [&str; 8] = [
    "PackedScene::instantiate_as",
    "godot_core::tools::save_load::load",
    "Button::set_button_icon",
    "Button::set_expand_icon",
    "Control::set_mouse_filter",
    "Control::set_force_pass_scroll_events",
    "GString::from(&str)",
    "Node::add_child_full",
];

// ---------------------------------------------------------------------------
// 7. 诚实清单
// ---------------------------------------------------------------------------

/// 本节**没解出来**的东西（照实写，不许乐观）。
///
/// 1. **`insert_button_in_vbox` / `update_buttons_background_in_vbox` 的 x86 地址**——
///    arm64 有符号（1012 / 1180 字节，具名被调者是 `Node::add_child_full`、
///    `Node::get_children_full`、`Button::set_button_icon`、`ClassId::to_cow_str`），
///    但 2.0.60 x86 侧没有一对一 `Location` 锚，`fixtures` 只给到 `歧义`。
///    **不替人选**（`AGENTS.md` §0.3）：候选里混着 `graphwar_nodes` 的单态化，
///    本轮**不**做这个判断。
/// 2. **`+0x10` / `+0x18` 那个两字宽句柄的确切类型**——只知道两半各自做非零检查、
///    并被当作 `get_node_by_name` 的查找起点。`Gd<T>` 在 gdext 里是 1 个字宽，
///    所以 2 个字宽**不是**单个 `Gd`；缺的证据：`Base<Node>` 之外还有一个字段
///    （可能是 `InstanceId`）的写点——`.text` 里没有找到唯一写点。
/// 3. **三个 setter 为什么走间接调用**（`call *[slot]` 而不是 `call rel32`）——
///    槽是静态初始化的，addend 就是入口，所以**语义**不受影响；
///    但"为什么没被内联"这一点没解出来。
/// 4. **`0x9bb7c0`（`skin_button_pressed` 里那条间接调用，addend `0x8cac40`）是什么**——
///    它返回 3 个 `dword` 判别值之一（`cmp dword [rsp+0x48], 3`），
///    在 `RawGd::bind` 之后被调用，最可能是 `GraphwarTextureLoader` 的一个方法，
///    **未证**。
/// 5. **玩家页的"存盘时机"**——`GraphwarPlayerConfig::save()` 在 `player_showcase::on_text_change`
///    里被直接调用（`docs/16` §25.6），但本模块三个 setter 之后**没有** `save` 调用
///    ⇒ 改外观不立即落盘，落盘靠别处（可能是 `_exit_tree` / 切屏）。
///    缺的证据：没找到调用 `save` 的唯一时机。
/// 6. **`load_items` 那 3 组内联块的行号 ↔ `SkinsVbox`/`FacesVbox`/`HatsVbox` 的先后**（见 [`LOADER_LINES`]）。
pub fn unknowns() -> &'static [&'static str] {
    &[
        "insert_button_in_vbox / update_buttons_background_in_vbox 的 x86 地址（只有歧义候选）",
        "+0x10/+0x18 两字宽句柄的确切类型",
        "三个 setter 为什么走间接调用（语义不受影响）",
        "0x9bb7c0（addend 0x8cac40）是什么",
        "改外观之后谁来调 save()（本模块不调）",
        "3 组内联块的行号与三个 Vbox 的先后未对齐",
    ]
}

/// 本模块 x86 侧的函数数（`fixtures/name_to_addr.tsv` 里 13 行去重后的名字数）。
///
/// 11 个有名字（8 唯一锚 + 2 歧义 + 1 未知），其中 8 个可归属。
pub const NAME_ROWS: usize = 13;

#[cfg(test)]
mod tests {
    use super::*;

    /// 8 个可归属函数地址升序、互不相同，且 `ATTRIBUTED_FUNCS` 对得上。
    #[test]
    fn eight_functions_are_sorted_and_distinct() {
        assert_eq!(ATTRIBUTED.len(), ATTRIBUTED_FUNCS);
        for w in ATTRIBUTED.windows(2) {
            assert!(w[0].1 < w[1].1, "必须按地址升序");
        }
        let mut a: Vec<u64> = ATTRIBUTED.iter().map(|t| t.1).collect();
        a.dedup();
        assert_eq!(a.len(), 8);
        // lock / pending / unlock 三个大小完全相同（324 / 0x144 附近），形状一致。
        assert_eq!(ATTRIBUTED.iter().filter(|t| t.2 == 0x140).count(), 3);
    }

    /// 三个按钮回调各改一个**不同**的字段，且字段偏移与 `docs/16` §25.9 一致。
    #[test]
    fn buttons_write_three_distinct_config_fields() {
        let fields: Vec<ConfigField> = BUTTON_ACTIONS.iter().map(|a| a.field).collect();
        assert_eq!(fields.len(), 3);
        assert_ne!(fields[0], fields[1]);
        assert_ne!(fields[1], fields[2]);
        assert_ne!(fields[0], fields[2]);

        assert_eq!(ConfigField::Skin.offset(), 0x60);
        assert_eq!(ConfigField::Face.offset(), 0x78);
        assert_eq!(ConfigField::Hat.offset(), 0x90);
        assert_eq!(ConfigField::Skin.default_value(), "yellow");
        assert_eq!(ConfigField::Face.default_value(), "regular_eyes");
        assert_eq!(ConfigField::Hat.default_value(), "soldier_helmet");
        assert_eq!(ConfigField::Skin.setter(), "set_skin");

        assert_eq!(field_of_callback(FN_SKIN_BUTTON_PRESSED), Some(ConfigField::Skin));
        assert_eq!(field_of_callback(FN_FACE_BUTTON_PRESSED), Some(ConfigField::Face));
        assert_eq!(field_of_callback(FN_HAT_BUTTON_PRESSED), Some(ConfigField::Hat));
        assert_eq!(field_of_callback(FN_LOCK), None);
        assert_eq!(field_of_setter(0x41_7c70), Some(ConfigField::Hat));
        assert_eq!(field_of_setter(0x41_7d50), Some(ConfigField::Face));
        assert_eq!(field_of_setter(0x41_7e20), Some(ConfigField::Skin));
        assert_eq!(field_of_setter(0x41_7c00), None);
    }

    /// 三个可见性状态互不相同，且 `SkinBlock` / `PendingBlock` 永远至少有一个是隐藏的
    /// （`unlock` 两个都隐藏）—— `lock` 与 `pending` 恰好互补。
    #[test]
    fn block_states_are_exact() {
        assert_eq!(BlockState::Locked.visibility(), [true, false]);
        assert_eq!(BlockState::Pending.visibility(), [false, true]);
        assert_eq!(BlockState::Unlocked.visibility(), [false, false]);

        let l = BlockState::Locked.visibility();
        let p = BlockState::Pending.visibility();
        for i in 0..2 {
            assert_ne!(l[i], p[i], "lock 与 pending 在第 {i} 个块上必须互补");
        }
        assert_eq!(BlockState::Locked.blocks(), [("SkinBlock", true), ("PendingBlock", false)]);
        assert_eq!(BlockState::Unlocked.blocks(), [("SkinBlock", false), ("PendingBlock", false)]);

        for s in [BlockState::Locked, BlockState::Pending, BlockState::Unlocked] {
            assert_eq!(BlockState::from_fn_addr(s.fn_addr()), Some(s));
        }
        assert_eq!(BlockState::from_fn_addr(FN_LOAD_ITEMS), None);
        assert_eq!(BlockState::Locked.fn_addr(), FN_LOCK);
        assert_eq!(BlockState::Unlocked.fn_addr(), FN_UNLOCK);
        assert_eq!(BlockState::Pending.fn_addr(), FN_PENDING);
    }

    /// 节点名表自洽（长度 = 字节数、不重复），且每个名字都被某个函数用到。
    #[test]
    fn node_names_are_self_consistent() {
        let mut seen = std::collections::BTreeSet::new();
        for (n, l) in NODE_NAMES {
            assert!(seen.insert(n));
            assert_eq!(n.len(), l, "{n} 的长度对不上");
        }
        let used: std::collections::BTreeSet<&str> =
            NODES_BY_FUNC.iter().flat_map(|(_, ns)| ns.iter().copied()).collect();
        for (n, _) in NODE_NAMES {
            assert!(used.contains(n), "{n} 没有被任何函数用到");
        }
        assert_eq!(BLOCKS, ["SkinBlock", "PendingBlock"]);
    }

    /// 22 条锚点的行号集合与 8 个函数的划分对得上：
    /// `load_items` 那一堆行号最多，`lock`/`pending`/`unlock` 各 2 条。
    #[test]
    fn anchor_lines_partition_as_expected() {
        assert_eq!(ANCHOR_LINES.len(), 22);
        assert_eq!(lines_of(107), vec![252, 260, 268]);
        assert_eq!(lines_of(113), vec![230, 255, 263, 271]);
        assert_eq!(lines_of(28), vec![118, 127, 131]);
        // load_items 的 9 条：113 自己 + 4 组内联锚点。
        let in_load_items = [113u32, 135, 139, 156, 159, 165, 183, 186, 192];
        for l in in_load_items {
            assert!(
                ANCHOR_LINES.iter().any(|(line, _)| *line == l),
                "load_items 的锚点 {l} 丢了"
            );
        }
        assert_eq!(LOADER_LINES.len(), 3);
        assert_eq!(INLINED_LOADERS.len(), 3);
        for (_, _, sz) in INLINED_LOADERS {
            assert_eq!(sz, 52, "三个 closure 都是 52 字节");
        }
    }

    /// `set_visible` 槽的结论：槽 / 包装 / 用它的函数必须对得上。
    #[test]
    fn set_visible_slot_is_consistent() {
        assert_eq!(SET_VISIBLE_SLOT, 0x9b_a978);
        assert_eq!(SET_VISIBLE_WRAPPER, 0x8a_b200);
        // 只有 lock / pending / unlock 用它（每函数 2 次 = 2 个块）。
        for s in [BlockState::Locked, BlockState::Pending, BlockState::Unlocked] {
            assert_eq!(s.blocks().len(), 2);
        }
        // 六个字面量块名里只有两个是块。
        assert!(BLOCKS.contains(&"SkinBlock"));
        assert!(!BLOCKS.contains(&"SkinsVbox"));
    }

    /// 被调者表里的每个游戏函数名都得能在 arm64 符号清单里找到（这里只查命名空间前缀）。
    #[test]
    fn game_callees_use_real_namespaces() {
        for (_, callees) in GAME_CALLEES {
            for c in callees {
                assert!(
                    c.starts_with("Graphwar") || c.starts_with("CanvasItem"),
                    "可疑的被调者名：{c}"
                );
            }
        }
        assert_eq!(LOAD_ITEMS_GODOT_CALLS.len(), 8);
        // load_items 是唯一"造"东西的那个（有 instantiate + add_child）。
        assert!(LOAD_ITEMS_GODOT_CALLS[0].contains("instantiate"));
        assert!(LOAD_ITEMS_GODOT_CALLS[7].contains("add_child"));
        assert_eq!(UNANCHORED_ARM64.len(), 3);
        assert_eq!(NODE_ACCESSORS_USED.len(), 3);
        assert_eq!(NAME_ROWS, 13);
    }

    /// 诚实清单不许空着（`AGENTS.md` §0.3）。
    #[test]
    fn unknowns_are_listed() {
        assert_eq!(unknowns().len(), 6);
    }
}
