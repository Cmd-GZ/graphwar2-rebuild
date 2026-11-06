//! `button_manager.rs` —— **按钮总线的总装**（原版 `ButtonManager`）。
//!
//! # 它是什么
//!
//! `ButtonManager` 是**上一层**：它先递归把屏幕上的按钮收起来（`find_buttons`），
//! 然后**造出一串子管理器**并把按钮分给它们 —— 其中能点出名字的一个就是
//! `function_button_manager::FunctionButtonManager::new`（x86 `0x4f0270`）。
//!
//! 也就是说两个模块是**同构的两层**：
//!
//! ```text
//! ButtonManager::new            0x4a2e10  （x86，9936 B —— 内联了 find_buttons + 子管理器装配）
//!   ├─ find_buttons              arm64 0x3a1a6c（440 B）
//!   ├─ FunctionButtonManager::new  0x4f0270  ← 唯一能定名的子管理器
//!   └─ …另外 53 个被调者（多数是 gdext/std，游戏侧多数无 Location 锚 ⇒ 不定名）
//! ```
//!
//! 两者各有一份**自己的** `find_buttons` 单态化：**两侧都是 440 B、被调者逐个相同**
//! （`check_rtti` / `get_children_full` / `AnyArray::len` / `to_i64` / `FromGodot::from_variant`
//! / `owned_cast` / `RawVec::grow_one` / 自递归）—— 这是"同一个泛型函数被两个类型各单态化一次"
//! 的教科书形态，也是本模块与 `function_button_manager` **边界**的物证。
//!
//! # 函数表（arm64 2.0.60 未 strip）
//!
//! | 函数 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 判据 |
//! |---|---|---|---|---|---|
//! | `find_buttons` | `0x3a1a6c` | 440 | 未知 | — | arm64 符号；无 Location ⇒ 不可归属 |
//! | `new` | **无符号**（被内联进调用者） | — | **`0x3a2e10`** | 9936 | `panic::Location` 2 个键 + 唯一调用者 `0x52fdf0`；arm64 无独立符号 ⇒ 比值闸**不适用** |
//! | `new::{{closure}}` | `0x3a1c24` | 96 | 未知（2 候选） | — | 体内只有 `panic_already_borrowed` |
//!
//! ## ⚠️ 与"2 个函数"这条口径的差异（照实写）
//!
//! `docs/coverage-matrix.tsv` 给本模块记的是 **2 个可归属函数**。本轮用
//! `tools/loc_xrefs.py` 重扫，`src/button_manager.rs` 的 3 个 Location 落在 **3 个**函数里：
//!
//! | x86 函数 | 体长 | 引用的 Location |
//! |---|---|---|
//! | `0x3a2e10` | 9936 | `:24:12`（槽 `0x951648`）+ `:26:26`（槽 `0x951630`） |
//! | `0x3aae60` | 64 | `:29:26`（槽 `0x951660`） |
//! | `0x3b70b0` | 144 | `:29:26`（同一个槽） |
//!
//! 后两个引用**同一个** Location：arm64 侧对应的是两份东西 —— `new::{{closure}}`（96 B）
//! 与 `drop_in_place<new::{{closure}}>`（28 B）。哪一个是闭包本体、哪一个是 drop glue
//! **没有定**（64 B 与 144 B 都不像 28 B 的 drop glue，也都不像 96 B 的闭包体）。
//! ⇒ 记法上按"`new` + 闭包"= **2 个角色**，但**地址只敢钉 `0x3a2e10` 一个**。
//!
//! # `0x3a2e10` 为什么这么大（9936 B）
//!
//! 因为它**不是一个方法体**，而是"`new` + 内联进去的 `find_buttons` + 各子管理器的装配"：
//! `procedure_callees(0x4a2e10)` 一共 **54** 个目标，其中游戏侧可点名的只有
//! `FunctionButtonManager::new`（`0x4f0270`）；其余集中在 `0x4a0810`–`0x4ae320`
//! 这一簇（**推断**：兄弟管理器，**缺的证据**：那 9 个函数没有一个带 Location 锚）。
//!
//! # 唯一的入口
//!
//! ```text
//! procedure_callers(0x4a2e10) == [0x52fdf0]
//! procedure_callers(0x4f0270) == [0x4a2e10]
//! ```
//!
//! `0x52fdf0` 是个薄壳：先从 `GraphwarModuleContainer` 里取一个模块
//! （`0x5b0f20` = 按 `TypeId` 取的泛型 `get`），再调 `ButtonManager::new`。
//! ⇒ 谁在 `ready` 里点这把火，本轮**没有继续往上追**（**未知**）。
//!
//! # 地址约定
//!
//! `x86` 是 **ELF 虚地址**（Ghidra/REA = 本值 + `0x100000`）；`arm64` 只用于定名。

/// 一个源级函数在两侧的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Func {
    /// arm64 2.0.60 的源级真名（`None` = 该臂无独立符号，被内联了）。
    pub name: Option<&'static str>,
    /// arm64 2.0.60 地址（`None` = arm64 侧也没有独立符号）。
    pub arm64: Option<u32>,
    /// arm64 字节数。
    pub arm64_len: u32,
    /// x86-64 目标的 ELF 虚地址。
    pub x86: Option<u32>,
    /// x86 字节数。
    pub x86_len: u32,
    /// 判据。
    pub how: &'static str,
}

/// 本模块的 3 条落点。
pub const FUNCTIONS: [Func; 3] = [
    Func {
        name: Some("find_buttons"),
        arm64: Some(0x003a_1a6c),
        arm64_len: 440,
        x86: None,
        x86_len: 0,
        how: "arm64 符号；440 B 与 function_button_manager 的同名函数逐被调者相同（两个单态化）",
    },
    Func {
        name: None,
        arm64: None,
        arm64_len: 0,
        x86: Some(0x003a_2e10),
        x86_len: 9936,
        how: "panic::Location 24:12/26:26；唯一调用者 0x52fdf0；arm64 无符号（内联）",
    },
    Func {
        name: Some("new::{{closure}}"),
        arm64: Some(0x003a_1c24),
        arm64_len: 96,
        x86: None,
        x86_len: 0,
        how: "arm64 符号；x86 侧两候选 0x3aae60(64 B) / 0x3b70b0(144 B) 共用同一个 Location :29:26，未定哪个是本体",
    },
];

/// `src/button_manager.rs` 的全部 `panic::Location`：**3 个**。
///
/// 字段：`(line, col, Location 结构体地址, 引用它的函数)`。
pub const LOCATIONS: [(u32, u32, u32, u32); 3] = [
    (24, 12, 0x0095_1648, 0x003a_2e10),
    (26, 26, 0x0095_1630, 0x003a_2e10),
    (29, 26, 0x0095_1660, 0x003a_ae60),
];

/// `:29:26` 那个 Location 还出现在第二个函数里（同一个槽）。
pub const LOCATION_29_SECOND_FUNCTION: u32 = 0x003b_70b0;

/// `new` 的唯一调用者（薄壳：从 `GraphwarModuleContainer` 取模块再调它）。
///
/// ⚠️ **Ghidra 口径**（vaddr `0x42fdf0` + `0x100000`）；本文件 `x86` 字段都是 vaddr。
pub const NEW_CALLER: u32 = 0x0052_fdf0;

/// `new` 里**唯一**能定名的子管理器构造函数 = `FunctionButtonManager::new`。
///
/// ⚠️ **Ghidra 口径**（vaddr `0x3f0270` + `0x100000`）。
pub const FUNCTION_BUTTON_MANAGER_NEW: u32 = 0x004f_0270;

/// `new` 的被调者总数（Ghidra 解析出的 54 个；另有约 100 条调用无目标，**未解析**）。
pub const NEW_CALLEE_COUNT: usize = 54;

/// `find_buttons` 这一族在两侧的**单态化份数**：2（`ButtonManager` 与 `FunctionButtonManager` 各一份）。
pub const FIND_BUTTONS_MONOMORPHIZATIONS: usize = 2;

/// 两份 `find_buttons` 的对应地址（arm64）。
pub const FIND_BUTTONS_PAIRS: [(u32, &str); 2] =
    [(0x003a_1a6c, "button_manager"), (0x0042_4494, "function_button_manager")];

/// 按模块名取那份 `find_buttons` 的 arm64 地址（两处单态化）。
pub fn find_buttons_arm64(module: &str) -> Option<u32> {
    FIND_BUTTONS_PAIRS
        .iter()
        .find(|(_, m)| *m == module)
        .map(|(a, _)| *a)
}

/// 一棵极小的节点树 —— 只为了把 `find_buttons` 的行为写成可测的纯逻辑。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// 节点名。
    pub name: &'static str,
    /// 是不是按钮。
    pub is_button: bool,
    /// 子节点。
    pub children: Vec<Node>,
}

impl Node {
    /// 叶子。
    pub fn leaf(name: &'static str, is_button: bool) -> Self {
        Self { name, is_button, children: Vec::new() }
    }

    /// 带子节点。
    pub fn branch(name: &'static str, children: Vec<Node>) -> Self {
        Self { name, is_button: false, children }
    }
}

/// `find_buttons` 的纯逻辑等价物：**深度优先、先序**收集按钮名。
///
/// 与本模块的兄弟实现（`function_button_manager::find_buttons`）**必须是同一个算法** ——
/// 两侧的机器码逐被调者相同，只是类型参数不同。
pub fn find_buttons<'a>(node: &'a Node, out: &mut Vec<&'a str>) {
    if node.is_button {
        out.push(node.name);
    }
    for c in &node.children {
        find_buttons(c, out);
    }
}

/// 按名字查一条（`name` 为 `None` 的那条只能按 x86 地址查，见 [`by_x86`]）。
pub fn func(name: &str) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.name == Some(name))
}

/// 按 x86 虚地址查一条。
pub fn by_x86(addr: u32) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.x86 == Some(addr))
}

/// x86 与 arm64 的字节数比值（arm64 无符号时 `None` —— 比值闸不适用）。
pub fn size_ratio(f: &Func) -> Option<f64> {
    match (f.x86, f.arm64) {
        (Some(_), Some(_)) => Some(f.x86_len as f64 / f.arm64_len as f64),
        _ => None,
    }
}
