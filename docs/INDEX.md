# docs 索引

> **规则**：根目录只放**长期有效的知识**（一屏能看完的索引 + 主题文档）。
> **审计 2026-11-08**：原文写「21 篇主题文档」，实测根目录 **32 篇**（`ls docs/*.md | wc -l`）；
> 篇数会变，本页只承诺「结构」不承诺条数（同一规矩见 `AGENTS.md` 的目录一节）。
> 历史、已推翻的结论、过程档案全在 `archive/`；每篇归档的原因写在 [`MIGRATION.md`](MIGRATION.md)。
> **四档证据标记**的定义在 [`06-verification.md`](06-verification.md) §1。

## 先读这三篇

| 文档 | 解决什么问题 |
|---|---|
| [00-status.md](00-status.md) | 现在到哪了、还缺什么、**诚实声明**（唯一权威状态页） |
| [HANDOVER.md](HANDOVER.md) | 上下文压缩后怎么接着干（约定、坑、常用命令） |
| [06-verification.md](06-verification.md) | 怎么证明复原是对的：四档定义 + 哪些**证明做不到** |

## 内核（读这一串就懂全链）

| 文档 | 解决什么问题 |
|---|---|
| [01-architecture.md](01-architecture.md) | 三层结构：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](02-module-map.md) | 原版 57+ 个 Rust 模块 ↔ 我们 `src/` 的对照（含对局元信息 8 模块） |
| [03-function-language.md](03-function-language.md) | 玩家输入的函数语言：文法、优先级、怪癖、**token → 文本** |
| [04-path-sampling.md](04-path-sampling.md) | 等弦长二分采样（核心算法、全部常量、复检的逐条指令） |
| [05-shot-pipeline.md](05-shot-pipeline.md) | 一次开火的完整链路 + 端到端串讲 + `Mode::Ode` |
| [08-plane.md](08-plane.md) | 坐标系、碰撞掩膜、光栅化、**摧毁 = 挖洞** |
| [15-target-life.md](15-target-life.md) | 致死规则、死亡报文、射手不自伤、靶子与"打不全就复原" |

## 联机与对局

| 文档 | 解决什么问题 |
|---|---|
| [07-protocol.md](07-protocol.md) | 事件枚举：**74/74 覆盖**、**74 条判别值**全表（72 条 `cmp` 链 + `63`/`65` 跳转表）、一回合报文顺序 |
| [09-network.md](09-network.md) | 帧格式、握手 7 条、`Match`、整局循环、**broker 协议** |
| [14-turn.md](14-turn.md) | tick 时钟、回合状态机、同时回合制的结算顺序与广播分组 |
| [13-ai.md](13-ai.md) | AI 函数进化（唯一权威）：参数、评分、25 s 预算、变体 A/B |
| [12-rng.md](12-rng.md) | 随机源与**两类非确定性**：ChaCha（不可复现）vs HashMap 序（可钉死） |
| [11-levelgen.md](11-levelgen.md) | 关卡生成：两种生产者、公式、逐位复现 13/13 |

## 内容与外壳

| 文档 | 解决什么问题 |
|---|---|
| [10-campaign.md](10-campaign.md) | 战役 21 关（**权威参数表**）、存档格式、胜负判定、队伍与颜色 |
| [16-client.md](16-client.md) | 客户端：15 界面台账、内核接缝、像素保真、回放/联机/音频/开火表现 |
| [17-resources.md](17-resources.md) | PCK v4 格式、`rsc/` 构成、音频产物逐个验证与播放链路 |
| [18-build.md](18-build.md) | GDExtension 骨架 → 真 Godot 4.7.2 里注册并跑起来 |
| [19-android.md](19-android.md) | 三平台官方包、广告与内购的真实链路、跨平台构建 |
| [20-source-recovery.md](20-source-recovery.md) | 原版**源码面**的回收：`.gdc` 字节码、GDRE 整项目恢复与对拍 |
| [21-guide-manual.md](21-guide-manual.md) | 攻略手册的元文档 + **矛盾台账**（素材打架的地方） |

## 其他

| 文档 | 解决什么问题 |
|---|---|
| [151-addr-open-items.md](151-addr-open-items.md) | **地址级悬案清单**：哪些"未知"只差一步、下一步查什么、哪几条查了也定不了（含 6 条已裁定） |
| [152-audit-of-merged-claims.md](152-audit-of-merged-claims.md) | **对已合并结论的对抗性审计**：独立复算 8 项、查出并就地更正 4 处（条件码 / 函数体尾 / 清单缺项 / 旁证推广） |
| [153-audit-downstream.md](153-audit-downstream.md) | **下游两轮 14 份任务报告（CG 分支）的对抗性复算**：18 项，查出 2 处轴/口径错配 + 1 处反事实当结果（1074 属 `batch_conv` / 收敛层 198-291 是预测 / 134-259 复现不出来） |
| [158-audit-407.md](158-audit-407.md) | **§25「407 批 = 回放自身不自洽」的对抗性审计**：独立重写读取器 + 复算器，A 262 / B 218 / ∩73 / ∪407 / 漏 0 **全部复现**、正对照 647 批 0 误报 ⇒ **判决成立**；就地更正 2 处口径（§25.1 常数取法、§25.6 副表），并记下父仓库 oracle 在无主 `)` 上截断解析式 |
| [159-audit-fingerprint.md](159-audit-fingerprint.md) | **§35「2,803 高置信结构同源」的对抗性审计**：84.2% 与 98.0% **逐位复现**、四份指纹库规模复现、抽检 20 对全部「像」（独立 objdump+序列比，20/20 唯一最佳）、反向检验 7/10 地址与名字都对（6 个高置信里 5 个对）；**查出 2 处必须打折**（`strs` 特征就是 Location 文件名，清零后 84.2%→73.3%；单射闸门只在分片内生效，56 行跨片共靶）⇒ 判决 **需打折**，并就地更正 §35 四处口径 |
| [144-lobby-fixes.md](144-lobby-fixes.md) | 大厅 / 房间列表的修正（**审计 2026-11-08**：原文缺索引行，本次补上） |
| [147-ai-behaviour.md](147-ai-behaviour.md) | AI 行为面（评分 / 世代 / 可观测统计）（同上补行） |
| [149-coverage-matrix.md](149-coverage-matrix.md) | **覆盖率矩阵**：按原版模块算的进度尺（`tools/coverage_matrix.py` 生成，勿手改）（同上补行） |
| [150-win-rule.md](150-win-rule.md) | **胜负条件裁定**：一条规则还是两条；推翻 `archive/131` §2 的两分口径（同上补行） |
| [154-docs-audit.md](154-docs-audit.md) | **文档审计（2026-11-08）**：过时数字 / 互相冲突 / 可整合 / 失效引用四类，逐条给处置 |
| [MIGRATION.md](MIGRATION.md) | **旧文件 → 归档/吸收** 的完整映射表（"没丢东西"的凭据） |
| [guide/README.md](guide/README.md) | 攻略向行为手册（另一套读者：想赢的人） |

## 过程档案（`archive/`：逐轮过程数据；**结论不在这里**，索引在对应主文档）

| 文件 | 里面是什么 |
|---|---|
| [155-round-log.md](archive/155-round-log.md) | `docs/06` **§24–§43** 的逐轮归因日志（2026-11-08 原样搬迁，节号不变） |
| [156-client-rounds.md](archive/156-client-rounds.md) | `docs/16` **§42–§51** 的逐轮"接线 / 接节点"记录（同上） |
| [157-name-matching.md](archive/157-name-matching.md) | `docs/02` **§34–§40** 的名字匹配三表过程记录（同上） |

> 搬迁口径与逐字自证见 [`154-docs-audit.md`](154-docs-audit.md) 附录 A；
> 三份的入口分别在 [`06`](06-verification.md) §24、[`16`](16-client.md) §42、[`02`](02-module-map.md) §34。
