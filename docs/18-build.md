# 18 · 构建出真实二进制（GDExtension 骨架）

状态：**可构建、可 dlopen、入口契约已验证**；尚未注册任何类。

## 1. 为什么是这个入口

原 `libgraphwarrust.so` 的动态导出**只有两个**：

```
$ nm -D --defined-only libgraphwarrust.so
00000000004dc6d0 T __cxa_thread_atexit_impl
00000000004dc6e0 T gdext_rust_init
```

`__cxa_thread_atexit_impl` 是 Rust 工具链的运行时符号，真正的扩展入口是
**`gdext_rust_init`**（godot-rust 生成的 ABI 入口）。所以重建物必须导出同名符号。

## 2. 改动

`Cargo.toml`：

```toml
[lib]
name = "gw2_rebuild"
crate-type = ["cdylib", "rlib"]   # cdylib 出 .so；rlib 留给 cargo test
```

`src/gdext.rs` 按 Godot 4 `gdextension_interface.h` 手写入口（不依赖 godot-rust）：

```rust
#[repr(C)]
pub struct Initialization {
    pub minimum_initialization_level: i32,
    pub userdata: *mut c_void,
    pub initialize: Option<InitFn>,     // void (*)(void*, int32_t)
    pub deinitialize: Option<InitFn>,
}

#[no_mangle]
pub unsafe extern "C" fn gdext_rust_init(
    get_proc_address: Option<GetProcAddress>,
    library: *mut c_void,
    r_initialization: *mut Initialization,
) -> u8                                // GDExtensionBool = uint8_t
```

返回 `1`（true）并把初始化结构填好；`minimum_initialization_level = CORE(0)`。

## 3. 构建（实测输出）

```bash
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=/home/user/MyProject/gw2/.cargo-home/target
cargo build --release --offline
```

```
Finished `release` profile [optimized] target(s) in 0.91s
-rwxr-xr-x 404K  .cargo-home/target/release/libgw2_rebuild.so
```

## 4. 验证（三条独立证据）

**① 文件形态**

```
$ file libgw2_rebuild.so
ELF 64-bit LSB shared object, x86-64, version 1 (SYSV), dynamically linked, not stripped
```

**② 导出符号**

```
$ nm -D --defined-only libgw2_rebuild.so
0000000000012d20 T gdext_rust_init
```

与原 `.so` 的入口**同名**。

**③ 入口契约（`tools/gdext_probe.c`）**

不需要 Godot：`dlopen` → `dlsym("gdext_rust_init")` → 用一个桩 `get_proc_address`
调用入口 → 检查返回值与结构体 → 再调用 `initialize`/`deinitialize`，
最后用 Rust 侧导出的计数器确认回调**真的执行了**。

```
ok   dlsym(gdext_rust_init) = 0x7f589c456e30
ok   entry returned 1 (expect 1)
ok   minimum_initialization_level = 0 (expect 0)
ok   initialize=0x7f589c456df0 deinitialize=0x7f589c456e10
ok   counters entry=1 init=1 deinit=1
RESULT: PASS
```

计数器是关键：它把"函数被调用"从"看起来对"变成"Rust 内部状态确实变了"。

## 5. 复现

```bash
cargo build --release --offline
gcc -O1 -o /tmp/gdext_probe tools/gdext_probe.c -ldl
/tmp/gdext_probe $CARGO_TARGET_DIR/release/libgw2_rebuild.so
```

## 6. 明确没有做到的

- **没有注册任何类**：`classdb_register_extension_class2`、方法绑定、属性、信号都没接。
  所以这个 `.so` 能被加载，但游戏里看不到新节点——**能加载 ≠ 功能完整**。
- **没有在真 Godot 里跑过**：本机的 `Graphwar.x86_64` 是**导出模板**（不是编辑器），
  无法用它打包 `.pck`；要真正加载需要 Godot 4.7 编辑器，当前环境没有。
  所以这里的"可加载"是**按 ABI 约定 + dlopen 实测**成立的，不是"进游戏验证过"。
- **不是原版行为**：入口只填结构体，不解析任何 VFS/类库。

## 7. 下一步

1. 用 `get_proc_address` 取 `classdb_register_extension_class2`，注册一个最小的类
   （例如把已经复原的 `parse`/`calculate` 暴露成方法），让扩展有实际功能；
2. 若能拿到 Godot 4.7 编辑器，再验证"真加载 + 真调用"。

## 8. 一个环境坑（本轮踩到）

```bash
export CARGO_HOME=/path/.cargo-home CARGO_TARGET_DIR=$CARGO_HOME/target   # ✗
```
同一条 `export` 里 `$CARGO_HOME` **在赋值生效前**就展开了，`CARGO_TARGET_DIR`
变成 `/target`，于是报 `Read-only file system ... /targetWN5pcA`。
两个变量要分行导出，或都用绝对路径。
