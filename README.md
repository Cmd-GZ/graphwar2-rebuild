# gw2-rebuild —— Graphwar II 源码复原

本仓库目标：基于 `libgraphwarrust.so` 的静态逆向结果，**复原 Graphwar II 的实现并重写成可编译的源码**，
同时把"游戏到底怎么运作"讲清楚。

与上游 `graphwar-algorithm-analysis` 的关系：那是**逆向报告与对拍工具链**（`REPORT.md` +
`analysis/` + `graphwar_curve.py`）；本仓库是**实现本体**，独立托管在
`Cmd-GZ/graphwar2`，与上游仓库分离、互不包含。

## 目标二进制

| 文件 | 角色 | 规模 |
|---|---|---|
| `libgraphwarrust.so` | GDExtension（Rust + godot-rust），**全部玩法逻辑** | 10 MB，stripped，`.eh_frame` 恢复出 13430 个函数 |
| `Graphwar.x86_64` | Godot 4.7 导出模板 | 73 MB |
| `Graphwar.pck` | Godot 资源包 | 105 个 `.gdc`（仅 UI）+ 31 个场景 |

关键约定：**Ghidra 镜像基址 `0x100000`**，文件地址 `0x3c2530` 在 Ghidra/REA 里是 `0x4c2530`。

## 进度

判定标准：**✅ = 有真值对拍或真引擎实测**；🔶 = 只到结构/调用关系；⬜ = 未逆向。

| 模块 | 状态 | 验证手段 |
|---|---|---|
| 函数文法解析器 | ✅ | 真二进制对拍（90 例 + 37 夹具） |
| 求值器 / 常量折叠 | ✅ | 真二进制对拍（70 夹具） |
| 网络协议（帧+消息目录） | ✅ | 真实抓包 **46 种消息**全部解析（`fixtures/protocol*.jsonl`） |
| **网络编解码实现** | ✅ | `Broadcast::to_json` 与 4 类消息序列化与抓包**逐字节一致**（含浮点格式，`docs/32`） |
| 点序列采样 | ✅ | 真二进制对拍（夹具 + **真实对局抓获的轨迹 69/69 逐点一致**，含 step=0.5/flip=True） |
| 坐标变换 / 掩膜 / 光栅化 | ✅ | 与 `0x4be3e0`、`0x4f11e0`、`0x4f15b0` **逐字节/随机点一致** |
| 构建 / 加载 / 类注册 / 方法调用 | ✅ | 真 Godot 4.7.2：类可实例化，`ping()` 回调执行 |
| 资源还原 | ✅ | 692 文件 md5 全符（`tools/pck_extract.py`） |
| 打包运行（代码+资源） | ✅ | 导出 `.pck` 并从包启动：扩展注册成功、还原的纹理加载成功（`docs/19` §7） |
| **弹道与命中结算**（`src/settle.rs::resolve`） | ✅ | 可运行实现：障碍决定停点、士兵经过即死。单发误差 < 采样步；**新建模：摧毁=命中点挖 `r=0.5` 洞**（frida 实证，`docs/39`） |
| 开火结算 `shot_calculator`（事件生成） | 🔶 | **整关 4 发回放 + 2 次致死验证通过**（直线+曲线，误差 0.228/0.460/0.433/0.393，均 < 采样步；修正文本逐字一致，`docs/25` §7）；`0x3c2bc0` 内部未逐条还原 |
| 命中与生命 `life_manager` | 🔶 | 死亡链路 + **致死判定已定量验证**（穿过即死、不挡弹道，2 例距离 0.4746/0.4944，`docs/30`）；血量/友军规则未知 |
| **回合状态机**（`src/turnstate.rs`） | ✅ | 6 状态 + 7 事件，**13 次迁移与抓包逐个相同**（`docs/41`） |
| 回合调度 `turn_manager` | 🔶 | 出场顺序只验到「队伍严格交替」（`docs/28`）；**状态时间驱动未还原**（`end_of_turn_tick` 与 60Hz 时钟对不上） |
| 关卡生成 `level_generator` | ⬜ | **随机源是 OS 熵**（`getrandom` 921 次/40s，`rand/srand` 全 0）⇒ 同局**不可复现**；只观测到分布（障碍 5~21 个、`r∈[0.8,5.26]`、位置可越界，`docs/40`） |
| AI `computer_player_manager` / `function_evolutionator` | 🔶 | 入口 `0x51b440`（每回合 1 次）+ 评分循环 `0x51a750`（每回合约 3800 次、step=0.5）已定量刻画；**候选生成/适应度/RNG 均未知**（`docs/29`） |
| **tick 时钟（60 Hz）** | ✅ | `tick_at()` 用 113 个真实样本验证；`TickReply` 逐字节一致（`docs/35`）。**另解出** `end_of_turn_tick = 回合开始 tick + 时长`，**六档全部实测**（含 `Timer30s`、`TimerInf=0` 哨兵，`docs/42`） |
| **服务端握手（状态同步）** | ✅ | `server::on_new_connection` 7 条消息与抓包逐字节一致；`serve_handshake` 经真实 TCP 往返验证（`docs/33`） |
| **服务端对象 `Match`（开火结算+出报文）** | ✅ | `fire()` 按抓包顺序产出 8 类报文（`docs/43`），状态推进位置与真实一致；按回合配对回放 4 发终点全对、致死集合一致（`docs/37`） |
| **一整回合报文序列** | ✅ | `begin_turn + fire + end_turn` 产出 **12 条**报文，种类顺序与抓包一致（`docs/46`） |
| **回合计数与结束判定** | ✅ | `TurnCountInfo` 每回合 +1；`EndingGame` 在某队全灭时紧跟其后（真实序列佐证，`docs/45`） |
| **障碍延迟删除** | ✅ | `DestroyObstacle` 实测延迟 **165 tick**（163.9/164.0/167.0 三样本），`Match::tick()` 到期发出、不重复（`docs/44`） |
| 联机实现（玩家/tick/结算广播） | 🔶 | 传输层与 46 种消息已还原；**颜色分配规则未定**（10 个观测值、非纯随机也非按 id 绑定，`docs/34`）、tick 节拍与结算广播未实现 |

### 离"全绿"还差什么

四行 🔶/⬜ 的共同问题是**没有真值来源**：

- 用真引擎把**原始扩展**装进去后确认：它只向 GDScript 暴露 **12 个类、25 个方法**，
  且几乎全是 UI 回调（`on_text_change` / `on_resized` / `pressed`…）。
  玩法逻辑（回合、关卡、AI、结算）**完全没有开放**。详见 `docs/19` §5。
- 所以这几块只能靠"反编译 + 构造合成状态做 oracle"，像掩膜那样。
  回合模块的入口是迭代器 + 闭包（`Arc::clone` + `for_each`），
  要造出可信的合成状态成本远高于掩膜。

**结论：目前没有任何一行是因为"懒得做"而空着，都是缺可验证的真值。**
要继续推进，要么接受"只到结构、标注推断"（现状），要么为每个模块单独造 oracle。

模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)，共约 90 个 `graphwar_*.rs`。

**Rust 实现状态**：`parse` / `eval` / `calculate` / `shot` / `plane` / `events` / `life` / `gdext` 已可运行，
`cargo test` **40 条**通过；另有 5 个对拍脚本与真二进制比较。

## 文档

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
| [12-ai.md](docs/12-ai.md) | AI：结构已探明，评分规则未还原 |
| [13-turns.md](docs/13-turns.md) | 回合：规模与驱动者、无时间常量的负结果 |
| [14-target.md](docs/14-target.md) | 目标选择：核心十字路口，SIMD 未解 |
| [16-reproduce.md](docs/16-reproduce.md) | **如何验证这份复原**：跑测试、真值来源、复现环境、环境坑 |
| [17-resources.md](docs/17-resources.md) | 游戏资源还原：PCK v4 格式、rsc/ 生成与为何不入库 |
| [18-build.md](docs/18-build.md) | 构建出真实二进制：cdylib、gdext_rust_init、dlopen 验证 |
| [19-godot-verify.md](docs/19-godot-verify.md) | 在真 Godot 4.7.2 里加载注册类；原扩展 API 面只有 12 类 |
| [20-live-oracle.md](docs/20-live-oracle.md) | 活体 oracle：frida 注入真游戏进程，调用内部函数 |
| [21-oracle-limits.md](docs/21-oracle-limits.md) | **玩法 oracle 为何不可达**：四条尝试与证据 |

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
每个都有对应的对拍脚本（见 `docs/16`）。

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
