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
| 棋盘 / 坐标 `graphwar_plane*.rs` | ⬜ 待逆向 | — |
| 回合 `graphwar_turn_manager.rs` | ⬜ 待逆向 | — |
| 命中与生命 `life/soldier/team_manager` | ⬜ 待逆向 | — |
| 关卡生成 `graphwar_level_generator.rs` | ⬜ 待逆向 | — |
| AI `computer_player_manager` / `function_evolutionator` | ⬜ 待逆向 | — |
| 联机 `game_server/client`、`lan_broadcaster_v6`、`broker` | ⬜ 待逆向 | — |

模块全集与行号下界见 [`docs/02-module-map.md`](docs/02-module-map.md)，共约 90 个 `graphwar_*.rs`。

## 文档

| 文档 | 内容 |
|---|---|
| [01-architecture.md](docs/01-architecture.md) | 整体分层：Godot 外壳 / GDExtension 内核 / 资源包 |
| [02-module-map.md](docs/02-module-map.md) | ~90 个模块的清单、职责、已知行号与地址锚点 |
| [03-function-language.md](docs/03-function-language.md) | 玩家输入的函数语言：文法、优先级、全部怪癖 |
| [04-path-sampling.md](docs/04-path-sampling.md) | 等弦长二分采样：算法本体与全部常量 |
| [05-shot-pipeline.md](docs/05-shot-pipeline.md) | 一次开火的完整链路：位置 → 修正 → 采点 → 广播 |
| [06-verification.md](docs/06-verification.md) | 如何证明复原正确；证据分级与已知边界 |

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
