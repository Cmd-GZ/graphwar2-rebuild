# 120 · 命中判定接进客户端：`GW2Rebuild.hit_test`

状态：**现行**。本文只讲一件事：客户端"这一炮打死了谁"从**几何近似**换成了
**内核的掩膜规则**（`plane` 的逐点查表 + `settle` 的结算顺序），并且界面上能看出
当前用的是哪一种。

> 前置阅读：[`116-kernel-api.md`](116-kernel-api.md)（GDExtension 手写 FFI 的既有做法、
> 坐标系那节别跳）、[`30-kill-semantics.md`](30-kill-semantics.md)（穿过即死、不挡弹道）、
> [`08-plane.md`](08-plane.md) §5–§6（掩膜的形状与地址）、
> [`84-kills-verified.md`](84-kills-verified.md) §2（击杀数与真游戏一致）、
> [`108-client-rebuild.md`](108-client-rebuild.md) §7（这一条原本登记为占位）。

---

## 0. 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新方法 | `GW2Rebuild.hit_test(points, obstacles, soldiers) -> Dictionary` | 逐位验证（`get_method_list` 与“注册成功”日志，§6.3） |
| 返回 | `{ ok, hits, hit_points, first_hit, stop, blocked, error }`（顺序固定） | 逐位验证（单测 `result_shape_is_exactly_the_contract`） |
| 坐标系 | 三个入参全是**棋盘坐标**（`|x|≤25`、`|y|≤15`）；掩膜是**像素**空间，换算只在 `plane::to_pixel` 里做一次 | 逐位验证（§2 三条测试） |
| 用的是不是真规则 | 是。判定本体是 `graphwar_shot_calculator::settle::resolve_points`，`graphwar_shot_calculator::settle::resolve` 也走它 —— **只有一份实现** | 逐位验证（等价性测试 `agrees_with_settle_resolve_on_captured_shots`） |
| `cargo test --offline` | **242 → 258**（+16，全绿） | — |
| `bash client/run.sh selftest` | **93 passed, 0 failed**（内核开 / 关各跑一遍；**未改任何断言**） | — |
| 客户端改动 | `core_bridge.gd` +102/−6，`play_screen.gd` +27/−12（两处函数体，无布局/文案/节点树改动） | — |
| 内核改动 | `src/_scaffold/api.rs`（新增）、`src/graphwar_shot_calculator/settle.rs`（新增一个函数 + `resolve` 改为委托）、`src/_scaffold/gdext.rs`（**+1 行**注册） | — |
| 客户端状态行 | `本轮命中 N 个士兵 ／ 判定来源：kernel|preview ／ <note>` | 观测（§6.2 原文） |
| 仍未确定 | 士兵半径常量的二进制出处、友军伤害、射手免疫的直接证据、同时回合顺序（§7） | — |

三条最容易出错的点，先写在前面：

1. **坐标只换一次**：进参是棋盘坐标、掩膜是像素坐标（`px = 20x+500`、`py = 20y+300`）。
   `hit_test` 里**不做**任何换算，换算在 `plane::is_occupied` / `plane::rasterize` 内部。
   在外面再翻一次就是整体偏 500/300。
2. **障碍与士兵不是一回事**：障碍进掩膜、让弹道**停下**；士兵**不**进掩膜，只在
   "弹道点落在半径内"时判死（`docs/30` 纠正过的建模）。所以 `first_hit` 说的是
   "弹道停在哪一点"，与"谁死"是两件事。
3. **手写 FFI 读 `Packed*Array` 别用 `variant_get_ptr_indexed_getter`**：4.7.2 上实测
   读出来是垃圾（§3.1）。正确做法是 `packed_*_array_operator_index_const`。

---

## 1. 签名与契约

### 1.1 方法（本条为逐位验证：§6.3 的注册日志 + GDScript 实调成功）

```text
GW2Rebuild.hit_test(points: PackedVector2Array,
                    obstacles: PackedFloat64Array,
                    soldiers: PackedFloat64Array)
    -> Dictionary
```

`obstacles` / `soldiers` 都是**扁平数组、步长 3**：

```text
obstacles = [x0, y0, size0,   x1, y1, size1,   …]    # size = 障碍半径（游戏单位）
soldiers  = [x0, y0, rad0,    x1, y1, rad1,    …]    # rad  = 致死半径，抓包里恒 0.5
```

返回（`src/_scaffold/api.rs::HIT_KEYS`，顺序固定，`ok` 在最前——客户端先读它）：

| 键 | 类型 | 含义 |
|---|---|---|
| `ok` | bool | 调用是否成功；`false` 时 `error` 给机器可读的错误码 |
| `hits` | `Array[int]` | 被判死的士兵在 **`soldiers` 里的下标**，按弹道经过的先后 |
| `hit_points` | `PackedVector2Array` | 与 `hits` 一一对应：第一次进入该士兵半径的那个弹道点 |
| `first_hit` | int | 弹道**第一次撞到障碍**的点序号；`-1` = 全程没撞到障碍 |
| `stop` | `Vector2` | 弹道停止处（棋盘坐标）；没撞障碍时是最后一个点 |
| `blocked` | bool | 是否因撞到障碍而停下 |
| `error` | String | 成功时为空串 |

错误码：`bad_arguments` / `unreadable_array` / `bad_obstacles` / `bad_soldiers` /
`no_points` / `points_not_finite` / `internal_panic`。
整个 `call_func` 包在 `catch_unwind` 里（`docs/116` §3.7 的硬要求）：跨 FFI 的 panic
会 abort 掉 Godot，所以 panic 一律翻成 `ok:false, error:"internal_panic"`。

### 1.2 为什么用扁平 `PackedFloat64Array`，而不是 `Array[Dictionary]`

`docs/116` §8.2 的建议签名写的是 `obstacles: Array`（每项 `{x, y, size}`）。这里改成
扁平数组，理由是**少一层没用过的 ABI**：

* 读扁平数组元素走的是与 `points` **完全相同**的那条路径（§3.2），一次验证覆盖三处；
* 读 `Array[Dictionary]` 还要在 FFI 层调 `Dictionary.get`（`variant_call` + 键 Variant），
  多一个静默失败点，却换不到任何表达力——客户端只有 2–4 个士兵、0 个障碍。

GDScript 侧一行就能拼出来（`core_bridge.gd::_flat_sizes`）：

```gdscript
flat[i * 3 + 0] = p.x
flat[i * 3 + 1] = p.y
flat[i * 3 + 2] = radius
```

### 1.3 只加不改

`docs/116` §8.2 的两个候选方法里，本次只实现 `hit_test`；`mask_blob`（把 1000×600
掩膜整块交给 GDScript）**没有做**——界面不需要掩膜，只需要"谁死、弹道停哪"，
把 600 KB 搬过去既慢又多一个版本不一致的来源（`docs/116` §8.2 的建议保留在文里，
这里明确记为"不做"）。

---

## 2. 坐标换算：怎么钉住的

### 2.1 事实（观测，出处见 `docs/08` §4 与 `src/plane.rs` 文件头）

```text
px = x * 1000.0 / 50.0 + 500.0        py = y * 1000.0 / 50.0 + 300.0      （浮点顺序照抄原程序）
px = px as i32                        py = py as i32                      （向零截断）
掩膜下标 index = py * 1000 + px       空 = 0xFF，其余 = 占据               越界 = 命中
```

`|x| ≤ 25`、`|y| ≤ 15` ⇒ 画布 1000×600；棋盘原点落在像素 (500, 300)。

### 2.2 `hit_test` 自己**不**换算

这是本文最要紧的一条约定：

* `points` / `obstacles` / `soldiers` 从 GDScript 进来，全是**棋盘坐标**；
* 建掩膜用 `plane::rasterize`（内部 `to_pixel`），查点用 `plane::is_occupied`
  （内部同样 `to_pixel`）；
* 返回的 `stop` / `hit_points` 也全是**棋盘坐标**（直接取自入参点序列）。

也就是说：`src/_scaffold/api.rs::hit_test` 里既没有 `20.0`、也没有 `500`/`300`。
谁要是在这里加一次换算，结果就会整体偏 500/300 像素，而且**看上去仍然"能打中人"**
（只是打错人）——所以必须有测试钉住。

### 2.3 三条钉住它的测试（逐位验证）

| 测试 | 钉的是什么 |
|---|---|
| `board_to_pixel_constants_are_20x_plus_500` | `(0,0)→(500,300)`、`(10,0)→(700,300)`、`(0,5)→(500,400)`、`(-25,-15)→(0,0)`、`(25,15)→(1000,600)` |
| `hit_test_does_not_treat_board_coordinates_as_pixels` | 士兵在棋盘 (10,0)：走 y=0 的弹道**必须**判死；而走棋盘 (−24.5,−15) 的弹道**必须不**判死（那是"把棋盘坐标当像素"时它会被画到的地方）。y 轴同构再钉一遍 |
| `obstacle_mask_uses_the_same_board_to_pixel_mapping` | 棋盘 (0,0) 半径 1 的障碍挡住 y=0 的直线；y=4 的直线不被挡；(−24.5,−15) 的直线**不**被挡 |

对照实验的写法值得说明：单看"命中"是无法区分对错的（坐标错了也可能碰巧命中），
所以每个正确断言都配一条**错误映射的指纹**——如果换算写错，那条会红。

### 2.4 "擦过"有两条不同的边界，别混

| 对象 | 判据 | 出处 | 测试 |
|---|---|---|---|
| 士兵 | `d <= radius`（**闭区间**，含等号） | `graphwar_shot_calculator::settle::resolve_points` | `grazes_the_kill_radius_boundary`（0.499 死 / 0.5 死 / 0.501 不死） |
| 障碍 | 掩膜像素 `!= 0xFF`（**不是几何圆**） | `plane::is_occupied` | `graze_into_the_antialias_ring_still_counts_as_a_hit` |

第二条容易被忽略：障碍光栅化会写一圈**抗锯齿值**（`docs/08` 实测出现过
`0x06 0x19 0x1f 0x39 0x3f 0x64 0x96`），它们虽然几乎透明，但**不是 0xFF**，所以算占据。
实测：障碍 (0,0) `size=1.0` 时，棋盘 (1.0, 0.1)（离圆心 **1.005** > 1.0）落在像素
(520,302)，掩膜值 25 —— 判命中；再往外一个像素（1.1）才是空。
"几何圆"模型在这里会给相反答案，**真规则是掩膜**。

---

## 3. 手写 FFI：这次新踩的两个坑

### 3.1 `variant_get_ptr_indexed_getter` 读 `Packed*Array` 是**错的**（实测）

`gdextension_interface.h` 里它长这样：

```c
typedef void (*GDExtensionPtrIndexedGetter)(GDExtensionConstTypePtr p_base,
                                           GDExtensionInt      p_index,
                                           GDExtensionTypePtr  r_value);
```

第一版按"`r_value` 是一个 Variant 槽"实现，结果**进程 SIGSEGV**：

```text
handle_crash: Program crashed with signal 11
[3] libgw2_rebuild.so  <gw2_rebuild::_scaffold::api::Api>::destroy_variant      src/_scaffold/api.rs:552
[2] libgw2_rebuild.so  <gw2_rebuild::_scaffold::api::Api>::read_packed_v2       src/_scaffold/api.rs:627
```

改成"`r_value` 收裸内置类型"（`Vector2` / `double`）后不再崩，但**读出来的值是垃圾**。
同一个数组、同一时刻，两路对照（`points` 的第 0/1 点，真值应是 (−18, −5.1) 起）：

```text
A 路 variant_get_ptr_indexed_getter : A0=(1.42682514493267e-35, 0.0)  A1=(8.407790785948902e-45, 0.0)
B 路 packed_vector2_array_operator_index_const : B0=(-18.0, -5.1)     B1=(-17.99125, -5.09425)   ← 对
```

`A0 ≈ 1e-35` 这种量级是把 4 字节小整数当 f32 读的特征值。**换了接口，一次就对了。**
`docs/116` §3.1 那张"缺任何一个就整体不注册"的表里，这条现在换成了 B 路的两个接口。

### 3.2 正确做法（`docs/116` §3 的直接延伸）

四种接口，两步搞定，`PackedVector2Array` 与 `PackedFloat64Array` 共用一套：

```text
① get_variant_to_type_constructor(V_PACKED_*)   把 Variant 里的**裸数组**取出来
                                                （8 字节 CowData 指针；取出来加过一次引用，
                                                  用完必须 variant_get_ptr_destructor(type) 放掉）
② packed_vector2_array_operator_index_const(raw, i) -> const Vector2*
   packed_float64_array_operator_index_const(raw, i) -> const double*
                                                逐个取元素地址，直接读裸值
③ 元素个数用 variant_call(v, "size")            （`docs/116` §3.4 那条"内置类型能调方法"）
```

两个接口名都在 4.7.2 的二进制里核实过（`strings | grep operator_index` 全部命中），
启动时也会打印（§6.3）。

### 3.3 写 Dictionary 的那一段被复用

`docs/116` §3.4 的 `write_result` 被拆成通用的 `write_dict(pairs)`，
`path_calculate` 与 `hit_test` 共用；`ApiValue` 增加了 `Int` / `Vec2` / `IntArray` 三种。
造字典的 ABI 因此在全仓库只出现一次。

---

## 4. 与内核规则的关系：没有第二份实现

### 4.1 先纠正一句：掩膜判定**不在** `src/graphwar_life_manager.rs`

`src/graphwar_life_manager.rs`（111 行）里只有**生命表**（两张 `HashMap` + `mode==1` 分支）和
状态字节的语义 `is_alive_byte(state & 1)`（`docs/98` §1.3）。它**没有**掩膜、
没有坐标、没有"谁死"的判定——档位：观测（读文件即可）。

真正的规则分布在两处，都是既有代码：

| 环节 | 代码 | 二进制出处 |
|---|---|---|
| 掩膜（1000×600 字节，`0xFF` 空，越界算命中） | `src/plane.rs`：`empty_mask` / `rasterize` / `is_occupied` / `to_pixel` | 光栅化 `0x4f11e0`、点测试 `0x4f15b0`、构造 `0x523020`（`docs/08` §6.1、§6.1.1、§6.1.2） |
| 判定顺序（先判死、再判挡；士兵不挡弹道） | `src/graphwar_shot_calculator/settle.rs`：`resolve_points` | 真对局回放（`docs/30`、`tests/live_hits.rs` 12 条） |

所以本次**没有新写判定**，只是把这两块拼起来并抬到 GDScript。

### 4.2 单一实现是**结构上**保证的

```rust
// src/graphwar_shot_calculator/settle.rs:43
pub fn resolve(text, x0, y0, step, flip, mode, mask, targets) -> ShotOutcome {
    let points = shot_points(text, x0, y0, step, flip, mode, true);   // 自己采样
    resolve_points(mask, targets, &points)                            // ← 同一函数
}

// src/graphwar_shot_calculator/settle.rs:73   （本次新增，规则体只有这一份）
pub fn resolve_points(mask: &[u8], targets: &[Target], points: &[(f64, f64)]) -> ShotOutcome { … }
```

`src/_scaffold/api.rs::hit_test` 里没有一行判定逻辑，它只做三件事：

1. 按 `docs/30` 建掩膜（**只放障碍**，`rasterize(…, 0x00)`）；
2. 把士兵三元组变成 `graphwar_shot_calculator::settle::Target { id: i, x, y, radius }`（`id` = 入参下标）；
3. 调 `resolve_points`，把结果翻译成 Dictionary。

`hit_test` 里那两次再扫描（找"第几个点第一次进半径"、找 `first_hit` 的点序号）
**只决定报哪个序号，不参与判定**——`resolve_points` 只回 `killed: Vec<id>`，
不带点序号，而界面要的是"第几点打中的"。这一点在代码注释里写明了。

### 4.3 等价性测试（逐位验证）

`agrees_with_settle_resolve_on_captured_shots`：把 `tests/live_hits.rs` 里那局
**真对局**（14 个障碍、4 名士兵、两发 `(x/3.02)` / `(-1.65/(-8.04/x))`）原样搬到
`_scaffold/api.rs` 的测试里，对每一发同时算：

* 真值 = `graphwar_shot_calculator::settle::resolve`（自己采样 + 判命中，`docs/30` 里验证过会打死 230/231）；
* 待验 = `hit_test`（点序列来自 `calculate_path`，与客户端完全同源）。

断言两者**致死集合、`blocked`、`stop` 逐项相等**。谁要在 `_scaffold/api.rs` 里偷偷重写一套判定，
这条就红。它同时证明了"客户端拿到的点序列足够用来判定"（不需要第二次采样）。

---

## 5. 客户端改动范围（`client/` 是别人的交付物，改动刻意最小）

### 5.1 逐处

| 文件 | 改动 | 行数 |
|---|---|---|
| `client/src/core/core_bridge.gd` | 新增 `hit_test()` / `_approx_hits()` / `_flat_sizes()`；`SOLDIER_HIT_RADIUS = 0.5`；`kernel_has_hit_test()`；文件头契约补一段 | +102 / −6 |
| `client/src/screens/play_screen.gd` | `_resolve_shots()` 整个函数体（原"几何近似"）；`fire()` 里 `shot` 字典**加一个键** `"soldier"`（把射手本人带进结算，用于排除自己） | +27 / −12 |

**没有动**：布局、文案模板、节点树、`run.sh`、`selftest.gd`（一条断言都没改，见 §6.2）。
`client/tools/` 下临时探针脚本跑完即删（§6.5），不入库。

### 5.2 判定那一段（`play_screen.gd:504`）

```gdscript
for shot in shots:
    var pts: PackedVector2Array = shot.points
    var targets: Array = []
    for s in _soldiers:
        if not bool(s.alive): continue
        if is_same(s, shot.get("soldier", null)): continue      # 射手本人不参与
        targets.append({"pos": s.pos, "radius": GwCore.SOLDIER_HIT_RADIUS, "soldier": s})
    if targets.is_empty(): continue
    var judged := GwCore.hit_test(pts, targets, [])             # ← 内核
    sources.append(String(judged.source))
    note = String(judged.note)
    for idx in judged.hits:
        var victim: Dictionary = targets[int(idx)].soldier
        if bool(victim.alive):
            victim.alive = false
            kills += 1
_status("本轮命中 %d 个士兵 ／ 判定来源：%s ／ %s" % [kills, sources[0], note])
```

三个细节：

* **状态行照原风格**：原来 `fire()` 写 `弹道来源：kernel ／ …`，这里同样把来源写进
  状态行，措辞改成"判定来源"。`source` 只有 `kernel` / `preview` / `none` 三种。
* **状态行现在无条件写**（原来只在有击杀时才写）：没有击杀也需要能看出
  "这一炮是内核判的、还是近似判的"，否则"内核没接上"在界面上看不出来。
* **射手排除**：`is_same()` 比字典内容相等稳（同一份 `Dictionary` 引用）。

### 5.3 一处**行为纠错**（不是本次功能，但必须记下来）

旧的近似判定从 `pts[0]` 开始扫，而 `pts[0]` **就是射手自己的位置**，
距离 0 ≤ 0.8 ⇒ **每一炮都把自己打死**。这就是为什么以前 1v1 一开炮就走到结算框
（`selftest.gd` 还特意写了"要在开火之前查计时器，因为 1v1 一炮就可能打完"）。
新实现按 `settle` 的约定把射手排除在外，于是这一炮**不再自伤**，对局会继续。

### 5.4 退到近似的分支（保留，并且看得出来）

`GwCore.hit_test()` 在下列情况退回 `_approx_hits()`（`source = "preview"`）：

* `ClassDB` 里没有 `GW2Rebuild`（内核没加载 / `.gdextension` 没被扫描到）；
* 类在、但方法列表里没有 `hit_test`（只注册了 `path_calculate`）；
* 调了、但返回 `ok:false`（把 `error` 拼进 `note`）。

近似版就是原来的规则（任一点到士兵中心 `< 半径`），半径取 `radius`，
所以它和"旧行为"一致，只是现在会**明说自己不是内核**。

---

## 6. 端到端验证记录

环境：本机 2026-11-06，Godot `4.7.2.stable.official.ed1daf0bf`，
`cargo` debug 构建，独立克隆 `.agent-work/hittest/repo`（分支 `agent/hit-test`）。

### 6.1 库侧

```bash
export CARGO_HOME=…/gw2/.cargo-home CARGO_TARGET_DIR=…/.agent-work/hittest/target
cargo test --offline
```

| 目标 | 条数 |
|---|---|
| `unittests src/lib.rs` | **218**（其中新增 `_scaffold::api::hit_tests::*` **16** 条） |
| `tests/broker.rs` | 4 |
| `tests/events.rs` | 6 |
| `tests/evidence.rs` | 6 |
| `tests/golden.rs` | 4 |
| `tests/live_hits.rs` | 12 |
| `tests/plane.rs` | 6 |
| `tests/server_match.rs` | 2 |
| **合计** | **258**（起点 242，+16） |

新增的 16 条（全部在 `src/_scaffold/api.rs` 的 `mod hit_tests`）：

```text
kills_a_soldier_the_trajectory_passes_through     正常命中
hit_points_are_indices_into_the_trajectory        hit_points 与点序列自洽
grazes_the_kill_radius_boundary                  擦过（0.499/0.5/0.501）
graze_into_the_antialias_ring_still_counts       擦过（掩膜抗锯齿环仍是占据）
misses_when_the_trajectory_never_gets_close      未命中
obstacle_stops_the_shot_without_killing_anyone    障碍只停弹道
soldier_dies_before_the_obstacle_stops_the_shot   先判死、后判停
board_to_pixel_constants_are_20x_plus_500         坐标常量
hit_test_does_not_treat_board_coordinates_as_pixels   坐标（错误映射的指纹）
obstacle_mask_uses_the_same_board_to_pixel_mapping    坐标（障碍侧）
result_shape_is_exactly_the_contract             契约形状
soldier_indices_follow_the_input_array           hits 是入参下标、按经过顺序
empty_points_is_an_error_not_a_panic             空点序列走 error
nan_points_are_rejected                          NaN 点序列被拒
flat_arrays_must_be_multiples_of_three           步长 3 校验
agrees_with_settle_resolve_on_captured_shots      与 graphwar_shot_calculator::settle::resolve 逐项一致
```

### 6.2 客户端自检（`selftest`，两种分支各跑一遍）

```bash
bash client/run.sh selftest      # 等价于 godot --headless --path client -- --selftest
```

| 内核状态 | 状态行（`StatusLabel`） | 结果 |
|---|---|---|
| `.so` 在场（`hit_test` 注册成功） | `本轮命中 0 个士兵 ／ 判定来源：kernel ／ 内核 hit_test（掩膜 1000×600 + 士兵半径）` | `SELFTEST 93 passed, 0 failed` / `@@SELFTEST_DONE code=0` |
| `.so` 拿掉（内核不在） | `本轮命中 0 个士兵 ／ 判定来源：preview ／ 内核无 hit_test → 几何近似` | `SELFTEST 93 passed, 0 failed` / `@@SELFTEST_DONE code=0` |

**一条断言都没改、也没加**（`grep -c GwCore client/tools/selftest.gd` 仍是 0）：
`selftest` 只检查"弹道点数 > 1""OffsetLabel 非空"，与判定来源无关。

> 这一炮为什么是 0 命中？射手在棋盘 (−18, −5.1)，`sin(x)` 的修正常数是
> `c = y0 − f(x0) = −5.1 − sin(−18) = −5.851`，于是 `y(18) = sin(18) − 5.851 = −6.602`；
> 对手站在 (18, −5.1)，差 **1.5** > 0.5 ⇒ 不判死。旧的近似会判死的那个是**射手自己**
> （§5.3）。所以 0 命中恰恰是新规则正确的表现。

### 6.3 注册日志（`hit_test` 确实挂到类上了）

```text
[gw2-rebuild] api resolve: … register_method=true vec2_opindex=true f64_opindex=true
[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)
[gw2-rebuild] registered method hit_test(points, obstacles, soldiers)
```

⚠️ 与前一次（`docs/116` §7.5）一样的坑，仍然有效：**新增/改动 `.gdextension` 或
首次拿到新方法后，必须让 Godot 重新扫描一次**，否则内核静默不加载（版本行仍显示
`kernel off`）：

```bash
godot --headless --path client --import
```

本机实测：第一次 `--import` 结束后引擎在退出阶段崩溃过一次（core dump，
与扩展无关的编辑器布局阶段），第二次干净退出、日志如上。**这条只影响首次扫描**，
之后 `selftest` 稳定。

### 6.4 一局完整流程里的击杀（真 PlayScreen，不是直接调内核）

`selftest` 那一炮打不到人，所以"击杀分支"（`for idx in judged.hits`）没被跑到。
为了不把它留在未验证状态，做了一次**临时**实验（跑完已撤销，工作区无残留）：

```gdscript
# play_screen.gd::_spawn_position 里临时插入（已删除）
if team == 1:
    return Vector2(18.0, -6.601974493543755)   # 正好落在 sin(x) 弹道上
```

结果（同一发 `sin(x)`，经 `fire() → _resolve_shots()` 的真实路径）：

```text
本轮命中 1 个士兵 ／ 判定来源：kernel ／ 内核 hit_test（掩膜 1000×600 + 士兵半径）
SELFTEST 93 passed, 0 failed
```

⇒ 内核判死 → 客户端改存活标记 → 状态行 → `_refresh_plane()` 整条链是通的。

### 6.5 直接调 `GwCore.hit_test` 的探针（`--script`，跑完即删）

探针脚本（原文，跑完删除、不入库；带 `App` 自动加载不需要，`GwCore` 是全局类）：

```gdscript
extends SceneTree
func _init() -> void:
	print("PROBE kernel_available=%s has_hit_test=%s" % [GwCore.kernel_available(), GwCore.kernel_has_hit_test()])
	var line := PackedVector2Array()
	for i in 201:
		line.append(Vector2(2.0 - 0.02 * i, 0.0))          # 棋盘 y=0，x 从 +2 到 −2
	var soldier := [{"pos": Vector2(0.0, 0.0), "radius": GwCore.SOLDIER_HIT_RADIUS}]
	var r1 := GwCore.hit_test(line, soldier, [])
	print("PROBE A_hit_on_path      hits=%s first_hit=%d blocked=%s source=%s" % [r1.hits, int(r1.first_hit), r1.blocked, r1.source])
	var graze := [{"pos": Vector2(0.0, 0.6), "radius": GwCore.SOLDIER_HIT_RADIUS}]
	var r2 := GwCore.hit_test(line, graze, [])
	var old_approx := false
	for p in line:
		if p.distance_to(Vector2(0.0, 0.6)) <= GwPlaneView.SOLDIER_RADIUS: old_approx = true; break
	print("PROBE B_graze_0.6        hits=%s source=%s  旧近似(0.8)=%s" % [r2.hits, r2.source, old_approx])
	var inner := [{"pos": Vector2(0.0, 0.45), "radius": GwCore.SOLDIER_HIT_RADIUS}]
	var r3 := GwCore.hit_test(line, inner, [])
	print("PROBE C_graze_0.45       hits=%s source=%s" % [r3.hits, r3.source])
	var r4 := GwCore.hit_test(line, [], [{"pos": Vector2(0.0, 0.0), "size": 1.0}])
	print("PROBE D_obstacle_stop    first_hit=%d stop=%s blocked=%s source=%s" % [int(r4.first_hit), str(r4.stop), r4.blocked, r4.source])
	var r5 := GwCore.hit_test(line, [{"pos": Vector2(10.0, 0.0), "radius": 0.5}], [])
	print("PROBE E_offboard_soldier hits=%s source=%s" % [r5.hits, r5.source])
	quit(0)
```

实跑输出（原样）：

```text
PROBE kernel_available=true has_hit_test=true
PROBE A_hit_on_path      hits=[0] first_hit=-1 blocked=false source=kernel
PROBE B_graze_0.6        hits=[] source=kernel  旧近似(0.8)=true
PROBE C_graze_0.45       hits=[0] source=kernel
PROBE D_obstacle_stop    first_hit=48 stop=(1.04, 0.0) blocked=true source=kernel
PROBE E_offboard_soldier hits=[] source=kernel
```

逐条读法：

* **A**：弹道穿过士兵 → 判死，`source=kernel`，没撞障碍 ⇒ `first_hit=-1`。
* **B**：士兵在 0.6 —— **内核说没死，旧的 0.8 近似说死**。这是"判定确实换掉了"的
  直接证据（不是措辞变了，是结论变了）。
* **C**：0.45 ≤ 0.5 → 判死（闭区间）。
* **D**：障碍 (0,0) `size=1` 把弹道停在 `x=1.04`（`line[48]`，采样步长 0.02 的网格点；
  像素上是 520，即 `20×1.04+500` 截断到 520 —— 与 `plane` 的向零截断一致）。
* **E**：棋盘 (10,0) 的士兵不在 y=0 的 ±2 段内 → 不判死（坐标没被当成像素的旁证）。

### 6.6 这次**没有**验的

* release 构建的端到端（只用 debug `.so` 跑过；`docs/116` §0 记过 debug/release 逐位相同，
  但那是 `path_calculate`，不是 `hit_test`）。
* 真实联机对手（`client/` 还没有联网结算；本次只在本地对局流程里验证）。
* `hit_test` 的耗时（未测；一次调用要重建 600 KB 掩膜 + 光栅化障碍 + 全点扫描，
  粗看是毫秒量级，但没有拿数据说话）。

---

## 7. 仍未确定（写清缺什么）

| # | 问题 | 现状 | 缺什么 |
|---|---|---|---|
| 1 | 士兵**致死半径 0.5** 的二进制出处 | 观测：抓包 `PosInfo.radius` 恒 0.5（`docs/24` §、`docs/25` §1），`docs/30` §3 用"到死者最小距离 ≤ 0.5"反证 | 判定函数里那个常量的地址/字节（`0x4f15b0` 是点测试，致死循环本体尚未定位） |
| 2 | **射手不会打死自己** | 观测：4 发配对回放里射手全存活（`tests/live_hits.rs`）；`settle` 把它写成"调用方负责" | 二进制里是否存在"跳过 owner"的显式分支（`docs/30` §5 一直列为未验证） |
| 3 | **友军伤害** | 未知：`docs/30` §5 明确"本局两次都是敌方击杀"，样本没覆盖 | 一局队友互射的抓包 |
| 4 | 障碍光栅化的 `fill` 值 | 约定：`rasterize(…, 0x00)`（沿用 `tests/live_hits.rs`） | 游戏实际传的 `fill`。注意：只要不是 `0xFF`，"是否占据"的结论不变，只有边缘像素的**具体值**会不同 |
| 5 | `docs/08` §6.3 的"0.5 单位栅格扫描"假设 | **未采纳**：本实现沿采样点逐个查（与 12 条真对局回放一致） | 若真游戏是"按 0.5 栅格扫"，极近擦过的判据会有 1 个像素级差异；未观测到反例 |
| 6 | 同时回合（`SIMULTANEOUS`）的结算顺序 | 推断：客户端按 `_shot_queue` 顺序逐发判，同一士兵只记一次 | 真游戏多发同时结算的 tick 顺序（`docs/86` 只算到"死亡并集"，没算顺序） |
| 7 | 掩膜的**生命周期** | 差异：真游戏是常驻、带锁的共享掩膜（`docs/08` §6.1.1，`0x523020` 构造一次）；本实现每次 `hit_test` 重建 | 无行为差异（同样的输入给同样的掩膜），只有性能特征不同 |
| 8 | `first_hit` 的语义 | 本文定义：**第一个撞障碍的点序号**（`docs/116` §8.2 的建议签名把它当"命中序号"） | 若将来要"第一个被打死的人"，用 `hits[0]`；两个概念在真规则里本来就是分开的（`docs/30`） |
| 9 | 士兵的**身份**只有下标 | 设计：`hits` 回 `soldiers` 数组下标，客户端不需要 id | 若将来要 id（联机对账），需要扩签名（只加不改：加第 4 个参数或加 `ids` 键） |

---

## 8. 结论档位汇总

| 结论 | 档位 | 依据 |
|---|---|---|
| `hit_test` 的方法名/参数/返回类型在 4.7.2 上注册成功并能被 GDScript 调用 | **逐位验证** | §6.3 注册日志 + §6.5 探针实跑 |
| 入参是棋盘坐标、掩膜是像素坐标、换算只发生一次 | **逐位验证** | §2.3 三条测试（含错误映射的指纹） |
| 判定本体与 `graphwar_shot_calculator::settle::resolve` 是同一条代码路径 | **逐位验证** | §4.2 结构 + §4.3 真对局等价性测试 |
| 掩膜规则（光栅化 / 点测试 / 越界算命中） | **逐位验证** | 既有：`docs/08` §6.1.2（300 随机点 0 差异、6 组光栅化 0 像素差异） |
| "障碍停弹道、士兵穿过即死" | **观测（强）** | 既有：`docs/30`（真对局的死亡点与终点）+ `tests/live_hits.rs` 12 条 |
| 客户端判定来源变成内核 | **观测** | §6.2 状态行原文、§6.4 击杀实跑、§6.5 B 组与旧近似结论相反 |
| 射手免疫 | **观测（样本内）** | §7 #2 |
| 士兵半径 0.5 是判定常量 | **推断** | §7 #1（抓包一致 + 距离反证，但没有常量地址） |
| 友军伤害 | **未知** | §7 #3 |
| 0.5 单位栅格扫描 | **推断（未采纳）** | §7 #5 |
| release 构建行为一致 | **未知** | §6.6 |

---

## 9. 复现步骤（最短路径）

```bash
# 0) 独立克隆（不要在主仓库里改）
git clone <主仓库> <work>/repo && cd <work>/repo && git checkout agent/hit-test
export CARGO_HOME=<ws>/.cargo-home CARGO_TARGET_DIR=<work>/target

# 1) 库侧
cargo test --offline            # 期望 258 passed

# 2) 客户端侧（三句缺一不可）
cargo build --offline
cp "$CARGO_TARGET_DIR/debug/libgw2_rebuild.so" client/          # .so 不进仓库
godot --headless --path client --import                         # ← 少了这句内核静默不加载
bash client/run.sh selftest    # 期望 SELFTEST 93 passed, 0 failed

# 3) 看判定来源（状态行）
#    kernel 在场 → 判定来源：kernel
#    把 client/*.so 挪走再跑 → 判定来源：preview ／ 内核无 hit_test → 几何近似
```

---

## 10. 这次碰了哪些文件

| 文件 | 性质 | 行数 |
|---|---|---|
| `src/_scaffold/api.rs` | 新增 `hit_test` 的纯逻辑 + FFI + 16 条测试；`write_result` 拆出通用 `write_dict` | +847 / −24 |
| `src/graphwar_shot_calculator/settle.rs` | 新增 `resolve_points`（规则体），`resolve` 改为委托 | +23 / −2 |
| `src/_scaffold/gdext.rs` | **+1 行**注册调用 | +1 |
| `client/src/core/core_bridge.gd` | 新增 `hit_test` 桥 + 近似分支 + 契约注释 | +102 / −6 |
| `client/src/screens/play_screen.gd` | `_resolve_shots` 换内核；`fire()` 的 shot 字典加一个键 | +27 / −12 |
| `docs/120-hit-test.md` | 本文 | 新增 |

**没有碰**：`src/graphwar_turn_manager.rs`、`src/graphwar_function_evolutionator.rs`、`src/_scaffold/protocol.rs`、`src/game.rs`、
`src/graphwar_game_server.rs`、`docs/INDEX.md`、`docs/HANDOVER.md`、`docs/108`（那是客户端交付物，
留给客户端负责人一并更新）、`client/run.sh`、`client/tools/selftest.gd`。
