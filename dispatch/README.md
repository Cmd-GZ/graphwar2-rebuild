# 任务书（分发用）

这个目录是**派发给别的机器/agent 的任务书**。它不是产品文档，不进 `docs/` 索引。

基线：`master` @ `bb4e8959`（12/12 个 GDExtension 类已节点化，可归属函数 527/527）。

## 所有任务共用的契约

### 仓库与分支

```bash
git clone git@github.com:Cmd-GZ/graphwar2-rebuild.git
cd gw2-rebuild
git checkout -b CG/<任务名> origin/master
```

干完推到 `origin`，**不要直接推 `master`**，也不要推 `CG` 本身——由父会话合并。

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
| 01 | `CG/assemble-min` | RootNode 真装配 3–5 个 manager，跑出棋盘 + 士兵 | **P0** | 是 |
| 02 | `CG/function-drawer-node` | `FunctionDrawer` 节点化（补上 12 类之外的唯一缺口） | **P0** | 是 |
| 03 | `CG/bit-exact-report` | 整局对拍剩余失败的归因分类 | P1 | 是（回放语料） |
| 04 | `CG/campaign-data` | 21 关参数表落码 + 接客户端 | P1 | 否 |
| 05 | `CG/crossbuild` | Windows / Android 编译验证 | P2 | 否 |
| 06 | `CG/name-match-review` | 63 个多候选人工定夺 + 444 无候选分类 | P2 | 是 |
| 07 | `CG/docs-renumber` | `docs/16` 的 §19/20/23/34 空号与交叉引用 | P2 | 否 |
| 08 | `CG/viewport` | 用原版 `.gd` 换视图层的可行性调研 | P3 | 否 |

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
