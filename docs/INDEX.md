# docs 索引

> **规则**：`docs/` 只放**现行事实**（持续更新）；历史与已推翻的进 `docs/archive/`，只作过程档案。
> **一个主题只有一份现行文档。** 新结论改现行文档，不新开编号。
>
> 2026-11 做过一轮**技术债清理**：`09`–`27`、`60`–`65` 那批按时间堆起来的过程记录已按主题归并，
> 逐份判定、归并映射、改掉的陈旧断言全部记在
> **[`107-docs-consolidation.md`](107-docs-consolidation.md)**。
> 三节（现行 / 已归档 / 已并档）与磁盘上的文件**逐一对得上**，可以当清单核对。

## 一、现行文档（85 份）

> 数法：`find docs -name "*.md" -not -path "*/archive/*" | wc -l` = **85**（2026-11 复核；
> 含本轮新增的 [`109`](109-kernel-turn-aigen.md)）。之前这里写的 76 是旧数，已改。

### 1.1 总览与索引

| 文件 | 主题 |
|---|---|
| **[107-docs-consolidation.md](107-docs-consolidation.md)** | **文档技术债清理报告**：家底清单、归并映射、陈旧断言逐条更正 |
| [HANDOVER.md](HANDOVER.md) | 给上下文压缩后的自己：一页替代全部（**建议先读**） |
| [00-status.md](00-status.md) | 当前状态总览（唯一权威） |
| [AI.md](AI.md) | **AI 函数进化（唯一权威）**，取代 66–77 十二份逐轮记录 |

### 1.2 内核主线（读这一串就懂全链）

| 文件 | 主题 |
|---|---|
| [01-architecture.md](01-architecture.md) | 整体架构：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](02-module-map.md) | 模块地图、函数↔模块映射；**§13 = 首批逐模块观测**（吸收 10/12/13/14） |
| [03-function-language.md](03-function-language.md) | 玩家输入的函数语言：文法、优先级、怪癖 |
| [04-path-sampling.md](04-path-sampling.md) | 等弦长二分采样（核心算法 + 全部常量） |
| [05-shot-pipeline.md](05-shot-pipeline.md) | 一次开火的完整链路 |
| [08-plane.md](08-plane.md) | 坐标系、碰撞掩膜、光栅化与 y 轴方向 |
| [15-end-to-end.md](15-end-to-end.md) | 端到端：从敲下函数到有人死亡 |
| **[06-verification.md](06-verification.md)** | **如何证明复原是对的**：证据分级 + 静态 oracle；**§12–§16 = 函数指针派发 / frida 活体 oracle / 原版 harness / 真实对局验证**（吸收 11/20/21/22/23/26） |
| [16-reproduce.md](16-reproduce.md) | 如何验证这份复原（复现步骤与环境坑） |

### 1.3 采样器缺陷专线（按时间读）

| 文件 | 主题 |
|---|---|
| [89-real-step.md](89-real-step.md) | 实况步长 **0.01**，以及早停缺陷的定位全过程 |
| [91-bisection-loop.md](91-bisection-loop.md) | 二分循环逐条汇编：`lo = x` **且** `hi = mid2`，判据 `not (L > L2)`（改码依据） |
| [90-anti-waste.md](90-anti-waste.md) | 防无用功守则；§2.6/§2.7 = REA 静默陷阱与地址换算（**开工必读**） |

### 1.4 联机 / 协议 / 服务端 / 回合

| 文件 | 主题 |
|---|---|
| [07-protocol.md](07-protocol.md) | 事件枚举、字段、变体顺序与传输线索 |
| [24-network-protocol.md](24-network-protocol.md) | 网络协议：帧格式与 46 种消息（真实抓包） |
| [31-event-generation.md](31-event-generation.md) | 由结算生成协议事件 |
| [32-netcode-implementation.md](32-netcode-implementation.md) | 网络编解码实现（逐字节一致） |
| [33-server-handshake.md](33-server-handshake.md) | 服务端握手（真实 TCP 验证） |
| [36-match-server.md](36-match-server.md) | 一局对战的服务端对象 `Match` |
| [43-turn-message-sequence.md](43-turn-message-sequence.md) | 一回合的报文顺序 |
| [46-full-turn.md](46-full-turn.md) | 一整回合的 12 条报文序列 |
| [62-server-loop.md](62-server-loop.md) | 能跑完整一局的服务端；**§5 = 自建客户端连真服务器（吸收 64，未打通）** |
| [96-protocol-coverage.md](96-protocol-coverage.md) | 协议覆盖面（还缺哪些消息） |
| [41-turn-state-machine.md](41-turn-state-machine.md) | 回合状态机（13 次迁移逐个相同；时间基准未解） |
| [42-end-of-turn.md](42-end-of-turn.md) | `end_of_turn_tick` = 回合截止时刻（六档实测） |
| [44-destroy-delay.md](44-destroy-delay.md) | `DestroyObstacle` 延迟 165 tick |
| [45-turn-count-and-ending.md](45-turn-count-and-ending.md) | 回合计数与结束判定 |
| [35-tick-clock.md](35-tick-clock.md) | tick 时钟：60 Hz（113 样本） |
| **[47-turn-order-random.md](47-turn-order-random.md)** | **回合出场顺序（唯一现行）**：已证伪来自 ChaCha 流，判为 HashMap 迭代序 ⇒ 不可复现（吸收 59/60） |
| [34-color-assignment.md](34-color-assignment.md) | 玩家颜色分配：观测记录（**结论见 [102](102-color-assignment.md)**） |
| [102-color-assignment.md](102-color-assignment.md) | **玩家颜色分配结案**：两张 6 色队色表 + 随机起点线性探测 |
| [105-team-sides.md](105-team-sides.md) | **队伍固定在 x 两侧**，`flip` 由队伍决定（不用几何猜） |
| [109-kernel-turn-aigen.md](109-kernel-turn-aigen.md) | **内核两处缺口的实现**：`turn.rs` 射手轮转（`Vec<soldier_id>`+游标/判活/全灭发 0）、`aigen` 按士兵的进化层（不共享、每回合重算）、`flip` 由队伍、超时跳过（用户口述） |
| [53-rng-found.md](53-rng-found.md) | **随机源找到了**（结论篇） |
| [54-rng-replay-verified.md](54-rng-replay-verified.md) | RNG 重放验证（结论篇） |
| [37-paired-replay.md](37-paired-replay.md) | 按回合配对的精确回放（致死集合一致） |
| [49-verification-checklist.md](49-verification-checklist.md) | 验收清单（每条都实际跑过） |

### 1.5 关卡 / 碰撞 / 生命 / 同时回合

| 文件 | 主题 |
|---|---|
| [40-level-generator.md](40-level-generator.md) | 关卡生成：OS 熵 ⇒ 不可复现（**顶部有勘误**，被 55/61 推进） |
| [55-level-generator-closed.md](55-level-generator-closed.md) | 关卡生成公式闭合 |
| [56-levelgen-replay-complete.md](56-levelgen-replay-complete.md) | 关卡生成 replay 全解 |
| [61-levelgen-rust.md](61-levelgen-rust.md) | 关卡生成的 Rust 实现（逐位复现 13/13） |
| [103-disk-levelgen.md](103-disk-levelgen.md) | `0x4ea6dc` **不是**第三种关卡模式；关卡只有两种 |
| [38-dynamic-mask.md](38-dynamic-mask.md) | 动态掩膜假设：**被数据否定**（结论见 `39`） |
| [39-punch-hole.md](39-punch-hole.md) | 障碍摧毁 = 命中点挖洞（frida 实证） |
| [30-kill-semantics.md](30-kill-semantics.md) | 致死判定：穿过即死，但不挡弹道 |
| **[65-life-rules-confirmed.md](65-life-rules-confirmed.md)** | **生命/致死规则（唯一现行）**：无血量 / 友军致死 / 按弹道先后；含死亡链路与负结果（吸收 27/63） |
| [25-live-collision.md](25-live-collision.md) | 碰撞链路端到端验证（含 `step=0.5` 勘误） |
| [86-simultaneous-turns.md](86-simultaneous-turns.md) | 同时回合制的击杀结算顺序 |

### 1.6 AI（除 `AI.md` 外的四篇深挖）

| 文件 | 主题 |
|---|---|
| [78-replay-files.md](78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 |
| [79-replay-ai-corpus.md](79-replay-ai-corpus.md) | 从 replay 按等级分离 AI 函数（真实数据验证） |
| [80-limit-vs-length.md](80-limit-vs-length.md) | 为什么低难度的 AI 会发射"超上限"的函数 |
| [85-statistical-check.md](85-statistical-check.md) | 生成器 vs 真实机器人函数的统计对照 |
| [87-function-assembly.md](87-function-assembly.md) | 函数装配规则（已并入 `AI.md` §6，本文留证据） |
| [93-ai-time-budget.md](93-ai-time-budget.md) | **25 s 墙钟预算**（怎么定的） |
| [94-var-mode-trim.md](94-var-mode-trim.md) | `FunctionMode` 是生成时区分，不是生成后裁剪 `y` |
| [95-elite5.md](95-elite5.md) | "精英 5"在 GW2 里**不存在**；精英 = 每代前 20% |
| [100-runtime-ai-checks.md](100-runtime-ai-checks.md) | **AI 三问的运行时验证**（25 s 预算 / 两阶段评分 / 每代评分次数，frida 跑真游戏；`tools/hook_ai_phases.js`） |
| [97-two-phase-scoring.md](97-two-phase-scoring.md) | 两阶段评分：粗评 0.5 → 前 3 名精评 0.01 → 不一致换新 |
| [98-shooter-selection.md](98-shooter-selection.md) | AI 不选射手：`turn_manager` 按 `Vec<soldier_id>`+游标轮转 |
| [99-generation-dynamics.md](99-generation-dynamics.md) | 每代组装：单父代工厂、无缓存全量重评、B 变体 |

### 1.7 覆盖面 / 验收 / 构建 / 界面

| 文件 | 主题 |
|---|---|
| [88-coverage-matrix.md](88-coverage-matrix.md) | 覆盖面矩阵（唯一台账）；§17 = `AxisMode`/`TimeMode` 不参与任何判定 |
| [83-physics-verified.md](83-physics-verified.md) | 物理验证通过（"停止判定之谜"是提取器 bug；数字是 `step=0.5` 测的） |
| [84-kills-verified.md](84-kills-verified.md) | 击杀数预测与真游戏一致（同上，`step=0.5`） |
| [17-resources.md](17-resources.md) | 游戏资源还原（`rsc/`、PCK v4 格式、为何不入库） |
| [18-build.md](18-build.md) | **构建与真引擎验证**：cdylib、`gdext_rust_init`、类/方法注册、`.pck` 启动（吸收 19） |
| [92-ui-inventory.md](92-ui-inventory.md) | **客户端界面台账**（15/15 界面，运行时枚举） |
| [104-recovered-project.md](104-recovered-project.md) | **GDRE 整项目恢复**：静态场景属性，并与 `92` 的运行时台账逐项对拍 |
| [101-gdc-contents.md](101-gdc-contents.md) | `.gdc` 逐字节格式 + 5 个行为脚本 + 47 行布局数值表 |
| [106-guide-crosscheck.md](106-guide-crosscheck.md) | `graphwar-guide`（GW1）的论断在 GW2 是否仍成立：逐条对账 |

## 二、已归档（`docs/archive/`，**不要引用其结论**）

> 2026-11 新归档的 16 份都带一行顶部横幅，写明"并入哪里"。`archive/ai/` 那 12 份是上一轮合并的。

| 归档项 | 被谁取代 / 并入哪里 |
|---|---|
| `archive/ai/` 66–77 共 12 份：[66](archive/ai/66-ai-is-gw1-evolution.md) · [67](archive/ai/67-ai-parameters.md) · [68](archive/ai/68-ai-token-set.md) · [69](archive/ai/69-token-codes.md) · [70](archive/ai/70-multiple-level-generators.md) · [71](archive/ai/71-token-weights.md) · [72](archive/ai/72-level-generator-modes.md) · [73](archive/ai/73-mutation-operator.md) · [74](archive/ai/74-fitness-formula-corrected.md) · [75](archive/ai/75-generation-composition.md) · [76](archive/ai/76-function-length.md) · [77](archive/ai/77-ai-player.md) | 合并为 [AI.md](AI.md) |
| [archive/09-report-decision.md](archive/09-report-decision.md) | 上游报告拆分决策（过程记录）；结论"暂不拆分"仍成立，依据摘要见 `105` §6 |
| [archive/10-lifecycle.md](archive/10-lifecycle.md) | [02](02-module-map.md) §13.1；规则部分 → [65](65-life-rules-confirmed.md) |
| [archive/11-dispatch.md](archive/11-dispatch.md) | [06](06-verification.md) §12 |
| [archive/13-turns.md](archive/13-turns.md) | [02](02-module-map.md) §13.3（原路径留占位指针） |
| [archive/14-target.md](archive/14-target.md) | [02](02-module-map.md) §13.4（原路径留占位指针） |
| [archive/19-godot-verify.md](archive/19-godot-verify.md) | [18](18-build.md) §6 |
| [archive/20-live-oracle.md](archive/20-live-oracle.md) | [06](06-verification.md) §13 |
| [archive/21-oracle-limits.md](archive/21-oracle-limits.md) | [06](06-verification.md) §14.1（其"不可达"结论已被 `22`→`06` §14.2 推翻） |
| [archive/22-live-game-harness.md](archive/22-live-game-harness.md) | [06](06-verification.md) §14（原路径留占位指针） |
| [archive/23-live-verification.md](archive/23-live-verification.md) | [06](06-verification.md) §15 |
| [archive/26-harness-control.md](archive/26-harness-control.md) | [06](06-verification.md) §15.3；AI 函数样本 → [AI.md](AI.md) §2.1（原路径留占位指针） |
| [archive/27-death-flow.md](archive/27-death-flow.md) | [65](65-life-rules-confirmed.md) |
| [archive/59-turnorder-negative-and-two-rngs.md](archive/59-turnorder-negative-and-two-rngs.md) | [47](47-turn-order-random.md) |
| [archive/60-turnorder-verdict.md](archive/60-turnorder-verdict.md) | [47](47-turn-order-random.md)（原路径留占位指针） |
| [archive/63-life-rules-status.md](archive/63-life-rules-status.md) | [65](65-life-rules-confirmed.md) |
| [archive/64-join-real-server-attempt.md](archive/64-join-real-server-attempt.md) | [62](62-server-loop.md) §5 |
| [archive/28-turn-order.md](archive/28-turn-order.md) | 被 `47` 修正（上一轮归档） |
| [archive/29-ai-probe.md](archive/29-ai-probe.md) | AI 规则由 66+ 重做、现并入 [AI.md](AI.md)（上一轮归档） |
| [archive/81-replay-validation.md](archive/81-replay-validation.md) | 掩膜"半像素"推断被 `83` 推翻（上一轮归档） |
| [archive/82-stop-test.md](archive/82-stop-test.md) | "停止判定不同"被 `83` 推翻（是提取器 bug，上一轮归档） |

## 三、已并档：原路径保留的占位指针（5 份）

这 5 个文件**不是现行文档**，只在原路径留了一张小卡片，说明"内容搬到哪一节"。
保留的理由是**不让旧引用变成死链**——`92`/`98` 里仍按 `docs/13`、`docs/14`、`docs/22`、
`docs/26`、`docs/60` 引用它们，而那几篇本轮不允许改。

| 占位文件 | 内容实际在 | 被谁引用 |
|---|---|---|
| [13-turns.md](13-turns.md) | [02](02-module-map.md) §13.3 | `98` §2/§6 |
| [14-target.md](14-target.md) | [02](02-module-map.md) §13.4 | `98` §5 |
| [22-live-game-harness.md](22-live-game-harness.md) | [06](06-verification.md) §14 | `92` §0/§8 |
| [26-harness-control.md](26-harness-control.md) | [06](06-verification.md) §15.3 | `92` §3/§4 |
| [60-turnorder-verdict.md](60-turnorder-verdict.md) | [47](47-turn-order-random.md) | `98` §6 |

## 四、已推翻但**仍留在 `docs/`** 的文件（**别引用其结论**）

规则说"已推翻的进 `archive/`"，但下面这几份同时含有仍然有效的观测或证据，暂时移不动，
所以在这里显式登记（2026-11 核对）：

| 文件 | 已被谁推翻 / 取代 | 说明 |
|---|---|---|
| [12-ai.md](12-ai.md) | [AI.md](AI.md)（现行唯一权威） | 标题即"评分规则未还原"，与现状不符；只剩模块级观测（已抄进 `02` §13.2）。**不归档的原因**：`97`/`98`/`99` 与 `AI.md` §8.6 把它当"vaddr 写成 Ghidra 地址"的历史案例引用，而那几篇本轮禁改 |
| [38-dynamic-mask.md](38-dynamic-mask.md) | 被数据否定 → [39-punch-hole.md](39-punch-hole.md) | "动态掩膜"假设不成立；摧毁 = 命中点挖洞 |
| [40-level-generator.md](40-level-generator.md) | [50](50-rng-source-reopened.md) / [55](55-level-generator-closed.md) / [61](61-levelgen-rust.md) / [103](103-disk-levelgen.md) | "OS 熵 ⇒ 关卡不可复现"是错的；生成已逐位复现 13/13。**顶部已有勘误横幅** |
| [89-real-step.md](89-real-step.md) §5–§29 | [91](91-bisection-loop.md) 与 `89` 自己的 §31/§32 | **步长结论（0.01）仍现行**；但"早停"的整条推理（`lo = x` 是刻意的 / 差一次二分迭代 / 接受上一轮 mid）**已被 §31/§32 全部更正**，只当过程档案读 |
| [83](83-physics-verified.md) / [84](84-kills-verified.md) 里与步长有关的数字 | [89](89-real-step.md) §2、[86](86-simultaneous-turns.md) §14 | 结论未被推翻，但那批数字（21 发差 <0.5、击杀数 12 发一致）是 `step=0.5` 测的；实况 `0.01` 的强证据在 `86` §14（152/152、38:0）。**两份文件顶部已注明** |
| [48-rng-replay-attempt.md](48-rng-replay-attempt.md) · [50](50-rng-source-reopened.md) / [51](51-obstacle-emit-chain.md) / [52](52-rng-hunt-status.md) / [55](55-level-generator-closed.md) / [56](56-levelgen-replay-complete.md) / [57](57-seed-source.md) / [58](58-rng-capability-summary.md) | 结论已收进 [53](53-rng-found.md)（找到 RNG）、[54](54-rng-replay-verified.md)（重放验证）、[55](55-level-generator-closed.md)/[56](56-levelgen-replay-complete.md)（关卡公式闭合）、[47](47-turn-order-random.md)（出场顺序） | 这一串是**按时间的过程记录**，含大量"当时还没找到"的中间态（`48` 标题就是"试过，不行"）。**要结论就看 `53`/`54`/`55`/`47`**，不要逐篇引用 |

## 五、地址空间约定（**全仓统一，引用前必看**）

- **Ghidra / REA 地址 = ELF 虚地址 + `0x100000`，且只能加一次**（`90` §2.7）。
- 给 REA 传一个**不存在的过程地址**时它不报错，而是按文件偏移解析出**无关函数**；
  必须先用 `procedure_info` / `resolve_containing_procedure` 确认是函数入口，
  并核对返回的 `selection.mapped_address`（`90` §2.6）。
- 早前文档（`12`、`archive/ai/74`、`87`）把 vaddr 当 Ghidra 地址写过，差 `0x100000`
  —— 引用那些地址时先确认空间（`AI.md` §8.6 有对照表）。
