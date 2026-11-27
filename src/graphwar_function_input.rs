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
//! 那是 `str`/`String` 的字符迭代辅助）。**T80 已解开**：判据是四张写死在立即数里的
//! 字符串表（[`DELETE_TOKEN_TOKENS_AT_CARET`] / [`DELETE_TOKEN_TOKENS_AFTER_CARET`]），
//! 见本文件第 7 节；`move_caret_*` 用的是同一套表（未逐条落码，见 UNKNOWNS 第 1 条）。
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
// 7. token 删除：`delete_token_at_caret` / `delete_token_after_caret`
// ---------------------------------------------------------------------------
//
// 这两个方法此前在本文件里只固定了"退格几次"（`delete_chars_at_caret_plan`），
// "token 有多长"写成**未知**（T42 的 UNKNOWNS 第 1 条）。T80 逐指令读完后**解开了**：
// 判据不是"字符类"，而是**四张写死在立即数里的字符串表**。
//
// ## `delete_token_at_caret`（`0x4f44c0`，FDE `0x4f44c0..0x4f4a5a`，`0x59a` = 1434 B）
//
// 控制流（每条都能指到地址）：
// ```text
// 4f44d8  call *0x9ba680 -> 0x89b170        ; 取节点/基类句柄
// 4f44e2  call 0x44b1c0                     ; 取「文本」那一对字（ptr,len）
// 4f44f1  lea  "upcast_ref"(0x1c41b7, 0xa)  ; 4f4505 call 0x406840 = Gd::upcast_ref 包装
// 4f450d  call *0x9bc328 -> 0x8b8560        ; caret_column()  ⇒ %eax
// 4f4513  test eax,eax ; 4f4515 js 4f4966   ; < 0 ⇒ Result::unwrap 的 Err 臂（panic）
// 4f45e9  cmp ebp,4 ; 4f45ec jbe 4f4660     ; caret <= 4 ⇒ 跳过「5 字符」这一档
// 4f461d  cmp [rsp+0x20],5 / 4f4625 exec 5  ; 与 5 字符表逐字节 xor 比较（见下表）
// 4f46a2..4f46dc  6 次 cmp dword,imm        ; 4 字符表
// 4f4753..4f4801  7 次 (word xor + byte xor) ; 3 字符表
// 4f4870..4f48ad  7 次 cmp word,imm          ; 2 字符表
// 4f46f8/4f481d/4f48d6/4f48e5  mov esi,N ; call 0x4f4410   ; 命中 ⇒ 退格 N 次
// 4f48c7  mov esi,1 ; 4f48cf call 0x4f4410 ; 全不中 ⇒ 退格 1 次（默认臂）
// ```
// 每一档的"候选"是 **caret 左侧 n 个字符**：`lea rax,[rbx-n]` + `mov [rsp+0x48],n`
// 组成一个 `Range{start,end,idx,len}` 交给 `0x3e90f0`（`str` 切片辅助），回来的是
// `(ptr,len)`；先判 `len == n`（`cmp qword [rsp+0x20],n`）再逐字节比 ⇒ 这就是
// "取左邻 n 个字符"的全部含义，**没有字符类判定**。
//
// ## `delete_token_after_caret`（`0x4f4a60`，FDE `0x4f4a60..0x4f53c5`，`0x965` = 2405 B）
//
// 同一套表，但候选是 **caret 右侧 n 个字符**，门槛是 `caret + n <= len`
// （`0x4f4b93 cmp rbx,r13 ; jb`），命中后的动作是"先把 caret 右移 n
// （`0x4f4fcf inc ebp` + `call *0x9bc318 -> 0x8b8830` = 设 caret 列），再退格 n 次"
// （`0x4f4fee call 0x4f4410`）—— 因为 `0x4f4410` 只会删 caret **左侧**。
// 右移的溢出检查逐档不同：`cmp ebp,0x7ffffffc`(+4) / `0x7ffffffd`(+3) /
// `0x7ffffffe`(+5) / `0x7ffffffb`(+2)（`0x7fffffff` 是 +1 的默认臂），正好是
// `i32::MAX - (n-1)` ⇒ 印证"右移 n"，不是"右移 1 再删 n"。
// 它的表比 at_caret **多出一组"半截 token 的后缀"**（`qrt(` / `n(` / `t(` …），
// 因为这些后缀正是把 caret 放在 token 中间时右侧看到的东西。

/// `delete_token_at_caret`（`0x4f44c0`）的 token 表：**按长度降序**，逐条 = 立即数比较。
///
/// | n | token | 证据（`0x4f44c0` 体内的立即数） |
/// |---|---|---|
/// | 5 | `sqrt(` | `0x4f462a mov $0x74727173` + `0x4f4633 xor byte [r12+4],0x28` |
/// | 4 | `abs( sin( cos( tan( exp( sqrt` | `0x4f46a2/4f46ac/4f46b6/4f46c0/4f46ca/4f46d4` 的 `cmp dword [r12],imm` |
/// | 3 | `ln( abs sin cos tan exp sqr` | `0x4f4753..0x4f4801` 的 `xor word` + `xor byte [r12+2]` |
/// | 2 | `ln ab si co ta ex sq` | `0x4f4870..0x4f48ad` 的 `cmp word [r12],imm` |
///
/// 档位：**观测**（每个 token 都能指到一条 `cmp/xor` 指令 + 它的立即数）。
/// 语义（**推断**，理由在表里）：这些是函数名的**逐字符前缀**（`sq`/`sqr` 是
/// `sqrt` 的前缀），加上带左括号的完整形态 ⇒ 退格把"正在打的函数名"整段删掉。
pub const DELETE_TOKEN_TOKENS_AT_CARET: [(usize, &[&str]); 4] = [
    (5, &["sqrt("]),
    (4, &["abs(", "sin(", "cos(", "tan(", "exp(", "sqrt"]),
    (3, &["ln(", "abs", "sin", "cos", "tan", "exp", "sqr"]),
    (2, &["ln", "ab", "si", "co", "ta", "ex", "sq"]),
];

/// `delete_token_after_caret`（`0x4f4a60`）的 token 表。
///
/// 前四档与 [`DELETE_TOKEN_TOKENS_AT_CARET`] 同名，但**每个名字还多收"半截后缀"**：
/// 4 档 `0x4f4ca7 cmp dword,0x28747271` = `qrt(`；3 档 `0x4f4dc8 xor word,0x7271` +
/// `0x4f4dd3 xor byte [r12+2],0x74` = `qrt`，另有 `bs(`/`in(`/`os(`/`an(`/`xp(`/`rt(`；
/// 2 档 `0x4f4eed cmp word,0x286e` = `n(`，另有 `bs`/`in`/`os`/`an`/`xp`/`rt`/`s(`/`p(`/`t(`。
/// 档位：**观测**。
pub const DELETE_TOKEN_TOKENS_AFTER_CARET: [(usize, &[&str]); 4] = [
    (5, &["sqrt("]),
    (4, &["abs(", "sin(", "cos(", "tan(", "exp(", "sqrt", "qrt("]),
    (3, &["ln(", "qrt", "bs(", "in(", "os(", "an(", "xp(", "rt("]),
    (2, &["n(", "ln", "bs", "in", "os", "an", "xp", "rt", "s(", "p(", "t("]),
];

/// 光标**左侧**要一次删掉几个字符（`delete_token_at_caret` 的那个 N）。
///
/// 档位：**观测** —— 档位顺序（5→4→3→2→1）、每档的"caret 至少 n"门槛
/// （`cmp ebp,4` / `cmp ebp,2` 两处 `jbe`）与四张表全部来自指令。
///
/// * `caret >= 5` 才试 5 档；`caret >= 4` 才试 4 档（`pos > 4` 的 5 档不中时也回落试 4 档，
///   所以 4 档的实际门槛是 `caret >= 4`）；3 档 `caret >= 3`；2 档 `caret >= 2`；
/// * 全不中 ⇒ **1**（默认臂 `0x4f48c7 mov esi,1`），即使 caret 在 0 也返回 1
///   （原版由 `0x4f4410` 的循环把"无可删"吸收掉：`delete_chars_at_caret` 会夹到 0）。
pub fn token_len_at_caret(text: &str, caret: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    for (n, tokens) in DELETE_TOKEN_TOKENS_AT_CARET.iter() {
        if caret < *n {
            continue;
        }
        let cand: String = chars[caret - n..caret].iter().collect();
        if tokens.iter().any(|t| *t == cand) {
            return *n;
        }
    }
    1
}

/// 光标**右侧**要一次删掉几个字符（`delete_token_after_caret` 的那个 N）。
///
/// 档位：**观测**。与 [`token_len_at_caret`] 的差别有两处（都是指令给的）：
/// 门槛是 `caret + n <= len`（`0x4f4b93` 那一族 `cmp/jb`），表里多一组"半截后缀"。
/// `caret >= len` 时原版**直接结束**（`0x4f4f97 cmp rbx,r14 ; jbe`）⇒ 这里返回 0。
pub fn token_len_after_caret(text: &str, caret: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    if caret >= chars.len() {
        return 0;
    }
    for (n, tokens) in DELETE_TOKEN_TOKENS_AFTER_CARET.iter() {
        if caret + n > chars.len() {
            continue;
        }
        let cand: String = chars[caret..caret + n].iter().collect();
        if tokens.iter().any(|t| *t == cand) {
            return *n;
        }
    }
    1
}

impl Buffer {
    /// `0x4f44c0 GraphwarFunctionInput::delete_token_at_caret` —— 删光标左侧的 token。
    ///
    /// 体：`caret_column()`（`0x4f450d`，负 ⇒ `unwrap` panic）→ [`token_len_at_caret`]
    /// → `N` 次退格（`0x4f4410`）。返回实际退格数。
    ///
    /// 档位：**观测**（N 的判据 = 四张立即数表；退格 = `0x4f4410` 的循环）。
    pub fn delete_token_at_caret(&mut self) -> usize {
        // 0x4f450d call *0x9bc328 -> 0x8b8560 = caret_column()；0x4f4515 js = Err 臂
        let n = token_len_at_caret(&self.text(), self.caret);
        // 0x4f46f8 / 0x4f481d / 0x4f48d6 / 0x4f48e5 / 0x4f48c7 五处 mov esi,N
        self.delete_chars_at_caret(n)
    }

    /// `0x4f4a60 GraphwarFunctionInput::delete_token_after_caret` —— 删光标右侧的 token。
    ///
    /// 体：`caret + n <= len` 判定 → [`token_len_after_caret`] → caret 右移 `n`
    /// （`0x4f4fcf inc ebp` + `call *0x9bc318 -> 0x8b8830`）→ 退格 `n` 次
    /// （`0x4f4fee call 0x4f4410`，它只删左侧）。`caret` 已在末尾（`0x4f4f97`）⇒ 什么都不做，返回 0。
    ///
    /// 档位：**观测**。
    pub fn delete_token_after_caret(&mut self) -> usize {
        let n = token_len_after_caret(&self.text(), self.caret);
        if n == 0 {
            return 0;
        }
        self.move_caret_right(n);
        self.delete_chars_at_caret(n)
    }
}

// ---------------------------------------------------------------------------
// 8. 未知（写清楚缺什么）
// ---------------------------------------------------------------------------

/// 未解清单。写文档时照抄。
pub const UNKNOWNS: [&str; 6] = [
    "token 边界判据（第 1 条）：**T80 已解开** —— 八张立即数表见 \
`DELETE_TOKEN_TOKENS_AT_CARET` / `DELETE_TOKEN_TOKENS_AFTER_CARET`。残留未知两项：\
①`0x3e90f0` 取切片时按 **char** 还是按 **byte** 下标（表全是 ASCII ⇒ 结果相同，\
但非 ASCII 文本下两者会分叉，缺一条非 ASCII 的运行时观测）；②`0x4f4410` 命中后 caret 的\
最终位置（`delete_char_at_caret` 是引擎侧，未见其返回值）。",
    "`'^'` 的附加条件：gui_input 0x4feeeb–0x4fef06 三次读引擎属性（字符串 \"upcast_ref\" \
经 0x407350）后与 '^' 比较，条件语义未知（shift？键盘布局？caret 是否为空？）。",
    "FN_CALL_FIRE 的 dyn 回调类型：[rdi+0x68] / [rax+0x20] 是个弱引用 trait 对象；\
weak_caller 模块有同类 idiom，但没有证据把它钉死。",
    "move_caret_end / move_caret_home / insert_text_at_caret / delete_chars_at_caret / clear / \
update_sizes 在 2.0.60 里**没有独立函数体**（name_to_addr.tsv 也把它们标“未知/歧义”）：\
要么被内联进调用者，要么只以 gdext Callable 形式存在。本文件只按“被谁调用”记录语义。",
    "FN_REGISTER_A..E 的表归属：T80 把 **A/B 定成 methods / constants 这一对**（同一个 3 指令 \
thunk `0x40e670` 先 `call *0x9bb620`(A) 再 `jmp *0x9bb628`(B)；arm64 每类恰好两条 \
`ImplementsGodotApi::__register_methods` / `__register_constants`，methods 地址更低）。\
残留未知：C/D/E 各是哪一类 `_::__init::__inner_init`（4 槽相邻表里各占一槽，缺 gdext 版本的 \
vtable 槽序对齐）；静态表 `[0x9c1088]/[0x9c1090]` 的**填充者**（`.data` 初值为零 ⇒ 静态状态下\
不迭代，写者未知）。",
    "process（0x4fd4a0；6.4 KB / 60 个基本块）的逐块语义：已确认它两次 \
call(\"force_update_cb\")、引用 src/graphwar_game_client.rs:152 的借用位置、入口构造 \
\"src/graphwar_function_input.rs:519\" 位置串，但没有把与 game_client 的数据流画完。",
];

// ---------------------------------------------------------------------------
// T80. gdext 登记存储 / 注册驱动 / `__init::__inner_init`（本轮落码的 6 条宏产物）
// ---------------------------------------------------------------------------
//
// 这 6 条是 `GraphwarFunctionInput` 的**宏产物**：源码层对应物是 `#[derive(GodotClass)]` /
// `#[godot_api]` 属性，不是手写 fn（`tools/function_coverage.py` 的 R3 判法）。本仓把每条
// **机器函数**落成一个 fn：文档给该机器函数的 FDE / `panic::Location` / `.data` 静态槽 /
// 重定位槽 / 调用链，函数体复述它的**可观测动作**（遍历登记表 / 依次调用胶水）。
// 台账：`fixtures/f3_rest_anchors.tsv`；审计：`docs/06` §122。
//
// ## `__registration_storage::STORAGE`（arm64 每类一份 56 B）
//
// arm64 `.symtab`：`GraphwarFunctionInput::__registration_storage::STORAGE` @ arm64 `0x8b4220`
// （56 B；`tools/arm64_sym.py find __registration_storage` 共 12 份，每类一份）。
// x86 侧同构的 `.data` 静态块（**观测**：每条地址都是本节某条指令的 rip 目标）：
//
// | 偏移 | x86 地址 | 读者 | 语义 |
// |---|---|---|---|
// | +0x00 | `0x9c1078` | 两条注册驱动的 `lock cmpxchg` / `xchg` | 状态字（原子） |
// | +0x04 | `0x9c107c` | `movzx` 读 / `mov byte,1` 写 | poison 字节 |
// | +0x10 | `0x9c1088` | `__register_methods` | 表 A 指针 |
// | +0x18 | `0x9c1090` | `__register_methods` | 表 A 长度 |
// | +0x20 | `0x9c1098` | 读全 8 槽那条 `__init::__inner_init` | 未定 |
// | +0x28 | `0x9c10a0` | `__register_constants` | 表 B 指针 |
// | +0x30 | `0x9c10a8` | `__register_constants` | 表 B 长度 |
//
// ⚠️ `.data` 初值（直接读 ELF）：`[0x9c1088]` = **8**、`[0x9c1090]` = **0**、
// `[0x9c10a0]` = **8**、`[0x9c10a8]` = **0** ⇒ 指针初值不是合法指针、长度初值为 0 ⇒ **静态初值下循环不执行**；表在运行期被填充（填充者未知）。
// 表的**填充者未知**（`UNKNOWNS` 里有这一条；缺运行时观测）。
//
// ## 两条注册驱动的名字 = **推断**；体 = 观测
//
// 它们被**同一个 3 指令 thunk** 依次取用：`0x40e670`: `push rax` /
// `call *0x9bb620` / `pop rax` / `jmp *0x9bb628` ⇒ A 先于 B。
// arm64 `.symtab` 每类恰好两条 `ImplementsGodotApi::__register_methods` /
// `::__register_constants`（各 400 B，methods 地址更低）；Windows 2.0.60 COFF 同名两条
// （`fixtures/windows_name_to_addr.tsv`，`无一对一锚`）。⇒ A = `__register_methods`、
// B = `__register_constants`。**顺序是观测，名字本身仍是推断**（缺 gdext 版本的 trait 声明序）。

/// 表 A 的两个 `.data` 槽：`(指针, 长度)` —— `__register_methods` 读它们。
pub const REGISTER_METHODS_TABLE_SLOTS: (u64, u64) = (0x9c1088, 0x9c1090);
/// 表 B 的两个 `.data` 槽：`(指针, 长度)` —— `__register_constants` 读它们。
pub const REGISTER_CONSTANTS_TABLE_SLOTS: (u64, u64) = (0x9c10a0, 0x9c10a8);
/// 依次取用这两条注册驱动的 thunk：`push rax` / `call *A` / `pop rax` / `jmp *B`。
pub const REGISTER_THUNK: u64 = 0x40e670;
/// 两条注册驱动在重定位表里的槽（A = methods，B = constants）。
pub const REGISTER_SLOTS: (u64, u64) = (0x9bb620, 0x9bb628);

/// 一条 gdext 胶水 / 引擎调用（原版 `call` 指令）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlueCall {
    /// 调用点（vaddr）。
    pub site: u64,
    /// 被调者（vaddr）；间接调用时是重定位槽的 addend。
    pub target: u64,
    /// 间接调用经过的重定位槽（直接 `call rel32` 时为 0）。
    pub slot: u64,
}

/// gdext 胶水的宿主：原版每条 `call` 直接落到引擎 / gdext；本仓把它抽象成一个动作。
pub trait GlueHost {
    /// 执行一条调用。
    fn call(&mut self, c: GlueCall);
}

/// 按地址顺序把该机器函数 FDE 内**全部** `call` 指令交给宿主，返回条数。
///
/// ⚠️ 清单是**普查**（census）：panic 冷臂、drop、全局构造的调用**都在里面**。
/// 「哪几条属于直线路径」本轮没做控制流分析 ⇒ 清单**不是**执行序列（档位：清单=观测，
/// 直线划分=未知）。
pub fn replay_calls(calls: &[GlueCall], host: &mut dyn GlueHost) -> usize {
    for c in calls {
        host.call(*c);
    }
    calls.len()
}

/// 遍历一张「登记项表」并逐项调用，返回条数。
///
/// 原版的表是 `(ptr,len)` 两个静态槽（见本节 `STORAGE` 表）；这里把它作为参数表达 ——
/// 本仓没有「gdext 全局登记存储」这一层（宏产物，见 `AGENTS.md` §4）。
pub fn run_register_table(table: &[fn()]) -> usize {
    for f in table {
        f();
    }
    table.len()
}

/// `0x4e6dc0` —— `GraphwarFunctionInput::__register_methods`（**名字推断**，**体观测**）
///
/// FDE `0x4e6dc0..0x4e6f39`（`0x179` = 377 B）；`panic::Location` = `src/graphwar_function_input.rs:434:1`。
/// 重定位槽 `0x9bb620`（同一 thunk 的 A 半）。
///
/// ```text
/// 0x4e6dd4  lock cmpxchg dword ptr [rip + 0x4da29c], ecx  @0x9c1078 (<bin 0000000000000000>)
/// 0x4e6dfa  movzx   eax, byte ptr [rip + 0x4da27b]   @0x9c107c (<bin 0000000000000000>)
/// 0x4e6e09  mov     r15, qword ptr [rip + 0x4da280]  @0x9c1090 (<bin 0000000000000000>)
/// 0x4e6e15  mov     r12, qword ptr [rip + 0x4da26c]  @0x9c1088 (<bin 0800000000000000>)
/// 0x4e6e30  call    qword ptr [r12 + r13]
/// 0x4e6e41  movabs  rax, 0x7fffffffffffffff          imm=0x7fffffffffffffff
/// 0x4e6e59  xchg    dword ptr [rip + 0x4da219], eax  @0x9c1078 (<bin 0000000000000000>)
/// 0x4e6efa  mov     byte ptr [rip + 0x4da17b], 1     @0x9c107c (<bin 0000000000000000>)
/// ```
///
/// 状态机（逐条在上面）：抢状态字 → poison 字节检查 → **遍历表 A**（`call *(%r12,%r13)`，步长 8）
/// → `xchg` 释放（老值 == 2 走唤醒）→ poison 时置字节 1；poison 后再进入走
/// `Result::unwrap()` 的 Err 臂（panic 面 `0x1c3b13`）。
///
/// 落码：只落「逐项遍历登记表」这一步（状态机 / 唤醒 / poison panic 在本仓没有对应层，
/// 见本节文档与 `UNKNOWNS`）。档位：**观测**（体）+ **推断**（名）。
pub fn register_methods(table: &[fn()]) -> usize {
    // 0x4e6dc0 本函数起点；FDE 0x4e6dc0..0x4e6f39（377 B）
    // 0x4e6e30  call qword ptr [r12 + r13]   ← 本行模型：逐项调用表 A
    run_register_table(table)
}

/// `0x4e6f40` —— `GraphwarFunctionInput::__register_constants`（**名字推断**，**体观测**）
///
/// 与 [`register_methods`] **逐指令同形**，唯一差别 = 读的是**表 B**
/// （`0x9c10a0` 指针 / `0x9c10a8` 长度）。FDE `0x4e6f40..0x4e70b9`
/// （`0x179` = 377 B）；`panic::Location` = `src/graphwar_function_input.rs:434:1`；重定位槽 `0x9bb628`。
/// 档位：**观测**（体）+ **推断**（名）。
pub fn register_constants(table: &[fn()]) -> usize {
    // 0x4e6f40 本函数起点；FDE 0x4e6f40..0x4e70b9（377 B）
    // 与 register_methods 同形：表 B 的指针/长度 = 0x9c10a0 / 0x9c10a8
    run_register_table(table)
}

/// `0x4e9ee0` —— `GraphwarFunctionInput` 的 `_::__init::__inner_init`（Location `434:1`）
///
/// FDE `0x4e9ee0..0x4ea07c`（`0x19c` = 412 B）；重定位槽 `0x9ba388`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / 归属：**推断**（4 槽连续 + arm64 每类 4 个同名符号；缺 gdext vtable 槽序对齐）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L434A_CALLS`]）：
/// * `0x4e9f4d` `call 0x460bb0`（槽 0x9baaf0）
/// * `0x4e9fc4` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4e9fcf` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea00c` `call 0x925430`（槽 0x9ba678）
/// * `0x4ea014` `call 0x4ffcda`
/// * `0x4ea025` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4ea02d` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea045` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea04b` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea05b` `call 0x501080`
/// * `0x4ea062` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4ea06b` `call 0x501230`
/// * `0x4ea070` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea076` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l434a(host: &mut dyn GlueHost) -> usize {
    // 0x4e9ee0 本函数起点；FDE 0x4e9ee0..0x4ea07c（412 B）
    replay_calls(INNER_INIT_L434A_CALLS, host)
}

pub const INNER_INIT_L434A_CALLS: &[GlueCall] = &[
    // 0x4e9f4d: call 0x460bb0（经槽 0x9baaf0）
    GlueCall { site: 0x4e9f4d, target: 0x460bb0, slot: 0x9baaf0 },
    // 0x4e9fc4: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4e9fc4, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4e9fcf: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4e9fcf, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea00c: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4ea00c, target: 0x925430, slot: 0x9ba678 },
    // 0x4ea014: call 0x4ffcda（直接）
    GlueCall { site: 0x4ea014, target: 0x4ffcda, slot: 0x0 },
    // 0x4ea025: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4ea025, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4ea02d: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4ea02d, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea045: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea045, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea04b: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea04b, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea05b: call 0x501080（直接）
    GlueCall { site: 0x4ea05b, target: 0x501080, slot: 0x0 },
    // 0x4ea062: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea062, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4ea06b: call 0x501230（直接）
    GlueCall { site: 0x4ea06b, target: 0x501230, slot: 0x0 },
    // 0x4ea070: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea070, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea076: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea076, target: 0x9254ab, slot: 0x9ba5b8 },
];

/// `0x4ea080` —— `GraphwarFunctionInput` 的 `_::__init::__inner_init`（Location `466:1`）
///
/// FDE `0x4ea080..0x4ea2c5`（`0x245` = 581 B）；重定位槽 `0x9ba390`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / Location：**观测**（`fixtures/windows_name_to_addr.tsv` 的 `唯一锚`）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L466_CALLS`]）：
/// * `0x4ea14c` `call 0x460bb0`（槽 0x9baaf0）
/// * `0x4ea20d` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4ea218` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea255` `call 0x925430`（槽 0x9ba678）
/// * `0x4ea25d` `call 0x4ffcda`
/// * `0x4ea26e` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4ea276` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea28e` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea294` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea2a4` `call 0x501080`
/// * `0x4ea2ab` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4ea2b4` `call 0x501230`
/// * `0x4ea2b9` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea2bf` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l466(host: &mut dyn GlueHost) -> usize {
    // 0x4ea080 本函数起点；FDE 0x4ea080..0x4ea2c5（581 B）
    replay_calls(INNER_INIT_L466_CALLS, host)
}

pub const INNER_INIT_L466_CALLS: &[GlueCall] = &[
    // 0x4ea14c: call 0x460bb0（经槽 0x9baaf0）
    GlueCall { site: 0x4ea14c, target: 0x460bb0, slot: 0x9baaf0 },
    // 0x4ea20d: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4ea20d, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4ea218: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4ea218, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea255: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4ea255, target: 0x925430, slot: 0x9ba678 },
    // 0x4ea25d: call 0x4ffcda（直接）
    GlueCall { site: 0x4ea25d, target: 0x4ffcda, slot: 0x0 },
    // 0x4ea26e: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4ea26e, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4ea276: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4ea276, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea28e: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea28e, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea294: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea294, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea2a4: call 0x501080（直接）
    GlueCall { site: 0x4ea2a4, target: 0x501080, slot: 0x0 },
    // 0x4ea2ab: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea2ab, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4ea2b4: call 0x501230（直接）
    GlueCall { site: 0x4ea2b4, target: 0x501230, slot: 0x0 },
    // 0x4ea2b9: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea2b9, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea2bf: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea2bf, target: 0x9254ab, slot: 0x9ba5b8 },
];

/// `0x4ea2d0` —— `GraphwarFunctionInput` 的 `_::__init::__inner_init`（Location `434:1`）
///
/// FDE `0x4ea2d0..0x4ea478`（`0x1a8` = 424 B）；重定位槽 `0x9ba380`
/// （该类 4 个 `__init::__inner_init` 落在连续 4 槽里，见本节文档头）。
/// 名字 / 归属：**推断**（4 槽连续 + arm64 每类 4 个同名符号；缺 gdext vtable 槽序对齐）。
///
/// 调用链（顺序 = 指令顺序，逐条见 [`INNER_INIT_L434B_CALLS`]）：
/// * `0x4ea32c` `call 0x8c5120`（槽 0x9baaf8）
/// * `0x4ea365` `call 0x8c5120`（槽 0x9baaf8）
/// * `0x4ea3bf` `call 0x8f5cc0`（槽 0x9ba9c8）
/// * `0x4ea3ca` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea410` `call 0x925430`（槽 0x9ba678）
/// * `0x4ea41f` `call 0x8f5db0`（槽 0x9ba9d8）
/// * `0x4ea427` `call 0x907300`（槽 0x9ba9d0）
/// * `0x4ea441` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea447` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea457` `call 0x501150`
/// * `0x4ea45e` `call 0x9254ab`（槽 0x9ba5b8）
/// * `0x4ea467` `call 0x501230`
/// * `0x4ea46c` `call 0x925623`（槽 0x9ba668）
/// * `0x4ea472` `call 0x9254ab`（槽 0x9ba5b8）
///
/// 档位：**观测**（体：FDE + 槽 + 每条 `call`）。
pub fn inner_init_l434b(host: &mut dyn GlueHost) -> usize {
    // 0x4ea2d0 本函数起点；FDE 0x4ea2d0..0x4ea478（424 B）
    replay_calls(INNER_INIT_L434B_CALLS, host)
}

pub const INNER_INIT_L434B_CALLS: &[GlueCall] = &[
    // 0x4ea32c: call 0x8c5120（经槽 0x9baaf8）
    GlueCall { site: 0x4ea32c, target: 0x8c5120, slot: 0x9baaf8 },
    // 0x4ea365: call 0x8c5120（经槽 0x9baaf8）
    GlueCall { site: 0x4ea365, target: 0x8c5120, slot: 0x9baaf8 },
    // 0x4ea3bf: call 0x8f5cc0（经槽 0x9ba9c8）
    GlueCall { site: 0x4ea3bf, target: 0x8f5cc0, slot: 0x9ba9c8 },
    // 0x4ea3ca: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4ea3ca, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea410: call 0x925430（经槽 0x9ba678）
    GlueCall { site: 0x4ea410, target: 0x925430, slot: 0x9ba678 },
    // 0x4ea41f: call 0x8f5db0（经槽 0x9ba9d8）
    GlueCall { site: 0x4ea41f, target: 0x8f5db0, slot: 0x9ba9d8 },
    // 0x4ea427: call 0x907300（经槽 0x9ba9d0）
    GlueCall { site: 0x4ea427, target: 0x907300, slot: 0x9ba9d0 },
    // 0x4ea441: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea441, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea447: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea447, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea457: call 0x501150（直接）
    GlueCall { site: 0x4ea457, target: 0x501150, slot: 0x0 },
    // 0x4ea45e: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea45e, target: 0x9254ab, slot: 0x9ba5b8 },
    // 0x4ea467: call 0x501230（直接）
    GlueCall { site: 0x4ea467, target: 0x501230, slot: 0x0 },
    // 0x4ea46c: call 0x925623（经槽 0x9ba668）
    GlueCall { site: 0x4ea46c, target: 0x925623, slot: 0x9ba668 },
    // 0x4ea472: call 0x9254ab（经槽 0x9ba5b8）
    GlueCall { site: 0x4ea472, target: 0x9254ab, slot: 0x9ba5b8 },
];



#[cfg(test)]
mod tests {
    use super::*;

    // ---- T80：gdext 宏产物落码的自校验（期望值全部来自反汇编）-----------------

    /// 记录宿主：把收到的调用按顺序存下来。
    struct Recorder(Vec<GlueCall>);
    impl GlueHost for Recorder {
        fn call(&mut self, c: GlueCall) {
            self.0.push(c);
        }
    }

    /// `__register_methods` / `__register_constants` 同形：只落「遍历登记表」。
    #[test]
    fn register_tables_are_run_entry_by_entry() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static HITS: AtomicUsize = AtomicUsize::new(0);
        fn t1() {
            HITS.fetch_add(1, Ordering::SeqCst);
        }
        fn t2() {
            HITS.fetch_add(10, Ordering::SeqCst);
        }
        let table: [fn(); 2] = [t1, t2];
        assert_eq!(run_register_table(&table), 2);
        assert_eq!(HITS.load(Ordering::SeqCst), 11);
        assert_eq!(run_register_table(&[]), 0);
    }

    /// 表槽 / thunk / 重定位槽：每个数都是本节某条指令的 rip 目标。
    #[test]
    fn storage_slots_and_thunk_are_the_disassembly_constants() {
        assert_eq!(REGISTER_THUNK & 0xf, 0, "thunk 表 16 字节对齐");
        assert_eq!(REGISTER_SLOTS.1 - REGISTER_SLOTS.0, 8, "A/B 两个槽相邻");
        assert_eq!(REGISTER_METHODS_TABLE_SLOTS.1 - REGISTER_METHODS_TABLE_SLOTS.0, 8);
        assert_eq!(REGISTER_CONSTANTS_TABLE_SLOTS.1 - REGISTER_CONSTANTS_TABLE_SLOTS.0, 8);
    }

    /// 每条 `__init::__inner_init` 的调用**普查**条数（从各自指令流数出来的）。
    #[test]
    fn inner_init_call_census_matches_the_disassembly() {
        assert_eq!(INNER_INIT_L434A_CALLS.len(), 14, "l434a 的 call 普查条数");
        assert_eq!(INNER_INIT_L466_CALLS.len(), 14, "l466 的 call 普查条数");
        assert_eq!(INNER_INIT_L434B_CALLS.len(), 14, "l434b 的 call 普查条数");
    }

    /// 普查里的每条调用点必须落在**该函数自己的 FDE** 内。
    #[test]
    fn inner_init_call_sites_lie_inside_their_own_fde() {
        for c in INNER_INIT_L434A_CALLS {
            assert!((0x4e9ee0..0x4ea07c).contains(&c.site), "l434a: {:#x} 不在 FDE", c.site);
        }
        for c in INNER_INIT_L466_CALLS {
            assert!((0x4ea080..0x4ea2c5).contains(&c.site), "l466: {:#x} 不在 FDE", c.site);
        }
        for c in INNER_INIT_L434B_CALLS {
            assert!((0x4ea2d0..0x4ea478).contains(&c.site), "l434b: {:#x} 不在 FDE", c.site);
        }
    }

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

    // ---- token 删除（T80：`0x4f44c0` / `0x4f4a60` 的立即数表）-------------

    /// 期望值**逐条来自反汇编立即数**（不是照抄我方实现）：
    /// `0x4f462a mov $0x74727173`+`xor byte[r12+4],0x28` = `sqrt(`；
    /// `0x4f46a2..0x4f46d4` 六个 `cmp dword`；`0x4f4753..0x4f4801` 七组 `xor word`+`xor byte`；
    /// `0x4f4870..0x4f48ad` 七个 `cmp word`。
    #[test]
    fn at_caret_tables_are_the_disassembly_immediates() {
        assert_eq!(
            DELETE_TOKEN_TOKENS_AT_CARET,
            [
                (5, &["sqrt("][..]),
                (4, &["abs(", "sin(", "cos(", "tan(", "exp(", "sqrt"][..]),
                (3, &["ln(", "abs", "sin", "cos", "tan", "exp", "sqr"][..]),
                (2, &["ln", "ab", "si", "co", "ta", "ex", "sq"][..]),
            ]
        );
    }

    /// `0x4f4ca7 cmp dword,0x28747271` = `qrt(`（4 档）；`0x4f4eed cmp word,0x286e` = `n(`（2 档）。
    #[test]
    fn after_caret_tables_carry_the_half_token_suffixes() {
        assert_eq!(
            DELETE_TOKEN_TOKENS_AFTER_CARET,
            [
                (5, &["sqrt("][..]),
                (
                    4,
                    &["abs(", "sin(", "cos(", "tan(", "exp(", "sqrt", "qrt("][..]
                ),
                (3, &["ln(", "qrt", "bs(", "in(", "os(", "an(", "xp(", "rt("][..]),
                (
                    2,
                    &["n(", "ln", "bs", "in", "os", "an", "xp", "rt", "s(", "p(", "t("][..]
                ),
            ]
        );
    }

    /// 每一档的 token 都真的能命中；末尾的 caret 在末尾 ⇒ 按"最长档"命中。
    #[test]
    fn every_at_caret_token_is_recognised_with_its_own_length() {
        for (n, tokens) in DELETE_TOKEN_TOKENS_AT_CARET.iter() {
            for t in tokens.iter() {
                let text = format!("x+{t}");
                assert_eq!(
                    token_len_at_caret(&text, text.chars().count()),
                    *n,
                    "token {t:?} 应删 {n} 个"
                );
            }
        }
    }

    /// 默认臂：`0x4f48c7 mov esi,1` —— 表外的字符只删 1 个。
    #[test]
    fn at_caret_default_arm_deletes_one() {
        assert_eq!(token_len_at_caret("x+y", 3), 1);
        assert_eq!(token_len_at_caret("x+z", 3), 1);
        assert_eq!(token_len_at_caret("", 0), 1, "原版由 0x4f4410 吸收空删");
        // 长度不够就落到更短的档：caret=3 时 "sin" 无法匹配（左边只有 3 个字符 ⇒ 恰好命中 3 档）
        assert_eq!(token_len_at_caret("sin", 3), 3);
        assert_eq!(token_len_at_caret("in", 2), 1, "「in」不在 2 档表里（只有 in( 在 3 档）");
    }

    /// 端到端：`Buffer` 上的删除效果（`0x4f4410` 的 N 次退格）。
    #[test]
    fn buffer_deletes_whole_tokens_leftwards() {
        let mut b = Buffer::with_caret_at_end("x+sin(");
        assert_eq!(b.delete_token_at_caret(), 4);
        assert_eq!(b.text(), "x+");
        let mut b = Buffer::with_caret_at_end("x+sqrt");
        assert_eq!(b.delete_token_at_caret(), 4);
        assert_eq!(b.text(), "x+");
        let mut b = Buffer::with_caret_at_end("x+sqrt(");
        assert_eq!(b.delete_token_at_caret(), 5);
        assert_eq!(b.text(), "x+");
        let mut b = Buffer::with_caret_at_end("x+sq");
        assert_eq!(b.delete_token_at_caret(), 2);
        assert_eq!(b.text(), "x+");
        let mut b = Buffer::with_caret_at_end("x+q");
        assert_eq!(b.delete_token_at_caret(), 1);
        assert_eq!(b.text(), "x+");
    }

    /// 右删：候选是 caret **右侧** n 个字符；命中后 caret 先右移 n 再退格（`0x4f4fcf`/`0x4f4fee`）。
    #[test]
    fn buffer_deletes_whole_tokens_rightwards() {
        // caret 在 "s|qrt(" ⇒ 右侧 4 个字符 "qrt(" ⇒ 删 4
        let mut b = Buffer::new("sqrt(x)");
        b.move_caret_right(1);
        assert_eq!(b.delete_token_after_caret(), 4);
        assert_eq!(b.text(), "sx)");
        // caret 在 "sq|rt(" ⇒ 右侧 3 个字符 "rt(" ⇒ 删 3
        let mut b = Buffer::new("sqrt(x)");
        b.move_caret_right(2);
        assert_eq!(b.delete_token_after_caret(), 3);
        assert_eq!(b.text(), "sqx)");
        // caret 在 "s|in(" ⇒ 右侧 3 个字符 "in(" ⇒ 删 3
        let mut b = Buffer::new("sin(x)");
        b.move_caret_right(1);
        assert_eq!(b.delete_token_after_caret(), 3);
        assert_eq!(b.text(), "sx)");
        // 表外字符 ⇒ 删 1
        let mut b = Buffer::new("q+z");
        assert_eq!(b.delete_token_after_caret(), 1);
        assert_eq!(b.text(), "+z");
    }

    /// caret 已在末尾 ⇒ 原版在 `0x4f4f97 cmp rbx,r14 ; jbe` 直接结束，什么都不删。
    #[test]
    fn after_caret_at_end_is_a_noop() {
        let mut b = Buffer::with_caret_at_end("x+sin(");
        assert_eq!(b.delete_token_after_caret(), 0);
        assert_eq!(b.text(), "x+sin(");
    }
}


// ===========================================================================
// 原版方法体：原版 `GraphwarFunctionInput` 的 gdext 节点类（`LineEdit` 子类）（T57 从 `src/graphwar_function_input.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `GraphwarFunctionInput` 的 gdext 节点类（`LineEdit` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/graphwar_function_input.rs` 搬进 `src/graphwar_function_input.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `graphwar_function_input.rs` 里**，证据：
//! `panic::Location` 证据：`ready 0x4fd1b0` → `src/graphwar_function_input.rs:508:43`、`process 0x4fd4a0` → `:521:133`、`force_update_cb 0x4f2200` → `:327:133`；`gui_input 0x4fedc0`、`move_caret_* 0x4f3210/0x4f3910`、`delete_token_* 0x4f44c0/0x4f4a60` 见 `fixtures/name_to_addr_v2.tsv`。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::graphwar_function_input::<类名>` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarFunctionInput` 是 **`LineEdit` 的 Rust 子类**（`ILineEdit` + `GodotClass` +
//! `ImplementsGodotApi` + `ImplementsGodotVirtual`）：**它不是 `.tscn` 里的 LineEdit 节点**，
//! 它自己就是输入框，文字状态在 Rust 侧（`docs/06-verification.md` §15.3 的运行时观测）。
//! `src/graphwar_function_input.rs` 已经把那 17 个函数里**可归属**的部分复原出来了 ——
//! 尤其是"输入框只做 `text.replace(' ',"")`"这一条 —— 但它按约定**不碰引擎**。
//! 本文件是那一层的薄壳：
//!
//! ```text
//! src/graphwar_function_input.rs            纯逻辑：strip_spaces / COMPOSED_FOLDS / intent_for_key
//! src/graphwar_function_input.rs   ← 本文件：connect 三条原生信号、插字、发信号
//! ```
//!
//! 预处理与折叠表**只有一份**，仍然在纯逻辑文件里；本文件不复制任何字符表。
//!
//! # 本轮要验的三种机制（结论见 `docs/16-client.md` §35）
//!
//! | # | 机制 | 原版形态 | 本文件怎么用 |
//! |---|---|---|---|
//! | 1 | **自造 `#[signal]`** | 无 | `#[signal] expression_changed(expression)` / `function_submitted(text)`，用**裸** `emit_signal` 发（与 `GraphwarButton` 的类型安全生成器对照） |
//! | 2 | **连引擎原生信号** | `ready` 里 3 条 `connect`（[`graphwar_function_input::SIGNAL_WIRING`]） | `size_changed`/`text_changed`/`text_submitted` → `on_size_changed`/`on_text_change`/`on_text_submitted` |
//! | 3 | **外部注册的闭包表** | **单个** `dyn` 回调（`[self+0x68]`，`0x4f425c`），按名字调 `"fire"`（`0x4f4269`） | 字段写 `Option<Box<dyn FnMut(&str)>>`；另存 `Vec<Callable>` 给 GDScript |
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 / trait | `LineEdit` + `ILineEdit` | 同 | **观测** |
//! | `ready` 的 3 条连接 | `size_changed`/`text_changed`/`text_submitted` → 三个 `#[func]` | 同（名字逐字取自 [`graphwar_function_input::SIGNAL_WIRING`]） | **观测** |
//! | 文本预处理 | `force_update_cb` 里 `str::replace(' ', "")` | 调 [`graphwar_function_input::strip_spaces`]（同一份） | **观测** |
//! | `pi`/`π` → `n`、小写化 | **不在输入框**，在 `MathFunction::new`（解析器） | **本类不做**（也不该做） | **观测** |
//! | `gui_input` 的 28 条折叠 | 跳转表 `0x1c3d5c`，198 项里 28 项不落默认 | 调 [`graphwar_function_input::intent_for_key`]（同一张表） | **观测** |
//! | `'^'` 的附加条件 | `0x4feeeb`–`0x4fef06` 三次读引擎属性，**语义未知** | `#[export] caret_condition : bool` 显式暴露，默认 `false` | 条件**未知**（缺那三次读的语义） |
//! | token 边界（`delete_token_*` 的 N） | 四张立即数表（5/4/3/2 字符），见 `DELETE_TOKEN_TOKENS_*` | 调 `token_len_at_caret` / `token_len_after_caret`（同一份表） | **观测** |
//! | `process` 那 6.4 KB | 两次 `call("force_update_cb")` + 与 `graphwar_game_client` 交互 | 只做"每帧 `force_update_cb`"，game_client 那半**没接** | 交互**未做** |
//! | 提交路径 | `[+0x68]` 的 dyn 回调 `"fire"`，文本**原样** | 同（`Option<Box<dyn FnMut(&str)>>`），文本原样 | **观测** |
//!
//! # 怎么打开 / 契约
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/signal_classes_check.gd
//! ```
//!
//! ```text
//! GraphwarFunctionInput : LineEdit
//!   @export caret_condition : bool           # '^' 那个未知条件的替身
//!
//!   signal expression_changed(expression: String)   # 自造：去空格之后
//!   signal function_submitted(text: String)         # 自造：提交（文本原样）
//!
//!   on_text_change(new_text) / on_text_submitted(text) / on_size_changed()
//!   force_update_cb() -> String               # 读 get_text() → 去空格 → 变了就发信号
//!   expression() -> String                    # 最近一次去空格的结果
//!   simulate_key(unicode: int, caret_condition: bool) -> String   # 28 条折叠的可测入口
//!   subscribe(cb: Callable) -> int            # 替代方案
//!   subscribe_native_probe() -> bool          # 原版形态：设那个单 dyn 回调
//!   gui_input_call_count() / text_change_count() / submit_count() -> int
//!   size_changed_count() / ready_call_count() / process_call_count() -> int
//!   expression_emit_count() / submit_emit_count() -> int
//!   fire_captured() / callable_captured() / fire_log() -> String
//!   insert_count() -> int
//! };
//! ```
//!
//! # 必须知道的三条（本轮实测）
//!
//! 1. **`--script` 里没有真实按键** ⇒ `gui_input` 不会被引擎调到。可测的是
//!    [`GraphwarFunctionInput::simulate_key`]（走**同一个** `apply_key`）。
//! 2. **信号名与处理函数名不同名**（`text_changed` → `on_text_change`），所以这里的
//!    `connect` 必须给两个不同的名字 —— 与 `GraphwarButton` 那条"同名"正好对照。
//! 3. **`Callable` 不能当 `#[export]` 属性**：gdext 的 `impl_property_by_godot_convert!`
//!    对 `Callable`/`Signal`/`Rid` 只生成 `SimpleVar`、**不生成 `Export`**
//!    （`godot-core-0.5.5/src/registry/property/mod.rs` 的 `no_export` 分支）；
//!    想把它暴露给编辑器只能加 `#[var]` 而不是 `#[export]`。见 `docs/16-client.md` §35.6。



use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::{ILineEdit, InputEvent, InputEventKey, LineEdit};
use godot::prelude::*;

use crate::graphwar_function_input::{self as finput};

/// 最多留多少条派发/提交流水。
const LOG_LIMIT: usize = 32;

/// 函数输入框。
///
/// 文字状态在**引擎**（`LineEdit` 自己的 `text`），Rust 侧只留"去空格之后的结果"
/// [`GraphwarFunctionInput::expression`] —— 这正是原版的分工：`force_update_cb`
/// 读引擎文本、去掉空格、再把结果送去算路径（`0x4f28be` 的 `str::replace`）。
#[derive(GodotClass)]
#[class(base = LineEdit)]
pub struct GraphwarFunctionInput {
    base: Base<LineEdit>,

    /// `'^'` 那个**未知**附加条件（`0x4fee94`/`0x4feea5`/`0x4feee3` 三次引擎属性读取）的替身。
    ///
    /// 默认 `false` = "按未知条件不成立处理"；这个默认值**不是**从原版读出来的。
    #[export]
    caret_condition: bool,

    /// 最近一次去空格之后的表达式（原版 `force_update_cb` 的中间结果）。
    expression: GString,

    /// **机制 3 的原版形态**：`[self+0x68]` 是**一个** dyn 回调（`mov [rdi+0x68],%rdi`）。
    ///
    /// 原版调它时按名字传 `"fire"`（`lea "fire"(4)` + `call *0x20(%rax)`）—— 名字派发是
    /// gdext 弱引用 trait 对象的形态；本文件只保留"一个可换的回调"这一条语义。
    fire_native: Option<Box<dyn FnMut(&str)>>,

    /// 探针闭包写到这儿（闭包不能借 `self`，见 `graphwar_button.rs` §坑 3）。
    native_capture: Rc<RefCell<Vec<String>>>,

    /// **机制 3 的替代方案**：GDScript 只能给 `Callable`，而且允许登记多个（我方选择）。
    fire_callables: Vec<Callable>,

    /// `Callable` 的到货记录。
    callable_capture: Rc<RefCell<Vec<String>>>,

    ready_calls: i64,
    process_calls: i64,
    text_change_calls: i64,
    submit_calls: i64,
    size_changed_calls: i64,
    force_update_calls: i64,
    gui_input_calls: i64,
    insert_count: i64,
    expression_emits: i64,
    submit_emits: i64,
    emit_errors: Vec<String>,
    /// `_ready` 里连不上的信号（`connect` 的返回码）。原版第 3 条 `size_changed` 会落这里。
    connect_errors: Vec<String>,
    /// `_ready` 里真连上几条（期望 2 —— 见 [`GraphwarFunctionInput::connect_errors`]）。
    wired: i64,
}

#[godot_api]
impl ILineEdit for GraphwarFunctionInput {
    fn init(base: Base<LineEdit>) -> Self {
        Self {
            base,
            caret_condition: false,
            expression: GString::new(),
            fire_native: None,
            native_capture: Rc::new(RefCell::new(Vec::new())),
            fire_callables: Vec::new(),
            callable_capture: Rc::new(RefCell::new(Vec::new())),
            ready_calls: 0,
            process_calls: 0,
            text_change_calls: 0,
            submit_calls: 0,
            size_changed_calls: 0,
            force_update_calls: 0,
            gui_input_calls: 0,
            insert_count: 0,
            expression_emits: 0,
            submit_emits: 0,
            emit_errors: Vec::new(),
            connect_errors: Vec::new(),
            wired: 0,
        }
    }

    /// `_ready`（原版 `0x4fd1b0`）：**机制 2** —— 三条 `LineEdit` 原生信号接回自己的方法。
    ///
    /// 原版三段的结构一样（`Callable::from_object_method(self, handler)` +
    /// `StringName::from(signal)` + `connect`），指令地址见
    /// [`graphwar_function_input::SIGNAL_WIRING`] 的文档。
    fn ready(&mut self) {
        self.ready_calls += 1;

        let myself = self.to_gd();
        for (signal, handler) in finput::SIGNAL_WIRING {
            let callable = myself.callable(handler);
            let err = self.base_mut().connect(signal, &callable);
            if err == godot::global::Error::OK {
                self.wired += 1;
            } else {
                // ⚠️ **实测**：`size_changed` 在 Godot 4.6/4.7 的 `Control`/`LineEdit` 上
                // **不是信号**（只有 `resized`）⇒ 这一条必然 `ERR_INVALID_PARAMETER`。
                // 原版的字面量确实是 `size_changed`（`.rodata 0x1c46d2`，12 字节，逐字节核过），
                // 所以这是**原版自己的**问题，不是本文件的笔误。见 `docs/16-client.md` §35.5。
                godot_warn!("GraphwarFunctionInput::ready 连接 {signal} 失败: {err:?}");
                self.connect_errors.push(format!("{signal}: {err:?}"));
            }
        }
    }

    /// `_process`：原版 `0x4fd4a0`（6.4 KB）里**已确认**的那部分 —— 两次
    /// `call("force_update_cb")`。与 `game_client` 的数据流（`docs/16` §17 的未知项）**没接**。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;
        self.force_update_cb();
    }

    /// 原版 `gui_input`（`0x4fedc0`，**推断**归属）：只处理 `InputEventKey` 的 `unicode`，
    /// 28 条死键/上标折回 ASCII，`'^'` 单独先判。
    ///
    /// 判定本体是 [`finput::intent_for_key`]；这里只负责"从引擎事件里拿 `unicode`"
    /// 与"把结论落到 `LineEdit` 上"。
    fn gui_input(&mut self, event: Gd<InputEvent>) {
        self.gui_input_calls += 1;

        // `try_cast` 消费 `event`，所以先判类型再取字段。
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() {
            return;
        }
        let Some(ch) = char::from_u32(key.get_unicode()) else {
            // `char::from_u32` 失败 = 代理区码位（原版 `0x4fef2b`–`0x4fef3d` 直接返回）。
            return;
        };
        self.apply_key(ch);
    }
}

#[godot_api]
impl GraphwarFunctionInput {
    /// **机制 1**：自造的、带参数信号（去空格之后的表达式）。
    #[signal]
    fn expression_changed(expression: GString);

    /// **机制 1**：自造的、带参数信号（提交的文本，**原样**）。
    #[signal]
    fn function_submitted(text: GString);

    /// 原版 `on_text_change`（导出方法，`0x4c42ff` 那个名字）：文本变了 ⇒ 重新去空格。
    ///
    /// 参数是 `text_changed(new_text)` 的那一个 `String` —— `Callable::from_object_method`
    /// 会按注册的元数（1）传参，多一个少一个都是错。
    #[func]
    fn on_text_change(&mut self, new_text: GString) {
        self.text_change_calls += 1;
        self.recompute_expression(new_text.to_string());
    }

    /// 原版 `on_size_changed`（导出方法）：`size_changed` 没有参数。
    #[func]
    fn on_size_changed(&mut self) {
        self.size_changed_calls += 1;
    }

    /// 原版 `on_text_submitted`（导出方法，`0x427070`）：把文本**原样**交给 `"fire"` 回调。
    ///
    /// 原版这条路上**没有任何字符串变换**（`docs/16` §17：`0x4f4250` 只做
    /// `[rdi+0x68]` 取值 + `call *0x20`）⇒ 传 `" s in ( x ) "` 就收到带空格的原文。
    #[func]
    fn on_text_submitted(&mut self, new_text: GString) {
        self.submit_calls += 1;
        let raw = new_text.to_string();
        let (method, text) = finput::submit_intent(&raw);
        debug_assert_eq!(method, "fire");

        // 机制 3 原版形态：一个 dyn 回调。
        if let Some(callback) = self.fire_native.as_mut() {
            callback(text);
        }
        // 机制 3 替代方案：任意多个 Callable。
        let arg = text.to_variant();
        for callable in self.fire_callables.iter() {
            callable.call(&[arg.clone()]);
        }

        self.submit_emits += 1;
        let payload = new_text.clone();
        let err = self
            .base_mut()
            .emit_signal("function_submitted", &[payload.to_variant()]);
        if err != godot::global::Error::OK {
            self.emit_errors.push(format!("function_submitted: {err:?}"));
        }
    }

    /// 原版 `force_update_cb`（导出方法，`0x4f2200`）：读引擎文本 → **去空格** → 变了就发信号。
    ///
    /// 返回去空格之后的结果。原版这一步之后还"送路径计算 + 与 game_client 对账"
    /// （`docs/16` §17 的角色描述），那两件**没接**。
    #[func]
    fn force_update_cb(&mut self) -> GString {
        self.force_update_calls += 1;
        let text = self.base().get_text().to_string();
        self.recompute_expression(text);
        self.expression.clone()
    }

    /// 最近一次去空格之后的表达式。
    #[func]
    fn expression(&self) -> GString {
        self.expression.clone()
    }

    /// **28 条折叠的可测入口**：走与 `gui_input` **同一个** `apply_key`，返回处理之后的文本。
    ///
    /// 返回 `""` 表示"没接管"（原版的默认分支，交给基类 `LineEdit`）。
    #[func]
    fn simulate_key(&mut self, unicode: i64, caret_condition: bool) -> GString {
        self.caret_condition = caret_condition;
        let Some(ch) = char::from_u32(unicode.clamp(0, 0x10ffff) as u32) else {
            return GString::new();
        };
        if self.apply_key(ch) {
            self.base().get_text()
        } else {
            GString::new()
        }
    }

    /// **机制 3 的替代方案**：登记一个 `Callable`，返回现在有几个。
    #[func]
    fn subscribe(&mut self, cb: Callable) -> i64 {
        if cb.is_valid() {
            self.fire_callables.push(cb);
        }
        self.fire_callables.len() as i64
    }

    /// **机制 3 的原版形态**：设那个**单个** dyn 回调（原版 `[+0x68]` 就只有一个）。
    #[func]
    fn subscribe_native_probe(&mut self) -> bool {
        let capture = self.native_capture.clone();
        self.fire_native = Some(Box::new(move |text: &str| {
            capture.borrow_mut().push(text.to_owned());
        }));
        self.fire_native.is_some()
    }

    /// `_ready` 里真连上了几条信号（原版写的是 3 条，实测只有 2 条能连上）。
    #[func]
    fn wired_count(&self) -> i64 {
        self.wired
    }

    /// `_ready` 里连不上的信号（`信号名: 错误码`，`|` 分隔）。
    ///
    /// 非空就是"原版那条 connect 在本机 Godot 上连不上"的证据。
    #[func]
    fn connect_errors(&self) -> GString {
        GString::from(join_text(&self.connect_errors).as_str())
    }

    /// `gui_input` 被调到几次（`--script` 模式下大概率是 0，见文件头 §3 第 1 条）。
    #[func]
    fn gui_input_call_count(&self) -> i64 {
        self.gui_input_calls
    }

    /// `on_text_change` 被调到几次。
    #[func]
    fn text_change_count(&self) -> i64 {
        self.text_change_calls
    }

    /// `on_text_submitted` 被调到几次。
    #[func]
    fn submit_count(&self) -> i64 {
        self.submit_calls
    }

    /// `on_size_changed` 被调到几次。
    #[func]
    fn size_changed_count(&self) -> i64 {
        self.size_changed_calls
    }

    /// `_ready` 被调到几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `_process` 被调到几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// `force_update_cb` 被调到几次。
    #[func]
    fn force_update_count(&self) -> i64 {
        self.force_update_calls
    }

    /// 往 `LineEdit` 里插过几次字（`FN_INSERT_TEXT` 的落点）。
    #[func]
    fn insert_count(&self) -> i64 {
        self.insert_count
    }

    /// `expression_changed` 发了几次。
    #[func]
    fn expression_emit_count(&self) -> i64 {
        self.expression_emits
    }

    /// `function_submitted` 发了几次。
    #[func]
    fn submit_emit_count(&self) -> i64 {
        self.submit_emits
    }

    /// 那个单 dyn 回调收到过的文本（`|` 分隔）。
    #[func]
    fn fire_captured(&self) -> GString {
        GString::from(join_text(&self.native_capture.borrow()).as_str())
    }

    /// `Callable` 收到过的文本（`|` 分隔）。
    #[func]
    fn callable_captured(&self) -> GString {
        GString::from(join_text(&self.callable_capture.borrow()).as_str())
    }

    /// emit 失败的记录（`|` 分隔；空 = 一次都没失败）。
    #[func]
    fn emit_errors(&self) -> GString {
        GString::from(join_text(&self.emit_errors).as_str())
    }

    // -----------------------------------------------------------------------
    // 内部
    // -----------------------------------------------------------------------

    /// 去空格并把结果写进 `self.expression`；**变了才**发信号（原版 `force_update_cb` 的语义）。
    fn recompute_expression(&mut self, raw: String) {
        let stripped = finput::strip_spaces(&raw);
        if self.expression.to_string() == stripped {
            return;
        }
        self.expression = GString::from(stripped.as_str());
        self.expression_emits += 1;
        // 机制 1：这条走**裸** `emit_signal`（与 GraphwarButton 的类型安全生成器对照）。
        let payload = self.expression.clone();
        let err = self
            .base_mut()
            .emit_signal("expression_changed", &[payload.to_variant()]);
        if err != godot::global::Error::OK {
            self.emit_errors.push(format!("expression_changed: {err:?}"));
        }
    }

    /// 一次按键的落点：`InsertAndRefresh` ⇒ 往光标处插字 + `force_update_cb`。
    ///
    /// 返回 `true` = 本类接管了（原版插完字就返回，不再交给基类）。
    fn apply_key(&mut self, ch: char) -> bool {
        match finput::intent_for_key(ch, self.caret_condition) {
            finput::InputIntent::Delegate => false,
            finput::InputIntent::InsertAndRefresh(text) => {
                // 原版是 `format!("{}", ch)` 成 `String` 再调 `FN_INSERT_TEXT`（`0x4f4320`）。
                let payload = GString::from(text.as_str());
                self.base_mut().insert_text_at_caret(&payload);
                self.insert_count += 1;
                // 插入之后 `call("force_update_cb")`（`REFRESH_METHOD`）。
                self.force_update_cb();
                true
            }
        }
    }
}

/// 把一串字符串接成 `a|b|c`。
///
/// **返回 `String` 而不是 `GString`**：`GString::from` 要调引擎的构造函数，没有 Godot 进程
/// 就 panic，所以纯 Rust 的那一半留在 `String` 上，到 `#[func]` 边界才转。
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

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `ready` 接的是原版那三对，而且**信号名与处理函数名不同名**（与 GraphwarButton 对照）。
    #[test]
    fn ready_wiring_pairs_are_the_original_three() {
        assert_eq!(
            finput::SIGNAL_WIRING,
            [
                ("size_changed", "on_size_changed"),
                ("text_changed", "on_text_change"),
                ("text_submitted", "on_text_submitted"),
            ]
        );
        for (signal, handler) in finput::SIGNAL_WIRING {
            assert_ne!(signal, handler, "这一组本来就是两个不同的名字");
            assert_eq!(finput::handler_for_signal(signal), Some(handler));
            assert_eq!(finput::signal_for_handler(handler), Some(signal));
        }
    }

    /// `LOG_LIMIT` 与"去空格是唯一变换"这两件事在纯逻辑里都成立（本类不另立规则）。
    #[test]
    fn the_only_preprocessing_is_space_stripping() {
        assert_eq!(finput::strip_spaces("sin ( x )"), "sin(x)");
        // 不转 π、不小写、不动其它空白 —— 那都在解析器里。
        assert_eq!(finput::strip_spaces("PI + π"), "PI+π");
        assert!(LOG_LIMIT > 0);
    }

    /// `'^'` 走的是那个未知条件参数：`true` 接管、`false` 交回基类。
    #[test]
    fn caret_key_depends_on_the_unknown_condition_parameter() {
        assert_eq!(
            finput::intent_for_key('^', true),
            finput::InputIntent::InsertAndRefresh("^".into())
        );
        assert_eq!(finput::intent_for_key('^', false), finput::InputIntent::Delegate);
    }

    /// 折叠表的两端：`â` 折成 `a`、ASCII 一律不接管。
    #[test]
    fn composed_keys_fold_and_ascii_is_delegated() {
        assert_eq!(
            finput::intent_for_key('\u{00e2}', false),
            finput::InputIntent::InsertAndRefresh("a".into())
        );
        assert_eq!(finput::intent_for_key('x', true), finput::InputIntent::Delegate);
        assert_eq!(finput::fold_composed('π'), None, "π 的规范化在解析器");
    }

    /// 提交路径不改写文本（本类的 `on_text_submitted` 就是照这一条写的）。
    #[test]
    fn submit_intent_passes_text_verbatim() {
        assert_eq!(finput::submit_intent(" s in ( x ) "), ("fire", " s in ( x ) "));
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
