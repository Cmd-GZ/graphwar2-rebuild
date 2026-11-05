# 125 · PlayScreen / JoinLevelPopUp 对齐原版

状态：**现行**。本文回答一件事：用户拿真机截图逐张比对后给出的 PlayScreen（战斗界面）
与 JoinLevelPopUp（关卡弹窗）差异清单，**逐条怎么修、颜色和几何从哪来、还剩什么**。

> 前置：`108-client-rebuild.md`（客户端总台账）、`121-visual-fidelity.md`（上一轮视觉修复）、
> `118-campaign-data.md` §4/§7（21 关真数据）、`08-plane.md`（坐标系）、`102-color-assignment.md`（队色）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与原版字节/逐帧对上；**观测** = 能指到文件偏移、字节、帧号或像素坐标；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

| 用户列的差异 | 修法 | 状态 |
|---|---|---|
| 棋盘深灰黑底、网格几乎看不见 | 改成**原版真配色**：白底 `#FFFFFF`、细网格 `#E6E6E6`、粗网格 `#B2B2B2`、主轴 `#333333`、外框纯黑 3px | ✅ 逐像素（§2） |
| 顶部工具条完全没有 | 补 `ExtraOptionsHBox` 的 4 个开关（原版图标 + 原版判据显隐 + 原版居中/尺寸），默认可见 | ✅ 逐项（§3.2、§4） |
| `f(x) =` 是个小框；没有修正常数位 | 底栏按原版比例重排：`f(x)=` 图标 + **横跨的输入框**（75% 屏宽）+ 右下角修正常数（`+ 4.638` / `- 8.315` 形状） | ✅ 逐项（§3.3） |
| 士兵是小色点 + 白字 | 三层精灵（皮肤/脸/帽）+ **玩家颜色名字 + 黑描边**；玩家颜色按 `docs/102` 的 12 色表分配 | ✅（§3.4） |
| 左下 `00:57` 计时（原版没看到） | 战役 21 关 `time` 全 = 5 = **∞** ⇒ 倒计时**不显示字符**（但保留原版的 140 px 占位宽） | ✅ 观测（§3.5） |
| 底部说明文字 `战役第 0 关（占位…）` | **删除**（节点保留但 `visible=false`，只给 selftest 当诊断用） | ✅ |
| 键盘/函数面板深灰底、图标自绘 | 已是原版图标与 `#1A3B2D` 主题；本轮把按钮 **content_margin 从 12 改成 3**（原版没有该字段），键盘列宽因此与原版对上 | ✅（§3.1） |
| 弹窗占位文字 | 21 关**标题 + 描述原文 + 四个模式初值**全部换成真数据 | ✅ 逐位（§5） |
| 弹窗底部 4 个小图标 | 换成原版 `axis_option_N` / `fx_option` / `turns_sequential` / `turn_inf`，并做到 **228×228 @1920**（原版 228） | ✅ 观测（§5.2） |
| Cancel/Start 圆角大按钮 | 尺寸/字号按原版：194×73 / 218×73 @1920（我们 197×78 / 220×78） | ✅ 观测（§5.2） |

**一句话**：颜色、几何、关卡数据现在都有**可指认的来源**（原版真帧像素 / 原版 `.gd` 的比例公式 /
`.rodata` 的字节），不再有"目测取色"或"占位数据"。

---

## 1. 方法：先把"原版真帧"跑出来

这一轮最重要的方法变化：**不再靠读 `.tscn` 猜观感，而是把原版工程跑起来截图。**

```bash
# 1) 复制本机现成的原版 harness（godot-live = rsc/ + 原版 .so + 场景），换成**真美术资产**
cp -a godot-live .agent-work/playfit/godot-orig
cp -f .agent-work/tools/recovered/*.svg  .agent-work/playfit/godot-orig/
cp -rf .agent-work/tools/recovered/{skins,faces,hats,ranks}/* .agent-work/playfit/godot-orig/<同目录>/

# 2) 挂一个探针 autoload（不改 recovered/，只改这份副本）
#    probe.gd：按 input_text 找 GraphwarButton → emit_signal("pressed") 走完
#    MainMenu→LanLobby→GameSetup→PlayScreen，或 MainMenu→Campaign→level_0→JoinLevelPopUp，
#    然后 get_viewport().get_texture().get_image().save_png(...) 并打印每个 Control 的
#    GEOM|class|path|visible|x|y|w|h

# 3) 1920×1160 跑（llvmpipe 可跑；`--resolution` 有效）
PROBE_MODE=campaignplay xvfb-run -a -s "-screen 0 1920x1160x24" \
  ../tools/Godot_v4.7.2-stable_linux.x86_64 --path . --resolution 1920x1160
```

产物（都在 `.agent-work/playfit/`，**不入库**）：

| 文件 | 内容 |
|---|---|
| `shots/orig_campaign_play.png` | **原版战役第 0 关 PlayScreen 真帧**（1920×1160）——本文所有像素取样的来源 |
| `shots/orig_play_default.png` / `orig_play_extra.png` | 原版 **LAN 对局**（有障碍、有计时 `00:58`、有对手） |
| `run_*.log` 的 `GEOM|` 行 | 原版每个控件的 `x/y/w/h`（628 行）——几何对照表的"原版"列 |
| `shots_ours/PlayScreenCampaign.png` 等 | 我们自己的同一状态 |

> **一个重要坑（记下来）**：`graphwar_root_node.rs` 自己会解析命令行，`-- --probe x` 会被它
> 当成非法参数**直接退出**（`unexpected argument '--probe' found`）。所以探针的"模式"
> 只能走环境变量（`PROBE_MODE`），不能走 `OS.get_cmdline_user_args()`。

> **另一个坑**：这份副本的 `project.godot` 里**没有** `rendering/environment/defaults/default_clear_color`
> （真值在 `project.binary` 里，GDRE 没导出来），所以原版真帧的"棋盘外围"是 Godot 默认
> `#4D4D4D`，而真正游戏里是 `#0E251C`（`docs/121` §3.2 已逐位验证）。**棋盘本身的颜色不受影响**
> （那是 Rust 自绘的），但拿真帧比对时**不要把外圈灰当成原版配色**。

---

## 2. 棋盘配色：全部逐像素取自原版真帧

### 2.1 取到的值

| 元素 | 颜色 | 线宽 | 取样（`orig_campaign_play.png`） |
|---|---|---|---|
| 棋盘底色 | **`#FFFFFF`** | 填充 | 棋盘矩形内 215 897 个采样点是纯白 |
| 细网格（每 1 格） | **`#E6E6E6`** = (230,230,230) | 1 canvas px | 行 `y=210`：x=325/351/376/402/453/478/504… |
| 粗网格（每 5 格） | **`#B2B2B2`** = (178,178,178) | 2 canvas px | 行 `y=210`：x=427,428 / 555 / 683 / 811（间隔 128） |
| 主坐标轴（x=0/y=0） | **`#333333`** = (51,51,51) | 2 canvas px | 列 `x=939` 整列；行 `y=578,579` 两行 |
| 棋盘外框 | **`#000000`** | 3 canvas px | `x=300..302`、`y=195..197`、右 `1576..1578` |

代码在 `client/src/play/plane_view.gd` 顶部，**逐条写明来源**：
`COL_BOARD / COL_GRID_MINOR / COL_GRID_MAJOR / COL_AXIS / COL_BORDER`。

### 2.2 `docs/08` 的坐标换算仍然成立

| 项 | `docs/08` §4 记的 | 本轮实测 | 结论 |
|---|---|---|---|
| 画布 | 1000 × 600 px | 棋盘控件 1279×767（= 1.279 倍），网格 50×30 格 | ✔ |
| 比例 | 20 px / 单位 | 1279 / 50 = **25.58 px/单位**；细线间隔实测 25–26 | ✔（同比例放大） |
| 中心 | (500, 300) | y 轴 x=939 ≈ 300+639.5；x 轴 y=578.5 ≈ 196+383 | ✔ |
| 网格 | 50×30 | 细线每 1 单位、粗线每 **5** 单位（427→555 = 128 = 5×25.58） | ✔（新增：粗线周期 = 5） |

⇒ `px = 20x + 500`、`py = 20y + 300` **仍然成立**，只是画布被等比放大到控件尺寸；
线宽用"画布像素"表示 ⇒ `1 canvas px = _scale()/20`，在任何窗口尺寸下与原版同比例。

### 2.3 精灵的"内容占比"（一个容易画小一半的坑）

`skins/yellow.svg`、`skins/target.svg` 的可见内容只占 512×512 画布的 **46.5%**
（逐像素量取：bbox 53..146）。所以精灵方块边长必须是"身体直径 ÷ 0.465"：

| 量 | 值 | 来源 |
|---|---|---|
| 士兵身体直径 | 1.0 棋盘单位（= 26 px @1920） | `orig_campaign_play.png` 的 `crop_soldier.png` 量取 |
| 靶心直径 | 0.94 单位（= 24 px @1920） | 同上（`crop_target` bbox 1462..1486） |
| 精灵方块 | 2.15 单位 | 1.0 / 0.465（推断：把资产的内容占比当成缩放因子） |
| 名字字号 | 1.12 单位 = 28.8 px @1920 | 原版 `name_label.tscn` 是 RichTextLabel，字号取主题默认 24@1600；实测 x-height ≈13 px 与 28.8 px 字号吻合 |

---

## 3. PlayScreen：逐项修复

### 3.1 先说一个全局修正：按钮 content_margin

原版 `graphwar_theme.tres` 的 5 个 `StyleBoxFlat` **没有 `content_margin_*` 字段**，
Godot 于是用默认值（= border width 3）。我们 `theme_factory.gd` 里自选的是 12/6，
每个按钮因此宽 18、高 6 —— 实测把 PlayScreen 的右键盘从 **337 px 顶到 426 px**，
棋盘被挤掉 90 px。改成 3 之后键盘列宽逐列对上（见 §4 的 99/92/102 vs 101/94/104）。

### 3.2 顶部工具条（`ExtraOptionsVBox/ExtraOptionsHBox`）

**先说一个更正**：用户把它读成"网格开/关、坐标轴、名字显示、静音"，但原版真帧里这 4 个图标是

| # | 节点 | 图标（原版 `.tscn` 的 `icon=`） | 行为（原版 Rust 的 `input_text`） |
|---|---|---|---|
| 1 | `ViewFunctionButton` | `view_recent_functions.svg` | `toggle_view_functions`（三档：single/recent/all，三个图标都在包里） |
| 2 | `ShowNames` | `view_names.svg` | `toggle_names`（隐藏态 `hide_names.svg`） |
| 3 | `ChatButtonPlay` | `no_chat.svg` | `toggle_chat`（有消息 `no_chat_new_chat.svg`） |
| 4 | `MuteButton` | `unmute.svg` | `toggle_mute`（静音态 `mute.svg`） |

三处独立证据：`play_screen.tscn` 的节点/`icon=`/`input_text=`、`docs/92` §4.10 的真机观测
（"按下 ShowNames 前后 4 个名字 `visible` 全变 0"）、以及 `orig_play_default.png` 里这 4 个图标
**看起来就是**"清单+时钟 / 眼镜 / 划掉的对话框 / 喇叭"（`crop_toolbar.png` 放大 2 倍可见）。

我们照**原版**实现（4 个都是开关，语义按 `input_text`）。网格/坐标轴的显隐**不是**这 4 个按钮管，
而是 **AxisMode**（见 §3.6）——原版就是这样让网格"越来越稀、最后连轴都没了"。

几何（`ExtraOptionsHBox.gd` 的真源码，逐条抄）：

```gdscript
# 每个键 custom_minimum_size = screen.x * 0.15 / 3, 字号 = screen.x / 30
# 行高 = LeftTopButtonsContainer 的高度；HBox alignment = 1（居中）
# 显隐判据（ExtraOptionPad.gd / ExtraOptionsHBox.gd 同一条）：
extra_y = plane_container.size.y - plane_container.size.x / 1.66667
show    = extra_y >= toolbar_h or force_show          # force_show 由 ShowExtraButtons 切
```

实测原版 @1920：4 个按钮各 **146×96**，x = **641 / 791 / 941 / 1091**，行高 96；
我们：**144×99**，x = **640 / 790 / 940 / 1090**。

另外：原版 `ExtraOptionsVBox` **默认可见**（`.tscn` 里没有 `visible=false`），我们之前写死
`visible=false`（要点左上角那个按钮才出来）——已改。

### 3.3 底栏（`HBoxContainer2`）

原版结构（`play_screen.tscn` + 四个 `*.gd` 的比例）：

```
HBoxContainer2  (height = 98 @1920)
├ TimerContainer        子节点字号 x/30 ；有字符时宽 184（"00:58"），无限时限时**空白但仍占 140**
├ MarginContainer       size_flags=EXPAND + stretch_ratio 0.01   → 实测 14 px
├ FxContainer           **只有一颗图标**（`fx_option.svg` 里就画着 "f(x) ="）/ DiffEq 换 `dx_dy_option`
├ GraphwarFunctionInput EXPAND → 实测 1480 px（= 屏宽 77%），主题 LineEdit 样式（透明底+3px 黑边+圆角 10）
└ OffsetContainer       子节点 min = (x*0.09, 0)、字号 x/60 → 173 px
```

我们之前的问题：底栏 5 个孩子**全部** `EXPAND_FILL`（`GwUI.margin()` 的默认），于是
定时器/图标/输入框/偏移各分到 315 px；`f(x) =` 是"小 Label + 小图标"；输入框不横跨；
Offset 只有 100 px 最小宽。现在按上表重排。

修正常数的**格式**也改了：`- 8.315` / `+ 16.872`（符号与数字之间一个空格、三位小数），
取自原版真帧与 `docs/92` §4.10 的三处样本（`+ 2.515` / `+ 0.919` / `+ 16.872`）。
代码：`GwCore.format_offset()`。

DiffEq 模式下 OffsetLabel 为空 —— 与 `docs/92` §4.10 的观测一致，逻辑早就对，现在位置也对了。

### 3.4 士兵：精灵 + 彩色名字

| 项 | 原版（观测） | 我们 |
|---|---|---|
| 外观 | 3 个 Sprite2D：`skins/<skin>.svg` + `faces/<face>.svg` + `hats/<hat>.svg` | 同上（`GwAssets.texture`） |
| 身体直径 | 1.0 棋盘单位（26 px @1920） | 同（§2.3） |
| 名字 | 士兵头顶，**玩家颜色 + 黑描边** | 同（`draw_string_outline` + 玩家色） |
| 玩家颜色 | `docs/102` §1.1 的 12 色表（Team1 暖 / Team2 冷），随机起点 + 线性探测 | `GwPlaneView.assign_colors()` 复刻同一算法（种子确定性化） |
| 手里的小枪 | Rust `entity_drawer` 的 `draw_polygon`（绿色小块） | **未画**（见 §7） |

实测原版"user"的名字颜色 = **(122,160,0)**，正是 Team1 表第 4 项 `(122,160,0)` —— 与
`docs/102` 的调色板**逐值对上**（这条同时给颜色表加了一个独立的运行时旁证）。

### 3.5 计时：为什么原版"没看到"

战役 21 关的 `time` 字节**全是 5 = `GwModes.TimeLimit.INF`**（`docs/118` §4.3 的 `imm32` 第 4 字节
恒 `0x05`）。真帧里 `TimerLabel` 的 `visible=1`、宽 140 px，但**一个字符都没有**；
同一份原版在 LAN 对局里同一位置显示 `00:58`。

所以我们：`turn_seconds(mode) == 0` ⇒ `text = ""`（不给 `∞`），但保留 `0.0729 × 屏宽` 的占位宽度
——否则整条底栏会整体左移 140 px（这是实测出来的：不保留时输入框从 267 变 130）。

> **未知**：那 140 px 宽度到底来自哪个字符（`∞` 缺字形？`--:--` 占位？）没有取到。
> 我们只复刻"宽度对、内容空"这个可观测事实。

### 3.6 网格按 AxisMode 画（原版的"网格开/关"）

`docs/118` §7.1 把 21 关的描述与 `axis` 值逐一对照：**3 = 每单位、2 = 每 5 单位、1 = 只有主轴、
0 = 无轴**（21/21 同向，其中包括 "The finer grid lines are gone"→2、"Most grid lines are gone"→1、
"The main axes are gone"→0）。

`GwPlaneView.set_axis_mode()` 按这四档画；`PlayScreen.begin_match()` 用 `state.axis_mode`
（战役里来自 `GwCampaign.level(i).axis`）喂进去。**之前这个字段根本没被用过** ——
棋盘永远是同一种网格，这也是"网格看不清"体感的一部分。

### 3.7 删掉的占位文字

| 位置 | 之前 | 现在 |
|---|---|---|
| PlayScreen 底部 | `对局开始：…／目标：把对面打完`、`战役第 0 关（占位关卡数据，见 docs/108 §7）`、`回放模式：…` | **节点保留、`visible=false`、不写状态字**（只给 selftest/`--dump` 当诊断；原版没有这一行） |
| 11 个界面的广告条 | `advertisement slot — disabled in this build` | 只留原版的 60 px 空高度，**不写字** |
| JoinLevelPopUp 说明 | `PLACEHOLDER — the real campaign data … is unknown`（9 行） | 该关**原文**（`docs/118` §4.2，逐字节） |

---

## 4. 数值对照表（1920×1160，原版真帧 vs 我们）

测量脚本：`.agent-work/playfit/measure.py`（棋盘矩形/网格间距/键位/输入框）+
`--shots` 里新增的 `GEOM|` 行（控件几何）。所有数字单位都是**设备像素**。

| 项 | 原版 | 我们 | 差 | 判据 |
|---|---|---|---|---|
| 棋盘矩形 | (303,198)–(1575,959) = **1273×762** | (310,205)–(1564,954) = **1255×750** | −18 / −12（−1.4%） | 白色区域 bbox |
| 棋盘宽高比 | 1273/762 = 1.671 | 1255/750 = 1.673 | ✔ | 均 ≈ 50:30 |
| 细网格间距 | 25.5 px（26/25/26） | 25.0 px（25/25/25） | −0.5 px | 行内非白像素 x 序列 |
| 粗网格间距 | 127.8 px | 124 px | −3.8 px（−3%） | 同上 |
| 主轴 x（y 轴） | 939 | 937 | −2 | 列扫描 |
| 主轴宽 | 2 px | 3 px | +1 | 见 §7 |
| 左键盘 键填充宽 | 84（x 6–89 / 106–189 / 206–289） | 84（6–89 / 108–191 / 210–293） | ✔ | 主题色游程 |
| 右键盘 列宽 | 99 / 92 / 102 | 101 / 94 / 104 | +2 | 同上 |
| 键高 | 96 | 99 | +3 | GEOM |
| 左键盘 网格原点 | (0, 281) 296×596 | (0, 268) 300×621 | y −13 | GEOM |
| 顶栏 4 键 | 146×96 @ x=641/791/941/1091 | 144×99 @ x=640/790/940/1090 | ✔（±2） | GEOM |
| Flight `Fire!` | 337×96 @ (1583,0) | 345×99 @ (1575,0) | +8 | GEOM |
| 底栏 y / 高 | 1062 / 98 | 1063 / 96 | +1 / −2 | GEOM |
| 定时器宽 | 140（空白） | 141（空白） | +1 | GEOM |
| `f(x)=` 图标 | 96×98 | 96×96 | −2 | GEOM |
| 函数输入框 | x 267–1736 = **1470** | x 275–1732 = **1458** | −12（−0.8%） | 黑框游程 |
| 修正常数位 | 173 宽（x 1746–1919） | 174（x 1746–1920） | +1 | GEOM |
| 修正常数样例 | `- 8.315` | `- 190.100`（同一发不同函数，格式一致） | ✔ 格式 | 像素裁剪 |
| 士兵身体直径 | 26 px | 26 px | ✔ | 裁剪量取 |
| 靶心直径 | 24 px | 25 px | +1 | 裁剪量取 |
| 弹窗面板 | 1434×910 @ (243,100) | 1422×973 @ (249,78) | −12 / +63 | GEOM |
| 弹窗 4 个模式键 | 228×228 | 232×232 | +4 | GEOM |
| 弹窗描述字号 | x/40 | x/40 | ✔ | 原版脚本 |
| Cancel / Start | 194×73 / 218×73 | 197×78 / 220×78 | +3…+5 | GEOM |

**残差的系统性来源**（不是随机误差）：我们的基准视口是 1280×720，原版是 1600×900，
所有**绝对像素**量（边框 3、圆角 10、content margin 3）在我们的逻辑空间里被放大 1.5 倍，
而在原版里只放大 1.2 倍 ⇒ 同样的 3 px 边框我们画成 4.5 设备像素、原版 3.6。
这就是键盘列宽 +2、棋盘 −12 的来源。要彻底消掉得把基准视口改成 1600×900（**未做**，
影响面是整个客户端，见 §7）。

---

## 5. JoinLevelPopUp：真数据接进来了

### 5.1 数据从哪来（21 关全表）

`client/src/app/campaign.gd` 的 `LEVELS`（21 项）字段与来源：

| 字段 | 来源 | 证据等级 |
|---|---|---|
| `title` | `docs/118` §4.1：`Level 0`..`Level 20`，`.rodata` 两种编码，21/21 | 逐位 |
| `description` | `docs/118` §4.2：`.rodata` 逐字节 21 段（含原版拼写错误 `writting`） | 逐位 |
| `axis/function/turn/time` | `docs/118` §4.3：每关 `mov dword [rsp+0x58], imm32`，按 `+0x38..+0x3b` 拆 | 逐位 |
| `soldiers/targets/bot_tier/turn_limit/obstacles` | `docs/118` §7.1（`S` 立即数 / `C` 闭包预设 / `R` 385 份 replay） | 逐位 + 运行时 |
| `seed` | **我们的自定值**（原版运行时用 ChaCha 抽，哪关用哪种子拿不到） | 未知 |

接进界面的三条路径：

1. **JoinLevelPopUp**：`on_enter()` → `GwCampaign.level(index)` → 标题/说明/四个图标初值（§5.2）。
2. **开局**：`app.gd::_start_level()` 现在用 `lv.soldiers` 设我方士兵数、用 `lv.bot_tier` 决定
   要不要加 Bot 以及加哪一档（之前写死"human 1 兵 + bot 档 2"）。
3. **PlayScreen**：`state.axis_mode`（→ 网格密度）、`state.function_mode`（→ `f(x)=` / `dy/dx` 图标）、
   `state.turn_mode`、`state.time_mode`（→∞ 时计时空白）、`lv.targets`（→ 靶子绘制）。

> **仍未做的两条**（`docs/118` §7 有数据但界面还没消费）：
> ① **障碍**（散布式 `n = trunc(normal()*7+15)` 钳到 `min_count`、中心式 `r=u*5+3`）——
> 有公式但没有位置来源（原版用全局 ChaCha，我们没有同一条流），所以棋盘上不画障碍；
> ② `turn_limit`（回合上限）——界面侧没有回合计数，所以没用上。

### 5.2 弹窗几何（按实测对齐）

| 项 | 原版 @1920 | 依据 | 我们 |
|---|---|---|---|
| 面板 | 1434×910 @ (243,100) | GEOM | 1422×973 @ (249,78) |
| 面板宽占比 | 74.7% | GEOM | 75%（`1:6:1` 拉伸比） |
| 标题字号 | `x/30` | `LevelTitleLabel.gd` | 同 |
| 说明字号 | `x/40` | `LevelDescriptionScrollContainer.gd` | 同 |
| 四键行高 | 228 = 0.25×910 | `OptionsContainer.gd` | 237（0.25×内容高，带收敛重算） |
| 四个键 | 228×228 | GEOM | 232×232 |
| 首尾空槽 | 各 ~238 | GEOM | 各 ~1/6 宽 |
| Cancel/Start 字号 | `x/40` | `IpPopUpMargin.gd` | 同 |
| Cancel/Start | 194×73 / 218×73 | GEOM | 197×78 / 220×78 |

> 原版 `.tscn` 写的拉伸比是 `3:6:3`（= 50%），但实测面板是 74.7%。差在**面板最小宽**：
> 原版由 `TextEdit` 的最小宽撑到 ~790，再按 3:6:3 分配得到 74.7%。我们没有复现这个
> 最小宽链条，改用能直接命中同一比例的 `1:6:1`（**拟合，已注明**）。

另外：原版这 4 个按钮**不是 disabled**（`.tscn` 里没有 `disabled=true`），我们之前设了
`disabled=true`（图标被主题画成灰）。已改。

---

## 6. 改动清单（逐文件）

| 文件 | 改了什么 |
|---|---|
| `client/src/play/plane_view.gd` | 棋盘配色（§2.1）、AxisMode 四档网格（§3.6）、外框 3 canvas px、精灵内容占比（§2.3）、玩家颜色名字+描边、靶子、`set_axis_mode/set_targets`、`assign_colors()` |
| `client/src/play/function_input.gd` | 透明底 + 3px 黑边 + 圆角 10（主题 LineEdit 样式）、字号 `x/30`、**文本居中**（原版 `alignment=1`）、白色光标 |
| `client/src/screens/play_screen.gd` | 整屏按原版骨架重搭（Margin 0 / 间距 4 / 三列 / 底栏五段）；顶栏 4 开关（含显隐判据、`force_show`）；底栏重排；删状态行；Grid 竖直居中；键位 `min=(x*0.15/3)²`；计时空白但留宽 |
| `client/src/screens/join_level_pop_up.gd` | 真数据（标题/说明/四图标）、原版骨架（75% 面板、四键 0.25 父高、Cancel/Start 居中且 `x/40`）、去 `disabled` |
| `client/src/app/campaign.gd` | **21 关全表**（标题/描述/四模式/士兵/靶子/Bot 档/回合上限/障碍模式 + 来源注释）、`PLACEHOLDER=false` |
| `client/src/app/modes.gd` | 新增 `AXIS_LABELS` / `axis_label()`（四档含义有 §7.1 的 21/21 描述对照） |
| `client/src/app/app.gd` | 战役开局用 `lv.soldiers` / `lv.bot_tier` |
| `client/src/ui/theme_factory.gd` | `content_margin` 12/6 → **3**（原版无该字段 = border width） |
| `client/src/ui/ui.gd` | 广告条**不再写占位文字** |
| `client/src/ui/icons.gd` | 补 `view_single_function` / `view_recent_functions` 两个别名 |
| `client/src/core/core_bridge.gd` | `format_offset()` → `- 8.315` / `+ 16.872` |
| `client/tools/screenshots.gd` | 新增 `PlayScreenCampaign` / `PlayScreenOffset` 两个验收基准状态 + `GEOM|` 几何打印 |
| `client/tools/selftest.gd` | 把"说明栏含 PLACEHOLDER"改成"说明栏 == 第 0 关原文"（**收紧**，条数不变） |

---

## 7. 仍然存在的差异（照实写）

| # | 差异 | 为什么没做 | 证据等级 |
|---|---|---|---|
| 1 | 棋盘 **1255×750** vs 原版 1273×762（−1.4%） | 基准视口 1280 vs 1600 导致绝对 px 量（边框/圆角/content margin）等比放大 1.5 vs 1.2 | 观测 |
| 2 | 网格线宽 1.5/3 设备像素 vs 原版 1/2 | 同上 | 观测 |
| 3 | 士兵手里的小枪（原版 `draw_polygon` 的绿色小块）没画 | 形状未逐点取证 | 未知 |
| 4 | **障碍物不画** | `docs/118` §7.2 有公式，但原版用全局 ChaCha 抽位置，我们没有同一条流 | 未知 |
| 5 | 靶子位置 | 原版随机放置；我们用关卡种子的确定性伪随机（右侧半区） | 自定 |
| 6 | 我方士兵出生位置 | 原版随机；我们固定 x=±18、y=(slot−1.5)×3.4 | 自定（**docs/105** 的队伍侧向是对的） |
| 7 | 顶栏第 1 个按钮的三档 | 原版三张图标都有，但"三档各显示什么"没有逐档取证；我们按 single/recent/all 过滤函数列表 | 推断 |
| 8 | 弹窗面板高 973 vs 910 | 顶部返回行/间距略有差异（`MarginContainer2` 40 行结构未逐节点复刻） | 观测 |
| 9 | 计时空白时的 140 px 宽度来源 | 只知道宽度、不知道是哪个字符 | 未知 |
| 10 | 回合上限 `turn_limit` 未接 | 界面没有回合计数 | 未知 |
| 11 | 弹窗 4 个模式键点下去不动 | 原版在 JoinLevelPopUp 里也是纯显示（真正能循环的是 GameSetup）；我们连"点了会怎样"都没做 | 推断 |
| 12 | 原版底栏在 **LAN 局**里显示 `00:58` 且带一条弹道；战役局没有计时 | 两种状态我们都截了（`PlayScreen.png` / `PlayScreenCampaign.png`） | 观测 |

> 另外登记一条**给父 agent 的判断依据**：用户把顶栏第 1/2 个图标读成"网格/坐标轴"，
> 但原版真帧 + `.tscn` + `.so` 三处都指向 `view_*_functions` / `view_names`。
> 本轮**按原版实现**；若用户坚持要"网格/坐标轴开关"，那就是**新增功能**（原版没有），
> 应当另开一条"有意偏离"（`docs/108` §7）而不是算作保真修复。

---

## 8. 验证

```bash
# 资产（首次）
python3 tools/extract_assets.py --pck ../Graphwar.pck

# 自检（有资产时 99 条）
bash client/run.sh selftest          # → SELFTEST 99 passed, 0 failed
bash client/run.sh dump | grep "kernel on"   # → client 0.1.0 · kernel on

# 逐界面截图（1920×1160；12 + 2 个验收基准状态）
xvfb-run -a "$GODOT" --path client --rendering-driver opengl3 --resolution 1920x1160 \
    -- --shots=/abs/out
# 产物里 PlayScreenCampaign.png = 与原版 orig_campaign_play.png 同一状态
```

* 自检条数 **99 = 99**（一条没删）；改的那条是"说明栏含 PLACEHOLDER"→"说明栏 == 第 0 关原文"（收紧）。
* 内核：`kernel on` 仍在（`.gdextension`/`libgw2_rebuild.so` 未动）。
* 资产自检：`git diff --cached --name-only | grep -E '\.(svg|ttf|png|tres|scn|gdc|pck)$'` 必须为空
  （`.gd`/`.tscn` 是本项目源码，本来就在库里，不属于资产）。
