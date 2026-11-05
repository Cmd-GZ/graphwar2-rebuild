# 146 —— 原版源码面的回收（第 0 步）

> 目标：把 `Graphwar.pck` 里**还以"源码"形态存在的那一部分**完整取回来，作为"还原源代码"
> 这条路的起点。本文只讲"取回原版已有的东西"，不讲"我们另写的实现"。
>
> 产物目录：[`recovered-src/`](../recovered-src/)　回收脚本：
> [`tools/recover_source.py`](../tools/recover_source.py)　清单：
> [`recovered-src/MANIFEST.tsv`](../recovered-src/MANIFEST.tsv)
>
> 相关前置：`docs/101-gdc-contents.md`（`.gdc` 容器格式）、`docs/104-recovered-project.md`
> （GDRE 全量恢复）、`docs/02-module-map.md`（Rust 模块归堆）。

**为什么"token 流逐个相同"就是能给到的最强等价（先说清楚，后面所有结论都靠它）：**
`.gdc` 里**根本不存文本**——它存的是标识符表、常量表、行表/列表和一个 token 流
（`docs/101` §3）。反编译器的输出只是这个 token 流的**一种渲染**：缩进、空行、
`- 3` 还是 `-3` 都可以变，注释和被编译器丢掉的括号永远回不来。所以"源文本逐字节相同"
在这个方向上不是难，而是**定义上不存在**。能给出的最强断言是：把恢复出的 `.gd`
重新词法化，与从 `.gdc` 解出的 `(kind, value)` 序列**逐项相等**——这排除了"反编译器
凭空生成 token、或悄悄丢掉 token"这两类唯一会误导人的错误。本文的 105/105 就是这个档位。

---

## 1. 结论（一句话）

包里的 692 个文件中，**有 161 个曾经是文本源码，全部取回，0 失败**：
105 个 `.gdc` → `.gd`（**token 流逐个相同**）、31 个 `.scn` → `.tscn`、22 个 `.res` → `.tres`
（**二进制/文本两态的 SceneState 与属性表完全相同**）、`project.binary` → `project.godot`、
`Graphwar.gdextension` 与 `addons/admob/plugin.cfg` 原样是文本。
其中 5 个 `.tres` 按仓库的"**不入库媒体**"策略落成**只留度量的骨架**（§3.5）。
剩下 531 个**天生不是源码**（`.ctex` / `.import` / `.remap` / `.fontdata` / 音视频 / 编辑器缓存），
它们的作者端输入（149 `.svg` + 17 `.png` + 1 `.jpeg` + 7 `.ttf` + 9 段音频）
**根本不在包里**，不存在"再努力一下就能取回"的问题。

同时确认了一个对后续路线至关重要的事实：**105 个脚本里没有一个是屏幕控制器**，
它们的逻辑全部在 `libgraphwarrust.so` 里（§5）。

---

## 2. 原版源码面的完整构成

（档位：**观测** —— 由 PCK 尾部索引逐条解析得到，`--list` 可复现）

| 扩展名 | 数量 | 它是什么 | 能不能变成文本源码 |
|---|---:|---|---|
| `.gdc` | 105 | Godot 4 GDScript 字节码（`GDSC` + zstd） | **能** → `.gd` |
| `.scn` | 31 | 二进制 `PackedScene` | **能** → `.tscn` |
| `.res` | 22 | 二进制 `Resource`（Theme / StyleBox / FontFile） | **能** → `.tres`（5 个只留骨架，§3.5） |
| `.gdextension` | 1 | 扩展声明，包内已是文本 | 已经是文本 |
| `project.binary` | 1 | `ECFG` 工程设置 | **能** → `project.godot` |
| `.import` | 184 | 导入器元数据 | 不是源码 |
| `.remap` | 158 | 导出重映射（"这个源路径其实在包里叫那个哈希名"） | 不是源码，但**是回收路径的依据** |
| `.ctex` | 167 | 导入后的压缩纹理 | 不是源码（是 149 `.svg` + 17 `.png` + 1 `.jpeg` 的产物） |
| `.fontdata` | 7 | 导入后的 MSDF 字体 | 不是源码（是 7 个 `.ttf` 的产物） |
| `.mp3str` / `.sample` / `.oggvorbisstr` / `.wav` | 11 | 音频 | 不是源码 |
| `.png` | 1 | 唯一一个未被导入的位图（`basic_soldier.png`，也是工程图标） | 不是源码 |
| `.cfg` | 3 | `addons/admob/plugin.cfg`（**是源码**，已收）×1 + 编辑器缓存 ×2 | 1 个是源码 |
| `.bin` | 1 | `.godot/uid_cache.bin`（编辑器 UID 缓存） | 不是源码 |
| **合计** | **692** | | **161 个可回收** |

### 2.1 原始路径从哪来 —— 不用猜

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

总表（`--check` 每次重算）：

| 类别 | 输入 | 输出 | 成功 | 档位 |
|---|---|---:|---:|---|
| GDScript | 105 `.gdc` | 105 `.gd`（4 210 行 / 90 648 B） | **105/105** | **逐位验证**（token 流） |
| 场景 | 31 `.scn` | 31 `.tscn`（215 575 B） | **31/31** | **观测**（SceneState 全等） |
| 资源 | 22 `.res` | 22 `.tres`（骨架合计 600 598 B；5 个剥了媒体，§3.5） | **22/22** | **观测**（属性表全等） |
| 工程设置 | 1 `project.binary` | `project.godot`（21 条） | **1/1** | **观测**（1346/1346 字节解完） |
| 扩展声明 | 1 `.gdextension` | 原样 | **1/1** | **观测**（逐字节相同） |
| 插件清单 | 1 `plugin.cfg` | 原样 | **1/1** | **观测**（包内已是文本） |

`MANIFEST.tsv` 的 692 行状态分布：**完整 125 / 部分 31 / 省略 5 / 失败 0 / 非源码 531**。
"部分"的全部原因只有一类（31 行）：`ext_resource` 的 uid 属性被文本写入器丢弃（§3.2）。
"省略"是**主动**的一类（5 行）：内嵌字体/图集媒体按策略剥掉，只留骨架（§3.5）。

### 3.1 `.gdc` → `.gd`：GDRE + **token 级对拍**

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

### 3.2 `.scn` / `.res` → `.tscn` / `.tres`：让 Godot 自己写

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

**已知损失：`ext_resource` 的 `uid` 属性（32 个文件都有；最终 31 行标 `部分`，
1 行因媒体省略改标 `省略`）。** 这是本篇唯一一处
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

### 3.3 22 个 `.res` 的真实构成：17 个是配置，5 个自带媒体

`Godot` 写出来的 `.tres` 共 17.45 MB，其中**16.85 MB 是 5 个文件里的内嵌字节**：

| 文件 | Godot 原始输出 | 内嵌了什么 | 丢弃的载荷 | 骨架 |
|---|---:|---|---:|---:|
| `game_menu_icon_theme.tres` | 6 077 060 B | 26 张 256×256 `Image`（`LumAlpha8` 字形图集） | 5 917 848 B | 159 397 B |
| `button_font.tres` | 3 719 404 B | 18 段（图集 + **真正的 `.ttf` 字节**） | 3 604 764 B | 114 825 B |
| `campaign_button_theme_2.tres` | 2 557 974 B | 11 段（同上） | 2 444 129 B | 114 030 B |
| `campaign_hidden_button_theme.tres` | 2 557 806 B | 11 段 | 2 444 129 B | 113 862 B |
| `campaign_spacer_theme.tres` | 2 542 428 B | 11 段 | 2 444 129 B | 98 484 B |

这些载荷是 `PackedByteArray("....")` 字符串，两种形态：

```
data = {                                    # 纹理图集
"data": PackedByteArray("...."),
"format": "LumAlpha8", "height": 256, "mipmaps": false, "width": 256
}

data = PackedByteArray("....")              # 字体文件本身（base64 头 AAEAAAASAQAA… = sfnt 1.0）
```

**它们是媒体，不是源码**，处理方式见 §3.5。

### 3.4 `project.binary` → `project.godot`

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

### 3.5 媒体策略：这 5 个 `.tres` 落成"只留度量的骨架"

**决策（项目长期策略，用户明确要求过）：仓库本身不提供媒体文件。**
那 16.85 MB 是 `.ttf` 字节和字形图集位图——虽然穿着 base64 文本的外衣，但它是
**导出产物**，不是作者的源文件（作者手上是 `.ttf`，而 `.ttf` 不在包里，见 §4.1）。
所以它们不按"完整文本"入库。

**做法：不是删文件，而是留骨架。** `recover_source.py` 在**两轮验证都通过之后**执行
（这样剥离永远不会掩盖转换缺陷），把 `>= 4096` 字符的 `PackedByteArray` 载荷整段丢掉，
其余原封不动。留下的骨架包含：

* 主题的全部条目表：`Button/styles/normal = SubResource("StyleBoxEmpty_…")`、
  `Button/colors/font_disabled_color = Color(...)`、`default_font = SubResource(...)`；
* 全部样式盒子（`StyleBoxFlat` 的边框宽度/颜色/圆角）；
* 外部引用关系（`ext_resource` 的 type + path）；
* 字体度量：`button_font.tres` 保留 **2 333 条** `<codepoint>/offset|size|uv_rect|texture_idx`。

**丢掉的只有像素与字体字节本身**；图集占位的 `[sub_resource type="Image" …]` 头还在，
所以"这里原本有一张图"是看得见的。

骨架仍然**能被 Godot 加载**（实测，**观测**）：

| 文件 | 加载后类 | 存储属性数 |
|---|---|---:|
| `game_menu_icon_theme.tres` | `Theme` | 20 |
| `button_font.tres` | `FontFile` | **2 333** |
| `campaign_button_theme_2.tres` | `Theme` | 13 |
| `campaign_hidden_button_theme.tres` | `Theme` | 15 |
| `campaign_spacer_theme.tres` | `Theme` | 13 |

代价是加载时会打 `ERROR: Invalid image: image is empty`（`--import` 全树 50 条）。
这是"图集声明还在、像素没了"的必然结果，也是**唯一**能证明骨架身份的现象——不是文本坏了。

**每个骨架自己会说明这件事**（写在 `[gd_resource]` 行之后）：

```
[gd_resource type="FontFile" format=4]
; MEDIA OMITTED: 18 embedded payload(s), 3604764 B of font/atlas data removed.
; This file's original size was 3719404 B. Original text: python3 tools/recover_source.py --include-media
```

`MANIFEST.tsv` 把这 5 行标成 **`省略`**（既不是"完整"也不是"部分"），备注里给出
**原始字节数 / 骨架字节数 / 丢弃段数与字节数**，所以"少了什么、有多大"一眼可见。

**怎么补回来**（本地看，不要提交）：

```bash
python3 tools/recover_source.py --include-media     # 17.45 MB，5 行状态变回"部分"
```

`--check` 的规则（CI 用，已实测能拦住）：

* 标 `省略` 的文件**必须**既没有 `>= 4096` 字符的字面量、又带 `MEDIA OMITTED` 头；
* 树里任何其它 `.tres`/`.tscn` 只要出现 `>= 4096` 字符的字面量 → **FAIL**
  （实测：往 `graphwar_theme.tres` 塞一个 5 000 字符的串，`--check` 立刻报
  `carries an embedded payload … media must not be committed`）；
* `--include-media` 产出的树会得到**警告**而不是失败，因为它本来就不可提交。

---

## 4. 哪些东西**天生**不是文本（不需要回收）

这一节是为了防止后面有人把"回收率 161/692"当成未完成度。

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
   客户端怎么处理见 `docs/121` §5。
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

## 5. 下一步：Rust 侧怎么按模块重建（结构建议，本轮不做）

### 5.1 先把"有多少模块"这件事定死（档位：观测）

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

这个项目里流传着三个不同的"模块数"，**它们量的不是同一件事**，混用会直接算错覆盖率。
先把三个口径并列定死，覆盖率矩阵请明确自己用的是哪一列：

| 口径 | 数值 | 定义与复现 | 该用它做什么 |
|---|---:|---|---|
| `src/graphwar_*.rs` **路径串** | **58** | `.so` 里互不相同的 `src/graphwar_*.rs` 文件名字符串（见下方脚本）。另有 2 个只以目录形态出现的模块名 `graphwar_media` / `graphwar_support`，故 `graphwar_*` 标识符共 **60** | **顶级源文件数**——建骨架、按文件名对账用这个 |
| `fixtures/module_map.tsv` **行数** | **87** | 按 panic 串引用点把函数归堆的产物。其中 `graphwar_*.rs` **56** 行，另 **31** 行是子模块：`entity_drawer.rs`、`effect_drawer.rs`、`math_function.rs`、`button_manager.rs`、`axis_drawer.rs`、`player_showcase.rs` …（不带 `graphwar_` 前缀，因为挂在 `graphwar_media/`、`graphwar_support/` 这类目录下） | **可归堆的模块数**——查某个函数属于哪个文件用这个 |
| `docs/02-module-map.md` 的"约 90 个" | ≈87 | 就是上面那 87 行；正文只列了 56 个 `graphwar_*.rs` | 旧口语口径，**建议改写为"58 个顶级文件 / 87 个可归堆模块"** |

三者不矛盾：**58 是顶级文件，87 是"顶级文件 + 子模块"，"约 90"是 87 的口语化说法。**
本轮脚本、清单、README 一律用 **58**（回收的单位是文件）。
（父任务先前记的"57"应是 56 行 `graphwar_*.rs` 的口误，与 58 差在
`graphwar_media`/`graphwar_support` 这两个只以目录出现的名字上——已按实测更正。）

复现第一条（真·观测）：

```bash
python3 - <<'EOF'
import re
b = open("libgraphwarrust.so","rb").read()
names = {m.decode() for m in re.findall(rb"src/graphwar_[A-Za-z0-9_]+\.rs", b)}
print(len(names)); print("\n".join(sorted(names)))
EOF
```

复现第二条：`tail -n +2 fixtures/module_map.tsv | wc -l` → 87；
`| grep -c '^graphwar_'` → 56；差值 31 就是子模块行数。

### 5.2 用 `src:line` panic 串反推文件规模（档位：观测，且**只是下界**）

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

### 5.3 可执行的重建顺序（建议）

1. **先建骨架，不写逻辑**：按 5.1 的 58 个名字建 `src/graphwar_*.rs`，
   每个文件先只放 `// 观测：字符串存在于 <so sha256> 偏移 X` 和空 `pub fn`。
   这样 `cargo test` 立刻能跑，且"文件名与二进制一致"这条可以被脚本校验。
2. **已逐位验证的 4 个入口先落地**（`docs/02`、`REPORT.md` 已有结论）：
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

## 6. 复现与自检

```bash
# 全量回收（约 25 s：staging 692 文件 + GDRE 3.5 s + 53 次 Godot 0.5 s + 两轮验证 + 媒体剥离）
python3 tools/recover_source.py

# 把 5 个骨架换回带媒体的完整 .tres（17.45 MB，本地看，不要提交）
python3 tools/recover_source.py --include-media

# CI：只检查不重写。校验
#   1) recovered-src/ 里没有白名单之外的文件（=.gitignore 那条硬规矩的可执行版）
#   2) 期望的 161 个输出一个不缺、都不为空
#   3) MANIFEST.tsv 覆盖包里的全部 692 条、且不虚构条目
#   4) project.godot 没丢任何一条设置
#   5) 媒体策略：标"省略"的必须是骨架；其它文件不得含 >=4096 字符的字面量
python3 tools/recover_source.py --check

# 只看计划，不落盘
python3 tools/recover_source.py --list
```

外部工具**不下载、不入库**，只按路径引用（`GDRE_BIN` / `GODOT_BIN` 可覆盖）：
GDRE Tools v2.7.4、Godot 4.7.2 headless。缺工具时脚本给出明确报错，
`--check` 不需要任何工具也能跑。

### 6.1 用 Godot 验证文本没坏（本轮实测结果）

在 `recovered-src/` 的**副本**上跑（避免它在交付目录里生成 `.godot/` 和 `*.gd.uid`
污染白名单）：

```
godot --headless --path <copy> --import      → 退出码 0
```

输出里**没有任何一条** `.tscn`/`.tres` 的解析错误（这是"文本没坏"的直接证据）。
133 行 `ERROR`/`SCRIPT ERROR` 全部落在下面五类里，没有第六类（**观测**，逐行归类可复现）：

| 报错 | 行数 | 原因 |
|---|---:|---|
| `Failed loading resource: res://*.svg` / `*.ttf` | 57（32 个不同文件：30 svg + 2 ttf） | 这些是**导入源**，包里只有 `.ctex`/`.fontdata` 产物（§4.1）。`recovered-src/` 只放文本，故意不放 `.ctex`，所以它们解析不了——这是设计，不是缺陷 |
| `Invalid image: image is empty` | 50 | **骨架的必然产物**（§3.5）：图集 `Image` 的声明还在、像素被剥掉了。只有这 5 个 `.tres` 会打，见到它说明媒体剥离生效 |
| GDExtension 库缺失（`GDExtension dynamic library not found` ×2、`Can't open dynamic library` ×2、`Failed to load script … Parse error` ×2） | 6 | `.gdextension` 里写的是 `res://../rust/target/debug/libgraphwarrust.so`（作者的工作路径），本机没有这个相对路径 |
| `Parse Error: Could not find base class "GraphwarButton"` / `Identifier not declared` | 4 | 上一类的连锁反应：`CampaingLevelsButtons.gd` / `ShowExtraButtons.gd` 继承 Rust 提供的类，库加载失败就没有这个基类 |
| 编辑器环境噪声（`Cannot create editor config/cache directory`、`Error saving editor settings to`（空路径）、`Cannot open directory '…/feature_profiles'`、`Plugin is not attached to debugger` 等） | 16 | `HOME` 被重定向到工作目录，Godot 的编辑器配置落不了盘。与恢复内容无关 |

**所以 `recovered-src/` 按设计不是可运行工程**，它只是"源码层"。
要"能跑"的是 `client/`（另写实现）+ 本机的 `libgraphwarrust.so`。

---

## 7. 档位汇总（哪些是硬结论，哪些还悬着）

| # | 结论 | 档位 | 依据 |
|---|---|---|---|
| 1 | 692 = 105+31+22+1+1+1 + 531，**161** 个曾是文本源码 | **观测** | PCK 尾部索引逐条解析；`--check` 重算 |
| 2 | 158 个 `.remap` 精确分解为 105/31/22，路径映射是满的 | **观测** | `--list` |
| 3 | 105/105 `.gd` 与字节码 **token 流逐个相同** | **逐位验证** | `gdc_verify_source.py`（token 粒度，非文本字节；§0 说明为何这是上限） |
| 4 | 31/31 `.tscn`、22/22 `.tres` 与二进制形态结构/属性全等 | **观测** | SceneState + 属性表对拍，53/53（**在媒体剥离之前**做，所以剥离不会掩盖缺陷） |
| 5 | 32 个文件的 `ext_resource` uid 被文本写入器丢弃 | **观测** | uid_cache 只有 108 条；uid 注册实验；`id_to_text` 逐字符一致 |
| 6 | `project.binary` 21 条、1346/1346 字节解完；type 34 = `PackedStringArray` | **观测** | 解完恰好 EOF |
| 7 | 149 `.svg` / 17 `.png` / 1 `.jpeg` / 7 `.ttf` 是 `.ctex`/`.fontdata` 的来源且不在包内 | **观测** | 每个 `.import` 的 `importer=` 与文件名 |
| 8 | `libgraphwarrust.so` 含 58 个 `src/graphwar_*.rs` 名字 + 2 个目录 | **观测** | 直接字符串扫描（§5.1） |
| 9 | 5 个 `.tres` 内嵌 16.85 MB 媒体；剥成骨架后仍可被 Godot 加载且属性数不变 | **观测** | 骨架加载实测（§3.5 表）；`--check` 拦截测试 |
| 10 | 12 个模块的**文件行数下界**（root_node ≥1302 等） | **观测（下界）** | 54 个 `path:line` 串 |
| 11 | `.gd` 继承 Rust 类（`GraphwarButton` ↔ `graphwar_button.rs`），故 `.gd` 是 Rust 公开 API 的证据 | **观测 → 推断** | 继承串是观测；"调用点 = API 面"是推断，未逐个核对 |
| 12 | 屏幕控制器逻辑全在 Rust（GDScript 侧只有小控件） | **观测** | 105 个 `extends` 统计 + 无 `play_screen` 类脚本 |
| 13 | 模块数到底是 58 / 87 / "约 90" | **观测（口径已定死）** | §5.1 的三行并列 |
| 14 | 46 个模块的文件行数 | **未知** | 没有 `path:line` 串可用，只能靠反编译量函数边界 |

---

## 8. 已知缺口 / 留给后面的事

1. **`recovered-src/` 不带 `.ctex`/`.fontdata`**，因此场景文本里的贴图与字体引用解析不了。
   若以后想要"能跑的复原工程"，正确做法是让 `client/` 的资产管线（`docs/121`）在运行时
   把 `.ctex` 按原路径塞进去，而**不是**把二进制搬进 `recovered-src/`。
2. **5 个骨架 `.tres` 不能当真资源用**：`--import` 会打 50 条
   `Invalid image: image is empty`（§3.5）。要真字体/真图集，用 `--include-media`
   在本地生成完整版（17.45 MB），或直接让 `client/` 从 `.ctex`/`.fontdata` 走原版路径。
   本目录当前 **1.5 MB**（骨架 601 KB），比原来省 16.9 MB。
3. **`addons/admob/plugin.cfg` 已收**（状态"完整"）。白名单为此增加了 `.cfg`；
   另两个 `.cfg`（编辑器缓存）仍不入库，见 `MANIFEST.tsv`。
4. **Rust 侧的"源码面"还没开始回收**：58 个模块名、12 个行数下界、
   87 行 module_map 路由表已经就位，但函数级重建仍要回到 Ghidra（§5.3 的顺序）。
5. **`docs/02-module-map.md` 的"约 90 个模块"应当改口径**（见 §5.1 的三行并列表）。
   本轮未改动该文件（不在本任务写入范围内）。
