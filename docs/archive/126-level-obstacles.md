# 126 · 关卡障碍真正出现：`GW2Rebuild.level_obstacles` + 客户端绘制 + 参与命中

状态：**现行**。本文回答一件事：客户端打战役时"空场上打靶"这条老账
（`docs/125` §7 第 4 条）**结了没有、怎么结的、还差什么**。

> 前置：[`118-campaign-data.md`](118-campaign-data.md) §7（21 关障碍参数 + 生成公式）、
> [`55-level-generator-closed.md`](55-level-generator-closed.md) / [`56-levelgen-replay-complete.md`](56-levelgen-replay-complete.md)
> / [`61-levelgen-rust.md`](61-levelgen-rust.md)（生成器已逐位复现）、
> [`120-hit-test.md`](120-hit-test.md)（`hit_test` 的掩膜规则）、
> [`125-playscreen.md`](125-playscreen.md) §7（本文要消掉的那条差异）、
> [`30-kill-semantics.md`](30-kill-semantics.md)（障碍让弹道**停下**、士兵**不**挡弹道）。
> 本文**不改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级沿用 `AGENTS.md`：**逐位验证** / **观测** / **推断** / **未知**。

---

## 0. 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新内核方法 | `GW2Rebuild.level_obstacles(level_index: int, seed: int) -> Dictionary{ok, obstacles, error}` | 逐位验证（§5.3 的注册日志 + GDScript 实调成功） |
| 生成本体 | **复用** `src/graphwar_level_generator.rs` 既有实现（`generate` 13/13 逐位、`generate_center`），本文件没有第二份公式 | 逐位验证（单测 `agrees_with_levelgen_entry_point_bit_for_bit` / `seeded_entry_reuses_the_verified_generators`） |
| 21 关参数表 | `src/graphwar_level_generator::LEVEL_OBSTACLE_PARAMS`，逐条抄自 `docs/118` §7.1（每行注释带该关的闭包预设地址） | 逐位验证 + 运行时（`docs/118` 原文的 `C`+`R` 双源）；本文再加一条"照文档重抄一遍"的测试 |
| **障碍真的画出来了** | 是。黑色实心圆，半径 = 生成器给的 `r`（棋盘单位），逐像素与原版真帧对齐（§4） | 观测（截图 + 像素取样） |
| **子弹真的被墙挡住** | 是。端到端手测：`x/10` 在 `first_hit=4480` 撞墙，`stop=(24.00, −0.90)`，画出来的点序列 = `4480+1` 点（§5.4） | 观测 + 逐位验证（单测 `generated_obstacles_actually_block_a_shot`） |
| `cargo test --offline` | **272 → 287**（+15，全绿） | — |
| `bash client/run.sh selftest` | **146 passed, 0 failed**（条数不变，**未改任何断言**） | — |
| `bash client/run.sh dump` | `@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…` | 逐位验证 |
| 内核改动 | `src/graphwar_level_generator.rs`（参数表 + 入口）、`src/api.rs`（新方法）、`src/gdext.rs`（**+1 行**注册） | — |
| 客户端改动 | `core_bridge.gd`（+52/−0）、`play_screen.gd`（+92/−7）、`plane_view.gd`（+51/−2）、`join_level_pop_up.gd`（+14/−0，父 agent 追加的弹窗配色） | — |
| 仍未定 | **客户端自生成 vs 服务端广播**的取舍（§6.1）、障碍被摧毁的洞、我方士兵/靶子位置、三条"用户口述"的开火表现（§6.3） | — |

一句话：**墙有了、画出来了、也真的挡子弹了；"每局布局不同"这条原版行为也复刻了
—— 但"谁决定这一局的布局"这件事，最终应当由服务端说了算，现在是客户端在代打。**

---

## 1. API 契约

### 1.1 签名（逐位验证：§5.3 注册日志 + GDScript 实调）

```text
GW2Rebuild.level_obstacles(level_index: int, seed: int) -> Dictionary
```

| 键 | 类型 | 含义 |
|---|---|---|
| `ok` | `bool` | 调用是否成功；`false` 时看 `error` |
| `obstacles` | `PackedFloat64Array` | `[x, y, size] * n`，**棋盘坐标**；`size` = **半径**（游戏单位） |
| `error` | `String` | 成功时空串 |

**为什么 `obstacles` 与 `hit_test` 的入参是同一个类型/同一套顺序**：
客户端拿到内核给的数组，**原样**再喂回 `hit_test(points, obstacles, soldiers)`，
中间不经过 `Vector2`（那是 f32）——否则障碍坐标会在往返里掉精度。
`level_obstacles` 返回的 `[x,y,size]` 与 `hit_test` 入参的 `[x,y,size]` 是**同一个量**
（`docs/120` §1：`size` = 障碍半径，`plane::rasterize` 里 `r = size * 1000/50` 像素）。

错误码：

| 码 | 触发条件 |
|---|---|
| `bad_level_index` | `level_index` 不在 `0..=20`（21 关，`docs/118` §5.2 三处独立证据） |
| `bad_arguments` | 参数个数/类型不对（`level_index`、`seed` 必须是整数；`4.5` 这种**整值以外**的 FLOAT 也拒） |
| `obstacles_not_finite` | 生成器给出 NaN/±inf（不该发生，单测钉住了；真出现就报错，不把 NaN 交给渲染） |
| `internal_panic` | `catch_unwind` 兜底（跨 FFI 的 panic 会 abort 掉 Godot） |

`level_index` 是 **0 基**（`0` = `level_0` = 第 1 关），与客户端
`GwCampaign.level(i)`、`GameState.level_index` 同一套编号。

### 1.2 `ok:true` + 空数组 ≠ 错误

`level_0` / `level_1` / `level_16` 三关 **本来就没有障碍**（`docs/118` §7.1「无障碍」），
所以它们返回 `ok:true, obstacles=[]`。客户端按"空表画空气"处理，**不当错误**。

---

## 2. 参数来源与种子语义

### 2.1 21 关参数表（`src/graphwar_level_generator::LEVEL_OBSTACLE_PARAMS`）

`docs/118` §7.1 的「障碍（模式 / 数量）」列，21 行全抄进代码，每行注释都带该关的
**闭包预设地址**（`C` 短码，`.data.rel.ro` 里可指认）：

| 关（1 基 / 内部 0 基） | 模式 | `min_count` | `C` 地址 |
|---|---|---|---|
| 1 / `level_0` | 无障碍 | 0 | `0x95a350`/`0x95a378` |
| 2 / `level_1` | 无障碍 | 0 | — |
| 3 / `level_2` | 中心单障碍 | 0（恒 1 颗） | `0x49f1c0` |
| 4 / `level_3` | 中心单障碍 | 0（恒 1 颗） | `0x49f370` |
| 5 / `level_4` | 散布 | **15** | `0x49fe80` |
| 6 / `level_5` | 散布 | **5** | `0x4a12f0` |
| 7 / `level_6` | 散布 | **10** | `0x4a2410` |
| 8 / `level_7` | 散布 | **6** | `0x4a2da0` |
| 9 / `level_8` | 散布 | **10** | `0x4a2410` |
| 10 / `level_9` | 散布 | **5** | `0x4a12f0` |
| 11 / `level_10` | 散布 | **10** | `0x4a2410` |
| 12 / `level_11` | 散布 | **5** | `0x4a12f0` |
| 13 / `level_12` | 中心单障碍 | 0（恒 1 颗） | `0x4a5e80` |
| 14 / `level_13` | 散布 | **10** | `0x4a2410` |
| 15 / `level_14` | 散布 | **5** | `0x4a12f0` |
| 16 / `level_15` | 散布 | **10** | `0x4a2410` |
| 17 / `level_16` | 无障碍 | 0 | `0x95d358` |
| 18 / `level_17` | 散布 | **10** | `0x4a2410` |
| 19 / `level_18` | 散布 | **10** | `0x4a2410` |
| 20 / `level_19` | 散布 | **10** | `0x4a2410` |
| 21 / `level_20` | 散布 | **15** | `0x49fe80` |

`min_count` 是**下界不是定值**（`docs/118` §7.2 的运行时硬证据：关 11/18/20 出现过恰好 10 个、
关 5/21 出现过恰好 15 个）。所以表面上"每关障碍数"其实是个**随机变量**：
`n = trunc(normal()*7 + 15)`，`n <= min_count` 时钳到 `min_count`。

三条防"改错表"的测试：
* `levelgen::tests::level_params_match_docs_118_table` —— 期望值是**照文档原文重抄一遍**的
  21 项 `(模式, min_count)`，与常量表逐条比对；
* `api::obstacle_tests::all_21_levels_generate_and_match_the_table` —— 走 API 实跑：
  无障碍关必须 0 个、中心式必须 1 个且 `(0,0)`/`r∈[3,8)`、散布式必须 `≥ min_count`；
* `api::obstacle_tests::agrees_with_levelgen_entry_point_bit_for_bit` —— API 产出与
  `levelgen::obstacles_from_seed` **逐位**相同（反"API 里又写了一遍公式"）。

### 2.2 **种子语义**（本轮最要紧的一条，务必读）

**原版的事实**（观测，出处可指认）：

1. 障碍布局**每局都不一样** —— `docs/118` §7.1 的 `R` 列（385 份 replay 的运行时真值）里，
   同一关出现过 **10…29** 个障碍（关 16）；关 19 出现过 14/15/23/29。
2. 原因是种子**每局开局重新取**：`docs/57` 记录种子来自 `/dev/urandom`（ChaCha 的 key），
   运行时能读到 key/counter（`docs/53`）。
3. 关卡生成从随机流的**第 3 个** `next_u64` 开始（`docs/56` §2：前 3 个被别处消费）。

**我们的事实**：

* 原版那一条 ChaCha 流**我们没有**（每局随机的 key，抓包里只有它跑过的那一次）。
  所以"给定原版种子复现历史布局"**做不到**；
* 因此 `seed` 是**调用方的输入**：同 `(level_index, seed)` ⇒ 逐位相同，换 `seed` ⇒ 换一局。
* 种子 → ChaCha key 的展开是**我们自定的**（`levelgen::seed_key`，splitmix64 摊成 4×u64），
  证据等级 **自定**：它保证"同种子同结果、不同种子不同结果"，并避开 `seed=0` 得到全零 key。
  用同款 ChaCha12（`src/rng.rs`，`docs/53`）只是为了"随机流的形态与原版一致"，
  **不代表**数值能对上任何一份历史抓包。
* 我们的流从**第 0 个** `next_u64` 开始。若将来真的拿到某局原版的 key，需要先丢弃 3 个
  再调 `obstacles_from_seed`（注释里写明了）。

一句话给后续的人：**`level_obstacles` 不是"还原那一局的坐标"，而是"用原版同一个生成器
造一局合法的布局"。** 判据只到"公式/分布/参数逐位一致"，**不到**"坐标逐位一致"。

### 2.3 客户端给什么种子（`play_screen.gd::_level_obstacles`）

| 运行方式 | 种子 | 为什么 |
|---|---|---|
| 正常玩（`gui`） | `GwCampaign.level_seed(i) ^ randi()`，**每局抽一次** | 复刻原版"逐局不同"（§2.2 第 1 条）。实测两次进入同一关，布局不同（§5.5） |
| `--selftest` / `--dump-tree` / `--shots=` | `GwCampaign.level_seed(i)`，**固定** | 自检与截图基准必须可复现，否则每跑一次变一张图 |

`GwCampaign.level_seed(i) = (0x9E3779B9 * (i+1)) & 0xFFFFFFFF`（`campaign.gd` 里原有的
"我们的自定值"，本轮**没有改**那个文件）。

---

## 3. 客户端改动（逐文件）

| 文件 | 改了什么 |
|---|---|
| `client/src/core/core_bridge.gd` | 新增 `level_obstacles(level_index, seed)`：内核有 `level_obstacles` 就走内核；没有就 `ok:false, source="none"`（**不**退回"界面自己撒点"）。文件头补了这条契约 |
| `client/src/screens/play_screen.gd` | ① `_obstacles` 状态 + 进关时向内核要（`_level_obstacles()`，种子按 §2.3）；② `_plane.set_obstacles(_obstacles)`；③ `_resolve_shots()` 把**同一个数组**传进 `GwCore.hit_test()`；④ 新增 `_clip_to_obstacle()`：**只**决定"画到哪一点"，撞墙就把绘制点序列截到 `first_hit+1`（`shot.points` 仍是整条，见 §3.1） |
| `client/src/play/plane_view.gd` | ① `obstacles` + `set_obstacles()`；② `_draw_obstacles()` 画纯黑实心圆（半径 = `size` × `_scale()`，开抗锯齿）；③ `_ready()` 里 `clip_contents = true`（落在外面的障碍要像原版一样被棋盘边裁掉）；④ 文件头的颜色表补了障碍一行与出处 |
| `client/src/screens/join_level_pop_up.gd` | （父 agent 追加的任务）说明栏的 `read_only` 样式改成**无底色**、字色钉成原版的 `(223,223,223)`；面板恢复成原版那种**统一深绿**（§4.3） |

### 3.1 为什么"截断"只做在绘制层

`fire()` 里现在是这样分工的：

```gdscript
var drawn := _clip_to_obstacle(pts)      # 只为画：points 截到撞点、impact = stop
_plane.set_path(drawn.points, impact, true)
...
var shot := {"points": pts, ...}          # 结算仍用**整条**点序列
```

**没有**把"一次画完整条 = 本回合结束"写死：`shot.points` 保留全量，`drawn.impact` 单独留着。
将来的 tick 弹道动画应当沿 `shot.points` 逐点推进、到 `drawn.impact` 收尾
（父 agent 已另外派人做"开火表现"，见 §6.3）。

---

## 4. 障碍长什么样（全部有取样坐标）

### 4.1 原版真帧的取样（观测）

来源：`.agent-work/playfit/shots/orig_play_default.png`（原版工程 + 原版 `.so` 的 **LAN 对局**真帧，
1920×1160 —— 战役局没有障碍，所以这里必须用 LAN 那一帧）。棋盘矩形 `(299,194)–(1579,962)`，
**25.6 px/单位**（粗网格 427→555 = 128 px = 5 单位；y 轴在 x=939）。

| 元素 | 实测 | 取样 |
|---|---|---|
| 颜色 | **`#000000` 纯黑实心** | 5 个圆的圆心全是 `(0,0,0)`；最大那个圆内部 9289 个采样点全黑 |
| 边缘 | **抗锯齿**：每个圆最外圈有 1 个半覆盖像素 | 灰度 `154` / `242` / `43`（同一条扫描线上圆的两侧） |
| 半径（棋盘单位） | **0.80 / 0.81 / 1.66 / 2.13 / 3.42** | 5 个连通块的 bbox 半宽 ÷ 25.6 |
| 是否裁剪 | **是**，落在棋盘外的部分被裁掉 | 左下那颗圆心 `y=−12.44`、`r=3.42`，下缘 `−15.86` 被切在 `y=−15` |
| 是否填充 | 实心（不是圆环） | 圆内无空洞 |

**"半径 = 生成器给的 `r`"是推断，但旁证很硬**：原版生成器的半径下限是 `maxsd 0.8`
（`docs/55` §2），而真帧里最小的那个圆量出来正好 **0.80 单位**；`docs/56` §3 抓到的 13 个
障碍里也确实出现过 `r = 0.800`。反过来我们这一局（关 5 Level 4，种子 387276957）生成的
20 个障碍里有 2 个是 `0.800000`，画出来与真帧那颗同尺寸。缺的证据：那一帧对应的真实
`r` 列表（原版每局随机，抓不到）。所以：**半径映射 = 推断（有 0.80 这一处两点对齐）**。

### 4.2 我们的实现（`plane_view.gd`）

```gdscript
draw_circle(board_to_screen(o.pos), _scale() * o.size, COL_OBSTACLE, true, -1.0, true)
```

* 颜色常量 `COL_OBSTACLE = Color(0,0,0)`（与棋盘外框同色，**不是**主题色）；
* `antialiased = true`：实测（Godot 4.7.2 探针）非 AA 时边缘是硬跳变 `255→0`，
  AA 时出现一个 `171` 的半覆盖像素 —— 与原版真帧的 `154/242/43` 同形；
* `clip_contents = true`：实测（同一探针）它连**控件自己** `_draw` 画出去的东西也裁，
  所以不需要另造裁剪容器。副作用是顺带裁掉了原来画在棋盘外的那半条网格/外框线
  （前后对比：棋盘四周各 1 px 的杂线消失，外框左缘对齐到与原版相同的 `x=300`），
  这属于**更接近原版**，不是回归（`MainMenu` 截图像素完全不变，棋盘内只差这一圈 5371 px）。

### 4.3 JoinLevelPopUp 的深色带（父 agent 追加）

**症状**：面板上横贯一条深色带（`Level 0` 标题与说明栏那块），原版整块面板是统一深绿。

**根因**（观测）：说明栏是 `TextEdit` + `editable = false`，而主题只覆盖了
`TextEdit/normal`；Godot 对只读文本框画的是 **`read_only`** 那套样式 ⇒ 落回默认主题，
于是出现深色底 `(26,49,39)` 与 `font_readonly_color` 暗绿字 `(124,136,131)`。

**修法**（`join_level_pop_up.gd`）：`normal` / `focus` / `read_only` 三个 StyleBox 全部设成
`StyleBoxEmpty`，字色钉成原版真帧取样值 `(223,223,223)`。

**核对**（像素）：

| 项 | 原版 | 修前 | 修后 |
|---|---|---|---|
| 面板内竖向扫描（x=250） | 全 `(26,59,45)` | `(26,59,45)` → `(26,49,39)`（y=198 起）→ `(26,59,45)` | **全 `(26,59,45)`**（y=104…1152） |
| 说明文字最亮像素 | `(223,223,223)` | `(124,136,131)` | **`(223,223,223)`** |

（原版取样：`.agent-work/playfit/shots/orig_join_level.png`；我们的截图见 §5.4。）

---

## 5. 验证记录

### 5.1 内核单测（`cargo test --offline`）

```
272 → 287 passed, 0 failed        # +15：levelgen 7 条、api 8 条
```

新增（每条都对应上文一个论点）：

| 测试 | 钉住什么 |
|---|---|
| `levelgen::level_params_match_docs_118_table` | 21 关参数 = `docs/118` §7.1 原文 |
| `levelgen::every_level_generates_through_the_seeded_entry` | 21 关全生成得出来；界/下限/有限性 |
| `levelgen::seeded_generation_is_reproducible_and_seed_dependent` | 同种子逐位、换种子换布局 |
| `levelgen::seeded_entry_reuses_the_verified_generators` | 入口 = 既有已验证生成器（无第二份公式） |
| `levelgen::scatter_counts_vary_with_the_seed` | 个数是随机变量（不是常数） |
| `levelgen::out_of_range_level_has_no_params` / `degenerate_seeds_are_safe` | 边界与退化种子不 panic |
| `api::obstacle_tests::result_shape_is_exactly_the_contract` | `{ok, obstacles, error}` + 扁平 f64 逐位 |
| `api::obstacle_tests::all_21_levels_generate_and_match_the_table` | 走 API 的 21 关全表 |
| `api::obstacle_tests::obstacle_free_levels_return_an_empty_ok_list` | "无碍"≠"失败" |
| `api::obstacle_tests::out_of_range_level_is_rejected` | `bad_level_index` |
| `api::obstacle_tests::same_seed_is_bit_identical_and_new_seed_changes_the_layout` | 种子语义 |
| `api::obstacle_tests::degenerate_seeds_are_safe` | `seed ∈ {0, −1, i64::MIN/MAX}` 不崩 |
| `api::obstacle_tests::agrees_with_levelgen_entry_point_bit_for_bit` | API ↔ 生成器逐位 |
| `api::obstacle_tests::generated_obstacles_actually_block_a_shot` | **墙真的挡子弹**（撞点 = 圆与直线的几何交点 ±0.25；拿掉障碍就飞到底） |

### 5.2 客户端自检

```
bash client/run.sh selftest   →   SELFTEST 146 passed, 0 failed
```

条数 **146 → 146**，**没有删也没有放宽任何断言**（本轮没有改 `tools/selftest.gd`）。

### 5.3 内核注册（`dump` 通道）

```
@@PROJECT version=2.0.60 stretch_mode=disabled viewport=1600x900 mode=2
@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…
```

启动日志四行齐全：

```
[gw2-rebuild] registered method ping()
[gw2-rebuild] registered method path_calculate(expr, x0, y0, direction)
[gw2-rebuild] registered method hit_test(points, obstacles, soldiers)
[gw2-rebuild] registered method level_obstacles(level_index, seed)
```

### 5.4 端到端：墙出现了 + 子弹被挡住（观测）

做法：把客户端复制到 `.agent-work/obst/harness/client`，在该副本的
`tools/screenshots.gd` 里加两个临时状态（**不入库**：`tools/` 不在本轮的改动范围）：

```
PlayScreenObstacles      → 战役关 4（Level 4，散布 min_count=15）+ 撞击
PlayScreenObstaclesFire  → 同一关，自动挑一条会被挡住的函数打出去
```

产物：`.agent-work/obst/harness/shots/PlayScreenObstacles.png` /
`PlayScreenObstaclesFire.png`（1920×1160）。日志：

```
OBST count=20                       # 关 5（Level 4）min_count=15，实际 20 个
OBST|6|-15.832451|-13.561327|0.800000   # 半径下限 0.8 出现了（与原版真帧那颗同尺寸）
OBST|10|-4.567729|21.684000|3.837304    # 圆心可以在棋盘外（公式允许）
TRY|0.0|points=4587|blocked=false|first_hit=-1|stop=(24.99375, -5.1)
TRY|x/10|points=4587|blocked=true|first_hit=4480|stop=(24.00, -0.90)
FIRE|x/10|drawn_points=4481|want=4481|match=true
            status=本轮命中 0 个士兵 ／ 判定来源：kernel ／ 内核 hit_test（掩膜 1000×600 + 士兵半径）（弹道被障碍拦下）
```

读法：

* `0.0`（水平线）**不**被挡，飞到 `x≈24.99`；
* `x/10` 在 `first_hit=4480` 撞墙、停在 `(24.00, −0.90)` —— 那个点正好落在
  障碍 #18 `(25.67, 3.09, r=4.27)` 的圆上（距离 4.32 ≈ r）；
* 画出来的点序列 = **4481** = `first_hit+1` ⇒ **绘制也在撞点截断**，不是"画完整条、只是判定说挡住了"；
* 同一次开火的 `hit_test` 走的是**内核**（`source=kernel`），且 `blocked` 被带回状态行。

### 5.5 "每局不同"也复刻了（观测）

把副本里的 `_reproducible_run()` 临时改成 `return false`（强制每局随机种子），连跑两次：

```
run 1: OBST count=15   OBST|0|6.758580|0.547861|4.649520
run 2: OBST count=15   OBST|0|13.850442|6.630931|1.596828
```

布局不同 ⇒ 与原版"逐局不同"（`docs/118` §7.1 `R` 列）同向。**注意**：个数两次都是 15
（= `min_count`），这符合公式 —— `n = trunc(normal*7+15)` 有一半概率低于 15 被钳住。

### 5.6 没有回归

* `MainMenu` 截图：与本轮改动前**逐像素相同**（0 个差异像素）；
* `PlayScreen` / `PlayScreenCampaign`（无碍关）：只差棋盘四周 **1 px** 的杂线（§4.2 的裁剪副作用，
  朝原版方向），棋盘内部、键位、底栏、计时、修正常数全无变化；
* `JoinLevelPopUp`：差异全部落在深色带矩形内（`(246,197)–(1674,789)`），见 §4.3；
* 复现命令（含 `--import`，Godot 会缓存 class 与资源）：

```bash
export CARGO_HOME=… CARGO_TARGET_DIR=…
cargo build --release --offline && cp target/release/libgw2_rebuild.so client/
export GODOT=…/Godot_v4.7.2-stable_linux.x86_64
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
$GODOT --headless --path client --import
bash client/run.sh selftest        # 146 passed
bash client/run.sh dump | grep @@KERNEL
```

---

## 6. 仍未定 / 仍未做（照实写）

### 6.1 **客户端自生成 vs 服务端广播**（最重要的一条）

现在：**客户端**在进关时按 `(关卡, 种子)` 自己调内核生成障碍。

* 这**不是**原版行为。原版是**权威一方**开局生成、把障碍作为 `AddObstacle` 事件发给对局各方
  （`docs/30`/`docs/48` 的抓包里能看到 `AddObstacle`；`src/protocol.rs` 里已有这条事件的解析）。
  客户端自己生成只是在"我们还没有服务端广播"时的**代打**。
* 危害很具体：**同一个对局里两边客户端的障碍可能不一致**（种子各自抽），
  于是同一发弹道在两边算出的停点不同 —— 联机对战会直接错位。
* 正确的接法（未做）：服务端广播的 `AddObstacle` 一到，`play_screen` 用**服务端那批**覆盖
  `_obstacles`（`obstacles` 是 `Array[{pos, size}]`，与 `level_obstacles()` 的返回同形，
  换源不需要改绘制/判定代码）；`level_obstacles` 退化成"单机练习/断线兜底"。
* 为什么本轮不做：`AddObstacle` 的**事件字段顺序与语义**属于别的 agent 的 `src/protocol.rs`
  范围，且客户端还没有接收服务端事件流的那条通路。**登记在此，不擅自接**。

### 6.2 障碍的其它未定项

| # | 项 | 现状 | 缺什么 |
|---|---|---|---|
| 1 | 障碍被摧毁后的"洞" | 不画、也不影响掩膜 | `QueueDestroyObstacle` 的语义本身仍未定（`docs/38` §3：两条样本对不上） |
| 2 | 半径 = `r` 的直接证据 | 推断（0.80 那一点两点对齐，§4.1） | 拿到某一局真实的 `r` 列表 + 同一帧截图 |
| 3 | 障碍的层级（障碍在士兵/弹道之上还是之下） | 画在弹道与士兵**之下** | 原版真帧里弹道不会与障碍重叠（撞点即停）， Soldiers 也没有与障碍重叠的样本 |
| 4 | 障碍的抗锯齿参数 | 用 `antialiased=true` 复现"半覆盖边缘" | 原版是 Rust 侧自绘，AA 的具体采样规则未取证（只对齐了"有半覆盖像素"这个观测） |
| 5 | 我方士兵 / 靶子的位置 | **仍是我们的自定值**（`_spawn_position` / `_target_positions`） | 原版由关卡生成器随机放置，坐标未解；与障碍无关，但同属"平面上的东西从哪来" |
| 6 | 服务端侧 | `src/game.rs` / `graphwar_game_server.rs` **没有**接这条生成（本轮没碰那几个文件） | 谁在服务端调 `obstacles_from_seed`、事件怎么广播 |

### 6.3 **三条"用户口述、未实现"的开火表现**（父 agent 明确要求只登记，本轮不动手）

以下三条是用户实测原版**有**、我们**没有**的行为，**全部标为"用户口述、未实现"**
（我们这边没有任何二进制/像素证据，`docs/92`/`docs/121`/`docs/125` 也没写过）：

| # | 行为 | 现状 | 本轮是否动 |
|---|---|---|---|
| 1 | **支持键盘输入表达式**（不只是点屏幕键盘） | 只能点屏幕上的 `GwButton`（`play_screen.key_input`） | ❌ 只登记。要动 `play_screen.gd` 的输入时序，与本轮的障碍绘制互相干扰 |
| 2 | **准备发射的士兵有光圈，准备完毕光圈闭合** | 完全没有画 | ❌ 只登记（同上） |
| 3 | **函数不是瞬间抵达对面**：弹道是随 tick 推进的动画 | 一次性画完整条折线 | ❌ 只登记。**但已留出口**：`shot.points` 保留全量、撞点单独记在 `drawn.impact`，动画实现者沿 `shot.points` 推进即可（§3.1） |

> 这三条都归"开火表现"那一摊，父 agent 已说明会另派一个 agent 做；本轮**只保证**
> 障碍渲染与 `hit_test` 的接法**不**把"一次画完 = 回合结束"写死。

> **2026-11 更新（另一轮）**：这三条**已经做出来了**，见 [`135-fire-fx.md`](135-fire-fx.md)。
> 那里的四档标注与本表的"用户口述"口径一致：第 1 条的**机制**（原版是
> `LineEdit` 子类 + `gui_input` ⇒ 输入框聚焦后接管键盘）升级成了**观测**；
> 第 2 条仍然只有"存在"是口述、**长什么样全部自定**；第 3 条 tick 频率是**观测**、
> 每 tick 弧长是**推断**。本文 §6.3 的登记保留原样（它是当时的准确状态）。

---

## 7. 对现有文档的影响

| 文档 | 影响 |
|---|---|
| `docs/125-playscreen.md` §5.1 / §7 第 4 条（"障碍物不画"） | **已消除**，以本文为准。那条的措辞"没有同一条 ChaCha 流所以不画"只说对了一半：流确实没有，但**生成器是逐位复现的**，缺的只是种子语义（§2.2） |
| `docs/108-client-rebuild.md` §7 第 3 条（"仅障碍物仍不画"） | 同上，已消除 |
| `docs/118-campaign-data.md` | **不改**（§7 的 `C`/`R` 两列就是本文参数表的来源，原文没有任何错） |
| `docs/INDEX.md` / `docs/HANDOVER.md` | **按约定不改**，需要父 agent 汇总时自行登记本文 |
