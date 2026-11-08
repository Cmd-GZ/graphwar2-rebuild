# 00 · 当前状态总览

> 本页是**唯一权威**的状态页。历史与已推翻的结论在 `archive/`，只作过程档案。

## 0. 诚实声明（先读这条）

1. **本目录只放长期有效的知识**，不放"本轮做了什么"。提交历史用 `git log` 看；
   `docs/archive/` 里是已被吸收的旧文，只为留证据轨迹与旧引用不失效。
2. **四档证据标记**（[`06-verification.md`](06-verification.md) §1）：
   **逐位验证**（与真二进制输出完全相等）/ **观测**（读到字节、伪代码、真帧）/ **推断**（写明缺什么）/
   **未知**（没有证据）。任何一条结论都必须能指到其中一档；合并文档时**标记原样搬运**。
3. **`client/` 是仿制品**：它照恢复出的 `.tscn` 与真机真帧重写，**不是原版源码**——
   原版的界面控制器是 Rust 模块，在 `.so` 里。所以"看起来对"**不是**复原证据。
4. **客户端自检条数是回归保护，不是复原证据。** 两个口径别混
   （**2026-11-08 审计**：本段原记"前置条件齐 = **694 / 0 / 0**"，现行已是 **709 / 0 / 0**
   —— 依据 `archive/156-client-rounds.md` §49.7 的"694 → 709（+15）"成对读数，**本轮审计未复跑 selftest**（需 Godot + 已编 `.so`），
   属**转引**；缺 `client/assets/` 的另外三档 `591 / 667 / 618` 是 2026-11-07 **当轮**读数，同样未复测（**未知**）。
   详见 [`18-build.md`](18-build.md) §10.3.1 与 README「总结」）：
   **前置条件齐 = 709 / 0 / 0**；缺资产时掉到别的数（当轮：`591 / 2 / 23`，补了 `serve_match` 时 `667 / 2 / 21`）。
   整局逐位的旧口径是 **71/400 帧完全一致**（[`06-verification.md`](06-verification.md) §21，**当轮**读数）；
   现行的三根驱动轴见 §0.4 顶部那张表。

## 1. 一句话状态

## 0.1b 「离完成还差多少」—— **四个维度分开看**（2026-11-08 晚，第八~十轮合并后）

> 只用 §0.2 那一个数（527/527）会被误读成"做完了"。下面四个维度的**分母完全不同**，
> 混着说必然虚报。数字都能复跑，命令见 `AGENTS.md`「合并纪律 B」。

| 维度 | 现状 | 分母 | 还差什么 | 可做性 |
|---|---|---|---|---|
| **① 函数身份** | **527 / 13,263 有归属（4.0%）** | Ghidra 函数总数 | 12,736 个**没有 `panic::Location`**，任何符号表都定不了名；结构指纹留出半 top-1 **76.4%（预注册 `scal=0.25`）/ 78.2%（并列桶最高）**，95% CI **[72.12%, 84.24%]**（n=165 ⇒ ±6 点）——**口径块见 `docs/02` §35 开头**（57 号把基线 72.7% 抬上来；`docs/160` 复核：数字成立、"新纪录"要打折），只给**同源候选**、不是名字 | 名字到顶；特征口径仍有洞（`consts` **上下界**都漏：`<0x100` 与 `>2^32`） |
| **② 名字** | **348 / 527 有原版真名（66%）** | 可归属函数 | 179 个无名：138 个给出推断、**41 个只能标未知**（其中 39 个在 Windows 侧本身也没名字） | 基本到顶 |
| **③ 行为一致** | **647 / 1080 批逐位相同（59.9%）** | 401 份快照的结算批（`fire` 轴 + `--policy default`） | 433 批失败按**归因确定度**分四档：**A 211**（值级互斥，已证与我方无关）· **B 210**（机制已定位、修法未验证：`end_x`/`pos_x` 两次读 205 + 死效块 5）· **C 12**（仍在三条开放线上：含 NaN 11 + 常数不符 1）· **D 0** | 可及上限 **775 / 1080**（悲观）· **869 / 1080**（乐观）；**211 批**已证与我方无关 ⇒ 天花板 **869**（还剩 **222 批**可争），**不是**"就在附近" |
| **④ 客户端** | 视图层已换 24 个原版脚本；**控制器仍是 GDScript 仿制** | 原版 Rust 控制器 | 控制器要迁回 Rust（`AGENTS.md` §0.4）；两个输入端（`pos_info_event` / graphics 51·53·55）已换真链路 | **最大的一块欠账** |

### 0.1b.1 ③「行为一致」那一行的三个界怎么来的（2026-11-09 实测）

> 口径：**`fire` 轴 + `--policy default`**（本快照上它与 `batch` 轴逐字节相同）。三条界都是
> 实测分档算出来的，不是估计；**不同轴的数不许放一起比**（`AGENTS.md`「合并纪律 C」）。

- **分档判据**（互斥、可加总：`433 = 211 + 210 + 12 + 0`）：
  **A** = 批内有「值级互斥」——回放**自己记的那个点**，用回放**自己记的** `corrected_function`
  （Normal 走 `y = f(x)`）或用我方逐位验证过的 RK4（DiffEq）**复算不出同一个 `y`**，
  且 `pos_x == end_x`（先把"两份数组"这条解释排除掉）；
  **B** = 没有值级互斥，但 `pos_x ≠ end_x`（`docs/06` §28：这两个字段是**两个装配点、两个时刻**的两次读），
  或死效块结构差（D 桶）；**C** = A/B 都不命中、落在三条开放线上；**D** = 以上都不是（实测 **0**）。
- **硬上界 = 1080 − 211 = 869**。A 档那一对 `pos_x`/`pos_y` 是回放**同时刻的一次 16 字节拷贝**
  （`docs/06` §37），`y ≠ f(x)` ⇒ 任何忠实实现都产不出这个点。背书两处：`docs/06` §33.3 的
  unicorn 单发复核；本次抽 60 发用真二进制求值器（`docs/06` §25.2 ④）——正对照 30 发 **30/30** 命中，
  407 里 28 发 **我方 28/28 与真二进制逐位相同**、其中 **18 发真二进制复现不出回放的 `pos_y`**。
  ⇒ A 档**永远不可能命中**，这就是天花板。
- **乐观界 = 647 + 210 + 12 = 869**：B、C 两档全中。它与硬上界**重合不是巧合**——D = 0，
  意思是"三条开放线之外没有别的批"。
- **保守界 = 647 + 128 = 775**：只数 B 档里 diff **全部**落在该机制负责字段上的批
  （`end_x`/`pos_x` 123 批 + 死效块 5 批）；B 档另有 82 批还带 `pos_y` 差，这套修法不负责，**不计入**。
  ⚠️ 这是"**若**修法成立"的上限、不是保证：`docs/06` §28 在 12 发新样本里**没复现出**两字段不等，
  触发条件仍未知。
- **旧说法更正（就地改，理由留档）**：本行原写"407 批已确认是回放自身不自洽 ⇒ 上限就在附近"。
  407 是 `docs/06` §25 的判决、经第三方 `docs/158` 独立复现，判决本身站得住；**但 `docs/06` §28 更晚**：
  `end_x` 与 `pos_x` 来自两个不同对象、两个时刻 ⇒ 只被"两字段不等"覆盖的 **145 批是机制、不是矛盾**，
  降为 B；`pos_x == end_x` 的那 262 批里又有 **189 批**只是末位差（1 ulp 级），
  正是 `docs/HANDOVER.md` §0.0.0.2f 里 66 号线（token 常数 +1 ulp）的靶子。
  另：旧文明细 "C 14 / E2 11 / D 5" 实测是 **C 14 / E2 7 / D 5**（合计仍是 26）。
- **条件上界**：若 66 号线成立（真二进制用的 token 常数确实 +1 ulp），A 里 Normal 侧那 **65 批**
  末位差要移进 C ⇒ A = 146、天花板 **934**；DiffEq 侧 129 批不受该线影响
  （`docs/06` §36：DiffEq 局 `corrected_function == function`，**没有常数**）。

复跑（在 `gw2-rebuild/` 根下；产物 `fire.jsonl` / `fold.jsonl` 留在同一目录）：

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<私有 target>
R=<401 份快照目录>
cargo run --release --offline -q --example replay_attrib -- --dir $R --drive fire --policy default --json fire.jsonl
cargo run --release --offline -q --example residual_fold  -- --dir $R --json fold.jsonl
python3 tools/bit_exact_attrib.py --json fire.jsonl --check-baseline --axis fire        # 桶表；护栏 7 项全一致
python3 tools/residual_fold.py --fold fold.jsonl --attrib fire.jsonl --table table.tsv  # 407 = 262 ∪ 218；正对照 647 批 0 误报
python3 - <<'PY'   # A/B/C 分档（判据见上；D 为 0）
import importlib.util as u
def mod(n, p):
    s = u.spec_from_file_location(n, p); m = u.module_from_spec(s); s.loader.exec_module(m); return m
B = mod("b", "tools/bit_exact_attrib.py"); F = mod("r", "tools/residual_fold.py")
fold = {}
for x in F.load_jsonl("fold.jsonl"): fold.setdefault((x["file"], x["batch"]), []).append(x)
n = {"A": 0, "B": 0, "C": 0}
for r in B.load("fire.jsonl"):
    if r["content_ok"]: continue
    ss = fold[(r["file"], r["batch"])]
    A = any((x["mode"] == "Normal" and x["pos_eq_endx"] and x["split_ok"] and x["has_expl"]
             and not F.pos_is_start(x) and not isinstance(x["pos_y"], str) and not x["self_hit"])
            or (x["mode"] == "DiffEq" and x["ode_found"] and not x["ode_hit"]) for x in ss)
    k = "A" if A else "B" if (B.classify(r)[0] == "D" or any(not x["pos_eq_endx"] for x in ss)) else "C"
    n[k] += 1
print(n)   # {'A': 211, 'B': 210, 'C': 12}
PY
```

#### 0.1b.2 **源代码还原**这条线还差什么（按"缺什么"分类，2026-11-09 第十三轮后）

> 上面那张表按**维度**看进度；这张表按**缺口性质**看 —— 两者别混：很多缺口不是"没干"，
> 而是**原版在这里没留证据**（`AGENTS.md` §0.3 的"未知"）。

| 缺口 | 具体是什么 | 缺的性质 | 可做性 |
|---|---|---|---|
| **客户端控制器** | `client/src/screens/*.gd` 仍有 **6,062 行**仿制品；67 号把弹道生命周期/回合超时/AI 闸门搬进 Rust（`play_screen.gd` 只降 12 行） | **缺工程**（证据大多已有） | ✅ 可做，但量大 |
| `ShotLifecycle` 的文件归属 | 原版在 `game_state_manager`，我方落在 `play_screen_control` | **缺边界证据** | 可做（需按 `Location` 再切一次） |
| 收敛层 `diffconv` 未跟 S3 | 内核按到达序逐发挖洞了，收敛层没有 | 缺工程 | ✅ 已派（73 号） |
| `drive_frame` 接线 | 没有真帧可对；接上会静默改 `EntityDrawer` 的 `V` | **缺真值** | 暂缓 |
| 弹道"打不全就复原" | 与内核 `should_reset_targets` 条件不一致 | **缺样本**（需要"部分命中"的回放，397 份里 0 份） | 卡住 |
| 联机 / 回放驱动 / 关卡坐标 | 未做 | **缺证据**（部分是我方自定） | 卡住 |
| 12,736 个无 Location 函数 | 只能到"结构同源候选"（**76.4%（预注册 `scal=0.25`）/ 78.2%（并列桶最高）**，95% CI [72.12, 84.24]；口径块见 `docs/02` §35 开头）；角色推断 40.5% < 平凡基线 44.3% | **信息天花板** | 到顶 |
| 4 个 manager 仍是 `assembled` | 只构造注册、没接事件流（11 wired / 2 layout / **7 assembled**） | 缺工程 | ✅ 可做 |
| 美术的**源文件**（`.svg`/`.ttf`） | 包里**只有产物**：167 `.ctex` + 7 `.fontdata`（导出时源资源被剥离） | **物理上拿不回**（光栅化不可逆） | 不可做；但**图本身能用**，见下 |

**另外两个不要忘的口径**：

- **平台**：Linux 已验；Windows/Android 可构建；**macOS 未验**（没有环境）。
- **资产**：要分清两件事 —— **图能用**（`.ctex` 内部就是 **WebP**，实测首字节 `GST2…RIFF…WEBP`，
  我们的 `client/assets/` 已放 **142 个 `.ctex`** 并在跑，见 `dispatch/37` 的 texture-loader）；
  **源拿不回**（`res://…svg` / `.ttf` 是 Godot 导入时的**源**，导出后只剩产物，光栅化不可逆）。
  ⇒ 只有"想要 SVG 源文件"时才需要描摹；**展示/绘制用直接解码 `.ctex` 即可**。

## 0.2 进度尺（2026-11-07 深夜，**这才是"还原了多少"**）

> 口径：**原版可归属函数里，我们还原并验证了多少**。不用测试条数当进度（见 `AGENTS.md` §0.2）。
> 复现：`python3 tools/coverage_matrix.py`（`--check` 只校验不重写）。

```
原版可归属函数 527 个，分布在 78 个模块（58 个 graphwar_*.rs 占 430、16 个 scope=other 的模块占 97）
   已实现 527（逐位 88 / 观测 439 / 推断 0）｜ 未做 0 ｜ 完全未知模块 0
归属率 527 / 13263 个 Ghidra 函数 = 4.0%（硬上限：纯计算函数没有 panic 就没有锚）
```

> **2026-11-07 夜更正**：这段长期写着 `已实现 281（逐位 98 / 观测 183）｜ 未做 161 ｜ 完全未知模块 22`，
> 与 `docs/coverage-matrix.tsv` 实测**不符**（该表 77 个模块行全部是 `观测`/`逐位`，`未做` 为 0）。
> 分子 281 是 2026-11-06 21:27 那一版的值、此后再未更新（分母 527 在 `ea287fc` 已改对，分子没跟上）。
> 现按 `python3 tools/coverage_matrix.py` 重算结果更正，与 `--check` 一致。

⚠️ **527 是可度量下限，不是模块规模**——求值器 `0x4eb9d0`、采样器 `0x3c2530` 全函数体零 Location 引用。

### 0.2.1 分母在 2026-11-07 深夜被**改大**过（不是进度倒退）

旧的 `488 / 70 个模块` 是错的，两个原因，都来自 2024 版未 strip 的符号表（`docs/02` §18）：

1. **漏记 7 个 crate 内模块**：`backtracer` / `function_token` / `graphwar_event_queue` /
   `graphwar_game_info` / `graphwar_player_config`(8 函数) / `graphwar_replay_reader` / `graphwar_stream_buffer`；
2. **`weak_caller` 分类错误**：旧文档判它是"第三方依赖、不计入总账"，
   但符号表里有 `graphwarrust::weak_caller` 的 27 个 direct 符号 ⇒ 它是 **crate 内模块**，
   那 39 个可归属函数应当计入 ⇒ 分母 +39。

⇒ `527/527` 是当前口径（改分母那次之后，分子在 2026-11-06 深夜也补齐了）；
更早的 `281/488 = 57.6%`、`281/527 = 53.3%` 都已作废——它们统计的是同一份代码的**半成品状态**。

## 0.3 本轮（2026-11-07 夜，8 份任务书）新增

| # | 分支 | 产出 | 档位 |
|---|---|---|---|
| 01 | `agent/assemble-min` | `_scaffold/root_wiring.rs`：`GraphwarModuleContainer` 装载器（`TypeId`→`Rc<RefCell<M>>`）+ **真装配 3 个 manager**；Setup/Game 两屏、Game 屏挂 Plane + EntityDrawer + 3 假实体 | 观测（86 条 headless 断言） |
| 02 | `agent/function-drawer-node` | `FunctionDrawer` 成真节点；`update_function` **增量追加**（132 条断言）；顺带更正 §32 的 map value 形状（40 B 三元组）与宽度常量 `0.125`（`.rodata 0x1baff8` 逐位） | 观测 + 逐位（常量） |
| 03 | `agent/bit-exact-report` | 整局对拍剩余失败**归因表**（401 份回放，失败 512 批全部落桶）：爆炸点 y 末位差 170 / 多发批结构 123 / end_x 差一点 95 / …；**否证** "A 类熵是主因"（0 批） | 观测（`--check-baseline` 7 个数全一致） |
| 04 | `agent/campaign-data` | 21 关参数表落进 `graphwar_campaign_control::LEVEL_INFO`，客户端 PLACEHOLDER 清零（13 条测试，21×7 逐字段） | 观测（数字与 118 §7.1 逐行相同） |
| 05 | `agent/crossbuild` | Win x86_64 / Android arm64 **check+build 都成立**（便携 llvm-mingw + NDK）；修 2 处"编得过但静默做错事"（`/dev/urandom`、`$HOME` 拼路径） | 观测（wine 入口探针 PASS） |
| 06 | `agent/name-match-review` | 唯一匹配 733 → 762；多候选 63 → 37；444 无候选归因（`icf 0 / tiny 172 / other 266 / inlined 6`）；修掉"夹具无法复现" | 观测 + 推断（每条带 `arbi=`） |
| 07 | `agent/docs-renumber` | `docs/16` 编号 1..40 连续（242 处标题）、318 处交叉引用、`tools/check_doc_refs.py` 退出码 0 | 观测（脚本可重跑） |
| 08 | `agent/viewport` | 105 个原版 `.gd` 清点：**40 可直接换 / 5 等装配 / 60 不能换**（52 游戏 + 53 插件）；最小试验 57 断言 | 观测（headless 实测） |

## 0.4 下一轮最高性价比（按"能不能指到证据"排）

> **现行值（2026-11-08 凌晨实测；唯一权威数字源 = `fixtures/guardrails.json`）**：
>
> | 指标 | `fire` | `batch` | `batch_conv` |
> |---|---|---|---|
> | 内容命中 | **647 / 1080** | **647 / 1080** | **633 / 1080** |
> | 种类序列 | **1075 / 1080** | **1075 / 1080** | **1075 / 1080** |
>
> 延迟段（`batch` 轴）：死亡集合 **366 / 385**（漏报 0）、挖洞点 **207 / 294**。
> 复跑：`README.md`「对拍现在多准」那三条命令（`replay_attrib --drive fire|batch --policy default`、
> `batch_conv --policy strict`）。
> ⚠️ **分母是 1080 批**（`guardrails.json` 的 `batches`；`replay_attrib` 里 `batches_total` 逐批 +1，
> `content_ok` / `kind_seq_ok` 也是逐批计）。交接稿里 `647/1075` 这种连写是
> **"内容 647、种类序列 1075"两项并列**，不是分数——**别当成分母 1075**。
> **审计 2026-11-08**：本节原表是 2026-11-07 深夜的**当轮**读数
> （内容 325 / 346 / 595、种类序列 819 / 919 / 1074、死亡 215/379、挖洞 140/262），已按现行值替换；
> 那一轮的数保留在 `archive/155-round-log.md` §24–§29、§39–§41（当轮日志，不删）。
> ⚠️ 下面第 1、3 条的归因数出自 `archive/155-round-log.md` §24 的**当轮**读数（`conv` 轴，那时内容命中 568/1080
> ⇒ 失败 512 批）；**现行 `batch_conv` 内容命中 633 ⇒ 失败 447 批**，百分比要按现行分母重算。

> ⚠️ **这次审计改了什么**（2026-11-08，详见 [`154-docs-audit.md`](154-docs-audit.md)）：
> ① 本页 §0 的自检条数 694 → **709**；② §0.2.1 的"327 个真名" → **348**
> （`archive/157-name-matching.md` §37–§40 的三表并集口径）；③ §0.4 三轴表按 `guardrails.json` 换新，并把
> 口径写清（`647 / 1075` 连写 = 内容 647 **与** 种类序列 1075 两项并列，分母都是 1080 批）；
> ④ §2「协议」行 73/73 + 72 → **74/74 + 74**（`docs/07` §5.1/§6）；
> ⑤ §2/§3/§4 里 10 条指错文档的链接（标签是 `16`/`12`/`15`，目标却成 `06`/`13`/`05`）已改回。

1. **爆炸点 y 的末位差**（`archive/155-round-log.md` §24 当轮：170 批，占该轴失败 33.2%）——三个假设已被数据否掉，
   机制仍**未知**（`archive/155-round-log.md` §25/§26），不是"改个常量"就能修；
2. ~~**多发批结构接线（123 批）**~~ **✅ 已完成**：`docs/141` 的分组已接进驱动与收敛层
   （原记"预计 818 → 918"）；**当轮**复算 `batch` 种类序列 **919/1080**、`batch_conv` **1074/1080**
   ⇒ **现行两条轴都是 1075/1080**（审计 2026-11-08，见上表）；
3. **挖洞点轴**（`archive/155-round-log.md` §24 当轮：延迟段 271 批里 200 例我方点在该回放的任何 `DestroyObstacle` 里都不存在；
   **现行分母是 294、命中 207**，2026-11-08 审计按 `guardrails.json` 更正，原记"262"是当轮值）——机制**未知**，先定性；
4. **`EffectInfo`(tag44) 的生产者**——全 `.text` 只有 2 处 `movabs` 且都是消费者，需换路子找（调用点/槽）；
5. 剩余 19 个 manager 的装配，与 `PlayScreenControl` 节点化（01 只做了 3 个 + 替代形状）。

## 0.5 源码目录约定（2026-11 起，**看代码前先读这条**）

`src/` 的文件名现在**就是原版模块名**，所以读代码不用再猜对应关系：

* `src/graphwar_*.rs` = 58 个原版 `graphwar_*.rs` 之一（如 `graphwar_turn_manager.rs`）；
  一个原版文件被我方拆成两份时，主文件用原版名、第二份进同名目录
  （`graphwar_shot_calculator/settle.rs`、`graphwar_level_generator/tables.rs`、
  `math_function/to_text.rs`）；
* `src/entity_drawer.rs` = 12 个非 `graphwar_` 游戏模块之一；
* `src/events.rs` / `game.rs` / `plane.rs` / `server.rs` / `turnstate.rs` = **混装**了多个原版模块、
  边界尚未证实，按 `AGENTS.md` 铁律 2 原地保留并登记在待拆表里；
* `src/_scaffold/` = **我方自造**的桥接/工具（GDExtension 入口、C ABI、报文装配、对拍器、
  ChaCha12、gzip、broker 服务端）——原版没有这些东西，目录里有 `README.md` 逐条说明。

完整对照表（78 个模块逐行 + 状态 + 依据）见 [`02-module-map.md`](02-module-map.md) §17；
**模块的权威清单（78 模块 / 165 类型 / 290 方法名）**见同文件 §18；
**527 个可归属函数里 348 个有原版真名**（并集 351 个地址，其中 6 个不在 527 内；无名 179 个
= 138 推断 + 41 未知）——名字 ↔ x86 地址见 `fixtures/name_to_addr.tsv`（§20 是**当轮**的
327 条单表口径，现行口径见 `archive/157-name-matching.md` §37–§40 的三表并集）。


内核（函数语言 / 弹道 / 掩膜 / 命中 / 协议 / broker / 回合与生命 / 服务端 / 关卡 / 战役数据 / AI 参数）
已复原到"逐位验证"或"强观测"；`client/` 里 **15/15 界面**能启动、能开一局，弹道与命中走内核，
并且**回放播放器、联机层、音频播放层、`token → 文本` 都已经接上了**。
仍做不到的是 [`06-verification.md`](06-verification.md) §17 那几项**被证明做不到**的逐位一致
（两类非确定性、墙钟、渲染管线）。

## 2. 已完成（可执行 + 有验收）

| 部件 | 代码 | 验收 | 文档 |
|---|---|---|---|
| 函数文法 / 求值 / token→文本 | `src/math_function.rs`（+ `src/math_function/to_text.rs`） | 90 例文法 + 37/70 条真值夹具；`to_text` 1032/1032 | [`03`](03-function-language.md) |
| 等弦长采点（含 RK4） | `src/graphwar_path_calculator.rs` | 224 条真值夹具 + **oracle 24/24 逐点逐位相等** | [`04`](04-path-sampling.md) §9 |
| 修正常数 + 开火原语 | `src/graphwar_shot_calculator.rs` | 端到端冒烟 + 上游 648 例对拍 + 装配校验 15 组 | [`05`](05-shot-pipeline.md) |
| 坐标 / 掩膜 / 光栅化 | `src/plane.rs` | 与真二进制**逐字节 / 300 随机点**一致；摧毁 = 挖 `r=0.5` 洞 | [`08`](08-plane.md) §10 |
| 弹道与命中结算 | `src/graphwar_shot_calculator/settle.rs` | 整关回放致死集合逐发一致；实况 0.01 下 82 发并集 **152/152** | [`14`](14-turn.md) §6 |
| 命中判定暴露给引擎 | `src/_scaffold/api.rs` | `hit_test` 与 `resolve` **同一个函数体**（无第二份实现） | [`16`](16-client.md) §4 |
| 事件 / 生命 / 回合状态机 | `src/events.rs`、`src/graphwar_life_manager.rs`、`src/turnstate.rs` | 6 状态 7 事件，**13 次迁移逐个相同** | [`14`](14-turn.md) §2 |
| 协议 + 编解码 | `src/_scaffold/protocol.rs` | **74/74 事件全字段**；判别值 **74 条**（72 条来自 `cmp` 相等测试链 + `63`/`65` 由 arm64 跳转表定死） | [`07`](07-protocol.md) §5.1、§6–§7 |
| broker（房间列表） | `src/_scaffold/broker.rs` | 12 条测试；`VersionInfo` → `GameInfo` 线格式与真实抓包一致 | [`09`](09-network.md) §6 |
| 服务端（握手 / Match / 一回合 / 整局循环） | `src/server.rs`、`src/game.rs`、`src/graphwar_game_server.rs` | 与抓包逐字节/逐条一致；真实 TCP 跑到 `EndingGame` | [`09`](09-network.md) §3–§5 |
| tick 时钟 | `src/graphwar_tick_handler.rs` | 113 个真实样本；`TickReply` 逐字节一致 | [`14`](14-turn.md) §1 |
| 关卡生成 | `src/graphwar_level_generator.rs` | 散布模式**逐位复现 13/13**；中心模式 4 个真值样本 | [`11`](11-levelgen.md) |
| 战役 21 关数据 | 数据在二进制；全表见 [`10`](10-campaign.md) §4 | 21/21 标题、21/21 描述、每关参数；**385 份回放交叉验证 21/21** | [`10`](10-campaign.md) |
| AI（进化） | `src/graphwar_function_evolutionator.rs` | 参数/权重/长度/装配/变异/每代构成全部还原；终止 = 代数 ∨ **25 s 墙钟** | [`13`](13-ai.md) |
| GDExtension 入口与构建 | `src/_scaffold/gdext.rs` | 真 Godot 4.7.2 里注册成功；`.pck` 能从包启动 | [`18`](18-build.md) |
| 客户端 15 界面 + 内核接缝 | `client/` | `bash client/run.sh selftest`（现行 **709 / 0**，转引 `archive/156-client-rounds.md` §49.7）；弹道/命中/障碍都走内核 | [`16`](16-client.md) |

## 3. 明确未知 / 已证不可复现

| 缺口 | 状态 | 文档 |
|---|---|---|
| 回合出场顺序 / 开局那批广播的顺序 | **A 类**：来自 `HashMap` 迭代序（`getrandom(16)` 播种）。钉住熵后跨进程一致，连 ChaCha 种子一起钉则 **345/370 帧逐字节相同** | [`12`](12-rng.md) §7 |
| 25 s 墙钟截断 / 多客户端竞态 | **B 类**：本质不由输入决定，只能定退化口径 | [`06`](06-verification.md) §17 |
| 目标「打不全就全部复原」的**判定点** | **未知**：形状已钉住（只能翻 alive + 广播 `LifeInfo`；不重建实体、不恢复障碍洞），但哪一刻、什么条件没找到；397 份回放里 **0 次部分命中** | [`15`](15-target-life.md) §4 |
| 音效 ↔ 事件的映射 | **未知**：播放链路、音量、静音都逐位；"哪个事件放哪个音"证不到 | [`17`](17-resources.md) §5 |
| ~~通视检查 `0x567530`~~ → **`calculate_paths`（已定名）** | `0x567530`(Ghidra) = `0x467530` = `GraphwarPathCalculator::calculate_paths`（9 个 Location 锚；旧文档的"通视检查"只是语义猜测）。**对局不用**：17 个直接调用点全部是关卡构造（`setup_level_N` ×16 + `generate_level` ×1），`game_state`/`shot_calculator`/`game_server`/`computer_player_manager`/`plane` 零调用；函数指针三条取址形态（RELATIVE 槽 / `lea` / `imm64`）全 0 命中。⇒ **弹道与命中不需要实现它** | [`08`](08-plane.md) §5.6–5.7 |
| 掩膜的持有者与生命周期 | 未知（带锁结构，不知谁在何时重建） | [`08`](08-plane.md) §7 |
| 段位怎么算 | `rank_manager` 里算不出来（无除法/取模/阈值表）；`RankInfo` 字段数 2 vs 3 **未收敛** | [`02`](02-module-map.md) §15 |
| 弹道用什么原语画 | `draw_polyline` 字符串在 `.so` 里根本不存在 | [`16`](16-client.md) §12 |
| 跨架构逐位一致（Windows/Android 内核） | **未验证**，不承诺 | [`19`](19-android.md) §6 |

## 4. 交付侧：离"可玩 / 可验收"还差什么

| # | 项 | 现状 | 文档 |
|---|---|---|---|
| 1 | 回放播放器 | ✅ 已实现（按 tick 推进 + 内核重算弹道）；**播放控件是自定的** | [`16`](16-client.md) §10 |
| 2 | 联机层 | ✅ 客户端能真的连服务器；**用的是我们自己的服务端**，界面层仍有本地分支 | [`16`](16-client.md) §11 |
| 3 | 音频播放 | ✅ 播放层通了（含 `humm` 的 generator 路径）；**事件映射仍是未知** | [`17`](17-resources.md) §4 |
| 4 | AI 产物上线 | ✅ `token → 文本`（`0x4ec160`，1032/1032）已接进服务端 | [`13`](13-ai.md) §22 |
| 5 | 战役数据 | ✅ 21 关真数据已接进界面 | [`10`](10-campaign.md) §11 |
| 6 | 界面保真 | 逐屏残差多数 < 0.5%，`EndGamePopUp` 仍 ~9.8% | [`16`](16-client.md) §6 |
| 7 | 目标「打不全就复原」 | 实现了，但标 **推断**（判定点未知） | [`10`](10-campaign.md) §9 |
| 8 | 两类非确定性 | A 类缺一份未捕获的熵、B 类只能退化口径 | [`06`](06-verification.md) §17 |

## 5. 文档地图（读这一屏就够）

| 想知道什么 | 看哪里 |
|---|---|
| 全部文档的索引 | [`INDEX.md`](INDEX.md) |
| 压缩上下文后怎么接着干 | [`HANDOVER.md`](HANDOVER.md) |
| 怎么证明复原是对的 / 四档定义 / 哪些做不到 | [`06-verification.md`](06-verification.md) |
| 一次射击从输入到死亡 | [`05`](05-shot-pipeline.md) §7 |
| 模块 ↔ 函数对照 | [`02-module-map.md`](02-module-map.md) |
| 文档怎么重组的（迁移表） | [`MIGRATION.md`](MIGRATION.md) |

## 6. 已知的方法学陷阱（都踩过）

1. 反编译里的 `_DAT_xxxx` 是 **Ghidra 地址**，读原始字节要减 `0x100000`。
2. 把**虚地址**当 Ghidra 地址传给工具会得到 `not_found`；REA 甚至会把非入口地址当**文件偏移**返回一个无关函数（[`03`](03-function-language.md) §12 有量化版）。
3. `decompile_all` 的文件头含 `Ghidra 12.1.4`，直接 grep 浮点常量会被 `12.1` 污染。
4. 用途要靠**调用关系**定，不能凭函数内部特征猜。
5. 上游 `funcstarts.pkl` 是坏的（见 [`02`](02-module-map.md) §12），用 `funcstarts_ghidra_va.pkl`。
6. **调用图会骗人**：Rust 大量用函数指针派发，直接调用为零**不代表**没被使用（[`06`](06-verification.md) §12）。
7. **先查权威常量表再写代码**：`step` 曾被硬编码成 `0.5`，4 轮分析作废（[`06`](06-verification.md) §22）。
8. **Godot 的 `.gdextension` 与新增脚本要 `--import` 一次**，否则"代码写对了但没加载"（[`16`](16-client.md) §3）。
9. **口述要先落到格式判据上**：音频那次"包里有文件损坏"的真凶是使用者自己重封装的文件（[`17`](17-resources.md) §3）。
10. **hook libc 只能看到走 libc 的调用**：Rust 的 `getrandom` crate 走 syscall 318（[`12`](12-rng.md) §9）。
