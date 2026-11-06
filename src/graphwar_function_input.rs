//! `graphwar_function_input.rs` —— **函数输入框**（原版 `GraphwarFunctionInput`）。
//!
//! # 这个模块是什么
//!
//! 原版的函数输入框是 `godot::LineEdit` 的 **Rust 子类**（gdext：`ILineEdit` +
//! `GodotClass` + `ImplementsGodotApi` + `ImplementsGodotVirtual`），**不是** `.tscn`
//! 里的 LineEdit 节点：它下面没有 `LineEdit` / `TextEdit` 子节点
//! （`docs/06-verification.md` §15.3 的运行时观测），文字状态在 Rust 侧。
//! 场景只负责把输入框自己的三条信号接回它自己的方法，见 [`SIGNAL_WIRING`]。
//!
//! # 地址约定
//!
//! 本文件所有地址是 **ELF 虚地址（vaddr，`objdump` 口径）**；
//! Ghidra / REA 地址 = 本文件地址 + `0x100000`（只加一次，见 `AGENTS.md`）。
//! `.rodata` 的 `Address == Offset`，所以字面量地址可以直接与 `strings -a -t x` 对齐。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条）
//!
//! * **观测** —— 能指到具体指令地址 / 具体 `.rodata` 字节；
//! * **推断** —— 有依据但没证，写明缺什么；
//! * **未知** —— 没解出来。
//!
//! `cargo test` 全绿只是回归保护，**不构成复原证据**（`AGENTS.md` §0.2）。
//!
//! # 名字从哪来
//!
//! `fixtures/name_to_addr.tsv` 里本模块有 **15 个原版方法名 / 23 个锚**，其中
//! **7 个是"唯一锚"**（两侧 `panic::Location` 一对一）。这 7 个本轮**逐条复核通过**：
//! 锚点行号集合与地址一一对上，且锚的**数量**与表里记录的条数一致——
//!
//! | 方法名（原版） | vaddr | 锚行号集合 | 锚数 | 复核 |
//! |---|---|---|---|---|
//! | `force_update_cb` | `0x4f2200` | `327:133, 332:62, 337:47, 341:49, 343:68` | 5 | ✅ 表里写「5 个双向唯一锚」 |
//! | `move_caret_left` | `0x4f3210` | `253:63` | 1 | ✅ |
//! | `move_caret_right` | `0x4f3910` | `145:63, 151:86, 160:86, 171:86, 182:86, 189:74` | 6 | ✅ 表里写「6 个」 |
//! | `delete_token_at_caret` | `0x4f44c0` | `97:63` | 1 | ✅ |
//! | `delete_token_after_caret` | `0x4f4a60` | `198:63, 204:86, 214:86, 226:86, 238:86, 246:78` | 6 | ✅ 表里写「6 个」 |
//! | `ready` | `0x4fd1b0` | `508:43` | 1 | ✅ |
//! | `process` | `0x4fd4a0` | `521:133, 523:58, 528:45, 530:51, 535:51, 550:45, 550:96, 558:53, 602:55, 607:68` | 10 | ✅ 表里写「10 个」 |
//!
//! ⇒ `fixtures/name_to_addr.tsv` 与本模块的反汇编**没有冲突**。
//! 复核口径与一处**工具**冲突见 [`VERIFICATION_NOTE`]。
//!
//! # 全模块 17 个 x86 函数
//!
//! `docs/02-module-map.md` §18 A 表按 **2024-07-30 版**给出 15 个函数；本文件针对
//! **2.0.60**。用「引用了本文件 `panic::Location`」这条硬证据定出 **13 个**，
//! 另外 **4 个**没有任何 Location 锚（没有 panic 路径），靠**调用关系 + 虚拟方法注册链**
//! 定位，档位 **推断（强）**。合计 17 条，见 [`FUNCTIONS`]。
//!
//! # 本轮的两个核心结论
//!
//! ## 1. 输入框到底有没有"预处理"？—— **只有一步：删空格**
//!
//! `force_update_cb`（`0x4f2200`）在 `0x4f28be`–`0x4f28dc` 调 `str::replace`
//! （`0x502d20`；全二进制只有 3 个调用者，另两个在 `MathFunction::new`）：
//!
//! ```text
//! 0x4f28b6  movq $0x0,(%rsp)          ; to.len   = 0   （第 7 个参数在栈上）
//! 0x4f28be  lea  -0x32e424(%rip),%rcx ; from.ptr = 0x1c44a1
//! 0x4f28c5  lea  0x108(%rsp),%rdi     ; out
//! 0x4f28cd  mov  $0x1,%r8d            ; from.len = 1
//! 0x4f28d3  mov  $0x1,%r9d            ; to.ptr   = 1     （空 &str 的非空对齐指针）
//! 0x4f28d9  mov  %rbx,%rsi            ; self.ptr
//! 0x4f28dc  call 502d20               ; str::replace
//! ```
//!
//! `0x1c44a1` 处的那个字节是 **`' '`（0x20）**——它夹在两个相邻字面量
//! `"FxTextureButton"` 与 `"delete_char_at_caret"` 之间，是**独立的 1 字节字面量**。
//! ⇒ 原版做的是 **`text.replace(' ', "")`**，见 [`strip_spaces`]。
//!
//! 除此之外输入框**不**转 π、**不**补 `*`、**不**动其它空白、**不**小写化：
//! 全模块引用的 9 条字面量里没有 `"π"`、没有 `"*"`、没有第二个空白串。
//!
//! ## 2. 真正吃 π / 隐式乘 / `log`=log10 的是**解析器**
//!
//! `MathFunction::new`（`0x4ed670`，见 `docs/03-function-language.md` §1）入口处：
//!
//! ```text
//! 0x4ed7a3  movzbl (%r15,%rax,1),%ecx  ; c = s[i]
//! 0x4ed7a8  lea    -0x41(%rcx),%esi    ; c - 'A'
//! 0x4ed7ab  cmp    $0x1a,%sil
//! 0x4ed7af  setb   %sil                ; 大写 ASCII？
//! 0x4ed7b3  shl    $0x5,%sil           ;  +0x20
//! 0x4ed7b7  or     %cl,%sil
//! 0x4ed7ba  mov    %sil,(%r15,%rax,1)  ; 就地写回 ⇒ ASCII 小写化
//! ...
//! 0x4ed7ce  lea 0x1c4342(%rip) -> rcx  ; from = "pi"(2)   to = 0x1c4344 "n"(1)
//! 0x4ed831  lea 0x1c4345(%rip) -> rcx  ; from = "π"(2B)   to = 0x1c4344 "n"(1)
//! 0x4ed884  lea 0x1c4347(%rip) -> rsi  ; 77 字节 token 模式 → 引擎调用
//! ```
//!
//! ⇒ **`pi`→`n`、`π`→`n` 发生在解析器里，不在输入框里**；
//! 隐式乘不是文本变换，是解析规则（`docs/03` §6）。77 字节的 token 模式
//! （`0x1c4347`，逐字节**观测**）：
//!
//! ```text
//! [0-9]*\.?[0-9]+|\(|\)|x|y'|y|\+|-|\*|/|\^|sqrt|log|abs|sin|cos|tan|ln|n|exp|e
//! ```
//!
//! ⚠️ **推断**：这条模式交给的是**引擎**的 `RegEx`（Godot 4 `RegEx.new()`），
//! 不是 Rust 的 `regex` crate —— `0x4ed898` 是 `call *0x9bc280(%rip)`，该槽是
//! `R_X86_64_RELATIVE` → `0x7547f0`（GDExtension 接口包装器），不是本地 Rust 函数。
//! 缺的证据：`0x7547f0` 对应的 `GDExtensionInterface` 成员名。
//!
//! # 键盘模型（`gui_input`）：28 个"死键合成 / 上标"字符折回 ASCII
//!
//! `gui_input`（[`FN_GUI_INPUT`]）只处理 `InputEventKey`：取 `unicode`（一个 `char`），
//! 按**字符码**分发（跳转表 `0x1c3d5c`，`0x4fef60` 处 `jmp *`，198 项，覆盖
//! `0xB2..=0x177`）：
//!
//! * 198 项里只有 **28 项**不落到"默认返回"（`0x4ff27b`），其余 **170 项**直接返回；
//! * 这 28 项各自 `mov $0x??,%eax` 后汇合到 `0x4ff1e1`，把该 ASCII 字符
//!   `format!` 成 `String`，再调 [`FN_INSERT_TEXT`]；
//! * **ASCII 字符（数字 / 括号 / 运算符 / 字母）根本不在表里** ⇒ 走默认分支返回，
//!   由基类 `LineEdit` 自己处理。
//!
//! 也就是说 `gui_input` 的唯一职责是：**把某些键盘布局（死键）和上标产生的字符折回
//! ASCII**，再把折出来的字符插进去（插入后 `call("force_update_cb")`）。28 条映射见
//! [`COMPOSED_FOLDS`]。
//!
//! `'^'`（码位 `0x5e`）是唯一在表**之前**单独判的字符（`0x4feeeb`–`0x4fef06`）：
//! 三次引擎属性读取之后才走 `0x4fef7a` 插入 `"^"`（字面量 `0x1c4341`，长度 1）。
//! ⚠️ 那三次读取的**语义未知**，见末尾"未知"。
//!
//! # 编辑模型：按 token 移动/删除，落地成 N 次基类退格
//!
//! 四个编辑方法都不自己改字符串，而是**退格 N 次**：`0x4f4410`
//! （[`FN_DELETE_CHARS_AT_CARET`]）是个循环，每轮 `call("delete_char_at_caret")`
//! （字面量 `0x1c44a2`，长度 **20**）：
//!
//! ```text
//! 0x4f4440  call 4ddb40                 ; 取对象
//! 0x4f444b  mov  $0xa,%edx
//! 0x4f4453  lea  "upcast_ref"(10)       ; 基类引用
//! 0x4f447a  call 39fe10                 ; NameCall：方法名 = "delete_char_at_caret"(0x14)
//! 0x4f448c  dec  %ebx
//! 0x4f448e  jne  0x4f4440               ; 循环 N 次
//! ```
//!
//! ⇒ "token 有多长"这个**判据本身**内联在四个方法里（各自调 `0x3e90f0` 若干次，
//! 那是 `str`/`String` 的字符迭代辅助），**规则未解**。本文件因此只固定
//! **可观测的那一半**：`N` 次退格 = [`delete_chars_at_caret_plan`]。
//!
//! # 提交路径：`on_text_submitted` → 弱回调 `"fire"`
//!
//! `on_text_submitted` 的 ptrcall 实例（`0x427070`；方法体被内联进 gdext 的
//! `handle_fallible_call` 单态化）在 `bind_mut()` 之后调 [`FN_CALL_FIRE`]（`0x4f4250`）：
//!
//! ```text
//! 0x4270eb  call 4f4250                ; (self, 提交的文本)
//! 0x4f425c  mov  [rdi+0x68],%rdi       ; 字段 0x68：一个 dyn 回调
//! 0x4f4269  lea  "fire"(4),%rsi        ; .rodata 0x1b97c8
//! 0x4f4275  call *0x20(%rax)           ; dyn Trait::call("fire", <文本>)
//! ```
//!
//! ⇒ 提交文本**原样**交给回调，中间没有任何字符串变换。见 [`submit_intent`]。

// ---------------------------------------------------------------------------
// 1. 函数表
// ---------------------------------------------------------------------------

/// 原版类名（gdext 类名，`.rodata` `0x1c41e1`，长度 `0x15` = 21）。档位：**观测**。
pub const CLASS: &str = "GraphwarFunctionInput";

/// 原版源文件名（`panic::Location` 的 `file` 段；`.rodata` 里也有同一串）。档位：**观测**。
pub const SOURCE_FILE: &str = "src/graphwar_function_input.rs";

/// 本文件的复核口径注记。
pub const VERIFICATION_NOTE: &str = "\
name_to_addr.tsv 的 7 个唯一锚逐条复核通过（锚集合与地址一一对应，锚数与表一致）。
⚠️ 但仓库自带工具 tools/class_probe.py 对本文件的 Location→函数归属是**错的**：
它把 src/graphwar_function_input.rs:97:63 归给 0x4f57d0（那是商店按钮里引用
'SkinsVbox'/'res://shop_button_entry.tscn' 的函数），而真正引用该 Location 的指令是
0x4f4974，落在 0x4f44c0 体内。REA resolve_containing_procedure 复核：
Ghidra 0x5f4974 ∈ FUN_005f44c0。以反汇编为准。";

/// 证据档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    /// 观测：地址 / 字节可指。
    Observed,
    /// 推断：有依据，缺一项证据。
    Inferred,
}

/// 单条函数的证据记录。
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    /// ELF 虚地址（`objdump` 口径）。Ghidra = 本值 + `0x100000`。
    pub vaddr: u64,
    /// 线性"下一个起点表"给出的跨度（`analysis/funcstarts_ghidra_va.pkl`）。
    /// 读法见 [`LINEAR_END_CAVEAT`]。
    pub span: u32,
    /// 原版方法名；`None` = 没有 Location 锚，名字没落地。
    pub name: Option<&'static str>,
    /// 本文件里指向它的记号名（便于交叉引用）。
    pub marker: &'static str,
    /// 角色（一句话）。
    pub role: &'static str,
    /// 档位（地址归属的档位）。
    pub grade: Grade,
}

/// 线性函数边界表的坑。
pub const LINEAR_END_CAVEAT: &str = "\
funcstarts_ghidra_va.pkl 只给起点，不给归属；Rust 的冷段会被放到别的函数之间。
本模块唯一一处：gui_input 的主块从 0x4fedc0 起，表里下一个起点是 0x4fefd5，但
Ghidra 的函数体还有 4 段冷块（0x5ff046–0x5ff0ef 等），total 713 B / span 856 B
⇒ 到 0x4ff117 才结束。判归属请用 REA 的 procedure_info / resolve_containing_procedure。";

/// gdext 惰性注册驱动（四兄弟之一）：once 锁 + 遍历静态函数指针表，
/// 唯一引用的 Location 是 `src/graphwar_function_input.rs:434:1`。
pub const FN_REGISTER_A: u64 = 0x4e6d_c0;
/// 见 [`FN_REGISTER_A`]（另一张表）。
pub const FN_REGISTER_B: u64 = 0x4e6f_40;
/// 见 [`FN_REGISTER_A`]。
pub const FN_REGISTER_C: u64 = 0x4e9e_e0;
/// 见 [`FN_REGISTER_A`]。
pub const FN_REGISTER_D: u64 = 0x4ea2_d0;
/// 另一个注册驱动，唯一引用的 Location 是 `src/graphwar_function_input.rs:466:1`。
pub const FN_REGISTER_E: u64 = 0x4ea0_80;
/// 唯一引用 `src/graphwar_function_input.rs:27:10`：once 锁下把一条 `0x78` 字节记录
/// （3 个代码指针 + 若干 `&str` 指针）压进一个全局 `Vec`。
pub const FN_CLASS_INIT: u64 = 0x4ea4_80;
/// `force_update_cb`（原版名；5 个唯一锚）。
pub const FN_FORCE_UPDATE_CB: u64 = 0x4f22_00;
/// `move_caret_left`（原版名；1 个唯一锚）。
pub const FN_MOVE_CARET_LEFT: u64 = 0x4f32_10;
/// `move_caret_right`（原版名；6 个唯一锚）。
pub const FN_MOVE_CARET_RIGHT: u64 = 0x4f39_10;
/// `delete_token_at_caret`（原版名；1 个唯一锚）。
pub const FN_DELETE_TOKEN_AT_CARET: u64 = 0x4f44_c0;
/// `delete_token_after_caret`（原版名；6 个唯一锚）。
pub const FN_DELETE_TOKEN_AFTER_CARET: u64 = 0x4f4a_60;
/// `ready`（原版名；1 个唯一锚；虚拟 `ready` 的实现）。
pub const FN_READY: u64 = 0x4fd1_b0;
/// `process`（原版名；10 个唯一锚；虚拟 `process` 的实现）。
pub const FN_PROCESS: u64 = 0x4fd4_a0;
/// `gui_input`（虚拟 `gui_input` 的实现）。**没有** Location 锚，归属见 [`Grade::Inferred`]。
///
/// 定位链（三步都可指）：
/// 1. `.rodata` `0x1c46c9` = `"gui_input"`（9 字节）；
/// 2. 唯一引用它的是注册闭包 `0x4f8dc0`（同一闭包也引用类名 `0x1c41e1`），
///    该闭包调用的 trampoline 是 `0x425a20`；
/// 3. `0x425a20` 里 `0x425c04` 处 `call *0x9bb7c8` → `R_X86_64_RELATIVE` 槽
///    `0x9bb7c8` 的 addend = **`0x4fedc0`**。
pub const FN_GUI_INPUT: u64 = 0x4fed_c0;
/// 无锚助手：插入文本 + `call("force_update_cb")`。**推断（强）**。
pub const FN_INSERT_TEXT: u64 = 0x4f43_20;
/// 无锚助手：把提交的文本交给 `dyn` 回调的 `"fire"`。**推断（强）**。
pub const FN_CALL_FIRE: u64 = 0x4f42_50;
/// 无锚助手：循环 N 次 `call("delete_char_at_caret")`。**观测**。
pub const FN_DELETE_CHARS_AT_CARET: u64 = 0x4f44_10;

/// 17 条函数记录（13 有锚 + 4 无锚），**按地址升序**。
pub const FUNCTIONS: [FunctionEntry; 17] = [
    FunctionEntry {
        vaddr: FN_REGISTER_A,
        span: 0x180,
        name: None,
        marker: "FN_REGISTER_A",
        role: "gdext 惰性注册驱动：once 锁 + 遍历静态函数指针表；Location 434:1",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_REGISTER_B,
        span: 0x180,
        name: None,
        marker: "FN_REGISTER_B",
        role: "同上（另一张表）",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_REGISTER_C,
        span: 0x1a0,
        name: None,
        marker: "FN_REGISTER_C",
        role: "同上",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_REGISTER_E,
        span: 0x250,
        name: None,
        marker: "FN_REGISTER_E",
        role: "同上；唯一引用 Location 466:1",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_REGISTER_D,
        span: 0x1b0,
        name: None,
        marker: "FN_REGISTER_D",
        role: "同上（434:1 四兄弟之一）",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_CLASS_INIT,
        span: 0x210,
        name: None,
        marker: "FN_CLASS_INIT",
        role: "once 锁下把 0x78 字节注册记录压进全局 Vec；Location 27:10",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_FORCE_UPDATE_CB,
        span: 0xfa0,
        name: Some("force_update_cb"),
        marker: "FN_FORCE_UPDATE_CB",
        role: "刷新：读文本 → 去空格 → 送路径计算 → 与 game_client 对账",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_MOVE_CARET_LEFT,
        span: 0x700,
        name: Some("move_caret_left"),
        marker: "FN_MOVE_CARET_LEFT",
        role: "光标左移一个 token",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_MOVE_CARET_RIGHT,
        span: 0x940,
        name: Some("move_caret_right"),
        marker: "FN_MOVE_CARET_RIGHT",
        role: "光标右移一个 token",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_CALL_FIRE,
        span: 0xd0,
        name: None,
        marker: "FN_CALL_FIRE",
        role: "把文本交给 dyn 回调的 \"fire\"（on_text_submitted 唯一的项目级调用）",
        grade: Grade::Inferred,
    },
    FunctionEntry {
        vaddr: FN_INSERT_TEXT,
        span: 0xf0,
        name: None,
        marker: "FN_INSERT_TEXT",
        role: "插入文本 + call(\"force_update_cb\")",
        grade: Grade::Inferred,
    },
    FunctionEntry {
        vaddr: FN_DELETE_CHARS_AT_CARET,
        span: 0xb0,
        name: None,
        marker: "FN_DELETE_CHARS_AT_CARET",
        role: "N 次 call(\"delete_char_at_caret\")",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_DELETE_TOKEN_AT_CARET,
        span: 0x5a0,
        name: Some("delete_token_at_caret"),
        marker: "FN_DELETE_TOKEN_AT_CARET",
        role: "删光标前一个 token（N 次退格）",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_DELETE_TOKEN_AFTER_CARET,
        span: 0x970,
        name: Some("delete_token_after_caret"),
        marker: "FN_DELETE_TOKEN_AFTER_CARET",
        role: "删光标后一个 token（N 次退格）",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_READY,
        span: 0x2f0,
        name: Some("ready"),
        marker: "FN_READY",
        role: "虚拟 _ready：接三条信号（见 SIGNAL_WIRING）",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_PROCESS,
        span: 0x1920,
        name: Some("process"),
        marker: "FN_PROCESS",
        role: "虚拟 _process：两次 call(\"force_update_cb\") + 与 game_client 交互",
        grade: Grade::Observed,
    },
    FunctionEntry {
        vaddr: FN_GUI_INPUT,
        span: 0x357,
        name: None,
        marker: "FN_GUI_INPUT",
        role: "虚拟 _gui_input：InputEventKey 的 unicode → 28 条折叠 + '^' 特判",
        grade: Grade::Inferred,
    },
];

/// 按地址找记录。
pub fn function_at(vaddr: u64) -> Option<&'static FunctionEntry> {
    FUNCTIONS.iter().find(|f| f.vaddr == vaddr)
}

/// 有原版名字的那 7 条（`name_to_addr.tsv` 唯一锚）。
pub fn named_functions() -> Vec<&'static FunctionEntry> {
    FUNCTIONS.iter().filter(|f| f.name.is_some()).collect()
}

/// Ghidra / REA 地址（= vaddr + `0x100000`）。
pub const fn ghidra(vaddr: u64) -> u64 {
    vaddr + 0x10_0000
}

// ---------------------------------------------------------------------------
// 2. gdext 面：导出方法、虚拟方法、信号接线
// ---------------------------------------------------------------------------

/// 一个导出方法（`#[func]`）的注册记录。
///
/// ⚠️ `closure_a` / `closure_b` 是一对注册闭包（都用 `&str` 名字 + 两三个代码指针
/// 拼一条 `ClassMethodInfo`）。哪个是 ptrcall、哪个是 varcall **没有**逐指令确认
/// （`docs/19-android.md` §3.0.3 在 arm64 上把两者的差别记作"闭包里的两个
/// `ClassMethodInfo::register_extension_class_method`"），所以这里按 A/B 记，不硬指。
#[derive(Debug, Clone, Copy)]
pub struct MethodRegistration {
    /// Godot 侧名字（`.rodata` 字面量）。
    pub name: &'static str,
    /// 名字字面量的 vaddr。
    pub name_literal: u64,
    /// `name.len()`（指令立即数 `mov $len,%ecx` / `movl $len,N(%rsp)`）。
    pub name_len: u32,
    /// 第一个注册闭包。
    pub closure_a: u64,
    /// 第二个注册闭包。
    pub closure_b: u64,
    /// `closure_a` 调用的 trampoline（方法体被内联在 gdext `handle_fallible_call` 里）。
    pub trampoline_a: u64,
    /// `closure_b` 调用的 trampoline。
    pub trampoline_b: u64,
}

/// **导出方法 4 个**。档位：**观测**。
///
/// 依据：8 个注册闭包（`0x4f1390..0x4f1960`）每个都引用「类名 `0x1c41e1` + 一个
/// 方法名」，四条名字与 `docs/18-build.md` §6.5 从扩展 API 导出的方法表逐一对应。
pub const EXPORTS: [MethodRegistration; 4] = [
    MethodRegistration {
        name: "on_text_change",
        name_literal: 0x1c42_ff,
        name_len: 14,
        closure_a: 0x4f15_d0,
        closure_b: 0x4f19_60,
        trampoline_a: 0x42bc_20,
        trampoline_b: 0x431c_f0,
    },
    MethodRegistration {
        name: "on_text_submitted",
        name_literal: 0x1c44_5e,
        name_len: 17,
        closure_a: 0x4f14_50,
        closure_b: 0x4f16_90,
        trampoline_a: 0x4270_70,
        trampoline_b: 0x4267_e0,
    },
    MethodRegistration {
        name: "on_size_changed",
        name_literal: 0x1c44_4f,
        name_len: 15,
        closure_a: 0x4f13_90,
        closure_b: 0x4f18_70,
        trampoline_a: 0x42c0_30,
        trampoline_b: 0x4234_90,
    },
    MethodRegistration {
        name: "force_update_cb",
        name_literal: 0x1c44_6f,
        name_len: 15,
        closure_a: 0x4f15_10,
        closure_b: 0x4f17_80,
        trampoline_a: 0x42cb_e0,
        trampoline_b: 0x4340_c0,
    },
];

/// 一个虚拟方法的注册记录。
#[derive(Debug, Clone, Copy)]
pub struct VirtualRegistration {
    /// 虚拟方法名（Godot 侧，带下划线的是 `_ready`，gdext 里注册成 `ready`）。
    pub name: &'static str,
    /// 名字字面量 vaddr。
    pub name_literal: u64,
    /// 指令里的长度。
    pub name_len: u32,
    /// `ClassVirtualInfo` 构造闭包。
    pub info_closure: u64,
    /// 该闭包调用的 trampoline。
    pub trampoline: u64,
    /// trampoline 通过相对槽间接调用的**实现**。
    pub implementation: u64,
    /// 承载"实现指针"的 `R_X86_64_RELATIVE` 槽。
    pub impl_slot: u64,
}

/// **虚拟方法 3 个**。档位：**观测**（类名 + 虚拟名对；三个 `ClassVirtualInfo`
/// 闭包在 `0x9bb530`/`0x9bb538`/`0x9bb540` 连号，由 `0x40d600` 使用）。
pub const VIRTUALS: [VirtualRegistration; 3] = [
    VirtualRegistration {
        name: "ready",
        name_literal: 0x1c40_ea,
        name_len: 5,
        info_closure: 0x4f8d_00,
        trampoline: 0x4299_f0,
        implementation: FN_READY,
        impl_slot: 0x9bb8_00,
    },
    VirtualRegistration {
        name: "process",
        name_literal: 0x1c41_b0,
        name_len: 7,
        info_closure: 0x4f8c_40,
        trampoline: 0x4329_a0,
        implementation: FN_PROCESS,
        impl_slot: 0x9bb8_40,
    },
    VirtualRegistration {
        name: "gui_input",
        name_literal: 0x1c46_c9,
        name_len: 9,
        info_closure: 0x4f8d_c0,
        trampoline: 0x425a_20,
        implementation: FN_GUI_INPUT,
        impl_slot: 0x9bb7_c8,
    },
];

/// `ready` 里接的三条"自己的信号 → 自己的方法"。档位：**观测**。
///
/// `0x4fd1b0` 里三段结构相同（`Callable::from_object_method(self, handler)` +
/// `StringName::from(signal)` + `connect`）：
///
/// | 信号 | 字面量 | 长度 | 处理函数 | 字面量 | 长度 | `ready` 内指令 |
/// |---|---|---|---|---|---|---|
/// | `size_changed` | `0x1c46d2` | 12 | `on_size_changed` | `0x1c444f` | 15 | `0x4fd1e0` / `0x4fd2a6` |
/// | `text_changed` | `0x1c430d` | 12 | `on_text_change` | `0x1c42ff` | 14 | `0x4fd2cf` / `0x4fd324` |
/// | `text_submitted` | `0x1c46de` | 14 | `on_text_submitted` | `0x1c445e` | 17 | `0x4fd34f` / `0x4fd3a4` |
///
/// ⇒ 输入框**自己接自己**：这三条信号本来就是 `LineEdit` 的信号，处理函数是本类的
/// `#[func]`。与 `docs/16-client.md` §1 的运行时快照（2724 条连接里那 6 条游戏 UI
/// 人工连接）一致。
pub const SIGNAL_WIRING: [(&str, &str); 3] = [
    ("size_changed", "on_size_changed"),
    ("text_changed", "on_text_change"),
    ("text_submitted", "on_text_submitted"),
];

/// 信号名 → 处理函数名。
pub fn handler_for_signal(signal: &str) -> Option<&'static str> {
    SIGNAL_WIRING
        .iter()
        .find(|(s, _)| *s == signal)
        .map(|(_, h)| *h)
}

/// 处理函数名 → 信号名。
pub fn signal_for_handler(handler: &str) -> Option<&'static str> {
    SIGNAL_WIRING
        .iter()
        .find(|(_, h)| *h == handler)
        .map(|(s, _)| *s)
}

// ---------------------------------------------------------------------------
// 3. 文本处理：唯一的一步（删空格）
// ---------------------------------------------------------------------------

/// 原版唯一的一个字符级文本变换：**删掉所有空格**。
///
/// 证据链见模块头 §1（`0x4f28be`–`0x4f28dc` 的 `str::replace` + `0x1c44a1` = `' '`）。
/// 档位：**观测**（"被替换的是空格"是观测；"该调用作用于输入框文本"由同一函数里
/// 紧邻的文本读取支持，属**推断**）。
///
/// 注意它**只**做这一件事：不转 π、不补 `*`、不动 `\t`/`\n`、不大写转小写。
pub fn strip_spaces(text: &str) -> String {
    text.replace(STRIPPED_CHAR, "")
}

/// 被替换掉的字符（`.rodata` `0x1c44a1`，1 字节）。档位：**观测**。
pub const STRIPPED_CHAR: char = ' ';

// ---------------------------------------------------------------------------
// 4. 键盘模型：28 条"死键合成 / 上标"折叠
// ---------------------------------------------------------------------------

/// `gui_input` 跳转表（`0x1c3d5c`，198 项）里**不走默认分支**的 28 个字符，
/// 以及它们被折叠成的 ASCII。
///
/// 左列 = `InputEventKey.unicode` 的码位；右列 = 各分支 `mov $0x??,%eax` 的立即数。
/// 档位：**观测**（跳转表 + 每个分支的 `mov` 都能逐条对上）。
///
/// 语义：这些字符是 **AZERTY/QWERTZ 的 `^` 死键合成结果**（`â ê î ô û` 与
/// `Ĉ Ĝ Ĥ Ĵ Ŝ Ŵ Ŷ` 及其大写）与 **Unicode 上标**（`² ³ ¹`、`·`）。函数语法里只有
/// ASCII 字母有意义，所以输入框把它们折回来再插入；不折的话
/// `delete_token_*` 的字符扫描会把 token 判错。
pub const COMPOSED_FOLDS: [(char, char); 28] = [
    ('\u{00b2}', '2'), // ²  0x4fef70: mov $0x32
    ('\u{00b3}', '3'), // ³  0x4ff1a4: mov $0x33
    ('\u{00b9}', '1'), // ¹  0x4ff1b9: mov $0x31
    ('\u{00b7}', '.'), // ·  0x4ff096: mov $0x2e
    ('\u{00c2}', 'A'), // Â  0x4ff188
    ('\u{00ca}', 'E'), // Ê  0x4ff181
    ('\u{00ce}', 'I'), // Î  0x4ff18f
    ('\u{00d4}', 'O'), // Ô  0x4ff15e
    ('\u{00db}', 'U'), // Û  0x4ff17a
    ('\u{00e2}', 'a'), // â  0x4ff173
    ('\u{00ea}', 'e'), // ê  0x4ff140
    ('\u{00ee}', 'i'), // î  0x4ff14a
    ('\u{00f4}', 'o'), // ô  0x4ff1b2
    ('\u{00fb}', 'u'), // û  0x4ff10e
    ('\u{0108}', 'C'), // Ĉ  0x4ff19d
    ('\u{0109}', 'c'), // ĉ  0x4ff122
    ('\u{011c}', 'G'), // Ĝ  0x4ff154
    ('\u{011d}', 'g'), // ĝ  0x4ff1ab
    ('\u{0124}', 'H'), // Ĥ  0x4ff118
    ('\u{0125}', 'h'), // ĥ  0x4ff196
    ('\u{0134}', 'J'), // Ĵ  0x4ff1ce
    ('\u{0135}', 'j'), // ĵ  0x4ff12c
    ('\u{015c}', 'S'), // Ŝ  0x4ff136
    ('\u{015d}', 's'), // ŝ  0x4ff16c
    ('\u{0174}', 'W'), // Ŵ  0x4ff1c7
    ('\u{0175}', 'w'), // ŵ  0x4ff1d5
    ('\u{0176}', 'Y'), // Ŷ  0x4ff1c0
    ('\u{0177}', 'y'), // ŷ  0x4ff165
];

/// 查折叠表。
pub fn fold_composed(c: char) -> Option<char> {
    COMPOSED_FOLDS
        .iter()
        .find(|(from, _)| *from == c)
        .map(|(_, to)| *to)
}

/// `gui_input` 对一次按键的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// 输入框**接管**：把该字符插进文本（随后 `call("force_update_cb")`）。
    Insert(char),
    /// 输入框不管：默认分支返回，由基类 `LineEdit` 处理。
    Delegated,
}

/// 把 `InputEventKey` 的 `unicode` 判成 [`KeyAction`]。顺序与二进制一致：
///
/// 1. `char::from_u32` 失败（代理区 `0xD800..=0xDFFF`，`0x4fef2b`–`0x4fef3d`）→ 直接返回
///    （这一条在本函数签名之外：调用方传进来的已经是 `char`）；
/// 2. `'^'`（`0x5e`）→ `0x4feee3` 的**附加条件**成立才插入 `'^'`（`0x4fef7a`）；
/// 3. 跳转表 `0x1c3d5c` 命中 → 插入折叠后的字符；
/// 4. 其余 → [`KeyAction::Delegated`]。
///
/// ⚠️ 第 2 步的附加条件（`0x4fee94` / `0x4feea5` / `0x4feee3` 三次引擎属性读取）
/// 语义**未知**，这里用参数 `caret_condition` 显式暴露，见末尾"未知"。
pub fn classify_key(unicode: char, caret_condition: bool) -> KeyAction {
    if unicode == CARET_CHAR {
        return if caret_condition {
            KeyAction::Insert(CARET_CHAR)
        } else {
            KeyAction::Delegated
        };
    }
    match fold_composed(unicode) {
        Some(c) => KeyAction::Insert(c),
        None => KeyAction::Delegated,
    }
}

/// `'^'`：`gui_input` 里唯一在折叠表**之前**单独判的字符。
/// 字面量 `0x1c4341`，长度 1（`0x4fef81` `mov $0x1,%edx`）。档位：**观测**。
pub const CARET_CHAR: char = '^';

/// 跳转表覆盖的码位区间（198 项 ⇒ `0xB2..=0x177`）。档位：**观测**。
pub const FOLD_TABLE_RANGE: (char, char) = ('\u{00b2}', '\u{0177}');

// ---------------------------------------------------------------------------
// 5. 编辑操作：把"按 token"落到"按字符"
// ---------------------------------------------------------------------------

/// 基类上"删光标前一个字符"的方法名（`.rodata` `0x1c44a2`，长度 **20**）。档位：**观测**。
pub const BACKSPACE_METHOD: &str = "delete_char_at_caret";

/// 插入文本之后触发的刷新方法名（`.rodata` `0x1c446f`，长度 **15**）。档位：**观测**。
///
/// `0x4f4398` `lea` + `0x4f43a9` `mov $0xf,%ecx`；`process` 在 `0x4fdecd` /
/// `0x4fe1b5` 也用它。
pub const REFRESH_METHOD: &str = "force_update_cb";

/// 同一地址、长度 12 = `"force_update"`（LLVM 把两个字面量合并到了同一缓冲区）。
/// 档位：**观测**（`0x4fdecd` / `0x4fe1b5` 两处 `lea` 加 `mov $0xf,%ecx` 与
/// `mov $0xc,%ecx` 的差别就是长度）。
pub const REFRESH_PREFIX: &str = "force_update";

/// 「退格 N 次」的计划：`0x4f4410` 的循环体就是这件事，`N` 由调用者算出。
///
/// `delete_token_at_caret`（`0x4f44c0`）与 `delete_token_after_caret`（`0x4f4a60`）
/// 分别调它 4 / 5 次；`move_caret_*` 不用它。
///
/// 档位：**观测**（循环 + 方法名 + 长度）；"N 等于 token 的字符数"是**推断**
/// （缺：token 边界判据本身，见末尾"未知"）。
pub fn delete_chars_at_caret_plan(n: usize) -> Vec<&'static str> {
    vec![BACKSPACE_METHOD; n]
}

/// 一个最小的"文本 + 光标"模型，只表达**已被观测到**的编辑语义。
///
/// 光标按 **`char` 下标**（不是字节）：`move_caret_*` 与 `delete_token_*` 都走
/// `0x3e90f0`（`str`/`String` 的字符迭代辅助），`delete_char_at_caret` 也是
/// Godot `LineEdit` 的**字符**级 API。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Buffer {
    chars: Vec<char>,
    caret: usize,
}

impl Buffer {
    /// 新建，光标在开头。
    pub fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            caret: 0,
        }
    }

    /// 新建，光标在末尾（`move_caret_end` 的语义）。
    pub fn with_caret_at_end(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let caret = chars.len();
        Self { chars, caret }
    }

    /// 文本。
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// 光标（`char` 下标）。
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// 长度（`char` 数）。
    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// 是否空。
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    /// 光标移到末尾。
    pub fn move_caret_end(&mut self) {
        self.caret = self.chars.len();
    }

    /// 光标移到开头。
    pub fn move_caret_home(&mut self) {
        self.caret = 0;
    }

    /// 光标左移 `n` 个字符（夹到 0）。
    pub fn move_caret_left(&mut self, n: usize) {
        self.caret = self.caret.saturating_sub(n);
    }

    /// 光标右移 `n` 个字符（夹到末尾）。
    pub fn move_caret_right(&mut self, n: usize) {
        self.caret = (self.caret + n).min(self.chars.len());
    }

    /// 在光标处插入文本，**原样**（[`FN_INSERT_TEXT`] 的语义，不做任何规范化）。
    ///
    /// 插入后光标落在插入内容之后。
    pub fn insert_text_at_caret(&mut self, text: &str) {
        let ins: Vec<char> = text.chars().collect();
        let n = ins.len();
        self.chars.splice(self.caret..self.caret, ins);
        self.caret += n;
    }

    /// 在光标前删掉 `n` 个字符（= `n` 次 `delete_char_at_caret`），返回实际删掉的个数。
    pub fn delete_chars_at_caret(&mut self, n: usize) -> usize {
        let k = n.min(self.caret);
        let start = self.caret - k;
        self.chars.drain(start..self.caret);
        self.caret = start;
        k
    }

    /// 按 [`delete_chars_at_caret_plan`] 执行，便于把"计划"与"执行"对账。
    pub fn run_plan(&mut self, plan: &[&'static str]) -> usize {
        assert!(
            plan.iter().all(|m| *m == BACKSPACE_METHOD),
            "0x4f4410 只 call(\"delete_char_at_caret\")"
        );
        self.delete_chars_at_caret(plan.len())
    }
}

// ---------------------------------------------------------------------------
// 6. 一次按键的完整意图（不含 Godot 类型）
// ---------------------------------------------------------------------------

/// `gui_input` 一次按键的意图。**不含任何 Godot 类型**：原版通过 gdext 调引擎
/// （`format!("{}", ch)` + `call`），这里只描述"要做什么"，由宿主实现。
///
/// 档位：**观测**（分支与调用点）＋**推断**（把 `format!`+`call` 抽象成一个变体）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    /// 不处理，交给基类。
    Delegate,
    /// 插入这段文本，然后 `call("force_update_cb")`。
    InsertAndRefresh(String),
}

/// 把一次按键算成 [`InputIntent`]。
pub fn intent_for_key(unicode: char, caret_condition: bool) -> InputIntent {
    match classify_key(unicode, caret_condition) {
        KeyAction::Delegated => InputIntent::Delegate,
        KeyAction::Insert(c) => InputIntent::InsertAndRefresh(c.to_string()),
    }
}

/// 提交文本的意图：原样转给回调（`on_text_submitted` 的唯一可见副作用）。
///
/// 返回值 `(方法名, 文本)`；`"fire"` 字面量在 `0x1b97c8`（长度 4）。
/// 档位：**观测**（调用点）；"那个 dyn 回调是谁"是**推断**。
pub fn submit_intent(text: &str) -> (&'static str, &str) {
    ("fire", text)
}

// ---------------------------------------------------------------------------
// 7. 未知（写清楚缺什么）
// ---------------------------------------------------------------------------

/// 未解清单。写文档时照抄。
pub const UNKNOWNS: [&str; 6] = [
    "token 边界判据：move_caret_* / delete_token_* 里内联的 0x3e90f0 调用序列各代表什么\
（字符类判定？只看左右各一个字符？），未逐指令还原 ⇒ 只固定“N 次退格”，不固定 N 的算法。",
    "`'^'` 的附加条件：gui_input 0x4feeeb–0x4fef06 三次读引擎属性（字符串 \"upcast_ref\" \
经 0x407350）后与 '^' 比较，条件语义未知（shift？键盘布局？caret 是否为空？）。",
    "FN_CALL_FIRE 的 dyn 回调类型：[rdi+0x68] / [rax+0x20] 是个弱引用 trait 对象；\
weak_caller 模块有同类 idiom，但没有证据把它钉死。",
    "move_caret_end / move_caret_home / insert_text_at_caret / delete_chars_at_caret / clear / \
update_sizes 在 2.0.60 里**没有独立函数体**（name_to_addr.tsv 也把它们标“未知/歧义”）：\
要么被内联进调用者，要么只以 gdext Callable 形式存在。本文件只按“被谁调用”记录语义。",
    "FN_REGISTER_A..E 这 5 个 gdext 注册驱动各自注册的是 methods / constants / properties / \
signals / virtuals 的哪一张表：形状（once 锁 + 遍历静态表）可观测，表的归属未逐项还原。\
name_to_addr.tsv 把 __register_constants 标为“歧义”，候选正是 0x4e6dc0 / 0x4e6f40 / \
0x4e9ee0 / 0x4ea2d0 这四个，本轮未能消歧。",
    "process（0x4fd4a0；6.4 KB / 60 个基本块）的逐块语义：已确认它两次 \
call(\"force_update_cb\")、引用 src/graphwar_game_client.rs:152 的借用位置、入口构造 \
\"src/graphwar_function_input.rs:519\" 位置串，但没有把与 game_client 的数据流画完。",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_unique_anchors_are_exactly_the_named_functions() {
        let mut addrs: Vec<u64> = named_functions().iter().map(|f| f.vaddr).collect();
        assert_eq!(addrs.len(), 7);
        addrs.sort_unstable();
        assert_eq!(
            addrs,
            vec![
                FN_FORCE_UPDATE_CB,
                FN_MOVE_CARET_LEFT,
                FN_MOVE_CARET_RIGHT,
                FN_DELETE_TOKEN_AT_CARET,
                FN_DELETE_TOKEN_AFTER_CARET,
                FN_READY,
                FN_PROCESS,
            ]
        );
    }

    #[test]
    fn function_table_is_sorted_and_ghidra_offset_holds() {
        assert_eq!(FUNCTIONS.len(), 17);
        let mut prev = 0u64;
        for f in FUNCTIONS.iter() {
            assert!(f.vaddr > prev, "表必须按地址升序：{:#x}", f.vaddr);
            prev = f.vaddr;
            assert_eq!(ghidra(f.vaddr), f.vaddr + 0x10_0000);
            assert!(function_at(f.vaddr).is_some());
        }
        assert!(function_at(0x4f44_c1).is_none());
    }

    #[test]
    fn fold_table_has_28_distinct_entries_inside_the_table_range() {
        assert_eq!(COMPOSED_FOLDS.len(), 28);
        let mut froms: Vec<char> = COMPOSED_FOLDS.iter().map(|(a, _)| *a).collect();
        froms.sort_unstable();
        froms.dedup();
        assert_eq!(froms.len(), 28);
        for (c, _) in COMPOSED_FOLDS.iter() {
            assert!(
                *c >= FOLD_TABLE_RANGE.0 && *c <= FOLD_TABLE_RANGE.1,
                "{c:?} 不在跳转表覆盖的 0xB2..=0x177 里"
            );
        }
        for (_, c) in COMPOSED_FOLDS.iter() {
            assert!(c.is_ascii(), "{c:?} 应该折成 ASCII");
        }
    }

    #[test]
    fn fold_semantics_dead_keys_and_superscripts() {
        assert_eq!(fold_composed('â'), Some('a'));
        assert_eq!(fold_composed('ê'), Some('e'));
        assert_eq!(fold_composed('î'), Some('i'));
        assert_eq!(fold_composed('ô'), Some('o'));
        assert_eq!(fold_composed('û'), Some('u'));
        assert_eq!(fold_composed('ŷ'), Some('y'));
        assert_eq!(fold_composed('ĉ'), Some('c'));
        assert_eq!(fold_composed('²'), Some('2'));
        assert_eq!(fold_composed('³'), Some('3'));
        assert_eq!(fold_composed('¹'), Some('1'));
        assert_eq!(fold_composed('·'), Some('.'));
        // 不在表里的一律不折（注意 π 不折：π 的规范化在解析器）
        for c in ['x', 'y', '1', '(', '+', 'π', 'é', 'à', ' ', '^'] {
            assert_eq!(fold_composed(c), None, "{c:?} 不在 0x1c3d5c 表的非默认项里");
        }
    }

    #[test]
    fn ascii_keys_are_delegated_to_the_base_class() {
        // 本轮最重要的一条：ASCII 不在跳转表里（198 项只覆盖 0xB2..=0x177）
        for c in [
            '0', '9', '(', ')', '+', '-', '*', '/', 'x', 'y', '\'', 's', 'n', 'e', ' ',
        ] {
            assert_eq!(classify_key(c, false), KeyAction::Delegated, "{c:?}");
            assert_eq!(classify_key(c, true), KeyAction::Delegated, "{c:?}");
        }
    }

    #[test]
    fn caret_key_has_the_unknown_extra_condition() {
        assert_eq!(classify_key('^', true), KeyAction::Insert('^'));
        assert_eq!(classify_key('^', false), KeyAction::Delegated);
        assert_eq!(fold_composed('^'), None);
    }

    #[test]
    fn key_intent_inserts_and_refreshes() {
        assert_eq!(
            intent_for_key('â', true),
            InputIntent::InsertAndRefresh("a".into())
        );
        assert_eq!(intent_for_key('x', true), InputIntent::Delegate);
        assert_eq!(
            intent_for_key('^', true),
            InputIntent::InsertAndRefresh("^".into())
        );
    }

    #[test]
    fn the_only_text_transform_is_space_stripping() {
        assert_eq!(strip_spaces("sin ( x )"), "sin(x)");
        assert_eq!(strip_spaces("   "), "");
        // 只吃 ASCII 空格：其它空白 / π / 运算符都不动
        assert_eq!(strip_spaces("x\t+\nπ"), "x\t+\nπ");
        // pi 的规范化在解析器（MathFunction::new），不在输入框
        assert_eq!(strip_spaces("pi"), "pi");
        assert_eq!(STRIPPED_CHAR, ' ');
        let once = strip_spaces("a b  c");
        assert_eq!(strip_spaces(&once), once);
    }

    #[test]
    fn submit_path_does_not_clean_the_text() {
        assert_eq!(submit_intent("sin( x )"), ("fire", "sin( x )"));
        assert_eq!(submit_intent(""), ("fire", ""));
    }

    #[test]
    fn backspace_plan_is_one_call_per_char() {
        assert_eq!(delete_chars_at_caret_plan(0).len(), 0);
        assert_eq!(delete_chars_at_caret_plan(3), vec![BACKSPACE_METHOD; 3]);
        assert_eq!(BACKSPACE_METHOD.len(), 20);
        assert_eq!(REFRESH_METHOD.len(), 15);
        assert_eq!(REFRESH_PREFIX.len(), 12);
        assert!(REFRESH_METHOD.starts_with(REFRESH_PREFIX));
    }

    #[test]
    fn buffer_insert_is_verbatim_and_deletion_backs_off_the_caret() {
        let mut b = Buffer::with_caret_at_end("sin(x)");
        assert_eq!(b.caret(), 6);
        b.insert_text_at_caret("+1"); // 原样插入，不做任何规范化
        assert_eq!(b.text(), "sin(x)+1");
        assert_eq!(b.caret(), 8);
        assert_eq!(b.delete_chars_at_caret(2), 2);
        assert_eq!(b.text(), "sin(x)");
        b.move_caret_home();
        assert_eq!(b.delete_chars_at_caret(5), 0);
        assert_eq!(b.text(), "sin(x)");
    }

    #[test]
    fn buffer_run_plan_matches_the_delete_count() {
        let mut b = Buffer::with_caret_at_end("sqrt(x)");
        let plan = delete_chars_at_caret_plan(3);
        assert_eq!(b.run_plan(&plan), 3);
        assert_eq!(b.text(), "sqrt");
        assert_eq!(b.caret(), 4);
    }

    #[test]
    fn caret_moves_are_clamped() {
        let mut b = Buffer::new("^a");
        b.move_caret_left(3);
        assert_eq!(b.caret(), 0);
        b.move_caret_right(9);
        assert_eq!(b.caret(), 2);
        b.move_caret_home();
        assert_eq!(b.caret(), 0);
        b.move_caret_end();
        assert_eq!(b.caret(), 2);
        assert!(!b.is_empty() && b.len() == 2);
    }

    #[test]
    fn signal_wiring_is_exactly_three_pairs() {
        assert_eq!(SIGNAL_WIRING.len(), 3);
        assert_eq!(
            handler_for_signal("text_submitted"),
            Some("on_text_submitted")
        );
        assert_eq!(handler_for_signal("text_changed"), Some("on_text_change"));
        assert_eq!(handler_for_signal("size_changed"), Some("on_size_changed"));
        // "force_update" 是 process 里 call 的方法名，不是信号
        assert_eq!(handler_for_signal("force_update"), None);
        assert_eq!(signal_for_handler("force_update_cb"), None);
        for (_, h) in SIGNAL_WIRING.iter() {
            assert!(EXPORTS.iter().any(|e| e.name == *h), "{h} 必须是 #[func]");
        }
    }

    #[test]
    fn exports_match_the_extension_api_dump() {
        let names: Vec<&str> = EXPORTS.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            vec![
                "on_text_change",
                "on_text_submitted",
                "on_size_changed",
                "force_update_cb"
            ]
        );
        for e in EXPORTS.iter() {
            assert_eq!(e.name.len() as u32, e.name_len, "{}", e.name);
        }
    }

    #[test]
    fn virtuals_are_ready_process_gui_input_and_point_at_the_right_impls() {
        let names: Vec<&str> = VIRTUALS.iter().map(|v| v.name).collect();
        assert_eq!(names, vec!["ready", "process", "gui_input"]);
        for v in VIRTUALS.iter() {
            assert_eq!(v.name.len() as u32, v.name_len, "{}", v.name);
        }
        assert_eq!(VIRTUALS[0].implementation, FN_READY);
        assert_eq!(VIRTUALS[1].implementation, FN_PROCESS);
        assert_eq!(VIRTUALS[2].implementation, FN_GUI_INPUT);
        // gui_input 是唯一没有 Location 锚的实现
        assert_eq!(function_at(FN_GUI_INPUT).unwrap().grade, Grade::Inferred);
        assert_eq!(function_at(FN_READY).unwrap().grade, Grade::Observed);
    }

    #[test]
    fn constants_are_the_documented_strings() {
        assert_eq!(CLASS, "GraphwarFunctionInput");
        assert_eq!(CLASS.len(), 21); // 0x1c41e1，指令立即数 0x15
        assert_eq!(SOURCE_FILE, "src/graphwar_function_input.rs");
        assert_eq!(CARET_CHAR, '^');
        assert_eq!(FOLD_TABLE_RANGE, ('\u{00b2}', '\u{0177}'));
    }
}
