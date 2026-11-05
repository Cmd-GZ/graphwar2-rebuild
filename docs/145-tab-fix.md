# 145 · 两个"看起来是小事"的视觉错误：**用固定图代替了动态合成**

状态：**现行**。本文只回答两件事：

1. **Faces / Hats 两页的格子底色为什么是错的**（原版 = 当前皮肤色，我们 = 打开界面那一刻的颜色），
   以及这个"动态合成"的规则是从**哪一条真帧 + 哪一个 `.tscn`** 读出来的；
2. **关卡选择弹窗（JoinLevelPopUp）"下面的部分太长" + 模式图标底色不是白**的
   **真值 rect / 色值**、根因、修法。

> 前置：`129-player-screen.md`（玩家页三页签的格数/白卡/图标顺序）、
> `130-popups.md`（弹窗骨架、`Panel` vs `PanelContainer`、真值管线）、
> `137-pixel-fidelity.md`（整树宿主真值管线、`shop_button_entry` 三层的残差）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md` §0.3）：
**逐位** = 与真二进制逐字节/逐像素一致；**观测** = 能指到文件、字节或真机真帧；
**推断** = 写明缺什么证据；**未知** = 没有证据。

⚠️ 本轮的**判据全部是"真机真帧 / 正版 pck + stock Godot 引擎自报的 rect"**；
`selftest` 里新增的断言只是**回归保护**，不构成复原证据（`AGENTS.md` §0.2）。

---

## 0. 一页结论

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

## 1. A：Faces / Hats 的底色 = **当前皮肤**（动态合成）

### 1.1 真帧证据（**观测**，正版二进制）

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

### 1.2 合成规则从哪读出来（**观测**，pck 里的 `.tscn`）

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

### 1.3 我们错在哪（**观测**：我们自己的帧）

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

### 1.4 修法（`client/src/screens/player_screen.gd`）

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

## 2. B：JoinLevelPopUp 的真值 rect / 色值

### 2.1 真值怎么来的（两条**互不依赖**的管线，结果一致）

| 管线 | 做法 | 给了什么 |
|---|---|---|
| **C（pck + stock Godot）** | 正版 `Graphwar.pck` 交给 stock `Godot_v4.7.2`（`Graphwar.pck` + `libgraphwarrust.so` 按 `Graphwar.gdextension` 的相对路径摆好），实例化原版 `main.tscn` 整棵树，只把 `ScreensControl/JoinLevelPopUp`（+ 底图 `Campaign`）设成可见，`print` 每个 `Control` 的 `position/size` | **引擎自报的 rect**（最硬） |
| **A（正版二进制真帧）** | Xvfb 1920×1160 跑 `Graphwar.x86_64`，`xdotool` 点 `Campaign(536,628)` → 关卡 `0(238,228)`，`import -window root` 截图 | **真实渲染帧**（含运行时文案） |

两条管线的结果**一致**：C 报的面板 rect 与 A 帧里那块 `#1A3B2D` 连通域**逐像素相同**
（黑边环位置、四个格子的 3px 黑边、按钮行的边全部对齐）。⇒ 本节数字是**双重观测**。

> 顺带纠一个坑：早先有一份同名截图（`orig_join_level`）是从**解包出来的 `godot-orig` 工程**
> 跑的（clear color 是 Godot 默认灰 77、面板 909 高），**不是**正版 pck 的结果。
> 拿它当"真值"会得出面板 909 的错误结论。判定方法：那一帧的底图是 `(77,77,77)` 灰，
> 而正版 clear color 是 `(14,37,28)`。**换真值之前先看底色**。

### 2.2 真值 rect（1920×1160，C 自报 + A 帧逐像素确认）

`join_level_pop_up.tscn` 的竖直结构是 **1 : 10 : 1 三条带**（顶/底两条的最小高度 = 返回键高 96）：

```
VBoxContainer (sep 4)
├ MarginContainer2  顶部返回键带      (0,0)    1920× 96
├ HBoxContainer2    面板带（ratio 10）(0,100)  1920×956   ← 只有它吃竖直剩余空间
│   ├ MarginContainer   左留白               239 宽（1 份）
│   ├ MarginContainer2  面板 (243,0)   1434×956（6 份）
│   │    ├ Panel                          ← 与下面那个 VBox **兄弟**，不是 PanelContainer
│   │    └ VBoxContainer (sep 4)  1434×956
│   │         ├ LevelTitleLabel       1434× 90
│   │         ├ HBoxContainer 说明行   1434×522（= 剩下的空间）
│   │         ├ OptionsContainer      1434×239（= 0.25 × 956，`OptionsContainer.gd`）
│   │         └ HBoxContainer3 按钮行  1434× 93（= 按钮 73 + `IpPopUpMargin` 10×2）
│   └ MarginContainer3   右留白              239 宽
├ HBoxContainer3    底部留白带        (0,1060) 1920× 96
└ AdContainer                        (0,1160) 1920×  0（桌面无广告，但那 4px 分隔**在**）
```

逐像素量到的细节（两条管线都对得上）：

| 元素 | 真值 |
|---|---|
| 四个模式格子 | **236×236** 正方，x = 482 / 722 / 962 / 1201，y = 722..957（含 3px 黑边） |
| `AxisModeButton` 底色 | **(255,255,255) 白** + 3px 黑边 + 圆角 10 |
| 另三个模式按钮底色 | **(26,59,45) = `#1A3B2D`** + 3px 黑边 + 圆角 10 |
| `Cancel` | x 742..935（**194** 宽），y 954..1045（92 高） |
| `Start` | x 960..1177（**218** 宽） |
| 两键之间的空档 | **24 px** = `HBoxContainer3` 默认 sep 4 + 两个 `IpPopUpMargin` 各 10 |
| 说明文字墨迹 | x 323..1262（左留白 63 + margin 10 + TextEdit 内缩）、y 221..272 |
| 面板边框 | 3px 黑边 + 圆角 10，面板内是 `#1A3B2D` |

模式图标底色的**文件级**证据（`观测`，两份 `.tres` 逐字段可读）：

* `join_level_pop_up.tscn` 里 `AxisModeButton` 的 `theme = axis_button_theme.tres`；
  该 theme 的 `Button/styles/{normal,hover,pressed,disabled}` **四个态全指向**
  `axis_menu_icon_theme.tres` —— 那是一个 `bg_color = Color(1,1,1,1)`（**纯白**）+ 3px 黑边 + 圆角 10 的 `StyleBoxFlat`；
* 另外三个（`FunctionModeButton` / `TurnModeButton` / `TimeModeButton`）的 theme 是
  `game_menu_icon_theme.tres`，其四个态指向 `game_menu_icon_button_theme.tres` =
  `bg_color = (0.101961,0.231373,0.176471)` = **`#1A3B2D`** + 3px 黑边 + 圆角 10；
* 四个的 `focus` 都是 `StyleBoxEmpty`。

### 2.3 我们错在哪

| # | 错法 | 后果 |
|---|---|---|
| 1 | 竖直方向只有 `[返回键带][面板带(EXPAND)][AdContainer]`，**没有底部留白带** | 面板带多拿 96 px ⇒ 面板 1049 高、底边 1152（真值 956 / 1056）。用户那句"弹窗下面的部分太长"就是它 |
| 2 | 面板用 `PanelContainer` | stylebox 的 content margin 把里面的 VBox 再内缩 3 px ⇒ 标题/说明/格子整体下移 3 px（真帧标题墨迹 121..171，我们 124..174） |
| 3 | 四个模式按钮**没给样式盒** | 落在默认（深绿）上 ⇒ 九宫格图标没有白底 |
| 4 | 说明行只有中间一格（没有左右 1:20:1 的留白与 10 px margin） | 说明文字从 x=250 开始（真值 323） |
| 5 | `HBoxContainer3` 写了 `separation = 8` | 两键间距 28（真值 24） |

### 2.4 修法（`client/src/screens/join_level_pop_up.gd`，整个 `build()` 按 `.tscn` 重写）

1. 三条竖带补齐：顶部带（`SIZE_FILL`，高 = 返回键高，1920 下 96）、面板带
   （`EXPAND_FILL` + `stretch_ratio = 10`）、底部带（`custom_minimum_size.y = 留白带高`）、
   再挂 `AdContainer`（0 高，保留它前面那条 4 px 分隔）；
2. 面板改成 `Panel` + `VBoxContainer` **两个兄弟**（同一矩形），去掉 `PanelContainer` 的 3 px 内缩；
3. 说明行补齐 `MarginContainer2(1) : IpPopUpMargin(20, 四边 10) : MarginContainer3(1)`；
4. 四个模式按钮各套 `GwThemeFactory.mode_icon_stylebox(on_white)`：
   `axis` ⇒ 白底，其余三个 ⇒ `#1A3B2D`；四个都是 3px 黑边 + 圆角 10，`focus` 用 `StyleBoxEmpty`；
5. `HBoxContainer3` 的 separation 改回默认 4；
6. `OptionsContainer` 的最小高仍然 = `0.25 × 面板内高`（`OptionsContainer.gd`），
   并且**不让它吃剩余空间**（否则它会从 239 长到 331，见 §3.2）。

---

## 3. 仍然未知 / 已知偏差（诚实清单）

### 3.1 四档标注

| 结论 | 等级 | 依据 |
|---|---|---|
| Faces/Hats 的圆底色 = **当前**皮肤 | **观测（真机真帧）** | 正版二进制在 Xvfb 里点 red 前后，15 格圆心 (255,204,0) → (255,114,114) |
| 皮肤只有一层（白卡不带图标） | **观测** | `shop_button_entry.tscn` 根 `GraphwarButton` 没有 `icon=`；改完 Faces 默认帧 0.572% → 0.060% |
| JoinLevelPopUp 的面板 = (243,100) 1434×**956** | **观测（双重）** | pck+stock Godot 自报 rect ＋ 正版二进制帧逐像素同一块 |
| 顶/底带各 96、格子 236×236、Cancel 194 / Start 218 | **观测** | 同上（引擎自报 + 真帧黑边环） |
| `AxisModeButton` 白底、其余 `#1A3B2D` | **观测** | 两份 `.tres` 逐字段 + 真帧取样 (255,255,255) / (26,59,45) |
| 底部带高 = 返回键高 | **观测@1920×1160**；**推断**在其他宽高比 | 真值 96 = 0.05×1920 = 返回键高；原版那 96 其实是 1:10:1 分出来的份额（1148/12 = 95.67 ⇒ 96）。桌面 16:9 下两者相等，**没有别的宽高比真值** |
| `OptionsContainer` 为什么最终停在 239（而不是按 `size_flags_vertical=3 + ratio 0.5` 再涨到 ~331） | **未知**（结果**观测**） | 引擎自报 rect 就是 239；缺：Godot `BoxContainer` 在"自定义最小尺寸 + expand"下的分配细节，或原版 Rust 有没有再写一次 min |
| 圆角/字形边缘的灰阶 | **未知** | 见 §3.3 |

### 3.2 `OptionsContainer` 的 ratio 疑点

`.tscn` 里 `OptionsContainer` 写着 `size_flags_vertical = 3`（EXPAND_FILL）与
`size_flags_stretch_ratio = 0.5`，而它的脚本又把自己 `custom_minimum_size.y` 设成 `0.25 × 父高`。
按"剩余空间按 2 : 0.5 分给说明行与选项行"的算法，选项行应该长到 ≈331；
但**两条真值管线量到的都是 239 = 0.25 × 956**。

我们的实现取**结果**：选项行只吃 `0.25 × 面板内高`，剩余空间全给说明行（真值 522）。
这条是**按真值结果对齐**，机制没查清（见 §3.1 表最后两行）。谁要动这一行，请先重跑 §4 的两条管线。

### 3.3 剩下的抗锯齿残差（都不影响"位置/内容对"）

| 位置 | 大小 | 是什么 |
|---|---|---|
| Faces / Hats 整屏 | 1337 px（0.060%），其中 tol>8 1082 px | 116 px 在返回键的 4 个圆角；~966 px 均匀落在**每个白卡的 4 个圆角**上（15 格 × 4 角 × ~16 px）。**推断**：原版白底是 `ColorRect`、黑边来自皮肤层 stylebox，两层的圆角 alpha 合成方式与我们"白卡自带同样半径黑边"不同。缺证据：`clip_children = 2`（Rust `GraphwarButton`）的裁剪/合成细节 —— 只有原生机器码，本轮没反编译 |
| JoinLevelPopUp 整帧 | 108 px 在标题/返回键、1932 px 在说明文字 | 字形边缘灰阶（与 `124` §6 #1 / `137` §4.4 同一类：内嵌字体的 MSDF 参数没逐字段对过） |
| PlayerScreen / Skins | 0.007%（152 px） | 返回键四角（与 `137` §3.3 记的同一处，本轮没动它） |

### 3.4 没做的

* **没验别的皮肤**：皮肤色只逐像素验过 `yellow`(默认) 与 `red` 两张。
  `white_skin` / `calico` / `wargod` 这类"带图案"的皮肤，圆心主色与边缘 AA 没有真值帧。
* **没验 Hats 第 4–9 行**（`129` §9 #4 一样的缺口）：只截了首屏 3 行。
* **没验弹窗在别的宽高比下**的顶部/底部带（§3.1 的"推断"那条）。

---

## 4. 怎么复现本文的每个数字

```bash
# 0) 资产（仓库不含任何媒体文件）
.venv/bin/python tools/extract_assets.py --pck ../Graphwar.pck

# 1) A 的真值：正版二进制 + Xvfb 1920×1160 + xdotool 点坐标 + import 截根窗口
#    actions 序列（同一进程里起 Xvfb / 跑游戏 / 点击 / 截图 / 清理）：
#      click 536 749 ; shot faces_default ; click 470 150 ; shot faces_default2
#      click 150 150 ; click 168 673 ; click 470 150 ; shot faces_red
Xvfb :91 -screen 0 1920x1160x24 & DISPLAY=:91 ./start_graphwar.sh --resolution 1920x1160 --position 0,0 &
DISPLAY=:91 xdotool mousemove 536 749 click 1        # 主菜单 CharacterButton
DISPLAY=:91 xdotool mousemove 470 150 click 1        # Faces 页签
DISPLAY=:91 import -window root faces_default.png
DISPLAY=:91 xdotool mousemove 150 150 click 1 ; DISPLAY=:91 xdotool mousemove 168 673 click 1
DISPLAY=:91 xdotool mousemove 470 150 click 1 ; DISPLAY=:91 import -window root faces_red.png
#    量圆心像素：python3 -c "from PIL import Image;print(Image.open('faces_red.png').getpixel((168,366)))"
#      → (255,114,114)；faces_default 的同一点 → (255,204,0)

# 2) B 的真值（管线 C）：正版 pck 喂 stock Godot，只显示 JoinLevelPopUp（底图 Campaign）
#    宿主工程（约 30 行）做三件事：load_resource_pack(pck) / 实例化 main.tscn /
#    把 ScreensControl 下只有 JoinLevelPopUp 与 Campaign 设成 visible，
#    其余界面 set_process(false)（否则原生 _process 会 panic，见 docs/137 §1.3），
#    然后 print 每个 Control 的 position/size 并把 viewport 存成 PNG。
#    关键数字应打印出：
#      Panel        pos=(243,100)  size=(1434,956)
#      HBoxContainer3(底部带) pos=(0,1060) size=(1920,96)
#      OptionsContainer pos=(0,620) size=(1434,239)
#    再与**正版二进制**同尺寸真帧逐像素比：
python3 - <<'PY'
import numpy as np; from PIL import Image
a=np.asarray(Image.open('truth_join_level.png').convert('RGB')).astype(int)
b=np.asarray(Image.open('ours_join_level.png').convert('RGB')).astype(int)
d=np.abs(a-b).max(axis=2); print('strict %.3f%%  tol>8 %.3f%%'%(100*(d>0).mean(),100*(d>8).mean()))
PY
#    改后实测：strict 0.108%  tol>8 0.096%（其中格子/按钮/底部带三块都是 0.000%）

# 3) 我们自己的帧：1920×1160，17 屏一次性出
DISPLAY=:95 tools/Godot_v4.7.2-stable_linux.x86_64 --path client --rendering-driver opengl3 \
    --resolution 1920x1160 --position 0,0 -- --shots=/abs/out

# 4) 自检（回归保护，不是复原证据）
bash client/run.sh selftest        # → SELFTEST 540 passed, 0 failed, 2 skipped
```

---

## 5. 本轮碰过的文件

| 文件 | 改了什么 |
|---|---|
| `client/src/screens/player_screen.gd` | 皮肤层改成**运行时合成**：保留 42 个 `SkinBackgroundButton` 引用、`_apply_skin_to_entries()`、`_on_skin_selected()`/`on_enter()` 触发重画；白卡层不再带皮肤图标（只画一层）；`_default_skin_index` → `_current_skin_index` |
| `client/src/screens/join_level_pop_up.gd` | `build()` 按 `.tscn` 重写：1:10:1 三条竖带 + 底部 96 px 留白带、`Panel`+`VBox` 兄弟、说明行 1:20:1 + 10 px margin、四个模式按钮的样式盒（axis 白 / 其余 `#1A3B2D`）、按钮行 sep 4；`_band_height()`/`_viewport_band_height()` 新增 |
| `client/tools/selftest.gd` | **只新增**（`_test_tab_fix()`，+12 条）：Faces/Hats 三层结构与"皮肤层 == 当前皮肤"、点皮肤后立刻换色、重进界面后恢复；JoinLevelPopUp 的 `Panel` 类型 / 面板 (243,100) 1434×956 / 底部带 (0,1060) 1920×96 / 格子 236×236 / Cancel 194 · Start 218 / axis 白底 · 其余三块 `#1A3B2D`。**没有改/删任何既有断言** |
| `docs/145-tab-fix.md` | 本文 |

**没碰**（按任务约定）：`play_screen.gd`、`lan_lobby.gd`、`client/src/play/plane_view.gd`、
`client/src/play/fire_fx.gd`、`client/src/net/`、`src/`（Rust）、`docs/INDEX.md`、`docs/HANDOVER.md`、
`client/src/play/player_showcase.gd`（本轮不需要改：它的层序 `skin → face → hat` 本来就对，
`137` §5.1 已验）。

### 关于自检条数的一个提醒

本轮起点在本机实测是 **528 passed / 0 failed / 2 skipped**（`git 76ee9d5`；
`examples/serve_match` 没编，两条联机断言显式 skip）。历史提交信息里写过 581 / 604，
那在本机**复现不出来**——同一份代码在共享工作区里也是 528。所以本轮的账是
**528 → 540（+12）**，不是"从 581 往上加"。这与 `AGENTS.md` §0.2 一致：
条数只是回归保护，别拿它当进度。
