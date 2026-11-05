# 92 · 客户端界面台账（运行时枚举，15/15 界面）

状态：**现行**。本文件回答一个此前从未回答的问题：
**Graphwar II 的界面到底有哪些控件，每个控件是干什么的，我们实现了没有。**

数据来源不是反编译，而是**把原版游戏跑在我们自己的 harness 里，直接把节点树 dump 出来**
（`godot-live/`，见 `docs/22`）。全部 15 个界面都进过（每个都至少一次 `visible=1`，已逐项核对），33 个不同快照（39 个「日志 x 快照」组合），858–2118 个节点/快照。

复现一条命令：`bash gw2-rebuild/tools/dump_ui.sh`（见 §9）。

---

## 0. 一页结论

| 项 | 结果 | 证据 |
|---|---|---|
| 界面（`ScreensControl` 的直接子节点） | **15 个，全部进入过** | `@@SCR` 行，每个界面至少一次 `visible=1` |
| 场景 `.scn`（游戏本体） | 25 个；**22 个观测到实例化节点**，3 个未观测 | `scene=` 字段（§5） |
| 控件节点（启动态全树） | 858 个节点 / 283 个 `GraphwarButton` / 25 个 `Label` / 4 个 `LineEdit` | `boot_all_screens` 快照 |
| 挂到活动节点上的脚本 | 46 个（`.gdc` 共 105 个，其中 51 个非插件） | `@@N` 第 5 段 |
| `.gdc` 反编译 | **无可用反编译器**（离线检索无 gdsdecomp/GDRE/pip 包）；但我们解开了 `GDSC` 容器与**字符串池**（105/105） | §2 |
| 本仓库对界面的实现 | **0 个控件**——本仓库是 Rust 逻辑库（`GW2Rebuild` RefCounted），没有 Godot 客户端 | `src/gdext_class.rs` |

**最重要的一条结论（推翻了 `docs/88` §11–§12 的判断）**：

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

## 1. 方法：为什么用运行时 dump 而不是反编译

1. `.gdc` 不是"源码压缩包"，反编译需要专门工具（本机没有，且不许联网下载，见 §2.1）。
2. 但**节点树是运行时的东西**：游戏启动时就把 15 个界面全部 `instantiate` 好，只切
   `visible`。所以只要进入一次进程，就能把所有界面（包括没打开过的弹窗）的完整控件树读出来。
3. 于是路线是：`probe.gd`（autoload）按脚本化的"按键序列"进各界面 → 每到一个状态
   调 `find_children("*","",true,false)` 走一遍树 → 打印 `@@N|...` 行 → 离线汇总成表。

`tools/dump_ui.gd` 打印的字段：

| 字段 | 含义 |
|---|---|
| `path` | 从 `GraphwarRootNode` 起的完整路径（含运行时自动名 `@HBoxContainer@32`） |
| `class` | `get_class()`（`GraphwarButton` / `GraphwarPlane` 等是 Rust 注册的类） |
| `vis` | 两位：`visible` / `is_visible_in_tree` |
| `script` | 挂在该节点上的脚本 `resource_path`（`.gd`，实际资源是 `.gdc`） |
| `text` | `text` 属性（Label/Button/RichTextLabel/LineEdit），换行转义成 `\n`，`\|`→`¦` |
| `flags` | `disabled` / `toggle` / `checked` / `readonly` / `items=` / `minsize=` / `texture=`/`icon=` / `scene=` / `tip=` |

外加 `@@C|path|signal|method`（信号连接）、`@@CYCLE|...`（选项按钮连点枚举）。

**"按键"是什么**：`emit_signal("pressed")`。Rust 侧的按钮只靠这个信号工作，
所以脚本化点击与真点等价（`docs/26` 已验证过 FireButton 会真的发 `FunctionFireRequest`）。

---

## 2. 反编译 `.gdc`：没有可用的反编译器（诚实记录）

### 2.1 离线检索结论：**无可用反编译器**

| 检索项 | 结果 |
|---|---|
| `which gdsdecomp` / `gdre_tools` | 无 |
| `pip`（`.venv`）里的 godot 相关包 | 无 |
| `tools/` 目录 | 只有 Ghidra 与 Godot 二进制，无 GDScript 反编译工具 |
| 全盘 `find -iname "*gdre*" -o -iname "*gdsdecomp*"` | 无 |
| Godot 4.7.2 自带 `--help` | 无 `--decompile` 之类的选项（只有 `--gdscript-docs`，方向相反） |
| 网络 | **禁止**（本轮规则），因此不下载 GDRE Tools |

⇒ **本轮没有拿到任何 `.gdc` 的反编译源码**，下面 §2.2 是能做到的部分，明确标注不是反编译。

### 2.2 做到的：`.gdc` 是 `GDSC` + zstd 容器，字符串池可解

`tools/gdc_strings.py`（新工具，105/105 全部解开）：

```
偏移 0  "GDSC"
偏移 4  u32  字节码版本（本包为 101）
偏移 8  u32  解压后长度
偏移 12 zstd 帧（magic 28 b5 2f fd）
```

解压后是 GDScript 的 token 流；其中每个字符串是 `u32 长度` + `长度 × 4 字节`，
每 4 字节形如 `{u8 c ^ 0xb6, 0xb6, 0xb6, 0xb6}`（Godot 的字符串混淆常量 `0xb6`）。

**这给到的是标识符与字面量，不是语句。** 例：`ExitButtonContainer.gdc` →
`MarginContainer | _ready | get_tree | root | size_changed | connect | _on_viewport_size_changed |
update_sizes | screen_size | get_viewport_rect | size | font_size | x | node | get_children |
add_theme_font_size_override`。

⇒ 结论直接可读：**它不是"退出按钮处理器"，而是一个按屏幕尺寸重算子节点字号的容器**。
名字骗人，这一点很重要。

> **2026-11 补**：字符串池只是第一层。payload 的标识符表/常量表/token 流已全部解开
> （105/105、零残留字节），见 [`docs/101`](101-gdc-contents.md) 与 `tools/gdc_disasm.py`：
> 常量池、函数表（名字+参数个数）、语句级伪源码都能拿到。

### 2.3 字符串池给出的脚本分工（51 个非插件脚本）

| 类别 | 数量 | 例子 | 说明 |
|---|---|---|---|
| 纯布局/字号助手 | 34 | `ExitButtonContainer` `ButtonGrid` `LanLobbyTitle` `TitleSize` `TimerContainer` `MarginContainer`… | 全部是 `_ready → size_changed.connect → update_sizes`，无游戏逻辑 |
| 空壳（只有 `_ready`/`_process`/`delta`） | 6 | `endgame_pop_up` `UnlockButton` `ShowcaseMarginContainer` `LevelTitleContainer` `ExtraOptionsVbox`(0 字符串) | 什么都没做 |
| 有行为 | 5 | `CampaingLevelsButtons`（运行时 `GraphwarButton.new()` 造 21 个关卡按钮）、`ShowExtraButtons`（切换附加按钮 + `set_button_icon(load(...))`）、`ChatBox`（点聊天区 → `chat_line_edit.grab_focus()`）、`PlayerTabContainer`（`set_tab_title`）、`OptionsContainer`（`resized → call_deferred(set_minimum_size)`） | 都不是"游戏规则" |
| 广告/IAP 插件 | 6+ | `Ad_holder` `android_iap` `IAP_IOS` `AdContainer` `*_mediastore_holder` | 与对局无关 |

**所以"界面逻辑在 `.gdc` 里"（`docs/88` §12 的前提）不成立。** 本轮运行时探针的独立证据是：
282 个 `GraphwarButton` 上只有自连 `pressed → pressed`，**没有任何 GDScript 处理器**；
切屏、开火、加 Bot 全是 Rust。⇒ 想复原客户端，只能照界面契约重写，不能靠反编译脚本。

---

## 3. 全局骨架（观测）

```
GraphwarRootNode                        [GraphwarRootNode]  (Rust)
├── VBoxContainer
│   └── ScreensControl                  ← 15 个界面全在这里，靠 visible 切换
│       ├── MainMenu          [Control]  scene=res://main_menu.tscn
│       ├── PlayScreen        [Control]  scene=res://play_screen.tscn
│       ├── Campaign          [Control]  scene=res://campaign.tscn
│       ├── LanLobby          [Control]  scene=res://lan_lobby.tscn
│       ├── GameSetup         [Control]  scene=res://GameSetup.tscn
│       ├── ChatPopUp         [Control]  scene=res://chat_pop_up.tscn
│       ├── JoinIpPopUp       [Control]  scene=res://join_ip_pop_up.tscn
│       ├── ConnectingPopUp   [Control]  scene=res://connecting_pop_up.tscn
│       ├── DisconnectedPopUp [Control]  scene=res://disconnected_pop_up.tscn
│       ├── JoinLevelPopUp    [Control]  scene=res://join_level_pop_up.tscn
│       ├── PlayerScreen      [Control]  scene=res://player_screen.tscn
│       ├── ReplayLobby       [Control]  scene=res://replay_lobby.tscn
│       ├── AddBotPopUp       [Control]  scene=res://add_bot_pop_up.tscn
│       ├── EndGamePopUp      [Control]  scene=res://endgame_pop_up.tscn
│       └── ConfirmPlayExitPopUp [Control] scene=res://confirm_exit_pop_up.tscn
├── IAP_holder  /  Ad_holder  /  IOS_album_holder  /  Android_mediastore_holder
```

观测：`boot` 快照中 15 个界面**全部存在**，仅 `MainMenu.visible = true`。
这意味着"没进去过的界面"也能拿到控件树——但**能否被打开/内容是否被填充**仍需真进一次，
两者分开标注（§4 每节开头写"进入方式 / 观测到的内容"）。

启动态类分布（`boot_all_screens`，共 858 节点）：

| 类 | 数 | 类 | 数 |
|---|---|---|---|
| `GraphwarButton` | **283** | `Label` | 25 |
| `MarginContainer` | 210 | `ScrollContainer` | 7 |
| `HBoxContainer` | 80 | `Panel` | 7 |
| `Control` | 63 | `GridContainer` | 4 |
| `ColorRect` | 59 | `LineEdit` | 4 |
| `VBoxContainer` | 43 | `RichTextLabel` | 2 |
| `TextureRect` | 31 | `GraphwarPlane` / `AxisDrawer` / `EntityDrawer` / `EffectDrawer` | 各 1 |
| `AspectRatioContainer` | 26 | 其余（Node/TabContainer/Tree…） | — |

**没有原生 `Button`**：所有按钮都是 Rust 类 `GraphwarButton`（`text`、`icon`、`disabled`、
`toggle_mode`/`button_pressed` 都可用，说明它继承自 `BaseButton`）。

---

## 4. 逐界面台账

> 列义：
> **类型**＝`get_class()`；**文本/图标**＝运行时观测到的 `text`/`icon`/`texture`；
> **作用**＝`观测`（有直接证据）或 `推断`（写明缺什么）；
> **本仓库**＝`❌`＝没有这个控件（我们没有客户端）；`逻辑✅/🔶/❌ src/xxx` 指该控件背后
> 的游戏逻辑在本仓库的复原程度。
> 路径统一省略前缀 `GraphwarRootNode/VBoxContainer/ScreensControl/`。

### 4.1 MainMenu（主菜单）

进入方式：启动即在此。观测快照：全部 39 个（`MainMenu.visible=1` 于 `boot`）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `MainMenu/MarginContainer/VBoxContainer/MarginContainer/Label` | Label | `Graphwar II` | 标题（`minsize=(64,0)`，字号由 `TitleSize.gd` 调） | ❌ |
| `.../HBoxContainer/MarginContainer/VBoxContainer/PlayButton` | GraphwarButton | `Campaign` | **观测**：点它 → `Campaign.visible=1` | ❌ 控件；关卡生成 逻辑✅ `src/levelgen.rs` |
| `.../VBoxContainer/CharacterButton` | GraphwarButton | `Player\n` | **观测**：点它 → `PlayerScreen.visible=1` | ❌ |
| `.../MarginContainer2/VBoxContainer2/LocalButton` | GraphwarButton | `Battle` | **观测**：点它 → `LanLobby.visible=1` | ❌（联机 逻辑✅ `src/server.rs`/`protocol.rs`） |
| `.../VBoxContainer2/ReplayButton` | GraphwarButton | `Replays` | **观测**：点它 → `ReplayLobby.visible=1` | ❌（回放格式 观测✅ `docs/78`） |
| `.../VBoxContainer2/MarginContainer3/VersionLabel` | Label | `2.0.60` | 版本号（硬编码在场景里；`VersionLabel.gd` 只管字号） | ❌ |
| `.../VBoxContainer/AdContainer` | MarginContainer | — `AdContainer.gd` | 广告位（我们 harness 里是桩 `Ad_holder.gd`） | ❌ |

⇒ 用户口述的"主界面 4 个按钮"**成立且已逐项核对**（Campaign / Player / Battle / Replays）。

### 4.2 Campaign（关卡选择）

进入方式：`MainMenu/PlayButton`。观测快照：`campaign`、`campaign_after_level0`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `Campaign/MarginContainer/VBoxContainer/HBoxContainer/MarginContainer/Label` | Label | `CAMPAIGN` | 标题 | ❌ |
| `.../VBoxContainer/Control/level_0 … level_20` | GraphwarButton ×**21** | `0`…`20` | **观测**：`level_0` 按下 → `JoinLevelPopUp.visible=1`（进关卡预览）。**推断**：每个按钮一个关卡；21 关与用户口述"21 关（0–20）"一致。**缺**：锁定态（`lock.svg` 存在但本进程全部可点，因为本地存档是新的/未锁定状态未知） | ❌ 控件；关卡生成 逻辑✅ `src/levelgen.rs`（**只有两种模式**，见 `docs/103`） |
| `Campaign/.../Control` | Control | — `CampaingLevelsButtons.gd` | **观测（字符串池）**：这 21 个按钮**是脚本运行时 new 出来的**（`GraphwarButton.new()` + `set_name("level_"+str(i))` + `add_child`），不是场景里摆好的 | ❌ |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton ×2 | icon=`return.svg` | 返回（`ClipButton` 按下无反应，`LongPressButton` 是长按返回；见 §4.11 的同类验证） | ❌ |

### 4.3 JoinLevelPopUp（关卡预览 / 开局确认）

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

### 4.4 LanLobby（联机大厅：Local / Global）

进入方式：`MainMenu/LocalButton`。观测快照：`lan_lobby_local`、`lan_lobby_global`、
`connect_room__connecting_timeout`（房间列表已填充）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `LanLobby/.../HBoxContainer/ShowLocalContainer/ShowLocalButton` | GraphwarButton | `Local`（`toggle;checked`） | **观测**：切换按钮，默认选中 | ❌ |
| `.../HBoxContainer/ShowGlobalContainer/ShowGlobalButton` | GraphwarButton | `Global`（`toggle`） | **观测**：按下后 `GraphwarLanVBox` 里出现 **5 个公网房间条目**：`US Room 1`、`US Room 3`、`US Room 4`、`EU Room 1`、`EU Room 2`（注意**没有** `US Room 2`）。⇒ `docs/88` §10 里"ShowGlobalButton/ShowLocalButton 用途未知"已解：**是 LAN 大厅的"本地/公网房间列表"开关，不是回放界面** | ❌ 控件；协议 观测✅ `docs/24` |
| `.../MarginContainer/HBoxContainer/CreateButton` | GraphwarButton | ` Create Game ` | **观测**：→ `GameSetup.visible=1` | ❌ 控件；建房/服务端 逻辑✅ `src/server.rs` |
| `.../MarginContainer/HBoxContainer/JoinButton` | GraphwarButton | ` Join IP ` | **观测**：→ `JoinIpPopUp.visible=1` | ❌ |
| `.../MarginContainer2/VBoxContainer/HBoxContainer/NameContainer/RoomName` | Label | `Room Name` | 列表表头 | ❌ |
| `.../PlayersContainer/RoomName` | Label | `Players` | 列表表头（名字重复，是场景原作者的习惯） | ❌ |
| `.../ModeContainer/GameMode` | Label | `Game Mode` | 列表表头 | ❌ |
| `.../ScrollContainer/GraphwarLanVBox/<房间条目>` | Control（`scene=res://game_room_entry.tscn`） | 每条：`NameContainer/RoomName`=`US Room 1`/`EU Room 1`…、`PlayersContainer/PlayerCount`=`  0  `/`  1  `、`ModeContainer/OptionsContainer/{Axis,Function,Turn,Time}ModeButton` 四个图标、`JoinButtonContainer/JoinButton`=` Join ` | **观测**：公网房间列表条目；Join 按下后**没有**出现 `ConnectingPopUp`（`connect_room` 路由 20 s 超时）⇒ **推断**：公网房间列表来自服务器/缓存，离线时 Join 不发起连接或直接静默失败。**缺**：房间列表的来源（无网络，未抓包验证） | ❌ |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回主菜单 | ❌ |

### 4.5 GameSetup（对局设置）

进入方式：`LanLobby/CreateButton`。观测快照：`game_setup`、`setup_initial`、
`setup_six_bots`（6 个 Bot）、`setup_after_cycle`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `GameSetup/GraphwarSetupControl` | GraphwarSetupControl（Rust） | — | 整个设置界面的 Rust 控件根 | ❌ |
| `.../VBoxContainer/HBoxContainer/RoomName/RoomNameLineEdit` | LineEdit | `Graphwar Game` | 房间名，可编辑（`RoomName.gd` 调字号） | ❌ |
| `.../HBoxContainer/MarginContainer2/ChatButtonSetup` | GraphwarButton | icon=`no_chat.svg` | 打开聊天（图标随"有新消息"变成 `no_chat_new_chat.svg`——**推断**，只在场景里见到两个图标名） | ❌ |
| `.../HBoxContainer/MarginContainer3/LockButton` | GraphwarButton | icon=`unlock.svg` | 房间上锁/解锁（锁图标 `lock.svg`/`unlock.svg`） | ❌ |
| `.../HBoxContainer/MarginContainer4/GhostButton` | GraphwarButton | `         `（空白占位） | 标题占位（`LanLobbyTitle.gd`） | ❌ |
| `.../VBoxContainer/MarginContainer/HBoxContainer/MarginContainer/GameIpLabel` | Label | **`198.18.0.1:36071`** 等（每个进程不同） | **观测：这就是 `game_ip_label`——本机房间的 IP:端口**，`docs/88` §10 里标"未查"的一项，现已拿到 | ❌ 控件；端口/握手 观测✅ `docs/33` |
| `.../OptionsContainer/AxisModeButton` | GraphwarButton | icon=`axis_option_?.svg` | 坐标轴模式，点击循环（§6） | ❌ 控件；对物理无影响 推断✅（`docs/88` §14） |
| `.../OptionsContainer/FunctionModeButton` | GraphwarButton | icon=`fx_option.svg`⇄`dx_dy_option.svg` | 函数模式 `NormalFunction`⇄`DiffEqFunction`，点击切换 | ❌ 控件；**逻辑✅** `src/path.rs`（RK4）、`src/aigen.rs`（变量权重） |
| `.../OptionsContainer/TurnModeButton` | GraphwarButton | icon=`turns_sequential.svg`⇄`turns_simultaneous.svg` | 回合模式，点击切换 | ❌ 控件；**逻辑✅** `src/turn.rs`+`src/settle.rs`（含同时回合结算 `docs/86`） |
| `.../OptionsContainer/TurnTimeButton` | GraphwarButton | icon=`turn_30s/1m/2m/3m/5m/inf.svg` | 回合时限，点击循环（§6） | ❌ 控件；只影响思考时间 |
| `.../OptionsContainer/AddBotButton` | GraphwarButton | icon=`add_bot_option.svg` | **观测**：→ `AddBotPopUp.visible=1` | ❌ |
| `.../MarginContainer/HBoxContainer/StartButton` | GraphwarButton | ` Start ` | **观测**：→ `PlayScreen.visible=1`。**注意**：不加 Bot 直接 Start 会静默无操作（`docs/22` §5.1） | ❌ 控件；开局/回合状态机 **逻辑✅** `src/turnstate.rs` |
| `.../MarginContainer2/ScrollContainer/PlayerListContainer/Team1Container` | VBoxContainer（`scene=res://team1_entry.tscn`） | `Label`=`Team 1` | 队伍 1 列表 | ❌ |
| `.../PlayerListContainer/Team2Container` | VBoxContainer（`scene=res://team2_entry.tscn`） | `Label`=`Team 2` | 队伍 2 列表 | ❌ |
| `.../TeamXContainer/HBoxContainer`（每个玩家一行，`scene=res://team1/2_entry.tscn` 的子结构） | MarginContainer 组合 | 见下 | **玩家行**（每行一个玩家） | ❌ |
| └ `.../HBoxContainer/MarginContainer/PlayerNameLabel` | Label | `user` / `Deep Blue` / `Alpha Zordon` / `ChatGPT` / `Multivac` / `Deep Thought` / `Skynet` / `Agent Smith` | 玩家名 | ❌ |
| └ `.../HBoxContainer/RankIcon` | TextureRect | 玩家 `ranks/rank_0.svg`；Bot `hats/no_hat.svg`（空） | 段位图标 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer/RemovePlayerButton` | GraphwarButton | icon=`x.svg` | 踢出玩家 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer2/AddSoldierButton` | GraphwarButton | icon=`plus.svg` | 该玩家**加一个士兵**（多士兵=多命） | ❌ 控件；多士兵生命 **逻辑✅** `src/life.rs` |
| └ `.../HBoxContainer/AspectRatioContainer3/RemoveSoldierButton` | GraphwarButton | icon=`minus.svg` | 减一个士兵 | ❌ |
| └ `.../HBoxContainer/AspectRatioContainer4/ChangeTeamButton` | GraphwarButton | icon=`arrow_right.svg`（Team1）/`arrow_left.svg`（Team2） | 换队 | ❌ |
| └ `.../HBoxContainer/SoldierIconContainer/MarginContainer…5` | MarginContainer ×5（`SoldierIconMarginContainer.gd`） | 每个槽 4 个 `TextureRect`：`SkinIcon`=`skins/yellow.svg` 或 `skins/bot_N.svg`、`FaceIcon`=`faces/regular_eyes.svg`/`no_face.svg`、`HatIcon`=`hats/soldier_helmet.svg`/`no_hat.svg`、`EmptyIcon`=`empty_soldier.svg` | **观测**：每个士兵一个槽（**5 个槽**⇒每玩家最多 5 个士兵），空槽显示 `empty_soldier.svg` | ❌ 控件；皮肤系统 `rsc/skins|faces|hats` |
| `.../VBoxContainer/AdContainer` | MarginContainer | — | 广告位 | ❌ |

**观测到的 Bot 名单**（6 档各点一次）：`Deep Blue`、`Alpha Zordon`、`Alpha Zordon`、`Multivac`、
`Deep Blue`、`ChatGPT` —— 名字会重复，**证明名字池与难度不是 1:1**（与 `AI.md` §1 的记载一致：
另有 `Skynet`/`HAL`/`Cleverbot` 等名字）。

### 4.6 AddBotPopUp（加 Bot）

进入方式：`GameSetup/OptionsContainer/AddBotButton`。观测快照：`add_bot_popup`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `AddBotPopUp/.../VBoxContainer/OptionsContainer/AspectRatioContainer/AddBotButton` | GraphwarButton | icon=`skins/bot_0.svg` | 难度档 0 | ❌ 控件；**六档参数表 逻辑✅** `AI.md` §1 / `src/aigen.rs` |
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

### 4.7 JoinIpPopUp（输入 IP 加入）

进入方式：`LanLobby/JoinButton`。观测快照：`join_ip_popup`、`connect_ip_port__after_join_click`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `JoinIpPopUp/.../HBoxContainer/MarginContainer/Label` | Label | `Ip:` | 提示 | ❌ |
| `.../HBoxContainer/MarginContainer2/JoinIpIpLineEdit` | LineEdit | 空；我们写入过 `10.255.255.1`、`10.255.255.1:12345`、`127.0.0.1:1` | IP 输入（**必须带端口**） | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Cancel ` | 取消 | ❌ |
| `.../HBoxContainer3/MarginContainer2/GraphwarButton` | GraphwarButton | `  Join  ` | **观测**：按下 `127.0.0.1:1` → 2 s 内出现 `DisconnectedPopUp`；按下 `10.255.255.1:12345` → 2 s 内出现 `ConnectingPopUp`。**只写 `10.255.255.1`（无端口）时毫无反应**（120 s 超时）⇒ 端口必填，格式 `host:port` | ❌ 控件；握手 **逻辑✅/观测** `docs/33`、`src/protocol.rs` |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

### 4.8 ConnectingPopUp（连接中）

进入方式：`JoinIpPopUp` 里 Join 一个**不可达但端口合法**的地址（`10.255.255.1:12345`）。
观测快照：`connect_ip_port__connecting`（`@@WATCHHIT` ms=1915）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ConnectingPopUp/.../HBoxContainer/MarginContainer/Label` | Label | `Connecting...` | 提示 | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Cancel ` | 取消连接 | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

### 4.9 DisconnectedPopUp（断开）

进入方式：`JoinIpPopUp` 里 Join `127.0.0.1:1`（端口拒绝）。
观测快照：`connect_refused__disconnected`（`@@WATCHHIT` ms=1980）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `DisconnectedPopUp/.../HBoxContainer/MarginContainer/DisconnectedLabel` | Label | `You have been disconnected.` | 提示 | ❌ |
| `.../HBoxContainer3/MarginContainer/GraphwarButton` | GraphwarButton | ` Ok ` | 确认 | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

### 4.10 PlayScreen（对局）

进入方式：`GameSetup` 加 Bot → `StartButton`。观测快照：`play_screen_t0/t3s/t6s`、
`play_diffeq_t0/t3s`、`play_extra_buttons`、`play_names_toggled`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `PlayScreen/.../PlayMainHBox/LeftControls/LeftButtonGrid` | GridContainer `ButtonGrid.gd` | **18 个** `GraphwarButton` | **左键盘**：`C` `π` `e` `1`…`9` `0` `.` `(` + 三个控制键（icon=`arrow_left_bar.svg` / `arrow_left.svg` / `delete_left.svg`） | ❌ 控件；函数文法 **逻辑✅** `src/function/` |
| `.../RightControls/RightButtonGrid` | GridContainer `ButtonGrid.gd` | **18 个** `GraphwarButton` | **右键盘**：`abs` `ln` `√` `sin` `cos` `tan` `*` `/` `exp` `+` `-` `^` `x` `y`(disabled) `)` + icon=`delete_right.svg` / `arrow_right.svg` / `arrow_right_bar.svg` | ❌ 控件；文法 **逻辑✅** `src/function/` |
| `.../RightButtonGrid/YButton` | GraphwarButton | `y`，**永远 `disabled`** | **观测**：Normal 与 DiffEq 两种模式下都 disabled。**推断**：与"`y'` 是死特性、DiffEq 只生成 x/y 权重"有关，但**为什么 `y` 也不给按**未知 | ❌ |
| `.../RightControls/HBoxContainer/FireContainer/FireButton` | GraphwarButton | ` Fire! ` | 开火。**`docs/26` 观测**：`emit_signal("pressed")` 真的发出 `FunctionFireRequest`，随后 `shot_calc/calculate_path` 计数上升（本文件只观测到按钮存在且未 `disabled`，**没有重复验证开火**） | ❌ 控件；**开火链路 逻辑✅** `src/shot.rs`/`path.rs` |
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
| `GraphwarPlane/@Line2D@N` | Line2D | — | **观测**：开火后出现（`scene_file_path` 为空 ⇒ **不是** `line_2d.tscn` 实例，而是运行时新建/由 Rust 建） | ❌ 控件；弹道 **逻辑✅** `src/path.rs` |
| `PlayScreen/.../VBoxContainer/HBoxContainer2/GraphwarFunctionInput` | GraphwarFunctionInput（Rust） | `text` = 当前函数串，如 `(1.44*(cos(x)+x))`、`log(exp((12.10/(-19.42/x))))`、`sin((-1.20+x))` | 函数输入框（Rust 自绘，无 LineEdit 子节点）。**观测**：`text` 属性可读 ⇒ 能读到场上显示的函数 | ❌ 控件；解析 **逻辑✅** `src/function/` |
| `.../HBoxContainer2/FxContainer/FxLabel` | Label | ` f(x) = `（DiffEq 时同位置显示 `dx/dy` 图标） | 函数前缀提示 | ❌ |
| `.../FxContainer/AspectRatioContainer/FxTextureButton` | GraphwarButton | icon `fx_option.svg`（Normal）/`dx_dy_option.svg`（DiffEq） | **观测**：模式指示图标 | ❌ |
| `.../HBoxContainer2/TimerContainer/TimerLabel` | Label | `02:59`、`01:59`、`00:04`、`00:00` | 回合倒计时。**观测**：按 2 次 `TurnTimeButton` ⇒ 起始 `02:59`（3 分钟档）、按 1 次 ⇒ `01:59`（2 分钟档）；见 §6 | ❌ 控件；时钟 **观测✅** `docs/35` |
| `.../HBoxContainer2/OffsetContainer/OffsetLabel` | Label | Normal：` + 2.515 `、` + 0.919 `；**DiffEq：空** | 修正常数显示。**观测**：DiffEq 模式下为空（`+c` 修正不显示/不适用） | ❌ 控件；修正 **逻辑✅** `src/shot.rs` |
| `.../HBoxContainer2/GraphwarFunctionInput`（同上） | — | — | — | — |
| `PlayScreen/.../VBoxContainer/PlayMainHBox/PlaneContainer/GamePlane/PlaneContainer/VBoxContainer/HBoxContainer/ViewFunctionsLabel` | RichTextLabel `ViewFunctionsLabel.gd` | `[outline_size=4][outline_color=black][color=green]player 1: f(x) = 3x+234[/color]…` | 顶部"各玩家当前函数"列表 | ❌ |
| `.../RightControls/HBoxContainer/FireContainer/ClipButton` | — | — | （同上，动画层） | ❌ |

**DiffEq 与 Normal 的运行时差异（观测，`play_diffeq` vs `play`）**：键盘完全一致（含 `y` 仍 disabled）、
`OffsetLabel` 从 `+ 0.919` 变空、`FxTextureButton` 图标 `fx_option.svg`→`dx_dy_option.svg`。
⇒ **界面几乎不区分两种模式**，差异只在 Rust 内的取值/显示。

### 4.11 ConfirmPlayExitPopUp（退出确认）

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

### 4.12 ChatPopUp（聊天）

进入方式：对局里 `ExtraOptionsHBox/ChatButtonPlay`（Setup 里是 `ChatButtonSetup`）。
观测快照：`play_chat__play_chat_popup`（`ChatPopUp.visible=1`）。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ChatPopUp/ChatContainer/ChatContainer2/ChatBox` | RichTextLabel `ChatBox.gd` | 空（无消息时） | 聊天记录。**观测（字符串池）**：`_gui_input` 里点它 → `chat_line_edit.grab_focus()` | ❌ 控件；聊天协议 **观测✅** `docs/24` |
| `.../ChatContainer2/ChatLineEdit` | LineEdit | 空 | 输入框；`text_submitted → on_chat_submitted`（**观测**：唯一一条指向 GDScript 的 `text_submitted` 连接，属于 `chat_pop_up.tscn` 内联脚本 `GDScript_k8tdp`） | ❌ |
| `.../ChatContainer2/MarginContainer/GraphwarButton` | GraphwarButton | `      `（空白，图标未在 `icon` 属性上） | 发送按钮（**推断**：无 text/icon，靠主题/子节点画） | ❌ |

### 4.13 EndGamePopUp（结算）

进入方式：**必须真的打完一局**。用**档 5 的 Bot**（`OptionsContainer2/AspectRatioContainer4`）
+ 30 s 回合时限，`endgame6` 路由里 31.7 s（首次）/154 s（复跑）后自然出现。
观测快照：`endgame6__endgame_popup`。

| 控件 | 类型 | 文本 | 作用 | 本仓库 |
|---|---|---|---|---|
| `EndGamePopUp/.../VBoxContainer/SelfVictoryLabel` | Label | **`YOU lOST`**（游戏原文如此，小写 l） | 自己的胜负 | ❌ |
| `.../VBoxContainer/TeamVictoryLabel` | Label | `Team 2 Victory!` | 获胜方 | ❌ 控件；胜负判定 **逻辑✅** `src/settle.rs`、`docs/86` |
| `.../VBoxContainer/HBoxContainer/VictoryPlayerContainer1 … 10` | MarginContainer ×10 | 每个槽 3 个 `TextureRect`：`SkinIcon`（观测 `skins/bot_5.svg`）、`FaceIcon`（`faces/no_face.svg`）、`HatIcon`（`hats/no_hat.svg`） | 胜利方士兵展示（**10 个槽**⇒最多展示 10 个士兵） | ❌ |
| `.../MarginContainer2/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 关闭 | ❌ |

> 在此之前的两轮尝试（档 2 的 Bot，各等 300 s / 420 s）**都没有出现**结算框
> ⇒ 结算框依赖"真的有人全灭"，不依赖时间。

### 4.14 PlayerScreen（玩家：改名 / 皮肤 / 脸 / 帽子）

进入方式：`MainMenu/CharacterButton`。观测快照：`player_screen`。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `PlayerScreen/.../HBoxContainer/MarginContainer/Label` | Label | `PLAYER OPTIONS` | 标题 | ❌ |
| `.../HBoxContainer3/TabContainer` | TabContainer `PlayerTabContainer.gd` | 3 个页：`SkinsContainer`、`FacesContainer`、`HatsScroll` | 三个分页。**观测（字符串池）**：脚本用 `set_tab_title(i, …)` 设标题；标题字面量不在字符串池 ⇒ 取自子节点 | ❌ |
| `.../TabContainer/SkinsContainer/VBoxContainer/HBoxContainer/SkinsVbox` | VBoxContainer `ShopButtonsSize.gd` | **5 行 × 3 个** `GraphwarButton`（15 个，直接摆在 `SkinsVbox` 里，**不是** `shop_button_entry` 实例） | 皮肤选择网格 | ❌ 控件；皮肤资源 `rsc/skins/` |
| `.../TabContainer/FacesContainer/.../FacesVbox` | VBoxContainer `ShopButtonsSize.gd` | **5 行 × 3 个槽**（15 个），每槽 `Control`（`scene=res://shop_button_entry.tscn`）内含 `GraphwarButton` + **`SkinBackgroundButton`** + `Button` | 脸选择网格。**`SkinBackgroundButton` 就在这里**（`docs/88` §10 里点名但没查过的控件） | ❌ |
| `.../TabContainer/HatsScroll/.../HatsVbox` | VBoxContainer `ShopButtonsSize.gd` | 同 Faces 结构，**9 行 × 3 = 27 个** `shop_button_entry` 实例 | 帽子选择网格 | ❌ |
| `.../VBoxContainer/HBoxContainer2/MarginContainer/Label` | Label | ` NAME: ` | 名字标签 | ❌ |
| `.../HBoxContainer2/MarginContainer2/PlayerNameLineEdit` | LineEdit `PlayerNameLineEdit.gd` | `user` | 改名（`text_changed → on_text_change`，Rust 回调） | ❌ |
| `.../VBoxContainer/MarginContainer/AspectRatioContainer/PlayerShowcase` | PlayerShowcase（Rust） | — | 士兵预览（Rust 自绘） | ❌ |
| `.../AspectRatioContainer/SkinBlock/VBoxContainer/Label` | Label | `Unlocking skins also removes all ads` | 广告说明 | ❌ |
| `.../SkinBlock/VBoxContainer/Label2` | Label | `Support the game` | 内购说明 | ❌ |
| `.../SkinBlock/VBoxContainer/HBoxContainer/UnlockButton` | GraphwarButton `UnlockButton.gd` | `Unlock` | 内购解锁 | ❌ |
| `.../AspectRatioContainer/PendingBlock/VBoxContainer/Label` | Label | `Pending...` | 内购挂起状态 | ❌ |
| `.../PlayerScreen/.../ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回 | ❌ |

> **`docs/88` §10 的"Player 界面从未查过"到此关闭**：改名 LineEdit、三页签、Skins 15 个按钮 /
> Faces 15 个槽 / Hats 27 个槽（`shop_button_entry` 实例共 **42** 个）、`SkinBackgroundButton`、
> `UnlockButton`、`PendingBlock` 全部实测到。

### 4.15 ReplayLobby（回放列表）

进入方式：`MainMenu/ReplayButton`。观测快照：`replay_lobby`（59 个条目，2118 节点）。

| 控件 | 类型 | 文本/图标 | 作用 | 本仓库 |
|---|---|---|---|---|
| `ReplayLobby/.../HBoxContainer/MarginContainer/Label` | Label | `REPLAYS` | 标题 | ❌ |
| `.../ScrollContainer/ReplaysContainer/<条目>` | HBoxContainer（`scene=res://replay_entry.tscn`） | — | 一条回放。**观测**：本机 replay 目录里有 59 个文件，就渲染 59 条 | ❌ 控件；回放格式 **观测✅** `docs/78`、解析器 `tools/parse_replay.py` |
| └ `.../ScrollContainer/NameContainer/ReplayName` | Label | `2026-11-05 12:48:11` 等（时间戳） | 回放名=**录制时间** | ❌ |
| └ `.../GameNameContainer/GameName` | Label | `Graphwar Game` | 房间/对局名 | ❌ |
| └ `.../ModeContainer/OptionsContainer/{Axis,Function,Turn,Time}ModeButton` | GraphwarButton ×4 | 四个模式图标 | 该局的四个模式 | ❌ |
| └ `.../AspectRatioContainer/PlayReplayButton` | GraphwarButton | 图标未在 `icon` 属性上 | **播放该回放**（未点击验证播放行为） | ❌ 控件；回放重放 逻辑🔶 |
| `.../HBoxContainer/ExitButtonContainer/{ClipButton,LongPressButton}` | GraphwarButton | icon=`return.svg` | 返回 | ❌ |
| `.../VBoxContainer/AdContainer` | MarginContainer | — | 广告位 | ❌ |

---

## 5. 场景实例台账（25 个 `.scn` → 节点路径）

`godot-live/.godot/exported/*/` 里有 **31 个 `.scn`**，其中 **6 个是 AdMob 插件场景**
（`Banner` `Interstitial` `Rewarded` `RewardedInterstitial` `Ump` `Main`），
**游戏本体 25 个**（与 `rsc/*.tscn.remap` 的 25 个一致）。

| 场景 | 观测到的实例路径 | 观测状态 |
|---|---|---|
| `main.tscn` | `GraphwarRootNode` 本身 | ✅ |
| `main_menu.tscn` | `ScreensControl/MainMenu` | ✅ |
| `play_screen.tscn` | `ScreensControl/PlayScreen` | ✅ |
| `campaign.tscn` | `ScreensControl/Campaign` | ✅ |
| `lan_lobby.tscn` | `ScreensControl/LanLobby` | ✅ |
| `GameSetup.tscn` | `ScreensControl/GameSetup` | ✅ |
| `player_screen.tscn` | `ScreensControl/PlayerScreen` | ✅ |
| `replay_lobby.tscn` | `ScreensControl/ReplayLobby` | ✅ |
| `add_bot_pop_up.tscn` | `ScreensControl/AddBotPopUp` | ✅ |
| `chat_pop_up.tscn` | `ScreensControl/ChatPopUp` | ✅ |
| `join_ip_pop_up.tscn` | `ScreensControl/JoinIpPopUp` | ✅ |
| `join_level_pop_up.tscn` | `ScreensControl/JoinLevelPopUp` | ✅ |
| `connecting_pop_up.tscn` | `ScreensControl/ConnectingPopUp` | ✅ |
| `disconnected_pop_up.tscn` | `ScreensControl/DisconnectedPopUp` | ✅ |
| `endgame_pop_up.tscn` | `ScreensControl/EndGamePopUp` | ✅ |
| `confirm_exit_pop_up.tscn` | `ScreensControl/ConfirmPlayExitPopUp` | ✅ |
| `team1_entry.tscn` | `GameSetup/.../Team1Container`（每个玩家再加一行） | ✅ 21 处 |
| `team2_entry.tscn` | `GameSetup/.../Team2Container` | ✅ 16 处 |
| `game_room_entry.tscn` | `LanLobby/.../GraphwarLanVBox/…` | ✅ 8 处 |
| `replay_entry.tscn` | `ReplayLobby/.../ReplaysContainer/…` | ✅ 59 处 |
| `shop_button_entry.tscn` | `PlayerScreen/.../{Skins,Faces,Hats}…` | ✅ 42 处 |
| `name_label.tscn` | `PlayScreen/.../GraphwarPlane/@EntityDrawer@N/…` | ✅ 15 处 |
| `function_buttons.tscn` | — | ❌ **未观测**（两个键盘网格是 `play_screen.tscn` 内的 `GridContainer`，`scene_file_path` 为空） |
| `function_buttons_no_control.tscn` | — | ❌ **未观测**（同上） |
| `line_2d.tscn` | 只见到一个 `@Line2D@N` 节点，`scene_file_path` 为空 | ❌ **未观测**（画线节点在，但不是该场景的实例） |

⇒ 三个场景**在本进程的运行路径里没有被实例化**。**推断（缺证据）**：
`function_buttons*` 可能是回放/观战模式下"只读键盘"的备选场景，`line_2d` 可能被
Rust 直接 `new` 而不是 `instantiate`。要证实需要进"回放播放"那条路径（本文件未进入）。

---

## 6. 选项枚举（连点 `@@CYCLE`，观测）

四个选项按钮连点后 `icon` 的变化。**注意一个坑**：`@@CYCLE` 打印的是**按下那一瞬间**
读到的 `icon`，而 Rust 在**下一帧**才刷新图标 ⇒ **打印值 = 这次按下之前的状态**。
这不是推断，是用"连点后的终态"交叉验证出来的（见下表最后一列）：

| 按钮 | 初值（未按之前） | 每按一次的转移顺序 | 状态数 | 连点后终态（`setup_after_cycle`） |
|---|---|---|---|---|
| `AxisModeButton` | `axis_option_3` | `3→2→1→0→3` | **4** | 按 6 次 ⇒ `axis_option_1` ✔ |
| `FunctionModeButton` | `fx_option` | `fx→dx_dy→fx` | **2** | 按 6 次 ⇒ `fx_option` ✔ |
| `TurnModeButton` | `turns_sequential` | `sequential→simultaneous→sequential` | **2** | 按 6 次 ⇒ `turns_sequential` ✔ |
| `TurnTimeButton` | `turn_1m` | `1m→2m→3m→5m→∞→30s→1m` | **6** | 按 8 次 ⇒ `turn_3m` ✔ |

原始 `@@CYCLE` 打印序列（＝按之前的状态，故整体比"按下后"早一格）：

```
AxisModeButton   3 2 1 0 3 2          FunctionModeButton  fx dx fx dx fx dx
TurnModeButton   seq sim seq sim seq sim   TurnTimeButton  1m 2m 3m 5m inf 30s 1m 2m
```

**玩法侧交叉验证**（两处独立观测）：

- `play` 路由按 **2 次** `TurnTimeButton` → 开局 `TimerLabel = 02:59`（= **3 分钟**档）；
- `play_diffeq` 路由按 **1 次** → `TimerLabel = 01:59`（= **2 分钟**档）；
- 四个快照（`game_setup` / `setup_initial` / `setup_six_bots`）在**未按任何选项**时
  `TurnTimeButton` 恒为 `turn_1m` ⇒ 初值 1m，每按一次前进一档。三条互相吻合。

⇒ 状态数：**Axis 4 / Function 2 / Turn 2 / Time 6**，与 `docs/88` §1 的 2×2×6×4 完全吻合
（**这是第一次用界面本身验证这张表**）。

## 7. 与 `docs/88` §10–§11 的差异（补正）

| `docs/88` 的说法 | 本文件观测 | 结论 |
|---|---|---|
| §11 "31 个编译场景（对应 31 个 `.tscn.remap`）" | `.tscn.remap` 只有 **25** 个；`.scn` 31 个里有 6 个是 AdMob 插件场景 | **修正**：游戏本体 25 个 |
| §11 "界面、按钮、信号、处理器全都在 `.gdc` 里，可以系统地一次性拿全" | 51 个非插件 `.gdc` 基本是字号/间距助手；282 个按钮只有自连 `pressed→pressed`，**没有 GDScript 处理器** | **推翻**：界面逻辑在 Rust；`.gdc` 反编译拿不到"处理器逻辑" |
| §12 "先反编译 105 个脚本 → 得到全部界面逻辑" | 反编译可以拿到"布局助手"，但**界面逻辑必须从 Rust 侧或运行时契约反推** | **修正路线**：运行时枚举（本文件）+ Rust 侧观测 |
| §10 "Player 界面 / 皮肤系统 **从未查过**" | §4.14 完整清单（三页签、改名、`SkinBackgroundButton`、`UnlockButton`、`PendingBlock`） | **关闭** |
| §10 "Replays 回放播放器" | §4.15 条目结构完整；**播放行为未点** | 部分关闭 |
| §10 "房间 IP/端口显示 `game_ip_label` **未查**" | 实测节点名是 **`GameIpLabel`**，显示 `198.18.0.1:<port>` | **关闭** |
| §10 "`ShowGlobalButton`/`ShowLocalButton` 用途未知" | LAN 大厅的**公网/本地房间列表开关**（toggle），公网列表 5 条：`US Room 1/3/4`、`EU Room 1/2` | **关闭** |
| §10 "用户口述 21 关（0–20）" | `level_0`…`level_20` 共 21 个按钮，且由脚本运行时创建 | **确认** |
| §10 "Battle：Local/Global、Create/Join IP、四模式、加 AI（6 级）" | 全部落实：`ShowLocal/ShowGlobal`、`CreateButton`、`JoinButton`→`JoinIpPopUp`、四个选项按钮、6 个 `AddBotButton` | **确认** |

---

## 8. 未观测 / 未知（不猜）

| 项 | 状态 | 缺什么 |
|---|---|---|
| `function_buttons.tscn`、`function_buttons_no_control.tscn` | **未观测** | 哪条路径会实例化它们（推测与回放/观战只读键盘有关） |
| `line_2d.tscn` | **未观测** | 画线节点存在但无 `scene_file_path`；是否由 Rust `new` 出来未证实 |
| 回放**播放**（`PlayReplayButton` 按下之后） | **未观测** | 点它之后的界面（是否进 `PlayScreen`、是否有回放控件条） |
| `JoinLevelPopUp` 的 `Start` 按下之后 | **未观测** | 进关卡后与普通对局有何不同（目标/掩膜） |
| `ConfirmPlayExitPopUp` 的 `Yes` | **未观测** | 是否回主菜单（有 `_process` 的 `endgame_pop_up.gdc` 之类的小脚本，未验证） |
| 关卡锁定态（`lock.svg`） | **未观测** | 本机存档下 21 关全可点；锁定判据（存档位）未知 |
| `bot_0..bot_5` 图标 ↔ `AI.md` 六档难度 | **推断** | 无直接映射观测；需读 Rust 内部表或按开局后的 Bot 行为对照 |
| `YButton(y)` 为何两种模式都 `disabled` | **未知** | 与"`y'` 是死特性"有关是推断，未证实 |
| 公网房间列表的来源 | **未知** | 无网络；未抓 `docs/24` 之外的包 |
| 聊天发送按钮的图标 | **推断** | 无 `icon` 属性文本；可能在主题或子节点 |
| `GraphwarButton.pressed → pressed` 自连（282 处） | **观测到现象、语义未知** | 是 Rust 类内部把信号转发给自己（可能是为了让 Rust 侧收到回调） |

---

## 9. 复现

```bash
cd /home/user/MyProject/gw2

# 1) 全量：17 条路由，逐个 headless 跑原版游戏并把节点树 dump 到 .rea-work/ui/
bash gw2-rebuild/tools/dump_ui.sh

# 2) 只跑某几条（boot = 启动态全树；play = 真打一局；endgame6 = 打到结算框）
bash gw2-rebuild/tools/dump_ui.sh boot play endgame6

# 3) 把 dump 汇总成"界面 × 控件"表
python3 gw2-rebuild/tools/dump_ui_report.py     # 生成 .rea-work/ui/summary/*.tsv

# 4) .gdc 字符串池（不是反编译）
python3 gw2-rebuild/tools/gdc_strings.py --all gw2-rebuild/rsc/ --json .rea-work/gdc_strings.json
python3 gw2-rebuild/tools/gdc_strings.py --grep LongPress --all gw2-rebuild/rsc/
```

前置条件（`docs/22` §4）：`godot-live/` 里已有 `rsc/` 全量 + 原版 `libgraphwarrust.so`、
**删掉 `godot-live/project.binary`**（否则 `project.godot` 的 autoload 不生效）、
Godot 在 `tools/Godot_v4.7.2-stable_linux.x86_64`。

环境变量（脚本已带）：`XDG_DATA_HOME=<ws>/.godot-data`、`XDG_CACHE_HOME`、`XDG_CONFIG_HOME`。

路由清单（`tools/dump_ui.gd` 的 `ROUTES`）：

| 路由 | 到达的界面/状态 | 快照标签 |
|---|---|---|
| `boot` | 启动态（15 个界面全在树里） | `boot_all_screens` |
| `campaign` | Campaign → JoinLevelPopUp | `campaign`、`campaign_after_level0` |
| `player` | PlayerScreen | `player_screen` |
| `replay` | ReplayLobby（59 条） | `replay_lobby` |
| `lobby` | LanLobby ←Local/Global→ | `lan_lobby_local`、`lan_lobby_global` |
| `setup` / `addbot` / `setup_bots` | GameSetup / AddBotPopUp / 6 Bot 玩家表 | `game_setup`、`add_bot_popup`、`setup_six_bots` |
| `joinip` / `connect_ip_port` / `connect_refused` / `connect_room` | JoinIpPopUp + 连接三态 | `join_ip_popup`、`connecting`、`disconnected` |
| `setup_cycle` | 四选项按钮连点枚举 | `setup_initial`、`setup_after_cycle`（+`@@CYCLE` 行） |
| `play` / `play_diffeq` / `play_extra` / `play_chat` / `play_exit` | 对局（Normal/DiffEq/附加按钮/聊天/退出确认） | `play_screen_t0/t3s/t6s`、`play_diffeq_*`、`play_chat_popup`、`confirm_exit_*` |
| `endgame` / `endgame6` | 等结算框（档 2 / 档 5 Bot） | `endgame_popup` / `endgame_popup_timeout` |

产出规模（本轮）：**19 条路由 → 33 个不同快照标签（39 个"日志×标签"组合）→ 15405 条控件行**，
`boot` 快照 858 节点、对局快照 969 节点、59 条回放时 2118 节点。

---

## 10. 结论：对"复原客户端"意味着什么

1. **界面的"形"已经完全可枚举**：15 个界面、283 个按钮、全部文本与图标、控件层级、
   场景实例位置、选项循环顺序，都在这份台账里，且一条命令可复现。
2. **界面的"行为"不在 GDScript 里**：`.gdc` 只是响应式布局助手。想复原点击后的行为，
   要么照本台账的"作用"列（多数已由本仓库的 Rust 逻辑覆盖：回合/结算/开火/关卡/AI），
   要么继续从 `libgraphwarrust.so` 侧观测（REA/frida），而不是指望反编译脚本。
3. **本仓库目前的实现程度**：控件 **0**；背后逻辑按列填：函数文法✅、采样✅、修正✅、
   坐标/掩膜✅、回合结算✅、关卡生成✅、AI✅、协议✅、回放格式✅；**皮肤/内购/广告/聊天 UI、
   房间列表、玩家存档（段位/解锁）全部未实现**。
4. **下一步（建议）**：
   - 若要"能玩的客户端"，最小闭环是 `MainMenu → LanLobby/local → GameSetup → PlayScreen →
     EndGamePopUp`，其余（Campaign/Player/Replays/公网）可后置；
   - 每个控件的"作用"列已可当验收清单用；
   - 三个未观测场景（§8 前三条）与回放播放，是唯一的枚举缺口。
