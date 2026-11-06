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
| 弹道采样 | ✅ 逐位 | 补上 `hi = mid2`（[91](docs/archive/91-bisection-loop.md)）后 **24/24 用例整条点序列与真二进制逐位相等**（修复前 17/24），两条验收点同时命中（[89 §32](docs/archive/89-real-step.md)）；`tests/live_hits.rs` 12 条全程实况 `step=0.01` | ① `chord2 = NaN` 那条分支与汇编的等价性**未实测**（[91 §9.1](docs/archive/91-bisection-loop.md)）；② `step≈50` 的越界参数域外仍多收 1 点（游戏不可达，[04 §7](docs/04-path-sampling.md)） |
| 障碍掩膜 / 碰撞 | ✅ 逐位 | 光栅化与 `is_occupied` 与真二进制**逐字节 / 300 随机点一致**；摧毁 = 命中点挖 `r=0.5` 的洞（frida 实证，[39](docs/archive/39-punch-hole.md)） | ① 82 发里 **1 例**圆边像素个案未查清（[86 §9.2](docs/archive/86-simultaneous-turns.md)）；② **通视检查**（`0x567530`）只到"观测"，未实现；③ 掩膜的持有者与重建时机未观测 |
| 命中判定与射手排除 | ✅ 逐位（规则）/ 🔶 落点 | 判定本体与内核**只有一份实现**：`_scaffold::api::hit_test` 走 `graphwar_shot_calculator::settle::resolve_points`，与 `resolve` 共用函数体（[120](docs/archive/120-hit-test.md)）；"射手不自伤"实测为**按实体 id 排除**（`0x3c4452` 指令级），既不是"跳过开头点"也不是"挖洞"（[122](docs/archive/122-self-hit.md)） | **`game.rs::fire` 里多留了一条 `1e-12` 位置排除**，二进制里只有按 id 排除；建议把规则下沉进内核（[122 §6](docs/archive/122-self-hit.md)） |
| 回合 / 生命 / 结算 | ✅ 逐位 | 穿过即死、不挡弹道、无血量、友军误伤；回合状态机 6 状态 7 事件 **13 次迁移逐个相同**；同时回合制几何并行 + 事件按飞行时间（**152/152**、时刻判决 38:0，[86 §14](docs/archive/86-simultaneous-turns.md)） | ① **出场顺序**属"缺一份熵"那类（见下方「两类非确定性」，[113](docs/archive/113-hashmap-determinism.md)）；② **目标「打不全就全部复原」**：判定点**已定位**（2026-11-07）——`GameStateManager::queue_end_turn_trigger::{{closure}}`（arm64 `0x4abc00`）：回合末 ∧ 两队都还有人活着 ∧ `[+0x11d]==1` ⇒ 遍历靶子集合逐个 `set_alive(id,true,0)`；`target_manager` 本身只是 `HashSet<u32>`（无 alive 字段）。形状见 [123](docs/archive/123-target-reset.md)，指令级见 `docs/16` §32。⚠️ 397 份回放里 **0 次部分命中** ⇒ 仍无真值可对拍 |
| 网络协议（事件） | ✅ **74/74 全字段** | 二进制里**共 74 个事件名**（旧文写 73，漏了 `VersionInfo`）：**全部 74 个**都有**字段名 + 线序 + 类型**，可从二进制复现（`tools/wire_fields.py --verify`）；原来"10 个透传、不猜键名"的那批**已全部升级成强类型**——字段名不是猜的，是从 serde 的 `serialize_field("键名", 值)` **调用序**读出来的（LLVM 内联后是 `lea rsi,[rip+名字]` + `mov edx,长度` + `call`）。三道交叉验证：字段数 == `struct … with N elements` 的 N（**67/67**）、`Deserialize` 侧同名比较（**67/67**）、抓包键序 == 二进制键序（**13/13 强类型 + 48 种事件 + 1 条 LAN 广播，共 133 行全同**）⇒ 旧的"serde 按声明顺序序列化"由**推断升为逐位+观测**（`docs/07` §3 是 74 条权威规格表） | **已无未知**（2026-11-07 收口）：`GameHeartbeat = 63` / `VersionInfo = 65` 由**两条机制不同**的独立证据互校 **72/72** 定死（x86 侧 serde `VARIANTS` 表 `.data.rel.ro 0x961608`、arm64 侧 `GraphwarEvent::serialize` 的 74 格跳转表 `0x19e770`）；**7 个线上枚举全部穷举**（判据是结构性的：serde `unknown_variant` 把表地址与长度都写成指令立即数，全 `.text` 扫该形状**恰好命中 8 张表，没有第 9 张**）——`AxisMode` 4 / `FunctionMode` 2 / `TurnMode` 2 / `TimeMode` 6 / `GameState` 8 / `GraphwarTeam` 2 / `EffectType` 4（`docs/07` §5） |
| broker / 房间列表 | ✅ 逐位 / 🔶 覆盖面 | 协议**已还原并实现**（`src/_scaffold/broker.rs`，12 条测试）：与对局同一套帧（`u16` 大端 + JSON），会话 = 客户端发 `VersionInfo{version:"2.0.60",system:"linux"}` → broker 逐条推 `GameInfo{game_info}`；房间表项 9 字段另有 LAN 通道的运行时验证（[115](docs/archive/115-broker.md)） | ① **`--broker_address` 在这个构建里完全不生效**（6 组受控实验），客户端取地址唯一路径是 HTTPS + **内编根证书** ⇒ **原版客户端无法被改指到自建 broker**；② 房主视角抓包、`GameHeartbeat`、`GameRemoved` 真实报文**都缺**（[115 §9](docs/archive/115-broker.md)） |
| 服务端循环 | ✅ 逐位 | `graphwar_game_server::run` 在**真实 TCP** 上跑完整一局到 `EndingGame`；延迟删障碍 **165 tick**；射手轮转（`Vec<soldier_id>` + 游标）与"按士兵的 AI 进化层"**已接进** `game.rs::Match::begin_next_turn` / `graphwar_game_server::advance`，端到端断言"首发射手 = `soldiers[1]`""跳过死者""每回合都进化过"，**25 s 墙钟预算在这条路径上真的生效了**（[117](docs/archive/117-server-wiring.md)） | ① 多客户端同房未实现（现在只支持单连接）；② 断线重连 / 超时踢人 / 房主迁移未做；③ Global 大厅与建房流程未做（客户端也不连服务器） |
| 关卡生成 | ✅ 逐位 / 🔶 覆盖面 | 随机散布模式 **13/13 逐位复现**（障碍 `x/y/r` 的 `to_bits()` 全等，[61](docs/archive/61-levelgen-rust.md)）；中心单障碍 **4 个真值样本**（半径 3.34/4.81/6.75/5.69，全在 `u*5+3` 区间）；障碍**无固定坐标**（每份回放都不同）⇒ 关卡 = 参数 + 生成器；"圆盘式"已结案——它不是关卡生成器（[103](docs/archive/103-disk-levelgen.md)） | 中心单障碍只有 **4 个**真值样本（缺更多回放/抓包真值来钉死半径区间 `u*5+3`） |
| 战役 21 关数据 | ✅ 数据解出 / 🔶 未落地 | **21 关数据硬编码在 Rust 里**（`level_info()` = `0x4ad380` 的 21 分支 `match`）；**21/21 标题、21/21 描述逐字节解出**，每关参数（障碍模式 / `min_count` / 士兵数 / 目标数 / Bot 难度 / 回合上限）**全部解出**，并与 **385 份真实回放交叉验证 21/21 全对**（[118 §7](docs/archive/118-campaign-data.md) 是权威表）；`campaign.save` = 单行 `level_<n>`、**无尾换行**、0 基、语义 = 最后通关关卡、只增不减、启动时不创建（[118 §3](docs/archive/118-campaign-data.md)） | ① 数据还**没填进客户端**：`client/src/app/campaign.gd` 仍写 PLACEHOLDER；② 这份表正在落进 `src/`（`graphwar_campaign_control` / `graphwar_level_generator` 已按原版边界还原，21 关参数表见 `docs/118` §7） |
| AI 进化 | 🔶 部分 | 权重表（合计 85）/ 长度 `trunc(N(0,1)*5+10)` / 六档参数 / 装配 / 交叉 / 变异 / 每代构成 = 反汇编 + 40 万次运行时抽样；适应度逐条反汇编；终止 = **代数到顶 ∨ 25 s 墙钟**；精英 = 每代前 **20%**，字面量 5 = 每槽 1 克隆 + 4 工厂子代（[AI.md](docs/archive/AI.md)、[93](docs/archive/93-ai-time-budget.md)、[95](docs/archive/95-elite5.md)）；第 0 代**没有独立初始化函数**（种群来自哈希表 move-out），**变体 B = 微分方程局**（[110](docs/archive/110-gen0-and-variantb.md)） | ① **AI 产物上线**：`token → 文本`（`0x4ec160`）**已实现**（`src/math_function/to_text.rs`，与求值器同一源文件的两个方向），[117 §4](docs/archive/117-server-wiring.md) 那条缺口已关闭；② 变体 B 的进化链未接；③ 搜索过程受 25 s 墙钟约束 ⇒ 属下面的 **B 类**，只能统计验收；④ DiffEq × AI 只验了变量槽权重 |
| **Godot 客户端** | 🔶 能走完整流程，**但不是原版源码** | 15 个界面能开一局、内核已接（`path_calculate` / `hit_test` / `level_obstacles` / `path_calculate_mode`，拔掉 `.so` 才退到 `preview`）；回放播放器、联机客户端、音频、战役数据**都已接**（`docs/133`/`134`/`128`/`125`）。⚠️ **但 `client/` 是照 `.tscn` 与截图重写的 GDScript**：原版界面控制器在 Rust 里，所以"回合推进/准备态/弹道生命周期/胜负时机"这些**流程**是推断出来的，且已实测出多处与原版不符。**下一步：把这些控制器迁回 Rust，视图层改用反编译出的原版 `.gd`**（`docs/146`）。⚠️ 2026-11-07 更新：那 12 个控制器的**逻辑**已经全部还原进 `src/`（纯 Rust、可测），但它们是**知识不是节点**——没注册成 Godot 类、也不调引擎 API；目前只有 `AxisDrawer` 验证过能被引擎实例化（见上方「能不能真的跑起来」） | ① 控制器迁回 Rust（57 个模块名已知）；② 视图层用 `recovered-src/` 的原版 `.gd`；③ 流程类 bug 逐条按 Rust 出处修（不要再"看起来合理");④ 新增脚本或 `.gdextension` 后**必须先** `godot --headless --path client --import`，否则静默不加载 |
| **脚本逻辑（`.gdc`）** | 🔶 已定性 | `.gdc` 是 `GDSC`+**zstd** 容器（字符串池 `c^0xb6` 混淆），`tools/gdc_strings.py` **105/105 全解**；51 个非插件脚本里 40 个是字号/间距布局助手、6 个空壳、**只有 5 个有行为**；启动快照 2724 条信号连接中**游戏 UI 只剩 6 条** ⇒ **界面逻辑在 Rust 里，不在脚本里**（[88 §12.0](docs/archive/88-coverage-matrix.md)、[101](docs/archive/101-gdc-contents.md)） | 布局助手的字符串已解出，可直接照抄；真正要做的是**把 Rust 侧的界面逻辑接出来**（我们已经在逆那份二进制），而不是等反编译器 |
| **资源 / 音频** | 🔶 还原完成，未接 | 资源 **692 文件 md5 全符**（`rsc/MANIFEST.tsv`，[17](docs/17-resources.md)）；音频 **10 个导入产物**（3 `.sample` + 6 `.mp3str` + 1 `.oggvorbisstr`）**四道独立检查全过**（PCK 自带 MD5 692/692、容器账本、mp3 帧链、PCM/Vorbis）；`humm.wav` 确证由 **Rust 侧 hound 解析**（`0x50a080`），走 `AudioStreamGenerator` + `push_buffer`（[111](docs/archive/111-audio.md)）。"有一份 `music.ogg` 损坏"的来源已查明：那是使用者自己 `rebuild_ogg.py` 生成的（granule 全 0、缺 EOS 页），**不是游戏资源** | ① **音频播放链路没接进客户端**（`SoundHandler`、音量 `-15f`/`-5f`、`32767` 归一化都已还原，[111 §4](docs/archive/111-audio.md)）；② **音效 ↔ 游戏事件已解**（`docs/16` §30）：订阅 `effect_info`(44，按 `+0x20` 的 u8 四臂) / `soldier_info`(15) / `entity_removed`(14) / `game_state_info`(30)；槽 `+0x80/90/a0/b0/c0` = explosion/squash/spark/pop/unpop，音量 −15/−15/−15/0/−5 dB；`state==1` 清待播队列；③ 仓库**不含任何媒体文件**（`.svg/.ttf/.png/.tres/.tscn/.pck` 一律不入库），要用自己的 `Graphwar.pck` 跑 `tools/extract_assets.py` 生成 `client/assets/`（gitignored） |
| **构建产物** | 🔶 部分 | `cargo build --release` 出 cdylib，`gdext_rust_init` 在真 Godot 4.7.2 注册成功；导出 `.pck` 能从包里启动并加载还原的纹理（[18](docs/18-build.md)，原 `19` 已并入其 §6） | 没有可玩的一体化产物：客户端要使用者自己的原版 `Graphwar.pck` 才能取到图标/字体；原扩展对外只有 **12 类 25 个方法**、几乎全是 UI 回调，玩法逻辑完全不开放（[18 §6.5](docs/18-build.md)） |

### 总结：现在到底差多少（2026-11-07 实测，按证据而非感觉）

**先给硬数字**：

| 指标 | 数值 | 口径 |
|---|---|---|
| 整局**完全一致**（消息级对拍） | **71/400 (17.8%)** | `docs/136`/`docs/142`，排除 A/B 类后仍严格逐条比 |
| 消息**种类序列**命中 | **916/1080**（84.8%） | 同时回合制批次结构修好后（`docs/141`） |
| 多发批**批结构** | **130/130** | 同上，剔掉死亡块 |
| 客户端自检 | **694 条**（2026-11-07 深夜实测，0 失败） | ⚠️ **回归保护，不是复原证据** |
| `cargo test` | **1488 条**（lib **457** + 51 个集成测试二进制） | 单测/集成；同样不是"与原版相同"的证据 |

**要补的活**（按"是不是复原"排序，不按工作量）：

| # | 缺口 | 性质 |
|---|---|---|
| 1 | **客户端控制器迁回 Rust** —— **78 个模块名已权威确定**（`docs/02` §18），**12 个 GDExtension 类已还原 11 个**（只剩 `GraphwarPlane`）；控制器仍在 `client/` 那套 GDScript 里，它是**仿制品**，只有迁回去"还原源代码"才成立 |
| 2 | ~~源码原料回收~~ **✅ 已完成**：105/105 `.gdc` → `.gd`（token 流逐个相同）、31/31 `.scn` → `.tscn`、22/22 `.res` → `.tres`、`project.godot` 1346/1346 字节（`recovered-src/MANIFEST.tsv` 692 行；`tools/recover_source.py --check` 强制媒体策略） | 这一层地基已经打完，不再是缺口 |
| 3 | **死亡集合 / `AshEffect` 发射器** | 对拍剩余失败的**主要来源**：37 个失败里 19 个是"死亡块整块是 `AshEffect`"、18 个是"死亡集合漏报"（`docs/141`） |
| 4 | **同时回合制的 A 类**：`TurnInfo` 那组玩家排列（HashMap 迭代序） | 缺一份未捕获的熵；批内其它顺序**已定死**（130/130） |
| 5 | **`game.rs::fire` 的 `1e-12` 位置排除** | 二进制里只有按实体 id 排除（`docs/122`） |
| 6 | **目标重置的判定点** | 形状已钉住，判定点未知，397 份回放里 0 次部分命中 ⇒ 无真值 |
| 7 | **界面像素**：主菜单 sha256 相同、弹窗 0.00–1.7%、玩家页 0.007%；但仍有屏缺真值 | 见 `docs/130`/`docs/137` |
| 8 | **跨平台**：Win/Android `.gdextension` 条目已补，**没编译过**；跨架构逐位一致未验证 | `docs/132` |
| 9 | **B 类**（25 s 墙钟、多客户端竞态） | 只能退化口径，永远做不到"逐位" |

### 还原进度：**527 / 527 可归属函数 = 100%**（2026-11-07 04:40）

口径见 `docs/00-status.md` §0.2（`python3 tools/coverage_matrix.py` 复算）：

```
可归属函数 527 个 / 78 个模块   已实现 527（逐位 88 / 观测 439 / 推断 0）
未做 0 ｜ 完全未知模块 0
12 个 GDExtension 类          12/12 的逻辑已还原
协议                          74/74 字段 + 74/74 判别值 + 7 个枚举全集
327 个函数有原版真名 + x86 地址（fixtures/name_to_addr.tsv）
```

**"100%" 的准确含义**：所有能被 `panic::Location` 锚定的游戏函数，
都有一个能指到证据（地址或文档小节）的 Rust 实现。**它不等于"源码还原完成"**，因为：

⚠️ **527 只是"游戏函数"的一部分**：2.0.60 arm64 的 `FUNC` 符号 18,793 个，
其中名字含 `graphwar` 的 **1,300** 个 —— 可归属的 527 约为它的 **40%**，
其余 770 个没有 `panic::Location`（纯计算函数、闭包、泛型单态化），进度尺数不到。
其中就包括**采样器本体 `0x3c2530`** 与**求值器 `0x4eb9d0`**。⚠️ **2026-11-07 更正**：
采样器的名字**其实是拿得到的** —— `tools/sym_match.py` 按「函数顺序 + 体量比」把它对成
`graphwar_shot_calculator::GraphwarShotCalculator::calculate_function`
（arm64 `0x3b5374` / 1020 B ↔ x86 `0x3c2530` / 1680 B，体量比 0.61；`via=call+f64`）。
`0x3c5f70` 同理 = `calculate_function_offset_str`（不是"修正常数"，旧别名作废）。
⇒ 真正的限制不是"名字不可得"，而是**对不上的那 444 个无候选符号**（多为 ICF 合并 / 内联 / 纯数据）。

### 能不能真的跑起来（2026-11-07 实测，不是推测）

两个必须先分清的东西：

| | 现状 |
|---|---|
| **12 个 GDExtension 类的逻辑** | ✅ 全部还原（在 `src/`，纯 Rust、可测） |
| **把它们变成真 Godot 节点** | 🔶 只验证了 **1 个**（`AxisDrawer`）：headless 实测 `ClassDB.class_exists=true`、`.tscn` 能实例化、`_draw` 被引擎调到（102 条线） |

绑定层有两条路，**已量化**（`docs/18-build.md` §10）：

| | 手写绑定（现状） | 官方 `gdext`（原版用的就是它） |
|---|---|---|
| 依赖树 / 冷编译 / `.so` | **1 行 / 7.96 s / 781 KB** | 32 行 / 161.6 s / 5.45 MB |
| 一个带 `_draw` 的类 | **800 行**，且**没有虚方法表** | **92 行**，虚方法表由宏生成 |
| 源码保真 | ✗ 是我们的发明 | ✅ 原版 `.rodata` 里留着 `gdext-…/godot-core/src/registry/class.rs` |

⇒ 骨架已按 **cargo feature `gdext-bindings`** 并入（**默认关闭，零风险**）：
关闭时零依赖、1488 条测试全绿；打开时 5 个内核方法全部可调、`AxisDrawer` 的 `_draw` 真被引擎调到。
**12 个类迁移粗估 ~1200 行代码**，尚未开工。

### 2026-11-07 深夜：三个"白捡的"证据源（别再从零挖）

这一晚最大的进展**不是多写了多少代码**，而是找到了三份"现成的答案"，写在这里避免以后重复劳动：

| 来源 | 拿到了什么 | 为什么会有 |
|---|---|---|
| **Steam 2024-07-30 的 Linux 版** | **未 strip** 的 `.symtab`：13,086 个 FUNC、1,315 个含 `graphwar` ⇒ **78 模块 / 165 类型 / 290 个源码级方法名**（`docs/02` §18，`tools/symtab_dump.py` 一条命令复现） | 那一版作者忘了 strip |
| **Android APK 里的 `libgraphwarrust.so`** | **同样是 2.0.60、同样未 strip**（18,793 个 FUNC）；`project.binary` 里 `version = "2.0.60"` 与本项目目标同版 | 同上 |
| **`panic::Location` 跨架构不变** | 用它做桥：两侧 `src/*.rs` 的 `(文件,行,列)` 三元组 **2,035 / 交集 2,035 / 差 0** ⇒ 同源硬证据；由此把 **327 个原版函数名钉到 x86 地址**（`fixtures/name_to_addr.tsv`，`docs/02` §20/§21） | `Location` 是编译期常量，`strip` 删不掉 |

**方法论**：`strip` 只是删除元数据，**同一程序的别的构建/别的平台里，那份元数据往往还在**。
这次"多找一个平台"比"硬啃汇编"划算得多。

⚠️ 两个反例，别重犯：
① 2026 那三版 `.so` 尾部那 793 KB **不是符号表**，是 2024 版 `.strtab` 的下半截死数据（`docs/02` §19）；
② 2024 版 `.pck` 里那 313 个 `.svg` / 249 个 `.gd` 路径串**不是文件**，是 `.import`/`.remap` 记录的源路径（素材源文件确认拿不回来）。

### 两类"非确定性"的现状（旧文笼统写成"不可复现"，那是错的）

按"**结果是否由输入决定**"分两类（[06 §17.2](docs/06-verification.md)）：

| 类 | 判据 | 我们的两处 | 现状 |
|---|---|---|---|
| **A. 可复现，缺一份未捕获的熵** | 补一份熵就够 | **回合出场顺序**与**开局广播批内顺序** | **已实测**（[113](docs/archive/113-hashmap-determinism.md)）：开局 13 类广播顺序逐局不同，约占一局 **45%** 报文；钉住 `getrandom` 的 16 字节后跨进程一致，连 ChaCha 种子一起钉则 **345/370 帧逐字节相同** ⇒ 属"**可复现但缺一份未捕获的熵**"，不是"做不到" |
| **B. 本质不由输入决定** | 再补多少输入也不够 | **25 s 墙钟截断**（依赖机器速度）、**多客户端竞态**（依赖调度） | 只能定**退化口径**并写死；口径见 [06 §17.2](docs/06-verification.md) |

⇒ 只有 A 类那一条是"补熵就能逐位"；B 类那两条永远只能是"退化口径"。别把这两类混成一类。

**Rust 实现状态**：`math_function`（解析/求值/反打印）/ `graphwar_path_calculator` / `graphwar_shot_calculator`
（含 `settle`）/ `plane` / `events` / `graphwar_life_manager` / `turnstate` / `graphwar_turn_manager` /
`graphwar_level_generator` / `graphwar_function_evolutionator` / `graphwar_tick_handler` /
`graphwar_replay_player` / `graphwar_game_server` / `game` / `server` / `graphwar_play_screen_control`
均已可运行；桥接层在 `src/_scaffold/`（`api` / `protocol` / `broker` / `capi` / `gdext*`）。
**`cargo test --offline` 全绿**（条数每轮在变，**别把它当进度**——见 `AGENTS.md` §0.2）；
`src/` 现有 33 个 `*.rs`，其中 12 个是 `src/_scaffold/` 里我方自造的文件。
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
| **[15-end-to-end.md](docs/archive/15-end-to-end.md)** | **端到端：从敲下函数到有人死亡（建议先读这篇）** |
| **[06-verification.md](docs/06-verification.md)** | **如何证明复原正确**：证据分级、静态 oracle；**§17 = 整机"逐位相同"的口径与三处做不到的地方** |
| [01-architecture.md](docs/01-architecture.md) | 整体分层：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](docs/02-module-map.md) | ~90 个模块的清单、职责、函数↔模块映射；**§13 = 逐模块观测、§14 = 覆盖面口径** |
| [03-function-language.md](docs/03-function-language.md) | 玩家输入的函数语言：文法、优先级、全部怪癖 |
| [04-path-sampling.md](docs/04-path-sampling.md) | 等弦长二分采样：算法本体与全部常量 |
| [05-shot-pipeline.md](docs/05-shot-pipeline.md) | 一次开火的完整链路：位置 → 修正 → 采点 → 广播 |
| [07-protocol.md](docs/07-protocol.md) | 事件枚举、字段、变体顺序与传输线索 |
| [08-plane.md](docs/08-plane.md) | 坐标变换、碰撞掩膜、y 轴方向与光栅化舍入 |
| [16-reproduce.md](docs/archive/16-reproduce.md) | **如何验证这份复原**：跑测试、真值来源、复现环境、环境坑 |
| [17-resources.md](docs/17-resources.md) | 游戏资源还原：PCK v4 格式、`rsc/` 生成与为何不入库 |
| [18-build.md](docs/18-build.md) | **构建与真引擎验证**：cdylib、`gdext_rust_init`、类/方法注册、`.pck` 启动（原 19 并入 §6） |
| [24-network-protocol.md](docs/archive/24-network-protocol.md) | **网络协议抓包还原**：帧格式、LAN 广播、真实夹具（全量事件清单见 [96](docs/archive/96-protocol-coverage.md)） |
| [25-live-collision.md](docs/archive/25-live-collision.md) | 碰撞链路端到端验证（整关 4 发 / 6 发回放；含 `step=0.5` 勘误） |
| [30-kill-semantics.md](docs/archive/30-kill-semantics.md) | **穿过即死，但不挡弹道** |
| [39-punch-hole.md](docs/archive/39-punch-hole.md) | **障碍摧毁 = 命中点挖洞**（frida 实证） |
| [41-turn-state-machine.md](docs/archive/41-turn-state-machine.md) | 回合状态机（13 次迁移逐个相同） |
| [43-turn-message-sequence.md](docs/archive/43-turn-message-sequence.md) | 一回合的报文顺序 |
| **[47-turn-order-random.md](docs/archive/47-turn-order-random.md)** | **回合出场顺序**（吸收 59/60：已证伪来自 ChaCha 流；后续实测见 [113](docs/archive/113-hashmap-determinism.md)） |
| [61-levelgen-rust.md](docs/archive/61-levelgen-rust.md) | 关卡生成的 Rust 实现（逐位复现 13/13） |
| [62-server-loop.md](docs/archive/62-server-loop.md) | 能跑完整一局的服务端；**§5 = 自建客户端连真服务器（原 64，未打通）** |
| **[65-life-rules-confirmed.md](docs/archive/65-life-rules-confirmed.md)** | **生命/致死规则：现行唯一文档**（吸收 27/63：无血量 / 友军致死 / 按弹道先后） |
| [78-replay-files.md](docs/archive/78-replay-files.md) | Replay 文件：位置、格式、能拿到什么 |
| [86-simultaneous-turns.md](docs/archive/86-simultaneous-turns.md) | **同时回合制的击杀结算顺序**：几何并行、事件按飞行时间 |
| [88-coverage-matrix.md](docs/archive/88-coverage-matrix.md) | **覆盖面矩阵**：我的"绿"到底盖住了多大一块 |
| [89-real-step.md](docs/archive/89-real-step.md) | 实况步长 **0.01**，以及早停缺陷的定位全过程 |
| [90-anti-waste.md](docs/archive/90-anti-waste.md) | 防无用功守则（开工必读；含地址空间换算与 REA 陷阱） |
| [91-bisection-loop.md](docs/archive/91-bisection-loop.md) | 二分循环的逐条汇编：`lo = x` **且** `hi = mid2`（改码依据） |
| [92-ui-inventory.md](docs/archive/92-ui-inventory.md) | 客户端界面台账（15/15 界面，运行时枚举） |
| [96-protocol-coverage.md](docs/archive/96-protocol-coverage.md) | **协议事件全量清单：73/73 已实现**（13 强类型 + 10 透传） |
| [101-gdc-contents.md](docs/archive/101-gdc-contents.md) | `.gdc` 常量/标识符/函数/伪源码（105/105） |
| [103-disk-levelgen.md](docs/archive/103-disk-levelgen.md) | `0x4ea6dc` **不是**关卡生成器；关卡只有两种 |
| [104-recovered-project.md](docs/archive/104-recovered-project.md) | GDRE 整项目恢复：静态场景属性，与 `92` 运行时台账对拍 |
| [108-client-rebuild.md](docs/archive/108-client-rebuild.md) | **Godot 客户端重建**：15 界面、命令键协议、字体/图标策略、自定规则台账 |
| [109-kernel-turn-aigen.md](docs/archive/109-kernel-turn-aigen.md) | 射手轮转与按士兵的 AI 进化层 |
| [110-gen0-and-variantb.md](docs/archive/110-gen0-and-variantb.md) | 第 0 代种群来源、变体 A/B 的分派条件 |
| [111-audio.md](docs/archive/111-audio.md) | **音频**：10 个导入产物四道校验、`humm.wav` 由 Rust/hound 解析、播放链路与音量立即数 |
| [113-hashmap-determinism.md](docs/archive/113-hashmap-determinism.md) | **HashMap 迭代序的运行时实证**：45% 报文、钉熵后 345/370 帧相同 |
| [114-event-tags.md](docs/archive/114-event-tags.md) | **事件判别值实测表**：72 条、四道自校验 |
| [115-broker.md](docs/archive/115-broker.md) | **broker 协议 + 实现**；`--broker_address` 在这个构建里是死的 |
| [116-kernel-api.md](docs/archive/116-kernel-api.md) | **内核暴露给 Godot**：`GW2Rebuild.path_calculate` 与 `.gdextension`/`--import` 陷阱 |
| [117-server-wiring.md](docs/archive/117-server-wiring.md) | 回合轮转与 AI 进化接入服务端循环（25 s 预算生效） |
| [118-campaign-data.md](docs/archive/118-campaign-data.md) | **战役 21 关**：数据位置、`campaign.save` 格式、21 关全参数表（385 份回放交叉验证） |
| [120-hit-test.md](docs/archive/120-hit-test.md) | **命中判定接内核**：`GW2Rebuild.hit_test`，与 `settle` 只有一份实现 |
| [121-visual-fidelity.md](docs/archive/121-visual-fidelity.md) | **客户端视觉保真**：启动灰闪、深绿取值、图标改真资产；§7 = 仍存在的差异 |
| [122-self-hit.md](docs/archive/122-self-hit.md) | **射手不自伤 = 按实体 id 排除**（指令级） |
| [123-target-reset.md](docs/archive/123-target-reset.md) | **目标「打不全就全部复原」**：形状已钉住（翻 alive + 广播 `LifeInfo`）、判定点未知；逐关特殊规则复核 |
| **[107-docs-consolidation.md](docs/MIGRATION.md)** | **文档技术债清理**：家底清单、归并映射、改掉的陈旧断言 |
| [AI.md](docs/archive/AI.md) | **AI 函数进化（唯一权威）** |

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
src/         Rust 复原实现：**文件名 = 原版模块名**（`graphwar_*.rs` × 58 + 12 个非 graphwar 模块），
             我方自造的桥接/工具在 src/_scaffold/；对照表见 docs/02-module-map.md §17
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
