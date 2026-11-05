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
//! 点序列**全部**来自 [`crate::path::calculate`]（`@0x3c2530` 的复原）：
//! 步长 [`crate::STEP_DEFAULT`]（0.01）、容差 [`crate::LENGTH_TOL`]、
//! 点数预算 `(200/step) as i32`、平面边界 `|x|<=25 / |y|<=15`、修正常数
//! [`crate::shot::correction`]。本文件**不重新采样**，只做类型转换与错误翻译。
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

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::function::parse;
use crate::path::calculate;
use crate::shot::{correction, shifted_tokens};
use crate::path::Mode;
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
/// [`crate::shot::shot_points`] 完全相同的三步，所以结果逐位一致
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
        // 但 `shot::shot_points` 与 C ABI `gw2_shot_points` 都按“空 = 没有点”处理，
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
// 2. FFI 层的数据整形（不依赖 Godot，可单测：决定 Dictionary 里放什么）
// ---------------------------------------------------------------------------

/// `path_calculate` 返回的 Dictionary 里的一个值。
#[derive(Debug, Clone, PartialEq)]
pub enum ApiValue {
    Bool(bool),
    F64(f64),
    Str(String),
    Points(Vec<(f64, f64)>),
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
}

static API: OnceLock<Api> = OnceLock::new();

/// 注册过一次 method 之后的计数（自检用）。
static CALLS: AtomicUsize = AtomicUsize::new(0);
static OK_CALLS: AtomicUsize = AtomicUsize::new(0);
static ERR_CALLS: AtomicUsize = AtomicUsize::new(0);

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
}

/// 注册时创建一次、之后一直复用的 `StringName`（方法名）。
struct Names {
    set: Box<Slot>,
    append: Box<Slot>,
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

/// 把结果写成 Dictionary，放进 `ret`（Godot 给的**空 Variant 槽**）。
unsafe fn write_result(api: &Api, ret: *mut c_void, out: &PathOutcome) {
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
    for (key, value) in result_pairs(out) {
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
            ApiValue::Str(text) => put_string_variant(api, val_slot.p(), text),
            ApiValue::Points(pts) => put_points_variant(api, val_slot.p(), pts),
        };
        if built {
            let _ = api.call(ret, names.set.p(), &[key_slot.p(), val_slot.p()]);
        }
        // 两个 Variant 都是 placement-new 出来的，必须按 Variant 析构
        api.destroy_variant(val_slot.p());
        api.destroy_variant(key_slot.p());
    }
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

/// 给已注册的 `GW2Rebuild` 类挂上 `path_calculate(expr, x0, y0, direction)`。
///
/// # Safety
/// `get` 必须是 Godot 的 `get_proc_address`，且必须在 `register_class` **之后**调用。
pub unsafe fn register_path_method(get: GetProc, library: *mut c_void) -> bool {
    let table = Api {
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
    };
    eprintln!(
        "[gw2-rebuild] api resolve: from_ctor={} to_ctor={} ptr_ctor={} variant_call={} \
         get_type={} variant_destroy={} raw_dtor={} sn_new={} str_new={} str_to_utf8={} \
         register_method={}",
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
    );
    // 缺任何一个都不能注册：宁可让客户端继续用预览，也不要一半可用
    if table.sn_new == 0
        || table.str_new == 0
        || table.str_to_utf8 == 0
        || table.from_ctor == 0
        || table.to_ctor == 0
        || table.ptr_ctor == 0
        || table.variant_call == 0
        || table.get_type == 0
        || table.register_method == 0
    {
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
    let Some(api) = api() else { return false };
    // 方法名 StringName 复用（省掉每次调用的表查找）
    let (Some(set_sn), Some(append_sn)) = (api.sn("set"), api.sn("append")) else {
        eprintln!("[gw2-rebuild] path_calculate: cannot build method StringNames");
        return false;
    };
    let _ = NAMES.set(Names { set: set_sn, append: append_sn });

    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!("[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)");
    true
}

// ---------------------------------------------------------------------------
// 7. 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shot::shot_points;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    /// 若 `calculate_path` 与已验证的 `shot::shot_points` 不一致，说明这里偷偷
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
}
