//! `function_drawer.rs` —— **把函数曲线画上屏幕的那个模块**（原版 `FunctionDrawer`）。
//!
//! # 它是什么
//!
//! 一个控制器（`Gd<Control>` 派生，`GraphwarPlane::init` 会建 **3 个**实例并 `add_child`），
//! 自己不画任何图元：它**持有 `Line2D` 子节点**，每个"玩家 id"一条线。
//! 曲线是**节点**，不是 `draw_line` —— 这与 `docs/16-client.md` §26.4 的结论一致，
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
//! 这句话，更像函数输入框的绘制），4 个在 `EffectDrawer::draw`（`docs/16-client.md` §26.4）。
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
/// 真值：`docs/16-client.md` §26.3（arm64 侧 `Gd::default_instance`×3、
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
