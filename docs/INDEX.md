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
| [91-bisection-loop.md](91-bisection-loop.md) | 采样器 normal 二分循环的逐条汇编：`lo`/`hi` 更新规则 | 二分 `lo`/`hi` 规则（早停根因，改码依据） |
| [92-ui-inventory.md](92-ui-inventory.md) | **客户端界面台账（15/15 界面，运行时枚举）** | 界面 × 控件 × 作用 × 实现状态（唯一台账） |

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

## 四、已推翻但仍留在 `docs/` 的文件（**别引用其结论**）

本节是 2026-11 的核对结果。规则说"已推翻的进 `archive/`"，但下面这些文件虽已被推翻/取代，
仍留在 `docs/`（它们同时含有仍然有效的观测，暂时删不动），所以在这里显式登记：

| 文件 | 已被谁推翻 / 取代 | 说明 |
|---|---|---|
| [12-ai.md](12-ai.md) | [`AI.md`](AI.md)（现行唯一权威） | 标题即"评分规则未还原"，与现状不符；只剩结构观测的价值 |
| [38-dynamic-mask.md](38-dynamic-mask.md) | 被数据否定 → [39-punch-hole.md](39-punch-hole.md) | "动态掩膜"假设不成立；摧毁=命中点挖洞 |
| [40-level-generator.md](40-level-generator.md) | [50](50-rng-source-reopened.md) / [55](55-level-generator-closed.md) / [61](61-levelgen-rust.md) | "OS 熵 ⇒ 关卡不可复现"是错的（当时统计到的是 libudev 的 getrandom）；生成已逐位复现 13/13 |
| [89-real-step.md](89-real-step.md) §5–§29 | [91](91-bisection-loop.md) 与 `89` 自己的 §31/§32 | 该文的**步长结论（0.01）仍现行**；但"早停"的整条推理（`lo = x` 是刻意的 / 差一次二分迭代 / 接受上一轮 mid）**已被 §31/§32 全部更正**，只当过程档案读 |
| [83](83-physics-verified.md) / [84](84-kills-verified.md) 里与步长有关的表述 | [89](89-real-step.md) §2、[86](86-simultaneous-turns.md) §14 | 结论未被推翻，但那批数字（21 发差 <0.5、击杀数 12 发一致）是 `step=0.5` 测的；实况 `0.01` 的强证据在 `86` §14（152/152、38:0） |

## 五、其他登记（2026-11 本轮追加）

| 文件 | 说明 |
|---|---|
| [HANDOVER.md](HANDOVER.md) | 给上下文压缩后的自己：一页替代全部（**建议先读**） |
| [00-status.md](00-status.md) | 已于 2026-11 重写：此前停在"目标轮次 7"，把回合/扣血/关卡/AI 写成空白 |
| [README.md](../README.md) | 进度表改为**按重建工作包组织**（12 行，带"重建还缺什么"列） |
