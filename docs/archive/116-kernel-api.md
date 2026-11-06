> **2026-11-06 落地记录（父 agent）**：§7.5 那三步已经**在真仓库的 `client/` 上执行**——
> `client/gw2rebuild.gdextension` 已提交，`.so` 由 `cargo build` 生成并已加进 `.gitignore`。
> 本机复核：`bash client/run.sh selftest` = **93 passed, 0 failed**，版本行 = **`client 0.1.0 · kernel on`**。
> ⚠️ 一个不明显但会卡人的点：**新增 `.gdextension` 后必须让 Godot 重新扫描一次**
> （`godot --headless --path client --import`），否则它会静默地不加载，版本行仍显示 `kernel off`。

# 116 · 内核 API：把采样器真正接到 GDScript

状态：**现行**。本文只讲一件事：GDExtension 类 `GW2Rebuild` 现在向 GDScript 暴露了
`path_calculate`，界面画的弹道**就是** `src/graphwar_path_calculator.rs` 那个等弦长二分采样器的输出。

> 前置阅读：[`108-client-rebuild.md`](108-client-rebuild.md) §2.3（接缝契约）与 §7（占位清单）、
> [`04-path-sampling.md`](04-path-sampling.md)（采样器权威结论）、
> [`05-shot-pipeline.md`](05-shot-pipeline.md)（修正常数与 `flip`）、
> [`91-bisection-loop.md`](91-bisection-loop.md)（二分修复史）。
> 本文不重述算法；算法层面的结论以那几篇为准。

---

## 0. 一页结论

| 项 | 结果 |
|---|---|
| 新方法 | `GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float) -> Dictionary` |
| 契约来源 | `client/src/core/core_bridge.gd` 文件头（逐字）+ `docs/108` §2.3 |
| 返回 | `{ ok: bool, points: PackedVector2Array, offset: float, error: String }` |
| 坐标系 | **棋盘（平面）坐标**，不是画布像素 —— 依据见 §2 |
| 用的是不是真采样器 | 是。`parse → correction → shifted_tokens → path::calculate`，参数全部取既有常量 |
| `GwCore.calculate_path()["source"]` | **`preview` → `kernel`**（§7.3 有前后对照） |
| `cargo test --offline` | **205 → 218**（+13，全绿） |
| `bash client/run.sh selftest` | **93 passed, 0 failed**（未改任何断言；见 §7.4） |
| 改动的文件 | `src/api.rs`（新增）、`src/gdext.rs`（+3 行）、`src/lib.rs`（+1 行）、`docs/116`（本文） |
| `client/` 改动 | **0 字节**（含 `client/` 下没有新增 `.gdextension`，见 §8.1） |
| 仍未做 | 命中/掩膜接口、ODE（微分方程）模式、`client/` 侧的扩展描述符 |
| debug / release | 两份 `.so` 的端到端结果**逐位相同**（2471 / 5295 点，弦长一致） |

三条最容易出错的点，先写在前面：

1. **坐标系**：返回棋盘坐标（`|x| ≤ 25`、`|y| ≤ 15`）。若误当像素坐标返回，整条弹道会偏 500/300。
2. **`GDExtensionPropertyInfo.class_name` / `hint_string` 不能传 NULL** —— 4.7.2 会直接解引用，实测 SIGSEGV。
3. **`variant_set_keyed` 不能用** —— 4.7.2 上实测崩溃；造 `Dictionary` 只能用 `variant_call("set")`。

---

## 1. 契约逐字对照

契约原文（`client/src/core/core_bridge.gd` 第 14–17 行）：

```text
## 接入真内核时只需要在内核侧加一个方法，约定如下（推荐签名）：
##     GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float)
##         -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
## 参数与返回值与本文件 `calculate_path()` 完全一致，调用方无需改动。
```

| 契约项 | 实现 | 档位 |
|---|---|---|
| 方法名 `path_calculate` | `src/api.rs::register_path_method` | 逐位验证（`get_method_list` 实测，§7.3） |
| 4 个参数、顺序、类型 | `String, float, float, float`（`GDExtensionPropertyInfo` 里 `type = 4/3/3/3`） | 观测（`get_method_list` 打印出 `expr:String, x0:float, y0:float, direction:float`） |
| 返回 `Dictionary` | `return_value_info.type = 27`（`DICTIONARY`） | 观测（`m.return` 打印 type=27） |
| 键名 `ok/points/offset/error` | `RESULT_KEYS`，顺序固定 | 逐位验证（单测 `result_pairs_shape_is_exactly_the_contract`） |
| `points` 是 `PackedVector2Array` | 默认构造 + `append` 逐点 | 观测（GDScript 里 `typeof(res["points"]) == 35`、`pts is PackedVector2Array`） |
| 调用方零改动 | PlayScreen `fire()` 直接吃到内核弹道 | 观测（§7.3 的 PlayScreen 逐点比对） |

客户端读法（`core_bridge.gd:92-98`）决定了两条隐含要求，都满足：

* 只要 `res.ok == false`，客户端就**静默退回预览**（`calculate_path` 末尾的 `return _preview(...)`）
  ⇒ 内核报错不会让界面崩，但也**不会报错给用户看**（`error` 字段目前无人读）。
* `offset` 用 `float(res.get("offset", 0.0))` 读，`NAN` 会被 `format_offset()` 变成空串 ⇒ 出错时传 `NaN` 是安全的。

---

## 2. 坐标系：为什么是棋盘坐标（这一节别跳）

**结论：`points` 用棋盘（平面）坐标，与 `GwCore._preview()` 完全一致。**
档位：**观测**（读调用方源码即可确定，无需推断）。

证据链（三步，都在 `client/` 里，逐行可查）：

1. `client/src/screens/play_screen.gd:454` —— 传进去的是 `active_soldier.pos`，
   而这个 `pos` 与 `GwPlaneView.SOLDIER_RADIUS = 0.8` 同量纲、被 `board_to_screen()` 换算；
   士兵坐标本身来自 `game_state.gd`，取值区间是 `|x| ≤ 25`、`|y| ≤ 15`。
2. `play_screen.gd:460` —— 拿到 `pts` 后**不做任何变换**就 `_plane.set_path(pts, impact, true)`。
3. `client/src/play/plane_view.gd:46-49` —— `board_to_screen(p)` 才做
   `Vector2(size.x*0.5 + p.x*s, size.y*0.5 - p.y*s)`；`s = min(w/50, h/30)`。
   **换算发生在绘制层，不在内核返回值上。**

反面证据（写错了会怎样）：如果内核返回 `px = 20x + 500`、`py = 20y + 300` 的像素坐标，
`board_to_screen()` 会再乘一次 scale 并加一次中心偏移 ⇒ 弹道整条飞出画布。
`docs/05` §5 也早写过"否则会把像素坐标和游戏单位混在一起"。

实测旁证：`GwCore.calculate_path("sin(x)", Vector2(5,2), 1.0)` 的 `points[0]` 就是 `(5.0, 2.0)`，
点数 2471，弦长 0.0090–0.0105 —— 与 `src/graphwar_path_calculator.rs` 单侧的值一模一样（§7.3）。

**顺带一条界面侧的观察（不是 bug，是接上内核后数字会变）**：
`_preview()` 的修正常数恒用 `c = y0 − f(x0)`（不问方向），而内核用 `c = y0 − f(±x0)`。
所以 team 1（向左）开火时，`OffsetLabel` 从今天起显示的数会和以前不同 —— 内核那个才是对的
（`docs/05` §2、`docs/105`）。这条属于**预期变化**，不是回归。

---

## 3. Godot ABI：手写 FFI 怎么造 `Dictionary` 和 `PackedVector2Array`

本仓库不用 godot-rust（`Cargo.toml` 零依赖），`gdext_class.rs` 里 `ping()` 只证明了
"能注册一个无参无返回的方法"。真返回 `Dictionary` 需要三样新东西：**类型化的参数/返回值信息**、
**把内置类型包成 Variant**、**在内置类型上调用方法**。全部靠 `get_proc_address` 逐个取。

### 3.1 需要的接口（4.7.2 实测全部非空）

```
get_variant_from_type_constructor   variant_get_ptr_constructor   variant_call
get_variant_to_type_constructor     variant_get_type              variant_destroy
variant_get_ptr_destructor          string_name_new_with_latin1_chars
string_new_with_utf8_chars          string_to_utf8_chars          classdb_register_extension_class_method
```

启动日志（`--headless` 的 stderr，逐项 `true`）：

```
[gw2-rebuild] api resolve: from_ctor=true to_ctor=true ptr_ctor=true variant_call=true get_type=true \
variant_destroy=true raw_dtor=true sn_new=true str_new=true str_to_utf8=true register_method=true
[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)
```

缺任何一个都**整体不注册**（宁可让客户端继续用预览，也不要"一半可用"的半成品）。

### 3.2 `call_func` 的真实签名

`GDExtensionClassMethodCall`（`ptrcall_func` 为 NULL 时被调用）：

```c
void (*)(void *method_userdata, GDExtensionClassInstancePtr instance,
         const GDExtensionConstVariantPtr *args, GDExtensionInt arg_count,
         GDExtensionVariantPtr ret, GDExtensionCallError *error);
```

两条容易忽略的：

* `args[i]` 是**真正的 `Variant *`**（不是裸 `String`/`double`）；
* `ret` 指向一块**未初始化的 Variant 槽**，必须用 `get_variant_from_type_constructor` 做
  **placement new**（`from_ctor(ret, &raw_dict)`）。档位：观测（§3.3 的实战 + GDScript 侧读回成功）。

### 3.3 `GDExtensionPropertyInfo`：`class_name` / `hint_string` 不能是 NULL

字段序（Godot 4.x，实测与本实现一致）：

```c
typedef struct {
    GDExtensionVariantType type;      /* i32 */
    GDExtensionStringNamePtr name;
    GDExtensionStringNamePtr class_name;   /* ← 不能 NULL */
    uint32_t hint;
    GDExtensionStringPtr hint_string;      /* ← 不能 NULL */
    uint32_t usage;                        /* PROPERTY_USAGE_DEFAULT = 6 */
} GDExtensionPropertyInfo;
```

**踩坑实录**（做原型时真撞上的）：第一版把 `class_name`、`hint_string` 传 `NULL`，
`classdb_register_extension_class_method` 里直接 SIGSEGV：

```
handle_crash: Program crashed with signal 11
[3] libgw2proto.so+1930b  gw2proto::register_method
```

修法：给它们指向**默认构造的空 `StringName` / 空 `String`**（各 8 字节零，Godot 里 `_data == nullptr`
就是合法的空值），并且这两个槽要活到注册调用返回。档位：观测。

`arguments_info` 必须给（否则 `get_method_argument_count()` 是 0，GDScript 传参会被判错），
`arguments_metadata` 给 4 个 `0`（`NONE`）。

### 3.4 造 `Dictionary`：`variant_get_ptr_constructor` + `variant_call("set")`

```text
ctor0 = variant_get_ptr_constructor(DICTIONARY, 0)      // 默认构造，p_args = NULL
ctor0(&raw_dict, NULL)
from_dict = get_variant_from_type_constructor(DICTIONARY)
from_dict(ret, &raw_dict)                               // placement new 进返回值槽
variant_call(ret, StringName("set"), [key_variant, value_variant], 2, scratch, &err)   // 每个键一次
```

`err.error` 为 `CALL_OK(0)`（实测每次都 0），GDScript 侧读回 `{ "ok": true, "n": 42.5 }` 正常。

**为什么不用另外两条看起来更直接的 API**：

* `variant_get_ptr_builtin_method(DICTIONARY, "set", hash)` —— 需要**编译期 hash**（godot-cpp
  在编译时算好传进去），手写 FFI 无从得到这个数；且 hash 不匹配就返回 NULL。
* `variant_set_keyed(self, key, value)` —— 4.7.2 上名字解析得到非空指针，但按头文件推断的签名调用
  **直接 SIGSEGV**（真撞过）。它的实际 ABI 与头文件不一致，**不要用**。

`variant_call` 能对**内置类型**生效是这套做法的关键前提 —— 这一条是实测结论（§7.3 的
`{ "ok": true, "n": 42.5 }` 与 `[(1.5,-2.5),(3.0,4.0)]`），不是从文档推的。

### 3.5 造 `PackedVector2Array`：同样 `ctor0` + `append`

```text
ctor0 = variant_get_ptr_constructor(PACKED_VECTOR2_ARRAY, 0)   // 空数组
from_packed(ret, &raw)                                        // 包成 Variant
variant_get_ptr_destructor(PACKED_VECTOR2_ARRAY)(&raw)         // 立刻放掉裸对象的那份引用
for p in points: variant_call(ret, "append", [Vector2_variant(p)])
```

为什么要"先包成 Variant、再放掉裸对象、之后**只**通过 Variant 追加"：
`from_ctor` 是 COW 拷贝（共享同一块 `CowData`，引用计数 +1）。若之后又去写裸对象，
`CowData::write` 会**只在裸对象那边**触发 copy-on-write，结果写进一片没人看的数组，
返回给 GDScript 的还是空的。（这条是设计时避开的分支，未作为失败案例复现 —— 档位：**推断**，
依据是 Godot 的 COW 语义；实测走的是上面那条安全路径。）

代价（实测，debug 构建）：**20000 次 `append` 约 12.8 ms**（时间全在引擎侧，本 crate 只做搬运）。
一次开火 2500–5300 点，即 ~2–4 ms；对每回合开一炮的节奏没有影响。

精度：Godot 标准（单精度）构建里 `Vector2` 是 **f32**，所以界面拿到的坐标相对精度约 1e-7。
采样本身仍是 f64（`Vec<(f64,f64)>`），损失只发生在导出这一步，不影响 0.01 弦长判定。
档位：观测（弦长统计 0.0090–0.0105，与 Rust 侧同值）。若将来需要 f64 坐标，
只能改成返回 `PackedFloat64Array` 平铺（`[x0,y0,x1,y1,…]`）—— **这会破坏契约**，不在本次范围。

### 3.6 生命周期 / 有没有漏内存

每个调用里创建的 Variant 都用 `variant_destroy` 显式析构，裸 `String`/`PackedVector2Array`
用 `variant_get_ptr_destructor(type)` 析构（两者不是一回事：前者销毁 Variant，后者销毁裸内置对象）。

实测：连续 200 次 `GwCore.calculate_path("sin(x)", (5,2), 1.0)`，
`Performance.MEMORY_STATIC` 前 69160904 B、后 69160904 B，**delta = 0**。
档位：**观测**。局限：只覆盖 Godot 自己的分配器；本 crate 的 Rust 堆没有被单独测（每次调用的
`Vec` 在函数返回时释放，未做长期 RSS 观测）—— 这一小块算 **未知**。

### 3.7 不 panic（硬要求）

跨 FFI 的 panic 会 abort 整个 Godot 进程，引擎接不住。所以：

* 所有错误都走 `ok: false + error`，不用 `panic!`/`unwrap`/索引越界；
* `path_calculate_call` 的整个函数体包在 `catch_unwind(AssertUnwindSafe(...))` 里，
  万一有漏网的 panic，翻成 `{ok: false, error: "internal_panic"}`；
* 单测 `no_panic_on_extreme_inputs` 拿 8 组怪输入（`))))`、`sin(`、`1/0`、`x^999999`、
  `tan(x)` 在 24.999、`1e308` 起点…）跑一遍。

另外一条**边界事实**：参数类型不对时（例如 `obj.call("path_calculate", 123, "x", null, [])`），
是**引擎**先拒绝（返回 `null`），根本到不了 `call_func` —— 因为参数 `PropertyInfo` 声明了类型。
那条 `E_BAD_ARGS` 分支实际是防御性代码（档位：观测，见 §7.2 的 `wrong types -> <null>`）。

---

## 4. `direction` / `flip` / 修正常数

`core_bridge.gd` 的语义：**正 = 向右（team 0），负 = 向左（team 1）**。
判据与客户端一致（`_preview()` 里 `1.0 if direction >= 0.0 else -1.0`）：

```
flip = (direction < 0.0)         // 0.0 归"向右"，与客户端一致
c    = y0 − f(flip ? −x0 : x0)   // = crate::graphwar_shot_calculator::correction
tokens' = [ADD, CONST c] ++ tokens   // = crate::graphwar_shot_calculator::shifted_tokens ⇒ 实际射击用 f(x)+c
```

`flip` 对**几何**的影响（单测 `normal_curve_left_uses_f_of_minus_x` 钉住）：
采样器在 flip 时用 `mid` 求值、把 `x` 取负存点，所以点满足 `y = f(−x) + c`，
而**不是**同一条曲线的反着走。换句话说"向左打"= 把函数关于 y 轴镜像。
首点永远是士兵位置 `(x0, y0)` 本身（与 `_preview()` 一致，且 `path::calculate` 就是这么做的）。

---

## 5. 用的是真采样器（没有第二份实现）

`src/api.rs::calculate_path` 只做四步，每一步都调用既有实现：

```rust
let tokens  = crate::math_function::parse(expr);          // @0x4ed670
let offset  = crate::graphwar_shot_calculator::correction(&tokens, x0, y0, flip);   // @0x3c5f70
let shifted = crate::graphwar_shot_calculator::shifted_tokens(&tokens, offset);
let points  = crate::graphwar_path_calculator::calculate(&shifted, x0, y0, STEP_DEFAULT, flip, Mode::Normal); // @0x3c2530
```

步长 `STEP_DEFAULT = 0.01`、容差 `LENGTH_TOL = 0.001`、点数预算 `(200/step) as i32 = 20000`、
二分上限 100、平面边界 `|x| ≤ 25 / |y| ≤ 15` —— **全部**来自 `src/lib.rs` 的常量与 `graphwar_path_calculator.rs` 的逻辑，
`api.rs` 里没有任何采样循环。

防"偷偷重写一份"的测试：`matches_shot_points_pipeline` 拿 5 条函数 × 2 个方向，
断言 `calculate_path(...)` 的 `points` **与 `crate::graphwar_shot_calculator::shot_points(...)` 逐点相等**
（后者是已有 331 条夹具 + 上游 2552 例验证过的端到端入口）。
档位：**逐位验证**（点序列 `assert_eq!`，f64 精确相等）。

---

## 6. 错误码

| `error` | 触发条件 | 档位 |
|---|---|---|
| `""`（空串） | 成功 | 逐位验证 |
| `start_not_finite` | `x0`/`y0` 是 `NaN`/`±inf` | 逐位验证 |
| `start_out_of_bounds` | `|x0| > 25` 或 `|y0| > 15` | **推断**（见下） |
| `empty_expression` | 词法切不出任何 token（`"@@@"`、`"   "`） | 逐位验证 |
| `undefined_at_start` | 修正常数不是有限数（`sqrt(x)` 在 `x=−1`、`ln(x)` 在 `x=−2`） | 逐位验证 |
| `no_points` | 首点之后再没有被接受的点（越界 / NaN / 100 次二分不收敛） | 逐位验证 |
| `bad_arguments` | 参数个数/类型不对（实测**到不了**这里，见 §3.7） | 观测 |
| `internal_panic` | `catch_unwind` 兜底 | 推断（未触发过；不想触发） |

三条要写清楚的判断：

1. **起点越界**为什么算错误：采样器把 `(x0, y0)` **无条件**塞成首点（`graphwar_path_calculator.rs` 第一行），
   再做逐点边界检查 —— 起点越界时整条曲线在平面外，界面上会画到战场外面去。
   `AGENTS.md` 的硬事实是"越界点在**接受之前**检查"。所以这里在入口先拒。
   档位：**推断**（真二进制在起点越界时是否也拒，未观测；客户端预览分支同样是先判 `|x| ≤ 25`）。
2. **空表达式**：原版**能画** —— 抓包里 `{"function":""}` 的修正文本是 `" + 10.990"`，
   也就是 `f(x) = 0` 的水平直线（`docs/05` §2）。但既有的 `shot::shot_points` 与 C ABI
   `gw2_shot_points` 都按"空 = 没有点"处理，且客户端在调内核**之前**就把空串拦下了
   （`GwCore.calculate_path` 先 `GwExpr.compile`）。本次保持与既有内核一致：报错。
   **这是一处已知的、有意保留的偏差**，要不要改成"空串 ⇒ 水平直线"见 §8.4。
3. **不收敛**：`path::calculate` 在第 100 次二分仍失败时直接 return，不区分原因；本层也不猜，
   统一归到 `no_points`（只在"一段都没走成"时触发）。半路停下的情况仍然是 `ok: true`，
   只是点少 —— 与真二进制一致（真值夹具里就有这种短曲线）。

---

## 7. 端到端验证记录

### 7.1 环境

| 项 | 值 |
|---|---|
| Godot | `/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64`（`4.7.2.stable.official.ed1daf0bf`） |
| 产物 | `target/debug/libgw2_rebuild.so`（crate 的 `[lib] name = "gw2_rebuild"`，**不是** `libgraphwarrust.so`） |
| XDG | `XDG_DATA_HOME/CACHE/CONFIG` 全部指到工作区内（`run.sh` 已处理；副本里手动指定） |
| 隔离 | 全部实验在 `gw2-rebuild` 的克隆 `.agent-work/kapi/repo`（分支 `agent/kernel-api`）里做 |

### 7.2 库侧

```bash
export CARGO_HOME=<ws>/.cargo-home
export CARGO_TARGET_DIR=<ws>/.agent-work/kapi/target
cargo test --offline        # 218 passed, 0 failed（基线 205 → +13）
```

新增 13 条全在 `src/api.rs` 的 `#[cfg(test)]`：正常曲线（向右/向左）、与
`shot::shot_points` 的逐点一致、水平直线（真实抓包样本）、空表达式、起点越界、起点非有限、
起点无定义、立即越界、方向映射、点数预算、契约键名/类型、极端输入不 panic。

GDScript 侧的单点验证（`GW2Rebuild` 直接调用，27 项检查，0 失败）：

```
@@ PASS has_method(path_calculate)
@@ method_list: args=[expr:String, x0:float, y0:float, direction:float] ret={ "name": "result", "type": 27, ...}
@@ PASS point count == 2471 (Rust 侧同值)
@@ PASS first point == soldier pos, got (5.0, 2.0)
@@ chord { "min": 0.00901130773127, "max": 0.01048363745213 }
@@ PASS offset = y0 - f(x0), got 2.958924
@@ PASS left offset = y0 - f(-x0)
@@ bad expr -> { "ok": false, "points": [], "offset": nan, "error": "empty_expression" }
@@ oob start -> { "ok": false, "points": [], "offset": nan, "error": "start_out_of_bounds" }
@@ wrong types -> <null>          ← 引擎侧先拒，不崩
@@ FAILS=0
```

### 7.3 客户端侧：`source` 前后对照（**本次最关键的验证**）

做法：把 `client/` **原样复制两份**到 `.agent-work/kapi/e2e/{nokernel,kernel}`，只在 `kernel`
那一份里加 `gw2rebuild.gdextension` + `.so` + 一个探针脚本/场景（探针把 `scenes/main.tscn` 挂进来，
再走真实的 `App.command(...)` 路由）。**原仓库的 `client/` 一个字节都没动。**

| 观测点 | 无内核（= 今天的 `client/`） | 有内核（`.so` 被加载） |
|---|---|---|
| `GwCore.kernel_available()` | `false` | `true` |
| `kernel_note()` | `内核未加载（ClassDB 里没有 GW2Rebuild）→ 弹道用界面预览` | `内核 GW2Rebuild 已加载，使用 path_calculate` |
| `kernel_methods()` 含 `path_calculate` | 否（只有 `ping`） | 是 |
| `GwCore.calculate_path("sin(x)", (5,2), +1)["source"]` | **`preview`** | **`kernel`** |
| 同上的 `points.size()` | 1001（dx=0.02，撞到 `abs(x)>25` 停） | **2471** |
| 同上弦长 min/max | `0.0208 / 0.0283` | **`0.0090 / 0.0105`**（等弦长 0.01） |
| 同上 `offset` | `2.95892427466314`（`y0 − f(x0)`） | 同值（向右时两式相同） |
| 向左的 `source` | `preview` | **`kernel`**，且 `offset = y0 − f(−x0)` |
| MainMenu 版本行（`--dump-tree`） | `client 0.1.0 · kernel off` | **`client 0.1.0 · kernel on`** |
| PlayScreen 开火后 `GwPlaneView.path_points` | 2151 点，弦长 `0.0200 / 0.0277` | **5295 点，弦长 `0.0090 / 0.0105`** |
| 界面上那条线 vs 内核同参数输出 | 两边都是 preview（相同，但不具判别力） | **逐点相同（5295 点，前 50 点全等且 size 相等）** |

最后一行是本任务真正要证的东西：**界面画的折线 = 内核采样器算出来的点序列**，
不是同一套预览算法的巧合。判据用的是 `GwCore.calculate_path` 的返回值（`source == kernel`）
与 `plane.path_points` 的逐元素比较。

### 7.4 两个 selftest 都在，条数不变

| 跑在哪 | 命令 | 结果 |
|---|---|---|
| 真 `client/`（无内核） | `bash client/run.sh selftest` | `SELFTEST 93 passed, 0 failed` / `@@SELFTEST_DONE code=0` |
| 上面 `kernel` 副本（有内核） | `godot --headless --path <copy> -- --selftest` | `SELFTEST 93 passed, 0 failed` / `@@SELFTEST_DONE code=0` |

**没有改任何断言**，也不需要改：`client/tools/selftest.gd` 里 `GwCore` 出现 **0 次**
（`grep -c GwCore client/tools/selftest.gd` → `0`），13 条与对局相关的断言只查
"`plane.path_points.size() > 1`"和"`OffsetLabel` 非空"——内核接上后两者照样成立。

### 7.5 怎么复现（在真 `client/` 上启用内核，需客户端负责人执行）

`client/` 里目前**没有** `.gdextension`（`grep -rn gdextension client/` 无结果），
所以今天的 `client/` 永远只能走预览。启用只需三步（都不涉及改代码）：

```bash
# 1) 编译并拷库（库名来自 Cargo.toml 的 [lib] name = "gw2_rebuild"）
cargo build --offline            # 或 --release
cp target/debug/libgw2_rebuild.so client/

# 2) 新建 client/gw2rebuild.gdextension
cat > client/gw2rebuild.gdextension <<'EOF'
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.1

[libraries]
linux.debug.x86_64 = "res://libgw2_rebuild.so"
linux.release.x86_64 = "res://libgw2_rebuild.so"
EOF

# 3) 验收
bash client/run.sh selftest     # 期望 93 passed, 0 failed
bash client/run.sh dump | grep -i "kernel on"   # 期望 client 0.1.0 · kernel on
```

注意 `client/.gitignore` 是否忽略了 `*.so`（本仓库的 `.gitignore` 已忽略二进制；
这条只影响是否把库入库，不影响运行）。**`.so` 不要提交**（`AGENTS.md` 硬规矩 3）。

### 7.6 这次**没有**验的

* ~~release profile~~ —— **已补测**：`cargo test --offline --release` 218 条全绿；
  `cargo build --release` 的 `.so` 放进同一个 `kernel` 副本跑 §7.3 的探针，
  结果与 debug **逐位相同**（`n=2471`、PlayScreen `5295` 点、弦长 min/max 小数点后 14 位一致）。
* **真实窗口渲染**（`run.sh shots` / Xvfb）：只在 `--headless` 下验证；
  弹道在屏幕上的落点靠 §2 的坐标系推理 + 逐点数据一致性，没有像素级截图比对。
* **多次开火后的长期内存**：只测了 200 次调用的 `MEMORY_STATIC`（delta 0），没做长时间对战。
* **起点越界的真二进制行为**：见 §6 第 1 条（推断）。

---

## 8. 仍未做 / 下一步

### 8.1 `client/` 侧要加一个 `.gdextension`（**阻塞项，不在我的权限内**）

这是唯一的阻塞项：无论内核注册得多完整，Godot 只会扫描**工程目录内**的 `.gdextension`。
`client/` 下没有这个文件，所以真 `client/` 跑的永远是 `source = preview`。
文件内容见 §7.5，需要客户端负责人落盘。

### 8.2 命中/掩膜还没接（建议签名）

`src/plane.rs` 的掩膜与点测试都已逐字节/逐点验证（`docs/05` §6），`src/graphwar_shot_calculator::simulate`
也是可用的原语，但**没有**暴露给 GDScript —— 界面的命中判定仍是"路径点到士兵中心 < 0.8"
的几何近似（`docs/108` §7 #2）。建议的下一步：

```text
GW2Rebuild.hit_test(points: PackedVector2Array, obstacles: Array) -> int
    # 返回首个命中点序号（-1 = 未命中）；obstacles 每项 {x, y, size}（棋盘坐标）
GW2Rebuild.mask_blob(obstacles: Array) -> PackedByteArray
    # 1000×600 掩膜，索引 = y*1000 + x，与 plane::mask_index 一致
```

两个必须写进契约的细节（否则又会出现坐标系错位）：

* 掩膜是**像素**空间（`CANVAS_W=1000`、`CANVAS_H=600`），棋盘→像素是
  `px = 20x + 500`、`py = 20y + 300`（`plane.rs`，与 `docs/08` 一致）；
  而 `points` 传进来是**棋盘**坐标 ⇒ `hit_test` 必须内部换算。
* **y 方向**：像素 y 向下、棋盘 y 向上，`plane.rs` 的换算里已经含了翻转，别在外面再翻一次。

### 8.3 ODE（微分方程）模式没有入口

`core_bridge.gd` 的 4 参数签名里**没有** mode，所以内核只走 `Mode::Normal`。
但界面有 `function_mode`（`play_screen.gd:461` 用它决定是否显示修正常数），
`docs/110` 也确认了"微分方程模式走变体 B"。要么加第 5 个参数（`default_argument_count = 1`），
要么加第二个方法 `path_calculate_ode(...)` —— **两者都要改契约**，需要和客户端一起定。
注意：ODE 模式下修正常数为空（`docs/92` §4.10），届时 `offset` 应返回 `NaN`。

### 8.4 空表达式的语义待定

见 §6 第 2 条：原版 `""` 是合法的（水平直线），当前内核报 `empty_expression`。
要改的话只影响 `src/api.rs` 的一行判断（`tokens.is_empty()` 之后补一条"空串 ⇒
只保留首点+直线"），但会与 `shot::shot_points` / `gw2_shot_points` 的既有约定不一致，
需要一起定。

### 8.5 `docs/108` 的两处"未接"描述现在已经过时

§7 #1（"弹道 = 固定 dx 折线，未向 GDScript 暴露"）与 §10 表格里
"内核 `path_calculate` 等接口 ❌ 未接" —— 本文之后应改成
"✅ `path_calculate` 已接（`docs/116`）；命中/掩膜仍未接"。
**本文不改 `108`**：那是客户端交付物，留给客户端负责人一并更新，避免并行改动打架。

---

## 9. 结论档位汇总

| 结论 | 档位 |
|---|---|
| `path_calculate` 的点序列与 `shot::shot_points` 逐点相等（= 真采样器） | **逐位验证** |
| 契约四个键名/类型/顺序 | **逐位验证** |
| 各错误码的触发条件（除下面两条） | **逐位验证** |
| `points` 是棋盘坐标（不是像素） | **观测**（调用方源码 + 实测首点 `(5,2)`） |
| `source` 从 `preview` 变成 `kernel`；界面画的线与内核输出逐点相同 | **观测**（§7.3） |
| PropertyInfo 的 `class_name`/`hint_string` 不能 NULL；`variant_set_keyed` 不可用 | **观测**（都是真崩溃复现后修的） |
| 20 次/200 次重复调用不漏 Godot 内存 | **观测**（`MEMORY_STATIC` delta 0） |
| 起点越界时真二进制也拒绝 | **推断**（依据：首点无条件入列 + AGENTS 硬事实；未观测） |
| 裸对象与 Variant 的 COW 分家会写丢数据 | **推断**（按 CowData 语义设计规避，未复现失败案例） |
| debug 与 release 的 `.so` 端到端结果一致 | **观测**（同一份探针，数值逐位相同） |
| 真实窗口渲染、真二进制对 `""`/越界起点的行为 | **未知**（缺实验，见 §7.6 / §6） |
