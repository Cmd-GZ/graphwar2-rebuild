# 137 · 像素级保真：**给还没有真值的屏造真值** + 残差归零

状态：**现行**。本文回答四件事：

1. **哪些屏现在有真值了、用什么管线造的**（本轮新建了"整棵主场景 + 只显示一个界面"的宿主模式，
   它让 `GameSetup` 第一次有了逐像素真值，也给弹窗补上了"盖在**同一个基础界面**上"的真值）；
2. **逐屏改动前后差异百分比**，以及哪些屏**仍然没有真值**、缺的到底是什么；
3. **PlayerScreen 残留 0.49% 到底是什么**——查清并修掉了，现在是 **0.007%**；
4. **默认玩家形象**（`yellow + regular_eyes + soldier_helmet`）的证据链，以及原版
   `user://player.config` 的读取语义（含"逐键兜底"的实测结论）。

> 前置：`124-ui-geometry.md`（视口/去拉伸/字号除数的来源）、`129-player-screen.md`（玩家页与
> LanLobby/ReplayLobby 的残差台账）、`130-popups.md`（弹窗骨架 + 第一条宿主真值管线）、
> `121-visual-fidelity.md`（资产管线）。
> 真值来源三件套：**正版 `Graphwar.x86_64`**（逐位等价性已被 `124` §2.1 证明）、
> **正版 `Graphwar.pck` + `libgraphwarrust.so` 喂 stock Godot 4.7.2**（`130` §1.1）、
> 以及本轮新增的 **`main.tscn` 整树宿主模式**（§1.2）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 与原版字节/逐像素一致；**观测** = 能指到文件、字节或帧；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

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

## 1. 真值管线（本轮新增第三条）

### 1.1 三条管线一览

| # | 管线 | 能给什么 | 局限 |
|---|---|---|---|
| A | **正版二进制** `Graphwar.x86_64` + Xvfb 1920×1160 + `import -window root` | 运行时**真实**画面（含玩家表、房间列表、具体选项状态） | 只能到"点得出来"的屏；`Create Game` 在离线环境**点了没反应**（§1.5） |
| B | **单场景宿主**（`docs/130` §1.2）：pck + `load_resource_pack` + 补 `Ad_holder`，实例化一个 `.tscn` | 任意屏的**静态**场景 | 弹窗只能盖在**纯底色**上；场景里运行时才填的内容是空的 |
| C | **整树宿主（本轮新增）**：实例化原版 `main.tscn` 整棵树，只把 `ScreensControl` 下指定那一个子节点设成 `visible` | 任意屏的**静态**场景 + **弹窗盖在原版同一个基础界面上** + 引擎自报的**每个 Control 的 rect** | 同上：运行时才填的内容仍然是空的（玩家条目 / 房间条目） |

管线 C 的宿主脚本在 `.agent-work/pixfit/harness/harness.gd`（不入库），关键点三条：

```gdscript
# ① 原版 main.tscn 的根节点**就是** GraphwarRootNode，挂到 /root 下之后
#    `AdContainer.gd` 的 get_node("/root/GraphwarRootNode/Ad_holder") 天然成立（不需要替身）
var inst: Node = load("res://main.tscn").instantiate()
inst.name = "GraphwarRootNode"
get_tree().root.add_child(inst)

# ② 只留一个界面可见（弹窗用 --base= 再把底图那个也设成可见）
for ch in screens.get_children():
    (ch as CanvasItem).visible = String(ch.name) in [_only, _base]
    if not (String(ch.name) in want):
        ch.set_process(false)          # ③ 见下面的坑
```

### 1.2 命令（可直接照抄）

```bash
# 真值 A：正版二进制（点哪算哪，受控输入 + 截根窗口）
cd <repo>/..                          # 有 Graphwar.x86_64 / Graphwar.pck 的那一级
bash .agent-work/pixfit/truth_run.sh 91 truth acts/a7.txt
#   acts/a7.txt 内容举例：
#     click 536 749      # 主菜单 CharacterButton
#     wait 3
#     shot player_skins

# 真值 C：整树宿主（GameSetup 就是这样造出来的）
bash .agent-work/pixfit/shoot_harness.sh truth_h --main --only=GameSetup --name=GameSetup
#   → truth_h/GameSetup.png（1920×1160）+ truth_h/nodes/GameSetup.tree（401 个 Control 的 rect）

# 我们自己的截图（两个口径：一次性 17 张 / 受控输入）
bash .agent-work/pixfit/shots_ours.sh ours_final        # 引擎内截图，17 张
bash .agent-work/pixfit/ours_run.sh ours_live acts/o2.txt   # 跑真窗口 + xdotool 点击

# 度量
python3 .agent-work/pixfit/pixdiff2.py <truth.png> <ours.png> \
    --out diff.png --regions 482,386,1437,768
```

### 1.3 四个坑（都会让结论错）

| 坑 | 现象 | 处理 |
|---|---|---|
| **`main.tscn` 里**所有界面的原生 `_process` 都在跑 | 一实例化整棵树，`graphwar_plane.rs:254` / `entity_drawer.rs:587` / `graphwar_setup_control.rs:813` 全在 `Option::unwrap()` 上 panic，渲染循环被打停，**截图存不出来** | 把不显示的那 14 个界面 `set_process(false)`（§1.1 的第 ③ 行） |
| 宿主脚本的 `--out=` **必须绝对路径** | `save_image: Can't save PNG at path`（err=7），`@@SHOT` 行有、文件没有 | 在驱动脚本里 `cd` 到输出目录取绝对路径 |
| `pkill -f "<自己命令行里也有的字符串>"` | 整条命令被自己杀掉（exit 143） | 按 PID 杀；或别在命令行里出现那个串（`HANDOVER` §2 早写过） |
| 宿主命令与 `Xvfb` 必须**同一条命令**里起 | 上一条命令里 `(Xvfb &)`，下一条命令 `unable to open X server` | 驱动脚本 `truth_run.sh` / `shoot_harness.sh` 把起服、点击、截图、清理写在同一个进程里 |

### 1.4 等价性复核（每轮都要重做的第一步）

真值必须先自证。每次开工的第一件事：**用 A 管线截主菜单，与 `docs/124` §2.1 记的 sha256 对比**：

```
orig_mainmenu.png  sha256 3d6b776c9d16c768230f2d9fb8bd0e78f6fd6b586f20cffbdda9731aee921df1
```

本轮实测**同值**（113780 B）⇒ 这套 Xvfb/分辨率/渲染后端和 `124` 完全一致，后面所有百分比可比。

### 1.5 `Create Game` 为什么点不动（管线 A 的天花板）

正版二进制里 LanLobby 的 `Create Game`（`input_text = "create_game"`，`docs/92` §4.5）在离线环境
**点下去什么都不发生**（试过单击 / 长按 0.8 s / 悬停后再点，截图与点击前**逐字节相同**）。
同时 Global 页**能拿到真实房间列表**（5 个房间，见 §7.2），说明"后端可达、只是 `create_game` 这条
路径没有离线分支"。⇒ GameSetup 只能走管线 B/C。

---

## 2. 真值清单（本轮结束时的状态）

| 屏 | 真值文件 | 管线 | 证据等级 |
|---|---|---|---|
| MainMenu | `truth/mainmenu.png` | A（sha256 与 `124` 相同） | **逐位** |
| Campaign | `truth/campaign.png` | A | 观测 |
| LanLobby / Local | `truth/lanlobby.png` | A | 观测 |
| LanLobby / Global | `truth/try_global.png`（**真实服务器数据**） | A | 观测 |
| PlayerScreen / Skins·Faces·Hats | `truth/player_{skins,faces,hats}.png` | A（**全新 `XDG_DATA_HOME`**，见 §6.1） | 观测 |
| ReplayLobby | `truth/replays.png` | A | 观测 |
| **GameSetup** | `truth_h/GameSetup.png` + `.tree`（401 节点） | **C（本轮新建）** | 观测（静态场景） |
| 7 个弹窗 | `.agent-work/popups/truthH/*.png`（`130` 建的） | B | 观测（静态场景） |

> `truth/` 与 `truth_h/` 都在 `.agent-work/pixfit/`（工作区，不入库）。
> 复现时注意：**同一个文件名会被后续实验覆盖**——本轮做 `player.config` 实验时就把
> `truth/player_skins.png` 覆盖成了"皮肤无效"的那一帧，导致一次假结论（§3.4 末尾有记录）。

---

## 3. 残差归零：0.49% 到底是什么

### 3.1 第一刀：工程少了两个渲染键（0.490% → 0.029%）

原版 `project.binary` 里有、我们 `project.godot` 里**没有**的两个键：

```
rendering/textures/canvas_textures/default_texture_filter = 2
rendering/anti_aliasing/quality/msaa_2d = 2
```

补上之后 PlayerScreen **0.490% → 0.029%**，主菜单仍是 0.000%（说明不是"把图弄模糊了"），
LanLobby 也从 0.029% 掉到同一档。**观测**：两个键取自 `client/assets/original_project_settings.json`。

> `msaa_2d` 在 GLES3 下是不生效的（正版自己的日志里就有
> `WARNING: 2D MSAA is not yet supported for GLES3`），所以真正起作用的是 `default_texture_filter`。
> 这条是**推断**（A/B 没分开做），但两个键都是原版值，留着不影响正确性。

### 3.2 第二刀：返回键把同一个箭头画了两遍（0.029% → 0.007%）

原版 `ExitButtonContainer` 里是**两个** `GraphwarButton`，都带 `icon = return.svg`：

```
ExitButtonContainer
├ ClipButton      theme = long_press_ghost_button_theme.tres   normal = #0F281D **不透明**
│  └ AnimationRect(show_behind_parent, #1A3B2D)
└ LongPressButton theme = long_press_button_theme.tres         normal = #1A3B2D **不透明**
```

我们以前给**两个**都写透明 stylebox（靠 `AnimationRect` 画绿底），于是两个箭头**叠着画**：
位置一样，但抗锯齿边缘按两次 alpha 合成 ⇒ 边缘比原版更白。实测证据（像素级）：

| 位置 | 真值 | 我们（改前） | 结论 |
|---|---|---|---|
| 箭头墨迹 bbox | x 41..104, y 25..69 | **同** | 位置没错 |
| 边缘像素（y=53, x=42） | (47,77,65) → α≈0.77 | (67,94,83) → α≈0.95 | 0.95 ≈ 1−(1−0.77)² ⇒ **画了两遍** |

改法（`GwUI.exit_pair()`，共享积木）：两个按钮各自带**原版那份不透明底**，
`LongPressButton` 的 `hover/pressed` 用透明底（原版如此，那就是"按下时露出下层"的动画）。

**效果**：返回键矩形 **4.616%（647 px）→ 1.084%（150 px）**，而且这个残差在**所有屏**上一起消失。

### 3.3 剩下的 150 px 是什么

PlayerScreen/LanLobby 现在只剩 **152 px**，全部落在返回键的 **4 个圆角**里（每个 60×60 块 38 px，
4 块 = 152），最大差 22 灰阶。**观测**：位置、尺寸、箭头 bbox 全部相同 ⇒ 是圆角抗锯齿的合成差异
（两层 stylebox 的圆角叠加）。**未知**：`GraphwarButton`（Rust 类）的圆角绘制是否与 Godot
`StyleBoxFlat` 逐像素一致——只有原生机器码，本轮没有为这 22 灰阶去反编译它。

### 3.4 Faces / Hats 的残差（0.977% → 0.572%、2.282% → 0.345%）

`tol>32` 只剩 0.190% / 0.112%，且**均匀分布在每个格子内部**（每 160×160 块约 150 px）。
放大一个格子看（`work/face_cell_cmp.png`）：差的是**格子内那层皮肤圆的 1 px 边缘环**，
不是图标身份、不是格子几何（格子外框逐像素相同）。
**推断**：`shop_button_entry.tscn` 里皮肤层挂在 `SkinBackgroundButton` 上（有自己的一份 stylebox 内缩），
我们画这层的矩形与原版差一个亚像素。**缺的证据**：原版那层 `SkinBackgroundButton` 的运行时 rect
（静态 `.tscn` 里没有尺寸，只能从原生类里取）。

> ⚠️ **一次假结论的复盘**：做 §6 的 `player.config` 实验时，`truth/player_skins.png` 被实验帧覆盖
> （`skin: bogus` 那一帧皮肤层不画），随后拿它去比我们的帧得到 4.238% 的"回归"。**教训**：
> 真值文件的命名要带条件（`player_skins__default.png`），别让实验覆盖基线条。本轮之后
> 重新采集了干净的真值，上表数字都是干净真值测的。

---

## 4. GameSetup：第一次有真值

### 4.1 真值怎么来的

`Create Game` 点不动（§1.5）⇒ 走管线 C：

```bash
bash .agent-work/pixfit/shoot_harness.sh truth_h --main --only=GameSetup --name=GameSetup
```

得到 **1920×1160 静态帧** + **401 个 Control 的 rect**。⚠️ 这一帧是**静态场景**：玩家条目为空
（原版在建房时由 `GraphwarSetupControl` 填），所以 §0 表里那 0.592% 是"我们有条目、真值没有"，
**不是错误**（见 §7.3 的同类问题）。

### 4.2 原版 GameSetup 的运行时几何（宿主导出，逐条可查）

| 元素 | 真值 rect（1920×1160） | 依据 |
|---|---|---|
| 顶栏 | y 0..96 | `ExitButtonContainer` 146×96 |
| `GhostButton` | x 146..305，**159×96** | `MarginContainer4` 里，`invisible_theme`，**只占位不画** |
| `RoomNameLineEdit` | (313,0) **1307×96** | 标题！它不是 Label，是挂了 `title_theme.tres` 的 LineEdit |
| `ChatButtonSetup` / `LockButton` | (1620,0)/(1766,0) 各 **146×96** | 文本 `"     "`（5 空格）+ `expand_icon` |
| 队名 `Label` | Team1 墨迹 x 354..594 y 101..162 | `MarginContainer`(`LanLobbyTitle.gd` = 宽/30) → `Label(title_theme)` |
| 中缝 | **x 955..964 纯白（10 px）** | `MarginContainer/ColorRect(custom_minimum_size=(10,0))`，`ColorRect` **没写 color** ⇒ 白 |
| 选项行 | y 922..1056，**11 个子节点**：6 个空 Margin（201/202）+ 5 个按钮 **134×134** | 按钮是 Margin 的**兄弟**；按钮 `expand_icon = false` ⇒ 128 图标 + 3+3 |
| `AxisModeButton` | 134×134，**白卡** | 单独挂 `axis_button_theme.tres`，其四个 stylebox 全是 `axis_menu_icon_theme.tres`（**白底** + 3 px 黑边 + 圆角 10） |
| 底栏 | y 1060..1156 | `LobbyButtonContainer.gd`：每个子节点 `custom_minimum_size = (屏宽×0.2, 0)` = 384 |
| `StartButton` | **(768,1060) 384×96** | 上面那条 + 两侧 384 的空位把它居中 |
| `GameIpLabel` | **空字符串** | 真帧那个位置什么都没有 |

### 4.3 我们改了什么（`game_setup.gd`）

| # | 错在哪 | 改法 | 量级 |
|---|---|---|---|
| 1 | 整屏字号除数写 40 ⇒ 标题/队名/Start 全小一号（48 vs 64） | `font_divisor = 30`（原版这屏的 4 个尺寸脚本全是 /30，只有玩家行是 /40） | 大 |
| 2 | 标题是自造的 Label + 面板，字体不对 | 改成 LineEdit + `title_theme` 字体 + `StyleBoxEmpty` + 居中（手写体只有大写 ⇒ 真帧是 `GRAPHWAR GAME`） | 大 |
| 3 | 队名用默认字体 | 队名 Label 挂 `title_font()`（→ `TEAM 1` / `TEAM 2` 手写体） | 大 |
| 4 | 中缝是一个不可见 `Control` | 改成 **10 px 白色 `ColorRect`**（真帧那条白线） | 大 |
| 5 | 选项按钮塞在 Margin **里** ⇒ 每个撑到 316 宽 | 按钮改成 Margin 的**兄弟**，5 个 134×134 + 6 个弹性 Margin | 大 |
| 6 | 轴模式按钮是深绿卡 | 单独给**白底** stylebox（`axis_button_theme` 的四个态都是白卡） | 中 |
| 7 | 选项按钮 `expand_icon = true` ⇒ 图标不占最小尺寸 | 5 个都改 `false` ⇒ 128 图标撑出 134 | 中 |
| 8 | `GhostButton` 用 9 个空格，我们的 `Button` 空格更宽（252）⇒ 标题右移 50 px | 钉 `custom_minimum_size.x = 159`（真值），标题回到 x=313 | 中 |
| 9 | 聊天/上锁按钮 48×48 | `"     "` + `expand_icon` ⇒ 146×96 | 中 |
| 10 | 底栏三块随手写的（160×48） | 三块都 `custom_minimum_size.x = 384`（`LobbyButtonContainer` 的 0.2×屏宽） | 中 |
| 11 | `GameIpLabel` 写着 `127.0.0.1:0 (no network layer)` | 文本改成空（真帧什么都没有；说明移到 tooltip） | 小 |

**13.048% → 1.330%**（`tol>32` 5.455% → **0.462%**）。分区账：

| 区域 | 占全帧 | 说明 |
|---|---|---|
| 队名区（y 100..180） | **0.000%** | 逐像素相同（手写体 + 字号 64 全对） |
| 底栏（y 1056..1160） | **0.000%** | 逐像素相同（Start 384×96 全对） |
| 顶栏（y 0..96） | 0.581%（**tol>32 = 0**） | 只剩标题字形边缘的抗锯齿灰阶 |
| 选项行（y 922..1056） | 0.158% | `TurnModeButton` 图标身份不同（见 §4.4） |
| 玩家表区（y 180..900） | 0.592% | **我们没有真值可比**（静态场景里没有条目） |

⇒ **可验收部分 = 0.738%**。

### 4.4 剩下的两个没解决的

1. **`TurnModeButton` 图标**：真帧（静态）画的是 `turns_simultaneous`，我们默认
   `GwModes.TURN_DEFAULT = 0` ⇒ `turns_sequential`。**未知**：原版进入界面时会不会用运行时状态
   覆盖这个静态图标。**没改**——改默认值会同时改对局语义（顺序/同时回合），没有证据不动。
2. **标题边缘抗锯齿**（0.581%，最大 32 灰阶）：`RoomNameLineEdit` 的墨迹 bbox 与真值**完全相同**
   （x 681..1251 / y 22..77），只是字形边缘灰阶差一点。与 `124` §6 #1 是同一类问题
   （内嵌 FontFile 与 `.fontdata` 的 MSDF 参数未逐字段比过）。

---

## 5. 默认玩家形象（用户报的"默认错了"）

### 5.1 默认三元组：**`yellow` + `regular_eyes` + `soldier_helmet`**

三条**互不依赖**的证据：

1. **全新 `XDG_DATA_HOME`**（`user://player.config` 不存在）跑正版二进制，进 PlayerScreen：
   预览画的是"黄圆 + 白眼黑瞳 + 深绿士兵盔"（`truth/player_skins.png`）；**观测**；
2. **文件名实验**：`player.config` 里**只**写 `skin: red` ⇒ 皮肤红、脸仍是 `regular_eyes`、
   帽子仍是 `soldier_helmet`（`truth/cfg_red_skins.png`）。这同时证明"**逐键兜底**"；
3. 用户提供的 App Store 图标就是这三张的合成，层序 **黄圆 → 白眼黑瞳 → 绿盔（最上层）**。

我们 `player_showcase.gd` 的层序本来就是 `skin → face → hat`（后画压上面）⇒ **符合**，
不需要改（`GwPlayerShowcase._draw()` 里三张 `draw_texture_rect` 同一个矩形）。

### 5.2 原版 `player.config` 的读取语义（用正版二进制做了三组实验）

| 实验 | 文件内容 | 原版表现 | 结论 |
|---|---|---|---|
| ① | 不存在 | 黄圆 + 绿盔 + regular eyes | 默认三元组 = 上面那三个 |
| ② | `skin: red\n`（其它键都没有） | 红皮肤，脸/帽仍默认 | **逐键兜底**，不是"文件在就整份覆盖" |
| ③ | `skin: bogus\nface: asian\n` | **皮肤层整个不画**（只剩盔和脸），脸 = asian | 值是**名字**；未知名字 ⇒ 那一层失效 |

文件格式：`key: value`、无 `[section]`、每行 `\n` 结尾、值里可以有 `:`。
**观测**：跑完游戏**不会**把我们预置的文件重写（`skin: red` 那组跑完文件还是 `skin: red`）。

### 5.3 代码侧：默认值按**名字**解析（不再有魔法下标）

```gdscript
const DEFAULT_SKIN_NAME := "yellow"          # store.gd
const DEFAULT_FACE_NAME := "regular_eyes"
const DEFAULT_HAT_NAME := "soldier_helmet"
static var DEFAULT_SKIN := 0                  # _static_init() 里按名字解析
```

### 5.4 **两套顺序**（这是最容易踩的坑，必须写清楚）

`libgraphwarrust.so` 里存在**两份**皮肤清单，它们**不一样**：

| 清单 | 内容 | 用在哪 |
|---|---|---|
| `.rodata` 那份（23 条） | `blue, gray, pink, purple, red, target, white_skin, yellow, green, light_green, true_purple, brown, calico, wargod, arrow, face_paint_1, face_paint_2, bot_0..bot_5` | 名字池 |
| **界面网格**（15 格，本轮**实测**） | 见下表 | PlayerScreen 的 Skins 页 |

**网格顺序是量出来的，不是读出来的**：把真帧前 9 个格子的圆心主色与 25 个 `skins/*.ctex` 的主色
逐一对上（`pixdiff2.py` 同目录的分析脚本，资产用 `tools/dump_textures.gd` 解成 PNG）：

| 格 | 真帧主色 | 对应资产 |
|---|---|---|
| 1 | (129,129,255) | **blue** |
| 2 | (255,153,85) | **pink** |
| 3 | (255,129,255) | **purple** |
| 4 | (255,114,114) | **red** |
| 5 | (255,215,193) | **white_skin** |
| 6 | (255,204,0) | **yellow**（与 `face_paint_2` 同色，按位置取 yellow） |
| 7 | (105,171,102) | **green** |
| 8 | (124,255,120) | **light_green** |
| 9 | (172,134,194) | **true_purple** |

⇒ **网格顺序 = `.rodata` 顺序去掉 `gray` 与 `target`**，正好 15 格。
`SkinBlock` / `PendingBlock` **不在**网格里（它们是右列那两个"解锁"方块节点，`visible = false`）。

> ⚠️ 这条与父 agent 转述的"网格串里有 `SkinBlock/PendingBlock`"**冲突**。以**真帧实测**为准：
> 第 1 格纯色是 (129,129,255)（= blue），不是 purple。如果按"purple 打头"的顺序排，
> 第 1 格会变成紫色，与真帧立刻矛盾。

我们 `player_screen.gd` 的 `SKIN_GRID` 与此一致（`[0,2,3,4,6,7,8,9,10,11,12,13,14,15,16]`，
即跳过下标 1=`gray`、5=`target`），`FACE_GRID[0] = regular_eyes`、`HAT_GRID[1] = soldier_helmet`
也都与真帧一致 ⇒ **网格顺序不需要改**。本轮补了 3 条断言把这件事钉住（§8）。

### 5.5 我们这边两个真实缺陷（用户"默认不对"的可复现来源）

1. **自检写脏存档**：`_test_player_screen` 把 `profile.name` 改成 `selftest-user` 并 `save_all()`；
   历史构建里还出现过 `skin=0 / face=0 / hat=0` 的存档（`.agent-work/client/data/.../gw2_client.cfg`
   实测就是这个值）。跑完自检再开界面 = "自检用户 + 蓝圆 + 没有脸和帽子"。**本轮修法**：
   在新增的自检末尾把 profile 恢复成默认三件套（新增断言，未改既有断言）。
2. **没读原版 `player.config`**：`store.gd` 有 `parse_player_config()` 但 `load_all()` 从没调用过它。
   **本轮修法**：`load_all()` 在我们自己的 `gw2_client.cfg` **不存在**时调用
   `apply_original_player_config()`（逐键、按名字、未知名字保留默认）。已有我们自己的档案时
   **不覆盖**（用户在本客户端里的选择优先）。

### 5.6 存档格式的偏差（**没改**，只记录）

| | 原版 | 我们 |
|---|---|---|
| 文件 | `user://player.config` | `user://gw2_client.cfg` |
| 格式 | `key: value`，无 section，每行 `\n` | `ConfigFile`（`[profile]` / `[campaign]`） |
| 方向 | 读 + 写 | **只读**原版；自己的写自己那份 |

**利弊**：改成原版格式的收益是"用户在原版里的选择能双向同步"；代价是**值的编码没有证据**
（`muted` 到底写 `true` 还是 `1`、写回时机、字段顺序），写错会**破坏用户真机的原版设置**。
⇒ 本轮维持"只读不写"，把这条标成**已知偏差**，留给有证据的一轮。

---

## 6. 还没有真值的屏（诚实清单）

| # | 屏 | 缺什么 | 能不能补 |
|---|---|---|---|
| 1 | **PlayScreen**（战斗界面） | 有真帧（`.agent-work/playfit/shots/orig_play_*.png`），但**不归本文**（`agent/playscreen-fidelity`） | — |
| 2 | **JoinLevelPopUp** | 有真帧（`playfit/shots/orig_join_level.png`），同样不归本文 | — |
| 3 | **EndGamePopUp 的"填满"状态** | 真值只有空结算框；十个胜利者格子是 `visible = false`。要把正版打到全灭才有真值 | 需要能真正打完一局（离线做不到） |
| 4 | **LanLobby / Global 的房间条目** | 真值里是**真实服务器数据**（5 个房间：US Room 3 / EU Room 3 / EU Room 5 / EU Room 4 / US Room 2） | 数据永远变，只能验收"表头/列宽/模式图标形制"；这一页我们现在 **4.311%**，主要是**占位数据**与真实数据不同（不归本文：`lan_lobby.gd` 属联机 agent） |
| 5 | **ChatPopUp** | 半透明黑压在底图上 ⇒ 全帧百分比没有意义（`130` §4.3 已说明），只能用结构量验收 | 沿用 `130` 的三条结构量 |
| 6 | **GameSetup 的玩家条目** | 静态场景里没有条目（原版建房时由原生控件填），我们的条目没有真值 | 需要 `Create Game` 那一步（§1.5 做不到） |
| 7 | **ReplayLobby 的标题** | 有真值，差的是**标题字号**：真帧墨迹 x 900..1186、我们 x 1010..1186（我们更小） | 归回放 agent（`replay_lobby.gd` 不在本文可改清单） |
| 8 | **Campaign 标题/数字** | 有真值，0.179% 全是字形边缘灰阶（`tol>32 = 0`） | 与 `124` §6 #1 同源，需要 MSDF 参数逐字段对比 |

---

## 7. 证据分级

| 结论 | 等级 | 依据 |
|---|---|---|
| 主菜单与正版**逐字节相同** | **逐位** | sha256 相同（`truth/mainmenu.png`） |
| PlayerScreen Skins **0.007%**（152 px，全在返回键四角） | **逐位**（除 152 px） | 逐像素 + 差异 bbox |
| 返回键箭头"画了两遍" | **观测** | 边缘 α 0.77 vs 0.95 ≈ 1−(1−0.77)² |
| 少写两个渲染键导致 0.49% | **观测** | 补键后 0.490%→0.029%，其它屏不受影响 |
| GameSetup 运行时几何（§4.2 全表） | **观测** | 宿主导出 `@@NODE ... pos/size`，401 节点 |
| GameSetup 玩家条目的布局 | **未知** | 静态场景无条目，无法验收 |
| 默认三元组 = yellow/regular_eyes/soldier_helmet | **观测** | 全新数据目录真帧 + `skin: red` 逐键实验 |
| `player.config` 是逐键兜底、值是名字 | **观测** | 三组二进制实验（§5.2） |
| 网格顺序 = `.rodata` 去掉 gray/target | **观测** | 9 个格子主色与 25 个资产主色逐一对应 |
| 未知名字在原版下"那一层不画" | **观测** | `skin: bogus` 那一帧皮肤消失 |
| `TurnModeButton` 的运行时默认值 | **未知** | 静态帧是 `turns_simultaneous`，运行时是否被覆盖没有证据 |
| `GraphwarButton` 的圆角/图标绘制细节 | **未知** | 只有原生机器码，未反编译 |

---

## 8. 自检（只增不减）

起点 **245** 条，本轮 **+25** → `SELFTEST 270 passed, 0 failed`。
新增的每条都能指到**真帧里的一个数值**：

| 断言组 | 指到的真值 |
|---|---|
| 默认三元组按名字解析（3 条） + `resolve_look()` 往返/未知名字（2 条） | §5.1/§5.2 |
| `player.config` 逐键兜底、未知名字保留默认（3 条） | §5.2 实验 ②③ |
| 前 9 个皮肤格顺序、Faces/Hats 首格（3 条） | §5.4 的主色表 |
| GameSetup 字号除数 30 / GhostButton 159 / Start 384 / 中缝白色 10 px / 聊天+上锁 5 空格 / 轴按钮白卡 / 选项行 11 子节点 / 5 个 `expand_icon=false`（8 条） | §4.2 的宿主 rect 表 |
| 返回键 `LongPressButton` 的 normal stylebox **不透明**（1 条） | §3.2 |
| `default_texture_filter = 2`、`msaa_2d = 2`（2 条） | §3.1 |
| 自检结束后 profile 恢复默认（1 条） | §5.5 |
| 其余（`player.config` 现场恢复等） | — |

---

## 9. 怎么复现本文的每个数字

```bash
# 0) 资产（仓库不含任何媒体文件）
.venv/bin/python ../tools/extract_assets.py --pck ../Graphwar.pck

# 1) 真值：正版二进制（1920×1160，同一 Xvfb/后端）
bash .agent-work/pixfit/truth_run.sh 91 truth acts/a7.txt        # PlayerScreen 三页签
bash .agent-work/pixfit/truth_run.sh 91 truth acts/a8.txt        # Campaign / Replays

# 2) 真值：整树宿主（GameSetup 等"点不到"的屏）
bash .agent-work/pixfit/shoot_harness.sh truth_h --main --only=GameSetup --name=GameSetup

# 3) 我们的截图（一次性 17 张 / 受控输入）
bash .agent-work/pixfit/shots_ours.sh ours_final
bash .agent-work/pixfit/ours_run.sh ours_live acts/o2.txt

# 4) 逐屏差异
python3 .agent-work/pixfit/pixdiff2.py truth/player_skins.png ours_final/PlayerScreen.png
python3 .agent-work/pixfit/pixdiff2.py truth_h/GameSetup.png ours_final/GameSetup.png --out diff/gs.png

# 5) 自检 / 内核通道
bash client/run.sh selftest        # → SELFTEST 270 passed, 0 failed
bash client/run.sh dump | grep -E '^@@(KERNEL|PROJECT)'
#   → @@KERNEL available=1 hit_test=1 ...
```

---

## 10. 本轮碰过的文件

| 文件 | 改了什么 |
|---|---|
| `client/project.godot` | **补两个渲染键**：`default_texture_filter=2`、`msaa_2d=2`（原版 `project.binary` 的值，§3.1） |
| `client/src/ui/ui.gd` | `exit_pair()`：两个按钮各带原版那份**不透明** stylebox，arrow 只画一次（§3.2） |
| `client/src/screens/game_setup.gd` | 按真值重做 11 处（§4.3） |
| `client/src/app/store.gd` | 默认外观改**按名字**解析；新增 `apply_original_player_config()` 并在首次运行时套用（§5.3/§5.5） |
| `client/tools/selftest.gd` | **只新增**断言（+25 条）与末尾的 profile 恢复；**没有改/删任何既有断言** |
| `client/tools/dump_textures.gd` / `dump_one.gd` | 新增 dev 工具：把 `assets/**/*.ctex` 解成 PNG（做颜色/模板比对用，§5.4） |
| `docs/137-pixel-fidelity.md` | 本文 |

**没碰**（按任务约定）：`play_screen.gd`、`replay_lobby.gd`、`lan_lobby.gd`、`join_ip_pop_up.gd`、
`connect_popups.gd`、`connecting_pop_up.gd`、`disconnected_pop_up.gd`、`app.gd`、`plane_view.gd`、
`src/`（Rust）、`docs/INDEX.md`、`docs/HANDOVER.md`。

### 关于共享积木的改动（需要说明的两处）

1. `ui.gd::exit_pair()` —— **所有 15 个界面**都用它。改动只影响返回键的绘制层数，
   几何（146×96）不变；实测让**每一屏**的残差都少了 647 px（§3.2）。
2. `project.godot` 的两个渲染键 —— **全局**影响纹理采样。主菜单改动前后都是 **0.000%**，
   其余屏全部下降或持平（`§0` 表），没有一屏变差；两键的取值直接抄自
   `client/assets/original_project_settings.json`（正版 `project.binary` 的导出）。
