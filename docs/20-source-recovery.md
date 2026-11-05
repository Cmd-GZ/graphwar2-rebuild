# 20 · 原版源码面的回收

> **本篇解决什么问题**：`Graphwar.pck` 里**还以"源码"形态存在的那一部分**能取回多少、
> `.gdc` 字节码里有什么、GDRE 整项目恢复（`.tscn` / `.gd` / `.tres`）能补上什么、
> 静态属性与运行时台账怎么对拍，以及"还原源代码"这条路现在站在哪。
>
> ⚠️ **资产警告**：GDRE 恢复出的 `.tscn` / `.gd` / `.tres` / 贴图 / 字体**全部是游戏资产**，
> 不入库、不推公开远端。仓库里只放清单（`recovered-src/MANIFEST.tsv`）。
>
> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `146-source-recovery.md` | 源码面的完整构成、回收方法与成功率 | [`archive/146-source-recovery.md`](archive/146-source-recovery.md) |
| `104-recovered-project.md` | GDRE 整项目恢复 + 与运行时台账对拍 | [`archive/104-recovered-project.md`](archive/104-recovered-project.md) |
| `101-gdc-contents.md` | `.gdc` 的常量池 / 标识符表 / 函数表 / 布局助手数值 | [`archive/101-gdc-contents.md`](archive/101-gdc-contents.md) |

界面台账在 [`16-client.md`](16-client.md) §2；模块地图在 [`02-module-map.md`](02-module-map.md)。
证据标记沿用四档（定义见 [`06-verification.md`](06-verification.md) §1）。

## 1. 一句话结论

### 1.1 结论（一句话）

包里的 692 个文件中，**有 160 个曾经是文本源码，全部取回，0 失败**：
105 个 `.gdc` → `.gd`（**token 流逐个相同**）、31 个 `.scn` → `.tscn`、22 个 `.res` → `.tres`
（**二进制/文本两态的 SceneState 与属性表完全相同**）、`project.binary` → `project.godot`、
`Graphwar.gdextension` 原样是文本。
剩下 532 个**天生不是源码**（`.ctex` / `.import` / `.remap` / `.fontdata` / 音视频），
它们的作者端输入（149 `.svg` + 17 `.png` + 1 `.jpeg` + 7 `.ttf` + 9 段音频）
**根本不在包里**，不存在"再努力一下就能取回"的问题。

同时确认了一个对后续路线至关重要的事实：**105 个脚本里没有一个是屏幕控制器**，
它们的逻辑全部在 `libgraphwarrust.so` 里（§5）。

---


## 2. 原版源码面的完整构成

### 2.1 原版源码面的完整构成

（档位：**观测** —— 由 PCK 尾部索引逐条解析得到，`--list` 可复现）

| 扩展名 | 数量 | 它是什么 | 能不能变成文本源码 |
|---|---:|---|---|
| `.gdc` | 105 | Godot 4 GDScript 字节码（`GDSC` + zstd） | **能** → `.gd` |
| `.scn` | 31 | 二进制 `PackedScene` | **能** → `.tscn` |
| `.res` | 22 | 二进制 `Resource`（Theme / StyleBox / FontFile） | **能** → `.tres` |
| `.gdextension` | 1 | 扩展声明，包内已是文本 | 已经是文本 |
| `project.binary` | 1 | `ECFG` 工程设置 | **能** → `project.godot` |
| `.import` | 184 | 导入器元数据 | 不是源码 |
| `.remap` | 158 | 导出重映射（"这个源路径其实在包里叫那个哈希名"） | 不是源码，但**是回收路径的依据** |
| `.ctex` | 167 | 导入后的压缩纹理 | 不是源码（是 149 `.svg` + 17 `.png` + 1 `.jpeg` 的产物） |
| `.fontdata` | 7 | 导入后的 MSDF 字体 | 不是源码（是 7 个 `.ttf` 的产物） |
| `.mp3str` / `.sample` / `.oggvorbisstr` / `.wav` | 11 | 音频 | 不是源码 |
| `.png` | 1 | 唯一一个未被导入的位图（`basic_soldier.png`，也是工程图标） | 不是源码 |
| `.cfg` / `.bin` | 4 | `plugin.cfg` ×1、Godot 编辑器缓存（`uid_cache.bin`、`extension_list.cfg`、`global_script_class_cache.cfg`） | 不是源码 |
| **合计** | **692** | | **160 个可回收** |

#### 2.1.1 原始路径从哪来 —— 不用猜

导出后的资源在包里叫哈希名：

```
res://.godot/exported/133200997/export-b5463874e5e2e3f9c8341776946bf479-play_screen.scn
```

**作者路径是包自己给的**：每个被转换的文件都有一份同名兄弟 `.remap`，正文是

```
[remap]

path="res://play_screen.gdc"        # 或上面那个哈希路径
```

于是 `play_screen.tscn` 是**读出来的**，不是推断的。
而且这是一个可校验的断言（**观测**）：158 个 `.remap` 恰好分解为
`105 × .gd` + `31 × .tscn` + `22 × .tres`，与三种容器的计数逐一相等——
映射是**满的**，没有剩余、没有多余。

---


## 3. 每一类的回收方法与成功率

### 3.1 每一类的回收方法与成功率

总表（`--check` 每次重算）：

| 类别 | 输入 | 输出 | 成功 | 档位 |
|---|---|---:|---:|---|
| GDScript | 105 `.gdc` | 105 `.gd`（4 210 行 / 90 648 B） | **105/105** | **逐位验证**（token 流） |
| 场景 | 31 `.scn` | 31 `.tscn`（215 575 B） | **31/31** | **观测**（SceneState 全等） |
| 资源 | 22 `.res` | 22 `.tres`（17 479 515 B，见 §3.3） | **22/22** | **观测**（属性表全等） |
| 工程设置 | 1 `project.binary` | `project.godot`（21 条） | **1/1** | **观测**（1346/1346 字节解完） |
| 扩展声明 | 1 `.gdextension` | 原样 | **1/1** | **观测**（逐字节相同） |

`MANIFEST.tsv` 的 692 行状态分布：**完整 124 / 部分 36 / 失败 0 / 非源码 532**。
"部分"的全部原因只有两类，且都写在每一行的备注里：`ext_resource` 的 uid 属性丢失（32 行，
§3.2）、文本里内嵌了二进制图集（5 行，§3.3）。

#### 3.1.1 `.gdc` → `.gd`：GDRE + **token 级对拍**

* 反编译用 **GDRE Tools v2.7.4**（`--recover --scripts-only`）。`--decompile` 也可用，
  但输出目录是扁平的，而本包有同名不同目录的文件（`addons/admob/sample/MobileAds.gdc`
  与 `addons/admob/src/api/MobileAds.gdc`），扁平输出会**互相覆盖**——所以按原路径整树恢复。
* 字节码版本是 **101**（`.gdc` 头 `GDSC` + `u32 101`）。GDRE 自带的版本表里 101 对应
  `4.5.0-stable (ebc36a7)` 的 token 表；GDRE 从 PCK 头自动识别，报告里也写 4.7.0。
* **不信任反编译器的正确性，而是对拍**：调 `tools/gdc_verify_source.py`（本仓库已有），
  把恢复出的 `.gd` 重新词法化，与从 `.gdc` 解出的 `(kind, value)` token 序列逐项比较。

```
verified 105/105 scripts token-for-token
```

  这意味着反编译器**没有凭空生成、也没有丢弃任何一个 token**；能差的只有排版
  （缩进、空行、`- 3` 与 `-3` 这种写法差异）。这是"还原源代码"能做到的最强档位之一——
  粒度要说清楚：**token 序列逐位相同，不是源文本逐字节相同**（`.gdc` 根本不存文本）。

* 105 个脚本的继承面（**观测**，`grep -h '^extends'` 统计）：
  `MarginContainer` 20、`MobileSingletonPlugin` 8、`HBoxContainer` 8、`Control` 6、
  `VBoxContainer` 4、`RichTextLabel` 2、`Node` 2、`Label` 2、**`GraphwarButton` 2**，
  其余 `TabContainer` / `ScrollContainer` / `LineEdit` / `GridContainer` /
  `AspectRatioContainer` / `Object` / `EditorPlugin` 各 1。
  全是"尺寸自适应 / 布局 / 插件门面"这类小控件，**没有一个是屏幕控制器**。

#### 3.1.2 `.scn` / `.res` → `.tscn` / `.tres`：让 Godot 自己写

做法：把一个 HEADLESS 脚本塞进"完整解包出来的工程"里，`ResourceLoader.load()` 之后
`ResourceSaver.save()` 成文本——**文本序列化由 Godot 自己完成**，不存在"我另写一个
`PackedScene` 读取器，然后它理解错了"这种失败模式。

两个实现细节，都是踩出来的：

1. **必须整包 staging，不能只放 53 个文件**。`play_screen.scn` 引用
   `res://graphwar_theme.tres`，后者又引用 `res://ComicRelief-Bold.ttf`。只解转换目标
   会让这些外部引用静默变成 `null`，转出来的文本"看起来对"，其实丢了一层。
2. **一个文件一个 Godot 进程**（0.5 s/个，53 个约 30 s）。同进程批量做时，引擎报错
   无法归因到具体文件；分进程后每条 `ERROR` 天然属于一个输入。

输出保持 `res://` 原样，**没有"修好"任何路径**——这是刻意的：`play_screen.tscn` 引用
`res://return.svg`，而包里只有它的产物 `return.svg.ctex`，这正是原版工程的真实状态。

**验证（比"能打开"强一档，档位：观测）**：把二进制形态和文本形态都加载进来，比较

* `PackedScene` → `get_state()`：节点数、每节点的 type / name / instance / placeholder、
  每个存储属性的名字与值、连接数；
* 其它 `Resource` → 类名 + 全部 `PROPERTY_USAGE_STORAGE` 属性的名字与值。

```
round-trip verify: 53/53 equal
```

唯一的"差异"是子资源路径 `<owner>::<id>` 里的 owner 前缀（两边 owner 本来就是不同文件），
规范化掉 `::` 之前的部分后全等。

**已知损失：`ext_resource` 的 `uid` 属性（32 个文件，`部分`）。** 这是本篇唯一一处
"文本比二进制少东西"，讲清楚：

* 二进制 `.scn`/`.res` 的外部资源表是
  `u32 type_len | type\0 | u32 path_len | path\0 | u64 uid`（逐字节走通 38 个条目验证过，
  `tools/recover_source.py: binary_ext_uids()`）；
* Godot 的二进制读取器要把这个 `u64` 换成 `uid://...` 才会保留它，查找表是
  `res://.godot/uid_cache.bin`。而**这个包里的 uid_cache 只登记了 108 条**
  （105 个 `.gdc` + `.png` + `.gdextension` + `.wav`），**一个 `.tres`/`.tscn` 都没有**；
* 于是读取器判定 UID 无效（原版运行时也是这么判的，日志会打
  `invalid UID ... using text path instead`），回退到路径，写文本时自然就没有 `uid=`。

副作用是好的：**我们的文本形态加载时不产生任何 "invalid UID" 警告**，因为它不再携带
本来就解析不了的 UID。路径是权威引用，节点/属性/子资源一个不少。
（脚本另外做了 uid 注册实验：把从二进制里读出的 81 个 uid 用 `ResourceUID.add_id()` 注册后，
`ResourceUID.id_to_text()` 输出 `uid://ck64hmdpe1hbv`，与 GDRE 从同一份字节里读出的值**逐字符相同**——
这既证明了 uid 解析正确，也证明 Godot 的文本写入器即使 UID 有效也只写路径。）

#### 3.1.3 个 `.res` 的真实构成：17 个是配置，5 个是"文本化的二进制"

`.tres` 总共 17.5 MB，其中 **17.2 MB 来自 5 个文件**：

| 文件 | 大小 | 里面是什么 |
|---|---:|---|
| `game_menu_icon_theme.tres` | 6.08 MB | 多张 256×256 `Image`（`LumAlpha8` 字形图集） |
| `button_font.tres` | 3.72 MB | `FontFile` 自带字形缓存 |
| `campaign_button_theme_2.tres` | 2.56 MB | 同上，内嵌图集 |
| `campaign_hidden_button_theme.tres` | 2.56 MB | 同上 |
| `campaign_spacer_theme.tres` | 2.54 MB | 同上 |

它们**是合法的 `.tres`**（Godot 能加载，属性表与二进制全等），但内容是
`PackedByteArray` 包着的像素数据——即"二进制资产穿了文本的外衣"。
`MANIFEST.tsv` 把这 5 行标成 `部分` 并在备注里写了实际字节数，方便一眼看出
"这 6 MB 不是源码"。要缩小仓库可以只删这 5 个文件（同时 `--check` 会报缺失，
所以删的时候应连同清单一起决定，不要半删）。

#### 3.1.4 `project.binary` → `project.godot`

`ECFG` 容器：`u32 'ECFG' | u32 count | count × (u32 klen | key | u32 vlen | Variant)`。
21 条设置，解完恰好 **1346/1346 字节**（这就是"没有漏读"的证明）。

Variant 类型覆盖：`1=bool 2=int 3=float 4=String 5=Vector2 20=Color 21=StringName 34=PackedStringArray`。
其中 **34 是关键的一个**——老的 `tools/extract_assets.py` 把它记成"未知类型 34"，
实际是 `PackedStringArray`（`u32 count` + `count × (u32 len | 数据补齐到 4)`，
`len` **含**结尾 NUL）。解开正好是：

```
application/config/features   = ["4.7", "Mobile"]
editor_plugins/enabled        = ["res://addons/admob/plugin.cfg",
                                 "res://addons/android_IAPP/plugin.cfg"]
```

字段名与值**原样**输出，不补默认值、不重排。唯一新增的一行是 `config_version=5`
（Godot 4 的格式要求，不写这个文件根本加载不了），在文件头注释里已标明。
一个有意义的观测：包里**只有 `rendering_method.mobile`，没有桌面版
`rendering_method`** ——因为导出目标就是移动端，这一条不是"丢了"，是本来就没有。

---


## 4. 哪些东西天生不是文本（不需要回收）

### 4.1 哪些东西**天生**不是文本（不需要回收）

这一节是为了防止后面有人把"回收率 160/692"当成未完成度。

1. **`.ctex`（167）/ `.fontdata`（7）不是源，是产物。** 184 个 `.import` 的正文（本包实测，
   与 `tools/extract_assets.py` 里写的 `source_file=` / `dest_files=` 那套**不同**）是：

   ```
   [remap]
   importer="texture"
   type="CompressedTexture2D"
   uid="uid://csf825kakwc76"
   path="res://.godot/imported/return.svg-df7d80a0e6571ad49ff5974add9474f3.ctex"
   ```

   源路径没有写在文件里——它就是 `.import` 的**文件名去掉 `.import`**。
   按 `importer=` 归类：`texture` 167（149 `.svg` + 17 `.png` + 1 `.jpeg`）、
   `font_data_dynamic` 7（`.ttf`）、`mp3` 6、`wav` 3、`oggvorbisstr` 1，合计 184。
   这些作者端输入（149 svg / 17 png / 1 jpeg / 7 ttf / 9 段音频）
   **全部不在包里**，导出器只留下了 167 个 `.ctex` + 7 个 `.fontdata` + 音频流。
   从 `.ctex` 反推 `.svg` 是没有定义的（光栅化不可逆），所以这一类永远是"素材"而不是"源码"。
   客户端怎么处理见 `16-client.md` §5。
   附带一个交叉验证：`return.svg.import` 里的 `uid="uid://csf825kakwc76"` 与我们从
   `play_screen.scn` 外部资源表读出的 `u64 6048615910000589367` 经 `ResourceUID.id_to_text()`
   的结果**完全一致**——这独立证明了 §3.2 的 uid 解析是对的。
2. **`.import`（184）/ `.remap`（158）是导出簿记。** 它们是包自己给的映射表，
   我们正是靠 `.remap` 还原出 158 个原始路径（§2.1）。它们是**工具**，不是内容。
3. **音频 11 + `.png` 1**：媒体文件。`basic_soldier.png` 是唯一没被导入的位图，
   同时是 `application/config/icon`；它留在包里说明原工程直接引用了 png。
4. **编辑器缓存 3+1**：`uid_cache.bin`（二进制）、`extension_list.cfg`、
   `global_script_class_cache.cfg`（45 个 `class_name` 声明，全是 AdMob 插件那一套）。
   这些在真实工程里是**生成物**，作者也不会提交。
5. **`.cfg` 里的例外**：`addons/admob/plugin.cfg` **确实是源码**（插件清单：
   `name="AdMob" / author="Poing Studios" / version="v3.1.1" / script="admob.gd"`）。
   它没进 `recovered-src/` 只有一个原因：本目录的扩展名白名单
   （`.gd .tscn .tres .godot .gdextension`）不含 `.cfg`。需要时 `--recover` 之外
   单独取即可，内容已在上面引全。

---


## 5. GDRE 整项目恢复：静态属性与运行时台账对拍（原 104）

### 5.1 一页结论

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
| `20-source-recovery.md` 的 5 个行为脚本 | **0 处更正**（5/5 与我用字节码推的伪源码语义一致） | 观测 |

三句话：

1. **静态能补的比想的多**：`theme_override_font_sizes` 有 9 处、`custom_minimum_size` 有 29 处、
   锚点分布 51 个 `FULL_RECT`——这些都是 92 的运行时 dump 拿不到的（dump 只打印文本/类名）。
2. **对不上的是"动态创建"，不是解析错**：运行时比静态多出来的节点可以用 3 条规则全部解释
   （玩家界面的皮肤格子、21 个关卡按钮、PlayScreen 的 3 个绘图节点），见 §5.3。
3. **连接不在场景里**：25 个游戏 `.tscn` 一条 `[connection]` 都没有——一切连接都在
   `.gd` 的 `connect()` 与 Rust 里。这解释了 92 §2.3 "只剩 27 条人为连接"的口径，见 §6。

---


## 6. `.gdc` 字节码里有什么（原 101）

### 6.1 一页结论

| 项 | 结果 | 证据等级 |
|---|---|---|
| `.gdc` payload 解析 | **105/105 成功**，每个文件都精确消耗到最后一个字节（0 残留） | 观测（`tools/gdc_disasm.py --all`） |
| 语料规模 | 105 脚本 · 2200 个标识符 · **546 个常量** · 14075 个 token | 观测 |
| 字节码版本 | 全部 `101`（Godot 4.7.2） | 观测 |
| token 类型表 | 语料里出现的 **59 种**全部有名字；编号由"编译探针"逐位对照得到 | 逐位对照（探针编译） |
| 5 个行为脚本 | 常量池 / 函数表（名字+参数个数）/ 成员 / **伪源码** 全部拿到（§4） | 观测 |
| 布局助手数值表 | 非插件脚本 47 个全部列出数值与表达式（§5） | 观测 |
| 是否"反编译" | 不是完整反编译：没有 AST、没有类型推断、注释/空行丢失；**但语句级伪源码可读** | 见 §7 |
| 与 GDRE 反编译源码的一致性 | **105/105 逐 token 一致**；5 个行为脚本 **0 处更正** | 逐位对照（§4.0、`tools/gdc_verify_source.py`） |
| 拿不到的 | 运行时行为、多行语句的精确换行、NodePath 的特殊形态；见 §7 | — |

三条最有用的事实：

* **字号全部是"屏幕宽 ÷ N"**，不是固定像素：28 处 `font_size` 赋值，N ∈ {8,10,15,20,30,40,50,60}，
  其中 N=30 出现 13 次（§5.2）。重建时只要拿到 N 和"用哪个宽度"（`get_viewport_rect().size`
  还是 `DisplayServer.get_display_safe_area().size`）就能逐像素复现。
* **边距只有三种写法**：固定像素（`ShowcaseMarginContainer` = 5、`IpPopUpMargin` = 10）；
  "安全区推出来"（`MarginContainer` / `MarginContainerPopUpBack`，无数字常量）；
  以及按屏幕宽的**负**比例（`SoldierIconMarginContainer` = −w/100、`HBoxVictoryIcon` 的
  `separation` = −w/10）。
* 5 个行为脚本的行为**全部是 UI 装配**，没有一条游戏规则——这与 `06-verification.md` §12.0 的判断一致，
  但现在有了逐条 token 证据（§4）。

---

### 6.2 payload 格式（实测）

#### 6.2.1 容器

```
偏移 0   "GDSC"
偏移 4   u32  字节码版本（本包 = 101）
偏移 8   u32  解压后长度
偏移 12  zstd 帧 → payload
```

与 `tools/gdc_strings.py` 完全一致（该文件本轮**未改动**）。

#### 6.2.2 payload 分段

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

#### 6.2.3 标识符与常量

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

#### 6.2.4 行表与列表

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

### 6.3 token 编码（全部实测，非推测）

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

#### 6.3.1 已确认的编号（语料中出现 59 种；探针另外补全了语料没用到的那几种）

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

#### 6.3.2 未验证的部分

编号表是**字节码版本 101** 的。Godot 4 的 token 编号在大版本内稳定，但换引擎版本必须重跑附录 A
的探针；`gdc_disasm.py` 遇到非 101 版本会在 JSON 里写 warning（不静默）。

---

### 6.4 布局助手数值表（唯一权威来源 = 常量表）

#### 6.4.1 分类口径

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
（`06-verification.md` §12.0 写的"40 个"与 `16-client.md` §2.3 写的"34 个"都不是按字节码统计的，见 §6。）

#### 6.4.2 字号：全部是"宽度 ÷ N"

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

#### 6.4.3 全表（47 行，由 `gdc_disasm.py --md` 生成）

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

#### 6.4.4 三个"零数字"脚本的读法

`MarginContainer` / `MarginContainerPopUpBack` / `AdContainer` 的常量表里**没有数字**，
它们完全靠系统 API 算：`DisplayServer.get_display_safe_area()` 与节点自身的 `size`。
这不是"没解出来"，而是源码本来就这么写——重建时应当照抄表达式，不要硬塞数字。

---

### 6.5 未知 / 拿不到

| 项 | 状态 | 缺什么 |
|---|---|---|
| 完整反编译（AST、局部类型、常量折叠） | **不做** | 本工具是 token→文本直译；复合表达式不还原成优化前源码，注释/空行不保留 |
| 跨行语句的行内布局 | **部分** | 行表只给"这一行从哪个 token 开始"；一个语句跨行时会被拆成几段（本语料未见跨行语句，105/105 都是单行） |
| 字符串的原始字节 | **已解**（含非 ASCII） | U+2009 等已按整 u32 异或正确解码 |
| NodePath 常量（`^"a/b"`） | **推断** | 只用 4 个探针样本验证过；本语料 0 个，格式里第三个 u32 的含义未知 |
| 运行时效果（字号在 1080p 下到底多大、会不会溢出） | **未验证** | 只解出代码；没跑原版游戏量像素（`16-client.md` 的 dump 里有控件树，但没有 theme override 后的实际字号） |
| token 编号在其它 Godot 版本 | **未验证** | 仅 4.7.2（版本 101）实测 |
| 未出现在本语料的 token | **未知** | 例如 `219-222` 之外的内置常量、`array/dict` 常量编码、`@export_range` 以外的注解形态 |
| `.gdc` 与 `.gd` 的逐字对应 | **已核（token 级）** | `tools/gdc_verify_source.py`：105/105 逐 token 一致；但空白行、注释、负数字面量写法（`-3` vs `- 3`）不保证逐字符 |
| 源码级"原样还原" | **仍有差距** | 反编译源码是 token 流的可读渲染，不是开发者原始 `.gd`（缩进/空行/注释已丢失） |

---


## 7. 下一步：Rust 侧怎么按模块重建（结构建议）

### 7.1 下一步：Rust 侧怎么按模块重建（结构建议，本轮不做）

#### 7.1.1 先把"有多少模块"这件事定死（档位：观测）

对 `libgraphwarrust.so` 直接扫字符串，**58 个**互不相同的 `src/graphwar_*.rs` 文件名，
另有 2 个模块目录名（`graphwar_media`、`graphwar_support`，只以目录形态出现）：

```bash
python3 - <<'EOF'
import re
b = open("libgraphwarrust.so","rb").read()
names = {m.decode() for m in re.findall(rb"src/graphwar_[A-Za-z0-9_]+\.rs", b)}
print(len(names)); print("\n".join(sorted(names)))
EOF
```

顺便修正两个流传的数字（**未知 → 观测**）：

* 父任务给的"57 个模块"、`02-module-map.md` 写的"约 90 个" **都不是这个量的定义**。
  实测口径有三个，别混：
  - `src/graphwar_*.rs` 路径串：**58**
  - `fixtures/module_map.tsv` 里按引用归堆出的模块行：**87**
    （其中 `graphwar_*.rs` 56 行 + 31 行是 `entity_drawer.rs` / `effect_drawer.rs` /
    `math_function.rs` / `button_manager.rs` / `axis_drawer.rs` / `player_showcase.rs`
    这类**子模块**，它们不是 `graphwar_` 前缀）
  - 两者并不矛盾：58 是"顶级文件"，87 是"能归堆的文件 + 子模块"。
    后续写文档时请分开表述。

#### 7.1.2 用 `src:line` panic 串反推文件规模（档位：观测，且**只是下界**）

Rust 在 panic 路径里保留 `src/graphwar_xxx.rs:NNN` 形式的字符串。
实测 `.so` 里有 **55 处、54 个互不相同的 `(模块, 行号)`、覆盖 12 个模块**：

| 模块 | 观测到的行号数 | 最小行 | 最大行（= 文件行数下界） |
|---|---:|---:|---:|
| `graphwar_root_node.rs` | 12 | 513 | **1302** |
| `graphwar_event.rs` | 2 | 1149 | **1154** |
| `graphwar_play_screen_control.rs` | 1 | 561 | 561 |
| `graphwar_function_input.rs` | 1 | 519 | 519 |
| `graphwar_remote_event_handler.rs` | 14 | 77 | 465 |
| `graphwar_game_server.rs` | 4 | 164 | 336 |
| `graphwar_audio_control.rs` | 1 | 241 | 241 |
| `graphwar_server_connection_handler.rs` | 1 | 196 | 196 |
| `graphwar_backend.rs` | 1 | 142 | 142 |
| `graphwar_lan_broadcaster_v6.rs` | 4 | 78 | 126 |
| `graphwar_crash_reporter.rs` | 8 | 54 | 100 |
| `graphwar_server_broker_handler.rs` | 5 | 62 | 94 |

**必须写清这个方法的边界**：没有 `path:line` 串 **不代表**文件小或不存在，
只代表"这个文件里被编译器保留下来的 panic 位置恰好没被观测到"。
所以上表是**下界**，不是行数；46 个模块连下界都还拿不到。

#### 7.1.3 可执行的重建顺序（建议）

1. **先建骨架，不写逻辑**：按 5.1 的 58 个名字建 `src/graphwar_*.rs`，
   每个文件先只放 `// 观测：字符串存在于 <so sha256> 偏移 X` 和空 `pub fn`。
   这样 `cargo test` 立刻能跑，且"文件名与二进制一致"这条可以被脚本校验。
2. **已逐位验证的 4 个入口先落地**（`02-module-map.md`、`REPORT.md` 已有结论）：
   `graphwar_function_input.rs`（解析，`0x4ed670`）、`graphwar_path_calculator.rs`
   （采样，`0x3c2530`）、`graphwar_shot_calculator.rs`（开火结算 `0x3c2bc0`）、
   修正 `0x3c5f70`。这四个是唯一"行为已被夹具钉死"的部分，先让它们占住位置。
3. **用 `.gd` 反推 extension 的 API 面**（这是本轮新拿到的抓手，**观测**）：
   105 个 `.gd` 里有 2 个 `extends GraphwarButton`，而 `.so` 里有 `graphwar_button.rs`——
   GDScript 层是**继承** Rust 类的，所以 `.gd` 的调用点就是 Rust 侧的公开方法清单。
   具体做法：
   ```bash
   grep -rn "GraphwarButton\|Graphwar" recovered-src/*.gd | head
   ```
   再配合 `tools/ptr_slots.py`（函数指针派发）补"零直接调用 ≠ 没人用"。
4. **接口清单用已有的 `fixtures/module_map.tsv` 当路由表**：87 行 × (函数数, 引用数,
   首末地址) 已经把 13 000+ 函数分好了桶；写哪个模块就查哪一行，不要重新归堆。
5. **别用 `funcstarts.pkl`**（`AGENTS.md` 已写明它错），用 `funcstarts_ghidra_va.pkl`。

---


## 8. 档位汇总与已知缺口

### 8.1 档位汇总（哪些是硬结论，哪些还悬着）

| # | 结论 | 档位 | 依据 |
|---|---|---|---|
| 1 | 692 = 105+31+22+1+1 + 532，160 个曾是文本源码 | **观测** | PCK 尾部索引逐条解析 |
| 2 | 158 个 `.remap` 精确分解为 105/31/22，路径映射是满的 | **观测** | `--list` |
| 3 | 105/105 `.gd` 与字节码 **token 流逐个相同** | **逐位验证** | `gdc_verify_source.py`（token 粒度，非文本字节） |
| 4 | 31/31 `.tscn`、22/22 `.tres` 与二进制形态结构/属性全等 | **观测** | SceneState + 属性表对拍，53/53 |
| 5 | 32 个文件的 `ext_resource` uid 被文本写入器丢弃 | **观测** | uid_cache 只有 108 条；uid 注册实验；`id_to_text` 逐字符一致 |
| 6 | `project.binary` 21 条、1346/1346 字节解完；type 34 = `PackedStringArray` | **观测** | 解完恰好 EOF |
| 7 | 149 `.svg` / 7 `.ttf` 是 `.ctex`/`.fontdata` 的来源且不在包内 | **观测** | 每个 `.import` 的 `source_file`/`dest_files` |
| 8 | `libgraphwarrust.so` 含 58 个 `src/graphwar_*.rs` 名字 + 2 个目录 | **观测** | 直接字符串扫描 |
| 9 | 12 个模块的**文件行数下界**（root_node ≥1302 等） | **观测（下界）** | 54 个 `path:line` 串 |
| 10 | `.gd` 继承 Rust 类（`GraphwarButton` ↔ `graphwar_button.rs`），故 `.gd` 是 Rust 公开 API 的证据 | **观测 → 推断** | 继承串是观测；"调用点 = API 面"是推断，未逐个核对 |
| 11 | 屏幕控制器逻辑全在 Rust（GDScript 侧只有小控件） | **观测** | 105 个 `extends` 统计 + 无 `play_screen` 类脚本 |
| 12 | "57 个模块"/"约 90 个模块" | **未知（口径不明）** | 见 §5.1，实测口径是 58 / 87，两者含义不同 |
| 13 | 46 个模块的文件行数 | **未知** | 没有 `path:line` 串可用，只能靠反编译量函数边界 |

---

