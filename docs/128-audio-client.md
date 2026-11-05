# 128 · 把音频接进客户端（播放层 / 音量 / 静音 / 映射状态）

状态：**现行**。本文回答一件事：`docs/111` 已经把原版音频链路解完了，这一轮**把它接进
`client/`**，并且把"哪些是原版证据、哪些是我们自己的选择"分开写清楚。

> 前置：**`docs/111-audio.md`（本任务的权威依据）**、`docs/125-playscreen.md`（客户端结构）、
> `docs/118-campaign-data.md` §3.2（`player.config` 格式）、`docs/114-event-tags.md`（事件 tag）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`（按任务边界，索引更新由父 agent 做）。

证据分级沿用 `AGENTS.md`：**逐位验证** / **观测**（能指到地址或字节）/ **推断**（写明缺什么）/ **未知**。

---

## 0. 一页结论

| 项 | 结果 | 等级 |
|---|---|---|
| 播放层 | `client/src/audio/gw_audio.gd`（`GwAudio`，`App` 的子节点，`App.audio`）；10 个音效全部真的能出声 | **观测**（捕获到非零 PCM） |
| 资产 | 包里的 `.mp3str` / `.sample` 在本机还原成 `client/assets/sounds/*.mp3|*.wav`；**一个媒体文件都不入库** | **观测**（`git check-ignore` 命中） |
| 音量 | 逐 player 的静态常量：`0x80/0x90/0xa0 = -15.0 dB`、`0xc0 = -5.0 dB`、`0xb0` 未设置（默认 0 dB） | **观测**（`docs/111` §4.5 + 本轮实测峰值比 0.1778 = 10^(-15/20)） |
| 静音 | `AudioServer.set_bus_mute(get_bus_index("Master"), …)`，**不是**逐 player 调音量；`store.muted` ↔ 总线**双向**同步 | **观测**（`0x4c13a4/0x4c13c7`）+ 实测总线峰值 -200 dB |
| 事件↔音效映射 | **0 条确定**。本轮把"未知"缩小了一步（见 §3），但仍证不到"哪个事件放哪个音" | **未知** |
| 客户端接线 | 3 条**临时的**、代码里带 `PENDING_MAPPING` 标记、文档里标"**自定，非原版**" | **自定**（不是原版行为） |
| 自检 | **181 passed, 0 failed**（基线 146 + 新增 35）；无音频资产时 **175 passed, 0 failed, 10 skipped**（逐条报原因） | **观测** |
| 端到端 | 真开一局 → 按 `Fire!` → `sfx_played=["explosion@0xa0"]`，抓到 32768 帧、峰值 0.20081，附 WAV/PNG 证据 | **观测** |

**一句话**：播放链路（含 `humm` 的 `AudioStreamGenerator` 那条路）已经通了、静音走的是原版的
Master 总线、音量用的是原版实测值；**唯一没解决的是原版最核心的那件事 —— 哪个事件放哪个音**，
而这一点 `docs/111` §5.1 本来就判了未知，本轮**没有**编一个出来。

---

## 1. 交付物（逐文件）

| 文件 | 改动 | 说明 |
|---|---|---|
| `client/src/audio/gw_audio.gd` | **新增** | 播放层 `GwAudio`。加载/播放/槽位音量/静音/`humm` 的 generator 推流 |
| `client/src/audio/extract_audio.py` | **新增** | 资产准备：从 `Graphwar.pck` 解出 10 个音频（详见 §2） |
| `client/src/app/app.gd` | 改 | 建 `App.audio`；`TOGGLE_MUTE` 改为真静音；`FIRE` / 对局结果分发到播放层；**总线→store 的每帧同步** |
| `client/src/app/store.gd` | 改 | `muted` 字段（落盘到我们自己的 `gw2_client.cfg`）；原版 `player.config` 的**只读**容错解析 |
| `client/tools/selftest.gd` | **只加断言**（+35 条） | 表结构 / 音量常数 / 静音双向 / 缺资产降级 / `player.config` 格式 / 真的出声 |

**没有**碰：`play_screen.gd`、`plane_view.gd`、`core_bridge.gd`（另外的 agent 正在改）、
`src/`（Rust 侧本任务不需要动）、`docs/INDEX.md`、`docs/HANDOVER.md`。

---

## 2. 资产：包里没有 `.mp3`，只有导入产物

`docs/111` §1.1 的清单是：包里 **9 个 `.mp3/.wav` 只剩 `.import` 存根**（源文件在导出时被吃掉了），
真正在包里的是 10 个导入产物（`.mp3str` / `.sample` / `.oggvorbisstr`），外加 `humm.wav` 这个
**唯一留在包里的源 WAV**（因为原版是自己用 hound 读它的，见 `docs/111` §4.3）。

⇒ 客户端要 `load()` 得到 `AudioStream`，必须先把导入产物**还原**成 Godot 能直接加载的文件。
`client/src/audio/extract_audio.py` 做的就是这个（只用标准库，只读 pck）：

```
.mp3str  → 解 RSRC 容器 → 取 AudioStreamMP3 的 data → assets/sounds/<name>.mp3
.sample  → 解 RSRC 容器 → 取 format/mix_rate/stereo/data → 套 RIFF 头 → assets/sounds/<name>.wav
humm.wav → 原样复制（原版读的就是源文件）
```

严格性（都是"解析不动就失败"而不是"猜一个"）：

* 容器解析必须**恰好**停在末尾 `RSRC` 魔数之后（`slack == 0`）；
* `PackedByteArray` 的 4 字节零填充必须真的是 0；
* 变体标签按资源格式自己的枚举（`NIL=1, BOOL=2, INT=3, FLOAT=4, STRING=5, PBA=31…`），
  遇到**跳不过去的标签就报错**，不猜；
* 落盘字节数与 `docs/111` 的内层流长度**逐一对上**：explosion 59520 / squash 13329 /
  spark 24576 / pop 2793 / unpop 15840 / tear 17430（`.mp3` 就是内层 mp3 本身），
  humm_2 113506+44 / humm_b 260218+44 / shepard_2 640292+44（PCM + 44 字节 RIFF 头）。

用法：

```bash
python3 client/src/audio/extract_audio.py --pck ../../Graphwar.pck      # 解 10 个到 client/assets/sounds/
python3 client/src/audio/extract_audio.py --check                        # 只查产物齐不齐（CI 用）
python3 client/src/audio/extract_audio.py --list                         # 只列会解出什么
```

产出目录 **`client/assets/` 整目录 gitignore**（`client/.gitignore:10`），媒体文件不入库。

> **位置说明**：为什么提取器不在 `tools/extract_assets.py`？那个脚本目前只解图标/字体
> （`docs/121` §5），音频这条线是本轮新加的，而本任务的改动边界只允许新建
> `client/src/audio/`。**建议**父 agent 把它合并进 `tools/extract_assets.py` 的 `plan()`
> （一行 TOCTOU 都不用改，见 §8 的"待办 1"）。

---

## 3. 事件↔音效：本轮把"未知"缩小了一步（但仍然未知）

`docs/111` §5.1 的结论是"每个音效对应哪个游戏事件 **未知**（缺：`0x4bfe10` 内 match 臂的解码 /
运行时观测）"。这一轮在这条线上**多拿到三条结构性证据**，都不足以定映射，但把"缺什么"写清楚了。

### 3.1 播放层订阅了 4 个**游戏事件**信号（观测）

`0x4bf550`（载资源的那个函数）在末尾并不只是 `load()`：它做了 **4 次信号连接**，
参数是"信号名 + 长度"的 `&'static str`，紧接着 `call *0x9bb030`（`R_X86_64_RELATIVE` → `0x514b40`）：

| 站点 | `.rodata` | 长度 | 信号名 | 事件 tag（`docs/114`） |
|---|---|---|---|---|
| `0x4bfa4a` | `0x1c2be2` | `0x11`=17 | `effect_info_event` | 44 |
| `0x4bfaa0` | `0x1c2bf3` | `0x12`=18 | `soldier_info_event` | 26 |
| `0x4bfaf6` | `0x1c2ab2` | `0x14`=20 | `entity_removed_event` | 32 |
| `0x4bfb4c` | `0x1c2c05` | `0x15`=21 | `game_state_info_event` | 34 |

⇒ **`SoundHandler` 是"网络事件驱动"的**，不是"UI 点击驱动"的。这条给了两个直接后果：

1. **原版很可能没有"按钮点击音"** —— 播放层根本没有连接任何 UI 信号。所以本客户端
   **没有**给按钮加点击音（这条是**负结果**，不是"没做"）。
2. 对局状态（`game_state_info_event`）确实会驱动音频 ⇒ "通关/失败有音效"这个方向是
   **结构性成立**的，但**是哪个音**仍然没有证据。

### 3.2 `0x4bf090` 是 `effect_info_event`（tag 44）的处理函数，按 `effect_type` 四臂分派（观测）

```
0x4bf09b  movabs $0x800000000000002c,%rax     ; 0x2c = 44 = effect_info_event 的 tag
0x4bf0a5  cmp    %rax,(%rsi)                  ; 事件判别值（首 8 字节 = 0x8000…|tag）
0x4bf0a8  jne    4bf2ce                       ; 不是这个事件 → 另一条路
0x4bf0b4  movzbl 0x20(%rsi),%eax              ; ← 分派依据：事件结构 +0x20 的一个**字节**
0x4bf0bc  lea    -0x2fc907(%rip),%rcx         ; 0x1c27bc = 跳转表
0x4bf0c3  movslq (%rcx,%rax,4),%rax
0x4bf0c7  add    %rcx,%rax
0x4bf0ca  jmp    *%rax                        ; 4 个臂
```

* 4 个臂各自落到 `self+0xa0` / `self+0x80` / `self+0x90` 三个 player 槽（与 `docs/111` §4.5
  的槽位表吻合：`0xa0` 有两个臂，`0x80`、`0x90` 各一个）；
* 每个臂都把 `event[+0x1c]`（`mov 0x1c(%r14),%r9d`）当 id 传给 `0x4bed70`，音量 `-15.0f`；
* `EffectInfo` 的字段是 `{effect_id, pos_x, pos_y, effect_type, start_tick}`（`docs/07` §…），
  `+0x20` 正好可以是 `effect_type`（**推断**：只证到"按一个字节分派"，没证到那个字节就是
  `effect_type` 字段本身，缺：事件结构的字段偏移表或运行时打印）。

`.rodata` 里 `effect_type` 的四个取值**首尾相接**（观测）：

```
0x1c64e5  ExplosionEffect (15)
0x1c64f4  DeathEffect     (11)
0x1c64ff  SparkEffect     (11)
0x1c650a  AshEffect        (9)
```

"4 个臂 vs 4 个效果名"很诱人，但**这不是证据**：没证到 `0..3` 的哪个值对应哪个名字
（可能是个 `match` 的名字表，也可能是序列化器的顺序），更没证到**每个臂播的是哪个流**。

### 3.3 槽位不是"音效身份"，是"通道"（观测）

`0x4bf090`（4 臂）、`0x4bf340`（`0xb0` 槽）、`0x4bf480`（`0xc0` 槽、`-5.0f`）这三个辅助函数
读的都是**同一个流字段** `self+0x60`/`self+0x68`：

```
0x4bf48c  mov 0x60(%rdi),%r15        ; 流的 ptr
0x4bf495  mov 0x68(%rbx),%r14        ; 流的 id
0x4bf4a2  lea 0x60(%rbx),%rdi ; lea 1bfe73 "clone" ; mov $5,%edx ; call 4068f0
0x4bf4cf  movss 1b9ba0(%rip),%xmm0    ; -5.0f
0x4bf4bb  lea 0xc0(%rbx),%rdi        ; ← 槽
```

⇒ **它们只决定"用哪个通道/音量"，不决定"放哪个音"**；流的身份是在调用之前（`0x4bfe10` 里）
设进 `self+0x60` 的。所以"读槽位表反推音效"这条路是**堵死**的：真正的映射在
`0x4bfe10`（45 KB，事件 Variant 机械）里。

### 3.4 结论

要把映射钉死，仍然需要 `docs/111` §5.1 说的两件事之一：
**解 `0x4bfe10` 的 match 臂**（每个臂把哪个 `self+0x60` 来源写进去），
或**运行时 hook**（在 `0x8ae4c0`=`play()` 上打印调用序 + stream 指针，配一次真机对局）。
本轮**没有**做这两件事，所以下表里"已确定"是空的。

---

## 4. 事件↔音效映射表

### 4.1 已确定（0 条）

| 游戏事件 | 音效 | 依据 |
|---|---|---|
| —— | —— | **没有任何一条映射有证据**。`docs/111` §5.1 判未知，本轮 §3 只把"缺什么"写细了 |

> 表格空着是**结论**，不是遗漏。按 `AGENTS.md` 第 1 条，宁可为空也不把"名字像"写成结论
> （`explosion.mp3` ↔ `ExplosionEffect` **不是**证据）。

### 4.2 未定（10 条，全部"映射未定"）

| 音效 | 原版是否加载/播放（`docs/111`） | 原版音量/槽位 | 映射 |
|---|---|---|---|
| `explosion.mp3` | ✅ `load()` 第 1 个（`0x4bf58b`） | -15 dB 四臂之一 | **未定** |
| `squash.mp3` | ✅ 第 2 个（`0x4bf5b2`） | -15 dB 四臂之一 | **未定** |
| `spark.mp3` | ✅ 第 3 个（`0x4bf5e3`） | -15 dB 四臂之一 | **未定** |
| `pop.mp3` | ✅ 第 4 个（`0x4bf624`） | -15 dB 四臂之一 | **未定** |
| `unpop.mp3` | ✅ 第 5 个（`0x4bf655`） | -15 dB 四臂之一 | **未定** |
| `humm.wav`（generator） | ✅ 原版自己读 + hound（`0x4bf681`/`0x4bf6c3`） | **-5 dB**（`0x4bf480`，槽 `0xc0`） | **未定**（何时开始推流未知） |
| `tear.mp3` | ❌ 全二进制无 `res://sounds/tear.mp3`，uid 在 105 `.gd` + 31 `.tscn` 里零引用（`docs/111` §5.2） | 无 | **未定**（原版是孤儿） |
| `humm_2.wav` / `humm_b.wav` | ❌ 同上 | 无 | **未定**（孤儿） |
| `shepard_2.wav` | ❌ 同上 | 无 | **未定**（孤儿） |

> 我们仍然**加载**这 4 个孤儿（任务要求 + 有人把真工程文件放进来就能直接用），
> 但代码里对它们的 `evidence` 字段明确写着"原版没有引用它"。

### 4.3 本客户端**临时**接线（3 条，自定，非原版）

代码位置：`client/src/app/app.gd` 的 `PENDING_MAPPING` 区块。每一条都带同一个前缀说明，
**没有任何一条**声称是原版行为。

| 动作（我们的客户端） | 播什么 | 槽/音量 | 为什么是这三条 | 等级 |
|---|---|---|---|---|
| `Fire!`（`GwCommands.FIRE` → `play_screen.fire()`） | `explosion` | `0xa0` / -15 dB | 原版的 `EffectInfo(ExplosionEffect)` 在弹道停止点（命中/爆炸），而我们的一次 `fire()` 里就把弹道+命中全结算了 ⇒ **时机**对得上；**音本身是自选的** | **自定** |
| 对局胜利（`report_match_result(true)`） | `unpop` | `0x80` / -15 dB | 原版播放层订阅了 `game_state_info_event`（§3.1）⇒ "对局状态变化会出声"结构上成立；**音是自选的** | **自定** |
| 对局失败（`report_match_result(false)`） | `squash` | `0x80` / -15 dB | 同上 | **自定** |

替换成本：这三条是**一个常量一处改**（`SFX_FIRE` / `SFX_WIN` / `SFX_LOSE`）；
`PENDING_MAPPING` 这个常量恒为 `true`，就是为了让"这些接线还没有原版依据"这件事在代码里无法被忽略。

### 4.4 未接（照实列）

| 时机 | 为什么没接 |
|---|---|
| **按钮点击** | 原版播放层只订阅 4 个**游戏事件**信号，没有任何 UI 信号（§3.1）⇒ 加点击音是**发明**，不接 |
| **命中/爆炸的分帧时机** | 原版是"事件到了才放"（`EffectInfo` 逐条），我们的 `fire()` 是同步一次结算完，App 看不到中间时刻；要分帧得改 `play_screen`（不在本任务范围） |
| **士兵死亡（`DeathEffect`）** | 同上：死在 `play_screen._resolve_shots()` 内部，App 观察不到；且**音效未定** |
| **`humm` 何时开始/停止** | 原版推流的触发条件没解出来（`0x4bfe10` 里 generator 分支的入口条件未知）⇒ 只提供 `App.audio.start_humm()` / `stop_humm()` 供以后的接线用 |
| **通关/失败的"结算音"之外的音** | 未定 |
| **`tear`/`humm_2`/`humm_b`/`shepard_2`** | 原版是孤儿，无时机可接（可用但未接线） |

---

## 5. 音量与静音：和原版逐条对应

### 5.1 音量是**槽的属性**，不是音效的属性

`docs/111` §4.5 的表照抄进 `GwAudio.SLOT_VOLUME_DB`（每个值都能指回一个立即数地址）：

| 槽 | `volume_db` | 原版依据 |
|---|---|---|
| `0x80` | **-15.0** | `0x1bb818`（`0x4bf233`→`0x4bf244`） |
| `0x90` | **-15.0** | `0x4bf279`→`0x4bf28a` |
| `0xa0` | **-15.0** | `0x4bf199` / `0x4bf1e6` 两个臂 |
| `0xb0` | **0.0**（默认） | 这一支（`0x4bf340` 附近）**没有** `set_volume_db` ⇒ Godot 默认 0 dB |
| `0xc0` | **-5.0** | `0x1b9ba0`（`0x4bf4cf`→`0x4bf4e3`），`0x4bf480` 建的那个槽 |

**本轮实测交叉验证（观测）**：同一个流（`pop.mp3`）分别放进 `0xa0`（-15 dB）与 `0xb0`（0 dB）
的 player，捕获到的**峰值**比 = **0.1778**，正好等于 `10^(-15/20)`。这条把"音量常量真的生效了"
钉死，不是只看代码里写的数。

### 5.2 静音 = Master 总线静音（照原版，不是逐 player 调音量）

原版 `0x4c11d0`：

```
4c135e  movzbl 0xd1(%rbx),%r15d
4c1366  or   %bpl,%r15b           ; 有效静音 = arg | self[0xd1]
4c1395  lea  0x1c2bdc             ; "Master"
4c13a4  call 4dd070               ; AudioServer::get_bus_index("Master")
4c13c7  call *0x9ba9a8            ; -> 0x89d7a0 = AudioServer::set_bus_mute(bus, mute)
```

我们照做：`GwAudio.set_muted()` 只调 `AudioServer.set_bus_mute(get_bus_index("Master"), v)`，
**没有任何一处**去改 `AudioStreamPlayer.volume_db` 来做静音（音量永远只由 §5.1 的常量决定）。
所有 player 都显式 `bus = "Master"`，所以总线静音对每一个音都生效。

`self+0xd1` 那个"第二个静音源"仍然**未知**（`docs/111` §5.3），我们只实现了已知的那一半（`arg`）。

**实测（观测）**：静音时 `AudioServer.get_bus_peak_volume_left_db("Master")` = **-200.0 dB**
（Godot 的静音地板），未静音时 = **-14.0 dB**（-15 dB 槽里的 explosion）。
⇒ 静音**真的**把总输出压到 0，不是只改了图标。

> 一个容易踩的坑：`AudioEffectCapture` 抓的是**静音之前**的信号（实测静音后 RMS 仍有值）。
> 所以自检里验证静音只能看 `get_bus_peak_volume_left_db()`，不能看捕获 RMS —— 这条写在
> `selftest.gd` 的注释里，免得下一个人再踩。

### 5.3 `store.muted` ↔ 总线的**双向**同步

```
        ┌──────────────┐   store → bus（启动/切换时）   ┌───────────────────────┐
        │ store.muted  │ ─────────────────────────────► │ AudioServer           │
        │ (gw2_client  │                                │ set_bus_mute(Master)  │
        │  .cfg)       │ ◄───────────────────────────── │ is_bus_mute(Master)   │
        └──────────────┘   bus → store（每帧 adopt_bus） └───────────────────────┘
```

* **store → bus**：`App._install_audio()` 里 `audio.set_muted(store.muted)`；`mute_changed` 一发
  就写回 `store.muted` 并 `save_all()`。
* **bus → store**：`App._process()` 每帧做一次 `audio.adopt_bus()`（一次布尔比较，代价可忽略）。
  任何人（界面、未来的设置面板、外部脚本）直接调 `AudioServer.set_bus_mute`，下一帧就被
  `store.muted` 吸收并落盘。
* **命令**：`GwCommands.TOGGLE_MUTE` → `App.toggle_mute()`。它**不**假设界面怎么写：
  * 界面有 `set_muted(bool)` ⇒ 直接推给它；
  * 界面只有 `toggle_mute()`（现在的 `play_screen` 就是这样，它自己翻 `_muted` 并写总线）⇒
    调用它，然后**以总线的实际变化为准**（总线变了就采纳，没变就由 `App` 翻转）。
  这样"另一个 agent 正在重写那四个开关"这件事不会打断静音语义。
* 旧界面的一个边角：`play_screen._muted` 初值是 `false`，所以**存档里 `muted=true` 时
  第一次进 PlayScreen** 要替它翻一次（`_align_play_mute_once()`，只做一次、无声音副作用）。
  等 `play_screen` 提供 `set_muted()` 后这段自动失效。

### 5.4 存档写在哪：**我们的**文件，不是原版的

* 静音状态存在我们自己的 `user://gw2_client.cfg` 的 `[profile] muted`（`store.gd` 既有的
  `ConfigFile` 机制，`docs/108` §7 第 19 条：这个文件本来就是我们自己的）。
* 原版是 `user://player.config`（`.rodata 0x1bfe12`，20 字节），格式是**每行 `key: value`、
  没有 `[section]` 头**、5 个键 `name/skin/face/hat/muted`（`docs/118` §3.2）。
  我们**读**它（`GwStore.parse_player_config()` / `read_player_config()`，容错：
  空行、`#` 注释行、没有 `:` 的行都跳过；值里允许再有 `:`），**不写**它。
* **为什么不写**：原版写者的**字段顺序**和 **`muted` 的取值编码**（`1`？`true`？）都只证到
  "分隔符是 `: `、每行以 `\n` 结尾"（`docs/118` §3.2.1 判**推断**）。拿未验证的编码去覆盖
  用户真机的 `player.config` 会把原版设置弄坏 —— 这条按"宁可不做"处理。
* `player.config` 的**格式本身**有断言覆盖（`parse`/`format` 往返、无 `[section]`、取值容错），
  这样将来要写回时格式那一半已经是验证过的。

---

## 6. 缺资产时的行为

| 层 | 行为 |
|---|---|
| `GwAudio._load_all()` | 逐个 `FileAccess.file_exists()`；缺了就记进 `missing()`，打印一行 `[gw2/audio] 缺资产，跳过：…`（**不** `push_error`，**不**崩） |
| `GwAudio.play()` | 未加载/名字未注册 ⇒ 返回 `false`；`App._sfx()` 把 `false` 咽掉（缺资产的入口在 `main.gd` 的硬检查里已经报过，不重复骚扰） |
| `humm` | `humm.wav` 解析失败（不是 RIFF/不是 16-bit/不是单声道/没有 data）⇒ 记 `missing("humm")` 并写下 `_humm_error` 原因，`start_humm()` 返回 `false` |
| 自检 | 有缺失时 **逐条 SKIP 并写原因**：`SKIP 音频 <name>：资产缺失（先跑 client/src/audio/extract_audio.py）`；其余（表结构/静音/降级/`player.config`）照常断言 |
| 实测 | 把 `client/assets/sounds/` 挪走后：**175 passed, 0 failed, 10 skipped**（一条没崩） |

---

## 7. 验证

```bash
# 资产（首次，或在 pck 换过之后）
python3 client/src/audio/extract_audio.py --pck ../../Graphwar.pck

# 自检（有音频：181 条全绿；无音频：175 passed + 10 skipped）
bash client/run.sh selftest          # → SELFTEST 181 passed, 0 failed

# 端到端（真开一局 + 按 Fire，抓总线音频与画面；headless 用的 Dummy 音频驱动同样真实混音）
xvfb-run -a "$GODOT" --path client --rendering-driver opengl3 --audio-driver Dummy \
    --resolution 1600x900 res://scenes/main.tscn
```

端到端那一步是用一个**临时驱动场景**（`res://src/audio/_demo.tscn`，跑完已删除、不入库）做的，
它做的事就是本文这几行，任何人都能照抄复现：

```gdscript
var main = load("res://scenes/main.tscn").instantiate(); add_child(main)
var eff = AudioEffectCapture.new(); eff.buffer_length = 2.0
AudioServer.add_bus_effect(AudioServer.get_bus_index("Master"), eff, 0)
get_node("/root/App").command(GwCommands.CREATE_GAME)     # 开一局（与 selftest 同一序列）
get_node("/root/App").state.add_bot(2)
get_node("/root/App").command(GwCommands.START_GAME)
eff.clear_buffer(); get_node("/root/App").command(GwCommands.FIRE)   # ★ 动作
await get_tree().create_timer(0.4).timeout
var frames = eff.get_buffer(eff.get_frames_available())              # → 峰值 0.20081
get_viewport().get_texture().get_image().save_png(".../gw2_play_fire.png")
```

**本轮实际跑出来的证据**（`bash client/run.sh selftest` + 上面的端到端驱动）：

```
@@DEMO screen=PlayScreen muted=false
@@DEMO fire: frames=32768 peak=0.20081 ms=773
@@DEMO sfx_played=["explosion@0xa0"]                       ← 动作 → 音效（我们的临时接线）
@@DEMO muted fire: peak=0.20081 bus_peak=-200.0 muted=true ← 同一动作，总线峰值已到静音地板
SELFTEST 181 passed, 0 failed
```

* 捕获到的 WAV（44.1 kHz 单声道，0.743 s，峰值 6580/32767）与截图作证据留在
  `.agent-work/audioclient/evidence/`（**不进仓**）。
* 新增断言 35 条，覆盖：音量/槽位常量、每个音效都有证据字段、缺资产降级、
  `player.config` 格式、静音的**两个方向**与落盘、**真的出声**（RMS/峰值）、
  `-15 dB vs 0 dB` 的峰值比、`humm` 的 generator 推流。
* **既有 146 条一条没删、没改**（只在其后追加）。

---

## 8. 仍未确定 / 后续

1. **事件↔音效映射（最重要的那一条）**：需要解 `0x4bfe10` 里"哪个臂把哪个 `self+0x60` 来源写进去"，
   或者按 `docs/111` §5.1 的方案做运行时 hook。§3 只是把范围从"整个模块"缩到
   "`effect_info_event` 的 4 个臂 + 4 个 `effect_type` 名字"。
2. **`+0x20` 是不是 `effect_type`**：本轮只证到"按事件结构 +0x20 的一个字节四臂分派"（**推断**）。
3. **`humm` 的触发条件**：generator 的启动/停止时机没解出来；`push_buffer` 的分块大小
   （我们按 `min(avail, 2048)` 推）也不是原版实测值。
4. **`self+0xd1`** 那个第二静音源（`docs/111` §5.3）。
5. **`0x3a5760`**（20288 字节的事件驱动者）的完整语义 —— 谁在什么时候把事件交给 `0x4bfe10`。
6. **孤儿音效的归属**：`tear` / `humm_2` / `humm_b` / `shepard_2` 在原版里没有任何引用
   （`docs/111` §5.2 的负结果），要判"是不是死资源"还得扫 `.gdc` 的字符串常量（没做）。
7. **提取器的归属**：现在的 `client/src/audio/extract_audio.py` 建议并进
   `tools/extract_assets.py` 的 `plan()`，这样 `bash client/run.sh assets` 一次解完图标+字体+音频。
   本轮不改那个脚本（改动边界），所以音频要单独跑一次。
8. **其它平台/驱动**：本轮只在 headless + `--audio-driver Dummy` 与本机 ALSA 缺失的环境下验证；
   真有声卡的机器上 `Master` 总线路径与原版一致，但**没在 Windows/macOS 上跑过**。
9. **`music.ogg` 不是游戏音乐**（AdMob 样例，`docs/111` §2.5）⇒ 客户端**没有**背景音乐。

---

## 9. 复现清单

```bash
# 1) 资产
python3 client/src/audio/extract_audio.py --pck ../Graphwar.pck --check

# 2) 解析器自证（容器账本 + 内层流长度都要对得上 docs/111 §3）
python3 client/src/audio/extract_audio.py --pck ../Graphwar.pck --list

# 3) 自检
bash client/run.sh selftest

# 4) 无资产时的行为（应当 10 条 SKIP、0 failed）
mv client/assets/sounds /tmp/sounds.bak && bash client/run.sh selftest; mv /tmp/sounds.bak client/assets/sounds

# 5) 提交前自检（必须为空）
git diff --cached --name-only | grep -E '\.(ogg|mp3|wav|sample|mp3str|oggvorbisstr|ctex|fontdata|svg|ttf|png|tres|pck)$'
```
