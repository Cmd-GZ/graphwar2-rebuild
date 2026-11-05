# 129 · PlayerScreen 的白卡 / 三页签格数 / LanLobby 与 ReplayLobby 的残差

状态：**现行**。本文回答三件事：

1. 上一轮遗留下来的 **PlayerScreen 全屏 70.2% 像素差**到底是怎么来的（根因，不是猜）；
2. 改完之后的**逐像素数字**（改动前 / 改动后），以及哪几屏仍然**无法验收**、为什么；
3. LanLobby（Local + Global 两页）与 ReplayLobby 的残差是怎么消掉的，以及顺手补的那道**护栏断言**。

> 前置：`124-ui-geometry.md`（上一轮的真值管线与剩余差异清单 §6）、`121-visual-fidelity.md`
> （资产管线 / 图标映射表）、`92-ui-inventory.md` §4.14 / §4.15（界面台账）、
> `104-recovered-project.md` §4.1 / §5.2（恢复出的原版工程与运行时节点数）。
> 真值来源：**把正版 `Graphwar.x86_64` + `Graphwar.pck` 在 Xvfb 里以 1920×1160 跑起来截根窗口**
> （`124` §2 的做法），再拿逐像素比对。本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与原版字节/逐像素一致；**观测** = 能指到文件、字节、帧或像素坐标；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

| # | 现象 | 根因（全部有真帧/`.tres` 证据） | 改后 |
|---|---|---|---|
| ① | 格子只有一圈 **~35 px 的深绿小点**，白卡完全没有 | 白卡不是 `Panel`，是 **`shop_theme.tres` 的 `Button/styles/normal`**：`bg=#FFFFFF` + 3 px 黑边 + 圆角 10。原版把这份主题挂在下拉框里那层 `VBoxContainer` 上，Skins 页 15 个 `GraphwarButton` 直接继承它 | 逐像素对齐（§3） |
| ② | 格子比原版小 3 倍 | 96 px 来自 `ButtonGrid.gd`（**别的控件**用的公式）。玩家页挂的是 **`ShopButtonsSize.gd`**：`(安全区宽/2 − 50)/3` ⇒ 1920 下 **303** | 同上 |
| ③ | 格内图标太小 | `skins/*.ctex` 是 512² 画布，实心圆只占 **240 px（46.875%）**；`expand_icon` 撑满内容区 297 ⇒ 圆 **139 px**（真帧实测 139） | 同上 |
| ④ | 右列被挤出屏幕（NAME 输入框跑到 x=1307） | 我们给 TabContainer 写了 `stretch_ratio = 2.0`；原版两边**都是** `size_flags_horizontal=3` ⇒ ≈50/50（真帧 TabContainer 959 / 右列 957） | 同上 |
| ⑤ | 右列下半屏多出 `Support the game / Unlocking skins… / Unlock` | 原版桌面端 **SkinBlock 与 PendingBlock 都不显示**（真帧那一块是纯底色 `#0E251C`）；`PendingBlock` 在 `.tscn` 里就写着 `visible = false` | 节点保留、`visible=false` |
| ⑥ | 三个页签看起来一模一样 | `tab_unselected` 我们写成了全透明 + 自选灰色；原版是 `tab_unselected.tres`（`#1A3B2D` + 上/左/右 3 px 黑边 + 半径 10），且**没有**覆盖 `font_unselected_color`（用 Godot 默认白@0.7 = 实测 `(178,178,178)`） | 逐像素（§3.2） |
| ⑦ | Hats 页 **27** 格 | 真帧是 **8 行 × 3 = 24** 格（一直滚到底那一帧的上边界正好落在第 6 行） | 15 / 15 / **24** |
| ⑧ | LanLobby ` Join IP ` 宽 280 vs 原版 378、表头左移 260 px | 表头比例写错（3:1:2，原版 1:**0.1**:1）+ 左对齐 + 自选灰色；底部按钮**没有** `LobbyButtonContainer.gd` 的"每个子节点 = 屏宽×0.2 = 384" | 4.31% → **0.03%** |
| ⑨ | LanLobby Global：Players 是 `--`、模式块是 4 个暗色小方块、底部泄漏内部文档 | Players 是 `0`；四块要按 `game_room_entry.tscn` 的 `AspectRatioContainer + expand_icon` 画，且**第一块白底、其余三块深绿底**（两份不同 `.tres`）；泄漏那句在 `app.gd` 的 `notify()` 里 | 12.50% → **2.70%** |
| ⑩ | ReplayLobby 空列表多两行提示 | 原版空列表**什么都不画**（真帧只有返回键 + `REPLAYS`） | 2.44% → **0.38%** |

---

## 1. PlayerScreen：根因逐条（改之前先查清）

### 1.1 白卡不是 `Panel`，是主题里的 `Button/styles/normal`

恢复出的工程里，`player_screen.tscn` 三个页签的公共骨架是：

```
TabContainer/SkinsContainer(ScrollContainer)/VBoxContainer  ← theme = shop_theme.tres
    ├ MarginContainer(0,10)
    ├ HBoxContainer/[MarginContainer(10,0), SkinsVbox, MarginContainer2(10,0)]
    └ MarginContainer2(0,10)
```

`shop_theme.tres` 的 `[resource]` 段里那一行是关键（**观测**，逐字段可读）：

```
Button/styles/normal = SubResource("StyleBoxFlat_bw5kj")
# StyleBoxFlat_bw5kj: bg_color = Color(1,1,1,1) / 四边 border_width = 3 / border_color 黑 / corner_radius 10
```

主题挂在容器上 ⇒ 容器下面的 `GraphwarButton` 全部继承成**白底黑框圆角卡**。
Faces/Hats 走的是另一条路：`shop_button_entry.tscn` 里

```
Control
├ GraphwarButton(theme=long_press_ghost_button_theme, clip_children=2)
│   ├ ColorRect   ← **没写 color** ⇒ 默认白 ⇒ 这就是那块白底
│   └ SkinBackgroundButton(theme=Theme_b8swt)  ← 透明底 + 3 px 黑边 + 圆角 10，icon=皮肤
└ Button(theme=Theme_p63gr)                    ← 透明底 + 3 px 黑边，icon=脸/帽子
```

⇒ 两条路画出来的都是"**白底 + 3 px 黑边 + 圆角 10**"，所以三个页签的格子长得一样。
我们两边都没画，格子底下就是深绿面板 —— 这就是"白卡缺失"。

**证据等级：观测**（`.tres` 字段逐个可读）+ **逐位**（按这个画出来之后真帧像素对上了，§3.1）。

### 1.2 格子边长：96 是别人的公式，玩家页是 303

| 脚本 | 公式 | 1920 下的值 | 用在哪 |
|---|---|---|---|
| `ButtonGrid.gd` | `screen.x * 0.15 / 3` | **96** | 战斗界面左侧函数键区（`GridContainer`） |
| `ShopButtonsSize.gd` | `(safe_area.x / 2 − 50) / 3` | **303.33 → 303** | 玩家页三个页签的格子（`VBoxContainer`，递归给非 Container 子节点设 `custom_minimum_size`） |

上一轮把 96 当成了玩家页的格子尺寸，于是整块网格只有原版的三分之一。
真帧量到的白区是 `20..316 / 327..623 / 634..930`（每个 297 px = 303 − 3 px 边框 ×2，间距 4）——
**逐位**对上之后 `custom_minimum_size` 的取值就锁死了：`floor((1920/2 − 50)/3) = 303`。

### 1.3 图标尺寸：一张 512² 画布，圆只占 46.875%

把 `client/assets/skins/*.ctex` 解成 PNG 量 alpha bbox：`x 136..375 / y 136..375` ⇒ **240×240**，
即 240/512 = **46.875%**，圆心正好在画布中心。

于是两处尺寸都能用同一条比例解释（**观测 + 逐位**）：

| 位置 | 内容区 | 预期圆直径 | 真帧实测 |
|---|---|---|---|
| 格子按钮（白卡，边框 3） | 303 − 6 = **297** | 297 × 0.46875 = **139.2** | **139**（`x 99..237`） |
| 右侧预览（`AspectRatioContainer`） | ≈ **933** | 933 × 0.46875 = **437.3** | `x 1237..1662`（426 可见，被帽子裁掉一部分；按三层 alpha 并集反解得 933） |

也就是说原版只是"**把整张 512² 贴满控件矩形**"，没有任何魔法系数。
我们以前乘了一个 `0.92`，预览因此小一圈（§3.3）。

### 1.4 右列宽度：两边都是 `size_flags_horizontal = 3`

`player_screen.tscn` 的 `HBoxContainer3` 只有两个子节点：`TabContainer` 与右列 `VBoxContainer`，
**两个都写 `size_flags_horizontal = 3`**（原版没有 `stretch_ratio`，默认 1:1）。
真帧：TabContainer 外框 `0..958`（宽 959）、右列 `963..1920`（宽 957）。
我们以前给 TabContainer 写了 `stretch_ratio = 2.0` ⇒ 它吃 2/3，右列只剩 1/3，NAME 输入框跑到 x=1307。

### 1.5 SkinBlock / PendingBlock：桌面端**不显示**

真帧右列下半屏（y > 900、x > 960）是**纯底色 `#0E251C`**，既没有 "Support the game" 的白字，
也没有 SkinBlock 那个 `ColorRect(0,0,0,0.9)` 的黑底 ⇒ 原版运行时这两块**不可见**。

用 `visible = false` 而不是 `modulate.a = 0`（**推断**，证据如下）：
* `PendingBlock` 在 `.tscn` 里静态就写着 `visible = false`，同一份工程对"不显示"只用这一种表达；
* 两块的父节点是 `AspectRatioContainer`，它给所有子节点同一个矩形、**不因不可见子节点影响兄弟节点位置**；
  实测把 SkinBlock 设成 `visible=false` 之后，预览区矩形与真帧一致（§3.3 的 933），
  说明"隐藏"没有参与布局 —— 与 `visible=false` 的语义一致，与"透明但仍参与"不一致。
  缺的证据：拿不到原版 Rust 里那句 `set_visible(false)` 的反编译（`PlayerShowcase` / 玩家页逻辑在
  `libgraphwarrust.so` 里，本轮没做）。所以这一条只能算**推断**。

---

## 2. 三个页签的格数与图标顺序

### 2.1 格数：15 / 15 / **24**（不是 15/15/27）

真帧逐行数（1920×1160，行距 307 = 303 + 4，能滚动的页签一直滚到底再截）：

| 页签 | 行数 | 格数 | 依据 |
|---|---|---|---|
| Skins | 5 | **15** | 真帧 5 行；第 6 行不存在（滚动条到底时最后一行的下沿 + 10 px 底距正好贴住可视区底部） |
| Faces | 5 | **15** | 同上 |
| Hats | **8** | **24** | 滚到底那一帧的最上一行是第 6 行（`horned_helmet / hair_band / unicorn`），据此 N−3=5 ⇒ N=8 |

上一轮的 27 是"28 个帽子资源 −1"的**推断**，不是观测。`button 计数` 也随之从
`15 + 15×3 + 27×3 = 141` 变成 **`15 + 15×3 + 24×3 = 132`**（与真帧的运行期节点数一致）。

### 2.2 图标顺序：怎么定的（这条要写清楚）

两套数据不能混：
* **格子里显示哪张图** = 我从真帧**逐个格子**匹配出来的（见下），这是**观测**；
* **点第 N 格写进存档的编号** = 我们自己的 `profile.skin/face/hat` 是**下标**；
  原版存档 `user://player.config` 存的是**名字**（`skin: purple` / `face: regular_eyes` / `hat: soldier_helmet`，
  逐行可读）⇒ "第 N 格 ↔ 原版第 N 号"这层对应关系**只对显示负责**，不影响保存语义。

匹配方法（两种，都只用真帧 + 原版资产）：

1. **皮肤**：取每格中心区的**主色**，和 `skins/*.ctex` 的主色做最近邻（阈值内的距离全是 0.0 或个位数）。
2. **脸 / 帽子**：把这 512² 资产按"黄皮肤圆 + 该层"合成一张 297² 的预测图，和真帧对应格子做逐像素平均差，
   取最小者。命中格的残差都在 3–12 灰阶，第二名普遍 >15，区分度足够。

结果（`SKIN_GRID` / `FACE_GRID` / `HAT_GRID` 三个常量，元素是 `GwIcons` 数组下标）：

| 页 | 顺序（真帧观测） | 被跳过的资源 |
|---|---|---|
| Skins | blue, pink, purple, red, white_skin, yellow, green, light_green, true_purple, brown, calico, wargod, arrow, face_paint_1, face_paint_2 | `gray`、`target`（商店里没有这两张） |
| Faces | regular_eyes, blue_eyes, green_eyes, purple_eyes, anime_eyes, aviator_glare, cute_eyes, nerd_glasses, mustache, baggy_eyes, asian, cat_eyes, eye_patch, baby_eyes, alien_eyes | `no_face`（Bot 的脸，全透明） |
| Hats | no_hat, soldier_helmet, mohawk, cowboy, straw_hat, blonde_boy, blonde_girl, brunette_girl, powdered_wig, brown_boy, ponytail, cat_ears, witch_hat, mohawk_red, hood, horned_helmet, hair_band, unicorn, roman_helmet, chef_hat, mad_hair, top_hat, tricone, asian_hat | `anime_hair`、`baseball_cap`、`crown`、`antenna_0` |

> **注意**：`GwIcons.SKINS/FACES/HATS` 这三个数组（`docs/121` §4.3 那条 `.rodata` 推断出来的顺序）
> **没有改**——那是"名字 ↔ 下标"的全局约定，改它会动到存档语义和其它界面。
> 网格顺序改为在 `player_screen.gd` 里用 `*_GRID` 常量表达（"第 N 格显示哪一号"），
> 点格子仍然写 `profile.skin = GwIcons 下标`，于是预览、战场、存档三处的语义不变。
> 两套顺序的对应关系见上表；如果以后要合并，应当把 `icons.gd` 的数组改成这个实测顺序。

### 2.3 页签本身

`PlayerTabContainer.gd` 的三个标题（含 U+2009 THIN SPACE）：

```
set_tab_title(0, " \u2009 Skins  \u2009")
set_tab_title(1, "  \u2009Faces  ")
set_tab_title(2, "  \u2009Hats\u2009  \u2009")
```

字号 = **安全区宽/30**（1920 ⇒ 64）；`TabContainer/side_margin = 10`；`clip_tabs = false`。

---

## 3. 改了什么

### 3.1 `client/src/screens/player_screen.gd`（重写 `build()`）

按 `player_screen.tscn` 逐节点照抄：

```
MarginContainer(margin 0)/VBoxContainer(sep 4)
  ├ HBoxContainer/{ExitButtonContainer, MarginContainer/Label("PLAYER OPTIONS")}
  ├ HBoxContainer3  ← 两个子节点都 EXPAND（≈50/50）
  │   ├ TabContainer（side_margin 10、三个页签标题、字号 宽/30）
  │   │   └ 三个页签各自的 ScrollContainer → VBox(sep 4) → {Margin(0,10), HBox[Margin(10,0), 网格, Margin(10,0)],
  │   │                                                                        Margin(0,10)}
  │   └ VBoxContainer（右列）
  │       ├ HBoxContainer2/{MarginContainer/Label(" NAME: "), MarginContainer2(margin 10, 字号 宽/40)/LineEdit}
  │       └ MarginContainer(margin 5)/AspectRatioContainer/{PlayerShowcase, SkinBlock(隐藏), PendingBlock(隐藏)}
  └ AdContainer（运行 0 高）
```

要点：
* 格子边长 `floor((安全区宽/2 − 50)/3)`；**每格一个白卡按钮**（Skins 页）或
  **白卡 + 皮肤层 + 脸/帽子层**（Faces/Hats 页，照 `shop_button_entry.tscn` 的三层结构）；
* 图标层用 `GwThemeFactory.shop_overlay_stylebox()`（透明底 + 3 px 黑边）而不是 `StyleBoxEmpty`：
  `expand_icon` 铺满的是**内容区**，空样式盒会多出 6 px，圆从 139 变成 142；
* `PlayerNameLineEdit.max_length = 12`（`.tscn` 静态值，我们以前写 24）；
* " NAME: " 用 `title_theme` 的手写体（Eraser），字号 64；
* `_rebuild_cell_size()` 在 `apply_font_scale()` 里重算（窗口尺寸变了要跟着变）。

### 3.2 `client/src/ui/theme_factory.gd`

* **新增** `shop_cell_stylebox()`：白底 + 3 px 黑边 + 圆角 10（`shop_theme.tres` 的 normal）；
* **新增** `shop_overlay_stylebox()`：透明底 + 3 px 黑边 + 圆角 10（`shop_button_entry` 的两层图标）；
* **新增** `mode_icon_stylebox(on_white)`：联机大厅模式块的白底 / 深绿底两份；
* **新增** `_tab_box(expand_bottom)`：`tab_selected` / `tab_unselected` 的样式盒
  （**只有上/左/右**黑边、圆角只给上面两个角、`expand_margin_right = 3`，选中态再加 `expand_margin_bottom = 3`）；
* **删掉** `TabContainer` 的 `font_selected_color` / `font_unselected_color` 覆盖 —— 原版没有这两条，
  用 Godot 默认的白 / 白@0.7（实测未选中文字 `(178,178,178)` = 0.7×255）。

### 3.3 `client/src/play/player_showcase.gd`

* 去掉 `0.92` 系数：**整张 512² 画布铺满控件矩形**（居中的正方形）；
* `custom_minimum_size` 从 `(120,160)` 改为 `(0,0)`：矩形由外层 `AspectRatioContainer` 决定，
  给了最小尺寸会把右列撑变形。

### 3.4 `client/src/screens/lan_lobby.gd`

* 表头：`NameContainer(ratio 1) / PlayersContainer(ratio **0.1**) / ModeContainer(ratio 1) / JoinButtonContainer(不 EXPAND)`，
  三个标签**居中**、字号 宽/40、颜色用默认白（原来 3:1:2 + 左对齐 + 自选灰）；
  第四格照 `.tscn` 放一个**全透明主题**的 `" Join "` 按钮（只为撑出 Join 列宽度）；
* 顶部 `ShowLocalContainer` / `ShowGlobalContainer` 的 margin 从 4 改成 **0**（`LanLobbyTitle.gd` 只改字号），
  字号改 宽/30（真帧 Local/Global 文字 516..664 / 1385..1566 才对得上；多出的 4 px margin 会把整行撑高 8 px、
  文字下移 4 px）；
* 底部：实现 `LobbyButtonContainer.gd` 的规则 —— **每个子节点**（含两个垫片）
  `custom_minimum_size.x = 屏宽 × 0.2`，字号 宽/30 ⇒ ` Join IP ` 由文本宽 286 变成 **384**（真帧 1000..1383）；
* 房间条目照 `game_room_entry.tscn`：`ScrollContainer/NameContainer/名字(居中)` +
  `PlayersContainer(ratio 0.1)/数字(居中)` + `ModeContainer/OptionsContainer/[Margin, 4×AspectRatio(图标), Margin]`
  + `JoinButtonContainer/Join`；
  **第一块模式图标白底、其余三块深绿底**（`axis_menu_icon_theme.tres` vs `game_menu_icon_button_theme.tres`）；
* `Players` 从 `--` 改成 **`0`**（真帧四行全是 0）；
* 行是运行时建的 ⇒ `_rebuild_list()` 末尾补一次 `apply_font_scale()`（否则四行停在主题默认 24 号字）；
* 删掉 `join` 的提示里的内部文档编号。

### 3.5 `client/src/screens/replay_lobby.gd`

* 删掉 `_empty_note` 整个节点：空列表**一个字都不画**（原版真帧只有返回键 + `REPLAYS` 标题）；
* 三处 `tooltip_text` 里的 `docs/108 §7` 字样一并去掉（tooltip 也是用户能看见的字）。

### 3.6 `client/src/app/app.gd`（父 agent 授权"只清内部说明"）

* 删掉 `SHOW_GLOBAL` 那条 `notify("公网房间列表来源未知（docs/92 §8）…")`（原版切页签时不弹任何东西）；
* 其余 5 条气泡去掉 `docs/xx §y` / "原版…" 这类字样但**保留语义**
  （例："静音只切图标：本项目没有音频资源（docs/108 §7）" → "静音只切图标：本构建没有音频"）；
* **新增护栏**：`notify()` 在发气泡前先扫 `docs/` `§` `TODO` `占位` `未观测` `原版`，
  命中就**不发**、记进 `App.internal_text_violations` 并 `push_error`。

### 3.7 `client/tools/selftest.gd`

新增 10 条断言（146 → **156**）：

1. 全部 15 个界面的 `Label/Button/LineEdit` 的 `text` 里没有内部标记；
2. `App.internal_text_violations` 为空（气泡护栏真的在生效）；
3. 玩家页 15 个皮肤格子边长 = `floor((安全区宽/2−50)/3)`；
4. 玩家页 15 个格子都是白卡（白底 + 3 px 黑边）；
5. `SkinBlock` 存在但不可见；
6. `PendingBlock` 存在但不可见；
7. LanLobby 表头三列都居中；
8. 底部 ` Join IP ` 宽度 = 屏宽×0.2；
9. 公网房间 4 行的 `Players` 列都是 `0`；
10. 回放列表为空时只剩标题一个非空 `Label`。

---

## 4. LanLobby 的残差（Local 页）

真帧量出来的三组数（**观测**，1920×1160）：

| 元素 | 原版 | 改前（我们） | 改后 |
|---|---|---|---|
| 表头 `Room Name` 字身 | x 268..533（列中心 400） | x 6..270（左对齐） | x 268..533 ✅ |
| 表头 `Players` | x 807..969（中心 888） | 826..986 | 807..969 ✅ |
| 表头 `Game Mode` | x 1243..1507（中心 1375） | 1104..1366 | 1243..1507 ✅ |
| ` Create Game ` 边框 | x 535..995（461 宽） | 581..1041（461 宽，位置偏） | 535..995 ✅ |
| ` Join IP ` 边框 | x 1000..1383（**384** 宽） | 1052..1337（286 宽） | 1000..1383 ✅ |

⇒ 384 = `屏幕宽 × 0.2`，不是文本宽度。这一条在 `LobbyButtonContainer.gd` 里写着（`desired_size = (screen_size.x * 0.2, 0)`）。

## 5. ReplayLobby

原版离线（本机 `user://replays/` 空）真帧：只有左上返回键 + 居中 `REPLAYS`，其余纯底色。
我们以前在列表下面写了两行提示，其中一行还带 `docs/78` / `docs/108` 字样。已删除。

---

## 6. 逐像素验收（前 / 后）

同一台机器、同一份资产、同样 1920×1160 根窗口截图（`import -window root`），
逐像素统计"任一通道不同"的比例（strict），以及容差 8 的比例（tol>8，用来滤掉抗锯齿）。

| 屏 | 改动前 strict | 改动后 strict | 改动后 tol>8 | 证据 |
|---|---|---|---|---|
| **PlayerScreen / Skins** | **70.06%** | **0.49%** | 0.22% | 逐位（白卡/格子/图标全部对上，见下） |
| PlayerScreen / Faces | 未截（见 §6.1） | **0.98%** | 0.62% | 观测 |
| PlayerScreen / Hats | 未截（见 §6.1） | **2.30%** | 1.36% | 观测 |
| **LanLobby / Local** | 4.31% | **0.03%** | 0.02% | 逐位（表头 + 底部按钮 + 文字位置全对齐） |
| LanLobby / Global | 12.50%（旧构建，含底部那句泄漏） | **2.70%** | 2.55% | 观测（房间数据是占位值） |
| **ReplayLobby** | 2.44% | **0.38%** | 0.36% | 观测 |

复现实测脚本见 §8。剩下的差异是**字形/图形边缘的抗锯齿**：
把差值图放大看，剩下的全是 1–2 px 的字形描边（`Local` / `Room Name` / 皮肤圆的边缘），
没有一处是"位置错了"或"少画了东西"。

### 6.1 Faces / Hats 的"改动前"为什么是空的

改动前我只截了默认页（Skins）。要补 Faces/Hats 的"前"图，得把工作区退回 `HEAD` 重新 import 再点两个页签；
本轮时间花在把 Global 页的四块模式图标查清楚上，没做这件事。**这一项算缺证据**，
不做推断：上一轮 `docs/124` §6 只记了整屏 70.2%，没有分页数字。

### 6.2 仍然**无法验收**的屏

| 屏 | 为什么 |
|---|---|
| GameSetup / 六个弹窗（JoinLevel / JoinIp / Connecting / Disconnected / AddBot / Chat / EndGame / ConfirmPlayExit） | 原版要点到"建房 / 进关卡 / 联机"才出得来，离线点什么都没反应；上一轮也只拿到 MainMenu / Campaign / LanLobby / Player / Replays 五处 |
| PlayScreen（战斗界面） | 需要进入对局；本轮没做（且该文件由别的 agent 负责） |
| LanLobby / Global 的**房间条目** | 四个房间名与在线人数是**服务端数据**，离线拿不到；我们放的是占位条目。所以这一页的 2.70% 里有一部分永远无法归零，能验收的只有"表头 / 列宽 / 模式图标形制 / Join 按钮"。 |

---

## 7. 改过的既有断言（**必须知道**）

规矩是"只许新增断言"。但有 4 条既有断言写的是**上一轮的错误观测**，不改就必然红。
逐条披露：

| 位置 | 旧断言 | 新断言 | 为什么必须改 |
|---|---|---|---|
| `selftest.gd` `_test_player_screen` | 帽子格子 27 个（`GwStore.HAT_COUNT`） | **24 个** | 真帧是 8 行 × 3（§2.1） |
| 同上 | 网格共 141 个按钮 | **132 个** | 15 + 15×3 + 24×3 |
| 同上 | PlayerScreen 共 144 个按钮 | **135 个** | 132 + 3 静态 |
| `selftest.gd` `_test_lan_lobby` | 公网房间列表 5 条 | **4 条** | 房间列表是服务端数据，条数每次都变；本轮真帧与用户两张截图都是 4 条 |

其余 146 条断言**一条没动**，新增 10 条（§3.7）。总数 146 → **156 passed**。

> `GwStore.HAT_COUNT` / `SKIN_COUNT` / `FACE_COUNT`（`store.gd`）**没有改**：
> 那是存档下标的上界，改动会影响老存档；网格格数由 `player_screen.gd` 的 `*_GRID` 表达。

---

## 8. 怎么复现本文的每个数字

```bash
# 0) 资产（仓库不含任何媒体文件）
.venv/bin/python tools/extract_assets.py --pck ../Graphwar.pck

# 1) 原版真帧：Xvfb 1920×1160，点 CharacterButton(536,749) / Battle(1342,628) / Replays(1342,747)
#    页签：Skins(150,150) Faces(470,150) Hats(780,150)；Global 在 (1500,48)
Xvfb :92 -screen 0 1920x1160x24 &
DISPLAY=:92 ./start_graphwar.sh --resolution 1920x1160 --position 0,0 &
DISPLAY=:92 import -window root orig_player.png

# 2) 我们自己的同尺寸截图（点同样的坐标）
Xvfb :90 -screen 0 1920x1160x24 &
DISPLAY=:90 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
    --rendering-driver opengl3 --resolution 1920x1160 --position 0,0
DISPLAY=:90 import -window root after_player.png

# 3) 逐像素
python3 - <<'PY'
import numpy as np
from PIL import Image
a=np.asarray(Image.open('orig_player.png').convert('RGB')).astype(int)
b=np.asarray(Image.open('after_player.png').convert('RGB')).astype(int)
d=np.abs(a-b).max(axis=2)
print('strict %.2f%%  tol>8 %.2f%%' % ((d>0).mean()*100, (d>8).mean()*100))
PY

# 4) 自检 / 内核通道
bash client/run.sh selftest        # → SELFTEST 156 passed, 0 failed
bash client/run.sh dump | grep -E '^@@(KERNEL|PROJECT)'
#   → @@PROJECT version=2.0.60 stretch_mode=disabled viewport=1600x900 mode=2
#   → @@KERNEL available=1 hit_test=1 ...
```

---

## 9. 剩下的已知偏差（诚实清单）

| # | 偏差 | 量级 | 证据等级 / 缺什么 |
|---|---|---|---|
| 1 | 皮肤圆 / 脸 / 帽子的**边缘抗锯齿**与原版略有差别（位置、尺寸、颜色都对） | Skins 0.49%、Faces 0.98%、Hats 2.30% | 观测；缺：`GraphwarButton`（Rust 类）里图标绘制路径的取整方式 —— 反编译那个类才能定论 |
| 2 | 字形描边（`Local` / `Room Name` / `CREATE GAME`）差 1–2 px | LanLobby 0.03%、ReplayLobby 0.38% | 观测；缺：同上的字体 MSDF 参数逐字段对比（`docs/124` §6 #1 同源问题） |
| 3 | `SkinBlock` / `PendingBlock` 用 `visible=false` | — | **推断**（§1.5）：拿不到原版 Rust 里 `set_visible(false)` 的那一句 |
| 4 | Hats 页滚到第 4 行以后的部分没有逐像素对照 | — | 只验了首屏 3 行 + 滚到底那一帧的行数；中间四行按同一套格数与顺序实现，但没有真帧逐像素 |
| 5 | 联机大厅房间条目的**数据**是占位 | Global 页 2.70% | 服务端数据，离线不可得（§6.2） |
| 6 | Faces/Hats 页的"改动前"数字缺 | — | 见 §6.1 |

---

## 10. 本轮碰过的文件

| 文件 | 改了什么 |
|---|---|
| `client/src/screens/player_screen.gd` | 按 `.tscn` 重写：白卡、303 格、三页签 15/15/24、`*_GRID` 网格顺序、右列 50/50、`max_length=12`、SkinBlock/PendingBlock 隐藏 |
| `client/src/play/player_showcase.gd` | 去掉 0.92 系数（整张 512² 铺满矩形）、去掉最小尺寸 |
| `client/src/screens/lan_lobby.gd` | 表头 1:0.1:1 + 居中、`LobbyButtonContainer` 的 0.2×屏宽、Players=0、四块模式图标结构 + 白底/绿底、运行时补字号 |
| `client/src/screens/replay_lobby.gd` | 删空列表提示、删 tooltip 里的文档编号 |
| `client/src/ui/theme_factory.gd` | 新增 `shop_cell_stylebox` / `shop_overlay_stylebox` / `mode_icon_stylebox` / `_tab_box`；删掉 TabContainer 的自定字体色 |
| `client/src/ui/ui.gd` | **没改**（`ad_slot()`、`frame()`、`exit_pair()` 的既有行为就是对的） |
| `client/src/app/app.gd` | 删 5 处气泡里的内部标记 + `notify()` 内部标记护栏（父 agent 授权，只动这一类） |
| `client/tools/selftest.gd` | 新增 10 条断言；修正 4 条上轮错误观测（§7） |
| `docs/129-player-screen.md` | 本文 |

**没碰**：`play_screen.gd`、`plane_view.gd`、`core_bridge.gd`、`client/src/audio/`、`src/`（Rust）、
`docs/INDEX.md`、`docs/HANDOVER.md`、`client/src/ui/icons.gd`（网格顺序改在 `player_screen.gd` 里表达，理由见 §2.2）。
