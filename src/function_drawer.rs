//! `function_drawer.rs` —— **把函数曲线画上屏幕的那个模块**（原版 `FunctionDrawer`）。
//!
//! # 它是什么
//!
//! 一个控制器（`Gd<Control>` 派生，`GraphwarPlane::init` 会建 **3 个**实例并 `add_child`），
//! 自己不画任何图元：它**持有 `Line2D` 子节点**，每个"玩家 id"一条线。
//! 曲线是**节点**，不是 `draw_line` —— 这与 `docs/16-client.md` §23.4 的结论一致，
//! 本节把那条结论里"缺的证据"（`add_point` 循环的上界与步长）补齐了，见 §「`update_function` 全解」。
//!
//! # 名字与地址（arm64 2.0.60 未 strip；arm64 地址只用于定名，绝不当 x86 用）
//!
//! ```text
//! readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
//!   | grep -c 'FUNC.*graphwarrust.*function_drawer'
//! ```
//!
//! `graphwarrust::function_drawer::FunctionDrawer` 下有 **5 个**源级方法符号
//! （外加 2 份 `drop_in_place<FunctionDrawer>`，各 104 B）。**没有** `clear_functions` /
//! `set_z_index` —— 见 [`LEGACY_2024_ONLY`]。
//!
//! | 方法 | arm64 2.0.60 | 字节 | x86 vaddr | 字节 | 怎么配上的 |
//! |---|---|---|---|---|---|
//! | `remove_function` | `0x390a2c` | 276 | **`0x498b70`** | 256 | 被调者多重集（`queue_free` + `HashMap::remove_entry`），比值 0.93 |
//! | `update_function` | `0x390b40` | 848 | **`0x498c70`** | 992 | `panic::Location` 唯一锚 + 6 个被调者逐个相同，比值 1.17 |
//! | `update_transform` | `0x390e90` | 528 | **`0x499050`** | 608 | 被调者：`set_width`×2 / `set_scale`×2 / `set_joint_mode`×2，比值 1.15 |
//! | `update_function_fade` | `0x3910a0` | 276 | **`0x4992b0`** | 272 | 被调者：`set_default_color`×1，比值 0.99 |
//! | `draw` | `0x3911b4` | 560 | **未知** | — | x86 侧找不到对应的独立函数（候选都被否定，见下） |
//!
//! 比值闸：四个配对全部落在 `0.61–1.19` 里（本仓 `docs/02` §20 的口径）。
//!
//! > ⚠️ **x86 侧只有 `0x498c70` 一个函数带 `src/function_drawer.rs` 的 `panic::Location`**，
//! > 所以按"可归属函数"口径本模块是 **1 个**；另外三个 x86 地址是本节用
//! > **被调者多重集**配出来的（**观测**，不是唯一锚）。
//!
//! # `update_function` 全解（`0x498c70`，**逐位**）
//!
//! 入参（`objdump` 逐条读，`0x498c70–0x499050`）：
//!
//! ```text
//! rdi = &self          esi = player_id: i32
//! rdx = points ptr      rcx = points.len
//! r8  = new_len        r9d = color（0x00RRGGBB，建节点时用一次）
//! ```
//!
//! 流程：
//!
//! 1. `0x498caa` 在 `self+0x20` 的 `HashMap<i32, _>` 里按 `player_id` 查（hashbrown 的
//!    SIMD 组扫描，Ghidra 的反编译里那段 `pcmpeqb/pmovmskb/tzcnt` 就是它）；
//! 2. 命中且 `new_len <= 已写入数` ⇒ **直接 return，一个点都不加**（`0x498d5a` 的 `ja`）；
//! 3. `0x498f44` 才是追加路径：
//!    * `cmp rsi, rdx` / `ja 0x498fd1` —— `new_len > points.len()` 就走
//!      **切片越界 panic**，Location 就是 `src/function_drawer.rs:45:38`（槽 `0x95a270`）；
//!    * `0x498f51`：`new_len << 4` / `0x498f60`：`written << 4` —— **步长 16 字节 = 一个点**；
//!    * `0x498f90`：`movsd xmm0,[r15]`、`0x498f95`：`movsd xmm1,[r15+8]`
//!      ⇒ 每个点是 **两个 f64**（`x` 在 `+0`、`y` 在 `+8`），再 `cvtsd2ss` 成 `f32`；
//!    * `0x498f8b`：`call 0x407840(&gd, "upcast_ref", 10)`（字面量 `0x1c21a7`，长 10）；
//!    * `0x498fa6`：`esi = 0xffffffff`；`0x498fab`：`call [0x9bbf98]` = **`Line2D::add_point`**，
//!      即 `add_point(Vector2{x,y}, -1)` —— **index = -1 = 追加到末尾**；
//! 4. 循环上界 = `&points[written..new_len]`（左闭右开），**每次调用只补新增的那段**；
//! 5. 循环结束后 `*(usize*)(value+8) = new_len`，把"已写入数"记成新的长度（`0x498fbc` 附近）。
//!
//! ⇒ **是增量追加，不是整条重画**：一条曲线的第 n 帧只 `add_point` 出第 n 帧新增的
//! `new_len - written` 个点；点数变少（`new_len <= written`）时**什么都不做**
//! （曲线不会因为函数变短而缩短 —— 想让它变短得靠 `remove_function` 重建）。
//!
//! 首次见到某个 `player_id` 时（`0x498d68` 起）走"建节点"分支：`Line2D` 由
//! `construct_engine_object` 造出（arm64 同名分支在 `0x390cc0`），随后
//! `set_default_color`（`0x9bbf90`）、`set_z_index`（`0x9bb028`）、
//! `add_child_full`（`0x390d38` 那一族），然后插进 `HashMap`。
//! `color` 只在这里用一次：`r9d` 的 `&0xff` / `>>8` / `>>0x10` 三个字节 ÷ 255.0 → `Color(r,g,b,1)`。
//!
//! # `draw` 为什么在 x86 找不到（**有界否定**）
//!
//! arm64 `draw`（560 B）的被调者是 `Line2D::get_point_count` + `Line2D::get_point_position`
//! + `CanvasItem::draw_circle_full`（各 1 处，在循环里）+ 两组 gdext cast。
//! x86 侧 `draw_circle` 的 GOT 槽 `0x9bbfc0` 在整份 `.text` 只有 9 个调用点，
//! 其中 5 个在 `0x4993c0`（那个函数还引用 `"Write a function of x to hit the target"`
//! 这句话，更像函数输入框的绘制），4 个在 `EffectDrawer::draw`（`docs/16-client.md` §23.4）。
//! **没有一个**是"每组 1 次 + 循环"的形态 ⇒ x86 侧这个 `draw` 要么被内联进
//! `GraphwarPlane::draw`（`0x4b5df0`），要么就没了。**结论：未知，不强行落地。**
//!
//! # 地址约定
//!
//! * [`FUNCTIONS`] 里的 `x86` 是 **ELF 虚地址**（`objdump` 口径）；Ghidra/REA 地址 = 本值 + `0x100000`。
//! * `arm64` 只用于定名，**不要**拿它当 x86 地址。

/// 一个源级方法在两侧的落点。`x86 = None` 表示 x86 侧没有可归属的独立函数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Func {
    /// arm64 2.0.60 的源级方法名（未 strip 符号表里的真名）。
    pub name: &'static str,
    /// arm64 2.0.60 的地址。
    pub arm64: u32,
    /// arm64 的字节数（`.symtab` 的 `st_size`）。
    pub arm64_len: u32,
    /// x86-64 目标（2026-08-28）的 ELF 虚地址。
    pub x86: Option<u32>,
    /// x86 的字节数（到下一个函数入口的距离，含尾部对齐填充）。
    pub x86_len: u32,
    /// 怎么配上来的（一句话，便于复核）。
    pub how: &'static str,
}

/// 2.0.60（arm64）里真实存在的 5 个方法 —— **本模块权威名单**。
///
/// 真值来源：`.reference/steam/android-2.0.60/libgraphwarrust.so` 的 `.symtab`。
pub const FUNCTIONS: [Func; 5] = [
    Func {
        name: "remove_function",
        arm64: 0x0039_0a2c,
        arm64_len: 276,
        x86: Some(0x0049_8b70),
        x86_len: 256,
        how: "被调者：Node::queue_free + hashbrown::RawTable::remove_entry；比值 0.93",
    },
    Func {
        name: "update_function",
        arm64: 0x0039_0b40,
        arm64_len: 848,
        x86: Some(0x0049_8c70),
        x86_len: 992,
        how: "panic::Location 唯一锚（src/function_drawer.rs:45:38）+ 6 个被调者逐个相同；比值 1.17",
    },
    Func {
        name: "update_transform",
        arm64: 0x0039_0e90,
        arm64_len: 528,
        x86: Some(0x0049_9050),
        x86_len: 608,
        how: "被调者：set_width×2 / set_scale×2 / set_joint_mode×2；比值 1.15",
    },
    Func {
        name: "update_function_fade",
        arm64: 0x0039_10a0,
        arm64_len: 276,
        x86: Some(0x0049_92b0),
        x86_len: 272,
        how: "被调者：Line2D::set_default_color×1；比值 0.99",
    },
    Func {
        name: "draw",
        arm64: 0x0039_11b4,
        arm64_len: 560,
        x86: None,
        x86_len: 0,
        how: "未知：x86 侧未定位（draw_circle 的 9 个调用点无一符合\"循环里 1 次\"的形态）",
    },
];

/// **两版符号不一致的地方**：2024-07-30（x86，未 strip）的符号表里有这两个方法名，
/// 2.0.60（arm64，同版本）**没有**。
///
/// 按仓库口径（`docs/02` §18 补注）：**以 Android 2.0.60 为准**，即 2.0.60 的
/// `FunctionDrawer` 只有 [`FUNCTIONS`] 那 5 个方法；这两个要么被内联、
/// 要么被删掉了。
///
/// * `set_z_index`：`CanvasItem::set_z_index` 的调用**确实还在** ——
///   在 `update_function` 的"建节点"分支里（arm64 `0x390d04`），
///   所以最可能是**被内联进了 `update_function`**（**推断**）。
/// * `clear_functions`：两个臂的 `FunctionDrawer` 里都没有 `queue_free` 的
///   "批量"形态（`remove_function` 只 `queue_free` 1 个），所以它连内联落点都没观测到，
///   档位 **未知**。
pub const LEGACY_2024_ONLY: [&str; 2] = ["clear_functions", "set_z_index"];

/// `src/function_drawer.rs` 在目标 `.so` 里的**全部** `panic::Location`：**1 个**。
///
/// 字段：`(line, col, Location 结构体地址, 引用它的函数)`。
/// 结构体地址在 `.data.rel.ro`，`refs` 是**引用指令地址**（这里是 `0x498fd1` 那条
/// 越界分支的 `lea`）。
pub const LOCATIONS: [(u32, u32, u32, u32); 1] =
    [(45, 38, 0x0095_a270, 0x0049_8c70)];

/// 点的内存布局（**逐位**，`0x498f51`/`0x498f90`）：步长 16 字节，`x` 在 `+0`、`y` 在 `+8`，
/// **都是 f64**，进 `add_point` 前 `cvtsd2ss` 成 f32。
pub const POINT_STRIDE: usize = 16;
/// `x` 在点里的偏移（f64）。
pub const POINT_X_OFFSET: usize = 0;
/// `y` 在点里的偏移（f64）。
pub const POINT_Y_OFFSET: usize = 8;

/// 调 `Line2D::add_point` 时传的 index = `0xffffffff`（`esi`，`0x498fa6`）—— Godot 里
/// `add_point(position, index = -1)` 的"追加到末尾"。
pub const ADD_POINT_INDEX: i32 = -1;

/// 全二进制只有一个 `Line2D::add_point` 的 GOT 槽 `0x9bbf98`，调用点 **1 个**：
/// `0x498fab`（在 `update_function` 的追加循环里）。
pub const ADD_POINT_SLOT: u32 = 0x009b_bf98;
/// 唯一的 `add_point` 调用点。
pub const ADD_POINT_CALL_SITE: u32 = 0x0049_8fab;

/// `GraphwarPlane::init`（x86 `0x4b7380`）建 **3 个** `FunctionDrawer` 并 `add_child`。
///
/// 真值：`docs/16-client.md` §23.3（arm64 侧 `Gd::default_instance`×3、
/// `ExAddChild::new`×3、`Node::add_child_full`×3；x86 侧 `0x4db030`×3）。
/// **这 3 个各自负责什么，未知**（推测是 3 层/3 组，但没有任何证据能指到）。
pub const CHILDREN_PER_PLANE: usize = 3;

/// 追加窗口：给定"已写入数"和"本次应有长度"，返回**本次要补的点下标区间**。
///
/// 这就是 `update_function` 的第 2/4 步（`0x498d5a` 的 `ja` + `&points[written..new_len]`）：
/// `new_len <= written` ⇒ `None`（一个点都不加），否则 `written..new_len`。
///
/// ```text
/// fn update_function(&self, id: i32, points: &[(f64, f64)], new_len: usize, color: u32) {
///     let Some(entry) = self.lines.get(&id) else { /* 建 Line2D，color 只在这里用 */ };
///     if new_len <= entry.written { return; }          // 0x498d5a
///     for p in &points[entry.written..new_len] {       // 45:38 的越界 panic 在这儿
///         entry.line.add_point(Vector2::new(p.0 as f32, p.1 as f32), -1);
///     }
///     entry.written = new_len;
/// }
/// ```
pub fn append_window(written: usize, new_len: usize) -> Option<std::ops::Range<usize>> {
    if new_len <= written {
        None
    } else {
        Some(written..new_len)
    }
}

/// 按 `update_function` 的真实逻辑维护"每个 player id 已写入多少点"的模型。
///
/// 原版是一个 `HashMap<i32, (Gd<Line2D>, usize)>`（hashbrown，`self+0x20`）；
/// 这里用 `BTreeMap` 只是为了**确定性**（每条 key 的判定与原版无关顺序）。
/// 本模型**只**复刻"补哪些点"，不碰任何 Godot 类型。
#[derive(Debug, Default, Clone)]
pub struct CurveStore {
    written: std::collections::BTreeMap<i32, usize>,
}

impl CurveStore {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 一次 `update_function` 调用会 **`add_point` 几个点**。
    ///
    /// 首次出现某个 id 时原版会把 `written` 当 0（新建的 `Line2D` 没有点）。
    pub fn points_to_append(&self, player_id: i32, new_len: usize) -> usize {
        let written = self.written.get(&player_id).copied().unwrap_or(0);
        append_window(written, new_len).map(|r| r.len()).unwrap_or(0)
    }

    /// 提交一次 `update_function`：只有真的追加了才推进 `written`（原版两条路径都写回，
    /// 但只有在追加分支里才会写 —— 提前 return 的那条不写）。
    pub fn commit(&mut self, player_id: i32, new_len: usize) -> Option<std::ops::Range<usize>> {
        let written = self.written.get(&player_id).copied().unwrap_or(0);
        let w = append_window(written, new_len)?;
        self.written.insert(player_id, new_len);
        Some(w)
    }

    /// 当前的"已写入数"。
    pub fn written(&self, player_id: i32) -> Option<usize> {
        self.written.get(&player_id).copied()
    }

    /// 有多少条曲线（= 多少个 `Line2D` 子节点）。
    pub fn len(&self) -> usize {
        self.written.len()
    }

    /// 一条都没有。
    pub fn is_empty(&self) -> bool {
        self.written.is_empty()
    }
}

/// 按 [`FUNCTIONS`] 的名字查一条。
pub fn func(name: &str) -> Option<&'static Func> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// 有多少个方法在 x86 侧落到了地址（**1** —— 只有一个 `update_function` 带 Location）。
pub fn anchored_in_x86() -> usize {
    FUNCTIONS.iter().filter(|f| f.x86.is_some()).count()
}

/// x86 与 arm64 的字节数比值，用于"同源"闸（`0.61–1.19`）。
pub fn size_ratio(f: &Func) -> Option<f64> {
    f.x86.map(|_| f.x86_len as f64 / f.arm64_len as f64)
}


// ===========================================================================
// 原版方法体：原版 `FunctionDrawer` 的 gdext 节点类（`Control` 子类）（T57 从 `src/function_drawer.rs` 搬回）
// ===========================================================================
#[cfg(feature = "gdext-bindings")]
mod node_class {
//! [T57 已搬回原版模块] 这里是**原版 原版 `FunctionDrawer` 的 gdext 节点类（`Control` 子类）** 的方法体。
//!
//! 2026-11-27 从 `src/function_drawer.rs` 搬进 `src/function_drawer.rs`：原版把 `#[derive(GodotClass)]`
//! 的 struct 与方法体**写在同一个 `function_drawer.rs` 里**，证据：
//! `panic::Location` 证据：`update_function 0x498c70` → `src/function_drawer.rs:45:38`（唯一锚；
// `init` 内联在 `GraphwarPlane::init` 里，无独立符号）。
//!
//! **本仓唯一的结构差异**：原版整个 crate 都是 gdext 扩展（不需要 `cfg`）；本仓 `cargo test`
//! （不带 `gdext-bindings`）必须能编，所以这一层挂在
//! `#[cfg(feature = "gdext-bindings")] mod node_class` 下，由 `pub use node_class::*;` 把对外路径
//! 抬平 ⇒ `crate::function_drawer::EntityDrawer` 这类路径与原版一致。
//!
//! ---
//! 以下是搬运前的原始说明（保留：里面逐条写着与原版的差别与证据档）。
//!
//!
//! # 为什么会有这一份
//!
//! 原版的 `FunctionDrawer` 是一个 `Control` 子类（`graphwarrust::function_drawer::FunctionDrawer`），
//! 它**自己不画任何图元**：`GraphwarPlane::init`（x86 `0x4b7380`）建 **3 个**它并 `add_child`，
//! 它给每个 player id 建一条 `Line2D` 子节点，`update_function` 只把**新增的那几个点**
//! `add_point` 进去（`docs/16-client.md` §32.3：全二进制唯一的 `add_point` 调用点 `0x498fab`）。
//!
//! `src/function_drawer.rs` 已经把「补哪些点」的纯逻辑复原出来了（`append_window` /
//! `CurveStore`），但它按约定**不碰引擎**。本文件是那层薄壳：
//!
//! ```text
//! src/function_drawer.rs                纯逻辑：5 个方法的地址 / 追加窗口 / 点布局常量
//! src/function_drawer.rs ← 本文件：真的建 Line2D、真的 add_point、真的 add_child
//! ```
//!
//! 追加判定**只有一份**：本类不写 `if new_len > written`，而是调
//! [`function_drawer::append_window`] —— 那个函数就是 `0x498d5a` 的 `ja` + `&points[written..new_len]`。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 曲线载体 | 每个 player id 一条 `Line2D` 子节点（`add_child_full`，`0x498e93`） | 同：`Line2D::new_alloc()` + `add_child` | **观测** |
//! | 容器 | `HashMap<i32, (Gd<Line2D>, Color, usize)>`，entry **40 B**（`Gd` 16 + `Color` 16 + `usize` 8，entry 48 B） | `BTreeMap<i32, Curve{Gd, Color, written}>`——**只改成有序**，value 字段与顺序照原版 | 布局**观测**（`0x498f13`–`0x498f36` 的插入块）；`BTreeMap` 是**我方选择**（同 `CurveStore`） |
//! | 追加语义 | `new_len <= 已写入数` ⇒ 一个点都不加（`0x498d5a` 的 `ja`）；否则只补 `points[written..new_len]`，每点 `add_point(Vector2, -1)`（`0x498fab`，`esi = 0xffffffff`） | 同：走 [`function_drawer::append_window`] + [`function_drawer::ADD_POINT_INDEX`] | **逐位** |
//! | `new_len` 从哪来 | **独立入参**（`r8`）；`new_len > points.len()` 走切片越界 panic（`function_drawer.rs:45:38`） | 取 `points.len()` ⇒ 那条 panic 分支**按构造不可达**（不变量交给类型，不是把证据删掉） | 原版**逐位**；化简是**我方选择** |
//! | 点的精度 | 点 16 B = 两个 **f64**（`movsd` ×2 + `cvtsd2ss`），`POINT_STRIDE/POINT_X_OFFSET/POINT_Y_OFFSET` | `PackedVector2Array`（Godot 的 `Vector2` 是 **f32**）⇒ 窄化发生在**数组边界**，等价于原版那两条 `cvtsd2ss` | 原版**逐位**；边界窄化**推断**（同一份数只掉一次精度） |
//! | `color` 从哪来 | `r9d`（`0x00RRGGBB`），**只在建节点时用一次**：低字节=R、次字节=G、第三字节=B，各 ÷255，alpha=1（`0x498d74`–`0x498da1`） | 同：`color_from_packed`，仍只在建节点时 `set_default_color` | **观测**（指令级） |
//! | `z_index` | 建节点时 `set_z_index([self+0x40] 的 i32)`（`0x498dee` 取值 / `0x498e49` 传参 / `0x9bb028`）；那个字段就是 2024 符号表里被内联掉的 `set_z_index` 的宿主（§36.1 的**推断**在这一条上更站得住） | **不设**：字段的**初值未知**，本类不发明一个数字（`Line2D` 默认 0） | 原版**观测**；值是**未知** |
//! | `update_transform` 的 4 个被调者 | 每条线：`set_position(Vector2(x,y))`（`0x9bb230`）、`set_width(0.125f32)`（`0x9bbfa0`）、`set_scale(±scale)`（`0x9bb238`）、`set_joint_mode(2)`（`0x9bbfa8`） | 同：顺序、常量、取负都照做 | 常量与取负**逐位**（见 [`LINE_WIDTH`]）；`flip` 标志**观测**；三个入参的**含义未知**（调用方给） |
//! | `update_function_fade` | 按 id 查 entry，把 `alpha as f32` 写进 value 里 `Color` 的**第 4 个 f32**（`0x499377`），写回，再 `set_default_color(那个 Color)`（`0x4993a3`） | 同 | **观测**（指令级） |
//! | `remove_function` | `Node::queue_free` + hashbrown `remove_entry`（`0x498b70`） | 同：`queue_free` + `BTreeMap::remove` | **观测** |
//! | `draw` | arm64 有（560 B：`get_point_count` + `get_point_position` + `draw_circle` 各 1 处在循环里）；x86 **未定位** | **不实现 `_draw`**（画什么本来就不是本类的职责：曲线是节点） | **未知** |
//! | 返回值 | 5 个方法都是 `()` | `update_function` 返回**本次追加了几个点**，其它回 `bool` / 计数 | **我方选择**（验收口；没有它 headless 里分不清"没加"和"加错了"） |
//!
//! ⇒ 这个类证明的是「**纯逻辑的追加窗口能不能真的落到 `Line2D::add_point` 上**」，
//! **不是**"曲线已经画对了"：`GraphwarPlane::draw` 还没喂函数（没有 `GraphwarFunctionManager`），
//! `z_index` 与 `draw` 仍是未知。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --features gdext-bindings
//! cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/function_drawer_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! FunctionDrawer : Control
//!   update_function(player_id: int, points: PackedVector2Array, color: int) -> int
//!                                    # 建/取该 id 的 Line2D，**只**补 points[written..len]；
//!                                    # 变短时一个点都不加；返回本次补了几个点（-1 = id 越界被拒）
//!   remove_function(player_id: int) -> bool                # queue_free 那条线并从表里删
//!   update_transform(x: float, y: float, scale: Vector2, flip: bool) -> int
//!                                    # 对每条线 set_position/set_width(0.125)/set_scale(±scale)
//!                                    # /set_joint_mode(2)；返回动了几条线
//!   update_function_fade(player_id: int, alpha: float) -> float
//!                                    # 覆写 value 里 Color 的 alpha，写回，再 set_default_color；
//!                                    # 返回生效的 alpha（-1.0 = 无此 id）
//!
//!   function_count() -> int                        # 表里有几条曲线（= 几个 Line2D 子节点）
//!   point_count(player_id: int) -> int             # Line2D::get_point_count()（-1 = 无此 id）
//!   written(player_id: int) -> int                 # value 里的"已写入数"（-1 = 无此 id）
//!   stored_alpha(player_id: int) -> float          # value 里 Color 的 alpha（-1.0 = 无此 id）
//!   line_instance_id(player_id: int) -> int        # 那条 Line2D 的 instance id（-1 = 无此 id）
//!
//!   add_point_total() / created_line_total() / removed_line_total() -> int
//!   rejected_call_total() / transform_line_total() / fade_call_total() -> int
//! ```
//!
//! 后面这 6 个计数是**观测点**：`add_point_total` 是**我方**调了几次 `add_point`
//! （不是引擎侧钩子——Godot 的 `Line2D::add_point` 没有信号可连），它只用来对
//! "本次追加了几个点"，真正证明"没重画"的是 `get_point_position(0)` 仍是旧值。
//!
//! # 写这种类必须知道的几条（与 §38.4 / §41.6 同一套，这里只记与本类有关的）
//!
//! 1. **`base_mut()` 是独占借用** ⇒ 建节点那段要先 `create_line()`（内部 `base_mut()`），
//!    等它的借用结束再 `self.lines.get_mut()`。本类比 `EntityDrawer` 好办：建节点只发生在
//!    "表里没有这个 id"时，两条路径不会交叉。
//! 2. **`Gd<Node>` 不保活**：`Line2D` 归本类（父节点）所有，`Gd` 存在表里只是句柄；
//!    `queue_free` 之后**必须**同时从表里删（否则下一次 `add_point` 会碰野句柄 ⇒ `1337` + 卡死）。
//! 3. **树外 `add_child` 只记父子关系**：`--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    本类不依赖子节点的 `_ready` / `_process`（它们没有这两个虚方法）。
//! 4. **不用写 `set_process(true)`**，本类也没有 `_process`。
//! 5. **`add_point` 的 index = `-1` 要显式传**（`add_point_ex(p).index(ADD_POINT_INDEX)`）：
//!    gdext 的 `add_point()` 恰好也用 `-1` 当默认值，但那**不是**证据；原版是 `esi = 0xffffffff`，
//!    所以常量必须来自 [`function_drawer::ADD_POINT_INDEX`]，不能靠默认值。



use std::collections::BTreeMap;

use godot::classes::line_2d::LineJointMode;
use godot::classes::{Control, IControl, Line2D, Node};
use godot::prelude::*;

use crate::function_drawer::{self, ADD_POINT_INDEX};

/// `Line2D::set_width` 的实参：**0.125**。
///
/// 逐位：`update_transform`（x86 `0x499050`）的两个调用点 `0x49911c` / `0x49921c` 都是
/// `movss xmm0, [rip-…] # 1baff8`，而 `.rodata 0x1baff8` 的 4 个字节是
/// `00 00 00 3e` = `0x3e000000` = `0.125f32`：
///
/// ```text
/// objdump -s --start-address=0x1baff0 --stop-address=0x1bb000 .reference/steam/2026-08-28/libgraphwarrust.so
///  1baff0 63616c6c 4e6f6e65 0000003e 504f5354  callNone...>POST
/// ```
///
/// ⚠️ **纯逻辑里没有这个常量**（`src/function_drawer.rs` 只记了"`set_width`×2"，没记实参），
/// 所以它落在本文件里、带着地址——不是从别处抄的、也不是我方挑的。
pub const LINE_WIDTH: f32 = 0.125;

/// `Line2D::set_joint_mode` 的实参：**2** = `LINE_JOINT_ROUND`。
///
/// 观测：`0x499158` / `0x49925a` 的 `mov esi, 0x2`，紧跟着就是槽 `0x9bbfa8`（`set_joint_mode`）。
pub const LINE_JOINT_MODE: LineJointMode = LineJointMode::ROUND;

/// 一条曲线在表里的 value。原版是 **40 B** 的三元组，顺序就是这三个字段：
///
/// ```text
/// 0x498f17  mov [bucket+0x08], r11      ; Gd<Line2D> 的指针
/// 0x498f1c  mov [bucket+0x10], r10      ; Gd<Line2D> 的 instance id
/// 0x498f21  movdqa xmm0, [rsp+0x30]     ; 建节点时算出来的 Color
/// 0x498f27  movdqu [bucket+0x18], xmm0  ; Color（16 B，alpha 在 +0x24）
/// 0x498f2d  mov [bucket+0x28], 0x0      ; usize 已写入数
/// ```
///
/// ⇒ 原版的 value 是 `(Gd<Line2D>, Color, usize)`，**不是**纯逻辑 §`update_function` 里
/// 写的二元组。多出来的 `Color` 就是 `update_function_fade` 要改的那一份
/// （`0x499391` 读回来、`0x499377` 改 alpha、`0x4993a3` 写回 `Line2D`）。
struct Curve {
    /// 那条 `Line2D` 子节点（原版 value 的 +0x00/+0x08）。
    line: Gd<Line2D>,
    /// 建节点时用的颜色（原版 value 的 +0x10；`update_function_fade` 改它）。
    color: Color,
    /// 已经 `add_point` 进去几个点（原版 value 的 +0x20）。
    written: usize,
}

/// `FunctionDrawer`：每个 player id 一条 `Line2D`，`update_function` **增量追加**点。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct FunctionDrawer {
    base: Base<Control>,

    /// 原版 `HashMap<i32, (Gd<Line2D>, Color, usize)>`（hashbrown）。
    ///
    /// 偏移：`RawTable` 在 `self+0x00`（`[self]` = ctrl、`[self+8]` = bucket_mask、
    /// `[self+0x10]` / `[self+0x18]` = growth_left / items），`self+0x20` 是交给
    /// `0x47dec0`（SipHash）的 hash state，`[self+0x40]` 是 z_index。
    /// ⚠️ `src/function_drawer.rs` 的文档写的是"`self+0x20` 的 `HashMap`"——
    /// 本轮按指令读到的表在 `self+0`（见 `docs/16-client.md` §38.3）。
    ///
    /// 换成 `BTreeMap` 只为**确定性**（与 [`function_drawer::CurveStore`] 同一个理由：
    /// 迭代顺序不参与任何判定）。key 是玩家 id，**没排序**也照样工作。
    lines: BTreeMap<i32, Curve>,

    /// 累计调了几次 `Line2D::add_point`（我方计数，见文件头契约那一段）。
    add_point_total: i64,
    /// 累计建了几条 `Line2D`。
    created_line_total: i64,
    /// 累计 `remove_function` 掉了几条。
    removed_line_total: i64,
    /// 累计被拒的调用（id 超出 i32 / 表里没这个 id）。
    rejected_call_total: i64,
    /// `update_transform` 累计动了几条线。
    transform_line_total: i64,
    /// `update_function_fade` 生效了几次。
    fade_call_total: i64,
}

/// 原版的 `& 0xff` / `>> 8` / `>> 0x10` 三个字节 ÷ 255.0 → `Color(r, g, b, 1)`（`0x498d74`–`0x498da1`）。
///
/// 入参是 GDScript 的 `int`（i64）：**只取低 24 位**，与 `r9d`（低 32 位寄存器）一致。
fn color_from_packed(packed: i64) -> Color {
    let bits = packed as u64;
    let r = (bits & 0xff) as f32 / 255.0;
    let g = ((bits >> 8) & 0xff) as f32 / 255.0;
    let b = ((bits >> 16) & 0xff) as f32 / 255.0;
    Color::from_rgba(r, g, b, 1.0)
}

#[godot_api]
impl IControl for FunctionDrawer {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            lines: BTreeMap::new(),
            add_point_total: 0,
            created_line_total: 0,
            removed_line_total: 0,
            rejected_call_total: 0,
            transform_line_total: 0,
            fade_call_total: 0,
        }
    }

    // 原版的 `draw`（arm64 `0x3911b4`）在 x86 侧**没有落点**（`FunctionDrawer::FUNCTIONS`
    // 里 `x86: None`，§36.3 的有界否定）⇒ 本类不覆写 `_draw`：它本来也不画图元。
}

#[godot_api]
impl FunctionDrawer {
    /// 喂一帧的函数点：**只补新增的那一段**。
    ///
    /// 对应 `update_function`（x86 `0x498c70`）：
    ///
    /// 1. 表里没有这个 `player_id` ⇒ 建 `Line2D`（`set_default_color(color)` + `add_child`）；
    /// 2. [`function_drawer::append_window`]（= `0x498d5a` 的 `ja`）：`new_len <= 已写入数` ⇒ **返回，一个点都不加**；
    /// 3. 否则 `points[written..new_len]` 逐点 `Line2D::add_point(p, -1)`（`0x498fab`）；
    /// 4. 追加过才把"已写入数"改成 `new_len`（原版提前 return 那条**不写**）。
    ///
    /// **返回本次追加了几个点**（0 = 变短/相等所以没加，-1 = `player_id` 超出 i32 被拒）。
    ///
    /// `color` 只在**建节点**时用（原版 `r9d` 也只在那里用一次）；之后想改走
    /// [`FunctionDrawer::update_function_fade`]。
    ///
    /// ⚠️ `pub` 是为了让 `GraphwarPlane`（`graphwar_plane.rs`）直接调——原版唯一的调用者
    /// 就是它（`0x4b69ec`，`docs/16-client.md` §41）。GDScript 侧照样能 `call("update_function", …)`。
    #[func]
    pub fn update_function(&mut self, player_id: i64, points: PackedVector2Array, color: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            // 原版的 key 是 i32（`0x498caa` 之后按 dword 比较）⇒ 越界不在契约里，拒绝而不是截断。
            self.rejected_call_total += 1;
            godot_warn!("FunctionDrawer::update_function player_id {player_id} 超出 i32");
            return -1;
        };

        // ① 首次见到这个 id 才建节点（原版 `0x498d68` 起的那条分支）。
        if !self.lines.contains_key(&id) {
            self.create_line(id, color);
        }

        // 原版的 `new_len` 是独立入参（`r8`）；本类取数组长度 ⇒ `new_len > points.len()` 不可达。
        let pts = points.as_slice();
        let new_len = pts.len();

        let curve = self.lines.get_mut(&id).expect("刚刚 ensure 过");
        let Some(window) = function_drawer::append_window(curve.written, new_len) else {
            // `new_len <= written`：原版在这里直接 return（曲线**不会**变短）。
            return 0;
        };

        let mut appended = 0i64;
        for i in window {
            let Some(p) = pts.get(i) else {
                // 上界就是 `pts.len()`，这行按构造到不了；真到了就停下而不是 panic。
                break;
            };
            // 原版：`movsd` ×2 + `cvtsd2ss` ⇒ `add_point(Vector2, -1)`（`esi = 0xffffffff`）。
            curve
                .line
                .add_point_ex(*p)
                .index(ADD_POINT_INDEX)
                .done();
            appended += 1;
        }
        curve.written = new_len;
        self.add_point_total += appended;
        appended
    }

    /// 建那条 `Line2D`（原版"首次见到 player_id"分支）：`set_default_color` → `add_child`。
    ///
    /// **没做** `set_z_index`：原版在 `0x498dee` 取 `[self+0x40]`、`0x498e49` 把它交给
    /// `0x9bb028`（`set_z_index`），那个字段的**初值未知** ⇒ 本类不发明一个数字
    /// （`Line2D` 默认 0）。
    fn create_line(&mut self, id: i32, packed_color: i64) {
        let color = color_from_packed(packed_color);

        let mut line = Line2D::new_alloc();
        // `0x498e30`：`set_default_color(Color)`。
        line.set_default_color(color);
        // `0x498e93`：`add_child_full`。子节点归本节点所有（文件头第 2 条）。
        let node: Gd<Node> = line.clone().upcast();
        self.base_mut().add_child(&node);

        self.created_line_total += 1;
        self.lines.insert(
            id,
            Curve {
                line,
                color,
                written: 0,
            },
        );
    }

    /// 删一条曲线：`Node::queue_free` + 从表里删（原版 `0x498b70`）。
    ///
    /// 返回是否真删了。**必须两件事一起做**：只 `queue_free` 不删表 ⇒ 下次 `add_point` 碰野句柄。
    ///
    /// ⚠️ `pub` 同上：原版唯一的调用者是 `GraphwarPlane::draw`（`0x4b6418`）。
    #[func]
    pub fn remove_function(&mut self, player_id: i64) -> bool {
        let Ok(id) = i32::try_from(player_id) else {
            self.rejected_call_total += 1;
            return false;
        };
        let Some(curve) = self.lines.remove(&id) else {
            self.rejected_call_total += 1;
            return false;
        };
        let mut line = curve.line;
        line.queue_free();
        self.removed_line_total += 1;
        true
    }

    /// 每条线摆位置 + 设宽度 / 缩放 / 接点样式（原版 `update_transform`，x86 `0x499050`）。
    ///
    /// 观测到的循环体（每条线 4 个调用，顺序如下）：
    ///
    /// ```text
    /// set_position(Vector2(x, y))        ; 0x9bb230，取两个 f32 入参
    /// set_width(0.125)                   ; 0x9bbfa0，实参见 LINE_WIDTH
    /// set_scale(±Vector2)                ; 0x9bb238；flip 为真时 xmm2 与 0x19e880 的
    ///                                    ; 四个 -0.0 做 pxor ⇒ 两个分量都取负
    /// set_joint_mode(2)                  ; 0x9bbfa8，实参见 LINE_JOINT_MODE
    /// ```
    ///
    /// ⚠️ **三个入参的含义未知**：`x`/`y` 按被调者 `set_position` 的用法命名，`scale` / `flip`
    /// 是形状（`xmm2` / `esi`）的直接翻译。原版的调用方（`GraphwarPlane::draw`）传什么，
    /// 本切片没接（没有 `GraphwarFunctionManager`）。
    ///
    /// 返回动了几条线（= 表长）。
    #[func]
    fn update_transform(&mut self, x: f64, y: f64, scale: Vector2, flip: bool) -> i64 {
        let s = if flip {
            Vector2::new(-scale.x, -scale.y)
        } else {
            scale
        };
        let position = Vector2::new(x as f32, y as f32);

        let mut touched = 0i64;
        for curve in self.lines.values_mut() {
            curve.line.set_position(position);
            curve.line.set_width(LINE_WIDTH);
            curve.line.set_scale(s);
            curve.line.set_joint_mode(LINE_JOINT_MODE);
            touched += 1;
        }
        self.transform_line_total += touched;
        touched
    }

    /// 改某条曲线的透明度（原版 `update_function_fade`，x86 `0x4992b0`）。
    ///
    /// 原版只做三件事：把 `alpha as f32` 写进 value 里那个 `Color` 的第 4 个 f32
    /// （`0x499377` 的 `movss [……-0xc]`，即 Color 的 alpha 车道），把整份 `Color` 写回 entry，
    /// 再 `set_default_color(那个 Color)`（`0x4993a3`）。
    ///
    /// 返回生效的 alpha（-1.0 = 表里没这个 id）。
    #[func]
    fn update_function_fade(&mut self, player_id: i64, alpha: f64) -> f64 {
        let Ok(id) = i32::try_from(player_id) else {
            self.rejected_call_total += 1;
            return -1.0;
        };
        let Some(curve) = self.lines.get_mut(&id) else {
            self.rejected_call_total += 1;
            return -1.0;
        };
        // 原版不夹取：`cvtsd2ss` 直接窄化（f64 → f32 的饱和语义也照 Rust 的 `as`）。
        curve.color.a = alpha as f32;
        let color = curve.color;
        let mut line = curve.line.clone();
        line.set_default_color(color);
        self.fade_call_total += 1;
        color.a as f64
    }

    /// 表里有几条曲线（= 有几个 `Line2D` 子节点）。
    #[func]
    fn function_count(&self) -> i64 {
        self.lines.len() as i64
    }

    /// 第 `player_id` 条曲线现在有几个点（`Line2D::get_point_count`）。无此 id 返回 -1。
    #[func]
    fn point_count(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.line.get_point_count() as i64,
            None => -1,
        }
    }

    /// value 里的"已写入数"。无此 id 返回 -1。
    #[func]
    fn written(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.written as i64,
            None => -1,
        }
    }

    /// value 里那个 `Color` 的 alpha。无此 id 返回 -1.0。
    #[func]
    fn stored_alpha(&self, player_id: i64) -> f64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1.0;
        };
        match self.lines.get(&id) {
            Some(c) => c.color.a as f64,
            None => -1.0,
        }
    }

    /// 那条 `Line2D` 的 instance id（GDScript 侧用它从子节点里认人）。无此 id 返回 -1。
    #[func]
    fn line_instance_id(&self, player_id: i64) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            return -1;
        };
        match self.lines.get(&id) {
            Some(c) => c.line.instance_id().to_i64(),
            None => -1,
        }
    }

    /// 累计调了几次 `Line2D::add_point`（**我方**计数）。
    #[func]
    fn add_point_total(&self) -> i64 {
        self.add_point_total
    }

    /// 累计建了几条 `Line2D`。
    #[func]
    fn created_line_total(&self) -> i64 {
        self.created_line_total
    }

    /// 累计删了几条曲线。
    #[func]
    fn removed_line_total(&self) -> i64 {
        self.removed_line_total
    }

    /// 累计被拒的调用（id 越界 / 无此 id）。
    #[func]
    fn rejected_call_total(&self) -> i64 {
        self.rejected_call_total
    }

    /// `update_transform` 累计动了几条线。
    #[func]
    fn transform_line_total(&self) -> i64 {
        self.transform_line_total
    }

    /// `update_function_fade` 生效了几次。
    #[func]
    fn fade_call_total(&self) -> i64 {
        self.fade_call_total
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」，不发明第二份）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/function_drawer_check.gd`（headless，真引擎 + 真 `Line2D`）。
// 这里钉三件容易悄悄跑偏的事：追加窗口、打包颜色 -> Color 的字节序、宽度常量。

#[cfg(test)]
mod tests {
    use super::*;
    use godot::obj::EngineEnum;

    /// 增量追加的判定就是 `append_window`：10 → 12 只补 2 个，12 → 5 一个都不补。
    #[test]
    fn append_window_is_the_only_increment_rule() {
        assert_eq!(function_drawer::append_window(0, 10), Some(0..10));
        assert_eq!(function_drawer::append_window(10, 12), Some(10..12));
        assert_eq!(
            function_drawer::append_window(10, 12).map(|r| r.len()),
            Some(2),
            "10 → 12 只补 2 个点（不是重画 12 个）"
        );
        assert_eq!(function_drawer::append_window(12, 12), None, "相等也不补");
        assert_eq!(function_drawer::append_window(12, 5), None, "变短一个点都不补");
        assert_eq!(function_drawer::append_window(0, 0), None);
    }

    /// `add_point` 的 index 取自纯逻辑（原版 `esi = 0xffffffff`），不是 gdext 的默认值。
    #[test]
    fn add_point_index_comes_from_the_pure_module() {
        assert_eq!(ADD_POINT_INDEX, -1);
        assert_eq!(ADD_POINT_INDEX as u32, 0xffff_ffff, "原版 esi = 0xffffffff");
    }

    /// 打包色 -> `Color`：**低字节是 R**（`0x498d74`–`0x498da1`），alpha 恒 1。
    #[test]
    fn packed_color_low_byte_is_red() {
        let c = color_from_packed(0x11_22_33);
        assert!((c.r - 0x33 as f32 / 255.0).abs() < 1e-6, "r = 低字节 = 0x33");
        assert!((c.g - 0x22 as f32 / 255.0).abs() < 1e-6, "g = 次字节 = 0x22");
        assert!((c.b - 0x11 as f32 / 255.0).abs() < 1e-6, "b = 第三字节 = 0x11");
        assert!((c.a - 1.0).abs() < 1e-6, "alpha 恒 1（原版常量槽 0x19cf70 的尾车道）");
        // 低字节=R ⇒ `0x0000ff` 才是纯红，`0xff0000` 是纯蓝
        // （写成 `0xRRGGBB` 的直觉会反过来；headless 脚本里那条断言是同口径）。
        let red = color_from_packed(0x00_00_ff);
        assert!((red.r - 1.0).abs() < 1e-6, "0x0000ff → 纯红，r={}", red.r);
        assert!(red.g.abs() < 1e-6 && red.b.abs() < 1e-6);
        let blue = color_from_packed(0xff_00_00);
        assert!((blue.b - 1.0).abs() < 1e-6, "0xff0000 → 纯蓝，b={}", blue.b);
        assert!(blue.r.abs() < 1e-6 && blue.g.abs() < 1e-6);
        // 只取低 24 位：高位（含 i64 的符号扩展）不进颜色。
        let high = color_from_packed(0x1234_5678_9abc_deff_u64 as i64);
        assert!((high.r - 1.0).abs() < 1e-6, "r={}", high.r);
        assert!((high.g - 0xde as f32 / 255.0).abs() < 1e-6, "g={}", high.g);
        assert!((high.b - 0xbc as f32 / 255.0).abs() < 1e-6, "b={}", high.b);
    }

    /// 宽度常量是 `.rodata 0x1baff8` 那 4 个字节：`00 00 00 3e` = `0x3e000000` = 0.125。
    #[test]
    fn line_width_is_the_bit_pattern_from_rodata() {
        assert_eq!(LINE_WIDTH.to_bits(), 0x3e00_0000);
        assert!((LINE_WIDTH - 0.125).abs() < f32::EPSILON);
        // 接点样式 = 2（`mov esi, 0x2`）。
        assert_eq!(LINE_JOINT_MODE, LineJointMode::ROUND);
        assert_eq!(LineJointMode::ROUND.ord(), 2);
    }
}
}

#[cfg(feature = "gdext-bindings")]
pub use node_class::*;
