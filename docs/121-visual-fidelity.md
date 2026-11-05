# 121 · 客户端视觉保真：启动画面 / 深绿背景 / 图标与字体

状态：**现行**。本文只回答一件事：用户实际跑起来看到的**三处视觉不一致**
（① 开头有 Godot 启动画面 ② 背景是灰色不是深绿 ③ 大量图标缺失）**为什么会出现、
改了什么、还剩什么**。

> 前置：`108-client-rebuild.md`（客户端总台账）、`116-kernel-api.md`（内核接缝）。
> 真值来源：恢复出的原版工程 `.agent-work/tools/recovered/`（149 `.svg` + 31 `.tscn` +
> 105 `.gd` + 22 `.tres`）**以及正版 `Graphwar.pck` / `libgraphwarrust.so` 本身**。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 能与原版字节/逐帧对上；**观测** = 能指到文件偏移、字节或帧号；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

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

## 1. 怎么验的（方法先说清）

三处问题都只能靠"真窗口 + 逐帧"和"与原版字节比对"来定，`--headless` 一律看不见：

```bash
# 逐帧抓启动画面：Xvfb + ffmpeg x11grab@60fps → ppm 序列 → 每帧主导色
bash client/tools/startup_capture.sh 8            # 产物落 .agent-work/client/startup/

# 逐界面截图（我们自己的界面，不含任何游戏资产）
bash client/run.sh shots /some/out/dir

# 资产提取 + 就绪检查
python3 tools/extract_assets.py          # pck → client/assets/
python3 tools/extract_assets.py --check  # 只查产出目录（不需要 pck）
```

抓帧脚本已随代码入库（`client/tools/startup_capture.sh`，纯 shell + stdlib python，
不含任何游戏资产）；它的产物（mp4/ppm）落 `.agent-work/`，不入库。

---

## 2. 问题①：开头那块"Godot 启动画面"

### 2.1 先复现，看清到底是什么

`project.godot` 里本来就有 `boot_splash/show_image=false`，但用户仍然看到东西。于是做了
**四组真窗口抓帧**（1280×720，60 fps，同一台机器、同一份工程）：

| 组 | 配置 | 首帧序列（主导色 × 帧数，60 fps） | 结论 |
|---|---|---|---|
| V0 | 原样（`show_image=false`，没写 `bg_color`） | 黑 ×88 → **`#242424`（100% 像素）×95** → `#16171b` | 有一块**纯灰矩形**，约 1.58 s |
| V1 | `show_image=true`（**阳性对照**） | 黑 ×74（末 17 帧已有 0.1% 非黑像素）→ **`#242424` 但只占 96.1%**（≈4% 像素被 logo 覆盖）×62 → `#16171b` | 抓帧看得见 logo ⇒ V0 里 logo **确实被关掉了** |
| V2 | `show_image=false` + `bg_color=Color(1,0,1,1)` | 黑 ×71 → **`#FF00FE`（100% 像素）×59** → `#16171b` | **那块矩形就是 `boot_splash/bg_color`** |
| V3 | 修复后（见 §2.3），用 `client/tools/startup_capture.sh` 复跑 | 黑 ×73 → **`#0f251b`（100%）×102** → **`#0f251b`（一直被主界面占了 ~64%）** | 只有**一段**底色，没有任何跳变 |

### 2.2 根因（不是"再加一个键试试"）

1. `boot_splash/show_image` 控制的只是**那张 logo 图**。设成 `false` 后，引擎**仍然**
   在"窗口已创建、首帧还没画出来"这段时间里铺一整块 `boot_splash/bg_color` 的纯色。
   V2 把该键改成洋红，那块矩形就变成洋红 —— **逐帧观测**，不是读文档猜的。
2. 我们从来没写过 `boot_splash/bg_color` → 用 Godot 默认 `Color(0.14,0.14,0.14)` =
   **`#242424`**（V0 实测值正是 `#242424`）。
3. 于是用户看到的是：**约 1.5 s 的纯中性灰**，紧接着是我们自己的中性灰蓝 `#16171b`。
   两段灰不一样、又都没有内容，人眼读成"Godot 的启动画面"。**它其实从来没有出现过 logo。**
4. 这段时间在本机被软件渲染（llvmpipe）放大了（1.5–2 s）；真显卡上更短，但"灰闪一下"仍在。

> 顺带排除的三种猜测：
> * **项目管理器/编辑器**：`run.sh gui` 走的是 `godot --path <client>`，没有 `--editor`，
>   抓帧里也没有任何管理器的界面元素（全程只有一个主导色 + 主菜单）。
> * **`run.sh` 的 `--import` 弹窗**：那一步是 `--headless`，不会建窗；而且 `.godot/` 存在时不跑。
> * **`boot_splash/fullsize`**：不需要 —— 见 §2.3，改了 `bg_color` 之后矩形本来就是全屏的。

### 2.3 修法

`client/project.godot`（两个值都是原版值，来源见 §3.2）：

```ini
[application]
boot_splash/show_image=false
boot_splash/bg_color=Color(0.054902, 0.145098, 0.109804, 1)

[rendering]
environment/defaults/default_clear_color=Color(0.054902, 0.145098, 0.109804, 1)
```

两个颜色**写成同一个**，启动矩形与首帧背景就无缝衔接：V3 抓帧里第 74 帧之后**主导色一直
是 `#0f251b`**，从来没有出现过第二个底色（对比 V0 的 `#242424` → `#16171b`），
即"启动即可见主界面、没有闪烁"。

> 实测值 `#0f251b` 与标称 `#0E251C` 差 1 个色阶，是抓帧链路的取整（V2 的洋红也读成
> `#FF00FE`），不是配置写错 —— 见 §3.4 的断言，那里是**逐位**比对的。

---

## 3. 问题②：深绿背景

### 3.1 深绿到底是 clear_color 还是某个控件画的？

**是 clear_color。** 核实方法：把恢复出的 31 个 `.tscn` 全查一遍背景类节点。

* `main_menu.tscn`（主界面）里**没有任何** `ColorRect` / `Panel` / `TextureRect` 铺底，
  只有 `MarginContainer/VBoxContainer/Label/GraphwarButton`；
* 全部 31 个 `.tscn` 里的 `ColorRect` 只有两类：`ClipButton/AnimationRect`（按钮动画层，
  `color = #1A3B2D`）与 `player_screen.tscn` 的两块 `Color(0,0,0,0.9)` 遮罩（未解锁格）；
* 全工程 `bg_color` 出现在 9 处 StyleBox（`chat_pop_up` / `player_screen` /
  `shop_button_entry`），都是 `#1A3B2D` —— **更浅的一层绿，是按钮/面板，不是背景**。

⇒ 主界面那层深绿 = `rendering/environment/defaults/default_clear_color`，
**没有控件参与**。所以只要这个键对，背景就对。

### 3.2 原版值不是"取色猜的"，是解出来的

`Graphwar.pck` 里有 `project.binary`（1346 B，`ECFG`）。`tools/extract_assets.py`
按下面这个**逐位解出来的布局**把它读成 JSON：

```
u32 magic 'ECFG' | u32 count |
count × ( u32 key_len | key | u32 value_len | value_len 字节的 Variant )
Variant = u32 type + 载荷（1=BOOL 2=INT 3=FLOAT 4/21=STRING 5=VECTOR2 20=COLOR 28=ARRAY）
```

21 条记录，解完**恰好到 EOF**（1346/1346），其中：

```json
"application/boot_splash/bg_color":                 [0.054902, 0.145098, 0.109804, 1.0],
"application/boot_splash/show_image":               false,
"rendering/environment/defaults/default_clear_color": [0.054902, 0.145098, 0.109804, 1.0]
```

* **两个颜色是同一个值**（逐位相同），这解释了原版为什么"看不到启动画面"；
* `0.054902/0.145098/0.109804 × 255 = 14/37/28 = **#0E251C**`，与用户从原版截图取到的
  `#0e251c` 逐通道吻合；
* 这份 JSON 落在 `client/assets/original_project_settings.json`，**selftest 有一条断言
  逐位比对**我们 `project.godot` 里的值与原版值（§6.1），所以"背景色照原版"是**可测的**，
  不是写死在代码里的一句口号。

### 3.3 顺手把"灰色"的另外两位也改成原版

用户说的"背景是灰色"，除了 clear_color，还有一层**观感**来源：我们的按钮/面板是自选配色
`#22272E`（中性灰蓝）、圆角 6、1 px 浅边框。原版主题（恢复出的 `.tres`）是：

| 项 | 原版值 | 出处 |
|---|---|---|
| `Button/styles/normal` | `bg #1A3B2D` + 3 px 黑边 + 圆角 10 | `graphwar_theme.tres` → `StyleBoxFlat_hm1tw` |
| `Button/styles/hover` | 同上（`graphwar_style_box_flat_hover.tres`） | 同上 `ExtResource("2")` |
| `Button/styles/pressed` | `bg #0F281D` + 3 px 黑边 + 圆角 10 | `StyleBoxFlat_r5led` |
| `Button/styles/disabled` | `bg #383838` + 3 px 黑边 + 圆角 10 | `StyleBoxFlat_b54nd` |
| `Button/colors/font_color` | 纯白 | `graphwar_theme.tres` |
| `Panel/styles/panel` | `graphwar_style_box_flat.tres` = `#1A3B2D` + 3 px 黑边 + 圆角 10 | 同上 `ExtResource("3")` |
| `default_font_size` | 24 @ **1600×900** | `graphwar_theme.tres` + `project.binary` 的 viewport |

`client/src/ui/theme_factory.gd` 全部按上表重写（`COL_PANEL/COL_PANEL_EDGE/COL_PRESSED/
COL_DISABLED/PANEL_RADIUS/PANEL_BORDER`）。**基准字号取 20 而不是 24**：原版的 24 是在
1600×900 视口里，本工程 1280×720，等物理大小 = 24 × 720/900 = 19.2 ≈ 20；
直接用 24 会让按钮/文字整体偏大、把战场挤到几乎看不见（§7.2 实测）。

### 3.4 证据等级

* 原版 `project.binary` 的两个颜色值、`show_image=false` —— **逐位验证**（解出来的字节
  `eb e0 60 3d 92 94 14 3e eb e0 e0 3d 00 00 80 3f`）；
* "深绿由 clear_color 画、没有控件参与" —— **观测**（31 个 `.tscn` 全查 + 逐界面截图里
  clear_color 之外没有任何铺底节点）；
* "原版观感里按钮也是这个绿" —— **观测**（`.tres` 里的 9 处 `bg_color`），但**按钮的
  最终像素**没有和原版逐像素比对过 → 属**推断**（缺：原版真机/真截图的逐像素对照）。

---

## 4. 问题③：图标（原版 149 个 svg）

### 4.1 先说一个会让方案作废的事实：`Graphwar.pck` 里**没有 svg，也没有 ttf**

对正版 `Graphwar.pck` 做全量清单（692 个文件），扩展名分布是：

```
.import 184 | .ctex 167 | .remap 158 | .gdc 105 | .scn 31 | .res 22
.fontdata 7 | 音频 11 | .png 1 | .binary 1 | .gdextension 1
```

**`.svg` = 0，`.ttf` = 0。** 原版导出时把源资源吃进了导入流程，包里只剩导入产物：

| 原版源 | 包里能拿到的 | 数量 |
|---|---|---|
| 149 个 `.svg` | `.ctex`（`CompressedTexture2D`，512×512 光栅化） | 167（含 App 图标等，Graphwar 美术 142） |
| 7 个 `.ttf` | `.fontdata`（`FontFile`，MSDF） | 7（其中 **2 个是 AdMob 插件的**，游戏自己用 5 个） |
| 22 个 `.tres` | `.res`（二进制 `Theme`/`StyleBox`） | 22 |

⇒ "解出 149 个 svg 和 7 个 ttf"**做不到**。能做的是解出**同一个画面所依据的资产**
（`.ctex` / `.fontdata`）—— 而且这比 svg/ttf 更贴近原版：**游戏自己渲染的就是它们**
（`.ctex` 送进 GPU、`.fontdata` 就是 MSDF 字形），不存在"我们重新光栅化一版"的偏差。

> `.svg.import` 里写着 `source_file="res://hats/no_hat.svg"` 与
> `dest_files=["res://.godot/imported/no_hat.svg-<md5>.ctex"]`，所以
> **"哪个 ctex 是哪个 svg 变的"是包自己给的映射**，不用猜。
> 注意目录信息只在这里：`res://skins/bot_0.svg` 与 `res://faces/bot_0.svg` 的产物
> **都叫** `bot_0.svg-<md5>.ctex`，只有 md5 不同。

### 4.2 提取管线（新增 `tools/extract_assets.py`）

```bash
python3 tools/extract_assets.py                 # 默认 <仓库根>/Graphwar.pck → client/assets/
python3 tools/extract_assets.py --pck /path/to/Graphwar.pck
python3 tools/extract_assets.py --list          # 只打印会解出什么
python3 tools/extract_assets.py --check         # 只检查产出目录（不需要 pck，CI/run.sh 用）
```

* **落盘命名 = 原版 `res://` 路径 + 实际容器扩展名**：
  `res://hats/no_hat.svg` → `client/assets/hats/no_hat.ctex`；
  `res://ComicRelief-Bold.ttf` → `client/assets/ComicRelief-Bold.fontdata`；
  `res://button_font.tres` → `client/assets/button_font.res`。
  好处：客户端按**原版名字**查找，同一个查找路径也能命中真 `.svg`/`.ttf`
  （手上有完整工程的人把同名文件放进去即可，**代码不用改**）。
* 解出 **148 个文件（4.6 MB）**：142 个 svg 的 ctex + `basic_soldier` + 5 个游戏字体
  `.fontdata` + `button_font.res` + `MANIFEST.tsv`（源路径/落盘路径/字节数/sha256）
  + `original_project_settings.json`。
* **不解**：AdMob 插件的 7 个 svg / 2 个 ttf（不是 Graphwar 美术，客户端也没有广告位），
  以及 21 个 `.res` 主题 —— 它们在包里靠 `res://xxx.tres` 的路径互相引用，
  单独解出来加载会失败（实测：`Failed loading resource: res://graphwar_theme.res`）。
  主题的**数值**我们照抄进 `theme_factory.gd`（§3.3），不搬文件。

### 4.3 图标名 ↔ 控件 ↔ 原版 svg：映射表怎么来的

`client/src/ui/icons.gd` 现在只是一张表 + 一个加载函数（**一行画图的代码都没有**）。
每条映射的证据分三类，表里逐条注了出处：

1. **原版 `.tscn` 的 `icon=` / `texture=` 引用**（31 个场景，**99 处节点→svg**，
   35 个不同的 svg）。例：
   * `play_screen.tscn`：`GraphwarButton3.icon → share_summary.svg`、
     `ShowNames.icon → view_names.svg`、`MuteButton.icon → unmute.svg`、
     `ViewFunctionButton.icon → view_recent_functions.svg`、
     `GraphwarButton16/17/18 → delete_right / arrow_right / arrow_right_bar`…
   * `team1_entry.tscn`：`RemovePlayerButton → x.svg`、`AddSoldierButton → plus.svg`、
     `RemoveSoldierButton → minus.svg`、`EmptyIcon → empty_soldier.svg`；
   * `add_bot_pop_up.tscn`：6 个 `AddBotButton` 的 icon 就是 `skins/bot_0..5.svg`；
   * 31 个界面的 `ClipButton`/`LongPressButton` 都是 `return.svg`。
2. **原版 `.gd` 里的动态 `load()`**：`ShowExtraButtons.gd` 在展开/收起时切
   `show_extra_buttons.svg` / `hide_extra_buttons.svg`。
3. **`libgraphwarrust.so` 的 `.rodata` 字面量池**（文件偏移 **0x1c2f97** 起一段连续
   字面量）。界面逻辑在 Rust 里，"第 N 档用哪张图"只能从这儿读：
   ```
   res://axis_option_0..3.svg          res://turn_30s/1m/2m/3m/5m/inf.svg
   res://turns_sequential|simultaneous.svg
   res://hats/<28 个>.svg              res://faces/<17 个>.svg
   res://ranks/rank_0..21.svg          res://skins/<23 个>.svg
   res://no_chat.svg res://view_chat.svg res://no_chat_new_chat.svg
   res://mute.svg res://view_names.svg res://hide_names.svg
   res://view_recent_functions.svg res://view_all_functions.svg
   res://play_option.svg res://pause_option.svg res://lock.svg res://empty_set.svg
   ```
   **组内顺序 = 索引顺序**这一点：`ranks/rank_0 … rank_21` 严格递增可以**反证**
   字面量池是按索引顺序排的；其余三组沿用同一约定 → 这三组的下标绑定属**推断**（见 §8）。

固定映射共 **43 条**，加 `axis_option_0..3`(4) / 皮肤表(17) / Bot 皮肤(6) /
脸(16) / 帽(28) / `ranks/rank_0` / `basic_soldier`，客户端一共引用 **116 个唯一路径**。

### 4.4 覆盖率（这是用户要的那张表）

对"原版 149 个 svg"分类：

| 类别 | 数量 | 说明 |
|---|---|---|
| **能加载真资产**（客户端引用且 pck 里能解出） | **116** | 全部命中，`selftest` 有断言逐个校验（§6.1） |
| **pck 里能解出、但本客户端没引用** | 26 | 见下 |
| **真缺失（pck 里根本没有）** | **0** | 142 个 Graphwar 美术 100% 可解 |
| **不是 Graphwar 美术**（AdMob 插件） | 7 | `addons/admob/assets/…`，故意不解 |

26 个"能用但没用"的具体是：

| 路径 | 为什么没用 |
|---|---|
| `ranks/rank_1 … rank_21`（21 个） | 段位系统未实现（`docs/108` §10：段位数据未知，玩家行 `RankIcon` 还是空占位）。`rank_0` 已引用 |
| `skins/cyan`、`skins/reference`、`faces/reference` | 设计参照图/未被二进制引用的图（`.so` 里没有它们） |
| `faces/bot_0` | Bot 专用脸；Bot 走 `faces/no_face`（`docs/92` §4.10 观测） |
| `view_single_function` | 原版有这张图，但 `.so`/`.tscn` 都没有引用点（**未知**：可能被 Rust 里某条未走到的分支用） |

> 与"原版 149"的差：149 − 7(AdMob) = 142；142 − 116(已用) = 26（上表），**闭合**。

### 4.5 缺资产时的行为（明确报错，不静默退化）

* `bash client/run.sh`（GUI）：打印可执行的补救命令并 **`exit 4`**，不启动；
* `run.sh selftest` / `dump`：**允许跑**，但打印"图标/字体断言会被显式跳过"；
* `GwAssets.texture()/font()` 找不到 → `push_error` + 记入 `GwAssets.missing()`，**返回 null**，
  **没有任何占位图路径**（原来的现画代码已整份删除）；
* `run.sh assets` 是提取入口；`--check` 只看产出目录，不需要 pck。

### 4.6 字体

原版 5 个游戏字体全部可解（`.fontdata`）：

| 用途 | 原版源 | 解出 | 依据 |
|---|---|---|---|
| 主题主字体 | `res://button_font.tres` | `client/assets/button_font.res` | `graphwar_theme.tres` 的 `Button/fonts/font`（5 个主题都用它） |
| 标题字体 | `res://EraserRegular.ttf` | `EraserRegular.fontdata` | `title_theme.tres` 的 `default_font` |
| 其余 3 个（ComicRelief / ComicRelief-Bold / CrayonLibre / EraserDust） | 同名 ttf | 同名 `.fontdata` | 已解出备用（`campaign_button_theme` 等用 EraserRegular） |

**系统字体候选链已删除**（`docs/108` §6.1 的 `GW2_FONT_PATH` + Noto/DejaVu 回退不再存在）。
现在只有一条路：原版字体，找不到就报错。

> 更正 `docs/108` §6.1 的一个数字：那里写"原版用 7 个 `.ttf`"。包里 7 个 `.fontdata`
> 里有 **2 个属于 AdMob 插件**（`arial`、`orange_juice_2_0`），**Graphwar 自己用 5 个**。

---

## 5. 资产策略：仓库不带媒体，使用者自带二进制

### 5.1 为什么

* **版权**：图标/字体/场景是游戏资产，不能随源码分发（`AGENTS.md` 第 3 条、
  `docs/108` §11）。
* **但仍然要"能复现原版"**：所以不是"不提供"，而是"不**随仓库**提供"——
  给一条**一条命令**的提取管线，产物目录 gitignore。
* **最要紧的一条**：宁可**报错**，也不要"看起来能跑但图标是假的"。上一版 `icons.gd`
  正是后者：界面能开、按钮位置对，图全是几何占位 —— 用户第一反应就是"图标缺失"。

### 5.2 三步复现（有 pck 的机器）

```bash
# 1) 把正版 Graphwar.pck 放到仓库根（与 libgraphwarrust.so 同级）
python3 tools/extract_assets.py          # → client/assets/（4.6 MB，gitignore）
#    也可以：bash client/run.sh assets

# 2) 构建内核（可选，弹道走内核时才需要）
export CARGO_HOME=$PWD/../.cargo-home
cargo build --release && cp target/release/libgw2_rebuild.so client/

# 3) 跑
bash client/run.sh            # 需要显示器
bash client/run.sh selftest   # 无显示器
```

### 5.3 没有 pck 的机器

`bash client/run.sh` → 一行明确的错误 + 上面那条命令，`exit 4`。
`bash client/run.sh selftest` → **照常跑**，但汇总行会变成
`SELFTEST 90 passed, 0 failed, 9 skipped`，并逐条打印 `@@SELFTEST_SKIP …`。

### 5.4 仓库里还剩什么"自己画的"

* 15 个界面的 `.tscn`（只有根节点 + 脚本，`docs/108` §9.1 的设计取舍）；
* 全部 `.gd`（界面结构、命令总线、内核接缝）；
* 配色常量里**只有 3 个**是原版没有的：`COL_TEXT_DIM` / `COL_ACCENT` / `COL_WARN`
  （状态行与提示行用），其余全部照原版 `.tres` 抄。
* **`client/icon.svg` 已删除**（连同 `.import`）。它原来是"我们自己画的占位 App 图标"，
  现在 `project.godot` 不再引用任何图标文件。

---

## 6. 验证

### 6.1 自动验收

```bash
bash client/run.sh selftest     # → SELFTEST 99 passed, 0 failed
bash client/run.sh dump | grep -i "kernel on"   # → … |client 0.1.0 · kernel on
```

* **原有 93 条断言一条没删、判据一条没放宽**（`tools/selftest.gd` 的 `_test_fonts_and_icons`
  仍是那 3 条：字体对象可用 / 图标可加载 / `bot_5` 可加载；只是描述从"现画"改成"原版资产"）。
* **新增 6 条**（99 = 93 + 6）：
  1. 全部 116 个引用路径都能在 `assets/` 解析（缺一个就 FAIL，并列出前 5 个）；
  2. `original_project_settings.json` 可读；
  3. 原版里 `clear_color` 与 `boot_splash/bg_color` 是同一个值；
  4. 我们的 `clear_color` 与原版**逐位一致**（容差 1e-5，比对 4 个通道）；
  5. 我们的 `boot_splash/bg_color` 与原版**逐位一致**；
  6. `boot_splash/show_image` 与原版一致（`false`）。
* 缺资产时这 9 条（3 + 6）**显式跳过并打印**，汇总行带 `9 skipped` —— 不会静默变成"通过"。
* 内核：`.gdextension` 未改动，`kernel on` 仍在（`docs/116` 的接缝没被碰）。

### 6.2 逐界面截图对照

用同一份 `tools/shots.sh`，对**改动前**（主仓库 `client/` 原样拷一份）与**改动后**各出
11 张，逐张目视比对。结论要点：

| 界面 | 改动后观察到 | 判断 |
|---|---|---|
| MainMenu | 深绿底、绿底黑边圆角按钮、**原版 ComicRelief 字体**、标题用 `EraserRegular` | ✅ 与原版观感一致 |
| PlayScreen | 左右键盘/返回/删除/箭头/回放键都是**原版图标**；战场里的士兵是**三层贴图合成**（皮肤+脸+帽），不再是纯色圆 | ✅ 大幅接近 |
| PlayerScreen | 皮肤网格是原版 15 张皮肤图；预览是 yellow+regular_eyes+soldier_helmet（原版默认外观） | ✅ |
| AddBotPopUp | 6 档 Bot 头像是原版 `skins/bot_N` | ✅ |
| GameSetup | 聊天/锁/四个模式/加 Bot 都是原版图标；玩家行士兵图标是原版皮肤 | ✅ |
| Campaign / JoinLevelPopUp / LanLobby / ReplayLobby / 三个连接弹窗 | 深绿底 + 原版字体；关卡按钮是文本（原版 `campaign_button_theme` 的按钮本来就是透明底+文字） | ✅ |

### 6.3 缺资产路径也验过

```bash
mv client/assets client/assets.off
bash client/run.sh            ; echo $?     # → 打印提取命令，exit 4
bash client/run.sh selftest   # → SELFTEST 90 passed, 0 failed, 9 skipped（逐条 SKIP）
mv client/assets.off client/assets
```

---

## 7. 仍然存在的视觉差异（诚实清单）

### 7.1 与原版**没法比**的部分（缺真值）

| # | 差异 | 缺什么证据 |
|---|---|---|
| 1 | 我们没有原版的**实时截图**，只有静态 `.tscn`/`.tres`/`.so`。所以"按钮像素是否与原版一致"只能到数值级（颜色/边框/圆角/字体），**不能**到像素级 | 一份原版真机截图（或 Android 包里的帧） |
| 2 | **21 关的关卡内容**、段位、皮肤解锁条件未解 —— 相关图标（`ranks/rank_1..21`）只是能加载，没处显示 | `docs/103`/`docs/108` §10 已登记 |
| 3 | 皮肤/脸/帽**第 N 格对应哪张图**：`.so` 里同时存在"23 条完整表"与"13 条商店表"，都不是界面上观测到的 15/15/27 格 | 见 §8 的"推断"条目；缺 Rust 里 `shop_button` 的下标绑定 |
| 4 | 结算界面 `YOU WON` 一侧文案、`turn_*` 之外的时间档图标 | 未观测 |

### 7.2 本轮引入或暴露、已修但值得记住的坑

| 现象 | 根因 | 处理 |
|---|---|---|
| 换成原版字体后**整个界面被挤塌**（PlayScreen 战场缩到 130 px 宽） | 原版图标的导入产物是 **512×512**，而 `GwButton` 没设 `expand_icon` → 图标参与了最小尺寸计算 | `GwButton.make()` 统设 `expand_icon = true`（原版 `.tscn` 里带图标的按钮也都这么写） |
| 纯图标按钮（返回键）**画不出图标** | `text = "     "`（5 个空格）把 expand_icon 的可用宽度吃光 | 那两个按钮 `text=""`，尺寸靠 `stack.custom_minimum_size` |
| GameSetup **整屏塌陷**，只看得见一个巨大的玩家预览 | 士兵槽是 `TextureRect`，默认最小尺寸 = 贴图尺寸（512）→ 把屏幕顶成 1280×1280 | 所有图标 `TextureRect` 加 `expand_mode = EXPAND_IGNORE_SIZE` |
| 玩家行士兵图标变成**小点** | 原版 svg 画布留白很大（士兵本体只占中间 ~45%），24 px 的槽里只剩 ~8 px | 槽 24→40 px、图标 22→38 px |
| 基准字号 24 太大 | 原版 24 是在 **1600×900** 视口；本工程 1280×720 | 取 20（= 24×720/900） |
| 玩家预览只剩一颗**光头** | 存档里 `skin/face/hat` 都是 0，而 0 号是 `no_face`/`no_hat`（**全透明**） | 默认值改成原版观测到的 `skin_7`(yellow)/`face_2`(regular_eyes)/`hat_1`(soldier_helmet) |

### 7.3 还没做的

| # | 项 | 说明 |
|---|---|---|
| 1 | 原版 22 个 `.res` **主题**没直接用 | 它们按 `res://xxx.tres` 互相引用，单独解出来加载失败（§4.2 实测）。数值已照抄，但 `StyleBox` 的 `corner_radius_*` 之外还有 `expand_margin` 等细节没逐条核 |
| 2 | `campaign_button_theme` 的**描边**（`outline_size = 5` + 黑描边）没做 | 关卡按钮现在是纯文字，原版有 5 px 描边 |
| 3 | 快照/回放/结算里的**分享图标语义**未验 | `share_*` 三张图已能加载，但原版点下去干什么未观测（`docs/108` §4 记 🔶） |
| 4 | 音频 | ~~包里 1 个音频文件损坏~~ ❌ **已证伪**（`docs/111`）：10 个导入产物四道独立检查全过；"损坏的那份"是使用者自己 `rebuild_ogg.py` 的产物（granule 全 0、缺 EOS 页）。`mute/unmute` 目前仍只切图标 |
| 5 | `view_single_function.svg` | 原版有图、无引用点（**未知**） |

---

## 8. 证据分级汇总

| 结论 | 等级 | 依据 / 缺什么 |
|---|---|---|
| 启动时有 `boot_splash/bg_color` 的纯色矩形，`show_image=false` 不关它 | **逐位验证**（逐帧） | V0/V1/V2/V3 四组抓帧：改 `bg_color` 为洋红 → 矩形变洋红（`#FF00FE`）；`show_image=true` → 同一阶段出现 ~4% 非底色像素（logo） |
| 原版 `bg_color` = `clear_color` = `#0E251C` | **逐位验证** | `project.binary` 解出 4 个 float 字节 `eb e0 60 3d 92 94 14 3e eb e0 e0 3d 00 00 80 3f` |
| 主界面深绿由 clear_color 画，无控件参与 | **观测** | 31 个 `.tscn` 全查：`main_menu.tscn` 无背景节点；全工程 `ColorRect` 只有动画层与遮罩 |
| 修复后启动无跳变 | **逐位验证**（逐帧） | V3：第 60 帧起主导色一直是 `#0f251b`，无切换 |
| 149 svg / 7 ttf 在 pck 里不存在 | **观测** | pck 全量清单：`.svg`=0、`.ttf`=0、`.ctex`=167、`.fontdata`=7 |
| ctex ↔ 原 svg 的对应 | **观测** | `.import` 存根里的 `source_file` / `dest_files`（包自己给的） |
| 43 条固定图标映射 | **观测** | `.tscn` 99 处节点引用 + `ShowExtraButtons.gd` + `.so` `.rodata`（0x1c2f97） |
| 皮肤/脸/帽**下标 → 具体文件** | **推断** | `.rodata` 组内顺序；`ranks/rank_0..21` 单调递增可反证顺序=索引。缺：Rust 里商店格子的下标绑定；界面上观测到的 15/15/27 与 `.rodata` 的 17/16/28 条对不齐，说明有一层"可选项过滤"未解 |
| 按钮/面板的观感与原版一致 | **推断** | 数值一致（颜色/边框/圆角/字体），但没有原版真机截图的逐像素对照 |
| `view_single_function.svg` 的用途 | **未知** | 有图、无引用点 |

---

## 9. 改了哪些文件

| 文件 | 改动 |
|---|---|
| `client/project.godot` | `boot_splash/bg_color` + `default_clear_color` 改成原版 `#0E251C`；删除 `config/icon`（仓库不再有 `icon.svg`） |
| `client/icon.svg`, `client/icon.svg.import` | **删除**（我们自己画的占位 App 图标） |
| `client/.gitignore` | 新增 `assets/` |
| `client/run.sh` | 新增 `assets` 模式；`gui`/`shots` 缺资产时打印提取命令并 `exit 4`；`selftest`/`dump` 允许跑但告警 |
| `tools/extract_assets.py` | **新增**：pck → `client/assets/`（内含 `ECFG` 解析器、`.import` 映射、`--list/--check`） |
| `client/src/ui/assets.gd` | **新增** `GwAssets`：资产目录、多扩展名查找、缓存、缺资产记录、原版设置读取 |
| `client/src/ui/icons.gd` | **整份重写**：从"现画 SVG"改成"名字→原版路径"映射表 + `GwAssets` 加载；删除全部画图代码 |
| `client/src/ui/theme_factory.gd` | 配色改成原版 `.tres` 数值；字体改成原版 `button_font` + `EraserRegular`；删除系统字体候选链 |
| `client/src/ui/gw_button.gd` | `make()` 统设 `expand_icon = true` |
| `client/src/ui/ui.gd` | 纯图标按钮 `text=""`（原来是 5 个空格） |
| `client/src/main.gd` | 新增 `_require_assets()`：GUI 缺资产 `quit(4)`；selftest 允许跑 |
| `client/src/screens/main_menu.gd` | 标题套原版 `EraserRegular` |
| `client/src/screens/game_setup.gd` | 图标 `TextureRect` 加 `EXPAND_IGNORE_SIZE`；士兵槽 24→40 px |
| `client/src/screens/endgame_pop_up.gd` | 同上（3 个 TextureRect） |
| `client/src/app/store.gd` | 默认外观改成原版观测值（yellow / regular_eyes / soldier_helmet） |
| `client/src/play/player_showcase.gd` | 改成三层原版贴图合成，删除自绘士兵 |
| `client/src/play/plane_view.gd` | 士兵改成三层贴图合成（原来是纯色圆） |
| `client/tools/selftest.gd` | 新增 6 条断言 + `_skip()` 机制；汇总行支持 `N skipped` |
| `docs/121-visual-fidelity.md` | 本文 |

**没有改**：`docs/INDEX.md`、`docs/HANDOVER.md`、`src/`（Rust 内核）、`client/gw2rebuild.gdextension`。

资产自检（提交前必跑）：

```bash
# 只看"新增/修改"—— 删除项里会出现我们自己的 client/icon.svg（本轮删掉的占位图）
git diff --cached --name-status --diff-filter=ACM \
  | grep -E '\.(svg|ttf|ots|png|tscn|tres|pck|scn|gdc)$'     # 必须为空
git diff --cached --name-status --diff-filter=D               # 只应有 client/icon.svg(.import)

# 二道闸：仓库里不许有**任何媒体/二进制资源**被跟踪
git ls-files | grep -E '\.(svg|ttf|ots|png|tres|pck|scn|gdc)$'      # 必须为空
```

两条已知的、**不是**游戏资产的例外（都是本项目自己的源码，历史上就在库里）：

| 路径 | 是什么 |
|---|---|
| `client/scenes/**/*.tscn`（16 个，每个 10–14 行） | 我们自己写的"根节点 + 脚本"存根（`docs/108` §9.1 的设计取舍），不是游戏场景 |
| `client/**/*.gd`、`tools/*.gd` | 本项目源码（界面/命令总线/分析探针），不是反编译出来的 105 个 `.gdc` |

> **`client/assets/` 必须查不到**：整目录在 `client/.gitignore` 里
> （`git check-ignore -v client/assets` → `client/.gitignore:10:assets/`）。

---

## 10. 复现清单

```bash
cd gw2-rebuild

# 1) 资产（需要正版 Graphwar.pck 在仓库根）
python3 tools/extract_assets.py            # 148 files, 4.6 MB → client/assets/
python3 tools/extract_assets.py --check    # "资产齐备：148 个文件…"

# 2) 内核（可选）
export CARGO_HOME=$PWD/../.cargo-home
cargo build --release && cp target/release/libgw2_rebuild.so client/

# 3) 验收
bash client/run.sh selftest                # SELFTEST 99 passed, 0 failed
bash client/run.sh dump | grep -i "kernel on"
bash client/run.sh shots /tmp/shots        # 11 张 PNG，肉眼核对

# 4) 启动画面（要 Xvfb + ffmpeg）
bash client/tools/startup_capture.sh 8       # 主导色应一路 #0f251b，不出现 #242424
```
