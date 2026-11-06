# 108 · Godot 客户端重建（`client/`）

状态：**现行**。本文是「客户端」这一块的**唯一**文档：架构、15 个界面的实现台账、
哪些是占位、怎么跑、怎么验、还差什么。

> 前置阅读：[`92-ui-inventory.md`](92-ui-inventory.md)（运行时界面台账）、
> [`104-recovered-project.md`](104-recovered-project.md)（GDRE 静态属性 + 对拍）、
> [`101-gdc-contents.md`](101-gdc-contents.md)（`.gdc` 字节码内容与布局数值表）。
> 本文不重复它们的观测，只写**我们把它们变成了什么代码**。

---

## 0. 一页结论

| 项 | 结果 |
|---|---|
| 工程位置 | `gw2-rebuild/client/`（Godot **4.7.2**，与游戏同版本） |
| 界面 | **15/15 全部实现**（7 个基础界面 + 8 个弹窗），可按原版路由互相跳转 |
| 能否启动 | `godot --headless --path client --quit` → **无错误、退出码 0** |
| 能否开一局 | ✅ 设置对局 → 加 Bot → Start → 键盘输入函数 → Fire → 画弹道 → 命中判定 → 结算框 |
| 自动验收 | `bash run.sh selftest` → **93 条断言全绿**（退出码 0） |
| 节点树导出 | `bash run.sh dump` → 每个节点一行（格式仿 `docs/92` 的 `@@N` 行，含 pos/size） |
| 逐界面截图 | `bash run.sh shots`（Xvfb + OpenGL）→ 11 张 PNG，用于肉眼核对布局 |
| 游戏资产 | **0 字节**：无 `.tscn`/`.gd`/`.ttf`/`.svg` 原件，无字体文件，图标全部现画 |
| 界面逻辑归属 | 仍在 Rust（`docs/92` §2.3）；本工程通过 `core_bridge.gd` 这一层接缝调用内核 |
| 弹道 | ~~内核只暴露 `ping()`，所以是界面预览~~ ✅ **已接上**（`docs/116`）：`client/gw2rebuild.gdextension` 已落地，`GwCore.compute_path()` 的 `source` 现在是 **`kernel`**，PlayScreen 画的就是内核采样器的输出；⚠️ 新增 `.gdextension` 后必须跑一次 `--import` 才会被扫描到 |

三句话：

1. **「形」可以照台账重建，「行为」必须自己写。** 15 个界面的控件树、文本、图标名、选项循环
   都在 `docs/92`/`104`/`101` 里，够搭出来；但按钮按下之后干什么（原版在 Rust 里）没有任何
   反编译产物，只能按台账的"作用"列重写 —— 这就是 `src/app/app.gd` 的命令总线。
2. **最重要的一个发现（新）**：恢复出的 `.tscn` 里每个按钮都带 `input_text`，
   那串小写标识符就是 **Rust 侧的命令键**（`campaign` / `add_bot_3` / `fire` / `toggle_names`…）。
   本项目把它当成界面与内核之间的**协议**照抄，见 §4。
3. **凡是我们自己定的规则，本文 §7 全部登记**（命中判定、关卡解锁、Bot 分队、聊天、
   回放、内购、广告、ScrollContainer 的取舍…），不允许出现"看起来像原版其实是编的"。

---

## 1. 依据与思路

### 1.1 三条已知事实决定了做法

| 事实 | 出处 | 对本工程的影响 |
|---|---|---|
| 界面**逻辑**不在 GDScript 里，在 Rust（51 个非插件脚本里只有 5 个有行为，且都不是游戏规则） | `docs/92` §2.3、`docs/101` §4/§6 | 不能"反编译出界面代码"；只能按台账的控件 + 作用**重写**逻辑 |
| 界面的**静态属性**（锚点 / `custom_minimum_size` / 9 条 theme override / 29 条最小尺寸）已完全解出 | `docs/104` §4 | 布局数值直接引用，不猜 |
| 字号规则**全是"屏幕宽 ÷ N"**（N ∈ {8,10,15,20,30,40,50,60}），28 处 | `docs/101` §5.2 | 字号随窗口缩放，且各控件用不同 N（要分开设，见 §9 的坑） |

### 1.2 为什么不直接使用恢复出的 31 个 `.tscn`

1. **许可与仓库规则**：它们是游戏资产（`AGENTS.md`、`docs/104` 顶部警告：`.tscn`/`.gd`/`.ttf`/`.svg`
   一个都不进 git）；
2. **搬进来也不够**：`.tscn` 里 0 条 `[connection]`（`docs/104` §6.1），按钮没有处理器；
   静态场景里还缺 268 个运行时才创建的节点（`docs/104` §5.3）；
3. **重建的意义**在于"我们写的代码能被审查"，不是"把资产换个地方存"。

所以本工程的界面是**用 GDScript 搭出来的**（`src/ui/ui.gd` 这套小积木），
每个界面一个只有根节点 + 脚本的 `.tscn`（`scenes/screens/*.tscn`）。
理由与代价都写在 §9.1。

---

## 2. 架构

```
client/
├── project.godot                 # 4.7.2，autoload=App，1280×720，canvas_items 拉伸
├── icon.svg                      # 我们自己画的占位图标（不是游戏里的任何贴图）
├── run.sh                        # 启动器：首次自动 --import，然后跑 gui/selftest/dump/shots
├── scenes/
│   ├── main.tscn                 # 根：Main → VBoxContainer → ScreensControl
│   └── screens/*.tscn            # 15 个界面，每个只有「根 Control + 脚本」
├── src/
│   ├── main.gd                   # 实例化 15 个界面、装主题、处理 --selftest/--dump-tree/--shots
│   ├── app/
│   │   ├── app.gd                # ★ 路由 + 命令总线（等价于原版 Rust 的 GraphwarRootNode 那部分）
│   │   ├── commands.gd           # ★ 从 .tscn 抽出来的命令键清单（协议）
│   │   ├── game_state.gd         # 一局的界面侧状态（玩家表、四个选项、模式）
│   │   ├── modes.gd              # 四个选项的循环（4/2/2/6 态，初值与原版一致）
│   │   ├── campaign.gd           # 21 关（占位数据）+ 关卡按钮布局算法（真码复刻）
│   │   ├── replay_index.gd       # 回放列表（只扫目录，不解析格式）
│   │   └── store.gd              # 存档：名字/skin/face/hat + 战役进度（user://）
│   ├── core/
│   │   ├── core_bridge.gd        # ★ 内核接缝：有内核用内核，没有就退到界面预览
│   │   └── expr.gd               # 只用来预览的表达式求值器（照抄内核的语言怪癖）
│   ├── ui/
│   │   ├── theme_factory.gd      # 主题 + 字体（系统字体，不打包任何字体文件）
│   │   ├── gw_button.gd          # GwButton：等价于原版 GraphwarButton 的「契约」
│   │   ├── icons.gd              # 图标：运行时现画 SVG（149 个原版 svg 一个没用）
│   │   ├── ui.gd                 # 积木：frame/dialog/margin/vbox/label/ad_bar/exit_pair…
│   │   └── screen.gd             # 界面基类：build()/on_enter()/字号缩放
│   ├── play/
│   │   ├── plane_view.gd         # 战场自绘（坐标轴 + 士兵 + 弹道 + 落点）
│   │   ├── function_input.gd     # 函数输入框（自绘，等价于 Rust 的 GraphwarFunctionInput）
│   │   └── player_showcase.gd    # 玩家预览（skin/face/hat）
│   └── screens/*.gd              # 15 个界面的实现
└── tools/
    ├── selftest.gd               # 93 条断言
    ├── screenshots.gd            # 逐界面截图
    └── shots.sh                  # Xvfb 包装
```

### 2.1 路由（`App`）

原版：`GraphwarRootNode/VBoxContainer/ScreensControl` 下面**同时挂着 15 个界面**，
只切 `visible`（`docs/92` §3 实测）。本项目照抄这个结构：

* `App.show_screen(name)` —— 7 个基础界面互斥切换（`MainMenu`/`PlayScreen`/`Campaign`/
  `LanLobby`/`GameSetup`/`PlayerScreen`/`ReplayLobby`）；
* `App.open_popup(name)` / `close_popup(name)` —— 8 个弹窗叠加在上面
  （`ChatPopUp`/`JoinIpPopUp`/`ConnectingPopUp`/`DisconnectedPopUp`/`JoinLevelPopUp`/
  `AddBotPopUp`/`EndGamePopUp`/`ConfirmPlayExitPopUp`）。7 + 8 = 15，与原版界面数一致；
* 换基础界面时先关掉所有弹窗（原版行为未逐条观测，这里取"不叠加"的简单语义）。

界面节点名**沿用原版**（`ConfirmPlayExitPopUp` 对 `confirm_exit_pop_up.tscn`），
方便与 `docs/92` §3 的表逐行对照。

### 2.2 命令总线（这是本工程的核心）

```gdscript
# 任何一个 GwButton 按下 → GwButton._fire(input_text) → App.command(input_text)
btn.input_text = "add_bot_3"
```

`input_text` 不是我们发明的：**恢复出的 31 个 `.tscn` 里每个按钮都有它**，
取值是小写命令键（还有 3 种特例：键盘键直接是字符、关卡按钮是 `level_<n>`、
Bot 按钮是 `add_bot_<0..5>`）。全量清单见 §4。原版里这串字符串由 Rust 收到并分发；
本项目由 `App.command()` 分发。

### 2.3 与内核的接缝（`core_bridge.gd`）

```gdscript
GwCore.calculate_path(expr_text, start, direction)
# → { ok, points: PackedVector2Array, offset: float, source: "kernel"|"preview", note }
```

* 先 `ClassDB.class_exists("GW2Rebuild")`；有该类且**有** `path_calculate` 方法 → 用内核；
* 否则退到 `_preview()`：按固定 `dx=0.02` 取样折线 + 修正常数 `c = y0 − f(x0)`，
  边界照样用 `|x| ≤ 25`、`|y| ≤ 15`（`AGENTS.md` 硬事实）；
* 界面上**必须**把 `source` 显示出来（PlayScreen 底部状态行会打印"弹道来源：preview…"），
  不允许把预览说成内核算的弹道。

**接入真内核时只需在内核侧加一个方法**（约定）：
`GW2Rebuild.path_calculate(expr: String, x0: float, y0: float, direction: float) -> Dictionary`，
返回 `{ok, points, offset, error}`。调用方（PlayScreen）不用改。

---

## 3. 15 个界面：结构依据 ↔ 实现状态

图例：✅ 结构与交互都实现；🔶 结构实现、行为是占位（§7 登记）；❌ 没做。

| # | 界面 | 结构依据 | 状态 | 说明 |
|---|---|---|---|---|
| 1 | **MainMenu** | `92` §4.1（25 节点） | ✅ | 4 个按钮文本/`input_text` 与原版一致；标题字号 = 宽/8（`TitleSize.gd`）；版本行显示本客户端版本 + 内核是否加载 |
| 2 | **Campaign** | `92` §4.2、`101` §4.1 | ✅ / 🔶 | 21 个按钮按**真码算法**运行时生成（`level_0..20`，字号 = 宽/15，碰边回弹，16:9 下 **7+7+7 三行**）；关卡数据是占位（§7） |
| 3 | **JoinLevelPopUp** | `92` §4.3（42 节点） | ✅ / 🔶 | `LevelTitleLabel` + 只读说明 + 四个模式图标 + Cancel/Start；说明栏明写"关卡数据未知" |
| 4 | **LanLobby** | `92` §4.4（32 节点） | ✅ / 🔶 | Local/Global 切换（Global = 5 条静态占位房间）、Create/Join IP、表头；联网不做 |
| 5 | **GameSetup** | `92` §4.5（47 节点）、`104` §4.3 | ✅ / 🔶 | 房间名/聊天/锁/`GameIpLabel`；玩家行含 5 个士兵槽与 4 个操作按钮；四个选项真循环（4/2/2/6）；AddBot/Start |
| 6 | **AddBotPopUp** | `92` §4.6（45 节点） | ✅ | 6 档按钮 `add_bot_0..5`，图标现画；难度↔图标对应关系**未观测**（tooltip 里写明） |
| 7 | **JoinIpPopUp** | `92` §4.7（31 节点） | ✅ / 🔶 | 地址**必须** `host:port`（照抄原版校验："只写 IP 时毫无反应"） |
| 8 | **ConnectingPopUp** | `92` §4.8（28 节点） | 🔶 | 连接是**模拟**的：1.2 s 后转 Disconnected |
| 9 | **DisconnectedPopUp** | `92` §4.9（28 节点） | ✅ | 文本 `You have been disconnected.` |
| 10 | **PlayScreen** | `92` §4.10（106 节点）、`101` §5.2 | ✅ / 🔶 | 左右各 18 键、Fire、Timer、`f(x) =`、Offset、附加按钮、战场自绘；命中判定是占位（§7） |
| 11 | **ConfirmPlayExitPopUp** | `92` §4.11（31 节点） | ✅ | `Are you sure you want to exit?` / No / Yes；`ClipButton` 只是动画层（`mouse_filter=IGNORE`） |
| 12 | **ChatPopUp** | `92` §4.12（7 节点）、`101` §4.3 | ✅ / 🔶 | `ChatBox.gd` 的行为照抄（点聊天区聚焦输入框）；消息只显示本地 |
| 13 | **EndGamePopUp** | `92` §4.13（69 节点） | ✅ | `YOU lOST`（原版原样，小写 l）+ `Team N Victory!` + 10 个展示槽 |
| 14 | **PlayerScreen** | `92` §4.14（64 静态 + 244 动态） | ✅ / 🔶 | 三个页签标题含 U+2009；皮肤 15 按钮 / 脸 15 格 / 帽子 27 格 ⇒ 网格共 **141** 个按钮（+3 静态 = 144）；改名写存档；内购不做 |
| 15 | **ReplayLobby** | `92` §4.15（13 节点） | ✅ / 🔶 | 扫 `user://replays/` 列条目；回放**不解析也不播放** |

界面之外还实现了原版按需实例化的"行"结构：玩家行（`team1_entry`/`team2_entry` 的等价物）、
房间条目（`game_room_entry`）、回放条目（`replay_entry`）、商店格子（`shop_button_entry`）。

---

## 4. 命令键清单（从 `.tscn` 抽出来的协议）

提取方法：把 31 个 `.tscn` 里所有 `input_text` / `long_press_input_text` 收集去重
（`grep -h input_text *.tscn | sort | uniq -c`）。下表就是**全部**键；"实现"列是本工程的状态。

| 类别 | 键 | 实现 |
|---|---|---|
| 切界面 | `campaign` `player_settings` `multiplayer` `replay` | ✅ |
| 返回 | `exit_campaign` `exit_lan_lobby` `exit_game_setup` `exit_player_settings` `exit_replay_lobby` `exit_play` | ✅ |
| 大厅 | `show_local` `show_global` `create_game` `join_ip` `join_join_ip` `cancel_join_ip` `cancel_connecting` `ok_disconnected` | ✅（`show_global` 列表来源未知→静态占位；`join_join_ip` 只校验格式） |
| 设置 | `lock_game` `add_bot` `start_game` `toggle_chat` | ✅（`start_game` 无对手时**不开始**，照抄原版静默行为 + 我们加一句提示） |
| 关卡 | `level_0` … `level_20` `start_level` `cancel_level` | ✅ |
| 加 Bot | `add_bot_0` … `add_bot_5` `cancel_add_bot` | ✅ |
| 对局中 | `ask_exit_play` `cancel_exit_play` `toggle_names` `toggle_view_functions` `toggle_mute` `fire` | ✅（`toggle_mute` 只切图标：没有音频资源） |
| 回放 | `play_replay` `play_pause` `⏮︎` `⏭` `save_screenshot` `save_summary` `save_clip` | 🔶 键名已观测，语义未观测；本项目只给提示 |
| 内购 | `unlock_skins` | 🔶 Linux 端不做广告/内购（用户要求） |
| 键盘 | `C` `π` `e` `1`…`9` `0` `.` `(` `)` `+` `-` `*` `/` `^` `x` `y` `y'` `abs(` `ln(` `sqrt(` `sin(` `cos(` `tan(` `exp(` `⌫` `⌦` `←` `→` `⇤` `⇥` | ✅ 编辑语义见 §4.1 |

### 4.1 键盘编辑语义（**推断**，原版未逐条观测）

原版由 Rust 的 `GraphwarFunctionInput` 处理这些键，行为没有观测记录。本项目按下表实现
（依据：键名字面含义 + 函数文法）：

| 键 | 行为 |
|---|---|
| `C` | 清空 |
| `⌫` / `⌦` | 删光标左 / 右一个字符 |
| `←` / `→` | 光标左 / 右移 |
| `⇤` / `⇥` | 光标到行首 / 行尾 |
| 其余 | 按字面插入（`sin(` 会连括号一起插入） |

两点补充：`y` 键**恒为 disabled**（原版两种模式下都如此，`docs/92` §4.10 观测；
原因未知，`docs/92` §8 列为未知）；输入长度上限 128 是我们定的（原版上限未观测）。

---

## 5. 已实现的交互路径（可当验收脚本读）

```
MainMenu
 ├─ Campaign → (点 level_i) → JoinLevelPopUp → Start → PlayScreen(campaign)
 │                                   └ Cancel → Campaign
 ├─ Battle → LanLobby
 │            ├ Local/Global 切列表
 │            ├ Create Game → GameSetup
 │            │     ├ Axis/Function/Turn/TurnTime 四键循环（4/2/2/6 态）
 │            │     ├ AddBot → AddBotPopUp → add_bot_N（6 档）→ 回到 GameSetup
 │            │     ├ 玩家行：踢人 / 加士兵 / 减士兵 / 换队
 │            │     ├ LockButton 切锁图标、RoomNameLineEdit 改房间名
 │            │     └ Start（无对手时不开始，原版行为）→ PlayScreen
 │            └ Join IP → JoinIpPopUp →（host:port 校验）→ Connecting → Disconnected
 ├─ Player → PlayerScreen（改名 / 三个页签选外观）→ 返回
 └─ Replays → ReplayLobby（列 user://replays/）→（PlayReplayButton）→ PlayScreen(replay, 空)
```

对局内：键盘输入 → Fire（或回车）→ 弹道画在战场上 + 修正常数显示 →
命中判定 → 换手（顺序制）或本轮结算（同时制）→ 一方全灭 → EndGamePopUp → 回主菜单。

---

## 6. 字体与图标（用户明确要求处理的部分）

### 6.1 字体：**不打包任何字体文件**

原版用 7 个 `.ttf`（游戏资产，不可分发）。本工程：

1. 若设了 `GW2_FONT_PATH=/abs/path/xxx.ttf` 且文件存在 → 用它（**你自己负责该字体的许可**）；
2. 否则用**本机已装字体**，候选链：`Noto Sans` → `DejaVu Sans` → `Liberation Sans` →
   `Adwaita Sans` → `FreeSans`，逐个用 `OS.get_system_font_path()` 找路径，
   找到就用 `FontFile.load_dynamic_font()` 加载（**不复制文件进仓库**）；
3. 都找不到 → `SystemFont` 兜底（并 push_warning）。

**许可证结论**：仓库里没有字体文件，因此没有字体再分发问题；运行时用的是系统里已经装好的字体，
各自遵循其自带许可（Noto/DejaVu/Liberation 都是 OFL/Apache/类似宽松许可，但**我们不复制它们**）。
原先版那 7 个 ttf，本项目**不用、不抄、不声称**。

⚠️ 实测坑：`SystemFont` 在控件里正常，但 `CanvasItem.draw_string()` 画出来是**缺字方块**；
而且每次 `new` 一个 `FontFile` 去 draw 也拿不到已光栅化的字形。所以字体对象**全局缓存一份**
（`theme_factory.gd::_font_cache`），自绘文本（函数输入框、战场上的士兵名字）才能正常显示。

### 6.2 图标：全部现画

原版 149 个 `.svg` 一个都没用。`src/ui/icons.gd` 用一个小 SVG 字符串生成器画
**同语义的几何占位图**（返回箭头、加/减/叉、方向键、`axis_option_N`、`turn_*`、
`bot_0..5`、`skin_N`/`face_N`/`hat_N`、`view_names`、`no_chat`…），
经 `Image.load_svg_from_string()` 转成 `ImageTexture` 并缓存。

因此：**按钮位置、尺寸、语义与原版一致，但美术观感不同**。这是有意的取舍（版权 + 无网络下载）。

### 6.3 语言

原版**没有多语言支持**（界面文本硬编码英文）。本项目界面文本保持原版英文；
只有 tooltip / 状态行 / 文档里的解释用中文。**不做 i18n**。

---

## 7. 有意偏离、占位与自定规则（全部登记）

> 判据：凡是"原版行为没有观测证据、由我们决定"的东西，都必须出现在这张表里。

| # | 项 | 我们的做法 | 原版状态 | 影响的界面 |
|---|---|---|---|---|
| 1 | ~~弹道~~ | ✅ **已消除**：`GW2Rebuild.path_calculate` 已暴露并被客户端使用，`source=kernel`，等弦长 0.01（`docs/116`） | 等弦长 0.01 二分采样 | PlayScreen |
| 2 | ~~命中/致死~~ | ✅ **已消除**：`GW2Rebuild.hit_test` 走 `graphwar_shot_calculator::settle::resolve_points`（掩膜 + 数据字段半径），旧 0.8 近似已删（`docs/120`）。⚠️ 残留：原版按实体 id 排除射手，`game.rs::fire` 仍有一条 1e-12 位置排除（`docs/122`） | 掩膜逐点采样、穿过即死、友军也会死 | PlayScreen |
| 3 | ~~关卡数据~~ | ✅ **已消除**：21 关标题 / 描述 / 参数（障碍模式、`min_count`、士兵数、目标数、Bot 难度、回合上限）**全部解出**，并与 385 份回放交叉验证 21/21（`docs/118`）。⚠️ 客户端已接入（`docs/125`）；**仅障碍物仍不画**（公式有、原版的随机流没有） | 硬编码在 `level_info()`（`0x4ad380`） | Campaign / JoinLevelPopUp |
| 4 | ~~关卡解锁~~ | ✅ **规则已还原**：`campaign.save` = 单行 `level_<n>`（0 基、**无尾换行**、只增不减、启动不创建），写点在 `0x4183b0`；客户端是否已接入见 `docs/124` | 同左（`docs/118` 附录 A） | Campaign |
| 5 | **Bot 分队** | 第 1 个 Bot 去对面（team 1），之后按人数奇偶 | 未观测 | GameSetup |
| 6 | **Bot 名字 ↔ 难度** | 固定把 `ChatGPT/Wolfram/Alpha Zordon/Multivac/Deep Blue/Deep Thought` 依次给 0..5 档 | **对应关系未观测**（`docs/92` §4.6 明写），且名字池与难度不是 1:1 | AddBotPopUp |
| 7 | **网络** | 没有网络层：`join_join_ip` 只校验 `host:port`，然后模拟"连接中→断开" | 真实 TCP 握手已复原（`docs/33`、`src/_scaffold/protocol.rs`），未接进界面 | LanLobby / 三个连接弹窗 |
| 8 | **公网房间列表** | 5 条静态占位（`US Room 1/3/4`、`EU Room 1/2`，名字照抄观测） | 来源未知（`docs/92` §8） | LanLobby |
| 9 | **聊天** | 只显示在本地，不发网络包 | 走协议（`docs/24`） | ChatPopUp |
| 10 | **回放** | 列 `user://replays/` 的文件名；不解析、不播放 | 格式已观测（`docs/78`），播放界面未观测 | ReplayLobby / PlayScreen |
| 11 | **内购/广告** | 广告位保留 60 px 占位条（写"disabled in this build"），内购按钮只提示 | Linux 端本来就没有（用户要求） | 11 个界面有广告条 |
| 12 | ~~音频~~ | 🔶 **链路已还原、尚未接入**：10 个导入产物四道独立检查全部通过；`humm.wav` 由 Rust 侧 hound 解析（`0x50a080`），音量 `-15f`/`-5f`、32767 归一化、静音走 Master 总线（`docs/111`）。"有一个文件损坏"已查明是**使用者自己的重建脚本**产物，非游戏资源 | 见 `docs/111` | PlayScreen |
| 13 | **结算文案** | 输：`YOU lOST`（原版原文）；赢：`YOU WON`（**占位**，tooltip 注明原文未观测） | 只观测到失败一侧 | EndGamePopUp |
| 14 | **玩家表滚动** | 玩家列表**不用** `ScrollContainer`（原版有） | 原版用 `ScrollContainer` | GameSetup（原因见 §9.1） |
| 15 | **"无对手时 Start"** | 不开始，并且**给一句提示** | 原版是**纯静默**（`docs/22` §5.1） | GameSetup |
| 16 | **键盘编辑语义** | §4.1 那张表（推断） | 未观测 | PlayScreen |
| 17 | **函数输入上限** | 128 字符 | 未观测 | PlayScreen |
| 18 | **字号上限** | 只在小窗口下钳到最小 10 px（不设上限） | 原版无观测 | 全部 |
| 19 | **存档** | `user://gw2_client.cfg`（名字/外观/战役进度） | 原版存档格式未解，我们**不读不写**它 | PlayerScreen / Campaign |
| 20 | **回合时限到点** | 自动换手 | 时间基准已观测（`docs/42`），自动换手行为未逐条观测 | PlayScreen |

---

## 8. 验证

### 8.1 三条命令（都在 `client/` 下跑，或直接 `bash client/run.sh <mode>`）

```bash
# 0) 首次：让 Godot 扫资源，生成 .godot/ 与 global_script_class_cache.cfg
#    （**必须**先跑一次，否则 class_name 解析不到，见 §9）
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . --import

# 1) 能启动
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . --quit   # 退出码 0、无 ERROR

# 2) 自动验收（93 条）
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . -- --selftest

# 3) 打印节点树（每行 `@@路径|类|visible/is_visible_in_tree|text|cmd+rect`）
../../../../tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . -- --dump-tree
```

### 8.2 自检覆盖了什么（`tools/selftest.gd`，93 条）

| 组 | 断言要点 |
|---|---|
| 启动 | 15 个界面全部实例化；只有 `MainMenu` 可见 |
| 主菜单 | 4 个按钮的文本与 `input_text` 与原版一致；**真的按一下** Campaign → Campaign 可见 |
| 战役 | 21 个 `level_*` 按钮、命名/文本/命令键一致；布局 3 行 × 7 个、字号 = 宽/15；点 level_0 → JoinLevelPopUp 且标题 `Level 0`；说明里含 `PLACEHOLDER` |
| 玩家界面 | 3 个页签、标题含 U+2009；皮肤 15 / 脸 15 / 帽子 27 格；网格 141 个按钮（+3 静态 = 144）；改名写进存档 |
| 大厅 | 公网列表 5 条；切回本地为空 |
| 对局设置 | 时间档初值 1m，**按 2 次 → 3 分钟档（180 s）**；Axis 4 档循环回初值；无 Bot 时 Start 不开始；加 Bot 后玩家表 2 人且 `can_start()`；空地址不会进 ConnectingPopUp |
| 对局 | 左右键盘各 18 键；`YButton` 恒 disabled；键盘敲出 `sin(x)`；Fire 后弹道点数 > 1；修正常数非空；3 分钟档时刻是 `02:xx`；`toggle_names` 生效；退出确认链 |
| 结算 | 触发后可见、`YOU lOST`、有胜方文案、10 个展示槽；`exit_play` 回主菜单 |
| 字体/图标 | 字体资源存在（系统字体）；两个图标生成成功 |

判据：`SELFTEST n passed, 0 failed` 且 `@@SELFTEST_DONE code=0`。
有看门狗：120 s 没跑完就以退出码 3 结束（CI 不会挂死）。

### 8.3 截图（布局核对）

```bash
bash client/tools/shots.sh          # Xvfb + opengl3，输出 11 张 PNG 到 .agent-work/client/shots/
```

截图是我们自己界面的画面（**不含任何游戏资产**）。本轮据此修掉了 6 个真实布局 bug，
都记在 §9.2 —— 这些坑只有真渲染才看得见，`--headless` 看不出来。

### 8.4 与 `docs/92` 台账的对照结果

* 15 个界面、每个界面的**关键控件名**（`PlayButton` / `ShowLocalButton` / `LevelTitleLabel` /
  `GraphwarFunctionInput` / `YButton` / `TimerLabel` / `OffsetLabel` / `SelfVictoryLabel`…）
  全部沿用原版命名，可直接与 `docs/92` 的表对照；
* 差异（我们**没有**逐节点复刻的部分）：
  * `docs/92` 的 858 节点/283 按钮是原版运行时总数；本项目的节点数**不同**（更少），
    因为我们没有广告/IAP 插件节点，且没有把每个 `MarginContainer` 都摆一遍；
  * 9 条 `theme_override` 里，我们只复刻了结算框的 70/40 与玩家界面 32/60 的字号，
    4 条 `StyleBoxFlat`（弹窗标题底）用主题统一代替。

---

## 9. 复现与踩过的坑

### 9.1 两个设计取舍

**（1）界面用代码搭，不手写 `.tscn`。**
好处：不用搬游戏资产；所有布局数值集中（`campaign.gd` 的布局算法、`modes.gd` 的循环表）；
运行时动态内容（21 个关卡按钮、244 个玩家界面节点、57 条回放条目）本来就该代码建。
代价：编辑器里打开场景只看到根节点；审阅时读 `.gd` 而不是 `.tscn`。

**（2）玩家列表不用 `ScrollContainer`。**
`ScrollContainer` 的最小尺寸**等于内容的最小尺寸**，两列玩家表（每行 7 个控件）会把整个
GameSetup 的最小宽度顶到 1700+ px，界面直接被推出 1280×720。代价：玩家很多时不能滚动。
其余界面的 `ScrollContainer`（皮肤/帽子、说明文本、回放列表）保留，因为它们内容方向单一。

### 9.2 真渲染才看得见的 6 个坑（每个都真实发生过）

1. **`Label` 默认开自动换行会炸布局**：开了 `AUTOWRAP_WORD_SMART` 的 Label 在水平容器里
   按"宽度 0"估算最小高度 —— `FxLabel` 的最小高度变成 **424 px**，整个 PlayScreen 溢出屏幕。
   ⇒ `GwUI.label()` 默认关闭自动换行，需要换行的显式传 `autowrap=true`。
2. **`MarginContainer` 默认竖直 EXPAND**：空白的 `ExtraOptionPad` 和战场**对半分高度**。
   ⇒ `GwUI.margin()` 默认竖直 `SIZE_FILL`，要弹性自己设。
3. **`spacer()` 默认水平 EXPAND**：玩家表中间那条 10 px 分隔条把 Team 2 顶出屏幕。
   ⇒ `spacer()` 默认 `SIZE_FILL`，要弹簧自己设。
4. **普通 `Control` 不是容器**：往 `GraphwarSetupControl`（普通 Control）里塞 VBox 时，
   必须自己 `set_anchors_preset(FULL_RECT)`，否则内容缩成最小尺寸（GameSetup 曾整屏塌陷）。
5. **`AspectRatioContainer` 会把空间分给每个子节点**：战场（`GraphwarPlane`）与函数列表
   都放进去，结果两者各占一半、战场只剩 380 px 宽。⇒ 改成"外层普通 Control + aspect 只放战场 +
   覆盖层叠在上面"。
6. **`SystemFont` + `draw_string` 画不出字形**（见 §6.1），并且**每次 new 一个 FontFile 也不行**。

另外两个非渲染坑：

7. **`class_name` 需要先 `--import`**：全新 clone 直接 `--headless` 跑，`GwStore`/`GwCommands`
   这些全局类解析不到（`global_script_class_cache.cfg` 还没生成）。`run.sh` 已自动处理。
8. **解析器自己的 bug**：隐式乘法插到了函数名和 `(` 之间（`sin(` → `sin * (`），
   于是 `sin(x)` 求值全是 NAN；selftest 的"Fire 后弹道点数 > 1"就是这么抓出来的。

### 9.3 一条对 `docs/101` §4.1 措辞的更正（**新观测**）

`docs/101` §4.1 把 21 个关卡按钮的布局概括为"**8 列 4 行**"。按恢复出的真源码，
`pos.y += pos_y_increment` **只在"碰边回弹"分支里**执行，所以 16:9 屏上实际是
**7 个一行 × 3 行 = 21**（x 从 `w/8` 走到 `7w/8`，bounds 是 `1/10` 与 `9/10`）。

本工程按真码实现（`campaign.gd::layout_buttons`），`selftest` 断言的也是"3 行 × 7 个"。
`docs/101` 的伪源码本身没问题，只是那句概括偏粗；`8` 与 `4` 是 **x/y 的步长分母**，
不是行列数。这条不改 `101`（本轮只加 `108`），但值得记一笔。

---

## 10. 仍未做 / 拿不到

| 项 | 状态 | 缺什么 |
|---|---|---|
| 内核 `path_calculate` | ✅ **已接** | `docs/116`；`client/gw2rebuild.gdextension` + `cargo build` 出的 `.so`（`.so` 已 gitignore） |
| 内核 `hit_test`（掩膜判定） | ✅ **已接**（`docs/120`） | 状态行显示 `判定来源：kernel / 内核 hit_test（掩膜 1000×600 + 士兵半径）`；拔掉 `.so` 才退到 `preview` |
| 真实掩膜/障碍物绘制、摧毁（挖洞） | ❌ | `src/plane.rs` 规则未暴露；本项目战场不画掩膜 |
| 真实杀伤规则 | 🔶 占位 | 见 §7 #2 |
| 21 关的关卡数据 | ❌ **未知** | 关卡布局/目标/通关条件从未被观测（`docs/103`） |
| 回放解析与播放 | ❌ | 格式见 `docs/78`；播放界面从未被观测（`docs/92` §8） |
| 联机（建房/加房/握手/聊天协议） | ❌ | 协议已复原（`docs/24`/`33`/`src/_scaffold/protocol.rs`），界面侧未接 |
| 段位、皮肤解锁、内购 | ❌ | 内购 Linux 端没有；段位数据未知（玩家行 `RankIcon` 是空占位） |
| AI 决策（Bot 怎么选函数） | ❌ | 在 Rust（`src/graphwar_function_evolutionator.rs`）；本项目 Bot 只是"站着挨打"的实体 |
| 同时回合制的真实结算顺序 | 🔶 | 现在是"本轮所有弹道按发射顺序结算"；真实规则见 `docs/86`/`src/graphwar_shot_calculator/settle.rs` |
| 音频 | ❌ | `.pck` 里一个音频文件损坏；资产不入库 |
| 9 条 `theme_override` 的完整复刻 | 🔶 | 只做了字号，没做 4 个 `StyleBoxFlat` |
| 原版 858 节点级的逐节点复刻 | ❌ | 见 §8.4：本项目是"同名关键控件 + 等价布局"，不是节点级克隆 |

---

## 11. 资产纪律（怎么核对"确实没进游戏资产"）

```bash
cd gw2-rebuild
git ls-files client | grep -E '\.(ttf|otf|pck|scn|svg|gdc)$'   # 只应看到 client/icon.svg（我们画的）
git ls-files client | grep -c ''                                # 文件数
grep -rniE 'GridContainer|GraphwarButton' client/src | wc -l    # 只有注释里提到原版类名
```

* `client/icon.svg` 是本项目自己画的占位图标（一条曲线 + 一个弹丸点），与游戏贴图无关；
* `client/` 下**没有**任何从 `rsc/` 或 `.agent-work/tools/recovered/` 复制来的文件；
* 恢复出的 `.tscn`/`.gd`/`.ttf` 只被**读取**（参考结构与字符串），从未复制进仓库；
* 文档里引用的界面文本（`YOU lOST`、`Are you sure you want to exit?` 等）是
  `docs/92` 早已记录的观测事实，属于"引用数据"，不是资产。

---

## 12. 复现清单（从零到能跑）

```bash
cd gw2-rebuild/client
G=../../../../tools/Godot_v4.7.2-stable_linux.x86_64
export XDG_DATA_HOME=../../../../.agent-work/client/data
export XDG_CACHE_HOME=../../../../.agent-work/client/cache
export XDG_CONFIG_HOME=../../../../.agent-work/client/config

$G --headless --path . --import                        # 必须一次
$G --headless --path . --quit                          # 启动检查（code 0）
$G --headless --path . -- --selftest                   # 93 条断言，code 0
$G --headless --path . -- --dump-tree | head            # 节点树
bash tools/shots.sh                                     # 截图（要 Xvfb）
```

自检判据（任一条不成立就说明改动坏了东西）：

* `--quit` 的 stderr 里**没有** `ERROR:`/`SCRIPT ERROR:`；
* `SELFTEST 93 passed, 0 failed`；
* `--dump-tree` 里 15 个界面各有一行 `@@…/ScreensControl/<界面名>`，且只有 `MainMenu` 是 `11`；
* 截图里 PlayScreen 的战场能看见两个士兵（一蓝一橙）与顶部函数列表。
