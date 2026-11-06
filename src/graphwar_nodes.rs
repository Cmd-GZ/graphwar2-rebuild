//! `graphwar_nodes.rs` —— **按名字 / 按类型在场景树里找节点**的工具模块（11 个可归属函数）。
//!
//! # 一句话结论
//!
//! 它**不构造**任何节点。原版把「界面控件怎么拿到手」这件事从各个界面控制器里抽出来，
//! 集中成这一个模块：调用方给一个**节点名字符串**（或一个类型），它从场景根开始**递归**
//! 找，找到后**上抛成 `Gd<T>`** 交给调用方。全二进制 40 个节点名、80 个直接调用点都走它，
//! 所以它是客户端所有界面控制器的公共入口（`docs/02` §16 曾把它列为"零锚点未知模块"，
//! 本轮已定名并解出结构）。
//!
//! # 三个入口（**观测**：2.0.60 arm64 `.symtab` 有完整符号名 + 具名被调者）
//!
//! | 入口 | arm64 单态化数 | 返回 | 具名被调者（arm64 `bl`，逐条解出） |
//! |---|---|---|---|
//! | `get_node_by_name::<T>` | **11** | `Gd<T>` | `Node::get_tree` → `SceneTree::get_root` → `Gd<T>::owned_cast` → `get_node_by_name_rec::<T>` |
//! | `get_node_by_name_rec::<T>` | 17 | `Gd<T>` | `Node::get_children_full`、`AnyArray::len`、`Node::get_name`、`StringName::fmt`（`Display`）、自递归、`Gd<T>::owned_cast` |
//! | `get_node_by_type_rec::<T>` | 1 | `Vec<Gd<T>>` | `get_children_full`、`AnyArray::len`、`owned_cast`、`RawVec::grow_one`、自递归 |
//!
//! ⇒ `get_node_by_name` 只是**薄壳**：先 `get_tree().get_root()`，再转给同名递归函数；
//! 真正遍历孩子的是 `_rec`。`get_node_by_type_rec` 多一条 `RawVec::grow_one`（往 `Vec` 里塞
//! 命中的节点）而**没有** `get_name` ⇒ 它按**类型**收集、返回**多个**。
//!
//! 名字本身不在这三个函数里：`name: &str` 由调用方放进 `%rcx`/`%r8`（x86）或 `x1`/`x2`（arm64），
//! 见 [`NODE_NAMES`] / [`NAMES_BY_ACCESSOR`]。
//!
//! # 11 个可归属函数是怎么定下来的（可复算）
//!
//! 1. `src/graphwar_nodes.rs` 在整个 `.so` 里**只有一个** `panic::Location`
//!    （[`PANIC_ANCHOR`] = `8:62`，结构体在 `0x951678`，`len=21` 即 `src/graphwar_nodes.rs`，
//!    `line=8`、`col=62`）。它被 **12** 个函数引用（`tools/loc_xrefs.py`，`.eh_frame` 定界）：
//!    `0x3a2e10` 与 `0x3aaea0 … 0x3ab890` 这 11 个。
//! 2. `0x3a2e10` 是 `GraphwarRootNode::_ready`（`0x26d0` 字节，同时引用多个模块的 Location，
//!    归 `coverage_matrix` 的 **contested** 规则记到别的模块）⇒ **扣掉**。
//! 3. 剩下 11 个大小 `0xf0…0x100`、首尾相接（`0x3aaea0, 0x3aafa0, 0x3ab0a0, 0x3ab190, …,
//!    0x3ab890`），形状完全一致 ⇒ 与 arm64 `.symtab` 里 **11 个 `get_node_by_name` 单态化**
//!    （每个 204/228 字节）一一对应。这就是 `docs/coverage-matrix.tsv` 里 `orig_funcs=11` 的来源。
//!
//! # 地址约定
//!
//! 本文件一律用 **ELF 虚地址（vaddr）**，即 `objdump` 打印的地址；
//! Ghidra / REA 地址 = 本文件地址 + `0x100000`（只加一次）。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条的四档）
//!
//! 本文件**没有**"逐位"级结论：这里没有可与真二进制逐字节对拍的纯函数，
//! 全部是 `观测`（能指到具体指令/符号/字面量）或 `推断`（写明缺什么）。
//! 文件末尾 [`unknowns`] 一节照实列出没解出来的部分。
//!
//! # 本节涉及的模块内测试
//!
//! 下面 `tests` 是**回归保护，不是复原证据**（`AGENTS.md` §0.2）。
//! 注意：本文件当前**未**被 `src/lib.rs` 注册（`pub mod` 由合并方统一加），
//! 集成测试用 `#[path]` 直接引本源文件，所以在注册前也能跑。

// ---------------------------------------------------------------------------
// 1. 锚点 / 反汇编坐标
// ---------------------------------------------------------------------------

/// 本模块的源文件名（与原版 `panic::Location` 里的 `file` 字段逐字节相同）。
pub const MODULE_PATH: &str = "src/graphwar_nodes.rs";

/// 本模块在整份 `.so` 里**唯一**的 `panic::Location`：`(file, line, col)`。
///
/// 档位：**观测**。`.data.rel.ro` 的结构体在 `0x951678`，解出
/// `{file_len: 21, line: 8, col: 62}`；`file` 指针是 `R_X86_64_RELATIVE`，
/// 其 addend 指向字面量 `src/graphwar_nodes.rs`。
///
/// `col = 62` 说明第 8 行是一条很长的表达式——`get_node_by_name` 的 `.unwrap()` 落在
/// 那一列（panic 种类由调用点证实：`0x4f7afa` 处 `lea` 的槽 `0x960810` 就是它，
/// 对应 `graphwar_player_screen_control::skin_button_pressed` 里"取不到节点"的那条 panic）。
pub const PANIC_ANCHOR: (&str, u32, u32) = ("src/graphwar_nodes.rs", 8, 62);

/// [`PANIC_ANCHOR`] 在 `.data.rel.ro` 里的地址（24 字节：`ptr8 + len8 + line4 + col4`）。
pub const PANIC_LOCATION_SLOT: u64 = 0x95_1678;

/// 引用了 [`PANIC_ANCHOR`] 的 **12** 个函数数（含被 contested 扣掉的 `0x3a2e10`）。
pub const ANCHOR_REFERRING_FUNCS: usize = 12;

/// 归属本模块的函数数（= `coverage_matrix` 的 `orig_funcs`）。**观测**。
pub const ATTRIBUTED_FUNCS: usize = 11;

/// 被 contested 规则扣掉的那一个：`GraphwarRootNode::_ready`（`0x26d0` 字节）。
///
/// 它同样引用 [`PANIC_ANCHOR`]（`0x3a4a43` 处一条 `R` 引用），但它体内还有别的模块的
/// Location 且锚点数更多 ⇒ 不计入本模块。**观测**。
pub const CONTESTED_FN: u64 = 0x3a_2e10;

// ---------------------------------------------------------------------------
// 2. 11 个 `get_node_by_name::<T>` 单态化（x86 侧）
// ---------------------------------------------------------------------------

/// 11 个 `get_node_by_name::<T>` 的 x86 虚地址，**按地址升序**。
///
/// 档位：**观测**（同一 `panic::Location` 的唯一引用点 + 首尾相接的函数边界）。
/// 每个 0xf0–0x100 字节，形状完全一致（见 [`Accessor`]）。
pub const GET_NODE_BY_NAME: [u64; 11] = [
    0x3a_aea0,
    0x3a_afa0,
    0x3a_b0a0,
    0x3a_b190,
    0x3a_b290,
    0x3a_b390,
    0x3a_b490,
    0x3a_b590,
    0x3a_b690,
    0x3a_b790,
    0x3a_b890,
];

/// 每个单态化**末尾**调用的"类型专属尾巴"的地址，与 [`GET_NODE_BY_NAME`] **同序**。
///
/// 档位：**观测**（`call rel32`，逐条）。这 11 个尾巴互不相同，正是 11 个不同的 `T`：
/// `T` 的 `owned_cast` / `check_rtti` 单态化被 LLVM 提取到了这些地址上。
/// ⚠️ **哪个尾巴对应哪个 `T` 是未知**（x86 侧没有类型名字符串；arm64 侧只有 hash）。
pub const TYPE_TAILS: [u64; 11] = [
    0x3a_d570,
    0x3a_b990,
    0x3a_d220,
    0x3a_cb00,
    0x3a_c770,
    0x3a_f0a0,
    0x3a_c3e0,
    0x3a_e650,
    0x3a_ce90,
    0x3a_c050,
    0x3a_dc50,
];

/// 一个单态化的两半：入口地址 + 类型专属尾巴地址。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accessor {
    /// `get_node_by_name::<T>` 的入口（vaddr）。
    pub fn_addr: u64,
    /// 该单态化末尾调用的类型专属尾巴（vaddr）。
    pub tail: u64,
}

/// [`GET_NODE_BY_NAME`] / [`TYPE_TAILS`] 的逐项配对。
pub const ACCESSORS: [Accessor; 11] = [
    Accessor { fn_addr: 0x3a_aea0, tail: 0x3a_d570 },
    Accessor { fn_addr: 0x3a_afa0, tail: 0x3a_b990 },
    Accessor { fn_addr: 0x3a_b0a0, tail: 0x3a_d220 },
    Accessor { fn_addr: 0x3a_b190, tail: 0x3a_cb00 },
    Accessor { fn_addr: 0x3a_b290, tail: 0x3a_c770 },
    Accessor { fn_addr: 0x3a_b390, tail: 0x3a_f0a0 },
    Accessor { fn_addr: 0x3a_b490, tail: 0x3a_c3e0 },
    Accessor { fn_addr: 0x3a_b590, tail: 0x3a_e650 },
    Accessor { fn_addr: 0x3a_b690, tail: 0x3a_ce90 },
    Accessor { fn_addr: 0x3a_b790, tail: 0x3a_c050 },
    Accessor { fn_addr: 0x3a_b890, tail: 0x3a_dc50 },
];

/// 按入口地址查 [`Accessor`]；不是这 11 个之一返回 `None`。
pub fn accessor(fn_addr: u64) -> Option<Accessor> {
    ACCESSORS.iter().copied().find(|a| a.fn_addr == fn_addr)
}

/// 按类型专属尾巴反查 [`Accessor`]；不是这 11 个之一返回 `None`。
pub fn accessor_by_tail(tail: u64) -> Option<Accessor> {
    ACCESSORS.iter().copied().find(|a| a.tail == tail)
}

// ---------------------------------------------------------------------------
// 3. 一次调用的骨架：gdext `Gd` 操作包装（跨模块观测，29 个模块共用）
// ---------------------------------------------------------------------------

/// `Gd<T>` 的一次"操作"——`upcast_ref` / `clone` / `cast` / `try_with_ref_counted`。
///
/// 档位：**观测（强）**。全 `.text` 里两个包装函数（[`GD_OP_WRAPPER_A`] 181 个调用点、
/// [`GD_OP_WRAPPER_B`] 295 个调用点）收到的 **名字立即数只有这四个**（逐条扫过）：
/// `clone` ×93、`upcast_ref` ×58、`cast` ×18（A）；`clone` ×121、`upcast_ref` ×117、
/// `cast` ×8、`try_with_ref_counted` ×1（B）。这四个名字正是 godot-rust(gdext) 里
/// `Gd<T>` 的四个内建操作 ⇒ 这两个包装是 gdext 的 **`Gd` 绑定调用胶水**，不是游戏业务代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GdOp {
    /// `Gd::clone`（引擎侧引用计数 +1）。字面量 `clone`（5 字节）。
    Clone,
    /// `Gd::upcast_ref::<Base>`（子类 → 基类）。字面量 `upcast_ref`（10 字节）。
    UpcastRef,
    /// `Gd::cast::<T>`（带 RTTI 检查的类型收窄）。字面量 `cast`（4 字节）。
    Cast,
    /// `Gd::try_with_ref_counted`。字面量 `try_with_ref_counted`（20 字节）。
    TryWithRefCounted,
}

impl GdOp {
    /// 传给包装函数的名字字面量（逐字节取自 `.rodata`）。
    pub fn as_str(self) -> &'static str {
        match self {
            GdOp::Clone => "clone",
            GdOp::UpcastRef => "upcast_ref",
            GdOp::Cast => "cast",
            GdOp::TryWithRefCounted => "try_with_ref_counted",
        }
    }

    /// 由名字字面量反查；不在这四个里返回 `None`。
    pub fn from_str(s: &str) -> Option<GdOp> {
        match s {
            "clone" => Some(GdOp::Clone),
            "upcast_ref" => Some(GdOp::UpcastRef),
            "cast" => Some(GdOp::Cast),
            "try_with_ref_counted" => Some(GdOp::TryWithRefCounted),
            _ => None,
        }
    }

    /// 四个操作名，按字面量长度升序（`cast` 4 → `clone` 5 → `upcast_ref` 10 →
    /// `try_with_ref_counted` 20）。
    pub const ALL: [GdOp; 4] =
        [GdOp::Cast, GdOp::Clone, GdOp::UpcastRef, GdOp::TryWithRefCounted];
}

/// `Gd` 操作包装 A：`0x4068f0`（181 个调用点）。
///
/// 签名（逐指令读出）：`fn(dst, name_ptr, name_len)` —— 先把 `(&'static str)` 变成 gdext
/// 的名字对象（槽 `0x9bb310`），再连同句柄送进 `0x9bb318`。**观测**。
/// ⚠️ 槽 `0x9bb310` / `0x9bb318` 的引擎侧成员名**未知**（需要运行时读 GOT）。
pub const GD_OP_WRAPPER_A: u64 = 0x40_68f0;

/// `Gd` 操作包装 B：`0x4066c0`（295 个调用点）。与 A 同形，多一条 `Result` 成败判别。
pub const GD_OP_WRAPPER_B: u64 = 0x40_66c0;

/// [11 个 `get_node_by_name`][`GET_NODE_BY_NAME`] 体内唯一的 `Gd` 操作：`upcast_ref`。
///
/// 档位：**观测**。11 个函数体里 `lea ...%r12` 的立即数都是 `0x1bd8f2`（`upcast_ref`），
/// `%edx = 10`；紧随其后的 panic 字面量是 `Upcast failed. This is a bug; please report it.`
/// （`0x1bd8fc`，`%esi = 0x2f = 47` 即其长度）。
///
/// ⚠️ 名字是"操作名"这一点有一个**反例风险**：`0x1bd8f2` 与 `0x1bd8fc` 在 `.rodata` 里相邻，
/// 但这只说明链接器把它们排在一起，不说明二者语义相关。真正的依据是**全表只有四个操作名**
/// 这一穷举结果（见 [`GdOp`]），而不是相邻性。
pub const NODE_ACCESSOR_GD_OP: GdOp = GdOp::UpcastRef;

// ---------------------------------------------------------------------------
// 4. 节点头名字表（观测：`.rodata` 字面量 + 长度立即数）
// ---------------------------------------------------------------------------

/// `get_node_by_name` 实参里出现过的**全部**节点名与长度（字节）。
///
/// 档位：**观测**。判据是每个调用点前的 `lea rcx,[rip+X]` + 紧随的 `mov r8d,<len>`；
/// 40 个名字、80 个调用点（11 个单态化各自的直接调用点合计）。名字长度是本文件里唯一"可逐字节复核"的一列，
/// 因此单独成表。
///
/// 交叉验证：其中 `RoomNameLineEdit` / `AxisModeButton` / `FunctionModeButton` /
/// `TurnModeButton` / `TurnTimeButton` / `LockButton` / `ChatButtonSetup` /
/// `ChatButtonPlay` / `ChatPopUp` / `ChatBox` / `ChatLineEdit` 这 11 条，
/// 与 `docs/16-client.md` §16.2.1 早已逐条量过的 `.rodata` 地址**完全一致**（独立两路）。
///
/// ⚠️ `"Team1Container"` 的长度来自相邻 `mov r8d,0xe`，是**同一条指令序列**里的立即数，
/// 仍属观测，但它与 `lea` 的配对窗口比其余 39 条宽 1 条指令。
pub const NODE_NAMES: [(&str, usize); 40] = [
    ("AxisModeButton", 14),
    ("ChatBox", 7),
    ("ChatButtonPlay", 14),
    ("ChatButtonSetup", 15),
    ("ChatLineEdit", 12),
    ("ChatPopUp", 9),
    ("DisconnectedLabel", 17),
    ("FacesVbox", 9),
    ("FireContainer", 13),
    ("FunctionModeButton", 18),
    ("FxTextureButton", 15),
    ("GameIpLabel", 11),
    ("GraphwarLanVBox", 15),
    ("GraphwarRootNode", 16),
    ("HatsVbox", 8),
    ("JoinIpIpLineEdit", 16),
    ("LeftButtonGrid", 14),
    ("LeftReplaysButtonsGrid", 22),
    ("LockButton", 10),
    ("MuteButton", 10),
    ("OffsetLabel", 11),
    ("PendingBlock", 12),
    ("PlayPauseButton", 15),
    ("PlayerListContainer", 19),
    ("PlayerNameLineEdit", 18),
    ("RightButtonGrid", 15),
    ("RightReplaysButtonsGrid", 23),
    ("RoomNameLineEdit", 16),
    ("ShowGlobalButton", 16),
    ("ShowLocalButton", 15),
    ("ShowNames", 9),
    ("SkinBlock", 9),
    ("SkinsVbox", 9),
    ("Team1Container", 14),
    ("TimerLabel", 10),
    ("TurnModeButton", 14),
    ("TurnTimeButton", 14),
    ("ViewFunctionButton", 18),
    ("ViewFunctionsLabel", 18),
    ("YButton", 7),
];

/// 某个单态化**被哪些函数调用**（调用方地址，去重后升序）。
///
/// 档位：**观测**（`call rel32` 反查）。同一函数里调多次只记一次；
/// 空切片 = 没有直接调用点（只剩函数指针派发的可能，**不是**"没被使用"）。
pub const CALLERS: [(u64, &[u64]); 11] = [
    (0x3a_aea0, &[0x3e_f990, 0x4c_f670]),
    (0x3a_afa0, &[0x4d_3180, 0x4f_57d0, 0x4f_8050]),
    (0x3a_b0a0, &[0x3b_0830]),
    (
        0x3a_b190,
        &[0x3d_9b50, 0x3d_9e40, 0x4c_f670, 0x4f_8850, 0x4f_8990, 0x4f_8ad0],
    ),
    (0x3a_b290, &[0x3a_5760, 0x4d_3180, 0x4f_2200, 0x52_2070]),
    (0x3a_b390, &[0x41_8820, 0x4f_2200, 0x52_2070, 0x52_38c0]),
    (0x3a_b490, &[0x3b_0830, 0x4c_f670, 0x4e_6480]),
    (
        0x3a_b590,
        &[
            0x4b_2890, 0x4b_4130, 0x4b_4b60, 0x4b_5df0, 0x4b_7a70, 0x4b_83f0, 0x4c_27f0,
            0x4c_44b0, 0x4c_4850, 0x4c_4c20, 0x4c_f670, 0x4d_3180, 0x4e_1520, 0x4e_25b0,
            0x4e_2f80, 0x4e_5400, 0x4e_6480, 0x4e_68f0, 0x4e_fd70, 0x4f_2200, 0x4f_7940,
            0x4f_90b0, 0x4f_d4a0,
        ],
    ),
    (0x3a_b690, &[0x3e_f990]),
    (0x3a_b790, &[0x3d_c7f0, 0x4c_f670]),
    (0x3a_b890, &[0x4d_3180]),
];

/// 已**逐条配对**的"单态化 → 节点名"。顺序 = 二进制里的调用顺序。
///
/// 档位：**观测**（每个名字都能指回一个 `lea rcx` + `mov r8d,<len>` + 紧接的 `call`）。
///
/// ⚠️ 有 5 个调用点的 `lea`/`mov r8d` 配对窗口被相邻的其它 `lea` 挤掉，
/// 那 5 个实参的**名字已读出、所属单态化未定**（`docs/16-client.md` §28.4）。
pub const NAMES_BY_ACCESSOR: [(u64, &[&str]); 11] = [
    (
        0x3a_aea0,
        &[
            "ShowLocalButton",
            "ShowGlobalButton",
            "AxisModeButton",
            "FunctionModeButton",
            "TurnModeButton",
            "TurnTimeButton",
            "LockButton",
            "ChatButtonSetup",
            "ChatButtonPlay",
        ],
    ),
    (
        0x3a_afa0,
        &["Team1Container", "SkinsVbox", "FacesVbox", "HatsVbox"],
    ),
    (0x3a_b0a0, &[]),
    (
        0x3a_b190,
        &[
            "LeftButtonGrid",
            "RightButtonGrid",
            "FireContainer",
            "LeftReplaysButtonsGrid",
            "RightReplaysButtonsGrid",
            "SkinBlock",
            "PendingBlock",
        ],
    ),
    (0x3a_b290, &["TimerLabel"]),
    (
        0x3a_b390,
        &[
            "ViewFunctionButton",
            "PlayPauseButton",
            "YButton",
            "ShowNames",
            "MuteButton",
        ],
    ),
    (
        0x3a_b490,
        &[
            "JoinIpIpLineEdit",
            "RoomNameLineEdit",
            "ChatLineEdit",
            "PlayerNameLineEdit",
        ],
    ),
    (0x3a_b590, &["GraphwarRootNode"]),
    (0x3a_b690, &["GraphwarLanVBox"]),
    (0x3a_b790, &["ViewFunctionsLabel"]),
    (0x3a_b890, &["PlayerListContainer"]),
];

/// 查某个节点名的长度（字节）；不在 [`NODE_NAMES`] 里返回 `None`。
pub fn name_len(name: &str) -> Option<usize> {
    NODE_NAMES.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// 某个单态化取过的节点名（顺序 = 调用顺序）。
pub fn names_of(fn_addr: u64) -> Vec<&'static str> {
    NAMES_BY_ACCESSOR
        .iter()
        .find(|(f, _)| *f == fn_addr)
        .map(|(_, ns)| ns.to_vec())
        .unwrap_or_default()
}

/// 有直接调用点的单态化数量（11 个全都有直接调用点）。
pub fn accessors_with_callers() -> usize {
    CALLERS.iter().filter(|(_, cs)| !cs.is_empty()).count()
}

// ---------------------------------------------------------------------------
// 5. arm64 2.0.60 符号表（名字的**唯一**来源）
// ---------------------------------------------------------------------------

/// 名字来源：Android 2.0.60 arm64 `libgraphwarrust.so` 的 `.symtab`（未 strip）。
///
/// ⚠️ 这是**同一版本**的另一架构构建，不是 2024-07-30 那版。两架构之间靠
/// `panic::Location` 的 `(file, line, col)` 做桥（`fixtures/name_to_addr.tsv`）。
/// 本模块的 11 个 x86 函数**没有**一个是一对一锚 ⇒ 表里标 `歧义`；
/// 11 与 11 的对应靠"同一 Location + 同形 + 同数量"三条合起来（**推断**，见 [`unknowns`]）。
pub const NAME_SOURCE: &str = "android-2.0.60-arm64 .symtab";

/// `GraphwarNodes` 在 arm64 `.symtab` 里的符号总数（`get_node_by_name` 11 +
/// `get_node_by_name_rec` 17 + `get_node_by_type_rec` 1 = 29）。**观测**。
pub const ARM64_SYMBOLS: usize = 29;

/// arm64 侧 11 个 `get_node_by_name::<T>` 的 `(vaddr, size)`，按地址升序。
pub const ARM64_GET_NODE_BY_NAME: [(u64, u32); 11] = [
    (0x003a_1c84, 228),
    (0x003a_1d68, 228),
    (0x003a_1e4c, 228),
    (0x003a_1f30, 228),
    (0x003a_2014, 228),
    (0x003a_20f8, 228),
    (0x003a_21dc, 228),
    (0x003a_22c0, 204), // 唯一一个 204 字节的：少一次字符串构造
    (0x003a_238c, 228),
    (0x003a_2470, 228),
    (0x003a_2554, 228),
];

/// arm64 侧 17 个 `get_node_by_name_rec::<T>` 的 vaddr（都 772–816 字节）。
///
/// 11 个与上面的 11 个单态化配对（`get_node_by_name` 的体内 `bl` 直接点名），
/// 另 6 个由别的模块直接调用。**观测**。
pub const ARM64_GET_NODE_BY_NAME_REC: [u64; 17] = [
    0x003a_2638,
    0x003a_2958,
    0x003a_2c6c,
    0x003a_2f9c,
    0x003a_32c4,
    0x003a_35e0,
    0x003a_38fc,
    0x003a_3c00,
    0x003a_3f14,
    0x003a_4234,
    0x003a_4554,
    0x003a_4868,
    0x003a_4b7c,
    0x003a_4e90,
    0x003a_51a8,
    0x003a_54bc,
    0x003a_57d0,
];

/// arm64 侧唯一的 `get_node_by_type_rec::<T>`（`0x3a5ae4`，636 字节）。**观测**。
pub const ARM64_GET_NODE_BY_TYPE_REC: (u64, u32) = (0x003a_5ae4, 636);

// ---------------------------------------------------------------------------
// 6. 诚实清单
// ---------------------------------------------------------------------------

/// 本节**没解出来**的东西（照实写，不许乐观）。
///
/// 1. **11 个 `T` 分别是什么类型**——未知。x86 侧只能看到 11 个不同的类型专属尾巴
///    （[`TYPE_TAILS`]），没有类型名字符串；arm64 侧符号只有 `::h<hash>`。
///    缺的证据：x86 尾巴体内 `check_rtti` 的 `ClassId` 立即数（需要先把
///    `godot_core::meta::class_id` 的表解出来，目前没有任何一节做过）。
///    唯一的旁证是**用法**：`0x3aafa0` 的 4 个实参全是 `*Vbox` + `Team1Container`（容器类），
///    `0x3a_b190` 的实参全是 `*Grid` / `*Block` / `FireContainer`（控件类），
///    `0x3a_b590` 的 18 处里 15 处是 `GraphwarRootNode`（游戏类）。这是**推断**，不是结论。
/// 2. **`get_node_by_name` 与 `get_node_by_name_rec` 谁是"公开 API"**——arm64 侧两个名字都在，
///    且 `get_node_by_name` 体内 `bl` 点名了 `get_node_by_name_rec` ⇒ **薄壳关系是观测**；
///    但"这 11 个 x86 函数对应哪一半"目前靠"11 == 11"这条计数论证，属**推断**。
///    缺的证据：x86 侧没有能与 arm64 一一对齐的指纹（`Location` 键在本模块只有一个）。
/// 3. **5 个实参的名字已读出、所属单态化未配对**（`lea rcx` 与 `mov r8d` 之间隔了别的 `lea`）。
/// 4. **`StringName` 比较的确切语义**——`get_node_by_name_rec` 里 `Node::get_name` 之后走
///    `StringName::fmt`（`Display`），再比较；是 `==` 还是 `format!` 之后比较**未知**。
///    缺的证据：`0x44ccc0` / `0x844d00` 这两个被调者没有符号。
/// 5. **`0x9bb310` / `0x9bb318`（[`GD_OP_WRAPPER_A`] 内部两个槽）的引擎侧成员名**——
///    需要运行时读 GOT（`frida` 本轮不可用）。
/// 6. **`0x3a_b0a0` 的 3 个实参**——全部落在 `GraphwarRootNode` 的命令总线 `0x3b0830` 里，
///    配对窗口被挤掉，名字未读出。
pub fn unknowns() -> &'static [&'static str] {
    &[
        "11 个单态化的 T 是什么类型（缺 class_id 表）",
        "11 个 x86 函数对应 get_node_by_name 还是 _rec（只有计数论证）",
        "5 个实参所属单态化未配对",
        "节点名比较是 == 还是 format! 后比较（0x44ccc0 / 0x844d00 无符号）",
        "0x9bb310 / 0x9bb318 的引擎侧成员名（需运行时读 GOT）",
        "0x3ab0a0 的 3 个实参名字未读出",
    ]
}

/// 11 个单态化在**整份 `.so`** 里的直接调用点数（逐条 `call rel32` 计数）。
pub const DIRECT_CALL_SITES: usize = 80;

#[cfg(test)]
mod tests {
    use super::*;

    /// 11 个单态化 = `coverage_matrix` 的 `orig_funcs`，且地址严格升序、互不相同。
    #[test]
    fn eleven_accessors_are_sorted_and_distinct() {
        assert_eq!(GET_NODE_BY_NAME.len(), ATTRIBUTED_FUNCS);
        for w in GET_NODE_BY_NAME.windows(2) {
            assert!(w[0] < w[1], "必须是升序：{:#x} !< {:#x}", w[0], w[1]);
        }
        // 首尾相接：相邻两个入口相差不超过 0x100，说明中间没有别的函数。
        for w in GET_NODE_BY_NAME.windows(2) {
            assert!(w[1] - w[0] <= 0x100, "首尾相接被破坏");
        }
    }

    /// 11 个类型专属尾巴互不相同（= 11 个不同的 `T`）。
    #[test]
    fn type_tails_are_distinct() {
        let mut v = TYPE_TAILS.to_vec();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 11);
        assert_eq!(ACCESSORS.len(), 11);
        for (i, a) in ACCESSORS.iter().enumerate() {
            assert_eq!(a.fn_addr, GET_NODE_BY_NAME[i]);
            assert_eq!(a.tail, TYPE_TAILS[i]);
            assert!(accessor(a.fn_addr).is_some());
            assert_eq!(accessor_by_tail(a.tail), Some(*a));
        }
        assert_eq!(accessor(0x3a_2e10), None, "root_node 的 _ready 不属于这 11 个");
    }

    /// 锚点三元组与二进制里的 Location 结构体一致。
    #[test]
    fn panic_anchor_matches_binary() {
        assert_eq!(PANIC_ANCHOR, ("src/graphwar_nodes.rs", 8, 62));
        assert_eq!(MODULE_PATH, PANIC_ANCHOR.0);
        assert_eq!(
            PANIC_ANCHOR.0.len(),
            "src/graphwar_nodes.rs".len(),
            "Location 的 len 字段是 21"
        );
        assert_eq!(ANCHOR_REFERRING_FUNCS - 1, ATTRIBUTED_FUNCS);
    }

    /// 四个 gdext `Gd` 操作名逐字节正确，且往返一致。
    #[test]
    fn gd_ops_roundtrip() {
        assert_eq!(GdOp::Cast.as_str(), "cast");
        assert_eq!(GdOp::Clone.as_str(), "clone");
        assert_eq!(GdOp::UpcastRef.as_str(), "upcast_ref");
        assert_eq!(GdOp::TryWithRefCounted.as_str(), "try_with_ref_counted");
        assert_eq!(GdOp::Cast.as_str().len(), 4);
        assert_eq!(GdOp::Clone.as_str().len(), 5);
        assert_eq!(GdOp::UpcastRef.as_str().len(), 10);
        assert_eq!(GdOp::TryWithRefCounted.as_str().len(), 20);
        for op in GdOp::ALL {
            assert_eq!(GdOp::from_str(op.as_str()), Some(op));
        }
        assert_eq!(GdOp::from_str("upcast"), None);
        assert_eq!(NODE_ACCESSOR_GD_OP, GdOp::UpcastRef);
    }

    /// 节点名表没有重复，且每个长度都等于名字自身的字节数。
    #[test]
    fn node_names_lengths_are_self_consistent() {
        let mut seen = std::collections::BTreeSet::new();
        for (n, l) in NODE_NAMES {
            assert!(seen.insert(n), "重复的名字：{n}");
            assert_eq!(n.len(), l, "{n} 的长度对不上");
        }
        assert_eq!(seen.len(), NODE_NAMES.len());
        assert_eq!(name_len("GraphwarRootNode"), Some(16));
        assert_eq!(name_len("SkinBlock"), Some(9));
        assert_eq!(name_len("PendingBlock"), Some(12));
        assert_eq!(name_len("NoSuchNode"), None);
    }

    /// `NAMES_BY_ACCESSOR` 里的每个名字都必须在 `NODE_NAMES` 里，且属于已登记的单态化。
    #[test]
    fn names_by_accessor_is_closed() {
        for (f, names) in NAMES_BY_ACCESSOR {
            assert!(accessor(f).is_some(), "{f:#x} 不是 11 个单态化之一");
            for n in names {
                assert!(name_len(n).is_some(), "{n} 没登记长度");
            }
            assert_eq!(names_of(f).as_slice(), names);
        }
        // GraphwarRootNode 是最热的那个节点（18 处调用里 15 处点名它）。
        assert_eq!(names_of(0x3a_b590), vec!["GraphwarRootNode"]);
        assert_eq!(names_of(0x3a_aea0).len(), 9);
    }

    /// arm64 侧 29 个符号 = 11 + 17 + 1，且 `_rec` 全部比薄壳大一个数量级。
    #[test]
    fn arm64_symbol_split_adds_up() {
        assert_eq!(
            ARM64_GET_NODE_BY_NAME.len() + ARM64_GET_NODE_BY_NAME_REC.len() + 1,
            ARM64_SYMBOLS
        );
        for (_, sz) in ARM64_GET_NODE_BY_NAME {
            assert!(sz == 204 || sz == 228);
        }
        assert_eq!(ARM64_GET_NODE_BY_TYPE_REC.1, 636);
        assert_eq!(ARM64_GET_NODE_BY_NAME.len(), ATTRIBUTED_FUNCS);
    }

    /// 调用方表里的每个地址都必须指向一个已知的 11 个之一之外的函数。
    #[test]
    fn callers_do_not_point_at_accessors() {
        for (f, callers) in CALLERS {
            assert!(accessor(f).is_some());
            for c in callers {
                assert!(
                    accessor(*c).is_none(),
                    "调用方 {c:#x} 居然也是单态化，环了"
                );
            }
        }
        assert_eq!(accessors_with_callers(), 11);
        assert_eq!(DIRECT_CALL_SITES, 80);
    }

    /// 诚实清单不许空着（`AGENTS.md` §0.3）。
    #[test]
    fn unknowns_are_listed() {
        assert_eq!(unknowns().len(), 6);
    }
}
