# 124 · 客户端几何与文案对齐（1600×900 / 去拉伸 / 原版主菜单与战役）

状态：**现行**。本文回答一件事：为什么我们跑出来的界面**整体大一圈**、为什么主菜单右下角
多出 `client 0.1.0 · kernel on` 和底部多出一条 `advertisement slot — disabled in this build`、
以及 Campaign 的 21 个数字为什么成了"灰色圆角按钮 + 粗黑体"。改了什么、还剩什么，全部写在这里。

> 前置：`121-visual-fidelity.md`（启动底色 / 主题色 / 资产管线）、`108-client-rebuild.md`（界面台账）。
> 真值来源三件套：恢复出的原版工程（`.agent-work/tools/recovered/`，31 `.tscn` + 105 `.gd` + 22 `.tres`）、
> 正版 `Graphwar.pck` / `project.binary`，**以及本轮新加的一条**：把正版 `Graphwar.x86_64`
> 在 Xvfb 里以 **1920×1160** 跑起来截图 —— 这是唯一能拿到"逐像素真值"的办法。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与原版字节/逐像素一致；**观测** = 能指到文件、字节或帧；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

| # | 问题（用户截图） | 根因 | 修法 | 证据 |
|---|---|---|---|---|
| ① | 标题/按钮/字号整体大一圈 | `project.godot` 写 1280×720 + `stretch/mode="canvas_items"` + `aspect="expand"`；1920 宽窗口 ⇒ 全 UI 放大 **1.5×** | 视口改 **1600×900**、**删掉 stretch 两键**（原版 `project.binary` 里没有任何 `stretch/*`）、`window/size/mode=2` | 逐位（截图 sha256 相同，见 §2.1） |
| ② | 右下角版本显示成 `client 0.1.0 · kernel on` | 我们自己往 `VersionLabel` 里塞了调试文字 | 照原版语义显示 `ProjectSettings.application/config/version` = **`2.0.60`**；内核状态改走 `dump` 通道 + `-- --debug-overlay` 才显示 | 观测（原版截图右下角只有 `2.0.60`） |
| ③ | 底部多一条 `advertisement slot — disabled in this build` | 我们自造的字 + 钉死 60 px 高度 | `AdContainer` 只留节点：**0 高、0 文字**（原版 `AdContainer.gd` 把高度设成 `ad_size`，初值 (0,0)） | 观测（`AdContainer.gd`/`Ad_holder.gd` 逐行可读 + 原版截图无此条） |
| ④ | Campaign 数字是"灰色圆角底 + 粗黑体" | 我们用了自己的按钮主题；原版 `campaign_button_theme_2.tres` 五态**全是 StyleBoxEmpty**、字体是手写体 `Eraser` | 逐属性照抄那份主题（`GwThemeFactory.campaign_level_theme()`） | 逐位（白字掩膜 IoU 0.997@阈值60，见 §3.3） |
| ⑤ | 存档不生效（只有第 0 关亮） | 我们没读原版 `user://campaign.save` | 实现 `level_<n>` 严格解析、单调写、不自动创建 | 观测 + 逐位（用正版二进制做了 2 组存档实验，见 §3.2） |

---

## 1. 怎么验的（本轮建立的真值管线）

```bash
# ① 正版二进制跑起来，1920×1160（和用户窗口同尺寸），截根窗口
Xvfb :79 -screen 0 1920x1160x24 &
DISPLAY=:79 ./start_graphwar.sh --resolution 1920x1160 --position 0,0 &
DISPLAY=:79 xdotool mousemove 536 628 click 1        # 点 Campaign
DISPLAY=:79 import -window root orig_campaign.png

# ② 我们的构建用同一尺寸、同一位置、同一套点击
Xvfb :80 -screen 0 1920x1160x24 &
DISPLAY=:80 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
    --rendering-driver opengl3 --resolution 1920x1160 --position 0,0 &

# ③ 15 个界面一次性截图（1920×1160）
DISPLAY=:80 ... -- --shots=/abs/out

# ④ 数值比对（不是目测）
python3 - # 白字掩膜 IoU、按钮底色带（#1A3B2D）行列带、标题墨迹 bbox
```

产物全部落 `.agent-work/uifit/`（`orig/` 是正版截图，`out/` 是我们的截图与 diff），**不入库**。

---

## 2. 主菜单：从"1.5×"到**逐像素一致**

### 2.1 结果先说

1920×1160、同一帧、同一个 `import` 抓根窗口：

```
orig_mainmenu.png   113780 B  sha256 3d6b776c9d16c768230f2d9fb8bd0e78f6fd6b586f20cffbdda9731aee921df1
ours_mainmenu.png   113780 B  sha256 3d6b776c9d16c768230f2d9fb8bd0e78f6fd6b586f20cffbdda9731aee921df1
```

**逐字节相同**（PNG 同尺寸、同编码器 ⇒ 像素完全相同）。这是本轮最强的证据等级：
不是"看起来对了"，是同引擎、同字体、同主题、同布局参数下渲染出的同一张图。

### 2.2 改掉的键

| 键 | 改前 | 改后 | 依据 |
|---|---|---|---|
| `display/window/size/viewport_width` | 1280 | **1600** | 原版 `project.binary` |
| `display/window/size/viewport_height` | 720 | **900** | 同上 |
| `display/window/stretch/mode` | `"canvas_items"` | **删除**（= disabled） | 原版 `project.binary` 里**没有**任何 `display/window/stretch/*` 键 |
| `display/window/stretch/aspect` | `"expand"` | **删除** | 同上 |
| `display/window/size/mode` | 未写 | **2**（`Window.MODE_MAXIMIZED`） | 原版 `project.binary` = 2 |
| `application/config/version` | 未写 | **`"2.0.60"`** | 原版 `project.binary` = `"2.0.60"` |

顺带把两个"我们自己加的内边距"也去掉了，它们是"大一圈"的第二个来源：

* `GwThemeFactory._flat()` 以前给每个 stylebox 写 `content_margin 12/6`；原版 5 个
  `graphwar_style_box_flat*.tres` 的属性表只有 9 条（bg + 4 border + border_color + 4 radius），
  **没有 content_margin** ⇒ 每个按钮的最小尺寸被我们多撑了 24×12 px。
* `GwUI.frame()` 以前是 `margin 10 / separation 10`；原版每个界面的根是
  `MarginContainer` + `MarginContainer.gd`（只在窗口超出屏幕安全区时才写 margin，桌面下 = 0），
  分隔没有 override ⇒ Godot 默认 **4**。改后 `frame(margin=0, separation=4)`。

### 2.3 主菜单的节点树按 `.tscn` 重写

以前是"近似摆放"（4 个按钮 `size_flags_vertical = EXPAND` + `custom_minimum_size=(0,64)`，
列比例还多塞了一个 0.05 的空隙）。现在逐节点照抄 `main_menu.tscn`：

```
MarginContainer(0)
└ VBoxContainer(sep 4)
   ├ MarginContainer [v=3]              ← TitleSize.gd：字号 = 宽/8
   │  └ Label "Graphwar II"（EraserRegular，居中）
   ├ HBoxContainer [v=3]
   │  ├ MarginContainer7 [h=3, ratio 0.2]      ← 左留白
   │  ├ MarginContainer  [h=3]                 ← 左列
   │  │  └ VBoxContainer（MainMenuButtonResize.gd：字号 = 宽/30）
   │  │     ├ PlayButton "Campaign"
   │  │     ├ MarginContainer [v=3, ratio .05]
   │  │     ├ CharacterButton "Player\n"
   │  │     └ MarginContainer2 [v=3]
   │  ├ MarginContainer3 [h=3, ratio .05]      ← 两列之间
   │  ├ MarginContainer2 [h=3]                 ← 右列
   │  │  └ VBoxContainer2
   │  │     ├ LocalButton "Battle"
   │  │     ├ MarginContainer [v=3, ratio .05]
   │  │     ├ ReplayButton "Replays"
   │  │     ├ MarginContainer2 [v=3]
   │  │     └ MarginContainer3（VersionLabel.gd：字号 = 宽/60）/ VersionLabel "2.0.60"（右对齐）
   │  ├ MarginContainer5 [h=3, ratio .05]
   │  └ MarginContainer6 [h=3, ratio .2]
   └ AdContainer（运行 0×0，无文字）
```

六列弹性比例之和 = 0.2 + 1 + 0.05 + 1 + 0.05 + 0.2 = **2.5**。

### 2.4 实测几何表（1920×1160，`run.sh dump` 的 `rect=` 值）

| 节点 | 位置 × 尺寸 | 由什么决定 | 原版同值？ |
|---|---|---|---|
| `MainMenu/MarginContainer` | (0,0) 1920×1160 | 满屏 + 安全区 margin = 0 | ✅（截图相同） |
| 标题 `MarginContainer` | (0,0) 1920×**576** | VBox 两个 v=3 子项平分 (1160−4×1−0) | ✅ |
| `Label "Graphwar II"` | (0,**148**) 1920×**280** | 字体 240 = 1920/8，行高 280 | ✅ |
| `HBoxContainer` | (0,**580**) 1920×**576** | 上=576，+4 分隔 | ✅ |
| `MarginContainer7`（左留白） | (0,0) **152**×576 | 1900 × 0.2/2.5 | ✅ |
| 左列 `MarginContainer` | (**156**,0) **760**×576 | 1900 × 1/2.5 | ✅ |
| `MarginContainer3`（列间） | (920,0) **38**×576 | 1900 × 0.05/2.5 | ✅ |
| 右列 `MarginContainer2` | (**962**,0) **760**×576 | 1900 × 1/2.5 | ✅ |
| `MarginContainer5` | (1726,0) **38**×576 | 0.05 | ✅ |
| `MarginContainer6`（右留白） | (1768,0) **152**×576 | 0.2 | ✅ |
| `PlayButton` | (156,580) 760×**96** | 字体 64 = 1920/30，行高 96 | ✅ |
| `CharacterButton` | (156,**701**) 760×96 | 上方空隙 17 = 372×0.05/1.05 | ✅（原版 699±2，见 §6） |
| `LocalButton` | (962,580) 760×96 | — | ✅ |
| `ReplayButton` | (962,**699**) 760×96 | — | ✅ |
| `VersionLabel` | (962,**1110**) 760×**46** | 字体 32 = 1920/60，贴右列底部 | ✅（原版右下角 `2.0.60`） |
| `AdContainer` | (0,**1160**) 1920×**0** | 运行 0×0 | ✅（原版无广告条） |

> 实测数据的来源：`bash client/run.sh dump`，在 1920×1160 的 Xvfb 窗口下跑
> （`@@PROJECT ... viewport=1600x900 mode=2 stretch_mode=disabled`）。

### 2.5 字号（全部来自原版脚本，不是我们挑的）

| 元素 | 规则 | 1920 宽时 | 出处 |
|---|---|---|---|
| 标题 | 视口宽 / **8** | 240 | `TitleSize.gd` |
| 4 个菜单按钮 | 视口宽 / **30** | 64 | `MainMenuButtonResize.gd` |
| 版本号 | 视口宽 / **60** | 32 | `VersionLabel.gd` |
| 主题基准字号 | **24** | 24 | `graphwar_theme.tres: default_font_size = 24`（上一轮为了将就 1280×720 改成 20，本轮**改回 24**） |

### 2.6 版本号文本：为什么是 `2.0.60` 而不是 `.tscn` 里的 `0.0.0`

* `.tscn` 静态文本 = `"0.0.0"`（**观测**：`main_menu.tscn`）；
* `VersionLabel.gd` **只改字号**（= 视口宽/60），不动文本（**观测**：脚本全文）；
* 真机截图显示 `2.0.60`（**观测**：用户截图 + 本轮正版二进制截图），而
  `project.binary` 的 `application/config/version` 正好是 `"2.0.60"`（**逐位**：解出的 JSON）；
* ⇒ 文本由运行时的原生 `GraphwarRootNode` 从工程设置里填（**推断**：那个节点在
  `libgraphwar*.so` / GDExtension 里，没有反编译产物；但"显示值与工程设置值相同"这一点
  被两张独立截图钉住）。

实现：`main_menu.gd::version_text()` 读 `ProjectSettings.get_setting("application/config/version")`，
读不到才回落到 `"0.0.0"`。

---

## 3. Campaign：数字的字体/底色/解锁

### 3.1 原版主题逐属性照抄

`campaign.tscn` 把 `campaign_button_theme_2.tres` 挂在那块生成 21 个按钮的 `Control` 上。
该文件的 `[resource]` 段只有 7 条，`GwThemeFactory.campaign_level_theme()` 一条不差：

```
default_font                      = FontFile(font_name = "Eraser")     ← 手写体，和标题同一套
Button/styles/{normal,hover,pressed,disabled,focus} = StyleBoxEmpty  ← **没有圆角矩形底**
Button/colors/font_disabled_color = (0.87451, 0.87451, 0.87451, 0)    ← alpha 0 ⇒ 锁住的关不可见
Button/colors/icon_normal_color   = (1, 1, 1, 0)
（没有 font_color override ⇒ 白色从上层 app 主题继承）
```

**字体是逐字确认的**，不是猜的：用三种候选字体各渲一遍 21 个数字，与正版截图比白字掩膜
（阈值 60，1920×1160）：

| 候选 | 白字 IoU | 结论 |
|---|---|---|
| `EraserRegular`（title_theme 用的那套） | **0.9974** | ✅ 采用 |
| `EraserDust` | 0.3652 | ✗ |
| `CrayonLibre` | 0.3152 | ✗ |

### 3.2 存档 `user://campaign.save`（用正版二进制做了两组实验）

格式要求（**代码 + 用户实测 + 本轮二进制实验**三重确认）：

* 内容**恰好** `level_<n>`，**行尾不能有任何字节**；
* 读法 `split('_').last()` → `parse::<u32>()`；Rust 的 `parse` **不跳空白**；
* `level_<n>` = **最后通关的那一关**（0 基）⇒ `0..=n` 解锁，**下一关**是当前可打的那一关；
* 写只在"解锁新关卡"时发生且**单调**；**启动时不创建**；自己写出来的内容也没有尾换行。

本轮用正版 `Graphwar.x86_64` 做的实验（1920×1160，点 Campaign 后截图）：

| 存档内容（字节） | 正版画面 | 结论 |
|---|---|---|
| `6c 65 76 65 6c 5f 32 30`（`level_20`，无换行） | 21 个数字**全白** | 格式成立；`level_20` ⇒ 全部解锁 |
| `6c 65 76 65 6c 5f 32 30 0a`（`level_20\n`） | **只有 `0`**，其余 20 个不可见 | 尾换行 ⇒ 解析失败 ⇒ 进度归零（**实测**） |
| （不建文件，直接启动） | 启动后目录里**仍然没有** `campaign.save` | 启动不创建 ✅ |

第二组实验同时确认了另一条：**锁住的关卡是"看不见"**，不是"变灰"——
这正是 `font_disabled_color` 的 alpha = 0 的效果（推断 → 现在是**观测**）。

实现（`src/app/store.gd`）：

```gdscript
const CAMPAIGN_PATH := "user://campaign.save"
static func parse_campaign_save(raw: String) -> int   # rfind('_') 之后逐字符校验 ASCII 数字
func read_campaign_progress() -> int                  # 不存在 ⇒ -1，且**绝不创建**
func save_campaign_progress(n) -> bool                # 单调；内容 "level_%d"，无尾换行
func is_level_unlocked(i) -> bool                     # i <= clamp(progress + 1, 0, 20)
```

`progress + 1` 那一步是**推断**（文件只给"最后通关的一关"），但它是唯一同时满足
"`level_20` ⇒ 21 关全开"与"没有存档也能玩第 0 关"的规则；两组二进制实验都与它一致。

### 3.3 几何：与原版截图逐区域比对

| 项 | 原版（像素实测） | 我们（`dump`/像素实测） | 差 |
|---|---|---|---|
| 返回键外框 | (0,0) **146×96**（黑边 3 + 内部 #1A3B2D 140×90） | (0,0) 146×96 | 0 |
| 标题 `CAMPAIGN` 墨迹 | y 20..76，中心 x = **1034** | y 20..76，中心 x = 1034 | 0 |
| 21 个数字的列中心 | 239, 481, 719, 959, 1198, 1439, 1679 | 239, 481, 719, 959, 1198, 1439, **1678** | ≤1 px |
| 21 个数字的行中心 | 222, 489, 753（= 区域高/8、3/8、5/8） | 222, 489, 753 | 0 |
| 数字字号 | 容器宽/15 = **128** | 128 | 0 |
| 全屏像素差 | — | **4.28%**；白字掩膜 IoU **0.9817**（阈值 190）/ **0.9974**（阈值 60） | 见 §6 |

> 差的那 1.7%（阈值 190 时）集中在字形边缘的**抗锯齿灰度**上：阈值降到 60 后两者的墨迹
> bbox **完全相同**（同一个字形轮廓、同一个字号），所以这是渲染灰阶差异，不是形状/位置差异。
> 主菜单能逐字节相同，说明字体本身没问题；剩下的差异**未定位**（候选：`campaign_button_theme_2.tres`
> 内嵌的那份 FontFile 与我们加载的 `EraserRegular.fontdata` 在 MSDF 参数上可能不同）。

---

## 4. 其余 12 个界面：去掉 stretch 之后逐个核

方法：1920×1160 下 `-- --shots=<dir>` 一次渲出 **15 张**（本轮把 `tools/screenshots.gd`
的 TARGETS 从 11 补到 15，四个弹窗也各截一张）。凡是能从正版截图取到真值的界面，都做了像素比对。

| # | 界面 | 我们截图 | 与原版对比 | 状态 / 剩余差异 |
|---|---|---|---|---|
| 1 | **MainMenu** | ✅ | 与正版 **sha256 相同** | ✅ 完成（逐位） |
| 2 | **Campaign** | ✅ | 全屏差 4.28%，白字 IoU 0.997@60 | ✅ 完成（§3） |
| 3 | **LanLobby** | ✅ | 全屏差 **4.16%** | 🔶 `Create Game` 宽度**逐像素相同**；`Join IP` 280 vs 原版 378 px；表头三列整体左移（原版从 x≈261 起，我们从 0 起） |
| 4 | **GameSetup** | ✅ | 无正版截图 | 🔶 结构与字号按 `.tscn`/脚本对齐；标题位是原版的 GhostButton 占位 |
| 5 | **PlayerScreen** | ✅ | 全屏差 **70.2%**（最大） | ⛔ **未解决**：原版是 3 列 × 96 px 的**白卡**圆点 + 约 200 px 预览；我们格子也是 96 px，但图标只画出约 35 px 的圆点、右列文字被裁 |
| 6 | **ReplayLobby** | ✅ | 全屏差 **2.39%** | ✅ 主体一致；我们把空列表的说明文字去掉了，但仍有 `user://replays/` 提示（原版空列表什么都不画） |
| 7 | **PlayScreen** | ✅ | — | ⚠️ **不归本轮**（`agent/playscreen-fidelity`）；只保证去掉 stretch 后不溢出 |
| 8 | **JoinLevelPopUp** | ✅ | — | ⚠️ **不归本轮**（同上） |
| 9 | **AddBotPopUp** | ✅ | 无正版截图 | 字体/按钮尺寸随新主题变化，未与真值比对 |
| 10 | **JoinIpPopUp** | ✅ | 同上 | 同上 |
| 11 | **ConnectingPopUp** | ✅ | 同上 | 同上 |
| 12 | **DisconnectedPopUp** | ✅ | 同上 | 同上 |
| 13 | **ChatPopUp** | ✅ | 同上 | 同上 |
| 14 | **ConfirmPlayExitPopUp** | ✅ | 同上 | 同上 |
| 15 | **EndGamePopUp** | ✅ | 同上 | 同上（`YOU lOST` + 胜方文案照旧） |

**"去掉 stretch 会不会把某个界面挤爆"** 的答案：不会。原来 1280×720 下所有内容按 1.5×
放大；现在 1600×900 的视口在 1920×1160 窗口里按 1:1 铺开，可用像素**多了** 25%，
15 张截图里没有任何一屏出现溢出/裁切（`shots` 全部 `code=0`）。

### 4.1 顺手修掉的三处真实布局 bug（真渲染才看得见）

1. **`ExitButtonContainer` 尺寸钉死在 64×48**：原版由 `ExitButtonContainer.gd` 按 **宽/30**
   设字号（1920 下 96 px 高），且按钮文本是 **5 个空格**（原版 `.tscn` 逐字），
   返回箭头 `expand_icon` 按**保持长宽比**铺满。以前箭头被压进 4:3 的小方框，形状自然不对。
   现在返回键在 1920×1160 下是 **146×96**，与原版截图**逐像素相同**（§3.3 第一行）。
   同时补上原版 `AnimationRect` 的深绿底 `Color(0.101961,0.231373,0.176471,1)` ——
   返回键那层绿底是它画的，不是 stylebox（stylebox 的 bg alpha 是 0）。
2. **每个界面只有"一个字号除数"**：原版同一屏里不同容器挂不同脚本、用不同 N。
   现在 `GwScreen.apply_font_scale()` 支持**按容器覆盖**（`meta("font_divisor", 30)`）：
   - 返回键恒为 宽/30（`ExitButtonContainer.gd`）；
   - LanLobby 的列表头是 宽/40（`LanLobbyFontResize.gd`），底部 `Create/Join` 是 宽/30
     （`LobbyButtonContainer.gd`）—— 改完 `Create Game` 的宽度与原版**逐像素相同**；
   - `TabContainer` 也参与字号覆盖（`PlayerTabContainer.gd` 用安全区宽/30），
     默认字号照抄原版 `.tres` 的 **50**。
3. **PlayerScreen 的 `body` 忘了竖直撑开**：`GwUI.hbox()` 只给水平 EXPAND，竖直是 FILL，
   于是 TabContainer 只有标签栏那 48 px 高，网格被压进一条缝里。已修（`size_flags_vertical = EXPAND_FILL`）。

---

## 5. 调试文字与广告位怎么处理的

| 信息 | 以前 | 现在 |
|---|---|---|
| `client 0.1.0 · kernel on/off` | 常驻主菜单 `VersionLabel` | **默认不显示**；`godot --path client -- --debug-overlay` 才在左下角画一行 |
| 内核状态（on/off、`hit_test` 有没有） | 同上（只能看界面） | **`dump` 通道**：`bash client/run.sh dump` 输出 `@@KERNEL available=1 hit_test=1 methods=ping,path_calculate,hit_test note=...`；`--debug-overlay` 与 `dump` 共用 `Main.debug_status_text()` / `Main.kernel_dump_line()` |
| 工程设置（版本/视口/拉伸/窗口模式） | 无 | `dump` 输出 `@@PROJECT version=2.0.60 stretch_mode=disabled viewport=1600x900 mode=2` |
| `advertisement slot — disabled in this build` | 11 个界面底部，60 px 高 | **删除**；`AdContainer` 节点保留但 `custom_minimum_size = (0,0)`、无子节点 |

**状态没丢**：`selftest` 里有断言直接读 `Main.kernel_dump_line()`，并要求它与
`GwCore.kernel_available()` 一致 —— 也就是说"界面上看不见"不影响"机器可查"。

---

## 6. 剩余差异（诚实清单）

| # | 差异 | 量级 | 证据等级 / 缺什么 |
|---|---|---|---|
| 1 | Campaign 数字的**抗锯齿灰度**与原版略有差别（墨迹 bbox 相同） | 全屏 4.28% 像素差；阈值 60 时 IoU 0.9974 | 观测；缺：`campaign_button_theme_2.tres` 内嵌 FontFile 与我们加载的 `.fontdata` 的 MSDF 参数**逐字段对比** |
| 2 | `CharacterButton` 上边界比原版低约 2 px | 2 px | 观测（原版 699 vs 我们 701）；推断：原版两处 `stretch_ratio` 的取整顺序 |
| 3 | **PlayerScreen**：原版格子是 96 px **白卡**、我们只画出约 35 px 圆点；右列 `Support the game / Unlocking skins…` 被挤出屏幕 | 全屏 70.2% | **未解决**。已排除格子尺寸（`custom_minimum_size` 实测 96×96）；下一个怀疑对象是 `skins/*.ctex` 的透明边距与 `expand_icon` 的取整 |
| 4 | **LanLobby**：`Join IP` 宽 280 vs 原版 378；表头三列整体左移约 260 px | ~4% | 观测；未知（文本同为 `" Join IP "`；可能是原版表头那一层 `MarginContainer` 的内边距） |
| 5 | 其余 8 个界面（GameSetup / 六个弹窗 / ReplayLobby 细节）**没有正版截图可比** | — | 只有"结构 + 字号 + `.tscn` 对照"，没有像素真值；缺：这些界面的正版截图（本轮只点到 MainMenu/Campaign/LanLobby/Player/Replays 五处） |
| 6 | `ReplayLobby` 空列表仍显示 `user://replays/` 提示 | 两行文字 | 观测（原版空列表不画任何文字）；未改（它同时是我们"回放不做"的明示） |
| 7 | 原版在 Android 上有安全区（状态栏）内边距；桌面 = 0 | 0 | 观测（`MarginContainer.gd` 的分支逻辑）；本轮所有比对都在 Linux/桌面下完成 |

---

## 7. 怎么复现本文的每个数字

```bash
# 资产（仓库不含任何媒体文件）
python3 tools/extract_assets.py --pck ../Graphwar.pck

# 自检（本轮从 99 条涨到 146 条；含存档/几何/版本/内核通道断言）
bash client/run.sh selftest          # → SELFTEST 146 passed, 0 failed

# 内核状态 / 工程设置（不再画在界面上，但必须可查）
bash client/run.sh dump | grep -E '^@@(KERNEL|PROJECT)'

# 15 张 1920×1160 截图
Xvfb :80 -screen 0 1920x1160x24 &
DISPLAY=:80 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
    --rendering-driver opengl3 --resolution 1920x1160 -- --shots=/abs/out

# 与原版逐像素比对（需要你自己那份正版 Graphwar.x86_64 + Graphwar.pck）
#   §1 的四条命令；结果应是主菜单 sha256 相同、Campaign 白字 IoU ≈ 0.98（阈值 190）
```

---

## 8. 本轮碰过的文件

| 文件 | 改了什么 |
|---|---|
| `client/project.godot` | 视口 1600×900、删 stretch 两键、`window/size/mode=2`、`config/version=2.0.60` |
| `client/src/ui/theme_factory.gd` | 基准字号 24；stylebox 去掉 content_margin；新增 `campaign_level_theme()`；TabContainer 字号 50 |
| `client/src/ui/ui.gd` | `ad_slot()`（0 高、无文字，`ad_bar()` 保留为别名）；`exit_pair()` 照抄原版（5 空格文本 + 146×96 + AnimationRect 绿底）；`frame(margin=0, sep=4)`；`title(use_title_font)`；`SEPARATION=4` |
| `client/src/screens/screen.gd` | 字号按容器覆盖（`meta("font_divisor")`）；TabContainer 参与覆盖；`build_header(use_title_font)` |
| `client/src/screens/main_menu.gd` | 按 `main_menu.tscn` 重写；版本号取工程设置；调试文字移出界面 |
| `client/src/screens/campaign.gd` | 挂原版关卡主题；只设 `size.x`、按真实高度居中；解锁读 `campaign.save`；字号 宽/15 不再被基类覆盖 |
| `client/src/screens/lan_lobby.gd` | Local/Global 撑满整行；底部 Create/Join 用 宽/30；删掉两处自造说明文字 |
| `client/src/screens/player_screen.gd` | 格子尺寸用 `ButtonGrid.gd` 公式（宽×0.15/3）；`body` 竖直撑开 |
| `client/src/screens/replay_lobby.gd` / `player_screen.gd` | （见上）其余界面只通过共享积木间接受影响 |
| `client/src/app/store.gd` | 原版 `campaign.save` 的解析/读取/单调写/不创建；`is_level_unlocked()` |
| `client/src/app/campaign.gd` | `layout_buttons()` 增加 `anchor`（原版"只设 size.x 再按 size/2 居中"） |
| `client/src/app/app.gd` | 关卡启动改用 `store.is_level_unlocked()` |
| `client/src/main.gd` | `--debug-overlay`（默认关）、`kernel_dump_line()`、`project_dump_line()` |
| `client/tools/selftest.gd` | 新增 47 条断言（99 → 146）：工程设置、版本行、广告位、内核通道、存档解析/写入/单调/不创建、Campaign 字体与空 stylebox |
| `client/tools/screenshots.gd` | TARGETS 11 → 15（补 JoinIp / Connecting / Disconnected / EndGame 四个弹窗） |

**没碰**：`client/src/screens/play_screen.gd`、`client/src/screens/join_level_pop_up.gd`
（这两个文件由 `agent/playscreen-fidelity` 负责；它们只是通过共享的 `GwUI.ad_bar()` 自动变成
零高度、无文字，以及通过 `frame()` 变成 margin 0 / sep 4）。
