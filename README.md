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
| 点序列采样 | ✅ | 真二进制对拍（224 夹具 + 上游 2552 例） |
| 坐标变换 / 掩膜 / 光栅化 | ✅ | 与 `0x4be3e0`、`0x4f11e0`、`0x4f15b0` **逐字节/随机点一致** |
| 构建 / 加载 / 类注册 | ✅ | 真 Godot 4.7.2 加载 + `ClassDB.instantiate` 成功 |
| 资源还原 | ✅ | 692 文件 md5 全符（`tools/pck_extract.py`） |
| 开火结算 `shot_calculator` | 🔶 | 调用链已还原；`0x3c2bc0`（12 KB）未逐条还原 |
| 命中与生命 `life_manager` | 🔶 | 结构（RwLock + 两张表）；伤害规则未知 |
| 回合 `turn_manager` | 🔶 | 14 函数/22 调用者已定位；语义（迭代器 + 闭包）未还原 |
| 关卡生成 `level_generator` | ⬜ | 仅知 8 个函数地址 |
| AI `computer_player_manager` / `function_evolutionator` | 🔶 | 经指针调用开火结算；评分规则未知 |
| 联机实现 | ⬜ | 协议**形状**已还原（`docs/07`），收发逻辑未逆向 |

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
| [19-godot-verify.md](docs/19-godot-verify.md) | **在真 Godot 4.7.2 里加载并注册类**；原扩展 API 面只有 12 类 |

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
