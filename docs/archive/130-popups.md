# 130 · 七个弹窗与原版对齐（含**返回按钮的真相**）

状态：**现行**。本文回答四件事：① 原版七个弹窗的返回按钮到底在不在、在哪、什么时候出现；
② 我们原来每个弹窗错在哪、怎么改的；③ 改前/改后的**逐像素差异百分比**；④ 还剩什么没验。

> 前置：`124-ui-geometry.md`（视口 1600×900、去拉伸、`frame(margin 0, sep 4)`、返回键 146×96）、
> `121-visual-fidelity.md`（资产管线/主题色）。
> 真值来源：正版 `Graphwar.pck`（**本轮新建了一条更硬的真值管线**，见 §1）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与原版字节/逐像素一致；**观测** = 能指到文件、字节或帧；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

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

## 1. 本轮新建的真值管线（比量 `.tscn` 硬，比 `import` 截图干净）

### 1.1 直接拿原版 `Graphwar.pck` 喂同版本引擎（等价性已被证明）

原版是 Godot 4.7 导出的工程。把 `Graphwar.pck` 交给 **stock `Godot_v4.7.2`**：

```bash
mkdir -p probe/run/pck probe/rust/target/debug
cp ../Graphwar.pck probe/run/pck/
cp ../libgraphwarrust.so probe/rust/target/debug/      # ← 关键：见下
DISPLAY=:93 tools/Godot_v4.7.2-stable_linux.x86_64 \
    --main-pack probe/run/pck/Graphwar.pck --rendering-driver opengl3 \
    --resolution 1920x1160 --position 0,0
```

`Rust` 扩展类（`GraphwarButton` / `GraphwarRootNode` / `GraphwarAdSwitch`）由
`res://Graphwar.gdextension` 提供，而那个文件里的库路径是**开发机路径**
`../rust/target/debug/libgraphwarrust.so`。在 pck 目录的相对位置放一份 `.so` 就能加载。

**等价性证明（逐位）**：这样跑出来的主菜单与**真二进制** `Graphwar.x86_64` 在同一
1920×1160 Xvfb 下的截图 **sha256 完全相同**
（`3d6b776c9d16c768230f2d9fb8bd0e78f6fd6b586f20cffbdda9731aee921df1`，
与 `docs/124` §2.1 记录的正版真帧一致）。⇒ 用它当"逐像素真值"不是近似。

### 1.2 受控宿主工程（`probe/harness/`）：解决 AdContainer 的 20 px 假差异

直接把单个场景当 main scene 跑也能出图，但**有一处系统性偏差**：
`AdContainer.gd::_ready()` 要 `get_node("/root/GraphwarRootNode/Ad_holder")`，单场景跑时
那个节点不存在 ⇒ `_ready()` 中断 ⇒ `custom_minimum_size` 停在 `.tscn` 里的 **60 px**。
真机运行时它会在 `visibility_changed` 时被 `_update_size()` 改成 `ad_holder.ad_size = (0,0)`。

60 px 的差别会把弹窗的竖直三段带从 **383/383/383** 压成 **363…**（实测差 20 px）。
所以本轮写了一个 30 行的受控宿主工程：

* 主场景只有一个 `Node` + `harness.gd`；
* `ProjectSettings.load_resource_pack(pck, false)` 挂上原版资源；
* 补一个 `/root/GraphwarRootNode/Ad_holder` 替身（`ad_size = (0,0)`，与桌面无广告一致）；
* 实例化 `--scene=res://<name>.tscn`，对每个 `AdContainer` 走一次 `visible=false → true`
  触发原版自己的 `_update_size()`；
* 截 `get_viewport().get_texture()`（不带窗口边框）+ 打印**每个 `Control` 的
  `position/size`**（这比从像素反推更硬）。

它的渲染键**逐条照抄** `project.binary`（`original_project_settings.json`）：
`default_clear_color = (0.054902,0.145098,0.109804)`、`msaa_2d = 2`、
`default_texture_filter = 2`、视口 1600×900、`mode = 2`。
**验证**：宿主工程实例化原版 `main.tscn`，输出与 §1.1 的主菜单截图
**逐像素 0 差异**（PNG 字节不同只是因为编码器不同）。

### 1.3 度量：不看图猜，量矩形

`probe` 里两个脚本（都在 `.agent-work/`，不入库）：

* `measure_win.py` —— 面板填充色（#1A3B2D）最大连通域 → 外框；内部亮像素按行切带 → 文案行/
  按钮行；纯黑连通域 → 每个按钮的 3 px 黑边环；
* `pixdiff.py` —— 给若干矩形，报"有差异像素占比 / >32 灰阶占比 / 平均绝对差"。

真值帧与我们的帧用**同一段代码**量，避免"两套口径"。

### 1.4 我们自己的截图

```bash
DISPLAY=:93 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
    --rendering-driver opengl3 --resolution 1920x1160 --position 0,0 -- --shots=<dir>
```

（`client/tools/shots.sh` 里写的是 1280×720，与本轮口径不符，所以直接调引擎。）

### 1.5 踩过的三个坑（都会让结论错，记下来）

| 坑 | 现象 | 原因/处理 |
|---|---|---|
| 位置参数**必须带扩展名** | 整批截图全是主菜单 | `godot --main-pack X.pck res://confirm_exit_pop_up`（少了 `.tscn`）**静默**回落到 `run/main_scene` |
| `/tmp` 是逐命令 tmpfs | 日志文件凭空消失 | 日志一律写 `.agent-work/`（`HANDOVER` §2 早写过） |
| `pkill -f "Xvfb :91"` 会杀掉自己 | 命令直接 exit 143 | 命令行里含同一字符串；改用 `xdotool getwindowpid` + `kill` |
| `--dump-tree` 对**隐藏**的弹窗没用 | 所有弹窗的 `rect` 都是最小尺寸（如 764×208） | 未 `show()` 的子树没被布局；弹窗几何只能从**真实渲染帧**或运行时 `print` 拿 |

---

## 2. 返回按钮的真相

### 2.1 结论（三方面互证）

| 问题 | 答案 | 证据 |
|---|---|---|
| 在不在？ | **七个弹窗全都有** | 逐位：每个真值帧左上角都有一圈 146×96 的黑边环；引擎自报的节点树里 `MarginContainer2/ExitButtonContainer` 的 `size = (146,96)` |
| 在哪？ | **`(0,0)`，屏幕左上角** | 逐位：`Control.position = (0,0)`；像素环 `(0,0) 146×96`（`endgame` 因根节点 +1 px 偏移而是 `(1,1)`） |
| 多大？ | 146×96（1920 宽） | 逐位：字号 = **屏幕宽/30**（`ExitButtonContainer.gd`）× 文本 `"     "`（**5 个空格**） |
| 什么时候出现？ | **无条件出现**，没有 `visible` 开关 | 观测：`.tscn` 里没有任何 `visible = false`；唯一的条件代码是 `MarginContainerPopUpBack.gd` 的"窗口是否超出屏幕安全区"，桌面恒不成立 ⇒ margin 全 0 |

`MarginContainerPopUpBack.gd` 逐行可读（恢复出的原文）：

```gdscript
func update_sizes():
	var safe_area = DisplayServer.get_display_safe_area()
	var window_size = DisplayServer.window_get_size()
	if (window_size.x > safe_area.size.x || window_size.y > safe_area.size.y):
		add_theme_constant_override("margin_left", safe_area.position.x)
		add_theme_constant_override("margin_right", window_size.x - safe_area.end.x)
		add_theme_constant_override("margin_top", safe_area.position.y)
```

⇒ 桌面最大化（窗口 == 安全区）时**一个 margin 都不写**，所以那个 `MarginContainer` 的
margin 全 0、尺寸 = 子节点最小尺寸 = 146×96、位置 = 父节点原点。

### 2.2 两种挂法（`.tscn` 层级 + 真帧）

* **6 个弹窗**（confirm / join_ip / connecting / disconnected / endgame / chat 之外的五个）
  —— 返回键是**根节点的兄弟浮层**：

```
Control (弹窗根)
├ MarginContainer            ← 弹窗主体（三条竖带 + 面板）
└ MarginContainer2           ← layout_mode=0（无锚点）+ MarginContainerPopUpBack.gd
   └ ExitButtonContainer     ← ExitButtonContainer.gd：字号 = 宽/30
      ├ ClipButton           ← 只画动画层（mouse_filter = IGNORE）
      └ LongPressButton      ← 真正吃点击的那个
```
  `layout_mode = 0`（Position 模式、四个 offset 都是 0）⇒ **位置恒 (0,0)**、
  尺寸被最小尺寸撑到 146×96 ⇒ 它**不可能**跟面板一起动。

* **add_bot** —— 唯一的例外：返回键在 `VBoxContainer` 的**第一条竖带**里
  （`MarginContainer [v=3] → HBoxContainer [v=3] → ExitButtonContainer`），
  因为那条带高 96、正好等于返回键高度，真帧上仍然是 `(0,0)`。

### 2.3 为什么用户"截图里看不到"

因为它的矩形（`(0,0)`–`(145,95)`）与**它下面那一屏自己的返回键完全重合**：同一位置、
同一尺寸、同一个 `return.svg`、同一个 `#1A3B2D` 底。所以原版截图上它看起来是"屏幕的
返回键"，不像"弹窗多出来的按钮"。

反过来说，我们原来的实现把它塞进面板内容区的左下角 —— 于是在原版截图里"找不到
弹窗的返回键"，在我们的截图里"面板里莫名多一个箭头"。两个现象是同一件事。

> **观测补充**：`docs/92` §4.11 记过"对局里点 `ClipButton` 不弹框、点 `LongPressButton`
> 弹框"——`ClipButton` 是动画层（`clip_children = 2`），两个按钮**叠在同一个
> MarginContainer 里**，尺寸相同、后者在上面。本轮真帧确认两者都是 146×96。

---

## 3. 逐弹窗：真值几何、错因、修法

三个"连接"弹窗与 confirm 共用同一套骨架，先把它写清楚（后面各屏只列不同处）：

```
Control（满屏）
├ MarginContainer（满屏，margin 0）
│  └ VBoxContainer（sep 4）
│     ├ HBoxContainer      [v=3]   ← 上留白带
│     ├ HBoxContainer2     [v=3]   ← 面板带
│     │  ├ MarginContainer  [h=3, ratio 1]
│     │  ├ MarginContainer2 [h=3, ratio R]   ← 面板：Panel + VBoxContainer **两个兄弟**
│     │  └ MarginContainer3 [h=3, ratio 1]
│     ├ HBoxContainer3     [v=3]   ← 下留白带
│     └ AdContainer                ← 运行时 0 高，但**它那 4 px 分隔还在**
└ MarginContainer2（(0,0)）→ ExitButtonContainer
```

**面板必须用 `Panel`（不是 `PanelContainer`）**：原版把 `Panel` 和 `VBoxContainer` 放成
两个兄弟、同时铺满 `MarginContainer2`；换成 `PanelContainer` 会被 stylebox 的
`content_margin` 再内缩一次（我们原来就是 `PanelContainer`）。

### 3.1 ConfirmPlayExitPopUp（ratio 2）

| 元素 | 原版真值（1920×1160，绝对像素） | 我们（改后） |
|---|---|---|
| 三条竖带 | 1920×382 / 1920×383 / 1920×382 | 同 |
| 面板外框 | **(482,386) 956×383** | 同 |
| 文案 Label 框 | (492,485) 936×67（h/v 都居中） | 同 |
| 文案墨迹 | x 595..1324, y 500..550 | **x 595..1324, y 500..550** |
| No 按钮 | (687,665) 154×73 | 同 |
| Yes 按钮 | (1061,665) 171×73 | 同 |
| 返回键 | (0,0) 146×96 | 同 |

改法：照 §3 的骨架 + 面板内 `[空白 1][文案行][空白 1][按钮行][空白 0.2]`，
文案与按钮各套一层 `IpPopUpMargin`（四边 10 px、字号 宽/40），按钮行两侧
`[h=3,v=3]` 弹性空格，返回键改成根节点浮层。

**面板区差异 56.717% → 0.042%**（剩的 0.042% = 155 px，是字形边缘的抗锯齿灰阶）。

### 3.2 JoinIpPopUp / ConnectingPopUp / DisconnectedPopUp（ratio 6）

| 元素 | 原版真值 | 我们（改后） |
|---|---|---|
| 面板外框 | **(243,386) 1434×383** | 同 |
| 文案墨迹 | join_ip `Ip:` x 255..318 / connecting `x 808..1105` / disconnected `x 625..1287` | **逐个相同** |
| Cancel | (758,686) 194×73 | 同 |
| Join | (976,686) 186×73 | 同 |
| Ok | (892,686) 112×73 | 同 |
| 返回键 | (0,0) 146×96 | 同 |

三个各自的小结构（原版如此，照抄）：

* `join_ip`：文案行 = `[Ip: 壳(不参与弹性)][LineEdit 壳(h=3)]`；按钮行 =
  `[h=3,v=3] · Cancel 壳 · Join 壳 · [h=3,v=3]`；
* `connecting` / `disconnected`：文案行只有一格 `[h=3]`；按钮行在 Cancel/Ok **右边**
  还有一个**空**的 `IpPopUpMargin` 壳（20 px 宽）⇒ 按钮组比正中**偏左 10 px**。
  这是原版 `.tscn` 里就有的（照抄，不是笔误）。

改法：三屏共用 `connect_popups.gd` 的一份实现（原版三个 `.tscn` 结构逐节点相同）。
另删掉我们自造的 `placeholder_text = "host:port"`（原版输入框是空的）。

**面板区差异**：join_ip 77.373% → **0.033%**；connecting 77.188% → **0.000%**；
disconnected 73.465% → **0.000%**（后两屏**逐像素完全相同**）。

### 3.3 AddBotPopUp（三条带 1:10:1 + 面板 ratio 6）

| 元素 | 原版真值 | 我们（改后） |
|---|---|---|
| 竖带 | **96 / 956 / 96** | 同 |
| 面板外框 | **(243,100) 1434×956** | 同 |
| 六个格子 | (508/829/1150, 216 与 626) **261×261** | 同 |
| Cancel | (863,973) 194×73 | 同 |
| 返回键 | (0,0) 146×96（在第一条带里） | 同 |

改法要点（两处是"我们自己加的"东西，真帧上直接露馅）：

1. **删掉 "ADD BOT" 标题**：原版这块面板里一个字都没有（`.tscn` 45 节点里没有标题 Label）。
2. 格子 = `AspectRatioContainer`（比例 1、**默认** `stretch_mode`）里塞一个只有图标、
   `expand_icon = true` 的按钮，**没有** `custom_minimum_size`：格子的 261×261 是
   "行高 405 + 比例 1 + 宽 261" 算出来的，我们原来钉的 96/120 是自造值。
3. 每条格行 = `M(1) AR AR2 M4(.2) AR4 M3(.2) AR4 M5(1)`，两行的子节点**同名**（原版如此）。

**面板区差异 78.097% → 1.676%**，其中：
**六个格子之外只有 2 px 不同（0.0002%）**，1.676% 全部落在格子内部的图标上 —— 见 §4.1。

### 3.4 EndGamePopUp（三条带 1:3:1，**没有** AdContainer）

| 元素 | 原版真值 | 我们（改后） |
|---|---|---|
| 弹窗根 | **(1,1) 1920×1160**（`.tscn` 四个 offset 都是 1.0） | 同 |
| 竖带 | **230 / 691 / 231** | 同 |
| 面板外框 | **(483,235) 956×691** | 同 |
| 返回键 | (1,1) 146×96 | (0..1,0..1) 146×96（见 §4.2） |
| 十个胜利者格子 | 静态 **`visible = false`**、`[h=3]`、`stretch_mode = 1` | 同 |

三处修正：

1. **删掉 AdContainer**：原版 VBox 只有三条带（所以分隔只有 2 个 4 px，比例正好 1:3:1）。
   我们原来多塞了一个 0 高 AdContainer，多出一条分隔 ⇒ 三条带从 230/691/231 变成 229/689/229。
2. 两个 Label 是**静态** override（`SelfVictoryLabel = 70` + `title_theme` 手写体、
   `TeamVictoryLabel = 40`），**不跟窗口缩放**；而 `GwScreen.apply_font_scale()` 会把每个
   Label 都改成 宽/30。现在在 `_apply_fixed_metrics()` 里盖回去（接在基类的
   `size_changed` 之后连，保证"后执行"）。
3. 图标行的 `separation` 由 `HBoxVictoryIcon.gd` 设成 **-屏幕宽/10 = -192 px**（图标叠着画），
   不是我们原来的正间距。

**面板区差异 62.586% → 9.604%**。分区看：

| 子区域 | 改前 | 改后 |
|---|---|---|
| 面板**左/右**边框带（无文字） | 99.99% / 100% | **0.000% / 0.000%** |
| 面板上/下边框带（窄） | 100% | 0.84% / 1.58% |
| 文案行（SelfVictory） | 100% | 8.96% |
| 文案行（TeamVictory） | 100% | 11.85% |
| 图标行 | 52.18% | 10.03% |

剩下的 9.6% 全是**原版静态场景里没有内容的那两块**：我们的结算框在运行时填了
`YOU lOST` / `Team N Victory!` 和胜利者的头像 —— 见 §4.2。

### 3.5 ChatPopUp（整屏浮层：96 / 62 / 994）

原版 `chat_pop_up.tscn` **只有 7 个节点**，但它是**整屏**的：

| 元素 | 原版真值 | 我们（改后） |
|---|---|---|
| `ChatContainer` | (0,0) 1920×1160（左右各 10 px） | 同 |
| 发送键（**看不见**） | (10,0) 1900×**96** | 同 |
| `ChatLineEdit` 黑边框 | **(10,100) 1900×62** | 同 |
| `ChatBox` 上边界 | **y = 166** | 同 |
| `ChatBox` 颜色 | (5,14,10) = 黑 62.7% 压在 `#0E251C` 上 | 同 |

改法：从"640×360 对话框"改成原版的整屏三段；两块黑都是
`StyleBoxFlat(bg = Color(0,0,0,0.627451))`（输入行**带** 3 px 黑边 + 圆角 10，聊天区**不带**）；
发送键用 `invisible_theme.tres` 的形状 —— 注意它是
**透明底 + 3 px 透明边框**（`invisible_button.tres`），不是 `StyleBoxEmpty`：
那 3 px 边框给了按钮 6 px 的 content margin，正是它决定了按钮 96 px 高
（⇒ 输入行落在 y=100）。我们一开始用 `StyleBoxEmpty`，输入行就高了 5 px。

还有一条**看不见但会要命**的属性：原版这三层（`Control` / `ChatContainer` /
`ChatContainer2`）的 `mouse_filter` 全是 **2（IGNORE）**。也就是说聊天浮层除了
输入行与聊天区之外**不吃鼠标**——否则顶部那条"看不见的发送键"区域会把
"再点一次聊天开关"的点击吞掉，聊天就关不掉了。本轮一并照抄（`send` 那个按钮
本身也是 IGNORE：它没有 `input_text`，原版点它什么都不发生）。

---

## 4. 剩余差异与"无法验收"

### 4.1 `AddBotPopUp` 六个格子里面的图标（约 1.7%）

格子外框**逐像素相同**（261×261，位置全同），但每个格子内部有 3.9%–8.5% 的像素差。
量过图标外接框：真值 122×138、我们 121×137（差 1 px）。原因写得明白但**未解决**：

* 原版的按钮类是 **`GraphwarButton`（Rust 原生类，`libgraphwarrust.so`）**，
  我们的 `GwButton` 只是 Godot `Button` 的子类；
* 两者都用 `expand_icon = true` 把 512×512 的 `skins/bot_N.svg` 缩到按钮内框
  （≈255 px）绘制，但**内框差 1 px**（`GraphwarButton` 的内容内边距/图标排布与
  `Button` 不完全相同）⇒ 缩放比差 0.4%，边缘抗锯齿整圈不同。
* 证据等级：**观测**（差 1 px 是量出来的）；**未知**：`GraphwarButton` 的图标布局实现
  （只有原生机器码，本轮没有为这一像素差去反编译它）。

### 4.2 `EndGamePopUp` 的"填满之后"没有真值

结算框只在**有人全灭**时出现（`docs/92` §4.13）。本轮的所有真值都是"静态场景"
（十个格子 `visible = false`、两个 Label 空字符串），因为：

* 无法把正版打到结算（要真的打完一局），所以**"填满图标之后长什么样"没有真值**；
* 因此这一屏只对齐了**空结算框**的几何：面板 (483,235) 956×691、三条带 230/691/231、
  两个 Label 的静态字号 70/40、返回键、十个格子的属性 —— 这些是**逐位**的（左右边框带 0.000%）；
* **推断**：运行时那 10 个格子多大，原版 `.tscn` 里没有任何尺寸（`stretch_mode = 1`
  会让**唯一**显示的那个格子撑到行高 543 px，画出来是一张 543 px 的大头像）。
  这里取 `行高 × 0.5 = 271 px` 作正方形——理由是 HBox 按 `separation = -192` 分给
  10 个格子正好是 268 px，两者一致。**这条是推断，没有真值**，代码里已标注。

返回键在 endgame 上仍有 1 px 级差异：真值帧里是 `(1,1) 146×96`（根节点 +1 px 偏移），
我们的环检测到 `(0,0) 147×97`——位置/尺寸都对，只是抗锯齿边缘把连通域撑大了 1 px。

### 4.3 `ChatPopUp` 不能用像素差验收

聊天区是**半透明黑（62.7%）**，会把底图混进来：真值的底图是纯底色 `#0E251C`，
我们的底图是 PlayScreen（有白色棋盘格）。所以"同一矩形逐像素比"必然差 70%+，
**这个数字不代表弹窗错**。本轮改用三条**结构量**验收（对齐同一列 x=960 的亮度剖面）：

| 结构量 | 原版 | 我们 |
|---|---|---|
| 输入行黑边框环 | (10,100) 1900×62 | 同 |
| 聊天区上边界 | y = 166 | 同 |
| x=960 剖面 y ∈ {96,99,101,103,158,162,165,168,172} | (14,37,28)/(0,0,0)/(5,14,10)… | **逐点相同** |

⇒ 结论：**几何逐像素相同**（逐位级），但"整帧百分比"给不出来。

### 4.4 其余

* 我们的截图是"弹窗盖在某个基础界面上"，真值是"弹窗盖在纯底色上"。**面板、按钮、返回键
  都是不透明或近不透明**，所以上表的百分比可信；但**贴边抗锯齿**（圆角、箭头边缘）
  会因底图不同而混入少量差异 —— 例如"返回键矩形"这一块在所有 6 个弹窗上恒为
  **4.616%**（= 647 px，全是圆角黑边的抗锯齿），与弹窗内容无关。
  返回键**本身**的证据是：环 `(0,0) 146×96` 相同 + 箭头墨迹 bbox
  **`x 41..104, y 26..69` 完全相同**。
* 本轮**没有**给 `selftest` 加断言：`client/tools/selftest.gd` 不在本任务的可改文件清单里。
  现有 **146 条**（改前改后都是 146 passed, 0 failed）。

---

## 5. 怎么复现本文的每个数字

```bash
# ① 资产（仓库不含任何媒体文件）
.venv/bin/python ../tools/extract_assets.py --pck ../Graphwar.pck

# ② 真值：受控宿主工程（§1.2）逐场景出图 + 引擎自报节点几何
mkdir -p .agent-work/popups/probe/run/pck .agent-work/popups/probe/rust/target/debug
cp ../Graphwar.pck .agent-work/popups/probe/run/pck/
cp ../libgraphwarrust.so .agent-work/popups/probe/rust/target/debug/
bash .agent-work/popups/shoot_h.sh <outdir> \
     confirm_exit_pop_up endgame_pop_up add_bot_pop_up chat_pop_up \
     join_ip_pop_up connecting_pop_up disconnected_pop_up

# ③ 我们的 17 张 1920×1160 截图
DISPLAY=:93 tools/Godot_v4.7.2-stable_linux.x86_64 --path client \
    --rendering-driver opengl3 --resolution 1920x1160 --position 0,0 -- --shots=<ours>

# ④ 度量与比对
python3 .agent-work/popups/measure_win.py  <png>            # 面板/文案/按钮矩形
python3 .agent-work/popups/pixdiff.py <truth> <ours> x0 y0 x1 y1
```

---

## 6. 本轮碰过的文件

| 文件 | 改了什么 |
|---|---|
| `client/src/screens/confirm_exit_pop_up.gd` | 按 `.tscn` 逐节点重写：三条竖带 + 1:2:1 面板 + `Panel`+`VBox` 兄弟 + 居中 J 文案 + 按钮行 + 返回键浮层 |
| `client/src/screens/connect_popups.gd` | 三个连接弹窗共用一份重写（1:6:1 面板、`IpPopUpMargin` 10 px、按钮行两侧弹性空格、空的 Ip 壳）；删掉自造的 `host:port` 占位符 |
| `client/src/screens/add_bot_pop_up.gd` | 三条带 1:10:1 + 面板 ratio 6 + 六个 `AspectRatioContainer` 格子（无 `custom_minimum_size`）；删掉自造的 "ADD BOT" 标题；返回键移进第一条带 |
| `client/src/screens/endgame_pop_up.gd` | 三条带 1:3:1、去掉 AdContainer、根节点 +1 px 偏移、两个 Label 静态字号 70/40（盖住基类的 宽/30）、图标行 `separation = -宽/10`、格子静态隐藏 |
| `client/src/screens/chat_pop_up.gd` | 从 640×360 对话框改成整屏三段（96/62/994）；两块黑底 stylebox；发送键用 `invisible_button.tres` 的形状 |
| `client/src/screens/join_ip_pop_up.gd` / `connecting_pop_up.gd` / `disconnected_pop_up.gd` | **未改**（只有 `kind`，实现全在 `connect_popups.gd`） |
| `docs/130-popups.md` | 本文 |

**没碰**（按任务约定）：`join_level_pop_up.gd`、`play_screen.gd`、`plane_view.gd`、
`core_bridge.gd`、`player_screen.gd`、`lan_lobby.gd`、`replay_lobby.gd`、
`client/src/audio/`、`client/src/ui/*`、`src/`（Rust）、`docs/INDEX.md`、`docs/HANDOVER.md`、
`client/tools/selftest.gd`、`client/tools/screenshots.gd`。

### 关于共享积木（`client/src/ui/ui.gd` / `theme_factory.gd`）

**本轮一次都没改共享积木，也不需要改**：面板用 `Panel`（`Panel/styles/panel` 主题里已经是
`#1A3B2D` + 3 px 黑边 + 圆角 10）、按钮用 `GwButton.make()`、返回键用现成的
`GwUI.exit_pair()`、字号用 `GwScreen` 已有的 `meta("font_divisor")` 覆盖机制，
布局比例全部用 `size_flags_stretch_ratio` 直接写在各弹窗里。

两个"共享积木里值得改、但不该由本轮改"的点（留给后续/维护者判断，**未改**）：

1. `GwUI.dialog(root, panel_min)` —— 全项目只剩 `join_level_pop_up.gd` 在用（那个文件由
   另一位 agent 负责）。它做的是"按内容最小尺寸居中"，**原版没有任何一个弹窗是这么布局的**
   （原版全是 `1:2:1` / `1:6:1` 的等分列）。建议它将来被 `1:R:1` 骨架取代。
2. `GwThemeFactory` 的 `Label` 没有"静态字号"通道 —— `apply_font_scale()` 无条件把每个
   Label 改成 宽/N，所以 endgame 那两个静态 70/40 只能在 `endgame_pop_up.gd` 里
   接一次 `size_changed` 再盖回去。若将来有更多"原版静态字号"的节点，
   建议在 `screen.gd` 里支持 `meta("font_size_fixed", px)`。
