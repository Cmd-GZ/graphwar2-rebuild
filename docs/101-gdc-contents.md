# 101 · `.gdc` 字节码内容：5 个行为脚本 + 布局助手数值表

状态：**现行**。本文补上 [`docs/92`](92-ui-inventory.md) §2 明确写着"拿不到"的那一层：
`.gdc` 的**常量池、标识符表、函数表**已经全部解出，105/105 个脚本零失败。

补的前提不是装了反编译器（机器上仍然没有，也**没有联网**），而是：

1. `.gdc` 的 payload 是**带类型的 token 流**，不是黑盒（§2 给出逐字节格式）；
2. 本机 `tools/Godot_v4.7.2-stable_linux.x86_64` 能**把 `.gd` 编译成 `.gdc`**（`--export-pack`），
   产物的字节码版本是 **101**，与游戏里 105 个脚本完全一致。于是可以用"已知源码 → 已知字节码"
   的探针，**实测**每一个 token 类型的编号（§3），而不是猜。

---

## 0. 一页结论

| 项 | 结果 | 证据等级 |
|---|---|---|
| `.gdc` payload 解析 | **105/105 成功**，每个文件都精确消耗到最后一个字节（0 残留） | 观测（`tools/gdc_disasm.py --all`） |
| 语料规模 | 105 脚本 · 2200 个标识符 · **546 个常量** · 14075 个 token | 观测 |
| 字节码版本 | 全部 `101`（Godot 4.7.2） | 观测 |
| token 类型表 | 语料里出现的 **59 种**全部有名字；编号由"编译探针"逐位对照得到 | 逐位对照（探针编译） |
| 5 个行为脚本 | 常量池 / 函数表（名字+参数个数）/ 成员 / **伪源码** 全部拿到（§4） | 观测 |
| 布局助手数值表 | 非插件脚本 47 个全部列出数值与表达式（§5） | 观测 |
| 是否"反编译" | 不是完整反编译：没有 AST、没有类型推断、注释/空行丢失；**但语句级伪源码可读** | 见 §7 |
| 拿不到的 | 运行时行为、多行语句的精确换行、NodePath 的特殊形态；见 §7 | — |

三条最有用的事实：

* **字号全部是"屏幕宽 ÷ N"**，不是固定像素：28 处 `font_size` 赋值，N ∈ {8,10,15,20,30,40,50,60}，
  其中 N=30 出现 13 次（§5.2）。重建时只要拿到 N 和"用哪个宽度"（`get_viewport_rect().size`
  还是 `DisplayServer.get_display_safe_area().size`）就能逐像素复现。
* **边距只有三种写法**：固定像素（`ShowcaseMarginContainer` = 5、`IpPopUpMargin` = 10）；
  "安全区推出来"（`MarginContainer` / `MarginContainerPopUpBack`，无数字常量）；
  以及按屏幕宽的**负**比例（`SoldierIconMarginContainer` = −w/100、`HBoxVictoryIcon` 的
  `separation` = −w/10）。
* 5 个行为脚本的行为**全部是 UI 装配**，没有一条游戏规则——这与 `docs/88` §12.0 的判断一致，
  但现在有了逐条 token 证据（§4）。

---

## 1. 方法

### 1.1 路线

```
.gdc ──GDSC+zstd──► payload ──按 §2 切段──► 标识符表 / 常量表 / 行表 / 列表 / token 流
                                                   │
                        ┌──────────────────────────┴─────────────────────────┐
                        ▼                                                    ▼
              常量池（§4①） + 函数表（func 后跟标识符）              伪源码（按 token 渲染，逐行）
```

### 1.2 关键手段：用同版本 Godot 做"已知答案"探针

这是本文与 `docs/92` §2.2 的分水岭。`docs/92` 当时只能扫字符串池，因为不知道 token 编号含义。
本轮的做法：

```bash
# 一个只有 project.godot + probe.gd + export_presets.cfg 的空工程
tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path <probe项目> \
    --export-pack "Linux" out.pck      # 不需要导出模板；会把 .gd 编成 .gdc
python3 tools/pck_extract.py --pck out.pck --out pck
# pck/probe.gdc 的容器头 = GDSC + 版本 101 —— 与游戏脚本同版本
```

探针源码里**每个关键字/运算符各占一行**，于是"源码第几行的第几个词"和"token 流的第几个 token"
一一对应，token 类型编号就被**直接读出来**，不需要推测。附录 A 是实际使用的探针。

> 为什么这不违反"不联网下载反编译器"：Godot 引擎本来就在仓库里（`tools/`），
> 只做本地编译，没有任何下载；产出的 `.gdc` 是自建源码的字节码，可随时重生成，不入库。

### 1.3 证据等级

| 结论 | 等级 | 依据 |
|---|---|---|
| 容器/payload 分段（§2） | **观测** | 105 个文件全部"精确耗尽"；错一个字节就会残留或越界 |
| token 编号 → 含义（§3） | **逐位对照** | 探针编译 + 源码逐词对齐（附录 A） |
| 5 个脚本的常量/函数/伪源码（§4） | **观测** | 直接来自字节码；伪源码是 token 直译，未经优化 |
| 布局数值（§5） | **观测** | 常量表原文 + 伪源码表达式 |
| 这些数值在运行时"好不好看" | **推断（未验证）** | 只解出"代码写了什么"，没跑起来量像素 |
| NodePath 常量编码 | **推断** | 4 个探针样本；游戏语料里 0 个 NodePath 常量 |
| 行表/列表的语义（§2.4） | **观测** | 用探针源码的行号/缩进逐条核对 |

---

## 2. payload 格式（实测）

### 2.1 容器

```
偏移 0   "GDSC"
偏移 4   u32  字节码版本（本包 = 101）
偏移 8   u32  解压后长度
偏移 12  zstd 帧 → payload
```

与 `tools/gdc_strings.py` 完全一致（该文件本轮**未改动**）。

### 2.2 payload 分段

全部小端。`payload[0:16]` 是四个计数：

```
u32 identifier_count
u32 constant_count
u32 line_count            # "源码行"条目数，等于 token 流里出现的不同行号个数
u32 token_count

identifiers[identifier_count]   # 见 2.3
constants[constant_count]       # 见 2.3
lines[line_count]               # u32 first_token_index, u32 source_line      （2.4）
columns[line_count]             # u32 first_token_index, u32 column + 1        （2.4）
tokens[token_count]             # u32 (type | operand << 8), u32 source_line   （3）
```

**逐字节实例**（`rsc/ExitButtonContainer.gdc`，payload 1496 字节）：

| 偏移 | 长度 | 内容 |
|---|---|---|
| 0 | 16 | `16, 2, 10, 66`（16 标识符 / 2 常量 / 10 行 / 66 token） |
| 16 | 764 | 16 个标识符：`MarginContainer, _ready, get_tree, root, size_changed, connect, _on_viewport_size_changed, update_sizes, screen_size, get_viewport_rect, size, font_size, x, node, get_children, add_theme_font_size_override` |
| 780 | 8 | 常量 0：`type=2 (INT)` + `30` |
| 788 | 20 | 常量 1：`type=4 (STRING)` + `len=9` + `"font_size"` + 3 字节对齐填充 |
| 808 | 80 | 行表 10 条 |
| 888 | 80 | 列表 10 条 |
| 968 | 528 | 66 个 token（每个 8 字节） |
| 1496 | — | **正好文件尾** |

### 2.3 标识符与常量

**标识符**：`u32 长度` + `长度 × u32`，每个 u32 是"码点 ^ `0xb6b6b6b6`"。
ASCII 走完异或后高 3 字节恰好是 0，这就是 `gdc_strings.py` 里"每 4 字节只取第 1 个再 ^0xb6"
也能work 的原因；对非 ASCII（本包里有 U+2009 窄空格）必须按整 u32 异或才对，否则会解错。

**常量**：Godot Variant 编码，`u32 word = type | (flags << 16)`：

| type | 含义 | 载荷 | 备注 |
|---|---|---|---|
| 0 | nil | — | |
| 1 | bool | u32 | |
| 2 | int | flags&1 ? int64 : int32 | 本包 87 个，全部 int32 |
| 3 | float | flags&1 ? double : float32 | **两种都出现**：float32 13 个、float64 14 个 |
| 4 | String | u32 字节数(含结尾 NUL) + UTF-8 + 4 字节对齐 | 本包 381 个 |
| 21 | StringName（`&"x"`） | 同 String | |
| 22 | NodePath（`^"x"`） | 见 §7 | 本包 0 个 |
| 5 / 6 | Vector2 / Vector2i | 2×f32 / 2×i32 | |

> 踩过的坑：一开始把 float 一律按 8 字节读，遇到 `3.0`（float32）就整段错位，
> 后面的"常量"会读成 `type=30/40/60` 这种不存在的类型。**看到不存在的 type 就是错位了**。

### 2.4 行表与列表

两个表各 `line_count` 条、每条 2 个 u32，都是"**某个源码行的起始 token 下标**"：

```
lines[i]   = (first_token_index, source_line)
columns[i] = (first_token_index, column + 1)
```

`column` 是按字符算的缩进（**一个 Tab 记 1**），+1 是为了让"第 0 列"不等于 0（0 表示无）。
用探针（附录 A 的 `probe4.gd`）核对：`func` 行为 1、函数体 2、嵌套体 3，逐条吻合。

实例（`ExitButtonContainer`）：
`lines = (2,4),(7,5),(19,6),(22,9),(27,10),(30,12),(35,13),(43,15),(51,17),(58,19)`，
对照 §4 的伪源码行号完全一致。

> 这两张表**不参与**函数/常量提取，但没有它们就无法把 token 还原成"一行一行"的源码形状。
> `gdc_strings.py` 不需要它；`gdc_disasm.py` 用它输出伪源码。

---

## 3. token 编码（全部实测，非推测）

一个 token 8 字节：

```
u32 value = type | (operand << 8)      # type 取低 8 位
u32 source_line                        # 该 token 所在源码行（1 起）
```

`operand` 只对三类 token 有意义：

| type | 含义 | operand |
|---|---|---|
| 129 | 注解（`@tool` / `@export` / `@onready` / `@warning_ignore` / `@export_range`） | 标识符表下标，**标识符文本就是带 `@` 的整串** |
| 130 | 标识符 | 标识符表下标 |
| 131 | 字面量（数字/字符串/字符串名/bool/null/Vector2…） | **常量表下标** |
| 其余 | 关键字/运算符/标点 | 恒为 0 |

### 3.1 已确认的编号（语料中出现 59 种；探针另外补全了语料没用到的那几种）

| 类别 | 编号 → 文本 |
|---|---|
| 带操作数 | `129 @注解` · `130 标识符` · `131 字面量` |
| 比较 | `132 <` `133 <=` `134 >` `135 >=` `136 ==` `137 !=` |
| 逻辑 | `138 and` `139 or` `140 not` `141 &&` `142 ||` `143 !` |
| 位运算 | `144 &` `145 \|` `146 ~` `147 ^` `148 <<` `149 >>` |
| 算术 | `150 +` `151 -` `152 *` `153 **` `154 /` `155 %` |
| 赋值 | `156 =` `157 +=` `158 -=` `159 *=` `160 **=` `161 /=` `162 %=` `163 <<=` `164 >>=` `165 &=` `166 \|=` `167 ^=` |
| 关键字 | `168 if` `169 elif` `170 else` `171 for` `172 while` `173 break` `174 continue` `175 pass` `176 return` `177 match` `178 when` `179 as` `180 assert` `181 await` `183 class` `184 class_name` `185 const` `186 enum` `187 extends` `188 func` `189 in` `190 is` `192 preload` `193 self` `194 signal` `195 static` `196 super` `198 var` `199 void` |
| 标点 | `201 [` `202 ]` `203 {` `204 }` `205 (` `206 )` `207 ,` `208 ;` `209 .` `212 :` `213 $` `214 ->` `215 _` |
| 内置常量 | `219 PI` `220 TAU` `221 INF` `222 NAN` |

几个容易看错的：

* `208` 是**分号 `;`**（探针 `print(2);` 复现）。全语料只出现 2 次
  （`CampaingLevelsButtons.gdc` L17 的 `add_child(button);`、`android_iap.gdc` L175）。
* `155` 既是取模 `%` 也是"唯一节点"简写 `%Unique`（GDScript 两者同 token）。
* `213` 是 `$`（`$Node` / `$"Node"`）。
* `212` 是冒号；`:=` 被拆成 **`212` + `156`** 两个 token（不是单独的类型）。
* `129` 的标识符文本含 `@`，所以 `@onready` 在标识符表里是 `"@onready"` 这 8 个字符。

### 3.2 未验证的部分

编号表是**字节码版本 101** 的。Godot 4 的 token 编号在大版本内稳定，但换引擎版本必须重跑附录 A
的探针；`gdc_disasm.py` 遇到非 101 版本会在 JSON 里写 warning（不静默）。

---

## 4. 五个"有行为"的脚本（字节码实测）

名单沿用 [`docs/92`](92-ui-inventory.md) §2.3（运行时台账得出），下面每一条都是本轮从字节码
独立复核的，顺带纠正了一处小误记。

### 4.1 `CampaingLevelsButtons.gdc` —— 运行时造 21 个关卡按钮

`extends Control`；常量 10 个；token 243。

| 函数 | 参数 |
|---|---|
| `_ready` | 0 |
| `_on_size_changed` | 0 |
| `place_buttons` | 0 |

常量池：`[0]=15 [1]=21 [2]="level_" [3]="font_size" [4]="place_buttons" [5]=8 [6]=4 [7]=2 [8]=9 [9]=10`

伪源码（token 直译，空行按行表保留）：

```gdscript
extends Control

var buttons = []

func _ready():
    var screen_size = get_parent().get_parent().size
    var font_size = screen_size.x / 15
    for i in 21:
        var button = GraphwarButton.new()
        button.text = str(i)
        button.input_text = "level_" + str(i)
        button.set_name("level_" + str(i))
        buttons.push_back(button)
        button.add_theme_font_size_override("font_size", font_size)
        add_child(button);

    self.resized.connect(_on_size_changed)
    call_deferred("place_buttons")

func _on_size_changed():
    call_deferred("place_buttons")

func place_buttons():
    var screen_size = self.size
    var font_size = screen_size.x / 15
    var pos_x_increment = screen_size.x / 8
    var pos_y_increment = screen_size.y / 4
    var pos = Vector2(pos_x_increment, pos_y_increment / 2)
    for button in buttons:
        button.add_theme_font_size_override("font_size", font_size)
        button.size.x = abs(pos_x_increment)
        button.position = pos - button.size / 2
        pos.x += pos_x_increment
        if pos.x > screen_size.x * 9 / 10 or pos.x < screen_size.x / 10:
            pos_x_increment = - pos_x_increment
        pos.x += pos_x_increment
        pos.y += pos_y_increment
    get_parent().queue_redraw()
```

重建素材：**21 个按钮**、命名 `level_0..level_20`、`text` 是 `str(i)`、x 方向 **8 列**、
y 方向 **4 行**、字号 = 按钮容器宽 /**15**、碰边回弹（`9/10` 与 `1/10` 是边界比例）。
注意 `for i in 21` 从 0 开始，所以 `str(i)` 是 `0..20`，按钮名是 `level_0..level_20`——
"编号从 0 起"**是字节码才看得出来的**；至于 `level_0` 是否对应界面上的第 1 关，属于**推断**
（`docs/92` §4.2 只观测到 21 个按钮，没有观测到命名与关卡序号的映射）。

### 4.2 `ShowExtraButtons.gdc` —— 切换附加按钮行 + 换图标

`extends GraphwarButton`；常量 10 个；token 161。

| 函数 | 参数 |
|---|---|
| `_ready` | 0 |
| `_button_pressed` | 0 |
| `_update_icon` | 0 |
| `_on_size_changed` | 0 |

常量池：
`[0]=false [1]="pressed" [2]="../../../../PlaneContainer" [3]="…/ExtraOptionsVBox/ExtraOptionsHBox"
[4]="…/GamePlane/PlaneContainer/VBoxContainer/ViewFunctionPad" [5]="res://hide_extra_buttons.svg"
[6]="res://show_extra_buttons.svg" [7]="../../../../LeftControls/LeftTopButtonsContainer"
[8]=1.66667 [9]=true`

```gdscript
extends GraphwarButton

var showing_buttons = false

func _ready():
    connect("pressed", _button_pressed)
    var plane_container = get_node("../../../../PlaneContainer")
    plane_container.item_rect_changed.connect(_on_size_changed)

func _button_pressed():
    var extra_options_box = get_node("../../../../PlaneContainer/ExtraOptionsVBox/ExtraOptionsHBox")
    showing_buttons = !showing_buttons
    extra_options_box.set_show_extra_buttons(showing_buttons)
    var view_function_pad = get_node("../../../../PlaneContainer/GamePlane/PlaneContainer/VBoxContainer/ViewFunctionPad")
    view_function_pad.update_sizes()
    _update_icon()

func _update_icon():
    if showing_buttons:
        self.set_button_icon(load("res://hide_extra_buttons.svg"))
    else:
        self.set_button_icon(load("res://show_extra_buttons.svg"))

func _on_size_changed():
    var left_top_button_container = get_node("../../../../LeftControls/LeftTopButtonsContainer")
    var target_size = left_top_button_container.size.y
    var plane_container = get_node("../../../../PlaneContainer")
    var minimum_y = plane_container.size.x / 1.66667
    var extra_y = plane_container.size.y - minimum_y
    if extra_y >= target_size:
        self.disabled = true
    else:
        self.disabled = false
```

重建素材：两张 svg 图标名、按下后切换的布尔、**1.66667 ≈ 5/3**（平面最小高度 = 宽 ÷ 1.66667 ≈ 宽 × 0.6），
以及"空间不够就禁用自己"的阈值判据。

### 4.3 `ChatBox.gdc` —— 点聊天区聚焦输入框

`extends RichTextLabel`；常量 1 个（`"../ChatLineEdit"`）；token 65。
`docs/92` 记为"`_gui_input` → `chat_line_edit.grab_focus()`"，实测**完全吻合**，补充细节：

| 函数 | 参数 |
|---|---|
| `_ready` | 0（`pass`） |
| `_process` | 1（`delta`，`pass`） |
| `_gui_input` | 1（`event`） |

分支是 **`InputEventMouseButton` 与 `InputEventScreenTouch` 各一遍**（触屏与鼠标各自处理）：

```gdscript
extends RichTextLabel

func _ready():
    pass

func _process(delta):
    pass

func _gui_input(event):
    if event is InputEventMouseButton:
        if event.pressed:
            var chat_line_edit = get_node("../ChatLineEdit")
            chat_line_edit.grab_focus()
    elif event is InputEventScreenTouch:
        if event.pressed:
            var chat_line_edit = get_node("../ChatLineEdit")
            chat_line_edit.grab_focus()
```

> 顺带纠正 `docs/92` §2.3 的一句话：它把 `_process` 也算进"空壳"，实际 `_process` 在
> **ChatBox** 里是存在的（`pass`），空的是 `_ready`。

### 4.4 `PlayerTabContainer.gdc` —— 三个页签标题

`extends TabContainer`；常量 8 个；token 77。

| 函数 | 参数 |
|---|---|
| `_ready` | 0 |
| `_on_viewport_size_changed` | 0 |
| `update_sizes` | 0 |

常量池（`repr` 原样，**含 U+2009 窄空格**）：

```
[0]=0  [1]=' \u2009 Skins  \u2009'  [2]=1  [3]='  \u2009Faces  '
[4]=2  [5]='  \u2009Hats\u2009  \u2009'  [6]=30  [7]='font_size'
```

```gdscript
extends TabContainer

func _ready():
    set_tab_title(0, " \u2009 Skins  \u2009")
    set_tab_title(1, "  \u2009Faces  ")
    set_tab_title(2, "  \u2009Hats\u2009  \u2009")
    get_tree().root.size_changed.connect(_on_viewport_size_changed)
    update_sizes()

func _on_viewport_size_changed():
    update_sizes()

func update_sizes():
    var safe_area = DisplayServer.get_display_safe_area()
    var font_size = safe_area.size.x / 30
    add_theme_font_size_override("font_size", font_size)
```

重建素材：三个页签名（**带窄空格，直接抄字符串**）、字号 = **安全区宽 / 30**（注意这里用
`safe_area`，不是 viewport）。

### 4.5 `OptionsContainer.gdc` —— 最小高度

`extends HBoxContainer`；常量 2 个（`"set_minimum_size"`, `0.25`）；token 48。

| 函数 | 参数 |
|---|---|
| `_ready` | 0 |
| `_on_size_changed` | 0 |
| `set_minimum_size` | 0 |

```gdscript
extends HBoxContainer

func _ready():
    self.resized.connect(_on_size_changed)

func _on_size_changed():
    call_deferred("set_minimum_size")

func set_minimum_size():
    var parent_size = get_parent().size
    self.custom_minimum_size.y = 0.25 * parent_size.y
```

重建素材：附加按钮区高度 = **父容器高度的 0.25**。

---

## 5. 布局助手数值表（唯一权威来源 = 常量表）

### 5.1 分类口径

本表的分类是**从字节码证据推的**，不是抄文档：

| 分类 | 判据 | 数量（非插件） |
|---|---|---|
| `theme-helper` | 出现 `add_theme_*_override` / `remove_theme_*_override` | 30 |
| `geometry-helper` | 无 theme 调用，但设置 `custom_minimum_size` / `minimum_size` | 6 |
| `shell` | 无常量、token ≤ 20（只有 `_ready`/`_process` 的 `pass`） | 4 |
| `other` | 广告/IAP/文件等非界面脚本（根目录 6 个） | 6 |
| `behavioural` | §4 的 5 个 | 5 |
| `empty` | 0 token（`ExtraOptionsVbox.gdc`） | 1 |

⇒ **真正"设置字号/边距/尺寸"的脚本 = 30 + 6 = 36 个**；其中 2 个（`CampaingLevelsButtons`、
`PlayerTabContainer`）同时属于 §4 的行为脚本。加上它们，带字号/尺寸常量的脚本共 **38 个**。
（`docs/88` §12.0 写的"40 个"与 `docs/92` §2.3 写的"34 个"都不是按字节码统计的，见 §6。）

### 5.2 字号：全部是"宽度 ÷ N"

28 处 `var font_size = <宽度> / N`，N 的分布：

| N | 1920 宽时的像素（示例，未运行验证） | 出现次数 | 脚本 |
|---|---|---|---|
| 8 | 240 | 1 | `TitleSize` |
| 10 | 192 | 1 | `ViewFunctionsLabel` |
| 15 | 128 | 3 | `CampaingButtonSize`、`CampaingLevelsButtons`(×2) |
| 20 | 96 | 1 | `TurnLimitText` |
| 30 | 64 | 13 | `PlayerTabContainer`、`ExitButtonContainer`、`LanLobbyTitle`、`LevelTitleLabel`、`MainMenuButtonResize`、`RoomName`、`TimerContainer`、`UnlockButtonSize`、`ButtonGrid`、`HBoxContainer`、`ExtraOptionsHBox`、`FireContainer`、`LobbyButtonContainer` |
| 40 | 48 | 5 | `GameNameContainer`、`GameSetupPlayerEntry`、`IpPopUpMargin`、`LanLobbyFontResize`、`LevelDescriptionScrollContainer` |
| 50 | 38.4 | 1 | `ChatContainer` |
| 60 | 32 | 3 | `FxContainer`、`OffsetContainer`、`VersionLabel` |

宽度来源两种：`get_viewport_rect().size`（27 处）与
`DisplayServer.get_display_safe_area().size`（`PlayerTabContainer`）。**重建时必须区分**，
否则刘海屏/多窗口下会偏移。

### 5.3 全表（47 行，由 `gdc_disasm.py --md` 生成）

> 列义：**数值常量** = 该脚本常量表里的数字（原文照抄，顺序即常量表顺序）；
> **关键表达式** = 伪源码里涉及尺寸/主题的行（`screen_size` = `get_viewport_rect().size`）。

| 脚本 | 分类 | 数值常量（常量表原文） | 关键表达式（伪源码原文） |
|---|---|---|---|
| `AdContainer.gdc` | geometry-helper | — | 无数字常量：`custom_minimum_size.x = ad_holder.ad_size.x`（从广告服务读尺寸） |
| `Ad_holder.gdc` | other | 0 | `var ad_size = Vector2i(0, 0)`（广告位占位） |
| `Android_mediastore_holder.gdc` | other | — | — |
| `ButtonGrid.gdc` | theme-helper | 0.15; 3.0; 30 | `var desired_size = Vector2(screen_size.x * 0.15 / 3.0, screen_size.x * 0.15 / 3.0)` \| `var font_size = screen_size.x / 30` |
| `CampaingButtonSize.gdc` | theme-helper | 15 | `var font_size = screen_size.x / 15` |
| `ChatContainer.gdc` | theme-helper | 50 | `var font_size = screen_size.x / 50` |
| `ExitButtonContainer.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `ExtraOptionPad.gdc` | geometry-helper | 1.66667; 0 | `var minimum_y = plane_container.size.x / 1.66667` \| `self.custom_minimum_size.y = 0` |
| `ExtraOptionsHBox.gdc` | theme-helper | 1.66667; 0.15; 3.0; 30 | `var minimum_y = plane_container.size.x / 1.66667` \| `var desired_size = Vector2(screen_size.x * 0.15 / 3.0, screen_size.x * 0.15 / 3.0)` \| `var font_size = screen_size.x / 30` |
| `ExtraOptionsVbox.gdc` | empty | — | 0 token，空脚本 |
| `FireContainer.gdc` | theme-helper | 0.07; 0.0; 30 | `var desired_size = Vector2(screen_size.x * 0.07, 0.0)` \| `var font_size = screen_size.x / 30` |
| `FxContainer.gdc` | theme-helper | 60 | `var font_size = screen_size.x / 60` |
| `GameNameContainer.gdc` | theme-helper | 0.24; 0.0; 40 | `var desired_size = Vector2(screen_size.x * 0.24, 0.0)` \| `var font_size = screen_size.x / 40` |
| `GameSetupPlayerEntry.gdc` | theme-helper | 40 | `var font_size = screen_size.x / 40` |
| `HBoxContainer.gdc` | theme-helper | 0.15; 3.0; 30 | 同 `ButtonGrid`（0.15/3.0 两轴 + 字号 /30） |
| `HBoxVictoryIcon.gdc` | theme-helper | 10 | `var margin = - screen_size.x / 10` → `add_theme_constant_override("separation", margin)`（**负值**） |
| `IAP_IOS.gdc` | other | 0; 1; 2; 3; 4 | — |
| `IAP_holder.gdc` | other | — | — |
| `IOS_album_holder.gdc` | other | — | — |
| `IpPopUpMargin.gdc` | theme-helper | 10; 40 | 四个 `margin_*` = `margin_value`（**10 px 固定**）\| `var font_size = screen_size.x / 40` |
| `LanLobbyFontResize.gdc` | theme-helper | 40 | `var font_size = screen_size.x / 40` |
| `LanLobbyTitle.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `LevelDescriptionScrollContainer.gdc` | theme-helper | 40 | `var font_size = screen_size.x / 40` |
| `LevelTitleContainer.gdc` | shell | — | `_ready`/`_process` 全 `pass` |
| `LevelTitleLabel.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `LobbyButtonContainer.gdc` | theme-helper | 0.2; 0.0; 30 | `var desired_size = Vector2(screen_size.x * 0.2, 0.0)` \| `var font_size = screen_size.x / 30` |
| `MainMenuButtonResize.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `MarginContainer.gdc` | theme-helper | — | `margin_left = safe_area.position.x` / `margin_right = window_size.x - safe_area.end.x` / `margin_top = safe_area.position.y` / `margin_bottom = window_size.y - safe_area.end.y`（**无数字常量**） |
| `MarginContainerPopUpBack.gdc` | theme-helper | — | 同上，但只设 left/right/top |
| `OffsetContainer.gdc` | theme-helper | 0.09; 0.0; 60 | `var desired_size = Vector2(screen_size.x * 0.09, 0.0)` \| `var font_size = screen_size.x / 60` |
| `OffsetLabel.gdc` | geometry-helper | 0.12; 0.0 | `var desired_size = Vector2(screen_size.x * 0.12, 0.0)` |
| `PlayerNameLineEdit.gdc` | shell | — | 只有 `extends LineEdit` |
| `ResizeContainer.gdc` | geometry-helper | 1.0 | `var desired_size = Vector2(screen_size.x * 1.0, screen_size.y * 1.0)` |
| `RoomName.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `ShopButtonsSize.gdc` | geometry-helper | 2; 50; 3 | `var minimum_size = (safe_area.size.x / 2 - 50) / 3` |
| `ShowcaseMarginContainer.gdc` | theme-helper | 5 | 四个 `margin_* = 5`（**5 px 固定**） |
| `SoldierIconMarginContainer.gdc` | theme-helper | 100 | `var margin = - screen_size.x / 100` → `margin_left`/`margin_right`（**负值**） |
| `TimerContainer.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `TitleSize.gdc` | theme-helper | 1.0; 0.0; 8 | `var desired_size = Vector2(screen_size.x * 1.0, 0.0)` \| `var font_size = screen_size.x / 8` |
| `TurnLimitText.gdc` | theme-helper | 20 | `var font_size = screen_size.x / 20` |
| `UnlockButton.gdc` | shell | — | `_ready`/`_process` 全 `pass` |
| `UnlockButtonSize.gdc` | theme-helper | 30 | `var font_size = screen_size.x / 30` |
| `VersionLabel.gdc` | theme-helper | 60 | `var font_size = screen_size.x / 60` |
| `ViewFunctionPad.gdc` | geometry-helper | 1.66667; 0 | `var minimum_y = plane_container.size.x / 1.66667` + `if extra_y >= target_y \|\| !force_show:` 分支设 `custom_minimum_size.y`/`visible` |
| `ViewFunctionsLabel.gdc` | theme-helper | 0.15; 3.0; 10 | `var desired_size = Vector2(screen_size.x * 0.15 / 3.0, …)` \| `var font_size = screen_size.x / 10` |
| `android_iap.gdc` | other | 0; 1; 2; -3; -2; -1; 3; 4; 5; 6; 7; 8; 12; 0.4 | 内购状态码，与界面无关 |
| `endgame_pop_up.gdc` | shell | — | `_ready`/`_process` 全 `pass` |

### 5.4 三个"零数字"脚本的读法

`MarginContainer` / `MarginContainerPopUpBack` / `AdContainer` 的常量表里**没有数字**，
它们完全靠系统 API 算：`DisplayServer.get_display_safe_area()` 与节点自身的 `size`。
这不是"没解出来"，而是源码本来就这么写——重建时应当照抄表达式，不要硬塞数字。

---

## 6. 与 `docs/88` §12.0 / `docs/92` §2.3 的差异（以字节码为准）

| 旧说法 | 本轮实测 | 说明 |
|---|---|---|
| "40 个布局助手"（`docs/88` §12.0）、"34 个纯布局助手"（`docs/92` §2.3） | 非插件脚本 **52 个**：30 个 theme-helper + 6 个 geometry-helper + 4 个 shell + 1 个 empty + 6 个广告/IAP + **5 个行为脚本** | 三个数字口径都不同（是否含行为脚本、是否含边距类、是否含空壳）；本节按字节码重数 |
| "51 个非插件脚本" | 52（`rsc/` 根目录 52 个 `.gdc`，`addons/` 53 个） | 差 1，可能是当时漏了某个根目录脚本 |
| "ChatBox：点聊天区 → `chat_line_edit.grab_focus()`" | ✅ 完全正确，且**鼠标/触屏各一遍** | 补充细节 |
| "`.gdc` 只是响应式布局助手，无法拿到常量" | 常量已经全部拿到（本文 §5） | 本文的增量 |
| "界面逻辑全在 Rust" | **仍然成立**：5 个脚本里没有一条游戏规则 | 字节码侧再次确认 |

> **不建议**据此推翻 `docs/88` §12.0 的结论：反编译拿到的是"UI 装配"，不是"对局行为"。
> 但"反编译没有价值"这句话要修正为：**它能把界面的静态数值一次拿全**（这正是重建需要的），
> 只是拿不到按钮按下之后发生什么。

---

## 7. 未知 / 拿不到

| 项 | 状态 | 缺什么 |
|---|---|---|
| 完整反编译（AST、局部类型、常量折叠） | **不做** | 本工具是 token→文本直译；复合表达式不还原成优化前源码，注释/空行不保留 |
| 跨行语句的行内布局 | **部分** | 行表只给"这一行从哪个 token 开始"；一个语句跨行时会被拆成几段（本语料未见跨行语句，105/105 都是单行） |
| 字符串的原始字节 | **已解**（含非 ASCII） | U+2009 等已按整 u32 异或正确解码 |
| NodePath 常量（`^"a/b"`） | **推断** | 只用 4 个探针样本验证过；本语料 0 个，格式里第三个 u32 的含义未知 |
| 运行时效果（字号在 1080p 下到底多大、会不会溢出） | **未验证** | 只解出代码；没跑原版游戏量像素（`docs/92` 的 dump 里有控件树，但没有 theme override 后的实际字号） |
| token 编号在其它 Godot 版本 | **未验证** | 仅 4.7.2（版本 101）实测 |
| 未出现在本语料的 token | **未知** | 例如 `219-222` 之外的内置常量、`array/dict` 常量编码、`@export_range` 以外的注解形态 |
| `.gdc` 与 `.gd` 的逐字对应 | **不追求** | 伪源码故意保留 `for i in 21`、`screen_size` 这类原始名字，但不保证与原 `.gd` 文本逐字符一致 |

---

## 8. 复现

```bash
cd gw2-rebuild        # 或你自己的 clone；rsc/ 是 gitignore，需要先生成

# ① 两类产物一次出（stdout：5 个行为脚本 + 47 行数值表）
../.venv/bin/python tools/gdc_disasm.py --all ../rsc

# ② 机器可读全量（105 脚本：标识符/常量/函数/伪源码/分类）
../.venv/bin/python tools/gdc_disasm.py --all ../rsc --no-report --json /tmp/gdc.json

# ③ 数值表落盘（Markdown / TSV）
../.venv/bin/python tools/gdc_disasm.py --all ../rsc --no-report --md layout.md
../.venv/bin/python tools/gdc_disasm.py --all ../rsc --no-report --tsv layout.tsv

# ④ 单脚本伪源码
../.venv/bin/python tools/gdc_disasm.py ../rsc/ChatBox.gdc --pseudo

# ⑤ 自检：必须"105 个脚本，失败 0 个"
../.venv/bin/python tools/gdc_disasm.py --all ../rsc | tail -3
```

**自检判据**（解析错了的话这些一定不成立）：

* 每个脚本都"精确耗尽 payload"，没有残留字节（解析器会报 `trailing bytes`）；
* 函数名必须是标识符表里的词（`_ready` / `update_sizes` …），不能出现 `id?17`；
* 字号除数必须落在 8–60、边距 5/10、比例 0.07–1.0——**出现 28800 或 0.0001 就是错位了**；
* 105 个脚本的 `font_size` 表达式形状应只有 §5.2 那几种。

### 8.1 token 表怎么重验（可选）

```bash
mkdir -p /path/to/probe && cd /path/to/probe
# project.godot: 只有 config_version=5 + application/config/name
# export_presets.cfg: 一个 Linux/X11 预设（export_filter="all_resources"）
# probe.gd: 附录 A 的源码
$WS/tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path . \
    --export-pack "Linux" out.pck          # 不需要导出模板
$WS/.venv/bin/python $WS/gw2-rebuild/tools/pck_extract.py --pck out.pck --out pck
$WS/.venv/bin/python $WS/gw2-rebuild/tools/gdc_disasm.py pck/probe.gdc --pseudo
# 把输出的每一行与 probe.gd 对照，即可重编号 §3 的表
```

---

## 附录 A：探针源码（生成 §3 表的输入，共 4 个小文件）

四个文件都编译通过（`godot --headless --export-pack`），产物版本均为 101。

**A1 `probe_a.gd`** —— 注解/关键字/运算符/常量/函数参数：

```gdscript
@tool
extends Node

class_name ProbeClass

signal sig_a(x, y)
enum En { A, B }

const C1 = 1
const C2 := 2.5
@export var e1: int = 3
@onready var e2 = 4
var v1 = 1
var v2: float = 2.0
static var sv = 3

static func sfn():
	pass

func f1(a):
	return a

func f2(a, b = 1, c: int = 2) -> int:
	var x := 1
	x = a + b
	x = x - 1
	x = x * 2
	x = x / 3
	x = x % 4
	x = x ** 2
	x += 1
	x -= 1
	x *= 2
	x /= 2
	x %= 2
	x = -x
	x = ~x
	x = x << 1
	x = x >> 2
	x = x & 3
	x = x | 4
	x = x ^ 5
	if x == 0 and a != b or not x:
		return x
	elif x <= 0:
		return -x
	elif x >= 1:
		pass
	else:
		pass
	while false:
		break
	for i in range(3):
		continue
	match x:
		1:
			pass
		_:
			pass
	var arr = [1, 2, 3]
	var d = {"a": 1}
	var s = "hi" + str(x)
	var sn = &"sname"
	var b1 = x is int
	var b2 = x in arr
	var l = x if true else y
	assert(true)
	return 0

func f3() -> void:
	super()
	await get_tree().process_frame
	pass
```

**A2 `probe_b.gd`** —— 旧式逻辑运算符与 `as`（语料里真的在用）：

```gdscript
extends Node

func g(a, x) -> void:
	var b = a && true
	var c = a || false
	var d = !a
	var e = x as Dictionary
	var f = x as Node
```

**A3 `probe_c.gd`** —— 比较、数学常量、`match ... when`：

```gdscript
extends Node

func g(a, b) -> void:
	var c = a < b
	var d = a > b
	var e = TAU
	var f = PI
	var g2 = INF
	var h = NAN
	match a:
		1 when b > 0:
			pass
		_:
			pass
```

**A4 `probe_d.gd`** —— 分号（= token 208）与注释不产生 token：

```gdscript
extends Node

func b() -> void:
	print(1)
	print(2);
	print(3)

	# 独立注释行不会产生 token
	print(4)
```

> 用法见 §8.1。**注意**：`probe_b` 里 `&&`/`||`/`!` 与 `as` 在 4.7.2 上仍可编译（它们是
> GDScript 4 保留的旧式写法），这正是游戏脚本能用到它们的原因；换引擎版本要重跑。
