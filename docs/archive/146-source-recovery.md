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

---

## 1. 结论（一句话）

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
| 资源 | 22 `.res` | 22 `.tres`（17 479 515 B，见 §3.3） | **22/22** | **观测**（属性表全等） |
| 工程设置 | 1 `project.binary` | `project.godot`（21 条） | **1/1** | **观测**（1346/1346 字节解完） |
| 扩展声明 | 1 `.gdextension` | 原样 | **1/1** | **观测**（逐字节相同） |

`MANIFEST.tsv` 的 692 行状态分布：**完整 124 / 部分 36 / 失败 0 / 非源码 532**。
"部分"的全部原因只有两类，且都写在每一行的备注里：`ext_resource` 的 uid 属性丢失（32 行，
§3.2）、文本里内嵌了二进制图集（5 行，§3.3）。

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

### 3.3 22 个 `.res` 的真实构成：17 个是配置，5 个是"文本化的二进制"

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

---

## 4. 哪些东西**天生**不是文本（不需要回收）

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

顺便修正两个流传的数字（**未知 → 观测**）：

* 父任务给的"57 个模块"、`docs/02` 写的"约 90 个" **都不是这个量的定义**。
  实测口径有三个，别混：
  - `src/graphwar_*.rs` 路径串：**58**
  - `fixtures/module_map.tsv` 里按引用归堆出的模块行：**87**
    （其中 `graphwar_*.rs` 56 行 + 31 行是 `entity_drawer.rs` / `effect_drawer.rs` /
    `math_function.rs` / `button_manager.rs` / `axis_drawer.rs` / `player_showcase.rs`
    这类**子模块**，它们不是 `graphwar_` 前缀）
  - 两者并不矛盾：58 是"顶级文件"，87 是"能归堆的文件 + 子模块"。
    后续写文档时请分开表述。

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
# 全量回收（约 30 s：staging 692 文件 + GDRE 3.5 s + 53 次 Godot 0.5 s + 两轮验证）
python3 tools/recover_source.py

# CI：只检查不重写。校验
#   1) recovered-src/ 里没有白名单之外的文件（=.gitignore 那条硬规矩的可执行版）
#   2) 期望的 160 个输出一个不缺、都不为空
#   3) MANIFEST.tsv 覆盖包里的全部 692 条、且不虚构条目
#   4) project.godot 没丢任何一条设置
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
80 行 `ERROR`/`SCRIPT ERROR` 全部落在下面四类里，没有第五类（**观测**，逐行归类可复现）：

| 报错 | 行数 | 原因 |
|---|---:|---|
| `Failed loading resource: res://*.svg` / `*.ttf` | 57（32 个不同文件） | 这些是**导入源**，包里只有 `.ctex`/`.fontdata` 产物（§4.1）。`recovered-src/` 只放文本，故意不放 `.ctex`，所以它们解析不了——这是设计，不是缺陷 |
| GDExtension 库缺失（`GDExtension dynamic library not found` ×2、`Can't open dynamic library` ×2、`Failed to load script ... Parse error` ×2） | 6 | `.gdextension` 里写的是 `res://../rust/target/debug/libgraphwarrust.so`（作者的工作路径），本机没有这个相对路径 |
| `Parse Error: Could not find base class "GraphwarButton"` / `Identifier not declared` | 4 | 上一类的连锁反应：`CampaingLevelsButtons.gd` / `ShowExtraButtons.gd` 继承 Rust 提供的类，库加载失败就没有这个基类 |
| 编辑器环境噪声（`Cannot create editor config/cache directory`、`Error saving editor settings to`（空路径）、`Plugin is not attached to debugger` 等） | 13 | `HOME` 被重定向到工作目录，Godot 的编辑器配置落不了盘。与恢复内容无关 |

**所以 `recovered-src/` 按设计不是可运行工程**，它只是"源码层"。
要"能跑"的是 `client/`（另写实现）+ 本机的 `libgraphwarrust.so`。

---

## 7. 档位汇总（哪些是硬结论，哪些还悬着）

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

## 8. 已知缺口 / 留给后面的事

1. **`recovered-src/` 不带 `.ctex`/`.fontdata`**，因此场景文本里的贴图与字体引用解析不了。
   若以后想要"能跑的复原工程"，正确做法是让 `client/` 的资产管线（`docs/121`）在运行时
   把 `.ctex` 按原路径塞进去，而**不是**把二进制搬进 `recovered-src/`。
2. **`.cfg` 里的 `addons/admob/plugin.cfg` 是可回收源码**，但目前被扩展名白名单挡在外面。
   要收就扩展白名单 + 改 `--check`，不要手工塞。
3. **17.2 MB 的内嵌图集 `.tres`**（§3.3）占了本目录 95% 的体积。若要瘦身，
   建议连 `MANIFEST.tsv` 一起重新生成并明确标注"刻意不含内嵌图集"，
   而不是只 `rm` 文件（会让 `--check` 报缺）。
4. **Rust 侧的"源码面"还没开始回收**：58 个模块名、12 个行数下界、
   87 行 module_map 路由表已经就位，但函数级重建仍要回到 Ghidra（§5.3 的顺序）。
5. **`docs/02-module-map.md` 的"约 90 个模块"应当改口径**（见 §5.1）。
   本轮未改动该文件（不在本任务写入范围内）。
