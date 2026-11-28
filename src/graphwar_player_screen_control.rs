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
//! # 函数体（T45 落码：**8/8**，见本文件 §7）
//!
//! 下面 8 个原版函数**此前只有登记（`fixtures/function_coverage.tsv` 判 `F3 register-only`）**，
//! T45 按地址逐个读反汇编后落码，每个函数体第一行给 vaddr、正文里逐步给指令地址：
//!
//! | 函数 | vaddr | 落码后能指到的东西 |
//! |---|---|---|
//! | [`GraphwarPlayerScreenControl::load_items`] | `0x4f57d0` | 场景 `res://shop_button_entry.tscn`（`esi=0x1c`）、子节点 `Button`（`r8d=6`）、三个容器名与长度、三条 `call 0x4f7bb0`、收尾 `call 0x4f8050` |
//! | [`GraphwarPlayerScreenControl::hat_button_pressed`] | `0x4f77a0` | `call *[0x9bb6e0] → 0x417c70`（`set_hat`） |
//! | [`GraphwarPlayerScreenControl::face_button_pressed`] | `0x4f7870` | `call *[0x9bb6e8] → 0x417d50`（`set_face`） |
//! | [`GraphwarPlayerScreenControl::skin_button_pressed`] | `0x4f7940` | `→0x417e20`（`set_skin`）、取 `GraphwarRootNode`（`r8d=0x10`）、再 `call 0x4f8050` |
//! | [`GraphwarPlayerScreenControl::update_buttons_background`] | `0x4f8050` | 贴图表空则早退（`cmp qword [rcx+0x168],0`）、`FacesVbox`→`HatsVbox` 两条 `call 0x4f8330` |
//! | [`GraphwarPlayerScreenControl::lock`] | `0x4f8850` | `SkinBlock`=`1`（`0x4f88da`）、`PendingBlock`=`0`（`0x4f8955`） |
//! | [`GraphwarPlayerScreenControl::pending`] | `0x4f8ad0` | `SkinBlock`=`0`（`0x4f8b62`）、`PendingBlock`=`1`（`0x4f8bdd`） |
//! | [`GraphwarPlayerScreenControl::unlock`] | `0x4f8990` | 两个块都 `0`（`0x4f8a22` / `0x4f8a9d`） |
//!
//! **档位**：可见性/节点名/长度/调用顺序 = **观测**；条目名表逐条内容 = **推断**（[`VBOXES`]）；
//! 宿主侧动作（`set_visible` / `set_button_icon`）经 [`PlayerScreenHost`] 交给
//! `src/_scaffold/*_class.rs`，本文件不模拟 Godot。
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
//! ⇒ 与 `docs/16` §22.9 的根节点字段表**不冲突**：根节点 `+0x16a8` 持有**同一个** `Rc` 的
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
///   `inc dword [rbx+0xa8]`（变更计数器 +1）—— 与 `docs/16` §22.9 的
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
    /// 该字段在 `GraphwarPlayerConfig` 里的字节偏移（**观测**，见 `docs/16` §22.9）。
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
/// `0x3a_afa0`（容器类，实参全是 `*Vbox` + 容器）、`0x3a_b190`（控件类，实参是
/// `*Grid`/`*Block`/`FireContainer`）、`0x3a_b590`（`GraphwarRootNode`）。
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
/// 不说明业务）。完整清单见 `docs/16-client.md` §26.3。
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
// 7. 函数体（T45 落码：8/8）
//
// 每个函数第一行注释 = **x86 vaddr + 档位**。落码口径：
// 控制流 / 节点名 / 长度立即数 / 可见性实参 / 调用顺序全部照反汇编；
// **Godot 侧的动作**（`get_node` / `set_visible` / `set_button_icon`）经
// [`PlayerScreenHost`] 交给宿主，原版那些调用点都写在各方法注释里。
// 本文件不模拟 Godot，也不复制 `client/` 的仿制品逻辑（`AGENTS.md` §0 第 4 条）。
// ---------------------------------------------------------------------------

/// `load_items` 里 `godot_core::tools::save_load::load` 的实参。**观测**：
/// `0x4f5fe9 mov esi,0x1c`（= 28 = 本串字节数）+ `0x4f5fee lea rdi,[0x1c4545]` → `0x4de050`。
pub const SHOP_BUTTON_ENTRY_SCENE: &str = "res://shop_button_entry.tscn";

/// 每个条目里取子节点 `Button`。**观测**：`0x4f60e1 lea rcx,[0x1c4561]` + `0x4f60d6 mov r8d,6`
/// → `0x3ad570`（`Node::get_node` 一类的单态化）。
pub const BUTTON_CHILD: &str = "Button";

/// `SkinsVbox`：`load_items` 第一个容器。**观测**（`0x1c4567` + `mov r8d,9`）。
pub const SKINS_VBOX: &str = "SkinsVbox";
/// `FacesVbox`：`load_items` 第二个容器、`update_buttons_background` 第一个。**观测**（`0x1c44bb` + `mov r8d,9`）。
pub const FACES_VBOX: &str = "FacesVbox";
/// `HatsVbox`：第三 / 第二个容器。**观测**（`0x19f0b0` + `mov r8d,8`）。
pub const HATS_VBOX: &str = "HatsVbox";

/// 三张表落进哪个容器 + 容器名的长度立即数 + 条目名表。
///
/// * 容器名与长度：**观测**（上面三个常量各自的 `lea` + `mov r8d,<len>`）；
/// * **顺序**（Skins → Faces → Hats）：**观测** —— `load_items` 里三次
///   `get_node_by_name` 的地址升序跟三次 `0x4f7bb0` 调用点（`0x4f5d1b` / `0x4f6359` / `0x4f6a20`）同序；
/// * 条目名表：**推断** —— 内容取自 `.rodata` 的拼接串（`0x1c44bb` 起：`regular_eyes…`、
///   `SkinsVbox` 之后的 `purple…`、`no_hat` 之后的 `soldier_helmet…`），
///   与 `client/src/ui/icons.gd` 已登记的 `.rodata` 顺序表一致；
///   **缺的证据**：本函数给每条 `String` 的长度立即数没有逐条核对 ⇒ 每条边界仍可能是"拼错一两个字母"。
pub const VBOXES: [VboxSpec; 3] = [
    VboxSpec { node: SKINS_VBOX, name_len: 9, items: SKIN_ITEMS },
    VboxSpec { node: FACES_VBOX, name_len: 9, items: FACE_ITEMS },
    VboxSpec { node: HATS_VBOX, name_len: 8, items: HAT_ITEMS },
];

/// 一个"容器 + 它的条目表"。
#[derive(Debug, Clone, Copy)]
pub struct VboxSpec {
    /// 容器节点名。
    pub node: &'static str,
    /// 该名字的字节数（原版是 `mov r8d,<len>` 立即数）。
    pub name_len: usize,
    /// 要造按钮的条目名（原版从 `.rodata` 取，见 [`VBOXES`] 的档位说明）。
    pub items: &'static [&'static str],
}

/// 皮肤条目表。**推断**（内容见 [`VBOXES`]），长度 = `.rodata` 里 `SkinsVbox` 之后那一段。
pub const SKIN_ITEMS: &[&str] = &[
    "purple", "red", "white_skin", "yellow", "green", "light_green", "true_purple", "brown",
    "calico", "wargod", "arrow", "face_paint_1", "face_paint_2",
];

/// 脸条目表。**推断**（见 [`VBOXES`]）。
pub const FACE_ITEMS: &[&str] = &[
    "regular_eyes", "blue_eyes", "green_eyes", "purple_eyes", "anime_eyes", "aviator_glare",
    "cute_eyes", "nerd_glasses", "baggy_eyes", "asian", "eye_patch", "baby_eyes", "alien_eyes",
];

/// 帽子条目表。**推断**（见 [`VBOXES`]）。
pub const HAT_ITEMS: &[&str] = &[
    "no_hat", "soldier_helmet", "mohawk", "cowboy", "straw_hat", "blonde_boy", "blonde_girl",
    "brunette_girl", "powdered_wig", "brown_boy", "witch_hat", "mohawk_red", "horned_helmet",
    "hair_band", "unicorn", "roman_helmet", "anime_hair", "top_hat", "tricone", "asian_hat",
    "baseball_cap", "crown",
];

/// 玩家页在 Godot 侧要做的动作。**一个方法 = 一条能指到指令的动作**。
///
/// 归属：这些都是 gdext 调用（原版经 GOT 槽 `call *[rip+…]` 或单态化包装），
/// 宿主侧真实转发见 `src/_scaffold/*_class.rs`；本 trait 只声明意图。
pub trait PlayerScreenHost {
    /// `get_node_by_name::<T>(name)`（`0x3a_afa0` 容器 / `0x3a_b190` 控件 / `0x3a_b590` 根节点）。
    /// 返回 `false` 表示取节点失败 ⇒ 原版走 `unwrap` 的 panic 臂（例如 `0x4f82ec`），
    /// 这里选择提前返回（**推断**：不 panic 是为可测；原版行为是 panic）。
    fn has_node(&mut self, name: &str) -> bool;

    /// `call *0x9ba978`（**推断 = `CanvasItem::set_visible`**，见 [`SET_VISIBLE_SLOT`]）。
    fn set_block_visible(&mut self, block: &str, visible: bool);

    /// `godot_core::tools::save_load::load::<PackedScene>`（`0x4de050`，实参 = [`SHOP_BUTTON_ENTRY_SCENE`]）。
    fn load_scene(&mut self, path: &str);

    /// `0x4f7bb0`（`load_items` 对每个容器调一次，**观测**）：
    /// 把条目 `item` 造成的按钮插进容器 `vbox`。返回是否真的插进去了。
    fn insert_button(&mut self, vbox: &str, child: &str, item: &str) -> bool;

    /// 取当前皮肤对应的**底色贴图**。原版是 `0x41ce80`（`推断` = `GraphwarTextureLoader::get_skin_texture`）
    /// 加一次 `HashMap` 查表（4 号实参 `[rcx+0x168]` = 表项数、`[rcx+0x170]` = hasher）。
    /// 返回 `None` = 表里没有该皮肤（原版 `0x4f805e` 的 `cmp qword [rcx+0x168],0` 早退）。
    fn background_texture(&mut self, skin: &str) -> Option<String>;

    /// `0x4f8330`（`update_buttons_background` 对每个容器调一次，**观测**）：
    /// 把 `tex` 刷到容器 `vbox` 的每个按钮上。
    fn set_icon_on_children(&mut self, vbox: &str, tex: &str) -> usize;
}

/// 玩家页控制器的**可测模型**。
///
/// 原版这个对象是 gdext 的 `Node` 扩展，字段布局见模块文档 §结构体：
/// `+0x00` = `Rc<RefCell<GraphwarPlayerConfig>>`（**观测**：本文件 8 个函数里有 4 个用
/// `[self+0]` → `[+0x10]` 借用计数 → `[+0x18]` 载荷的路径），`+0x10/+0x18` = `Base<Node>` 句柄。
/// 这里只保留**能被证据钉住**的那部分：档案（`Rc<RefCell<…>>`）与最近一次造好的条目。
#[derive(Debug, Clone, Default)]
pub struct GraphwarPlayerScreenControl {
    /// 原版 `+0x00`：档案的共享句柄。三个按钮回调都**通过它**写字段。
    pub config: std::rc::Rc<std::cell::RefCell<crate::graphwar_player_config::GraphwarPlayerConfig>>,
    /// 最近一次 `load_items` 的产物（几个容器各造出多少个按钮）。
    pub built: [usize; 3],
}

impl GraphwarPlayerScreenControl {
    /// `GraphwarPlayerConfig::new` 的调用方模型（原版 `init` 里 `Rc::new(RefCell::new(config))`）。
    pub fn new(config: crate::graphwar_player_config::GraphwarPlayerConfig) -> Self {
        Self {
            config: std::rc::Rc::new(std::cell::RefCell::new(config)),
            built: [0; 3],
        }
    }

    /// 当前档案例（读）。
    pub fn config(&self) -> std::cell::Ref<'_, crate::graphwar_player_config::GraphwarPlayerConfig> {
        self.config.borrow()
    }

    /// `hat_button_pressed` —— **`0x4f77a0`**（`0xd0` 字节）。
    ///
    /// **观测**：`0x4f782d` 组 24 字节实参 → `0x4f7833 call *[0x9bb6e0]` → `0x417c70`
    /// （= `GraphwarPlayerConfig` 的 `set_hat`，写 `+0x90` 并 `inc dword [+0xa8]`）。
    /// 体内**没有**第二次调用 ⇒ 帽子按钮不改按钮底色（`update_buttons_background` 只被
    /// `skin_button_pressed` 与 `load_items` 调用：全 `.text` 仅 `0x4f7ac0` / `0x4f6fc7` 两处 `call`）。
    pub fn hat_button_pressed<H: PlayerScreenHost + ?Sized>(&mut self, _host: &mut H, value: &str) {
        self.config.borrow_mut().set_hat(value);
    }

    /// `face_button_pressed` —— **`0x4f7870`**（`0xd0` 字节）。
    ///
    /// **观测**：`0x4f7903 call *[0x9bb6e8]` → `0x417d50`（`set_face`，写 `+0x78` + 计数）。
    pub fn face_button_pressed<H: PlayerScreenHost + ?Sized>(&mut self, _host: &mut H, value: &str) {
        self.config.borrow_mut().set_face(value);
    }

    /// `skin_button_pressed` —— **`0x4f7940`**（`0x270` 字节，三个回调里最长的那个）。
    ///
    /// **观测**，三步：
    /// 1. `0x4f79e1 call *[0x9bb6f0]` → `0x417e20`（`set_skin`，写 `+0x60` + 计数）；
    /// 2. `0x4f7a1a..0x4f7a32`：取节点 `GraphwarRootNode`（`0x19e490`，`r8d=0x10`）→
    ///    `0x3a_b590`；再 `[root+0x16a0]` 取**同一个档案 Rc 的另一份克隆**并借用
    ///    （`0x4f7aa3..0x4f7aaf` 的 `0x7fff…` 上限检查 + `+0x10` 借用计数）；
    /// 3. `0x4f7ac0 call 0x4f8050` = [`Self::update_buttons_background`] ——
    ///    **这就是"换皮肤之后 Faces/Hats 按钮底色跟着变"的实现位置**。
    pub fn skin_button_pressed<H: PlayerScreenHost + ?Sized>(&mut self, host: &mut H, value: &str) {
        self.config.borrow_mut().set_skin(value); // ① 0x417e20
        if host.has_node(ROOT_NODE) {
            // ②③ 取根节点、借用档案、刷底色（`0x4f7a1a` → `0x4f7ac0`）
            self.update_buttons_background(host, value);
        }
    }

    /// `lock` —— **`0x4f8850`**（`0x133` 字节）。
    ///
    /// **观测**（指令地址 → 可见性实参）：
    /// `SkinBlock`（`0x1c45e6`，`r8d=9`）→ `0x4f88da mov esi,1` → `call *0x9ba978`；
    /// `PendingBlock`（`0x1c45ef`，`r8d=0xc`）→ `0x4f8955 xor esi,esi`。
    pub fn lock<H: PlayerScreenHost + ?Sized>(&mut self, host: &mut H) {
        for (block, visible) in BlockState::Locked.blocks() {
            host.set_block_visible(block, visible);
        }
    }

    /// `pending` —— **`0x4f8ad0`**（`0x140` 字节）：`SkinBlock` 隐藏（`0x4f8b62 xor esi,esi`）、
    /// `PendingBlock` 显示（`0x4f8bdd mov esi,1`）。
    pub fn pending<H: PlayerScreenHost + ?Sized>(&mut self, host: &mut H) {
        for (block, visible) in BlockState::Pending.blocks() {
            host.set_block_visible(block, visible);
        }
    }

    /// `unlock` —— **`0x4f8990`**（`0x13d` 字节）：两个块都隐藏
    /// （`0x4f8a22` / `0x4f8a9d` 各一条 `xor esi,esi`）。
    pub fn unlock<H: PlayerScreenHost + ?Sized>(&mut self, host: &mut H) {
        for (block, visible) in BlockState::Unlocked.blocks() {
            host.set_block_visible(block, visible);
        }
    }

    /// `update_buttons_background` —— **`0x4f8050`**（`0x2e0` 字节）。
    ///
    /// 实参：`(self, 节点句柄, 皮肤名, 贴图表)`。**观测**：
    /// * `0x4f805e cmp qword [rcx+0x168],0 / je 0x4f82dd` —— 贴图表为空就**整体早退**（什么都不写）；
    /// * `0x4f8077 lea rdi,[rcx+0x170]` + `0x4f8081 call 0x47daa0` = 用 `rdx`（皮肤名）查表；
    /// * 命中后 `0x41ce80` 取贴图，然后 `FacesVbox`（`0x1c44bb`，`r8d=9`，`0x3a_afa0`）
    ///   → `0x4f820f call 0x4f8330`；`HatsVbox`（`0x19f0b0`，`r8d=8`）→ `0x4f8287 call 0x4f8330`；
    /// * 之后 `0x4f8296..0x4f82d7` 还有一段 gdext 收尾（`0x405d20` / `*0x9ba8b8 → 0x88d750` /
    ///   静态 `0x9c18f0` 的 `[+0x14f0]` 开关 → `[+0x418]` 虚调用）。**推断**：那是
    ///   "标脏 / 请求重绘"一类；缺的证据：`0x88d750` 与 `[0x9c18f0+0x418]` 的成员名。
    ///
    /// 返回 `false` = 贴图表里没有这个皮肤（原版早退），`true` = 刷了两个容器。
    pub fn update_buttons_background<H: PlayerScreenHost + ?Sized>(
        &mut self,
        host: &mut H,
        skin: &str,
    ) -> bool {
        let Some(tex) = host.background_texture(skin) else {
            return false; // 0x4f805e：表项数 0 ⇒ 早退
        };
        // 顺序 = 两条 call 的地址升序（Faces 先、Hats 后）
        host.set_icon_on_children(FACES_VBOX, &tex); // 0x4f820f
        host.set_icon_on_children(HATS_VBOX, &tex); // 0x4f8287
        true
    }

    /// `load_items` —— **`0x4f57d0`**（`0x1fa0` = 8096 字节，本模块最大的函数）。
    ///
    /// **观测**（外骨架，逐条给地址）：
    /// 1. `0x4f5fe9 mov esi,0x1c` + `0x4f5fee lea rdi` → `0x4de050`：`load(SHOP_BUTTON_ENTRY_SCENE)`；
    /// 2. 三次 `get_node_by_name`：`SkinsVbox`（`0x1c4567`/9）、`FacesVbox`（`0x1c44bb`/9）、
    ///    `HatsVbox`（`0x19f0b0`/8）；
    /// 3. 每次取到容器后 `call 0x4f7bb0`（`0x4f5d1b` / `0x4f6359` / `0x4f6a20`）= 往该容器造按钮
    ///    （每次还先 `get_node("Button", 6)`，`0x4f60d6`/`0x4f60e1`）；
    /// 4. 收尾 `0x4f6fc7 call 0x4f8050` = 刷一次底色。
    ///
    /// 体内那 8 条 `panic::Location` 锚（`113/135/139/156/159/165/183/186/192`）说明它把
    /// `load_skins`/`load_faces`/`load_hats` 三个 loader 全**内联**了（见 [`INLINED_LOADERS`]）。
    /// **未落码**：每个条目从 `.rodata` 取名的长度立即数（逐条边界），见 [`VBOXES`] 档位。
    pub fn load_items<H: PlayerScreenHost + ?Sized>(&mut self, host: &mut H) -> LoadItemsReport {
        host.load_scene(SHOP_BUTTON_ENTRY_SCENE); // ① 0x4de050
        let mut counts = [0usize; 3];
        for (i, spec) in VBOXES.iter().enumerate() {
            if !host.has_node(spec.node) {
                continue; // 原版此处 unwrap panic（0x4f82ec 一类的臂）
            }
            for item in spec.items {
                if host.insert_button(spec.node, BUTTON_CHILD, item) {
                    counts[i] += 1; // ③ 0x4f7bb0
                }
            }
        }
        self.built = counts;
        // ④ 0x4f6fc7：用**当前**皮肤刷一次底色
        let skin = self.config.borrow().skin.clone();
        let background_updated = self.update_buttons_background(host, &skin);
        LoadItemsReport {
            buttons: counts,
            background_updated,
        }
    }
}

/// `skin_button_pressed` 里 `get_node_by_name` 的根节点名。**观测**：
/// `0x4f7a1a lea rcx,[0x19e490]` + `0x4f7a26 mov r8d,0x10`（16 字节）→ `0x3a_b590`。
pub const ROOT_NODE: &str = "GraphwarRootNode";

/// `load_items` 的产出（便于对拍"三个容器各造了几个"）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadItemsReport {
    /// 三个容器（Skins / Faces / Hats，顺序同 [`VBOXES`]）各成功插入几个按钮。
    pub buttons: [usize; 3],
    /// 收尾那次 `0x4f8050` 是否真的刷了（贴图表里有没有当前皮肤）。
    pub background_updated: bool,
}

// ---------------------------------------------------------------------------
// 8. 诚实清单
// ---------------------------------------------------------------------------

/// 本节**没解出来**的东西（照实写，不许乐观）。
///
/// 1. **`insert_button_in_vbox` / `update_buttons_background_in_vbox` 的 x86 地址**——
///    arm64 有符号（1012 / 1180 字节），但 2.0.60 x86 侧没有一对一 `Location` 锚。
///    **T45 部分解决**：`0x4f57d0`（`load_items`）里有**三条** `call 0x4f7bb0`
///    （`0x4f5d1b` / `0x4f6359` / `0x4f6a20`，每张表一条），`0x4f8050`
///    （`update_buttons_background`）里有**两条** `call 0x4f8330`（`0x4f820f` / `0x4f8287`，
///    `FacesVbox` / `HatsVbox` 各一条）⇒ **这两个 per-vbox 助手的 x86 入口已定**
///    （`0x4f7bb0` 体 `0x49d`、`0x4f8330` 体 `0x438`）。
///    **T113 解决**（2026-11-28）：「哪个 arm64 名字对哪个 x86 入口」已经钉住 ——
///    x86 `0x4f8330` 与 arm64 `update_buttons_background_in_vbox` 引用**同一个**
///    `"SkinBackgroundButton"` 串（x86 `.rodata 0x1c45d2`）⇒ 观测级配对；
///    `0x4f7bb0` = `insert_button_in_vbox` 是**推断**（结构指纹 top-1 + 两侧都恰好
///    **3 条**调用点）。详见 §9。
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
///    里被直接调用（`docs/16` §22.6），但本模块三个 setter 之后**没有** `save` 调用
///    ⇒ 改外观不立即落盘，落盘靠别处（可能是 `_exit_tree` / 切屏）。
///    缺的证据：没找到调用 `save` 的唯一时机。
/// 6. **`load_items` 那 3 组内联块的行号 ↔ `SkinsVbox`/`FacesVbox`/`HatsVbox` 的先后**。
///    **T45 部分解决**：三次 `call 0x4f7bb0` 的地址升序（`0x4f5d1b` → `0x4f6359` → `0x4f6a20`）
///    与三次 `get_node_by_name` 的名字（`SkinsVbox` → `FacesVbox` → `HatsVbox`）同序，
///    也与锚点行号（`135` → `156` → `183`）同序 ⇒ **三块 = Skins/Faces/Hats 这个顺序可以定死**。
///    仍缺：条目名表里**每条**的长度立即数（[`VBOXES`] 的 `items` 是 `推断`）。
pub fn unknowns() -> &'static [&'static str] {
    &[
        // T113：x86 入口已配对上（0x4f8330 观测 / 0x4f7bb0 推断），不再算未知
        "insert_button_in_vbox 的**分支布局**：3 条 add_child 各属哪条路径（类名/append 语义/>=3 判据都已证）",
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

    /// 三个按钮回调各改一个**不同**的字段，且字段偏移与 `docs/16` §22.9 一致。
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

    // ---------------------------------------------------------------------
    // T45：函数体测试。**每个期望值都来自反汇编的立即数/调用点**，
    // 不是"我方实现输出"（`AGENTS.md` §0.5）。出处写在断言上一行。
    // ---------------------------------------------------------------------

    /// 记录宿主动作顺序的假宿主。
    ///
    /// `textures` 为空 ⇒ 模拟原版 `0x4f805e` 的 `cmp qword [rcx+0x168],0` 早退。
    #[derive(Default)]
    struct Recorder {
        log: Vec<String>,
        nodes: Vec<String>,
        /// 贴图表：皮肤名 → 底色贴图名。
        textures: Vec<(&'static str, String)>,
        /// `has_node` 对哪些名字返回假（模拟取节点失败）。
        missing: Vec<&'static str>,
    }

    impl Recorder {
        fn with_texture(skin: &'static str, tex: &str) -> Self {
            Self {
                textures: vec![(skin, tex.to_string())],
                ..Default::default()
            }
        }
    }

    impl PlayerScreenHost for Recorder {
        fn has_node(&mut self, name: &str) -> bool {
            if self.missing.contains(&name) {
                return false;
            }
            self.nodes.push(name.to_string());
            true
        }
        fn set_block_visible(&mut self, block: &str, visible: bool) {
            self.log.push(format!("visible {block} {}", visible as u8));
        }
        fn load_scene(&mut self, path: &str) {
            self.log.push(format!("load {path}"));
        }
        fn insert_button(&mut self, vbox: &str, child: &str, item: &str) -> bool {
            self.log.push(format!("insert {vbox}/{child}/{item}"));
            true
        }
        fn background_texture(&mut self, skin: &str) -> Option<String> {
            self.textures
                .iter()
                .find(|(k, _)| *k == skin)
                .map(|(_, v)| v.clone())
        }
        fn set_icon_on_children(&mut self, vbox: &str, tex: &str) -> usize {
            self.log.push(format!("icons {vbox}={tex}"));
            1
        }
    }

    fn ctrl() -> GraphwarPlayerScreenControl {
        let mut cfg = crate::graphwar_player_config::GraphwarPlayerConfig::default();
        cfg.skin = "yellow".into();
        cfg.face = "regular_eyes".into();
        cfg.hat = "soldier_helmet".into();
        GraphwarPlayerScreenControl::new(cfg)
    }

    /// `lock` / `pending` / `unlock` 的两个块与可见性实参，全部照指令立即数：
    /// `0x4f88da mov esi,1` / `0x4f8955 xor esi,esi`（lock = 1,0）、
    /// `0x4f8b62` / `0x4f8bdd`（pending = 0,1）、`0x4f8a22` / `0x4f8a9d`（unlock = 0,0）。
    #[test]
    fn three_state_functions_write_exact_visibility_in_binary_order() {
        let cases: [(&str, [bool; 2], u64); 3] = [
            ("lock", [true, false], FN_LOCK),
            ("pending", [false, true], FN_PENDING),
            ("unlock", [false, false], FN_UNLOCK),
        ];
        for (name, expect, addr) in cases {
            let mut c = ctrl();
            let mut h = Recorder::default();
            match name {
                "lock" => c.lock(&mut h),
                "pending" => c.pending(&mut h),
                "unlock" => c.unlock(&mut h),
                _ => unreachable!(),
            }
            // 顺序 = 二进制里两条 call 的先后：先 SkinBlock（len 9）后 PendingBlock（len 0xc）
            assert_eq!(h.log.len(), 2, "{name} 必须写两次 set_visible（{addr:#x}）");
            assert_eq!(h.log[0], format!("visible SkinBlock {}", expect[0] as u8));
            assert_eq!(h.log[1], format!("visible PendingBlock {}", expect[1] as u8));
        }
    }

    /// 三个按钮回调各写**自己的**字段；只有 `skin` 回调会再刷底色。
    /// 判据：`call 0x4f8050` 在 `.text` 里只有 `0x4f7ac0`（skin 回调）与 `0x4f6fc7`（load_items）两处。
    #[test]
    fn only_skin_callback_refreshes_background() {
        let mut c = ctrl();
        let mut h = Recorder::with_texture("red", "red_bg");

        c.hat_button_pressed(&mut h, "witch_hat");
        assert_eq!(c.config().hat, "witch_hat");
        assert!(h.log.is_empty(), "hat 回调体内没有第二次调用");

        c.face_button_pressed(&mut h, "alien_eyes");
        assert_eq!(c.config().face, "alien_eyes");
        assert!(h.log.is_empty(), "face 回调体内没有第二次调用");

        c.skin_button_pressed(&mut h, "red");
        assert_eq!(c.config().skin, "red");
        assert_eq!(h.nodes, vec![ROOT_NODE.to_string()], "先取根节点（len 16）");
        assert_eq!(
            h.log,
            vec![
                "icons FacesVbox=red_bg".to_string(),
                "icons HatsVbox=red_bg".to_string()
            ],
            "Faces 先、Hats 后（0x4f820f < 0x4f8287）"
        );
    }

    /// `update_buttons_background`：贴图表空 ⇒ 早退（`0x4f805e`），一个字都不写。
    #[test]
    fn background_refresh_short_circuits_on_empty_table() {
        let mut c = ctrl();
        let mut h = Recorder::default(); // 表空
        assert!(!c.update_buttons_background(&mut h, "yellow"));
        assert!(h.log.is_empty());

        let mut h2 = Recorder::with_texture("yellow", "yellow_bg");
        assert!(c.update_buttons_background(&mut h2, "yellow"));
        assert_eq!(h2.log.len(), 2);
        // 容器名与长度立即数：FacesVbox/9（0x4f81d6）、HatsVbox/8（0x4f824b）
        assert_eq!(FACES_VBOX.len(), 9);
        assert_eq!(HATS_VBOX.len(), 8);
    }

    /// `load_items` 的外骨架：先 `load("res://shop_button_entry.tscn")`（`esi=0x1c`=28），
    /// 三个容器按 Skins→Faces→Hats 顺序，最后刷一次底色（`0x4f6fc7`）。
    #[test]
    fn load_items_order_and_scene_path_match_binary() {
        assert_eq!(SHOP_BUTTON_ENTRY_SCENE.len(), 0x1c, "esi=0x1c 是这条串的长度");
        assert_eq!(BUTTON_CHILD.len(), 6, "r8d=6 是 'Button' 的长度"); // 0x4f60d6
        assert_eq!(ROOT_NODE.len(), 0x10); // 0x4f7a26 r8d=0x10

        let mut c = ctrl();
        let mut h = Recorder::with_texture("yellow", "yellow_bg");
        let r = c.load_items(&mut h);

        // ① 第一条动作是 load
        assert_eq!(h.log[0], format!("load {SHOP_BUTTON_ENTRY_SCENE}"));
        // ② 三个容器的取节点顺序（名字长度 9/9/8）
        let want_nodes: Vec<String> = vec![SKINS_VBOX.into(), FACES_VBOX.into(), HATS_VBOX.into()];
        assert_eq!(h.nodes, want_nodes);
        assert_eq!(VBOXES[0].name_len, 9);
        assert_eq!(VBOXES[1].name_len, 9);
        assert_eq!(VBOXES[2].name_len, 8);
        // ③ 每个容器都被插过按钮，且最后刷底色
        assert_eq!(r.buttons, [SKIN_ITEMS.len(), FACE_ITEMS.len(), HAT_ITEMS.len()]);
        assert!(r.background_updated);
        assert_eq!(
            h.log.last().unwrap(),
            "icons HatsVbox=yellow_bg",
            "收尾那次 0x4f8050 以 Hats 为最后一步"
        );
        // ④ 每个容器都被调过 0x4f7bb0（per-vbox 助手）
        for spec in VBOXES {
            assert!(
                h.log.iter().any(|l| l.starts_with(&format!("insert {}/", spec.node))),
                "{} 没有插入调用",
                spec.node
            );
        }
    }

    /// 条目表本身要自洽（不重复、都非空），避免"推断"表里混进空串。
    #[test]
    fn item_tables_are_sane() {
        for spec in VBOXES {
            assert!(!spec.items.is_empty(), "{} 的表是空的", spec.node);
            let mut seen = std::collections::BTreeSet::new();
            for it in spec.items {
                assert!(!it.is_empty());
                assert!(seen.insert(*it), "{} 里 {} 重复", spec.node, it);
            }
        }
        // 原版默认三件套必须在表里（`docs/16` §22.9）
        assert!(SKIN_ITEMS.contains(&"yellow"));
        assert!(FACE_ITEMS.contains(&"regular_eyes"));
        assert!(HAT_ITEMS.contains(&"soldier_helmet"));
    }
}

// ---------------------------------------------------------------------------
// 9. 两条 `*_in_vbox`（2026-11-28 T113）
// ---------------------------------------------------------------------------
//
// 这两条是 T105/T109 挂在「分母外真缺」里的（`fixtures/denom_outside.tsv` 的
// `0x44fa48` / `0x450020` 两行），T109 的登记是「缺节点模板 + 名字表」。
// 本轮把**能证的部分**落地：
//
// * `update_buttons_background_in_vbox`（arm64 **`0x450020`** / 1180 B）**整条落码**：
//   它的"名字表"只有**一个**名字（`SkinBackgroundButton`），图标是**入参**；
// * `insert_button_in_vbox`（arm64 **`0x44fa48`** / 1012 B）**没有落函数体**，但把三件原本
//   记成"缺"的事证死了：① 新建的节点类是 **`HBoxContainer`**（不是"未知"）；
//   ② `Node::add_child_full` 的实参是 `(force_readable_name=false, internal=0)` ——
//   **没有插入下标**（原版就是 append）；③ 命中判据是"某子节点的子节点数 **>= 3**"。
//   仍缺的是**分支布局**（3 条 `add_child` 各属哪条路径）⇒ 按 §0.3 不编函数体。
//
// # 名字 → 图标映射：T109 记的"缺映射表"其实是**张冠李戴**
//
// `update_buttons_background_in_vbox` 体内**只有** `SkinBackgroundButton` 一个名字，
// 图标由第 2 个参数传进来；"哪张图配哪个 vbox"在**调用方** `update_buttons_background`
// （arm64 `0x44fe3c` / 484 B）里：
//
// ```text
// 0x44fe64  bl GraphwarTextureLoader::get_skin_texture      ; 取当前皮肤贴图（Option）
// 0x44fe90  check_rtti 标签 "clone"                          ; 取 clone 诊断名
// 0x44fec8  bl GraphwarNodes::get_node_by_name("FacesVbox", 9)   ; 槽 0x19de28 / w3=9
// 0x44fee0  bl RawGd::clone                                   ; 贴图句柄
// 0x44fef4  bl update_buttons_background_in_vbox(FacesVbox, 皮肤贴图)
// 0x44ff40  bl GraphwarNodes::get_node_by_name("HatsVbox", 8)    ; 槽 0x1790f8 / w3=8
// 0x44ff6c  bl update_buttons_background_in_vbox(HatsVbox, **同一个**皮肤贴图)
// ```
//
// ⇒ 映射 = **`FacesVbox` 与 `HatsVbox` 都用 `get_skin_texture` 的那张图**，落到各自子树里
// 名为 `SkinBackgroundButton` 的节点上。（`SkinsVbox` 走的是 `load_items`，不在这一对里。）
//
// # x86 侧配对（T45 只定到"两个入口"，本轮定到"哪个对哪个"）
//
// * x86 **`0x4f8330`**（`0x438` B）引用了**同一个** `SkinBackgroundButton` 串
//   （x86 `.rodata 0x1c45d2`）⇒ 它就是 `update_buttons_background_in_vbox` —— **观测**；
// * x86 **`0x4f7bb0`**（`0x49d` B）= `insert_button_in_vbox`：**推断**，两条独立证据同向
//   （`fixtures/hybrid_names.tsv` 的结构指纹 top-1，score 0.7647；且 T45 在 x86
//   `load_items` 里数到 **3 条** `call 0x4f7bb0`，与 arm64 侧 `insert_button_in_vbox`
//   的 **3 个** `bl` 调用点同数）。

/// 被刷图标的节点名：**`"SkinBackgroundButton"`**（20 字节）。
///
/// 档位：**观测**。arm64 槽 **`0x19df3f`**（`0x450090 adrp 0x19d000` + `0x450094 add #0xf3f`，
/// 长度在 `0x4501dc mov w3,#0x14` = 20）；x86 槽 **`0x1c45d2`**（`0x4f8549 lea rcx,...`）。
/// 同池后面紧跟 `SkinBlock` / `PendingBlock` / `no_hat` / `soldier_helmet` / `mohawk` / `cowboy`
/// —— 那些属于 `load_items` 等其他函数，**不在**本条函数体里。
pub const SKIN_BACKGROUND_BUTTON: &str = "SkinBackgroundButton";

/// `insert_button_in_vbox` 新建节点的类：**`HBoxContainer`**（一行按钮的容器）。
///
/// 档位：**观测**，证据链是三级的：
/// 1. `0x44fb70 bl godot_core::classes::class_runtime::construct_engine_object::hda9be1bf82a23c18`；
/// 2. 该单态化函数体里 `0x4b89b8 bl <HBoxContainer as GodotClass>::class_id`（符号名直接写着类）；
/// 3. 之后 `0x7ef13c bl godot_ffi::classdb_construct_object` 用这个 `ClassId` 造对象。
/// ⇒ 新建的是 **HBoxContainer**，不是 Button/TextureButton。
pub const INSERT_ROW_CLASS: &str = "HBoxContainer";

/// `insert_button_in_vbox` 里"这一行还能再放一个"的判据：现有子节点数 **< 3**。
///
/// 档位：**观测**（`0x44fb20 bl AnyArray::len` + `0x44fb24 cmp x0,#3` + `0x44fb28 b.lo 0x44fc34`）。
///
/// 语义（**推断**，缺分支布局审计）：`b.lo` 之后是"往这一行里再塞一个"的路径，
/// `>= 3` 落到"新建一行"。这条推断与 [`INSERT_ROW_CLASS`] = HBoxContainer 自洽，
/// 但没有逐条走完 3 条 `add_child` 的归属。
pub const INSERT_ROW_CAPACITY: usize = 3;

/// `insert_button_in_vbox` 里 3 条 `Node::add_child_full` 的调用点。
///
/// 档位：**观测**（`0x44fbf4` / `0x44fcd0` / `0x44fd48`，都带 `mov w2,wzr` + `mov w3,wzr`）。
///
/// ⇒ `add_child(node, force_readable_name=false, internal=0)` = **追加到末尾**，
/// **不存在"插入下标"这个东西**（T109 记的"插在哪个下标"是前提错误）。
pub const INSERT_ADD_CHILD_SITES: [u64; 3] = [0x44_fbf4, 0x44_fcd0, 0x44_fd48];

/// `add_child` 的两个附加实参：`force_readable_name` / `internal`，两侧都是 0。
///
/// 档位：**观测**（3 个调用点前的 `mov w2,wzr` / `mov w3,wzr`）。
pub const INSERT_ADD_CHILD_FLAGS: (bool, u8) = (false, 0);

/// `update_buttons_background_in_vbox` 需要的宿主能力（引擎侧一律走 trait）。
pub trait ButtonIconSink {
    /// 节点句柄（原版是 `Gd<Node>` / `Gd<Button>` / `Gd<Texture2D>`）。
    type Node: Copy;
    /// 贴图句柄（原版是 `Gd<Texture2D>`）。
    type Texture: Copy;

    /// `Node::get_children_full()`（`0x450068` / `0x450104` / `0x450130`）。
    fn children(&mut self, node: Self::Node) -> Vec<Self::Node>;

    /// `GraphwarNodes::get_node_by_name_rec(node, name)`（`0x4501e0`）。
    fn node_by_name_rec(&mut self, node: Self::Node, name: &str) -> Option<Self::Node>;

    /// `Button::set_button_icon(icon)`（`0x450124`）。
    fn set_button_icon(&mut self, button: Self::Node, icon: Self::Texture);
}

/// `GraphwarPlayerScreenControl::update_buttons_background_in_vbox`
/// （arm64 **`0x450020`** / **1180** B；x86 **`0x4f8330`** / `0x438` B）：
/// 把一个图标刷到 `parent` 子树里所有 `SkinBackgroundButton` 上。
///
/// 逐条指令：
///
/// ```text
/// 0x450040  stp x0, x1, [sp, #0x10]              ; arg1 = 父节点（Gd，两字宽）
/// 0x450044  stp x2, x3, [sp, #0x20]              ; arg2 = 贴图（Gd）
/// 0x450054  mov w2, #0xa  "upcast_ref"
/// 0x450068  bl Node::get_children_full(parent)   ; 外层 AnyArray
/// 0x45009c  bl AnyArray::len
/// 0x4500a4  cmp  x8, x0 ; b.hs → 0x450290        ; 外层循环
/// 0x450104  bl Node::get_children_full(child)    ; 内层 AnyArray（孙节点）
/// 0x450130  bl AnyArray::len（内层）
/// 0x4501dc  mov  w3, #0x14                       ; 名字长度 20
/// 0x4501e0  bl GraphwarNodes::get_node_by_name_rec(grandchild, "SkinBackgroundButton")
/// 0x4501ec  cmp  x8, #1 ; b.ne → 下一个孙节点     ; 仅在命中时往下走
/// 0x450124  bl Button::set_button_icon(命中节点, arg2)
/// ```
///
/// 档位：**观测**（上面每条都指到指令；x86 同函数引用同一个 `SkinBackgroundButton` 串）。
/// 返回值：本层返回"刷了几个"，方便断言；原版无返回值。
pub fn update_buttons_background_in_vbox<S>(
    sink: &mut S,
    parent: S::Node,
    icon: S::Texture,
) -> usize
where
    S: ButtonIconSink + ?Sized,
{
    // 0x450068：外层 = parent 的子节点
    let children = sink.children(parent);
    let mut painted = 0;
    for child in children {
        // 0x450104：内层 = 每个子节点的子节点（孙节点）
        for grandchild in sink.children(child) {
            // 0x4501e0：按名字递归找 SkinBackgroundButton
            if let Some(button) = sink.node_by_name_rec(grandchild, SKIN_BACKGROUND_BUTTON) {
                // 0x450124
                sink.set_button_icon(button, icon);
                painted += 1;
            }
        }
    }
    painted
}

#[cfg(test)]
mod vbox_tests {
    use super::*;

    /// 节点 = (名字, 子节点下标)；贴图 = u32。
    struct FakeTree {
        nodes: Vec<(&'static str, Vec<usize>)>,
        icon: u32,
        painted: Vec<(usize, u32)>,
    }

    impl ButtonIconSink for FakeTree {
        type Node = usize;
        type Texture = u32;

        fn children(&mut self, node: usize) -> Vec<usize> {
            self.nodes[node].1.clone()
        }

        fn node_by_name_rec(&mut self, node: usize, name: &str) -> Option<usize> {
            // 只在本节点自己身上找（够测本层的调用规约）
            if self.nodes[node].0 == name {
                Some(node)
            } else {
                None
            }
        }

        fn set_button_icon(&mut self, button: usize, icon: u32) {
            self.painted.push((button, icon));
        }
    }

    /// 只刷"孙节点里叫 SkinBackgroundButton"的那几个；父 → 行 → 按钮 三层。
    #[test]
    fn paints_only_the_named_grandchildren() {
        // 0 = parent, 1..=2 = 两行, 3..=4 = 行1 的两个按钮, 5 = 行2 的按钮
        let mut tree = FakeTree {
            nodes: vec![
                ("vbox", vec![1, 2]),
                ("row1", vec![3, 4]),
                ("row2", vec![5]),
                (SKIN_BACKGROUND_BUTTON, vec![]),
                ("SkinBlock", vec![]),
                ("SkinBackgroundButton2", vec![]),
            ],
            icon: 7,
            painted: vec![],
        };
        let n = update_buttons_background_in_vbox(&mut tree, 0, 7);
        assert_eq!(n, 1);
        assert_eq!(tree.painted, vec![(3, 7)]);
    }

    /// 名字、容量、类名与 `.rodata`/立即数读出来的一致。
    #[test]
    fn constants_are_the_binary_ones() {
        assert_eq!(SKIN_BACKGROUND_BUTTON, "SkinBackgroundButton");
        assert_eq!(SKIN_BACKGROUND_BUTTON.len(), 20); // 0x4501dc mov w3,#0x14
        assert_eq!(INSERT_ROW_CLASS, "HBoxContainer");
        assert_eq!(INSERT_ROW_CAPACITY, 3); // 0x44fb24 cmp x0,#3
        assert_eq!(INSERT_ADD_CHILD_SITES.len(), 3); // 3 条 bl add_child_full
        assert_eq!(INSERT_ADD_CHILD_FLAGS, (false, 0)); // mov w2,wzr / mov w3,wzr
        for w in INSERT_ADD_CHILD_SITES.windows(2) {
            assert!(w[0] < w[1], "调用点必须按地址升序");
        }
    }

    /// 调用方 `update_buttons_background` 的两个 vbox 名与长度（9 / 8）。
    #[test]
    fn caller_vbox_names_and_lengths() {
        let names = [("FacesVbox", 9usize, 0x19_de28u64), ("HatsVbox", 8, 0x17_90f8)];
        for (n, len, slot) in names {
            assert_eq!(n.len(), len, "{} 的长度必须等于 mov w3 的立即数", n);
            assert!(slot != 0);
        }
    }
}
