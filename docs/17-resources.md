# 17 · 游戏资源（`rsc/`）与音频

> **本篇解决什么问题**：游戏资产是怎么从 `Graphwar.pck` 里取出来的、PCK v4 的格式
> 长什么样、包里到底有什么、为什么**不入库**、以及音频那 10 个产物如何逐个验证、
> 播放链路与音量常量是多少、哪些音效对应哪个事件（**未知**）。
>
> **本篇合并自**：

| 原文件 | 内容 | 归档 |
|---|---|---|
| `17-resources.md` | PCK v4 格式、`rsc/` 的构成、为什么不入库 | 本文 §1（原文保留，未归档） |
| `111-audio.md` | 音频资源清单、损坏判定、播放链路、音量常量 | [`archive/111-audio.md`](archive/111-audio.md) |

客户端的**播放层**（`GwAudio`、总线静音、事件映射状态）在 [`16-client.md`](16-client.md) §9；
客户端如何取资产在 [`16-client.md`](16-client.md) §5。证据标记沿用四档。

## 1. 从 `Graphwar.pck` 解包到 `rsc/`

### 1.1 做了什么

从 `Graphwar.pck` 直接解包到 `rsc/`，**按原始路径**组织，并生成 `rsc/MANIFEST.tsv`
（路径 / 大小 / SHA-256）。

```bash
../.venv/bin/python tools/pck_extract.py --verify
```

实测输出：

```
PCK: Graphwar.pck  godot 4.7.0  pack v4  文件 692 个  file_base=0x70
写出 692 个文件, 共 19.6 MiB, 清单 rsc/MANIFEST.tsv
```

`--verify` 会对每个文件校验 PCK 里记录的 **md5**，692/692 全部相符。

### 1.2 为什么不做格式转换

`.ctex`（压缩纹理）、`.scn`（二进制场景）、`.gdc`（GDScript 字节码）、`.res` 等
**本身就是 Godot 可直接加载的资源格式**。转成 PNG/文本/JSON 会丢信息，
而且转换器自己也需要逆向——等于把问题变大。
所以这里只做"忠实解包"，还原的是**可被 Godot 使用的资源**。

### 1.3 PCK v4 的格式（实测，不是照抄文档）

| 偏移 | 内容 |
|---|---|
| `0x00` | magic `GDPC`（`0x43504447`） |
| `0x04` | pack_version = **4** |
| `0x08/0x0c/0x10` | Godot 版本 4.7.0 |
| `0x14` | pack_flags（本包 = 2） |
| `0x18` | `file_base` = `0x70`（文件内容起点） |
| `0x20` | `directory_offset` = `0x138f440`（**文件索引在包尾**） |
| `0x28..0x68` | 保留 |
| `dir_offset` | `u32 file_count`，随后每项：`u32 path_len`、path（4 字节对齐）、
`u64 offset`、`u64 size`、`u8 md5[16]`、`u32 flags` |

**一个踩过的坑**：我一开始按 v2 解析（索引紧跟头部），得到 `file_count = 0`。
v4 把索引挪到了包尾，头部多了 `directory_offset`。

**另一个坑**：偏移的基准是 `file_base`，**不是包起点**。本包 `pack_flags = 2`，
按 Godot 源码里 `PACK_REL_FILEBASE` 的字面意思应该相对包起点，但我实测
`base = 0` 时 692 个文件 md5 **全部不符**，`base = file_base` 时**全部相符**。
所以代码里以实测为准，并把这段写进注释——**标志位的语义不能想当然**。

### 1.4 资源构成（观测）

| 类型 | 数量 | 说明 |
|---|---|---|
| `.import` | 184 | Godot 导入配置 |
| `.ctex` | 167 | 压缩纹理 |
| `.remap` | 158 | 资源重映射 |
| `.gdc` | 105 | GDScript 字节码（UI 脚本） |
| `.scn` | 31（其中 **6 个是 AdMob 插件场景** ⇒ 游戏场景 **25** 个，`92` §2.3） | 二进制场景 |
| `.res` | 22 | 二进制资源 |
| `.fontdata` / `.mp3str` / `.sample` / `.cfg` | 7 / 6 / 3 / 3 | 字体、音频、配置 |

与 `01` 的结论一致：**玩法逻辑不在这些脚本里**，只有 UI。
后来 `92`/`101` 把这条坐实了：51 个非插件脚本里只有 5 个有行为，且没有一条是游戏规则。

### 1.5 为什么不入库（重要）

`rsc/` 已加入 `.gitignore`。两个理由：

1. 上游 `AGENTS.md` 的硬性规则：**不提交二进制衍生资源**（`*.pck`、`pck/` 等），
   它们体积大、可再生，而且会把游戏发行商的资源带进版本历史；
2. `Cmd-GZ/graphwar2-rebuild` 是公开仓库。把这些资源推上去等于**再分发游戏素材**，
   这是发行商的版权范围，不是我们能替他决定的。

需要时可一键再生（`tools/pck_extract.py`），所以不入库不损失可复现性。

**如果你确认有权提交**（例如仓库改为私有、或仅内部使用），告诉我，我可以：
- 把 `rsc/` 从 `.gitignore` 移除并提交；
- 或改为提交**清单**（`MANIFEST.tsv` 只有路径/大小/哈希，不含内容）。

> 当前状态：**已提交 `MANIFEST.tsv`**（清单不含资源内容），`rsc/` 本体保持本地。


## 2. 音频资源：逐个验证到「逐位」

### 2.1 结论速查（按四档分类）

| # | 结论 | 档位 | 证据 |
|---|---|---|---|
| 1 | PCK 里 **692 个文件**（含 20 个音频相关条目）与其目录表记录的 MD5 **全部相符** | **逐位验证** | `pck_md5_check.py`：`indexed=692 md5-ok=692 md5-MISMATCH=0` |
| 2 | 10 个音频产物**逐个**通过容器账本校验：解析恰好停在 `EOF-4`，末尾正是 `RSRC` 魔数，`slack=0` | **逐位验证** | `audio_audit.py`，`container_report.json` |
| 3 | 6 个 `.mp3str` 的内层 mp3 **帧链正好走到流末尾**（残留 0 字节），Xing 帧数/字节数与 ID3v2 尺寸三方对账一致 | **逐位验证** | §3.1 表 |
| 4 | 3 个 `.sample` 都是 **16-bit / 单声道 / 16000 Hz**，PCM 长度是 2 的整数倍 | **逐位验证** | §3.2 |
| 5 | `humm_b.wav` 的 PCM 与 `humm.wav` 的 `data` 块 **SHA-256 完全相同**（260218 字节） | **逐位验证** | §3.2 |
| 6 | `music.ogg` 的 114 个页的 granule 单调递增，末页 granule 4964955 = 112.584 s @44100；重封装后 ffmpeg 全片解码零告警 | **逐位验证** | §3.3 |
| 7 | **`.oggvorbisstr` 里没有 Ogg 页**（无 `OggS`），存的是 Godot 的 `OggPacketSequence`（包列表）⇒ 任务书里"校验 Ogg 页 CRC"**在构造上不可能**，改做包级校验 | **观测**（见 §2.4） | 裸扫 `OggS` 无命中；容器里是 `packet_data`/`granule_positions` |
| 8 | `humm.wav` 由 Rust 读取并解析（hound 3.5.1） | **观测** | `0x4bf670`…`0x4bf6c3`，§4.3 |
| 9 | 播放用 `AudioStreamPlayer::set_stream` + `play()`；`humm` 用 `AudioStreamGenerator` + `push_buffer` | **观测** | `0x4c01b9/0x4c01d8/0x4c0a5d`，§4.4 |
| 10 | 音量常量：四个 player `-15.0f`，第五个 `-5.0f`；generator `16000.0f` / `0.1f`；PCM 归一化 `32767.0f` | **观测**（立即数） | `0x1bb818`/`0x1b9ba0`/`0x1b93f4`/`0x1bb144`/`0x1b9a58` |
| 11 | 静音 = 整个 `Master` 总线静音，输入是 `arg | self[0xd1]` | **观测** | `0x4c11d0`，§4.5 |
| 12 | `humm.wav` 的 PCM 流入 `push_buffer` 的那一段数据流 | **推断**（缺：解析结果→`push_buffer` 实参的逐指令数据流证明） | §4.4 末尾 |
| 13 | 每个音效对应哪个游戏事件 | **未知**（缺：`0x4bfe10` 内 match 臂的解码 / 运行时观测） | §5.1 |
| 14 | `tear.mp3`、`humm_2.wav`、`humm_b.wav`、`shepard_2.wav` 由谁播放 | **未知**（负结果：全二进制与全部恢复出的 `.gd`/`.tscn` 都没有引用） | §5.2 |
| 15 | `music.ogg` 属于 **AdMob 插件样例**，不是游戏音乐 | **观测** | `addons/admob/assets/music.ogg.import`，§2.5 |

---

### 2.2 资源清单

#### 2.2.1 包内条目（PCK 索引，共 20 条与音频相关）

| 文件 | 字节 | MD5（PCK 目录表记录，已复核） | 说明 |
|---|---|---|---|
| `sounds/humm.wav` | 260380 | `55122336a7e861b1c51d65c42cef7e60` | **唯一留在包里的 WAV 源** |
| `sounds/explosion.mp3.import` | 156 | `dd4f32dee631e2aa418c6073d8a8d480` | 存根 |
| `sounds/squash.mp3.import` | 152 | `a2ad17c2ea1c16f19d34b6741a28e11b` | 存根 |
| `sounds/spark.mp3.import` | 152 | `609483fffb8e460fe72d955e7927f09e` | 存根 |
| `sounds/pop.mp3.import` | 150 | `89ed6e4dd58e4abea9619318cf10e134` | 存根 |
| `sounds/unpop.mp3.import` | 152 | `c3fa55181eae8293aab54e5240302db9` | 存根 |
| `sounds/tear.mp3.import` | 151 | `7f7eb2b32b85310142ff15f4158115ba` | 存根（**代码里无人引用**，§5.2） |
| `sounds/humm_2.wav.import` | 153 | `60929fa32792bcb74b3eadfb67885f69` | 存根 |
| `sounds/humm_b.wav.import` | 152 | `1ce6d9b465ac7f5516c49230f10f3497` | 存根 |
| `sounds/shepard_2.wav.import` | 156 | `1b764beeec628f066349b8cc839f6b70` | 存根 |
| `addons/admob/assets/music.ogg.import` | 172 | （见脚本输出） | **AdMob 样例的**.import |

#### 2.2.2 导入产物（`rsc/.godot/imported/`，10 个）

| 产物 | 字节 | SHA-256（前 16 位） | 资源类 | 头部版本 | 子资源块偏移 |
|---|---|---|---|---|---|
| `explosion.mp3-9407d9….mp3str` | 59855 | `f51ae375fb7f7fa2` | AudioStreamMP3 | 4.2.5 | 288 |
| `pop.mp3-b1aab9….mp3str` | 3131 | `0fbb3dce12c494ff` | AudioStreamMP3 | 4.2.5 | 288 |
| `spark.mp3-32fcd6….mp3str` | 24911 | `ca7303b213c81eb7` | AudioStreamMP3 | 4.2.5 | 288 |
| `squash.mp3-63d23a….mp3str` | 13667 | `21828ab863b2057c` | AudioStreamMP3 | 4.2.5 | 288 |
| `tear.mp3-13b99c….mp3str` | 17767 | `128b2245d76c9d76` | AudioStreamMP3 | 4.2.5 | 288 |
| `unpop.mp3-0c7058….mp3str` | 16175 | `c8ee69d7c9be1a15` | AudioStreamMP3 | 4.2.5 | 288 |
| `humm_2.wav-bbba26….sample` | 113882 | `65e112416d7dba30` | AudioStreamWAV | 4.2.5 | 303 |
| `humm_b.wav-ef692e….sample` | 260594 | `ffd56aea963bf44f` | AudioStreamWAV | 4.2.5 | 303 |
| `shepard_2.wav-f4fd2d….sample` | 640666 | `d31e8bcec9a15b7e` | AudioStreamWAV | 4.2.5 | 303 |
| `music.ogg-6849ed….oggvorbisstr` | 1854570 | `df140331b4c5bd0d` | AudioStreamOggVorbis + OggPacketSequence | 4.2.5 | 411 / 1854501 |

* 完整 SHA-256 与结构化报告：`.agent-work/audio/extracted/container_report.json`。
* 头部版本 `(ver_major, ver_minor, ver_format) = (4,2,5)`：前两个是**导出它的 Godot 版本 4.2.x**，第三个是
  **二进制资源格式版本 5**（`FORMAT_VERSION = 5`，见 Godot 4.2 `core/io/resource_format_binary.cpp`）。
* PCK 头自己写的是 `godot=4.7.0, pack_version=4`（`file_base=112, dir_offset=20509760, 692 files`）⇒
  包是 4.7 打的，里面的导入资源是 4.2.5 写的（两者格式版本 5 兼容）。

#### 2.2.3 源文件 `humm.wav`（父 agent 已验，这里复核一遍）

```
RIFF size=260372 (== 文件长度-8 ✓)
  fmt  16  : PCM(1) ch=1 rate=16000 byterate=32000 blockalign=2 bits=16
  data 260218 @44      ← 与 humm_b.wav.sample 的 PCM 逐字节相同
  LIST 46
  id3  56
块链首尾相接走到文件末尾，无溢出
```

---


## 3. 音频：损坏判定与判据

### 3.1 损坏判定：方法与判据

四道**互相独立**的检查，任何一道失败都能定位到具体字节。

#### 3.1.1 第 1 道：PCK 自带的 MD5 表（最强，且与格式无关）

Godot 4 的 PCK 目录条目是
`u32 path_len | path(pad4) | u64 offset | u64 size | u8 md5[16] | u32 flags`。
所以"文件有没有坏"有一个不依赖任何解析器的答案：文件要么哈希到打包时记录的值，要么不是。

脚本 `pck_md5_check.py`（只读，不重新解包）：

```
indexed=692  md5-ok=692  md5-MISMATCH=0  not-extracted=0
```

**10 个音频产物 + `humm.wav` + 9 个 `.import` + `music.ogg.import` 全部 `ok`。**
⇒ **在"传输/打包"这一层没有任何文件损坏。**

#### 3.1.2 第 2 道：容器账本（能抓出截断、错位、字段被改写）

`godotres.py` 是严格解析器：字段布局逐行对照 Godot 4.2 的 `ResourceLoaderBinary::load()` /
`parse_variant()` / `ResourceFormatSaverBinaryInstance::save()` 写成（含 `_pad_buffer` 的 4 字节对齐、
以及"变体标签用的是**资源格式自己的枚举**而不是 `Variant::Type`"这个坑——`NIL=1, BOOL=2, INT=3,
FLOAT=4, STRING=5, PBA=31`）。

判据（任一不满足即判损坏）：

1. 解析必须**恰好**停在 `EOF-4`；
2. 那 4 字节必须正好是 `RSRC` 结束魔数（`save()` 末尾 `store_buffer("RSRC",4)`）；
3. 每个 `PackedByteArray` 的零填充必须真的是 0，且 `data_start + len + pad` 与下一个字段衔接；
4. 子资源块偏移必须落在文件内，且块里 `type_len` 指向的资源类别与字符串表一致。

结果（10/10 通过）：

```
explosion.mp3str   59855B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
pop.mp3str          3131B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
spark.mp3str       24911B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
squash.mp3str      13667B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
tear.mp3str        17767B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
unpop.mp3str       16175B  block@288   consumed_to_eof=True  end='RSRC'  slack=0
humm_2.sample     113882B  block@303   consumed_to_eof=True  end='RSRC'  slack=0
humm_b.sample     260594B  block@303   consumed_to_eof=True  end='RSRC'  slack=0
shepard_2.sample  640666B  block@303   consumed_to_eof=True  end='RSRC'  slack=0
music.oggvorbisstr 1854570B block@411+1854501  consumed_to_eof=True  end='RSRC'  slack=0
```

#### 3.1.3 第 3 道：mp3 帧链（内层流）

判据（写死的规则，便于以后回归）：

* `ID3v2` 头（若有）声明的 `size` 必须是 syncsafe 且 `10+size(+footer)` 落在流内；
* 从 `ID3v2` 之后开始按帧头同步（`0xFF` + `(b1 & 0xE0)==0xE0`），版本/层/比特率索引/采样率索引必须合法；
* 帧长按 `144*bitrate/sr (+pad)`（MPEG1）、`72*bitrate/sr (+pad)`（MPEG2/2.5 Layer III）累加；
* **最后一帧的末尾必须正好等于内层流长度**（残留 0 字节；若残留是 `TAG`… 则是 ID3v1，另行扣除）；
* 若有 `Xing/Info` 头：`frames` 必须等于"总帧数-1"，`bytes` 必须等于 `ID3v2 之后的帧字节数`。

结果（6/6 通过，全部 `tail=none`，即帧链分毫不差地盖满整个内层流）：

| 文件 | 内层字节 | 帧数 | 帧链覆盖 | 采样率 | MPEG | 声道 | 码率 | Xing 对账 |
|---|---|---|---|---|---|---|---|---|
| explosion | 59520 | 124 | 59520/59520 | 24000 | 2.0 L3 | joint | 160k CBR | 无 Xing；末帧刚好到 EOF |
| pop | 2793 | 7 | 2793/2793 | 44100 | 1.0 L3 | stereo | 64–128k VBR | `frames=6`（=7−Info帧），`bytes=2715`，且 78+2715=2793 ✓ |
| spark | 24576 | 32 | 24576/24576 | 48000 | 1.0 L3 | joint | 256k CBR | 无 Xing |
| squash | 13329 | 32 | 13329/13329 | 44100 | 1.0 L3 | stereo | 64–128k VBR | `frames=31`，`bytes=13164`，且 165+13164=13329 ✓ |
| tear | 17430 | 42 | 17430/17430 | 44100 | 1.0 L3 | stereo | 64–128k VBR | `frames=41`，`bytes=17344`，且 86+17344=17430 ✓ |
| unpop | 15840 | 33 | 15840/15840 | 24000 | 2.0 L3 | joint | 160k CBR | 无 Xing |

> 注：尾部的 `0x55`/`0xAA` 长串**在最后一帧内部**（编码器的填充/ancillary 数据），不是帧外的垃圾；
> 帧链正好到 EOF 就证明了这一点。

外部解码器复核（`run_external_checks.sh`）：6 个 mp3 用 **ffmpeg** 与 **mpg123** 各解一遍，
两者都**零告警**（`ALL EXTERNAL CHECKS PASSED`）。

#### 3.1.4 第 4 道：内层流语义（PCM / Vorbis 包）

##### `.sample`（AudioStreamWAV）

判据：`data_len % (bytes_per_sample × channels) == 0`；`format`/`mix_rate` 必须是文件里真实存的属性值；
再加一条"有没有被截断"的旁证（PCM 统计：min/max、不同取值数、首尾样本）。

| 文件 | format | mix_rate | stereo | data 字节 | 样本数 | 时长 | PCM 统计 |
|---|---|---|---|---|---|---|---|
| humm_2 | 1 = 16-bit | 16000 | false(缺省) | 113506 | 56753 | 3.5471 s | min −23483 / max 22749 / 26490 个不同值 |
| humm_b | 1 = 16-bit | 16000 | false | 260218 | 130109 | 8.1318 s | min −20249 / max 22888 / 29491 个不同值 |
| shepard_2 | 1 = 16-bit | 16000 | false | 640292 | 320146 | 20.0091 s | min −10446 / max 12692 / 20511 个不同值 |

* 三者 `data_len` 都是 2 的倍数，`%2==0` ✓；
* 三者用 ffmpeg（外面套一个 RIFF 头，PCM s16le mono）解码**零告警**：`ffmpeg rc=0`；
* **`humm_b` 与 `humm.wav` 的 `data` 块逐字节相同**：
  `sha256 = 4143c0f915bed79a7fdb638750a7b429…`（260218 字节，两边一致）⇒ 这一份**不可能**是坏的那个，
  而且说明包里同时存在"同一段音频的源 WAV 与导入 PCM"。

##### `.oggvorbisstr`（AudioStreamOggVorbis + OggPacketSequence）

**任务书的假设要修正**：这个产物里**没有 Ogg 页链**。裸扫整个文件没有 `OggS`；容器里是
`OggPacketSequence{ packet_data: Array[114] of Array[包], granule_positions: PackedInt64Array[114],
sampling_rate: 44100.0 }` ⇒ Godot 在导入阶段就把页**拆成了包**。
所以"逐页 CRC"在构造上不可能，改为包级校验 + 重封装后解码：

| 检查 | 结果 |
|---|---|
| 页数 / 包数 | 114 页 / 6920 包（其中 3 个是头包，6917 个音频包；空音频包 0） |
| `granule_positions` 数量 | 114 = 页数 ✓ |
| granule 单调 | 单调不减 ✓（首两页 0,0；末页 4964955） |
| 末 granule ⇒ 时长 | 4964955 / 44100 = **112.584 s**，与 ffmpeg 报的 `duration=112.584014` 一致 |
| 识别头包 | `\x01vorbis`，version=0，channels=2，rate=44100，bitrate_nominal=160000，blocksizes=184 ⇒ 256/2048，framing=1 ✓ |
| 注释头包 | `\x03vorbis`，vendor `Lavf58.45.100`，`encoder=Lavc58.91.100 libvorbis`，`title=Sour by Safe Soundz (Lofi)`，framing=1 ✓ |
| 建立头包 | `\x05vorbis`，4225 字节 |
| 重封装 + 全片解码 | 用 `audio_audit.py` 按包重建 114 个页（页 CRC 由脚本自己算，因为原产物里没有页），`ffmpeg -i remux.ogg -f null -` **零告警，rc=0**，`ffprobe` 报 `duration=112.584014` |

> 只把"建立头最后一位 = framing bit"当判据是**不成立**的（framing bit 之后可能还有填充比特），
> 所以这一条不作为问题项；真正的判据是"能解码"。

#### 3.1.5 `music.ogg` 的归属（顺手纠正）

`music.ogg` 的存根不在 `sounds/`，而在 **`addons/admob/assets/music.ogg.import`**
（`uid://f3f176vu6pfo`，`importer="oggvorbisstr"`）。恢复出的 `addons/admob/sample/MobileAds.gd` 里有
`music.playing = not set_mute_music.button_pressed` ⇒ 这是 **AdMob 插件样例工程**的音乐，
与游戏玩法音频无关（这也解释了它为什么带版权注释字段）。

#### 3.1.6 结论

**在可校验的 10 个产物（以及 `humm.wav` 源）里，没有一个是损坏的。**
判定依据是上面四道独立检查，最强的一道是 PCK 自带的 MD5 表（692/692 相符）。

所以"用户口述 `.pck` 里有一个文件是损坏的"这句，只有三种可能，都需要包外证据：

1. **损坏的是被导出剥掉的源资源**（`explosion/squash/spark/pop/unpop/tear.mp3`、`humm_2/humm_b/shepard_2.wav`
   的**源文件**都不在包里）——这部分**无法从包内验证**（未知）；
2. **损坏发生在导出之后**（例如某个工具重新打包/编辑过某个文件）——但 MD5 与包一致，所以"包内的字节
   就是打包时的字节"；要判"内容坏了"只能靠**语义**判据，而 10 个产物的语义检查全过；
3. 用户记的是**别的类型**的文件（纹理/场景/字体），不在本条线范围内。

#### 3.1.7 顺带否证一条误导

`.so` 里的 `corrupt deflate stream`（`0x1c62ab`）**不属于音频**：它的邻居是
`/home/lucas/.cargo/registry/.../miniz_oxide-0.7.4/src/deflate/core.rs`（`0x1a755a`），
是 Godot 绑定解压缩（纹理/压缩资源）用的。音频的 WAV 错误串在另一处，属于
**hound 3.5.1**（crate 路径 `/home/lucas/.cargo/registry/.../hound-3.5.1/src/read.rs`，`0x1a1b66`）。

---

### 3.2 判定标准（写成规则，方便以后回归）

```
MP3 坏 ⟺ ① ID3v2 size 越界；② 某帧头非法（版本/层/码率/采样率索引保留值）；
        ③ 某帧长度越过内层流末尾；④ 帧链停止处 ≠ 内层流长度（且残留不是 ID3v1/APE 的合法尾巴）；
        ⑤ Xing.frames/bytes 与实际不符。
WAV(sample) 坏 ⟺ ① data 长度不是 frame(2B) 的整数倍；② format/mix_rate 非法；
        ③ 容器账本不到 EOF；④ 已知参考时逐字节不等。
VORBIS 坏 ⟺ ① 三个头包魔数/版本/framing 不对；② granule 数量 ≠ 页数或出现下降；
        ③ 重封装后解码器报错。
容器坏 ⟺ 解析停点 ≠ EOF-4，或末尾魔数 ≠ 'RSRC'，或填充非 0。
```

---


## 4. 播放链路与音量常量

### 4.1 播放链路

#### 4.1.1 模块定位

`src/graphwar_audio_control.rs` 的位置串在 `.rodata`：`0x1c2ca2`（**:241:17**，一个 `dbg!` 风格日志）
与 `0x1c2cdb`；另一个在 `0x1b5daf`。带该位置串的 7 个函数就是任务书给的
`0x3bfdd0 / 0x4bef20 / 0x4bf090 / 0x4bf550 / 0x4bfe10 / 0x4c11d0 / 0x5238c0`
（`fixtures/module_map.tsv` 记的也是这 7 个，`graphwar_audio_control.rs 7 17 0x3bfdd0 0x5238c0`）。
**注意**：这 7 个只是"带 panic 位置串"的函数，模块里还有不带 panic 的辅助函数
（例如 `0x4bed70`、`0x4bf340`、`0x4bf480`、`0x4bf000`，见下）。

节点类名是 **`SoundHandler`**（`.rodata 0x1adeab`，唯一 xref 在 gdext 类注册函数 `0x73afa0`）。

#### 4.1.2 谁在调用（调用者已闭合）

| 函数 | 作用（判定依据见后） | 直接调用者 |
|---|---|---|
| `0x4bf550` | 载入资源（5 mp3 + humm.wav 解析） | `0x3a2e10`（call site `0x3a439c`） |
| `0x4bfe10` | 播放 / 装 stream / 推 PCM 的主流程 | `0x3a5760`（call site `0x3a6b67`，`rsi` = 事件数据） |
| `0x4c11d0` | 静音设置 | `0x3a2e10`（`0x3a4413`）、`0x3b0830`（`0x3b179b`） |
| `0x4bf090` | 取/建 player 并按分支设 stream + 音量 | 无直接调用者（Rust vtable/函数指针派发，未更进一步） |
| `0x4bf480` | 另建 player（-5.0 dB） | 同上 |
| `0x4bef20` | 小函数（224 B），无直接调用者 | 同上 |
| `0x5238c0` | 由 `0x4bf550` 调用（`0x4bf9e0`）的收尾/构造 | `0x4bf550` |

> 反汇编里大量 `call *0xNNN(%rip)` 是经 `.data.rel.ro` 槽位的间接调用（GOT 槽 → `R_X86_64_RELATIVE`
> + addend），`relocs.py` 负责把槽位还原成真实目标地址。只看"直接 call"会漏掉 audio 的整条链路。

#### 4.1.3 `humm.wav` 是谁读的（**证实**：Rust 侧自己读 + hound 解析）

`0x4bf550` 里，前 5 个资源走 Godot `load()`，第 6 个（`humm.wav`）走了一条**完全不同的路**：

```asm
4bf670  lea  -0x2fc9ec(%rip),%rsi   # 1c2c8b  "res://sounds/humm.wav"
4bf677  lea  0x70(%rsp),%rdi
4bf67c  mov  $0x15,%edx                      ; 21 = strlen("res://sounds/humm.wav")
4bf681  call 3eaa00                          ; FileAccess::get_file_as_bytes(path, len) -> bytes
4bf686  lea  0x70(%rsp),%rdi
4bf68b  call *0x9ba918                       ; 槽位-> 0x89eec0（取 Result/Option 的状态）
4bf697  test %al,%al
4bf699  je   4bf808                         ; 读不到就跳过（不 panic）
4bf6a8  lea  0x70(%rsp),%rdi
4bf6ad  call *0x9ba920                       ; 槽位-> 0x89ee90（取出 &[u8]）
4bf6b8  mov  %r15,%rdi   ; 输出
4bf6bb  mov  %r12,%rsi   ; 1
4bf6be  mov  0x8(%rsp),%rdx
4bf6c3  call 50a080                          ; ★ hound WAV 解析
4bf6c8  movzbl 0x68(%rsp),%eax
4bf6cd  cmp  $0x2,%al
4bf6d9  jne  4bf85e                          ; 非 Ok 走错误分支
```

* `0x3eaa00` 被证实是 `FileAccess::get_file_as_bytes`：它引用的类/方法名字符串簇是
  `0x1bfbac` = `…FileAccess store_string get_file_as_bytes ImageTexture…`，且 `get_file_as_bytes`
  的具体偏移 `0x1bfbf2` 就在其中，函数体里 `0x3eaa6d` / `0x3eabfc` 两处 `lea` 指向它。
* `0x50a080`（2704 B）是 WAV 解析器：它包含 `missing fmt chunk` / `no WAVE tag found` /
  `no RIFF tag found` / `invalid data chunk length` / `data chunk length is not a multiple of sample size`
  等串，并调用 `0x509a90`（`bits per sample is not 8 or 16` 等校验）；crate 路径是 **hound-3.5.1**。
* `0x4bf7de` 处 `lea 1c3b13` + `lea 95f910`(=0x4da4d0) + `lea 95ebc0`(=`0x1b5daf` =
  `src/graphwar_audio_control.rs`) ⇒ 解析结果做了 `Result::unwrap()`，panic 位置就在该模块。

⇒ **结论（观测）**：`humm.wav` 由 **Rust** 通过 `FileAccess::get_file_as_bytes` 读取（**打开/读取调用点
`0x4bf681`**，函数 `0x3eaa00`），再由 **hound**（函数 `0x50a080`，调用点 `0x4bf6c3`）解析。
这条路径**不经过 Godot 的资源导入系统**，所以 `humm.wav` 的**源文件必须搬进包**
——这正好解释了"9 个音效只剩 `.import` 存根，唯独 `humm.wav` 源文件还在包里"。

#### 4.1.4 播放：`AudioStreamPlayer` 一次性的 + `AudioStreamGenerator` 流式的

**（a）五个 mp3：`load()` 后 `set_stream` + `play()`**

`0x4bf550` 里五次 `call 0x4b1da0`（每次前面 `lea <路径>; mov $len`），路径依次是：

| 顺序 | 路径（`.rodata`） | 长度立即数 | 调用点 |
|---|---|---|---|
| 1 | `res://sounds/explosion.mp3` `0x1c2c1a` | `$0x1a`=26 | `0x4bf58b` |
| 2 | `res://sounds/squash.mp3` `0x1c2c34` | `$0x17`=23 | `0x4bf5b2` |
| 3 | `res://sounds/spark.mp3` `0x1c2c4b` | `$0x16`=22 | `0x4bf5e3` |
| 4 | `res://sounds/pop.mp3` `0x1c2c61` | `$0x14`=20 | `0x4bf624` |
| 5 | `res://sounds/unpop.mp3` `0x1c2c75` | `$0x16`=22 | `0x4bf655` |

> 这些串在 `.rodata` 里**不是 NUL 结尾**（Rust 传 `&'static str` + 长度），
> 例如 `0x1c2c1a` 处的原始字节就是
> `res://sounds/explosion.mp3res://sounds/squash.mp3…`。写脚本时必须用长度截断，`strings` 会骗人。

播放侧（`0x4bfe10`，被 `0x3a5760` 以事件数据调用）：

```asm
4c01b4  lea  0x20(%rsp),%rsi            ; generator 流
4c01b9  call 3ebb80                     ; AudioStreamPlayer::set_stream
4c01d0  xorps %xmm0,%xmm0               ; 参数 0
4c01d8  call *0x9bc0f0                  ; 槽位-> 0x8ae4c0 = AudioStreamPlayer::play()
...
4c0cb4  xorps %xmm0,%xmm0
4c0cba  call *0x9bc0f0                  ; 另一处 play()（在 player 池的循环里）
4c0cc0  movb $0x1,0x14(%r15)            ; 标记该 player 已启动
```

* `0x8ae4c0` 被证实是 `AudioStreamPlayer::play`：它 `lea 0x29f005`（该簇含
  `AudioStreamPlayer set_volume_db get_stream_playback…`）并 `lea 0x1b97b4`（4 字节 = `play`）。
* `0x3eb8b0` / `0x3ebb80` 是 `AudioStreamPlayer::set_stream` 的两份绑定代码（各自引用
  `0x1bfc3e`=`AudioStreamPlayer`、`0x1bfc4f`=`set_stream`）；`0x4bed70` 是包住前者的辅助函数
  （签名：`out, ?, ?, player_ref, stream_ref, ?id, volume_db`）。

**（b）`humm`：`AudioStreamGenerator`（16000 Hz / 0.1 s）+ `push_buffer`**

```asm
4c0165  lea  0x20(%rsp),%rdi
4c016a  movss 0x1b93f4                  ; = 16000.0f   (00 00 7a 46)
4c0172  call *0x9bc0e0                  ; -> 0x89dd40 = AudioStreamGenerator::set_mix_rate
4c018f  movss 0x1bb144                  ; = 0.1f       (cd cc cc 3d)
4c0197  call *0x9bc0e8                  ; -> 0x89dff0 = AudioStreamGenerator::set_buffer_length
4c01b9  call 3ebb80                     ; set_stream(generator)
4c01d8  call *0x9bc0f0                  ; play()
```

类名字符串簇 `0x29e200` 明确写着
`AudioServer set_bus_mute … AudioStreamGenerator set_mix_rate set_buffer_length set_flip_h set_flip_v`。

推 PCM 的位置（同一函数）：

```asm
4c05d3  call *0x9bc100                  ; -> 0x8ba840 = AudioStreamGeneratorPlayback::get_frames_available()
4c08f6  movss 0x1b9a58                  ; = 32767.0f   (i16 满量程归一化)
4c090e / 4c094d  movss 0x1b952c         ; = 1.0f
4c082e  movss 0x1ba2b4                  ; = 2.0f
4c0a5d  call *0x9bc118                  ; -> 0x8ba630 = AudioStreamGeneratorPlayback::push_buffer()
```

* `0x8ba630` / `0x8ba840` 由字符串簇 `0x29f6e4`
  （`…AudioStreamGeneratorPlayback push_buffer get_frames_available…`）证实。
* `0x4bf550` 里解析完 humm 后紧跟 `call 0x3bfdd0`（`0x4bf8d2`，把 Godot 容器里的 2 字节样本
  摊平成 `Vec<u16>`/`Vec<i16>`：分配 `2*n+2` 字节、`mov %bx,(%rax)` 逐元素 2 字节写入）
  与 `call 0x5238c0`（`0x4bf9e0`）。

**推断（缺什么）**：`humm.wav` 的 PCM → `push_buffer` 的**数据流**没有被逐指令证明。
支撑是三条独立事实：① 采样率对得上（WAV `fmt` 里 16000 Hz，generator `set_mix_rate(16000.0f)`）；
② 归一化常量 `32767.0f` 正是 i16 满量程；③ 整个二进制里只有这一处读 WAV、只有这一处用
`AudioStreamGenerator`。缺的证据：从 `0x4bf8d2`（解析结果）到 `0x4c0a5d`（`push_buffer` 实参）的
寄存器/内存传递链，或一次运行时 hook。

#### 4.1.5 音量与静音

**音量是逐 player 的静态常量，不是玩家可调项**：

| player 槽位 | 音量立即数 | 站点 |
|---|---|---|
| self+0xa0（match 臂 1） | `movss 0x1bb818` = **-15.0f**（`00 00 70 c1`） | `0x4bf199` → `call 0x4bed70` @`0x4bf1aa` |
| self+0xa0（match 臂 2） | **-15.0f** | `0x4bf1e6` → `0x4bf1f7` |
| self+0x80 | **-15.0f** | `0x4bf233` → `0x4bf244` |
| self+0x90 | **-15.0f** | `0x4bf279` → `0x4bf28a` |
| self+0xb0（`0x4bf340`） | （未在这一支里设音量） | `0x4bf39e` |
| self+0xc0（`0x4bf480`） | `movss 0x1b9ba0` = **-5.0f**（`00 00 a0 c0`） | `0x4bf4cf` → `0x4bf4e3` |

`0x4bf090` 的四个臂都以 `xor %r15d,%r15d; xor %r12d,%r12d` 开头、且都读 `0x1c(%r14)` 作为分支依据，
是"按音效种类选 player"的 match：4 个臂落到 3 个槽（`0xa0`×2、`0x80`、`0x90`），
另外两支分别落到 `0xb0`、`0xc0` ⇒ 共 **5 个 player 槽**（`0x80/0x90/0xa0/0xb0/0xc0`）。
`AudioStreamPlayer::set_volume_db` 的绑定在 `0x8adf10`（字符串簇 `0x29f016`）。

**静音（`res://mute.svg` 那条路）**：`0x4c11d0(self, mute: bool)`

```asm
4c11dc  mov  %esi,%ebp
4c11de  mov  %bpl,0xd0(%rdi)            ; self.muted = 参数
4c135e  movzbl 0xd1(%rbx),%r15d         ; 另一个标志位
4c1366  or   %bpl,%r15b                 ; 有效静音 = arg | self[0xd1]
4c1395  lea  0x1c2bdc                   ; "Master"（长度 $0x6 在 edx）
4c13a4  call 4dd070                     ; AudioServer::get_bus_index("Master") -> eax
4c13bb  movzbl %r15b,%edx ; and $0x1,%edx
4c13c5  mov  %ebp,%esi                  ; bus
4c13c7  call *0x9ba9a8                  ; -> 0x89d7a0 = AudioServer::set_bus_mute(bus, mute)
```

⇒ 静音是**整条 `Master` 总线静音**（不是逐 player 调 `volume_db`），
有效值 = 参数 **或** `self+0xd1` 那个标志（"另一个静音源"的语义未确定，见 §5.3）。
`0x4dd070`（`get_bus_index`）另有调用者 `0x3a5760`（`0x3a7e8d`）与 `0x4bf000`（`0x4bf053`），
说明主线里还有别处读写总线音量——**未展开**。

#### 4.1.6 一次性的入口顺序（观测）

`0x3a2e10` → `0x4bf550`（载资源，`0x3a439c`）→ … → `0x3a2e10` 里再 `0x4c11d0`（静音初始化，`0x3a4413`）。
之后每帧/每事件由 `0x3a5760` → `0x4bfe10`（`0x3a6b67`）驱动播放与推缓冲。

---


## 5. 仍未确定（写明缺什么）

### 5.1 仍未确定（写明缺什么）

#### 5.1.1 音效 ↔ 游戏事件（**未知**）

`0x4bfe10` 是"按事件播放"的主函数（第二参数 `rsi` 是事件数据；入口 `cmpq $0x0,0x1e0(%rsi)`），
它内部对事件分支、每个分支选 player 槽并 `set_stream`/`play`。要把"哪个事件放哪个音"钉死，需要：

* 解出 `0x4bfe10` 的 match：判别值加载点 + 跳转表 + 每个臂落到哪个 player 槽；
* 或者运行时 hook：在 `0x4c01b9`/`0x4c0cba` 打印 player 指针与 stream 指针，在 `0x8ae4c0` 打印调用序。

现有材料只够支撑"名字与加载顺序"的对应（explosion→squash→spark→pop→unpop 依次 load），
以及"5 个 player 槽 + 4×(-15 dB) + 1×(-5 dB)"的结构。**不要**把"名字像"写成结论。

#### 5.1.2 `tear.mp3` / `humm_2` / `humm_b` / `shepard_2` 由谁播放（**未知**，且是"负结果"）

* 全二进制的 `res://sounds/` 字符串**只有 6 条**（上述 5 个 mp3 + humm.wav），没有 `tear.mp3`；
* `tear.mp3.import` 的 `uid://ct7fmgd1ocr7i` 在恢复出的 **105 个 `.gd` + 31 个 `.tscn`**
  里**没有任何引用**（同样地 `humm_2`/`humm_b`/`shepard_2` 的 uid 也没有）；
* 6 次 `set_stream` 调用（`0x4bf090`×4、`0x4bf340`×1、`0x4bf480`×1）对应的流全部来自
  `0x4bf550` 里那 5 次 `load()` + 1 个 generator ⇒ **`tear.mp3` 的产物在代码里是孤儿**。

结论只能是"未观测到引用"。要判"是不是死资源"，还需要：全 GDScript 字节码（`.gdc`）的**字符串常量**里
再扫一遍 uid（现有恢复物是反编译结果，可能有损）——这一步没做。

#### 5.1.3 `self+0xd1` 那个静音源（**未知**）

`0x4c11d0` 里 `or self[0xd1]` 参与了最终 `set_bus_mute` 的取值，但 `0xd1` 由谁写、语义是什么
（"没有音频设备"？"music 静音"？"全局开关"？）没有找到写点。需要：全二进制里对 `+0xd1` 的写指令扫描，
或运行时观测该字节的变化。

#### 5.1.4 未展开但已知存在的旁路

* `0x4bf000` 也调用 `AudioServer::get_bus_index`（`0x4bf053`），可能是总线音量设置；
* `0x3a5760`（20288 字节）同时接触 `get_bus_index` 与播放主流程，是上层"事件/状态驱动者"，
  它的完整语义没解；
* `0x4bef20`（224 B，无直接调用者）与 `0x5238c0`（864 B）的职责只到"参与了载入阶段的收尾"。
* Vorbis **建立头（setup header）没有做完整语法解析**（只验了魔数与"能解码"）⇒ 若将来要做
  "逐位复刻 vorbis 解析"，这里还是空白。

---


## 6. 对任务书三处前提的修正

### 6.1 对任务书三处前提的修正

1. **`.oggvorbisstr` 不是 Ogg 页链**，是 Godot 拆好的 `OggPacketSequence`（无 `OggS`）⇒ 页 CRC 无法校验，
   改用"头包 + granule + 重封装解码"（§2.4）。**这不是损坏**，是导入格式本来如此。
2. **`music.ogg` 不是游戏音乐**，是 `addons/admob/` 样例工程的素材（§2.5）。
3. **`.so` 里的 `corrupt deflate stream` 不属于音频**，是 miniz_oxide（§2.7）；
   音频的 WAV 错误串属于 **hound-3.5.1**。

