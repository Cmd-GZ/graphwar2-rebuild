# 18 · 构建与真引擎验证（GDExtension 骨架 → 真 Godot 里注册类）

状态：**可构建、可 dlopen、入口契约已验证；已在真 Godot 4.7.2 里注册类并跑通方法调用；
`.pck` 能从包里启动并加载还原的资源**（§6）。

> 2026-11 文档归并：原 `archive/19-godot-verify.md` 全部并入本文件 §6（那条线本来就是
> "构建产物 → 真引擎验证"的同一条），原文件移入 `docs/archive/`。

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

`src/_scaffold/gdext.rs` 按 Godot 4 `gdextension_interface.h` 手写入口（不依赖 godot-rust）：

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

## 4.5 接口请求（本轮新增）

`initialize(SCENE)` 时会按名向 Godot 请求注册一个类所需的最小接口集合，
并记录请求过的名字：

```
classdb_register_extension_class2
classdb_register_extension_class_method
classdb_register_extension_class_integer_constant
object_set_instance
object_get_instance_binding
mem_alloc
mem_free
print_error
string_name_new_with_latin1_chars
```

探测器把这些名字逐条打出来；因为桩 `get_proc_address` 一律返回 `NULL`，
`resolved=0` 是**正确**结果——说明缺接口时我们能安全降级，不会崩。

这一步的价值：**"知道该请求什么"是可验证的**，而"注册成功"必须等真 Godot。

## 4.6 功能证据：产物真的复现了算法（本轮新增）

符号存在只能证明"能加载"，不能证明"算得对"。所以另开一条 C ABI：

```
gw2_parse_len(text)                          -> token 个数
gw2_shot_points(text, x0, y0, step, flip, ode, out, cap) -> 点数（写 xy 交替）
gw2_correction(text, x0, y0, flip)           -> 修正常数
```

`tools/capi_check.py` 用 `ctypes` 直接 `CDLL` 载入构建产物，跑 5 组用例并与
`graphwar_curve.py`（上游已验证与真二进制逐位一致）逐点比较：

```
ok   'sin(x)'                2471 点  修正 +2.958924
ok   'x^2/10 - 3'            3062 点  修正 +2.100000
ok   'sqrt(abs(x))'          2735 点  修正 -5.000000
ok   '2*x+1'                  309 点  修正 -6.000000
ok   'log(abs(x)+1)'         1916 点  修正 +4.886057
RESULT: PASS
```

合计 10493 个点逐位相同，含修正项。这条比符号检查强得多：
**它证明从复原源码构建出的二进制，行为与游戏一致。**

## 4.7 类注册：先调研，当时的结论是"不硬写"（**后来改成硬写并成功了**）

取到权威头文件（`godot-cpp/4.4/gdextension/gdextension_interface.h`）后确认：

- `GDExtensionClassCreationInfo2` 有 **21 个函数指针 + `class_userdata`**；
- 该接口函数 `classdb_register_extension_class2` **在 Godot 4.3 起已废弃**，
  应改用 `classdb_register_extension_class4` + `GDExtensionClassCreationInfo4`；
- `GDExtensionClassCreateInstance2` 多了一个 `notify_postinitialize` 参数。

**当时的结论：不硬写。** 理由：目标引擎是 Godot 4.7，而本机只有导出模板、没有编辑器，
无法验证任何一个布局假设；把 21 个函数指针按记忆排错，只会在真 Godot 里崩。

> ⚠️ **这一条已被 §6 取代**：后来拿到了 Godot 4.7.2（`tools/Godot_v4.7.2-stable_linux.x86_64`，
> 74.3 MB，gitignore），按 `godot-cpp/4.4` 头文件逐字段照抄 **`classdb_register_extension_class4`
> + `GDExtensionClassCreationInfo4`**，注册成功且 GDScript 能实例化。
> 教训：**"没有可验证手段"≠"不该做"，先找引擎再判断**。

## 5. 复现

```bash
cargo build --release --offline
gcc -O1 -o ./gdext_probe tools/gdext_probe.c -ldl     # 别写 /tmp：本机每条命令是独立 tmpfs
./gdext_probe $CARGO_TARGET_DIR/release/libgw2_rebuild.so
```

## 6. 真 Godot 4.7.2 里验证（原 `19`）

### 6.1 拿到了引擎

```
Godot_v4.7.2-stable_linux.x86_64 --version
→ 4.7.2.stable.official.ed1daf0bf
```

与 `Graphwar.pck` 记录的 `godot 4.7.0` 同 minor。本机 `~` 只读，所以要给引擎三个目录：

```bash
XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache \
XDG_CONFIG_HOME=$PWD/.godot-config tools/Godot_v4.7.2-stable_linux.x86_64 --headless ...
```

### 6.2 我们的扩展被真引擎加载（观测）

`godot-probe/`：一个最小项目 + `gw2.gdextension` 指向我们的 `.so`。

```
[ 33% ] 正在校验 GDExtension……
[gw2-rebuild] initialize level=0   ← CORE
[gw2-rebuild] initialize level=1   ← SERVERS
[gw2-rebuild] initialize level=2   ← SCENE
[gw2-rebuild] initialize level=3   ← EDITOR
[gw2-rebuild] resolve: class4=true string_name=true construct=true set_instance=true
[gw2-rebuild] registered class GW2Rebuild (parent RefCounted)
```

四个初始化级别全部触发，接口全部解析成功。

### 6.3 类注册被 GDScript 确认（观测）

```gdscript
print("GW2_HAS_CLASS=", ClassDB.class_exists("GW2Rebuild"))
var o = ClassDB.instantiate("GW2Rebuild")
print("GW2_INSTANCE=ok class=", o.get_class())
```

```
GW2_HAS_CLASS=true
GW2_INSTANCE=ok class=GW2Rebuild
```

实例化走 `classdb_construct_object("RefCounted")` + `object_set_instance`。
**这比 `dlopen` 探针强一档**：不是"按 ABI 约定能调通"，而是**引擎真的认了这个类并造出实例**。

### 6.4 方法注册与调用（观测）

注册方法需要 `GDExtensionClassMethodInfo`。**第一次尝试让引擎 abort 了**，
原因是我漏了结构体末尾两个字段：

```c
uint32_t default_argument_count;
GDExtensionVariantPtr *default_arguments;
```

补上后正常。实测：

```
[gw2-rebuild] registered method ping()
GW2_HAS_CLASS=true
GW2_INSTANCE=ok
GW2_HAS_PING=true
[gw2-rebuild] ping() called from GDScript   ← Rust 侧回调真的执行了
GW2_PING_DONE
```

`ping()` 无参数、无返回值，所以不需要构造 Variant；它证明的是
**"引擎 → 我们代码"的调用链通了**。带参数/返回值的方法仍未做。

> 教训：跨语言结构体必须逐字段核对头文件，少一个尾部字段就可能越界读栈。

### 6.5 原扩展的完整 API 面（观测，**重要的负结果**）

`godot-ref/`：把 `libgraphwarrust.so` 当 GDExtension 加载。编辑器模式会核心转储
（扩展需要游戏运行态），**运行模式可用**：

```
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)
```

与"不带该扩展的基线"对比（1066 vs 1055 个类），差异 **12 个类**：

```
AxisDrawer  EffectDrawer  EntityDrawer  GraphwarAdSwitch  GraphwarButton
GraphwarFunctionInput  GraphwarLanVBox  GraphwarLongPressAnimation
GraphwarPlane  GraphwarRootNode  GraphwarSetupControl  PlayerShowcase
```

方法一共只有 **25 个**，且几乎全是 UI 回调：

| 类 | 方法 |
|---|---|
| `GraphwarFunctionInput` | `on_text_change`、`on_text_submitted`、`on_size_changed`、`force_update_cb` |
| `GraphwarRootNode` | `request_permissions_result`、`purchase_confirmed/pending/denied`、`log` |
| `GraphwarSetupControl` | `on_text_change`、`on_visibility_change`、`force_title_update`、`on_chat_submitted` |
| `GraphwarPlane` / `EntityDrawer` | `on_resized` |
| `GraphwarButton` | `pressed`、`button_up/down`、`get/set_input_text`… |
| `GraphwarAdSwitch` | `are_ads_enabled`、`get_ad_unit_id` |
| 其余 4 个类 | **没有任何方法** |

⇒ **玩法逻辑没有开放给 GDScript。** 回合、关卡生成、AI、命中结算都不在这 12 个类的方法里——
它们只在 Rust 内部运行，通过场景节点和信号驱动。所以"调一个方法问出游戏规则"的 oracle
在 API 层面就被否定了（后续改用 frida + 自建 harness，见 `06` §13–§14）。

### 6.6 用还原的资源打包并运行（观测）

用 `rsc/` 里还原出来的 `basic_soldier.png`（1024×1016）建最小项目，导出 `.pck` 后**从包启动**：

```
$ ls godot-dist/
libgw2_rebuild.so   out.pck (28K)

$ godot --headless --main-pack out.pck
[gw2-rebuild] initialize level=0..3
[gw2-rebuild] registered class GW2Rebuild (parent RefCounted)
[gw2-rebuild] registered method ping()
GW2_TEX=true size=(1024.0, 1016.0)      ← 还原的资源真的被引擎加载了
GW2_INSTANCE=true
[gw2-rebuild] ping() called from GDScript
```

`.pck` 内容用我们自己的 `tools/pck_extract.py` 独立核对过（11 个文件，含
`basic_soldier.png` 的 `.ctex` 25 102 字节、`gw2.gdextension`、`main.gdc`）。

**踩到的坑**：

- **`.pck` 不包含 `.so`**：扩展库默认与可执行文件并列存放，不在包里。写成
  `res://libgw2_rebuild.so` 会报 `Can't open dynamic library, file not found`；
  改成裸文件名并把 `.so` 放在 `.pck` 同目录才通过；
- 场景脚本引用写错时有**静默失败**（主场景仍指向旧的 `dump.gd`，什么都不打印就退出）。

### 6.7 复现

```bash
# 我们的扩展
cd godot-probe && ../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --editor --path . --quit
# 原扩展的 API 面
cd godot-ref   && ../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path .
```

## 7. 到这一步什么成立、什么不成立

**成立**（每条都有上面某节的实测输出）：

| 结论 | 证据 |
|---|---|
| 复原源码能构建成 Godot 能加载的 GDExtension | §4①② |
| 构建产物的**算法行为**与原版一致 | §4.6：10493 点逐位相同 |
| 真引擎认我们注册的**类**、能造实例 | §6.3 |
| 真引擎能**调到我们的方法** | §6.4 |
| 打包 `.pck` 能从包里启动并加载还原的资源 | §6.6 |

**不成立 / 没做**：

- **没有属性、信号、完整类树**：只注册了 1 个类 + 1 个无参方法 ⇒ **能加载 ≠ 功能完整**；
- **不是可玩客户端**：界面（31 个场景）与脚本逻辑（105 个 `.gdc`）仍是零进度，
  所以这个 `.so` 是"内核扩展"，不是游戏（`README` 工作包表、`00-status` §5）；
- **不是原版行为**：入口只填结构体，不解析任何 VFS/类库。

## 8. 下一步

1. 把已复原的 `parse`/`calculate` 暴露成**带参数与返回值**的方法（先解决 Variant 构造），
   让扩展有实际功能；
2. 注册信号与属性，验证 `GDExtensionClassCreationInfo4` 里其余字段；
3. 界面那一大块（与本文无关的另一条线，见 `92`/`101`）。

## 9. 两个环境坑

**9.1 `export` 里的变量展开顺序**

```bash
export CARGO_HOME=/path/.cargo-home CARGO_TARGET_DIR=$CARGO_HOME/target   # ✗
```
同一条 `export` 里 `$CARGO_HOME` **在赋值生效前**就展开了，`CARGO_TARGET_DIR`
变成 `/target`，于是报 `Read-only file system ... /targetWN5pcA`。
两个变量要分行导出，或都用绝对路径。

**9.2 别把临时产物写 `/tmp`**

`/tmp` 在这台机器上是**每条命令独立的 tmpfs**：上一条命令写的东西下一条就没了
（`90` §1 #4 就因此丢过备份）。探针、日志、抓样一律写工作区内。

## 10. 绑定层换回官方 gdext（最小骨架，2026-11）

**一句话**：我们手写的那套 GDExtension ABI 外壳（`gdext.rs` + `gdext_class.rs` +
`axis_drawer_class.rs` + `api.rs` 的 FFI 段）能整体换成官方 `godot` crate（gdext 0.5.5），
**类名、方法名、返回契约一个字不用改**，`client/` 一行都不用动；代价是 32 行依赖树、
一次性 154 秒的依赖编译、`.so` 781 KB → 5.2 MB。默认**关闭**（`--features gdext-bindings`），
关闭时本 crate 仍然零依赖。

### 10.1 为什么值得换（结论与档位）

| 结论 | 档位 | 证据 |
|---|---|---|
| 原版 `libgraphwarrust.so` 自己就是 gdext 编的 | **观测** | `.rodata` 里有 `/home/lucas/.cargo/git/checkouts/gdext-067f4b88e7bd088f/477501f/godot-core/src/registry/class.rs` |
| 我们的手写层是**等价物**，但**虚方法表一个都没接** | **观测** | `docs/16` §31 原文"它还不会画"；`gdext.rs` 文件头也自己写了这条 |
| 换成 gdext 后 `_draw` 真的被引擎调用 | **观测** | §10.4：`draw_call_count=1 drawn_line_total=102` |
| 原版 `axis_drawer.rs` 的形状就是 `impl IControl … fn draw` | **推断** | arm64 `.symtab` 的符号名 `… as …IControl>::draw`（`docs/16` §18）+ 上面第一条；缺的是 x86 侧的 demangle 符号（stripped） |

一句话：**源码级还原要的是"长得像原版"，而原版的绑定层是 gdext**。手写 ABI 证明了
"能注册、能调"，但长不出 `impl IControl` 这种结构，也永远接不上虚方法表。

### 10.2 改了什么

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | 新增 `[features] gdext-bindings = ["dep:godot"]`（`default = []`）与可选依赖 `godot = { version = "0.5", optional = true }` |
| `src/_scaffold/mod.rs` | 手写入口加 `#[cfg(not(feature = "gdext-bindings"))]`；新增 `#[cfg(feature = "gdext-bindings")] pub mod gdext_mod;` |
| `src/_scaffold/gdext_mod.rs` | **新增 448 行**（代码 263 / 文档注释 141 / 空行 44）：`GW2Rebuild` + `AxisDrawer` + 入口 + 4 条单测 |
| `client/tools/gdext_check.gd` | **新增**：24 条断言的 headless 验收脚本 |

`src/lib.rs` **一行没动**（gdext 版模块挂在 `_scaffold/` 下面）。

**必须 `#[cfg]` 掉的那一处**：手写 `gdext.rs` 的 `#[no_mangle] fn gdext_rust_init` 与
`#[gdextension]` 宏生成的同名符号**在一个 `.so` 里不能共存**（重复定义）。所以 feature
打开时只关掉手写入口模块；`gdext_class.rs` / `axis_drawer_class.rs` / `capi.rs` 照旧编译，
只是没人调（类不会注册）。这是**唯一**为兼容性做的让步。

### 10.3 a) 两种 feature 各跑一遍（2026-11-10 复测；括号里是 2026-11-07 的当轮值）

```text
$ cargo test --offline                              1756 passed / 0 failed   （1585）
$ cargo test --offline --features gdext-bindings    1862 passed / 0 failed   （1669）
$ cargo tree | wc -l
  1                       # 仍然零依赖（只有 gw2-rebuild 自己）
$ bash client/run.sh selftest
  SELFTEST 717 passed, 0 failed, 1 skipped           # 前置条件见 §10.3.1（709）
$ cargo test --offline 2>&1 | grep '^test result'    # 求和 "ok. N" 即得总数
```

#### 10.3.3 `selftest` 的"整组少跑"档 —— **根因已定：`user://` 不可写**（2026-11-10）

96 号把这一档定因了，并且**把"少跑"做成显式**（`@@SELFTEST_COVERAGE … missing=… (claimed/degraded/silent)`）。
我在本机复核时又找到**触发它的具体条件** —— **是调用方的环境变量，不是代码**：

```
错法（我先前就是这么跑的）：
    export XDG_DATA_HOME=/home/user/.local/share      ← 指向工作区**外面**
    bash client/run.sh selftest
  ⇒ Godot 的 `user://` 落在沙箱外 ⇒ **写不进去** ⇒ 两组"真服务端"用例各跑几条就 return
  ⇒ @@SELFTEST_COVERAGE … missing=78(claimed 6 / degraded 72 / silent 0) ；SELFTEST 632 / 7 failed / 9 skipped

正法（run.sh 本来就这么设计）：
    env -u XDG_DATA_HOME -u XDG_CACHE_HOME -u XDG_CONFIG_HOME bash client/run.sh selftest
  ⇒ run.sh 把三个 XDG 变量指到 `<ws>/.agent-work/client/{data,cache,config}`（**工作区内**）
  ⇒ @@SELFTEST_COVERAGE groups=33 checks=717 expected=717 **missing=0** ；SELFTEST **717 / 0 / 1 skip**
```

**结论**：
1. 96 号的诊断（`user://` 不可写）**对的**，这一档不是代码缺陷；
2. **触发条件是调用方自己 export 了 `XDG_DATA_HOME`**（`run.sh` 用的是 `${XDG_DATA_HOME:-…}`，会**尊重**已导出的值）；
3. ⇒ **跑 selftest 不要预设 XDG_* 变量**；headless 巡检要设就设在工作区内；
4. "少跑"现在**不会静默**了（`silent 0`），这是 96 号真正的贡献。


## 11. 跨平台编译验证：Windows x86_64 / Android arm64（2026-11）

### 11.1 结论

```text
target                      check   build   失败原因 / 备注
x86_64-pc-windows-gnu       ✅      ✅      系统里没有 mingw-w64 ⇒ 用便携 llvm-mingw 链成功
aarch64-linux-android       ✅      ✅      系统里没有 NDK     ⇒ 用便携 NDK r27c 链成功
```

`check` = `cargo check --target`（默认 feature 与 `--features gdext-bindings` **各一次**，都过），
不经过链接器，所以它验的是"代码在目标平台上类型/API 成立"；`build` = `cargo build --release --target`。

`check` 全过本身是个结论（**观测**）：`src/` 里**没有** `std::os::unix` 之类的平台专用 API
（`grep -rn 'os::unix' src/` 零命中）；平台相关的只有 `std::os::raw::c_char` 三处，
以及 `std::fs` / `std::net`（TCP）/ `std::env::temp_dir()` —— 都是标准库保证跨平台的用法。
真正会"编得过但做错事"的两处在 §11.7 修掉。

### 11.2 这台机器缺什么、怎么补（不装系统包、不要 root）

| 缺的 | 补法（实测） |
|---|---|
| `x86_64-w64-mingw32-gcc` | 便携 **llvm-mingw 20240917（ucrt, ubuntu-20.04-x86_64，87,950,564 B）** 解到 `.reference/toolkits/`，把它的 `bin/` 放进 `PATH`。**这个可执行文件的名字正好是 rustc 给 `x86_64-pc-windows-gnu` 找的默认链接器名** ⇒ `Cargo.toml` 不用动、`CARGO_TARGET_..._LINKER` 也不用设 |
| NDK 的 `aarch64-linux-android-clang` | 便携 **NDK r27c**（linux zip 663,987,688 B，解压 2.0 GB）⇒ 设 `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=<ndk>/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang` |

llvm-mingw 侧要打**两个软链**（工具链目录内的 hack，**不进仓库**）：rustc 给 windows-gnu 的尾部
链接参数带 `-lgcc -lgcc_eh`，而 llvm-mingw 用 libunwind/clang_rt 顶掉 libgcc：

```bash
cd <llvm-mingw>/x86_64-w64-mingw32/lib
ln -sf libunwind.a libgcc_eh.a
ln -sf ../../lib/clang/19/lib/windows/libclang_rt.builtins-x86_64.a libgcc.a
```

**试过但不行的一条**：zig 0.13（47,082,308 B）的 `zig cc -target x86_64-windows-gnu` 当链接驱动 ——
`-lgcc/-lgcc_eh` 它自带，但 rustc 会传 `-Wl,<tmp>/list.def`（cdylib 的导出表来源），
zig 的驱动直接 `error: unsupported linker arg: .../list.def` ⇒ 链接失败。
llvm-mingw 的 `x86_64-w64-mingw32-gcc` 把 `.def` 透传给 `ld.lld`，`ld.lld` 认这个文件，才成立。

### 11.3 实测命令

```bash
export RUSTUP_HOME=<ws>/.reference/rustup CARGO_HOME=<ws>/.reference/cargo-home
export CARGO_TARGET_DIR=$CARGO_HOME/target PATH=$CARGO_HOME/bin:$PATH
LMW=<ws>/.reference/toolkits/llvm-mingw-20240917-ucrt-ubuntu-20.04-x86_64/bin
NDK=<ws>/.reference/toolkits/android-ndk-r27c/toolchains/llvm/prebuilt/linux-x86_64/bin

# ① check（不链接；两个 target × 两种 feature，共 4 次）
PATH=$LMW:$PATH cargo check --target x86_64-pc-windows-gnu
PATH=$LMW:$PATH cargo check --target x86_64-pc-windows-gnu --features gdext-bindings
cargo check --target aarch64-linux-android                    # check 不链接，默认 PATH 就够
cargo check --target aarch64-linux-android --features gdext-bindings

# ② build（这一步才要链接器）
PATH=$LMW:$PATH cargo build --release --target x86_64-pc-windows-gnu
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDK/aarch64-linux-android21-clang \
  cargo build --release --target aarch64-linux-android
```

### 11.4 产物（实测）

```text
x86_64-pc-windows-gnu/release/gw2_rebuild.dll     2,787,328 B   PE32+ (DLL), x86-64
aarch64-linux-android/release/libgw2_rebuild.so     826,992 B   ELF aarch64, for Android 21, NDK r27c
```

- **导出（观测）**：Windows 的 COFF 导出表 **14 条**，与 Android `.so` 的 14 个 `T` 符号
  **同名同集合**（`gdext_rust_init` + 13 个 `gw2_*` C ABI）——两个平台对 Godot 的入口约定一致。
- **导入（观测）**：DLL 只依赖系统 DLL —— `KERNEL32` / `ntdll` / `USERENV` / `WS2_32` /
  `bcryptprimitives` / `api-ms-win-crt-*`；**没有** `libgcc_s_seh-1.dll`、`libwinpthread-1.dll`
  这类要随包发的 mingw DLL ⇒ 目标机不用额外放 DLL。
  代价：这套 sysroot 是 **UCRT** 口径 ⇒ 至少要 Windows 10（老系统要靠 UCRT 更新包；**未在真 Windows 上验**）。
- **Android 依赖（观测）**：`NEEDED` 只有 `libdl.so` / `libm.so` / `libc.so`（都是 bionic 系统库），
  没有 `libc++_shared.so`。

### 11.5 入口契约在 Windows 上真的成立（观测，wine 11.21）

本机有 wine，所以这次不止"编过了"：把 `gw2_rebuild.dll` 加载起来走一遍 Godot 的调用序列。

```text
ok   LoadLibrary(gw2_rebuild.dll) = 00006FFFFAEE0000
ok   GetProcAddress(gdext_rust_init) = 00006FFFFAFA4CF0
ok   entry returned 1 (expect 1)
ok   minimum_initialization_level = 0 (expect 0)
     initialize=00006FFFFAFA4690 deinitialize=00006FFFFAFA4AF0
ok   counters entry=1 init=1 deinit=1
RESULT: PASS
```

探针就是 Linux 版 `tools/gdext_probe.c` 的 Windows 版：`dlopen/dlsym` 换成 `LoadLibrary/GetProcAddress`，
其余一样 —— 桩 `get_proc_address` → 调入口 → 检查返回 1 与 `minimum_initialization_level=0`
→ 回调 `initialize`/`deinitialize` → 最后读 Rust 侧导出的
`gw2_rebuild_{entry,initialize,deinitialize}_calls` 计数器。**计数器 1/1/1 才说明回调真的执行了**，
不是"入口返回 1 就算过"。用 llvm-mingw 的 `x86_64-w64-mingw32-gcc` 编译探针即可。

⚠ wine **不是** Windows：加载器与 UCRT 实现都有差异。这一步的档位是**观测（unix 上的 PE 加载器）**，
**不**等于"在 Windows 上验过"（§11.8）。

### 11.6 `.gdextension` 库名/路径核对

| 条目 | 文件里写的 | 实际产物 | 一致 |
|---|---|---|---|
| `windows.{debug,release}.x86_64` | `res://gw2_rebuild.dll` | `gw2_rebuild.dll`（`[lib] name = "gw2_rebuild"`，Windows 无 `lib` 前缀） | ✅ |
| `linux.{debug,release}.x86_64` | `res://libgw2_rebuild.so` | `libgw2_rebuild.so` | ✅ |
| `android.{debug,release}.arm64` | `res://android/arm64-v8a/libgw2_rebuild.so` | `libgw2_rebuild.so`（`arm64-v8a/` 由 Godot 导出时铺） | ✅ |
| `macos.{debug,release}` | `res://libgw2_rebuild.dylib` | 未编译（本机无 macOS target / Apple SDK） | 名字符合 Rust 惯例，**未验** |

⇒ 这一项**没有要改的**（所以本轮没动 `client/gw2rebuild.gdextension`）。
缺口（没做，不是错）：没有 `windows.arm64` / `linux.arm64` 条目（对应 target 也没装）；
Android 真要出 APK 还缺 `client/export_presets.cfg` + Android SDK + JDK。

### 11.7 修的两处平台问题（都在 `_scaffold/`，不改逻辑）

1. **`src/_scaffold/rng.rs`：`/dev/urandom` 在 Windows 上不存在。**
   原来 `seed_from_os()` 直接 `File::open("/dev/urandom")` —— Linux/Android 走通，**Windows 上必然
   open 失败**，静默退化成"时间戳 + xorshift"种子（每局种子可预测）。
   改法：熵源抽成 `os_entropy()`；`#[cfg(not(windows))]` 走 `/dev/urandom`（与原代码逐字相同），
   `#[cfg(windows)]` 用 std 的 `RandomState`（k0/k1 由 OS 播种、每次 `new()` 递增）拼 32 字节；
   **失败回退路径一个字没动**。
   ⚠ 差异：Windows 上真正的 OS 熵只有 128 bit（不是 32 字节全熵）——**推断**，本机无 Windows 未实测；
   要对齐成 32 字节全熵得换 `BCryptGenRandom`（要加 FFI + 链接 bcrypt）。
2. **`src/_scaffold/replaydiff.rs`：`default_corpus_dir()` 用 `$HOME` 拼 unix 路径。**
   Windows 上 `HOME` 通常不设 ⇒ 退化成 `./.local/share/...`，等于找不到回放。
   改法：抽成 `godot_user_replays()`；`#[cfg(windows)]` 用
   `%APPDATA%\Godot\app_userdata\Graphwar\replays`，`#[cfg(not(windows))]` 与原来那一行不差；
   `GW2_REPLAY_DIR` 的覆盖优先级不变。
   Windows 那条路径是 **推断**（Godot 4 `user://` 口径，本机无 Windows 桌面对拍）；
   Android 的 `user://` 在应用私有目录、具体值 **未知**（要真机 `OS.get_user_data_dir()`），
   所以 Android 上仍退化为相对路径 + 用 `GW2_REPLAY_DIR` 指定。

这两处都是"跨平台能编、但会静默做错事"，**不是**编译错误 —— 改之前那 4 次 `check` 就已经全绿。

### 11.8 做不了的（如实写）

| 想验的 | 状态 | 缺什么 / 怎么补 |
|---|---|---|
| Windows 真机加载 DLL | ✗ | 没有 Windows 机器，只有 wine（§11.5）。要真验：一台 Win10+ 跑 `godot --headless --path client --import` 再看 `ClassDB` |
| Android 真机 / 模拟器加载 `.so` | ✗ | 没有设备；`which qemu-aarch64` 零命中，且即便有 qemu-user 也跑不了 bionic 的动态链接。要真验：arm64 安卓机 + `adb`，或导出 APK 装上 |
| Android APK 打包 | ✗ | 缺 `client/export_presets.cfg`、Android SDK、JDK（NDK 已有便携版） |
| macOS `.dylib` | ✗ | 缺 macOS target + Apple SDK（本机也拿不到） |
| `windows.arm64` / `linux.arm64` | ✗ | 对应 rustc target 没装，也没有 arm64 Windows/树莓派环境 |

### 11.9 回归

改了 `src/` ⇒ 本机默认 target 重跑 `cargo test --offline`：**lib 464 / 总计 1520**、0 failed、
1 ignored（与基线一致）（**审计**：本节原记「lib 457 / 总计 1499」，2026-11-07 深夜复测 464 / 1520）；
重跑 `tools/coverage_matrix.py` 后 `--check` **OK**，
两个生成物一起提交（差异只有 `src_tree_sha256` 与 `src_fns 2171 → 2175`，
归属 527、逐位 88 / 观测 439 **不变**）。
