//! `function_history_recorder.rs` —— **函数历史记录器**（原版 `FunctionHistoryRecorder`）。
//!
//! # 它是什么
//!
//! 订阅 **`function_info_event`**（判别值 **35** = `0x8000_0000_0000_0023`），每收到一次就把
//! **"谁 + 什么函数 + 什么颜色"** 追加到自己的 `Vec<FunctionData>` 里。
//! 就这么多：本模块**只有 3 个函数**（`new` / `handle_function_fire_event` /
//! 订阅闭包），容量、去重、上下键翻历史**都不在这里**。
//!
//! > ⚠️ **2026-11-09（任务 77）口径裁决（G5）**：本文此前写"订阅 `function_fire_event`"，
//! > 常量也叫 `EVENT_FIRE_TAG`。**这是错的**：`docs/07` §7.2 的 72 条表里
//! > **33 = `function_fire_event`、35 = `function_info_event`**，而 x86 字节证据两边都指向 **35**：
//! >
//! > | 证据 | x86 指令 | 结论 |
//! > |---|---|---|
//! > | 订阅名 | `0x3f1141 lea rsi,[0x1bfccd]` + `0x3f114f mov edx,0x13`（19 字节） | 串 `0x1bfccd` = **`function_info_event`**；该串在全 `.text` **只有这一个引用点**（`addr_probe.py callers 0x1bfccd` = 1） |
//! > | 处理器判别闸 | `0x3f0e4e movabs r12,0x7fff…ffff` + `0x3f0e58 lea rax,[r12+0x24]` + `0x3f0e5d cmp [rsi],rax` | `isize::MAX + 0x24` = `0x8000_0000_0000_0023` = **35** |
//! > | 反证（33 没有订阅者） | `function_fire_event` 串 `0x1bf607` 的引用点只有 2 个（`0x3d53d4` in `0x3d5260`、`0x3dca26` in `0x3dc7f0`），都不在 `subscribe_to_event` 的调用形态里；tag 33 的 materialize 点只有 3 处（`0x3cab13`/`0x3caba7`/`0x470f3a`），全在 game_state_manager 的邻域 | 客户端 22 个 manager 里**没有**任何槽订阅 33 |
//! >
//! > ⇒ **裁决：本记录器订阅的是 `function_info_event`(35)。** 常量改为
//! > [`EVENT_FUNCTION_INFO_TAG`]（值不变，仍是 35）；[`EVENT_FIRE_TAG`] 保留为**旧名别名**
//! > （`tests/last_five.rs` 仍在用，本分支不许改 `tests/`），并在文档里标"旧名，语义错"。
//! > `FunctionData.color` 的"颜色"是**推断**（见 §颜色），但**事件名不是推断**。
//!
//! # 3 个函数（arm64 2.0.60 未 strip，真名 + 真边界）
//!
//! | 函数 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 判据 |
//! |---|---|---|---|---|---|
//! | `new` | `0x49d9a0` | 312 | **`0x3f10a0`** | 316 | 唯一订阅 `function_info_event` + 唯一 `subscribe` 调用 + 被 `0x517690` 的 `0x517e27` 调用；`alloc 0x48` = 16+8+48 |
//! | `handle_function_fire_event` | `0x49d71c` | 596 | **`0x3f0e40`** | 566 | `panic::Location` 3 个键 + 被调者（`String::clone` + `RawVec::grow_one` + 模块容器取值）；比值 0.81 |
//! | `subscribe_to_events_event_handler::{{closure}}` | `0x49d970` | 48 | **`0x3f1080`** | 32 | 闭包槽 `0x955050` 的 `+0x20` = `0x3f1080`，体内唯一 `lea rsi` 指 `0x3f0e40` |
//!
//! 另有 `drop_in_place<…>` 若干（含 `Vec<FunctionData>` 的 124 B）—— 那是编译器产物，不是源级方法。
//!
//! # x86 `0x3f0e40` 全解（**观测**，任务 77 逐条复核）
//!
//! ```text
//! 0x3f0e4e movabs r12, 0x7fffffffffffffff
//! 0x3f0e58 lea    rax, [r12 + 0x24]              ; = 0x8000_0000_0000_0023 (35)
//! 0x3f0e5d cmp    qword ptr [rsi], rax
//! 0x3f0e60 jne    0x3f0fee                        ; 不是 35 ⇒ panic!(…:78:18, Location 0x955038)
//! 0x3f0e69 lea    rax, [r12 + 1]                  ; = 0x8000_0000_0000_0000
//! 0x3f0e6e cmp    qword ptr [rsi + 0x20], rax     ; Option<String> 的 None niche ⇒ 直接 return
//! 0x3f0e7b mov    ebp, dword ptr [r15 + 0x38]     ; player id
//! 0x3f0e7f add    rdi, 0x28 ; call 0x46e580       ; 容器 Weak 升级 + 取模块（Location 在 module_container.rs:30:28）
//! 0x3f0eb1 cmp    qword ptr [rbx + 0x48], 0       ; 模块 +0x18 那张表的 items
//! 0x3f0ebc lea    rdi, [rbx + 0x50]               ; hasher = 表基 + 0x20
//! 0x3f0ec8 call   0x47dec0                        ; hash_one(&player_id)
//! 0x3f0f58 mov    eax, dword ptr [rcx + r8*8 - 4] ; 取 4 字节 value（= FunctionData.color）
//! 0x3f0f66 call   [0x9ba8f0] -> 0x91e260          ; String::clone（事件 +0x20）
//! 0x3f0f83..0x3f0fc2                              ; Vec::push 一条 32 B 记录
//! ```
//!
//! 三个 `panic::Location` 就是这三处失败路径的坐标（[`LOCATIONS`]），本轮逐条复核：
//! `0x3f0ff5`→`0x955038`（`:78:18`，tag 闸）、`0x3f1007`→`0x955008`（`:66:123`）、
//! `0x3f1014`→`0x955020`（`:67:62`）。
//!
//! ## 48 B 布局（任务 77，逐位）
//!
//! ```text
//! new 0x3f10a0: alloc 0x48 = RcInner 16 + RefCell 8 + 48
//! 0x3f10df mov [rax+0x18], 0        ; M+0x00 = Vec cap  = 0
//! 0x3f10e7 mov [rax+0x20], 8        ; M+0x08 = Vec ptr  = 8（`Vec::new()` 的 dangling = align_of::<FunctionData>()）
//! 0x3f10ef mov [rax+0x28], 0        ; M+0x10 = Vec len  = 0
//! 0x3f10f7 mov [rax+0x30], rax      ; M+0x18 = 弱自句柄（盒基）
//! 0x3f10fb mov [rax+0x38], r12      ; M+0x20 = 事件通道 Rc（arg2）
//! 0x3f10ff mov [rax+0x40], r15      ; M+0x28 = 容器 Weak（arg3）
//! 0x3f110f mov [rax+0x10], -1       ; RefCell 借用字 = 写借；0x3f1160 inc 回 0
//! ⇒ 0x00..0x18 = `Vec`（cap/ptr/len 三字，顺序由 repr(Rust) 重排）、0x18 弱自句柄、
//!   0x20 事件通道、0x28 容器 Weak，共 0x30 = 48
//! ```
//!
//! ## 颜色（`FunctionData.color`）：**推断**仍是推断
//!
//! 处理器把 `rbx+0x30` 当 ctrl、`+0x38` 当 mask、`+0x48` 当 items、`+0x50` 当 hasher 查一张
//! `u32 → 4 字节` 的表。`rbx` 是容器里取出来的**某个模块**的盒基，所以那张表是该模块的
//! `+0x18`（= 盒 + 0x30）。`graphwar_color_manager` 的占用表正好在 `+0x18`、hasher 在 `+0x38`，
//! 且 value 是 3 字节 `GraphwarColor`（读 4 字节 dword）⇒ **最可能**是玩家颜色；
//! 但 `graphwar_rank_manager` 的表也在 `+0x18`（value 是 `u32` 段位）、
//! `player_manager` / `team_manager` 同款 ⇒ **取的是哪个模块仍未定**（缺 `0x46e580` 的
//! TypeId 归属）。所以本字段仍标 **推断**。
//!
//! # 容量与去重（这是任务问的两个点，**都有硬结论**）
//!
//! | 问题 | 结论 | 档位 | 证据 |
//! |---|---|---|---|
//! | 存多少条 | **没有上限，也没有预留容量** | **观测** | `new`（arm64 `0x49d9a0`）只 `__rust_alloc(0x48,8)` 造 `RcInner` 与 `__rust_alloc(8,8)` 造 `WeakCaller`，**没有**任何 `0x20*N` 的缓冲区分配；两条推送路径都是 `Vec::push` 语义（x86 `0x3f0f..` 里 `len == cap` 时调 `RawVec` 的 grow，arm64 直接 `RawVec::grow_one`）⇒ 从 0 涨起、按需翻倍 |
//! | 重复的函数会去重吗 | **不会** | **观测** | 推送路径上**没有**任何"拿新文本和已有条目比"的循环/比较；唯一带键的查找是 `HashMap<i32, _>`（键 = player id，不是文本），且它只用来取那个 `u32`。⇒ 同一个玩家连发同一个函数，历史里就是两条 |
//!
//! # 上下键怎么翻（**未知**，而且本模块里没有）
//!
//! * 本模块 3 个函数里**没有**任何读 `Vec<FunctionData>` 的代码：`handle_function_fire_event`
//!   只 push，`new` 只建对象 + 订阅，闭包只 `WeakCaller::call` 转一次。
//! * 全库也**没有** `Vec<FunctionData>` 的读取点符号：只有它的 `drop_in_place`。
//! * 两份二进制里都**搜不到** `"ui_up"` / `"ui_down"` 这两个 Godot 输入动作名
//!   （`grep -c` 均为 0）；`GraphwarFunctionInput::process`（arm64 `0x4547a0`，5888 B）与
//!   `gui_input`（`0x455ea0`，1404 B）里也**没有**任何箭头键码立即数（`0x4000xx` 一个都没有）。
//! * ⇒ 诚实的说法：**"上下键翻历史"在本版里没有观测到落点**。候选只剩
//!   `GraphwarFunctionInput::process` / `gui_input` / `force_update_cb`（都没读完），
//!   或者这个历史**根本没被消费**（只写不读）。**缺的证据**：一次真机按键 + `LineEdit.text` 变化。
//!
//! # 地址约定
//!
//! `x86` 是 **ELF 虚地址**；Ghidra/REA 地址 = 本值 + `0x100000`。`arm64` 只用于定名。

/// 一个源级函数在两侧的落点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Func {
    /// arm64 2.0.60 的源级真名。
    pub name: &'static str,
    /// arm64 2.0.60 地址。
    pub arm64: u32,
    /// arm64 字节数。
    pub arm64_len: u32,
    /// x86-64 目标的 ELF 虚地址（`None` = 该函数在 x86 侧被内联/未观测到）。
    pub x86: Option<u32>,
    /// x86 字节数（Ghidra 的函数体长度）。
    pub x86_len: u32,
    /// 判据。
    pub how: &'static str,
}

/// 本模块 **3 个**源级函数（2.0.60 arm64 权威名单）。
pub const FUNCTIONS: [Func; 3] = [
    Func {
        name: "new",
        arm64: 0x0049_d9a0,
        arm64_len: 312,
        x86: Some(0x003f_10a0),
        x86_len: 0x13c,
        how: "唯一订阅 `function_info_event`（`0x3f115a`）+ 被 `0x517690` 的 `0x517e27` 调用；`alloc 0x48` = 16+8+48",
    },
    Func {
        name: "handle_function_fire_event",
        arm64: 0x0049_d71c,
        arm64_len: 596,
        x86: Some(0x003f_0e40),
        x86_len: 483,
        how: "3 个 panic::Location 键 + 被调者 String::clone / RawVec::grow_one / 模块容器；比值 0.81",
    },
    Func {
        name: "subscribe_to_events_event_handler::{{closure}}",
        arm64: 0x0049_d970,
        arm64_len: 48,
        x86: Some(0x003f_1080),
        x86_len: 0x20,
        how: "闭包槽 `0x955050` 的 `+0x20` = `0x3f1080`；体内唯一 `lea rsi` 指 `0x3f0e40`",
    },
];

/// `src/function_history_recorder.rs` 的全部 `panic::Location`：**3 个**，全在
/// `handle_function_fire_event`（x86 `0x3f0e40`）体内。
///
/// 字段：`(line, col, Location 结构体地址, 引用它的函数)`。
pub const LOCATIONS: [(u32, u32, u32, u32); 3] = [
    (66, 123, 0x0095_5008, 0x003f_0e40),
    (67, 62, 0x0095_5020, 0x003f_0e40),
    (78, 18, 0x0095_5038, 0x003f_0e40),
];

/// `FunctionData` 的字节数：**32**。
///
/// 真值：x86 `0x3f0e40` 的推送块逐个 `mov` ——
/// `0x00/0x08/0x10` 三个 8 字节（`String` = ptr/cap/len）+ `0x18` 一个 u32 + `0x1c` 一个 i32。
pub const RECORD_SIZE: usize = 32;
/// `text: String` 在记录里的偏移（24 字节）。
pub const TEXT_OFFSET: usize = 0;
/// `color: u32` 在记录里的偏移。
pub const COLOR_OFFSET: usize = 24;
/// `player: i32` 在记录里的偏移。
pub const PLAYER_OFFSET: usize = 28;

/// 只处理这一个事件：**`function_info_event`**（判别值 **35**）。
///
/// 真值：`0x3f0e40` 开头 `movabs r12,0x7fff…ffff` / `lea rax,[r12+0x24]` /
/// `cmp [rsi],rax`，即判别值 **`0x8000_0000_0000_0023`**（enum 的 niche 编码：高位 + tag `0x23`）。
/// 不是这个 ⇒ 走 `panic!()`（`0x3f0fee`，Location `:78:18`）。
///
/// 名字：`docs/07` §7.2 的 72 条表里 **35 = `function_info_event`**；
/// 订阅名串也是它（`0x3f1141` 引 `0x1bfccd`）——见模块文档首段的口径裁决（任务 77）。
pub const EVENT_FUNCTION_INFO_TAG: u64 = 0x8000_0000_0000_0023;

/// **旧名别名**（值相同 = 35），语义是错的：它指向 `function_info_event`，不是
/// `function_fire_event`(33)。保留只为 `tests/last_five.rs`（本分支不许改 `tests/`）；
/// 新代码用 [`EVENT_FUNCTION_INFO_TAG`]。
pub const EVENT_FIRE_TAG: u64 = EVENT_FUNCTION_INFO_TAG;

/// 本记录器**唯一**的订阅（`new` 的 `0x3f115a`）。
///
/// 事件名 19 字节（`0x3f114f mov edx,0x13`）、串在 `0x1bfccd`。
pub const SUBSCRIPTION_NAME: &str = "function_info_event";
/// 订阅名的字节数（与 `mov edx,0x13` 对齐）。
pub const SUBSCRIPTION_NAME_LEN: usize = 0x13;

/// `event + 0x20` 那个 `Option<String>` 的 "None" niche（`i64::MIN`）。
///
/// 真值：`0x3f0e55` 的 `cmp QWORD PTR [?? + 0x20], 0x8000000000000000` ⇒ 相等就 `return`。
pub const OPTION_NONE: i64 = i64::MIN;

/// 历史上限：**没有**。
pub const MAX_ENTRIES: Option<usize> = None;

/// 去重：**没有**。
pub const DEDUP: bool = false;

/// 本模块**会发**的事件种类 —— **一个都没有**（"发 0 条"是正面证据，见模块文档）。
///
/// 空枚举在 Rust 里**不可构造**，所以 `Vec<FunctionRecorderEvent>` 恒空：
/// 这把"本模块不发事件"从注释升级成了编译期不变量。
/// （不用 `crate::events::Event` 还有一个现实原因：`tests/last_five.rs` 把本文件
/// `#[path]` 编进另一个测试二进制时**不**带 `src/events.rs`，而本分支不许改 `tests/`。）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionRecorderEvent {}

/// 一条历史记录（`FunctionData`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionData {
    /// 触发的函数文本（原版是 `String`，从事件里 `clone` 出来）。
    pub text: String,
    /// 第二条模块给出的 4 字节值，按 player id 查表得到。
    ///
    /// **推断**：是玩家/队伍颜色（`GraphwarColor` 是 4 字节 enum，见 `docs/02` §18.3）；
    /// **缺的证据**：那个 map 的 value 类型名（泛型被擦除，符号表里只有 `get_module_wk`）。
    pub color: u32,
    /// 玩家 id（事件 `+0x38` 的 i32）。
    pub player: i32,
}

impl FunctionData {
    /// 造一条。
    pub fn new(text: impl Into<String>, color: u32, player: i32) -> Self {
        Self { text: text.into(), color, player }
    }
}

/// 历史 = 一个**只写不删**的 `Vec<FunctionData>`。
///
/// 与原版一致的三条性质：`push` 只追加、**不去重**、**无上限**（[`DEDUP`] / [`MAX_ENTRIES`]）。
///
/// # 事件流（任务 77 补，`wired` 的第二件；见模块文档）
///
/// `new`（x86 `0x3f10a0`）订阅 **1 条**：`function_info_event`（35）@`0x3f115a` →
/// 闭包槽 `0x955050`（`+0x20` = `0x3f1080`）→ 处理器 `0x3f0e40`。
/// **发 0 条**：`0x3f0e40` 体内**没有** `add_event`（`0x9bac80` 零命中）——这是"发方向"的
/// 正面证据（无事件可发），所以本模块的发件箱**恒空**，`outbox` 只用来让验收能断言"确实 0 条"。
#[derive(Debug, Default, Clone)]
pub struct History {
    entries: Vec<FunctionData>,
    /// 订阅名（`new` 的那一条 `subscribe`；顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱：原版**不发**事件 ⇒ 恒空（见上）。
    ///
    /// 元素类型是**空枚举** [`FunctionRecorderEvent`]（在 Rust 里不可构造）⇒
    /// "本模块不发事件"这条不是注释，而是**类型系统**钉住的：任何 `push` 都编译不过。
    pub outbox: Vec<FunctionRecorderEvent>,
}

impl History {
    /// 空历史 + 那一条订阅（原版 `new` 做的同一件事）。
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            subscribed: vec![SUBSCRIPTION_NAME],
            outbox: Vec::new(),
        }
    }

    /// 一次 `function_info_event`（35）的效果：**无条件追加一条**。
    ///
    /// 返回 `true` 表示真的加进去了（本模型永远 `true` —— 这正是"没有去重"的意思）。
    /// 旧名 `on_function_fire` 保留为别名（[`Self::on_function_fire`]）。
    pub fn on_function_info(
        &mut self,
        text: impl Into<String>,
        color: u32,
        player: i32,
    ) -> bool {
        self.entries.push(FunctionData::new(text, color, player));
        true
    }

    /// 旧名（旧文档把 35 叫 `function_fire_event`）—— 行为与 [`Self::on_function_info`] 相同。
    pub fn on_function_fire(&mut self, text: impl Into<String>, color: u32, player: i32) -> bool {
        self.on_function_info(text, color, player)
    }

    /// `handle_function_fire_event`（`0x3f0e40`）的可判定模型。
    ///
    /// 逐条对齐原版的三道闸：
    ///
    /// 1. `tag != 35` ⇒ 原版 `panic!()`（`:78:18`）；本模型**不 panic**，返回 `false`
    ///    （接线层不该因为事件流错乱把进程打死）。
    /// 2. 文本是 `None`（事件 `+0x20` 的 `Option<String>` niche）⇒ 原版直接 `return`，不入库。
    /// 3. 否则 push 一条 `(text, color, player)`。
    pub fn handle_function_info_event(
        &mut self,
        player_id: i32,
        text: Option<String>,
        color: u32,
        tag: u64,
    ) -> bool {
        if tag != EVENT_FUNCTION_INFO_TAG {
            return false;
        }
        match text {
            None => false,
            Some(t) => self.on_function_info(t, color, player_id),
        }
    }

    /// **我方探针的幂等口**（原版没有）：`interaction_selftest` 每次从头开始。
    ///
    /// 原版这条 `Vec` 只写不删（模块文档"容量与去重"一节），所以这个 `clear` **不是**复原行为，
    /// 只是验收侧的重置；它不出现在任何原版路径上。
    pub fn clear_for_probe(&mut self) {
        self.entries.clear();
    }

    /// 取走发件箱（原版不发事件 ⇒ 恒空；这个口只为验收"确实 0 条"）。
    pub fn take_outbox(&mut self) -> Vec<FunctionRecorderEvent> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条（恒 0）。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 条数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 只读遍历（原版**没有**观测到任何读取点，见模块文档）。
    pub fn entries(&self) -> &[FunctionData] {
        &self.entries
    }

    /// 某个文本出现了几次 —— 用来说明"不去重"。
    pub fn count_of(&self, text: &str) -> usize {
        self.entries.iter().filter(|e| e.text == text).count()
    }
}

/// 按名字查一条 [`FUNCTIONS`]。
pub fn func(name: &str) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// x86 与 arm64 的字节数比值（同源闸 `0.61–1.19`）。
pub fn size_ratio(f: &Func) -> Option<f64> {
    f.x86.map(|_| f.x86_len as f64 / f.arm64_len as f64)
}

/// 事件是否属于本记录器（判别值比对）。
pub fn is_function_info_event(tag: u64) -> bool {
    tag == EVENT_FUNCTION_INFO_TAG
}

/// 旧名别名（语义同 [`is_function_info_event`]，保留给 `tests/last_five.rs`）。
pub fn is_fire_event(tag: u64) -> bool {
    is_function_info_event(tag)
}

/// 事件里那个 `Option<String>` 是否为 `None`（是 ⇒ 原版直接 `return`，不记录）。
pub fn is_absent(raw: i64) -> bool {
    raw == OPTION_NONE
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    /// `new` 里那一条订阅在（`wired` 第一件）；发件箱恒空是**正面证据**（0x3f0e40 无 add_event）。
    #[test]
    fn new_subscribes_function_info_event_only() {
        let h = History::new();
        assert_eq!(h.subscribed, vec!["function_info_event"]);
        assert_eq!(h.subscribed.len(), 1);
        assert_eq!(SUBSCRIPTION_NAME.len(), SUBSCRIPTION_NAME_LEN);
        assert_eq!(EVENT_FUNCTION_INFO_TAG, 0x8000_0000_0000_0023);
        assert!(h.outbox.is_empty());
        assert!(h.is_empty());
    }

    /// 三个闸：tag 35 才收、`None` 文本直接丢、其余全进（不去重、无上限）。
    #[test]
    fn handler_gates_match_the_three_x86_checks() {
        let mut h = History::new();
        // 闸①：tag 不是 35 ⇒ 原版 panic（0x3f0e60 → 0x3f0fee），本模型返回 false
        assert!(!h.handle_function_info_event(1, Some("x".into()), 0, 33 | 0x8000_0000_0000_0000));
        assert!(h.is_empty());
        // 闸②：Option<String> 是 None ⇒ 原版直接 return（0x3f0e6e / 0x3f0e72）
        assert!(!h.handle_function_info_event(1, None, 0, EVENT_FUNCTION_INFO_TAG));
        assert!(h.is_empty());
        // 闸③：进
        assert!(h.handle_function_info_event(7, Some("sin(x)".into()), 0x00ff_0000, EVENT_FUNCTION_INFO_TAG));
        assert_eq!(h.len(), 1);
        assert_eq!(h.entries()[0], FunctionData::new("sin(x)", 0x00ff_0000, 7));
        // 不去重：同一条再来一次就是两条
        assert!(h.handle_function_info_event(7, Some("sin(x)".into()), 0x00ff_0000, EVENT_FUNCTION_INFO_TAG));
        assert_eq!(h.count_of("sin(x)"), 2);
        // 发方向：一条也不发
        assert_eq!(h.outbox_len(), 0);
        assert!(h.take_outbox().is_empty());
    }

    /// 旧名别名与新名指向同一个值（35），裁决只改名字不改字节。
    #[test]
    fn legacy_names_still_point_at_tag_35() {
        assert_eq!(EVENT_FIRE_TAG, EVENT_FUNCTION_INFO_TAG);
        assert!(is_fire_event(EVENT_FUNCTION_INFO_TAG));
        assert!(is_function_info_event(0x8000_0000_0000_0023));
        assert!(!is_function_info_event(0x8000_0000_0000_0021));
    }
}
