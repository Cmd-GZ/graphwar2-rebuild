# 任务书（分发用）

这个目录是**派发给别的机器/agent 的任务书**。它不是产品文档，不进 `docs/` 索引。

基线：`master` @ `a8788b61`（**第四轮起点**）。第一轮 8 份任务已全部合并进 master；
CG 分支上保留任务书与执行报告（`dispatch/reports/CG-2026-11-07.md`）。
当前回归：`cargo test --offline` **1512** / `--features gdext-bindings` **1583**。

## 所有任务共用的契约

### 仓库与分支

```bash
git clone git@github.com:Cmd-GZ/graphwar2-rebuild.git
cd gw2-rebuild
git checkout -b agent/<任务名> origin/master
```

干完推到 `origin`，**不要直接推 `master`**，也不要推 `CG` 本身——由父会话合并。

⚠️ **分支前缀必须是 `agent/`，不能是 `CG/`。**
2026-11-07 实测：远端已存在分支 `CG`，而 Git 不允许同时存在 `CG` 与 `CG/xxx`
（`cannot lock ref ... 'refs/heads/CG/xxx' exists; cannot create 'refs/heads/CG'` 的反向同样成立）。
⇒ 本目录早先写的 `CG/<任务名>` 已全部改成 **`agent/<任务名>`**。

### 环境（这台机器上已经跑通的写法）

```bash
export CARGO_HOME=<workspace>/.cargo-home          # 本机 ~/.cargo 只读
export CARGO_TARGET_DIR=<workspace>/.cargo-home/target   # ← 必须共享，别放克隆里
GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new"
```

⚠️ **`CARGO_TARGET_DIR` 一定要指到克隆之外**。之前有 agent 把 target 建在克隆里，
每个克隆 11–12 GB，三个并行就堆了 33 GB。

### 硬性验收（每个任务都要过）

```text
cargo test --offline                 # 基线：lib 457 / 总计 1499，必须全绿
不许改、不许删既有断言
不许新增 docs/*.md
不许碰 docs/INDEX.md、docs/HANDOVER.md
改过 src/ 就重跑：python3 tools/coverage_matrix.py   并一起提交两个生成物
```

### 二进制不在 git 里

做逆向的任务自己备（**不要提交进仓库**）：

| 文件 | 用途 | 怎么来 |
|---|---|---|
| `2026-08-28/libgraphwarrust.so` | x86-64 目标（stripped） | Steam depot **3066841**（Linux），或从原机拷 `.reference/steam/2026-08-28/` |
| `android-2.0.60/libgraphwarrust.so` | **未 strip**、与目标同版本的 arm64，所有名字都来自它 | 原 APK 的 `split_config.arm64_v8a.apk` → `lib/arm64-v8a/` |
| capstone | 反汇编 | `pip install capstone`（本机用 `../.venv/bin/python`） |

地址口径：**Ghidra/REA 地址 = ELF 虚地址 + `0x100000`**。忘了会静默读到垃圾。

### 汇报格式（≤8 行）

分支 + sha、做了什么、**数字**（不是"完成了"）、踩的坑、未决项、`cargo` 条数。

---

## 任务清单

| # | 分支 | 主题 | 优先级 | 需要二进制 |
|---|---|---|---|---|
| 01 | `agent/assemble-min` | RootNode 真装配 3–5 个 manager，跑出棋盘 + 士兵 | **P0** | 是 |
| 02 | `agent/function-drawer-node` | `FunctionDrawer` 节点化（补上 12 类之外的唯一缺口） | **P0** | 是 |
| 03 | `agent/bit-exact-report` | 整局对拍剩余失败的归因分类 | P1 | 是（回放语料） |
| 04 | `agent/campaign-data` | 21 关参数表落码 + 接客户端 | P1 | 否 |
| 05 | `agent/crossbuild` | Windows / Android 编译验证 | P2 | 否 |
| 06 | `agent/name-match-review` | 63 个多候选人工定夺 + 444 无候选分类 | P2 | 是 |
| 07 | `agent/docs-renumber` | `docs/16` 的 §19/20/23/34 空号与交叉引用 | P2 | 否 |
| 08 | `agent/viewport` | 用原版 `.gd` 换视图层的可行性调研 | P3 | 否 |

## 冲突矩阵（并行前先看这个）

`src/_scaffold/` 是热点，只有 01 和 02 会动它：

```text
01 assemble-min          改 src/_scaffold/graphwar_root_node_class.rs + 新增 root_wiring.rs + mod.rs
02 function-drawer-node  新增 function_drawer_class.rs + mod.rs

⇒ 两者都要改 mod.rs（只是加一行），git 能自动合并；
  但 01 会改 root_node_class.rs，02 不碰它 ⇒ 可以并行，合并时按"先 01 后 02"顺序即可
```

其余六个任务**互不重叠**：

```text
03  tools/ + examples/（只加不改）+ 文档追加
04  src/graphwar_campaign_control.rs + client/src/app/campaign.gd
05  Cargo.toml + 必要 #[cfg]
06  tools/sym_match.py + fixtures/sym_match.tsv
07  docs/16-client.md 的标题编号（不改内容）
08  client/tools/（新增）+ 文档追加
```

⚠️ `07` 只改**标题编号与交叉引用**；`08` 只**新增**脚本。两者都不该碰正文。

---

# 第二轮（2026-11-07 晚）

第一轮的结论见 `dispatch/reports/CG-2026-11-07.md` —— **它推翻了任务书里的四个前提**，
第二轮的任务就是按那份报告的真实归因写的。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据（第一轮实测） |
|---|---|---|---|---|
| 09 | `agent/fx-explosion-y` | 爆炸点 `pos_y` 末位差 | **P0** | 失败 **170/512 = 33.2%**，单项最大；修好预计 56.3% → **68.9%** |
| 10 | `agent/multi-shot-wiring` | 多发批结构接线 | **P0** | **123 批**；`diffconv` 里已修好，只差接进驱动；预计 818 → 918 |
| 11 | `agent/endx-sample-point` | `end_x` 差一个采样点 | P1 | **95 批**；机制已实测（回放 `end_x` 命中我方 `pts[i-1]`） |
| 12 | `agent/assembly-22` | 装配从 3 个扩到 22 个 manager | P1 | 只观测到"调用序相同"，**逐型证据缺** |
| 13 | `agent/curve-on-screen` | 让屏幕上**真的画出曲线** | P1 | 两个断点：`Plane::draw` 没接 `0x456df0`、没人喂点给 `FunctionDrawer` |
| 14 | `agent/hole-axis` | 挖洞点轴定性 | P2 | 200 例"我方挖的点回放里不存在"——机制问题 |

## 第一轮学到的两条（**必须写进这一轮的做法里**）

### ① `CARGO_TARGET_DIR` 的取舍反过来了

第一轮 README 写"必须共享 target"。**并行做任务时那样会串味**——
下游见过一次假失败（`cannot find ObstacleMode`）。

```text
顺序做      可以共享 <WS>/.cargo-home/target（省 30 GB）
并行 worktree  每个 worktree 一个私有 target
```

判据：一次只编一个人用共享；两个以上同时编，各用各的。

### ② 验收必须能从干净树重跑

下游抓到：某份任务"报全过"，但按它给的命令重跑报 `Could not find type "GraphwarButton"`——
真因是工作树里留着**手写绑定**的旧 `.so`（只注册 2 个类），不是 `--features gdext-bindings` 那份。

```text
⇒ 每份任务书的验收里必须包含"重建 .so + --import"这两步，
  并在报告里给出可复现的命令序列；不许只贴"我跑过了"。
```

### ③ 任务书里的"已知"可能是推断

第一轮有四个前提被推翻（没有颜色常量、A 类熵为 0 批、ICF 假设不成立、分支前缀不可用）。
⇒ **第二轮任务书里凡是标"已知"的，都注明了证据档**；读到没标的，默认当"待验证"。

---

# 第三轮（2026-11-08 凌晨）

第二轮的验收报告在 `dispatch/reports/CG-round2-2026-11-07.md`。**收益很大**：

| 指标 | 第二轮前 | 第二轮后 |
|---|---|---|
| 内容命中（`fire` 驱动 = `src/game.rs` 真实实现） | 21 / 1080 | **324 / 1080** |
| 内容命中（`batch` 驱动） | 21 / 1080 | **341 / 1080** |
| 种类序列 | 946 / 1080 | **1074 / 1080** |
| 延迟段挖洞点 | 1 / 259 | **139 / 258** |
| 22 条 `add_module` | 0 定名 | **全部定名**，容器 3 → 22 |

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 15 | `agent/eval-unicorn` | **unicorn 对拍求值器，解最后 1 ulp** | **P0** | 报告 §8 第 1 条：唯一"工具现成、只差执行"；09 的 154 批 + 11 的残差同源 |
| 16 | `agent/diffconv-merge` | 把 `diffconv` 的坐标计算并回内核 | P1 | 报告 §5 点名的跨任务结构债，已出现过不一致 |
| 17 | `agent/path-event-producer` | 追 `calculate_paths` 一侧的事件生产者 | P1 | 报告 §8 第 3 条：同时解释"撞什么算撞"与"一次挖洞发 2 条" |
| 18 | `agent/plane-in-client` | 把真 `GraphwarPlane` 接进客户端播放屏 | P2 | 13 号把曲线画出来了，但只在 headless 里 |
| 19 | `agent/manager-behaviors` | 22 个 manager 的**行为**（本轮只做 5 个） | P2 | 装配已到 22 个，但容器里都是空表 |

## 这一轮的注意点（第二轮踩到的）

1. **任务书里的"已知"仍有假**：第二轮 5 条前提被证伪（`FunctionDrawer` 无颜色常量、
   A 类熵 0 批、ICF 假设不成立、`CG/` 前缀不可用、`end_x` 差一个采样点不成立）。
   ⇒ 读到"已知"先看它有没有标证据档；没标的一律当**待验证**。
2. **别信"我的实现 vs 我的另一份实现"**：`diffconv` 就是这种债（16 号任务专门修它）。
3. **数字护栏**：任何改动后这三个数不许退 —— 内容命中 **324**、种类序列 **1074**、挖洞点 **139/258**。
   退了就是改错了，不是"另一种口径"。

---

# 数字护栏（**2026-11-07 复核后重写，旧的 1074 那一行是错的**）

下面的数都是**合并后 master 上实测**的，同一份 401 份定格快照。任何改动后**不许退**。

## 三条轴要分清楚（这就是上一轮报错的根源）

| 驱动 | 是什么 | 命令 |
|---|---|---|
| `fire` | 每发一次 `Match::fire` —— 与 `src/game.rs` 真实实现同口径 | `replay_attrib --drive fire --policy default` |
| `batch` | `Match::fire_batch` —— 一个批产出一整个结算批 | `replay_attrib --drive batch --policy default` |
| **`batch_conv`** | **收敛层** `ConvPredictor::fire_batch` | **`batch_conv --policy strict`** |

⚠️ **上一轮把收敛层的「种类序列 1074」写进了 `batch` 那一行**，导致我这边怎么测都是 915/919。
**1074 是真的，但它属于 `batch_conv` 轴**（已逐数复现：多发批 128/130 + 单发批 946/950 = 1074）。
**报数字时必须写清是哪条轴。**

## 护栏值（master `51ddfd87` 实测）

| 指标 | 值 |
|---|---|
| 内容命中（`fire`） | **325 / 1080** |
| 内容命中（`batch`） | **346 / 1080** |
| 种类序列（`fire`） | **819 / 1080** |
| 种类序列（`batch`） | **919 / 1080** |
| 种类序列（`batch_conv`，收敛层） | **1074 / 1080** |
| 内容命中（`batch_conv`） | **595 / 1080** |
| 延迟段死亡集合 | **215 / 379** |
| 延迟段挖洞点 | **140 / 262** |
| `cargo test --offline` | **1520** 全绿 |

## 另外三条纪律（这轮踩出来的）

1. **报数字必须附可复跑的命令 + JSONL 路径**。只给数字不给复现路径的，一律当未验证。
2. **负结论要有正对照**：说"扫不到函数指针"之前，先用已知槽（如采样器的 `0x9bac98`）证明你的扫法有效。
3. **改完必须给四件套**：`cargo test`（两条 feature）、`coverage --check`、
   **上面这张表里的数**、`check_doc_refs.py`。

---

# 第四轮（2026-11-08）

第三轮报告在 `dispatch/reports/CG-round3-2026-11-08.md`。**它把两个我们以为"快解开了"的东西推翻了**：

1. **"1 ulp 残差在求值器"是错的** —— unicorn 直调 `0x4ed670`/`0x4eb9d0`，1290 发里
   `eval_differ = 0`、`const_differ = 0`。**求值器已逐位排除。**
2. **"`EffectInfo` 的生产者查不动"是扫描方法的问题** —— tag 有**三种** materialize 形态
   （`movabs` 直写 / `i64::MAX+add` / `i64::MAX+inc`），上一轮只扫了第一种。

## 护栏（master `a8788b61` 实测，**报数必须带轴名**）

| 指标 | `fire` | `batch` | `batch_conv`（`--drive batch`） |
|---|---|---|---|
| 内容命中 | **331 / 1080** | **355 / 1080** | **633 / 1080** |
| 种类序列 | **819 / 1080** | **919 / 1080** | **1075 / 1080** |

延迟段（`batch` 轴）：死亡集合 **215 / 379**、挖洞点 **140 / 262**。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 20 | `agent/point-array-filler` | 追点数组的**填充者** | **P0** | 1 ulp 与"同坐标不同序号"两条悬案的**共同入口**（报告 §8-1） |
| 21 | `agent/diffeq-fire-path` | 给内核加 `Mode` 入参（DiffEq 开火） | **P0** | **515 批 / 550 发目前不可比**，因内核写死 `Mode::Normal`（§8-2） |
| 22 | `agent/manager-layout` | `FunctionManager`(32B)/`ObstacleManager`(80B) 字段布局 | P1 | 两槽仍是我方形状、标 `stub`（§8-3） |
| 23 | `agent/obstacle-drawer-node` | 补 `ObstacleDrawer` 节点类 | P1 | 客户端替换清单里唯一"类还不存在"的缺口（§8-4） |

## 这一轮必须守的三条

1. **`EffectInfo` / `DestroyObstacle` 的 tag 有三种 materialize 形态**（`movabs` / `i64::MAX+add` / `i64::MAX+inc`）
   —— 少扫一种就会得出"查不动"的错误结论。
2. **`gdext` 构建三步不能少**：`cargo build --release --features gdext-bindings` → **拷进 `client/`** → `--import`。
   合并方今天就在这一步上翻过车（拷了非 gdext 版，`assemble_check` 直接崩、`plane_on_screen_check` 5 条假失败）。
3. **数字必须能复跑**：给命令 + JSONL 路径；只给数不给命令的一律当未验证。

### 第四轮增补（2026-11-08，两个"规模/运行时"任务）

| # | 分支 | 主题 | 为什么给下游 |
|---|---|---|---|
| 24 | `agent/three-way-match` | **三向符号匹配器**（Linux / Android / Windows×2） | **纯算力**：四份构建 × 1.3 万函数的结构指纹最近邻 |
| 25 | `agent/frida-point-array` | 在**真游戏**里抓点数组填充者 | 需要能跑起游戏；静态已卡住两次 |

**给 24 的二进制**：`dist/gw2-windows-refs.tar.zst`（14 MB，两份 Windows DLL + SHA256SUMS + 说明）。
拿不到就自己下：Steam 控制台 `download_depot 3066840 3066842`（**下完立刻 `cp` 出来**）。

**⚠️ macOS 暂时不派**：没有 macOS 环境既不能编译也不能验证，
写一份"无法验证"的配置没有意义（`AGENTS.md` §1：做不到就说做不到）。
真要 macOS，前提是先有机器或 Apple 的交叉工具链。

**给 24 的硬约束（必须写进结论）**：
能跨构建对齐的键只有 `panic::Location`，目标里共 **2,035 个** ⇒ 名字上限是 2,035，**不是 13,503**。
天花板之外只能给"上限估计 + 正对照准确率"，**不许硬凑**。

---

# 第五轮（2026-11-08 凌晨）

第四轮报告在 `dispatch/reports/CG-round4-2026-11-08.md`。**这轮是最大的一次跃升**：

| 指标 | 第四轮前 | 第四轮后 |
|---|---|---|
| fire 内容 / 种类 | 331 / 819 | **608 / 947** |
| batch 内容 / 种类 | 355 / 919 | **643 / 1075** |
| 死亡集合 | 215 / 379 | **366 / 385**（漏报 **0**） |
| 挖洞点 | 140 / 262 | **206 / 293** |

增益主要来自 **21（内核补 `Mode`，DiffEq 局从"不可比"变成能比）** 与 **20/25（点数组填充者结案）**。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 26 | `agent/pos-y-from-network` | 证伪/证实「`pos_y` 来自网络」 | **P0** | 若成立，1 ulp 从"bug"变成"不可复现字段"（报告 §8-1） |
| 27 | `agent/obstacle-room` | 造带障碍的对局，收「同坐标不同序号」样本 | P1 | 这条至今**没有样本**（§8-2） |
| 28 | `agent/stub-semantics` | 把 `stub` 语义一次性改对 | P1 | 状态字符串落后于事实，已误导两轮（§8-3） |

## 这一轮的护栏（master 第四轮合并后实测）

| 指标 | `fire` | `batch` | `batch_conv` |
|---|---|---|---|
| 内容命中 | **608 / 1080** | **643 / 1080** | **633 / 1080** |
| 种类序列 | **947 / 1080** | **1075 / 1080** | **1075 / 1080** |

延迟段（`batch`）：死亡集合 **366 / 385**、挖洞点 **206 / 293**。
`cargo test --offline` = **1569**。

## 三条仍然要守的

1. **数字必须带轴名 + 可复跑命令**；只给数当未验证。
2. **gdext 构建三步**：build（`--features gdext-bindings`）→ `cp` 进 `client/` → `--import`。
3. **合并后 `ls dispatch` 必须为空**（master 侧）；本次已是第二次因合并把 `dispatch/` 带进 master。

---

# 第六轮（2026-11-08 凌晨，**下游休息期间由主会话准备**）

## 先看这两条（会改变你对"剩余失败"的理解）

1. **`S` 桶 126 批不是真失败**：130 批里 **128 批在 `batch` 轴上种类序列是 OK 的**，
   是 `fire` 驱动表达不了"一个结算批一次广播"。⇒ 已派 `agent/fire-batch` 让 `fire` 轴对多发批改走
   `fire_batch`。**这条落地后 `fire` 轴的数会变，"涨了"不等于改好了，是口径变了。**
2. **`E1` 桶里混着真几何差**：`1782764906` 批 #0（`Ode` 局）我方一路走到 `-24.9928`，
   回放停在 `-20.7553`；**y 逐位相同、x 差 4.24**。这不是末位问题，是**停止判据**问题。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 29 | `agent/ode-stop-condition` | `Ode` 局**停止判据** | **P0** | 上面第 2 条；任务 21 只接了 `Mode`，没验停止条件 |
| 30 | `agent/unnamed-176` | 给"有 Location 但无名"的 176 个函数找归属 | P1 | 527 里 351 有名字，剩 176 两边符号表都没有 |

## 主会话正在跑的（结果会回到 master）

```
agent/explosion-pos     追爆炸点坐标表达式（Y+X = 226 批 = 剩余失败 48%）
agent/fire-batch        fire 驱动支持多发批（消 S 桶 126 批假失败）
agent/correction-round3 上游 graphwar_curve.py 的修正量取整（round 到 3 位）
```

## 护栏（**以 `fixtures/guardrails.json` 为准**，这里只是快照）

```
fire  608 / 947      ← agent/fire-batch 落地后会变（口径变更）
batch 643 / 1075
batch_conv 633 / 1075
死亡  366 / 385（漏报 0）｜ 挖洞 206 / 293
cargo test --offline  1569
```

## 这一轮新增的硬规矩

- **改分桶口径 = 改测量定义**，必须在 `docs/06` 写清，并同步 `fixtures/guardrails.json`；
- **桶标签是结论**：标签过期（如 `Y` 曾写"求值器末位"、`S` 曾写"未分组"）会误导所有人，
  已经在 `a48e12db` 改了一轮，**以后再发现过期标签就地改**；
- 合并 master→CG 时，`dispatch/` 会被当成"上游已删除"而静默删掉 —— **合完必须 `ls dispatch | wc -l`**。

## 第六轮补记：客户端迁回 Rust 的**第一刀**已经可以派了

`docs/16` §43.6 那张表写于第四轮之前，第 6 行还写着「`ObstacleDrawer` 类**还不存在**」——**已过期**：
第四轮任务 23 已做出该类（headless 116 条全过）。⇒ **6 个节点位置现在全都已有真类**，只缺接线。

| # | 分支 | 主题 | 依据 |
|---|---|---|---|
| 31 | `agent/play-screen-real-nodes` | 6 个真节点挂进播放屏，删 2 个仿制 `.gd`（788 行），改 44 处调用 + 18 条 selftest 断言 | §43.6 (a)(b)(c)，状态已按第四轮更新 |
| 32 | `agent/play-screen-evidence-gaps` | 补 3 个**可解**的证据缺口：tick 源 `0x519530`、`update_transform` 三实参、队色/底色两张表 | §43.6(d) 的 7 条里挑出 3 条 |

**31 与 32 可以并行**：31 改接线（`client/src/**`），32 补证据（`src/` + `docs/`），文件基本不重叠。

**两件的共同前提**：gdext 构建三步（build `--features gdext-bindings` → `cp` 进 `client/` → `--import`），
缺一步就会拿旧 `.so` 验出假结果。

---

# 第七轮（2026-11-08 凌晨）

## 一条**约定变更**（结构性，防止再出错）

CG 侧的交接文档从 `docs/HANDOVER.md` **迁到 `dispatch/HANDOVER-CG.md`**。
`docs/HANDOVER.md` 在 CG 上**保持与 master 一致**，不再改。

理由：master 那份被 CG 版**静默覆盖三次**（`a8788b61` / 第四轮 / 第六轮）——
git 判定「我方未改、对方全改」时不报冲突、直接取对方。
分开之后 `master..CG` 的差异只剩 `dispatch/` 一个目录，**结构性不会再撞**。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 33 | `agent/entity-drawer-draw` | 补 `EntityDrawer::draw`（`0x4b2890`）—— **准备圈回到画面上** | **P0** | 第六轮报告 §8 第 1 条；用户能直接看见的回退。**算术现成**（`entity_drawer::ring_primitives`），缺的只是接引擎调用 |
| 34 | `agent/ptr-pair-probe` | 运行时抓 `(ptr, n)` 两处读数，判掉 407 批残差的方向 | **P0** | `docs/06 §40.7` 的入口；也是**唯一没被否证**的假设（每发两次 `calculate_function`） |

## 当前护栏（master `a4ad8d86`，以 `fixtures/guardrails.json` 为准）

```
fire 647 / 1075   ｜ batch 647 / 1075   ｜ batch_conv 633 / 1075
死亡 366 / 385（漏报 0）｜ 挖洞 207 / 294
cargo test --offline  1585
headless：assemble 203 / plane_on_screen 96 / obstacle 116 / function_drawer 132 / entity_drawer 72
selftest 709 / 0 failed（本机 assets）
```

## 剩余失败 433 批的成分

```
Y  189（爆炸点 y 末位差，≤1e-9）
E1 127（end_x 小差，含 Ode 局几何差）
X   91（回放自身 pos_x ≠ 其 end_x）
C   14 ｜ E2 11 ｜ D 5
```

---

# 第八轮（2026-11-08 凌晨）

## 先认一件事（写给下一个合并的人）

第七轮我（协调方）**没读报告就合了**，而报告 §2 的标题就是「33：绘制做了，但**画面上什么都没变**」。
后来是靠用户问「第七轮验了吗」才回去读的。⇒ `AGENTS.md` 合并纪律 **3b** 就是为这个写的：
**先读 §5 遗留 + §6 建议，再合并。** 只跑四件套不算验完。

## 新任务

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 35 | `agent/residual-407` | 把 34 号那套折叠复算**跑到 407 批上** | **P0** | 报告 §6-1：目前唯一「有机制、可复算」的入口；**DiffEq/Ode 114 发一发未覆盖** |
| 36 | `agent/plane-pos-manager` | 解 `PlanePosManager`(24 B) —— 最后一个 `stub` | P1 | 报告 §6-4；它连 `src/` 文件都没有 |
| 37 | `agent/texture-loader` | 士兵/靶子不该是纯色方块 | P1 | 报告 §6-3 |

## 当前护栏（master，以 `fixtures/guardrails.json` 为准）

```
fire 647 / 1075 ｜ batch 647 / 1075 ｜ batch_conv 637 / 1075
死亡 366 / 385（漏报 0）｜ 挖洞 207 / 294
cargo test --offline 1620
headless：entity_drawer 118 / assemble 213 / plane_on_screen 96 / obstacle 116 / function_drawer 132
```

## 第七轮留下的、还没人接的

```
准备圈的**输入端**   现在用 _sync_entities 代 pos_info_event 接线（能看见，但链路是代偿）
pos_radius 语义     PosInfo.radius 恒 0.5，第 3 个 f64 的标定未做（docs/140 §3.1 那 14%）
「越界停」          未动
```

---

# 第九轮（2026-11-08 傍晚）

## 上一轮最重要的结论，以及它带来的下一批活

第八轮把 407 批判成「**回放自身不自洽**」（407/407，正对照 647/0）。
那句话有个前提：**回放里记的文本就是当时真正喂进采样器的文本**。第九轮先把这个前提钉死。

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 39 | `agent/token-at-fire` | 抓**开火那一刻**真正喂进 `calculate_function` 的 token 列表 | **P0** | 报告 §8-1；407 那条结论最后一块替代解释 |
| 40 | `agent/ode-runtime-samples` | 补 **DiffEq/Ode 局**运行时样本 | **P0** | 报告 §8-2；残差 219 批是 DiffEq，而 34 号 37 发**一发 Ode 都没有** |
| 41 | `agent/get-all-positions` | 把 `get_all_positions` 接进 `EntityDrawer` | P1 | 报告 §8-3；原版 `_draw` 读它，我们现在不读，顺序是散列序**要排序** |
| 42 | `agent/real-inputs` | 两个"输入端"从**代偿**换成真链路 | P1 | 报告 §8-4；`pos_info_event` 与 graphics 51/53/55 现在都是我方接线 |

## 当前护栏（master `9f3b7d18`，以 `fixtures/guardrails.json` 为准）

```
fire 647 / 1075 ｜ batch 647 / 1075 ｜ batch_conv 637 / 1075
死亡 366 / 385（漏报 0）｜ 挖洞 207 / 294
cargo test --offline 1628
headless：entity_drawer 118 / assemble 213 / plane_on_screen 96 / obstacle 116 / function_drawer 132
selftest 712 / 0（**有 serve_match**；没有它时是 636，差的是联机那 76 条）
```

## 合并前请先跑

```
python3 tools/merge_check.py --cg origin/CG
```

它会把本报告的 §遗留/§建议 打出来、扫自曝句、查禁区与污染。**缺节/碰禁区/二进制/dispatch 进 master 一律非 0。**

---

# 第十轮（2026-11-08 傍晚）

## 第九轮的遗留（报告 §7/§8 原文要点）

```
· 谁每帧驱动 KernelBus::pump   —— 原版 GraphwarGameClient::process 在 x86 上**还没定位**（最后一环）
· end_x 的真身                —— 未知；需要同时抓 FunctionPoints 装配处 + 0x472865
· Ode 的三条边界              —— 预算用尽 / 二分不收敛 / 有障碍提前停，**一条都没验**
· EntityDrawer 的门控         —— get_alive / is_on_turn 现在用 prep 成员**近似**（推断）
```

## 新任务

| # | 分支 | 主题 | 优先级 |
|---|---|---|---|
| 43 | `agent/game-client-process` | 定位 `GraphwarGameClient::process`（x86）→ 到 `pump` 的完整链路 | **P0** |
| 44 | `agent/endx-truth` | **同时**抓 `FunctionPoints` 装配处 + `0x472865`，定那 100 发哪个字段不是"这次读" | **P0** |
| 45 | `agent/ode-edge-cases` | Ode 三类边界样本（预算用尽 20001 点 / 不收敛 / 有障碍） | P1 |
| 46 | `agent/entity-drawer-gating` | `get_alive` / `is_on_turn` 从近似换成真查询 | P1 |

## 当前护栏（master `4277d901`）

```
fire 647 / 1075 ｜ batch 647 / 1075 ｜ batch_conv 637 / 1075
死亡 366 / 385 ｜ 挖洞 207 / 294
cargo test --offline 1631
headless entity_drawer 236 / assemble 217 / plane_on_screen 96 / obstacle 116 / function_drawer 132
selftest 712（有 serve_match）/ 636（无）
```

## 一个已完成的第三方审计（可以少走弯路）

`docs/158-audit-407.md`：「407 批 = 回放自身不自洽」**经独立复现成立**（不用本仓库 tools 自写复算器；
正对照 20/20；并集 407 漏 0）。⇒ **不要再花力气去"修"那 407 批**，它们不是我们的错。
唯一的替代解释（回放记的文本≠当时喂进去的文本）由第九轮 39 号在抓。

---

# 第十一轮（2026-11-08 深夜）

| # | 分支 | 主题 | 优先级 | 依据 |
|---|---|---|---|---|
| 47 | `agent/process-loop` | 把 **60 Hz 定步 `process` 循环**落进内核 | **P0** | 43 号已给出 `0x518400` 与 3 处改造清单，只差落地（**动 src/ 要同提交两份 coverage 生成物**） |
| 48 | `agent/array-divergence` | 抓"两装配点数组**真的分叉**"的那一刻 + 定位改 `Vec.len` 的点 | **P0** | 44 号证了"不同对象不同时刻"但没抓到分叉；45 号只给了 `5098→1528` 的区间 |
| 49 | `agent/level-flip-samples` | 补 **Level 18–20** 与 **`flip=1`** 的样本 | P1 | 现有多士兵/Bot/翻转**一发都没有** |
| 50 | `agent/turndata-ordering` | `TurnData[+0x58]` 顺序回合分支 + `get_alive==2` 语义 | P1 | 现在 `is_on_turn` 是原版**超集**，画面上可能多画 |

## 当前护栏（master `9cfa3bea` 之后）

```
fire 647 / 1075 ｜ batch 647 / 1075 ｜ batch_conv 637 / 1075
死亡 366 / 385 ｜ 挖洞 207 / 294
cargo test --offline 1638
headless entity_drawer 292 / assemble 217 / plane_on_screen 96 / obstacle 116 / function_drawer 132
selftest 712（有 serve_match）/ 636（无）
```

## 两条别忘的（都来自已完成的第三方审计）

- `docs/158`：**407 批已独立确认是「回放自身不自洽」**（正对照 647/0）⇒ **别再花力气修它们**；
- `docs/159`：结构指纹 2,745 个高置信是**候选（推断级，≈80–95%）**，不是名字；单射已改成全局。

---

## 算力分配（2026-11-08 记录）

| 机器 | 规格 | 适合什么 |
|---|---|---|
| **下游（CG）** | **32 GB RAM / Ryzen 9 7945HX（16C32T）/ RTX 4060** | **算力活**：全量指纹重建与网格调参、unicorn 逐指令矩阵、多进程差分、gdext 全量重建 |
| 上游（本机） | 16 核 / 15.3 GB RAM / 无独显 | 审查、合并、小修、文档、单点验证 |

⇒ **凡是"多进程 + 大内存 + 长时间"的活优先派下游**；本机只做不需要吃满机器的事。

## 第十一轮增补（算力活）

| # | 分支 | 主题 | 为什么给下游 |
|---|---|---|---|
| 51 | `agent/fingerprint-v2` | 去掉**泄漏特征**重训指纹（`strs` = Location 文件名；`consts` 跨构建不可用；`hist` 无效） | 8 片 × 多组权重的网格，纯算力 |
| 52 | `agent/unicorn-matrix` | 解析器差分矩阵放大到 **≥5,000 条**（现在只 349 条） | unicorn 逐指令 emulation，可多进程并行 |

## 关于「下游 CPU 只跑到 20%」

原因不是机器不行，是**活的形状不对**：现在的任务大多是单线程（一次 `replay_attrib` 走完 1080 批、
一个 frida 进程挂一局、一个 unicorn worker 跑一条式子）。16C32T 只用 1–2 个核 = 正常。

⇒ 派活时**优先挑 embarrassingly parallel 的形状**，并在任务书里写死并行要求：

```
· 按回放文件 / 按函数分片 → 进程池并发（--jobs N, N ≥ 16）
· 汇报必须给 墙钟时间 与 加速比；墙钟接近串行 ⇒ 不算完成
· 汇总表入库，大 JSONL 不入库
```

已按此形状派：**51（指纹网格）、52（unicorn 矩阵）、53（假设空间整片扫）**。

---

# 第十二轮（2026-11-09）

**用户已放开禁区 `client/src/**`**（客户端要按 Rust 控制器还原）。

| # | 分支 | 主题 | 优先级 |
|---|---|---|---|
| 54 | `agent/client-controllers-rust` | `client/src/screens/*.gd`（17 个 / **6,055 行**仿制品）的**流程**换回 Rust；视图留 GDScript | **P0（最大工程）** |
| 55 | `agent/overlap-scenario` | 造「结算与开火重叠」的对局 —— 唯一还能解释那 100 发的场景 | **P0** |
| 56 | `agent/print-roundtrip` | 盯「打印-回代链」：3 位小数常数 ↔ 全精度常数（Y 189 + X 91 桶） | **P0** |
| 57 | `agent/new-evidence` | 给结构指纹换新证据来源（机器码 n-gram / 常量 xref / 字符串内容） | P1（算力） |
| 58 | `agent/callgraph-roles` | 用调用图给无名函数反推**角色**（不是名字） | P1（算力） |
| ~~59~~ | ~~`agent/replay-version`~~ | ~~判定「老版本 replay」假设~~ → **上游已用实验否掉**（见下） | 已结 |
| 60 | `agent/posy-writer` | **谁把 `EffectInfo.pos_y` 写成那个值**（替代 59） | **P0** |

## 关于 59：为什么先算了一遍

上游已按 replay 文件名时间戳分月统计过命中率（见任务书 §0）——**失败率不随月份下降**
（2026-11 的批次仍有 26% 是 y 末位差）⇒ "老版本"**不能单独解释**。
但两件事没证：① 文件名时间戳是否真是录制时间；② 老版本求值器/libm 是否不同（末位差正是这个量级）。
⇒ 任务书要求**先证 A，再用老 .so 重算 B**，并写死了判据（≥80% 复现才算成立）。

## 59 号为什么被否（2026-11-09，上游实验）

用户新打两把**当前版本**的 AI 对决 → 一样差 1 ulp；再把该发的修正 token 流喂给
**unicorn 直调真 `.so`**（与录制该 replay 的同一构建）⇒ 真二进制给出的是**我方那个值**，
回放记的是另一个值。⇒ **不是老版本的问题**，是"回放里的 `pos_y` 不是采样器的输出"。

详见 `docs/06` §33。新任务 **60** 就是查这件事。

## 第十二轮增补（2026-11-09）：新样本与"用完删除"

用户当场新录两把 AI 对决，已放进 **`dispatch/replays/`**：

| 文件 | 回合模式 | 我方对拍 | 用途 |
|---|---|---|---|
| `1794158844` | **顺序**（TICK 3661→5594→7893→10190→12315 严格递增） | **7 / 10** | 作**正对照** |
| `1794158991` | **同时**（5 发全 `TICK 22734`） | **0 / 4** | 55 号的主样本（重叠假设） |

⚠️ **做完请把 `dispatch/replays/` 整个删掉**（`git rm -r dispatch/replays && rm -rf dispatch/replays`），
理由与做法见 `dispatch/replays/README.md`：它是运行时产物（二进制），按 AGENTS 不长期保存；
要留证据只把**数值**写进 `docs/06`。

## 55 号因此改题

原本要"造结算与开火重叠的场景"——**现在不用造了**，`1794158991` 就是现成的同时回合样本，
而且它里面已经有两个"几十个步长"的差（**0.375 = 37.5 步**、**0.000234 = 23.4 步**），
比 1 ulp 大得多，属于另一类问题。任务书已改成"把这份样本吃透"。

## 第十二轮再增补（2026-11-09 深夜）：新样本 + 算力/显卡分配

### 新样本（`dispatch/replays/`，**用完删**）

| 文件 | 回合模式 | 每回合发数 | 我方对拍 |
|---|---|---|---|
| `1794158844` | 顺序 | 1 / 1 / …（10 回合） | 7 / 10 |
| `1794158991` | 同时 | 8 / 7 / 3 / 3 | 0 / 4 |
| `1794161153` | 同时 | 8 / 8 / 3 | 1 / 3 |
| `1794161259` | 同时 | 8 / 8 / 2 | 1 / 3 |
| `1794161376` | 同时 | 8 / 8 / 5 | 0 / 3 |

**上游已量化的规律**（`docs/06` §33.4f，26 个批）：**≥6 发的批 8/8 全失败**，
`>1e-3` 的大差 8 个里 5 个落在 ≥6 发的批；而 ≤3 发的 17 批只失败 6 个、大差 1 个。
⇒ **55 号据此改成"查 ≥6 发回合的机制"**。

### 新增两份算力活

| # | 分支 | 主题 | 为什么给下游 |
|---|---|---|---|
| 61 | `agent/gpu-similarity` | **GPU** 做结构指纹的大规模相似度检索（12,736 × 51,985，6.6 亿对） | 本项目**唯一真正适合 GPU** 的形状（纯 GEMM + 集合交，可切块无分支）。RTX 4060 够用；混合显卡可能要 `prime-run`，**不通就退回 CPU 但必须报实测** |
| 62 | `agent/shots-vs-fail` | 全语料 401 份上验「每回合发数 ↔ 失败」 | 现在只有 5 份新样本（26 批）的结论，**样本太小**；全语料一跑就知道 ≥6 发这条规律站不站得住 |

### 派活原则（更新）

```
· 想吃满 CPU：按文件/函数/批分片 + --jobs ≥16 + 汇报加速比（55/62）
· 想吃满 GPU：只给它"矩阵型"的活（61）；不要为了用 GPU 而用 GPU
· 混合显卡不确定时：先 nvidia-smi / prime-run nvidia-smi 自检，把结果写进汇报
```

---

# 第十三轮（2026-11-09 凌晨）

第十二轮的结论：**73% 不是天花板 → 78.2%**（57 号）；**四条假设线被关掉**（55/56/60/62）；
58 号诚实负结论但量出了调用图结构（指针边比直接边多 53%）。

| # | 分支 | 主题 | 类型 | 优先级 |
|---|---|---|---|---|
| 63 | `agent/fingerprint-ablation` | **GPU** 特征消融 + bootstrap 置信区间 + **逐特征取值范围审计** | GPU + 多进程 | **P0** |
| 64 | `agent/nan-batches` | `corrected_function` 含 **NaN** 的批（幅度 >10 的真错） | 单线程 | **P0** |
| 65 | `agent/settle-order` | **报文顺序 ≠ 结算顺序**：按到达序解算 + 逐发挖洞 | 单线程 + 三轴回归 | **P0** |
| 66 | `agent/token-triple` | **三样同时抓**：token / `(rdx,rcx,n)`+16 B / EffectInfo JSON | 运行时（frida） | **P0** |
| 67 | `agent/play-flow-2` | `play_screen` 剩下的流程搬进 Rust（弹道生命周期 ~200 行等） | 客户端 | P1 |
| 68 | `agent/tooling-hygiene` | `merge_check` 大文件闸 + **冻结语料指纹检查** | 工具 | P2 |

## 并行度（谁该吃满机器，谁天生串行）

| # | 形态 | 并行做法 |
|---|---|---|
| 63 | **GPU + 多进程** | 配置 × 特征 × bootstrap 全部独立；显存切块 |
| 64 | **可并行**（补） | 检出按文件分片；诊断按 `(batch, shot)` 分片 —— unicorn 每发独立 |
| 65 | **可并行**（补） | **把顺序假设参数化**（S1–S5）× 三轴 × 401 文件 ⇒ 一次扫出矩阵表，而不是改一次跑一遍 |
| 66 | **串行**（硬约束） | 三样必须在同一局里按时间对齐；能省的是**把人工开炮自动化**，不是并行 |
| 67 | 代码串行 / 验证并行 | 14 个 headless 脚本同时跑 |
| 68 | 串行（量小） | — |

## 63 为什么这次真该用 GPU

```
配置空间 ≥300 组（16 特征留一法 + 权重网格 + 2^4 子集枚举）× 每组 ≥200 次 bootstrap
⇒ CPU 单进程 1131 s/组 ⇒ 数百组 = 数天；GPU 4.48 s/组 ⇒ 十几分钟
★ 注意 61 号已量到：**小矩阵 GPU 反而慢**（800 行 0.7×），所以只用全量测
```

## 62 号给 63 的直接线索

```
"一个特征的取值范围假设错了，就藏掉 5.5 个点"（`consts` 只收 0x100..0xFFFFFFFF）
⇒ 63 的任务 B 就是**把 16 个特征逐个审一遍取值范围**
```

---

# 第十四轮（2026-11-09）

第十三轮关掉了两条线（66：常数末位差 + 数组被改双双被否；65：结算顺序成立并落地），
63 把消融做到 318 组、给出 **78.18% 到顶 + CI [72.12, 84.24]**（n=165 ⇒ ±6 点，再调权重是浪费算力）。

| # | 分支 | 主题 | 类型 | 优先级 |
|---|---|---|---|---|
| 69 | `agent/lan-ai-match` | 造 **LAN + AI 局**（1 ulp 唯一还开着的入口）+ 给 `0x3c5f70` 找调用者 | 运行时 | **P0** |
| 70 | `agent/callee-graph` | 建**间接调用图**（52,577 条指针边）⇒ 一次救活 6 个"两侧不可比"的特征 | **CPU 多进程（重）** | **P0** |
| 71 | `agent/sim-tensor-sweep` | **物化一次相似度张量 + N 组加权和** —— 这才是 GPU 该干的活 | **GPU** | **P0** |
| 72 | `agent/holdout-500` | 正对照 348 → **500+**，把 ±6 点压下去 | CPU | P1 |
| 73 | `agent/conv-follow-s3` | 收敛层跟上 S3（否则两层口径分叉） | 小改 | P2 |

## 71 为什么这次真能吃到 GPU（63 号自己给的诊断）

```
61 号那套是 **top-K 检索器**：只吐排名，不吐"每个特征的相似度"
而 N 组权重需要的是**同一批相似度的加权和**（相似度与权重无关 ⇒ 只该算一次）
63 号的做法：每换一组权重就重算一遍 ⇒ 接口形状决定 GPU 帮不上
正确设计 = 物化一次相似度张量 + N 组加权和 —— 63 明确写"我没做、也未实测"
⇒ 71 就是把它做出来并实测
```

⚠️ 报 GPU 数必须给**两个口径**：工具内部计算时间 / 进程墙钟（后者含 `import torch` ≈5 s）。
63 号那次就是混了口径才得出"小矩阵 GPU 更慢"的错误结论。
