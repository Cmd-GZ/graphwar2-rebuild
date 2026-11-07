# 任务书（分发用）

这个目录是**派发给别的机器/agent 的任务书**。它不是产品文档，不进 `docs/` 索引。

基线：`master` @ `8755db9e`（**第三轮起点**）。第一轮 8 份任务已全部合并进 master；
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
