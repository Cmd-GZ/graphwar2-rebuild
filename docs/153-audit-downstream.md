# 153 · 下游两轮报告（CG 14 份任务）的对抗性复算

> ⚠️ **审计 2026-11-08（读本文前先看）**：本文的 "master = …" 都是**当轮**（2026-11-07）读数，
> 不是现行基线。现行值：`cargo test --offline` **1710 / 0 failed**（`--features gdext-bindings` 1816）、
> 三轴内容命中 **647 / 647 / 637**、种类序列 **1075 ×3**、死亡集合 **366 / 385**、挖洞点 **207 / 294**
> （唯一权威源 `fixtures/guardrails.json`）。本文的**判定结论**（轴/口径错配、反事实当结果）不受影响。

状态：**审计完成**（2026-11-07 夜，master `02fdfe21` 上复算）。
目的不是再确认一遍，而是**找出虚报**：凡是复算通过写「一致」，不通过写「不一致」并给证据，
复算不了的写「无法判定（缺什么）」。**报告本身不当证据**，只当"待验的声称"。

## 结论计数（**先看这个，再往下翻**）

| 判定 | 项数 | 哪几项 |
|---|---|---|
| **一致** | **13** | R1-01（当轮）· R1-02 · R1-03 · R1-04 · R1-06 · R2-09 · R2-10b · R2-11 · R2-13 · R2-14a · R2-14c · R2-§2（当轮）· R2-§1 |
| **部分一致**（主结论成立，附带的计数复现不出来） | **2** | R1-07（1..40 连续 ✓ / 242、318 无法判定）· R2-12（22 条 ✓ / 逐型定名无法判定） |
| **不一致** | **2** | **R2-10a**（种类序列 1074 标成 `batch` 轴，实为 `batch_conv`）· **R2-14b**（收敛层 198/291 是反事实，实测仍 66/291） |
| **无法判定** | **1** | R1-05（缺交叉产物与交叉工具链） |
| **文档级不一致（已就地更正）** | **1** | §1-C：`docs/14` §12.5 / `docs/06` §24.7 的「真实路径 134/259」在同一提交上是 **139/258** |

⇒ 合计 **18 项**：**一致 13 / 部分一致 2 / 不一致 2 / 无法判定 1**，另加 1 条文档级更正。

## 0 口径与前提（**这一节是本次审计能成立的原因**）

1. **地址一律 ELF 虚地址**（`objdump`/`readelf` 直接给的）；Ghidra/REA = 该值 `+0x100000`，
   只加一次。本文每个地址都注明是哪种口径。
2. **数字必须带轴**。同一份 401 回放 / 1080 批上有三条驱动轴，互不相同：

   | 轴 | 入口 | 代码位置 |
   |---|---|---|
   | `fire` | `replay_attrib --drive fire` | 每发调一次 `Match::fire`（`src/game.rs`） |
   | `batch` | `replay_attrib --drive batch` | 每批调一次 `Match::fire_batch`（`src/game.rs`，**真实实现**） |
   | `batch_conv` | `batch_conv --drive batch --policy strict` | `_scaffold/diffconv.rs::ConvPredictor::fire_batch`（**收敛层预测器**） |

   本文所有数字后面都跟轴名。上一轮事故（`archive/155-round-log.md` §29、`docs/HANDOVER.md` §0.0.0.3）就是这张表被写错一行。
3. **`origin/CG` 与 master 的代码逐字节相同**，所以"测量差异"不可能来自代码差异：

   ```bash
   git -C gw2-rebuild diff --stat master..origin/CG -- src/ client/ examples/ tools/ tests/ fixtures/ Cargo.toml
   # （空）—— 两分支只差 dispatch/ 与 docs/HANDOVER.md
   ```

   于是任何"当时测的"数字都能**在对应提交上重跑**，不需要相信记录：
   `git worktree add wt 8755db9e` 后 `cargo build --release --offline --examples` 即可
   （replay 相关 example **不需要** `gdext-bindings`，冷编 20–30 s）。
4. 被审对象：`origin/CG:dispatch/reports/CG-2026-11-07.md`（第一轮 8 份）、
   `origin/CG:dispatch/reports/CG-round2-2026-11-07.md`（第二轮 6 份）。

## 1 不一致（先看这三条）

| # | 声称（出处） | 实测 | 影响 |
|---|---|---|---|
| A | 第二轮 §0/§2：**`batch` 驱动**「种类序列 946 → **1074** / 1080」 | 真 `batch` 轴：**918 → 915**（`b49c3150` 括号修复后 **919**）。`946` 与 `1074` 都是 `batch_conv`（收敛层）轴：master 上实测 `batch_conv --drive fire` = **946**、`--drive batch` = **1074** | **表头错配**。`docs/06` §29 已定性为"不是虚报、是口径"，但**下游报告本身没改**，读报告的人仍会把 1074 记到 `batch` 上——这正是要防的那个错 |
| B | 第二轮 §2：延迟段挖洞点「（收敛层，同一 291 分母）66/291 → **198/291**」；`archive/155-round-log.md` §24.7 与 `docs/14` §12.5 同 | 收敛层**仍是 66 / 291**（master，两种 conv 驱动都测过）。**198 是工具的反事实预测**：`tools/hole_axis_attrib.py` 打印的是「**改成**「挖洞点 = 爆炸点」**以后** 198 / 291」——它按探针里的爆炸点**重算**，不代表合并后的代码；14 的修改只落在 `src/game.rs`，`_scaffold/diffconv.rs` 一行没改 | **把"反事实"写成"已达成"**。第二轮报告 §5 自己就写着"convergence 路径的修正在 `diffconv` 里不生效"⇒ 与 §2 的 198 自相矛盾 |
| C | `docs/14` §12.5 / `docs/06` §24.7：真实代码路径挖洞点「1/259 → **134/259**」 | 在**同一提交** `8755db9e` 复跑 `--drive batch --policy strict` = **139/258**；`--drive fire` = 138/258；合并后（含括号修复）= 140/262 / 139/262。第二轮报告写的 **139/258 才是对的** | 文档数 134/259 在它自己的提交上也复现不出来（分母 259 也对不上 258），是"在另一棵树上量的" |

三条的共同点：**没有一条是"编数据"，两条是"轴/口径没写清"，一条是"反事实当成结果"**。
按 `AGENTS.md`「合并纪律」C 条，这三条都必须带轴名 + 带"实测/预测"标记。

## 2 逐项复算（18 项）

树的口径：`f0ea313` = 第二轮起点（CG 起点）、`8755db9e` = 第二轮 6 份的合并点、
`02fdfe21` = master（= `origin/CG` 的代码 + 一行括号修复 `b49c3150`）。

先把三条轴在三棵树上的数摆出来（本次全部自测）：

| 树 | `fire` 内容 / 种类 | `batch` 内容 / 种类 | `batch_conv` 内容 / 种类 | 死亡集合（默认口径） | 挖洞点（默认口径） |
|---|---|---|---|---|---|
| `f0ea313` | 21 / 818 | 21 / 918 | 568 / 945（逐发） | 211 / 379 | 1 / 259 |
| `8755db9e` | **324** / 818 | **341** / **915** | 595 / **1074** | **209** / 379 | 138–139 / **258** |
| `02fdfe21`（master） | 325 / 819 | 346 / 919 | 595 / 1074 | 215 / 379 | 139–140 / 262 |

`conv --policy strict` 的内容命中：`f0ea313` = **568 / 1080**（⇒ 失败 **512** 批，正是第一轮 §2 那个 512），
`8755db9e` 与 master = **569 / 1080**（⇒ 失败 511 批）。

| # | 声称（带轴） | 我怎么复算 | 实测 | 判定 |
|---|---|---|---|---|
| R1-01 | 容器**按类型**查得 **3** 项；`ASSEMBLE 86 passed` | 跑 `client/tools/assemble_check.gd`（headless）；并看当轮脚本的 `WIRED` | 当轮 `6e9d639` 的 `WIRED` = **3 项**（`GraphwarGameStateManager` / `TurnManager` / `PlayerManager`）；脚本静态 `ok(` 当轮 71 处、现 79 处 ⇒ 断言条数随 `WIRED` 线性增长，86 与 3 项相容。**现树**（`WIRED` 22 项）= `ASSEMBLE 183 passed, 0 failed` | **一致（当轮）／已过期**：86 是 3 模块时的数，任务 12 扩到 22 后必须写 183 |
| R1-02 | `update_function` 增量追加：10→12 只补 2 点、喂回 5 点仍是 12；132 断言 | `cargo test --offline` 的 `append_window_is_the_only_increment_rule`；跑 `function_drawer_check.gd` | `append_window(10,12)=Some(10..12)`（长度 2）、`(12,5)=None`；`FUNCTION_DRAWER 132 passed, 0 failed` | **一致** |
| R1-03 | 401 份回放 / 1080 批 / 失败 **512** 全落桶；`--check-baseline` 7 个数逐项一致 | 在 `6159280b` 上 `--drive fire` + `tools/bit_exact_attrib.py --check-baseline`；在 `f0ea313` 上 `--drive conv --policy strict` 复算 512 | 工具打印 `文件 286（读失败 115）`= **401**；**1080** 批；7 个数（1080/21/818/211/379/1/259）**全部 OK**；`conv strict` 内容 = 568 ⇒ 失败 = **512** ✅；加总校验（我跑的四个轴）`755 / 734 / 511 / 485 == 各自失败数` 全部 OK | **一致（但 512 没写轴）**：512 只在 `conv strict` 轴上成立，`fire` 轴是 1059 |
| R1-04 | 21 关 × 7 参数逐项相等、13 条测试、客户端 `PLACEHOLDER` 清零 | `cargo test --offline --test campaign_data`；`grep -c PLACEHOLDER client/src/app/campaign.gd` | `running 13 tests … 13 passed`；`grep -c PLACEHOLDER client/src/app/campaign.gd` = **0**（`grep PLACEHOLDER client/src` 仅剩 3 处真实常量，与 campaign 无关） | **一致** |
| R1-05 | Win `gw2_rebuild.dll` 2.79 MB、Android `.so` 827 KB、14 个导出同名同集合 | 找产物 + 找交叉工具链 | 全工作区**没有** `gw2_rebuild.dll`，也没有 arm64 `libgw2_rebuild.so`（`client/libgw2_rebuild.so` 是宿主 x86_64 的 8.2 MB）；本机**无 rustup**、无 mingw/NDK 包、无 Android 设备 | **无法判定（缺产物 + 缺交叉工具链）**：这两条只能在产出物那台机器上复核 |
| R1-06 | 唯一 **762**；多候选 **37**；444 分类 `icf 0 / tiny 172 / other 266 / inlined 6`；`--verify 86.2%`；退出码仍 1 | `tools/sym_match.py report` / `--verify`；并取 `aab6b196^` 的 fixture 对比 | `unique 681 + weak 81 = 762`；multi 37；`none 444 = inlined 6 + other 266 + tiny 172`；`一致率 86.2% < 90% ⇒ FAIL`，`EXIT=1`。基线 `aab6b196^` = 733 unique/63 multi/444 none/25 merge ⇒ +29、−26 | **一致** |
| R1-07 | `docs/16` 编号 **1..40 连续**、空号 19/20/23/34 消失；242 处标题 / 318 处交叉引用；`check_doc_refs.py` 退出码 0 | 在 `f076e8b` / `7f1e232` 上数标题；跑 `check_doc_refs.py` | `^## N` = **1..40 全在、无空号** ✅；`check_doc_refs.py` **EXIT=0** ✅；**242 / 318 复现不出来**：同提交上 `^### N.M` = 253、`^#{2,4} N.M` = 299、`§N` 引用 = 487（现树 354 / 519） | **核心一致**；**242/318 无法判定（计数口径没定义）** |
| R2-09 | `fire` 内容命中 **21 → 324** | 三棵树各跑一遍 | `f0ea313` = 21；`8755db9e` = **324**；master = 325（`b49c3150` 括号修复再加 1） | **一致** |
| R2-10a | **`batch` 驱动**种类序列 **946 → 1074** | 各树跑 `--drive batch`（`Match::fire_batch`）与 `batch_conv --drive {fire,batch}` | `batch`：918（`f0ea313`）→ **915**（`8755db9e`）→ 919（master）。`batch_conv`：946（逐发）→ **1074**（按批） | **不一致（轴错配）** → §1-A |
| R2-10b | `batch` 驱动内容命中 **21 → 341** | 同上 | 21 → **341**（master 346） | **一致（带轴）** |
| R2-11 | 回放 `end_x` 取 `pts[n-2]` 不成立；`set_function_points` = `pts[len-1].x` | `llvm-objdump -d` arm64 `libgraphwarrust.so` 0x489428 | `0x489428: ldur d1, [x8, #-0x10]`，`x8 = base + len*16` ⇒ 加载**最后一个点**的 `.x`（点 = 16 B） | **一致** |
| R2-12 | 22 条 `add_module` **全部定名**；容器 **3 → 22** | `objdump` 扫 `GraphwarGameClient::new`（x86 `0x517690`，函数界 `0x517690..0x518240`），数 `call` 进 `add_module` 单态化块（`0x4694f0 + i*160`, i<25）；跑 headless | 函数内 **22 条** `call` 打到该块，命中 **22 个不同**单态化地址（25 个里缺 idx 3/8/12，= 25 − 22，与"客户端不注册 3 型"数目一致）；`container_module_count()` = **22**、`container_real_manager_count()` = 17 + 5 空占位 | **数量一致**；**"逐型定名"无法判定**（要逐槽 vtable/TypeId 对应，本轮没做） |
| R2-13 | `0x456df0` 不是绘制；全 `.text` 唯一调用点 `0x4b6339`；`Plane::draw` 喂点后曲线画出 | `objdump -d` + 读函数体；跑 `curve_on_screen_check.gd` / `draw_classes_check.gd` | `0x456df0` 是 sret 取 `Vec<u32>` 的函数（hashbrown 控制字节 `pmovmskb` 迭代 + tick 指针作实参），**不是任何绘制调用**；直接 `call 0x456df0` **只有** `0x4b6339` 一条（另 1 条是 `jmp *0x456df0(%rip)`，内存操作数，与函数地址无关）；`CURVE_ON_SCREEN 96 passed` / `DRAW_CLASSES 235 passed` | **一致** |
| R2-14a | 挖洞点 = 爆炸点：401 份里 **503/503** 条 `DestroyObstacle` 与同批 `ExplosionEffect` **逐位**相同 | 自写脚本解 gzip JSONL：503 条 `DestroyObstacle`，按 `struct.pack('<d')` 比位 | `DestroyObstacle` 总数 = **503** ✅；**503/503** 的 `(pos_x,pos_y)` 能在**同一文件**的 `ExplosionEffect` 里找到**逐位相同**的一条（"最近的前一条爆炸"只对上 389/503，说明"同批"需要按批边界而不是按 tick 取——这不是反证） | **一致**（"同批"的配对规则我没复现，结论不受影响） |
| R2-14b | 收敛层挖洞点 **66/291 → 198/291** | `replay_attrib --drive conv --policy strict` + `bit_exact_attrib`；`hole_axis_attrib.py` 的前后两行 | 收敛层 = **66 / 291**（不变）；`198 / 291` 是工具自己标的「**改成**「挖洞点 = 爆炸点」**以后**」 | **不一致** → §1-B |
| R2-14c | 真实路径挖洞点 **1/259 → 139/258** | `8755db9e` 上 `--drive batch --policy strict` | 1/259（`f0ea313`）→ **139/258** | **一致（`batch` 轴）**；`fire` 轴是 138/258，报告没写轴 |
| R2-§2 | 死亡集合 **211/379 → 209/379**（来源 09，−2） | 各树 `bit_exact_attrib` 的延迟段 | 211/379（`f0ea313`）→ **209/379**（`8755db9e`）；master = **215/379**（括号修复 +6） | **一致（当轮）／现已过期** |
| R2-§1 | `cargo test --offline` **1519**；headless 8 脚本合计 **1047** | `cargo test`；8 个 headless 脚本逐个跑 | master = **1520 passed / 0 failed**（65 个测试二进制 + doc-tests = 66 条 `test result`）；`8755db9e` 的 `#[test]` 比 master 少 1 ⇒ 1519 与实测相容；headless = 72+117+235+155+183+132+96+57 = **1047** ✅ | **一致** |

## 3 无法判定（缺什么，写清楚不编）

| 项 | 缺什么 |
|---|---|
| R1-05 的 2.79 MB / 827 KB / 14 导出 | **交叉产物本身**（本机无 rustup target、无 mingw/NDK）；有产物后用 `objdump -p` 数导出即可 |
| R2-12 的"22 条**全部定名**" | 逐槽把 `add_module` 单态化 ↔ `dyn Any` vtable 的 `size` 字（`docs/02` §22.3 的方法）与 `TypeId` 常量对上；本轮只独立复算了**数量**（22/22 不同）与**运行时容器**（22 项、5 个空占位） |
| R1-07 的 242 / 318 | 计数口径（哪个正则、是否含子标题、是否只算带 `docs/16` 前缀的引用）；本轮三个自然口径都不是 242/318 |
| R1-01 的 86 复跑 | 需要 `6e9d639` 那棵树的 `gdext-bindings` 冷编 + 3 模块 `.so`；本轮只核到"当轮 `WIRED` = 3 项、静态 `ok(` 71 处（现 79 处）"，**没重跑当轮二进制**，所以 86 本身算"佐证相容"而不是"复现" |
| 空占位 5 型的**类型本身** | 与下游报告一致：`TickHandler`/`ClientConnectionHandler`/`PlanePosManager`/`FunctionManager`/`ObstacleManager` 仍无结构体 |

## 4 就地更正（本轮改了哪些文档）

1. `archive/155-round-log.md` §24.7 的结案注：`134/259` 与 `198/291` 两处加**审计注**
   （实测 139/258（`batch`）、收敛层仍 66/291，198 是反事实）。
2. `docs/14-turn.md` §12.5 表格同一行加审计注（同上），§12.7 第 2 步的 `134/259` 标注实测值。
3. 两份下游报告**没有改**：它们在 `CG` 分支的 `dispatch/` 下，master 上不存在，
   且本轮不改 `dispatch/`。→ 若有第三轮，任务书里请把 §1-A/§1-B 两条作为"已知错"发下去。

## 5 复现配方（照这个来，别信记录）

```bash
export CARGO_HOME=<WS>/.cargo-home
export CARGO_TARGET_DIR=<WS>/.agent-work/<你的>/target       # 私有，别串味
SNAP=<WS>/.agent-work/replay-snapshot                         # 401 份，gzip JSONL

# 三棵树（第一轮合并点 / 第二轮起点 / 第二轮合并点）
git worktree add wt-r1 6159280b ; git worktree add wt-start f0ea313 ; git worktree add wt-pre 8755db9e
# 每棵树各自: cargo build --release --offline --examples   （~20-30 s，不需要 gdext）

# 三条轴（数字必须带轴）
$T/replay_attrib --dir $SNAP --drive fire  --policy default --json fire.jsonl
$T/replay_attrib --dir $SNAP --drive batch --policy default --json batch.jsonl
$T/batch_conv    --dir $SNAP --drive batch --policy strict  --json bconv.jsonl   # = 收敛层
python3 tools/bit_exact_attrib.py --json fire.jsonl
# 种类序列：JSONL 里数 kind_seq_ok；内容：content_ok

# 二进制侧（地址是 ELF 虚地址）
objdump -d --no-show-raw-insn .reference/steam/2026-08-28/libgraphwarrust.so > x86.dis
llvm-objdump -d --start-address=0x4893f0 --stop-address=0x489440 \
    .reference/steam/android-2.0.60/libgraphwarrust.so        # end_x = pts[len-1]
grep -n "^  456df0:" x86.dis ; grep -n "call.*456df0" x86.dis  # 非绘制 / 唯一调用点
```

## 6 三条可带走的教训

1. **"→"是有罪的**：`66/291 → 198/291` 里的箭头把"反事实"写成了"已达成"。
   以后凡是预测值，必须写成 `若改成 X，则 Y`，并注明"未合入"。
2. **表头比数字更容易错**：1074 这个数**三次测量都正确**，错的只有它上面那一行的轴名。
   只看数字对不对，永远抓不到这种错——必须连"分母/驱动/口径"一起复算。
3. **一棵树，一个数**：`134/259` 与 `139/258` 都是"真跑出来的"，但跑在不同的树上。
   报数必须带 **提交号 + 驱动 + 口径**（本文每个数字都带了树名，就是为了避免这件事）。
