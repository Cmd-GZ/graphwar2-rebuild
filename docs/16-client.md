# 16 · 客户端（Godot 4.7 重建）

> **本篇解决什么问题**：`client/` 这一半怎么来的、它和内核之间的接缝长什么样、
> 15 个界面各自对齐到什么程度、命中/弹道/障碍/音频/回放/联机在客户端里怎么接、
> 以及**哪些行为是我们自定的**（必须能一条条指出来，不许冒充原版）。
>
> ⚠️ **边界（先读）**：`client/` 是**照 `.tscn` 与真帧重写的仿制品**，不是原版源码。
> 原版的界面控制器是 Rust 模块（在 `.so` 里）。所以"看起来对"**不是**复原证据；
> 客户端自检条数也只是**回归保护**，不是复原证据。

> ⚠️ **条数口径（2026-11-07 深夜审计 / 2026-11-08 复核，先读）**：本篇按**轮次**记录，各节里的
> `cargo test` / `selftest` 条数（`1488` / `1494` / `1499` / `1512` / `1520` / `1584` / `1586` / `1595` …）
> 都是**那一轮当场的读数**，**不是现行基线**。
> **现行基线**：
> `cargo test --offline` **1585 / 0 failed**、`--features gdext-bindings` **1669 / 0 failed**
> —— 这两条是**本轮审计实测**（69 个测试二进制含 doc-tests，`grep '^test result'` 逐行求和）；
> `bash client/run.sh selftest` **709 / 0 failed** —— 这条是**转引** §49.7（本轮未复跑：要 Godot + 已编 `.so` + `--import`）。
> selftest 的前置条件 = `client/assets/` + `examples/serve_match` + 用 `run.sh` 的默认 `XDG_*`；
> 缺资产的机器是 **591 / 2 / 23**、补了 `serve_match` 是 **667 / 2 / 21** —— 这一档是 2026-11-07
> **当轮**读数，本次未复测（旧文里的 `606` 复现不出来）。
> ⚠️ **审计 2026-11-08 改了这里**：原记「1520 / 1595 / 694」，实测已是 **1585 / 1669 / 709**
> （selftest 的 +15 来自 §49.7）。复原进度**不看这些条数**（`AGENTS.md` §0.2）。

> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `108-client-rebuild.md` | 客户端总台账 + 自定规则登记 | [`archive/108-client-rebuild.md`](archive/108-client-rebuild.md) |
| `92-ui-inventory.md` | 15/15 界面的**运行时**控件台账 | [`archive/92-ui-inventory.md`](archive/92-ui-inventory.md) |
| `116-kernel-api.md` | GDExtension 手写 FFI 的接缝（坐标系 / ABI / 错误码） | [`archive/116-kernel-api.md`](archive/116-kernel-api.md) |
| `120-hit-test.md` | `GW2Rebuild.hit_test` 接进客户端 | [`archive/120-hit-test.md`](archive/120-hit-test.md) |
| `121-visual-fidelity.md` | 启动画面 / 深绿背景 / 图标与字体 | [`archive/121-visual-fidelity.md`](archive/121-visual-fidelity.md) |
| `137-pixel-fidelity.md` | 像素级保真：给没有真值的屏造真值 | [`archive/137-pixel-fidelity.md`](archive/137-pixel-fidelity.md) |
| `124-ui-geometry.md` | 视口 1600×900 / 去拉伸 / 主题与文案 | [`archive/124-ui-geometry.md`](archive/124-ui-geometry.md) |
| `129-player-screen.md` | PlayerScreen 白卡 / 三页签格数 / 大厅残差 | [`archive/129-player-screen.md`](archive/129-player-screen.md) |
| `130-popups.md` | 七个弹窗与返回按钮的真相 | [`archive/130-popups.md`](archive/130-popups.md) |
| `145-tab-fix.md` | Faces/Hats 动态合成底色 + JoinLevelPopUp 比例 | [`archive/145-tab-fix.md`](archive/145-tab-fix.md) |
| `126-level-obstacles.md` | 关卡障碍的绘制与参与命中 | [`archive/126-level-obstacles.md`](archive/126-level-obstacles.md) |
| `128-audio-client.md` | 音频播放层 / 音量 / 静音 | [`archive/128-audio-client.md`](archive/128-audio-client.md) |
| `133-replay-player.md` | 回放播放器（tick 推进 + 内核重算弹道） | [`archive/133-replay-player.md`](archive/133-replay-player.md) |
| `78-replay-files.md` | 回放文件的位置与格式 | [`archive/78-replay-files.md`](archive/78-replay-files.md) |
| `134-net-client.md` | 客户端联机层（真的能连服务器） | [`archive/134-net-client.md`](archive/134-net-client.md) |
| `139-play-online.md` | PlayScreen 接上联机 | [`archive/139-play-online.md`](archive/139-play-online.md) |
| `135-fire-fx.md` | 三条开火表现（键盘 / 光圈 / 弹道 tick 推进） | [`archive/135-fire-fx.md`](archive/135-fire-fx.md) |
| `140-plane-visual.md` | 准备圈与弹道绘制的画法（扒到代码了） | [`archive/140-plane-visual.md`](archive/140-plane-visual.md) |
| `143-play-flow.md` | 对局流程的六条硬伤 | [`archive/143-play-flow.md`](archive/143-play-flow.md) |

界面**静态属性**的权威来源是恢复出的原版工程（[`20-source-recovery.md`](20-source-recovery.md)）；
坐标系与掩膜在 [`08-plane.md`](08-plane.md)；事件与报文在 [`07-protocol.md`](07-protocol.md)；
关卡数据在 [`10-campaign.md`](10-campaign.md)。证据标记沿用四档。

## 1. 客户端总台账

### 1.1 一页结论

| 项 | 结果 |
|---|---|
| 工程位置 | `gw2-rebuild/client/`（Godot **4.7.2**，与游戏同版本） |
| 界面 | **15/15 全部实现**（7 个基础界面 + 8 个弹窗），可按原版路由互相跳转 |
| 能否启动 | `godot --headless --path client --quit` → **无错误、退出码 0** |
| 能否开一局 | ✅ 设置对局 → 加 Bot → Start → 键盘输入函数 → Fire → 画弹道 → 命中判定 → 结算框 |
| 自动验收 | `bash run.sh selftest` → **93 条断言全绿**（退出码 0） |
| 节点树导出 | `bash run.sh dump` → 每个节点一行（格式仿 `16-client.md` 的 `@@N` 行，含 pos/size） |
| 逐界面截图 | `bash run.sh shots`（Xvfb + OpenGL）→ 11 张 PNG，用于肉眼核对布局 |
| 游戏资产 | **0 字节**：无 `.tscn`/`.gd`/`.ttf`/`.svg` 原件，无字体文件，图标全部现画 |
| 界面逻辑归属 | 仍在 Rust（`16-client.md` §2.3）；本工程通过 `core_bridge.gd` 这一层接缝调用内核 |
| 弹道 | ~~内核只暴露 `ping()`，所以是界面预览~~ ✅ **已接上**（`16-client.md`）：`client/gw2rebuild.gdextension` 已落地，`GwCore.compute_path()` 的 `source` 现在是 **`kernel`**，PlayScreen 画的就是内核采样器的输出；⚠️ 新增 `.gdextension` 后必须跑一次 `--import` 才会被扫描到 |

三句话：

1. **「形」可以照台账重建，「行为」必须自己写。** 15 个界面的控件树、文本、图标名、选项循环
   都在 `16-client.md`/`104`/`101` 里，够搭出来；但按钮按下之后干什么（原版在 Rust 里）没有任何
   反编译产物，只能按台账的"作用"列重写 —— 这就是 `src/app/app.gd` 的命令总线。
2. **最重要的一个发现（新）**：恢复出的 `.tscn` 里每个按钮都带 `input_text`，
   那串小写标识符就是 **Rust 侧的命令键**（`campaign` / `add_bot_3` / `fire` / `toggle_names`…）。
   本项目把它当成界面与内核之间的**协议**照抄，见 §4。
3. **凡是我们自己定的规则，本文 §7 全部登记**（命中判定、关卡解锁、Bot 分队、聊天、
   回放、内购、广告、ScrollContainer 的取舍…），不允许出现"看起来像原版其实是编的"。

---### 一页结论

| 项 | 结果 | 证据 |
|---|---|---|
| 界面（`ScreensControl` 的直接子节点） | **15 个，全部进入过** | `@@SCR` 行，每个界面至少一次 `visible=1` |
| 场景 `.scn`（游戏本体） | 25 个；**22 个观测到实例化节点**，3 个未观测 | `scene=` 字段（§5） |
| 控件节点（启动态全树） | 858 个节点 / 283 个 `GraphwarButton` / 25 个 `Label` / 4 个 `LineEdit` | `boot_all_screens` 快照 |
| 挂到活动节点上的脚本 | 46 个（`.gdc` 共 105 个，其中 51 个非插件） | `@@N` 第 5 段 |
| `.gdc` 反编译 | **无可用反编译器**（离线检索无 gdsdecomp/GDRE/pip 包）；但我们解开了 `GDSC` 容器与**字符串池**（105/105） | §2 |
| 本仓库对界面的实现 | **0 个控件**——本仓库是 Rust 逻辑库（`GW2Rebuild` RefCounted），没有 Godot 客户端 | `src/_scaffold/gdext_class.rs` |

**最重要的一条结论（推翻了 `06-verification.md` §11–§12 的判断）**：

> 界面**不在** `.gdc` 里。51 个非插件 `.gdc` 里，**绝大多数是"随窗口缩放重算字号/边距"的
> 布局助手**（`_ready` → `size_changed.connect` → 遍历子节点 `add_theme_font_size_override`），
> 按钮点下去发生什么（切屏、加 Bot、开火、连服）**全部由 Rust 的 `GraphwarRootNode` 完成**。
> 证据：启动快照共 **2724 条信号连接**，其中 1851 条是引擎容器内部、846 条是按钮的
> `pressed/button_up/button_down` 自连（282 个按钮 × 3），只剩 **27 条**"人为连接"：
> 12 条广告 + 9 条内购 + **6 条游戏 UI**（`ShowExtraButtons.pressed→_button_pressed`、
> `GraphwarFunctionInput.text_changed→on_text_change` / `text_submitted→on_text_submitted`、
> `RoomNameLineEdit.text_changed→on_text_change`、`ChatLineEdit.text_submitted→on_chat_submitted`、
> `PlayerNameLineEdit.text_changed→on_text_change`）。逐条见 §2.3、§7。

---


## 2. 15/15 界面的运行时台账（摘要）

### 2.1 逐界面台账

> 列义：
> **类型**＝`get_class()`；**文本/图标**＝运行时观测到的 `text`/`icon`/`texture`；
> **作用**＝`观测`（有直接证据）或 `推断`（写明缺什么）；
> **本仓库**＝`❌`＝没有这个控件（我们没有客户端）；`逻辑✅/🔶/❌ src/xxx` 指该控件背后
> 的游戏逻辑在本仓库的复原程度。
> 路径统一省略前缀 `GraphwarRootNode/VBoxContainer/ScreensControl/`。

#### 2.1.1 MainMenu（主菜单）

进入方式：启动即在此。观测快照：全部 39 个（`MainMenu.visible=1` 于 `boot`）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `MainMenu/MarginContainer/VBoxContainer/MarginContainer/Label` | Label | `Graphwar II` | 标题（`minsize=(64,0)`，字号由 `TitleSize.gd` 调） | ❌ |
| `.../HBoxContainer/MarginContainer/VBoxContainer/PlayButton` | GraphwarButton | `Campaign` | **观测**：点它 → `Campaign.visible=1` | ❌ 控件；关卡生成 逻辑✅ `src/graphwar_level_generator.rs` |
| `.../VBoxContainer/CharacterButton` | GraphwarButton | `Player\n` | **观测**：点它 → `PlayerScreen.visible=1` | ❌ |
| `.../MarginContainer2/VBoxContainer2/LocalButton` | GraphwarButton | `Battle` | **观测**：点它 → `LanLobby.visible=1` | ❌（联机 逻辑✅ `src/server.rs`/`_scaffold/protocol.rs`） |
| `.../VBoxContainer2/ReplayButton` | GraphwarButton | `Replays` | **观测**：点它 → `ReplayLobby.visible=1` | ❌（回放格式 观测✅ `16-client.md`） |
| `.../VBoxContainer2/MarginContainer3/VersionLabel` | Label | `2.0.60` | 版本号（硬编码在场景里；`VersionLabel.gd` 只管字号） | ❌ |
| `.../VBoxContainer/AdContainer` | MarginContainer | — `AdContainer.gd` | 广告位（我们 harness 里是桩 `Ad_holder.gd`） | ❌ |

⇒ 用户口述的"主界面 4 个按钮"**成立且已逐项核对**（Campaign / Player / Battle / Replays）。

#### 2.1.2 Campaign（关卡选择）

进入方式：`MainMenu/PlayButton`。观测快照：`campaign`、`campaign_after_level0`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `Campaign/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer/Label` | Label | `CAMPAIGN` | 标题 | ❌ |
| `.../VBoxContainer/Control/level_0 … level_20` | GraphwarButton ×**21** | `0`…`20` | **观测**：`level_0` 按下 → `JoinLevelPopUp.visible=1`（进关卡预览）。**推断**：每个按钮一个关卡；21 关与用户口述"21 关（0–20）"一致。**缺**：锁定态（`lock.svg` 存在但本进程全部可点，因为本地存档是新的/未锁定状态未知） | ❌ 控件；关卡生成 逻辑✅ `src/graphwar_level_generator.rs`（**只有两种模式**，见 `11-levelgen.md`） |
| `Campaign/.../Control` | Control | — `CampaingLevelsButtons.gd` | **观测（字符串池）**：这 21 个按钮**是脚本运行时 new 出来的**（`GraphwarButton.new()` + `set_name("level_"+str(i))` + `add_child`），不是场景里摆好的 | ❌ |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton ×2 | icon=`return.svg` | 返回（`ClipButton` 按下无反应，`LongPressButton` 是长按返回；见 §4.11 的同类验证） | ❌ |

#### 2.1.3 JoinLevelPopUp（关卡预览 / 开局确认）

进入方式：`Campaign/.../level_0`。观测快照：`campaign_after_level0`（`visible=1`）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `JoinLevelPopUp/.../VBoxContainer/LevelTitleLabel` | Label | `Level 0` | 关卡标题（`LevelTitleLabel.gd` 调字号） | ❌ |
| `.../HBoxContainer/MarginContainer/ScrollContainer/LevelDescriptionEdit` | TextEdit | `Write a function of x to hit the …`（观测截断，见下） | 关卡说明（只读文本） | ❌ |
| `.../VBoxContainer/OptionsContainer/AspectRatioContainer/AxisModeButton` | GraphwarButton | icon=`axis_option_*.svg` | 本关的坐标轴模式（图标显示，不可改） | ❌ |
| `.../OptionsContainer/AspectRatioContainer2/FunctionModeButton` | GraphwarButton | icon=`fx_option.svg`/`dx_dy_option.svg` | 本关函数模式 | ❌ |
| `.../OptionsContainer/AspectRatioContainer3/TurnModeButton` | GraphwarButton | icon=`turns_sequential/simultaneous.svg` | 本关回合模式 | ❌ |
| `.../OptionsContainer/AspectRatioContainer4/TimeModeButton` | GraphwarButton | icon=`turn_*.svg` | 本关回合时限（注意：这里叫 **TimeModeButton**，GameSetup 里叫 `TurnTimeButton`） | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | `Cancel` | 取消 | ❌ |
| `.../HBoxContainer3/MarginContainer2/GraphwarButton` | GraphwarButton | `Start` | **推断**：开始本关（未按下验证，避免污染存档） | ❌ |

#### 2.1.4 LanLobby（联机大厅：Local / Global）

进入方式：`MainMenu/LocalButton`。观测快照：`lan_lobby_local`、`lan_lobby_global`、
`connect_room__connecting_timeout`（房间列表已填充）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `LanLobby/.../HBoxContainer/ShowLocalContainer/ShowLocalButton` | GraphwarButton | `Local`（`toggle;checked`） | **观测**：切换按钮，默认选中 | ❌ |
| `.../HBoxContainer/ShowGlobalContainer/ShowGlobalButton` | GraphwarButton | `Global`（`toggle`） | **观测**：按下后 `GraphwarLanVBox` 里出现 **5 个公网房间条目**：`US Room 1`、`US Room 3`、`US Room 4`、`EU Room 1`、`EU Room 2`（注意**没有** `US Room 2`）。⇒ `06-verification.md` §10 里"ShowGlobalButton/ShowLocalButton 用途未知"已解：**是 LAN 大厅的"本地/公网房间列表"开关，不是回放界面** | ❌ 控件；协议 观测✅ `09-network.md` |
| `.../MarginContainer/HBoxContainer/CreateButton` | GraphwarButton | ` Create Game ` | **观测**：→ `GameSetup.visible=1` | ❌ 控件；建房/服务端 逻辑✅ `src/server.rs` |
| `.../MarginContainer/HBoxContainer/JoinButton` | GraphwarButton | ` Join IP ` | **观测**：→ `JoinIpPopUp.visible=1` | ❌ |
| `.../MarginContainer2/VBoxContainer/HBoxContainer/NameContainer/RoomName` | Label | `Room Name` | 列表表头 | ❌ |
| `.../PlayersContainer/RoomName` | Label | `Players` | 列表表头（名字重复，是场景原作者的习惯） | ❌ |
| `.../ModeContainer/GameMode` | Label | `Game Mode` | 列表表头 | ❌ |
| `.../ScrollContainer/GraphwarLanVBox/<房间条目>` | Control（`scene=res://game_room_entry.tscn`） | 每条：`NameContainer/RoomName`=`US Room 1`/`EU Room 1`…、`PlayersContainer/PlayerCount`=`  0  `/`  1  `、`ModeContainer/OptionsContainer/{Axis,Function,Turn,Time}ModeButton` 四个图标、`JoinButtonContainer/JoinButton`=` Join ` | **观测**：公网房间列表条目；Join 按下后**没有**出现 `ConnectingPopUp`（`connect_room` 路由 20 s 超时）⇒ **推断**：公网房间列表来自服务器/缓存，离线时 Join 不发起连接或直接静默失败。**缺**：房间列表的来源（无网络，未抓包验证） | ❌ |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回主菜单 | ❌ |

#### 2.1.5 GameSetup（对局设置）

进入方式：`LanLobby/CreateButton`。观测快照：`game_setup`、`setup_initial`、
`setup_six_bots`（6 个 Bot）、`setup_after_cycle`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `GameSetup/GraphwarSetupControl` | GraphwarSetupControl（Rust） | — | 整个设置界面的 Rust 控件根 | ❌ |
| `.../VBoxContainer/HBoxContainer/RoomName/RoomNameLineEdit` | LineEdit | `Graphwar Game` | 房间名，可编辑（`RoomName.gd` 调字号） | ❌ |
| `.../HBoxContainer/MarginContainer2/ChatButtonSetup` | GraphwarButton | icon=`no_chat.svg` | 打开聊天（图标随"有新消息"变成 `no_chat_new_chat.svg`——**推断**，只在场景里见到两个图标名） | ❌ |
| `.../HBoxContainer/MarginContainer3/LockButton` | GraphwarButton | icon=`unlock.svg` | 房间上锁/解锁（锁图标 `lock.svg`/`unlock.svg`） | ❌ |
| `.../HBoxContainer/MarginContainer4/GhostButton` | GraphwarButton | `         `（空白占位） | 标题占位（`LanLobbyTitle.gd`） | ❌ |
| `.../VBoxContainer/MarginContainer/HBoxContainer/MarginContainer/GameIpLabel` | Label | **`198.18.0.1:36071`** 等（每个进程不同） | **观测：这就是 `game_ip_label`——本机房间的 IP:端口**，`06-verification.md` §10 里标"未查"的一项，现已拿到 | ❌ 控件；端口/握手 观测✅ `09-network.md` |
| `.../OptionsContainer/AxisModeButton` | GraphwarButton | icon=`axis_option_?.svg` | 坐标轴模式，点击循环（§6） | ❌ 控件；对物理无影响 推断✅（`06-verification.md` §14） |
| `.../OptionsContainer/FunctionModeButton` | GraphwarButton | icon=`fx_option.svg`⇄`dx_dy_option.svg` | 函数模式 `NormalFunction`⇄`DiffEqFunction`，点击切换 | ❌ 控件；**逻辑✅** `src/graphwar_path_calculator.rs`（RK4）、`src/graphwar_function_evolutionator.rs`（变量权重） |
| `.../OptionsContainer/TurnModeButton` | GraphwarButton | icon=`turns_sequential.svg`⇄`turns_simultaneous.svg` | 回合模式，点击切换 | ❌ 控件；**逻辑✅** `src/graphwar_turn_manager.rs`+`src/graphwar_shot_calculator/settle.rs`（含同时回合结算 `14-turn.md`） |
| `.../OptionsContainer/TurnTimeButton` | GraphwarButton | icon=`turn_30s/1m/2m/3m/5m/inf.svg` | 回合时限，点击循环（§6） | ❌ 控件；只影响思考时间 |
| `.../OptionsContainer/AddBotButton` | GraphwarButton | icon=`add_bot_option.svg` | **观测**：→ `AddBotPopUp.visible=1` | ❌ |
| `.../MarginContainer/HBoxContainer/StartButton` | GraphwarButton | ` Start ` | **观测**：→ `PlayScreen.visible=1`。**注意**：不加 Bot 直接 Start 会静默无操作（`archive/22-live-game-harness.md` §5.1） | ❌ 控件；开局/回合状态机 **逻辑✅** `src/turnstate.rs` |
| `.../MarginContainer2/ScrollContainer/PlayerListContainer/Team1Container` | VBoxContainer（`scene=res://team1_entry.tscn`） | `Label`=`Team 1` | 队伍 1 列表 | ❌ |
| `.../PlayerListContainer/Team2Container` | VBoxContainer（`scene=res://team2_entry.tscn`） | `Label`=`Team 2` | 队伍 2 列表 | ❌ |
| `.../TeamXContainer/HBoxContainer`（每个玩家一行，`scene=res://team1/2_entry.tscn` 的子结构） | MarginContainer 组合 | 见下 | **玩家行**（每行一个玩家） | ❌ |
| └ `.../HBoxContainer/MarginContainer/PlayerNameLabel` | Label | `user` / `Deep Blue` / `Alpha Zordon` / `ChatGPT` / `Multivac` / `Deep Thought` / `Skynet` / `Agent Smith` | 玩家名 | ❌ |
| └ `.../HBoxContainer/RankIcon` | TextureRect | 玩家 `ranks/rank_0.svg`；Bot `hats/no_hat.svg`（空） | 段位图标 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer/RemovePlayerButton` | GraphwarButton | icon=`x.svg` | 踢出玩家 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer2/AddSoldierButton` | GraphwarButton | icon=`plus.svg` | 该玩家**加一个士兵**（多士兵=多命） | ❌ 控件；多士兵生命 **逻辑✅** `src/graphwar_life_manager.rs` |
| └ `.../HBoxContainer/AspectRatioContainer3/RemoveSoldierButton` | GraphwarButton | icon=`minus.svg` | 减一个士兵 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer4/ChangeTeamButton` | GraphwarButton | icon=`arrow_right.svg`（Team1）/`arrow_left.svg`（Team2） | 换队 | ❌ |
| └ `.../HBoxContainer/SoldierIconContainer/MarginContainer…5` | MarginContainer ×5（`SoldierIconMarginContainer.gd`） | 每个槽 4 个 `TextureRect`：`SkinIcon`=`skins/yellow.svg` 或 `skins/bot_N.svg`、`FaceIcon`=`faces/regular_eyes.svg`/`no_face.svg`、`HatIcon`=`hats/soldier_helmet.svg`/`no_hat.svg`、`EmptyIcon`=`empty_soldier.svg` | **观测**：每个士兵一个槽（**5 个槽**⇒每玩家最多 5 个士兵），空槽显示 `empty_soldier.svg` | ❌ 控件；皮肤系统 `rsc/skins|faces|hats` |
| `.../VBoxContainer/AdContainer` | MarginContainer | — | 广告位 | ❌ |

**观测到的 Bot 名单**（6 档各点一次）：`Deep Blue`、`Alpha Zordon`、`Alpha Zordon`、`Multivac`、
`Deep Blue`、`ChatGPT` —— 名字会重复，**证明名字池与难度不是 1:1**（与 `AI.md` §1 的记载一致：
另有 `Skynet`/`HAL`/`Cleverbot` 等名字）。

#### 2.1.6 AddBotPopUp（加 Bot）

进入方式：`GameSetup/OptionsContainer/AddBotButton`。观测快照：`add_bot_popup`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `AddBotPopUp/.../VBoxContainer/OptionsContainer/AspectRatioContainer/AddBotButton` | GraphwarButton | icon=`skins/bot_0.svg` | 难度档 0 | ❌ 控件；**六档参数表 逻辑✅** `AI.md` §1 / `src/graphwar_function_evolutionator.rs` |
| `.../OptionsContainer/AspectRatioContainer2/AddBotButton` | GraphwarButton | icon=`skins/bot_1.svg` | 难度档 1 | 同上 |
| `.../OptionsContainer/AspectRatioContainer4/AddBotButton` | GraphwarButton | icon=`skins/bot_2.svg` | 难度档 2（**本文件的默认点击目标**） | 同上 |
| `.../OptionsContainer2/AspectRatioContainer/AddBotButton` | GraphwarButton | icon=`skins/bot_3.svg` | 难度档 3 | 同上 |
| `.../OptionsContainer2/AspectRatioContainer2/AddBotButton` | GraphwarButton | icon=`skins/bot_4.svg` | 难度档 4 | 同上 |
| `.../OptionsContainer2/AspectRatioContainer4/AddBotButton` | GraphwarButton | icon=`skins/bot_5.svg` | 难度档 5（**最强**：`endgame6` 用它，31 s 内把玩家打死） | 同上 |
| `.../VBoxContainer/HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Cancel ` | 取消 | ❌ |
| `.../MarginContainer/HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

> **推断（缺证据）**：`bot_0..bot_5` 图标与 `AI.md` 六档（ChatGPT/Wolfram/Alpha Zordon/
> Multivac/Deep Blue/Deep Thought）的对应关系没有直接观测；只能确认"6 个按钮、6 个图标、
> 加进的 Bot 用对应 `skins/bot_N.svg` 皮肤"。要证实需读 Rust 内部表。

#### 2.1.7 JoinIpPopUp（输入 IP 加入）

进入方式：`LanLobby/JoinButton`。观测快照：`join_ip_popup`、`connect_ip_port__after_join_click`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `JoinIpPopUp/.../HBoxContainer/MarginContainer/Label` | Label | `Ip:` | 提示 | ❌ |
| `.../HBoxContainer/MarginContainer2/JoinIpIpLineEdit` | LineEdit | 空；我们写入过 `10.255.255.1`、`10.255.255.1:12345`、`127.0.0.1:1` | IP 输入（**必须带端口**） | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Cancel ` | 取消 | ❌ |
| `.../HBoxContainer3/MarginContainer2/GraphwarButton` | GraphwarButton | `  Join  ` | **观测**：按下 `127.0.0.1:1` → 2 s 内出现 `DisconnectedPopUp`；按下 `10.255.255.1:12345` → 2 s 内出现 `ConnectingPopUp`。**只写 `10.255.255.1`（无端口）时毫无反应**（120 s 超时）⇒ 端口必填，格式 `host:port` | ❌ 控件；握手 **逻辑✅/观测** `09-network.md`、`src/_scaffold/protocol.rs` |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

#### 2.1.8 ConnectingPopUp（连接中）

进入方式：`JoinIpPopUp` 里 Join 一个**不可达但端口合法**的地址（`10.255.255.1:12345`）。
观测快照：`connect_ip_port__connecting`（`@@WATCHHIT` ms=1915）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ConnectingPopUp/.../HBoxContainer/MarginContainer/Label` | Label | `Connecting...` | 提示 | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Cancel ` | 取消连接 | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

#### 2.1.9 DisconnectedPopUp（断开）

进入方式：`JoinIpPopUp` 里 Join `127.0.0.1:1`（端口拒绝）。
观测快照：`connect_refused__disconnected`（`@@WATCHHIT` ms=1980）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `DisconnectedPopUp/.../HBoxContainer/MarginContainer/DisconnectedLabel` | Label | `You have been disconnected.` | 提示 | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Ok ` | 确认 | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

#### 2.1.10 PlayScreen（对局）

进入方式：`GameSetup` 加 Bot → `StartButton`。观测快照：`play_screen_t0/t3s/t6s`、
`play_diffeq_t0/t3s`、`play_extra_buttons`、`play_names_toggled`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `PlayScreen/.../PlayMainHBox/LeftControls/LeftButtonGrid` | GridContainer `ButtonGrid.gd` | **18 个** `GraphwarButton` | **左键盘**：`C` `π` `e` `1`…`9` `0` `.` `(` + 三个控制键（icon=`arrow_left_bar.svg` / `arrow_left.svg` / `delete_left.svg`） | ❌ 控件；函数文法 **逻辑✅** `src/math_function/` |
| `.../RightControls/RightButtonGrid` | GridContainer `ButtonGrid.gd` | **18 个** `GraphwarButton` | **右键盘**：`abs` `ln` `√` `sin` `cos` `tan` `*` `/` `exp` `+` `-` `^` `x` `y`(disabled) `)` + icon=`delete_right.svg` / `arrow_right.svg` / `arrow_right_bar.svg` | ❌ 控件；文法 **逻辑✅** `src/math_function/` |
| `.../RightButtonGrid/YButton` | GraphwarButton | `y`，**永远 `disabled`** | **观测**：Normal 与 DiffEq 两种模式下都 disabled。**推断**：与"`y'` 是死特性、DiffEq 只生成 x/y 权重"有关，但**为什么 `y` 也不给按**未知 | ❌ |
| `.../RightControls/HBoxContainer/FireContainer/FireButton` | GraphwarButton | ` Fire! ` | 开火。**`archive/26-harness-control.md` 观测**：`emit_signal("pressed")` 真的发出 `FunctionFireRequest`，随后 `shot_calc/calculate_path` 计数上升（本文件只观测到按钮存在且未 `disabled`，**没有重复验证开火**） | ❌ 控件；**开火链路 逻辑✅** `src/graphwar_shot_calculator.rs`/`graphwar_path_calculator.rs` |
| `.../FireContainer/ClipButton` | GraphwarButton | ` Fire! `（同文本，带 `AnimationRect` 子节点） | 开火按钮的动画层（点击用 `FireButton`） | ❌ |
| `.../LeftControls/LeftTopButtonsContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton ×2 | icon=`return.svg` | 退出对局（**长按**才出确认框，见 §4.11） | ❌ |
| `.../LeftTopButtonsContainer/MarginContainer/ShowExtraButtons` | GraphwarButton `ShowExtraButtons.gd` | icon 由脚本 `set_button_icon(load(...))` 切换（`hide_extra_buttons.svg`/`show_extra_buttons.svg`） | **观测（字符串池）**：切换"附加按钮区"显示，`set_show_extra_buttons(...)` 联动 `plane_container`/`view_function_pad` | ❌ |
| `.../PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox/ViewFunctionButton` | GraphwarButton | icon=`view_all_functions.svg`/`view_recent_functions.svg`/`view_single_function.svg`（三个候选名都在 `rsc/`） | 查看函数列表（**推断**：图标名三选一，未逐一点击验证） | ❌ |
| `.../ExtraOptionsHBox/ShowNames` | GraphwarButton | **观测** icon=`view_names.svg`（另有 `hide_names.svg`） | 显示/隐藏士兵名字。**观测**：按下前后 `GraphwarPlane/@EntityDrawer@N` 下 4 个名字 `RichTextLabel` 由 `visible=1` 全变 `visible=0` | ❌ |
| `.../ExtraOptionsHBox/MuteButton` | GraphwarButton | **观测** icon=`unmute.svg`（点后推测变 `mute.svg`） | 静音开关（未点击验证） | ❌ |
| `.../ExtraOptionsHBox/ChatButtonPlay` | GraphwarButton | **观测** icon=`no_chat.svg`（有消息时推测变 `no_chat_new_chat.svg`） | **观测**：→ `ChatPopUp.visible=1` | ❌ |
| `.../PlaneContainer/GamePlane/PlaneContainer/GraphwarPlane` | GraphwarPlane（Rust） | — | 战场（Rust 自绘）。子节点：`@AxisDrawer@N`、`@EntityDrawer@N`、`@EffectDrawer@N`、`@Line2D@N` | ❌ 控件；**坐标/掩膜 逻辑✅** `src/plane.rs` |
| `GraphwarPlane/@EntityDrawer@N/@Sprite2D@N` | Sprite2D | `texture=res://skins/yellow.svg`、`faces/regular_eyes.svg`、`hats/soldier_helmet.svg`；Bot 用 `skins/bot_N.svg`、`faces/no_face.svg`、`hats/no_hat.svg` | **观测**：每个士兵 3 个 Sprite2D（皮肤/脸/帽） | ❌ |
| `GraphwarPlane/@EntityDrawer@N/<name_label 实例>` | RichTextLabel（`scene=res://name_label.tscn`） | 名字文本（bbcode，观测到空串时是未填） | 士兵头顶名字 | ❌ |
| `GraphwarPlane/@Line2D@N` | Line2D | — | **观测**：开火后出现（`scene_file_path` 为空 ⇒ **不是** `line_2d.tscn` 实例，而是运行时新建/由 Rust 建） | ❌ 控件；弹道 **逻辑✅** `src/graphwar_path_calculator.rs` |
| `PlayScreen/.../VBoxContainer/HBoxContainer2/GraphwarFunctionInput` | GraphwarFunctionInput（Rust） | `text` = 当前函数串，如 `(1.44*(cos(x)+x))`、`log(exp((12.10/(-19.42/x))))`、`sin((-1.20+x))` | 函数输入框（Rust 自绘，无 LineEdit 子节点）。**观测**：`text` 属性可读 ⇒ 能读到场上显示的函数 | ❌ 控件；解析 **逻辑✅** `src/math_function/` |
| `.../HBoxContainer2/FxContainer/FxLabel` | Label | ` f(x) = `（DiffEq 时同位置显示 `dx/dy` 图标） | 函数前缀提示 | ❌ |
| `.../FxContainer/AspectRatioContainer/FxTextureButton` | GraphwarButton | icon `fx_option.svg`（Normal）/`dx_dy_option.svg`（DiffEq） | **观测**：模式指示图标 | ❌ |
| `.../HBoxContainer2/TimerContainer/TimerLabel` | Label | `02:59`、`01:59`、`00:04`、`00:00` | 回合倒计时。**观测**：按 2 次 `TurnTimeButton` ⇒ 起始 `02:59`（3 分钟档）、按 1 次 ⇒ `01:59`（2 分钟档）；见 §6 | ❌ 控件；时钟 **观测✅** `14-turn.md` |
| `.../HBoxContainer2/OffsetContainer/OffsetLabel` | Label | Normal：` + 2.515 `、` + 0.919 `；**DiffEq：空** | 修正常数显示。**观测**：DiffEq 模式下为空（`+c` 修正不显示/不适用） | ❌ 控件；修正 **逻辑✅** `src/graphwar_shot_calculator.rs` |
| `.../HBoxContainer2/GraphwarFunctionInput`（同上） | — | — | — | — |
| `PlayScreen/.../VBoxContainer/PlayMainHBox/PlaneContainer/GamePlane/PlaneContainer/VBoxContainer/HBoxContainer/ViewFunctionsLabel` | RichTextLabel `ViewFunctionsLabel.gd` | `[outline_size=4][outline_color=black][color=green]player 1: f(x) = 3x+234[/color]…` | 顶部"各玩家当前函数"列表 | ❌ |
| `.../RightControls/HBoxContainer/FireContainer/ClipButton` | — | — | （同上，动画层） | ❌ |

**DiffEq 与 Normal 的运行时差异（观测，`play_diffeq` vs `play`）**：键盘完全一致（含 `y` 仍 disabled）、
`OffsetLabel` 从 `+ 0.919` 变空、`FxTextureButton` 图标 `fx_option.svg`→`dx_dy_option.svg`。
⇒ **界面几乎不区分两种模式**，差异只在 Rust 内的取值/显示。

#### 2.1.11 ConfirmPlayExitPopUp（退出确认）

进入方式：对局里 `LeftTopButtonsContainer/ExitButtonContainer/**LongPressButton**`。
观测快照：`play_exit__confirm_exit_after_longpress`（`visible=1`）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ConfirmPlayExitPopUp/.../HBoxContainer/MarginContainer/Label` | Label | `Are you sure you want to exit?` | 提示 | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | `  No  ` | 不退出 | ❌ |
| `.../HBoxContainer3/MarginContainer2/GraphwarButton` | GraphwarButton | `  Yes  ` | 退出（**推断**，未点，避免中断已有观测） | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

> **观测细节**：`ExitButtonContainer` 里 `ClipButton` 单击**不弹**确认框，`LongPressButton`
> 单击**弹**。⇒ `LongPressButton` 才是"点一下就走"的入口，`ClipButton` 是动画/长按载体
> （与 `ExitButtonContainer.gdc` 只做字号缩放的事实一致：**退出逻辑在 Rust**）。

#### 2.1.12 ChatPopUp（聊天）

进入方式：对局里 `ExtraOptionsHBox/ChatButtonPlay`（Setup 里是 `ChatButtonSetup`）。
观测快照：`play_chat__play_chat_popup`（`ChatPopUp.visible=1`）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ChatPopUp/ChatContainer/ChatContainer2/ChatBox` | RichTextLabel `ChatBox.gd` | 空（无消息时） | 聊天记录。**观测（字符串池）**：`_gui_input` 里点它 → `chat_line_edit.grab_focus()` | ❌ 控件；聊天协议 **观测✅** `09-network.md` |
| `.../ChatContainer2/ChatLineEdit` | LineEdit | 空 | 输入框；`text_submitted → on_chat_submitted`（**观测**：唯一一条指向 GDScript 的 `text_submitted` 连接，属于 `chat_pop_up.tscn` 内联脚本 `GDScript_k8tdp`） | ❌ |
| `.../ChatContainer2/MarginContainer/GraphwarButton` | GraphwarButton | `      `（空白，图标未在 `icon` 属性上） | 发送按钮（**推断**：无 text/icon，靠主题/子节点画） | ❌ |

#### 2.1.13 EndGamePopUp（结算）

进入方式：**必须真的打完一局**。用**档 5 的 Bot**（`OptionsContainer2/AspectRatioContainer4`）
+ 30 s 回合时限，`endgame6` 路由里 31.7 s（首次）/154 s（复跑）后自然出现。
观测快照：`endgame6__endgame_popup`。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `EndGamePopUp/.../VBoxContainer/SelfVictoryLabel` | Label | **`YOU lOST`**（游戏原文如此，小写 l） | 自己的胜负 | ❌ |
| `.../VBoxContainer/TeamVictoryLabel` | Label | `Team 2 Victory!` | 获胜方 | ❌ 控件；胜负判定 **逻辑✅** `src/graphwar_shot_calculator/settle.rs`、`14-turn.md` |
| `.../VBoxContainer/HBoxContainer/VictoryPlayerContainer1 … 10` | MarginContainer ×10 | 每个槽 3 个 `TextureRect`：`SkinIcon`（观测 `skins/bot_5.svg`）、`FaceIcon`（`faces/no_face.svg`）、`HatIcon`（`hats/no_hat.svg`） | 胜利方士兵展示（**10 个槽**⇒最多展示 10 个士兵） | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

> 在此之前的两轮尝试（档 2 的 Bot，各等 300 s / 420 s）**都没有出现**结算框
> ⇒ 结算框依赖"真的有人全灭"，不依赖时间。

#### 2.1.14 PlayerScreen（玩家：改名 / 皮肤 / 脸 / 帽子）

进入方式：`MainMenu/CharacterButton`。观测快照：`player_screen`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `PlayerScreen/.../HBoxContainer/MarginContainer/Label` | Label | `PLAYER OPTIONS` | 标题 | ❌ |
| `.../HBoxContainer3/TabContainer` | TabContainer `PlayerTabContainer.gd` | 3 个页：`SkinsContainer`、`FacesContainer`、`HatsScroll` | 三个分页。**观测（字符串池）**：脚本用 `set_tab_title(i, …)` 设标题；标题字面量不在字符串池 ⇒ 取自子节点 | ❌ |
| `.../TabContainer/SkinsContainer/VBoxContainer/HBoxContainer/SkinsVbox` | VBoxContainer `ShopButtonsSize.gd` | **5 行 × 3 个** `GraphwarButton`（15 个，直接摆在 `SkinsVbox` 里，**不是** `shop_button_entry` 实例） | 皮肤选择网格 | ❌ 控件；皮肤资源 `rsc/skins/` |
| `.../TabContainer/FacesContainer/.../FacesVbox` | VBoxContainer `ShopButtonsSize.gd` | **5 行 × 3 个槽**（15 个），每槽 `Control`（`scene=res://shop_button_entry.tscn`）内含 `GraphwarButton` + **`SkinBackgroundButton`** + `Button` | 脸选择网格。**`SkinBackgroundButton` 就在这里**（`06-verification.md` §10 里点名但没查过的控件） | ❌ |
| `.../TabContainer/HatsScroll/.../HatsVbox` | VBoxContainer `ShopButtonsSize.gd` | 同 Faces 结构，**9 行 × 3 = 27 个** `shop_button_entry` 实例 | 帽子选择网格 | ❌ |
| `.../VBoxContainer/HBoxContainer2/MarginContainer/Label` | Label | ` NAME: ` | 名字标签 | ❌ |
| `.../HBoxContainer2/MarginContainer2/PlayerNameLineEdit` | LineEdit `PlayerNameLineEdit.gd` | `user` | 改名（`text_changed → on_text_change`，Rust 回调） | ❌ |
| `.../VBoxContainer/MarginContainer/AspectRatioContainer/PlayerShowcase` | PlayerShowcase（Rust） | — | 士兵预览（Rust 自绘） | ❌ |
| `.../AspectRatioContainer/SkinBlock/VBoxContainer/Label` | Label | `Unlocking skins also removes all ads` | 广告说明 | ❌ |
| `.../SkinBlock/VBoxContainer/Label2` | Label | `Support the game` | 内购说明 | ❌ |
| `.../SkinBlock/VBoxContainer/HBoxContainer/UnlockButton` | GraphwarButton `UnlockButton.gd` | `Unlock` | 内购解锁 | ❌ |
| `.../AspectRatioContainer/PendingBlock/VBoxContainer/Label` | Label | `Pending...` | 内购挂起状态 | ❌ |
| `.../PlayerScreen/.../ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回 | ❌ |

> **`06-verification.md` §10 的"Player 界面从未查过"到此关闭**：改名 LineEdit、三页签、Skins 15 个按钮 /
> Faces 15 个槽 / Hats 27 个槽（`shop_button_entry` 实例共 **42** 个）、`SkinBackgroundButton`、
> `UnlockButton`、`PendingBlock` 全部实测到。

#### 2.1.15 ReplayLobby（回放列表）

进入方式：`MainMenu/ReplayButton`。观测快照：`replay_lobby`（59 个条目，2118 节点）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ReplayLobby/.../HBoxContainer/MarginContainer/Label` | Label | `REPLAYS` | 标题 | ❌ |
| `.../ScrollContainer/ReplaysContainer/<条目>` | HBoxContainer（`scene=res://replay_entry.tscn`） | — | 一条回放。**观测**：本机 replay 目录里有 59 个文件，就渲染 59 条 | ❌ 控件；回放格式 **观测✅** `16-client.md`、解析器 `tools/parse_replay.py` |
| └ `.../ScrollContainer/NameContainer/ReplayName` | Label | `2026-11-05 12:48:11` 等（时间戳） | 回放名=**录制时间** | ❌ |
| └ `.../GameNameContainer/GameName` | Label | `Graphwar Game` | 房间/对局名 | ❌ |
| └ `.../ModeContainer/OptionsContainer/{Axis,Function,Turn,Time}ModeButton` | GraphwarButton ×4 | 四个模式图标 | 该局的四个模式 | ❌ |
| └ `.../AspectRatioContainer/PlayReplayButton` | GraphwarButton | 图标未在 `icon` 属性上 | **播放该回放**（未点击验证播放行为） | ❌ 控件；回放重放 逻辑🔶 |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回 | ❌ |
| `.../VBoxContainer/AdContainer` | MarginContainer | — | 广告位 | ❌ |

---


## 3. 内核接缝：GDExtension 手写 FFI

### 3.1 一页结论

| 项 | 结果 |
|---|---|
| 新方法 | `GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float) -> Dictionary` |
| 契约来源 | `client/src/core/core_bridge.gd` 文件头（逐字）+ `16-client.md` §2.3 |
| 返回 | `{ ok: bool, points: PackedVector2Array, offset: float, error: String }` |
| 坐标系 | **棋盘（平面）坐标**，不是画布像素 —— 依据见 §2 |
| 用的是不是真采样器 | 是。`parse → correction → shifted_tokens → graphwar_path_calculator::calculate`，参数全部取既有常量 |
| `GwCore.calculate_path()["source"]` | **`preview` → `kernel`**（§7.3 有前后对照） |
| `cargo test --offline` | **205 → 218**（+13，全绿） |
| `bash client/run.sh selftest` | **93 passed, 0 failed**（未改任何断言；见 §7.4） |
| 改动的文件 | `src/_scaffold/api.rs`（新增）、`src/_scaffold/gdext.rs`（+3 行）、`src/lib.rs`（+1 行）、`16-client.md`（本文） |
| `client/` 改动 | **0 字节**（含 `client/` 下没有新增 `.gdextension`，见 §8.1） |
| 仍未做 | 命中/掩膜接口、ODE（微分方程）模式、`client/` 侧的扩展描述符 |
| debug / release | 两份 `.so` 的端到端结果**逐位相同**（2471 / 5295 点，弦长一致） |

三条最容易出错的点，先写在前面：

1. **坐标系**：返回棋盘坐标（`|x| ≤ 25`、`|y| ≤ 15`）。若误当像素坐标返回，整条弹道会偏 500/300。
2. **`GDExtensionPropertyInfo.class_name` / `hint_string` 不能传 NULL** —— 4.7.2 会直接解引用，实测 SIGSEGV。
3. **`variant_set_keyed` 不能用** —— 4.7.2 上实测崩溃；造 `Dictionary` 只能用 `variant_call("set")`。

---


#### 3.1.1 坐标系：为什么是棋盘坐标（这一节别跳）


**结论：`points` 用棋盘（平面）坐标，与 `GwCore._preview()` 完全一致。**
档位：**观测**（读调用方源码即可确定，无需推断）。

证据链（三步，都在 `client/` 里，逐行可查）：

1. `client/src/screens/play_screen.gd:454` —— 传进去的是 `active_soldier.pos`，
   而这个 `pos` 与 `GwPlaneView.SOLDIER_RADIUS = 0.8` 同量纲、被 `board_to_screen()` 换算；
   士兵坐标本身来自 `game_state.gd`，取值区间是 `|x| ≤ 25`、`|y| ≤ 15`。
2. `play_screen.gd:460` —— 拿到 `pts` 后**不做任何变换**就 `_plane.set_path(pts, impact, true)`。
3. `client/src/play/plane_view.gd:46-49` —— `board_to_screen(p)` 才做
   `Vector2(size.x*0.5 + p.x*s, size.y*0.5 - p.y*s)`；`s = min(w/50, h/30)`。
   **换算发生在绘制层，不在内核返回值上。**

反面证据（写错了会怎样）：如果内核返回 `px = 20x + 500`、`py = 20y + 300` 的像素坐标，
`board_to_screen()` 会再乘一次 scale 并加一次中心偏移 ⇒ 弹道整条飞出画布。
`05-shot-pipeline.md` §5 也早写过"否则会把像素坐标和游戏单位混在一起"。

实测旁证：`GwCore.calculate_path("sin(x)", Vector2(5,2), 1.0)` 的 `points[0]` 就是 `(5.0, 2.0)`，
点数 2471，弦长 0.0090–0.0105 —— 与 `src/graphwar_path_calculator.rs` 单侧的值一模一样（§7.3）。

**顺带一条界面侧的观察（不是 bug，是接上内核后数字会变）**：
`_preview()` 的修正常数恒用 `c = y0 − f(x0)`（不问方向），而内核用 `c = y0 − f(±x0)`。
所以 team 1（向左）开火时，`OffsetLabel` 从今天起显示的数会和以前不同 —— 内核那个才是对的
（`05-shot-pipeline.md` §2、`10-campaign.md`）。这条属于**预期变化**，不是回归。

---


#### 3.1.2 错误码


| `error` | 触发条件 | 档位 |
|---|---|---|
| `""`（空串） | 成功 | 逐位验证 |
| `start_not_finite` | `x0`/`y0` 是 `NaN`/`±inf` | 逐位验证 |
| `start_out_of_bounds` | `|x0| > 25` 或 `|y0| > 15` | **推断**（见下） |
| `empty_expression` | 词法切不出任何 token（`"@@@"`、`"   "`） | 逐位验证 |
| `undefined_at_start` | 修正常数不是有限数（`sqrt(x)` 在 `x=−1`、`ln(x)` 在 `x=−2`） | 逐位验证 |
| `no_points` | 首点之后再没有被接受的点（越界 / NaN / 100 次二分不收敛） | 逐位验证 |
| `bad_arguments` | 参数个数/类型不对（实测**到不了**这里，见 §3.7） | 观测 |
| `internal_panic` | `catch_unwind` 兜底 | 推断（未触发过；不想触发） |

三条要写清楚的判断：

1. **起点越界**为什么算错误：采样器把 `(x0, y0)` **无条件**塞成首点（`graphwar_path_calculator.rs` 第一行），
   再做逐点边界检查 —— 起点越界时整条曲线在平面外，界面上会画到战场外面去。
   `AGENTS.md` 的硬事实是"越界点在**接受之前**检查"。所以这里在入口先拒。
   档位：**推断**（真二进制在起点越界时是否也拒，未观测；客户端预览分支同样是先判 `|x| ≤ 25`）。
2. **空表达式**：原版**能画** —— 抓包里 `{"function":""}` 的修正文本是 `" + 10.990"`，
   也就是 `f(x) = 0` 的水平直线（`05-shot-pipeline.md` §2）。但既有的 `graphwar_shot_calculator::shot_points` 与 C ABI
   `gw2_shot_points` 都按"空 = 没有点"处理，且客户端在调内核**之前**就把空串拦下了
   （`GwCore.calculate_path` 先 `GwExpr.compile`）。本次保持与既有内核一致：报错。
   **这是一处已知的、有意保留的偏差**，要不要改成"空串 ⇒ 水平直线"见 §8.4。
3. **不收敛**：`graphwar_path_calculator::calculate` 在第 100 次二分仍失败时直接 return，不区分原因；本层也不猜，
   统一归到 `no_points`（只在"一段都没走成"时触发）。半路停下的情况仍然是 `ok: true`，
   只是点少 —— 与真二进制一致（真值夹具里就有这种短曲线）。

---


## 4. 命中判定接进客户端：`GW2Rebuild.hit_test`

### 4.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新方法 | `GW2Rebuild.hit_test(points, obstacles, soldiers) -> Dictionary` | 逐位验证（`get_method_list` 与“注册成功”日志，§6.3） |
| 返回 | `{ ok, hits, hit_points, first_hit, stop, blocked, error }`（顺序固定） | 逐位验证（单测 `result_shape_is_exactly_the_contract`） |
| 坐标系 | 三个入参全是**棋盘坐标**（`|x|≤25`、`|y|≤15`）；掩膜是**像素**空间，换算只在 `plane::to_pixel` 里做一次 | 逐位验证（§2 三条测试） |
| 用的是不是真规则 | 是。判定本体是 `graphwar_shot_calculator::settle::resolve_points`，`graphwar_shot_calculator::settle::resolve` 也走它 —— **只有一份实现** | 逐位验证（等价性测试 `agrees_with_settle_resolve_on_captured_shots`） |
| `cargo test --offline` | **242 → 258**（+16，全绿） | — |
| `bash client/run.sh selftest` | **93 passed, 0 failed**（内核开 / 关各跑一遍；**未改任何断言**） | — |
| 客户端改动 | `core_bridge.gd` +102/−6，`play_screen.gd` +27/−12（两处函数体，无布局/文案/节点树改动） | — |
| 内核改动 | `src/_scaffold/api.rs`（新增）、`src/graphwar_shot_calculator/settle.rs`（新增一个函数 + `resolve` 改为委托）、`src/_scaffold/gdext.rs`（**+1 行**注册） | — |
| 客户端状态行 | `本轮命中 N 个士兵 ／ 判定来源：kernel|preview ／ <note>` | 观测（§6.2 原文） |
| 仍未确定 | 士兵半径常量的二进制出处、友军伤害、射手免疫的直接证据、同时回合顺序（§7） | — |

三条最容易出错的点，先写在前面：

1. **坐标只换一次**：进参是棋盘坐标、掩膜是像素坐标（`px = 20x+500`、`py = 20y+300`）。
   `hit_test` 里**不做**任何换算，换算在 `plane::is_occupied` / `plane::rasterize` 内部。
   在外面再翻一次就是整体偏 500/300。
2. **障碍与士兵不是一回事**：障碍进掩膜、让弹道**停下**；士兵**不**进掩膜，只在
   "弹道点落在半径内"时判死（`15-target-life.md` 纠正过的建模）。所以 `first_hit` 说的是
   "弹道停在哪一点"，与"谁死"是两件事。
3. **手写 FFI 读 `Packed*Array` 别用 `variant_get_ptr_indexed_getter`**：4.7.2 上实测
   读出来是垃圾（§3.1）。正确做法是 `packed_*_array_operator_index_const`。

---


## 5. 视觉保真：启动画面 / 背景 / 图标（原 121）

### 5.1 一页结论

| # | 问题 | 根因（一句话） | 状态 |
|---|---|---|---|
| ① | 开头出现"Godot 启动画面" | `boot_splash/show_image=false` **只关掉了 logo 图**；引擎仍从建窗到首帧铺一块 `boot_splash/bg_color` 的纯色矩形，而这个键我们**从没写过** → 用的是 Godot 默认 `#242424` 中性灰，闪了 ~1.5 s | ✅ 修好（逐帧验证） |
| ② | 背景是灰色，原版深绿 | `default_clear_color` 写的是我们当初自选的中性灰蓝 `(0.086,0.094,0.11)`；原版是 **`(0.054902,0.145098,0.109804)` = `#0E251C`**，而且那层深绿**确实就是 clear_color**（主菜单场景里没有任何 `ColorRect`/`Panel` 画底）。顺带：按钮/面板的绿色也照原版主题改了 | ✅ 修好（逐位 + 截图） |
| ③ | 大量图标缺失 | `client/` 里从来没有原版图标，`icons.gd` 是**运行时用 SVG 字符串现画**的同语义几何图；观感与原版无关，用户认成"缺图标" | ✅ 改成"从使用者自己的 pck 解出真资产"；**仓库不再含任何自绘图标** |

配套的三条硬规矩（本轮新增，见 §5）：

1. **仓库里没有任何媒体文件**（`.svg/.ttf/.png/.tres/.tscn/.gd/.pck` 一律不入库）。
2. 资产由 `tools/extract_assets.py` 从**使用者自己的 `Graphwar.pck`** 解到 `client/assets/`（gitignore）。
3. 没有资产时**明确报错并给出提取命令**；selftest 允许跑，但**显式跳过并报告**，不假装通过。

---


## 6. 像素级保真：给没有真值的屏造真值（原 137）

### 6.1 一页结论

| # | 屏 | 真值来源 | 改前 | 改后 | 改后 tol>32 |
|---|---|---|---|---|---|
| 1 | MainMenu | 正版二进制 | 0.000% | **0.000%** | 0.000% |
| 2 | Campaign | 正版二进制 | 0.201% | **0.179%** | 0.000% |
| 3 | LanLobby / Local | 正版二进制 | 0.029% | **0.007%** | 0.000% |
| 4 | **PlayerScreen / Skins** | 正版二进制 | **0.490%** | **0.007%** | 0.000% |
| 5 | **PlayerScreen / Faces** | 正版二进制 | **0.977%** | **0.572%** | 0.190% |
| 6 | **PlayerScreen / Hats** | 正版二进制 | **2.282%** | **0.345%** | 0.112% |
| 7 | ReplayLobby | 正版二进制 | 0.384% | **0.362%** | 0.313% |
| 8 | **GameSetup** | **宿主整树（本轮新建）** | **无真值** | **1.330%**（其中 0.592% 是静态场景里不存在的玩家条目） | 0.462% |
| 9 | ConfirmPlayExitPopUp（面板区） | 宿主工程 | 0.042% | **0.042%** | — |
| 10 | JoinIpPopUp（面板区） | 宿主工程 | 0.033% | **0.032%** | — |
| 11 | ConnectingPopUp（面板区） | 宿主工程 | 0.000% | **0.000%** | — |
| 12 | DisconnectedPopUp（面板区） | 宿主工程 | 0.000% | **0.000%** | — |
| 13 | **AddBotPopUp（面板区）** | 宿主工程 | **1.676%** | **0.016%** | — |
| 14 | EndGamePopUp（面板区） | 宿主工程 | 9.604% | 9.797%（**填满后没有真值**，见 §7.3） | — |
| 15 | 返回键矩形（所有屏） | 宿主工程 + 正版二进制 | 4.616%（647 px） | **1.084%**（150 px） | — |

**三句话**：

* 改前那 0.49% **不是抗锯齿玄学**，是两件具体的事：**工程少写了两个渲染键**（`default_texture_filter`
  与 `msaa_2d`，各占一半量级），以及**返回键把同一个箭头画了两遍**（§3.1/§3.2）；
* `GameSetup` 现在**有真值了**（宿主整树模式），13.048% → 1.330%，其中真正能验收的部分是 **0.738%**；
* 默认形象在**当前 HEAD 上是对的**（黄圆 + 绿盔 + regular eyes，与真帧逐像素 0.007%）——
  用户看到"不对"的两个可复现来源是**自检写脏了 `gw2_client.cfg`** 与**没读原版 `player.config`**，两条都已处理（§6）。

---


## 7. 几何、主题与文案（原 124 / 129 / 130 / 145）

### 7.1 一页结论

| # | 问题（用户截图） | 根因 | 修法 | 证据 |
|---|---|---|---|---|
| ① | 标题/按钮/字号整体大一圈 | `project.godot` 写 1280×720 + `stretch/mode="canvas_items"` + `aspect="expand"`；1920 宽窗口 ⇒ 全 UI 放大 **1.5×** | 视口改 **1600×900**、**删掉 stretch 两键**（原版 `project.binary` 里没有任何 `stretch/*`）、`window/size/mode=2` | 逐位（截图 sha256 相同，见 §2.1） |
| ② | 右下角版本显示成 `client 0.1.0 · kernel on` | 我们自己往 `VersionLabel` 里塞了调试文字 | 照原版语义显示 `ProjectSettings.application/config/version` = **`2.0.60`**；内核状态改走 `dump` 通道 + `-- --debug-overlay` 才显示 | 观测（原版截图右下角只有 `2.0.60`） |
| ③ | 底部多一条 `advertisement slot — disabled in this build` | 我们自造的字 + 钉死 60 px 高度 | `AdContainer` 只留节点：**0 高、0 文字**（原版 `AdContainer.gd` 把高度设成 `ad_size`，初值 (0,0)） | 观测（`AdContainer.gd`/`Ad_holder.gd` 逐行可读 + 原版截图无此条） |
| ④ | Campaign 数字是"灰色圆角底 + 粗黑体" | 我们用了自己的按钮主题；原版 `campaign_button_theme_2.tres` 五态**全是 StyleBoxEmpty**、字体是手写体 `Eraser` | 逐属性照抄那份主题（`GwThemeFactory.campaign_level_theme()`） | 逐位（白字掩膜 IoU 0.997@阈值60，见 §3.3） |
| ⑤ | 存档不生效（只有第 0 关亮） | 我们没读原版 `user://campaign.save` | 实现 `level_<n>` 严格解析、单调写、不自动创建 | 观测 + 逐位（用正版二进制做了 2 组存档实验，见 §3.2） |

---

### 7.2 一页结论

| # | 现象 | 根因（全部有真帧/`.tres` 证据） | 改后 |
|---|---|---|---|
| ① | 格子只有一圈 **~35 px 的深绿小点**，白卡完全没有 | 白卡不是 `Panel`，是 **`shop_theme.tres` 的 `Button/styles/normal`**：`bg=#FFFFFF` + 3 px 黑边 + 圆角 10。原版把这份主题挂在下拉框里那层 `VBoxContainer` 上，Skins 页 15 个 `GraphwarButton` 直接继承它 | 逐像素对齐（§3） |
| ② | 格子比原版小 3 倍 | 96 px 来自 `ButtonGrid.gd`（**别的控件**用的公式）。玩家页挂的是 **`ShopButtonsSize.gd`**：`(安全区宽/2 − 50)/3` ⇒ 1920 下 **303** | 同上 |
| ③ | 格内图标太小 | `skins/*.ctex` 是 512² 画布，实心圆只占 **240 px（46.875%）**；`expand_icon` 撑满内容区 297 ⇒ 圆 **139 px**（真帧实测 139） | 同上 |
| ④ | 右列被挤出屏幕（NAME 输入框跑到 x=1307） | 我们给 TabContainer 写了 `stretch_ratio = 2.0`；原版两边**都是** `size_flags_horizontal=3` ⇒ ≈50/50（真帧 TabContainer 959 / 右列 957） | 同上 |
| ⑤ | 右列下半屏多出 `Support the game / Unlocking skins… / Unlock` | 原版桌面端 **SkinBlock 与 PendingBlock 都不显示**（真帧那一块是纯底色 `#0E251C`）；`PendingBlock` 在 `.tscn` 里就写着 `visible = false` | 节点保留、`visible=false` |
| ⑥ | 三个页签看起来一模一样 | `tab_unselected` 我们写成了全透明 + 自选灰色；原版是 `tab_unselected.tres`（`#1A3B2D` + 上/左/右 3 px 黑边 + 半径 10），且**没有**覆盖 `font_unselected_color`（用 Godot 默认白@0.7 = 实测 `(178,178,178)`） | 逐像素（§3.2） |
| ⑦ | 三页签格数被写成 15/15/27（对）但**本轮差点改成 24**（错） | 用**滚动条 grabber 长度比**定案：`grabber/轨道 = 可视高/内容高`。原版 Skins/Faces = 578 px、Hats = **324 px**，反解出 **5 / 5 / 9 行** ⇒ **15 / 15 / 27** 格 | 15 / 15 / **27**（§2.1） |
| ⑧ | LanLobby ` Join IP ` 宽 280 vs 原版 378、表头左移 260 px | 表头比例写错（3:1:2，原版 1:**0.1**:1）+ 左对齐 + 自选灰色；底部按钮**没有** `LobbyButtonContainer.gd` 的"每个子节点 = 屏宽×0.2 = 384" | 4.31% → **0.03%** |
| ⑨ | LanLobby Global：Players 是 `--`、模式块是 4 个暗色小方块、底部泄漏内部文档 | Players 是 `0`；四块要按 `game_room_entry.tscn` 的 `AspectRatioContainer + expand_icon` 画，且**第一块白底、其余三块深绿底**（两份不同 `.tres`）；泄漏那句在 `app.gd` 的 `notify()` 里 | 12.50% → **2.70%** |
| ⑩ | ReplayLobby 空列表多两行提示 | 原版空列表**什么都不画**（真帧只有返回键 + `REPLAYS`） | 2.44% → **0.38%** |

---

### 7.3 一页结论

| # | 弹窗 | 改前 | 改后 | 主要错因 |
|---|---|---|---|---|
| 1 | ConfirmPlayExitPopUp | 面板区 **56.717%** | **0.042%** | 面板按"内容最小尺寸 560×220 居中"；文案/按钮全用默认左对齐；返回键塞在面板里 |
| 2 | JoinIpPopUp | **77.373%** | **0.033%** | 面板 560×240，不是原版的 1:6:1（1434×383）；多画了一个 `host:port` 占位符 |
| 3 | ConnectingPopUp | **77.188%** | **0.000%** | 同上（这一屏现在是**逐像素完全相同**） |
| 4 | DisconnectedPopUp | **73.465%** | **0.000%** | 同上（**逐像素完全相同**） |
| 5 | AddBotPopUp | **78.097%** | **1.676%** | 自造了 "ADD BOT" 标题；格子尺寸钉死 96/120；面板/三段带比例不对 |
| 6 | EndGamePopUp | **62.586%** | **9.604%** | 多了 AdContainer（比例本该 1:3:1）；两个 Label 该是静态 70/40 不是 宽/30；根节点该有 +1 px 偏移 |
| 7 | ChatPopUp | —（见 §4.3） | — | 我们画的是 640×360 小对话框，原版是**整屏浮层**（96 / 62 / 994 三段） |

> 百分比口径：**"面板区"** = 真值帧里面板外框的矩形（例如 confirm 是 (482,386)–(1437,768)），
> 在真值帧与我们的帧上**同一块矩形**逐像素比，值 = 有差异的像素占比。
> 全帧差异没有意义：我们的截图是把弹窗盖在 PlayScreen/LanLobby/GameSetup 上，
> 真值是盖在原版纯底色 `#0E251C` 上，两者的**底图**本来就不同（全帧差 48%–90% 全是底图）。

**返回按钮（最重要的一条）**：
**七个弹窗全都有返回按钮，位置恒为左上角 `(0,0)`、尺寸 146×96（1920 宽时），无条件显示。**
用户"截图里看不到"是因为它就在**屏幕左上角**、和下面那一屏自己的返回键**矩形完全重合**
（同 146×96、同箭头）——它看起来像"屏幕的一部分"而不是"弹窗的一部分"；
而我们原来把它塞进了面板内容区的左下角，位置完全不对。详见 §2。

---

### 7.4 一页结论

| # | 现象 | 根因（一句话） | 改后 |
|---|---|---|---|
| A | Faces / Hats 两页整页同一个**旧颜色**（用户选 yellow，页面却是打开界面那一刻的皮肤） | 皮肤层图标在 `build()` 里**一次性**写死；原版是**运行时按 `profile.skin` 合成**，换皮肤立刻重画 | 选皮肤/进界面都重画 42 个皮肤层；顺带发现并修掉"皮肤画了两遍" |
| B1 | JoinLevelPopUp **"下面的部分太长"** | 底部那条 96 px 的留白带**整条缺失** ⇒ 面板带吃掉它那份，面板高 1049（真值 **956**），底边压到 y=1152 | 1:10:1 三条竖带补齐，面板 (243,100) **1434×956** |
| B2 | 坐标模式的九宫格图标**底不是白色** | 四个模式按钮都没给样式盒 ⇒ 落在默认深绿上；原版 `AxisModeButton` 走 `axis_button_theme.tres` = **白底** | 白底 + 3px 黑边（另三个仍 `#1A3B2D`） |
| B3 | （改 B1 时量出来的）Cancel / Start 间距 28 px（真值 **24**） | `HBoxContainer3` 写了 `separation = 8`；原版没写 ⇒ 默认 4 | 4（视觉间距 = 4+10+10 = 24） |

**逐像素**（1920×1160，与**正版 `Graphwar.x86_64` 真帧**比，"任一通道不同"的像素占比）：

| 屏 | 改前 | 改后 | 改后 tol>8 |
|---|---|---|---|
| PlayerScreen / Faces（默认 yellow） | 0.572% | **0.060%** | 0.049% |
| PlayerScreen / Faces（**在 Skins 页点了 red 之后**） | **5.977%** | **0.060%** | 0.049% |
| PlayerScreen / Hats（默认） | 0.345% | **0.060%** | 0.049% |
| PlayerScreen / Skins（默认，**不许回归**） | 0.007% | **0.007%** | 0.005% |
| PlayerScreen / Skins（点了 red 之后） | 0.169% | 0.169% | 0.167% |
| GameSetup 背景上的 JoinLevelPopUp（整帧） | 12.921% | **0.108%** | 0.096% |
| —— 其中：四个模式格子 / Cancel+Start / 底部带 | 20.7% / 16.2% / 71.7% | **0.000% / 0.000% / 0.000%** | 0 |

剩下的 0.060% / 0.108% 全是**字形与圆角的抗锯齿灰阶**（见 §3.3），没有一处是"位置错了"或"少画了东西"。

---


#### 7.4.1 Faces/Hats 动态合成底色 + JoinLevelPopUp 比例（原 145）

### 7.5 A：Faces / Hats 的底色 = **当前皮肤**（动态合成）

#### 7.5.1 真帧证据（**观测**，正版二进制）

做法：1920×1160 的 Xvfb 里跑正版 `Graphwar.x86_64`，用 `xdotool` 点坐标，
`import -window root` 截根窗口（同一套环境在 `137` §1.4 已用主菜单 sha256 自证过等价性）：

```
click 536 749        # 主菜单 CharacterButton → PlayerScreen
shot  faces_default  # 点 Faces 页签 (470,150)
click 150 150        # 回 Skins 页
click 168 673        # 第 2 行第 1 格 = red 皮肤（SKIN_GRID[3]）
click 470 150        # 再进 Faces 页
shot  faces_red
```

量第一个格子圆心 `(168,366)` 的像素：

| 帧 | 15 格的圆心 |
|---|---|
| `faces_default`（存档默认 = yellow） | **(255,204,0)** —— 全部 15 格都是黄的 |
| `faces_red`（在 Skins 页点了 red 之后） | **(255,114,114)** —— 全部 15 格都变红 |
| `hats_default` | (255,204,0) |

**结论：Faces / Hats 的格子 = 「当前 `profile.skin` 的皮肤圆 + 该格的脸/帽子图标」**，
是**运行时合成**。⇒ 用户看到"原版整页都是黄色圆"完全正确，这不是"皮肤图"，是**合成**。

#### 7.5.2 合成规则从哪读出来（**观测**，pck 里的 `.tscn`）

`shop_button_entry.tscn`（原版 pck 里的场景，`tools/pck_extract.py` 可重新解出；Faces 15 份 +
Hats 27 份都是它的实例）只有 5 个节点，逐字段可读：

```
Control                        (满屏锚点、mouse_filter=1)
├ GraphwarButton               根：theme=long_press_ghost_button_theme、clip_children=2
│   │                          ⚠️ **没有 icon= 这一行**（皮肤不是它画的）
│   ├ ColorRect                show_behind_parent=true、**没写 color** ⇒ 默认白
│   └ SkinBackgroundButton     theme=Theme_b8swt、icon=**skins/yellow.svg**、
│                              icon_alignment=1、expand_icon=true     ← 皮肤就在这一层
└ Button                       theme=Theme_p63gr（透明底 + 3px 黑边）
                               ⚠️ 静态也没有 icon / expand_icon（脸/帽子由运行时填）
```

三条要点，都能指到文件：

1. **皮肤只有一层**（`SkinBackgroundButton`）。根 `GraphwarButton` 与 `Button` 的图标都是空的；
2. `.tscn` 里的 `icon = skins/yellow.svg` 只是**静态初值**——真帧证明运行时会换成 `profile.skin`
   （§1.1）。`player.config` 是**逐键兜底、值是名字**（`137` §5.2 做过三组二进制实验），
   默认三件套 = `yellow + regular_eyes + soldier_helmet`；
3. 白卡来自那个**默认就是白色**的 `ColorRect`，不是 stylebox。

#### 7.5.3 我们错在哪（**观测**：我们自己的帧）

我们在 `player_screen.gd::_fill_entry_grid()` 里：

```gdscript
var skin_layer := _overlay("SkinBackgroundButton", GwIcons.get_icon("skin_%d" % _default_skin_index()))
```

—— 皮肤下标在 `build()` 那一刻求值一次，之后就**没有任何代码再碰它**。
`build()` 在 `_ready()` 里跑（所有界面启动时一次建好），所以：

* 用户在 Skins 页选 yellow ⇒ `profile.skin` 改了、右侧预览改了，**但 Faces/Hats 的 42 个格子不动**；
* 于是"整页都是打开界面那一刻的颜色"。用户描述成"固定的紫/蓝"，我们的复现是
  "先红后黄"——**同一件事**：格子不跟着当前皮肤走。
* 另一个真实来源：历史版本的自检把 `skin=0` 写进存档（`137` §5.5），
  那之后打开玩家页，Faces/Hats 就是**整页蓝**，而 Skins 页第 1 格也是蓝 ⇒ 很容易读成"固定图"。

同时还有**第二个错**：白卡和皮肤层**都**被写了皮肤图标（`_cell_button(..., skin_icon)` +
`_overlay("SkinBackgroundButton", skin_icon)`）⇒ **同一个圆画了两遍**，边缘 alpha 合成两次。
`137` §3.4 记过 Faces 有"格子内那层皮肤圆的 1 px 边缘环"残差（0.572%），
**那就是这一条**——本轮去掉白卡的图标之后，Faces 默认帧从 0.572% 直接掉到 **0.060%**。

#### 7.5.4 修法（`client/src/screens/player_screen.gd`）

1. 新增 `_entry_skin_layers: Array[GwButton]`，`_fill_entry_grid()` 建格子时把
   `SkinBackgroundButton` 收进来；
2. 新增 `_apply_skin_to_entries(skin_index)`：把 42 层的 `icon` 全部换成
   `GwIcons.get_icon("skin_%d" % skin_index)`；
3. 触发点两处：`_on_skin_selected()`（点了皮肤**立刻**重画）与 `on_enter()`（进界面再同步一次，
   覆盖"别处改了存档"的情况）；
4. `_default_skin_index()` 改名 `_current_skin_index()` 并写清语义（读存档、不是常量 7）；
5. 白卡层**不再带图标**（`_cell_button("GraphwarButton", null)`），皮肤只由
   `SkinBackgroundButton` 画一层。

`Skins` 页**没动**：那一页每格就是"皮肤本身"（真帧 `faces_red` 那一帧里 Skins 页的 15 格颜色
一格都没变，见 §1.1 的量法），所以它不该跟着 `profile.skin` 走——改后 0.007% → 0.007%，没有回归。

---


## 8. 关卡障碍：`GW2Rebuild.level_obstacles` + 绘制 + 参与命中（原 126）

### 8.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新内核方法 | `GW2Rebuild.level_obstacles(level_index: int, seed: int) -> Dictionary{ok, obstacles, error}` | 逐位验证（§5.3 的注册日志 + GDScript 实调成功） |
| 生成本体 | **复用** `src/graphwar_level_generator.rs` 既有实现（`generate` 13/13 逐位、`generate_center`），本文件没有第二份公式 | 逐位验证（单测 `agrees_with_levelgen_entry_point_bit_for_bit` / `seeded_entry_reuses_the_verified_generators`） |
| 21 关参数表 | `src/graphwar_level_generator::LEVEL_OBSTACLE_PARAMS`，逐条抄自 `10-campaign.md` §7.1（每行注释带该关的闭包预设地址） | 逐位验证 + 运行时（`10-campaign.md` 原文的 `C`+`R` 双源）；本文再加一条"照文档重抄一遍"的测试 |
| **障碍真的画出来了** | 是。黑色实心圆，半径 = 生成器给的 `r`（棋盘单位），逐像素与原版真帧对齐（§4） | 观测（截图 + 像素取样） |
| **子弹真的被墙挡住** | 是。端到端手测：`x/10` 在 `first_hit=4480` 撞墙，`stop=(24.00, −0.90)`，画出来的点序列 = `4480+1` 点（§5.4） | 观测 + 逐位验证（单测 `generated_obstacles_actually_block_a_shot`） |
| `cargo test --offline` | **272 → 287**（+15，全绿） | — |
| `bash client/run.sh selftest` | **146 passed, 0 failed**（条数不变，**未改任何断言**） | — |
| `bash client/run.sh dump` | `@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…` | 逐位验证 |
| 内核改动 | `src/graphwar_level_generator.rs`（参数表 + 入口）、`src/_scaffold/api.rs`（新方法）、`src/_scaffold/gdext.rs`（**+1 行**注册） | — |
| 客户端改动 | `core_bridge.gd`（+52/−0）、`play_screen.gd`（+92/−7）、`plane_view.gd`（+51/−2）、`join_level_pop_up.gd`（+14/−0，父 agent 追加的弹窗配色） | — |
| 仍未定 | **客户端自生成 vs 服务端广播**的取舍（§6.1）、障碍被摧毁的洞、我方士兵/靶子位置、三条"用户口述"的开火表现（§6.3） | — |

一句话：**墙有了、画出来了、也真的挡子弹了；"每局布局不同"这条原版行为也复刻了
—— 但"谁决定这一局的布局"这件事，最终应当由服务端说了算，现在是客户端在代打。**

---


## 9. 音频播放层（原 128）

### 9.1 一页结论

| 项 | 结果 | 等级 |
|---|---|---|
| 播放层 | `client/src/audio/gw_audio.gd`（`GwAudio`，`App` 的子节点，`App.audio`）；10 个音效全部真的能出声 | **观测**（捕获到非零 PCM） |
| 资产 | 包里的 `.mp3str` / `.sample` 在本机还原成 `client/assets/sounds/*.mp3|*.wav`；**一个媒体文件都不入库** | **观测**（`git check-ignore` 命中） |
| 音量 | 逐 player 的静态常量：`0x80/0x90/0xa0 = -15.0 dB`、`0xc0 = -5.0 dB`、`0xb0` 未设置（默认 0 dB） | **观测**（`17-resources.md` §4.5 + 本轮实测峰值比 0.1778 = 10^(-15/20)） |
| 静音 | `AudioServer.set_bus_mute(get_bus_index("Master"), …)`，**不是**逐 player 调音量；`store.muted` ↔ 总线**双向**同步 | **观测**（`0x4c13a4/0x4c13c7`）+ 实测总线峰值 -200 dB |
| 事件↔音效映射 | **0 条确定**。本轮把"未知"缩小了一步（见 §3），但仍证不到"哪个事件放哪个音" | **未知** |
| 客户端接线 | 3 条**临时的**、代码里带 `PENDING_MAPPING` 标记、文档里标"**自定，非原版**" | **自定**（不是原版行为） |
| 自检 | **181 passed, 0 failed**（基线 146 + 新增 35）；无音频资产时 **175 passed, 0 failed, 10 skipped**（逐条报原因） | **观测** |
| 端到端 | 真开一局 → 按 `Fire!` → `sfx_played=["explosion@0xa0"]`，抓到 32768 帧、峰值 0.20081，附 WAV/PNG 证据 | **观测** |

**一句话**：播放链路（含 `humm` 的 `AudioStreamGenerator` 那条路）已经通了、静音走的是原版的
Master 总线、音量用的是原版实测值；**唯一没解决的是原版最核心的那件事 —— 哪个事件放哪个音**，
而这一点 `17-resources.md` §5.1 本来就判了未知，本轮**没有**编一个出来。

---


## 10. 回放：文件格式与播放器（原 78 / 133）

### 10.1 存在哪

二进制里写死了两条路径（`strings libgraphwarrust.so | grep replay` 可见）：

```
user://replays/                    ← 对局录像
user://graphwar_media/level_replay ← 关卡录像（另一个东西）
```

对应 Rust 模块：`src/graphwar_replay_control.rs`、`src/graphwar_replay_player.rs`；
UI 用 `res://replay_lobby.tscn` / `res://replay_entry.tscn`。

Godot 4 里 `user://` 映射到 `$XDG_DATA_HOME/godot/app_userdata/<项目名>/`
（默认 `XDG_DATA_HOME` = `~/.local/share`），所以：

```
~/.local/share/godot/app_userdata/<项目名>/replays/<unix 时间戳>
```

**本机实测路径**：

| 路径 | 来源 |
|---|---|
| `/home/user/.local/share/godot/app_userdata/Graphwar/replays/` | **真实对局**（本机已有 **375 个文件**） |
| `gw2/.godot-data/godot/app_userdata/gw2-live/replays/` | 我们 harness 自己跑的 |

文件名就是 Unix 时间戳，没有扩展名。

### 10.2 一页结论

| 问题 | 结论 | 级别 |
|---|---|---|
| 回放文件长什么样 | gzip + JSONL，每行 `{"event":{"<事件名>":{…}},"tick":N}` | **观测**（400 份实文件） |
| 事件名/字段名和网络协议是同一套吗 | 是。解析直接复用 `src/_scaffold/protocol.rs` 的 `parse_message` 与 `get_*` | **观测 + 逐位验证** |
| 一局有多少事件 | 真实语料 400 份、**103 104** 条；单局 2 … 1471 条（另有 2 份空回放） | **观测** |
| 播放按什么推进 | **按 tick**（不是按墙钟/按事件）。60 tick/秒 | **观测**（`14-turn.md`）+ 我们自定的取整规则 |
| 回放的弹道点序列在哪 | **没有**。只有 `FunctionPoints{start_x,start_y,end_x}` | **观测**（400 份里 0 例带点序列） |
| 那弹道怎么画 | 用内核重算：`GwCore.calculate_path(函数文本, 起点, 队伍方向, "ode"/"normal")`，`mode` **取自这一局的 `FunctionModeInfo`** | 我们的做法（走内核，不绕过） |
| `DiffEqFunction` 局（320/400）的弹道 | 走 **`Mode::Ode`**（RK4）⇒ 能画了。内核钩子是 `GW2Rebuild.path_calculate_mode(…, mode: String)`，取 `"normal"` / `"ode"`（`05-shot-pipeline.md` 交付，已进 main）；`path_calculate` 的签名一字未动 | **逐位验证**（§5.3 第 7 组、§5.6：4 发 `end_x` 全中） |
| 回放界面长什么样 | **未知**（只有场景名 `replay_lobby.tscn` / `replay_entry.tscn`） | **未知** ⇒ 播放控件我们自定 |
| 播放/暂停/进度 | **我们自定**（原版无观测） | 自定，显式标注 |
| 能播了吗 | 能。真实回放读、解析、按 tick 播、死亡事件生效、弹道由内核重算 | **逐位验证**（§5） |

**新增文件**：`src/graphwar_replay_player.rs`（格式/局面/播放器/对拍 ABI）、`src/_scaffold/replay_inflate.rs`
（纯 Rust gzip+DEFLATE）、`client/src/play/replay_file.gd`（客户端读文件）、
`client/src/play/replay_player.gd`（tick 播放器 + 局面还原）、`16-client.md`（本文）。
**改动文件**（都是"只加"或删占位，没有改既有语义）：`client/src/screens/play_screen.gd`
（回放分支 + 一条隐藏进度行 + **按录像的 `FunctionModeInfo` 选 `normal`/`ode`**）、
`client/src/screens/replay_lobby.gd`（列表数据源）、
`client/src/app/app.gd`（**只删**两处"回放未实现"占位）、
`src/lib.rs`（**两行** `pub mod`）、`client/tools/selftest.gd`（只新增断言）。
微分方程模式的**内核侧**（`src/_scaffold/api.rs` / `src/_scaffold/gdext.rs` / `core_bridge.gd`）由另一条分支
`agent/ode-api` 交付、已进 main，见 `05-shot-pipeline.md` —— 本文只负责"回放里怎么用它"，
**不重复实现**。

---


## 11. 联机层：真的能连服务器（原 134 / 139）

### 11.1 一页结论

| 问题 | 答案 |
|---|---|
| 现在客户端能连服务器吗 | **能**。TCP 真的连、`u16` 大端帧真的收发、7 条状态同步真的解析 |
| `ConnectingPopUp` 还是模拟的吗 | **不是**。`await create_timer(1.2)` 那条假链路已删除；成败都由 socket 事件决定 |
| 连的是谁 | 本轮验证的是**我们自己的服务端**（`examples/serve_match`），不是原版服务器 |
| broker 用上了吗 | 用上了（`Global` 页）。但地址**默认空**：原版默认从 `graphwar.com` 取，本项目不发外网请求 |
| LAN 房间列表真的能用吗 | **能收**（UDP 42968，裸 9 字段 JSON + 换行）。**我们没有广播循环**（原版在 `graphwar_lan_broadcaster_v6.rs`，`src/` 本轮不动） |
| 进去之后能打完一局吗 | **协议层能**：客户端提交函数 + 开火，服务端广播 `FunctionPoints` 等结算报文（§6 有实测）。**界面层不能**：PlayScreen 还按本地规则画（§7） |
| 断线/失败真的走弹窗吗 | **是**。连不上、握手超时、对端关闭三种都落到 `DisconnectedPopUp`（§4.2） |
| 数字 | 客户端自检 **245 → 393**（+148，0 失败 0 跳过）；`cargo test --offline` **296**（未改 Rust） |

---

### 11.2 一页结论

| 问题 | 答案 |
|---|---|
| 联机战场现在是服务端摆的吗 | **是**。士兵 = `room.entities` 里 `kind=="soldier"` 的实体（坐标/队伍/生出来自 `PosInfo`/`TeamInfo`/`LifeInfo`），障碍 = `room.obstacles`（`AddObstacle` 的顺序）。`_spawn_position()` 与内核关卡生成器在联机分支里**不再被调用** |
| 开火还是本地结算吗 | **不是**。走 `session.submit_function(pid,text)` + `session.fire(pid)`，本地不再算命中、不再换手、不再判胜负（那三件事都在服务端） |
| 弹道哪来的 | 服务端只发 `FunctionPoints{start_x,start_y,end_x}`（**没有点序列**）。客户端用**内核**按"函数原文 + 起点 + 方向"复算，方向由 `end_x` 落在起点哪一侧定，画到 `end_x` 为止 |
| 士兵死亡会消失吗 | **会**。`LifeInfo{alive:false}` 与 `LifeQueuedDeath` 都收；联机下阵亡者从绘制列表里去掉（服务端也在 `Match::fire` 里 retain 掉） |
| "我是谁"解决了吗 | **解决了**。服务端补了 `PlayerInfo{player_id,connection_id}`（抓包原文形状，逐字节测试）；拿不到时降级成"按 `TurnInfo` 的玩家假定"并在状态行写明 |
| 顺手修了什么 | **一个真 bug**：`FunctionUpdateRequest` 只写了玩家**第一条** `PlayerState`，于是"谁开火读谁的函数"永远读到默认值 `"0.00"` —— 客户端提交的函数从没真正生效过（以前的端到端只提交 `"0.00"`，看不出来）。**台账见 §10** |
| 三种老模式还好吗 | **好**。回放分支、`_fire_fx` 的 4 处落点、战役/LAN 的本地规则一个字没动；自检 478 条既有断言**全部照过** |
| 数字 | `cargo test --offline` **314 → 315**（+1，`PlayerInfo` 逐字节）；客户端自检 **478 → 529**（+51，0 失败） |

---


## 12. 开火表现与绘制（原 135 / 140）

### 12.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 原版键盘是怎么接的 | `GraphwarFunctionInput` **本身就是 `LineEdit` 的 Rust 子类**，并在自己的 `gui_input` 里处理按键 ⇒ **输入框聚焦时接管键盘**，不是全局按键 | **观测**（§1.1：恢复出的 `.tscn` 属性 + `.so` 字符串） |
| 我们的接线 | `PlayScreen._unhandled_key_input()` **屏幕级接管**（不用先点输入框） | **自定**（原版"会不会自动 `grab_focus`"**未知**；§1.3 写了升级路径） |
| 键位表 | 数字/`.`/括号/五则运算符/字母（大写折小写）、`π`、`√`→`sqrt(`、退格、Delete、←/→/Home/End、回车提交 | 字符集合**推断**（文法认得哪些字符是**逐位验证**的）；映射本身**自定** |
| 光圈的存在 | 有 | **用户口述**（`16-client.md` §6.3 第 2 条） |
| 光圈谁画 | `plane_view.gd`（棋盘上的一切都由它画，分工不变） | — |
| 光圈的状态源 | 谁该开火 = `TurnInfo{player_id, soldier_id}` 那一个；准备好 = **函数非空** | 谁开火**观测**（`07-protocol.md`/`13-ai.md`）；"准备好"的口径**推断** |
| 光圈的半径/颜色/线宽/**闭合时长**/插值 | 1.15 单位 / 玩家色 / 2 画布像素 / 0.18 s / 线性 | **自定**（§2.3 写了要什么证据） |
| tick 频率 | **60 ticks/秒** | **观测**（`14-turn.md` §2，113 个样本） |
| 每 tick 沿弹道前进 | **0.29 棋盘单位**（恒定速度） | **推断**（`14-turn.md` §6/§7：起飞 tick 与爆炸 tick 之差） |
| 弹道"逐 tick 揭示" | 沿 `shot.points` 前缀推进，撞点（`drawn.impact`）是最后一点 | **推断**（协议按 tick 建模：`FunctionActive{active_tick}` / `EffectInfo{start_tick}` / `QueueDestroyObstacle{tick}`） |
| "一次画完 = 回合结束"有没有写死 | **没有**。绘制推进与回合结算互不相干，`GwFireFx.shot_finished` 是留的口 | — |
| 结算时序 | **没改**：仍在 `fire()` 里同步做完（为了不动既有 245 条断言的时序） | **本轮的取舍**（§3.4 给了改法） |
| `bash client/run.sh selftest` | **245 → 280 passed, 0 failed**（+35 条断言，未改/未删任何既有断言） | — |
| `cargo test --offline` | **296**（与起点一致，`src/` 一行没动） | — |
| `bash client/run.sh dump` | `@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test,level_obstacles,…` | **逐位验证**（内核仍在） |
| 截图 | 光圈开环/闭合、弹道前缀随时间变长（§5.3 有产物与量测） | **观测**（我们自己的截图） |

一句话：**三条都做出来了，且把"哪部分是原版事实、哪部分是我们拍的板"逐条摊开写清了。**

---

### 12.2 一页结论

| # | 问题 | 结论 | 档位 |
|---|---|---|---|
| ① | 准备圈是协议事件还是客户端画的？ | **客户端自绘**：在 `EntityDrawer::draw` 里，`draw_arc` 直接画 3 段弧 | **观测**（§2.2） |
| ② | 画环的函数在哪？ | **`EntityDrawer::draw` = vaddr `0x4b2890`**（Ghidra `0x5b2890`，5984 字节） | **观测**（§2.1 三条独立证据） |
| ③ | 几段？ | **3 段**，方位角**正好相差 2π/3 = 120°** | **立即数观测**（`+2.09439510239`、`+4.18879020479`） |
| ④ | 每段张角多大？ | 半张角 `H`：`min(π/6 + (V−map)/60, 1.07337748998)` ⇒ **30° 起、61.5° 封顶**（单段 60°→123°） | **立即数观测** |
| ⑤ | 环多宽、用什么线宽画？ | `draw_arc(..., width = 0.3, ...)`，**立刻数 `0x1bb2b4` = f32 0x3e99999a = 0.3** | **立即数观测** |
| ⑥ | 抗锯齿？几个折点？ | **关抗锯齿**（`xor %ecx,%ecx`）、**每段 10 个折点**（`mov $0xa,%esi`） | **立即数观测** |
| ⑦ | "张开→闭合"时为什么会转？ | `A0 = V/120` 与 `H = H0 + (V−…)/60` 由**同一个 V** 驱动 ⇒ `dA0 = dH/2` = **15.75°**，与真值图实测的 +15° 吻合 | **立即数观测**（推导）+ **观测**（像素互证） |
| ⑧ | 环的半径？ | **没能静态定死**：运行时取 `PosInfo` 的第 3 个 f64。用"实测外径/内径 = 1.4240"反解得 `R/W = 2.859` ⇒ `R = 0.8576`（W=0.3） | **推断**（比例是观测） |
| ⑨ | 外凸箭头怎么画的？ | 调用点找到了（`EntityDrawer::draw` 三次调 `0x4b9a70`，后者走 `draw_polygon`）；**多边形顶点没解出来** ⇒ 形状用像素量 | **观测**（调用点）/ **推断**（形状） |
| ⑩ | 用户说"4 段" | **量出来是 3 段**：准备态缺口 4 处（0–15°/75–135°/195–255°/315–357°）⇒ 3 段；闭合态箭头实测 2 个（60.5°/180.2°），第 3 个在 300° 被名字与弹道线压住 | **观测**（§3.4） |
| ⑪ | 闭合时机 | 用户实测：**点 Fire 之后**才闭合，不是"表达式非空" | **用户口述**（本轮据此改） |
| ⑫ | 弹道颜色 | **玩家色**（真帧 `#7D6800` = `TEAM_COLORS[0][2]`，与环/箭头/名字同色） | **观测** |
| ⑬ | 弹道线宽 | **2.83 px**（45° 斜线横向跨 4 px ⇒ 4·sin45°）⇒ 我们改成 2.2 画布 px（原来 3.0，粗 1.35×） | **观测** |
| ⑭ | 弹道用什么原语画 | **没扒到**。`draw_polyline` 这个字符串在 `.so` 里**根本不存在**；`draw_line` 的 5 个调用点里没有一个画弹道 | **未知**（§4.5） |
| ⑮ | 速度是按点还是按弧长 | **仍不能判定**。见 §4.4：这个"二选一"其实不是自由度，两种模型只差弦长与弧长之差，但极端陡的段上能差到 4 倍，需要逐 tick 位置才能定 | **推断** |

一句话：**准备圈的画法这次是扒到代码了**（3 段 `draw_arc` + 三个立即数公式），
于是"4 段/1 段弧/半径 1.15/线宽 2 画布 px/半透明"这些我们过去的猜测**全部作废**；
**弹道的绘制原语没扒到**，只有像素证据，本文明确标成推断。

---

### 12.3 实体绘制：`EntityDrawer::draw` 的图元清单（**已进 Rust**）

> 本节补的是 §12.2 的一处空白：§12.2 把准备圈扒成了"3 段 `draw_arc`"，
> 但**箭头（⑨）的形状只是像素量的**、"半径（⑧）静态定不到"、"这个函数到底还画不画别的"没答。
> 本轮把 `0x4b2890` 的**全部绘制调用点**扫完，并把几何落成 `src/entity_drawer.rs`
> （图元描述 + 计算函数，**不调 Godot**）。`cargo test --offline` 全绿（新增 11 条断言，未改/未删既有断言）。

#### 12.3.1 结论表

| # | 问题 | 结论 | 档位 |
|---|---|---|---|
| ① | 这个函数一共画几种原语？ | **两种，共 6 次调用**：`draw_arc` ×3（`0x4b366c`/`0x4b36fd`/`0x4b378a`，GOT 槽 `0x9bc020` → 包装 `0x8abca0`）、实心多边形 ×3（`0x4b30b7`/`0x4b30e8`/`0x4b3119` → `0x4b9a70`） | **观测**（GOT 槽 + 全 `.text` 扫描） |
| ② | 画不画士兵贴图 / 弹道？ | **不画**。`draw_texture_rect`（士兵皮肤/脸/帽）与 `draw_line` 在本函数内调用点 **= 0**。⇒ 这两类是**别的函数**画的，本函数只画"圈 + 箭头" | **观测**（同一张 GOT 表全量比对） |
| ③ | 函数边界 | 体 = `0x4b2890..=0x4b3fea`；`0x4b3feb` 起 `int3` 填充，`0x4b3ff0` 已是下一个函数（TLS 惰性初始化，与绘制无关） | **观测**（`int3` 填充 + 下一条 `push %r15`） |
| ④ | 弧的半径是多少 | **`f32(PosInfo.radius) × 1.5f32`**。`cvtsd2ss` @`0x4b35ba`、`mulss 0x1b9b98` @`0x4b35be`，槽 `0x1b9b98` = `1.5f32` | **逐位（立即数）** |
| ⑤ | 箭头的半径 / 角度 | 同一个 `1.5`：`mulsd 0x19f8e0` @`0x4b3083`（槽 = `1.5f64`）；角度依次取栈槽 `0x170`/`0x168`/`0x180` | **逐位（立即数）** |
| ⑥ | 箭头的顶点 | **向心三角**（详见下）：`P0 = C + r·(cos θ, sin θ)`；`P1 = C + (r+0.5)·(cos(θ−12°), sin(θ−12°))`；`P2 = C + (r+0.5)·(cos(θ+12°), sin(θ+12°))` | **观测**（`0x4b9b13`/`0x4b9b56`/`0x4b9b6e`/`0x4b9b92`/`0x4b9bcf` 五处立即数与指令序） |
| ⑦ | 箭头的张角半宽 / 外沿量 | 半宽 `0.20943951023931953`（= **12°**，两侧各 12°，整口 24°……**更正见下**）；外沿 `+0.5`（槽 `0x19f6d0`） | **逐位（立即数）** |
| ⑧ | 颜色从哪来 | **不是常量**：从哈希表取打包 `u32`（低字节 = R），三通道各 `/255`（`divps` 槽 `0x19c7c0 = {255,255,255,1}`），第 4 通道不参与除法 | **观测**（`0x4b347a` 起 + `divps` @`0x4b34b4`） |
| ⑨ | 箭头颜色几份 | 每个箭头 push **同一个颜色 3 份**（`0x4b9c3c` 起三次 `movupd (%r14)`） | **观测** |
| ⑩ | 三段弧与三个箭头是同一批角度吗 | 弧：`0x170`/`0x168`/**`0x198`**；箭头：`0x170`/`0x168`/**`0x180`** ⇒ 前两个同源，**第三个用了不同的栈槽** | **观测** |
| ⑪ | 为什么第三个不同 | **未知**。`0x198` 是 `A0 + 4π/3`（`0x4b2ebe` 二次累加）；`0x180` 的来源没追出来（可能是 `A0 + 4π/3` 的另一路中间量，也可能差 2π） | **未知** |
| ⑫ | 两组（箭头/弧）会不会同时画 | **未知**。箭头侧在 `0x4b307d` 之后、弧侧在 `0x4b32e2 test %r14b,%r14b` 与 `0x4b34c3 test %bl,%bl` 之后，两处的分支目标都是同一个循环尾 `0x4b37f0`；两个哈希表的语义没解出来 ⇒ **不能断言互斥，也不能断言并存** | **未知** |
| ⑬ | 地址序 | 箭头组（`0x4b30b7`…）在弧组（`0x4b366c`…）**之前** | **观测** |

> **⑦ 的自我更正**：本节初稿写"整口 24°"。按指令 `0x4b9ab8 addsd θ+12°` 与
> `0x4b9b74 addsd θ−12°` 两处看，张角半宽是 **12°**（整口 24°）；
> 先前文档里的"6°"是把它当成了半宽的一半，**没有证据**。以两处 `addsd` 的立即数为准。

#### 12.3.2 箭头形状与真帧的独立互证（**推断**，但两条证据同向）

`0x4b9a70` 的指令序（`rdi` = 输出 `PackedVector2Array`，`rsi` = `&Color`，
`xmm0/xmm1` = 圆心，`xmm2` = 半径，`xmm3` = 方位角）：

```text
0x4b9aab  r15 = [0x9bbed0]                  ; sin_cos
0x4b9ab5  call *r15   xmm0 = θ       → (0xa0 = cos, 0xa8 = sin)
0x4b9ab8  xmm0 = 0.20943951023931953 + θ   ; 槽 0x19fb68
0x4b9ad6  call *r15                  → (0x90 = cos, 0x98 = sin)
0x4b9b13  P0 = C + r·(cos θ, sin θ)                      → push   (0x9bc0a0)
0x4b9b56  r += 0.5                                        ; 槽 0x19f6d0
0x4b9b6e  xmm0 = θ + (−0.20943951023931953)               ; 槽 0x19fc80
0x4b9b92  P1 = C + (r+0.5)·(cos(θ−12°), sin(θ−12°))      → push
0x4b9bcf  P2 = C + (r+0.5)·(cos(θ+12°), sin(θ+12°))      → push
0x4b9c3c  颜色 push ×3
0x4b9cc4  call *[0x9bb200]                                ; 实心多边形原语
```

把 §12.2 的真帧量测代进来做交叉检查（**推断**，因为比例尺本身还差 14%，见原 §3.6）：

| 量 | 代码 | 换成像素（用"`width = 0.3` ↔ 实测环带 6.72 px"这把尺，1 单位 = 22.4 px） | 真帧实测 |
|---|---|---|---|
| 环心半径 | `1.5R` | `1.5R × 22.4` | 中心线 **19.21 px**（圆拟合，838/824 点） |
| 箭头外沿半径 | `1.5R + 0.5` | 比环心多 `0.5 × 22.4 = 11.2 px` | 尖端 **31.0 px**，比环心多 **11.8 px** |

两条**互相独立**的量（外/内径比 → `R`；尖端半径 − 环心半径 → `+0.5`）在**同一把尺**下同时对上，
这比原 §3.6 的单点反解可信：`R_draw = 19.21/22.4 = 0.8576`（与原 `R/W = 2.859` 反解一致），
而 `PosInfo.radius = R_draw / 1.5 = 0.5717`。
⚠️ 但"1 单位 = 22.4 px"与"网格线 25.5 px = 1 单位"仍差 14%，**比例尺本身仍是未解的那一项**。

#### 12.3.3 进了 Rust 的东西

`src/entity_drawer.rs`（**不调 Godot**，只产出图元描述）：

| 项 | 内容 |
|---|---|
| 常量表 | `ARC_POINT_COUNT=10`、`ARC_WIDTH=0.3f32`、`ARC_ANTIALIASED=false`、`SEGMENT_STEP_RAD=2.0943951023931953`、`SEGMENT_STEP2_RAD=4.1887902047863905`、`ANGLE_DIVISOR=120`、`HALF_ANGLE_MIN_RAD=0.5235987755982988`、`HALF_ANGLE_MAX_RAD=1.0733774899765125`、`HALF_ANGLE_GROWTH_DIVISOR=60`、`RADIUS_SCALE=1.5`、`ARROW_HALF_SPREAD_RAD=0.20943951023931953`、`ARROW_RADIUS_DELTA=0.5`。每条注释带 `.rodata` 槽地址 |
| 几何 | `segment_angles(V)`、`half_angle(V, base)`、`drawn_radius(pos_radius)`、`arrow_head(C, r, θ)` |
| 图元 | `Primitive::{Arc{center,radius,start,end,point_count,color,width,antialiased}, Polygon{points,colors}}` |
| 入口 | `ring_primitives()` / `arrow_primitives()` / `entity_primitives()`（= 箭头 3 + 弧 3，地址序） |
| 颜色 | `Color::from_packed_rgb(u32)` —— 三通道 `/255`、`a = 1.0`（**推断**：第 4 通道不参与 `divps`，另取常量；与真帧不透明一致） |
| 验收 | `tests/entity_drawer.rs`，11 条；每条期望值都注明来自哪条指令/哪个槽（**回归保护，不是复原证据**，见 `AGENTS.md` §0.2） |

**客户端还没改**：`client/src/play/*.gd` 仍在用它自己的近似画法（`RING_SPIN_DEG` 那套）。
本轮只把**原版事实**落成可查询的数据 + 计算，客户端照着画的接线留到下一轮
（`src/_scaffold/api.rs` / `src/_scaffold/gdext.rs` 的对外暴露也**没做**，因为一旦暴露就要同时改 `client/`，
而 `client/` 本轮明确不动）。

#### 12.3.4 仍未解（诚实清单）

1. **`PosInfo` 是什么**：`0x4b314b call 0x3c62b0`（`graphwar_plane_pos_manager.rs`）返回的
   `{u8 tag, f64 x, f64 y, f64 r}`，`tag == 1` 才继续画。`r` 的**物理含义**（碰撞半径？缩放？）没定。
2. **`V` 是什么时钟**：`0x4b2e10 call 0x519530` 返回 `f64`。§12.2 ⑦ 已经证明 `V` 同时驱动
   方位角与半张角，但 `V` 的**单位与起点**没定 ⇒ 动画时长仍未定。
3. **两个哈希表各存什么**（`map[i]` 的"起始量"、颜色表）。`base` 的语义只到"查不到就取 π/6"。
4. **第三段的角度槽** `0x180` 与 `0x198` 为什么不同（⑪）。
5. **两组是否互斥**（⑫）。
6. **`0x9bb200` 那个槽对应的引擎方法名**：本轮只到"GOT 槽 → `0x4b9a70` 内部、
   在 `PackedVector2Array`×3 + `PackedColorArray`×3 之后调用"；
   原 §2.6 用另一条路（`lea 0x1c2bad "draw_polygon"` + `call 0x3a01a0`）指到 `draw_polygon`，
   **两条路的槽号不同**，不能据此断言就是 `draw_polygon` ⇒ 记为**推断**。
7. **士兵贴图与弹道的绘制函数**（本函数不画）：`draw_texture_rect` 在 `0x4e5400`，
   `draw_line` 的 5 个调用点里没有弹道 ⇒ 仍需单独定位。

---


## 13. 对局流程的六条硬伤（原 143）

### 13.1 一页结论

| # | 用户现象 | 根因（本次定的） | 修法 | 档位 |
|---|---|---|---|---|
| 1 | 函数碰壁没有爆炸效果 | `PlayScreen` **根本没有效果层**：`EffectInfo` 在本地局不存在，联机局只记录不绘制（`16-client.md` §7 的旧账） | 新增 `GwEffectLayer`（`client/src/game/effect_layer.gd`）+ 在**弹道停止点**发 `ExplosionEffect`、在**死者站位**发 `DeathEffect`/`AshEffect` | 效果名/位置/时机 **逐位+观测**；画法 **自定** |
| 2 | 函数一直留到下一次发射 | 曲线没有任何清除点（`fire()` 只会覆盖它） | 弹道**飞到尽头**就 `_plane.clear_path()` | **用户实测**（无二进制判定点，见 §3.3） |
| 3 | 音效缺失 | 事件↔音效映射 0 条确定（`16-client.md` §4.1），本地局连"什么时候该响"都没有 | 按效果事件出声；**槽位**用二进制跳表，**音效身份仍是自定** | 槽位 **逐位**、音效 **自定** |
| 4 | 运动过程中再次发射会打断当前函数 | `fire()` 没有任何"在飞"判断 | `_shot_in_flight` 拦截，并写清拒绝理由 | **观测**（状态机）+ 客户端自定实现 |
| 5 | **还没击中目标就判胜利** | `fire()` 里**同步**做了 `_resolve_shots → _settle_round → _check_match_over`：开火当刻就判胜负、当刻就弹结算框 | 结算**延后到弹道飞完**；死亡在**弹道走到那个点**才落地 | 顺序 **观测**（`14-turn.md`/`07-protocol.md`）+ tick 语义 **观测**（`15-target-life.md`） |
| 6 | 对面是 AI 时卡住不动 | ① 回合推进靠"人类又按了一次 Fire"（`_next_turn()` 只在自己 `fire()` 之后调）；② **轮到 AI 之后没有任何人开火** | 回合推进改为**结算完成时**由 `GwTurnDriver`（规则 = `src/graphwar_turn_manager.rs`）驱动；轮到 AI 就倒计时后由 AI 自己开火 | 轮转规则 **逐位**；AI 的那一发 **自定** |
| 附 1 | 默认士兵数是 1 | `begin_match()` 无条件用玩家表的 `soldiers`（默认 1），**忽略关卡数据** | 战役关按 `10-campaign.md` §7.1 的「我方士兵」列装人 | **逐位 + 运行时**（21/21） |
| 附 2 | 成就/队伍图标过小 | 定位到 `client/src/screens/game_setup.gd:316` 的 `RankIcon` : `custom_minimum_size = Vector2(24, 24)` | **没改**（不在本任务的可改文件里，且原版尺寸无证据）——见 §8 | **未知** |

**新增文件**：`client/src/game/effect_layer.gd`、`client/src/game/turn_driver.gd`、本文。
**改动文件**：`client/src/screens/play_screen.gd`、`client/tools/selftest.gd`（**只加断言**）。
**没动**：`client/src/play/fire_fx.gd`、`client/src/play/plane_view.gd`、`src/`（Rust 一行没改）、
`client/src/app/*`、其它界面。

验证：`cargo test --offline` **333 passed**（一条没变，本轮没动 Rust）；
`bash client/run.sh selftest` **635 passed, 0 failed**（基线 581 → +54，既有断言一条没删没改）。

---


## 14. 我们自定的规则登记（**必须能一条条指出来**）

### 14.1 有意偏离、占位与自定规则（全部登记）

> 判据：凡是"原版行为没有观测证据、由我们决定"的东西，都必须出现在这张表里。

| # | 项 | 我们的做法 | 原版状态 | 影响的界面 |
|---|---|---|---|---|
| 1 | ~~弹道~~ | ✅ **已消除**：`GW2Rebuild.path_calculate` 已暴露并被客户端使用，`source=kernel`，等弦长 0.01（`16-client.md`） | 等弦长 0.01 二分采样 | PlayScreen |
| 2 | ~~命中/致死~~ | ✅ **已消除**：`GW2Rebuild.hit_test` 走 `graphwar_shot_calculator::settle::resolve_points`（掩膜 + 数据字段半径），旧 0.8 近似已删（`16-client.md`）。⚠️ 残留：原版按实体 id 排除射手，`game.rs::fire` 仍有一条 1e-12 位置排除（`15-target-life.md`） | 掩膜逐点采样、穿过即死、友军也会死 | PlayScreen |
| 3 | ~~关卡数据~~ | ✅ **已消除**：21 关标题 / 描述 / 参数（障碍模式、`min_count`、士兵数、目标数、Bot 难度、回合上限）**全部解出**，并与 385 份回放交叉验证 21/21（`10-campaign.md`）。⚠️ 客户端已接入（`10-campaign.md`）；**仅障碍物仍不画**（公式有、原版的随机流没有） | 硬编码在 `level_info()`（`0x4ad380`） | Campaign / JoinLevelPopUp |
| 4 | ~~关卡解锁~~ | ✅ **规则已还原**：`campaign.save` = 单行 `level_<n>`（0 基、**无尾换行**、只增不减、启动不创建），写点在 `0x4183b0`；客户端是否已接入见 `16-client.md` | 同左（`10-campaign.md` 附录 A） | Campaign |
| 5 | **Bot 分队** | 第 1 个 Bot 去对面（team 1），之后按人数奇偶 | 未观测 | GameSetup |
| 6 | **Bot 名字 ↔ 难度** | 固定把 `ChatGPT/Wolfram/Alpha Zordon/Multivac/Deep Blue/Deep Thought` 依次给 0..5 档 | **对应关系未观测**（`16-client.md` §4.6 明写），且名字池与难度不是 1:1 | AddBotPopUp |
| 7 | **网络** | 没有网络层：`join_join_ip` 只校验 `host:port`，然后模拟"连接中→断开" | 真实 TCP 握手已复原（`09-network.md`、`src/_scaffold/protocol.rs`），未接进界面 | LanLobby / 三个连接弹窗 |
| 8 | **公网房间列表** | 5 条静态占位（`US Room 1/3/4`、`EU Room 1/2`，名字照抄观测） | 来源未知（`16-client.md` §8） | LanLobby |
| 9 | **聊天** | 只显示在本地，不发网络包 | 走协议（`09-network.md`） | ChatPopUp |
| 10 | **回放** | 列 `user://replays/` 的文件名；不解析、不播放 | 格式已观测（`16-client.md`），播放界面未观测 | ReplayLobby / PlayScreen |
| 11 | **内购/广告** | 广告位保留 60 px 占位条（写"disabled in this build"），内购按钮只提示 | Linux 端本来就没有（用户要求） | 11 个界面有广告条 |
| 12 | ~~音频~~ | 🔶 **链路已还原、尚未接入**：10 个导入产物四道独立检查全部通过；`humm.wav` 由 Rust 侧 hound 解析（`0x50a080`），音量 `-15f`/`-5f`、32767 归一化、静音走 Master 总线（`17-resources.md`）。"有一个文件损坏"已查明是**使用者自己的重建脚本**产物，非游戏资源 | 见 `17-resources.md` | PlayScreen |
| 13 | **结算文案** | 输：`YOU lOST`（原版原文）；赢：`YOU WON`（**占位**，tooltip 注明原文未观测） | 只观测到失败一侧 | EndGamePopUp |
| 14 | **玩家表滚动** | 玩家列表**不用** `ScrollContainer`（原版有） | 原版用 `ScrollContainer` | GameSetup（原因见 §9.1） |
| 15 | **"无对手时 Start"** | 不开始，并且**给一句提示** | 原版是**纯静默**（`archive/22-live-game-harness.md` §5.1） | GameSetup |
| 16 | **键盘编辑语义** | §4.1 那张表（推断） | 未观测 | PlayScreen |
| 17 | **函数输入上限** | 128 字符 | 未观测 | PlayScreen |
| 18 | **字号上限** | 只在小窗口下钳到最小 10 px（不设上限） | 原版无观测 | 全部 |
| 19 | **存档** | `user://gw2_client.cfg`（名字/外观/战役进度） | 原版存档格式未解，我们**不读不写**它 | PlayerScreen / Campaign |
| 20 | **回合时限到点** | 自动换手 | 时间基准已观测（`14-turn.md`），自动换手行为未逐条观测 | PlayScreen |

---


## 15. `graphwar_root_node.rs` 规格（本轮新增：屏表 / 命令总线 / 状态分派）

> **本节解决什么**：`graphwar_root_node.rs` 是 12 个 GDExtension 类里**规模最大**的一个
> （`docs/02` §16.2 类清单第 3 项），`docs/149` 记 23 个可归属函数、0 个已做。
> 本节把 6 个关键函数解到「能照着写」的粒度（其中 `0x3a22f0` **不在**那 23 条里，是无 panic 锚的构造器），并给全**屏名表**与**命令键表**。
>
> **地址口径**：一律 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + `0x100000`（只加一次）。
> 本节所有「函数入口」都用 `analysis/funcstarts_ghidra_va.pkl` 与 REA `procedure_info`
> 交叉验证过；**注意 0x3a5ea8 / 0x3a6038 / 0x3a60dc 都不是函数入口**（见 §15.8 的更正）。

### 15.1 模块函数清单（`fixtures/module_funcs.tsv`，23 条）

```text
0x3a2e10 0x3a54e0 0x3a55e0 0x3a5620 0x3a5660 0x3a5760 0x3aa6a0 0x3aa820
0x3afb00 0x3afcf0 0x3afe90 0x3b00d0 0x3b0830 0x3b4ec0 0x3b56c0 0x3b5910
0x3b5ac0 0x3b5ce0 0x3b6e00 0x3b6ed0 0x3b7020 0x426430 0x433c00 0x4f90b0
```

下面 6 个是「实例装配 / 分发」的主干，本节逐条给规格：

| vaddr | 大小 | 猜的名字 | 档位 |
|---|---|---|---|
| `0x3a22f0` | `0xb20` | **构造/初始化**（把下面各字段一次装配好） | 观测（写入点 §15.3）｜⚠️ 该地址是独立函数入口，但**不在** `fixtures/module_funcs.tsv` 的 23 条里（无 panic 锚） |
| `0x3a2e10` | `0x26d0` | **`_ready()`**（名字是推断）：取 3 个 holder、连 IAP 信号 | 内容=观测／命名=推断 |
| `0x3a5760` | `0x4f40` | **逐帧/事件状态机**（Ghidra 把几十个内联体合成一个函数） | 部分观测 |
| `0x3af850` | `0x2b0` | **广告可见性闸门**（按屏名决定 create/destroy） | 观测 |
| `0x3afcf0` | `0x1a0` | 进屏前的「当前是否对局中」判定 | 推断 |
| `0x3b0830` | `0x4690` | **命令总线**（按钮 `input_text` → 动作） | 观测 |

⚠️ **`0x3a5760` 是 20288 字节、339 条比较、64 个直接调用目标**。Ghidra 把它当**一个**函数，
但它的入口第一屏就在查 `screens["PlayScreen"]`、下半段又在跑状态机——
**它极可能是 `_process()`/`notification()` 级别的总入口 + 大量内联**，不是"一个业务函数"。
本节只给能指到字节的结构，不猜它的 Rust 形状。

### 15.2 屏名表：**15 个值，地址全部逐条量到**

屏名是 `&'static str`，判据 = 代码里的 `lea rsi,[rip±X]` + 紧跟的 `mov edx,len`
（`.rodata` 里字面量**不以 NUL 结尾**，`strings` 会把整段粘成一条，见 `docs/02` §16.1）。
下表 len 都是**指令里的立即数**，不是目测。

| # | 屏名 | vaddr | len | 引用它的函数 |
|---|---|---|---|---|
| 1 | `MainMenu` | `0x19fc70` | 8 | `0x3b0830` ×3 |
| 2 | `Campaign` | `0x19fcc0` | 8 | `0x3a5760`、`0x3b0830` ×3 |
| 3 | `LanLobby` | `0x19f380` | 8 | `0x3b0830` ×2 |
| 4 | `GameSetup` | `0x1bda32` | 9 | `0x3a7cba`(in `0x3a5760`)、`0x3b0830` |
| 5 | `PlayScreen` | `0x1bd92b` | 10 | `0x3a5760` ×5、`0x3b0830` ×5 |
| 6 | `ReplayLobby` | `0x1bdaec` | 11 | `0x3b0830` ×3 |
| 7 | `PlayerScreen` | `0x1bdb2f` | 12 | `0x3b0830` |
| 8 | `JoinIpPopUp` | `0x1bdaf7` | 11 | `0x3b0830` |
| 9 | `AddBotPopUp` | `0x1bdb02` | 11 | `0x3b0830` |
| 10 | `ConfirmPlay` | `0x1bdb0d` | 11 | `0x3b0830` |
| 11 | `ExitPopUp` | `0x1bdb18` | 9 | `0x3b0830` |
| 12 | `JoinLevelPopUp` | `0x1bdb21` | 14 | `0x3b0830` ×2 |
| 13 | `ConnectingPopUp` | `0x1bda23` | 15 | `0x3a5760` |
| 14 | `EndGamePopUp` | `0x1bda3b` | **12** | `0x3a5760` |
| 15 | `DisconnectedPopUp` | `0x1bda73` | 17 | `0x3a5760` |

⇒ **7 个基础屏 + 8 个弹窗 = 15，正好等于 `docs/16` §1 的「15/15 界面」**（观测）。
另有三条**不是屏**但同池的节点名，写实现时别混进去：
`ScreensControl` `0x1bd935`(14)、`IAP_holder` `0x1bd943`(10)、`Ad_holder` `0x1bd94d`(9)
—— 这三个只在 `0x3a2e10`（`_ready`）里各被 `get_node` 一次（观测）。

**屏的持有结构**：`screens: HashMap<&'static str, Gd<Node>>`，落在 `[self+0x1638]`（表）
/ `[self+0x1640]`（bucket mask）/ `[self+0x1650]`（长度）/ `[self+0x1658]`（hash builder）。
判据（观测）：`0x3a57c8` 起是完整的 hashbrown 查找内联体 ——
`lea rdi,[rbx+0x1658]`（hasher）→ `lea rsi,"PlayScreen"; mov edx,0xa` → `call 0x47daa0`（哈希）
→ `shr rsi,0x39` 取高 7 位 → 16 字节 `pcmpeqb` 探测循环 → 命中后 `cmp qword [bucket-0x18],0xa`
（比长度）→ `movabs 0x6572635379616c50`("PlayScre") + `xor word 0x6e65`("en")（比内容）。
`cmp qword [rbx+0x1650],0; je ...` 是「表空则跳过」。
⚠️ **推断**：K 是 `&'static str`（16 字节 `(ptr,len)`，桶步长 40 字节）；V 是 `Gd<Node>`。
缺的证据：命中后的 `upcast_ref` 目标类型（`0x406250` 的第二个实参是类型名字面量，
本轮没逐条解）。

### 15.3 `[self+0xNN]` 字段表（写入点全列，用于还原结构体）

| 偏移 | 大小 | 语义 | 判据（vaddr） |
|---|---|---|---|
| `+0x58` | dword | 开关/计数，**只有 0 与 1 两个值** | `0x3a7005 cmp byte[+0x58],0`、`0x3a7d4c cmp dword[+0x58],1`、`0x3afe f1 cmp dword[rbx+0x70],1`（相邻字段） |
| `+0x60` | 指针 | 交给 `0x41d1f0` 的对象 | `0x3a700f`、`0x3a7d56` |
| `+0x15c8` | qword（低字节=bool） | 主开关：为 0 时 `0x3a5760` 直接走 `0x3a60f0` | 读 `0x3a5789`；写 `0x3a60e6`(0)、`0x3a8ae9`(0)、`0x3b1ff2`、`0x3b398f` |
| `+0x15cc` | dword | **倒计数**（`dec` 一次就跳到 `0x3a60f0`） | `0x3a5796`-`0x3a57a2`、`0x3a61d5`-`0x3a61e1` |
| `+0x15d0` / `+0x15d8` | qword / dword | 一个 `String`+长度的返回值落点 | `0x3a60ad`、`0x3a60b4` |
| `+0x1638…+0x1668` | — | 屏表 `HashMap<&str, Gd<Node>>` | §15.2 |
| `+0x1668` | 指针→单元 | **play 侧控制器**；`[单元+0x10]` 借用标记、`[单元+0x18]` 载荷 | 约 50 处，见 `0x3a6a7f`、`0x3b0b75` |
| `+0x16c8` | 指针→单元 | **另一个控制器/数据对象**，同形状 | `0x3a5f27`、`0x3a6a60`、`0x3b0b49` |
| `+0x1702` | bool | 闸门 A（为 1 时 `0x3a5760` 走 `0x3a60f0`） | 读 `0x3a5774`、`0x3a65f1`；写 `0x3b3996`(1)、`0x3a8af3`(0)、`0x3b4094`(0) |
| `+0x1703` | bool | 闸门 B | 读 `0x3a57ad`、`0x3a61ec`；写 `0x3b2130`(1)、`0x3b3ad4`(1)、`0x3b50d4`(0) |
| `+0x1704` | u8 | **当前屏枚举**，观测到值 0/1/2/3/4；**只在 `0x3b0830` 里被写** | 写 `0x3b0931`(0)、`0x3b0996`(1)、`0x3b3168`(2)、`0x3b08ac`(3)、`0x3b237f`(4)；`0x3a5760` 在 `0x3a6fdc` 按它 `match {1,2,4}` |
| `+0x1705` | u8 | **对局状态枚举**（`GameState`），全库**只有一个写入点** `0x3a8a09`（`mov [rbx+0x1705], r15b`） | 比较：`0x3a6e7e`(=8)、`0x3a7cd8`(=6)、`0x3afd1a`(=6) |

**构造期的初值（观测，`0x3a2b25`-`0x3a2b30`）** —— 这一条把两个枚举的起点钉住了：

```text
0x3a2b25  mov dword [r15+0x15c8], 0
0x3a2b30  mov dword [r15+0x1702], 0x8010000     ; ← 4 个字节一次写
          ⇒ [0x1702]=0x00  [0x1703]=0x00  [0x1704]=0x01  [0x1705]=0x08
```

⇒ **`+0x1704` 初值 1、`+0x1705` 初值 8**。
再结合「`0x3b0830` 把 `+0x1704` 写成 0/1/2/3/4」与「启动默认进 `MainMenu`」，
可以给一条**推断**：`+0x1704` 的 1 = `MainMenu`（缺的证据：把 5 个写点与 5 个
`get_node` 目标一一对上）。`+0x1705` 的 8 则明示它**不是** `docs/14` §2 那个 6 态枚举
（6 态最大下标是 5），实现时要另建一个枚举。

⚠️ **未知（重要）**：`+0x1705` 的**枚举定义**没解出来。已知它被与 `8`、`6` 比较，
而 `docs/14` §2 的状态机只有 6 态（`Setup … EndingGame`）⇒ **值 6 不可能是那 6 个状态之一**，
要么客户端状态机比服务端多几态，要么这个字段不是 `GameState`。
缺的证据：写入点 `0x3a8a09` 的 `r15b` 来源（本轮只追到 `0x3a6f59` 的常量 `6` 与 `0x3afcf0` 的判定）。
**这是本节最大的未决项，实现时不要照抄成「state==6 就是 X」。**

### 15.4 `0x3a2e10` `_ready()`（观测）

- **入参**：`rdi` = self（`GraphwarRootNode`）。
- **取节点**：`get_node("ScreensControl")`（`0x1bd935`）、`get_node("IAP_holder")`（`0x1bd943`）、
  `get_node("Ad_holder")`（`0x1bd94d`），各一次；随后 `upcast_ref` 到具体类
  （`0x1bd8f2`，14 次）——被上抛的类型名在 `0x1c1c5a` / `0x1c1c87` 池里：
  `graphwar_button::GraphwarButton`、`graphwar_function_input::GraphwarFunctionInput`、
  `graphwar_ad_switch::GraphwarAdSwitch`（观测）。
- **连信号**（同池字面量 + `connect` 口径的间接调用）：
  `request_permissions_result`（`0x1bd956`）、`on_request_permissions_result`（`0x1bd970`）、
  `purchase_confirmed`（`0x1bd98d`）、`purchase_denied`（`0x1bd99f`）、
  `VersionLabel`（`0x1bd9ae`）、`process`（`0x1bd9b9`）、
  以及 `0x19c1d0` 池里的 `purchase_pending` / `function_update_limit_info_event` /
  `add_soldier_request` / `entity_removed_event` / `ban_connect…`。
- **写字段**：`+0x1638`（屏表，`0x3a3b71`）、`+0x1668`（`0x3a3249` 起 8 处）、`+0x16c8`（`0x3a3df2`）。
- 档位：**观测**（字面量 + 上抛点）。⚠️ **未知**：`connect` 用的是哪个 GOT 槽（
  `0x9bb030` 未定名，见 `docs/14` §9.2），所以「连的是信号还是 property」是**强推断**。

### 15.5 `0x3af850` 广告可见性闸门（观测，可照写）

```
fn 0x3af850(rdi = &{names: Vec<(&str)>, ad_holder: Option<Gd<AdHolder>>},
            rsi = name_ptr, rdx = name_len, rcx = &bool)
```
- `rbx = [rdi+0x18]`，`test rbx,rbx; je 0x3afaf0`（`0x3af868`）⇒ 没有 `Ad_holder` 直接返回。
- `r15 = [rdi]`；`movdqa xmm0,[r15]; pmovmskb` ⇒ 表头带 16 字节「空位掩码」（`0x3af883`）。
- 遍历：`rbp = [r15+rax*8-0x20]`（名字 ptr）、`r12 = [r15+rax*8-0x18]`（len）（`0x3af928`）。
- 判等：`cmp r12,rdx; jne`（长度先比，`0x3af932`）→ `call *0x9baa10`（三参 memcmp，`0x3af944`）。
- **两个长度 10 的特例**（`0x3af957` 之后）：`0x756f72676b636162`+`0x646e` = **`background`**；
  `0x6572635379616c50`+`0x6e65` = **`PlayScreen`**。
- 命中/未命中都归到 `0x3af8a0`：`upcast_ref`（`0x406250`，"upcast_ref" 长 10）→
  `call *0x9ba978`，`esi = bpl`（来自第 4 个实参那个 `bool`）。
- ⇒ **推断（方向唯一）**：`*0x9ba978` = 设置可见性一类的 bool 成员（`set_visible` 候选）；
  `0x3af850(name, visible)` 决定「这一屏要不要挂广告」。**`PlayScreen` 与 `background`
  被单列**，与 `docs/19` 的「`play_screen` 里没有 `AdContainer`」一致（两路证据独立）。
- `call_deferred` 侧：`create_ad_view`（`0x1bd9c1`）/ `destroy_ad_view`（`0x1bd9cf`）
  **只被本函数引用**（各 1 处，`0x3af9xx` 那两个分支）。
- 调用者：`0x3a7bed`、`0x3a899b`（在 `0x3a5760` 内）、`0x3b3f2b`、`0x3b410f`、`0x3b4257`（在 `0x3b0830` 内）。
  ⚠️ **更正**：`docs/19` §③ 记的「`show_screen` `0x3a5ea8` → create_ad_view `0x3a6038`」
  这两个地址**都不是指令边界**（`0x3a6033` 起是一条 7 字节的 `mov rdi,[rsp+0xe8]`）。
  真实的调用链是 **命令总线/状态机 → `0x3af850` → `call_deferred("create_ad_view"|"destroy_ad_view")`**。
- **未知**：`0x9ba978` / `0x9baa10` 的引擎侧成员名；`Vec` 里到底是 12 个「带 AdContainer 的屏」
  还是 15 个屏名（`docs/19` 的 `.tscn` 清点给 12）。

### 15.6 `0x3b0830` 命令总线（观测：键表逐条量到）

签名（Ghidra）：`ulong FUN_004b0830(long self, ulong *name_ptr, ulong *name_len)`。
参数其实是 `(self, &str)`：`rdx` 是**长度**，函数体第一件事就是
`if (len != 8) { if (len == 0x14) … }`（`0x4b0830.c` 第 128/129 行）。

**入口**：唯一调用点 `0x3a6d7d`（在 `0x3a5760` 内），而且是在一个**循环**里：
`0x3a6d40 cmp dword[r14],3` → `call 0x3faf60`（Variant→&str）→
`0x3a6d7d call 0x3b0830(self, ptr, len)` → 回收。

**键表**：函数体用「先比长度、再比 8 字节立即数」的方式匹配下列键
（下面每条都是把 `movabs/xor` 的立即数按小端还原出来的，**观测**）：

| 键 | 长度 | 判据（反编译行） |
|---|---|---|
| `player_settings` | 15 | `0x4b0830.c:204`（`0x73676e6974746573`+`0x735f726579616c70`） |
| `start_level` | 11 | `:1389` |
| `multiplayer` | 11 | `:1418` |
| `create_game` | 11 | `:1419` |
| `toggle_mute` | 11 | `:1438` |
| `exit_campaign` | 13 | `:1769` |
| `ask_exit_play` | 13 | `:1790` |
| `cancel_add_bot` | 14 | `:1846` |
| `exit_lan_lobby` | 14 | `:1857` |
| `cancel_join_ip` | 14 | `:1874` |
| `add_bot` | 7 | `:1026` |
| `join_ip` | 7 | `:1040` |

⚠️ 与 `docs/16` §1 第 2 条互相印证：**按钮的 `input_text` 就是这套键**（`campaign` /
`add_bot_3` / `fire` / `toggle_names` …）⇒ 本函数就是那个「命令总线」。
**推断**：上面 12 条只是本轮**解到底**的；函数体还有大量 `param_3` 长度分支
（8 / 0x14 / 0xf / 6 / 3 …），键总数应当 >12，**未穷举**。缺的证据：
把 `0x4b0830.c` 的 131 KB 反编译逐分支过一遍（本轮只做了长度分支 + 立即数还原）。

**它做的事**（观测）：
1. 写 `[self+0x1704]`（当前屏）—— 0/1/2/3/4（§15.3）；
2. 写 `[self+0x1702]`、`[self+0x1703]` 两个闸门；
3. 大量 `lea rdi,[rbx+0x1638]` + `lea r14,[rbx+0x1638]`（屏表查找，32 处）后接
   `[self+0x1668]` / `[self+0x16c8]` 的两个控制器；
4. 调 `0x3af850` ×3（广告）、`0x3afcf0`（`0x3b385f`）；
5. `get_node` 到 `PlayerScreen` 下的 `LevelTitleLabel`/`AxisModeButton`/`FunctionModeButton`/
   `TurnModeButton`/`TimeModeButton` 并 `set_button_icon`（`0x1bdb86`，长 15）；
   `unlock_skins`（`0x1bdaaf`）/`purchase_graphwar_support`（`0x1bdabb`）/
   `Waiting for permissions.`（`0x1bdad4`）。
6. 事件名比较池（`pcmpeqb` 16 字节）：`0x19cd70`（`…state_info_event`）、`0x19cef0`
   （`toggle_view_functions`）、`0x19d180`（`cancel_exit_play`/`player_info_event`）、
   `0x19d3e0`（`cancel_connecting`/`function_active_event`）、`0x19dc90`
   （`exit_replay_lobby`/`destroy_obstacle`/…`count_info_event`/`time_mode_info_event`）、
   `0x19e6b0`（`exit_player_sett`/`target_info_event`/`function_active…`/`chat_message_eve`）。
   ⇒ **`0x3b0830` 不只是「切屏」，它还吞掉了一批远程事件名**（观测到串，**推断**为同一
   `match` 的其它分支）。⚠️ 这条与 `docs/14` §9.3「`function_active_event` 只被 `0x3d5260` 引用」
   **不冲突**：`0x3b0830` 引用的是那 16 字节池的**同一段字面量**，属另一处引用点。

### 15.7 `0x3a5760`：状态机骨架 + 与三个外部对象的接线（部分观测）

**入口闸门**（全部观测，`cmp`/跳转地址逐条给）：

```text
0x3a5774  cmp byte [self+0x1702], 0 ; jne 0x3a60f0      ; 闸门 A
0x3a5789  cmp byte [self+0x15c8], 0 ; je  0x3a60f0      ; 主开关
0x3a5796  mov eax,[self+0x15cc]; test; je 0x3a57ad
0x3a57a0  dec eax; mov [self+0x15cc],eax; jmp 0x3a60f0  ; 倒计数一次就收工
0x3a57ad  cmp byte [self+0x1703],0 ; jne 0x3a60f0      ; 闸门 B
0x3a57ba  cmp qword [self+0x1650],0 ; je 0x3a6095       ; 屏表空
0x3a57c8  …查 screens["PlayScreen"]（§15.2）→ 拿到 play 控制器
0x3a5e18  rax = [rip+0x614891] (GOT 0x9ba6b0); cl=[rax+0x14f0]
0x3a5e26  test cl,cl; je 0x3a98c7                       ; 引擎单例未就绪 → panic 路径
0x3a5e36  call *[rax+0x11e0]
0x3a5e3c  取 OS 名 → cmp len,3 + "iOS"(0x4f69/0x53) → 0x3a8db7
0x3a5ebc  再取一次 → cmp len,7 + "Android"(0x72646e41/0x64696f72) → 0x3a8ee0
```

⇒ **推断**：`0x3a5e18`-`0x3a5f00` 这一段是 `OS::get_name()` 的平台判定（iOS / Android），
两处命中分别跳到 `0x3a8db7` / `0x3a8ee0`（媒体相册/权限那套，见同池
`publish_video_to_album` `0x1bd9de`、`publish_image_to_album` `0x1bda0d`、
`Android_mediastore_holder` `0x1bd9f4`、`IOS_album_holder` `0x19c040`）。

**与 `play_screen_control` 的接线**（观测，调用点逐条）：

| 调用点 | 目标 | 传参 |
|---|---|---|
| `0x3a6aaf` | `0x3dcbb0` | `rdi=[[self+0x16c8]+0x18]`、`rsi=[[self+0x1668]+0x18]` |
| `0x3a6d7d` | `0x3b0830` | `rdi=self`、`rsi/rdx = 循环里的 &str` |
| `0x3a6fbf` | `0x3da130`（胜负横幅） | `rdi=[rbx+0x18]`、`rsi=[r14+0x1f8]` |
| `0x3a60dd` | `0x3d9b50`（回放视图） | `rdi=[[self+0x16c8]+0x18]` |
| 4×（`docs/14` §9.1） | `0x3d9b50` | 同上形状 |

**与 `Ad_holder` 的接线**：只有 `0x3af850` 一条路（§15.5），
`+0x1668`/`+0x16c8` 两个控制器都**不直接**碰广告。

**与 `lan_vbox` 的接线**：**观测到的是「没有直接调用」** ——
`tools/callers.py --module graphwar_lan_vbox.rs`（10 个函数）只查到 **1 个外部调用者
`0x500810`（未归属）**，不在 `root_node` 的 64 个直接调用目标里。
`root_node` 与局域网列表的唯一联系是 `get_node("LanLobby")`（`0x19f380`，`0x3b0830` 里 2 处）
⇒ **推断**：`lan_vbox` 是挂在 `LanLobby` 场景里的独立控件，由脚本/引擎驱动，
`root_node` 只负责切屏到它。⚠️ 缺的证据：`Gd` 信号回调（`call_deferred`/`connect` 的闭包）
不在 `call rel32` 里，需要 `tools/ptr_slots.py` 或运行时复核。

### 15.8 三条地址更正（写实现前必读）

1. **`0x3a5ea8` 不是函数入口**。它落在 `0x3a5760`（`0x3a5760…0x3aa6a0`）**内部**。
   `docs/19` §③ 把它当作 `GraphwarRootNode::show_screen` 的入口，那是**文件偏移口径**
   （`0x3a5ea8 - 0x100000 + 0x101000`）——本仓库的 `AGENTS.md` 已记过这个坑。
2. **`0x3a6038` / `0x3a60dc` 也不是指令边界**（`0x3a6033` 起是 7 字节 `mov`）。
   广告链路的正确落点是 `0x3af850`（§15.5）。
3. **`0x3b0830` ≠ `show_screen`**。它匹配的是**小写命令键**（§15.6），屏名在它里面
   只作为 `get_node` 实参出现。真正的「切屏」由它写 `[self+0x1704]` 完成 ——
   所以复现时应当是 **`command(&str) -> 设 current_screen + 显隐 + 广告闸门`** 一个函数，
   而不是「`show_screen(name)` 再进来分派」。

### 15.9 本节没解出来的（照实写）

| 缺口 | 缺什么证据 |
|---|---|
| `[+0x1705]` 枚举定义（值 6 / 8 是什么） | 追 `0x3a8a09` 的 `r15b`；目前只知道 `0x3a6f59` 有过 `mov r15b,6` |
| `0x3b0830` 的**全部**命令键（已解 12 条） | 131 KB 反编译逐分支过一遍 |
| `0x47daa0` / `0x9ba978` / `0x9baa10` / `0x9ba990` / `0x9ba998` 的引擎侧成员名 | 运行时读 GOT 槽（Frida 本轮不可用：`frida_binding.node` 缺失） |
| 屏表 V 的确切类型、`upcast_ref` 的目标类名 | 逐条解 `0x406250` 的第 2 实参 |
| `lan_vbox` 与 `root_node` 的信号级联系 | `ptr_slots` + 运行时 |

---

### 15.10 实现侧：已落码的部分、四条更正、仍未落的（实现轮补充）

> **本节解决什么**：§15.1–§15.9 是**规格**；本节记「规格变成代码」时**逐字节复核**的结果。
> 落地文件：`src/graphwar_root_node.rs`（公开 API）、`tests/graphwar_root_node.rs`（接受验收）。
> **本节新增的每一条都标了档位**；与 §15 旧结论冲突的，以本节为准并写明改了什么。

#### 15.10.1 落码清单（与原版分母对照）

| 原版函数 | 落在哪 | 覆盖程度 | 档位 |
|---|---|---|---|
| `0x3af850` **切屏 + 广告** | `FN_SHOW_SCREEN` / `ScreenSwap` / `screen_swap` / `apply_show_screen` | **纯逻辑全覆盖**（逐分支真值表见 §15.10.4） | 观测 |
| `0x3b0830` 命令总线 | `FN_COMMAND_BUS` / `Command` / `CommandEvidence` / `field::CURRENT_SCREEN` | **12 条键 + 字段写点**；其余分支未过完 | 观测 |
| `0x3a2e10` `_ready` | `FN_READY` / `READY_NODES` / `READY_POOL_LITERALS` | 3 个 `get_node` + 池字面量；**不可执行**（Godot 侧不在本 crate） | 观测 |
| `0x3a22f0` 装配/初始化 | `FN_CONSTRUCT` / `constructor_init_fields` / `CONSTRUCTOR_IMM_STORES` | 初值字 + 8 条立即数写 | 观测 |
| `0x3af700` 按名查屏 | `FN_SCREEN_GET` / `SCREENS_MAP_OFFSET` | 只给「它是 hashbrown 查找 + 表形状」 | 观测 |
| `0x3afcf0` 对局中判定 | `FN_BUSY_CHECK` | **仅地址**，函数体未解 | 推断 |
| `0x3a5760` 状态机 | `FN_STATE_MACHINE` | **仅地址**；只落了「进 `PlayScreen` 那条路」的产物（切屏调用） | 部分观测 |

⇒ **进度口径**（`AGENTS.md` §0 第 2 条）：23 条可归属函数里，**2 条部分落码**
（`0x3b0830`、`0x3a2e10`），**21 条未落码**；另有 3 条**无 panic 锚**的骨干函数
（`0x3a22f0` / `0x3af700` / `0x3af850`）不在那 23 条里，其中 `0x3af850` 已全覆盖。
⚠️ `cargo test` 全绿**不**改变这个分母。

`0x3a5760`（20288 字节 / 339 条比较 / 64 个直接调用目标）**刻意不猜**：Ghidra 把它当
一个函数，但它极可能是 `_process` 级入口 + 大量内联，本文件只写它**可指认的产物**。

#### 15.10.2 逐位钉住的两张表（复核方法与结果）

**做法**：`.rodata` 的 vaddr == 文件偏移 ⇒ 直接按 [`addr`] 读字节；长度用**第二个独立来源**
（使用点的 `mov $len,%edx`/`%r8d`）或**紧邻关系**（`addr + len` == 下一个字面量起点）交叉验证。

屏名（15 条，全部字节复核过；`len` = 字节数）：

| # | 屏名 | `.rodata` | len | 长度立即数所在指令 | 紧邻的下一个字面量 |
|---|---|---|---|---|---|
| 1 | `MainMenu` | `0x19fc70` | 8 | `0x3b3f23` | `0x19fc78`（未解） |
| 2 | `Campaign` | `0x19fcc0` | 8 | `0x3a8993` | `0x19fcc8` = `YOU lOST` |
| 3 | `LanLobby` | `0x19f380` | 8 | `0x3b3e7b` | `0x19f388`（未解） |
| 4 | `GameSetup` | `0x1bda32` | 9 | `0x3a7cc6` | `0x1bda3b` = `EndGamePopUp` |
| 5 | `PlayScreen` | `0x1bd92b` | 10 | `0x3a57d6` | `0x1bd935` = `ScreensControl` |
| 6 | `ReplayLobby` | `0x1bdaec` | 11 | `0x3b3ef0` | `0x1bdaf7` = `JoinIpPopUp` |
| 7 | `PlayerScreen` | `0x1bdb2f` | 12 | `0x3b11fd` | `0x1bdb3b` = `LevelTitleLabel` |
| 8 | `JoinIpPopUp` | `0x1bdaf7` | 11 | `0x3b0a94` | `0x1bdb02` = `AddBotPopUp` |
| 9 | `AddBotPopUp` | `0x1bdb02` | 11 | `0x3b2d6f` | `0x1bdb0d` = `ConfirmPlay` |
| 10 | `ConfirmPlay` | `0x1bdb0d` | 11 | **无**（见更正 ②） | `0x1bdb18` = `ExitPopUp` |
| 11 | `ExitPopUp` | `0x1bdb18` | 9 | **无**（见更正 ②） | `0x1bdb21` = `JoinLevelPopUp` |
| 12 | `JoinLevelPopUp` | `0x1bdb21` | 14 | `0x3b0df8` | `0x1bdb2f` = `PlayerScreen` |
| 13 | `ConnectingPopUp` | `0x1bda23` | 15 | `0x3a6c58` | `0x1bda32` = `GameSetup` |
| 14 | `EndGamePopUp` | `0x1bda3b` | **12** | `0x3a8122`（`mov $0xc,%edx`） | `0x1bda47` |
| 15 | `DisconnectedPopUp` | `0x1bda73` | 17 | `0x3a7555` | `0x1bda84` |

命令键（12 条，全部由**两条窗口立即数**还原；`len` 由第二窗口偏移 `len-8`（7 字节的取 `len-4`）钉死）：

| # | 键 | len | 第一窗口 | 第二窗口 | 第二窗口偏移 |
|---|---|---|---|---|---|
| 1 | `player_settings` | 15 | `0x3b0886` | `0x3b0894` | `0x7` |
| 2 | `start_level` | 11 | `0x3b16b7` | `0x3b16c5` | `0x3` |
| 3 | `multiplayer` | 11 | `0x3b16dd` | `0x3b16eb` | `0x3` |
| 4 | `create_game` | 11 | `0x3b1703` | `0x3b1711` | `0x3` |
| 5 | `toggle_mute` | 11 | `0x3b1729` | `0x3b1737` | `0x3` |
| 6 | `exit_campaign` | 13 | `0x3b1ed4` | `0x3b1ee2` | `0x5` |
| 7 | `ask_exit_play` | 13 | `0x3b1efa` | `0x3b1f08` | `0x5` |
| 8 | `cancel_add_bot` | 14 | `0x3b21d0` | `0x3b21de` | `0x6` |
| 9 | `exit_lan_lobby` | 14 | `0x3b21f6` | `0x3b2204` | `0x6` |
| 10 | `cancel_join_ip` | 14 | `0x3b221c` | `0x3b222a` | `0x6` |
| 11 | `add_bot` | 7 | `0x3b0a50`（4 字节） | `0x3b0a59`（`xor 0x3(%r12)`） | `0x3` |
| 12 | `join_ip` | 7 | `0x3b0a6b`（4 字节） | `0x3b0a74`（`xor 0x3(%r12)`） | `0x3` |

比较形态（**观测**，全部 12 条一致）：
`movabs $<前 8 字节>,%rax; xor (%r12),%rax` → `movabs $<后 8 字节>,%rcx; xor 0x<len-8>(%r12),%rcx`
→ `or %rax,%rcx; je <命中>`。两个窗口的**并集**正好覆盖整条键，所以 `len` 不是抄来的。

#### 15.10.3 四条更正（与 §15 旧结论冲突，以此为准）

**① `EndGamePopUp` 是 12 字节，不是 11**（§15.2 第 14 行已就地改）。
判据有两条且互证：名字内容 12 字节；唯一使用点 `0x3a811b`（`lea`）+ `0x3a8122`
（`mov $0xc,%edx`）。旧表的 11 会切出 `"EndGamePopU"`，`get_node` 永远查不到。

**② `ConfirmPlay`(11) / `ExitPopUp`(9) 没有「代码里用的长度」证据，而且这里有个未解之谜**：
全库只有**一处** `lea 0x1bdb0d`，在 `0x3b3783`，紧跟的长度是 **`0x14 = 20`**；
它随后 `add $0x1638,%rbx`（把 `rbx` 变成**屏表**指针，`0x3b377c`）、`mov %rbx,%rdi`，
再**尾调用** `0x3af700`（按名查屏）。而 `0x1bdb0d` 起的 20 字节正好是
`"ConfirmPlayExitPopUp"`（两个屏名在 `.rodata` 里紧邻）。
⇒ 两种读法都成立，**未定**：(a) 原版这一处的键写成 20 字节，于是查不到、这一支**无动作**；
(b) 屏表里真有一个 20 字节的键（那就不是「15 个屏」）。缺的证据：屏表的**插入点**
（`map.insert` 的 `(ptr,len)` 实参）；拿到插入点就能一次定案。
`ExitPopUp` 更彻底：全库**没有** `lea 0x1bdb18`，只能用 `0x1bdb0d + 11` 得到。

**③ `0x3af850` 的语义是「切屏」，`[rdi+0x18]` 是**屏表长度**，不是广告持有者**（§15.5 已就地更新口径）。
两条独立证据：
1. 同一条 `[rdi+0x18]` 在 `0x3af868` 取出后，`0x3af8d7` 是 `dec %rbx; je <结束>`
   —— 它被当**循环计数**用，不可能是 `Option<Gd<..>>` 的判空；
2. `0x3a57ba` 用**同一个偏移**（`[self+0x1650]` = `0x1638 + 0x18`）判「屏表空，跳过」。
   `0x3af71f` 又用 `+0x20`（= `0x1658`）取 hasher ⇒ 这三处描述的正是**同一张屏表**。
   广告持有者在**第 4 实参**那个 16 字节捕获态的 `+8`：`0x3a7bed` 那个调用点先把
   `%r12(=0)`/`%r14(=0)` 存到 `0x48(%rsp)`/`0x50(%rsp)`、把 `1` 存到 `0x40(%rsp)`
   （`0x3a7bc8`–`0x3a7bd7`），再 `lea 0x40(%rsp),%rcx` 传进第 4 实参。
而函数主体是「屏表逐项 `set_visible(name == 目标)`」⇒ **它就是 `docs/19` §③ 记成
`show_screen` 的那个动作**（旧记的两个入口地址都不是函数边界，§15.8 已说）。
⚠️ 仍然只是**推断**的部分：`call *0x9ba978(gd, bool)` = `set_visible`、
`call *0x9baa20(gd) -> bool` = `is_visible`、`0x39fe10` = 组 `call_deferred` 的 `Variant`。
三者的 GOT 槽都没定名（§15.9 同一缺口）。

**④ 第 7 条命令键是 `ask_exit_play`，不是 `exit_ask_play`**（§15.6 表已就地改）。
立即数是 `0x746978655f6b7361`（`"ask_exit"`）+ `0x79616c705f746978`（`"xit_play"`），
指令 `0x3b1efa` / `0x3b1f08`。词序写反会让这条命令永远不被命中。

#### 15.10.4 `0x3af850` 的完整决策表（观测）

`fn(self, name: &str, capture: &{ +0x00: bool, +0x08: <Godot 对象> })`，对屏表
**每一个条目** `entry`：

| 条件 | 动作（指令） | 可见性 |
|---|---|---|
| `entry == name` 且 `entry != "PlayScreen"` | — | `true`（`0x3afa54` → `0x3af8a7`） |
| `entry == name == "PlayScreen"`，闸门未开 | — | `true`（`0x3afa8e`） |
| 同上，闸门开（`capture[0] && is_visible(PlayScreen)`） | `call_deferred("destroy_ad_view")` @`0x3afad5` | `true` |
| `entry == "background"` 且未命中 | **什么都不做**（`0x3af980` → `0x3af8cd`） | 不变 |
| `entry == "PlayScreen"` 且未命中，闸门未开 | — | `false`（`0x3af9d1`） |
| 同上，闸门开 | `call_deferred("create_ad_view")` @`0x3afa1e` | `false` |
| 其它未命中 | — | `false`（`0x3af8a0`） |

⇒ 一句话：**只有被点名的那个屏可见**；`PlayScreen` 进出对局时顺带拆/建广告；
`background` 完全不动。**顺序**：`call_deferred` 在 `set_visible` **之前**
（两个分支都是先组 `Variant`、再跳回 `0x3af8a7` 写可见性）。
⚠️ 条目**顺序**未知（`HashMap` 迭代序）；本仓库把「顺序」留成调用方的输入。

#### 15.10.5 实现轮新量到的（§15 里没有的）

- **`[+0x1704]`（当前屏）的全部 8 个写入点，全在 `0x3b0830` 内**（原先只知道 5 个值）：
  `0x3b08ac`(3)、`0x3b0931`(0)、`0x3b0996`(1)、`0x3b237f`(4)、`0x3b2caa`(0)、
  `0x3b2e24`(0)、`0x3b3168`(2)、`0x3b3416`(0)。⇒ §15.3「只在 `0x3b0830` 里被写」**成立**。
- **`[+0x1705]`（对局状态）的写入候选值至少 {1, 6, 7, 8}**（§15.9 说“只知道 6”）：
  紧邻 `0x3a8a09` 的跳转前各有 `mov $N,%r15b` —— `0x3a6e7b`=8、`0x3a6f59`=6、
  `0x3a7467`=1、`0x3a7f53`=7、`0x3a89a0`=1（`0x3a7f53` 之后 3 条指令就是
  `jmp 0x3a8a09`，定值最硬）。**枚举定义仍未解**：一个字段至少 4 个值，
  而 `docs/14` §2 的 6 态枚举最大下标只有 5。
- 构造体热路径的其余立即数写（`0x3a22f0`–`0x3a2baa`）：
  `[0x00]=0`、`[0x20]=0`、`[0x90]=3`、`[0xa0]=3`、`[0x40]=0`、`[0x58]=0`、`[0x70]=0`、
  `[0x15c8]=0`（主开关）、`[0x16fc]=0`、`[0x15d8]=0x3b9aca00`（=1e9）。
  ⚠️ 最后一条与 §15.3「`+0x15d8` 是 `String`+长度落点」**两说未合**（登记为未知）。
- `_ready` 的三个 `get_node` 都是**同一个**调用目标 `0x3acb00`，`lea`/长度/调用三地址：
  `ScreensControl` `0x3a3b07`/`0x3a3b16`/`0x3a3b1c`、
  `IAP_holder` `0x3a4129`/`0x3a4138`/`0x3a413e`、
  `Ad_holder` `0x3a41bc`/`0x3a41cb`/`0x3a41d1`。
- `0x3af700` 的**唯一**两处使用：`0x3b2d77`（`("AddBotPopUp", 11)`，表指针来自
  `add $0x1638,%rbx` @`0x3b2d61`）与 `0x3b37a3`（尾调用，见更正 ②）。

#### 15.10.6 仍未落的（诚实清单，与 §15.9 合并去重）

| 缺口 | 缺什么证据 |
|---|---|
| `[+0x1705]` 枚举定义（{1,6,7,8} 各是什么） | `0x3a8a09` 处 `r15b` 的全函数定值分析 |
| `0x3b0830` 其余命令键（已 12 条） | 131 KB 反编译逐分支审计 |
| 屏表的**插入点**与键集（决定更正 ② 的读法） | `map.insert` 的 `(ptr,len)` 实参 |
| `background` 键的来源（全库无字面量、无重定位指向它） | 同上；三者一起解 |
| `0x9ba978` / `0x9baa20` / `0x9baa10` / `0x9ba720` / `0x39fe10` 的引擎侧符号 | 运行时读 GOT 槽 |
| 屏表 V 的确切类型、`upcast_ref` 目标类名 | `0x406250` 第 2 实参逐条解 |
| `HashMap` 的**迭代序**（切屏时 `set_visible` 的调用顺序） | 运行时观测 |
| `0x3a22f0` 是构造器还是 `Clone::clone` | 它的调用点 + gdext 虚表槽 |
| `[+0x15d8]` 的 `1e9` 与「String+长度落点」两说 | 运行时观测该字段的读写 |
| `0x3a5760` 的 Rust 形状 | 按「一个个内联块各自的入口」拆，不是按 Ghidra 的函数边界 |

---

## 16. `graphwar_setup_control.rs` 规格（本轮新增：37 个锚函数 / 四格模式 / 时间档 / 玩家条目）

> **本节解决什么**：`docs/149` 记本模块 **21** 个可归属函数、**0** 个已做，`docs/02` §17 记「未实现」。
> 本轮把这一屏解到"能照着写"的粒度：37 个 `panic::Location` 锚函数**全部登记并按 7 类分工**，
> 其中 **四格模式 / 时间档 / 玩家条目 / Start-Cancel** 四类落进
> `src/graphwar_setup_control.rs`（23 条验收在 `tests/graphwar_setup_control.rs`）。
>
> **地址口径**：本节一律 **ELF 虚拟地址**；Ghidra/REA 地址 = 本地址 + `0x100000`（**只加一次**）。
> 读字节时注意 `.rodata` 与 `.text` 的平移**不一样**（`readelf -SW`：
> `.rodata` Address `0x19c000` = Off `0x19c000`；`.text` Address `0x37b940` = Off `0x37a940`，差 `0x1000`）。
> 把 `.text` 的平移套到 `.rodata` 上，会读出 ` lenVAR_XVAR_YVAR_DYADD` 这种垃圾（本轮真踩过）。

### 16.1 分母：37 / 70 / 21 —— 三个数各有各的口径

| 数 | 含义 | 档位 / 判据 |
|---|---|---|
| **70** | `src/graphwar_setup_control.rs` 的 `panic::Location` 结构体个数（= 矩阵的 `anchors`） | **逐位**：每个 `(file.ptr, file.len)` 槽的 `+8` 必须等于文件名字面量长度（`src/graphwar_setup_control.rs` = 29 字节），`line/col` 是合理正整数 |
| **37** | 引用这些 Location 的指令所在的 Ghidra 函数入口个数（= `fixtures/module_funcs.tsv` 的本模块行数） | **逐位**：37 条与 fixture **逐条相同、同序** |
| **21** | 矩阵的 `orig_funcs`：把"一个函数引用两个模块 Location"（`contested`，本模块 **8** 个）压平到贡献锚点最多的那个模块之后的数 | **观测**（`tools/coverage_matrix.py` 的口径） |

⇒ **本模块自己的分母是 37**，"21"是跨模块去重后的口径；两个数都写进
`LOCATION_ANCHOR_COUNT` / `ATTRIBUTED_FUNCS_AFTER_FLATTENING` 两个常量并在测试里钉住。

#### 16.1.1 ⚠️ `tools/class_probe.py` 的函数归属偏移 `0x1000`（本轮实测的工具坑）

`class_probe` 的 `Index.funcs_ref_slot()` 把 `analysis/xrefs.pkl` 里的**指令虚地址**
又过了一次 `off2va()`：

```python
# tools/class_probe.py（现状）
def funcs_ref_slot(self, slot):
    for t in self.xrefs.get(slot, ()):
        va = self.off2va(t[0])       # ← t[0] 已经是 vaddr，再过 off2va 就 +0x1000
        f  = self.func_of(va)
```

`xrefs.pkl` 的 `(insn_addr, kind, mnemonic, op_str)` 里 `insn_addr` **已经是虚地址**
（`analysis/build_index.py` 用 `md.disasm(blob, t['addr'])` 生成），而 `off2va` 对落在
`.text` 的地址会再加 `0x1000`（`.text` 的 Address 比 Off 大 `0x1000`）。
**后果**：`tools/class_probe.py module graphwar_setup_control` 打印的 `funcs:` 列表
是"真实函数**再高 0x1000**"的那个函数。实测症状：它给出 23 个函数，
与 `fixtures/module_funcs.tsv` 的 37 个**只有 6 个重合**，一眼看像"两份数据打架"。

正确做法（本节全部数据用的就是它）：**直接 `bisect` 虚地址**归属，不要过 `off2va`。
三步复算，结果与 fixture 逐条相同：

1. 取 `.data.rel.ro` 里 `addend == 0x1a4a55`（`src/graphwar_setup_control.rs` 字面量；
   全库**只有这一份**，70 个 `R_X86_64_RELATIVE` 槽**全部**指向它）的
   `R_X86_64_RELATIVE` 槽 = Location 的 `file` 槽（70 个）；
2. 每个槽 `+8` 是长度（必须 = 29），`+16` 是 `(line, col)`；
3. `xrefs.pkl[槽]` 给出**指令虚地址**，直接 `bisect(funcstarts_ghidra_va.pkl, vaddr)` 得函数入口
   ⇒ 37 个，逐条等于 `fixtures/module_funcs.tsv`。

`tools/module_map.py` 早就是对的（`i = bisect.bisect_right(funcstarts, insn_addr) - 1`），
所以 **fixture 可信、`class_probe` 的 `funcs:` 列不可信**（它的 `literals` 列不受影响）。

### 16.2 37 个函数的分工（观测，逐个量过）

`size` = 到下一个 Ghidra 函数入口的距离；`Location 行` = 该函数引用的 `src/graphwar_setup_control.rs:line:col`。
"已落码"指本轮 `src/graphwar_setup_control.rs` 有对应实现。

| # | 函数 | size | Location 行 | 分类 | 档位 | 已落码 |
|---|---|---|---|---|---|---|
| 1 | `0x4b88d0` | 384 | `95:1` | 虚方法/属性胶水（槽 `0x9bb670`） | 观测形状 / 语义**未知** | — |
| 2 | `0x4b8a50` | 384 | `95:1` | 虚方法/属性胶水（槽 `0x9bb678`） | 同上 | — |
| 3 | `0x4bcc60` | 544 | `46:10` | 虚方法/属性胶水（槽 `0x9ba278`） | 同上 | — |
| 4 | `0x4bce80` | 432 | `95:1` | 虚方法/属性胶水（槽 `0x9ba290`） | 同上 | — |
| 5 | `0x4bd030` | 592 | `608:1` | 虚方法/属性胶水（槽 `0x9ba288`） | 同上 | — |
| 6 | `0x4bd280` | 432 | `95:1` | 虚方法/属性胶水（槽 `0x9ba280`） | 同上 | — |
| 7 | `0x4c1e40` | 768 | `178:127,180:49` | 玩家名 `PlayerNameLabel` | 观测 | — |
| 8 | `0x4c2140` | 1712 | `187:121,189:42` | 段位图标 `RankIcon` + `set_texture` | 观测 | — |
| 9 | `0x4c27f0` | 928 | `462:133,464:26` | 事件/属性回调（哪个事件**未知**） | 推断 | — |
| 10 | `0x4c2b90` | 864 | `310:45,312:57` | **模式面板同步**（命令总线 + 列表重建都调） | 观测 | ✅ 四格取值 |
| 11 | `0x4c2ef0` | 5568 | `202:47,204:41,206:49,212:148,227:120,235:120,243:118,251:122` | **士兵图标槽**（`SoldierIconContainer`→`Skin/Face/Hat/EmptyIcon`） | 观测 | ✅ 5 槽 |
| 12 | `0x4c44b0` | 928 | `494:133,496:62` | 事件/属性回调 | 推断 | — |
| 13 | `0x4c4850` | 848 | `474:133,476:49,486:60` | 事件/属性回调 | 推断 | — |
| 14 | `0x4c4c20` | 3888 | `257:133,260:145,267:149,274:145,281:151` | **一行的四个按钮**（换队/踢人/加兵/减兵） | 观测 | ✅ 增删/换队 |
| 15 | `0x4c5b50` | 176 | `263:72` | `impl` 薄壳 | 观测形状 / 语义**未知** | — |
| 16 | `0x4c5c00` | 176 | `277:72` | `impl` 薄壳 | 同上 | — |
| 17 | `0x4c5cb0` | 176 | `284:72` | `impl` 薄壳 | 同上 | — |
| 18 | `0x4c5d60` | 176 | `270:72` | `impl` 薄壳 | 同上 | — |
| 19 | `0x4cf670` | 9776 | `652:127,744:88,745:82,746:88,762:26,781:26` | **接线**（`_ready` 级）：11 个 `get_node` + 3 个信号 + 2 个事件 | 观测 | ✅ 节点名表 |
| 20 | `0x4d1ca0` | 176 | `696:76` | `impl` 薄壳 | 观测形状 / 语义**未知** | — |
| 21 | `0x4d1d50` | 176 | `687:76` | `impl` 薄壳 | 同上 | — |
| 22 | `0x4d1e00` | 96 | `705:76` | `impl` 薄壳 | 同上 | — |
| 23 | `0x4d1e60` | 176 | `669:76` | `impl` 薄壳 | 同上 | — |
| 24 | `0x4d1f10` | 1600 | `728:76` | `GraphwarSetupControl` 泛型体（`contested`→texture_loader） | 观测 | — |
| 25 | `0x4d2550` | 176 | `678:76` | `impl` 薄壳 | 未知 | — |
| 26 | `0x4d2600` | 2416 | `793:82,797:82,802:26` | `GraphwarSetupControl` 泛型体（`contested`→texture_loader） | 观测 | — |
| 27 | `0x4d2f70` | 528 | `772:26` | `GraphwarSetupControl` 泛型体 | 观测 | — |
| 28 | `0x4d3180` | 15936 | `100:45,103:41,128:45,131:41,144:149,161:149,298:149,329:41,356:45,384:75,442:60,450:51,527:51,532:53,536:50,813:137,815:62,824:70,826:68,828:74,831:58` | **列表重建**（Team1/Team2/PlayerList/GameIpLabel，`contested`） | 观测 | — |
| 29 | `0x4d88b0` | 176 | `705:76` | `impl` 薄壳 | 未知 | — |
| 30 | `0x4d8a00` | 256 | `284:72` | `impl` 薄壳 | 未知 | — |
| 31 | `0x4d8d30` | 256 | `263:72` | `impl` 薄壳 | 未知 | — |
| 32 | `0x4d8ff0` | 239 | `687:76` | `impl` 薄壳 | 未知 | — |
| 33 | `0x4d9270` | 256 | `277:72` | `impl` 薄壳 | 未知 | — |
| 34 | `0x4d9370` | 256 | `270:72` | `impl` 薄壳 | 未知 | — |
| 35 | `0x4d9500` | 240 | `696:76` | `impl` 薄壳 | 未知 | — |
| 36 | `0x4d95f0` | 240 | `669:76` | `impl` 薄壳 | 未知 | — |
| 37 | `0x4d96e0` | 320 | `678:76` | `impl` 薄壳 | 未知 | — |

**两条从表里直接读出来的结构**（观测）：

* 第 15–18 / 30–34 两组**行号成对重复**（263/277/284/270 与 669/678/687/696/705/728/772/793），
  且第二组每个都恰好比第一组大 `0x140`–`0x150` 字节、体内无字符串无调用
  ⇒ 这是 gdext `#[godot_api]` 展开出来的**成对薄壳**（`I*` 常量 + 方法体），不是业务函数。
  **语义未知**，本文件不猜它们对应哪个方法。
* 第 1–6 六个函数的入口地址**全都出现在 `.data.rel.ro` 的函数指针槽里**
  （`0x9bb670`/`0x9bb678` 两个相邻槽 + `0x9ba280`/`0x9ba278`/`0x9ba288`/`0x9ba290` 四个）
  ⇒ **观测**：它们是函数指针表（虚方法/回调）里的实现；
  **推断**：是 `Virtual` 的几个 `_ready`/`_process`/`_draw`/`_notification` 之一，
  **缺**"槽序号 ↔ gdext 版本 vtable 顺序"的对齐证据。

#### 16.2.1 `0x4cf670` 的 11 个 `get_node` 实参（逐条，观测）

判据 = `lea rcx,[rip+X]`（`.rodata`）+ 紧随的 `mov edx,<len>`；长度都是指令立即数：

| 节点名 | `.rodata` | len | 用途 |
|---|---|---|---|
| `RoomNameLineEdit` | `0x19ea20` | 16 | 房间名（连 `on_text_change` ← `text_changed`） |
| `AxisModeButton` | `0x1c2eb2` | 14 | 第 1 格：坐标轴模式 |
| `FunctionModeButton` | `0x1c2ec0` | 18 | 第 2 格：`f(x)` / 微分方程 |
| `TurnModeButton` | `0x1c2ed2` | 14 | 第 3 格：顺序 / 同时回合 |
| `TurnTimeButton` | `0x1c39b9` | 14 | 第 4 格：回合时限（**注意**：`JoinLevelPopUp` 里同一个按钮叫 `TimeModeButton`，`docs/16` §2.1.3 已记） |
| `LockButton` | `0x1c39c7` | 10 | 房间上锁 |
| `ChatButtonSetup` | `0x1c39d1` | 15 | 打开聊天 |
| `ChatButtonPlay` | `0x1c39e0` | 14 | 对局里同一个聊天键（同一份 `impl` 复用） |
| `ChatPopUp` | `0x1c39ee` | 9 | 聊天浮层 |
| `ChatBox` | `0x1c39f7` | 7 | 聊天记录 |
| `ChatLineEdit` | `0x1c39fe` | 12 | 聊天输入（连 `on_chat_submitted` ← `text_submitted`） |

另连 `on_visibility_change` ← `visibility_changed`（`0x1c2d1f`/`0x1c39a7`），
并订阅事件 `game_state_info_event`（`0x1c2c05`）、`chat_message_event`（`0x1c3a18`）。

⚠️ **4 个模式按钮没有连 `pressed`**：函数体里连的信号只有 `text_changed`、
`visibility_changed`、`text_submitted` 三个。⇒ **推断**：点模式走按钮的 `input_text`
→ `root_node` 命令总线（`docs/16` §15.6）。缺：`.tscn` 里这 4 个按钮的 `input_text` 值 +
那 131 KB 反编译里对应分支。

### 16.3 四格模式：两张**逐位**表

#### 16.3.1 图标 id 表（`0x4c9190`，函数 size `0x6130`）

这个函数（矩阵记在 `graphwar_texture_loader.rs`，本节只**借用**数据、不声称归属）
把 "id → 纹理" 一条条塞进表里，形态固定：

```text
lea    rdi,[rip+X]        ; X = res://… 字面量
mov    esi,<len>          ; 长度立即数
call   0x4b1c80
mov    edx,<id>           ; id（为 0 时是 xor edx,edx）
call   0x43f7c0
```

全函数 **45** 条 `(path, len, id)`。四格模式用到的 14 条（**逐位**）：

| 类 | id | 路径 | len | `call 0x4b1c80` 处 |
|---|---|---|---|---|
| Axis（`+0x3a`） | 0 | `res://axis_option_0.svg` | 23 | `0x4c9205` |
| | 1 | `res://axis_option_1.svg` | 23 | `0x4c928b` |
| | 2 | `res://axis_option_2.svg` | 23 | `0x4c9314` |
| | 3 | `res://axis_option_3.svg` | 23 | `0x4c939d` |
| Function（`+0x38`） | 0 | `res://fx_option.svg` | 19 | `0x4c9493` |
| | 1 | `res://dx_dy_option.svg` | 22 | `0x4c9519` |
| Turn（`+0x39`） | 0 | `res://turns_sequential.svg` | 26 | `0x4c960f` |
| | 1 | `res://turns_simultaneous.svg` | 28 | `0x4c9695` |
| Time（`+0x3b`） | 0 | `res://turn_30s.svg` | 18 | `0x4c978b` |
| | 1 | `res://turn_1m.svg` | 17 | `0x4c9811` |
| | 2 | `res://turn_2m.svg` | 17 | `0x4c989a` |
| | 3 | `res://turn_3m.svg` | 17 | `0x4c9923` |
| | 4 | `res://turn_5m.svg` | 17 | `0x4c99ac` |
| | 5 | `res://turn_inf.svg` | 18 | `0x4c9a35` |

⚠️ `res://turn_30s.svg` 与 `res://turn_inf.svg` 在 `.rodata` 里**和邻居首尾相接**
（`0x1c3236`、`0x1c328c`），`strings` 会粘成一条；只有按长度立即数切才拿到 18 字节。
同样，`0x962720`（`OnlyMain`）与 `0x962750`（`Timer30s`）在 `.data.rel.ro` 里也是"夹"在
别的结构中间的，判据是槽 `+8` 的长度字段。

#### 16.3.2 名字表（`.data.rel.ro` `0x9626f0`…`0x9627a0`，12 项）

**这是"判别值 → 线上名字"的表**：12 个 `(ptr,len)` 连续排列，表尾紧接
`src/graphwar_name_manager.rs` 的 Location（`0x9627b0`）。**逐位**（addend 与 `+8` 互校）：

| 段 | 判别值 | 名字 | 槽 | 字面量 | len |
|---|---|---|---|---|---|
| Function | 0 | `NormalFunction` | `0x9626f0` | `0x1c640f` | 14 |
| | 1 | `DiffEqFunction` | `0x962700` | `0x1c641d` | 14 |
| Axis | 0 | `NoAxis` | `0x962710` | `0x1c642b` | 6 |
| | 1 | `OnlyMain` | `0x962720` | `0x19f3a0` | 8 |
| | 2 | `EveryFive` | `0x962730` | `0x1c6431` | 9 |
| | 3 | `EveryUnit` | `0x962740` | `0x1c643a` | 9 |
| Time | 0 | `Timer30s` | `0x962750` | `0x19f768` | 8 |
| | 1 | `Timer1m` | `0x962760` | `0x1c6443` | 7 |
| | 2 | `Timer2m` | `0x962770` | `0x1c644a` | 7 |
| | 3 | `Timer3m` | `0x962780` | `0x1c6451` | 7 |
| | 4 | `Timer5m` | `0x962790` | `0x1c6458` | 7 |
| | 5 | `TimerInf` | `0x9627a0` | `0x19f1b0` | 8 |

`TurnMode` 的名字在另一处：`0x957d40` `SequentialTurns`(15)、`0x957d50` `SimultaneousTurns`(17)，
表尾紧接 `src/graphwar_color_manager.rs` 的 Location（`0x957d60`）。**逐位**。

⇒ **三处独立数据同序**：图标 id（0..5）、名字表段内下标（0..5）、`AxisMode` 那 4 条
与运行时 `AxisModeInfo` 的 21/21 对应。所以 `src/graphwar_setup_control.rs` 把
"枚举判别值 = 图标 id = 名字表下标" 当**数据表**实现，并把"判别值就是图标 id"这一步标**推断**。

### 16.4 时间档：时长 / 显示 / 循环

| 档 | 判别值 | 线上名 | 时长 | 进对局时显示的标签 | 档位 |
|---|---|---|---|---|---|
| 30 s | 0 | `Timer30s` | 30 s（1800 tick） | `00:29`（**推断**：同一规则的延伸） | 时长**观测**（抓包）/ 标签**推断** |
| 1 m | 1 | `Timer1m` | 60 s（3600） | `00:59`（**推断**） | 同上 |
| 2 m | 2 | `Timer2m` | 120 s（7200） | **`01:59`** | 时长**观测** / 标签**观测**（真帧） |
| 3 m | 3 | `Timer3m` | 180 s（10800） | **`02:59`** | 同上 |
| 5 m | 4 | `Timer5m` | 300 s（18000） | `04:59`（**推断**） | 同上 |
| ∞ | 5 | `TimerInf` | 无截止 | **未知**（没有观测到它显示什么） | 时长**观测** / 标签**未知** |

* **时长**：真实抓包（`docs/42`，已落地在 `src/graphwar_tick_handler.rs`）。
  `TimerInf` 线上把 `end_of_turn_tick` 发成 `0`。
* **显示规则 = 时长 − 1 秒**，`mm:ss` 两位补零：真帧观测到 `02:59`（3 分钟档）、`01:59`（2 分钟档），
  以及过程中的 `00:04`、`00:00` ⇒ 倒计时从 `时长-1` 走到 `00:00`。
  ⚠️ 30 s / 5 m 两档没有逐帧观测，标**推断**；无限档标**未知**（代码里返回 `None`，**不**编 `inf`）。
* **点击循环**：`docs/16` §2.1.5 的原话是"点击循环"。循环顺序取判别值序
  （依据：图标 id 序 = 名字表序），这一步**推断**；缺"从 5 档再点一次回到 30 s"的观测。

**交叉验证（本轮最有用的一条）**：`docs/16` §2.1.10 的按键实验说
"按 1 次 `TurnTimeButton` ⇒ `01:59`（2 分钟档）、按 2 次 ⇒ `02:59`（3 分钟档）"
⇒ 房间初值必须是 `Timer1m`（判别值 1）。这条与"新建房间首个 `GameInfo` 的
`time_mode="Timer1m"`"（`docs/09-network.md`）**互相独立**，两条都对得上
⇒ `ROOM_DEFAULT_OPTIONS.time = Timer1m` 判**观测**。

### 16.5 初值：新建房间（观测）vs 战役关（逐位）

| 格 | 新建房间（`LanLobby/CreateButton` → `GameSetup`） | 战役第 n 关（`JoinLevelPopUp`） |
|---|---|---|
| Axis | `EveryUnit`(3) | `LEVELS[n].modes.axis`（21 关里 3 出现 10 次、2/1/0 各若干） |
| Function | `NormalFunction`(0) | 0（关 0–15）/ 1（关 16–20，`DiffEqFunction`） |
| Turn | `SequentialTurns`(0) | 0，只有关 11、14 是 1（`SimultaneousTurns`） |
| Time | `Timer1m`(1) | **21/21 都是 5 = `TimerInf`** |

* 新建房间那一列：**观测**（`docs/09-network.md` 新建房间的首个 `GameInfo`）+ 16.4 的按键算术交叉验证。
* 战役那一列：**逐位**（`LEVELS[i].modes` 的四字节来自 `mov dword [rsp+0x58], <imm32>`）。
* **本轮升级**：`docs/10` §7 第 6 条原为**推断**（"5 = `TimerInf` 缺直接证据"）。
  名字表第 `6+5 = 11` 项就是 `"TimerInf"`（`0x9627a0`，**逐位**），
  与运行时 `TimeModeInfo` 21/21 = `TimerInf` 同向 ⇒ **该条升为逐位**。
  ⚠️ `src/graphwar_level_manager.rs` 的注释仍写"推断"（该文件不在本轮可改范围），
  但 `src/graphwar_setup_control.rs::TimeMode::from_level_byte` 已按逐位实现并有测试
  断言 21/21 都是 `TimerInf`。

### 16.6 玩家条目：5 个槽 / 4 个按钮 / 士兵数来自关卡数据

| 项 | 值 | 档位 |
|---|---|---|
| 每行最多几个士兵 | **5** | 观测（`docs/16` §2.1.5 的 `SoldierIconContainer/MarginContainer…5`；空槽画 `res://empty_soldier.svg`） |
| 槽里画什么 | `SkinIcon`/`FaceIcon`/`HatIcon`/`EmptyIcon` 四层 | 观测（`0x4c2ef0` 的 `get_node` 实参） |
| 加/减士兵 | `AddSoldierButton`/`RemoveSoldierButton`，**饱和在 5 / 0** | 观测（按钮存在）+ **推断**（饱和点由槽数推出） |
| 换队 | `ChangeTeamButton`，两队容器 `Team1Container`/`Team2Container` | 观测（容器名逐位）；换到对面 **推断** |
| 踢人 | `RemovePlayerButton` | 观测（按钮存在） |
| 难度档 | 0..=5（`AddBotPopUp` 六个按钮） | 观测（`docs/16` §2.1.6） |
| 战役局士兵数 | `SETUPS[n].soldiers` | **逐位**（`0x476320(mgr, player, N)` 的 `edx` 立即数） |
| LAN 局新玩家初始士兵数 | **未知** | 缺观测。`docs/143` 附 1 的教训：**别把 1 当默认值** |

⚠️ **`GameSetupPlayerEntry` 这个名字是任务书里的叫法，二进制里不存在**：
`GameSetupPlayerEntry` / `PlayerEntry` / `SetupPlayer` 在 `.so` 里 `strings` 全库 **0 命中**。
所以代码里的 `PlayerEntry` 只是"这一行可变状态"的建模，**不是**原版结构体名（已在代码注释里标明）。

### 16.7 Start / Cancel 的屏切换

| 动作 | 去哪 | 档位 / 判据 |
|---|---|---|
| `GameSetup` 的 ` Start ` | `PlayScreen` | **观测**（`Start` → `PlayScreen.visible=1`，`docs/16` §2.1.5） |
| ` Start ` 但**没有对手** | 什么都不做，**也不提示** | **观测**（`docs/16` §2.1.5 + `archive/22-live-game-harness.md` §5.1）。判据"两队都非空"是**推断**（缺每队一人的边界与 3 人 2:1 的观测） |
| `GameSetup` 返回键 | `LanLobby` | **推断**（进入路径唯一观测是 `LanLobby/CreateButton` → `GameSetup`；缺"按下返回键"的观测） |
| `JoinLevelPopUp` ` Start ` | `PlayScreen` | **推断**（`docs/16` §2.1.3 明写未按下验证） |
| `JoinLevelPopUp` ` Cancel ` | `Campaign` | **推断**（这一屏从 `Campaign/<level_n>` 打开是观测；命令总线有 `exit_campaign` 键是逐位） |

⚠️ `docs/16` §14.1 第 15 条（我们自己的客户端"无对手时给一句提示"）与这里的
`SilentNoOp` 冲突：**原版是纯静默**，提示是**自定**行为。

### 16.8 落码清单（`src/graphwar_setup_control.rs`）

| 项 | 内容 | 档位 |
|---|---|---|
| `MODULE_FUNCS[37]` | 37 个锚函数入口（= `fixtures/module_funcs.tsv`） | **逐位** |
| `LOCATION_ANCHOR_COUNT` / `ATTRIBUTED_FUNCS_AFTER_FLATTENING` | 70 / 21 | **逐位** / **观测** |
| `FN_*` 常量 7 个 + `FN_EVENT_CALLBACKS` | 主干函数的地址 + 判据写进注释 | **观测** |
| `AXIS_ICONS` / `FUNCTION_MODE_ICONS` / `TURN_MODE_ICONS` / `TIME_MODE_ICONS` | 14 条图标路径（含长度） | **逐位** |
| `MODE_NAME_TABLE[12]` / `TURN_MODE_NAMES[2]` | 判别值 → 线上名字 | **逐位** |
| `TimeMode`（6 档） | 判别值 / 图标 / 名字 / 时长 / 起始标签 / 循环 | 逐位 + 观测（时长、标签）+ 推断（循环顺序） |
| `AxisMode`/`FunctionMode`/`TurnMode` 的 `ALL`/`next`/`prev`/`icon_path`/`name` | 补在**本文件**（`level_manager` 的类型不动） | 同上 |
| `SetupOptions` | 四格状态 + `room_default()` + `from_level(n)` + `icons()`/`names()`/`timer_label()` | 观测 + 逐位 |
| `PlayerEntry` / `Team` | 5 槽 / 加兵 / 减兵 / 换队 / 难度档 | 观测 + 推断（饱和点、换队方向） |
| `campaign_soldier_count` / `campaign_turn_limit` | 关卡数据直读（修 `docs/143` 附 1 那个"默认 1"的错） | **逐位** |
| `start_action` / `can_start` / `exit_action` / `join_level_start` / `join_level_cancel` | 屏切换 | 观测 + 推断 |
| **未做** | 玩家列表重建（`0x4d3180`，15 936 B）、行构件的节点装配（`0x4c1e40`/`0x4c2140`/`0x4c2ef0`/`0x4c4c20` 的 gdext 调用序列）、事件回呼（`0x4c27f0`/`0x4c44b0`/`0x4c4850`）、24 个 `impl` 薄壳 | 见 16.11 |

### 16.9 就地更正与跨模块结论

1. **`docs/10` §7 第 6 条（`byte3 = 5` 的含义）由推断升为逐位**：名字表 `0x9627a0` = `"TimerInf"`。
2. **`tools/class_probe.py` 的 `funcs:` 列偏移 `0x1000`**（见 16.1.1）。本模块的 37 个函数
   一律以 `fixtures/module_funcs.tsv` 为准；`class_probe` 的 `literals` 列不受影响。
3. **`docs/16` §15.6 第 5 条说 `0x3b0830` 去 `PlayerScreen` 下取
   `LevelTitleLabel`/`AxisModeButton`/…/`TimeModeButton` 并 `set_button_icon`** ——
   这条**归属可疑**：那 5 个节点名只在 `JoinLevelPopUp` 的 `.tscn` 里有
   （`docs/16` §2.1.3），`PlayerScreen` 那一屏的节点名是 `PlayerNameLineEdit`/`PlayerShowcase`/…（§2.1.14）。
   另：§15.6 引的 `0x1bdb86`（长 15）**不是节点名**，是方法名 `set_button_icon`（逐字节核对过）。
   ⇒ **推断**：那次 `set_button_icon` 刷的是 `JoinLevelPopUp` 的四格图标（与本文 16.3 的 id 表是同一套）。
   缺：`get_node` 那个 `NodePath` 的父节点实参。
4. **`GameSetup` 的四格与 `JoinLevelPopUp` 的四格是同一套 id/名字表**：
   两屏的按钮名只差第 4 个（`TurnTimeButton` vs `TimeModeButton`），图标表只有一套（14 条）。
   ⇒ 推断（缺两屏各自的 set 指令）。§15.6 第 5 条若按 3 修正，这条就变成观测。

### 16.10 复现（需要仓库根的原版 `.so` + `analysis/*.pkl`）

```bash
# 0) 地址口径自检：.rodata vaddr == 文件偏移，.text 差 0x1000
readelf -SW ../libgraphwarrust.so | grep -E '\.(rodata|text) '

# 1) 70 个 Location → 37 个函数：bisect 虚地址（不要过 off2va）
#    见 16.1.1 的三步；产物应与 fixtures/module_funcs.tsv 的 37 行逐条相同
../.venv/bin/python - <<'PY'
import bisect, pickle, struct, sys
sys.path.insert(0, '../analysis')
from elf import Elf
E = Elf('../libgraphwarrust.so'); d = E.data
SECS = [(s['addr'], s['offset'], s['size']) for s in E.sections if s.get('addr') and s.get('offset')]
def rd(va, n):
    for a, o, sz in SECS:
        if a <= va < a + sz: return d[o + (va - a): o + (va - a) + n]
rel   = pickle.load(open('../analysis/relocs.pkl', 'rb'))
xrefs = pickle.load(open('../analysis/xrefs.pkl', 'rb'))
starts = sorted(pickle.load(open('../analysis/funcstarts_ghidra_va.pkl', 'rb')))
NAME = b'src/graphwar_setup_control.rs'
# .rodata 的 vaddr == 文件偏移（readelf 自检），所以直接按偏移找字面量
ro = next(s for s in E.sections if s['sname'] == '.rodata')
blob = d[ro['offset']:ro['offset'] + ro['size']]
addr = ro['addr'] + blob.index(NAME)
print('literal @0x%x' % addr)
funcs = set()
for slot, ents in rel.items():
    if any(t == 8 and ad == addr for t, ad in ents):
        if struct.unpack('<Q', rd(slot + 8, 8))[0] != len(NAME):   # 长度自校验
            continue
        for insn_va, kind, mn, ops in xrefs.get(slot, ()):
            i = bisect.bisect_right(starts, insn_va) - 1
            if i >= 0: funcs.add(starts[i])
print('%d functions' % len(funcs))
print(' '.join('0x%x' % f for f in sorted(funcs)))
PY

# 2) 图标 id 表：capstone 扫 0x4c9190..0x4cf2c0 的 (lea rdi, mov esi, call 0x4b1c80,
#    然后 call 0x43f7c0 前的 mov/xor edx) —— 45 条，其中 14 条是四格（本文 16.3.1）

# 3) 名字表：.data.rel.ro 0x9626f0..0x9627b0 逐 0x10 取 addend + 长度（本文 16.3.2）

# 4) 代码侧验收（回归保护，不是复原证据）
cargo test --offline --test graphwar_setup_control
```

### 16.11 本节没解出来的（照实写）

| 缺口 | 缺什么证据 |
|---|---|
| 4 个模式按钮点下去的信号/命令 | `.tscn` 里 `AxisModeButton`/`FunctionModeButton`/`TurnModeButton`/`TurnTimeButton` 的 `input_text`；`0x3b0830` 131 KB 体里的对应分支（已知 12 个键，`docs/16` §15.6） |
| `0x4c2b90` 到底按什么刷图标 | 它体内"哪个枚举 → 哪个 id"的指令级证据（现在只有形状 + id 表两条同向） |
| 6 个函数指针槽方法（`0x4b88d0`…）是谁 | `0x9bb670`/`0x9ba278…` 这几张 vtable 的槽序 ↔ gdext 版本 `Virtual` 方法的对齐 |
| 24 个 `impl` 薄壳的语义 | 同上；另外它们的 `Location` 行号（263/277/284/270…）能反推源文件结构，但没有方法名 |
| 3 个事件回呼（`0x4c27f0`/`0x4c44b0`/`0x4c4850`）处理哪个事件 | 它们体内只引 `GraphwarRootNode` 类型名与 Display panic；缺事件名字面量 |
| `TimerInf` 时计时器显示什么 | 真机进无限档对局的一帧 |
| LAN 局新玩家初始士兵数 | 真机"新玩家进房"后的 `SoldierIconContainer` 截图 |
| 玩家列表重建（`0x4d3180`）的执行序 | 它 15 936 B、211 个直接调用；要先把 `Team1/Team2` 两条分支的节点装配逐条对完 |
| `user://local_ip.config` 里那个 `1` | `0x4d3180` 读它之后的消费者（`docs/10` §7 第 11 条） |
| 名单排序 / 玩家表是否 `ScrollContainer` | 场景层（`.tscn`）证据，不在 `.so` 里 |

---

## 17. `graphwar_game_client.rs` 规格（本轮新增：36 个可归属函数的分类 + 两条命令链）

> **本节解决什么**：`graphwar_game_client.rs` 是全库**归属函数最多**的模块
> （`docs/coverage-matrix.tsv` 的 36 条），但它此前**一条规格都没有**——
> `docs/02` §17 只写了「只有 `docs/archive` 的调用形态观测」，`src/` 里是占位骨架。
> 本节给出：36 条**怎么数出来的**、它们**分成哪 8 类**、其中被解到「能照着写」的三条链
> （命令入口 / 8 种请求事件 / `add_bot_N`），以及**没解出来的诚实清单**。
>
> **地址口径**：一律 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + `0x100000`（只加一次）。
> 本节每个「函数入口」都是 `analysis/funcstarts_ghidra_va.pkl` 的边界，
> 关键几条另外用 `objdump` 逐指令核过（判据里的指令地址就是核过的那些）。
> 落码：`src/graphwar_game_client.rs`（纯逻辑层）+ `tests/graphwar_game_client.rs`。

### 17.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 可归属函数 | **36**（`tools/coverage_matrix.py` 口径），其中 **19** 条同时引用别的模块的 Location（`contested`），**68** 条函数体内引用到本模块的 Location | 观测（脚本可复现） |
| 分类 | **8 类**：请求事件构造 16、连接/后端 5、状态查询 4、对局内动作 3、setup 回调 3、回放/远端 3、命令入口 1、事件装配 1 | 观测（类别名是推断） |
| 模块干什么 | 客户端侧**把动作变成 `Event` 投进发送队列** + **把 14 个 manager 与 3 个事件名接起来** | 推断（依据见 §17.4–§17.7） |
| 命令入口 `0x514a10` | 只认两条键：`start_game`（投 `Event` #31）、`fire`（转 `0x516510`）；其余立即返回 | **观测**（立即数逐条量到） |
| 8 种请求事件 | 判别值 **11/12/16/17/18/19/20/21**，每种**两份实例化**（共 16 个函数），载荷是一个 i32 | **观测**（立即数 + 槽位相邻） |
| `add_bot_0..5` | 六条键 → `0x516970` 收到 **3/10/25/100/250/1000** | **观测**（六个 `mov $N,%esi`） |
| `0x517690` | 订阅 **3** 个事件名 + 装配 **14** 个 manager | **观测**（3 条 `lea`+长度立即数 + 14 个 `call`） |
| 屏名 / 命令总线 12 条键 | **不在本模块**（在 `graphwar_root_node.rs`，§15） | 观测（否定） |

### 17.2 36 这个数怎么来的（可复现）

口径与 `tools/coverage_matrix.py` 完全一致：`core::panic::Location` 的
`(file, line, col)` 从 `.rela.dyn` 的 `R_X86_64_RELATIVE`（addend = 文件名字面量）
还原 → capstone 扫 `.text` 找引用 → 落到 **Ghidra 函数边界** →
一个函数引用多模块时按「锚点数最多、并列取字典序」压平。

复算结果：**本模块 36 条可归属 / 19 条 contested / 68 条函数体内引用到本模块**。
`python3 tools/coverage_matrix.py` 打出的 `orig_funcs` 就是这 36。

⚠️ **和旧数字的差别（就地说明，不改旧文）**：

* `fixtures/module_funcs.tsv` 里本模块有 **68** 行 —— 那是「panic 串启发式」的粗表，
  既不是 36 也不是「模块规模」，而且**不能**用来判函数身份。
* `docs/02-module-map.md` §8 的表把本模块记成 **68 个归属函数 / 311 个引用点**，
  那是**同一套 `module_funcs.tsv` 口径**；§17 又写 **36/59**。
  ⇒ 同一份文档里两个数来自两套口径，**36 是这套脚本的口径**，本节一律用 36。

### 17.3 分类：8 类（每类一句话 + 证据）

| # | 类 | 个数 | 一句话 | 证据（地址 / 调用者 / 字符串） |
|---|---|---|---|---|
| 1 | **请求事件构造** | **16** | 把 `{tag=0x8000…\|判别值, +0x8=i32}` 这条 0x48 字节的 `Event` push 进 `[[self]+0x1f8]` 的 `Vec`（`+0x18`） | 判别值立即数 `0x4d8a4b` 等 16 处；`call *0x9bac80`（`0x3d3530` = `Vec::push`）；槽位 `0x95eff0…0x95f330` 成对；调用者 `graphwar_setup_control.rs` 的 `0x4c4c20`/`0x4cf670`，其体内有 `ChangeTeamButton`/`RemovePlayerButton`/`AddSoldierButton`/`RemoveSoldierButton` |
| 2 | **连接与后端** | 5 | `0x515590`/`0x5175b0` 构造 `Event` **#22 `game_name_request_event`**；`0x515590`/`0x5159a0` 直接 `call` `graphwar_backend.rs`；`0x516370`/`0x518320` 直接 `call` `graphwar_client_connection_handler.rs` | `0x5157d3` / `0x51763e` 的 `movabs $0x8000000000000016`（判别值 22）；直接 `call` 目标：`0x514d80`(`graphwar_backend.rs`，两处 `0x5156eb`/`0x515aa5`)、`0x4592d0` 与 `0x45a340`(`graphwar_client_connection_handler.rs`，`0x5163c7`/`0x518377`) |
| 3 | **状态查询** | 4 | 一律「取 `+0x40` 句柄 → `incq 0x10` → 调内层一次 → `decq`」，内层是 `0x46aa50`/`0x46aea0`/`0x46b180` | `0x516450`/`0x5164b0`/`0x518240`/`0x518400` 的开头 8 条指令；`0x518240` 的槽被 `graphwar_replay_player` `0x3d61a0` 读走 |
| 4 | **对局内动作** | 3 | `0x516510`（`fire` 的落地：调 `turn_manager` 一族 `0x46b740`/`0x46c430`/`0x46b460`/`0x463340`，再 push 一条 `Event`）；`0x516ad0`（收到一段字节后投给客户端）；`0x516fd0` **语义未定，但归属已定 = 本模块**（`src/graphwar_game_client.rs`，6 个 Location 锚，`docs/151` §2.2 附带）| `0x514acd` 调 `0x516510`；`0x3a559c`（root_node `0x3a54e0`：`memcpy` 进 `+0xf8` 缓冲后转给 `0x516ad0`） |
| 5 | **setup 回调** | 3 | `0x516970` = `add_bot_N` 的取值器（六个立即数）；`0x5172d0`/`0x517490` 的槽只被 `graphwar_setup_control` 读 | 六个 `mov $N,%esi`（§17.6）；槽 `0x9bc128`/`0x9bc160` 的使用者 `0x4c44b0`/`0x4d1e00`/`0x4d88b0` |
| 6 | **回放 / 远端** | 3 | `0x3d6080`/`0x3d7380`/`0x3d7630` 体内**同时**引用本模块与 `graphwar_replay_player.rs` 的 Location；`0x3d7380` 还直接 `call` `graphwar_remote_event_handler.rs` 两次 | 归属表 `contested`；`0x3d7380` 的 call 目标 |
| 7 | **命令入口** | 1 | `0x514a10`：根节点命令总线的**兜底分支**，只认 `start_game`/`fire` | §17.4 |
| 8 | **事件装配** | 1 | `0x517690`：3 个事件名订阅 + 14 个 manager 逐个装配 | §17.7 |

分类的可执行版本在 `src/graphwar_game_client.rs` 的 `ORIG_FUNCS`（36 条，带
`vaddr`/`size`/`anchors`/`also_in`），测试断言 8 类之和 = 36、
类名与条数与上表一致。

### 17.4 `0x514a10`：命令入口（**观测**，逐条立即数）

签名 `fn(&self, name: &str)`（`rdx` = 长度）。先比长度、再比字节：

```text
514a17  cmp $0xa,%rdx ; je 514a33          ; len == 10
514a33  movabs $0x61675f7472617473,%rax    ; "start_ga"
514a3d  xor (%rsi),%rax
514a40  movzwl 0x8(%rsi),%ecx
514a44  xor $0x656d,%rcx                   ; "me"   ⇒ "start_game"
514a4e  jne 514a2b                         ; 不中 → 直接返回
514a1d  cmp $0x4,%rdx ; jne 514a2b         ; len == 4
514a23  cmpl $0x65726966,(%rsi)            ; "fire"
514a29  je 514a96
```

两个命中分支：

| 键 | 指令 | 做什么 |
|---|---|---|
| `start_game` | `514a50` 取 `[self+0x1e0]`；空则返回。`514a73` 组 `0x800000000000001f`；`514a84` `call *0x9bac80`（`0x3d3530`） | 把 `Event` #31 `game_start_request_event` push 进该对象的 `Vec`（`r14+0x18`） |
| `fire` | `514a96` `lea 0xe0(%rdi),%rsi`；`514aa6` `call *0x9ba8f0`（`0x91e260` = `Clone::clone`）；`514acd` `call *0x9bc450`（**`0x516510`**），`rdi=&[self+0x1e0]` | 克隆 `[self+0xe0]` 后交给 `0x516510`（语义未解，§17.9） |

**谁调它**（槽 `0x9ba890` 的 5 个引用点，全部是 `call *0x9ba890`）：
`0x3b2266`（`graphwar_root_node` 命令总线 `0x3b0830` 的**兜底分支**，见下）、
`0x3a55f7`、`0x3a5637`、`0x3b6e1e`、`0x3b703e`（四个 `(self,name,len)` 蹦床）。

兜底分支的形状（**观测**，这一段把「命令总线 → 本模块」的接缝钉死了）：

```text
3b2242  mov 0x1668(%rbx),%rbx        ; root 的控制器单元（docs/16 §15.3 的 +0x1668）
3b2249  cmpq $0x0,0x10(%rbx) ; jne … ; 借用标记
3b2254  movq $-1,0x10(%rbx)
3b225c  lea 0x18(%rbx),%rdi          ; 载荷
3b2260  mov %r12,%rsi                ; &name
3b2263  mov %r15,%rdx                ; len
3b2266  call *0x9ba890               ; = 0x514a10
```

⇒ **`docs/16` §15.3 里记成「play 侧控制器」的 `[+0x1668]`，它被调的方法落在
`graphwar_game_client.rs`**（`0x514a10`、`0x515590`、`0x5159a0`、`0x516970`、
`0x516ad0` 都是从这个偏移取出来的）。字段↔模块的对应关系据此收窄一条，**但**
`[+0x1668]` 的确切 Rust 类型仍未证（缺：`upcast_ref` 的目标类名，与 §15.9 同一个缺口）。

### 17.5 8 种请求事件 × 2 份实例化 = 16 个函数（**观测**）

16 个 0xb0–0x140 字节的函数，每个只做一件事。以 `0x4d8a00` 为例（其余同形；
唯一例外是 `0x4d96e0`，它多一次对 `graphwar_setup_control.rs` 的 `0x4d1f10` 调用，语义未定）：

```text
4d8a07  mov (%rdi),%r14          ; self.0 = 绑定单元
4d8a0a  mov 0x8(%rdi),%eax       ; self.1 = i32（请求参数）
4d8a28  mov 0x1f8(%r14),%rbx     ; 内层单元
4d8a4b  movabs $0x8000000000000011,%rax     ; ⇒ 判别值 0x11 = 17
4d8a55  mov %rax,0x10(%rsp)      ; 记录 +0x00 = tag word
4d8a47  mov %eax,0x18(%rsp)      ; 记录 +0x08 = i32
4d8a5f  call *0x9bac80           ; = 0x3d3530 = Vec::push（记录 0x48 字节）
```

| 判别值 | 事件（`docs/07` §7.2） | 实例化 A | 实例化 B | 共用槽 | 锚行 |
|---|---|---|---|---|---|
| 11 | `change_team_request_event` | `0x4c5b50` | `0x4d8d30` | `0x95eff0`/`0x95eff8` | 190 |
| 12 | `remove_player_request_event` | `0x4c5d60` | `0x4d9370` | `0x95f030`/`0x95f038` | 194 |
| 16 | `add_soldier_request_event` | `0x4c5c00` | `0x4d9270` | `0x95f070`/`0x95f078` | 198 |
| 17 | `remove_soldier_request_event` | `0x4c5cb0` | `0x4d8a00` | `0x95f0b0`/`0x95f0b8` | 202 |
| 18 | `axis_mode_change_request_event` | `0x4d1e60` | `0x4d95f0` | `0x95f2b0`/`0x95f2b8` | 206 |
| 19 | `function_mode_change_request_event` | `0x4d2550` | `0x4d96e0` | `0x95f2d8`/`0x95f2e0` | 210 |
| 20 | `turn_mode_change_request_event` | `0x4d1d50` | `0x4d8ff0` | `0x95f300`/`0x95f308` | 214 |
| 21 | `time_mode_change_request_event` | `0x4d1ca0` | `0x4d9500` | `0x95f328`/`0x95f330` | 218 |

四条独立证据：

1. **判别值**是 `movabs $0x80000000000000NN` 的指令立即数，与 `docs/07` §7.2 用
   `0x3d38f0` 的相等测试解出的名字表**逐条相等**（`src/events.rs` 的
   `Event::*ChangeRequest`），两条来源完全独立。
2. **两份实例化共用同一个 panic 锚行**（同一行、列 26）⇒ 同一源函数的两个实例化，
   **不是**两个源函数；8 行 190→218 步长 4 = 源文件里的书写顺序。
3. **槽位相邻**（8 字节步长）⇒ 是「同一张表里的两个方法」，不是两个独立函数指针。
4. **两份的差别只在收尾**：B 多一次 `decq (%rcx)`（`0x4d8a78`）与一次 `0x525f60`/`0x526d80`
   调用 ⇒ **推断**：接收者所有权不同（`&Self` vs `Self`）。缺的证据：两份各自
   调用点的 Rust 签名。

**谁在用它们**（观测）：A 组的 8 张表被 `0x4c4c20`（`graphwar_setup_control.rs`）引用，
B 组的 8 张被 `0x4cf670`（同模块）引用；`0x4c4c20` 体内逐条 `lea` 了
`GraphwarRootNode`(`0x19e490`)、`ChangeTeamButton`(`0x19e030`)、
`RemovePlayerButton`(`0x1c2de5`)、`AddSoldierButton`(`0x19e740`)、
`RemoveSoldierButton`(`0x1c2df7`) ⇒ **推断**：这 8 条请求就是 GameSetup/LanLobby
那排按钮按下时发出去的（缺的证据：闭包经 `call_deferred`/`connect`，不在 `call rel32` 里）。

⚠️ 8 个判别值**不含** 13/14/15（`player_info`/`entity_removed`/`soldier_info`）——
那三个是**广播（info）**不是**请求（request）**，与「这些都叫 `*_request_event`」一致。

### 17.6 `add_bot_0..add_bot_5`：命令总线转给本模块的 6 条键（**观测**）

`graphwar_root_node.rs` 的命令总线 `0x3b0830` 里有 6 条键共用一个模板：
`movabs $0x5f746f625f646461`（`"add_bot_"`）+ 第 9 字节与 `'0'..'5'` 异或比较
（`0x3b1935`-`0x3b19d2`），命中后跳到各自的块，块内形状完全相同：

```text
mov 0x1668(%rbx),%r15          ; 同一个控制器单元
cmpq $0x0,0x1f8(%r15) ; je …   ; 内层为空则跳过
lea 0x1f8(%r15),%rdi
mov $N,%esi                    ; ← 六个不同的立即数
call *0x9baaa0                 ; = 0x516970
```

| 键 | N | `mov $N,%esi` | `call` | 键判据 |
|---|---|---|---|---|
| `add_bot_0` | **3** | `0x3b31b6` | `0x3b31bb` | `0x3b194c`(`xor $0x30`) → `0x3b1953` |
| `add_bot_1` | **10** | `0x3b3674` | `0x3b3679` | `0x3b1966`(`xor $0x31`) → `0x3b196d` |
| `add_bot_2` | **25** | `0x3b3975` | `0x3b397a` | `0x3b1980`(`xor $0x32`) → `0x3b1987` |
| `add_bot_3` | **100** | `0x3b3eb4` | `0x3b3eb9` | `0x3b199a`(`xor $0x33`) → `0x3b19a1` |
| `add_bot_4` | **250** | `0x3b40d4` | `0x3b40d9` | `0x3b19b4`(`xor $0x34`) → `0x3b19bb` |
| `add_bot_5` | **1000** | `0x3b41ec` | `0x3b41f1` | `0x3b19cb`(`xor $0x35`) → `0x3b19d2` |

六个值单调递增 ⇒ **推断**：数字是**该档 bot 的强度/预算**，不是 bot 序号。
缺的证据：`0x516970` 拿这个数做了什么（它转给 `0x46b180`，后者是一次按名查表，
键字面量在 `.rodata` `0x19e1d0`，未解）。

⚠️ 顺带解出的**命令总线新键**（补 `docs/16` §15.6 的 12 条，判据同样是
`movabs`+`xor` 立即数）：`exit_play`(`0x3b1914`→`0x3b2cc9`)、
`add_bot_0..5`（上表）、`save_clip`(`0x3b19d8`→`0x3b19f9`)。
**仍未穷举**（§17.9）。

### 17.7 `0x517690`：事件装配（**观测**）：3 个名字 + 14 个 manager

0xbb0 字节。它对 3 个事件名各做一次「分配闭包环境 → 订阅」：

```text
5179ec  call *0x9ba628            ; __rust_alloc（环境）
5179fc  call *0x9ba630            ; alloc(8,8)
5179b8  mov %rbp,(%rax)           ; 捕获态
5179bb  lea 0x1c64c2,%rsi         ; "connected_to_server_event"
5179cc  mov $0x19,%edx            ; len = 25
5179c2  lea 0x962ba8,%r8          ; 回调
5179d4  call *0x9bac88            ; = 0x3d38f0
```

| 名字 | `.rodata` | len | `lea` / `mov $len` | 判别值 |
|---|---|---|---|---|
| `connected_to_server_event` | `0x1c64c2` | 25 | `0x5179bb` / `0x5179cc` | 7 |
| `player_info_event` | `0x1c6558` | 17 | `0x517a0e` / `0x517a1f` | 13 |
| `entity_removed_event` | `0x1c646d` | 20 | `0x517a5e` / `0x517a6f` | 14 |

⇒ 三个名字经 `0x3d38f0` 变成判别值 7/13/14，与 `docs/07` §7.2 第 7/13/14 行一致。
**`0x3d38f0` 的第一类调用者是「订阅」**：它先按 `len-10` 查 25 项跳表
（`0x3d3909`-`0x3d3928`，跳表 `0x1be9c8`），再在 `HashMap<u64, _>`（entry 步长 `0x38`，
公共尾 `0x3d4c5c`）里插入 —— 也就是 `docs/07` §7.3.2 逐位钉过的那张表。
〔观测；「第一类调用者」= 本模块这 3 处 + 别处，别处未统计〕

同一函数里还**逐个装配了 14 个 manager**（每处一次直接 `call`）：
`graphwar_client_connection_handler`(`0x4599f0`)、`player`(`0x455410`)、
`soldier`(`0x455760`)、`plane_pos`(`0x3c6d30`)、`target`(`0x465910`)、
`team`(`0x3c1fc0`)，以及 life / obstacle / turn / graphics / entity_effect /
computer_player / color / rank 各 1 处（模块名来自归属表，具体 `call` 地址未逐条定位）。
⇒ **推断**：这是客户端的「一次性装配」入口（订阅 3 个事件 + 把 14 个 manager 挂上）。
缺的证据：`0x469xxx`–`0x46bxxx` 那族胶水（取句柄，每个 8-16 条指令）没有锚，不敢起名。

### 17.8 委派入口表：外部怎么进本模块（**观测**）

`call rel32` 与 `.rela.dyn` 指针槽的引用点合起来给出下面这张表（完整版在
`src/graphwar_game_client.rs` 的 `ENTRY_POINTS`，测试断言每条入口都在 36 条清单里、
且调用者不在被调函数体内）：

| 目标 | 调用者 | 是什么 |
|---|---|---|
| `0x514a10` | `0x3b2266` + 4 个蹦床 | 命令总线兜底分支（§17.4） |
| `0x515590` | `0x3b2b84`、`0x3b363a` | 总线里 `JoinLevelPopUp` 那条路（先 `0x5148a0` 收串再调本函数） |
| `0x5159a0` | `0x3b3333`、`0x4f923c` | 总线 + `graphwar_lan_vbox` |
| `0x516970` | 六个（§17.6） | `add_bot_0..5` |
| `0x516ad0` | `0x3a559c`（root `0x3a54e0`）、`0x3b6f9b` | 字节到达：`memcpy` 进 `+0xf8` 缓冲后转发 |
| `0x516450` | `0x3b1c93` | 总线 `save_clip` 附近 |
| `0x5164b0` | `0x3a707a`（在 `0x3a5760` 内）、`0x3b3f6c` | 帧状态机 |
| `0x516370` | 槽被 `0x3a5760`/`0x3b7ac0`/`0x5147f0`/`0x525f60` | 帧状态机 + 本模块 |
| `0x517690` | 槽被 `graphwar_backend` `0x514f20` + 本模块 `0x515590`/`0x5159a0` | 装配（§17.7） |
| `0x518240` | 槽被 `graphwar_replay_player` `0x3d61a0` + 本模块 `0x518400` | 状态查询 |
| `0x518320`/`0x518400`/`0x516fd0` | 槽（见 `ENTRY_POINTS`） | 状态查询 / `0x516fd0` 语义未定（归属 = 本模块，6 个 Location 锚）|

**否定结论（省一类死路）**：本模块 36 个函数里**没有**任何屏名（15 个）、
任何命令总线的 `input_text` 键（12 条）、任何 `set_visible`/`get_node` 字面量。
⇒ 切屏与命令总线**不在这里**，在 `graphwar_root_node.rs`（§15）。
本模块只有 `.rodata` 字面量 3 条（§17.7 的三个事件名）+ 若干 panic 串。

### 17.9 没解出来的（诚实清单）

| 缺口 | 缺什么证据 |
|---|---|
| `0x516510`（`fire` 的落地）语义 | 体内调 `turn_manager` 一族（`0x46b740`/`0x46c430`/`0x46b460`/`0x463340`）并 push 一条 `Event`，但判别值走 `lea`+`add` 编码，未定位；`0x516510:0x51682b` 起还有一串 `.data.rel.ro` 引用（疑似格式化串），未解 |
| `0x516ad0`（字节到达）怎么解析 | 只观测到调用者把 `memcpy` 后的 `(ptr,len)` 交给它；报文解析可能在这里，但没读函数体 |
| `0x516fd0` / `0x5172d0` / `0x517490` / `0x516450` / `0x5164b0` / `0x518240` / `0x518400` | 内层 `0x46aa50`/`0x46aea0`/`0x46b180` 未解（它们都做一次按名查表，键字面量 `0x19cdc0`/`0x19d8c0`/`0x19e1d0` 在 `.rodata` 里是**非 ASCII**数据，不是名字；未定它们是散列表还是别的常量表） |
| `0x515590`/`0x5159a0`/`0x5175b0` | 只知道「构造 `game_name_request_event`」与「调 `graphwar_backend.rs`」；`JoinLevelPopUp` 那条路的 `&str` 语义（`[root+0x16f8]`/`[root+0x16fa]`）未定 |
| `0x3d6080`/`0x3d7380`/`0x3d7630` | contested（同时引用本模块与 `graphwar_replay_player.rs`）⇒ 归属本身有歧义 |
| 命令总线的**全部**键 | 已 12（§15.6）+ 8（§17.6：`exit_play`/`add_bot_0..5`/`save_clip`）；`0x3b0830` 是 0x4690 字节、131 KB 反编译，**未穷举** |
| 屏表的插入点 / `[+0x1668]` 的 Rust 类型 | 与 §15.9、§15.10.6 同一缺口 |
| 本模块与 `graphwar_server_connection_handler.rs` / `graphwar_game_server.rs` 的边界 | `0x51xxxx` 一带还有一批无锚函数（`0x5147b0`/`0x5147f0`/`0x5148a0` 等）与 `0x51a570`（`docs/07` §7.3.4 提到）**归属未定**；本轮没有把它们算进 36。**2026-11 复核（`docs/151` §2.2 附带）**：`locs` 逐个投票后 —— `0x51a570`（`0x97` B）= **`graphwar_local_player_manager.rs`**（2 个锚）；`0x516fd0`/`0x518320`/`0x518400`/`0x516450`/`0x5164b0`/`0x518240`/`0x5172d0`/`0x517490` = **`graphwar_game_client.rs`**（各 2–6 个锚）；`0x5147b0`（`0x40` B）/`0x5147f0`（`0x6e` B）/`0x5148a0`（`0x161` B）**零锚**，仍定不了（`docs/151` §3.4）|

### 17.10 进度口径（按 `AGENTS.md` §0 第 2 条）

| 口径 | 数 | 说明 |
|---|---|---|
| 原版可归属函数（本模块） | **36** | `coverage_matrix.py`，可复现 |
| 有**完整真值表**（能照着写） | **17** | 16 个请求事件构造器 + `0x514a10` 的两条键 |
| 只到「它跟谁打交道」 | **9** | `0x516970`（六个立即数）、`0x517690`（3 名字 + 14 manager）、`0x515590`/`0x5175b0`（#22）、`0x516370`/`0x518320`（调 conn_handler）、`0x516ad0`、`0x516510` |
| 只到「调用图一级」 | **10** | §17.9 第 3 行那 7 个 + 3 个 contested |
| `cargo test` 条数 | 见 §17.11 | **回归保护，不是复原证据** |

⚠️ `docs/coverage-matrix.tsv` 本模块那一行**仍然是 `未做`**：那是
`tools/coverage_matrix.py` 的**生成物**（`status` 由脚本里的手写表 `M` 决定），
本轮没有改它的三个允许文件之外的东西。下一轮把它改成 `观测`（并同步
`ours_file`/`ours_funcs`）需要重跑那个脚本。

### 17.11 落码清单

| 原版 | 落在 | 覆盖程度 | 档位 |
|---|---|---|---|
| 36 个函数的清单 | `ORIG_FUNCS` / `orig_fn` / `class_count` / `Class` / `Role` | 36 条地址+大小+锚+contested | 观测 |
| `0x514a10` | `on_command` / `CommandAction` / `CMD_*` / `*_WINDOW*` / `FIRE_IMM32` | **两条键全覆盖**（含长度关） | 观测 |
| `0x4c5b50..0x4d96e0` | `RequestKind` / `RequestEvent` / `request_event` / `REQUEST_CONSTRUCTORS` / `EVENT_RECORD_SIZE` / `EVENT_TAG_BASE` | **8 种 × 2 实例化的判别值与记录形状** | 观测（载荷语义 = i32，观测；参数含义未知） |
| `add_bot_0..5` | `ADD_BOT_KEYS` / `add_bot_value` / `ADD_BOT_PREFIX` | 六条键 → 六个立即数 | 观测（数字含义 = 推断） |
| `0x517690` | `SUBSCRIBED_EVENTS` / `WIRED_MANAGERS` / `FN_EVENT_SUBSCRIBE` | 3 名字 + 14 manager | 观测（装配结构 = 推断） |
| 外部入口 | `ENTRY_POINTS` / `entry_point` | 19 条入口记录 | 观测 |
| 跨模块调用点 | `DIRECT_CALLS_TO_OTHER_MODULES` / `INDIRECT_CALLS_TO_OTHER_MODULES` | 4 条 `call rel32` + 4 条 GOT 槽调用（含 3 处订阅、1 处 `game_state_manager`） | 观测 |

`tests/graphwar_game_client.rs` 的断言都是**回归保护**；每条右侧的真值要么是
指令立即数、要么是 `.rodata` 字节，测试里逐条注明了地址。

---

## 18. `axis_drawer.rs` / `effect_drawer.rs` 规格（本轮新增：坐标轴网格 + 特效状态机）

> **本篇解决什么问题**：这两个模块是**纯绘制**的，判据只有两种——"调了哪些引擎方法"
> 和"传了什么常量"。本节把两者都钉到指令地址；凡是钉不住的，§18.5 明写未知。
>
> **方法名来源**：`fixtures/name_to_addr.tsv`（`panic::Location` 跨架构 join）给出
> `EffectDrawer::{draw, ready, process}` 与 `EffectData::new` / `step_effect`；
> 本轮在此之上**直接用 arm64 2.0.60 的 `.symtab`**（`readelf -sW`）补齐了 x86 侧
> 没有 Location 锚的那几个函数（`step_effect`、两个 `ready` 闭包、`EffectData::new`）。
> 两侧都是 **2.0.60**（arm64 sha256 `8a612e95…`，x86 sha256 `9a538115…`）。

### 18.1 函数清单（x86 虚地址；Ghidra = +`0x100000`）

| 模块 | arm64 符号（`.symtab`，2.0.60） | arm64 vaddr / size | x86 vaddr / size | 档位 |
|---|---|---|---|---|
| `axis_drawer` | `<AxisDrawer as IControl>::draw` | `0x43b400` / 1492 | **`0x4e05b0`** / `0x670` | 观测 |
| `effect_drawer` | `<EffectDrawer as IControl>::draw` | `0x43c578` / 3356 | **`0x4e1520`** / `0x1090` | 观测 |
| `effect_drawer` | `<EffectDrawer as IControl>::ready` | `0x43d294` / 1204 | **`0x4e25b0`** / `0x480` | 观测 |
| `effect_drawer` | `ready::{{closure}}`（`tick_event`） | `0x43d930` / 708 | **`0x4e2a30`** / `0x320` | 观测 |
| `effect_drawer` | `ready::{{closure}}`（`effect_info_event`） | `0x43d748` / 488 | **`0x4e2d50`** / `0x230` | 观测 |
| `effect_drawer` | `<EffectDrawer as IControl>::process` | `0x43dbf4` / 3004 | **`0x4e2f80`** / `0xb50` | 观测 |
| `effect_drawer` | `EffectDrawer::step_effect` | `0x444d2c` / 992 | **`0x4eb450`** / `0x3cc` | 观测 |
| `effect_drawer` | `EffectData::new` | `0x44486c` / 1216 | **未知** | 仅 arm64 名字 |

两处**容易读错的地方**：

1. `fixtures/name_to_addr.tsv` 里 `effect_drawer` 有 **3 行 `ready`**（`0x4e25b0`/
   `0x4e2a30`/`0x4e2d50`）——那不是三个 `ready` 方法，而是 `ready` 本体 +
   它注册的两个闭包（桥表把 `{{closure}}` 后缀剥掉了）。
2. `step_effect`（`0x4eb450`）的跳表分支在 Ghidra 里被切成了 8 个"函数"
   （`0x4eb52e`/`0x4eb559`/`0x4eb57d`/`0x4eb5ef`/`0x4eb65b`/…）。
   **它们都在 `step_effect` 体内**：从 `0x4eb450` 到共同出口 `0x4eb81c`。
   判据：跳表 `0x1c3bec` 的项直接落在这些地址上。

### 18.2 `AxisDrawer::draw`（`0x4e05b0`）画什么

一次 `draw` 只有三种原语，**没有 `draw_string`、没有贴图、没有圆**：

| # | 原语 | 实参 | 指令 |
|---|---|---|---|
| 1 | `draw_set_transform` | `pos = size × 0.5`、`rotation = 0`、`scale = (W/50, −W/50)` | `0x4e0630..0x4e0667`（槽 `0x9bc018`） |
| 2 | `draw_rect` | `rect = (-25,-15,50,30)`、`color = (1,1,1,1)`、`filled = true`、`width = -1`、`aa = false` | `0x4e069f..0x4e06ca`（槽 `0x9bc028`） |
| 3 | `draw_line` × N | 见下 | `0x4e0775` `0x4e0815` `0x4e089f` `0x4e0944` `0x4e09c6` `0x4e0a74` `0x4e0b02` `0x4e0b62` |

`scale` 的两个分量**都除以 50**（`xmm3 = size.x/50`，`xmm4 = −xmm3`）⇒ 一个棋盘单位在
x、y 上等长（正方形格子），y 方向翻过来。这一点值得单独记：原版**不是**按控件高宽
分别拉伸，而是各向同性，所以 50:30 的棋盘只有在 5:3 的控件里才铺满。

`draw_line` 由 `self+0x18` 的 `axis_mode: u8` 控制（三条比较：`cmp al,3` / `cmp al,2` /
`test al,al`）：

| `axis_mode` | 细网格（1 单位，`0.9` 灰，宽 `0.05`） | 粗网格（5 单位，`0.7` 灰，宽 `0.05`） | 主轴（`0.2` 深灰，宽 `0.075`） | 合计 `draw_line` |
|---|---|---|---|---|
| `0` NoAxis | — | — | — | **0** |
| `1` OnlyMain | — | — | x=0、y=0 | **2** |
| `2` EveryFive | — | 左边界 + 竖直 `-20..=25`（10）+ 下边界 + 水平 `-10..=15`（6）= **18** | 2 | **20** |
| `3` EveryUnit | 竖直 `-25..=25`（51）+ 水平 `-15..=15`（31）= **82** | 18 | 2 | **102** |

细节（都会被断言）：

* 细网格**包含** x/y 为 5 的倍数的位置，也**包含** 0——那几条会被后画的粗线、
  轴线覆盖，但原版**确实发了**（没有 `if x % 5 != 0` 这种跳过）。
* 粗网格把**左边界 `x=-25`** 与**下边界 `y=-15`** 单独画一次，循环体从 `-20`/`-10`
  起（`mov $0xffffffe8` / `mov $0xfffffff2` 之后 `lea ebx,[rax+4]`）。上/右边界没有。
* 主轴**先竖后横**（`0x4e0b02` → `0x4e0b62`）。
* 所有 `draw_line` 的 `antialiased` 都是 `false`（每个调用点前 `xor %edx,%edx`）。

### 18.3 `EffectDrawer`：图元、相位、时长

**图元**（`draw` = `0x4e1520`）：`draw_set_transform` ×1 + 每个元素按相位选一种：

| 相位 | 图元 | 半径 | 颜色（r,g,b） | alpha |
|---|---|---|---|---|
| 0 | `draw_circle` | `0.05` | `(0.6, 0.6, 0.1)` | `max(0,(1−t)×0.9)` |
| 1 | `draw_polygon` | — | `(0.6, 0.6, 0.1)` | `max(0,(1−t/0.9)×0.9)` |
| 2 | `draw_circle` | `0.06` | `(0.7, 0.7, 0.0)` | 同上式 |
| 3 | `draw_circle` | `0.06` | `(0.7, 0.7, 0.0)` | 同上式 |
| 4 | `draw_line` | — | `(0.9, 0.9, 0.1)` | 同上式 |
| 5 | `draw_line` | — | `(0.9, 0.9, 0.1)` | 同上式 |
| 6 | `draw_circle` | `0.06` | `(0.1, 0.1, 0.1)` | 固定 `0.7` |

相位表来自 `draw` 里的相对跳表 `0x1c3b8c`（`movsxd rax,[r13+rax*4]; add rax,r13;
jmp rax` @ `0x4e1932`）：**表里只有 7 项落在本函数内**，第 8 项起跳到 `0x4ea6af`
等别处的地址 ⇒ 相位枚举 7 个值。所有圆/线的 `filled = true`、`width = -1`、
`antialiased = false`。

**元素布局**（`EffectData`，stride `0x30` = 48 字节，`lea r15,[rax+rax*2]` 其中
`rax = len<<4` @ `0x4eb471`）：

| 偏移 | 内容 | 依据 |
|---|---|---|
| `+0x00` | 位置 `(f64, f64)` | `movupd xmm0,[rbx]` @ `0x4eb566`；`movsd` @ `0x4e19cc` |
| `+0x10` | 速度 `(f64, f64)` | `movupd xmm1,[rbx+0x10]` @ `0x4eb56a`；撞障碍后整体清零 @ `0x4eb592` |
| `+0x20` | `start_tick: u32` | `mov ecx,[r14+rbp+0x20]` @ `0x4e1950` |
| `+0x24` | `duration_ticks: u32` | `mov eax,[r14+rbp+0x24]` @ `0x4e1955` |
| `+0x28` | 相位 `u8`（0..=6） | `movzx eax,byte [r14+rax+0x28]` @ `0x4e1932` |
| `+0x2c` | **未知 4 字节** | 未见访问 |

**时长与时间单位**（本节最硬的一条新结论）：

| 量 | 值 | 槽 / 指令 |
|---|---|---|
| tick/秒 | `60.0` | 槽 `0x19f918`，`mulsd` @ `0x4eb481` |
| 物理子步 | **`1/600` s**（`0x3f5b4e81b4e81b4f`） | 槽 `0x19ed20`/`0x19ed28`/`0x19f9f0`；`mulpd` @ `0x4eb56f`；**全二进制只有 `step_effect` 用这个常量** |
| 子步循环条件 | `delta > acc`（f64，不是 tick 计数） | `ucomisd`/`ja` @ `0x4eb628` |
| 相位推进 | `+0x258` = **600 tick** | `add eax,0x258` @ `0x4eb4c2`/`0x4eb530`；arm64 `add wN,wN,#0x258` |
| `tick_event` 的 delta | `1/60` s | 槽 `0x19f2a8`，`0x4e2a30` 引用 |
| 元素存活 | `now <= start + duration` | `cmp edx,r15d; jb 跳过` @ `0x4e195c` |

⇒ **"爆炸动画播多久"由元素自己的 `duration_ticks` 决定**（tick 单位，60 tick = 1 秒）；
`step_effect` 推进的 600 tick 是**相位**节拍（10 秒），不是整段特效的寿命。
`duration_ticks` 的**具体取值**在 `EffectData::new` 里，x86 侧没定位到（§18.5）。

**接线**：`ready`（`0x4e25b0`）用 `GraphwarBackend::subscribe_to_event` 订阅两个事件，
名字字面量就在指令里：

```text
0x4e2802  lea rsi,[0x1c4225]  mov edx,0x11  ; "effect_info_event"
0x4e2893  lea rsi,[0x1c4236]  mov edx,0xa   ; "tick_event"
```

两张闭包 vtable 的实参函数指针（`.rela.dyn` 的 `R_X86_64_RELATIVE` addend）：
`0x95fb70 → 0x4e2d50`（`effect_info_event`）、`0x95fb98 → 0x4e2a30`（`tick_event`，
它调 `step_effect`）。

`process`（`0x4e2f80`）里没有绘制，只有：`Instant::now` → 取当前 tick
（`get_module` 后 `[module+0x58]`）→ `Vec::retain` 丢掉过期元素 →
`CanvasItem::queue_redraw` → `Instant::elapsed`，若 `> 0x3b9aca00`（**1e9 ns = 1 秒**）
则 `_print` 打一行 `src/effect_drawer.rs:507`（槽 `0x1c4240`，24 字节）。

### 18.4 与 `client/` 仿制实现的出入（**不改 client，只登记**）

`client/src/game/effect_layer.gd` 是照协议枚举猜的；拿到代码后**有六处对不上**：

| 项 | 原版（观测） | `client` 当前 | 结论 |
|---|---|---|---|
| 半径 | `0.05` / `0.06` **棋盘单位**（≈ W/50×0.05 px） | `RADIUS_UNITS` = `0.7 … 1.6` | **差 1 个数量级**：原版是"小点"，不是大团 |
| 时长 | `duration_ticks`（tick 计，60 tick = 1 s）+ 相位 600 tick | `LIFETIME` = 0.30–0.90 **秒** | 原版是 tick 制；且相位节拍 = 10 s |
| 颜色 | `(0.6,0.6,0.1)` / `(0.7,0.7,0)` / `(0.9,0.9,0.1)` / `(0.1,0.1,0.1)`，alpha 上限 `0.9` | 自定义暖色四组 | 全部替换 |
| 图元 | `draw_circle` ×4 + `draw_polygon` ×1 + `draw_line` ×2 | `draw_arc` + `draw_circle` + `draw_line` + `draw_rect` | 原版**没有 `draw_arc`、没有 `draw_rect`** |
| 种类数 | `draw` 的相位枚举 **7 个**（协议 4 种 → 内部 7 相位） | 4 种（= 协议枚举） | 协议 4 种是**事件**的分类，不是绘制分支 |
| 运动 | 元素带**速度**，`step_effect` 按 `1/600 s` 积分，撞障碍速度清零 | 无速度、只计时 | 原版特效是**会动**的粒子 |
| 驱动 | `tick_event` 闭包推进；`process` 只 `retain` + `queue_redraw` | `_process(delta)` 里累计 `age` | 推进时钟不是帧间隔，是 tick |

坐标轴一侧**基本对得上**（`client/src/play/plane_view.gd::_draw_grid` 的四档含义、
三种灰阶 `0.9/0.7/0.2` 与白底都与二进制一致），只有三处细节差：

| 项 | 原版 | `client` |
|---|---|---|
| 细网格是否跳过 5 的倍数与 0 | **不跳过**（照发，被上层覆盖） | 跳过（`fmod(x,5)` 与 `x≈0` 时不画） |
| 粗网格边界 | 左边界 `x=-25` + 下边界 `y=-15` 各一条 | 无边界线（边界由 `_draw_border` 另画） |
| 线宽 | `0.05` / `0.075` **世界单位**（随缩放变） | `unit` / `unit*2` **画布像素**（不随缩放变） |

`0.05` 世界单位在 `W=1600` 时是 `1600/50×0.05 = 1.6 px`，`0.075` 是 `2.4 px` —— 与
`client` 的 1/2 px 同量级但不同源；控件尺寸不是 1600 时两者会越差越远。

### 18.5 诚实清单（没解出来的）

1. **`EffectData::new` 的 x86 地址与字段初值**。arm64 里它只被 `effect_info_event`
   闭包调用且已内联，x86 没有 `call rel32` 指向它；arm64 版用了 `rand::random` 和 π
   ⇒ 它是**带随机初值**的构造函数。`duration_ticks` 的真实取值只能从这里拿。
2. **相位 1（`draw_polygon`）的顶点公式**。只读到两组点各乘 `0.2`（槽 `0x19c460`）后
   进 `PackedVector2Array`；逐点的三角函数与排布未复原，`src/effect_drawer.rs` 里
   `Primitive::Polygon` 的 `points` **故意留空**。
3. **相位 4/5（`draw_line`）的端点**。颜色/线宽已定，端点几何公式未逐条读；代码里
   `from`/`to` 都填 `center`，只是占位，不代表原版。
4. **`draw` 里 `self+0x1f8` 的位掩码语义**（`and 0x101` / `test 0x10000` /
   `cmp 0x1000000` @ `0x4e175f..0x4e1783`）：它决定 `scale.y` 的正负。推断与 y 轴朝向
   / 函数模式有关，未证。
5. **`EffectData` 的 `+0x2c`** 那 4 字节：未见任何读写。
6. **`AxisMode` 判别值**：`0=NoAxis … 3=EveryUnit` 是从代码行为（0 一条线都不画、
   3 画最细网格）反推的；`client/src/app/modes.gd` 的 `AXIS_LABELS` 独立给出同一顺序，
   但两者都不是"读到了枚举定义"。

### 18.6 落码清单

| 原版 | 落在 | 覆盖程度 | 档位 |
|---|---|---|---|
| `AxisDrawer::draw` `0x4e05b0` | `src/axis_drawer.rs`：`axis_primitives` / `line_count` / `AxisMode` + 16 条常量 | 三种原语、四种模式、每条线的坐标与颜色宽度 | 观测 |
| `EffectDrawer::draw` `0x4e1520` | `src/effect_drawer.rs`：`frame_primitives` / `effect_primitive` / `Phase` / 12 条常量 | 7 个相位的图元、半径、颜色、alpha | 观测（相位 1 的顶点、相位 4/5 的端点 = 未知） |
| `EffectDrawer::step_effect` `0x4eb450` | `step_effects` / `ticks_for_delta` | 1/600 子步积分、撞障碍清零、`0x5b` 相位推进 | 观测（各相位分支的细节 = 推断） |
| `EffectDrawer::ready` `0x4e25b0` | `SUBSCRIBED_EVENTS` / `READY_SUBSCRIPTIONS` | 两个事件名 + 两张 vtable | 观测 |
| `EffectDrawer::process` `0x4e2f80` | `PROCESS_STEPS` / `TIME_PROBE_*` | 五步 + 1 s 探针阈值 | 观测 |
| `EffectData` 布局 | `effect_data_offset` / `EFFECT_STRIDE` | 0x30 字节里已证的 5 个字段 | 观测 |
| `EffectData::new` | **未落码**（只留 `EffectData::new` 作为显式构造） | — | 未知 |

`tests/axis_drawer.rs`（11 条）与 `tests/effect_drawer.rs`（14 条）的断言都是
**回归保护**；右侧真值要么是 `.rodata` 字节、要么是指令立即数，测试里逐条注明了地址。

---

## 19. `graphwar_button.rs` / `graphwar_long_press_animation.rs` 规格（本轮新增：2.0.60 arm64 符号表定名 + 6 个方法体 + `draw` 全解）

> **本节解决什么**：这两个模块是全库**最难定位**的两个 GDExtension 类 ——
> `fixtures/name_to_addr.tsv` 给 `graphwar_button` 的 5 行全是 `歧义`/`未知`，
> 且它的候选地址**经反汇编核对全是错的**（§19.2）。
> 本节把它们的**真名、x86 地址、方法分派表、状态机、`draw` 的每一步算术**全部钉下来。
>
> **地址口径**：一律 **ELF 虚地址**（`objdump` 口径）。Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。
> 落码：`src/graphwar_button.rs` + `src/graphwar_long_press_animation.rs`；
> 验收：`tests/graphwar_button.rs`（18 条）+ `tests/graphwar_long_press_animation.rs`（12 条）。

### 19.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 新杠杆 | **同版本（2.0.60）arm64 `.so` 没 strip**：`.reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so` 有完整 `.symtab` | 观测 |
| `GraphwarButton` 的方法 | `init`/`ready`/`process`（`IButton` 虚函数）+ **`button_down`/`button_up`/`pressed`**（`#[func]`，`ready` 里连到同名信号） | **观测**（arm64 真名 + x86 指令） |
| 它的导出属性 | **2 个**：`input_text`、`long_press_input_text`（每个一对 getter/setter，见 §19.6） | 观测（`.rodata` 名字对） |
| 它的状态 | `+0x60/+0x68` 按下时刻；`+0x50/+0x58` 阈值 `0.5 s`；`+0x88/+0x90` 两个 GString；`+0x40/+0x48` 订阅者 `Vec`；`+0x18` 动画节点判别位 | 观测（偏移）+ 推断（字段名） |
| `GraphwarLongPressAnimation` 的方法 | **2.0.60 只有 `draw`**（2024 版有 5 个，见 §19.8 更正） | **观测**（arm64 `.symtab` 全长搜索） |
| 它的状态 | `+0x18` secs / `+0x20` nanos，哨兵 `1e9` = 「没在动画」 | 观测 |
| 动画参数 | `angle = elapsed × 4π`，`angle > 2π` 结束 ⇒ **0.5 s**；两次绘制（黑 `{0,0,0,1}` + 白 `{1,1,1,1}`） | **观测**（逐指令 + `.rodata` 比特） |
| 两个模块的接缝 | `button_down` 写 LPA 的 `+0x18/+0x20`（启动），`button_up` 写哨兵（停止） | **观测**（两侧指令都核过） |
| 还缺什么 | 3 次取值 / 2 次绘制的 **godot API 名**（GOT 槽）；4 个属性存取器的**函数体**；订阅者由谁 `push` | 未知（§19.10） |

### 19.2 名字从哪来；以及 `name_to_addr.tsv` 的 5 条候选**全错**（冲突登记）

```bash
awk -F'\t' '$1=="graphwar_button"' fixtures/name_to_addr.tsv
awk -F'\t' '$1=="graphwar_long_press_animation"' fixtures/name_to_addr.tsv
```

TSV 对这两个模块只给了 **2 条可用信息**：

* `GraphwarLongPressAnimation::draw` = **`0x4d8100`**，`唯一锚`
  （`src/graphwar_long_press_animation.rs:52:55`，4 个双向唯一锚）—— **本条正确，本节采用**；
* 两个模块的 `__register_constants` 各 4 个候选（button：`0x4e0e60`/`0x4e0fe0`/`0x4e8230`/`0x4e83d0`；
  LPA：`0x4bd650`/`0x4bda50`/`0x4d7bc0`/`0x4d7d40`）—— **这 8 个地址全部正确**，
  它们是 gdext 样板簇（§19.3 表末行），只是 TSV 分不出是哪一个。

其余 **5 行（`button_down`/`button_up`/`ready`/`process`/`init`）的候选地址全部错误**。
否掉的判据（可复跑，见 §19.11）：

| 方法 | TSV 候选 | 反汇编事实 |
|---|---|---|
| `button_down`/`button_up` | `0x3afb00` | 体内引用 `src/graphwar_root_node.rs:892:35`、`:894:28` |
| 同上 | `0x3afe90` | 体内引用 `src/graphwar_root_node.rs:868:35`、`:870:28` |
| `ready` | `0x3a2e10` | `graphwar_root_node` 的 `_ready`（§15.4） |
| `process` | `0x3a5760` | `graphwar_root_node` 的状态机（§15.7） |
| `init` | `0x920850` | gdext 通用初始化样板 |

⇒ **以反汇编为准**。真名改用**同版本 arm64 `.symtab`**（两个构建同源，函数集合与调用结构可比；
arm64 地址只用于定名，绝不当作 x86 地址）：

```bash
readelf -sW .reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so | c++filt | grep graphwar_button
# graphwarrust::graphwar_button::GraphwarButton::button_down   @0x448470 (436 B)
# graphwarrust::graphwar_button::GraphwarButton::pressed       @0x448624 (300 B)
# graphwarrust::graphwar_button::GraphwarButton::button_up     @0x448750 (428 B)
# <GraphwarButton as …::IButton>::init                         @0x43eb08 (280 B)
# <GraphwarButton as …::IButton>::ready                        @0x43ec20 (868 B)
# <GraphwarButton as …::IButton>::process                      @0x43ef84 (336 B)
```

### 19.3 x86 定位表（**观测**；每条都给判据）

这 6 个方法**一个 panic 锚都没有**（`src/graphwar_button.rs` 只有 14/83/111 三处，
全被 gdext 样板占用）⇒ 只能靠**跨架构指纹**：

| 方法 | x86 vaddr | x86 B | arm64 B | 比 | 判据 |
|---|---|---|---|---|---|
| `init` | `0x4e3e00` | 288 | 280 | 1.029 | 逐字段写初值（`[+0x68]=0x3b9aca00`、`[+0x58]=0x1dcd6500`、两个空 GString）；紧邻 `ready`/`process`，顺序与 arm64 符号表一致 |
| `ready` | `0x4e3f20` | 896 | 868 | 1.032 | 唯一同时 `lea` `"pressed"`/`"button_up"`/`"button_down"`/`"AnimationRect"` 的函数，且含 `"Upcast failed. This is a bug; please report it."` |
| `process` | `0x4e42a0` | 352 | 336 | 1.048 | `cmp/mov [r+0x68],0x3b9aca00` + `GString::is_empty(+0x90)`，逐句同形 |
| `button_down` | `0x4ef390` | 464 | 436 | 1.064 | 全库**只有它和 `button_up`** 引用 `type_name::<GraphwarLongPressAnimation>()`（`.rodata 0x1c1c13`）；arm64 侧同样只有这两个 |
| `pressed` | `0x4ef560` | 320 | 300 | 1.067 | `[self+0x48]` 当长度、`[self+0x40]` 当指针、步长 `0x10` 的循环 + `GString::fmt(self+0x88)` |
| `button_up` | `0x4ef6a0` | 448 | 428 | 1.047 | 开头 `mov dword [r+0x68],0x3b9aca00`；其余与 `button_down` 同形 |

**gdext 样板簇**（不是游戏代码，但地址可钉住）：

| 锚 | x86 函数 | 说明 |
|---|---|---|
| `graphwar_button.rs:14:10` | `0x4e8020`（528 B） | `#[derive(GodotClass)]` 生成 |
| `graphwar_button.rs:83:1` | `0x4e0e60`/`0x4e0fe0`（384 B×2）、`0x4e8230`（416）、`0x4e83d0`（432） | 两个 `#[godot_api]` 块 × (`__register_methods`,`__register_constants`) |
| `graphwar_button.rs:111:1` | `0x4e8580`（592 B） | `impl IButton` 块 |
| `graphwar_long_press_animation.rs:10:10` | `0x4bd430`（544 B） | 同上 |
| `…:17:1` | `0x4bd650`/`0x4bda50`（432×2）、`0x4d7bc0`/`0x4d7d40`（384×2） | 同上 |
| `…:31:1` | `0x4bd800`（592 B） | 同上 |

**两个架构的 Location 取值完全相同**（`graphwar_button.rs`：14:10 / 83:1 / 111:1；
`graphwar_long_press_animation.rs`：10:10 / 17:1 / 31:1 / 52:55 / 52:86 / 53:56 / 53:87），
因为 `core::panic::Location` 是编译期常量、与指令集无关。

### 19.4 方法名 ↔ 分派表（**观测**）

gdext 的 `#[func]` 与属性存取器**不产生 Rust 符号**，而是「按名字分派」：
每个入口是一个小函数，体内 `lea` 出 `(类名, 方法名)` 两对 `&str`
（`(ptr,len)` 全部自校验过）。`GraphwarButton` 实测 16 个入口：

| x86 vaddr | 大小 | 类名 | 方法名 |
|---|---|---|---|
| `0x4e4400` / `0x4e47f0` | 192 / 240 | `GraphwarButton` | `get_input_text` |
| `0x4e44c0` / `0x4e4700` | 192 / 240 | `GraphwarButton` | `set_long_press_input_text` |
| `0x4e4580` / `0x4e49d0` | 192 / 240 | `GraphwarButton` | `set_input_text` |
| `0x4e4640` / `0x4e48e0` | 192 / 240 | `GraphwarButton` | `get_long_press_input_text` |
| `0x4e4d40` | 192 | `GraphwarButton` | `process` |
| `0x4e4e00` | 192 | `GraphwarButton` | `ready` |
| `0x4ef860` / `0x4efaa0` | 192 / 240 | `GraphwarButton` | `button_down` |
| `0x4ef920` / `0x4efc81` | 192 / 239 | `GraphwarButton` | `button_up` |
| `0x4ef9e0` / `0x4efb90` | 192 / 240 | `GraphwarButton` | `pressed` |

成对的 192/240 是同一模板的两个实例化（ptrcall / varcall），差别只在尾部
（多一次 `drop`）⇒ **推断**：接收者所有权不同（`&Self` vs `Self`）。

另外两处：

* `0x4e4ac0`（640 B）= **属性表**（`__register_exports`）：成对给出
  `input_text` ↔ `get_input_text`/`set_input_text`、
  `long_press_input_text` ↔ `get_long_press_input_text`/`set_long_press_input_text`（§19.6）；
* `0x5009f0`（368 B）= 三个信号方法名的**分派器**（长度关 + 前 8 字节立即数比较）。

### 19.5 `GraphwarButton` 状态机（**观测**：偏移 + 立即数；字段名是推断）

`init`（`0x4e3e00`，写初值段 `0x4e3e5a`–`0x4e3eb3`）逐字段写出：

| 偏移 | 初值 | 含义 | 读它的地方 |
|---|---|---|---|
| `+0x18` | `0` | 动画节点判别位（`Some`=1） | `button_down` 的 `0x4ef3aa`、`button_up` 的 `0x4ef6b1` |
| `+0x38/+0x40/+0x48` | `{0, 8}` / `0` | 订阅者 `Vec<Box<dyn FnMut(&str)>>`（cap=0、悬垂 8、len=0） | `pressed`、`process` |
| `+0x50` | `0` | 阈值 `Duration.secs` | —（本模块无读者，§19.10） |
| `+0x58` | **`0x1dcd6500`** | 阈值 `Duration.nanos` = **500 ms** | —（同上） |
| `+0x60` | （`Instant.secs`） | 按下时刻 | — |
| `+0x68` | **`0x3b9aca00`** | → 换成「没按下」哨兵 `nanos = 1e9` | `process` 的 `0x4e42c9` |
| `+0x88` | `GString("")` | `input_text` | `pressed` |
| `+0x90` | `GString("")` | `long_press_input_text` | `process` |

四个动作（逐句对拍 arm64 同名函数；x86 指令地址在括号里）：

```text
button_down (0x4ef390)
  0x4ef39d  Instant::now()            → [self+0x60]=secs,[self+0x68]=nanos     ← 无条件
  0x4ef3aa  cmp dword [self+0x18],1 ; jne 返回                                ← 没接动画节点就结束
  0x4ef3e2  Instant::now()（第二次）  → [lpa+0x18]=secs,[lpa+0x20]=nanos
  0x4ef420  base_mut() → queue_redraw()

button_up   (0x4ef6a0)
  0x4ef6aa  mov dword [self+0x68],0x3b9aca00      ← **在**判别之前，所以总会释放
  0x4ef6e9  mov dword [lpa +0x20],0x3b9aca00
  0x4ef711  base_mut() → queue_redraw()

process     (0x4e42a0)
  0x4e42bb  GString::is_empty(self+0x90) → 空则返回
  0x4e42c9  cmp dword [self+0x68],0x3b9aca00 → 已释放则返回
  0x4e42d7  mov dword [self+0x68],0x3b9aca00      ← **在**派发之前（一次性）
  0x4e42df  循环 self+0x40/+0x48，用 GString(self+0x90) 逐个回调

pressed     (0x4ef560)
  同形循环，但格式化的字段是 self+0x88，且**不改**状态
```

⇒ **点一下发 `input_text`；按住不放发 `long_press_input_text` 且只发一次。**
`process` **没有**读 `0.5 s` 阈值（它只判「还按着」）；阈值的消费者是长按动画
（`elapsed > 0.5`，§19.8）〔推断：两者是同一个 0.5 s〕。

### 19.6 `ready`：3 条信号 + 1 次按名找节点（**观测**）

```text
0x4e3f4e  lea "pressed"      + 0x4e3f5d  len 7   → Callable::from_object_method → connect
0x4e3fc7  lea "button_up"    + 0x4e3fd6  len 9   → 同上
0x4e4040  lea "button_down"  + 0x4e404f  len 11  → 同上
0x4e410d  lea "AnimationRect" (13 B)             → GraphwarNodes::get_node_by_name_rec → cast
```

信号名与「被连的方法名」是**同一个 **`&str`（同一地址）⇒
`connect("pressed", Callable::from_object_method(self, "pressed"))` 这种写法；
三个名字在 `.rodata` 里首尾相接：`0x1c4258`(7) → `0x1c425f`(9) → `0x1c4268`(11) → `0x1c4273`(13)。

最后那次按名字找的是 **`"AnimationRect"` 节点**（`0x1c4273`），随后 cast 成
`GraphwarLongPressAnimation`（arm64 `ready` 的 `check_rtti` 目标类型名 = 71 字节的
`graphwarrust::graphwar_long_press_animation::GraphwarLongPressAnimation`，x86 在 `0x1c1c13`）。

**两个导出属性**（`0x4e4ac0` 的 24 个 `(str,len)` 立即数还原）：

| 属性 | 名字地址 | getter | setter |
|---|---|---|---|
| `input_text` | `0x1c42ce` | `get_input_text`（`0x4e4400`/`0x4e47f0`） | `set_input_text`（`0x4e4580`/`0x4e49d0`） |
| `long_press_input_text` | `0x1c42d8` | `get_long_press_input_text`（`0x4e4640`/`0x4e48e0`） | `set_long_press_input_text`（`0x4e44c0`/`0x4e4700`） |

⚠️ 这两组存取器的**函数体未解**（只经名字分派，静态拿不到函数指针）。

### 19.7 `GraphwarLongPressAnimation::draw`（`0x4d8100`，1232 B，**全解**）

唯一锚（4 个，两两成对）：`52:55`+`52:86`、`53:56`+`53:87`
⇒ 源码第 52、53 行各有**两个** `.expect()`；二进制里对应 4 条 panic 出口
`0x4d8522`/`0x4d852f`/`0x4d855e`/`0x4d856b`。

```text
4d810d  eax = [rdi+0x20]                          ; 启动时刻 nanos
4d8110  cmp eax, 0x3b9aca00 ; je 4d8514（返回）     ; == 1e9 ⇒ 没在动画
4d811e  [rsp+0x78] = [rdi+0x18] ; [rsp+0x80] = eax ; 组 Instant
4d812e  3 次 godot 取值（GOT 槽，名字不可解）⇒ p0,p1 / s0,s1 / n,c
4d82b2  v48 = s0/10 ; v4c = s1/3 ; v44 = 0.5*v4c ; v10 = s1*0.5 - v44
4d8337  half = n/2（i32，`shr 31`/`add`/`sar 1`）；条件 `(float)half > c`
4d82f3  v8 = 正分支 p0+v48+v44+s0+v10 ／ 负分支 p0-v48-v44-v10
4d8347  y  = p1 + s1*0.5
4d83a0  call *[0x9bafe8] on [rsp+0x78]            ; Instant::elapsed()（只吃启动时刻）
4d83fb  (double)nanos / 1e9 + (double)secs         ; = elapsed（f64）
4d8407  ×2 ; ×2 ; ×π                               ; = elapsed × 4π ← 角度
4d8485  draw #1  xmm0=v8 xmm1=y xmm2=v10 xmm3=-0.2 xmm4=angle+0.2 xmm5=2*0.2*v4c+v4c
                rdx=.rodata 0x19cf70（Color{0,0,0,1}）  esi=0x1e ecx=0
4d84ed  draw #2  xmm0=v8 xmm1=y xmm2=v10 xmm3= 0.0 xmm4=angle     xmm5=v4c
                rdx=.rodata 0x19ed00（Color{1,1,1,1}）  esi=0x1e ecx=0
4d8503  if (angle > 2π) [rdi+0x20] = 0x3b9aca00     ; 结束
```

`.rodata` 常量（**原始比特**，改一个就等于宣称二进制变了）：

| 值 | 地址 | 指令 | f32 比特 / f64 比特 |
|---|---|---|---|
| 3.0 | `0x1bb2b8` | `4d82b2 divss`（`s1/3`） | `40400000` |
| 10.0 | `0x1b9530` | `4d82c0 divss`（`s0/10`） | `41200000` |
| 0.5 | `0x1ba128` | `4d82d6 mulss` | `3f000000` |
| 0.2f32 | `0x1ba598` | `4d8423 movss` | `3e4cccd` |
| 1e9 | `0x19eff8` | `4d83fb divsd` | `41cdcd6500000000` |
| π | `0x19fdc8` | `4d840f mulsd` | `400921fb54442d18` |
| 2π | `0x19fab8` | `4d8503 ucomisd` | `401921fb54442d18` |
| `{0,0,0,1}` | `0x19cf70` | 第 1 次绘制的 `rdx` | Color（黑） |
| `{1,1,1,1}` | `0x19ed00` | 第 2 次绘制的 `rdx` | Color（白） |
| `{-0,-0,-0,-0}` | `0x19e880` | `4d8442 movaps` + `xorps` | 取负掩码 |

⇒ **动画时长 = 0.5 s**：`angle = elapsed × 4π > 2π ⇔ elapsed > 0.5`（严格大于；
正好 0.5 s 这一帧还不算结束）。这与 `GraphwarButton::init` 里 `Duration{0, 0x1dcd6500}`
**数值相同**〔推断：同一个常量；缺的证据：把 0.5 s 传给本节点的那条指令〕。

**被调用的 godot API 名未知**（`call *[0x9bc020]` 与 3 次取值全是 GOT 槽，
静态解析不到符号）⇒ 本节只说「两次绘制 + 这些实参」，不猜它们是 `draw_rect` 还是别的。

### 19.8 就地更正三条

1. **`docs/02-module-map.md` §16.3 给本模块的锚地址是旧值**：那行写
   `:52:56`/`:52:86`/`:53:56`/`:53:87` → `0x4d9500`。**实测是 `0x4d8100`**
   （4 个 Location 的引用者只有它一个；`0x4d9500` 是
   `time_mode_change_request_event` 的实例化 B，见 §17.5）。以本节为准。
   同一行的列号也差 1：实测 `52:55`（不是 `52:56`）与 `53:56`/`53:87`。
2. **`docs/02` §18.5 C 表记本模块有 5 个方法**（`draw, process, ready, start, stop`）——
   那是**2024** 版符号表。**2.0.60 的 arm64 `.symtab` 只有 `draw`**
   （`grep -c` 全长搜索，`start`/`stop`/`ready`/`process` 为 0）；
   `trait impl` 也只有 `IControl`（不是 `IButton`）。
3. **`fixtures/name_to_addr.tsv` 对 `graphwar_button` 的 5 行候选全部错误**（§19.2 表），
   本节以反汇编为准并已把冲突登记进 `src/graphwar_button.rs::NAME_BRIDGE_CONFLICTS`。
   同表里两个模块的 `__register_constants` 4 候选地址是**正确的**（= §19.3 的样板簇）。

### 19.9 跨模块接缝：谁驱动长按动画（**观测**）

| 写入方 | 指令 | 写的字段 |
|---|---|---|
| `GraphwarButton::button_down` | `0x4ef3e8` / `0x4ef3ec` | LPA `+0x18` = `now.secs`、`+0x20` = `now.nanos`（**启动**） |
| `GraphwarButton::button_up` | `0x4ef6e9` | LPA `+0x20` = `0x3b9aca00`（**停止**） |
| `GraphwarLongPressAnimation::draw` | `0x4d850d` | 自己 `+0x20` = `0x3b9aca00`（转满一圈后自停） |

`GraphwarButton::ready` 用 `"AnimationRect"` 这个名字把节点找出来并 cast 成本类型
⇒ 场景里长按动画节点的名字就是 `AnimationRect`（[推断] 缺的证据是 `.tscn` 文本）。

### 19.10 本节没解出来的（照实写）

| 缺口 | 缺什么证据 |
|---|---|
| LPA `draw` 里 3 次取值的 API 名与语义（哪一对是位置、哪一对是尺寸） | 全是 `call *[..](%rip)`（GOT 槽），静态解析不到符号；需要 `tools/ptr_slots.py` 补查槽→目标，或运行时打桩 |
| `draw` 的两次绘制是被哪个 API 调用（`esi=0x1e` 是什么） | 同上（`call *[0x9bc020]`） |
| 4 个属性存取器（`get/set_input_text`、`get/set_long_press_input_text`）的函数体 | 只经 `(类名,方法名)` 分派，静态没有函数指针；需要运行时抓 vcall 表 |
| 订阅者 `Vec<Box<dyn FnMut(&str)>>` 由谁 `push` | 本模块 6 个方法里**没有** `push`（已全函数搜索 `+0x40/+0x48` 的写点） |
| `GraphwarButton` 的 `+0x50/+0x58`（`0.5 s` 阈值）的读者 | 本模块 6 个方法里没有读它；需要找到把阈值交给 LPA 的那条指令 |
| `init` 里 `[self+0x00]`/`[+0x10..0x18]`/`[+0x28..0x38]`/`[+0x70..0x88]` 的字段名 | 写了但没读到 ⇒ 未知 |

### 19.11 复现（不依赖本节，任何人可重跑）

```bash
SO=.reference/steam/2026-08-28/libgraphwarrust.so          # sha256 9a538115…
ARM=.reference/x/arm64/lib/arm64-v8a/libgraphwarrust.so

# 1) 真名（x86 被 strip，arm64 没 strip）
readelf -sW $ARM | c++filt | grep -E 'graphwar_(button|long_press)'

# 2) 两个模块的 Location 集合（x86 与 arm64 应完全相同）
../.venv/bin/python tools/coverage_matrix.py --check        # 依赖 Location 管线

# 3) 方法分派表（类名/方法名两对 &str）
objdump -d -M intel --start-address=0x4e4400 --stop-address=0x4e4ee0 $SO

# 4) 状态机的四个动作
objdump -d -M intel --start-address=0x4e3e00 --stop-address=0x4e43f0 $SO
objdump -d -M intel --start-address=0x4ef390 --stop-address=0x4ef860 $SO

# 5) draw 与它的常量
objdump -d -M intel --start-address=0x4d8100 --stop-address=0x4d85d0 $SO
```

### 19.12 落码清单（`src/` 两个文件）

| 原版 | 落在 | 覆盖程度 | 档位 |
|---|---|---|---|
| 6 个方法的名 + 地址 + 大小 | `ORIG_FUNCS` / `orig_fn` | 6 条，含大小比判据 | 观测 |
| arm64 真名 | `ARM64_SYMS` / `arm_sym` | 8 条（含注册数据） | 观测 |
| 3 处 panic 锚 | `SRC_ANCHORS` | 3 条（引用函数逐个列出） | 观测 |
| 5 条 TSV 冲突 | `NAME_BRIDGE_CONFLICTS` | 6 条（含否掉的理由） | 观测 |
| 类名 / 信号 / 节点名 / 属性 | `CLASS*`、`SIGNAL_LINKS`、`NODE_ANIMATION_RECT*`、`PROPERTIES` | 全部 `(ptr,len)` 自校验 | 观测 |
| 状态机 | `Stamp` / `ButtonState` / `Effect` / `button_down` / `button_up` / `process` / `pressed` | 四个动作全覆盖（含哨兵与「一次性」） | 观测（字段名 = 推断） |
| LPA `draw` | `DRAW` / `SpinState` / `geometry` / `draw_plan` / `draw_frame` / `DRAW_CONSTS` | 判定 + 算术 + 两次绘制 + 结束条件 | 观测（API 名 = 未知） |
| LPA 样板簇 | `ORIG_FUNCS` 后 6 条 | 只到「是样板」 | 观测 |

`tests/graphwar_button.rs`（18 条）与 `tests/graphwar_long_press_animation.rs`（12 条）
的断言都是**回归保护**；每条右侧的真值要么是指令立即数、要么是 `.rodata` 比特、
要么是 arm64 符号表真名。

---

## 20. `graphwar_ad_switch.rs` 规格（本轮新增：2 个 getter / 1 个字段 / 6 个函数）

落码：`src/graphwar_ad_switch.rs`；验收：`tests/graphwar_ad_switch.rs`。
地址口径：**ELF 虚地址**（Ghidra = +`0x100000`）；凡是引自 `docs/19-android.md`
的地址都是 **Android arm64**，下表逐条标了。

### 20.1 它不是联机层

`docs/02-module-map.md` §14.4 曾把 `graphwar_backend`(6) 与 `graphwar_ad_switch`(7)
一起归到"广告 / 后端"堆里，**这个归类已被 `docs/19-android.md` 更正**：
`graphwar_backend.rs` 是联机层，与广告无关；广告视图的开合在别的模块里
（命令 → `0x3af850` → `call_deferred("create_ad_view"|"destroy_ad_view")`，见 §15）。

本模块只做一件事：给 GDScript 提供**两个只读判据**（开关 + AdMob 单元 id），
自己**不碰**任何 UI、不发任何请求。

### 20.2 两个导出方法

| 方法 | 名字字面量 | 长度 | 注册闭包 A / B | trampoline A / B | 档位 |
|---|---|---|---|---|---|
| `are_ads_enabled() -> bool` | `0x1c4425` | 15 | `0x4f02b0` / `0x4f0520` | `0x422ab0` / `0x42b7e0` | 观测 |
| `get_ad_unit_id() -> GString` | `0x1c4434` | 14 | `0x4f0370` / `0x4f0430` | `0x42e920` / `0x42e500` | 观测 |

四个闭包都还引用**类名字面量** `0x19e6a0`（长度 `0x10` = 16 = `"GraphwarAdSwitch"`，
`lea` 在 `0x4f02d2` / `0x4f0392` / `0x4f045f` / `0x4f054f`）。
两条方法名与 `docs/18-build.md` §6.5 的扩展 API 导出表一致。

⚠️ 闭包 A/B 与 ptrcall / varcall 的对应关系没有逐指令钉死（`docs/19` §3.0.3 在 arm64
上只说"每个方法各一份 ptrcall + varcall，方法体内联在 `handle_fallible_call` 里"），
所以本仓库的代码里按 A/B 记，不硬指。

### 20.3 唯一的字段：`owned`，偏移 `+0x18`

档位：**逐位（arm64）+ 推断（"该字节就是 owned"）**：

* `are_ads_enabled` 的 ptrcall 实例（arm64 `0x377cf4`）= `borrow_mut` →
  `ldrb w22,[obj+0x18]` → `eor w8,w22,#1` → 写回 out 参数 ⇒ **`!owned`**；
* 初值 `1`：`create` 里 `mov w8,#0x1` 写进该槽。跨类判别器：另一个 Control 派生类
  在同一槽写 `3`，所以该槽是**按类不同的实例字段**，不是 gdext 框架常量；
* 全 `.text` 落在 `[reg,#0x18]` 的字节写共 **167 处**，只有 **2 处**属于本类：
  `GraphwarRootNode::purchase_confirmed`（arm64 `0x3a658c`，写 `1`）与
  `purchase_denied`（arm64 `0x3a61b0`，写 `0`）；
* **没有任何 `graphwar_ad_switch` 函数**写这个字节 ⇒ `owned` 只能由 GDScript 改。

⇒ **首次启动 `are_ads_enabled() == false`**（`owned = true`），
只有 `purchase_denied()` 能把它变成 `true`。Rust 侧**没有**"重新查单 / 从存档读
`owned`"的路径（穷举写点已覆盖）。

⚠️ x86 侧 `!owned` 那一步没有逐指令钉死：`0x422ab0` 在 `0x422b26` 确有一条
`movb $0x0,(%r15)` 写返回槽，但它是"借用成功"分支的常量写，不是取反本身。
缺的证据：x86 上 `ldrb/eor` 那一对的地址（arm64 侧已有）。

### 20.4 38 字节的 AdMob 单元 id

`.rodata` `0x1c43ff`，长度 **38**（arm64 侧 `docs/19` §3.0.3 记的是 `0x19dd6c`，
长度同为 38）：

```text
ca-app-pub-3940256099942544/6300978111
```

全 `.text` 里只有两个函数引用它（`0x42e500` / `0x42e920`），正是
`get_ad_unit_id` 的两个 trampoline ——**这条字面量只为这一个方法存在**。

⚠️ 这是 **Google 官方的测试广告单元**。原版在 2.0.60 的构建里**没有**换成正式 id，
所以"游戏靠广告赚钱"在 Linux 构建里**没有证据**；真正的闸门是
`are_ads_enabled()`，而它的初值是"不显示广告"。

### 20.5 六个函数（全部是 gdext 惰性注册驱动）

`docs/19-android.md` §3.0.3 记「全二进制只有 6 条 `FUNC` 符号」；x86 侧用
`panic::Location` 归属也正好 **6 个**，数量一致：

| vaddr | 跨度 | Location | 形状 |
|---|---|---|---|
| `0x4e5100` | `0x180` | `14:1` | once 锁（`lock cmpxchg`）+ 遍历静态函数指针表 |
| `0x4e5280` | `0x180` | `14:1` | 同上 |
| `0x4e9730` | `0x1b0` | `14:1` | 同上 |
| `0x4e98e0` | `0x210` | `7:10` | 同上 |
| `0x4e9af0` | `0x1a0` | `14:1` | 同上 |
| `0x4e9c90` | `0x250` | `45:1` | 同上 |

六条的**跨度多重集**与 `graphwar_function_input` 的 6 个注册驱动**完全相同**
（`0x180/0x180/0x1a0/0x1b0/0x210/0x250`）⇒ 同一组 gdext 单态化被两个类各实例化一次。

Location 只落在 `7:10`、`14:1`、`45:1` 三行，与 `docs/02-module-map.md` §18 A 表给的
`graphwar_ad_switch.rs` 行范围 `7–45` 一致 ⇒ **原版这个文件极短**（一个字段 +
两个 getter + 一个 `#[godot_api]` impl）。

### 20.6 与广告视图的关系

本模块**不**创建/销毁广告视图。两条 `call_deferred` 的名字（`create_ad_view` 字面量
`0x1bd9c1`、`destroy_ad_view` `0x1bd9cf`）由 `0x3af850` 发出，触发条件见 §15 的闸门表；
`GraphwarRootNode.purchase_confirmed` 在关广告时还会
`Ad_holder.call_deferred("destroy_ad_view", 15)`（`docs/19` §3.0.2）。

⇒ 链路是：**买断（GDScript 回调改 `owned`）→ 命令 / `0x3af850` → `create|destroy_ad_view`**，
中间**不经过**本模块的任何函数。

### 20.7 落码清单

| 原版 | 落在 | 覆盖程度 | 档位 |
|---|---|---|---|
| 6 个函数 | `FUNCTIONS` / `function_at` / `ANCHOR_LINES` / `ghidra` | 6 条地址 + 跨度 + 锚 | 观测 |
| 2 个导出方法 | `EXPORTS` / `MethodRegistration` / `export` | 名字 + 长度 + 4 闭包 + 4 trampoline | 观测 |
| `are_ads_enabled = !owned` | `ads_enabled_from_owned` / `AdSwitch::are_ads_enabled` | 真值表 | 观测（arm64 逐指令）/ 字段名推断 |
| `owned` 偏移与初值 | `OWNED_FIELD_OFFSET` = `0x18` / `DEFAULT_OWNED` = `true` | — | 观测 |
| 唯一写者 | `OWNED_WRITERS` / `MODULE_HAS_NO_OWNED_WRITER` / `purchase_confirmed` / `purchase_denied` | 2 条 | 观测（arm64 穷举） |
| 38 字节单元 id | `AD_UNIT_ID` / `AD_UNIT_ID_LITERAL` / `AD_UNIT_ID_LEN` / `AD_UNIT_ID_REFS` | — | 观测 |
| 广告链在别处 | `AD_VIEW_CHAIN` | 一条指针 | 观测（§15） |
| 未知 | `UNKNOWNS`（3 条） | x86 取反位置 / 6 张表的归属 / 字段名本身 | 未知 |

## 21. `entity_drawer.rs` 全量规格（本轮新增：从 1 个函数补到 10 个；士兵到底在哪画）

### 21.0 一句话

`EntityDrawer` 的 **10 个有源级名字的方法**全部落到 x86 地址并解出算术；
**士兵/靶子不是 `draw()` 画的**——它们是一具实体的 **3 个 `Sprite2D` 子节点
（皮肤 / 脸 / 帽子）+ 1 个 `RichTextLabel` 子节点（名字）**，由 `process` 每帧摆位置，
`draw()` 只负责画**瞄准用的三段弧 + 三个箭头**。这与 `src/entity_drawer.rs` 上一轮
「`draw` 里没有 `draw_texture_rect`」的结论**不冲突**，是把那个空洞填上了。

### 21.1 名字从哪来，地址怎么钉的

* **名字**：2.0.60 Android arm64 **未 strip** 的 `.symtab`
  （`.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so`，18,793 个 `FUNC`）。
  与 `docs/02` §18/§20 用的是同一份物证、同一套 `(file,line,col)` 桥。
* **地址**：Linux x86-64 `2026-08-28/libgraphwarrust.so`。
* **判据不是函数大小**（`docs/02` §19 已证否）。本轮用三类语义指纹，逐条可复核：
  1. `ready`：x86 `0x4b4130` 引用的 8 条字符串
     （`on_resized` `0x1c2a7a`、`upcast_ref` `0x1c2a84`、`resized` `0x1c2a8e`、
     `pos_info_event` `0x1c2a95` … `hat_graphics_info_event` `0x1c2af6`）
     与 arm64 `0x4bab44` 的（`0x1a1380`、`0x1a131b`、`0x1a138a`、`0x1a1391` … `0x1a13f2`）
     **内容与长度逐个相等，且两侧都是同一串连续字面量**。
  2. `update_name_label`：`divss [0x1b9fc8]`（= `50.0f32`）+
     `test 0x10000` / `sete` / `cmp 0x1000000` / `setae` / `and 0x101` / `cmp 0x100`
     这串位测试，与 arm64 `0x4bf690` 的 `lsr #24` / `tst 0x10000` / `csel` / `and 0x101`
     / `cset` / `or` / `tst #1` 同构。
  3. **最强的一条：`process` 的调用多重集**。两侧都恰好各调
     `get_entity_skin_sprite` 2 次、`get_entity_face_sprite` 2 次、`get_entity_hat_sprite` 2 次、
     `get_entity_name_label` 2 次、`update_sprite` **3** 次、`update_name_label` **2** 次。
     一次比对同时钉死 6 个地址，而不是 6 个独立猜测。

### 21.2 地址表（**观测**；x86 虚地址，Ghidra = `+0x100000`）

| 源级名 | arm64 | arm64 B | x86 vaddr | x86 B | 比值 |
|---|---|---|---|---|---|
| `draw` | `0x4b9430` | 5300 | **`0x4b2890`** | 5984 | 1.13 |
| `ready` | `0x4bab44` | 2100 | **`0x4b4130`** | 2144 | 1.02 |
| `process` | `0x4bb508` | 3696 | **`0x4b4b60`** | 4176 | 1.13 |
| `update_sprite` | `0x4bf418` | 632 | **`0x4b9530`** | 736 | 1.16 |
| `update_name_label` | `0x4bf690` | 496 | **`0x4b9810`** | 608 | 1.23 |
| `draw_offset_triangle` | `0x4bf880` | 816 | **`0x4b9a70`** | 1024 | 1.25 |
| `get_entity_hat_sprite` | `0x4bfbb0` | 1236 | **`0x4b9e70`** | 1584 | 1.28 |
| `get_entity_name_label` | `0x4c0084` | 2832 | **`0x4ba4a0`** | 3072 | 1.08 |
| `get_entity_face_sprite` | `0x4c0b94` | 1236 | **`0x4bb0a0`** | 1552 | 1.26 |
| `get_entity_skin_sprite` | `0x4c1068` | 1236 | **`0x4bb6b0`** | 1552 | 1.26 |

外加（**不在** 10 个之列，是 gdext 的构造器）：`IControl::init` = x86 **`0x4b3ff0`**
（arm64 `0x4ba8e4`，608 B）。位置在 `draw` 结束（`0x4b3fea`）与 `ready`（`0x4b4130`）之间，
体内 `call __tls_get_addr` 取对象 id 计数器、`movups` 从 `.data.rel.ro`
（`0x95e2e0` / `0x95e2f0`）搬默认字段值写 `+0x18/+0x28/+0x48/+0x58`。

> `fixtures/name_to_addr.tsv` 里 `update_name_label` 与 `draw_offset_triangle` 两行是
> **`未知`**（全函数体不引用任何 `panic::Location`）。本节把这两个补上了；
> `init` / `__register_constants` 那两行仍是 `歧义`，本轮**没有替人选**。

### 21.3 最重要的一条：士兵/靶子在哪画

**结论：`EntityDrawer` 自己有一棵子节点树，士兵是"贴图子节点"，不是 `draw()` 的图元。**

四个 `process` 取用的子节点，按叠放顺序（`Node::add_child` 先后）：

| # | 节点类型 | 从哪来 | 建好后统一设置 |
|---|---|---|---|
| 1 | `Sprite2D` | `GraphwarTextureLoader::get_skin_texture` | `texture_filter = 4`、`flip_v = true` |
| 2 | `Sprite2D` | `GraphwarTextureLoader::get_face_texture` | 同上 |
| 3 | `Sprite2D` | `GraphwarTextureLoader::get_hat_texture` | 同上 |
| 4 | `RichTextLabel` | `load("res://name_label.tscn")` + `instantiate_as` | `push_font_size` / `push_outline_size` / `push_outline_color` / `push_color` / `append_text` / `pop`×3 |

三个 `get_entity_*_sprite` 在 arm64 上**字节大小完全相同（1236）**，归一化后的指令序列
也完全相同——它们是**同一段"懒建 + 缓存"代码的 3 份副本**，差别只有
①取哪张贴图 ②落在哪个 `HashMap` 字段。每份的流程是：

```text
if let Some(gd) = self.<map>.get(&entity_id) { return Some(gd.clone()) }
let tex = loader.get_<kind>_texture(&info)?;      // Option，None 直接落下界
let sp  = Sprite2D::new_gd();                     // construct_engine_object
sp.set_texture(tex);
sp.set_texture_filter(4);                         // mov w1,#0x4  @0x4bfe10
sp.set_flip_v(true);                              // mov w1,#0x1  @0x4bfe30
self.base_mut().add_child(sp);                     // Node::add_child_full
self.<map>.insert(entity_id, sp.clone());
```

`update_sprite` 反而**不建节点**，只算 `set_scale` / `set_position` / `set_visible(true)` /
`set_flip_h` —— 一个"每帧更新"的纯变换函数。

> ⇒ 上一轮 `src/entity_drawer.rs` 开头写的"**不含** `draw_texture_rect` 与 `draw_line`，
> 所以士兵贴图与弹道由别的函数画"这句**对**，但**后半句不完整**：士兵贴图不是"别的函数用
> `draw_texture_rect` 画"，而是**根本不用 `draw()`，用子节点**。弹道仍然不归本模块（见 §21.10）。

### 21.4 `ready`（`0x4b4130`）：1 个信号 + 6 个后端事件

顺序（arm64 `0x4bab44`，逐调用）:

1. `Callable::from_object_method(self, "on_resized")` —— 名长 10
2. `Object::connect("resized", callable)` —— 信号名长 7
   （x86 `0x1c2a7a` / `0x1c2a8e`＝arm64 `0x1a1380` / `0x1a138a`）
3. `strb 1, [self + 0x108]`（x86 `0x4b41be: mov BYTE PTR [rbx+0x108],0x1`）—— 字段名**推断**
4. 6 × `GraphwarBackend::subscribe_to_event`，**按此顺序**：

| # | 事件名 | 长度 | 处理者（`Gd::cast` 后的回调） |
|---|---|---|---|
| 1 | `pos_info_event` | 14 | 位置广播 → 记 `PosInfo` |
| 2 | `life_info_event` | 15 | 存活/死亡 |
| 3 | `entity_removed_event` | 20 | 实体移除 |
| 4 | `skin_graphics_info_event` | 24 | 皮肤贴图 |
| 5 | `face_graphics_info_event` | 24 | 脸贴图 |
| 6 | `hat_graphics_info_event` | 23 | 帽子贴图 |

（6 条在 `.rodata` 里**首尾相接**：arm64 `0x1a1391/…/0x1a13f2`，长度合计 = 区段长。）
6 个 `Gd::cast` + 6 个 `subscribe_to_event` + 2 个 `Gd::from_obj_sys_weak` 一一对应。

### 21.5 `process`（`0x4b4b60`）：每帧的子节点驱动

```text
"show_names" = <某模块对象 +0xa1 的 bool>        // ldrb w9,[x27,#0xa1] @0x4bb748（身份未知）
tick         = <+0x58 的 u32>                    // ldr w8,[x0,#0x58]    @0x4bb824
if !self.ready_flag(+0x108) { 跳过全部 }

for id in pos_manager.get_all_positions() {              // 3b8348 / 0x3b7a78
    let Some((px, py)) = pos_manager.get_position(id) else { continue };
    let alive = life_manager.get_alive(id);

    if alive && show_names {
        // 可见分支
        skin -> get_entity_skin_sprite(id)?  -> update_sprite(s, util, px, py)
        face -> get_entity_face_sprite(id)?  -> update_sprite(s, util, px, py)
        hat  -> get_entity_hat_sprite(id)?   -> update_sprite(s, util, px, py)
        label = get_entity_name_label(id)?
        -> update_name_label(label, util, px, py, alpha = 1.0)   // d2 = 1.0
    } else if alive {
        // 已知名字子节点：藏起来
        label -> set_visible(false)
    } else if show_names {
        // 死亡分支：三个贴图无条件藏、名字按 death_tick 淡出
        skin -> set_visible(false)
        face -> set_visible(false)
        hat  -> set_visible(false)
        label = get_entity_name_label(id)?
        if let Some(dt) = life_manager.get_death_tick(id) {
            let t = tick.saturating_sub(dt);                 // subs + csel lo @0x4bbb30/34
            if t >= 60 { hide(label) }                        // b.hs @0x4bbb3c
            else {
                alpha = max(1.0 + t/(-60.0), 0.0);            // @0x4bbb40..4c
                update_name_label(label, util, px, py, alpha)
            }
        } else { hide(label) }
    }
}

4 × HashMap::retain(...)      // 0x4bbc54 / 0x4bbc60 / 0x4bbc6c / 0x4bbc78，清掉已消失实体的节点
CanvasItem::queue_redraw()    // 0x7e6da0
```

`update_sprite` 内部会 `set_visible(true)`，所以"可见"分支不需要再自己置 `true`；
`process` 里 **5 次 `set_visible` 全是 `false`**（藏）——这一点与"`process` 负责显示"的
直觉相反，值得记下来。

### 21.6 `update_sprite`（`0x4b9530`）：棋盘单位 → 控件像素

入参：`Control::get_size()` 的 `(w,h)`、`get_team_overview()` 的 packed 色、
`Texture2D::get_width()`、棋盘 `(x,y)`。设 `k = w/50`、`s = ±1`（§21.7 的镜像判据）：

```text
position = ( w/2 + s·k·x ,  h/2 − k·y )
scale    = ( (s/60)·(128/tex_w) ,  (k/−60)·(128/tex_w) )
flip_h   = x > 0.0
```

常量（**逐位**）:

| 量 | x86 槽 / 指令 | arm64 | 值 |
|---|---|---|---|
| `k` 的除数 | `0x1b9fc8`，`divss` @`0x4b9679` | `mov w8,#0x42480000` | `50.0f32` |
| 贴图基准边 | —（向量第 2 lane）`divps` @`0x4b972e` | `movi v1.2s,#0x43,lsl#24` | `128.0f32` |
| x 方向除数 | `0x19d1b0`，`0x4b972e` | `mov w8,#0x42700000` | `60.0f32` |
| y 方向除数 | `0x1baacc`，`divss` @`0x4b9736` | `mov w8,#-0x3d900000` | `-60.0f32` |
| `0.5` | `0x1ba128` | `fmov s2,#0.5` | — |

`128.0` 在 x86 侧**不是一条独立常量**：它躺在 `0x19c634`（前一个 float `0x19c630` = `0.0`、
后一个 `0x19c638` = `0.0`），因为 arm64 那条 `movi` 立即数也要靠一个 4-float 向量承载，
`shufps …,0x55`（`0x4b9741`）把 lane1 广播出来。**只按"槽里第一个 float"读会读到 0.0。**

`scale.y` 天然为负 ⇒ Godot 屏幕 y 向下、棋盘 y 向上，和 `position.y` 里的减号是同一个翻转。

### 21.7 名字标签：`update_name_label`（`0x4b9810`）与死亡淡出

镜像判据（`update_sprite` 与 `update_name_label` **各算一遍、算式相同**）：

```text
mirror = ((c & 0x101) == 0x100)                     // 绿置位且红不置位
       | ((c & 0x10000) == 0 && (c >> 24) != 0)     // 蓝不置位且 alpha != 0
s = mirror ? -1.0 : +1.0
```

布局（`w,h` = 标签自身 `get_size`；`mw,mh` = `get_minimum_size`；`k = w/50`）：

```text
cx    = w/2 + s·k·x − mw/2
cx'   = max(cx, 0)                              // fcmp/fcsel mi @0x4bf7b4
x_pos = if mw + cx' > w { w − mw } else { cx' } // fcmp @0x4bf7d8（上界）
base  = (h/2 − k·y) + k·(−1.5) − mh/2
alt   = base + (k·1.75 + mh/2)
y_pos = if base < 0 { alt } else { base }       // fcmp/fcsel mi @0x4bf7ec
```

然后 `set_modulate(Color(1,1,1, alpha))` → `set_position_full(x_pos, y_pos, false)`
→ `set_visible(true)`。`alpha` 是第 3 个 `f64` 实参，**只进 modulate**，不影响位置。

⚠️ 注意 `alt` **没有再夹一次**：`base < 0` 只是换一支，`alt` 本身仍可为负
（`w=h=500/300, mw=80, mh=40, y=20` ⇒ `base = −85`、`alt = −47.5`）。

死亡淡出（`process` 内联，不在 `update_name_label` 里）：

```text
t     = tick.saturating_sub(death_tick)     // 0x4bbb30 subs / 0x4bbb34 csel lo
if t >= 60 { 藏 }                            // 0x4bbb38 cmp #0x3c / 0x4bbb3c b.hs
alpha = max(1.0 + t/(−60.0), 0.0)            // 0x4bbb40 ucvtf / 44 fdiv / 48 fadd / 4c fmaxnm
```

`−60.0` 是 **`f64`** 位型 `0xC04E000000000000`（`mov x8,#-0x3fb2000000000000` @`0x4bb850`），
与 §21.6 那个 `f32` `−60.0`（`0xC2700000`）**不是同一条常量**，只是数值相等。

### 21.8 `draw_offset_triangle`（`0x4b9a70`）＝ `draw()` 里的箭头

**这就是上一轮代码里那个"匿名包装 `0x4b9a70`"的真名。** 它画一个图元：

`Object::call("draw_polygon", [PackedVector2Array, PackedColorArray])`

* 方法名 `"draw_polygon"` 长 12：x86 `0x1c2bad` / `mov ecx,0xc` @`0x4b9d41`，
  arm64 `0x1a14a9` / `mov w2,#0xc` @`0x4bfa94`。参数个数 2（`mov r9d,0x2`）。
* 顶点：`P0 = C + r·(cosθ,sinθ)`、`P1 = C + (r+0.5)·(cos(θ+c₂),sin(θ+c₂))`、
  `P2 = C + (r+0.5)·(cos(θ+c₁),sin(θ+c₁))`。`c₁ = +0.20943951023931953`（x86 槽 `0x19fb68`，
  `addsd` @`0x4b9ac0`）、`c₂ = −0.20943951023931953`（槽 `0x19fc80`，`addsd` @`0x4b9b74`）——
  **两个独立槽**，不是取同一个值的相反数。`r += 0.5` 用 `0x19f6d0`（`addsd` @`0x4b9b5c`）。
* 颜色：同一个 `Color` **push 3 份**（x86 `movupd xmm0,[r14]` @`0x4b9c1e/0x4b9c3c/0x4b9c5c`）。

⇒ 既有 `arrow_head` / `arrow_primitives` 的**几何与着色结论一条都不用改**，
本轮只是把名字补上，并把 `arrow_primitives(inp) == 3 × offset_triangle(...)` 写成断言。

### 21.9 与既有 `draw` 结论有无冲突

**没有。** 逐条对账：

| 既有结论（上一轮） | 本轮 | 结果 |
|---|---|---|
| 段角 `A0=V/120`、`±2π/3`、`±4π/3` | 未重解（不在 10 个里） | 不动 |
| 半径 = `f32(radius)×1.5` | 未重解 | 不动 |
| 箭头 = 向心三角，`±12°`、外沿 `r+0.5` | 本轮由 `draw_offset_triangle` 独立复核，逐条相同 | **互证** |
| 颜色 = packed `/255`，3 份同色 | 本轮确认 `draw_polygon` 侧也是 3 份同色 | **互证** |
| `draw` 不含 `draw_texture_rect` / `draw_line` | 仍然不含（全函数体再核过一遍） | 一致 |
| "士兵贴图与弹道由别的函数画" | 士兵贴图**不是**别的函数画的，是**子节点**；弹道仍不归本模块 | **补全**（不是矛盾） |

唯一需要"改口"的是措辞：那句话把"别的函数画"暗示成"别处也是 `draw_*` 图元"，
实际是"**根本不是图元，是节点**"。已在 `src/entity_drawer.rs` 的模块头就地改写。

### 21.10 诚实清单（**未知**）

1. `process` 里"要不要画名字"那个 `bool` 取自某个模块对象的 `+0xa1`
   （`ldrb w9,[x27,#0xa1]` @arm64 `0x4bb748`）——**哪个模块、字段真名 = 未知**。
2. 4 次 `HashMap::retain` 的谓词只看得出是"清掉已消失实体的节点"，
   **"消失"的判据 = 未知**。
3. `process` 里有一个 `TimeProbe`（`Instant::now` / `elapsed` / `std::io::_print`）
   ⇒ 开了帧耗时打印，**格式串未解**（只看到一句 `: dt:` 片段 `0x1918de`）。
4. 4 个 `HashMap` 的字段名（只有偏移：`0xa8` / `0xc0` 等）= **推断**。
5. `ready` 之后 `+0x108` 那个 `bool` 的真名（写成"已就绪"）= **推断**。
6. `SPRITE_TEXTURE_FILTER = 4` 的**数值**是观测（`mov w1,#0x4`），
   "4 == Godot 的 `TEXTURE_FILTER_LINEAR_WITH_MIPMAPS`" 是查文档得来的 = **推断**。
7. **弹道（`draw_line`）不在本模块**。上一轮已扫过全 GOT 表，本轮 `process` / `draw` 的
   直接调用表里也没有画线原语 ⇒ 仍在别的模块（`graphwar_plane` / `graphwar_effect_manager`
   候选，**本轮未证，不替人选**）。

### 21.11 落码清单

| 原版 | 落在 `src/entity_drawer.rs` | 覆盖程度 | 档位 |
|---|---|---|---|
| 10 个方法的地址 | `ENTITY_DRAWER_FUNCS`（名字 + x86 vaddr + arm64 `st_size`） | 10/10 | 观测 |
| `IControl::init` | `ENTITY_DRAWER_INIT_ADDR` = `0x4b3ff0` | 1 | 观测 |
| `ready` 的 1 信号 + 6 事件 | `RESIZED_SIGNAL` / `RESIZED_HANDLER` / `READY_EVENTS` / `READY_FLAG_OFFSET` | 全 | 观测（字符串 + 长度 + 顺序） |
| 子节点树 | `EntityChildNode` / `ENTITY_CHILD_NODES` / `NAME_LABEL_SCENE` / `SPRITE_TEXTURE_FILTER` / `SPRITE_FLIP_V` | 4 类节点 | 观测 |
| 镜像判据 | `team_mirror` / `team_mirror_sign` | 全 | 观测（逐条立即数） |
| `update_sprite` | `sprite_transform` / `SpriteTransform` / 4 个常量 | 全 | 观测 |
| `update_name_label` | `name_label_layout` / `NameLabelLayout` / `PIXELS_PER_UNIT_DIVISOR` | 全 | 观测 |
| 死亡淡出 | `death_alpha` / `DEATH_FADE_TICKS` / `DEATH_FADE_DIVISOR` | 全 | 观测 |
| `draw_offset_triangle` | `offset_triangle`（+ 既有 `arrow_head`） | 全 | 观测 |
| 未知 | `UNKNOWNS`（5 条） | — | 未知 |

`tests/entity_drawer.rs`：既有 11 个测试**一条未改**，新增 8 个（共 19）。
按 `AGENTS.md` §0.2，这些条数**是回归保护，不是复原证据**。

---

## 22. `player_showcase.rs` 规格（本轮新增：4 个方法体 / 10 个函数 / 三层贴图 + 档案字段）

> **本节解决什么**：`player_showcase` 是 12 个 GDExtension 类里**名字最容易被想歪**的一个
> ——"showcase"听着像主菜单的装饰，实际是**玩家名片预览**：一行名字输入框 + 三层贴图
> （皮肤 / 脸 / 帽子）拼出来的头像。本节把它的**10 个函数、18 个 panic 锚、
> 4 个方法体、档案字段偏移、三次绘制的五个实参**全部钉下来。
>
> **地址口径**：一律 **ELF 虚地址**（`objdump` 口径）。Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。
> 引用的 **arm64** 地址只用于**定名**（2.0.60 的 Android 构建没 strip），绝不当 x86 用。
> 落码：`src/player_showcase.rs`；验收：`tests/player_showcase.rs`（22 条，全部是回归保护）。

### 22.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 它是什么 | **角色外观预览**：`PlayerNameLineEdit`（名字）+ 皮肤/脸/帽子三层贴图 | **观测** |
| 方法体 | 只有 **4 个**：`ready` / `on_text_change` / `process` / `draw`（其余 6 个是 gdext 生成物） | **观测**（arm64 `.symtab`） |
| 画什么 | 三次 `CanvasItem::draw_texture_rect_full`，**不是**多边形 / `draw_circle` / `draw_rect` | **观测** |
| 分层顺序 | **皮肤 → 脸 → 帽子**（后画的压在上面） | **观测**（三次绘制与三次取图配对） |
| 矩形 | `Rect2(0, 0, Control::get_size())` —— 铺满整个控件矩形，**不居中、不裁成正方形** | **观测**（x86 `0x4e5cc3`/`0x4e5ccf`/`0x4e5cd8`） |
| 颜色 | `modulate` 恒为 `.rodata 0x19ed00` = `Color{1,1,1,1}`；`tile=false`、`transpose=false` | **观测** |
| 跟 `graphwar_color_manager` 的关系 | **没有关系**（有界否定，见 §22.9） | **观测** |
| 它读的档案 | 根节点 `+0x16a8` 的 `GraphwarPlayerConfig`：名字 `+0x00`、皮肤 `+0x60`、脸 `+0x78`、帽 `+0x90`、变更计数 `+0xa8` | **观测**（字段名是推断） |
| 它读的贴图源 | 根节点 `+0x16a0` 的 `GraphwarTextureLoader`：三张 HashMap 在 `+0x158`/`+0x188`/`+0x1b8`（间距 `0x30`） | **观测** |
| 它自己的状态 | **只有一个 `u32`**：上次看到的变更计数，偏移 `+0x18` | **观测**（字段名推断） |
| 还缺什么 | `__default_virtual_call` / `class_id` 的 x86 地址；两个 `#[godot_api]` 块各注册了什么；`ready` 为什么取两次同一个节点 | 未知（§22.11） |

### 22.2 名字从哪来：TSV 的 7 行里有 4 行是硬锚

```bash
awk -F'\t' '$1=="player_showcase"' fixtures/name_to_addr.tsv
```

| 方法 | x86 | TSV 判定 | 本节处理 |
|---|---|---|---|
| `draw` | `0x4e5400` | 唯一锚（**7 个**双向唯一 Location，展示锚 `src/player_showcase.rs:72:129`） | **采用** |
| `ready` | `0x4e6480` | 唯一锚（4 个，`:18:129`） | **采用** |
| `process` | `0x4e68f0` | 唯一锚（2 个，`:59:133`） | **采用** |
| `on_text_change` | `0x4efd70` | 唯一锚（2 个，`:31:129`） | **采用** |
| `__register_constants` | 4 候选 | 歧义 | 定到 **`0x4e12e0`**（§22.3，**推断**） |
| `__default_virtual_call` | — | 未知 | 不落地 |
| `class_id` | — | 未知 | 不落地 |

**两条"未知"是真未知，不是没找**：这两个函数**一个 `panic::Location` 都不引用**
（`__default_virtual_call` 是 156 字节的薄壳，`class_id` 是 56 字节的 `once` 读取），
`src/player_showcase.rs` 的 18 个 Location 已经**全部**被另外 10 个函数占满
（21 条指令级引用点 = `fixtures/module_map.tsv` 的 `refs=21`，`functions=10` 也对得上）
⇒ 这两个函数只能靠跨架构指纹落，本轮**没有**做（登记在 `src` 的 `UNKNOWNS` 与冲突表里）。

**为什么 4 条"唯一锚"可信**：`Location` 是编译期常量 `{file, line: u32, col: u32}`（24 字节），
与指令集无关。两侧的 `(file,line,col)` 集合**逐条相同**（`only-arm` / `only-x86` 都为空），
于是 10 个函数在两侧是**双射**——不是靠"函数大小相近"配的。
⚠️ 顺便记一条反例：`draw` 的 x86/arm64 字节比是 **1.59**（3264/2652），
远高于别处记的 1.03–1.07；原因很明确：**x86 把三个取贴图函数内联进了 `draw`**
（`0x4e5717..0x4e5c69` 三段 inlined SwissTable 查表），arm64 则保留成 `bl`。
所以"大小相近"既不是必要条件也不是充分条件。

### 22.3 10 个函数（**观测**；x86 地址 + 两侧字节数 + 角色）

| # | x86 | Ghidra 跨度 | 真实字节 | arm64 | arm B | 角色 | 锚 |
|---|---|---|---|---|---|---|---|
| 1 | `0x4e1160` | 384 | — | `0x43c1bc` | 400 | `__register_methods` | `27:1` |
| 2 | `0x4e12e0` | 384 | — | `0x43c34c` | 400 | `__register_constants`〔推断：左右序〕 | `27:1` |
| 3 | `0x4e5400` | 4224 | **3264** | `0x43fe7c` | 2652 | **`IControl::draw`** | 7 条 |
| 4 | `0x4e6480` | 1136 | — | `0x4408d8` | 1108 | **`IControl::ready`** | 4 条 |
| 5 | `0x4e68f0` | 656 | — | `0x440d2c` | 656 | **`IControl::process`** | 2 条 |
| 6 | `0x4e87d0` | 416 | — | `0x442adc` | 428 | `__inner_init` | `27:1` |
| 7 | `0x4e8970` | 432 | — | `0x442e80` | 448 | `__inner_init` | `27:1` |
| 8 | `0x4e8b20` | 592 | — | `0x443040` | 512 | `__inner_init` | `37:1` |
| 9 | `0x4e8d70` | 528 | — | `0x442c88` | 504 | `#[derive(GodotClass)]` 生成 | `9:10` |
| 10 | `0x4efd70` | 672 | — | `0x448d10` | 704 | **`on_text_change`** | 2 条 |

两处**必须记住的坑**：

* **`0x4e5400` 的真实边界是 `0x4e60bf`**（收尾 `ret` 在 `0x4e60be`），
  `0x4e60bf..0x4e6476` 是**外置冷路径**（`lea "Upcast failed…"(47)` + `core::result::unwrap_failed`），
  Ghidra 因为后面没有新起点把这 240 字节并进了 `draw` 的跨度 ⇒ **跨度 ≠ 函数体**。
* `__register_constants` 那 4 个候选（`0x4e1160`/`0x4e12e0`/`0x4e87d0`/`0x4e8970`）**全部是真函数**，
  它们是**同一个** `#[godot_api]` 块的 4 个生成物。两个 `__register_*` 的 x86 跨度都是 384、
  arm64 都是 400 ⇒ **大小区分不开**，只能按 arm64 符号表的地址序对应 ⇒ 记 **推断**。

### 22.4 18 个 Location → panic 种类（**本节最强的一条证据**）

每个 `(line, col)` 通向一个**具体的 panic 出口**，而 panic 的种类能从被调符号直接读出来
（`core::option::unwrap_failed` / `core::cell::panic_already_borrowed` /
`core::cell::panic_already_mutably_borrowed` / `core::result::unwrap_failed`）。
⇒ `(line, col)` → "这一行在干什么"是**可证**的，不是从行号猜的：

| `(line:col)` | 引用指令（x86） | 归属 | panic 种类 | 说明 |
|---|---|---|---|---|
| `9:10` | `0x4e8f00` | `0x4e8d70` | 样板 unwrap | `#[derive(GodotClass)]`（col 10 = `GodotClass` 的 G） |
| `18:129` | `0x4e67d7` | `0x4e6480` | `Option::unwrap` | 取 `"GraphwarRootNode"` |
| `21:132` | `0x4e684d` | `0x4e6480` | `Option::unwrap` | 取 `"PlayerNameLineEdit"` |
| `22:47` | `0x4e685c` | `0x4e6480` | **共享**借用冲突 | 读档案名字 |
| `27:1`×4 | `0x4e125b`/`0x4e13db`/`0x4e88ed`/`0x4e8aa1` | 1/2/6/7 | 样板 unwrap | `#[godot_api]` 块 A |
| `31:129` | `0x4eff14` | `0x4efd70` | `Option::unwrap` | `on_text_change` 取 root |
| `33:23` | `0x4eff61` | `0x4efd70` | **独占**借用冲突 | `bind_mut()`（计数置 `-1`） |
| `37:1` | `0x4e8ce6` | `0x4e8b20` | 样板 unwrap | `#[godot_api]` 块 B |
| `52:132` | `0x4e686b` | `0x4e6480` | `Option::unwrap` | **第二次**取名字输入框（连线用） |
| `59:133` | `0x4e6ad8` | `0x4e68f0` | `Option::unwrap` | `process` 取 root（列比另外三处多 4） |
| `62:41` | `0x4e6b1d` | `0x4e68f0` | **共享**借用冲突 | 读档案计数 |
| `72:129` | `0x4e60f4` | `0x4e5400` | `Option::unwrap` | `draw` 取 root（**在冷路径里**） |
| `76:38`/`77:38`/`78:37` | `0x4e618e`/`0x4e55f0`/`0x4e619d` | `0x4e5400` | 共享借用 | 皮肤 / 脸 / 帽子的 `String` clone |
| `80:39`/`81:39`/`82:38` | `0x4e61ac`/`0x4e61bb`/`0x4e61ca` | `0x4e5400` | 共享借用 | 三个取贴图调用 |

**列号签名是本节最漂亮的一条判别式**：两组三行里，**第三行都比同组另两行少 1 列**
（`78:37` 对 `76:38`/`77:38`；`82:38` 对 `80:39`/`81:39`）。
少 1 的原因可证：`hat`(3) 比 `skin`/`face`(4) 少一个字符，
`get_hat_texture`(15) 比 `get_skin_texture`/`get_face_texture`(16) 也少一个字符。
⇒ "78 行是帽子、82 行是取帽子贴图"是**推出来的**。这条被写进
`borrow_column_signature()` / `group_column_shortfall()` 并由测试对拍
（`hat_lines_are_one_column_shorter`）。

⚠️ 唯一一条 `Result::unwrap`（消息 47 字节 = `"Upcast failed. This is a bug; please report it."`）
用的是 **gdext 自己文件**里的 Location（x86 `.data.rel.ro 0x853ce8`），
所以本文件的 18 个 Location 里**一条都没有**这个种类（`check_rtti` 的失败口）。

### 22.5 `ready`（`0x4e6480`，1136 B）

```text
44092c  self.base()                      → Gd<PlayerShowcase>
44095c  get_node_by_name(:<root>, "GraphwarRootNode", 16).expect(…)
44097c  root.bind()                      → 共享借用（cell+0x10 +1）
4409b4  self.base()                       （重建一遍 Gd）
440a24  get_node_by_name::<LineEdit>(…, "PlayerNameLineEdit", 18).expect(…)   ← 第一次
440a38  check_rtti("upcast_ref", 10)     ← gdext 固定的 10 字节串，**不是**类名
440a5c  ldp x1,x2,[cell+0x20]            ← 档案名字的 (ptr,len)
440a6c  LineEdit::set_text(名字)
440b04  get_node_by_name::<LineEdit>(…, "PlayerNameLineEdit", 18).expect(…)   ← 第二次
440b40  Callable::from_object_method(self, "on_text_change", 14)
440b58  Object::connect(编辑框, "text_changed", 12, callable)
```

* 三条字面量在 `.rodata` 里**首尾相接**：`0x1c42ed`(18) → `0x1c42ff`(14) → `0x1c430d`(12)。
* **取两次同一个节点是事实**（两个不同的 `get_node_by_name::<LineEdit>` + 两个不同的
  Option unwrap Location `21:132` / `52:132`），不是抄错；**为什么**取两次未知（§22.11）。
* `.expect()` 失败 = **panic**（原版没有"找不到节点就静默跳过"的路径）。

### 22.6 `on_text_change`（`0x4efd70`，672 B）

```text
448d9c  get_node_by_name(:<root>, "GraphwarRootNode", 16).expect(…)
448dbc  root.bind_mut()                  → **独占**借用（cell+0x10 = -1）
448e08  String::from(GString)            ← 新名字
448e14  释放 载荷+0x00 的旧 String（__rust_dealloc）
448e30  写 载荷+0x00 的 (cap,ptr,len)
448e3c  GraphwarPlayerConfig::save(&载荷.名字)
448e40  [载荷+0xa8] += 1                 ← **变更计数 +1**（在 save 之后）
```

⇒ **先存档、再加计数**：`save()` 拿到的已经是新名字，但计数在返回之后才变，
所以同一帧的 `process` 还看不到变化。

### 22.7 `process`（`0x4e68f0`，656 B）

```text
440d6c  CanvasItem::is_visible_in_tree()  ; false ⇒ **直接返回**（连档案都不碰）
440de0  get_node_by_name(:<root>, "GraphwarRootNode", 16).expect(…)
440e00  root.bind()                       → 共享借用
440e48  w21 = [载荷+0xa8]                 ← 档案的变更计数
440e4c  w8  = [self+0x18]                 ← 自己的上一份
440e50  cmp / b.eq → 相等就结束
440e64  base_mut() + CanvasItem::queue_redraw()
440e8c  [self+0x18] = w21                 ← **重画之后**才写回
```

⇒ 本类**只有一个 `u32` 字段（`+0x18`）**。初值 `0` 与 `GraphwarPlayerConfig::new`
里 `str wzr,[sp,#0xa8]` 一致 ⇒ 刚进场景不重画。
计数在三个地方被 `+1`：`on_text_change`、`set_skin`、`set_face`、`set_hat`
（后三个在 `graphwar_player_config.rs`，不在本模块）⇒ **任何外观变化都会让预览重画**。

### 22.8 `draw`（`0x4e5400`，真实 3264 B）

```text
4e5488  get_node_by_name(:<root>, "GraphwarRootNode", 16).expect(…)   （锚 72:129）
        root.bind()                       → 三次 String clone（皮肤/脸/帽，锚 76/77/78）
        loader.bind()                     → 三次取贴图（锚 80/81/82）
4e5cbd  Control::get_size()               → (s0,s1)
4e5cc3  Rect2.position = (0,0)            ← 原点清零，**不居中**
4e5ccf  Rect2.size     = (s0,s1)          ← 铺满整个控件矩形
4e5d2d  modulate = .rodata 0x19ed00 = Color{1,1,1,1}
4e5d77  draw_texture_rect_full(#1)  皮肤   tile=false(ecx=0) transpose=false(r9d=0)   [GOT 9bc248]
4e5ea9  draw_texture_rect_full(#2)  脸     同上
4e5faf  draw_texture_rect_full(#3)  帽子   同上
```

五个实参逐个可指（arm64 同形，`0x4402b0`/`0x44037c`/`0x440444`）：

| 实参 | 值 | 判据 |
|---|---|---|
| `texture` | `Some(t)` 才画 | `Option` 的 tag 判别（arm64 `cmp x24,#1`） |
| `rect` | `Rect2(0, 0, get_size())` | 上面的 `4e5cc3`/`4e5ccf` |
| `tile` | `false` | `xor ecx,ecx` / `mov w3,wzr` |
| `modulate` | `Color{1,1,1,1}` | `.rodata 0x19ed00` / `fmov v0.4s,#1.0` |
| `transpose` | `false` | `xor r9d,r9d` / `mov w5,wzr` |

⚠️ `0x19ed00` 与 §19.7（`GraphwarLongPressAnimation::draw` 第 2 次绘制）**同一个地址**
—— 两个模块共用一个"白"。
缺贴图的那层**不画、也不补占位图元**（`draw` 里没有 `draw_rect`/`draw_circle`/`draw_string`）。

### 22.9 它读谁：根节点的两个"持有单元"，以及它跟颜色表**无关**

`GraphwarRootNode` 上两个**指针→单元**字段（单元 = `RcInner{strong,weak,RefCell{borrow,value}}`，
所以 `[单元]`=强引用计数、`[单元+0x10]`=借用计数、`[单元+0x18]`=载荷）：

| 偏移 | 内容 | 写者（全 `.text` 唯一） | 谁读 |
|---|---|---|---|
| `+0x16a0` | `Rc<RefCell<GraphwarTextureLoader>>`（贴图缓存） | `<GraphwarRootNode as INode>::init`（arm64 `0x39a44c`） | 本模块 `draw`；另有 `EntityDrawer::draw`、`GraphwarSetupControl`、`GraphwarFunctionInput`、`GraphwarLanVBox`、`GraphwarPlayerScreenControl::skin_button_pressed` |
| `+0x16a8` | `Rc<RefCell<GraphwarPlayerConfig>>`（玩家档案） | 同一个 `init`（arm64 `0x39a450`） | 本模块 4 个方法；`GraphwarRootNode` 的 `process_button_pressed`/`purchase_confirmed`/`purchase_denied` |

判据是**穷举写点**：`.text` 里 `str [+0x16a0]` / `str [+0x16a8]` **各只有一条**，
都在 `<GraphwarRootNode as INode>::init`（arm64 `0x399dd0`）里。

⚠️ **就地更正**：本节第一稿把这两条 `str` 记成 `GraphwarRootNode::ready`——
原因是扫描脚本把过长的 trait impl 符号名截断到 95 字符，`…::re_expor` 之后看不见了。
按符号边界复核（`ranges_from_symtab` 的前驱起点）后是 **`INode::init`**，
语义也更合理：两个单元在节点构造时建好，`ready` 不可能是唯一写者。

**`GraphwarPlayerConfig` 的布局**（档位：偏移=观测，字段名=推断）：

| 偏移 | 类型 | 内容 | 证据 |
|---|---|---|---|
| `+0x00` | `String` | **名字** | `on_text_change` 覆盖 `+0x00..0x18`；`ready` 把 `+0x08`/`+0x10` 当 `(ptr,len)` |
| `+0x18` | `String` | "原始"皮肤（`lock()` 写 `yellow`） | `lock()` 里 `str x8,[x19,#0x18]` |
| `+0x30` | `String` | "原始"脸（`lock()` 写 `regular_eyes`） | 同上，12 字节字面量 |
| `+0x48` | `String` | "原始"帽（`lock()` 写 `soldier_helmet`） | 同上，14 字节字面量 |
| `+0x60` | `String` | **皮肤**（当前） | `set_skin` 的 `ldr x1,[x21,#0x60]!` + `str q0,[x21]` |
| `+0x78` | `String` | **脸** | `draw` 的第二个 `(ptr,len)` 对 |
| `+0x90` | `String` | **帽子** | `draw` 的第三个 `(ptr,len)` 对 |
| `+0xa8` | `u32` | **变更计数器**（`new` 清 0；四处 `+1`；`process` 轮询） | arm64 `0x448e40..0x448e54` |
| `+0xac` | `bool` | 锁定/标记位（`lock()` 写 0，`set_skin` 读它） | `ldrb w8,[x0,#0xac]` |

**`Rust String` 在本二进制里是 `(cap, ptr, len)`**（24 字节）——**不是**常见的 `(ptr, cap, len)`：
* 释放序：`on_text_change` 里 `ldr x1,[x19,#0x18]!`（非零才释放）+ `ldr x0,[x20,#0x20]`
  → `__rust_dealloc(x0, x1, 1)` ⇒ `x1` 是 size/cap、`x0` 是 ptr；
* 使用序：`ready` 把 `+0x08`/`+0x10` 当 `(ptr,len)` 传给 `set_text`。
若照搬常见布局，`skin` 的 `len` 会被读成 `cap`。

**三个默认外观名**（`GraphwarPlayerConfig::new` 的字面量，arm64 `0x39152c..0x3915a4`）：

```text
skin = "yellow"           (6)
face = "regular_eyes"     (12)
hat  = "soldier_helmet"   (14)
```

**`GraphwarTextureLoader` 三张表**（间距 `0x30`，每张一个 `HashMap<String, Gd<Texture2D>>`）：

| 外观 | getter（arm64） | 表控制字节 | hasher | 桶/掩码 |
|---|---|---|---|---|
| 皮肤 | `get_skin_texture` `0x40c1cc` | `+0x168` | `+0x170` | `+0x150` |
| 脸 | `get_face_texture` `0x40c0a4` | `+0x198` | `+0x1a0` | `+0x180` |
| 帽子 | `get_hat_texture` `0x40bf7c` | `+0x1c8` | `+0x1d0` | `+0x1b0` |

### 22.9.1 与 `graphwar_color_manager` 的关系：**没有**（有界否定）

本模块 10 个函数的**具名被调者穷举**如下（arm64 `.symtab` 逐条给出，`CALLEES`）：
`get_node_by_name` / `RawGd::bind` / `RawGd::bind_mut` / `Gd::from_obj_sys_weak` /
`Gd::owned_cast` / `RawGd::check_rtti` / `Control::get_size` /
`CanvasItem::draw_texture_rect_full` / `CanvasItem::is_visible_in_tree` /
`CanvasItem::queue_redraw` / `GraphwarTextureLoader::get_skin_texture` /
`GraphwarPlayerConfig::save`。

* 里面**没有**任何 `GraphwarColor` / 颜色表 / 队色 / 段位函数；
* 三次绘制的 `modulate` 是**纯白常量**（`0x19ed00`），不做任何乘法；
* 玩家颜色 / 段位的落点是 `graphwar_player_screen_control` 等别的模块，
  **不经过** `player_showcase`。

⇒ "玩家名片按玩家颜色染色"这个猜想在本构建里**没有证据**；如果要染色，
染色发生在该控件的 GDScript 侧（场景里给 `modulate`），Rust 侧不参与。

### 22.10 与 `client/` 仿制实现的出入（**登记，不改 `client/`**）

⚠️ 按 `AGENTS.md` §0 第 4 条：`client/` 是**改写**，不是原版源码；下面只登记差异，
**本轮不动 `client/`**。

| # | `client/src/play/player_showcase.gd` | 原版（本节证据） | 影响 |
|---|---|---|---|
| 1 | `side = min(size.x, size.y)`，再**居中**成正方形 | `Rect2(0,0,get_size())`——**不裁、不居中** | 容器非 1:1 时两者不同。原版靠外层 `AspectRatioContainer` 保证方形（与 `docs/129` §1.3 的 46.875% 实测一致）；缺的证据：原版 `.tscn` 里这个容器是否恒为 1:1 |
| 2 | `draw_texture_rect(tex, rect, false)`（4 参） | `draw_texture_rect_full(tex, rect, false, WHITE, false)`（5 参） | **等价**（默认 modulate 就是白、transpose=false），但 API 不同 |
| 3 | `set_look(int(p.skin), int(p.face), int(p.hat))`，图标名 `skin_%d` | 原版用**字符串键**（`"yellow"`/`"regular_eyes"`/`"soldier_helmet"`）查三张 HashMap | 客户端的"整数下标 → 名字"映射是**自造**的（映射表在 `docs/17-resources.md`，与本模块无关） |
| 4 | 由 GDScript 显式 `_showcase.queue_redraw()` | 原版 `process` **轮询变更计数**（`+0x18` vs 档案 `+0xa8`）+ 可见性闸门 | 原版连 `set_skin/set_face/set_hat` 也会自动触发重画；客户端靠调用点自觉 |
| 5 | 名字输入框在 `player_screen.gd` 里**动态创建**（`LineEdit.new()`，`max_length = 12`） | 原版按**名字**从根节点找 `PlayerNameLineEdit`（`get_node_by_name`），且它自己 `connect("text_changed")` → **Rust** 的 `on_text_change` → 写档案 + `save()` | 客户端把"改名"写进 GDScript；原版写进 `GraphwarPlayerConfig` 并落盘。`max_length = 12` 在 Rust 侧**没有**证据 |
| 6 | 找不到节点只报错 | `.expect()` ⇒ **panic** | 原版没有"优雅降级"这条路 |

真正需要**改 `client/`** 的两条（留给后续轮次，不在本轮范围）：
第 1 条（改成铺满整个矩形）与第 4 条（改成让 Rust 侧轮询计数器）。

### 22.11 本节没解出来的（照实写）

| 缺口 | 缺什么证据 |
|---|---|
| `__default_virtual_call`（arm64 156 B）与 `class_id`（arm64 `0x438088`，56 B）的 x86 地址 | 两条都**不引用任何 Location**；`class_id` 的 `CLASS_ID` 静态槽（arm64 `0x8b40b0`）在 x86 的地址也没解 |
| 两个 `#[godot_api]` 块（行 27 / 行 37）各注册了什么 | 块 27 有 4 个生成物（2 个 `__register_*` + 2 个 `__inner_init`），块 37 只有 1 个；哪一个是 `#[func]`、哪一个是 `#[signal]`/属性，静态分不出（缺：注册闭包写进 `.data.rel.ro` 的 `(名字,len)` 表项） |
| `ready` 为什么**取两次** `PlayerNameLineEdit` | 两个 unwrap 是事实；中间那 30 行源码（行 23–51）里到底是什么，静态只能看到"取 → set_text → 再取 → connect" |
| `draw` 里 6 个借用点与源码行的**一一对应** | 能证明两组各三次、且每组第三行是帽子（列号少 1）；但"哪一次借用配哪一行"缺（x86 把 `bind` 内联后没有独立调用点） |
| 类名字面量 `PlayerShowcase` 的 `.rodata` 地址与长度 | 注册闭包里没有可自校验的 `(类名ptr, 类名len)` 立即数对；`check_rtti` 拿到的 10 字节串是 gdext 固定的 `"upcast_ref"`，**不是**类名 |
| 四个字段的**源码标识符** | 偏移与读写指令都钉住了；名字是按 `set_skin/set_face/set_hat` 与默认字面量推的。`+0x18`/`+0x30`/`+0x48` 三槽的语义也未定 |
| 原版 `.tscn` 里 `PlayerShowcase` 外层是不是恒 1:1 的 `AspectRatioContainer` | 缺 `.tscn` 文本（`AspectRatioContainer` 只在客户端仿制品里出现过） |

### 22.12 复现（不依赖本节结论，任何人可重跑）

```bash
SO=.reference/steam/2026-08-28/libgraphwarrust.so          # sha256 9a538115…
ARM=.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so   # 2.0.60，**未 strip**

# 1) 真名与真边界（x86 被 strip，arm64 没有）
readelf -sW $ARM | c++filt | grep player_showcase

# 2) 10 个函数 + 18 个 Location 的双射（工具自己做两侧一对一判定）
../.venv/bin/python tools/name_bridge.py module player_showcase

# 3) 四个方法体逐条（x86 用 objdump；arm64 要 llvm-objdump 才有符号名）
objdump -d -M intel --start-address=0x4e5400 --stop-address=0x4e60bf $SO
llvm-objdump -d --no-show-raw-insn --demangle \
  --start-address=0x43fe7c --stop-address=0x4408d8 $ARM

# 4) 「写者唯一」：扫 .text 里 disp ∈ {0x16a0,0x16a8} 的写指令
#    → 各 1 条，都在 <GraphwarRootNode as INode>::init（arm64 0x39a44c / 0x39a450）
```

### 22.13 落码清单

| 原版 | 落在 `src/player_showcase.rs` | 覆盖程度 | 档位 |
|---|---|---|---|
| 10 个函数（名/地址/两侧字节/角色/锚） | `OrigFn` / `FUNCTIONS` / `function_at` / `by_role` / `total_anchor_sites` | 10 条（跨度为 Ghidra 起点表，`draw` 另给真实 3264） | 观测 |
| arm64 真名与真边界 | `ARM64_SYMS` / `arm_sym` / `arm_size_of` / `arm_of` / `ARM64_VIRTUAL_FN` | 12 + 3 条 | 观测 |
| 18 个 Location → panic 种类 | `LocAnchor` / `LOCATIONS` / `locations_of` / `borrow_sites_of` / `borrow_column_signature` | 21 条指令级引用点、18 条去重 | 观测 |
| 节点名 / 方法名 / 信号名 | `NODE_ROOT`…`SIGNAL_TEXT_CHANGED_LEN` / `LITERAL_RUN` / `literal_pair_ok` / `node_name_ok` | 4 条 `(ptr,len)` 全自校验 | 观测 |
| 根节点两个持有单元 | `ROOT_PLAYER_CONFIG` / `ROOT_TEXTURE_LOADER` / `ROOT_UNIT_WRITERS` / `root_unit_writer` | 写者唯一 | 观测 |
| 档案布局与 `String` 三字序 | `CFG_*` / `StringLayout` / `DEFAULT_*` / `UNIT_PAYLOAD` / `UNIT_BORROW` | 偏移全给，字段名推断 | 观测 / 推断 |
| `ready` / `on_text_change` / `process` / `draw` | `READY_PLAN` / `on_text_change_effect` / `Showcase` / `ProcessOutcome` / `DrawCall` / `draw_plan` / `DRAW_ORDER` | 四个方法体的判定与算术全覆盖 | 观测（引擎 API 名 = 观测，见 §22.8） |
| 与颜色表无关 | `CALLEES` / `touches_color_manager` | 有界否定 | 观测 |
| TSV 冲突 | `NAME_BRIDGE_CONFLICTS` / `conflict` | 3 条 | 观测 / 推断 |
| 未知 | `UNKNOWNS`（6 条）/ `DOC_SECTIONS` / `REPRO` | — | 未知 |

`tests/player_showcase.rs`（22 条）的断言都是**回归保护**；每条右侧的真值要么是
指令立即数、要么是 `.rodata` 比特、要么是 arm64 `.symtab` 真名。
本轮 `cargo test --offline`：lib **339**、总计 **645**（+22），doctest 0 条通过。

---

## 23. `graphwar_plane.rs` 规格（最后一个 GDExtension 类）+ 悬案结案：**弹道不是 `draw_line` 画的**

> **本节解决两件事**：① 12 个 GDExtension 类里最后一个 `GraphwarPlane` 的**全部方法落到地址**；
> ② 悬了很久的「弹道（`draw_line`）到底在哪个模块画」——结论是**没有任何模块用 `draw_line` 画弹道**，
> 曲线走的是 **`Line2D` 子节点**，由 `function_drawer` 配置、`GraphwarPlane::draw` 驱动。
>
> **地址口径**：一律 **ELF 虚地址**（`objdump` 口径）。Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。
> 引用的 **arm64** 地址只用于**定名**（2.0.60 的 Android 构建没 strip），绝不当 x86 用。
> 落码：`src/graphwar_plane.rs`（新，占位→实现）＋ `src/plane.rs`（只搬走 1 个函数）；
> 验收：`tests/graphwar_plane.rs`（18 条，全部是回归保护）。

### 23.0 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 它是什么 | `Control` 子类，棋盘本体：画棋盘框 + 每帧驱动障碍绘制器 + 做脏矩形 + 持 3 个 `FunctionDrawer` 子节点 | **观测** |
| 源级方法数 | **8 个**方法（`draw` / `init` / `ready` / `process` / `on_resized` / `clear_state` / `update_entities` / `obstacle_partial_update_event`）+ **2 个** `ready::{{closure}}`；其余是 gdext 样板 | **观测**（arm64 `.symtab`） |
| 落到 x86 地址 | 8 个方法 **8/8**（5 个 `唯一锚`、3 个靠「调用者集合 + 首尾相接」）；1 个闭包；gdext 入口 6 个（其中 4 个只到**推断**） | 观测 / 推断（逐条标） |
| `draw` 画什么 | **两个**图元：`draw_set_transform` ×1 + `draw_rect` ×1（`Rect2(-25,-15,50,30)`、黑、`filled=false`、宽 `0.2`） | **逐位**（常量槽逐个给了地址） |
| 弹道在哪画 | **不在本类、也不在任何 `draw_*` 图元里**——是 `Line2D` 子节点 | **观测**（穷举 + 有界否定，§23.4） |
| `plane.rs` 搬走了什么 | 只搬 `to_pixel`（1 个函数），`plane.rs` 保留 `pub use` 再导出 ⇒ 既有断言一行未改 | 观测（归属锚） |
| 未知 | 实体 tag `0x28`/`0x29` 的语义、`draw` 里 5 个无锚被调者、`on_resized` 的三个 setter、字段真名 | **未知**（§23.5） |

### 23.1 名字从哪来，地址怎么钉的（三类指纹，可逐条复核）

> ⚠️ **2026-11-07 撤回**：本节曾把 `tools/class_probe.py` 的归属错误归因为「`analysis/xrefs.pkl` 与 `.so` 不是同一次扫描 / pkl 已过期」。**这个判断是错的**，已在 §24.1 更正：
> 实测 `xrefs.pkl` 与本版 `.so` **逐条一致**（5301/5301 个 Location 的引用指令集合完全相同），
> 真因是 `class_probe` 把**已经是虚地址**的引用点又过了一遍 `off2va()` ⇒ 整体 `+0x1000`。
> 本节其余内容（三类指纹、逐条复核方法）仍然有效。

* **名字**：arm64 2.0.60 未 strip 的 `.symtab`（`graphwarrust::graphwar_plane::GraphwarPlane` 下
  有源级名的 `FUNC` 共 8 个方法 + 2 个闭包）。
* **判据不是函数大小**（`docs/02` §19 已证否）。本节用三类：

| # | 指纹 | 一句话 | 定死了谁 |
|---|---|---|---|
| 1 | **`panic::Location` 键集** | 两侧各 **19** 个 `src/graphwar_plane.rs:(line,col)` 键，**一个不多一个不少**；按函数分组后**完全同构**（`draw` 9 键 / `ready` 4 键 / `process` 2 键 / `obstacle_partial_update_event` 1 键 / gdext 入口 4 个共享 `:68:1`） | `draw` `ready` `process` `obstacle_partial_update_event` + `init`（锚在 `obstacle_drawer.rs:37:97`） |
| 2 | **被调者多重集** | 两侧逐个相等：`ready` 3× `subscribe_to_event`；`process` 2× `ObstacleDrawer::update` + 1× `get_team_overview`；闭包 1× `obstacle_partial_update_event` | `ready` / `process` / 闭包 |
| 3 | **调用者集合 + 首尾相接** | arm64 与 x86 的调用者**逐个相同**，且三个函数在两侧都**严丝合缝**：`on_resized`+span = `clear_state`，`clear_state`+span = `update_entities`，`update_entities`+span = `obstacle_partial_update_event` | `on_resized` / `clear_state` / `update_entities` |

第 3 条的具体数字（x86 span 取「到下一个 Ghidra 函数起点的距离」，含对齐填充）：

```text
0x4bddb0 + 0x120 = 0x4bded0   on_resized  → clear_state
0x4bded0 + 0x3a0 = 0x4be270   clear_state → update_entities
0x4be270 + 0x170 = 0x4be3e0   update_entities → obstacle_partial_update_event
```

arm64 侧同样相接：`0x4c3308`+256 = `0x4c3408`，`0x4c3408`+900 = `0x4c378c`，
`0x4c378c`+332 = `0x4c38d8`。

> ⚠️ **本轮不采信 `tools/class_probe.py` 的地址**。它读的是上游 `analysis/*.pkl`，那些 pkl 与
> `2026-08-28` 这份 `.so` **不是同一次构建**：同一行号 `:274:129` 它归 `0x4b7a70`，本轮按
> `.rela.dyn` + capstone 重扫归 `0x4b5df0`；`:218:129` 它归 `0x4b8bd0`，重扫归 `0x4b7a70`；
> `:68:1` 它归 `0x4b2890`，重扫归四个 gdext 入口。**重扫可自证**：`0x95e4f8`（`:274:129`）
> 在整份 `.text` 里只有 **1** 条引用指令（`lea rdi,[rip+0x4a772c]` @ `0x4b6dc5`），落在
> `0x4b5df0..0x4b7380` 之内。
>
> ⇒ 结论：**pkl 那一套（`docs/140`/`149` 时点的 `class_probe`）对本版 `.so` 已经过期**，
> 引用它的旧表要按本节重核。

### 23.2 地址表（**观测**；x86 虚地址，Ghidra = `+0x100000`）

| 源级名 | x86 vaddr | x86 span | arm64 | arm64 B | 判据 |
|---|---|---|---|---|---|
| `draw` | **`0x4b5df0`** | 0x1590 | `0x4bc54c` | 4592 | 键集 9/9 |
| `init` | **`0x4b7380`** | 0x06f0 | `0x4bd73c` | 1336 | `obstacle_drawer.rs:37:97` 双向唯一 |
| `ready` | **`0x4b7a70`** | 0x05a0 | `0x4bdc74` | 1400 | 键集 4/4 + 3× subscribe |
| `ready::{{closure}}` | **`0x4b8210`** | 0x01e0 | `0x4be1ec` | 404 | 两侧都调 `obstacle_partial_update_event` |
| `process` | **`0x4b83f0`** | 0x03f0 | `0x4be534` | 956 | 键集 2/2 + 2× `ObstacleDrawer::update` |
| `on_resized` | **`0x4bddb0`** | 0x0120 | `0x4c3308` | 256 | 唯一游戏内调用者 = `ready`；相接 |
| `clear_state` | **`0x4bded0`** | 0x03a0 | `0x4c3408` | 900 | 调用者 = 根节点 `process` + `process_button_pressed`；相接 |
| `update_entities` | **`0x4be270`** | 0x0170 | `0x4c378c` | 332 | 调用者只有根节点 `process_button_pressed`；相接 |
| `obstacle_partial_update_event` | **`0x4be3e0`** | 0x0150 | `0x4c38d8` | 260 | `graphwar_plane.rs:174:18` 双向唯一 |

gdext 入口（不是原版写的方法，是 `#[gdextension]` 生成的样板）：

| 源级名 | x86 vaddr | arm64 | 判据 |
|---|---|---|---|
| `_::__init::__inner_init(:33:10)` | **`0x4bc490`** | `0x4c1e28` | 观测（唯一锚） |
| `_::__init::__inner_init(:179:1)` | **`0x4bc860`** | `0x4c1c58` | 观测（唯一锚） |
| `__register_methods` | `0x4b2260` | `0x4b8e60` | **推断**（`:68:1` 组内次序 + 等长 384↔400） |
| `__register_constants` | `0x4b23e0` | `0x4b8ff0` | **推断**（同上） |
| `_::__init::__inner_init(:68:1)a` | `0x4bc6b0` | `0x4c2000` | **推断**（组内次序） |
| `_::__init::__inner_init(:68:1)b` | `0x4bcab0` | `0x4c21c0` | **推断**（组内次序） |

> `docs/08` §3 当年记的「`0x4bcab0` 是 Rust lazy static 初始化」= 这一族 `_::__init::__inner_init`
> （`std::sync::OnceLock::initialize` + `Mutex` + panic 路径）。**归属对了，名字现在有了**。

### 23.3 四个方法体（**观测**）

**`init`（`0x4b7380`）**：建 **3 个 `FunctionDrawer`** 并 `add_child`。
arm64 侧 `Gd::default_instance` ×3、`ExAddChild::new` ×3、`Node::add_child_f…` ×3；
x86 侧 `0x4db030` ×3 + 三组 `0x406d20`/`0x407030`/`0x406520`。另有
`Image::create`（arm64）/ `Control::set_c…` 各 1 次，**用途未知**。

**`ready`（`0x4b7a70`）**：一个信号 + 3 个后端事件，与 `EntityDrawer::ready` 同一套写法：

```text
Callable::from_object_method(self, "on_resized")     ; .rodata 0x1c2a7a 长 10  (0x4362f0)
Object::connect("resized", callable)                 ; .rodata 0x1c2a8e 长 7   (0x3a0fb0)
GraphwarBackend::subscribe_to_event ×3 (0x44b7f0)    ; 名字首尾相接：
  add_obstacle_event      0x1c2b32 长 18
  destroy_obstacle_event  0x1c2b44 长 20
  clear_obstacles_event   0x1c2b5a 长 19
```

`ready` 体内还直接调了一次 `on_resized`（`0x4bddb0`）—— 两侧相同，这也是 §23.1 第 3 类的锚。

**`process`（`0x4b83f0`）**：每帧 `ObstacleDrawer::update`（`0x3ee190`）**2 次** +
`GraphwarDrawUtil::get_team_overview`（`0x4be6e0`）**1 次**。没有别的游戏内调用。

**`draw`（`0x4b5df0`）**：47 条直接调用（gdext / std 的辅助调用不计）。其中**只有 4 个目标能定名**
（共 5 个调用点）：`0x4be6e0` `get_team_overview` ×2、`0x519530` `get_fractional_tick`、
`0x456df0` `get_active_functions_on_tick`、`0x498c70` `FunctionDrawer::update_function`。
其余 **11 个**游戏内目标**一个 Location 锚都没有**（无 module 可归，也不替人起名）。
按调用顺序（首次出现）全列如下：

| # | 目标 | 名字 | 说明 |
|---|---|---|---|
| 1 | `0x4be6e0` | `graphwar_draw_util::get_team_overview` | 队伍色（调 2 次） |
| 2 | `0x499050` | `<未定名>` | 体内 `Line2D::set_width` ×2（`0x49911c` / `0x49921c`） |
| 3 | `0x46bb90` | `<未定名>` | — |
| 4 | `0x46a770` | `<未定名>` | — |
| 5 | `0x46b5d0` | `<未定名>` | — |
| 6 | `0x46aa50` | `<未定名>` | — |
| 7 | `0x519530` | `graphwar_tick_handler::get_fractional_tick` | 当前 tick 的小数进度 |
| 8 | `0x456df0` | `graphwar_function_manager::get_active_functions_on_tick` | 本 tick 生效的函数 |
| 9 | `0x3bf010` | `<未定名>` | — |
| 10 | `0x498b70` | `<未定名>` | FunctionDrawer 家族 |
| 11 | `0x498c70` | **`function_drawer::FunctionDrawer::update_function`** | 体内 `Line2D::add_point`（`0x498fab`）+ `set_default_color`（`0x498e30`） |
| 12 | `0x4992b0` | `<未定名>` | 体内 `Line2D::set_default_color`（`0x4993a3`） |
| 13 | `0x4971e0` | `<未定名>` | FunctionDrawer 家族 |
| 14 | `0x4993c0` | `<未定名>` | 体内 `draw_circle` ×5；引用 `"Write a function of x to hit the target"`（`0x1c21a7`） |
| 15 | `0x46abc0` | `<未定名>` | — |

两个图元（逐位）：

```text
draw_set_transform(...)                                         ; 槽 0x9bc018，调用点 0x4b607d
draw_rect(Rect2(-25,-15,50,30), Color(0,0,0,1), false, 0.2, false)
  ; 槽 0x9bc028，调用点 0x4b61ab
  ; Rect2 槽 0x19cc20（与 AxisDrawer 同一个槽）、Color 槽 0x19cf70、
  ; 宽度槽 0x1ba598 = cd cc 4c 3e = 0.2f32；filled: xor ecx,ecx @0x4b61a6；
  ; antialiased: xor r8d,r8d @0x4b61a8
```

**`on_resized`（`0x4bddb0`）**：`Control::set_*` ×3 + `get_*` ×1（都走 GOT 槽，方法名未解），
288 B，无其它调用。**`clear_state`（`0x4bded0`）**：`Node::queue_free` ×2 + `HashMap/Vec` 清理，
并引用 `EntityDrawer` / `EffectDrawer` 两个类名（用途见 §23.5）。
**`update_entities`（`0x4be270`）**：与 `clear_state` 同型但只 `queue_free` ×1。

### 23.4 悬案结案：**弹道不是 `draw_line` 画的**（有界穷举）

**做法**：把 `CanvasItem` 的每个绘图方法名（`.rodata` 的方法名字面量）→ 包装函数 → `.got` 槽
的全链条解出来，然后**穷举** `.text` 里所有对该槽的调用，**两种形态都算**：

* `call *0x…(%rip)`（直接读槽）；
* `mov 0x…(%rip),%reg` + `call *%reg`（寄存器间接，**必须也扫**——`AxisDrawer` 的
  8 个 `draw_line` 里有 2 个是这种形态，只扫前者会漏）。

| 方法名 | 字面量 | 包装 | GOT 槽 | 调用点数 | 调用者 |
|---|---|---|---|---|---|
| `draw_line` | `0x29ef7a` | `0x8abff0` | `0x9bc228` | **10** | `AxisDrawer::draw` `0x4e05b0` **×8**（`0x4e0775` `0x4e0815` `0x4e089f` `0x4e0944` `0x4e09c6` `0x4e0a74` `0x4e0b02` `0x4e0b62`）+ `EffectDrawer::draw` `0x4e1520` **×2**（`0x4e2104` `0x4e2234`） |
| `draw_arc` | `0x19f9d0` | `0x8abca0` | `0x9bc020` | 5 | `EntityDrawer::draw` `0x4b2890` ×3 + `GraphwarLongPressAnimation::draw` `0x4d8100` ×2 |
| `draw_circle` | `0x29ef8c` | `0x8ac690` | `0x9bbfc0` | 9 | `0x4993c0` ×5 + `EffectDrawer::draw` ×4 |
| `draw_polygon` | `0x29ef97` | — | `0x9bc238` | 1 | `EffectDrawer::draw` ×1（`0x4e1d8f`） |
| `draw_rect` | `0x29ef83` | — | `0x9bc028` | 2 | **`GraphwarPlane::draw` ×1**（`0x4b61ab`）+ `AxisDrawer::draw` ×1 |
| `draw_texture_rect` | `0x29efd8` | — | `0x9bc248` | 3 | `PlayerShowcase::draw` `0x4e5400` ×3 |
| `draw_set_transform` | `0x29efe9` | — | `0x9bc018` | 4 | `EntityDrawer` / **`GraphwarPlane`** / `AxisDrawer` / `EffectDrawer` 各 1 |

**有界否定的三条**（每一条都覆盖了整个 `.text`）：

1. **`draw_polyline` / `draw_multiline` / `draw_dashed_line` / `draw_primitive` / `draw_mesh` /
   `draw_string` / `draw_char` 这些方法名在 `.rodata` 里根本不存在** ⇒ 不可能是它们画的
   （与 `docs/00-status` 那条旧结论一致，本轮复跑确认）。
2. **`draw_line` 的 10 个调用点里没有一个是曲线**：8 个在 `AxisDrawer`（网格线，端点/颜色/线宽
   已由 `src/axis_drawer.rs` 逐条落码），2 个在 `EffectDrawer`（爆炸十字，`src/effect_drawer.rs` 已落码）。
   ⇒ **`graphwar_plane` / `graphwar_effect_manager` / `graphwar_draw_util` 三个候选全部排除**
   （`GraphwarPlane::draw` 里 `draw_line` 调用点 = 0；`graphwar_draw_util` 只有 1 个带符号函数
   `get_team_overview` `0x4be6e0`，它不调任何绘图原语；`graphwar_effect_manager` 的
   `add_effect` / `tick` / `new` 三个函数也都不在下面这 7 个里）。
3. **全二进制只有 7 个函数调用过任何绘图原语**：`EntityDrawer::draw`(0x4b2890)、
   `GraphwarPlane::draw`(0x4b5df0)、`GraphwarLongPressAnimation::draw`(0x4d8100)、
   `0x4993c0`（未定名）、`AxisDrawer::draw`(0x4e05b0)、`EffectDrawer::draw`(0x4e1520)、
   `PlayerShowcase::draw`(0x4e5400)。

**那曲线是怎么来的？—— `Line2D` 子节点**（观测）：

| `Line2D` 方法 | GOT 槽 | 调用点数 | 落点 |
|---|---|---|---|
| `add_point` | `0x9bbf98` | **1** | `0x498fab`，在 **`FunctionDrawer::update_function`（`0x498c70`）** 体内（循环里） |
| `set_default_color` | `0x9bbf90` | 2 | `0x498e30`（同一个 `update_function`）+ `0x4993a3`（`0x4992b0`） |
| `set_width` | `0x9bbfa0` | 2 | `0x49911c` / `0x49921c`，都在 `0x499050` 里 |

而 `0x498c70` / `0x499050` / `0x4992b0` **三个都被 `GraphwarPlane::draw`（`0x4b5df0`）调用**。
⇒ 平面的 `draw` 每帧把"当前 tick 的函数"喂给 `FunctionDrawer`，后者往自己的 `Line2D` 里
`add_point`；曲线是**节点**，不是图元。这与 `entity_drawer` 那条"士兵不是图元、是子节点"
（§21.3）是同一个设计取向。

> **推断（未证）**：`update_function` 的入参里有 `get_fractional_tick`（`0x519530`）与
> `get_active_functions_on_tick`（`0x456df0`）⇒ 曲线长度随 tick 增长，像是**开火动画**
> 而不是静态预览。**缺的证据**：`add_point` 那处循环的上界与步长没逐条读；
> 也没有运行时帧对拍（谁能逐帧量 Line2D 的 `points` 就能定案）。

### 23.5 本节没解出来的（照实写）

1. 实体 tag **`0x28` / `0x29`** 各是哪一类（士兵 / 障碍 / 靶子）：`0x4be3e0` 只做枚举判别
   （niche 优化形态），没有类型名字面量可比。
2. `draw` 里 **5 个无 Location 锚**的被调者（`0x499050` `0x4992b0` `0x4971e0` `0x4993c0`
   `0x46bb90`…）的**函数名**：只能确认它们在 `function_drawer` 的地址簇里。
3. `on_resized` 里 3 次 `Control::set_*` 设的是哪三个属性（走 GOT 槽，槽 → 方法名的映射未做）。
4. `obstacle_partial_update_event` 开头那次 `call *0x9bb288`（`0x4be41f`）：入参是 `size`，
   出参直接乘 1000；**是什么转换未定**（`docs/08` §4 记成 `FUN_00a30650`）。
5. `clear_state` 引用 `EntityDrawer` / `EffectDrawer` 类名的用途（只是 `cast` 目标？还是清子节点？）。
6. `GraphwarPlane` 的**字段真名与偏移**：只确证了那个 `Vec` 的三个槽 `+0x50`(cap) / `+0x58`(ptr) /
   `+0x60`(len)。
7. 曲线到底是不是**弹道**（开火轨迹）：见 §23.4 末尾的「推断」。

### 23.6 复现（不依赖本节结论，任何人可重跑）

```text
# ① 两侧的 Location 键集（本节的指纹 1）
python3 - <<'PY'
import sys; sys.path.insert(0,'tools'); import name_bridge as NB
for p in (NB.DEFAULT_ARM64, NB.DEFAULT_X86):
    e = NB.Elf(p); locs = e.locations()
    ks = sorted({k for k in locs.values() if k[0]=='src/graphwar_plane.rs'})
    print(p.split('/')[-3:], len(ks), ks[:4], '...')
PY

# ② draw_line 的槽与全部调用点（含 mov slot,%reg + call *%reg）
#    字面量 0x29ef7a -> 包装 0x8abff0；.rela.dyn 里 addend==0x8abff0 的槽 = 0x9bc228
objdump -d --start-address=0x8abff0 --stop-address=0x8ac1a0 \
        .reference/steam/2026-08-28/libgraphwarrust.so | grep -E '29ef7a|5d1f43f3'

# ③ 三条首尾相接（本节的指纹 3）
python3 - <<'PY'
import pickle
s = sorted(pickle.load(open('analysis/funcstarts_ghidra_va.pkl','rb')))
for a in (0x4bddb0,0x4bded0,0x4be270):
    i = s.index(a); print(hex(a), 'span', hex(s[i+1]-a), '-> next', hex(s[i+1]))
PY
```

### 23.7 落码清单

| 原版东西 | 落在 | 覆盖 | 档位 |
|---|---|---|---|
| 8 个方法 + 1 个闭包的地址 | `PLANE_FUNCS`（名字 + x86 + span + arm64 + 长度 + 判据） | 9/9 | 观测 |
| 6 个 gdext 入口的地址 | `PLANE_GDEXT_ENTRIES` | 6/6 | 观测（2）/ 推断（4） |
| 首尾相接链 | `PLANE_TAIL_CHAIN`（+ 两侧断言） | 4 | 观测 |
| 坐标变换 | `to_pixel`（**从 `src/plane.rs` 搬来**，那边保留 `pub use`） | 1 | 逐位 |
| 脏矩形 | `PixelRect` / `obstacle_rect` / `obstacle_partial_update_event`（含"非 0x28/0x29 就 panic"） | 全 | 观测 |
| `draw` 的图元 | `BOARD_RECT` / `BOARD_COLOR` / `BOARD_WIDTH` / `BOARD_FILLED` / `BOARD_ANTIALIASED` + 两个 GOT 槽 | 全 | 逐位 |
| `init` / `ready` / `process` 的意图 | `FUNCTION_DRAWER_CHILDREN` / `READY_EVENTS` / `RESIZED_SIGNAL` / `RESIZED_HANDLER` / `OBSTACLE_DRAWER_UPDATES_PER_FRAME` + `GraphwarPlane` 意图结构 | 全 | 观测 |
| 未知 | `UNKNOWNS`（6 条） | — | 未知 |

`tests/graphwar_plane.rs`（18 条）与 `src/graphwar_plane.rs` 内的 9 条单测都是**回归保护**；
`src/plane.rs` 的 6 条既有断言**一条未改**（`to_pixel` 变成再导出，`use super::*` 照样拿到）。
本轮 `cargo test --offline`：lib **361**、总计 **707**（+27），doctest 阶段 0 条通过（本仓库无 Rust doctest，
文档里的 shell 一律用 ```` ```text ```` 围栏）。

## 24. 工具修订：`class_probe.py` 的归属列已修复（新扫描器 `tools/loc_xrefs.py`）+ 需要重核的旧结论

> 本节回答三件事：① §23.1 对 `class_probe` 的**归因是错的**——不是 pkl 过期，是代码把
> `off2va()` 套在已经是虚地址的引用点上；② 归属链路已换成**只读目标 `.so` 的独立扫描器**，
> 一条命令复算、并拿 327 个唯一锚与 `fixtures/module_funcs.tsv` 双向交叉验证；
> ③ 全库范围内**哪些旧结论要重核**（4 处确定冲突 + 1 处可结案 + 2 条 TSV 地址 + 1 条标注）。
>
> **地址口径**：本节一律 **ELF 虚地址**（`objdump` 口径）。Ghidra/REA 地址 = 本值 + `0x100000`（只加一次）。

### 24.0 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| v1 的真 bug | `Index.funcs_ref_slot()` / `classes` 把 `xrefs.pkl` 里**已经是虚地址**的引用点又过一遍 `off2va()` ⇒ 每条引用 `+0x1000`，归属整体右漂一个函数 | **逐位**（27.1 三例复算全中） |
| pkl 是否过期 | **没有**。5301 个 Location 的「引用指令集合」与独立新扫描**逐条相同** | **逐位** |
| 新扫描器 | `tools/loc_xrefs.py`：`.rela.dyn` + `.eh_frame` + capstone；**不读任何 `analysis/*.pkl`**，不需要 Ghidra | 观测（可复算） |
| 引用判据 | 4 种形态（R/I/G/D）× 首址/`+8`/`+16`；本版实测 **R=9739，G/I/D=0** | **观测**（`stats` 逐形态报数） |
| 函数边界 | `.eh_frame` FDE（13430 条，覆盖 `.text` **98.35%**，全部起点通过指令边界校验）＋ FDE 外 `call/jmp` 段内目标 63 ＋ 段首 | 观测（可复算） |
| 对 `fixtures/module_funcs.tsv` | **544 行 / 56 个模块逐条相同** | **逐位** |
| 对 `fixtures/name_to_addr.tsv` 的 327 唯一锚 | **324 一致**；2 条地址不在指令边界/已被编译器合并；1 条 TSV 自标「唯一锚」但证据栏写着「消去法」 | **观测**（27.3 逐条列出） |
| 归属变化规模 | 5301 个 Location 里 **5019 个（94.7%）** 的函数归属变了；只有 282 个「碰巧对」（函数体 > `0x1000` 字节，右漂仍落在同一函数内） | **逐位** |
| 要重核的旧结论 | 4 处**冲突**、1 处**可结案**、2 条 TSV 地址、1 条标注；另有 **13 个模块**没有 fixture 兜底 | 27.4 |

### 24.1 真因：不是 pkl 过期，是 `off2va()` 套错了一次（**更正 §23.1**）

§23.1 写的是「`analysis/*.pkl` 与 2026-08-28 这份 `.so` 不是同一次构建 ⇒ 过时」。
**这条归因是错的**，三条反证：

1. 上游 `libgraphwarrust.so` 与 `.reference/steam/2026-08-28/libgraphwarrust.so`
   **sha256 相同**（`9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187`）。
2. `xrefs.pkl` 里那一跳**本来就是对的**：
   `xrefs.pkl[0x95e4f8] == [(0x4b6dc5, 'rip', 'lea', 'rdi, [rip + 0x4a772c]')]`，
   而 `0x4b6dc5` 正是 `src/graphwar_plane.rs:274:129` 在整份 `.text` 里的**唯一**引用点
   （与 §23.1 自己的「重扫可自证」一句完全对上——那句话其实已经推翻了「pkl 过期」）。
3. 5301 个 Location 的引用指令集合，与新扫描器**逐条相同**（无一条多、无一条少）。

真因在 `class_probe.py` 自己身上：`xrefs.pkl` 的 `insn_addr` 已经是**虚地址**
（`analysis/build_index.py` 用 `md.disasm(blob, t['addr'])` 生成），而 v1 又过了一次
`off2va()`，`.text` 的「虚地址 − 文件偏移」= `0x1000`：

```python
# tools/class_probe.py —— v1（错误）
def funcs_ref_slot(self, slot):
    for t in self.xrefs.get(slot, ()):
        va = self.off2va(t[0])       # ← t[0] 已经是 vaddr；再套一次 off2va = +0x1000
        f  = self.func_of(va)
```

三例复算（`+0x1000` 后再落到 Ghidra 函数表上，与 §23.1 记录的症状**完全一致**）：

| Location | 引用指令 | 正确函数 | v1 算成 `+0x1000` | v1 归属 |
|---|---|---|---|---|
| `:274:129`（`0x95e4f8`） | `0x4b6dc5` | **`0x4b5df0`** | `0x4b7dc5` | `0x4b7a70` |
| `:218:129`（`0x95e540`） | `0x4b7ee0` | **`0x4b7a70`** | `0x4b8ee0` | `0x4b8bd0` |
| `:68:1`（`0x95e280`） | `0x4b235b` / `0x4b24db` / `0x4bc7cd` / `0x4bcbe1` | **`0x4b2260` / `0x4b23e0` / `0x4bc6b0` / `0x4bcab0`** | `0x4b335b` / `0x4b34db` / `0x4bd7cd` / `0x4bdbe1` | `0x4b2890` / `0x4bd650` / `0x4bda50`（4 个塌成 3 个） |

> `docs/16` §16.1.1 当时的判断**是对的**（就是 `off2va` 平移 `0x1000`，且指出
> `module_map.py` 没这个 bug、fixture 可信）。§23.1 是后来的回退，本节把它改回来。

### 24.2 新扫描器：`tools/loc_xrefs.py`（一条命令复现）

```text
../.venv/bin/python tools/loc_xrefs.py stats        # 判据逐形态计数 + 目标 sha256 + 自检
../.venv/bin/python tools/loc_xrefs.py check-tsv    # 327 个唯一锚交叉验证（27.3）
../.venv/bin/python tools/loc_xrefs.py loc src/graphwar_plane.rs:274:129
../.venv/bin/python tools/loc_xrefs.py slot 0x95e4f8
../.venv/bin/python tools/loc_xrefs.py func 0x4b5df0
../.venv/bin/python tools/loc_xrefs.py dump refs    # 全量 xref TSV
```

只依赖 **Python 标准库 + capstone**。目标 `.so` 用 `--so` / `GW2_TARGET_SO` 指定，
缺省 `.reference/steam/2026-08-28/libgraphwarrust.so`。全量扫描 ≈ 45 s（1 521 067 条指令）。

**覆盖判据（四种引用形态，全部实测计数）**

| 代号 | 形态 | 判据 | 本版命中 |
|---|---|---|---|
| **R** | rip-relative 直引 | 任意指令的 `[rip+disp]`，`insn.end+disp == loc{,+8,+16}` | **9739** |
| **I** | 立即数直引 | 任意指令的立即数操作数 `== loc{,+8,+16}` | 0 |
| **G** | 寄存器携带 | 寄存器先被 `lea r,[rip+X]` / `mov r,[rip+slot]` / `mov r,imm` 赋成该地址，再作 `call *r` / `jmp *r` / 内存基址（**这就是「只匹配 `call *(slot)` 会漏」的那一类**） | 0 |
| **D** | 数据槽携带 | 某个 `R_X86_64_RELATIVE` 数据槽的**值**本身指向该 Location，代码经该槽间接取到 | 0 |

* `G`/`D` 的 0 是**实测值，不是没实现**：`G` 走寄存器污点传播（遇分支/调用/写回即清空），
  `D` 走「全表扫 24501 条 RELATIVE 重定位的 addend 是否落在 Location 集合里」。
  同一套代码对**普通函数指针槽**不是 0：全库 `G` 命中 **3624 条**（其中 128 条落在
  `.data.rel.ro`，即「表基址 + 位移」的间接调用落点，共 79 个地址）——所以
  「`call *[rip+slot]`」与「`mov slot,%reg` + `call *%reg`」两种形态都覆盖了。
* 一个**必须记住的假阳性坑**：本工具第一版在 Location 侧报出「2 条 R 之外的 G 引用」，
  查下去是 `nop dword ptr [rax + rax]` 这类**多字节填充**——寄存器恰好被污点命中。
  现在 `nop` 整体不参与引用判定。**凡是声称「多找到 N 条引用」的工具，都要先把 N 条逐条读过。**
* `+8`/`+16` 两个阶段本版**命中 0**：这个构建里编译器一律 `lea` 结构体首址，
  没有单独取 `len` / `line` 的写法。判据保留，是因为它决定「换个构建还对不对」。

**函数边界（不依赖 Ghidra）**：`.eh_frame` 的 FDE `initial_location + range`
（13430 条，`.text` 内 13426 条，覆盖 `0x5ba84e` / `0x5d3244` 字节 = **98.35%**；
13426 个起点**全部**是最长指令流扫出来的指令边界）＋ FDE 覆盖不到的 `call/jmp` 段内目标
（63 个，`.text` 内 6 个 + `.text` 尾 2 个 + `.plt` 55 个桩）＋ 段首。
合计 **13494 个函数起点**。5301 个 Location 的**全部引用点无一落在 FDE 覆盖之外的缝隙**，
所以归属里没有「最近前驱兜底」的成分。**作为对照**，`analysis/funcstarts.pkl` 是坏的
（最小起点 `0x37b94c` 比真值偏 `0xc`，最大 `0x969054` 越过 `.text` 末尾），本工具不读它。

**两条自洽性检查（都已通过）**

| 检查 | 结果 |
|---|---|
| 与 `analysis/xrefs.pkl` 对打（pkl 只当对照） | 5301 / 5301 个 Location 的引用指令集合**完全相同** |
| 与 `fixtures/module_funcs.tsv` 对打（`module_map.py` 生成，本来就对） | 544 行 / 56 模块**逐条相同**（函数地址集合逐一相等） |

### 24.3 交叉验证：`fixtures/name_to_addr.tsv` 的 327 个唯一锚

`check-tsv` 的判据：TSV 每行断言「`x86_addr` 这个函数引用了 `src_file:src_line`」；
本工具独立算「`(file,line)` → 函数」，检查该行是否落进 `x86_addr` 的 Location 集合。

```text
唯一锚 327 条：一致 324 / 不一致 1 / 起点缺失 2
```

**3 条例外全部查清，没有一条是「新方法算错」**：

| # | TSV 行 | 现象 | 结论 |
|---|---|---|---|
| 1 | `0x4eb86a math_function::remove_offset`（锚 `math_function.rs:52:43`） | **`0x4eb86a` 不是指令边界**：它落在 `0x4eb868 add eax,-3`（3 字节，`…868…86b`）的中间。Ghidra 在这造了个函数头。真正的 FDE 起点是 **`0x4eb830`**，且它确实引用 `math_function.rs:52:43` 与 `:53:43` | **TSV 地址要改成 `0x4eb830`**（`name_bridge.py` 的 Ghidra 边界口径问题，不是归属逻辑错） |
| 2 | `0x4eccb3 math_function::to_string_recursive`（锚 `math_function.rs:285:28`） | `0x4eccb3` 是指令边界，但落在 FDE 函数 **`0x4ec160`**（`to_string`）的体内 —— x86 把 `to_string_recursive` **合并/内联**进了调用者；该 Location 的引用指令 `0x4ece8f` ∈ `0x4ec160` | **地址应记 `0x4ec160`**（或注明「x86 无独立函数，已并入 `to_string`」） |
| 3 | `0x748000 math_function::mutate_function` | 该行 `join_kind` 写 `唯一锚`，但**证据栏自己写着「消去法唯一：无直接一对一锚」**；它展示的锚 `slice/index.rs:1020:51` 在本工具里被 **26 个函数**引用（8 个不同 Location 结构体共用同一行号），根本不唯一 | **标注问题**：这一行不该计入「唯一锚」的 327（地址本身可能仍对，但它不是靠锚定的名） |

⇒ 结论：**324/327 = 99.1% 一致**，且 3 条例外都能指到「函数边界口径」或「TSV 标注」，
没有一条指向「谁引用了这个 Location」这一跳。**这一跳现在可信。**

### 24.4 需要重核的旧结论（清单）

**（a）先看规模**：v1 的 `+0x1000` 平移让 **5019/5301（94.7%）** 个 Location 的函数归属
发生变化，只有 282 个因为「函数体 > 4096 字节」而碰巧落在同一个函数里。
⇒ **凡是引用过 `class_probe` 的 `funcs:` 列的地方，都要按本节重算**；
`literals` 列（字面量地址）**不受影响**（它没用 `off2va`）。

**（b）逐处判定**（`grep -rn class_probe docs/ src/ tools/` 的全部命中 + 受影响的 fixture）

| # | 位置 | 旧说法 | 本工具的新结果 | 判定 |
|---|---|---|---|---|
| 1 | `docs/02` §17.1、`docs/coverage-matrix.tsv` 第 56 行 | `graphwar_obstacle_manager.rs` 的锚函数 = `0x3c64c0 / 0x3f00b0 / 0x3f2480 / 0x3f27d0`（4 个） | **10 个**：`0x3c2bc0 0x3ee190 0x3f11e0 0x3f15b0 0x3f17a0 0x3f19a0 0x3f1b30 0x3f1d30 0x3f20f0 0x3f2360`（= `module_funcs.tsv` 本模块 10 行） | **冲突** |
| 1b | `docs/02` §17.1 的「⚠ 两处证据冲突 ⇒ 文件级对应**未定**」 | `0x4f15b0` 不在本模块锚函数里 | `0x4f15b0` 是 **Ghidra 口径**；ELF 虚地址 **`0x3f15b0` 就是**本模块锚函数之一（`module_funcs.tsv` 里也有它）。**「冲突」是地址空间混用 + v1 平移共同造成的，可以结案** | **可结案**（`混装（待拆）` 的档位由 `docs/02` 维护者定，本节只给证据） |
| 2 | `docs/02` §17.4（`math_function.rs` 行） | 「解析器 `0x4ed670` 的 panic 锚落 `math_function.rs:285:28`（`docs/03`、`class_probe`）」 | `285:28` 全库**唯一**引用指令是 `0x4ece8f` ⇒ 属 **`0x4ec160`**（`to_string`）。`0x4ed670` 自己引的是 `math_function.rs:75:10`（另有一条 regex 池 `350:17`）。v1 把 `0x4ece8f + 0x1000 = 0x4ede8f` 归给了 `0x4ed670` —— **这正是文档那句话的来源** | **冲突**（v1 artifact） |
| 3 | `docs/02` §17.4（`polish_notation_converter.rs` 行）、`docs/coverage-matrix.tsv` 第 94 行 | 「本模块的锚函数是 `0x4f57d0`（`:36:52` / `:116:55`）」 | **`0x4f5450`**（引用指令 `0x4f5789` / `0x4f577c`；`+0x1000` 后正好落到 `0x4f57d0`） | **冲突** |
| 4 | `docs/14`（`0x3dcbf4` 那一节） | 「`class_probe` 记的 Location 结构体锚点是 **233/234/235/236/239/244/250/256**（8 个）」 | 该函数是 **`0x3dcbb0`**，真实 Location 行 **18 个**：`275 278 292 307 310 345 350 351 352 353 354 363 364 365 527 530 535 544`。括号里那 8 个是 v1 归给它的 **17** 行里的一段 —— 逐条对得上，属 v1 artifact | **冲突** |
| 5 | `docs/03` §13.2（「⚠ 一处**工具**冲突」） | 「`97:63` 被 v1 归给 `0x4f57d0`；真正引用指令 `0x4f4974` 落在 `0x4f44c0` 体内（REA 核实）」 | 本工具：`graphwar_function_input.rs:97:63` → **`0x4f44c0`**。v1 会给 `0x4f57d0` | **一致**（当时手工纠对了，可删掉这处「工具冲突」） |
| 6 | `docs/16` §16.1.1 | 真因 = `off2va` 平移 `0x1000`；fixture 可信、`funcs:` 列不可信 | 完全相同 | **一致**（本条是正确诊断，§23.1 是回退） |
| 7 | `docs/16` §23.1 | 真因 = 「pkl 与 `.so` 不是同一次扫描」「pkl 已过期」 | **错**：pkl 与 `9a538115…` 逐条相符，真因是 `off2va` | **冲突（要改）** |
| 8 | `docs/16` §23.2 的地址表（`draw 0x4b5df0` / `ready 0x4b7a70` / 4 个 gdext 入口 …） | 全表 | 本工具逐条复核：**全部一致**（那张表是手算 + REA 得出的，不是 `class_probe` 输出） | **一致** |
| 9 | `src/graphwar_function_input.rs:172`、`src/graphwar_plane.rs:23`、`src/graphwar_setup_control.rs:32` | 「不要直接引用 `class_probe` 的归属」 | 结论对。但 `graphwar_plane.rs:23` 的**理由**（「pkl 与本版 `.so` 不是同一次构建」）要改成「v1 的 `off2va` 平移 `+0x1000`，已在 v2 修复」 | **结论一致 / 措辞需改** |
| 10 | `fixtures/module_funcs.tsv`（544 行 / 56 模块）、`docs/coverage-matrix.tsv` 的**模块行** | `module_map.py` 生成 | 本工具**逐条相同** | **一致**（不受影响） |
| 11 | `fixtures/name_to_addr.tsv`（327 唯一锚） | 地址来自 `name_bridge.py` + Ghidra 边界 | 324 一致；`0x4eb86a`→`0x4eb830`、`0x4eccb3`→`0x4ec160`；`0x748000` 行的 `join_kind` 与证据不符 | **2 条地址 + 1 条标注** |

**（c）13 个模块没有 fixture 兜底** —— 这些模块的函数清单**只可能来自 v1 或手工分析**，
是「最该先重核」的一批。根因：`tools/module_map.py` 写 `module_funcs.tsv` 时有一句
`if not module.startswith("graphwar_"): continue`，所以 12 个非 `graphwar_` 游戏模块
与 `weak_caller.rs` 从来没进过 fixture。本工具给出的正确清单（可用
`tools/loc_xrefs.py dump locs` + `awk` 复算）：

| 模块 | 锚函数数 | 备注 |
|---|---|---|
| `entity_drawer.rs` | **14** | `docs/16` §21 记的是 10 个（按 arm64 符号表 + REA 做的）；本工具多出 6 个地址（`0x4b1f60 0x4b20e0 0x4bbcc0 0x4bbee0 0x4bc090 0x4bc240`）在 §21 里**两种地址口径都没出现** ⇒ 清单可补，**但不判冲突**（§21 用名字而非地址记） |
| `player_showcase.rs` | **10** | 与 `docs/16` §22 列出的地址**完全一致** |
| `effect_drawer.rs` | **11** | `docs/16` §18 覆盖其中若干；6 个地址（`0x4e02b0 0x4e0430 0x4e7870 0x4e7a20 0x4e7bc0 0x4e7e10`）未出现 ⇒ 清单可补 |
| `math_function.rs` | **7** | 见上表第 2 行（`0x4eb830` / `0x4ec160` …） |
| `weak_caller.rs` | **40** | gdext 侧模块，非游戏源码 |
| `axis_drawer.rs` | 6 | 6 个地址（`0x4dcd70 0x4dcef0 0x4e70c0 0x4e7260 0x4e7470 0x4e76c0`）在 `docs/16` §18 里两种口径都未出现 ⇒ 清单可补 |
| `button_manager.rs` | 3 | |
| `function_button_manager.rs` | 2 | |
| `obstacle_drawer.rs` | 2 | |
| `function_drawer.rs` | 1 | |
| `function_history_recorder.rs` | 1 | |
| `polish_notation_converter.rs` | 1 | 见上表第 3 行 → **`0x4f5450`** |
| `server_broker_request_connection.rs` | 1 | |

> 「清单可补」= 本工具能给出完整地址表，但**没有**逐条做 REA 复核；本节只把它标成
> **观测（可复算）**，不替那些章节改结论。谁要写进 `docs/02` 的矩阵，请按
> `dump locs` 重算并保留本节的命令。

### 24.5 本轮同时修的第二个同类小坑（`classes` 列）

`classes` 子命令在「字面量 → 引用它的函数」这一跳也套了 `off2va()`
（`f = self.func_of(self.off2va(off[0]) or 0)`），同一个 `+0x1000`，而且 `or 0`
会把失败静默变成「归到 0 号地址」。v2 改成直接 `refs_to()` + `func_of()`。
`docs/16` §16.1.1 说的「`literals` 列不受影响」是对的，但 `classes` 列**受影响**，
本轮一并修好（`classes` 现在能给出 12 个类型名 × 正确函数集合）。

### 24.6 验证结果

| 命令 | 结果 |
|---|---|
| `../.venv/bin/python tools/loc_xrefs.py check-tsv` | 324/327 一致，3 条例外已逐条解释（27.3） |
| `../.venv/bin/python tools/class_probe.py module graphwar_plane` | `:68:1` → 4 个 gdext 入口、`:274:129` → `0x4b5df0`、`:218:129` → `0x4b7a70`（与 `docs/16` §23.2 一致）；行数 175→232（19 个 Location 不变，`funcs` 段 21→22，多出的是线索 A 修好后新增的字面量行） |
| `cargo test --offline` | **全绿**（本轮未改 `src/`） |
| `python3 tools/coverage_matrix.py --check` | **红**，但**与本轮改动无关**：master 上（干净工作树）同样是红的，且差异行号相同（`docs/coverage-matrix.tsv` 第 10 行、`docs/149` 第 247 行）。实测差异**只有表头 2 行**：`src_files: 46` vs 重算 `47`、`src_tree_sha256` 变化 —— 上一个提交 `c7107934` 新增了 `src/graphwar_module_container.rs` 却没重跑生成器；**96 行模块行逐行相同**。修它要重生成 `docs/coverage-matrix.tsv` + `docs/149-coverage-matrix.md`，不在本轮可改文件范围内 |

## 25. `graphwar_nodes.rs` 规格（本轮新增：从"零锚点未知模块"到 11 个归一化函数 + 40 个节点名）

### 25.0 一页结论

* 这个模块**不造节点**。它是全客户端**唯一的"按名字/按类型找节点"入口**：
  调用方给一个节点名字符串，它从**场景根**开始递归找，找到后**上抛成 `Gd<T>`**。
  `docs/02` §16 当年把它列进"13 个零锚点未知模块"，现在名字与结构都有了。
* **11 个可归属函数 = 同一泛型函数 `get_node_by_name::<T>` 的 11 个单态化。**
  依据是 arm64 2.0.60 `.symtab`：`graphwarrust::graphwar_nodes::GraphwarNodes::
  get_node_by_name::h<hash>` 正好 **11** 条（204/228 字节），`get_node_by_name_rec`
  17 条（772–816 字节），`get_node_by_type_rec` 1 条（636 字节），合计 **29**。
* 三个入口的分工由**具名被调者**直接读出（不是猜）：

| 入口 | 返回 | 具名被调者（arm64 `bl`，逐条解出） | 档位 |
|---|---|---|---|
| `get_node_by_name::<T>(名字)` | `Gd<T>` | `Node::get_tree` → `SceneTree::get_root` → `Gd<T>::owned_cast` → `get_node_by_name_rec::<T>` | 观测 |
| `get_node_by_name_rec::<T>(节点, 名字)` | `Gd<T>` | `Node::get_children_full`、`AnyArray::len`、`Node::get_name`、`StringName` 的 `Display::fmt`、**自递归**、`Gd<T>::owned_cast` | 观测 |
| `get_node_by_type_rec::<T>(节点)` | `Vec<Gd<T>>` | 同上**但换成** `RawVec::grow_one`（无 `get_name`） ⇒ 按**类型**收集、返回**多个** | 观测 |

⇒ `get_node_by_name` 只是**薄壳**（先 `get_tree().get_root()`），遍历孩子的是 `_rec`。

### 25.1 分母：11 怎么来的（可复现，三条判据串起来）

1. `src/graphwar_nodes.rs` 在整份 `.so` 里**只有一个** `panic::Location`：
   `.data.rel.ro` 的 `0x951678` = `{file_len 21, line 8, col 62}`，`file` 指针的
   `R_X86_64_RELATIVE` addend 指向字面量 `src/graphwar_nodes.rs`。
2. 引用它的函数有 **12** 个（`../.venv/bin/python tools/loc_xrefs.py loc src/graphwar_nodes.rs:8:62`）：
   `0x3a2e10` 与 `0x3aaea0, 0x3aafa0, 0x3ab0a0, 0x3ab190, 0x3ab290, 0x3ab390, 0x3ab490,
   0x3ab590, 0x3ab690, 0x3ab790, 0x3ab890`。
3. `0x3a2e10`（`0x26d0` 字节，= `GraphwarRootNode::_ready`）**同时**引用别的模块的 Location，
   按 `coverage_matrix.py` 的 contested 规则记给别的模块 ⇒ **扣掉**，剩 **11**。
   这与 `docs/coverage-matrix.tsv` 里 `graphwar_nodes.rs` 的 `orig_funcs = 11`
   （`contested = 0`）**同一个分母**。

剩下 11 个的大小是 `0xf0`/`0x100`，首尾相接；与 arm64 的 11 个 228 字节单态化一一对应。

⚠️ **档位**：第 1–2 步是**观测**；"11 个 x86 函数 ⇄ 11 个 arm64 单态化"这一步是**推断**——
缺的是"某个 x86 函数 ↔ 某个 `::h<hash>`"的一对一锚（本模块的 Location 只有一个，
天然做不到）。三条旁证：同一 Location、同形（都是 `upcast_ref` + 一个类型专属尾巴）、同数量。

### 25.2 11 个函数体长什么样（x86 逐指令）

每个都是 ~0x100 字节的同一个骨架：

```text
push r15/r14/r13/r12/rbx; sub $0x50,%rsp
%rbx=%r8, %r15=%rcx, %r14=%rdi            ; 实参：out / 句柄两半 / 名字两半
[%rsp+0x20]=%rsi; [%rsp+0x28]=%rdx        ; 把句柄两半钉到栈上
%r12 = lea 0x1bd8f2                       ; "upcast_ref"（10 字节）
call 0x4068f0(&pair, "upcast_ref", 10)    ; ← gdext 的 Gd 操作胶水
call *0x9ba7d0(&name)                     ; 引擎槽
call 0x405a60(&pair, "upcast_ref", 10)
call *0x9ba7d8(&pair)                     ; → Result，判别值 1 = 成功
if disc == 1: call <类型专属尾巴>          ; 0x3ad570 / 0x3ab990 / 0x3ad220 / …
else:         panic(0x951678)             ; = src/graphwar_nodes.rs:8:62
```

* 11 个函数体**唯一的** `Gd` 操作是 `upcast_ref`；panic 字面量是
  `0x1bd8fc`（`%esi = 0x2f = 47` = "Upcast failed. This is a bug; please report it."）。
* 11 个**类型专属尾巴**互不相同（`0x3ad570, 0x3ab990, 0x3ad220, 0x3acb00, 0x3ac770,
  0x3af0a0, 0x3ac3e0, 0x3ae650, 0x3ace90, 0x3ac050, 0x3adc50`）⇒ 11 个不同的 `T`。
* **跨模块观测（本轮顺带解出，29 个模块共用）**：`0x4068f0` / `0x4066c0` 这两个包装
  全 `.text` 里的调用点分别是 181 / 295 个，而它们收到的**名字立即数只有四个**：

| 操作名 | 长度 | `0x4068f0` 次数 | `0x4066c0` 次数 |
|---|---|---|---|
| `clone` | 5 | 93 | 121 |
| `upcast_ref` | 10 | 58 | 117 |
| `cast` | 4 | 18 | 8 |
| `try_with_ref_counted` | 20 | 1 | 1 |

  这四个正是 **godot-rust(gdext) 里 `Gd<T>` 的内建操作** ⇒ 这两个包装是 gdext 的
  `Gd` 绑定胶水，**不是**游戏业务代码。以后任何模块看到 `call 0x4068f0/0x4066c0`
  都可以直接判成"一次 `Gd` 的 clone/cast/upcast"，不必再逐个反编译。
  落码：`src/graphwar_nodes.rs` 的 `GdOp` / `GD_OP_WRAPPER_A` / `GD_OP_WRAPPER_B`。

### 25.3 它按什么键索引：40 个节点名（**这一节最实用**）

判据 = 每个调用点前的 `lea rcx,[rip+X]` + 紧随的 `mov r8d,<len>`；共 **80** 个调用点。
长度是 `.rodata` 字面量的真实字节数，可逐条复核。

| 名字 | len | 名字 | len | 名字 | len |
|---|---|---|---|---|---|
| `AxisModeButton` | 14 | `HatsVbox` | 8 | `PlayerNameLineEdit` | 18 |
| `ChatBox` | 7 | `JoinIpIpLineEdit` | 16 | `RightButtonGrid` | 15 |
| `ChatButtonPlay` | 14 | `LeftButtonGrid` | 14 | `RightReplaysButtonsGrid` | 23 |
| `ChatButtonSetup` | 15 | `LeftReplaysButtonsGrid` | 22 | `RoomNameLineEdit` | 16 |
| `ChatLineEdit` | 12 | `LockButton` | 10 | `ShowGlobalButton` | 16 |
| `ChatPopUp` | 9 | `MuteButton` | 10 | `ShowLocalButton` | 15 |
| `DisconnectedLabel` | 17 | `OffsetLabel` | 11 | `ShowNames` | 9 |
| `FacesVbox` | 9 | `PendingBlock` | 12 | `SkinBlock` | 9 |
| `FireContainer` | 13 | `PlayPauseButton` | 15 | `SkinsVbox` | 9 |
| `FunctionModeButton` | 18 | `PlayerListContainer` | 19 | `Team1Container` | 14 |
| `FxTextureButton` | 15 | `TimerLabel` | 10 | `TurnModeButton` | 14 |
| `GameIpLabel` | 11 | `TurnTimeButton` | 14 | `ViewFunctionButton` | 18 |
| `GraphwarLanVBox` | 15 | `ViewFunctionsLabel` | 18 | `YButton` | 7 |
| `GraphwarRootNode` | 16 | | | | |

交叉验证：其中 `RoomNameLineEdit` / `AxisModeButton` / `FunctionModeButton` /
`TurnModeButton` / `TurnTimeButton` / `LockButton` / `ChatButtonSetup` / `ChatButtonPlay` /
`ChatPopUp` / `ChatBox` / `ChatLineEdit` 这 **11** 条，与 §16.2.1 早已逐条量过的
`.rodata` 地址**完全一致**（两条独立路径）。

**名字 ↔ 单态化（已配对的部分）**：

| 单态化 | 取的名字（调用顺序） |
|---|---|
| `0x3aaea0` | `ShowLocalButton`, `ShowGlobalButton`, `AxisModeButton`, `FunctionModeButton`, `TurnModeButton`, `TurnTimeButton`, `LockButton`, `ChatButtonSetup`, `ChatButtonPlay` |
| `0x3aafa0` | `Team1Container`, `SkinsVbox`, `FacesVbox`, `HatsVbox` |
| `0x3ab190` | `LeftButtonGrid`, `RightButtonGrid`, `FireContainer`, `LeftReplaysButtonsGrid`, `RightReplaysButtonsGrid`, `SkinBlock`, `PendingBlock` |
| `0x3ab290` | `TimerLabel` |
| `0x3ab390` | `ViewFunctionButton`, `PlayPauseButton`, `YButton`, `ShowNames`, `MuteButton` |
| `0x3ab490` | `JoinIpIpLineEdit`, `RoomNameLineEdit`, `ChatLineEdit`, `PlayerNameLineEdit` |
| `0x3ab590` | `GraphwarRootNode`（24 个调用点里 15 个点名它 —— 全客户端最热的取节点操作） |
| `0x3ab690` | `GraphwarLanVBox` |
| `0x3ab790` | `ViewFunctionsLabel` |
| `0x3ab890` | `PlayerListContainer` |
| `0x3ab0a0` | **未读出**（3 个调用点全在 `GraphwarRootNode` 的命令总线 `0x3b0830` 里，配对窗口被挤掉） |

### 25.4 谁在用它（调用方全表，**观测**）

| 单态化 | 直接调用点 | 调用方（去重） |
|---|---|---|
| `0x3aaea0` | 9 | `0x3ef990`(lan_vbox), `0x4cf670`(setup_control `_ready`) |
| `0x3aafa0` | 7 | `0x4d3180`, **`0x4f57d0`(player_screen `load_items`)**, **`0x4f8050`(player_screen `update_buttons_background`)** |
| `0x3ab0a0` | 3 | `0x3b0830`(root_node 命令总线) |
| `0x3ab190` | 17 | `0x3d9b50`/`0x3d9e40`(play_screen 视图切换 ×5 各), `0x4cf670`, **`0x4f8850`/`0x4f8990`/`0x4f8ad0`(player_screen `lock`/`unlock`/`pending`)** |
| `0x3ab290` | 4 | `0x3a5760`, `0x4d3180`, `0x4f2200`, `0x522070` |
| `0x3ab390` | 7 | `0x418820`, `0x4f2200`, `0x522070`(×4), `0x5238c0` |
| `0x3ab490` | 5 | `0x3b0830`, `0x4cf670`(×2), `0x4e6480`(player_showcase `ready` ×2) |
| `0x3ab590` | 24 | `0x4b2890`/`0x4b4130`/`0x4b4b60`/`0x4b5df0`/`0x4b7a70`/`0x4b83f0`(entity_drawer ×6), `0x4c27f0`/`0x4c44b0`/`0x4c4850`/`0x4c4c20`(setup_control), `0x4cf670`, `0x4d3180`, `0x4e1520`/`0x4e25b0`/`0x4e2f80`, `0x4e5400`/`0x4e6480`/`0x4e68f0`/`0x4efd70`(player_showcase), `0x4f2200`, **`0x4f7940`(player_screen `skin_button_pressed`)**, `0x4f90b0`(lan_vbox `process` ×2), `0x4fd4a0` |
| `0x3ab690` | 1 | `0x3ef990` |
| `0x3ab790` | 2 | `0x3dc7f0`(play_screen 订阅), `0x4cf670` |
| `0x3ab890` | 1 | `0x4d3180` |

⇒ 客户端**每一个**界面控制器都从这一个模块拿节点。这解释了为什么它的 11 个函数
个个都带 panic 锚：`get_node_by_name(...).unwrap()` 是**唯一**会失败的取节点方式
（找不到名字就 panic），而 `get_node_by_name_rec` / `get_node_by_type_rec` 返回
`Option` / `Vec`，不 panic。

### 25.5 落码清单（`src/graphwar_nodes.rs`）

| 项 | 内容 | 档位 |
|---|---|---|
| `PANIC_ANCHOR` / `PANIC_LOCATION_SLOT` | `8:62` / `0x951678` | 观测 |
| `GET_NODE_BY_NAME` / `TYPE_TAILS` / `ACCESSORS` | 11 个入口 + 11 个类型尾巴 | 观测 |
| `GdOp` / `GD_OP_WRAPPER_A` / `GD_OP_WRAPPER_B` | gdext `Gd` 的四个操作名 + 两个包装地址 | 观测（穷举 476 个调用点） |
| `NODE_NAMES` | 40 个名字 + 长度 | 观测 |
| `NAMES_BY_ACCESSOR` / `CALLERS` | 名字↔单态化、单态化↔调用方 | 观测 |
| `ARM64_*` | 29 个 arm64 符号的拆分 | 观测 |
| `unknowns()` | 6 条 | — |

**本节没解出来的（照实写）**：

1. **11 个 `T` 分别是什么类型** —— x86 侧只有 11 个互不相同的类型尾巴，没有类型名字符串；
   arm64 符号只有 `::h<hash>`。缺的证据：x86 尾巴体内 `check_rtti` 的 `ClassId` 立即数
   （要先把 `godot_core::meta::class_id` 那张表解出来，目前没人做过）。
   唯一旁证是**用法**（`0x3aafa0` 的实参全是 `*Vbox`/容器、`0x3ab190` 全是 `*Grid`/`*Block`、
   `0x3ab590` 全是 `GraphwarRootNode`）——这是**推断**，不是结论。
2. **11 个 x86 函数对上 `get_node_by_name` 还是 `_rec`** —— 只有"11 == 11"这一条计数论证。
3. **5 个实参的名字已读出、所属单态化未配对**（`lea rcx` 与 `mov r8d` 之间隔了别的 `lea`）。
4. **节点名比较的确切语义** —— `get_node_by_name_rec` 里 `Node::get_name` 之后走
   `StringName` 的 `Display::fmt` 再比较；是 `==` 还是 `format!` 之后比较**未知**
   （`0x44ccc0` / `0x844d00` 两个被调者没有符号）。
5. **`0x9bb310` / `0x9bb318`（`0x4068f0` 内部两个槽）的引擎侧成员名** —— 需要运行时读 GOT。
6. **`0x3ab0a0` 的 3 个实参名字**。

### 25.6 复现（不依赖本节结论，任何人可重跑）

```text
# ① 本模块的 Location 只有一条
../.venv/bin/python tools/loc_xrefs.py loc src/graphwar_nodes.rs:8:62

# ② arm64 的 29 个符号
../.venv/bin/python tools/symtab_dump.py methods graphwar_nodes

# ③ 11 个薄壳体里的名字 + 被调者（x86）
objdump -d --start-address=0x3aaea0 --stop-address=0x3ab990 \
        ../libgraphwarrust.so | grep -E 'lea|call'
#   应看到每条都是 0x1bd8f2("upcast_ref") + 0x951678(panic 锚) + 一个不同的 call 尾巴

# ④ 40 个节点名（本轮的判据）
#    对 11 个入口各自的 call 点，回看 lea rcx,[rip+X] 与紧邻的 mov r8d,<len>
```

## 26. `graphwar_player_screen_control.rs` 规格（本轮新增：8 个函数全定位；**它就是写 `GraphwarPlayerConfig` 的地方**）

### 26.0 一页结论

1. **回答了本轮的核心问题：是。** 玩家页三个外观按钮的回调分别调
   `GraphwarPlayerConfig::set_skin` / `set_face` / `set_hat` —— 这条在 arm64 `.symtab`
   里是**具名的 `bl`**，三对一一对应，**不是**"从行为猜出来的"。
2. `lock()` / `pending()` / `unlock()`**不动档案**，只改两个块的可见性。
3. `load_items()` 是**造按钮**的那个函数（`PackedScene::instantiate_as` +
   `Node::add_child_full` + `Button::set_button_icon`），并把
   `load_skins` / `load_faces` / `load_hats` **全部内联**进来。
4. 顺带结清一条旧"未知"：**`0x9ba978` = `CanvasItem::set_visible`**
   （`docs/14-turn.md` §9.3 与本文 §15.9 曾登记为"需要运行时读 GOT"）。
5. `docs/coverage-matrix.tsv` 里这一格的 `status` 本轮仍写 `未知`/`未做` ——
   因为那张表的 `M` 字典在 `tools/coverage_matrix.py` 里手写，
   改它不在本轮可改文件范围内（见 §26.7）。

### 26.1 分母：8 个函数 + 22 个锚（`fixtures/name_to_addr.tsv` 的 8 条 `唯一锚`）

| # | 函数 | x86 vaddr | size | 锚（`src/graphwar_player_screen_control.rs:line:col`） |
|---|---|---|---|---|
| 1 | `load_items` | `0x4f57d0` | `0x1fa0` | `113:39` + 8 条内联锚 |
| 2 | `hat_button_pressed` | `0x4f77a0` | `0xd0` | `131:28` |
| 3 | `face_button_pressed` | `0x4f7870` | `0xd0` | `127:28` |
| 4 | `skin_button_pressed` | `0x4f7940` | `0x270` | `118:28`、`120:118`、`123:68` |
| 5 | `update_buttons_background` | `0x4f8050` | `0x2e0` | `230:113`、`233:111` |
| 6 | `lock` | `0x4f8850` | `0x140` | `252:107`、`255:113` |
| 7 | `unlock` | `0x4f8990` | `0x140` | `268:107`、`271:113` |
| 8 | `pending` | `0x4f8ad0` | `0x140` | `260:107`、`263:113` |

22 个 `Location` 覆盖的源码行列（观测，`tools/loc_xrefs.py` 从 `.rela.dyn` 的 addend 解出）：

```text
113:39  118:28 120:118 123:68  127:28  131:28
135:103 139:74  156:103 159:74 165:64
183:102 186:73  192:64
230:113 233:111
252:107 255:113   260:107 263:113   268:107 271:113
```

三组"借用 / 解包 / 借用计数溢出"三元组同形：`118:28/120:118/123:68`（`skin_button_pressed`）、
`252:107/255:113`（`lock`）、`268:107/271:113`（`unlock`）、`260:107/263:113`（`pending`）。
`load_items` 占 9 条（自己 `113` + 三个被内联 loader 各 2 条）。

### 26.2 三个按钮改档案（**本节最强的一条证据**，两条独立路径）

| 回调 | x86 | arm64 具名被调者 | 改哪个字段（偏移） | x86 setter（槽的 addend） |
|---|---|---|---|---|
| `skin_button_pressed` | `0x4f7940` | `GraphwarPlayerConfig::set_skin` | 皮肤 `+0x60`（默认 `yellow`） | 槽 `0x9bb6f0` → **`0x417e20`** |
| `face_button_pressed` | `0x4f7870` | `GraphwarPlayerConfig::set_face` | 脸 `+0x78`（默认 `regular_eyes`） | 槽 `0x9bb6e8` → **`0x417d50`** |
| `hat_button_pressed` | `0x4f77a0` | `GraphwarPlayerConfig::set_hat` | 帽子 `+0x90`（默认 `soldier_helmet`） | 槽 `0x9bb6e0` → **`0x417c70`** |

**x86 侧的独立确认**（三个 setter 的体内逐指令）：

```text
0x417c70  cmp byte ptr [rdi+0xac], 0     ; 解锁标记（=1 才同时更新"原始值"）
          ... 把新 String 拷到 [rbx+0x48]  ; "原始"帽子
          ... 把新 String 拷到 [rbx+0x90]  ; 帽子（当前）
          inc dword ptr [rbx+0xa8]         ; 变更计数器 +1
```

`+0x48` / `+0x90` / `+0xa8` / `+0xac` 与 §22.9 的 `GraphwarPlayerConfig` 布局表
**逐字段吻合**（那一节是从 `GraphwarPlayerConfig::lock` 与 `draw` 反推的，
本节是从三个 setter 反推的 —— 两条独立路径给出同一张表）。
`set_face`（`0x417d50`）写 `+0x78`，`set_skin`（`0x417e20`）写 `+0x60`，同形。

⚠️ **推断**（不影响上面那条结论）：x86 侧这三个 setter 是**经 `.data.rel.ro` 槽间接调用**的
（`call *[slot]`），槽被 `R_X86_64_RELATIVE` 静态写入，addend 就是入口 —— 所以语义确定；
但"为什么没内联"（`#[godot_api]` 方法表？`OnceLock` 初始化？）**未知**。

### 26.3 每个函数的具名被调者（arm64 `.symtab`，逐条解出）

godot/gdext/alloc/panic 那些基础被调者已滤掉，只留说明业务的：

| 函数 | 游戏内具名被调者 |
|---|---|
| `load_items` | `GraphwarNodes::get_node_by_name`(×3), `GraphwarNodes::get_node_by_name_rec`(×2), `GraphwarPlayerScreenControl::insert_button_in_vbox`(×3), `GraphwarTextureLoader::get_skin_texture` |
| `skin_button_pressed` | `GraphwarNodes::get_node_by_name`, **`GraphwarPlayerConfig::set_skin`**, `GraphwarPlayerScreenControl::update_buttons_background` |
| `face_button_pressed` | **`GraphwarPlayerConfig::set_face`** |
| `hat_button_pressed` | **`GraphwarPlayerConfig::set_hat`** |
| `update_buttons_background` | `GraphwarNodes::get_node_by_name`(×2), `GraphwarPlayerScreenControl::update_buttons_background_in_vbox`(×2), `GraphwarTextureLoader::get_skin_texture` |
| `lock` / `unlock` / `pending` | `GraphwarNodes::get_node_by_name`(×2), `CanvasItem::set_visible`(×2) |

`load_items` 另外用到（gdext 侧）：`PackedScene::instantiate_as`(×2)、
`godot_core::tools::save_load::load`(×2)、`Button::set_button_icon`、`Button::set_expand_icon`、
`Control::set_mouse_filter`、`Control::set_force_pass_scroll_events`、`GString::from(&str)`、
`Node::add_child_full` ⇒ **它是"造按钮"的那一个**（按钮 = 现成场景实例化的，不是代码拼的）。

`insert_button_in_vbox` 的被调者（`Node::add_child_full`、`Node::get_children_full`、
`Array::len`、`Gd::owned_cast`、`construct_engine_object`、`FromGodot::from_variant`）
⇒ 它负责"把新按钮插进 VBox 的合适位置"。

### 26.4 `lock` / `pending` / `unlock`：只有可见性（**观测**，逐指令）

| 函数 | `SkinBlock` | `PendingBlock` | `%esi` 来源 |
|---|---|---|---|
| `lock` `0x4f8850` | `1` | `0` | `0x4f88da` / `0x4f8955`（`xor`） |
| `pending` `0x4f8ad0` | `0`（`xor`） | `1` | `0x4f8b62` / `0x4f8bdd` |
| `unlock` `0x4f8990` | `0`（`xor`） | `0`（`xor`） | `0x4f8a22` / `0x4f8a9d` |

三个函数都是"取 `SkinBlock` → `upcast_ref` → `call *0x9ba978(node, bool)`，
再取 `PendingBlock` → `upcast_ref` → `call *0x9ba978(node, bool)`"，
**没有任何**对 `+0x16a8`（档案）的读写 ⇒ 它们不改进度，只改界面。

⚠️ 注意**同名不同物**：`GraphwarPlayerConfig::lock()`（档案模块）会写默认外观
（`yellow` / `regular_eyes` / `soldier_helmet`，§22.9），而
`GraphwarPlayerScreenControl::lock()`（本节）只翻可见性。两个 `lock` 别混。

**就地更正一条旧"未知"**：§15.9 / `docs/14-turn.md` §9.3 把 `0x9ba978`、`0x9baa10`
登记为"引擎侧成员名未知，需要运行时读 GOT"。本轮**不需要运行时**：

* `0x9ba978` 的 `R_X86_64_RELATIVE` addend = **`0x8ab200`**（gdext 生成的方法包装，
  与 `draw_arc` 的包装 `0x8abca0` 同一段）；
* 本模块这三个函数是**唯一**一类"取 2 个节点 + 各调一次 `0x9ba978` + 只传 bool"的函数，
  同一批函数在 arm64 侧具名 `bl` 全部是 `CanvasItem::set_visible`（×2，逐条解出）
  ⇒ **`0x9ba978` = `CanvasItem::set_visible` 的 GOT 槽**。

档位：地址/包装 = **观测**；"槽 ↔ 成员名"= **推断（强）**，缺的证据是 `0x8ab200` 体内
那条 `object_method_bind_ptrcall` 的 method bind 名字。
（`0x9baa10` 是**另一个**槽，本节没碰 —— **2026-11 解出（`docs/151` §2.4）**：它是
`R_X86_64_GLOB_DAT` 的 **`bcmp@GLIBC_2.2.5`**（libc 的 `memcmp`），**根本不是 gdext 成员**，
所以"可见性 setter 候选"这个方向从一开始就是错的。）

⚠️ **本轮对 `0x9ba978` 的独立复核，结论是"没有加强也没有推翻"**：`readelf -r` 读出
addend = `0x8ab200`（与本节一致），但 `0x8ab200` 体内**没有** `set_visible` 字面量 ——
它引用的是 `validated_objectarray_to_variant`（`0x19d710`）。`.rodata` 里**确实**有
`set_visible`（`0x29f00c`，紧邻 `visible` / `set_modulate`），但**没有任何指令直接引用它**
（`tools/addr_probe.py callers 0x29f00c` = 0）：名字表与调用点之间还隔着一层初始化时按名取指针的循环。
⇒ 本节"`set_visible`"仍然只靠 arm64 侧 `bl` 那一条证据，挡位维持**推断（强）**。

### 26.5 结构体：`+0x00` 是档案，`+0x10/+0x18` 是查找起点

8 个函数的第一个实参 `%rdi` 指同一个结构体，两个字段区被用到（**偏移观测 / 字段名推断**）：

| 偏移 | 内容 | 判据 |
|---|---|---|
| `+0x00` | `Rc<RefCell<GraphwarPlayerConfig>>` | `mov %rbp,(%rdi)` → `[%rbp+0x10]` 借用计数检查（`cmp …,0` + `jne panic_already_borrowed`）、`[%rbp+0x18]` = 载荷；三个 `set_*` 与 `lock`/`pending` 都直接对它操作 |
| `+0x10` / `+0x18` | 一个**两字宽**句柄，两半各自做非零检查，作为 `get_node_by_name` 的查找起点 | `lock`/`unlock`/`pending` 只读这两处；`skin_button_pressed` 用它找 `GraphwarRootNode` |

与 §22.9 **不冲突**：根节点 `+0x16a8` 持有同一个 `Rc` 的一份克隆，本模块把这个克隆
**缓存在自己身上**（`+0x00`），所以"写档案"这条路径不必每次都从根节点取。

`skin_button_pressed` 里还有一条值得记的：它**同时也读** `GraphwarRootNode + 0x16a0`
（`0x4f7a60: mov %rcx,(%rdx+0x16a0)` + `0x4f7a70: mov %r14,(%rdx+0x16a0)`）——
即"改完外观还要拿贴图缓存去刷按钮底色"，与 §22.9 那张表里
"`+0x16a0` 的读者包含 `GraphwarPlayerScreenControl::skin_button_pressed`"一致。

### 26.6 `load_items`：三个 loader 已被内联

arm64 `.symtab` 里 `GraphwarPlayerScreenControl` 共 14 个符号，其中
`load_skins` / `load_faces` / `load_hats` **只剩** `::{{closure}}`（各 52 字节）：

```text
0x0044de9c  52  GraphwarPlayerScreenControl::load_faces::{{closure}}
0x0044f5c8  52  GraphwarPlayerScreenControl::load_skins::{{closure}}
0x00450888  52  GraphwarPlayerScreenControl::load_hats::{{closure}}
```

外层函数体已经不在符号表里 ⇒ 被内联进 `load_items`（`0x44ded0`，5880 字节）。
这正面解释了 `fixtures/name_to_addr.tsv` 为什么给它们标
"**未知** — 该函数不引用任何 panic::Location"：**它们已经没有独立函数体了**，
不是"没解出来"。

按内联残留的锚点，三个块大致落在 `135` / `156` / `183` 行（各带一条 `+3~4` 行的第二个锚），
`165` / `192` 两条是 `load_items` 自己的。
⚠️ **推断**：行号 `135 < 156 < 183` 与 `SkinsVbox` / `FacesVbox` / `HatsVbox` 的**先后**
还需要逐段对齐才能定死（本轮只做到"三个块各占一段、且紧邻对应的 `get_node_by_name`"）。

### 26.7 落码清单（`src/graphwar_player_screen_control.rs`）

| 项 | 内容 | 档位 |
|---|---|---|
| `ATTRIBUTED` / `FN_*` | 8 个函数 + 地址 + 大小 | 观测 |
| `ANCHOR_LINES` | 22 个 `(line, col)` | 观测 |
| `ConfigField` / `BUTTON_ACTIONS` | 三个回调 → 三个字段 + 三个 setter + 三个槽 | 观测（setter **经槽间接调用**这一点是推断） |
| `BlockState` / `BLOCKS` | `lock`/`pending`/`unlock` 的两张可见性表 | 观测 |
| `SET_VISIBLE_SLOT` / `SET_VISIBLE_WRAPPER` | `0x9ba978` / `0x8ab200` | 观测（地址）/ 推断（成员名） |
| `NODE_NAMES` / `NODES_BY_FUNC` / `NODE_ACCESSORS_USED` | 本模块用到的 6 个名字 + 3 个 `graphwar_nodes` 单态化 | 观测 |
| `INLINED_LOADERS` / `UNANCHORED_ARM64` | 3 个 closure + 3 个 arm64 符号 | 观测 |
| `GAME_CALLEES` / `LOAD_ITEMS_GODOT_CALLS` | 具名被调者 | 观测 |
| `unknowns()` | 6 条 | — |

**本节没解出来的（照实写）**：

1. **`insert_button_in_vbox` / `update_buttons_background_in_vbox` 的 x86 地址** ——
   arm64 有符号（1012 / 1180 字节），但 2.0.60 x86 侧没有一对一 `Location` 锚，
   `fixtures` 只给到 `歧义`，而且**候选里混着 `graphwar_nodes` 的单态化**
   （那些函数也引用同一个 `src/graphwar_nodes.rs:8:62`）。**不替人选**。
2. **`+0x10/+0x18` 两字宽句柄的确切类型** —— 只知道两半各自做非零检查、被当作查找起点。
   `Gd<T>` 在 gdext 里是 1 字宽，所以 2 字宽**不是**单个 `Gd`；
   缺的证据：`.text` 里找不到唯一写点（可能是 `Base<Node>` + 一个 `InstanceId` 之类的组合）。
3. **三个 setter 为什么走间接调用**（语义不受影响，见 §26.2 的注）。
4. **`0x9bb7c0`（`skin_button_pressed` 里那条间接调用，addend `0x8cac40`）是什么** ——
   它在 `RawGd::bind` 之后被调用、返回 3 个 `dword` 判别值之一
   （`cmp dword ptr [rsp+0x48], 3`），最可能是 `GraphwarTextureLoader` 的一个方法，**未证**。
5. **改完外观谁来 `save()`** —— `GraphwarPlayerConfig::save()` 在
   `player_showcase::on_text_change` 里被直接调用（§22.6），但本模块三个 setter 之后
   **没有** `save` 调用 ⇒ 改外观不立即落盘，落盘靠别处（可能在切屏/退出时）。
   缺的证据：没找到那个唯一时机。
6. **§26.6 的行号与三个 Vbox 的先后**。

### 26.8 复现

```text
# ① 8 条唯一锚（名字 ↔ x86 地址）
awk -F'\t' '$1=="graphwar_player_screen_control" && $7=="唯一锚"' fixtures/name_to_addr.tsv

# ② 22 个 Location 与它们的引用点/归属函数
../.venv/bin/python tools/loc_xrefs.py loc src/graphwar_player_screen_control.rs:118:28
../.venv/bin/python tools/loc_xrefs.py func 0x4f7940

# ③ 三个 setter 的槽 addend（x86）
#    R_X86_64_RELATIVE @ 0x9bb6f0 = 0x417e20, @ 0x9bb6e8 = 0x417d50, @ 0x9bb6e0 = 0x417c70
objdump -d --start-address=0x417c70 --stop-address=0x417e20 ../libgraphwarrust.so
#    应看到 cmp byte [rdi+0xac] / 拷 String 到 +0x48 与 +0x90 / inc dword [rbx+0xa8]

# ④ arm64 侧的具名被调者（本条结论的真正来源）
../.venv/bin/python tools/symtab_dump.py methods graphwar_player_screen_control
#    再用 arm64 反汇编查每个符号的 bl 目标（本轮脚本：解析 .symtab + capstone ARM64）
```

### 26.9 与 `client/` 仿制实现的出入（**登记，不改 `client/`**）

按 `AGENTS.md` §0 第 4 条，这里只登记差异：

| # | `client/src/screens/player_screen.gd` | 原版（本节证据） | 影响 |
|---|---|---|---|
| 1 | 皮肤/脸/帽子点选只改 GDScript 里的本地变量 | Rust 侧 `GraphwarPlayerConfig::set_skin/set_face/set_hat` 直接写档案（`+0x60/+0x78/+0x90`）并 `+0xa8` 变更计数 | 仿制品**没有档案**，所以"关掉游戏再进来外观丢了"这类问题在它身上不存在，也就测不出来 |
| 2 | `SkinBlock` / `PendingBlock` 在桌面端恒 `visible = false` | 原版由 `lock()`/`pending()`/`unlock()` **按内购状态**切换（三张表见 §26.4） | 仿制品只做了"桌面端不显示"这一个特例 |
| 3 | 按钮图标/底色在 GDScript 里直接设 | 原版走 `GraphwarTextureLoader::get_skin_texture` + `update_buttons_background`（§26.3） | 仿制品没有贴图缓存这一层 |

---

## 27. `graphwar_audio_control.rs` 规格（本轮新增：**音效 ↔ 游戏事件**已解；`docs/111` §5.1 的未知项结案）

> 方法的取舍：`docs/111` 已经把"资源清单 / 损坏判定 / 播放 API / 音量常量"做实，
> 唯一没做的是 **§5.1「每个音效对应哪个游戏事件」**（当时标注"缺 `0x4bfe10` 内 match 臂的解码"）。
> 本轮用**未 strip 的 arm64 2.0.60** 的 `.symtab`（源码级函数名）+ x86 侧 `.eh_frame` 函数边界
> + 跨架构尺寸比闸（0.61–1.19）把 9 个函数全部定名，并把那条 match 解干净。
> 代码落在 `src/graphwar_audio_control.rs`，验收在 `tests/graphwar_audio_control.rs`。

### 27.1 一页结论

| # | 结论 | 等级 |
|---|---|---|
| 1 | 模块订阅**四条**事件：`effect_info_event`(44) / `soldier_info_event`(15) / `entity_removed_event`(14) / `game_state_info_event`(30) | **逐位** |
| 2 | 两条独立链互校：`new` 里 4 个 `Box::new(8)` + 4 个 `dyn FnMut` vtable 的 `call_mut` 各自 `adr` 到处理方法 | **逐位** |
| 3 | 三个处理方法按"资源槽"选音，槽 = `self+0x80/+0x90/+0xa0/+0xb0/+0xc0` | **逐位** |
| 4 | 槽 ↔ 文件 = **构造顺序**：explosion / squash / spark / pop / unpop | **逐位** |
| 5 | `effect_info_event` 按 payload `+0x20` 的 u8 分四臂落三槽 | **逐位** |
| 6 | 音量：三个特效槽 `-15.0f`、`pop` 是 `0.0f`、`unpop` 是 `-5.0f`（`docs/111` §4.5 把 `-5.0f` 归给 humm 是**错的**） | **观测** |
| 7 | `game_state_info_event` 的 payload `+0x8` **等于 1** 时清空待播队列 | **逐位** |
| 8 | 静音 = `+0xd0`（用户）`|` `+0xd1`（隐式），**只设 Master 总线** | **逐位** |
| 9 | `docs/111` §5.3 悬着的 `+0xd1` 就是 `set_hidden_muted` 写的那个字节 | **逐位** |
| 10 | 结构体 **0xf0 = 240 字节**（构造助手 `alloc(0xf0,8)`），队列元素 **0x18 = 24 字节** | **逐位** |

### 27.2 函数清单（arm64 名字是符号名，不是猜的）

| 函数 | x86 vaddr / 字节 | arm64 vaddr / 字节 | 源码行 | 职责 |
|---|---|---|---|---|
| `process` | `0x4bfe10` / 5052 | `0x4c5040` / 4268 | 170 | `humm` 的 PCM 泵（每帧） |
| `new` | `0x4bf550` / 2230 | `0x4c4964` / 1756 | 245 | 载 6 个资源 + 订阅 4 条 |
| `effect_info_event` | `0x4bf090` / 681 | `0x4c442c` / 712 | 328 | 按 kind 选槽入队 |
| `game_state_event` | `0x4bef20` / 212 | `0x4c42c8` / 212 | 351 | `state == 1` 清队列 |
| `set_muted` | `0x4c11d0` / 578 | `0x4c60ec` / 548 | 362 | 写 `+0xd0` + 总线静音 |
| `set_hidden_muted` | `0x4bf000` / 134 | `0x4c439c` / 144 | — | 写 `+0xd1` + 总线静音 |
| `soldier_info_event` | `0x4bf340` / 188 | `0x4c46f4` / 216 | — | 入队 `pop`（0 dB） |
| `entity_removed_event` | `0x4bf480` / 193 | `0x4c488c` / 216 | — | 入队 `unpop`（-5 dB） |
| `EffectAudioData::new` | `0x4bed70` / 423 | `0x4c4144` / 388 | — | 建一条待播记录 |
| `subscribe_to_events`（4 个闭包） | 内联进 `new` | `0x4c47cc` `0x4c47fc` `0x4c482c` `0x4c485c` | — | 订阅胶合层 |

x86 的 5 个锚（`:170 / :245 / :328 / :351 / :362`）来自 `fixtures/name_to_addr.tsv`；
余下 4 个在 x86 侧无锚，用 arm64 符号名 + 尺寸比（1.07 / 1.15 / 1.12 / 1.09，全在闸内）
+ 每个函数一处独立字节证据定名：

* `set_hidden_muted` = `0x4bf000`：函数第一件事就是 `mov %bl,0xd1(%rdi)`。
* `soldier_info_event` = `0x4bf340`：`lea rdi,[rbx+0xb0]`（选 `pop` 槽）。
* `entity_removed_event` = `0x4bf480`：`lea rdi,[rbx+0xc0]`（选 `unpop` 槽）+ `movss 0x1b9ba0`（-5.0f）。
* `EffectAudioData::new` = `0x4bed70`：`docs/111` §4.2 记的"包住 `set_stream` 的辅助函数"
  ——arm64 侧 6 个调用点（`effect_info_event` ×4、`soldier_info_event` ×1、`entity_removed_event` ×1）
  与文档里"6 次 `set_stream`"逐一对上，**这个名字可以收口了**。

### 27.3 四条订阅与"调用链闭合"

```
new: Box::new(8) ──> 闭包壳 ──> vtable(0x85b0f8/20/48/70) ──[+0x18]──> call_mut ──> 处理方法
```

| # | 事件名 | 长度立即数 | 判别值 | vtable | `call_mut` | 处理方法 |
|---|---|---|---|---|---|---|
| 1 | `effect_info_event` | `0x11`=17 | 44 | `0x85b0f8` | `0x4d5434` | `effect_info_event` |
| 2 | `soldier_info_event` | `0x12`=18 | 15 | `0x85b120` | `0x4d632c` | `soldier_info_event` |
| 3 | `entity_removed_event` | `0x14`=20 | 14 | `0x85b148` | `0x4d560c` | `entity_removed_event` |
| 4 | `game_state_info_event` | `0x15`=21 | 30 | `0x85b170` | `0x4d5f58` | `game_state_event` |

⚠️ 方法名叫 `game_state_event`，**订阅的事件名是 `game_state_info_event`**（30，多 `_info`）。

### 27.4 事件 → 音效（`docs/111` §5.1 的答案）

| 资源槽 | 文件 | 载入顺序 | 触发事件 | 音量 |
|---|---|---|---|---|
| `+0x80` | `res://sounds/explosion.mp3` | 1 | `effect_info_event`，`kind == 0` | `-15.0f` |
| `+0x90` | `res://sounds/squash.mp3` | 2 | `effect_info_event`，`kind == 1` | `-15.0f` |
| `+0xa0` | `res://sounds/spark.mp3` | 3 | `effect_info_event`，`kind == 2` 或其余 | `-15.0f` |
| `+0xb0` | `res://sounds/pop.mp3` | 4 | `soldier_info_event`(15) | `0.0f` |
| `+0xc0` | `res://sounds/unpop.mp3` | 5 | `entity_removed_event`(14) | `-5.0f` |

**槽 ↔ 文件这一列怎么来的（逐位，两条独立链）**

1. **载入顺序**：`new` 里 5 次 `load(path, len)` 依次是 explosion/squash/spark/pop/unpop（`docs/111` §4.4 已列）。
2. **写入顺序**：构造助手（x86 `0x5238c0`，`alloc(0xf0,8)` 后逐字段初始化）把
   源结构体的 `+0x18/+0x28/+0x38/+0x48/+0x58` 搬到载荷的 `+0x80/+0x90/+0xa0/+0xb0/+0xc0`；
   源结构体这五个字段正是按上面 1..5 的顺序写的（`[rsp+0x130]…[rsp+0x108]` 的搬运顺序），
   而**载荷基址 = `rbx+0x18`** 由最后那条 `mov word ptr [rbx+0xe8], 0` 定死
   （`+0xe8-0x18 = payload+0xd0` = 两个静音字节，与 `set_muted` 写的 `+0xd0` 对上）。
3. **读侧**：三个处理方法读的正是 `+0x80…+0xc0`（`effect_info` 四臂、`soldier` 的 `+0xb0`、
   `entity_removed` 的 `+0xc0`），x86 侧 `lea rdi,[rbx+0xb0]` / `[rbx+0xc0]` 同址。

语义自洽：士兵**出现**响 `pop`、**移除**响 `unpop`，三种命中特效响 `explosion`/`squash`/`spark`。

**`effect_info_event` 的分支（逐位）**：先断言判别值 `0x800000000000002c` = 44，
再按 payload `+0x20` 的 u8 分四臂 —— `0 → +0x80`、`1 → +0x90`、`2 → +0xa0`、其余 → `+0xa0`
（`cmp #1 / b.gt / cmp #2 / b.ne` 的骨架，四臂落三槽，与 `docs/111` §4.5 的结构描述一致）。
载荷 `+0x1c` 的 u32 原样带进记录的值字段。
`soldier_info_event` / `entity_removed_event` **不校验判别值**（两侧都是），值恒 0。

### 27.5 队列 / 清空 / 静音 / `humm`

* **队列**：`Vec<EffectAudioData>` 在 `self+0x00`，字段序 **cap / ptr / len** = `+0x00/+0x08/+0x10`；
  元素 **0x18 字节**（`mov w8,#0x18; madd x8, len, x8, ptr`）：
  `{ +0x00 16 字节句柄, +0x10 u32 值, +0x14 bool 已启动 }`。
  `push` 前有一次 `len == cap` 的增长检查（`cmp x20, x8; bl <Vec::grow>`）。
* **清空**：`game_state_event` 读 payload `+0x8` 的 u8，**等于 1** 时 `len := 0` 并按保存的旧 len
  逐个清理（每个元素 +0x8 处的句柄先做有效性检查）。其它状态什么都不做。
* **静音**：`+0xd0`（用户）/ `+0xd1`（隐式），有效值 = `a | b`（`orr w8, w19, w22; and w2, #1`），
  动作是 `AudioServer::set_bus_mute(AudioServer::get_bus_index("Master"), 有效值)`。
  `set_muted` 与 `set_hidden_muted` 是**同一套动作的两个入口**，只差写哪个字节。
* **`humm`**：`FileAccess::get_file_as_bytes` → `hound` 解析 → `AudioStreamGenerator`
  （`set_mix_rate(16000.0f)`、`set_buffer_length(0.1f)`）→ `push_buffer`，
  归一化 `32767.0f`；一帧样本预算 `16000 × 0.1 = 1600`。这条路径**不经过**那 5 个资源槽。

### 27.6 与 `docs/111` 的差异（就地改正）

| # | `docs/111` 原文 | 本轮实测 |
|---|---|---|
| 1 | §4.5 "`humm` 那一路是 -5.0 dB" | **错**：`-5.0f` 在 `entity_removed_event`（`0xc0` 槽）里，对应 `unpop.mp3`；`humm` 走 generator，不是这 5 个槽之一 |
| 2 | §4.4 把 `0x4c01b9`(set_stream) / `0x4c0cba`(play) 记在 `new` 名下 | **错位**：`0x4bfe10` 起 5052 字节是 `process`，这两条落在 `process` 里（`new` 只到 `0x4bfdf0` 一带） |
| 3 | §5.3 "`+0xd1` 由谁写、语义未定" | **结案**：`set_hidden_muted` 写它；与 `+0xd0` 相或后设 Master 总线 |
| 4 | §5.1 "音效 ↔ 事件 未知" | **结案**：见 §27.4 表 |

### 27.7 仍未确定

| # | 项 | 缺什么 |
|---|---|---|
| A | `self+0x60` 那个 16 字节 `Option<Gd<…>>` 的对象身份 | `EffectAudioData::new` 用它做 `set_stream`/`set_volume_db` 的接收者（绑定字符串簇是 `AudioStreamPlayer is_playing set_volume_db get_stream_playback`），但构造路径把它写成 `([rsp], rbp)`、第二字为 0 ⇒ 只能标**推断**；缺一次运行时 hook 打印该句柄 |
| B | `EffectAudioData` 记录的**播放时机** | `process` 里 `play()` 的调用点没有逐指令闭合（`docs/111` §4.4 的地址错位也在这条上）⇒ **推断** |
| C | `tear.mp3` / `humm_2` / `humm_b` / `shepard_2` 的引用 | 仍是负结果（`docs/111` §5.2），本轮没有新证据 |

### 27.8 复现

```text
# 名字与尺寸（arm64 未 strip）
nm -S --defined-only <arm64>/libgraphwarrust.so | grep graphwar_audio_control
# 订阅链：new 里的 adrp/add 指向事件名串；对应 vtable 的第 4 项是 call_mut
#   vtable 地址来自 add x4, x4, #0xf8/0x120/0x148/0x170
# x86 侧函数边界（只读 .so，不需要 Ghidra）
readelf --debug-dump=frames <x86>/libgraphwarrust.so     # .eh_frame 的 FDE
```

---

## 28. `graphwar_graphics_manager.rs` 规格 + **`GraphwarTextureLoader` 到底在哪**（本轮新增：`docs/02` §686 那条"未知"结案）

> 任务是把 `graphwar_graphics_manager`（"22 个完全未知"之一）解出来，
> 并回答 `GraphwarTextureLoader` 的宿主是不是它。**答案：不是**——两者是两个模块。
> 代码落在 `src/graphwar_graphics_manager.rs`，验收在 `tests/graphwar_graphics_manager.rs`。

### 28.1 一页结论

| # | 结论 | 等级 |
|---|---|---|
| 1 | `GraphwarGraphicsManager` 三张表 `HashMap<u32, String>`，在 `+0x18 / +0x48 / +0x78`（步长 **0x30**）= 皮肤 / 脸 / 帽子 | **逐位** |
| 2 | 三个 setter 同形（各 276 B）：写表 + 发 `Skin/Face/HatGraphicsInfo`(51/53/55) | **逐位** |
| 3 | 只订阅两条：`new_connection_event` → `send_all_info_events`；`entity_removed_event` → `handle_entity_removed_event` | **逐位** |
| 4 | `send_all_info_events` 用基数 `0x8000…0033` **加** `0/2/4` 造三个判别值（脸是 `add #2`、帽子是 `orr #4`） | **逐位** |
| 5 | 外观来源只有远端事件：`RemoteEventHandler::handle_{client,server}_remote_event`；另两条旁路是 `GameStateManager::add_target`(skin) / `add_soldier_to_player`(face+hat) | **观测** |
| 6 | **`GraphwarTextureLoader` 属于 `src/graphwar_texture_loader.rs`**（arm64 符号的模块段就是 `graphwar_texture_loader`） | **逐位** |
| 7 | 它持三张 `HashMap<String, Gd<Texture2D>>` 在 `+0x150 / +0x180 / +0x1b0`（同样步长 0x30），表项 **0x28 = 40 字节** | **逐位** |
| 8 | **按名字（`String`）索引**，不是按编号：hash 表 + 逐字节 `memcmp` | **逐位** |
| 9 | 由 `GraphwarRootNode`（`impl INode`）里的**唯一**构造点 `0x39a328` 建一次；`new` **21212 字节** ⇒ 饿汉式一次性加载 | **观测** |

### 28.2 `graphwar_graphics_manager`：6 个函数

| 函数 | x86 vaddr | arm64 vaddr / 字节 | 源码行 |
|---|---|---|---|
| `new` | `0x469390` | `0x49e394` / 396 | 38 |
| `set_skin_graphics` | `0x468c70` | `0x49dd00` / 276 | 60 |
| `set_face_graphics` | `0x468b60` | `0x49dbec` / 276 | 69 |
| `set_hat_graphics` | `0x468a50` | `0x49dad8` / 276 | 78 |
| `send_all_info_events` | `0x468d80` | `0x49de14` / 1036 | 88 |
| `handle_entity_removed_event` | `0x469220` | `0x49e220` / 280 | 108 |

6 个方法名 + 6 个 `panic::Location` 锚双向唯一（`fixtures/name_to_addr.tsv`），
`subscribe_to_events_event_handler` 内联进 `new`（两个闭包 `0x49e338`/`0x49e368`）。

### 28.3 数据形状（逐位）

三个 setter 骨架完全同形，差别只有三处常量：

```text
set_X_graphics(self, player_id: u32, name: &str)
  st = self[8]                        ; 事件通道（+0x10 borrow 标志 / +0x18 emit 入口）
  if st[0x10] != 0 → panic("src/graphwar_graphics_manager.rs")
  event = { tag: 0x8000…|TAG          ; 0x33 / 0x35 / 0x37
            String(cap,ptr,len) @+0x08,
            u32 player_id       @+0x20 }
  emit(st + 0x18, &event)
  map.insert(player_id, name)         ; map = self + 0x18 / 0x48 / 0x78
```

每张表是 hashbrown `HashMap`：
`ctrl@+0, bucket_mask@+8, growth_left@+0x10, items@+0x18, RandomState@+0x20`。
`handle_entity_removed_event` 里三次 `hash(self+0x38 / +0x68 / +0x98)` 正好是
"表基 + 0x20"，**这就是三张表的自证**。

`send_all_info_events`（1036 B）：三个循环，判别值 = 基数 + `k`：
皮肤直接用 `0x8000…33`；脸 `add x19, x9, #2`；帽子 `orr x27, x9, #4`。
（`0x33 | 2 == 0x33`，所以脸那一支**只能**是加法——编译器混用 `add` / `orr` 反而把基数钉死了。）

### 28.4 外观数据从哪来（观测，arm64 全量 `bl` 扫描）

| 调用点 | 所在函数 |
|---|---|
| `0x3e4df4` `0x3e4e84` `0x3e5060` `0x3e50f0` `0x3e5294` `0x3e5324` | `GraphwarRemoteEventHandler::handle_client_remote_event`（`0x3e3ad0`） |
| `0x3e6f34` `0x3e794c` `0x3e7f18` | `GraphwarRemoteEventHandler::handle_server_remote_event`（`0x3e6df0`） |
| `0x4a5868`（skin） | `GraphwarGameStateManager::add_target`（`0x4a5704`） |
| `0x4aaf24` `0x4ab050`（face/hat） | `GraphwarGameStateManager::add_soldier_to_player`（`0x4aaab4`） |

⇒ 名字**只从远端事件**进来（`*_graphics_info_event`），本机不生成。

### 28.5 `GraphwarTextureLoader` 的宿主（问题的正面答案）

arm64 `.symtab` 的模块段直接写着答案：

```text
_ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader3new
_ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader15get_hat_texture
_ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader16get_face_texture
_ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader16get_skin_texture
```

⇒ 它是 **`src/graphwar_texture_loader.rs`**，与 `graphwar_graphics_manager.rs` 平级；
`graphics_manager` 只存 `player_id → 名字`，**贴图本体在 loader**。
`docs/02` §686 把两者混在一起问"谁持有资源表、谁只做加载"，本轮分工是：

* `graphics_manager`：**索引层**（谁穿什么），事件源、三条 info 事件、三张表。
* `texture_loader`：**资源层**（名字 → `Gd<Texture2D>`），三张表 + 3 个 getter + 一次性加载。

**表与索引（逐位）**

| getter | arm64 vaddr / 字节 | 表偏移 | 索引方式 |
|---|---|---|---|
| `get_skin_texture` | `0x40c1cc` / 296 | `+0x150` | `HashMap<String, Gd<Texture2D>>` |
| `get_face_texture` | `0x40c0a4` / 296 | `+0x180` | 同上 |
| `get_hat_texture` | `0x40bf7c` / 296 | `+0x1b0` | 同上 |
| `new` | `0x40c2f4` / **21212** | — | 一次性载入全部 |

三个 getter 同形（296 B）：`items == 0` 快路返回 `None` → `hash(name, state=表基+0x20)`
→ 探 ctrl 字节 → 命中桶比 `len` → `memcmp(key, entry)` → clone 值（8 字节 `Gd`）。
表项 **0x28 = 40 字节**（`mov w25, #0x28`：1 ctrl + 24 `String` + 8 `Gd`，对齐到 40）。
⇒ **按名字索引**（键就是 `res://…` 里的资源名），**不是**按编号。

**创建与所有者（观测）**：`GraphwarTextureLoader::new` 全二进制**只有一个调用点 `0x39a328`**，
落在 `impl INode for GraphwarRootNode`（arm64 `0x399dd0` 起）里；`GraphwarRootNode` 用
`Weak<Rc<RefCell<GraphwarTextureLoader>>>` 持有它（drop 符号里带完整类型路径）。
与既有结论"持有者是 `GraphwarRootNode + 0x16a0`"一致 —— 本轮补上的是**谁调用 `new`**。

**消费者（getter 的全部调用点，观测）**

| getter | 所在函数 |
|---|---|
| skin | `PlayScreenControl::add_player_victory_icon`、`PlayerShowcase`、`PlayerScreenControl::load_items` / `update_buttons_background`、`EntityDrawer::get_entity_skin_sprite`、`SetupControl::update_soldiers` |
| face | `add_player_victory_icon`、`PlayerShowcase`、`load_items`、`EntityDrawer::get_entity_face_sprite`、`update_soldiers` |
| hat | `add_player_victory_icon`、`PlayerShowcase`、`load_items`、`EntityDrawer::get_entity_hat_sprite` |

`EntityDrawer::get_entity_{skin,face,hat}_sprite` 三个**函数名本身**就是 §21/§22
"`EntityDrawer` 从 loader 取三张贴图"的正面证据。

### 28.6 两处需要提醒后来者的歧义

| # | 项 | 现状 |
|---|---|---|
| A | §21/§22 记的"`PlayerShowcase` 自己有三张 `HashMap`（`+0x168/+0x198/+0x1c8`，间距 0x30）" | 这三个偏移**正好等于 loader 三张表的 `items` 字段**（`+0x150+0x18` / `+0x180+0x18` / `+0x1b0+0x18`）。两种读法都能解释字节：① 那是 `get_*_texture` 内联后的表访问被误记成 PlayerShowcase 的字段（**推断**）；② PlayerShowcase 另有一套同形缓存表（**未证**）。**本节不选边** |
| B | `docs/02` §1187 列了 6 个方法（多 `get_chat_texture` / `get_time_mode_texture`） | arm64 `.symtab` 里**只有 4 个**（`new` + 3 个 getter）。缺的两个要么被内联/合并，要么旧表多列 ⇒ **未知** |

### 28.7 复现

```text
nm -S --defined-only <arm64>/libgraphwarrust.so | grep graphwar_texture_loader
# 三个 getter 的表偏移：函数体里 ldr x9,[x0,#0x168/0x198/0x1c8]（= items）
#                       与 add x0,x0,#0x170/0x1a0/0x1d0（= RandomState）
# 构造点：全量反汇编里 bl 0x40c2f4 只有一个（0x39a328，在 GraphwarRootNode 的 INode impl 里）
```

## 29. 靶子 / 特效 / 录像 / 回放 七模块（本轮新增：`docs/123` 的判定点结案 + `video_writer` 不是"猜错的名字"）

> 任务：`graphwar_{target,entity_effect,effect}_manager` / `graphwar_game_recorder` /
> `graphwar_video_writer` / `graphwar_replay_control` / `graphwar_texture_loader`。
> 代码落在同名 `src/*.rs`，验收在 `tests/` 同名文件。
> ⚠️ 本节**只**回答已经能指到地址的部分；推断一律标出来，未知一律列在 §29.7。

### 29.1 一页结论

| # | 结论 | 等级 |
|---|---|---|
| 1 | `target_manager` 是**一张 `HashSet<u32>`**（桶距 4、值类型 `()`），只有 3 个可归属函数 + 被内联的 `add_target` | **逐位** |
| 2 | 它**没有** alive 字段、**没有**任何重置分支 ⇒ `docs/123` §2.5「不在目标管理器里」在指令级成立 | **逐位** |
| 3 | 「打不全就全部复原」的判定点在 **`GameStateManager::queue_end_turn_trigger::{{closure}}`**（arm64 `0x4abc00`）：**回合末**、两队都还有人活着、`self[+0x11d]==1` ⇒ 遍历 `target_manager` 的集合并 `life_manager.set_alive(id, true, 0)` | **观测** |
| 4 | 死亡特效**不是死亡时判定的**：`execute_results` 拿 `entity_effect_map[entity_id]` 当类型、拿**会话级自增计数器**当 `effect_id`（**§33.6 改正了旧稿的「实体 id」**）；**查不到就一条都不发** | **逐位** |
| 5 | ⇒ `AshEffect` / `DeathEffect` 的分界在**实体出生登记**（`EntityEffectInfo`），这解释了 `docs/141` 的 19 例；**登记规则已解到指令**（士兵看皮肤 `bot_` 前缀、靶子恒 `Spark`）——见 §33 | **逐位** |
| 6 | `effect_manager`：登记 + 广播 `EffectInfo`(44)；`tick` 只回收 **`start_tick + 3600 <= now`** 的记录，不发事件 | **逐位** |
| 7 | `EffectType` 四变体判别值 `0..3` = `ExplosionEffect/DeathEffect/SparkEffect/AshEffect`（`.rodata 0x19caf8` 首尾相接） | **逐位** |
| 8 | `game_recorder` 录 **32 条**服务端广播事件（`{"event":…,"tick":N}` 逐行），**`GameStateInfo==EndingGame`** 时落盘到 **`user://replays/<unix 秒>`**（gzip，flate2） | **逐位** |
| 9 | `video_writer` **真的是录视频**：`openh264::Encoder`（H.264）+ `minimp4` 封装 MP4，**60 fps**，轨道名 `Graphwar Clip`，`FileAccess` 一次写完 | **逐位** |
| 10 | `texture_loader` 是 **4 个方法不是 1 个**（矩阵那个 1 是 `panic::Location` 口径的下限）；三张 `HashMap<String, Gd<Texture2D>>` @ `+0x150/+0x180/+0x1b0`，**119 条** `res://*.svg` | **逐位** |
| 11 | `replay_control`：列表**异步**加载（`mpmc` 通道 + 后台线程 `load_game`），四个模式按钮 + `set_button_icon`，`process` 按 tick 推进并重放几何 | **观测** |

### 29.2 (a) 靶子重置的判定点

`docs/123` §3.2 列了三条候选、全部出局；本轮的答案在**另一处**：

```text
GameStateManager::queue_end_turn_trigger::{{closure}}   arm64 0x4abc00 (0x58c)
  turn_count += 1 ; add_event(TurnCountInfo{58, turn_count})        ; 0x4abc20-0x4abc5c
  w23 = if turn_count < self[+0xcc] { 0 } else { self[+0xc8] }      ; 0x4abc60-0x4abc78
  (t1, t2) = get_alive_teams()                                      ; 0x4abc7c
  if (w23 & 1) == 0 && t1 && t2 && self[+0x11d] == 1 {              ; 0x4abc80-0x4abc94
      for id in target_set { life.set_alive(id, /*alive=*/1, 0) }   ; 0x4abea8-0x4abef8
  }
```

* 「谁会写 `LifeInfo{alive:true}`」是反查出来的：全库构造判别值 27 的位置**只有两处**
  （`life_manager::send_all_info_events` `0x497240`、`set_alive` `0x497cb0`）；
  `set_alive` 的全部调用点是 5 个，其中唯一带条件的就是这个闭包；
* 与 `docs/123` §0 的①③④⑤完全一致：**翻 alive + 广播 + 不重建实体 + 不恢复障碍洞**；
* 三个条件里的 `self[+0x11d]`（u8）是**推断**为「一発で全部」关卡的标志位，
  `+0xc8/+0xcc` 与 `(w23 & 1) == 0` 的用意**未定**（见 §29.7）。

**`target_manager` 里没有这条规则**：它连「谁死了」都不存 —— 表项 4 字节、
值类型 `()`，三个可归属函数里唯一写操作是 `remove(&entity_id)`。

### 29.3 (b) 死亡时发哪个特效

`GraphwarGameStateManager::execute_results` 里有两处 `EffectInfo`（44）的生产点：

| 生产点 | `effect_id` | `effect_type` | 语义 |
|---|---|---|---|
| `0x4a89ec` | **自增计数器**（`ldxr/stxr`） | **恒 0** = `ExplosionEffect` | 弹道停止/命中的爆炸 |
| `0x4a8cb0` | **同一个自增计数器**（`0x4a8c68` 的 `ldxr w23,[x28]`）〔§33.6 改正：旧稿误记成「实体 id」〕 | **`entity_effect_map[entity_id]`** | 死亡 |

第二处的查表就是 `GraphwarEntityEffectManager` 的 `HashMap<u32, EffectType>`（桶距 8，
键 `u32` 在 +0、值 `u8` 在 +4）：`hash_one(+0x50, &entity_id)` → 键比较
`subs wzr, w23, w14` → 命中后 `ldurb w21, [x13, #-0x4]`。**未命中分支 `0x4a8c38`
直接释放借用、跳过 `add_event`**。

⇒ **分界条件**：这个实体**出生时**被 `EntityEffectInfo` 登记成什么，死亡就发什么；
没登记就什么都不发。任何 `unwrap_or(DeathEffect)` 都会让 `docs/141` 那 19 例继续错。

**登记规则在 §33 解完了**（士兵：皮肤 `bot_*` ⇒ `Ash`、否则 `Death`；靶子：`Spark`；
迟到的 `SkinGraphicsInfo{bot_*}` 会改写），那里还有 6 个写入点的地址与数据侧 899/953 的复核。

### 29.4 (c) 录像写什么、何时写、写到哪

```text
new       : 32 条订阅 = 24 条 *_info_event + 8 条同样单向广播的
            (entity_removed / remove_team / function_fire / function_active /
             function_points / clear_obstacles / add_obstacle / destroy_obstacle)
            一条 *_request_event 都不订；符号表里只有一个闭包 ⇒ 32 个 Box 同一个类型
on_event  : tick = get_module_wk(self+0x40)[+0x58] ; events.push({tick, event})
            if GameStateInfo(30):
                state == 6 EndingGame ⇒ 上升沿落盘
                state == 1 Setup && 上一条是 6 ⇒ 清空缓冲（新一局）
save_game : DirAccess::make_dir_recursive_absolute("user://replays/", 15)
            path = "user://replays/" + SystemTime::now().duration_since(UNIX_EPOCH).as_secs()
            FileAccess::open(path, WRITE) → GzEncoder(flate2 1.0.30) → 每行
            '{"event":<GraphwarEvent 的 serde 外部标签>,"tick":N}'
```

`EventInfo` = **0x50 字节** = `GraphwarEvent` 0x48 + `tick`（u32 + 对齐）。
写侧（反汇编 + flate2 的 `panic::Location`）与读侧（`src/graphwar_replay_player.rs`
解析 400 份真实回放）**逐字吻合**。

### 29.5 (d) `video_writer` 到底是什么

**是录像导出，不是别的东西被我们猜错名字。** 被调者符号名把链路钉死：

| 步骤 | 证据 |
|---|---|
| H.264 编码 | `openh264::encoder::Encoder::new`(`0x4499b0`) / `encode_at`(`0x449cb0`) / `EncodedBitStream::write_vec`(`0x449cd8`) |
| RGB→YUV | `openh264::formats::rgb::{Rgb,Rgba}SliceU8::new` + `YUVBuffer::read_rgb` |
| MP4 封装 | `minimp4::Mp4Muxer::init_video`(`0x449550`) / `minimp4::writer::write_mp4`(`0x449564`) / `MP4E_close` |
| 帧率 | `mov w1, #0x3c` = **60** |
| 轨道名 | `.rodata 0x19ddaf` = `Graphwar Clip`（13 字节，`mov w5,#0xd`） |
| 落盘 | `FileAccess::open(path, 2)` + `store_buffer(PackedArray<u8>)` |

构造 `new(w, h, flag)` 把宽高**向上取偶**（`w + (w & 1)`，`0x4499d8`/`0x4499dc`），
存 `+0x14e8/+0x14ec`（原始）与 `+0x14f0/+0x14f4`（取偶）。`minimp4` 自己的
`panic::Location` 也出现在 `write_video` 的 Location 表里 ⇒ 依赖是函数级确认的。

### 29.6 七个模块的函数清单

| 模块 | arm64 方法数 | 可归属（x86 锚） | 本轮落码 |
|---|---|---|---|
| `graphwar_target_manager` | **3**（`new` + 2 闭包）+ 内联 `add_target` | 4 枚锚（`:35:24` `:65:26` `:74:30` `:84:18`） | 全 |
| `graphwar_entity_effect_manager` | **3** + 内联 `add_entity_effect` | 4 枚锚（`:35:31` `:57:26` `:67:30` `:77:18`） | 全 |
| `graphwar_effect_manager` | **5**（含 `EffectType::deserialize`） | 2（`tick` `0x519d80`；`add_effect` 只在内联副本里） | 全 |
| `graphwar_game_recorder` | **3** | 4 枚锚（`:107:105` `:108:41` `:150:81` `:156:74`） | 全（gzip 只做文本侧） |
| `graphwar_video_writer` | **3** | 4 枚锚（`:61:37` `:125:41` `:154:56` `:154:93`） | 结构全；**不产 MP4**（无 openh264） |
| `graphwar_replay_control` | **4** | 2（`process` `0x4c6f40`、`update_replay_list` `0x523410`） | 结构全；时钟字段未解 |
| `graphwar_texture_loader` | **4** | 1 枚锚（`:145:47`，落在 `new`） | 三张表全（值是路径字符串，不是 `Gd`） |

`coverage-matrix` 里记的「1 个可归属函数」= `panic::Location` 口径的**下限**，
不是模块规模：`texture_loader` 的 3 个 getter 一次都不 panic，所以数不到。

### 29.7 未定项（都带地址，可直接接手）

| # | 未定 | 已知 |
|---|---|---|
| 1 | `self[+0x11d]`（u8）是哪个关卡字段 | 必须 `== 1` 才复原；**推断**为「一発で全部」标志 |
| 2 | `self[+0xc8]` / `self[+0xcc]` 与 `(w23 & 1) == 0` | `+0xcc` 参与 `turn_count <` 比较；偶数要求**无解释** |
| 3 | `set_alive` 第三参数（`w3 = 0`） | 第二参数是 `alive`；第三个**未知** |
| 4 | `tick` 取的模块身份 | `get_module_wk::h4ae2b3f89e2f8a81` 被 recorder / turn_manager / function_manager / gsm 共用，读 `+0x58` 当 tick；**推断**为 `TickHandler`（`TypeId` 常量不在符号里） |
| 5 | `video_writer` 第三参数（`+0x14f8` 的 bool） | 设 `EncoderConfig` 的某个字段，字段名没逐条读 |
| 6 | `replay_control::process` 的时钟字段 | 6760 字节，只知按 tick 推进 + 引用 `graphwar_plane.rs:174/179`（会重放几何） |
| 7 | `texture_loader` 里 `ranks/` 与 UI 图标的查询入口 | 加载了 50 张（22 徽章 + 28 图标），**没有任何按名字取它们的方法** |
| 8 | 士兵出生时登记的实体特效类型 | 回放侧可统计，代码侧未定位构造点 ⇒ 不给默认值 |

### 29.8 复现

```text
ARM=.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so
nm -S --defined-only $ARM | grep -E "graphwar_(target|entity_effect|effect)_manager|game_recorder|video_writer|replay_control|texture_loader" | grep -v core3ptr
# 判定点：反查 set_alive(0x497bb0) 的调用点，看 0x4abc00 那段三个条件
# 死亡特效：搜 0x800000000000002C (EffectInfo=44) 的 mov 立即数对 → execute_results 两处
# 录像格式：0x3d8184 里 "user://replays/" + u64 Display + FileAccess::open(...,2)
# 视频：0x4494e4 的 bl 目标全是 openh264/minimp4
# 纹理表：0x40bf7c/0x40c0a4/0x40c1cc 里 ldr [x0,#0x1c8/0x198/0x168] + add x0,x0,#0x1d0/0x1a0/0x170
```

## 30. 最后一批杂项小模块：11 个"至今没定性的东西"逐条结案（本轮新增）

本节处理的是 `docs/02-module-map.md` §18.4 / §17.5 里那批**可归属函数 0~2 个**的模块。
它们每个都不大，但每一个都曾经是"我们知道名字、不知道它是什么"的空白。
**0 个可归属 ≠ 没内容**：`graphwar_game_info` 一整个结构体、`graphwar_event_queue`
全游戏的发件箱，都在这批里。

### 30.0 证据链与新工具（读结论前先看这一节）

本轮**没有**用 Ghidra 伪代码下任何结论，只有三类证据：

| 代号 | 来源 | 怎么复核 |
|---|---|---|
| **A** | **2.0.60 arm64 未 strip 版**的 `.symtab`（`graphwar_crash_reporter` 等 0 锚模块**只在这里有名字**） | `nm -C -S --defined-only $ARM \| grep graphwarrust::<模块>::` |
| **B** | **arm64 `bl` 图**：把 `.text` 按 `bl` 目标聚成 caller/callee（符号表给了函数边界，不需要 FDE） | 见本节 §30.10 的复现命令 |
| **C** | **x86-64 目标**的 `panic::Location` 锚 + `.rodata` 字面量直引 | `../.venv/bin/python tools/loc_xrefs.py loc src/<模块>.rs` |

**为什么必须用 A**：x86 目标被 strip，`docs/02.md` §17 的模块名是从 `.rodata` 残留的
`src/<mod>.rs` 字面量反推的。**而这 11 个模块里有 6 个连一个字面量都没有**：

```text
                       arm64 src/<mod>.rs 字面量     x86 同
graphwar_player_config            0                    0
graphwar_game_info                0                    0
graphwar_event_queue              0                    0
backtracer                        0                    0
function_token                    0                    0
graphwar_crash_reporter           8                    8
```

⇒ 这 5 个模块在 x86 目标上**没有任何字符串可用作搜索键**，只能靠 arm64 的符号名 + 行为指纹
搬回来。这也解释了为什么它们长期停在"未做"：**不是没做，是没有入口**。

（`graphwar_crash_reporter` 的 8 份是唯一的异常值：其它模块各 1 份。它见 §30.3。）

### 30.1 `polish_notation_converter` —— **活代码，而且是解析器的后半段**（旧结论要改）

**问题 a 的答案：活的，删了 `sin(x)^2` 的语义就没了。唯一调用者是 `MathFunction::new`。**

x86 目标 `0x4f5450`（`src/polish_notation_converter.rs:36:52` / `116:55` 两个锚，`逐位`）。
`tools/loc_xrefs.py` 的 `calls` 表里它的调用点有三个：`0x4edd1f`、`0x4f56b3`、`0x4f5721`——
后两个是**自递归**，第一个在 `MathFunction::new`（`0x4ed670`）里。arm64 版同构：
`PolishNotationConverter::reorder_recursive`（`0x44db78`）← `MathFunction::new`（`0x446c4c`）+`0x28c`。

调用点上下文（`逐位`，x86 `0x4edcc9..0x4edd24`）：

```text
4edcc9  mov 0x120(%rsp),%rdx        ; rdx = 输入 token 数 len
4edce9  movq $0x0,0xa8(%rsp)        ; output Vec: cap=0
4edcf5  movq $0x8,0xb0(%rsp)        ;             ptr=8（16 字节对齐的 dangling）
4edd01  movq $0x0,0xb8(%rsp)        ;             len=0
4edd0d  test %rdx,%rdx ; je 跳过     ; len==0 就不调用
4edd12  dec %rdx                    ; end = len-1   ← 闭区间
4edd15  lea 0x90(%rsp),%rdi
4edd1d  xor %esi,%esi               ; start = 0
4edd1f  call 0x4f5450
```

**修正 `docs/03-function-language.md` §13 的口径**：那里写的"解析器 `MathFunction::new` 自己就做分词"
**只对了一半**。它自己做的是**分词**（正则 + `FunctionToken::new`），
但**括号、优先级、缺操作数补 0 全在 `polish_notation_converter` 里**。
分词产物是**中缀**的，且**含两个额外 token：tag 18 `(`、tag 19 `)`**（`逐位`，
`FunctionToken::new` 的单字节跳转表里 `'('` → 18、`')'` → 19）。这两个 tag 在
`docs/03` 的 `0..=17` 表里**没有**，因为它们是**中间表示专用**、转换完就消失。

#### 30.1.1 原版正则（**第一次逐字节拿到**，`逐位`）

`.rodata` 里 77 字节，arm64 `0x19dcb4` 与 x86 `0x1c4347` **逐字节相同**：

```text
[0-9]*\.?[0-9]+|\(|\)|x|y'|y|\+|-|\*|/|\^|sqrt|log|abs|sin|cos|tan|ln|n|exp|e
```

三个可观测后果：

* **交替顺序 = 优先级**：`y'` 在 `y` 前、`exp` 在 `e` 前、`sqrt|log|abs|sin|cos|tan|ln` 全在 `n|e` 前。
* **`5.` 只切出 `5`**：尾部那段 `[0-9]+` 硬性要求数字，孤立的 `.` 被 `find_iter` 丢掉。
* **没有任何"插乘号"**：`2x` 只切出 `2`、`x` 两个 token，中间不补 `*`。

在此之前还有两步**字符串重写**（`逐位`，`MathFunction::new` 开头）：

```text
逐字节 to_ascii_lowercase   （sub w12,w11,#0x41 ; cmp w12,#0x1a ; orr w11,w12,w11）
str::replace("pi", "n")     （0x19dcaf len=2 → 0x19dcb1 len=1）
str::replace("π",  "n")     （0x19dcb2 len=2，字节 CF 80）
```

⇒ `docs/03` 的 "`pi`/`π`/`n` = π" 的**机制**在这里：三者先被折成同一个单字符 `n`，
再由 `FunctionToken::new` 的**单字节**表映射成 `CONST(π)`。`FunctionToken::new` 里残留的
`"pi"` 那一臂（`0x3c0dea`）是**死臂**。

#### 30.1.2 算法：递归找"这一层的根"，**不是** shunting-yard

一个操作符栈都没有。`reorder_recursive(&mut self, start, end)`（`end` **闭区间**）：

1. 在 `[start, end]` 扫一遍，`depth` 数括号（18 `+1`、19 `-1`，`0x4f550c..0x4f551a`）；
2. 只在 **`depth` 最小**的那一层里挑 **`PRECEDENCE[tag]` 最大**的 token 当**根**
   （`0x4f5533`；数值大 = 结合松 = 根）；
3. 根 push 进 `output`（`0x4f5490`），然后：
   * 一元（`neg`=5，或 `sqrt..tan`=9..=16）：**不递归**，`start = root+1` 接着扫
     （`0x4f54ba` 的 `tag-9 < 8`）；
   * 原子/常量（`0/e0007` 掩码 = `{0,1,2,17,18,19}`，`0x4f5688`）：本层结束；
   * 二元：递归左 `[start, root-1]`（`0x4f56b3`）、递归右 `[root+1, end]`（`0x4f5721`），
     **`+`/`-` 缺哪边就补 `CONST 0`**（`0x4f56f7` / `0x4f575f` 的 `movq $0x11`）。

优先级表（`逐位`，`.rodata` `0x1c4838`；`0x1c4888` 是**同一张表的第二份拷贝**，
链接器没合并 —— 别误当成两张不同的表）：

```text
tag  0  1  2   3  4   5   6  7   8   9 10 11 12 13 14 15 16  17
值   2  2  2   7  7   4   6  6   4   3  3  3  3  3  3  3  3   2
     x  y  y'  +  -  neg  *  /   ^  sqrt log ln exp abs sin cos tan const
```

**`docs/03` 里"`^` 左结合"和"`+` 补 `CONST 0`"的机制现在都在这一节了**：

* **平级左结合**：两个二元相遇时判据是 `PRECEDENCE[new] >= PRECEDENCE[best]`
  （`0x4f55ca` `jae`，三档 `+/-`=7、`*/ /`=6、`^`=4）；`>=` ⇒ 后出现的当根 ⇒ 左结合。
* **一元右结合**：一元相遇走的是 `PRECEDENCE[best] < PRECEDENCE[new]`
  （`0x4f5565`/`0x4f557b` `jb`，**严格**小于）⇒ 先出现的当根 ⇒ `sin cos x` = `sin (cos x)`。
* **补洞只对 `+`/`-`**：`0x4f56c7` 的 `cmp $2,%edx`（`tag-3 < 2`）把范围锁死在 tag 3/4。

**新发现（本轮）**：词法器**不产 `neg`（tag 5）**。`FunctionToken::new("-")` 给的是**二元减 4**
（`逐位`，单字节跳转表）。⇒ `-x` 在词法层就是 `(- x)` 两个 token，靠"缺左操作数补 `CONST 0`"
变成 `(- 0 x)`。tag 5 的产生点**本轮未定位**（`未知`；候选 `FunctionRandomizer::mutate_fine_tune`）。

#### 30.1.3 结构体布局（`逐位`，两架构一致；`repr(Rust)` 被重排过）

```text
+0x00 input.cap   +0x08 input.ptr   +0x10 input.len
+0x18 output.cap  +0x20 output.ptr  +0x28 output.len
```

两个 `Vec` 都装**同一个类型**（16 字节 token）⇒ `PolishNotationConverter`
依赖 `function_token`，不是泛型。**没有 `new` 符号**：结构体在 `MathFunction::new` 里就地构造，
所以本模块符号数 = 1。

### 30.2 `function_token` —— **就是那个 16 字节 token，外加一张字符串表**

`FunctionToken` = `{ tag: u64, value: f64 }`，与 `docs/03` §2 的编码、本 crate 的
`Token`/`Function` **同一个东西**。本模块**唯一多出来的**是
`FunctionToken::new(&str)`（arm64 `0x3c0da0`，860 字节，**唯一调用者**
`MathFunction::new`+`0x28c`）—— 把正则切出的**一个片段**变成 token。

顺序（`逐位`）：

1. `parse::<f64>()` 成功 ⇒ `CONST`(17) + 值（`0x3c0dbc` 的 `float_parse`，失败分支 `0x3c0e3c`）；
2. 否则按**长度**分派：`cmp x20,#0x1/#0x2/#0x3/#0x4` 四张表。

| 片段 | tag | 备证 |
|---|---|---|
| `x` `y` | 0 / 1 | 单字节表（`'x'` → `0x3c0ed0`，`x0` 已清零 ⇒ 0） |
| `y'` | 2 | 16 位比较 `0x2779` |
| `+` `-` `*` `/` `^` | 3/4/6/7/8 | 单字节表 |
| `sqrt` | 9 | 32 位比较 `0x74727173` |
| `log` `abs` `sin` `cos` `tan` `exp` | 10/13/14/15/16/12 | 3 字节表 |
| `ln` | 11 | 16 位比较 `0x6e6c` |
| `n` | `CONST(π)` | `0x3c10cc`，`ldr d0` = `0x400921fb54442d18` = 3.141592653589793 |
| `e` | `CONST(e)` | `0x3c1058`，`0x4005bf0a8b145769` = 2.718281828459045 |
| `(` `)` | **18 / 19** | 单字节表 + `0x3c1074` 的 `cinc` |
| `ˆ`(U+02C6) | **8** | 16 位比较 `0x86cb`，**这条以前没有任何文档记过** |
| 其它 | **20** | `mov w0,#0x14`；本 crate 映射成 `None` |

tag **20 = 无效**。因为正则封闭，这一档**不可达**（`逐位` + 推理）。

### 30.3 `graphwar_crash_reporter` —— **崩溃日志的上报端**（**问题 b 的答案**）

**它是什么**：不是"崩溃时抓栈"，而是**把上次留下的崩溃日志发去收集服务器**的组件。
上报（`graphwar_crash_reporter`）与落盘（`backtracer`）**是两个模块**。

三个函数（`逐位` arm64 符号表；0 个 `Location` 锚，所以 x86 只能靠指纹，见 §30.8）：

| 符号 | arm64 | 字节 |
|---|---|---|
| `GraphwarCrashReporter::send_pending_report` | `0x4ca444` | 1136 |
| `GraphwarCrashReporter::resolve_default_address` | `0x4ca8b4` | 980 |
| `GraphwarCrashReporter::clear_report` | `0x4ca340` | 260 |

**上报到哪**（`逐位`）：`resolve_default_address` 里 **`mov w8,#0xa7dc` 出现两次** ⇒
**`CRASH_REPORT_PORT = 42972`**，与 `.rodata` 的 clap 帮助原文
`e.g. 127.0.0.1:42972` 对得上。三级回退：

1. CLI/环境 `--crash_report_address` / `CRASH_REPORT_ADDRESS`
   （clap 名 `crash_report_address`，命令行风格 `crash-report-address`）；
   失败 → `Failed to parse --crash_report_address: `
2. 否则 **全局服务器地址 + 42972**（帮助原文：*"…the game's global server address is
   reused with `CRASH_REPORT_PORT`"*）；全局地址来自
   `https://graphwar.com/graphwar_II_ip.txt`（39 字节字面量，`mov w1,#0x27`）；
   失败 → `Failed to parse global server address for crash reporting: `
3. 都没有 → `Crash report pending but no crash report server address is available.`，
   **什么都不发**，日志留在盘上等下次。

**什么时候装钩子 / 什么时候发**（`观测`，arm64 `bl` 图；这是**问题 b 后半**的答案）：

```text
<GraphwarRootNode as INode>::ready  +0x30  →  backtracer::Backtracer::install_panic_log_hook
<GraphwarRootNode as INode>::ready  +0x44  →  GraphwarCrashReporter::send_pending_report
```

⇒ **每次进主场景**（`ready` 是 Godot 虚函数，会被反复回调）装一次钩子、补发一次上次的日志，
**不是 `main()` 里一次性**。顺序是"先装钩子、再补发"。

**为什么 2024 版完全没有它**（`观测`）：2024-07-30 版 `0 符号 0 字符串`，2026 版有
**8 处** `src/graphwar_crash_reporter.rs:NN` 字面量（其它模块各 1 处，全库唯一）。
⇒ 是**后加的功能**，不是我们漏扫。8 处对应 8 个 panic 分支（`:54/62/65/68/82/87/92/100`）。
**这 8 条为什么没配上 `Location` 结构体**仍未解（`未知`）：它们是
`\x01[\xc0&] src/graphwar_crash_reporter.rs:82 - ` 这种**带长度前缀的拼装串**，
既不被 `lea` 直接引用、也不在 `.data.rel.ro` 的 `(&str,len)` 槽里
（`tools/loc_xrefs.py` 的 `refs_to` 与 `string_slots` 双向都查不到）。
**推断**：它们是 `format_args!` 的**提升常量**（`.rodata` 里的 `Arguments` 片段），
被 `.data.rel.ro` 里一个**非 `(&str,len)`** 形状的槽引用，而本仓库的槽扫描器只认后者。

**五个结局**各有可 grep 的字面量：`Crash report sent to ` / `Failed to send crash report to ` /
`Failed to connect to crash report server at ` / `Failed to clear crash log at ` / 上面那条 pending。

`clear_report`（`逐位`）= **把日志 truncate 成空**：`bl std::fs::write::inner` 之后紧跟
`chrono::Utc::now` + `format_inner`（写失败才拼错误串），时间格式 `%Y-%m-%d %H:%M:%S`。

### 30.4 `backtracer` —— **panic 钩子 → 崩溃日志落盘**（**问题 e 前半的答案**）

**它是什么**：`Backtracer::install_panic_log_hook`（arm64 `0x4becec`，876 字节）
+ 它的闭包（`0x4bf058`，960 字节，**钩子本体**）+ 两个静态量
`CRASH_LOG_PATH`（`0x8b4380`）与 `OS_NAME`（`0x8b43a0`）。
**它不上报**（那是 §30.3），只往 `user://crash_log.txt` 追加。

**有界否定**：arm64/x86 两侧，本模块的 `bl` 目标里**没有** `Backtrace::capture`。
⇒ **名字误导**：它不抓栈，只写日志。这是本轮之前没人验证过的一点。

`OS_NAME` 的**取值**未读出（`未知`）；用途**推断**为区分 Android/Linux/Windows 三份构建。

### 30.5 `graphwar_event_queue` —— **它不是 manager，是全游戏的发件箱**（**问题 c 的答案**）

**为什么不在 25 个 manager 的注册表里**（`观测`）：

1. **持有者不是容器，是 `GraphwarGameClient`**：
   `core::ptr::drop_in_place<RefCell<GraphwarEventQueue>>` 的调用者是
   `GraphwarGameClient::new +0xcdc`（落在 `new` 的恐慌清理段里 ⇒ 局部构造）与 `Rc::drop_slow`。
   其余所有 manager 拿的是 **`Weak<RefCell<GraphwarEventQueue>>`**。
   ⇒ **一个 `Rc` 主人 + 一堆 `Weak` 用户**，不是 `GraphwarModuleContainer` 的成员。
2. **形态不像 manager**：manager 的形态是 `new` + `subscribe_to_events_event_handler`
   + `send_all_info_events`；本模块只有 `add_event` 与 `send_buffer` 两个函数，
   **没有 `new`、没有订阅、没有 `send_all_info_events`**。
3. **它是所有人的出口**：`add_event`（arm64 `0x3d7a40`，276 字节）有 **200 余个**直接调用点，
   覆盖 `game_state_manager` / `function_manager` / `life_manager` / `team_manager` /
   `player_manager` / `soldier_manager` / `turn_manager` / `obstacle_manager` /
   `mode_manager` / `color_manager` / `graphics_manager` / `game_client` / `game_server` /
   `remote_event_handler` / `setup_control` 的每个 `set_*` / `handle_*` / `send_all_info_events`。

**它被谁持有**：`GraphwarGameClient`（`Rc<RefCell<>>`）。

**一条反常如实记录**：`send_buffer`（`0x3d793c`，260 字节）在 `bl` 图里**只有 1 个直接调用者**
——`GraphwarLevelGenerator::generate_level_custom +0x424`。三种解释（**不选定**，标 `推断`）：
被内联进各 `tick` / 走函数指针槽 / 真的只有关卡生成主动冲刷。
要定死需要 `tools/ptr_slots.py` 那类间接派发扫描，**本轮未做**。

### 30.6 `graphwar_mode_manager` —— **3 套模式，不是 4 套**（**问题 d 的答案**）

**它是不是"当前模式的唯一真相"？不是。**

* **它拿 3 套**：`AxisMode` / `FunctionMode` / `TimeMode`（arm64 符号表里三个 setter
  `set_axis_mode` `0x497dc8`、`set_time_mode` `0x497e44`、`set_function_mode` `0x497ec0`，
  各 124 字节，**函数体形状完全相同**：`borrow_mut` → 写 1 字节 → `add_event` ×1）。
  **`TurnMode` 不在这里** —— 在 `graphwar_turn_manager`
  （`GraphwarTurnManager::set_turn_mode`；`TurnMode` 序列化臂 x86 `0x479310`，`docs/07` §5.3.3）。
  ⇒ `docs/16-client.md` §16 的"四格模式"是**界面上的四个按钮**，不是同一个 Rust 结构体。
* **谁写它**（`观测`，`bl` 图）：`set_axis_mode` / `set_function_mode` ←
  **只有** `GraphwarRemoteEventHandler::handle_server_remote_event`；
  `set_time_mode` ← `GraphwarLevelManager::setup_level` + 同上。
  ⇒ 写入方是**服务器遥控**与关卡装载，**UI 按钮不直接写**。
* **为什么权威不在它**：客户端按钮走
  `GraphwarGameStateManager::request_{axis,function,turn,time}_mode_change`
  → 发 18/19/20/21 请求 → **服务器**回 45/46/47/48 Info 才落值（`docs/07` §4）。
  ⇒ 本模块是**服务器状态的副本**。
* 枚举取值**不重抄**：`docs/07` §5.2 已经穷举（`逐位`）。

`0x4b07a0` 上**三个** `__FieldVisitor::expecting` 是同一地址（14 字节的同一函数体）——
链接器把三个枚举的同形小函数合并了，别误读成"只有一个枚举"。

### 30.7 剩下 5 个：logger / draw_util / player_config / campaign_control / game_info

| 模块 | 一句话定性 | 证据 |
|---|---|---|
| `graphwar_logger` | **进程内调试日志 HTTP 端点**：`new` 里 `bl` 到 **`tiny_http::Server::new` ×2** + `to_socket_addrs` ×2；`log` 收的是 **`OsString`**（`RawVec<OsString>::grow_one`）；端口来自 clap `LOGGER_PORT`。`new` ← `RootNode::init`，`log` ← `RootNode::process_button_pressed` | `观测` |
| `graphwar_draw_util` | **不画东西**（上一轮的有界否定升级为正面定性）：`get_team_overview`（arm64 `0x4c3b38`）**4 次** `GraphwarModuleContainer::get_module` + 8 次 `Rc::drop_slow`，0 次 draw API ⇒ 它是"取队伍数据"的容器访问器；7 个调用者全是 `draw`/`process`/`update_*_label`/结算面板 | `观测` |
| `graphwar_player_config` | 8 个函数全定位；`new`→`load`（唯一）、三个 `set_*`→`save`；**`set_skin/face/hat` 没有直接调用者，因为它们是 gdext 导出方法**（`lock` 的调用者里有 `handle_fallible_call`）；`load` 用 `FileAccess::get_as_text` + `trim_matches`、`save` 用**一次** `store_string` ⇒ 文本存档、非 JSON | `逐位`/`观测` |
| `graphwar_campaign_control` | 4 个函数；存的是**字符串**、`get_level_as_int` 再用 `StrSearcher` 抠数字；`show_hide_campaign_levels` 用 `GraphwarNodes::get_node_by_name` + `<u32>::_fmt` 拼节点名按键显隐 | `逐位`/`观测` |
| `graphwar_game_info` | **`GameInfo` 的家在这里，不在 `graphwar_event.rs`**（serde impl 路径 `graphwarrust::graphwar_game_info::_::<impl Serialize for …::GameInfo>`）；0 个可归属函数是因为两个 impl 都在 `_::` 匿名模块下；两条进出口：`serialize` ← `GraphwarLanBroadcasterV6::process`，`visit_str` ← `serde_json::deserialize_struct` ⇒ **LAN 发现是 JSON**，且它是 `GameInfoEvent`（判别值 62）的新类型载荷 | `观测` + `逐位` |

`graphwar_player_config` 的字段偏移沿用 §26 的观测（名字 `+0x00`、皮肤 `+0x60`、脸 `+0x78`、
帽 `+0x90`、变更计数 `+0xa8`）。`+0x10..0x60` 那 80 字节是**解锁表**，
正好解释 8 个函数里为什么有 2 个专管 `lock`/`unlock`。

### 30.8 这 6 个 0 锚模块在 x86 目标上的落点（**证据等级不同，别一锅端**）

| 模块 | x86 候选 | 等级 | 依据 |
|---|---|---|---|
| `polish_notation_converter` | `0x4f5450` | **逐位** | `src/polish_notation_converter.rs:36:52` / `116:55` 两个 Location 锚 |
| `graphwar_draw_util` | `0x4be6e0` | **逐位** | `28:53 / 30:51 / 31:41 / 33:42` 四个锚 |
| `graphwar_campaign_control` | `0x418820` | **逐位** | `94:107` 锚（`get_level_as_int` 的 `StrSearcher` 分支） |
| `graphwar_logger` | `0x3b6c70`（208 字节，只 `bl` 到 memcpy ⇒ **`log`**）/ `0x44dd60`（1584 字节，`bl` 到一组 format 目标 ⇒ **`process` 或 `new`**，未进一步区分） | **逐位**（锚）| `12:37` / `44:64` 两个锚 |
| `graphwar_crash_reporter::send_pending_report` | `0x438220` | **观测** | 函数体直引 `.rodata 0x1c04fb` = `Crash report pending but no crash report server address is available.` |
| `graphwar_crash_reporter::clear_report` | `0x4c5fa0` | **观测** | 2 条指令直引 `user://crash_log.txt`（`0x1c2b89`） |
| `backtracer::install_panic_log_hook` | `0x4b8bd0` | **推断** | 1 条指令直引 `user://crash_log.txt`；体量 0x… 与 arm64 876 字节的比值落在 0.61–1.19 闸内 |
| `graphwar_crash_reporter::resolve_default_address` | `0x4c6520` | **推断** | 被 `0x438220` 直接调用 + `bl` 到 TCP 相关目标；体量 1152 vs arm64 980 = 1.176（闸内） |
| `graphwar_mode_manager` | 6 个锚落在 **21** 个函数上 | **逐位**（锚）| `132:26/133:26/134:26` 三锚同落 **`0x5185a0`**（该函数 `calls` 为空、只在指针表里被用）；其余锚（`79:26`/`97:26`/`113:26`）散落在 20 个函数上，是 **serde 泛型被内联**的结果，**不代表**模块有 20 个函数 |
| `graphwar_event_queue` / `function_token` / `graphwar_player_config` / `graphwar_game_info` | **未定位** | **未知** | 无锚、无独有字符串；要定位必须做 arm64↔x86 的跨架构指纹（被调者多重集 / 常量多重集），**本轮未做** |

`0x5185a0` 的 `calls` 表为空（`callers: NONE`）——它只在**函数指针表**里被用到，
这正是 `AGENTS.md` 说的"直接调用为零不代表没被使用"。

### 30.9 仍然未知（本轮**没**解决的）

| # | 未知 | 缺什么 |
|---|---|---|
| 1 | `neg`（tag 5）的产生点 | 词法器不产它（§30.1.2）；候选 `FunctionRandomizer::mutate_fine_tune` 未逐条读 |
| 2 | `crash_reporter` 那 8 条 `\x01[\xc0&]` 串为什么不是 `Location` 结构 | 需要解 `format_args!` 的提升常量（`Arguments`）布局，本仓库没有该扫描器 |
| 3 | `event_queue::send_buffer` 为什么只有 1 个直接调用者 | 需要 `tools/ptr_slots.py` 类间接派发扫描 |
| 4 | `backtracer::OS_NAME` 的取值 | 静态量内容未读出 |
| 5 | `logger` 的 HTTP 报文格式 | `process`（788 字节）里的 `tiny_http::Response` 构造未逐条解 |
| 6 | `player_config` 存档的**分隔符** | `load` 只知 `trim_matches` + 切分，分隔符字面量未定位 |
| 7 | `campaign_control::show_hide_campaign_levels` 的节点名模板 | 格式化串未逐字节还原 |
| 8 | `GameInfo` 9 个字段里哪 4 个走 `serialize_field`、哪些走 map | `serialize` 的 1264 字节未逐条对齐 |

### 30.10 复现命令

```text
ARM=.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so
X86=.reference/steam/2026-08-28/libgraphwarrust.so

# A. 0 锚模块的名字只在这里
nm -C -S --defined-only $ARM | grep -E "graphwarrust::(polish_notation_converter|backtracer|function_token|graphwar_event_queue|graphwar_crash_reporter|graphwar_mode_manager|graphwar_logger|graphwar_draw_util|graphwar_campaign_control|graphwar_player_config|graphwar_game_info)::"

# B. 谁装钩子 / 谁持有队列：arm64 bl 图（符号表给边界，capstone 扫 bl）
#    → <GraphwarRootNode as INode>::ready +0x30 / +0x44

# C. x86 锚
../.venv/bin/python tools/loc_xrefs.py loc src/polish_notation_converter.rs
../.venv/bin/python tools/loc_xrefs.py func 0x4f5450     # callers/callees + 两个锚

# D. 正则与优先级表（两份二进制逐字节相同）
#    正则 77 字节 @ x86 0x1c4347 / arm64 0x19dcb4
#    优先级 18 项 @ x86 0x1c4838（0x1c4888 是第二份拷贝）
#    bt 掩码 0x3fe27 / 0xffe27 / 0xe0007
```

### 30.11 本轮新增的文件（**`src/lib.rs` 待父 agent 接线**）

新增源码（每个都能指到原版模块，`AGENTS.md` §4 合规）：

```text
src/function_token.rs              原版 src/function_token.rs
src/polish_notation_converter.rs   原版 src/polish_notation_converter.rs
src/graphwar_mode_manager.rs       原版 src/graphwar_mode_manager.rs
src/graphwar_event_queue.rs        原版 src/graphwar_event_queue.rs
src/graphwar_crash_reporter.rs     原版 src/graphwar_crash_reporter.rs
src/backtracer.rs                  原版 src/backtracer.rs
src/graphwar_logger.rs             原版 src/graphwar_logger.rs
src/graphwar_draw_util.rs          原版 src/graphwar_draw_util.rs
src/graphwar_player_config.rs      原版 src/graphwar_player_config.rs
src/graphwar_campaign_control.rs   原版 src/graphwar_campaign_control.rs
src/graphwar_game_info.rs          原版 src/graphwar_game_info.rs
tests/misc_modules.rs              11 个模块的验收（54 条）
```

**需要加进 `src/lib.rs` 的 11 行**（顺序无所谓，放在现有 `pub mod` 块里）：

```text
pub mod function_token;
pub mod polish_notation_converter;
pub mod graphwar_mode_manager;
pub mod graphwar_event_queue;
pub mod graphwar_crash_reporter;
pub mod backtracer;
pub mod graphwar_logger;
pub mod graphwar_draw_util;
pub mod graphwar_player_config;
pub mod graphwar_campaign_control;
pub mod graphwar_game_info;
```

**在接线之前 `tests/misc_modules.rs` 也能跑**：它用
`#[path = "../src/<模块>.rs"]` 直接编译源码，并把依赖的 `crate::Function` / `crate::Token` /
`crate::events::Event` 在测试 crate 根上重导出。接线之后同一份断言仍然成立（会多跑一遍）。

`graphwar_game_info.rs` 只做 `pub use crate::events::{GameInfo, …}` —— **没有搬结构体**：
`docs/02-module-map.md` §17.4 的规矩是"一个文件混了多个原版模块而边界无证据时不猜不搬"，
本轮只登记归属依据（serde impl 路径），搬迁留给专门一轮。

## 31. 技术验证（spike）：`AxisDrawer` 真的注册成了 Godot 节点类

**一句话**：注册、三条实例化路径、属性读写、方法调用 + 复原逻辑执行，**全部通了**；
**但它还不会画** —— 虚方法表（`_draw`）一个都没接。这是"用重建源码跑起游戏"的第一个门槛，
本轮它过了；第二个门槛（虚方法表）是下一个任务。

> 本节所有结论都标了档位。`cargo test` / `client/run.sh selftest` 只算**回归保护**，
> 不是复原证据（`AGENTS.md` §0.2）。

### 31.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/_scaffold/axis_drawer_class.rs` | **新增 800 行**（代码 503 / 文档注释 119 / 空行 66 / 单测 112） | 类注册 + `axis_mode` 属性 + 4 个方法 + 两条「数线」观测点 + 8 条单测 |
| `src/_scaffold/gdext.rs` | +29 / −6 | `SCENE` 级别多注册一个类；`WANTED` 诊断表补上新接口；文件头「当前能力」按现状改写（原文写「还没有注册任何类」，已过时） |
| `src/_scaffold/gdext_class.rs` | +8 / −4 | `CreationInfo4::empty()` 与三个回调类型别名改 `pub`（给新文件复用），**无行为改动** |
| `src/_scaffold/mod.rs` | +1 | `pub mod axis_drawer_class;` |
| `client/tools/spike_class_check.gd` | **新增 139 行** | 验收脚本（23 条断言），headless 跑 |
| `client/tools/spike_axis_drawer.tscn` | **新增 12 行** | 最小场景：一个节点 `type="AxisDrawer"` + `axis_mode = 3` |

`client/` 下**只新增这两个验证文件**，没有改任何既有 `.gd` / `.tscn` / `run.sh`。

### 31.2 a) 能不能注册成功 —— **能**（观测）

```text
$ Godot_v4.7.2-stable_linux.x86_64 --headless --path client \
      --script res://tools/spike_class_check.gd
[gw2-rebuild] axis api resolve: class4=true method=true property=true construct=true \
    set_instance=true sn_new=true from_ctor=true to_ctor=true get_type=true
[gw2-rebuild] registered class AxisDrawer (parent Control)
[gw2-rebuild] registered AxisDrawer members: axis_mode (property), \
    set_axis_mode/get_axis_mode/axis_line_count/axis_line_count_formula (methods)
      before load: GW2Rebuild=true AxisDrawer=true
ok   ClassDB.class_exists("AxisDrawer")
ok   对照组：单例 GW2Rebuild 仍在（没有被这次改动弄坏）
ok   ClassDB.instantiate("AxisDrawer") 非空
ok   实例能当 Control 用（父类正确）
ok   inst.is_class("AxisDrawer") 为真
      engine facts: get_class()=AxisDrawer is_class(Control)=true
      property axis_mode: type=2 usage=6
ok   get_property_list() 里有 axis_mode
ok   axis_mode 默认值 = 0（NoAxis；我方选择，非二进制证据）
ok   set/get axis_mode=3 往返一致
ok   axis_mode=0 -> 枚举 0 条 / 公式 0 条（期望 0）
ok   axis_mode=1 -> 枚举 2 条 / 公式 2 条（期望 2）
ok   axis_mode=2 -> 枚举 20 条 / 公式 20 条（期望 20）
ok   axis_mode=3 -> 枚举 102 条 / 公式 102 条（期望 102）
ok   axis_mode=200（越界）→ 20 条（原版分支）
ok   载入 res://tools/spike_axis_drawer.tscn
ok   场景能实例化出节点
ok   场景节点是 Control
ok   场景节点 is_class(AxisDrawer)
ok   .tscn 里的 axis_mode=3 生效（102 条线）
      add_child 后立刻查: root.is_inside_tree=false node.is_inside_tree=false
ok   节点能挂进场景树（Node 生命周期可用）
ok   AxisDrawer.new() 非空
ok   AxisDrawer.new() 出来的是 Control
ok   新实例的 axis_mode=2 → 20 条
[gw2-rebuild] AxisDrawer 实例被引擎回收（free_instance）
SPIKE 23 passed, 0 failed        # 退出码 0
```

四条实例化/调用路径都过：`ClassDB.instantiate`、`.tscn` 场景加载器、GDScript 的
`AxisDrawer.new()`、以及 `free_instance`（引擎回收时会回调我们）。

**两个必须先知道的坑（本轮各踩一次）**：

1. **`--import` 是前置条件**。第一次跑时 `AxisDrawer.new()` 直接报
   `Parse Error: Identifier "AxisDrawer" not declared in the current scope` ——
   因为 `client/.godot/extension_list.cfg` 还不存在，`--script` 模式下扩展**根本没被加载**
   （`AGENTS.md` 里「改了但没生效」第 1 条）。`--import` 之后同一份脚本立刻通过。
2. **`--script` 模式的 `_initialize()` 跑在 `SceneTree` 根节点进树之前**：那一刻
   `add_child` 只记录父子关系，`is_inside_tree()` 仍是 false，要等一帧（脚本里
   `await process_frame` 之后再断言）。这是引擎时序，不是我们的 bug。

**两条引擎事实（观测，省得下一个人再猜）**：

* 对扩展类，`Object.get_class()` 返回**扩展类名**（`"AxisDrawer"`），不是原生父类（`"Control"`）；
  `is_class("Control")` 同时为真。
* `godot --headless --import` 期间**编辑器也会实例化一次**这个类
  （日志：`[gw2-rebuild] AxisDrawer 实例化成功 (object=0x430e56c0, notify_postinitialize=1)`）
  ⇒ `create_instance` 必须无副作用、可重入，注册必须幂等。

### 31.3 b) 从「纯逻辑」到「真节点」，一个类要补什么

按本轮实测（`axis_drawer_class.rs` 的 503 行代码）拆块，**没有一块是那个类专有的逻辑**，
全是 gdext 宏平时替我们生成的东西：

| # | 块 | 实测行数 | 具体是什么 |
|---|---|---|---|
| 1 | 接口表 | ~75 | 9 个 `get_proc_address` 名字 + 用 `usize` 存的表（`static OnceLock` 要 `Send + Sync`） |
| 2 | 类注册 | ~45 | `CreationInfo4`（**160 字节**，字段序有单测钉住）+ `create_instance`（`classdb_construct_object("Control")` 造真 Control，再 `object_set_instance` 挂 `Box<Instance>`）+ `free_instance` |
| 3 | 方法表 | ~110 | 4 个方法：每个要 `StringName`×(1+参数+返回)、`PropertyInfo` 数组、14 字段的 `ClassMethodInfo`，再加一个 `call_func` |
| 4 | 属性表 | ~75 | `classdb_register_extension_class_property` + `PropertyInfo`；**setter/getter 必须先注册成方法**（顺序不能反） |
| 5 | Variant 读写 | ~50 | 读参数走 `get_variant_to_type_constructor`、写返回走 `from_type_constructor`；**不许**自己猜 `Variant` 的布局 |
| 6 | 回调壳 | ~150 | `catch_unwind` + 空指针/参数个数检查（每个方法 20–40 行） |

⇒ **一个「1 属性 + 4 方法、不画东西」的类 ≈ 500 行**。其中约 300 行与具体类无关，
12 个类必须抽成公共模块（接口表 / Variant 读写 / 注册辅助），否则就是 6 000 行重复。

抽出去之后，每加一个类的**边际**成本（推断，按本轮结构外推）：

* 类 + 实例结构 + 构造/析构：**~60 行**
* 每个方法：**~20 行**（一个 `call_func` + 一条 `add_method`）
* 每个属性：**~10 行**
* 每个虚方法：**~40–80 行**（**未实测**，见 §31.5 风险 1）

按 `docs/18` §6.5 的 12 类 / 25 个暴露方法：**~1 200–1 800 行**能到「全部能实例化、属性可读写、
暴露方法可调用」，**不含**虚方法表。

### 31.4 c) 继续手写 `_scaffold/gdext*.rs`，还是换官方 gdext？

三条**观测**先摆出来：

1. **手写绑定够用**：类的注册、属性、方法、三条实例化路径，只用
   `gdextension_interface.h` 的 9 个函数就打通了，`Cargo.toml` 仍是**零依赖**，
   `cargo build --offline` 照旧可跑。
2. **原版就是 gdext 构建的**：`libgraphwarrust.so` 的 `.rodata` 里有
   `/home/lucas/.cargo/git/checkouts/gdext-067f4b88e7bd088f/477501f/godot-core/src/registry/class.rs`
   这类 `panic::Location` 字符串，配合 `docs/18` §6.5 的
   `Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2…)`。
   也就是说 `IControl` / `GodotClass` / `ImplementsGodotVirtual` / `ImplementsGodotApi`
   这些 trait impl **是 gdext 宏的产物**（`477501f` 是 gdext 的 git checkout 提交号前缀，
   具体 tag 未知——**未知**）。
3. **本机没有 gdext**：`~/.cargo/registry/cache/*/` 里没有 `godot`/`gdext` 任何版本
   （`ls ~/.cargo/registry/cache/*/ | grep -ci godot` = **0**），`CARGO_HOME` 下只有 target 目录。
   换 gdext 要联网，或把 ~50–100 MB 依赖 vendored 进仓库——`cargo build --offline` 那条
   「零依赖」性质当场消失。

**判断（推断，理由即上三条）**：

* **要「源码级还原」就必须走 gdext。** 项目的终点是同一套文件 / 模块 / trait impl
  （`AGENTS.md` §0）。原版每个类上面是 `#[derive(GodotClass)]` + `#[godot_api]`，
  手写绑定产出不了这些 impl 名字，**结构上只会离原文更远**：还原度这一项上，
  手写绑定是减法，不是替代品。
* **要「离线、可读、能跑」就先留手写。** 它已经把最难的第一步（真类 + 真场景实例化）打通，
  客户端现有全部断言都建立在它之上。
* **体积/依赖/可维护性三笔账**（推断）：
  * 依赖：手写 **0** vs gdext **vendored 数十 MB + 首次编译数分钟**；
  * 体积：手写 debug `.so` 18.5 MB（未 strip，含全部复原模块）；gdext 版本会更大（生成绑定），
    但 release 可接受；
  * 可维护性：手写 **~500 行/类且全是裸指针**，gdext 大约 **20–40 行/类**
    （`#[export]` 属性 + `impl IControl::draw`），差一个数量级。

**建议给下一轮的做法**：把这个 spike 当成「手写路线的可行性证明」，
**虚方法表（`_draw`）单独开一轮评估**；一旦决定要做 `_draw`，
就**同一轮把 gdext 引进来**（vendored + `--offline` 仍可验证），
而不是继续手写虚方法表——那是 12 个类 × 每个 40–80 行裸指针代码，且没有任何类型兜底。

### 31.5 d) 做 12 个类，最容易翻车的地方

按「翻车概率 × 修起来的代价」排序：

| # | 风险 | 为什么 | 能先做什么 |
|---|---|---|---|
| 1 | **虚方法表**（`get_virtual_func` + `get_virtual_call_data_func` + `call_virtual_with_data_func`） | `docs/02` §18.3 A 表里 **12 个类全部**带 `ImplementsGodotVirtual` ⇒ 每个类都有虚方法要接（`AxisDrawer` / `EntityDrawer` / `EffectDrawer` 是 `draw`，`GraphwarPlane` 还有 `on_resized`）。ptrcall 参数是裸指针（`Transform2D*` / `Rect2*` / `Color*` / `Ref<Texture2D>*`），签名错一个字节就是段错误，**没有类型检查兜底** | 只做一个 `AxisDrawer::draw`，用截图/selftest 证；成了再谈其余 |
| 2 | **原生父类选错** | 父类决定 `classdb_construct_object` 造什么。A 表的 trait impl 列已经把 11 个类钉住了（见下表），但 **`GraphwarAdSwitch` 没有 `I*` 列** ⇒ 它的父类只能算**未知**，得单独查 | 每个类先落一行「父类 + 证据」再动手 |
| 3 | **属性表对不上原版字段** | 属性名/类型/默认值错 ⇒ `.tscn` 里的值写进去没人读。本轮 `axis_mode` 只到「`self+0x18` 是 `u8` 判别值」这一层**推断**，默认值更是我方选择 | 用 `panic::Location` + 反编译逐字段恢复（`docs/16` §15/§16/§23 已有部分规格） |
| 4 | **StringName / Variant 生命周期与 ABI 布局** | `PropertyInfo.class_name` / `hint_string` 传 NULL **直接段错误**（`docs/116` §3.3 踩过）；`PropertyInfo` 在 x86-64 上是 **48 字节**（有两处 4 字节填充），本轮第一次就按 40 字节写错了，靠单测抓出来；每次 `create_instance` 建的 StringName 不缓存就是泄漏 | 新类必须带 ABI 布局单测（本轮的 `creation_info4_layout_matches_header` / `property_info_layout`） |
| 5 | **`NOTIFICATION_POSTINITIALIZE` 没发** | 官方 gdext 在 `notify_postinitialize=1` 时会补发，我们没做。对挂了脚本、或自己有 `notification_func` 的类可能有可观测差异 | 先在文档标**未知**；等有类挂脚本再处理 |
| 6 | **重复注册 / 编辑器也实例化** | `--import` 期间编辑器真的造了一次实例（§31.2）。注册必须幂等；`deinitialize` 要不要 `classdb_unregister_extension_class` 还没定（本轮没做） | 注册函数加「已注册就直接返回」的守卫；默认构造别碰资源 |
| 7 | **`RefCounted` 派生类的引用计数** | 12 个类里若父类落到 `RefCounted`，还得接 `reference_func` / `unreference_func`，否则可能双重释放 | 先定父类（风险 2），再决定要不要接引用回调 |
| 8 | **构建/加载的三个静默坑** | ① 新增 `.gd`/`.tscn` 后不 `--import` ⇒ 扩展**静默不加载**；② `cargo build` 后不覆盖 `client/libgw2_rebuild.so` ⇒ 跑的是**旧内核**；③ 用 `CARGO_TARGET_DIR` 时 `examples/serve_match` 也要在那里，否则 selftest 的联机段会 skip | 验收命令写进 §31.6，别靠记忆 |

**12 个类的父类（`docs/02` §18.3 A 表的 `gdext trait impl` 列，观测）**：

| 类 | 父类 | 依据 |
|---|---|---|
| `AxisDrawer` / `EffectDrawer` / `EntityDrawer` / `GraphwarLongPressAnimation` / `GraphwarPlane` / `GraphwarSetupControl` / `PlayerShowcase` | `Control` | 都带 `IControl` |
| `GraphwarButton` | `Button` | `IButton` |
| `GraphwarFunctionInput` | `LineEdit` | `ILineEdit` |
| `GraphwarLanVBox` | `VBoxContainer` | `IVBoxContainer` |
| `GraphwarRootNode` | `Node` | `INode` |
| `GraphwarAdSwitch` | **未知** | 只有 `ImplementsGodotApi` / `ImplementsGodotVirtual`，没有 `I*` 列 |

### 31.6 怎么复跑（照抄就能过）

```text
# 1) 编译内核并覆盖进 client/（不覆盖就是在跑旧 .so）
export CARGO_HOME=/home/user/MyProject/gw2/.cargo-home
export CARGO_TARGET_DIR=$PWD/../.cargo-home/target
cargo build --offline
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so

# 2) 首次（以及每次新增 .gd / .tscn 之后）必须先扫一遍，
#    否则 .godot/extension_list.cfg 里没有扩展，--script 模式下扩展不会被加载
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path client --import

# 3) 验收：23 条断言，退出码 0
/home/user/MyProject/gw2/tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path client \
    --script res://tools/spike_class_check.gd
```

`client/run.sh` 没有为它加子命令（本轮不允许改 `run.sh`）；上面三条就是完整步骤。

### 31.7 本轮**没做**的（未做 / 未知，别当成做完了）

* **虚方法表**：类**不画任何东西**。`AxisDrawer::draw` 的三次原语
  （`draw_set_transform` / `draw_rect` / `draw_line`×N）一条都没发。
* `NOTIFICATION_POSTINITIALIZE`、`classdb_unregister_extension_class`、信号注册、
  `RefCounted` 的 `reference/unreference`：全没做。
* `axis_mode` 的**默认值**（我方取 0）与**原版字段名**：**推断 / 我方选择**，不是二进制证据。
* 另外 11 个类：一个都还没注册。

### 31.8 回归（回归保护，不是复原证据）

```text
cargo build --offline        → 绿（3 条 warning 是既有模块的，与本节无关）
cargo test  --offline        → lib 457 passed / 0 failed
                               总计 1448 passed / 0 failed
bash client/run.sh selftest  → SELFTEST 694 passed, 0 failed
```

基线取自 `9ee40e46`：lib **449**（任务书里写的 442 是更早的 master）、总计 **1440**。
本轮新增的 8 条单测全部在新文件里，**既有断言一条都没改、没删**。

---

## 32. 最后 8 个可归属函数：5 个**非 `graphwar_` 前缀**的小模块（本轮新增）

> 这一节收掉进度尺上最后一批"一直被漏在任务书之外"的东西：
> `function_drawer` / `function_history_recorder` / `function_button_manager` /
> `button_manager` / `obstacle_drawer`。五个模块合计 **8 个**可归属函数
> （`1 + 1 + 2 + 2 + 2`），口径与 `docs/coverage-matrix.tsv` 一致。
>
> **地址口径**：`x86` 一律 **ELF 虚地址**（`objdump` 口径），Ghidra/REA 地址 = 本值 + `0x100000`；
> `arm64` 是 **2.0.60 的 Android 构建**，**只用于定名**，绝不当 x86 地址用。
> 唯一的例外是 `procedure_callers`/`procedure_callees` 返回的地址（Ghidra 口径），本节逐处标注。
> 落码：`src/{function_drawer,function_history_recorder,function_button_manager,button_manager,obstacle_drawer}.rs`；
> 验收：`tests/last_five.rs`（**回归保护，不是复原证据**）。

### 32.0 一页结论

| 模块 | 函数（arm64 2.0.60 真名） | x86 落地 | 档位 |
|---|---|---|---|
| `function_drawer` | `remove_function` / `update_function` / `update_transform` / `update_function_fade` / `draw`（5） | **4/5**（只有 `update_function` 带 Location；`draw` 未落地） | 观测（1 个唯一锚）+ 观测（3 个被调者多重集）+ 未知（1 个） |
| `function_history_recorder` | `new` / `handle_function_fire_event` / 订阅闭包（3） | **1/3** | 观测 |
| `function_button_manager` | `find_buttons` / `new` / `new::{{closure}}`（3） | **2/3** | 观测（Location + 被调者 8/8 + 首尾相接） |
| `button_manager` | `find_buttons` / `new`（arm64 无符号）/ `new::{{closure}}`（3） | **2/3**（角色口径 2，见 36.2 的孪生说明） | 观测 1 + 未知 1 |
| `obstacle_drawer` | `update`（1）+ 一条 Location 落在 `GraphwarPlane::init` 体内 | **2/2** | 观测 |

四条"任务问的答案"各占一小节：**a** → §32.3，**b** → §32.4，**c** → §32.5，**d** → §32.6。

### 32.1 名字从哪来：**Android 2.0.60 优先，并与 2024-07-30 对账**

两份未 strip 的构建都带完整 `.symtab`。按 `docs/02` §18 的补注，**优先 Android**（与逆向目标同版本）：

```text
for so in android-2.0.60/libgraphwarrust.so 2024-07-30/libgraphwarrust.so; do
  readelf -sW .reference/steam/$so | awk '$4=="FUNC"' \
    | c++filt --format rust | grep -E 'graphwarrust::(function_drawer|function_history_recorder|
        function_button_manager|button_manager|obstacle_drawer)::'
done
```

本批 5 个模块在 **2.0.60（arm64）** 里的源级符号数：

| 模块 | 源级方法 | 另有 `drop_in_place<…>` |
|---|---|---|
| `function_drawer` | **5** | 2（各 104 B） |
| `function_history_recorder` | **3** | 5（含 `Vec<FunctionData>` 124 B ×2） |
| `function_button_manager` | **3** | 2 |
| `button_manager` | **2**（`new` 被内联，只剩 `find_buttons` + 闭包） | 3 |
| `obstacle_drawer` | **1** | 3（各 268 B） |

**两版不一致只有一处，而且方向明确**：

| 名字 | 2024-07-30（x86，未 strip） | 2.0.60（arm64，未 strip） | 结论 |
|---|---|---|---|
| `FunctionDrawer::clear_functions` | **有** | **无** | 以 2.0.60 为准 ⇒ 2.0.60 里不存在（连内联落点都没观测到）→ **未知** |
| `FunctionDrawer::set_z_index` | **有** | **无** | 以 2.0.60 为准；但 `CanvasItem::set_z_index` 的调用**还在**，在 `update_function` 的"建节点"分支里（arm64 `0x390d04`）⇒ 最可能**被内联** → **推断** |

> 口径提醒：`docs/02` §18 那张"权威名字清单"的标题写的是"2024-07-30 版"，
> 那是因为当时手上只有那份**同架构**文件。**"与 2.0.60 是否一致"这个问题，Android 版更权威。**
> 本节按这条规矩执行，并把上面两行差异记在这里。

### 32.2 八个函数的地址表（全部 **观测**）

| 模块 | 方法 | arm64 2.0.60 | B | x86 vaddr | B | 比值 | 怎么配上的 |
|---|---|---|---|---|---|---|---|
| `function_drawer` | `remove_function` | `0x390a2c` | 276 | `0x498b70` | 256 | 0.93 | 被调者：`queue_free` + `RawTable::remove_entry` |
| | `update_function` | `0x390b40` | 848 | **`0x498c70`** | 992 | 1.17 | **Location 唯一锚** + 6 个被调者逐个相同 |
| | `update_transform` | `0x390e90` | 528 | `0x499050` | 608 | 1.15 | 被调者：`set_width`×2 / `set_scale`×2 / `set_joint_mode`×2 |
| | `update_function_fade` | `0x3910a0` | 276 | `0x4992b0` | 272 | 0.99 | 被调者：`set_default_color`×1 |
| | `draw` | `0x3911b4` | 560 | **未知** | — | — | x86 未定位（有界否定见 §32.3 末） |
| `function_history_recorder` | `handle_function_fire_event` | `0x49d71c` | 596 | **`0x3f0e40`** | 483 | 0.81 | 3 个 Location 键 + `String::clone`/`grow_one` |
| `function_button_manager` | `new` | `0x42464c` | 924 | **`0x3f0270`** | 1088 | 1.18 | 2 个 Location 键 + 被调者 |
| | `new::{{closure}}` | `0x4249e8` | 2040 | **`0x3f06b0`** | 1936 | 0.95 | 8 个 `GraphwarFunctionInput` 方法逐个相同 |
| `button_manager` | `new`（内联体） | 无符号 | — | **`0x3a2e10`** | 9936 | 不适用 | 2 个 Location 键 + **唯一**调用者 `0x52fdf0`（Ghidra 口径） |
| `obstacle_drawer` | `update` | `0x422590` | 4688 | **`0x3ee190`** | 5216 | 1.11 | 6 个 Location 键 + `create_from_data`×2 / `rustc_entry`×3 |
| | （Location 落在）`GraphwarPlane::init` | `0x4bd73c` | 1336 | **`0x4b7380`** | 1776 | — | `obstacle_drawer.rs:37:97` 双向唯一（§23.2） |

比值闸（`0.61–1.19`）**只对"同源方法"用**：`GraphwarPlane::init` 那条是**跨文件**的行归属
（同一函数体里带着两个源文件的 Location），不参与比值判定；`button_manager::new` 的 arm64
对照物被内联，也没有比值。

**7 个 `panic::Location` 槽（`function_drawer` 1 / `function_history_recorder` 3 /
`function_button_manager` 3 / `button_manager` 3 / `obstacle_drawer` 7）逐个落在哪个函数，
写在各自 `src/*.rs` 的 `LOCATIONS` 常量里**，一列 `(line, col, 结构体地址, 引用它的函数)`。

**`button_manager` 的"2 个"与重扫出的"3 个引用者"**：`:29:26` 那个 Location（槽 `0x951660`）
被**两个** x86 函数共用（`0x3aae60` 64 B / `0x3b70b0` 144 B），arm64 侧对应
`new::{{closure}}`（96 B）与 `drop_in_place<…{{closure}}>`（28 B）—— **哪一个是本体没定**
（64/144 都不像 28，也都不像 96）。所以：**角色口径 2 个**（`new` + 闭包），
**地址只钉 `0x3a2e10` 一个**，孪生关系记在 `tests/last_five.rs::eight_functions_and_the_twins` 里。

### 32.3 **a) `function_drawer` 是怎么把曲线画上屏幕的**

**答**：它自己不画 —— 它**持有 `Line2D` 子节点**，每个玩家 id 一条；`GraphwarPlane::draw`
每帧把"本 tick 生效的函数"喂给它，它**只把新增的那几个点** `add_point` 进去。

1. **子节点**：`GraphwarPlane::init`（`0x4b7380`）建 **3 个** `FunctionDrawer` 并 `add_child`
   （arm64 侧 `Gd::default_instance`×3 / `ExAddChild::new`×3 / `Node::add_child_full`×3；
   x86 侧 `0x4db030`×3 —— §23.3）。**这 3 个各管什么，未知**（没有证据能指到）。
   `FunctionDrawer` 自己是 `HashMap<i32, (Gd<Line2D>, usize)>`（`self+0x20`，hashbrown）：
   key = player id（i32），value 第二项是**已写入点数**。
2. **`add_point` 的调用时机**：只在 `update_function`（`0x498c70`）里，而它由
   `GraphwarPlane::draw`（`0x4b5df0`）每帧调一次 —— 同帧还取了
   `graphwar_tick_handler::get_fractional_tick`（`0x519530`）与
   `graphwar_function_manager::get_active_functions_on_tick`（`0x456df0`）（§23.4）。
   ⇒ **曲线随 tick 生长**，是"开火后逐帧画出弹道"的形态（§23.4 那条**推断**在本轮得到加强）。
3. **点数**（§23.4 缺的那条证据，本轮补上）：`0x498d5a` 先比 `new_len` 与"已写入数"，
   **不大于就直接 return**；`0x498f51`/`0x498f60` 各 `<<4` ⇒ 步长 **16 字节/点**；
   `0x498f90`/`0x498f95` 读 `[p]` 与 `[p+8]` 两个 **f64**（`x`、`y`），`cvtsd2ss` 后
   调 `0x498fab` 的 `Line2D::add_point(Vector2, -1)`（`esi = 0xffffffff` ⇒ 追加到末尾）。
   ⇒ **每次调用只 `add_point` 出 `new_len - written` 个点，是增量追加，不是整条重画**；
   函数变短（`new_len <= written`）时**一个点都不加**，曲线不会缩 —— 要缩得靠
   `remove_function` 重建（`0x498b70`，体内 `queue_free`）。
4. `color` 只在**首次建节点**时用一次（`r9d` 的 `&0xff` / `>>8` / `>>0x10` ÷ 255.0 →
   `Color(r,g,b,1)`），随后 `set_default_color`（`0x9bbf90`）+ `set_z_index`。
   `update_function_fade`（`0x4992b0`）再单独改颜色（淡出）。
5. **曲线不是图元**：全库 `add_point` 只有一个调用点（`0x498fab`），而 7 个绘画图元的
   调用点里没有一个是本模块（§23.4 的有界穷举）。

**`draw` 为什么没落地（有界否定）**：arm64 `FunctionDrawer::draw`（560 B）被调者是
`Line2D::get_point_count` + `Line2D::get_point_position` + `CanvasItem::draw_circle_full`
（各 1 处，在循环里）。x86 侧 `draw_circle` 的 GOT 槽 `0x9bbfc0` 全 `.text` 只有 **9** 个
调用点：**5 个在 `0x4993c0`**（那个函数还引用 `"Write a function of x to hit the target"`，
更像函数输入框的绘制）、**4 个在 `EffectDrawer::draw`**（§23.4）。
**没有一个**是"循环里 1 次"的形态 ⇒ x86 侧这个 `draw` 要么被内联进 `GraphwarPlane::draw`、
要么不存在。**结论：未知，不强行落地。**

### 32.4 **b) `function_history_recorder` 的容量与去重**

| 问题 | 结论 | 档位 |
|---|---|---|
| 存多少条 | **没有上限，也没有预留容量**：`new`（arm64 `0x49d9a0`）只 `__rust_alloc(0x48,8)` 造 `RcInner` + `__rust_alloc(8,8)` 造 `WeakCaller`，**没有任何 `0x20*N` 的缓冲区**；推送走 `Vec::push` 语义（x86 侧 `len==cap` 时调 grow，arm64 侧直接 `RawVec::grow_one`）⇒ 从 0 涨、按需翻倍 | **观测** |
| 重复的函数会去重吗 | **不会**。推送路径上**没有**任何"拿新文本和已有条目比"的循环；唯一带键的查找是 `HashMap<i32,_>`（键 = **player id**，不是文本），且只用来取那个 `u32`。⇒ 同一玩家连发同一函数，历史里就是两条 | **观测** |
| 上下键怎么翻 | **本模块里没有**（3 个函数：`new` 只建对象+订阅、`handle_…` 只 push、闭包只 `WeakCaller::call`）。全库也**没有** `Vec<FunctionData>` 的读取点符号（只有 `drop_in_place`）。两份二进制的 `.rodata` 里 **`"ui_up"` / `"ui_down"` 各 0 处**；`GraphwarFunctionInput::process`（arm64 `0x4547a0`，5888 B）与 `gui_input`（`0x455ea0`，1404 B）里**一个箭头键码立即数（`0x4000xx`）都没有** | **未知** |

一条历史记录的布局（**逐位**，x86 `0x3f0e40` 的推送块）：

```text
FunctionData { text: String (24 B) , color: u32 (+0x18) , player: i32 (+0x1c) }  = 32 B
事件闸：*event == 0x8000_0000_0000_0023（即 -0x7fffffffffffffdd）；event+0x20 == i64::MIN ⇒ 直接 return
```

⇒ **诚实的说法**：这个历史**只写不读**（至少在本版观测不到读者）。缺的证据是一次
真机按键 + `LineEdit.text` 的变化；候选落点只剩 `GraphwarFunctionInput::{process,gui_input,force_update_cb}`
（都没读完）。**不把"上下键翻历史"写成结论。**

### 32.5 **c) `function_button_manager` 是干什么的**

**答**：它是**函数输入键盘的按键分发器**，不是"sin/sqrt 快捷插入"那么窄。

* `find_buttons`（arm64 `0x424494`，440 B）：`Node::get_children_full` → `AnyArray::len` →
  `FromGodot::from_variant` → `Gd::owned_cast`，**递归**（体内 `bl` 到自己，`0x4245cc`）；
  非按钮节点**不终止递归**。
* `new`（`0x42464c` ↔ x86 `0x3f0270`）：收按钮，然后给每个按钮挂一个
  **`Box<dyn FnMut(&str)>`**（drop glue 符号见 `src/function_button_manager.rs`）。
* `new::{{closure}}`（`0x4249e8` ↔ x86 `0x3f06b0`）：**整个闭包只调这 8 个**
  `GraphwarFunctionInput` 方法 —— `clear` / `insert_text_at_caret` /
  `delete_token_at_caret` / `delete_token_after_caret` /
  `move_caret_left` / `move_caret_right` / `move_caret_home` / `move_caret_end`
  （另有 3 处 `assert_failed::<usize,usize>`）。
* **按钮文本从哪来**：**不来自本模块的字符串字面量**。闭包入参是 `&str`（按钮把自己的字符串
  交进来）；x86 `0x3f06b0` 引用的字符串**总共 11 条**，逐个列出后**没有一条**是函数名
  （没有 `sin`/`sqrt`/`log`/`pi`），只有 `called \`Result::unwrap()\`…` 与类名
  `graphwarrust::graphwar_function_input::GraphwarFunctionInput`（cast 用）。
  ⇒ **按钮的文本/名字来自场景**（`Graphwar.pck` 里的 `.tscn` 节点），Rust 侧只按文本分发。

> ⚠️ **反例（本轮踩到并记下来）**：Ghidra 把 x86 `0x3f0792` / `0x3b1622` 引用的两段字节认成
> 字符串 `"e_view_functions"`（`0x19cd70`）与 `"toggle_view_func"`（`0x19ceef`），
> 看上去像"信号名/按钮名"。逐指令看，那两条都是 **`pcmpeqb xmm1, [rip-…]`** —— 用的是
> **16 字节 SIMD 常量**，只是恰好落在字符串池里；`0x19ceef` 处真实字节是
> `b"toggle_view_funcy"`（17 字节，第 18 字节 `\0`）。
> ⇒ **"谁引用了某个字符串"不能当"代码里写着这个名字"用**；`strings` × `xref` 组合会骗人。

### 32.6 **d) `obstacle_drawer` 与 `graphwar_plane` 的脏矩形怎么配合**

**答**：**脏矩形由 `graphwar_plane` 决定并持有，`obstacle_drawer` 只是消费者** ——
它连一个矩形参数都没有，只 `Control::get_size()` 拿整张平面的尺寸。

* `ObstacleDrawer::update`（`0x3ee190` ↔ arm64 `0x422590`）**不用任何绘图图元**：把障碍
  **光栅化进一张 CPU 侧 `Image`**（`PackedArray<u8>::op_push_back` / `ffi_index_mut` 填字节），
  再 `ImageTexture::create_from_image`（字面量 `0x1bfc0f`）+ `ImageTexture::update`
  （`0x1bfc20`），`Sprite2D::set_texture`（`0x1bfc8c`，cast 目标字面量 `Sprite2D` `0x19fb58`），
  节点是自己 `ExAddChild::new` + `add_child_full` 建的，按 `Control::get_size()` 设
  `Node2D::set_position` / `set_scale`。障碍数据来自 `GraphwarObstacleManager` 的
  `Mutex<GraphwarObstacleData>`（体内有 `MutexGuard<GraphwarObstacleData>` 与
  `PoisonError<…>` 的 drop glue）。
* **谁决定重画哪块**：`GraphwarPlane::obstacle_partial_update_event`（x86 `0x4be3e0`）
  收到障碍事件后把矩形塞进平面自己的字段（§23.3 / §23.7 的落码：`PixelRect` /
  `obstacle_rect`）。本模块 6 个 Location（`:51 :62 :92 :113 :120 :128`）**没有一个**在做
  矩形运算 —— 全是 `unwrap`/断言类失败路径。
* **每帧调几次**：`GraphwarPlane::process`（`0x4b83f0`）每帧调 `update` **2 次**（§23.3，观测）。
  **为什么是 2 次 —— 未知**；每次体内 `Image::create_from_data` 出现 2 次
  （arm64 `0x422780` / `0x422d1c`），`hashbrown::rustc_entry` 出现 3 次（按 key 缓存节点）。
* ⇒ **"只重画那一块"这个更强的说法，本轮没有逐条读到 —— 标未知**。
  **缺的证据**：对 `update` 的两次入参做运行时抓取（谁能逐帧量 `Sprite2D.texture` 的尺寸就能定案）。

### 32.7 本轮**没**解决的（照实写）

1. `FunctionDrawer::draw` 的 x86 落点（有界否定见 §32.3 末）—— **未知**。
2. `FunctionDrawer` 那 3 个子节点各管什么 —— **未知**（只确证"建了 3 个"）。
3. `FunctionHistoryRecorder` 的读者 / 上下键 —— **未知**（§32.4）。
4. `FunctionButtonManager` 闭包里那 3 处 `assert_failed::<usize,usize>` 比较的是什么 ——
   **推断**（token 下标与文本偏移对齐），缺逐条读。
5. `ButtonManager::new` 里除 `FunctionButtonManager` 外的 53 个被调者：其中 9 个集中在
   `0x4a0810–0x4ae320` 这一簇，**推断**是兄弟管理器，但**一个 Location 锚都没有** ⇒ 不定名。
   `button_manager` 的 `:29:26` 孪生（`0x3aae60` / `0x3b70b0`）哪个是闭包本体 —— **未知**。
6. `ObstacleDrawer::update` 每帧两次的语义差别 —— **未知**（§32.6）。
7. `function_drawer` 的 `clear_functions`（2024 版有、2.0.60 无）连内联落点都没观测到 —— **未知**。

### 32.8 复现（不依赖本节结论，任何人可重跑）

```text
# A. 两侧符号（真名的唯一来源；**Android 优先**）
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so | awk '$4=="FUNC"' \
  | c++filt --format rust | grep -E 'graphwarrust::(function_drawer|button_manager|obstacle_drawer)::'

# B. 五个模块的 Location 全扫（只读 .so，不需要 Ghidra）
../.venv/bin/python tools/loc_xrefs.py loc src/function_drawer.rs
../.venv/bin/python tools/loc_xrefs.py loc src/obstacle_drawer.rs
#    → 输出 (file:line:col, 槽地址, 引用它的函数)；本节的 LOCATIONS 表就是它的副本

# C. a 问的那 6 条指令（增量追加 + 唯一 add_point 调用点）
objdump -d --no-show-raw-insn -M intel --start-address=0x498f44 --stop-address=0x498fb0 \
        .reference/steam/2026-08-28/libgraphwarrust.so
#    0x498f51/0x498f60 的 shl 0x4 = 步长 16；0x498f90/0x498f95 的 movsd = 两个 f64；
#    0x498fa6 的 esi=0xffffffff；0x498fab 的 call [0x9bbf98] = Line2D::add_point

# D. 方法名 → GOT 槽（判断"这个函数调了哪个 Godot 方法"）
#    字面量 set_default_color 0x29f3c5 → 包装 0x8b7a70 → 槽 0x9bbf90
#    set_position 0x29db01 → 0x891e00 → 0x9bc090；set_scale 0x29daa8 → 0x892b50 → 0x9bb238
#    set_joint_mode 0x29f3a8 → 0x8b74f0 → 0x9bbfa8；set_width 0x29f346 → 0x8b7fe0 → 0x9bbfa0
```

### 32.9 落码清单（**`src/lib.rs` 待父 agent 接线**）

```text
src/function_drawer.rs            原版 src/function_drawer.rs
src/function_history_recorder.rs  原版 src/function_history_recorder.rs
src/function_button_manager.rs    原版 src/function_button_manager.rs
src/button_manager.rs             原版 src/button_manager.rs
src/obstacle_drawer.rs            原版 src/obstacle_drawer.rs
tests/last_five.rs                5 个模块的验收
```

**需要加进 `src/lib.rs` 的 5 行**（顺序无所谓）：

```text
pub mod function_drawer;
pub mod function_history_recorder;
pub mod function_button_manager;
pub mod button_manager;
pub mod obstacle_drawer;
```

**接线之前 `tests/last_five.rs` 也能跑**：它用 `#[path = "../src/<模块>.rs"]` 直接编译这 5 个源文件
（五个模块彼此**零依赖**，只用了 `std`，所以不需要在测试 crate 根上重导出任何东西）。
接线之后同一份断言仍然成立（会多跑一遍）。

## 33. 实体特效登记表：`AshEffect` / `DeathEffect` / `SparkEffect` **在出生时定死**（本轮新增：`docs/141` 那 19 例的机制解）

> 任务：`docs/141` §4.4 记着「37 个失败的多发批里，19 个是死亡块**整块**是 `AshEffect`」，
> 并把它归成"我方没有 Ash 发射器"。§29 已经把分界推到"实体出生登记"，
> 但**士兵到底登记什么**当时是空的（§29.7 列为未知）。
> 本轮把 6 个写入点逐个解到指令，并落成 `src/graphwar_entity_effect_manager.rs` +
> `src/game.rs` 的真实行为；同时**改正 §29.3 的一处错**（见 §33.6）。

### 33.1 一页结论

| # | 结论 | 等级 |
|---|---|---|
| 1 | `entity_effect_map` 全库只有 **6 个写入点**，分布在 **4 个**宿主函数里（`add_entity_effect` 本构建**全部内联**，没有独立函数体） | **逐位** |
| 2 | **士兵出生**（`add_soldier_to_player`，arm64 `0x4aaab4`）登记 `DeathEffect`(1)，**但**：皮肤的 `graphics_str` 以 **`bot_`** 开头时登记 **`AshEffect`(3)** | **逐位** |
| 3 | **靶子出生**（`add_target`，`0x4a5704`）恒登记 **`SparkEffect`(2)**（`mov w2, #0x2`），并把皮肤写成 `"target"` | **逐位** |
| 4 | **迟到的皮肤**（`handle_graphics_info`，`0x4aa304`）收到 `SkinGraphicsInfo{bot_*}` 时，会把受影响实体**改写**成 `AshEffect`；非 `bot_` 前缀**什么都不做** | **逐位** |
| 5 | 与**队伍无关**：判定里没有 `get_team` 的结果参与（`0x4aac40` 那次 `get_team` 只用来写 `flip`/颜色） | **逐位** |
| 6 | 死亡结算**照抄表**、查不到**一条都不发**；`effect_id` 仍是**会话级自增计数器**（**改正 §29.3**，见 §33.6） | **逐位** |
| 7 | 数据侧独立复核：全语料 953 条死亡 `EffectInfo` 里 899 条按坐标唯一对上实体，**536/536 `DeathEffect`、363/363 `AshEffect`、54/54 `SparkEffect` 全等于该实体的登记值** | **观测** |
| 8 | `src/game.rs` 已按此改：`Match::new`/`add_target` 做出生登记、`note_skin_graphics` 做迟到改写、`execute_results` 查表（查不到不发） | **逐位** |
| 9 | **对拍口径没变**：`replaydiff::Predictor` 不喂 `SkinGraphicsInfo`，所以 `docs/141` 的 401 份数字**一位不动**（§33.5 有实测与原因） | **观测** |

### 33.2 6 个写入点（这张表的全部写者）

`entity_effect_map` 是 `HashMap<u32 /*entity_id*/, EffectType /*u8*/>`（桶距 8，唯一）。
它的 `insert` monomorphization 是 `hashbrown::map::HashMap::insert::hd46922447724ee7`
（arm64 **`0x466f14`**；`target_manager` 那张值类型是 `()` 的表走 `he15425207191a51e`，
两者**不是**同一份代码）。扫全 `.text` 的 `bl 0x466f14` 只有 6 条：

| # | 宿主函数（arm64） | 调用点 | 登记的类型 | 触发时机 |
|---|---|---|---|---|
| 1 | `GraphwarGameStateManager::add_soldier_to_player` `0x4aaab4` | `0x4ab0d0` | `1 DeathEffect`（皮肤名**为空/取不到**） | 士兵出生 |
| 2 | 同上 | `0x4ab154` | `1 DeathEffect`（皮肤名不以 `bot_` 开头，或长度 ≤ 3） | 士兵出生 |
| 3 | 同上 | `0x4ab2a4` | `3 AshEffect`（皮肤名以 `bot_` 开头） | 士兵出生 |
| 4 | `GraphwarGameStateManager::add_target` `0x4a5704` | `0x4a58d4` | `2 SparkEffect` | 靶子出生 |
| 5 | `GraphwarGameStateManager::handle_graphics_info` `0x4aa304` | `0x4aa464` | `3 AshEffect`（对一批 id 循环） | 收到 `SkinGraphicsInfo{bot_*}` |
| 6 | `GraphwarRemoteEventHandler::handle_server_remote_event` `0x3e6df0` | `0x3e8554` | 照抄收到的 `EntityEffectInfo` | 收到远端登记 |

每一处都是同一段内联体：`mov w8, #<ty>` + `strb w8,[sp,#<payload+0x0c>]` 组
`EntityEffectInfo`（判别值 56，`mov x8,#0x38` / `movk x8,#0x8000,lsl #48`）→
`GraphwarEventQueue::add_event`（`0x3d7a40`）→ `HashMap::insert(key=entity_id, value=ty)`。
**事件载荷里的类型字节与 `insert` 的值是两个独立立即数**，同分支里两者一致 ⇒ 不是看错。

### 33.3 士兵：`bot_` 前缀（`0x4ab060`–`0x4ab270`）

皮肤名取自 `GraphicsManager` 的 `HashMap<u32 /*player_id*/, String>`（桶距 `0x20`，
`0x4aac8c`–`0x4aad48` 查表），命中就 `String::clone` 进一个 `Cow<str>` 槽
（`[sp+0x20..0x38]`，borrowed 用 `capacity` 的高位标记）；取不到就写
`0x8000000000000000`（= `Cow::Borrowed("")`）。随后是三个分支：

```text
0x4ab060  ldr  x20, [sp, #0x20]        ; Cow 的 capacity
0x4ab068  cmp  x20, #0x8000000000000000
0x4ab06c  b.ne 0x4ab0e4                ; ── 空皮肤 ⇒ 写点 1（DeathEffect）
0x4ab0e4  ldp  x21, x8, [sp, #0x28]    ; x21 = ptr, x8 = len
0x4ab0e8  cmp  x8, #0x3
0x4ab0ec  b.hi 0x4ab218                ; ── len ≤ 3 ⇒ 写点 2（DeathEffect）
0x4ab218  ldr  w9,  [x21]              ; 前 4 个字节
0x4ab220  mov  w10, #0x6f62            ; 'b','o'
0x4ab224  movk w10, #0x5f74, lsl #16   ; 't','_'
0x4ab22c  cmp  w9, w10
0x4ab234  b.eq 0x4ab24c                ; ── "bot_*" ⇒ 写点 3（AshEffect）
                                       ; ── 其余 ⇒ 写点 2（DeathEffect）
```

**这与队伍无关**：同一条路径上唯一的 `get_team(player_id)`（`0x4aac40`）结果被
`and w2, w8, #0x1` 送进 `flip`/颜色，没有进入登记的任何一个分支。

### 33.4 靶子与"迟到的皮肤"

* `add_target` 在 `0x4a583c`–`0x4a5858` 现场拼出 6 个字节 `target`（`0x7465` + `0x6174`/`0x6772`
  两个立即数），交给 `set_skin_graphics`，然后在 `0x4a589c`/`0x4a58d0` 两处写 `2`。
* `handle_graphics_info` 的两条闸门：事件 tag 必须是 **51 `SkinGraphicsInfo`**，
  且 `graphics_str` **长度 ≥ 4 且以 `bot_` 开头**（`0x4aa348` 同一对立即数）；
  命中后对一批 id 逐个登记 `3`（循环体 `0x4aa410`–`0x4aa47c`）。

**回放里能直接看到这条链**（`1782640836`）：

```text
tick 20  SkinGraphicsInfo{881,"bot_0"}
tick 20  EntityEffectInfo{880,"AshEffect"} / {881,"AshEffect"} ×2 趟   ← §33.2 写点 5
tick 3464 EffectInfo{885, AshEffect} / {886, AshEffect}                ← 这两人死时的死亡块
```

⇒ 一个玩家开局时若还没选皮肤（登记 `DeathEffect`），等 `bot_N` 的 `SkinGraphicsInfo`
到了，他名下实体的登记会被**改写**；死亡时发的就是这个改写后的值。

### 33.5 落码与**对拍口径**（数字一位没动，原因是硬约束）

`src/` 的改动（都在允许范围内）：

* `graphwar_entity_effect_manager`：新增 `BOT_SKIN_PREFIX`/`TARGET_SKIN`、
  `register_soldier`/`register_target`/`apply_skin_graphics` 三个**登记点**投影；
* `game::Match`：`new` 给入参士兵做出生登记（此刻没有皮肤 ⇒ `DeathEffect`）、
  `add_target` 登记 `Spark`、新增 `note_skin_graphics`（§33.4 那次改写）、
  新增 `entity_effect_of` / `handle_entity_removed`；
  `execute_results` 的死亡块改成**查表**，`None` ⇒ **连 `EffectInfo` 都不发**。

**`docs/141` 的 401 份快照上，before/after 一位没变**：

| 指标（`examples/sim_turns --replay-sim`，401 份定格快照） | before | after |
|---|---|---|
| 结算批内容命中 | 21 / 1080 | 21 / 1080 |
| 结算批**消息种类序列**命中 | 916 / 1080（84.8%） | 916 / 1080（84.8%） |
| 多发批（`>=2` 发）种类序列 | 93 / 130（71.5%） | 93 / 130（71.5%） |
| 完全一致的回放 | 1 / 401 | 1 / 401 |

不是"没改对"，而是**这份口径拿不到皮肤**：`_scaffold::replaydiff::Predictor::new` 只用
`Inputs::scan` 的 `pos`/`alive` 建 `Match`（`Match::new(room, obstacles, soldiers, tick)`），
`SkinGraphicsInfo` / `TargetInfo` 都在**没喂进去**的那一侧。于是每个实体走的都是
"出生时还没有皮肤"这条分支 ⇒ 全部 `DeathEffect` ⇒ 与本轮之前的"写死 DeathEffect"
**输出逐字节相同**。

要看到收益，需要让对拍器把两条**输入**喂进来（`docs/141` 的量纲不变，只是多喂输入）：

```rust
// _scaffold/replaydiff.rs::Predictor::predict，在 cursor 推进的那一段里
"SkinGraphicsInfo" => if let (Some(id), Some(s)) = (m.get_i64("entity_id"), m.get_str("graphics_str")) {
    self.model.note_skin_graphics(id, &s);
},
"TargetInfo" => if let Some(id) = m.get_i64("target_id") { self.model.add_target(id, 2) },
```

**实测（本轮用一次性探针跑过，探针不入库）**：把上面两条喂进去、并把
`Policy.produced_effects` 扩成四类（否则回放的 `AshEffect` 会被当成"我方不产"整类投掉，
这个问题**问不出来**）：

| 口径 | 未喂皮肤 | 喂了皮肤 |
|---|---|---|
| 默认 `Policy`（Ash/Spark 当我方不产） | 多发批 93 / 130 | 93 / 130（Ash 仍被投掉，看不见） |
| 严格 `Policy`（四类都参与比较） | 多发批 **77 / 130** | 多发批 **100 / 130（+23）** |
| 严格 `Policy`，全语料结算批种类序列 | 833 / 1080 | **918 / 1080（+85）** |

⇒ 机制对了：**+23 个多发批**靠"照抄登记表"救回来（`docs/141` 记的 19 例全在这个集合里，
多出的 4 个是"死亡块里 Ash 与 Death 混排"的批）。剩下的差距是死亡集合漏报与爆炸点，
不在本节范围。

### 33.6 **改正 §29.3**：死亡那条的 `effect_id` 是**计数器**，不是实体 id

§29.3 的表格把 `0x4a8cb0` 那处写成"`effect_id` = 实体 id"。本轮逐指令复核后**改成计数器**：

```text
0x4a8c50  ldurb w21, [x13, #-0x4]      ; effect_type ← 表值（u8）
0x4a8c68  ldxr  w23, [x28]             ; w23 ← 会话级原子计数器
0x4a8c6c  add   w8, w23, #0x1
0x4a8c70  stxr  w9, w8, [x28]          ; 计数器 +1
0x4a8c90  mov   w1, w23
0x4a8c94  bl    EffectManager::add_effect   ; ← effect_id = w23
0x4a8cb8  stp   w23, w24, [x29, #-0x48]     ; EffectInfo +0x18 = w23, +0x1c = w24(start_tick)
0x4a8cbc  sturb w21, [x29, #-0x40]          ; EffectInfo +0x20 = effect_type
```

事件基址 `x29-0x60` 与 §29 的 72 字节布局逐字段对上（`+0x00` tag /
`+0x08`,`+0x10` 坐标 / `+0x18` `effect_id` / `+0x1c` `start_tick` / `+0x20` 类型）；
`w23` 在 `0x4a8c68` 被 `ldxr` **覆盖**，所以它此刻只能是计数器。
回放侧同结论：`1782640836` 一批里死者是 877/881/880，三条死亡 `EffectInfo` 的
`effect_id` 是 **884/885/886**（连号），与随后的爆炸 887 共用同一个计数器。

⇒ 我们原来的 `self.next_effect_id` 自增是**对的**；错的是"类型写死 `DeathEffect`"。

### 33.7 未知

| # | 未确定 | 缺什么 | 等级 |
|---|---|---|---|
| 1 | `handle_graphics_info` 里"一批 entity id"的确切来源 | 只解到形状（一张 `HashMap` 查表 + `SpecFromIterNested::from_iter`，`0x4aa398`–`0x4aa3e8`）；是"该玩家名下实体"还是"名下士兵"没有逐条确认 | **未知**（两种读法在本仓库行为相同：我们只有 `Match::soldiers`） |
| 2 | 写点 6（远端处理器）是覆盖还是先比较 | 没逐指令核 | **未知**（不影响本地局） |
| 3 | `bot_` 之外还有没有别的"特殊皮肤" | 全语料只出现 `target` / `bot_0..5` / 人类皮肤三类；**没有**枚举全表皮肤名 | **未知**（回放侧 899/953 全中，没有反例） |

---

## 34. `EntityDrawer` 成了真 Godot 节点（2026-11：第一个垂直切片，72 条 headless 断言全过）

**一句话**：`src/_scaffold/entity_drawer_class.rs` 在 `gdext-bindings` feature 下把
`entity_drawer.rs` 的**纯逻辑**挂成 `Control` 子类 `EntityDrawer`；`spawn_entity(id, x, y)`
真的建出 **3 个 `Sprite2D` + 1 个 `RichTextLabel`**，位置 / 缩放 / 翻面 / 不透明度**全部**来自
复原的算式（`sprite_transform` / `name_label_layout` / `death_alpha`），贴图是程序生成的
8×8 纯色 `ImageTexture`（**不依赖原版 pck**）。headless 实测 **72 条断言全过**。

> 档位（`AGENTS.md` §0.3）：**观测** = 这轮 headless 真跑出来的输出；**推断** = 有依据未证；
> **未知** = 没解出来。`cargo test` / `selftest` 的条数**只是回归保护**，不是复原证据。

### 34.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/_scaffold/entity_drawer_class.rs` | **新增 677 行**（代码 291 / `//!` 82 / `///` 119 / 行内注释 129 / 空行 56） | 类本体 + 程序生成贴图 + 名字标签 + 6 条纯逻辑单测 |
| `src/_scaffold/mod.rs` | +4 | `#[cfg(feature = "gdext-bindings")] pub mod entity_drawer_class;` + 3 行注释 |
| `client/tools/entity_drawer_check.gd` | **新增 296 行**（72 条断言） | headless 验收脚本 |
| `docs/16-client.md` | +本节 | — |

`src/entity_drawer.rs` **一行没动**（仍然零引擎依赖）；`src/lib.rs` 没动；`client/` 下既有
`.gd` / `.tscn` / `run.sh` 一个没改。

### 34.2 a) 能不能建成 —— **能**（观测）

```text
$ cargo build --release --features gdext-bindings
$ cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
$ godot --headless --path client --import
$ godot --headless --path client --script res://tools/entity_drawer_check.gd
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)

ok   ClassDB.class_exists("EntityDrawer")
ok   EntityDrawer 能实例化且是 Control
      drawer.get_class() = EntityDrawer
ok   方法 spawn_entity() 存在                       # 另有 8 个方法逐一断言
ok   属性 team_color 往返一致（255）
ok   属性 texture_size 往返一致（8）
ok   控件尺寸设为 (500.0, 300.0)
ok   spawn_entity(0, 5, 5) 返回 4（建了几个子节点）
ok   子节点个数 == 4（实际 4）
ok   子节点 0 是 Sprite2D（皮肤）
ok   子节点 1 是 Sprite2D（脸）
ok   子节点 2 是 Sprite2D（帽子）
ok   子节点 3 是 RichTextLabel（名字）
      子节点名: @Sprite2D@2 / @Sprite2D@3 / @Sprite2D@4 / RichTextLabel
ok   子节点 0 的 texture 非空
ok   子节点 0 的 texture 是 Texture2D
ok   子节点 0 的贴图是 8×8（8×8）                 # 1、2 同上
ok   三个 Sprite2D 都 flip_v = true（entity_drawer::SPRITE_FLIP_V）
ok   三个 Sprite2D 都 texture_filter = 4（entity_drawer::SPRITE_TEXTURE_FILTER）
ok   三层贴图互不相同（皮肤 / 脸 / 帽子）
      sprite0 position=(300.0, 100.0) scale=(0.266667, -2.666667) flip_h=true
      期望   position=(300.0, 100.0) scale=(0.266667, -2.666667) flip_h=true
ok   位置 == (300, 100)（k=10: 250+10·5, 150−10·5）
ok   缩放 == (16/60, −160/60)（unit = 128/8）
ok   fit_content = true（name_label.tscn）        # scroll_active / autowrap_mode 各一条
ok   初始尺寸 25×40（offset_right / offset_bottom，实际 (25.0, 40.0)）
ok   set_entity_name 后 text == "Bob"（实际 "Bob"）
      label position=(15.0, 16.75)（期望 (15.0, 16.75)）min_size=(0.0, 0.0)
ok   名字标签位置 == name_label_layout 的手算结果
ok   死了 30 刻 ⇒ alpha = 0.5（实际 0.500000）
ok   死了 ≥60 刻 ⇒ alpha = 0 且标签隐藏
ok   第二个实体位置 == (200, 115)
ok   x < 0 ⇒ flip_h = false
ok   queue_free 之后回到 4 个子节点（实际 4）
ok   被移除实体的子节点已经真的被释放（is_instance_valid == false）
      引擎回调: ready=1 process=2 resized=0
ok   _ready 被引擎调到（1 次）
ok   _process 被引擎调到（2 次；实测不用 set_process，Godot 4.7 自己开）
ok   resized 信号真的连上了（on_resized 被调到 1 次）
      放大后 sprite0 position=(600.0, 200.0)（期望 (600.0, 200.0)）
ok   控件放大到 1000×600 ⇒ 位置重摆成 (600, 200)
ok   子节点随父节点一起被回收（没有单独 free）
ENTITY_DRAWER 72 passed, 0 failed        # 退出码 0
```

> 上面是**节选**：省掉了 8 条同形的「方法 X() 存在」与 2 条同形的贴图断言；
> 行尾 `#` 之后是**我加的行内说明**，不是程序输出。逐字全文用 37.7 的命令重跑即可。

六条结论（每条都指到上面某一行）：

| 结论 | 档位 | 证据 |
|---|---|---|
| 类注册成 `Control` 子类，9 个方法 + 2 个导出属性都在 | **观测** | `ClassDB.class_exists` / `get_method_list` / 属性往返 |
| `spawn_entity` ⇒ **子节点数 4**：3×`Sprite2D` + 1×`RichTextLabel` | **观测** | `子节点个数 == 4` + 四行类型断言 |
| `Sprite2D.texture` **非空**、是 `Texture2D`、三层互不相同 | **观测** | 三段 `texture 非空 / 是 Texture2D / 8×8` |
| 位置 / 缩放 / 翻面 = `sprite_transform` 的手算值 | **观测** | `(300,100)`、`(16/60, −160/60)`、`flip_h=true`；放大后 `(600,200)` |
| 名字标签的文本 / 不透明度 / 属性跟着复原逻辑走 | **观测** | `text == "Bob"`、`alpha 0.5 / 0 / 1`、`fit_content` 等四条 |
| `_ready` / `_process` / `resized` 真的被引擎调到 | **观测** | `ready=1 process=2`，`set_size` 后 `on_resized` +1 且位置重摆 |

**这张表能证明的是**「纯逻辑 → 节点树」这条链路通了；**不能**证明士兵长什么样
（贴图是纯色方块、名字文本是脚本给的、`_draw` 的准星没画）。

### 34.3 b) 一个「建 4 个子节点 + 接纯逻辑」的类多少行

先给总数，再给能推广到别的类的拆分：

| 块 | 行数 | 其中代码 | 说明 |
|---|---|---|---|
| 文件头 `//!` 文档 | 94 | 0 | 为什么 / 与原版逐条对照表 / 命令 / 契约 / 五条坑 |
| **注册**（`#[derive]`+`#[class]`+字段+`IControl::init`） | 56 | ~30 | **宏只占 2 行**；剩下是 5 个字段与 `init` |
| 属性 2 个（`team_color` / `texture_size`） | 8 | 8 | `#[export] i64` + 两个默认常量 |
| 虚方法 2 个（`ready` / `process`） | 25 | 15 | `ready` 19 行里 10 行是注释 |
| `#[func]` 9 个 + `texture_for` | 234 | ~150 | spawn 72 / relayout 65 / 名字 18 / 死亡 17 / 计数 20 / remove 13 / on_resized 9 / entity_count 9 / texture_for 11 |
| 「造物」：`solid_texture` + `layer_color` + `new_name_label` | 120 | ~45 | 8×8 `ImageTexture`、三层颜色、`name_label.tscn` 的 6 个属性 |
| `EntityNodes` 结构 | 22 | 8 | 4 个句柄 + 棋盘坐标 + 名字 + 死亡刻 |
| 纯逻辑单测 6 条 | 107 | 60 | 顺序 / 颜色 / 手算位置 / 镜像 / 淡出 / 标签布局 |
| **合计** | **677** | **291** | — |

**能推广的结论**：

* **注册、属性表、方法表、虚方法表一律 0 行**（gdext 宏全包）。对比 §31.3 的手写绑定：
  同一个「1 属性 + 4 方法、不画东西」的类要 **~500 行**手工 ABI 搬运。
* 真正与「建 4 个子节点 + 接纯逻辑」有关的**代码**只有 **~155 行**
  （`spawn_entity` 49 + `relayout` 52 + `remove_entity` 11 + `texture_for` 8 + 两个造物函数 ~35）。
  再往前的纯逻辑（算式、常量、子节点种类表）**一行没写**——全在 `entity_drawer.rs` 里。
* 剩下 136 行代码是**观测点与文档**：9 个方法的 doc、5 条坑的说明、被照抄的原版常量注释。
* 换成 6 条单测（107 行）之后，「类里有没有偷藏第二份算术」这件事有回归保护。

### 34.4 c) 踩到的坑（每条都是本轮实测，不是听说）

1. **`Gd<Node>` 不保活，也不负责释放。** `godot-core` 的 `RawGd::drop` 注释写死：
   「If the held object is manually-managed, **nothing happens**」。节点归**父节点**所有
   ⇒ 缓存句柄只是弱引用；校验脚本第 14 组断言：本类被 `queue_free` 之后，我们手里那个
   `Gd<Sprite2D>` 已经 `is_instance_valid == false`，而**子节点是引擎跟着父节点回收的**
   （我们一行 free 都没写）。所以要删实体，必须自己 `queue_free`（`remove_entity` 就是这么做的）。
2. **句柄失效之后再碰 = panic + 卡死。** 从 GDScript 直接把某个子节点 `free()` 掉，
   再调 `relayout()`：
   ```text
   PROBE before: valid=true children=4
   PROBE after free(): valid=false children=3
   ERROR: [panic godot-core/src/classes/class_runtime.rs:379]  Sprite2D::upcast_ref:
          access to instance with ID 32631686609 after it has been freed
   SCRIPT ERROR: Bug: Invalid call error code 1337.
   ```
   balanced safeguard **检出了**野句柄（不会静默 UB），但那次 `#[func]` 直接失败，
   **进程随后卡住不退出**（60 s `timeout` 才杀掉）。⇒ 句柄与节点必须同生共死；
   本轮**没有**加 `is_instance_valid` 防御（原版也不加），靠「只在自己的 `remove_entity` 里删」。
3. **树外 `add_child` 只记父子关系。** `--script` 模式的 `_initialize()` 跑在根节点进树之前：
   那一刻 `is_inside_tree()` 是 false、子节点的 `_ready` / `_process` 都不会来。
   本类**不依赖**它们（`spawn_entity` 自己调一次 `relayout()`），所以「先 spawn 再进树」
   和「先进树再 spawn」一样能过断言。
4. **`resized` 是 `_ready` 里才连的** ⇒ 树外 `set_size` 不会自动重摆（信号那时还没连）。
   要嘛手动 `relayout()`，要嘛等进树。
5. **不用写 `set_process(true)`。** 本轮把这句**删掉重编**，`process_call_count` 照样是 2
   —— Godot 4.7 对「实现了 `_process` 虚方法」的扩展类会自己开；原版 `ready` 的证据里
   也没有这一句（连完信号、订完 6 个事件后写的是 `+0x108` 那个 bool）。**观测**
   （上一版代码里我们写了它，还写了错注释，本轮删掉并改正）。
6. **借用检查 `E0499` 是这类类的日常**：`self.entities.get_mut(id)` 的借用没结束就调
   `self.relayout()`（它也借 `*self`）⇒ 编译不过。要先用一个块把「改状态」做完。
7. **`--import` 与覆盖 `.so` 两个静默陷阱照旧**（`AGENTS.md` 那三条）：
   新增 `.gd` 后不 `--import`，Godot 静默不加载；`cargo build` 之后不 `cp` 到 `client/`，
   跑的是旧内核。本轮两次都先 `--import` 再断言，才没有出现「改了没生效」的假阴性。

### 34.5 原版节点树的证据（本轮新用到的两条，都不是猜的）

1. **`docs/16` §2.1.10 的运行时 dump**：`GraphwarPlane/@EntityDrawer@N/@Sprite2D@N`、
   `GraphwarPlane/@EntityDrawer@N/<name_label 实例>`；`ShowNames` 那一行还观测到
   `@EntityDrawer@N` 下 **4 个名字 `RichTextLabel`**（visible 1→0）。
   ⇒ ① 一个 `EntityDrawer` 挂在 `GraphwarPlane` 下、**每个实体一个名字标签**（与
   `ENTITY_CHILD_NODES` 的 4 项、每实体 4 个节点一致）；② 三个 sprite **没有名字**
   （`@Sprite2D@N` 是 Godot 自动名）⇒ 本轮也**不显式命名** sprite。
2. **`recovered-src/name_label.tscn`（pck 回收的原版场景）**：`offset_right=25` /
   `offset_bottom=40`、`fit_content=true`、`scroll_active=false`、`autowrap_mode=0`、
   内联 `Theme`（`StyleBoxFlat`：bg/border 全 alpha 0、四边 2px、圆角 20）
   ⇒ 本类的名字标签按这 6 项逐条重建（不加载那个场景，因为 `client/` 里没有它）。

**还没解 / 没做**（诚实清单）：

* `_draw` 的准星（三段弧 + 三个箭头，几何已在 `entity_drawer::ring_primitives`，但本切片没接）；
* `ready` 里那 6 个后端事件（`GraphwarBackend` 还没接到节点层）；
* `entity_removed_event` 触发的清理（`entity_drawer::UNKNOWNS` 第 2 条"消失判据"仍未解）；
* `GraphwarTextureLoader`（本类用程序生成贴图顶替）、`GraphwarPlane` 谁来 `add_child` 本类；
* 名字文本从哪来（原版走名字管理器 / backtracer，不归本模块）。

### 34.6 d) 剩下 10 个类能不能照这个模板批量做 —— **能，但要认清三类差异**

名单按 `docs/18` §6.5 的 12 个 GDExtension 类：减去已落地的 `AxisDrawer`（§31，能注册 + `_draw`）
与本节 `EntityDrawer`，**实剩 10 个**（任务书说 11 个，是把 `EntityDrawer` 自己也算进去的数）。

基类与虚方法一列来自 `src/*.rs` 各文件头 + `docs/02` §17 的 `TRAIT_IMPLS`（**观测**）：

| 类 | 形状 | 照本模板还要补什么 | 风险 |
|---|---|---|---|
| `PlayerShowcase` | `Control` + `IControl::draw`（画三层贴图）+ `ready`（灌名字 + 连信号）+ `process`（可见性闸门） | 补一个 `_draw`；其余与本类几乎一样 | 低 |
| `GraphwarPlane` | `Control` + 画棋盘框 + 每帧驱动障碍绘制器 + 子节点（`@AxisDrawer@N` / `@EntityDrawer@N` / `@Line2D@N`） | 补 `_draw`；**它才是父节点**，要真的 `add_child` 本类 | 中 |
| `EffectDrawer` | 图元 + `EffectData` 状态机（`src/effect_drawer.rs`） | 补 `_draw`（与 `AxisDrawer` 同形，§10 已证 gdext 的 `_draw` 会被调到） | 低 |
| `GraphwarLongPressAnimation` | `Control` + **唯一的虚函数 `draw`**（读自己的启动时刻） | 补 `_draw` | 低 |
| `GraphwarLanVBox` | `VBoxContainer`（`IVBoxContainer`）、**没有任何方法** | 换基类（`#[class(base = VBoxContainer)]`） | 低 |
| `GraphwarButton` | `Button`（`IButton`）+ `pressed` / `button_up/down` / `input_text` | 信号接线（见下）+ 字符串导出属性 | 中 |
| `GraphwarFunctionInput` | `LineEdit`（`ILineEdit`），**没有 `LineEdit` 子节点**（文字状态在 Rust 侧） | 信号接线 + 字符串属性 | 中 |
| `GraphwarSetupControl` | `Control`（`IControl`）+ `on_text_change` / `on_visibility_change` / `on_chat_submitted` / `force_title_update` | 信号接线 + 三个容器的子节点管理 | 中 |
| `GraphwarRootNode` | `Node`（`INode`）+ 屏表 `HashMap<&'static str, Gd<Node>>` + 平台回调（权限 / 内购 / `log`） | 换基类；平台回调只能打桩 | 中（内容未知） |
| `GraphwarAdSwitch` | `Node` + 2 个导出方法（`are_ads_enabled` → bool、`get_ad_unit_id` → String）+ 1 个字段 | **本模板用不上**：两个 `#[func]` 就够 | 低 |

**能批量做的三个理由**（本轮实测支撑）：

1. 注册 / 属性 / 方法 / 虚方法全由宏生成（§31.3 那 500 行手工 ABI 已经不需要了）；
2. 「读引擎值 → 调纯逻辑 → 写回节点」这个循环本轮跑通了两次（`_draw` 的 §10 + 子节点的本节），
   剩下 10 个类的算式同样在 `src/*.rs` 里现成躺着（10 个类里有 **4 个**只需再加一个 `fn draw`）；
3. 验收脚本可以**照抄 `entity_drawer_check.gd` 的 14 个段落**（类/方法/属性 → 子节点/类型 →
   贴图 → 位置 → 文本 → 淡出 → 第二实体 → 移除 → `.new()` → 进树回调 → 尺寸变化 → 回收），
   每类只要改「期望值」与「子节点类型」。

**两个必须注意的「不一样」**：

* **有信号的类**：本切片只连**别人的**信号（`resized` → `on_resized`）。`GraphwarButton` 的
  `pressed` / `button_up/down`、`GraphwarFunctionInput` 的 `on_text_change` / `on_text_submitted`、
  `GraphwarSetupControl` 的 `on_text_change` / `on_visibility_change` 三组，**要么**是连原生信号
  （`pressed` / `text_changed` / `visibility_changed`，本类已验证「连别人」这一条路），**要么**是
  自造信号（`#[signal]` + 生成器 `emit()`）。**哪一种还没有证据**（`docs/02` §17 的 `TRAIT_IMPLS`
  只写了 `IControl` / `IButton` 这类 trait，不含信号）⇒ 自造信号那条路**未验过**。
* **有导出属性的类**：本轮只验了 `#[export] i64`。`GString` / `Vector2` / `Gd<Resource>` 属性的
  导出与往返没验过（**未知**）。

**一个要忍住的诱惑**：不要为了「少踩坑」把 4 张 `HashMap` 合成一个公共 helper。
原版 `EntityDrawer` 就是 4 张表（字段偏移 `+0xa8/+0xc0/…`，名字仍是**推断**），
本节的 `EntityNodes` 已经是**一次化简**（4 个句柄塞一个结构）；下一轮迁 `player_showcase`
时应先按 `docs/16` §22 的证据决定要不要拆回去。

### 34.7 复现（三分钟）

```text
# 1) 打开 feature 构建（默认关闭 ⇒ 关着时 crate 仍然零依赖）
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --release --features gdext-bindings --offline
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so

# 2) 必须先 import（否则扩展静默不加载，`entity_drawer_check.gd` 连类都认不出来）
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
godot --headless --path client --import

# 3) 验收（72 条断言，退出码 0）
godot --headless --path client --script res://tools/entity_drawer_check.gd

# 4) 回归保护（与本节无关的两条硬要求）
cargo test --offline                          # lib 457 / 总计 1488（当轮；现行 1520）
cargo test --offline --features gdext-bindings # lib 463（457 + 本类 6 条）/ 总计 1494（当轮；现行 1595）
bash client/run.sh selftest                   # 694 passed（默认 feature 的 .so）
```

## 35. 三个「基类不是 `Control`」的类成了真节点（2026-11：`Node` / `VBoxContainer` / 基类未定）

**一句话**：`src/_scaffold/` 下新增三份绑定外壳，把 `graphwar_root_node`（基类 **`Node`**）、
`graphwar_lan_vbox`（基类 **`VBoxContainer`**）、`graphwar_ad_switch`（基类**仍未定**，按 `Node` 落）
挂成**真 Godot 节点类**，用**同一套模板**（只有虚方法块不同）。headless 实测
**117 条断言全过**；三份纯逻辑文件**一行没动**。

> 档位（`AGENTS.md` §0.3）：**观测** = 能指到 arm64 `.symtab` 记录 / 反汇编地址 / 本轮 headless 真跑出来的输出；
> **推断** = 有依据未证（同行写清缺什么）；**未知** = 没解出来。
> `cargo test` / `selftest` 的条数**只是回归保护**，不是复原证据。

### 35.0 这一节回答的三个问题

| # | 问题 | 结论 | 档位 |
|---|---|---|---|
| 1 | **换基类要不要改虚方法 trait？** | **只改 3 处**（trait 名 / `Base<T>` 类型 / `init` 参数类型）；`#[func]` 与 `#[export]` **一个字都不用改** | 观测（三份类真跑） |
| 2 | **`GraphwarAdSwitch` 到底是不是 Godot 类？** | **是**（6 条 arm64 `.symtab` 证据，见 39.4）。**但基类定不出来** —— 缺的正是「它一个虚方法都没覆写」 | 观测（是）/ 未知（基类） |
| 3 | **`GraphwarRootNode` 能不能只做骨架？** | **能**：装配部分不做，但「持有子对象 → 转发一次调用」两条链路都通了（`purchase_confirmed` → `Ad_holder`、`show_screen` → 15 个屏的 `set_visible`） | 观测（headless） |

### 35.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/graphwar_root_node_class.rs` | **新增** | 555 | `Node` 基类骨架 + `ScreenHost` 适配器 + 5 条纯逻辑单测 |
| `src/_scaffold/graphwar_lan_vbox_class.rs` | **新增** | 560 | `VBoxContainer` 基类 + 行投影 + 4 条单测 |
| `src/_scaffold/graphwar_ad_switch_class.rs` | **新增** | 269 | `Node` 基类 + 2 个原版 getter + 3 条单测 |
| `src/_scaffold/mod.rs` | 改 | +9 | 三个 `#[cfg(feature = "gdext-bindings")] pub mod …` + 注释 |
| `client/tools/base_classes_check.gd` | **新增** | 402 | headless 验收脚本（117 条断言） |
| `docs/16-client.md` | 改 | +本节 | — |

`src/graphwar_root_node.rs` / `graphwar_lan_vbox.rs` / `graphwar_ad_switch.rs` **一行没动**
（仍然零引擎依赖）；`src/lib.rs`、其它 `src/*.rs`、既有 `client/*.gd`、`docs/INDEX.md`、
`docs/HANDOVER.md` **都没碰**。

### 35.2 问题 1：换基类要改什么（**实测**，不是推理）

三份文件是**同一个模板**的三个实例。逐项对照如下（左 = 本批真正改动的全部内容）：

| 项 | `Control`（§34 的 `EntityDrawer`） | `Node`（`GraphwarRootNode` / `GraphwarAdSwitch`） | `VBoxContainer`（`GraphwarLanVBox`） |
|---|---|---|---|
| 虚方法 trait | `impl IControl` | **`impl INode`** | **`impl IVBoxContainer`** |
| 基类字段 | `base: Base<Control>` | **`base: Base<Node>`** | **`base: Base<VBoxContainer>`** |
| `init` 参数 | `Base<Control>` | **`Base<Node>`** | **`Base<VBoxContainer>`** |
| 可覆写的虚方法集 | `ready` / `process` / `draw` / `notification` … | `ready` / `process` / `physics_process` / `enter_tree` / `exit_tree` … | **同一套**（`IVBoxContainer` 上溯 `IBoxContainer` → `IContainer` → `IControl`；`VBoxContainer` 自己不新增虚方法） |
| `#[godot_api] impl 类名 { … }` | `#[func]` + `#[export]` | **完全一样** | **完全一样** |
| `#[derive(GodotClass)]` + `#[class(base = …)]` | 同一行 | 同一行（只换 `base =`） | 同一行 |

⇒ **结论**：换基类 = 改「虚方法块」的 3 个类型位置 + `#[class(base = X)]` 那一处。
`#[func]` 方法表、`#[export]` 属性、`#[derive(GodotClass)]` 的字段生成（`init` 之外的
`Default` 兜底）**都与基类无关** —— 三套基类下 headless 实测行为一致（39.7 的第 1、2 组断言）。

两条**本轮实测到的基类差异**（不是模板差异，是引擎差异）：

1. **`VBoxContainer` 会自动纵向排列子节点**（`child0.y = 0`、`child1.y = 28`，观测）。
   这正是原版「显示顺序 = `add_child` 顺序」的机制来源；纯 `Node` 没有这个行为。
   ⇒ 「不排序」这条结论在 `VBoxContainer` 下才**看得见**。
2. **`#[export]` 生成的 setter 会占名字**：`#[export] owned: bool` 自动生成
   `set_owned` / `get_owned`，自己再写一个 `pub fn set_owned` 会 **`E0592` 重复定义**
   （本轮踩到，改名 `write_owned`）。同理 `Base<…>` 字段必须叫 `base`。

### 35.3 12 个类的基类对照表（**本轮新产物**，arm64 2.0.60 `.symtab`）

`docs/02-module-map.md` §18.3 只列到「哪 11 个类有 Godot 基类 impl」。本轮把**基类名**取出来了：
gdext 把 `impl I<Base>` 的路径写进了符号名，所以 `grep 'gen::classes'` 一试即得。

复现（`.reference/steam/android-2.0.60/libgraphwarrust.so`，未 strip）：

```text
readelf -sW libgraphwarrust.so | c++filt | grep "gen::classes" \
  | grep -oE "(graphwarrust::[a-z_0-9]+::[A-Za-z]+).{0,200}?gen::classes::[a-z_0-9]+::" \
  | sed -E 's/(graphwarrust::[a-z_0-9]+::[A-Za-z]+).*gen::classes::([a-z_0-9]+)::/\1  <=  \2/' \
  | sort -u
```

输出（11 行，逐条 = 一个类的基类）：

| 类 | 基类（`gen::classes::` 段） | Godot 类名 | 覆写的虚方法（符号里能看到的） |
|---|---|---|---|
| `AxisDrawer` | `control` | `Control` | —（另有 `__virtual_call` ×3） |
| `EffectDrawer` | `control` | `Control` | — |
| `EntityDrawer` | `control` | `Control` | — |
| `GraphwarButton` | `button` | `Button` | — |
| `GraphwarFunctionInput` | `line_edit` | `LineEdit` | — |
| **`GraphwarLanVBox`** | **`v_box_container`** | **`VBoxContainer`** | **`init`（`0x450a90`）、`process`（`0x450c20`）** |
| `GraphwarLongPressAnimation` | `control` | `Control` | — |
| `GraphwarPlane` | `control` | `Control` | — |
| **`GraphwarRootNode`** | **`node`** | **`Node`** | **`init`（`0x399dd0`）、`ready`（`0x39a6f0`）、`process`（`0x39c8a0`）** |
| `GraphwarSetupControl` | `control` | `Control` | — |
| `PlayerShowcase` | `control` | `Control` | — |
| **`GraphwarAdSwitch`** | **（没有）** | **未知** | **没有** |

最后一行就是本批最难的一条，见 39.4。

### 35.4 问题 2：`GraphwarAdSwitch` 到底是不是 Godot 类

#### 结论一：**是**（档位：观测）. 6 条 arm64 `.symtab` 证据

`readelf -sW libgraphwarrust.so | c++filt | grep ad_switch` 逐条读出来的：

| # | 符号 | 地址 / 大小 | 为什么它是证据 |
|---|---|---|---|
| 1 | `<…GraphwarAdSwitch as godot_core::obj::traits::GodotClass>::class_id::**CLASS_ID**` | `.data` `0x8b40c0`，8 B | 这个静态**定义在** `fn class_id()` 里面 ⇒ `impl GodotClass for GraphwarAdSwitch` 存在 |
| 2 | `<… as cap::ImplementsGodotApi>::__register_methods` | `0x43fb5c`，400 B | 方法表注册驱动（`ImplementsGodotApi` 的 supertrait 是 `GodotClass`） |
| 3 | `<… as cap::ImplementsGodotApi>::__register_constants` | `0x43fcec`，400 B | 常量表注册驱动 |
| 4 | `<… as cap::ImplementsGodotVirtual>::__virtual_call::virtual_fn` | `0x4410f4`，156 B | 虚方法派发 |
| 5 | `<… as UserClass>::__default_virtual_call::virtual_fn` | `0x438194`，156 B | 未覆写虚方法时的兜底派发 |
| 6 | `graphwar_ad_switch::GraphwarAdSwitch::__registration_storage::STORAGE` | `.data` `0x8b41e8`，56 B | 惰性注册库的存储（`__register_*` 用的那把锁+函数表） |

外加 `drop_in_place<GraphwarAdSwitch>`（`0x3ae8cc`，28 B）与 4 个
`graphwar_ad_switch::_::__init::__inner_init`（`0x4439a4` 428 B / `0x443b50` 504 B /
`0x443d48` 448 B / `0x443f08` 512 B）—— 后者的**跨度多重集**与 `GraphwarLanVBox` 的 4 个
`__inner_init`（512/504/448/428）**逐个相同** ⇒ 同一组 gdext 单态化被两个类各实例化一次。

⇒ 「`GraphwarAdSwitch` 不是 Godot 类、只是个纯 Rust 模块被误当成类」这条**被证否**。

⚠️ **容易误判的一步**（这也是本批出发时的假设）：`docs/02` §18.3 与出发任务里说的
「符号表里只有 11 个 `impl GodotClass`，缺 `GraphwarAdSwitch`」，数的是
`<… as GodotClass>::class_id` 这个**函数**符号。12 个类确实只有 11 个 `class_id` 函数，
但**第 12 个 impl 的关联常量 `CLASS_ID` 在表里**（证据 1）。所以正确读法是：

```text
class_id() 这个函数被内联/裁掉了 ≠ impl GodotClass 不存在
（关联常量还留在 .data 里，LOCAL HIDDEN，8 字节）
```

#### 结论二：**基类定不出来**（档位：未知）. 缺什么证据写在这里

一个类的基类在这份 `.symtab` 里**只有一个出口**：`<T as I<Base>>` 这族符号（39.3 那张表）。
它**只在类覆写了至少一个虚方法时才生成** —— 而 `GraphwarAdSwitch` **一个都没覆写**
（`docs/16` §20.5：6 个函数全是惰性注册驱动，没有独立方法体；本批也复核了：
它连 `ready` / `process` 都没有）。一个空的 `impl INode for GraphwarAdSwitch {}` 与
「不写这个 impl」编出来的机器码是同一件事 ⇒ **符号表里不存在能区分基类的东西**。

本轮还试过另外两条静态路子，都**失败**了（写下来省得别人再试）：

| 路子 | 结果 |
|---|---|
| 在 `.rodata` 里找 `"GraphwarAdSwitch"` 再找引用它的指令 | 4 处命中，全是 **`type_name` 游程**（`…GraphwarAdSwitchgraphwarrust::axis_drawe…`，**不以 NUL 结尾**）；全 `.text` 的 `adrp`+`add`/`ldr` 扫完，**没有任何一条指令**指向它们 |
| 在 `.data.rel.ro` / `.rela.dyn` 里找指向类名字符串的指针 | **0 命中**（qword 搜 + 20707 条 `RELA` 的 addend 搜都试了） |

⇒ 类的注册走 `.init_array` → `.data.rel.ro` 的函数指针表，基类名是一个 4–13 字节的
**合并池短串**（`"Node"` / `"Control"` / `"VBoxContainer"`），不跟表就**归属不到某个类**。

**缺的证据（任一条即可定案）**：

1. **静态**：`classdb_register_extension_class` 调用点上传的 `parent_class_name` 实参。
   要跟的是 `__registration_storage::STORAGE`（`0x8b41e8`，56 B）里那张函数指针表，
   逐个 disasm 到 `adrp+add` 的短串 + 长度立即数。
2. **运行时**（更省事）：真机 / 能跑原版 `.so` 的 Godot 4.0 环境里
   `ClassDB.get_parent_class("GraphwarAdSwitch")` 一行就够。本机没有那个环境。

#### 结论三：**本文件按 `Node` 落，并标成推断**

理由三条（也写进了 `graphwar_ad_switch_class.rs` 的文件头）：

* 模块文档 [`graphwar_ad_switch`] 与 `docs/02` §16.2 / `docs/16` §20 一直写 `Node`（同样是**推断**）；
* 实例布局能对上「`base` 字段在最前」的写法：`owned` 在 `+0x18`
  （**逐位（arm64）**，`docs/19` §2.10），而 gdext 的 `Base<T>` 是 24 字节
  ⇒ 源码形状就是 `struct GraphwarAdSwitch { base: Base<…>, owned: bool }`；
  但 **24 字节对所有 gdext 基类都一样**，所以它**区分不了**基类（别拿它当证据）；
* 它不覆写虚方法 ⇒ 基类只影响「父类名」与「能不能挂在某个父节点下」；`Node` 是这一族里最宽的，
  **行为上没有可观测差别**。

⇒ **它需要注册**（不是纯 Rust 模块），基类按 `Node` 落、档位 **推断**。

### 35.5 问题 3：`GraphwarRootNode` 只做骨架也能转发

建出来的子节点树（headless 实测，`build_scene_graph()` 幂等）：

```text
GraphwarRootNode                         ← 本类（Node）
├── ScreensControl                      ← 空 Node 容器（原版 _ready 里 get_node 过）
│   └── MainMenu … DisconnectedPopUp     ← 15 个 Control，名字 = Screen::name()
├── IAP_holder                          ← 空 Node 占位（原版 _ready 里 get_node 过）
├── Ad_holder                            ← **GraphwarAdSwitch**（另一个扩展类）
└── GraphwarModuleContainer
    └── GraphwarBanList …                ← 25 个空 Node，名字 = 模块类名
```

两个 holder 的名字、数量、`get_node` 顺序都来自 `graphwar_root_node::READY_NODES`
（**观测**：3 个 `lea` + 3 个长度立即数 + 3 个 `call 0x3acb00`）。

两条**真的跑通**的转发链：

| 链路 | 做了什么 | 证据 |
|---|---|---|
| `purchase_confirmed()` / `purchase_denied()` → `Ad_holder` | 拿到 `Gd<GraphwarAdSwitch>`，`bind_mut()` 写 `owned` ⇒ 再读 `are_ads_enabled()` 变了 | arm64 `0x3a658c` 写 1 / `0x3a61b0` 写 0（`+0x18` 的**唯一**两个写点，`docs/19` §2.10③） |
| `show_screen(name)` → 15 个屏 | 跑 `graphwar_root_node::apply_show_screen`（`0x3af850` 决策表的纯逻辑版），把 `set_visible` 落到真 `Control` 上；两个单列分支的 `call_deferred` 进流水 | `0x3af850` 的控制流（`docs/16` §15.5）+ 本轮 headless |

**刻意没做**（不猜）：`0x3a5760` 那 20288 字节状态机的主体、`0x3b0830` 命令总线的全部键
（只解到 12 条）、25 个 manager 的**真实**装配（只建同名空 `Node` 占位）、两个
`Rc<RefCell<T>>`（`+0x1668` / `+0x16c8`）、`GraphwarTextureLoader`（`+0x16a0`）。

### 35.6 三个类各自的行为清单（原版 vs 本文件 vs 档位）

| 类 | 原版 | 本文件 | 档位 |
|---|---|---|---|
| `GraphwarAdSwitch` | 2 个 getter（`are_ads_enabled = !owned` / 38 字节 AdMob id）+ 1 个 `+0x18` bool | 同 2 个 getter + `#[export] owned`（**推断**：属性可写）+ `owned_raw` / `owned_field_offset` 观测点 | getter 观测 / 基类与属性**推断** |
| `GraphwarLanVBox` | `VBoxContainer`；每帧把源表投影成行（`load`+`instantiate`+`add_child` 一次），复用行只刷字段，源表没有的行 `queue_free`；**不排序**；Join 走内部 `mpmc` 通道、下一帧才连 | 同一套判据（`LanVBox::sync_rows` + `plan_frame`），但**源表由 GDScript 传**；行节点手工造（`Control` + `RoomName` 标签 + `JoinButton` 按钮）；4 个模式图标没画 | 判据/时序观测；行节点造法是我方选择 |
| `GraphwarRootNode` | 装配屏表 + 2 个控制器 + 贴图加载器 + 一批 manager；`0x3af850` 切屏与广告；`0x3a5760` 状态机；`0x3b0830` 命令总线 | 只做「3 个 holder + 15 个屏 + 25 个占位槽」的形状，与 `0x3af850` 的转发；`ready` / `process` 计数**不猜行为** | 形状与切屏观测；状态机/总线**未做** |

### 35.7 验收：`client/tools/base_classes_check.gd`（**117 条断言，0 失败**）

```text
$ cargo build --release --features gdext-bindings --offline
$ cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
$ godot --headless --path client --import          # 必须先 import，否则扩展静默不加载
$ godot --headless --path client --script res://tools/base_classes_check.gd
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)
Godot Engine v4.7.2.stable.official.ed1daf0bf - https://godotengine.org

ok   ClassDB.class_exists("GraphwarRootNode")      # 另两个类同形
ok   GraphwarRootNode 能实例化且是 Node
ok   GraphwarLanVBox 能实例化且是 VBoxContainer
ok   GraphwarAdSwitch 能实例化且是 Node
ok   GraphwarLanVBox.is_class("Container")（继承链）
ok   GraphwarLanVBox.is_class("Control")（继承链）
ok   GraphwarAdSwitch **不**实现 ready/process（原版 6 个函数全是注册驱动）
ok   属性 ads_slot 往返一致（true）                 # show_global / owned 同形，共 6 条
ok   show_global = true ⇒ 源 = Broker
ok   owned = true ⇒ are_ads_enabled() = false
      ad_unit_id = "ca-app-pub-3940256099942544/6300978111"（38 字节）
ok   owned 的字段偏移 == 0x18（arm64 逐位）
     期望顺序 = ["RoomEntry_127_0_0_1_35249", "RoomEntry_127_0_0_1_35243", "RoomEntry_127_0_0_1_35241"]
     实际顺序 = ["RoomEntry_127_0_0_1_35249", "RoomEntry_127_0_0_1_35243", "RoomEntry_127_0_0_1_35241"]
ok   子节点顺序 == 传入顺序（**没有**按地址排序）
ok   行节点被**复用**（3 个 instance_id 都没变 ⇒ 没有整页重建）
ok   被清掉的行真的被 queue_free 释放了
ok   press_join 之后请求还在通道里（1 条）
      root 直接子节点 = 4
ok   ScreensControl 下有 15 个屏节点（实际 15）
ok   manager_slot_count() == 25（模块槽）
ok   manager_registration_count() == 22（GraphwarGameClient::new 的注册点）
ok   Ad_holder 的身份是 GraphwarAdSwitch（跨类持有，class_exists 级别的证据）
ok   **转发真的写进了子对象的字段**（Ad_holder.are_ads_enabled() = true）
      show_screen("Campaign") 写了 15 次 set_visible
ok   **转发到了真节点**：ScreensControl/Campaign.visible == true
      deferred_log = ["create_ad_view"]
ok   离开 PlayScreen ⇒ call_deferred("create_ad_view") 被记下
ok   进入 PlayScreen ⇒ call_deferred("destroy_ad_view")
      Node 基类回调: ready=1 process=2；VBoxContainer 基类 process=2
ok   GraphwarRootNode._ready 被引擎调到（1 次）
ok   GraphwarLanVBox._process 被引擎调到（2 次）      # 不写 set_process，Godot 4.7 自己开
      VBoxContainer 排列: child0.y=0.000000 child1.y=28.000000
ok   VBoxContainer 把第 2 行排在 1 行下面（y 递增）—— 基类真的是 VBoxContainer
ok   子对象 Ad_holder 随父节点一起被回收（没有单独 free）
BASE_CLASSES 117 passed, 0 failed                  # 退出码 0
```

> 上面是**节选**（省掉了同形的方法存在性断言与重复的属性往返）；行尾 `#` 之后是我加的说明，
> 不是程序输出。逐字全文用 39.7 的命令重跑即可。

四条结论：

| 结论 | 档位 | 证据 |
|---|---|---|
| 三个类都注册成**正确的基类**，`#[func]` / `#[export]` 在三套基类下行为一致 | **观测** | `is_class(...)` 5 条 + 方法表 26 条 + 属性往返 6 条 |
| `GraphwarLanVBox` 加 3 个子项**顺序 = 插入顺序**，复用行不重建，过期行 `queue_free` | **观测** | 期望/实际顺序两行逐字相同 + `instance_id` 不变 + `is_instance_valid == false` |
| `GraphwarRootNode` **只做骨架也能转发**：`purchase_confirmed` → 子对象字段；`show_screen` → 15 个真节点的 `set_visible` | **观测** | `Ad_holder.owned == true` + `ScreensControl/Campaign.visible == true` |
| `_ready` / `_process` 在 `Node` 与 `VBoxContainer` 两种基类下都真的被引擎调到 | **观测** | `ready=1 process=2` / `process=2`；**没有**任何 `set_process(true)` |

### 35.8 回归保护（与本节无关的两条硬要求）

```text
cargo test --offline                            # lib 457 passed / 总计 1499 passed / 0 failed
cargo test --offline --features gdext-bindings   # lib 475 passed / 总计 1517 passed / 0 failed
bash client/run.sh selftest                      # SELFTEST 694 passed, 0 failed
```

* `457 → 475` 的差 = `-4`（`_scaffold::gdext` 的手写入口在打开 feature 时不编译）
  `+22`（`entity_drawer_class` 6 + `gdext_mod` 4 + 本批 12）；
* `selftest` 的 694 在**两份 .so 下都跑过**：默认 feature 的手写绑定一份、本批的
  `gdext-bindings` 一份 —— 都是 `694 passed, 0 failed`（换绑定层对客户端透明，§34 的结论再次成立）；
* ⚠️ §34.7 里记的「总计 1488 / lib 463」是**上一轮**的数；本轮实测是 **1499 / 475**，
  差的是别的批次加进来的测试，不是本批。条数**不是进度**（`AGENTS.md` §0.2）。

### 35.9 仍未解 / 下一步

1. **`GraphwarAdSwitch` 的基类**（本节最大未决项）：缺 39.4 那两条证据之一。
2. **`GraphwarRootNode` 的 `process` 主体**（`0x3a5760`，20288 B / 339 条比较 / 64 个直接调用目标）
   与命令总线 `0x3b0830` 的**全部**键（只解了 12 条）。
3. **25 个 manager 的真实装配**：本批只按 `graphwar_module_container::MODULES` 建了同名空 `Node`；
   各 manager 的节点形态（哪些真是节点、哪些是 `Rc<RefCell<_>>`）**未知**。
4. **`GraphwarLanVBox` 的源表还不在节点层**：原版每帧直接读模块容器里的
   `GraphwarLanBroadcasterV6` / `GraphwarServerBrokerHandler`（`+0x128` / `+0x240`）；
   本批由 GDScript 把表传进来。要让这条链自洽，得先把那两个 handler 挂成节点。
5. **两个 `Rc<RefCell<T>>`（`+0x1668` / `+0x16c8`）与 `GraphwarTextureLoader`（`+0x16a0`）**
   仍然只有偏移、没有类型。

---

---

## 36. 三个"要信号接线"的类成了真节点 + **自造 `#[signal]` 的结论**（2026-11：155 条 headless 断言全过）

**一句话**：`GraphwarButton`（`Button`）/ `GraphwarFunctionInput`（`LineEdit`）/
`GraphwarSetupControl`（`Control`）三个类都注册成了**真 Godot 节点类**，headless 实测
**155 条断言全过**；本节把 §34.6 留的那个「**自造 `#[signal]` 未验**」结案 ——
**能**，而且三种"信号/回调"机制逐条给了**能/不能 + 证据**：

| # | 机制 | 结论 | 一条证据 |
|---|---|---|---|
| 1 | **自造 `#[signal]`**（`#[signal] fn long_press_finished();` + GDScript `connect`） | **能** | `ok 自造信号 long_press_finished 在 get_signal_list 里`、`ok 机制 1：GDScript 收到自造信号 input_selected("X")` |
| 2 | **连引擎原生信号**（`self.base_mut().connect(…)`） | **能**（三种形态） | `ok ready 把 pressed 连到了自己的同名方法（is_connected 为真）`、`ok ready 连上了 3 条子节点信号（实际 3）` |
| 3 | **外部注册闭包表**（原版 `Vec<Box<dyn FnMut(&str)>>`） | **Rust 侧能原样用；GDScript 交不进来** | `ok 机制 3 原版形态：Box<dyn FnMut(&str)> 收到 "X"` vs `ok subscribe() 的参数类型是 Callable` |

> 档位（`AGENTS.md` §0.3）：**观测** = 本轮 headless 真跑出来的输出；**推断** = 有依据未证；
> **未知** = 没解出来。`cargo test` / `selftest` 的条数**只是回归保护**，不是复原证据。

### 36.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/_scaffold/graphwar_button_class.rs` | **新增 619 行**（代码 326 / 接线代码 268 / 其中单测 58） | `GraphwarButton : Button`，3 条原生信号接线 + 两张回调表 + 2 个自造信号 |
| `src/_scaffold/graphwar_function_input_class.rs` | **新增 545 行**（代码 309 / 接线代码 262 / 单测 47） | `GraphwarFunctionInput : LineEdit`，3 条原生信号（**第 3 条连不上，见 §36.5**）+ 28 条折叠 + `fire` 回调 |
| `src/_scaffold/graphwar_setup_control_class.rs` | **新增 750 行**（代码 465 / 接线代码 383 / 单测 82） | `GraphwarSetupControl : Control`，**连子节点**的 3 条信号 + 四格模式 + 玩家条目 + 4 个自造信号 |
| `src/_scaffold/mod.rs` | **+8**（3 行 `pub mod` + 2 行注释） | 三个类都在 `#[cfg(feature = "gdext-bindings")]` 下 |
| `client/tools/signal_classes_check.gd` | **新增 579 行**（代码 402 / 注释 100 / 空行 77） | 155 条断言的验收脚本 |
| `docs/16-client.md` | +本节 | — |

`src/graphwar_button.rs` / `src/graphwar_function_input.rs` / `src/graphwar_setup_control.rs`
**一行没动**（仍然零引擎依赖）；`src/lib.rs` 没动；`client/` 下既有 `.gd` / `.tscn` /
`run.sh` 一个没改；**没有**新增 `docs/*.md`。

### 36.2 a) 三个类都能注册 + 实例化（观测）

```text
$ cargo build --release --features gdext-bindings
$ cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
$ godot --headless --path client --import
$ godot --headless --path client --script res://tools/signal_classes_check.gd
Initialize godot-rust (API v4.6.stable.official, runtime v4.7.2.stable.official, safeguards balanced)

ok   ClassDB.class_exists("GraphwarButton")
ok   ClassDB.class_exists("GraphwarFunctionInput")
ok   ClassDB.class_exists("GraphwarSetupControl")
--- A. GraphwarButton ---
ok   GraphwarButton 能实例化且是 Button
      btn.get_class() = GraphwarButton
ok   is_class("BaseButton") 为真（父类正确）
--- B. GraphwarFunctionInput ---
ok   GraphwarFunctionInput 能实例化且是 LineEdit
ok   GraphwarFunctionInput 是 LineEdit                       # 17 个方法与 1 个属性逐一断言
--- C. GraphwarSetupControl ---
ok   GraphwarSetupControl 能实例化且是 Control
SIGNAL_CLASSES 155 passed, 0 failed        # 退出码 0
```

三个类**各自**的"和原版对不对得上"（脚本里逐条断言，这里摘关键）：

```text
ok   ready 把 pressed 连到了自己的同名方法（is_connected 为真）      # 原版 SIGNAL_LINKS 第 1 条
ok   ready 把 button_up 连到了自己的同名方法（is_connected 为真）
ok   ready 把 button_down 连到了自己的同名方法（is_connected 为真）
ok   dispatch_log == "X|S"（先点一下的 input_text，后长按的 long_press_input_text；实际 "X|S"）
ok   long_press_emit_count() == 1（一次性，只发一次）
ok   再等 2 帧仍是 1 次（不重复派发）
ok   去空格后的表达式 == "sin(x)"（实际 "sin(x)"）
ok   PI 与 π 原样保留（π→n 在 MathFunction::new 里，不在输入框）
ok   â（0xE2）折成 "a"（实际 "a"）
ok   上标 ²（0xB2）折成 "2"（实际 "2"）
ok   '^' 在条件为真时插入 "^"（实际 "^"）
ok   时限循环到 Timer2m ⇒ 标签 01:59（实际 "01:59"）
ok   第 6 个士兵加不进去（饱和在 5）
ok   没有玩家时**不发** start_requested（原版是纯静默，不给提示）
```

⇒ 三个类的**取值/时序**都是从 `src/graphwar_*.rs` 里调出来的同一份纯逻辑
（`ButtonState` / `strip_spaces` / `intent_for_key` / `SetupOptions` / `PlayerEntry`），
本层一行算式都没复制。**这一点能证明**"纯逻辑 → 节点 + 信号"这条链路通了；
**不能**证明原版 UI 长什么样（子节点是替身、图标是路径字符串、聊天日志只有一行）。

### 36.3 b) **三种机制逐条（本节的核心）**

#### 36.3.1 机制 1：自造 `#[signal]` —— **能**（观测）

四件事全验过：**注册**、**带参**、**GDScript `connect`**、**两条发射路径**。

```text
ok   自造信号 long_press_finished 在 get_signal_list 里
ok   自造信号 input_selected 在 get_signal_list 里
ok   基类 BaseButton 的 pressed/button_up/button_down 三条信号还在     # 没被顶掉
ok   自造信号 input_selected 带 1 个 String 参数（实际 1）
ok   GDScript connect("long_press_finished", cb) 返回 OK（0）
ok   GDScript connect("input_selected", cb) 返回 OK（0）
ok   机制 1：GDScript 收到自造信号 input_selected("X")
ok   机制 1：GDScript 收到长按信号 1 次（实际 1）
ok   GDScript connect("options_changed", cb) 返回 OK（4 个 int 参数）
ok   GDScript 收到 options_changed(3, 0, 0, 2)（实际 [[3, 0, 0, 2]]）
```

两条发射路径**都对**，写法不同（本仓库两个类各用一种，方便以后挑）：

| 写法 | 代码 | 出处 | 结论 |
|---|---|---|---|
| **类型安全生成器** | `self.signals().input_selected().emit(&payload)` | `graphwar_button_class.rs` / `graphwar_setup_control_class.rs` | **能**（`WithUserSignals::signals(&mut self)` 要 `&mut self`） |
| **裸 `emit_signal`** | `self.base_mut().emit_signal("expression_changed", &[payload.to_variant()])` | `graphwar_function_input_class.rs` | **能**，而且**返回 `Error`**（可以记账，`emit_errors()` 是空的） |

三条**坑**（都不报错、只静默出错的那种）：

1. **带参数的信号要传引用**。gdext 把信号参数生成成 `impl AsArg<T>`，`GString` 的 `Pass` 是
   `ByRef` ⇒ `emit(payload)` 编译不过（`E0271: expected ByValue, found ByRef`），必须
   `emit(&payload)`。`i64` 参数没这个问题。
2. **生成器借的是 `&mut self`** ⇒ 发信号那一刻**不能再读 `self` 的字段**。所有要发的值
   必须先算成局部变量（本文件里 `let axis = …; let function = …;` 四行就是为了这个）。
3. **自造信号名不能与基类已有信号同名**。`BaseButton` 已经有 `pressed` / `button_up` /
   `button_down` / `toggled` ⇒ 本类自造的两个只能叫 `long_press_finished` /
   `input_selected`。**但信号名与自己的方法名同名是允许的** —— 原版就是这么用的
   （`connect("pressed", Callable::from_object_method(self, "pressed"))`，信号名与处理函数名
   是**同一个 `&str`**），本轮实测这套写法在 gdext 上照跑（`is_connected` 三条全真）。

#### 36.3.2 机制 2：连引擎原生信号 —— **能**（观测；三种形态都验了）

| 形态 | 谁连谁 | 证据 |
|---|---|---|
| ① 连**自己**的、**同名**方法 | `self.base_mut().connect("pressed", callable(self,"pressed"))`（原版 `GraphwarButton::ready`） | `ok ready 把 pressed 连到了自己的同名方法（is_connected 为真）` ×3 |
| ② 连**自己**的、**不同名**方法 | `connect("text_changed", callable(self,"on_text_change"))`（原版 `GraphwarFunctionInput::ready`） | `ok ready 把 text_changed 连到了 on_text_change（is_connected 为真）` |
| ③ 连**别人的**信号（子节点） | `find_child("RoomNameLineEdit") → node.connect("text_changed", callable(self,"on_text_change"))`（原版 `GraphwarSetupControl` 的 `FN_SETUP_WIRING`） | `ok ready 连上了 3 条子节点信号（实际 3；缺失: "")` + 三条 `is_connected` |

判据一律用 `Object.is_connected(信号名, Callable(节点, 方法名))` —— 它比"计数变了"硬：
计数可能因为别的原因变，`is_connected` 直接问引擎"这条边在不在"。

另外**虚方法**（不是信号）也验了：`_gui_input` / `_process` / `_ready` 都是**引擎自己调的**
（`ok _process 虚方法被引擎调到（3 次）`）。而 `_gui_input` 这条路本轮**第一次真的被驱动了**：

```text
      Input.parse_input_event(ê) : gui_input_call_count 0 -> 1, text="eê"
ok   机制 2'：_gui_input 虚方法**被引擎真的派发了**（Input.parse_input_event ⇒ 0 -> 1）
```

⇒ **headless 里可以造真输入**：`Input.parse_input_event(InputEventKey)` → 主循环 →
`Viewport` → `gui_input`。这条以前只在 `entity_drawer_check.gd` 里靠"等两帧"间接验过
`_process`，现在**键盘事件**也能造。

⚠️ 但注意 `text="eê"`：我们折出来的 `e` 与**基类 `LineEdit` 自己插的 `ê`** 都在。
⇒ GDExtension 覆写 `_gui_input` **不是**"替换基类的处理"，基类照样处理同一次按键。
原版插完字之后有没有 `accept_event()` / `set_input_as_handled()` **没有证据**
（`FN_GUI_INPUT` 只解到"插入 + `call("force_update_cb")`"）⇒ 原版**大概也双插**，
这一条登记为**未知**，别当成"我们多插了一个"。

#### 36.3.3 机制 3：外部注册闭包表 —— **一半能**（这是本轮最需要如实说的一条）

原版 `GraphwarButton` 用的是 `Vec<Box<dyn FnMut(&str)>>`（`+0x38/+0x40/+0x48`），
`GraphwarFunctionInput` 用的是**一个** `dyn` 回调（`[self+0x68]`，按名字调 `"fire"`）。
它们**都不是** Godot 的 `connect`。

| 问题 | 结论 | 证据 |
|---|---|---|
| **这个类型在 gdext 的类里能不能原样写** | **能** | 字段就是 `subscribers: Vec<Box<dyn FnMut(&str)>>` / `fire_native: Option<Box<dyn FnMut(&str)>>`，编译通过、运行时真的被调到：`ok 机制 3 原版形态：Box<dyn FnMut(&str)> 收到 "X"` |
| **GDScript 能不能往里塞闭包** | **不能** | `ok subscribe() 的参数类型是 Callable —— GDScript 交给它的只能是 Callable`；`ok subscribe_native_probe() **没有参数** —— 闭包只能在 Rust 侧构造` |
| **Godot 侧看不看得见这张表** | **看不见** | `ok Vec<Box<dyn FnMut(&str)>> **不在**属性表里（不是 #[export]，Godot 侧不可见）` |
| **替代方案** | `Vec<Callable>` / `Option<Callable>`，`callable.call(&[text.to_variant()])` | `ok 机制 3 替代方案：GDScript 的 Callable 收到 "X"`、`… 收到**带空格的原文**（实际 " s in ( x ) "）` |

**为什么交不进来**（三条，都是可核对的硬事实，不是推测）：

1. 跨 FFI 的**只有 `Variant`**。`Variant` 的类型表是 Godot 定义的固定枚举
   （`Callable` / `Signal` / `Rid` / 内建类型 / `Object`），**没有**"任意 Rust 闭包"这一项
   ⇒ 一个 `Box<dyn FnMut(&str)>` 没有任何一条路能被 GDScript 构造出来。
2. gdext 的 `#[func]` 参数必须实现 `GodotConvert`；`Box<dyn FnMut>` 不实现，也**不该**
   实现（`FnMut` 要 `&mut` 收，跨语言调用的生命周期没有对应的 Godot 语义）。
3. 反过来，GDScript 里能给的**唯一**"可调用物"就是 `Callable`（方法 / lambda / 内建方法）。

**三个必须知道的限制**（都实测过，不是听说的）：

* **闭包不能捕获 `Gd<Self>`**。`reentrancy_probe()` 在 `#[func](&mut self)` 里对同一实例再
  要一次 `bind_mut()`，实测：
  ```text
        reentrancy_probe() = panic: Gd<T>::bind_mut() failed, already bound;
        T = ...GraphwarButton. Details: cannot borrow while accessible mutable borrow exists.
  ok   闭包捕获 Gd<Self> 再 bind_mut() 会 panic（被 catch_unwind 接住）
  ```
  所以本文件的探针闭包捕获的是 `Rc<RefCell<Vec<String>>>` 这一层间接。
  （**这条 panic 被 `catch_unwind` 抓住了**，进程没死 —— 那个 `#[func]` 的返回值就是证据。）
* **表的长度不归引擎管**。`Vec` 只说 Rust 侧有几个，`+0x48` 那个"长度"要自己同步
  （`state.subscribers = subscribers.len() + callables.len()`）。
* **`Callable` 不能当 `#[export]` 属性**（gdext 的 `impl_property_by_godot_convert!` 对
  `Callable`/`Signal`/`Rid` 只生成 `SimpleVar`、**不生成 `Export`**，见
  `godot-core-0.5.5/src/registry/property/mod.rs` 的 `no_export` 分支）⇒ 想暴露给编辑器
  只能用 `#[var]`。§34.6 说「`GString` 属性没验过」，本轮**验了：能**（`ok 属性 input_text
  往返一致（"X"）`），`bool` 也能（`ok 属性 caret_condition 往返一致（true）`）。

#### 36.3.4 机制 3 在 `GraphwarSetupControl` 里**不存在**（照实说）

纯逻辑文件里 `FnMut` / `Box<dyn>` 命中 **0 处**（`grep` 可复核）。原版这一屏只有
3 个 `get_node` + 3 条 `connect`（`FN_SETUP_WIRING`），所以本类的接线**全走机制 2**，
本文件**没有**为它编一张回调表。

### 36.4 c) 每个类"真实接线代码"多少行

先给实测分类（`python` 数的：非空非注释行 = 代码；`mod tests` 之后算单测）：

| 文件 | 总行 | 代码 | 其中**接线**（不含单测） | 其中单测 | `//!` + `///` + `//` |
|---|---|---|---|---|---|
| `graphwar_button_class.rs` | 619 | 326 | **268** | 58 | 226 |
| `graphwar_function_input_class.rs` | 545 | 309 | **262** | 47 | 180 |
| `graphwar_setup_control_class.rs` | 750 | 465 | **383** | 82 | 206 |
| `entity_drawer_class.rs`（§34 的模板，对照） | 677 | 291 | **~155**（§34.3 的口径） | 60 | 330 |

**能推广的结论**：

* **注册 / 属性表 / 方法表 / 虚方法表 / 信号表一律 0 行**（gdext 宏全包）。本轮新增了
  「信号」这一项 —— `#[signal]` 也只占 **1 行**，注册、参数类型、生成器全是宏产物。
* 三个类的接线代码比模板的 155 行**多**，原因不是"接线更复杂"，而是：
  ① 每个类都带**一批单行观测点**（`ready_call_count()` / `pressed_call_count()` /
  `wired_count()` …）：三个类合计 **81 个 `#[func]`，其中 34 个是观测点**
  （`*_count` / `*_captured` / `*_log` 三类，`grep` 可复核）——
  这是 §34 定的规矩（没有计数分不清"引擎没回调"和"回调了但没干活"），不是业务代码；
  ② `GraphwarSetupControl` 多出**替身子节点**（`build_stub_children`，30 行）与
  四格/玩家条目的 `#[func]` 搬运（约 120 行）。
* **纯逻辑仍然一行没复制**：`ButtonState`（4 个判定）、`strip_spaces`、`COMPOSED_FOLDS`、
  `intent_for_key`、`SetupOptions` / `TimeMode::next` / `PlayerEntry` / `start_action`
  **全部**从 `src/graphwar_*.rs` 调过来。

### 36.5 d) 本轮新踩的坑 + **两个新发现**

**坑（都实测过）**

1. **`#[export]` 没有 setter 钩子**（gdext 生成的是直接字段访问）。GDScript 改
   `long_press_input_text` 之后，状态机自己那份拷贝**不会变** ⇒ 第一版当场红了
   （`FAIL long_press_input_text 为空时 process 不派发`）。修法：每次读之前
   `sync_from_properties()`（`process` 每帧一次）。**这是"导出属性 + Rust 侧镜像状态"
   这类类的通用坑。**
2. **`Node::find_child` 的 `owned` 默认是 `true`**（Godot 4.6 的默认参数），而代码里
   `add_child` 建出来的节点 `owner` 是 **null** ⇒ 直接 `find_child(name)` **永远找不到**。
   必须 `find_child_ex(name).owned(false).done()`。gdext 把默认参数生成成
   `…_ex()` builder（`recursive` 也是默认 `true`），**不看文档就会静默返回 None**。
3. **单测里不能碰 `GString`**。`GString::from("x")` 要调引擎的构造函数，`cargo test` 下直接
   panic：`Godot binding accessed before initialization or after deinitialization`
   （`godot-ffi-0.5.5/src/binding/single_threaded.rs:170`）。⇒ 纯 Rust 的那一半（`join_text`）
   必须返回 `String`，到 `#[func]` 边界才 `GString::from`。本轮为此改过一次代码。
4. **虚方法不能用 `Object.call("_gui_input", …)` 从 GDScript 调**：
   ```text
   SCRIPT ERROR: Invalid call. Nonexistent function '_gui_input (via call)' in base 'GraphwarFunctionInput'.
   ```
   而且**当前函数被整个中断**（后面 40 条断言一条都没跑，第一次跑就栽在这里）⇒ 这类实验
   必须**单独一个函数**、放在**全部断言之后**（脚本里的 `_probe_virtual_call()`）。
   顺带一条冷知识：**`_gui_input` 在 `get_method_list()` 里、但 `has_method()` 是 false**
   （虚方法进方法表、不进"可按名调用"的名字表）。
5. **`signal` 是 GDScript 关键字**：`for signal in [...]` 直接 `Parse Error: Expected loop
   variable name after "for"`。改叫 `sig_name`。
6. **`--import` + `cp .so` 两个静默陷阱照旧**（`AGENTS.md` 那三条）。本轮还多一条
   **环境坑**：`client/run.sh` 用 `$here/../..` 找 `tools/Godot_…`，在
   `.agent-work/<topic>/repo` 这种**深一层**的克隆里会指错 ⇒ 跑 selftest 要
   `GODOT=/path/to/Godot_…`（见 §36.7）。

**发现 1（原版的缺陷，不是我们的笔误）：`size_changed` 在 Godot 4.6/4.7 上**不是**信号**

`GraphwarFunctionInput::ready` 的第 3 条连接指向 `size_changed`。三条核过：

1. `.rodata` 字面量**确实是** `size_changed`（`0x1c46d2`，12 字节；紧挨
   `"gui_input"` 与 `"text_submitted"`，逐字节读出来的），处理函数 `on_size_changed`
   （`0x1c444f`，15 字节）也在；
2. 本机 Godot 4.7.2 的 `Control`/`LineEdit` **没有**这条信号（只有 `resized` /
   `size_flags_changed` / `minimum_size_changed`；gdext 生成的 `control.rs` 里也查不到
   `size_changed`）；
3. 实测 connect 结果：
   ```text
   ERROR: In Object of type 'GraphwarFunctionInput': Attempt to connect nonexistent signal 'size_changed' …
   ok   ready 只连上 2 条（第 3 条 size_changed 在 Godot 4.6+ 上没有这条信号）
        size_changed 的连接结果: "size_changed: ERR_INVALID_PARAMETER"（wired=2）
   ```

⇒ **原版这条 `connect` 在 4.x 上必然失败**，`on_size_changed` 在原版里**大概永远不会被调到**。
**推断**（缺证据）：这是从 Godot 3 迁移到 4 时的残留（Godot 3 的 `Control` 有
`NOTIFICATION_SIZE_CHANGED`，Godot 4 改成了 `resized`）。缺的证据：原版运行时的
stderr（要能在 Android 上抓到那条 `Attempt to connect nonexistent signal` 才算观测）。
本文件**照抄原版**（连 `size_changed`、失败就记进 `connect_errors()`），没有擅自改成
`resized` —— 改了就不是复原了。要让它真能工作，改法只有一行（`resized`），但那属于
**我方修订**，得单独登记。

**发现 2：headless 里 `Input.parse_input_event` 能驱动 `gui_input`，但会和基类双插**

见 §36.3.2 末尾（`text="eê"`）。这一条的价值是：**以后所有"键盘/鼠标驱动"的类都能在
`--script` 里真验**，不用只靠 `simulate_key` 这种自己造的口子。

### 36.6 三个类各自"没做/未知"的清单（照实写）

| 类 | 没做 / 未知 |
|---|---|
| `GraphwarButton` | 动画节点（`AnimationRect` → `GraphwarLongPressAnimation`）的**两个 `Instant` 字段没写**（那个类还没节点实现）；只 `find_child` 记住"有没有"，**故意不持句柄**（§34.4 坑 2 的野句柄） |
| `GraphwarFunctionInput` | `size_changed` 连不上（见上）；token 边界（`move_caret_*` / `delete_token_*` 的 N）**未解**，没实现；`process` 里与 `graphwar_game_client` 的数据流**没接**；`'^'` 的附加条件仍用 `#[export] caret_condition` 显式代替（语义**未知**） |
| `GraphwarSetupControl` | 11 个 `get_node` 里只建 3 个替身；**18 个 gdext 薄壳 + 6 个虚方法胶水 + 3 个事件回调的语义仍然未知**（`src/graphwar_setup_control.rs` 模块头那两张表，本文件**不声称**还原了它们）；4 个模式按钮**按证据不连** `pressed`（原版就没连）；玩家列表 5 个行构件只做数据层，不建行节点 |

### 36.7 复现（五分钟）+ 回归条数

```text
# 1) 打开 feature 构建（默认关闭 ⇒ 关着时 crate 仍然零依赖）
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --release --features gdext-bindings --offline
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so

# 2) 必须先 import（否则扩展静默不加载，脚本连类都认不出来）
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
godot --headless --path client --import

# 3) 验收（155 条断言，退出码 0）
godot --headless --path client --script res://tools/signal_classes_check.gd
# SELFTEST 风格的汇总行：SIGNAL_CLASSES 155 passed, 0 failed

# 4) 回归保护（三条硬要求）
cargo test --offline                            # lib 457 / 总计 1499（**与本轮之前逐条相同**）
cargo test --offline --features gdext-bindings   # lib 481（= 457 默认 + 6 §34 + 18 本节）/ 总计 1523
# selftest 要**默认 feature** 的 .so ⇒ 重编一次再 cp：
cargo build --release --offline
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
bash client/run.sh selftest                      # SELFTEST 618 passed, 0 failed, 2 skipped
```

⚠️ **selftest 的条数说明（2026-11-07 第二次更正，这次是 694 对的）**：

`618 passed / 0 failed / **2 skipped**` 与 `694 passed / 0 failed` **不是矛盾，是前置条件不同**——
差的 76 条全在那 2 个被 skip 的套件里，而它们 skip 的唯一原因是**没编 `examples/serve_match`**：

```text
bash client/run.sh selftest
  → 没编 example：SELFTEST 618 passed, 0 failed, 2 skipped
  → 先 cargo build --offline --example serve_match，再跑：SELFTEST 694 passed, 0 failed
```

2026-11-07 18:20 在本仓库实测确认：先把 `serve_match` 编出来，同一份 `client/`、同一个
默认 feature 的 `.so`，得到的就是 **694 passed, 0 failed**。所以 §31 / §34 / `docs/HANDOVER.md`
里写的 694 **没有过期**；需要补的是**这条前置条件**，而不是改数字。

另外两个会把条数弄错的环境陷阱（都实测过）：

* **不要覆盖 `XDG_DATA_HOME`**（例如指到仓库内的 `.godot-data`）——那会把 `user://replays`
  指到空目录，回放相关套件会红（历史记录里出现过 693/1）；
* 依赖 `client/assets/`（用户自己的 `Graphwar.pck` 解出来的）与 `extract_audio.py --import`
  的步骤，缺了也会有套件降级成 skip。

---

## 37. 四个「只多一个 `_draw`」的类成了真 Godot 节点（2026-11：第二～第五个垂直切片，235 条 headless 断言全过）

**一句话**：`EffectDrawer` / `GraphwarLongPressAnimation` / `GraphwarPlane` / `PlayerShowcase`
在 `gdext-bindings` 下各成了一个 `Control` 子类；`_draw` / `_process` 里发出的每一个图元、
每一次状态推进，**实参全部来自 `src/*.rs` 的纯逻辑**（`frame_primitives` / `draw_frame` /
`BOARD_*` / `draw_plan` / `step_effects` / `Showcase::tick`），四个类**没有第二份算术**。
headless 实测 **235 条断言全过**（退出码 0）。

> 档位同 §34：**观测** = 这轮 headless 真跑出来的输出；**推断** = 有依据未证；
> **未知** = 没解出来。`cargo test` / `selftest` 的条数**只是回归保护**，不是复原证据。

### 37.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/effect_drawer_class.rs` | **新增** | 592（类本体代码 239 / 单测 76 / 注释 196 / 空行 49） | 7 个相位 → `draw_circle` / `draw_polygon` / `draw_line`；`step` 走 `step_effects` |
| `src/_scaffold/long_press_animation_class.rs` | **新增** | 484（155 / 85 / 179 / 40） | 两次 `draw_arc`；`_process` + `advance` 推进相位；越过 2π 自己写哨兵 |
| `src/_scaffold/graphwar_plane_class.rs` | **新增** | 495（181 / 64 / 177 / 45） | 3 个占位子节点 + 棋盘框 `draw_rect` + 脏矩形队列 + `resized` |
| `src/_scaffold/player_showcase_class.rs` | **新增** | 665（289 / 75 / 206 / 62） | 读档案 + 3×`draw_texture_rect` + 计数器闸门 + `text_changed` 连线 |
| `src/_scaffold/mod.rs` | +12 | — | 4 个 `#[cfg(feature = "gdext-bindings")] pub mod` + 注释 |
| `client/tools/draw_classes_check.gd` | **新增** | 570（235 条断言） | headless 验收脚本 |
| `docs/16-client.md` | +本节 | — | — |

`src/effect_drawer.rs` / `src/graphwar_long_press_animation.rs` / `src/graphwar_plane.rs` /
`src/player_showcase.rs` / `src/graphwar_player_config.rs` **一行没动**（仍然零引擎依赖）；
`src/lib.rs` 没动；`client/` 下既有 `.gd` / `.tscn` / `run.sh` 一个没改。

### 37.2 四个类各自「真的画出东西了吗」—— 都画了（**观测**）

四个 `ClassDB.class_exists` / `instantiate` / `is_class("Control")` 全过（每个类一条），
下面是**每个类最要紧的那一条**：

```text
--- EffectDrawer
ok   相位 0：_draw 被引擎调到（2 次）
ok   相位 0：每次 _draw 发 1 条 draw_set_transform（2/2）
ok   相位 0：每次 _draw 发 1 个 draw_circle（2/2）
ok   相位 4：每次 _draw 发 1 条 draw_line（3/3）
ok   相位 1：顶点未复原 ⇒ 每次 _draw 记一次 skipped（3/3）
ok   7 个相位分档：圆 4 个（0/2/3/6）、线 2 个（4/5）、多边形 1 个（1）—— 实际 4/2/1
ok   给了 3 个顶点之后真的调了 draw_polygon（3 次，_draw 共 6 次）
ok   每次 _draw 要么画多边形、要么记 skipped，二者之和 == _draw 次数
ok   step(1.0) 推进 60 tick / 相位 2 到期后推进到 3（CircleLargeA → B）

--- GraphwarLongPressAnimation
      angle = 1.256637（期望 0.1·4π = 1.256637）
      draw +3, arc +6（期望 2 倍）
ok   运行中每次 _draw 发 2 条 draw_arc（+6 = 2×3）
ok   没 press 时一条 draw_arc 都不发（原版 0x4d8115）
ok   越过 2π 之后 finished_count ≥ 1（哨兵已写）
ok   结束之后再 _draw 也不发弧了（nanos == 1e9 直接返回）

--- GraphwarPlane
      子节点: Control(@Control@12), Control(@Control@13), Control(@Control@14)
ok   子节点个数 == 3（实际 3）            # 幂等：进树之后 `ready` 没有重复建
      脏矩形 = [570, 370, 630, 430]（期望 [570, 370, 630, 430]）
      plane draw=2 rect=2 process=2 obstacle_updates=4 ready=1
ok   每次 _draw 发 1 条 draw_rect（2/2）
ok   每帧刷 2 次障碍绘制器（4 次 / 2 帧）
ok   `resized` 连上了（on_resized 被调到 1 次）

--- PlayerShowcase
      ready 的 6 步: 取 GraphwarRootNode, 从档案读名字（共享借用）, 取 PlayerNameLineEdit,
                     LineEdit::set_text(名字), 再取 PlayerNameLineEdit, text_changed → on_text_change
      showcase draw=1 texture_rects=3 process=2
ok   第 1 步找不到 GraphwarRootNode（本类没有那个节点；原版这里会 .expect() 失败）
ok   ready 第 4 步把档案里的名字灌进了控件（"Bob"）
ok   ready 第 6 步把 text_changed 连到了 on_text_change
ok   每次 _draw 发 3 条 draw_texture_rect（3 = 3×1）
ok   计数器变了 ⇒ _process 请求重画（1 次）
ok   touches_rank() == false（不读段位 / 队色，有界否定）
      demo 子节点: sprites=yellow, regular_eyes, soldier_helmet label=1
DRAW_CLASSES 235 passed, 0 failed        # 退出码 0
```

> 上面是**节选**：省掉了每个类的 11～21 条「方法 X() 存在」（19 + 11 + 16 + 21 = 66 条）。
> 逐字全文用 39.7 的命令重跑即可。

五条结论（每条都指到上面某一行）：

| 结论 | 档位 | 证据 |
|---|---|---|
| 四个类都注册成 `Control` 子类，方法表齐全 | **观测** | 4×(class_exists + instantiate + is_class) + 66 条「方法存在」 |
| `EffectDrawer` 的 7 个相位各发**恰好**一条图元，多边形在缺顶点时**不调** | **观测** | `4/2/1` 分档 + `skipped == draws` + 占位顶点后 `draw_polygon` 真被调 |
| 长按动画每次 `_draw` 发 **2 条** `draw_arc`，越过 2π 后自己停 | **观测** | `arc +6 = 2×3` + `finished_count ≥ 1` + 停后再 `_draw` 不发弧 |
| 棋盘类建 **3 个**子节点、每帧刷 2 次、脏矩形进出、`resized` 真连上 | **观测** | `子节点 == 3` / `obstacle_updates=4 (2 帧)` / `flushed_total ≥ 1` / `on_resized +1` |
| 名片读档案 + 三层贴图 + 计数器闸门 + 信号连线 | **观测** | 三个默认外观名 + `3×draw` + `redraw_request_count` + `text_changed` 连线 |

**这张表能证明的是**「纯逻辑 → 引擎调用」这条链通了（四类各一种：图元 / 每帧相位 / 子节点 + 脏矩形 / 档案 + 信号）；
**不能**证明这些类长得对：贴图是程序生成的纯色、多边形顶点仍未复原、棋盘上没有曲线和士兵、
`GraphwarRootNode` / `FunctionDrawer` / `ObstacleDrawer` / `GraphwarBackend` 四个类都还不存在。

### 37.3 一处**就地更正**：那两次绘制是 `draw_arc`，不是「两层矩形」

`src/graphwar_long_press_animation.rs` 的 §0 写「画**两层矩形**（黑底 + 白层）」，
§6 把被调 API 记成**未知**（`call *[0x9bc020]`）。本轮按仓库已有的 GOT 槽表把它落定：

| # | 证据 | 内容 | 档位 |
|---|---|---|---|
| 1 | 槽 → 包装 | `draw_arc` 字面量 `0x19f9d0` → 包装 `0x8abca0` → GOT 槽 **`0x9bc020`**（`R_X86_64_RELATIVE` 写入） | **观测** |
| 2 | 调用点 | 全 `.text` 引用 `0x9bc020` 的 `call *…(%rip)` 只有 **5** 条：`EntityDrawer::draw` `0x4b2890` ×3 + `GraphwarLongPressAnimation::draw` `0x4d8100` ×2（`0x4d8485` 黑 / `0x4d84ed` 白） | **观测** |
| 3 | 形参对齐 | 6 个 f32（`xmm0..xmm5`）+ 1 个 int（`esi = 0x1e` = 30）+ 1 个颜色指针（`rdx`）+ 1 个 bool（`ecx = 0`）——与 `draw_arc(center, radius, start_angle, end_angle, point_count, color, width, antialiased)` 的形参表**逐槽对齐** | **观测**（个数/类型）/ **推断**（哪一槽是哪个形参） |

`draw_rect` 的实参是 4 个 f32 + 颜色 + 2 个标量，装不进 6 个 f32 槽；而「旋转角从 0 走到 2π」
这件事本身也只有在**弧**上才有意义 ⇒「两层矩形」那句应当作废（本轮**不改**纯逻辑文件，
只在本节记录；下次动那个文件时请连同 §0 / §6 一起改）。

### 37.4 两处**按证据**与任务书不同（改了什么、为什么）

任务书给这一批的四条指引里，有两条与观测不符。照 `AGENTS.md` §0.3 / §0.6 的规矩**按证据做、
就地写明**：

**(a) 多边形是 `draw_polygon`，不是 `draw_colored_polygon`。**
`docs/16` §18 + GOT 槽表：`draw_polygon | 0x29ef97 | — | 0x9bc238 | 1 | EffectDrawer::draw ×1（0x4e1d8f）`，
全 `.text` 只有这一个调用点；实参是 `(PackedVector2Array, PackedColorArray)`，
而 §18 记到「颜色 = packed/255，**3 份同色**」⇒ 颜色是**每个顶点一份**的数组。
`draw_colored_polygon` 只吃**一个** `Color`，形状对不上。
⇒ 本类调 `draw_polygon`，并把纯逻辑那 1 个颜色按**顶点数**复制（顶点数由调用方给，不硬编码 3）。

**(b) `PlayerShowcase` 原版**没有** 3 个 `Sprite2D` + 1 个标签。**
那是**兄弟类 `EntityDrawer`** 的形状（§21：「士兵不是 `draw()` 画的，是 3 个 `Sprite2D` + 1 个
`RichTextLabel` 子节点」）。`PlayerShowcase` 的 `draw` 是三次 `draw_texture_rect_full` 画在
**自己身上**（arm64 `0x4402b0` / `0x44037c` / `0x440444`），`ready` 里只**取**（不建）一个
`PlayerNameLineEdit`。
⇒ 本类**默认路径 = 证据**（`_draw` 里三次 `draw_texture_rect`）；任务书要的那个形状以
`build_demo_children()` 提供（**显式 opt-in**、文档与代码里都标**非原版**、默认不建），
验收脚本照样断言「3 个 `Sprite2D` + 1 个 `Label`」。
这样「默认跑起来 == 原版形状」，而「任务书要的形状」也有可断言的方法。

### 37.5 每个类多少行 —— 「接引擎」的代码只有 864 行里的三分之一

| 文件 | 总行 | **类本体代码** | 单测代码 | 注释 | 空行 |
|---|---|---|---|---|---|
| `effect_drawer_class.rs` | 592 | **239** | 76 | 196 | 49 |
| `long_press_animation_class.rs` | 484 | **155** | 85 | 179 | 40 |
| `graphwar_plane_class.rs` | 495 | **181** | 64 | 177 | 45 |
| `player_showcase_class.rs` | 665 | **289** | 75 | 206 | 62 |
| **合计** | **2236** | **864** | 300 | 758 | 196 |

对比 §34.3 的 `EntityDrawer`（677 行 / 本体代码 291）：**这四个类加起来 864 行**，
平均每个 216 行——比 §34 那一个还小，因为**注册、属性表、方法表、虚方法表一律 0 行**
（gdext 宏全包），真正要手写的只有「把纯逻辑的返回值搬进 `draw_*` / `add_child` / `connect`」。
四类的纯逻辑（相位跳表、角度算式、棋盘常量、档案偏移、绘制顺序）**一行没写**——全在 `src/*.rs` 里。

### 37.6 本轮新踩的坑（每条都是实测，不是听说）

1. **`step_effects` 会**在验收脚本背后**改相位**。`STATE_TERMINAL_MASK = 0x5b` ⇒ 相位
   0/1/3/4/6 是终态，**2 和 5 不是**；`_process` 每帧调 `step`，只要 `start_tick <= elapsed`
   就换相位（2→3、5→6）。实测现象：给相位 5 建一个元素、等两帧，**3 次 `_draw` 里只有 1 次
   发 `draw_line`**（后两次元素已经变成相位 6 的圆）。
   ⇒ 要断言「**某个相位**画什么」，必须让 `start_tick` 落在**未来**
   （`set_now_tick(1e6)` → `spawn_effect` → `set_now_tick(0)`），否则测的是相位推进。
2. **`60.0 × (1.0 / 60.0)` 在 f64 里截断后是 1 tick，不是 0。** 单测里按「略小于 1」写过一次
   `assert_eq!(ticks_for_delta(1.0/60.0), 0)`，红了；`ticks_for_delta(1.0/600.0)` 才是 0。
   ⇒「一帧等于几 tick」由浮点舍入决定，别用整除猜。
3. **Godot 4.7 的 `LineEdit.set_text()` 不发 `text_changed`**（属性 `text = ...` 也一样）。
   实测：`edit.text = "Carol"` 之后 `on_text_change` 一次都没被调到。headless 里没有键盘，
   要验「信号连对了」只能 `edit.emit_signal("text_changed", "Carol")`。**这条要记住：
   原版这条链是用户输入驱动的，headless 断言的只能是「连线存在且回调正确」。**
4. **`base_mut()` 的独占借用连 `&self` 的方法都挡。** `PlayerShowcase::_draw` 里先
   `let mut base = self.base_mut();` 再在循环里调 `self.texture_for(..)`（`&self`）⇒ `E0502`。
   解法：贴图在 `base_mut()` **之前**一次性取成 `Vec`。这是 §34.4 第 6 条（`E0499`）的孪生坑——
   那次是 `&mut self` 方法，这次连 `&self` 都不行。
5. **`Gd::instance_id()` 返回 `InstanceId`（`.to_i64()`），没有 `Object::get_instance_id()`。**
   gdext 0.5 里 `Gd<T>` 上的是 `instance_id()`；`get_instance_id()` 是 C++ 侧的名字，编译不过。
6. **`PackedStringArray::push` 吃 `&GString`**（`AsArg<GString>` 的 `Pass` 是 `ByRef`）；
   直接传 `GString` 会 `E0271: <GString as ToGodot>::Pass == ByValue` 不满足。
7. **`draw_polygon` 少于 3 个顶点会打印引擎错误**（`CanvasItem::draw_polygon` 的
   `ERR_FAIL_COND_MSG`）。顶点没复原时**不要**「调一下看它抱不抱怨」——本类改成
   `points.len() < 3` 就跳过并记 `polygon_skipped_total`。
8. **跑 `selftest` 时不要覆盖 `XDG_DATA_HOME`。** 实测：手工 `export XDG_DATA_HOME=...` 之后
   `user://replays` 指到了一个含 `client/` 文件的目录，回放列表那条断言直接红
   （`SELFTEST 693 passed, 1 failed`）；用 `run.sh` 自己的默认值就是 **694 / 0**。
9. **音频资产必须「先解、再 import」。** 先 `--import` 后解音频，`GwAudio` 仍然报
   `缺资产`（10 条 audio 断言全变 SKIP，`SELFTEST 686 passed`）；解完**再跑一次** `--import`
   才是 694。/tmp 之外的产物与 `.godot/` 都进了 gitignore，不会污染提交。

### 37.7 复现（三分钟）

```text
# 1) 打开 feature 构建（默认关闭 ⇒ 关着时 crate 仍然零依赖）
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --release --features gdext-bindings --offline
cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so

# 2) 必须先 import（否则扩展静默不加载，脚本连类都认不出来）
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
godot --headless --path client --import

# 3) 验收（235 条断言，退出码 0）
godot --headless --path client --script res://tools/draw_classes_check.gd

# 4) 回归保护（三条硬要求，缺一样都跑不到文档里的数字）
cargo test --offline                           # lib 457 / 总计 1499
cargo test --offline --features gdext-bindings  # lib 488（457 − 手写 gdext 的 4 + 本批 35）/ 总计 1530
bash client/run.sh selftest                    # 694 passed（默认 feature 的 .so；别覆盖 XDG_DATA_HOME）
```

### 37.8 还没做 / 未知（诚实清单）

* **多边形顶点公式**（`effect_drawer` 模块 §5）：只读到两组点各乘 `0.2`。本类**不发明**顶点，
  由调用方给；`polygon_skipped_total` 就是「欠了多少几何」的计数器。
* **`draw_line` 的端点**（相位 4/5）：纯逻辑给的是 `from == to == center` 的退化线段；
  原版端点未复原，本类照发（每次 `_draw` 真的调一次 `draw_line`）。
* **`EffectDrawer` 的 tick 从哪来**：原版读 `GraphwarModuleContainer` 那个模块的 `+0x58`，
  本类自己数（`step(delta)`）。
* **障碍**：`step_effects` 的 `is_obstacle` 恒 `false`——`GraphwarObstacleManager` 还没接。
* **`ready` 的 11 个后端事件**（`EffectDrawer` 2 个 + `GraphwarPlane` 3 个 + `EntityDrawer` 6 个）
  一个都没订：要 `GraphwarBackend` 到节点层。
* **`GraphwarLongPressAnimation` 的三次取值**：API 名与语义仍**未知**；本类用
  `get_position()` / `get_size()` 顶前两对，`n` / `c` 做成导出属性。
* **`GraphwarPlane` 的曲线 / 障碍 / 士兵**：一个都没有。`FunctionDrawer` / `ObstacleDrawer` 类
  还不存在，3 个子节点是 `Control` **占位**（自动名 `@Control@N`，名字里的类名就是"还缺什么"）。
* **`PlayerShowcase` 的贴图加载器**：`GraphwarTextureLoader`（根节点 `+0x16a0`）没有，本类用
  程序生成的纯色 `ImageTexture`；`save()` 也不写盘（headless 验收不该有副作用）。
* **`GraphwarRootNode`**：`PlayerShowcase::ready` 第 1 步真的会失败（`root_lookup_failures`），
  原因是**那棵树里没有那个节点**——类本身在 §35 那一轮就做出来了（`ClassDB.class_exists("GraphwarRootNode")`
  一直为真）。本条原文写的"因为那个类还没做"是误记，已在 §38.9 就地更正。

## 38. `GraphwarRootNode` 真装配：3 个 manager 进容器、Setup 屏 → Game 屏、3 个假士兵（2026-11：86 条 headless 断言全过）

**一句话**：此前 12 个 GDExtension 类是**孤立节点**——`GraphwarRootNode` 里那 25 个模块槽
全是空 `Node` 占位，容器的 `add_module` 一次都没被调过。本节把**能开一局的最小集合**
（`GameStateManager` → `TurnManager` → `PlayerManager`）真装进 `GraphwarModuleContainer`，
并把两张屏接上真节点类：**Setup 屏**（`GameSetup`）= `GraphwarSetupControl`，
**Game 屏**（`PlayScreen`）= `GraphwarPlane` + 它的子节点 `EntityDrawer` + **3 个假实体**
（每个 3 个 `Sprite2D` + 1 个 `RichTextLabel`）。headless 实测 **86 条断言全过**（退出码 0）。

> 档位同 §34/§37：**观测** = 这轮 headless 真跑出来的输出 / 能指到 `docs/02` §22 的指令级证据；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / `selftest` 的条数**只是回归保护**，
> 不是复原证据（`AGENTS.md` §0.2）。

### 38.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/root_wiring.rs` | **新增** | 499（代码 149 / 单测 136 / 注释 162 / 空行 52） | 容器装载器：`TypeId` 注册/取回 + 3 个 manager 的装配 + 槽位三态判据 |
| `src/_scaffold/graphwar_root_node_class.rs` | 改 | +318 / −11 | `ensure_scene_graph` 里装配容器；`GameSetup` / `PlayScreen` 换真内容；10 个 `#[func]` 观测口 |
| `src/_scaffold/mod.rs` | +4 | — | `#[cfg(feature = "gdext-bindings")] pub mod root_wiring;` + 注释 |
| `client/tools/assemble_check.gd` | **新增** | 335（86 条断言） | headless 验收脚本 |
| `docs/16-client.md` | +本节 | — | — |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑生成物 | — | `src_files` 104 → **105**、`src_fns` 2171 → **2209**（只这两个数变了；527 个可归属函数与档位分布一位没动） |

`src/graphwar_*.rs` 的纯逻辑文件、`src/lib.rs`、其它 `*_class.rs`、`client/src/**`、
`client/*.tscn`、任何既有断言**一行没改**。

### 38.2 「按类型查到 3 个」到底查的是什么（本节的核心）

容器是 `HashMap<TypeId, Rc<dyn Any>>`（1 字段 / 48 字节，`docs/02` §22.1 的**否定结论**：
**没有** `+0xNN` 偏移、**没有** 25 个字段）。所以"装配"只有一个动作：

```text
container.add_module::<M>(Rc::new(RefCell::new(M::new(…))))
        ↑ 键 = TypeId::of::<M>()，值 = 那个 Rc<RefCell<M>> 的胖指针
container.get_module::<M>() -> Option<Rc<RefCell<M>>>
        ↑ 命中后经 vtable 第 4 字调 Any::type_id，与 TypeId::of::<RefCell<M>>() 比
```

本节装的 3 个（类型名 = `graphwar_module_container::MODULES` 的**原文**）：

| 容器里的键（原版类型名） | 我方等价类型 | 我方 `new` | 取回后这一轮真的读到了什么 |
|---|---|---|---|
| `GraphwarGameStateManager` | `crate::game::Match`（那个模块"运行实体那一半"） | `Match::new(room, obstacles, soldiers, tick_base)` | 3 个 `Target`（`soldiers.len() == 3`） |
| `GraphwarTurnManager` | `graphwar_turn_manager::TurnManager` | `TurnManager::new()` | 玩家 1 = 2 个士兵、玩家 2 = 1 个、玩家 99 = `-1` |
| `GraphwarPlayerManager` | `graphwar_player_manager::GraphwarPlayerManager` | `GraphwarPlayerManager::new()` | `player_id → connection_id` 2 行 |

验收脚本**逐型**断言（`container_has_module("GraphwarTurnManager")` …），**不数总数**；
同一时刻 `container_has_module("GraphwarBanList")` 必须是 `false`、`container_module_count()`
才是 `3` —— 这一对正反断言就是"判据是 `TypeId` 而不是计数"的证据：

```text
ok   container_module_count() == 3（实际 3）
ok   wired_manager_count() == 3（**按类型查**出来的）
ok   容器里按类型查得到 GraphwarGameStateManager
ok   容器里按类型查得到 GraphwarTurnManager
ok   容器里按类型查得到 GraphwarPlayerManager
ok   没装的 GraphwarBanList 查不到（⇒ 判据是类型，不是数量）
ok   胡编的名字 GraphwarNotARealManager 查不到
ok   取回 GameStateManager ⇒ 里面是 3 个假士兵（实际 3）
ok   取回 TurnManager ⇒ 玩家 1 有 2 个士兵
ok   取回 PlayerManager ⇒ player_id → connection_id 表有 2 行
ok   GraphwarBanList 的槽位状态 = placeholder（**如实标**：没装）
ok   GraphwarNotARealManager 的槽位状态 = unknown（原版 25 型里也没有）
```

槽位三态由 `module_slot_status(name)` 回答，判据是**运行时**的（`assembled` 只在
`get_module::<M>()` 真能取回时给出）：`assembled`（3 个）/ `placeholder`（原版 25 型里
有、我们没装的 22 个）/ `unknown`（原版台账里也没有的名字）。

> ⚠️ **§47 起作废这一段**：§41 已把槽位扩到 22 个，§47 起 `module_slot_status()` 报**四档**
> （`wired` / `layout` / `assembled` / `stub`，判据见 §47.1），不再是三态。旧句保留备查。

⚠️ **装配顺序**（`GameStateManager → TurnManager → PlayerManager`）是**我方选择**（= 任务书顺序），
**不是**观测到的原版顺序：`docs/02` §22.4 只给出两侧 22 条 `add_module` 的**调用序相同**与
尺寸序列，没有做 22 条逐型对齐。原版是否让 `GameStateManager` 与容器里的 `TurnManager`
**共享**同一个实例：**未知**（我方 `Match` 自带一个内部 `TurnManager`，容器里的是另一个）。

### 38.3 两张屏：Setup 屏 → Game 屏（切换仍然走 `0x3af850` 的决策表）

没有新增切屏逻辑：`show_screen(name)` 一直是 `graphwar_root_node::apply_show_screen`
（`0x3af850` 的纯逻辑版，§15）。本节只是把两条屏的内容换成了真节点：

```text
ScreensControl/GameSetup   → GraphwarSetupControl（§35 那个类；四格 / 玩家表 / Start）
ScreensControl/PlayScreen  → Control 容器
    └── GraphwarPlane                 ← 有名字（原版 dump 里就叫这个）
        └── @EntityDrawer@N           ← **不** set_name（原版也没命名，§2.1.10 的 dump）
```

父子关系与命名都是**观测**（`docs/16` §2.1.10 的运行时节点树：
`GraphwarPlane/@EntityDrawer@N/@Sprite2D@N`、`GraphwarPlane/@EntityDrawer@N/<name_label 实例>`）；
"Game 屏 = `PlayScreen`"这一条也是**观测**（同一张表里战场挂在 `PlayScreen/.../GraphwarPlane`）。

```text
ok   show_screen(GameSetup) 遍历了 15 个屏（实际 15）
ok   Setup 屏可见
ok   Game 屏不可见（切屏互斥）
ok   visible_screens() == [GameSetup]（实际 ["GameSetup"]）
ok   show_screen(PlayScreen) 遍历了 15 个屏（实际 15）
ok   Game 屏可见
ok   Setup 屏不可见（切屏互斥）
ok   visible_screens() == [PlayScreen]（实际 ["PlayScreen"]）
ok   Setup 屏是 GraphwarSetupControl（真节点类，不是空 Control）
```

### 38.4 Game 屏里的棋盘与 3 个假实体

```text
ok   Game 屏里 GraphwarPlane 恰好 1 个（实际 1）
ok   棋盘节点名 == GraphwarPlane（原版 dump 里的名字）
ok   棋盘下 EntityDrawer 恰好 1 个（实际 1）
ok   EntityDrawer 是 GraphwarPlane 的**子节点**（原版 dump 如此）
ok   EntityDrawer 是引擎自动名 @EntityDrawer@N（原版没 set_name，实际 @EntityDrawer@19）
ok   EntityDrawer 里有 3 个实体（实际 3）
ok   3 个实体 × 4 个子节点 = 12（实际 12）
ok   实体 1 的 4 个子节点 = 3 Sprite2D + 1 RichTextLabel（实际 3 sprite / label=true）
ok   实体 1 的名字标签文本 == P1-1（实际 P1-1）        # 2/3 同理
ok   贴图子节点是引擎自动名 @Sprite2D@N（原版 dump 如此，实际 @Sprite2D@2）
```

* **假实体**：id `1/2/3`、队伍一两人（`x ≤ 0`）/ 队伍二一人（`x ≥ 0`，照 `Team` 的观测判据）、
  名字 `P1-1` / `P1-2` / `P2-1` —— 坐标与名字都是**我方选择**（原版的名字来自
  `GraphwarNameManager` / backtracer，那条链不通）。
* 这 3 个假实体与容器里 `GraphwarGameStateManager` 的那 3 个 `Target` 是**同一份数据**
  （`root_wiring::DEMO_SOLDIERS`），所以"容器里 3 个士兵"和"屏上 3 个士兵"能对上号。

### 38.5 每帧：Plane 的 `_draw` 真的被调到

```text
      plane: draw=1 rect=1 process=1 ready=1
ok   棋盘在树里且可见（否则 _draw 不会被调）
ok   Plane 的 _draw 被引擎调到（1 次）
ok   每次 _draw 发 1 条 draw_rect（1/1）
ok   Plane 的 _process 被引擎调到（1 次）
ok   EntityDrawer._ready 被引擎调到（1 次）
ok   EntityDrawer._process 被引擎调到（1 次）
ok   进树后棋盘有 4 个子节点（3 个 FunctionDrawer 占位 + 我们的 EntityDrawer，实际 4）
```

`_draw` 只在"该重画"时才来（`CanvasItem` 的语义），所以 2 帧后计数仍是 1 ——
脚本断言的是"**被调到过**（≥1）且计数只增不减"，不是"每帧 +1"。
棋盘进树后补上的 3 个 `@Control@N` 是 `GraphwarPlane::ready` 建的
`FunctionDrawer` **占位**（§37.8 的欠账，本轮没动）。

### 38.6 与原版的差别（逐条，别当成"已复原"）

| 项 | 原版 | 本轮 | 档位 |
|---|---|---|---|
| 装了几个 manager | `GraphwarGameClient::new` **22** 条 `add_module` | **3** | 22 是**观测**（`docs/02` §22.4）；3 是本轮范围 |
| 注册顺序 | 22 条的调用序两侧逐位相同，逐型对齐**没做** | `GameStateManager → TurnManager → PlayerManager` | **我方选择** |
| 容器挂在谁身上 | `root+0x1668`（某 manager 的 `Rc<RefCell<_>>`）→ 该 manager `+0x238`（容器）→ `.borrow().get_module::<X>()` | 容器直接挂在根节点的一个字段上 | 原版**观测**；链接方式不同，**容器语义相同** |
| `Weak` 环 | 各 manager 持 `Weak<RefCell<GraphwarModuleContainer>>`（`get_module_wk` 的实参形态） | 3 个 manager **都不**反向持有容器，`get_module_wk` 这条路没接 | 原版**观测**；本层未用 |
| `GraphwarGameStateManager` | 296 字节的原版结构 | 我方 `game::Match`（带 `RoomState` / `Vec` / AI 状态） | 结构**没有**按字节复原（与 296 一致与否未验证） |
| `PlayScreen` 这一屏 | `play_screen_control`（Rust，模块在 `src/` 有纯逻辑） | 只是一个 `Control` 容器 | 控制器**未**节点化 |
| 屏尺寸 | `.tscn` 的容器布局（`PlayScreen/.../PlaneContainer`） | 写死 `1600×900`（= `client/project.godot` 的视口） | **我方选择** |
| 3 个士兵的坐标 / 名字 / 队伍 | `SoldierInfo` / `TeamInfo` / 名字管理器 | 写死（`DEMO_SOLDIERS` / `DEMO_PLAYERS`） | **我方选择**（假数据） |
| 房间状态 6 字段 | `GameInfo` 抓包 | `demo_room()`，6 个取值照搬 `tests/graphwar_game_state_manager.rs` | 取值**观测**（抓包夹具） |
| 棋盘的 `_draw` | 1 个 `draw_set_transform` + 1 个 `draw_rect` | 同（§37 已做） | **逐位**（常量槽） |
| 3 个贴图的像素 | pck 里的真皮肤 / 脸 / 帽子 | 程序生成纯色 `ImageTexture`（§34 的选择） | **我方选择** |

### 38.7 仍是占位 / 未知（诚实清单）

* **剩下 19 个 manager 一个都没装**：容器里只有 3 个键；`GraphwarModuleContainer` 节点下的
  **25 个空 `Node`** 仍然是占位（`module_slot_status()` 回答的是**容器**里装没装，不是节点树）。
* **`PlayScreenControl` / `SetupControl` 的大部分行为**：本轮只把 `GraphwarSetupControl`
  那个类挂上去（它自己按 §36 的结论只连 3 条信号），四格模式的**按钮点击**原版就没连
  （§36 的 `pressed` 未知项）。
* **`GraphwarRootNode` 的两跳链**（`+0x1668` / `+0x16c8` 的控制器、`+0x16a0` 的贴图加载器）
  **未解**，本轮把容器直接挂根节点，是**替代形状**。
* **事件总线**：3 个 manager 的订阅（`new_connection_event` / `tick_event` …）一条都没接——
  本仓库没有 `GraphwarBackend`/`EventHandler`。`Match::new` 里那 7 个订阅点在 `src/game.rs`
  的注释里已如实标注。
* **回合推进 / 开火**：容器里的 `TurnManager` 与 `Match` 内部那个是**两个实例**，本轮不驱动
  回合（`Match::begin_next_turn` 没有被节点层调用）。

### 38.8 复现（三分钟）+ 回归条数

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=$CARGO_HOME/target
export GODOT=<ws>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64
cargo build --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 86 passed, 0 failed
```

同轮回归（都在本轮实测）：

| 命令 | 结果 |
|---|---|
| `cargo test --offline` | lib **457** / 总计 **1499**，全绿（与本轮之前**一位不差**） |
| `cargo test --offline --features gdext-bindings` | lib **524** / 总计 **1560**，全绿（`root_wiring` 的 6 条单测只在有 feature 时编译） |
| `python3 tools/coverage_matrix.py --check` | **OK**（改 `src/` 后必须重跑生成器，两个生成物一起提交） |
| `base_classes_check` / `draw_classes_check` / `entity_drawer_check` / `signal_classes_check` | **117 / 235 / 72 / 155** 条，全过（既有断言一条没改） |
| `bash client/run.sh selftest`（缺 `client/assets/`） | **606 passed / 2 failed / 23 skipped**（前置条件：当前 `CARGO_TARGET_DIR` 里**已构建** `examples/serve_match`；没编时是 **530 / 2 / 25**，少的就是联机那 2 条 —— 两条数都对，别当成口径打架） |

⚠️ **selftest 那 2 条失败不是本轮引入的**（这是**观测**，不是推断）：把本轮 3 个 `src/` 文件
`git stash` 掉、重编 `.so` 再跑一次，结果**逐字相同**（`606 passed, 2 failed, 23 skipped`，
同样两条 `FAIL`）。两条都依赖缺失的资产：
`点 Skins 页的第 -1 号皮肤`（皮肤列表为空 ⇒ 下标 `-1`）与
`Cancel = 194 宽 / Start = 218 宽（真帧；实际 184 / 167）`（字体宽）。缺 `client/assets/` 时
拿不到 694 条是**预期**，本节只报实测数字。

### 38.9 就地更正（`§37.8` 的最后一条）

`§37.8` 写「**`GraphwarRootNode`**：`PlayerShowcase::ready` 第 1 步真的会失败
（`root_lookup_failures`），**因为那个类还没做**」——**后半句是错的**（`docs/16` §35 那一轮
就把 `GraphwarRootNode` 这个类做出来了，`ClassDB.class_exists("GraphwarRootNode")` 一直为真）。
`PlayerShowcase` 那一步查的是**场景里的节点实例**（`get_node` 一路上溯），
而验收脚本里根本没把 `GraphwarRootNode` 放进那棵树 ⇒ 它照样找不到。前半句（会失败）不变，
原因改成「**那棵树里没有那个节点**」。本节已按 `AGENTS.md` §0.6 就地更正。

## 39. 用 `recovered-src/` 的 105 个原版 `.gd` 换视图层：清点 + 一个最小试验（2026-11，P3 调研）

> **本篇解决什么问题**：`AGENTS.md` §0 第 4 条给的方向是"**视图层改用反编译出的原版 `.gd`**"。
> 本节回答**换得动吗、能换几个、代价多大、先做哪个**，并附一个已经在 headless 里跑通的最小试验。
>
> **产出是决策依据，不是产品代码**：试验脚本只在 `client/tools/` 下，`client/src/**`、`client/*.tscn`、
> `recovered-src/**` **一行没动**（试验用的副本由 `cp` 产生，验收脚本每次都验"逐字节相同"）。
>
> 档位沿用四档，**推断与观测分开写**：本节凡是"数"都来自 headless 真跑或文件枚举（**观测**）；
> "要不要换""代价多大"是**推断**，缺的证据都写在 §39.9。

### 39.1 一页结论

| 项 | 结果 | 证据 |
|---|---|---|
| `.gd` 总数 | **105** = **52 个游戏脚本**（`recovered-src/` 根） + **53 个插件脚本**（`addons/`） | **观测**（文件枚举；`addons/admob/` 52 + `addons/android_IAPP/` 1） |
| **现在就能直接换** | **40 个**（820 行）＝ **39 个零错误** + **1 个有 3 条引擎报错但功能正常** | **观测**：headless 挂树后按 stderr 归属；C 段对副作用逐条断言（§39.3） |
| **要等 PlayScreen 装配** | **5 个**（149 行） | **观测**：`_ready` 直接报 `Node not found: PlaneContainer` 等 |
| **不能换** | **60 个**（3241 行）＝ 平台桥 **6** + 空文件 **1** + 插件 **53** | **观测**：`Ad_holder.gd` 解析失败；8 个原生类不在 `ClassDB`；PCK 里 **0 个** `.so` |
| 最小试验 | 原版 `AdContainer.gd`（19 行，**一个字节没改**）在我们注册的节点上跑通 | **观测**：`RECOVERED_VIEW 57 passed, 0 failed`，退出码 0 |
| 换 `AdContainer.gd` 的唯一障碍 | **2 个成员**：`signal ad_loaded` + `var ad_size: Vector2i`（`GraphwarAdSwitch` 上没有） | **观测**：D③ 段补齐这 2 个成员后原版脚本原样可用 |
| 结论 | 视图层可以换，**但换来的不是"行为"，是"布局公式"**（40 个里 27 个出现 `add_theme_font_size_override`，其余只干最小尺寸/内边距） | **观测**（逐文件读原文） |

三句话：

1. **能换，而且比想的简单**：52 个游戏脚本里有 40 个是"**只看自己子节点 + 视口宽度**"的布局脚本，
   挂在任何容器上都能跑——不需要 `PlayScreen`、不需要联机层、不需要资产。
2. **换不到行为**：这 40 个一共 820 行，**没有一行是按钮按下去干什么**（那在 Rust 里，
   `play_screen_control` 等模块）。换它们只把**字号/最小尺寸的算式**从"我们猜的"变成"原版的"。
3. **原版 `.gd` 的另一半价值是接口规格书**：`AdContainer.gd` 用两行代码（`.ad_loaded` / `.ad_size`）
   就点名了我们 `GraphwarAdSwitch` 缺的两个成员——这比反汇编更快、更准（**观测**，见 §39.5）。

### 39.2 105 个 `.gd` 的构成（先把分母钉死）

| 组成 | 数 | 是什么 | 证据 |
|---|---:|---|---|
| `recovered-src/*.gd` | **52** | 挂在 25 个游戏场景上的**小控件脚本**（`AdContainer.gd`、`VersionLabel.gd`、`CampaingLevelsButtons.gd` …） | **观测**：`find recovered-src -maxdepth 1 -name '*.gd' \| wc -l` |
| `recovered-src/addons/admob/**` | **52** | AdMob 插件（`class_name AdError` 那批 + 7 个 `sample/` + 编辑器插件 `admob.gd`） | **观测** |
| `recovered-src/addons/android_IAPP/` | **1** | `AndroidIAPP.gd`（Google Play 结算插件） | **观测** |
| **合计** | **105** | | 与 `docs/20-source-recovery.md` §2.1 的 105 个 `.gdc` 一致 |

> 与 `docs/20` §5.1 "105 个脚本里**没有一个是屏幕控制器**"那句对得上：52 个游戏脚本里
> **没有** `play_screen` / `lan_lobby` / `player_screen` 这些名字的控制器，全是小控件。

### 39.3 逐文件结论表 · A：**现在就能直接换**（40 个 / 820 行）

判据（**观测**）：下面每个文件都在 `client/tools/recovered_view_check.gd` 的 B/C 段里
`load()` → `new()` → 挂进场景树 → 等两帧，C 段对可观测副作用做了断言
（字号 / `custom_minimum_size` / theme constant / 子节点数），并且 **stderr 里没有一条
`SCRIPT ERROR` 指向它**。**唯一例外**是 `PlayerTabContainer.gd`：它有 3 条
`ERROR: Parameter "child" is null.`（`set_tab_title` 在页签注册前被调，见 §39.9 第 2 条），
标题照样写进去了 —— 所以它算 ✅，但**不能**算"零错误"。

| 文件 | 类型 | 依赖什么节点 | 能不能直接用 |
|---|---|---|---|
| `VersionLabel.gd`（18 行） | 视图（字号） | 自己的子节点 | ✅ 观测：`font_size = int(视口宽/60)` |
| `LanLobbyTitle.gd`（19） | 视图（字号） | 自己的子节点 | ✅ 观测：`/30` |
| `LanLobbyFontResize.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/40` |
| `MainMenuButtonResize.gd`（18） | 视图（字号） | 自己的子节点 | ✅ `/30` |
| `UnlockButtonSize.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/30` |
| `GameSetupPlayerEntry.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/40` |
| `RoomName.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/30` |
| `ExitButtonContainer.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/30`（场景里用了 14 次） |
| `TimerContainer.gd`（21） | 视图（字号） | 自己的子节点 | ✅ `/30` |
| `FireContainer.gd`（21） | 视图（字号+最小宽） | 自己的子节点 | ✅ 观测：`custom_minimum_size.x = 视口宽×0.07` |
| `FxContainer.gd`（21） | 视图（字号） | 自己的子节点 | ✅ `/60` |
| `OffsetContainer.gd`（21） | 视图（字号+最小宽） | 自己的子节点 | ✅ `×0.09` + `/60` |
| `LobbyButtonContainer.gd`（18） | 视图（字号+最小宽） | 自己的子节点 | ✅ `×0.2` + `/30` |
| `TitleSize.gd`（18） | 视图（字号+最小宽） | 自己的子节点 | ✅ 观测：`/8`（主菜单标题） |
| `CampaingButtonSize.gd`（19） | 视图（字号） | 自己的子节点 | ✅ `/15` |
| `ChatContainer.gd`（18） | 视图（字号） | 自己的子节点 | ✅ `/50` |
| `LevelDescriptionScrollContainer.gd`（18） | 视图（字号） | 自己的子节点 | ✅ `/40` |
| `TurnLimitText.gd`（18） | 视图（字号） | 自己的子节点 | ✅ `/20` |
| `IpPopUpMargin.gd`（25） | 视图（字号+四边 10px） | 自己的子节点 | ✅ 观测：`margin_* = 10` + `/40` |
| `HBoxContainer.gd`（25） | 视图（字号+格子尺寸） | 自己的子节点 | ✅ 观测：`×0.05` + `/30` |
| `ViewFunctionsLabel.gd`（25） | 视图（字号+格子尺寸） | 自己的子节点 | ✅ `/10` |
| `ButtonGrid.gd`（26） | 视图（字号+格子尺寸） | 自己的子节点 | ✅ 观测：子节点 `custom_minimum_size = 视口宽×0.05` |
| `LevelTitleLabel.gd`（15） | 视图（**自己的**字号） | 无 | ✅ 观测：给自己写 `/30` |
| `OffsetLabel.gd`（17） | 视图（自己的最小宽） | 自己的子节点 | ✅ |
| `SoldierIconMarginContainer.gd`（15） | 视图（负 margin） | 无 | ✅ 观测：`margin = -视口宽/100`（64 px 视口下截断成 0，断言退化成"覆盖存在"） |
| `HBoxVictoryIcon.gd`（13） | 视图（负 separation） | 无 | ✅ 观测：`separation = -视口宽/10` |
| `ShowcaseMarginContainer.gd`（14） | 视图（四边 5px） | 无 | ✅ `_ready` 只写 4 个 theme constant |
| `LevelTitleContainer.gd`（11） | 视图（空 `_ready`） | 无 | ✅ |
| `endgame_pop_up.gd`（9） | 视图（空 `_ready`） | 无 | ✅ |
| `PlayerNameLineEdit.gd`（1） | 视图（只有 `extends LineEdit`） | 无 | ✅ |
| `MarginContainer.gd`（50） | 视图（安全区内边距） | `DisplayServer` 安全区 | ✅ 观测：`_process` 里比对 `get_display_safe_area()` |
| `MarginContainerPopUpBack.gd`（35） | 视图（同上） | `DisplayServer` 安全区 | ✅ |
| `ShopButtonsSize.gd`（38） | 视图（递归改子节点最小尺寸） | 自己的子树 | ✅ |
| `OptionsContainer.gd`（13） | 视图（`0.25×父高`） | 自己的父节点尺寸 | ✅ 观测：父高 400 ⇒ `custom_minimum_size.y = 100` |
| `ResizeContainer.gd`（17） | 视图（铺满视口） | 视口 | ✅ 观测：`= 视口尺寸` |
| `PlayerTabContainer.gd`（20） | 视图（三个页签标题） | 需要 **3 个子节点**（页签） | ✅ 观测：标题逐字符等于原版（含 U+2009）；⚠️ 见 §39.9 第 2 条 |
| `ChatBox.gd`（21） | 视图（点一下把焦点给输入框） | `../ChatLineEdit`（**只在 `_gui_input`**，`_ready` 不碰） | ✅ 挂树零错误；点一下才需要那个兄弟节点 |
| `CampaingLevelsButtons.gd`（57） | 视图（**运行时造 21 个按钮**） | 无（自己 `new()` 我们注册的 `GraphwarButton`） | ✅ **观测**：见 §39.5.2 |
| `UnlockButton.gd`（11） | 视图（空 `_ready`，**基类是我们注册的 `GraphwarButton`**） | 无 | ✅ |
| `ViewFunctionsLabel.gd`（25） | 视图（字号+格子尺寸） | 自己的子节点 | ✅ |

> 合计 **40 个 / 820 行**。其中 **27 个**出现 `add_theme_font_size_override`（**观测**），
> **32 个**连 `get_tree().root.size_changed`（**观测**，与 `docs/20` §5.1 "32 处"一致）。

### 39.4 逐文件结论表 · B/C/D：**要等装配 / 不能换**（65 个）

#### B1：要等 `PlayScreen` 装配 —— **5 个 / 149 行**（❌）

| 文件 | 类型 | 依赖什么节点 | 能不能直接用 |
|---|---|---|---|
| `ExtraOptionPad.gd`（21） | 视图（按棋盘余高决定显隐） | `find_parent("*PlayMainHBox")` → `PlaneContainer` / `LeftControls/LeftTopButtonsContainer` | ❌ 观测：`Cannot call method 'get_node' on a null value`（`_ready` 第 4 行） |
| `ViewFunctionPad.gd`（27） | 同上 | 同上 + `PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox` | ❌ 同一句错误（第 4 行） |
| `ExtraOptionsHBox.gd`（44） | 同上（`item_rect_changed` 驱动） | `../../../LeftControls/LeftTopButtonsContainer`、`../../../PlaneContainer` | ❌ 观测：`Invalid access to property 'item_rect_changed' on null`（第 8 行） |
| `ShowExtraButtons.gd`（38） | 视图（**基类是 `GraphwarButton`**，切图标） | `../../../../PlaneContainer` 及其 3 个子路径 + `res://show_extra_buttons.svg` / `hide_extra_buttons.svg`（**资产不在库里**） | ❌ 观测：第 9 行同一个 null 错误；另外 2 个 SVG 也不在仓库里 |
| `AdContainer.gd`（19） | 视图（广告位高度 = 广告实际高度） | `/root/GraphwarRootNode/Ad_holder` + 它的 `ad_loaded` 信号 / `ad_size` 属性 | ❌ **但只差 2 个成员**：见 §39.5.3，补齐后原样可用 |

#### B2：平台桥 —— **6 个 / 491 行**（🚫 桌面/headless 永远用不上）

| 文件 | 类型 | 依赖什么节点 | 能不能直接用 |
|---|---|---|---|
| `Ad_holder.gd`（66） | 平台桥（AdMob） | `AdView` / `AdSize` / `AdPosition` / `AdRequest` / `AdListener` / `LoadAdError` / `MobileAds` **7 个原生类** + `/root/GraphwarRootNode` | 🚫 **观测**：`load()` 直接**解析失败**（10 条 Parse Error），是 52 个里唯一连编译都过不去的 |
| `IAP_holder.gd`（33） | 平台桥（内购转发） | `$IAP_Android` / `$IAP_IOS` 两个子节点 | 🚫 观测：`Invalid access to property 'purchase_confirmed' on null instance` |
| `IAP_IOS.gd`（94） | 平台桥（iOS 内购） | `InAppStore` 单例 + `root.log()` | 🚫 观测：`Attempt to call function 'log' in base 'null instance'` |
| `android_iap.gd`（254） | 平台桥（Google Play 结算） | `AndroidIAPP` 单例 + `root.log()` | 🚫 观测：`await create_timer(1)` 之后 `run_iapp_billing()` 报同一句 |
| `IOS_album_holder.gd`（22） | 平台桥（存相册） | `AlbumIOS` 单例 + `root.log()` | 🚫 观测：`Node not found: "/root/GraphwarRootNode"` |
| `Android_mediastore_holder.gd`（22） | 平台桥（存相册） | `GodotAndroidMediaStorePlugin` 单例 + `root.log()` | 🚫 同上 |

> 这 6 个的共同点：**都要 `root.log()`**（`/root/GraphwarRootNode` 上的一个方法，我们没实现），
> 而且都只在 `Engine.has_singleton(...)` 为真时才干活 ⇒ 在 Linux/headless 上它们**无事可做**。
> **推断**（缺证据：原版 APK 里这些插件的 `.so` 行为）：它们的完整语义只能在真机上验证。

#### B3：空文件 —— **1 个 / 1 行**（🚫）

| 文件 | 类型 | 依赖什么节点 | 能不能直接用 |
|---|---|---|---|
| `ExtraOptionsVbox.gd`（1 行，只有换行） | —— | 无 `extends` ⇒ 是 `RefCounted` 脚本 | 🚫 **观测**：`new()` 出来的不是 `Node`，挂不到任何节点上；25 个场景里也**没有**引用它 |

#### B4：插件 —— **53 个 / 2749 行**（🚫 永远不能换）

| 目录 | 数 | 是什么 | 能不能直接用 |
|---|---:|---|---|
| `addons/admob/admob.gd` + `version_helper.gd` | 2 | AdMob 的**编辑器插件**（`extends EditorPlugin`） | 🚫 编辑器插件，运行时不存在 |
| `addons/admob/sample/` | 7 | 插件自带的 7 个示例（Banner / Interstitial / Rewarded / Ump …） | 🚫 示例，不是游戏的界面 |
| `addons/admob/src/api/` | 9 | `AdView` / `MobileAds` / `InterstitialAd*` / `RewardedAd*` 的 GDScript 外壳 | 🚫 外壳而已，真正的类在原生插件里 |
| `addons/admob/src/api/core/` | 11 | `AdRequest` / `AdSize` / `AdError` / `ResponseInfo` … 的值类型 | 🚫 同上（`class_name` 只是 TypeScript 式的包装） |
| `addons/admob/src/api/listeners/` | 7 | 回调对象（`AdListener` / `FullScreenContentCallback` …） | 🚫 同上 |
| `addons/admob/src/core/` | 1 | `MobileSingletonPlugin.gd`（自检单例是否在） | 🚫 同上 |
| `addons/admob/src/mediation/` | 1 | `MediationExtras.gd` | 🚫 同上 |
| `addons/admob/src/mediation/extras/{adcolony,meta,vungle}/` | 2 + 1 + 4 | 三家广告聚合的 extras | 🚫 同上 |
| `addons/admob/src/ump/{api,core}/` | 3 + 4 | 欧盟同意书（UMP） | 🚫 同上 |
| `addons/android_IAPP/AndroidIAPP.gd` | 1 | Google Play 结算的 GDScript 外壳 | 🚫 原生插件不在包里 |
| **合计** | **53** | | |

**为什么是"永远"**（两条都是**观测**）：

1. `ClassDB` 里**一个都没有**：`MobileAds` / `AdView` / `AdSize` / `AdPosition` / `AdRequest` /
   `AdListener` / `AndroidIAPP` / `InAppStore` —— 8 个全不在（`recovered_view_check.gd` E 段）。
2. **这些插件的二进制不在 PCK 里**：`recovered-src/MANIFEST.tsv` 的 692 个文件里
   **`.so` 出现 0 次**（`grep -c '\.so' recovered-src/MANIFEST.tsv` → `0`）。
   它们是 Android/iOS 侧的原生库，随 APK/AAB 分发，**不在本仓库能触达的范围内**。

### 39.5 最小试验：原版 `.gd` 挂在我们注册的节点上，headless 跑通

脚本：`client/tools/recovered_view_check.gd`（**57 条断言，退出码 0**）。
原件副本：`client/tools/recovered/*.gd`（52 个，**逐字节等于** `recovered-src/`，A 段每次验）。

#### 39.5.1 先说结论：57 条断言里最要紧的 6 行

```text
ok   A：52/52 个副本与 recovered-src 原文件**逐字节相同**
ok   B：51/52 个原版脚本能在 Godot 4.7.2 里**解析**
ok   ★ CampaingLevelsButtons.gd 造了 21 个按钮（实际 21）
ok   ★ 21 个全是**我们注册的** GraphwarButton
ok   D③：`ad_loaded` 抬起 ⇒ 原版 `_update_size()` 把 custom_minimum_size 写成 (320.0, 50.0)（期望 (320, 50)）
ok   E：AdMob / IAP 的 8 个原生类一个都不在 ClassDB 里（实际在：）
RECOVERED_VIEW 57 passed, 0 failed
```

#### 39.5.2 旗舰：原版 `CampaingLevelsButtons.gd` 驱动**我们注册的** `GraphwarButton`

原版这 57 行里有一句 `GraphwarButton.new()`（关卡按钮是**运行时造**的，`docs/16` §2.1.2 记过）。
试验证据（**观测**）：

```text
ok   ★ CampaingLevelsButtons.gd 造了 21 个按钮（实际 21）
ok   ★ 21 个全是**我们注册的** GraphwarButton
ok   ★ 21 个都进了场景树（child_count = 21）
ok   ★ 名字是原版的 `level_0` … `level_20`（首 level_0 / 末 level_20）
ok   ★ `input_text` 也照原样写进去了（首 level_0 / 末 level_20）
```

⇒ 「**原版视图脚本 + 我们注册的节点类**」这条接缝是通的：`set_name()` / `text` /
`add_theme_font_size_override()` / **`input_text`（我们自造的 `#[export]`）** 全部对得上。
这条比"能 load"强得多：它证明**原版脚本能把命令键灌进我们的按钮**（`input_text` 就是 §1 说的"界面与内核的协议"）。

#### 39.5.3 `AdContainer.gd` 三步：原样 → 有节点无成员 → 补 2 个成员

这是本任务最有决策价值的一段：**原版脚本自己写明了它要 Rust 节点类提供什么**。

| 步 | 环境 | 结果 | 证据 |
|---|---|---|---|
| ① | 什么都不给（`/root/GraphwarRootNode` 不存在） | `_ready` 第 8 行炸 | **观测**：`Invalid access to property 'ad_loaded' on a base object of type 'null instance'` |
| ② | 用**我们注册的** `GraphwarRootNode` 建骨架（它自己会造 `Ad_holder = GraphwarAdSwitch`） | 节点找到了，**成员没有** | **观测**：`Ad_holder` 的类就是 `GraphwarAdSwitch`；它有 `are_ads_enabled()`，**没有** `ad_loaded` / `ad_size`；`_ready` 报 `Invalid access to property 'ad_loaded' on a base object of type 'GraphwarAdSwitch'` |
| ③ | 同一个节点换成 `client/tools/recovered_ad_holder_stub.gd`（`extends GraphwarAdSwitch` + 补那 2 个成员） | **原版脚本一行不改，跑通** | **观测**：`ad_loaded` 一抬 ⇒ `custom_minimum_size == (320, 50)`；`visibility_changed` 一抬 ⇒ `(640, 100)` |

⇒ 结论（**观测**）：`AdContainer.gd` 换不过去**不是**"还缺一整个类"，而是**缺 1 个 signal + 1 个 property**。
这两个成员的名字、类型、语义**全部由原版 `.gd` 自己给出**，不需要再反汇编。

### 39.6 「能直接用」的 40 个到底换来了什么

**换来的**（**观测**，逐文件读原文）：

| 项 | 原版 | 我们现在（`client/src/screens/`） |
|---|---|---|
| 字号公式 | `add_theme_font_size_override("font_size", screen.x / N)`，N 逐控件写死（8/10/15/20/30/40/50/60） | 各屏 `FONT_DIVISOR` + `meta("font_divisor")`，**N 基本对得上**（`main_menu.gd` 就写了 8/30/60） |
| **有没有下限** | **没有**：52 个脚本里 `maxi` / `max(` 出现 **0** 次 | 有：`maxi(10, int(width / font_divisor))`（`screen.gd`），**这是本项目自加的** |
| 触发方式 | 32 个连 `get_tree().root.size_changed`，另有 9 个连 `resized` | 同样是 `size_changed`，**但按屏重算**（`screen.gd` 递归） |
| 最小尺寸 | 逐控件乘固定系数（0.05 / 0.07 / 0.09 / 0.2 / 0.24 / 1.0 …） | 一部分有、一部分是 `GwUI` 里的间距常量 |

**没换来的**（同样是**观测**）：

* **没有任何按钮行为**：40 个文件里没有一处 `connect("pressed", ...)` 之后做业务（只有
  `ShowExtraButtons.gd` / `ChatBox.gd` 两处是切换图标 / 抢焦点，且都在 ❌ 或需要兄弟节点的那一档）。
  按下 `Campaign` 之后干什么，仍然在 Rust 的 `play_screen_control` 里。
* **界面不会"变成原版样子"**：40 个脚本只改**字号 / 最小尺寸 / theme constant**；
  外观来自 `.tscn` + `.tres`（主题/字体）+ 149 个 `.svg`，那些要么没入库（资产），
  要么需要整项目换 `.tscn`（那是另一件事，见 §39.7 第 5 条）。

### 39.7 工作量估计与先后顺序（**推断**，括号里写缺什么证据）

| 批次 | 内容 | 估工 | 缺的证据/前提 |
|---|---|---|---|
| **P0（立即做）** | 给 `GraphwarAdSwitch` 补 `signal ad_loaded` + `var ad_size`；把 `GraphwarRootNode` 做成 autoload（原版就挂在 `/root/GraphwarRootNode`） | **0.5 人日** | 证据已经齐（§39.5.3）；`log()` 也建议一起补（6 个平台桥都要它，**推断** 它是 `print()` 的包装，缺证据：arm64 上那个方法的函数体） |
| **P0（同上）** | `AdContainer.gd` 原样换上去（**观测**：`recovered-src/*.tscn` 里 **12 个场景**各挂 1 个 —— 每个屏都有广告位：main_menu / campaign / lan_lobby / GameSetup / player_screen / replay_lobby / 6 个弹窗） | 含在上面 | 需要那 12 个屏的 `AdContainer` 节点真存在（我们现在只有 `GwUI.ad_slot()` 桩） |
| **P1（半天～1 人日）** | 把 40 个里的 **37** 个纯插入布局脚本挂到现有容器上，删掉 `GwUI` 里那套自造缩放（另 3 个要顺带补结构：`PlayerTabContainer` 要 3 个页签子节点、`ChatBox` 要兄弟 `../ChatLineEdit`、`CampaingLevelsButtons` 要替掉 `campaign.gd` 现造的按钮） | **0.5–1 人日** | 我们的节点**类和名字已经对得上**（`main_menu.gd` 就是 `MarginContainer/VBoxContainer/MarginContainer/Label`，**观测**）；风险是 `maxi(10,…)` 删掉后小窗口字号会变小——**推断** 原版就是会变小 |
| **P2（等装配）** | 5 个 `PlaneContainer` 系的（`ExtraOptionPad` / `ViewFunctionPad` / `ExtraOptionsHBox` / `ShowExtraButtons`） | 等 `PlayScreen` Rust 装配 | 缺 `PlayMainHBox` / `PlaneContainer` / `LeftControls` 这棵树 |
| **不做** | 53 个插件 + 6 个平台桥 | 0 | 原生 `.so` 不在包里（**观测**），桌面端无意义 |
| **另立项** | 整项目换 `.tscn` + `.tres`（31 + 22 个） | 远超本任务 | 要和 Rust 控制器同时换，否则节点路径全变 |

**建议的先后顺序**（一句话版）：

1. **补 `ad_loaded` + `ad_size`**（半天）→ 立刻多换 1 个、并且把 `GraphwarRootNode` 的 autoload 位置定死；
2. **换这批布局脚本**（半天～1 天）→ 纯收益：字号/最小尺寸回归原版，删掉一段自造逻辑；
3. **等 `PlayScreen` 装配完**再换那 4 个棋盘布局脚本；
4. **插件与平台桥不换**，在 `client/` 里保留桩（现状）即可；
5. **不要**在本阶段做"整项目换 `.tscn`"——那是与"控制器迁回 Rust"绑定的大工程。

### 39.8 复现（三分钟）

```text
# 1) 构建带 12 个节点类的扩展（默认 feature 里没有它们）
export CARGO_HOME=<工作区>/.reference/cargo-home CARGO_TARGET_DIR=$CARGO_HOME/target
cargo build --features gdext-bindings --offline
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so

# 2) 新增 .gd 后必须先 import（否则静默不加载，见 §37.6 第 3 条）
export XDG_DATA_HOME=.../.godot-data XDG_CACHE_HOME=.../.godot-cache XDG_CONFIG_HOME=.../.godot-config
godot --headless --path client --import

# 3) 验收（57 条断言，退出码 0）
godot --headless --path client --script res://tools/recovered_view_check.gd

# 4) "哪些文件真的报错"由 stderr 归属（错误行自带脚本路径）：
godot --headless --path client --script res://tools/recovered_view_check.gd 2>err.log
grep -oE 'recovered/[A-Za-z_0-9]+\.gd' err.log | sort -u     # 12 个
```

`err.log` 里被点名的正好 **12 个**（**观测**）：`Ad_holder` / `AdContainer` / `ExtraOptionPad` /
`ExtraOptionsHBox` / `ShowExtraButtons` / `ViewFunctionPad` / `IAP_holder` / `IAP_IOS` /
`android_iap` / `IOS_album_holder` / `Android_mediastore_holder` / `PlayerTabContainer`。
⇒ `52 − 12 = 40` 个"零错误"。§39.3 的 ✅ 表恰好也是 40 行，但**不是同一个集合**：
它在零错误那 40 个里**去掉 `ExtraOptionsVbox.gd`**（它在零错误里，可是空文件、`new()` 不是 Node，
挂不上节点 ⇒ 归 B3）、**加上 `PlayerTabContainer.gd`**（它在那 12 个里，但只是 3 条引擎报错，
功能正常）。39 + 1 = 40，两份清单可以逐条对齐。

### 39.9 未知与诚实清单

1. **`root.log()` 到底是什么**：6 个平台桥 + `GraphwarRootNode` 的 15 个屏切换都在调它。
   现在我们**没有**这个方法（**观测**：`GraphwarRootNode` 的方法表里没有 `log`）。
   **推断** 它是带前缀的 `print()`；**缺** arm64 上那个方法的函数体（地址未定名）。
2. **`PlayerTabContainer.gd` 的 3 条引擎错误是不是原版也有**：我们复现出
   `ERROR: Parameter "child" is null.`（`set_tab_title` 在页签注册前被调）。
   原版同样在 `_ready` 里这么写，**推断** 原版也会打这 3 条；**缺** 原版 APK 的 stderr。
3. **headless 的视口只有 64×64**（**观测**：设 `root.size = (1600,900)`，下一帧被 dummy
   DisplayServer 改回 64×64）⇒ 所有字号断言验的是**公式**，不是 1600×900 下的绝对值。
4. **`ShowExtraButtons.gd` 还缺 2 个 SVG**（`show_extra_buttons.svg` / `hide_extra_buttons.svg`）：
   资产按约定不入库（`client/assets/` 由使用者自己从 PCK 解），所以这个文件即使装配完成也要先有资产。
5. **"零 `SCRIPT ERROR`"不等于"行为正确"**：它只说明 `_ready` 跑到底了。
   40 个里的 34 个只有"字号/最小尺寸"这一条行为，**C 段断言的就是这一条**；
   `MarginContainer.gd` / `ShopButtonsSize.gd` 的完整分支（安全区非零、嵌套容器）**没有覆盖**。
6. **本节的 40/5/60 是"能不能跑"，不是"该不该换"**：`AdContainer.gd` 在 ✅/❌ 之间反复
   （原样 ❌、补 2 个成员后 ✅），计数按**原样**算。若 P0 做完，这个数会变成 **41 / 4 / 60**。

## 40. `FunctionDrawer` 成了真 Godot 节点 —— 棋盘上那 3 个占位子节点换成真类（2026-11：§32 之后唯一的缺口）

**一句话**：`FunctionDrawer` 在 `gdext-bindings` 下成了一个 `Control` 子类——每个 player id 一条
`Line2D` 子节点，`update_function` **只把 `points[已写入数..len]` 逐点 `add_point`**；
`GraphwarPlane` 的 3 个子节点从 `@Control@N` **占位**换成了真的 `@FunctionDrawer@N`。
headless 实测 **132 条断言全过**（退出码 0）。追加判定本身一行没写在本类里——它调的是
`src/function_drawer.rs` 的 `append_window`（§32.3 那条 `0x498d5a` 的 `ja`）。

> 档位同 §34：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / 本脚本的条数**只是回归保护**，
> 不是复原证据。

### 40.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/function_drawer_class.rs` | **新增** | **532**（类本体代码 216 / 单测 60 / 注释 219 / 空行 37） | `FunctionDrawer : Control`；每 id 一条 `Line2D`；4 个真方法（`update_function` / `remove_function` / `update_transform` / `update_function_fade`）+ 6 个计数 |
| `src/_scaffold/mod.rs` | +5 | — | `#[cfg(feature = "gdext-bindings")] pub mod function_drawer_class;` + 3 行注释 |
| `src/_scaffold/graphwar_plane_class.rs` | 改 | +34/−15 | `Control::new_alloc()` → `FunctionDrawer::new_alloc()`；文件头对照表与契约里"占位 / `@Control@N`"的说法就地改掉 |
| `client/tools/function_drawer_check.gd` | **新增** | **420**（代码 314 / 注释 65 / 空行 41） | headless 验收脚本（132 条断言） |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | 1 行 | `src_files` 104→**105**、`src_fns` 2171→**2193**、`src_tree_sha256` 跟着变；**527/527 与每模块行一行没变** |
| `docs/16-client.md` | +本节 | — | — |

`src/function_drawer.rs`（纯逻辑）、`src/lib.rs`、其它 `*_class.rs`、`client/src/**`、`client/*.tscn`
**一行没动**；本类没有第二份追加算术。

### 40.2 验收：132 条 headless 断言全过（**观测**）

```text
ClassDB.class_exists("FunctionDrawer")                     ⇒ true（能 instantiate、is_class("Control")）
第一次喂 10 点 ⇒ 返回 10；Line2D.get_point_count() == 10    ⇒ 10
再喂 12 点（**前 10 个点与第一批全不同**）⇒ 返回 2          ⇒ 点数 12、add_point 次数 12（= 10+2）
   第 0/9 点仍是**第一批**的值、第 10/11 点是第二批 ⇒ 没有重画整条曲线
再喂 5 点（变短）⇒ 返回 0；点数仍 12、add_point 次数仍 12    ⇒ 一个点都没加
GraphwarPlane.build_children() ⇒ 3 个 FunctionDrawer(@FunctionDrawer@7/8/9)  ⇒ 不再是 @Control@N
```

`add_point` 的调用次数用的是本类自己的 `add_point_total()`（**我方**计数，Godot 的
`Line2D::add_point` 没有信号可连）⇒ 它只说明"我们调了几次"；**证明增量的是另外两条**：
① `Line2D.get_point_count()` 从 10 只涨到 12；② `get_point_position(0..9)` 仍是**第一批**喂进去
的点（若整条重画，前 10 个点会变成第二批的值）。脚本里还专门加了一条对照断言
（"第二批前 10 个点确实与第一批不同"），否则这两条会空过。

其余断言的分布（同一份脚本）：

| 组 | 条数 | 关键一条 |
|---|---|---|
| 类注册 / 方法表 / 对照组 | 23 | `FunctionDrawer.get_class() == "FunctionDrawer"`、15 个方法都在 |
| 增量追加（10 → 12 → 5） | 34 | 见上 |
| 多玩家（id 1 / id 2 互不干扰） | 10 | id 2 的颜色按"低字节=R"解出来是**蓝**（`0xff0000` ⇒ `(0,0,1,1)`） |
| `remove_function` + 重建 | 14 | 重建出来的是**新** `Line2D`（instance id 变），点数从 0 重新长 |
| `update_function_fade` | 8 | value 里 `Color` 的 alpha 与 `Line2D.default_color.a` 同时变 0.25，r/g/b 不动 |
| `update_transform` | 12 | 宽度 **0.125**、接点 **2**、`flip=true` ⇒ 缩放 **取负** |
| 边界（空数组 / 超 i32 的 id）+ `FunctionDrawer.new()` | 12 | 空数组**也会建节点**（原版建节点在长度判定之前） |
| `GraphwarPlane` 的 3 个子节点 | 19 | 3 个全是 `FunctionDrawer`，进树后 `ready` 不重建、已喂的点不丢 |

### 40.3 本轮新读出来的证据（**指令级 / 逐位**；其中推断处已逐条标出）

**(a) HashMap 的 value 是 **40 B 三元组** `(Gd<Line2D>, Color, usize)`，不是 §32.3 写的二元组。**
`0x498f13`–`0x498f36` 的插入块（bucket 步长 48 B = 4 B key + 4 B pad + 40 B value）：

```text
0x498f17  mov [bucket+0x08], r11      ; Gd<Line2D> 的指针
0x498f1c  mov [bucket+0x10], r10      ; Gd<Line2D> 的 instance id
0x498f21  movdqa xmm0, [rsp+0x30]     ; 建节点时用 r9d 拼出来的 Color
0x498f27  movdqu [bucket+0x18], xmm0  ; Color（16 B；alpha 在 +0x24）
0x498f2d  mov [bucket+0x28], 0x0      ; usize 已写入数 = 0
```

多出来的 `Color` 就是 `update_function_fade` 要改的那一份，下面 (e) 是它的读者。

**(b) `Line2D::set_width` 的实参是 **0.125**（逐位）。**
`update_transform`（`0x499050`）的两个调用点 `0x49911c` / `0x49921c` 都是
`movss xmm0, [rip-…] # 1baff8`，而 `.rodata 0x1baff8` 的 4 字节是 `00 00 00 3e`：

```text
objdump -s --start-address=0x1baff0 --stop-address=0x1bb000 .reference/steam/2026-08-28/libgraphwarrust.so
 1baff0 63616c6c 4e6f6e65 0000003e 504f5354  callNone...>POST
```

⇒ `0x3e000000` = **0.125f32**。这正是任务书说"宽度取自纯逻辑常量"的那一项——**纯逻辑里没有这个常量**
（`src/function_drawer.rs` 只记了"`set_width`×2"），所以它落在
`src/_scaffold/function_drawer_class.rs::LINE_WIDTH` 里、带着地址，而不是我方挑的数。

**(c) `set_joint_mode` 的实参是 **2**（`LINE_JOINT_ROUND`，观测）。**
`0x499158` / `0x49925a` 的 `mov esi, 0x2`，紧跟 `call *0x9bbfa8`。

**(d) `update_transform` 的 `flip` 分支：`set_scale(两个分量都取负)`（逐位）。**
`0x499080` 载入掩码 `0x19e880`（16 字节全是 `00 00 00 80` = **-0.0**），`0x49908e` 用 `pxor` 把
`xmm2`（那个 `Vector2`）的符号位翻过来；`esi != 0` 的分支用取负后的值，`esi == 0` 的分支用原值
（`0x499235`）。⇒ 签名按被调者的用法读作 `update_transform(x: f32, y: f32, scale: Vector2, flip: bool)`。

**(e) `update_function_fade` 改的是 value 里那份 `Color` 的 alpha。**
`0x499377` 的 `movss [bucket+0x24], xmm0`（`cvtsd2ss` 之后）⇒ 正好是 value 里 `Color` 的第 4 个 f32；
`0x499391` 把整份 `Color` 读回来、`0x4993a3` 调 `set_default_color`（槽 `0x9bbf90`）。

**(f) 建节点时的 `z_index` 来自 `[self+0x40]`，**不是常量**（观测）。**
`0x498c93` 的 `mov r12, rdi` ⇒ `r12 = self`；`0x498dee` 的 `mov r14d, [r12+0x40]` → `0x498e49` 的
`mov esi, r14d` → `call *0x9bb028`（`set_z_index`）。⇒ 这个字段就是 2024 符号表里有、2.0.60 里
**被内联掉的 `FunctionDrawer::set_z_index`** 的宿主（§32.1 那条推断因此更站得住）。
**字段初值未知** ⇒ 本类不设 z_index（`Line2D` 保持默认 0），见 §40.5。

**(g) 布局更正（不改旧节，按 §37.3 的做法记在这里）：纯逻辑说 map 在 `self+0x20`，本轮按指令读到
的是 `self+0x00`。**
`0x498cb9` / `0x498cbd` 的 `mov r14,[r12]` / `mov rax,[r12+8]` 是 hashbrown 的 **ctrl 指针 /
bucket_mask**（随后 `[r14+rdx]` 的 `pcmpeqb` 组扫描；`[r12+0x10]` 在插入时递减 = `growth_left`、
`[r12+0x18]` 递增 = `items`）；`0x498c9e` 的 `lea r15,[rdi+0x20]` 交给 `0x47dec0` 的是 **hash state**
（那个函数从 `[rdi]` 读 16 字节、再 SipHash 一个 4 字节 key）。⇒ `self+0x00` = `RawTable`、
`self+0x20` = `RandomState`、`self+0x40` = `z_index`。
偏移本身是**观测**（指令级）；`0x47dec0` 的符号名**未知**（形状像 `hash_one`，所以"它是 hash state"
是**推断**）。`src/function_drawer.rs` 里那句 `self+0x20` 与
`src/_scaffold/function_drawer_class.rs::FunctionDrawer::lines` 的文档注释已按本节改正。

**(h) 建节点时的颜色字节序是"低字节 = R"（观测）。**
`0x498d74`–`0x498da1`：`movzx ecx, bl`（低字节）→ Color 车道 0、`movzx eax, bh`（次字节）→ 车道 1、
`shr 0x10` + `movzx`（第三字节）→ 车道 2，再 `divps` 除以 `255.0`（`0x19c7c0`），车道 3 取常量槽
`0x19cf70` 的 `1.0`。⇒ `0xff0000` 解出来是**蓝**（r=0, g=0, b=1）——验收脚本与 Rust 单测都钉了这一条。

### 40.4 两处**按证据**与任务书不同（改了什么、为什么）

**(a) "颜色/宽度取自 `src/function_drawer.rs` 里已有常量" —— 这个前提不成立。**
纯逻辑模块里**没有**颜色常量、也**没有**宽度常量（它的常量只有 `POINT_STRIDE` / `POINT_X_OFFSET` /
`POINT_Y_OFFSET` / `ADD_POINT_INDEX` / `ADD_POINT_SLOT` / `ADD_POINT_CALL_SITE` /
`CHILDREN_PER_PLANE` 与地址表）。按证据：
* **颜色在原版里是入参**（`update_function` 的 `r9d`，`GraphwarPlane::draw` 用
  `get_team_overview` 的队色填它）⇒ 本类把它做成 `update_function(player_id, points, color)` 的
  第三个参数，**只在建节点时用一次**，不给默认值（默认值就是发明）；
* **宽度是 `update_transform` 里的常量** ⇒ 从 `.rodata` 逐位读出 **0.125**（见 §40.3b），
  写在 `LINE_WIDTH` 里带地址。

**(b) `new_len` 不再是独立入参。**
原版是 `r8` 一个单独长度，且 `new_len > points.len()` 会走切片越界 panic
（`function_drawer.rs:45:38`，槽 `0x95a270`）。本类取 `new_len = points.len()` ⇒ 那条 panic
分支**按构造不可达**（不变量交给类型）。**不是**删证据：`append_window` 仍是唯一的判定。

### 40.5 还没做 / 未知（诚实清单）

* **棋盘上还是画不出曲线**：3 个子节点是真的 `FunctionDrawer` 了，但 `GraphwarPlane::draw` 里
  那个 `0x456df0`（`graphwar_function_manager::get_active_functions_on_tick`）**没有接**——
  没有东西喂函数点。本类只是"能喂就会画"。
* **`z_index` 的初值未知**（§40.3f）：本类不设。缺的证据是 `self+0x60` 的写入点（`init` 里）。
* **`FunctionDrawer::draw`（arm64 `0x3911b4`）x86 侧仍未定位** ⇒ 本类**不实现 `_draw`**
  （§32.3 的有界否定照旧成立）。
* **`update_transform` 三个入参的含义未知**：`x`/`y`/`scale` 是按被调者 `set_position` / `set_scale`
  的用法起的名字，真正传什么由 `GraphwarPlane::draw` 决定，本切片没接。
* **3 个 `FunctionDrawer` 各管什么仍是未知**（§32.7 第 2 条没有被本轮推翻，也没有被加强）。
* **`add_point_total()` 是我方计数**，不是引擎侧钩子；"增量"的硬证据是 `get_point_count()` 与
  `get_point_position(0..9)` 那几条，脚本里另有对照断言防止空过。
* **过期登记（不改旧节，按 §37.3 的做法记在这里）**：§37.8 那条"`FunctionDrawer` / `ObstacleDrawer`
  类还不存在，3 个子节点是 `Control` 占位"对 `FunctionDrawer` 已经过期——本类存在了；
  `ObstacleDrawer` 仍不存在。§32.3 那句 `HashMap<i32, (Gd<Line2D>, usize)>` 应读作 §40.3a 的三元组。

### 40.6 复现（三分钟）

```text
# 1) 打开 feature 构建
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --features gdext-bindings

# 2) 覆盖客户端 .so（不覆盖就还在跑旧内核）
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so

# 3) 必须先 import（否则扩展静默不加载，脚本连类都认不出来）
export XDG_DATA_HOME=$PWD/.godot-data XDG_CACHE_HOME=$PWD/.godot-cache XDG_CONFIG_HOME=$PWD/.godot-config
godot --headless --path client --import

# 4) 验收（132 条断言，退出码 0）
godot --headless --path client --script res://tools/function_drawer_check.gd

# 5) 回归保护（本条分支的实测数，§37.7 那组 488/1530 是更早提交的快照）
cargo test --offline                            # lib 457 / 总计 1499
cargo test --offline --features gdext-bindings  # lib 522 / 总计 1564（基线 518/1560 + 本批 4 条单测）
python3 tools/coverage_matrix.py --check        # OK（本次重跑只动了 src_files/src_fns/src_tree_sha256）
```

## 41. 容器里从 **3 个到 22 个**：`GraphwarGameClient::new` 的注册序**逐条定死**（2026-11）

> 本节做的是 `docs/02` §22.4 留下的那条尾巴：「22 条 `add_module` 的调用序两侧逐位相同，
> 但**哪一条对应哪个型**没做跨架构对齐」。§38 只装了 3 个，而且顺序是**我方挑的**。
> 本节把 22 条的**型**与**序**一起钉到指令上，然后把 22 个全装进容器。
>
> 一句话：**22 条全部定死（档位 `观测`；顺序 `逐位`）**，
> 22 个全部装配进去（**17 个是我方复原类型 + 5 个是零行为空占位**）。

### 41.1 四问四答

| 问题 | 结论 | 档位 |
|---|---|---|
| 22 条各是哪个型？ | **22/22 全部有名字**：每条调用点前一条 `bl` 就是 `<Type>::new`（arm64 `.symtab` 原文，未 strip） | **观测** |
| 顺序对不对？ | 22 条的 `size_of::<RefCell<M>>` 序列在两架构上**逐字相同**（`80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80`） | **逐位** |
| 有没有漏/多？ | 全 `.text` 扫下来进 25 个 `add_module` 单态化的调用点**恰好 44 条** = 客户端 22 + 服务端线程 20 + `GraphwarBackend::start_*` 1 + 1；并集 = 25 | **观测** |
| 有没有重复型？ | 22 条**没有重复型**；客户端不注册的正好是 `GraphwarBanList` / `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`（服务端与 backend 才装） | **观测** |

⚠️ 与 §38.7 的关系：那里写「剩下 19 个 manager 一个都没装」——**本节已过期它**，
改成「22 个全装，其中 5 个是空占位」。旧句不删，按 `AGENTS.md` §0.6 就地标明。

### 41.2 22 条注册序（逐条证据）

`size` = `size_of::<M>()`（= `dyn Any` vtable 的 size 字 − 8，`docs/02` §22.3）；
`arm64 new` = 那个 `<Type>::new` 的地址；`arm64 vt` = `add_module` 里 `adrp/add` 拼出来的
vtable 地址；`arm64 add` / `x86 add` = 该型的 `add_module` 单态化。

| # | 原版类型（arm64 `.symtab`） | `size` | arm64 `new` | arm64 vtable | arm64 `add` | x86 `add` | 我方装配的类型 | 档位 |
|---|---|---|---|---|---|---|---|---|
| 1 | `GraphwarTickHandler` | 72 | `0x40b528` | `0x858b28` | `0x49e870` | `0x469810` | ——（空占位 `TickHandlerSlot`） | 型 `观测` |
| 2 | `GraphwarClientConnectionHandler` | 112 | `0x48b048` | `0x858d68` | `0x49f758` | `0x469950` | ——（空占位） | 型 `观测` |
| 3 | `GraphwarLocalPlayerManager` | 64 | 内联（`0x409eac` `alloc(0x58,8)`） | `0x858cc8` | `0x49f334` | `0x4694f0` | `graphwar_local_player_manager::GraphwarLocalPlayerManager` | 型 `观测` |
| 4 | `GraphwarPlayerManager` | 72 | `0x49d170` | `0x858ca8` | `0x49f260` | `0x469f90` | `graphwar_player_manager::GraphwarPlayerManager` | 型 `观测` |
| 5 | `GraphwarSoldierManager` | 72 | `0x49d590` | `0x858c88` | `0x49f18c` | `0x46a170` | `graphwar_soldier_manager::SoldierTable` | 型 `观测` |
| 6 | `GraphwarPlanePosManager` | 24 | `0x3b89a0` | `0x858c48` | `0x49efe4` | `0x46a0d0` | ——（空占位） | 型 `观测` |
| 7 | `GraphwarGameStateManager` | 296 | `0x4ade6c` | `0x858c08` | `0x49ee3c` | `0x46a210` | `game::Match` | 型 `观测` |
| 8 | `GraphwarFunctionManager` | 32 | 内联（`Rc::new_cyclic_in` `0x40a158`） | `0x858b68` | `0x49ea18` | `0x469db0` | ——（空占位） | 型 `观测` |
| 9 | `GraphwarTargetManager` | 72 | `0x3dae34` | `0x858ce8` | `0x49f408` | `0x46a350` | `graphwar_target_manager::GraphwarTargetManager` | 型 `观测` |
| 10 | `GraphwarTeamManager` | 32 | `0x49960c` | `0x858d08` | `0x49f4dc` | `0x469770` | `graphwar_team_manager::TeamTable` | 型 `观测` |
| 11 | `GraphwarLifeManager` | 72 | `0x4977b8` | `0x858b88` | `0x49eaec` | `0x469b30` | `graphwar_life_manager::LifeState` | 型 `观测` |
| 12 | `GraphwarObstacleManager` | 80 | `0x426300` | `0x858d88` | `0x49f82c` | `0x46a3f0` | ——（空占位） | 型 `观测` |
| 13 | `GraphwarTurnManager` | 32 | `0x49adf8` | `0x858da8` | `0x49f900` | `0x46a030` | `graphwar_turn_manager::TurnManager` | 型 `观测` |
| 14 | `GraphwarEffectManager` | 72 | `0x40bc0c` | `0x858c68` | `0x49f0b8` | `0x469630` | `graphwar_effect_manager::GraphwarEffectManager` | 型 `观测` |
| 15 | `GraphwarModeManager` | 32 | `0x498090` | `0x858aa8` | `0x49e520` | `0x469a90` | `graphwar_mode_manager::GraphwarModeManager` | 型 `观测` |
| 16 | `GraphwarGraphicsManager` | 168 | `0x49e394` | `0x858b48` | `0x49e944` | `0x4698b0` | `graphwar_graphics_manager::GraphwarGraphicsManager` | 型 `观测` |
| 17 | `GraphwarEntityEffectManager` | 72 | `0x3e9f40` | `0x858ba8` | `0x49ebc0` | `0x469d10` | `graphwar_entity_effect_manager::GraphwarEntityEffectManager` | 型 `观测` |
| 18 | `GraphwarComputerPlayerManager` | 72 | `0x4b02a4` | `0x858ac8` | `0x49e5f4` | `0x469e50` | `graphwar_computer_player_manager::ComputerPlayers` | 型 `观测` |
| 19 | `FunctionHistoryRecorder` | 48 | `0x49d9a0` | `0x858be8` | `0x49ed68` | `0x469bd0` | `function_history_recorder::History` | 型 `观测` |
| 20 | `GraphwarNameManager` | 72 | `0x40af68` | `0x858bc8` | `0x49ec94` | `0x46a2b0` | `graphwar_name_manager::GraphwarNameManager` | 型 `观测` |
| 21 | `GraphwarColorManager` | 120 | `0x49cbe8` | `0x858ae8` | `0x49e6c8` | `0x469590` | `graphwar_color_manager::GraphwarColorManager` | 型 `观测` |
| 22 | `GraphwarRankManager` | 72 | `0x498d2c` | `0x858c28` | `0x49ef10` | `0x469ef0` | `graphwar_rank_manager::GraphwarRankManager` | 型 `观测` |

**三条互证**（每一条都能单独复算，见 §41.3）：

1. **调用点的 `bl <Type>::new`**：arm64 未 strip，符号名直接给出型（20 条如此；
   3 号与 8 号的 `new` 被内联/放进 `Rc::new_cyclic` 闭包，见下）；
2. **`add_module` 里的 vtable 第 0 字**：`.data.rel.ro` 的 `R_AARCH64_RELATIVE` addend
   → `drop_in_place<RefCell<module::Type>>` 的符号名；第 1 字 = `size_of::<RefCell<M>>()`
   （= 上表 `size` + 8）；
3. **两架构的尺寸序列**：x86 是 stripped（没名字），但 vtable 的 size 字是**数据字节**，
   22 个读出来与 arm64 逐个相等 ⇒ 位置不会错。

**LLVM 合并 drop glue 的坑（`docs/02` §22.2 预告过，这里实锤）**：4 号 `GraphwarPlayerManager`
与 5 号 `GraphwarSoldierManager` 的 vtable 第 0 字**都指向**
`drop_in_place<RefCell<GraphwarRankManager>>`（与 22 号同一个函数）。
所以**drop glue 名不能单独定名**——必须用调用点的 `<Type>::new` 复核。
反过来，3 号与 8 号的 `new` 不可见（3 号在调用点内联：`0x409eac` 处
`alloc(0x58, 8)`，`0x58 − 0x18 = 64` = 它的 `size`；8 号走 `Rc::new_cyclic_in`），
这两条就**只能靠 drop glue + size**：两者的 drop glue 名唯一，尺寸也唯一。

### 41.3 复算（三条命令，都不依赖本仓库的脚本）

```text
# (1) 22 条「构造函数 → add_module」对：arm64 未 strip，名字直接给
llvm-objdump -d --no-show-raw-insn --demangle \
  --start-address=0x409c70 --stop-address=0x40a9bc \
  <arm64 2.0.60>/libgraphwarrust.so | grep -E "bl\s+0x" | grep -E "add_module|::new"
#   → 22 条 add_module；其中 20 条的前一条正是 `::new`，1 条前一条是 `Rc::new_cyclic_in`
#     （8 号），1 条前面什么都没有（3 号，构造被内联进调用点）

# (2) 每条 add_module 的 vtable（第 0 字 = drop glue 名，第 1 字 = size_of::<RefCell<M>>）
llvm-objdump -d --no-show-raw-insn --start-address=0x49e520 --stop-address=0x49f9d4 \
  <arm64 so> | grep -E "adrp|add .*x3"
readelf -rW <arm64 so> | grep <vtable 地址>      # → drop_in_place<RefCell<..::Type>>

# (3) x86 侧（stripped）：22 条调用序 + vtable size 字，与 arm64 逐字比对
#     调用点 = 0x4694f0 + k*160（k = 0..24）里的 22 个；每条里的 lea rip+disp 给 vtable
#     → size 字读出 [80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80]
```

**这也回答了「要不要用 `tools/sym_match.py` 的尺寸比闸 0.61–1.19」**：
**不需要**。那个闸是给**没有名字**的单态化做候选筛选用的（`docs/02` §18/§36）；
这里 arm64 有名字、且两架构的尺寸**完全相等**（不是比值），比闸强一档。

**全 `.text` 的完整性扫**（防"22 条只是我以为的 22 条"）：把 `.text` 一次性反汇编，
统计所有 `call` 进那 25 个 `add_module` 地址的调用点 ⇒ **44 条**，
按地址聚成两簇：`0x45035d..0x45143f` 共 20 条（服务端线程，含 `BanList` / `ServerConnectionHandler`）、
`0x515658..0x517ee6` 共 24 条（其中 2 条是 `GraphwarBackend::start_*` 各注册一次
`GraphwarGameRecorder`，其余 22 条就是客户端）。22 + 20 − 18（交集）+ 1 = 25，与 §22.4 一致。

### 41.4 落码：22 个槽 = 17 个真类型 + 5 个空占位

`src/_scaffold/root_wiring.rs` 里：

* `Wired`（22 个变体，**顺序 = 上表**）+ `SLOTS: [Slot; 22]`（逐条台账：
  原名 / 模块名 / `size` / arm64 `new` / 我方类型 / 档位）；
* `RootWiring::assemble_demo()` 按上表顺序 22 次
  `container.add_module(Rc::new(RefCell::new(<M>::new(…))))`；
* 判据从"数个数"升级成两档：
  `has()`（`HashMap::contains_key(&TypeId::of::<M>())`）与
  `retrievable()`（真走 `get_module::<M>()`：`hash_one` → 探测 → `Any::type_id` →
  `Rc::downcast`，`docs/02` §22.5 的那条路径）。

**5 个空占位**（`SlotKind::Stub`，`module_slot_status()` 如实回 `"stub"`，**不是** `"assembled"`）：

> ⚠️ **§47 起作废这一段**：档位现在是四档，这 5 个槽的实际档位是
> **`wired` / `layout` / `stub` / `layout` / `wired`**（`TickHandler` / `ClientConnectionHandler` /
> `PlanePosManager` / `FunctionManager` / `ObstacleManager`），理由逐条见 §47.2。
> `SlotKind::Stub` 仍在（那是"容器类型来源"的 5 个），但它**不是档位**。
`TickHandler` / `ClientConnectionHandler` / `PlanePosManager` / `FunctionManager` / `ObstacleManager`。
理由逐个可查：`src/graphwar_tick_handler.rs` 只有 `tick_at` 一个自由函数；
`graphwar_client_connection_handler.rs` 只有 `HandlerField` 偏移表；
`graphwar_plane_pos_manager` / `graphwar_function_manager` / `graphwar_obstacle_manager`
**连文件都没有**（逻辑分别散在 `src/plane.rs` / `src/game.rs` 的自由函数里）。
按 `AGENTS.md` §0 第 3 条，**不把别的模块的类型冒充成它们**
（例如 `SoldierFunctions` 属 `graphwar_function_evolutionator`，不属 `function_manager`）。

**"装进去"不等于"能跑"**：22 个里的 17 个是**空表/默认值**，本轮没接任何事件订阅，
也没驱动任何 manager。`docs/02` §34 的 `WeakCaller`（各 manager 的弱自句柄）一条都没接。

### 41.5 验收（`client/tools/assemble_check.gd`：86 → **183** 条）

```text
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 183 passed, 0 failed
```

脚本总数 **86 → 183（净 +97）**。断言组现在是：

| 断言组 | 现在 | 原来 | 判据 |
|---|---|---|---|
| 6 个新观测口存在 | 6 | 0 | `container_module_live` / `container_live_count` / `container_real_manager_count` / `container_stub_manager_count` / `manager_slot_size` / `manager_impl_type` |
| 容器总数与分档 | 6 | 3 | `container_module_count()==22`、`wired_manager_count()==22`、`container_live_count()==22`、`assembled_manager_total()==22`、真类型 17、空占位 5 |
| **逐槽 4 条 × 22** | 88 | 6 | 键在（`contains`）/ **取回**（`get_module` downcast）/ 槽位状态（`assembled` \| `stub`）/ `impl_type` 与档位一致 |
| 注册序 + 尺寸序列 | 3 | 1 | `container_module_names()` == 22 个名字（原版序）；`manager_slot_size()` 22 个 == §41.2 的 `size` 列；胡编名字 ⇒ `-1` |
| 客户端不注册的 3 型 | 6 | 2 | 每个：`contains == false` + `module_slot_status == "placeholder"` |

净增 = (6−0) + (6−3) + (88−6) + (3−1) + (6−2) = **97**。
既有断言**一条没删**：3 条 `== 3` 只是随范围改成 `== 22`，3 型 × 2 条逐槽断言扩成 22 型 × 4 条。

⚠️ 按 `AGENTS.md` §0.2：**这 183 条是回归保护，不是复原证据**。本节的证据是 §41.3 那三条命令。

### 41.6 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么 |
|---|---|---|
| 17 个 manager 的**行为** | 未做 | 本轮只"构造 + 注册"（任务书 §4）；空表不会自己动 |
| 5 个空占位的**类型本身** | 未知 → ⚠️ **§47 已改** | §47 起的现状：3 个已可构造（两个 = §46 的原版布局 + 我方表，一个 = `graphwar_tick_handler.rs` 的 `GraphwarTickHandler`），`ClientConnectionHandler` 有**全偏移表**（§47.2），只剩 `PlanePosManager` 仍未知 |
| 各 `new` 的**实参**来自哪 | 部分 `未知` | 例如 `GraphwarGameStateManager::new` 的 4 个实参在 `0x4ade6c` 里，本轮没有逐参数解；我方用演示数据 |
| `GameStateManager` 内部是否**也持** `TurnManager` | 未知 | 与 §38 同一条；容器里是两个独立实例 |
| 21 号之前那 3 条 `subscribe_to_event` | 观测到调用 | `0x409f24/0x409f70/0x409fbc` 三次订阅发生在 3 号 `add_module` 之前；订阅的是谁、订阅了什么事件**未解** |
| `size` 与**我方类型**尺寸的关系 | 必然不等 | 例：`Match` 远大于 296；结构没有按字节复原，本轮**不**做尺寸对拍 |
| 22 条里第 3 号、第 8 号的 `new` | 内联 | 只能靠 vtable 的 drop glue 与 size 定名（两者都唯一）；**没有**独立的构造函数名 |

### 41.7 复现 + 回归条数（本节实测）

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<你的>/target
export GODOT=<ws>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd # 183 passed, 0 failed
```

| 命令 | 结果 |
|---|---|
| `cargo test --offline` | lib **457** / 总计 **1512**（当轮；现行 1520），全绿（与 §38 的 1512 **一位不差**） |
| `cargo test --offline --features gdext-bindings` | lib **529** / 总计 **1584**，全绿 |
| `python3 tools/coverage_matrix.py --check` | **OK**（改了 `src/`，两个生成物一起提交） |
| `assemble_check.gd` | **183 passed / 0 failed**（§38 的 86 条 → 183） |

⚠️ 本节**没有**改任何既有断言：`assemble_check.gd` 原有的 86 条全部保留
（只有 `== 3` 那几处随范围改成 `== WIRED.size()`，断言的**意图**不变）。

## 42. 曲线真的画出来了：`0x456df0` 是**取值器**不是绘制，喂点的是 `GraphwarPlane::draw`（2026-11，任务 13）

**一句话**：`0x456df0` 是 `GraphwarFunctionManager::get_active_functions_on_tick`，签名
`fn(&self, tick: u32) -> Vec<u32>`——**它一个图元都不画**，只把"本 tick 还在场的 player id"挑出来；
真正往 `Line2D` 里塞点的是 `GraphwarPlane::draw`，调用点 `0x4b69ec`（全二进制**唯一**一个
`FunctionDrawer::update_function` 调用点）。本轮把那段循环接进
`src/_scaffold/graphwar_plane_class.rs`，headless 实测 `Line2D.get_point_count()` =
**60 → 90 → 100**（进树走 `_draw` 时 **30**），新脚本 **96 条断言全过**（退出码 0）。

> 档位口径同 §40：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / 本脚本的条数**只是回归保护**。

### 42.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `src/_scaffold/graphwar_plane_class.rs` | 改 | +468/−2 | 喂点循环 `feed_functions()`（`_draw` 里调）+ 活跃谓词 + `new_len` 算式 + 最小函数存储 + 3 条单测 |
| `src/_scaffold/function_drawer_class.rs` | 改 | +8/−2 | `update_function` / `remove_function` 加 `pub`（让棋盘类直接调，**行为一行没变**）+ 注释 |
| `client/tools/curve_on_screen_check.gd` | **新增** | 400 | headless 验收脚本（**96 条断言**） |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | 2 行 | `src_fns` 2257→**2277**、`src_tree_sha256` 跟着变；**527/527 与每模块行一行没变** |
| `docs/16-client.md` | +本节 | — | — |

`src/function_drawer.rs` / `src/graphwar_plane.rs`（纯逻辑）、`src/lib.rs`、`src/_scaffold/mod.rs`、
别的 `*_class.rs`、`client/src/**`、`dispatch/` **一行没动**；既有断言一条未改
（`function_drawer_check.gd` 132 条、`draw_classes_check.gd` 235 条，复跑仍全过）。

### 42.2 `0x456df0` 到底是什么（**观测**；`docs/02` §15.2.1 那行"推断"就地升级）

| 项 | 值 | 判据 |
|---|---|---|
| 名字 | `GraphwarFunctionManager::get_active_functions_on_tick` | `fixtures/name_to_addr.tsv:435`，唯一锚 `src/graphwar_function_manager.rs:208:37` |
| 大小 | `0x190` | 到 `0x456f80`（同族 `..._no_fade`）的距离 |
| 签名 | `fn(&self, tick: u32) -> Vec<u32>` | `rdi` = sret 输出槽、`rsi` = self、`edx` = tick（`0x456dff` 把它存进 `0xc(%rsp)`） |
| 返回什么 | 命中者的 **4 字节 player id** 组成的 `Vec` | `0x3be619` 的 `mov -0x78(%rcx),%eax` = bucket+0 的 key，随后 `mov %ecx,(%rax)` 写进新分配的 4 字节槽 |
| 谁调它 | **只有** `0x4b6339`（`GraphwarPlane::draw` 体内） | 见 §42.3 的一条命令 |

体内三件事（`0x456df0`–`0x456e8f`）：**锁**（`0x456e12` 的 `lock cmpxchg` 一条 Mutex，state 在
`[self+0x18]` 那个 `Arc` 的 `+0x10`）→ **遍历**（`RawTable` 的 ctrl 在 `+0x18`、bucket_mask 在
`+0x20`，与 `FunctionDrawer` 的表同一个 hashbrown 形态）→ **收 id**（`0x456e8f` 调 `0x3be540`，
把命中的 id 逐个 push 进 `rdi` 指的 Vec）。

过滤逻辑被内联在 `0x3be540` 里，而 `0x3be540` 在整份 `.text` **只有一个调用者**（`0x456e8f`）
⇒ 那段谓词**确定为**本函数的。三条（bucket 步长 `0x78`，key 在 `+0x00`）：

```text
0x3be5e6  cmpl $0x1,-0x8(%rbp,%rcx,1)        ; (value+0x70) == 1 —— 字段语义未知
0x3be5f0  cmp  %r13,-0x28(%rcx) / je skip    ; (value+0x50) != 0x8000_0000_0000_0000（i64::MIN）
0x3be5f6  mov  %eax,%edx / sub -0x4(%rcx),%edx   ; edx = tick - active_tick（u32，value+0x74）
0x3be5fb  jbe  skip                          ; tick <= active_tick ⇒ 不在场（**严格**大于）
0x3be5fd  mov  -0x18(%rcx),%rdi              ; rdi = points.len()（value+0x60）
0x3be601  lea (%rdx,%rdx,1),%r8d / shl $5,%edx / sub %r8d,%edx
                                             ; edx = 30 * d（30 = .rodata 0x19ef90）
0x3be60b  add  $0x708,%edi                   ; points.len() + 1800
0x3be611  cmp  %edi,%edx / jae skip          ; 30*d >= len+1800 ⇒ 不在场
```

⇒ 一条曲线"在场"的窗口 = 揭示期（`len/30` 个 tick）+ **1800/30 = 60 tick** 的尾巴
（那 60 tick 就是淡出窗，对应 `get_function_fade_on_tick`；那个函数**没有 x86 落点**，见 §42.5）。

### 42.3 谁调 `update_function`、什么时候、传什么（**观测**，全 `.text` 唯一）

```text
objdump -d .reference/steam/2026-08-28/libgraphwarrust.so | grep -E 'call +(456df0|498c70|498b70|499050|4992b0) <'
  4b6147  call 499050      ; FunctionDrawer::update_transform     ← 在 GraphwarPlane::draw
  4b6339  call 456df0      ; get_active_functions_on_tick         ← 唯一调用点
  4b6418  call 498b70      ; FunctionDrawer::remove_function      ← 唯一调用点
  4b69ec  call 498c70      ; FunctionDrawer::update_function      ← **唯一调用点**
  4b6a0e  call 4992b0      ; FunctionDrawer::update_function_fade ← 唯一调用点
```

**⇒ 结论：原版只有 `GraphwarPlane::draw`（`0x4b5df0`）喂点。** 顺序与实参（`0x4b6317`–`0x4b6a13`）：

| # | 地址 | 干什么 | 实参从哪来 |
|---|---|---|---|
| 1 | `0x4b6317` | `mov 0x58(%rbp),%ebx` | 整数 tick（tick handler 上的 **u32** 字段） |
| 2 | `0x4b631a` | `call 0x519530` = `get_fractional_tick` | 返回 f64，存 `0x130(%rsp)` |
| 3 | `0x4b6339` | `get_active_functions_on_tick(&mut out, self, tick)` | out = `0xe0(%rsp)`；随后按 4 字节步长遍历它（`0x4b6562` 的 `mov (%r14),%ebp`） |
| 4 | `0x4b65d9` | 在函数管理器里按 id 查一条 | bucket 步长 `0x78`；点数组 = **`Vec<(f64,f64)>`**（ptr `+0x58`、len `+0x60`） |
| 5 | `0x4b67a0`–`0x4b6812` | `new_len = min(len, trunc(30*(fractional_tick - active_tick)))` | `30.0` = `.rodata 0x19ef90`（逐位） |
| 6 | `0x4b69ec` | `update_function(id, points, len, new_len, color)` | `rdi`=绘制器 `self+0x80`、`esi`=id、`rdx`=点指针、`rcx`=len、`r8`=new_len、`r9d`=颜色 |
| 7 | `0x4b69fe` | `call *0x9bbd08`（`get_function_fade_on_tick`？） | `(self+0x18, id, tick)`，返回值直接进下一步 |
| 8 | `0x4b6a0e` | `update_function_fade(id)` | 参数 = 上一步的返回值 |
| 9 | `0x4b6418` | `remove_function(id)` | 遍历"另一个集合"，不在某个 `HashSet` 里就删 |

两个附带结论：

* **三次 `FunctionDrawer` 调用都打在同一个绘制器上**：`0x4b6132` 的 `lea 0x80(%rax),%rdi`
  把 `self+0x80` 存进 `0x90(%rsp)`，`0x4b69d4` / `0x4b6a04` 都从那里取。
* **颜色是入参、每帧重查**：`0x4b6825` / `0x4b68f3` 两次 hashbrown 查表（第二张表的值取
  3 个字节拼 `r9d`）。
  ⚠️ **2026-11 就地更正（§48.3）**：这句原来接着写"两张表分别来自 `get_team_overview`
  （`0x4be6e0`）在 `0x4b5fd3`/`0x4b60e3` 的两次返回值"——**是错的**（当时是推断）。
  `get_team_overview` 返回的是 **`u32` 的 4 个布尔位**（`0x4beb95`–`0x4bebb6` 的拼装），
  只喂 [`flip`] 那一个判定；两张颜色表本身是**游戏状态里两个 `HashMap`**
  （`0x4b6825` 的表：key `u32` → value `u32`；`0x4b68f3` 的表：key `u32` → value
  **3 字节 `GraphwarColor`**，bucket stride 8 = 4+3+padding），由 `0x46a770` / `0x46b5d0`
  两个借用器从 `[r12+0x238]` 那个对象里借出来。队色池本体在
  `init_team_colors`（x86 **`0x465070`**）里，见 §48.3。

### 42.4 落码：本类怎么接的

| 原版 | 本类 | 档位 |
|---|---|---|
| `get_active_functions_on_tick` 的三条谓词 | `is_active_function()`（第 3 条；第 1/2 条见 §42.5） | **观测** |
| `new_len = min(len, trunc(30*(frac-active)))` | `revealed_points()`，`30.0` 逐位 | **逐位**（常量）/ **观测**（算式） |
| `update_function(id, points, len, new_len, color)` | `FunctionDrawer::update_function(id, points[..new_len], color)` | 传前缀**等价**（`append_window` 仍是唯一判定，同 §40.4b） |
| `remove_function(id)`（`0x4b6418`） | 不在场就删表 + `remove_function` | **观测**（调用点）/ 集合形状**推断** |
| 每帧喂 | `draw()` 末尾调 `feed_functions()`（位置与原版一致：画完棋盘框之后） | **观测** |
| 喂给哪个 `FunctionDrawer` | 固定第 0 个子节点 | **推断**（"`self+0x80` 是 3 个里的第一个"没有直接证据） |
| 让 `_draw` 每帧跑 | `_process` 里"有曲线正在揭示就 `queue_redraw()`" | **我方选择**（原版 `process` 无此调用，见 §42.5） |

**替身（必须写清，别当成复原）**：`GraphwarFunctionManager` **还没有 `src/` 文件**
（`docs/coverage-matrix.tsv` 第 36 行把它算进 `src/game.rs`），而本轮只许改这两个 `*_class.rs`
⇒ 棋盘类里先放一份"最小函数存储 + 入口"，方法名照原版的语义起：

```text
GraphwarPlane.set_function_points(id, points, active_tick, color) -> 存下的点数
    # 覆盖 GraphwarFunctionManager::set_function_points(0x456810) + set_function_active_tick(0x456b80) + 队色
GraphwarPlane.set_fractional_tick(tick) -> tick
    # 覆盖 GraphwarTickHandler::get_fractional_tick(0x519530) 的读取（原版每帧自己读）
GraphwarPlane.feed_functions() -> 本帧追加的点数
    # = 原版 draw 尾部那段循环；headless 直接调它（`_draw` 在 headless 下不一定被调）
```
`points` 是**扁平 f64 数组** `[x0,y0,x1,y1,…]`：原版的点就是两个 f64（16 B/点），
用 `PackedVector2Array` 会在入口就掉到 f32。

### 42.5 还没做 / 未知（诚实清单）

1. **点的生产者没复原**。`set_function_points`（`0x456810`）的调用者是 `0x3e44d4`
   （落在一个 70 项跳转表的事件分派函数里，前面 `0x3e44aa` 先调 `0x3cae50` 把点表按 x 区间
   就地压实；**那个函数的模块归属未证**——`fixtures/module_map.tsv` 只给出
   `graphwar_game_state_manager.rs` 的锚区间 `0x3caa20..0x4ac440`，落在区间内不等于归属）
   与 `0x471a88`；**谁算出那张原始点表**没继续挖。
   ⇒ 现在点必须由调用方注入，**这是本轮最大的缺口**（比"喂点"本身更靠上游）。
2. **`get_function_fade_on_tick` 没有 x86 落点**：`0x4b69fe` 走的是 GOT 槽 `0x9bbd08`（方法指针
   调用），alpha 的算式未知 ⇒ `update_function_fade` **没接**（不发明一个淡出曲线）。
3. **谓词第 1/2 条没做**：`value+0x70 == 1` 与 `value+0x50 != i64::MIN` 两个字段的语义未知
   （可能是 `FunctionState` 的某个变体与一个 `Option`），本类不猜。
4. ~~**`update_transform`（`0x4b6147`）仍未接**~~ ⇒ **已解出并接上（2026-11，§48.2）**。
   第三实参**不是 `Vector2`，是一个 `f32` 标量** `s = w/50`：被调者用 `pxor` 与
   `.rodata 0x19e880`（四个 `-0.0`）把 `s` 的符号位翻出来当"第二个分量"
   （`0x499088`/`0x499092` 存两份，`0x499232`–`0x499241` 各取一份喂 `set_scale` 的
   两个 f32）⇒ `set_scale` 收到 `flip ? (-s,-s) : (s,-s)`。
   旧文说"`MOVSS` 清高 lane ⇒ `Vector2(w/50,0)` 会把线压扁、构造没解全"——那个矛盾**不存在**，
   是把标量当成了 `Vector2` 的 y 分量。三个实参与 `flip` 位见 §48.2（逐指令）。
5. **客户端还没用真类**：`client/src/play/plane_view.gd` 是照 `.tscn` 重写的 GDScript 仿制品
   （自己 `draw_line`），`GraphwarPlane` 目前只在 `client/tools/*.gd` 里被实例化
   ⇒ **真机上眼前仍然看不到 Rust 侧的曲线**，要等播放屏改用真类（不在本轮允许改的文件里）。
6. `_process` 里"揭示中就 `queue_redraw`"是我方补的（原版 `process` = `ObstacleDrawer::update` ×2
   + `get_team_overview`，没有该调用）⇒ **谁让平面每帧重画未解**。
7. `ObstacleDrawer` 仍不存在；`FunctionDrawer::draw`（arm64 `0x3911b4`）x86 侧仍未定位（§40.5 照旧）。

### 42.6 验收（可从干净树重跑）

```text
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                    # 3
"$GODOT" --headless --path client --import                              # 新增 .gd 后必须
"$GODOT" --headless --path client --script res://tools/curve_on_screen_check.gd ; echo "EXIT=$?"
    # CURVE_ON_SCREEN 96 passed, 0 failed / EXIT=0
"$GODOT" --headless --path client --script res://tools/function_drawer_check.gd   # 132 passed（未回归）
"$GODOT" --headless --path client --script res://tools/draw_classes_check.gd      # 235 passed（未回归）
cargo test --offline                              # 1512 passed（当轮基线；现行 1520）
cargo test --offline --features gdext-bindings     # 1586 passed（当轮；现行 1595。含本批 3 条单测）
python3 tools/coverage_matrix.py --check           # OK
```

headless 实测的 `Line2D.get_point_count()`（`points.len() = 100`、`active_tick = 10`）：

| 场景 | 追加 | `get_point_count()` |
|---|---|---|
| `fractional_tick = 12.0` ⇒ 期望 `trunc(30*2)=60` | 60 | **60** |
| `fractional_tick = 13.0` ⇒ 期望 `trunc(30*3)=90` | 30（只补增量） | **90** |
| `fractional_tick = 14.0` ⇒ 期望封顶 100（120>100） | 10 | **100** |
| 同一个 tick 再喂 | 0 | 100 |
| `tick = 73`（d=63）⇒ `30*63=1890 < 1900` 仍在场 | 0 | 100 |
| `tick = 74`（d=64）⇒ `30*64=1920 >= 1900` 出队 ⇒ `remove_function` | 0 | 线被删（`function_count() == 0`） |
| **进树**（`_process` 请求重画 ⇒ `_draw` 里喂）`tick=11`、`active=10` | 30 | **30** |

「进树 ⇒ 30」那一条是本轮唯一**走完整条路径**的断言：`_process` 请求重画 → 引擎调 `_draw` →
`draw()` 里 `feed_functions()` → `FunctionDrawer::update_function` → `Line2D::add_point`。
期望值在 GDScript 里按反汇编算式**独立重写**（不调 Rust 的 `revealed_points`），
常量 `30.0` / `1800` 也写死在脚本里 ⇒ Rust 那边改常量脚本会红。

## 43. 真 `GraphwarPlane` 进场景：5314 点的真实弹道由 `_draw` 一条条喂进 `Line2D`（2026-11，任务 18）

**一句话**：新增 `client/tools/plane_on_screen.tscn`（一个 `Control` + 一个**真节点** `GraphwarPlane`）
+ 接线脚本 + 验收脚本，把**一条真实弹道**（内核 `GW2Rebuild.path_calculate("cos(x)", …)` 算出的
**5314** 个点）按原版"每 tick 露 30 点"喂进棋盘的第 0 个 `FunctionDrawer`，曲线**真的画在屏幕上**。
headless 实测 `Line2D.get_point_count()`：30 → 60 → … → 5310 → **5314 == 喂进去的点数**；
`_draw` 被引擎调用 **7** 次、`draw_rect` 同样 7 次；Xvfb 截图里数出 **645** 个 `#7D6800`
像素（真帧量测的弹道色）。**76 条断言全过**（退出码 0）。`client/src/**` 一行没动。

> 档位口径同 §40 / §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 真实报文 / 真机真帧；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / 本脚本的条数**只是回归保护**，不是复原证据。

### 43.1 本轮动了哪些文件

| 文件 | 增/改 | 行数 | 内容 |
|---|---|---|---|
| `client/tools/plane_on_screen.tscn` | **新增** | 39 | 场景：`PlaneOnScreen`（Control）+ 子节点 `GraphwarPlane`（**`type="GraphwarPlane"`**，两个 `#[export]` 属性写成显式值） |
| `client/tools/plane_on_screen.gd` | **新增** | 359 | 接线：取真实弹道 → `set_function_points` → 每帧推 tick；`update_transform` 把 `Line2D` 摆进棋盘坐标系；可选截图模式（`--shot=`） |
| `client/tools/plane_on_screen_check.gd` | **新增** | 321 | headless 验收（**76 条断言**）：场景加载 / 类身份 / 点数 / 逐 tick 揭示 / `_draw` 次数 / 点的内容 / 屏幕位置 |
| `src/_scaffold/graphwar_plane_class.rs` | 改 | +34/−6 | **必需**（理由见 §43.3）：`FunctionEntry` 加 `fed` 记账；`has_revealing_function()` 的判据从"应喂 < 全长"改成"应喂 > 已喂" |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | 2 行 | 只动 `src_tree_sha256`（`src_fns` 2311 不变：本批没加 `fn`） |
| `docs/16-client.md` | +本节 | — | — |

`client/src/**`、`client/scenes/**`、`client/*.tscn`、`client/run.sh`、`src/function_drawer.rs` /
`src/graphwar_plane.rs`（纯逻辑）、`src/lib.rs`、别的 `*_class.rs`、`src/game.rs` **一行没动**；
既有断言一条未改（本轮复跑：`curve_on_screen_check.gd` 96、`function_drawer_check.gd` 132、
`draw_classes_check.gd` 235、`entity_drawer_check.gd` 72、`base_classes_check.gd` 117、
`signal_classes_check.gd` 155、`spike_class_check.gd` 23、`gdext_check.gd` 24、
`assemble_check.gd` 183、`recovered_view_check.gd` 57，**全 0 失败**）。

### 43.2 验收：76 条断言 + 数字（**观测**）

```text
$GODOT --headless --path client --script res://tools/plane_on_screen_check.gd
  → PLANE_ON_SCREEN 76 passed, 0 failed / EXIT=0
```

**核心一条**（本轮验收口径）：

```text
喂进去的点数（内核弹道） = 5314
Line2D.get_point_count() = 5314      ← 屏幕上真有的点数
```

逐 tick 的揭示（脚本里**一次 `feed_functions` 都不调**，点数只可能来自 `_draw` 里的喂点循环
`0x4b6339` → `0x4b69ec`；期望值按反汇编算式在 GDScript 里独立重写，不调 Rust 的
`revealed_points`）：

| tick | 独立算式 `min(5314, trunc(30·t))` | 本帧追加 | `Line2D.get_point_count()` |
|---|---|---|---|
| 1 | 30 | 30 | **30** |
| 2 | 60 | 30 | **60** |
| 3 | 90 | 30 | **90** |
| 10 | 300 | 210 | **300** |
| 60 | 1800 | 1500 | **1800** |
| 177 | 5310 | 3510 | **5310** |
| 178 | 5314（`30·178 = 5340 > 5314` ⇒ 封顶） | 4 | **5314** |
| 同一 tick 再喂 | — | 0 | 5314 |

`_draw` 的计数（`draw_call_count()`，**我方计数点**——Godot 的 `_draw` 没有信号可连）：

```text
进树后第一次断言：1（这一轮；帧时序不同可能是 3）
每推进一个 tick：+1（断言的是**严格增加**，不是固定值）
最后一次 report()：draws=7  rects=7  writes=8  processes=16  reveal_redraws=6
      ⇒ drawn_rect_total() == draw_call_count()（每次 `_draw` 都画了那条棋盘框）
```

点**内容**也钉住（不是只把计数凑上去）：第 0 / 2657 / 5313 个点与内核弹道（f32 化后）
逐点相等；`Line2D.default_color == #7D6800`；`position == (500,300)`、`scale == (20,-20)`、
`width == 0.125`；三个采样点按 `to_pixel + 翻 y` 落在 `(140,402)` / `(570.5,433.7)` /
`(999.9,395.4)`，全在 1000×600 的棋盘内。

断言分布（三组）：

| 组 | 条数 | 关键一条 |
|---|---|---|
| 类/方法表/内核/场景加载/`.tscn` 属性 | 33 | `plane.get_class() == "GraphwarPlane"`、`kernel.ping() == "pong:gdext"`（不是预览折线） |
| 逐 tick 揭示（30 → 60 → … → 5310） | 23 | 每次推进后 `Line2D` 点数 == 独立算式，且 `_draw` 次数 +1 |
| 揭完 + 点内容 + 屏幕位置 | 20 | `get_point_count() == 5314 == fed_points`、首/中/末点与弹道逐点相等 |

> ⚠️ `--headless` 下**没有真窗口**：`get_viewport_rect()` 只有 64×64（脚本里打印了），
> 所以"整条曲线在可见区域内"那一条只在**有显示器**时断言；headless 断言的是
> "按 `to_pixel` 落在棋盘坐标系里"（这一条与窗口大小无关）。截图证据见 §43.5。

### 43.3 为什么改了 `graphwar_plane_class.rs`（**必需**，附一个真 bug）

§42.5 第 6 条早就写着"**谁让平面每帧重画未解**"，本类补的是我方选择（`_process` 里
"正在揭示就 `queue_redraw`"）。本轮把它接进场景后，**这条补丁自己露出了一个死角**：

```text
判据原来是   revealed_points(tick) < points.len()
真实弹道 5314 点，tick 177 ⇒ revealed = 5310 < 5314  ⇒ 请求重画 ⇒ `_draw` 喂到 5310 ✓
                tick 178 ⇒ revealed = 5314 == 5314  ⇒ **算"揭完"** ⇒ 不再请求重画
⇒ 最后一帧的 `_draw` 永远不会来 ⇒ 剩下的 4 个点**永远喂不进去**
⇒ 验收口径"Line2D 点数 == 喂进去的点数"卡在 5310（第一次跑就是红的：5310 vs 5314）
```

改法（两处，都带注释）：

* `FunctionEntry` 加一个 **我方记账** 字段 `fed`：这个 id 已经通过 `update_function` 喂到第几点
  （原版这个数在 `FunctionDrawer` 的 value 里，即 §40.3a 的 `bucket+0x28`；本类从
  `FunctionDrawer::written()` **读不到**——那是 `#[func]`，Rust 侧没有入口）；
* `has_revealing_function()` 改成"**应喂 > 已喂**"：只要还有该露而没露的点，就请一帧重画。

档位：判据本身是**我方选择**（原版 `process` `0x4b83f0` 里没有 `queue_redraw`，§42.5 第 6 条
照旧）；改的不是复原出来的行为，是我们自己那条补丁。**副作用为零**：`feed_functions()` 的
返回值、`append_window` 的唯一判定、`update_function` 的入参形状一个都没变，
`curve_on_screen_check.gd`（96 条，含"进树 ⇒ 30 点"那条）复跑仍全过。

### 43.4 这条"真实弹道"的档位（**别当成原版真值**）

| 项 | 值 | 档位 / 出处 |
|---|---|---|
| 函数文本 | `"cos(x)"` | **观测**：`fixtures/protocol_match.jsonl` 第 20 行 `{"FunctionUpdateRequest":{"player_id":232,"function":"cos(x)"}}`（第二轮真实抓包，166 帧去重成 109 条，见 `docs/09-network.md` §1.4） |
| player id | `232` | **观测**：同一份抓包第 21 行 `{"FunctionFireRequest":{"player_id":232}}` |
| 起点 / 方向 | `(-18, -5.1)` / `+1` | **我方选择**：抓包的开火报文**只有** player_id，坐标与方向在客户端（士兵位置）⇒ 没有报文证据 |
| 点序列本身 | 5314 个点 | **推断**：由复原的 `graphwar_shot_calculator` / `graphwar_path_calculator` 算出（`cargo test` 与 C ABI 同口径），**不是**截下来的原版运行时点。缺的证据：原版 `GraphwarFunctionManager::set_function_points`（`0x456810`）的写入者 `0x3e44d4` 归属未证（§42.5 第 1 条照旧） |
| 点精度 | f32（`PackedVector2Array`）→ f64 → `PackedVector2Array` | **观测**：窄化发生在内核 API 边界（`src/_scaffold/api.rs` 的 `v2()`），与 `FunctionDrawer` 里那两条 `cvtsd2ss` 是同一处掉队（§40.4b），不额外丢精度 |
| 队色 | `0x0000687D`（= `#7D6800` = `Color8(125,104,0)`） | **观测**：真帧量测的弹道色（§12.2 ⑫，= `TEAM_COLORS[0][2]`）；打包时**低字节 = R**（§40.3h） |

### 43.5 截图（可选证据，**怎么重新生成**）

PNG 属于"二进制衍生内容"，**不入库**（`AGENTS.md` 三条硬规矩第 3 条）。想留图就放到 gitignored
目录（例如 `.agent-work/` 或 `rsc/` 之外的自建目录），命令是：

```text
# 1) 构建 + 覆盖 .so + import（§42.6 那三步）
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import

# 2) 抓一帧（需要 Xvfb / 显示器；headless 没有渲染器，拿不到图）
xvfb-run -a "$GODOT" --path client --rendering-driver opengl3 \
    res://tools/plane_on_screen.tscn -- --shot=<gitignored 目录>/plane_on_screen.png
```

本轮实测（Xvfb，`MODE_MAXIMIZED` ⇒ 窗口 2560×1566；棋盘 1000×600 居中）：

```text
SCREENSHOT path=…/plane_on_screen.png err=0 size=2560x1566 samples=150000 line_pixels=645 bg_pixels=147761
SCREENSHOT_TOP_COLORS ["0e251c=147761", "000000=1594", "7d6800=645"]
```

* `samples` = 棋盘矩形内**每 2 像素**采一次（500×300 = 150000）；
* `line_pixels=645` ⇒ 颜色距 `#7D6800` ≤ 0.12 的采样点（全分辨率约 4×645 ≈ 2580 像素）
  —— 这就是"曲线真的被画到屏幕上"的像素级证据，不是只涨计数器；
* `000000=1594` = `GraphwarPlane::draw` 画的那条棋盘框（空心黑框，`BOARD_*` 常量）；
* `0e251c` = `project.godot` 的 `default_clear_color`（按位等于原版 `#0E251C`，观测记录在 §5.1 第 ② 行）。
* 退出码：**Godot 自己退 0**；外层 `xvfb-run` 在 teardown 时会打
  `kill: (…)- 没有那个进程` 并以 1 退出（它的老毛病，与本次运行无关）。
  `ObjectDB instances leaked: DisplayServer / NativeMenu` 两条是引擎内部的，不是本场景的节点。

### 43.6 要替换现有仿制品，得动什么（清单：**5 个 `.gd` / 6 个节点位置 / 7 个缺口**）

现有播放屏的棋盘是 GDScript 仿制品（`client/src/screens/play_screen.gd` 自己 `new` 出来、
自己 `draw_polyline`）。**本轮没动它**——本任务只证明真类能在场景里画出来。

**（a）要删的 2 个 `.gd`**

| 文件 | 行数 | 它替代的是哪个原版类 | 换成 |
|---|---|---|---|
| `client/src/play/plane_view.gd`（`GwPlaneView`） | 527 | `GraphwarPlane` + `AxisDrawer` + `EntityDrawer` + `ObstacleDrawer`（+ `FunctionDrawer`） | `GraphwarPlane`（本轮验过）+ 下面 (b) 的节点 |
| `client/src/play/fire_fx.gd`（`GwFireFx`） | 261 | `GraphwarPlane::draw` 尾部的喂点循环（`0x4b6339`–`0x4b6a13`）+ 准备圈 | `GraphwarPlane.set_fractional_tick()` + Rust `_draw` 里的 30 点/tick（本轮验过） |

**（b）要存在的 6 个节点位置**（现在 1 个都没有）

| # | 原版 | 现在的仿制品在哪 | 状态 |
|---|---|---|---|
| 1 | `GraphwarPlane`（棋盘框 + 脏矩形 + 3 个 `FunctionDrawer`） | `GwPlaneView._draw` / `_draw_border` | **真类已存在**，本轮验过 |
| 2 | `FunctionDrawer`（每个 player id 一条 `Line2D`） | `GwPlaneView._draw_path` + `path_reveal` | **真类已存在**，本轮验过 |
| 3 | `EntityDrawer`（士兵 / 准备圈 / 名字） | `GwPlaneView._draw_soldiers` / `_draw_ready_rings` | 真类已存在（§33），**还没挂进播放屏** |
| 4 | `AxisDrawer`（网格线） | `GwPlaneView._draw_grid` | 真类已存在，**还没挂** |
| 5 | `EffectDrawer`（爆炸/火花） | `client/src/game/effect_layer.gd`（`GwEffectLayer`） | 真类已存在，**还没挂** |
| 6 | `ObstacleDrawer`（障碍圆 + 脏矩形） | `GwPlaneView._draw_obstacles` | **类还不存在**（§40.5 / §42.5 第 7 条） |

**（c）要改的 2 个 `.gd` + 1 个验收脚本**

* `client/src/screens/play_screen.gd`（2610 行）：**44 处** `_plane.*` 调用（12 个方法/字段名：
  `set_path`×5 / `clear_path`×8 / `set_targets`×6 / `set_obstacles`×4 / `set_soldiers`×2 /
  `set_show_names`×3 / `set_axis_mode`×3 / `path_color`×5 / `path_points`×2 / `queue_redraw`×2 /
  `add_child`×3 / `soldiers`×1）+ 3 处节点创建（`_plane = GwPlaneView.new()` 在第 301 行、
  `_fire_fx` 第 306–309 行、`_effects` 第 311–314 行）；
* `client/src/game/effect_layer.gd`：它靠 `plane.plane`/坐标换算画特效，换成 Rust 的
  `EffectDrawer` 后这段映射要去掉；
* `client/tools/selftest.gd`：**18 处**读 `plane.path_points` / `plane.path_reveal` 的断言
  （回放/开火的既断言）要改成读 `GraphwarPlane` 的方法。

**（d）还差的 7 个东西**（缺证据，不是缺代码）

1. **点的生产者**（§42.5 第 1 条）：`set_function_points`（`0x456810`）的写入者 `0x3e44d4`
   归属未证 ⇒ 本轮只能由场景注入；**（仍在，本任务未碰）**
2. ~~**tick 源**~~ ⇒ **已接（§48.1）**：`get_fractional_tick`（`0x519530`）的算式逐条解出并落进
   `graphwar_tick_handler::GraphwarTickHandler`（`tick + ((now-base)-accum)/0.016666666`），
   `GraphwarPlane` 自己持有一个 tick handler 并按帧调它（`0x4b6317` 的 u32 tick 取 `M+0x40`）。
   剩下的**只是接线**：原版那个对象在游戏状态里（`draw` 里 `mov r14,[r12+0x238]`），
   本类还没从那儿取 ⇒ 自己 `new` 一个 + `set_fractional_tick()` 当摆钟口（**我方**）；
3. ~~**`update_transform` 的三个实参**~~ ⇒ **已解出并接上（§48.2）**：
   `x = w/2`、`y = h/2`、`scale = w/50`（f32 标量，`.rodata 0x1b9fc8`）、
   `flip` = `get_team_overview` 的位组合（`(flags≥2^24 && !bit16) || ((flags&0x101)==0x100)`）。
   最终 `set_scale` 的两个分量 = `flip ? (-s,-s) : (s,-s)`。**不再是我方选择**；
   唯一还是我方的是 `flip` 的来源（拿导出属性 `mirror_x` 顶替那个位组合）；
4. **淡出**：`get_function_fade_on_tick` 没有 x86 落点（走 GOT 槽 `0x9bbd08`）⇒ 没接；**（仍在）**
5. **`z_index` 初值未知**（§40.3f）；**（仍在）**
6. **`FunctionDrawer::draw`**（arm64 `0x3911b4`）x86 侧仍未定位（不影响曲线：曲线是 `Line2D`）；**（仍在）**
7. ~~**棋盘底色/网格/队色表**（`get_team_overview`，`0x4be6e0` 的两张表）~~ ⇒ **表解出来了，
   归属也纠正了（§48.3）**：
   * `get_team_overview`（`0x4be6e0`）返回的是 **`u32` 的 4 个布尔位**（bit 0/8/16/24），
     **不是两张表**；`draw` 里两处调用（`0x4b5fd3`/`0x4b60e3`）只用它算 `flip`；
   * 真正的"两张表" = `init_team_colors`（x86 **`0x465070`**，arm64 `0x49c660`）里的
     **两个 6 色 `Vec<GraphwarColor>`**（键 = 队号 0/1），12 个颜色在 x86
     `0x4650a3`–`0x465297` 逐条 `mov word` + `mov byte` 落库，与 `docs/10` §15.3 的 arm64
     复算**一字不差**；表已落在 `src/graphwar_color_manager.rs` 的
     `TEAM1_COLORS`/`TEAM2_COLORS`，本类用 `team_color_pool(team)` 暴露出来（验收断言 12 个分量）；
   * **棋盘内部的白底不是队色**：它是 `AxisDrawer::draw` 的 `draw_rect(filled=true,
     Color(1,1,1,1), rect(-25,-15,50,30))`（`0x4e069f`–`0x4e06ca`）。
     `AxisDrawer` 节点还没实现 `draw` ⇒ 本类加了导出属性 `board_fill`（**默认关**）
     照那组实参补画白底；**播放屏要"不再是工程底色"就得把它置真**（或等 AxisDrawer 补 `draw`）。

### 43.7 还没做 / 未知（诚实清单）

* **客户端播放屏仍是仿制品**：本轮只证明"真类 + 真弹道 + 真喂点路径能画出来"（§43.6 是路线图）。
  §42.5 第 5 条那句"客户端还没用真类"**对播放屏仍然成立**，只在这里补一条：
  现在**存在**一个能跑的场景（`client/tools/plane_on_screen.tscn`），不再是"只能 headless 调方法"。
* **`_process` 里那条重画策略仍是我方选择**（§43.3 改的是它的一个死角，不是复原它）。
* **`fed` 是第二份记账**：原版在 `FunctionDrawer` 的 value 里（§40.3a），本类在棋盘侧又记一份。
  两份**不会分叉**（都只由 `update_function` 的追加决定），但这是"本类没有 `FunctionDrawer`
  的 Rust 入口"造成的重复；等 `FunctionDrawer` 的访问器公开后应删掉这一份。
* ~~**截图里的棋盘内部没有网格/底色**（原版有；`AxisDrawer` 与 `GraphwarPlane::draw` 的
  `draw_set_transform` 实参都还没定死）~~ ⇒ **实参定死了（§48.2）**，底色也定位了
  （`AxisDrawer::draw` 的白底矩形，§48.3）；但 `AxisDrawer` 节点仍没实现 `draw`，
  所以那张截图里**仍然**没有网格；底色现在可以由 `GraphwarPlane.board_fill = true` 补上。
* **`--headless` 的视口是 64×64**（不是 1600×900）：任何"像素位置"类断言在 headless 下
  只能测到"棋盘坐标系里对不对"，测不到"窗口里对不对"；后者靠 §43.5 的截图。

### 43.8 复现（三分钟）+ 回归数字（本分支实测）

```text
export CARGO_HOME=<工作区>/.cargo-home CARGO_TARGET_DIR=<工作区>/target
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
"$GODOT" --headless --path client --import                              # 新增 .gd/.tscn 后必须
"$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd ; echo "EXIT=$?"
    # PLANE_ON_SCREEN 76 passed, 0 failed / EXIT=0
cargo test --offline                              # 1520 passed, 0 failed（基线同数）
cargo test --offline --features gdext-bindings     # 1595 passed, 0 failed
python3 tools/coverage_matrix.py --check           # OK
```

`bash client/run.sh selftest`（无 assets）：本环境 **530 passed / 2 failed / 25 skipped**
（退出码 1）。⚠️ 任务书写的基线是 **606 / 2 / 23**，**这里复现不出来**：把
`src/_scaffold/graphwar_plane_class.rs` 退回 `418319d`、重编 `.so` 再跑一遍，数字**逐字相同**
（530/2/25）⇒ 差异来自环境（缺 `assets` / 缺回放样本：日志里 `SKIP 回放：约定的样本 … 不在`），
**不是本批改动造成的**。两条 FAIL 也是既有的（Skins 页皮肤、Cancel/Start 按钮宽度）。

## 44. 5 个 manager 从"有键"到"能交互"：装配 + 事件流 + 一次最小交互（2026-11，任务 19）

**一句话**：§41 让 22 个 manager **进**了容器，但**没有一个是能调的**。本节挑 5 个
（`life` / `target` / `obstacle` / `function` / `tick_handler`）补上
**构造 + 注册 + 按类型取回 + 一次最小交互**，并把它们的**事件流**（订阅什么、发什么）
逐条指到 x86 指令；`assemble_check.gd` 从 183 条扩到 **199 条**，
新增的 16 条里有 27 个"交互后读回"的值，与 `root_wiring.rs` 的单测**逐字相同**。

> 档位口径同 §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。`cargo test` / 本脚本的条数**只是回归保护**。

### 44.0 先说"没做到"的部分（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| `GraphwarFunctionManager`（32 字节）的**字段** | **一个都没解** | **未知** |
| `GraphwarObstacleManager`（80 字节）的**字段** | **一个都没解** | **未知** |
| `GraphwarTickHandler`（72 字节）的字段 | 只落了**有地址证据**的几个（见 §44.2），**没有**按字节复原 | 部分**观测** |
| 这 3 个槽的档位 | **仍然是 `stub` / `impl_type = "—"`**（上一轮已经进了逐槽断言，本轮不改既有断言）→ ⚠️ **§47 已改**：`TickHandler` / `ObstacleManager` = `wired`，`FunctionManager` = `layout`，`impl_type` 报真类型路径 | 我方口径（§47.2 重判） |

⇒ **`stub` 的含义就地更正**：上一轮它是"**零行为**的空占位"，本节起它是
"**原版结构未复原**"（其中 3 个现在**能交互**）。两件事分开验：
档位看 `module_slot_status`，能不能交互看 `manager_probe_selftest`。

### 44.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_life_manager.rs` | +接线层 | 3 条订阅名 + 5 个判别值 + `set_alive` / `set_queued_death` / `get_alive` / `get_death_tick` / `get_all_entities` / `handle_tick_event` / `handle_entity_removed_event` / `send_all_info_events` + `outbox` + 4 条单测（纯逻辑 `set` / `status` / `take_queued_death` **一行没动**） |
| `src/graphwar_target_manager.rs` | +接线层 | `handle_new_connection_event`（判别值闸 5）+ `take_outbox` / `outbox_len` |
| `src/graphwar_tick_handler.rs` | +类型 | `GraphwarTickHandler`（累加器 / `current_tick` / `tick()` / `handle_connected_to_server_event` / 接已有 `tick_at`）+ 5 条单测（两个自由函数**一行没动**） |
| `src/_scaffold/root_wiring.rs` | 改 | 3 个槽从零字段变成带状态（`TickHandlerSlot(GraphwarTickHandler)` / `FunctionManagerSlot(FunctionTable)` / `ObstacleManagerSlot(ObstacleTable)`）+ 5 个按类型取回口 + `interaction_selftest` + 5 条单测 |
| `src/_scaffold/graphwar_root_node_class.rs` | **+2 个 `#[func]`** | `manager_probe_selftest()` / `manager_probe(name)`——**我方自造**的只读观测口，原版**没有**（见 §44.5 的边界） |
| `client/tools/assemble_check.gd` | +16 条 | 27 条交互值 + 幂等 + 反面（桩仍报 stub）+ 未知名字（⚠️ §47 起反面段断的是**四档**，5 个包装槽里 4 个不再是 `stub`） |
| `docs/16-client.md` | +本节 | — |

❌ `src/game.rs` / 别的 manager / `client/src/**` / `src/lib.rs` / 任何既有断言 **一行没动**。

### 44.2 5 个 manager：订阅什么、发什么（全部 `观测`，x86 2026-08-28）

复算工具：`../.venv/bin/python tools/addr_probe.py dis <vaddr>`（本机 `capstone`，离线）。

| manager | 原版 `new` | 订阅（按调用序） | 发出的事件（tag 写在哪条指令） | 本轮接上的入口 | 档位 |
|---|---|---|---|---|---|
| `graphwar_life_manager` | `0x454790` | `new_connection_event`(5) / `entity_removed_event`(14) / `tick_event`(0) | `LifeInfo`(27) @`0x454c52`（`set_alive`）、@`0x4542af`（`send_all_info_events`）；`LifeQueuedDeath`(28) @`0x453f6d`（`set_queued_death`） | `set_alive` `set_queued_death` `get_alive` `get_death_tick` `get_all_entities` `send_all_info_events` `handle_tick_event` `handle_entity_removed_event` | 订阅/发 **观测** |
| `graphwar_target_manager` | `0x465910` | `new_connection_event`(5) / `entity_removed_event`(14) | `TargetInfo`(23)（§41 前的逐位结论不变） | `add_target` `contains` `handle_new_connection_event` `handle_entity_removed_event` | 订阅 **观测**（与既有 `逐位` 一致） |
| `graphwar_obstacle_manager` | `0x3f2360` | **只一条** `tick_event`(0) | `AddObstacle`(40) @`0x3f1876`、`DestroyObstacle`(41) @`0x3f1c09`、`QueueDestroyObstacle`(42) @`0x3f22b2`、`ClearObstacles`(39)（`clear_obstacles`） | `add_obstacle` `is_obstacle` `queue_destroy_obstacle` `destroy_obstacle` `clear_obstacles` `handle_tick_event` | 订阅/发 **观测**；延迟 165 tick **推断** |
| `graphwar_function_manager` | 内联（`0x5236e0` 由 `0x517b9f` 调，紧跟 `add_module` `0x469db0`） | **不订阅**（无 `subscribe_to_events_event_handler`，构造体内无 `0x3d38f0` 调用） | **不发**（四个 `set_*` 体内无 `0x3d3530` 调用） | `set_function` `get_function` `get_function_text` `set_function_active_tick` `set_function_points` `get_active_functions_on_tick` | **推断**（正对照见 §44.3） |
| `graphwar_tick_handler` | `0x519690` | **只一条** `connected_to_server_event`(7) | `TickEvent`(0)（`tick()` @`0x51988b`，载荷 = `current_tick`）、`TickRequest`(1)（`handle_connected_to_server_event` @`0x51964c`；以及 `tick()` 里 `current_tick % 60 == 0` 时 @`0x5198de`） | `tick` `handle_connected_to_server_event` + 接已有 `tick_at` | 订阅/发 **观测** |

三条**顺带更正/新证**：

1. **§41.6 那行"21 号之前 3 条 `subscribe_to_event` 订阅的是谁未解"⇒ 已解**：
   `0x5179bb` / `0x517a0e` / `0x517a5e` 三条订阅的是
   `connected_to_server_event`(len `0x19`) / `player_info_event`(len `0x11`) /
   `entity_removed_event`(len `0x14`)，正好是 `GraphwarLocalPlayerManager` 的
   `handle_connected_to_server` / `handle_player_info` / `handle_entity_removed_event`
   三个处理器 ⇒ **它们属于 3 号 `GraphwarLocalPlayerManager`（构造被内联）**，不是 `TickHandler`
   （`TickHandler` 的订阅在它自己的 `new` `0x519690` 里，只有 1 条）。
2. **`GraphwarTickHandler::new` 的实参**：`0x519690` 里 `M+0x00..0x0f` = `SystemTime::now()`
   （call `0x8f3a10`）、`M+0x28 = 0x3b9aca00`（1e9 ns）、`M+0x38` = 传入的事件队列
   `Rc`、`M+0x40 = 0`（`current_tick`）。⇒ 本层的 `base_tick` / `base_ms` 是**我方形状**
   （把"壁钟基准"换成抓包基准，好接已有的 `tick_at`）。
3. **`tick()` 的步长是 1/60 秒**：`0x51986c` `add eax, 0xfe502a` = **16 666 666 ns**
   （截断值，不是四舍五入的 16 666 667），溢出路径的串是
   `'overflow when adding durations'`（`Duration` 的加法）。那个"判 60 整除"的分支是
   幻数 `imul 0xeeeeeeef` + `ror 2` + `cmp 0x4444444`。

### 44.3 负结论的正对照（`AGENTS.md` 合并纪律 A-4）

`function_manager` 那两条"不订阅 / 不发"是**负结论**，所以先证明扫法有效：同一条命令
`addr_probe.py dis <vaddr>` 在另外 4 个 manager 的 `new` 体内**都**能看到
`call qword ptr [...]; slot 0x9bac88 -> 0x3d38f0`（= `EventHandler::subscribe`）
和它的 `lea rsi,'<事件名>'` + `mov edx,<len>`：

```text
life   0x454790 → 0x454812 / 0x454867 / 0x4548b8 三条 subscribe（+3 条事件名 lea）
target 0x465910 → 0x465995 / 0x4659e6 两条
obstac 0x3f2360 → 0x3f23f0 一条（'tick_event', len 0xa）
tick   0x519690 → 0x519762 一条（'connected_to_server_event', len 0x19）
func   0x5236e0 → **零条**
```

发事件同理：`GraphwarEventQueue::add_event` 是 `slot 0x9bac80 -> 0x3d3530`，
在 `set_alive` / `send_all_info_events` / `set_queued_death` / `add_obstacle` /
`destroy_obstacle` / `queue_destroy_obstacle` / `tick` / `handle_connected_to_server_event`
里**都**有；在 `set_function`(0x455e30) / `set_function_points`(0x456810) /
`set_function_active_tick`(0x456b80) 里**零条**。

### 44.4 落码：容器里现在装的是什么

```text
TickHandlerSlot(GraphwarTickHandler)     # graphwar_tick_handler.rs（原版模块文件）
FunctionManagerSlot(FunctionTable)       # root_wiring.rs（我方形状，原版字段未解）
ObstacleManagerSlot(ObstacleTable)       # root_wiring.rs（我方形状，纯逻辑复用 plane.rs）
LifeState                                # graphwar_life_manager.rs（原类型 + 接线层）
GraphwarTargetManager                    # graphwar_target_manager.rs（原类型 + 接线层）
```

新增的取回口（都走 `get_module::<M>()`，不是查表）：`tick_handler()` / `life_manager()` /
`target_manager()` / `function_manager()` / `obstacle_manager()`。

`ObstacleTable` 的纯逻辑**全部是已有函数**：光栅化 `plane::rasterize`（与 `0x3f11e0` 同构）、
单点查询 `plane::is_occupied`（与 `0x3f15b0` 同构）、延迟 `game::DESTROY_DELAY_TICKS`（165，
`docs/44` 的实测 163.9 / 164.0 / 167.0）。`FunctionTable::get_function_text` 走
`math_function::to_text`（`docs/119` 1032/1032 对拍的那个函数）。

### 44.5 headless 那侧：**我方自造的**观测口（原版没有）

`RootWiring` 是 Rust 结构体，GDScript 只能调 `#[func]`。§41 之前那 18 个 `#[func]`
全是"数个数 / 查键 / 取类型名"这类**只读静态**口，**没有任何办法触发一次 manager 交互**
⇒ 本轮在 `graphwar_root_node_class.rs` 里**只加**了两个（既有 18 个与既有断言**一条没动**）：

```text
manager_probe_selftest() -> PackedStringArray   # 5 个 manager 各一次最小交互，27 条 "名字=值"
manager_probe(name) -> GString                  # 单条查询；未知名字 ⇒ 空串
```

边界（写死在这里，别当成复原成果）：

* **原版没有这两个方法**，它们是验收工具（`src/_scaffold/` 里本来就没有原版对应物）；
* **只读**：不碰 22 个槽的注册，不碰 `Match` / `TurnManager` / `PlayerManager`
  （那三个是既有断言在读的）；动的只有这 5 个 manager 自己表里的固定 id（`9001..=9003`）
  与固定坐标，每次调用末尾把发件箱取走；
* **幂等**：连调两次返回**同一串**（`assemble_check.gd` 就是这么比的）；
* **值是真算的**：每一条都是"写进去 → 从被写的那张表读回来"，没有一条 `return` 常量
  （反证：`root_wiring.rs` 的 `the_read_back_values_track_the_written_state` 换了输入，
  读回值跟着变）。

27 条期望值（两边逐字相同：GDScript 的 `expected_probe` 与 Rust 的 `expected_selftest()`）：

```text
life.alive=1  life.death_tick=42  life.due_at_42=1  life.death_tick_after=none  life.subs=3
target.contains=true  target.replayed=1  target.removed=true  target.contains_after=false  target.subs=2
obstacle.hit_after_add=true  obstacle.hit_far=false  obstacle.pending=1
obstacle.due_164=0  obstacle.due_165=1  obstacle.hit_after_destroy=false  obstacle.subs=1
function.text=x  function.active_tick=100  function.active_at_101=1
function.active_at_100=0  function.active_at_200=0  function.subs=0
tick.delta=1  tick.event_tag=0  tick.at_500ms=30  tick.subs=1
```

### 44.6 验收（本节实测）

```text
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 199 passed, 0 failed
```

| 命令 | 本节实测 | 基线（`CG@418319d`，同一条命令） |
|---|---|---|
| `cargo test --offline` | **1530**，全绿 | **1520**（本节 +10：`life_manager` 5 条 + `tick_handler` 5 条） |
| `cargo test --offline --features gdext-bindings` | **1609**，全绿 | **1595**（本节 +14：上面 10 条 + `root_wiring` 4 条——那个模块挂在这个 feature 下） |
| `assemble_check.gd` | **199 passed / 0 failed**，退出码 0 | **183**（§41；本节净 +16）。⚠️ §47 起是 **203**（四档断言） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_fns` 2311 → **2385**，`src_tree_sha256` 跟着变） | OK |

**两条护栏（三条驱动轴，分开跑、分开报，`AGENTS.md` 合并纪律 C）**：

| 轴 | 命令 | 本节实测 | 基线 |
|---|---|---|---|
| `fire` | `cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire --policy default` | 内容命中 **325 / 1080** | **325**（一位不差） |
| `batch` | `… --drive batch --policy default` | 内容命中 **346 / 1080** | **346**（一位不差） |
| `batch_conv` | `cargo run --release --offline -q --example batch_conv -- --dir <快照> --policy strict` | 种类序列 **1074 / 1080**、内容命中 **595 / 1080** | **1074 / 595**（一位不差） |

⇒ 本节只动 `_scaffold/` 与 5 个 manager 的**接线层**，一条几何/弹道代码都没碰，
三条轴逐数字不动，符合"接线层不该动几何"的预期。

**新增 16 条的构成**：2 条（两个新 `#[func]` 存在）+ 2 条（27 条整串 / 条数）+
5 条（5 个 manager 各一条关键值）+ 5 条（5 个槽仍报 `stub`，反面）+ 2 条（幂等 / 未知名字）。
既有 183 条**一条没删、一条没改**。

### 44.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `GraphwarFunctionManager` / `GraphwarObstacleManager` 的**字段布局** | **未知** | 只解到函数名与落点（`fixtures/name_to_addr.tsv`）；两者的 `new` / `set_*` 体内字段偏移没逐条读。所以本轮**不**冒充类型名，装的是我方形状 |
| `queue_destroy_obstacle` 的**到期时刻算法** | **推断** | 用 `docs/44` 的实测 165 tick；`0x3f20f0` 里那个加法（`now + ?`）没逐条解 |
| `life::handle_tick_event` 到期后调 `set_alive` 的**实参** | **未知** | `0x4540aa` 那个调用点只看到 `call 0x454b40`，两个实参的来源没解 ⇒ 本层只把到期项**取出来返回**，不改第一张表 |
| `get_fractional_tick`(`0x519530`) / `get_time_to_tick`(`0x519450`) 的**算式** | **未知** | 两个函数有落点、体内公式没读。本层只给了一个**我方**派生量 `elapsed_tick_units()`，名字都没沿用 |
| `get_active_functions_on_tick` 的**点条数** | **推断** | §42.2 的三条谓词已落码，但本表不存曲线点 ⇒ `len` 按 0 算，窗口退化成 60 tick 尾巴 |
| `function_manager` **不订阅/不发事件** | **推断**（有正对照） | 构造路径 `0x5236e0` 体内无 `0x3d38f0`/`0x3d3530` 调用；但那个闭包可能被内联到别处（没有独立符号可核） |

## 45. `ObstacleDrawer` 成了真节点类 —— 客户端替换清单里最后一个"类还不存在"的缺口（2026-11，任务 23）

**一句话**：§40 把棋盘上 3 个占位子节点换成了真 `FunctionDrawer`，但障碍层**一行代码都没有**。
本节补上 `ObstacleDrawer`：`#[derive(GodotClass)] #[class(base = Control)]`，
把 1000×600 的障碍掩膜拼成一张 **`LA8`** `Image` → `ImageTexture` → **一个** `Sprite2D` 子节点，
按平面的 `Control::get_size()` 摆位；接进 `GraphwarPlane` 后由 `process` **每帧刷 2 次**
（原版就是这个数）。`client/tools/obstacle_drawer_check.gd` **116 条断言全过**，退出码 0。

> 档位口径同 §42：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址/指令级读写；
> **推断** = 有依据未证；**未知** = 没解出来。脚本条数**只是回归保护**，不是复原证据。

### 45.1 一页结论

| 项 | 结果 | 证据 |
|---|---|---|
| 基类 | **原版没有基类**（`ObstacleDrawer` 不是 Godot 类）；本类是 `Control` | 原版负面结论是**观测**（见 §45.2）；`Control` 是**推断** |
| 结构体字段 | **4 个**：`Gd<ImageTexture>` / `Gd<Sprite2D>` / `PackedByteArray` / `Gd<Image>` | **观测**（`drop_in_place` @ `0x39518c`，268 B，逐字段析构） |
| 图的规格 | `Image::create_from_data(1000, 600, false, **1 = LA8**, ..)` | **逐位**（立即数 `0x3e8` / `0x258` / `wzr` / `#0x1`） |
| 像素映射 | `LA8 = (L = 0, A = !mask_byte)` ⇒ 障碍 = 不透明**黑**、空 = 全透明 | **逐位**（`mvn w8, w23` + `sturb wzr`） |
| `Sprite2D` 摆法 | `position = size × 0.5`；`k = size.x / 1000`；`scale = (±k, **−k**)` | **逐位**（`fmov s2,#0.5` / `0x447a0000` / `fneg` / `fcsel`） |
| 每帧几次 | **2**（`process_plan()` 已记；本类照做，不再只是计数） | **观测**（§36 / `src/obstacle_drawer.rs`） |
| 新增代码 | `src/_scaffold/obstacle_drawer_class.rs` 683 行（**真实代码 257 行**，去掉注释/文档/单测） | 本节 §45.7 有复现命令 |
| Plane 侧改动 | **+60 行 / −0 行**（其中**代码 32 行**） | `git diff --stat` |
| 验收 | `obstacle_drawer_check.gd` **116 passed, 0 failed**；`cargo test` 1559 / 0；gdext 轴 1642 / 0；`selftest` **694 / 0**；既有 11 个 headless 脚本**一条没回归** | 本节 §45.7 |

### 45.2 基类：**原版根本没有基类**（这条必须先说清，别把它当成"已复原的 `Control`"）

任务书问"基类要有证据"。本轮查完的答案是：**原版 `ObstacleDrawer` 不是 Godot 类**，
是 `GraphwarPlane` 的一个**普通字段**，所以它没有基类可抄。两条命令、一张表：

```text
# ① 谁注册成了 Godot 类：gdext 给每个 GodotClass 生成 __registration_storage
readelf -sW libgraphwarrust.so | c++filt | grep "__registration_storage" \
  | sed -E 's/.*graphwarrust::([a-z_0-9]+)::([A-Za-z]+).*/\1::\2/' | sort -u
   → 12 个：axis_drawer / effect_drawer / entity_drawer / graphwar_ad_switch / graphwar_button /
            graphwar_function_input / graphwar_lan_vbox / graphwar_long_press_animation /
            graphwar_plane / graphwar_root_node / graphwar_setup_control / player_showcase
     **没有 obstacle_drawer，也没有 function_drawer**

# ② 基类（符号名里带 gen::classes::<base>::；§35.3 的那条命令）
   → 11 行（正对照：effect_drawer/effect_drawer 那批确实扫得出来）
   → grep -cE "function_drawer|obstacle_drawer" = 0
```

| 判据 | `ObstacleDrawer` | 正对照 |
|---|---|---|
| `__registration_storage::STORAGE` 符号 | **没有** | 12 个类都有（`GraphwarAdSwitch` 也有 —— §35.4 就是靠它定的性） |
| `impl I<Base>`（`gen::classes::…`） | **没有** | 11 个类有 |
| `.rodata` 里的**类名字面量**（`GraphwarAdSwitch` 那种 `type_name` 游程） | **没有**（全文件 4 处 `ObstacleDrawer` 全是 `.strtab` 的 mangled 符号名） | `GraphwarAdSwitch` 有游程 |
| 运行时真·节点树 dump（§2.1.10） | `GraphwarPlane` 下**没有** `@ObstacleDrawer@N`（只有 `@AxisDrawer@N` / `@EntityDrawer@N` / `@EffectDrawer@N` / `@Line2D@N`） | 同上 |
| `GraphwarPlane::init`（`0x4b7380`）里有 `obstacle_drawer.rs:37:97` 的 `panic::Location` | **有** | `src/obstacle_drawer.rs` 的 `FUNCTIONS[1]` 已记 |

⇒ **本类选 `Control` 是推断**，两条理由（都**不是**原版证据）：

1. **同族**：`*Drawer` 里凡是真注册成类的（`AxisDrawer` / `EffectDrawer` / `EntityDrawer`）
   基类**全是 `Control`**（§35.3 观测）；
2. **它按 `Control` 的语义布局**：`update` 那两次 `Control::get_size()` 拿的是**平面的**尺寸，
   Sprite2D 的 `position` / `scale` 由它算出（§45.3d）。

**缺的证据**（不硬说定死）：原版那个结构体在场景树里**没有对应节点**，所以"该是 `Control`
还是 `Node2D`"没有可观测对象；要定死只能拿到 2024 版带 `impl I<…>` 的符号表的二进制，
或抓到 `GraphwarPlane::init` 里那次构造的 `Gd::default_instance` 目标类名。
⚠️ 同一条判据也适用于 **`FunctionDrawer`**：它**不在**那 12 个里（§40 的类注释说"原版是
`Control` 子类"，那是**推断**，本节的扫描是对它的**更正/降档**）。

### 45.3 从指令里读出来的五条（`update` = arm64 `0x422590`，4688 B）

#### (a) 结构体只有 4 个字段（**观测**：`drop_in_place<ObstacleDrawer>` @ `0x39518c`）

那 268 字节的 drop glue 只碰 3 个"需要析构"的字段，而有 4 个字段拼得进去：

```text
+0x00  Gd<ImageTexture>   24 B   upcast_ref<RefCounted> + RefCounted::unreference（0x3951bc）
+0x18  Gd<Sprite2D>       24 B   **没有**析构代码（Node 不是 RefCounted，`Gd<Node>` 不保活）
+0x30  PackedByteArray    16 B   PackedElement::ffi_destroy（0x3951f8）
+0x40  Gd<Image>          24 B   同上；cleanup 路径里直接写 drop_in_place<Gd<Image>>
```

`update` 里三条指令印证这三个偏移：`add x0, x26, #0x20` 是那个 sprite 的 `RawGd`
（`+0x18` 的 `Gd` 的 obj/id 半边）、`add x4, x19, #0x30` 是像素缓冲、`ldr x0, [x21, #0x40]!`
是那张 `Image`。⇒ **一张图、一个贴图、一个 sprite**（不是"每个障碍一个 `Sprite2D`"），
而且 **结构体里没有 `HashMap`、没有父节点句柄**。

⚠️ **就地更正 `src/obstacle_drawer.rs` 文档里的一句话**（那个文件一行没改，按 §37.3 的做法记在这里）：
它写"`hashbrown::rustc_entry` ×3 … 按 key 缓存那些节点"。按 (a) 的字段表，那 3 次 entry 的 map
**不在 `ObstacleDrawer` 的字段里** ⇒ 读作"缓存的是**障碍数据**（`Mutex<GraphwarObstacleData>`，
`0x4226e4` 那条 `ldxr` 锁）里的表"更合适。**档位：观测（字段数）+ 推断（那 3 次 entry 归属哪个 map）**，
缺的证据是从 `0x422828` / `0x422df4` / `0x42306c` 三个 entry 调用点往上追它读的是哪个基址。

#### (b) 图是 **1000×600 / `LA8` / 无 mipmap**（**逐位**）

两处 `Image::create_from_data`（`0x422780` / `0x422d1c`）的实参**完全相同**：

```text
0x422764  mov w0, #0x3e8     ; 宽 1000
0x422768  mov w1, #0x258     ; 高 600
0x42276c  mov w2, wzr        ; use_mipmaps = false
0x422770  mov w3, #0x1       ; Format = 1 = LA8（Godot 4：L8=0 / LA8=1 / R8=2 / RG8=3 / RGB8=4 / RGBA8=5）
0x422760  add x4, x19, #0x30 ; &self.pixels（+0x30 那个 PackedByteArray）
```

1000×600 与 `plane::CANVAS_W/H` 一致 ⇒ **掩膜像素与图片像素一对一**（不需要再缩放）。

#### (c) 掩膜字节 → 像素：**L = 0、A = `!m`**（**逐位**）

`0x422cbc`–`0x422cf0` 的循环，每个掩膜字节 push **两个**字节（`op_push_back` ×2）：

```text
0x422cbc  ldrb  w23, [x22, x20]    ; m = mask[i]
0x422cc0  sturh wzr, [x29, #-0xa8] ; pair = [0, 0]
0x422ccc  bl    op_push_back       ; 推 L = 0
0x422cd0  mvn   w8, w23            ; !m
0x422cd8  sturb w8, [x29, #-0xa7]  ; pair[1] = !m
0x422ce4  bl    op_push_back       ; 推 A = !m
```

掩膜口径（`plane::rasterize`）：`0x00` = 障碍、`0xFF` = 空 ⇒ **障碍 = 不透明黑、空 = 全透明**。
另一条路径（`0x422bfc`–`0x422c3c`）改用 `ffi_index_mut(2*(row*1000+col)+1)` **只写 A 通道**，
写的还是 `mvn`（`0x422c38`）——两条路径映射一致。⇒ "障碍物是黑的"是**从指令读出来的**，
不是我们挑的颜色（颜色真要复原得等 `0x30` 那个缓冲的**来源**解出来，见 §45.6）。

#### (d) `Sprite2D` 的摆法（**逐位**）

```text
0x422608 / 0x42262c  Control::get_size ×2       ; 平面的像素尺寸（原版调两次；本类调一次，同值）
0x42260c  fmov s2, #0.5                         ; 0.5
0x42261c  fmul s8, s0, s2 / fmul s9, s1, s2     ; position = size * 0.5（居中）
0x422630  mov  w8, #0x447a0000                  ; 1000.0f32
0x422644  fdiv s10, s0, s1                      ; k = size.x / 1000
0x42266c  fneg s1, s10                          ; scale.y = -k（**永远**取负）
0x422678  fcsel s0, s1, s10, ne                 ; scale.x = (w5 != 0) ? -k : k
```

`s = (k, −k)` 与 `plane` 自己的 `draw_set_transform(scale = (20, −20))` 是同一个取向
（棋盘 y 向上、画布 y 向下）。那个 `w5` 是第 5 个入参（`0x4225fc` 存进 `w20`），
**语义未知**，本类命名 `flip_x`（默认 `false` ⇒ `scale = (k, −k)`）。

#### (e) `update` 的入参形状 + `add_child` 的父（**观测**）

```text
x0      = &mut self
x1, x2  = 一个 16 B 的 RawGd（`stp x1,x2,[sp,#0x50]` @ 0x4225dc）→ 紧跟 upcast_ref
          （字面量 "upcast_ref" @ 0x19d85a，长 10）+ Control::get_size ⇒ **平面节点**
x3      = 指针（`mov x19, x3`），它的 `+0x40` 被当 GraphwarModuleContainer 用：
          `ldr x19,[x19,#0x40]` → `get_module(x19+0x18)` @ 0x4226a0
x4      = 一份 ptr/len 借用（`[x23]` / `[x23+0x8]` / `[x23+0x20]`）——障碍数据那一侧
w5      = bool（见 (d)）
```

`add_child_full`（`0x423224`）的**父**是 `x1/x2` 那个节点（`0x4231f8` 又 `upcast_ref` 了一次）
⇒ **原版把 `Sprite2D` 挂到平面上**（因为本结构体不是节点、也不存父句柄）。
**本类的差别**：挂成自己的子节点（任务书要求"`Sprite2D` 子节点"），于是坐标原点 = 本节点原点；
只要本节点贴在平面左上角（`position = (0,0)`），`position = size*0.5` 的语义与原版**逐像素相同**。

### 45.4 与原版的差别（逐条）

| 项 | 原版 | 本文件 | 档位 |
|---|---|---|---|
| 基类 | **没有**（平面的普通字段） | `Control` | 原版**观测**（负面）；`Control` 是**推断** |
| 像素缓冲 | `PackedByteArray`（`+0x30`），逐字节 `op_push_back` | 先拼 `Vec<u8>`，再 `PackedByteArray::from(..)` 一次交出去 | 布局**观测**；批量是**我方选择**（逐字节 FFI 每帧 120 万次不划算） |
| 贴图 | `create_from_image` 一次 + `update` N 次（`+0x00` 那个字段就是为 `update` 留的） | 同：首次 `create_from_image`，之后 `Image::set_data` + `ImageTexture::update` | **观测**（`CALLED_API` 里两条都在） |
| `create_from_data` 每帧 2 次 | 是（`0x422780` / `0x422d1c`，两条路径） | 首次 1 次 + 之后每次 `set_data` | 次数**观测**；化简是**我方选择**（两条路径各管什么**未知**） |
| 掩膜从哪来 | `GraphwarObstacleManager` 的 `Mutex<GraphwarObstacleData>`（每帧锁一次） | 本类自己持一张；改它的仍是 `plane::rasterize` | 锁**观测**；"自己持"是**化简**（那个 manager 还没节点化） |
| 脏矩形 | 由 `graphwar_plane` 持有，本模块入参里没有矩形 | **同**（本类也不收矩形，每次刷整张图） | **观测** |
| sprite 的父 | 平面（§45.3e） | 自己 | 原版**观测**；差别是**我方选择** |
| 名字 | 无（不是节点，没有 `set_name` 的机会） | 不 `set_name` ⇒ 自动名 `@ObstacleDrawer@N` | 我方选择（同 §40） |

### 45.5 接进 `GraphwarPlane`：为什么**默认不建**

三处改动（`git diff` 全文 60 行）：

1. 字段 `obstacle_drawer: Option<Gd<ObstacleDrawer>>`；
2. `process` 里照 `process_plan()` 给的 **2** 真的刷两次（没有障碍层时退回"只计数"，行为与改前一致）；
3. `#[func] build_obstacle_drawer()` / `obstacle_drawer_count()` / `obstacle_drawer_updates()`。

**为什么不自动建**：既有验收脚本断言过平面的子节点数 —— `draw_classes_check.gd` 的
`c.get_child_count() == 3`（`build_children` 之后）、`assemble_check.gd` 的 `1` / `4`。
自动建就会把它们打红。⇒ 改成显式调用，`obstacle_drawer_check.gd` 里**两种情形都断言**：
`plane.get_child_count()` 在"没建"时仍是 0、建完 1、进树后 4（3 个 `FunctionDrawer` + 1 个障碍层）。

### 45.6 还没做 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| 障碍的**颜色**（现在照 (c) 是**黑**） | 黑是**逐位**；"原版最终是什么颜色"是**未知** | `0x30` 那个 `PackedByteArray` 的**上游**：原版是先往缓冲里 `ffi_index_mut` 写 A、再从别处 `memcpy` 一份源数据（`0x422b68`–`0x422b8c`），那份源数据的生产者没追 |
| `update` 的 `w5`（第 5 个入参） | **未知** | 只有"为真时 `scale.x` 取负"这一条（`0x422678`）；调用方是谁还没定位 |
| 每帧为什么刷 **2** 次 | **未知**（数字是**观测**） | 候选：一次整图 + 一次脏区；候选：两层障碍各一次。要抓 `update` 两次入参的运行时值 |
| 两条 `create_from_data` 路径的分工 | **未知** | `0x4226dc` 那个 `b.ne 0x422a58` 判的是 `self[0]`（`Gd<ImageTexture>` 的 cached rtti 标记）；"首次建 / 之后走脏区"只是**推断** |
| 脏矩形（只重画那一块） | **未知** | `src/obstacle_drawer.rs` 已记：本模块入参里没有矩形；"只重画一块"没有逐条读到 |
| `Sprite2D` 的 `texture_filter` / `centered` | **未知** | `update` 里只有 `set_position` / `set_scale` / `set_texture`（`CALLED_API`），没有第三条 |
| headless 下"贴图真的换了"的引擎侧证据 | **观测到的限制** | dummy 渲染器里 `RenderingServer::texture_2d_update` 不生效：同一像素建贴图时 alpha=255，`update` 之后从 `texture.get_image()` 读回来**还是** 255，而那张 `Image` 已经是 0（脚本里有一行 INFO 记这个）。⇒ 验收改读"喂给 `update` 的那张 `Image`" + 计数器；要看渲染结果得上有 GPU 的机器 |

### 45.7 复现（四分钟）

```text
export CARGO_HOME=<工作区>/.reference/cargo-home CARGO_TARGET_DIR=<工作区>/.agent-work/cg/target23
export GODOT=<工作区>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64

cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so     # 不覆盖就是旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                     # 判据：3
"$GODOT" --headless --path client --import                                # 不 import 扩展静默不加载
"$GODOT" --headless --path client --script res://tools/obstacle_drawer_check.gd
   → OBSTACLE_DRAWER 116 passed, 0 failed（退出码 0）

cargo test --offline                             # 1559 passed / 0 failed（改了 `src/` 必须全绿）
cargo test --offline --features gdext-bindings   # 1642 passed / 0 failed（本类 4 条纯函数单测在这条轴里）
python3 tools/coverage_matrix.py --check         # OK（重跑过：src_files 106 → 107、src_fns 2420 → 2453）
python3 tools/check_doc_refs.py                  # 通过（退出码 0）
```

**"真实代码行数"的算法**（与任务书的模板对照同一个口径）：去掉空行、去掉以 `//` 开头的整行注释、
去掉 `#[cfg(test)] mod tests { … }` 整块，剩下**有语句的行**：

```text
src/_scaffold/obstacle_drawer_class.rs   683 行 → 真实代码 **257** 行（其中单测块 58 行另计）
src/_scaffold/entity_drawer_class.rs     678 行 → 真实代码 **292** 行（模板对照，同一口径）
src/_scaffold/function_drawer_class.rs   538 行 → 真实代码 **216** 行（同一口径）

# 口径（可复跑）：非空行、去掉整行注释（`//`/`///`/`//!`）、去掉 `#[cfg(test)] mod tests {..}` 整块
python3 - <<'EOF'
src = open('<文件>').read().split('\n'); n = 0; t = False
for line in src:
    st = line.strip()
    if st.startswith('#[cfg(test)]'): t = True
    if t or not st or st.startswith('//'): continue
    n += 1
print(n)
EOF
```

⚠️ 任务书说模板 `entity_drawer_class.rs` "677 行，真实代码约 155 行"：那个 155 的口径比上面更严
（只数函数体里的语句，不数 `struct` 字段 / 常量 / 签名），**本轮复现不出来**。为了让"本类 vs 模板"
可比，三个文件都用上面这条命令的同一口径 ⇒ **257 : 292 : 216**。

**selftest**（`bash client/run.sh selftest`）：**694 passed, 0 failed**（前置：`client/assets/` 已解出
+ `cargo build --example serve_match` + `export CARGO_TARGET_DIR` + 两个回放样本
`1793897298` / `1783255495` 放进 Godot 的 `app_userdata/Graphwar/replays`）。缺那两个样本时是
**633 / 0 / 2**、缺音频资产时还要再少 10 条 —— 这两个数**不是**本次改动造成的（本改动的 116 条
在 `obstacle_drawer_check.gd` 里单独跑）。

**既有 headless 脚本回归**（本节提交前逐个跑过，全绿）：`gdext_check` 24 / `base_classes_check` 117 /
`entity_drawer_check` 72 / `function_drawer_check` 132 / `draw_classes_check` 235 /
`curve_on_screen_check` 96 / `plane_on_screen_check` 76 / `signal_classes_check` 155 /
`assemble_check` 199 / `spike_class_check` 23 / `recovered_view_check` 57，**`0 failed` ×11**。

## 46. 两个"字段一个都没解"的管理器解开了：`GraphwarFunctionManager`（32 B / 4 字段）与 `GraphwarObstacleManager`（80 B / 5 字段）（2026-11，任务 22）

**一句话**：§44.0 那两行「`GraphwarFunctionManager` / `GraphwarObstacleManager` 的字段**一个都没解**」
本节**作废**——两个结构体的每个字段都落到了指令上，而且**三份构建（arm64 2.0.60 / x86 Linux
2.0.60 / Windows 2.0.60 DLL）的偏移序列逐条相同**；布局写进
`src/graphwar_function_manager.rs` / `src/graphwar_obstacle_manager.rs`，
`size_of` 分别由 **编译期硬闸** 与单测断言为 **32 / 80**。

> 档位口径同 §42 / §44：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / `assemble_check.gd` 的条数**只是回归保护**，不是复原证据。

### 46.0 先说"没做到"的，以及两条就地更正

| 项 | 现在 | 档位 |
|---|---|---|
| 两个结构体的**字段布局** | **全部解开**（4 字段 / 5 字段，见 §46.2 / §46.3） | **观测**（三构建交叉验证） |
| 它们的**行为** | 仍未复原：`FunctionState` 的字段语义、到期 tick 的加法、`draw_circle` 写什么值 | **未知** / **推断**（见 §46.8） |
| 容器槽里的**档位字符串** | **仍然是 `"stub"`**，`module_impl_type` 仍然是 `"—"` → ⚠️ **§47 已改**：`FunctionManager` = `layout`、`ObstacleManager` = `wired`，`module_impl_type` 报 `graphwar_*_manager::Graphwar*Manager` | 我方口径（§47.2） |

**就地更正 1（`AGENTS.md` §0.6）**：§44.0 / §44.7 写「两个管理器的字段**一个都没解**」。**已解**，
本节起那两行过期；`src/_scaffold/root_wiring.rs` 的两个槽已从"我方形状"换成"**原版布局 + 我方行为表**"。

**就地更正 2（更重要，是一条**错的负结论**）**：§44.2 / §44.3 写
「`function_manager` 的四个 `set_*` 体内**零条** `GraphwarEventQueue::add_event`（`0x3d3530`）
⇒ 它**不发事件**」。**这条是错的**，而且是 `AGENTS.md`「间接调用与函数归属」第 1 条那个坑的复发：
`add_event` 在 x86 上是**经 GOT 槽 `0x9bac80` 的间接调用**，只扫 `call rel32` 直调会漏。

```text
x86 0x455e30 set_function             0x45638f  call qword [rip+..] slot 0x9bac80 -> 0x3d3530
x86 0x456810 set_function_points      0x4569e1  同上
x86 0x456b80 set_function_active_tick 0x456ce1  同上
arm64 0x488a9c set_function           0x488e30  bl 0x3d7a40   ; .symtab 名 = GraphwarEventQueue::add_event
```

`set_function` 发的是 **tag `0x23` = 35 = `FunctionInfo`**（arm64 `0x488e18`
`mov x8, #0x23; movk x8, #0x8000, lsl #48`，载荷 = `(id, 文本, tick)`）——这条与
**本仓自己的 `docs/02-module-map.md:524`**（「`0x455e30` … 再发事件 tag 35」）一致，
也就是说 §44 那条结论当时就和 docs/02 冲突。**结论改为**：`function_manager` **不订阅**（这条仍成立，
有正对照），但它**会发** `FunctionInfo` / `FunctionActive` 这类事件，共 3 个 `set_*` 各一条。

### 46.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_function_manager.rs` | **新增** | 模块文档（方法表 / 32 B 布局表 / 内层 48 B / 更正）+ `GraphwarFunctionManager` / `GraphwarFunctionData` / `FunctionState` + **3 条 `const` 硬闸** + 3 条单测 |
| `src/graphwar_obstacle_manager.rs` | **新增** | 同上（80 B / 5 字段 / 内层 24 B 网格 + 索引式）+ 硬闸 + 3 条单测 |
| `src/_scaffold/root_wiring.rs` | 改 | ①`#[path]` 把上面两个原版模块文件挂进 crate（`lib.rs` 在禁区，见 §46.6）；②两个槽 `XxxSlot(表)` → `XxxSlot { layout, table }`；③20 处 `.0` → `.table`（探针 18 处 + 单测 2 处）；④§44 那条错结论就地更正 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑 | `src_files` 106→**108**、`src_fns` 2420→**2432**、`src_tree_sha256` 跟着变；**527/527 与每模块行一行没变** |
| `docs/16-client.md` | +本节 | — |

❌ `src/lib.rs` / 任何 `*_class.rs` / `client/**` / `src/game.rs` / `src/plane.rs` / 既有测试断言
**一行没动**；`tools/`（含 `coverage_matrix.py` 的模块归属表）**没动**（见 §46.8 最后一行）。

### 46.2 `GraphwarFunctionManager` = **32 字节 / 4 字段**（`观测`）

| 偏移 | 宽 | 类型 | 证据（指令地址；三构建见 §46.5） | 档位 |
|---|---|---|---|---|
| `+0x00` | 8 | `Weak<RefCell<GraphwarFunctionManager>>` | 构造点把 **`RcBox` 指针本身**写进该槽：arm64 `0x419d44` `stp xzr, x19, [x19, #0x10]`（第 2 字 = `RcBox+0x18` = `m+0x00`）、x86 `0x5237f3` `mov [rbx+0x18], rbx`、DLL `0x18013f476`；析构交给 `drop_in_place<Weak<RefCell<GraphwarFunctionManager>>>`（arm64 `0x419df0`、DLL `0x18013f4f9`） | **观测** |
| `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | `set_function` 读它 → `+0x10` 是 `RefCell` 借用位 → `+0x18` 当 `&mut GraphwarEventQueue` 传给 `add_event`（arm64 `0x488dc4`→`0x488e30`；x86 `0x4562e8`→`0x45638f`）；`set_function_active_tick` 同（arm64 `0x489708`、x86 `0x456ce1`）；构造点第二字；析构做 `Rc` 强计数 `-1` / `drop_slow` | **观测** |
| `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `set_function` 把 `self+0x10` 当 `&Weak` 传给 `GraphwarModuleContainer::get_module_wk`（arm64 `0x488ac0` `add x0,x0,#0x10` → `bl 0x4a34c4`，该函数第一句就是 `ldr x20,[x0]` + `cmn x20,#1` 的 `Weak` 哨兵判定；x86 `0x455e5a` → `call 0x46d040`）；析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419dcc`、DLL `0x18013f4e2`） | **观测** |
| `+0x18` | 8 | `Arc<Mutex<GraphwarFunctionData>>` | **7 个方法的第一句**都是读这个槽（arm64 `0x488814`/`0x488c48`/`0x489068`/`0x489298`/`0x489608`/`0x48986c`/`0x489a0c`；x86 `0x455b74`/`0x456604`/`0x45682b`/`0x456b9e`/`0x456e03`/`0x456f93`）；`ArcInner` = `{strong@0, weak@8, Mutex@0x10}`，锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（arm64 `ldaxr w9,[x+0x10]` / `ldrb w8,[x+0x14]`；x86 `lock cmpxchg dword [x+0x10]`）；析构 `Arc::drop_slow` | **观测** |

**尺寸的两条独立证据**（都不依赖 §41 的 vtable 表）：

1. `RcBox<RefCell<GraphwarFunctionManager>>` 的分配常量是 **`0x38` = 56 = 24 + 32**
   （arm64 `0x419ca4`、x86 `0x523704`、DLL `0x18013f381`；DLL 错误路径
   `0x18013f4a2` 的 `handle_alloc_error(size=0x38)` 对得上）；
2. §41.2 的 `dyn Any` vtable 第 1 字 = `size_of::<RefCell<M>>()` = **40 = 32 + 8**（两架构逐字相同）。

**构造在哪儿**（`观测`）：原版**没有** `GraphwarFunctionManager::new` 的独立符号——构造整个被内联进
`Rc::…::new_cyclic_in`：arm64 `0x419c80`（416 B，由 `GraphwarGameClient::new` 在 `0x40a158` 调用）、
x86 `0x5236e0`（208 B，由 `0x517b9f` 调用、紧接 `add_module::<GraphwarFunctionManager>` `0x469db0`）、
DLL `0x18013f360`（DLL 里 `new` `0x1800d9ab0` 只是个 16 B 的 `jmp` thunk）。
三份构建的字段写入顺序完全一致 ⇒
`new(event_queue: Rc<RefCell<GraphwarEventQueue>>, container: Weak<RefCell<GraphwarModuleContainer>>)`，
弱自句柄由 `Rc::new_cyclic` 的 `&Weak` 直接给出。
（§41.2 表里「arm64 `new` = `0x40a158`」其实是**调用点**，构造体是 `0x419c80`——本节一并更正。）

### 46.3 `GraphwarObstacleManager` = **80 字节 / 5 字段**（`观测`）

| 偏移 | 宽 | 类型 | 证据（指令地址；三构建见 §46.5） | 档位 |
|---|---|---|---|---|
| `+0x00` | 8 | `Weak<RefCell<GraphwarObstacleManager>>` | `new` 里做 `Weak::clone`：`cmn x21,#-1`（`Weak::new` 哨兵）→ `ldr x8,[x21,#8]`/`adds`/`str`（弱计数 +1）→ 塞进 `tick_event` 的订阅闭包：arm64 `0x42633c`–`0x42635c`、x86 `0x3f2399`–`0x3f23a3`（`cmp r15,-1` / `inc [r15+8]`）、DLL `0x1801047f8`–`0x180104806`。构造点写 `RcBox` 指针：arm64 `0x419b70`、DLL `0x18013f634`；析构 `drop_in_place<Weak<RefCell<GraphwarObstacleManager>>>`（`0x416ff8` / DLL `0x18013c480`） | **观测** |
| `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | 4 个入口都读它、`+0x10` 是 `RefCell` 借用位、`+0x18` 传给 `add_event`：arm64 `0x42572c`（add）/`0x4258d4`（clear）/`0x425adc`（destroy）/`0x42620c`（queue_destroy）；x86 `0x3f184b`/`0x3f1a05`/`0x3f1bde`/`0x3f2245`；DLL `0x180103b6f` 等。构造点第二实参 | **观测** |
| `+0x10` | 8 | `Weak<RefCell<GraphwarModuleContainer>>` | `new` 的第三个实参（arm64 `x21`→`RcBox+0x28`；x86 `r15`→`[rbx+0x28]`；DLL `%rbx`）；析构 `drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>`（arm64 `0x419c2c`、DLL `0x18013f6e7`） | **观测** |
| `+0x18` | 8 | `Arc<Mutex<GraphwarObstacleData>>` | 每个方法第一句都读它（arm64 `0x425488`/`0x4256a8`/`0x425888`/`0x425a58`；x86 `0x3f15bc`/`0x3f17b4`/`0x3f19b1`/`0x3f1b44`）；锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（数据里 `ptr@+0x20`、`len@+0x28`）；`drop_in_place<ArcInner<Mutex<GraphwarObstacleData>>>` 存在（DLL `0x18013c810`） | **观测** |
| `+0x20` | 48 | `HashMap<u32, Vec<[f64; 3]>>`（待销毁队列） | `RawTable`：`ctrl@+0x20`、`bucket_mask@+0x28`、`growth_left@+0x30`、`items@+0x38` + `RandomState@+0x40`（16 B）。决定性指令：arm64 `0x425c50` `add x0,x0,#0x40` → `BuildHasher::hash_one`(`0x3ec7f0`)、`0x425c68` `add x0,x19,#0x20` → `RawTable::remove_entry`(`0x3c6620`)；x86 `0x3f2110`/`0x3f2128`；DLL `0x18010455f`/`0x180104577`。键 4 字节、值 `Vec<[f64;3]>`（插入走 `RawVec::grow_one`、元素 24 B；析构 `dealloc(ptr, 24*cap)`） | **观测** |

尺寸的独立证据：`RcBox<RefCell<GraphwarObstacleManager>>` 的分配常量 **`0x68` = 104 = 24 + 80**
（arm64 `0x419aa4`、DLL `0x18013f562` + 错误路径 `size=0x68`）；且 `+0x20 + 48 = 0x50 = 80` 自洽。

`new` 的签名（`观测`）：`new(&mut GraphwarEventHandler, Rc<RefCell<GraphwarEventQueue>>,
Weak<RefCell<GraphwarModuleContainer>>) -> Rc<RefCell<Self>>`——第一个实参用来
`subscribe_to_event("tick_event", len 0xa, 闭包)`（arm64 `0x4263a0`、x86 `0x3f23f0` 经 GOT `0x9bac88`）：
这是这个管理器**唯一**一条订阅（与 §44.2 一致）。

### 46.4 两个内层数据类型也被顺带钉死了

| 类型 | 大小 | 是什么 | 证据 | 档位 |
|---|---|---|---|---|
| `GraphwarFunctionData` | **48 B** | 一张 `HashMap<u32, FunctionState>`（没有别的字段） | `ArcInner<Mutex<…>>` 分配 `0x48` = 72 = 16+8+48（arm64 `0x419d2c`、x86 `0x5237aa`、DLL `0x18013f42e`）；4 组字段偏移 = `ctrl@0`/`bucket_mask@8`/`growth_left@0x10`/`items@0x18` + `RandomState@0x20`；drop glue 名 `hashbrown::raw::RawTable<(u32, FunctionState)>` | **观测** |
| `FunctionState` | **112 B**（`推断`） | 上表的**值**；字段语义未知 | 由 bucket 步长 `0x78`（§42.2 已测）反推：`size_of::<(u32, FunctionState)>() = align_up(8+S, 8) = 120` ⇒ `S = 112`（`S` 含 `f64`/`Vec` ⇒ `align = 8` ⇒ 8 的倍数） | **推断** |
| `GraphwarObstacleData` | **24 B** | 一张 **1000×600 = 600000 字节**的网格（`0xff` = 空） | `ArcInner<Mutex<…>>` 分配 `0x30` = 48 = 16+8+24；`new` 里 `__rust_alloc(0x927c0, 1)` + `memset(0xff, 600000)`，然后写 `cap=0x927c0`/`ptr`/`len=0x927c0`（DLL `0x180103895`–`0x1801038a0`）；索引式见下 | **观测** |

网格索引式（`观测`，arm64 `is_obstacle` `0x4254c4`–`0x425534`，常量全是双精度）：

```text
x = trunc(1000.0 * p.x / 50.0 + 500.0)    合法区间 [0, 999]   (cmp #0x3e7)
y = trunc(1000.0 * p.y / 50.0 + 300.0)    合法区间 [0, 599]   (cmp #0x257)
index = y * 1000 + x                      (mul w9, w9, #1000)
有障碍 ⇔ grid[index] != 0xff              (x86 `cmp byte ptr [rax+rcx], 0xff`)
```

⇒ 600000 = 1000 × 600 与 24 字节的 `Vec<u8>` 形状（本工具链内存序 `cap@0/ptr@8/len@0x10`，
`cap == len == 600000`）完全对得上。

### 46.5 三向交叉验证：偏移序列逐条相同（这是本节最硬的一层）

| 偏移 | arm64 2.0.60 | x86 Linux 2.0.60 | Windows 2.0.60 DLL |
|---|---|---|---|
| `FunctionManager +0x00` 存 `RcBox` 指针 | `0x419d44` | `0x5237f3` | `0x18013f476` |
| `+0x08` 事件队列 | `0x488dc4`（用）/ `0x419d58`（写） | `0x4562e8` / `0x5237f7` | `0x1800d997b` / `0x18013f47a` |
| `+0x10` 容器 `Weak` | `0x488ac0`（用）/ `0x419d58`（写） | `0x455e5a` / `0x5237fb` | `0x18013f47e` |
| `+0x18` `Arc<Mutex<…>>` | `0x488814` …（7 处） | `0x455b74` …（6 处） | `0x1800d8940` …（5 处） |
| `RcBox` 大小 56 | `0x419ca4` | `0x523704` | `0x18013f381`（+错误路径 `0x18013f4a2`） |
| `ObstacleManager +0x00` 弱自句柄 | `0x42633c`–`0x42635c` | `0x3f2399`–`0x3f23a3` | `0x1801047f8`–`0x180104806` |
| `+0x08` 事件队列 | `0x42572c` 等 4 处 | `0x3f184b` 等 4 处 | `0x180103b6f` 等 4 处 |
| `+0x18` `Arc<Mutex<…>>` | `0x425488` 等 4 处 | `0x3f15bc` 等 4 处 | `0x1801038d9` 等 4 处 |
| `+0x20` 哈希表 `ctrl` / `+0x40` hasher | `0x425c68` / `0x425c50` | `0x3f2128` / `0x3f2110` | `0x180104577` / `0x18010455f` |
| `RcBox` 大小 104 | `0x419aa4` | （内联，`0x3f2372` 调 `0x523020`） | `0x18013f562` |

**"不一致"的唯一一处要解释**：x86 Linux 侧两个 `new`（`0x5236e0` / `0x3f2360`→`0x523020`）都是
**内联后的独立函数**（没有符号名），而 DLL 那份还留着 `GraphwarFunctionManager::new` /
`GraphwarObstacleManager::new` 的符号——**函数边界不同，偏移序列相同**，所以不影响结论；
反过来，这也正是"只按函数地址对齐会配错函数"的例子（`AGENTS.md` §0 第 5 条）。

复算命令（三条，互不依赖；`$PY` 有 capstone）：

```text
# ① arm64 未 strip：类型名 + 大小 + 方法名
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so | grep -E "graphwar_(function|obstacle)_manager"
# ② x86 Linux（同名落点来自 fixtures/name_to_addr.tsv）
grep -E "graphwar_(function|obstacle)_manager" fixtures/name_to_addr.tsv
$PY tools/addr_probe.py dis 0x455e30      # set_function：add rdi,0x10 = 容器 Weak
$PY tools/addr_probe.py dis 0x3f2360      # ObstacleManager::new：cmp r15,-1 / inc [r15+8] = Weak
# ③ Windows DLL（同版本、同 ISA、COFF 符号表在；VA = 0x180000000 + 0x1000 + 符号值）
objdump -t .reference/windows/graphwarrust-2.0.60.dll | grep -E "Graphwar(Function|Obstacle)Manager"
```

### 46.6 落码：布局进原版模块文件、`size_of` 有硬闸

1. **两个原版模块文件**新增：`src/graphwar_function_manager.rs`（4 字段 + 3 条 `const` 硬闸 +
   3 条单测）与 `src/graphwar_obstacle_manager.rs`（5 字段 + 2 条硬闸 + 3 条单测）。
   硬闸是**编译期**的，改错一个字段类型就编不过：

   ```rust
   const _: () = assert!(std::mem::size_of::<GraphwarFunctionManager>() == 32);
   const _: () = assert!(std::mem::size_of::<GraphwarFunctionData>() == 48);
   const _: () = assert!(std::mem::size_of::<GraphwarObstacleManager>() == 80);
   const _: () = assert!(std::mem::size_of::<GraphwarObstacleData>() == 24);
   ```

   单测再逐字段断言 `offset_of!`（`0x00/0x08/0x10/0x18` 与 `0x00/0x08/0x10/0x18/0x20`）。
2. **`src/lib.rs` 在禁区**（任务书 §5 的 ❌）⇒ 这两个文件暂时由
   `src/_scaffold/root_wiring.rs` 用 `#[path = "../graphwar_function_manager.rs"] pub mod …`
   挂进 crate。这是**临时通道**，`lib.rs` 一放开就换成 crate 根的 `pub mod`，两个文件一行都不用改。
3. **容器槽**：`FunctionManagerSlot(FunctionTable)` → `FunctionManagerSlot { layout, table }`，
   `ObstacleManagerSlot(ObstacleTable)` → `ObstacleManagerSlot { layout, table }`；
   `layout` 是原版布局，`table` 是我方行为表（探针/验收仍读它）。
   槽的档位字符串**仍报 `"stub"`**、`module_impl_type` **仍报 `"—"`**——因为那两处进的是
   `client/tools/assemble_check.gd` 的既有断言（5 条 `stub` + 1 条 `container_stub_manager_count == 5`），
   而 `client/**` 本轮在禁区。⇒ **读法**：这两个槽的 `stub` 现在只表示"**行为**未复原"，
   "结构未知"那层已经消掉；要改字符串必须与 `assemble_check.gd` 一起改（**留给协调者**）。

   ✅ **§47 已完成这件事**（协调者就是任务 28）：`root_wiring.rs` 与 `assemble_check.gd` 同批改，
   档位词表换成四档，逐槽断言从 "assembled/stub" 改成 "wired/layout/assembled/stub"。
   `container_stub_manager_count` 仍是 5，因为它数的是 `SlotKind::Stub`（容器类型来源），
   **不是**档位 `stub`（现在 1 个）——两件不同的事，别混。

### 46.7 验收（本节实测，可从干净树重跑）

| 命令 | 本节实测 | 基线（`CG@c99c8b5`，同一条命令/同一构建目录） |
|---|---|---|
| `cargo test --offline` | **1559**，全绿 | **1559**（一位不差；新模块挂在 `gdext-bindings` 下，默认轴不动） |
| `cargo test --offline --features gdext-bindings` | **1644**，全绿 | **1638**（本节 +6：两个模块各 3 条） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_files` 106→108、`src_fns` 2420→2432） | OK |

**三条护栏（三条驱动轴分开跑、分开报；`AGENTS.md` 合并纪律 C）**：

| 轴 | 命令 | 本节实测 | 护栏（任务书 §3） |
|---|---|---|---|
| `fire` | `cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire --policy default --json .work/guard/f.jsonl` | 内容命中 **331 / 1080**（文件 286、读失败 117） | **331**（一位不差） |
| `batch` | `… --drive batch --policy default --json .work/guard/b.jsonl` | 内容命中 **355 / 1080** | **355**（一位不差） |
| `batch_conv` | `cargo run --release --offline -q --example batch_conv -- --dir <快照> --policy strict` | 内容命中 **633 / 1080**；种类 **1075 / 1080**（多发批 128/130 + 单发批 947/950） | **633 / 1075**（一位不差） |

⇒ 本节只加"布局结构体 + 槽的字段"，**一行几何/弹道代码都没碰**，三条轴逐数字不动。

### 46.8 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `FunctionState`（112 B）的**字段语义** | **未知** | 只有 §42.2 那三条谓词（`+0x50` 与 `i64::MIN` 比、`+0x70 == 1`、`+0x74` = `active_tick`(u32)、`+0x58/+0x60` = 点数组 ptr/len）；没有逐字段定名 |
| `FunctionState` 的**精确大小** | **推断**（112 B） | 缺"一次性 `memcpy`/`drop` 整结构"或 `size_of` 常量；只能由 bucket 步长 `0x78` 反推 |
| `GraphwarFunctionManager +0x00` 弱自句柄**谁在用** | **未知** | 7 个方法**没有一处**读 `m+0x00`，本模块又**不订阅**事件（构造体内无 `subscribe`）⇒ "谁替它订阅"未观测到；字段本身的存在有三构建证据 |
| `draw_circle` 往网格里**写的值** | **推断**（非 `0xff`） | 660 字节函数体里的 `strb` 常量没逐条读 |
| 网格坐标换算的**语义**（`1000/50`、`+500`、`+300`） | **观测**（常量与算式）/ **推断**（"棋盘 50 单位 → 1000 像素"） | 缺一份真帧量测（棋盘像素宽到底是不是 1000） |
| `Vec<[f64;3]>` 三个 f64 的**顺序** | **推断**（`x, y, r`） | `is_destruction_queued` 只做逐分量 `ucomisd`；`draw_circle` 的入参顺序未证 |
| `queue_destroy_obstacle` 的**到期 tick 算法** | **未知** | 键是 tick（`handle_tick_event` 用 tick 直接 `remove_entry`，**观测**），但 `now + ?` 那个加法没解（§44.7 照旧） |
| 两个槽的 `stub` / `—` 字符串 | **已改**（§47） | 档位改成四档（`layout` / `wired`）、`impl_type` 报真类型；只剩 `graphwar_root_node_class.rs` 的两处**注释**与 `container_stub_manager_count` 这个**函数名**还带旧口径（`*_class.rs` 是禁区，见 §47.6） |
| `tools/coverage_matrix.py` 的**模块归属表** | 未改（`tools/` 不在本轮允许清单） | 表里这两个模块仍写 `src/game.rs` / `src/plane.rs`；本轮新增的两个文件只影响 `src_files`/`src_fns` 总数，**每模块的 527 行一行没动**。下一轮把 `ours` 加上这两个文件即可 |

### 46.9 复现（干净树）

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<你的>/target
cargo test --offline                                  # 1559，全绿
cargo test --offline --features gdext-bindings        # 1644，全绿（含本节 6 条）
python3 tools/coverage_matrix.py --check              # OK
python3 tools/check_doc_refs.py                       # OK（本节的小节号连续）
```

**结构断言在哪儿**：`src/graphwar_function_manager.rs` / `src/graphwar_obstacle_manager.rs`
末尾的 `mod tests`（`layout_matches_the_original_32_bytes` / `layout_matches_the_original_80_bytes`
逐字段 `offset_of!`），以及两个文件顶部**编译期**的 `const _: () = assert!(…)`。

## 47. `stub` 语义一次性改对：槽位复原档位 = `wired` / `layout` / `assembled` / `stub`（2026-11，任务 28）

**一句话**：`stub` 一个词原来**同时**表示"我方只有占位"和"零行为"。任务 19 把 3 个桩接上了
事件流与交互断言、任务 22 又把其中 2 个的**原版字段布局**解开了（§46），但**字符串没跟**，
于是 §44 / §46 的报告一直在用错误的现状判断下一步（同一个坑踩了两轮）。本节把
**结构**（原版字段布局解没解）与**行为**（容器 / 事件流 / 最小交互断言）拆成两条轴，
定成**四个词**写进 `src/_scaffold/root_wiring.rs` 的模块文档，并**同批**改了
`client/tools/assemble_check.gd` 的逐槽断言（**199 → 203 条**）——上一轮就是因为
"断言在禁区"只改了注释，才留下这个坑。

> 档位口径同 §42 / §44 / §46：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编
> 地址 / 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。
> `cargo test` / `assemble_check.gd` 的条数**只是回归保护**，不是复原证据。

### 47.1 判据：四个词，代码与断言共用（原文在 `root_wiring.rs` 的模块文档）

| 档位 | 判据（**按序判断，第一个命中者为准**） | 现值 |
|---|---|---|
| `wired` | 已接容器 **+** 已接事件流 **+** 有最小交互断言（`RootWiring::interaction_selftest` 覆盖这一槽） | **4** → **10**（§50） |
| `layout` | 原版字段布局**已逐字段解出**（有证据档），但**不满足** `wired` | **2** |
| `assembled` | 容器里装的是**我方复原类型**（能按类型取回并调用），但**不满足**上面两条 | **15** → **9**（§50） |
| `stub` | 以上都不是：**我方形状占位**，原版字段布局**未知** | **1** |

```text
结构轴：stub（布局未知） ──→ layout（布局逐字段解出）
行为轴：assembled（装上我方复原类型） ──→ wired（+ 事件流 + 最小交互断言）
一个槽只报**它达到的更高那一档**；两条轴都到顶 = wired（任务 40 起：10 + 2 + 9 + 1 = 22）。
⇒ "结构已解但行为未做" = layout；"结构都没解" = stub —— 这就是本轮要根治的那个模糊。
```

三条读法（每条都有本轮的实际槽作反例，别合并省略）：

1. **`wired` 不要求布局逐字段解出**：`GraphwarTickHandler` 三件齐，但字段只落到有地址证据的
   4 个偏移（`+0x00..0x0f` 壁钟 / `+0x28` 1e9 ns / `+0x38` 事件队列 / `+0x40` `current_tick`），
   它就是 `wired`；布局那一轴单独记在 §47.2 与各 manager 文件的文档块里，**不**并进档位字符串。
2. **`layout` 不等于"零行为"**：`GraphwarFunctionManager` 布局全解（§46.2）、容器能取回、
   6 条交互断言都在跑，但**发方向的事件流没接**（§46.0 的就地更正 2 证明原版 3 个 `set_*`
   各发一条 `FunctionInfo` 类事件，我方 `FunctionTable` 一条都不发）⇒ 它停在 `layout`。
   真正"布局全解、行为 0 行"的是 `GraphwarClientConnectionHandler`（§47.2）。
3. **`assembled` 不等于"复原了"**：这 15 个只有"类型 + 部分行为 + 按类型可调"，原版布局多数
   **没逐字段解出**，事件流也没接全。

⚠️ **与本文件别处的 `SlotKind` 不是一回事**（这条最容易被误读）：`SlotKind` 是**容器类型来源**的
二分类——`RealType`（容器里直接装原版模块类型，**17** 个）/ `Stub`（容器类型是 `_scaffold` 的
包装或标记，**5** 个），`container_real_manager_count()` / `container_stub_manager_count()`
报的就是这两个数（§41.2 的既有口径）。档位回答的是"这一槽离原版还有多远"。
⇒ **`SlotKind::Stub` 的 5 个槽里，现在有 2 个档位是 `wired`、2 个是 `layout`、1 个是 `stub`**；
"5 个 stub" 与 "1 个 stub" 指的是两件事，引用时必须写清是哪一个。

### 47.2 5 个原 `stub` 槽的逐槽重判（任务书要求的那三条事实）

| 槽（原版尺寸） | 原版字段布局 | 事件流（订阅 / 发） | 最小交互断言 | 档位 | 差什么才能上一档 |
|---|---|---|---|---|---|
| `GraphwarTickHandler`（72 B） | **部分**：4 个偏移有指令证据，其余我方形状 | 订 **1**（`connected_to_server_event`）/ 发 **2**（`TickEvent`、`TickRequest`）——与原版一致（§44.2） | **4** 条 | `wired` | 布局：`get_fractional_tick` / `get_time_to_tick` 的算式（§44.7 仍列未知） |
| `GraphwarClientConnectionHandler`（112 B） | **逐字段全解**：`HandlerField` 9 条偏移 + `SIZE = 0x70 = 112`，与 §41.2 的 vtable size 对上（`src/graphwar_client_connection_handler.rs`，**观测**） | **未接**（订阅名 / 处理器 / `forward_event` 的 24 个请求事件名都有证据，代码里一条没接） | **0** 条 | `layout` | 行为：`connect` / `tick` / `disconnect` / `forward_event` 的状态机没落码（4 态只有纯函数 `tick_action` / `state_after_connect_attempt`） |
| `GraphwarPlanePosManager`（24 B） | **未知**：连 `src/graphwar_plane_pos_manager.rs` 都没有；已知常量（`px = 20x + 500`）以自由函数落在 `plane.rs` | **未接** | **0** 条 | `stub` | 结构：24 B 的字段布局一条都没解 |
| `GraphwarFunctionManager`（32 B） | **逐字段全解**：4 字段，三构建偏移序列逐条相同（§46.2） | 订 **0**（正面证据：原版也不订阅）/ 发 **0**，而原版 3 个 `set_*` 各 1 条 ⇒ **发方向未接** | **6** 条 | `layout` | 事件流：把 `FunctionInfo` 类事件写进 `FunctionTable::outbox`（tag `0x23` 已证，载荷 `(id, 文本, tick)` 只证了 tag） |
| `GraphwarObstacleManager`（80 B） | **逐字段全解**：5 字段，三构建偏移序列逐条相同（§46.3） | 订 **1**（`tick_event`）/ 发 **4**（`AddObstacle` / `DestroyObstacle` / `QueueDestroyObstacle` / `ClearObstacles`）——与原版一致 | **7** 条 | `wired` | 行为：`draw_circle` 写什么值、`queue_destroy_obstacle` 的到期加法（§46.8） |

两点补充：

* **`ClientConnectionHandler` 为什么算 `layout` 而不是 `stub`**：它的 112 字节布局是**逐字段**夹出来的
  （`new` 只读不写、`connect`/`tick`/`disconnect`/`forward_event` 各用固定偏移访问，交叉后没有空洞；
  `SocketAddr` 占 `+0x08..0x28` 32 字节、`JoinHandle` 3 个字 `+0x50/0x58/0x60`、`Latch` `+0x68`），
  总长 `0x70` 与 §41.2 的 vtable size 一个字不差 ⇒ **结构这一轴到底了，行为一行没写**。
  这正是 `layout` 这个档位存在的理由。
* **`FunctionManager` 的 "订 0 / 发 0" 不是"没查"**：订阅方向是**正面证据**（§44.3 那种正对照），
  发方向是 §46.0 就地更正过的**真结论**（经 GOT 槽 `0x9bac80` 的间接 `add_event`）。
  我方两条都没接（订阅本来就不该有），所以档位只能到 `layout`。

### 47.3 22 个槽的档位（现值）

> ⚠️ **任务 40 已改**（§50）：`wired` 4 → **10**、`assembled` 15 → **9**。下表是任务 28 当时的
> 快照，保留作对照；**现值请看 §50.8**。

| 档位 | 槽 | 依据 |
|---|---|---|
| `wired`（4 → **10**） | `TickHandler` / `TargetManager` / `LifeManager` / `ObstacleManager`（§44.2）+ 任务 40 的 6 个（§50.2） | §44.2 / §50.2 的订阅/发事件表 + `interaction_selftest` 的 4 / 5 / 5 / 7 条与 6 / 6 / 7 / 5 / 7 / 7 条 |
| `layout`（2） | `ClientConnectionHandler`（§47.2）/ `FunctionManager`（§46.2） | 布局逐字段解出，行为没做完 |
| `assembled`（15 → **9**） | 任务 40 之后剩 `LocalPlayerManager` / `SoldierManager` / `GameStateManager` / `GraphicsManager` / `ComputerPlayerManager` / `FunctionHistoryRecorder` / `NameManager` / `ColorManager` / `RankManager` | 容器里是我方复原类型，但没接事件流 / 没有最小交互断言（`GameStateManager` 的理由见 §50.4） |
| `stub`（1） | `PlanePosManager` | 布局未知 |

`SlotKind`（容器类型来源）仍是 **17 + 5 = 22**，一行没变；两个轴的对应关系见 §47.1 末尾那段。

### 47.4 落码：改了哪几处（**代码与断言同批**）

| 文件 | 改了什么 |
|---|---|
| `src/_scaffold/root_wiring.rs` | ①模块文档新增"槽位复原档位"一节（判据 + 三条读法 + 与 `SlotKind` 的边界）；②22 行台账加**档位**列（原"档位"列改名"证据"，避免一个词两个意思）；③新增 `SLOT_WIRED` / `SLOT_LAYOUT` / `SLOT_TIERS` / 四档计数常量 + **编译期闸**（合计必须 22）；④`Wired::status()` 改成四档判据；⑤`SlotKind` 的文档重写成"容器类型来源"，并标明**不是档位**；⑥3 个包装槽的 `our_type` 从 `—` 改成里面真正的类型（`graphwar_tick_handler::GraphwarTickHandler` 等）；⑦`FunctionTable` 的发件箱注释更正（原版**会**发、我方**没接**，档位因此 `layout`）；⑧§3 换成逐槽事实表；⑨单测：改 1 条（`unwired_module_types_are_placeholders`）+ 新 1 条（`slot_tiers_are_the_four_word_vocabulary`，逐槽钉死四档） |
| `client/tools/assemble_check.gd` | `STUBBED`（5 个名字）→ `WIRED_TIER`（22 个档位，与注册序一一对应）+ `TIER_TALLY` + `EMPTY_IMPL` + `PACKAGED` / `PACKAGED_TIER`；逐槽断言改查**四档**；`impl_type` 改断"只有两个纯标记是 `—`"；新增四档计数 / 合计 / 词表越界三条；5 个包装槽的反面段改成新档位 + "只剩 1 个真 stub" 一条 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑（`src/` 变了）：`src_fns` **2480 → 2481**（+1 = 新加的那条单测）、`src_tree_sha256` 跟着变；**527/527 行与每模块数字一行未变** |
| `docs/16-client.md` | 本节 + §41.4 / §41.6 / §38 那几处过期措辞的就地标注（§47.6） |
| `docs/02-module-map.md` | §17.3 加一段：`root_wiring.rs` 不在那张表里，档位判据落在哪、`SlotKind` 与档位的区别 |

❌ **没动**：`src/lib.rs`、任何 `*_class.rs`、manager 的纯逻辑、`client/src/**`、`dispatch/`、
`client/tools/` 下别的脚本、以及 `tools/bit_exact_attrib.py` 的基线（本轮它本来就是红的，见任务书 §3）。

### 47.5 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<ws>/.reference/cargo-home CARGO_TARGET_DIR=<ws>/.agent-work/<你的>/target28
export GODOT=<ws>/.reference/godot/Godot_v4.7.2-stable_linux.x86_64
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
grep -a -c line_instance_id client/libgw2_rebuild.so          # 必须是 3（判据：.so 真的是新的）
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd   # 203 passed, 0 failed
```

| 命令 | 本节实测 | 改前（`CG@0ea7729`） |
|---|---|---|
| `assemble_check.gd` | **203 passed / 0 failed**，退出码 0 | **199 passed / 0 failed** |
| `obstacle_drawer_check.gd` | **116** passed | 116 |
| `plane_on_screen_check.gd` | **76** passed | 76 |
| `function_drawer_check.gd` | **132** passed | 132 |
| `draw_classes_check.gd` | **235** passed | 235 |
| `curve_on_screen_check.gd` | **96** passed | 96 |
| `cargo test --offline` | **1569**，全绿 | **1569**（一位不差） |
| `cargo test --offline --features gdext-bindings` | **1653**，全绿 | **1652**（+1 = 新单测） |
| `python3 tools/coverage_matrix.py --check` | **OK**（527 / 527 一行未变；`src_fns` 2480 → 2481） | OK |
| `python3 tools/check_doc_refs.py` | **OK** | OK |

**新增 4 条的构成**：四档计数 1 + 四档合计 1 + 词表越界 1 + "只剩 1 个真 stub" 1。
既有 199 条**一条没删**：22 条逐槽状态断言换了**词**（`assembled`/`stub` → 四档），
22 条 `impl_type` 断言换了**判据**（原按档位、现在按"容器里有没有东西"），
其余 155 条数字与判据**一字未动**。

⚠️ 护栏三条轴（`fire` / `batch` / `batch_conv`）与延迟段**本节没跑也不该动**：
本轮只改注释、档位字符串、断言脚本与生成物，**一行几何/弹道/状态机代码都没碰**
（`cargo test` 1569 一位不差可以佐证默认轴没动）；三条轴的数字以协调者合并后的四件套为准。

### 47.6 就地更正清单（哪几处旧说法从本节起作废）

| 位置 | 旧说法 | 现在 |
|---|---|---|
| §41.4 | 「**5 个空占位**（`SlotKind::Stub`，`module_slot_status()` 如实回 `"stub"`，**不是** `"assembled"`）」 | 档位是 **2 `wired` + 2 `layout` + 1 `stub`**（§47.2） |
| §41.4 | 「`graphwar_tick_handler.rs` 只有 `tick_at` 一个自由函数」「`plane_pos` / `function` / `obstacle` **连文件都没有**」 | 任务 19 起 `graphwar_tick_handler.rs` 有 `GraphwarTickHandler`；§46 起后两个**有文件、有原版布局**；只有 `plane_pos_manager` 仍无文件 |
| §41.6 | 「5 个空占位的**类型本身** 未知 ⇒ 这 5 个型在本仓库没有可构造的类型」 | 3 个已可构造（两个是原版布局 + 我方表，一个是原版模块里的 `GraphwarTickHandler`），1 个有**全偏移表**（§47.2），1 个仍未知 |
| §38（`module_slot_status` 那段） | 「槽位**三态**：`assembled`（3 个）/ `placeholder` / `unknown`」 | §41 起是 22 个槽；§47 起状态是**四档** |
| §44.0 表 | 「这 3 个槽的档位 **仍然是 `stub` / `impl_type = "—"`**」 | `wired` / `wired` / `layout`，`impl_type` 报真类型路径 |
| §44.1 表 | 「+16 条：… 反面（桩仍报 stub）」 | 反面段现在断的是**四档**（5 个包装槽里 4 个不再是 `stub`） |
| §44.6 | 「5 条（5 个槽仍报 `stub`，反面）」 | **6 条**（5 条档位 + 1 条"只剩 1 个真 stub"） |
| §46.0 表 | 「容器槽里的档位字符串 **仍然是 `"stub"`**、`module_impl_type` 仍然是 `"—"`」 | `FunctionManager` = `layout`、`impl_type` = `graphwar_function_manager::GraphwarFunctionManager`；`ObstacleManager` = `wired` |
| §46.6 第 3 条 | 「槽的档位字符串**仍报 `"stub"`** … 要改字符串必须与 `assemble_check.gd` 一起改（**留给协调者**）」 | **本轮做完**（同批改了 `root_wiring.rs` + `assemble_check.gd`） |
| §46.8 表末第二行 | 「两个槽的 `stub` / `—` 字符串：**过期但未改**」 | 已改（本节） |

⚠️ **本轮仍没改的（禁区）**：`src/_scaffold/graphwar_root_node_class.rs` 里两处注释还写着旧口径——
`module_slot_status()` 的「`stub`（容器里查得到，但装的是**零行为的空占位**）」与
`container_stub_manager_count()` 的「空占位」。**函数名也别按档位读**：
`container_stub_manager_count()` 数的仍是 `SlotKind::Stub` = **5**（容器类型是 `_scaffold` 包装），
**不是**档位 `stub` 的 **1**。`*_class.rs` 是 `AGENTS.md` 合并纪律 A-1 的禁区，本轮没碰，
下一轮放开时把那两处注释与两个函数名一起改（`assemble_check.gd` 那边已经把文案改成
"SlotKind::Stub，不是档位 stub"，可以在那儿先读）。

### 47.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `FunctionManager` 的**发方向**事件流 | **未知**（原版**有**，我方**没接**） | 原版 3 个 `set_*` 各发一条（§46.0）；tag `0x23` 已证，`FunctionActive` 那两条与载荷另两段没逐字节核 |
| `ClientConnectionHandler` 的**行为** | **未知** | 4 态状态机只有纯函数；`connect_timeout` / 连接线程 / `forward_event` 的线程与 socket 语义没落码 |
| `PlanePosManager` 的**字段布局** | **未知** | 24 字节结构体的偏移一条都没有；`px = 20x + 500` 只在 `plane.rs` 的自由函数里 |
| 15 个 `assembled` 的**原版布局** | 多数**未知** | 本节只逐槽核了 5 个原桩 + §46 的两个；其余按 §41.6 的既有结论（我方类型尺寸与原版不等）判"没逐字段解出" |
| 档位本身的**判据强度** | **我方口径** | 档位是**静态台账**，只说"做到哪一档"，**不说**行为对不对；行为正确性仍只能由真二进制决定（`AGENTS.md` §0 第 2/5 条） |
## 48. 播放屏那 3 个"可解"缺口的证据补齐：tick 算式 / `update_transform` 三实参 / 两张颜色表（2026-11，任务 32）

> 本节对应 `dispatch/32-play-screen-evidence-gaps.md`（分支 `agent/play-screen-gaps`）。
> §43.6(d) 的 7 个缺口里**3 个是"证据缺"**：本节把它们补上，并**就地改正**了两条旧推断
> （§42.3 那句"两张表来自 `get_team_overview`"、§42.5 第 4 条那句"`Vector2(w/50,0)` 自相矛盾"）。
> 剩下 4 个（点生产者归属 / 淡出的 GOT 槽 / `z_index` 初值 / `FunctionDrawer::draw`）**没动**。

### 48.0 一页结论

| # | 缺口 | 结论 | 算式 / 内容 | 落地位置 | 档位 |
|---|---|---|---|---|---|
| 1 | tick 源 | **解出** | `fractional = tick + ((now − base) − accum) / 0.016666666`，`0.016666666` = `.rodata 0x19f7f8` = `f64(16_666_666)/1e9`（**不是** `1/60`） | `graphwar_tick_handler::GraphwarTickHandler::get_fractional_tick`（`0x519530`）；`GraphwarPlane` 自己持有并每帧读 | **观测**（逐指令）+ **逐位**（除数位型） |
| 1b | 连带的 `get_time_to_tick` | **解出**（顺带） | `(accum + 1 tick) − (now − base)` | 同文件 `get_time_to_tick`（`0x519450`） | **观测** |
| 2 | `update_transform` 三实参 | **解出** | `x = w/2`、`y = h/2`、`scale = s = w/50`（f32 标量）+ `flip` 位；被调者自己拼成 `flip ? (−s,−s) : (s,−s)` | `GraphwarPlane::apply_update_transform`（调用点 `0x4b6147`） | **观测**（逐指令） |
| 3 | `get_team_overview` 返回什么 | **解出，且旧说法被否** | **`u32` 的 4 个布尔位**（bit 0/8/16/24），只用于 `flip`；**不是**两张表 | `GraphwarPlane::flip_from_overview_bits` | **观测** |
| 3b | 两张表（队色） | **解出** | 两个 6 色池（队号 0/1），12 个 `(r,g,b)`，见 §48.3 表 | 表本来就在 `src/graphwar_color_manager.rs`（`TEAM1_COLORS`/`TEAM2_COLORS`）；本节补 **x86 观测**与 `team_color_pool()` 出口 | **逐位**（12 条 `mov word`+`mov byte`） |
| 3c | 棋盘内部底色 | **解出归属** | **不是平面画的**：`AxisDrawer::draw` 的 `draw_rect(filled=true, Color(1,1,1,1), rect(−25,−15,50,30))` | `GraphwarPlane.board_fill`（导出属性，**默认关**）照那组实参补画 | 原版**观测**；"由平面代画"是**我方接线** |

**一句话**：三个缺口都解出来了，而且都是**逐指令**级别的；唯一还是"我方"的只剩**接线**
（tick handler 的对象还在游戏状态里没接过来、`flip` 用一个导出属性顶替、白底由平面代画）。

### 48.1 缺口 1：`get_fractional_tick`（`0x519530`）的算式

函数体 `0x519530`–`0x5195f2`（其余分支是两条下溢 `panic`，`.rodata 0x1c649f` / `0x1c63b4`）。
`self` = `M`（72 B，`new` = `0x519690` 分配 `0x60` = 16 B Rc 头 + 8 B + 72 B）：

```text
519534  mov  esi,[rdi+0x28]        ; M+0x28 = "now" 的纳秒分量
519537  cmp  esi,0x3b9aca00        ; == 1e9 ? ⇒ **哨兵**（合法纳秒 < 1e9）
51953d  jne  519555                ; 有覆盖值 ⇒ now = (M+0x20, M+0x28)
51953f  call *0x9ba8f8 (0x8f3a10)  ; 否则 SystemTime::now() → (rax=secs, edx=nanos)
519545  mov  r8,[rbx]              ; base.secs  = M+0x00
519548  mov  ecx,[rbx+8]           ; base.nanos = M+0x08
51955f  call *0x9baf28 (0x90cdd0)  ; Duration::checked_sub(now, base)
519565  sub  rax,[rbx+0x10]        ; − accum.secs  = M+0x10    （jb ⇒ panic 0x1c649f）
519588  sub  edx,ecx               ; − accum.nanos = M+0x18
5195a9  …                          ; secs(u64) → f64：punpckldq 0x19c780 / subpd 0x19c000 那套
5195ce  divsd xmm2,[0x19eff8]      ; nanos / 1e9   （0x19eff8 = f64 1e9）
5195d6  addsd xmm2,xmm1            ; + secs        ⇒ 秒
5195da  divsd xmm2,[0x19f7f8]      ; / 0.016666666 ⇒ 不足一 tick 的小数部分
5195e2  mov  eax,[rbx+0x40]        ; M+0x40 = u32 tick
5195ed  addsd xmm0,xmm2            ; tick + 小数部分 ← 返回值（f64）
```

**算式**（`elapsed = (now − base) − accum`）：

```text
get_fractional_tick() = tick as f64 + elapsed.as_secs_f64() / 0.016666666
```

* `0.016666666` 的 8 字节 `.rodata 0x19f7f8` = `0x3f911111059d0921` = **`f64(16_666_666) / 1e9`**，
  **不是** `1/60`（`1/60` = `0x3f91111111111111`）。它与 `tick()` 每次累加的
  `NANOS_PER_TICK = 0xfe502a = 16 666 666` **同源**。⇒ 单测 `divisor_is_the_bit_exact_rodata_constant`
  直接钉位型（`tick_seconds().to_bits()`）。
* `accum` 每 `tick()` 加 `16 666 666 ns`（`0x51986c` 的 `add eax,0xfe502a`），所以
  `elapsed` 是"**这一 tick 已经走了多久**"。
* 下溢（`now < base + accum`）在原版是 `panic!`；本实现给 `0.0`（gdext FFI 里 panic = abort）。

**顺带解出的 `get_time_to_tick`（`0x519450`）**：`(accum + 1 tick) − (now − base)`，
进位判据 `0x51948f` 的 `cmp edi,0x3a9c79d6`（= `1e9 − 16 666 666`）。

**字段归属（72 B 全对上；`new` 的写点 + 上面两处读点）**

| 偏移 | 内容 | 指令 |
|---|---|---|
| `+0x00`/`+0x08` | `base: SystemTime` | 写 `0x5196df`/`0x5196e3`；读 `0x519545`/`0x519548` |
| `+0x10`/`+0x18` | `accum: Duration`（每 tick +16 666 666 ns） | 写 `0x519871`/`0x519875`（`tick` 里）；读 `0x519565` |
| `+0x20`/`+0x28` | `now` 的**覆盖值**；`+0x28 == 1e9` = "没设" | `0x5196f5` 只写哨兵；读 `0x519555` |
| `+0x30` | 自身指针（用途未知） | `0x5196fc` 的 `mov [rbx+0x48],rbx` |
| `+0x38` | `Rc<RefCell<GraphwarEventQueue>>` | `0x519700` |
| `+0x40` | `u32 tick` | 写 `0x51987d`；读 `0x5195e2` / `0x4b6317`（`draw` 里） |

⇒ **就地更正**（旧代码注释把 `+0x28` 的 `1e9` 读成"一秒的纳秒数"）：`0x519537` 的
`cmp …/jne` 表明它是**"now 覆盖值无效"的哨兵**。

**落地**：`src/graphwar_tick_handler.rs` 的 `GraphwarTickHandler::get_fractional_tick` /
`get_time_to_tick` / `now()` / `set_now()` / `clear_now()`（`frozen_now: Option<(u64,u32)>`
⇔ `M+0x20/+0x28`），以及 `GraphwarPlane` 侧：

```text
GraphwarPlane::fractional_tick()   -> tick_source.get_fractional_tick()      # 原版 0x4b631a
GraphwarPlane::integer_tick_now()  -> tick_source.current_tick as i64        # 原版 0x4b6317 (M+0x40)
GraphwarPlane::set_fractional_tick(t)                                        # **我方**摆钟口
    n = trunc(t)；accum = n×16 666 666 ns；now = accum + round(frac × 0.016666666 × 1e9)
    ⇒ 整数 t 读回来**逐位相等**（验收脚本按整数 tick 断言，不会因浮点抖动变红）
```

⚠️ **仍是接线不是复原**：原版那个 `GraphwarTickHandler` 在游戏状态里
（`draw` 里 `mov r14,[r12+0x238]` 借出来的 `Ref`，`0x4b61e5`–`0x4b631a`），本类还没有那条线
⇒ 自己 `new` 一个。`src/_scaffold/root_wiring.rs` 里那句"`get_fractional_tick`/`get_time_to_tick`
的算式（§44.7 仍列未知）"**已过期**，但那个文件不在本任务的允许改动清单里，留给下一个改它的任务。

### 48.2 缺口 2：`update_transform` 的三个实参（逐指令）

**先纠正任务书的一个地址**：调用点在 **`GraphwarPlane::draw`（`0x4b5df0`）体内 `0x4b6147`**，
**不在** `ready`（`0x4b7a70`）里。正对照：`callers 0x499050` = **1**（只有 `0x4b6147`），
同一套扫法对 `update_function`（`0x498c70`）给出唯一调用点 `0x4b69ec`，与 §42.3 一致。

调用点逐指令（`0x4b60a0`–`0x4b6147`）：

```text
4b60a0  mov  rdi,[rax+0x70]  ; 4b60a4 call 0x44ac60（拿 Gd 句柄）→ 4b60c9 前一路 upcast
4b60c9  call *0x9bb000 (0x8925f0)  ; Control::get_size() → xmm0 = w, xmm1 = h
4b60d4  movss [rsp+8],xmm0         ; w
4b60da  movss [rsp+0x10],xmm1      ; h
4b60e3  call 0x4be6e0              ; get_team_overview(...) → eax = **u32 的 4 个位**
4b60e8  movss xmm0,[0x1ba128]      ; 0.5f32（同一个 8 字节池的下一个 f32 是 30.0）
4b60f0  movss xmm1,[rsp+0x10] ; 4b60f6 mulss xmm1,xmm0    ; **arg2 y = h × 0.5**
4b60fa  movss xmm2,[rsp+8]    ; 4b6104 divss xmm2,[0x1b9fc8]  ; **arg3 scale = w / 50.0**
4b6100  mulss xmm0,xmm2(=0.5)                              ; **arg1 x = w × 0.5**
4b610c–4b6129   flip 位组合（见下）
4b6132  lea  rdi,[rax+0x80]        ; 被调者 = 平面上的函数绘制器
4b6139  movzx esi,dl               ; **arg4 flip（bool）**
4b6147  call 0x499050              ; FunctionDrawer::update_transform
```

`flip` 的位组合（两处同一个表达式：`0x4b6111`–`0x4b6129` 给 `esi`，`0x4b600d`–`0x4b6029`
给 `draw_set_transform` 的 x 分量）：

```text
A = (flags >= 0x1000000) && !(flags & 0x10000)     ; 4b6012 setae / 4b601b sete / 4b6024 and
B = ((flags & 0x101) == 0x100)                     ; 4b600d and / 4b605c cmp / 4b6129 or
flip = A || B
```

**被调者（`0x499050`）怎么用这三个实参**（这是"第三实参不是 `Vector2`"的证据）：

```text
49905e  movd  [rsp+0xc],xmm1      ; 存 y
499064  movss [rsp+8],xmm0        ; 存 x
499080  movdqa xmm0,[0x19e880]    ; 四个 -0.0 的符号掩膜
499088  movdqa [rsp+0x20],xmm2    ; 原值那份（低 32 位 = s）
49908e  pxor  xmm0,xmm2
499092  movdqa [rsp+0x10],xmm0    ; 取负那份
4990ef/4990f5  set_position([rsp+8],[rsp+0xc])          ; 两个 f32
499114/49911c  set_width(0.125f32 @0x1baff8)
499135  movdqa xmm0,[rsp+0x10]    ; **flip 分支**：取负那份
49913b  movdqa xmm1,xmm0
49913f  call *0x9bb238            ; set_scale(xmm0, xmm1) ⇒ (-s,-s)
499235  movdqa xmm0,[rsp+0x20]    ; **非 flip 分支**：xmm0 = 原值（s）
49923b  movdqa xmm1,[rsp+0x10]    ;                  xmm1 = 取负（-s）
499241  call *0x9bb238            ; set_scale ⇒ (s,-s)
```

gdext 的 `Vector2` 在这里是**两个 f32、各占一个 xmm**（`set_position` 的包装 `0x892890`
开头就是 `movss [rsp+0xc],xmm1; movss [rsp+8],xmm0`，`set_scale` 的 `0x892b50` 同样）。
⇒ `update_transform(x: f32, y: f32, scale: f32, flip: bool)`，**`scale` 是标量**，
y 分量的负号是被调者自己加的。

**同一套换算也出现在 `draw_set_transform`（`0x4b607d`）**：`pos = size×0.5`、`rotation = 0`、
`scale = (flip ? −s : s, −s)`（`0x4b6001` 的 `pxor` 给 y、`0x4b6059`–`0x4b6063` 选 x）。
⇒ 曲线上那 3 个 `Line2D` 与棋盘框**共用**一个变换。

**旁证**：`AxisDrawer::draw`（`0x4e05b0`）用的是**同一个**算式，只是没有 `flip`
（`0x4e0639` `divss 50.0`、`0x4e064b` `0.5`、`0x4e0648` 取负）⇒ `scale = (s, −s)`。

**⇒ 原来那组 `position=(500,300)` / `scale=(20,−20)` 的数值恰好就是原版值**
（1000×600 的平面 ⇒ `w/2 = 500`、`w/50 = 20`），但**来源从"我方选择"变成"算出来的"**。
`client/tools/plane_on_screen_check.gd` 已改成按 `plane.size` 反推期望值再断言。

⚠️ 还是我方的两处：
1. `flip` 的来源（原版 = `get_team_overview` 的位组合；本类没有游戏状态 ⇒ 导出属性 `mirror_x`，
   默认 `false` = 不镜像；另有一个导出属性 `flip_y`（默认 `true`）只管"y 是否取负"，
   原版**永远取负**，那个开关只为复现工具留）；
2. 喂给 `FunctionDrawer::update_transform` 的 `scale` 得先"反着取负"
   （那个文件本轮不许改：它在 `flip` 为真时把**两个**分量都取负 ⇒ 想得到 `(−s,−s)`
   就得传 `(s,s)`）。

### 48.3 缺口 3：`get_team_overview` 返回什么 + 两张表 + 棋盘底色

**(a) `get_team_overview`（`0x4be6e0`）返回 `u32` 的 4 个布尔位，不是表。**
函数尾部 `0x4beb95`–`0x4bebb6` 就是拼装：

```text
4beb95  movzx eax,[rsp+0x38] ; shl eax,0x18        ; bit 24
4beb9d  movzx ecx,[rsp+4]    ; shl ecx,0x10        ; bit 16
4beba7  movzx edx,[rsp+8]    ; shl edx,8           ; bit 8
4bebb1  movzx eax,[rsp+0xc]                        ; bit 0
4bebb6  or    eax,edx   ⇒ eax = (b38<<24)|(b4<<16)|(b8<<8)|(bc)
```

四个字节在 `0x4be83e`/`0x4be847`/`0x4be84f`/`0x4be857` 清零，循环体里被置
`0`/`1`（`0x4be870`–`0x4be888`、`0x4bead4`–`0x4beaee`）。
**⇒ `draw` 里两次调用（`0x4b5fd3` / `0x4b60e3`）拿到的只是一个 4 字节标志字**，
第二次的 `eax` 就是 `flip` 的位组合（§48.2）。**§42.3 那句"两张表来自它的两次返回值"
是错的，已就地更正。**

它内部在干什么（形状，**语义未知**）：从 `[rdi+0x40]` 的 `Rc<RefCell<…>>` 借 4 个东西
（`0x46c430` / `0x46b460` / `0x46b8b0` / `0x46b740`），遍历里面一个 `Vec<u32>`
（元素 4 字节、`lea rax,[rax*4]`），再按 `0x78` 的 bucket 步长查表（与函数管理器同一个步长）
⇒ **推断**：逐个玩家看队/在场情况，汇总成 4 个"是不是"结论。缺的证据：那 4 个借用器
各自指向哪个字段、以及 4 个位各叫什么。

**(b) 真正的"两张表" = `init_team_colors` 的两个 6 色池。**
容器是 `HashMap<u8, Vec<GraphwarColor>>`（键 = 队号 0/1，`GraphwarColor` = **3 字节 r,g,b**）。
x86 侧**不是内联**（`docs/10` §15.2 记的"内联"要改成这个地址）：**`0x465070`–`0x46545e`（1006 B）**，
12 个颜色在 `0x4650a3`–`0x465297` 用 `mov word [r+3k], 0xRG` + `mov byte [r+3k+2], 0xBB` 落库：

| 队 | 地址（每个 2 条指令） | 6 个 `(r,g,b)` |
|---|---|---|
| 0（Team1） | `0x4650a6/aa`、`0x4650cd/d1`、`0x4650f5/f9`、`0x465124/28`、`0x46514f/53`、`0x465176/7a` | `(186,42,32) (238,114,0) (125,104,0) (122,160,0) (149,5,62) (234,0,219)` |
| 1（Team2） | `0x4651b4/b8`、`0x4651de/e2`、`0x465209/0d`、`0x46523c/40`、`0x46526a/6e`、`0x465293/97` | `(107,0,234) (17,0,108) (21,83,221) (0,203,163) (0,116,26) (2,85,63)` |

逐位读数：`0x2aba`+`0x20` = (186,42,32)；`0x72ee`+`0x00` = (238,114,0)；`0x687d`+`0x00`
= **`(125,104,0)` = `#7D6800`**（就是真帧量测出来的那个弹道色，`docs/16` §12.2 ⑫）；
`0xa07a`+0 = (122,160,0)；`0x0595`+`0x3e` = (149,5,62)；`0x00ea`+`0xdb` = (234,0,219)；
第二池 `0x006b`+`0xea`、`0x0011`+`0x6c`、`0x5315`+`0xdd`、`0xcb00`+`0xa3`、`0x7400`+`0x1a`、
`0x5502`+`0x3f`。与 `docs/10` §15.3 的 arm64（`0x49c660`）复算**一字不差** ⇒ 档位从"arm64 逐位"
升级成**两架构互证**。

**表本来就有**（`src/graphwar_color_manager.rs` 的 `TEAM1_COLORS` / `TEAM2_COLORS`，`TEAM_POOL_LEN = 6`）；
本节新加的只是**出口**：`GraphwarPlane::team_color_pool(team) -> PackedColorArray`
（`0..=1` 给 6 个，别的队号给空 = 原版 `add_team_color` 的随机兜底），
验收脚本逐个分量对拍 12 个数。

**(c) `draw` 里那个"颜色"是怎么查出来的**（§42.3 的表结构，这里补形状）：

```text
4b681d  mov rax,[rsp+0xd0]        ; 表 1（HashMap<u32,u32>）
4b6842  call 0x47dec0             ; hash(key = player id)
4b68a3  cmp ebp,[r10-8]           ; 比 key
4b68de  mov r15d,[rcx+r8*8-4]     ; value = 表1[player id]（4 字节）⇒ 当表 2 的 key
4b68eb  mov rax,[rsp+0xc8]        ; 表 2
4b69a7  movzx eax,[rcx+r8*8-4]    ; 3 个**字节**：r
4b69ad  movzx edx,[rcx+r8*8-3]    ;                  g
4b69b3  movzx ecx,[rcx+r8*8-2]    ;                  b
4b69be–4b69d1  r9d = (b<<16)|(g<<8)|r      ; = update_function 的颜色实参
```

⇒ 表 2 是 `HashMap<u32, GraphwarColor>`（bucket 8 = 4 字节 key + 3 字节值 + padding），
与 `GraphwarColorManager` 的 `used` 表形状一致（`used_map` 的键 = entity id）。
**推断（未证）**：表 1 = "player id → entity/队"，表 2 = 颜色管理器的占用表；
两个表的来源是两个借用器 `0x46a770`（→ `[rsp+0xd0]`）与 `0x46b5d0`（→ `[rsp+0xc8]`），
对象是 `draw` 里 `mov r12,[…+0x238]` 的那个游戏状态。缺的证据：那两个字段的**归属**。

**(d) 棋盘内部的底色：不是队色，也不在平面里。**
`GraphwarPlane::draw` 只有那一条**空心黑框**（`Rect2(−25,−15,50,30)`、`Color(0,0,0,1)`、
`width = 0.2`、`filled = false`，`0x4b617d`–`0x4b61ab`）。**白底在 `AxisDrawer::draw`**：

```text
4e069f  movaps xmm0,[0x19cc20]   ; Rect2(-25,-15,50,30)
4e06ab  lea    rdx,[0x19ed00]    ; Color(1,1,1,1)
4e06b7  movss  xmm0,[0x1b9d08]   ; width = -1.0
4e06c2  mov    ecx,1             ; filled = true
4e06c7  xor    r8d,r8d           ; antialiased = false
4e06ca  call   *0x9bc028         ; draw_rect
```

⇒ **"棋盘内部是工程底色"的真正原因是 `AxisDrawer` 节点还没实现 `draw`**
（`src/_scaffold/axis_drawer_class.rs` 文件头明写"没接管 `draw`"），不是缺一张颜色常量表。
本类给了导出属性 `board_fill`（**默认关**）：置真就在同一套变换下补画那块白底。
**默认关的理由**：其它验收脚本按"每次 `_draw` 1 条 `draw_rect`"断言
（`draw_classes_check.gd` / `assemble_check.gd`），打开会变 2 条。

### 48.4 落了哪些文件

| 文件 | 改了什么 |
|---|---|
| `src/graphwar_tick_handler.rs` | 新增 `tick_seconds()`（+`TICK_SECONDS_BITS` = `0x3f911111059d0921`）、`duration_sub()`、`get_fractional_tick()`、`get_time_to_tick()`、`now/set_now/clear_now`、`accum()`；字段按 `M+0x00..0x48` 重排；文件头补 `0x519530`/`0x519450` 的逐指令算式表；**6 条新单测** |
| `src/_scaffold/graphwar_plane_class.rs` | `tick_source: GraphwarTickHandler`（不再存 `fractional_tick: f64`）；`_draw` 按 `size` 派生 `position`/`scale` 并调 `apply_update_transform`（`0x4b6147`）；新导出属性 `mirror_x`（= 原版 `flip`）、`board_fill`；纯函数 `flip_from_overview_bits` / `board_transform_args` / `board_scale_xy`；新方法 `transform_total` / `last_transform` / `integer_tick_now` / `team_color_pool` |
| `client/tools/plane_on_screen_check.gd` | 变换断言改成**按 `plane.size` 反推**（+`last_transform()` 对拍）；新增 tick 源、两张队色表、`flip` 真值表（9 个点）、`board_fill` 打开后每次 `_draw` 2 条 `draw_rect` 的断言（76 → **96** 条） |
| `docs/16-client.md` | 本节 + §42.3 / §42.5 第 4 条 / §43.6(d) 的第 2/3/7 条 / §43.7 就地更正 |

### 48.5 验收（本分支实测，可从干净树重跑）

```text
cargo test --offline                       # 1575 passed / 0 failed（基线 1569 + 6 条新单测）
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/plane_on_screen_check.gd   # 96 / 0（基线 76）
"$GODOT" --headless --path client --script res://tools/obstacle_drawer_check.gd   # 116 / 0（不退）
"$GODOT" --headless --path client --script res://tools/assemble_check.gd          # 203 / 0（不退）
"$GODOT" --headless --path client --script res://tools/draw_classes_check.gd      # 235 / 0（不退）
"$GODOT" --headless --path client --script res://tools/curve_on_screen_check.gd   # 96 / 0（不退）
```

### 48.6 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `get_team_overview` 那 4 个位**各是什么** | **推断** | 只证了"4 个布尔位 + `flip` 的位组合"；4 个位对应哪个判定、`0x46c430`/`0x46b460`/`0x46b8b0`/`0x46b740` 四个借用器指向哪些字段都没解 |
| `draw` 里两张 `HashMap` 的**归属** | **推断** | 形状是观测（表 2 的值是 3 字节 `GraphwarColor`）；哪个模块的字段、`0x46a770`/`0x46b5d0` 借的是谁，未证 |
| `GraphwarTickHandler` 的 `M+0x20/+0x28`（now 覆盖值）**谁写** | **未知** | 全二进制只观测到 `new` 写哨兵 `1e9`；原版可能从不写它（那就永远走系统时钟） |
| `M+0x30`（自身指针）**用途** | **未知** | 只观测到 `new` 里写 `rbx` |
| tick handler 的**对象来源** | **我方接线** | 原版从游戏状态借（`mov r14,[r12+0x238]`），本类自己 `new`；接过来要先复原那个 `Ref` 的所有者 |
| `flip` 的来源 | **我方替换** | 原版 = `get_team_overview` 的位组合；本类用导出属性 `mirror_x` 顶替（默认 `false`） |
| 棋盘白底由谁画 | 原版**观测** / 本类**我方接线** | 原版在 `AxisDrawer::draw`；那个节点没实现 `draw`，本类用 `board_fill` 代画。**网格线仍然没有**（`AxisDrawer` 的 102 条 `draw_line` 还没接） |
| 剩下 4 个缺口 | **未动** | 点生产者 `0x456810` 的写入者 `0x3e44d4` 归属、`get_function_fade_on_tick` 只有 GOT 槽 `0x9bbd08`、`z_index` 初值、`FunctionDrawer::draw` x86 未定位 |

### 48.7 一个踩过的坑（**地址空间**，下一个做反汇编的人会用到）

`libgraphwarrust.so` 的两个 `PT_LOAD` 里，`.text` 段的 **`p_offset` = `0x37a940` 而
`p_vaddr` = `0x37b940`（差 `0x1000`）**，第一个（R）段才是 `offset == vaddr`。
⇒ 用 Python 直接按**文件偏移**搜字节时，`.text` 里的命中要 **+0x1000** 才是文档/工具里的 vaddr
（本次就是在文件偏移 `0x4640d1` 搜到 `mov word […],0x72ee`，对应 vaddr **`0x4650d1`**）。
`.rodata` 在第一个段里，`offset == vaddr`（`.rodata 0x19f7f8` 直接读就对）。
`tools/addr_probe.py` 打印的与 `@0x…` 解析的都是 **vaddr**，两者不要混。

## 49. 六个真 GDExtension 节点挂进播放屏：`plane_view.gd` / `fire_fx.gd` 删除（2026-11，任务 31）

**一句话**：播放屏（`client/src/screens/play_screen.gd`）的战场不再由 GDScript 自绘 ——
**棋盘框 / 曲线 / 士兵 / 网格 / 障碍 / 特效** 六样东西现在分别由 `GraphwarPlane` /
`FunctionDrawer` / `EntityDrawer` / `AxisDrawer` / `ObstacleDrawer` / `EffectDrawer`
六个 **Rust 扩展类**画，两个仿制 `.gd`（`plane_view.gd` 527 行、`fire_fx.gd` 261 行）删除。

**这是"客户端迁回 Rust"的第一刀**（`AGENTS.md` §0 第 4 条那条路的第一个落地点），
**不是**"客户端复原完了"：§49.5 列了 8 条还缺的，其中**准备圈在画面上已经没有了**。

### 49.0 先说没做到的（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| **准备圈**（`docs/140` 那三弧 + 三箭头） | **绘制已补（§50），触发仍未接**：`EntityDrawer::draw`（`0x4b2890`）那 6 条图元现在由 `_draw` 真的发出去（3 `draw_arc` + 3 `draw_polygon`），但只有 `set_ring()` 填过的实体才画，而播放屏还没人调它 ⇒ **画面上仍然没有** | 绘制=**观测**（118 条断言）；触发=缺口 |
| **士兵/靶子的真贴图** | `EntityDrawer` 用的是**程序生成的 8×8 纯色**（皮肤/脸/帽子三块方块），不是 pck 里的 `skins/*`、`faces/*`、`hats/*` | 缺口（§33 对照表早就记了） |
| **tick 源** | 原版 `GraphwarTickHandler::get_fractional_tick`（`0x519530`）没接 ⇒ 播放屏按**帧间隔**自己推 | 缺口（§43.6(d) 第 2 条） |
| 点的生产者 | 仍由播放屏把内核算出的弹道 `set_function_points()` 进去（`0x3e44d4` 归属仍未证） | 缺口（§43.6(d) 第 1 条） |
| 队色表 / 淡出 / `z_index` / `FunctionDrawer::draw` | 与 §43.6(d) 第 4–7 条**一模一样**，没有变化 | 缺口 |

### 49.1 本轮动了哪些文件

| 文件 | 动作 | 说明 |
|---|---|---|
| `client/src/screens/play_screen.gd` | 改（+698/−？） | 6 个真节点接线的本体；44 处 `_plane.*` 全部改到真节点 API；新增 `_sync_entities()` / `_set_curve()` / `_advance_curve()` / `plane_points()` / `plane_state()` 等 |
| `client/src/game/effect_layer.gd` | 改（自绘 → 适配，211 行里删掉 ~120 行） | 不再 `draw_arc`/`draw_circle`；坐标映射整段删除，改成 `EffectDrawer.spawn_effect()` + `retain_alive()` |
| `client/src/play/plane_view.gd`（`GwPlaneView`，527 行） | **删除** | 六个位置全被真类接管 |
| `client/src/play/fire_fx.gd`（`GwFireFx`，261 行） | **删除** | 曲线揭示改用 `GraphwarPlane` 的 30 点/tick；准备圈**没有替身**（§49.5 第 1 条） |
| `client/src/play/plane_view.gd.uid` | **删除** | 上面那个脚本的 uid 存根（脚本没了，存根也就没了） |
| `client/tools/selftest.gd` | 改（44 条断言换主语 + 4 条新增，净 **+15** 条） | 详见 §49.7；**没有删断言**，只换主语；新增的 4 条是"网格/实体/障碍真的被引擎画过"与"画布真的被容器铺开" |
| `src/**` | **一个字没动** | 六个节点类上一轮就都在（§33 / §35 / §34 / §43 / §45），本轮只接线 ⇒ `cargo test` 不用重跑新逻辑（仍是 1569），`coverage_matrix.py` 也不用重生成 |

### 49.2 现在播放屏里的节点树（以及"画布模型"这条我方选择）

```text
PlaneContainer (AspectRatioContainer, ratio = 50:30, STRETCH_FIT)   ← 原版同名同参数
 └ PlaneCanvas (Control)                     ← 拿到容器铺的 5:3 矩形；clip_contents = true
     └ GraphwarPlane  (size 1000×600, scale = fit)   ← ① 棋盘框（`_draw` 的 draw_set_transform + draw_rect）
         ├ @FunctionDrawer@N ×3  (z=2)       ← ③ 曲线（每 player 一条 Line2D，平面 `_draw` 里喂点）
         ├ AxisDrawer            (z=-1)      ← ④ 白底 + 网格（`axis_primitives`）
         ├ ObstacleDrawer        (z=1)       ← ⑥ 障碍掩膜贴图（`update` 每帧 2 次）
         ├ EntityDrawer          (z=3)       ← ⑤ 士兵 / 名字 / 靶子（每实体 3 sprite + 1 标签）
         ├ EffectDrawer          (z=4)       ← ② 特效（`frame_primitives`）
         └ EffectLayer           (适配层)     ← 只记账 + 派给 EffectDrawer，**不画**
```

**画布模型是"我方选择"**（`§43.6(d)` 第 3 条：原版 `draw_set_transform` 的三个实参没解出来）：
六个类各自按**自己控件的尺寸**换算 —— `axis_drawer` / `effect_drawer` 用 `size/50` 与
`size/2`，`entity_drawer` 用 `size.x/50` + `size/2`，`obstacle_drawer` 用 `size/1000` +
`size/2`，`graphwar_plane` 用固定的 `PX_PER_UNIT = 20` 与 `PIXEL_OFFSET = (500,300)`。
要把它们摆进同一个坐标系，只有一种不改 Rust 的做法：**把它们全放到 1000×600 的固定画布上**
（`src/plane.rs::CANVAS_W/H`，也是 `obstacle_drawer` 掩膜、`axis_drawer` 白底、`to_pixel()`
共同的那块画布），再用 `Control.scale` 把整块画布缩放进 `AspectRatioContainer` 给的矩形。
⇒ 播放屏只做**一处**缩放（`_fit_canvas()`），六个类内部一个数都不用改。
`--headless` 的视口是 64×64，缩放比会很小，但断言全部是"点数 / 条数 / 像素数"这类
**不依赖窗口尺寸**的量（§49.7 的口径），所以 headless 能验。

**z 序也是我方选择**（`GraphwarPlane` 的 `z_index` 初值是 `未知`，§40.3f）：`AxisDrawer`
用 `z_as_relative = false` + `z_index = -1` 画在棋盘框**之下**（否则它那张 1000×600 白底会
盖住平面的黑框 —— 引擎先画父再画子）；其余四层按"障碍 < 曲线 < 士兵 < 特效"排。

### 49.3 `effect_layer.gd`：从"自绘"变成"适配"（附一张**我方选择**的映射表）

| 项 | 旧（自绘） | 新（适配真节点） |
|---|---|---|
| 形状 | 手画环/火舌/实心圆/十字/方点（**自定**） | `effect_drawer::frame_primitives` 的四类图元（半径 0.05/0.06、四组基色、`max(0,(1−t)·0.9)`，全是 `.so` 立即数） |
| 坐标 | 靠 `plane.board_to_screen()` 换算 | **整段删除**：`EffectDrawer` 自己发 `draw_set_transform(size/2, 0, (size.x/50, ∓))`（`draw_transform`，逐位） |
| 寿命 | 自定秒数（`LIFETIME`） | `duration_ticks`（旧的秒数 ×60 **搬过来**，因为原版时长无证据）+ 调用方每帧 `retain_alive()`（判据**推断**，所以节点类故意没自己跑） |
| 名字 → 相位 | 四种名字各有一套自定形状 | **映射表**（`PHASE_FOR_KIND`）：`ExplosionEffect`→2 `CircleLargeA`、`DeathEffect`→6 `CircleDark`、`SparkEffect`→0 `CircleSmall`、`AshEffect`→4 `LineA` ——**我方选择**：原版是 `EffectInfo` 判别值（0..3 的跳表）进 `EffectData`，而 `Phase` 有 **7** 档，两者是不是同一个编号**没有任何证据** |
| 多边形 / 线段 | 会画 | 多边形顶点未复原 ⇒ 跳过（`polygon_skipped_total` 计数）；线段端点未复原 ⇒ 退化线段 |

⚠️ 附带一个**真的观测**：`EffectDrawer` 的 `step_effects` 会把相位 `CircleLargeA(2)` 推进成
`CircleLargeB(3)`（其余推进成 `CircleDark(6)`），所以"刚 spawn 时的相位"和"两帧后的相位"
**本来就不同** —— 自检里那条断言因此必须**当刻**读（§49.7 的踩坑记录）。

### 49.4 `TEAM_COLORS` / `assign_colors()` 搬到播放屏（数字一个字没动）

`GwPlaneView.TEAM_COLORS`（`docs/102` §1.1 从 `.so` 调色板表解出的 2×6 色表）与
`assign_colors()`（随机起点 + 线性探测）**唯一**的两个消费者都在 `play_screen.gd`
（开局配色、联机配色、回放的缺省色）。类删掉之后整块搬到播放屏，**六个数一个都没改**，
自检里 `TEAM_COLORS[0][2] == #7D6800` 那条断言也照旧成立（只是主语从 `GwPlaneView` 换成播放屏）。

### 49.5 缺口诚实清单（8 条，按"画面上看得见的影响"排序）

1. **准备圈：绘制补上了（§50），触发还没接**（最显眼的一条）。原版是 `EntityDrawer::draw`
   的 3 次 `draw_arc` + 3 个外凸箭头（`0x4b2890`，`docs/140` §2 逐条立即数）。
   §50 之后 `EntityDrawer` 的 `_draw` 就照那 6 条图元发（`client/tools/entity_drawer_check.gd`
   72 → **118** 条断言，含"关掉准备态就不画"），**但画面上仍然看不到**：
   `_draw` 只画 `set_ring()` 填进 `prep` 表的实体，而播放屏还没人调它 —— 原版填这张表的是
   `ready` 里订的 `pos_info_event`（`GraphwarBackend` 没接），而任务 33 的允许改动清单
   **不含播放屏逻辑** ⇒ 触发留到 §50.6 那三行改法。几何证据没丢：
   `tests/entity_drawer.rs` 的 `arcs_carry_the_exact_draw_arc_arguments`
   （10 折点 / 0.3 带宽 / 关 AA / `0x00687d`）与 `open_ring_is_narrower_than_closed_ring`
   （张开 60° / 闭合 123°）仍在 `cargo test` 里跑。
2. **士兵与靶子的贴图是纯色方块**（`EntityDrawer` 的程序生成贴图，§33 对照表"我方选择"）。
   真贴图要 `GraphwarTextureLoader`（三张 `HashMap<String, Texture2D>`）那一层，没接。
   靶子**还能看见**，但长得和士兵一样（`EntityDrawer` 没有"种类"概念，靠 id 区分：
   士兵 = `_soldiers` 下标，靶子 = `1000 + target_id`）。
3. **点的生产者仍是我方注入**（§43.6(d) 第 1 条没变）。
4. **tick 源仍是我方推**（`get_fractional_tick` 没接）：`_advance_curve()` 按帧间隔推
   `60 tick/s`，`advance_shot_for_test()` 是自检的确定性入口。
5. **`update_transform` 的两个实参**（`position=(500,300)` / `scale=(20,-20)`）仍是**我方选择**，
   与 `plane_on_screen.tscn` 逐值相同（§43.6(d) 第 3 条没变）。
6. **淡出没接**（`get_function_fade_on_tick` 无 x86 落点，走 GOT 槽 `0x9bbd08`）。
7. **队色表没接**（`GraphwarDrawUtil::get_team_overview`，`0x4be6e0` 的两张表）。
   于是 `EntityDrawer.team_color` 用默认值（`0x0000ff`，镜像判据 = 不翻转），
   曲线颜色仍由播放屏把玩家的 `Color` 打包成 `0x00RRGGBB` 传进去。
8. **三处注释还写着旧类名**（`client/src/core/core_bridge.gd` 第 59 行、`client/src/game/target_set.gd`
   第 75 行、`client/src/play/replay_player.gd` 第 364 行都提到 `GwPlaneView.xxx`）。那三个文件
   不在本任务的允许改动清单里 ⇒ 没动，下一轮顺手改。

### 49.6 就地把 §43.6 那张表的过期行改掉（改了什么）

| 位置 | 原文（过期） | 现在 |
|---|---|---|
| §43.6 抬头 | "**本轮没动它**——本任务只证明真类能在场景里画出来" | 加了一句"本节已被 §48 执行完"，正文保留（记录当时缺什么） |
| §43.6 (a) 表 | 两行只有"换成什么"，没有现状 | 加"现状"列：**两行都已删除**；`fire_fx` 那行注明**准备圈没有替身** |
| §43.6 (b) 表头 | "（现在 1 个都没有）" | "（写本节时 1 个都没有；**§48 之后 6 个全挂上了**）"，并加"现状"列 |
| §43.6 (b) 第 6 行 | "**类还不存在**（§40.5 / §42.5 第 7 条）" | 就地注明**写的时候已经过期**（第四轮任务 23 做出了该类，§45 有 116 条断言），现状"已挂" |
| §43.6 (c) | 三件"要改的" | 标"三件都改完了" |
| §43.6 (d) 7 条 | 纯缺口清单 | 每条后面加"→ §48 之后是什么状态"：第 7 条的**网格**已经由 `AxisDrawer` 画出来（白底 + 线），只有**队色表**仍缺 |
| §43.7 第 1 条 | "客户端播放屏仍是仿制品" | 见下方 §49.6b 的就地更正 |

**§49.6b（就地更正 §43.7）**：§43.7 第 1 条写"客户端播放屏仍是仿制品、§42.5 第 5 条那句
'客户端还没用真类'对播放屏仍然成立" —— **从 §48 起这句话对六个绘制位置不再成立**
（棋盘框/曲线/士兵/网格/障碍/特效都来自 Rust 类），**但整屏仍是仿制品**（
`play_screen.gd` 的流程/时序/输入/回合仍是我们写的，`AGENTS.md` §0 第 4 条的判断不变）。

### 49.7 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target31
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
cargo build --offline --example serve_match                             # 自检的两个联机用例要它
export HOME=<假 HOME>                      # selftest 读 $HOME，不走 XDG_DATA_HOME
mkdir -p $HOME/.local/share/godot/app_userdata/Graphwar
cp -r <replays 快照>/replays $HOME/.local/share/godot/app_userdata/Graphwar/
"$GODOT" --headless --path client --import                              # 删了 .gd 必须重扫
"$GODOT" --headless --path client -- --selftest
```

| 口径 | 改前（`CG@1877049`，同一台机同一份 .so） | 改后 |
|---|---|---|
| selftest（**有** assets + 回放样本 + `serve_match`） | **694 passed / 0 failed** | **709 passed / 0 failed**（+15） |
| selftest（**无** assets：`GW2_ASSET_DIR=res://no_assets`） | 675 passed / **2 failed** / 10 skipped | 690 passed / **2 failed** / 10 skipped |
| 两条 FAIL 的身份 | `点 Skins 页的 -1 号皮肤…`、`Cancel = 194 宽 / Start = 218 宽` | **同两条**（既有问题，不是本批引入） |
| `cargo test --offline` | 1569 passed / 0 failed（1 ignored） | **1569 / 0**（`src/` 一个字没动） |

`client/tools/` 那九个 headless 脚本（改后实测，一个都没退）：

```text
gdext_check         24 passed, 0 failed
assemble_check     203 passed, 0 failed
plane_on_screen     96 passed, 0 failed
obstacle_drawer    116 passed, 0 failed
function_drawer    132 passed, 0 failed
draw_classes       235 passed, 0 failed
curve_on_screen     96 passed, 0 failed
entity_drawer       72 passed, 0 failed
base_classes       117 passed, 0 failed
signal_classes     155 passed, 0 failed
spike_class_check   23 passed, 0 failed
```

> ⚠️ **审计 2026-11-08**：本清单原记 `plane_on_screen 76`，与同一份文档 §48.5 的
> 「`plane_on_screen_check.gd` 96 / 0（基线 76）」矛盾；按 §48.5（+`plane.size` 反推变换断言那一批）
> 改成 **96**。⚠️ 这一格是**转引**（§48.5 + 现行 master 实测值），**本次审计没有复跑 headless**
> （需要 Godot + 已编 `.so` + `--import`），所以标 **观测（转引）** 而不是本机复测。

**"6 个节点都挂上了"在自检里怎么验的**（`selftest.gd` 的 `_test_fire_fx` ② 那一段）：
`ClassDB.class_exists` ×6 + `plane_state()` 六个字段全部**从真节点读回来**
（`function_drawers`=3、`axis_lines`=102、`plane_draws`>0、`entities`=士兵+靶子、
`obstacle_updates`>0、`fx_draws`>0）+ 四个网格档位 0/2/20/102 + 一个实体 4 个子节点 +
`Line2D` 线宽 `0.125`（逐位常量）+ **三个"引擎真回调过"的计数**
（`AxisDrawer.draw_call_count` / `EntityDrawer.process_call_count` / `EffectDrawer.draw_call_count`）
+ **一次真注入**（注入 1 个障碍 ⇒ 掩膜里 11310 个像素被占）
+ **画布真的被容器铺开**（headless 下实测 `canvas = (124.0, 74.4)`、`plane.scale = (0.124, 0.124)`，
  两个数正好是 5:3 与 `124/1000`）。
**一条都不是"看代码猜的"**：每条都是引擎回调的计数或真节点里的真值。

**两个本轮踩到的坑（写下来省下一轮的时间）**：

1. `GraphwarPlane.clear_function_points()` **不删 `Line2D`**（它只清平面自己那张登记表）。
   只调它的后果不是"看着没删"，而是**下一条曲线一个点都画不出来**：同一 player id 再
   `update_function` 时 `FunctionDrawer` 还记着"已写入 N 点"，`append_window` 判
   `new_len <= N` 直接 return（`0x498d5a` 的 `ja`）。必须再对那个 `FunctionDrawer`
   调 `remove_function`（原版 `0x498b70`：`queue_free` + 删表）。
   平面的 `_draw` 里那条 `remove_function`（`0x4b6418`）只在"函数离开活跃窗口"时跑。
2. `EffectDrawer` 的相位会自己往前走（2 → 3、其余 → 6，`step_effects` 的"到期换相位"），
   所以"spawn 时的相位"必须**当刻**读；隔两帧读到 3 是**对的**，不是 bug。
3. `ObstacleDrawer.add_obstacle(x, y, radius)` 收的是**棋盘单位**，不是画布像素 ——
   `plane::raster_params()`（`0x3f17a0` 那条路径的入口）自己就做那次换算
   （`px = x*20 + 500`、`r = size*20`）。本轮第一版在播放屏里**又换了一次**，
   圆心被推到 (10500, 6300) ⇒ 一个像素都画不出来（自检条数上表现为 1 条 FAIL，
   肉眼看是"障碍整个消失"）。

### 49.8 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么 |
|---|---|---|
| "六个类都**在画**" | **观测** | 引擎回调计数（`draw_call_count` / `drawn_*_total` / `occupied_pixels` / `plane_draws`），本轮实测非零 |
| 六个类**画得与原版一致** | **未知** | 没有本轮的截图对拍：① 准备圈缺失（§49.5 第 1 条）；② 士兵贴图是纯色方块；③ 画布模型（1000×600 + 缩放）是**我方选择**，原版那次 `draw_set_transform` 的实参没解；④ 队色表没接 |
| 准备圈的几何 | **观测** | `tests/entity_drawer.rs` 的两条测试；**§50 之后**这 6 条图元已经由 `_draw` 真的发到引擎（`entity_drawer_check.gd` 118 条断言），但**画面上仍然没有**（触发没接，§50.0 第 1 条） |
| 名字 → 相位那张表 | **我方选择** | 原版 `EffectInfo` 判别值 → `EffectData.phase` 的映射没有任何证据 |
| 特效时长 | **我方选择** | 原版只给 `start_tick`；`retain` 的判据是**推断** |
| 障碍坐标换算 | **观测** | 掩膜是 1000×600 画布像素（`plane::CANVAS_W/H`），棋盘单位 → 像素走 `px = 20x + 500`、`py = 300 − 20y`、`r_px = 20r`（`to_pixel` 的同一套数） |
| `--headless` 下的画面对不对 | **测不到** | 视口 64×64、没有渲染器；像素级只能靠有限窗口截图（`docs/16` §43.5 的办法），本轮**没做**截图 |

## 50. 第二批 6 个 manager 接上容器 + 事件流 + 最小交互（2026-11，任务 40）

**一句话**：§44 让 5 个 manager 从"有键"到"能交互"，§47 把档位词表定成四档。本节把
**另外 6 个**（`turn` / `player` / `team` / `mode` / `entity_effect` / `effect`）按同一口径接上：
**容器 + 事件流（订阅/发两个方向都指到指令）+ 一次最小交互**，档位从 `assembled` 抬到 **`wired`**。
`assemble_check.gd` 从 **203 条扩到 213 条**，新增的 10 条里有 38 个"交互后读回"的值
（与 `root_wiring.rs` 的单测**逐字相同**）。

> 档位口径同 §42 / §44 / §47：**逐位** = 与真二进制逐字节/逐位对拍；**观测** = 能指到反汇编地址 /
> 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。`cargo test` / `assemble_check.gd` 的条数
> **只是回归保护**，不是复原证据。

### 50.0 先说"没做到"的部分（免得被下面那张表误导）

| 项 | 现状 | 档位 |
|---|---|---|
| `GraphwarGameStateManager`（296 B）的**事件层** | **没接**：容器里装的是 `game::Match`，它的字段全在 `src/game.rs`，本轮该文件是**禁区**（有人正在 `execute_results` 上干活）⇒ 这一槽**留在 `assembled`** | 缺：`Match` 的 7 条订阅 + 发方向落进 `src/game.rs` |
| `Match` 的 7 条订阅（`new_connection_event` / `entity_removed_event` / `tick_event` / `connection_removed_event` / `function_points_event` / `skin_graphics_info_event` / `chat_message_event`） | 名字**已有证据**（`docs/14` §11.7 的 `.rodata` 逐条 `lea`，`0x477bc0`，`fixtures/name_to_addr.tsv` 里 7 行 `subscribe_to_events_event_handler`），但**代码没接** | 订阅名 `观测`，接线 `未知`（禁区） |
| `turn_manager::send_all_info_events` 的**载荷** | **我方形状**：`0x462fe0` 能确证"加锁遍历 + 一条 `add_event`"，载荷构造没解（它**不**调 `get_current_turn_players`/`get_current_turn_soldiers`） | **推断**（见 §50.7） |
| 6 个 manager 各自的**原版字段布局** | 只有 `ModeManager` 这一轮新解到 32 B（`+0x00/0x08/0x10` 三个借用字段 + `+0x18` 两个模式字节）；其余 5 个**仍是我方形状** | 结构轴 `未知`/`部分观测`（**不**因升到 `wired` 而变成"布局已复原"，见 §47.1 读法 1） |
| 每条订阅**闭包函数体**的逐指令语义 | 只有 `entity_effect`（§46 已有的 `:67:30` / `:77:18`）与 `player`（§3/§4 的 `逐位`）跳到"处理器 = `send_all_info_events` / `handle_entity_removed_event`"这一层 | 处理器**绑定** `观测`；闭包体内**逐条**语义多数 `未知` |

### 50.1 本轮动了哪些文件

| 文件 | 增/改 | 内容 |
|---|---|---|
| `src/graphwar_turn_manager.rs` | +接线层 | `SUBSCRIBED_EVENTS`（2 条）+ `subscribed` / `outbox` + `begin_turn` 发 `TurnInfo`(43) + `handle_entity_removed_event` + `send_all_info_events` / `handle_new_connection_event` + `take_outbox` / `outbox_len` + 5 条单测（轮转/判决/分队算术**一行没动**） |
| `src/graphwar_player_manager.rs` | +接线层 | `subscribed` / `outbox` + `add_player`/级联各发一条 + `handle_new_connection_event` + `take_outbox` / `outbox_len` + 3 条单测（`SUBSCRIPTIONS` 常量原来就有，**这次真的用上了**） |
| `src/graphwar_team_manager.rs` | +接线层 | `subscribed` / `outbox`（`Vec<TeamEventKind>`，理由见 §50.7 第 1 条）+ `handle_new_connection_event` + `take_outbox` / `outbox_len` + 3 条单测（`pick_team_from` 的算术**一行没动**） |
| `src/graphwar_mode_manager.rs` | +接线层 | `SUBSCRIBED_EVENTS`（1 条）+ `subscribed` / `outbox` + 三个 setter 各发一条 + `handle_new_connection_event` + `ModeInfoEvent::event()` + `take_outbox` / `outbox_len` + 3 条单测；`Copy` 派生**去掉**（`Vec` 不是 `Copy`），`Default` 改成手工（否则 `default()` 会静默丢订阅） |
| `src/graphwar_entity_effect_manager.rs` | +接线口 | `handle_new_connection_event`（= `send_all_info_events` 的同义入口）+ `take_outbox` / `outbox_len` + 3 条单测（`outbox` / `subscribed` / 两个处理器原来就有） |
| `src/graphwar_effect_manager.rs` | +接线口 | `handle_tick_event(tag, now)`（判别值闸 0）+ `take_outbox` / `outbox_len` + `Default` 改手工 + 2 条单测（`tick` / 寿命 3600 **一行没动**） |
| `src/_scaffold/root_wiring.rs` | 改 | 2 个新取回口（`effect_manager` / `entity_effect_manager`）+ `interaction_selftest` 追加 38 条 + `Wired::status()` 6 个槽 → `wired` + 四档计数 4/2/15/1 → **10/2/9/1** + 3 条新单测 + 文档表同步 |
| `client/tools/assemble_check.gd` | +10 条 | `WIRED_TIER` 6 个槽 + `TIER_TALLY` + `expected_probe` 27 → **65** 条 + 10 条单值/事件流断言 |
| `docs/coverage-matrix.tsv` / `docs/149-coverage-matrix.md` | 重跑（**派生**） | `src_fns` **2504 → 2553**、`src_tree_sha256` 跟着变；**527/527 行与每模块数字一行未变** |
| `docs/16-client.md` | +本节 | — |

❌ `src/game.rs` / `src/lib.rs` / `client/src/**` / 除这 6 个之外的 manager / 任何既有断言 **一行没动**。
既有 27 条交互读数**一个字都没改**（新 38 条是**追加**在末尾），所以 §44 的验收串仍然原样全绿。

### 50.2 6 个 manager：订阅什么、发什么（全部 `观测`，x86 2026-08-28）

复算工具：`../.venv/bin/python tools/addr_probe.py dis <vaddr>`（本机 capstone，离线）+ `callers <vaddr>`。
每条订阅的判据是**三件事同时成立**：`new` 里 `lea rsi,'<名字>'`、`mov edx,<len>`
（= `strlen`）、`call [slot 0x9bac88] -> 0x3d38f0`（`GraphwarEventHandler::subscribe_to_event`）；
处理器判据是"`lea r8` 那个槽 `+0x20` 指向的跳板 → 哪个函数"。

| manager | 原版 `new` | 订阅（按调用序，名字 / len / 调用点） | 处理器 | 发出的事件（tag） | 交互条数 | 档位 before → after |
|---|---|---|---|---|---|---|
| `graphwar_turn_manager` | `0x463790` | `new_connection_event` /`0x14`/ `0x463815`；`entity_removed_event` /`0x14`/ `0x463866` | `0x462fe0` / `0x463580` | `TurnInfo`(**43**)：`next_turn` `0x46406b`–`0x464072`（活射手）/ `0x464016`–`0x464021`（`{p,0}`） | **6** | `assembled` → **`wired`** |
| `graphwar_player_manager` | `0x455410` | `new_connection_event` /`0x14`/ `0x455495`；`entity_removed_event` /`0x14`/ `0x4554e6` | `0x455160` / `0x455250` | `PlayerInfo`(**13**)（`add_player` / `send_all_info_events`）、`ConnectionRemoved`(**6**)（级联）——与模块文档 §3/§4 的 `逐位` 结论一致 | **6** | `assembled` → **`wired`** |
| `graphwar_team_manager` | `0x3c1fc0` | `new_connection_event` /`0x14`/ `0x3c2045`；`entity_removed_event` /`0x14`/ `0x3c2096` | `0x3c1cf0` / `0x3c1f40` | `TeamInfo`(**24**)（`add_to_team` / `send_all_info_events`）、`RemoveTeam`(**25**)（**删到了才发**，`0x3c1a08` 的 `cmp dl,2`） | **7** | `assembled` → **`wired`** |
| `graphwar_mode_manager` | `0x5186c0` | **只一条** `new_connection_event` /`0x14`/ `0x51876c` | `0x5185a0`（跳板 `0x5186a0`） | `AxisModeInfo`(**45**) / `FunctionModeInfo`(**46**) / `TimeModeInfo`(**48**)（三个 setter 各一条 + `send_all_info_events` 三条） | **5** | `assembled` → **`wired`** |
| `graphwar_entity_effect_manager` | `0x478490` | `new_connection_event` /`0x14`/ `0x478515`；`entity_removed_event` /`0x14`/ `0x478566` | `0x4782f0` / `0x4783f0`（§46 已有） | `EntityEffectInfo`(**56**)（`add_entity_effect` / 整表重播；`handle_entity_removed_event` **不发**） | **7** | `assembled` → **`wired`** |
| `graphwar_effect_manager` | `0x519ba0` | **只一条** `tick_event` /`0xa`/ `0x519ca5` | 体内（无独立跳板） | `EffectInfo`(**44**)（`add_effect`）；`tick` **不发**（模块文档 §4） | **7** | `assembled` → **`wired`** |

上表的地址串（可直接复跑，逐条与 `addr_probe.py` 的输出对得上）：

```text
# TurnManager::new 0x463790 的两条 subscribe（名字 / len / 调用点）
0x4637fc  lea rsi @0x1c18ad 'new_connection_event…'   0x46380a mov edx,0x14   0x463815 call subscribe
0x46384d  lea rsi @0x1c18c1 'entity_removed_event…'   0x46385b mov edx,0x14   0x463866 call subscribe
          r8 = 0x957be8 → +0x20 = 0x957c08 = 跳板 0x463750 → 0x462fe0 send_all_info_events
          r8 = 0x957c10 → +0x20 = 0x957c30 = 跳板 0x463770 → 0x463580 handle_entity_removed_event
# PlayerManager::new 0x455410
0x45547c  0x1c0eef 'new_connection_event…'   0x45548a edx,0x14   0x455495 call   → 跳板 0x4553d0 → 0x455160
0x4554cd  0x1c0e12 'entity_removed_event…'   0x4554db edx,0x14   0x4554e6 call   → 跳板 0x4553f0 → 0x455250
# TeamManager::new 0x3c1fc0
0x3c202c  0x1be298 'new_connection_event…'   0x3c203a edx,0x14   0x3c2045 call   → 跳板 0x3c1fa0 → 0x3c1cf0
0x3c207d  0x1be2ac 'entity_removed_event…'   0x3c208b edx,0x14   0x3c2096 call   → 跳板 0x3c1f80 → 0x3c1f40
# ModeManager::new 0x5186c0
0x518753  0x1c63fb 'new_connection_event…'   0x518761 edx,0x14   0x51876c call   → 跳板 0x5186a0 → 0x5185a0
# EntityEffectManager::new 0x478490
0x4784fc  0x1c18ad 'new_connection_event…'   0x47850a edx,0x14   0x478515 call   → 闭包 0x4782f0
0x47854d  0x1c18c1 'entity_removed_event…'   0x47855b edx,0x14   0x478566 call   → 闭包 0x4783f0
# EffectManager::new 0x519ba0（**只一条**）
0x519c8c  0x1c6513 'tick_event…'             0x519c9a edx,0xa    0x519ca5 call
```

两条**新事实**（相对 §41 / §47 的台账）：

1. **`ModeManager` 只有 1 条订阅、而且是 `new_connection_event`**，不是四个
   `*_mode_change_request`。也就是说这个模块"收到新连接就把三个模式重播一遍"，
   **不**订阅模式变更——与模块文档那句"权威在服务器"是同一件事的两面：
   写入走 `GraphwarRemoteEventHandler::handle_server_remote_event`，订阅只服务"补发"。
2. **`TurnManager` 的 `entity_removed_event` 删的是整条玩家记录，不发任何事件**。
   `0x463580` 体内没有 `add_event`（那槽 `0x9bac80` 在这 960 字节里**一次都没出现**），
   删到的值按 **4 字节元素**的 `Vec` 释放（`0x46362e shl rsi,2` / `0x463632 mov edx,4`）
   ⇒ 值里那个 `Vec<soldier_id>` 跟着整条记录一起没了，**不是**"从某个 Vec 里摘掉一个士兵"。

### 50.3 落码：容器里现在装的是什么

```text
TurnManager                 # graphwar_turn_manager.rs（原类型 + 接线层）
GraphwarPlayerManager       # graphwar_player_manager.rs（原类型 + 接线层）
TeamTable                   # graphwar_team_manager.rs（原类型 + 接线层）
GraphwarModeManager         # graphwar_mode_manager.rs（原类型 + 接线层；32 B 布局本轮新解）
GraphwarEntityEffectManager # graphwar_entity_effect_manager.rs（原类型；outbox/subscribed 原来就有）
GraphwarEffectManager       # graphwar_effect_manager.rs（原类型；outbox/subscribed 原来就有）
```

新增的取回口（都走 `get_module::<M>()`，不是查表）：`effect_manager()` / `entity_effect_manager()`；
另外 4 个的取回口**§41 就有**（`turn_manager()` / `player_manager()` / `team_manager()` / `mode_manager()`），
本轮只是把它们**真用起来**（此前只有一条"取回来读一个字段"的断言）。

`ModeManager` 的 32 字节布局（`观测`，`0x5186ff`–`0x51870b`，管理体基址 = `rax+0x18`）：

```text
+0x00 弱自句柄（Rc 盒基址）      +0x08 事件队列 Rc        +0x10 容器 Weak
+0x18 word 0x0301（两个模式字节 0x01 / 0x03）    +0x1a 0（第三字节）
合计 0x1b = 27 → 补齐 32，与 §41.2 的 vtable size **一字不差**
```

⚠️ 本层只保留"三个模式"的语义，`+0x00 / +0x08 / +0x10` 那三个借用字段**不复原**（我方形状）。

### 50.4 为什么 `GameStateManager` 这一轮没升档（**禁区**）

`Wired::GameStateManager` 在容器里装的是 **`game::Match`**（`src/game.rs`），而任务书 §5 的 ❌
明确写着"不许改 `src/game.rs`"（有人正在 `execute_results` 上干活）。核对结果：

```text
$ grep -n "outbox\|subscribed" src/game.rs      →  零命中
```

`Match` 既没有订阅表也没有发件箱，往它身上加"读 7 条订阅 / 写发件箱"**必须改 `src/game.rs`**。
两条退路都被否定：

* **在 `root_wiring` 里加一层壳**（`GameStateSlot { layout: Match, table: … }`）——会改掉容器里的类型，
  `container_game_state_soldiers()` / `assemble_check.gd` 的既有断言全部要跟着改（禁区：既有断言）；
* **另起一张旁表**——那不是"这一槽接了事件流"，只是把假数据换个地方放，属于"把猜测写成行为"。

⇒ `GameStateManager` **明确留在 `assembled`**，缺的东西写在 §50.0 第 1–2 行。
替补进本轮名单的是 **`graphwar_effect_manager`**（同样 72 B、同样 `assembled`、订阅 1 条 `tick_event`
有 `0x519ca5` 的指令证据、发 `EffectInfo`(44) 有模块文档 §3 的 `逐位` 结论）。

### 50.5 headless 那侧：38 条新交互读数

`manager_probe_selftest()`（§44.5 的**我方自造**观测口）现在返回 **65 条**
（`27 + 38`），既有 27 条的**字符串一个字都没变**（新 38 条追加在末尾）：

```text
turn.soldier=9023  turn.flip=false  turn.emitted=1  turn.subs=2  turn.removed=true  turn.present_after=false
player.conn=9025  player.emitted=1  player.subs=2  player.replayed=3  player.cascade=true  player.outbox=5
team.added=Team2  team.counts=1,2  team.pick=Team1  team.subs=2  team.replayed=3  team.removed=1  team.outbox=5
mode.axis=2  mode.replayed=3  mode.restored=true  mode.outbox=5  mode.subs=1
entity_effect.kind=1  entity_effect.emitted=1  entity_effect.subs=2  entity_effect.replayed=1
entity_effect.removed=true  entity_effect.after=true  entity_effect.outbox=2
effect.kind=0  effect.emitted=1  effect.subs=1  effect.due_early=0  effect.due_at=1  effect.gone=true  effect.outbox=1
```

三条设计约束（都写进了代码注释）：

1. **幂等**：固定 id（`9021` / `9024` / `9026` / `9027` / `9028`）+ 末尾把临时记录删掉 / 复位
   ⇒ 连调两次返回**同一串**（`assemble_check.gd` 就是这么比的）。
   `TurnManager` 那条特别处理：`begin_turn` 会**推进游标**，所以末尾用
   `handle_entity_removed_event` 把那条玩家记录删掉，第二次调用仍从游标 0 开始 ⇒ 读数相同。
2. **不碰别人的表**：`Match` / `LifeState` / `TargetManager` / `ObstacleManager` / `FunctionTable` / `TickHandler`
   一个字都没动；新 6 个用的都是**临时 id**（`9021`–`9028`），调度完就删。
3. **值是真算的**：`root_wiring.rs` 的 `the_new_read_back_values_track_the_written_state`
   换输入（换射手名单 / 换模式 / 换 `start_tick`），读回值跟着变 ⇒ 不是 `return` 常量。

### 50.6 验收（本节实测）

```bash
export CARGO_HOME=<WS>/.cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/mgr2/target
cargo test --offline
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
"$GODOT" --headless --path client --script res://tools/assemble_check.gd
```

| 命令 | 本节实测 | 基线（master `c798eccf`） |
|---|---|---|
| `cargo test --offline` | **1620**，全绿 | **1585**（本节 **+35**）。构成：新增单测 **19** 条（`turn` 5 / `team` 3 / `player` 3 / `mode` 3 / `effect` 2 / `entity_effect` 3）+ 它们在 `#[path]` 集成测试里被**重复编入**的 16 条（`cargo test` 按目标分别计数：`effect` 3 份、`mode` 2 份、`team`/`player`/`entity_effect` 各 2 份）。另 3 条 `root_wiring` 的只在 `--features gdext-bindings` 下跑，**不计入**这 35 |
| `assemble_check.gd` | **213 passed / 0 failed**，退出码 0 | **203**（本节净 +10） |
| `plane_on_screen_check.gd` | **96 passed** | 96（未动） |
| `obstacle_drawer_check.gd` | **116 passed** | 116（未动） |
| `function_drawer_check.gd` | **132 passed** | 132（未动） |
| `entity_drawer_check.gd` | **72 passed** | 72（未动） |
| `python3 tools/coverage_matrix.py --check` | 需重跑（`src_fns` 变）→ 已重跑：`src_fns` **2504 → 2553**，527 行未变 | OK |

**三条驱动轴（分开跑、分开报，`AGENTS.md` 合并纪律 C；快照 = `.agent-work/replay-snapshot`，
401 个文件 / 286 个可读 / 1080 个结算批）**：

| 轴 | 命令 | 本节实测 | `fixtures/guardrails.json` | 判 |
|---|---|---|---|---|
| `fire` | `replay_attrib --drive fire --policy default` | 批次 1080；内容 **647**；种类序列 **1075**；死亡 **366/385**；挖洞 **207/294** | 1080 / 647 / 1075 / 366 / 385 / 207 / 294 | **7/7 一致** |
| `batch` | `replay_attrib --drive batch --policy default` | 批次 1080；内容 **647**；种类序列 **1075**；死亡 **366/385**；挖洞 **207/294** | 同上 | **7/7 一致** |
| `batch_conv` | `batch_conv --policy strict` | 本节**没跑**（本轮零几何改动；上一轮 §49 的记录是 1074/595） | 633 / 1075 | 未复核 |

⇒ 本节只动"接线层"（订阅表 / 发件箱 / 取回口 / 断言），**一条几何、弹道、结算代码都没碰**，
两条轴逐数字不动，符合"接线层不该动几何"的预期。

### 50.7 仍是推断 / 未知（诚实清单）

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| `TurnManager::send_all_info_events`（`0x462fe0`）的**载荷** | **推断**（明确标"我方形状"） | 该函数里只有一条 `add_event`（`0x46306a`）可确证；载荷构造没解，而且它**不**调用 `get_current_turn_players`(`0x463340`) / `get_current_turn_soldiers`(`0x463460`)（`addr_probe.py callers` 两处都没有 `0x462fe0` 的调用点）⇒ 本层按"每人一条 `TurnInfo{player, 当前射手}`"发，**别当证据用** |
| `TeamManager` 的发件箱用 `Vec<TeamEventKind>`（`u16` 判别值）而不是 `Vec<Event>` | **我方选择**（有硬约束） | `tests/graphwar_team_manager.rs` 用 `#[path]` **单独**把本文件编进测试二进制（不带 `src/events.rs`），它的文件头约定"只依赖 `std`"。为一行 `use` 去改那个文件不值得；判别值是同一个数（24 / 25）。另外 5 个的测试带 `events.rs`，所以它们用 `Vec<Event>` |
| `TurnManager::handle_entity_removed_event` 的 key 空间 | **推断** | 二进制把 `event+8` 直接当本表 key 用（`0x4635a5`），"实体 id = player_id"这条与 `graphwar_player_manager` §5 的同一个推断同源 |
| 6 个 manager 各自订阅**闭包体内**的逐条语义 | 部分 `观测` | `entity_effect`（§46 已有 `:67:30` / `:77:18`）与 `player`（§3/§4 `逐位`）到了"处理器 = 谁"这一层；`turn` / `team` / `mode` 只到"跳板 → 哪个函数"，闭包体内的边界检查/载荷拼接没逐条读 |
| `ModeManager` 的 `+0x18` 两个模式字节 | **部分观测** | `word 0x0301` 是**初值**（`0x51870b`），两个字节与 `AxisMode`/`FunctionMode`/`TimeMode` 三个枚举谁对谁**没逐条核**（本层按语义放三个字段，不按字节摆） |
| `GraphwarGameStateManager` 的 7 条订阅落码 | **未知**（禁区） | 见 §50.4：`src/game.rs` 不许改 |
| `batch_conv` 轴 | **未复核** | 本轮没跑；本轮零几何改动，理由与 §49 相同 |

### 50.8 22 个槽的档位（任务 40 之后）

| 档位 | 槽 | 依据 |
|---|---|---|
| `wired`（**10**） | `TickHandler` / `TargetManager` / `LifeManager` / `ObstacleManager`（§44.2）+ `TurnManager` / `PlayerManager` / `TeamManager` / `ModeManager` / `EntityEffectManager` / `EffectManager`（§50.2） | 订阅/发事件两个方向都指到指令 + `interaction_selftest` 各自的 4/5/5/7 与 6/6/7/5/7/7 条 |
| `layout`（2） | `ClientConnectionHandler`（§47.2）/ `FunctionManager`（§46.2） | 布局逐字段解出，行为没做完 |
| `assembled`（**9**） | `LocalPlayerManager` / `SoldierManager` / **`GameStateManager`（§50.4）** / `GraphicsManager` / `ComputerPlayerManager` / `FunctionHistoryRecorder` / `NameManager` / `ColorManager` / `RankManager` | 容器里是我方复原类型，但没接事件流 / 没有最小交互断言 |
| `stub`（1） | `PlanePosManager` | 24 B 布局未知 |

`SlotKind`（容器类型来源）仍是 **17 + 5 = 22**，一行没变。
`interaction_selftest` 覆盖 **11 / 22** 个槽（§44 的 5 + 本节的 6）。

## 51. `EntityDrawer::draw` 补上了：准备圈的 3 段弧 + 3 个外凸箭头真的发到了引擎（2026-11，任务 33）

**一句话**：`src/_scaffold/entity_drawer_class.rs` 的 `_draw` 从"一个 `draw_*` 都不调"变成
"**每具准备态士兵发 6 条图元**"——3 次 `draw_arc`（三段弧）+ 3 次 `draw_polygon`（外凸箭头），
实参逐条对 `docs/archive/140-plane-visual.md` §2 的立即数；验收脚本
`client/tools/entity_drawer_check.gd` **72 → 118** 条断言。
**但"用户能在屏幕上看见准备圈"仍然没做到**——卡的是**触发**，不是绘制，见 §50.0 第 1 条。

### 51.0 先说没做到的（免得被下面那张表误导）

| # | 项 | 现状 | 档位 |
|---|---|---|---|
| 1 | **画面上能看见准备圈** | **不能**。`_draw` 只画 `prep` 表里的实体（= 调过 `set_ring()` 的），而**播放屏一处都没调它**：原版填这张表的是 `ready` 里订的 `pos_info_event`（`GraphwarBackend` 没接），任务 33 的允许改动清单**不含播放屏逻辑** ⇒ 触发留到 §50.6 那三行改法 | **缺口**（绘制侧是观测） |
| 2 | `PosInfo` 的**单位** | `docs/140` §2.2 只读到"来自士兵坐标"，`[rsp+0xb0]/[rsp+0xa8]` 是不是已是画布像素**没证**。本类按**棋盘坐标**处理：复用 `sprite_transform` 那条投影（`k = size.x/50`）+ 半径乘同一个 `k` | **推断**（差一条证据；换 `k = 1.0` 就改完） |
| 3 | 环与箭头的**颜色来源** | 原版从哈希表取打包 RGB（`0x4b347a` 的三字节）；本类用导出属性 `team_color` | **我方接线**（队色表没接，§49.5 第 7 条） |
| 4 | 弧与箭头**是否互斥** | `entity_drawer::entity_primitives` 两处分支条件（`0x4b32e2 test %r14b` / `0x4b34c3 test %bl`）的哈希表语义**没解**⇒ 本类按"同一具士兵两组都画"发 | **未知**（`tests/entity_drawer.rs` 早就这么记着） |
| 5 | 驱动量 `V` 是什么时钟 | `0x4b2e10 call 0x519530`（`get_fractional_tick`）的返回值；本类由调用方给 | **观测**（算式）/ **我方接线**（谁每帧喂它） |
| 6 | `z_index` | 环画在 `EntityDrawer` **自己的** canvas item 上、三个 `Sprite2D` 是它的子节点 ⇒ 环在士兵**下面**（与原版同一棵树、同一种关系）；`z_index` 初值仍**未知**（§43.6(d) 第 6 条） | 结构=**观测**；`z_index`=**未知** |

### 51.1 本轮动了哪些文件

| 文件 | 改了什么 |
|---|---|
| `src/_scaffold/entity_drawer_class.rs` | `IControl::draw` 实现（6 条图元）；`prep` 表 + `PrepRing`；`set_ring` / `set_ring_base` / `clear_ring_base` / `clear_ring` / `ring_count`；观测口 `draw_call_count` / `drawn_arc_total` / `drawn_polygon_total` / `last_arc_count` / `last_arc` / `last_polygon`；纯函数 `ring_center_px` / `ring_canvas_scale`；`process` 末尾 `queue_redraw()`；`remove_entity` 顺手清 `prep`；**3 条新单测** |
| `client/tools/entity_drawer_check.gd` | 新增第 13 节（准备圈）**46 条**断言 + `wanted` 方法表 +11 条（72 → 118） |
| `client/src/screens/play_screen.gd` | **只改那段注释**："准备圈丢了"→"几何已复原、触发还没接（附最小改法）" |
| `client/tools/selftest.gd` | 同上，对应说明就地更新（**断言条数不变**） |
| `docs/16-client.md` | 本节 + §49.0 / §49.5 第 1 条 / §49.8"准备圈的几何"那一行就地更正 |
| `docs/coverage-matrix.tsv` + `docs/149-coverage-matrix.md` | 生成物（`src/` 变了必须重跑）：`src_fns` 2504 → **2522**，树哈希随之变 |

**没动的**：`src/entity_drawer.rs` 的算术（一个字没改）、`src/game.rs`、其它 `*_class.rs`、
`client/src/**` 的其它逻辑、`dispatch/`、`fixtures/guardrail*.json`。

### 51.2 为什么是这 6 条图元（地址链沿用 `docs/140` 的观测，本轮没重新扒）

| 原语 | 原版调用点 | 包装 / GOT 槽 | 本类 |
|---|---|---|---|
| `draw_arc` ×3 | `0x4b366c` / `0x4b36fd` / `0x4b378a`（全在 `EntityDrawer::draw` = `0x4b2890` 内） | 包装 `0x8abca0` ← GOT 槽 **`0x9bc020`**（`R_X86_64_RELATIVE` 写入） | `base.draw_arc_ex(...).width(0.3).antialiased(false).done()` |
| `draw_polygon` ×3 | `0x4b30b7` / `0x4b30e8` / `0x4b3119`（三次 `call 0x4b9a70` = `draw_offset_triangle`） | `0x4b9a70` 末尾 `lea "draw_polygon"(0x1c2bad)` + `call 0x3a01a0`（gdext 通用 `call` 通道） | `base.draw_polygon(&points, &colors)`（3 个顶点 / 3 份同色） |

`Primitive` 只有 `Arc` 与 `Polygon` 两个变体 ⇒ **`draw_polyline` / `draw_colored_polygon` 一条都用不上**
（原版也没在这条路径上用它们：`"draw_polyline"` 这个字符串在整份 `.so` 里**不存在**，`docs/140` §4.5）。

顺序取 [`entity_drawer::entity_primitives`] 的返回值 = **箭头在前、弧在后**（代码地址序）。

### 51.3 `_draw` 里只有两步是"接线"，算术一个字没动

```text
                        ┌─ entity_drawer::entity_primitives(EntityRingInput) ─┐   ← 已复原的算术
A_i = V/120 + {0, 2π/3, 4π/3}                                               │
H   = min(π/6 + (V−map[i])/60, 1.07337748998)                               │
r   = f32(pos_radius) × 1.5f32                                              │
P_k = C + {r, r+0.5, r+0.5}·(cos, sin)(θ ∓ 6°)                              │
                        └────────────────────────────────────────────────────┘
                                   ↓  本文件只加两步
① 坐标：center_px = ring_center_px(size, packed, board)   ← 直接复用 sprite_transform 的 position
        r_px      = r × ring_canvas_scale(size)           （k = size.x / 50）
② 颜色：Color::from_packed_rgb(team_color)
```

* **角度不做镜像**（`team_mirror` 的符号只进投影的 `x`）：原版 `draw_arc` 的起止角就是
  `A_i ∓ H`，证据里没有镜像那一步（**观测**：`subsd` / `addsd` 之后直接 `cvtsd2ss`）。
  本类不替它加。
* 三段的 A 相差 `2π/3` ⇒ 只要 `A0` 是对称的（`V = 0`），镜像前后是同一组弧；`V ≠ 0` 时
  两者差一个相位，这条差异在 §50.6 里登记为**未知**（原版有没有在别处补镜像，没证据）。

### 51.4 验收脚本里新增的 46 条断言（**这一段代替截图**；`--headless` 没有渲染器）

契约（`entity_drawer_check.gd` 第 13 节）·控件 `500×300`、棋盘 `(5, 5)`、`pos_radius = 2.0`：

| 断言 | 实测值 | 对应立即数 |
|---|---|---|
| 没调 `set_ring` 时**一条都不发** | `drawn_arc_total` 不变 | `prep` 空表（原版哈希表为空） |
| 每次 `_draw` 发 **3 条 `draw_arc`** | `arcs_delta == 3 × draws_delta` | `0x4b366c/0x4b36fd/0x4b378a` |
| 每次 `_draw` 发 **3 个 `draw_polygon`** | `polys_delta == 3 × draws_delta` | 三次 `call 0x4b9a70` |
| 圆心 | `(300.0, 100.0)` | 棋盘 `(5,5)` 的投影，`k = 10` |
| 半径 | `30.0` | `pos_radius × 1.5 × k`（槽 `0x1b9b98` = `1.5f32`） |
| 第 1 条弧 start / end | `−0.52359879016876` / `+0.52359879016876` | `A0 ∓ H`，`H = π/6`（槽 `0x19f2e8`） |
| 第 2 / 3 条弧中分角 | `2.0943951023931953` / `4.1887902047863905` | 槽 `0x19f9e8` / `0x19fbf8` |
| 折点数 | `10` | `mov $0xa,%esi` |
| 线宽 | `0.30000001192093` | 槽 `0x1bb2b4` = f32 `0x3e99999a` |
| 抗锯齿 | `0.0`（false） | `xor %ecx,%ecx` |
| 颜色 | `(0.49019607901573, 0.40784314274788, 0.0, 1.0)` = `#7D6800` | `from_packed_rgb(0x00687d)`（低字节 = R） |
| 张开态单段张角 | `2×π/6` = 60°（三段合计 180°，画面上是断开的） | `0x4b3594 addsd` |
| 闭合态单段张角 | `2×1.07337748998` = 123°（三段合计 369° ⇒ 必成整圈） | `0x4b359c minsd` |
| 闭合态中分角 | `0.5` | `V/120`（槽 `0x19f620` = `120.0`） |
| 箭头尖端 | `(330.0, 100.0)`，距圆心 `30` px | `P0 = C + r·(cos θ, sin θ)`（`0x4b9b13`） |
| 箭头底边两点 | 距圆心 `35` px（`r + 0.5`） | 槽 `0x19f6d0` = `0.5` |
| 箭头底边张角 | `12°` | 槽 `0x19fb68` / `0x19fc80` = `±0.20943951023931953` |
| **关掉准备态**（`clear_ring`）之后 | 弧与多边形都不再增加、`last_arc_count() == 0` | `prep` 表删除（原版两处 `test` 分支，语义未知） |

⇒ **"画面上有没有圈"这条，用上表第 2–3 行（每次 `_draw` 恰好 3 弧 + 3 多边形）与
圆心/半径/角度那几行代替截图**：`_draw` 被引擎调到过（`draw_call_count` 每帧 +1），
每次调用的实参就是这些数。**截不到图不等于没画**，但也不等于"画得像原版"（§50.6）。

### 51.5 验收（本节实测，可从干净树重跑）

```text
export CARGO_HOME=<WS>/.reference/cargo-home CARGO_TARGET_DIR=<WS>/.agent-work/cg/target33
cargo build --offline --features gdext-bindings
cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so   # 不覆盖就还在跑旧内核
grep -a -c line_instance_id client/libgw2_rebuild.so                    # 3
"$GODOT" --headless --path client --import                              # 新增 .gd/bindings 后必须重扫
"$GODOT" --headless --path client --script res://tools/entity_drawer_check.gd   # 118 / 0（基线 72）
cargo build --offline --example serve_match
export HOME=<假 HOME>                       # selftest 读 $HOME，不走 XDG_DATA_HOME
mkdir -p $HOME/.local/share/godot/app_userdata/Graphwar
cp -r <replays 快照>/replays $HOME/.local/share/godot/app_userdata/Graphwar/
"$GODOT" --headless --path client -- --selftest                         # 709 / 0
python3 tools/coverage_matrix.py                                        # 生成物重生成
python3 tools/coverage_matrix.py --check                                # OK
python3 tools/check_doc_refs.py                                         # 退出码 0
```

| 口径 | 改前（`CG@e846c15` + `3d1e504`） | 改后 | 退没退 |
|---|---|---|---|
| `entity_drawer_check` | 72 passed / 0 failed | **118 passed / 0 failed** | +46，全是准备圈 |
| `assemble_check` | 203 | **203** / 0 | 没退 |
| `plane_on_screen_check` | 96 | **96** / 0 | 没退 |
| `obstacle_drawer_check` | 116 | **116** / 0 | 没退 |
| `function_drawer_check` | 132 | **132** / 0 | 没退 |
| `draw_classes_check` | 235 | **235** / 0 | 没退 |
| `curve_on_screen_check` | 96 | **96** / 0 | 没退 |
| `selftest`（assets + 回放样本 + `serve_match`） | 709 / 0 failed | **709 / 0 failed** | 没退（断言条数不变） |
| `cargo test --offline` | 1585 | **1585** / 0 | 没退（`_scaffold` 只在 `gdext-bindings` 下编译） |
| `cargo test --offline --features gdext-bindings` | 1669 | **1672** / 0 | +3（新单测） |
| `coverage_matrix --check` | OK | **OK**（`src_fns` 2504 → 2522） | — |
| `check_doc_refs.py` | rc = 0 | **rc = 0** | — |

### 51.6 仍是推断 / 未知 + 下一步

**触发接线：已做**（2026-11-08，**协调者接力**，任务 33 的允许清单不含播放屏逻辑）。

`play_screen.gd` 的 `_sync_entities()` 末尾现在会填 `prep` 表：只给"本回合行动的士兵"一条
（`set_ring(id, p.x, p.y, GwCore.SOLDIER_HIT_RADIUS, _curve_tick)` + 首次出现时 `set_ring_base`），
其余实体 `clear_ring(id)`；`_ring_base` 表记每具士兵的起始量。

⚠️ 这条接的是**我方选择**，不是观测到的原版路径，三处都是选定的：
- **谁填表**：原版是 `GraphwarBackend` 的 `pos_info_event`（还没接）⇒ 这里按回合状态代填；
- **`pos_radius`**：用 `GwCore.SOLDIER_HIT_RADIUS` = 0.5（`PosInfo.radius` 恒 0.5，`docs/131`）；
  若证实 `PosInfo` 的第 3 个 f64 已是像素，则 `k` 要换成 1.0（见 §50.0）；
- **`V`**：用播放屏的 `_curve_tick`（同一个喂给 `GraphwarPlane.set_fractional_tick` 的时钟）。

自检加了 3 条端到端断言（`selftest` 709 → **712**）：`EntityDrawer` 在树里、`prep` 表里恰 1 条、
`set_ring`/`clear_ring` 两个接线口都在。

| 项 | 档位 | 缺什么证据 |
|---|---|---|
| 触发（谁在什么时候把士兵标成准备态） | **未知** | `pos_info_event` 的载荷语义 / `draw` 里两处 `test` 分支的条件；本轮只做出"填了就画"的机制 |
| `PosInfo` 三个 f64 的**单位** | **未知** | 需要运行时读 `pos_info_event` 的真值（`docs/140` §3.1 那 14% 与它同源） |
| `pos_radius` 的**物理含义** | **未知** | `docs/140` §0 第 ⑧ 条：环半径只能从"实测外径/内径 = 1.4240"反解到 `R/W = 2.859` |
| 弧/箭头两组分支是否互斥、`V` 的相位是否要镜像 | **未知** | 哈希表语义；本类按"都画、不镜像"实现 |
| 画出来的样子与原版是否**逐像素**一致 | **未知** | `--headless` 视口 64×64、无渲染器，本轮**没做**有限窗口截图（§43.5 的办法） |
| `z_index` / 队色表 / 淡出 | **未知** | §43.6(d) 第 4–7 条，与本轮无关，仍然缺 |

## 52. 把原版视图脚本真正换进 `client/`：换 24 个 / 跳 15 个（2026-11，任务「gdswap」）

> **本篇解决什么问题**：`docs/16` §39 只回答了"这些原版 `.gd` **能不能**跑"，本篇回答
> "**换不换得进去**"，并且已经换完、跑过全量验收。
> 换进去的是**原版脚本的逐字节副本**（`client/src/views/*.gd` ← `recovered-src/*.gd`，`cmp` 逐个相同），
> 挂载点是 `client/src/ui/gw_views.gd`，被换掉的**仿制品**是各屏里那套自造的字号/尺寸算式
> （`GwScreen._apply_size_recursive()` 的 `maxi(10, 宽/N)` 与各屏自己的 `apply_font_scale()` 尾巴）。

### 52.1 先复现那份"40 个可直接换"的清单：**口径对不上**（就地更正）

三条**观测**，都能用一条命令复现：

1. **"40 个"是行数，不是文件数。** §39.3 的表有 **40 行**，但只有 **39 个不同文件名**
   —— `ViewFunctionsLabel.gd` 在两处出现（表里第 21 行与第 40 行，两行内容相同）。
   `sed -n '/^### 39\.3/,/^### 39\.4/p' docs/16-client.md | grep -oP '^\| `\K[A-Za-z0-9_]+\.gd' | sort -u | wc -l` → **39**。
   所以 §39.3 的"40 个 / 820 行"应按 **39 个文件**读（那份 820 行里 `ViewFunctionsLabel.gd` 的 25 行被数了两次）。
2. **§39.3 的判据和本任务的判据不是同一件事。** §39.3 说的是"**这个原版脚本单独挂进 headless 树能不能跑、
   有没有可观测副作用**"（`client/tools/recovered_view_check.gd` 的 B/C 段）；本任务的判据是
   "**`client/` 里有没有同名仿制品、接口对不对得上**"。
   按后者复现（`find client -name <那 39 个文件名>`，排除 `client/tools/recovered/` 这份试验副本）：
   **只有 1 个命中** —— `client/src/screens/endgame_pop_up.gd`。
   而它**不是**那个脚本的仿制品：原版 `endgame_pop_up.gd` 是 9 行、`_ready` 空的 `Control` 视图脚本；
   我们这个是 205 行的 `EndGamePopUp` 控制器。**同名，不同角色。**
3. 由此得到的**真接缝**：不是"同名文件"，而是"**同名同类型的节点**"。我们这半边本来就不是按原版文件名
   组织的（界面是 `client/src/screens/*.gd` 里照 `.tscn` 逐节点搭出来的），而各屏的注释里**早就写好了**
   要挂哪个原版脚本（`main_menu.gd` 的节点表上就标着"← 挂 TitleSize.gd"）。
   ⇒ 本节按"**原版 `.tscn` 的节点路径 → 我们树里的同名同类型节点**"执行，判据三条：
   ① 能从原版 `.tscn` 里读出该脚本挂在**哪个节点**（路径 + 类型）；
   ② 我们树里有**同名同类型**的节点；
   ③ 挂上之后 `client/tools/*.gd` 全量 + `selftest` 一条不退。
   原版侧的对应关系这样取（**观测**，可复跑）：解析 `recovered-src/*.tscn` 的
   `[ext_resource type="Script" …]` + 每个 `[node]` 的 `script = ExtResource(…)`，按 `parent=` 累路径。

### 52.2 换了哪些：**24 个文件 / 79 个运行实例**

`client/src/views/` 里 24 个 `.gd` 全部与 `recovered-src/` **逐字节相同**
（`for f in client/src/views/*.gd; do cmp "$f" "recovered-src/$(basename $f)"; done` ⇒ 24 个全过），
运行时实例数由一次 headless 遍历数出来（**观测**：实例化 `scenes/main.tscn`、等 6 帧、
对每个 `GwViews.owns(node)` 的节点按脚本名计数）：

| 原版脚本 | 实例数 | 原版 `.tscn` 里挂在哪个节点 | 我们挂在哪儿 |
|---|---:|---|---|
| `MarginContainer.gd` | 7 | 8 个界面的**根** `MarginContainer` | `GwUI.frame()`（main_menu 自己挂） |
| `ExitButtonContainer.gd` | 14 | 14 个场景的 `ExitButtonContainer` | `GwUI.exit_pair()`（与 §39.3 "场景里用了 14 次"吻合） |
| `LanLobbyTitle.gd` | 11 | 标题/队名/开关外层 `MarginContainer` | `GwScreen.build_header()`、game_setup 的 3 个角标 + 2 个队名、lan_lobby 的 Local/Global、player_screen 的 NAME |
| `LanLobbyFontResize.gd` | 4 | 表头四列 / 房间行四列 | lan_lobby 的 `_header_cell()` 与 `_room_entry()` |
| `IpPopUpMargin.gd` | 18 | 弹窗里的文案行/按钮壳（margin 10 + 字号 宽/40） | 五个弹窗的 `_ip_margin()`、player_screen 的输入框外层、join_level 的说明与两键 |
| `MarginContainerPopUpBack.gd` | 6 | 6 个弹窗的安全区层 | confirm_exit / connect_popups / endgame / join_level 的 overlay（add_bot 那条见 §52.3） |
| `MainMenuButtonResize.gd` | 2 | 主菜单两列 `VBoxContainer` | main_menu 左右两列 |
| `TitleSize.gd` | 1 | 主菜单标题外层 | main_menu 标题外层（字号 宽/8、子节点最小宽 = 整屏宽） |
| `VersionLabel.gd` | 1 | 主菜单版本号外层 | main_menu 版本号外层（宽/60） |
| `FireContainer.gd` | 1 | play_screen 的 `FireContainer` | play_screen 的同名节点（子节点最小宽 = 宽×0.07） |
| `TimerContainer.gd` | 1 | play_screen 的 `TimerContainer` | 同上（字号 宽/30） |
| `FxContainer.gd` | 1 | play_screen 的 `FxContainer` | 同上（字号 宽/60） |
| `OffsetContainer.gd` | 1 | play_screen 的 `OffsetContainer` | 同上（最小宽 宽×0.09、字号 宽/60） |
| `RoomName.gd` | 1 | GameSetup 的 `RoomName` HBox | game_setup 的 `RoomName`（字号 宽/30） |
| `LobbyButtonContainer.gd` | 1 | lan_lobby 底栏 `HBoxContainer` | lan_lobby 底栏（每个子节点宽 = 屏宽×0.2、字号 宽/30） |
| `PlayerTabContainer.gd` | 1 | player_screen 的 `TabContainer` | 同名 `TabContainer`（三个页签标题 + 字号 安全区宽/30） |
| `PlayerNameLineEdit.gd` | 1 | `PlayerNameLineEdit` | 同名 `LineEdit`（原版就是空壳脚本） |
| `ShowcaseMarginContainer.gd` | 1 | 展示块外层 | 同名节点（四边 margin 5） |
| `UnlockButtonSize.gd` | 1 | `SkinBlock/…/HBoxContainer` | Unlock 那一行（字号 宽/30） |
| `ChatContainer.gd` | 1 | chat_pop_up 的 `ChatContainer2` | 同名 `VBoxContainer`（字号 宽/50） |
| `ChatBox.gd` | 1 | chat_pop_up 的 `ChatBox` | 同名 `RichTextLabel`（点一下把焦点给 `../ChatLineEdit`） |
| `HBoxVictoryIcon.gd` | 1 | endgame 的胜利者图标行 | 同名 `HBoxContainer`（separation = −宽/10） |
| `LevelTitleLabel.gd` | 1 | join_level 的 `LevelTitleLabel` | 同名 `Label`（字号 宽/30） |
| `LevelDescriptionScrollContainer.gd` | 1 | join_level 的说明 `ScrollContainer` | 同名节点（子节点字号 宽/40） |
| **合计** | **79** | | |

**怎么证明它真在跑**（不是只 `load()` 成功）：headless 视口恒为 **64×64**，所以原版算式给出的就是
`int(64/N)`。实例化主场景后逐节点读回来（**观测**，本轮探针，已删除，命令形状见下）：

```text
TitleSize        → Label      font_size = 8   （64/8，我们的旧实现是 maxi(10,8)=10）
MainMenuButtonResize → 4 个按钮 font_size = 2 （64/30，旧的 10）
VersionLabel     → Label      font_size = 1   （64/60）
LobbyButtonContainer → 4 个子节点 custom_minimum_size = 12.80（64×0.2）
LanLobbyFontResize   → font_size = 1（64/40）；LanLobbyTitle → 2（64/30）
FireContainer    → 子节点 min = (4.48, …)（64×0.07）；OffsetContainer → min 5.76（64×0.09）、字号 1（64/60）
IpPopUpMargin    → 字号 1（64/40）；ChatContainer → 字号 1（64/50）
```

⇒ **24 个文件里 24 个都有可观测的、与算式逐值吻合的效果**；没有一个是"挂上去等于没挂"。

### 52.3 跳了哪些：15 个（逐个理由）

| 原版脚本 | 为什么跳 | 档位 |
|---|---|---|
| `CampaingLevelsButtons.gd` | 它运行时 `GraphwarButton.new()`（**扩展类**）造 21 个按钮；我们的 21 个关卡按钮是 `GwButton`（负责把命令送进 `App`）。`selftest` 断言"Campaign 生成 21 个关卡按钮"（`child is GwButton`）与每个 `level_N` 的 `text`/`input_text` ⇒ 换上去类就变了、命令不再派发、断言全崩 | **观测**（读原版源码 + 读断言） |
| `ShopButtonsSize.gd` | `selftest` 断言格子边长 = `max(32, (安全区宽/2−50)/3)`（**32 这个下限是我们自加的**）；原版没有下限，headless 64 宽下算出来是 **−6** ⇒ 15 条断言会失败 | **观测**（原版无 `max`，`docs/16` §39.6 已记） |
| `GameSetupPlayerEntry.gd` | `extends MarginContainer`，原版是 `team1_entry.tscn` 的**根**；我们的玩家行是 `GwUI.hbox()` 现搭的 `HBoxContainer` ⇒ **类型不符**，`set_script` 直接被引擎拒绝 | **观测**（引擎报 `Script inherits from native type 'MarginContainer', so it can't be assigned to an object of type 'HBoxContainer'`） |
| `SoldierIconMarginContainer.gd` | 原版挂在 team entry 的 5 个图标 `MarginContainer` 上（负 margin = −宽/100）；我们行内图标是 `_icon_box()` 现搭的 `AspectRatioContainer` 链 ⇒ 没有同名同类型节点 | **观测** |
| `OptionsContainer.gd` | 公式对得上（`custom_minimum_size.y = 0.25×父高`），但**时序对不上**：原版只连 `self.resized`，我们的 `_update_options_height()` 另有"弹窗隐藏、父高为 0 时用真值兜底"这一支，而 `selftest` 正是在未布局时读这个值。换上去要先删兜底并重验 —— 本轮**没做**，登记为下一步 | **推断**（未实测，缺一次"删兜底后断言是否仍过"的运行） |
| `ButtonGrid.gd` | 4 个 `GridContainer`（左右键盘 + 两排回放键）。我们的 `_make_grid()` 是"先建 holder 并 `add_child`、再 `_fill_grid()` 填键"，键是**进树之后**才有的 ⇒ 要换必须把 `_make_grid()` 拆成"建 → 填 → 挂 → add"，属结构调整，本轮未做 | **观测** |
| `HBoxContainer.gd` | 同上（回放键那一行），且它的效果与 `ButtonGrid.gd` 完全重复（都是给子节点设 `0.15x/3` 的最小尺寸 + `x/30` 字号） | **观测** |
| `ViewFunctionsLabel.gd` | `extends RichTextLabel`，但 `update_child_sizes()` 是给**子节点**设字号/最小尺寸 —— `RichTextLabel` 没有 `Control` 子节点 ⇒ 挂上去等于空操作；它自己的字号原版根本没设 | **观测**（读原版源码） |
| `TurnLimitText.gd` | 原版挂在 `OptionsContainer/AspectRatioContainer4`（回合上限数字）；我们的 join_level 只有 4 个模式格，没有第 5 个读出格 | **观测** |
| `CampaingButtonSize.gd` | **没有任何 `.tscn` 引用它**（观测：31 个 `.tscn` 的 `ExtResource(Script)` 全解析出来，这 5 个名字一次都不出现）⇒ 原版是 Rust 控制器运行时 `new()` 的；client 里没有对应节点 | **观测** |
| `LevelTitleContainer.gd` | 同上 | **观测** |
| `OffsetLabel.gd` | 同上（它的字号/最小宽由 `OffsetContainer.gd` 写，后者**已换**） | **观测** |
| `ResizeContainer.gd` | 同上 | **观测** |
| `UnlockButton.gd` | 同上，且它 `extends GraphwarButton`（扩展类）—— 我们的 Unlock 按钮是 `GwButton` | **观测** |
| `endgame_pop_up.gd` | 唯一同名文件，但它是**我们的 205 行控制器**，不是那个 9 行视图脚本的仿制品（§52.1 第 2 条） | **观测** |

> 另外 `add_bot_pop_up.gd` 里那一处 `MarginContainerPopUpBack.gd` 也**跳了**：原版那个位置是
> `MarginContainer`，而我们的 `_band()` 一律建成 `HBoxContainer`（引擎实测拒绝挂载）。
> 该文件本身仍然算"换了"——它在 confirm_exit / connect_popups / endgame / join_level 四处有挂点。

### 52.4 两个真踩到的坑（都会**静默**失效，必须写下来）

1. **`set_script()` 在节点已经进树之后调用，`_ready` 不会补发。** 实测（headless，同一棵树里对照）：
   `add_child()` 之后挂 `MainMenuButtonResize.gd` 的 `VBoxContainer`，子 `Label` 字号停在主题默认
   **16**；先挂再 `add_child()` 的是 **2**（= 64/30）。原版脚本的 `_ready` 里才有
   `size_changed.connect(...)` 与首次 `update_sizes()` ⇒ 挂晚了这个脚本**一次都不会跑**，
   而且没有任何报错（只有"字号看起来还是旧的"）。
   现在 `GwViews.attach()` 会在 `node.is_inside_tree()` 时 `push_error`，把这个坑变成响亮的失败。
2. **子节点必须**先**就位。** 这些脚本的 `_ready` 都只遍历**当次**的 `get_children()`。
   先 `add_child(容器)`、再往里塞子节点 ⇒ 容器进树时子节点还是空的，脚本算的是空集。
   本轮的写法统一成"**建节点（不挂父）→ 填子节点 → `GwViews.attach()` → `add_child()`**"；
   少数子节点确实要在进树后才补的地方（`IpPopUpMargin` 的按钮壳、说明栏的 `TextEdit`），
   用 `call_deferred("update_child_sizes")` 复用**原版自己的**刷新函数（不是我们另写一份算式）。
3. 连带教训：**"换了"和"没换"在 headless 里看起来可以一模一样**（断言只看效果，不看谁写的）。
   所以本轮对每个挂点都做了"实例化主场景 → 逐个 owned 节点读回字号/最小尺寸 → 与算式逐值比对"的探针
   （§52.2 的 79 个实例就是这么数出来的）。

### 52.5 验收（本轮全量，命令都能原地复跑）

| 项 | 本轮值 | 说明 |
|---|---|---|
| `client/tools/*.gd` 全量 | **2018 passed / 0 failed / 12 个脚本 rc=0** | 与改动前**逐条相同**：`assemble 213`、`base_classes 117`、`curve_on_screen 96`、`draw_classes 235`、**`entity_drawer 118`**、**`function_drawer 132`**、`gdext 24`、**`obstacle_drawer 116`**、**`plane_on_screen 96`**、`recovered_view 57`、`signal_classes 155`、`spike 23` |
| `selftest` | **636 passed / 0 failed / 2 skipped** | ⚠️ **就地更正**：`docs/00-status.md` §0 与本轮任务书写的是 **709**，本机在 master（2e69ed5e）与本分支上**都**是 636（2 skip = 没编 `examples/serve_match`）。709 这个数**当前复现不出来**，改按实测 636 记 |
| `cargo test --offline` | **1620 passed / 0 failed / 1 ignored** | 与任务书一致（本轮没动 `src/`） |
| 三轴（分母 1080 批） | `fire` **647** 内容 / **1075** 种类；`batch` **647** / **1075**；`batch_conv` **637** / **1075** | 前两轴与任务书一致；`batch_conv` 内容命中实测 **637**（`docs/00-status.md` 记的是 633，差 4 批，未追因，就地记） |
| 延迟段（`batch` 轴） | 死亡集合 **366 / 385**（漏报 0）、挖洞点 **207 / 294** | 与任务书一致 |
| `stderr` | 与改动前**逐行相同**（12 个检查脚本 + selftest 全比过） | 没有新增 `SCRIPT ERROR` / `WARNING` |

复跑：

```bash
export XDG_DATA_HOME=<WS>/.agent-work/<topic>/xdg/data XDG_CACHE_HOME=… XDG_CONFIG_HOME=…
godot --headless --path client --import          # 加了 client/src/views/*.gd 之后**必须**跑
godot --headless --path client --script res://tools/entity_drawer_check.gd    # 12 个同理
godot --headless --path client -- --selftest
cargo test --offline
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive batch --policy default --json b.jsonl
cargo run --release --offline -q --example batch_conv    -- --dir <快照> --drive batch --policy strict --json c.jsonl
python3 tools/bit_exact_attrib.py --json b.jsonl
```

### 52.6 换来了什么、还没换来什么（与 §39.6 一致，别重复期待）

* **换来了**：这 24 个节点的**字号/最小尺寸/内边距**现在由**原版代码**给出，我们的
  `maxi(10, …)` 下限与各屏自己那份除数表已经删掉（`GwScreen._apply_size_recursive()` 见到
  `GwViews.owns(node)` 就整棵跳过）。**风险已落地**：小窗口下字号会真的变小（原版就是没有下限）。
* **没换来**：仍然**没有任何按钮行为**（那在 Rust 控制器里）。视图层"看起来一样"≠ 复原了行为。
* **没换来**：外观（字体/主题/`.svg`）没变 —— 本轮一个 `.tres`/资产都没动。

### 52.7 遗留 / 未做（下一步按这个顺序）

1. `OptionsContainer.gd`：删掉 `join_level_pop_up.gd` 里 `_update_options_height()` 的真值兜底，
   改挂原版脚本，重验 `selftest` 那条"`0.25 × 父高`"断言（本轮**未验证**）。
2. `ButtonGrid.gd` + `HBoxContainer.gd`：把 `play_screen._make_grid()` 拆成"建 → 填 → 挂 → add"，
   4 个网格 + 4 排回放键一起换。
3. `GameSetupPlayerEntry.gd` / `SoldierIconMarginContainer.gd`：需要先把玩家行改回
   `team1_entry.tscn` 的形状（根 = `MarginContainer`），这属于**结构**改动，不在"换脚本"范围内。
4. `CampaingLevelsButtons.gd` / `UnlockButton.gd`：卡在 `GwButton`（我们的派发层）与扩展类
   `GraphwarButton` 之间 —— 要么把命令派发下沉进扩展类，要么承认视图层用我们的按钮。
   这条与"控制器迁回 Rust"是同一件事。
5. **`selftest` 里 709 那个数**要查清（本机两处都是 636）；`batch_conv` 的 633 → 637 也要追一下差在哪 4 批。
