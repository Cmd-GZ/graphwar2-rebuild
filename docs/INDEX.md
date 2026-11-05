# docs 索引

> **规则**：`docs/` 只放两类东西——**现行事实**（会被持续更新）与**历史记录**（结论定格，仅供追溯）。
> 每份文档开头必须有一行 `状态：现行 / 历史 / 已推翻`。新结论先改现行文档，不要新开一份。

## 一、现行文档（读这些就够了）

| # | 文件 | 主题 | 说明 |
|---|---|---|---|
| 00 | [00-status.md](00-status.md) | 当前状态总览 | 当前状态总览（唯一权威） |
| 01 | [01-architecture.md](01-architecture.md) | 整体架构 | 整体架构 |
| 03 | [03-function-language.md](03-function-language.md) | 函数语言 | 函数语言 |
| 04 | [04-path-sampling.md](04-path-sampling.md) | 点序列采样（核心算法） | 点路径采样 |
| 05 | [05-shot-pipeline.md](05-shot-pipeline.md) | 一次开火的完整链路 | 一次开火的完整链路 |
| 06 | [06-verification.md](06-verification.md) | 如何证明复原是对的 | 如何证明复原是对的 |
| 07 | [07-protocol.md](07-protocol.md) | 事件协议（联机与玩法状态机） | 事件协议 |
| 08 | [08-plane.md](08-plane.md) | 坐标系（graphwar_plane.rs） | 坐标系 |
| 15 | [15-end-to-end.md](15-end-to-end.md) | 端到端：从敲下函数到有人死亡 | 端到端 |
| 16 | [16-reproduce.md](16-reproduce.md) | 如何验证这份复原 | 复现步骤 |
| 17 | [17-resources.md](17-resources.md) | 游戏资源还原（rsc/） | 资源还原 |
| 24 | [24-network-protocol.md](24-network-protocol.md) | 网络协议（抓包还原） | 网络协议 |
| 78 | [78-replay-files.md](78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 | replay 文件位置与格式 |
| 79 | [79-replay-ai-corpus.md](79-replay-ai-corpus.md) | 从 replay 里按等级分离 AI 函数（真实数据验证 | 按等级分离 AI 函数 |
| 80 | [80-limit-vs-length.md](80-limit-vs-length.md) | 为什么低难度的 AI 会发射"超上限"的函数 | 低难度超限发射 |
| 83 | [83-physics-verified.md](83-physics-verified.md) | 物理验证通过：之前的"停止判定之谜"是我提取器的 bug | 物理验证通过（含提取器 bug 复盘） |
| 84 | [84-kills-verified.md](84-kills-verified.md) | 击杀数预测与真游戏完全一致 | 击杀数逐项一致 |
| 85 | [85-statistical-check.md](85-statistical-check.md) | 生成器 vs 真实机器人函数的统计对照（结论：一元可验， | 统计验收 |
| 86 | [86-simultaneous-turns.md](86-simultaneous-turns.md) | 同时回合制（SimultaneousTurns）的击杀结 | 同时回合制结算（数字待按 0.01 重算） |
| 87 | [87-function-assembly.md](87-function-assembly.md) | 函数装配规则（`docs/76` §4.5 的悬案，已解 | 函数装配规则 |
| 88 | [88-coverage-matrix.md](88-coverage-matrix.md) | 覆盖面矩阵：我的"绿"到底盖住了多大一块 | 覆盖面矩阵（唯一台账） |
| 89 | [89-real-step.md](89-real-step.md) | 实况步长是 **0.01**，不是 0.5（此前所有对比 | 实况步长 0.01（含追责） |
| 90 | [90-anti-waste.md](90-anti-waste.md) | 防无用功守则（本轮会话的失败清单与对策） | 防无用功守则（开工必读） |

## 二、已推翻 / 仅留过程（**不要引用其结论**）

| # | 文件 | 主题 | 状态 |
|---|---|---|---|
| 28 | [28-turn-order.md](28-turn-order.md) | 回合出场顺序（四局抓包验证） | 回合顺序结论被 docs/47 修正 |
| 29 | [29-ai-probe.md](29-ai-probe.md) | AI 内部探针（定量刻画，规则未还原） | AI 探针只做了定量刻画，规则由 docs/66+ 重做 |
| 81 | [81-replay-validation.md](81-replay-validation.md) | 用 replay 局面验证我们的弹道与适应度 | §3 的"半像素"推断已被 docs/83 推翻，仅留过程记录 |
| 82 | [82-stop-test.md](82-stop-test.md) | 弹道"在哪里停下"——本轮定位到的差异点 | "停止判定不同"的结论已由 docs/83 推翻（是我提取器 bug） |

## 三、历史记录（按时间累积，未整理）

共 65 份。它们是过程档案，**不作为当前事实来源**；引用前先确认对应主题的现行文档。

02-module-map | 09-report-decision | 10-lifecycle | 11-dispatch | 12-ai | 13-turns | 14-target | 18-build | 19-godot-verify | 20-live-oracle | 21-oracle-limits | 22-live-game-harness | 23-live-verification | 25-live-collision | 26-harness-control | 27-death-flow | 30-kill-semantics | 31-event-generation | 32-netcode-implementation | 33-server-handshake | 34-color-assignment | 35-tick-clock | 36-match-server | 37-paired-replay | 38-dynamic-mask | 39-punch-hole | 40-level-generator | 41-turn-state-machine | 42-end-of-turn | 43-turn-message-sequence | 44-destroy-delay | 45-turn-count-and-ending | 46-full-turn | 47-turn-order-random | 48-rng-replay-attempt | 49-verification-checklist | 50-rng-source-reopened | 51-obstacle-emit-chain | 52-rng-hunt-status | 53-rng-found | 54-rng-replay-verified | 55-level-generator-closed | 56-levelgen-replay-complete | 57-seed-source | 58-rng-capability-summary | 59-turnorder-negative-and-two-rngs | 60-turnorder-verdict | 61-levelgen-rust | 62-server-loop | 63-life-rules-status | 64-join-real-server-attempt | 65-life-rules-confirmed | 66-ai-is-gw1-evolution | 67-ai-parameters | 68-ai-token-set | 69-token-codes | 70-multiple-level-generators | 71-token-weights | 72-level-generator-modes | 73-mutation-operator | 74-fitness-formula-corrected | 75-generation-composition | 76-function-length | 77-ai-player | HANDOVER

## 四、以后怎么写

1. **一个主题一份现行文档**，改结论就改它，不新开编号；
2. 结论被推翻时，把旧文档移到"已推翻"并写明**被谁推翻**，不要删（过程有价值）；
3. 每份新文档第一行写 `状态：现行` 或 `状态：历史`；
4. 编号只增不复用，但**编号不等于优先级**——看本索引的分类。
