//! `function_history_recorder.rs` —— **函数历史记录器**（原版 `FunctionHistoryRecorder`）。
//!
//! # 它是什么
//!
//! 订阅 `GraphwarBackend` 的 **`function_fire_event`**，每收到一次就把
//! **"谁 + 什么函数 + 什么颜色"** 追加到自己的 `Vec<FunctionData>` 里。
//! 就这么多：本模块**只有 3 个函数**（`new` / `handle_function_fire_event` /
//! 订阅闭包），容量、去重、上下键翻历史**都不在这里**。
//!
//! # 3 个函数（arm64 2.0.60 未 strip，真名 + 真边界）
//!
//! | 函数 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 判据 |
//! |---|---|---|---|---|---|
//! | `new` | `0x49d9a0` | 312 | 被内联（无 Location） | — | arm64 符号；x86 侧无独立函数 |
//! | `handle_function_fire_event` | `0x49d71c` | 596 | **`0x3f0e40`** | 483 | `panic::Location` 3 个键 + 被调者（`String::clone` + `RawVec::grow_one` + 模块容器取值）；比值 0.81 |
//! | `subscribe_to_events_event_handler::{{closure}}` | `0x49d970` | 48 | 未知 | — | 体内唯一调用 = `WeakCaller::call` |
//!
//! 另有 `drop_in_place<…>` 若干（含 `Vec<FunctionData>` 的 124 B）—— 那是编译器产物，不是源级方法。
//!
//! # x86 `0x3f0e40` 全解（**观测**，Ghidra 反编译 + `objdump` 对照）
//!
//! ```text
//! 0x3f0e40  if (*event != 0x8000_0000_0000_0023) -> panic!(src/…:78:18)   ; EVENT_FIRE_TAG
//!           if event[4] == i64::MIN              -> return               ; Option 的 niche，None 不处理
//!           id = (i32)event[7]                                            ; +0x38 = player id
//!           mgr = weak.upgrade()                                          ; self+0x28 的 Weak（x86）/ 模块容器（arm64）
//!           Some(color_u32) = mgr.map.get(&id)                            ; +0x30 起是 hashbrown 表
//!           push(FunctionData { text: event[4..].clone(), color_u32, player: id })
//! ```
//!
//! 三个 `panic::Location` 就是这三处失败路径的坐标（[`LOCATIONS`]）。
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
        x86: None,
        x86_len: 0,
        how: "arm64 符号；x86 侧内联（本模块的 3 个 Location 全在 0x3f0e40 体内）",
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
        x86: None,
        x86_len: 0,
        how: "arm64 符号；体内唯一调用 = WeakCaller::call，x86 侧未定位",
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

/// 只处理这一个事件：`function_fire_event`。
///
/// 真值：`0x3f0e40` 开头的 `if (*param_2 == -0x7fffffffffffffdd)`，
/// 即判别值 **`0x8000_0000_0000_0023`**（enum 的 niche 编码：高位 + tag `0x23`）。
/// 不是这个 ⇒ 走 `panic!()`（Location `:78:18`）。
pub const EVENT_FIRE_TAG: u64 = 0x8000_0000_0000_0023;

/// `event + 0x20` 那个 `Option<String>` 的 "None" niche（`i64::MIN`）。
///
/// 真值：`0x3f0e55` 的 `cmp QWORD PTR [?? + 0x20], 0x8000000000000000` ⇒ 相等就 `return`。
pub const OPTION_NONE: i64 = i64::MIN;

/// 历史上限：**没有**。
pub const MAX_ENTRIES: Option<usize> = None;

/// 去重：**没有**。
pub const DEDUP: bool = false;

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
#[derive(Debug, Default, Clone)]
pub struct History {
    entries: Vec<FunctionData>,
}

impl History {
    /// 空历史（原版 `new` 里那条 `Vec` 的初始状态：无缓冲区）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 一次 `function_fire_event` 的效果：**无条件追加一条**。
    ///
    /// 返回 `true` 表示真的加进去了（本模型永远 `true` —— 这正是"没有去重"的意思）。
    pub fn on_function_fire(&mut self, text: impl Into<String>, color: u32, player: i32) -> bool {
        self.entries.push(FunctionData::new(text, color, player));
        true
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
pub fn is_fire_event(tag: u64) -> bool {
    tag == EVENT_FIRE_TAG
}

/// 事件里那个 `Option<String>` 是否为 `None`（是 ⇒ 原版直接 `return`，不记录）。
pub fn is_absent(raw: i64) -> bool {
    raw == OPTION_NONE
}
