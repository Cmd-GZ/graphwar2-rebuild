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
| `graphwar_ad_switch.rs` / `android_iap.gdc` | 广告 / 内购 |

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
  漂移的是那份 FDE 表，不是反汇编。已在 `docs/06` §11 更正。
- 归属流程改为：Ghidra 函数表 →（减 `0x100000`）→ 虚地址边界 → 与 capstone
  指令地址对齐。转换结果落在 `analysis/funcstarts_ghidra_va.pkl`（13263 条）。
- §8 / §10 的数字已按新边界重算；旧数字（483 / 628）作废。

### 12.4 仍然成立的结论

模块↔字符串的**关联**不受影响（那是"字符串被谁引用"，只看 xref，不需要函数边界）。
受影响的只是"某个函数算哪个模块"的判定，因此 §8/§10 之外、
依赖具体函数地址的结论（如 §11 里的 plane 地址清单）都需要按新边界重取。

## 13. 首批逐模块反编译观测（2026-10，含负结果）

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
> 适应度、装配、变异、每代构成、两阶段评分、25 s 预算都在 [`AI.md`](AI.md)（唯一权威）。
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

`0x465780`（95 行）是典型 SIMD 字节扫描：把 16 字节向量逐位压成 16 位掩码再取反
（编译器生成 `memchr`/`strchr` 一类循环的写法）。当时的推断是"在扫 1000×600 掩膜找第一个命中像素"，
**未证实**：没看到缓冲区大小、没看到下标换算、也没与 `0x4f15b0`（单点测试）建立调用关系。

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

用户反复问"所以到底漏了什么"。此前唯一的答案是 `docs/88` 的**模式维度**矩阵
（`TurnMode × FunctionMode × TimeMode × AxisMode`），它回答的是"我这些结论在哪个
模式下成立"，**没有回答"哪些代码我从没看过"**。§1–§7 的表全是按模块名**推断**的职责，
不是审计。

工具：`tools/module_audit.py`（输入全是已有夹具，不重新反汇编）。

### 14.2 口径，以及它为什么不能当完成度

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
| **对局元信息** | `function_manager`(7)、`rank_manager`(4)、`name_manager`(3)、`local_player_manager`(3)、`event_handler`(3)、`logger`(3)、`campaign_control`(1)、`lobby_control`(1) | **25** | 真盲区。`name_manager` / `rank_manager` 对应 `docs/96` 里缺的 `NameInfo`/`RankInfo` 一组事件 |
| **音频** | `audio_control`(14) | **14** | `.so` 里有完整的 WAV 解析错误串（`no WAVE tag found`、`no RIFF tag found`、`bits per sample is not 8 or 16`…），说明**音频是运行时解析 WAV 的**。与用户说的"有一个文件损坏"直接相关：坏文件会在这里被拒 |
| **广告 / 后端** | `ad_switch`(7)、`backend`(6) | **13** | 见 §14.5 |
| **录像 / 回放 / 视频** | `replay_control`(3)、`video_writer`(3)、`game_recorder`(2) | **8** | 我们只做了**读**回放（`tools/parse_replay.py`），没做**写**。注意还有 `publish_video_to_album` / `Android_mediastore_holder`——"把回放发到相册"是 Android 专属 |
| **联机外围** | `lan_broadcaster_v6`(4)、`server_broker_handler`(3) | **7** | 房间发现与 broker，见 `docs/24` §7 |
| **已知假阴性** | `function_input`(22)、`plane_pos_manager`(7) | **29** | §14.2 说的两个反例 |

### 14.5 顺带确认的两件事

1. **广告 / 内购在 `.so` 里是有代码的**，不是"Linux 上不存在"这么简单：
   字符串里有 `IAP_holder`、`Ad_holder`、`purchase_pending` / `purchase_confirmed` /
   `purchase_denied`、`create_ad_view` / `destroy_ad_view`、`request_permissions_result`、
   `unlock_skins`、`purchase_graphwar_support`。
   这些是 Godot 的 **Android 单例**调用（`Android_mediastore_holder` 同类），
   ⇒ **Linux 端跑不到**（用户判断正确），但**代码路径在**，要看行为得走 APK。
2. **段位图标共 22 张**：`res://ranks/rank_0.svg` … `rank_21.svg`。
   `graphwar_campaign_control.rs` 只有一个函数，配合 `user://campaign.save`，
   ⇒ 战役进度**存在存档文件里，不在代码里**（推断：那 1 个函数只做存取）。

### 14.6 这份审计没做的事

- 没区分"函数属于游戏逻辑"还是"编译器/库内联产物"——`module_funcs.tsv` 已经是
  "能归到 `graphwar_*.rs` 的"，但传播层（`module_map_propagated.tsv` 的 `inferred` 列）
  会把邻居也算进来，所以 885 这个总数是**上界估计**；
- 没评价"看懂了多少"，只评价"提没提过地址"；
- `NO-CITE` 的 28 个模块**没有逐个反编译**，§14.4 的"说明"列是据名字与同族字符串的推断。
