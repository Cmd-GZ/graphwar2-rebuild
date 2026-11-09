# HANDOVER —— 给上下文压缩后的自己

> **这是一份 cache，不是历史。** 目的只有一个：让被压缩掉上下文的"我"能在五分钟内
> 恢复判断力，不重复已经走过的死路。**只放"现在必须知道"的东西**；过程与逐例证据在
> `docs/06`（验证）、`docs/02`（模块）、`docs/00`（状态总览）、`dispatch/reports/`（每轮报告）。

---

## 0. 一句话任务（改不了）

**还原 Graphwar II 的源代码**（不是造克隆）。原版是 Rust + Godot 4.7 GDExtension，
`libgraphwarrust.so`（sha256 `9a538115…`，stripped）。**行为一致只是第一步，源码级还原才是终点。**
判据见 `AGENTS.md` §0（六条硬标准）——尤其：**`cargo test` / `selftest` 的条数是回归保护，不是复原证据。**

我的角色：**派活 + 合并前审查**（`AGENTS.md` 合并纪律 A/B/C/D）。分析与实现派给子 agent 或下游。

---

## 1. 现在的数（报数**必须带轴名/前置条件**，否则等于没报）

```
可归属函数   527 / 527 = 100%（逐位 88 / 观测 439 / 推断 0）｜78 模块｜未做 0
             但这只是 13,263 个 Ghidra 函数里的 4.0% —— 见 §3「信息天花板」
真名         348 / 527（三份符号表并集）；无名 179 = 138 族级 + 41 未知
协议         74 / 74 全字段 + 74 条判别值 + 7 个线上枚举全集（无未知）
GDExtension  **原版 12 类**（`docs/02` §16.2）+ 我方 3 = **15 个，全部节点化**；headless 14 个 SceneTree 脚本 / 1827 条 / 0 failed
cargo        1710 / 0（`--features gdext-bindings` 1816 / 0）
selftest     714 / 0 / 1 skip（**前置**：`client/assets/` + 编好 `examples/serve_match`；
             没有 serve_match 时 636 / 2 skip；缺 assets 掉到 591–667）

行为一致     fire **647** / batch **647** / batch_conv **637**（分母都 1080，内容命中）
种类序列     三轴都 **1075 / 1080**（≠ 进度：只说明消息种类顺序对）
延迟段       死亡 **366 / 385（漏报 0）**、挖洞 **207 / 294**（batch 轴）
可及上限     悲观 **775** / 乐观 **869** / **硬上界 869**（= 1080 − 211 批已证与我方无关）
             三界怎么算的见 `docs/00` §0.1b.1（可复跑）
```

**两张最容易混的口径**：`527 vs 13,263`（可归属 vs 全部函数）、`1075 vs 647`（种类 vs 内容）。

---

## 2. 已经**关掉**的路（别再花时间，这是本文件最值钱的部分）

每条都做过实验、都有数。**引用任何"假设"之前先看这里。**

| 方向 | 判决 | 关键证据 |
|---|---|---|
| "老版本 replay 打的所以对不上" | **否** | 用户新录**当前版本**的 bot-vs-bot，一样差 1 ulp；unicorn 直调真 `.so` 给出的是**我方那个值** |
| "回放自身不自洽"（407 批） | **成立但被收窄** | 只有 **211 批**是值级互斥（任何忠实实现都产不出）；其余是机制（两次读、同批挖洞） |
| "换个点 / 换精度 / 换步长" | **否，一整族** | 29 档假设全扫：**没有一档超过 647，fix 全 0**，且量级不匹配（差 1e-9 vs 换点差 0.01） |
| "打印-回代链丢了精度" | **否** | 167/280 批是 DiffEq，`corrected_function == function` **184/184 相同** ⇒ DiffEq 局根本没有常数 |
| "修正常数 token 差 1 ulp" | **否** | 三样同抓：常数 token vs 真机印字 **21/21、0 ulp**；数组未被改 20/20 |
| "1 ulp 出在 AI 开的那一发" | **仍未复现** | LAN+AI 局造出来了、AI 真开火；**直接量**"喂进 `0x3c5f70` 的 `&Point`" = `start_x`，**3/3、0 ulp** |
| "补间接调用图能提准确率" | **否（边际 0）** | 判别力 −0.126 → **+0.696**，但叠到 REF：78.2 → 77.0（p=0.5） |
| "加特征 / 调权重能再涨" | **否** | 24 特征里 **7 个拿掉后 top-1 反而涨**；3,000 组配置只比 24 组多 3 个样本；边际只在 `gram`（−16.63 点）与 `scal`（−2.49），`size`/`consts` **纯冗余** |
| "扩样本能压窄置信区间" | **部分** | 348 → 975、CI ±6.36 → ±3.02，但新增 **622/629 是依赖 crate**；**游戏代码仍 n≈169、±6.4** ⇒ 瓶颈是**锚**不是名字 |
| "重叠/串发导致整批崩" | **否** | 89 发交叉命中 0/178；真因是**同批更早爆炸的洞已进掩膜** + `end_x` 早一个采样点 |
| "角色推断能给无名函数定角色" | **否** | 正对照 40.5% **< 平凡基线 44.3%** |
| "换证据类型就能再涨"（crash 栈 / campaign 表 / 真机帧） | **三条都不通** | crash 栈：做匹配的那份**无 `.symtab`/`.debug_*`**、仓库 0 份崩溃产物 ⇒ 新锚 0；真机帧：仓库 0 帧、能给的量已被 `.rodata`/`client/`/campaign 覆盖；campaign 表：**能锚但只能当锚**（覆盖率 4.62% vs 0.05% = 84× 偏斜，塞成特征就是 `strs` 同类错误），留出半一格没动 |
| "换权重/换配置能再涨"（63 号到 3,000 组） | **否（不显著）** | 3,000 组调参半只比 24 组多 3 个样本；预注册口径仍 **76.4%** |

**已被证实、且已落地的**（不用重做）：
`报告顺序 ≠ 结算顺序`（批内按**到达序**逐发挖洞，S3，三轴未退、净 2 批变好，内核与收敛层**共用一份实现**）；
`打不全就全部复原`（部分命中 ⇒ 回合末**整组**靶子广播 `alive:true`；全清 ⇒ 不复原）；
`pos_y` 只有两条写入指令、一条 16 字节 `movdqu` **同时写 x 和 y**。

---

## 3. 信息天花板（不是没干，是物理上到顶）

```
12,736 / 13,263 个函数**一个 src/*.rs 的 panic::Location 都不引用** ⇒ 没有名字可锚
   · 结构指纹只能给"同源候选"：预注册 **76.4%**（`scal=0.25`）/ 并列桶最高 78.2%，CI [72.12, 84.24]（n=165）
     ⚠️ **引用 76.4**；78.2 只是"23 组并列桶的最高值"（`docs/160` 复核）→ 口径块在 `docs/02` §35 开头
   · 换编码已到顶（71 号：边际只在 `gram`/`scal`，`size`/`consts` 纯冗余）
   · **换证据类型也试过了，三条全不通**（crash 栈 / campaign 表 / 真机帧，见 §2）
   · 已知的**族内失效**：关卡闭包族 **14/27 = 51.9%** vs 族外 407/454 = 89.6%（p=2.6e-6），错 19/19 全在族内
     ⇒ 这一族结构上分不开，只能靠 campaign 表那种非结构证据（覆盖仅 27 行）⇒ **接受现状**（`docs/02` §35.12）
· 服务端撮合逻辑：客户端二进制里**没有**（我们 `src/_scaffold/broker.rs` 是自建的，不是复原）
· 美术**源文件**（`.svg`/`.ttf`）：包里只有产物（167 `.ctex` / 7 `.fontdata`）。
  **但图能用** —— `.ctex` 内部就是 WebP（实测首字节 `GST2…RIFF…WEBP`），`client/assets/` 已在用
```

---

## 4. 还剩什么（按"能不能靠干活推上去"分档；权威表 `docs/00` §0.1b.2）

| 档 | 内容 | 备注 |
|---|---|---|
| **Ａ 缺工程** | ① `client/src/screens/*.gd` **≈6,062 行**仿制流程（原版控制器在 Rust 里）；② **6 个 manager 仍是 `assembled`**（**14 wired / 2 layout / 6 assembled / 0 stub = 22**）—— Rank / Name / LocalPlayer 已升档（74 号），余下六格见 `docs/161`，**GSM 卡 `src/game.rs` 禁区别派**；③ `ShotLifecycle` 文件归属（原版在 `game_state_manager`） | 证据齐了只差写 |
| **Ｂ 缺样本（易得）** | 目前**空**（Level 18–20 / flip=1 / 部分命中 都已补齐） | 找人打一局即可 |
| **Ｃ 缺样本（难造）** | 1 ulp 正主 —— 但目前证据表明**它可能根本不在链上**，别再立项 | — |
| **Ｄ 缺真值（可运行时挖）** | `drive_frame` 的**节奏真值**（钩 `0x518400` + 泵函数数调用）；libm 边界 | 开游戏 + frida |
| **Ｅ 物理不可得** | 服务端撮合、`.svg`/`.ttf` 源、12,736 个名字 | **别立项** |

**当前在派的两件**（都是 Ａ 档）：
**① manager 升档**（74 号完成 Rank / Name / LocalPlayer；77 号在做 Color / Soldier / Graphics / FHR）；
**② 跑得起来的那几块工程**：`selftest` 的 flake 修复（76）、回放驱动搬进 Rust（81）、
`drive_frame` 节奏真值（80）、LocalPlayer 第五道门（79）。

---

## 5. 干活时必须遵守的（都是踩出来的）

### 合并 CG 之前（`AGENTS.md` 合并纪律 A）

```bash
python3 tools/merge_check.py --cg origin/CG     # ①报告 §遗留/§建议全文 ②范围/禁区 ③污染 ④四件套
```
- **必须读报告的 §7/§8（遗留/建议）**——跳过它我漏过"准备圈画面上没有了"这种真回退；
- 合并命令要剔 `dispatch/`：`git merge --no-commit origin/CG && git rm -r -q --cached dispatch && rm -rf dispatch`；
- 合完 `head -1 docs/HANDOVER.md` 必须是「给上下文压缩后的自己」（CG 会静默覆盖，已发生三次）；
- 生成物冲突（`docs/coverage-matrix.*`）重跑 `python3 tools/coverage_matrix.py` 而不是手改。

### 合并之后（纪律 B）

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<WS>/.cargo-home/target
cargo test --offline                                  # 1694
python3 tools/coverage_matrix.py --check
python3 tools/check_doc_refs.py
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive batch --policy default --json b.jsonl
python3 tools/bit_exact_attrib.py --json f.jsonl --check-baseline --axis fire
bash client/run.sh selftest                           # 714 / 0 / 1 skip
```

**headless 巡检**只喂 `extends SceneTree` 的脚本（`client/tools/*_check.gd`，14 个）：
拿 `plane_on_screen.gd` / `screenshots.gd` / `selftest.gd` 去 `--script` 会**弹 ALERT 并留僵尸实例**，
后续结果被污染（我踩过：selftest 报 704/4，干净重跑是 714/0）。

### 三条硬约束

1. **护栏语料必须冻结**：三轴用 `.agent-work/replay-snapshot/` 的**固定 401 份**；
   新样本放 `replay-new*`。混进去三轴数字全变（401→403 那次：647→654、1080→1094）。
2. **报数必须带轴名与前置条件**（§1 那几条）。
3. **证据三档来源**（`AGENTS.md` §1b）：主证据=二进制；观测证据=真实报文/回放/真机；
   **玩家说法只是线索**（能指方向，不能进结论）。

---

## 6. 环境与地址（错了会静默出错）

```
地址          Ghidra/REA vaddr = ELF vaddr + 0x100000（`0x3c2530` ↔ Ghidra `0x4c2530`）
              读 `_DAT_xxxx` 原始字节要减 0x100000
引擎调用      gdext 走 **GOT 槽**：`call *0x…(%rip)`；只看 `call rel32` 会得出"原版不画任何东西"
              实例：draw_arc 字面量 0x19f9d0 → 包装 0x8abca0 → 槽 0x9bc020
函数归属      `panic::Location` 在 `.data.rel.ro`（指针是重定位）；解析 `.rela.dyn` 才能还原
函数起点      用 `analysis/funcstarts_ghidra_va.pkl`；**`funcstarts.pkl` 是错的**（.eh_frame，与 Ghidra 只交 2 个）
GPU 口径      比"计算时间"还是"进程墙钟"？（后者含 `import torch` ≈5 s）
              实测：全量 252×；小规模内部 4.83×、进程墙钟 1.25×。**报 GPU 必须同时给口径**
```

**"改了但没生效"的三个静默陷阱**：新增 `.gd`/`.gdextension` 后没 `--import`；
`cargo build` 后没 `cp` 进 `client/`（客户端跑旧 `.so`）；残留旧的 `target/…/serve_match`。
`/tmp` 是逐命令 tmpfs，临时产物放工作区。

---

## 7. 我这一程犯过、已经写进纪律的错（别重犯）

```
① 跳过报告的 §7/§8 就合并 ⇒ 漏掉"准备圈画面上没有了"（用户发现）
② 把"某条轴的读数"记成另一条轴 ⇒ 1074 是 batch_conv 不是 batch（AGENTS 合并纪律的来源）
③ 拿局部观测当普遍结论三次 ⇒ "61 是唯一适合 GPU 的"/"63 是唯一需要 GPU 的"/"小矩阵 GPU 亏"
   ⇒ 真原因第一次是"口径混用"、第二次是"接口形状"，都不是"规模"
④ 算错量级 ⇒ 0.37578125 我写成"37.5 个步长"，实际是 **166 个采样点**
⑤ 把新样本混进冻结快照 ⇒ 三轴数字全变（已修：快照 401 份 + 指纹登记）
⑥ 拿"并列桶最高值"当新纪录 ⇒ 78.2 是 23 组里挑最大，预注册值是 76.4
⑦ 用 helper 脚本喂 `--script` ⇒ ALERT 弹窗 + 僵尸实例污染后续 selftest
⑧ README/文档写成 git 日志 ⇒ 文档只写"现在是什么"，历史归 git
```

---

## 8. 现在的协作状态

```
上游（本机）  派活 + 审查 + 合并 + 文档           16 核 / 15 GB / 无独显
下游（CG）    执行任务，提交到 CG 分支            32 GB / Ryzen 9 7945HX(16C32T) / RTX 4060
              dispatch/ 只在 CG 上；**master 里一份都没有**（每轮合并必剔）
在跑的        第十六轮 7 份：76 selftest flake（P0）｜77 manager 再升 4 格（P0）｜78 特征瘦身
             79 LocalPlayer 门 + Computer state｜80 drive_frame 节奏真值｜81 回放驱动｜82 1ulp 收敛解
关键文档      docs/00 状态总览 · docs/02 模块图 · docs/06 验证记录 · docs/161 manager 缺口
             docs/158/159/160 三份对抗性审计 · docs/guide/ 攻略手册
```
