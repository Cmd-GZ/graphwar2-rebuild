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
