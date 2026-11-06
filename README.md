# gw2-rebuild —— Graphwar II 源码复原

本仓库目标：基于 `libgraphwarrust.so` 的静态逆向结果，**复原 Graphwar II 的实现并重写成可编译的源码**，
同时把"游戏到底怎么运作"讲清楚。

与上游 `graphwar2-analysis` 的关系：那是**逆向报告与对拍工具链**（`REPORT.md` +
`analysis/` + `graphwar_curve.py`）；本仓库是**实现本体**，独立托管在
`Cmd-GZ/graphwar2-rebuild`，与上游仓库分离、互不包含。

**现在是什么**（2026-11-07）：**内核**（曲线 / 物理 / 命中 / 协议 / broker / 服务端 / 关卡 / 战役数据 / AI 参数）
按真值复刻的部分**每一条都能指到证据**；`client/` 里 15 个界面能走完整流程。

⚠️ **但有两条必须先说清，否则本文档就是在虚报**：

1. **`client/` 不是原版源码，是照 `.tscn` 与截图重写的 GDScript。**
   原版的界面控制器在 **Rust 里**（`libgraphwarrust.so` 的 57 个 `graphwar_*.rs` 模块，
   含 `play_screen_control` / `entity_drawer` 等）；包里那 105 个 `.gdc` **全是小控件 + AdMob 插件**，
   **没有** `play_screen` / `player_screen` / `lan_lobby` / `campaign` / `GameSetup`。
   ⇒ 所以"回合推进 / 准备态 / 弹道生命周期 / 胜负时机"这些**流程**，在 `client/` 里是**推断出来的**，
   不是从原版读出来的。**这是当前最大的一块不诚实，也是下一步要迁回 Rust 的原因。**
2. **客户端自检的条数（现 581）不是复原进度。**
   那些断言是**写实现的人自己写的**，只能证明"和实现一致"，不能证明"和原版一致"。
   它现在的定位是**回归保护**。复原证据只看：反汇编地址 / 真实报文 / 真机真帧。

**验收标准**仍是 `docs/06` §17.1 的"逐位相同"（状态 / 消息字节及其批内顺序 / 几何三条全中）；
"能开一局"只是必要条件。当前**整局逐位**的实测是 **71/400（17.8%）**（见"总结"）。

## 目标二进制

| 文件 | 角色 | 规模 / 身份 |
|---|---|---|
| `libgraphwarrust.so` | GDExtension（Rust + godot-rust），**全部玩法逻辑** | 10 227 896 字节；sha256 `9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187`；stripped。函数边界以 Ghidra 的 **13503** 条为准（13263 条落在 `.text`）。`.eh_frame` 解出的 13430 条**不可用**（`docs/02` §12） |
| `Graphwar.x86_64` | Godot **4.7** 导出模板 | 73 433 400 字节（73 MB） |
| `Graphwar.pck` | Godot 资源包（pack v4，头里写 `godot=4.7.0`） | 20 569 396 字节、**692** 个文件：105 个 `.gdc`（仅 UI）+ **31** 个 `.scn` + 167 `.ctex` + 7 `.fontdata` ……**没有 `.svg` / `.ttf`**（`docs/17`、`docs/121` §4.1） |

⚠️ **版本别搞错**：本仓库所有地址、所有结论都针对上面**这一份 Godot 4.7 构建**（sha256 见表格）。
另有一个 **Godot 4.2.2 的旧构建**（`.so` 约 11 MB，地址布局与原版**完全不同**），
**不是目标**，只能作版本对比；拿它的地址来本仓库查会全部落空。该旧构建不在本次复核的工作区里，
它的 sha256 与函数表这里都没有记录（**未知**）。

关键约定：**Ghidra 镜像基址 `0x100000`**，文件地址 `0x3c2530` 在 Ghidra/REA 里是 `0x4c2530`（只加一次）。

## 进度（按**重建工作包**组织）

判定标准：**✅ 逐位 / 🔶 部分 / ⬜ 未做**。
「重建还缺什么」一列只写**具体缺的东西**，不写"待完善"。

> 按**重建这个游戏需要哪些工作包**列，不按"模块"列：按模块列会让人以为"全绿 = 能玩"。
> 2026-11-06 这版把三行**整行重写**：客户端界面（旧文"⬜ 未做"）、资源/音频（旧文"只有 1 个 humm.wav"）、
> 网络协议（旧文"46 种消息、缺 24 个"）；另新增 broker、战役数据、命中判定三行。

| 工作包 | 状态 | 证据（一句话 + 现行文档） | 重建还缺什么 |
|---|---|---|---|
| 函数语言与求值器 | ✅ 逐位 | 文法 90 例 + 37 夹具、求值 70 夹具与真二进制对拍；`^` 左结合、`log`=log10、隐式乘、缺操作数补 `CONST 0` 等怪癖全部落码（[03](docs/03-function-language.md)、[06](docs/06-verification.md)） | 算法无缺口。注：第三变量 `y'` 在全部 7 个求值调用点都传 `0`，是**游戏原样**，不是缺陷 |
| 弹道采样 | ✅ 逐位 | 补上 `hi = mid2`（[91](docs/91-bisection-loop.md)）后 **24/24 用例整条点序列与真二进制逐位相等**（修复前 17/24），两条验收点同时命中（[89 §32](docs/89-real-step.md)）；`tests/live_hits.rs` 12 条全程实况 `step=0.01` | ① `chord2 = NaN` 那条分支与汇编的等价性**未实测**（[91 §9.1](docs/91-bisection-loop.md)）；② `step≈50` 的越界参数域外仍多收 1 点（游戏不可达，[04 §7](docs/04-path-sampling.md)） |
| 障碍掩膜 / 碰撞 | ✅ 逐位 | 光栅化与 `is_occupied` 与真二进制**逐字节 / 300 随机点一致**；摧毁 = 命中点挖 `r=0.5` 的洞（frida 实证，[39](docs/39-punch-hole.md)） | ① 82 发里 **1 例**圆边像素个案未查清（[86 §9.2](docs/86-simultaneous-turns.md)）；② **通视检查**（`0x567530`）只到"观测"，未实现；③ 掩膜的持有者与重建时机未观测 |
| 命中判定与射手排除 | ✅ 逐位（规则）/ 🔶 落点 | 判定本体与内核**只有一份实现**：`api::hit_test` 走 `settle::resolve_points`，与 `resolve` 共用函数体（[120](docs/120-hit-test.md)）；"射手不自伤"实测为**按实体 id 排除**（`0x3c4452` 指令级），既不是"跳过开头点"也不是"挖洞"（[122](docs/122-self-hit.md)） | **`game.rs::fire` 里多留了一条 `1e-12` 位置排除**，二进制里只有按 id 排除；建议把规则下沉进内核（[122 §6](docs/122-self-hit.md)） |
| 回合 / 生命 / 结算 | ✅ 逐位 | 穿过即死、不挡弹道、无血量、友军误伤；回合状态机 6 状态 7 事件 **13 次迁移逐个相同**；同时回合制几何并行 + 事件按飞行时间（**152/152**、时刻判决 38:0，[86 §14](docs/86-simultaneous-turns.md)） | ① **出场顺序**属"缺一份熵"那类（见下方「两类非确定性」，[113](docs/113-hashmap-determinism.md)）；② **目标「打不全就全部复原」**：规则**形状**已钉住（只能翻 alive + 广播 `LifeInfo`；不重建实体、不恢复障碍洞 —— [123](docs/123-target-reset.md)），但**判定点未知**，397 份回放里 **0 次部分命中** ⇒ 没有真值可对 |
| 网络协议（事件） | ✅ 逐位 / 🔶 字段 | 二进制里**共 73 个事件名，73/73 全部实现**：**13 个**有真实抓包、做成强类型（键名/键序/`null` 逐位对拍），**10 个**字段名未证实，一律**原样透传、不猜键名**（[96](docs/96-protocol-coverage.md)）；`events.rs` 判别值从 33 个悬空值改成 **72 个实测值**（`0x3d38f0` 逐条解出，四道自校验），旧"判别值 = `.rodata` 位置"假设**已被证伪**；线上是 serde 外部标签 JSON，判别值不上线（[114](docs/114-event-tags.md)） | ① 10 个透传事件里 **9 个**的字段名与键序**没有任何真实流量佐证**（能收发、不能用来构造报文；第 10 个 `ClearObstacles` 是空体）；② 强类型结构体的键序只在 41 行抓包上同序，"serde 按声明顺序序列化"是**推断** ⇒ 换 serde 版本未对拍（[96 §9.1](docs/96-protocol-coverage.md)） |
| broker / 房间列表 | ✅ 逐位 / 🔶 覆盖面 | 协议**已还原并实现**（`src/broker.rs`，12 条测试）：与对局同一套帧（`u16` 大端 + JSON），会话 = 客户端发 `VersionInfo{version:"2.0.60",system:"linux"}` → broker 逐条推 `GameInfo{game_info}`；房间表项 9 字段另有 LAN 通道的运行时验证（[115](docs/115-broker.md)） | ① **`--broker_address` 在这个构建里完全不生效**（6 组受控实验），客户端取地址唯一路径是 HTTPS + **内编根证书** ⇒ **原版客户端无法被改指到自建 broker**；② 房主视角抓包、`GameHeartbeat`、`GameRemoved` 真实报文**都缺**（[115 §9](docs/115-broker.md)） |
| 服务端循环 | ✅ 逐位 | `serverloop::run` 在**真实 TCP** 上跑完整一局到 `EndingGame`；延迟删障碍 **165 tick**；射手轮转（`Vec<soldier_id>` + 游标）与"按士兵的 AI 进化层"**已接进** `game.rs::Match::begin_next_turn` / `graphwar_game_server::advance`，端到端断言"首发射手 = `soldiers[1]`""跳过死者""每回合都进化过"，**25 s 墙钟预算在这条路径上真的生效了**（[117](docs/117-server-wiring.md)） | ① 多客户端同房未实现（现在只支持单连接）；② 断线重连 / 超时踢人 / 房主迁移未做；③ Global 大厅与建房流程未做（客户端也不连服务器） |
| 关卡生成 | ✅ 逐位 / 🔶 覆盖面 | 随机散布模式 **13/13 逐位复现**（障碍 `x/y/r` 的 `to_bits()` 全等，[61](docs/61-levelgen-rust.md)）；中心单障碍 **4 个真值样本**（半径 3.34/4.81/6.75/5.69，全在 `u*5+3` 区间）；障碍**无固定坐标**（每份回放都不同）⇒ 关卡 = 参数 + 生成器；"圆盘式"已结案——它不是关卡生成器（[103](docs/103-disk-levelgen.md)） | 中心单障碍只有 **4 个**真值样本（缺更多回放/抓包真值来钉死半径区间 `u*5+3`） |
| 战役 21 关数据 | ✅ 数据解出 / 🔶 未落地 | **21 关数据硬编码在 Rust 里**（`level_info()` = `0x4ad380` 的 21 分支 `match`）；**21/21 标题、21/21 描述逐字节解出**，每关参数（障碍模式 / `min_count` / 士兵数 / 目标数 / Bot 难度 / 回合上限）**全部解出**，并与 **385 份真实回放交叉验证 21/21 全对**（[118 §7](docs/118-campaign-data.md) 是权威表）；`campaign.save` = 单行 `level_<n>`、**无尾换行**、0 基、语义 = 最后通关关卡、只增不减、启动时不创建（[118 §3](docs/118-campaign-data.md)） | ① 数据还**没填进客户端**：`client/src/app/campaign.gd` 仍写 PLACEHOLDER；② 我们 `src/` 里也还没有这份表 |
| AI 进化 | 🔶 部分 | 权重表（合计 85）/ 长度 `trunc(N(0,1)*5+10)` / 六档参数 / 装配 / 交叉 / 变异 / 每代构成 = 反汇编 + 40 万次运行时抽样；适应度逐条反汇编；终止 = **代数到顶 ∨ 25 s 墙钟**；精英 = 每代前 **20%**，字面量 5 = 每槽 1 克隆 + 4 工厂子代（[AI.md](docs/AI.md)、[93](docs/93-ai-time-budget.md)、[95](docs/95-elite5.md)）；第 0 代**没有独立初始化函数**（种群来自哈希表 move-out），**变体 B = 微分方程局**（[110](docs/110-gen0-and-variantb.md)） | ① **AI 产物上不了线**：进化出的是 token 序列，`token → 文本`（`0x4ec160`）未实现（[117 §4](docs/117-server-wiring.md)）；② 变体 B 的进化链未接；③ 搜索过程受 25 s 墙钟约束 ⇒ 属下面的 **B 类**，只能统计验收；④ DiffEq × AI 只验了变量槽权重 |
| **Godot 客户端** | 🔶 能走完整流程，**但不是原版源码** | 15 个界面能开一局、内核已接（`path_calculate` / `hit_test` / `level_obstacles` / `path_calculate_mode`，拔掉 `.so` 才退到 `preview`）；回放播放器、联机客户端、音频、战役数据**都已接**（`docs/133`/`134`/`128`/`125`）。⚠️ **但 `client/` 是照 `.tscn` 与截图重写的 GDScript**：原版界面控制器在 Rust 里，所以"回合推进/准备态/弹道生命周期/胜负时机"这些**流程**是推断出来的，且已实测出多处与原版不符。**下一步：把这些控制器迁回 Rust，视图层改用反编译出的原版 `.gd`**（`docs/146`） | ① 控制器迁回 Rust（57 个模块名已知）；② 视图层用 `recovered-src/` 的原版 `.gd`；③ 流程类 bug 逐条按 Rust 出处修（不要再"看起来合理");④ 新增脚本或 `.gdextension` 后**必须先** `godot --headless --path client --import`，否则静默不加载 |
| **脚本逻辑（`.gdc`）** | 🔶 已定性 | `.gdc` 是 `GDSC`+**zstd** 容器（字符串池 `c^0xb6` 混淆），`tools/gdc_strings.py` **105/105 全解**；51 个非插件脚本里 40 个是字号/间距布局助手、6 个空壳、**只有 5 个有行为**；启动快照 2724 条信号连接中**游戏 UI 只剩 6 条** ⇒ **界面逻辑在 Rust 里，不在脚本里**（[88 §12.0](docs/88-coverage-matrix.md)、[101](docs/101-gdc-contents.md)） | 布局助手的字符串已解出，可直接照抄；真正要做的是**把 Rust 侧的界面逻辑接出来**（我们已经在逆那份二进制），而不是等反编译器 |
| **资源 / 音频** | 🔶 还原完成，未接 | 资源 **692 文件 md5 全符**（`rsc/MANIFEST.tsv`，[17](docs/17-resources.md)）；音频 **10 个导入产物**（3 `.sample` + 6 `.mp3str` + 1 `.oggvorbisstr`）**四道独立检查全过**（PCK 自带 MD5 692/692、容器账本、mp3 帧链、PCM/Vorbis）；`humm.wav` 确证由 **Rust 侧 hound 解析**（`0x50a080`），走 `AudioStreamGenerator` + `push_buffer`（[111](docs/111-audio.md)）。"有一份 `music.ogg` 损坏"的来源已查明：那是使用者自己 `rebuild_ogg.py` 生成的（granule 全 0、缺 EOS 页），**不是游戏资源** | ① **音频播放链路没接进客户端**（`SoundHandler`、音量 `-15f`/`-5f`、`32767` 归一化都已还原，[111 §4](docs/111-audio.md)）；② **音效 ↔ 游戏事件**的对应关系未知（[111 §5.1](docs/111-audio.md)）；③ 仓库**不含任何媒体文件**（`.svg/.ttf/.png/.tres/.tscn/.pck` 一律不入库），要用自己的 `Graphwar.pck` 跑 `tools/extract_assets.py` 生成 `client/assets/`（gitignored） |
| **构建产物** | 🔶 部分 | `cargo build --release` 出 cdylib，`gdext_rust_init` 在真 Godot 4.7.2 注册成功；导出 `.pck` 能从包里启动并加载还原的纹理（[18](docs/18-build.md)，原 `19` 已并入其 §6） | 没有可玩的一体化产物：客户端要使用者自己的原版 `Graphwar.pck` 才能取到图标/字体；原扩展对外只有 **12 类 25 个方法**、几乎全是 UI 回调，玩法逻辑完全不开放（[18 §6.5](docs/18-build.md)） |

### 总结：现在到底差多少（2026-11-07 实测，按证据而非感觉）

**先给硬数字**：

| 指标 | 数值 | 口径 |
|---|---|---|
| 整局**完全一致**（消息级对拍） | **71/400 (17.8%)** | `docs/136`/`docs/142`，排除 A/B 类后仍严格逐条比 |
| 消息**种类序列**命中 | **916/1080**（84.8%） | 同时回合制批次结构修好后（`docs/141`） |
| 多发批**批结构** | **130/130** | 同上，剔掉死亡块 |
| 客户端自检 | 604 条 | ⚠️ **回归保护，不是复原证据** |
| `cargo test` | 349 条 | 单测/集成；同样不是"与原版相同"的证据 |

**要补的活**（按"是不是复原"排序，不按工作量）：

| # | 缺口 | 性质 |
|---|---|---|
| 1 | **客户端控制器迁回 Rust**（57 个模块名已知，`src/graphwar_*.rs`） | 现在那套 GDScript 是**仿制品**；只有迁回去，"还原源代码"才成立 |
| 2 | **源码原料回收**：105 个 `.gdc` → `.gd`、31 个 `.scn` → `.tscn`、22 个 `.res` → `.tres`、`project.binary` → `project.godot` | 进行中（`docs/146`）；这是"源码"这一层的地基 |
| 3 | **死亡集合 / `AshEffect` 发射器** | 对拍剩余失败的**主要来源**：37 个失败里 19 个是"死亡块整块是 `AshEffect`"、18 个是"死亡集合漏报"（`docs/141`） |
| 4 | **同时回合制的 A 类**：`TurnInfo` 那组玩家排列（HashMap 迭代序） | 缺一份未捕获的熵；批内其它顺序**已定死**（130/130） |
| 5 | **`game.rs::fire` 的 `1e-12` 位置排除** | 二进制里只有按实体 id 排除（`docs/122`） |
| 6 | **目标重置的判定点** | 形状已钉住，判定点未知，397 份回放里 0 次部分命中 ⇒ 无真值 |
| 7 | **界面像素**：主菜单 sha256 相同、弹窗 0.00–1.7%、玩家页 0.007%；但仍有屏缺真值 | 见 `docs/130`/`docs/137` |
| 8 | **跨平台**：Win/Android `.gdextension` 条目已补，**没编译过**；跨架构逐位一致未验证 | `docs/132` |
| 9 | **B 类**（25 s 墙钟、多客户端竞态） | 只能退化口径，永远做不到"逐位" |

### 两类"非确定性"的现状（旧文笼统写成"不可复现"，那是错的）

按"**结果是否由输入决定**"分两类（[06 §17.2](docs/06-verification.md)）：

| 类 | 判据 | 我们的两处 | 现状 |
|---|---|---|---|
| **A. 可复现，缺一份未捕获的熵** | 补一份熵就够 | **回合出场顺序**与**开局广播批内顺序** | **已实测**（[113](docs/113-hashmap-determinism.md)）：开局 13 类广播顺序逐局不同，约占一局 **45%** 报文；钉住 `getrandom` 的 16 字节后跨进程一致，连 ChaCha 种子一起钉则 **345/370 帧逐字节相同** ⇒ 属"**可复现但缺一份未捕获的熵**"，不是"做不到" |
| **B. 本质不由输入决定** | 再补多少输入也不够 | **25 s 墙钟截断**（依赖机器速度）、**多客户端竞态**（依赖调度） | 只能定**退化口径**并写死；口径见 [06 §17.2](docs/06-verification.md) |

⇒ 只有 A 类那一条是"补熵就能逐位"；B 类那两条永远只能是"退化口径"。别把这两类混成一类。

**Rust 实现状态**：`parse` / `eval` / `calculate` / `shot` / `plane` / `events` / `life` / `settle` /
`turnstate` / `protocol` / `api` / `broker` / `game` / `server` / `serverloop` / `levelgen` / `aigen` 均已可运行。
**`cargo test --offline` = 333 条全绿**（本机实测，0 失败；`src/` 共 23 个 `*.rs`）。
**客户端自检 = 604 条断言**：有资产时全绿；缺资产时 `175 passed, 10 skipped`，每条 skip 都显式报原因。
模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)。

## 文档

全部文档的索引（现行 / 已归档 / 已并档）见 **[`docs/INDEX.md`](docs/INDEX.md)**。
**一份文档一个主题**：同一主题只有一篇现行文档，已归并的进 `docs/archive/`。
`docs/` 下现行 **99 篇**（`ls docs/*.md | wc -l`），`docs/archive/` 另有 **32 篇**过程档案。
本表只列**现行**的与少数必须知道的指针，不是全量清单。

| 文档 | 内容 |
|---|---|
| [HANDOVER.md](docs/HANDOVER.md) | **给上下文压缩后的人/自己：一篇替代全部** |
| [00-status.md](docs/00-status.md) | 状态总览：什么已完成、什么只观测、什么还空白 |
| **[15-end-to-end.md](docs/15-end-to-end.md)** | **端到端：从敲下函数到有人死亡（建议先读这篇）** |
| **[06-verification.md](docs/06-verification.md)** | **如何证明复原正确**：证据分级、静态 oracle；**§17 = 整机"逐位相同"的口径与三处做不到的地方** |
| [01-architecture.md](docs/01-architecture.md) | 整体分层：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](docs/02-module-map.md) | ~90 个模块的清单、职责、函数↔模块映射；**§13 = 逐模块观测、§14 = 覆盖面口径** |
| [03-function-language.md](docs/03-function-language.md) | 玩家输入的函数语言：文法、优先级、全部怪癖 |
| [04-path-sampling.md](docs/04-path-sampling.md) | 等弦长二分采样：算法本体与全部常量 |
| [05-shot-pipeline.md](docs/05-shot-pipeline.md) | 一次开火的完整链路：位置 → 修正 → 采点 → 广播 |
| [07-protocol.md](docs/07-protocol.md) | 事件枚举、字段、变体顺序与传输线索 |
| [08-plane.md](docs/08-plane.md) | 坐标变换、碰撞掩膜、y 轴方向与光栅化舍入 |
| [16-reproduce.md](docs/16-reproduce.md) | **如何验证这份复原**：跑测试、真值来源、复现环境、环境坑 |
| [17-resources.md](docs/17-resources.md) | 游戏资源还原：PCK v4 格式、`rsc/` 生成与为何不入库 |
| [18-build.md](docs/18-build.md) | **构建与真引擎验证**：cdylib、`gdext_rust_init`、类/方法注册、`.pck` 启动（原 19 并入 §6） |
| [24-network-protocol.md](docs/24-network-protocol.md) | **网络协议抓包还原**：帧格式、LAN 广播、真实夹具（全量事件清单见 [96](docs/96-protocol-coverage.md)） |
| [25-live-collision.md](docs/25-live-collision.md) | 碰撞链路端到端验证（整关 4 发 / 6 发回放；含 `step=0.5` 勘误） |
| [30-kill-semantics.md](docs/30-kill-semantics.md) | **穿过即死，但不挡弹道** |
| [39-punch-hole.md](docs/39-punch-hole.md) | **障碍摧毁 = 命中点挖洞**（frida 实证） |
| [41-turn-state-machine.md](docs/41-turn-state-machine.md) | 回合状态机（13 次迁移逐个相同） |
| [43-turn-message-sequence.md](docs/43-turn-message-sequence.md) | 一回合的报文顺序 |
| **[47-turn-order-random.md](docs/47-turn-order-random.md)** | **回合出场顺序**（吸收 59/60：已证伪来自 ChaCha 流；后续实测见 [113](docs/113-hashmap-determinism.md)） |
| [61-levelgen-rust.md](docs/61-levelgen-rust.md) | 关卡生成的 Rust 实现（逐位复现 13/13） |
| [62-server-loop.md](docs/62-server-loop.md) | 能跑完整一局的服务端；**§5 = 自建客户端连真服务器（原 64，未打通）** |
| **[65-life-rules-confirmed.md](docs/65-life-rules-confirmed.md)** | **生命/致死规则：现行唯一文档**（吸收 27/63：无血量 / 友军致死 / 按弹道先后） |
| [78-replay-files.md](docs/78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 |
| [86-simultaneous-turns.md](docs/86-simultaneous-turns.md) | **同时回合制的击杀结算顺序**：几何并行、事件按飞行时间 |
| [88-coverage-matrix.md](docs/88-coverage-matrix.md) | **覆盖面矩阵**：我的"绿"到底盖住了多大一块 |
| [89-real-step.md](docs/89-real-step.md) | 实况步长 **0.01**，以及早停缺陷的定位全过程 |
| [90-anti-waste.md](docs/90-anti-waste.md) | 防无用功守则（开工必读；含地址空间换算与 REA 陷阱） |
| [91-bisection-loop.md](docs/91-bisection-loop.md) | 二分循环的逐条汇编：`lo = x` **且** `hi = mid2`（改码依据） |
| [92-ui-inventory.md](docs/92-ui-inventory.md) | 客户端界面台账（15/15 界面，运行时枚举） |
| [96-protocol-coverage.md](docs/96-protocol-coverage.md) | **协议事件全量清单：73/73 已实现**（13 强类型 + 10 透传） |
| [101-gdc-contents.md](docs/101-gdc-contents.md) | `.gdc` 常量/标识符/函数/伪源码（105/105） |
| [103-disk-levelgen.md](docs/103-disk-levelgen.md) | `0x4ea6dc` **不是**关卡生成器；关卡只有两种 |
| [104-recovered-project.md](docs/104-recovered-project.md) | GDRE 整项目恢复：静态场景属性，与 `92` 运行时台账对拍 |
| [108-client-rebuild.md](docs/108-client-rebuild.md) | **Godot 客户端重建**：15 界面、命令键协议、字体/图标策略、自定规则台账 |
| [109-kernel-turn-aigen.md](docs/109-kernel-turn-aigen.md) | 射手轮转与按士兵的 AI 进化层 |
| [110-gen0-and-variantb.md](docs/110-gen0-and-variantb.md) | 第 0 代种群来源、变体 A/B 的分派条件 |
| [111-audio.md](docs/111-audio.md) | **音频**：10 个导入产物四道校验、`humm.wav` 由 Rust/hound 解析、播放链路与音量立即数 |
| [113-hashmap-determinism.md](docs/113-hashmap-determinism.md) | **HashMap 迭代序的运行时实证**：45% 报文、钉熵后 345/370 帧相同 |
| [114-event-tags.md](docs/114-event-tags.md) | **事件判别值实测表**：72 条、四道自校验 |
| [115-broker.md](docs/115-broker.md) | **broker 协议 + 实现**；`--broker_address` 在这个构建里是死的 |
| [116-kernel-api.md](docs/116-kernel-api.md) | **内核暴露给 Godot**：`GW2Rebuild.path_calculate` 与 `.gdextension`/`--import` 陷阱 |
| [117-server-wiring.md](docs/117-server-wiring.md) | 回合轮转与 AI 进化接入服务端循环（25 s 预算生效） |
| [118-campaign-data.md](docs/118-campaign-data.md) | **战役 21 关**：数据位置、`campaign.save` 格式、21 关全参数表（385 份回放交叉验证） |
| [120-hit-test.md](docs/120-hit-test.md) | **命中判定接内核**：`GW2Rebuild.hit_test`，与 `settle` 只有一份实现 |
| [121-visual-fidelity.md](docs/121-visual-fidelity.md) | **客户端视觉保真**：启动灰闪、深绿取值、图标改真资产；§7 = 仍存在的差异 |
| [122-self-hit.md](docs/122-self-hit.md) | **射手不自伤 = 按实体 id 排除**（指令级） |
| [123-target-reset.md](docs/123-target-reset.md) | **目标「打不全就全部复原」**：形状已钉住（翻 alive + 广播 `LifeInfo`）、判定点未知；逐关特殊规则复核 |
| **[107-docs-consolidation.md](docs/107-docs-consolidation.md)** | **文档技术债清理**：家底清单、归并映射、改掉的陈旧断言 |
| [AI.md](docs/AI.md) | **AI 函数进化（唯一权威）** |

已归档的 32 篇（含 `archive/ai/` 12 份 AI 逐轮记录）与 5 份"已并档"占位指针不再在此列出，
见 [`docs/INDEX.md`](docs/INDEX.md) §2/§3。

## 对外接口

**GDExtension 方法**（`GW2Rebuild` 类；`docs/116`、`docs/120`）：

```
GW2Rebuild.path_calculate(expr, x0, y0, direction)
    -> { ok, points: PackedVector2Array, offset, error }   弹道点序列（棋盘坐标）
GW2Rebuild.hit_test(points, obstacles, soldiers)
    -> { ok, hits, hit_points, first_hit, stop, blocked, error }   掩膜命中判定
```

**C ABI 符号**（为对拍自加，`docs/16`）：

```
gdext_rust_init                                            Godot GDExtension 入口
gw2_parse_len(text)                                        -> token 个数
gw2_shot_points(text,x0,y0,step,flip,ode,out,cap)          -> 点数（xy 交替写 out）
gw2_correction(text,x0,y0,flip)                            -> 修正常数
gw2_rasterize(x,y,size,out,len,fill)                       -> 0（写掩膜）
gw2_is_occupied(mask,len,x,y)                              -> 1 命中 / 0 空
gw2_simulate(text,x0,y0,step,flip,ode,obstacles,n,count)   -> 首个命中序号 / -1
```

注意：这 6 个 `gw2_*` 是**我们为对拍自加的验证接口**，不是游戏原扩展的接口面——
原扩展只暴露 12 类 25 个方法，且几乎全是 UI 回调（`18` §6.5）。

## 目录

```
docs/        逆向得到的实现说明（99 篇现行 + archive/ 32 篇过程档案）
src/         Rust 复原实现（可编译，23 个模块）
client/      Godot 4.7.2 重建客户端（15 个界面；媒体资产不入库，用 tools/extract_assets.py 从自己的 pck 解）
tests/       与真二进制对齐用的用例（cargo test 333 条）
fixtures/    真值夹具（已入库）
tools/       分析脚本（含 extract_assets.py / parse_replay.py / campaign_data.py 等离线工具）
rsc/         ← gitignore，用 tools/pck_extract.py 生成
```

本仓库**不存放任何媒体文件**：`.svg / .ttf / .png / .tres / .tscn / .pck` 一律不入库（版权 + 体积）。
`client/assets/` 由使用者自己的 `Graphwar.pck` 生成，已 gitignore。

## 与上游工具链的关系

真值判定不在本仓库：`../analysis/emu.py` 用 unicorn 把 `.so` 当库调用，
`graphwar_curve.py` 是已证明逐位一致的 Python 参照实现。本仓库的代码若改动，
必须回到上游那套对拍里验证，而不是自证。
