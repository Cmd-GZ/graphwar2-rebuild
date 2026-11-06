//! 内核 → GDScript 的对外 API：`GW2Rebuild.path_calculate(...)`。
//!
//! 契约写在客户端接缝 `client/src/core/core_bridge.gd` 的文件头（以及
//! `docs/108-client-rebuild.md` §2.3）里，这里**逐字**满足它：
//!
//! ```text
//! GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float)
//!     -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
//! ```
//!
//! ## 坐标系（**这是最容易错的地方**）
//!
//! `points` 用**棋盘（平面）坐标**，不是画布像素坐标。依据是调用方：
//! `client/src/screens/play_screen.gd::fire()` 把 `active_soldier.pos`（棋盘坐标）传进来，
//! 拿到 `pts` 后直接 `_plane.set_path(pts, …)`；而 `GwPlaneView.board_to_screen()` 才做
//! `屏幕 = 中心 + p * scale` 的换算（`client/src/play/plane_view.gd`）。
//! 旧的界面预览 `GwCore._preview()` 也是棋盘坐标（它用 `|x| <= 25`、`|y| <= 15` 判界）。
//! 所以这里**不做** `px = 20x+500` 这类像素换算 —— 做了的话整条弹道会整体偏 500/300 像素。
//!
//! `direction` 的语义与 `core_bridge.gd` 一致：**正 = 向右（team 0），负 = 向左（team 1）**，
//! 负值对应采样器的 `flip`（`docs/105`）。`flip` 同时决定修正常数取
//! `c = y0 − f(−x0)` 而不是 `c = y0 − f(x0)`（`docs/05` §2）。
//!
//! ## 只用真采样器
//!
//! 点序列**全部**来自 [`crate::graphwar_path_calculator::calculate`]（`@0x3c2530` 的复原）：
//! 步长 [`crate::STEP_DEFAULT`]（0.01）、容差 [`crate::LENGTH_TOL`]、
//! 点数预算 `(200/step) as i32`、平面边界 `|x|<=25 / |y|<=15`、修正常数
//! [`crate::graphwar_shot_calculator::correction`]。本文件**不重新采样**，只做类型转换与错误翻译。
//!
//! ## 不 panic
//!
//! 跨 FFI 的 panic 会直接崩掉 Godot（Rust 的 abort 无法被引擎捕获），所以：
//! 1. 所有错误都走 `ok: false + error`；
//! 2. [`path_calculate_call`] 整个函数体包在 `catch_unwind` 里，兜底再把 panic 翻成
//!    `ok: false, error: "internal_panic"`。
//!
//! ## Godot ABI（实测，见 `docs/116-kernel-api.md` §3）
//!
//! * `call_func` 的 `args` 是 `const Variant *` 数组，`ret` 是一个**空 Variant 槽**，
//!   必须用 `get_variant_from_type_constructor` **placement new** 一个 Variant 进去；
//! * `GDExtensionPropertyInfo` 里的 `class_name` / `hint_string` **不能是 NULL 指针**
//!    —— 引擎会解引用它们（实测：传 NULL 直接 SIGSEGV），必须指向合法的空
//!    `StringName` / `String`；
//! * 内置类型（`Dictionary`/`PackedVector2Array`）**可以用 `variant_call` 调方法**
//!   （`dict.set(k,v)`、`packed.append(v)`），不需要 `variant_get_ptr_builtin_method`
//!   所需的编译期 hash —— 这是本实现能不依赖 godot-rust 的关键；
//! * `variant_set_keyed` 在 4.7.2 上签名与头文件推断不符（实测 SIGSEGV），**不用**。
//!
//! ## 微分方程模式（`docs/138-ode-api.md`）
//!
//! 4 参数的 `path_calculate` 只跑 [`Mode::Normal`]；`Mode::Ode`（`dy/dx = f(x,y)`，
//! 一阶 RK4）由**另一个**方法 [`calculate_path_mode`] 暴露成
//! `GW2Rebuild.path_calculate_mode(expr, x0, y0, direction, mode)`：
//! 第 5 个参数是**字符串** `"normal"` / `"ode"`（大小写不敏感、忽略两侧空白），
//! 非法值走 `ok:false, error:"bad_arguments"`（沿用既有错误码，不新造）。
//! `"normal"` 直接**委托给** [`calculate_path`]（所以逐位相同，不是第二份实现）；
//! `"ode"` 走 [`calculate_path_ode`]，**不做**修正常数（与 [`crate::graphwar_shot_calculator::shot_points`]
//! 里 `apply_correction && mode == Mode::Normal` 一致），`offset` 恒为 `NaN`。

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::math_function::parse;
use crate::graphwar_path_calculator::calculate;
use crate::graphwar_shot_calculator::{correction, shifted_tokens};
use crate::graphwar_path_calculator::Mode;
use crate::{Token, MAX_ABS_X, MAX_ABS_Y, STEP_DEFAULT};

// ---------------------------------------------------------------------------
// 1. 纯 Rust 层（可以脱离 Godot 单元测试）
// ---------------------------------------------------------------------------

/// `path_calculate` 的结果：契约里的四个字段加上内部用的点数。
#[derive(Debug, Clone, PartialEq)]
pub struct PathOutcome {
    pub ok: bool,
    pub points: Vec<(f64, f64)>,
    /// 修正常数 `c = y0 − f(±x0)`；出错时为 `NaN`。
    pub offset: f64,
    /// 空字符串表示成功；否则是机器可读的错误码（见 [`crate::api`] 文档）。
    pub error: &'static str,
}

impl PathOutcome {
    fn err(code: &'static str) -> Self {
        PathOutcome { ok: false, points: Vec::new(), offset: f64::NAN, error: code }
    }
}

/// `direction >= 0` 向右飞（`flip = false`），否则向左（`flip = true`）。
///
/// 与 `core_bridge.gd` 的 `_preview()` 里 `1.0 if direction >= 0.0 else -1.0` 同一判据：
/// 客户端只在“向左”时才传负数，0 归右侧。
pub fn direction_is_flip(direction: f64) -> bool {
    direction < 0.0
}

/// 错误码：表达式里解析不出任何 token（例如 `"@@@"`）。
pub const E_EMPTY: &str = "empty_expression";
/// 错误码：起点不在平面内（`|x| > 25` 或 `|y| > 15`）。
pub const E_START_BOUNDS: &str = "start_out_of_bounds";
/// 错误码：起点坐标不是有限数（NaN/±inf）。
pub const E_START_FINITE: &str = "start_not_finite";
/// 错误码：函数在起点 `±x0` 处无定义（修正常数不是有限数，如 `sqrt(x)` 在 `x=-1`）。
pub const E_UNDEFINED_AT_START: &str = "undefined_at_start";
/// 错误码：一个点都没走出去（起点之后立刻越界，或者第一段就不收敛）。
pub const E_NO_POINTS: &str = "no_points";
/// 错误码：FFI 参数不是约定的类型/个数。
pub const E_BAD_ARGS: &str = "bad_arguments";
/// 错误码：兜底（Rust 侧 panic 被 `catch_unwind` 接住）。
pub const E_PANIC: &str = "internal_panic";

/// 算一条弹道。**这是 `path_calculate` 的全部语义**，FFI 层只是搬运。
///
/// 唯一的行为决策在这里：先用 [`correction`] 求 `c`，再用 [`shifted_tokens`] 得到
/// `f(x)+c` 的 token 列表，最后交给 [`calculate`] —— 与
/// [`crate::graphwar_shot_calculator::shot_points`] 完全相同的三步，所以结果逐位一致
/// （单测 `matches_shot_points_pipeline` 钉住了这一点）。
pub fn calculate_path(expr: &str, x0: f64, y0: f64, direction: f64) -> PathOutcome {
    if !x0.is_finite() || !y0.is_finite() {
        return PathOutcome::err(E_START_FINITE);
    }
    // 采样器把 (x0, y0) 无条件塞成首点，之后再逐点检查边界；
    // 起点本身越界时那条曲线整体在平面外，直接拒绝（AGENTS.md 的 |x|<=25 / |y|<=15）。
    if x0.abs() > MAX_ABS_X || y0.abs() > MAX_ABS_Y {
        return PathOutcome::err(E_START_BOUNDS);
    }

    let tokens: Vec<Token> = parse(expr);
    if tokens.is_empty() {
        // 原程序能画 `""`（抓包里 `"function":""` → 修正文本 `" + 10.990"`，水平直线），
        // 但 `graphwar_shot_calculator::shot_points` 与 C ABI `gw2_shot_points` 都按“空 = 没有点”处理，
        // 客户端也在调内核之前就把空串拦下了。这里保持与既有内核一致：
        // 空表达式返回错误而不是直线（`docs/116` §6 记为待定）。
        return PathOutcome::err(E_EMPTY);
    }

    let flip = direction_is_flip(direction);
    let offset = correction(&tokens, x0, y0, flip);
    if !offset.is_finite() {
        return PathOutcome::err(E_UNDEFINED_AT_START);
    }

    let shifted = shifted_tokens(&tokens, offset);
    let points = calculate(&shifted, x0, y0, STEP_DEFAULT, flip, Mode::Normal);

    if points.len() < 2 {
        // 首点之后的第一段就没被接受（越界、NaN、或 100 次二分仍不收敛）。
        // 采样器不区分这三种，这里也不猜。
        return PathOutcome { ok: false, points, offset, error: E_NO_POINTS };
    }
    PathOutcome { ok: true, points, offset, error: "" }
}

// ---------------------------------------------------------------------------
// 1b. 微分方程模式：`path_calculate_mode(expr, x0, y0, direction, mode)`
// ---------------------------------------------------------------------------
//
// `docs/116` §8.3 留的坑：`Mode::Ode` 在核心里有实现（`src/graphwar_path_calculator.rs` 的 RK4 分支），
// 但**没有**暴露给 GDScript，所以微分方程局只能画士兵/障碍、弹道留空
// （`docs/133` §6 #3）。这里补上入口，**不改** `path_calculate` 的签名与行为。

/// 解析模式名（`path_calculate_mode` 的第 5 个参数）。
///
/// 接受的是**字符串**，不是整数：
/// * `"normal"` → [`Mode::Normal`]：`y = f(x)`，等弦长采样 + 修正常数 `c = y0 − f(±x0)`；
/// * `"ode"` → [`Mode::Ode`]：`dy/dx = f(x, y)`，一阶 RK4，**没有**修正常数。
///
/// 大小写不敏感、忽略两侧空白（`" ODE "` 等价于 `"ode"`）；其余任何输入（空串、
/// `"0"`、`"diffeq"`…）返回 `None` —— 调用方把它翻成 `bad_arguments`。
///
/// 为什么不做整数重载：FFI 侧 `PropertyInfo` 把这个参数声明成 `String`，GDScript 传
/// 整数会被**引擎先拒**（`docs/116` §3.7 实测同形），所以整数分支永远走不到，写了也是死代码。
pub fn parse_mode(mode: &str) -> Option<Mode> {
    let m = mode.trim();
    if m.eq_ignore_ascii_case("normal") {
        Some(Mode::Normal)
    } else if m.eq_ignore_ascii_case("ode") {
        Some(Mode::Ode)
    } else {
        None
    }
}

/// ODE 模式：`dy/dx = f(x, y)`，一阶 RK4（[`Mode::Ode`]，见 `src/graphwar_path_calculator.rs`）。
///
/// 与 [`calculate_path`] 的**唯一**差别：**不求修正常数**、也不做 `shifted_tokens`。
/// 依据是 [`crate::graphwar_shot_calculator::shot_points`] 的那一行判断
/// `if apply_correction && mode == Mode::Normal { … }` —— 微分方程局里修正常数
/// 根本不参与运算，所以这里传进采样器的就是**原样**的 `f(x, y)` token。
/// （单测 `ode_matches_shot_points_without_correction` 钉住这一点：本函数与
/// `shot_points(expr, x0, y0, STEP_DEFAULT, flip, Mode::Ode, false)` 逐点相等。）
///
/// `offset` 恒为 `NaN`（成功时也是），理由：
/// 1. ODE 模式下**不存在** `c`，返回任何有限数（尤其 `0.0`）都会让界面显示
///    `+ 0.000`，那是在暗示"弹道被平移过" —— 与事实相反；
/// 2. 客户端对这个值的读法是 `float(res.get("offset", 0.0))` + `format_offset()`，
///    `NaN` 会变成**空串**，正好是原版 DiffEq 局右下角不显示修正文本的样子
///    （`docs/92` §4.10；`docs/116` §8.3 也记着"ODE 下修正常数为空"）；
/// 3. 与模块既有约定一致：`offset` 的 `NaN` 表示"本次没有修正常数"，
///    是否成功一律看 `ok` / `error` 两个字段（错误分支的 `offset` 也是 `NaN`，
///    所以**不能**用 `offset` 判别成功与否）。
///
/// 一处有意保留的差异：Normal 模式的 `undefined_at_start` 来自"修正常数不是有限数"；
/// ODE 没有修正常数，所以**不会**报这个码。导数在起点无定义（`sqrt(x)` 在 `x=−1`）
/// 会让 RK4 的 `k1` 变 NaN ⇒ 整段不收敛 ⇒ 退化成 `no_points`（采样器的既有语义，
/// 与 `graphwar_shot_calculator::shot_points(..., Mode::Ode, false)` 完全一样）。
pub fn calculate_path_ode(expr: &str, x0: f64, y0: f64, direction: f64) -> PathOutcome {
    if !x0.is_finite() || !y0.is_finite() {
        return PathOutcome::err(E_START_FINITE);
    }
    // 与 `calculate_path` 同一条判据：采样器把 (x0, y0) 无条件塞成首点，
    // 起点越界时整条曲线在平面外，入口先拒（AGENTS.md 的 |x|<=25 / |y|<=15）。
    if x0.abs() > MAX_ABS_X || y0.abs() > MAX_ABS_Y {
        return PathOutcome::err(E_START_BOUNDS);
    }

    let tokens: Vec<Token> = parse(expr);
    if tokens.is_empty() {
        // 与 `calculate_path` 保持一致（空表达式 = 错误，`docs/116` §6 第 2 条）。
        return PathOutcome::err(E_EMPTY);
    }

    let flip = direction_is_flip(direction);
    // **不**求 correction、**不**做 shifted_tokens —— 见函数文档第 1 段。
    let points = calculate(&tokens, x0, y0, STEP_DEFAULT, flip, Mode::Ode);

    if points.len() < 2 {
        // 导数在起点就爆炸/越界/100 次二分不收敛：采样器只留下首点，它不区分原因，这里也不猜。
        return PathOutcome { ok: false, points, offset: f64::NAN, error: E_NO_POINTS };
    }
    PathOutcome { ok: true, points, offset: f64::NAN, error: "" }
}

/// 带模式的弹道入口：`mode` 决定走 [`calculate_path`]（`"normal"`）还是
/// [`calculate_path_ode`]（`"ode"`）；其余值一律 `bad_arguments`。
///
/// `"normal"` 分支**直接调用** [`calculate_path`]，不是复制一份：
/// 这样"`mode="normal"` 与 `path_calculate` 结果完全相同"是**结构上**成立的，
/// 不依赖两处实现保持同步（单测仍逐位复核一遍）。
pub fn calculate_path_mode(
    expr: &str,
    x0: f64,
    y0: f64,
    direction: f64,
    mode: &str,
) -> PathOutcome {
    match parse_mode(mode) {
        Some(Mode::Normal) => calculate_path(expr, x0, y0, direction),
        Some(Mode::Ode) => calculate_path_ode(expr, x0, y0, direction),
        None => PathOutcome::err(E_BAD_ARGS),
    }
}

// ---------------------------------------------------------------------------
// 2. FFI 层的数据整形（不依赖 Godot，可单测：决定 Dictionary 里放什么）
// ---------------------------------------------------------------------------

/// `path_calculate` / `hit_test` 返回的 Dictionary 里的一个值。
#[derive(Debug, Clone, PartialEq)]
pub enum ApiValue {
    Bool(bool),
    F64(f64),
    /// 整数（`hit_test` 的点序号、士兵下标）。
    Int(i64),
    Str(String),
    Points(Vec<(f64, f64)>),
    /// 单个 `Vector2`（`hit_test` 的 `stop`）。
    Vec2(f64, f64),
    /// `Array[int]`（`hit_test` 的 `hits` / `hit_points_index`）。
    IntArray(Vec<i64>),
    /// `PackedFloat64Array`（`level_obstacles` 的扁平 `[x, y, size] * n`）。
    ///
    /// 与 `hit_test` 的**入参**同一个类型：客户端拿到什么就原样传回去，
    /// 中间不经过 `Vector2`（那是 f32）——否则障碍的位置会在往返里掉精度。
    F64Array(Vec<f64>),
}

/// 契约要求的四个键，顺序固定（`ok` 在前：客户端先读它）。
pub const RESULT_KEYS: [&str; 4] = ["ok", "points", "offset", "error"];

/// 把结果整形为要写进 Dictionary 的键值对。
///
/// `offset` 用**原始 f64**，不做三位小数格式化 —— 界面自己用
/// `GwCore.format_offset()` 决定显示格式（`docs/92` §4.10）。
pub fn result_pairs(out: &PathOutcome) -> Vec<(&'static str, ApiValue)> {
    vec![
        ("ok", ApiValue::Bool(out.ok)),
        ("points", ApiValue::Points(out.points.clone())),
        ("offset", ApiValue::F64(out.offset)),
        ("error", ApiValue::Str(out.error.to_string())),
    ]
}

// ---------------------------------------------------------------------------
// 2b. `hit_test`（`docs/120`）：把 `graphwar_shot_calculator::settle::resolve_points` 的规则抬到 GDScript
// ---------------------------------------------------------------------------
//
// 这是 `docs/116` §8.2 留的坑。要点只有两条，其余全是搬运：
//
// * **规则本体唯一**：本函数先按 `docs/30` 把障碍光栅化进掩膜、把士兵变成
//   `graphwar_shot_calculator::settle::Target`，然后调用 [`crate::graphwar_shot_calculator::settle::resolve_points`] ——
//   与 `graphwar_shot_calculator::settle::resolve`（以及 `tests/live_hits.rs` 里那 12 条真对局回放）
//   走的是**同一个函数**，没有第二份判定；
// * **坐标只换一次**：进参 `points`/`obstacles`/`soldiers` 全是**棋盘坐标**，
//   掩膜是**像素**空间；换算在 `plane::is_occupied` / `plane::rasterize` 内部完成
//   （`px = 20x + 500`、`py = 20y + 300`）。这里**绝不**再做任何换算，
//   否则整条弹道会偏 500/300 像素（`docs/116` §2 的老坑）。

/// 一个被判死的士兵。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoldierHit {
    /// 士兵在入参数组里的下标（0 起）。
    pub soldier: usize,
    /// 弹道点序列里**第一次**进入该士兵半径的点序号。
    pub point: usize,
    /// 那个点的坐标（棋盘坐标）。
    pub x: f64,
    pub y: f64,
}

/// `hit_test` 的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct HitOutcome {
    pub ok: bool,
    /// 被判死的士兵，按**弹道经过的先后顺序**。
    pub hits: Vec<SoldierHit>,
    /// 弹道**第一次**撞到障碍的点序号；`-1` = 没有撞到障碍。
    ///
    /// 注意语义分工（`docs/30`）：障碍让弹道**停下**，士兵只是**被穿过**。
    /// 所以这个字段说的是"弹道停在哪一点"，与 `hits` 是两件事。
    pub first_hit: i64,
    /// 弹道停止处（棋盘坐标）：撞到障碍就是撞击点，否则是最后一个点。
    pub stop: (f64, f64),
    /// 是否因撞到障碍而停止。
    pub blocked: bool,
    /// 空字符串表示成功；否则是机器可读的错误码。
    pub error: &'static str,
}

impl HitOutcome {
    fn err(code: &'static str) -> Self {
        HitOutcome {
            ok: false,
            hits: Vec::new(),
            first_hit: -1,
            stop: (f64::NAN, f64::NAN),
            blocked: false,
            error: code,
        }
    }
}

/// 错误码：点序列为空（调用方还没算弹道就来了）。
pub const E_NO_POINTS_ARR: &str = "no_points";
/// 错误码：点序列里有 NaN / inf。
pub const E_POINTS_NOT_FINITE: &str = "points_not_finite";
/// 错误码：`obstacles` 长度不是 3 的倍数（约定是 `[x, y, size] * n`）。
pub const E_BAD_OBSTACLES: &str = "bad_obstacles";
/// 错误码：`soldiers` 长度不是 3 的倍数（约定是 `[x, y, radius] * n`）。
pub const E_BAD_SOLDIERS: &str = "bad_soldiers";
/// 错误码：取不到 `packed_*_array_operator_index_const`，或参数不是内置数组。
pub const E_UNREADABLE_ARRAY: &str = "unreadable_array";

/// 判一次命中。**这是 `hit_test` 的全部语义**，FFI 层只是搬运。
///
/// 三个入参都是**棋盘坐标**：
///
/// * `points`：弹道点序列（就是 `path_calculate` 返回的那串）；
/// * `obstacles`：三元组 `(x, y, size)`，`size` 是该障碍的半径（游戏单位）；
/// * `soldiers`：三元组 `(x, y, radius)`；**调用方负责把射手本人排掉**
///   （`settle` 的约定，`docs/30` §5 里"射手不会被自己打死"只有样本级证据）。
///
/// 返回值里 `hits[i].soldier` 是**入参下标**，所以调用方不需要给士兵编 id。
pub fn hit_test(
    points: &[(f64, f64)],
    obstacles: &[(f64, f64, f64)],
    soldiers: &[(f64, f64, f64)],
) -> HitOutcome {
    if points.is_empty() {
        return HitOutcome::err(E_NO_POINTS_ARR);
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return HitOutcome::err(E_POINTS_NOT_FINITE);
    }

    // 掩膜只放**障碍**（`docs/30` §2：把士兵画进掩膜会让弹道错误地停在士兵处）。
    let mut mask = crate::plane::empty_mask();
    for &(x, y, size) in obstacles {
        crate::plane::rasterize(&mut mask, x, y, size, 0x00);
    }

    let targets: Vec<crate::graphwar_shot_calculator::settle::Target> = soldiers
        .iter()
        .enumerate()
        .map(|(i, &(x, y, radius))| crate::graphwar_shot_calculator::settle::Target { id: i as i64, x, y, radius })
        .collect();

    // ↓ 判定本体：与 `graphwar_shot_calculator::settle::resolve` 同一函数，没有第二份实现。
    let out = crate::graphwar_shot_calculator::settle::resolve_points(&mask, &targets, points);

    // 下面两次扫描**只决定"报哪几个序号"，不参与判定**：
    // `resolve_points` 只回 `killed: Vec<id>`，没有点序号，而界面要的是"第几点打中的"。
    let mut hits: Vec<SoldierHit> = Vec::with_capacity(out.killed.len());
    for id in &out.killed {
        let t = match targets.iter().find(|t| t.id == *id) {
            Some(t) => t,
            None => continue,
        };
        let idx = points.iter().position(|&(px, py)| {
            let d = ((px - t.x).powi(2) + (py - t.y).powi(2)).sqrt();
            d <= t.radius
        });
        if let Some(point) = idx {
            hits.push(SoldierHit { soldier: *id as usize, point, x: points[point].0, y: points[point].1 });
        }
    }

    let first_hit = if out.blocked {
        points
            .iter()
            .position(|&(px, py)| crate::plane::is_occupied(&mask, px, py))
            .map(|i| i as i64)
            .unwrap_or(-1)
    } else {
        -1
    };

    HitOutcome { ok: true, hits, first_hit, stop: out.stop, blocked: out.blocked, error: "" }
}

/// `hit_test` 返回的 Dictionary 键（顺序固定）。
pub const HIT_KEYS: [&str; 7] = ["ok", "hits", "hit_points", "first_hit", "stop", "blocked", "error"];

/// 把 [`HitOutcome`] 整形为要写进 Dictionary 的键值对。
///
/// `hits` 与 `hit_points` **一一对应**（同下标 = 同一个士兵的第一次被判点），
/// 这样界面既能改存活标记，也能在弹道上画一笔。
pub fn hit_result_pairs(out: &HitOutcome) -> Vec<(&'static str, ApiValue)> {
    vec![
        ("ok", ApiValue::Bool(out.ok)),
        ("hits", ApiValue::IntArray(out.hits.iter().map(|h| h.soldier as i64).collect())),
        ("hit_points", ApiValue::Points(out.hits.iter().map(|h| (h.x, h.y)).collect())),
        ("first_hit", ApiValue::Int(out.first_hit)),
        ("stop", ApiValue::Vec2(out.stop.0, out.stop.1)),
        ("blocked", ApiValue::Bool(out.blocked)),
        ("error", ApiValue::Str(out.error.to_string())),
    ]
}

// ---------------------------------------------------------------------------
// 2c. `level_obstacles`（`docs/126`）：把"关卡 → 障碍"抬到 GDScript
// ---------------------------------------------------------------------------
//
// 契约（`docs/126` §1）：
//
// ```text
// GW2Rebuild.level_obstacles(level_index: int, seed: int)
//     -> Dictionary { ok: bool, obstacles: PackedFloat64Array, error: String }
// ```
//
// `obstacles` = `[x, y, size] * n`，**棋盘坐标**、`size` = 半径（游戏单位）——
// 与 `hit_test` 的第 2 个入参**同一种数组**，客户端拿到就原样传回去，中间不重排。
//
// 生成本体在 [`crate::graphwar_level_generator::obstacles_from_seed`]（既有、已逐位验证的实现），
// 这里只做三件事：参数校验、类型转换、错误翻译。**没有第二份公式。**

/// 一关的障碍生成结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ObstacleOutcome {
    pub ok: bool,
    /// `[x, y, size] * n`（棋盘坐标，`size` = 半径）。
    pub obstacles: Vec<(f64, f64, f64)>,
    /// 空字符串表示成功；否则是机器可读的错误码。
    pub error: &'static str,
}

impl ObstacleOutcome {
    fn err(code: &'static str) -> Self {
        ObstacleOutcome { ok: false, obstacles: Vec::new(), error: code }
    }
}

/// 错误码：关卡号超出 `0..=20`（`docs/118` §5.2：一共 21 关）。
pub const E_BAD_LEVEL_INDEX: &str = "bad_level_index";

/// 关卡总数（与 [`crate::graphwar_level_generator::LEVEL_COUNT`] 同一个数，这里只是给错误信息用）。
pub const LEVEL_COUNT: usize = crate::graphwar_level_generator::LEVEL_COUNT;

/// 生成一关的障碍。**这是 `level_obstacles` 的全部语义**，FFI 层只是搬运。
///
/// * `level_index`：**0 基**，`0..=20`（`level_0`..`level_20`，与客户端
///   `GwCampaign.level(i)`、`state.level_index` 同一套编号）；越界 → `bad_level_index`。
/// * `seed`：**调用方给的种子**。同 `(level_index, seed)` ⇒ 逐位相同；换 seed ⇒ 换一局布局。
///   语义与"为什么不是原版那条流"写在 [`crate::graphwar_level_generator::obstacles_from_seed`] 的文档里
///   （要点：原版每局开局从 `/dev/urandom` 抽种子 ⇒ 逐局不同，`docs/118` §7.1 的 `R` 列）。
///
/// `seed` 是 `i64`：负值也接受，按 `u64` 位模式解释（GDScript 的 `int` 就是 64 位有符号）。
pub fn level_obstacles(level_index: i64, seed: i64) -> ObstacleOutcome {
    if level_index < 0 || level_index as usize >= LEVEL_COUNT {
        return ObstacleOutcome::err(E_BAD_LEVEL_INDEX);
    }
    let Some(obs) = crate::graphwar_level_generator::obstacles_from_seed(level_index as usize, seed as u64) else {
        // `obstacles_from_seed` 只在关卡号越界时返回 None —— 上面已挡住，这里再兜一次。
        return ObstacleOutcome::err(E_BAD_LEVEL_INDEX);
    };
    if obs.iter().any(|o| !o.x.is_finite() || !o.y.is_finite() || !o.r.is_finite()) {
        // 生成器不该产出非有限值（单测钉住了）；真出现就报错，别把 NaN 交给渲染。
        return ObstacleOutcome::err(E_NOT_FINITE);
    }
    ObstacleOutcome {
        ok: true,
        obstacles: obs.iter().map(|o| (o.x, o.y, o.r)).collect(),
        error: "",
    }
}

/// 错误码：生成器给出了非有限值（NaN/±inf）。
pub const E_NOT_FINITE: &str = "obstacles_not_finite";

/// `level_obstacles` 返回的 Dictionary 键（顺序固定，`ok` 在最前）。
pub const OBSTACLE_KEYS: [&str; 3] = ["ok", "obstacles", "error"];

/// 把 [`ObstacleOutcome`] 整形为要写进 Dictionary 的键值对。
pub fn obstacle_pairs(out: &ObstacleOutcome) -> Vec<(&'static str, ApiValue)> {
    let flat: Vec<f64> = out
        .obstacles
        .iter()
        .flat_map(|&(x, y, r)| [x, y, r])
        .collect();
    vec![
        ("ok", ApiValue::Bool(out.ok)),
        ("obstacles", ApiValue::F64Array(flat)),
        ("error", ApiValue::Str(out.error.to_string())),
    ]
}

// ---------------------------------------------------------------------------
// 3. Godot 接口表
// ---------------------------------------------------------------------------

type GetProc = unsafe extern "C" fn(name: *const u8) -> *mut c_void;

// `GDEXTENSION_VARIANT_TYPE_*`（Godot 4.x 的 Variant::Type 判别值，已实测：
// 参 `docs/116` §3.1 —— 探测方法里 STRING=4 / DICTIONARY=27 / PACKED_VECTOR2_ARRAY=35）。
const V_NIL: i32 = 0;
const V_BOOL: i32 = 1;
const V_INT: i32 = 2;
const V_FLOAT: i32 = 3;
const V_STRING: i32 = 4;
const V_VECTOR2: i32 = 5;
const V_DICTIONARY: i32 = 27;
const V_ARRAY: i32 = 28;
const V_PACKED_FLOAT64_ARRAY: i32 = 33;
const V_PACKED_VECTOR2_ARRAY: i32 = 35;

/// `PROPERTY_USAGE_DEFAULT`。
const USAGE_DEFAULT: u32 = 6;
/// `METHOD_FLAG_NORMAL`。
const METHOD_FLAG_NORMAL: u32 = 1;

/// 一块 16 字节对齐的裸内存，用来放 Godot 的内置类型（`Variant` 24 字节、
/// `String`/`StringName`/`Dictionary`/`PackedVector2Array` 各 8 字节）。
///
/// 之所以自己给内存而不是 `mem_alloc`：GDExtension 没有暴露 `sizeof(Variant)`，
/// 而这里放得下所有内置类型还有富余，不会有越界写。
#[repr(align(16))]
struct Slot([u8; 128]);

impl Slot {
    fn new() -> Box<Slot> {
        Box::new(Slot([0u8; 128]))
    }
    fn p(&self) -> *mut c_void {
        self.0.as_ptr() as *mut c_void
    }
}

type FnSnNew = unsafe extern "C" fn(dest: *mut c_void, text: *const u8, is_static: u8);
type FnStrNew = unsafe extern "C" fn(dest: *mut c_void, text: *const u8);
type FnStrToUtf8 = unsafe extern "C" fn(src: *const c_void, buf: *mut u8, max: i64) -> i64;
type FnGetCtor = unsafe extern "C" fn(ty: i32) -> *mut c_void;
type FnCtor = unsafe extern "C" fn(dest: *mut c_void, value: *mut c_void);
type FnGetPtrCtor = unsafe extern "C" fn(ty: i32, idx: i32) -> *mut c_void;
type FnPtrCtor = unsafe extern "C" fn(base: *mut c_void, args: *const *const c_void);
type FnVariantCall = unsafe extern "C" fn(
    this_: *mut c_void,
    method: *const c_void,
    args: *const *const c_void,
    argc: i64,
    ret: *mut c_void,
    err: *mut c_void,
);
type FnGetType = unsafe extern "C" fn(v: *const c_void) -> i32;
type FnDestroy = unsafe extern "C" fn(v: *mut c_void);
/// `packed_*_array_operator_index_const` 的函数类型（照 `gdextension_interface.h`）：
///
/// ```c
/// typedef const double *(*GDExtensionInterfacePackedFloat64ArrayOperatorIndexConst)(
///         GDExtensionConstTypePtr p_self, GDExtensionInt p_index);
/// typedef GDExtensionTypePtr (*GDExtensionInterfacePackedVector2ArrayOperatorIndexConst)(
///         GDExtensionConstTypePtr p_self, GDExtensionInt p_index);
/// ```
///
/// `p_self` 是**裸的** `Packed*Array`（用 `get_variant_to_type_constructor` 从 Variant
/// 里取出来），不是 Variant —— 见 `docs/120` §3.1 里那条踩坑记录。
type FnPackedOpIndex = unsafe extern "C" fn(base: *const c_void, index: i64) -> *mut c_void;

/// 已解析的接口函数指针。存成 `usize` 是为了让静态表 `Send + Sync`。
struct Api {
    from_ctor: usize,   // get_variant_from_type_constructor
    to_ctor: usize,     // get_variant_to_type_constructor
    ptr_ctor: usize,    // variant_get_ptr_constructor
    variant_call: usize,
    get_type: usize,    // variant_get_type
    variant_destroy: usize, // variant_destroy(Variant)
    raw_dtor: usize,        // variant_get_ptr_destructor(type) -> fn(TypePtr)
    sn_new: usize,      // string_name_new_with_latin1_chars
    str_new: usize,     // string_new_with_utf8_chars
    str_to_utf8: usize, // string_to_utf8_chars
    register_method: usize,
    /// `packed_vector2_array_operator_index_const`（读 `PackedVector2Array` 元素，
    /// `docs/120` §3.1）。
    vec2_opindex: usize,
    /// `packed_float64_array_operator_index_const`（读 `PackedFloat64Array` 元素）。
    f64_opindex: usize,
}

static API: OnceLock<Api> = OnceLock::new();

/// 注册过一次 method 之后的计数（自检用）。
static CALLS: AtomicUsize = AtomicUsize::new(0);
static OK_CALLS: AtomicUsize = AtomicUsize::new(0);
static ERR_CALLS: AtomicUsize = AtomicUsize::new(0);
/// `hit_test` 的调用计数（`docs/120`）。
static HIT_CALLS: AtomicUsize = AtomicUsize::new(0);
static HIT_OK_CALLS: AtomicUsize = AtomicUsize::new(0);
/// `level_obstacles` 的调用计数（`docs/126`）。
static OBST_CALLS: AtomicUsize = AtomicUsize::new(0);
static OBST_OK_CALLS: AtomicUsize = AtomicUsize::new(0);
/// `level_obstacles` 最近一次成功调用生成的障碍个数（调试/自检用）。
static OBST_LAST_COUNT: AtomicUsize = AtomicUsize::new(0);
/// `path_calculate_mode`（`docs/138`）的调用计数。
static MODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static MODE_OK_CALLS: AtomicUsize = AtomicUsize::new(0);

/// `path_calculate` 被 GDScript 调用的次数。
pub fn calls() -> usize {
    CALLS.load(Ordering::Relaxed)
}
/// 其中返回 `ok: true` / `ok: false` 的次数。
pub fn ok_calls() -> usize {
    OK_CALLS.load(Ordering::Relaxed)
}
pub fn err_calls() -> usize {
    ERR_CALLS.load(Ordering::Relaxed)
}
/// `hit_test` 被调用 / 返回 `ok: true` 的次数。
pub fn hit_calls() -> (usize, usize) {
    (HIT_CALLS.load(Ordering::Relaxed), HIT_OK_CALLS.load(Ordering::Relaxed))
}
/// `level_obstacles` 被调用 / 返回 `ok: true` 的次数。
pub fn obstacle_calls() -> (usize, usize) {
    (OBST_CALLS.load(Ordering::Relaxed), OBST_OK_CALLS.load(Ordering::Relaxed))
}
/// `level_obstacles` 最近一次成功调用生成多少个障碍（`usize::MAX` = 还没成功过）。
pub fn obstacle_last_count() -> usize {
    OBST_LAST_COUNT.load(Ordering::Relaxed)
}
/// `path_calculate_mode` 被调用 / 返回 `ok: true` 的次数（`docs/138`）。
pub fn mode_calls() -> (usize, usize) {
    (MODE_CALLS.load(Ordering::Relaxed), MODE_OK_CALLS.load(Ordering::Relaxed))
}

fn api() -> Option<&'static Api> {
    API.get()
}

unsafe fn sym(get: GetProc, name: &str) -> usize {
    let mut c = Vec::with_capacity(name.len() + 1);
    c.extend_from_slice(name.as_bytes());
    c.push(0);
    get(c.as_ptr()) as usize
}

impl Api {
    unsafe fn from_ctor(&self, ty: i32) -> Option<FnCtor> {
        if self.from_ctor == 0 {
            return None;
        }
        let f: FnGetCtor = std::mem::transmute(self.from_ctor);
        let p = f(ty);
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute(p))
        }
    }
    unsafe fn to_ctor(&self, ty: i32) -> Option<FnCtor> {
        if self.to_ctor == 0 {
            return None;
        }
        let f: FnGetCtor = std::mem::transmute(self.to_ctor);
        let p = f(ty);
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute(p))
        }
    }
    unsafe fn sn(&self, text: &str) -> Option<Box<Slot>> {
        if self.sn_new == 0 {
            return None;
        }
        let f: FnSnNew = std::mem::transmute(self.sn_new);
        let s = Slot::new();
        let mut c = text.as_bytes().to_vec();
        c.push(0);
        f(s.p(), c.as_ptr(), 0);
        Some(s)
    }
    unsafe fn string(&self, text: &str) -> Option<Box<Slot>> {
        if self.str_new == 0 {
            return None;
        }
        let f: FnStrNew = std::mem::transmute(self.str_new);
        let s = Slot::new();
        let mut c = text.as_bytes().to_vec();
        c.push(0);
        f(s.p(), c.as_ptr());
        Some(s)
    }
    /// 读一个 Variant 的判别值；失败返回 -1。
    unsafe fn ty(&self, v: *const c_void) -> i32 {
        if self.get_type == 0 || v.is_null() {
            return -1;
        }
        let f: FnGetType = std::mem::transmute(self.get_type);
        f(v)
    }
    /// 读 `f64` 参数（接受 FLOAT 与 INT）。
    unsafe fn get_f64(&self, v: *const c_void) -> Option<f64> {
        match self.ty(v) {
            V_FLOAT => {
                let f = self.to_ctor(V_FLOAT)?;
                let mut out = 0.0f64;
                f(&mut out as *mut f64 as *mut c_void, v as *mut c_void);
                Some(out)
            }
            V_INT => {
                let f = self.to_ctor(V_INT)?;
                let mut out = 0i64;
                f(&mut out as *mut i64 as *mut c_void, v as *mut c_void);
                Some(out as f64)
            }
            _ => None,
        }
    }
    /// 读 `i64` 参数（`level_obstacles` 的 `level_index` / `seed`）。
    ///
    /// 接受 INT；也接受**整值** FLOAT（GDScript 里 `4.0` 这种写法不该让整次调用失败），
    /// 但**不接受**带小数的 FLOAT —— 静默截断会把"关卡 4.5"悄悄变成关卡 4。
    unsafe fn get_i64(&self, v: *const c_void) -> Option<i64> {
        match self.ty(v) {
            V_INT => {
                let f = self.to_ctor(V_INT)?;
                let mut out = 0i64;
                f(&mut out as *mut i64 as *mut c_void, v as *mut c_void);
                Some(out)
            }
            V_FLOAT => {
                let x = self.get_f64(v)?;
                if x.is_finite() && x.fract() == 0.0 && x >= (i64::MIN as f64) && x <= (i64::MAX as f64)
                {
                    Some(x as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// 读 `String` 参数（接受 STRING 与 NIL）。
    unsafe fn get_string(&self, v: *const c_void) -> Option<String> {
        match self.ty(v) {
            V_STRING => {}
            V_NIL => return Some(String::new()),
            _ => return None,
        }
        let f = self.to_ctor(V_STRING)?;
        let raw = Slot::new();
        f(raw.p(), v as *mut c_void);
        let g: FnStrToUtf8 = std::mem::transmute(self.str_to_utf8);
        let need = g(raw.p(), std::ptr::null_mut(), 0);
        if need < 0 {
            return None;
        }
        let mut buf = vec![0u8; need as usize + 1];
        let got = g(raw.p(), buf.as_mut_ptr(), need);
        buf.truncate(got.max(0) as usize);
        // 释放这次 copy 出来的 String（否则每次调用漏一个 String 引用）
        self.destroy_raw(V_STRING, raw.p());
        String::from_utf8(buf).ok()
    }
    /// 销毁一个 **Variant** 槽（`variant_destroy`）。
    unsafe fn destroy_variant(&self, p: *mut c_void) {
        if self.variant_destroy != 0 && !p.is_null() {
            let f: FnDestroy = std::mem::transmute(self.variant_destroy);
            f(p);
        }
    }
    /// 销毁一个**裸内置对象**（`variant_get_ptr_destructor(type)`）。
    unsafe fn destroy_raw(&self, ty: i32, p: *mut c_void) {
        if self.raw_dtor == 0 || p.is_null() {
            return;
        }
        let g: FnGetCtor = std::mem::transmute(self.raw_dtor);
        let d = g(ty);
        if d.is_null() {
            return;
        }
        let f: FnDestroy = std::mem::transmute(d);
        f(p);
    }
    /// 在 `this_`（一个 Variant 槽）上调用内置方法 `method`。
    unsafe fn call(&self, this_: *mut c_void, method: *const c_void, args: &[*const c_void]) -> i32 {
        if self.variant_call == 0 {
            return -1;
        }
        let f: FnVariantCall = std::mem::transmute(self.variant_call);
        let mut err = [0u8; 16];
        let ret = Slot::new();
        f(this_, method, args.as_ptr(), args.len() as i64, ret.p(), err.as_mut_ptr() as *mut c_void);
        i32::from_le_bytes([err[0], err[1], err[2], err[3]])
    }

    // ---- 读内置数组（`docs/120` §3.1：手写 FFI 怎么把 Packed* 拆开）----
    //
    // 做法固定两步，两种数组共用：
    //   ① `get_variant_to_type_constructor(type)` 把 Variant 里的**裸数组**取出来
    //      （`Packed*Array` 就是 8 字节的 CowData 指针；取出来会加一次引用，
    //      用完必须 `variant_get_ptr_destructor(type)` 放掉）；
    //   ② 用 `packed_*_array_operator_index_const` 逐个取元素地址，直接读裸值。
    //
    // **不要**改用 `variant_get_ptr_indexed_getter`：它在 4.7.2 上实测读出来是垃圾
    // （`docs/120` §3.1 有对照数据：同一个数组 A 路全是 1e-35 量级，B 路才对）。

    /// 读 `PackedVector2Array` 的全部元素。
    unsafe fn read_packed_v2(&self, v: *const c_void) -> Option<Vec<(f64, f64)>> {
        let n = self.array_len(v)?;
        if self.vec2_opindex == 0 {
            return None;
        }
        let to_raw = self.to_ctor(V_PACKED_VECTOR2_ARRAY)?;
        let raw = Slot::new();
        to_raw(raw.p(), v as *mut c_void);
        let index_of: FnPackedOpIndex = std::mem::transmute(self.vec2_opindex);
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let p = index_of(raw.p(), i);
            if p.is_null() {
                self.destroy_raw(V_PACKED_VECTOR2_ARRAY, raw.p());
                return None;
            }
            // Vector2 是 2 × f32（标准精度的 Godot 构建）。
            let xy = std::slice::from_raw_parts(p as *const f32, 2);
            out.push((xy[0] as f64, xy[1] as f64));
        }
        self.destroy_raw(V_PACKED_VECTOR2_ARRAY, raw.p());
        Some(out)
    }

    /// 读 `PackedFloat64Array` 的全部元素。
    unsafe fn read_packed_f64(&self, v: *const c_void) -> Option<Vec<f64>> {
        let n = self.array_len(v)?;
        if self.f64_opindex == 0 {
            return None;
        }
        let to_raw = self.to_ctor(V_PACKED_FLOAT64_ARRAY)?;
        let raw = Slot::new();
        to_raw(raw.p(), v as *mut c_void);
        let index_of: FnPackedOpIndex = std::mem::transmute(self.f64_opindex);
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let p = index_of(raw.p(), i);
            if p.is_null() {
                self.destroy_raw(V_PACKED_FLOAT64_ARRAY, raw.p());
                return None;
            }
            out.push(*(p as *const f64));
        }
        self.destroy_raw(V_PACKED_FLOAT64_ARRAY, raw.p());
        Some(out)
    }

    /// 数组长度：`size()`（`Array` 与所有 `Packed*Array` 都有这个方法）。
    unsafe fn array_len(&self, v: *const c_void) -> Option<i64> {
        if self.variant_call == 0 {
            return None;
        }
        let names = NAMES.get()?;
        let f: FnVariantCall = std::mem::transmute(self.variant_call);
        let mut err = [0u8; 16];
        let ret = Slot::new();
        f(v as *mut c_void, names.size.p(), std::ptr::null(), 0, ret.p(),
          err.as_mut_ptr() as *mut c_void);
        let ok = i32::from_le_bytes([err[0], err[1], err[2], err[3]]) == 0;
        if !ok {
            return None;
        }
        let to_i64 = self.to_ctor(V_INT)?;
        let mut n = 0i64;
        to_i64(&mut n as *mut i64 as *mut c_void, ret.p());
        self.destroy_variant(ret.p());
        Some(n)
    }
}

/// 注册时创建一次、之后一直复用的 `StringName`（方法名）。
struct Names {
    set: Box<Slot>,
    append: Box<Slot>,
    size: Box<Slot>,
}

static NAMES: OnceLock<Names> = OnceLock::new();

// ---------------------------------------------------------------------------
// 4. GDExtension 结构体（字段序照 `gdextension_interface.h`）
// ---------------------------------------------------------------------------

/// `GDExtensionPropertyInfo`。**`class_name` / `hint_string` 必须是合法指针**
/// （传 NULL 引擎会段错误，实测）。
#[repr(C)]
struct PropertyInfo {
    ty: i32,
    name: *const c_void,
    class_name: *const c_void,
    hint: u32,
    hint_string: *const c_void,
    usage: u32,
}

/// `GDExtensionClassMethodCall`。
type MethodCall = unsafe extern "C" fn(
    method_userdata: *mut c_void,
    instance: *mut c_void,
    args: *const *const c_void,
    arg_count: i64,
    ret: *mut c_void,
    error: *mut c_void,
);

/// `GDExtensionClassMethodInfo`。
#[repr(C)]
struct ClassMethodInfo {
    name: *const c_void,
    method_userdata: *mut c_void,
    call_func: Option<MethodCall>,
    ptrcall_func: *const c_void,
    method_flags: u32,
    has_return_value: u8,
    return_value_info: *const PropertyInfo,
    return_value_metadata: i32,
    argument_count: u32,
    arguments_info: *const PropertyInfo,
    arguments_metadata: *const c_void,
    default_argument_count: u32,
    default_arguments: *const c_void,
}

// ---------------------------------------------------------------------------
// 5. call_func：真正被 GDScript 调到的那个函数
// ---------------------------------------------------------------------------

/// 造一个裸 `String`，包成 Variant 写进 `dest`，再立刻释放裸对象
/// （引用计数只剩 Variant 那一份，最后由 [`Api::destroy_variant`] 释放）。
///
/// # Safety
/// `dest` 必须是 Godot 给的合法 Variant 槽。
unsafe fn put_string_variant(api: &Api, dest: *mut c_void, text: &str) -> bool {
    let (Some(raw), Some(from_str)) = (api.string(text), api.from_ctor(V_STRING)) else {
        return false;
    };
    from_str(dest, raw.p());
    api.destroy_raw(V_STRING, raw.p());
    true
}

/// 造一个 `PackedVector2Array` 的 Variant 写进 `dest`。
///
/// 做法：先默认构造一个空数组，`from_ctor` 包成 Variant（CowData 共享一次），
/// 立刻释放裸对象，然后**只**通过 Variant 上的 `append` 追加点 —— 这样不会有
/// COW 分家导致的“写了一片没人看的数组”，也不需要 `get_variant_to_type_constructor`
/// 把数据再拷回来。
///
/// 实测代价：20000 点约 13 ms（debug 构建，时间全在引擎侧）。
unsafe fn put_points_variant(api: &Api, dest: *mut c_void, pts: &[(f64, f64)]) -> bool {
    let Some(get_ptr_ctor) = (if api.ptr_ctor != 0 {
        Some(std::mem::transmute::<usize, FnGetPtrCtor>(api.ptr_ctor))
    } else {
        None
    }) else {
        return false;
    };
    let ctor0 = get_ptr_ctor(V_PACKED_VECTOR2_ARRAY, 0);
    if ctor0.is_null() {
        return false;
    }
    let ctor0: FnPtrCtor = std::mem::transmute(ctor0);
    let raw = Slot::new();
    ctor0(raw.p(), std::ptr::null());
    let (Some(from_packed), Some(from_v2)) =
        (api.from_ctor(V_PACKED_VECTOR2_ARRAY), api.from_ctor(V_VECTOR2))
    else {
        api.destroy_raw(V_PACKED_VECTOR2_ARRAY, raw.p());
        return false;
    };
    from_packed(dest, raw.p());
    api.destroy_raw(V_PACKED_VECTOR2_ARRAY, raw.p());

    let Some(names) = NAMES.get() else { return false };
    let v2 = Slot::new();
    for &(x, y) in pts {
        // 棋盘坐标是 f64，Godot 的 Vector2 是 f32 —— 精度损失发生在引擎侧，
        // 是渲染精度的上限，不是采样器的损失（采样结果仍是 f64，见 docs/116 §5）。
        let mut component = [x as f32, y as f32];
        from_v2(v2.p(), component.as_mut_ptr() as *mut c_void);
        let _ = api.call(dest, names.append.p(), &[v2.p()]);
    }
    true
}

/// 把 `pairs` 写成 Dictionary，放进 `ret`（Godot 给的**空 Variant 槽**）。
///
/// `path_calculate` 与 `hit_test` 共用这一段：造字典的 ABI 只在这里出现一次。
unsafe fn write_dict(api: &Api, ret: *mut c_void, pairs: Vec<(&'static str, ApiValue)>) {
    if ret.is_null() {
        return;
    }
    let Some(get_ptr_ctor) = (if api.ptr_ctor != 0 {
        Some(std::mem::transmute::<usize, FnGetPtrCtor>(api.ptr_ctor))
    } else {
        None
    }) else {
        return;
    };
    let Some(from_dict) = api.from_ctor(V_DICTIONARY) else { return };
    let ctor0 = get_ptr_ctor(V_DICTIONARY, 0);
    if ctor0.is_null() {
        return;
    }
    let ctor0: FnPtrCtor = std::mem::transmute(ctor0);
    let raw = Slot::new();
    ctor0(raw.p(), std::ptr::null());
    from_dict(ret, raw.p());
    api.destroy_raw(V_DICTIONARY, raw.p());

    let Some(names) = NAMES.get() else { return };
    for (key, value) in pairs {
        let key_slot = Slot::new();
        if !put_string_variant(api, key_slot.p(), key) {
            continue;
        }
        let val_slot = Slot::new();
        let built = match &value {
            ApiValue::Bool(b) => match api.from_ctor(V_BOOL) {
                Some(f) => {
                    let mut raw_b = *b;
                    f(val_slot.p(), &mut raw_b as *mut bool as *mut c_void);
                    true
                }
                None => false,
            },
            ApiValue::F64(d) => match api.from_ctor(V_FLOAT) {
                Some(f) => {
                    let mut raw_d = *d;
                    f(val_slot.p(), &mut raw_d as *mut f64 as *mut c_void);
                    true
                }
                None => false,
            },
            ApiValue::Int(i) => match api.from_ctor(V_INT) {
                Some(f) => {
                    let mut raw_i = *i;
                    f(val_slot.p(), &mut raw_i as *mut i64 as *mut c_void);
                    true
                }
                None => false,
            },
            ApiValue::Str(text) => put_string_variant(api, val_slot.p(), text),
            ApiValue::Points(pts) => put_points_variant(api, val_slot.p(), pts),
            ApiValue::Vec2(x, y) => match api.from_ctor(V_VECTOR2) {
                Some(f) => {
                    let mut component = [*x as f32, *y as f32];
                    f(val_slot.p(), component.as_mut_ptr() as *mut c_void);
                    true
                }
                None => false,
            },
            ApiValue::IntArray(items) => put_int_array_variant(api, val_slot.p(), items),
            ApiValue::F64Array(items) => put_f64_array_variant(api, val_slot.p(), items),
        };
        if built {
            let _ = api.call(ret, names.set.p(), &[key_slot.p(), val_slot.p()]);
        }
        // 两个 Variant 都是 placement-new 出来的，必须按 Variant 析构
        api.destroy_variant(val_slot.p());
        api.destroy_variant(key_slot.p());
    }
}

/// 造一个 `Array` 的 Variant 写进 `dest`，元素是整数（`append` 逐个追加）。
unsafe fn put_int_array_variant(api: &Api, dest: *mut c_void, items: &[i64]) -> bool {
    let Some(get_ptr_ctor) = (if api.ptr_ctor != 0 {
        Some(std::mem::transmute::<usize, FnGetPtrCtor>(api.ptr_ctor))
    } else {
        None
    }) else {
        return false;
    };
    let ctor0 = get_ptr_ctor(V_ARRAY, 0);
    if ctor0.is_null() {
        return false;
    }
    let ctor0: FnPtrCtor = std::mem::transmute(ctor0);
    let raw = Slot::new();
    ctor0(raw.p(), std::ptr::null());
    let (Some(from_array), Some(from_int)) = (api.from_ctor(V_ARRAY), api.from_ctor(V_INT)) else {
        api.destroy_raw(V_ARRAY, raw.p());
        return false;
    };
    from_array(dest, raw.p());
    api.destroy_raw(V_ARRAY, raw.p());

    let Some(names) = NAMES.get() else { return false };
    let item = Slot::new();
    for &v in items {
        let mut raw_i = v;
        from_int(item.p(), &mut raw_i as *mut i64 as *mut c_void);
        let _ = api.call(dest, names.append.p(), &[item.p()]);
        api.destroy_variant(item.p());
    }
    true
}

/// 把 `path_calculate` 的结果写成 Dictionary。
unsafe fn write_result(api: &Api, ret: *mut c_void, out: &PathOutcome) {
    write_dict(api, ret, result_pairs(out));
}

/// 造一个 `PackedFloat64Array` 的 Variant 写进 `dest`（`level_obstacles` 的 `obstacles`）。
///
/// 与 [`put_int_array_variant`] 同一条路：默认构造一个**空** `PackedFloat64Array` →
/// `from_ctor` 包成 Variant（CowData 共享一次）→ 立刻释放裸对象 → 之后**只**通过
/// Variant 上的 `append` 追加元素。这样不会有 COW 分家写丢的问题。
unsafe fn put_f64_array_variant(api: &Api, dest: *mut c_void, items: &[f64]) -> bool {
    let Some(get_ptr_ctor) = (if api.ptr_ctor != 0 {
        Some(std::mem::transmute::<usize, FnGetPtrCtor>(api.ptr_ctor))
    } else {
        None
    }) else {
        return false;
    };
    let ctor0 = get_ptr_ctor(V_PACKED_FLOAT64_ARRAY, 0);
    if ctor0.is_null() {
        return false;
    }
    let ctor0: FnPtrCtor = std::mem::transmute(ctor0);
    let raw = Slot::new();
    ctor0(raw.p(), std::ptr::null());
    let (Some(from_packed), Some(from_float)) =
        (api.from_ctor(V_PACKED_FLOAT64_ARRAY), api.from_ctor(V_FLOAT))
    else {
        api.destroy_raw(V_PACKED_FLOAT64_ARRAY, raw.p());
        return false;
    };
    from_packed(dest, raw.p());
    api.destroy_raw(V_PACKED_FLOAT64_ARRAY, raw.p());

    let Some(names) = NAMES.get() else { return false };
    let item = Slot::new();
    for &v in items {
        // f64 全程不降精度：障碍的坐标/半径与 `hit_test` 的入参是同一批 f64
        let mut raw_d = v;
        from_float(item.p(), &mut raw_d as *mut f64 as *mut c_void);
        let _ = api.call(dest, names.append.p(), &[item.p()]);
        api.destroy_variant(item.p());
    }
    true
}

/// GDScript 调 `path_calculate` 时走这里。
unsafe extern "C" fn path_calculate_call(
    _ud: *mut c_void,
    _instance: *mut c_void,
    args: *const *const c_void,
    argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    // 跨 FFI 的 panic 会 abort 整个 Godot 进程，这里兜底翻译成 ok:false。
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(api) = api() else {
            return PathOutcome::err(E_PANIC);
        };
        if argc < 4 || args.is_null() || ret.is_null() {
            return PathOutcome::err(E_BAD_ARGS);
        }
        let arg = |i: usize| *args.add(i);
        let expr = match api.get_string(arg(0)) {
            Some(s) => s,
            None => return PathOutcome::err(E_BAD_ARGS),
        };
        let (x0, y0, dir) = match (
            api.get_f64(arg(1)),
            api.get_f64(arg(2)),
            api.get_f64(arg(3)),
        ) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => return PathOutcome::err(E_BAD_ARGS),
        };
        let outcome = calculate_path(&expr, x0, y0, dir);
        write_result(api, ret as *mut c_void, &outcome);
        outcome
    }))
    .unwrap_or_else(|_| {
        // panic 之后 ret 里可能已经有半成品；不再动它。
        PathOutcome::err(E_PANIC)
    });
    if out.ok {
        OK_CALLS.fetch_add(1, Ordering::Relaxed);
    } else {
        ERR_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// 6. 注册
// ---------------------------------------------------------------------------

/// GDScript 调 `path_calculate_mode` 时走这里（`docs/138`）。
///
/// 参数约定（与 `path_calculate` 相比只多第 5 个）：
///
/// ```text
/// path_calculate_mode(expr: String, x0: float, y0: float, direction: float, mode: String)
///     -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
/// ```
///
/// 返回的 Dictionary **与 `path_calculate` 完全同形**（同样是 [`RESULT_KEYS`] 四个键），
/// 所以客户端的读法不用改：`mode` 只在 GDScript 包装层回填，内核不往契约里塞第 5 个键。
///
/// 三条与 `path_calculate` 相同的硬要求：`catch_unwind` 兜底、出错走 `ok:false + error`、
/// `ret` 是引擎给的**空 Variant 槽**（用 [`write_dict`] 填）。
unsafe extern "C" fn path_calculate_mode_call(
    _ud: *mut c_void,
    _instance: *mut c_void,
    args: *const *const c_void,
    argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    MODE_CALLS.fetch_add(1, Ordering::Relaxed);
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(api) = api() else {
            return PathOutcome::err(E_PANIC);
        };
        if argc < 5 || args.is_null() || ret.is_null() {
            return PathOutcome::err(E_BAD_ARGS);
        }
        let arg = |i: usize| *args.add(i);
        let expr = match api.get_string(arg(0)) {
            Some(s) => s,
            None => return PathOutcome::err(E_BAD_ARGS),
        };
        let (x0, y0, dir) = match (
            api.get_f64(arg(1)),
            api.get_f64(arg(2)),
            api.get_f64(arg(3)),
        ) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => return PathOutcome::err(E_BAD_ARGS),
        };
        // 非法 mode 在 `calculate_path_mode` 里翻成 E_BAD_ARGS（不 panic、不猜一个默认值）。
        let mode = match api.get_string(arg(4)) {
            Some(s) => s,
            None => return PathOutcome::err(E_BAD_ARGS),
        };
        let outcome = calculate_path_mode(&expr, x0, y0, dir, &mode);
        write_result(api, ret as *mut c_void, &outcome);
        outcome
    }))
    .unwrap_or_else(|_| {
        // panic 之后 ret 里可能已经有半成品；不再动它。
        PathOutcome::err(E_PANIC)
    });
    if out.ok {
        MODE_OK_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

/// GDScript 调 `hit_test` 时走这里。
///
/// 参数约定（`docs/120` §1）：
///
/// ```text
/// hit_test(points: PackedVector2Array,
///          obstacles: PackedFloat64Array,   # [x, y, size] * n，棋盘坐标
///          soldiers:  PackedFloat64Array)   # [x, y, radius] * n，棋盘坐标
///     -> Dictionary { ok, hits, hit_points, first_hit, stop, blocked, error }
/// ```
///
/// 之所以用扁平 `PackedFloat64Array` 而不是 `Array[Dictionary]`：读数组元素走的是
/// 与 `points` **完全相同**的那条 `operator_index_const` 路径，
/// 不需要在 FFI 层调 `Dictionary.get` —— 少一层没用过的 ABI，就少一个静默失败点。
unsafe extern "C" fn hit_test_call(
    _ud: *mut c_void,
    _instance: *mut c_void,
    args: *const *const c_void,
    argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    HIT_CALLS.fetch_add(1, Ordering::Relaxed);
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(api) = api() else {
            return HitOutcome::err(E_PANIC);
        };
        if argc < 3 || args.is_null() || ret.is_null() {
            return HitOutcome::err(E_BAD_ARGS);
        }
        let arg = |i: usize| *args.add(i);
        let Some(points) = api.read_packed_v2(arg(0)) else {
            return HitOutcome::err(E_UNREADABLE_ARRAY);
        };
        let Some(flat_obstacles) = api.read_packed_f64(arg(1)) else {
            return HitOutcome::err(E_UNREADABLE_ARRAY);
        };
        let Some(flat_soldiers) = api.read_packed_f64(arg(2)) else {
            return HitOutcome::err(E_UNREADABLE_ARRAY);
        };
        let (Some(obstacles), Some(soldiers)) =
            (triples(&flat_obstacles), triples(&flat_soldiers))
        else {
            // 两个数组的步长都是 3；长度对不上就是调用方写错了。
            return HitOutcome::err(if flat_obstacles.len() % 3 != 0 {
                E_BAD_OBSTACLES
            } else {
                E_BAD_SOLDIERS
            });
        };
        let outcome = hit_test(&points, &obstacles, &soldiers);
        write_dict(api, ret as *mut c_void, hit_result_pairs(&outcome));
        outcome
    }))
    .unwrap_or_else(|_| HitOutcome::err(E_PANIC));
    if out.ok {
        HIT_OK_CALLS.fetch_add(1, Ordering::Relaxed);
    }
}

/// GDScript 调 `level_obstacles` 时走这里（`docs/126` §1）。
///
/// 参数约定：
///
/// ```text
/// level_obstacles(level_index: int, seed: int) -> Dictionary { ok, obstacles, error }
/// ```
///
/// 与 `path_calculate` / `hit_test` 相同的三条硬要求：
/// 1. 整个函数体包在 `catch_unwind` 里（跨 FFI 的 panic 会 abort 掉 Godot）；
/// 2. 出错一律走 `ok:false + error`，不 panic；
/// 3. `ret` 是 Godot 给的**空 Variant 槽**，用 `write_dict` 填。
unsafe extern "C" fn level_obstacles_call(
    _ud: *mut c_void,
    _instance: *mut c_void,
    args: *const *const c_void,
    argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    OBST_CALLS.fetch_add(1, Ordering::Relaxed);
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(api) = api() else {
            return ObstacleOutcome::err(E_PANIC);
        };
        if argc < 2 || args.is_null() || ret.is_null() {
            return ObstacleOutcome::err(E_BAD_ARGS);
        }
        let arg = |i: usize| *args.add(i);
        let (Some(level_index), Some(seed)) = (api.get_i64(arg(0)), api.get_i64(arg(1))) else {
            // level_index 可以是"整值的 FLOAT"，但 4.5 这种一律拒绝（见 `Api::get_i64`）。
            return ObstacleOutcome::err(E_BAD_ARGS);
        };
        let outcome = level_obstacles(level_index, seed);
        write_dict(api, ret, obstacle_pairs(&outcome));
        outcome
    }))
    .unwrap_or_else(|_| ObstacleOutcome::err(E_PANIC));
    if out.ok {
        OBST_OK_CALLS.fetch_add(1, Ordering::Relaxed);
        OBST_LAST_COUNT.store(out.obstacles.len(), Ordering::Relaxed);
    }
}

/// `[a, b, c, d, e, f]` → `[(a,b,c), (d,e,f)]`；长度不是 3 的倍数返回 `None`。
fn triples(flat: &[f64]) -> Option<Vec<(f64, f64, f64)>> {
    if flat.len() % 3 != 0 {
        return None;
    }
    Some(flat.chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect())
}

/// 全部接口函数指针；`API` 是 `OnceLock`，两次注册共用同一张表。
unsafe fn build_table(get: GetProc) -> Api {
    Api {
        from_ctor: sym(get, "get_variant_from_type_constructor"),
        to_ctor: sym(get, "get_variant_to_type_constructor"),
        ptr_ctor: sym(get, "variant_get_ptr_constructor"),
        variant_call: sym(get, "variant_call"),
        get_type: sym(get, "variant_get_type"),
        variant_destroy: sym(get, "variant_destroy"),
        raw_dtor: sym(get, "variant_get_ptr_destructor"),
        sn_new: sym(get, "string_name_new_with_latin1_chars"),
        str_new: sym(get, "string_new_with_utf8_chars"),
        str_to_utf8: sym(get, "string_to_utf8_chars"),
        register_method: sym(get, "classdb_register_extension_class_method"),
        vec2_opindex: sym(get, "packed_vector2_array_operator_index_const"),
        f64_opindex: sym(get, "packed_float64_array_operator_index_const"),
    }
}

/// 注册 `path_calculate` 需要的最小接口集合是否齐备（缺一个就整体不注册）。
fn table_ok(table: &Api) -> bool {
    table.sn_new != 0
        && table.str_new != 0
        && table.str_to_utf8 != 0
        && table.from_ctor != 0
        && table.to_ctor != 0
        && table.ptr_ctor != 0
        && table.variant_call != 0
        && table.get_type != 0
        && table.register_method != 0
}

/// 把方法名 `set` / `append` / `size` 的 `StringName` 建一次，之后复用。
unsafe fn install_names() -> bool {
    let Some(api) = api() else { return false };
    let (Some(set_sn), Some(append_sn), Some(size_sn)) =
        (api.sn("set"), api.sn("append"), api.sn("size"))
    else {
        return false;
    };
    let _ = NAMES.set(Names { set: set_sn, append: append_sn, size: size_sn });
    true
}

/// 给已注册的 `GW2Rebuild` 类挂上 `path_calculate(expr, x0, y0, direction)`。
///
/// # Safety
/// `get` 必须是 Godot 的 `get_proc_address`，且必须在 `register_class` **之后**调用。
pub unsafe fn register_path_method(get: GetProc, library: *mut c_void) -> bool {
    let table = build_table(get);
    eprintln!(
        "[gw2-rebuild] api resolve: from_ctor={} to_ctor={} ptr_ctor={} variant_call={} \
         get_type={} variant_destroy={} raw_dtor={} sn_new={} str_new={} str_to_utf8={} \
         register_method={} vec2_opindex={} f64_opindex={}",
        table.from_ctor != 0,
        table.to_ctor != 0,
        table.ptr_ctor != 0,
        table.variant_call != 0,
        table.get_type != 0,
        table.variant_destroy != 0,
        table.raw_dtor != 0,
        table.sn_new != 0,
        table.str_new != 0,
        table.str_to_utf8 != 0,
        table.register_method != 0,
        table.vec2_opindex != 0,
        table.f64_opindex != 0,
    );
    // 缺任何一个都不能注册：宁可让客户端继续用预览，也不要一半可用
    if !table_ok(&table) {
        eprintln!("[gw2-rebuild] path_calculate NOT registered (missing interface)");
        return false;
    }

    let f_reg: unsafe extern "C" fn(*mut c_void, *const c_void, *const ClassMethodInfo) =
        std::mem::transmute(table.register_method);
    let mut cls_sn = [0u8; 8];
    let f_sn: FnSnNew = std::mem::transmute(table.sn_new);
    f_sn(cls_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);

    // 方法名 + 参数名（StringName）与空 StringName/String 都得活到注册调用结束
    let mut meth_sn = [0u8; 8];
    f_sn(meth_sn.as_mut_ptr() as *mut c_void, b"path_calculate\0".as_ptr(), 0);
    let empty_sn = [0u8; 8]; // 默认构造的 StringName（内部指针为空，合法）
    let empty_str = [0u8; 8]; // 默认构造的 String
    let mut arg_names = [[0u8; 8]; 4];
    for (slot, name) in arg_names.iter_mut().zip(["expr\0", "x0\0", "y0\0", "direction\0"]) {
        f_sn(slot.as_mut_ptr() as *mut c_void, name.as_ptr(), 0);
    }
    let mut ret_name = [0u8; 8];
    f_sn(ret_name.as_mut_ptr() as *mut c_void, b"result\0".as_ptr(), 0);

    let ret_info = PropertyInfo {
        ty: V_DICTIONARY,
        name: ret_name.as_ptr() as *const c_void,
        class_name: empty_sn.as_ptr() as *const c_void,
        hint: 0,
        hint_string: empty_str.as_ptr() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    let arg_types = [V_STRING, V_FLOAT, V_FLOAT, V_FLOAT];
    let arg_info: Vec<PropertyInfo> = (0..4)
        .map(|i| PropertyInfo {
            ty: arg_types[i],
            name: arg_names[i].as_ptr() as *const c_void,
            class_name: empty_sn.as_ptr() as *const c_void,
            hint: 0,
            hint_string: empty_str.as_ptr() as *const c_void,
            usage: USAGE_DEFAULT,
        })
        .collect();
    let arg_meta: [i32; 4] = [0; 4];

    let info = ClassMethodInfo {
        name: meth_sn.as_ptr() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(path_calculate_call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: 1,
        return_value_info: &ret_info,
        return_value_metadata: 0,
        argument_count: 4,
        arguments_info: arg_info.as_ptr(),
        arguments_metadata: arg_meta.as_ptr() as *const c_void,
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };

    // 先建好 API 表（call_func 里要用）
    let _ = API.set(table);
    let Some(_api) = api() else { return false };
    // 方法名 StringName 复用（省掉每次调用的表查找）
    if !install_names() {
        eprintln!("[gw2-rebuild] path_calculate: cannot build method StringNames");
        return false;
    }

    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!("[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)");
    true
}

/// 给已注册的 `GW2Rebuild` 类挂上
/// `path_calculate_mode(expr, x0, y0, direction, mode) -> Dictionary`（`docs/138`）。
///
/// 与 [`register_path_method`] 的差别只有"多一个 `String` 参数"：
/// `class_name` / `hint_string` 一律指向**默认构造的空** `StringName` / `String`
/// （传 NULL 会 SIGSEGV，`docs/116` §3.3 的实测坑），`arguments_metadata` 给 5 个 0。
///
/// 必须**在** [`register_path_method`] **之后**调用：接口表是同一个 `OnceLock`，
/// 这里只是复用（`build_table` 的返回值会被 `API.set` 忽略）。
///
/// # Safety
/// `get` / `library` 的约束同 [`register_path_method`]。
pub unsafe fn register_path_mode_method(get: GetProc, library: *mut c_void) -> bool {
    let table = build_table(get);
    if !table_ok(&table) {
        eprintln!("[gw2-rebuild] path_calculate_mode NOT registered (missing interface)");
        return false;
    }
    let _ = API.set(table);
    let Some(api) = api() else { return false };
    if !install_names() {
        eprintln!("[gw2-rebuild] path_calculate_mode: cannot build method StringNames");
        return false;
    }

    let f_reg: unsafe extern "C" fn(*mut c_void, *const c_void, *const ClassMethodInfo) =
        std::mem::transmute(api.register_method);
    let mut cls_sn = [0u8; 8];
    let f_sn: FnSnNew = std::mem::transmute(api.sn_new);
    f_sn(cls_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);

    let mut meth_sn = [0u8; 8];
    f_sn(meth_sn.as_mut_ptr() as *mut c_void, b"path_calculate_mode\0".as_ptr(), 0);
    let empty_sn = [0u8; 8];
    let empty_str = [0u8; 8];
    let mut arg_names = [[0u8; 8]; 5];
    for (slot, name) in arg_names
        .iter_mut()
        .zip(["expr\0", "x0\0", "y0\0", "direction\0", "mode\0"])
    {
        f_sn(slot.as_mut_ptr() as *mut c_void, name.as_ptr(), 0);
    }
    let mut ret_name = [0u8; 8];
    f_sn(ret_name.as_mut_ptr() as *mut c_void, b"result\0".as_ptr(), 0);

    let ret_info = PropertyInfo {
        ty: V_DICTIONARY,
        name: ret_name.as_ptr() as *const c_void,
        class_name: empty_sn.as_ptr() as *const c_void,
        hint: 0,
        hint_string: empty_str.as_ptr() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    // 第 5 个参数是 mode（String），不是 int：见 `parse_mode` 的文档。
    let arg_types = [V_STRING, V_FLOAT, V_FLOAT, V_FLOAT, V_STRING];
    let arg_info: Vec<PropertyInfo> = (0..5)
        .map(|i| PropertyInfo {
            ty: arg_types[i],
            name: arg_names[i].as_ptr() as *const c_void,
            class_name: empty_sn.as_ptr() as *const c_void,
            hint: 0,
            hint_string: empty_str.as_ptr() as *const c_void,
            usage: USAGE_DEFAULT,
        })
        .collect();
    let arg_meta: [i32; 5] = [0; 5];

    let info = ClassMethodInfo {
        name: meth_sn.as_ptr() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(path_calculate_mode_call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: 1,
        return_value_info: &ret_info,
        return_value_metadata: 0,
        argument_count: 5,
        arguments_info: arg_info.as_ptr(),
        arguments_metadata: arg_meta.as_ptr() as *const c_void,
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };

    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!(
        "[gw2-rebuild] registered method path_calculate_mode(expr, x0, y0, direction, mode)"
    );
    true
}

/// 给已注册的 `GW2Rebuild` 类挂上
/// `hit_test(points, obstacles, soldiers) -> Dictionary`（`docs/120`）。
///
/// 必须**在** [`register_path_method`] **之后**调用：那张接口表是同一个 `OnceLock`，
/// 这里只是复用（`build_table` 的返回值会被 `API.set` 忽略）。
///
/// 与 `path_calculate` 的差别只有一条：这里额外要求两个
/// `packed_*_array_operator_index_const`（读 `Packed*Array` 的元素）。
///
/// # Safety
/// `get` / `library` 的约束同 [`register_path_method`]。
pub unsafe fn register_hit_test_method(get: GetProc, library: *mut c_void) -> bool {
    let table = build_table(get);
    if !table_ok(&table) {
        eprintln!("[gw2-rebuild] hit_test NOT registered (missing interface)");
        return false;
    }
    if table.vec2_opindex == 0 || table.f64_opindex == 0 {
        // 本机 4.7.2 两个都在；缺一个就整体不注册（客户端会退回近似判定并显示出来）。
        eprintln!("[gw2-rebuild] hit_test NOT registered (no packed_*_array_operator_index_const)");
        return false;
    }
    let _ = API.set(table);
    let Some(api) = api() else { return false };
    if !install_names() {
        eprintln!("[gw2-rebuild] hit_test: cannot build method StringNames");
        return false;
    }

    let f_reg: unsafe extern "C" fn(*mut c_void, *const c_void, *const ClassMethodInfo) =
        std::mem::transmute(api.register_method);
    let mut cls_sn = [0u8; 8];
    let f_sn: FnSnNew = std::mem::transmute(api.sn_new);
    f_sn(cls_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);

    let mut meth_sn = [0u8; 8];
    f_sn(meth_sn.as_mut_ptr() as *mut c_void, b"hit_test\0".as_ptr(), 0);
    let empty_sn = [0u8; 8];
    let empty_str = [0u8; 8];
    let mut arg_names = [[0u8; 8]; 3];
    for (slot, name) in arg_names.iter_mut().zip(["points\0", "obstacles\0", "soldiers\0"]) {
        f_sn(slot.as_mut_ptr() as *mut c_void, name.as_ptr(), 0);
    }
    let mut ret_name = [0u8; 8];
    f_sn(ret_name.as_mut_ptr() as *mut c_void, b"result\0".as_ptr(), 0);

    let ret_info = PropertyInfo {
        ty: V_DICTIONARY,
        name: ret_name.as_ptr() as *const c_void,
        class_name: empty_sn.as_ptr() as *const c_void,
        hint: 0,
        hint_string: empty_str.as_ptr() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    let arg_types = [V_PACKED_VECTOR2_ARRAY, V_PACKED_FLOAT64_ARRAY, V_PACKED_FLOAT64_ARRAY];
    let arg_info: Vec<PropertyInfo> = (0..3)
        .map(|i| PropertyInfo {
            ty: arg_types[i],
            name: arg_names[i].as_ptr() as *const c_void,
            class_name: empty_sn.as_ptr() as *const c_void,
            hint: 0,
            hint_string: empty_str.as_ptr() as *const c_void,
            usage: USAGE_DEFAULT,
        })
        .collect();
    let arg_meta: [i32; 3] = [0; 3];

    let info = ClassMethodInfo {
        name: meth_sn.as_ptr() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(hit_test_call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: 1,
        return_value_info: &ret_info,
        return_value_metadata: 0,
        argument_count: 3,
        arguments_info: arg_info.as_ptr(),
        arguments_metadata: arg_meta.as_ptr() as *const c_void,
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };

    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!("[gw2-rebuild] registered method hit_test(points, obstacles, soldiers)");
    true
}

/// 给已注册的 `GW2Rebuild` 类挂上 `level_obstacles(level_index, seed) -> Dictionary`
/// （`docs/126`）。
///
/// 必须在 [`register_path_method`] **之后**调用（接口表是同一个 `OnceLock`）。
/// 这是三者里最省的一个：只要 `path_calculate` 那一套接口齐备就够 ——
/// **不**需要 `packed_*_array_operator_index_const`（那两个只在**读**数组时用；
/// 这里只**写**一个 `PackedFloat64Array`，走 `append`）。
///
/// # Safety
/// `get` / `library` 的约束同 [`register_path_method`]。
pub unsafe fn register_level_obstacles_method(get: GetProc, library: *mut c_void) -> bool {
    let table = build_table(get);
    if !table_ok(&table) {
        eprintln!("[gw2-rebuild] level_obstacles NOT registered (missing interface)");
        return false;
    }
    let _ = API.set(table);
    let Some(api) = api() else { return false };
    if !install_names() {
        eprintln!("[gw2-rebuild] level_obstacles: cannot build method StringNames");
        return false;
    }

    let f_reg: unsafe extern "C" fn(*mut c_void, *const c_void, *const ClassMethodInfo) =
        std::mem::transmute(api.register_method);
    let mut cls_sn = [0u8; 8];
    let f_sn: FnSnNew = std::mem::transmute(api.sn_new);
    f_sn(cls_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);

    let mut meth_sn = [0u8; 8];
    f_sn(meth_sn.as_mut_ptr() as *mut c_void, b"level_obstacles\0".as_ptr(), 0);
    let empty_sn = [0u8; 8];
    let empty_str = [0u8; 8];
    let mut arg_names = [[0u8; 8]; 2];
    for (slot, name) in arg_names.iter_mut().zip(["level_index\0", "seed\0"]) {
        f_sn(slot.as_mut_ptr() as *mut c_void, name.as_ptr(), 0);
    }
    let mut ret_name = [0u8; 8];
    f_sn(ret_name.as_mut_ptr() as *mut c_void, b"result\0".as_ptr(), 0);

    let ret_info = PropertyInfo {
        ty: V_DICTIONARY,
        name: ret_name.as_ptr() as *const c_void,
        class_name: empty_sn.as_ptr() as *const c_void,
        hint: 0,
        hint_string: empty_str.as_ptr() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    // 两个参数都是 int：GDScript 的 `int` 就是 Variant::INT（64 位）。
    let arg_types = [V_INT, V_INT];
    let arg_info: Vec<PropertyInfo> = (0..2)
        .map(|i| PropertyInfo {
            ty: arg_types[i],
            name: arg_names[i].as_ptr() as *const c_void,
            class_name: empty_sn.as_ptr() as *const c_void,
            hint: 0,
            hint_string: empty_str.as_ptr() as *const c_void,
            usage: USAGE_DEFAULT,
        })
        .collect();
    let arg_meta: [i32; 2] = [0; 2];

    let info = ClassMethodInfo {
        name: meth_sn.as_ptr() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(level_obstacles_call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: 1,
        return_value_info: &ret_info,
        return_value_metadata: 0,
        argument_count: 2,
        arguments_info: arg_info.as_ptr(),
        arguments_metadata: arg_meta.as_ptr() as *const c_void,
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };

    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!("[gw2-rebuild] registered method level_obstacles(level_index, seed)");
    true
}

// ---------------------------------------------------------------------------
// 7. 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphwar_shot_calculator::shot_points;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    /// 若 `calculate_path` 与已验证的 `graphwar_shot_calculator::shot_points` 不一致，说明这里偷偷
    /// 重新实现了采样器 —— 这条测试就是防它的。
    #[test]
    fn matches_shot_points_pipeline() {
        let cases = [
            ("sin(x)", 5.0, 2.0),
            ("x/5", -3.0, -4.0),
            ("cos(x/3)", 0.0, 0.0),
            ("sqrt(abs(x))", -2.0, 1.0),
            ("1/x", 4.0, 0.0),
        ];
        for (expr, x0, y0) in cases {
            for dir in [1.0f64, -1.0f64] {
                let ours = calculate_path(expr, x0, y0, dir);
                let theirs =
                    shot_points(expr, x0, y0, STEP_DEFAULT, dir < 0.0, Mode::Normal, true);
                assert_eq!(ours.points, theirs, "{expr} x0={x0} y0={y0} dir={dir} 点序列不同");
                assert!(ours.ok, "{expr} 应当算得出来");
                assert_eq!(ours.error, "");
            }
        }
    }

    #[test]
    fn normal_curve_right() {
        let out = calculate_path("sin(x)", 5.0, 2.0, 1.0);
        assert!(out.ok, "err={}", out.error);
        assert!(out.points.len() > 100, "点数 {}", out.points.len());
        assert_eq!(out.points[0], (5.0, 2.0), "首点必须是士兵位置");
        // 首点之后必须向右走
        assert!(out.points[1].0 > out.points[0].0, "向右方向 x 应递增");
        // 相邻点弦长 = 0.01（±容差）；用前 50 段，避免端点效应
        for w in out.points.windows(2).take(50) {
            let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
            let chord = dx.hypot(dy);
            assert!(close(chord, STEP_DEFAULT, 2e-3), "弦长 {chord} 不等于 step");
        }
        // 曲线必须穿过士兵：修正后 y(x0) == y0
        let c = out.offset;
        assert!(close((5.0f64).sin() + c, 2.0, 1e-9), "修正常数不对");
        // 平面边界
        assert!(out.points.iter().all(|p| p.0.abs() <= MAX_ABS_X && p.1.abs() <= MAX_ABS_Y));
    }

    #[test]
    fn normal_curve_left_uses_f_of_minus_x() {
        let right = calculate_path("sin(x)", 5.0, 2.0, 1.0);
        let left = calculate_path("sin(x)", 5.0, 2.0, -1.0);
        assert!(right.ok && left.ok);
        assert_eq!(left.points[0], (5.0, 2.0), "向左时首点仍是士兵位置");
        assert!(left.points[1].0 < left.points[0].0, "向左方向 x 应递减");
        // 修正常数的 ± 由方向决定（docs/105 / docs/05 §2）
        assert!(close(left.offset, 2.0 - (-5.0f64).sin(), 1e-9), "向左取 c = y0 − f(−x0)");
        assert!(close(right.offset, 2.0 - (5.0f64).sin(), 1e-9), "向右取 c = y0 − f(x0)");
        // 采样器在 flip 时用 `mid` 求值、把 x 取负存点：点满足 y = f(−x) + c，而不是 f(x) + c。
        // 这就是"向左打"的几何含义（曲线是 f 关于 y 轴的镜像，不是同一条曲线的反着走）。
        for &(x, y) in left.points.iter().take(200) {
            assert!(close(y, (-x as f64).sin() + left.offset, 1e-9), "flip 点应满足 y=f(−x)+c");
        }
        for &(x, y) in right.points.iter().take(200) {
            assert!(close(y, (x as f64).sin() + right.offset, 1e-9), "非 flip 点应满足 y=f(x)+c");
        }
    }

    #[test]
    fn horizontal_line_offset_is_y0() {
        // 抓包样本：`0.00` 配 4.638 → 水平直线（docs/05 §2 的真实抓包）
        let out = calculate_path("0.00", 1.651220991201045, 4.638020825690482, 1.0);
        assert!(out.ok, "err={}", out.error);
        assert!(out.points.iter().all(|p| close(p.1, 4.638020825690482, 1e-12)));
    }

    #[test]
    fn invalid_expression_reports_error() {
        // 词法切不出任何 token
        for bad in ["@@@", "   ", "???"] {
            let out = calculate_path(bad, 0.0, 0.0, 1.0);
            assert!(!out.ok, "{bad:?} 不应成功");
            assert_eq!(out.error, E_EMPTY);
            assert!(out.points.is_empty());
            assert!(out.offset.is_nan());
        }
    }

    #[test]
    fn start_out_of_bounds_is_rejected() {
        for (x0, y0) in [(25.5, 0.0), (-25.5, 0.0), (0.0, 15.5), (0.0, -15.5)] {
            let out = calculate_path("sin(x)", x0, y0, 1.0);
            assert!(!out.ok, "({x0},{y0}) 越界却成功了");
            assert_eq!(out.error, E_START_BOUNDS);
        }
        // 正好在边界上应当允许（采样器本身检查 |x| <= 25）
        let edge = calculate_path("x", 25.0, 15.0, -1.0);
        assert!(edge.ok || edge.error == E_NO_POINTS, "边界点不应被判成越界：{}", edge.error);
    }

    #[test]
    fn non_finite_start_is_rejected() {
        let out = calculate_path("sin(x)", f64::NAN, 0.0, 1.0);
        assert_eq!(out.error, E_START_FINITE);
        let out = calculate_path("sin(x)", 0.0, f64::INFINITY, 1.0);
        assert_eq!(out.error, E_START_FINITE);
    }

    #[test]
    fn undefined_at_start_is_rejected() {
        // sqrt(-1) = NaN ⇒ 修正常数 NaN ⇒ 没有有限的首点
        let out = calculate_path("sqrt(x)", -1.0, 0.0, 1.0);
        assert!(!out.ok);
        assert_eq!(out.error, E_UNDEFINED_AT_START);
        let out = calculate_path("ln(x)", -2.0, 0.0, 1.0);
        assert_eq!(out.error, E_UNDEFINED_AT_START);
    }

    #[test]
    fn leaving_the_plane_immediately_gives_no_points() {
        // `x` 在 (25, 0) 处：修正常数 c = −25 ⇒ y = x − 25，
        // 第一段收敛在 mid ≈ 25.005，超过 |x| <= 25 ⇒ 采样器只留下首点。
        let out = calculate_path("x", 25.0, 0.0, 1.0);
        assert!(!out.ok, "越界却成功了：{} 点", out.points.len());
        assert_eq!(out.error, E_NO_POINTS);
        assert_eq!(out.points, vec![(25.0, 0.0)], "越界点不进入点序列");
        assert!(close(out.offset, -25.0, 1e-12));
    }

    #[test]
    fn direction_mapping_matches_the_client() {
        // core_bridge.gd: `1.0 if direction >= 0.0 else -1.0`；负 = 向左 = flip
        assert!(!direction_is_flip(1.0));
        assert!(!direction_is_flip(0.0));
        assert!(direction_is_flip(-1.0));
        assert!(direction_is_flip(-0.5));
    }

    #[test]
    fn point_budget_is_the_sampler_budget() {
        // 0.01 步长 ⇒ (200/0.01) as i32 = 20000 点上限（AGENTS.md 硬事实）
        let out = calculate_path("sin(x)", 0.0, 0.0, 1.0);
        assert!(out.points.len() <= 20_001, "点数 {} 超过预算", out.points.len());
    }

    #[test]
    fn result_pairs_shape_is_exactly_the_contract() {
        let ok = calculate_path("sin(x)", 5.0, 2.0, 1.0);
        let pairs = result_pairs(&ok);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, RESULT_KEYS.to_vec(), "键名/顺序必须与 core_bridge.gd 一致");
        assert_eq!(pairs[0].1, ApiValue::Bool(true));
        match &pairs[1].1 {
            ApiValue::Points(p) => assert_eq!(p.len(), ok.points.len()),
            other => panic!("points 类型不对：{other:?}"),
        }
        assert_eq!(pairs[2].1, ApiValue::F64(ok.offset));
        assert_eq!(pairs[3].1, ApiValue::Str(String::new()));

        let bad = calculate_path("@@@", 0.0, 0.0, 1.0);
        let pairs = result_pairs(&bad);
        assert_eq!(pairs[0].1, ApiValue::Bool(false));
        assert_eq!(pairs[3].1, ApiValue::Str(E_EMPTY.to_string()));
        match &pairs[1].1 {
            ApiValue::Points(p) => assert!(p.is_empty()),
            other => panic!("points 类型不对：{other:?}"),
        }
        match &pairs[2].1 {
            ApiValue::F64(v) => assert!(v.is_nan(), "出错时 offset 应为 NaN"),
            other => panic!("offset 类型不对：{other:?}"),
        }
    }

    #[test]
    fn no_panic_on_extreme_inputs() {
        // 越怪越不能 panic：跨 FFI 的 panic 会崩掉 Godot
        let inputs = [
            ("))))", 0.0, 0.0, 1.0),
            ("sin(", 0.0, 0.0, 1.0),
            ("1/0", 0.0, 0.0, 1.0),
            ("", 0.0, 0.0, 1.0),
            ("x^999999", 0.0, 0.0, 1.0),
            ("tan(x)", 24.999, 0.0, 1.0),
            ("sin(x)", 1e308, 1e308, 1.0),
            ("0.00", -25.0, -15.0, -1.0),
        ];
        for (expr, x0, y0, dir) in inputs {
            let out = calculate_path(expr, x0, y0, dir);
            // 只要不 panic、且不变量成立就算过
            if out.ok {
                assert!(out.points.len() >= 2);
                assert!(out.offset.is_finite());
            }
        }
    }

    // -----------------------------------------------------------------------
    // 微分方程模式（`docs/138`）
    // -----------------------------------------------------------------------
    //
    // 四条硬要求，每条一个测试：
    //   1. `mode="ode"` 能出点，而且真的是 RK4 积分（对照解析解）；
    //   2. `mode="normal"` 与 `calculate_path` **逐位**相同（不许改坏既有契约）；
    //   3. ODE 的点序列 == `shot_points(..., Mode::Ode, apply_correction=false)`
    //      （= "ODE 不做修正常数" 与既有端到端入口同一个语义）；
    //   4. 非法 mode 走错误分支、不 panic。

    /// `dy/dx = cos(x)` 从 (0,0) 出发 ⇒ `y = sin(x)`（RK4 的解析对照）。
    ///
    /// 这条同时证明"ODE 不是把 `f` 当 `y=f(x)` 画"：Normal 模式在同一起点给出
    /// `y = cos(x) − 1`，两条点序列必然不同。
    #[test]
    fn ode_mode_integrates_the_derivative() {
        let out = calculate_path_mode("cos(x)", 0.0, 0.0, 1.0, "ode");
        assert!(out.ok, "err={}", out.error);
        assert!(out.points.len() > 100, "点数 {}", out.points.len());
        assert_eq!(out.points[0], (0.0, 0.0), "首点仍是士兵位置");
        assert!(out.offset.is_nan(), "ODE 没有修正常数 ⇒ offset 必须是 NaN");
        // RK4 + 弦长容差 0.001 ⇒ 与解析解差在 5e-3 内（x 方向最多差一个容差）
        for &(x, y) in out.points.iter().take(300) {
            assert!(close(y, x.sin(), 5e-3), "在 x={x} 处 y={y}，解析解 {}", x.sin());
        }
        // 反面：Normal 给出的是 y = cos(x) − 1，不是积分曲线
        let normal = calculate_path("cos(x)", 0.0, 0.0, 1.0);
        assert!(normal.ok, "err={}", normal.error);
        assert_ne!(out.points, normal.points, "ODE 与 Normal 给出了同一条点序列");
    }

    /// ODE 的右端真的能用 `y`：`dy/dx = y` 从 (0,1) 出发 ⇒ `y = e^x`。
    ///
    /// Normal 模式下第三个变量 `y` 恒被求值成 `0`（AGENTS.md 的"死特性"），
    /// 同一表达式会变成水平直线 —— 这条是"两种模式确实走了不同分支"的最强证据。
    #[test]
    fn ode_mode_uses_the_y_variable() {
        let ode = calculate_path_mode("y", 0.0, 1.0, 1.0, "ode");
        assert!(ode.ok, "err={}", ode.error);
        assert!(ode.points.len() > 100, "点数 {}", ode.points.len());
        for &(x, y) in ode.points.iter().take(100) {
            assert!(close(y, x.exp(), 2e-3), "在 x={x} 处 y={y}，解析解 {}", x.exp());
        }
        let normal = calculate_path("y", 0.0, 1.0, 1.0);
        assert!(normal.ok, "err={}", normal.error);
        assert!(normal.points.iter().all(|p| p.1 == 1.0), "Normal 下 y 变量恒 0 ⇒ 水平线");
        assert_ne!(ode.points, normal.points, "ODE 与 Normal 给出了同一条点序列");
    }

    /// `mode="normal"`（含大小写/空白变体）与 `path_calculate` **逐位**相同。
    ///
    /// 覆盖 5 种成功输入 + 5 种错误分支（错误码与 `offset` 也要一致）。
    #[test]
    fn normal_mode_is_bit_identical_to_calculate_path() {
        let cases = [
            ("sin(x)", 5.0, 2.0),
            ("x/5", -3.0, -4.0),
            ("cos(x/3)", 0.0, 0.0),
            ("sqrt(abs(x))", -2.0, 1.0),
            ("1/x", 4.0, 0.0),
            ("@@@", 0.0, 0.0),         // empty_expression
            ("sin(x)", 25.5, 0.0),     // start_out_of_bounds
            ("sqrt(x)", -1.0, 0.0),    // undefined_at_start
            ("x", 25.0, 0.0),          // no_points
            ("sin(x)", f64::NAN, 0.0), // start_not_finite
        ];
        for (expr, x0, y0) in cases {
            for dir in [1.0f64, -1.0f64] {
                let want = calculate_path(expr, x0, y0, dir);
                for mode in ["normal", "NORMAL", "  Normal  "] {
                    let got = calculate_path_mode(expr, x0, y0, dir, mode);
                    assert_eq!(got.ok, want.ok, "{expr} dir={dir} mode={mode:?} 的 ok");
                    assert_eq!(got.error, want.error, "{expr} mode={mode:?} 的 error");
                    assert_eq!(
                        got.points.len(),
                        want.points.len(),
                        "{expr} mode={mode:?} 的点数"
                    );
                    for (g, w) in got.points.iter().zip(want.points.iter()) {
                        assert_eq!(g.0.to_bits(), w.0.to_bits(), "{expr} mode={mode:?} 的 x");
                        assert_eq!(g.1.to_bits(), w.1.to_bits(), "{expr} mode={mode:?} 的 y");
                    }
                    assert_eq!(
                        got.offset.to_bits(),
                        want.offset.to_bits(),
                        "{expr} mode={mode:?} 的 offset"
                    );
                }
            }
        }
    }

    /// ODE 的点序列 == `shot_points(..., Mode::Ode, apply_correction = false)` 逐点相等。
    ///
    /// 这条钉住的是 `src/graphwar_shot_calculator.rs` 的那一行判断：
    /// `if apply_correction && mode == Mode::Normal { … }` —— 微分方程局里
    /// **修正常数根本不参与**，API 层不许自己加一个。
    #[test]
    fn ode_matches_shot_points_without_correction() {
        let cases = [
            ("cos(x)", 0.0, 0.0),
            ("y", 0.0, 1.0),
            ("x-y", 2.0, 1.0),
            ("sin(x*y/4)", -1.0, 0.5),
        ];
        for (expr, x0, y0) in cases {
            for dir in [1.0f64, -1.0f64] {
                let ours = calculate_path_mode(expr, x0, y0, dir, "ode");
                // 端到端既有入口：同一个采样器、同一个 Mode，只是不经过 api 层
                let theirs =
                    shot_points(expr, x0, y0, STEP_DEFAULT, dir < 0.0, Mode::Ode, false);
                assert_eq!(ours.points, theirs, "{expr} dir={dir} 与 shot_points 不一致");
                assert!(ours.ok, "{expr} dir={dir} err={}", ours.error);
            }
        }
        // `apply_correction = true` 对 ODE **无效**（同一行判断的 `mode == Normal` 那一半）：
        // 两次调用必须一模一样。谁在 api 层偷偷加修正，这条就会红。
        let a = shot_points("y", 0.0, 1.0, STEP_DEFAULT, false, Mode::Ode, false);
        let b = shot_points("y", 0.0, 1.0, STEP_DEFAULT, false, Mode::Ode, true);
        assert_eq!(a, b, "ODE 模式下 apply_correction 应当被忽略");
    }

    /// 非法 mode：`ok:false + bad_arguments`（沿用既有错误码），不 panic、不猜默认值。
    #[test]
    fn invalid_mode_is_an_error_not_a_panic() {
        for mode in ["", " ", "0", "1", "od", "ode2", "diffeq", "normal2", "-", "正常"] {
            let out = calculate_path_mode("sin(x)", 0.0, 0.0, 1.0, mode);
            assert!(!out.ok, "mode={mode:?} 不应成功");
            assert_eq!(out.error, E_BAD_ARGS, "mode={mode:?} 应走 bad_arguments");
            assert!(out.points.is_empty(), "mode={mode:?} 不该有点");
            assert!(out.offset.is_nan(), "mode={mode:?} 出错时 offset 应为 NaN");
        }
        // 合法的两种写法（大小写/空白不敏感）必须成功
        for mode in ["ode", "ODE", "  Ode  "] {
            assert!(calculate_path_mode("cos(x)", 0.0, 0.0, 1.0, mode).ok, "mode={mode:?}");
        }
    }

    /// ODE 模式的错误分支沿用既有错误码。
    ///
    /// 唯一的差异（有意保留）：ODE 没有修正常数 ⇒ **不会**报 `undefined_at_start`；
    /// 导数在起点无定义会让整段不收敛，退化成 `no_points`（采样器既有语义）。
    #[test]
    fn ode_error_codes_are_the_shared_ones() {
        assert_eq!(
            calculate_path_mode("y", f64::INFINITY, 0.0, 1.0, "ode").error,
            E_START_FINITE
        );
        assert_eq!(
            calculate_path_mode("y", 25.5, 0.0, 1.0, "ode").error,
            E_START_BOUNDS
        );
        assert_eq!(calculate_path_mode("@@@", 0.0, 0.0, 1.0, "ode").error, E_EMPTY);
        // `1/x` 在 x=0 处导数是 inf ⇒ 整段不接受 ⇒ 只剩首点
        let out = calculate_path_mode("1/x", 0.0, 1.0, 1.0, "ode");
        assert!(!out.ok);
        assert_eq!(out.error, E_NO_POINTS);
        assert_eq!(out.points, vec![(0.0, 1.0)], "坏点不进入点序列");
        assert!(out.offset.is_nan());
    }
}

// ---------------------------------------------------------------------------
// 8. `hit_test` 的测试（`docs/120`）
// ---------------------------------------------------------------------------
//
// 四类必须钉住的东西：
//   1. 正常命中：弹道穿过士兵 → 判死；
//   2. **擦过**：半径边界（`<=` 含等号）与**掩膜抗锯齿环**（`!= 0xFF` 即占据）——
//      后者是"掩膜规则"与"几何圆"真正分家的地方；
//   3. 未命中；
//   4. **坐标换算**：入参是棋盘坐标、掩膜是像素坐标。写错会让命中整体偏 500/300。
//
// 外加一条"没有第二份实现"的等价性测试：把 `tests/live_hits.rs` 里那局真对局的
// 障碍/士兵/函数原样搬过来，比较 `hit_test` 与 `graphwar_shot_calculator::settle::resolve` 的致死集合。

#[cfg(test)]
mod hit_tests {
    use super::*;
    use crate::plane::to_pixel;
    use crate::graphwar_shot_calculator::settle::{resolve, Target};

    /// 棋盘 y = 0 上从 x = `from` 走到 `to` 的直线点序列（步长 0.01，与内核弦长一致）。
    fn line(from: f64, to: f64, y: f64) -> Vec<(f64, f64)> {
        let mut pts = Vec::new();
        let n = ((to - from).abs() / 0.01).ceil() as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            pts.push((from + (to - from) * t, y));
        }
        pts
    }

    // ---- 1. 正常命中 ----

    #[test]
    fn kills_a_soldier_the_trajectory_passes_through() {
        let pts = line(10.0, -10.0, 0.0);
        let out = hit_test(&pts, &[], &[(0.0, 0.0, 0.5)]);
        assert!(out.ok, "err={}", out.error);
        assert_eq!(out.hits.len(), 1, "应恰好判死一个");
        assert_eq!(out.hits[0].soldier, 0);
        assert!(!out.blocked, "士兵不该让弹道停下（docs/30 §2）");
        assert_eq!(out.first_hit, -1, "没有障碍就没有 first_hit");
        // 停止点应是最后一个点（一路飞过去）
        assert!((out.stop.0 - (-10.0)).abs() < 1e-9, "stop={:?}", out.stop);
        // 判死发生在弹道经过士兵处：x ≈ 0（点序列自 +10 向 −10）
        assert!(out.hits[0].x.abs() <= 0.5, "判死点 x={}", out.hits[0].x);
    }

    #[test]
    fn hit_points_are_indices_into_the_trajectory() {
        let pts = line(10.0, -10.0, 0.0);
        let out = hit_test(&pts, &[], &[(0.0, 0.0, 0.5)]);
        let h = out.hits[0];
        assert!(h.point < pts.len(), "点序号越界");
        assert_eq!(pts[h.point], (h.x, h.y), "hit_points 必须就是那个点");
        // 再前面的点都还在半径外（这是"第一次"的判据）
        for p in &pts[..h.point] {
            let d = (p.0 * p.0 + p.1 * p.1).sqrt();
            assert!(d > 0.5, "第 {} 点已在半径内，不该等到 {}", pts.iter().position(|q| q == p).unwrap(), h.point);
        }
    }

    // ---- 2. 擦过 ----

    /// 半径边界是**闭区间**（`d <= radius`）：差 0.001 就跨过去了。
    #[test]
    fn grazes_the_kill_radius_boundary() {
        let pts = line(5.0, -5.0, 0.0);
        let inside = hit_test(&pts, &[], &[(0.0, 0.499, 0.5)]);
        assert_eq!(inside.hits.len(), 1, "0.499 < 0.5 应判死");
        let outside = hit_test(&pts, &[], &[(0.0, 0.501, 0.5)]);
        assert_eq!(outside.hits.len(), 0, "0.501 > 0.5 不该判死");
        // 正好 0.5：`d <= radius` 含等号 ⇒ 判死（判据来自 graphwar_shot_calculator::settle::resolve）
        let exact = hit_test(&pts, &[], &[(0.0, 0.5, 0.5)]);
        assert_eq!(exact.hits.len(), 1, "d == radius 应判死（含等号）");
    }

    /// **掩膜的抗锯齿环也算占据**（`!= 0xFF`）：这是掩膜规则与"几何圆"的分界。
    ///
    /// 障碍 (0,0) size=1.0 ⇒ 像素半径 20；像素 (520, 302) 不在硬填充的行内
    /// （该行 hw=19），落在环上，值 ≈ 25；而它离圆心 1.005 > 1.0。
    /// 几何近似会说"没撞到"，掩膜说"撞到了"——真实规则是后者。
    #[test]
    fn graze_into_the_antialias_ring_still_counts_as_a_hit() {
        let mut mask = crate::plane::empty_mask();
        crate::plane::rasterize(&mut mask, 0.0, 0.0, 1.0, 0x00);
        assert_eq!(crate::plane::to_pixel(1.0, 0.1), (520, 302), "前提：这个点落在环上");
        let v = mask[crate::plane::mask_index(520, 302)];
        assert!(v != crate::plane::EMPTY && v < 0x40, "环上的值应在 (0, 0x40) 之间，实际 {v:#x}");

        let pts = vec![(5.0, 0.1), (1.0, 0.1), (0.0, 0.1)];
        let out = hit_test(&pts, &[(0.0, 0.0, 1.0)], &[]);
        assert_eq!(out.first_hit, 1, "环上的点也要算撞到（first_hit 应为第 1 点）");
        assert!(out.blocked);
        // 反面：再往外一个像素就是空的
        let free = hit_test(&[(5.0, 0.1), (1.1, 0.1)], &[(0.0, 0.0, 1.0)], &[]);
        assert_eq!(free.first_hit, -1, "1.1（离圆心 1.1）应为空");
    }

    // ---- 3. 未命中 ----

    #[test]
    fn misses_when_the_trajectory_never_gets_close() {
        let pts = line(10.0, -10.0, 5.0);
        let out = hit_test(&pts, &[], &[(0.0, 0.0, 0.5)]);
        assert!(out.ok);
        assert!(out.hits.is_empty());
        assert_eq!(out.first_hit, -1);
        assert!(!out.blocked);
    }

    #[test]
    fn obstacle_stops_the_shot_without_killing_anyone() {
        let pts = line(10.0, -10.0, 0.0);
        let out = hit_test(&pts, &[(0.0, 0.0, 1.0)], &[]);
        assert!(out.ok);
        assert!(out.blocked, "障碍应让弹道停下");
        assert!(out.first_hit >= 0, "first_hit 应给出停下那一点");
        let stop = pts[out.first_hit as usize];
        assert_eq!(out.stop, stop, "stop 必须就是 first_hit 那一点");
        assert!(stop.0 > 0.9 && stop.0 < 1.1, "撞击点应在障碍右缘 (1.0)，实际 {}", stop.0);
        assert!(out.hits.is_empty());
    }

    /// 同一次射击里：先经过士兵（判死），再撞障碍（停下）——两者互不干扰。
    #[test]
    fn soldier_dies_before_the_obstacle_stops_the_shot() {
        let pts = line(10.0, -10.0, 0.0);
        let out = hit_test(&pts, &[(-3.0, 0.0, 1.0)], &[(3.0, 0.0, 0.5)]);
        assert!(out.blocked);
        assert_eq!(out.hits.len(), 1, "撞障碍之前经过的士兵应判死");
        // 判死在 x≈3，撞击在 x≈-2 附近
        assert!((out.hits[0].x - 3.0).abs() <= 0.5, "判死点 x={}", out.hits[0].x);
        assert!(out.stop.0 < -1.0, "stop x={}", out.stop.0);
    }

    // ---- 4. 坐标换算（本节最要紧）----

    /// 棋盘 → 像素的换算只有一处（`plane::to_pixel`），常量就是 `20x+500 / 20y+300`。
    #[test]
    fn board_to_pixel_constants_are_20x_plus_500() {
        assert_eq!(to_pixel(0.0, 0.0), (500, 300), "棋盘原点 = 画布中心");
        assert_eq!(to_pixel(10.0, 0.0), (700, 300), "10 单位 = 200 像素");
        assert_eq!(to_pixel(0.0, 5.0), (500, 400));
        assert_eq!(to_pixel(-25.0, -15.0), (0, 0), "左下角");
        assert_eq!(to_pixel(25.0, 15.0), (1000, 600), "右上角（不含）");
    }

    /// 入参确实是**棋盘坐标**：若内核把它们当像素用，命中点会整体挪 500/300。
    ///
    /// 做法：士兵放在棋盘 (10, 0)；正确的换算把它画到像素 (700, 300)。
    /// 若错当像素，它会画到像素 (10, 0) = 棋盘 (-24.5, -15) —— 于是
    /// "走过棋盘 (-24.5,-15) 的弹道会判死"这条就成了错误映射的指纹。
    #[test]
    fn hit_test_does_not_treat_board_coordinates_as_pixels() {
        // 正确：棋盘 (10,0) 上的士兵被穿过 (10,0) 的弹道判死
        let good = hit_test(&line(15.0, 5.0, 0.0), &[], &[(10.0, 0.0, 0.5)]);
        assert_eq!(good.hits.len(), 1, "棋盘 (10,0) 上的士兵应被 y=0 的弹道判死");
        assert!((good.hits[0].x - 10.0).abs() <= 0.5, "判死点 x={}", good.hits[0].x);

        // 错误映射的指纹：走过棋盘 (-24.5,-15) 的弹道**不该**判死任何人
        let bad = hit_test(&line(-24.0, -25.0, -15.0), &[], &[(10.0, 0.0, 0.5)]);
        assert!(bad.hits.is_empty(),
                "在 (-24.5,-15) 判死了 (10,0) 的士兵 —— 说明棋盘坐标被当成像素用了");

        // y 轴同理：士兵 (0,10) 在像素 (500,500)；错当像素会落到 (0,10)→棋盘 (-25,-14.5)
        let good_y = hit_test(&line(3.0, -3.0, 10.0), &[], &[(0.0, 10.0, 0.5)]);
        assert_eq!(good_y.hits.len(), 1, "棋盘 (0,10) 上的士兵应被 y=10 的弹道判死");
        let bad_y = hit_test(&line(-24.0, -25.0, -14.5), &[], &[(0.0, 10.0, 0.5)]);
        assert!(bad_y.hits.is_empty(), "y 方向也把棋盘坐标当成像素了");
    }

    /// 障碍也走同一套换算：棋盘 (0,0) 的障碍占据像素 480..520 / 280..320。
    #[test]
    fn obstacle_mask_uses_the_same_board_to_pixel_mapping() {
        let blocked = hit_test(&line(5.0, -5.0, 0.0), &[(0.0, 0.0, 1.0)], &[]);
        assert!(blocked.blocked);
        // 棋盘 (0, 4) 在像素 (500, 380)，离圆心 80 px，肯定空
        let free = hit_test(&line(5.0, -5.0, 4.0), &[(0.0, 0.0, 1.0)], &[]);
        assert!(!free.blocked, "y=4 的直线不该撞到 (0,0) 上半径 1 的障碍");
        // 若把棋盘坐标当像素：障碍会落在像素 (0,0) = 棋盘 (-25,-15)，那条线会撞上
        let wrong = hit_test(&line(-24.0, -25.0, -15.0), &[(0.0, 0.0, 1.0)], &[]);
        assert!(!wrong.blocked, "在 (-24.5,-15) 撞上了 (0,0) 的障碍 —— 坐标被当成像素了");
    }

    // ---- 5. 契约形状与错误码 ----

    #[test]
    fn result_shape_is_exactly_the_contract() {
        let out = hit_test(&line(5.0, -5.0, 0.0), &[], &[(0.0, 0.0, 0.5)]);
        let pairs = hit_result_pairs(&out);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, HIT_KEYS.to_vec(), "键名与顺序都必须固定");
        match &pairs[1].1 {
            // hits 是**入参下标**，不是 id
            ApiValue::IntArray(v) => assert_eq!(v, &vec![0], "hits 应是士兵下标"),
            other => panic!("hits 类型不对：{other:?}"),
        }
        match &pairs[2].1 {
            ApiValue::Points(v) => assert_eq!(v.len(), 1, "hit_points 应与 hits 一一对应"),
            other => panic!("hit_points 类型不对：{other:?}"),
        }
    }

    #[test]
    fn soldier_indices_follow_the_input_array() {
        let pts = line(10.0, -10.0, 0.0);
        // 两个士兵：下标 0 在 (-4,0)，下标 1 在 (4,0)；弹道自 +10 向 −10
        let out = hit_test(&pts, &[], &[(-4.0, 0.0, 0.5), (4.0, 0.0, 0.5)]);
        assert_eq!(out.hits.len(), 2);
        assert_eq!(
            out.hits.iter().map(|h| h.soldier).collect::<Vec<_>>(),
            vec![1, 0],
            "顺序必须是弹道经过的先后（先 (4,0)），而不是数组顺序"
        );
    }

    #[test]
    fn empty_points_is_an_error_not_a_panic() {
        let out = hit_test(&[], &[], &[(0.0, 0.0, 0.5)]);
        assert!(!out.ok);
        assert_eq!(out.error, E_NO_POINTS_ARR);
        assert_eq!(out.first_hit, -1);
        assert!(out.stop.0.is_nan(), "出错时 stop 用 NaN（客户端靠它判无效）");
    }

    #[test]
    fn nan_points_are_rejected() {
        let out = hit_test(&[(0.0, 0.0), (f64::NAN, 1.0)], &[], &[]);
        assert!(!out.ok);
        assert_eq!(out.error, E_POINTS_NOT_FINITE);
    }

    #[test]
    fn flat_arrays_must_be_multiples_of_three() {
        assert!(triples(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).is_some());
        assert!(triples(&[]).is_some());
        assert!(triples(&[1.0, 2.0, 3.0, 4.0]).is_none());
        let t = triples(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        assert_eq!(t, vec![(1.0, 2.0, 3.0), (4.0, 5.0, 6.0)]);
    }

    // ---- 6. 没有第二份实现：与 graphwar_shot_calculator::settle::resolve 逐项一致 ----

    /// 真对局的三发（障碍/士兵/函数都来自 `tests/live_hits.rs` 的抓包）。
    ///
    /// `hit_test` 与 `graphwar_shot_calculator::settle::resolve` 必须给出**同一个致死集合与同一个停止点**；
    /// 只要有人偷偷在 api.rs 里重写一套判定，这条就会红。
    #[test]
    fn agrees_with_settle_resolve_on_captured_shots() {
        // 抓包里这局的 14 个障碍（docs/30 的死亡关卡）
        const OBSTACLES: [(f64, f64, f64); 14] = [
            (-21.5884800661654, -7.3800965520851, 1.1902918644742717),
            (-0.0043405309383072535, 8.031053141932013, 4.03260643486631),
            (-27.485404156862483, -2.607348073571232, 3.0563754620559624),
            (19.549097387425583, 12.244367460469363, 0.8),
            (5.664280020587462, -1.4520102469532084, 5.530115811710694),
            (-3.0987736677523, -0.41426820089414934, 2.7894043084418967),
            (-7.271546855545921, 7.407667665483476, 2.1546288278638563),
            (2.6337852096200587, -16.364006888151255, 4.150737650686631),
            (-33.029423617493215, 15.326196716300089, 4.120993686035668),
            (-3.5817439411020002, 15.61255105748268, 2.5176156833383114),
            (31.181023534936582, -16.99000672897561, 3.187013128939303),
            (19.295935788332006, 10.214361424896895, 2.6420553382885146),
            (28.267826384274272, -9.996620908589026, 3.6649732987672836),
            (-8.732270571729165, 12.312386915630467, 3.662220619519833),
        ];
        // (id, x, y) —— 士兵半径恒为 0.5（PosInfo 里的 radius）
        const SOLDIERS: [(i64, f64, f64); 4] = [
            (233, 18.434132014691514, 0.8654732803815559),
            (231, -11.75347210076399, -5.862016522961236),
            (234, 17.627629369923454, -12.391816789113197),
            (230, -10.452779035793856, -2.651563472451839),
        ];

        // 掩膜只放障碍（docs/30 §2）
        let mut mask = crate::plane::empty_mask();
        for (x, y, r) in OBSTACLES {
            crate::plane::rasterize(&mut mask, x, y, r, 0x00);
        }

        // (函数原文, 起点 x, 起点 y, flip) —— 两发都来自 Bot 234
        let shots: [(&str, f64, f64, bool); 2] = [
            ("(x/3.02)", 17.627629369923454, -12.391816789113197, true),
            ("(-1.65/(-8.04/x))", 17.627629369923454, -12.391816789113197, true),
        ];
        // 排除射手本人（settle 的约定）
        let shooter = 234_i64;
        let targets: Vec<Target> = SOLDIERS
            .iter()
            .filter(|(id, _, _)| *id != shooter)
            .map(|(id, x, y)| Target { id: *id, x: *x, y: *y, radius: 0.5 })
            .collect();
        let triple: Vec<(f64, f64, f64)> = targets.iter().map(|t| (t.x, t.y, t.radius)).collect();

        for (text, x0, y0, flip) in shots {
            let path = calculate_path(text, x0, y0, if flip { -1.0 } else { 1.0 });
            assert!(path.ok, "{text}: 弹道算不出来（{}）", path.error);

            // 真值：完整结算（自己采样 + 判命中）
            let theirs = resolve(text, x0, y0, STEP_DEFAULT, flip, Mode::Normal, &mask, &targets);
            // 待验：只判命中（点序列来自 path_calculate）
            let ours = hit_test(&path.points, &OBSTACLES, &triple);

            let mut a: Vec<i64> = ours.hits.iter().map(|h| targets[h.soldier].id).collect();
            let mut b = theirs.killed.clone();
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "{text}: 致死集合与 graphwar_shot_calculator::settle::resolve 不一致");
            assert_eq!(ours.blocked, theirs.blocked, "{text}: blocked 不一致");
            assert_eq!(ours.stop, theirs.stop, "{text}: 停止点不一致");
        }
    }
}

// ---------------------------------------------------------------------------
// 8. `level_obstacles` 的单测（`docs/126`）
// ---------------------------------------------------------------------------
//
// 三条硬要求，每条都有对应的测试：
// * 21 关**全都**生成得出来，且模式/数量与 `docs/118` §7.1 一致；
// * 同 `(关卡, 种子)` 逐位可复现，换种子换布局；
// * 生成出来的障碍**真的能挡弹道**（不是只把数字交出去）—— 这一条把
//   `level_obstacles` 与 `hit_test` 串起来，避免"画了墙但子弹穿墙"。
#[cfg(test)]
mod obstacle_tests {
    use super::*;

    /// 表里的 `min_count`（测试内自抄一份，故意不引用 `levelgen` 的常量：
    /// 若两处不同步，这条测试会红 —— 这正是目的）。
    const MIN_COUNT: [i32; 21] = [0, 0, 0, 0, 15, 5, 10, 6, 10, 5, 10, 5, 0, 10, 5, 10, 0, 10, 10, 10, 15];
    /// 该关用的是散布式（关 5/6/7/8/9/10/11/12/14/15/16/18/19/20/21，1 基）。
    const IS_SCATTER: [bool; 21] = [
        false, false, false, false, true, true, true, true, true, true, true, true, false, true,
        true, true, false, true, true, true, true,
    ];
    /// 该关用的是中心单障碍（关 3/4/13，1 基）。
    const IS_CENTER: [bool; 21] = [
        false, false, true, true, false, false, false, false, false, false, false, false, true,
        false, false, false, false, false, false, false, false,
    ];

    /// 契约形状：`{ok, obstacles, error}`，`obstacles` 是步长 3 的扁平 f64 数组。
    #[test]
    fn result_shape_is_exactly_the_contract() {
        let out = level_obstacles(4, 1); // 关 5 Level 4，散布 min=15
        assert!(out.ok, "err={}", out.error);
        assert_eq!(out.error, "", "成功时 error 必须是空串");
        let pairs = obstacle_pairs(&out);
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, OBSTACLE_KEYS.to_vec(), "键名与顺序都必须固定");
        match &pairs[1].1 {
            ApiValue::F64Array(flat) => {
                assert_eq!(flat.len() % 3, 0, "必须是 [x, y, size] * n");
                assert_eq!(flat.len() / 3, out.obstacles.len(), "扁平长度与元组个数必须一致");
                // 逐位相等：写进 Dictionary 的不能是"再算一遍"的值
                for (i, &(x, y, r)) in out.obstacles.iter().enumerate() {
                    assert_eq!(flat[i * 3].to_bits(), x.to_bits());
                    assert_eq!(flat[i * 3 + 1].to_bits(), y.to_bits());
                    assert_eq!(flat[i * 3 + 2].to_bits(), r.to_bits());
                }
            }
            other => panic!("obstacles 类型不对：{other:?}"),
        }
    }

    /// 关卡号越界 → `ok:false` + `bad_level_index`（不 panic、不返回空表蒙混）。
    #[test]
    fn out_of_range_level_is_rejected() {
        for bad in [-1i64, 21, 22, 999, i64::MIN, i64::MAX] {
            let out = level_obstacles(bad, 1);
            assert!(!out.ok, "关卡 {bad} 不应当成功");
            assert_eq!(out.error, E_BAD_LEVEL_INDEX, "关卡 {bad} 的错误码");
            assert!(out.obstacles.is_empty(), "失败时不应带障碍");
        }
        // 边界内的两支必须成功（0 与 20 都是合法关卡号）
        assert!(level_obstacles(0, 1).ok);
        assert!(level_obstacles(20, 1).ok);
    }

    /// 21 关全表：模式、个数下界、坐标/半径的合法范围。
    #[test]
    fn all_21_levels_generate_and_match_the_table() {
        assert_eq!(MIN_COUNT.len(), LEVEL_COUNT);
        for i in 0..LEVEL_COUNT {
            let out = level_obstacles(i as i64, 0x0BAD_C0DE_1234_5678);
            assert!(out.ok, "level_{i} 失败：{}", out.error);
            let n = out.obstacles.len();
            if IS_SCATTER[i] {
                assert!(
                    n >= MIN_COUNT[i] as usize,
                    "level_{i} 只有 {n} 个，低于 min_count {}",
                    MIN_COUNT[i]
                );
            } else if IS_CENTER[i] {
                assert_eq!(n, 1, "level_{i} 是中心单障碍，整关只有 1 个");
            } else {
                assert_eq!(n, 0, "level_{i} 无障碍（docs/118 §7.1）");
            }
            for &(x, y, r) in &out.obstacles {
                assert!(x.is_finite() && y.is_finite() && r.is_finite(), "level_{i} 出现非有限值");
                // 原版公式自带的界（docs/55 §2）：x ∈ ±(2r+25)、y ∈ ±(2r+15)、r ≥ 0.8
                assert!(x.abs() <= 2.0 * r + 25.0, "level_{i} x 越界: {x} r={r}");
                assert!(y.abs() <= 2.0 * r + 15.0, "level_{i} y 越界: {y} r={r}");
                assert!(r >= 0.8, "level_{i} 半径 {r} 低于下限 0.8");
            }
            if IS_CENTER[i] {
                let (x, y, r) = out.obstacles[0];
                assert_eq!((x, y), (0.0, 0.0), "中心式位置恒为原点（docs/118 §7.2）");
                assert!((3.0..8.0).contains(&r), "中心式半径应在 [3,8) 内，实测 {r}");
            }
        }
    }

    /// 三关"无障碍"必须真的给空表 —— 而且**不是**错误（客户端按空表画空气）。
    #[test]
    fn obstacle_free_levels_return_an_empty_ok_list() {
        for i in [0usize, 1, 16] {
            let out = level_obstacles(i as i64, 42);
            assert!(out.ok, "level_{i} 应当成功（无碍 ≠ 失败）");
            assert!(out.obstacles.is_empty(), "level_{i}（docs/118 §7.1「无障碍」）不该有障碍");
        }
    }

    /// 同 `(关卡, 种子)` 逐位可复现；换种子换布局。
    #[test]
    fn same_seed_is_bit_identical_and_new_seed_changes_the_layout() {
        let a = level_obstacles(19, 2026);
        let b = level_obstacles(19, 2026);
        assert_eq!(a.obstacles.len(), b.obstacles.len());
        for (p, q) in a.obstacles.iter().zip(b.obstacles.iter()) {
            assert_eq!(p.0.to_bits(), q.0.to_bits(), "x 必须逐位相同");
            assert_eq!(p.1.to_bits(), q.1.to_bits(), "y 必须逐位相同");
            assert_eq!(p.2.to_bits(), q.2.to_bits(), "r 必须逐位相同");
        }
        // 换种子：至少有一个值不同（个数可能相同，所以比较内容）
        let c = level_obstacles(19, 2027);
        assert!(
            a.obstacles.len() != c.obstacles.len()
                || a.obstacles
                    .iter()
                    .zip(c.obstacles.iter())
                    .any(|(p, q)| p.0.to_bits() != q.0.to_bits()),
            "换了种子却得到同一份布局"
        );
    }

    /// 退化种子（0 / 负值 / i64 上下界）不 panic、不产生非有限值。
    #[test]
    fn degenerate_seeds_are_safe() {
        for seed in [0i64, -1, i64::MIN, i64::MAX, 1] {
            for level in [0i64, 2, 4, 20] {
                let out = level_obstacles(level, seed);
                assert!(out.ok, "level {level} seed {seed} 失败：{}", out.error);
                for &(x, y, r) in &out.obstacles {
                    assert!(x.is_finite() && y.is_finite() && r.is_finite());
                }
            }
        }
    }

    /// 反"偷偷重写生成器"：API 的产出必须与 `levelgen` 既有实现逐位相同。
    #[test]
    fn agrees_with_levelgen_entry_point_bit_for_bit() {
        for i in 0..LEVEL_COUNT {
            let seed = 0x5EED_0000_0000_0000u64 + i as u64;
            let want = crate::graphwar_level_generator::obstacles_from_seed(i, seed).unwrap();
            let got = level_obstacles(i as i64, seed as i64);
            assert_eq!(got.obstacles.len(), want.len(), "level_{i} 个数不同");
            for (k, (g, w)) in got.obstacles.iter().zip(want.iter()).enumerate() {
                assert_eq!(g.0.to_bits(), w.x.to_bits(), "level_{i} #{k} x");
                assert_eq!(g.1.to_bits(), w.y.to_bits(), "level_{i} #{k} y");
                assert_eq!(g.2.to_bits(), w.r.to_bits(), "level_{i} #{k} r");
            }
        }
    }

    /// **墙真的挡子弹**：把生成的障碍喂给 `hit_test`，一条穿过障碍的水平弹道
    /// 必须停在障碍圆上；把同一批障碍拿掉就飞到底。
    ///
    /// 这条是"客户端画了墙但子弹穿墙"这一整类 bug 的守门测试 ——
    /// 它同时钉住两件事：`obstacles` 的 `size` 是**半径**（不是直径），
    /// 以及 `level_obstacles` 与 `hit_test` 用的是**同一套坐标系**（棋盘坐标）。
    #[test]
    fn generated_obstacles_actually_block_a_shot() {
        // 关 5（Level 4，散布 min=15）障碍多，随便找个种子都有横跨 y=0 的
        let out = level_obstacles(4, 7);
        assert!(out.ok && out.obstacles.len() >= 10, "样本太少：{}", out.obstacles.len());

        // 水平弹道：从 x = −24 走到 x = +24（y 恒 0）
        let mut pts = Vec::new();
        let mut x = -24.0;
        while x <= 24.0 {
            pts.push((x, 0.0));
            x += 0.01;
        }

        // 理论上第一个撞点：所有"与 y=0 相交、且入口在 x=−24 右边"的障碍里，入口最小的那个。
        // 入口 x = cx − sqrt(r² − cy²)（圆与直线的交点）—— 纯几何，不依赖掩膜实现。
        let mut best: Option<(f64, f64, f64, f64)> = None; // (entry, cx, cy, r)
        for &(cx, cy, r) in &out.obstacles {
            if cy.abs() >= r {
                continue; // 圆与 y=0 不相交
            }
            let entry = cx - (r * r - cy * cy).sqrt();
            if entry <= -24.0 {
                continue; // 弹道起点已经在圆里（或圆整个在起点左边）
            }
            if best.map_or(true, |(e, _, _, _)| entry < e) {
                best = Some((entry, cx, cy, r));
            }
        }
        let (entry, cx, cy, r) = best.expect("这一关的样本里应当有一个能挡住 y=0 直线的障碍");

        let blocked = hit_test(&pts, &out.obstacles, &[]);
        assert!(blocked.ok, "err={}", blocked.error);
        assert!(blocked.blocked, "弹道穿过了障碍却没被挡住");
        assert!(blocked.first_hit >= 0, "blocked=true 却给不出撞点序号");

        let (hx, hy) = pts[blocked.first_hit as usize];
        assert_eq!(hy, 0.0);
        // 掩膜是"20 px/单位"的像素圆 + 一圈边缘容差 ⇒ 撞点可能比几何入口早 ~1 像素（0.05 单位）
        assert!(
            (hx - entry).abs() <= 0.25,
            "撞点 x={hx:.3} 与几何入口 {entry:.3} 差太多（打到的不是算出来那面墙）"
        );
        // 撞点到该圆圆心的距离必须 ≈ 半径（这就是"size 是半径"的直接证据）
        let d = ((hx - cx).powi(2) + (hy - cy).powi(2)).sqrt();
        assert!(
            (d - r).abs() <= 0.25,
            "撞点到圆心 ({cx:.3},{cy:.3}) 距离 {d:.3}，半径 {r:.3} —— size 不是半径？"
        );
        // 停止点与撞点一致（`stop` 是棋盘坐标的撞击点）
        assert_eq!(blocked.stop.0, hx, "stop 应当就是撞点");
        assert_eq!(blocked.stop.1, hy, "stop 应当就是撞点");

        // 反证：同一批点、没有障碍 ⇒ 一路飞到底，不 blocked
        let clear = hit_test(&pts, &[], &[]);
        assert!(clear.ok);
        assert!(!clear.blocked, "没有障碍时不该 blocked");
        assert_eq!(clear.first_hit, -1);
        assert_eq!(clear.stop, *pts.last().unwrap(), "没有障碍时停在最后一个点");
    }
}
