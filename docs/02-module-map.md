# 02 · 模块地图

来源：`.so` 里 Rust panic 保留的 `src/<file>.rs:<line>` 字符串。
共约 **90 个 `graphwar_*.rs`** 模块。下表"职责"列除标注【已验证】者外均为
**按模块名与行号规模推断**，不构成行为结论。

## 1. 核心算法（已验证）

| 模块 | 行号下界 | 已知地址 | 职责 |
|---|---|---|---|
| `graphwar_function_input.rs` | ≥519 | `0x4ed670` 解析 | 文本 → 前缀 token【已验证】 |
| （求值器，同源） | — | `0x4eb9d0` | token → f64【已验证】 |
| （常量折叠） | — | `0x4eb830` | 纯常量子树折叠【已验证】 |
| `graphwar_path_calculator.rs` | ≥86 | `0x3c2530` | 点序列采样【已验证】 |
| `graphwar_shot_calculator.rs` | — | `0x3c2bc0` | 开火结算【调用链已验证】 |
| （修正常数） | — | `0x3c5f70` / `0x3c5c50` | `c = y0 − f(±x0)`，拼 `"f(x)+c"` 串 |

## 2. 场景与坐标

| 模块 | 职责（推断） |
|---|---|
| `graphwar_root_node.rs` (≥1302) | 主场景节点，游戏总控，规模最大 |
| `graphwar_plane.rs` | 坐标系、绘制 |
| `graphwar_plane_pos_manager.rs` | 坐标↔位置换算 |
| `graphwar_draw_util.rs` | 绘制工具 |
| `entity_drawer.rs` / `effect_drawer.rs` | 实体/特效绘制 |
| `graphwar_graphics_manager.rs` | 图形资源管理 |
| `graphwar_texture_loader.rs` | 贴图加载 |

## 3. 对战流程

| 模块 | 职责（推断） |
|---|---|
| `graphwar_turn_manager.rs` | 回合推进、当前玩家 |
| `graphwar_game_state_manager.rs` | 一局的状态机 |
| `graphwar_mode_manager.rs` | 游戏模式（经典/ODE/…） |
| `graphwar_team_manager.rs` | 队伍 |
| `graphwar_player_manager.rs` / `graphwar_local_player_manager.rs` | 玩家 |
| `graphwar_soldier_manager.rs` | 士兵实体 |
| `graphwar_life_manager.rs` | 生命/死亡 |
| `graphwar_target_manager.rs` | 目标选择 |
| `graphwar_obstacle_manager.rs` | 障碍物 |
| `graphwar_level_manager.rs` / `graphwar_level_generator.rs` | 地图生成 |
| `graphwar_function_manager.rs` | 每个玩家的函数存储（原串/修正串） |

## 4. AI

| 模块 | 职责（推断） |
|---|---|
| `graphwar_computer_player_manager.rs` | 电脑玩家调度 |
| `graphwar_function_evolutionator.rs` | **函数进化**：疑似对候选表达式做变异/选择，用于 AI 找射击函数 —— 优先逆向目标 |

## 5. 联机

| 模块 | 职责（推断） |
|---|---|
| `graphwar_event.rs` (≥1154) | 事件枚举（最大规模的协议定义） |
| `graphwar_remote_event_handler.rs` (≥465) | 远端事件分发 |
| `graphwar_event_handler.rs` | 本地事件处理 |
| `graphwar_game_server.rs` (≥336) | 房间内权威服务端 |
| `graphwar_game_client.rs` | 客户端 |
| `graphwar_server_broker_handler.rs` (≥94) | 房间撮合 |
| `graphwar_server_connection_handler.rs` (≥196) | 连接管理 |
| `graphwar_client_connection_handler.rs` | 客户端连接 |
| `graphwar_lan_broadcaster_v6.rs` (≥126) | 局域网广播（v6） |
| `graphwar_lan_vbox.rs` | 局域网列表 UI |
| `graphwar_backend.rs` (≥142) | 后端接入 |

## 6. 界面与外围

| 模块 | 职责（推断） |
|---|---|
| `graphwar_play_screen_control.rs` (≥561) | 对战界面逻辑 |
| `graphwar_player_screen_control.rs` | 玩家界面 |
| `graphwar_setup_control.rs` / `graphwar_lobby_control.rs` | 开局/大厅 |
| `graphwar_campaign_control.rs` | 战役模式 |
| `graphwar_replay_control.rs` / `graphwar_replay_player.rs` | 回放 |
| `graphwar_button.rs` / `graphwar_long_press_animation.rs` | UI 控件 |
| `graphwar_audio_control.rs` (≥241) | 音频 |
| `graphwar_name_manager.rs` / `graphwar_color_manager.rs` / `graphwar_rank_manager.rs` | 名称/配色/段位 |
| `graphwar_tick_handler.rs` | 每帧 tick |
| `graphwar_game_recorder.rs` | 对局录制 |
| `graphwar_video_writer.rs` | 录像导出 |
| `graphwar_crash_reporter.rs` (≥100) | 崩溃上报 |
| `graphwar_logger.rs` / `graphwar_nodes.rs` / `graphwar_module_container.rs` | 基础设施 |
| `graphwar_ad_switch.rs` / `android_iap.gdc` | **广告 / 内购**（`19-android.md`：判据只有 `are_ads_enabled() = !owned`） |
| `graphwar_backend.rs` | **联机层，不是广告**（15 个方法名：`start_server` / `start_client_to_*` / `lan_broadcaster` / `broker` / `get_next_clip_checkpoint`…，且**不是 Godot 类**）——`19-android.md` 更正 |

## 7. 第三方 crate（同名 `src/` 路径，勿混淆）

`src/parser.rs`、`src/uts46.rs`（idna）、`src/zio.rs`（flate2）、`src/time.rs`、
`src/socket.rs`、`src/types.rs` 等属于依赖库，不属于游戏本体。

## 8. 观测结果：函数 → 模块（首批）

**方法**（无 Ghidra，纯静态，2.5 分钟）：

1. 在二进制里扫 `src/<file>.rs` 字符串，算出其虚拟地址；
2. Rust 的 panic `Location` 是 `{ptr, len}` 结构体，字符串地址存在 `.data.rel.ro`
   的 `R_X86_64_RELATIVE` 重定位 addend 里 —— 于是反查"哪个指针槽指向这个字符串"；
3. 取引用该槽位的指令（capstone 建的 rip-relative 交叉引用）；
4. 用 `.eh_frame` 恢复的函数边界判定指令属于哪个函数。

产物：`fixtures/module_map.tsv`（脚本 `tools/module_map.py`）。

**观测到的规模**（游戏自身模块，按归属函数数排序）：

| 模块 | 归属函数 | 引用点 | 模块 | 归属函数 | 引用点 |
|---|---|---|---|---|---|
| `graphwar_game_client.rs` | 68 | 311 | `graphwar_plane.rs` | — | — |
| `graphwar_game_state_manager.rs` | 50 | 256 | `graphwar_function_input.rs` | — | — |
| `graphwar_level_manager.rs` | 40 | 590 | `graphwar_nodes.rs` | — | — |
| `graphwar_setup_control.rs` | 37 | 82 | `graphwar_play_screen_control.rs` | — | — |
| `graphwar_root_node.rs` | 24 | 186 | `graphwar_lan_vbox.rs` | — | — |
| `graphwar_mode_manager.rs` | 21 | 46 | `graphwar_client_connection_handler.rs` | — | — |
| `graphwar_module_container.rs` | 20 | 20 | `graphwar_level_generator.rs` | — | — |
| `graphwar_function_evolutionator.rs` | 16 | 39 | `graphwar_audio_control.rs` | — | — |

合计 **56 个游戏模块、544 个函数归属**（约占 13263 个可分析函数的 4.1%）。

> ⚠ 本表已按 §12 修正后的函数边界重算；§12 之前的旧数字（483 / 3.6%）作废。

**这个数字为什么小**：只有带 `Location` 的 panic / `unwrap` / 索引越界点才留锚。
没有 panic 点的函数（纯计算、被内联的叶子函数）不会出现在表里。

**下一步**：以这些锚点做种子，沿调用图扩散（被 A 调用的函数按 A 的模块归属），
并标注为"推断"而非"观测"。这一步能覆盖大部分剩余函数，但必然带噪声。

顺带可读出的结构信息：`graphwar_game_client.rs` 归属函数最多（72），说明联机客户端
逻辑远比"一个客户端类"要重；`level_manager` 的引用点密度最高（590 次 / 36 函数），
说明它对配置项的读取非常频繁。

## 9. 第三方 crate 的干扰

同一套方法也会命中依赖库（`time.rs`、`weak_caller.rs`、`entity_drawer.rs` 等），
它们**不是**游戏代码：文件名没有 `graphwar_` 前缀。统计时先按前缀过滤。
注意 `entity_drawer.rs` / `effect_drawer.rs` 虽不带前缀，但确实是游戏自己的绘制模块
（见 §2），这类要靠人工判断。

## 10. 调用图扩散（推断层）

以 §8 的 376 个锚点做种子，沿**直接调用边**扩散（`tools/propagate.py`）。
全库共解析出 **34326 条**直接调用边。

规则刻意保守：未标注函数要至少 2 条已标注邻居边、且同一模块占 ≥ 2/3 才被标注；
单一邻居的一律不标。

| 轮次 | 新增 | 累计 |
|---|---|---|
| 1 | 169 | 610 |
| 2 | 182 | 792 |
| 3 | 93 | 885 |

**结果：885 / 13263（6.7%），其中观测 441、推断 444。**

| 模块 | 观测 | 推断 | 合计 |
|---|---|---|---|
| `graphwar_root_node.rs` | 24 | **218** | 242 |
| `graphwar_game_client.rs` | 43 | 70 | 113 |
| `graphwar_obstacle_manager.rs` | 8 | 42 | 50 |
| `graphwar_setup_control.rs` | 37 | 9 | 46 |
| `graphwar_lan_vbox.rs` | 10 | 34 | 44 |
| `graphwar_function_evolutionator.rs` | 16 | 12 | 28 |

**必须说明的偏差**：`root_node` 从 16 暴涨到 194，不是因为它真有 194 个函数，
而是它作为主场景节点**调用几乎所有模块**——扩散法对 hub 节点天然高估。
所以 §10 的数字只能用于"排序参考"，不能当作模块规模结论；
真正的规模要靠反编译后逐个确认。

`plane` / `turn_manager` / `level_manager` 这些**没有**被高估（推断数为 0 或 1），
说明它们的调用关系比较封闭，可信度反而更高。

## 11. 逐模块反编译的入口

`fixtures/module_funcs.tsv` 给出每个模块的函数地址清单（观测层，474 条），
可直接喂给反编译器。

**地址换算**：这张表用的是 ELF 虚地址；Ghidra/REA 的地址 = 虚地址 **+ 0x100000**。
例如 `graphwar_plane.rs` 的首个函数 `0x4b1784` → REA 里是 `0x5b1784`。

`graphwar_plane.rs` 的 10 个函数（虚地址，已按 §12 的 Ghidra 边界重取）：

```
0x4b2260 0x4b23e0 0x4b5df0 0x4b7a70 0x4b83f0
0x4bc490 0x4bc6b0 0x4bc860 0x4bcab0 0x4be3e0
```

各函数的规模与已确认的语义见 [`08-plane.md`](08-plane.md)。

## 12. 函数边界的数据源修正（重要）

**本节推翻了本仓库第一版的地基。**

### 12.1 发现

上游 `analysis/build_index.py` 依赖 `funcstarts.pkl`（从 `.eh_frame` 恢复函数起点）。
用它做归属时，我注意到 Ghidra 反编译出的函数入口与它**对不上**，于是做了三项交叉检查：

| 检查 | 结果 |
|---|---|
| 旧表 13430 个起点 与 Ghidra 13503 个起点 的交集 | **只有 2 个** |
| 旧表起点处是否存在指令边界 | 否（如 `0x4b1784` 处的指令根本不存在） |
| Ghidra 起点处是否是函数序言 | 是（如 `0x4b1800` = `push r15; push r14`） |
| 旧表地址是否落在 `.text` 内 | 否，最大值 `0x969054` **超出** `.text` 末尾 `0x94eb84` |

结论：`.eh_frame` 那份解析**是坏的**，Ghidra 的边界才是真的。

### 12.2 地址换算（现在钉死了）

- `Ghidra 地址 = 虚地址 + 0x100000`
- 验证：13503 个 Ghidra 函数里 **13263** 个落在 `.text` 的换算区间
  `[0x47b940, 0xa4eb84]` 内；其余 240 个是外部/其他块。

### 12.3 影响与处置

- 之前"线性反汇编与 `.eh_frame` 有字节级漂移"的说法**因果搞反了**：
  漂移的是那份 FDE 表，不是反汇编。已在 `06-verification.md` §11 更正。
- 归属流程改为：Ghidra 函数表 →（减 `0x100000`）→ 虚地址边界 → 与 capstone
  指令地址对齐。转换结果落在 `analysis/funcstarts_ghidra_va.pkl`（13263 条）。
- §8 / §10 的数字已按新边界重算；旧数字（483 / 628）作废。

### 12.4 仍然成立的结论

模块↔字符串的**关联**不受影响（那是"字符串被谁引用"，只看 xref，不需要函数边界）。
受影响的只是"某个函数算哪个模块"的判定，因此 §8/§10 之外、
依赖具体函数地址的结论（如 §11 里的 plane 地址清单）都需要按新边界重取。

## 13. 首批逐模块反编译观测（含负结果）

> 来源：早期按模块逐篇记录的四份文档（原 `10`/`12`/`13`/`14`），2026-11 文档归并时并入本节，
> 原文件移入 `docs/archive/`。**本节只收"能指到地址"的观测与负结果**；
> 语义若后来被解出，"后续"列会给出现在的权威文档。
>
> 地址约定：除特别标注外都是**虚地址**；Ghidra/REA 地址 = 虚地址 + `0x100000`，
> **只能加一次**（踩过两次，见 `90` §2.6/§2.7）。

### 13.1 `graphwar_life_manager.rs`（原 `10`）

Ghidra 边界下 **9 个函数**，全部反编译成功：

| 虚地址 | 规模 | 备注 |
|---|---|---|
| `0x453a60` | 167 行 | |
| `0x453c60` | 93 行 | |
| `0x453de0` | 229 行 | 本模块最大 |
| `0x454020` | 48 行 | |
| `0x454120` | 204 行 | |
| `0x454420` | 194 行 | |
| `0x454790` | 84 行 | |
| `0x454950` | 167 行 | 存活判定（`&1` 为真才算活，`AI.md` §8.4） |
| `0x454b40` | 104 行 | 入口迹象 |

另有 3 个 33 行的极小函数（`0x554730/750/770`）不带模块锚，归属未知。

外部调用者（`tools/callers.py`，只覆盖直接调用）：

| 调用者（虚地址） | 次数 | 模块 |
|---|---|---|
| `0x4759f0` / `0x473520` / `0x472470` / `0x470b70` / `0x476a20` | 5 | `graphwar_game_state_manager.rs` |
| `0x4b4b60` / `0x517690` / `0x4b2890` | 3 | `graphwar_game_client.rs` |
| `0x3e2830` | 1 | `graphwar_target_manager.rs` |
| `0x463da0` | 1 | `graphwar_turn_manager.rs` |
| `0x4500c0` | 1 | `graphwar_server_connection_handler.rs` |

**结构观测**：每个入口先拿一个 `RwLock`，再在锁内操作**两张表**：

```c
FUN_005554b40(lock, id, a, mode, b):
    lock(...)
    FUN_00540d40(lock->0x18, id, a);        /* 表一：无条件写 */
    if (mode == 1)
        FUN_00542330(lock->0x48, id, b);    /* 表二：仅当 mode == 1 写 */
    unlock(...)
```

`0x540d40` / `0x542330` 都是 HashMap 操作（`0x646f72616e646f6d` 是 Rust HashMap 的
SipHash XOR 常量）。当时的推断是「表一 = 存活状态、表二 = 排队死亡」——
**后续：这个推断成立**，死亡在协议上就是「计划 tick + 到期生效」两条（见 `65` §1）。

**后续**：致死规则已结案（无血量 / 友军致死 / 按弹道先后，用户实测 + 我方实现一致，`65` §0）。
仍然是未知：伤害是否有数值、`0x3c2bc0` 下游怎么接到 `life_manager`、`target_manager`
为什么也写生命表（`00-status` §3）。

### 13.2 `graphwar_function_evolutionator.rs`（原 `12`，结论已被 `AI.md` 取代）

> ⚠️ 原 `12` 的标题是"评分规则未还原"，现在**已全部还原**：参数表、权重、长度、
> 适应度、装配、变异、每代构成、两阶段评分、25 s 预算都在 [`AI.md`](13-ai.md)（唯一权威）。
> 本节只保留 `AI.md` 里没写的**模块级观测**。原文地址空间混写（vaddr 当 Ghidra 写），
> 统一口径见 `AI.md` §8.6：**评分核心 = vaddr `0x51a750` = Ghidra `0x61a750`**。

原 `12` §1 列出的 16 个函数（数值按**虚地址**读）：

```
0x41a9d0 0x41ab40 0x41b500 0x41b720 0x41b880 0x41bdf0
0x4600e0 0x4602f0 0x481140 0x481870 0x4825b0 0x482c20
0x51a750 0x51b440 0x51d080 0x51eb90
```

其中末尾四个是主体（`0x51a750` 353 行、`0x51b440` 992 行、`0x51d080` 1001 行、
`0x51eb90` 853 行）。

**两个负结果（有价值，省得重走）**：

1. 这四个主体函数里**没有任何浮点字面量**（唯一的 `1.93e25` 是 `u64` 饱和转换的边界常量）。
   ⇒ 排除了"评分权重硬编码在代码里"这一路；权重实际来自配置结构体（后由 `AI.md` §2 定性）。
2. 原 `12` §5 想让 Ghidra 把 `(long)&UINT_002c63a0`（虚地址 `0x1c63a0`，`.rodata`）
   识别成 switch 跳转表，**失败了**：`jump_tables: []`，分支清单拿不到。
   ⇒ 该跳转表至今**没有**被解成 case 列表，属未完成路径。

**调用形态**：`0x61a750` 几乎不做直接调用，分派走「跳转表 + 函数指针槽」两处；
另一个观测是它拿了开火结算 `0x3c2bc0` 的指针槽——这条后来被 `98` 证实为
"AI 把候选函数当一次真实射击跑一遍"（见 `AI.md` §4、`06` §12）。

### 13.3 `graphwar_turn_manager.rs`（原 `13`）

Ghidra 边界下 **14 个函数**，全是小函数（33～236 行）：

| 虚地址 | 行数 | 直接调用数 |
|---|---|---|
| `0x4628d0` | 236 | 4 |
| `0x4627c0` | 83 | 4 |
| `0x462c10` / `0x462ec0` | 68 / 68 | 6 / 6 |
| `0x462d20` / `0x462fe0` | 81 / 80 | 8 / 8 |
| `0x463170` | 89 | 8 |
| `0x463340` / `0x463460` | 67 / 67 | 7 / 7 |
| `0x463580` / `0x463790` | 85 / 69 | 10 / 7 |
| `0x463da0` | — | 核心：轮转发 `TurnInfo`（后由 `AI.md` §8.4 解出，见下） |
| `0x46f690` | — | 位置孤立 |

外部调用者 22 个：`game_state_manager`(5)、`target_manager`(1 处 3 次)、
`mode_manager`(3)、`play_screen_control`(1 处 2 次)、`server_connection_handler`(1 处 2 次)、
`game_client`(1)。

**负结果（省一类死路）**：这 14 个函数里**没有任何浮点字面量**（连 `1000.0` 这种毫秒换算都没有）
⇒ 回合时限、超时判定**不在代码里硬编码**，是服务端下发的数据
（与 `TurnLimitInfoEvent{turn_limit}`、`TimeModeInfoEvent{time_mode}` 一致）。

**形态**：最大的 `0x4628d0` 通篇是 `0x28` 步长的开放寻址循环，外层套 `RwLock`。
**后续**：那张 `0x28` 步长的表 = **玩家 → `{Vec<soldier_id>, index}`**
（射手轮转用，`98` §2 已从 `0x4628d0` 读死）；状态机见 `41`、报文序列见 `43`。

### 13.4 `graphwar_target_manager.rs`（原 `14`，**仍未解**）

模块共 **5 个函数**：`0x3e2830`、`0x465780`、`0x465870`、`0x465910`、`0x46f310`。

枢纽 `0x3e2830` 的直接调用者只有 4 个：

| 调用者 | 模块 | 含义 |
|---|---|---|
| `0x3d61a0` | `graphwar_tick_handler.rs` | **每帧/定时触发** |
| `0x459690` / `0x459f20` | `graphwar_client_connection_handler.rs` | 网络事件到达 |
| `0x3d7380` | `graphwar_replay_player.rs` | **回放走同一条路径** |

三条观测结论：

1. 它是**周期性更新**，不是"玩家点一下就调用一次"；
2. **回放与实时对局复用同一段逻辑**——这解释了回放为什么能精确重现对局；
3. 网络事件也驱动它。

~~`0x465780`（95 行）是典型 SIMD 字节扫描……当时的推断是"在扫 1000×600 掩膜找第一个命中像素"。~~
**❌ 这条推断已被 `15-target-life.md` §6 证伪**：`0x465780` **不是**掩膜扫描，而是
**hashbrown 集合迭代 + 逐个 id 发 `TargetInfo`**（新连接进来时补发已有目标）。
全二进制只有 2 处构造 `TargetInfo`：`0x46f3af`（造目标）与 `0x46580e`（补发）。
当年"读不懂"正是因为它根本不是那件事。

⚠️ 同节把 `0x3e2830` 归到 `target_manager` 的依据是 panic 串启发式——而那套启发式
已被 `10-campaign.md` §12.6 判为**不可靠**（一个函数引用两个模块的 Location 会出两行）。
用行号法复核只剩 1 条，且可由内联解释 ⇒ **归属存疑**。

**为什么停在这里（诚实记录）**：这几个函数都很小（30～95 行），但反编译产物**基本不可读**——
SIMD 内联把 20 行 C 展开成 60 行位运算，`SUB161(...)`/`auVar` 满屏。再往下读要换手段
（按 16 字节向量反推原始 C，或用 `read_bytes` 看缓冲区首地址与长度）。

**若要继续**：
1. 查 `0x465780` 里缓冲区指针的来源，拿到 `(data_ptr, len)` 的实际常量
   → 能直接判定它扫的是不是 1000×600 掩膜；
2. 查 `0x3e2830` 调用点前后传了什么参数（尤其是否带 `soldier_id`）；
3. `0x28` 步长那张表已由 `98` 解出（不是"目标候选集"，原 `14` §5 的猜测**可以划掉**）。

### 13.5 一条贯穿三个模块的形态观测

`life_manager`、`turn_manager`、`function_evolutionator` 都是**同一套路子**：
`RwLock` + 若干张 HashMap，没有明显的面向对象层次。
所以这里的"模块"更像"一组围绕同一张表的小函数"，不是类。

## 14. 模块级覆盖审计：54 个模块，479 个函数连地址都没落过纸（2026-11）

### 14.1 为什么要做这个

用户反复问"所以到底漏了什么"。此前唯一的答案是 `06-verification.md` 的**模式维度**矩阵
（`TurnMode × FunctionMode × TimeMode × AxisMode`），它回答的是"我这些结论在哪个
模式下成立"，**没有回答"哪些代码我从没看过"**。§1–§7 的表全是按模块名**推断**的职责，
不是审计。

工具：`tools/module_audit.py`（输入全是已有夹具，不重新反汇编）。

### 14.2 口径，以及它为什么不能当完成度

> ⚠️ **2026-11-06 更正（`10-campaign.md` §2）**：本节的**基础数据 `fixtures/module_funcs.tsv` 不能用来判定函数身份**。
> 那张表是按 `src/<file>.rs:<line>` 的 panic 串启发式归堆的，**一个函数只要引用了两个模块的
> `panic::Location` 就会在那两个模块下各出现一行**（全表 75 个地址如此，例如 `0x466d20`
> 同时被归到 `plane_pos_manager` 和关卡相关模块）。
> ⇒ 本节的分档**只能当"某模块名下的函数里有多少被文档提过地址"的粗估**，
> **不能**据此说"这个函数属于哪个模块"，更不能据此判断"某个功能没做"。
> 真实例子：`0x466d20` 被本表归到 `graphwar_plane_pos_manager.rs`，
> 实际它是**关卡开场驱动**（`10-campaign.md` §2）。

`cited` = **函数起始地址在 `docs/**/*.md` 里被写下来过**。它测的是"我有没有把地址
落在纸面上"这个**记录习惯**。

两个方向都实测过噪声：

* **假阴性**（懂了但没写地址）：`graphwar_function_input.rs` 映射到 13 个函数，
  引用数 **0** —— 可解析器 `0x4ed670` 是**逐位验证**过的，它根本不在
  `fixtures/module_funcs.tsv` 里（该表只收"有 panic 字符串被引用"的函数）。
  `graphwar_plane_pos_manager.rs` 同理：`px = 20x + 500` 是常量反查得到的，文档里
  一个 `0x3c62xx` 都没有。
* **假阳性**（写了地址但没读懂）：只被 xref 表连带提到的地址也计数。

⇒ 所以下面这张表**只能**用来找"大面积、连地址都没出现过"的模块。`NO-CITE` 不等于
"没看过"，要下结论必须逐条人工复核。

### 14.3 分档

| 档 | 模块数 | 函数数 | 占比 |
|---|---|---|---|
| `core`（引用率 ≥ 40%） | 18 | 188 | 21.2% |
| `partial`（15%–40%） | 4 | 53 | 6.0% |
| `thin`（> 0 但 < 15%） | 4 | 165 | 18.6% |
| `NO-CITE`（一个地址都没写过） | **28** | **479** | **54.1%** |

另有全二进制口径：**13263** 个函数里，文档写下过地址的 **168** 个 = **1.27%**。
（分母混着 Rust std / hashbrown / Godot 绑定，所以这个数注定很小，
**不能**读成"只做了 1.3%"。它只能说明"绝大多数函数我连提都没提过"。）

### 14.4 28 个 `NO-CITE` 模块，按域归堆

| 域 | 模块（函数数） | 小计 | 说明 |
|---|---|---|---|
| **界面 / 呈现** | `root_node`(242)、`setup_control`(46)、`lan_vbox`(44)、`nodes`(14)、`long_press_animation`(8)、`player_screen_control`(8)、`play_screen_control`(7)、`graphics_manager`(7)、`button`(6)、`texture_loader`(1) | **383** | 客户端 agent 正在做。注意：**界面行为主要写在 `.gdc` 脚本里（105/105 已反编译），这些 Rust 函数是脚本底下那层原生调用**——所以"没引用过 Rust 地址"不等于"界面没还原" |
| **对局元信息** | `function_manager`(7)、`rank_manager`(4)、`name_manager`(3)、`local_player_manager`(3)、`event_handler`(3)、`logger`(3)、`campaign_control`(1)、`lobby_control`(1) | **25** | 真盲区。`name_manager` / `rank_manager` 对应 `07-protocol.md` 里缺的 `NameInfo`/`RankInfo` 一组事件 |
| **音频** | `audio_control`(14) | **14** | `.so` 里有完整的 WAV 解析错误串（`no WAVE tag found`、`no RIFF tag found`、`bits per sample is not 8 or 16`…），说明**音频是运行时解析 WAV 的**。与用户说的"有一个文件损坏"直接相关：坏文件会在这里被拒 |
| **广告 + 联机后端** | `ad_switch`(7)、`backend`(6) | **13** | 见 §14.5。⚠️ **`backend` 不是广告，是联机层**（`19-android.md` §3） |
| **录像 / 回放 / 视频** | `replay_control`(3)、`video_writer`(3)、`game_recorder`(2) | **8** | 我们只做了**读**回放（`tools/parse_replay.py`），没做**写**。注意还有 `publish_video_to_album` / `Android_mediastore_holder`——"把回放发到相册"是 Android 专属 |
| **联机外围** | `lan_broadcaster_v6`(4)、`server_broker_handler`(3) | **7** | 房间发现与 broker，见 `09-network.md` §7 |
| **已知假阴性** | `function_input`(22)、`plane_pos_manager`(7) | **29** | §14.2 说的两个反例 |

### 14.5 顺带确认的两件事

1. **广告 / 内购在 `.so` 里是有代码的**，不是"Linux 上不存在"这么简单：
   字符串里有 `IAP_holder`、`Ad_holder`、`purchase_pending` / `purchase_confirmed` /
   `purchase_denied`、`create_ad_view` / `destroy_ad_view`、`request_permissions_result`、
   `unlock_skins`、`purchase_graphwar_support`。
   这些是 Godot 的 **Android 单例**调用（`Android_mediastore_holder` 同类），
   ⇒ **Linux 端跑不到**（用户判断正确），但**代码路径在**，要看行为得走 APK。
2. **段位图标共 22 张**（`rank_0` … `rank_21`，观测）。⚠️ 包里**没有 `.svg`/`.ttf` 文件**——
   导出时源资源已剥离，包里只有 `.ctex`（编译后纹理）与 `.fontdata`；
   那些 `res://.../xxx.svg` 路径是 `.import/.remap` 存根里的**源路径**（`16-client.md` §4）。
   ⚠️ **本节原先由它推出的两条结论已被 `02-module-map.md` 推翻一半**：
   - "22 张图标 ⇒ 22 个段位"**不成立**：`rank_manager` 的 4 个函数里**没有任何**
     22 项表、取模或阈值比较，段位换算**不在这个模块**（可复现的负结果）。22 档位仍未知。
   - `user://campaign.save` 的引用点**不在** `0x418820`，而在 `0x418170` / `0x4183b0`
     ⇒ "那 1 个函数只做存取"的说法证据链要换：方向对，但地址指错了。
   详见 `02-module-map.md` §14。

### 14.6 这份审计没做的事

- 没区分"函数属于游戏逻辑"还是"编译器/库内联产物"——`module_funcs.tsv` 已经是
  "能归到 `graphwar_*.rs` 的"，但传播层（`module_map_propagated.tsv` 的 `inferred` 列）
  会把邻居也算进来，所以 885 这个总数是**上界估计**；
- 没评价"看懂了多少"，只评价"提没提过地址"；
- `NO-CITE` 的 28 个模块**没有逐个反编译**，§14.4 的"说明"列是据名字与同族字符串的推断。

---

## 15. 对局元信息八模块（23 函数）——原 `112-meta-managers`

> 这一节补的是 §14 里归为「对局元信息 25 个函数、`NO-CITE`」的那一堆（本文覆盖其中 23 个）。
> 全文与逐地址证据：[`archive/112-meta-managers.md`](archive/112-meta-managers.md)。

### 15.1 一页结论

1. **名字存哪**（Q1）：`HashMap<u32 /*player_id*/, String>`，
   **entry 32 字节**、key 是 u32 @entry+0、`String` @entry+0x8（cap/ptr/len = +0x8/+0x10/+0x18）。
   名字本身是 Rust `String`，**不是** Godot `GString`。〔逐位〕
2. **`NameInfo` 的字段**（Q1）：serde 原文 `struct NameInfoEvent with 2 element`。
   线上事件 `name_info_event` 的判别值 = **60（0x3c）**，payload = `String` @+8 + `u32` @+0x20。
   两处独立命中（构造处 + 反查表），见 §4。〔逐位〕
3. **`RankInfo` 的字段**（Q2）：判别值 = **67（0x43）**，serde 原文也说 2 个元素，
   但 `rank_manager` 造出来的 payload 写了 **3 个 dword**（+8/+0xc/+0x10）。
   **这是一处未收敛的矛盾，按原样记录，不强行圆。** 见 §5.4。〔观测冲突〕
4. **段位怎么算**（Q2）：**`rank_manager` 里算不出来**。4 个函数里没有任何除法、取模、
   阈值比较或 22 项常量表；它只是一个 `HashMap<u32,u32>` 的存取 + 广播。
   22 张 SVG 对应"段位编号 0..21"这一点在本模块内**无法**验证。〔观测（负结果）〕
5. **每个士兵的函数存哪**（Q3）：`HashMap<u32 /*player_id*/, 112 字节>`，
   **entry 120（0x78）字节**，value 开头就是一个 `Vec<Token>`（元素 16 字节）。
   `Token = 16 字节` 这一条与 `AGENTS.md` 的硬事实**对得上**。〔逐位〕
6. **战役进度**（Q4）：`user://campaign.save` 确实存在（0x1bfe28），但它**不**被
   `graphwar_campaign_control.rs` 唯一那个函数（0x418820）引用，而是被 0x418170 / 0x4183b0 引用；
   这 2 个函数没有 panic 串、**无法证明**属于 `campaign_control.rs`。〔观测 + 归属未知〕
7. **副产品**：顺手把 `.rodata` 里的**事件名全表 + 判别值对应关系**解出来了（§3、§4.2）。
   `07-protocol.md` §4 说"判别值不上线、不用管"——**结论仍成立**，但它现在不再是"不可验证"了。

---

### 15.2 地址总表

`vaddr` 为 ELF 虚地址；`Ghidra = vaddr + 0x100000`。`size` 由 Ghidra 入口表给出。
`line` 来自该函数内引用的 `panic::Location`（`src/<file>.rs:LINE`），**是观测**，
不是"函数在源文件第几行开始"，但足以给出源文件内的相对次序。

#### 15.2.1 `graphwar_function_manager.rs`（7，`NO-CITE`）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x455b60` | `0x2d0` | 165 | **取**某 player 的 token 向量（克隆 `Vec<Token>` 返回） | 观测→逐位（`shl r15,4` + `memcpy` 16N） |
| `0x455e30` | `0x7c0` | 152/153/158/161 | **设**：两个 `&str` 各喂一次解析器 `0x4ed670`，再发事件 tag 35 | 观测 |
| `0x4565f0` | `0x220` | 169 | 同类存取（与 0x455b60 的 self 骨架完全同形） | 推断 |
| `0x456810` | `0x370` | 192/199 | 按 player_id 在 `X+0x38` 的 map 上查一条并写字段 | 观测 |
| `0x456b80` | `0x270` | 181/186 | 同上，另一个字段 | 推断 |
| `0x456df0` | `0x190` | 208 | 同形，偏移 0x20/0x28/0x30/0x38/0x40/0x48 | 推断 |
| `0x456f80` | `0x190` | 213 | 与 `0x456df0` **偏移直方图完全一致**（疑似同构的另一字段/另一模式） | 推断 |

#### 15.2.2 `graphwar_name_manager.rs`（3）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x518f10` | `0x120` | 69 | 遍历 `HashMap<u32,String>`，逐条发 `name_info_event`(60) | 逐位 |
| `0x519030` | `0x90` | 79 | **处理 `entity_removed_event`(14)**：从 map 里删掉该 player | 逐位 |
| `0x519350` | `0x100` | 57 | **设名**：`map.insert(player_id, String)` + 发 `name_info_event`(60) | 逐位 |

#### 15.2.3 `graphwar_rank_manager.rs`（4）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x454d80` | `0x100` | 73 | 遍历 `HashMap<u32,u32>`，逐条发 `rank_info_event`(67) | 逐位 |
| `0x454e80` | `0x70` | 83 | 处理 `entity_removed_event`(14)：从 map 删 | 观测 |
| `0x454f30` | `0x160` | 33 | **初始化/订阅**：向事件处理器订阅 `new_connection_event`(20 B) 与 `entity_removed_event`(20 B) | 逐位 |
| `0x455090` | `0xd0` | 61 | **设/清**：`edx==1` → 写 `self+0x18` 的 map，否则删 `self+0x38` 的 map，然后发 (67) | 逐位 |

#### 15.2.4 `graphwar_local_player_manager.rs`（3）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x519f40` | `0x630` | 107–128 | 处理 `player_info_event`(13)：按连接 id 过滤，push 进 `Vec<u32>`，首次入列时发 5 个"请求"事件 | 观测（tag 值逐位） |
| `0x51a570` | `0xa0` | 76/81 | 处理 tag **7**：`self+0x18:=1`、`self+0x1c:=payload.u32`、`self+0x10:=0`，并发 tag **8** | 逐位 |
| `0x51a610` | `0xe0` | 146 | 处理 `entity_removed_event`(14)；函数体只有一条 `panic_fmt` 出口 | 观测 |

#### 15.2.5 `graphwar_event_handler.rs`（2）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x3d38f0` | `0x16b0` | 396/407/454 | **事件名 → 判别值**分派（~50 个名字，逐字节精确比较），每个名字把 `ebx` 置成判别值 | 逐位（比较常量可复算） |
| `0x3d3630` | `0x2c0` | 467 | 用**常量** key `0x800000000000004a` 去查一条 `HashMap<u64, 订阅者集合>`（entry 0x38 B） | 观测 |

#### 15.2.6 `graphwar_logger.rs`（2）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x3b6c70` | `0xd0` | 12 | 线程局部（`thread_local!`，Location 落在 `std/thread/local.rs:428`）槽的**换值**：拿旧句柄、`dec` 强引用、`dec==0` 就 drop、再存新句柄 | 观测 |
| `0x44dd60` | `0x630` | 44 | 真正写日志的一条（大量 `0x528070` = dealloc 与 `0x45f160`/`0x461100` 格式化/落盘） | 推断 |

#### 15.2.7 `graphwar_campaign_control.rs`（1）/ `graphwar_lobby_control.rs`（1）

| vaddr | size | line | 一句话 | 证据 |
|---|---|---|---|---|
| `0x418820` | `0x2e0` | 94 | Godot 节点取值：`clone` / `init_ref()` / `upcast_ref`，随即调用 `graphwar_nodes.rs` 的节点访问器 | 观测 |
| `0x3ef990` | `0x6e0` | 28/29/30/42/47/52 | UI 装配：取 `ShowLocalButton` / `ShowGlobalButton` / `GraphwarLanVBox` 等节点并设文本/图标 | 观测 |

#### 15.2.8 三个"影子"函数（不在 8 个模块里，但离开它们读不懂本批代码）

| vaddr | 名字（本文暂定） | 为什么必须知道它 | 证据 |
|---|---|---|---|
| `0x3d3530` | **事件入队 sink** | 本批 8 个模块共 12 处 `call [0x9bac80]` 全部指向它。`tools/ptr_slots.py` 实测 **113 个使用者**，覆盖 team/life/obstacle/turn/color/mode/… 几乎所有 manager ⇒ 它是全局唯一的"发事件"出口 | 逐位（无条件拷贝 0x48 字节） |
| `0x3e6830` | 事件骨架加工（`0x140` B） | `0x3d38f0` 的每个分支在置 `ebx` 前都 `call 0x3e6830(rdi=rsp)` | 未知 |
| `0x418170` / `0x4183b0` | 存档读写 | `user://campaign.save` 的**真正**使用者 | 观测 |

---


### 15.3 顺带拿到的完整事件名清单


`.rodata` `0x1c0d84`–`0x1c14fc` 是一段**连续的** `*_info_event` 名字数组，
被 `0x4500c0`（`graphwar_server_connection_handler.rs`）在栈上逐项建出来，
再对每一项用 `cmp r15, 0x26` 循环 **38** 次。数组按**判别值递增**排列（下面 §5.4 的交叉验证）：

```
player_info_event          entity_removed_event      soldier_info_event
target_info_event          team_info_event           remove_team_event
pos_info_event             life_info_event           life_queued_death_event
game_state_info_event      function_fire_event       function_info_event
function_active_event      function_points_event     clear_obstacles_event
add_obstacle_event         destroy_obstacle_event    queue_destroy_obstacle_event
turn_info_event            effect_info_event         axis_mode_info_event
function_mode_info_event   turn_mode_info_event      time_mode_info_event
end_of_turn_info_event     game_name_info_event      skin_graphics_info_event
face_graphics_info_event   hat_graphics_info_event   entity_effect_info_event
turn_limit_info_event      turn_count_info_event     name_info_event
color_info_event           rank_info_event           lock_info_event
chat_message_event
```

每一项的 `(ptr,len)` 都是**逐条从代码里抄的**（不是从 blob 切出来的），
所以 `name_info_event` = 15 B @`0x1c14ce`、`rank_info_event` = 15 B @`0x1c14dd`、
`lock_info_event` = 15 B @`0x1c0e26` 这些长度都是确定的。

> `07-protocol.md` §3 把这一族记成 `NameInfo` / `RankInfo`（CamelCase）。
> **线上/spine 里的键是 snake_case 加 `_event` 后缀**（`name_info_event`），
> CamelCase 只是 Rust 变体名。补协议时别写错。

---


---

## 16. 12 个 Godot 扩展类清单 + 15 个「完全未知」模块的定性（2026-11）

> 本节回答两个以前只能靠猜的问题：**哪些模块真的是 Godot 类**、**那 15 个零锚点模块各干什么**。
> **地址口径**：全部是 **ELF 虚地址**；Ghidra/REA 地址 = 虚地址 **+ 0x100000**（只加一次）。
> 所有 `.rodata` 地址都可直接与 `strings -a -t x libgraphwarrust.so` 的输出对齐
> （该文件 `.rodata` 的 `Address == Offset`，这是本节能一句句核对的原因）。

### 16.1 先纠一个地基问题：`strings` 会把相邻字面量粘成一条

Rust 的字面量**不以 NUL 结尾**，`.rodata` 里首尾相接。所以
`strings` 输出 `GraphwarAdSwitchexit_player_setttarget_info_evenfunction_active_chat_message_eve`
这种"单词"其实是 **6 个以上的字面量**。

⇒ 本节引用的 `0x1cXXXX` 一律是**一条连续可打印游程的起点**，不是每个字面量的精确起点。
要精确边界只能用 `(ptr,len)`：`.data.rel.ro` 里 `R_X86_64_RELATIVE` 的 addend 是 ptr，
**槽位 +8 就是 len**（`docs/149` §1.1 用的同一套自校验）。
**精确边界本轮只对 12 个类名做了**（见 16.2），其余都是游程级证据 —— 这就是下面很多行只能标「推断」的原因。

### 16.2 结论一：全二进制只有 **12 个** GDExtension 类

`graphwarrust::<module>::<Type>` 形态的字面量（`core::any::type_name` 的输出）只出现在
**三份 12 元清单**里，内容完全一致：`0x1bfe9c`、`0x1c1854`、`0x1c1ef8`。
（三份是因为三个不同的调用点各带了一份；`strings` 里能看到的
`graphwarrust::player_showcase::PlayerShowcasegraphwarrust::entity_drawer::EntityDrawer…` 就是其中一份。）

| # | 类型 | 模块 |
|---|---|---|
| 1 | `PlayerShowcase` | `player_showcase.rs` |
| 2 | `EntityDrawer` | `entity_drawer.rs` |
| 3 | `GraphwarRootNode` | `graphwar_root_node.rs` |
| 4 | `GraphwarPlane` | `graphwar_plane.rs` |
| 5 | `AxisDrawer` | `axis_drawer.rs` |
| 6 | `GraphwarSetupControl` | `graphwar_setup_control.rs` |
| 7 | `GraphwarLongPressAnimation` | `graphwar_long_press_animation.rs` |
| 8 | `GraphwarButton` | `graphwar_button.rs` |
| 9 | `GraphwarFunctionInput` | `graphwar_function_input.rs` |
| 10 | `GraphwarAdSwitch` | `graphwar_ad_switch.rs` |
| 11 | `EffectDrawer` | `effect_drawer.rs` |
| 12 | `GraphwarLanVBox` | `graphwar_lan_vbox.rs` |

**这条结论的用处（也是反面证据的上限）**：15 个未知模块里**只有 3 个是 Godot 类**
（`button` / `lan_vbox` / `long_press_animation`），其余 12 个**不是类**，
它们是被这 12 个节点调用的**普通 Rust 结构体/模块**。
⇒ 对手写场景的还原者：不要再给 `player_manager` / `team_manager` 去"造一个节点"，
它们没有 `class_name`、没有方法表，只以函数形式存在。
**负证据的强度**：`graphwar_graphics_manager` / `graphwar_crash_reporter` / … 这些名字
在 `.rodata` 里**没有** `graphwarrust::…` 形态的字符串（全长搜索，0 命中），
但"没有类型名"**只**说明"不是以该模块名注册的类"，**不排除**它注册时用了别的类名。

### 16.3 结论二：15 个「完全未知」模块的定性表

**档位定义**（同 `AGENTS.md` §0.3）：`观测` = 能指到反汇编地址/字符串地址；
`推断` = 有依据但**未证**（同一行必须写清缺什么）；`未知` = 除模块名外没有可用证据。

| module | 职责（一句话） | 证据 | 档位 | 还缺什么 |
|---|---|---|---|---|
| `graphwar_button.rs` | 可换皮肤图标的**按钮控件**（`GraphwarButton`），商店/对战界面共用 | 类名在两份清单里（`0x1c1ef8`/`0x1bfe9c`）；短类名 `GraphwarButton` @`0x1c40c8` 游程；**4 个 panic 锚**：`:14:10`→`0x4e8f80`、`:83:1`→`0x4e1520`/`0x4e91d0`/`0x4e9380`、`:111:1`→`0x4e9730`（ELF 虚地址）；同池有 `set_button_icon`、`res://shop_button_entry.tscn`、`ButtonSkinsVbox`、`SkinBackgroundButton` | 观测 | 方法表与属性表（`classdb_register_extension_class_method/property` 的名字序列）；`:14`/`:83`/`:111` 三处的语义 |
| `graphwar_lan_vbox.rs` | **局域网房间列表** `VBoxContainer`（`GraphwarLanVBox`），带"显示本机 IP"按钮 | 类名清单同上；短类名 @`0x1c40fd`、`0x1bfcb0`；方法名 `ShowLocalButton` @`0x1bfbac` 游程；同池 `JoinButton`/`PlayerCount`/`JoinIp`/`IpLineEdit`/`ShowLocal`；`docs/149` 记 11 个 panic 锚 | 观测 | 11 个锚函数各自语义；房间条目什么时候刷新（订阅了 `function_info_event`/`tick_event` 仍需确认调用点） |
| `graphwar_long_press_animation.rs` | **长按动画控件**（`GraphwarLongPressAnimation`）：按下/抬起喂动画矩形，并代理一个 `LineEdit` 的 `input_text` / `long_press_input_text` | 类名清单同上；短类名 @`0x1c4319`、`0x1c2b6f`；**5 个 panic 锚**：`:10:10`→`0x4be530`、`:17:1`→`0x4be6e0`/`0x4d8b00`/`0x4d8e30`、`:31:1`→`0x4be6e0`、`:52:56`/`:52:86`/`:53:56`/`:53:87`→`0x4d9500`；同池 `AnimationRect`/`button_up`/`button_down`/`pressed`/`get_input_text`/`set_long_press_input_text`/`get_long_press_input_text`/`PlayerNameLineEdit`/`text_changed` | 观测 | 动画时长/曲线的常量；`:52`/`:53` 那两对 (col 56/87) 说明同一条语句里有两个 `.expect()` |
| `graphwar_computer_player_manager.rs` | **电脑玩家**：每个 bot 一个线程，名字从固定表里取 | 字符串池 @`0x1c1854` 内有 `Failed to join computer player thread.`、`failed to spawn thread`、bot 名 `Deep Thought`/`HAL`/`Skynet`/`Agent Smith`/`Deep Blue`/`Cleverbot`/`Alpha Zordon`/`Wolfram`/`ChatGPT`、以及 `bot_0`…`bot_5` | 推断 | **缺关键一步**：这些串还没有被绑到具体函数（本轮只看清了字符串池，没做 `lea`+`mov len` 的逐条配对）；线程模型（一人一线程 vs 一池）未证 |
| `graphwar_crash_reporter.rs` | 崩溃日志落盘 `user://crash_log.txt` + 上报到 collector（地址来自 CLI/全局服务器） | 字符串 `user://crash_log.txt` @`0x1c29a9` 游程；`Failed to get global server address for crash reporting.` @`0x1c2dce` 游程；CLI 选项 `CRASH_REPORT_ADDRESS`/`crash_report_address` @`0x1bdaaf` 游程（与 `BROKER_ADDRESS`/`SERVER_PORT` 同池，`docs/07` 的启动参数块）；时间格式 `%Y-%m-%d %H:%M:%S` | 推断 | **该模块 0 个 panic 锚**，全二进制却有 **8 份** `src/graphwar_crash_reporter.rs` 字面量（其它模块各 1 份，这个 8 份是未解释的异常）；8 份都没配上 `Location` 结构 ⇒ 它们是 inline `&str`，得逐条解 `lea`+`mov len` 才能钉住函数 |
| `graphwar_texture_loader.rs` | **资源路径 → 贴图**的查表/加载（皮肤/帽子/脸/段位/图标整套 `res://…svg`） | `src/graphwar_texture_loader.rs` @`0x1b29f6`，被函数 `0x4d3180`（0x3e40 B）与 `0x4c2b90` 引用；`0x4d3180` 同体还引用 `Team1Container`/`Team2Container`/`PlayerListContainer`/`GameIpLabel`/`user://local_ip.config`，并带 `src/graphwar_setup_control.rs`、`src/graphwar_game_client.rs` 的 Location ⇒ 它是**被 setup/lan 界面复用的泛型/内联体**；panic 锚 `:145:47` | 推断 | 一个函数被两个模块的内联体共用 ⇒ 归属本身有歧义；`res://hats|faces|ranks|skins/*.svg` 全表 @`0x1c2dce` 游程只能证明"在这片代码里被引用"，**没有**证明 `(名→路径)` 的映射顺序 |
| `graphwar_team_manager.rs` | **队伍**：两队各一个容器 + 队伍条目场景 | `Team1Container`/`Team2Container` @`0x1c2d0d` 游程；同池 `res://team1_entry.tscn`、`res://team2_entry.tscn`；`docs/149` 记 7 个锚 | 推断 | 7 个锚函数语义未解；两队容器是 setup 界面还是 game 界面尚未分辨（本轮它们与 `texture_loader` 出现在同一函数体里） |
| `graphwar_player_manager.rs` | **玩家条目**的增删（玩家列表里移除玩家） | `RemovePlayerButton` @`0x1c2dce` 游程；同池 `PlayerListContainer`、`PlayerNameLabel`、`force_title_update`、`on_visibility_change` | 推断 | 3 个锚函数语义；"移除玩家"到底是 UI 还是管理器自身行为未证 |
| `graphwar_soldier_manager.rs` | **士兵条目/移除**（每名士兵一个图标格） | `RemoveSoldierButton` @`0x1c2dce` 游程；同池 `SoldierIconContainer`、`HatIcon`、`EmptyIcon` | 推断 | 4 个锚函数语义；士兵图标格是否由 `player_showcase` 而非本模块绘制，未证 |
| `graphwar_player_screen_control.rs` | **玩家界面控制器**：等级标题、改名、皮肤/帽子/脸的选择 | `PlayerScreenLevelTitleLabel` @`0x1bdaaf` 游程，同池 `Replay`/`Lobby`/`JoinIpPopUp`/`AddBotPopUp`/`ConfirmPlay`/`ExitPopUp`/`JoinLevelPopUp`（= 该屏弹出的全部弹窗名）；`unlock_skins`/`purchase_graphwar_support` 同池 | 推断 | 8 个锚函数语义；`client/` 里同名 GDScript 是**仿制品**，不能当证据（`AGENTS.md` §0.4） |
| `graphwar_effect_manager.rs` | — | 仅 `docs/149` 记 1 个 panic 锚；全仓库无字符串线索 | 未知 | 缺最小证据：那 1 个锚函数的地址 + 它引用的字符串。**这一条本轮没跑出来**（工具在工作目录被清空后只来得及重建字符串面） |
| `graphwar_entity_effect_manager.rs` | — | 仅 3 个 panic 锚（`docs/149`） | 未知 | 同上：3 个锚函数地址/字符串 |
| `graphwar_game_recorder.rs` | — | 仅 2 个 panic 锚（`docs/149`）；`docs/02` 旧行写"对局录制"是**按名字猜的**，没有证据 | 未知 | 同上；另需确认它是否与 `graphwar_replay_player.rs` 共用录像格式 |
| `graphwar_graphics_manager.rs` | — | 仅 6 个 panic 锚（`docs/149`）；旧行"图形资源管理"是**按名字猜的** | 未知 | 同上；并需分辨它与 `texture_loader` 的分工（谁持有资源表、谁只做加载） |
| `graphwar_video_writer.rs` | — | 仅 3 个 panic 锚（`docs/149`）；旧行"录像导出"是**按名字猜的** | 未知 | 同上；另需确认它是写 mp4/png 序列还是仅转发 Godot 的 `MovieWriter` |

**统计**：15 个里 **3 个 `观测`**（button / lan_vbox / long_press_animation）、
**7 个 `推断`**（computer_player_manager / crash_reporter / texture_loader / team_manager /
player_manager / soldier_manager / player_screen_control）、
**5 个 `未知`**（effect_manager / entity_effect_manager / game_recorder / graphics_manager / video_writer）。

### 16.4 这批结论怎么复核（不依赖本节，任何人可重跑）

```bash
# 1) 类清单（12 个）：三份 12 元清单都在 .rodata，rodata 的 Address == Offset
strings -a -t x libgraphwarrust.so | grep -o 'graphwarrust::[a-z_]*::[A-Za-z]*' | sort -u

# 2) 某个串是否真的属于某模块的代码：先取串地址，再找引用它的函数
#    （lea rXX,[rip±disp] 的 disp 是「到指令末尾」的偏移；写成 +disp 会算错）
#    本轮用的判据：panic::Location 的 (file.ptr,file.len,line,col) 四元组，
#    以及「同一函数体里还 lea 了哪些 .rodata 串」

# 3) 别用 fixtures/module_funcs.tsv 判函数身份：那是 panic 串的启发式，不是归属
```

**本节的可复跑脚本**：`tools/class_probe.py`
（`classes` / `module <mod>` / `func <ELF 虚地址>` / `literals <正则>`）。
它把 §16.4 的三步做成自动的：`.rela.dyn` 解 `(ptr,len)` + capstone 解 `lea`+`mov len`，
不依赖 Ghidra，秒级。`module graphwar_texture_loader` 能直接复现 16.3 里那一行的全部证据。

**本节的已知偏差（写结论时必须带上）**：
① 除 12 个类名外，字符串边界的精确长度没解，所以"同池"≠"同一模块"；
② `call rel32` 看不到间接调用，凡写"没有调用者"的地方都要配 `tools/ptr_slots.py`；
③ 一个 `Location` 结构体被多个函数引用时（`texture_loader` 就是），归属**天然有歧义**，
本节对此只写"复用"，不写"属于"。
## 17. 我们的 `src/` ↔ 原版模块（文件级对照）

> 为什么要有这一节：`src/` 原来用的是**我们自己的架构名**（`game.rs` / `api.rs` / `shot.rs`…），
> 而原版是 **58 个 `graphwar_*.rs` + 12 个非 `graphwar_` 模块**。文件名言不达意，读者就得自己猜
> 对应关系。本节把对应关系写成表，并把能定为 1:1 的**用原版名重命名**（`git mv`，历史可 `--follow`）。

**这张表只回答"哪个文件对哪个模块"，不回答"还原到什么程度"**——证据档位（逐位/观测/推断/未知）
仍以 [`149-coverage-matrix.md`](149-coverage-matrix.md) 与各专题文档为准。

数据来源（只许用这三个）：

1. **模块清单**：`docs/coverage-matrix.tsv`（`tools/coverage_matrix.py` 生成）。
   下面第一列括号里的数字是 `原版可归属函数数 / panic 锚点数`（即该表的 `orig_funcs` / `anchors`）。
2. **模块职责**：本文 §1–§16（尤其 §16.2 的 12 个 GDExtension 类）。
3. **函数 → 源文件的行号锚**：`tools/class_probe.py module <名字>`（`.rela.dyn` 解 `(ptr,len)` + capstone 解 `lea`），
   以及 `.so` 里 `src/<file>.rs:LINE` 字面量。凡本表写"锚点"，都是这个工具的**观测**输出。

**状态词表**（结构状态，不是证据档位）：

| 状态 | 含义 |
|---|---|
| `一一对应` | 该原版模块的内容只落在一个（或一组同名目录下的）我方文件里，且**没有**别的东西混进来 |
| `一一对应（部分）` | 结构上 1:1，但我方只覆盖了该模块的**一部分**函数（覆盖面写在「依据」列） |
| `一一对应（⚠待核实）` | 同上，但"这个文件就是这个模块"目前只有模块名/旧文档支持，锚点**不支持** |
| `混装（待拆）` | 我方一个文件里混了**多个**原版模块的内容，边界尚无证据 ⇒ 按 `AGENTS.md` 铁律 2 保持原样并登记（§17.4） |
| `未实现` | 该原版模块在我方 `src/` 里没有实现 |
| `我方自造（无原版对应）` | 我们自己的桥接/工具，**不许**挂原版名 ⇒ 已收进 `src/_scaffold/`（§17.3） |

**地址口径**：本表所有 `0x……` 都是 **ELF 虚地址**；Ghidra/REA 地址 = 虚地址 + `0x100000`（只加一次）。

每个表里的行**按状态分组**（`一一对应` → `混装（待拆）` → `未实现`），组内按模块名排序。

### 17.1 58 个 `graphwar_*.rs`

| 原版模块（函数/锚点） | 我们的文件 | 依据（锚点 / 函数名 / 文档） | 状态 |
|---|---|---|---|
| `graphwar_turn_manager.rs`（14/45） | `src/graphwar_turn_manager.rs` | 锚点 14 个（0x463790 订阅 new/entity_removed、0x4638f0、0x463da0 轮转体、0x465620、0x465780、0x470eb0…）；`docs/98`（Vec<soldier_id>+游标）、`docs/117`（真实 TCP 上的射手轮转断言）、`docs/14-turn.md` §9。原文件 `turn.rs` | `一一对应` |
| `graphwar_function_evolutionator.rs`（16/25） | `src/graphwar_function_evolutionator.rs` | 锚点 16 个（0x41b880/0x41c360/0x41c720 抽样与装配、0x4825b0/0x482c20/0x4839e0 带 `mid > len`/`VAR_XVAR_Y` 的 token 操作）；`docs/71`（权重表 `random % 85`、常数 `N(0,1)×10.2`）、`docs/74`（适应度）、`docs/147`（两阶段评分与预算）、40 万次运行时抽样。原文件 `aigen.rs`。⚠ 含我方接线层 `SoldierEvolver`（见 §17.4） | `一一对应` |
| `graphwar_play_screen_control.rs`（10/64） | `src/graphwar_play_screen_control.rs` | 锚点 10 个（0x3da130、0x3dee00…，`docs/02 §11` 同款口径）；`docs/14-turn.md` §9 逐函数规格；`docs/135`（开火特效）。原文件 `play_control.rs`。⚠ 旧矩阵（`docs/149`）把它记成 `未做`，与本文件头和 `src/lib.rs` 冲突 —— 本轮按代码更正 | `一一对应` |
| `graphwar_life_manager.rs`（9/12） | `src/graphwar_life_manager.rs` | 锚点 9 个（0x454b40…0x455b60，`docs/02 §13.1` 那张独立表逐个相同；0x454950 是 `&1` 存活判定）；`docs/65`（穿过即死/友军致死，真实对局与回放逐个相同）。原文件 `life.rs` | `一一对应` |
| `graphwar_path_calculator.rs`（1/9） | `src/graphwar_path_calculator.rs` | 采样器 `0x3c2530`（`docs/04`）+ `docs/91`（补 `hi = mid2` 后 24/24 用例整条点序列逐位相等）。⚠ 采样器本体**没有** panic 锚，本模块只归属到 1 个辅助函数（0x469630/0x4696d0）⇒ "采样器属于本模块"是 `docs/02 §1` 的文档级归属，不是锚点归属。原文件 `path.rs` | `一一对应` |
| `graphwar_replay_player.rs`（5/18） | `src/graphwar_replay_player.rs` | 锚点 5 个（0x3d6d70、0x3d7380、0x3d76f0 写 `user://replays/`）；`docs/133` §2（gzip+JSONL 格式）、`docs/136/142`（400 份回放消息级对拍，71 份完全一致）。原文件 `replay.rs`。⚠ 文件内 `ReplayPlayer`（播放时钟）是我方自定模型，见 §17.4 | `一一对应` |
| `graphwar_tick_handler.rs`（2/3） | `src/graphwar_tick_handler.rs` | `docs/35`（113 个 `TickReply` 样本逐字节）+ `docs/42`（TimeMode → 回合时长）。原文件 `tick.rs`。⚠ **待核实**：本模块的锚函数是 0x51a610 / 0x51a750 / 0x3d76f0（后两个与回放写盘同址、`contested` 与 replay_player 共有），与本文件实现的"60Hz tick 时钟 + 时长表"**没有函数级对应**；现在的对应来自矩阵旧行与模块名，属**推断** | `一一对应（⚠待核实）` |

| `src/graphwar_level_manager.rs` | 原版 `graphwar_level_manager.rs` + **可能属于 `graphwar_campaign_control.rs`** 的 `level_info`(`0x4ad380`) 与 `campaign.save` 簇(`0x417ef0`/`0x418170`/`0x4183b0`) | 这三处**都没有 Location 锚**；`docs/10` §12.6 | 无锚 ⇒ 归属只能靠调用形态，证据不足。暂时承载并登记，等 `graphwar_campaign_control.rs` 有锚点再决定搬不搬 |
| `graphwar_level_generator.rs`（4/26） | `src/graphwar_level_generator.rs` + `src/graphwar_level_generator/tables.rs` | 锚点 26 个（0x466d20/0x467530 等）；`docs/55/56/61`（障碍 x/y/r 的 `to_bits()` 13/13 逐位）、`docs/103/126`（种子语义）。原文件 `levelgen.rs` + `levelgen_tables.rs`（ziggurat 表）——**同一个原版文件，我方按"逻辑 / 常量表"拆成两份**，主文件用原版名，表放同名目录 | `一一对应` |
| `graphwar_shot_calculator.rs`（1/12） | `src/graphwar_shot_calculator.rs` + `src/graphwar_shot_calculator/settle.rs` | 锚点 12 个（0x3c5f70 修正常数、0x3c62b0 等）；`docs/05`（修正 `c = y0 − f(±x0)`）、`docs/86 §14`（82 发并集 152/152）、`docs/122`（按 id 排除射手）。原文件 `shot.rs` + `settle.rs`——**同一个原版文件**（0x3c2bc0 主函数 + 0x3c5f70 修正），我方按主题拆两份。⚠ 0x3c2bc0 本体**没有** panic 锚，"settle 属于本模块"靠 `docs/86` 的行为证据 | `一一对应` |
| `graphwar_event.rs`（0/0） | `src/events.rs` | `enum Event` 72 个具名变体 + `GameInfo`（`docs/114`：判别值逐条解出、两道自校验；`docs/07 §3.1` 9 个字段）。⚠ 本模块**零锚点**（纯数据），对应关系靠职责与 `docs/114`。同文件还收了 event_handler 的名字表 → 见 §17.4 | `混装（待拆）` |
| `graphwar_event_handler.rs`（2/4） | `src/events.rs` | 只落了一张表：`DISPATCH_NAME_TAGS` + `from_dispatch_name`（从 `0x3d38f0` 逐字节解出的"事件名 → 判别值"，`docs/114` §3）。⚠ `0x3d3630`（用常量 key 订阅 `HashMap<u64,_>`）**未实现**，所以本模块**没有** handler 行为，只有数据 | `混装（待拆）` |
| `graphwar_remote_event_handler.rs`（3/171） | `src/events.rs` | 只落了枚举与名字表（同上）。本模块 171 个锚点是全二进制第三密，我方**未实现**其分发/订阅行为；旧矩阵把 `events.rs` 记成它的实现，口径偏宽 —— 本轮按"没有 handler 代码"重记 | `混装（待拆）` |
| `graphwar_game_state_manager.rs`（33/207） | `src/game.rs` + `src/turnstate.rs` | `docs/41`（6 状态 7 事件、13 次迁移在真实对局里逐个相同）、`docs/150`（胜负判定）。两个我方文件都属本模块，但 `game.rs` 还含 function_manager 的内容 → 见 §17.4 | `混装（待拆）` |
| `graphwar_function_manager.rs`（7/12） | `src/game.rs` | `SoldierFunctions`（`HashMap<u32, 112 字节>`、entry 120、value 开头 `Vec<Token>`）逐条读自 `docs/112` §0/§2.1；7 个锚函数在 0x455b60…0x456f80。⚠ 与 game_state_manager 混在同一文件 | `混装（待拆）` |
| `graphwar_game_server.rs`（3/86） | `src/graphwar_game_server.rs` + `src/server.rs` | `docs/43/46`（一回合 12 条报文与抓包逐字节）、`docs/62`（真实 TCP 跑到 EndingGame）、`docs/117`（回合推进接线）。原文件 `serverloop.rs`（主循环）+ `server.rs`（首包同步，与 server_connection_handler 共有） | `混装（待拆）` |
| `graphwar_server_connection_handler.rs`（6/24） | `src/server.rs` | `docs/33`（新连接 7 条状态同步逐字节一致）；`docs/112` §15.3 指出建事件名数组的 `0x4500c0` 就在本模块。⚠ `server.rs` 里还有 `on_tick_request`（tick 应答）→ 见 §17.4 | `混装（待拆）` |
| `graphwar_plane.rs`（10/19） | `src/plane.rs` | `docs/08/39`（掩膜光栅化与 `is_occupied` 逐字节 / 300 随机点一致）；`px = x*1000/50 + 500` 来自 `0x4be3e0`。⚠ 同文件还含 plane_pos_manager 与（疑似的）obstacle_manager 内容 → §17.4 | `混装（待拆）` |
| `graphwar_plane_pos_manager.rs`（6/9） | `src/plane.rs` | `docs/140`：`px = 20x + 500` 由常量反查得出。⚠ 本模块的锚函数在 `0x3c7330…0x3c7cb0`（与 path_calculator / shot_calculator 同区段），与 `plane.rs` 里那条换算**没有**做过函数级对齐 ⇒ "plane.rs 属于本模块"是**推断** | `混装（待拆）` |
| `graphwar_obstacle_manager.rs`（8/14） | `src/plane.rs`（只有掩膜部分） | `docs/08/39` 只覆盖 `0x4f15b0` 的光栅化 / `is_occupied`（摧毁 = 挖 r=0.5 洞）。⚠ 两处证据冲突，按原样记录：`docs/02 §8/§14` 说 `0x4f15b0` 属本模块（依据是**已作废**的 `.eh_frame` 函数边界），而 `class_probe` 给出的本模块锚函数是 `0x3c64c0/0x3f00b0/0x3f2480/0x3f27d0` —— `0x4f15b0` **不在**其中。⇒ 文件级对应**未定**；障碍的生成与生命周期也仍未落码 | `混装（待拆）` |
| `graphwar_ad_switch.rs`（6/3） | — | `docs/19`/`docs/132`：判据只有 `are_ads_enabled() = !owned`；AdMob 在 `.gdc` 插件侧 | `未实现` |
| `graphwar_audio_control.rs`（7/16） | — | `docs/111`：SoundHandler / `-15f` / `-5f` / 32767 归一化已还原，但没接进 `src/`（`client/` 是仿制） | `未实现` |
| `graphwar_backend.rs`（6/12） | — | `docs/19` §3：联机层**不是**广告；读出 15 个方法名（`start_server` / `lan_broadcaster` / `broker`…），不是 Godot 类 | `未实现` |
| `graphwar_button.rs`（6/3） | — | `docs/02 §16.3`：类名 `GraphwarButton` + 4 个锚函数，方法表未读 | `未实现` |
| `graphwar_campaign_control.rs`（1/1） | — | `docs/10` §7：21 关数据已从 `0x4ad380` 解出、`campaign.save` 格式已解；未落码 | `未实现` |
| `graphwar_client_connection_handler.rs`（8/16） | — | `docs/134` 只覆盖 `client/` 的 GDScript 联机；原版 Rust handler 未复原 | `未实现` |
| `graphwar_color_manager.rs`（4/6） | — | `docs/102`：两张 6 色队色表 + 随机起点线性探测已解出；无实现 | `未实现` |
| `graphwar_computer_player_manager.rs`（5/27） | — | `docs/02 §16.3`：bot 名表 / 线程串已定位（推断档）；AI 的算法在 function_evolutionator 里，但**调度**（一人一线程？）未证 | `未实现` |
| `graphwar_crash_reporter.rs`（0/0） | — | `docs/02 §16.3`：`user://crash_log.txt` 等串已定位；**0 个 panic 锚** | `未实现` |
| `graphwar_draw_util.rs`（1/4） | — | `docs/140`：准备圈/弹道画法已从机器码扒出；绘制落 `client/` GDScript | `未实现` |
| `graphwar_effect_manager.rs`（1/3） | — | `docs/02 §16.3`：全仓库零线索（1 个锚） | `未实现` |
| `graphwar_entity_effect_manager.rs`（3/4） | — | `docs/02 §16.3`：3 个锚，无字符串线索 | `未实现` |
| `graphwar_function_input.rs`（13/33） | — | `docs/02 §16.3`/`docs/149`：解析器 `0x4ed670` 体内唯一的 Location 锚是 `math_function.rs:75/285` ⇒ 解析器**不是**本模块的代码；本模块本体（13 个归属函数）未复原 | `未实现` |
| `graphwar_game_client.rs`（36/59） | — | 归属函数最多的模块（36 个）；只有 `docs/archive` 的调用形态观测 | `未实现` |
| `graphwar_game_recorder.rs`（2/4） | — | `docs/02 §16.3`：2 个锚；"对局录制"只是按名字猜的 | `未实现` |
| `graphwar_graphics_manager.rs`（6/8） | — | `docs/02 §16.3`：6 个锚，无字符串线索；与 texture_loader 的分工未分辨 | `未实现` |
| `graphwar_lan_broadcaster_v6.rs`（4/4） | — | `docs/07/24`：局域网广播报文已抓包；`src/` 无实现 | `未实现` |
| `graphwar_lan_vbox.rs`（11/19） | — | `docs/02 §16.3`：类名 `GraphwarLanVBox` + 11 个锚，语义未解 | `未实现` |
| `graphwar_level_manager.rs`（40/590） | `src/graphwar_level_manager.rs` | 锚点最密（590 个 Location / 40 个函数）；只有 `docs/118` §2 的入口观测 | `观测` |
| `graphwar_lobby_control.rs`（1/6） | — | `docs/112` 只有调用关系；界面在 `client/` GDScript | `未实现` |
| `graphwar_local_player_manager.rs`（3/15） | — | `docs/112` §13 记录结构观测（0x519f40/0x51a570/0x51a610）；无实现 | `未实现` |
| `graphwar_logger.rs`（2/2） | — | `docs/112` §15.2.6：log 入口结构观测（0x3b6c70/0x44dd60）；无实现 | `未实现` |
| `graphwar_long_press_animation.rs`（7/7） | — | `docs/02 §16.3`：类名 + 5 个锚；动画常量未解 | `未实现` |
| `graphwar_mode_manager.rs`（2/6） | — | `docs/88` §7：TurnMode/FunctionMode/AxisMode/TimeMode 的取值已解；模块本体未复原 | `未实现` |
| `graphwar_module_container.rs`（20/1） | — | `docs/110/112` 有调用观测；无实现 | `未实现` |
| `graphwar_name_manager.rs`（3/3） | — | `docs/112` §0/§15.2.2：`HashMap<u32,String>`、`name_info_event`=60 逐位解出；无实现 | `未实现` |
| `graphwar_nodes.rs`（11/1） | — | `docs/112` 有结构观测；无实现 | `未实现` |
| `graphwar_player_manager.rs`（3/6） | — | `docs/02 §16.3`：`RemovePlayerButton` 串池（推断）；3 个锚未解 | `未实现` |
| `graphwar_player_screen_control.rs`（8/22） | — | `docs/02 §16.3`：弹窗名串池（推断）；界面在 `client/` GDScript | `未实现` |
| `graphwar_rank_manager.rs`（4/4） | — | `docs/112` §5：`RankInfo` 判别值 67、结构观测；段位算法在本模块内**算不出来**（负结果） | `未实现` |
| `graphwar_replay_control.rs`（2/9） | — | `docs/133` 的回放播放器在 `client/` GDScript。⚠ `graphwar_replay_player.rs` 里的 `ReplayPlayer` 是**我方自定**的播放模型（§17.4），不能算作本模块 | `未实现` |
| `graphwar_root_node.rs`（23/178） | `src/graphwar_root_node.rs` | 主场景节点；`docs/125` 只有调用形态观测 | `观测` |
| `graphwar_server_broker_handler.rs`（3/5） | — | 原模块是**客户端侧**的 broker 处理器；`src/_scaffold/broker.rs` 是我方自建的服务端，**不是**它的复原（`docs/115`） | `未实现` |
| `graphwar_setup_control.rs`（21/70） | `src/graphwar_setup_control.rs` | `docs/137` 有像素级观测；界面在 `client/` GDScript | `观测` |
| `graphwar_soldier_manager.rs`（4/5） | — | `docs/02 §16.3`：`RemoveSoldierButton` 串池（推断）；4 个锚未解 | `未实现` |
| `graphwar_target_manager.rs`（3/4） | — | `docs/123`：重置规则形状已钉住，判定点未知（397 份回放 0 次部分命中）；`src/` 只有 `Target` 数据结构 | `未实现` |
| `graphwar_team_manager.rs`（7/10） | — | `docs/02 §16.3`：`Team1Container`/`Team2Container` 串池（推断）；7 个锚未解 | `未实现` |
| `graphwar_texture_loader.rs`（1/1） | — | `docs/02 §16.3`：`0x4d3180` 一对多复用 ⇒ 归属天然有歧义；无实现 | `未实现` |
| `graphwar_video_writer.rs`（3/4） | — | `docs/02 §16.3`：3 个锚，无字符串线索 | `未实现` |

### 17.2 12 个非 `graphwar_` 模块（`other` 作用域）

这 12 个文件名不带 `graphwar_` 前缀，但**是游戏自己的模块**（§9 提醒过：`entity_drawer.rs` /
`effect_drawer.rs` 虽无前缀，确实是游戏代码）。

| 原版模块（函数/锚点） | 我们的文件 | 依据（锚点 / 函数名 / 文档） | 状态 |
|---|---|---|---|
| `math_function.rs`（7/15） | `src/math_function.rs` + `src/math_function/to_text.rs` | 解析器 `0x4ed670` 的 panic 锚落 `src/math_function.rs:285:28`（`docs/03`、`class_probe`）；求值器 `0x4eb9d0` 与它同层；token → 文本 `0x4ec160` 的 panic 分支同样引 `math_function.rs:285:28`（`docs/119` §4.6，指令级）。原目录 `function/`。**同一个原版文件**，我方按"解析/求值"与"反向打印"拆两份。另见 `polish_notation_converter.rs` 的正名（本轮更正） | `一一对应` |
| `polish_notation_converter.rs`（1/2） | — | **本轮更正**：本模块的锚函数是 `0x4f57d0`（`:36:52` / `:116:55`，`class_probe`），**不是** `0x4ec160`。旧矩阵把 `src/math_function/to_text.rs` 记在这里 —— 但 `0x4ec160` 的 panic 分支引的是 `math_function.rs:285:28`（`docs/119` §4.6），按锚点应归 `math_function.rs`。⇒ `to_text.rs` 移到 `src/math_function/`，本模块改记 `未实现` | `未实现` |
| `entity_drawer.rs`（14/36） | `src/entity_drawer.rs` | `EntityDrawer::draw` = vaddr `0x4b2890`（GOT 槽 + `.rela.dyn` 定位 `draw_arc`/`draw_polygon` 六个调用点）；每条常量都标了 `.rodata` 槽与指令地址（`docs/140`）。⚠ 只覆盖 `draw` 一个函数，本模块另有 13 个锚函数未做；且我方只还原到"图元清单"层（不复现 gdext 调用） | `一一对应（部分）` |
| `effect_drawer.rs`（11/17） | — | `docs/02 §2` 确认是游戏自己的特效绘制模块；无实现 | `未实现` |
| `axis_drawer.rs`（6/3） | — | `docs/88` §7 记录了 AxisMode 的 4 个取值；绘制本体未落码 | `未实现` |
| `obstacle_drawer.rs`（2/7） | — | 除模块名外没有一条能指到地址或报文的观测 | `未实现` |
| `function_drawer.rs`（1/1） | — | 除模块名外没有任何观测 | `未实现` |
| `button_manager.rs`（2/3） | — | 除模块名外没有任何观测 | `未实现` |
| `function_button_manager.rs`（2/3） | — | 除模块名外没有任何观测 | `未实现` |
| `function_history_recorder.rs`（1/3） | — | 除模块名外没有任何观测 | `未实现` |
| `player_showcase.rs`（10/18） | — | 除模块名外没有任何观测（`PlayerShowcase` 类名见 `docs/02 §16.2`） | `未实现` |
| `server_broker_request_connection.rs`（0/1） | — | `docs/115` 只覆盖 broker 线格式；这个连接对象本身没看过 | `未实现` |

> `weak_caller.rs`（`dep`，39 个函数）是第三方依赖库，不算游戏代码，本表不列。

### 17.3 我方自造（`src/_scaffold/`，无原版对应）

`AGENTS.md` 铁律 3：**我们自造的东西不许挂原版名**。下列文件全部移进 `src/_scaffold/`，
目录里有一份 [`README.md`](../src/_scaffold/README.md) 说明"这些不是原版的东西"。

| 我们的文件 | 做什么 | 依据 / 说明 | 状态 |
|---|---|---|---|
| `src/_scaffold/api.rs` | 内核 → GDScript / JSON 的对外 API（`GW2Rebuild.path_calculate`）；原版没有这一层（它本身就是 Godot 模块） | `docs/108` §2.3 / `docs/116` | `我方自造（无原版对应）` |
| `src/_scaffold/protocol.rs` | 报文编解码装配层（`Frame` / `parse_message` / `emit_*`）；原版没有同名模块，编解码分散在各 handler 里 | `docs/07`/`docs/33` 的抓包口径 | `我方自造（无原版对应）` |
| `src/_scaffold/broker.rs` | **我们自己的一台** broker 服务端；原版只有连 broker 的客户端，没有服务端 | `docs/115` | `我方自造（无原版对应）` |
| `src/_scaffold/capi.rs` | C ABI，纯为把实现暴露出去做对拍（ctypes）；原版无此物 | — | `我方自造（无原版对应）` |
| `src/_scaffold/gdext.rs` | GDExtension 入口脚手架（`gdext_rust_init`）；原版入口在 Godot 模块侧，不在 58+12 个文件里 | `docs/116` | `我方自造（无原版对应）` |
| `src/_scaffold/gdext_class.rs` | `classdb_register_extension_class4` 的封装（同上） | `docs/116` §3 | `我方自造（无原版对应）` |
| `src/_scaffold/rng.rs` | ChaCha12 的重写；原版是 `rand_chacha` crate，不是游戏模块 | `docs/53` | `我方自造（无原版对应）` |
| `src/_scaffold/replay_inflate.rs` | 手写 DEFLATE / gzip；原版用第三方 crate | `docs/133` §6 | `我方自造（无原版对应）` |
| `src/_scaffold/replaydiff.rs` | 回放**消息级**对拍器（工具） | `docs/136` | `我方自造（无原版对应）` |
| `src/_scaffold/diffconv.rs` | 上一条的收敛层（工具） | `docs/142` | `我方自造（无原版对应）` |
| `src/_scaffold/ai_explain.rs` | AI 决策链解释器（工具）：把 evolutionator 的零件按原版顺序跑一遍并留痕 | `docs/147` | `我方自造（无原版对应）` |

**为什么 `events.rs` 不在这里**：它是原版 `graphwar_event.rs` 的复原（72 个具名变体 + 判别值），
不是桥接层。`game.rs` / `plane.rs` / `server.rs` / `turnstate.rs` 同理。

### 17.4 混装与待拆（`AGENTS.md` 铁律 2 的登记表）

一个文件里混了原版**多个**模块、而"边界在哪"没有证据 ⇒ **不猜、不搬**，登记在这里。

| 我方文件 | 混了哪些原版内容 | 证据 | 为什么现在不拆 |
|---|---|---|---|
| `src/events.rs` | graphwar_event.rs（主体：`enum Event` 72 变体 + `GameInfo`）+ graphwar_event_handler.rs（`DISPATCH_NAME_TAGS` 名字→判别值表） | `docs/114` §3 | 表只有约 70 行数据；拆出去会让 `graphwar_event_handler.rs` 只剩一张表、没有 handler 行为，反而误导 |
| `src/game.rs` | graphwar_game_state_manager.rs（主体：一局的状态/开火/胜负）+ graphwar_function_manager.rs（`SoldierFunctions`、`FunctionSlot`） | `docs/41`/`docs/112` §2.1 | 两者共用同一个 `Match`，槽表是它的字段；按函数切会让两边互相引用 |
| `src/plane.rs` | graphwar_plane.rs（像素变换）+ graphwar_plane_pos_manager.rs（`px = 20x + 500`）+ graphwar_obstacle_manager.rs 的掩膜（⚠ 归属见 §17.1 该行的冲突） | `docs/08`/`docs/39`/`docs/140` | 三者的函数在二进制里就是分开的，但我方只实现了"点/掩膜"这一层，尚未逐函数对齐 |
| `src/server.rs` | graphwar_server_connection_handler.rs（主体：新连接 7 条同步，`0x4500c0`）+ graphwar_tick_handler.rs（`on_tick_request`） | `docs/33`/`docs/112` §15.3 | tick 应答只有 5 行；等 `graphwar_tick_handler.rs` 的锚点核实完再拆 |
| `src/turnstate.rs` | graphwar_game_state_manager.rs（与 `src/game.rs` 同模块：状态枚举 + 迁移表） | `docs/41` | 与 `game.rs` 同属一个原版模块，我方按"迁移表 / 运行实体"拆成两份；原版名已被 `game.rs` 占用（见 §17.4 说明） |
| `src/graphwar_function_evolutionator.rs` | 原版 graphwar_function_evolutionator.rs + **我方接线层** `SoldierEvolver` / `FunctionSlot` / `SoldierFunctions` 的用法 | `docs/98` §3/§7、`docs/117` | 接线层（把进化结果写进哪个槽、什么时候跑）在原版的 computer_player_manager 里，**未还原**；现在混在进化器文件尾部，待 `computer_player_manager` 落地后迁出 |
| `src/graphwar_replay_player.rs` | 原版 graphwar_replay_player.rs（文件读取/解析/推进）+ **我方自定**的 `ReplayPlayer` 播放时钟 | `docs/133` §4（自述"我们自定，原版未观测"） | 播放时钟是我们为了让客户端能播而设计的，原版没有对应函数；等 `graphwar_replay_control.rs` 落地后迁进 `_scaffold/` |
| `src/graphwar_shot_calculator/settle.rs` | 原版 graphwar_shot_calculator.rs 的 `0x3c2bc0`（开火结算主函数） | `docs/86`/`docs/122` | `0x3c2bc0` 没有 panic 锚，函数体也**没有**逐条还原（逐位的是行为）；边界只能按主题切 |
| `src/graphwar_level_generator/tables.rs` | 原版 graphwar_level_generator.rs 的 ziggurat 常量表 | `docs/53`/`docs/55` | 纯数据；拆出来只为让主文件可读，不需要边界判断 |

### 17.5 自查（改完必须重跑）

```bash
# 1) 本表"我们的文件"列里的每个路径都必须真的存在（改名漏一处就报）
awk -F'|' '/^\| `?src\// {print $2"|"$3}' docs/02-module-map.md \
  | grep -o 'src/[A-Za-z0-9_/.-]*\.rs' | sort -u \
  | while read -r p; do [ -e "$p" ] || echo "MISSING $p"; done

# 2) 旧的我方文件名必须清零（`_scaffold/` 内的新路径、以及本节自身的说明除外）
grep -rInE 'src/(api|broker|capi|diffconv|gdext|gdext_class|protocol|replay_inflate|replaydiff|
rng|ai_explain|turn|aigen|play_control|life|path|tick|replay|shot|settle|levelgen|levelgen_tables|
function/mod|function/to_text)\.rs' --include='*.md' --include='*.rs' --include='*.py' \
  --include='*.toml' --include='*.gd' . | grep -v 'src/_scaffold/' | grep -v 'docs/02-module-map.md'
```

> 第 1 条是**真检查**：表里写了 `src/xxx.rs` 而文件不在，就说明改名没改完。
> 第 2 条只查**旧的我方文件名**（`graphwar_*.rs` 这类**原版名**会合法地出现在文档里，不能一刀切）。
> 改完还要跑 `cargo build --offline` / `cargo test --offline` /
> `python3 tools/coverage_matrix.py --check`（见 §17.6 与 `docs/149`）。

### 17.6 本轮改了什么（一句话）

1. **14 个文件**用 `git mv` 改成原版名（`git log --follow` 全部能追到搬动之前）：
   * 8 个整文件 1:1 改名：`turn` / `aigen` / `play_control` / `life` / `path` / `tick` / `replay` / `serverloop`；
   * 6 个"一个原版文件、我方两份"：主文件用原版名，第二份进同名目录
     （`graphwar_shot_calculator/settle.rs`、`graphwar_level_generator/tables.rs`、`math_function/to_text.rs`）；
2. **11 个**我方自造模块移进 `src/_scaffold/`（外加 `mod.rs` 与 `README.md`，那里写明"这些不是原版的东西"）；
3. 混装的 5 个文件（`events` / `game` / `plane` / `server` / `turnstate`）**原地不动**，登记在 §17.4；
4. `polish_notation_converter.rs` 的对应关系按锚点**更正**（旧矩阵把 `to_text.rs` 记在它名下）。
5. 全仓库的 `src/<旧名>.rs` 引用（文档 / `client/**.gd` / 代码注释 / `tools/coverage_matrix.py` 的映射）
   一并改完，并用 §17.5 的两条命令自查过。

