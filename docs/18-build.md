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
