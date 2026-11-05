# gw2-rebuild —— Graphwar II 源码复原

本仓库目标：基于 `libgraphwarrust.so` 的静态逆向结果，**复原 Graphwar II 的实现并重写成可编译的源码**，
同时把"游戏到底怎么运作"讲清楚。

与上游 `graphwar-algorithm-analysis` 的关系：那是**逆向报告与对拍工具链**（`REPORT.md` +
`analysis/` + `graphwar_curve.py`）；本仓库是**实现本体**，独立托管在
`Cmd-GZ/graphwar2`，与上游仓库分离、互不包含。

## 目标二进制

| 文件 | 角色 | 规模 |
|---|---|---|
| `libgraphwarrust.so` | GDExtension（Rust + godot-rust），**全部玩法逻辑** | 10 MB，stripped；函数边界以 Ghidra 的 **13503** 条为准（13263 条落在 `.text`）。`.eh_frame` 解出的 13430 条**不可用**（`docs/02` §12） |
| `Graphwar.x86_64` | Godot 4.7 导出模板 | 73 MB |
| `Graphwar.pck` | Godot 资源包 | 105 个 `.gdc`（仅 UI）+ **31** 个场景 + 692 个资源文件 |

关键约定：**Ghidra 镜像基址 `0x100000`**，文件地址 `0x3c2530` 在 Ghidra/REA 里是 `0x4c2530`。

## 进度（按**重建工作包**组织）

判定标准：**✅ 逐位 / 🔶 部分 / ⬜ 未做**。

> 这里不再按"模块"列，而是按**重建这个游戏需要哪些工作包**列。
> 之所以改：按模块列会让人以为"全绿 = 能玩"，而实际到 2026-11 为止，
> **界面与脚本这两块（共 31 个场景 + 105 个 `.gdc`）进度为 0**，它们才决定"能不能玩"。
> 「重建还缺什么」一列只写**具体缺的东西**，不写"待完善"。

| 工作包 | 状态 | 证据（一句话 + 现行文档） | 重建还缺什么 |
|---|---|---|---|
| 函数语言与求值器 | ✅ 逐位 | 文法 90 例 + 37 夹具、求值 70 夹具与真二进制对拍；`^` 左结合、`log`=log10、隐式乘、缺操作数补 `CONST 0` 等怪癖全部落码（[03](docs/03-function-language.md)、[06](docs/06-verification.md)） | 算法无缺口，只剩界面侧的输入框/预览（见"界面""脚本"两行）。注：第三变量 `y'` 在全部 7 个求值调用点都传 `0`，是**游戏原样**，不是缺陷 |
| 弹道采样 | ✅ 逐位 | 补上 `hi = mid2`（[91](docs/91-bisection-loop.md)）后，**24/24 用例整条点序列与真二进制逐位相等**（修复前 17/24）；(401, 24.8098, 9.8719) 与 (20001, 23.2128, 6.4176) 两条验收同时命中（[89 §32](docs/89-real-step.md)）。另有真实对局抓样 69/69 逐点一致（那次抓样的 `step=0.5`，非实况，[23](docs/23-live-verification.md)） | ① `tests/live_hits.rs` **仍留在 0.5**：切到 0.01 后 4 个测试失败的矛盾**未解**（[89 §7](docs/89-real-step.md)）；② `chord2 = NaN` 那条分支与汇编的等价性**未实测**（[91 §9.1](docs/91-bisection-loop.md)）；③ `step≈50` 的越界参数域外仍多收 1 点（游戏不可达，[04 §7](docs/04-path-sampling.md)） |
| 障碍掩膜 / 碰撞 | ✅ 逐位 | 光栅化与 `is_occupied` 与真二进制**逐字节 / 300 随机点一致**；摧毁=命中点挖 `r=0.5` 的洞（frida 实证，[39](docs/39-punch-hole.md)） | ① 82 发里 **1 例**圆边像素个案未查清（[86 §9.2](docs/86-simultaneous-turns.md)）；② **通视检查**（`0x567530`）只到"观测"，未实现；③ 掩膜的持有者与重建时机未观测 |
| AI 进化 | 🔶 部分 | 权重表（合计 85）/ 长度 `trunc(N(0,1)*5+10)` / 六档参数 / 装配 / 交叉 / 变异 / 每代构成 = 反汇编 + 40 万次运行时抽样；适应度公式逐条反汇编（[AI.md](docs/AI.md)） | ① **搜索过程不可逐位复现**（RNG 取 OS 熵）⇒ 永远只能统计验收；② **AI 有没有时间预算未知**（若有，难度 6 的 1000 代跑不完）；③ **"精英 5" 的用途未知**；④ `AxisMode`/`TimeMode` 是否参与判定**未验证**；⑤ DiffEq × AI 只验了变量槽权重，完整进化未验 |
| 关卡生成 | 🔶 部分 | 随机散布模式 Rust 实现**逐位复现 13/13**（障碍 `x/y/r` 的 `to_bits()` 全等，[61](docs/61-levelgen-rust.md)）；公式与 ziggurat 表见 [55](docs/55-level-generator-closed.md)/[56](docs/56-levelgen-replay-complete.md) | ① **圆盘式**（`0x5ea6dc`）未实现（半径语义存疑）；② 三种模式的**触发条件未知**（默认对局只见到散布，协议里也没有模式字段）；③ Campaign 21 关的**关卡数据与逐关解锁流程**未做 |
| 网络协议 | ✅ 逐位 | 46 种消息全部解析；`to_json` 与 4 类消息**逐字节一致**；握手 7 条、一回合 12 条报文的种类与顺序与抓包一致（[24](docs/24-network-protocol.md)、[32](docs/32-netcode-implementation.md)、[46](docs/46-full-turn.md)） | ① 玩家**颜色分配规则未定**（[34](docs/34-color-assignment.md)）；② 玩家加入流程是**单连接简化版**（[62 §4](docs/62-server-loop.md)）；③ **自建客户端连真服务器从未成功**（端口找到但握手无应答，[64](docs/64-join-real-server-attempt.md)，需重试）；④ Global/大厅/匹配这一层没碰 |
| 回合 / 生命 / 结算 | ✅ 逐位 | 穿过即死、不挡弹道、无血量、友军误伤；回合状态机 6 状态 7 事件 **13 次迁移逐个相同**；同时回合制 = 几何并行 + 事件按飞行时间（修复后 82 发并集 **152/152**、时刻判决 **38:0**，[86 §14](docs/86-simultaneous-turns.md)） | ① **回合出场顺序不可确定性复现**（最可能是 HashMap 迭代序，[60](docs/60-turnorder-verdict.md)）⇒ 重建版必须自定规则并声明；② 目标选择 `target_manager` 仍是 SIMD 未解（[14](docs/14-target.md)）；③ `AxisMode`/`TimeMode` 是否参与判定未验证 |
| 服务端循环 | ✅ 逐位 | `serverloop::run` 在**真实 TCP** 上跑完整一局到 `EndingGame`；延迟删障碍 **165 tick**；回合计数与结束判定（[62](docs/62-server-loop.md)、[44](docs/44-destroy-delay.md)、[45](docs/45-turn-count-and-ending.md)） | ① 多客户端同房未实现（现在只支持单连接）；② 断线重连 / 超时踢人 / 房主迁移未做；③ 真实 LAN/Global 大厅与建房流程未做 |
| **Godot 客户端界面** | ⬜ 未做 | 清单已从 `.pck` 枚举：**31 个场景**（`main_menu`/`campaign`/`lan_lobby`/`player_screen`/`replay_lobby`/`endgame_pop_up`…，[88 §10](docs/88-coverage-matrix.md)）。仓库里**没有任何 Godot 工程、场景或控件** | **全部界面从零建**：主菜单 4 按钮、Campaign 21 关与关卡弹窗、Battle（建房/加入 IP/四模式/加 AI 六档）、Player（改名 + Skins/Faces/Hats）、Replays 播放器、聊天与各种确认框；还有 149 个 SVG 图标与主题资源的重新挂接 |
| **脚本逻辑（`.gdc`）** | ⬜ 未做 | 清单已枚举：**105 个 `.gdc`**（+105 个 `.gd.remap`，[88 §11](docs/88-coverage-matrix.md)）。本仓库**一个都没反编译**：按钮、信号、状态推进全部未知 | GDScript 字节码反编译（→ 近似源码）；105 个脚本的界面逻辑、信号连接与状态推进；广告（`Ad_holder`/`Rewarded*`/`Interstitial`）与平台分支。**没有这一层，界面画出来也不会动** |
| **资源 / 音频 / 本地化** | 🔶 部分 | 资源还原 **692 文件 md5 全符**（`rsc/MANIFEST.tsv`，[17](docs/17-resources.md)）：167 `.ctex`、22 `.res`、184 `.import`、149 个 SVG 图标 | ① 音频只有 **1 个** `sounds/humm.wav`（无音乐、无音效库）⇒ "是否还有音频系统"**未确认**；② 包里**没有** `.translation`/`.csv` ⇒ 无本地化资源，文本硬编码在场景/脚本里；③ 资源齐 ≠ 能组装，依赖"界面""脚本"两行 |
| **构建产物** | 🔶 部分 | `cargo build --release` 出 cdylib，`gdext_rust_init` 在真 Godot 4.7.2 注册成功；导出 `.pck` 能从包里启动并加载还原的纹理（[18](docs/18-build.md)、[19](docs/19-godot-verify.md)） | 只产出**内核扩展**，不是游戏。没有可玩客户端（缺"界面""脚本"两行）；原扩展对外只有 **12 类 25 个方法**、几乎全是 UI 回调，玩法逻辑完全不开放（[19 §5](docs/19-godot-verify.md)） |

### 总结：现在离"可玩"还差哪几块

| 层 | 现状 |
|---|---|
| 内核（前 8 行） | **6 ✅ / 2 🔶**。缺的不是"算法没搞懂"，而是"没有真值可验"（AI 搜索过程/颜色/出场顺序）或"根本不可确定性复现" |
| 外壳（界面 + 脚本） | **0 进度**：31 个场景、105 个脚本，一行都还没重建 ⇒ **这一块决定"能不能玩"** |
| 交付（构建产物） | 只能构建**内核扩展**；**没有任何可玩产物** |

**一句话：现在是一个"可编程调用的内核 + 协议 + 服务端"，不是能玩的游戏。**

要变成可玩的重建版，还差三块：

1. **界面**（31 个场景）—— 从零建 Godot 工程；
2. **脚本**（105 个 `.gdc`）—— 反编译出界面逻辑与状态推进（现在这块由另一个 agent 在做清单）；
3. **三处"永不可逐位"的规则必须自己定**（回合出场顺序、AI 搜索过程（RNG 取 OS 熵）、玩家颜色分配），
   并在文档里标成"重建版自有规则"，不能冒充复原结论。

**Rust 实现状态**：`parse` / `eval` / `calculate` / `shot` / `plane` / `events` / `life` /
`turnstate` / `protocol` / `game` / `server` / `serverloop` / `levelgen` / `aigen` 均已可运行；
`cargo test` 共 **135 条**（`docs/89` §32 记录为全绿，本轮未重跑）。
模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)，共约 90 个 `graphwar_*.rs`。

## 文档

全部文档的索引（现行 / 已归档 / 已推翻）见 **[`docs/INDEX.md`](docs/INDEX.md)**。主要几篇：

| 文档 | 内容 |
|---|---|
| [HANDOVER.md](docs/HANDOVER.md) | **给上下文压缩后的人/自己：一篇替代全部** |
| [00-status.md](docs/00-status.md) | 状态总览：什么已完成、什么只观测、什么还空白 |
| **[15-end-to-end.md](docs/15-end-to-end.md)** | **端到端：从敲下函数到有人死亡（建议先读这篇）** |
| [01-architecture.md](docs/01-architecture.md) | 整体分层：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](docs/02-module-map.md) | ~90 个模块的清单、职责、行号与地址锚点；函数↔模块映射 |
| [03-function-language.md](docs/03-function-language.md) | 玩家输入的函数语言：文法、优先级、全部怪癖 |
| [04-path-sampling.md](docs/04-path-sampling.md) | 等弦长二分采样：算法本体与全部常量 |
| [05-shot-pipeline.md](docs/05-shot-pipeline.md) | 一次开火的完整链路：位置 → 修正 → 采点 → 广播 |
| [06-verification.md](docs/06-verification.md) | 如何证明复原正确；证据分级、工具链现状与已知边界 |
| [07-protocol.md](docs/07-protocol.md) | 事件枚举、字段、变体顺序与传输线索 |
| [08-plane.md](docs/08-plane.md) | 坐标变换、碰撞掩膜、y 轴方向与光栅化舍入 |
| [09-report-decision.md](docs/09-report-decision.md) | 上游报告是否拆分的决策与判据 |
| [10-lifecycle.md](docs/10-lifecycle.md) | 生命与死亡：模块规模、驱动者、两张表 |
| [11-dispatch.md](docs/11-dispatch.md) | 间接派发：用重定位表补调用图的盲区 |
| [12-ai.md](docs/12-ai.md) | AI：结构已探明，评分规则未还原（**结论已被 [AI.md](docs/AI.md) 取代**） |
| [13-turns.md](docs/13-turns.md) | 回合：规模与驱动者、无时间常量的负结果 |
| [14-target.md](docs/14-target.md) | 目标选择：核心十字路口，SIMD 未解 |
| [16-reproduce.md](docs/16-reproduce.md) | **如何验证这份复原**：跑测试、真值来源、复现环境、环境坑 |
| [17-resources.md](docs/17-resources.md) | 游戏资源还原：PCK v4 格式、rsc/ 生成与为何不入库 |
| [18-build.md](docs/18-build.md) | 构建出真实二进制：cdylib、gdext_rust_init、dlopen 验证 |
| [19-godot-verify.md](docs/19-godot-verify.md) | 在真 Godot 4.7.2 里加载注册类；原扩展 API 面只有 12 类 |
| [20-live-oracle.md](docs/20-live-oracle.md) | 活体 oracle：frida 注入真游戏进程，调用内部函数 |
| [21-oracle-limits.md](docs/21-oracle-limits.md) | **玩法 oracle 为何不可达**：四条尝试与证据 |
| [22-live-game-harness.md](docs/22-live-game-harness.md) | 让原版游戏在我们自己的 harness 里跑起来 |
| [23-live-verification.md](docs/23-live-verification.md) | 用真实对局轨迹验证采样器 |
| [24-network-protocol.md](docs/24-network-protocol.md) | **网络协议**：帧格式与 46 种消息（真实抓包） |
| [25-live-collision.md](docs/25-live-collision.md) | 碰撞链路端到端验证（整关 4 发 / 6 发回放） |
| [26-harness-control.md](docs/26-harness-control.md) | 把 harness 当玩家用：能做什么、卡在哪 |
| [27-death-flow.md](docs/27-death-flow.md) | 死亡语义与回合状态机（真值） |
| [archive/28-turn-order.md](docs/archive/28-turn-order.md) | 回合出场顺序：只有「队伍交替」稳定（**已归档**，被 `47` 修正） |
| [archive/29-ai-probe.md](docs/archive/29-ai-probe.md) | AI 内部探针：每回合 1 次 + 数千次模拟打分（**已归档**，结论并入 [AI.md](docs/AI.md)） |
| [30-kill-semantics.md](docs/30-kill-semantics.md) | **穿过即死，但不挡弹道** |
| [31-event-generation.md](docs/31-event-generation.md) | 由结算生成协议事件 |
| [32-netcode-implementation.md](docs/32-netcode-implementation.md) | 网络编解码实现（逐字节一致） |
| [33-server-handshake.md](docs/33-server-handshake.md) | 服务端握手（真实 TCP 验证） |
| [34-color-assignment.md](docs/34-color-assignment.md) | 玩家颜色分配（规则未定） |
| [35-tick-clock.md](docs/35-tick-clock.md) | tick 时钟：60 Hz（113 样本） |
| [36-match-server.md](docs/36-match-server.md) | 一局对战的服务端对象 Match |
| [37-paired-replay.md](docs/37-paired-replay.md) | 按回合配对的精确回放（致死集合一致） |
| [38-dynamic-mask.md](docs/38-dynamic-mask.md) | 动态掩膜假设：**被数据否定** |
| [39-punch-hole.md](docs/39-punch-hole.md) | **障碍摧毁=命中点挖洞**（frida 实证） |
| [40-level-generator.md](docs/40-level-generator.md) | 关卡生成：OS 熵 ⇒ 不可复现（**已被 55/61 推进，见该文顶部勘误**） |
| [41-turn-state-machine.md](docs/41-turn-state-machine.md) | 回合状态机（13 次迁移逐个相同） |
| [42-end-of-turn.md](docs/42-end-of-turn.md) | end_of_turn_tick = 回合截止时刻（六档实测） |
| [43-turn-message-sequence.md](docs/43-turn-message-sequence.md) | 一回合的报文顺序 |
| [44-destroy-delay.md](docs/44-destroy-delay.md) | DestroyObstacle 延迟 165 tick |
| [45-turn-count-and-ending.md](docs/45-turn-count-and-ending.md) | 回合计数与结束判定 |
| [46-full-turn.md](docs/46-full-turn.md) | 一整回合的 12 条报文序列 |
| [47-turn-order-random.md](docs/47-turn-order-random.md) | 回合出场顺序：推断为开局随机 ⇒ 不可复现 |
| [48-rng-replay-attempt.md](docs/48-rng-replay-attempt.md) | 随机源重放：试过，不行 |
| [49-verification-checklist.md](docs/49-verification-checklist.md) | **验收清单**（每条都实际跑过） |
| [86-simultaneous-turns.md](docs/86-simultaneous-turns.md) | **同时回合制的击杀结算顺序**：几何并行、事件按飞行时间 |
| [87-function-assembly.md](docs/87-function-assembly.md) | 函数装配规则（已并入 [AI.md](docs/AI.md)） |
| [88-coverage-matrix.md](docs/88-coverage-matrix.md) | **覆盖面矩阵**：我的"绿"到底盖住了多大一块 |
| [89-real-step.md](docs/89-real-step.md) | 实况步长 **0.01**，以及早停缺陷的定位全过程 |
| [90-anti-waste.md](docs/90-anti-waste.md) | 防无用功守则（开工必读） |
| [91-bisection-loop.md](docs/91-bisection-loop.md) | 二分循环的逐条汇编：`lo = x` **且** `hi = mid2`（改码依据） |
| [AI.md](docs/AI.md) | **AI 函数进化（唯一权威）** |

## 对外接口（构建产物里的符号）

```
gdext_rust_init                                               Godot GDExtension 入口
gw2_parse_len(text)                                        -> token 个数
gw2_shot_points(text,x0,y0,step,flip,ode,out,cap)          -> 点数（xy 交替写 out）
gw2_correction(text,x0,y0,flip)                            -> 修正常数
gw2_rasterize(x,y,size,out,len,fill)                       -> 0（写掩膜）
gw2_is_occupied(mask,len,x,y)                              -> 1 命中 / 0 空
gw2_simulate(text,x0,y0,step,flip,ode,obstacles,n,count)   -> 首个命中序号 / -1
```

前四个与"曲线"相关，中间两个是掩膜 API，最后一个是把两者拼起来的开火模拟。
每个都有对应的对拍脚本（见 `docs/16`）。注意：这 7 个是**我们为对拍自加的验证接口**，
不是游戏原扩展的接口面——原扩展只暴露 12 类 25 个方法，且几乎全是 UI 回调（`docs/19` §5）。

## 目录

```
docs/        逆向得到的实现说明
src/         Rust 复原实现（可编译）
tests/       与真二进制对齐用的用例
```

## 与上游工具链的关系

真值判定不在本仓库：`../analysis/emu.py` 用 unicorn 把 `.so` 当库调用，
`graphwar_curve.py` 是已证明逐位一致的 Python 参照实现。本仓库的代码若改动，
必须回到上游那套对拍里验证，而不是自证。
