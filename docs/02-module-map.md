# 02 · 模块地图

来源：`.so` 里 Rust panic 保留的 `src/<file>.rs:<line>` 字符串。
共约 **90 个 `graphwar_*.rs`** 模块。下表"职责"列除标注【已验证】者外均为
**按模块名与行号规模推断**，不构成行为结论。

## 1. 核心算法（已验证）

| 模块 | 行号下界 | 已知地址 | 职责 |
|---|---|---|---|
| `graphwar_function_input.rs` | ≥519 | `0x4ed670` 解析 | 文本 → 前缀 token【已验证】 |
| （求值器，同源） | — | `0x4eb9d0` | token → f64【已验证】 |
| （常量折叠） | — | `0x4eb830` | 纯常量子树折叠【已验证】 |
| `graphwar_path_calculator.rs` | ≥86 | `0x3c2530` | 点序列采样【已验证】 |
| `graphwar_shot_calculator.rs` | — | `0x3c2bc0` | 开火结算【调用链已验证】 |
| （修正常数） | — | `0x3c5f70` / `0x3c5c50` | `c = y0 − f(±x0)`，拼 `"f(x)+c"` 串 |

## 2. 场景与坐标

| 模块 | 职责（推断） |
|---|---|
| `graphwar_root_node.rs` (≥1302) | 主场景节点，游戏总控，规模最大 |
| `graphwar_plane.rs` | 坐标系、绘制 |
| `graphwar_plane_pos_manager.rs` | 坐标↔位置换算 |
| `graphwar_draw_util.rs` | 绘制工具 |
| `entity_drawer.rs` / `effect_drawer.rs` | 实体/特效绘制 |
| `graphwar_graphics_manager.rs` | 图形资源管理 |
| `graphwar_texture_loader.rs` | 贴图加载 |

## 3. 对战流程

| 模块 | 职责（推断） |
|---|---|
| `graphwar_turn_manager.rs` | 回合推进、当前玩家 |
| `graphwar_game_state_manager.rs` | 一局的状态机 |
| `graphwar_mode_manager.rs` | 游戏模式（经典/ODE/…） |
| `graphwar_team_manager.rs` | 队伍 |
| `graphwar_player_manager.rs` / `graphwar_local_player_manager.rs` | 玩家 |
| `graphwar_soldier_manager.rs` | 士兵实体 |
| `graphwar_life_manager.rs` | 生命/死亡 |
| `graphwar_target_manager.rs` | 目标选择 |
| `graphwar_obstacle_manager.rs` | 障碍物 |
| `graphwar_level_manager.rs` / `graphwar_level_generator.rs` | 地图生成 |
| `graphwar_function_manager.rs` | 每个玩家的函数存储（原串/修正串） |

## 4. AI

| 模块 | 职责（推断） |
|---|---|
| `graphwar_computer_player_manager.rs` | 电脑玩家调度 |
| `graphwar_function_evolutionator.rs` | **函数进化**：疑似对候选表达式做变异/选择，用于 AI 找射击函数 —— 优先逆向目标 |

## 5. 联机

| 模块 | 职责（推断） |
|---|---|
| `graphwar_event.rs` (≥1154) | 事件枚举（最大规模的协议定义） |
| `graphwar_remote_event_handler.rs` (≥465) | 远端事件分发 |
| `graphwar_event_handler.rs` | 本地事件处理 |
| `graphwar_game_server.rs` (≥336) | 房间内权威服务端 |
| `graphwar_game_client.rs` | 客户端 |
| `graphwar_server_broker_handler.rs` (≥94) | 房间撮合 |
| `graphwar_server_connection_handler.rs` (≥196) | 连接管理 |
| `graphwar_client_connection_handler.rs` | 客户端连接 |
| `graphwar_lan_broadcaster_v6.rs` (≥126) | 局域网广播（v6） |
| `graphwar_lan_vbox.rs` | 局域网列表 UI |
| `graphwar_backend.rs` (≥142) | 后端接入 |

## 6. 界面与外围

| 模块 | 职责（推断） |
|---|---|
| `graphwar_play_screen_control.rs` (≥561) | 对战界面逻辑 |
| `graphwar_player_screen_control.rs` | 玩家界面 |
| `graphwar_setup_control.rs` / `graphwar_lobby_control.rs` | 开局/大厅 |
| `graphwar_campaign_control.rs` | 战役模式 |
| `graphwar_replay_control.rs` / `graphwar_replay_player.rs` | 回放 |
| `graphwar_button.rs` / `graphwar_long_press_animation.rs` | UI 控件 |
| `graphwar_audio_control.rs` (≥241) | 音频 |
| `graphwar_name_manager.rs` / `graphwar_color_manager.rs` / `graphwar_rank_manager.rs` | 名称/配色/段位 |
| `graphwar_tick_handler.rs` | 每帧 tick |
| `graphwar_game_recorder.rs` | 对局录制 |
| `graphwar_video_writer.rs` | 录像导出 |
| `graphwar_crash_reporter.rs` (≥100) | 崩溃上报 |
| `graphwar_logger.rs` / `graphwar_nodes.rs` / `graphwar_module_container.rs` | 基础设施 |
| `graphwar_ad_switch.rs` / `android_iap.gdc` | 广告 / 内购 |

## 7. 第三方 crate（同名 `src/` 路径，勿混淆）

`src/parser.rs`、`src/uts46.rs`（idna）、`src/zio.rs`（flate2）、`src/time.rs`、
`src/socket.rs`、`src/types.rs` 等属于依赖库，不属于游戏本体。

## 8. 观测结果：函数 → 模块（首批）

**方法**（无 Ghidra，纯静态，2.5 分钟）：

1. 在二进制里扫 `src/<file>.rs` 字符串，算出其虚拟地址；
2. Rust 的 panic `Location` 是 `{ptr, len}` 结构体，字符串地址存在 `.data.rel.ro`
   的 `R_X86_64_RELATIVE` 重定位 addend 里 —— 于是反查"哪个指针槽指向这个字符串"；
3. 取引用该槽位的指令（capstone 建的 rip-relative 交叉引用）；
4. 用 `.eh_frame` 恢复的函数边界判定指令属于哪个函数。

产物：`fixtures/module_map.tsv`（脚本 `tools/module_map.py`）。

**观测到的规模**（游戏自身模块，按归属函数数排序）：

| 模块 | 归属函数 | 引用点 | 模块 | 归属函数 | 引用点 |
|---|---|---|---|---|---|
| `graphwar_game_client.rs` | 68 | 311 | `graphwar_plane.rs` | — | — |
| `graphwar_game_state_manager.rs` | 50 | 256 | `graphwar_function_input.rs` | — | — |
| `graphwar_level_manager.rs` | 40 | 590 | `graphwar_nodes.rs` | — | — |
| `graphwar_setup_control.rs` | 37 | 82 | `graphwar_play_screen_control.rs` | — | — |
| `graphwar_root_node.rs` | 24 | 186 | `graphwar_lan_vbox.rs` | — | — |
| `graphwar_mode_manager.rs` | 21 | 46 | `graphwar_client_connection_handler.rs` | — | — |
| `graphwar_module_container.rs` | 20 | 20 | `graphwar_level_generator.rs` | — | — |
| `graphwar_function_evolutionator.rs` | 16 | 39 | `graphwar_audio_control.rs` | — | — |

合计 **56 个游戏模块、544 个函数归属**（约占 13263 个可分析函数的 4.1%）。

> ⚠ 本表已按 §12 修正后的函数边界重算；§12 之前的旧数字（483 / 3.6%）作废。

**这个数字为什么小**：只有带 `Location` 的 panic / `unwrap` / 索引越界点才留锚。
没有 panic 点的函数（纯计算、被内联的叶子函数）不会出现在表里。

**下一步**：以这些锚点做种子，沿调用图扩散（被 A 调用的函数按 A 的模块归属），
并标注为"推断"而非"观测"。这一步能覆盖大部分剩余函数，但必然带噪声。

顺带可读出的结构信息：`graphwar_game_client.rs` 归属函数最多（72），说明联机客户端
逻辑远比"一个客户端类"要重；`level_manager` 的引用点密度最高（590 次 / 36 函数），
说明它对配置项的读取非常频繁。

## 9. 第三方 crate 的干扰

同一套方法也会命中依赖库（`time.rs`、`weak_caller.rs`、`entity_drawer.rs` 等），
它们**不是**游戏代码：文件名没有 `graphwar_` 前缀。统计时先按前缀过滤。
注意 `entity_drawer.rs` / `effect_drawer.rs` 虽不带前缀，但确实是游戏自己的绘制模块
（见 §2），这类要靠人工判断。

## 10. 调用图扩散（推断层）

以 §8 的 376 个锚点做种子，沿**直接调用边**扩散（`tools/propagate.py`）。
全库共解析出 **34326 条**直接调用边。

规则刻意保守：未标注函数要至少 2 条已标注邻居边、且同一模块占 ≥ 2/3 才被标注；
单一邻居的一律不标。

| 轮次 | 新增 | 累计 |
|---|---|---|
| 1 | 169 | 610 |
| 2 | 182 | 792 |
| 3 | 93 | 885 |

**结果：885 / 13263（6.7%），其中观测 441、推断 444。**

| 模块 | 观测 | 推断 | 合计 |
|---|---|---|---|
| `graphwar_root_node.rs` | 24 | **218** | 242 |
| `graphwar_game_client.rs` | 43 | 70 | 113 |
| `graphwar_obstacle_manager.rs` | 8 | 42 | 50 |
| `graphwar_setup_control.rs` | 37 | 9 | 46 |
| `graphwar_lan_vbox.rs` | 10 | 34 | 44 |
| `graphwar_function_evolutionator.rs` | 16 | 12 | 28 |

**必须说明的偏差**：`root_node` 从 16 暴涨到 194，不是因为它真有 194 个函数，
而是它作为主场景节点**调用几乎所有模块**——扩散法对 hub 节点天然高估。
所以 §10 的数字只能用于"排序参考"，不能当作模块规模结论；
真正的规模要靠反编译后逐个确认。

`plane` / `turn_manager` / `level_manager` 这些**没有**被高估（推断数为 0 或 1），
说明它们的调用关系比较封闭，可信度反而更高。

## 11. 逐模块反编译的入口

`fixtures/module_funcs.tsv` 给出每个模块的函数地址清单（观测层，474 条），
可直接喂给反编译器。

**地址换算**：这张表用的是 ELF 虚地址；Ghidra/REA 的地址 = 虚地址 **+ 0x100000**。
例如 `graphwar_plane.rs` 的首个函数 `0x4b1784` → REA 里是 `0x5b1784`。

`graphwar_plane.rs` 的 11 个函数（虚地址）：

```
0x4b1784 0x4b6b74 0x4b6d04 0x4b6dcc 0x4b6e94 0x4b6ecc
0x4b8634 0x4bc0fc 0x4bc6a4 0x4bc8ac 0x4bca94 0x4be164
```

## 12. 函数边界的数据源修正（重要）

**本节推翻了本仓库第一版的地基。**

### 12.1 发现

上游 `analysis/build_index.py` 依赖 `funcstarts.pkl`（从 `.eh_frame` 恢复函数起点）。
用它做归属时，我注意到 Ghidra 反编译出的函数入口与它**对不上**，于是做了三项交叉检查：

| 检查 | 结果 |
|---|---|
| 旧表 13430 个起点 与 Ghidra 13503 个起点 的交集 | **只有 2 个** |
| 旧表起点处是否存在指令边界 | 否（如 `0x4b1784` 处的指令根本不存在） |
| Ghidra 起点处是否是函数序言 | 是（如 `0x4b1800` = `push r15; push r14`） |
| 旧表地址是否落在 `.text` 内 | 否，最大值 `0x969054` **超出** `.text` 末尾 `0x94eb84` |

结论：`.eh_frame` 那份解析**是坏的**，Ghidra 的边界才是真的。

### 12.2 地址换算（现在钉死了）

- `Ghidra 地址 = 虚地址 + 0x100000`
- 验证：13503 个 Ghidra 函数里 **13263** 个落在 `.text` 的换算区间
  `[0x47b940, 0xa4eb84]` 内；其余 240 个是外部/其他块。

### 12.3 影响与处置

- 之前"线性反汇编与 `.eh_frame` 有字节级漂移"的说法**因果搞反了**：
  漂移的是那份 FDE 表，不是反汇编。已在 `docs/06` §11 更正。
- 归属流程改为：Ghidra 函数表 →（减 `0x100000`）→ 虚地址边界 → 与 capstone
  指令地址对齐。转换结果落在 `analysis/funcstarts_ghidra_va.pkl`（13263 条）。
- §8 / §10 的数字已按新边界重算；旧数字（483 / 628）作废。

### 12.4 仍然成立的结论

模块↔字符串的**关联**不受影响（那是"字符串被谁引用"，只看 xref，不需要函数边界）。
受影响的只是"某个函数算哪个模块"的判定，因此 §8/§10 之外、
依赖具体函数地址的结论（如 §11 里的 plane 地址清单）都需要按新边界重取。
