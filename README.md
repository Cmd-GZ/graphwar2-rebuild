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
| 弹道采样 | ✅ 逐位 | 补上 `hi = mid2`（[91](docs/91-bisection-loop.md)）后，**24/24 用例整条点序列与真二进制逐位相等**（修复前 17/24）；(401, 24.8098, 9.8719) 与 (20001, 23.2128, 6.4176) 两条验收同时命中（[89 §32](docs/89-real-step.md)）。`tests/live_hits.rs` 12 条**全程用实况 0.01**（当年切 0.01 后的 4 个失败是**测试自己**跳过射手身体的半径固定 0.5、撞上掩膜像素量化抖动，[89 §33](docs/89-real-step.md)）。另有真实对局抓样 69/69 逐点一致（那次抓样的 `step=0.5`，非实况，[06 §15.1](docs/06-verification.md)） | ① `chord2 = NaN` 那条分支与汇编的等价性**未实测**（[91 §9.1](docs/91-bisection-loop.md)）；② `step≈50` 的越界参数域外仍多收 1 点（游戏不可达，[04 §7](docs/04-path-sampling.md)） |
| 障碍掩膜 / 碰撞 | ✅ 逐位 | 光栅化与 `is_occupied` 与真二进制**逐字节 / 300 随机点一致**；摧毁=命中点挖 `r=0.5` 的洞（frida 实证，[39](docs/39-punch-hole.md)） | ① 82 发里 **1 例**圆边像素个案未查清（[86 §9.2](docs/86-simultaneous-turns.md)）；② **通视检查**（`0x567530`）只到"观测"，未实现；③ 掩膜的持有者与重建时机未观测 |
| AI 进化 | 🔶 部分 | 权重表（合计 85）/ 长度 `trunc(N(0,1)*5+10)` / 六档参数 / 装配 / 交叉 / 变异 / 每代构成 = 反汇编 + 40 万次运行时抽样；适应度公式逐条反汇编（[AI.md](docs/AI.md)）。终止条件已解 = **代数到顶 ∨ 25 s 墙钟**（[93](docs/93-ai-time-budget.md)）；"精英 5" 也已解：每代留**前 20%** 全部克隆，字面量 5 = **每个槽位 1 克隆 + 4 工厂子代**（[95](docs/95-elite5.md)） | ① **搜索过程不可逐位复现**（RNG 取 OS 熵）⇒ 永远只能统计验收；② **`aigen::evolve` 还没有 25 s 墙钟上限**（只按代数循环 ⇒ 难度 6 会多算，且结果依机器速度，[93 §5.4](docs/93-ai-time-budget.md)）；③ **难度 6 的 replay 样本可能被 25 s 截断且无任何标记** ⇒ 不能拿"1000 代跑满"当前提；④ ~~`AxisMode`/`TimeMode` 是否参与判定~~ **已结案：不参与**（[88 §17](docs/88-coverage-matrix.md)）；⑤ DiffEq × AI 只验了变量槽权重，完整进化未验 |
| 关卡生成 | 🔶 部分 | 随机散布模式 Rust 实现**逐位复现 13/13**（障碍 `x/y/r` 的 `to_bits()` 全等，[61](docs/61-levelgen-rust.md)）；公式与 ziggurat 表见 [55](docs/55-level-generator-closed.md)/[56](docs/56-levelgen-replay-complete.md) | ① ~~圆盘式未实现~~ **已结案**：它不是关卡生成器（`docs/103`），关卡只有散布 + 中心单障碍两种；② 中心单障碍已有 **4 个真值样本**（半径 3.34/4.81/6.75/5.69，全在 `u*5+3` 区间 ✓）；③ Campaign 21 关的**关卡数据与逐关解锁流程**仍未做 |
| 网络协议 | ✅ 逐位 | 46 种消息全部解析；`to_json` 与 4 类消息**逐字节一致**；握手 7 条、一回合 12 条报文的种类与顺序与抓包一致（[24](docs/24-network-protocol.md)、[32](docs/32-netcode-implementation.md)、[46](docs/46-full-turn.md)） | ① ~~玩家颜色分配规则未定~~ **已结案**（[102](docs/102-color-assignment.md)：两张 6 色队色表 + 随机起点线性探测）；② 玩家加入流程是**单连接简化版**（[62 §4](docs/62-server-loop.md)）；③ **自建客户端连真服务器从未成功**（端口找到但握手无应答，[62 §5](docs/62-server-loop.md)，需重试）；④ Global/大厅/匹配这一层没碰 |
| 回合 / 生命 / 结算 | ✅ 逐位 | 穿过即死、不挡弹道、无血量、友军误伤；回合状态机 6 状态 7 事件 **13 次迁移逐个相同**；同时回合制 = 几何并行 + 事件按飞行时间（修复后 82 发并集 **152/152**、时刻判决 **38:0**，[86 §14](docs/86-simultaneous-turns.md)） | ① **回合出场顺序不可确定性复现**（已证伪来自 ChaCha 流，判为 HashMap 迭代序，[47](docs/47-turn-order-random.md)）⇒ 重建版必须自定规则并声明；② 目标选择 `target_manager` 仍是 SIMD 未解（[02 §13.4](docs/02-module-map.md)）；③ ~~`AxisMode`/`TimeMode` 是否参与判定~~ **已结案：不参与**（[88 §17](docs/88-coverage-matrix.md)） |
| 服务端循环 | ✅ 逐位 | `serverloop::run` 在**真实 TCP** 上跑完整一局到 `EndingGame`；延迟删障碍 **165 tick**；回合计数与结束判定（[62](docs/62-server-loop.md)、[44](docs/44-destroy-delay.md)、[45](docs/45-turn-count-and-ending.md)） | ① 多客户端同房未实现（现在只支持单连接）；② 断线重连 / 超时踢人 / 房主迁移未做；③ 真实 LAN/Global 大厅与建房流程未做 |
| **Godot 客户端界面** | ⬜ 未做 | 已用**运行时枚举**建台账：`docs/92` 的 **125 条**（界面 × 控件 × 类型 × 作用），**15/15 界面全部进入过**、22/25 个游戏场景观测到实例化位置（`docs/92`）。仓库里**没有任何 Godot 工程** | **全部界面从零建**：主菜单 4 按钮、Campaign 21 关（按钮由脚本运行时创建）、Battle（建房/加入 IP/四模式/加 AI 六档）、Player（三页签 Skins15/Faces15/Hats27）、Replays 播放器、聊天与确认框；另需重挂 SVG 图标与主题 |
| **脚本逻辑（`.gdc`）** | 🔶 已定性 | `.gdc` 是 `GDSC`+**zstd** 容器（字符串池 `c^0xb6` 混淆），`tools/gdc_strings.py` **105/105 全解**。**关键结论**：51 个非插件脚本里 40 个是字号/间距布局助手、6 个空壳、**只有 5 个有行为**；启动快照 2724 条信号连接中**游戏 UI 只剩 6 条** ⇒ **界面逻辑在 Rust 里，不在脚本里**（[88 §12.0](docs/88-coverage-matrix.md)） | 布局助手的字符串已解出，可直接照抄；真正要做的是**把 Rust 侧的界面逻辑接出来**（我们已经在逆那份二进制），而不是等反编译器 |
| **资源 / 音频 / 本地化** | 🔶 部分 | 资源还原 **692 文件 md5 全符**（`rsc/MANIFEST.tsv`，[17](docs/17-resources.md)）：167 `.ctex`、22 `.res`、184 `.import`、149 个 SVG 图标 | ① 音频只有 **1 个** `sounds/humm.wav`（无音乐、无音效库）⇒ "是否还有音频系统"**未确认**；② 包里**没有** `.translation`/`.csv` ⇒ 无本地化资源，文本硬编码在场景/脚本里；③ 资源齐 ≠ 能组装，依赖"界面""脚本"两行 |
| **构建产物** | 🔶 部分 | `cargo build --release` 出 cdylib，`gdext_rust_init` 在真 Godot 4.7.2 注册成功；导出 `.pck` 能从包里启动并加载还原的纹理（[18](docs/18-build.md)，原 `19` 已并入其 §6） | 只产出**内核扩展**，不是游戏。没有可玩客户端（缺"界面""脚本"两行）；原扩展对外只有 **12 类 25 个方法**、几乎全是 UI 回调，玩法逻辑完全不开放（[18 §6.5](docs/18-build.md)） |

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
3. **两处"永不可逐位"的规则必须自己定**（回合出场顺序、AI 搜索过程（RNG 取 OS 熵）——颜色分配已于 `102` 解出），
   并在文档里标成"重建版自有规则"，不能冒充复原结论。

**Rust 实现状态**：`parse` / `eval` / `calculate` / `shot` / `plane` / `events` / `life` /
`turnstate` / `protocol` / `game` / `server` / `serverloop` / `levelgen` / `aigen` 均已可运行；
`cargo test` 共 **150 条**（`#[test]` 实测；`89` §32 当时记的是 135 条全绿，此后新增 AI 两阶段评分等用例。本轮未重跑）。
模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)，共约 90 个 `graphwar_*.rs`。

## 文档

全部文档的索引（现行 / 已归档 / 已并档）见 **[`docs/INDEX.md`](docs/INDEX.md)**。
**一份文档一个主题**：同一主题只有一篇现行文档，已归并的进 `docs/archive/`。
本表只列**现行**的与少数必须知道的指针，不是全量清单。

| 文档 | 内容 |
|---|---|
| [HANDOVER.md](docs/HANDOVER.md) | **给上下文压缩后的人/自己：一篇替代全部** |
| [00-status.md](docs/00-status.md) | 状态总览：什么已完成、什么只观测、什么还空白 |
| **[15-end-to-end.md](docs/15-end-to-end.md)** | **端到端：从敲下函数到有人死亡（建议先读这篇）** |
| [01-architecture.md](docs/01-architecture.md) | 整体分层：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](docs/02-module-map.md) | ~90 个模块的清单、职责、函数↔模块映射；**§13 = 首批逐模块观测（原 10/12/13/14）** |
| [03-function-language.md](docs/03-function-language.md) | 玩家输入的函数语言：文法、优先级、全部怪癖 |
| [04-path-sampling.md](docs/04-path-sampling.md) | 等弦长二分采样：算法本体与全部常量 |
| [05-shot-pipeline.md](docs/05-shot-pipeline.md) | 一次开火的完整链路：位置 → 修正 → 采点 → 广播 |
| [06-verification.md](docs/06-verification.md) | **如何证明复原正确**：证据分级、静态 oracle；**§12–§16 = 函数指针派发、frida 活体 oracle、原版 harness、真实对局验证（原 11/20/21/22/23/26）** |
| [07-protocol.md](docs/07-protocol.md) | 事件枚举、字段、变体顺序与传输线索 |
| [08-plane.md](docs/08-plane.md) | 坐标变换、碰撞掩膜、y 轴方向与光栅化舍入 |
| [16-reproduce.md](docs/16-reproduce.md) | **如何验证这份复原**：跑测试、真值来源、复现环境、环境坑 |
| [17-resources.md](docs/17-resources.md) | 游戏资源还原：PCK v4 格式、rsc/ 生成与为何不入库 |
| [18-build.md](docs/18-build.md) | **构建与真引擎验证**：cdylib、`gdext_rust_init`、类/方法注册、`.pck` 启动（原 19 并入 §6） |
| [24-network-protocol.md](docs/24-network-protocol.md) | **网络协议**：帧格式与 46 种消息（真实抓包） |
| [25-live-collision.md](docs/25-live-collision.md) | 碰撞链路端到端验证（整关 4 发 / 6 发回放；含 `step=0.5` 勘误） |
| [30-kill-semantics.md](docs/30-kill-semantics.md) | **穿过即死，但不挡弹道** |
| [39-punch-hole.md](docs/39-punch-hole.md) | **障碍摧毁 = 命中点挖洞**（frida 实证） |
| [41-turn-state-machine.md](docs/41-turn-state-machine.md) | 回合状态机（13 次迁移逐个相同；时间基准未解） |
| [43-turn-message-sequence.md](docs/43-turn-message-sequence.md) | 一回合的报文顺序 |
| **[47-turn-order-random.md](docs/47-turn-order-random.md)** | **回合出场顺序：现行唯一文档**（吸收 59/60：已证伪来自 ChaCha，判为 HashMap 迭代序） |
| [61-levelgen-rust.md](docs/61-levelgen-rust.md) | 关卡生成的 Rust 实现（逐位复现 13/13） |
| [62-server-loop.md](docs/62-server-loop.md) | 能跑完整一局的服务端；**§5 = 自建客户端连真服务器（原 64，未打通）** |
| **[65-life-rules-confirmed.md](docs/65-life-rules-confirmed.md)** | **生命/致死规则：现行唯一文档**（吸收 27/63：无血量 / 友军致死 / 按弹道先后） |
| [78-replay-files.md](docs/78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 |
| [86-simultaneous-turns.md](docs/86-simultaneous-turns.md) | **同时回合制的击杀结算顺序**：几何并行、事件按飞行时间 |
| [87-function-assembly.md](docs/87-function-assembly.md) | 函数装配规则（已并入 [AI.md](docs/AI.md)） |
| [88-coverage-matrix.md](docs/88-coverage-matrix.md) | **覆盖面矩阵**：我的"绿"到底盖住了多大一块 |
| [89-real-step.md](docs/89-real-step.md) | 实况步长 **0.01**，以及早停缺陷的定位全过程 |
| [90-anti-waste.md](docs/90-anti-waste.md) | 防无用功守则（开工必读；含地址空间换算） |
| [91-bisection-loop.md](docs/91-bisection-loop.md) | 二分循环的逐条汇编：`lo = x` **且** `hi = mid2`（改码依据） |
| [92-ui-inventory.md](docs/92-ui-inventory.md) | 客户端界面台账（15/15 界面，运行时枚举） |
| [101-gdc-contents.md](docs/101-gdc-contents.md) | `.gdc` 常量/标识符/函数/伪源码（105/105） |
| [103-disk-levelgen.md](docs/103-disk-levelgen.md) | `0x4ea6dc` **不是**关卡生成器；关卡只有两种 |
| **[107-docs-consolidation.md](docs/107-docs-consolidation.md)** | **文档技术债清理**：家底清单、归并映射、改掉的陈旧断言 |
| [AI.md](docs/AI.md) | **AI 函数进化（唯一权威）** |

已归档/已并档的 28 篇（含 `archive/ai/` 12 份 AI 逐轮记录）不再在此列出，
见 [`docs/INDEX.md`](docs/INDEX.md) §2/§3。

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
不是游戏原扩展的接口面——原扩展只暴露 12 类 25 个方法，且几乎全是 UI 回调（`18` §6.5）。

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
