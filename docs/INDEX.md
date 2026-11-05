# docs 索引

> **规则**：`docs/` 只放**现行事实**（持续更新）；历史与已推翻的进 `docs/archive/`，只作过程档案。
> 一个主题只有一份现行文档。新结论改现行文档，不新开编号。

## 一、现行文档

| 文件 | 主题 | 说明 |
|---|---|---|
| **[AI.md](AI.md)** | **AI 函数进化（唯一权威）** | 取代 66–77 十二份逐轮记录 |
| [00-status.md](00-status.md) | 当前状态总览 | 当前状态总览（唯一权威） |
| [01-architecture.md](01-architecture.md) | 整体架构 | 整体架构 |
| [03-function-language.md](03-function-language.md) | 函数语言 | 函数语言 |
| [04-path-sampling.md](04-path-sampling.md) | 点序列采样（核心算法） | 点路径采样 |
| [05-shot-pipeline.md](05-shot-pipeline.md) | 一次开火的完整链路 | 开火链路 |
| [06-verification.md](06-verification.md) | 如何证明复原是对的 | 如何证明复原是对的 |
| [07-protocol.md](07-protocol.md) | 事件协议（联机与玩法状态机） | 事件协议 |
| [08-plane.md](08-plane.md) | 坐标系（graphwar_plane.rs） | 坐标系 |
| [15-end-to-end.md](15-end-to-end.md) | 端到端：从敲下函数到有人死亡 | 端到端 |
| [16-reproduce.md](16-reproduce.md) | 如何验证这份复原 | 复现步骤 |
| [17-resources.md](17-resources.md) | 游戏资源还原（rsc/） | 资源还原 |
| [24-network-protocol.md](24-network-protocol.md) | 网络协议（抓包还原） | 网络协议 |
| [78-replay-files.md](78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 | replay 文件位置与格式 |
| [79-replay-ai-corpus.md](79-replay-ai-corpus.md) | 从 replay 里按等级分离 AI 函数（真实数据验证） | 按等级分离 AI 函数 |
| [80-limit-vs-length.md](80-limit-vs-length.md) | 为什么低难度的 AI 会发射"超上限"的函数 | 低难度超限发射 |
| [83-physics-verified.md](83-physics-verified.md) | 物理验证通过：之前的"停止判定之谜"是我提取器的 bug | 物理验证通过 |
| [84-kills-verified.md](84-kills-verified.md) | 击杀数预测与真游戏完全一致 | 击杀数逐项一致 |
| [85-statistical-check.md](85-statistical-check.md) | 生成器 vs 真实机器人函数的统计对照（结论：一元可验，二元 | 统计验收 |
| [86-simultaneous-turns.md](86-simultaneous-turns.md) | 同时回合制（SimultaneousTurns）的击杀结算顺 | 同时回合制结算 |
| [87-function-assembly.md](87-function-assembly.md) | 函数装配规则（`docs/76` §4.5 的悬案，已解） | 函数装配规则（并入 AI.md） |
| [88-coverage-matrix.md](88-coverage-matrix.md) | 覆盖面矩阵：我的"绿"到底盖住了多大一块 | 覆盖面矩阵（唯一台账） |
| [89-real-step.md](89-real-step.md) | 实况步长是 **0.01**，不是 0.5（此前所有对比都用 | 实况步长 0.01 与早停留查 |
| [90-anti-waste.md](90-anti-waste.md) | 防无用功守则（本轮会话的失败清单与对策） | 防无用功守则（开工必读） |

## 二、已归档（`docs/archive/`，**不要引用其结论**）

| 归档项 | 被谁取代 |
|---|---|
| `archive/ai/`（66–77 共 12 份） | 合并为 `AI.md` |
| `archive/81-replay-validation.md` | 掩膜"半像素"推断被 `83` 推翻 |
| `archive/82-stop-test.md` | "停止判定不同"被 `83` 推翻（是提取器 bug） |
| `archive/28-turn-order.md` | 被 `47` 修正 |
| `archive/29-ai-probe.md` | AI 规则由 `66`+ 重做、现并入 `AI.md` |

## 三、其余未整理

编号 `09`–`27`、`60`–`65`、`01`–`08` 之外的早中期文档仍是按时间累积的过程档案，
尚未逐份归并。引用前先确认对应主题的现行文档。
