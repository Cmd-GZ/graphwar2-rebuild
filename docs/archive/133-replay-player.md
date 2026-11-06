# 133 · 回放播放器（ReplayLobby → PlayScreen 回放模式）

状态：**现行**。本文回答四件事：回放文件**怎么读**、播放器**按什么推进**、
哪些是**原版观测**哪些是**我们自定**、以及这一轮**验证到了什么程度**。

> 前置：`78-replay-files.md`（格式与位置）、`79-replay-ai-corpus.md`（实体 id 链路）、
> `35-tick-clock.md`（tick 时钟 60 Hz）、`125-playscreen.md` / `126-level-obstacles.md`
> / `131-level-rules.md`（PlayScreen 现状）、`08-plane.md`（棋盘几何）。
> 本文**不修改** `docs/INDEX.md` 与 `docs/HANDOVER.md`。

证据分级（沿用 `AGENTS.md`）：
**逐位验证** = 由 `cargo test` / 自检断言钉住；**观测** = 能指到文件字节、事件名或抓包；
**推断** = 写明缺什么证据；**未知** = 没有证据。

---

## 0. 一页结论

| 问题 | 结论 | 级别 |
|---|---|---|
| 回放文件长什么样 | gzip + JSONL，每行 `{"event":{"<事件名>":{…}},"tick":N}` | **观测**（400 份实文件） |
| 事件名/字段名和网络协议是同一套吗 | 是。解析直接复用 `src/protocol.rs` 的 `parse_message` 与 `get_*` | **观测 + 逐位验证** |
| 一局有多少事件 | 真实语料 400 份、**103 104** 条；单局 2 … 1471 条（另有 2 份空回放） | **观测** |
| 播放按什么推进 | **按 tick**（不是按墙钟/按事件）。60 tick/秒 | **观测**（`docs/35`）+ 我们自定的取整规则 |
| 回放的弹道点序列在哪 | **没有**。只有 `FunctionPoints{start_x,start_y,end_x}` | **观测**（400 份里 0 例带点序列） |
| 那弹道怎么画 | 用内核重算：`GwCore.calculate_path(函数文本, 起点, 队伍方向, "ode"/"normal")`，`mode` **取自这一局的 `FunctionModeInfo`** | 我们的做法（走内核，不绕过） |
| `DiffEqFunction` 局（320/400）的弹道 | 走 **`Mode::Ode`**（RK4）⇒ 能画了。内核钩子是 `GW2Rebuild.path_calculate_mode(…, mode: String)`，取 `"normal"` / `"ode"`（`docs/138` 交付，已进 main）；`path_calculate` 的签名一字未动 | **逐位验证**（§5.3 第 7 组、§5.6：4 发 `end_x` 全中） |
| 回放界面长什么样 | **未知**（只有场景名 `replay_lobby.tscn` / `replay_entry.tscn`） | **未知** ⇒ 播放控件我们自定 |
| 播放/暂停/进度 | **我们自定**（原版无观测） | 自定，显式标注 |
| 能播了吗 | 能。真实回放读、解析、按 tick 播、死亡事件生效、弹道由内核重算 | **逐位验证**（§5） |

**新增文件**：`src/graphwar_replay_player.rs`（格式/局面/播放器/对拍 ABI）、`src/replay_inflate.rs`
（纯 Rust gzip+DEFLATE）、`client/src/play/replay_file.gd`（客户端读文件）、
`client/src/play/replay_player.gd`（tick 播放器 + 局面还原）、`docs/133`（本文）。
**改动文件**（都是"只加"或删占位，没有改既有语义）：`client/src/screens/play_screen.gd`
（回放分支 + 一条隐藏进度行 + **按录像的 `FunctionModeInfo` 选 `normal`/`ode`**）、
`client/src/screens/replay_lobby.gd`（列表数据源）、
`client/src/app/app.gd`（**只删**两处"回放未实现"占位）、
`src/lib.rs`（**两行** `pub mod`）、`client/tools/selftest.gd`（只新增断言）。
微分方程模式的**内核侧**（`src/api.rs` / `src/gdext.rs` / `core_bridge.gd`）由另一条分支
`agent/ode-api` 交付、已进 main，见 `docs/138` —— 本文只负责"回放里怎么用它"，
**不重复实现**。

---

## 1. ⚠️ 根因（单列一条）：列表按**扩展名**过滤，而真实回放**没有扩展名**

> **这是"看起来是功能没做、其实是过滤条件错了"的典型 bug。**
> `client/src/app/replay_index.gd` 的 `EXTENSIONS := ["replay","gw2r","json","dat"]` 白名单，
> 而真实回放的文件名是**纯 unix 时间戳、没有扩展名**（`docs/78` §1 早就写了这一点）。
> ⇒ **400 份真实回放一份都进不了 ReplayLobby**，界面长期显示空白列表；
> 空列表看起来像"回放功能没做"，实际只是筛掉了全部文件。
>
> 修法：`GwReplayFile.list_dir()` 列举**全部常规文件**，不按扩展名筛（`docs/133` §4.4）。
> 教训：**当"某个目录里明明有东西、界面却是空的时候"，先看过滤条件，再看功能实现。**
> 同类陷阱：`get_files()` 只列文件不列目录、只列**不递归**子目录、
> 大小写敏感的 `get_extension()`、以及 `.hidden` 文件在部分平台被跳过。

两件事叠在一起，缺一个都看不见回放：

1. 上面那条：扩展名白名单把**全部**真实回放筛掉了。
2. 就算进了列表，`PlayScreen` 也没有回放分支：`state.kind == REPLAY` 时
   `begin_match()` 走的是普通对局路径，玩家表是空的（`reset_replay()` 清空了 `players`），
   于是棋盘上一个士兵都没有。

本轮把数据源换成 `GwReplayFile.list_dir()`（**不过滤扩展名**，新的在前），
并在 `PlayScreen` 里加了唯一一个分叉（§4.1）。

---

## 2. 格式与解析

### 2.1 容器（**观测**）

```
user://replays/<unix 时间戳>      ← 文件名没有扩展名
1f 8b 08 00 …                     ← gzip 头（RFC 1952）
<DEFLATE 正文>
<解压后：每行一个 JSON，UTF-8>
```

样本 `1793897298`：**1276 字节**（gzip）→ **8617 字节**明文 → **104 条**事件。
语料整体：400 份文件 / 103 104 条事件（其中 **2 份是空回放**，见 §2.2）。

### 2.2 三种脏数据（**观测**）

| 现象 | 数量 | 处理 |
|---|---|---|
| **空回放**（gzip 有效、正文 0 字节） | **2 / 400**（`1782674520`、`1782761998`，都 20 字节） | 内核 `ReplayError::Empty` / ABI 报 `0` 条；客户端 `ok()==false` + 原因。**不假装能播** |
| **tick 回退**（后一行 tick 比前一行小） | **7 / 400**，单次最多回退 **19** tick（`1783011645` 里 2 次） | **不排序**，游标模型天然容忍（§4.2） |
| 解析不了的行 | 系统内 0 例（自检里人为造了 1 行） | 跳过并计入 `skipped_lines`，与 `tools/parse_replay.py` 的 `continue` 同口径 |

### 2.3 一行怎么解析（**观测 + 逐位验证**）

```
{"event":{"FunctionInfo":{"owner_id":635,"function":"x","corrected_function":null}},"tick":1234}
           └─────────┬──────────────────────────────────────────────────────┘        └──┬─┘
                载荷对象（内层单键）                                                    tick
```

1. 从 `"event"` 后面取出**花括号配平**的那一段（字符串里的花括号不算）；
2. 这一段交给 `protocol::parse_message()` —— **和网络报文同一个解析器**，
   得到 `{kind: "FunctionInfo", body: "…"}`；
3. `tick` 只从 event 对象**之后**取（否则会把 `active_tick` / `start_tick` 当成它）。

字段读取一律走 `protocol::get_str / get_int / get_f64 / get_bool / get_color`。
**协议里没有的字段不猜**：`ReplayEvent.body` 保存载荷的原始 JSON 内部原文
（不含最外层花括号，与 `RawMessage.body` 同一种切法），客户端侧则是整份
JSON 字典原样保留 —— 两条路都保证"未知事件也能透传"。

### 2.4 解压为什么自己写（内核侧）

`Cargo.toml` 的 `[dependencies]` 是**空的**，`cargo test --offline` 才在任何机器上能跑；
加 `flate2` 就要联网 + 改 `Cargo.lock`。所以 `src/replay_inflate.rs` 自己实现
DECODER：stored / 固定 Huffman / 动态 Huffman 三种块 + `FEXTRA/FNAME/FCOMMENT/FHCRC`
四种可选头 + 可拼接 member。**不校验** gzip 尾部的 CRC32/ISIZE（解压失败由 DEFLATE
结构自身报错，回放不是校验场景）。

### 2.5 客户端为什么不用自己写的解压

客户端不用内核那个 DECODER，而是 Godot 自带的
`PackedByteArray.decompress(size, FileAccess.COMPRESSION_GZIP)`。

> ⚠️ **坑（`docs/133` §5.4 记录的就是它）**：`FileAccess.open_compressed()`
> **不能**读裸 gzip —— 它读的是 Godot 自己的 `GDPC` 容器（魔数 `0x43454447`），
> 对真回放一律返回 `ERR_FILE_UNRECOGNIZED`（"File unrecognized"）。
> 能读裸 gzip 的只有 `PackedByteArray.decompress()`。
> 分配尺寸直接取 gzip 尾部 4 字节 `ISIZE`（RFC 1952 §2.3.1 的未压缩大小 mod 2^32），
> 精确、不用猜。

---

## 3. 局面还原规则（哪条事件改哪个字段）

口径与 `tools/replay_state.py` **逐条一致**（这是"三方对拍"能成立的前提，§5.2）。
只有下面这些事件改局面，其余**只进事件流**（不猜副作用）：

| 事件 | 改什么 | 级别 |
|---|---|---|
| `ClearObstacles` | 障碍清空 | **观测**（`docs/30`） |
| `AddObstacle{pos_x,pos_y,radius}` | 障碍追加（半径是棋盘单位） | **观测** |
| `DestroyObstacle{pos_x,pos_y,radius?}` | 记一个"洞"（**不移除**障碍数组，`radius` 缺省 0.5） | **观测** |
| `PosInfo{entity_id,pos_x,pos_y,radius?}` | 实体位置（`radius` 缺省 0.5，抓包里恒 0.5） | **观测** |
| `TeamInfo{entity_id,team}` | `"Team1"/"Team2"`（**不是数字**） | **观测** |
| `LifeInfo{entity_id,alive}` | 生死；**缺省视为活着** | **观测** + 推断（缺省语义） |
| `FunctionInfo{owner_id,function,corrected_function}` | `owner_id` 是**士兵 id**；`function` 为空**不覆盖**上一条（玩家打字中间态，`docs/78` §4）；`corrected_function` 为 `null` 时单独记 `"__null__"` | **观测** |
| `FunctionPoints{owner_id,start_x,start_y,end_x}` | 记一次开火 + 那一刻的函数文本（**没有点序列**） | **观测** |
| `FunctionActive{owner_id,active_tick}` | 记激活 tick | **观测** |
| `EffectInfo{effect_id,pos_x,pos_y,effect_type,start_tick}` | 记特效 | **观测** |
| `EntityRemoved{entity_id}` | 记移除（**不删**位置/队伍表，与参考实现一致） | **观测** |
| `GameStateInfo / GameNameInfo / *ModeInfo / TurnCountInfo / TurnInfo` | 状态机、房间名、四个模式、回合数、射手 | **观测** |
| `Skin/Face/HatGraphicsInfo{entity_id,graphics_str}` | 外观名（就是资产文件名，如 `face_paint_2` / `asian` / `asian_hat`） | **观测** |
| `ColorInfo{entity_id,color:{r,g,b}}` | 玩家颜色（真实值，`docs/102` 的队色表就是这么解出来的） | **观测** |

**两张全局表**（在**整份文件**上预扫，不随 tick 变）：
`NameInfo.entity_id → name`（`entity_id` 就是 `player_id`）与
`SoldierInfo.soldier_id → player_id`（`docs/79` §1 的链路：函数归属要经过士兵→玩家）。
靶子集合取 `TargetInfo.target_id`。

### 3.3 四个模式事件的字符串取值（**观测**，400 份回放全量统计）

| 事件 | 取值（括号内 = 出现次数） | → `GwModes` 档位 |
|---|---|---|
| `AxisModeInfo` | `EveryUnit`(615) / `EveryFive`(34) / `OnlyMain`(54) / `NoAxis`(144) | 3 / 2 / 1 / 0 |
| `FunctionModeInfo` | `NormalFunction`(548) / `DiffEqFunction`(320) | 0 / 1 |
| `TurnModeInfo` | `SequentialTurns`(658) / `SimultaneousTurns`(168) | 0 / 1 |
| `TimeModeInfo` | `Timer1m`(342) / `TimerInf`(470) / `Timer3m`(28) / `Timer5m`(22) / `Timer30s`(11) / `Timer2m`(191) | 1 / 5 / 3 / 4 / 0 / 2 |

档位顺序本身来自 `docs/92` §6 的连点观测。`AxisModeInfo` 的四个名字与 `docs/118` §7.1
从 21 关描述反推的 "no axis / main axes only / every 5 units / every unit"
**顺序完全对上**（3 = EveryUnit），是两条独立证据的交叉。

---

## 4. 播放模型（tick 驱动）与界面

### 4.1 分叉点（`play_screen.gd` 里唯一为回放新增的东西）

```
app.gd:  PLAY_REPLAY → state.reset_replay(lobby.selected_replay()) → begin_match()
play_screen.gd: begin_match()
                  ├─ kind == REPLAY 且 replay_file != ""  →  _begin_replay(path)   ← 新
                  └─ 其它                                  →  原有对局路径（一字未改）
```

回放模式下**不接管输入**：`key_input()` 与 `fire()` 直接 return，键盘与 Fire 按钮
显式 `disabled`（不是"按了没反应"）。其余绘制/结算路径**原样复用**：
`_plane.set_soldiers()`、`set_targets()`、`set_obstacles()`、`set_path()`、`_clip_to_obstacle()`。

### 4.2 tick 时钟（**观测** + 我们自定的取整）

* 真值：`docs/35` §2 —— `current_tick = base + round((now - base_ms) × 60/1000)`，
  113 个 `TickReply` 样本里 110 个完全吻合 ⇒ **精确 60 tick/秒**；
* 播放器每帧 `tick += dt × 60 × speed`，**余数留着**（不留就会 1× 每帧慢 1 tick）；
* 只消费 `事件.tick <= 当前 tick` 的事件，**按文件顺序**：
  * 不排序 —— 同一 tick 内的批内顺序本身有意义（`docs/113`），排序会打乱它；
  * 游标模型天然容忍那 7 份 tick 回退（迟到的条目会在一次推进里被一起消费）；
* **跳到任意 tick = 从头重放**（整局最多 1500 条事件，代价可忽略）。这样
  "任意时刻的状态" 恒等于 `state_at(tick)`，不会出现增量重放的漂移；
* 掉帧保护：单帧 `dt` 上限 1 s（不然一次卡顿会把几十秒的事件一口气吞掉）；
* 放完自动暂停；再按播放 = 从头放（我们自定的交互）。

### 4.3 播放控件（**我们自定**；原版未观测）

原版只有**场景名**（`res://replay_lobby.tscn`、`res://replay_entry.tscn`，`docs/78` §1）
与三颗键的图名（`prev` / `play_pause` / `next`，`docs/92` §4.10）。界面长什么样、
进度条有没有、快捷键是什么，**都没有观测**。我们做的：

| 元素 | 我们的做法 | 级别 |
|---|---|---|
| 播放 / 暂停 | 左侧 `play_pause` 键取反（原版键名保留） | 自定 |
| 上一事件 / 下一事件 | 左侧 `prev` / `next` 键：跳到上/下一条**事件**的 tick | 自定 |
| 进度 | 底栏新增一行 `ReplayStatusBar`：`ReplayProgress`（0..1）+ `ReplayTickLabel`（`tick 1291/2276 ／ 事件 88/104 ／ 暂停`） | 自定 |
| 速度 | 播放器有 `speed`（1× / 2× 自检覆盖），界面**没有**做倍速按钮 | 自定 |
| 状态行 | 隐藏的 `StatusLabel`（`docs/125` §5 的既有约定：诊断用，不显示） | 沿用 |

> ⚠️ **实现偏离（必须记）**：左侧三颗播放键**不走 App 命令总线**。
> `app.gd` 对 `play_pause` / `⏮︎` / `⏭` 三个键仍是
> `notify("回放播放未实现")` 的占位分支，而 `app.gd` 不在本轮可改范围
> ⇒ 按钮直接接本界面的播放器（原版命令键记在 `meta("observed_input_text")` 与 tooltip 里），
> 免得按一下弹一个假提示。右侧三颗（存截图/存摘要/存片段）保持原样 —— 它们的语义本来就**未知**。
>
> **两个占位已经删掉**（本轮获准只动这两处）：`app.gd::_play_replay()` 里的
> `notify("回放播放未实现")`，以及 `PLAY_PAUSE` 分支的同名占位（现在什么都不做，
> 因为那三颗播放键直接接播放器）。回放分支加载成功后会弹一句准确的气泡
> ——`回放 <房间名> ／ N 条事件 ／ tick A–B`（气泡里不能出现
> `app.gd::INTERNAL_MARKERS` 那几个内部标记，所以只写事实）。

### 4.4 ReplayLobby

| 项 | 我们的做法 | 级别 |
|---|---|---|
| 列哪些文件 | `user://replays/` 下**全部**常规文件（不按扩展名筛），新的在前 | 观测（文件名 = 时间戳，无扩展名） |
| 条目文本 | 文件名（= 录制时间戳）、右侧 `YYYY-MM-DD HH:MM ／ N B` | 自定（原版条目的三段文本是 `ReplayName`/`GameName`/四个模式图标） |
| 房间名 / 四个模式 | **列表里不解析**（要解压整份文件，400 份 ≈ 64 MB）；打开回放后显示 | 自定 |
| 空列表 | 一个字都不画（既有断言 `回放列表空时只剩标题一个 Label` 仍然成立） | 沿用 |

### 4.5 弹道怎么画（不绕过内核；DiffEq 局走 ODE）

回放**没有**点序列（`FunctionPoints` 在 400 份里出现 1290 次，字段**恒为**
`{owner_id, start_x, start_y, end_x}` 四个，0 例带点、0 例多余字段）。所以每一发都走：

```
expr = state.functions[owner]                    # 开火那一刻最近一次非空 FunctionInfo
mode = "ode" if state.function_mode == "DiffEqFunction" else "normal"   # 取自**这一局自己的**录像
dir  = +1 (Team1) / -1 (Team2)                   # 方向由队伍决定（docs/105），队伍未知才退回几何
GwCore.calculate_path(expr, Vector2(start_x, start_y), dir, mode)       # ← 内核采样器
→ _clip_to_obstacle(...) → _plane.set_path(...)  # 与普通对局同一个绘制入口
```

**为什么要有 `mode`**：`DiffEqFunction` 局（400 份真实回放里 **320 局**）的 `f` 是
`dy/dx`，要走 `src/graphwar_path_calculator.rs` 的 RK4（`Mode::Ode`）；既有的
`GW2Rebuild.path_calculate` 只跑 `Mode::Normal` —— 拿普通模式去画一条导数曲线，
**看起来能画、其实是错的线**。

内核侧的钩子由 **`agent/ode-api` 交付并已进 main**（`docs/138`），本文不重复实现。
与本文件相关的三条契约（**逐位验证**，见 §5.3 第 7 组）：

```text
GW2Rebuild.path_calculate_mode(expr: String, x0: float, y0: float,
                               direction: float, mode: String)
    -> Dictionary { ok, points, offset, error }      # 与 path_calculate 同结构
mode: "normal"（y = f(x)）/ "ode"（dy/dx = f(x, y)），大小写与两侧空白不敏感
      其它任何取值 ⇒ ok:false, error:"bad_arguments"（不静默退回 Normal）
```

| 位置 | 说明 |
|---|---|
| `src/api.rs::calculate_path_mode()` | `"normal"` 分支**直接转调** `calculate_path` ⇒ 与原契约结构上等价；`"ode"` 走 `calculate_path_ode()` |
| `src/api.rs::parse_mode()` | 只认 `normal` / `ode`（`trim` + `eq_ignore_ascii_case`）；**不做整数重载** —— `PropertyInfo` 把第 5 个参数声明成 `String`，GDScript 传整数会被引擎先拒 |
| `client/src/core/core_bridge.gd::calculate_path()` | 第 4 个参数 `mode: String = "normal"`（默认值 ⇒ **既有三参数调用点一个字都不用改**） |

**修正常数的口径**：只有 `normal` 加修正（`src/graphwar_shot_calculator::shot_points` 的既有规则）。
`ode` 下内核返回的 `offset` 是 **`NaN`** ⇒ `GwCore.format_offset()` 出空串，
正好是原版 DiffEq 局右下角不显示修正文本的样子（`docs/92` §4.10、`docs/138`）。
这是**观测**：微分方程回放的 `FunctionInfo{function:"y+0.1", corrected_function:"y+0.1"}`
逐字相同，没有 `" - 3.694"` 那种尾巴。

**内核算不出来时**（表达式非法、起点越界、首点之后一步都接受不了）⇒ **清空弹道**并在
状态行写明原因，**不拿界面预览折线冒充**。

---

## 5. 验证记录

### 5.1 单元测试（`cargo test --offline`）

**333 条**：`--lib` **273** + 9 个集成测试二进制 **60**（broker 4 / events 6 / evidence 6 /
golden 4 / live_hits 12 / plane 6 / replay_diff 12 / server_match 3 / to_text 7）；
其中 **331 绿、2 红**（见下面的已知红）。
回放相关本轮新增 **18** 条：12 条 `replay` + 6 条 `replay_inflate`。
带模式的弹道那几条不在这里 —— 它是 `agent/ode-api` 的交付（`docs/138`，+6 条）。
⚠️ `tests/server_match` 里 **两条已知会红**（本机实测就是这两条，第三条过但很慢）：
`plays_a_full_match_over_tcp` / `rotation_and_ai_are_wired_over_tcp` ——
`src/game.rs::fire` 的步长从写死的 `0.5` 改成 `SERVER_STEP`(0.01) 之后，
那两条靠"穿过墙"才打中的用例被正确挡住（`docs/136`）。
**不是回放引入的**：本轮 `src/` 相对 main **零改动**（`git diff --stat origin/main` 只有
`play_screen.gd` 与 `selftest.gd`），因此红/绿与 main 逐条相同。
新增断言覆盖：明文/gzip 两种输入、未知事件透传、坏行跳过、空回放、
tick 边界（2168 活 / 2169 死）、规范串格式、播放器 tick 推进与倍速、
`seek` 与从头重放一致、上一/下一事件、C ABI 往返；
带模式的弹道那三条硬约束（`"normal"` ≡ `path_calculate`、`"ode"` 复现真实 DiffEq 局的
`end_x` 与点数、未知取值被拒）由 `agent/ode-api` 自己的测试覆盖（`docs/138`），
本文不重复。

### 5.2 三方对拍（这是本轮最强的一条证据）

同一份真实回放、同一个 tick，**三个独立实现**给出**逐字符相同**的规范局面串：

| 实现 | 位置 | 它证明了什么 |
|---|---|---|
| Python 参考 | `tools/replay_state.py` 的循环（口径来源） | 规则本身 |
| Rust 内核 | `src/graphwar_replay_player.rs`（`cargo test` 覆盖） | 复原实现与参考一致 |
| GDScript 客户端 | `client/src/play/replay_player.gd`（自检断言） | 真正播放的那一份也一致 |

规范串格式（三端同一份代码长度 ≤ 20 行，**没有任何浮点精度容差**）：

```
tick=T|O=x,y,r;…|E=id,team,alive,x,y,r;…|F=owner=text;…
```

（数字一律 `%.6f`；`team` 未知写 `?`；`alive` 写 `0/1`；没有 `PosInfo` 的实体留空三项；
障碍保持插入顺序；实体与函数表按 id 升序。）

**全语料对拍**（内核 C ABI ↔ Python 参考，每份文件取 4 个 tick：首、中、末 + 每个死亡 tick）：

```bash
# 先构建，再用 ctypes 调内核的两个导出函数
cargo build --offline
python3 - <<'PY'
import ctypes
lib = ctypes.CDLL("<target>/debug/libgw2_rebuild.so")
lib.gw2_replay_state_at.restype = ctypes.c_int64
lib.gw2_replay_state_at.argtypes = [ctypes.c_char_p, ctypes.c_int64, ctypes.c_char_p, ctypes.c_size_t]
buf = ctypes.create_string_buffer(1 << 20)
n = lib.gw2_replay_state_at(b"/path/to/replays/1793897298", 2169, buf, len(buf))
print(buf.value.decode())
PY
```

实测结果：**files=400 / events=103 104 / states_checked=1610 / failures=0**
（1610 = 400 份 × 首/中/末 3 个 tick + 每份最多 2 个死亡 tick；2 份空回放跳过）。

样本 `1793897298` 的三处真值（Level 0、NormalFunction、1 士兵 230 + 1 靶子 232）：

| tick | 期望串（`…` 处为三段相同的前缀） | 含义 |
|---|---|---|
| 550 | `tick=550\|O=\|E=229,Team1,1,,,;230,Team1,1,-5.156307,-9.889981,0.500000;231,Team1,1,,,;232,Team2,1,9.782073,4.595394,0.500000\|F=230=x+2sin(20x)` | 第一发刚打出 |
| 2168 | `…;232,Team2,1,…` | 靶子还活着 |
| 2169 | `…;232,Team2,0,…` | **死亡事件**：靶子被打掉 |
| 2276 | 同 2169，`turn_count=2`、`game_state=EndingGame` | 结尾 |

### 5.3 自检（`bash client/run.sh selftest`）

**581 条全绿**（main `e9c188b` 上实测；回放本轮新增 **16** 条 —— 就是第 7 组。
**没有改/删任何既有断言**）。
把 `examples/serve_match` 拿掉时是 `505 passed, 0 failed, 2 skipped`（联机两组整组 SKIP）。
两个数都 ≥ 交付要求的 489。
回放相关的新增断言（`_test_replay_player`，故意排在最后，不影响其它用例）：

1. 行解析：坏行跳过并计数、空回放明确报错、协议里没有的事件名照样读出字段；
2. 真实回放（`$HOME/.local/share/godot/app_userdata/Graphwar/replays/1793897298`，**只读**）：
   gzip、104 条事件、tick 1..2276、`NameInfo.entity_id` = `player_id`、士兵→玩家映射、靶子集合；
3. 播放模型：起点/终点、0.5 s = 30 tick、暂停不推进、2× 倍速、往回跳 = 从头重放、
   上一/下一事件；
4. 局面与 `replay_state.py` 口径**逐字符相同**（550 / 2168 / 2169 / 2276 四处，含死亡那一拍）；
5. ReplayLobby 能列出真实存档目录（条数 = 目录里的文件数，只读）；
6. **端到端走 App 的真实命令链**：在 ReplayLobby 里按那份回放的
   `PlayReplayButton` → `app.gd::_play_replay()` → `state.reset_replay()` →
   `begin_match()` → 回放分支；然后断言：进入即播、进度条在 0..1、tick 标签、
   **键盘与 Fire 不接管**、靶子在 2169 从棋盘上消失、tick 550 的弹道由内核重算出 >1 个点、
   **内核算出的修正 = 录像记录的 `-3.694`**、播放键取反/跳转、`exit_play` 回到 MainMenu
   且模式归位；
7. **第 7 组：微分方程局能画出弹道**（`_test_replay_ode`，样本 `1783255495`，Level 16、
   131 条事件、4 发、靶子 3683 在 tick 1291 阵亡）：
   内核确实暴露 `path_calculate_mode`、`calculate_path(..., "ode")` 走内核且
   **`offset` 是 `NaN`**（`format_offset()` 出空串）、ODE 弹道 **1058 点**且
   **最后一点 x = 录像 `end_x`（-2.759978…）**、`mode="diffeq"` 这类未知取值被拒
   （不静默退回 Normal）、`mode="normal"` 与三参数调用同点数（既有契约未变）、
   回放里模式**取自录像**（`DiffEqFunction`）、状态行写明 `Ode(dy/dx)`、
   DiffEq 局不显示修正项、靶子在 1290 活 / 1291 死。

缺存档目录的机器上，第 2–7 组各记 **1 条 `SKIP`** 并打印原因（不算通过），
其余合成用例照常跑 —— 实测：`HOME` 指到空目录时
`SELFTEST 285 passed, 0 failed, 2 skipped`（280 条既有 + 5 条新增合成用例）。

### 5.4 本轮踩到并修掉的坑

1. **`FileAccess.open_compressed()` 读不了裸 gzip**：它读的是 Godot 自己的 `GDPC` 容器
   （魔数 `0x43454447`），对真回放返回 `ERR_FILE_UNRECOGNIZED`。
   正解：`PackedByteArray.decompress(size, FileAccess.COMPRESSION_GZIP)`，
   尺寸取 gzip 尾部 `ISIZE`。
2. **拼接 member 不能靠扫 `1f 8b`**：内核第一版 gunzip 解完一个 member 就往后扫
   `1f 8b` 找下一个，结果压缩流内部恰好出现这两个字节时会**跳错位置** ——
   400 份真实回放里有 **18 份**因此整份解析失败。正解：DECODER 回报**消耗了多少字节**
   （`inflate_with_consumed`），trailer 从那里算。
3. **`queue_free()` 是延迟删除**：换目录后立刻数条目，数到的是上一批（400 条）。
   自检里必须 `await _frames(2)` 再数。
4. **`GwFunctionInput` 不是 `LineEdit`**：没有 `editable`，禁用只能靠"清空 + 入口 return"。
5. **`YButton` 恒 `disabled`**（`docs/92` §4.10 观测）：回放模式恢复输入时不能把它一起开回来
   —— 既有的那条断言立刻会红。

### 5.5 构建与产物（照 `HANDOVER` §0 的两步）

```bash
export CARGO_HOME=<ws>/.cargo-home; export CARGO_TARGET_DIR=<ws>/.agent-work/replay/target
cargo build --offline && cp -f $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
tools/Godot_v4.7.2-stable_linux.x86_64 --headless --path client --import   # 新增 .gd 后**必须**
bash client/run.sh selftest
bash client/run.sh dump | head -1
```

`dump` 确认内核仍在线：`@@KERNEL available=1 hit_test=1 methods=…path_calculate,hit_test,level_obstacles…`。

### 5.6 顺带发现：`FunctionPoints.end_x` 是弹道的**停点** x

把每一发按"函数文本 + 起点 + 队伍方向 + 这一局的模式"重算，再和录像里的 `end_x` 比：

| 分组（这发之前有没有障碍 / 模式） | 最后一点 x **逐位相同** | 最大差 |
|---|---|---|
| Normal / 无障碍 | 148 / 200 | 48.7 |
| Normal / 有障碍 | 254 / 537 | 45 |
| Ode / 无障碍 | 280 / 312 | 0.525 |
| Ode / 有障碍 | 111 / 234 | 45 |

合计 **793 / 1290 发逐位相同**。抽查不同意的样本（前 12 例）里 **10 例**这一发实际
**穿进了障碍**（`crosses_obs=True`）⇒ `end_x` 是**停点**（被墙拦住时的撞点），
不是"整条数学路径的最后一点"。剩下 2 例是 1e-4 量级的小差（`24.9999` vs `25.0000`），
疑似像素/精度量化 —— **未证实**。

⇒ 因此：**我们不用 `end_x` 画弹道**（画整条路径，遇障碍按 `hit_test` 截断 ——
与游戏判停是同一套掩膜），只把它当**验证基准**。上文两个样本（Level 0 的 Normal 局、
Level 16 的 DiffEq 局）都**没有障碍**，所以它们 6 发的 `end_x` 全部逐位相同，
可以直接当断言。
**仍未确定**：有障碍的那 483 发为什么不是"截断后的停点"—— 客户端确实按 `hit_test` 截断，
但没有逐发核对过（见 §6）。

---

## 6. 仍未确定

| # | 未知 | 缺什么证据 | 影响 |
|---|---|---|---|
| 1 | **原版回放界面**（有没有进度条、能不能拖、倍速/跳转语义） | 原版真帧 / 原版脚本。恢复出的 `.tscn` 只有 13 + 21 个静态节点，播放控件的行为在 Rust 里 | 我们的播放控件全是自定（§4.3 已逐项标注） |
| 2 | 原版**播放时钟**：是按 tick 还是按事件时间轴 | `graphwar_replay_player.rs`（8 个函数）/`graphwar_replay_control.rs`（2 个函数）**整块没逆** | 我们按 tick（`docs/35` 的真值）—— 若原版按事件，进度语义会不同 |
| 3 | ~~微分方程局的弹道~~ | ✅ **已解决**：内核 `path_calculate_mode(…, mode: `"normal"`/`"ode"`)`（`agent/ode-api` 交付，`docs/138`）＋本文的「按录像的 `FunctionModeInfo` 选模式」接线；真实 DiffEq 局 4 发的 `end_x` 全中（§5.3 第 7 组、§5.6） | — |
| 4 | 谁是 **bot** | `NameInfo` 只给名字，`docs/79` §4 说"名字↔难度"这一步没做完 | 回放里士兵一律按真人画（皮肤从 `SkinGraphicsInfo` 反查，头像/帽子名未映射） |
| 5 | **gzip 尾部的 CRC32/ISIZE 不校验** | 有意省略（回放不是校验场景） | 损坏文件可能"解出一半"而不是报错；DEFLATE 结构错误仍会报 |
| 6 | 回放里 `end_x` 的语义 | 793/1290 发与重算路径的最后一点逐位相同；其余多数是"被障碍拦住"（撞点），少数是 1e-4 量级的小差（§5.6）。**没有逐发核对过**"按 `hit_test` 截断后是否等于 `end_x`" | 我们**不用**它画弹道（只当验证基准），所以没有功能影响，但**别拿它当命中点/终点坐标** |
| 7 | 多 member gzip / 未压缩回放 | 真实语料里 0 例，代码支持但**没有真值样本** | 属"支持但未验证" |
