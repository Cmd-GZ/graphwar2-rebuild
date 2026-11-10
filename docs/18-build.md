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

#### 10.3.3 **整组"少跑"**：`632/7` 那一档的定因与处置（2026-11-10 新增，**已做成显式**）

> **一句话**：这一档**不是"红了几条"，是"有一整组断言根本没跑"** —— 绿着/红着都看不出来。
> 真身是 **`632 passed / 7 failed / 1 skipped`，断言总数 639，而全量 717 ⇒ 78 条一条没跑**。
>
> 根因（**观测**，可确定性复现）：**`user://` 不可写** ⇒ `serve_match` 的 `LISTEN` 那行从来没落进日志文件
> ⇒ 两组"真服务端"**各跑 2 条就 `return`**（少 **34 + 38 = 72** 条）；另有 6 条是第二种形态：
> **`if <文件打开成功>: <断言>` 没有 `else`**（`pixel_truth_geometry` 3 条 + `campaign.save` 3 条）。
>
> 处置（**判据一条没改、断言一条没删**）：给 `selftest` 加**组级对账** —— 每条"没跑"的断言都必须有一条
> skip 或 FAIL 认领它，**没人认领 ⇒ 记 FAIL、退出码 1**；整组降级 ⇒ 打 `DEGRADED` 横幅 + 逐条 skip 带原因。

##### (1) 先把读数分清：五种"降级"各是什么、少的是哪几条（本机实测 2026-11-10）

| 读数（`bash client/run.sh selftest`） | 断言总数 | 少跑 | 少的组（`@@SELFTEST_GROUP_SHORT` 逐行） | 认领者 | rc |
|---|---|---|---|---|---|
| `717 passed, 0 failed, 1 skipped` | 717 | 0 | — | 唯一那条 skip：本局没有靶子 | 0 |
| `641 passed, 0 failed, 3 skipped` | 641 | 76 | `net_real_server` 36 + `play_online_real_server` 40 | 2 条 skip：没找到 `examples/serve_match` | 0 |
| `654 passed, 0 failed, 3 skipped` | 654 | 63 | `replay_player` 47 + `replay_ode` 16 | 2 条 skip：回放样本不在 `$HOME/.local/share/…/Graphwar/replays` | 0 |
| `689 passed, 3 failed, 22 skipped`（缺 `client/assets/`） | 692 | 25 | `fonts_and_icons` 3 / `assets_and_original_settings` 6 / `audio` 6 / `geometry_and_campaign_save` 8 / `play_flow` 2 | 21 条 skip + 3 FAIL（+1 条既有 skip） | 1 |
| **`632 passed, 7 failed, 9 skipped` + `DEGRADED`（`user://` 不可写）** | **639** | **78** | `net_real_server` **34** + `play_online_real_server` **38** + `pixel_truth_geometry` 3 + `geometry_and_campaign_save` 3 | 8 条 skip（带原因）+ 4 行 SHORT；**改前是 0 认领** | 1 |
| `642 passed, 3 failed, 3 skipped`（`GW2_NET_SERVER=/bin/false`，故意坏） | 645 | 72 | 同上两组，各 2 条 | 2 条 skip："进程在 LISTEN 之前就退出了" | 1 |

> **判据只有一条**：`@@SELFTEST_COVERAGE … missing=0` 才是"真全跑"。
> `passed` 数**不能**当判据 —— 上表里 `639`（78 条没跑）与 `641`（76 条没跑）只差 2，
> 历史上正是因此把"两套联机整段 skip"和"两组各跑 2 条就收摊"看成了同一件事。

##### (2) `629/7` 到底是什么（取证）：它**不是一个真实读数**

- 全工作区 `grep -rn "629 passed"` 只命中**一份日志**：第 31 轮的 `.agent-work/cg/self31_after5.log`，
  写的是 **`SELFTEST 629 passed, 0 failed, 2 skipped`** + 两条 `SKIP 联机…没找到 examples/serve_match`
  ⇒ 那是**"缺 serve_match"那一档**，`failed` 是 **0**，不是 7。
- `grep -rn "629 passed, 7 failed"` / `"632 passed"` 在**任何日志**里都零命中（`632 passed` 只出现在
  `dispatch/` 的任务书与报告正文里，无原始输出）。
- 所以旧版本节那句「`629 + 7 = 636` ⇒ 正好是『没有 serve_match』那一档的分母」**建立在两个不同运行的
  拼接上**：`629` 来自"缺服务端"那一档，`7` 来自另一处，`636` 这个分母也不存在（2026-11-10 复测："缺 serve_match"那一档是 **641**）。
  **这一条按 `AGENTS.md` §0.6 就地更正**：旧签名作废，换成 (1) 表里那三档实测读数。
- 「78 条」这个数是对的（`717 − 639`），「632 + 7 + 1 = 640 ⇒ 少 78」的算法**不对**：
  skip 不是断言，正确算式是 **`717 − (632 + 7) = 78`**。

##### (3) 逐前置抽掉：各少跑多少条、少的是哪几条（2026-11-10 本机实测）

四前置 = ① `client/assets/`（gitignore）② 回放样本（`$HOME/.local/share/godot/app_userdata/Graphwar/replays`）
③ `cargo build --examples serve_match`（且 `CARGO_TARGET_DIR` 指到它）④ `$CARGO_TARGET_DIR/release/libgw2_rebuild.so`
覆盖 `client/libgw2_rebuild.so` 且 `--import` 过。**抽掉一个的后果**：

| 抽掉的 | 读数 | 少跑 | **少的组：条数** | 机制（改前） |
|---|---|---|---|---|
| ① `client/assets/` | `689/3/22` | 25 | `fonts_and_icons` 3、`assets_and_original_settings` 6、`audio` 6、`geometry_and_campaign_save` 8、`play_flow` 2 | 资产组的 `_skip` + `return`（**已声明**，本来就有） |
| ② 回放样本 | `654/0/3` | 63 | `replay_player` 47、`replay_ode` 16 | 同上（**已声明**） |
| ③ `serve_match` | `641/0/3` | 76 | `net_real_server` 36、`play_online_real_server` 40 | 同上（**已声明**） |
| ③' 只抽 `CARGO_TARGET_DIR` | `641/0/3` | 76 | 同上 | 同上（**同一档**：找不到二进制） |
| ① + ③ | `613/3/24` | 101 | 上两行合并 | 已声明 |
| **`user://` 不可写** | **`632/7/9`** | **78** | `net_real_server` **34**、`play_online_real_server` **38**、`pixel_truth_geometry` 3、`geometry_and_campaign_save` 3 | **静默**（本次要修的） |
| `GW2_NET_SERVER=/bin/false` | `642/3/3` | 72 | 同上两组，各 2 条 | 改前静默；改后 2 条 skip 带原因（"进程在 LISTEN 之前就退出了"）+ 2 条前置仍红 |

两条算术值得写在纸上：

- `25 + 76 = 101`：资产档与联机档**互不重叠**，加得起来 ⇒ 两个前置是独立的。
- `78 = 72 + 6`：`72` = 两组真服务端各跑 2 条（36−34、40−38），`6` = 两处守卫块。
  它比"缺 serve_match 那一档的 76" **只多 2**，却**红 7 条 vs 绿 0 条** —— 这就是从
  `passed`/`skipped` 数上分不出这两档的原因，也是这条洞最危险的地方。

##### (4) 根因链（观测）

```text
user:// 不可写（目录只读 / 磁盘满 / HOME·XDG_DATA_HOME 指错）
  └─ `_spawn_server()` 里 `FileAccess.open(<user://serve_match_logs/…log>, WRITE)` 失败
       └─ 于是 `2> <log>` 也写不进去 ⇒ 服务端**就算起来了也没有 LISTEN 那行可读**
            └─ 600 帧后 `ok == false`、`port == 0`
                 ├─ `_test_net_real_server`：2 条 `_expect` 后 `return`  ⇒ **少 34 条**
                 └─ `_test_play_online_real_server`：6 次尝试全败后 `return` ⇒ **少 38 条**
```

同一时刻 `user://` 上的**另外四类写**也会红，正好凑出那 7 条：
`campaign.save` 3 条（`写入 level_7 成功`／`能打开刚写入的 campaign.save`／`单调写入后文件仍是 'level_7'`）
+ 1 条 `静音：muted 落盘后可被新的 GwStore 读回` + 2 + 1（两组真服务端的前置）。
**这 7 条不是代码回归**，但改前它们**无法**告诉你"还有 78 条根本没跑"。

**为什么在这台机器上"套跑顺序"本身复现不出来（否定证据）**：按任务书的场景
（`cargo build --release --features gdext-bindings` → `cp .so client/` → `--import` → 连跑 **14** 个
`*_check.gd` headless 脚本 → 紧接着 3 次 `selftest`）本机 **3/3 都是 `717/0/1`**
（14 个脚本全 `rc=0`、断言合计 1930；随后 3 次 selftest 的 `@@SELFTEST_COVERAGE` 都是 `missing=0`）。
⇒ **不是"跑完 headless 就会降级"**，触发条件是**前置层**（`user://` 可写性 / 服务端日志可读性）；
上游那两次具体是什么让 `user://` 写不进去，**没有日志可查 ⇒ 标「未知」**（不猜）。

##### (5) 修法：把"少跑"变成看得见的东西（判据零改动）

三件事，都在 `client/tools/selftest.gd`：

1. **组级进出 + 冻结表**：`run()` 里每组前后 `_group_begin()/_group_end()`，打一行
   `@@SELFTEST_GROUP <组名> checks=N fails=F skips=S total=T`；收尾拿 `GROUP_EXPECT`（33 组，合计 717）对账。
2. **一条铁律**：`missing <= fails + skips` 才算"认领过了"；本组调过 `_degrade()` 则整组算"已声明降级"；
   **否则记一条 FAIL（`组 X 少跑了 M 条（期望 E／实际 A）—— 本组没有任何 skip/FAIL 认领它（静默少跑）`）**，
   退出码变 1。⇒ **"绿着但少跑了"在数学上不可能再出现**。
3. **两种"少跑"形态各自补上出口**：
   - 整组降级 ⇒ 新函数 `_degrade(reason)`（= `_skip` + 把本组标成"已声明降级"），
     用在 12 处：10 处原本就是"前置缺 ⇒ `_skip` + `return`"，另 2 处是"真服务端起不来"；
   - 守卫块 ⇒ 给 `if <文件打开成功>:` 补 `else:` 的 `_skip(...)`（`pixel_truth_geometry` 3 处、
     `campaign.save` 3 处），不再让断言在 `f == null` 时凭空消失；
   - `_spawn_server()` 增加**起服务端之前的前置探针**与**失败原因** `srv["err"]`
     （"日志写不进去（user:// 不可写…）"／"进程在 LISTEN 之前就退出了"／"600 帧内没等到 LISTEN"），
     两组的失败分支把它打进 skip 行 —— 下次遇到同一档，**原因直接写在输出里**。

**计数（`_expect(` / `_skip(` / `_degrade(` 的语句级调用点）**：

| | 改前 | 改后 | 说明 |
|---|---|---|---|
| `_expect(` | 697 | **698** | 多的 1 条是覆盖护栏自己（只在"静默少跑"时触发，全绿路径不跑） |
| `_skip(` | 21 | **19** | −10 处改叫 `_degrade(`；+7 处"守卫块没有 else"的显式跳过；+1 是 `_degrade()` 内部的 `_skip()` |
| `_degrade(` | 0 | **12** | 10 处整组前置 + 2 处真服务端起不来 |
| 全量断言 | 717 | **717** | **一条没删、一条没加**（`GROUP_EXPECT` 合计 = 717） |

**没有做的事（红线）**：没有把任何一条断言改成 skip 来对齐总数；`user://` 不可写时
**退出码仍是 1**（那 4 类写失败还在红）；`serve_match` 起不来时**也仍是 1**（"你自己刚构建的那份
服务端起不来"是回归信号，不该被 skip 盖掉）—— 只是现在它**顺带说清**"余下 34/38 条没跑"。

##### (6) 下一个人怎么用（判据速查）

```bash
bash client/run.sh selftest 2>&1 | grep -E '^SELFTEST|@@SELFTEST_COVERAGE|@@SELFTEST_GROUP_SHORT|^DEGRADED|^  - '
# ① missing=0            ⇒ 真全跑（这一行是判据，`passed` 数不是）
# ② missing>0 且 reason=SILENT  ⇒ **静默少跑**（改前就是这样被漏掉的）→ 已自动记成 FAIL
# ③ reason=declared/claimed + DEGRADED 横幅 ⇒ 已声明的降级：横幅里逐组写着差多少条、为什么
# ④ 组数：本机 33 行 @@SELFTEST_GROUP；合计 717
```

⚠️ **加/删断言时同步改 `GROUP_EXPECT`**（`GW2_SELFTEST_GROUP_DUMP=1` 会把可直接粘的 33 行打出来）；
不同步的话护栏自己会红 `@@SELFTEST_GROUP_LONG`／`SHORT`，报错里带期望值与实际值。

##### (7) 连跑记录（2026-11-10，四前置齐，`agent/selftest-78`）

```text
run  1..10 | rc=0 | SELFTEST 717 passed, 0 failed, 1 skipped | @@SELFTEST_COVERAGE missing=0 | 10–16 s
```

同一台机器上**另跑过的档**（不是常态，是取证）：`641/0/3`（缺 serve_match）、`654/0/3`（缺回放）、
`689/3/22`（缺 assets）、`613/3/24`（缺 assets + serve_match）、`632/7/9`（user:// 不可写）、
`642/3/3`（故意坏的服务端）—— 六档**每一档的 78/76/63/25/101/72 条去向都在输出里点得出来**。

> ⚠️ **审计 2026-11-08 改了这一段**：原记「65 个二进制（lib 464 / 539）、总计 **1520** / **1595**、
> `SELFTEST 694`」。现按本机复测替换：`cargo test --offline` = **1585**、`--features gdext-bindings`
> = **1669**（两条轴都是 69 个二进制、0 failed，逐二进制求和），`selftest` 按 `archive/156-client-rounds.md` §49.7 = **709**。
> 复跑命令：`cargo test --offline 2>&1 | grep '^test result'`（求和 `ok\. N` 即得总数）。
> lib/集成的**拆分数**本次没记（`cargo test` 只在每行给总数）；要逐二进制清单跑 `cargo test -- --list`。

> **审计（2026-11-07 深夜）**：本节原记「`64 个 test 二进制；lib 457 / 总计 1488 / 0 failed`、
> 两种 feature 的条数**完全相同是巧合**」。**实测现在是默认 1520 / gdext 1595**（lib 464 / 539），
> 两者**并不相同** ⇒ "完全相同是巧合"那句解释**作废**（它是早期只有 4 条 `gdext_mod` 单测时的现象）。
> （**当轮读数**：这段的 1520 / 1595 是 2026-11-07 的值；**现行 1585 / 1669**，见上一段审计注。）
> 现在的差来自：feature 打开时手写 `gdext.rs` 的入口不编译，而 `gdext_mod.rs` 与各 `*_class.rs`
> 补进更多单测。**条数是回归保护，不是复原进度**（`AGENTS.md` §0.2）。

### 10.3.1 `selftest` 的两个口径：**694 需要什么前置条件**（2026-11-07 深夜，本机四组实测）

> **现行读数（2026-11-10 复测，四前置齐）**：**`717 passed, 0 failed, 1 skipped`**。
> 四个前置各自的档位现在是 **717 / 641（缺 serve_match 或 `CARGO_TARGET_DIR`）/ 689（缺 assets）/
> 613（缺 assets + serve_match）**，逐前置"少跑哪些组、各几条"的表在 **§10.3.3 (1)(3)**。
> ⚠️ 判据从今天起不是 `passed` 数，而是 `@@SELFTEST_COVERAGE … missing=0`（见 §10.3.3 (6)）；
> 另：缺 assets 时固定红的那两条如今是**三条**（多的一条是 `外观：皮肤层贴图来自真资源`，
> 同样依赖真资产，不是回归）。

一句话：**694 不是版本号，是"前置条件全齐"的读数**；缺一样就落到别的数上，四个数都实测过
（⚠️ **审计 2026-11-08**：下面的 694 是 2026-11-07 的**当轮**基数，"全齐"档现已涨到 **709**
＝ 694 + 15（`archive/156-client-rounds.md` §49.7）；其余三档没有跟着复测，**仍按当轮值记**，不要当成现行读数）：

| `client/assets/` | 编了 `examples/serve_match` | 结果 | 差在哪 |
|---|---|---|---|
| ✅ | ✅ | **694 passed / 0 failed** | **完整口径**。gdext 版与手写版 `.so` **都**给 694（sha256 `3a1f28a1…` / `85b1cf3b…`）⇒ 换绑定层对客户端透明 |
| ✅ | ❌ | **618 passed / 0 failed / 2 skipped** | 少的 76 条 = 两条联机套件**整段 skip**（`联机端到端` / `联机 PlayScreen`） |
| ❌ | ✅ | **667 passed / 2 failed / 21 skipped** | 21 条 skip = 10 条图标/字体/设置 + 11 条音频 |
| ❌ | ❌ | **591 passed / 2 failed / 23 skipped** | 在上面基础上再 +2 条联机 skip |

两条恒等式把三个前置条件钉死：**694 = 618 + 76**、**667 = 591 + 76**（当轮基数；现行"全齐"= 709）。

**694 的三个前置条件**：

1. **`client/assets/` 齐备** —— 用**你自己的** `Graphwar.pck` 解（`tools/extract_assets.py` + `client/src/audio/extract_audio.py`），
   两个脚本的默认 pck 路径是**仓库根的上一级**，在子目录/克隆里跑要显式 `--pck <你的 Graphwar.pck>`；
   **音频必须"先解、再 `--import`"**（顺序反了 `GwAudio` 仍报缺资产，**11 条**音频类断言全变 SKIP）。
2. **`cargo build --offline --example serve_match`** —— 缺它少 76 条（`618` 而不是 `694`）。
3. 用 `run.sh` 自己的 `XDG_DATA_HOME/CACHE/CONFIG` 默认值。**手改会把 `user://replays` 指到空目录**，
   回放列表那条断言直接红（历史记录里的 `693 passed, 1 failed` 就是这么来的）。

**没有 `client/assets/` 时应该期待什么数**：

* **`failed == 2`，且这 2 条逐字固定**（本机四组里重复出现，与 `.so` 版本无关）：
  1. `点 Skins 页的第 -1 号皮肤 ⇒ Faces/Hats 42 格立刻换成它（实际 42）`
     —— 皮肤图标取不到 ⇒ 找不到可点的格子 ⇒ `target` 停在 **`-1`**；
     括号里的"实际 42"是**格数**，不是"通过"（`target >= 0` 那一半没过）。
  2. `Cancel = 194 宽 / Start = 218 宽（真帧；实际 184 / 167）` —— 缺字体资产 ⇒ 量出来 184 / 167。
* 这两条**不是回归**：用未改动基线对拍过（`530/2/25`，两条**逐字相同**）⇒ **基线自带 + 依赖真资产**。
* **`passed` 数会随客户端版本漂移**（历史文档里的 `606 / 2 / 23` 是旧版本读数，
  当前 HEAD 复现不出来：实测是 `591/2/23` 或 `667/2/21`）⇒ **别拿 `passed` 当判据**，
  判据只有"`failed == 2` 且就是上面这两条" + "skip 里全是资产/联机"。
* 退出码：`failed > 0` 时 `run.sh selftest` **返回 1**（`694`/`618` 时返回 0）。

> **现读数（2026-11-10 实测，`agent/lan-lobby-flake`）**：四个前置条件齐备时是
> **`SELFTEST 717 passed, 0 failed, 1 skipped`**、`rc=0`（`716 → 717` 多的是 §10.3.4 那条
> "LAN 发现隔离成功"；上面表里的 694/618/667/591 是 2026-11-07 的**当轮**基数，
> `passed` 随客户端版本漂移，**别拿它当判据**）。
> 这个读数**必须可复跑** —— 同一条命令两次给出不同结果的护栏等于没有护栏，
> 所以"确定性"单独立了一节：**§10.3.2**（根因、修法、连续 10 次的原始记录）。

**复跑（三分钟）**：

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/<topic>/target
python3 tools/extract_assets.py --pck <你的 Graphwar.pck>        # → client/assets/
python3 client/src/audio/extract_audio.py --pck <你的 Graphwar.pck>   # 音频先解
cargo build --offline --example serve_match                     # 缺它 694 → 618
cargo build --release --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import                      # 解完资产必须再 import 一次
bash client/run.sh selftest                                     # 期望 SELFTEST 694 passed, 0 failed
```

### 10.3.2 `selftest` 的**确定性**：同一条命令必须给出同一个结果（2026-11-09 新增）

> 一句话：`714 / 0 / 1 skipped` 这个读数以前**不是可复跑的** —— 同一台机器、同一条
> `bash client/run.sh selftest` 会偶发 `704/4`、`697/1`。护栏一旦不确定，
> "红了"就分不清是真回归还是骰子。本节记：根因（带证据）、修法（**只动"等什么、
> 什么时候起、什么时候退"，判据一条没改**）、以及复跑记录。

#### (1) 症状

上游实测（原文）：

| 现象 | 备注 |
|---|---|
| `SELFTEST 704 passed, 4 failed, 1 skipped` | 与 `714/0/1` 是同一条命令 |
| `SELFTEST 697 passed, 1 failed, 1 skipped` + `FAIL 真服务端：开局那一批到了（障碍 0、SoldierInfo 4）` | 失败项名字是关键线索 |
| 干净重跑又回到 `714 / 0 / 1 skipped` | ⇒ 不是回归 |

本机（2026-11-09，工作树 `agent/selftest-flake`）**连续 10 次没有复现**，全是 `714/0/1`
（原始输出见 (4)）。不复现 ≠ 没问题 —— 把"骰子"换成旋钮之后，那条失败可以**确定性重放**：

```bash
GW2_SELFTEST_MATCH_SEED=89 bash client/run.sh selftest
# SELFTEST 696 passed, 1 failed, 2 skipped
#   FAIL 真服务端：开局那一批到了（障碍 0、SoldierInfo 4）
# [gw2/selftest] 真服务端：开局那一批没到（seed=89）：serve_match 日志尾部（…/serve_match_4_1_s89.log）
#     | SEED 89
#     | LISTEN 127.0.0.1:44553
#     | ACCEPT 127.0.0.1:58518
#     | RECV NewConnectionRequest
#     | RECV NewPlayerRequest
#     | PLAYER_INFO player_id=229 connection_id=228
#     | RECV GameStartRequest
#     | STARTED obstacles=0 players=4 order=[229, 230] bots=[] ai=level1 budget=Some(25.0)
```

⇒ 失败串与上游**逐字相同**；服务端自己的日志说明 `obstacles=0`：
**它真的一个障碍都没生成**，不是"到了没收到"。第 2 条 skip 是
`_test_play_online_real_server` 里"障碍 > 0"那条在 0 障碍局面下显式跳过的（它本来就这么写）。

#### (2) 根因（三条，都有证据）

**① 关卡种子是骰子 —— 决定性的一条（观测）。**
`src/graphwar_game_server.rs::run` 的默认种子来自 `seed_from_os()`，所以**每局障碍布局都不同**。
`_test_play_online` 早就钉了 `GW2_MATCH_SEED`，但 `_test_net_real_server` **没钉**。
用 `examples/serve_match` 配一个只做握手的客户端，扫 **400 个种子**（`GW2_MATCH_SEED=<n>`）：

```text
14 个种子（3.5%）一个障碍都不生成：
  89, 110, 119, 146, 173, 197, 209, 228, 244, 301, 305, 313, 352, 387
障碍数分布：min = 0，max = 37
```

而这条用例的判据 `_server_started()` 要求 `AddObstacle > 0` ⇒ 抽到这类种子就**必红**
（括号里的"SoldierInfo 4"是恒定的：士兵固定 4 名）。
正对照：真·随机种子（不设 `GW2_MATCH_SEED`）跑 60 次，**2 次**（3.3%）拿到 `AddObstacle == 0`
—— 与 14/400 一致。⇒ **单次约 3.5% 必红，这就是"同一条命令两次不同结果"。**

顺带排除一条嫌疑（**负结果，观测**）：同一批 60 次随机里，开火后的
`EffectInfo / QueueDestroyObstacle` **60/60 都有** ⇒ 上游那次 `704/4` 不是"命中链路偶发打空"
造成的；这一档本机没复现，也没找到能产生它的机制，**如实记为未知**。

**② 端口是"先占后放"（推断 —— 机制成立，本轮未撞上实例）。**
`_free_port()` 让内核挑一个端口再立刻 `stop()` 放掉。从"放掉"到服务端
`TcpListener::bind` 之间，这个端口可能已经被别人拿走（包括本进程自己的出站连接 ——
临时端口同段）⇒ 服务端 `SERVER_ERR` 退出，而自检只看到"连不上"，看不出为什么。

**③ 起/停都没有握手，而且服务端的话留不下来（观测：代码里到处是定长 sleep）。**
原来 `OS.create_process()` 之后就 `await create_timer(0.25)` / `0.2` 赌它 bind 好了；
`OS.kill(pid)` 之后立刻往下走（进程可能还没死、端口还占着）。
更要紧的是 `OS.create_process()` **不能重定向 stderr** ⇒ 服务端的
`SEED / LISTEN / ACCEPT / RECV / SERVER_ERR` 一行都留不下 ——
这就是为什么上游只看到一句 `FAIL …（障碍 0、SoldierInfo 4）` 却查不下去。

#### (3) 修法（`client/tools/selftest.gd`，判据一条没改）

新增一组 helper，把"起一台真服务端"做成**有握手、有日志、有 teardown**：

| helper | 做什么 | 替掉了什么 |
|---|---|---|
| `_spawn_server(bin, seed)` | 地址交给内核（`127.0.0.1:0`），端口从服务端**自己打的** `LISTEN <addr>` 读回；stderr 落 `user://serve_match_logs/serve_match_<godot pid>_<序号>_s<seed>.log`；**等到 LISTEN 才返回**（最多 10 s，实测 30 ms） | `_free_port()` + `OS.create_process()` + 定长 sleep |
| `_kill_server(pid)` / `_pid_alive(pid)` | `OS.kill` 之后**等它真的退出**（最多 4 s；Linux 读 `/proc/<pid>/stat` 的状态位，僵尸(`Z`)算已退出） | 裸 `OS.kill(pid)` |
| `_dump_server_log()` | 红了打印服务端日志尾部（`SEED/LISTEN/ACCEPT/RECV/SERVER_ERR`） | 什么都没有 |
| `_match_seed()` | 所有起真服务端的用例统一钉 `ONLINE_TEST_SEED = 8`；`GW2_SELFTEST_MATCH_SEED=<n>` 可覆盖（诊断用） | 只有 `_test_play_online` 钉了 |
| `_prune_server_logs()` | `run()` 开头清空日志目录 | 残留状态跨运行 |

另外两处：

* **LAN 广播**：从"发一条 UDP + 定长等 120 帧"改成"一边等一边重发同一条"
  （UDP 无连接，而原版房主本来就是**周期性**广播，`docs/24` §7.2）；
  同一个 `address` 只占一行（`GwLanDiscovery::_ingest` 按 address 去重）。
* **连接重试**里的 `await create_timer(0.25/0.2)` 换成 `await _frames(2)`：
  服务端已被确认 `LISTEN`，不必再用墙钟赌它 bind 好了。

`ONLINE_TEST_SEED = 8` 是**实测挑的**（不是"跑通了就算"）：10 个障碍、4 名士兵都不在障碍里、
`0.00` 那一发能命中（服务端日志 `STARTED obstacles=10`）。两个联机用例共用同一份布局。

#### (3b) 修的时候自己踩到的第二个坑：「就绪信号」读到了**半行**（如实记）

第一版改完（读 `LISTEN` 再连）**自己在连跑里红过 1 次**（那批 10 次里的第 8 次）：

```text
run  8 | rc=1 | SELFTEST 681 passed, 2 failed, 1 skipped | 13.1s
  FAIL 真服务端：找到一个空闲端口
  FAIL 真服务端：从 Local 列表点 Join 真的连上并完成握手
[gw2/selftest] 真服务端：pid=428 LISTEN 127.0.0.1: seed=8 日志=…/serve_match_386_1_s8.log
[toast] 地址要写成 host:port                 ← ×6：addr 成了 "127.0.0.1:"
[gw2/selftest] 真服务端：连不上：serve_match 日志尾部（…/serve_match_386_1_s8.log）
    | SEED 8
    | LISTEN 127.0.0.1:46203                  ← 同一份日志**稍后**已经写全了
```

根因：`eprintln!("LISTEN {}", local)` 那一行**不是一次性可见的** —— 第一次读到的内容停在
`127.0.0.1:`，端口那几位还没落地，于是 `int("")` = 0、`addr` 成了 `127.0.0.1:`。
（这也正好说明：**旧的"发一条 + 睡 0.25 s"其实一直在赌同一件事**，只是赌错了没人知道。）

修法：**只认以 `\n` 结尾的完整行**（服务端每行都以 `\n` 收尾 ⇒ 没有 `\n` 就是还没写完），
并且端口那一段必须 `is_valid_int()` 且 `> 0`，否则继续等。

顺手还收拾了一个噪音坑：`OS.is_process_running()` 一旦把子进程回收掉，**再问同一个 pid**
就会打 `ERROR: The process 428 does not exist or is not a child of the calling process.`
（混在自检日志里很像真失败）。现在轮询走 `_pid_alive()`。
⚠️ 这条也有坑：`FileAccess.get_file_as_string("/proc/<pid>/stat")` **永远是空串**
（procfs 的 `size` 报 0，Godot 按 size 读），必须 `FileAccess.open()` + `get_line()`（实测）。

这两条是同一类错误：**「就绪/存活」不能"读一眼文件"就下结论，要读到一个完整的记录。**

**没有做的事**：没删任何断言、没把失败改成跳过、没有加大 sleep。
`grep -c '_expect('` 前后都是 **701** 处（运行时 **714** 条），`grep -c '_skip('` 前后都是 **22** 处。

#### (4) 复跑记录（本机，2026-11-09）

循环脚本（就是它跑的）：

```bash
for i in $(seq 1 20); do                     # 修复前/第一版各跑 10 次，第二版跑 20 次
  bash client/run.sh selftest > run$i.log 2>&1
  echo "run $i rc=$?"; grep -E '^SELFTEST ' run$i.log | tail -1
done
```

修复**前**（HEAD `e150ccd`）：10/10 全绿，**没复现** ——

```text
run  1 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  2 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.9s
run  3 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  4 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.9s
run  5 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.9s
run  6 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  7 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  8 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  9 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.9s
run 10 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.9s
```

第一版修法（只换起/停，**还没**修 (3b) 那半行）：**9 绿 1 红** ——

```text
run  1..7 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4–10.1s
run  8    | rc=1 | SELFTEST 681 passed, 2 failed, 1 skipped | 13.1s   ← (3b) 那半行
run  9..10| rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
```

第二版（只认完整行 + `/proc` 判活）：**20/20 全绿** ——

```text
run  1 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run  2 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run  3 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  4 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  5 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  6 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  7 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  8 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run  9 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run 10 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run 11 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.4s
run 12 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run 13 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run 14 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run 15 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run 16 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.7s
run 17 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run 18 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run 19 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
run 20 | rc=0 | SELFTEST 714 passed, 0 failed, 1 skipped | 9.8s
```

判据是"`714 / 0 / 1 skipped` 且 `rc=0`"，不是"看着像好了"；跑完 `ps` 里**没有**残留的
`serve_match` / Godot 进程（teardown 真的等到了退出）。


**确定性的第二个判据**：同一个坏种子必须**稳定**给出同一个结果 ——

```bash
GW2_SELFTEST_MATCH_SEED=89 bash client/run.sh selftest    # 每次都 696/1/2 + 同一条 FAIL
```

#### (5) 红了怎么读

1. 先看 `[gw2/selftest] 真服务端：pid=… LISTEN … seed=… 日志=…` 里的 **seed** 与 **端口**；
2. 失败时自检会自动打 `serve_match 日志尾部`：`LISTEN` 有没有？`ACCEPT` 有没有？
   `STARTED obstacles=N` 是多少？`SERVER_ERR` 有没有？
3. 想换一份布局复跑：`GW2_SELFTEST_MATCH_SEED=<n> bash client/run.sh selftest`；
4. **别用 `--script` 喂 `client/tools/*.gd` 里的 helper**（`selftest.gd` /
   `plane_on_screen.gd` / `screenshots.gd` 都不是 `extends SceneTree`）——
   那会弹 ALERT 并留下僵尸 Godot 实例，跟本节的坑叠在一起更难查。

**仍然存在的时序依赖（如实记，未改）**：`_test_audio` 的几条断言按**真实时间**
（0.25 / 0.35 / 0.45 s）取 `AudioEffectCapture` 的窗口（`_capture_rms/_capture_peak/_capture_humm`）——
headless 下按帧数等会读到 0，所以那里必须用墙钟。机器**极端**过载时它们理论上仍可能抖；
本轮 20 次里没出现过，真出现时按上面 (5) 的办法先确认是不是它。

### 10.3.4 `selftest` 的第二条既有 flake：LAN 大厅**成对红**（2026-11-10 新增，已修）

> **一句话**：那两条红只有**一个**根因 —— LAN 大厅的**房间表里多了一行**。
> 那一行是**外部输入**（`App._install_net()` 启动就绑原版的 `UDP 42968`），
> 而两条断言都以"房间表是空的"为前提，用例自己从没把这个前提拿回来。
>
> 修法两条，**判据一个字没改、断言一条没删**（`_expect(` 703 → 704，多的那条是"隔离成功"）：
> 1. **前提由用例自己控制**：`_isolate_lan_discovery()` = 清表 + 换私有端口 + **断言这次隔离成功**；
> 2. **底栏那个 " Join IP " 按 `input_text` 找**，不按节点名找 —— 房间条目里也有一个
>    叫 `JoinButton` 的节点，`find_child("JoinButton")` 给的是**深度优先的第一个**。
>
> 证据档：根因链是 **观测**（确定性复现 3/3 + 逐帧快照）；
> "上游那两次红具体是谁在广播"是 **未知**（没抓到），候选与排除写在 (6)。

#### (1) 症状与上游记录

| 项 | 值 |
|---|---|
| 现象 | 连跑 `bash client/run.sh selftest`，LAN 大厅两条**成对**变红，总数差恒为 `+2` |
| 上游（第十七轮 84 号 A/B，基线 `7323124`） | 连跑 4 次第 **3** 次红；本轮 9 次第 **3、7** 次红 ⇒ **不是当轮引入的** |
| 红的两条 | `本地房间列表初始为空（实际 1）` / `底部 Join IP 宽度 = 屏宽×0.2（LobbyButtonContainer.gd，期望 12.8）` |
| 本机裸跑（`CG@38cb827`，工作树 `agent/lan-lobby-flake`） | **10 次全绿**（(4) 的原始表）⇒ 这条**要触发条件**，不是"改坏了" |

⚠️ 与本文件 §10.3.3 那一档（`632/7`，78 条根本没跑）**不是同一件事**：那个掉的是**整组**（两组"真服务端"
各跑 2 条就 `return`，另 6 条是守卫块静默），根因是 `user://` 不可写、**已在 §10.3.3 定因并做成显式**
（`@@SELFTEST_COVERAGE missing=0` 才是判据）；本条的 signature 是**恰好 2 条**且名字固定。

#### (2) 根因（观测）：房间表里多了一行 ⇒ 两条断言同时红

`LanLobby` 的 Local 页每帧重投影一次（`_sync_list` → `_wanted_rows("local")`），数据源只有两个：

```
net.local_rooms()            # GwLanDiscovery：UDP 42968 收到的 9 字段 JSON 行
App.local_room_row()         # 自己开的那间（create_game 才有）
```

而 `App._install_net()`（`client/src/app/app.gd:127`）在**进程启动时**就绑了原版端口
`GwLanDiscovery.DEFAULT_PORT = 42968`。⇒ **任何**在这台机器上广播房主的进程，
只要在用例走到 LAN 大厅之前（进程第 ~2 秒）灌一条合法 JSON（房间 TTL 12 s），
房间表就有 1 条，Local 页就画 1 行。

两条断言的依赖关系**不是"同一个尺寸/同一次页面切换"，而是"同一个列表"**：

| 断言 | 怎么被那一行弄红 |
|---|---|
| `本地房间列表初始为空（实际 1）` | 直接数 `GraphwarLanVBox` 的 `HBoxContainer` 子节点 ⇒ 1 |
| `底部 Join IP 宽度 = 屏宽×0.2（期望 12.8）` | 量的是 `lobby.find_child("JoinButton")`。**房间条目里也有一个 `JoinButton`**（`game_room_entry.tscn` 的 `JoinButtonContainer/JoinButton`），而 `find_child` 是**深度优先的第一个** ⇒ 列表有行时拿到的是**行里**那个（`custom_minimum_size.x == 0`，不受 `LobbyButtonContainer.gd` 管） |

逐帧快照（修**之前**的代码 + 一条注入的广播行，原始输出）：

```text
@@DIAG boot      f=2  win=(64, 64) vp=64 mode=local rows=0 items=[] join_min=12.80 first_join=MarginContainer/VBoxContainer/MarginContainer/HBoxContainer/JoinButton
@@DIAG signal rooms_changed(local) f=3 rooms=1 pkts=1 [{ "room_name": "Stray Room", … "address": "127.0.0.1:1" … }]
@@DIAG lan_local f=31 win=(64, 64) vp=64 mode=local rows=1 items=["Stray Room"] join_min=0.00  first_join=MarginContainer/VBoxContainer/ScrollContainer/GraphwarLanVBox/HBoxContainer/JoinButtonContainer/JoinButton
@@DIAG geom_join f=31 （同上）
SELFTEST 714 passed, 2 failed, 1 skipped
  FAIL 本地房间列表初始为空（实际 1）
  FAIL 底部 Join IP 宽度 = 屏宽×0.2（LobbyButtonContainer.gd，期望 12.8）
```

`join_min` 从 **12.80 → 0.00**、`first_join` 从**底栏**变到**列表行里**，就是第二条红的原因；
`rows` 从 **0 → 1** 就是第一条红的原因。**一行 ⇒ 两条**，这就是"成对"。

#### (3) 确定性复现（3/3，修之前）

把"外部广播"做成旋钮（就是 §10.3.2 用 `GW2_SELFTEST_MATCH_SEED=89` 造"0 个障碍"的同一套路）：
启动时往自己的 LAN 监听端口塞一条房主广播（`GwWireSamples.broadcast` 造的 9 字段 JSON，
与 `docs/24` §1.1 抓包同形状）。**判据一条没改的基线代码**连跑 3 次：

```text
PREFIX-inject run 1 | rc=1 | SELFTEST 714 passed, 2 failed, 1 skipped
      FAIL 本地房间列表初始为空（实际 1）
      FAIL 底部 Join IP 宽度 = 屏宽×0.2（LobbyButtonContainer.gd，期望 12.8）
PREFIX-inject run 2 | rc=1 | SELFTEST 714 passed, 2 failed, 1 skipped   （同上两条）
PREFIX-inject run 3 | rc=1 | SELFTEST 714 passed, 2 failed, 1 skipped   （同上两条）
```

⇒ "成对红"100% 可重放，**不是骰子**；上游看到的是"概率"，因为**广播源在不在**才是概率。

#### (4) 本机裸跑记录（修之前，10 次全绿 —— 如实记"没复现"）

```text
run  1..10 | rc=0 | SELFTEST 716 passed, 0 failed, 1 skipped | 9.4–10.3s
```

这 10 次里 **3 次**（run 1 / 2 / 9）`UDP 42968` **绑不上**（`LAN 发现不可用：… 绑定失败（2）`），
另 7 次绑上了 —— 绑上了但**没有广播源**，所以没复现。
`ss -lunpe` 显示这台机器上 `*:42968` 曾被**另一个 DSH 子进程**（cgroup
`dsh-subprocess-…`，uid 1000）占着 ⇒ 42968 在这个环境里本来就是**抢**的。
⇒ 本机复现率 0/10 是"没有广播源 + 端口被抢"的巧合，**不能当成没问题**（(3) 已证明）。

#### (5) 修法（`client/tools/selftest.gd`；判据、条数、期望值全部不动）

| # | 改什么 | 为什么 |
|---|---|---|
| ① | 新增 `_isolate_lan_discovery(tag)`：`lan.stop()`（**会清空房间表**）→ `lan_listen(0)`（私有端口）→ **断言** `err == OK && is_listening() && room_count() == 0` | "房间表是空的"是这组用例的**前提**，必须由用例自己控制；私有端口外部的周期性广播再也到不了。多出来的那 **1** 条断言就是"隔离成功"，隔离失败会**红**，不会静默 |
| ② | `_test_lan_lobby()` 第一行 `await _isolate_lan_discovery("联机大厅")` | 把①接上去。**在进大厅之前**做，`show_global/show_local` 的语义不受影响 |
| ③ | 新增 `_lobby_join_ip_button(lobby)`：按 `input_text == GwCommands.JOIN_IP`（`"join_ip"`）找底栏按钮 | 修的是**选哪个节点**，不是判据：断言原文写的就是"**底部** Join IP 宽度"，判据仍是 `custom_minimum_size.x == 屏宽×0.2`（期望值一个字没改） |
| ④ | `GW2_SELFTEST_INJECT_LAN_ROOM=1/2` + `GW2_SELFTEST_DIAG=1` | 旋钮与诊断（只读）。`=1` = 启动时塞一行（**回归判据**：修好之后不许再红）；`=2` = 隔离**之后**再塞一行（**正对照**：断言必须还红）；`DIAG` 打逐帧快照 |

**没有做的事**：没删断言、没把失败改成跳过、**没有加大任何 sleep**、没动 `client/src/**`。
`grep -c '_expect('` **703 → 704**（+1 = 隔离断言）、`grep -c '_skip('` **22 → 22**；
运行时 `SELFTEST 716 passed, 0 failed, 1 skipped` → **`717 passed, 0 failed, 1 skipped`**。

#### (6) 那一行到底是谁送的（触发源：**未知**，候选与排除如实记）

| 候选 | 档 | 说明 |
|---|---|---|
| `tools/gate5_serve.py` | **观测**（代码） | 默认往 `127.0.0.1,255.255.255.255:42968` 每 ~1 s 灌一条 `GameInfo`，`--broadcast-for 0` = **不停**。它是本仓库里**唯一**会周期性广播房主的脚本（gate5 那几轮实测都用它） |
| 同机另一个客户端 / 另一个 agent 的进程 | **观测**（`ss`） | `*:42968` 确实被另一个 DSH 子进程占用过；任何绑上它的客户端都在收同一条广播 |
| 局域网里**原版真机**的房主 | **未知**（未验证） | 原版广播走 IPv6 组播 `ff02::1`（`docs/24` §7.2、`src/graphwar_backend.rs:170`），我们监听的是 IPv4 `*` ⇒ **大概率收不到**。这一条不做结论 |
| `examples/serve_match` 自己广播 | **排除**（观测） | `examples/serve_match.rs` 里没有任何 UDP/广播代码；`_test_net_real_server` 那条广播是**自检自己发的**，而且发往**私有端口**（`net.lan_port`），从不发 42968 |
| 自检自己发的包 | **排除**（观测） | `_test_net_lan_discovery` / `_test_net_real_server` 都 `set_dest_address("127.0.0.1", net.lan_port)`（私有的临时端口）；`client/tools/*.gd`（14 个 headless 脚本）里 0 处 `42968`/`set_dest_address` |

⇒ 上游那两次红具体是哪一份广播**没抓到**，所以标 **未知**；但"有广播 ⇒ 必定成对红"是 (3) 的 3/3。

#### (7) 修完的复跑记录

① **连跑 20 次全绿**（本机，`bash client/run.sh selftest`）：

```text
run  1..20 | rc=0 | SELFTEST 717 passed, 0 failed, 1 skipped | 9.5–10.1s
```

② **原始触发条件开着**也全绿（`GW2_SELFTEST_INJECT_LAN_ROOM=1`，修之前是 3/3 红）：

```text
inject-run 1..5 | rc=0 | SELFTEST 717 passed, 0 failed, 1 skipped
```

③ **正对照：断言还有牙齿**（`=2`，隔离之后再塞一行）：

```text
@@DIAG lan_local … rows=1 items=["Stray Room"] join_min=0.00 join_ip_min=12.80 first_join=…/GraphwarLanVBox/…/JoinButton
SELFTEST 716 passed, 1 failed, 1 skipped
  FAIL 本地房间列表初始为空（实际 1）        ← 只有这一条；"Join IP 宽度"不再跟着红
```

⇒ 既证明"多一行仍然会红"（护栏没被削弱），也证明**成对**这件事本身被去掉了：
第二条现在量的是它自己声明的那个节点。

④ **dispatch/90 的关键判据**："先连跑 14 个 headless，再跑 selftest，第一次就必须全绿"：

```text
HEADLESS_TOTAL passed=1923 failed=0 scripts=14        ← 与基线 1923 一致
SELFTEST_AFTER_HEADLESS rc=0 | SELFTEST 717 passed, 0 failed, 1 skipped
```

#### (8) 红了怎么读（这一组）

1. 先看 `FAIL` 里有没有 `本地房间列表初始为空（实际 N）`：有 ⇒ **房间表里真有 N 条**，
   加 `GW2_SELFTEST_DIAG=1` 重跑一次，`@@DIAG lan_local` 那行的 `items=[…]` 就是**房间名**、
   `rooms=`/`pkts=` 是表里的条数与收到过的包数；
2. 想知道"是不是外部广播"，看 `@@DIAG boot` 的 `lan=on:<port>`：`42968` = 绑到了原版端口
   （外部能打进来），其它端口 = 已经隔离；
3. 想**确定性地**验证护栏还在：`GW2_SELFTEST_INJECT_LAN_ROOM=2 bash client/run.sh selftest`
   ⇒ 必须看到 `本地房间列表初始为空（实际 1）` 这一条红（其它不许红）。

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
两份绑定都过；feature 关闭时零依赖、`selftest` **694 不变**（两份 `.so` 都实测过，
**前提是 §10.3.1 那三条前置条件**）。

**不成立 / 没做**：

- 只迁了 **2 个类**（`GW2Rebuild` / `AxisDrawer`）；12 个 GDExtension 类里其余 10 个没动；
- `client/gw2rebuild.gdextension` 的 `compatibility_minimum` 还是 4.1（见 §10.4 差异 3）；
- 没有 vendor、`Cargo.lock` 没提交；
- 手写层的**报文 / C ABI / broker** 那些（`capi.rs` / `protocol.rs` / `broker.rs`）与
  gdext 无关，仍然留在 `_scaffold/`；
- §8 的"下一步 1"（把 `parse`/`calculate` 暴露成带参数与返回值的方法）**已经过时**：
  那件事在 `docs/116`/`120`/`126`/`138` 里做完了，本节就是它的 gdext 版。

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
