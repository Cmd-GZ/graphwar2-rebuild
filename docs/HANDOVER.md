# HANDOVER —— 给上下文压缩后的自己

> 最后更新：token 权重表 + 变异算子 + 三种关卡模式那一轮之后。**先读这一页**；细节按文档编号查。
> 进度表在 `README.md`，状态总览在 `docs/00-status.md`。

## 0. 一句话状态

Graphwar II 的 **曲线 / 物理 / 协议 / 服务端 / 关卡生成** 都已按真值复刻并验证；
**AI 的算法、适应度、六档参数表、机器人名单**都已还原（`docs/66`/`67`/`68`）。
AI 的 **token 权重表**已从二进制直读、并用 40 万样本实测吻合，实现为 `src/aigen.rs`；
**变异算子**与**三种关卡生成模式**也已还原。
只剩：**变异/交叉的个体数**、**`-5e7` 阈值**、**回合出场顺序**（已证不可复现）、
**玩家颜色分配**。

数字：**117 条测试全绿**；文档 73 篇；进度表 **20 ✅ / 4 🔶 / 1 ⬜**。

## 1. 仓库与推送规矩

| 仓库 | 路径 | 远端 | 说明 |
|---|---|---|---|
| 上游 | `..`（`gw2/`） | `Cmd-GZ/graphwar-algorithm-analysis`（`master`） | 曲线算法报告；**绝不包含**本子仓库 |
| 本仓库 | `gw2-rebuild/` | `Cmd-GZ/graphwar2`（`main`） | 源码复原 + 全部文档 |

- 推送必须用：`GIT_SSH_COMMAND="ssh -F /dev/null -o StrictHostKeyChecking=accept-new" git push`
- 自检：在上游执行 `git ls-files | grep gw2-rebuild` **必须为空**。
- `rsc/`（游戏资源）、`.pck`、反编译产物一律 gitignore（体积 + 版权）。

## 2. 环境坑（每条都真踩过）

| 症状 | 原因 | 处理 |
|---|---|---|
| 编译报无法访问 registry | `~/.cargo` 只读 | `CARGO_HOME=<工作区>/.cargo-home`，且 **`CARGO_TARGET_DIR` 必须单独一行赋值**（同一行展开会变成 `/target`） |
| 下一次命令看不到上次的日志 | **`/tmp` 是逐命令 tmpfs** | 日志一律写工作区 `.rea-work/` |
| `pkill -f Godot` 把自己那条命令也杀了 | 命令行里含该字符串 | `ps -eo pid,args` 过滤后按 PID 杀 |
| `ss -tln \| grep godot` 匹配不到 | 不带 `-p` 就没有进程名 | 用 `ss -tlnp` |
| frida 注入报 binding 缺失 | REA 自带 frida 缓存是坏的 | `cd .rea-runtime && npm install-scripts approve frida && npm rebuild frida --foreground-scripts` |
| Godot 里改的脚本不生效 | `project.binary` 覆盖了 `project.godot` | 删掉 `godot-live/project.binary` |
| harness 里机器人不开火 | 回合时间太长 | 探针按 `TurnTimeButton` 缩短回合时间 |

Godot 统一环境变量：

```bash
XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
```

## 3. 地址与坐标约定（错了会静默出错）

- **Ghidra 地址 = ELF 虚地址 + `0x100000`**；REA / frida 用虚地址。
- `.rodata` 段里 **文件偏移 == 虚地址**；`.text` 段两者差 `0x1000`
  （换算走 ELF 段表：`vaddr = sec.addr + (off - sec.offset)`）。
- 函数起点用 `analysis/funcstarts_ghidra_va.pkl`（已是 Ghidra 地址空间）。
  **`analysis/funcstarts.pkl` 是错的，永远不要用。**
- 坐标：`px = 20x + 500`、`py = 20y + 300`，画布 1000×600。

## 4. 已完成且有真值验证

| 行 | 关键证据 |
|---|---|
| 函数文法 / 求值 / 采点 | 真二进制对拍；真实对局轨迹 69/69 逐点一致 |
| 坐标 / 掩膜 / 光栅化 | 与 `0x4be3e0`、`0x4f11e0`、`0x4f15b0` 逐字节/随机点一致 |
| 构建 / 加载 / 类注册 / 打包 | 真 Godot 4.7.2 跑通；`.pck` 能从包启动 |
| 资源还原 | 692 文件 md5 全符 |
| 网络协议 + 编解码 | 46 种消息；序列化**逐字节一致** |
| 弹道与命中结算 `settle::resolve` | 整关回放（4 发 / 6 发）误差 < 采样步；致死集合逐发一致 |
| 命中与生命 | 穿过即死、不挡弹道；**无血量 / 有友军误伤 / 按弹道先后**（`docs/65`） |
| 回合状态机 | 6 状态 7 事件，13 次迁移逐个相同 |
| tick 时钟 | 60 Hz；`end_of_turn_tick = 回合开始 tick + 时长`，六档实测 |
| 障碍延迟删除 | `QueueDestroyObstacle → DestroyObstacle` **165 tick** |
| 关卡生成 | 公式闭合 + **Rust 实现逐位复现 13/13**（`docs/61`） |
| 服务端握手 / `Match` / 一回合 12 条报文 | 与抓包逐字节/逐条一致 |
| **服务端循环（完整一局）** | `tests/server_match.rs` 真实 TCP 跑到 `EndingGame`（`docs/62`） |

## 5. 核心结论速查（省掉重新推导）

**曲线**：`parse 0x4ed670` / `eval 0x4eb9d0` / `path 0x3c2530` / `correction 0x3c5f70`。
`step` 默认 `0.01`（实况 `0.5`）、容差 `0.001`、点数 `(200/step) as i32`、
边界 `|x|≤25 |y|≤15`、修正 `c = y0 − f(±x0)`。

**掩膜**：1000×600 字节，`idx = py*1000+px`，`!= 0xFF` 命中，**越界也算命中**。
由 `obstacle_manager` 的 `0x523020` 创建（alloc 600000 + memset 0xFF + Arc）。
**摧毁 = 在命中点挖半径 0.5 的洞**（frida 抓到 `fill=0xFF` 的光栅化调用，`docs/39`）。

**协议**：UDP 广播明文 JSON 无长度前缀；TCP `u16` 大端长度 + JSON。
**同一进程里两端都在跑**，按 fd 区分方向：`fd 15` 客户端→服务端，`fd 16` 反向。

**tick**：60 Hz，`current_tick = base + round(ms*60/1000)`。
回合时长（tick）：`Timer30s=1800`、`1m=3600`、`2m=7200`、`3m=10800`、`5m=18000`，
`TimerInf` 线上发 **`0`**（哨兵）。

**关卡生成**（`docs/55`/`56`）：
`n = trunc(N(0,1)*7+15)`；`r = max(N(0,1)*1.6+2.6, 0.8)`；
`x = (2r+25)(2u−1)`、`y = (2r+15)(2v−1)`，`u,v = (u64>>11)·2⁻⁵³`。
`N(0,1)` 是 **ziggurat** 采样器（表在 `.rodata`：`X@0x25b428`、`F@0x25ac20`）。
生成从随机流**第 3 个**数开始；重放要消耗**不定数量**的随机数（拒绝采样）。

**RNG**（`docs/53`/`57`）：ChaCha（块函数 `FUN_00850f30`，虚地址 `0x750f30`）。
hook **`0x47f700`**（取随机 u64）即可读状态：`arg0` 是状态指针，
`[0..8]` = 32 字节 key、`[8..10]` = 计数器。播种**惰性**（首次调用前全零），
种子来自 **OS 熵**（游戏自己的 getrandom 包装 `GW+0x907c66`）。
**不同线程各有独立实例**（一局里能看到 2~4 条流）。

**AI**（`docs/66`/`67`/`68`）：GW1 的进化算法，GW2 改了参数与适应度。

- 入口 `FUN_0061b440`（虚地址 `0x51b440`），签名
  `(ctx, ?, player_id, soldier_id, 代数, a5, ?, ?)`；**实参可直接 hook 出来**
  （`tools/hook_ai_args.js`，System V：rcx/r8/r9/栈）
- **每回合评估 = 代数 × (种群 + 3)**；种群 = `a5 × 5`；每代末尾对前 3 名用步长
  `0.01` 精评（粗评步长 `0.5`，即 `0x3fe0000000000000`）
- 六档参数（**实测到的实参**，不是推断）：

  | 难度 | 机器人 | 代数 | 种群 | 每回合评估 |
  |---|---|---|---|---|
  | 1 | ChatGPT | 3 | 50 | 159 |
  | 2 | Wolfram | 10 | 100 | 1030 |
  | 3 | Alpha Zordon | 25 | 150 | 3825 |
  | 4 | Multivac | 100 | 200 | 20300 |
  | 5 | Deep Blue | 250 | 250 | 63250 |
  | 6 | Deep Thought | 1000 | 250 | 253000 |

- 栈上第一个参数恒为 **5**（精英数，与 GW1 的 5 一致）
- 适应度：`命中×2e6 − minDist²/100 − (步数超限 ? 5e7 : 0)`
  （`2e6` 在 `.rodata 0x19fc00`、`1e6` 在 `0x19fb30`，引用点全在 AI 区域）
- **⚠️ 有两种函数模式**：`NormalFunction`（y=f(x)）与 `DiffEqFunction`（y'=f(x,y)）。
  权重表只在变量槽位上不同：Normal 下 10 个权重全给 `x`；
  **DiffEq 下 `x` 6 个 + `y` 4 个**（实测 6.83% / 4.86%，`docs/71` §5.0）。
  `y'` 两种模式都不产生。Rust 侧已用 `aigen::VarMode` 区分
- **token 权重表已从二进制直读**（`docs/71`，`src/aigen.rs`）：
  `idx = random % 85`，区间权重
  `const 11 | x 10 | + 10 | - 5 | * 11 | / 7 | ^ 2 | sqrt 1 | log 1 | ln 1 | exp 6 | abs 7 | sin 7 | cos 5 | tan 1`，
  常数 = `N(0,1) × 10.2`。**运行时 40 万样本逐项吻合到小数点后第二位**
  （`tools/hook_token_tags.js`）
- **变异算子**（`docs/73`，`FUN_005ee720`）：挑一个值 token →
  `y` 插常数；`const` 一半概率 `*= (1+N(0,1))`（**乘性漂移**）、一半换新；
  `x`/算子则包一层 `(op const v)`（op 按 50/25/25 取 `+`/`*`/`/`，**这条是推断**）
- **长度公式两种模式共用**（`docs/76` §2.6）：DiffEq 下重测均值 9.27（去掉 0/1 后 9.79
  ≈ Normal 的 9.81）；直接量公式输出可见两种模式都会抽到 0/1/负数
- **`limit` 与模式无关**（`docs/80`）：DiffEq 下实测仍是 5/5/…/200；
  低难度会发射超限函数是因为**长度均值 10 ≫ L1 的 limit 5**（P(>5)≈84%），
  而 `V>limit` 只是扣 5e7 的**惩罚不是过滤**
- **随机函数长度**（`docs/76`，已从二进制读死）：**`trunc(N(0,1) × 5.0 + 10.0)`**
  （`0x4ef08e` mulsd 5.0、`0x4ef096` addsd 10.0、`0x4ef0b2` cvttsd2si；
  中间的 `maxsd/minsd` 是 ±2³¹ 的溢出保护，不是语义上下限）。
  已实现为 `aigen::random_length`
- **适应度**（`docs/74`）：`命中×2e6 − 1e6 − V²/100 − (V>limit?5e7:0)`，
  **`V` = 该函数的 token 数**（运行时 12/12 命中栈上 `[rsp+440]` 的向量）；
  **常规对局里没有友军惩罚项**（`X` 是采样步长 0.5/0.01，与它相乘的掩膜累加器恒为 0）
- **每代构成**（`docs/75`，实测 8 代）：子代工厂 `0x4ee3a0` 的入口恒为
  **0.8 × 种群**；工厂内部：随机整数为负（~50%）走变异，否则按第二父代长度
  `< 20` 决定"重生随机函数"还是"交叉+变异"；**剩下 20% 不经过工厂 ⇒ 直接复制**
  （含 5 个精英）。⇒ GW1 的 5/25/20 三分结构**不适用**
- ⚠️ `docs/68` 曾据 149 个样本推断"token 集不含 sqrt/ln/^"，**已被 `docs/71` 推翻**
- 观察到 `1770417.48` 这种超大常数 ⇒ 存在**微调变异漂移**（GW1 的 `mutateFineTune`）

**探针**：`godot-live/probe.gd` 支持 `GW2_BOT_OPT=1..6`（六档）与 `GW2_TT_PRESSES`（回合时间）、
`GW2_START_FRAME`（延迟开局）、`GW2_SCENE_DUMP`（场景节点 dump）。

## 6. 明确未知 / 已证不可复现

| 项 | 状态 |
|---|---|
| 回合出场顺序 | **已证伪来自 ChaCha 流**（6 局 × 2 基准 × 4 方法；3 局 × 每流 × 3 方法 × 25 偏移）。最可能来自 **HashMap 迭代顺序**（SipHash 键由 getrandom 播种）⇒ 不可确定性复现（`docs/59`/`60`） |
| ~~`V` 的语义~~ | **已确认**（`docs/74` §5）：`V` = 被评分函数的 **token 数**，运行时 12/12 命中栈上 `[rsp+440]` 的 token 向量；`−V²/100` 是复杂度二次惩罚，`V>limit` 的 `−5e7` 是复杂度硬上限 |
| 20% 副本里除 5 精英外的选取规则、`cmp` 只看第二父代的原因 | 未定（`docs/75` §4） |
| 变异 `y` 分支会产生"两个值相邻"的非法前缀式 | 未解（可能调用方还有一趟修复，见 `docs/73` §5.1） |
| 关卡生成的其他模式 | **5 个候选全部有了运行时定性**（`docs/72` §5.1）：散布 `0x3f1e50` 开局调 1 次（真生成器），`0x465a70`/`0x46f690` 紧跟其后（流程辅助），`0x464d10`/`0x478fc0` 在开局前（与关卡无关）。圆盘式/中心式在默认对局**从未被调用**，触发条件未知；Rust 已实现散布 + 中心单障碍 |
| 玩家颜色分配 | 观测到 10 个取值、有重复；既非纯随机也非按 id 绑定，规则未定（`docs/34`） |
| 玩家加入流程 | 服务端只实现了单连接简化版（`docs/62` §4） |
| 自建客户端连真服务器 | 端口找到（46868）但握手无应答；**很可能当时探针脚本有解析错误、游戏根本没开房**（`docs/64`，需重试） |

## 7. 正在进行：AI 与关卡

已达成：算法、适应度、六档参数、机器人名单、**token 权重表**、**变异算子**、
**随机函数长度**、**V 的语义**、**每代构成**，并已落成
**结构还原版 AI**（`src/aigen.rs`：`params`/`random_length`/`mutate`/`crossover`/
`next_generation`/`evolve`/`fitness`/`initial_population`，18 条测试含端到端"6 代学会命中"）。

剩余：

1. ~~变异/交叉的个体数~~ **已完成**（`docs/75`：80% 工厂 + 20% 复制）；
2. ~~`-5e7` 阈值~~ **已完成**（`docs/74`：5/5/10/15/25/200，随难度放宽）；
3. ~~随机函数的长度规则~~ **已完成**（`docs/76`：`trunc(N(0,1)×5+10)`，且两种模式共用）；
   **装配机制也查清了**：没有"修复"过程——`simplify`(`0x4ebbe0`) 是递归下降解析器，
   从头读一个完整表达式、**多余的 token 丢弃**（`docs/76` §4.5）；
4. 关卡的圆盘式 / 中心单障碍两种模式**尚未写进 `src/levelgen.rs`**。

**批量提取真实函数文本**已打通：`tools/ocr_functions.py`（用户装了 `tesseract-data-eng`）。

## 7.5 统计验收（`docs/85`）

真实机器人函数的一元算子频率与权重表吻合（abs 10.2/10.9、sin 10.2/10.9、cos 7.7/7.8）；
二元算子因**进化选择压力**偏高/缺失，**不能用来验证权重表**。
权重表的可靠证据仍是二进制直读 + 40 万次运行时抽样。

## 8. 我犯过的错（别再犯）

**地址 / 反汇编类**

1. 把虚地址当 Ghidra 地址传工具 → `not_found`（差 `0x100000`）。
2. 把文件偏移当虚地址 → 掩膜归属算错（`.text` 差 `0x1000`）。
3. grep 反编译文件里的浮点常量 → 被文件头版本号 `12.1` 污染。
4. 相信 `analysis/funcstarts.pkl` → 与 Ghidra 交集仅 2/13430。
5. 从局部伪代码推取整方式 → 把"截断"说成"四舍五入"；必须用输入输出对拍。

**结论 / 证据类**

6. 只看系统调用次数、没看**调用栈** → 把 libudev 的 921 次 `getrandom` 当成游戏的，
   得出"OS 熵 ⇒ 关卡不可复现"的错误结论（`docs/40` 已被 `docs/50` 推翻）。
7. 用 3 局样本得出"命中"就下结论 → 扩到 6 局立刻消失，是统计噪声。
   **小样本一律先算假阳性期望**。
8. 凭函数内部特征猜用途 → 把 `0x567530` 猜成"AI 选弹道"，实际是关卡通视检查。
9. 把"障碍被整块移除"当成机制写进文档 → 被数据否定后才改成"挖洞"（`docs/38`→`docs/39`）。
   **推断必须标明；能用实验否定就要去否定。**

**工具 / 脚本类**

10. 一次 `str.replace` 改文档没核对 → README 索引只更新一半、某行没改成功（静默失败）。
    **每次替换后都要 grep 验证**。
11. `/tmp` 当持久目录 → 下一条命令就没了。
12. `pkill -f <名字>` → 连自己那条命令一起杀。
13. GDScript 的 `elif` 顺序写错（`frames >= 600` 先匹配，`frames == 720` 永不执行）。
14. 探针脚本有解析错误 → 游戏照常"运行"，但**整局根本没开始**；我却在对着空环境调网络。
    **先用一条 print 确认脚本真的加载成功。**
15. 用 hook 计数做性能/预算结论 → 难度 6 要拦 25 万次评分，**instrumentation 把游戏拖慢**
    导致测到的数是被自己截断的（`docs/67` §2）。**先确认 hook 不改变被测行为。**
16. AI 参数靠"评估数整除"反推（虽然后来对上了），正解是**直接 hook 实参**
    （rcx/r8/r9/栈），一步到位。**能读输入就别猜输出。**
17. 用户录屏里右下角数字被我当成回合计时，实际是**视频总时长**。**图上读数不可靠，
    要问具体数值。**
18. 用 149 个 OCR 样本里"出现 0 次"推断"算子不存在" → 被二进制打脸（`docs/68` 被 `docs/71` 推翻）。
    **低频事件在小样本里为 0 是常态，不能当否定证据。**
19. 直接拿函数起点表里的地址去 `Interceptor.attach` → 可能不是指令边界，**attach 会抛异常**，
    且**异常会中断同一段脚本里后续的 attach**（我用一个 try/catch 都没包的脚本踩到）。
    **每个 attach 都要单独 try/catch。**
20. 把 Ghidra 地址当虚地址去 hook → 命中了另一个每帧都调用的函数（3514 次），
    差点得出"散布生成器每帧都在跑"的相反结论。**这是第二次踩同一个坑（第 2 条也是）。**
    现在工具脚本里一律写注释标明"虚地址 = Ghidra − 0x100000"。

## 9. 文件地图与常用命令

```
src/function/   文法与求值        src/protocol.rs  46 种消息 + 序列化
src/path.rs     采点              src/settle.rs    命中与致死结算
src/plane.rs    坐标/掩膜/光栅化   src/game.rs      Match（一回合报文序列）
src/shot.rs     修正+开火原语      src/turnstate.rs 回合状态机
src/tick.rs     60Hz + 回合时长    src/levelgen.rs  关卡生成（含 ziggurat）
src/rng.rs      ChaCha12          src/serverloop.rs 服务端循环
src/server.rs   握手/状态同步      src/gdext*.rs    Godot 扩展
tests/          events / evidence / golden / live_hits / plane / server_match
tools/          pck_extract / mask_check / simulate_check / hook_* / capture_* / predict_obstacles
docs/           00 状态总览、16 复现方法、49 验收清单、62 服务端、66 AI
```

```bash
export CARGO_HOME=$PWD/../.cargo-home
export CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo test --offline                      # 113 条，无需游戏二进制
cargo run --example serve_match -- 127.0.0.1:39993 &
python3 tools/client_match.py 39993       # 打完整一局

node tools/capture_rand_values.js         # 抓随机流 + 真实报文
python3 tools/predict_obstacles.py        # 关卡重放 → 应输出 "完全一致 13/13"
```
