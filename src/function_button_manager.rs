//! `function_button_manager.rs` —— **函数输入键盘的按钮管理器**（原版 `FunctionButtonManager`）。
//!
//! # 它是什么（一句话）
//!
//! 把**函数输入框那一排按钮**接到 `GraphwarFunctionInput` 上：`find_buttons` 递归收按钮，
//! `new` 给每个按钮挂一个 **`Box<dyn FnMut(&str)>`** 回调，回调按"按钮给的那个字符串"
//! 分发到 8 个 `GraphwarFunctionInput` 方法之一（清空 / 插入 / 删 token / 移光标）。
//!
//! ⇒ 它不是"sin/sqrt 快捷插入"那么窄，而是**整个函数输入键盘的按键分发器**；
//! 而且它**一个函数名/按钮名都没硬编码** —— 见 §「按钮文本从哪来」。
//!
//! # 3 个函数（arm64 2.0.60 未 strip，真名 + 真边界）
//!
//! | 函数 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 判据 |
//! |---|---|---|---|---|---|
//! | `find_buttons` | `0x424494` | 440 | **未知** | — | 递归 + `Node::get_children_full` + `Gd::owned_cast`；x86 侧无 Location ⇒ 不可归属 |
//! | `new` | `0x42464c` | 924 | **`0x3f0270`** | 1088 | `panic::Location` 2 个键 + 被调者（`find_buttons`、`RawVec::grow_one`、`Box<dyn FnMut>` 的 drop glue）；比值 **1.18** |
//! | `new::{{closure}}` | `0x4249e8` | 2040 | **`0x3f06b0`** | 1936 | `panic::Location` 1 个键 + 被调者 8 个 `GraphwarFunctionInput` 方法逐个相同；比值 **0.95** |
//!
//! 两个比值都在同源闸 `0.61–1.19` 里。`0x3f0270` 的 1088 B 正好接到 `0x3f06b0`、
//! `0x3f06b0` 的 1936 B 正好接到下一个带 Location 的函数 `0x3f0e40`
//! （`function_history_recorder::handle_function_fire_event`）—— **首尾相接**，独立于名字。
//!
//! # 唯一的调用者：`ButtonManager::new`
//!
//! ```text
//! procedure_callers(0x4f0270) == [0x4a2e10]      ; = button_manager::ButtonManager::new
//! ```
//!
//! ⇒ 它是 `button_manager` 造出来的一组子管理器之一（见 `src/button_manager.rs`）。
//!
//! # `new::{{closure}}` 干什么（**观测**，被调者多重集）
//!
//! 整个闭包**只**调用这 8 个 `GraphwarFunctionInput` 方法（外加 3 次
//! `assert_failed::<usize,usize>`、`panic_already_borrowed`、`Result::unwrap_failed`）：
//!
//! | 目标方法 | arm64 2.0.60 | 字节 | 干什么 |
//! |---|---|---|---|
//! | `clear` | `0x44daec` | 140 | 清空输入框 |
//! | `insert_text_at_caret` | `0x44c9d8` | 236 | 在光标处插入一段文本 |
//! | `delete_token_at_caret` | `0x44ccc0` | 1368 | 删光标前的 token |
//! | `delete_token_after_caret` | `0x44d218` | 2260 | 删光标后的 token |
//! | `move_caret_left` | `0x44b9c0` | 1684 | 光标左移 |
//! | `move_caret_right` | `0x44c054` | 2192 | 光标右移 |
//! | `move_caret_home` | `0x44b950` | 112 | 光标到最前 |
//! | `move_caret_end` | `0x44aa6c` | 464 | 光标到最后 |
//!
//! `assert_failed::<usize, usize>` ×3 说明闭包里还有 3 处 `assert_eq!` 长度的代码
//! （**推断**：把 token 序列的下标和文本偏移对齐；**缺的证据**：没有逐条读那 3 处的比较对象）。
//!
//! # 按钮文本从哪来（**这是任务问的点**）
//!
//! **不来自本模块的字符串字面量。** 三层证据：
//!
//! 1. 闭包的入参类型是 `&str` —— `new` 里 drop glue 的符号是
//!    `drop_in_place<Box<dyn FnMut<(&str,)> + Output = ()>>`（arm64 `0x424908`），
//!    即"按钮把**自己的字符串**交给回调"，而不是"回调自带一张按钮表"；
//! 2. x86 `0x3f06b0` 引用的**字符串只有 11 条**，逐个列出后**没有一条**是函数名
//!    （没有 `sin` / `sqrt` / `log` / `pi`）—— 只有 `called \`Result::unwrap()\`…` 与
//!    类名字面量 `graphwarrust::graphwar_function_input::GraphwarFunctionInput`（`0x2c1c87`，cast 用）；
//! 3. ⇒ 按钮的**文本/名字来自场景**（`Graphwar.pck` 里的 `.tscn`/`Control` 节点），
//!    Rust 侧只按文本分发。**档位：观测**（引用字符串穷举）。
//!
//! > ⚠️ **反例记一笔（别踩这个坑）**：Ghidra 把 x86 `0x3f0792` / `0x3b1622` 引用的两段字节
//! > 认成字符串 `"e_view_functions"`（`0x19cd70`）与 `"toggle_view_func"`（`0x19ceef`）。
//! > 逐指令看，那两条指令都是 **`pcmpeqb xmm1, [rip-…]`** —— 用的是 **16 字节 SIMD 常量**，
//! > 只是恰好落在字符串池里。`0x19ceef` 处的字节是 `b"toggle_view_funcy"`（17 字节，
//! > 第 18 字节是 `\0`），根本不是信号名/按钮名。
//! > ⇒ **"谁引用了某个字符串"不能当"代码里写着这个名字"用**；这正是 `strings` 与
//! > `xref` 组合会骗人的那一类。
//!
//! # 地址约定
//!
//! `x86` 是 **ELF 虚地址**（Ghidra/REA = 本值 + `0x100000`）；`arm64` 只用于定名。

/// 一个源级函数在两侧的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Func {
    /// arm64 2.0.60 的源级真名。
    pub name: &'static str,
    /// arm64 2.0.60 地址。
    pub arm64: u32,
    /// arm64 字节数。
    pub arm64_len: u32,
    /// x86-64 目标的 ELF 虚地址。
    pub x86: Option<u32>,
    /// x86 字节数。
    pub x86_len: u32,
    /// 判据。
    pub how: &'static str,
}

/// 本模块 **3 个**源级函数（2.0.60 arm64 权威名单）。
pub const FUNCTIONS: [Func; 3] = [
    Func {
        name: "find_buttons",
        arm64: 0x0042_4494,
        arm64_len: 440,
        x86: None,
        x86_len: 0,
        how: "arm64 符号；递归 + get_children_full + owned_cast；x86 侧无 Location，未定位",
    },
    Func {
        name: "new",
        arm64: 0x0042_464c,
        arm64_len: 924,
        x86: Some(0x003f_0270),
        x86_len: 1088,
        how: "2 个 panic::Location 键 + 被调者 find_buttons / RawVec::grow_one / Box<dyn FnMut> drop glue；比值 1.18",
    },
    Func {
        name: "new::{{closure}}",
        arm64: 0x0042_49e8,
        arm64_len: 2040,
        x86: Some(0x003f_06b0),
        x86_len: 1936,
        how: "1 个 panic::Location 键 + 8 个 GraphwarFunctionInput 方法逐个相同；比值 0.95",
    },
];

/// `src/function_button_manager.rs` 的全部 `panic::Location`：**3 个**。
///
/// 字段：`(line, col, Location 结构体地址, 引用它的函数)`。
pub const LOCATIONS: [(u32, u32, u32, u32); 3] = [
    (28, 12, 0x0095_4fd8, 0x003f_0270),
    (30, 26, 0x0095_4fc0, 0x003f_0270),
    (33, 26, 0x0095_4ff0, 0x003f_06b0),
];

/// 闭包能分发到的 `GraphwarFunctionInput` 方法：`(方法名, arm64 地址, arm64 字节)`。
///
/// 这 8 条就是 `new::{{closure}}` 的**全部**游戏内被调者（穷举）。
pub const DISPATCH: [(&str, u32, u32); 8] = [
    ("clear", 0x0044_daec, 140),
    ("insert_text_at_caret", 0x0044_c9d8, 236),
    ("delete_token_at_caret", 0x0044_ccc0, 1368),
    ("delete_token_after_caret", 0x0044_d218, 2260),
    ("move_caret_left", 0x0044_b9c0, 1684),
    ("move_caret_right", 0x0044_c054, 2192),
    ("move_caret_home", 0x0044_b950, 112),
    ("move_caret_end", 0x0044_aa6c, 464),
];

/// 闭包里的 `assert_failed::<usize, usize>` 次数：**3**。
///
/// 真值（arm64）：`0x424fa4` / `0x425044` / `0x4250b4`。`new` 里另有 **1** 处（`0x42487c`），
/// 那个不属于闭包。
pub const CLOSURE_ASSERTS: usize = 3;

/// **唯一**调用 `new` 的函数 = `button_manager::ButtonManager::new`。
///
/// ⚠️ 这是 **Ghidra 口径**（vaddr `0x3a2e10` + `0x100000`）—— 直接抄自 REA 的
/// `procedure_callers`；本文件其余 `x86` 字段都是 vaddr。
pub const ONLY_CALLER: u32 = 0x004a_2e10;

/// `find_buttons` 用的 Godot API（`Node::get_children_full` 的 GOT 槽在 x86 侧未解，
/// 这里只记 arm64 侧被调者名）。
pub const FIND_BUTTONS_API: [&str; 4] = [
    "Node::get_children_full",
    "AnyArray::len",
    "FromGodot::from_variant",
    "Gd::owned_cast",
];

/// 一棵极小的节点树 —— 只为了把 `find_buttons` 的**递归收集**行为写成可测的纯逻辑。
///
/// 原版的收集条件是 `Gd<Node> → owned_cast::<按钮类型>()` 成功（arm64 `0x424578`），
/// 失败就**跳过但继续往它的子树走**（递归调用自己，`0x4245cc`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// 节点名（场景里的名字 —— 原版不会读它，这里只是给测试看得见）。
    pub name: &'static str,
    /// 它是不是一个"按钮"（对应 `owned_cast` 成功）。
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

/// `find_buttons` 的纯逻辑等价物：**深度优先、先序**地把所有按钮名收进 `out`。
///
/// 与原版一致的一点：**非按钮节点不终止递归**（arm64 的 `0x4245cc` 是自调用，
/// 位置在 cast 判定之外）。
pub fn find_buttons<'a>(node: &'a Node, out: &mut Vec<&'a str>) {
    if node.is_button {
        out.push(node.name);
    }
    for c in &node.children {
        find_buttons(c, out);
    }
}

/// 按名字查一条 [`FUNCTIONS`]。
pub fn func(name: &str) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// 按方法名查 [`DISPATCH`]。
pub fn dispatch_target(method: &str) -> Option<(&'static str, u32, u32)> {
    DISPATCH.iter().copied().find(|(n, _, _)| *n == method)
}

/// x86 与 arm64 的字节数比值。
pub fn size_ratio(f: &Func) -> Option<f64> {
    f.x86.map(|_| f.x86_len as f64 / f.arm64_len as f64)
}
