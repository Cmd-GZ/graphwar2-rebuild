//! 扩展类注册（Godot 4.7 的 `classdb_register_extension_class4`）。
//!
//! ABI 来源：`godot-cpp/4.4/gdextension/gdextension_interface.h`（唯一能取到的分支），
//! 结构与签名在该版已稳定；`CreationInfo4` 的字段序见文件末尾的注释。
//!
//! 目标：把复原出来的算法暴露成 Godot 里的一个类 `GW2Rebuild`，
//! 让 GDScript 能 `ClassDB.class_exists("GW2Rebuild")` 并实例化。

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

type GetProc = unsafe extern "C" fn(name: *const u8) -> *mut c_void;

type FnClass4 = unsafe extern "C" fn(
    library: *mut c_void,
    class_name: *const c_void,
    parent_name: *const c_void,
    info: *const CreationInfo4,
);
type FnStringName = unsafe extern "C" fn(dest: *mut c_void, text: *const u8, is_static: u8);
type FnConstruct = unsafe extern "C" fn(class_name: *const c_void) -> *mut c_void;
type FnSetInstance =
    unsafe extern "C" fn(obj: *mut c_void, class_name: *const c_void, instance: *mut c_void);
type FnMemAlloc = unsafe extern "C" fn(bytes: usize) -> *mut c_void;

/// `GDExtensionClassCreateInstance2`（4.2 起带 `p_notify_postinitialize`）。
pub type CreateInstance2 = Option<unsafe extern "C" fn(*mut c_void, u8) -> *mut c_void>;
/// `GDExtensionClassFreeInstance`。
pub type FreeInstance = Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>;

/// `GDExtensionClassCreationInfo4`（字段顺序严格照头文件）。
#[repr(C)]
pub struct CreationInfo4 {
    pub is_virtual: u8,
    pub is_abstract: u8,
    pub is_exposed: u8,
    pub is_runtime: u8,
    pub icon_path: *const c_void,
    pub set_func: *const c_void,
    pub get_func: *const c_void,
    pub get_property_list_func: *const c_void,
    pub free_property_list_func: *const c_void,
    pub property_can_revert_func: *const c_void,
    pub property_get_revert_func: *const c_void,
    pub validate_property_func: *const c_void,
    pub notification_func: *const c_void,
    pub to_string_func: *const c_void,
    pub reference_func: *const c_void,
    pub unreference_func: *const c_void,
    pub create_instance_func: CreateInstance2,
    pub free_instance_func: FreeInstance,
    pub recreate_instance_func: *const c_void,
    pub get_virtual_func: *const c_void,
    pub get_virtual_call_data_func: *const c_void,
    pub call_virtual_with_data_func: *const c_void,
    pub class_userdata: *mut c_void,
}

impl CreationInfo4 {
    /// 全零的 `CreationInfo4`：只填 `is_exposed` / `create_instance_func` /
    /// `free_instance_func` 就能注册一个空壳类。别的类（`axis_drawer_class.rs`）也用这一份。
    pub fn empty() -> Self {
        CreationInfo4 {
            is_virtual: 0,
            is_abstract: 0,
            is_exposed: 0,
            is_runtime: 0,
            icon_path: std::ptr::null(),
            set_func: std::ptr::null(),
            get_func: std::ptr::null(),
            get_property_list_func: std::ptr::null(),
            free_property_list_func: std::ptr::null(),
            property_can_revert_func: std::ptr::null(),
            property_get_revert_func: std::ptr::null(),
            validate_property_func: std::ptr::null(),
            notification_func: std::ptr::null(),
            to_string_func: std::ptr::null(),
            reference_func: std::ptr::null(),
            unreference_func: std::ptr::null(),
            create_instance_func: None,
            free_instance_func: None,
            recreate_instance_func: std::ptr::null(),
            get_virtual_func: std::ptr::null(),
            get_virtual_call_data_func: std::ptr::null(),
            call_virtual_with_data_func: std::ptr::null(),
            class_userdata: std::ptr::null_mut(),
        }
    }
}

/// 已经注册过的实例数量（自检用）。
static INSTANCES: AtomicUsize = AtomicUsize::new(0);
/// 类是否注册成功（1/0）。
static REGISTERED: AtomicUsize = AtomicUsize::new(0);

pub fn instances() -> usize {
    INSTANCES.load(Ordering::Relaxed)
}
pub fn registered() -> bool {
    REGISTERED.load(Ordering::Relaxed) == 1
}

/// 类实例的占位数据：真正的重建逻辑将来挂在这里。
struct Instance {
    _marker: u64,
}

unsafe fn sym(get: GetProc, name: &str) -> *mut c_void {
    let mut c = Vec::with_capacity(name.len() + 1);
    c.extend_from_slice(name.as_bytes());
    c.push(0);
    get(c.as_ptr())
}

unsafe extern "C" fn create_instance(_ud: *mut c_void, _notify: u8) -> *mut c_void {
    // 父类名与类名要在调用时重建 StringName；这里用静态函数指针缓存
    let construct = CONSTRUCT.load(Ordering::Relaxed);
    let set_inst = SET_INSTANCE.load(Ordering::Relaxed);
    if construct.is_null() || set_inst.is_null() {
        return std::ptr::null_mut();
    }
    let mut parent_sn = [0u8; 8];
    let mut self_sn = [0u8; 8];
    let f_sn = STRING_NAME.load(Ordering::Relaxed);
    if f_sn.is_null() {
        return std::ptr::null_mut();
    }
    let f_sn: FnStringName = std::mem::transmute(f_sn);
    f_sn(parent_sn.as_mut_ptr() as *mut c_void, b"RefCounted\0".as_ptr(), 0);
    f_sn(self_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);

    let construct: FnConstruct = std::mem::transmute(construct);
    let obj = construct(parent_sn.as_ptr() as *const c_void);
    if obj.is_null() {
        return std::ptr::null_mut();
    }
    let inst = Box::into_raw(Box::new(Instance { _marker: 0x4757 })) as *mut c_void;
    let set_inst: FnSetInstance = std::mem::transmute(set_inst);
    set_inst(obj, self_sn.as_ptr() as *const c_void, inst);
    INSTANCES.fetch_add(1, Ordering::Relaxed);
    obj
}

unsafe extern "C" fn free_instance(_ud: *mut c_void, instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance as *mut Instance));
    }
}

static CONSTRUCT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SET_INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STRING_NAME: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// 在 SCENE 级别注册 `GW2Rebuild`。成功返回 true。
///
/// # Safety
/// `get` 必须是 Godot 提供的 `get_proc_address`，`library` 是引擎传入的库句柄。
pub unsafe fn register_class(get: GetProc, library: *mut c_void) -> bool {
    let f_class4 = sym(get, "classdb_register_extension_class4");
    let f_sn = sym(get, "string_name_new_with_latin1_chars");
    let f_construct = sym(get, "classdb_construct_object");
    let f_set = sym(get, "object_set_instance");
    eprintln!(
        "[gw2-rebuild] resolve: class4={} string_name={} construct={} set_instance={}",
        !f_class4.is_null(),
        !f_sn.is_null(),
        !f_construct.is_null(),
        !f_set.is_null()
    );
    if f_class4.is_null() || f_sn.is_null() || f_construct.is_null() || f_set.is_null() {
        return false;
    }
    CONSTRUCT.store(f_construct, Ordering::Relaxed);
    SET_INSTANCE.store(f_set, Ordering::Relaxed);
    STRING_NAME.store(f_sn, Ordering::Relaxed);

    let f_class4: FnClass4 = std::mem::transmute(f_class4);
    let f_sn: FnStringName = std::mem::transmute(f_sn);

    let mut name_sn = [0u8; 8];
    let mut parent_sn = [0u8; 8];
    f_sn(name_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);
    f_sn(parent_sn.as_mut_ptr() as *mut c_void, b"RefCounted\0".as_ptr(), 0);

    let mut info = CreationInfo4::empty();
    info.is_exposed = 1; // 让它在 ClassDB 里可见
    info.create_instance_func = Some(create_instance);
    info.free_instance_func = Some(free_instance);

    f_class4(
        library,
        name_sn.as_ptr() as *const c_void,
        parent_sn.as_ptr() as *const c_void,
        &info,
    );
    REGISTERED.store(1, Ordering::Relaxed);
    eprintln!("[gw2-rebuild] registered class GW2Rebuild (parent RefCounted)");
    true
}

// ---------------------------------------------------------------------------
// 方法注册：classdb_register_extension_class_method
// ---------------------------------------------------------------------------

/// `GDExtensionClassMethodInfo`（字段序照头文件）。
#[repr(C)]
pub struct ClassMethodInfo {
    pub name: *const c_void,          // StringName
    pub method_userdata: *mut c_void,
    pub call_func: CallFunc,
    pub ptrcall_func: *const c_void,
    pub method_flags: u32,
    pub has_return_value: u8,
    pub return_value_info: *const c_void,
    pub return_value_metadata: i32,
    pub argument_count: u32,
    pub arguments_info: *const c_void,
    pub arguments_metadata: *const c_void,
    pub default_argument_count: u32,
    pub default_arguments: *const c_void,
}

pub type CallFunc = Option<
    unsafe extern "C" fn(
        method_userdata: *mut c_void,
        instance: *mut c_void,
        args: *const *const c_void,
        arg_count: i64,
        ret: *mut c_void,
        error: *mut c_void,
    ),
>;

/// `METHOD_FLAG_NORMAL`
const METHOD_FLAG_NORMAL: u32 = 1;

static PINGS: AtomicUsize = AtomicUsize::new(0);

/// `ping()` 被调用次数（自检用）。
pub fn pings() -> usize {
    PINGS.load(Ordering::Relaxed)
}

unsafe extern "C" fn ping_call(
    _ud: *mut c_void,
    _instance: *mut c_void,
    _args: *const *const c_void,
    _argc: i64,
    _ret: *mut c_void,
    _err: *mut c_void,
) {
    PINGS.fetch_add(1, Ordering::Relaxed);
    eprintln!("[gw2-rebuild] ping() called from GDScript");
}

/// 给已注册的类挂一个 `ping()` 方法。
///
/// # Safety
/// `get` 必须是 Godot 的 `get_proc_address`，且必须在类注册之后调用。
pub unsafe fn register_ping_method(get: GetProc, library: *mut c_void) -> bool {
    let f_reg = sym(get, "classdb_register_extension_class_method");
    let f_sn = sym(get, "string_name_new_with_latin1_chars");
    if f_reg.is_null() || f_sn.is_null() {
        eprintln!("[gw2-rebuild] method registration unavailable");
        return false;
    }
    type FnRegister = unsafe extern "C" fn(*mut c_void, *const c_void, *const ClassMethodInfo);
    let f_reg: FnRegister = std::mem::transmute(f_reg);
    let f_sn: FnStringName = std::mem::transmute(f_sn);

    let mut cls_sn = [0u8; 8];
    let mut meth_sn = [0u8; 8];
    f_sn(cls_sn.as_mut_ptr() as *mut c_void, b"GW2Rebuild\0".as_ptr(), 0);
    f_sn(meth_sn.as_mut_ptr() as *mut c_void, b"ping\0".as_ptr(), 0);

    let info = ClassMethodInfo {
        name: meth_sn.as_ptr() as *const c_void,
        method_userdata: std::ptr::null_mut(),
        call_func: Some(ping_call),
        ptrcall_func: std::ptr::null(),
        method_flags: METHOD_FLAG_NORMAL,
        has_return_value: 0,
        return_value_info: std::ptr::null(),
        return_value_metadata: 0,
        argument_count: 0,
        arguments_info: std::ptr::null(),
        arguments_metadata: std::ptr::null(),
        default_argument_count: 0,
        default_arguments: std::ptr::null(),
    };
    f_reg(library, cls_sn.as_ptr() as *const c_void, &info);
    eprintln!("[gw2-rebuild] registered method ping()");
    true
}
