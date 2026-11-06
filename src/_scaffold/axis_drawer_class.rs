//! `AxisDrawer` 类注册：把 `src/axis_drawer.rs` 的复原逻辑接成**真 Godot 节点**（spike）。
//!
//! 这个文件只回答一个问题：**我们手写的 `gdext*.rs` 能不能注册一个继承 `Control`、
//! 带属性、能被 `.tscn` 实例化的节点类 —— 不只是单例？**
//!
//! # 为什么挑 `AxisDrawer`
//!
//! 它是原版 12 个 GDExtension 类里最简单的一个：`docs/02` §18.3 A 表里 3 个函数、
//! 1 个真方法（`draw`）；`docs/18` §6.5 实测它**没有任何暴露给 GDScript 的方法**。
//! 类名与父类是**观测**：`"AxisDrawer"` 字面量在 `.rodata` 槽 `0x1c41b7`（长 10），
//! 三个虚方法 shim（`0x4e01f0` / `0x4e0070` / `0x4e0130`）都 `lea` 到它；
//! A 表的 `gdext trait impl` 列写的是 `IControl` + `GodotClass` + `ImplementsGodotVirtual`
//! ⇒ 父类是 `Control`。
//!
//! # 注册了什么
//!
//! | 项 | 名字 | 签名 | 证据档位 |
//! |---|---|---|---|
//! | 类 | `AxisDrawer` | 父类 `Control` | 观测（`.rodata` 类名 + `IControl` impl） |
//! | 属性 | `axis_mode` | `int` | 推断（`self+0x18` 是 `u8` 判别值，见 `docs/16` §21） |
//! | 方法 | `get_axis_mode` / `set_axis_mode` | `() -> int` / `(int) -> void` | 属性的读写器 |
//! | 方法 | `axis_line_count` | `() -> int` | 跑 [`crate::axis_drawer::axis_primitives`] 数 `Line` |
//! | 方法 | `axis_line_count_formula` | `() -> int` | 跑闭式 [`crate::axis_drawer::line_count`] |
//!
//! 两个"数线"的方法**故意重复**：一个走图元生成、一个走闭式公式。GDScript 里断言
//! `axis_line_count() == axis_line_count_formula()`（并且 0/2/20/102 对得上）⇒ 复原出的
//! `axis_drawer` 逻辑确实被引擎**调用到了**，而不是只注册了个空壳。
//!
//! `axis_mode` 的**默认值未知**（原版构造时写什么值没有证据，只有 `draw` 里的读取）。
//! 这里用 `0`（`NoAxis`，一条线都不画）——不是从二进制推出来的，是我们自己的选择。
//!
//! # 还没做的（**未知 / 未做**，别当成做完了）
//!
//! * **没接管 `draw`**：虚方法表（`get_virtual_func` / `get_virtual_call_data_func` /
//!   `call_virtual_with_data_func`）一个都没填，所以这个类现在**不画任何东西**。
//!   要真画，得填虚方法表 + 用 `object_method_bind_ptrcall` 调 `CanvasItem::draw_line`
//!   / `draw_rect` / `draw_set_transform_full`（`docs/16` §21 有原版的三次调用）。
//! * **没发 `NOTIFICATION_POSTINITIALIZE`**：`create_instance_func` 收到
//!   `notify_postinitialize = 1` 时，官方 gdext 会补发这条通知；我们没做。它只对
//!   "挂了脚本"或"自己有 `notification_func`"的类有可观测差异，对空壳 `Control` 无。
//! * **不做资源/属性回读**（`property_can_revert` / `property_get_revert` 全空）。
//!
//! # 怎么验证
//!
//! ```text
//! cargo build --offline
//! cp target/debug/libgw2_rebuild.so client/libgw2_rebuild.so
//! bash client/run.sh            # 或者按 docs/16 §35 的命令跑 headless 检查脚本
//! ```
//!
//! 判据只有三条：`ClassDB.class_exists("AxisDrawer")` 为真、`AxisDrawer.new()` 出得来、
//! 属性/方法能读能写。**"能注册" ≠ "会画"**，后者是下一步的虚方法表。

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::_scaffold::gdext_class::{ClassMethodInfo, CreationInfo4};
use crate::axis_drawer::{self, AxisMode, Primitive, Vec2};

// ---------------------------------------------------------------------------
// 1. GDExtension 接口（Godot 4 的 `get_proc_address` 口径，逐条照头文件）
// ---------------------------------------------------------------------------

type GetProc = unsafe extern "C" fn(name: *const u8) -> *mut c_void;

/// `string_name_new_with_latin1_chars(dest, text, is_static)`。
type FnSnNew = unsafe extern "C" fn(dest: *mut c_void, text: *const u8, is_static: u8);
/// `classdb_register_extension_class4(library, class_name, parent_name, info)`。
type FnClassReg = unsafe extern "C" fn(
    library: *mut c_void,
    class_name: *const c_void,
    parent_name: *const c_void,
    info: *const CreationInfo4,
);
/// `classdb_register_extension_class_method(library, class_name, info)`。
type FnMethodReg =
    unsafe extern "C" fn(library: *mut c_void, class_name: *const c_void, info: *const ClassMethodInfo);
/// `classdb_register_extension_class_property(library, class_name, info, setter, getter)`。
type FnPropertyReg = unsafe extern "C" fn(
    library: *mut c_void,
    class_name: *const c_void,
    info: *const PropertyInfo,
    setter: *const c_void,
    getter: *const c_void,
);
/// `classdb_construct_object(class_name) -> Object*`：造**父类**的引擎对象。
type FnConstruct = unsafe extern "C" fn(class_name: *const c_void) -> *mut c_void;
/// `object_set_instance(object, class_name, instance)`：把我们的 `Instance` 挂上去。
type FnSetInstance =
    unsafe extern "C" fn(object: *mut c_void, class_name: *const c_void, instance: *mut c_void);
/// `variant_get_type(v) -> Variant::Type`。
type FnGetType = unsafe extern "C" fn(v: *const c_void) -> i32;
/// `get_variant_{from,to}_type_constructor(ty) -> fn*`。
type FnGetCtor = unsafe extern "C" fn(ty: i32) -> *mut c_void;
/// `get_variant_from_type_constructor` 拿到的构造器：`fn(dest_variant, src_raw)`。
type FnCtor = unsafe extern "C" fn(dest: *mut c_void, value: *mut c_void);
/// `GDExtensionClassMethodCall`。
type CallFn = unsafe extern "C" fn(
    method_userdata: *mut c_void,
    instance: *mut c_void,
    args: *const *const c_void,
    arg_count: i64,
    ret: *mut c_void,
    error: *mut c_void,
);

/// `GDExtensionPropertyInfo`。`class_name` / `hint_string` **不能是 NULL**（实测会段错误，
/// `docs/116` §3.3）；这里一律指向"默认构造的空 StringName / String"。
#[repr(C)]
struct PropertyInfo {
    ty: i32,
    name: *const c_void,
    class_name: *const c_void,
    hint: u32,
    hint_string: *const c_void,
    usage: u32,
}

// `GDEXTENSION_VARIANT_TYPE_*`
const V_INT: i32 = 2;
const V_FLOAT: i32 = 3;
/// `PROPERTY_USAGE_DEFAULT`（STORAGE|EDITOR）。
const USAGE_DEFAULT: u32 = 6;
/// `METHOD_FLAG_NORMAL`。
const METHOD_FLAG_NORMAL: u32 = 1;

/// 一块对齐的裸内存，放 Godot 的内置类型（`StringName` / `String` 各 8 字节，
/// 这里给 64 字节的余量，和 `api.rs` 的 `Slot` 同一个思路）。
#[repr(align(16))]
struct Slot([u8; 64]);

impl Slot {
    fn new() -> Box<Slot> {
        Box::new(Slot([0u8; 64]))
    }
    fn p(&self) -> *mut c_void {
        self.0.as_ptr() as *mut c_void
    }
}

/// 注册时向引擎要的接口表。存成 `usize` 是为了让 `static OnceLock` 能 `Send + Sync`。
struct Ifaces {
    class_reg: usize,
    method_reg: usize,
    property_reg: usize,
    construct: usize,
    set_instance: usize,
    sn_new: usize,
    from_ctor: usize,
    to_ctor: usize,
    get_type: usize,
}

static IFACES: OnceLock<Ifaces> = OnceLock::new();

fn ifaces() -> Option<&'static Ifaces> {
    IFACES.get()
}

/// 类名 / 父类名的 `StringName` 只建一次（`create_instance` 每次实例化都要用，
/// 每次重建会泄漏 `StringName` 的内部引用）。
struct ClassNames {
    parent: Box<Slot>,
    this: Box<Slot>,
}

static CLASS_NAMES: OnceLock<ClassNames> = OnceLock::new();

// ---------------------------------------------------------------------------
// 2. 实例数据
// ---------------------------------------------------------------------------

/// 一个 `AxisDrawer` 实例里我们自己的那部分状态。
///
/// 原版把判别值放在 `self + 0x18`（`u8`），`draw` 里只读不写（`movzx eax, byte [self+0x18]`，
/// `0x4e06da`）。所以这里也用 `u8` 存，越界值交给 [`AxisMode::from_raw`] 按原版分支处理。
struct Instance {
    /// `axis_mode` 的**原始判别值**（0..=255）。
    axis_mode: u8,
}

/// `axis_mode` 的默认值。**推断**：原版构造时写什么没有证据，取 `NoAxis`（0）。
const DEFAULT_AXIS_MODE: u8 = AxisMode::NoAxis as u8;

/// 棋盘尺寸（`BOARD_RECT.size`）：只影响 `axis_primitives` 里的像素换算，
/// 不影响线的**条数**，所以拿来数线是安全的。
const BOARD_SIZE: Vec2 = Vec2::new(50.0, 30.0);

// 观测计数器（注册/实例化的自检用；`pub` 出去，单元测试与以后的 selftest 可以读）。
static INSTANCES: AtomicUsize = AtomicUsize::new(0);
static FREED: AtomicUsize = AtomicUsize::new(0);
static SET_MODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static GET_MODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static LINE_COUNT_CALLS: AtomicUsize = AtomicUsize::new(0);

/// `create_instance` 成功造出多少个引擎对象。
pub fn instance_count() -> usize {
    INSTANCES.load(Ordering::Relaxed)
}

/// `free_instance` 回收过多少个 `Instance`。
pub fn freed_count() -> usize {
    FREED.load(Ordering::Relaxed)
}

/// `set_axis_mode` / `get_axis_mode` 各被调用多少次。
pub fn mode_calls() -> (usize, usize) {
    (
        SET_MODE_CALLS.load(Ordering::Relaxed),
        GET_MODE_CALLS.load(Ordering::Relaxed),
    )
}

/// `axis_line_count` 被调用多少次。
pub fn line_count_calls() -> usize {
    LINE_COUNT_CALLS.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// 3. 纯逻辑（可单测，不碰引擎）
// ---------------------------------------------------------------------------

/// 把任意 `i64` 收成判别值 `u8`：负数当 0，大于 255 当 255。
///
/// 原版判别值是 `u8` 且只比较 `==3` / `>=2` / `!=0`（见 [`AxisMode`] 的文档），
/// 所以收窄后语义不变；这里**不做**"只接受 0..=3"的校验，因为原版对越界值是**有行为**的。
fn clamp_mode(value: i64) -> u8 {
    value.clamp(0, u8::MAX as i64) as u8
}

/// 某个判别值下 `draw_line` 的**实际条数**：跑 [`axis_drawer::axis_primitives`] 数
/// `Primitive::Line`。这是"复原逻辑真的被执行了"的那个观测点。
fn line_primitive_count(raw_mode: u8) -> usize {
    let mode = AxisMode::from_raw(raw_mode);
    axis_drawer::axis_primitives(mode, BOARD_SIZE)
        .iter()
        .filter(|p| matches!(p, Primitive::Line { .. }))
        .count()
}

// ---------------------------------------------------------------------------
// 4. 读参数 / 写返回值（只用 `from/to_type_constructor`，不猜 `Variant` 布局）
// ---------------------------------------------------------------------------

unsafe fn ctor(get: usize, ty: i32) -> Option<FnCtor> {
    if get == 0 {
        return None;
    }
    let f: FnGetCtor = std::mem::transmute(get);
    let p = f(ty);
    if p.is_null() {
        None
    } else {
        Some(std::mem::transmute(p))
    }
}

/// 读一个 `int` 参数（也接受整值 `float`；带小数的一律拒绝，别静默截断）。
unsafe fn read_i64(ifc: &Ifaces, v: *const c_void) -> Option<i64> {
    if v.is_null() || ifc.get_type == 0 {
        return None;
    }
    let get_type: FnGetType = std::mem::transmute(ifc.get_type);
    match get_type(v) {
        V_INT => {
            let to = ctor(ifc.to_ctor, V_INT)?;
            let mut out = 0i64;
            to(&mut out as *mut i64 as *mut c_void, v as *mut c_void);
            Some(out)
        }
        V_FLOAT => {
            let to = ctor(ifc.to_ctor, V_FLOAT)?;
            let mut out = 0f64;
            to(&mut out as *mut f64 as *mut c_void, v as *mut c_void);
            if out.is_finite() && out.fract() == 0.0 && out.abs() <= i64::MAX as f64 {
                Some(out as i64)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 往引擎给的 `Variant` 槽里原地构造一个 `int`（引擎负责 destroy）。
unsafe fn write_int(ifc: &Ifaces, dest: *mut c_void, value: i64) -> bool {
    if dest.is_null() {
        return false;
    }
    let Some(from) = ctor(ifc.from_ctor, V_INT) else {
        return false;
    };
    let mut raw = value;
    from(dest, &mut raw as *mut i64 as *mut c_void);
    true
}

unsafe fn instance_mut<'a>(p: *mut c_void) -> Option<&'a mut Instance> {
    if p.is_null() {
        None
    } else {
        Some(&mut *(p as *mut Instance))
    }
}

// ---------------------------------------------------------------------------
// 5. 四个回调：create / free / 方法
// ---------------------------------------------------------------------------

unsafe extern "C" fn create_instance(_class_userdata: *mut c_void, notify_postinitialize: u8) -> *mut c_void {
    let Some(ifc) = ifaces() else {
        return std::ptr::null_mut();
    };
    let Some(names) = CLASS_NAMES.get() else {
        return std::ptr::null_mut();
    };
    let construct: FnConstruct = std::mem::transmute(ifc.construct);
    let set_instance: FnSetInstance = std::mem::transmute(ifc.set_instance);

    // 父类是 `Control` ⇒ 让引擎造一个**真的 Control**，再把我们的实例数据挂上去。
    let object = construct(names.parent.p() as *const c_void);
    if object.is_null() {
        eprintln!("[gw2-rebuild] AxisDrawer: classdb_construct_object(\"Control\") 返回空");
        return std::ptr::null_mut();
    }
    let instance = Box::into_raw(Box::new(Instance {
        axis_mode: DEFAULT_AXIS_MODE,
    }));
    set_instance(object, names.this.p() as *const c_void, instance as *mut c_void);

    let n = INSTANCES.fetch_add(1, Ordering::Relaxed) + 1;
    if n == 1 {
        eprintln!(
            "[gw2-rebuild] AxisDrawer 实例化成功 (object={object:p}, notify_postinitialize={notify_postinitialize})"
        );
    }
    object
}

unsafe extern "C" fn free_instance(_class_userdata: *mut c_void, instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance as *mut Instance));
        let n = FREED.fetch_add(1, Ordering::Relaxed) + 1;
        if n == 1 {
            // 只打第一条：证明 `free_instance_func` 真的被引擎调到了（不是只进不出）。
            eprintln!("[gw2-rebuild] AxisDrawer 实例被引擎回收（free_instance）");
        }
    }
}

unsafe extern "C" fn set_axis_mode_call(
    _ud: *mut c_void,
    instance: *mut c_void,
    args: *const *const c_void,
    argc: i64,
    _ret: *mut c_void,
    _error: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ifc) = ifaces() else { return };
        let Some(inst) = instance_mut(instance) else { return };
        if argc < 1 || args.is_null() {
            return;
        }
        let Some(value) = read_i64(ifc, *args) else { return };
        inst.axis_mode = clamp_mode(value);
        SET_MODE_CALLS.fetch_add(1, Ordering::Relaxed);
    }));
}

unsafe extern "C" fn get_axis_mode_call(
    _ud: *mut c_void,
    instance: *mut c_void,
    _args: *const *const c_void,
    _argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ifc) = ifaces() else { return };
        let Some(inst) = instance_mut(instance) else { return };
        if write_int(ifc, ret, inst.axis_mode as i64) {
            GET_MODE_CALLS.fetch_add(1, Ordering::Relaxed);
        }
    }));
}

/// `axis_line_count() -> int`：跑一遍 [`axis_drawer::axis_primitives`] 数 `Line`。
unsafe extern "C" fn axis_line_count_call(
    _ud: *mut c_void,
    instance: *mut c_void,
    _args: *const *const c_void,
    _argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ifc) = ifaces() else { return };
        let Some(inst) = instance_mut(instance) else { return };
        let n = line_primitive_count(inst.axis_mode);
        if write_int(ifc, ret, n as i64) {
            LINE_COUNT_CALLS.fetch_add(1, Ordering::Relaxed);
        }
    }));
}

/// `axis_line_count_formula() -> int`：跑闭式 [`axis_drawer::line_count`]（与上一个对拍）。
unsafe extern "C" fn axis_line_count_formula_call(
    _ud: *mut c_void,
    instance: *mut c_void,
    _args: *const *const c_void,
    _argc: i64,
    ret: *mut c_void,
    _error: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(ifc) = ifaces() else { return };
        let Some(inst) = instance_mut(instance) else { return };
        let n = axis_drawer::line_count(AxisMode::from_raw(inst.axis_mode));
        if write_int(ifc, ret, n as i64) {
            LINE_COUNT_CALLS.fetch_add(1, Ordering::Relaxed);
        }
    }));
}

// ---------------------------------------------------------------------------
// 6. 注册
// ---------------------------------------------------------------------------

unsafe fn sym(get: GetProc, name: &str) -> usize {
    let mut c = Vec::with_capacity(name.len() + 1);
    c.extend_from_slice(name.as_bytes());
    c.push(0);
    get(c.as_ptr()) as usize
}

/// 造一个 `StringName`（拉丁 1 字符即可，这里全是 ASCII）。
unsafe fn sn(f_sn: FnSnNew, text: &str) -> Box<Slot> {
    let mut c = Vec::with_capacity(text.len() + 1);
    c.extend_from_slice(text.as_bytes());
    c.push(0);
    let s = Slot::new();
    f_sn(s.p(), c.as_ptr(), 0);
    s
}

/// 给类挂一个方法。参数名/返回名都要是**合法** `StringName`（不能 NULL）。
unsafe fn add_method(
    ifc: &Ifaces,
    library: *mut c_void,
    class_sn: *const c_void,
    name: &str,
    call: CallFn,
    args: &[(&str, i32)],
    ret_ty: Option<i32>,
) -> bool {
    let f_sn: FnSnNew = std::mem::transmute(ifc.sn_new);
    let f_reg: FnMethodReg = std::mem::transmute(ifc.method_reg);

    let method_sn = sn(f_sn, name);
    let empty_sn = Slot::new();
    let empty_str = Slot::new();
    let arg_sn: Vec<Box<Slot>> = args.iter().map(|(n, _)| sn(f_sn, n)).collect();
    let ret_name = sn(f_sn, "r_ret");

    let ret_info = PropertyInfo {
        ty: ret_ty.unwrap_or(0),
        name: ret_name.p() as *const c_void,
        class_name: empty_sn.p() as *const c_void,
        hint: 0,
        hint_string: empty_str.p() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    let arg_info: Vec<PropertyInfo> = args
        .iter()
        .enumerate()
        .map(|(i, (_, ty))| PropertyInfo {
            ty: *ty,
            name: arg_sn[i].p() as *const c_void,
            class_name: empty_sn.p() as *const c_void,
            hint: 0,
            hint_string: empty_str.p() as *const c_void,
            usage: USAGE_DEFAULT,
        })
        .collect();
    let arg_meta: Vec<i32> = vec![0; args.len()];

    let info = ClassMethodInfo {
        name: method_sn.p() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: u8::from(ret_ty.is_some()),
        return_value_info: if ret_ty.is_some() {
            &ret_info as *const PropertyInfo as *const c_void
        } else {
            std::ptr::null()
        },
        return_value_metadata: 0,
        argument_count: args.len() as u32,
        arguments_info: if args.is_empty() {
            std::ptr::null()
        } else {
            arg_info.as_ptr() as *const c_void
        },
        arguments_metadata: if args.is_empty() {
            std::ptr::null()
        } else {
            arg_meta.as_ptr() as *const c_void
        },
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };
    f_reg(library, class_sn, &info);
    true
}

/// 挂一个属性（setter/getter 名字**必须**是已注册的方法，所以顺序是方法在前）。
unsafe fn add_property(
    ifc: &Ifaces,
    library: *mut c_void,
    class_sn: *const c_void,
    name: &str,
    setter: &str,
    getter: &str,
    ty: i32,
) -> bool {
    let f_sn: FnSnNew = std::mem::transmute(ifc.sn_new);
    let f_reg: FnPropertyReg = std::mem::transmute(ifc.property_reg);

    let name_sn = sn(f_sn, name);
    let setter_sn = sn(f_sn, setter);
    let getter_sn = sn(f_sn, getter);
    let empty_sn = Slot::new();
    let empty_str = Slot::new();

    let info = PropertyInfo {
        ty,
        name: name_sn.p() as *const c_void,
        class_name: empty_sn.p() as *const c_void,
        hint: 0,
        hint_string: empty_str.p() as *const c_void,
        usage: USAGE_DEFAULT,
    };
    f_reg(
        library,
        class_sn,
        &info,
        setter_sn.p() as *const c_void,
        getter_sn.p() as *const c_void,
    );
    true
}

/// 在 `SCENE` 级别注册 `AxisDrawer`（父类 `Control`）。成功返回 `true`。
///
/// 顺序有讲究：**先类 → 再方法 → 最后属性**。属性注册时引擎会去查 setter/getter 的
/// `MethodBind`，方法没先注册的话属性会变成"有名字、点了没反应"。
///
/// # Safety
/// `get` 必须是 Godot 的 `get_proc_address`；`library` 是引擎传入的库句柄。
/// 必须在 `SCENE` 初始化级别调用（父类 `Control` 那时才存在）。
pub unsafe fn register_axis_drawer_class(get: GetProc, library: *mut c_void) -> bool {
    let ifc = Ifaces {
        class_reg: sym(get, "classdb_register_extension_class4"),
        method_reg: sym(get, "classdb_register_extension_class_method"),
        property_reg: sym(get, "classdb_register_extension_class_property"),
        construct: sym(get, "classdb_construct_object"),
        set_instance: sym(get, "object_set_instance"),
        sn_new: sym(get, "string_name_new_with_latin1_chars"),
        from_ctor: sym(get, "get_variant_from_type_constructor"),
        to_ctor: sym(get, "get_variant_to_type_constructor"),
        get_type: sym(get, "variant_get_type"),
    };
    eprintln!(
        "[gw2-rebuild] axis api resolve: class4={} method={} property={} construct={} \
         set_instance={} sn_new={} from_ctor={} to_ctor={} get_type={}",
        ifc.class_reg != 0,
        ifc.method_reg != 0,
        ifc.property_reg != 0,
        ifc.construct != 0,
        ifc.set_instance != 0,
        ifc.sn_new != 0,
        ifc.from_ctor != 0,
        ifc.to_ctor != 0,
        ifc.get_type != 0,
    );
    if ifc.class_reg == 0
        || ifc.method_reg == 0
        || ifc.property_reg == 0
        || ifc.construct == 0
        || ifc.set_instance == 0
        || ifc.sn_new == 0
        || ifc.from_ctor == 0
        || ifc.to_ctor == 0
        || ifc.get_type == 0
    {
        eprintln!("[gw2-rebuild] AxisDrawer 未注册：缺少接口函数");
        return false;
    }
    let _ = IFACES.set(ifc);
    let Some(ifc) = ifaces() else { return false };

    let f_sn: FnSnNew = std::mem::transmute(ifc.sn_new);
    let f_class_reg: FnClassReg = std::mem::transmute(ifc.class_reg);

    // 类名 / 父类名：一次成型，之后 `create_instance` 复用。
    let class_sn = sn(f_sn, "AxisDrawer");
    let parent_sn = sn(f_sn, "Control");
    let names = ClassNames {
        parent: sn(f_sn, "Control"),
        this: sn(f_sn, "AxisDrawer"),
    };
    let _ = CLASS_NAMES.set(names);

    let mut info = CreationInfo4::empty();
    info.is_exposed = 1; // 让 GDScript / `.tscn` 能按名找到它
    info.create_instance_func = Some(create_instance);
    info.free_instance_func = Some(free_instance);

    f_class_reg(
        library,
        class_sn.p() as *const c_void,
        parent_sn.p() as *const c_void,
        &info,
    );
    eprintln!("[gw2-rebuild] registered class AxisDrawer (parent Control)");

    // 方法：读写器 + 两个"数线"的观测点。
    add_method(
        ifc,
        library,
        class_sn.p() as *const c_void,
        "set_axis_mode",
        set_axis_mode_call,
        &[("axis_mode", V_INT)],
        None,
    );
    add_method(
        ifc,
        library,
        class_sn.p() as *const c_void,
        "get_axis_mode",
        get_axis_mode_call,
        &[],
        Some(V_INT),
    );
    add_method(
        ifc,
        library,
        class_sn.p() as *const c_void,
        "axis_line_count",
        axis_line_count_call,
        &[],
        Some(V_INT),
    );
    add_method(
        ifc,
        library,
        class_sn.p() as *const c_void,
        "axis_line_count_formula",
        axis_line_count_formula_call,
        &[],
        Some(V_INT),
    );
    // 属性必须在方法之后：引擎注册属性时会去找 setter/getter 的方法绑定。
    add_property(
        ifc,
        library,
        class_sn.p() as *const c_void,
        "axis_mode",
        "set_axis_mode",
        "get_axis_mode",
        V_INT,
    );
    eprintln!(
        "[gw2-rebuild] registered AxisDrawer members: axis_mode (property), \
         set_axis_mode/get_axis_mode/axis_line_count/axis_line_count_formula (methods)"
    );
    true
}

// ---------------------------------------------------------------------------
// 7. 测试：ABI 布局 + 纯逻辑（不需要引擎）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `GDExtensionClassCreationInfo4` 的字段序/大小。**这条测试是 ABI 证据**：
    /// 4 个 `GDExtensionBool`（各 1 字节）+ 4 字节对齐 + 19 个指针 = 160 字节。
    /// 原版 `libgraphwarrust.so` 用 gdext 注册同样的结构（`classdb_register_extension_class4`
    /// 的名字在 `.rodata` 里），字段序对不上就会读到垃圾函数指针。
    #[test]
    fn creation_info4_layout_matches_header() {
        assert_eq!(std::mem::size_of::<CreationInfo4>(), 160);
        assert_eq!(std::mem::align_of::<CreationInfo4>(), 8);
        // 报头四个 bool 之后第一个指针在偏移 8。
        assert_eq!(std::mem::offset_of!(CreationInfo4, is_virtual), 0);
        assert_eq!(std::mem::offset_of!(CreationInfo4, is_runtime), 3);
        assert_eq!(std::mem::offset_of!(CreationInfo4, icon_path), 8);
        assert_eq!(std::mem::offset_of!(CreationInfo4, set_func), 16);
        assert_eq!(std::mem::offset_of!(CreationInfo4, get_property_list_func), 32);
        assert_eq!(std::mem::offset_of!(CreationInfo4, free_property_list_func), 40);
        assert_eq!(std::mem::offset_of!(CreationInfo4, notification_func), 72);
        // 4.2 起 create/free 在 reference/unreference 之后，4.3 再插入 recreate。
        assert_eq!(std::mem::offset_of!(CreationInfo4, create_instance_func), 104);
        assert_eq!(std::mem::offset_of!(CreationInfo4, free_instance_func), 112);
        assert_eq!(std::mem::offset_of!(CreationInfo4, recreate_instance_func), 120);
        assert_eq!(std::mem::offset_of!(CreationInfo4, class_userdata), 152);
    }

    /// `GDExtensionPropertyInfo` 的布局：`type`(4) + 4 字节填充 + `name`/`class_name`(8+8)
    /// + `hint`(4) + **4 字节填充** + `hint_string`(8) + `usage`(4) + 4 字节尾填充 = **48**。
    ///
    /// 那两段填充是 `hint_string` 的 8 字节对齐带来的 —— 漏掉它就会把 `usage` 写在
    /// `hint_string` 里（属性变成"看得见、读写错位"）。这条测试是 ABI 证据，不是自证。
    #[test]
    fn property_info_layout() {
        assert_eq!(std::mem::size_of::<PropertyInfo>(), 48);
        assert_eq!(std::mem::align_of::<PropertyInfo>(), 8);
        assert_eq!(std::mem::offset_of!(PropertyInfo, ty), 0);
        assert_eq!(std::mem::offset_of!(PropertyInfo, name), 8);
        assert_eq!(std::mem::offset_of!(PropertyInfo, class_name), 16);
        assert_eq!(std::mem::offset_of!(PropertyInfo, hint), 24);
        assert_eq!(std::mem::offset_of!(PropertyInfo, hint_string), 32);
        assert_eq!(std::mem::offset_of!(PropertyInfo, usage), 40);
    }

    /// 类名与 `src/axis_drawer.rs` 里从 `.rodata` 读出来的字面量必须是同一个。
    #[test]
    fn class_name_matches_axis_drawer_constant() {
        assert_eq!(axis_drawer::CLASS_NAME, "AxisDrawer");
    }

    /// 判别值收窄：负数→0，>255→255，正常值原样。
    #[test]
    fn clamp_mode_is_u8_narrowing() {
        assert_eq!(clamp_mode(-7), 0);
        assert_eq!(clamp_mode(0), 0);
        assert_eq!(clamp_mode(3), 3);
        assert_eq!(clamp_mode(3.0 as i64), 3);
        assert_eq!(clamp_mode(255), 255);
        assert_eq!(clamp_mode(256), 255);
        assert_eq!(clamp_mode(i64::MAX), 255);
    }

    /// 两个"数线"路径必须一致，且四个已知变体是 0 / 2 / 20 / 102（`docs/16` §21）。
    #[test]
    fn line_counts_match_formula() {
        let expected = [
            (AxisMode::NoAxis as u8, 0usize),
            (AxisMode::OnlyMain as u8, 2),
            (AxisMode::EveryFive as u8, 20),
            (AxisMode::EveryUnit as u8, 102),
        ];
        for (raw, want) in expected {
            let mode = AxisMode::from_raw(raw);
            assert_eq!(axis_drawer::line_count(mode), want, "闭式公式 mode={raw}");
            assert_eq!(line_primitive_count(raw), want, "图元枚举 mode={raw}");
            assert_eq!(line_primitive_count(raw), axis_drawer::line_count(mode));
        }
    }

    /// 越界判别值按原版分支走：`==3` 不成立、`>=2` 成立、`!=0` 成立 ⇒ 粗网格 + 主轴 = 20 条。
    #[test]
    fn out_of_range_mode_keeps_original_branching() {
        for raw in [4u8, 5, 200, 255] {
            let mode = AxisMode::from_raw(raw);
            assert!(!mode.draws_minor_grid(), "raw={raw} 不该画细网格");
            assert!(mode.draws_major_grid(), "raw={raw} 该画粗网格");
            assert!(mode.draws_main_axis(), "raw={raw} 该画主轴");
            assert_eq!(line_primitive_count(raw), 20);
        }
    }

    /// 默认 `axis_mode` 是 `NoAxis`（**我们自己的选择**，不是二进制证据）。
    #[test]
    fn default_axis_mode_is_no_axis() {
        assert_eq!(DEFAULT_AXIS_MODE, 0);
        assert_eq!(line_primitive_count(DEFAULT_AXIS_MODE), 0);
    }

    /// 没拿到的接口表时，回调必须**安全地什么都不做**（不能解引用空指针）。
    #[test]
    fn callbacks_are_noops_without_interfaces() {
        // 这个测试进程里 `IFACES` 可能已被别的测试填过；只断言"不 panic"这一条。
        unsafe {
            let mut value = 0i64;
            set_axis_mode_call(std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null(), 0, std::ptr::null_mut(), std::ptr::null_mut());
            get_axis_mode_call(std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null(), 0, std::ptr::null_mut(), std::ptr::null_mut());
            axis_line_count_call(std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null(), 0, &mut value as *mut i64 as *mut c_void, std::ptr::null_mut());
            axis_line_count_formula_call(std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null(), 0, std::ptr::null_mut(), std::ptr::null_mut());
        }
        assert_eq!(instance_count(), 0, "单元测试里不该真的造出实例");
    }
}
