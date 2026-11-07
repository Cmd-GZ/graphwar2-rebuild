# 156 · `docs/16` 的逐轮"接线 / 接节点"记录（§42–§51，原样搬迁）

> **出处**：摘自 [`docs/16-client.md`](../16-client.md) §42–§51，2026-11-08 搬迁，**内容未改**。
> 搬迁范围 = 该文件搬迁前（`2e69ed5e`）的**第 7719–9857 行**（§42–§51，10 节，2,139 行），逐字复制；
> 只在本文件开头加了这一段出处说明，**其余一个字节都没动，节号也不变**（archive 里仍叫 §42–§51）。
> 自证：`git show 2e69ed5e:docs/16-client.md | sed -n '7719,9857p'` 与本文件从下面
> 第一条 `## 42.` 起的全部内容 `cmp` 相同（命令与 md5 见 `docs/154-docs-audit.md` 附录）。
>
> **怎么读**：各节里的 `cargo test` / `selftest` 条数都是**那一轮当场的读数**，不是现行基线；
> 现行基线在 [`docs/16-client.md`](../16-client.md) 文首的「条数口径」横幅
> （2026-11-08 实测：**1585 / 1669**；selftest **709** 出自 §49.7 = 本文件的 §49.7）。
> 本文里 `docs/16 §NN` 一律指**本文自己的 §NN**（同一批文字，只是搬了家）；
> 代码注释里的 `docs/16 §42`–`§51` 也照样落在这里，编号没变。
>
> 说明：`docs/154-docs-audit.md` 的 C2 一行原来写的范围是「第 42–49 节」，那是审计当时的末尾；
> 审计之后又长出 50 / 51 两节（同一批逐轮日志），一并搬入，避免主文档留两条尾巴。

---

## 42. 曲线真的画出来了：`0x456df0` 是**取值器**不是绘制，喂点的是 `GraphwarPlane::draw`（2026-11，任务 13）

**一句话**：`0x456df0` 是 `GraphwarFunctionManager::get_active_functions_on_tick`，签名
`fn(&self, tick: u32) -> Vec<u32>`——**它一个图元都不画**，只把"本 tick 还在场的 player id"挑出来；
真正往 `Line2D` 里塞点的是 `GraphwarPlane::draw`，调用点 `0x4b69ec`（全二进制**唯一**一个
`FunctionDrawer::update_function` 调用点）。本轮把那段循环接进
`src/_scaffold/graphwar_plane_class.rs`，headless 实测 `Line2D.get_point_count()` =
**60 → 90 → 100**（进树走 `_draw` 时 **30**），新脚本 **96 条断言全过**（退出码 0）。

> 档位口径同 §40：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / 本脚本的条数**只是回归保护**。

### 42.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/graphwar_plane_class.rs` | 改 | +468/−2 | 喂点循环 `feed_functions()`（`_draw` 里调）+ 活跃谓词 + `new_len` 算式 + 最小函数存储 + 3 条单测 |
| `src/_scaffold/function_drawer_class.rs` | 改 | +8/−2 | `update_function` / `remove_function` 加 `pub`（让棋盘类直接调，**行为一行没变**）+ 注释 |
| `client/tools/curve_on_screen_check.gd` | **新增** | 400 | headless 验收脚本（**96 条断言**） |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | 2 行 | `src_fns` 2257→**2277**、`src_tree_sha256` 跟着变；**527/527 与每模块行一行没变** |
| `docs/16-client.md` | +本节 | — | — |

`src/function_drawer.rs` / `src/graphwar_plane.rs`（纯逻辑）、`src/lib.rs`、`src/_scaffold/mod.rs`、
别的 `*_class.rs`、`client/src/**`、`dispatch/` **一行没动**；既有断言一条未改
（`function_drawer_check.gd` 132 条、`draw_classes_check.gd` 235 条，复跑仍全过）。

### 42.2 `0x456df0` 到底是什么（**观测**；`docs/02` §15.2.1 那行"推断"就地升级）

| 项 | 值 | 判据 |
|---|---|---|
| 名字 | `GraphwarFunctionManager::get_active_functions_on_tick` | `fixtures/name_to_addr.tsv:435`，唯一锚 `src/graphwar_function_manager.rs:208:37` |
| 大小 | `0x190` | 到 `0x456f80`（同族 `..._no_fade`）的距离 |
| 签名 | `fn(&self, tick: u32) -> Vec<u32>` | `rdi` = sret 输出槽、`rsi` = self、`edx` = tick（`0x456dff` 把它存进 `0xc(%rsp)`） |
| 返回什么 | 命中者的 **4 字节 player id** 组成的 `Vec` | `0x3be619` 的 `mov -0x78(%rcx),%eax` = bucket+0 的 key，随后 `mov %ecx,(%rax)` 写进新分配的 4 字节槽 |
| 谁调它 | **只有** `0x4b6339`（`GraphwarPlane::draw` 体内） | 见 §42.3 的一条命令 |

体内三件事（`0x456df0`–`0x456e8f`）：**锁**（`0x456e12` 的 `lock cmpxchg` 一条 Mutex，state 在
`[self+0x18]` 那个 `Arc` 的 `+0x10`）→ **遍历**（`RawTable` 的 ctrl 在 `+0x18`、bucket_mask 在
`+0x20`，与 `FunctionDrawer` 的表同一个 hashbrown 形态）→ **收 id**（`0x456e8f` 调 `0x3be540`，
把命中的 id 逐个 push 进 `rdi` 指的 Vec）。

过滤逻辑被内联在 `0x3be540` 里，而 `0x3be540` 在整份 `.text` **只有一个调用者**（`0x456e8f`）
⇒ 那段谓词**确定为**本函数的。三条（bucket 步长 `0x78`，key 在 `+0x00`）：

```text
0x3be5e6  cmpl $0x1,-0x8(%rbp,%rcx,1)        ; (value+0x70) == 1 —— 字段语义未知
0x3be5f0  cmp  %r13,-0x28(%rcx) / je skip    ; (value+0x50) != 0x8000_0000_0000_0000（i64::MIN）
0x3be5f6  mov  %eax,%edx / sub -0x4(%rcx),%edx   ; edx = tick - active_tick（u32，value+0x74）
0x3be5fb  jbe  skip                          ; tick <= active_tick ⇒ 不在场（**严格**大于）
0x3be5fd  mov  -0x18(%rcx),%rdi              ; rdi = points.len()（value+0x60）
0x3be601  lea (%rdx,%rdx,1),%r8d / shl $5,%edx / sub %r8d,%edx
                                             ; edx = 30 * d（30 = .rodata 0x19ef90）
0x3be60b  add  $0x708,%edi                   ; points.len() + 1800
0x3be611  cmp  %edi,%edx / jae skip          ; 30*d >= len+1800 ⇒ 不在场
```

⇒ 一条曲线"在场"的窗口 = 揭示期（`len/30` 个 tick）+ **1800/30 = 60 tick** 的尾巴
（那 60 tick 就是淡出窗，对应 `get_function_fade_on_tick`；那个函数**没有 x86 落点**，见 §42.5）。

### 42.3 谁调 `update_function`、什么时候、传什么（**观测**，全 `.text` 唯一）

```text
objdump -d .reference/steam/2026-08-28/libgraphwarrust.so | grep -E 'call +(456df0|498c70|498b70|499050|4992b0) <'
  4b6147  call 499050      ; FunctionDrawer::update_transform     ← 在 GraphwarPlane::draw
  4b6339  call 456df0      ; get_active_functions_on_tick         ← 唯一调用点
  4b6418  call 498b70      ; FunctionDrawer::remove_function      ← 唯一调用点
  4b69ec  call 498c70      ; FunctionDrawer::update_function      ← **唯一调用点**
  4b6a0e  call 4992b0      ; FunctionDrawer::update_function_fade ← 唯一调用点
```

**⇒ 结论：原版只有 `GraphwarPlane::draw`（`0x4b5df0`）喂点。** 顺序与实参（`0x4b6317`–`0x4b6a13`）：

| # | 地址 | 干什么 | 实参从哪来 |
|---|---|---|---|
| 1 | `0x4b6317` | `mov 0x58(%rbp),%ebx` | 整数 tick（tick handler 上的 **u32** 字段） |
| 2 | `0x4b631a` | `call 0x519530` = `get_fractional_tick` | 返回 f64，存 `0x130(%rsp)` |
| 3 | `0x4b6339` | `get_active_functions_on_tick(&mut out, self, tick)` | out = `0xe0(%rsp)`；随后按 4 字节步长遍历它（`0x4b6562` 的 `mov (%r14),%ebp`） |
| 4 | `0x4b65d9` | 在函数管理器里按 id 查一条 | bucket 步长 `0x78`；点数组 = **`Vec<(f64,f64)>`**（ptr `+0x58`、len `+0x60`） |
| 5 | `0x4b67a0`–`0x4b6812` | `new_len = min(len, trunc(30*(fractional_tick - active_tick)))` | `30.0` = `.rodata 0x19ef90`（逐位） |
| 6 | `0x4b69ec` | `update_function(id, points, len, new_len, color)` | `rdi`=绘制器 `self+0x80`、`esi`=id、`rdx`=点指针、`rcx`=len、`r8`=new_len、`r9d`=颜色 |
| 7 | `0x4b69fe` | `call *0x9bbd08`（`get_function_fade_on_tick`？） | `(self+0x18, id, tick)`，返回值直接进下一步 |
| 8 | `0x4b6a0e` | `update_function_fade(id)` | 参数 = 上一步的返回值 |
| 9 | `0x4b6418` | `remove_function(id)` | 遍历"另一个集合"，不在某个 `HashSet` 里就删 |

两个附带结论：

* **三次 `FunctionDrawer` 调用都打在同一个绘制器上**：`0x4b6132` 的 `lea 0x80(%rax),%rdi`
  把 `self+0x80` 存进 `0x90(%rsp)`，`0x4b69d4` / `0x4b6a04` 都从那里取。
* **颜色是入参、每帧重查**：`0x4b6825` / `0x4b68f3` 两次 hashbrown 查表（第二张表的值取
  3 个字节拼 `r9d`）。
  ⚠️ **2026-11 就地更正（§48.3）**：这句原来接着写"两张表分别来自 `get_team_overview`
  （`0x4be6e0`）在 `0x4b5fd3`/`0x4b60e3` 的两次返回值"——**是错的**（当时是推断）。
  `get_team_overview` 返回的是 **`u32` 的 4 个布尔位**（`0x4beb95`–`0x4bebb6` 的拼装），
  只喂 [`flip`] 那一个判定；两张颜色表本身是**游戏状态里两个 `HashMap`**
  （`0x4b6825` 的表：key `u32` → value `u32`；`0x4b68f3` 的表：key `u32` → value
  **3 字节 `GraphwarColor`**，bucket stride 8 = 4+3+padding），由 `0x46a770` / `0x46b5d0`
  两个借用器从 `[r12+0x238]` 那个对象里借出来。队色池本体在
  `init_team_colors`（x86 **`0x465070`**）里，见 §48.3。

### 42.4 落码：本类怎么接的

| 原版 | 本类 | 档位 |
|---|---|---|
| `get_active_functions_on_tick` 的三条谓词 | `is_active_function()`（第 3 条；第 1/2 条见 §42.5） | **观测** |
| `new_len = min(len, trunc(30*(frac-active)))` | `revealed_points()`，`30.0` 逐位 | **逐位**（常量）/ **观测**（算式） |
| `update_function(id, points, len, new_len, color)` | `FunctionDrawer::update_function(id, points[..new_len], color)` | 传前缀**等价**（`append_window` 仍是唯一判定，同 §40.4b） |
| `remove_function(id)`（`0x4b6418`） | 不在场就删表 + `remove_function` | **观测**（调用点）/ 集合形状**推断** |
| 每帧喂 | `draw()` 末尾调 `feed_functions()`（位置与原版一致：画完棋盘框之后） | **观测** |
| 喂给哪个 `FunctionDrawer` | 固定第 0 个子节点 | **推断**（"`self+0x80` 是 3 个里的第一个"没有直接证据） |
| 让 `_draw` 每帧跑 | `_process` 里"有曲线正在揭示就 `queue_redraw()`" | **我方选择**（原版 `process` 无此调用，见 §42.5） |

**替身（必须写清，别当成复原）**：`GraphwarFunctionManager` **还没有 `src/` 文件**
（`docs/coverage-matrix.tsv` 第 36 行把它算进 `src/game.rs`），而本轮只许改这两个 `*_class.rs`
⇒ 棋盘类里先放一份"最小函数存储 + 入口"，方法名照原版的语义起：

```text
GraphwarPlane.set_function_points(id, points, active_tick, color) -> 存下的点数
    # 覆盖 GraphwarFunctionManager::set_function_points(0x456810) + set_function_active_tick(0x456b80) + 队色
GraphwarPlane.set_fractional_tick(tick) -> tick
    # 覆盖 GraphwarTickHandler::get_fractional_tick(0x519530) 的读取（原版每帧自己读）
GraphwarPlane.feed_functions() -> 本帧追加的点数
    # = 原版 draw 尾部那段循环；headless 直接调它（`_draw` 在 headless 下不一定被调）
```
`points` 是**扁平 f64 数组** `[x0,y0,x1,y1,…]`：原版的点就是两个 f64（16 B/点），
用 `PackedVector2Array` 会在入口就掉到 f32。

### 42.5 还没做 / 未知（诚实清单）

1. **点的生产者没复原**。`set_function_points`（`0x456810`）的调用者是 `0x3e44d4`
   （落在一个 70 项跳转表的事件分派函数里，前面 `0x3e44aa` 先调 `0x3cae50` 把点表按 x 区间
   就地压实；**那个函数的模块归属未证**——`fixtures/module_map.tsv` 只给出
   `graphwar_game_state_manager.rs` 的锚区间 `0x3caa20..0x4ac440`，落在区间内不等于归属）
   与 `0x471a88`；**谁算出那张原始点表**没继续挖。
   ⇒ 现在点必须由调用方注入，**这是本轮最大的缺口**（比"喂点"本身更靠上游）。
2. **`get_function_fade_on_tick` 没有 x86 落点**：`0x4b69fe` 走的是 GOT 槽 `0x9bbd08`（方法指针
   调用），alpha 的算式未知 ⇒ `update_function_fade` **没接**（不发明一个淡出曲线）。
3. **谓词第 1/2 条没做**：`value+0x70 == 1` 与 `value+0x50 != i64::MIN` 两个字段的语义未知
   （可能是 `FunctionState` 的某个变体与一个 `Option`），本类不猜。
4. ~~**`update_transform`（`0x4b6147`）仍未接**~~ ⇒ **已解出并接上（2026-11，§48.2）**。
   第三实参**不是 `Vector2`，是一个 `f32` 标量** `s = w/50`：被调者用 `pxor` 与
   `.rodata 0x19e880`（四个 `-0.0`）把 `s` 的符号位翻出来当"第二个分量"
   （`0x499088`/`0x499092` 存两份，`0x499232`–`0x499241` 各取一份喂 `set_scale` 的
   两个 f32）⇒ `set_scale` 收到 `flip ? (-s,-s) : (s,-s)`。
   旧文说"`MOVSS` 清高 lane ⇒ `Vector2(w/50,0)` 会把线压扁、构造没解全"——那个矛盾**不存在**，
   是把标量当成了 `Vector2` 的 y 分量。三个实参与 `flip` 位见 §48.2（逐指令）。
5. **客户端还没用真类**：`client/src/play/plane_view.gd` 是照 `.tscn` 重写的 GDScript 仿制品
   （自己 `draw_line`），`GraphwarPlane` 目前只在 `client/tools/*.gd` 里被实例化
   ⇒ **真机上眼前仍然看不到 Rust 侧的曲线**，要等播放屏改用真类（不在本轮允许改的文件里）。
6. `_process` 里"揭示中就 `queue_redraw`"是我方补的（原版 `process` = `ObstacleDrawer::update` ×2
   + `get_team_overview`，没有该调用）⇒ **谁让平面每帧重画未解**。
7. `ObstacleDrawer` 仍不存在；`FunctionDrawer::draw`（arm64 `0x3911b4`）x86 侧仍未定位（§40.5 照旧）。

### 42.6 验收（可从干净树重跑）

```text
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                    # 3
"$GODOT" --headless --path client --import                              # 新增 .gd 后必须
"$GODOT" --headless --path client --script res://tools/curve_on_screen_check.gd ; echo "EXIT=$?"
    # CURVE_ON_SCREEN 96 passed, 0 failed / EXIT=0
"$GODOT" --headless --path client --script res://tools/function_drawer_check.gd   # 132 passed（未回归）
"$GODOT" --headless --path client --script res://tools/draw_classes_check.gd      # 235 passed（未回归）
cargo test --offline                              # 1512 passed（当轮基线；现行 1520）
cargo test --offline --features gdext-bindings     # 1586 passed（当轮；现行 1595。含本批 3 条单测）
python3 tools/coverage_matrix.py --check           # OK
```

headless 实测的 `Line2D.get_point_count()`（`points.len() = 100`、`active_tick = 10`）：

| 场景 | 追加 | `get_point_count()` |
|---|---|---|
| `fractional_tick = 12.0` ⇒ 期望 `trunc(30*2)=60` | 60 | **60** |
| `fractional_tick = 13.0` ⇒ 期望 `trunc(30*3)=90` | 30（只补增量） | **90** |
| `fractional_tick = 14.0` ⇒ 期望封顶 100（120>100） | 10 | **100** |
| 同一个 tick 再喂 | 0 | 100 |
| `tick = 73`（d=63）⇒ `30*63=1890 < 1900` 仍在场 | 0 | 100 |
| `tick = 74`（d=64）⇒ `30*64=1920 >= 1900` 出队 ⇒ `remove_function` | 0 | 线被删（`function_count() == 0`） |
| **进树**（`_process` 请求重画 ⇒ `_draw` 里喂）`tick=11`、`active=10` | 30 | **30** |

「进树 ⇒ 30」那一条是本轮唯一**走完整条路径**的断言：`_process` 请求重画 → 引擎调 `_draw` →
`draw()` 里 `feed_functions()` → `FunctionDrawer::update_function` → `Line2D::add_point`。
期望值在 GDScript 里按反汇编算式**独立重写**（不调 Rust 的 `revealed_points`），
常量 `30.0` / `1800` 也写死在脚本里 ⇒ Rust 那边改常量脚本会红。

## 43. 真 `GraphwarPlane` 进场景：5314 点的真实弹道由 `_draw` 一条条喂进 `Line2D`（2026-11，任务 18）

**一句话**：新增 `client/tools/plane_on_screen.tscn`（一个 `Control` + 一个**真节点** `GraphwarPlane`）
+ 接线脚本 + 验收脚本，把**一条真实弹道**（内核 `GW2Rebuild.path_calculate("cos(x)", …)` 算出的
**5314** 个点）按原版"每 tick 露 30 点"喂进棋盘的第 0 个 `FunctionDrawer`，曲线**真的画在屏幕上**。
headless 实测 `Line2D.get_point_count()`：30 → 60 → … → 5310 → **5314 == 喂进去的点数**；
`_draw` 被引擎调用 **7** 次、`draw_rect` 同样 7 次；Xvfb 截图里数出 **645** 个 `#7D6800`
像素（真帧量测的弹道色）。**76 条断言全过**（退出码 0）。`client/src/**` 一行没动。

> 档位口径同 §40 / §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 真实报文 / 真机真帧；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / 本脚本的条数**只是回归保护**，不是复原证据。

### 43.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `client/tools/plane_on_screen.tscn` | **新增** | 39 | 场景：`PlaneOnScreen`（Control）+ 子节点 `GraphwarPlane`（**`type="GraphwarPlane"`**，两个 `#[export]` 属性写成显式值） |
| `client/tools/plane_on_screen.gd` | **新增** | 359 | 接线：取真实弹道 → `set_function_points` → 每帧推 tick；`update_transform` 把 `Line2D` 摆进棋盘坐标系；可选截图模式（`--shot=`） |
| `client/tools/plane_on_screen_check.gd` | **新增** | 321 | headless 验收（**76 条断言**）：场景加载 / 类身份 / 点数 / 逐 tick 揭示 / `_draw` 次数 / 点的内容 / 屏幕位置 |
| `src/_scaffold/graphwar_plane_class.rs` | 改 | +34/−6 | **必需**（理由见 §43.3）：`FunctionEntry` 加 `fed` 记账；`has_revealing_function()` 的判据从"应喂 < 全长"改成"应喂 > 已喂" |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | 2 行 | 只动 `src_tree_sha256`（`src_fns` 2311 不变：本批没加 `fn`） |
| `docs/16-client.md` | +本节 | — | — |

`client/src/**`、`client/scenes/**`、`client/*.tscn`、`client/run.sh`、`src/function_drawer.rs` /
`src/graphwar_plane.rs`（纯逻辑）、`src/lib.rs`、别的 `*_class.rs`、`src/game.rs` **一行没动**；
既有断言一条未改（本轮复跑：`curve_on_screen_check.gd` 96、`function_drawer_check.gd` 132、
`draw_classes_check.gd` 235、`entity_drawer_check.gd` 72、`base_classes_check.gd` 117、
`signal_classes_check.gd` 155、`spike_class_check.gd` 23、`gdext_check.gd` 24、
`assemble_check.gd` 183、`recovered_view_check.gd` 57，**全 0 失败**）。

### 43.2 验收：76 条断言 + 数字（**观测**）

```text
$GODOT --headless --path client --script res://tools/plane_on_screen_check.gd
  → PLANE_ON_SCREEN 76 passed, 0 failed / EXIT=0
```

**核心一条**（本轮验收口径）：

```text
喂进去的点数（内核弹道） = 5314
Line2D.get_point_count() = 5314      ← 屏幕上真有的点数
```

逐 tick 的揭示（脚本里**一次 `feed_functions` 都不调**，点数只可能来自 `_draw` 里的喂点循环
`0x4b6339` → `0x4b69ec`；期望值按反汇编算式在 GDScript 里独立重写，不调 Rust 的
`revealed_points`）：

| tick | 独立算式 `min(5314, trunc(30·t))` | 本帧追加 | `Line2D.get_point_count()` |
|---|---|---|---|
| 1 | 30 | 30 | **30** |
| 2 | 60 | 30 | **60** |
| 3 | 90 | 30 | **90** |
| 10 | 300 | 210 | **300** |
| 60 | 1800 | 1500 | **1800** |
| 177 | 5310 | 3510 | **5310** |
| 178 | 5314（`30·178 = 5340 > 5314` ⇒ 封顶） | 4 | **5314** |
| 同一 tick 再喂 | — | 0 | 5314 |

`_draw` 的计数（`draw_call_count()`，**我方计数点**——Godot 的 `_draw` 没有信号可连）：

```text
进树后第一次断言：1（这一轮；帧时序不同可能是 3）
每推进一个 tick：+1（断言的是**严格增加**，不是固定值）
最后一次 report()：draws=7  rects=7  writes=8  processes=16  reveal_redraws=6
      ⇒ drawn_rect_total() == draw_call_count()（每次 `_draw` 都画了那条棋盘框）
```

点**内容**也钉住（不是只把计数凑上去）：第 0 / 2657 / 5313 个点与内核弹道（f32 化后）
逐点相等；`Line2D.default_color == #7D6800`；`position == (500,300)`、`scale == (20,-20)`、
`width == 0.125`；三个采样点按 `to_pixel + 翻 y` 落在 `(140,402)` / `(570.5,433.7)` /
`(999.9,395.4)`，全在 1000×600 的棋盘内。

断言分布（三组）：

| 组 | 条数 | 关键一条 |
|---|---|---|
| 类/方法表/内核/场景加载/`.tscn` 属性 | 33 | `plane.get_class() == "GraphwarPlane"`、`kernel.ping() == "pong:gdext"`（不是预览折线） |
| 逐 tick 揭示（30 → 60 → … → 5310） | 23 | 每次推进后 `Line2D` 点数 == 独立算式，且 `_draw` 次数 +1 |
| 揭完 + 点内容 + 屏幕位置 | 20 | `get_point_count() == 5314 == fed_points`、首/中/末点与弹道逐点相等 |

> ⚠️ `--headless` 下**没有真窗口**：`get_viewport_rect()` 只有 64×64（脚本里打印了），
> 所以"整条曲线在可见区域内"那一条只在**有显示器**时断言；headless 断言的是
> "按 `to_pixel` 落在棋盘坐标系里"（这一条与窗口大小无关）。截图证据见 §43.5。

### 43.3 为什么改了 `graphwar_plane_class.rs`（**必需**，附一个真 bug）

§42.5 第 6 条早就写着"**谁让平面每帧重画未解**"，本类补的是我方选择（`_process` 里
"正在揭示就 `queue_redraw`"）。本轮把它接进场景后，**这条补丁自己露出了一个死角**：

```text
判据原来是   revealed_points(tick) < points.len()
真实弹道 5314 点，tick 177 ⇒ revealed = 5310 < 5314  ⇒ 请求重画 ⇒ `_draw` 喂到 5310 ✓
                tick 178 ⇒ revealed = 5314 == 5314  ⇒ **算"揭完"** ⇒ 不再请求重画
⇒ 最后一帧的 `_draw` 永远不会来 ⇒ 剩下的 4 个点**永远喂不进去**
⇒ 验收口径"Line2D 点数 == 喂进去的点数"卡在 5310（第一次跑就是红的：5310 vs 5314）
```

改法（两处，都带注释）：

* `FunctionEntry` 加一个 **我方记账** 字段 `fed`：这个 id 已经通过 `update_function` 喂到第几点
  （原版这个数在 `FunctionDrawer` 的 value 里，即 §40.3a 的 `bucket+0x28`；本类从
  `FunctionDrawer::written()` **读不到**——那是 `#[func]`，Rust 侧没有入口）；
* `has_revealing_function()` 改成"**应喂 > 已喂**"：只要还有该露而没露的点，就请一帧重画。

档位：判据本身是**我方选择**（原版 `process` `0x4b83f0` 里没有 `queue_redraw`，§42.5 第 6 条
照旧）；改的不是复原出来的行为，是我们自己那条补丁。**副作用为零**：`feed_functions()` 的
返回值、`append_window` 的唯一判定、`update_function` 的入参形状一个都没变，
`curve_on_screen_check.gd`（96 条，含"进树 ⇒ 30 点"那条）复跑仍全过。

### 43.4 这条"真实弹道"的档位（**别当成原版真值**）

| 项 | 值 | 档位 / 出处 |
|---|---|---|
| 函数文本 | `"cos(x)"` | **观测**：`fixtures/protocol_match.jsonl` 第 20 行 `{"FunctionUpdateRequest":{"player_id":232,"function":"cos(x)"}}`（第二轮真实抓包，166 帧去重成 109 条，见 `docs/09-network.md` §1.4） |
| player id | `232` | **观测**：同一份抓包第 21 行 `{"FunctionFireRequest":{"player_id":232}}` |
| 起点 / 方向 | `(-18, -5.1)` / `+1` | **我方选择**：抓包的开火报文**只有** player_id，坐标与方向在客户端（士兵位置）⇒ 没有报文证据 |
| 点序列本身 | 5314 个点 | **推断**：由复原的 `graphwar_shot_calculator` / `graphwar_path_calculator` 算出（`cargo test` 与 C ABI 同口径），**不是**截下来的原版运行时点。缺的证据：原版 `GraphwarFunctionManager::set_function_points`（`0x456810`）的写入者 `0x3e44d4` 归属未证（§42.5 第 1 条照旧） |
| 点精度 | f32（`PackedVector2Array`）→ f64 → `PackedVector2Array` | **观测**：窄化发生在内核 API 边界（`src/_scaffold/api.rs` 的 `v2()`），与 `FunctionDrawer` 里那两条 `cvtsd2ss` 是同一处掉队（§40.4b），不额外丢精度 |
| 队色 | `0x0000687D`（= `#7D6800` = `Color8(125,104,0)`） | **观测**：真帧量测的弹道色（§12.2 ⑫，= `TEAM_COLORS[0][2]`）；打包时**低字节 = R**（§40.3h） |

### 43.5 截图（可选证据，**怎么重新生成**）

PNG 属于"二进制衍生内容"，**不入库**（`AGENTS.md` 三条硬规矩第 3 条）。想留图就放到 gitignored
目录（例如 `.agent-work/` 或 `rsc/` 之外的自建目录），命令是：

```text
# 1) 构建 + 覆盖 .so + import（§42.6 那三步）
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import

# 2) 抓一帧（需要 Xvfb / 显示器；headless 没有渲染器，拿不到图）
xvfb-run -a "$GODOT" --path client --rendering-driver opengl3 \
    res://tools/plane_on_screen.tscn -- --shot=<gitignored 目录>/plane_on_screen.png
```

本轮实测（Xvfb，`MODE_MAXIMIZED` ⇒ 窗口 2560×1566；棋盘 1000×600 居中）：

```text
SCREENSHOT path=…/plane_on_screen.png err=0 size=2560x1566 samples=150000 line_pixels=645 bg_pixels=147761
SCREENSHOT_TOP_COLORS ["0e251c=147761", "000000=1594", "7d6800=645"]
```

* `samples` = 棋盘矩形内**每 2 像素**采一次（500×300 = 150000）；
* `line_pixels=645` ⇒ 颜色距 `#7D6800` ≤ 0.12 的采样点（全分辨率约 4×645 ≈ 2580 像素）
  —— 这就是"曲线真的被画到屏幕上"的像素级证据，不是只涨计数器；
* `000000=1594` = `GraphwarPlane::draw` 画的那条棋盘框（空心黑框，`BOARD_*` 常量）；
* `0e251c` = `project.godot` 的 `default_clear_color`（按位等于原版 `#0E251C`，观测记录在 §5.1 第 ② 行）。
* 退出码：**Godot 自己退 0**；外层 `xvfb-run` 在 teardown 时会打
  `kill: (…)- 没有那个进程` 并以 1 退出（它的老毛病，与本次运行无关）。
  `ObjectDB instances leaked: DisplayServer / NativeMenu` 两条是引擎内部的，不是本场景的节点。

### 43.6 要替换现有仿制品，得动什么（清单：**5 个 `.gd` / 6 个节点位置 / 7 个缺口**）

现有播放屏的棋盘是 GDScript 仿制品（`client/src/screens/play_screen.gd` 自己 `new` 出来、
自己 `draw_polyline`）。**本轮没动它**——本任务只证明真类能在场景里画出来。

**（a）要删的 2 个 `.gd`**

| 文件 | 行数 | 它替代的是哪个原版类 | 换成 |
|---|---|---|---|
| `client/src/play/plane_view.gd`（`GwPlaneView`） | 527 | `GraphwarPlane` + `AxisDrawer` + `EntityDrawer` + `ObstacleDrawer`（+ `FunctionDrawer`） | `GraphwarPlane`（本轮验过）+ 下面 (b) 的节点 |
| `client/src/play/fire_fx.gd`（`GwFireFx`） | 261 | `GraphwarPlane::draw` 尾部的喂点循环（`0x4b6339`–`0x4b6a13`）+ 准备圈 | `GraphwarPlane.set_fractional_tick()` + Rust `_draw` 里的 30 点/tick（本轮验过） |

**（b）要存在的 6 个节点位置**（现在 1 个都没有）

| # | 原版 | 现在的仿制品在哪 | 状态 |
|---|---|---|---|
| 1 | `GraphwarPlane`（棋盘框 + 脏矩形 + 3 个 `FunctionDrawer`） | `GwPlaneView._draw` / `_draw_border` | **真类已存在**，本轮验过 |
| 2 | `FunctionDrawer`（每个 player id 一条 `Line2D`） | `GwPlaneView._draw_path` + `path_reveal` | **真类已存在**，本轮验过 |
| 3 | `EntityDrawer`（士兵 / 准备圈 / 名字） | `GwPlaneView._draw_soldiers` / `_draw_ready_rings` | 真类已存在（§33），**还没挂进播放屏** |
| 4 | `AxisDrawer`（网格线） | `GwPlaneView._draw_grid` | 真类已存在，**还没挂** |
| 5 | `EffectDrawer`（爆炸/火花） | `client/src/game/effect_layer.gd`（`GwEffectLayer`） | 真类已存在，**还没挂** |
| 6 | `ObstacleDrawer`（障碍圆 + 脏矩形） | `GwPlaneView._draw_obstacles` | **类还不存在**（§40.5 / §42.5 第 7 条） |

**（c）要改的 2 个 `.gd` + 1 个验收脚本**

* `client/src/screens/play_screen.gd`（2610 行）：**44 处** `_plane.*` 调用（12 个方法/字段名：
  `set_path`×5 / `clear_path`×8 / `set_targets`×6 / `set_obstacles`×4 / `set_soldiers`×2 /
  `set_show_names`×3 / `set_axis_mode`×3 / `path_color`×5 / `path_points`×2 / `queue_redraw`×2 /
  `add_child`×3 / `soldiers`×1）+ 3 处节点创建（`_plane = GwPlaneView.new()` 在第 301 行、
  `_fire_fx` 第 306–309 行、`_effects` 第 311–314 行）；
* `client/src/game/effect_layer.gd`：它靠 `plane.plane`/坐标换算画特效，换成 Rust 的
  `EffectDrawer` 后这段映射要去掉；
* `client/tools/selftest.gd`：**18 处**读 `plane.path_points` / `plane.path_reveal` 的断言
  （回放/开火的既断言）要改成读 `GraphwarPlane` 的方法。

**（d）还差的 7 个东西**（缺证据，不是缺代码）

1. **点的生产者**（§42.5 第 1 条）：`set_function_points`（`0x456810`）的写入者 `0x3e44d4`
   归属未证 ⇒ 本轮只能由场景注入；**（仍在，本任务未碰）**
2. ~~**tick 源**~~ ⇒ **已接（§48.1）**：`get_fractional_tick`（`0x519530`）的算式逐条解出并落进
   `graphwar_tick_handler::GraphwarTickHandler`（`tick + ((now-base)-accum)/0.016666666`），
   `GraphwarPlane` 自己持有一个 tick handler 并按帧调它（`0x4b6317` 的 u32 tick 取 `M+0x40`）。
   剩下的**只是接线**：原版那个对象在游戏状态里（`draw` 里 `mov r14,[r12+0x238]`），
   本类还没从那儿取 ⇒ 自己 `new` 一个 + `set_fractional_tick()` 当摆钟口（**我方**）；
3. ~~**`update_transform` 的三个实参**~~ ⇒ **已解出并接上（§48.2）**：
   `x = w/2`、`y = h/2`、`scale = w/50`（f32 标量，`.rodata 0x1b9fc8`）、
   `flip` = `get_team_overview` 的位组合（`(flags≥2^24 && !bit16) || ((flags&0x101)==0x100)`）。
   最终 `set_scale` 的两个分量 = `flip ? (-s,-s) : (s,-s)`。**不再是我方选择**；
   唯一还是我方的是 `flip` 的来源（拿导出属性 `mirror_x` 顶替那个位组合）；
4. **淡出**：`get_function_fade_on_tick` 没有 x86 落点（走 GOT 槽 `0x9bbd08`）⇒ 没接；**（仍在）**
5. **`z_index` 初值未知**（§40.3f）；**（仍在）**
6. **`FunctionDrawer::draw`**（arm64 `0x3911b4`）x86 侧仍未定位（不影响曲线：曲线是 `Line2D`）；**（仍在）**
7. ~~**棋盘底色/网格/队色表**（`get_team_overview`，`0x4be6e0` 的两张表）~~ ⇒ **表解出来了，
   归属也纠正了（§48.3）**：
   * `get_team_overview`（`0x4be6e0`）返回的是 **`u32` 的 4 个布尔位**（bit 0/8/16/24），
     **不是两张表**；`draw` 里两处调用（`0x4b5fd3`/`0x4b60e3`）只用它算 `flip`；
   * 真正的"两张表" = `init_team_colors`（x86 **`0x465070`**，arm64 `0x49c660`）里的
     **两个 6 色 `Vec<GraphwarColor>`**（键 = 队号 0/1），12 个颜色在 x86
     `0x4650a3`–`0x465297` 逐条 `mov word` + `mov byte` 落库，与 `docs/10` §15.3 的 arm64
     复算**一字不差**；表已落在 `src/graphwar_color_manager.rs` 的
     `TEAM1_COLORS`/`TEAM2_COLORS`，本类用 `team_color_pool(team)` 暴露出来（验收断言 12 个分量）；
   * **棋盘内部的白底不是队色**：它是 `AxisDrawer::draw` 的 `draw_rect(filled=true,
     Color(1,1,1,1), rect(-25,-15,50,30))`（`0x4e069f`–`0x4e06ca`）。
     `AxisDrawer` 节点还没实现 `draw` ⇒ 本类加了导出属性 `board_fill`（**默认关**）
     照那组实参补画白底；**播放屏要"不再是工程底色"就得把它置真**（或等 AxisDrawer 补 `draw`）。

### 43.7 还没做 / 未知（诚实清单）

* **客户端播放屏仍是仿制品**：本轮只证明"真类 + 真弹道 + 真喂点路径能画出来"（§43.6 是路线图）。
  §42.5 第 5 条那句"客户端还没用真类"**对播放屏仍然成立**，只在这里补一条：
  现在**存在**一个能跑的场景（`client/tools/plane_on_screen.tscn`），不再是"只能 headless 调方法"。
* **`_process` 里那条重画策略仍是我方选择**（§43.3 改的是它的一个死角，不是复原它）。
* **`fed` 是第二份记账**：原版在 `FunctionDrawer` 的 value 里（§40.3a），本类在棋盘侧又记一份。
  两份**不会分叉**（都只由 `update_function` 的追加决定），但这是"本类没有 `FunctionDrawer`
  的 Rust 入口"造成的重复；等 `FunctionDrawer` 的访问器公开后应删掉这一份。
* ~~**截图里的棋盘内部没有网格/底色**（原版有；`AxisDrawer` 与 `GraphwarPlane::draw` 的
  `draw_set_transform` 实参都还没定死）~~ ⇒ **实参定死了（§48.2）**，底色也定位了
  （`AxisDrawer::draw` 的白底矩形，§48.3）；但 `AxisDrawer` 节点仍没实现 `draw`，
  所以那张截图里**仍然**没有网格；底色现在可以由 `GraphwarPlane.board_fill = true` 补上。
* **`--headless` 的视口是 64×64**（不是 1600×900）：任何"像素位置"类断言在 headless 下
  只能测到"棋盘坐标系里对不对"，测不到"窗口里对不对"；后者靠 §43.5 的截图。

### 43.8 复现（三分钟）+ 回归数字（本分支实测）

```text
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
"$GODOT" --headless --path client --import                              # 新增 .gd/.tscn 后必须
"$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd ; echo "EXIT=$?"
    # PLANE_ON_SCREEN 76 passed, 0 failed / EXIT=0
cargo test --offline                              # 1520 passed, 0 failed（基线同数）
cargo test --offline --features gdext-bindings     # 1595 passed, 0 failed
python3 tools/coverage_matrix.py --check           # OK
```

`bash client/run.sh selftest`（无 assets）：本环境 **530 passed / 2 failed / 25 skipped**
（退出码 1）。⚠️ 任务书写的基线是 **606 / 2 / 23**，**这里复现不出来**：把
`src/_scaffold/graphwar_plane_class.rs` 退回 `418319d`、重编 `.so` 再跑一遍，数字**逐字相同**
（530/2/25）⇒ 差异来自环境（缺 `assets` / 缺回放样本：日志里 `SKIP 回放：约定的样本 … 不在`），
**不是本批改动造成的**。两条 FAIL 也是既有的（Skins 页皮肤、Cancel/Start 按钮宽度）。

## 44. 5 个 manager 从"有键"到"能交互"：装配 + 事件流 + 一次最小交互（2026-11，任务 19）

**一句话**：§41 让 22 个 manager **进**了容器，但**没有一个是能调的**。本节挑 5 个
（`life` / `target` / `obstacle` / `function` / `tick_handler`）补上
**构造 + 注册 + 按类型取回 + 一次最小交互**，并把它们的**事件流**（订阅什么、发什么）
逐条指到 x86 指令；`assemble_check.gd` 从 183 条扩到 **199 条**，
新增的 16 条里有 27 个"交互后读回"的值，与 `root_wiring.rs` 的单测**逐字相同**。

> 档位口径同 §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / 本脚本的条数**只是回归保护**。

### 44.0 先说"没做到"的部分（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| `GraphwarFunctionManager`（32 字节）的**字段** | **一个都没解** | **未知** |
| `GraphwarObstacleManager`（80 字节）的**字段** | **一个都没解** | **未知** |
| `GraphwarTickHandler`（72 字节）的字段 | 只落了**有地址证据**的几个（见 §44.2），**没有**按字节复原 | 部分**观测** |
| 这 3 个槽的档位 | **仍然是 `stub` / `impl_type = "—"`**（上一轮已经进了逐槽断言，本轮不改既有断言）→ ⚠️ **§47 已改**：`TickHandler` / `ObstacleManager` = `wired`，`FunctionManager` = `layout`，`impl_type` 报真类型路径 | 我方口径（§47.2 重判） |

⇒ **`stub` 的含义就地更正**：上一轮它是"**零行为**的空占位"，本节起它是
"**原版结构未复原**"（其中 3 个现在**能交互**）。两件事分开验：
档位看 `module_slot_status`，能不能交互看 `manager_probe_selftest`。

### 44.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_life_manager.rs` | +接线层 | 3 条订阅名 + 5 个判别值 + `set_alive` / `set_queued_death` / `get_alive` / `get_death_tick` / `get_all_entities` / `handle_tick_event` / `handle_entity_removed_event` / `send_all_info_events` + `outbox` + 4 条单测（纯逻辑 `set` / `status` / `take_queued_death` **一行没动**） |
| `src/graphwar_target_manager.rs` | +接线层 | `handle_new_connection_event`（判别值闸 5）+ `take_outbox` / `outbox_len` |
| `src/graphwar_tick_handler.rs` | +类型 | `GraphwarTickHandler`（累加器 / `current_tick` / `tick()` / `handle_connected_to_server_event` / 接已有 `tick_at`）+ 5 条单测（两个自由函数**一行没动**） |
| `src/_scaffold/root_wiring.rs` | 改 | 3 个槽从零字段变成带状态（`TickHandlerSlot(GraphwarTickHandler)` / `FunctionManagerSlot(FunctionTable)` / `ObstacleManagerSlot(ObstacleTable)`）+ 5 个按类型取回口 + `interaction_selftest` + 5 条单测 |
| `src/_scaffold/graphwar_root_node_class.rs` | **+2 个 `#[func]`** | `manager_probe_selftest()` / `manager_probe(name)`——**我方自造**的只读观测口，原版**没有**（见 §44.5 的边界） |
| `client/tools/assemble_check.gd` | +16 条 | 27 条交互值 + 幂等 + 反面（桩仍报 stub）+ 未知名字（⚠️ §47 起反面段断的是**四档**，5 个包装槽里 4 个不再是 `stub`） |
| `docs/16-client.md` | +本节 | — |

❌ `src/game.rs` / 别的 manager / `client/src/**` / `src/lib.rs` / 任何既有断言 **一行没动**。

### 44.2 5 个 manager：订阅什么、发什么（全部 `观测`，x86 2026-08-28）

复算工具：`../.venv/bin/python tools/addr_probe.py dis <vaddr>`（本机 `capstone`，离线）。

| manager | 原版 `new` | 订阅（按调用序） | 发出的事件（tag 写在哪条指令） | 本轮接上的入口 | 档位 |
|---|---|---|---|---|---|
| `graphwar_life_manager` | `0x454790` | `new_connection_event`(5) / `entity_removed_event`(14) / `tick_event`(0) | `LifeInfo`(27) @`0x454c52`（`set_alive`）、@`0x4542af`（`send_all_info_events`）；`LifeQueuedDeath`(28) @`0x453f6d`（`set_queued_death`） | `set_alive` `set_queued_death` `get_alive` `get_death_tick` `get_all_entities` `send_all_info_events` `handle_tick_event` `handle_entity_removed_event` | 订阅/发 **观测** |
| `graphwar_target_manager` | `0x465910` | `new_connection_event`(5) / `entity_removed_event`(14) | `TargetInfo`(23)（§41 前的逐位结论不变） | `add_target` `contains` `handle_new_connection_event` `handle_entity_removed_event` | 订阅 **观测**（与既有 `逐位` 一致） |
| `graphwar_obstacle_manager` | `0x3f2360` | **只一条** `tick_event`(0) | `AddObstacle`(40) @`0x3f1876`、`DestroyObstacle`(41) @`0x3f1c09`、`QueueDestroyObstacle`(42) @`0x3f22b2`、`ClearObstacles`(39)（`clear_obstacles`） | `add_obstacle` `is_obstacle` `queue_destroy_obstacle` `destroy_obstacle` `clear_obstacles` `handle_tick_event` | 订阅/发 **观测**；延迟 165 tick **推断** |
| `graphwar_function_manager` | 内联（`0x5236e0` 由 `0x517b9f` 调，紧跟 `add_module` `0x469db0`） | **不订阅**（无 `subscribe_to_events_event_handler`，构造体内无 `0x3d38f0` 调用） | **不发**（四个 `set_*` 体内无 `0x3d3530` 调用） | `set_function` `get_function` `get_function_text` `set_function_active_tick` `set_function_points` `get_active_functions_on_tick` | **推断**（正对照见 §44.3） |
| `graphwar_tick_handler` | `0x519690` | **只一条** `connected_to_server_event`(7) | `TickEvent`(0)（`tick()` @`0x51988b`，载荷 = `current_tick`）、`TickRequest`(1)（`handle_connected_to_server_event` @`0x51964c`；以及 `tick()` 里 `current_tick % 60 == 0` 时 @`0x5198de`） | `tick` `handle_connected_to_server_event` + 接已有 `tick_at` | 订阅/发 **观测** |

三条**顺带更正/新证**：

1. **§41.6 那行"21 号之前 3 条 `subscribe_to_event` 订阅的是谁未解"⇒ 已解**：
   `0x5179bb` / `0x517a0e` / `0x517a5e` 三条订阅的是
   `connected_to_server_event`(len `0x19`) / `player_info_event`(len `0x11`) /
   `entity_removed_event`(len `0x14`)，正好是 `GraphwarLocalPlayerManager` 的
   `handle_connected_to_server` / `handle_player_info` / `handle_entity_removed_event`
   三个处理器 ⇒ **它们属于 3 号 `GraphwarLocalPlayerManager`（构造被内联）**，不是 `TickHandler`
   （`TickHandler` 的订阅在它自己的 `new` `0x519690` 里，只有 1 条）。
2. **`GraphwarTickHandler::new` 的实参**：`0x519690` 里 `M+0x00..0x0f` = `SystemTime::now()`
   （call `0x8f3a10`）、`M+0x28 = 0x3b9aca00`（1e9 ns）、`M+0x38` = 传入的事件队列
   `Rc`、`M+0x40 = 0`（`current_tick`）。⇒ 本层的 `base_tick` / `base_ms` 是**我方形状**
   （把"壁钟基准"换成抓包基准，好接已有的 `tick_at`）。
3. **`tick()` 的步长是 1/60 秒**：`0x51986c` `add eax, 0xfe502a` = **16 666 666 ns**
   （截断值，不是四舍五入的 16 666 667），溢出路径的串是
   `'overflow when adding durations'`（`Duration` 的加法）。那个"判 60 整除"的分支是
   幻数 `imul 0xeeeeeeef` + `ror 2` + `cmp 0x4444444`。

### 44.3 负结论的正对照（`AGENTS.md` 合并纪律 A-4）

`function_manager` 那两条"不订阅 / 不发"是**负结论**，所以先证明扫法有效：同一条命令
`addr_probe.py dis <vaddr>` 在另外 4 个 manager 的 `new` 体内**都**能看到
`call qword ptr [...]; slot 0x9bac88 -> 0x3d38f0`（= `EventHandler::subscribe`）
和它的 `lea rsi,'<事件名>'` + `mov edx,<len>`：

```text
life   0x454790 → 0x454812 / 0x454867 / 0x4548b8 三条 subscribe（+3 条事件名 lea）
target 0x465910 → 0x465995 / 0x4659e6 两条
obstac 0x3f2360 → 0x3f23f0 一条（'tick_event', len 0xa）
tick   0x519690 → 0x519762 一条（'connected_to_server_event', len 0x19）
func   0x5236e0 → **零条**
```

发事件同理：`GraphwarEventQueue::add_event` 是 `slot 0x9bac80 -> 0x3d3530`，
在 `set_alive` / `send_all_info_events` / `set_queued_death` / `add_obstacle` /
`destroy_obstacle` / `queue_destroy_obstacle` / `tick` / `handle_connected_to_server_event`
里**都**有；在 `set_function`(0x455e30) / `set_function_points`(0x456810) /
`set_function_active_tick`(0x456b80) 里**零条**。

### 44.4 落码：容器里现在装的是什么

```text
TickHandlerSlot(GraphwarTickHandler)     # graphwar_tick_handler.rs（原版模块文件）
FunctionManagerSlot(FunctionTable)       # root_wiring.rs（我方形状，原版字段未解）
ObstacleManagerSlot(ObstacleTable)       # root_wiring.rs（我方形状，纯逻辑复用 plane.rs）
LifeState                                # graphwar_life_manager.rs（原类型 + 接线层）
GraphwarTargetManager                    # graphwar_target_manager.rs（原类型 + 接线层）
```

新增的取回口（都走 `get_module::<M>()`，不是查表）：`tick_handler()` / `life_manager()` /
`target_manager()` / `function_manager()` / `obstacle_manager()`。

`ObstacleTable` 的纯逻辑**全部是已有函数**：光栅化 `plane::rasterize`（与 `0x3f11e0` 同构）、
单点查询 `plane::is_occupied`（与 `0x3f15b0` 同构）、延迟 `game::DESTROY_DELAY_TICKS`（165，
`docs/44` 的实测 163.9 / 164.0 / 167.0）。`FunctionTable::get_function_text` 走
`math_function::to_text`（`docs/119` 1032/1032 对拍的那个函数）。

### 44.5 headless 那侧：**我方自造的**观测口（原版没有）

`RootWiring` 是 Rust 结构体，GDScript 只能调 `#[func]`。§41 之前那 18 个 `#[func]`
全是"数个数 / 查键 / 取类型名"这类**只读静态**口，**没有任何办法触发一次 manager 交互**
⇒ 本轮在 `graphwar_root_node_class.rs` 里**只加**了两个（既有 18 个与既有断言**一条没动**）：

```text
manager_probe_selftest() -> PackedStringArray   # 5 个 manager 各一次最小交互，27 条 "名字=值"
manager_probe(name) -> GString                  # 单条查询；未知名字 ⇒ 空串
```

边界（写死在这里，别当成复原成果）：

* **原版没有这两个方法**，它们是验收工具（`src/_scaffold/` 里本来就没有原版对应物）；
* **只读**：不碰 22 个槽的注册，不碰 `Match` / `TurnManager` / `PlayerManager`
  （那三个是既有断言在读的）；动的只有这 5 个 manager 自己表里的固定 id（`9001..=9003`）
  与固定坐标，每次调用末尾把发件箱取走；
* **幂等**：连调两次返回**同一串**（`assemble_check.gd` 就是这么比的）；
* **值是真算的**：每一条都是"写进去 → 从被写的那张表读回来"，没有一条 `return` 常量
  （反证：`root_wiring.rs` 的 `the_read_back_values_track_the_written_state` 换了输入，
  读回值跟着变）。

27 条期望值（两边逐字相同：GDScript 的 `expected_probe` 与 Rust 的 `expected_selftest()`）：

```text
life.alive=1  life.death_tick=42  life.due_at_42=1  life.death_tick_after=none  life.subs=3
target.contains=true  target.replayed=1  target.removed=true  target.contains_after=false  target.subs=2
obstacle.hit_after_add=true  obstacle.hit_far=false  obstacle.pending=1
obstacle.due_164=0  obstacle.due_165=1  obstacle.hit_after_destroy=false  obstacle.subs=1
function.text=x  function.active_tick=100  function.active_at_101=1
function.active_at_100=0  function.active_at_200=0  function.subs=0
tick.delta=1  tick.event_tag=0  tick.at_500ms=30  tick.subs=1
```

### 44.6 验收（本节实测）

```text
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 199 passed, 0 failed
```

| 命令 | 本节实测 | 基线（`CG@418319d`，同一条命令） |
|---|---|---|
| `cargo test --offline` | **1530**，全绿 | **1520**（本节 +10：`life_manager` 5 条 + `tick_handler` 5 条） |
| `cargo test --offline --features gdext-bindings` | **1609**，全绿 | **1595**（本节 +14：上面 10 条 + `root_wiring` 4 条——那个模块挂在这个 feature 下） |
| `assemble_check.gd` | **199 passed / 0 failed**，退出码 0 | **183**（§41；本节净 +16）。⚠️ §47 起是 **203**（四档断言） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_fns` 2311 → **2385**，`src_tree_sha256` 跟着变） | OK |

**两条护栏（三条驱动轴，分开跑、分开报，`AGENTS.md` 合并纪律 C）**：

| 轴 | 命令 | 本节实测 | 基线 |
|---|---|---|---|
| `fire` | `cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire --policy default` | 内容命中 **325 / 1080** | **325**（一位不差） |
| `batch` | `… --drive batch --policy default` | 内容命中 **346 / 1080** | **346**（一位不差） |
| `batch_conv` | `cargo run --release --offline -q --example batch_conv -- --dir <快照> --policy strict` | 种类序列 **1074 / 1080**、内容命中 **595 / 1080** | **1074 / 595**（一位不差） |

⇒ 本节只动 `_scaffold/` 与 5 个 manager 的**接线层**，一条几何/弹道代码都没碰，
三条轴逐数字不动，符合"接线层不该动几何"的预期。

**新增 16 条的构成**：2 条（两个新 `#[func]` 存在）+ 2 条（27 条整串 / 条数）+
5 条（5 个 manager 各一条关键值）+ 5 条（5 个槽仍报 `stub`，反面）+ 2 条（幂等 / 未知名字）。
既有 183 条**一条没删、一条没改**。

### 44.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `GraphwarFunctionManager` / `GraphwarObstacleManager` 的**字段布局** | **未知** | 只解到函数名与落点（`fixtures/name_to_addr.tsv`）；两者的 `new` / `set_*` 体内字段偏移没逐条读。所以本轮**不**冒充类型名，装的是我方形状 |
| `queue_destroy_obstacle` 的**到期时刻算法** | **推断** | 用 `docs/44` 的实测 165 tick；`0x3f20f0` 里那个加法（`now + ?`）没逐条解 |
| `life::handle_tick_event` 到期后调 `set_alive` 的**实参** | **未知** | `0x4540aa` 那个调用点只看到 `call 0x454b40`，两个实参的来源没解 ⇒ 本层只把到期项**取出来返回**，不改第一张表 |
| `get_fractional_tick`(`0x519530`) / `get_time_to_tick`(`0x519450`) 的**算式** | **未知** | 两个函数有落点、体内公式没读。本层只给了一个**我方**派生量 `elapsed_tick_units()`，名字都没沿用 |
| `get_active_functions_on_tick` 的**点条数** | **推断** | §42.2 的三条谓词已落码，但本表不存曲线点 ⇒ `len` 按 0 算，窗口退化成 60 tick 尾巴 |
| `function_manager` **不订阅/不发事件** | **推断**（有正对照） | 构造路径 `0x5236e0` 体内无 `0x3d38f0`/`0x3d3530` 调用；但那个闭包可能被内联到别处（没有独立符号可核） |

## 45. `ObstacleDrawer` 成了真节点类 —— 客户端替换清单里最后一个"类还不存在"的缺口（2026-11，任务 23）

**一句话**：§40 把棋盘上 3 个占位子节点换成了真 `FunctionDrawer`，但障碍层**一行代码都没有**。
本节补上 `ObstacleDrawer`：`#[derive(GodotClass)] #[class(base = Control)]`，
把 1000×600 的障碍掩膜拼成一张 **`LA8`** `Image` → `ImageTexture` → **一个** `Sprite2D` 子节点，
按平面的 `Control::get_size()` 摆位；接进 `GraphwarPlane` 后由 `process` **每帧刷 2 次**
（原版就是这个数）。`client/tools/obstacle_drawer_check.gd` **116 条断言全过**，退出码 0。

> 档位口径同 §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。脚本条数**只是回归保护**，不是复原证据。

### 45.1 一页结论

| 项 | 结果 | 证据 |
|---|---|---|
| 基类 | **原版没有基类**（`ObstacleDrawer` 不是 Godot 类）；本类是 `Control` | 原版负面结论是**观测**（见 §45.2）；`Control` 是**推断** |
| 结构体字段 | **4 个**：`Gd<ImageTexture>` / `Gd<Sprite2D>` / `PackedByteArray` / `Gd<Image>` | **观测**（`drop_in_place` @ `0x39518c`，268 B，逐字段析构） |
| 图的规格 | `Image::create_from_data(1000, 600, false, **1 = LA8**, ..)` | **逐位**（立即数 `0x3e8` / `0x258` / `wzr` / `#0x1`） |
| 像素映射 | `LA8 = (L = 0, A = !mask_byte)` ⇒ 障碍 = 不透明**黑**、空 = 全透明 | **逐位**（`mvn w8, w23` + `sturb wzr`） |
| `Sprite2D` 摆法 | `position = size × 0.5`；`k = size.x / 1000`；`scale = (±k, **−k**)` | **逐位**（`fmov s2,#0.5` / `0x447a0000` / `fneg` / `fcsel`） |
| 每帧几次 | **2**（`process_plan()` 已记；本类照做，不再只是计数） | **观测**（§36 / `src/obstacle_drawer.rs`） |
| 新增代码 | `src/_scaffold/obstacle_drawer_class.rs` 683 行（**真实代码 257 行**，去掉注释/文档/单测） | 本节 §45.7 有复现命令 |
| Plane 侧改动 | **+60 行 / −0 行**（其中**代码 32 行**） | `git diff --stat` |
| 验收 | `obstacle_drawer_check.gd` **116 passed, 0 failed**；`cargo test` 1559 / 0；gdext 轴 1642 / 0；`selftest` **694 / 0**；既有 11 个 headless 脚本**一条没回归** | 本节 §45.7 |

### 45.2 基类：**原版根本没有基类**（这条必须先说清，别把它当成"已复原的 `Control`"）

任务书问"基类要有证据"。本轮查完的答案是：**原版 `ObstacleDrawer` 不是 Godot 类**，
是 `GraphwarPlane` 的一个**普通字段**，所以它没有基类可抄。两条命令、一张表：

```text
# ① 谁注册成了 Godot 类：gdext 给每个 GodotClass 生成 __registration_storage
readelf -sW libgraphwarrust.so | c++filt | grep "__registration_storage" \
  | sed -E 's/.*graphwarrust::([a-z_0-9]+)::([A-Za-z]+).*/\1::\2/' | sort -u
   → 12 个：axis_drawer / effect_drawer / entity_drawer / graphwar_ad_switch / graphwar_button /
            graphwar_function_input / graphwar_lan_vbox / graphwar_long_press_animation /
            graphwar_plane / graphwar_root_node / graphwar_setup_control / player_showcase
     **没有 obstacle_drawer，也没有 function_drawer**

# ② 基类（符号名里带 gen::classes::<base>::；§35.3 的那条命令）
   → 11 行（正对照：effect_drawer/effect_drawer 那批确实扫得出来）
   → grep -cE "function_drawer|obstacle_drawer" = 0
```

| 判据 | `ObstacleDrawer` | 正对照 |
|---|---|---|
| `__registration_storage::STORAGE` 符号 | **没有** | 12 个类都有（`GraphwarAdSwitch` 也有 —— §35.4 就是靠它定的性） |
| `impl I<Base>`（`gen::classes::…`） | **没有** | 11 个类有 |
| `.rodata` 里的**类名字面量**（`GraphwarAdSwitch` 那种 `type_name` 游程） | **没有**（全文件 4 处 `ObstacleDrawer` 全是 `.strtab` 的 mangled 符号名） | `GraphwarAdSwitch` 有游程 |
| 运行时真·节点树 dump（§2.1.10） | `GraphwarPlane` 下**没有** `@ObstacleDrawer@N`（只有 `@AxisDrawer@N` / `@EntityDrawer@N` / `@EffectDrawer@N` / `@Line2D@N`） | 同上 |
| `GraphwarPlane::init`（`0x4b7380`）里有 `obstacle_drawer.rs:37:97` 的 `panic::Location` | **有** | `src/obstacle_drawer.rs` 的 `FUNCTIONS[1]` 已记 |

⇒ **本类选 `Control` 是推断**，两条理由（都**不是**原版证据）：

1. **同族**：`*Drawer` 里凡是真注册成类的（`AxisDrawer` / `EffectDrawer` / `EntityDrawer`）
   基类**全是 `Control`**（§35.3 观测）；
2. **它按 `Control` 的语义布局**：`update` 那两次 `Control::get_size()` 拿的是**平面的**尺寸，
   Sprite2D 的 `position` / `scale` 由它算出（§45.3d）。

**缺的证据**（不硬说定死）：原版那个结构体在场景树里**没有对应节点**，所以"该是 `Control`
还是 `Node2D`"没有可观测对象；要定死只能拿到 2024 版带 `impl I<…>` 的符号表的二进制，
或抓到 `GraphwarPlane::init` 里那次构造的 `Gd::default_instance` 目标类名。
⚠️ 同一条判据也适用于 **`FunctionDrawer`**：它**不在**那 12 个里（§40 的类注释说"原版是
`Control` 子类"，那是**推断**，本节的扫描是对它的**更正/降档**）。

### 45.3 从指令里读出来的五条（`update` = arm64 `0x422590`，4688 B）

#### (a) 结构体只有 4 个字段（**观测**：`drop_in_place<ObstacleDrawer>` @ `0x39518c`）

那 268 字节的 drop glue 只碰 3 个"需要析构"的字段，而有 4 个字段拼得进去：

```text
+0x00  Gd<ImageTexture>   24 B   upcast_ref<RefCounted> + RefCounted::unreference（0x3951bc）
+0x18  Gd<Sprite2D>       24 B   **没有**析构代码（Node 不是 RefCounted，`Gd<Node>` 不保活）
+0x30  PackedByteArray    16 B   PackedElement::ffi_destroy（0x3951f8）
+0x40  Gd<Image>          24 B   同上；cleanup 路径里直接写 drop_in_place<Gd<Image>>
```

`update` 里三条指令印证这三个偏移：`add x0, x26, #0x20` 是那个 sprite 的 `RawGd`
（`+0x18` 的 `Gd` 的 obj/id 半边）、`add x4, x19, #0x30` 是像素缓冲、`ldr x0, [x21, #0x40]!`
是那张 `Image`。⇒ **一张图、一个贴图、一个 sprite**（不是"每个障碍一个 `Sprite2D`"），
而且 **结构体里没有 `HashMap`、没有父节点句柄**。

⚠️ **就地更正 `src/obstacle_drawer.rs` 文档里的一句话**（那个文件一行没改，按 §37.3 的做法记在这里）：
它写"`hashbrown::rustc_entry` ×3 … 按 key 缓存那些节点"。按 (a) 的字段表，那 3 次 entry 的 map
**不在 `ObstacleDrawer` 的字段里** ⇒ 读作"缓存的是**障碍数据**（`Mutex<GraphwarObstacleData>`，
`0x4226e4` 那条 `ldxr` 锁）里的表"更合适。**档位：观测（字段数）+ 推断（那 3 次 entry 归属哪个 map）**，
缺的证据是从 `0x422828` / `0x422df4` / `0x42306c` 三个 entry 调用点往上追它读的是哪个基址。

#### (b) 图是 **1000×600 / `LA8` / 无 mipmap**（**逐位**）

两处 `Image::create_from_data`（`0x422780` / `0x422d1c`）的实参**完全相同**：

```text
0x422764  mov w0, #0x3e8     ; 宽 1000
0x422768  mov w1, #0x258     ; 高 600
0x42276c  mov w2, wzr        ; use_mipmaps = false
0x422770  mov w3, #0x1       ; Format = 1 = LA8（Godot 4：L8=0 / LA8=1 / R8=2 / RG8=3 / RGB8=4 / RGBA8=5）
0x422760  add x4, x19, #0x30 ; &self.pixels（+0x30 那个 PackedByteArray）
```

1000×600 与 `plane::CANVAS_W/H` 一致 ⇒ **掩膜像素与图片像素一对一**（不需要再缩放）。

#### (c) 掩膜字节 → 像素：**L = 0、A = `!m`**（**逐位**）

`0x422cbc`–`0x422cf0` 的循环，每个掩膜字节 push **两个**字节（`op_push_back` ×2）：

```text
0x422cbc  ldrb  w23, [x22, x20]    ; m = mask[i]
0x422cc0  sturh wzr, [x29, #-0xa8] ; pair = [0, 0]
0x422ccc  bl    op_push_back       ; 推 L = 0
0x422cd0  mvn   w8, w23            ; !m
0x422cd8  sturb w8, [x29, #-0xa7]  ; pair[1] = !m
0x422ce4  bl    op_push_back       ; 推 A = !m
```

掩膜口径（`plane::rasterize`）：`0x00` = 障碍、`0xFF` = 空 ⇒ **障碍 = 不透明黑、空 = 全透明**。
另一条路径（`0x422bfc`–`0x422c3c`）改用 `ffi_index_mut(2*(row*1000+col)+1)` **只写 A 通道**，
写的还是 `mvn`（`0x422c38`）——两条路径映射一致。⇒ "障碍物是黑的"是**从指令读出来的**，
不是我们挑的颜色（颜色真要复原得等 `0x30` 那个缓冲的**来源**解出来，见 §45.6）。

#### (d) `Sprite2D` 的摆法（**逐位**）

```text
0x422608 / 0x42262c  Control::get_size ×2       ; 平面的像素尺寸（原版调两次；本类调一次，同值）
0x42260c  fmov s2, #0.5                         ; 0.5
0x42261c  fmul s8, s0, s2 / fmul s9, s1, s2     ; position = size * 0.5（居中）
0x422630  mov  w8, #0x447a0000                  ; 1000.0f32
0x422644  fdiv s10, s0, s1                      ; k = size.x / 1000
0x42266c  fneg s1, s10                          ; scale.y = -k（**永远**取负）
0x422678  fcsel s0, s1, s10, ne                 ; scale.x = (w5 != 0) ? -k : k
```

`s = (k, −k)` 与 `plane` 自己的 `draw_set_transform(scale = (20, −20))` 是同一个取向
（棋盘 y 向上、画布 y 向下）。那个 `w5` 是第 5 个入参（`0x4225fc` 存进 `w20`），
**语义未知**，本类命名 `flip_x`（默认 `false` ⇒ `scale = (k, −k)`）。

#### (e) `update` 的入参形状 + `add_child` 的父（**观测**）

```text
x0      = &mut self
x1, x2  = 一个 16 B 的 RawGd（`stp x1,x2,[sp,#0x50]` @ 0x4225dc）→ 紧跟 upcast_ref
          （字面量 "upcast_ref" @ 0x19d85a，长 10）+ Control::get_size ⇒ **平面节点**
x3      = 指针（`mov x19, x3`），它的 `+0x40` 被当 GraphwarModuleContainer 用：
          `ldr x19,[x19,#0x40]` → `get_module(x19+0x18)` @ 0x4226a0
x4      = 一份 ptr/len 借用（`[x23]` / `[x23+0x8]` / `[x23+0x20]`）——障碍数据那一侧
w5      = bool（见 (d)）
```

`add_child_full`（`0x423224`）的**父**是 `x1/x2` 那个节点（`0x4231f8` 又 `upcast_ref` 了一次）
⇒ **原版把 `Sprite2D` 挂到平面上**（因为本结构体不是节点、也不存父句柄）。
**本类的差别**：挂成自己的子节点（任务书要求"`Sprite2D` 子节点"），于是坐标原点 = 本节点原点；
只要本节点贴在平面左上角（`position = (0,0)`），`position = size*0.5` 的语义与原版**逐像素相同**。

### 45.4 与原版的差别（逐条）

| 项 | 原版 | 本文件 | 档位 |
|---|---|---|---|
| 基类 | **没有**（平面的普通字段） | `Control` | 原版**观测**（负面）；`Control` 是**推断** |
| 像素缓冲 | `PackedByteArray`（`+0x30`），逐字节 `op_push_back` | 先拼 `Vec<u8>`，再 `PackedByteArray::from(..)` 一次交出去 | 布局**观测**；批量是**我方选择**（逐字节 FFI 每帧 120 万次不划算） |
| 贴图 | `create_from_image` 一次 + `update` N 次（`+0x00` 那个字段就是为 `update` 留的） | 同：首次 `create_from_image`，之后 `Image::set_data` + `ImageTexture::update` | **观测**（`CALLED_API` 里两条都在） |
| `create_from_data` 每帧 2 次 | 是（`0x422780` / `0x422d1c`，两条路径） | 首次 1 次 + 之后每次 `set_data` | 次数**观测**；化简是**我方选择**（两条路径各管什么**未知**） |
| 掩膜从哪来 | `GraphwarObstacleManager` 的 `Mutex<GraphwarObstacleData>`（每帧锁一次） | 本类自己持一张；改它的仍是 `plane::rasterize` | 锁**观测**；"自己持"是**化简**（那个 manager 还没节点化） |
| 脏矩形 | 由 `graphwar_plane` 持有，本模块入参里没有矩形 | **同**（本类也不收矩形，每次刷整张图） | **观测** |
| sprite 的父 | 平面（§45.3e） | 自己 | 原版**观测**；差别是**我方选择** |
| 名字 | 无（不是节点，没有 `set_name` 的机会） | 不 `set_name` ⇒ 自动名 `@ObstacleDrawer@N` | 我方选择（同 §40） |

### 45.5 接进 `GraphwarPlane`：为什么**默认不建**

三处改动（`git diff` 全文 60 行）：

1. 字段 `obstacle_drawer: Option<Gd<ObstacleDrawer>>`；
2. `process` 里照 `process_plan()` 给的 **2** 真的刷两次（没有障碍层时退回"只计数"，行为与改前一致）；
3. `#[func] build_obstacle_drawer()` / `obstacle_drawer_count()` / `obstacle_drawer_updates()`。

**为什么不自动建**：既有验收脚本断言过平面的子节点数 —— `draw_classes_check.gd` 的
`c.get_child_count() == 3`（`build_children` 之后）、`assemble_check.gd` 的 `1` / `4`。
自动建就会把它们打红。⇒ 改成显式调用，`obstacle_drawer_check.gd` 里**两种情形都断言**：
`plane.get_child_count()` 在"没建"时仍是 0、建完 1、进树后 4（3 个 `FunctionDrawer` + 1 个障碍层）。

### 45.6 还没做 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| 障碍的**颜色**（现在照 (c) 是**黑**） | 黑是**逐位**；"原版最终是什么颜色"是**未知** | `0x30` 那个 `PackedByteArray` 的**上游**：原版是先往缓冲里 `ffi_index_mut` 写 A、再从别处 `memcpy` 一份源数据（`0x422b68`–`0x422b8c`），那份源数据的生产者没追 |
| `update` 的 `w5`（第 5 个入参） | **未知** | 只有"为真时 `scale.x` 取负"这一条（`0x422678`）；调用方是谁还没定位 |
| 每帧为什么刷 **2** 次 | **未知**（数字是**观测**） | 候选：一次整图 + 一次脏区；候选：两层障碍各一次。要抓 `update` 两次入参的运行时值 |
| 两条 `create_from_data` 路径的分工 | **未知** | `0x4226dc` 那个 `b.ne 0x422a58` 判的是 `self[0]`（`Gd<ImageTexture>` 的 cached rtti 标记）；"首次建 / 之后走脏区"只是**推断** |
| 脏矩形（只重画那一块） | **未知** | `src/obstacle_drawer.rs` 已记：本模块入参里没有矩形；"只重画一块"没有逐条读到 |
| `Sprite2D` 的 `texture_filter` / `centered` | **未知** | `update` 里只有 `set_position` / `set_scale` / `set_texture`（`CALLED_API`），没有第三条 |
| headless 下"贴图真的换了"的引擎侧证据 | **观测到的限制** | dummy 渲染器里 `RenderingServer::texture_2d_update` 不生效：同一像素建贴图时 alpha=255，`update` 之后从 `texture.get_image()` 读回来**还是** 255，而那张 `Image` 已经是 0（脚本里有一行 INFO 记这个）。⇒ 验收改读"喂给 `update` 的那张 `Image`" + 计数器；要看渲染结果得上有 GPU 的机器 |

### 45.7 复现（四分钟）

```text
export CARGO_HOME=<工作区>/.reference/cargo-home CARGO_TARGET_DIR=<工作区>/.agent-work/cg/target23
export GODOT=<工作区>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64

cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so     # 不覆盖就是旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                     # 判据：3
"$GODOT" --headless --path client --import                                # 不 import 扩展静默不加载
"$GODOT" --headless --path client --script res://tools/obstacle_drawer_check.gd
   → OBSTACLE_DRAWER 116 passed, 0 failed（退出码 0）

cargo test --offline                             # 1559 passed / 0 failed（改了 `src/` 必须全绿）
cargo test --offline --features gdext-bindings   # 1642 passed / 0 failed（本类 4 条纯函数单测在这条轴里）
python3 tools/coverage_matrix.py --check         # OK（重跑过：src_files 106 → 107、src_fns 2420 → 2453）
python3 tools/check_doc_refs.py                  # 通过（退出码 0）
```

**"真实代码行数"的算法**（与任务书的模板对照同一个口径）：去掉空行、去掉以 `//` 开头的整行注释、
去掉 `#[cfg(test)] mod tests { … }` 整块，剩下**有语句的行**：

```text
src/_scaffold/obstacle_drawer_class.rs   683 行 → 真实代码 **257** 行（其中单测块 58 行另计）
src/_scaffold/entity_drawer_class.rs     678 行 → 真实代码 **292** 行（模板对照，同一口径）
src/_scaffold/function_drawer_class.rs   538 行 → 真实代码 **216** 行（同一口径）

# 口径（可复跑）：非空行、去掉整行注释（`//`/`///`/`//!`）、去掉 `#[cfg(test)] mod tests {..}` 整块
python3 - <<'EOF'
src = open('<文件>').read().split('\n'); n = 0; t = False
for line in src:
    st = line.strip()
    if st.startswith('#[cfg(test)]'): t = True
    if t or not st or st.startswith('//'): continue
    n += 1
print(n)
EOF
```

⚠️ 任务书说模板 `entity_drawer_class.rs` "677 行，真实代码约 155 行"：那个 155 的口径比上面更严
（只数函数体里的语句，不数 `struct` 字段 / 常量 / 签名），**本轮复现不出来**。为了让"本类 vs 模板"
可比，三个文件都用上面这条命令的同一口径 ⇒ **257 : 292 : 216**。

**selftest**（`bash client/run.sh selftest`）：**694 passed, 0 failed**（前置：`client/assets/` 已解出
+ `cargo build --example serve_match` + `export CARGO_TARGET_DIR` + 两个回放样本
`1793897298` / `1783255495` 放进 Godot 的 `app_userdata/Graphwar/replays`）。缺那两个样本时是
**633 / 0 / 2**、缺音频资产时还要再少 10 条 —— 这两个数**不是**本次改动造成的（本改动的 116 条
在 `obstacle_drawer_check.gd` 里单独跑）。

**既有 headless 脚本回归**（本节提交前逐个跑过，全绿）：`gdext_check` 24 / `base_classes_check` 117 /
`entity_drawer_check` 72 / `function_drawer_check` 132 / `draw_classes_check` 235 /
`curve_on_screen_check` 96 / `plane_on_screen_check` 76 / `signal_classes_check` 155 /
`assemble_check` 199 / `spike_class_check` 23 / `recovered_view_check` 57，**`0 failed` ×11**。

## 46. 两个"字段一个都没解"的管理器解开了：`GraphwarFunctionManager`（32 B / 4 字段）与 `GraphwarObstacleManager`（80 B / 5 字段）（2026-11，任务 22）

**一句话**：§44.0 那两行「`GraphwarFunctionManager` / `GraphwarObstacleManager` 的字段**一个都没解**」
本节**作废**——两个结构体的每个字段都落到了指令上，而且**三份构建（arm64 2.0.60 / x86 Linux
2.0.60 / Windows 2.0.60 DLL）的偏移序列逐条相同**；布局写进
`src/graphwar_function_manager.rs` / `src/graphwar_obstacle_manager.rs`，
`size_of` 分别由 **编译期硬闸** 与单测断言为 **32 / 80**。

> 档位口径同 §42 / §44：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / `assemble_check.gd` 的条数**只是回归保护**，不是复原证据。

### 46.0 先说"没做到"的，以及两条就地更正

| 项 | 现在 | 档位 |
|---|---|---|
| 两个结构体的**字段布局** | **全部解开**（4 字段 / 5 字段，见 §46.2 / §46.3） | **观测**（三构建交叉验证） |
| 它们的**行为** | 仍未复原：`FunctionState` 的字段语义、到期 tick 的加法、`draw_circle` 写什么值 | **未知** / **推断**（见 §46.8） |
| 容器槽里的**档位字符串** | **仍然是 `"stub"`**，`module_impl_type` 仍然是 `"—"` → ⚠️ **§47 已改**：`FunctionManager` = `layout`、`ObstacleManager` = `wired`，`module_impl_type` 报 `graphwar_*_manager::Graphwar*Manager` | 我方口径（§47.2） |

**就地更正 1（`AGENTS.md` §0.6）**：§44.0 / §44.7 写「两个管理器的字段**一个都没解**」。**已解**，
本节起那两行过期；`src/_scaffold/root_wiring.rs` 的两个槽已从"我方形状"换成"**原版布局 + 我方行为表**"。

**就地更正 2（更重要，是一条**错的负结论**）**：§44.2 / §44.3 写
「`function_manager` 的四个 `set_*` 体内**零条** `GraphwarEventQueue::add_event`（`0x3d3530`）
⇒ 它**不发事件**」。**这条是错的**，而且是 `AGENTS.md`「间接调用与函数归属」第 1 条那个坑的复发：
`add_event` 在 x86 上是**经 GOT 槽 `0x9bac80` 的间接调用**，只扫 `call rel32` 直调会漏。

```text
x86 0x455e30 set_function             0x45638f  call qword [rip+..] slot 0x9bac80 -> 0x3d3530
x86 0x456810 set_function_points      0x4569e1  同上
x86 0x456b80 set_function_active_tick 0x456ce1  同上
arm64 0x488a9c set_function           0x488e30  bl 0x3d7a40   ; .symtab 名 = GraphwarEventQueue::add_event
```

`set_function` 发的是 **tag `0x23` = 35 = `FunctionInfo`**（arm64 `0x488e18`
`mov x8, #0x23; movk x8, #0x8000, lsl #48`，载荷 = `(id, 文本, tick)`）——这条与
**本仓自己的 `docs/02-module-map.md:524`**（「`0x455e30` … 再发事件 tag 35」）一致，
也就是说 §44 那条结论当时就和 docs/02 冲突。**结论改为**：`function_manager` **不订阅**（这条仍成立，
有正对照），但它**会发** `FunctionInfo` / `FunctionActive` 这类事件，共 3 个 `set_*` 各一条。

### 46.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_function_manager.rs` | **新增** | 模块文档（方法表 / 32 B 布局表 / 内层 48 B / 更正）+ `GraphwarFunctionManager` / `GraphwarFunctionData` / `FunctionState` + **3 条 `const` 硬闸** + 3 条单测 |
| `src/graphwar_obstacle_manager.rs` | **新增** | 同上（80 B / 5 字段 / 内层 24 B 网格 + 索引式）+ 硬闸 + 3 条单测 |
| `src/_scaffold/root_wiring.rs` | 改 | ①`#[path]` 把上面两个原版模块文件挂进 crate（`lib.rs` 在禁区，见 §46.6）；②两个槽 `XxxSlot(表)` → `XxxSlot { layout, table }`；③20 处 `.0` → `.table`（探针 18 处 + 单测 2 处）；④§44 那条错结论就地更正 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | `src_files` 106→**108**、`src_fns` 2420→**2432**、`src_tree_sha256` 跟着变；**527/527 与每模块行一行没变** |
| `docs/16-client.md` | +本节 | — |

❌ `src/lib.rs` / 任何 `*_class.rs` / `client/**` / `src/game.rs` / `src/plane.rs` / 既有测试断言
**一行没动**；`tools/`（含 `coverage_matrix.py` 的模块归属表）**没动**（见 §46.8 最后一行）。

### 46.2 `GraphwarFunctionManager` = **32 字节 / 4 字段**（`观测`）

| 偏移 | 宽 | 类型 | 证据（指令地址；三构建见 §46.5） | 档位 |
|---|---|---|---|---|
| `+0x00` | 8 | `Weak<RefCell<GraphwarFunctionManager>>` | 构造点把 **`RcBox` 指针本身**写进该槽：arm64 `0x419d44` `stp xzr, x19, [x19, #0x10]`（第 2 字 = `RcBox+0x18` = `m+0x00`）、x86 `0x5237f3` `mov [rbx+0x18], rbx`、DLL `0x18013f476`；析构交给 `drop_in_place<Weak<RefCell<GraphwarFunctionManager>>>`（arm64 `0x419df0`、DLL `0x18013f4f9`） | **观测** |
| `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | `set_function` 读它 → `+0x10` 是 `RefCell` 借用位 → `+0x18` 当 `&mut GraphwarEventQueue` 传给 `add_event`（arm64 `0x488dc4`→`0x488e30`；x86 `0x4562e8`→`0x45638f`）；`set_function_active_tick` 同（arm64 `0x489708`、x86 `0x456ce1`）；构造点第二字；析构做 `Rc` 强计数 `-1` / `drop_slow` | **观测** |
| `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `set_function` 把 `self+0x10` 当 `&Weak` 传给 `GraphwarModuleContainer::get_module_wk`（arm64 `0x488ac0` `add x0,x0,#0x10` → `bl 0x4a34c4`，该函数第一句就是 `ldr x20,[x0]` + `cmn x20,#1` 的 `Weak` 哨兵判定；x86 `0x455e5a` → `call 0x46d040`）；析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419dcc`、DLL `0x18013f4e2`） | **观测** |
| `+0x18` | 8 | `Arc<Mutex<GraphwarFunctionData>>` | **7 个方法的第一句**都是读这个槽（arm64 `0x488814`/`0x488c48`/`0x489068`/`0x489298`/`0x489608`/`0x48986c`/`0x489a0c`；x86 `0x455b74`/`0x456604`/`0x45682b`/`0x456b9e`/`0x456e03`/`0x456f93`）；`ArcInner` = `{strong@0, weak@8, Mutex@0x10}`，锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（arm64 `ldaxr w9,[x+0x10]` / `ldrb w8,[x+0x14]`；x86 `lock cmpxchg dword [x+0x10]`）；析构 `Arc::drop_slow` | **观测** |

**尺寸的两条独立证据**（都不依赖 §41 的 vtable 表）：

1. `RcBox<RefCell<GraphwarFunctionManager>>` 的分配常量是 **`0x38` = 56 = 24 + 32**
   （arm64 `0x419ca4`、x86 `0x523704`、DLL `0x18013f381`；DLL 错误路径
   `0x18013f4a2` 的 `handle_alloc_error(size=0x38)` 对得上）；
2. §41.2 的 `dyn Any` vtable 第 1 字 = `size_of::<RefCell<M>>()` = **40 = 32 + 8**（两架构逐字相同）。

**构造在哪儿**（`观测`）：原版**没有** `GraphwarFunctionManager::new` 的独立符号——构造整个被内联进
`Rc::…::new_cyclic_in`：arm64 `0x419c80`（416 B，由 `GraphwarGameClient::new` 在 `0x40a158` 调用）、
x86 `0x5236e0`（208 B，由 `0x517b9f` 调用、紧接 `add_module::<GraphwarFunctionManager>` `0x469db0`）、
DLL `0x18013f360`（DLL 里 `new` `0x1800d9ab0` 只是个 16 B 的 `jmp` thunk）。
三份构建的字段写入顺序完全一致 ⇒
`new(event_queue: Rc<RefCell<GraphwarEventQueue>>, container: Weak<RefCell<GraphwarModuleContainer>>)`，
弱自句柄由 `Rc::new_cyclic` 的 `&Weak` 直接给出。
（§41.2 表里「arm64 `new` = `0x40a158`」其实是**调用点**，构造体是 `0x419c80`——本节一并更正。）

### 46.3 `GraphwarObstacleManager` = **80 字节 / 5 字段**（`观测`）

| 偏移 | 宽 | 类型 | 证据（指令地址；三构建见 §46.5） | 档位 |
|---|---|---|---|---|
| `+0x00` | 8 | `Weak<RefCell<GraphwarObstacleManager>>` | `new` 里做 `Weak::clone`：`cmn x21,#-1`（`Weak::new` 哨兵）→ `ldr x8,[x21,#8]`/`adds`/`str`（弱计数 +1）→ 塞进 `tick_event` 的订阅闭包：arm64 `0x42633c`–`0x42635c`、x86 `0x3f2399`–`0x3f23a3`（`cmp r15,-1` / `inc [r15+8]`）、DLL `0x1801047f8`–`0x180104806`。构造点写 `RcBox` 指针：arm64 `0x419b70`、DLL `0x18013f634`；析构 `drop_in_place<Weak<RefCell<GraphwarObstacleManager>>>`（`0x416ff8` / DLL `0x18013c480`） | **观测** |
| `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | 4 个入口都读它、`+0x10` 是 `RefCell` 借用位、`+0x18` 传给 `add_event`：arm64 `0x42572c`（add）/`0x4258d4`（clear）/`0x425adc`（destroy）/`0x42620c`（queue_destroy）；x86 `0x3f184b`/`0x3f1a05`/`0x3f1bde`/`0x3f2245`；DLL `0x180103b6f` 等。构造点第二实参 | **观测** |
| `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `new` 的第三个实参（arm64 `x21`→`RcBox+0x28`；x86 `r15`→`[rbx+0x28]`；DLL `%rbx`）；析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419c2c`、DLL `0x18013f6e7`） | **观测** |
| `+0x18` | 8 | `Arc<Mutex<GraphwarObstacleData>>` | 每个方法第一句都读它（arm64 `0x425488`/`0x4256a8`/`0x425888`/`0x425a58`；x86 `0x3f15bc`/`0x3f17b4`/`0x3f19b1`/`0x3f1b44`）；锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（数据里 `ptr@+0x20`、`len@+0x28`）；`drop_in_place<ArcInner<Mutex<GraphwarObstacleData>>>` 存在（DLL `0x18013c810`） | **观测** |
| `+0x20` | 48 | `HashMap<u32, Vec<[f64; 3]>>`（待销毁队列） | `RawTable`：`ctrl@+0x20`、`bucket_mask@+0x28`、`growth_left@+0x30`、`items@+0x38` + `RandomState@+0x40`（16 B）。决定性指令：arm64 `0x425c50` `add x0,x0,#0x40` → `BuildHasher::hash_one`(`0x3ec7f0`)、`0x425c68` `add x0,x19,#0x20` → `RawTable::remove_entry`(`0x3c6620`)；x86 `0x3f2110`/`0x3f2128`；DLL `0x18010455f`/`0x180104577`。键 4 字节、值 `Vec<[f64;3]>`（插入走 `RawVec::grow_one`、元素 24 B；析构 `dealloc(ptr, 24*cap)`） | **观测** |

尺寸的独立证据：`RcBox<RefCell<GraphwarObstacleManager>>` 的分配常量 **`0x68` = 104 = 24 + 80**
（arm64 `0x419aa4`、DLL `0x18013f562` + 错误路径 `size=0x68`）；且 `+0x20 + 48 = 0x50 = 80` 自洽。

`new` 的签名（`观测`）：`new(&mut GraphwarEventHandler, Rc<RefCell<GraphwarEventQueue>>,
Weak<RefCell<GraphwarModuleContainer>>) -> Rc<RefCell<Self>>`——第一个实参用来
`subscribe_to_event("tick_event", len 0xa, 闭包)`（arm64 `0x4263a0`、x86 `0x3f23f0` 经 GOT `0x9bac88`）：
这是这个管理器**唯一**一条订阅（与 §44.2 一致）。

### 46.4 两个内层数据类型也被顺带钉死了

| 类型 | 大小 | 是什么 | 证据 | 档位 |
|---|---|---|---|---|
| `GraphwarFunctionData` | **48 B** | 一张 `HashMap<u32, FunctionState>`（没有别的字段） | `ArcInner<Mutex<…>>` 分配 `0x48` = 72 = 16+8+48（arm64 `0x419d2c`、x86 `0x5237aa`、DLL `0x18013f42e`）；4 组字段偏移 = `ctrl@0`/`bucket_mask@8`/`growth_left@0x10`/`items@0x18` + `RandomState@0x20`；drop glue 名 `hashbrown::raw::RawTable<(u32, FunctionState)>` | **观测** |
| `FunctionState` | **112 B**（`推断`） | 上表的**值**；字段语义未知 | 由 bucket 步长 `0x78`（§42.2 已测）反推：`size_of::<(u32, FunctionState)>() = align_up(8+S, 8) = 120` ⇒ `S = 112`（`S` 含 `f64`/`Vec` ⇒ `align = 8` ⇒ 8 的倍数） | **推断** |
| `GraphwarObstacleData` | **24 B** | 一张 **1000×600 = 600000 字节**的网格（`0xff` = 空） | `ArcInner<Mutex<…>>` 分配 `0x30` = 48 = 16+8+24；`new` 里 `__rust_alloc(0x927c0, 1)` + `memset(0xff, 600000)`，然后写 `cap=0x927c0`/`ptr`/`len=0x927c0`（DLL `0x180103895`–`0x1801038a0`）；索引式见下 | **观测** |

网格索引式（`观测`，arm64 `is_obstacle` `0x4254c4`–`0x425534`，常量全是双精度）：

```text
x = trunc(1000.0 * p.x / 50.0 + 500.0)    合法区间 [0, 999]   (cmp #0x3e7)
y = trunc(1000.0 * p.y / 50.0 + 300.0)    合法区间 [0, 599]   (cmp #0x257)
index = y * 1000 + x                      (mul w9, w9, #1000)
有障碍 ⇔ grid[index] != 0xff              (x86 `cmp byte ptr [rax+rcx], 0xff`)
```

⇒ 600000 = 1000 × 600 与 24 字节的 `Vec<u8>` 形状（本工具链内存序 `cap@0/ptr@8/len@0x10`，
`cap == len == 600000`）完全对得上。

### 46.5 三向交叉验证：偏移序列逐条相同（这是本节最硬的一层）

| 偏移 | arm64 2.0.60 | x86 Linux 2.0.60 | Windows 2.0.60 DLL |
|---|---|---|---|
| `FunctionManager +0x00` 存 `RcBox` 指针 | `0x419d44` | `0x5237f3` | `0x18013f476` |
| `+0x08` 事件队列 | `0x488dc4`（用）/ `0x419d58`（写） | `0x4562e8` / `0x5237f7` | `0x1800d997b` / `0x18013f47a` |
| `+0x10` 容器 `Weak` | `0x488ac0`（用）/ `0x419d58`（写） | `0x455e5a` / `0x5237fb` | `0x18013f47e` |
| `+0x18` `Arc<Mutex<…>>` | `0x488814` …（7 处） | `0x455b74` …（6 处） | `0x1800d8940` …（5 处） |
| `RcBox` 大小 56 | `0x419ca4` | `0x523704` | `0x18013f381`（+错误路径 `0x18013f4a2`） |
| `ObstacleManager +0x00` 弱自句柄 | `0x42633c`–`0x42635c` | `0x3f2399`–`0x3f23a3` | `0x1801047f8`–`0x180104806` |
| `+0x08` 事件队列 | `0x42572c` 等 4 处 | `0x3f184b` 等 4 处 | `0x180103b6f` 等 4 处 |
| `+0x18` `Arc<Mutex<…>>` | `0x425488` 等 4 处 | `0x3f15bc` 等 4 处 | `0x1801038d9` 等 4 处 |
| `+0x20` 哈希表 `ctrl` / `+0x40` hasher | `0x425c68` / `0x425c50` | `0x3f2128` / `0x3f2110` | `0x180104577` / `0x18010455f` |
| `RcBox` 大小 104 | `0x419aa4` | （内联，`0x3f2372` 调 `0x523020`） | `0x18013f562` |

**"不一致"的唯一一处要解释**：x86 Linux 侧两个 `new`（`0x5236e0` / `0x3f2360`→`0x523020`）都是
**内联后的独立函数**（没有符号名），而 DLL 那份还留着 `GraphwarFunctionManager::new` /
`GraphwarObstacleManager::new` 的符号——**函数边界不同，偏移序列相同**，所以不影响结论；
反过来，这也正是"只按函数地址对齐会配错函数"的例子（`AGENTS.md` §0 第 5 条）。

复算命令（三条，互不依赖；`$PY` 有 capstone）：

```text
# ① arm64 未 strip：类型名 + 大小 + 方法名
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so | grep -E "graphwar_(function|obstacle)_manager"
# ② x86 Linux（同名落点来自 fixtures/name_to_addr.tsv）
grep -E "graphwar_(function|obstacle)_manager" fixtures/name_to_addr.tsv
$PY tools/addr_probe.py dis 0x455e30      # set_function：add rdi,0x10 = 容器 Weak
$PY tools/addr_probe.py dis 0x3f2360      # ObstacleManager::new：cmp r15,-1 / inc [r15+8] = Weak
# ③ Windows DLL（同版本、同 ISA、COFF 符号表在；VA = 0x180000000 + 0x1000 + 符号值）
objdump -t .reference/windows/graphwarrust-2.0.60.dll | grep -E "Graphwar(Function|Obstacle)Manager"
```

### 46.6 落码：布局进原版模块文件、`size_of` 有硬闸

1. **两个原版模块文件**新增：`src/graphwar_function_manager.rs`（4 字段 + 3 条 `const` 硬闸 +
   3 条单测）与 `src/graphwar_obstacle_manager.rs`（5 字段 + 2 条硬闸 + 3 条单测）。
   硬闸是**编译期**的，改错一个字段类型就编不过：

   ```rust
   const _: () = assert!(std::mem::size_of::<GraphwarFunctionManager>() == 32);
   const _: () = assert!(std::mem::size_of::<GraphwarFunctionData>() == 48);
   const _: () = assert!(std::mem::size_of::<GraphwarObstacleManager>() == 80);
   const _: () = assert!(std::mem::size_of::<GraphwarObstacleData>() == 24);
   ```

   单测再逐字段断言 `offset_of!`（`0x00/0x08/0x10/0x18` 与 `0x00/0x08/0x10/0x18/0x20`）。
2. **`src/lib.rs` 在禁区**（任务书 §5 的 ❌）⇒ 这两个文件暂时由
   `src/_scaffold/root_wiring.rs` 用 `#[path = "../graphwar_function_manager.rs"] pub mod …`
   挂进 crate。这是**临时通道**，`lib.rs` 一放开就换成 crate 根的 `pub mod`，两个文件一行都不用改。
3. **容器槽**：`FunctionManagerSlot(FunctionTable)` → `FunctionManagerSlot { layout, table }`，
   `ObstacleManagerSlot(ObstacleTable)` → `ObstacleManagerSlot { layout, table }`；
   `layout` 是原版布局，`table` 是我方行为表（探针/验收仍读它）。
   槽的档位字符串**仍报 `"stub"`**、`module_impl_type` **仍报 `"—"`**——因为那两处进的是
   `client/tools/assemble_check.gd` 的既有断言（5 条 `stub` + 1 条 `container_stub_manager_count == 5`），
   而 `client/**` 本轮在禁区。⇒ **读法**：这两个槽的 `stub` 现在只表示"**行为**未复原"，
   "结构未知"那层已经消掉；要改字符串必须与 `assemble_check.gd` 一起改（**留给协调者**）。

   ✅ **§47 已完成这件事**（协调者就是任务 28）：`root_wiring.rs` 与 `assemble_check.gd` 同批改，
   档位词表换成四档，逐槽断言从 "assembled/stub" 改成 "wired/layout/assembled/stub"。
   `container_stub_manager_count` 仍是 5，因为它数的是 `SlotKind::Stub`（容器类型来源），
   **不是**档位 `stub`（现在 1 个）——两件不同的事，别混。

### 46.7 验收（本节实测，可从干净树重跑）

| 命令 | 本节实测 | 基线（`CG@c99c8b5`，同一条命令/同一构建目录） |
|---|---|---|
| `cargo test --offline` | **1559**，全绿 | **1559**（一位不差；新模块挂在 `gdext-bindings` 下，默认轴不动） |
| `cargo test --offline --features gdext-bindings` | **1644**，全绿 | **1638**（本节 +6：两个模块各 3 条） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_files` 106→108、`src_fns` 2420→2432） | OK |

**三条护栏（三条驱动轴分开跑、分开报；`AGENTS.md` 合并纪律 C）**：

| 轴 | 命令 | 本节实测 | 护栏（任务书 §3） |
|---|---|---|---|
| `fire` | `cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire --policy default --json .work/guard/f.jsonl` | 内容命中 **331 / 1080**（文件 286、读失败 117） | **331**（一位不差） |
| `batch` | `… --drive batch --policy default --json .work/guard/b.jsonl` | 内容命中 **355 / 1080** | **355**（一位不差） |
| `batch_conv` | `cargo run --release --offline -q --example batch_conv -- --dir <快照> --policy strict` | 内容命中 **633 / 1080**；种类 **1075 / 1080**（多发批 128/130 + 单发批 947/950） | **633 / 1075**（一位不差） |

⇒ 本节只加"布局结构体 + 槽的字段"，**一行几何/弹道代码都没碰**，三条轴逐数字不动。

### 46.8 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `FunctionState`（112 B）的**字段语义** | **未知** | 只有 §42.2 那三条谓词（`+0x50` 与 `i64::MIN` 比、`+0x70 == 1`、`+0x74` = `active_tick`(u32)、`+0x58/+0x60` = 点数组 ptr/len）；没有逐字段定名 |
| `FunctionState` 的**精确大小** | **推断**（112 B） | 缺"一次性 `memcpy`/`drop` 整结构"或 `size_of` 常量；只能由 bucket 步长 `0x78` 反推 |
| `GraphwarFunctionManager +0x00` 弱自句柄**谁在用** | **未知** | 7 个方法**没有一处**读 `m+0x00`，本模块又**不订阅**事件（构造体内无 `subscribe`）⇒ "谁替它订阅"未观测到；字段本身的存在有三构建证据 |
| `draw_circle` 往网格里**写的值** | **推断**（非 `0xff`） | 660 字节函数体里的 `strb` 常量没逐条读 |
| 网格坐标换算的**语义**（`1000/50`、`+500`、`+300`） | **观测**（常量与算式）/ **推断**（"棋盘 50 单位 → 1000 像素"） | 缺一份真帧量测（棋盘像素宽到底是不是 1000） |
| `Vec<[f64;3]>` 三个 f64 的**顺序** | **推断**（`x, y, r`） | `is_destruction_queued` 只做逐分量 `ucomisd`；`draw_circle` 的入参顺序未证 |
| `queue_destroy_obstacle` 的**到期 tick 算法** | **未知** | 键是 tick（`handle_tick_event` 用 tick 直接 `remove_entry`，**观测**），但 `now + ?` 那个加法没解（§44.7 照旧） |
| 两个槽的 `stub` / `—` 字符串 | **已改**（§47） | 档位改成四档（`layout` / `wired`）、`impl_type` 报真类型；只剩 `graphwar_root_node_class.rs` 的两处**注释**与 `container_stub_manager_count` 这个**函数名**还带旧口径（`*_class.rs` 是禁区，见 §47.6） |
| `tools/coverage_matrix.py` 的**模块归属表** | 未改（`tools/` 不在本轮允许清单） | 表里这两个模块仍写 `src/game.rs` / `src/plane.rs`；本轮新增的两个文件只影响 `src_files`/`src_fns` 总数，**每模块的 527 行一行没动**。下一轮把 `ours` 加上这两个文件即可 |

### 46.9 复现（干净树）

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<你的>/target
cargo test --offline                                  # 1559，全绿
cargo test --offline --features gdext-bindings        # 1644，全绿（含本节 6 条）
python3 tools/coverage_matrix.py --check              # OK
python3 tools/check_doc_refs.py                       # OK（本节的小节号连续）
```

**结构断言在哪儿**：`src/graphwar_function_manager.rs` / `src/graphwar_obstacle_manager.rs`
末尾的 `mod tests`（`layout_matches_the_original_32_bytes` / `layout_matches_the_original_80_bytes`
逐字段 `offset_of!`），以及两个文件顶部**编译期**的 `const _: () = assert!(…)`。

## 47. `stub` 语义一次性改对：槽位复原档位 = `wired` / `layout` / `assembled` / `stub`（2026-11，任务 28）

**一句话**：`stub` 一个词原来**同时**表示"我方只有占位"和"零行为"。任务 19 把 3 个桩接上了
事件流与交互断言、任务 22 又把其中 2 个的**原版字段布局**解开了（§46），但**字符串没跟**，
于是 §44 / §46 的报告一直在用错误的现状判断下一步（同一个坑踩了两轮）。本节把
**结构**（原版字段布局解没解）与**行为**（容器 / 事件流 / 最小交互断言）拆成两条轴，
定成**四个词**写进 `src/_scaffold/root_wiring.rs` 的模块文档，并**同批**改了
`client/tools/assemble_check.gd` 的逐槽断言（**199 → 203 条**）——上一轮就是因为
"断言在禁区"只改了注释，才留下这个坑。

> 档位口径同 §42 / §44 / §46：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编
> 地址 / 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / `assemble_check.gd` 的条数**只是回归保护**，不是复原证据。

### 47.1 判据：四个词，代码与断言共用（原文在 `root_wiring.rs` 的模块文档）

| 档位 | 判据（**按序判断，第一个命中者为准**） | 现值 |
|---|---|---|
| `wired` | 已接容器 **+** 已接事件流 **+** 有最小交互断言（`RootWiring::interaction_selftest` 覆盖这一槽） | **4** → **10**（§50） |
| `layout` | 原版字段布局**已逐字段解出**（有证据档），但**不满足** `wired` | **2** |
| `assembled` | 容器里装的是**我方复原类型**（能按类型取回并调用），但**不满足**上面两条 | **15** → **9**（§50） |
| `stub` | 以上都不是：**我方形状占位**，原版字段布局**未知** | **1** |

```text
结构轴：stub（布局未知） ──→ layout（布局逐字段解出）
行为轴：assembled（装上我方复原类型） ──→ wired（+ 事件流 + 最小交互断言）
一个槽只报**它达到的更高那一档**；两条轴都到顶 = wired（任务 40 起：10 + 2 + 9 + 1 = 22）。
⇒ "结构已解但行为未做" = layout；"结构都没解" = stub —— 这就是本轮要根治的那个模糊。
```

三条读法（每条都有本轮的实际槽作反例，别合并省略）：

1. **`wired` 不要求布局逐字段解出**：`GraphwarTickHandler` 三件齐，但字段只落到有地址证据的
   4 个偏移（`+0x00..0x0f` 壁钟 / `+0x28` 1e9 ns / `+0x38` 事件队列 / `+0x40` `current_tick`），
   它就是 `wired`；布局那一轴单独记在 §47.2 与各 manager 文件的文档块里，**不**并进档位字符串。
2. **`layout` 不等于"零行为"**：`GraphwarFunctionManager` 布局全解（§46.2）、容器能取回、
   6 条交互断言都在跑，但**发方向的事件流没接**（§46.0 的就地更正 2 证明原版 3 个 `set_*`
   各发一条 `FunctionInfo` 类事件，我方 `FunctionTable` 一条都不发）⇒ 它停在 `layout`。
   真正"布局全解、行为 0 行"的是 `GraphwarClientConnectionHandler`（§47.2）。
3. **`assembled` 不等于"复原了"**：这 15 个只有"类型 + 部分行为 + 按类型可调"，原版布局多数
   **没逐字段解出**，事件流也没接全。

⚠️ **与本文件别处的 `SlotKind` 不是一回事**（这条最容易被误读）：`SlotKind` 是**容器类型来源**的
二分类——`RealType`（容器里直接装原版模块类型，**17** 个）/ `Stub`（容器类型是 `_scaffold` 的
包装或标记，**5** 个），`container_real_manager_count()` / `container_stub_manager_count()`
报的就是这两个数（§41.2 的既有口径）。档位回答的是"这一槽离原版还有多远"。
⇒ **`SlotKind::Stub` 的 5 个槽里，现在有 2 个档位是 `wired`、2 个是 `layout`、1 个是 `stub`**；
"5 个 stub" 与 "1 个 stub" 指的是两件事，引用时必须写清是哪一个。

### 47.2 5 个原 `stub` 槽的逐槽重判（任务书要求的那三条事实）

| 槽（原版尺寸） | 原版字段布局 | 事件流（订阅 / 发） | 最小交互断言 | 档位 | 差什么才能上一档 |
|---|---|---|---|---|---|
| `GraphwarTickHandler`（72 B） | **部分**：4 个偏移有指令证据，其余我方形状 | 订 **1**（`connected_to_server_event`）/ 发 **2**（`TickEvent`、`TickRequest`）——与原版一致（§44.2） | **4** 条 | `wired` | 布局：`get_fractional_tick` / `get_time_to_tick` 的算式（§44.7 仍列未知） |
| `GraphwarClientConnectionHandler`（112 B） | **逐字段全解**：`HandlerField` 9 条偏移 + `SIZE = 0x70 = 112`，与 §41.2 的 vtable size 对上（`src/graphwar_client_connection_handler.rs`，**观测**） | **未接**（订阅名 / 处理器 / `forward_event` 的 24 个请求事件名都有证据，代码里一条没接） | **0** 条 | `layout` | 行为：`connect` / `tick` / `disconnect` / `forward_event` 的状态机没落码（4 态只有纯函数 `tick_action` / `state_after_connect_attempt`） |
| `GraphwarPlanePosManager`（24 B） | **未知**：连 `src/graphwar_plane_pos_manager.rs` 都没有；已知常量（`px = 20x + 500`）以自由函数落在 `plane.rs` | **未接** | **0** 条 | `stub` | 结构：24 B 的字段布局一条都没解 |
| `GraphwarFunctionManager`（32 B） | **逐字段全解**：4 字段，三构建偏移序列逐条相同（§46.2） | 订 **0**（正面证据：原版也不订阅）/ 发 **0**，而原版 3 个 `set_*` 各 1 条 ⇒ **发方向未接** | **6** 条 | `layout` | 事件流：把 `FunctionInfo` 类事件写进 `FunctionTable::outbox`（tag `0x23` 已证，载荷 `(id, 文本, tick)` 只证了 tag） |
| `GraphwarObstacleManager`（80 B） | **逐字段全解**：5 字段，三构建偏移序列逐条相同（§46.3） | 订 **1**（`tick_event`）/ 发 **4**（`AddObstacle` / `DestroyObstacle` / `QueueDestroyObstacle` / `ClearObstacles`）——与原版一致 | **7** 条 | `wired` | 行为：`draw_circle` 写什么值、`queue_destroy_obstacle` 的到期加法（§46.8） |

两点补充：

* **`ClientConnectionHandler` 为什么算 `layout` 而不是 `stub`**：它的 112 字节布局是**逐字段**夹出来的
  （`new` 只读不写、`connect`/`tick`/`disconnect`/`forward_event` 各用固定偏移访问，交叉后没有空洞；
  `SocketAddr` 占 `+0x08..0x28` 32 字节、`JoinHandle` 3 个字 `+0x50/0x58/0x60`、`Latch` `+0x68`），
  总长 `0x70` 与 §41.2 的 vtable size 一个字不差 ⇒ **结构这一轴到底了，行为一行没写**。
  这正是 `layout` 这个档位存在的理由。
* **`FunctionManager` 的 "订 0 / 发 0" 不是"没查"**：订阅方向是**正面证据**（§44.3 那种正对照），
  发方向是 §46.0 就地更正过的**真结论**（经 GOT 槽 `0x9bac80` 的间接 `add_event`）。
  我方两条都没接（订阅本来就不该有），所以档位只能到 `layout`。

### 47.3 22 个槽的档位（现值）

> ⚠️ **任务 40 已改**（§50）：`wired` 4 → **10**、`assembled` 15 → **9**。下表是任务 28 当时的
> 快照，保留作对照；**现值请看 §50.8**。

| 档位 | 槽 | 依据 |
|---|---|---|
| `wired`（4 → **10**） | `TickHandler` / `TargetManager` / `LifeManager` / `ObstacleManager`（§44.2）+ 任务 40 的 6 个（§50.2） | §44.2 / §50.2 的订阅/发事件表 + `interaction_selftest` 的 4 / 5 / 5 / 7 条与 6 / 6 / 7 / 5 / 7 / 7 条 |
| `layout`（2） | `ClientConnectionHandler`（§47.2）/ `FunctionManager`（§46.2） | 布局逐字段解出，行为没做完 |
| `assembled`（15 → **9**） | 任务 40 之后剩 `LocalPlayerManager` / `SoldierManager` / `GameStateManager` / `GraphicsManager` / `ComputerPlayerManager` / `FunctionHistoryRecorder` / `NameManager` / `ColorManager` / `RankManager` | 容器里是我方复原类型，但没接事件流 / 没有最小交互断言（`GameStateManager` 的理由见 §50.4） |
| `stub`（1） | `PlanePosManager` | 布局未知 |

`SlotKind`（容器类型来源）仍是 **17 + 5 = 22**，一行没变；两个轴的对应关系见 §47.1 末尾那段。

### 47.4 落码：改了哪几处（**代码与断言同批**）

| 文件 | 改了什么 |
|---|---|
| `src/_scaffold/root_wiring.rs` | ①模块文档新增"槽位复原档位"一节（判据 + 三条读法 + 与 `SlotKind` 的边界）；②22 行台账加**档位**列（原"档位"列改名"证据"，避免一个词两个意思）；③新增 `SLOT_WIRED` / `SLOT_LAYOUT` / `SLOT_TIERS` / 四档计数常量 + **编译期闸**（合计必须 22）；④`Wired::status()` 改成四档判据；⑤`SlotKind` 的文档重写成"容器类型来源"，并标明**不是档位**；⑥3 个包装槽的 `our_type` 从 `—` 改成里面真正的类型（`graphwar_tick_handler::GraphwarTickHandler` 等）；⑦`FunctionTable` 的发件箱注释更正（原版**会**发、我方**没接**，档位因此 `layout`）；⑧§3 换成逐槽事实表；⑨单测：改 1 条（`unwired_module_types_are_placeholders`）+ 新 1 条（`slot_tiers_are_the_four_word_vocabulary`，逐槽钉死四档） |
| `client/tools/assemble_check.gd` | `STUBBED`（5 个名字）→ `WIRED_TIER`（22 个档位，与注册序一一对应）+ `TIER_TALLY` + `EMPTY_IMPL` + `PACKAGED` / `PACKAGED_TIER`；逐槽断言改查**四档**；`impl_type` 改断"只有两个纯标记是 `—`"；新增四档计数 / 合计 / 词表越界三条；5 个包装槽的反面段改成新档位 + "只剩 1 个真 stub" 一条 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑（`src/` 变了）：`src_fns` **2480 → 2481**（+1 = 新加的那条单测）、`src_tree_sha256` 跟着变；**527/527 行与每模块数字一行未变** |
| `docs/16-client.md` | 本节 + §41.4 / §41.6 / §38 那几处过期措辞的就地标注（§47.6） |
| `docs/02-module-map.md` | §17.3 加一段：`root_wiring.rs` 不在那张表里，档位判据落在哪、`SlotKind` 与档位的区别 |

❌ **没动**：`src/lib.rs`、任何 `*_class.rs`、manager 的纯逻辑、`client/src/**`、`dispatch/`、
`client/tools/` 下别的脚本、以及 `tools/bit_exact_attrib.py` 的基线（本轮它本来就是红的，见任务书 §3）。

### 47.5 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<你的>/target28
export GODOT=<ws>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
grep -a -c line_instance_id client/libgw2_rebuild.so          # 必须是 3（判据：.so 真的是新的）
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 203 passed, 0 failed
```

| 命令 | 本节实测 | 改前（`CG@0ea7729`） |
|---|---|---|
| `assemble_check.gd` | **203 passed / 0 failed**，退出码 0 | **199 passed / 0 failed** |
| `obstacle_drawer_check.gd` | **116** passed | 116 |
| `plane_on_screen_check.gd` | **76** passed | 76 |
| `function_drawer_check.gd` | **132** passed | 132 |
| `draw_classes_check.gd` | **235** passed | 235 |
| `curve_on_screen_check.gd` | **96** passed | 96 |
| `cargo test --offline` | **1569**，全绿 | **1569**（一位不差） |
| `cargo test --offline --features gdext-bindings` | **1653**，全绿 | **1652**（+1 = 新单测） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_fns` 2480 → 2481） | OK |
| `python3 tools/check_doc_refs.py` | **OK** | OK |

**新增 4 条的构成**：四档计数 1 + 四档合计 1 + 词表越界 1 + "只剩 1 个真 stub" 1。
既有 199 条**一条没删**：22 条逐槽状态断言换了**词**（`assembled`/`stub` → 四档），
22 条 `impl_type` 断言换了**判据**（原按档位、现在按"容器里有没有东西"），
其余 155 条数字与判据**一字未动**。

⚠️ 护栏三条轴（`fire` / `batch` / `batch_conv`）与延迟段**本节没跑也不该动**：
本轮只改注释、档位字符串、断言脚本与生成物，**一行几何/弹道/状态机代码都没碰**
（`cargo test` 1569 一位不差可以佐证默认轴没动）；三条轴的数字以协调者合并后的四件套为准。

### 47.6 就地更正清单（哪几处旧说法从本节起作废）

| 位置 | 旧说法 | 现在 |
|---|---|---|
| §41.4 | 「**5 个空占位**（`SlotKind::Stub`，`module_slot_status()` 如实回 `"stub"`，**不是** `"assembled"`）」 | 档位是 **2 `wired` + 2 `layout` + 1 `stub`**（§47.2） |
| §41.4 | 「`graphwar_tick_handler.rs` 只有 `tick_at` 一个自由函数」「`plane_pos` / `function` / `obstacle` **连文件都没有**」 | 任务 19 起 `graphwar_tick_handler.rs` 有 `GraphwarTickHandler`；§46 起后两个**有文件、有原版布局**；只有 `plane_pos_manager` 仍无文件 |
| §41.6 | 「5 个空占位的**类型本身** 未知 ⇒ 这 5 个型在本仓库没有可构造的类型」 | 3 个已可构造（两个是原版布局 + 我方表，一个是原版模块里的 `GraphwarTickHandler`），1 个有**全偏移表**（§47.2），1 个仍未知 |
| §38（`module_slot_status` 那段） | 「槽位**三态**：`assembled`（3 个）/ `placeholder` / `unknown`」 | §41 起是 22 个槽；§47 起状态是**四档** |
| §44.0 表 | 「这 3 个槽的档位 **仍然是 `stub` / `impl_type = "—"`**」 | `wired` / `wired` / `layout`，`impl_type` 报真类型路径 |
| §44.1 表 | 「+16 条：… 反面（桩仍报 stub）」 | 反面段现在断的是**四档**（5 个包装槽里 4 个不再是 `stub`） |
| §44.6 | 「5 条（5 个槽仍报 `stub`，反面）」 | **6 条**（5 条档位 + 1 条"只剩 1 个真 stub"） |
| §46.0 表 | 「容器槽里的档位字符串 **仍然是 `"stub"`**、`module_impl_type` 仍然是 `"—"`」 | `FunctionManager` = `layout`、`impl_type` = `graphwar_function_manager::GraphwarFunctionManager`；`ObstacleManager` = `wired` |
| §46.6 第 3 条 | 「槽的档位字符串**仍报 `"stub"`** … 要改字符串必须与 `assemble_check.gd` 一起改（**留给协调者**）」 | **本轮做完**（同批改了 `root_wiring.rs` + `assemble_check.gd`） |
| §46.8 表末第二行 | 「两个槽的 `stub` / `—` 字符串：**过期但未改**」 | 已改（本节） |

⚠️ **本轮仍没改的（禁区）**：`src/_scaffold/graphwar_root_node_class.rs` 里两处注释还写着旧口径——
`module_slot_status()` 的「`stub`（容器里查得到，但装的是**零行为的空占位**）」与
`container_stub_manager_count()` 的「空占位」。**函数名也别按档位读**：
`container_stub_manager_count()` 数的仍是 `SlotKind::Stub` = **5**（容器类型是 `_scaffold` 包装），
**不是**档位 `stub` 的 **1**。`*_class.rs` 是 `AGENTS.md` 合并纪律 A-1 的禁区，本轮没碰，
下一轮放开时把那两处注释与两个函数名一起改（`assemble_check.gd` 那边已经把文案改成
"SlotKind::Stub，不是档位 stub"，可以在那儿先读）。

### 47.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `FunctionManager` 的**发方向**事件流 | **未知**（原版**有**，我方**没接**） | 原版 3 个 `set_*` 各发一条（§46.0）；tag `0x23` 已证，`FunctionActive` 那两条与载荷另两段没逐字节核 |
| `ClientConnectionHandler` 的**行为** | **未知** | 4 态状态机只有纯函数；`connect_timeout` / 连接线程 / `forward_event` 的线程与 socket 语义没落码 |
| `PlanePosManager` 的**字段布局** | **未知** | 24 字节结构体的偏移一条都没有；`px = 20x + 500` 只在 `plane.rs` 的自由函数里 |
| 15 个 `assembled` 的**原版布局** | 多数**未知** | 本节只逐槽核了 5 个原桩 + §46 的两个；其余按 §41.6 的既有结论（我方类型尺寸与原版不等）判"没逐字段解出" |
| 档位本身的**判据强度** | **我方口径** | 档位是**静态台账**，只说"做到哪一档"，**不说**行为对不对；行为正确性仍只能由真二进制决定（`AGENTS.md` §0 第 2/5 条） |
## 48. 播放屏那 3 个"可解"缺口的证据补齐：tick 算式 / `update_transform` 三实参 / 两张颜色表（2026-11，任务 32）

> 本节对应 `dispatch/32-play-screen-evidence-gaps.md`（分支 `agent/play-screen-gaps`）。
> §43.6(d) 的 7 个缺口里**3 个是"证据缺"**：本节把它们补上，并**就地改正**了两条旧推断
> （§42.3 那句"两张表来自 `get_team_overview`"、§42.5 第 4 条那句"`Vector2(w/50,0)` 自相矛盾"）。
> 剩下 4 个（点生产者归属 / 淡出的 GOT 槽 / `z_index` 初值 / `FunctionDrawer::draw`）**没动**。

### 48.0 一页结论

| # | 缺口 | 结论 | 算式 / 内容 | 落地位置 | 档位 |
|---|---|---|---|---|---|
| 1 | tick 源 | **解出** | `fractional = tick + ((now − base) − accum) / 0.016666666`，`0.016666666` = `.rodata 0x19f7f8` = `f64(16_666_666)/1e9`（**不是** `1/60`） | `graphwar_tick_handler::GraphwarTickHandler::get_fractional_tick`（`0x519530`）；`GraphwarPlane` 自己持有并每帧读 | **观测**（逐指令）+ **逐位**（除数位型） |
| 1b | 连带的 `get_time_to_tick` | **解出**（顺带） | `(accum + 1 tick) − (now − base)` | 同文件 `get_time_to_tick`（`0x519450`） | **观测** |
| 2 | `update_transform` 三实参 | **解出** | `x = w/2`、`y = h/2`、`scale = s = w/50`（f32 标量）+ `flip` 位；被调者自己拼成 `flip ? (−s,−s) : (s,−s)` | `GraphwarPlane::apply_update_transform`（调用点 `0x4b6147`） | **观测**（逐指令） |
| 3 | `get_team_overview` 返回什么 | **解出，且旧说法被否** | **`u32` 的 4 个布尔位**（bit 0/8/16/24），只用于 `flip`；**不是**两张表 | `GraphwarPlane::flip_from_overview_bits` | **观测** |
| 3b | 两张表（队色） | **解出** | 两个 6 色池（队号 0/1），12 个 `(r,g,b)`，见 §48.3 表 | 表本来就在 `src/graphwar_color_manager.rs`（`TEAM1_COLORS`/`TEAM2_COLORS`）；本节补 **x86 观测**与 `team_color_pool()` 出口 | **逐位**（12 条 `mov word`+`mov byte`） |
| 3c | 棋盘内部底色 | **解出归属** | **不是平面画的**：`AxisDrawer::draw` 的 `draw_rect(filled=true, Color(1,1,1,1), rect(−25,−15,50,30))` | `GraphwarPlane.board_fill`（导出属性，**默认关**）照那组实参补画 | 原版**观测**；"由平面代画"是**我方接线** |

**一句话**：三个缺口都解出来了，而且都是**逐指令**级别的；唯一还是"我方"的只剩**接线**
（tick handler 的对象还在游戏状态里没接过来、`flip` 用一个导出属性顶替、白底由平面代画）。

### 48.1 缺口 1：`get_fractional_tick`（`0x519530`）的算式

函数体 `0x519530`–`0x5195f2`（其余分支是两条下溢 `panic`，`.rodata 0x1c649f` / `0x1c63b4`）。
`self` = `M`（72 B，`new` = `0x519690` 分配 `0x60` = 16 B Rc 头 + 8 B + 72 B）：

```text
519534  mov  esi,[rdi+0x28]        ; M+0x28 = "now" 的纳秒分量
519537  cmp  esi,0x3b9aca00        ; == 1e9 ? ⇒ **哨兵**（合法纳秒 < 1e9）
51953d  jne  519555                ; 有覆盖值 ⇒ now = (M+0x20, M+0x28)
51953f  call *0x9ba8f8 (0x8f3a10)  ; 否则 SystemTime::now() → (rax=secs, edx=nanos)
519545  mov  r8,[rbx]              ; base.secs  = M+0x00
519548  mov  ecx,[rbx+8]           ; base.nanos = M+0x08
51955f  call *0x9baf28 (0x90cdd0)  ; Duration::checked_sub(now, base)
519565  sub  rax,[rbx+0x10]        ; − accum.secs  = M+0x10    （jb ⇒ panic 0x1c649f）
519588  sub  edx,ecx               ; − accum.nanos = M+0x18
5195a9  …                          ; secs(u64) → f64：punpckldq 0x19c780 / subpd 0x19c000 那套
5195ce  divsd xmm2,[0x19eff8]      ; nanos / 1e9   （0x19eff8 = f64 1e9）
5195d6  addsd xmm2,xmm1            ; + secs        ⇒ 秒
5195da  divsd xmm2,[0x19f7f8]      ; / 0.016666666 ⇒ 不足一 tick 的小数部分
5195e2  mov  eax,[rbx+0x40]        ; M+0x40 = u32 tick
5195ed  addsd xmm0,xmm2            ; tick + 小数部分 ← 返回值（f64）
```

**算式**（`elapsed = (now − base) − accum`）：

```text
get_fractional_tick() = tick as f64 + elapsed.as_secs_f64() / 0.016666666
```

* `0.016666666` 的 8 字节 `.rodata 0x19f7f8` = `0x3f911111059d0921` = **`f64(16_666_666) / 1e9`**，
  **不是** `1/60`（`1/60` = `0x3f91111111111111`）。它与 `tick()` 每次累加的
  `NANOS_PER_TICK = 0xfe502a = 16 666 666` **同源**。⇒ 单测 `divisor_is_the_bit_exact_rodata_constant`
  直接钉位型（`tick_seconds().to_bits()`）。
* `accum` 每 `tick()` 加 `16 666 666 ns`（`0x51986c` 的 `add eax,0xfe502a`），所以
  `elapsed` 是"**这一 tick 已经走了多久**"。
* 下溢（`now < base + accum`）在原版是 `panic!`；本实现给 `0.0`（gdext FFI 里 panic = abort）。

**顺带解出的 `get_time_to_tick`（`0x519450`）**：`(accum + 1 tick) − (now − base)`，
进位判据 `0x51948f` 的 `cmp edi,0x3a9c79d6`（= `1e9 − 16 666 666`）。

**字段归属（72 B 全对上；`new` 的写点 + 上面两处读点）**

| 偏移 | 内容 | 指令 |
|---|---|---|
| `+0x00`/`+0x08` | `base: SystemTime` | 写 `0x5196df`/`0x5196e3`；读 `0x519545`/`0x519548` |
| `+0x10`/`+0x18` | `accum: Duration`（每 tick +16 666 666 ns） | 写 `0x519871`/`0x519875`（`tick` 里）；读 `0x519565` |
| `+0x20`/`+0x28` | `now` 的**覆盖值**；`+0x28 == 1e9` = "没设" | `0x5196f5` 只写哨兵；读 `0x519555` |
| `+0x30` | 自身指针（用途未知） | `0x5196fc` 的 `mov [rbx+0x48],rbx` |
| `+0x38` | `Rc<RefCell<GraphwarEventQueue>>` | `0x519700` |
| `+0x40` | `u32 tick` | 写 `0x51987d`；读 `0x5195e2` / `0x4b6317`（`draw` 里） |

⇒ **就地更正**（旧代码注释把 `+0x28` 的 `1e9` 读成"一秒的纳秒数"）：`0x519537` 的
`cmp …/jne` 表明它是**"now 覆盖值无效"的哨兵**。

**落地**：`src/graphwar_tick_handler.rs` 的 `GraphwarTickHandler::get_fractional_tick` /
`get_time_to_tick` / `now()` / `set_now()` / `clear_now()`（`frozen_now: Option<(u64,u32)>`
⇔ `M+0x20/+0x28`），以及 `GraphwarPlane` 侧：

```text
GraphwarPlane::fractional_tick()   -> tick_source.get_fractional_tick()      # 原版 0x4b631a
GraphwarPlane::integer_tick_now()  -> tick_source.current_tick as i64        # 原版 0x4b6317 (M+0x40)
GraphwarPlane::set_fractional_tick(t)                                        # **我方**摆钟口
    n = trunc(t)；accum = n×16 666 666 ns；now = accum + round(frac × 0.016666666 × 1e9)
    ⇒ 整数 t 读回来**逐位相等**（验收脚本按整数 tick 断言，不会因浮点抖动变红）
```

⚠️ **仍是接线不是复原**：原版那个 `GraphwarTickHandler` 在游戏状态里
（`draw` 里 `mov r14,[r12+0x238]` 借出来的 `Ref`，`0x4b61e5`–`0x4b631a`），本类还没有那条线
⇒ 自己 `new` 一个。`src/_scaffold/root_wiring.rs` 里那句"`get_fractional_tick`/`get_time_to_tick`
的算式（§44.7 仍列未知）"**已过期**，但那个文件不在本任务的允许改动清单里，留给下一个改它的任务。

### 48.2 缺口 2：`update_transform` 的三个实参（逐指令）

**先纠正任务书的一个地址**：调用点在 **`GraphwarPlane::draw`（`0x4b5df0`）体内 `0x4b6147`**，
**不在** `ready`（`0x4b7a70`）里。正对照：`callers 0x499050` = **1**（只有 `0x4b6147`），
同一套扫法对 `update_function`（`0x498c70`）给出唯一调用点 `0x4b69ec`，与 §42.3 一致。

调用点逐指令（`0x4b60a0`–`0x4b6147`）：

```text
4b60a0  mov  rdi,[rax+0x70]  ; 4b60a4 call 0x44ac60（拿 Gd 句柄）→ 4b60c9 前一路 upcast
4b60c9  call *0x9bb000 (0x8925f0)  ; Control::get_size() → xmm0 = w, xmm1 = h
4b60d4  movss [rsp+8],xmm0         ; w
4b60da  movss [rsp+0x10],xmm1      ; h
4b60e3  call 0x4be6e0              ; get_team_overview(...) → eax = **u32 的 4 个位**
4b60e8  movss xmm0,[0x1ba128]      ; 0.5f32（同一个 8 字节池的下一个 f32 是 30.0）
4b60f0  movss xmm1,[rsp+0x10] ; 4b60f6 mulss xmm1,xmm0    ; **arg2 y = h × 0.5**
4b60fa  movss xmm2,[rsp+8]    ; 4b6104 divss xmm2,[0x1b9fc8]  ; **arg3 scale = w / 50.0**
4b6100  mulss xmm0,xmm2(=0.5)                              ; **arg1 x = w × 0.5**
4b610c–4b6129   flip 位组合（见下）
4b6132  lea  rdi,[rax+0x80]        ; 被调者 = 平面上的函数绘制器
4b6139  movzx esi,dl               ; **arg4 flip（bool）**
4b6147  call 0x499050              ; FunctionDrawer::update_transform
```

`flip` 的位组合（两处同一个表达式：`0x4b6111`–`0x4b6129` 给 `esi`，`0x4b600d`–`0x4b6029`
给 `draw_set_transform` 的 x 分量）：

```text
A = (flags >= 0x1000000) && !(flags & 0x10000)     ; 4b6012 setae / 4b601b sete / 4b6024 and
B = ((flags & 0x101) == 0x100)                     ; 4b600d and / 4b605c cmp / 4b6129 or
flip = A || B
```

**被调者（`0x499050`）怎么用这三个实参**（这是"第三实参不是 `Vector2`"的证据）：

```text
49905e  movd  [rsp+0xc],xmm1      ; 存 y
499064  movss [rsp+8],xmm0        ; 存 x
499080  movdqa xmm0,[0x19e880]    ; 四个 -0.0 的符号掩膜
499088  movdqa [rsp+0x20],xmm2    ; 原值那份（低 32 位 = s）
49908e  pxor  xmm0,xmm2
499092  movdqa [rsp+0x10],xmm0    ; 取负那份
4990ef/4990f5  set_position([rsp+8],[rsp+0xc])          ; 两个 f32
499114/49911c  set_width(0.125f32 @0x1baff8)
499135  movdqa xmm0,[rsp+0x10]    ; **flip 分支**：取负那份
49913b  movdqa xmm1,xmm0
49913f  call *0x9bb238            ; set_scale(xmm0, xmm1) ⇒ (-s,-s)
499235  movdqa xmm0,[rsp+0x20]    ; **非 flip 分支**：xmm0 = 原值（s）
49923b  movdqa xmm1,[rsp+0x10]    ;                  xmm1 = 取负（-s）
499241  call *0x9bb238            ; set_scale ⇒ (s,-s)
```

gdext 的 `Vector2` 在这里是**两个 f32、各占一个 xmm**（`set_position` 的包装 `0x892890`
开头就是 `movss [rsp+0xc],xmm1; movss [rsp+8],xmm0`，`set_scale` 的 `0x892b50` 同样）。
⇒ `update_transform(x: f32, y: f32, scale: f32, flip: bool)`，**`scale` 是标量**，
y 分量的负号是被调者自己加的。

**同一套换算也出现在 `draw_set_transform`（`0x4b607d`）**：`pos = size×0.5`、`rotation = 0`、
`scale = (flip ? −s : s, −s)`（`0x4b6001` 的 `pxor` 给 y、`0x4b6059`–`0x4b6063` 选 x）。
⇒ 曲线上那 3 个 `Line2D` 与棋盘框**共用**一个变换。

**旁证**：`AxisDrawer::draw`（`0x4e05b0`）用的是**同一个**算式，只是没有 `flip`
（`0x4e0639` `divss 50.0`、`0x4e064b` `0.5`、`0x4e0648` 取负）⇒ `scale = (s, −s)`。

**⇒ 原来那组 `position=(500,300)` / `scale=(20,−20)` 的数值恰好就是原版值**
（1000×600 的平面 ⇒ `w/2 = 500`、`w/50 = 20`），但**来源从"我方选择"变成"算出来的"**。
`client/tools/plane_on_screen_check.gd` 已改成按 `plane.size` 反推期望值再断言。

⚠️ 还是我方的两处：
1. `flip` 的来源（原版 = `get_team_overview` 的位组合；本类没有游戏状态 ⇒ 导出属性 `mirror_x`，
   默认 `false` = 不镜像；另有一个导出属性 `flip_y`（默认 `true`）只管"y 是否取负"，
   原版**永远取负**，那个开关只为复现工具留）；
2. 喂给 `FunctionDrawer::update_transform` 的 `scale` 得先"反着取负"
   （那个文件本轮不许改：它在 `flip` 为真时把**两个**分量都取负 ⇒ 想得到 `(−s,−s)`
   就得传 `(s,s)`）。

### 48.3 缺口 3：`get_team_overview` 返回什么 + 两张表 + 棋盘底色

**(a) `get_team_overview`（`0x4be6e0`）返回 `u32` 的 4 个布尔位，不是表。**
函数尾部 `0x4beb95`–`0x4bebb6` 就是拼装：

```text
4beb95  movzx eax,[rsp+0x38] ; shl eax,0x18        ; bit 24
4beb9d  movzx ecx,[rsp+4]    ; shl ecx,0x10        ; bit 16
4beba7  movzx edx,[rsp+8]    ; shl edx,8           ; bit 8
4bebb1  movzx eax,[rsp+0xc]                        ; bit 0
4bebb6  or    eax,edx   ⇒ eax = (b38<<24)|(b4<<16)|(b8<<8)|(bc)
```

四个字节在 `0x4be83e`/`0x4be847`/`0x4be84f`/`0x4be857` 清零，循环体里被置
`0`/`1`（`0x4be870`–`0x4be888`、`0x4bead4`–`0x4beaee`）。
**⇒ `draw` 里两次调用（`0x4b5fd3` / `0x4b60e3`）拿到的只是一个 4 字节标志字**，
第二次的 `eax` 就是 `flip` 的位组合（§48.2）。**§42.3 那句"两张表来自它的两次返回值"
是错的，已就地更正。**

它内部在干什么（形状，**语义未知**）：从 `[rdi+0x40]` 的 `Rc<RefCell<…>>` 借 4 个东西
（`0x46c430` / `0x46b460` / `0x46b8b0` / `0x46b740`），遍历里面一个 `Vec<u32>`
（元素 4 字节、`lea rax,[rax*4]`），再按 `0x78` 的 bucket 步长查表（与函数管理器同一个步长）
⇒ **推断**：逐个玩家看队/在场情况，汇总成 4 个"是不是"结论。缺的证据：那 4 个借用器
各自指向哪个字段、以及 4 个位各叫什么。

**(b) 真正的"两张表" = `init_team_colors` 的两个 6 色池。**
容器是 `HashMap<u8, Vec<GraphwarColor>>`（键 = 队号 0/1，`GraphwarColor` = **3 字节 r,g,b**）。
x86 侧**不是内联**（`docs/10` §15.2 记的"内联"要改成这个地址）：**`0x465070`–`0x46545e`（1006 B）**，
12 个颜色在 `0x4650a3`–`0x465297` 用 `mov word [r+3k], 0xRG` + `mov byte [r+3k+2], 0xBB` 落库：

| 队 | 地址（每个 2 条指令） | 6 个 `(r,g,b)` |
|---|---|---|
| 0（Team1） | `0x4650a6/aa`、`0x4650cd/d1`、`0x4650f5/f9`、`0x465124/28`、`0x46514f/53`、`0x465176/7a` | `(186,42,32) (238,114,0) (125,104,0) (122,160,0) (149,5,62) (234,0,219)` |
| 1（Team2） | `0x4651b4/b8`、`0x4651de/e2`、`0x465209/0d`、`0x46523c/40`、`0x46526a/6e`、`0x465293/97` | `(107,0,234) (17,0,108) (21,83,221) (0,203,163) (0,116,26) (2,85,63)` |

逐位读数：`0x2aba`+`0x20` = (186,42,32)；`0x72ee`+`0x00` = (238,114,0)；`0x687d`+`0x00`
= **`(125,104,0)` = `#7D6800`**（就是真帧量测出来的那个弹道色，`docs/16` §12.2 ⑫）；
`0xa07a`+0 = (122,160,0)；`0x0595`+`0x3e` = (149,5,62)；`0x00ea`+`0xdb` = (234,0,219)；
第二池 `0x006b`+`0xea`、`0x0011`+`0x6c`、`0x5315`+`0xdd`、`0xcb00`+`0xa3`、`0x7400`+`0x1a`、
`0x5502`+`0x3f`。与 `docs/10` §15.3 的 arm64（`0x49c660`）复算**一字不差** ⇒ 档位从"arm64 逐位"
升级成**两架构互证**。

**表本来就有**（`src/graphwar_color_manager.rs` 的 `TEAM1_COLORS` / `TEAM2_COLORS`，`TEAM_POOL_LEN = 6`）；
本节新加的只是**出口**：`GraphwarPlane::team_color_pool(team) -> PackedColorArray`
（`0..=1` 给 6 个，别的队号给空 = 原版 `add_team_color` 的随机兜底），
验收脚本逐个分量对拍 12 个数。

**(c) `draw` 里那个"颜色"是怎么查出来的**（§42.3 的表结构，这里补形状）：

```text
4b681d  mov rax,[rsp+0xd0]        ; 表 1（HashMap<u32,u32>）
4b6842  call 0x47dec0             ; hash(key = player id)
4b68a3  cmp ebp,[r10-8]           ; 比 key
4b68de  mov r15d,[rcx+r8*8-4]     ; value = 表1[player id]（4 字节）⇒ 当表 2 的 key
4b68eb  mov rax,[rsp+0xc8]        ; 表 2
4b69a7  movzx eax,[rcx+r8*8-4]    ; 3 个**字节**：r
4b69ad  movzx edx,[rcx+r8*8-3]    ;                  g
4b69b3  movzx ecx,[rcx+r8*8-2]    ;                  b
4b69be–4b69d1  r9d = (b<<16)|(g<<8)|r      ; = update_function 的颜色实参
```

⇒ 表 2 是 `HashMap<u32, GraphwarColor>`（bucket 8 = 4 字节 key + 3 字节值 + padding），
与 `GraphwarColorManager` 的 `used` 表形状一致（`used_map` 的键 = entity id）。
**推断（未证）**：表 1 = "player id → entity/队"，表 2 = 颜色管理器的占用表；
两个表的来源是两个借用器 `0x46a770`（→ `[rsp+0xd0]`）与 `0x46b5d0`（→ `[rsp+0xc8]`），
对象是 `draw` 里 `mov r12,[…+0x238]` 的那个游戏状态。缺的证据：那两个字段的**归属**。

**(d) 棋盘内部的底色：不是队色，也不在平面里。**
`GraphwarPlane::draw` 只有那一条**空心黑框**（`Rect2(−25,−15,50,30)`、`Color(0,0,0,1)`、
`width = 0.2`、`filled = false`，`0x4b617d`–`0x4b61ab`）。**白底在 `AxisDrawer::draw`**：

```text
4e069f  movaps xmm0,[0x19cc20]   ; Rect2(-25,-15,50,30)
4e06ab  lea    rdx,[0x19ed00]    ; Color(1,1,1,1)
4e06b7  movss  xmm0,[0x1b9d08]   ; width = -1.0
4e06c2  mov    ecx,1             ; filled = true
4e06c7  xor    r8d,r8d           ; antialiased = false
4e06ca  call   *0x9bc028         ; draw_rect
```

⇒ **"棋盘内部是工程底色"的真正原因是 `AxisDrawer` 节点还没实现 `draw`**
（`src/_scaffold/axis_drawer_class.rs` 文件头明写"没接管 `draw`"），不是缺一张颜色常量表。
本类给了导出属性 `board_fill`（**默认关**）：置真就在同一套变换下补画那块白底。
**默认关的理由**：其它验收脚本按"每次 `_draw` 1 条 `draw_rect`"断言
（`draw_classes_check.gd` / `assemble_check.gd`），打开会变 2 条。

### 48.4 落了哪些文件

| 文件 | 改了什么 |
|---|---|
| `src/graphwar_tick_handler.rs` | 新增 `tick_seconds()`（+`TICK_SECONDS_BITS` = `0x3f911111059d0921`）、`duration_sub()`、`get_fractional_tick()`、`get_time_to_tick()`、`now/set_now/clear_now`、`accum()`；字段按 `M+0x00..0x48` 重排；文件头补 `0x519530`/`0x519450` 的逐指令算式表；**6 条新单测** |
| `src/_scaffold/graphwar_plane_class.rs` | `tick_source: GraphwarTickHandler`（不再存 `fractional_tick: f64`）；`_draw` 按 `size` 派生 `position`/`scale` 并调 `apply_update_transform`（`0x4b6147`）；新导出属性 `mirror_x`（= 原版 `flip`）、`board_fill`；纯函数 `flip_from_overview_bits` / `board_transform_args` / `board_scale_xy`；新方法 `transform_total` / `last_transform` / `integer_tick_now` / `team_color_pool` |
| `client/tools/plane_on_screen_check.gd` | 变换断言改成**按 `plane.size` 反推**（+`last_transform()` 对拍）；新增 tick 源、两张队色表、`flip` 真值表（9 个点）、`board_fill` 打开后每次 `_draw` 2 条 `draw_rect` 的断言（76 → **96** 条） |
| `docs/16-client.md` | 本节 + §42.3 / §42.5 第 4 条 / §43.6(d) 的第 2/3/7 条 / §43.7 就地更正 |

### 48.5 验收（本分支实测，可从干净树重跑）

```text
cargo test --offline                       # 1575 passed / 0 failed（基线 1569 + 6 条新单测）
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd   # 96 / 0（基线 76）
"$GODOT" --headless --path client --script res://tools/obstacle_drawer_check.gd   # 116 / 0（不退）
"$GODOT" --headless --path client --script res://tools/assemble_check.gd          # 203 / 0（不退）
"$GODOT" --headless --path client --script res://tools/draw_classes_check.gd      # 235 / 0（不退）
"$GODOT" --headless --path client --script res://tools/curve_on_screen_check.gd   # 96 / 0（不退）
```

### 48.6 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `get_team_overview` 那 4 个位**各是什么** | **推断** | 只证了"4 个布尔位 + `flip` 的位组合"；4 个位对应哪个判定、`0x46c430`/`0x46b460`/`0x46b8b0`/`0x46b740` 四个借用器指向哪些字段都没解 |
| `draw` 里两张 `HashMap` 的**归属** | **推断** | 形状是观测（表 2 的值是 3 字节 `GraphwarColor`）；哪个模块的字段、`0x46a770`/`0x46b5d0` 借的是谁，未证 |
| `GraphwarTickHandler` 的 `M+0x20/+0x28`（now 覆盖值）**谁写** | **未知** | 全二进制只观测到 `new` 写哨兵 `1e9`；原版可能从不写它（那就永远走系统时钟） |
| `M+0x30`（自身指针）**用途** | **未知** | 只观测到 `new` 里写 `rbx` |
| tick handler 的**对象来源** | **我方接线** | 原版从游戏状态借（`mov r14,[r12+0x238]`），本类自己 `new`；接过来要先复原那个 `Ref` 的所有者 |
| `flip` 的来源 | **我方替换** | 原版 = `get_team_overview` 的位组合；本类用导出属性 `mirror_x` 顶替（默认 `false`） |
| 棋盘白底由谁画 | 原版**观测** / 本类**我方接线** | 原版在 `AxisDrawer::draw`；那个节点没实现 `draw`，本类用 `board_fill` 代画。**网格线仍然没有**（`AxisDrawer` 的 102 条 `draw_line` 还没接） |
| 剩下 4 个缺口 | **未动** | 点生产者 `0x456810` 的写入者 `0x3e44d4` 归属、`get_function_fade_on_tick` 只有 GOT 槽 `0x9bbd08`、`z_index` 初值、`FunctionDrawer::draw` x86 未定位 |

### 48.7 一个踩过的坑（**地址空间**，下一个做反汇编的人会用到）

`libgraphwarrust.so` 的两个 `PT_LOAD` 里，`.text` 段的 **`p_offset` = `0x37a940` 而
`p_vaddr` = `0x37b940`（差 `0x1000`）**，第一个（R）段才是 `offset == vaddr`。
⇒ 用 Python 直接按**文件偏移**搜字节时，`.text` 里的命中要 **+0x1000** 才是文档/工具里的 vaddr
（本次就是在文件偏移 `0x4640d1` 搜到 `mov word […],0x72ee`，对应 vaddr **`0x4650d1`**）。
`.rodata` 在第一个段里，`offset == vaddr`（`.rodata 0x19f7f8` 直接读就对）。
`tools/addr_probe.py` 打印的与 `@0x…` 解析的都是 **vaddr**，两者不要混。

## 49. 六个真 GDExtension 节点挂进播放屏：`plane_view.gd` / `fire_fx.gd` 删除（2026-11，任务 31）

**一句话**：播放屏（`client/src/screens/play_screen.gd`）的战场不再由 GDScript 自绘 ——
**棋盘框 / 曲线 / 士兵 / 网格 / 障碍 / 特效** 六样东西现在分别由 `GraphwarPlane` /
`FunctionDrawer` / `EntityDrawer` / `AxisDrawer` / `ObstacleDrawer` / `EffectDrawer`
六个 **Rust 扩展类**画，两个仿制 `.gd`（`plane_view.gd` 527 行、`fire_fx.gd` 261 行）删除。

**这是"客户端迁回 Rust"的第一刀**（`AGENTS.md` §0 第 4 条那条路的第一个落地点），
**不是**"客户端复原完了"：§49.5 列了 8 条还缺的，其中**准备圈在画面上已经没有了**。

### 49.0 先说没做到的（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| **准备圈**（`docs/140` 那三弧 + 三箭头） | **绘制已补（§50），触发仍未接**：`EntityDrawer::draw`（`0x4b2890`）那 6 条图元现在由 `_draw` 真的发出去（3 `draw_arc` + 3 `draw_polygon`），但只有 `set_ring()` 填过的实体才画，而播放屏还没人调它 ⇒ **画面上仍然没有** | 绘制=**观测**（118 条断言）；触发=缺口 |
| **士兵/靶子的真贴图** | `EntityDrawer` 用的是**程序生成的 8×8 纯色**（皮肤/脸/帽子三块方块），不是 pck 里的 `skins/*`、`faces/*`、`hats/*` | 缺口（§33 对照表早就记了） |
| **tick 源** | 原版 `GraphwarTickHandler::get_fractional_tick`（`0x519530`）没接 ⇒ 播放屏按**帧间隔**自己推 | 缺口（§43.6(d) 第 2 条） |
| 点的生产者 | 仍由播放屏把内核算出的弹道 `set_function_points()` 进去（`0x3e44d4` 归属仍未证） | 缺口（§43.6(d) 第 1 条） |
| 队色表 / 淡出 / `z_index` / `FunctionDrawer::draw` | 与 §43.6(d) 第 4–7 条**一模一样**，没有变化 | 缺口 |

### 49.1 本轮动了哪些文件

| 文件 | 动作 | 说明 |
|---|---|---|
| `client/src/screens/play_screen.gd` | 改（+698/−？） | 6 个真节点接线的本体；44 处 `_plane.*` 全部改到真节点 API；新增 `_sync_entities()` / `_set_curve()` / `_advance_curve()` / `plane_points()` / `plane_state()` 等 |
| `client/src/game/effect_layer.gd` | 改（自绘 → 适配，211 行里删掉 ~120 行） | 不再 `draw_arc`/`draw_circle`；坐标映射整段删除，改成 `EffectDrawer.spawn_effect()` + `retain_alive()` |
| `client/src/play/plane_view.gd`（`GwPlaneView`，527 行） | **删除** | 六个位置全被真类接管 |
| `client/src/play/fire_fx.gd`（`GwFireFx`，261 行） | **删除** | 曲线揭示改用 `GraphwarPlane` 的 30 点/tick；准备圈**没有替身**（§49.5 第 1 条） |
| `client/src/play/plane_view.gd.uid` | **删除** | 上面那个脚本的 uid 存根（脚本没了，存根也就没了） |
| `client/tools/selftest.gd` | 改（44 条断言换主语 + 4 条新增，净 **+15** 条） | 详见 §49.7；**没有删断言**，只换主语；新增的 4 条是"网格/实体/障碍真的被引擎画过"与"画布真的被容器铺开" |
| `src/**` | **一个字没动** | 六个节点类上一轮就都在（§33 / §35 / §34 / §43 / §45），本轮只接线 ⇒ `cargo test` 不用重跑新逻辑（仍是 1569），`coverage_matrix.py` 也不用重生成 |

### 49.2 现在播放屏里的节点树（以及"画布模型"这条我方选择）

```text
PlaneContainer (AspectRatioContainer, ratio = 50:30, STRETCH_FIT)   ← 原版同名同参数
 └ PlaneCanvas (Control)                     ← 拿到容器铺的 5:3 矩形；clip_contents = true
     └ GraphwarPlane  (size 1000×600, scale = fit)   ← ① 棋盘框（`_draw` 的 draw_set_transform + draw_rect）
         ├ @FunctionDrawer@N ×3  (z=2)       ← ③ 曲线（每 player 一条 Line2D，平面 `_draw` 里喂点）
         ├ AxisDrawer            (z=-1)      ← ④ 白底 + 网格（`axis_primitives`）
         ├ ObstacleDrawer        (z=1)       ← ⑥ 障碍掩膜贴图（`update` 每帧 2 次）
         ├ EntityDrawer          (z=3)       ← ⑤ 士兵 / 名字 / 靶子（每实体 3 sprite + 1 标签）
         ├ EffectDrawer          (z=4)       ← ② 特效（`frame_primitives`）
         └ EffectLayer           (适配层)     ← 只记账 + 派给 EffectDrawer，**不画**
```

**画布模型是"我方选择"**（`§43.6(d)` 第 3 条：原版 `draw_set_transform` 的三个实参没解出来）：
六个类各自按**自己控件的尺寸**换算 —— `axis_drawer` / `effect_drawer` 用 `size/50` 与
`size/2`，`entity_drawer` 用 `size.x/50` + `size/2`，`obstacle_drawer` 用 `size/1000` +
`size/2`，`graphwar_plane` 用固定的 `PX_PER_UNIT = 20` 与 `PIXEL_OFFSET = (500,300)`。
要把它们摆进同一个坐标系，只有一种不改 Rust 的做法：**把它们全放到 1000×600 的固定画布上**
（`src/plane.rs::CANVAS_W/H`，也是 `obstacle_drawer` 掩膜、`axis_drawer` 白底、`to_pixel()`
共同的那块画布），再用 `Control.scale` 把整块画布缩放进 `AspectRatioContainer` 给的矩形。
⇒ 播放屏只做**一处**缩放（`_fit_canvas()`），六个类内部一个数都不用改。
`--headless` 的视口是 64×64，缩放比会很小，但断言全部是"点数 / 条数 / 像素数"这类
**不依赖窗口尺寸**的量（§49.7 的口径），所以 headless 能验。

**z 序也是我方选择**（`GraphwarPlane` 的 `z_index` 初值是 `未知`，§40.3f）：`AxisDrawer`
用 `z_as_relative = false` + `z_index = -1` 画在棋盘框**之下**（否则它那张 1000×600 白底会
盖住平面的黑框 —— 引擎先画父再画子）；其余四层按"障碍 < 曲线 < 士兵 < 特效"排。

### 49.3 `effect_layer.gd`：从"自绘"变成"适配"（附一张**我方选择**的映射表）

| 项 | 旧（自绘） | 新（适配真节点） |
|---|---|---|
| 形状 | 手画环/火舌/实心圆/十字/方点（**自定**） | `effect_drawer::frame_primitives` 的四类图元（半径 0.05/0.06、四组基色、`max(0,(1−t)·0.9)`，全是 `.so` 立即数） |
| 坐标 | 靠 `plane.board_to_screen()` 换算 | **整段删除**：`EffectDrawer` 自己发 `draw_set_transform(size/2, 0, (size.x/50, ∓))`（`draw_transform`，逐位） |
| 寿命 | 自定秒数（`LIFETIME`） | `duration_ticks`（旧的秒数 ×60 **搬过来**，因为原版时长无证据）+ 调用方每帧 `retain_alive()`（判据**推断**，所以节点类故意没自己跑） |
| 名字 → 相位 | 四种名字各有一套自定形状 | **映射表**（`PHASE_FOR_KIND`）：`ExplosionEffect`→2 `CircleLargeA`、`DeathEffect`→6 `CircleDark`、`SparkEffect`→0 `CircleSmall`、`AshEffect`→4 `LineA` ——**我方选择**：原版是 `EffectInfo` 判别值（0..3 的跳表）进 `EffectData`，而 `Phase` 有 **7** 档，两者是不是同一个编号**没有任何证据** |
| 多边形 / 线段 | 会画 | 多边形顶点未复原 ⇒ 跳过（`polygon_skipped_total` 计数）；线段端点未复原 ⇒ 退化线段 |

⚠️ 附带一个**真的观测**：`EffectDrawer` 的 `step_effects` 会把相位 `CircleLargeA(2)` 推进成
`CircleLargeB(3)`（其余推进成 `CircleDark(6)`），所以"刚 spawn 时的相位"和"两帧后的相位"
**本来就不同** —— 自检里那条断言因此必须**当刻**读（§49.7 的踩坑记录）。

### 49.4 `TEAM_COLORS` / `assign_colors()` 搬到播放屏（数字一个字没动）

`GwPlaneView.TEAM_COLORS`（`docs/102` §1.1 从 `.so` 调色板表解出的 2×6 色表）与
`assign_colors()`（随机起点 + 线性探测）**唯一**的两个消费者都在 `play_screen.gd`
（开局配色、联机配色、回放的缺省色）。类删掉之后整块搬到播放屏，**六个数一个都没改**，
自检里 `TEAM_COLORS[0][2] == #7D6800` 那条断言也照旧成立（只是主语从 `GwPlaneView` 换成播放屏）。

### 49.5 缺口诚实清单（8 条，按"画面上看得见的影响"排序）

1. **准备圈：绘制补上了（§50），触发还没接**（最显眼的一条）。原版是 `EntityDrawer::draw`
   的 3 次 `draw_arc` + 3 个外凸箭头（`0x4b2890`，`docs/140` §2 逐条立即数）。
   §50 之后 `EntityDrawer` 的 `_draw` 就照那 6 条图元发（`client/tools/entity_drawer_check.gd`
   72 → **118** 条断言，含"关掉准备态就不画"），**但画面上仍然看不到**：
   `_draw` 只画 `set_ring()` 填进 `prep` 表的实体，而播放屏还没人调它 —— 原版填这张表的是
   `ready` 里订的 `pos_info_event`（`GraphwarBackend` 没接），而任务 33 的允许改动清单
   **不含播放屏逻辑** ⇒ 触发留到 §50.6 那三行改法。几何证据没丢：
   `tests/entity_drawer.rs` 的 `arcs_carry_the_exact_draw_arc_arguments`
   （10 折点 / 0.3 带宽 / 关 AA / `0x00687d`）与 `open_ring_is_narrower_than_closed_ring`
   （张开 60° / 闭合 123°）仍在 `cargo test` 里跑。
2. **士兵与靶子的贴图是纯色方块**（`EntityDrawer` 的程序生成贴图，§33 对照表"我方选择"）。
   真贴图要 `GraphwarTextureLoader`（三张 `HashMap<String, Texture2D>`）那一层，没接。
   靶子**还能看见**，但长得和士兵一样（`EntityDrawer` 没有"种类"概念，靠 id 区分：
   士兵 = `_soldiers` 下标，靶子 = `1000 + target_id`）。
3. **点的生产者仍是我方注入**（§43.6(d) 第 1 条没变）。
4. **tick 源仍是我方推**（`get_fractional_tick` 没接）：`_advance_curve()` 按帧间隔推
   `60 tick/s`，`advance_shot_for_test()` 是自检的确定性入口。
5. **`update_transform` 的两个实参**（`position=(500,300)` / `scale=(20,-20)`）仍是**我方选择**，
   与 `plane_on_screen.tscn` 逐值相同（§43.6(d) 第 3 条没变）。
6. **淡出没接**（`get_function_fade_on_tick` 无 x86 落点，走 GOT 槽 `0x9bbd08`）。
7. **队色表没接**（`GraphwarDrawUtil::get_team_overview`，`0x4be6e0` 的两张表）。
   于是 `EntityDrawer.team_color` 用默认值（`0x0000ff`，镜像判据 = 不翻转），
   曲线颜色仍由播放屏把玩家的 `Color` 打包成 `0x00RRGGBB` 传进去。
8. **三处注释还写着旧类名**（`client/src/core/core_bridge.gd` 第 59 行、`client/src/game/target_set.gd`
   第 75 行、`client/src/play/replay_player.gd` 第 364 行都提到 `GwPlaneView.xxx`）。那三个文件
   不在本任务的允许改动清单里 ⇒ 没动，下一轮顺手改。

### 49.6 就地把 §43.6 那张表的过期行改掉（改了什么）

| 位置 | 原文（过期） | 现在 |
|---|---|---|
| §43.6 抬头 | "**本轮没动它**——本任务只证明真类能在场景里画出来" | 加了一句"本节已被 §48 执行完"，正文保留（记录当时缺什么） |
| §43.6 (a) 表 | 两行只有"换成什么"，没有现状 | 加"现状"列：**两行都已删除**；`fire_fx` 那行注明**准备圈没有替身** |
| §43.6 (b) 表头 | "（现在 1 个都没有）" | "（写本节时 1 个都没有；**§48 之后 6 个全挂上了**）"，并加"现状"列 |
| §43.6 (b) 第 6 行 | "**类还不存在**（§40.5 / §42.5 第 7 条）" | 就地注明**写的时候已经过期**（第四轮任务 23 做出了该类，§45 有 116 条断言），现状"已挂" |
| §43.6 (c) | 三件"要改的" | 标"三件都改完了" |
| §43.6 (d) 7 条 | 纯缺口清单 | 每条后面加"→ §48 之后是什么状态"：第 7 条的**网格**已经由 `AxisDrawer` 画出来（白底 + 线），只有**队色表**仍缺 |
| §43.7 第 1 条 | "客户端播放屏仍是仿制品" | 见下方 §49.6b 的就地更正 |

**§49.6b（就地更正 §43.7）**：§43.7 第 1 条写"客户端播放屏仍是仿制品、§42.5 第 5 条那句
'客户端还没用真类'对播放屏仍然成立" —— **从 §48 起这句话对六个绘制位置不再成立**
（棋盘框/曲线/士兵/网格/障碍/特效都来自 Rust 类），**但整屏仍是仿制品**（
`play_screen.gd` 的流程/时序/输入/回合仍是我们写的，`AGENTS.md` §0 第 4 条的判断不变）。

### 49.7 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target31
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
cargo build --offline --example serve_match                             # 自检的两个联机用例要它
export HOME=<假 HOME>                      # selftest 读 $HOME，不走 XDG_DATA_HOME
mkdir -p $HOME/.local/share/godot/app_userdata/Graphwar
cp -r <replays 快照>/replays $HOME/.local/share/godot/app_userdata/Graphwar/
"$GODOT" --headless --path client --import                              # 删了 .gd 必须重扫
"$GODOT" --headless --path client -- --selftest
```

| 口径 | 改前（`CG@1877049`，同一台机同一份 .so） | 改后 |
|---|---|---|
| selftest（**有** assets + 回放样本 + `serve_match`） | **694 passed / 0 failed** | **709 passed / 0 failed**（+15） |
| selftest（**无** assets：`GW2_ASSET_DIR=res://no_assets`） | 675 passed / **2 failed** / 10 skipped | 690 passed / **2 failed** / 10 skipped |
| 两条 FAIL 的身份 | `点 Skins 页的 -1 号皮肤…`、`Cancel = 194 宽 / Start = 218 宽` | **同两条**（既有问题，不是本批引入） |
| `cargo test --offline` | 1569 passed / 0 failed（1 ignored） | **1569 / 0**（`src/` 一个字没动） |

`client/tools/` 那九个 headless 脚本（改后实测，一个都没退）：

```text
gdext_check         24 passed, 0 failed
assemble_check     203 passed, 0 failed
plane_on_screen     96 passed, 0 failed
obstacle_drawer    116 passed, 0 failed
function_drawer    132 passed, 0 failed
draw_classes       235 passed, 0 failed
curve_on_screen     96 passed, 0 failed
entity_drawer       72 passed, 0 failed
base_classes       117 passed, 0 failed
signal_classes     155 passed, 0 failed
spike_class_check   23 passed, 0 failed
```

> ⚠️ **审计 2026-11-08**：本清单原记 `plane_on_screen 76`，与同一份文档 §48.5 的
> 「`plane_on_screen_check.gd` 96 / 0（基线 76）」矛盾；按 §48.5（+`plane.size` 反推变换断言那一批）
> 改成 **96**。⚠️ 这一格是**转引**（§48.5 + 现行 master 实测值），**本次审计没有复跑 headless**
> （需要 Godot + 已编 `.so` + `--import`），所以标 **观测（转引）** 而不是本机复测。

**"6 个节点都挂上了"在自检里怎么验的**（`selftest.gd` 的 `_test_fire_fx` ② 那一段）：
`ClassDB.class_exists` ×6 + `plane_state()` 六个字段全部**从真节点读回来**
（`function_drawers`=3、`axis_lines`=102、`plane_draws`>0、`entities`=士兵+靶子、
`obstacle_updates`>0、`fx_draws`>0）+ 四个网格档位 0/2/20/102 + 一个实体 4 个子节点 +
`Line2D` 线宽 `0.125`（逐位常量）+ **三个"引擎真回调过"的计数**
（`AxisDrawer.draw_call_count` / `EntityDrawer.process_call_count` / `EffectDrawer.draw_call_count`）
+ **一次真注入**（注入 1 个障碍 ⇒ 掩膜里 11310 个像素被占）
+ **画布真的被容器铺开**（headless 下实测 `canvas = (124.0, 74.4)`、`plane.scale = (0.124, 0.124)`，
  两个数正好是 5:3 与 `124/1000`）。
**一条都不是"看代码猜的"**：每条都是引擎回调的计数或真节点里的真值。

**两个本轮踩到的坑（写下来省下一轮的时间）**：

1. `GraphwarPlane.clear_function_points()` **不删 `Line2D`**（它只清平面自己那张登记表）。
   只调它的后果不是"看着没删"，而是**下一条曲线一个点都画不出来**：同一 player id 再
   `update_function` 时 `FunctionDrawer` 还记着"已写入 N 点"，`append_window` 判
   `new_len <= N` 直接 return（`0x498d5a` 的 `ja`）。必须再对那个 `FunctionDrawer`
   调 `remove_function`（原版 `0x498b70`：`queue_free` + 删表）。
   平面的 `_draw` 里那条 `remove_function`（`0x4b6418`）只在"函数离开活跃窗口"时跑。
2. `EffectDrawer` 的相位会自己往前走（2 → 3、其余 → 6，`step_effects` 的"到期换相位"），
   所以"spawn 时的相位"必须**当刻**读；隔两帧读到 3 是**对的**，不是 bug。
3. `ObstacleDrawer.add_obstacle(x, y, radius)` 收的是**棋盘单位**，不是画布像素 ——
   `plane::raster_params()`（`0x3f17a0` 那条路径的入口）自己就做那次换算
   （`px = x*20 + 500`、`r = size*20`）。本轮第一版在播放屏里**又换了一次**，
   圆心被推到 (10500, 6300) ⇒ 一个像素都画不出来（自检条数上表现为 1 条 FAIL，
   肉眼看是"障碍整个消失"）。

### 49.8 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么 |
|---|---|---|
| "六个类都**在画**" | **观测** | 引擎回调计数（`draw_call_count` / `drawn_*_total` / `occupied_pixels` / `plane_draws`），本轮实测非零 |
| 六个类**画得与原版一致** | **未知** | 没有本轮的截图对拍：① 准备圈缺失（§49.5 第 1 条）；② 士兵贴图是纯色方块；③ 画布模型（1000×600 + 缩放）是**我方选择**，原版那次 `draw_set_transform` 的实参没解；④ 队色表没接 |
| 准备圈的几何 | **观测** | `tests/entity_drawer.rs` 的两条测试；**§50 之后**这 6 条图元已经由 `_draw` 真的发到引擎（`entity_drawer_check.gd` 118 条断言），但**画面上仍然没有**（触发没接，§50.0 第 1 条） |
| 名字 → 相位那张表 | **我方选择** | 原版 `EffectInfo` 判别值 → `EffectData.phase` 的映射没有任何证据 |
| 特效时长 | **我方选择** | 原版只给 `start_tick`；`retain` 的判据是**推断** |
| 障碍坐标换算 | **观测** | 掩膜是 1000×600 画布像素（`plane::CANVAS_W/H`），棋盘单位 → 像素走 `px = 20x + 500`、`py = 300 − 20y`、`r_px = 20r`（`to_pixel` 的同一套数） |
| `--headless` 下的画面对不对 | **测不到** | 视口 64×64、没有渲染器；像素级只能靠有限窗口截图（`docs/16` §43.5 的办法），本轮**没做**截图 |

## 50. 第二批 6 个 manager 接上容器 + 事件流 + 最小交互（2026-11，任务 40）

**一句话**：§44 让 5 个 manager 从"有键"到"能交互"，§47 把档位词表定成四档。本节把
**另外 6 个**（`turn` / `player` / `team` / `mode` / `entity_effect` / `effect`）按同一口径接上：
**容器 + 事件流（订阅/发两个方向都指到指令）+ 一次最小交互**，档位从 `assembled` 抬到 **`wired`**。
`assemble_check.gd` 从 **203 条扩到 213 条**，新增的 10 条里有 38 个"交互后读回"的值
（与 `root_wiring.rs` 的单测**逐字相同**）。

> 档位口径同 §42 / §44 / §47：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。`cargo test` / `assemble_check.gd` 的条数
> **只是回归保护**，不是复原证据。

### 50.0 先说"没做到"的部分（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| `GraphwarGameStateManager`（296 B）的**事件层** | **没接**：容器里装的是 `game::Match`，它的字段全在 `src/game.rs`，本轮该文件是**禁区**（有人正在 `execute_results` 上干活）⇒ 这一槽**留在 `assembled`** | 缺：`Match` 的 7 条订阅 + 发方向落进 `src/game.rs` |
| `Match` 的 7 条订阅（`new_connection_event` / `entity_removed_event` / `tick_event` / `connection_removed_event` / `function_points_event` / `skin_graphics_info_event` / `chat_message_event`） | 名字**已有证据**（`docs/14` §11.7 的 `.rodata` 逐条 `lea`，`0x477bc0`，`fixtures/name_to_addr.tsv` 里 7 行 `subscribe_to_events_event_handler`），但**代码没接** | 订阅名 `观测`，接线 `未知`（禁区） |
| `turn_manager::send_all_info_events` 的**载荷** | **我方形状**：`0x462fe0` 能确证"加锁遍历 + 一条 `add_event`"，载荷构造没解（它**不**调 `get_current_turn_players`/`get_current_turn_soldiers`） | **推断**（见 §50.7） |
| 6 个 manager 各自的**原版字段布局** | 只有 `ModeManager` 这一轮新解到 32 B（`+0x00/0x08/0x10` 三个借用字段 + `+0x18` 两个模式字节）；其余 5 个**仍是我方形状** | 结构轴 `未知`/`部分观测`（**不**因升到 `wired` 而变成"布局已复原"，见 §47.1 读法 1） |
| 每条订阅**闭包函数体**的逐指令语义 | 只有 `entity_effect`（§46 已有的 `:67:30` / `:77:18`）与 `player`（§3/§4 的 `逐位`）跳到"处理器 = `send_all_info_events` / `handle_entity_removed_event`"这一层 | 处理器**绑定** `观测`；闭包体内**逐条**语义多数 `未知` |

### 50.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_turn_manager.rs` | +接线层 | `SUBSCRIBED_EVENTS`（2 条）+ `subscribed` / `outbox` + `begin_turn` 发 `TurnInfo`(43) + `handle_entity_removed_event` + `send_all_info_events` / `handle_new_connection_event` + `take_outbox` / `outbox_len` + 5 条单测（轮转/判决/分队算术**一行没动**） |
| `src/graphwar_player_manager.rs` | +接线层 | `subscribed` / `outbox` + `add_player`/级联各发一条 + `handle_new_connection_event` + `take_outbox` / `outbox_len` + 3 条单测（`SUBSCRIPTIONS` 常量原来就有，**这次真的用上了**） |
| `src/graphwar_team_manager.rs` | +接线层 | `subscribed` / `outbox`（`Vec<TeamEventKind>`，理由见 §50.7 第 1 条）+ `handle_new_connection_event` + `take_outbox` / `outbox_len` + 3 条单测（`pick_team_from` 的算术**一行没动**） |
| `src/graphwar_mode_manager.rs` | +接线层 | `SUBSCRIBED_EVENTS`（1 条）+ `subscribed` / `outbox` + 三个 setter 各发一条 + `handle_new_connection_event` + `ModeInfoEvent::event()` + `take_outbox` / `outbox_len` + 3 条单测；`Copy` 派生**去掉**（`Vec` 不是 `Copy`），`Default` 改成手工（否则 `default()` 会静默丢订阅） |
| `src/graphwar_entity_effect_manager.rs` | +接线口 | `handle_new_connection_event`（= `send_all_info_events` 的同义入口）+ `take_outbox` / `outbox_len` + 3 条单测（`outbox` / `subscribed` / 两个处理器原来就有） |
| `src/graphwar_effect_manager.rs` | +接线口 | `handle_tick_event(tag, now)`（判别值闸 0）+ `take_outbox` / `outbox_len` + `Default` 改手工 + 2 条单测（`tick` / 寿命 3600 **一行没动**） |
| `src/_scaffold/root_wiring.rs` | 改 | 2 个新取回口（`effect_manager` / `entity_effect_manager`）+ `interaction_selftest` 追加 38 条 + `Wired::status()` 6 个槽 → `wired` + 四档计数 4/2/15/1 → **10/2/9/1** + 3 条新单测 + 文档表同步 |
| `client/tools/assemble_check.gd` | +10 条 | `WIRED_TIER` 6 个槽 + `TIER_TALLY` + `expected_probe` 27 → **65** 条 + 10 条单值/事件流断言 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑（**派生**） | `src_fns` **2504 → 2553**、`src_tree_sha256` 跟着变；**527/527 行与每模块数字一行未变** |
| `docs/16-client.md` | +本节 | — |

❌ `src/game.rs` / `src/lib.rs` / `client/src/**` / 除这 6 个之外的 manager / 任何既有断言 **一行没动**。
既有 27 条交互读数**一个字都没改**（新 38 条是**追加**在末尾），所以 §44 的验收串仍然原样全绿。

### 50.2 6 个 manager：订阅什么、发什么（全部 `观测`，x86 2026-08-28）

复算工具：`../.venv/bin/python tools/addr_probe.py dis <vaddr>`（本机 capstone，离线）+ `callers <vaddr>`。
每条订阅的判据是**三件事同时成立**：`new` 里 `lea rsi,'<名字>'`、`mov edx,<len>`
（= `strlen`）、`call [slot 0x9bac88] -> 0x3d38f0`（`GraphwarEventHandler::subscribe_to_event`）；
处理器判据是"`lea r8` 那个槽 `+0x20` 指向的跳板 → 哪个函数"。

| manager | 原版 `new` | 订阅（按调用序，名字 / len / 调用点） | 处理器 | 发出的事件（tag） | 交互条数 | 档位 before → after |
|---|---|---|---|---|---|---|
| `graphwar_turn_manager` | `0x463790` | `new_connection_event` /`0x14`/ `0x463815`；`entity_removed_event` /`0x14`/ `0x463866` | `0x462fe0` / `0x463580` | `TurnInfo`(**43**)：`next_turn` `0x46406b`–`0x464072`（活射手）/ `0x464016`–`0x464021`（`{p,0}`） | **6** | `assembled` → **`wired`** |
| `graphwar_player_manager` | `0x455410` | `new_connection_event` /`0x14`/ `0x455495`；`entity_removed_event` /`0x14`/ `0x4554e6` | `0x455160` / `0x455250` | `PlayerInfo`(**13**)（`add_player` / `send_all_info_events`）、`ConnectionRemoved`(**6**)（级联）——与模块文档 §3/§4 的 `逐位` 结论一致 | **6** | `assembled` → **`wired`** |
| `graphwar_team_manager` | `0x3c1fc0` | `new_connection_event` /`0x14`/ `0x3c2045`；`entity_removed_event` /`0x14`/ `0x3c2096` | `0x3c1cf0` / `0x3c1f40` | `TeamInfo`(**24**)（`add_to_team` / `send_all_info_events`）、`RemoveTeam`(**25**)（**删到了才发**，`0x3c1a08` 的 `cmp dl,2`） | **7** | `assembled` → **`wired`** |
| `graphwar_mode_manager` | `0x5186c0` | **只一条** `new_connection_event` /`0x14`/ `0x51876c` | `0x5185a0`（跳板 `0x5186a0`） | `AxisModeInfo`(**45**) / `FunctionModeInfo`(**46**) / `TimeModeInfo`(**48**)（三个 setter 各一条 + `send_all_info_events` 三条） | **5** | `assembled` → **`wired`** |
| `graphwar_entity_effect_manager` | `0x478490` | `new_connection_event` /`0x14`/ `0x478515`；`entity_removed_event` /`0x14`/ `0x478566` | `0x4782f0` / `0x4783f0`（§46 已有） | `EntityEffectInfo`(**56**)（`add_entity_effect` / 整表重播；`handle_entity_removed_event` **不发**） | **7** | `assembled` → **`wired`** |
| `graphwar_effect_manager` | `0x519ba0` | **只一条** `tick_event` /`0xa`/ `0x519ca5` | 体内（无独立跳板） | `EffectInfo`(**44**)（`add_effect`）；`tick` **不发**（模块文档 §4） | **7** | `assembled` → **`wired`** |

上表的地址串（可直接复跑，逐条与 `addr_probe.py` 的输出对得上）：

```text
# TurnManager::new 0x463790 的两条 subscribe（名字 / len / 调用点）
0x4637fc  lea rsi @0x1c18ad 'new_connection_event…'   0x46380a mov edx,0x14   0x463815 call subscribe
0x46384d  lea rsi @0x1c18c1 'entity_removed_event…'   0x46385b mov edx,0x14   0x463866 call subscribe
          r8 = 0x957be8 → +0x20 = 0x957c08 = 跳板 0x463750 → 0x462fe0 send_all_info_events
          r8 = 0x957c10 → +0x20 = 0x957c30 = 跳板 0x463770 → 0x463580 handle_entity_removed_event
# PlayerManager::new 0x455410
0x45547c  0x1c0eef 'new_connection_event…'   0x45548a edx,0x14   0x455495 call   → 跳板 0x4553d0 → 0x455160
0x4554cd  0x1c0e12 'entity_removed_event…'   0x4554db edx,0x14   0x4554e6 call   → 跳板 0x4553f0 → 0x455250
# TeamManager::new 0x3c1fc0
0x3c202c  0x1be298 'new_connection_event…'   0x3c203a edx,0x14   0x3c2045 call   → 跳板 0x3c1fa0 → 0x3c1cf0
0x3c207d  0x1be2ac 'entity_removed_event…'   0x3c208b edx,0x14   0x3c2096 call   → 跳板 0x3c1f80 → 0x3c1f40
# ModeManager::new 0x5186c0
0x518753  0x1c63fb 'new_connection_event…'   0x518761 edx,0x14   0x51876c call   → 跳板 0x5186a0 → 0x5185a0
# EntityEffectManager::new 0x478490
0x4784fc  0x1c18ad 'new_connection_event…'   0x47850a edx,0x14   0x478515 call   → 闭包 0x4782f0
0x47854d  0x1c18c1 'entity_removed_event…'   0x47855b edx,0x14   0x478566 call   → 闭包 0x4783f0
# EffectManager::new 0x519ba0（**只一条**）
0x519c8c  0x1c6513 'tick_event…'             0x519c9a edx,0xa    0x519ca5 call
```

两条**新事实**（相对 §41 / §47 的台账）：

1. **`ModeManager` 只有 1 条订阅、而且是 `new_connection_event`**，不是四个
   `*_mode_change_request`。也就是说这个模块"收到新连接就把三个模式重播一遍"，
   **不**订阅模式变更——与模块文档那句"权威在服务器"是同一件事的两面：
   写入走 `GraphwarRemoteEventHandler::handle_server_remote_event`，订阅只服务"补发"。
2. **`TurnManager` 的 `entity_removed_event` 删的是整条玩家记录，不发任何事件**。
   `0x463580` 体内没有 `add_event`（那槽 `0x9bac80` 在这 960 字节里**一次都没出现**），
   删到的值按 **4 字节元素**的 `Vec` 释放（`0x46362e shl rsi,2` / `0x463632 mov edx,4`）
   ⇒ 值里那个 `Vec<soldier_id>` 跟着整条记录一起没了，**不是**"从某个 Vec 里摘掉一个士兵"。

### 50.3 落码：容器里现在装的是什么

```text
TurnManager                 # graphwar_turn_manager.rs（原类型 + 接线层）
GraphwarPlayerManager       # graphwar_player_manager.rs（原类型 + 接线层）
TeamTable                   # graphwar_team_manager.rs（原类型 + 接线层）
GraphwarModeManager         # graphwar_mode_manager.rs（原类型 + 接线层；32 B 布局本轮新解）
GraphwarEntityEffectManager # graphwar_entity_effect_manager.rs（原类型；outbox/subscribed 原来就有）
GraphwarEffectManager       # graphwar_effect_manager.rs（原类型；outbox/subscribed 原来就有）
```

新增的取回口（都走 `get_module::<M>()`，不是查表）：`effect_manager()` / `entity_effect_manager()`；
另外 4 个的取回口**§41 就有**（`turn_manager()` / `player_manager()` / `team_manager()` / `mode_manager()`），
本轮只是把它们**真用起来**（此前只有一条"取回来读一个字段"的断言）。

`ModeManager` 的 32 字节布局（`观测`，`0x5186ff`–`0x51870b`，管理体基址 = `rax+0x18`）：

```text
+0x00 弱自句柄（Rc 盒基址）      +0x08 事件队列 Rc        +0x10 容器 Weak
+0x18 word 0x0301（两个模式字节 0x01 / 0x03）    +0x1a 0（第三字节）
合计 0x1b = 27 → 补齐 32，与 §41.2 的 vtable size **一字不差**
```

⚠️ 本层只保留"三个模式"的语义，`+0x00 / +0x08 / +0x10` 那三个借用字段**不复原**（我方形状）。

### 50.4 为什么 `GameStateManager` 这一轮没升档（**禁区**）

`Wired::GameStateManager` 在容器里装的是 **`game::Match`**（`src/game.rs`），而任务书 §5 的 ❌
明确写着"不许改 `src/game.rs`"（有人正在 `execute_results` 上干活）。核对结果：

```text
$ grep -n "outbox\|subscribed" src/game.rs      →  零命中
```

`Match` 既没有订阅表也没有发件箱，往它身上加"读 7 条订阅 / 写发件箱"**必须改 `src/game.rs`**。
两条退路都被否定：

* **在 `root_wiring` 里加一层壳**（`GameStateSlot { layout: Match, table: … }`）——会改掉容器里的类型，
  `container_game_state_soldiers()` / `assemble_check.gd` 的既有断言全部要跟着改（禁区：既有断言）；
* **另起一张旁表**——那不是"这一槽接了事件流"，只是把假数据换个地方放，属于"把猜测写成行为"。

⇒ `GameStateManager` **明确留在 `assembled`**，缺的东西写在 §50.0 第 1–2 行。
替补进本轮名单的是 **`graphwar_effect_manager`**（同样 72 B、同样 `assembled`、订阅 1 条 `tick_event`
有 `0x519ca5` 的指令证据、发 `EffectInfo`(44) 有模块文档 §3 的 `逐位` 结论）。

### 50.5 headless 那侧：38 条新交互读数

`manager_probe_selftest()`（§44.5 的**我方自造**观测口）现在返回 **65 条**
（`27 + 38`），既有 27 条的**字符串一个字都没变**（新 38 条追加在末尾）：

```text
turn.soldier=9023  turn.flip=false  turn.emitted=1  turn.subs=2  turn.removed=true  turn.present_after=false
player.conn=9025  player.emitted=1  player.subs=2  player.replayed=3  player.cascade=true  player.outbox=5
team.added=Team2  team.counts=1,2  team.pick=Team1  team.subs=2  team.replayed=3  team.removed=1  team.outbox=5
mode.axis=2  mode.replayed=3  mode.restored=true  mode.outbox=5  mode.subs=1
entity_effect.kind=1  entity_effect.emitted=1  entity_effect.subs=2  entity_effect.replayed=1
entity_effect.removed=true  entity_effect.after=true  entity_effect.outbox=2
effect.kind=0  effect.emitted=1  effect.subs=1  effect.due_early=0  effect.due_at=1  effect.gone=true  effect.outbox=1
```

三条设计约束（都写进了代码注释）：

1. **幂等**：固定 id（`9021` / `9024` / `9026` / `9027` / `9028`）+ 末尾把临时记录删掉 / 复位
   ⇒ 连调两次返回**同一串**（`assemble_check.gd` 就是这么比的）。
   `TurnManager` 那条特别处理：`begin_turn` 会**推进游标**，所以末尾用
   `handle_entity_removed_event` 把那条玩家记录删掉，第二次调用仍从游标 0 开始 ⇒ 读数相同。
2. **不碰别人的表**：`Match` / `LifeState` / `TargetManager` / `ObstacleManager` / `FunctionTable` / `TickHandler`
   一个字都没动；新 6 个用的都是**临时 id**（`9021`–`9028`），调度完就删。
3. **值是真算的**：`root_wiring.rs` 的 `the_new_read_back_values_track_the_written_state`
   换输入（换射手名单 / 换模式 / 换 `start_tick`），读回值跟着变 ⇒ 不是 `return` 常量。

### 50.6 验收（本节实测）

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/mgr2/target
cargo test --offline
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd
```

| 命令 | 本节实测 | 基线（master `c798eccf`） |
|---|---|---|
| `cargo test --offline` | **1620**，全绿 | **1585**（本节 **+35**）。构成：新增单测 **19** 条（`turn` 5 / `team` 3 / `player` 3 / `mode` 3 / `effect` 2 / `entity_effect` 3）+ 它们在 `#[path]` 集成测试里被**重复编入**的 16 条（`cargo test` 按目标分别计数：`effect` 3 份、`mode` 2 份、`team`/`player`/`entity_effect` 各 2 份）。另 3 条 `root_wiring` 的只在 `--features gdext-bindings` 下跑，**不计入**这 35 |
| `assemble_check.gd` | **213 passed / 0 failed**，退出码 0 | **203**（本节净 +10） |
| `plane_on_screen_check.gd` | **96 passed** | 96（未动） |
| `obstacle_drawer_check.gd` | **116 passed** | 116（未动） |
| `function_drawer_check.gd` | **132 passed** | 132（未动） |
| `entity_drawer_check.gd` | **72 passed** | 72（未动） |
| `python3 tools/coverage_matrix.py --check` | 需重跑（`src_fns` 变）→ 已重跑：`src_fns` **2504 → 2553**，527 行未变 | OK |

**三条驱动轴（分开跑、分开报，`AGENTS.md` 合并纪律 C；快照 = `.agent-work/replay-snapshot`，
401 个文件 / 286 个可读 / 1080 个结算批）**：

| 轴 | 命令 | 本节实测 | `fixtures/guardrails.json` | 判 |
|---|---|---|---|---|
| `fire` | `replay_attrib --drive fire --policy default` | 批次 1080；内容 **647**；种类序列 **1075**；死亡 **366/385**；挖洞 **207/294** | 1080 / 647 / 1075 / 366 / 385 / 207 / 294 | **7/7 一致** |
| `batch` | `replay_attrib --drive batch --policy default` | 批次 1080；内容 **647**；种类序列 **1075**；死亡 **366/385**；挖洞 **207/294** | 同上 | **7/7 一致** |
| `batch_conv` | `batch_conv --policy strict` | 本节**没跑**（本轮零几何改动；上一轮 §49 的记录是 1074/595） | 633 / 1075 | 未复核 |

⇒ 本节只动"接线层"（订阅表 / 发件箱 / 取回口 / 断言），**一条几何、弹道、结算代码都没碰**，
两条轴逐数字不动，符合"接线层不该动几何"的预期。

### 50.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `TurnManager::send_all_info_events`（`0x462fe0`）的**载荷** | **推断**（明确标"我方形状"） | 该函数里只有一条 `add_event`（`0x46306a`）可确证；载荷构造没解，而且它**不**调用 `get_current_turn_players`(`0x463340`) / `get_current_turn_soldiers`(`0x463460`)（`addr_probe.py callers` 两处都没有 `0x462fe0` 的调用点）⇒ 本层按"每人一条 `TurnInfo{player, 当前射手}`"发，**别当证据用** |
| `TeamManager` 的发件箱用 `Vec<TeamEventKind>`（`u16` 判别值）而不是 `Vec<Event>` | **我方选择**（有硬约束） | `tests/graphwar_team_manager.rs` 用 `#[path]` **单独**把本文件编进测试二进制（不带 `src/events.rs`），它的文件头约定"只依赖 `std`"。为一行 `use` 去改那个文件不值得；判别值是同一个数（24 / 25）。另外 5 个的测试带 `events.rs`，所以它们用 `Vec<Event>` |
| `TurnManager::handle_entity_removed_event` 的 key 空间 | **推断** | 二进制把 `event+8` 直接当本表 key 用（`0x4635a5`），"实体 id = player_id"这条与 `graphwar_player_manager` §5 的同一个推断同源 |
| 6 个 manager 各自订阅**闭包体内**的逐条语义 | 部分 `观测` | `entity_effect`（§46 已有 `:67:30` / `:77:18`）与 `player`（§3/§4 `逐位`）到了"处理器 = 谁"这一层；`turn` / `team` / `mode` 只到"跳板 → 哪个函数"，闭包体内的边界检查/载荷拼接没逐条读 |
| `ModeManager` 的 `+0x18` 两个模式字节 | **部分观测** | `word 0x0301` 是**初值**（`0x51870b`），两个字节与 `AxisMode`/`FunctionMode`/`TimeMode` 三个枚举谁对谁**没逐条核**（本层按语义放三个字段，不按字节摆） |
| `GraphwarGameStateManager` 的 7 条订阅落码 | **未知**（禁区） | 见 §50.4：`src/game.rs` 不许改 |
| `batch_conv` 轴 | **未复核** | 本轮没跑；本轮零几何改动，理由与 §49 相同 |

### 50.8 22 个槽的档位（任务 40 之后）

| 档位 | 槽 | 依据 |
|---|---|---|
| `wired`（**10**） | `TickHandler` / `TargetManager` / `LifeManager` / `ObstacleManager`（§44.2）+ `TurnManager` / `PlayerManager` / `TeamManager` / `ModeManager` / `EntityEffectManager` / `EffectManager`（§50.2） | 订阅/发事件两个方向都指到指令 + `interaction_selftest` 各自的 4/5/5/7 与 6/6/7/5/7/7 条 |
| `layout`（2） | `ClientConnectionHandler`（§47.2）/ `FunctionManager`（§46.2） | 布局逐字段解出，行为没做完 |
| `assembled`（**9**） | `LocalPlayerManager` / `SoldierManager` / **`GameStateManager`（§50.4）** / `GraphicsManager` / `ComputerPlayerManager` / `FunctionHistoryRecorder` / `NameManager` / `ColorManager` / `RankManager` | 容器里是我方复原类型，但没接事件流 / 没有最小交互断言 |
| `stub`（1） | `PlanePosManager` | 24 B 布局未知 |

`SlotKind`（容器类型来源）仍是 **17 + 5 = 22**，一行没变。
`interaction_selftest` 覆盖 **11 / 22** 个槽（§44 的 5 + 本节的 6）。

## 51. `EntityDrawer::draw` 补上了：准备圈的 3 段弧 + 3 个外凸箭头真的发到了引擎（2026-11，任务 33）

**一句话**：`src/_scaffold/entity_drawer_class.rs` 的 `_draw` 从"一个 `draw_*` 都不调"变成
"**每具准备态士兵发 6 条图元**"——3 次 `draw_arc`（三段弧）+ 3 次 `draw_polygon`（外凸箭头），
实参逐条对 `docs/archive/140-plane-visual.md` §2 的立即数；验收脚本
`client/tools/entity_drawer_check.gd` **72 → 118** 条断言。
**但"用户能在屏幕上看见准备圈"仍然没做到**——卡的是**触发**，不是绘制，见 §50.0 第 1 条。

### 51.0 先说没做到的（免得被下面那张表误导）

| # | 项 | 现状 | 档位 |
|---|---|---|---|
| 1 | **画面上能看见准备圈** | **不能**。`_draw` 只画 `prep` 表里的实体（= 调过 `set_ring()` 的），而**播放屏一处都没调它**：原版填这张表的是 `ready` 里订的 `pos_info_event`（`GraphwarBackend` 没接），任务 33 的允许改动清单**不含播放屏逻辑** ⇒ 触发留到 §50.6 那三行改法 | **缺口**（绘制侧是观测） |
| 2 | `PosInfo` 的**单位** | `docs/140` §2.2 只读到"来自士兵坐标"，`[rsp+0xb0]/[rsp+0xa8]` 是不是已是画布像素**没证**。本类按**棋盘坐标**处理：复用 `sprite_transform` 那条投影（`k = size.x/50`）+ 半径乘同一个 `k` | **推断**（差一条证据；换 `k = 1.0` 就改完） |
| 3 | 环与箭头的**颜色来源** | 原版从哈希表取打包 RGB（`0x4b347a` 的三字节）；本类用导出属性 `team_color` | **我方接线**（队色表没接，§49.5 第 7 条） |
| 4 | 弧与箭头**是否互斥** | `entity_drawer::entity_primitives` 两处分支条件（`0x4b32e2 test %r14b` / `0x4b34c3 test %bl`）的哈希表语义**没解**⇒ 本类按"同一具士兵两组都画"发 | **未知**（`tests/entity_drawer.rs` 早就这么记着） |
| 5 | 驱动量 `V` 是什么时钟 | `0x4b2e10 call 0x519530`（`get_fractional_tick`）的返回值；本类由调用方给 | **观测**（算式）/ **我方接线**（谁每帧喂它） |
| 6 | `z_index` | 环画在 `EntityDrawer` **自己的** canvas item 上、三个 `Sprite2D` 是它的子节点 ⇒ 环在士兵**下面**（与原版同一棵树、同一种关系）；`z_index` 初值仍**未知**（§43.6(d) 第 6 条） | 结构=**观测**；`z_index`=**未知** |

### 51.1 本轮动了哪些文件

| 文件 | 改了什么 |
|---|---|
| `src/_scaffold/entity_drawer_class.rs` | `IControl::draw` 实现（6 条图元）；`prep` 表 + `PrepRing`；`set_ring` / `set_ring_base` / `clear_ring_base` / `clear_ring` / `ring_count`；观测口 `draw_call_count` / `drawn_arc_total` / `drawn_polygon_total` / `last_arc_count` / `last_arc` / `last_polygon`；纯函数 `ring_center_px` / `ring_canvas_scale`；`process` 末尾 `queue_redraw()`；`remove_entity` 顺手清 `prep`；**3 条新单测** |
| `client/tools/entity_drawer_check.gd` | 新增第 13 节（准备圈）**46 条**断言 + `wanted` 方法表 +11 条（72 → 118） |
| `client/src/screens/play_screen.gd` | **只改那段注释**："准备圈丢了"→"几何已复原、触发还没接（附最小改法）" |
| `client/tools/selftest.gd` | 同上，对应说明就地更新（**断言条数不变**） |
| `docs/16-client.md` | 本节 + §49.0 / §49.5 第 1 条 / §49.8"准备圈的几何"那一行就地更正 |
| `docs/coverage-matrix.tsv` + `docs/149-coverage-matrix.md` | 生成物（`src/` 变了必须重跑）：`src_fns` 2504 → **2522**，树哈希随之变 |

**没动的**：`src/entity_drawer.rs` 的算术（一个字没改）、`src/game.rs`、其它 `*_class.rs`、
`client/src/**` 的其它逻辑、`dispatch/`、`fixtures/guardrail*.json`。

### 51.2 为什么是这 6 条图元（地址链沿用 `docs/140` 的观测，本轮没重新扒）

| 原语 | 原版调用点 | 包装 / GOT 槽 | 本类 |
|---|---|---|---|
| `draw_arc` ×3 | `0x4b366c` / `0x4b36fd` / `0x4b378a`（全在 `EntityDrawer::draw` = `0x4b2890` 内） | 包装 `0x8abca0` ← GOT 槽 **`0x9bc020`**（`R_X86_64_RELATIVE` 写入） | `base.draw_arc_ex(...).width(0.3).antialiased(false).done()` |
| `draw_polygon` ×3 | `0x4b30b7` / `0x4b30e8` / `0x4b3119`（三次 `call 0x4b9a70` = `draw_offset_triangle`） | `0x4b9a70` 末尾 `lea "draw_polygon"(0x1c2bad)` + `call 0x3a01a0`（gdext 通用 `call` 通道） | `base.draw_polygon(&points, &colors)`（3 个顶点 / 3 份同色） |

`Primitive` 只有 `Arc` 与 `Polygon` 两个变体 ⇒ **`draw_polyline` / `draw_colored_polygon` 一条都用不上**
（原版也没在这条路径上用它们：`"draw_polyline"` 这个字符串在整份 `.so` 里**不存在**，`docs/140` §4.5）。

顺序取 [`entity_drawer::entity_primitives`] 的返回值 = **箭头在前、弧在后**（代码地址序）。

### 51.3 `_draw` 里只有两步是"接线"，算术一个字没动

```text
                        ┌─ entity_drawer::entity_primitives(EntityRingInput) ─┐   ← 已复原的算术
A_i = V/120 + {0, 2π/3, 4π/3}                                               │
H   = min(π/6 + (V−map[i])/60, 1.07337748998)                               │
r   = f32(pos_radius) × 1.5f32                                              │
P_k = C + {r, r+0.5, r+0.5}·(cos, sin)(θ ∓ 6°)                              │
                        └────────────────────────────────────────────────────┘
                                   ↓  本文件只加两步
① 坐标：center_px = ring_center_px(size, packed, board)   ← 直接复用 sprite_transform 的 position
        r_px      = r × ring_canvas_scale(size)           （k = size.x / 50）
② 颜色：Color::from_packed_rgb(team_color)
```

* **角度不做镜像**（`team_mirror` 的符号只进投影的 `x`）：原版 `draw_arc` 的起止角就是
  `A_i ∓ H`，证据里没有镜像那一步（**观测**：`subsd` / `addsd` 之后直接 `cvtsd2ss`）。
  本类不替它加。
* 三段的 A 相差 `2π/3` ⇒ 只要 `A0` 是对称的（`V = 0`），镜像前后是同一组弧；`V ≠ 0` 时
  两者差一个相位，这条差异在 §50.6 里登记为**未知**（原版有没有在别处补镜像，没证据）。

### 51.4 验收脚本里新增的 46 条断言（**这一段代替截图**；`--headless` 没有渲染器）

契约（`entity_drawer_check.gd` 第 13 节）·控件 `500×300`、棋盘 `(5, 5)`、`pos_radius = 2.0`：

| 断言 | 实测值 | 对应立即数 |
|---|---|---|
| 没调 `set_ring` 时**一条都不发** | `drawn_arc_total` 不变 | `prep` 空表（原版哈希表为空） |
| 每次 `_draw` 发 **3 条 `draw_arc`** | `arcs_delta == 3 × draws_delta` | `0x4b366c/0x4b36fd/0x4b378a` |
| 每次 `_draw` 发 **3 个 `draw_polygon`** | `polys_delta == 3 × draws_delta` | 三次 `call 0x4b9a70` |
| 圆心 | `(300.0, 100.0)` | 棋盘 `(5,5)` 的投影，`k = 10` |
| 半径 | `30.0` | `pos_radius × 1.5 × k`（槽 `0x1b9b98` = `1.5f32`） |
| 第 1 条弧 start / end | `−0.52359879016876` / `+0.52359879016876` | `A0 ∓ H`，`H = π/6`（槽 `0x19f2e8`） |
| 第 2 / 3 条弧中分角 | `2.0943951023931953` / `4.1887902047863905` | 槽 `0x19f9e8` / `0x19fbf8` |
| 折点数 | `10` | `mov $0xa,%esi` |
| 线宽 | `0.30000001192093` | 槽 `0x1bb2b4` = f32 `0x3e99999a` |
| 抗锯齿 | `0.0`（false） | `xor %ecx,%ecx` |
| 颜色 | `(0.49019607901573, 0.40784314274788, 0.0, 1.0)` = `#7D6800` | `from_packed_rgb(0x00687d)`（低字节 = R） |
| 张开态单段张角 | `2×π/6` = 60°（三段合计 180°，画面上是断开的） | `0x4b3594 addsd` |
| 闭合态单段张角 | `2×1.07337748998` = 123°（三段合计 369° ⇒ 必成整圈） | `0x4b359c minsd` |
| 闭合态中分角 | `0.5` | `V/120`（槽 `0x19f620` = `120.0`） |
| 箭头尖端 | `(330.0, 100.0)`，距圆心 `30` px | `P0 = C + r·(cos θ, sin θ)`（`0x4b9b13`） |
| 箭头底边两点 | 距圆心 `35` px（`r + 0.5`） | 槽 `0x19f6d0` = `0.5` |
| 箭头底边张角 | `12°` | 槽 `0x19fb68` / `0x19fc80` = `±0.20943951023931953` |
| **关掉准备态**（`clear_ring`）之后 | 弧与多边形都不再增加、`last_arc_count() == 0` | `prep` 表删除（原版两处 `test` 分支，语义未知） |

⇒ **"画面上有没有圈"这条，用上表第 2–3 行（每次 `_draw` 恰好 3 弧 + 3 多边形）与
圆心/半径/角度那几行代替截图**：`_draw` 被引擎调到过（`draw_call_count` 每帧 +1），
每次调用的实参就是这些数。**截不到图不等于没画**，但也不等于"画得像原版"（§50.6）。

### 51.5 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target33
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                    # 3
"$GODOT" --headless --path client --import                              # 新增 .gd/bindings 后必须重扫
"$GODOT" --headless --path client --script res://tools/entity_drawer_check.gd   # 118 / 0（基线 72）
cargo build --offline --example serve_match
export HOME=<假 HOME>                       # selftest 读 $HOME，不走 XDG_DATA_HOME
mkdir -p $HOME/.local/share/godot/app_userdata/Graphwar
cp -r <replays 快照>/replays $HOME/.local/share/godot/app_userdata/Graphwar/
"$GODOT" --headless --path client -- --selftest                         # 709 / 0
python3 tools/coverage_matrix.py                                        # 生成物重生成
python3 tools/coverage_matrix.py --check                                # OK
python3 tools/check_doc_refs.py                                         # 退出码 0
```

| 口径 | 改前（`CG@e846c15` + `3d1e504`） | 改后 | 退没退 |
|---|---|---|---|
| `entity_drawer_check` | 72 passed / 0 failed | **118 passed / 0 failed** | +46，全是准备圈 |
| `assemble_check` | 203 | **203** / 0 | 没退 |
| `plane_on_screen_check` | 96 | **96** / 0 | 没退 |
| `obstacle_drawer_check` | 116 | **116** / 0 | 没退 |
| `function_drawer_check` | 132 | **132** / 0 | 没退 |
| `draw_classes_check` | 235 | **235** / 0 | 没退 |
| `curve_on_screen_check` | 96 | **96** / 0 | 没退 |
| `selftest`（assets + 回放样本 + `serve_match`） | 709 / 0 failed | **709 / 0 failed** | 没退（断言条数不变） |
| `cargo test --offline` | 1585 | **1585** / 0 | 没退（`_scaffold` 只在 `gdext-bindings` 下编译） |
| `cargo test --offline --features gdext-bindings` | 1669 | **1672** / 0 | +3（新单测） |
| `coverage_matrix --check` | OK | **OK**（`src_fns` 2504 → 2522） | — |
| `check_doc_refs.py` | rc = 0 | **rc = 0** | — |

### 51.6 仍是推断 / 未知 + 下一步

**触发接线：已做**（2026-11-08，**协调者接力**，任务 33 的允许清单不含播放屏逻辑）。

`play_screen.gd` 的 `_sync_entities()` 末尾现在会填 `prep` 表：只给"本回合行动的士兵"一条
（`set_ring(id, p.x, p.y, GwCore.SOLDIER_HIT_RADIUS, _curve_tick)` + 首次出现时 `set_ring_base`），
其余实体 `clear_ring(id)`；`_ring_base` 表记每具士兵的起始量。

⚠️ 这条接的是**我方选择**，不是观测到的原版路径，三处都是选定的：
- **谁填表**：原版是 `GraphwarBackend` 的 `pos_info_event`（还没接）⇒ 这里按回合状态代填；
- **`pos_radius`**：用 `GwCore.SOLDIER_HIT_RADIUS` = 0.5（`PosInfo.radius` 恒 0.5，`docs/131`）；
  若证实 `PosInfo` 的第 3 个 f64 已是像素，则 `k` 要换成 1.0（见 §50.0）；
- **`V`**：用播放屏的 `_curve_tick`（同一个喂给 `GraphwarPlane.set_fractional_tick` 的时钟）。

自检加了 3 条端到端断言（`selftest` 709 → **712**）：`EntityDrawer` 在树里、`prep` 表里恰 1 条、
`set_ring`/`clear_ring` 两个接线口都在。

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| 触发（谁在什么时候把士兵标成准备态） | **未知** | `pos_info_event` 的载荷语义 / `draw` 里两处 `test` 分支的条件；本轮只做出"填了就画"的机制 |
| `PosInfo` 三个 f64 的**单位** | **未知** | 需要运行时读 `pos_info_event` 的真值（`docs/140` §3.1 那 14% 与它同源） |
| `pos_radius` 的**物理含义** | **未知** | `docs/140` §0 第 ⑧ 条：环半径只能从"实测外径/内径 = 1.4240"反解到 `R/W = 2.859` |
| 弧/箭头两组分支是否互斥、`V` 的相位是否要镜像 | **未知** | 哈希表语义；本类按"都画、不镜像"实现 |
| 画出来的样子与原版是否**逐像素**一致 | **未知** | `--headless` 视口 64×64、无渲染器，本轮**没做**有限窗口截图（§43.5 的办法） |
| `z_index` / 队色表 / 淡出 | **未知** | §43.6(d) 第 4–7 条，与本轮无关，仍然缺 |
