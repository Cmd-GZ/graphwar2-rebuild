# 00 · 当前状态总览

> 本页是**唯一权威**的状态页。历史与已推翻的结论在 `archive/`，只作过程档案。

## 0. 诚实声明（先读这条）

1. **本目录只放长期有效的知识**，不放"本轮做了什么"。提交历史用 `git log` 看；
   `docs/archive/` 里是已被吸收的旧文，只为留证据轨迹与旧引用不失效。
2. **四档证据标记**（[`06-verification.md`](06-verification.md) §1）：
   **逐位验证**（与真二进制输出完全相等）/ **观测**（读到字节、伪代码、真帧）/ **推断**（写明缺什么）/
   **未知**（没有证据）。任何一条结论都必须能指到其中一档；合并文档时**标记原样搬运**。
3. **`client/` 是仿制品**：它照恢复出的 `.tscn` 与真机真帧重写，**不是原版源码**——
   原版的界面控制器是 Rust 模块，在 `.so` 里。所以"看起来对"**不是**复原证据。
4. **客户端自检条数是回归保护，不是复原证据。** 整局逐位的实测是 **71/400 帧完全一致**
   （[`06-verification.md`](06-verification.md) §21）。

## 1. 一句话状态

## 0.2 进度尺（2026-11-07 深夜，**这才是"还原了多少"**）

> 口径：**原版可归属函数里，我们还原并验证了多少**。不用测试条数当进度（见 `AGENTS.md` §0.2）。
> 复现：`python3 tools/coverage_matrix.py`（`--check` 只校验不重写）。

```
原版可归属函数 527 个，分布在 78 个模块（58 个 graphwar_*.rs 占 431、15 个非 graphwar 占 96）
   已实现 281（逐位 98 / 观测 183 / 推断 0）｜ 未做 161 ｜ 完全未知模块 22（graphwar 里 15）
归属率 527 / 13263 个 Ghidra 函数 = 4.0%（硬上限：纯计算函数没有 panic 就没有锚）
```

⚠️ **527 是可度量下限，不是模块规模**——求值器 `0x4eb9d0`、采样器 `0x3c2530` 全函数体零 Location 引用。

### 0.2.1 分母在 2026-11-07 深夜被**改大**过（不是进度倒退）

旧的 `488 / 70 个模块` 是错的，两个原因，都来自 2024 版未 strip 的符号表（`docs/02` §18）：

1. **漏记 7 个 crate 内模块**：`backtracer` / `function_token` / `graphwar_event_queue` /
   `graphwar_game_info` / `graphwar_player_config`(8 函数) / `graphwar_replay_reader` / `graphwar_stream_buffer`；
2. **`weak_caller` 分类错误**：旧文档判它是"第三方依赖、不计入总账"，
   但符号表里有 `graphwarrust::weak_caller` 的 27 个 direct 符号 ⇒ 它是 **crate 内模块**，
   那 39 个可归属函数应当计入 ⇒ 分母 +39。

⇒ **`281/527 = 53.3%`** 是诚实数字；之前说的 `281/488 = 57.6%` 偏高。

## 0.3 本轮（07:00 前后）新增

| 模块 | 产出 | 档位 |
|---|---|---|
| `entity_drawer.rs` | `src/entity_drawer.rs`（**只覆盖 `draw` 1/14 个归属函数**）：`EntityDrawer::draw` 只画两类图元（`draw_arc`×3 + 多边形×3），**不画士兵贴图/弹道**；弧半径 = `f32(radius)×1.5`、箭头 = 向心三角（∓12°、+0.5） | 观测（常量逐条带槽地址） |
| `play_screen_control` | `src/graphwar_play_screen_control.rs`：回放/对局视图是一对互补的孪生函数；只订阅 `function_info_event` + `function_fire_event` ⇒ **`TurnInfo`/`GameStateInfo` 不经这层**；**弹道清除不在这层**（否定证据） | 观测 |
| 15 个零锚点模块 | `docs/02` §16：定性 10 个（观测 3 / 推断 7），剩 5 个纯未知。**全库只有 12 个 GDExtension 类** ⇒ 那 15 个里只有 3 个是 Godot 类，其余是被节点调用的普通 Rust 模块 | 见 §16 |

## 0.4 下一轮最高性价比（按"能不能指到证据"排）

1. **`ptr_slots.py` 解 `0x3d5260` 的名字表** ⇒ 一次性定死"弹道什么时候清除"的落点（`play_control` 已排除本层）；
2. **`0x3dcbb0`（0x2250 字节）**：`play_screen_control` 内最大函数，被 `root_node 0x3a5760` 直接调用，很可能是"回合推进 / game_state 分派"的真身；
3. **`entity_drawer` 补暴露**：`src/_scaffold/api.rs`/`_scaffold/gdext.rs` 还没把新图元暴露给客户端（本轮禁止改 `client/`），
   所以棋盘仍用旧近似画法；
4. 5 个纯未知模块（effect_manager / entity_effect_manager / game_recorder / graphics_manager / video_writer）。

## 0.5 源码目录约定（2026-11 起，**看代码前先读这条**）

`src/` 的文件名现在**就是原版模块名**，所以读代码不用再猜对应关系：

* `src/graphwar_*.rs` = 58 个原版 `graphwar_*.rs` 之一（如 `graphwar_turn_manager.rs`）；
  一个原版文件被我方拆成两份时，主文件用原版名、第二份进同名目录
  （`graphwar_shot_calculator/settle.rs`、`graphwar_level_generator/tables.rs`、
  `math_function/to_text.rs`）；
* `src/entity_drawer.rs` = 12 个非 `graphwar_` 游戏模块之一；
* `src/events.rs` / `game.rs` / `plane.rs` / `server.rs` / `turnstate.rs` = **混装**了多个原版模块、
  边界尚未证实，按 `AGENTS.md` 铁律 2 原地保留并登记在待拆表里；
* `src/_scaffold/` = **我方自造**的桥接/工具（GDExtension 入口、C ABI、报文装配、对拍器、
  ChaCha12、gzip、broker 服务端）——原版没有这些东西，目录里有 `README.md` 逐条说明。

完整对照表（78 个模块逐行 + 状态 + 依据）见 [`02-module-map.md`](02-module-map.md) §17；
**模块的权威清单（78 模块 / 165 类型 / 290 方法名）**见同文件 §18；
**327 个函数的原版真名 ↔ x86 地址**见 `fixtures/name_to_addr.tsv`（§20）。


内核（函数语言 / 弹道 / 掩膜 / 命中 / 协议 / broker / 回合与生命 / 服务端 / 关卡 / 战役数据 / AI 参数）
已复原到"逐位验证"或"强观测"；`client/` 里 **15/15 界面**能启动、能开一局，弹道与命中走内核，
并且**回放播放器、联机层、音频播放层、`token → 文本` 都已经接上了**。
仍做不到的是 [`06-verification.md`](06-verification.md) §17 那几项**被证明做不到**的逐位一致
（两类非确定性、墙钟、渲染管线）。

## 2. 已完成（可执行 + 有验收）

| 部件 | 代码 | 验收 | 文档 |
|---|---|---|---|
| 函数文法 / 求值 / token→文本 | `src/math_function.rs`（+ `src/math_function/to_text.rs`） | 90 例文法 + 37/70 条真值夹具；`to_text` 1032/1032 | [`03`](03-function-language.md) |
| 等弦长采点（含 RK4） | `src/graphwar_path_calculator.rs` | 224 条真值夹具 + **oracle 24/24 逐点逐位相等** | [`04`](04-path-sampling.md) §9 |
| 修正常数 + 开火原语 | `src/graphwar_shot_calculator.rs` | 端到端冒烟 + 上游 648 例对拍 + 装配校验 15 组 | [`05`](05-shot-pipeline.md) |
| 坐标 / 掩膜 / 光栅化 | `src/plane.rs` | 与真二进制**逐字节 / 300 随机点**一致；摧毁 = 挖 `r=0.5` 洞 | [`08`](08-plane.md) §10 |
| 弹道与命中结算 | `src/graphwar_shot_calculator/settle.rs` | 整关回放致死集合逐发一致；实况 0.01 下 82 发并集 **152/152** | [`14`](14-turn.md) §6 |
| 命中判定暴露给引擎 | `src/_scaffold/api.rs` | `hit_test` 与 `resolve` **同一个函数体**（无第二份实现） | [`16`](06-verification.md) §4 |
| 事件 / 生命 / 回合状态机 | `src/events.rs`、`src/graphwar_life_manager.rs`、`src/turnstate.rs` | 6 状态 7 事件，**13 次迁移逐个相同** | [`14`](14-turn.md) §2 |
| 协议 + 编解码 | `src/_scaffold/protocol.rs` | **73/73 事件**；判别值 **72 个实测值** | [`07`](07-protocol.md) §6–§7 |
| broker（房间列表） | `src/_scaffold/broker.rs` | 12 条测试；`VersionInfo` → `GameInfo` 线格式与真实抓包一致 | [`09`](09-network.md) §6 |
| 服务端（握手 / Match / 一回合 / 整局循环） | `src/server.rs`、`src/game.rs`、`src/graphwar_game_server.rs` | 与抓包逐字节/逐条一致；真实 TCP 跑到 `EndingGame` | [`09`](09-network.md) §3–§5 |
| tick 时钟 | `src/graphwar_tick_handler.rs` | 113 个真实样本；`TickReply` 逐字节一致 | [`14`](14-turn.md) §1 |
| 关卡生成 | `src/graphwar_level_generator.rs` | 散布模式**逐位复现 13/13**；中心模式 4 个真值样本 | [`11`](11-levelgen.md) |
| 战役 21 关数据 | 数据在二进制；全表见 [`10`](10-campaign.md) §4 | 21/21 标题、21/21 描述、每关参数；**385 份回放交叉验证 21/21** | [`10`](10-campaign.md) |
| AI（进化） | `src/graphwar_function_evolutionator.rs` | 参数/权重/长度/装配/变异/每代构成全部还原；终止 = 代数 ∨ **25 s 墙钟** | [`13`](13-ai.md) |
| GDExtension 入口与构建 | `src/_scaffold/gdext.rs` | 真 Godot 4.7.2 里注册成功；`.pck` 能从包启动 | [`18`](18-build.md) |
| 客户端 15 界面 + 内核接缝 | `client/` | `bash client/run.sh selftest`；弹道/命中/障碍都走内核 | [`16`](06-verification.md) |

## 3. 明确未知 / 已证不可复现

| 缺口 | 状态 | 文档 |
|---|---|---|
| 回合出场顺序 / 开局那批广播的顺序 | **A 类**：来自 `HashMap` 迭代序（`getrandom(16)` 播种）。钉住熵后跨进程一致，连 ChaCha 种子一起钉则 **345/370 帧逐字节相同** | [`12`](13-ai.md) §7 |
| 25 s 墙钟截断 / 多客户端竞态 | **B 类**：本质不由输入决定，只能定退化口径 | [`06`](06-verification.md) §17 |
| 目标「打不全就全部复原」的**判定点** | **未知**：形状已钉住（只能翻 alive + 广播 `LifeInfo`；不重建实体、不恢复障碍洞），但哪一刻、什么条件没找到；397 份回放里 **0 次部分命中** | [`15`](05-shot-pipeline.md) §4 |
| 音效 ↔ 事件的映射 | **未知**：播放链路、音量、静音都逐位；"哪个事件放哪个音"证不到 | [`17`](17-resources.md) §5 |
| 通视检查 `0x567530` | 语义定为"关卡用的通视检查"，但没有调用点证明对局里用到 | [`02`](02-module-map.md) |
| 掩膜的持有者与生命周期 | 未知（带锁结构，不知谁在何时重建） | [`08`](08-plane.md) §7 |
| 段位怎么算 | `rank_manager` 里算不出来（无除法/取模/阈值表）；`RankInfo` 字段数 2 vs 3 **未收敛** | [`02`](02-module-map.md) §15 |
| 弹道用什么原语画 | `draw_polyline` 字符串在 `.so` 里根本不存在 | [`16`](06-verification.md) §12 |
| 跨架构逐位一致（Windows/Android 内核） | **未验证**，不承诺 | [`19`](19-android.md) §6 |

## 4. 交付侧：离"可玩 / 可验收"还差什么

| # | 项 | 现状 |
|---|---|---|
| 1 | 回放播放器 | ✅ 已实现（按 tick 推进 + 内核重算弹道）；**播放控件是自定的** | [`16`](06-verification.md) §10 |
| 2 | 联机层 | ✅ 客户端能真的连服务器；**用的是我们自己的服务端**，界面层仍有本地分支 | [`16`](06-verification.md) §11 |
| 3 | 音频播放 | ✅ 播放层通了（含 `humm` 的 generator 路径）；**事件映射仍是未知** | [`17`](17-resources.md) §4 |
| 4 | AI 产物上线 | ✅ `token → 文本`（`0x4ec160`，1032/1032）已接进服务端 | [`13`](13-ai.md) §22 |
| 5 | 战役数据 | ✅ 21 关真数据已接进界面 | [`10`](10-campaign.md) §11 |
| 6 | 界面保真 | 逐屏残差见 [`16`](06-verification.md) §6（多数 < 0.5%，`EndGamePopUp` 仍 ~9.8%） |
| 7 | 目标「打不全就复原」 | 实现了，但标 **推断**（判定点未知） | [`10`](10-campaign.md) §9 |
| 8 | 两类非确定性 | A 类缺一份未捕获的熵、B 类只能退化口径 | [`06`](06-verification.md) §17 |

## 5. 文档地图（读这一屏就够）

| 想知道什么 | 看哪里 |
|---|---|
| 全部文档的索引 | [`INDEX.md`](INDEX.md) |
| 压缩上下文后怎么接着干 | [`HANDOVER.md`](HANDOVER.md) |
| 怎么证明复原是对的 / 四档定义 / 哪些做不到 | [`06-verification.md`](06-verification.md) |
| 一次射击从输入到死亡 | [`05`](05-shot-pipeline.md) §7 |
| 模块 ↔ 函数对照 | [`02-module-map.md`](02-module-map.md) |
| 文档怎么重组的（迁移表） | [`MIGRATION.md`](MIGRATION.md) |

## 6. 已知的方法学陷阱（都踩过）

1. 反编译里的 `_DAT_xxxx` 是 **Ghidra 地址**，读原始字节要减 `0x100000`。
2. 把**虚地址**当 Ghidra 地址传给工具会得到 `not_found`；REA 甚至会把非入口地址当**文件偏移**返回一个无关函数（[`03`](03-function-language.md) §12 有量化版）。
3. `decompile_all` 的文件头含 `Ghidra 12.1.4`，直接 grep 浮点常量会被 `12.1` 污染。
4. 用途要靠**调用关系**定，不能凭函数内部特征猜。
5. 上游 `funcstarts.pkl` 是坏的（见 [`02`](02-module-map.md) §12），用 `funcstarts_ghidra_va.pkl`。
6. **调用图会骗人**：Rust 大量用函数指针派发，直接调用为零**不代表**没被使用（[`06`](06-verification.md) §12）。
7. **先查权威常量表再写代码**：`step` 曾被硬编码成 `0.5`，4 轮分析作废（[`06`](06-verification.md) §22）。
8. **Godot 的 `.gdextension` 与新增脚本要 `--import` 一次**，否则"代码写对了但没加载"（[`16`](06-verification.md) §3）。
9. **口述要先落到格式判据上**：音频那次"包里有文件损坏"的真凶是使用者自己重封装的文件（[`17`](17-resources.md) §3）。
10. **hook libc 只能看到走 libc 的调用**：Rust 的 `getrandom` crate 走 syscall 318（[`12`](13-ai.md) §9）。
