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

| 模块 | 状态 | 证据等级 |
|---|---|---|
| 函数文法解析器 `graphwar_function_input.rs` | ✅ 复原完成 | **逐位验证**（真二进制对拍） |
| 求值器 / 常量折叠 | ✅ 复原完成 | 逐位验证 |
| 点序列采样 `graphwar_path_calculator.rs` | ✅ 复原完成 | 逐位验证（2552 条曲线） |
| 开火结算 `graphwar_shot_calculator.rs` | 🔶 调用链已还原，内部细节待补 | 观测（调用点） |
| 棋盘 / 坐标 `graphwar_plane*.rs` | 🔶 核心变换已解出（20 px/单位，画布 1000×600）；命中判定未证 | 观测 |
| 回合 `graphwar_turn_manager.rs` | 🔶 结构已探明（14 函数/22 调用者/无时间常量），语义未还原 | 观测 |
| 命中与生命 `life/soldier/team_manager` | 🔶 结构已还原（RwLock + 两张表），伤害规则未知 | 观测 |
| 关卡生成 `graphwar_level_generator.rs` | ⬜ 待逆向 | — |
| AI `computer_player_manager` / `function_evolutionator` | 🔶 已知它经指针间接调用开火结算（疑似批量试射评分） | 观测+推断 |
| 联机 `game_server/client`、`lan_broadcaster_v6`、`broker` | ⬜ 协议形状已还原（见 docs/07），实现未逆向 | 观测（字符串） |

模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)，共约 90 个 `graphwar_*.rs`。

**Rust 实现状态**：`parse` / `eval` / `calculate` / `shot` / `plane`（坐标变换 + 掩膜）/ `events`（枚举形状）/ `life`（两张表）已可运行，
由 `cargo test` 对 331 条真值用例验收（其中 294 条直接来自真二进制），
外加常量锚定与形状断言。夹具的生成方式见
[`docs/06-verification.md §9`](docs/06-verification.md)。

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
| [18-build.md](docs/18-build.md) | **构建出真实二进制**：cdylib、gdext_rust_init、dlopen 验证 |

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
