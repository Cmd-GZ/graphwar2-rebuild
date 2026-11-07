# HANDOVER（**CG 分支 · 执行侧交接**）

> **位置约定（2026-11-08 起）**：CG 侧的交接文档放在 `dispatch/HANDOVER-CG.md`，
> **不再改 `docs/HANDOVER.md`**。原因：master 那份每轮都会被 CG 版静默覆盖（已发生三次），
> 把两份分开之后，`master..CG` 的差异就只剩 `dispatch/` 一个目录，结构性不会再撞。

> 上下文压缩后先读这一份。它记的是"**在这台机器上怎么把活干完**"：
> 环境在哪、流程怎么走、护栏是多少、**我犯过哪些错**。
>
> 复原到什么程度看 `master` 的 `docs/HANDOVER.md`；合并纪律看 `AGENTS.md` §「合并纪律」。
> 本文件是 CG 分支独有（与 master 那份相互独立），改它不会影响主线。

## 0. 一分钟看懂

```text
分支布局
  CG               我的工作分支：**所有已完成的活 + dispatch/（任务书 + 执行报告）**
  agent/<task>     每个任务一个分支（我派 subagent 时建），**全部已推 origin**
  master           上游主线：他们把我的 CG 并进 master，我再用 master 同步回来

已交付      第一轮 01–08、第二轮 09–14、第三轮 15–19、第四轮 20–25、
            第五轮 26–28、第六轮 29–32、第七轮 33–34 —— 全部已合并进 CG
            报告在 dispatch/reports/CG-*.md
下一轮候选  见 §7

护栏（2026-11-08 04:00 实测；**以 fixtures/guardrails.json 为准**）
  fire       内容 647/1080 ｜ 种类 947/1080   ← 口径已变：fire 对多发批走 fire_batch
  batch      内容 647/1080 ｜ 种类 1075/1080
  batch_conv 内容 637/1080 ｜ 种类 1075/1080
  延迟段     死亡 366/385 ｜ 挖洞 206(fire) · 207(batch)（分母 293·294）
  cargo test 1585 / 0 failed ｜ --features gdext-bindings 1672 / 0 failed
  headless   12 个脚本 1372 条断言 0 失败
  selftest   **712** / 0 failed（四前置齐时；见 §5）

⚠️ 写护栏：`python3 tools/bit_exact_attrib.py --json <jsonl> --update-baseline --axis fire|batch|batch_conv`

⚠️ 写护栏用什么：`python3 tools/bit_exact_attrib.py --json <jsonl> --update-baseline --axis fire|batch|batch_conv`
（**不是** `--write-guardrails`，那是我自己那版临时改法的参数，已废弃）。
```

## 1. 环境（**都在 `.reference/`，已 gitignore；工作区外只读**）

```bash
cd /home/CG/Myprojects/graphwar2-rebuild        # 仓库根 = 工作区根
export CARGO_HOME=$PWD/.reference/cargo-home
export CARGO_TARGET_DIR=$PWD/.agent-work/cg/targetCG      # ⚠️ 必须单独一行赋值
export GODOT=$PWD/.reference/godot/Godot_v4.7.2-stable_linux.x86_64
export PY=$PWD/.reference/venv/bin/python        # capstone/unicorn/zstandard/frida
export COVERAGE_MATRIX_UPSTREAM=$PWD/.reference/upstream
export COVERAGE_MATRIX_PYTHON=$PY
export R=$PWD/.reference/replays/gw2-replays-snapshot/replays   # 401 份 + 2 份元数据
```

| 路径 | 是什么 |
|---|---|
| `.reference/steam/2026-08-28/libgraphwarrust.so` | **逆向目标**（x86-64，sha256 `9a538115…`） |
| `.reference/steam/android-2.0.60/libgraphwarrust.so` | 同版本 arm64、**未 strip**（名字的主要来源） |
| `.reference/windows/graphwarrust-2.0.60.dll` | Windows 同 ISA + **COFF 符号表**（939 名）；`early2026` 那份 746 名 |
| `.reference/game/` | **游戏本体**（`Graphwar.x86_64` + `.pck` + `.so`，`.so` 与目标逐字节相同） |
| `.reference/analysis-repo/` | 上游 `graphwar2-analysis` 的浅克隆（`analysis/emu.py` = unicorn oracle） |
| `.reference/upstream/` | `coverage_matrix.py` 要的"上游目录"（`.so` 软链 + `analysis/funcstarts_ghidra_va.pkl`） |
| `.reference/rustup/` + `.reference/toolkits/` | 交叉编译（便携 minGW / NDK）+ 各工具包 tarball |
| `.reference/venv/` | Python 依赖（frida 17.26.1 也在这） |
| `.agent-work/cg/` | 我的工作区：`wt-NN`（工作树）、`targetNN`（私有 target）、`fakehome`（selftest 用） |

**Game headless 跑法**（`user://` 默认落只读家目录会 core dump）：

```bash
D=$PWD/.agent-work/cg/gamedata; mkdir -p $D/{data,cache,config}
cd $PWD/.reference/game && XDG_DATA_HOME=$D/data XDG_CACHE_HOME=$D/cache XDG_CONFIG_HOME=$D/config \
  ./Graphwar.x86_64 --headless --quit
```

## 2. 我做一份任务的流程（照这个走）

```bash
# ① 建工作树 + 分支（基于当前 CG）
git worktree add -q .agent-work/cg/wt-NN -b agent/<task> CG

# ② 派 subagent，prompt 里必须带：
#    - 工作目录、分支、禁止 push/merge
#    - 完整环境变量（私有 CARGO_TARGET_DIR！）
#    - 我**刚刚实测**的护栏数 + 可复跑命令
#    - 允许改的文件清单 + 禁区
#    - "数字带轴名"、"负结论要正对照"、"验收必须能从干净树重跑"
#    - 汇报 ≤8 行

# ③ 回来后**自己复核**（少一条都不合）：
git -C .agent-work/cg/wt-NN diff --name-only <分叉点>..HEAD   # 范围（⚠️ 用分叉点，不是 CG!）
#   然后：私有 target 冷编 → 重跑它自己的验收脚本 → 复现它报告的数字 → 读关键 diff
#   负结论抽查正对照；污染检查（二进制/大文件/art 目录）

# ④ 合并：git merge --no-ff agent/<task>
#   冲突处：生成物**一律重跑**，绝不"取一边"；docs 节号冲突由我顺延并同步子标题

# ⑤ 合并后四件套：cargo test ×2、coverage+docrefs、三轴护栏、headless（如有客户端改动）
# ⑥ 写报告到 dispatch/reports/CG-roundN-<date>.md，推 CG + 各 agent 分支
```

## 3. 护栏怎么更新（**别写死在代码里**）

上游已改成读配置：`fixtures/guardrails.json`（三轴各一份，含每轴的复跑命令）。

```bash
# 改善了实现之后（必须与代码同批提交）：
python3 tools/bit_exact_attrib.py --json f.jsonl --write-guardrails --axis fire
python3 tools/bit_exact_attrib.py --json b.jsonl --write-guardrails --axis batch
# 校验：
python3 tools/bit_exact_attrib.py --json f.jsonl --check-baseline --axis fire
```

⚠️ **`--axis` 必须与实际 JSONL 对应**：`fire`/`batch` 用 `replay_attrib`，`batch_conv` 用 `batch_conv`。
传错轴会报"不一致"，那不是回归。

## 4. 我犯过的错（**压缩后最该记住的一节**）

| # | 错 | 后果 / 正确做法 |
|---|---|---|
| 1 | 用 `git diff CG..HEAD` 查分支改动范围 | CG 前进过 ⇒ 把别人的改动算进来、结论全错。**永远用分叉点**（`git merge-base` 或建树时的那个 sha） |
| 2 | 断言"selftest 694 复现不出来" | **错**。四个前置：`client/assets/` 齐、**解完资产后再 `--import`**、`build --example serve_match`、两个回放样本进 `$HOME/.../replays`。少一样就掉到 686/606/530 |
| 3 | 合并时把 coverage 生成物"取 theirs"直接 commit | 基线变红（**犯过两次**）。生成物冲突**必须重跑生成器**后再提交 |
| 4 | 验收 subagent 时只核了"范围 + 结论措辞" | 它交的 frida 钩子脚本有**三个缺陷**（没真读指针槽、onLeave 没记返回值、队列 dump 静默跳过整条数组），上游复现时才发现并纠正了它的三处结论。**"能跑出数"≠"别人能重放"** |
| 5 | 重排文档节号时只改主标题 | 漏掉 `### xx.y` 子标题 ⇒ `check_doc_refs.py` 报 10 项死链。**改节号要连子标题与 `§` 引用一起改** |
| 6 | 用 `${ax:0:1}` 拼文件名做循环 | 拿到错的 JSONL ⇒ 假报"1 项不一致"。**先怀疑自己的脚手架，再怀疑工具** |
| 7 | 一度在 `src/lib.rs` 禁区外绕路（`#[path]` 临时通道） | 副作用是两个模块只在 gdext 下编译。后来我自己接力正本清源（补 `pub mod`），默认轴 1563→1569 |

## 5. 陷阱清单（都真踩过）

1. **`export CARGO_TARGET_DIR=$CARGO_HOME/target` 必须单独一行**——同一行展开会变成 `/target`。
2. **并行任务各用私有 `CARGO_TARGET_DIR`**；共享会"串味"，出现假失败。
3. **gdext 三步缺一不可**：`cargo build --features gdext-bindings` → `cp` 进 `client/` → `--import`。
   判据：`grep -a -c line_instance_id client/libgw2_rebuild.so` = 3。
4. **`cargo build` 可能判成 fresh（0.01s）而客户端仍跑旧 `.so`** ⇒ 新类"不存在"。`touch` 源文件或换 target。
5. **selftest 读 `$HOME`，不走 `XDG_DATA_HOME`** ⇒ 要 `HOME=$H bash client/run.sh selftest`。
6. **`/tmp` 是逐命令 tmpfs**——跨命令的临时文件会凭空消失，一律写 `.agent-work/`。
7. **合并 `master` 会静默删掉 `dispatch/`**（master 里没有它）⇒ 合完 `ls dispatch | wc -l`。
8. **任务书里的路径是别的机器的布局**：本机 Godot 在 `.reference/godot/`、Windows DLL 在 `.reference/windows/`、
   快照在 `.reference/replays/…`。派活时必须给**本机路径**。
9. **地址口径**：Ghidra = ELF 虚地址 + `0x100000`；arm64 与 x86 地址**不通用**。
10. **函数指针三种 materialize 形态**：`movabs` 直写 / `i64::MAX+add` / `i64::MAX+inc`——只扫一种会得出"查不动"的错结论。

## 6. 已完成的六轮：每轮最值钱的一条

| 轮 | 任务 | 最值钱的结论 |
|---|---|---|
| 1 | 01–08 | 跨平台 build 成立；原版 `.gd` 40 个可直接换；21 关数据落码 |
| 2 | 09–14 | **修正文本只印 3 位小数** ⇒ 内容命中 21→324；挖洞点 = 爆炸点；22 条 `add_module` 全定名 |
| 3 | 15–19 | 求值器**逐位排除**（`eval_differ=0`）；收敛层并回内核 740/740 逐位；tag 三种形态 |
| 4 | 20–25 | 点数组填充者 = `calculate_function`；**DiffEq 550 发从不可比变逐位全同**（fire 331→608）；真机开炮 |
| 5 | 26–28 | **证伪"pos_y 来自网络"**；8 组样本结案"同坐标不同序号" = **两个 ObstacleManager 实例各排一次队**；`stub` 5→1 |

报告都在 `dispatch/reports/`：`CG-2026-11-07`（一）、`CG-round2-2026-11-07`、`CG-round3-2026-11-08`、
`CG-round4-2026-11-08`、`CG-round5-2026-11-08`。

## 7. 第六轮（进行中）与待办

```text
29 ode-stop             Ode 局停止判据（我方走到 -24.99、回放停 -20.7553；y 逐位相同）
30 unnamed-176          176 个"有 Location 但无名"的函数找归属
31 play-screen-real-nodes  6 个真节点挂进播放屏 + 删 plane_view.gd / fire_fx.gd
32 play-screen-gaps     tick 源 / update_transform 三实参 / 队色两张表
```

**下一轮该优先的**（按"能不能指到证据"排）：

1. **偏移常数的十进制→double** —— 26 号点名，能解释"纯 `+-*/`、x 逐位相同却差 1 ulp"那 23 发；
2. `FunctionManager` 的**发方向事件流**（接上就从 `layout` 升 `wired`）；
3. **客户端侧挖洞记录的结构**（27 号在客户端侧读到 `rec_index = 0x3fe00000` = 0.5 的 f64 高位，
   说明两侧记录布局不同）；
4. `PlanePosManager`（24 B）—— 最后一个结构未知的槽。

## 8. 与上游（master）的协作

- 上游会把我的 `CG` 并进 `master`，**不带 `dispatch/`**；我再用 `master` 同步回来。
- 不变量：`git diff --name-only origin/master..CG` **只应出现 `dispatch/…` 与 `docs/HANDOVER.md`**。
  如果出现别的，说明有东西只活在 CG 上，要及时并回 master（或说明为什么不该并）。

  ⚠️ **已知例外（2026-11-08 记）**：`tools/bit_exact_attrib.py` 的**桶标签描述**有一条改动只在 CG
  （上游自己的提交 `80570e7`「工具标签取 master 的更正版」，把 `S`/`E1`/`X`/`Y` 四个桶的说明改准确了，
  例如 `S` 改成"fire 驱动的口径产物：batch 轴 128/130 已 OK"、`Y` 补上"求值器已逐位排除"）。
  它没进 master ⇒ 上面那条不变量现在会多出这一个文件。**这是上游的改动，别去 revert 它**；
  下次上游把 CG 并进 master 时它会自动过去。
- 推送：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push origin CG`，
  各任务分支 `git push origin agent/<task>`。**不推 master**。
