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
| 我们的手写层是**等价物**，但**虚方法表一个都没接** | **观测** | `docs/16` §35 原文"它还不会画"；`gdext.rs` 文件头也自己写了这条 |
| 换成 gdext 后 `_draw` 真的被引擎调用 | **观测** | §10.4：`draw_call_count=1 drawn_line_total=102` |
| 原版 `axis_drawer.rs` 的形状就是 `impl IControl … fn draw` | **推断** | arm64 `.symtab` 的符号名 `… as …IControl>::draw`（`docs/16` §21）+ 上面第一条；缺的是 x86 侧的 demangle 符号（stripped） |

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

### 10.3 a) feature 关闭时：一切照旧（实测）

```text
$ cargo test --offline
  64 个 test 二进制；lib 457 passed / 总计 1488 passed / 0 failed
$ cargo test --offline --features gdext-bindings
  64 个 test 二进制；lib 457 passed / 总计 1488 passed / 0 failed
$ cargo tree | wc -l
  1                       # 仍然零依赖（只有 gw2-rebuild 自己）
$ bash client/run.sh selftest
  SELFTEST 694 passed, 0 failed
```

两种 feature 的条数**完全相同**是巧合：feature 打开时手写 `gdext.rs` 的 4 条单测被
`#[cfg]` 掉、`gdext_mod.rs` 又补了 4 条。

### 10.4 b) feature 打开时：5 个方法 + 真节点类都活着（headless 实测）

```text
$ cargo build --release --features gdext-bindings
$ cp target/release/libgw2_rebuild.so client/libgw2_rebuild.so
$ godot --headless --path client --import
$ godot --headless --path client --script res://tools/gdext_check.gd

Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)
      before load: GW2Rebuild=true AxisDrawer=true
ok   ClassDB.class_exists("GW2Rebuild")
ok   ClassDB.instantiate("GW2Rebuild") 非空
      kernel.get_class() = GW2Rebuild
ok   内核方法 ping() 存在
ok   内核方法 path_calculate() 存在
ok   内核方法 path_calculate_mode() 存在
ok   内核方法 hit_test() 存在
ok   内核方法 level_obstacles() 存在
      ping() -> pong:gdext（String）
      binding_version() -> gdext-bindings/gdext
ok   path_calculate("x", 0, 0, +1).ok
ok   path_calculate 出点：2000 个
      first=(0.0, 0.0) last=(14.9925, 14.9925) offset=0.0
ok   path_calculate_mode(...,"normal") 与 path_calculate 点数一致（2000）
ok   path_calculate_mode(...,"ode") 能算（dy/dx = x + y）
ok   未知 mode 走 bad_arguments（不猜默认值）
ok   hit_test(...).ok
      hit_test -> hits=[0] first_hit=-1 blocked=false stop=(14.9925, 14.9925)
ok   level_obstacles(0..20, 12345) 全部 ok，且扁平数组都是 3 的倍数
ok   至少一关有障碍：18 关 / 共 139 个
ok   ClassDB.class_exists("AxisDrawer")
ok   AxisDrawer 能实例化且是 Control
ok   属性 axis_mode=3 往返一致
ok   axis_mode=3 ⇒ 枚举 102 条 / 公式 102 条
      _draw 观测: draw_call_count=1 drawn_line_total=102
ok   _draw 真的发了线（102 条 ≥ 102）
GDEXT 24 passed, 0 failed
```

**同一份脚本对两份绑定都跑**（A/B，这是"换绑定层对客户端透明"的第二条证据）：

```text
# 手写版（master 源码，feature 关闭，781 KB）
[gw2-rebuild] ping() called from GDScript
      ping() -> <null>（Nil）
GDEXT 23 passed, 0 failed      # 少的一条是 draw_call_count —— 手写层没有虚方法表，脚本主动跳过
```

三处**已知差异**（都在观测范围内，客户端都不读）：

1. `ping()`：手写版是空方法（`has_return_value = 0` ⇒ 返回 `null`），gdext 版返回
   `"pong:gdext"`。留作"现在加载的是哪份绑定"的指纹。
2. `binding_version()` / `draw_call_count()` / `drawn_line_total()` 是 gdext 版**新增**的
   自检口子，不在 `core_bridge.gd` 的契约里。
3. ⚠ **`client/gw2rebuild.gdextension` 的 `compatibility_minimum` 需要从 `4.1` 抬到 `4.6`**：
   gdext 0.5.5 编译目标是 Godot **4.6** 的 API（上面那行 `API v4.6.stable.official`）。
   在 4.7.2 上 4.1 照样能加载（实测），但引擎版本低于 4.6 时 gdext 会 `panic`
   （`runtime < static`，见 `godot-ffi` 的 `ensure_static_runtime_compatibility`）。
   本轮**没有改**这个文件（`client/` 下不是本任务的改动面），合并这条线时要一起改。

> 顺手的一条观测（`AGENTS.md` "改了但没生效" 第 2 条）：仓库里的
> `client/libgw2_rebuild.so` 是 **11-06 23:22 的旧构建**（14.9 MB，带 debug_info），
> 里面**没有 `AxisDrawer`** —— 第一次拿它跑本脚本会失败 2 条；用 master 源码重编（781 KB）
> 之后 23 条全过。别拿仓库里那份旧 `.so` 判断当前源码。

### 10.5 c) 代价（全部实测，同一台机器、同一 target 目录）

| 指标 | feature 关 | feature 开（gdext） |
|---|---|---|
| 依赖树 `cargo tree \| wc -l` | **1**（只有本 crate） | **32**（godot 0.5.5 及其 16 个传递依赖） |
| 冷 release 构建 | **7.96 s** | **161.6 s**（2 分 42 秒；多出来的 154 s 全是一次性编依赖） |
| 增量 release 构建（改一行 `.rs`） | **5.76 s** | **8.00 s** |
| `.so`（release，not stripped） | **781 072 B**（763 KB） | **5 453 776 B**（5.2 MiB），**×7.0** |
| `.so`（debug） | 18 904 096 B（18.0 MiB） | 118 144 136 B（**112.7 MiB**） |

**要不要联网：要（一次性）**。

- 首次构建从 crates.io 拉 **17 个 `.crate`，合计 11.3 MB**（解包 77.4 MB）；之后 `--offline` 可编。
- `Cargo.lock` 目前在 `.gitignore` 里 ⇒ **版本没钉住**。两种收口：
  1. 提交 `Cargo.lock`（改 `.gitignore`，零成本）；
  2. `cargo vendor`（≈77 MB 源码）+ `.cargo/config.toml`（可离线、可审计，但仓库变重）。
  建议先做 1 —— 换机器时仍然要联网拉一次，但版本至少是确定的。
- **日常开发不受影响**：增量是 8 s，和 feature 关闭时一样。痛的是 CI 冷启动与 debug `.so`。

### 10.6 d) 12 个类的迁移量

| 东西 | 手写（今天） | gdext |
|---|---|---|
| 入口 + 类注册 + 方法/属性注册 | `gdext.rs` 309 行（代码 200）+ `gdext_class.rs` 294 行（代码 235） | `gdext_mod.rs` §4：**12 行（代码 3）** |
| 一个 5 方法的算法单例（`GW2Rebuild`） | 上面两个文件 + `api.rs` 第 540 行以后的 FFI 段（约 2.2k 行） | `gdext_mod.rs` §2：**128 行（代码 80）** |
| 一个带 `_draw` 的节点类（属性 + 4 方法 + 绘制本体） | `axis_drawer_class.rs` **800 行（代码 584）**，**且虚方法表没接** | `gdext_mod.rs` §3：**152 行（代码 92）**，含 `_draw` |
| 类型搬运（`ApiValue` → `Dictionary`/`Packed*`） | `api.rs` 的 `write_dict` 等（在 2.2k 行里） | `gdext_mod.rs` §1：**58 行（代码 38）**，一次写下、所有类共用 |

⇒ **一个类的量级：70–130 行代码**（doc 注释另计；上面 `-AxisDrawer` 92 行里还包含
4 个辅助函数）。**12 个类粗估 ~1200 行代码**（含共享的 §1），对比手写路线外推的 ~7000 行；
**而且手写路线根本没有虚方法表**，那部分不是"少写代码"，是"写不出来"。

反过来说清楚**迁移量不在"写"，在"读"**：每个原版模块要读的是
`__register_methods` / `ImplementsGodotVirtual` 里那些**属性名、方法名、方法表标志**
（`docs/02-module-map.md` §17 的归属 + 各模块的 `.rodata` 名字表），这部分工作量换不换 gdext 都一样。

### 10.7 怎么复现

```text
# 1) 编 gdext 版（默认 feature 关闭，所以必须显式打开）
cargo build --release --features gdext-bindings

# 2) 先备份手写版，再覆盖（client/*.so 已在 .gitignore 里）
cp client/libgw2_rebuild.so ../.agent-work/keep-libgw2_rebuild-hand.so   # 别写 /tmp，见 §9.2
cp <target>/release/libgw2_rebuild.so client/libgw2_rebuild.so

# 3) 第一次必须先 import（否则 --script 模式下扩展不加载，`AGENTS.md` 那条坑）
godot --headless --path client --import
godot --headless --path client --script res://tools/gdext_check.gd

# 4) 验完换回手写版，再跑一遍 selftest（它用的是默认 feature）
bash client/run.sh selftest
```

⚠ 第 4 步别省：覆盖之后 `client/libgw2_rebuild.so` 就是 gdext 版了，`selftest` 跑的是它。

### 10.8 这条线到什么程度（成立 / 不成立）

**成立（实测/观测）**：类名与方法契约不变；`_draw` 虚方法真的被引擎调到；同一份脚本对
两份绑定都过；feature 关闭时零依赖、条数与 master 一致、selftest 694 不变。

**不成立 / 没做**：

- 只迁了 **2 个类**（`GW2Rebuild` / `AxisDrawer`）；12 个 GDExtension 类里其余 10 个没动；
- `client/gw2rebuild.gdextension` 的 `compatibility_minimum` 还是 4.1（见 §10.4 差异 3）；
- 没有 vendor、`Cargo.lock` 没提交；
- 手写层的**报文 / C ABI / broker** 那些（`capi.rs` / `protocol.rs` / `broker.rs`）与
  gdext 无关，仍然留在 `_scaffold/`；
- §8 的"下一步 1"（把 `parse`/`calculate` 暴露成带参数与返回值的方法）**已经过时**：
  那件事在 `docs/116`/`120`/`126`/`138` 里做完了，本节就是它的 gdext 版。
