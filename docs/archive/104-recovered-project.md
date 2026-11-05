# 104 · GDRE 整项目恢复：静态场景属性 + 与运行时台账对拍

状态：**现行**。本文补 [`docs/92`](92-ui-inventory.md) 缺的那一半：92 是**运行时**枚举
（只有节点路径 / `get_class()` / 文本 / 脚本），本文是**静态**属性（锚点、offset、size flags、
theme override、九宫格、`[connection]`），并做逐项对拍。

> **资产警告（先读）**
> GDRE 恢复出来的 `.tscn` / `.gd` / `.tres` / 贴图 / 字体**全部是游戏资产**。
> 它们只放在 `.agent-work/tools/recovered/`（外层仓库 gitignore），**绝不提交**，
> 连单个 `.tscn`、`.gd` 也不进 git。本仓库只提交**工具**（`tools/tscn_inventory.py`、
> `tools/gdc_verify_source.py`）和**结论**；文档里只引用够说明问题的短片段（≤ 几行）。

---

## 0. 一页结论

| 项 | 结果 | 证据等级 |
|---|---|---|
| GDRE Tools v2.7.4 恢复 | 成功：**31 `.tscn` / 105 `.gd` / 22 `.tres`**，另有 149 svg、167 ctex、17 png、7 ttf | 观测（产物已落盘，1171 个文件） |
| 必需参数 | `--bytecode=4.5.0`（本包字节码版本 101 = 4.5.0-stable） | 观测（缺它直接报错） |
| 游戏场景 | **25 个**（`rsc/` 根目录 25 个 `.scn` → 25 个 `.tscn`），另 6 个是 AdMob 插件样例 | 观测 |
| 静态节点 | 25 个场景共 **1374 个节点条目**；其中 `main.tscn` 展开 15 个界面占 590，**去重后 794 个实例节点** | 观测 |
| 15 个界面静态节点 | **580 个**（不含 `main.tscn` 的根/容器/holder 共 10 个） | 观测 |
| 静态 vs 运行时（33 个快照） | 15 个界面**全部 100% 命中**；匹配节点的 class / script **0 处不一致**；**静态独有 0 个**（已实例化的界面） | 观测（`tools/tscn_inventory.py --runtime`） |
| 运行时动态创建 | boot 快照 **268 个**，逐类可解释（§5.3）；`replay_lobby` 快照 1239 个、`setup_six_bots` 330 个 | 观测 |
| `[connection]` | 游戏 25 个场景里 **0 条**；31 条全部在 AdMob 插件样例场景 | 观测 |
| GDScript 侧连接 | 52 个非插件脚本共 **65 处 `connect()`**（32 处 `get_tree().root.size_changed`…） | 观测 |
| 反编译源码可信度 | **105/105 脚本逐 token 与 `.gdc` 字节码一致** | 逐位对照（`tools/gdc_verify_source.py`） |
| `docs/101` 的 5 个行为脚本 | **0 处更正**（5/5 与我用字节码推的伪源码语义一致） | 观测 |

三句话：

1. **静态能补的比想的多**：`theme_override_font_sizes` 有 9 处、`custom_minimum_size` 有 29 处、
   锚点分布 51 个 `FULL_RECT`——这些都是 92 的运行时 dump 拿不到的（dump 只打印文本/类名）。
2. **对不上的是"动态创建"，不是解析错**：运行时比静态多出来的节点可以用 3 条规则全部解释
   （玩家界面的皮肤格子、21 个关卡按钮、PlayScreen 的 3 个绘图节点），见 §5.3。
3. **连接不在场景里**：25 个游戏 `.tscn` 一条 `[connection]` 都没有——一切连接都在
   `.gd` 的 `connect()` 与 Rust 里。这解释了 92 §2.3 "只剩 27 条人为连接"的口径，见 §6。

---

## 1. 环境配方（照抄即可）

```bash
export XDG_DATA_HOME=$PWD/gdre-data XDG_CACHE_HOME=$PWD/gdre-cache XDG_CONFIG_HOME=$PWD/gdre-config
# ↑ 三个变量必须设，否则 GDRE 会去写 ~/.local 并失败（实测）

/home/user/MyProject/gw2/.agent-work/tools/gdre_tools.x86_64 --headless \
  "--recover=/home/user/MyProject/gw2/gw2-rebuild/rsc" \
  --bytecode=4.5.0 \
  "--output=$PWD/recovered"
```

坑（都踩过）：

| 现象 | 原因 | 处理 |
|---|---|---|
| `--bytecode is required` | 恢复/反编译都要显式给字节码版本 | `--bytecode=4.5.0`（游戏的 101 对应 4.5.0-stable） |
| `--bin-to-txt` 对 `.scn` 静默无输出 | 那条路不适用于二进制场景 | 用 `--recover`，它会把 `.scn` 写成 `.tscn` |
| GDRE 写 `~/.local` 失败 | 家目录只读/受限 | 三个 `XDG_*` 指到工作区 |
| `recovered/` 里同时有 `.gdc` 和 `.gd` | GDRE 保留原 `.gdc`，`.gd` 是反编译产物 | 两者都用得上：`.gdc` 是字节码真值，`.gd` 是可读渲染 |

工具路径与产物路径都是 `.agent-work/`（外层 `gw2/` 仓库 gitignore，见 `.gitignore:64`）。

---

## 2. 产物清单

```
.agent-work/tools/recovered/          1171 个文件
├── 31 × *.tscn      25 游戏场景 + 6 AdMob 样例
├── 105 × *.gd       ← 反编译 GDScript（与 105 × *.gdc 一一对应）
├── 22 × *.tres      主题 / StyleBox / 字体资源
├── 149 × *.svg / 17 × *.png / 167 × *.ctex / 7 × *.ttf   界面贴图与字体
└── addons/…         AdMob / android_IAP 插件（53 个 .gd）
```

**不要提交**：`*.tscn`、`*.gd`、`*.tres`、`*.svg`、`*.png`、`*.ttf`、`*.ctex`、`*.gdc`、`*.scn`。
本仓库已有的规则（`gw2-rebuild/.gitignore`：`rsc/`、`*.pck`、反编译产物）同样适用；
本轮新增的工具只**读**这些文件。

---

## 3. 静态解析：工具与口径

```bash
# 摘要（每个场景：节点数 / [connection] 数 / StyleBox 数 / 实例化节点数）
../.venv/bin/python tools/tscn_inventory.py --all ../.agent-work/tools/recovered

# 单个场景的完整控件表（Markdown）；输出放 .agent-work/，不要用 /tmp（per-command tmpfs）
../.venv/bin/python tools/tscn_inventory.py --scene <recovered>/play_screen.tscn \
    --md ../.agent-work/play_controls.md

# 与运行时 dump 对拍（boot 快照）
../.venv/bin/python tools/tscn_inventory.py --all <recovered> \
    --runtime ../.rea-work/ui/boot.log --snapshot boot_all_screens
```

解析口径（都是 Godot 4 文本场景的实测语法）：

* 段：`[gd_scene]` / `[ext_resource]` / `[sub_resource]` / `[node]` / `[connection]` / `[editable]`；
* 节点行的 `parent="."` 表示"场景根的子节点"，所以工具内部路径一律带根名前缀；
* `instance=ExtResource("id")` 会被**递归展开**：读被实例化场景的根属性并合并子节点
  （`main.tscn` 因此展开成 590 个节点、含 15 个界面）；
* 属性的右值用一个小型递归下降解析器读：字符串、`Vector2(..)`、`Color(..)`、
  `ExtResource("id")`、`SubResource("id")`、`&"StringName"`、数组、字典、`true/false/null`；
* `script = ExtResource("id")` 在**解析该文件时**就解析成 `res://Xxx.gd` ——
  实例化节点的 ext id 属于**子场景**的 ext 表，晚一步解析会张冠李戴（这个 bug 真出现过：
  105 个"script 不一致"全部是它造成的）。

**计数口径**：`--all` 会把 `main.tscn` 和 15 个界面场景各算一遍，所以 25 个场景合计 1374；
去掉 `main.tscn` 重复展开的部分后，实际不同的实例节点是 **794 个**（= `main.tscn` 590 + 其余 10 个场景 204）。

---

## 4. 控件表（静态，`docs/92` 缺的字段）

### 4.1 逐场景统计

| 场景 | 节点 | 其中 `GraphwarButton` | theme override | 实例化子场景 |
|---|---|---|---|---|
| `main.tscn`（展开后） | 590 | 121 | 9 | 15 |
| `play_screen.tscn` | 106 | 58 | 0 | 0 |
| `player_screen.tscn` | 64 | 3 | 3 | 0 |
| `endgame_pop_up.tscn` | 69 | 2 | 2 | 0 |
| `GameSetup.tscn` | 47 | 11 | 0 | 0 |
| `add_bot_pop_up.tscn` | 45 | 9 | 0 | 0 |
| `join_level_pop_up.tscn` | 42 | 8 | 0 | 0 |
| `lan_lobby.tscn` | 32 | 7 | 0 | 0 |
| `join_ip_pop_up.tscn` | 31 | 4 | 1 | 0 |
| `confirm_exit_pop_up.tscn` | 31 | 4 | 1 | 0 |
| `connecting_pop_up.tscn` | 28 | 3 | 1 | 0 |
| `disconnected_pop_up.tscn` | 28 | 3 | 1 | 0 |
| `main_menu.tscn` | 25 | 4 | 0 | 0 |
| `replay_lobby.tscn` | 13 | 2 | 0 | 0 |
| `campaign.tscn` | 12 | 2 | 0 | 0 |
| `chat_pop_up.tscn` | 7 | 1 | 0 | 0 |
| `team1_entry.tscn` | 48 | 4 | 0 | 0 |
| `team2_entry.tscn` | 46 | 4 | 0 | 0 |
| `function_buttons.tscn` / `_no_control` | 31 / 31 | 30 / 30 | 0 | 0 |
| `replay_entry.tscn` | 21 | 5 | 0 | 0 |
| `game_room_entry.tscn` | 20 | 5 | 0 | 0 |
| `shop_button_entry.tscn` | 5 | 3 | 0 | 0 |
| `name_label.tscn` / `line_2d.tscn` | 1 / 1 | 0 / 0 | 0 | 0 |

（15 个界面的 theme override 计数相加 = 9，与 `main.tscn` 展开后的 9 条一致。）

> 「theme override」只数 `.tscn` 里**写在节点上**的条目；主题资源（`.tres`）里的默认值另算（§8）。

### 4.2 theme override 全表（静态，共 9 条）

| 值 | 出现次数 | 位置 |
|---|---|---|
| `theme_override_font_sizes/font_size = 70` | 1 | `EndgamePopUp/…/SelfVictoryLabel`（"我方胜利"大标题） |
| `= 40` | 1 | `EndgamePopUp/…/TeamVictoryLabel` |
| `= 60` | 1 | `PlayerScreen/…/PendingBlock/VBoxContainer/Label`（未解锁遮罩） |
| `= 32` | 2 | `PlayerScreen/…/SkinBlock/VBoxContainer/Label`、`Label2` |
| `theme_override_styles/normal = StyleBoxFlat` | 4 | 四个弹窗的标题 `Label`（`ConfirmExit` / `Connecting` / `Disconnected` / `JoinIp`） |

这 9 条是**脚本之外**的字号/样式来源：它们不随窗口缩放（脚本只改 `font_size`，
见 `docs/101` §5.2），重建时容易漏。

### 4.3 `custom_minimum_size`（静态，共 29 条）

| 值 | 次数 | 含义 |
|---|---|---|
| `Vector2(0, 60)` | 11 | 每屏底部的 `AdContainer` 固定 60 px 高（**11 个界面都有广告条**） |
| `Vector2(10, 0)` | 11 | 队伍条目/玩家界面的左右 10 px 间隔 |
| `Vector2(0, 10)` | 6 | 玩家界面三页签内的上下 10 px 间隔 |
| `Vector2(100, 0)` | 1 | `PlayScreen/…/OffsetContainer/OffsetLabel` 偏移标签 |

### 4.4 锚点 / size flags 分布（25 个场景，不含 `main.tscn` 展开部分）

| `anchors_preset` | 次数 | Godot 枚举 |
|---|---|---|
| 15 | 51 | `PRESET_FULL_RECT`（铺满父容器） |
| -1 | 30 | 自定义锚点（`anchor_*` 逐项给出） |
| 2 | 15 | `PRESET_BOTTOM_LEFT` |
| 3 | 15 | `PRESET_BOTTOM_RIGHT` |
| （未写） | 其余 | 由父容器的 `layout_mode` 决定 |

`size_flags_horizontal = 3`（`SIZE_EXPAND_FILL`）244 次、`size_flags_vertical = 3` 94 次，
`= 0` / `= 8` 各 1 次（`SIZE_SHRINK_BEGIN` / `SIZE_SHRINK_END`）。

### 4.5 一条完整记录长什么样

`play_screen.tscn` 的一个节点（工具输出，原文）：

```
PlayScreen/MarginContainer/VBoxContainer/PlayMainHBox/PlaneContainer/GamePlane
  type=GraphwarPlane
  anchors: layout_mode=2
  size:    size_flags_horizontal=3, size_flags_vertical=3
  script:  -
```

（完整表格用 `tools/tscn_inventory.py --md` 生成；一屏 106 行、全项目 1374 行，不适合贴进文档。）

---

## 5. 对拍：静态 vs 运行时（33 个快照）

数据源：`.rea-work/ui/*.log` 的 `@@N` 行（`docs/92` §9 的同一批 dump）。

### 5.1 15 个界面（boot 快照）

| 界面 | 静态节点 | 运行时节点 | 静态独有 | 运行时独有 | 运行时独有的构成 |
|---|---|---|---|---|---|
| MainMenu | 25 | 25 | 0 | 0 | — |
| PlayScreen | 106 | 109 | 0 | 3 | `AxisDrawer` / `EntityDrawer` / `EffectDrawer` 各 1 |
| Campaign | 12 | 33 | 0 | 21 | **21 个 `GraphwarButton`（`level_0..level_20`）** |
| LanLobby | 32 | 32 | 0 | 0 | — |
| GameSetup | 47 | 47 | 0 | 0 | — |
| ChatPopUp | 7 | 7 | 0 | 0 | — |
| JoinIpPopUp | 31 | 31 | 0 | 0 | — |
| ConnectingPopUp | 28 | 28 | 0 | 0 | — |
| DisconnectedPopUp | 28 | 28 | 0 | 0 | — |
| JoinLevelPopUp | 42 | 42 | 0 | 0 | — |
| PlayerScreen | 64 | 308 | 0 | 244 | 141 `GraphwarButton` + 42 `Control` + 42 `ColorRect` + 19 `HBoxContainer` |
| ReplayLobby | 13 | 13 | 0 | 0 | 回放条目在 boot 未创建（见 §5.3） |
| AddBotPopUp | 45 | 45 | 0 | 0 | — |
| EndGamePopUp | 69 | 69 | 0 | 0 | — |
| ConfirmPlayExitPopUp | 31 | 31 | 0 | 0 | — |
| **合计** | **580** | **853** | **0** | **268** | （+ `shop_button_entry` 5 个 = 858） |

**匹配节点的 class 与 script 全部一致，0 处不一致。** 名字层面有自动重命名
（`MarginContainer` → `MarginContainer7`），工具按"同序 + 同类型 + 名字去后缀"匹配，
不是按名字硬找。

### 5.2 类计数对账（`main.tscn` 静态 vs boot 快照）

| 类 | 静态 | 运行时 | 差 | 差额来源 |
|---|---|---|---|---|
| `GraphwarButton` | 121 | 283 | **+162** | 玩家界面 141 + 关卡按钮 21 |
| `Control` | 21 | 63 | +42 | 玩家界面皮肤格子 |
| `ColorRect` | 17 | 59 | +42 | 同上（每个格子一个底色） |
| `HBoxContainer` | 61 | 80 | +19 | 同上（每行一个 HBox） |
| `AxisDrawer` / `EntityDrawer` / `EffectDrawer` | 0 | 各 1 | +3 | PlayScreen 的绘图节点 |
| 其余 20 类 | — | — | **0** | 完全一致（`MarginContainer` 210、`Label` 25、`LineEdit` 4、`TextureRect` 31…） |
| **合计** | **590** | **858** | **+268** | 162+42+42+19+3 = 268 ✔ |

这条对账把 `docs/92` §3 的"283 个 `GraphwarButton` / 25 个 `Label` / 4 个 `LineEdit`"
**第一次拆成了"静态 121 + 动态 162"**——此后重建时不必猜哪些按钮是场景里就有的。

### 5.3 运行时独有的三类（全部可解释，无一残留）

1. **玩家界面的皮肤/脸/帽子网格（244 个）**：`PlayerScreen/…/SkinsContainer|FacesContainer|HatsScroll`
   下面运行时才生成（`@HBoxContainer@51` 这类自动名就是证据），**Rust 侧创建**（没有对应 `.gd`）。
2. **21 个关卡按钮**：`Campaign/…/Control/level_0..level_20`，由 `CampaingLevelsButtons.gd`
   `GraphwarButton.new()` 生成——静态对拍正好 21 个，与 `docs/101` §4.1 的常量 `21` 对得上。
3. **PlayScreen 的 3 个绘图节点**：`GraphwarPlane/` 下的 `@AxisDrawer@2`、`@EntityDrawer@3`、
   `@EffectDrawer@4`（Rust 类，运行时挂上去）。

其它快照上的动态量（同一工具，`--runtime <log> --snapshot <name>`）：

| 快照 | 场景 | 静态 | 运行时 | 动态 | 内容 |
|---|---|---|---|---|---|
| `replay_lobby` | ReplayLobby | 13 | 1252 | 1239 | **59 条回放 × 21 节点**（`replay_entry.tscn` 实例化） |
| `setup_six_bots` | GameSetup | 47 | 377 | 330 | 6 个玩家条目（`team1_entry`/`team2_entry`） |
| `play_screen_t6s` | PlayScreen | 106 | 126 | 20 | 19 个 `Sprite2D` + 1 个 `RichTextLabel`（弹道/文字） |
| `endgame_popup` | EndGamePopUp | 69 | 69 | 0 | 完全一致 |

### 5.4 静态有、运行时没观测到的

* **已实例化的 15 个界面：0 个节点**。也就是说 `docs/92` 的运行时枚举**没有漏掉任何静态控件**。
* **整场景未观测**：`function_buttons.tscn`（31 节点）、`function_buttons_no_control.tscn`（31）、
  `line_2d.tscn`（1）——33 个快照里从未出现，与 `docs/92` §8 的"未观测"清单一致，
  现在**从静态侧确认它们确实存在**（不是文档笔误）。
* **按需实例化的子场景**：`replay_entry` / `team1_entry` / `team2_entry` / `shop_button_entry` /
  `game_room_entry` / `name_label` 在 boot 里没有节点，属于"父容器按需 `instantiate`"，
  不是缺失。

### 5.5 对不上的具体项（写清，不含糊）

| 项 | 静态 | 运行时 | 结论 |
|---|---|---|---|
| `docs/92` §2.3 "2724 条信号连接 / 只剩 27 条人为连接" | `.tscn` **0 条** `[connection]` | dump 只保留非引擎、非按钮自连的 8 类 | **口径问题，不是错误**：`dump_ui.gd` 主动过滤了 `resized/size_changed/visibility_changed/item_rect_changed`，见 §6 |
| `docs/92` §3 "283 个 `GraphwarButton`" | `main.tscn` 静态 121 | boot 283 | 都对；差额 = 运行时创建（§5.2） |
| `docs/92` §8 "`function_buttons.tscn` 未观测" | 存在（31 节点） | 未出现 | 静态确认存在，仍**未观测其何时被实例化** |
| `docs/101` §4.1 "8 列 × 4 行" | — | — | 真源码 `screen_size.x / 8`、`/ 4`；`21` 个按钮按 8 列折行，OK |

**没有**发现"静态与运行时 class/script 不一致"的情况——105 个 `script` 归属、
1374 个节点类型，全部一致。

---

## 6. 连接对账（这是本轮最实质的更正）

### 6.1 `.tscn` 里没有连接

* 游戏 25 个场景：`[connection]` **0 条**；
* 6 个 AdMob 插件样例场景：**31 条**（`Banner.tscn` 14 条、`Main.tscn` 6 条…），与游戏 UI 无关。

⇒ 想从场景文件拿"信号连接清单"这条路**不成立**；连接只有两个来源：`.gd` 的 `connect()`
和 Rust 侧（`GraphwarFunctionInput` 等 Rust 类在自己的 `_ready` 里连）。

### 6.2 `.gd` 侧：65 处 `connect()`

| 类别 | 条数 | 例 |
|---|---|---|
| `get_tree().root.size_changed` | 32 | `MarginContainer.gd`、`TitleSize.gd`… |
| `billing.*`（android_iap） | 17 | `billing.purchase.connect(_on_purchase)` |
| `purchase_confirmed/pending/denied`（IAP_holder） | 6 | `$IAP_Android.purchase_confirmed.connect(...)` |
| `item_rect_changed` | 5 | `ExtraOptionPad`、`ExtraOptionsHBox`×2、`ViewFunctionPad`、`ShowExtraButtons` |
| `resized` | 2 | `CampaingLevelsButtons`、`OptionsContainer` |
| `visibility_changed` | 1 | `AdContainer` |
| `ad_loaded` | 1 | `AdContainer` |
| `connect("pressed", _button_pressed)` | 1 | `ShowExtraButtons` |

### 6.3 运行时 `@@C` 只看到 8 类

`ad_loaded→_update_size`、`pressed→_button_pressed`、`text_changed→on_text_change`、
`text_submitted→on_text_submitted`、`text_submitted→on_chat_submitted`、
`purchase_confirmed/pending/denied→on_purchase_*`（另有 3 类 `purchase_*→purchase_*` 自连）。

**差异原因（读 `tools/dump_ui.gd` 的 `_conns_of()` 就能看到）**：它显式跳过
`resized`、`size_changed`、`visibility_changed`、`item_rect_changed` 等 14 个"引擎噪声"信号；
而 `get_tree().root` 是 **Window**，根本不在 `GraphwarRootNode` 子树里，dump 走不到。

⇒ `docs/92` 的"6 条游戏 UI 连接"的精确清单应为：

| # | 来源 | 信号 → 方法 | 谁能证明 |
|---|---|---|---|
| 1 | `ShowExtraButtons.gd` | `pressed → _button_pressed` | `.gd` + 运行时 |
| 2 | Rust `GraphwarFunctionInput` | `text_changed → on_text_change` | 运行时 |
| 3 | Rust `GraphwarFunctionInput` | `text_submitted → on_text_submitted` | 运行时 |
| 4 | Rust `RoomNameLineEdit` | `text_changed → on_text_change` | 运行时（同一 pair，节点不同） |
| 5 | Rust `ChatLineEdit` | `text_submitted → on_chat_submitted` | 运行时 |
| 6 | Rust `PlayerNameLineEdit` | `text_changed → on_text_change` | 运行时（同一 pair） |

外加 **40 条布局连接**（32 `size_changed` + 5 `item_rect_changed` + 2 `resized` + 1 `visibility_changed`），
它们不是"游戏 UI 逻辑"，但**是界面刷新链的一部分**，重建时必须补上（否则缩放后字号不变）。

---

## 7. 反编译源码 vs 字节码：105/105 逐 token 一致

`tools/gdc_verify_source.py` 把反编译 `.gd` 重新词法化，映射成与 `.gdc` 相同的
`(种类, 值)` 序列，要求逐个相等：

```bash
../.venv/bin/python tools/gdc_verify_source.py \
    --gdc ../rsc --gd ../.agent-work/tools/recovered
# verified 105/105 scripts token-for-token
```

两点已知差异，都写进工具注释里，不藏：

1. **负数字面量**：编译器把 `-3` 折成一个常量，源码文本是 `-` + `3`；校验器先折叠再比。
   `android_iap.gd` 的 3 个枚举值（`-3 / -2 / -1`）就是这种情况。
2. **三引号字符串**：AdMob 的 `admob.gd` 用了 `"""…"""`，词法器要单独处理。

⇒ GDRE 的反编译在本项目上**没有增删 token**；`docs/101` §4 的伪源码可以放心当作等价文本。

### 7.1 `docs/101` 的 5 个行为脚本：**0 处更正**

| 脚本 | 我此前的结论（字节码推出） | 真源码 | 判定 |
|---|---|---|---|
| `CampaingLevelsButtons` | 21 个按钮、`level_0..20`、字号 = 宽/15、8 列 4 行、碰边回弹 | 完全一致（含 `add_child(button);` 的分号） | ✅ 无需改 |
| `ShowExtraButtons` | `!showing_buttons`、两张 svg、宽/1.66667 阈值、置灰 | 完全一致 | ✅ |
| `ChatBox` | 鼠标/触屏各一遍 `grab_focus` | 完全一致 | ✅ |
| `PlayerTabContainer` | 三个页签标题（含 U+2009）、字号 = 安全区宽/30 | 完全一致 | ✅ |
| `OptionsContainer` | `resized → call_deferred`、高 = 父高 × 0.25 | 完全一致 | ✅ |

`docs/101` §4 已加一句"与 GDRE 反编译源码逐 token 一致"的标注与两条短片段，正文伪源码保留
（它由 `tools/gdc_disasm.py` 从 `.gdc` 直接生成，不依赖恢复产物，可单独复现）。

---

## 8. 未知 / 限制

| 项 | 状态 | 缺什么 |
|---|---|---|
| `.scn` 二进制里是否还有 GDRE 没写出的信息 | **未验证** | 本轮只用 GDRE 的文本产物，没有自己解析 `.scn`；若将来发现字段丢失，得回到二进制 |
| `anchors_preset` 语义 | **推断** | 按 Godot 的 `Control.LayoutPreset` 枚举解释（15=FULL_RECT、2/3=下左/下右），没有逐节点跑起来验证 |
| 动态节点的创建者 | **部分** | 玩家界面 244 个节点确定为 Rust 创建（无对应 `.gd`），但**没有定位到具体 Rust 函数** |
| 主题资源（`.tres`）默认值 | **未展开** | 只统计了 `.tscn` 里的 9 条 override；`graphwar_theme.tres` 等 22 个资源的默认字号/边距没提取 |
| 贴图/字体 | **仅清点** | 149 svg / 7 ttf / 167 ctex 未分析（版权也敏感，不入库） |
| GDRE 版本绑定 | **已知** | `--bytecode=4.5.0` 必需；换引擎版本要重跑，且 token 编号表（`docs/101` §3）也要重验 |
| 回放播放界面 | **仍未观测** | `docs/92` §8 的老问题；本轮只确认 `replay_entry.tscn` 的结构（21 节点/条） |

---

## 9. 复现

```bash
cd /home/user/MyProject/gw2

# 1) 恢复（产物在 .agent-work，不入库）
export XDG_DATA_HOME=$PWD/.agent-work/tools/gdre-data \
       XDG_CACHE_HOME=$PWD/.agent-work/tools/gdre-cache \
       XDG_CONFIG_HOME=$PWD/.agent-work/tools/gdre-config
.agent-work/tools/gdre_tools.x86_64 --headless \
  "--recover=$PWD/gw2-rebuild/rsc" --bytecode=4.5.0 "--output=$PWD/.agent-work/tools/recovered"

# 2) 静态控件表 / 摘要
.venv/bin/python gw2-rebuild/tools/tscn_inventory.py --all .agent-work/tools/recovered
.venv/bin/python gw2-rebuild/tools/tscn_inventory.py --scene <recovered>/play_screen.tscn --md <out>.md

# 3) 与运行时 dump 对拍
.venv/bin/python gw2-rebuild/tools/tscn_inventory.py --all .agent-work/tools/recovered \
    --runtime .rea-work/ui/boot.log --snapshot boot_all_screens

# 4) 反编译源码 vs 字节码
.venv/bin/python gw2-rebuild/tools/gdc_verify_source.py \
    --gdc gw2-rebuild/rsc --gd .agent-work/tools/recovered
```

自检判据：

* 对拍输出里 **`static_only` 必须为 0**（否则说明静态解析漏节点）；
* `mism` 必须为 0（class/script 不一致说明解析错位，不是游戏问题）；
* `verified 105/105`——少一个都要先查词法器，而不是改文档。
