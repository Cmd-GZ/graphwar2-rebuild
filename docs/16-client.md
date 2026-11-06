# 16 · 客户端（Godot 4.7 重建）

> **本篇解决什么问题**：`client/` 这一半怎么来的、它和内核之间的接缝长什么样、
> 15 个界面各自对齐到什么程度、命中/弹道/障碍/音频/回放/联机在客户端里怎么接、
> 以及**哪些行为是我们自定的**（必须能一条条指出来，不许冒充原版）。
>
> ⚠️ **边界（先读）**：`client/` 是**照 `.tscn` 与真帧重写的仿制品**，不是原版源码。
> 原版的界面控制器是 Rust 模块（在 `.so` 里）。所以"看起来对"**不是**复原证据；
> 客户端自检条数也只是**回归保护**，不是复原证据。

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
