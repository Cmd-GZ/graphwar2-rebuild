//! GDExtension 入口：让本 crate 编译出的 `.so` 能被 Godot 当作扩展加载。
//!
//! 证据：原 `libgraphwarrust.so` 的**唯一**动态导出就是 `gdext_rust_init`
//! （`nm -D --defined-only libgraphwarrust.so` 只打出 `gdext_rust_init` 和
//! `__cxa_thread_atexit_impl`）。所以重建物的入口符号必须与之同名。
//!
//! ABI 取自 Godot 4 的 `gdextension_interface.h`：
//!
//! ```c
//! typedef void (*GDExtensionInitializationFunction)(void *userdata,
//!                                                   GDExtensionInitializationLevel level);
//! typedef struct {
//!     GDExtensionInitializationLevel minimum_initialization_level;
//!     void *userdata;
//!     GDExtensionInitializationFunction initialize;
//!     GDExtensionInitializationFunction deinitialize;
//! } GDExtensionInitialization;
//!
//! GDExtensionBool entry(GDExtensionInterfaceGetProcAddress get_proc_address,
//!                       GDExtensionClassLibraryPtr library,
//!                       GDExtensionInitialization *r_initialization);
//! ```
//!
//! `GDExtensionBool` 是 `uint8_t`；`GDExtensionInitializationLevel` 是 `int32`。
//!
//! ## 当前能力
//!
//! 本入口**只完成注册骨架**：填写初始化结构、在 `SCENE` 级别被调用、
//! 并记录 `get_proc_address` 以便后续按名取接口。它**还没有注册任何类**
//! （`classdb_register_extension_class2` 等尚未接入），所以加载后游戏里看不到新节点。
//! 这一点是有意标注的：能加载 ≠ 功能完整。

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

/// `GDExtensionInterfaceGetProcAddress`：按名字取接口函数指针。
pub type GetProcAddress = unsafe extern "C" fn(name: *const u8) -> *mut c_void;

/// `GDExtensionInitializationFunction`。
pub type InitFn = unsafe extern "C" fn(userdata: *mut c_void, level: i32);

/// Godot 4 的初始化级别。
pub const LEVEL_CORE: i32 = 0;
pub const LEVEL_SERVERS: i32 = 1;
pub const LEVEL_SCENE: i32 = 2;
pub const LEVEL_EDITOR: i32 = 3;

/// `GDExtensionInitialization`。字段顺序与 C 结构体一致。
#[repr(C)]
pub struct Initialization {
    pub minimum_initialization_level: i32,
    pub userdata: *mut c_void,
    pub initialize: Option<InitFn>,
    pub deinitialize: Option<InitFn>,
}

// 观测用的计数器：验证入口确实被调用、以及在哪个级别被调用。
static GET_PROC_CALLS: AtomicUsize = AtomicUsize::new(0);
static INIT_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEINIT_CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST_LEVEL: AtomicUsize = AtomicUsize::new(usize::MAX);
static PROC_ADDR: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static LIBRARY: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// 入口实际拿到的 `get_proc_address`（供自检使用）。
pub fn proc_address() -> *mut c_void {
    PROC_ADDR.load(Ordering::Relaxed)
}

/// 入口被调用的次数（`gdext_rust_init` 自身）。
pub fn init_entry_calls() -> usize {
    GET_PROC_CALLS.load(Ordering::Relaxed)
}

/// `initialize` 回调被调用的次数与最后一次的级别。
pub fn initialize_calls() -> (usize, i32) {
    (INIT_CALLS.load(Ordering::Relaxed), LAST_LEVEL.load(Ordering::Relaxed) as i32)
}

/// `deinitialize` 回调被调用的次数。
pub fn deinitialize_calls() -> usize {
    DEINIT_CALLS.load(Ordering::Relaxed)
}

/// 初始化时要向 Godot 请求的接口函数名（按注册一个类的最小集合）。
pub const WANTED: &[&str] = &[
    "classdb_register_extension_class2",
    "classdb_register_extension_class_method",
    "classdb_register_extension_class_integer_constant",
    "object_set_instance",
    "object_get_instance_binding",
    "mem_alloc",
    "mem_free",
    "print_error",
    "string_name_new_with_latin1_chars",
];

static REQUESTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
static RESOLVED: AtomicUsize = AtomicUsize::new(0);

/// 按名取接口函数指针，并记录请求过的名字。
///
/// # Safety
/// 只有在 `gdext_rust_init` 记录了 `get_proc_address` 之后调用才安全。
unsafe fn request(name: &str) -> *mut c_void {
    if let Ok(mut v) = REQUESTED.lock() {
        v.push(name.to_string());
    }
    let f = PROC_ADDR.load(Ordering::Relaxed);
    if f.is_null() {
        return std::ptr::null_mut();
    }
    let getter: GetProcAddress = std::mem::transmute(f);
    let mut c = Vec::with_capacity(name.len() + 1);
    c.extend_from_slice(name.as_bytes());
    c.push(0);
    let p = getter(c.as_ptr());
    if !p.is_null() {
        RESOLVED.fetch_add(1, Ordering::Relaxed);
    }
    p
}

unsafe extern "C" fn on_initialize(_userdata: *mut c_void, level: i32) {
    INIT_CALLS.fetch_add(1, Ordering::Relaxed);
    LAST_LEVEL.store(level as usize, Ordering::Relaxed);

    // 真引擎里通过 stderr 观察初始化（headless 时也会出现在控制台）
    eprintln!("[gw2-rebuild] initialize level={level}");
    // 只在 SCENE 级别做类注册（与 Godot 的约定一致）。
    if level == LEVEL_SCENE {
        for name in WANTED {
            let _ = request(name);
        }
        // 真正注册一个类，供 GDScript 通过 ClassDB 访问
        let f = PROC_ADDR.load(Ordering::Relaxed);
        let lib = LIBRARY.load(Ordering::Relaxed);
        if !f.is_null() {
            let get: GetProcAddress = std::mem::transmute(f);
            unsafe {
                if crate::gdext_class::register_class(get, lib) {
                    crate::gdext_class::register_ping_method(get, lib);
                    // 真正的算法出口：GDScript 的 `GW2Rebuild.path_calculate(...)`
                    // （契约见 `client/src/core/core_bridge.gd` 文件头、`docs/116`）
                    let _ = crate::api::register_path_method(get, lib);
                    let _ = crate::api::register_hit_test_method(get, lib);
                }
            }
        }
    }
}

unsafe extern "C" fn on_deinitialize(_userdata: *mut c_void, level: i32) {
    DEINIT_CALLS.fetch_add(1, Ordering::Relaxed);
    eprintln!("[gw2-rebuild] deinitialize level={level}");
}

/// Godot 的扩展入口。符号名必须与 godot-rust 生成的一致。
///
/// # Safety
/// 调用方（Godot 或自检 harness）必须传入有效的 `r_initialization` 指针。
#[no_mangle]
pub unsafe extern "C" fn gdext_rust_init(
    get_proc_address: Option<GetProcAddress>,
    library: *mut c_void,
    r_initialization: *mut Initialization,
) -> u8 {
    if r_initialization.is_null() {
        return 0;
    }
    GET_PROC_CALLS.fetch_add(1, Ordering::Relaxed);
    if let Some(f) = get_proc_address {
        PROC_ADDR.store(f as *mut c_void, Ordering::Relaxed);
    }
    LIBRARY.store(library, Ordering::Relaxed);

    // 只在 CORE 级别以上完成注册；这里目前什么都不注册。
    r_initialization.write(Initialization {
        minimum_initialization_level: LEVEL_CORE,
        userdata: library,
        initialize: Some(on_initialize),
        deinitialize: Some(on_deinitialize),
    });

    1 // GDExtensionBool: true
}

/// 自检用导出：入口被调用次数。名称带前缀，避免与 Godot 符号冲突。
#[no_mangle]
pub extern "C" fn gw2_rebuild_entry_calls() -> usize {
    GET_PROC_CALLS.load(Ordering::Relaxed)
}

/// 自检用导出：`initialize` 回调次数。
#[no_mangle]
pub extern "C" fn gw2_rebuild_initialize_calls() -> usize {
    INIT_CALLS.load(Ordering::Relaxed)
}

/// 自检用导出：向 Godot 请求过的接口函数名，`\n` 分隔、以 NUL 结尾。
///
/// 指针指向一块 `static` 的 C 字符串，生命周期到进程结束。
#[no_mangle]
pub extern "C" fn gw2_rebuild_requested_names() -> *const std::os::raw::c_char {
    use std::sync::OnceLock;
    static BUF: OnceLock<std::ffi::CString> = OnceLock::new();
    let joined = match REQUESTED.lock() {
        Ok(v) => v.join("\n"),
        Err(_) => String::new(),
    };
    // 只构建一次；重复调用返回首次快照（够用于自检）。
    let c = BUF.get_or_init(|| std::ffi::CString::new(joined).unwrap_or_default());
    c.as_ptr()
}

/// 自检用导出：成功解析到的接口函数个数。
#[no_mangle]
pub extern "C" fn gw2_rebuild_resolved_count() -> usize {
    RESOLVED.load(Ordering::Relaxed)
}

/// 自检用导出：`deinitialize` 回调次数。
#[no_mangle]
pub extern "C" fn gw2_rebuild_deinitialize_calls() -> usize {
    DEINIT_CALLS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn stub_get_proc(_name: *const u8) -> *mut c_void {
        std::ptr::null_mut()
    }

    #[test]
    fn entry_fills_initialization() {
        let mut init = Initialization {
            minimum_initialization_level: -1,
            userdata: std::ptr::null_mut(),
            initialize: None,
            deinitialize: None,
        };
        let rc = unsafe {
            gdext_rust_init(Some(stub_get_proc), std::ptr::null_mut(), &mut init)
        };
        assert_eq!(rc, 1, "入口应返回 GDExtensionBool=1");
        assert_eq!(init.minimum_initialization_level, LEVEL_CORE);
        assert!(init.initialize.is_some(), "initialize 回调必须被填写");
        assert!(init.deinitialize.is_some(), "deinitialize 回调必须被填写");
    }

    #[test]
    fn null_out_pointer_is_rejected() {
        let rc = unsafe { gdext_rust_init(None, std::ptr::null_mut(), std::ptr::null_mut()) };
        assert_eq!(rc, 0, "空指针必须返回 0（不初始化）");
    }

    #[test]
    fn callbacks_run() {
        let mut init = Initialization {
            minimum_initialization_level: -1,
            userdata: std::ptr::null_mut(),
            initialize: None,
            deinitialize: None,
        };
        unsafe { gdext_rust_init(None, std::ptr::null_mut(), &mut init) };
        let before = initialize_calls().0;
        unsafe {
            (init.initialize.unwrap())(std::ptr::null_mut(), LEVEL_SCENE);
            (init.deinitialize.unwrap())(std::ptr::null_mut(), LEVEL_SCENE);
        }
        assert_eq!(initialize_calls().0, before + 1);
        assert_eq!(initialize_calls().1, LEVEL_SCENE);
        assert!(deinitialize_calls() >= 1);
    }

    #[test]
    fn initialization_layout_matches_c() {
        // C 侧的布局：i32 + pad + void* + fn* + fn*  => 4+4+8+8+8 = 32 (x86-64)
        assert_eq!(std::mem::size_of::<Initialization>(), 32);
        assert_eq!(std::mem::align_of::<Initialization>(), 8);
    }
}
