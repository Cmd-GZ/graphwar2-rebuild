# 138 · 微分方程弹道：把 `Mode::Ode` 暴露给 GDScript

状态：**现行**。本文消掉两条已登记缺口的**内核侧与桥接侧**：

* [`116-kernel-api.md`](116-kernel-api.md) §8.3 —— "ODE（微分方程）模式没有入口"；
* [`133-replay-player.md`](133-replay-player.md) §6 #3 —— "`Mode::Ode` 在内核里没有暴露成
  GDScript 方法"（那一行的**表现**是：微分方程回放只画士兵/障碍、弹道留空）。

**没消掉的那一半**：界面侧还没人传 `mode`（`play_screen.gd` / `replay_player.gd` 不在本轮
可改范围）。也就是说：**内核与桥现在都能算 ODE 弹道了，但还没有界面在调用它**（§8.1）。
谁接这一行，谁就能让 DiffEq 局出弹道 —— 不需要再动 `src/`。

证据分级沿用 `AGENTS.md` 的四档：**逐位验证** / **观测** / **推断** / **未知**；
凡是"我们定、原版没证据"的量额外标 **自定**（并写明要什么证据才能升级）。

---

## 0. 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新方法 | `GW2Rebuild.path_calculate_mode(expr: String, x0: float, y0: float, direction: float, mode: String) -> Dictionary` | **逐位验证**（`get_method_list` + `dump` 实测，§6.3） |
| `mode` 的类型 | **字符串** `"normal"` / `"ode"`；大小写不敏感、忽略两侧空白 | **逐位验证**（§6.1 单测） |
| 非法 `mode` | `ok:false, error:"bad_arguments"`（**沿用**既有错误码，不新造） | **逐位验证**（单测） |
| 返回结构 | 与 `path_calculate` **同形**：`{ok, points, offset, error}`（`RESULT_KEYS` 四个键，顺序相同） | **逐位验证**（复用 `result_pairs`，无第二份整形） |
| ODE 采样器本身 | `dy/dx = f(x, y)`，一阶 RK4；第三个变量（`y'`）恒 `0` | **逐位验证**（224 条 path 夹具里 **112 条是 ODE**：56 条 `flip=0` + 56 条 `flip=1`，`tests/golden.rs::path_matches_binary`） |
| RK4 的 **`y` 反馈** | `k2/k3/k4` 用 `y + k·h/2` 回代（`src/graphwar_path_calculator.rs`） | **推断**（按 `docs/04-path-sampling.md` §2 的反编译阅读实现；**没有**真值夹具覆盖，见 §8.2） |
| ODE 下修正常数 | **不做**（与 `shot_points` 的 `apply_correction && mode == Mode::Normal` 一致） | **逐位验证**（单测与 `shot_points(…, Mode::Ode, false)` 逐点相等） |
| ODE 下 `offset` | **恒 `NaN`**（成功也是），理由见 §3 | **自定**（原版内部这个数没有人观测到；界面那行字为空是**观测**，§3） |
| `mode="normal"` | 与 `path_calculate` **逐位相同**（直接委托，不是第二份实现） | **逐位验证**（单测逐位对拍 + 结构上委托） |
| `path_calculate` 契约 | **一字未改**：签名、行为、错误码、既有 13 条单测全部原样通过 | **逐位验证** |
| `cargo test --offline` | **314 → 320**（+6，全绿） | **逐位验证** |
| `bash client/run.sh selftest` | **478 → 489 passed, 0 failed**（+11 条断言；未改/未删任何既有断言） | **观测** |
| `bash client/run.sh dump` | `methods=…,path_calculate,path_calculate_mode,…` | **观测**（§6.3 贴了原行） |
| `client/` 的 `GwCore.calculate_path` | 新增**可选**第 4 参数 `mode`（默认 `"normal"`）；返回字典多一个 `mode` 字段 | **逐位验证**（selftest：3 参数调用与显式 `"normal"` 结果相同） |
| 微分方程局真的能画了吗 | **桥好了，界面还没接**（§8.1） | **观测** |

三条最容易出错的点，先写在前面：

1. **`offset` 在 ODE 下是 `NaN`，不是 `0.0`**。用 `0.0` 会让界面右下角显示 `+ 0.000`，
   而原版 DiffEq 局那行字是**空**的（`docs/92` §4.10 观测，`play_screen.gd:786` 也照此置空）。
2. **ODE 不做修正常数**。谁在 API 层补一个 `c = y0 − f(±x0)`，整条微分方程弹道就平移
   —— `src/graphwar_shot_calculator::shot_points` 那行判断（`apply_correction && mode == Mode::Normal`）
   已经把这件事钉死了。
3. **`mode` 是字符串参数**。`PropertyInfo` 声明成 `String`；传整数会被**引擎先拒**
   （`docs/116` §3.7 同形实测），所以内核侧不做整数重载（那段代码永远走不到）。

---

## 1. 契约

### 1.1 内核方法（新增，GDScript 直接可调）

```text
GW2Rebuild.path_calculate_mode(expr: String, x0: float, y0: float,
                               direction: float, mode: String)
    -> Dictionary { ok: bool, points: PackedVector2Array, offset: float, error: String }
```

与 `path_calculate` 相比**只多第 5 个参数**，返回**完全同形**（同样四个键、同样顺序）：

| 键 | 类型 | 语义 |
|---|---|---|
| `ok` | `bool` | 是否算出了曲线（`points.len() >= 2` 且没出错） |
| `points` | `PackedVector2Array` | **棋盘坐标**点序列（`|x| ≤ 25`、`|y| ≤ 15`），首点是士兵位置 |
| `offset` | `float` | normal：`c = y0 − f(±x0)`；**ode：恒 `NaN`**（§3） |
| `error` | `String` | 空串 = 成功；否则是机器可读错误码（§4） |

### 1.2 `mode` 的取值

| 传入 | 结果 |
|---|---|
| `"normal"` / `"NORMAL"` / `"  Normal  "` | 走 `calculate_path`（`y = f(x)` + 修正常数） |
| `"ode"` / `"ODE"` / `"  Ode  "` | 走 `calculate_path_ode`（`dy/dx = f(x,y)`，一阶 RK4，**无**修正常数） |
| 其它（`""`、`"0"`、`"1"`、`"diffeq"`、`"ode2"`…） | `ok:false, error:"bad_arguments"` |

两条口径写清楚：

* **不做整数重载**。不是"不想做"，是**做不到**：FFI 的 `PropertyInfo` 把这个参数声明成
  `String`，GDScript 传 `1` 会被引擎在**进入 `call_func` 之前**拒绝（`docs/116` §3.7
  用另一种类型组合实测过同一条路径）。写了整数分支也是死代码。
* **不做模糊匹配**。`"od"`、`"normal2"` 一律报错，不"猜一个最像的"。理由：这条参数决定
  弹道是"函数图"还是"积分曲线"，猜错比报错糟得多；而报错在客户端是**可见**的
  （`GwCore` 返回字典里带 `source="mode"` 与 `note`）。

### 1.3 `client/` 侧的桥（`client/src/core/core_bridge.gd`）

```gdscript
static func calculate_path(expr_text: String, start: Vector2, direction: float,
        mode: String = "normal") -> Dictionary
```

* 第 4 个参数**可选**，默认 `"normal"` ⇒ 既有三参数调用点（`play_screen.gd:767`、
  `play_screen.gd:1329`、selftest）**一个字都不用改**；
* 返回字典在原有 `{ok, points, offset, source, note}` 之外多一个 `mode` 字段
  （归一化后的 `"normal"` / `"ode"`），便于状态行/排查；
* `source` 增加了两种取值：`"mode"`（模式名不认识）与 `"none"`（内核没有 ODE 入口，
  **不拿预览折线冒充**）。

---

## 2. ODE 模式到底做什么

### 2.1 采样器一行没动

`src/graphwar_path_calculator.rs` 的 `Mode::Ode` 分支本来就在（一阶 RK4 + 与 Normal 相同的等弦长二分、
相同的平面边界、相同的点数预算），本轮**没有改它一个字节**。新增的只是"谁来调用它、
用哪些 token 调用"。

采样器那部分的权威结论在 [`04-path-sampling.md`](04-path-sampling.md) §2：
`dy/dx = eval(tokens, x, y, 0)`，RK4 的 `1/6` 来自 `.rodata`（`0x19f098`），
且 **ODE 模式没有 Normal 那条"半步再校验一次"**（`0x3c2840` 容差满足即接受）。
本轮 `src/_scaffold/api.rs` 的改动全部在调用侧，不触碰这三条。

### 2.2 不做修正常数：与 `shot_points` 一致（**逐位验证**）

`src/graphwar_shot_calculator::shot_points` 的那行判断是唯一依据：

```rust
if apply_correction && mode == Mode::Normal {
    let c = correction(&toks, x0, y0, flip);
    toks = shifted_tokens(&toks, c);
}
```

* **Normal**：只要求出 `c`，把 `[ADD, CONST c] ++ tokens` 交给采样器 ⇒ 曲线穿过士兵；
* **ODE**：`mode != Normal` ⇒ **即使 `apply_correction = true` 也不加**。

所以 `calculate_path_ode` 的流水线就是 `parse → calculate(tokens, …, Mode::Ode)`，
中间没有 `correction`、没有 `shifted_tokens`。单测
`ode_matches_shot_points_without_correction` 用 4 条表达式 × 2 个方向断言：

```text
calculate_path_mode(expr, x0, y0, dir, "ode").points
  == shot_points(expr, x0, y0, STEP_DEFAULT, dir<0, Mode::Ode, false)
```

并额外断言 `shot_points(…, Mode::Ode, false)` 与 `shot_points(…, Mode::Ode, true)`
**逐点相同** —— 即"ODE 下 `apply_correction` 被忽略"这条语义本身也有测试钉着。

> 为什么这很重要：`shot_points` 是既有 331 条夹具 + 上游 2552 例验证过的端到端入口
> （`docs/116` §5）。ODE 分支若自己发明一个 `c`，就与它分家了。

### 2.3 ODE 的几何含义（`flip`）

`flip` 的语义与 Normal 完全相同（`src/graphwar_path_calculator.rs` 一处 `if flip { -x0 }` 与一处
`px = if flip { -mid } else { mid }`）：向左开火时**从 `−x0` 向 `+x` 积分、再把 x 取负存点**。
对 `dy/dx = f(x,y)` 来说这不是"同一条曲线反着走"，而是"在镜像坐标里解一次"。

档位：**逐位验证** —— 112 条 ODE 夹具里 **56 条 `flip=1`**（`fixtures/path.tsv`
第 2 列），`tests/golden.rs::path_matches_binary` 逐点对拍全绿。

---

## 3. `offset` 在 ODE 下返回什么，以及为什么

**返回 `NaN`（成功时也是 `NaN`）。**

### 3.1 为什么不是 `0.0`

`offset` 的契约是"**实际被加到函数上的那个修正常数**"（Normal：`c = y0 − f(±x0)`，
`docs/05-shot-pipeline.md` §2、`docs/116` §4）。ODE 模式**根本没有加过任何常数**
（§2.2），返回 `0.0` 会变成一句假话：

* 语义上：`0.0` 意味着"加了个 0"，暗示这条曲线经过 `shifted_tokens` 那条路——它没有；
* 显示上：客户端的 `GwCore.format_offset(0.0)` 会输出 `+ 0.000`，而原版 **DiffEq 局
  右下角那行字是空的**（`docs/92` §4.10 观测；`play_screen.gd:786` 也按此对 DiffEq
  置空）。`+ 0.000` 是一个真实对战里不会出现的字符串。

### 3.2 为什么 `NaN` 恰好是对的

客户端读这个字段的既有代码是：

```gdscript
"offset": float(res.get("offset", 0.0))     # core_bridge.gd
_offset_label.text = GwCore.format_offset(...) if function_mode == 0 else ""
```

而 `format_offset()` 对 `NaN` 返回**空串**（`docs/116` §1：NaN 读法已被验证安全）。
于是 ODE 下界面那行字自动是空 —— **不需要**改任何调用方，也与原版 DiffEq 局一致。

### 3.3 代价（写清楚，别踩）

`offset = NaN` 在**错误分支**里也是约定（`PathOutcome::err` 一律 `NaN`）。所以：

> **不能**用 `offset` 判断这次调用成功没有；判断一律看 `ok` 与 `error`。

这一条在 Normal 下同样成立（既有行为），本轮只是让它对 ODE 也成立。
档位：**自定**（这是"如何表达不存在"的设计选择；原版内部这个字段的真实取值**未知**，
见 §8.3）。

---

## 4. 错误码（沿用既有，不新造）

| `error` | 触发条件 | ODE 下会不会出现 | 档位 |
|---|---|---|---|
| `""` | 成功 | 会 | 逐位验证 |
| `start_not_finite` | `x0`/`y0` 是 `NaN`/`±inf` | 会（与 Normal 同判据） | 逐位验证 |
| `start_out_of_bounds` | `|x0| > 25` 或 `|y0| > 15` | 会（同判据） | **推断**（真二进制行为未观测，`docs/116` §6 第 1 条） |
| `empty_expression` | 词法切不出任何 token | 会（与 Normal 一致，`docs/116` §6 第 2 条的待定项照旧） | 逐位验证 |
| `undefined_at_start` | 修正常数不是有限数 | **不会**——ODE 不求 `c`。导数在起点无定义会退化成 `no_points`（§4.1） | 逐位验证（单测 `ode_error_codes_are_the_shared_ones`） |
| `no_points` | 首点之后再没有被接受的点 | 会（越界 / NaN / 100 次二分不收敛） | 逐位验证 |
| `bad_arguments` | 参数个数/类型不对，**或 `mode` 不认识** | 会（`mode` 非法就走这里） | 逐位验证（单测） |
| `internal_panic` | `catch_unwind` 兜底 | 会（未触发过） | 推断 |

### 4.1 `undefined_at_start` 为什么在 ODE 下不出现

Normal 的这条错误来自"**修正常数**不是有限数"（`sqrt(x)` 在 `x=−1` ⇒ `c = NaN`）。
ODE 没有修正常数，就没有这个检查；导数在起点无定义时是：

```text
k1 = eval(tokens, x0, y0, 0) = NaN
  ⇒ yn = NaN ⇒ chord = NaN ⇒ !(tol > NaN.abs()) 为真、chord.is_nan() 为真 ⇒ hi = mid
  ⇒ 二分 100 次仍不接受 ⇒ 采样器只留下首点 ⇒ no_points
```

这条与 `shot_points(…, Mode::Ode, false)` **完全一致**（同一个采样器），
不是本轮新发明的口径。选择"不额外加一次导数预检"的理由：`_scaffold/api.rs` 的角色是搬运
（`docs/116` §5），加预检就多一处可能与被验证过的采样器分家的判断。

---

## 5. 内核实现（`src/_scaffold/api.rs`，全部新增）

### 5.1 纯 Rust 层（可脱离 Godot 单测）

| 项 | 说明 |
|---|---|
| `parse_mode(&str) -> Option<Mode>` | 字符串 → `Mode`；`trim` + `eq_ignore_ascii_case`；不认识返回 `None` |
| `calculate_path_ode(expr, x0, y0, direction)` | ODE 流水线（`parse → calculate(…, Mode::Ode)`），`offset = NaN` |
| `calculate_path_mode(expr, x0, y0, direction, mode)` | 分派：`Normal` **直接委托** `calculate_path`；`Ode` → `calculate_path_ode`；其余 `bad_arguments` |

`"normal"` 分支是**委托**而不是复制，所以"与 `path_calculate` 完全一致"是结构上成立的
（单测 `normal_mode_is_bit_identical_to_calculate_path` 仍逐位复核 5 条成功 + 5 条错误输入、
2 个方向、3 种写法 = 60 组对比）。

### 5.2 FFI 层

`path_calculate_mode_call` / `register_path_mode_method` 与 `path_calculate` 那两个函数
**同形**，差异只有"多一个 `String` 参数"：

* 整个 `call_func` 体包在 `catch_unwind(AssertUnwindSafe(…))` 里，兜底 `internal_panic`；
* `argc < 5` / `args` 或 `ret` 为空 / 读不出字符串或 `f64` ⇒ `bad_arguments`（不 panic）；
* `GDExtensionPropertyInfo` 的 `class_name` / `hint_string` 指向**默认构造的空**
  `StringName` / `String`（**不能传 NULL**：4.7.2 实测 SIGSEGV，`docs/116` §3.3）；
* `arguments_metadata` 给 5 个 `0`（`NONE`）；`argument_count = 5`；`default_argument_count = 0`
  （五个参数都必填；GDScript 侧的可选性由 `core_bridge.gd` 的默认值提供）；
* 返回复用 `write_result` / `result_pairs`（**没有**第二份字典整形，`RESULT_KEYS` 仍是四个键）。

### 5.3 `src/_scaffold/gdext.rs`：+1 行

```rust
let _ = crate::_scaffold::api::register_path_method(get, lib);
let _ = crate::_scaffold::api::register_path_mode_method(get, lib);   // ← 新增的唯一一行
```

必须在 `register_path_method` **之后**：接口表是同一个 `OnceLock`（`docs/120` §3 的约定）。

---

## 6. 验证记录

### 6.1 `cargo test --offline`：314 → 320（+6，全绿）

新增 6 条，全在 `src/_scaffold/api.rs::tests`：

| 测试 | 钉住什么 |
|---|---|
| `ode_mode_integrates_the_derivative` | `dy/dx = cos(x)` 从 (0,0) ⇒ `y = sin(x)`（前 300 点误差 < 5e-3）；且与 Normal 的点序列不同 |
| `ode_mode_uses_the_y_variable` | `dy/dx = y` 从 (0,1) ⇒ `y = e^x`（前 100 点误差 < 2e-3）；同一表达式 Normal 是水平线 ⇒ 两条序列不同 |
| `normal_mode_is_bit_identical_to_calculate_path` | `mode="normal"`（3 种写法）× 10 组输入 × 2 方向 与 `calculate_path` **逐位**相同（`f64::to_bits`） |
| `ode_matches_shot_points_without_correction` | ODE 点序列 == `shot_points(…, Mode::Ode, false)`；并断言 `apply_correction` 在 ODE 下被忽略 |
| `invalid_mode_is_an_error_not_a_panic` | 10 个非法 mode ⇒ `bad_arguments`、无点、`offset=NaN`；3 个合法写法必须成功 |
| `ode_error_codes_are_the_shared_ones` | `start_not_finite` / `start_out_of_bounds` / `empty_expression` / `no_points` 四个分支 |

```text
cargo test --offline
  lib:          266 → 272    (+6)
  其余 8 个测试二进制：48（不变）
  合计：        314 → 320    0 failed
```

> 说明：`mode="ode"` **能出点**这条（任务硬要求①）由前两条覆盖；
> **`normal` 与 `path_calculate` 逐位相同**（②）由第三条覆盖；
> **非法 mode 不 panic**（③）由第五、六条覆盖。

### 6.2 `bash client/run.sh selftest`：478 → 489（+11 条断言，0 failed）

前置（与既有文档一致，别漏）：

```bash
export GODOT=<ws>/tools/Godot_v4.7.2-stable_linux.x86_64   # 克隆里 run.sh 的相对路径会指错
export GW2_NET_SERVER=<ws>/.agent-work/odeapi/target/debug/examples/serve_match
#   或把 examples/serve_match 放到 <repo>/target/debug/examples/（selftest 自己会找）
#   否则真服务端 e2e 那条会**显式 skip**（本轮起点就变成 442）
cargo build --offline && cp target/debug/libgw2_rebuild.so client/
godot --headless --path client --import
```

新增 11 条（`client/tools/selftest.gd::_test_core_path_mode`，**只加不改**）：

1. 默认 `mode = "normal"`（返回字典里的 `mode` 字段）；
2. 三参数调用与显式 `"normal"` 的 `points` 相同（加参数没改坏老路径）；
3. 未知 mode 直接 `ok:false`；
4. `mode="ode"` 能算出弹道；
5. `mode="ode"` 的 `source == "kernel"`；
6. **同一表达式下 `ode` 与 `normal` 的 `points` 不相等**（任务硬要求，真的走了不同分支）；
7. ODE 的 `offset` 是 `NaN` ⇒ `format_offset()` 显示空串；
8. ODE 的右端能用 `y`（`dy/dx = y` 从 `y=1` 出发能出点；顺带证明 ODE **没有**被客户端
   求值器（只认 `x`）提前拒掉）；
9. 内核类可实例化（下面两条要直调内核，不经过 `GwCore`）；
10. **直调内核**：`path_calculate_mode(…, "normal")` 与 `path_calculate(…)` 的 `points`
    与 `offset` 相同（第四、六条走的是 `GwCore`，这两条才真正验到 FFI 的 `normal` 分支）；
11. **直调内核**：非法 mode（`"nope"`）⇒ `ok:false, error:"bad_arguments"`
    （`GwCore` 会先把未知 mode 拦下，所以内核这条 `bad_arguments` 分支只能靠直调覆盖）。

```text
# 起点（主仓库 main，2b87b69 + 本机 assets + serve_match 就位）
SELFTEST 478 passed, 0 failed
# 本分支
SELFTEST 489 passed, 0 failed
```

### 6.3 注册与 `dump`

`--import`（Godot 4.7.2）的 stderr：

```text
[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)
[gw2-rebuild] registered method path_calculate_mode(expr, x0, y0, direction, mode)
[gw2-rebuild] registered method hit_test(points, obstacles, soldiers)
[gw2-rebuild] registered method level_obstacles(level_index, seed)
```

`bash client/run.sh dump`（行首部分，`methods` 太长已截断）：

```text
@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,path_calculate_mode,hit_test,level_obstacles,… note=内核 GW2Rebuild 已加载，使用 path_calculate + hit_test + level_obstacles
```

### 6.4 数值样本（内核层实测，客户端拿到的是同一批点）

| 输入 | `normal` | `ode` |
|---|---|---|
| `sin(x)` 从 (5,2) 向右 | 2471 点，末点 (24.998125, 2.824714)，`offset=2.958924` | 2467 点，末点 (24.996875, **1.292878**)，`offset=NaN` |
| `sin(x)` 从 (5,2) 向左 | 3696 点，末点 (−25, 0.908724)，`offset=1.041076` | 3706 点，末点 (−24.993125, 1.293393)，`offset=NaN` |
| `cos(x)` 从 (0,0) 向右 | 3087 点，末点 (24.99875, −0.008963)，`offset=−1` | 3083 点，末点 (24.99625, −0.136068)，`offset=NaN` |
| `y` 从 (0,1) 向右 | 2667 点，**水平线** y ≡ 1（Normal 下 `y` 恒被求值成 0） | 1448 点，在 (2.7075, **14.99**) 停住（`|y| ≤ 15` 边界），即 `y = e^x` |
| `x−y` 从 (2,1) 向右 | 1867 点，末点 (15.995, 14.995) | 1867 点，末点 (15.995, 14.995) —— **巧合重合**（见下） |

两条要写清的读数：

* `dy/dx = cos(x)` 从 (0,0) 的解析解是 `sin(x)`：ODE 前 500 点与解析解的**最大偏差
  = 1.352e-12**（RK4 + 弦长容差 0.001 的量级，远好于容差）；
* ODE 曲线的弦长统计 `min 0.009001 / max 0.010607`，与 Normal 的 `0.0090–0.0105` 同一量级
  —— 说明等弦长二分在 ODE 分支上照常工作。

> `x−y` 那行是**反面样本**，写在这里防误读：`y = x − 1` 恰好是 `dy/dx = x − y` 过 (2,1)
> 的解，所以两种模式画的是**同一条直线**。`assert_ne!` 只在真正非线性的输入上成立
> （单测/selftest 用的都是 `sin(x)` / `y`）。

### 6.5 改动的文件

```text
src/_scaffold/api.rs                      +431 行（新方法 + 单测；path_calculate 一行未改）
src/_scaffold/gdext.rs                    +1 行（注册）
client/src/core/core_bridge.gd  +83 −4（可选 mode 参数 + ODE 分支）
client/tools/selftest.gd        +68（新增断言；既有断言一条未动）
docs/138-ode-api.md             新增（本文）
```

**没碰**：`src/graphwar_path_calculator.rs`（采样器）、`src/graphwar_shot_calculator.rs`、`src/_scaffold/protocol.rs`、`play_screen.gd`、
`replay_player.gd`、`client/src/net/`、`docs/INDEX.md`、`docs/HANDOVER.md`。

---

## 7. 怎么复现

```bash
# 1) 内核
export CARGO_HOME=<ws>/.cargo-home
export CARGO_TARGET_DIR=<ws>/.agent-work/odeapi/target
cargo test --offline            # 期望 320 passed, 0 failed
cargo build --offline
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/

# 2) Godot 必须重新扫一次（否则新方法不出现，docs/116 §8）
<ws>/tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path client --import

# 3) 验收
bash client/run.sh dump | grep -o "path_calculate_mode"
bash client/run.sh selftest     # 期望 489 passed, 0 failed
```

GDScript 侧最小调用：

```gdscript
var r := GwCore.calculate_path("cos(x)", Vector2(5, 2), 1.0, "ode")
print(r["ok"], " ", r["mode"], " ", r["points"].size(), " ", r["offset"])  # true ode N nan
```

---

## 8. 仍未确定 / 下一步

### 8.1 界面还没把 `mode` 传下去（**唯一的剩余接线**）

`play_screen.gd:767` 与 `:1329` 仍然只传三个参数：

```gdscript
var result := GwCore.calculate_path(expr, active_soldier.pos, direction)
```

所以**今天**在 DiffEq 局里开火，画的仍是 `Mode::Normal` 的图（与之前一样）。
接线是**一行**，但 `play_screen.gd` / `replay_player.gd` 不在本轮可改范围，留给客户端负责人：

```gdscript
var mode := "ode" if int(app_node.state.function_mode) == 1 else "normal"
var result := GwCore.calculate_path(expr, active_soldier.pos, direction, mode)
```

同理，回放侧（`docs/133` §6 #3）要把 `FunctionModeInfo == "DiffEqFunction"` 翻成 `"ode"`；
`GwReplayState.function_mode` 已经存了那个字符串（`replay_player.gd:240`），
`mode_index()` 也已经把它映射成 `1`（`replay_player.gd:290`）。

⚠️ 顺带一条**界面口径**要一起改：`play_screen.gd:786` 目前是
"DiffEq ⇒ OffsetLabel 置空"，而 ODE 的 `offset` 本来就是 `NaN`（`format_offset()` ⇒ 空串），
两条口径**结果相同**，不会打架；将来若删掉那条 `if`，显示也不会变。

### 8.2 RK4 的 `y` 反馈没有真值夹具（**推断**）

`fixtures/path.tsv` 的 224 行里，**两种模式都只用了 tag 0（`x`）**，没有一行用到
tag 1（`y`）或 tag 2（`y'`）。所以：

* "RK4 积分 + 等弦长二分 + flip + 边界" —— 112 条 ODE 夹具**逐位验证**；
* "`k2/k3/k4` 把 `y + k·h/2` 回代" —— 按 `docs/04` §2 的反编译阅读实现，**推断**，
  单测只能对**解析解**（`e^x`、`sin(x)`）而不是对真二进制。

要升级这一条，需要新的真值夹具：让 `analyze` 侧用一次**带 `y` 的 ODE 函数**跑真二进制
采样器（`0x3c2530`，`mode=1`），把点序列落成 `path.tsv` 的新行。

### 8.3 `offset = NaN` 是**自定**，不是观测

已观测到的是"原版 DiffEq 局右下角那行字是空的"（`docs/92` §4.10 + `play_screen.gd:786`）。
**没有**观测到的是"原版内部那个 `offset`/`c` 变量在 DiffEq 下取什么值"。
`NaN` 是能让客户端既有代码自动显示空串、且不撒谎的选择；若将来发现原版其实是
`Some(0.0)` 但界面按 `function_mode` 强制不显示，那也不影响本契约（本契约只承诺
"ODE 下没有可显示的修正文本"这一**行为**，见 §3.2）。要推翻它需要一份真二进制观测。

### 8.4 空表达式 / 起点越界：与 `path_calculate` 保持同一处待定

`docs/116` §6 的两条已知事项（空串是否该画水平直线、起点越界真二进制是否也拒）
在 ODE 侧**原样继承**（`empty_expression` / `start_out_of_bounds`）。
ODE 特有的那条差异（`undefined_at_start` 不出现，退化成 `no_points`）已在 §4.1 写明，
它是"没有修正常数"的直接后果，不是新的待定项。

### 8.5 没有验的

* **真窗口渲染**：只在 `--headless` 下验；ODE 曲线在屏幕上的落点靠"点序列 = 内核输出"
  这条（与 `docs/116` §7.6 同一局限）。
* **release 构建**：本轮只跑 debug（`docs/116` §7.6 曾验证过 debug/release 的 `.so`
  端到端逐位相同；本轮没复测）。
* **长时间/连续调用**：没做 ODE 的 `MEMORY_STATIC` 长跑（`docs/116` §3.6 的 200 次
  调用测试未在 ODE 方法上重做）。
* **原版 DiffEq 局的真实弹道**：回放的 `FunctionPoints` **不带点**（`docs/133` §4），
  所以"我们重算的 ODE 曲线与原版当时画的那条一样"**无法直接对拍** —— 能对拍的只有
  采样器本身（§2.1 的 112 条夹具）。

---

## 9. 档位汇总

| 结论 | 档位 |
|---|---|
| ODE 采样器（RK4 主体 + 等弦长二分 + flip + 边界）与真二进制一致 | **逐位验证**（112 条 ODE 夹具） |
| `mode="normal"` 与 `path_calculate` 逐位相同 | **逐位验证** |
| ODE 不做修正常数（与 `shot_points` 同一行判断） | **逐位验证** |
| 契约四个键/顺序/类型不变；`path_calculate` 一字未改 | **逐位验证** |
| `mode` 字符串语义、非法值 → `bad_arguments` | **逐位验证** |
| 新方法在 `dump` 里可见、8 条 selftest 断言全绿 | **观测** |
| 数值样本（点数/末点/弦长统计/解析解偏差 1.35e-12） | **观测**（内核层实测） |
| RK4 的 `y` 反馈（`k2/k3/k4` 回代） | **推断**（无真值夹具，§8.2） |
| 起点越界时真二进制也拒绝 | **推断**（沿用 `docs/116` §6 第 1 条） |
| `offset = NaN` 这个取值本身 | **自定**（行为依据是观测：DiffEq 局不显示修正文本，§8.3） |
| `undefined_at_start` 在 ODE 下退化成 `no_points` | **逐位验证**（单测 + 与采样器同构），但"原版是否也这样"**未知** |
| 原版 DiffEq 局真实弹道与我们重算的一致 | **未知**（回放不带点，无法对拍，§8.5） |
| 界面（PlayScreen / 回放）已经画出 ODE 弹道 | **还没有**（§8.1） |
