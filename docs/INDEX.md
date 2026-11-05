# docs 索引

> **规则**：根目录只放**长期有效的知识**（一屏能看完的索引 + 21 篇主题文档）。
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
| [07-protocol.md](07-protocol.md) | 事件枚举：73/73 覆盖、**72 个实测判别值**全表、一回合报文顺序 |
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
| [MIGRATION.md](MIGRATION.md) | **旧文件 → 归档/吸收** 的完整映射表（"没丢东西"的凭据） |
| [guide/README.md](guide/README.md) | 攻略向行为手册（另一套读者：想赢的人） |
