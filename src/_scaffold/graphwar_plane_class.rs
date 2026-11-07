//! **不是原版的东西**：把复原的 [`crate::graphwar_plane`] 挂成**真 Godot 节点类**的薄壳。
//!
//! # 为什么会有这一份
//!
//! 原版的 `GraphwarPlane` 是**棋盘本体**（`docs/16-client.md` §23）：它自己只画**一个空心黑框**
//! （`draw_set_transform` ×1 + `draw_rect` ×1，`Rect2(-25,-15,50,30)`、黑、宽 `0.2`、
//! **不填充**），曲线**不在**它里面画——它 `init` 里建 **3 个 `FunctionDrawer` 子节点**，
//! 弹道是 `Line2D` 子节点，由那些子节点画（§26.4 的**有界否定**：`draw_line` 在本类里 0 次）。
//!
//! `src/graphwar_plane.rs` 已经把「建几个子节点 / 脏矩形怎么算 / 每帧刷几次 / 画哪个框」
//! 复原成纯逻辑（`GraphwarPlane::init_children` / `ready_plan` / `process_plan` /
//! `obstacle_partial_update_event` / `BOARD_*`），但按约定**不碰引擎**。本文件是那层壳：
//!
//! ```text
//! src/graphwar_plane.rs                 纯逻辑：子节点意图 / 脏矩形 / 棋盘框常量 / 信号名
//! src/_scaffold/graphwar_plane_class.rs ← 本文件：真的 add_child、真的 draw_rect、真的接 resized
//! ```
//!
//! # 本切片做四件本类自己的事（`FunctionDrawer` 已由 §40 的节点类补上）
//!
//! 1. **建 3 个子节点**（`init` 观测）——它们现在是**真的 `FunctionDrawer`**
//!    （`src/_scaffold/function_drawer_class.rs`，`docs/16-client.md` §40）；
//! 2. **脏矩形**：`obstacle_partial_update_event` 的纯数据版 + 每帧冲刷；
//! 3. **画棋盘框**：`draw_set_transform` + `draw_rect`，常量全取 `graphwar_plane::BOARD_*`；
//! 4. **喂函数曲线**（`docs/16-client.md` §41）：`draw` 末尾按
//!    "`get_active_functions_on_tick`（`0x456df0`）给的 id → 每 tick 露 30 个点"往第 0 个
//!    `FunctionDrawer` 里塞点（`update_function`，`0x4b69ec`）。点的**来源**
//!    （`GraphwarFunctionManager`，原版由 `set_function_points` `0x456810` 写）还没复原
//!    ⇒ 现在由调用方经 [`GraphwarPlane::set_function_points`] 注入。
//!
//! 障碍 / 士兵**不在本文件**：它们分别归 `ObstacleDrawer` / `EntityDrawer`（都没有节点类）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 子节点 | `init` 里 `Gd::default_instance::<FunctionDrawer>()` ×3 + `Node::add_child` ×3 | 同样 3 个 `FunctionDrawer`（`FunctionDrawer::new_alloc()`）——**不再是 `Control` 占位**（§42） | **观测**（个数 / 类名） |
//! | 子节点的名字 | 没 `set_name` ⇒ 引擎自动名 `@FunctionDrawer@N`（`docs/16` §2.1.10 的 dump 里同族的 `@AxisDrawer@N` / `@EntityDrawer@N` 都是自动名） | 也没 `set_name` ⇒ 自动名 `@FunctionDrawer@N`。**运行时观测**的值是 `@FunctionDrawer@N`（`docs/16` §38.2） | 原版**观测**；本类**不伪造**那个名字 |
//! | 建子节点的时机 | `init` | **`ready`**（`#[func] build_children()` 也能手动调）。理由：gdext 的 `init` 里 `Base` 还没构造完（`InitState::Uninitialized`），`base_mut()` 用不了 | 原版**观测**；时机差是**gdext 约束** |
//! | `draw` 的图元 | `draw_set_transform` ×1（槽 `0x9bc018`）+ `draw_rect` ×1（槽 `0x9bc028`）；`draw_line` **0** 次 | 同 | **逐位**（常量槽）/ **观测**（调用点） |
//! | `draw_set_transform` 的**实参** | 只确证了 GOT 槽，**实参没读到** | 用 `to_pixel` 那套换算当默认值（`PIXEL_OFFSET` / `PX_PER_UNIT`），两个导出属性可改 | 原版**未知**；默认值是**我方选择** |
//! | `ready` 的 3 个订阅 | `add_obstacle_event` / `destroy_obstacle_event` / `clear_obstacles_event`（`READY_EVENTS`） | **没订**（要 `GraphwarBackend`）；只把信号名与事件名**从纯逻辑里取出来用** | 名字与顺序**观测**；订阅**未做** |
//! | `resized` → `on_resized` | 信号名 `"resized"`、处理者名 `"on_resized"`（`.rodata` 两条字面量） | 真连（信号名与处理者名都取 `graphwar_plane::RESIZED_*`，不另写字符串） | **观测** |
//! | `process` 每帧 | `ObstacleDrawer::update` ×2 + 读一次队色（`process_plan` ⇒ `(2, true)`） | 用 `process_plan()` 的 `2` 当"本帧刷几次"并计数；**没有** `ObstacleDrawer` 节点、**没有**队色 | 数字**观测**；刷的是空操作 |
//! | `on_resized` 里那 3 次 `Control::set_*` | 走 GOT 槽，**方法名未解**（`UNKNOWNS` 第 3 条） | 只 `queue_redraw()` | 原版**未知** |
//! | `obstacle_partial_update_event` 的坏 tag | **`panic!()`**（消息是 `.rodata` 的 `"explicit panic"`） | **先拦**：`is_partial_update_tag` 为假就 `godot_warn!` + 返回空数组 | 原版**观测**；本类**故意不同**（panic 穿过 FFI 会 abort 整个进程） |
//! | `clear_state` / `update_entities` | 两个方法存在（`0x4bded0` / `0x4be270`），语义未解 | **不实现**（`UNKNOWNS` 第 5 条：`clear_state` 引用 `EntityDrawer` / `EffectDrawer` 类名的用途未知） | **未知** |
//! | 曲线的点从哪来 | `GraphwarFunctionManager` 里存的采样点（bucket+0x58/+0x60），由 `set_function_points`（`0x456810`）写入 | **本类自己存一份**（`FunctionEntry`）+ 调用方注入；那个模块还没有 `src/` 文件 | 原版**观测**；存储是**替身**（§41.4） |
//! | 曲线的上场速率 | `new_len = min(len, trunc(30*(fractional_tick - active_tick)))`，`30.0` = `.rodata 0x19ef90` | 同（[`revealed_points`]） | **逐位**（常量）/ **观测**（算式） |
//! | tick 从哪来 | 每帧从 `GraphwarTickHandler` 读（u32 tick + `get_fractional_tick` `0x519530`） | 调用方 [`GraphwarPlane::set_fractional_tick`] | 原版**观测**；接线**未做**（没有 tick handler 模块） |
//! | 淡出 | `update_function_fade`（`0x4b6a0e`），alpha = `get_function_fade_on_tick` 的返回 | **没接**：那个函数**没有 x86 落点**（走 GOT 槽 `0x9bbd08`），算式未知 | **未知** |
//! | `update_transform`（位置/宽度/缩放） | `0x4b6147`，实参解出一半（见 §41.5） | **没接** | **未知**（解出的部分自相矛盾，别照抄） |
//! | 每帧重画的来源 | `process` 里没有 `queue_redraw` 的直接调用（未解） | `_process` 里"**还有该喂没喂的点**（应喂 > 已喂）就 `queue_redraw`"（§43 修的：原来按"应喂 < 全长"判，最后一小段永远喂不进去） | **我方选择** |
//!
//! ⇒ 这个类证明的是「**子节点意图 / 脏矩形 / 棋盘框常量 / 喂点路径**能真的落到引擎上」，
//! **不是**"棋盘已经能玩"：曲线现在真的会往 `Line2D` 里长点（headless 实测点数 60→90→100），
//! 但①点的生产者没复原（要外部注入）、②`update_transform` 没接（位置/粗细还不对）、
//! ③障碍与士兵也没有节点。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/draw_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarPlane : Control
//!   @export board_scale : float = 20.0    # 棋盘单位 → 像素（PX_PER_UNIT = 1000/50）
//!   @export flip_y      : bool  = true    # 棋盘 y 向上、画布 y 向下 ⇒ 默认取负
//!
//!   build_children() -> int                       # 建 3 个 FunctionDrawer 子节点（幂等），返回个数
//!   function_drawer_count() -> int                # 纯逻辑记下的子节点数（FUNCTION_DRAWER_CHILDREN）
//!   to_pixel(x, y) -> Vector2i                    # graphwar_plane::to_pixel 的搬运
//!   on_obstacle_partial_update(tag, x, y, size) -> PackedInt32Array
//!                                                 # [min_x, min_y, max_x, max_y]；坏 tag 返回空
//!   partial_update_count() -> int
//!   drain_partial_updates() -> int                # 冲刷队列并返回冲掉几个
//!   ready_event_names() -> PackedStringArray      # 原版 ready 会订的 3 个事件（本类没订）
//!   draw_call_count() / drawn_rect_total() -> int
//!   process_call_count() / obstacle_update_total() / resized_call_count() / ready_call_count() -> int
//!
//!   # 曲线喂点（docs/16-client.md §41）
//!   set_fractional_tick(tick) -> float            # 原版每帧自己从 tick handler 读
//!   fractional_tick() -> float
//!   set_function_points(id, points, active_tick, color) -> int   # points 是扁平 f64 [x0,y0,x1,y1,…]
//!   clear_function_points() -> int
//!   active_function_ids() -> PackedInt32Array     # = get_active_functions_on_tick(0x456df0) 的返回集
//!   feed_functions() -> int                       # = 原版 draw 尾部那段循环；返回本帧追加几点
//!   function_count() / function_point_count(id) -> int
//!   function_write_total() / function_point_total() / removed_function_total() / reveal_redraw_count() -> int
//! ```
//!
//! # 写这种类必须知道的几条（`docs/16-client.md` §34.4 实测过，这里只记与本类有关的）
//!
//! 1. **`Gd<Node>` 不保活**：子节点归父节点所有，`add_child` 之后把 `Gd` 丢掉是安全的；
//!    反过来"父节点被回收而手里还留着 `Gd`"就成了野句柄（碰一下就 `1337` + 卡死）。
//!    ⇒ 本类**只**把子节点的 `instance_id()`（`u64`，纯逻辑的 `NodeRef`）记进
//!    [`graphwar_plane::GraphwarPlane::function_drawers`]，一个 `Gd` 都不留。
//! 2. **树外 `add_child` 只记父子关系**：`--script` 模式的 `_initialize()` 跑在根节点进树之前，
//!    那一刻子节点的 `_ready` / `_process` 不会来。本类不依赖它们（`build_children` 自己就完事）。
//! 3. **`resized` 只在 `ready` 里连** ⇒ 树外 `set_size` 不会触发 `on_resized`（连都还没连）。
//! 4. **不用写 `set_process(true)`**：实现了 `_process` 的扩展类由 Godot 4.7 自己开。
//! 5. **`base_mut()` 是独占借用** ⇒ `add_child` 那段里不能同时碰 `self.plane`。本类的做法：
//!    先把父节点取成**局部** `Gd<Node>`（`self.to_gd().upcast()`），再把
//!    `self.plane.init_children(|_| …)` 的闭包只捕获那个局部的父节点。

use godot::classes::{Control, IControl, Node};
use godot::prelude::*;

use super::function_drawer_class::FunctionDrawer;
use super::obstacle_drawer_class::ObstacleDrawer;
use crate::graphwar_plane::{self as gp, PixelRect};

/// 默认的棋盘缩放：`PX_PER_UNIT = CANVAS_W / BOARD_W = 1000 / 50 = 20`。
///
/// `PX_PER_UNIT` 本身是**观测**（`to_pixel` 的换算），但把它当 `draw_set_transform` 的
/// `scale` 用是**我方选择**（原版那次变换的实参没读到，见文件头对照表）。
const DEFAULT_BOARD_SCALE: f64 = gp::PX_PER_UNIT;

// ---------------------------------------------------------------------------
// 曲线喂点：常量与调用点（**都带地址**，见 `docs/16-client.md` §41）
// ---------------------------------------------------------------------------

/// `GraphwarFunctionManager::get_active_functions_on_tick`（x86 `0x456df0`）在整份 `.text` 里的
/// **唯一**调用点（`scan.py --callers` 扫 0x37b940 起 0x5d36f0 字节的全部 `call rel32`）：
/// `0x4b6339`，落在 `GraphwarPlane::draw`（`0x4b5df0`）体内。
///
/// 它就是任务书里"查 `0x456df0` 到底画什么"的答案：**它不画任何东西**——`0x456df0` 是
/// `GraphwarFunctionManager` 上的一个**取值器**，签名 `fn(&self, tick: u32) -> Vec<u32>`
/// （`rdi` = sret 输出槽、`rsi` = self、`edx` = tick；见 `0x456df8`–`0x456e8f`），
/// 谓词逐条翻在 [`is_active_function`] 里。画曲线的是它返回的那串 id 走进
/// `FunctionDrawer::update_function`（`0x498c70`）。
pub const ACTIVE_FUNCTIONS_CALL_SITE: u32 = 0x004b_6339;

/// `FunctionDrawer::update_function`（x86 `0x498c70`）的**唯一**调用点：`0x4b69ec`。
///
/// 实参（`0x4b69d4`–`0x4b69e9`）：`rdi` = 绘制器、`esi` = player id、`rdx` = 点数组指针、
/// `rcx` = 点数组长度、`r8` = `new_len`、`r9d` = 打包颜色。
/// ⇒ 原版里**喂点的就是 `GraphwarPlane::draw`**，不是别的模块。
pub const UPDATE_FUNCTION_CALL_SITE: u32 = 0x004b_69ec;

/// `FunctionDrawer::remove_function`（x86 `0x498b70`）的**唯一**调用点：`0x4b6418`
/// （同一个 `draw` 里，"不在场了"的那几条要删掉）。
pub const REMOVE_FUNCTION_CALL_SITE: u32 = 0x004b_6418;

/// `FunctionDrawer::update_transform`（`0x499050`）的唯一调用点（`0x4b6147`）——
/// **本轮仍未接**：三个实参只解出一半，见 `docs/16-client.md` §41.5。
pub const UPDATE_TRANSFORM_CALL_SITE: u32 = 0x004b_6147;

/// 每 tick "露"出多少个点（**逐位**）：`.rodata 0x19ef90` 的 8 字节是
/// `00 00 00 00 00 00 3e 40` = f64 **30.0**，调用点 `0x4b67ba` 的 `mulsd [0x19ef90]`：
///
/// ```text
/// 4b67a0  mov  -0x4(%r12),%eax        ; active_tick（u32，value+0x74）
/// 4b67a8  cvtsi2sd %rax,%xmm0
/// 4b67ad  movsd  0x130(%rsp),%xmm1    ; get_fractional_tick() 的返回值
/// 4b67b6  subsd  %xmm0,%xmm1          ; tick - active_tick
/// 4b67ba  mulsd  0x19ef90,%xmm1       ; × 30.0
/// 4b67c2  cvttsd2si …                 ; 饱和转 u64（负数 → 0）
/// 4b6812  cmp %r13,%r12 / cmovb       ; new_len = min(points.len(), 上面那个数)
/// ```
///
/// ⇒ 曲线是**逐 tick 长出来**的：每 tick 补 30 个点。
pub const REVEAL_POINTS_PER_TICK: f64 = 30.0;

/// 一条曲线"还在场"的尾巴（**观测**）：`0x3be60b` 的 `add $0x708,%edi`（= **1800**）加在
/// `points.len()` 上，再与 `30*(tick-active_tick)` 做**无符号**比较（`0x3be611` 的 `jae`）：
///
/// ```text
/// 3be5f6  mov %eax,%edx ; sub -0x4(%rcx),%edx   ; edx = tick - active_tick（u32）
/// 3be5fb  jbe  skip                             ; tick <= active_tick ⇒ 不在场
/// 3be5fd  mov -0x18(%rcx),%rdi                  ; points.len()（value+0x60）
/// 3be601  lea (%rdx,%rdx,1),%r8d / shl $5,%edx / sub %r8d,%edx   ; edx = 30*d
/// 3be60b  add $0x708,%edi                       ; points.len() + 1800
/// 3be611  cmp %edi,%edx ; jae skip              ; 30*d >= len+1800 ⇒ 不在场
/// ```
///
/// ⇒ 揭示完之后再留 **1800/30 = 60 tick**（淡出窗口，`get_function_fade_on_tick` 的分子，
/// 那个函数**没定位**，见 §41.5）。
pub const ACTIVE_TAIL_POINTS: usize = 1800;

/// 本帧该"露"几个点。逐条翻自 `0x4b67a0`–`0x4b6815`（见 [`REVEAL_POINTS_PER_TICK`]）。
///
/// 负数（tick < active_tick）⇒ 0；超过总点数 ⇒ 总点数。
/// 原版是 `cvttsd2si` + 一段饱和处理（`cvttsd2si` 对超界给 `i64::MIN`，随后 `sar`/`and`/`or`
/// 拼回"饱和到 u64::MAX"，再 `cmova` 比 `2^64`）—— Rust 的 `as u64` 正好是同一套语义。
pub fn revealed_points(fractional_tick: f64, active_tick: i64, total: usize) -> usize {
    let d = (fractional_tick - active_tick as f64) * REVEAL_POINTS_PER_TICK;
    // `f64 as u64` 正好是原版那套语义：NaN / 负数 ⇒ 0，`+∞` / 超界 ⇒ `u64::MAX`（再被
    // `points.len()` 夹住）。**不要**加 `is_finite()` 之类的过滤——那会把 `+∞` 变成 0。
    let n = d as u64;
    (n.min(usize::MAX as u64) as usize).min(total)
}

/// 本帧这发还在不在场 —— `get_active_functions_on_tick`（`0x456df0`）的谓词，
/// 逐条翻自 `0x3be5e6`–`0x3be611`（那个取值器的过滤逻辑被内联进了 `0x3be540`）。
///
/// 三条同时成立才算在场：
///
/// 1. `value+0x70 == 1`（`0x3be5e6` 的 `cmpl $0x1,-0x8(%rbp,%rcx,1)`）—— 那个字段的**名字未知**
///    （`FunctionState` 的哪个变体不知道），本类用"注入了点"代替；
/// 2. `value+0x50 != i64::MIN`（`0x3be5f0` 的 `cmp %r13,-0x28(%rcx)`；`r13 = 0x8000_0000_0000_0000`）
///    —— 某个 8 字节 `Option` 字段是 `Some`，**字段语义未知**，本类不做这个判定；
/// 3. `active_tick < tick`（**严格**，无符号）且 `30*(tick-active_tick) < points.len() + 1800`。
pub fn is_active_function(integer_tick: i64, active_tick: i64, total: usize) -> bool {
    if integer_tick <= active_tick {
        return false;
    }
    let d = (integer_tick - active_tick) as u64;
    // 30*d 与原版一样按 u32/无符号比；d 不会超过 i64 范围，转 u32 之前先夹一下。
    let revealed = d.saturating_mul(REVEAL_POINTS_PER_TICK as u64);
    revealed < (total as u64).saturating_add(ACTIVE_TAIL_POINTS as u64)
}

/// 一条函数在本类里的存储（**替代未复原的 `GraphwarFunctionManager` 的 value**）。
///
/// 字段与偏移来自 `GraphwarPlane::draw` 的读者（hashbrown bucket 步长 `0x78`，key 在 `+0x00`）：
///
/// ```text
/// 4b6670  add %r12,%rsi                        ; rsi = 本条 bucket 末尾（键在 bucket+0x00）
/// 4b66e9  mov -0x20(%rsi),%rax → 0x140(%rsp)   ; 点数组指针（update_function 的 rdx）
/// 4b66f5  mov -0x18(%rsi),%rax → 0x138(%rsp)   ; 点数组长度（update_function 的 rcx）
/// 4b6794  cmpb $0x0,-0x8(%r12)                 ; 一个 bool（=1 才继续往下喂）
/// 4b67a0  mov -0x4(%r12),%eax                  ; active_tick（u32）
/// ```
///
/// ⇒ 点数组是 **`Vec<(f64, f64)>`**：16 字节/点、`x` 在 `+0`、`y` 在 `+8`
/// （与 `src/function_drawer.rs::POINT_STRIDE` 的逐位结论一致）。
///
/// ⚠️ **这不是复原出来的结构体**：`graphwar_function_manager` 模块**还没有 `src/` 文件**
/// （`docs/coverage-matrix.tsv` 第 36 行把它算进 `src/game.rs`），所以本类先在棋盘类里放一份
/// 最小存储，好让"原版唯一的喂点路径"能真的跑起来。见 `docs/16-client.md` §41.4。
#[derive(Debug, Clone)]
struct FunctionEntry {
    /// 采样点（原版 value 里的 `Vec<(f64,f64)>`：ptr 在 `+0x58`、len 在 `+0x60`）。
    points: Vec<(f64, f64)>,
    /// 起飞 tick（原版 value+0x74，u32）。
    active_tick: i64,
    /// 队色 `0x00RRGGBB`（原版由 `get_team_overview` 的两张表查出来；本类由调用方给，见 §41.5）。
    color: u32,
    /// **我方记账**：这个 id 已经通过 `FunctionDrawer::update_function` 喂进去几个点。
    ///
    /// 原版这个数字也存在——在 `FunctionDrawer` 的 value 里（`usize` 已写入数，
    /// `docs/16-client.md` §40.3a 的 `bucket+0x28`）。本类不从那边读，因为
    /// `FunctionDrawer::written()` 是 `#[func]`（GDScript 侧的方法），Rust 侧拿不到；
    /// 记一份是为了让 [`GraphwarPlane::has_revealing_function`] 能判断
    /// "**还有该喂但没喂的点**"（§43 的验收：曲线要能揭到最后一个点）。
    /// 曲线不会变短（`append_window`），所以这个数**只增不减**；同一 id 被重新注入点数组时
    /// 归 0（重新对齐，不会造成重复追加——`append_window` 仍是唯一判定）。
    fed: usize,
}

/// `PixelRect`（**角点**：`min_x/min_y/max_x/max_y`）→ `PackedInt32Array`。
///
/// 不转成 `Rect2i`：原版存的就是角点（`psubd` / `paddd` 那两条），换成
/// `position + size` 的形状就把"原版不是 `Rect2i`"这个事实抹掉了。
fn pixels_of(r: PixelRect) -> PackedInt32Array {
    let mut out = PackedInt32Array::new();
    out.push(r.min_x);
    out.push(r.min_y);
    out.push(r.max_x);
    out.push(r.max_y);
    out
}

/// `GraphwarPlane`：棋盘本体（`Control` 子类）。画棋盘框 + 持 3 个 `FunctionDrawer` + 收脏矩形。
#[derive(GodotClass)]
#[class(base = Control)]
pub struct GraphwarPlane {
    base: Base<Control>,

    /// 棋盘单位 → 像素的缩放（默认 [`DEFAULT_BOARD_SCALE`]）。
    #[export]
    board_scale: f64,

    /// 是否把 y 轴翻过来（棋盘 y 向上、Godot 画布 y 向下）。
    #[export]
    flip_y: bool,

    /// 纯逻辑的**意图结构**：3 个 `FunctionDrawer` 的 `NodeRef` + 脏矩形队列 + ready 标志。
    ///
    /// 原版对应 3 个 `Gd<FunctionDrawer>` 字段 + 一个 `Vec<PixelRect>`（`+0x50/+0x58/+0x60`）；
    /// 本类只存 id（文件头第 1 条）。
    plane: gp::GraphwarPlane,

    /// 障碍层（`ObstacleDrawer`，`docs/16-client.md` §45）。**默认不建**，按需
    /// [`GraphwarPlane::build_obstacle_drawer`]。
    ///
    /// 原版这一层是**平面的一个普通字段**（`obstacle_drawer.rs:37:97` 的 Location 落在
    /// `GraphwarPlane::init` 里）；本类把它落成子节点，所以留一个 `Gd` 句柄——它与节点同生共死
    /// （文件头第 1 条：句柄不能比节点活得久）。
    ///
    /// ⚠️ **为什么不自动建**：既有验收脚本断言过平面的子节点数（`draw_classes_check.gd`
    /// 的 `3`、`assemble_check.gd` 的 `1`/`4`）。自动建会打破它们 ⇒ 改成显式调用。
    obstacle_drawer: Option<Gd<ObstacleDrawer>>,

    /// `_draw` 被引擎调用过几次。
    draw_calls: i64,
    /// 累计发出过几条 `draw_rect`（恒 `draw_calls`，除非 `_draw` 中途返回）。
    rect_total: i64,
    /// `_process` 被引擎调用过几次。
    process_calls: i64,
    /// 累计"刷了 ObstacleDrawer 几次"（`process_plan()` 说每帧 2 次）。
    obstacle_update_total: i64,
    /// `ready` 被引擎调用过几次。
    ready_calls: i64,
    /// `on_resized` 被调到过几次。
    resized_calls: i64,
    /// 累计从脏矩形队列里**冲掉**了几个框。
    flushed_total: i64,
    /// 因为脏矩形而请求重画的次数。
    dirty_redraws: i64,

    /// 每个 player id 一条函数（**替代未复原的 `GraphwarFunctionManager`**，见 [`FunctionEntry`]）。
    ///
    /// 用 `BTreeMap` 只为确定性：原版是 hashbrown `HashMap`，迭代顺序不参与任何判定
    /// （与 `src/function_drawer.rs::CurveStore` 同一个理由）。
    functions: std::collections::BTreeMap<i32, FunctionEntry>,

    /// 当前 tick（原版 `GraphwarTickHandler::get_fractional_tick`，x86 `0x519530`，调用点
    /// `0x4b631a`）。小数部分参与"露几个点"，整数部分参与"还在不在场"（`0x4b6317` 的
    /// `mov 0x58(%rbp),%ebx` 是同一个 tick handler 上的 u32 字段）。
    ///
    /// ⚠️ 原版是**每帧从 tick handler 模块读**；本切片没有那个模块 ⇒ 由调用方
    /// [`GraphwarPlane::set_fractional_tick`] 推进（见 §41.5）。
    fractional_tick: f64,

    /// 累计调了几次 `FunctionDrawer::update_function`（**我方计数**）。
    function_write_total: i64,
    /// 累计追加了几个点（**我方计数**，与 `FunctionDrawer::add_point_total()` 应当一致）。
    function_point_total: i64,
    /// 累计因为"不在场"而 `remove_function` 掉几条（`0x4b6418`）。
    removed_function_total: i64,
    /// 累计"本帧有曲线正在揭示"⇒ 请求重画的次数。
    reveal_redraws: i64,
}

#[godot_api]
impl IControl for GraphwarPlane {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            board_scale: DEFAULT_BOARD_SCALE,
            flip_y: true,
            // 纯逻辑的 `new()` = `Default::default()`：3 个 id 表 + 空脏队列 + ready=false。
            plane: gp::GraphwarPlane::new(),
            obstacle_drawer: None,
            draw_calls: 0,
            rect_total: 0,
            process_calls: 0,
            obstacle_update_total: 0,
            ready_calls: 0,
            resized_calls: 0,
            flushed_total: 0,
            dirty_redraws: 0,
            functions: std::collections::BTreeMap::new(),
            fractional_tick: 0.0,
            function_write_total: 0,
            function_point_total: 0,
            removed_function_total: 0,
            reveal_redraws: 0,
        }
    }

    /// `_ready`：原版在这里连 `resized` + 订 3 个事件。
    ///
    /// 本类：① 建 3 个 `FunctionDrawer` 子节点（原版在 `init`，见文件头对照表）；② 用纯逻辑给的
    /// **信号名 / 处理者名**连 `resized`；③ 3 个事件**没订**（事件名照样能从
    /// [`GraphwarPlane::ready_event_names`] 读到，只是没真的订阅）。
    fn ready(&mut self) {
        self.ready_calls += 1;

        // ① 子节点（原版 `init` 观测：3 个 FunctionDrawer）。
        self.build_children();

        // ② `ready_plan()` 同时把纯逻辑的 `ready` 标志置 true，并给出 (信号名, 处理者名)。
        let ((signal, handler), _events) = self.plane.ready_plan();
        let myself = self.to_gd();
        let callable = myself.callable(handler);
        let err = self.base_mut().connect(signal, &callable);
        if err != godot::global::Error::OK {
            godot_warn!("GraphwarPlane::ready 连接 {signal} 失败: {err:?}");
        }
    }

    /// `_process`：原版每帧把 `ObstacleDrawer::update` 刷 **2** 次并读一次队色
    /// （`process_plan() -> (2, true)`）。
    ///
    /// 本类：照 `2` 计数（没有 `ObstacleDrawer` 节点可刷），
    /// 再把脏矩形队列冲掉——冲掉过东西才 `queue_redraw()`。
    fn process(&mut self, _delta: f64) {
        self.process_calls += 1;

        let (updates, _reads_team) = self.plane.process_plan();
        self.obstacle_update_total += updates as i64;

        // 原版 `process` 每帧真的把 `ObstacleDrawer::update` 刷 **2** 次（`process_plan()` 的
        // `2`）。本类照做——**有障碍层就真刷**，没有就只有计数（§45）。同一帧刷 2 次是原版
        // 观测到的事实，不是我们的节流；为什么 2 次仍然**未知**。
        if let Some(drawer) = self.obstacle_drawer.clone() {
            let plane: Gd<Control> = self.to_gd().upcast::<Control>();
            for _ in 0..updates {
                drawer.clone().bind_mut().update(plane.clone());
            }
        }

        let flushed = self.plane.partial_updates.len() as i64;
        if flushed > 0 {
            self.plane.partial_updates.clear();
            self.flushed_total += flushed;
            self.dirty_redraws += 1;
            self.base_mut().queue_redraw();
        }

        // 曲线是**在 `_draw` 里**一帧一帧长出来的（原版：`0x4b69ec` 落在 `draw` 体内），
        // 而引擎只在"队列里有重画请求"时才调 `_draw` ⇒ 有曲线正在揭示就必须请求重画，
        // 否则那条线一帧都长不出来。
        //
        // ⚠️ **证据缺口（推断）**：原版 `process`（`0x4b83f0`）里只有
        // `ObstacleDrawer::update` ×2 + `get_team_overview`，**没有** `queue_redraw` 的直接调用，
        // 所以"谁让平面每帧重画"**没解出来**。本类按"还有该喂没喂的点就重画"补上
        // （判据与那个死角见 `has_revealing_function` 的文档），档位：我方选择。
        if self.has_revealing_function() {
            self.reveal_redraws += 1;
            self.base_mut().queue_redraw();
        }
    }

    /// `_draw`：原版 `GraphwarPlane::draw`（vaddr `0x4b5df0`）。
    ///
    /// 只有两个图元：`draw_set_transform` ×1（`0x4b607d`，槽 `0x9bc018`）+
    /// `draw_rect` ×1（`0x4b61ab`，槽 `0x9bc028`）。`draw_line` **0 次**（有界否定）。
    fn draw(&mut self) {
        self.draw_calls += 1;

        let scale = self.board_scale as f32;
        let scale_y = if self.flip_y { -scale } else { scale };

        {
            let mut base = self.base_mut();
            // 实参**未知**（原版只确证了槽）；默认值来自 `to_pixel` 的换算（`PIXEL_OFFSET`）。
            base.draw_set_transform_ex(Vector2::new(
                gp::PIXEL_OFFSET.0 as f32,
                gp::PIXEL_OFFSET.1 as f32,
            ))
            .rotation(0.0)
            .scale(Vector2::new(scale, scale_y))
            .done();

            // 四个常量全部来自 `graphwar_plane::BOARD_*`（逐位：`.rodata 0x19cc20` / `0x19cf70`
            // / `0x1ba598`，两个 `xor` 给的 `false`）。
            let rect = Rect2::new(
                Vector2::new(gp::BOARD_RECT.0, gp::BOARD_RECT.1),
                Vector2::new(gp::BOARD_RECT.2, gp::BOARD_RECT.3),
            );
            let color = Color::from_rgba(
                gp::BOARD_COLOR.0,
                gp::BOARD_COLOR.1,
                gp::BOARD_COLOR.2,
                gp::BOARD_COLOR.3,
            );
            base.draw_rect_ex(rect, color)
                .filled(gp::BOARD_FILLED)
                .width(gp::BOARD_WIDTH)
                .antialiased(gp::BOARD_ANTIALIASED)
                .done();
        }
        self.rect_total += 1;

        // 原版 `draw` 在画完棋盘框之后（`0x4b61ab` 之后）紧接着就是那一段喂点循环
        // （`0x4b6339` → `0x4b69ec`）。本类放在这里，位置与原版一致。
        self.feed_functions();
    }
}

#[godot_api]
impl GraphwarPlane {
    /// 建 3 个 `FunctionDrawer` 子节点（`FUNCTION_DRAWER_CHILDREN`）。幂等：已经有就不重复建。
    ///
    /// 名字**不自拟**：原版没 `set_name` ⇒ 引擎自动名（运行时是 `@FunctionDrawer@N`）。
    ///
    /// 返回子节点个数（恒 `3`）。
    #[func]
    fn build_children(&mut self) -> i64 {
        if !self.plane.function_drawers.is_empty() {
            return self.plane.function_drawers.len() as i64;
        }

        // 父节点取成**局部** `Gd<Node>`：闭包里就不必再借 `self`（`base_mut()` 是独占借用）。
        let mut parent: Gd<Node> = self.to_gd().upcast::<Node>();

        // `init_children` 把闭包返回的 id 依次记进 `plane.function_drawers`（纯逻辑的意图结构）。
        self.plane.init_children(|_class_name| {
            // 原版这里是 `Gd::default_instance::<FunctionDrawer>()`；现在是**真的**
            // `FunctionDrawer` 节点类（`docs/16-client.md` §38）。
            let child = FunctionDrawer::new_alloc();
            let node = child.clone().upcast::<Node>();
            parent.add_child(&node);
            // 原版没 `set_name` ⇒ 自动名（`@FunctionDrawer@N`）。
            // 句柄到这里就可以丢掉：节点归**父节点**所有（文件头第 1 条）。
            // `InstanceId::to_i64()` 就是原版 `NodeRef` 要的那个 u64（纯逻辑只拿它当不透明 id）。
            child.instance_id().to_i64() as u64
        });
        debug_assert_eq!(self.plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        self.plane.function_drawers.len() as i64
    }

    /// 纯逻辑记下的子节点数（`FUNCTION_DRAWER_CHILDREN` = 3）；还没建时是 0。
    #[func]
    fn function_drawer_count(&self) -> i64 {
        self.plane.function_drawers.len() as i64
    }

    // -----------------------------------------------------------------------
    // 障碍层（`ObstacleDrawer`，`docs/16-client.md` §45）
    // -----------------------------------------------------------------------

    /// 建障碍层：一个 [`ObstacleDrawer`] 子节点。幂等：已经有了就直接返回 `1`。
    ///
    /// 返回：平面上现在有几个 `ObstacleDrawer` 子节点（`0` / `1`）。
    ///
    /// **为什么是显式调用而不是 `ready` 里自动建**：见字段 `obstacle_drawer` 的注释
    /// （既有脚本断言过子节点数）。建完由 [`GraphwarPlane::process`] 每帧刷 2 次。
    #[func]
    fn build_obstacle_drawer(&mut self) -> i64 {
        if self.obstacle_drawer.is_some() {
            return 1;
        }
        let drawer = ObstacleDrawer::new_alloc();
        let node = drawer.clone().upcast::<Node>();
        self.base_mut().add_child(&node);
        self.obstacle_drawer = Some(drawer);
        1
    }

    /// 障碍层建了吗（`0` / `1`）。
    #[func]
    fn obstacle_drawer_count(&self) -> i64 {
        i64::from(self.obstacle_drawer.is_some())
    }

    /// 障碍层自己记的 `update` 次数（**由那一侧计数**，用来证明"平面真的在每帧驱动它"）。
    #[func]
    fn obstacle_drawer_updates(&self) -> i64 {
        match self.obstacle_drawer.clone() {
            Some(drawer) => drawer.bind().update_call_count(),
            None => 0,
        }
    }

    /// 棋盘坐标 → 像素（`graphwar_plane::to_pixel`：`x*1000/50 + 500`、`y*1000/50 + 300`）。
    #[func]
    fn to_pixel(&self, x: f64, y: f64) -> Vector2i {
        let (px, py) = gp::to_pixel(x, y);
        Vector2i::new(px, py)
    }
    /// 收一个脏矩形：`obstacle_partial_update_event` 的纯数据版。
    ///
    /// 返回 `PackedInt32Array[min_x, min_y, max_x, max_y]`（**角点**，不是 `Rect2i`）。
    ///
    /// ⚠️ 坏 tag 原版是 **`panic!()`**；本类**先拦**并返回空数组（panic 穿过 FFI 会 abort
    /// 整个进程）——这是一处**故意**与原版不同的行为，见文件头对照表。
    #[func]
    fn on_obstacle_partial_update(
        &mut self,
        tag: i64,
        x: f64,
        y: f64,
        size: f64,
    ) -> PackedInt32Array {
        let tag = tag.clamp(0, u32::MAX as i64) as u32;
        if !gp::is_partial_update_tag(tag) {
            godot_warn!(
                "GraphwarPlane::on_obstacle_partial_update 坏 tag {tag}（原版在这里 panic!）"
            );
            return PackedInt32Array::new();
        }
        pixels_of(self.plane.on_obstacle_partial_update(tag, x, y, size))
    }

    /// 脏矩形队列现在有几个。
    #[func]
    fn partial_update_count(&self) -> i64 {
        self.plane.partial_updates.len() as i64
    }

    // -----------------------------------------------------------------------
    // 函数曲线：喂点（原版 `GraphwarPlane::draw` 尾部那段循环，`0x4b6339` → `0x4b69ec`）
    // -----------------------------------------------------------------------

    /// 推进到某个 tick（小数部分参与"露几个点"，整数部分参与"还在不在场"）。
    ///
    /// 原版是每帧自己从 `GraphwarTickHandler` 读两样东西：`0x4b6317` 的 `mov 0x58(%rbp),%ebx`
    /// （u32 tick，喂给 `get_active_functions_on_tick`）与 `0x4b631a` 的
    /// `call 0x519530` = `get_fractional_tick`（f64，参与 `new_len`）。本切片没有那个模块 ⇒
    /// 由调用方喂一个 f64，整数 tick 取 `trunc()`。
    ///
    /// 返回设置后的值。
    #[func]
    fn set_fractional_tick(&mut self, tick: f64) -> f64 {
        self.fractional_tick = if tick.is_finite() { tick } else { 0.0 };
        self.fractional_tick
    }

    /// 现在的 tick（`set_fractional_tick` 设进去的那个）。
    #[func]
    fn fractional_tick(&self) -> f64 {
        self.fractional_tick
    }

    /// 注入一条函数的采样点。
    ///
    /// ⚠️ **这不是原版 `GraphwarPlane` 的方法**：原版这三件事分别属于
    /// `GraphwarFunctionManager::set_function_points`（`0x456810`）、
    /// `set_function_active_tick`（`0x456b80`）与队色（`GraphwarDrawUtil::get_team_overview`，
    /// `0x4be6e0`，`draw` 里调 2 次）。那个模块**还没有 `src/` 文件**，而本轮只许改这个文件 ⇒
    /// 先在这里放一份最小存储 + 入口，好让原版**唯一的**喂点路径能真的跑起来（§41.4）。
    ///
    /// * `points`：**扁平 f64 数组** `[x0, y0, x1, y1, …]`。原版的点是 `(f64, f64)`（16 B/点，
    ///   `x` 在 `+0`、`y` 在 `+8`）——用 f64 而不是 `PackedVector2Array` 是为了不在入口就掉精度；
    ///   奇数长度时**丢掉最后一个**。
    /// * `active_tick`：起飞 tick（原版 value+0x74 的 u32）。
    /// * `color`：`0x00RRGGBB`，只在第一次建 `Line2D` 时用（同 `FunctionDrawer::update_function`）。
    ///
    /// 返回存下来的点数；`player_id` 超出 i32 ⇒ `-1`。
    ///
    /// 同一 id 重复注入 ⇒ **覆盖**点数组。已经画出去的点不会缩回去（`update_function` 的
    /// "变短不加点"，`0x498d5a`）——要让曲线从 0 重来，得先让它离开活跃窗口（或重建节点）。
    #[func]
    fn set_function_points(
        &mut self,
        player_id: i64,
        points: PackedFloat64Array,
        active_tick: i64,
        color: i64,
    ) -> i64 {
        let Ok(id) = i32::try_from(player_id) else {
            godot_warn!("GraphwarPlane::set_function_points player_id {player_id} 超出 i32");
            return -1;
        };
        let flat = points.as_slice();
        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(flat.len() / 2);
        let mut i = 0;
        while i + 1 < flat.len() {
            pts.push((flat[i], flat[i + 1]));
            i += 2;
        }
        let len = pts.len() as i64;
        self.functions.insert(
            id,
            FunctionEntry {
                points: pts,
                active_tick,
                color: (color as u32) & 0x00ff_ffff,
                fed: 0,
            },
        );
        len
    }

    /// 清掉所有函数（下次 `draw` 会把对应的 `Line2D` 也删掉）。
    ///
    /// 返回清掉几条。
    #[func]
    fn clear_function_points(&mut self) -> i64 {
        let n = self.functions.len() as i64;
        self.functions.clear();
        n
    }

    /// 本 tick 还在场的函数 id（= `get_active_functions_on_tick`（`0x456df0`）的返回集）。
    ///
    /// 谓词见 [`is_active_function`]（三条，逐条翻自 `0x3be5e6`–`0x3be611`）。
    #[func]
    fn active_function_ids(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        let tick = self.integer_tick();
        for (id, entry) in &self.functions {
            if is_active_function(tick, entry.active_tick, entry.points.len()) {
                out.push(*id);
            }
        }
        out
    }

    /// **原版 `GraphwarPlane::draw` 尾部那段喂点循环**（`0x4b6339`–`0x4b6a13`）。
    ///
    /// ```text
    /// for id in function_manager.get_active_functions_on_tick(tick)   ; 0x4b6339
    ///     entry = manager[id]                                         ; 0x4b65d9 起的 hashbrown 查表
    ///     if entry.state != 1 || entry.opt == i64::MIN { continue }    ; 0x3be5e6 / 0x3be5f0
    ///     new_len = min(points.len(), 30*(fractional_tick - entry.active_tick))
    ///                                                                 ; 0x4b67a0–0x4b6812
    ///     drawer.update_function(id, points, points.len, new_len, color)
    ///                                                                 ; 0x4b69ec
    ///     drawer.update_function_fade(id, get_function_fade_on_tick(id, tick))
    ///                                                                 ; 0x4b69fe + 0x4b6a0e
    /// for id in <不再在场的>                                            ; 0x4b6410
    ///     drawer.remove_function(id)                                  ; 0x4b6418
    /// ```
    ///
    /// **本类做到的**：① 活跃谓词（三条里第 3 条）；② `new_len` 的算式与常量；
    /// ③ `update_function` 的实参形状（点前缀 + 队色）；④ 不再在场的走 `remove_function`。
    ///
    /// **本类做不到 / 故意没做**：
    /// * `entry.state != 1` 与 `entry.opt != i64::MIN` 两条判定（字段语义未知，§41.5）；
    /// * `update_function_fade`：`get_function_fade_on_tick` **没有 x86 落点**（走 GOT 槽
    ///   `0x9bbd08`，是个方法指针调用），alpha 的算式没解出来 ⇒ 不发明；
    /// * 喂给**哪一条** `FunctionDrawer`：原版三次调用都用 `self+0x80` 那**一个**
    ///   （`0x4b6132` 的 `lea 0x80(%rax),%rdi`）⇒ 本类固定喂第 0 个子节点（"`self+0x80` 是 3 个里的
    ///   第一个"是**推断**：3 个 `Gd` 字段的排列没有直接证据）。
    ///
    /// 返回本帧追加了几个点（0 = 没有新增，-1 = 子节点还没建）。
    #[func]
    fn feed_functions(&mut self) -> i64 {
        if self.functions.is_empty() {
            return 0;
        }
        let Some(mut drawer) = self.function_drawer(0) else {
            return -1;
        };
        let tick = self.integer_tick();
        let ftick = self.fractional_tick;

        // ① 已经不在场的：`remove_function`（`0x4b6418`）。
        let stale: Vec<i32> = self
            .functions
            .iter()
            .filter(|(_, e)| !is_active_function(tick, e.active_tick, e.points.len()))
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if drawer.bind_mut().remove_function(id as i64) {
                self.removed_function_total += 1;
            }
            self.functions.remove(&id);
        }

        // ② 还在场的：喂点。
        let ids: Vec<i32> = self.functions.keys().copied().collect();
        let mut appended = 0i64;
        for id in ids {
            let mut prefix = PackedVector2Array::new();
            let color;
            let new_len;
            {
                let entry = &self.functions[&id];
                new_len = revealed_points(ftick, entry.active_tick, entry.points.len());
                for point in entry.points.iter().take(new_len) {
                    // 原版：`movsd` ×2 + `cvtsd2ss` ⇒ `add_point(Vector2, -1)`。
                    prefix.push(Vector2::new(point.0 as f32, point.1 as f32));
                }
                color = entry.color as i64;
            }
            // `new_len` 在这里就是 `prefix.len()`：原版把全长与 `new_len` 分开传，
            // 而 `FunctionDrawer` 只补 `points[written..new_len]` ⇒ 传前缀是**等价**的
            // （§40.4b 已记过这条；`append_window` 仍是唯一判定）。
            appended += drawer.bind_mut().update_function(id as i64, prefix, color);
            // 记账：本 id 现在"该喂到"哪一点（只增；曲线不会变短）。
            if let Some(entry) = self.functions.get_mut(&id) {
                entry.fed = entry.fed.max(new_len);
            }
            self.function_write_total += 1;
        }
        self.function_point_total += appended;
        appended
    }

    /// 存着几条函数。
    #[func]
    fn function_count(&self) -> i64 {
        self.functions.len() as i64
    }

    /// 某条函数现在存了几个点（没有这个 id ⇒ `-1`）。
    #[func]
    fn function_point_count(&self, player_id: i64) -> i64 {
        i32::try_from(player_id)
            .ok()
            .and_then(|id| self.functions.get(&id))
            .map(|e| e.points.len() as i64)
            .unwrap_or(-1)
    }

    /// 累计调了几次 `FunctionDrawer::update_function`（**我方计数**）。
    #[func]
    fn function_write_total(&self) -> i64 {
        self.function_write_total
    }

    /// 累计追加了几个点（**我方计数**，应与 `FunctionDrawer::add_point_total()` 相等）。
    #[func]
    fn function_point_total(&self) -> i64 {
        self.function_point_total
    }

    /// 累计因为"不在场"而删掉几条线（`0x4b6418`）。
    #[func]
    fn removed_function_total(&self) -> i64 {
        self.removed_function_total
    }

    /// 因为"有曲线正在揭示"而请求重画的次数（`_process`）。
    #[func]
    fn reveal_redraw_count(&self) -> i64 {
        self.reveal_redraws
    }

    /// 冲刷脏矩形队列，返回冲掉几个（`_process` 每帧也会自动冲一次）。
    #[func]
    fn drain_partial_updates(&mut self) -> i64 {
        let n = self.plane.partial_updates.len() as i64;
        self.plane.partial_updates.clear();
        self.flushed_total += n;
        n
    }

    /// 原版 `ready` 会按顺序订阅的 3 个事件名（`READY_EVENTS`）——**本类没订**，只读名字。
    #[func]
    fn ready_event_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for (name, _slot) in gp::READY_EVENTS {
            out.push(&GString::from(name));
        }
        out
    }

    /// `resized` 信号的处理者（原版名字就是 `on_resized`，`.rodata 0x1c2a7a`）。
    ///
    /// 它是 `#[func]` ⇒ GDScript 也能直接调（验收脚本用它验"树外不会自动触发"）。
    #[func]
    fn on_resized(&mut self) {
        self.resized_calls += 1;
        // 原版这里做 3 次 `Control::set_*`，方法名未解（`UNKNOWNS` 第 3 条）⇒ 只重画。
        self.base_mut().queue_redraw();
    }

    /// `_draw` 被引擎调用过几次。
    #[func]
    fn draw_call_count(&self) -> i64 {
        self.draw_calls
    }

    /// 累计发出过几条 `draw_rect`。
    #[func]
    fn drawn_rect_total(&self) -> i64 {
        self.rect_total
    }

    /// `_process` 被引擎调用过几次。
    #[func]
    fn process_call_count(&self) -> i64 {
        self.process_calls
    }

    /// 累计"刷了障碍绘制器几次"（每帧 `OBSTACLE_DRAWER_UPDATES_PER_FRAME` = 2 次）。
    #[func]
    fn obstacle_update_total(&self) -> i64 {
        self.obstacle_update_total
    }

    /// `_ready` 被引擎调用过几次。
    #[func]
    fn ready_call_count(&self) -> i64 {
        self.ready_calls
    }

    /// `on_resized` 被调到过几次。
    #[func]
    fn resized_call_count(&self) -> i64 {
        self.resized_calls
    }

    /// 累计从脏矩形队列里冲掉过几个框。
    #[func]
    fn flushed_total(&self) -> i64 {
        self.flushed_total
    }

    /// 因为脏矩形而请求重画的次数。
    #[func]
    fn dirty_redraw_count(&self) -> i64 {
        self.dirty_redraws
    }
}

/// 不走 `#[godot_api]` 的内部小工具（`#[godot_api]` 块里只能放 `#[func]` / signal / constant）。
impl GraphwarPlane {
    /// 整数 tick：原版是 `[tick_handler+0x58]` 的 u32（`0x4b6317`）。本类从同一个 f64 取整数部分。
    fn integer_tick(&self) -> i64 {
        let t = self.fractional_tick.trunc();
        if t <= 0.0 {
            0
        } else if t >= u32::MAX as f64 {
            u32::MAX as i64
        } else {
            t as i64
        }
    }

    /// 第 `index` 个 `FunctionDrawer` 子节点（原版 `init` 建 3 个）。
    ///
    /// 不存 `Gd` 句柄：本类只记 instance id（文件头第 1 条），用的时候从 `base()` 现取。
    fn function_drawer(&self, index: i32) -> Option<Gd<FunctionDrawer>> {
        let child: Gd<Node> = self.base().get_child(index)?;
        child.try_cast::<FunctionDrawer>().ok()
    }

    /// 还有"该喂但没喂"的点 ⇒ `_process` 要请求重画。
    ///
    /// **判据是"已喂 vs 应喂"，不是"应喂 < 全长"**。早先写的是后者
    /// （`revealed_points(...) < points.len()`），它有一个真实的死角：当
    /// `revealed_points` 第一次**恰好等于全长**时（`30*t >= len`，而 `len` 往往不是 30 的
    /// 整数倍），这一帧已经算"揭完"⇒ `_process` 不再请求重画 ⇒ 引擎不会为它再调一次
    /// `_draw` ⇒ **最后不足 30 个点永远喂不进去**（`docs/16-client.md` §43 里 5314 点的
    /// 真实弹道就是卡在 5310）。改成比 [`FunctionEntry::fed`]（本 id 已喂到第几点）：
    /// `应喂 > 已喂` 就必须再来一帧。
    ///
    /// ⚠️ 与 [`GraphwarPlane::process`] 里那条一样，这是**我方选择**：原版 `process`
    /// （`0x4b83f0`）里**没有** `queue_redraw` 的直接调用，"谁让平面每帧重画"仍未解
    /// （§42.5 第 6 条）。
    fn has_revealing_function(&self) -> bool {
        let tick = self.integer_tick();
        self.functions.values().any(|e| {
            is_active_function(tick, e.active_tick, e.points.len())
                && revealed_points(self.fractional_tick, e.active_tick, e.points.len()) > e.fed
        })
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只钉住「本类用的是哪份常量/算式」）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 3 个子节点这个数字来自纯逻辑，不是本类自己写的 3。
    #[test]
    fn child_count_comes_from_the_pure_module() {
        assert_eq!(gp::FUNCTION_DRAWER_CHILDREN, 3);
        let mut plane = gp::GraphwarPlane::new();
        let mut n = 0;
        plane.init_children(|_name| {
            n += 1;
            n as u64
        });
        assert_eq!(plane.function_drawers.len(), gp::FUNCTION_DRAWER_CHILDREN);
        assert_eq!(plane.function_drawers, vec![1, 2, 3]);
    }

    /// 默认缩放取的是 `PX_PER_UNIT`（1000/50 = 20），棋盘框是 50×30 个单位 ⇒ 1000×600 像素。
    #[test]
    fn default_scale_maps_the_board_onto_the_canvas() {
        assert!((DEFAULT_BOARD_SCALE - 20.0).abs() < 1e-9);
        assert!((gp::BOARD_RECT.2 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_W as f32).abs() < 1e-3);
        assert!((gp::BOARD_RECT.3 * DEFAULT_BOARD_SCALE as f32 - gp::CANVAS_H as f32).abs() < 1e-3);
        // 棋盘框的角在像素空间里正好是 (0,0) 与 (1000,600)。
        let (x0, y0) = gp::to_pixel(gp::BOARD_RECT.0 as f64, gp::BOARD_RECT.1 as f64);
        assert_eq!((x0, y0), (0, 0), "左下角 → 画布原点");
        let (x1, y1) = gp::to_pixel(
            (gp::BOARD_RECT.0 + gp::BOARD_RECT.2) as f64,
            (gp::BOARD_RECT.1 + gp::BOARD_RECT.3) as f64,
        );
        assert_eq!((x1, y1), (1000, 600), "右上角 → 画布右下角");
    }

    /// 脏矩形的换算与手算一致，且**只接受两个 tag**。
    #[test]
    fn partial_update_rect_matches_the_hand_computed_pixels() {        // (5, 5) → (600, 400)；半径 = size*1000/50 = 1.5*20 = 30。
        let mut plane = gp::GraphwarPlane::new();
        let r = plane.on_obstacle_partial_update(gp::ENTITY_TAG_A, 5.0, 5.0, 1.5);
        assert_eq!(
            r,
            PixelRect {
                min_x: 570,
                min_y: 370,
                max_x: 630,
                max_y: 430
            }
        );
        assert_eq!((r.width(), r.height()), (60, 60), "正方形，不是圆");
        assert_eq!(plane.partial_updates.len(), 1);
        // 第二个合法 tag 也收。
        plane.on_obstacle_partial_update(gp::ENTITY_TAG_B, -5.0, -5.0, 0.5);
        assert_eq!(plane.partial_updates.len(), 2);
        assert!(!gp::is_partial_update_tag(0));
        assert!(!gp::is_partial_update_tag(0x27));
    }

    /// `ready_plan()` 给出的信号名 / 处理者名 / 3 个事件名就是本类用的那几个字符串。
    #[test]
    fn ready_plan_supplies_the_names_the_node_class_uses() {
        let mut plane = gp::GraphwarPlane::new();
        let ((signal, handler), events) = plane.ready_plan();
        assert_eq!(signal, "resized");
        assert_eq!(handler, "on_resized");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], "add_obstacle_event");
        assert_eq!(events[1], "destroy_obstacle_event");
        assert_eq!(events[2], "clear_obstacles_event");
        assert!(plane.ready, "ready_plan() 会把标志置 true");
    }

    /// `process_plan()` 说每帧刷 2 次、读一次队色；本类只用了那个 2。
    #[test]
    fn process_plan_reports_two_obstacle_updates_per_frame() {
        let plane = gp::GraphwarPlane::new();
        assert_eq!(
            plane.process_plan(),
            (gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, true)
        );
        assert_eq!(gp::OBSTACLE_DRAWER_UPDATES_PER_FRAME, 2);
    }

    // -----------------------------------------------------------------------
    // 曲线喂点：常量与算式（**不碰引擎**）
    // -----------------------------------------------------------------------

    /// `30.0` 是从 `.rodata 0x19ef90` 逐位读出来的，不是我方挑的数。
    #[test]
    fn reveal_rate_is_the_bit_exact_rodata_constant() {
        assert_eq!(REVEAL_POINTS_PER_TICK, 30.0);
        assert_eq!(REVEAL_POINTS_PER_TICK.to_bits(), 0x403e_0000_0000_0000);
        // `0x708` = 1800（`0x3be60b` 的 `add $0x708,%edi`）。
        assert_eq!(ACTIVE_TAIL_POINTS, 1800);
        assert_eq!(ACTIVE_TAIL_POINTS / REVEAL_POINTS_PER_TICK as usize, 60, "60 tick 的淡出窗");
    }

    /// `new_len = min(points.len(), trunc(30*(fractional_tick - active_tick)))`（`0x4b67a0`–`0x4b6812`）。
    #[test]
    fn revealed_points_follows_the_decoded_formula() {
        // 还没起飞 ⇒ 0（负数走的是 0，不是回绕）。
        assert_eq!(revealed_points(9.0, 10, 100), 0);
        assert_eq!(revealed_points(10.0, 10, 100), 0);
        // 起飞 2 个 tick ⇒ 60 点。
        assert_eq!(revealed_points(12.0, 10, 100), 60);
        // 小数部分参与（原版是 f64 乘 30 再截断）。
        assert_eq!(revealed_points(12.5, 10, 100), 75);
        assert_eq!(revealed_points(12.999, 10, 100), 89, "89.97 ⇒ 截断成 89");
        // 满了就封顶（`0x4b6812` 的 `cmovb`）。
        assert_eq!(revealed_points(100.0, 10, 100), 100);
        // NaN / 无穷不 panic。
        assert_eq!(revealed_points(f64::NAN, 10, 100), 0);
        assert_eq!(revealed_points(f64::INFINITY, 10, 100), 100);
    }

    /// 活跃谓词（`0x3be5f6`–`0x3be611`）：`active_tick < tick` 且 `30*d < len + 1800`。
    #[test]
    fn active_predicate_matches_the_decoded_window() {
        // 严格大于：tick == active_tick 不算在场（`jbe`）。
        assert!(!is_active_function(10, 10, 100));
        assert!(!is_active_function(9, 10, 100));
        assert!(is_active_function(11, 10, 100));
        // 1800 点 = 60 tick 的尾巴：len=100 ⇒ 100+1800=1900 ⇒ d < 1900/30 = 63.33 ⇒ d ≤ 63。
        assert!(is_active_function(10 + 63, 10, 100), "d=63 ⇒ 1890 < 1900");
        assert!(!is_active_function(10 + 64, 10, 100), "d=64 ⇒ 1920 >= 1900");
        // 空曲线也有 60 tick 的尾巴（len=0 ⇒ 只留 1800/30 = 60）。
        assert!(is_active_function(10 + 59, 10, 0));
        assert!(!is_active_function(10 + 60, 10, 0));
    }
}
