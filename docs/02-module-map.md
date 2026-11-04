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

## 8. 下一步：把 13430 个函数映射到模块

方法：以 panic 字符串的**引用点**为锚，取所在函数，再沿调用图扩散；
未命中的函数按调用簇归类。产物应是一张 `函数地址 → 模块` 表，
作为后续逐模块逆向的索引。
