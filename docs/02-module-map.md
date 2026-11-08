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

~~另有 3 个 33 行的极小函数（`0x554730/750/770`）不带模块锚，归属未知。~~
**✅ 已结案（2026-11，`docs/151` §2.7）**：这一簇**不是游戏代码**，是**三方 X.509/CRL 栈**
（`webpki`/`rustls` 一类）的 panic/`format_args!` 助手 —— 它们 `lea` 的全是证书词汇
（`revocation_date` / `reason_code` / `invalidity_date` / `KeyCompromise` / `CaCompromise` /
`signed_data` / `BorrowedCertRevocationList` / `next_update` @`0x1c94bc`–`0x1c971f`）。
而且 **`0x554730` / `0x554750` 根本不是函数起点**：`.eh_frame` FDE 给出这两处所属函数是
`0x5546c0..0x554762`；只有 `0x554770`（`0x554770..0x554826`）是起点。
⇒ **不进复原分母**（AGENTS §4：指不到 `graphwar_*.rs` 的一律不算原版模块）。

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
| `0x3e6830` | **`Event` 的析构（drop glue）**——不是"事件骨架加工" | `0x3d38f0` 的每个分支在置 `ebx` 前都 `call 0x3e6830(rdi=rsp)`，收尾把手上那个 `Event` 丢掉。**真长度 `0xd5`（`0x3e6830..0x3e6905`，FDE），不是 `0x140`**：Ghidra 把隔壁 `0x3e6910` 并了进来，`archive/112` 记的"自身调 `0x3e6600`/`0x3e6370`"其实是 `0x3e6910` 的调用 | **观测**（`docs/151` §2.1：零次写回 `(%rdi)`；69 项跳表 50 项 = `pop rbx; ret`；其余臂只 `jmp *0x9ba5a0` → `dealloc` `0x528070`） |
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

> ⚠️ **本节所有模块名都是从 `.rodata` 残留串反推的。**
> 2026-11 拿到了 **2024-07-30 版未 strip 的符号表**，从中读出 78 个模块、165 个类型、290 个方法名，
> 并解出本节「未知」档 22 个模块里的 21 个真身。**名字以 [`§18`](#18-权威名字清单2024-07-30-版未-strip-的符号表2026-11) 为准**；
> 本节 §17.1/§17.2/§17.4 里有 5 处归属被 §18.7 更正（`weak_caller` / `graphwar_crash_reporter` /
> `GameInfo` / `FunctionRandomizer` / `graphwar_replay_player`），本节的表**暂时保留原样**以免与其他分支冲突。

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

> 🔴 **审计 2026-11-08：§17.1 最后一组「`未实现`」（44 行）与 §17.2 里的「`未实现`」行已经整体过期，
> 别当进度读。**
> 它们写的是 `src/` 大规模改名/落码**之前**的状态（那时不少界面向的文件还挂在 `game.rs` / `shot.rs` 这类自造名下）。
> 现在 **这 44 个模块 + §17.2 的 15 个模块在 `src/` 里全部有同名文件**（本轮逐个 `test -f` 验证，59/59 存在：
> `graphwar_campaign_control.rs` / `graphwar_color_manager.rs` / `graphwar_audio_control.rs` /
> `graphwar_graphics_manager.rs` / `graphwar_draw_util.rs` … 全部存在），
> 且 `python3 tools/coverage_matrix.py --check` 的总账是 **527 / 527 已实现、未做 0、未知模块 0**。
> ⇒ **进度一律以 [`149-coverage-matrix.md`](149-coverage-matrix.md) 为准**；那一组的「`未实现`」标签
> 只是**历史**（保留原样是为了不丢"当时判断"的轨迹，也是 `AGENTS.md` §0.6 说的
> "把已做完的写成没做"那类虚报的样本）。
> 每一行的**依据列**（锚点 / 文档 / 已解出的判据）仍然有效，仍可当该模块的入口读。

### 17.1 58 个 `graphwar_*.rs`

| 原版模块（函数/锚点） | 我们的文件 | 依据（锚点 / 函数名 / 文档） | 状态 |
|---|---|---|---|
| `graphwar_turn_manager.rs`（14/45） | `src/graphwar_turn_manager.rs` | 锚点 14 个（0x463790 订阅 new/entity_removed、0x4638f0、0x463da0 轮转体、0x465620、0x465780、0x470eb0…）；`docs/98`（Vec<soldier_id>+游标）、`docs/117`（真实 TCP 上的射手轮转断言）、`docs/14-turn.md` §9。原文件 `turn.rs` | `一一对应` |
| `graphwar_function_evolutionator.rs`（16/25） | `src/graphwar_function_evolutionator.rs` | 锚点 16 个（0x41b880/0x41c360/0x41c720 抽样与装配、0x4825b0/0x482c20/0x4839e0 带 `mid > len`/`VAR_XVAR_Y` 的 token 操作）；`docs/71`（权重表 `random % 85`、常数 `N(0,1)×10.2`）、`docs/74`（适应度）、`docs/147`（两阶段评分与预算）、40 万次运行时抽样。原文件 `aigen.rs`。⚠ 含我方接线层 `SoldierEvolver`（见 §17.4） | `一一对应` |
| `graphwar_play_screen_control.rs`（10/64） | `src/graphwar_play_screen_control.rs` | 锚点 10 个（0x3da130、0x3dee00…，`docs/02 §11` 同款口径）；`docs/14-turn.md` §9 逐函数规格；`docs/135`（开火特效）。原文件 `play_control.rs`。⚠ 旧矩阵（`docs/149`）把它记成 `未做`，与本文件头和 `src/lib.rs` 冲突 —— 本轮按代码更正 | `一一对应` |
| `graphwar_life_manager.rs`（9/12） | `src/graphwar_life_manager.rs` | 锚点 9 个（0x454b40…0x455b60，`docs/02 §13.1` 那张独立表逐个相同；0x454950 是 `&1` 存活判定）；`docs/65`（穿过即死/友军致死，真实对局与回放逐个相同）。原文件 `life.rs` | `一一对应` |
| `graphwar_path_calculator.rs`（1/9） | `src/graphwar_path_calculator.rs` | 采样器 `0x3c2530`（`docs/04`）+ `docs/91`（补 `hi = mid2` 后 24/24 用例整条点序列逐位相等）。⚠ 采样器本体**没有** panic 锚（**§20 复核结论不变**：`0x3c2530` 1,680 B 全函数体 0 个 Location 引用）；但「本模块只归属到 1 个辅助函数（0x469630/0x4696d0）」已被 **§20 更正**——本模块自己的 9 个 Location 锚**全部**落在 **`0x467530`**（= `calculate_paths`，5,408 B），`0x469630`/`0x4696d0` 只是 160 B 的 panic/drop 胶水。⇒ 「采样器 `0x3c2530` 属于本模块」仍是文档级归属，**不是**锚点归属（§20.5 / §20.7 第 2 条）。原文件 `path.rs` | `一一对应` |
| `graphwar_replay_player.rs`（5/18） | `src/graphwar_replay_player.rs` | 锚点 5 个（0x3d6d70、0x3d7380、0x3d76f0 写 `user://replays/`）；`docs/133` §2（gzip+JSONL 格式）、`docs/136/142`（400 份回放消息级对拍，71 份完全一致）。原文件 `replay.rs`。⚠ 文件内 `ReplayPlayer`（播放时钟）是我方自定模型，见 §17.4 | `一一对应` |
| `graphwar_tick_handler.rs`（2/3） | `src/graphwar_tick_handler.rs` | `docs/35`（113 个 `TickReply` 样本逐字节）+ `docs/42`（TimeMode → 回合时长）。原文件 `tick.rs`。⚠ **待核实**：本模块的锚函数是 0x51a610 / 0x51a750 / 0x3d76f0（后两个与回放写盘同址、`contested` 与 replay_player 共有），与本文件实现的"60Hz tick 时钟 + 时长表"**没有函数级对应**；现在的对应来自矩阵旧行与模块名，属**推断**。**2026-11 复核（`tools/addr_probe.py locs`，`docs/151` 第 27 条）⇒ 核实失败**：`0x51a750`（`0xcf0` B）体内 5 条 `panic::Location` 全指 `src/graphwar_function_evolutionator.rs`（⇒ 它是**进化器**的函数，不可能是 tick_handler）；`0x51a610`（`0xd6` B）只有 1 条且指 `src/graphwar_local_player_manager.rs`；`0x3d76f0`（`0x254` B）**零** Location。三个锚都撑不住本行，函数级定位要重找 | `⚠核实失败（锚不属本模块）` |

| `src/graphwar_level_manager.rs` | 原版 `graphwar_level_manager.rs` + **可能属于 `graphwar_campaign_control.rs`** 的 `level_info`(`0x4ad380`) 与 `campaign.save` 簇(`0x417ef0`/`0x418170`/`0x4183b0`) | 这三处**都没有 Location 锚**；`docs/10` §12.6 | 无锚 ⇒ 归属只能靠调用形态，证据不足。暂时承载并登记，等 `graphwar_campaign_control.rs` 有锚点再决定搬不搬 |
| `graphwar_level_generator.rs`（4/26） | `src/graphwar_level_generator.rs` + `src/graphwar_level_generator/tables.rs` | 锚点 26 个（0x466d20/0x467530 等）；`docs/55/56/61`（障碍 x/y/r 的 `to_bits()` 13/13 逐位）、`docs/103/126`（种子语义）。原文件 `levelgen.rs` + `levelgen_tables.rs`（ziggurat 表）——**同一个原版文件，我方按"逻辑 / 常量表"拆成两份**，主文件用原版名，表放同名目录 | `一一对应` |
| `graphwar_shot_calculator.rs`（1/12） | `src/graphwar_shot_calculator.rs` + `src/graphwar_shot_calculator/settle.rs` | 锚点 12 个（0x3c5f70 修正常数、0x3c62b0 等）；`docs/05`（修正 `c = y0 − f(±x0)`）、`docs/86 §14`（82 发并集 152/152）、`docs/122`（按 id 排除射手）。原文件 `shot.rs` + `settle.rs`——**同一个原版文件**（0x3c2bc0 主函数 + 0x3c5f70 修正），我方按主题拆两份。⚠ **本节原写「`0x3c2bc0` 本体没有 panic 锚」，该条已被 §20 证否**：它有 **13 个 `src/*.rs` 锚**（12 个 `graphwar_shot_calculator.rs` + 1 个 `graphwar_obstacle_manager.rs`），其中 **11 个双向唯一**⇒ 本函数已由锚点定名为 `GraphwarShotCalculator::calculate_shot_results`（`观测`，§20.5）；"settle 属于本模块"不再只靠 `docs/86` 的行为证据 | `一一对应` |
| `graphwar_event.rs`（0/0） | `src/events.rs` | `enum Event` **74** 个具名变体（**审计 2026-11-08**：原记 72；`docs/07` §5.1 已用 arm64 跳转表把 74 格全部具名）+ `GameInfo`（`docs/114`：判别值逐条解出、两道自校验；`docs/07 §3.1` 9 个字段）。⚠ 本模块**零锚点**（纯数据），对应关系靠职责与 `docs/114`。同文件还收了 event_handler 的名字表 → 见 §17.4 | `混装（待拆）` |
| `graphwar_event_handler.rs`（2/4） | `src/events.rs` | 只落了一张表：`DISPATCH_NAME_TAGS` + `from_dispatch_name`（从 `0x3d38f0` 逐字节解出的"事件名 → 判别值"，`docs/114` §3）。⚠ `0x3d3630`（用常量 key 订阅 `HashMap<u64,_>`）**未实现**，所以本模块**没有** handler 行为，只有数据 | `混装（待拆）` |
| `graphwar_remote_event_handler.rs`（3/171） | `src/events.rs` | 只落了枚举与名字表（同上）。本模块 171 个锚点是全二进制第三密，我方**未实现**其分发/订阅行为；旧矩阵把 `events.rs` 记成它的实现，口径偏宽 —— 本轮按"没有 handler 代码"重记 | `混装（待拆）` |
| `graphwar_game_state_manager.rs`（33/207） | `src/game.rs` + `src/turnstate.rs` | `docs/41`（6 状态 7 事件、13 次迁移在真实对局里逐个相同）、`docs/150`（胜负判定）。两个我方文件都属本模块，但 `game.rs` 还含 function_manager 的内容 → 见 §17.4 | `混装（待拆）` |
| `graphwar_function_manager.rs`（7/12） | `src/game.rs` | `SoldierFunctions`（`HashMap<u32, 112 字节>`、entry 120、value 开头 `Vec<Token>`）逐条读自 `docs/112` §0/§2.1；7 个锚函数在 0x455b60…0x456f80。⚠ 与 game_state_manager 混在同一文件 | `混装（待拆）` |
| `graphwar_game_server.rs`（3/86） | `src/graphwar_game_server.rs` + `src/server.rs` | `docs/43/46`（一回合 12 条报文与抓包逐字节）、`docs/62`（真实 TCP 跑到 EndingGame）、`docs/117`（回合推进接线）。原文件 `serverloop.rs`（主循环）+ `server.rs`（首包同步，与 server_connection_handler 共有） | `混装（待拆）` |
| `graphwar_server_connection_handler.rs`（6/24） | `src/server.rs` | `docs/33`（新连接 7 条状态同步逐字节一致）；`docs/112` §15.3 指出建事件名数组的 `0x4500c0` 就在本模块。⚠ `server.rs` 里还有 `on_tick_request`（tick 应答）→ 见 §17.4 | `混装（待拆）` |
| `graphwar_plane.rs`（10/19） | `src/plane.rs` | `docs/08/39`（掩膜光栅化与 `is_occupied` 逐字节 / 300 随机点一致）；`px = x*1000/50 + 500` 来自 `0x4be3e0`。⚠ 同文件还含 plane_pos_manager 与（疑似的）obstacle_manager 内容 → §17.4 | `混装（待拆）` |
| `graphwar_plane_pos_manager.rs`（6/9） | `src/plane.rs` | `docs/140`：`px = 20x + 500` 由常量反查得出。⚠ 本模块的锚函数在 `0x3c7330…0x3c7cb0`（与 path_calculator / shot_calculator 同区段），与 `plane.rs` 里那条换算**没有**做过函数级对齐 ⇒ "plane.rs 属于本模块"是**推断**。**2026-11 复核（`docs/151` 第 28 条）**：`0x3c7330` 只有 `0x2f`（47）字节、`0x3c7cb0` 是 `0x8d9` B，**两者都零 `panic::Location`** ⇒ 函数级对齐仍然做不到，缺的是非锚证据（结构/体量/常量指纹） | `混装（待拆，锚无效）` |
| `graphwar_obstacle_manager.rs`（8/10） | `src/plane.rs`（只有掩膜部分） | **2026-11-07 更正（`docs/16` §24.4 第 1/1b 行）**：本模块的锚函数是 **10 个** —— `0x3c2bc0 0x3ee190 0x3f11e0 0x3f15b0 0x3f17a0 0x3f19a0 0x3f1b30 0x3f1d30 0x3f20f0 0x3f2360`（= `fixtures/module_funcs.tsv` 本模块 10 行）。旧文那 4 个（`0x3c64c0/0x3f00b0/0x3f2480/0x3f27d0`）是 `class_probe` v1 的 `+0x1000` 平移 artifact。**旧「两处证据冲突」可结案**：`0x4f15b0` 是 Ghidra 口径，其 ELF 虚地址 `0x3f15b0` **就是**本模块锚函数之一；所谓冲突是地址空间混用 + v1 平移共同造成的。`docs/08/39` 覆盖的光栅化 / `is_occupied`（摧毁 = 挖 r=0.5 洞）不变。障碍的生成与生命周期仍未落码 | `混装（待拆）` |
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
| `graphwar_game_client.rs`（36/59） | `src/graphwar_game_client.rs` | 归属函数最多的模块（36 个）；只有 `docs/archive` 的调用形态观测 | `观测` |
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
| `graphwar_module_container.rs`（20/1） | `src/graphwar_module_container.rs` | `docs/02 §22`（2026-11 全解）：类型本体 = `HashMap<TypeId, Rc<dyn Any>>`（48 字节 / 1 字段，**没有任何 manager 偏移**）；3 个泛型方法 × 25 型 = 68 个单态化，两架构地址全定名；`add_module`/`get_module` 零 Location 锚是**正确结果**（无 panic 点） | `观测` |
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

### 17.2 非 `graphwar_` 模块（`other` 作用域）

> ⚠️ **审计 2026-11-08**：标题原写「**12** 个」，但 `docs/coverage-matrix.tsv` 里 `scope=other` 是 **16** 行
> = 下表 12 个 + `backtracer.rs` / `function_token.rs` / `graphwar_texture_loader.rs`（这三个表中缺）+ `weak_caller.rs`。
> 注意 **`graphwar_texture_loader.rs` 是带 `graphwar_` 前缀的**，所以"非 `graphwar_`"这个说法本身不准确；
> 准确的口径是 **`scope=other` 的 16 个模块**（`docs/149` 表里那 16 行、合计 97 个可归属函数）。

下表这 12 个文件名不带 `graphwar_` 前缀，但**是游戏自己的模块**（§9 提醒过：`entity_drawer.rs` /
`effect_drawer.rs` 虽无前缀，确实是游戏代码）。缺的 3 个见 [`149-coverage-matrix.md`](149-coverage-matrix.md)。

| 原版模块（函数/锚点） | 我们的文件 | 依据（锚点 / 函数名 / 文档） | 状态 |
|---|---|---|---|
| `math_function.rs`（7/15） | `src/math_function.rs` + `src/math_function/to_text.rs` | **2026-11-07 更正（`docs/16` §24.4 第 2 行）**：`math_function.rs:285:28` 全库唯一引用指令是 `0x4ece8f` ⇒ 属 **`0x4ec160`**（`to_string`）；解析器 `0x4ed670` 自己引的是 `math_function.rs:75:10`（另有一条 regex 池 `350:17`）。旧文那句是 `class_probe` v1 的 `+0x1000` artifact（`0x4ece8f + 0x1000 = 0x4ede8f` 被归给了 `0x4ed670`）。求值器 `0x4eb9d0` 与它同层；token → 文本 `0x4ec160` 的 panic 分支同样引 `math_function.rs:285:28`（`docs/119` §4.6，指令级）。原目录 `function/`。**同一个原版文件**，我方按"解析/求值"与"反向打印"拆两份。另见 `polish_notation_converter.rs` 的正名（本轮更正） | `一一对应` |
| `polish_notation_converter.rs`（1/2） | — | **2026-11-07 再更正**：本模块的锚函数是 **`0x4f5450`**（引用指令 `0x4f5789` / `0x4f577c`，`docs/16` §24.4 第 3 行），不是 `0x4f57d0` —— 后者是 `class_probe` v1 的 `+0x1000` artifact。**也不是** `0x4ec160`（那是 `to_string`）。旧矩阵把 `src/math_function/to_text.rs` 记在这里 —— 但 `0x4ec160` 的 panic 分支引的是 `math_function.rs:285:28`（`docs/119` §4.6），按锚点应归 `math_function.rs`。⇒ `to_text.rs` 移到 `src/math_function/`，本模块改记 `未实现` | `未实现` |
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

> `weak_caller.rs`（39 个函数）不在上表里。~~它是第三方依赖库，不算游戏代码~~ —— **审计 2026-11-08 更正**：这条旧判据已被 **§34** 推翻（符号表里 `graphwarrust::weak_caller` 的 27 个 direct 符号 ⇒ **crate 内模块**，不是第三方），所以它的 39 个可归属函数**要计入**总账（`docs/149` 的 scope=`other` 一档 97 个里就含它；430 + 97 = 527）。旧口径 `dep` / 40 个函数均已作废。

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

> **`src/_scaffold/root_wiring.rs` / `graphwar_root_node_class.rs` 不在上表**：它们是
> **22 个 manager 的装配器 + 它在 Godot 侧的观测口**（原版这两件事在 `GraphwarGameClient::new`
> 与各节点的实现里，`docs/16` §41），不是"桥接 A 到 B"那一类，所以没有进 §17.3 的桥接清单。
> 它们身上有一个**判据口径**要单独记住：**槽位复原档位** `wired` / `layout` / `assembled` / `stub`
> （判据写在 `src/_scaffold/root_wiring.rs` 的模块文档，全表与逐槽理由在 `archive/156-client-rounds.md` §47）。
> ⚠️ 别把它和 `SlotKind` 混起来：`SlotKind` 是**容器类型来源**（容器里直接装原版模块类型 = 17 个，
> 装 `_scaffold` 包装 = 5 个，`container_real_manager_count()` / `container_stub_manager_count()`
> 数的就是这两个），档位回答的是"这一槽离原版还有多远"（2026-11 任务 28 定）。
> 读 `module_slot_status()` 的返回值时，`stub` 的是 **1** 个，而 `SlotKind::Stub` 的是 **5** 个。

> ⚠️ **§8 与本节的"归属函数数"口径不同**：§8 那张表来自 `fixtures/module_map.tsv`（按"体内引用本模块"粗算，
> `game_client` 记 **68**），本节来自 `docs/coverage-matrix.tsv`（按 `panic::Location` 锚，
> `game_client` 记 **36**，19 个 contested 压平）。**两个都对，但回答的问题不同**：
> §8 回答"这个模块的代码被哪些函数引用"，本节回答"有多少函数能逐条归属到本模块"。
> 引用时请写清用的是哪个口径（`docs/16` §17.1 有更细的对比）。

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



## 18. 权威名字清单：未 strip 符号表（2026-11）

> ⚠️ **2026-11-07 补充：本节那张表来自 2024-07-30 版（x86-64），但它不是唯一来源，也不是最权威的来源。**
> 实际上**有两份构建都没 strip**：
>
> | 来源 | 版本 | 架构 | 用途 |
> |---|---|---|---|
> | `.reference/steam/2024-07-30/` | 旧 | **x86-64**（与目标同架构） | 本节这张表就是它跑出来的（当时手上只有这份同架构文件） |
> | `.reference/steam/android-2.0.60/` | **2.0.60（与逆向目标同版本）** | arm64 | **对"与目标版是否一致"这个问题，它更权威** |
>
> 两者都有完整 `.symtab`。判断某模块名/函数名在 **2.0.60** 里是否还叫这个，
> **优先看 Android 那份**，2024 那份只作交叉验证；两版不一致时以 **Android** 为准并记录差异。
> 例：`button_manager` 2024 有 9 个符号、Android 2.0.60 有 10 个。

> **本节是全仓库唯一一处「名字有物证」的地方。**
> §1–§17 的模块名、函数名，全都是从 `.rodata` 里残留的 `src/<mod>.rs` 字面量**反推**的；
> 本节的名字来自**符号表本身**。

### 18.0 一句话

2026 年的三个版本都被 `strip` 了，只剩 `.dynsym`；但 **2024-07-30 首发周那一版没 strip**，
带完整 `.symtab`（**13,086** 个 `FUNC`，其中 **1,315** 个名字里含 `graphwar`）。
它直接给出 **78 个模块、165 个类型、290 个源码级方法名**，并把我们标着「完全未知」的
**22 个模块解出 21 个真身**（只剩 `graphwar_crash_reporter` 不在里面——见 §18.7 第 2 条）。

三个立刻可用的结论：

| 我们现在写的 | 物证给出的真名 |
|---|---|
| 采样器 `0x3c2530`（"语义已知、名字未知"） | 名字确是 `GraphwarPathCalculator::calculate_paths`，**但它不在 `0x3c2530`**——§20 把这个名字锚到 **`0x467530`**；`0x3c2530` 全函数体 **0 个 Location 引用**，用锚点无法定名（仍是 `未知`）。**补充（`docs/151` §2.5，观测）**：它**零 `call rel32`**，唯一入口是 `.data.rel.ro` 的函数指针槽 **`0x9bac98`**（`R_X86_64_RELATIVE` addend = `0x3c2530`），全库只有 2 处使用：`0x3c30c1`（在 `0x3c2bc0` 体内）与 `0x3e2d35`（在 `0x3e2830` 体内）⇒ 复查任何"采样器没被调用"的结论前，先看这个槽 |
| 开火结算 `0x3c2bc0`（12 KB，"没有逐条还原"） | `GraphwarShotCalculator::calculate_shot_results` —— §20 已用 **11 个双向唯一锚**把它**落到地址上**（`观测`），不再只是名字级推断 |
| `SETUPS[21]`（`docs/10` 从数据表反推的 21 关） | `setup_level_0` … `setup_level_20`（21 个，一个不多一个不少） |



### 18.1 物证、口径、以及这条路能走通的边界

| 项 | 值 | 档位 |
|---|---|---|
| 二进制 | `.reference/steam/2024-07-30/libgraphwarrust.so`，sha256 `6eee7bb855d7fc46f6c9e9bb90bb233100b157e10b27c06c8fb9ba07e510db29` | `观测` |
| 符号表 | `.symtab` 段 `0x08bd88` 字节 / 23,715 条 / `FUNC` **13,086** 条 | `观测` |
| 名字编码 | 全部是 Rust **legacy** mangling（`_ZN…17h<16hex>E`）；**一个 `_R`（v0）都没有** | `观测` |
| 名字解出的方式 | `readelf -sW` 取 `FUNC` 行 → `c++filt --format rust` 一次性解 13,086 条 | `观测` |
| 这些名字对 **2.0.60** 也成立 | 两年间 `src/*.rs` 文件名只差 1 个增 1 个删（§18.7），推断源码改动集中在函数体而非改名 | **`推断`** |
| 本节的地址 | **一律是 2024 版的 ELF 虚地址，禁止挪用到 2.0.60**（两版 `.text` 差 40 万字节） | — |

**能跨版本复用的只有两样东西**，其余都作废：

1. **名字**（模块 / 类型 / 方法）；
2. **`src/<file>.rs:LINE:COL`**——它来自 `panic::Location`，是**源码坐标**，与机器码地址无关。

> ⚠️ 这正是本节比 §17 硬的地方：§17 只能靠"`.rodata` 里有没有 `src/graphwar_x.rs` 这个串"
> 判断模块存在；本节既有符号名、又有 `Location` 源坐标，两条独立证据链可以互相对账（§18.2）。

**符号分类**（后面所有统计都按这个口径）：

| 类 | 定义 | 例 | 数量 |
|---|---|---|---|
| `direct` | 名字**以** `graphwarrust::` 开头 | `graphwarrust::graphwar_shot_calculator::GraphwarShotCalculator::calculate_shot_results` | **692** |
| `impl` | 以 `<…graphwarrust::…>` 开头 = 给游戏类型做的 trait impl | `<graphwarrust::graphwar_plane::GraphwarPlane as …IControl>::draw` | **253** |
| `generic` | std 的泛型单态化，只是**提到**了游戏类型 | `core::ptr::drop_in_place<…TickEvent…>` | **370** |

`generic` 那 370 条**不是**游戏代码，不能算进度；但它们把 165 个类型名露出来了（§18.3 的「类型」列）。
`impl` 那 253 条里有一大半是 gdext 样板（`__register_methods`、`__virtual_call`、`class_name`），
**只有 `IControl::draw` / `INode::ready` / `ILineEdit::process` 这类虚函数才是原版真写过的方法**。



### 18.2 总量与交叉验证（这两个数字互相独立）


#### 总量

* 含 `graphwar` 的 `FUNC` 符号：**1315** 个（direct 692 / impl 253 / generic 370）
* 模块：**78**；类型：**165**；非样板方法名（去重）：**290**
* `panic::Location` 结构体：**5015**；其中游戏自己的 `src/*.rs` 文件 **70** 个
* 能配上 Location 锚的函数：**2747**

**交叉验证**：692 个 direct 函数里 433 个带 `src/*.rs` 锚；其中 **426** 个的「符号名模块段」正好等于某个锚点文件的 stem（98.4%）。不一致的 7 个全是泛型/内联复用（见 §18.6）。

第二段交叉验证值得单独说：`direct` 符号的**名字里的模块段**，与它体内 `panic::Location`
给出的**源文件 stem**，在 **433 个带锚的函数上一致 426 个（98.4%）**。
两条证据链互相独立（一条来自 mangling、一条来自 `.data.rel.ro` 的重定位），
所以本节的模块名可以当**权威**用。**7 个不一致的全是泛型/内联复用**，逐个如下——
它们不是错误，是「这个函数为多个模块的代码做同一件事」：

| 符号的模块段 | 锚点落在 | 个数 | 解释（`推断`） |
|---|---|---|---|
| `graphwar_backend` | `graphwar_game_client.rs` | 4 | `GraphwarBackend` 的 `start_*` / `stop_*` 被 `GraphwarGameClient` 内联 |
| `graphwar_play_screen_control` | `weak_caller.rs` | 1 | `WeakCaller` 是泛型，闭包落在 `weak_caller.rs:11–17` |
| `graphwar_target_manager` | `weak_caller.rs` | 1 | 同上 |
| `graphwar_lan_vbox` | `graphwar_root_node.rs` | 1 | `GraphwarLanVBox` 的 `update_game_list` 被 `root_node` 内联 |



### 18.3 A. 权威模块清单


读法：**「函数」= `.symtab` 里 `graphwarrust::<模块>::…` 的 `FUNC` 条数**（含 `panic_cold_explicit`
与闭包，是"编译出来的函数个数"，**不是**原版源码里的方法个数）；**「方法」= 去掉样板与闭包后的
源码级名字个数**。
**「行范围」= 该模块自己的函数体内 `Location` 锚在 `src/<模块>.rs` 上的最小–最大行 + 锚点个数**
——这是**原版该文件长度的一个硬下界**（例如 `graphwar_level_manager.rs` 至少 1,888 行）。
最后两列是 gdext/第三方 trait 的 impl：有 `IControl`/`INode`/`IVBoxContainer` 才是 Godot 类，
与 §16.2 那 12 个类清单可以逐条对。


#### A. 权威模块清单（78 个模块；其中 76 个有自己的函数）

| 模块 | 函数 | 类型 | 方法 | `src/<模块>.rs` 行范围（锚点） | gdext trait impl |
|---|---|---|---|---|---|
| `axis_drawer` | 3 | `AxisDrawer` | 0 | 13–31（3） | `IControl`, `GodotClass`, `ImplementsGodotVirtual` |
| `backtracer` | 1 | — | 1 | — | — |
| `button_manager` | 2 | `ButtonManager` | 1 | 29–29（1） | — |
| `effect_drawer` | 6 | `EffectData`, `EffectDrawer` | 3 | 28–469（4） | `IControl`, `GodotClass`, `ImplementsGodotVirtual` |
| `entity_drawer` | 10 | `EntityDrawer` | 7 | 38–513（10） | `IControl`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `function_button_manager` | 3 | `FunctionButtonManager` | 2 | 28–33（3） | — |
| `function_drawer` | 7 | `FunctionDrawer` | 7 | 45–45（1） | — |
| `function_history_recorder` | 4 | `FunctionData`, `FunctionHistoryRecorder` | 2 | 66–78（3） | — |
| `function_token` | 1 | `FunctionToken` | 1 | — | — |
| `graphwar_ad_switch` | 3 | `GraphwarAdSwitch` | 0 | 7–45（3） | `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `graphwar_audio_control` | 15 | `EffectAudioData`, `FunctionAudioData`, `GraphwarAudioControl` | 8 | 168–472（15） | — |
| `graphwar_backend` | 20 | `GraphwarBackend` | 19 | 77–352（12） | — |
| `graphwar_button` | 5 | `GraphwarButton` | 2 | 14–109（3） | `IButton`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotExports`, `ImplementsGodotVirtual` |
| `graphwar_campaign_control` | 4 | `GraphwarCampaignControl` | 4 | 94–94（1） | — |
| `graphwar_client_connection_handler` | 11 | `ConnectionState`, `GraphwarClientConnectionHandler` | 7 | 65–255（12） | — |
| `graphwar_color_manager` | 8 | `GraphwarColor`, `GraphwarColorManager` | 5 | 46–134（6） | `Visitor` |
| `graphwar_computer_player_manager` | 11 | `BotData`, `GraphwarComputerPlayerManager`, `SoldierBotData`, `ThreadData` | 6 | 84–369（28） | — |
| `graphwar_draw_util` | 2 | `GraphwarDrawUtil` | 2 | 28–33（4） | — |
| `graphwar_effect_manager` | 3 | `EffectType`, `GraphwarEffectManager` | 2 | 64–104（3） | `Visitor` |
| `graphwar_entity_effect_manager` | 6 | `GraphwarEntityEffectManager` | 3 | 35–77（3） | — |
| `graphwar_event` | 2 | 73 个（见下） | 2 | — | `Clone`, `Visitor` |
| `graphwar_event_handler` | 3 | `GraphwarEventHandler` | 3 | 396–458（4） | — |
| `graphwar_event_queue` | 2 | `GraphwarEventQueue` | 2 | — | — |
| `graphwar_function_evolutionator` | 1 | `FunctionResult`, `GraphwarFunctionEvolutionator` | 1 | 306–331（5） | — |
| `graphwar_function_input` | 15 | `GraphwarFunctionInput` | 12 | 23–413（23） | `ILineEdit`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `graphwar_function_manager` | 10 | `FunctionState`, `GraphwarFunctionData`, `GraphwarFunctionManager` | 8 | 152–213（12） | — |
| `graphwar_game_client` | 13 | `GraphwarGameClient` | 12 | 51–239（72） | — |
| `graphwar_game_info` | 0 | `GameInfo` | 0 | — | `Visitor` |
| `graphwar_game_recorder` | 4 | `EventInfo`, `GraphwarGameRecorder` | 3 | 107–163（6） | `Visitor` |
| `graphwar_game_server` | 6 | `GameServerData`, `GraphwarGameServer`, `RxTxData` | 5 | 149–356（71） | — |
| `graphwar_game_state_manager` | 45 | `GraphwarGameResult`, `GraphwarGameState`, `GraphwarGameStateManager` | 30 | 117–1171（202） | `Visitor` |
| `graphwar_graphics_manager` | 9 | `GraphwarGraphicsManager` | 6 | 38–108（8） | — |
| `graphwar_lan_broadcaster_v6` | 5 | `GraphwarLanBroadcasterV6`, `LanGameInfoInt` | 5 | 36–138（5） | — |
| `graphwar_lan_vbox` | 5 | `GraphwarLanVBox` | 1 | 28–135（4） | `IVBoxContainer`, `ImplementsGodotVirtual` |
| `graphwar_level_generator` | 6 | `GraphwarLevelGenerator` | 4 | 49–261（26） | — |
| `graphwar_level_manager` | 50 | `GraphwarLevelManager`, `LevelData` | 19 | 121–1888（590） | — |
| `graphwar_life_manager` | 14 | `GraphwarLifeData`, `GraphwarLifeManager` | 9 | 76–175（12） | — |
| `graphwar_lobby_control` | 4 | `GraphwarLobbyControl` | 2 | — | — |
| `graphwar_local_player_manager` | 10 | `GraphwarLocalPlayerManager` | 4 | 76–146（15） | — |
| `graphwar_logger` | 1 | `GraphwarLogger` | 1 | — | — |
| `graphwar_long_press_animation` | 5 | `GraphwarLongPressAnimation` | 2 | 10–31（3） | `IControl`, `GodotClass`, `ImplementsGodotVirtual` |
| `graphwar_mode_manager` | 3 | `AxisMode`, `FunctionMode`, `GraphwarModeManager`, `TimeMode` | 2 | 132–134（3） | `Visitor` |
| `graphwar_module_container` | 66 | `GraphwarModuleContainer` | 3 | 30–30（19） | — |
| `graphwar_name_manager` | 7 | `GraphwarNameManager` | 4 | 34–79（4） | — |
| `graphwar_nodes` | 31 | `GraphwarNodes` | 3 | 8–8（26） | — |
| `graphwar_obstacle_manager` | 10 | `GraphwarObstacleData`, `GraphwarObstacleManager` | 8 | 89–295（13） | — |
| `graphwar_path_calculator` | 1 | `GraphwarPathCalculator` | 1 | 45–105（9） | — |
| `graphwar_plane` | 8 | `GraphwarPlane` | 4 | 33–178（4） | `IControl`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `graphwar_plane_pos_manager` | 9 | `GraphwarPlanePosData`, `GraphwarPlanePosManager` | 6 | 81–144（8） | — |
| `graphwar_play_screen_control` | 13 | `GraphwarPlayScreenControl` | 12 | 71–625（59） | — |
| `graphwar_player_config` | 8 | `GraphwarPlayerConfig` | 8 | — | — |
| `graphwar_player_manager` | 6 | `GraphwarPlayerManager` | 3 | 36–124（4） | — |
| `graphwar_player_screen_control` | 10 | `GraphwarPlayerScreenControl` | 7 | 118–271（9） | — |
| `graphwar_rank_manager` | 7 | `GraphwarRankManager` | 4 | 33–83（4） | — |
| `graphwar_remote_event_handler` | 3 | `GraphwarRemoteEventHandler` | 3 | 134–869（167） | — |
| `graphwar_replay_control` | 3 | `GraphwarReplayScreenControl`, `ReplayEntryInfo` | 2 | 95–197（8） | — |
| `graphwar_replay_player` | 9 | `GraphwarReplayPlayer` | 9 | 78–256（18） | — |
| `graphwar_replay_reader` | 2 | `GraphwarReplayReader` | 2 | — | — |
| `graphwar_root_node` | 13 | `GraphwarRootNode` | 10 | 66–1060（102） | `INode`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `graphwar_server_broker_handler` | 5 | `GraphwarServerBrokerHandler` | 5 | 75–114（4） | — |
| `graphwar_server_connection_handler` | 11 | `GraphwarServerConnectionHandler` | 5 | 49–306（14） | — |
| `graphwar_setup_control` | 18 | `GraphwarSetupControl` | 11 | 46–609（33） | `IControl`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `graphwar_shot_calculator` | 4 | `FunctionResult`, `GraphwarShotCalculator`, `GraphwarShotResult` | 4 | 181–419（12） | — |
| `graphwar_soldier_manager` | 7 | `GraphwarSoldierManager` | 4 | 35–105（3） | — |
| `graphwar_stream_buffer` | 2 | `GraphwarStreamBuffer` | 2 | 24–24（1） | — |
| `graphwar_target_manager` | 4 | `GraphwarTargetManager` | 2 | 35–84（3） | — |
| `graphwar_team_manager` | 10 | `GraphwarTeam`, `GraphwarTeamData`, `GraphwarTeamManager` | 7 | 63–139（10） | `Visitor` |
| `graphwar_texture_loader` | 6 | `GraphwarTextureLoader` | 6 | 145–145（1） | — |
| `graphwar_tick_handler` | 6 | `GraphwarTickHandler` | 5 | 42–125（4） | — |
| `graphwar_turn_manager` | 18 | `GraphwarTurnData`, `GraphwarTurnManager`, `TurnHolder`, `TurnMode` | 13 | 80–428（45） | `Visitor` |
| `graphwar_video_writer` | 3 | `GraphwarVideoWriter` | 3 | 60–153（4） | — |
| `math_function` | 10 | `FunctionRandomizer`, `MathFunction` | 10 | 52–454（8） | — |
| `obstacle_drawer` | 1 | `ObstacleDrawer` | 1 | 51–128（7） | — |
| `player_showcase` | 4 | `PlayerShowcase` | 1 | 9–37（5） | `IControl`, `GodotClass`, `ImplementsGodotApi`, `ImplementsGodotVirtual` |
| `polish_notation_converter` | 1 | `PolishNotationConverter` | 1 | 36–116（2） | — |
| `server_broker_request_connection` | 1 | `ServerBrokerRequestConnectionClient` | 1 | — | — |
| `time_probe` | 0 | `TimeProbe` | 0 | — | — |
| `weak_caller` | 37 | `WeakCaller` | 2 | 11–17（37） | — |

**没有一个自己的函数**（只在 impl / 泛型实参里露面）的模块：`graphwar_game_info`, `time_probe`

##### `graphwar_event` 的 73 个类型（符号表原样）

`AddObstacleEvent`, `AddSoldierRequestEvent`, `AxisModeChangeRequestEvent`, `AxisModeInfoEvent`
`BotAddedEvent`, `ChangeTeamRequestEvent`, `ChatMessageEvent`, `ChatMessageRequestEvent`
`ClearObstaclesEvent`, `ColorInfoEvent`, `ConnectedToServerEvent`, `ConnectionRemovedEvent`
`DestroyObstacleEvent`, `EffectInfoEvent`, `EndOfTurnInfoEvent`, `EntityEffectInfoEvent`
`EntityRemovedEvent`, `FaceGraphicsInfoEvent`, `FaceGraphicsRequestEvent`, `FunctionActiveEvent`
`FunctionFireEvent`, `FunctionFireRequestEvent`, `FunctionInfoEvent`, `FunctionModeChangeRequestEvent`
`FunctionModeInfoEvent`, `FunctionPointsEvent`, `FunctionUpdateRequestEvent`, `FunctionsCalculatedEvent`
`GameHeartbeatEvent`, `GameInfoEvent`, `GameNameInfoEvent`, `GameNameRequestEvent`
`GameRemovedEvent`, `GameStartRequestEvent`, `GameStateInfoEvent`, `GraphwarEvent`
`HatGraphicsInfoEvent`, `HatGraphicsRequestEvent`, `LifeInfoEvent`, `LifeQueuedDeathEvent`
`LockInfoEvent`, `LockRequestEvent`, `NameInfoEvent`, `NameRequestEvent`
`NewBotRequestEvent`, `NewConnectionEvent`, `NewConnectionRequestEvent`, `NewPlayerRequestEvent`
`PlayerInfoEvent`, `PosInfoEvent`, `QueueDestroyObstacleEvent`, `RankInfoEvent`
`RankRequestEvent`, `RemovePlayerRequestEvent`, `RemoveSoldierRequestEvent`, `RemoveTeamEvent`
`SkinGraphicsInfoEvent`, `SkinGraphicsRequestEvent`, `SoldierInfoEvent`, `SubscribeEvent`
`TargetInfoEvent`, `TeamInfoEvent`, `TickEvent`, `TickReplyEvent`
`TickRequestEvent`, `TimeModeChangeRequestEvent`, `TimeModeInfoEvent`, `TurnCountInfoEvent`
`TurnInfoEvent`, `TurnLimitInfoEvent`, `TurnModeChangeRequestEvent`, `TurnModeInfoEvent`
`VersionInfoEvent`

> **和 §16.2 的 12 个类对账**：符号表里带 Godot 基类 impl 的正好是
> `AxisDrawer` / `EntityDrawer` / `EffectDrawer` / `PlayerShowcase` /
> `GraphwarButton` / `GraphwarFunctionInput` / `GraphwarLanVBox` / `GraphwarLongPressAnimation` /
> `GraphwarPlane` / `GraphwarRootNode` / `GraphwarSetupControl` —— **11 个**。
> 第 12 个 `GraphwarAdSwitch` 在符号表里只剩 `ImplementsGodotApi` / `ImplementsGodotVirtual`，
> `GodotClass::class_name` 没被编出来（没用到就被消掉了）⇒ §16.2 的 12 个**没有被推翻**，
> 只是符号表这一侧只能看到 11 个（`观测` + 已知的消解规则）。


### 18.4 B. 与我们现有认知对照


> 口径：「我们记录」= `docs/02` §17 的两张表 ∪ `docs/coverage-matrix.tsv` 的模块列
> （后者多一个 `weak_caller`，见下面「分类要改的」）。


#### 与现有认知对照

* 符号表（`graphwarrust::` 直系函数）：**76** 个模块
* 只靠泛型实参/impl 露面的模块：**2** 个（`graphwar_game_info`, `time_probe`）
* `docs/02` §17 记录：**70** 个；`docs/coverage-matrix.tsv`：**71** 个；其中标 `未知` 的 **22** 个

##### 对得上的（70 个，名字逐一相同）

`axis_drawer`, `button_manager`, `effect_drawer`, `entity_drawer`, `function_button_manager`, `function_drawer`, `function_history_recorder`, `graphwar_ad_switch`, `graphwar_audio_control`, `graphwar_backend`, `graphwar_button`, `graphwar_campaign_control`, `graphwar_client_connection_handler`, `graphwar_color_manager`, `graphwar_computer_player_manager`, `graphwar_draw_util`, `graphwar_effect_manager`, `graphwar_entity_effect_manager`, `graphwar_event`, `graphwar_event_handler`, `graphwar_function_evolutionator`, `graphwar_function_input`, `graphwar_function_manager`, `graphwar_game_client`, `graphwar_game_recorder`, `graphwar_game_server`, `graphwar_game_state_manager`, `graphwar_graphics_manager`, `graphwar_lan_broadcaster_v6`, `graphwar_lan_vbox`, `graphwar_level_generator`, `graphwar_level_manager`, `graphwar_life_manager`, `graphwar_lobby_control`, `graphwar_local_player_manager`, `graphwar_logger`, `graphwar_long_press_animation`, `graphwar_mode_manager`, `graphwar_module_container`, `graphwar_name_manager`, `graphwar_nodes`, `graphwar_obstacle_manager`, `graphwar_path_calculator`, `graphwar_plane`, `graphwar_plane_pos_manager`, `graphwar_play_screen_control`, `graphwar_player_manager`, `graphwar_player_screen_control`, `graphwar_rank_manager`, `graphwar_remote_event_handler`, `graphwar_replay_control`, `graphwar_replay_player`, `graphwar_root_node`, `graphwar_server_broker_handler`, `graphwar_server_connection_handler`, `graphwar_setup_control`, `graphwar_shot_calculator`, `graphwar_soldier_manager`, `graphwar_target_manager`, `graphwar_team_manager`, `graphwar_texture_loader`, `graphwar_tick_handler`, `graphwar_turn_manager`, `graphwar_video_writer`, `math_function`, `obstacle_drawer`, `player_showcase`, `polish_notation_converter`, `server_broker_request_connection`, `weak_caller`

##### 我们记录了、2024 版符号表里没有（1 个）

`graphwar_crash_reporter`

##### 新发现的模块（6 个，我们完全没记录）

* `graphwar_player_config`（8 个函数）类型 `GraphwarPlayerConfig`；方法 `load`, `lock`, `new`, `save`, `set_face`, `set_hat`, `set_skin`, `unlock`
* `graphwar_event_queue`（2 个函数）类型 `GraphwarEventQueue`；方法 `add_event`, `send_buffer`
* `graphwar_replay_reader`（2 个函数）类型 `GraphwarReplayReader`；方法 `load_game`, `load_games`
* `graphwar_stream_buffer`（2 个函数）类型 `GraphwarStreamBuffer`；方法 `read_event`, `write_event`
* `backtracer`（1 个函数）类型 —；方法 `handle_interrupt`
* `function_token`（1 个函数）类型 `FunctionToken`；方法 `new`

##### 分类要改的

* `weak_caller` 在 `coverage-matrix.tsv` 里被记成 `scope=dep`（第三方依赖），但符号路径是 `graphwarrust::weak_caller::…` ⇒ 它**在 graphwarrust crate 里**，是本仓库自己的一个模块（37 个函数）。内容仍是 `WeakCaller` 这类通用工具，但它不是外部 crate。

##### 22 个「完全未知」模块，这次解出真名的

| 模块 | 符号表里的真身 | 函数数 | 方法（节选） |
|---|---|---|---|
| `button_manager` | `ButtonManager` | 2 | `find_buttons`, `new` |
| `function_button_manager` | `FunctionButtonManager` | 3 | `find_buttons`, `new` |
| `function_drawer` | `FunctionDrawer` | 7 | `clear_functions`, `draw`, `remove_function`, `set_z_index`, `update_function`, `update_function_fade`, `update_transform` |
| `function_history_recorder` | `FunctionHistoryRecorder` | 4 | `handle_function_fire_event`, `new`, `subscribe_to_events_event_handler` |
| `graphwar_button` | `GraphwarButton` | 5 | `button_down`, `pressed`, `process`, `ready` |
| `graphwar_computer_player_manager` | `GraphwarComputerPlayerManager` | 11 | `add_computer_player`, `computer_function_thread`, `handle_entity_removed_event`, `handle_tick`, `handle_turn_info_event`, `new`, `subscribe_to_events_event_handler` |
| `graphwar_crash_reporter` | **符号表里完全没有** | 0 | — |
| `graphwar_effect_manager` | `GraphwarEffectManager` | 3 | `deserialize`, `new`, `subscribe_to_events_event_handler`, `tick`, `visit_str` |
| `graphwar_entity_effect_manager` | `GraphwarEntityEffectManager` | 6 | `handle_entity_removed_event`, `new`, `send_all_info_events`, `subscribe_to_events_event_handler` |
| `graphwar_game_recorder` | `GraphwarGameRecorder` | 4 | `new`, `save_event`, `save_game`, `subscribe_to_events` |
| `graphwar_graphics_manager` | `GraphwarGraphicsManager` | 9 | `handle_entity_removed_event`, `new`, `send_all_info_events`, `set_face_graphics`, `set_hat_graphics`, `set_skin_graphics`, `subscribe_to_events_event_handler` |
| `graphwar_lan_vbox` | `GraphwarLanVBox` | 5 | `get_join_address`, `process`, `update_game_list` |
| `graphwar_long_press_animation` | `GraphwarLongPressAnimation` | 5 | `draw`, `process`, `ready`, `start`, `stop` |
| `graphwar_player_manager` | `GraphwarPlayerManager` | 6 | `handle_entity_removed_event`, `new`, `send_all_info_events`, `subscribe_to_events_event_handler` |
| `graphwar_player_screen_control` | `GraphwarPlayerScreenControl` | 10 | `face_button_pressed`, `hat_button_pressed`, `insert_button_in_vbox`, `load_faces`, `load_hats`, `load_skins`, `skin_button_pressed`, `unlock` … |
| `graphwar_soldier_manager` | `GraphwarSoldierManager` | 7 | `get_soldiers_from_owner`, `handle_entity_removed_event`, `new`, `send_all_info_events`, `subscribe_to_events_event_handler` |
| `graphwar_team_manager` | `GraphwarTeamManager` | 10 | `add_to_team`, `deserialize`, `get_most_empty_team`, `get_team`, `handle_entity_removed_event`, `new`, `remove_from_team`, `send_all_info_events` … |
| `graphwar_texture_loader` | `GraphwarTextureLoader` | 6 | `get_chat_texture`, `get_face_texture`, `get_hat_texture`, `get_skin_texture`, `get_time_mode_texture`, `new` |
| `graphwar_video_writer` | `GraphwarVideoWriter` | 3 | `add_frame`, `new`, `write_video` |
| `obstacle_drawer` | `ObstacleDrawer` | 1 | `update` |
| `player_showcase` | `PlayerShowcase` | 4 | `draw`, `on_text_change`, `process`, `ready` |
| `server_broker_request_connection` | `ServerBrokerRequestConnectionClient` | 1 | `process` |

**22 个未知模块里，21 个在符号表里有自己的函数。**

**怎么读这张表**：22 个「未知」里 21 个解出来了，但这**不等于**它们被复原了——
本节给的是**名字**，不是**行为**。按 `AGENTS.md` §0 的口径，
`docs/coverage-matrix.tsv` 里它们的 `status` 仍然应该是「未做」，
只是「未知」这一档（= 连名字都没有）现在可以说**已经清零**（`graphwar_crash_reporter` 除外）。



### 18.5 C. 方法名清单（原版源码级函数名）


> 这是本节最有用的部分：它把「`0x516510` 语义未知」换成
> 「`GraphwarTurnManager::next_turn`」这种能直接去找文档、写测试的名字。
> `†` = 只有 `{{closure}}` 符号、外层本体被内联掉的名字（`setup_level_0`…`_4` 就是这种，
> 外层进了 `setup_level` 的分派）。


#### C. 方法名（原版源码级函数名）

> `panic_cold_explicit` / `__register_methods` / `__virtual_call` / `__inner_init` /
> `class_name` / `__register_exports` / serde 的 `expecting` 是 rustc 的冷段 + gdext/serde
> 的样板，**不是**原版源码里的函数名，已剔除。`{{closure}}` 也算不到方法名上。

| 模块 | 方法名（按所属类型分组） |
|---|---|
| `axis_drawer` | `AxisDrawer`: `draw` |
| `backtracer` | `_`: `handle_interrupt` |
| `button_manager` | `ButtonManager`: `find_buttons`<br>`ButtonManager`: `new`† |
| `effect_drawer` | `EffectData`: `new`<br>`EffectDrawer`: `draw`, `effect_info_event`, `process`, `ready`, `step_effect` |
| `entity_drawer` | `EntityDrawer`: `draw`, `draw_offset_triangle`, `get_entity_face_sprite`, `get_entity_hat_sprite`, `get_entity_name_label`, `get_entity_skin_sprite`, `init`, `process`, `ready`, `update_name_label`, `update_sprite` |
| `function_button_manager` | `FunctionButtonManager`: `find_buttons`, `new` |
| `function_drawer` | `FunctionDrawer`: `clear_functions`, `draw`, `remove_function`, `set_z_index`, `update_function`, `update_function_fade`, `update_transform` |
| `function_history_recorder` | `FunctionHistoryRecorder`: `handle_function_fire_event`, `new`<br>`FunctionHistoryRecorder`: `subscribe_to_events_event_handler`† |
| `function_token` | `FunctionToken`: `new` |
| `graphwar_audio_control` | `EffectAudioData`: `new`<br>`GraphwarAudioControl`: `effect_info_event`, `entity_removed_event`, `game_state_event`, `new`, `process`, `set_muted`, `soldier_info_event`, `update_muted`<br>`GraphwarAudioControl`: `subscribe_to_events`† |
| `graphwar_backend` | `GraphwarBackend`: `button_pressed`, `get_next_clip_checkpoint`, `new`, `process`, `resubscribe_to_events`, `set_game_name`, `setup_level`, `start_client_to_local_server`, `start_client_to_remote_server`, `start_client_to_replay`, `start_lan_broadcaster`, `start_server`, `start_server_broker_handler`, `stop_client`, `stop_lan_broadcaster`, `stop_server`, `stop_server_broker_handler`, `subscribe_to_event`, `subscribe_to_event_with_id` |
| `graphwar_button` | `GraphwarButton`: `button_down`, `pressed`, `process`, `ready` |
| `graphwar_campaign_control` | `GraphwarCampaignControl`: `get_current_level`, `get_level_as_int`, `save_current_level`, `show_hide_campaign_levels` |
| `graphwar_client_connection_handler` | `GraphwarClientConnectionHandler`: `connect`, `disconnect`, `forward_event`, `get_connection_state`, `handle_connected_to_server`, `new`, `tick`<br>`GraphwarClientConnectionHandler`: `subscribe_to_events_event_handler`† |
| `graphwar_color_manager` | `GraphwarColor`: `serialize`<br>`GraphwarColorManager`: `add_team_color`, `handle_entity_removed_event`, `init_team_colors`, `new`, `send_all_info_events`<br>`GraphwarColorManager`: `subscribe_to_events_event_handler`† |
| `graphwar_computer_player_manager` | `GraphwarComputerPlayerManager`: `add_computer_player`, `computer_function_thread`, `handle_entity_removed_event`, `handle_tick`, `handle_turn_info_event`, `new`<br>`GraphwarComputerPlayerManager`: `subscribe_to_events_event_handler`† |
| `graphwar_draw_util` | `GraphwarDrawUtil`: `get_team_overview`, `set_plane_transform` |
| `graphwar_effect_manager` | `EffectType`: `deserialize`, `visit_str`<br>`GraphwarEffectManager`: `new`, `tick`<br>`GraphwarEffectManager`: `subscribe_to_events_event_handler`† |
| `graphwar_entity_effect_manager` | `GraphwarEntityEffectManager`: `handle_entity_removed_event`, `new`, `send_all_info_events`<br>`GraphwarEntityEffectManager`: `subscribe_to_events_event_handler`† |
| `graphwar_event` | `EffectInfoEvent`: `visit_str`<br>`FunctionInfoEvent`: `clone`<br>`GraphwarEvent`: `clone`, `deserialize`, `serialize`, `visit_str`<br>`VersionInfoEvent`: `clone` |
| `graphwar_event_handler` | `GraphwarEventHandler`: `get_event_discriminant`, `process_events`, `subscribe_to_event` |
| `graphwar_event_queue` | `GraphwarEventQueue`: `add_event`, `send_buffer` |
| `graphwar_function_evolutionator` | `GraphwarFunctionEvolutionator`: `score_function` |
| `graphwar_function_input` | `GraphwarFunctionInput`: `clear`, `delete_chars_at_caret`, `delete_token_after_caret`, `delete_token_at_caret`, `force_update_cb`, `insert_text_at_caret`, `move_caret_end`, `move_caret_home`, `move_caret_left`, `move_caret_right`, `process`, `ready`, `substr`, `update_sizes` |
| `graphwar_function_manager` | `GraphwarFunctionData`: `get_function`, `get_function_fade_on_tick`, `set_function`<br>`GraphwarFunctionManager`: `get_active_functions_on_tick`, `get_active_functions_on_tick_no_fade`, `get_function`, `get_function_text`, `set_function`, `set_function_active_tick`, `set_function_points` |
| `graphwar_game_client` | `GraphwarGameClient`: `connect`, `disconnect`, `fire_function`, `get_module`, `new`, `process`, `request_add_bot`, `request_lock_change`, `send_chat_message`, `tick`, `update_function`, `wait_for_players` |
| `graphwar_game_info` | `GameInfo`: `serialize`, `visit_str` |
| `graphwar_game_recorder` | `GraphwarGameRecorder`: `new`, `save_event`, `save_game`<br>`GraphwarGameRecorder`: `subscribe_to_events`† |
| `graphwar_game_server` | `GraphwarGameServer`: `get_server_port`, `new`, `start`, `stop`<br>`_`: `game_server_thread`<br>`GameServerData`: `subscribe_to_room_info_updates`† |
| `graphwar_game_state_manager` | `GraphwarGameState`: `deserialize`, `serialize`, `visit_str`<br>`GraphwarGameStateManager`: `add_soldier_to_player`, `add_target`, `execute_results`, `fire_functions`, `get_alive_teams`, `handle_chat_message`, `handle_connection_removed_event`, `handle_entity_removed_event`, `handle_function_points`, `handle_graphics_info`, `handle_new_connection_event`, `handle_tick`, `new`, `remove_all_players`, `remove_soldier_from_player`, `request_axis_mode_change`, `request_change_team`, `request_fire_function`, `request_function_mode_change`, `request_new_player`, `request_time_mode_change`, `request_turn_mode_change`, `request_update_function`, `set_all_alive`, `set_name`, `set_player_num_soldiers`, `set_state`, `set_turn_limit`, `setup_and_start_game`, `start_game`<br>`GraphwarGameStateManager`: `queue_end_turn_trigger`†<br>`GraphwarGameStateManager`: `subscribe_to_events_event_handler`† |
| `graphwar_graphics_manager` | `GraphwarGraphicsManager`: `handle_entity_removed_event`, `new`, `send_all_info_events`, `set_face_graphics`, `set_hat_graphics`, `set_skin_graphics`<br>`GraphwarGraphicsManager`: `subscribe_to_events_event_handler`† |
| `graphwar_lan_broadcaster_v6` | `GraphwarLanBroadcasterV6`: `get_game_info`, `new`, `process`, `set_room_info`, `start` |
| `graphwar_lan_vbox` | `GraphwarLanVBox`: `get_join_address`, `process`<br>`GraphwarLanVBox`: `update_game_list`† |
| `graphwar_level_generator` | `GraphwarLevelGenerator`: `generate_level`, `generate_level_custom`, `generate_terrain`, `place_entity` |
| `graphwar_level_manager` | `GraphwarLevelManager`: `get_level_data`, `new`, `setup_level`, `setup_level_10`, `setup_level_11`, `setup_level_12`, `setup_level_13`, `setup_level_14`, `setup_level_15`, `setup_level_16`, `setup_level_17`, `setup_level_18`, `setup_level_19`, `setup_level_20`, `setup_level_5`, `setup_level_6`, `setup_level_7`, `setup_level_8`, `setup_level_9`<br>`GraphwarLevelManager`: `setup_level_0`†<br>`GraphwarLevelManager`: `setup_level_1`†<br>`GraphwarLevelManager`: `setup_level_2`†<br>`GraphwarLevelManager`: `setup_level_3`†<br>`GraphwarLevelManager`: `setup_level_4`† |
| `graphwar_life_manager` | `GraphwarLifeManager`: `get_alive`, `get_all_entities`, `get_death_tick`, `handle_entity_removed_event`, `handle_tick_event`, `new`, `send_all_info_events`, `set_alive`, `set_queued_death`<br>`GraphwarLifeManager`: `subscribe_to_events_event_handler`† |
| `graphwar_lobby_control` | `GraphwarLobbyControl`: `show_global_button`, `show_local_button`<br>`GraphwarLobbyControl`: `new`† |
| `graphwar_local_player_manager` | `GraphwarLocalPlayerManager`: `handle_connected_to_server`, `handle_entity_removed_event`, `handle_player_info`, `new`<br>`GraphwarLocalPlayerManager`: `subscribe_to_events_event_handler`† |
| `graphwar_logger` | `GraphwarLogger`: `log` |
| `graphwar_long_press_animation` | `GraphwarLongPressAnimation`: `draw`, `process`, `ready`, `start`, `stop` |
| `graphwar_mode_manager` | `AxisMode`: `deserialize`, `visit_str`<br>`FunctionMode`: `deserialize`<br>`GraphwarModeManager`: `new`, `send_all_info_events`<br>`TimeMode`: `deserialize`, `serialize`, `visit_str`<br>`GraphwarModeManager`: `subscribe_to_events_event_handler`† |
| `graphwar_module_container` | `GraphwarModuleContainer`: `add_module`, `get_module`, `get_module_wk` |
| `graphwar_name_manager` | `GraphwarNameManager`: `handle_entity_removed_event`, `new`, `send_all_info_events`, `set_name`<br>`GraphwarNameManager`: `subscribe_to_events_event_handler`† |
| `graphwar_nodes` | `GraphwarNodes`: `get_node_by_name`, `get_node_by_name_rec`, `get_node_by_type_rec` |
| `graphwar_obstacle_manager` | `GraphwarObstacleData`: `draw_circle`<br>`GraphwarObstacleManager`: `add_obstacle`, `clear_obstacles`, `destroy_obstacle`, `handle_tick_event`, `is_obstacle`, `new`, `queue_destroy_obstacle` |
| `graphwar_path_calculator` | `GraphwarPathCalculator`: `calculate_paths` |
| `graphwar_plane` | `GraphwarPlane`: `clear_state`, `draw`, `init`, `obstacle_partial_update_event`, `on_resized`, `process`, `ready`, `update_entities` |
| `graphwar_plane_pos_manager` | `GraphwarPlanePosManager`: `get_all_positions`, `get_position`, `handle_entity_removed_event`, `new`, `send_all_info_events`, `set_position`<br>`GraphwarPlanePosManager`: `subscribe_to_events_event_handler`† |
| `graphwar_play_screen_control` | `GraphwarPlayScreenControl`: `add_player_victory_icon`, `add_tie_icon`, `append_debug_text`, `append_function_entry`, `clear_state`, `disable_replay_buttons`, `enable_replay_buttons`, `new`, `process`, `update_end_game_pop_up`, `update_names_button_texture`, `update_view_function_button_texture` |
| `graphwar_player_config` | `GraphwarPlayerConfig`: `load`, `lock`, `new`, `save`, `set_face`, `set_hat`, `set_skin`, `unlock` |
| `graphwar_player_manager` | `GraphwarPlayerManager`: `handle_entity_removed_event`, `new`, `send_all_info_events`<br>`GraphwarPlayerManager`: `subscribe_to_events_event_handler`† |
| `graphwar_player_screen_control` | `GraphwarPlayerScreenControl`: `face_button_pressed`, `hat_button_pressed`, `insert_button_in_vbox`, `skin_button_pressed`, `unlock`, `update_buttons_background`, `update_buttons_background_in_vbox`<br>`GraphwarPlayerScreenControl`: `load_faces`†<br>`GraphwarPlayerScreenControl`: `load_hats`†<br>`GraphwarPlayerScreenControl`: `load_skins`† |
| `graphwar_rank_manager` | `GraphwarRankManager`: `handle_entity_removed_event`, `new`, `send_all_info_events`, `set_rank`<br>`GraphwarRankManager`: `subscribe_to_events_event_handler`† |
| `graphwar_remote_event_handler` | `GraphwarRemoteEventHandler`: `handle_client_remote_event`, `handle_server_remote_event`, `is_leader` |
| `graphwar_replay_control` | `GraphwarReplayScreenControl`: `process`, `update`<br>`GraphwarReplayScreenControl`: `update_replay_list`† |
| `graphwar_replay_player` | `GraphwarReplayPlayer`: `go_to_first_checkpoint`, `go_to_next_checkpoint`, `go_to_previous_checkpoint`, `inject_events`, `is_paused`, `new`, `pause`, `process`, `toggle_pause` |
| `graphwar_replay_reader` | `GraphwarReplayReader`: `load_game`, `load_games` |
| `graphwar_root_node` | `GraphwarRootNode`: `check_save_level`, `get_media_storage_path`, `init`, `log`, `process`, `process_button_pressed`, `process_replay_button_pressed`, `purchase_confirmed`, `purchase_denied`, `purchase_pending`, `ready`, `show_pop_up`, `show_screen` |
| `graphwar_server_broker_handler` | `GraphwarServerBrokerHandler`: `get_game_info`, `new`, `process`, `process_events`, `stop` |
| `graphwar_server_connection_handler` | `GraphwarServerConnectionHandler`: `forward_connection_specific_event`, `forward_event`, `handle_connection_removed_event`, `new`, `tick`<br>`GraphwarServerConnectionHandler`: `subscribe_to_events_event_handler`† |
| `graphwar_setup_control` | `GraphwarSetupControl`: `clear_state`, `connect_entry_buttons`, `enable_play_chat_button`, `force_title_update`, `init`, `is_leader_local`, `on_chat_submitted`, `on_text_change`, `on_visibility_change`, `process`, `ready`, `update_name`, `update_rank`, `update_soldiers` |
| `graphwar_shot_calculator` | `GraphwarShotCalculator`: `calculate_corrected_function`, `calculate_function`, `calculate_function_offset_str`, `calculate_shot_results` |
| `graphwar_soldier_manager` | `GraphwarSoldierManager`: `get_soldiers_from_owner`, `handle_entity_removed_event`, `new`, `send_all_info_events`<br>`GraphwarSoldierManager`: `subscribe_to_events_event_handler`† |
| `graphwar_stream_buffer` | `GraphwarStreamBuffer`: `read_event`, `write_event` |
| `graphwar_target_manager` | `GraphwarTargetManager`: `handle_entity_removed_event`, `new`<br>`GraphwarTargetManager`: `subscribe_to_events_event_handler`† |
| `graphwar_team_manager` | `GraphwarTeam`: `deserialize`<br>`GraphwarTeamManager`: `add_to_team`, `get_most_empty_team`, `get_team`, `handle_entity_removed_event`, `new`, `remove_from_team`, `send_all_info_events`<br>`GraphwarTeamManager`: `subscribe_to_events_event_handler`† |
| `graphwar_texture_loader` | `GraphwarTextureLoader`: `get_chat_texture`, `get_face_texture`, `get_hat_texture`, `get_skin_texture`, `get_time_mode_texture`, `new` |
| `graphwar_tick_handler` | `GraphwarTickHandler`: `get_fractional_tick`, `get_time_to_tick`, `handle_connected_to_server_event`, `new`, `tick` |
| `graphwar_turn_manager` | `GraphwarTurnData`: `get_current_turn_players`, `get_current_turn_soldiers`<br>`GraphwarTurnManager`: `get_current_turn_players`, `get_current_turn_soldiers`, `get_end_of_turn_tick`, `get_turn_mode`, `handle_entity_removed_event`, `initialize_turns`, `is_on_turn`, `new`, `next_turn`, `send_all_info_events`, `set_end_of_turn_tick`, `set_turn`, `set_turn_mode`<br>`TurnMode`: `deserialize`<br>`GraphwarTurnManager`: `subscribe_to_events_event_handler`† |
| `graphwar_video_writer` | `GraphwarVideoWriter`: `add_frame`, `new`, `write_video` |
| `math_function` | `FunctionRandomizer`: `adjust_tokens`, `create_random_region`, `mutate_fine_tune`, `mutate_function`<br>`MathFunction`: `evaluate_recursive`, `evaluate_token`, `new`, `remove_offset`, `simplify_recursive`, `to_string_recursive` |
| `obstacle_drawer` | `ObstacleDrawer`: `update` |
| `player_showcase` | `PlayerShowcase`: `draw`, `on_text_change`, `process`, `ready` |
| `polish_notation_converter` | `PolishNotationConverter`: `reorder_recursive` |
| `server_broker_request_connection` | `ServerBrokerRequestConnectionClient`: `process` |
| `weak_caller` | `WeakCaller`: `call`, `call_0` |

<sub>† 只有 `{{closure}}` 符号、外层本体被内联掉的名字——名字本身仍是存在的证据。</sub>
### 18.6 「符号名 ↔ `src/文件.rs:行:列`」的桥（最硬的一条证据）

`panic::Location` 在 `.data.rel.ro` 里是 24 字节：`{file.ptr, file.len, line:u32, col:u32}`。
槽位 0 被 `R_X86_64_RELATIVE` 占着（addend 就是串地址），**槽位 +8 就是 len**——
按 len 取出来的字节必须全 ASCII 且以 `.rs` 结尾，这就是自校验。
本次在 2024 版上解出 **5,015 个 Location 结构体**，覆盖 **70 个 `src/*.rs`**（与 §16.2/§17 的 70 个吻合）。

把「函数体里 `lea` 到哪个 Location」与「`.symtab` 里的符号名」一交叉，就得到下面这种一行行可核对的桥：

| 符号（原版真名） | `panic::Location` 锚点 | 函数字节数 |
|---|---|---|
| `GraphwarPathCalculator::calculate_paths` | `graphwar_path_calculator.rs:45:108`, `49:38`, `59:108`, `60:113`, `65:45`, `66:48`, `86:37`, `86:70`, `105:71`（9 个） | 5,351 |
| `GraphwarShotCalculator::calculate_shot_results` | `graphwar_shot_calculator.rs:181:59` … `419:47`（12 个）+ `graphwar_obstacle_manager.rs:89:28` | 10,726 |
| `MathFunction::new` | `math_function.rs:75:10` | 3,177 |
| `MathFunction::to_string_recursive` | `math_function.rs:285:28` | 5,454 |
| `FunctionRandomizer::mutate_fine_tune` | `math_function.rs:452:44`, `454:44` | 2,322 |
| `GraphwarClientConnectionHandler::forward_event` | `graphwar_client_connection_handler.rs:255:67` | 425 |

> **这张表的最强之处是它和旧结论对得上。**
> `docs/03` 早就从 **2026 版**里读出「解析器 `0x4ed670` 的 panic 锚是 `math_function.rs:75`」，
> `docs/119` §4.6 读出「`0x4ec160` 的 panic 分支引 `math_function.rs:285:28`」。
> 现在 2024 版的符号表把这两个函数的**名字**给出来了：
> `MathFunction::new` 对应 `:75:10`，`MathFunction::to_string_recursive` 对应 `:285:28`。
> **两条完全独立的路径（2026 反汇编 vs 2024 符号表）落到同一对 `(文件, 行)` 上**
> ⇒ ① 2024 与 2026 的源码布局在这两个函数上**没变**（`观测`）；
> ⇒ ② 本节这套解析方法自身是对的（不是自洽，是交叉命中）。

**每个模块的源文件行跨度**（已并入 §18.3 的「行范围」列）用来做还原的**下限检查**：
把我们写的 `graphwar_root_node.rs` 和"原版至少 1,060 行、`GraphwarRootNode` 有 10 个方法"对一下，
就知道还差多少。最长的几个文件：`graphwar_level_manager.rs` ≥ **1,888** 行、
`graphwar_root_node.rs` ≥ **1,060**、`graphwar_game_state_manager.rs` ≥ **1,171**、
`graphwar_remote_event_handler.rs` ≥ **869**、`graphwar_setup_control.rs` ≥ **609**、
`graphwar_play_screen_control.rs` ≥ **625**。
⚠ 这只是**下界**：没有 panic 的代码不会留下 Location，所以真实行数只会更多（`推断`）。



### 18.7 本轮顺带确证 / 纠正的 10 件事

| # | 结论 | 依据 | 档位 | 影响 |
|---|---|---|---|---|
| 1 | `weak_caller` **不是第三方依赖**，是 `graphwarrust` crate 自己的模块（`graphwarrust::weak_caller::WeakCaller`，37 个函数、2 个方法 `call`/`call_0`、锚 `weak_caller.rs:11–17`） | 符号路径 | `观测` | `docs/coverage-matrix.tsv` 把它记成 `scope=dep`，应改成 `graphwar` 域的普通模块（内容仍是泛型工具，但归属变了） |
| 2 | `graphwar_crash_reporter` **在 2024 版里完全不存在**：0 个符号、`src/graphwar_crash_reporter.rs` 串 0 处 | 全 `FUNC` 名字搜索 + `strings` | `观测` | 它在 2026 版有 **8 处**同名串 ⇒ 该模块是**两年间新增的**（`推断`：无法排除"2024 有但被整模块内联掉"，但连一个符号都没有，可能性很低） |
| 3 | `graphwar_stream_buffer`（`read_event` / `write_event`）**2024 版有**，而 2026 版的 `src/*.rs` 串里没有它 | 符号表 + 两版 `strings` 差集 | `观测`/`推断` | 可能被删或并入 `graphwar_event_queue`（`推断`，缺 2026 侧证据） |
| 4 | `GameInfo` 在**独立的** `graphwar_game_info` 模块（`GameInfo` 有 `serialize` impl） | 符号路径 `graphwarrust::graphwar_game_info::GameInfo` | `观测` | §17.1 把 `GameInfo` 记在 `graphwar_event.rs` 名下，应改为 `graphwar_game_info.rs` |
| 5 | `FunctionRandomizer`（`adjust_tokens` / `mutate_function` / `mutate_fine_tune` / `create_random_region`）在 **`math_function` 模块**里，锚 `math_function.rs:452:44` | 符号路径 + Location | `观测` | `docs/AI`/§17.2 把"token 级随机化"记在 `graphwar_function_evolutionator` 下；实际那个模块只有 `score_function` 一个方法（1 个 direct 函数）。分工要重划 |
| 6 | `graphwar_level_manager` 有 `setup_level_0` … `setup_level_20`，**正好 21 个** | 符号名（`_0`…`_4` 只剩闭包，本体被内联） | `观测` | 与 `docs/10` 从数据表反推的 `SETUPS[21]` **互相印证**（两条独立证据） |
| 7 | `graphwar_event` 有 **72 个变体** + `GraphwarEvent` 本体 = 73 个类型 | 符号名 | `观测` | 与 `docs/114` 的「72 个具名变体」**逐个数量相同**（本轮只对了数量，没逐个对名字——那是一件值得单独做的事） |
| 8 | `graphwar_replay_player` 有 `go_to_first_checkpoint` / `go_to_next_checkpoint` / `go_to_previous_checkpoint` / `inject_events` / `is_paused` / `pause` / `toggle_pause` / `process` / `new` = **9 个方法** | 符号名 | `观测` | §17.4 说「播放时钟是我们为了让客户端能播而设计的，原版没有对应函数」—— **这句在 2024 版上被推翻了**：`GraphwarReplayPlayer` 本身就是个带动画时钟的对象，有 `pause` / `toggle_pause` / `is_paused` 和三个 checkpoint 跳转。我方 `ReplayPlayer` 应改成按这 9 个名字组织（2.0.60 是否同名属 `推断`，但同一文件里同名方法大概率没改） |
| 9 | `graphwar_shot_calculator` 的 4 个方法是 `calculate_function` / `calculate_corrected_function` / `calculate_function_offset_str` / `calculate_shot_results` | 符号名 | `观测` | "修正"（`docs/05` 的 `c = y0 − f(±x0)`）是**独立方法** `calculate_corrected_function`；"`_offset_str`"说明还有一条生成函数文本的路径 |
| 10 | 本仓库最长的原版源文件是 `graphwar_level_manager.rs`（≥1,888 行），不是 `graphwar_root_node.rs` | Location 行跨度 | `观测`（下界） | 排工时的优先级应该按行数×函数数，不是按 `docs/149` 的 anchors 排序 |

**还有 3 条"没说成"的**（防止把本节读得过硬）：

* `graphwar_module_container` 有 **66 个** direct 函数、但只有 3 个方法名（`add_module`/`get_module`/`get_module_wk`）——
  这是 gdext 的泛型单态化：每注册一个模块就编一份。**它不说明原版有 66 个函数**（`观测` + `推断`）。
* `graphwar_nodes` 31 / `graphwar_plane_pos_manager` 9 / `graphwar_local_player_manager` 10 —— 同理，
  泛型实例的个数 ≠ 源码里的函数个数。
* `graphwar_client_connection_handler` 有 11 个 direct 函数但 7 个方法名，差的那 4 个是
  `panic_cold_explicit` ×1 与 `subscribe_to_events_event_handler::{{closure}}` ×3。



### 18.8 这条路**做不到**什么（写清楚，免得下一轮重复试）

1. **拿不到函数体。** 符号表只有名字 + 地址 + 大小。`0x516510` 干什么，还得靠反汇编。
2. **拿不到 2.0.60 的地址。** 本节的每个 `0x…` 都是 2024 版的；两版 `.text` 差 40 万字节，
   连函数顺序都不保证一致。**唯一能用的对照物是名字与 `src/file.rs:LINE`。**
3. **拿不到参数表/返回类型。** legacy mangling 不编码签名（这也是为什么它比 v0 好读但信息更少）。
   想知道 `forward_event(?)` 收什么，还得看反汇编或 `docs/07` 的报文。
4. **拿不到 `impl Trait` / 泛型参数的约束。**
5. **不能证明 2026 版还是这些名字。** 2026 版 stripped，名字无法直接验证；
   间接证据只有「`src/*.rs` 文件名两年只差 1 增 1 删」这一条，属于 `推断`。
6. **不能把 `graphwar_event` 的 72 个变体名直接对上判别值。** 名字有了，但"哪个名字 = 哪个
   `u32` 判别值"仍需 `docs/114` 的逐条解 —— 本次**没有**做这个配对（`docs/114` 的判别值表不受影响，
   但现在可以拿去和这 72 个名字对齐，是一件明确可做的下一步）。



### 18.9 复跑（一条命令复现本节全部表格）

```bash
# 物证：只有 2024-07-30 那一版有 .symtab，另外三版 stripped
SO=/home/user/MyProject/gw2/.reference/steam/2024-07-30/libgraphwarrust.so
sha256sum "$SO"      # 必须是 6eee7bb855d7fc46f6c9e9bb90bb233100b157e10b27c06c8fb9ba07e510db29
readelf -SW "$SO" | grep symtab        # [28] .symtab SYMTAB … 08bd88

# 本节 §18.2 / §18.3 / §18.4 / §18.5 的四张表（原样贴进来的，未改一个字节）
python3 tools/symtab_dump.py --so "$SO" summary
python3 tools/symtab_dump.py --so "$SO" modules
python3 tools/symtab_dump.py --so "$SO" compare
python3 tools/symtab_dump.py --so "$SO" methods

# §18.7 第 2/3 条（两版 src/*.rs 串的差集）
python3 tools/symtab_dump.py --so "$SO" srcdiff

# 单个模块的方法名 / 锚点（§18.6 那几张表就是这两条的子集）
python3 tools/symtab_dump.py --so "$SO" methods   graphwar_shot_calculator
python3 tools/symtab_dump.py --so "$SO" locations graphwar_shot_calculator
```

`tools/symtab_dump.py` 只依赖 `readelf` / `objdump` / `c++filt`（binutils）与 Python 标准库，
**不依赖 capstone、不依赖 Ghidra、不读上游的 `.pkl`**——所以任何人换一台机器都能重跑。

**本节的两条已知偏差**：
① `c++filt --format rust` 的展开形态随 binutils 版本略有差别（约束语法、`{{closure}}` 的写法），
表里出现 `{{closure}}` 的地方就是这样来的；名字主体不受影响。
② `Location` 结构体的「32 位 line/col」是 `.data.rel.ro` 里的裸数据，本工具只做了
"长度自校验 + 全 ASCII + 以 `.rs` 结尾"三道检查，**没有**逐个核对 ptr 是否真的是某条 `mov len`
配对的起点；所以 `:行:列` 里极少数可能偏，出现"列号大得离谱"（>300）的行时应当复核。

## 19. 版本差分：08-27 → 08-28 那一版改了什么（2026-11）

> **本章所有地址都是 08-28（现网当前版）的 ELF 虚地址**；凡出现 08-27 的地址都会显式标注。
> Ghidra/REA 地址 = 虚地址 + `0x100000`（见 §12.2）。
> 复现工具：`tools/version_diff.py`（整包对比）、`tools/measure.py`（函数口径）、
> `tools/func_refs.py`（一个函数碰了哪些东西）、`tools/sym_refs.py`（谁引用了某个导入符号）、
> `tools/func_diff.py`（单个函数的归一化指令级 diff）。五个都能独立跑，不需要 Ghidra。

### 19.1 一句话结论

**这一版没有"改了 272 字节代码"这回事。** 两版之间 `.text` 的**大小**只差 272 字节，但
**函数布局被整体重排**：同一段代码在两版里位于完全不同的偏移，`.rodata` 的合并串表也被重新
排序。所以"字节差异"在这两版之间基本没有信息量。

真正**离散、无噪声**的差异只有五项（都不受重排影响），全部列在 §19.4。它们的指向是一致的：
**这版多出一条"问一句是不是终端（`isatty`）、刷 stdio 然后 `_exit` 掉"的路径，
并让 clap 错误格式化里的一个新 panic 点变得可达。**

### 19.2 先回答"793 KB 去哪了"：**那不是 section，是死数据**

`08-28` 比 `08-27` 小了 793,016 字节，但**没有任何 section 被删**。逐节给数：

| section | 08-27 | 08-28 | Δ |
|---|---:|---:|---:|
| `.dynsym` | 3,216 | 3,264 | **+48** |
| `.gnu.version` | 268 | 272 | +4 |
| `.hash` | 1,080 | 1,096 | +16 |
| `.dynstr` | 1,689 | 1,702 | **+13** |
| `.rela.dyn` | 588,456 | 590,736 | **+2,280** |
| `.gcc_except_table` | 1,084,616 | 1,084,068 | −548 |
| `.rodata` | 1,124,156 | 1,129,484 | **+5,328** |
| `.eh_frame_hdr` | 107,372 | 107,452 | +80 |
| `.eh_frame` | 725,460 | 723,272 | −2,188 |
| `.text` | 6,107,444 | 6,107,716 | **+272** |
| `.tbss` | 408 | 432 | +24 |
| `.data.rel.ro` | 433,456 | 434,528 | **+1,072** |
| `.got` | 21,832 | 22,048 | +216 |
| `.relro_padding` | 1,440 | 712 | −728（纯对齐填充） |
| `.bss` | 3,248 | 3,264 | +16 |
| 其余 13 节 | — | — | 0 |
| **section 大小之和** | 10,219,824 | 10,225,729 | **+5,905** |

把文件尾也算进来就清楚了（**逐位**，`readelf -SW` 即可复算）：

```
                        e_shoff+28*64    文件长度      无 section 覆盖的尾部
2026-08-27               10,218,976     11,020,912       801,936 字节
2026-08-28               10,227,896     10,227,896             0 字节
```

- 08-28 的文件**正好在 section 表末尾结束**；
- 08-27 在 section 表之后还有 **801,936 字节**，**没有任何 section 覆盖它**，
  也**不在任何 PT_LOAD 里**（最后一个 LOAD 到文件偏移 `0x9be580` 就结束了）；
- 净变化 = −801,936（尾部消失） + 8,920（section 表末尾后移 10,218,976 → 10,227,896）= **−793,016** ✓
  （其中 section 大小之和只 +5,905，剩下 +3,015 是节间对齐填充。）

#### 那 801,936 字节是什么：**是 2024-07-30 那版的尾巴，逐字节相同**

不是"08-27 自己的符号表"。`cmp` 一下就知道：**08-27 的最后 801,936 字节与 2024-07-30 的最后
801,936 字节完全相同**（两个文件长度都是 11,020,912）。这段尾巴由三块组成，都能在 2024-07-30
自己的 section 表里指到：

| 组成 | 2024-07-30 里的位置 | 长度 |
|---|---|---:|
| `.strtab` 的后半段（从文件偏移 `0x9bede0` 起） | `.strtab` @ `0x8f6678`，长 `0x18bb22` | 799,674 |
| `.shstrtab`（内容就是 `.symtab\0.strtab\0.shstrtab\0…`） | @ `0xa8219a`，长 `0x114` | 276 |
| 2024 的 **31 项** section 表 | @ `0xa822b0`，31×64 | 1,984 |
| （两段之间的 2 字节对齐） | | 2 |
| **合计** | | **801,936** |

**自校验**：这段尾巴的**第一个字节是半截字符串**（`5b902aa510eebfE\0anon.eda857…llvm.…`）——
正因为它从 2024 的 `.strtab` 中间（第 820,072 字节处）被截断，所以开头的 mangled 名不完整。
如果它是 08-27 自己的符号表，串表一定会以 `\0` 开头。

**这条结论直接接上 §18**：§18 用的是 2024-07-30 的 `.symtab`/`.strtab`，而 2024 的 `.strtab` 有 **799,674 字节**以**死数据**的形式也躺在 08-19 / 08-27 的尾部，**逐字节相同**。换言之：2026 那三个版里"多出来的那段字节"**不是 2026 自己的符号表**，是 2024 那张名字表的下半截。⇒ 想从 08-27 的尾巴里捞 2026 的函数名是捞不到的；能用仍然只有 §18 那张 2024 的表。

**08-19 也有同样的尾巴**（856,928 字节，结尾同样是 2024 那 1,984 字节的 section 表）。
⇒ 结论（**观测**）：这 793 KB 是**无任何 section/segment 引用的死字节**，删掉它不影响加载；
它的内容是 2024-07-30 那版的残留。**（推断）**最可能是新版本被写在一个 2024 尺寸的缓冲区里、
没有截断；**（未知）**具体是构建工具还是 depot 差分造成的，本仓库无法定论。
但两版各自的 `sha256` 与 SteamDB manifest 记录一致（`39709a20…` / `9a538115…`），
所以**这确实是 Steam 实际发出来的字节**，不是本地复现的产物。

> 顺带纠正一个长期口径：**2026 系列的 `.so` 都没有 `.symtab`/`.strtab` section**
> （section 数 28，没有符号表）。08-19/08-27 那 800 多 KB "多余字节"是**死数据**，
> 不是"还没被 strip 掉的符号表"——它不提供任何可用的函数名。

### 19.3 `.text` 只长 272 字节，但**它不是"改了 272 字节"**

这一节是本章最该记住的一件事。

1. **函数数**（FDE 表，两版同源口径，**逐位**）：08-27 **13,420** → 08-28 **13,430**，**净 +10**。
   （Ghidra 侧的 13,503 含无 `.eh_frame` 的汇编桩，口径不同，见 §12。）
2. **字节层面几乎没有可比性**：08-28 的 `.text` 里只有 **39.9%** 的 64 字节窗口能在 08-27 里找到，
   但两版最长的公共连续片段有 **36,280 字节**，且这些片段的"对角偏移"是**分段的常数**
   （`−21,392` / `−21,440` / `+272` / `+154,864`）。⇒ 代码被**整块搬家**，不是被改写。
3. **把地址抹掉再比就一致了**：把每条指令的**立即数与位移字段清零**（`call rel32`、`[rip±disp]`、
   `[rsp+disp]` 全部抹成 0），08-28 有 **12,969 / 13,430** 个函数的指令流与 08-27 某函数**完全一致**。
   也就是说 96.6% 的函数"一个字都没改，只是换了地址"。
4. **剩下的"不一致"也不能直接当改动数**——做了对照实验：

| 对比 | 函数数变化 | U_B（new 独有形态） | U_A（old 独有形态） |
|---|---:|---:|---:|
| 08-28 vs 08-28（自比） | 0 | **0** | **0** |
| 08-19 → 08-27（普通版本推进） | +77 | **450** | **327** |
| **08-27 → 08-28（本版）** | **+10** | **461** | **522** |

   `U_B` 口径：08-28 的函数里，**归一化指令形态在 08-27 中完全找不到**的个数（集合成员，不重复计数）。
   自比给 0 ⇒ 这个口径**没有假阳性**；但 08-19→08-27 这种"正常推进"也有 450，
   **本版是 461**。⇒ **"461 个函数被改了"是错的读法**，这个口径的噪声底就是 ~450。
   能说的只有：**本版的结构变化量与一次普通版本推进同量级，没有出现"整模块新增"那种级别的差异。**

⇒ **（观测）** 这两版之间不存在"272 字节的定点补丁"。`+272` 只是**净尺寸**，
它由几百处函数增删/内联变化相互抵消后剩下的数；逐字节 diff 在这种构建上得不到结论。

### 19.4 唯一离散、无噪声的证据（这五项才是结论）

**(a) 动态符号：新增 `isatty`、`_exit`，没有删任何导入**（**逐位**，`.dynsym` 集合差）

| 版本 | `.dynsym` 条数 | `isatty` | `_exit` | `fflush` |
|---|---:|---|---|---|
| 2024-07-30 | 153 | — | — | — |
| 2026-08-19 | 134 | — | — | ✓ |
| 2026-08-27 | 134 | — | — | ✓ |
| **2026-08-28** | **136** | **✓** | **✓** | ✓ |

`fflush` 从 08-19 就有；**`isatty` 和 `_exit` 是 2026 系列里第一次出现**。
没有新增 PLT 项（`.plt` 896→896、`.rela.plt` 1320→1320，**逐位**）⇒ 这两个符号走 **GOT 间接**
（`R_X86_64_GLOB_DAT`），槽位 `0x9bef40`(`isatty`) / `0x9baae0`(`_exit`)。

**(b) 精确边界的字面量：新增 2 条，消失 0 条**（**逐位**；`(ptr,len)` 口径见 `tools/version_diff.py` §5）

```
+ '  '                                                                    ← 两个空格
+ '/home/lucas/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/
   clap_builder-4.5.11/src/error/format.rs'
-（无）
```

那条 clap 字面量的 `(ptr,len)` 槽在 `.data.rel.ro` 的 `0x9b05f0`（addend `0x1b694c`，len 104），
**引用它的是 08-28 的 `0x870110`**（9,604 字节）——而这个函数在 08-27 里没有任何形态相同的对应物
（就在 `U_B` 的最大函数榜上）。⇒ **（观测）** clap 错误格式化这一块**是新变得可达的代码**。

去重后的可打印字面量：08-27 **2,258** 条 → 08-28 **2,260** 条。
⚠️ **不要把第二条当成"新引入了 clap"**：`clap_builder 4.5.11` 与 `clap_lex 0.7.2` 在
**08-19 和 08-27 里就已经有 16 条 `panic::Location`**（`parser/parser.rs`、`output/usage.rs`、
`mkeymap.rs` …）。本版只**多了一条** `error/format.rs`——即 clap 错误格式化里的**某个 panic 点
新变得可达**，不是新依赖。

⇒ 这也解释了 `+5,328` 字节 `.rodata` 里为什么**只**捞出 2 条新字面量：
`.rodata` 的合并串表被**重新排序**了（同一个 `IOS_album_holder` 在 08-27 是 section 内 `+0x100`、
在 08-28 是 `+0x40`）。**逐字节 diff `.rodata` 只会得到边界噪声**（实测 77 KB"新字节"里没有一条
是真正的新字符串）。`.rodata` 尺寸 +5,328 与内容之间的关系：**（未知）**，可指认的内容增量只有 2 条字面量。

**(c) 新增的函数**

| 08-28 地址 | 大小 | 做什么（**观测**） |
|---|---:|---|
| `0x3b5290` | 255 B | 调 4 个内部函数后 `call [0x9baad8]`=`fflush`、`call [0x9baae0]`=`_exit` |
| `0x8624d0` | 19 B | 调 `isatty`（`call [0x9bef40]`） |
| `0x8624f0` | 19 B | 同上 |
| `0x862690` | 149 B | 同上 |
| `0x862730` | 149 B | 同上 |
| `0x870110` | 9,604 B | 引用 clap `error/format.rs` 的 `panic::Location`（见 (b)），clap 错误格式化 |

这 5 个函数在 08-27 里**都没有任何形态相同的对应物**（`tools/measure.py` 的"形态独有"= True），
所以它们属于**真正新写/新可达的代码**，不是搬家。

**(d) 模块归属：没有集中在新模块**

`U_B` 的 461 个函数里，只有 73 个能按 `fixtures/module_funcs.tsv` 指到模块名，
分散在 **34 个** `graphwar_*.rs` 模块上（最多的 `graphwar_game_client.rs` 8 个）。
⇒ **（观测）** 本版**不存在"新增了一整个模块"**这种量级的改动。

**(e) 没有可指认的删除**

没有 section 消失、没有导入符号消失、没有字面量消失；`.gcc_except_table` −548 与 `.eh_frame`
−2,188 是展开元数据（函数数反而 +10）。⇒ **（观测）** 本版没删功能。
**（未知）** 个别函数被删无法排除——形态口径有 ~450 的噪声底。

### 19.5 最可能的解释，以及它的证据缺口

**（推断）** 08-28 加了一条**命令行/诊断路径**：clap 的错误格式化 → 判断输出是不是终端
（`isatty`，4 个调用点，正好是 stdin/stdout/stderr 类"这个 fd 是不是 tty"的形态）→ 打印后
`fflush` + `_exit` 直接结束进程。`'  '`（两个空格）也符合 clap 帮助文本的列对齐填充。

**缺什么证据**（所以只能标推断）：

1. `.so` 只导出 `gdext_rust_init`（另一个 `__cxa_thread_atexit_impl` 是 glibc 的），**没有 `main`**。
   在 cdylib 里 `std::env::args()` 确实可用（Linux 上 Rust std 是惰性读 `/proc/self/cmdline`），
   但**这条路径从哪个入口被调到，未知**。
2. 没有运行时证据（本章全部是静态结论；本轮没跑 Frida、没真机触发）。
3. `0x3b5290` 调用的 4 个内部函数（`0x528070`、`0x906700`、`0x90fb50`、`0x9254ab`）**未定性**。

### 19.6 我们 `src/` 的缺口（这是本章对复原进度最直接的一条）

按上面 (a)(b)(c) 的三项，到 `src/` 里查：**三样都没有**。

```bash
grep -rn 'clap' src/ Cargo.toml                 # 无
grep -rnE 'isatty|IsTerminal|env::args' src/     # 无
# `_exit` 只命中 src/graphwar_root_node.rs 里的 "confirm_exit" 之类字样，不是 libc::_exit
```

⇒ **（观测）** 我们的复原实现里**没有 argv/clap 解析、没有终端判定、没有 `fflush`+`_exit` 退出路径**。
是否算"漏了"取决于 §19.5 那条路径到底可不可达：**（未知）**。如果它只是开发者留在里面的调试入口，
不进复原范围也说得过去；如果是玩家可触发的（例如启动参数），那 `src/` 里缺一整条支路。
**在拿到可达性证据之前，不要把它写进任何模块的"已复原"。**

### 19.7 复现（照抄即可，不需要 Ghidra）

```bash
# 0) 前提：四个版本的 .so 放在 ../.reference/steam/<日期>/libgraphwarrust.so（gitignore，不入库）
#    跑工具要用带 capstone 的 venv
V27=../.reference/steam/2026-08-27/libgraphwarrust.so
V28=../.reference/steam/2026-08-28/libgraphwarrust.so

# 1) 节表 + 文件尾死数据 + 函数级对比 + dynsym + 字面量（一次跑完，约 3 分钟）
../.venv/bin/python tools/version_diff.py $V27 $V28

# 2) 函数口径（U_B/U_A、净增函数数、模块归属）
../.venv/bin/python tools/measure.py                      # 08-27 -> 08-28
GW2_OLD=2026-08-19 GW2_NEW=2026-08-27 ../.venv/bin/python tools/measure.py   # 对照实验

# 3) 谁引用了 isatty / _exit（给出 08-28 的函数地址）
../.venv/bin/python tools/sym_refs.py $V28 isatty _exit

# 4) 某个新函数到底碰了什么（间接调用 / RIP 引用 / 能还原成字面量的都还原出来）
../.venv/bin/python tools/func_refs.py $V28 func 0x3b5290

# 5) 单个函数的归一化指令级 diff（例：08-28 的修正器 0x3c5f70）
../.venv/bin/python tools/func_diff.py $V27 $V27 $V28 $V28 0x3c5f70
```

**已经核对过的一条负结果**（值得记）：08-28 的核心算法入口里，
**修正器 `0x3c5f70` 与 08-27 的 `0x4e5040`（这是 08-27 的地址）归一化指令序列完全相同**
（823 字节 / 195 条指令，`shape-ratio = 1.0000`，0 行差异）。96 个不同的原始字节**全部落在
位移/立即数字段内**——注意其中包含少量栈偏移重排（`[rsp+0x30]`→`[rsp+0x40]`、
`0x48`→`0x30`、`0x50`→`0x38`），所以准确的说法是：
⇒ **（观测）** 这条函数**没有增删或替换任何一条指令**，只有地址与栈帧排布变了；
弹道修正的**指令形态**没变。**（未知）** 栈帧排布的变化是否对应局部变量集合的微调，本轮无法判定。
采样器 `0x3c2530`（§18 用 2024 符号表定名为 `GraphwarPathCalculator::calculate_paths`——⚠ **§20 更正**：这个名字在 2.0.60 上锚到的是 **`0x467530`**，不是 `0x3c2530`；`0x3c2530` 零 Location 锚，名字仍是 `未知`（另：它**零直接调用**，只经函数指针槽 `0x9bac98` 被调，
见 `docs/151` §2.5）。本节"没有同尺寸对应物"说的是 `0x3c2530` 这个**名字未知**的函数，与 `0x467530` 无关）在 08-27 侧**没有
同等大小的对应函数**（最接近的是 08-27 的 `0x4e18e0`，1,717 字节，形态相似度 0.859），
⇒ **（未知）** 采样器这一版是否被改过——被重排/内联差异盖住了，本轮的静态口径分辨不出来。
**不要把这一条写成"采样器没变"**。

## 20. 名字 ↔ 2.0.60 地址：用 `panic::Location` 把原版函数名钉到 x86-64 的地址上（2026-11）

### 20.0 一句话

§18 给的是**名字**，而且明说「地址一律是 2024 版的，禁止挪用到 2.0.60」。
本节补上缺的那半：**把名字钉到 2.0.60 (Linux x86-64) 的地址上**——不是靠推断，是靠
`panic::Location` 这个编译期常量做双向唯一的 join。产出 `fixtures/name_to_addr.tsv`。

### 20.1 为什么这次不需要「跨版本推断」

§18 用的物证是 **2024-07-30** 那一版未 strip 的 `.so`，所以它对 2.0.60 只能算 `推断`。
本节换了一份物证：

| 项 | 值 | 档位 |
|---|---|---|
| 名字来源 | **Android `lib/arm64-v8a/libgraphwarrust.so`**（带完整 `.symtab`：18,793 个 `FUNC`，1,300 个含 `graphwar`） | `观测` |
| 它就是 2.0.60 | 同包 `.rea-work/apk/asset/assets/project.binary` 里 `application/config/version` = **`2.0.60`** | `观测` |
| 地址来源 | `.reference/steam/2026-08-28/libgraphwarrust.so`（x86-64，stripped），sha256 `9a538115…` | `观测` |
| 两者同源 | 两侧 `src/*.rs` 的 Location 三元组 **2,035 个，交集 2,035 个，差 0** | `观测` |

⇒ **名字与地址都出自 2.0.60 这一版**，§18 的「两年间没改名」那条 `推断` 在本节**不需要**。
（两边 `src/*.rs` 的 Location 三元组一个不多一个不少，本身就是「同一份源码」的硬证据；
差异全部落在依赖 crate 上：`ring` / `reqwest` / `hyper` / `regex-automata` / `memchr` /
`base64` / `aho-corasick` / `flate2` —— 那些是各架构各自单态化/内联出来的代码路径。）

### 20.2 方法：桥是 `(file, line, col)`

`core::panic::Location` 的布局（rustc 1.79 起未变）：

```
struct Location { file: &'static str, line: u32, col: u32 }   // 24 字节
```

编译器把它写进 `.data.rel.ro`：第 0 个 8 字节槽是 `R_*_RELATIVE` 重定位，addend 指向
`.rodata` 里的文件名字面量；槽位 +8 是 `len`；+16/+20 是 `line`/`col`。
**这四个字段全是编译期常量，与指令集无关**，于是：

```
Android arm64:  符号名  ↔  它引用的 (file,line,col) 集合
Linux x86-64 :  地址    ↔  它引用的 (file,line,col) 集合
                            ↓ 按 (file,line,col) join
                符号名  ↔  x86-64 地址
```

三个实现要点（都会静默影响结果，写下来免得后人重踩）：

1. **join 的键用三元组，不用 `(file,line)`**。同一行常有两个 `.expect()`：本二进制里
   **281** 个 `(file,line)` 带 >1 个 `col`。只用 `(file,line)` 会把本来唯一的锚**人为**变成歧义。
2. **两侧必须用同一套「什么算引用了这个 Location」的规则**。取地址（x86 `lea` / arm64
   `adrp+add`）和取字段（`mov rax,[loc]`、`ldr x0,[x8,#off]`）都算；实现见
   `LocIndex.lookup()`（命中 `loc`、`loc+8`、`loc+16` 三者都归到 `loc`）。
   规则不对称会直接制造假歧义。
3. **arm64 的寄存器跟踪必须在 `bl`/`b`/`ret` 处清空**。不清的话陈旧值会在后续基本块里
   被当成 Location 地址——第一版实测 arm64 只解出 2,783 个键、x86 侧 4,666 个，
   差的就是这一类（改了之后 arm64 侧 src Location 覆盖率是 2,040/2,040）。

### 20.3 产出与覆盖统计

分母口径：`graphwar*` 符号 demangle 后、去掉 gdext/rustc 样板（`__register_methods` /
`panic_cold_explicit` / `__init` …）与 `{{closure}}`、再按地址去重，得到
**758 个有源码级方法名的原版函数**，分布在 **80 个模块**。

| 结果 | 行数 | 占比 | 含义 |
|---|---|---|---|
| `唯一锚` | **327** | 43.1% | 名字 ↔ 地址**已落定**（311 条直接双向唯一 + 16 条消去法） |
| `歧义` | 171 | 22.6% | 有锚，但没有一个是两侧一对一 ⇒ **不选**，候选写进 `evidence` 列 |
| `未知` | 260 | 34.3% | 全函数体**不引用任何 `panic::Location`** ⇒ 这条路定不了名 |
| `次级证据` | **0** | — | **有意不产出**，理由见 §20.7 |

地址覆盖 **60 / 80 个模块**。落地最多的：`graphwar_level_manager` 40、
`graphwar_game_state_manager` 34、`graphwar_setup_control` 14、`graphwar_turn_manager` 13。

两侧各自的中间量（`tools/name_bridge.py stats` 会全部打印）：

| 项 | arm64 (Android 2.0.60) | x86-64 (Linux 2.0.60) |
|---|---|---|
| 函数数 | 12,609（`.symtab`，限 `.text`） | 13,263（Ghidra 起点表） |
| `Location` 结构体 | 5,223（src 2,040） | 5,300（src 2,042） |
| 带任意锚的函数 | 1,722 | 3,107 |
| 带 `src/*.rs` 锚的函数 | 532 | 527 |
| 被引用的键（三元组） | 2,783 | 4,666 |

> arm64 侧的键数少是**真的少**，不是漏：它在依赖 crate 上把大量 panic 路径内联掉了。
> 但 `src/*.rs` 这一半两侧都是全覆盖（arm64 2,040/2,040 个 src 结构体都被引用到，
> x86 2,042/2,042），所以**游戏代码的锚不缺**。

### 20.4 质量指标：歧义率就是这次方法的分母

| 口径 | 歧义率 | 说明 |
|---|---|---|
| 仅 `src/*.rs`，键 = `(file,line,col)` | **3.5%**（72 / 2,035） | 本方法的主指标 |
| 仅 `src/*.rs`，键退化到 `(file,line)` | **3.8%**（70 / 1,833） | 任务口径要求报的那个数：**「同一行被多个函数共享」只值 0.3 个百分点** |

**歧义是内生的、而且两侧对称**：把每个键的「arm64 侧有几个函数引用 / x86 侧有几个」
画成表，几乎全落在对角线上——`(1,1)` 1,963 个、`(2,2)` 32、`(4,4)` 15、`(3,3)` 6、
`(6,6)` 3、`(8,8)` 2、`(45,45)` 2。两侧独立算出来的重数**相等**，这既是「同一份源码」的
旁证，也说明歧义来自源码本身（同一行被 N 个函数内联），不是我这边漏检。

另外三条检查（都只是**校验**，不参与配对）：

* **与 §18 独立对账（最强的一条）**：§18 用 2024 版符号表记下
  `GraphwarPathCalculator::calculate_paths` 的 9 个锚，本节在 2.0.60 的 `0x467530` 上解出的
  **是同样那 9 个**（`45:108 / 49:38 / 59:108 / 60:113 / 65:45 / 66:48 / 86:37 / 86:70 / 105:71`），
  函数字节数 5,408（§18 记 2024 版 5,351）。两份物证、两版代码、独立算法，锚点集合逐个相同。
* **函数大小比** x86/arm64 中位数 **1.06**，98.6% 的配对落在 `[1/3, 3]` 内。
* **次序相关**（Spearman）0.70 —— 偏低，但这是**预期**的：§19 已证这一版函数被整体重排，
  且两个架构的内联策略本就不同。**它不构成「配对错了」的证据，也不被用来配对。**

### 20.5 具体结论（本节最该被引用的三行）

| 地址（ELF 虚地址） | 名字 | 证据 | 档位 |
|---|---|---|---|
| **`0x3c2bc0`** | **`GraphwarShotCalculator::calculate_shot_results`** | **13 个 `src/*.rs` 锚**（12 个 `graphwar_shot_calculator.rs` + 1 个 `graphwar_obstacle_manager.rs`），其中 **11 个双向唯一**；12,432 B（arm64 同名为 8,760 B） | **`观测`** |
| **`0x467530`** | **`GraphwarPathCalculator::calculate_paths`** | 模块自己的 9 个 Location 锚**全部**落在它体内（+0x1138 起的一段冷 panic 块），5,408 B（arm64 同名 4,812 B） | **`观测`** |
| **`0x4ed670`** | **`MathFunction::new`** | 锚 `src/math_function.rs:75:10`；与 §18 记的 2024 版同名同锚 | **`观测`** |

### 20.6 顺手回答的两个问题（本节的起因）

**问：采样器 `0x3c2530` 和开火结算 `0x3c2bc0` 到底有没有锚？**

| 函数 | 有锚吗 | 能否定名 |
|---|---|---|
| 开火结算 **`0x3c2bc0`** | **有，13 个**（11 个双向唯一） | **能** → `GraphwarShotCalculator::calculate_shot_results`（`观测`） |
| 采样器 **`0x3c2530`** | **没有，0 个**（1,680 B 全函数体零 Location 引用） | **不能** → 名字仍是 `未知`。可达性已结案（`docs/151` §2.5）：**零 `call rel32`**，唯一入口是函数指针槽 `0x9bac98`，使用者只有 `0x3c30c1`、`0x3e2d35` 两处 |

所以 `docs/00-status.md` 说「求值器 `0x4eb9d0`、采样器 `0x3c2530` 全函数体零 Location 引用」
**是对的**（`0x4eb9d0` 复核也是 0 个）——但 **`0x3c2bc0` 不是**，见 §20.7 第 1 条。

**附带纠正一个被传错的事实**：`calculate_paths` 不在 `0x3c2530`，在 **`0x467530`**。
`0x3c2530` 引用的浮点常量是 `0.001` / `6.0` / `15.0` / `25.0` / `200.0`
（= 容差、`|y|≤15`、`|x|≤25`、`200/step` 的点预算，与 `AGENTS.md` 的硬事实逐个对上），
而 `0x467530` 一个都不引用 ⇒ **两者是不同函数，采样核心没有被内联进 `calculate_paths`**。
`0x3c2530` 的模块归属因此**没有锚点证据**；它是不是 `calculate_paths` 里被 outlining
出来的一段（arm64 侧 `graphwar_path_calculator` 只有 `calculate_paths` 一个符号），
本节**判定不了**，标 `未知`。

### 20.7 与既有文档冲突的地方（逐条）

1. **`§17.1` 说「`0x3c2bc0` 本体没有 panic 锚」→ 证否**。
   它有 13 个 `src/*.rs` 锚、11 个双向唯一。判据：`tools/name_bridge.py probe 0x3c2bc0`
   （会把每个锚的两侧 owner 数与 arm64 符号名都打出来）。**以本节为准**，§17.1 那两行已就地更正。
2. **`§17.1` 说 `graphwar_path_calculator` 模块「只归属到 1 个辅助函数（0x469630/0x4696d0）」→ 更正**。
   该模块自己的 9 个锚全部落在 `0x467530`。`0x469630`/`0x4696d0` 只有 160 B，是 panic/drop 胶水，
   不是模块的本体函数。
3. **`§18` 的名字表把「采样器 `0x3c2530`」直接写成 `calculate_paths` → 更正（名字对，地址错）**。
   名字确实是 `calculate_paths`，但地址是 `0x467530`。
4. **`§19` 结尾「采样器 `0x3c2530` …定名为 `calculate_paths`」→ 同上，已就地加更正**。
5. **对本项目最要紧的一条**：`docs/04`（点序列采样）通篇以 `0x3c2530` 为题眼，
   而 `graphwar_path_calculator.rs` 这个名字对应的地址是 `0x467530`。
   ⇒ 关于「哪段代码在哪个文件里」的归属，`docs/04` 的**行为**结论（`hi = mid2`、24/24 逐位相等）
   **不受影响**，但它的**文件名归属**缺锚点支持。`src/graphwar_path_calculator.rs` 该装
   `0x467530` 还是 `0x3c2530`，需要另找证据（字符串引用 / 调用邻居），本轮**不猜**。

### 20.8 局限（别把这些当结论）

* **`歧义` 的 171 行没有地址**。其中 9 条是「一个名字在 x86 侧被拆成多个函数」
  （内联策略不同导致的拆分，例如 `GraphwarGameRecorder::save_game` → 3 个候选地址）。
  这类**只能人工按模块一致性挑一个**，`evidence` 列已给候选与票数，本节不选。
* **`未知` 的 260 行是硬边界**：没有 `panic`/`unwrap`/`expect` 的函数，这条路永远定不了名。
  `0x3c2530` 就是典型。
* 本节**不产出 `次级证据`**：§19 已证 08-27→08-28 函数被整体重排、`.text` 净差只有 +272 字节，
  大小/形态相似度在这个版本对上**分辨不出来**（采样器在 08-27 侧最接近的 `0x4e18e0`
  相似度只有 0.859）；字符串引用又只证明「被这片代码引用」、不证明归属（§16.4 第 3 条）。
  ⇒ 宁缺勿错，无锚一律 `未知`。
* x86 侧函数边界用的是 Ghidra 起点表（`analysis/funcstarts_ghidra_va.pkl`，13,263 条），
  **不是** `.eh_frame` 那份错的。若某个函数缺条目，它体内的引用会被记到前一个函数上；
  这使得个别配对可能整体错位——`次序相关 0.70` 里有多少是这种噪声，本轮没分离出来。
* 名字来自 **arm64** 符号表，`src_file:line` 是两侧共同的编译期常量；**arm64 地址一律没进任何表**。

### 20.9 怎么复算（不依赖本节，任何人可重跑）

```bash
# 一条命令重建核心交付物（约 80 s：capstone 全量扫两个 .so 的 .text）
../.venv/bin/python tools/name_bridge.py build        # 写 fixtures/name_to_addr.tsv

../.venv/bin/python tools/name_bridge.py stats        # 覆盖/歧义率/对账指标
../.venv/bin/python tools/name_bridge.py probe 0x3c2530 0x3c2bc0   # 某地址有没有锚
../.venv/bin/python tools/name_bridge.py module graphwar_shot_calculator
../.venv/bin/python tools/name_bridge.py conflicts    # 「一个名字被拆成多个函数」的 9 条
```

输入路径可用 `--arm64 / --x86 / --funcstarts` 或环境变量
`GW2_ARM64_SO` / `GW2_X86_SO` / `GW2_FUNCSTARTS` 覆盖，默认在这台机器上是：

* arm64：`.rea-work/apk/arm64/lib/arm64-v8a/libgraphwarrust.so`
* x86-64：`.reference/steam/2026-08-28/libgraphwarrust.so`（与仓库根 `libgraphwarrust.so` 同 sha256）
* 函数起点：`analysis/funcstarts_ghidra_va.pkl`（向上逐级查找，**不要**换成 `funcstarts.pkl`）

`fixtures/name_to_addr.tsv` 的列：
`module / type / method / x86_addr / src_file / src_line / join_kind / evidence`。
`x86_addr` 是 **ELF 虚地址**，Ghidra/REA 地址 = 该值 + `0x100000`（只加一次）；
`join_kind` ∈ `唯一锚 / 歧义 / 次级证据 / 未知`，其中 `唯一锚` 的 `evidence` 会写明
是「直接唯一」还是「消去法唯一」。
## 21. 327 个原版函数的真名索引（按模块）

> 由 `fixtures/name_to_addr.tsv` 汇总；`唯一锚` 行才有 x86 地址。
> 生成依据见 §20（Android 2.0.60 arm64 的 `.symtab` × `panic::Location` × Linux 2.0.60）。
> **地址口径**：下表是 ELF 虚地址，Ghidra/REA 地址 = 该值 + `0x100000`。

### effect_drawer（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw` | `EffectDrawer` | `0x4e1520` | `src/effect_drawer.rs:356` |
| `ready` | `EffectDrawer` | `0x4e25b0` | `src/effect_drawer.rs:479` |
| `ready` | `EffectDrawer` | `0x4e2a30` | `src/effect_drawer.rs:291` |
| `ready` | `EffectDrawer` | `0x4e2d50` | `src/effect_drawer.rs:456` |
| `process` | `EffectDrawer` | `0x4e2f80` | `src/effect_drawer.rs:250` |

### entity_drawer（8 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw` | `EntityDrawer` | `0x4b2890` | `src/entity_drawer.rs:244` |
| `ready` | `EntityDrawer` | `0x4b4130` | `src/entity_drawer.rs:532` |
| `process` | `EntityDrawer` | `0x4b4b60` | `src/entity_drawer.rs:60` |
| `update_sprite` | `EntityDrawer` | `0x4b9530` | `src/entity_drawer.rs:192` |
| `get_entity_hat_sprite` | `EntityDrawer` | `0x4b9e70` | `src/entity_drawer.rs:369` |
| `get_entity_name_label` | `EntityDrawer` | `0x4ba4a0` | `src/entity_drawer.rs:398` |
| `get_entity_face_sprite` | `EntityDrawer` | `0x4bb0a0` | `src/entity_drawer.rs:346` |
| `get_entity_skin_sprite` | `EntityDrawer` | `0x4bb6b0` | `src/entity_drawer.rs:323` |

### function_button_manager（2 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `new` | `FunctionButtonManager` | `0x3f0270` | `src/function_button_manager.rs:28` |
| `new` | `FunctionButtonManager` | `0x3f06b0` | `src/function_button_manager.rs:33` |

### function_drawer（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `update_function` | `FunctionDrawer` | `0x498c70` | `src/function_drawer.rs:45` |

### function_history_recorder（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `handle_function_fire_event` | `FunctionHistoryRecorder` | `0x3f0e40` | `src/function_history_recorder.rs:66` |

### graphwar_audio_control（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `game_state_event` | `GraphwarAudioControl` | `0x4bef20` | `src/graphwar_audio_control.rs:351` |
| `effect_info_event` | `GraphwarAudioControl` | `0x4bf090` | `src/graphwar_audio_control.rs:328` |
| `new` | `GraphwarAudioControl` | `0x4bf550` | `src/graphwar_audio_control.rs:245` |
| `process` | `GraphwarAudioControl` | `0x4bfe10` | `src/graphwar_audio_control.rs:170` |
| `set_muted` | `GraphwarAudioControl` | `0x4c11d0` | `src/graphwar_audio_control.rs:362` |

### graphwar_backend（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `button_pressed` | `GraphwarBackend` | `0x514a10` | `src/graphwar_game_client.rs:186` |
| `subscribe_to_event` | `GraphwarBackend` | `0x514b40` | `src/graphwar_backend.rs:344` |
| `resubscribe_to_events` | `GraphwarBackend` | `0x514d80` | `src/graphwar_backend.rs:350` |
| `start_client_to_replay` | `GraphwarBackend` | `0x514f20` | `src/graphwar_backend.rs:173` |
| `process` | `GraphwarBackend` | `0x515dc0` | `src/graphwar_backend.rs:79` |

### graphwar_campaign_control（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `show_hide_campaign_levels` | `GraphwarCampaignControl` | `0x418820` | `src/graphwar_campaign_control.rs:94` |

### graphwar_client_connection_handler（7 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `disconnect` | `GraphwarClientConnectionHandler` | `0x4592d0` | `src/graphwar_client_connection_handler.rs:150` |
| `forward_event` | `GraphwarClientConnectionHandler` | `0x459690` | `src/graphwar_client_connection_handler.rs:255` |
| `get_connection_state` | `GraphwarClientConnectionHandler` | `0x459840` | `src/graphwar_client_connection_handler.rs:161` |
| `handle_connected_to_server` | `GraphwarClientConnectionHandler` | `0x459950` | `src/graphwar_client_connection_handler.rs:249` |
| `new` | `GraphwarClientConnectionHandler` | `0x4599f0` | `src/graphwar_client_connection_handler.rs:65` |
| `tick` | `GraphwarClientConnectionHandler` | `0x459f20` | `src/graphwar_client_connection_handler.rs:190` |
| `connect` | `GraphwarClientConnectionHandler` | `0x45a340` | `src/graphwar_client_connection_handler.rs:124` |

### graphwar_color_manager（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `set_color` | `GraphwarColorManager` | `0x3e2830` | `src/graphwar_color_manager.rs:114` |
| `add_team_color` | `GraphwarColorManager` | `0x464d10` | `src/graphwar_color_manager.rs:92` |
| `send_all_info_events` | `GraphwarColorManager` | `0x465460` | `src/graphwar_color_manager.rs:124` |
| `handle_entity_removed_event` | `GraphwarColorManager` | `0x465570` | `src/graphwar_color_manager.rs:134` |
| `new` | `GraphwarColorManager` | `0x465620` | `src/graphwar_color_manager.rs:46` |

### graphwar_computer_player_manager（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `handle_tick` | `GraphwarComputerPlayerManager` | `0x4785f0` | `src/graphwar_computer_player_manager.rs:221` |
| `add_computer_player` | `GraphwarComputerPlayerManager` | `0x478fc0` | `src/graphwar_computer_player_manager.rs:195` |
| `handle_turn_info_event` | `GraphwarComputerPlayerManager` | `0x479520` | `src/graphwar_computer_player_manager.rs:276` |
| `handle_entity_removed_event` | `GraphwarComputerPlayerManager` | `0x47a520` | `src/graphwar_computer_player_manager.rs:331` |
| `new` | `GraphwarComputerPlayerManager` | `0x47a720` | `src/graphwar_computer_player_manager.rs:84` |

### graphwar_draw_util（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_team_overview` | `GraphwarDrawUtil` | `0x4be6e0` | `src/graphwar_draw_util.rs:28` |

### graphwar_effect_manager（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `tick` | `GraphwarEffectManager` | `0x519d80` | `src/graphwar_effect_manager.rs:103` |

### graphwar_entity_effect_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `subscribe_to_events_event_handler` | `GraphwarEntityEffectManager` | `0x4782f0` | `src/graphwar_entity_effect_manager.rs:67` |
| `subscribe_to_events_event_handler` | `GraphwarEntityEffectManager` | `0x4783f0` | `src/graphwar_entity_effect_manager.rs:77` |
| `new` | `GraphwarEntityEffectManager` | `0x478490` | `src/graphwar_entity_effect_manager.rs:35` |

### graphwar_event_handler（2 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `process_events` | `GraphwarEventHandler` | `0x3d3630` | `src/graphwar_event_handler.rs:467` |
| `subscribe_to_event` | `GraphwarEventHandler` | `0x3d38f0` | `src/graphwar_event_handler.rs:396` |

### graphwar_function_evolutionator（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `score_function` | `GraphwarFunctionEvolutionator` | `0x51a750` | `src/graphwar_function_evolutionator.rs:306` |
| `calculate_functions` | `GraphwarFunctionEvolutionator` | `0x51b440` | `src/graphwar_function_evolutionator.rs:106` |
| `calculate_functions_old` | `GraphwarFunctionEvolutionator` | `0x51d080` | `src/graphwar_function_evolutionator.rs:203` |
| `new` | `GraphwarFunctionEvolutionator` | `0x51eb90` | `src/graphwar_function_evolutionator.rs:67` |

### graphwar_function_input（7 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `force_update_cb` | `GraphwarFunctionInput` | `0x4f2200` | `src/graphwar_function_input.rs:327` |
| `move_caret_left` | `GraphwarFunctionInput` | `0x4f3210` | `src/graphwar_function_input.rs:253` |
| `move_caret_right` | `GraphwarFunctionInput` | `0x4f3910` | `src/graphwar_function_input.rs:145` |
| `delete_token_at_caret` | `GraphwarFunctionInput` | `0x4f44c0` | `src/graphwar_function_input.rs:97` |
| `delete_token_after_caret` | `GraphwarFunctionInput` | `0x4f4a60` | `src/graphwar_function_input.rs:198` |
| `ready` | `GraphwarFunctionInput` | `0x4fd1b0` | `src/graphwar_function_input.rs:508` |
| `process` | `GraphwarFunctionInput` | `0x4fd4a0` | `src/graphwar_function_input.rs:521` |

### graphwar_function_manager（7 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_function` | `GraphwarFunctionManager` | `0x455b60` | `src/graphwar_function_manager.rs:165` |
| `set_function` | `GraphwarFunctionManager` | `0x455e30` | `src/graphwar_function_manager.rs:152` |
| `get_function_text` | `GraphwarFunctionManager` | `0x4565f0` | `src/graphwar_function_manager.rs:169` |
| `set_function_points` | `GraphwarFunctionManager` | `0x456810` | `src/graphwar_function_manager.rs:192` |
| `set_function_active_tick` | `GraphwarFunctionManager` | `0x456b80` | `src/graphwar_function_manager.rs:181` |
| `get_active_functions_on_tick` | `GraphwarFunctionManager` | `0x456df0` | `src/graphwar_function_manager.rs:208` |
| `get_active_functions_on_tick_no_fade` | `GraphwarFunctionManager` | `0x456f80` | `src/graphwar_function_manager.rs:213` |

### graphwar_game_client（11 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `disconnect` | `GraphwarGameClient` | `0x516370` | `src/graphwar_game_client.rs:118` |
| `fire_function` | `GraphwarGameClient` | `0x516510` | `src/graphwar_game_client.rs:152` |
| `request_add_bot` | `GraphwarGameClient` | `0x516970` | `src/graphwar_game_client.rs:152` |
| `update_function` | `GraphwarGameClient` | `0x516ad0` | `src/graphwar_game_client.rs:152` |
| `wait_for_players` | `GraphwarGameClient` | `0x516fd0` | `src/graphwar_game_client.rs:101` |
| `send_chat_message` | `GraphwarGameClient` | `0x5172d0` | `src/graphwar_game_client.rs:152` |
| `request_lock_change` | `GraphwarGameClient` | `0x517490` | `src/graphwar_game_client.rs:152` |
| `new` | `GraphwarGameClient` | `0x517690` | `src/graphwar_game_client.rs:53` |
| `tick` | `GraphwarGameClient` | `0x518240` | `src/graphwar_game_client.rs:139` |
| `connect` | `GraphwarGameClient` | `0x518320` | `src/graphwar_game_client.rs:93` |
| `process` | `GraphwarGameClient` | `0x518400` | `src/graphwar_game_client.rs:130` |

### graphwar_game_recorder（2 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `subscribe_to_events` | `GraphwarGameRecorder` | `0x3d4fa0` | `src/graphwar_game_recorder.rs:107` |
| `save_game` | `GraphwarGameRecorder` | `0x3d5740` | `src/graphwar_game_recorder.rs:150` |

### graphwar_game_server（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `subscribe_to_room_info_updates` | `GameServerData` | `0x44e8f0` | `src/graphwar_game_server.rs:264` |
| `start` | `GraphwarGameServer` | `0x44f990` | `src/graphwar_game_server.rs:130` |
| `game_server_thread` | `_` | `0x4500c0` | `src/graphwar_game_server.rs:219` |

### graphwar_game_state_manager（34 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `report_fired_function` | `GraphwarGameStateManager` | `0x3e2830` | `src/graphwar_game_state_manager.rs:677` |
| `add_target` | `GraphwarGameStateManager` | `0x46f310` | `src/graphwar_game_state_manager.rs:593` |
| `start_game` | `GraphwarGameStateManager` | `0x46f690` | `src/graphwar_game_state_manager.rs:191` |
| `handle_tick` | `GraphwarGameStateManager` | `0x470840` | `src/graphwar_game_state_manager.rs:256` |
| `set_all_alive` | `GraphwarGameStateManager` | `0x470b70` | `src/graphwar_game_state_manager.rs:607` |
| `fire_functions` | `GraphwarGameStateManager` | `0x470eb0` | `src/graphwar_game_state_manager.rs:256` |
| `set_turn_limit` | `GraphwarGameStateManager` | `0x472400` | `src/graphwar_game_state_manager.rs:182` |
| `execute_results` | `GraphwarGameStateManager` | `0x472470` | `src/graphwar_game_state_manager.rs:256` |
| `get_alive_teams` | `GraphwarGameStateManager` | `0x473520` | `src/graphwar_game_state_manager.rs:867` |
| `remove_all_players` | `GraphwarGameStateManager` | `0x4737a0` | `src/graphwar_game_state_manager.rs:485` |
| `request_new_player` | `GraphwarGameStateManager` | `0x4739c0` | `src/graphwar_game_state_manager.rs:282` |
| `handle_chat_message` | `GraphwarGameStateManager` | `0x473d00` | `src/graphwar_game_state_manager.rs:1187` |
| `request_change_team` | `GraphwarGameStateManager` | `0x473e50` | `src/graphwar_game_state_manager.rs:306` |
| `handle_graphics_info` | `GraphwarGameStateManager` | `0x4741a0` | `src/graphwar_game_state_manager.rs:1147` |
| `setup_and_start_game` | `GraphwarGameStateManager` | `0x474490` | `src/graphwar_game_state_manager.rs:458` |
| `add_soldier_to_player` | `GraphwarGameStateManager` | `0x474a10` | `src/graphwar_game_state_manager.rs:503` |
| `request_fire_function` | `GraphwarGameStateManager` | `0x4754c0` | `src/graphwar_game_state_manager.rs:655` |
| `request_remove_player` | `GraphwarGameStateManager` | `0x475590` | `src/graphwar_game_state_manager.rs:334` |
| `handle_function_points` | `GraphwarGameStateManager` | `0x4758b0` | `src/graphwar_game_state_manager.rs:1129` |
| `queue_end_turn_trigger` | `GraphwarGameStateManager` | `0x4759f0` | `src/graphwar_game_state_manager.rs:191` |
| `request_update_function` | `GraphwarGameStateManager` | `0x475f50` | `src/graphwar_game_state_manager.rs:635` |
| `set_player_num_soldiers` | `GraphwarGameStateManager` | `0x476320` | `src/graphwar_game_state_manager.rs:370` |
| `request_axis_mode_change` | `GraphwarGameStateManager` | `0x4764b0` | `src/graphwar_game_state_manager.rs:393` |
| `request_time_mode_change` | `GraphwarGameStateManager` | `0x4765f0` | `src/graphwar_game_state_manager.rs:426` |
| `request_turn_mode_change` | `GraphwarGameStateManager` | `0x476730` | `src/graphwar_game_state_manager.rs:415` |
| `remove_soldier_from_player` | `GraphwarGameStateManager` | `0x476830` | `src/graphwar_game_state_manager.rs:544` |
| `handle_entity_removed_event` | `GraphwarGameStateManager` | `0x476a20` | `src/graphwar_game_state_manager.rs:1064` |
| `handle_new_connection_event` | `GraphwarGameStateManager` | `0x477140` | `src/graphwar_game_state_manager.rs:203` |
| `request_function_mode_change` | `GraphwarGameStateManager` | `0x477410` | `src/graphwar_game_state_manager.rs:404` |
| `handle_connection_removed_event` | `GraphwarGameStateManager` | `0x477540` | `src/graphwar_game_state_manager.rs:494` |
| `new` | `GraphwarGameStateManager` | `0x477bc0` | `src/graphwar_game_state_manager.rs:116` |
| `set_lock` | `GraphwarGameStateManager` | `0x477fa0` | `src/graphwar_game_state_manager.rs:454` |
| `set_name` | `GraphwarGameStateManager` | `0x478010` | `src/graphwar_game_state_manager.rs:203` |
| `set_state` | `GraphwarGameStateManager` | `0x4780c0` | `src/graphwar_game_state_manager.rs:223` |

### graphwar_graphics_manager（6 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `set_hat_graphics` | `GraphwarGraphicsManager` | `0x468a50` | `src/graphwar_graphics_manager.rs:78` |
| `set_face_graphics` | `GraphwarGraphicsManager` | `0x468b60` | `src/graphwar_graphics_manager.rs:69` |
| `set_skin_graphics` | `GraphwarGraphicsManager` | `0x468c70` | `src/graphwar_graphics_manager.rs:60` |
| `send_all_info_events` | `GraphwarGraphicsManager` | `0x468d80` | `src/graphwar_graphics_manager.rs:88` |
| `handle_entity_removed_event` | `GraphwarGraphicsManager` | `0x469220` | `src/graphwar_graphics_manager.rs:108` |
| `new` | `GraphwarGraphicsManager` | `0x469390` | `src/graphwar_graphics_manager.rs:38` |

### graphwar_lan_broadcaster_v6（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `set_room_info` | `GraphwarLanBroadcasterV6` | `0x4572c0` | `src/graphwar_lan_broadcaster_v6.rs:141` |
| `new` | `GraphwarLanBroadcasterV6` | `0x457480` | `src/graphwar_lan_broadcaster_v6.rs:35` |
| `start` | `GraphwarLanBroadcasterV6` | `0x457600` | `src/graphwar_lan_broadcaster_v6.rs:59` |
| `process` | `GraphwarLanBroadcasterV6` | `0x457790` | `src/graphwar_lan_broadcaster_v6.rs:86` |

### graphwar_lan_vbox（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `process` | `GraphwarLanVBox` | `0x4f90b0` | `src/graphwar_lan_vbox.rs:90` |

### graphwar_level_generator（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `place_entity` | `GraphwarLevelGenerator` | `0x465a70` | `src/graphwar_level_generator.rs:208` |
| `generate_level` | `GraphwarLevelGenerator` | `0x4661a0` | `src/graphwar_level_generator.rs:94` |
| `generate_terrain` | `GraphwarLevelGenerator` | `0x466c40` | `src/graphwar_level_generator.rs:179` |
| `generate_level_custom` | `GraphwarLevelGenerator` | `0x466d20` | `src/graphwar_level_generator.rs:49` |

### graphwar_level_manager（40 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `setup_level` | `GraphwarLevelManager` | `0x499740` | `src/graphwar_level_manager.rs:121` |
| `setup_level_0` | `GraphwarLevelManager` | `0x49e570` | `src/graphwar_level_manager.rs:148` |
| `setup_level_0` | `GraphwarLevelManager` | `0x49e650` | `src/graphwar_level_manager.rs:153` |
| `setup_level_1` | `GraphwarLevelManager` | `0x49e880` | `src/graphwar_level_manager.rs:223` |
| `setup_level_1` | `GraphwarLevelManager` | `0x49eab0` | `src/graphwar_level_manager.rs:218` |
| `setup_level_2` | `GraphwarLevelManager` | `0x49eb90` | `src/graphwar_level_manager.rs:293` |
| `setup_level_2` | `GraphwarLevelManager` | `0x49f1c0` | `src/graphwar_level_manager.rs:286` |
| `setup_level_3` | `GraphwarLevelManager` | `0x49f370` | `src/graphwar_level_manager.rs:379` |
| `setup_level_4` | `GraphwarLevelManager` | `0x49f510` | `src/graphwar_level_manager.rs:409` |
| `setup_level_4` | `GraphwarLevelManager` | `0x49fea0` | `src/graphwar_level_manager.rs:441` |
| `setup_level_5` | `GraphwarLevelManager` | `0x4a04d0` | `src/graphwar_level_manager.rs:501` |
| `setup_level_5` | `GraphwarLevelManager` | `0x4a0db0` | `src/graphwar_level_manager.rs:540` |
| `setup_level_6` | `GraphwarLevelManager` | `0x4a1310` | `src/graphwar_level_manager.rs:581` |
| `setup_level_6` | `GraphwarLevelManager` | `0x4a1cd0` | `src/graphwar_level_manager.rs:615` |
| `setup_level_7` | `GraphwarLevelManager` | `0x4a2430` | `src/graphwar_level_manager.rs:689` |
| `setup_level_7` | `GraphwarLevelManager` | `0x4a2dc0` | `src/graphwar_level_manager.rs:721` |
| `setup_level_8` | `GraphwarLevelManager` | `0x4a33f0` | `src/graphwar_level_manager.rs:782` |
| `setup_level_8` | `GraphwarLevelManager` | `0x4a3df0` | `src/graphwar_level_manager.rs:817` |
| `setup_level_9` | `GraphwarLevelManager` | `0x4a43a0` | `src/graphwar_level_manager.rs:879` |
| `setup_level_9` | `GraphwarLevelManager` | `0x4a4c80` | `src/graphwar_level_manager.rs:918` |
| `setup_level_10` | `GraphwarLevelManager` | `0x4a5390` | `src/graphwar_level_manager.rs:995` |
| `setup_level_11` | `GraphwarLevelManager` | `0x4a5940` | `src/graphwar_level_manager.rs:1099` |
| `setup_level_12` | `GraphwarLevelManager` | `0x4a5e80` | `src/graphwar_level_manager.rs:1168` |
| `setup_level_12` | `GraphwarLevelManager` | `0x4a6020` | `src/graphwar_level_manager.rs:1175` |
| `setup_level_13` | `GraphwarLevelManager` | `0x4a63d0` | `src/graphwar_level_manager.rs:1258` |
| `setup_level_14` | `GraphwarLevelManager` | `0x4a6980` | `src/graphwar_level_manager.rs:1320` |
| `setup_level_14` | `GraphwarLevelManager` | `0x4a73b0` | `src/graphwar_level_manager.rs:1362` |
| `setup_level_15` | `GraphwarLevelManager` | `0x4a78f0` | `src/graphwar_level_manager.rs:1403` |
| `setup_level_15` | `GraphwarLevelManager` | `0x4a8320` | `src/graphwar_level_manager.rs:1445` |
| `setup_level_16` | `GraphwarLevelManager` | `0x4a8860` | `src/graphwar_level_manager.rs:1488` |
| `setup_level_16` | `GraphwarLevelManager` | `0x4a91b0` | `src/graphwar_level_manager.rs:1521` |
| `setup_level_16` | `GraphwarLevelManager` | `0x4a93e0` | `src/graphwar_level_manager.rs:1516` |
| `setup_level_17` | `GraphwarLevelManager` | `0x4a94c0` | `src/graphwar_level_manager.rs:1555` |
| `setup_level_17` | `GraphwarLevelManager` | `0x4a9f00` | `src/graphwar_level_manager.rs:1590` |
| `setup_level_18` | `GraphwarLevelManager` | `0x4aa640` | `src/graphwar_level_manager.rs:1664` |
| `setup_level_18` | `GraphwarLevelManager` | `0x4aafc0` | `src/graphwar_level_manager.rs:1704` |
| `setup_level_19` | `GraphwarLevelManager` | `0x4ab500` | `src/graphwar_level_manager.rs:1744` |
| `setup_level_19` | `GraphwarLevelManager` | `0x4abf00` | `src/graphwar_level_manager.rs:1785` |
| `setup_level_20` | `GraphwarLevelManager` | `0x4ac440` | `src/graphwar_level_manager.rs:1826` |
| `setup_level_20` | `GraphwarLevelManager` | `0x4ace40` | `src/graphwar_level_manager.rs:1867` |

### graphwar_life_manager（9 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_death_tick` | `GraphwarLifeManager` | `0x453a60` | `src/graphwar_life_manager.rs:118` |
| `get_all_entities` | `GraphwarLifeManager` | `0x453c60` | `src/graphwar_life_manager.rs:139` |
| `set_queued_death` | `GraphwarLifeManager` | `0x453de0` | `src/graphwar_life_manager.rs:123` |
| `handle_tick_event` | `GraphwarLifeManager` | `0x454020` | `src/graphwar_life_manager.rs:175` |
| `send_all_info_events` | `GraphwarLifeManager` | `0x454120` | `src/graphwar_life_manager.rs:144` |
| `handle_entity_removed_event` | `GraphwarLifeManager` | `0x454420` | `src/graphwar_life_manager.rs:154` |
| `new` | `GraphwarLifeManager` | `0x454790` | `src/graphwar_life_manager.rs:76` |
| `get_alive` | `GraphwarLifeManager` | `0x454950` | `src/graphwar_life_manager.rs:114` |
| `set_alive` | `GraphwarLifeManager` | `0x454b40` | `src/graphwar_life_manager.rs:109` |

### graphwar_lobby_control（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `new` | `GraphwarLobbyControl` | `0x3ef990` | `src/graphwar_lobby_control.rs:28` |

### graphwar_local_player_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `handle_player_info` | `GraphwarLocalPlayerManager` | `0x519f40` | `src/graphwar_local_player_manager.rs:107` |
| `handle_connected_to_server` | `GraphwarLocalPlayerManager` | `0x51a570` | `src/graphwar_local_player_manager.rs:76` |
| `handle_entity_removed_event` | `GraphwarLocalPlayerManager` | `0x51a610` | `src/graphwar_local_player_manager.rs:146` |

### graphwar_logger（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `new` | `GraphwarLogger` | `0x44dd60` | `src/graphwar_logger.rs:44` |

### graphwar_long_press_animation（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw` | `GraphwarLongPressAnimation` | `0x4d8100` | `src/graphwar_long_press_animation.rs:52` |

### graphwar_mode_manager（2 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `set_time_mode` | `GraphwarModeManager` | `0x518530` | `src/graphwar_mode_manager.rs:113` |
| `send_all_info_events` | `GraphwarModeManager` | `0x5185a0` | `src/graphwar_mode_manager.rs:132` |

### graphwar_name_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `send_all_info_events` | `GraphwarNameManager` | `0x518f10` | `src/graphwar_name_manager.rs:69` |
| `handle_entity_removed_event` | `GraphwarNameManager` | `0x519030` | `src/graphwar_name_manager.rs:79` |
| `set_name` | `GraphwarNameManager` | `0x519350` | `src/graphwar_name_manager.rs:57` |

### graphwar_obstacle_manager（8 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw_circle` | `GraphwarObstacleData` | `0x3f11e0` | `src/graphwar_obstacle_manager.rs:89` |
| `is_obstacle` | `GraphwarObstacleManager` | `0x3f15b0` | `src/graphwar_obstacle_manager.rs:89` |
| `add_obstacle` | `GraphwarObstacleManager` | `0x3f17a0` | `src/graphwar_obstacle_manager.rs:258` |
| `clear_obstacles` | `GraphwarObstacleManager` | `0x3f19a0` | `src/graphwar_obstacle_manager.rs:252` |
| `destroy_obstacle` | `GraphwarObstacleManager` | `0x3f1b30` | `src/graphwar_obstacle_manager.rs:263` |
| `handle_tick_event` | `GraphwarObstacleManager` | `0x3f1d30` | `src/graphwar_obstacle_manager.rs:295` |
| `queue_destroy_obstacle` | `GraphwarObstacleManager` | `0x3f20f0` | `src/graphwar_obstacle_manager.rs:269` |
| `new` | `GraphwarObstacleManager` | `0x3f2360` | `src/graphwar_obstacle_manager.rs:208` |

### graphwar_path_calculator（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `calculate_paths` | `GraphwarPathCalculator` | `0x467530` | `src/graphwar_path_calculator.rs:45` |

### graphwar_plane（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw` | `GraphwarPlane` | `0x4b5df0` | `src/graphwar_plane.rs:84` |
| `init` | `GraphwarPlane` | `0x4b7380` | `src/obstacle_drawer.rs:37` |
| `ready` | `GraphwarPlane` | `0x4b7a70` | `src/graphwar_plane.rs:218` |
| `process` | `GraphwarPlane` | `0x4b83f0` | `src/graphwar_plane.rs:254` |
| `obstacle_partial_update_event` | `GraphwarPlane` | `0x4be3e0` | `src/graphwar_plane.rs:174` |

### graphwar_plane_pos_manager（6 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_position` | `GraphwarPlanePosManager` | `0x3c62b0` | `src/graphwar_plane_pos_manager.rs:116` |
| `set_position` | `GraphwarPlanePosManager` | `0x3c64c0` | `src/graphwar_plane_pos_manager.rs:111` |
| `get_all_positions` | `GraphwarPlanePosManager` | `0x3c66a0` | `src/graphwar_plane_pos_manager.rs:120` |
| `send_all_info_events` | `GraphwarPlanePosManager` | `0x3c6a10` | `src/graphwar_plane_pos_manager.rs:133` |
| `handle_entity_removed_event` | `GraphwarPlanePosManager` | `0x3c6b40` | `src/graphwar_plane_pos_manager.rs:142` |
| `new` | `GraphwarPlanePosManager` | `0x3c6d30` | `src/graphwar_plane_pos_manager.rs:81` |

### graphwar_play_screen_control（9 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `add_tie_icon` | `GraphwarPlayScreenControl` | `0x3d9000` | `src/graphwar_play_screen_control.rs:265` |
| `enable_replay_buttons` | `GraphwarPlayScreenControl` | `0x3d9b50` | `src/graphwar_play_screen_control.rs:591` |
| `disable_replay_buttons` | `GraphwarPlayScreenControl` | `0x3d9e40` | `src/graphwar_play_screen_control.rs:605` |
| `update_end_game_pop_up` | `GraphwarPlayScreenControl` | `0x3da130` | `src/graphwar_play_screen_control.rs:119` |
| `add_player_victory_icon` | `GraphwarPlayScreenControl` | `0x3db3e0` | `src/graphwar_play_screen_control.rs:233` |
| `update_names_button_texture` | `GraphwarPlayScreenControl` | `0x3dc470` | `src/graphwar_play_screen_control.rs:625` |
| `update_view_function_button_texture` | `GraphwarPlayScreenControl` | `0x3dc630` | `src/graphwar_play_screen_control.rs:619` |
| `new` | `GraphwarPlayScreenControl` | `0x3dc7f0` | `src/graphwar_play_screen_control.rs:71` |
| `process` | `GraphwarPlayScreenControl` | `0x3dcbb0` | `src/graphwar_play_screen_control.rs:275` |

### graphwar_player_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `send_all_info_events` | `GraphwarPlayerManager` | `0x455160` | `src/graphwar_player_manager.rs:91` |
| `handle_entity_removed_event` | `GraphwarPlayerManager` | `0x455250` | `src/graphwar_player_manager.rs:120` |
| `new` | `GraphwarPlayerManager` | `0x455410` | `src/graphwar_player_manager.rs:36` |

### graphwar_player_screen_control（8 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `load_items` | `GraphwarPlayerScreenControl` | `0x4f57d0` | `src/graphwar_player_screen_control.rs:113` |
| `hat_button_pressed` | `GraphwarPlayerScreenControl` | `0x4f77a0` | `src/graphwar_player_screen_control.rs:131` |
| `face_button_pressed` | `GraphwarPlayerScreenControl` | `0x4f7870` | `src/graphwar_player_screen_control.rs:127` |
| `skin_button_pressed` | `GraphwarPlayerScreenControl` | `0x4f7940` | `src/graphwar_player_screen_control.rs:118` |
| `update_buttons_background` | `GraphwarPlayerScreenControl` | `0x4f8050` | `src/graphwar_player_screen_control.rs:230` |
| `lock` | `GraphwarPlayerScreenControl` | `0x4f8850` | `src/graphwar_player_screen_control.rs:252` |
| `unlock` | `GraphwarPlayerScreenControl` | `0x4f8990` | `src/graphwar_player_screen_control.rs:268` |
| `pending` | `GraphwarPlayerScreenControl` | `0x4f8ad0` | `src/graphwar_player_screen_control.rs:260` |

### graphwar_rank_manager（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `send_all_info_events` | `GraphwarRankManager` | `0x454d80` | `src/graphwar_rank_manager.rs:73` |
| `handle_entity_removed_event` | `GraphwarRankManager` | `0x454e80` | `src/graphwar_rank_manager.rs:83` |
| `new` | `GraphwarRankManager` | `0x454f30` | `src/graphwar_rank_manager.rs:33` |
| `set_rank` | `GraphwarRankManager` | `0x455090` | `src/graphwar_rank_manager.rs:61` |

### graphwar_remote_event_handler（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `handle_client_remote_event` | `GraphwarRemoteEventHandler` | `0x3dee00` | `src/graphwar_remote_event_handler.rs:143` |
| `handle_server_remote_event` | `GraphwarRemoteEventHandler` | `0x3e2830` | `src/graphwar_remote_event_handler.rs:444` |
| `is_leader` | `GraphwarRemoteEventHandler` | `0x3e5910` | `src/graphwar_remote_event_handler.rs:135` |

### graphwar_replay_control（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `process` | `GraphwarReplayScreenControl` | `0x4c6f40` | `src/graphwar_replay_control.rs:100` |

### graphwar_replay_player（8 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `toggle_pause` | `GraphwarReplayPlayer` | `0x3d6080` | `src/graphwar_replay_player.rs:244` |
| `inject_events` | `GraphwarReplayPlayer` | `0x3d61a0` | `src/graphwar_replay_player.rs:210` |
| `go_to_next_checkpoint` | `GraphwarReplayPlayer` | `0x3d68b0` | `src/graphwar_replay_player.rs:181` |
| `go_to_first_checkpoint` | `GraphwarReplayPlayer` | `0x3d6a40` | `src/graphwar_replay_player.rs:151` |
| `go_to_previous_checkpoint` | `GraphwarReplayPlayer` | `0x3d6bb0` | `src/graphwar_replay_player.rs:162` |
| `pause` | `GraphwarReplayPlayer` | `0x3d7270` | `src/graphwar_replay_player.rs:249` |
| `process` | `GraphwarReplayPlayer` | `0x3d7380` | `src/graphwar_replay_player.rs:78` |
| `is_paused` | `GraphwarReplayPlayer` | `0x3d7630` | `src/graphwar_replay_player.rs:256` |

### graphwar_root_node（10 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `ready` | `GraphwarRootNode` | `0x3a2e10` | `src/graphwar_root_node.rs:1302` |
| `ready` | `GraphwarRootNode` | `0x3a5660` | `src/graphwar_root_node.rs:1338` |
| `process` | `GraphwarRootNode` | `0x3a5760` | `src/graphwar_root_node.rs:635` |
| `purchase_denied` | `GraphwarRootNode` | `0x3afb00` | `src/graphwar_root_node.rs:892` |
| `check_save_level` | `GraphwarRootNode` | `0x3afcf0` | `src/graphwar_root_node.rs:656` |
| `purchase_confirmed` | `GraphwarRootNode` | `0x3afe90` | `src/graphwar_root_node.rs:868` |
| `get_media_storage_path` | `GraphwarRootNode` | `0x3b00d0` | `src/graphwar_root_node.rs:904` |
| `process_button_pressed` | `GraphwarRootNode` | `0x3b0830` | `src/graphwar_root_node.rs:225` |
| `request_permissions_result` | `GraphwarRootNode` | `0x3b4ec0` | `src/graphwar_root_node.rs:861` |
| `from_arg_matches_mut` | `Args` | `0x3bd640` | `/home/lucas/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/clap_builder-4.5.11/src/parser/error.rs:32` |

### graphwar_server_broker_handler（2 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `stop` | `GraphwarServerBrokerHandler` | `0x4586d0` | `src/graphwar_server_broker_handler.rs:90` |
| `process` | `GraphwarServerBrokerHandler` | `0x4588d0` | `src/graphwar_server_broker_handler.rs:98` |

### graphwar_server_connection_handler（6 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `forward_event` | `GraphwarServerConnectionHandler` | `0x45a560` | `src/graphwar_server_connection_handler.rs:350` |
| `handle_clear_bans_request_event` | `GraphwarServerConnectionHandler` | `0x45a6b0` | `src/graphwar_server_connection_handler.rs:247` |
| `handle_connection_removed_event` | `GraphwarServerConnectionHandler` | `0x45a7b0` | `src/graphwar_server_connection_handler.rs:224` |
| `forward_connection_specific_event` | `GraphwarServerConnectionHandler` | `0x45aab0` | `src/graphwar_server_connection_handler.rs:326` |
| `handle_ban_connection_request_event` | `GraphwarServerConnectionHandler` | `0x45ad30` | `src/graphwar_server_connection_handler.rs:233` |
| `tick` | `GraphwarServerConnectionHandler` | `0x45b020` | `src/graphwar_server_connection_handler.rs:206` |

### graphwar_setup_control（14 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `clear_state` | `GraphwarSetupControl` | `0x4c1ae0` | `src/graphwar_texture_loader.rs:145` |
| `update_name` | `GraphwarSetupControl` | `0x4c1e40` | `src/graphwar_setup_control.rs:178` |
| `update_rank` | `GraphwarSetupControl` | `0x4c2140` | `src/graphwar_setup_control.rs:187` |
| `on_text_change` | `GraphwarSetupControl` | `0x4c27f0` | `src/graphwar_setup_control.rs:462` |
| `is_leader_local` | `GraphwarSetupControl` | `0x4c2b90` | `src/graphwar_setup_control.rs:310` |
| `update_soldiers` | `GraphwarSetupControl` | `0x4c2ef0` | `src/graphwar_setup_control.rs:202` |
| `on_chat_submitted` | `GraphwarSetupControl` | `0x4c44b0` | `src/graphwar_setup_control.rs:494` |
| `force_title_update` | `GraphwarSetupControl` | `0x4c4850` | `src/graphwar_setup_control.rs:474` |
| `connect_entry_buttons` | `GraphwarSetupControl` | `0x4c4c20` | `src/graphwar_setup_control.rs:257` |
| `ready` | `GraphwarSetupControl` | `0x4cf670` | `src/graphwar_setup_control.rs:652` |
| `ready` | `GraphwarSetupControl` | `0x4d1f10` | `src/graphwar_setup_control.rs:728` |
| `ready` | `GraphwarSetupControl` | `0x4d2600` | `src/graphwar_setup_control.rs:793` |
| `ready` | `GraphwarSetupControl` | `0x4d2f70` | `src/graphwar_setup_control.rs:772` |
| `process` | `GraphwarSetupControl` | `0x4d3180` | `src/graphwar_setup_control.rs:100` |

### graphwar_shot_calculator（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `calculate_shot_results` | `GraphwarShotCalculator` | `0x3c2bc0` | `src/graphwar_shot_calculator.rs:181` |

### graphwar_soldier_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `send_all_info_events` | `GraphwarSoldierManager` | `0x455570` | `src/graphwar_soldier_manager.rs:87` |
| `handle_entity_removed_event` | `GraphwarSoldierManager` | `0x4556d0` | `src/graphwar_soldier_manager.rs:105` |
| `new` | `GraphwarSoldierManager` | `0x455760` | `src/graphwar_soldier_manager.rs:35` |

### graphwar_target_manager（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `subscribe_to_events_event_handler` | `GraphwarTargetManager` | `0x465780` | `src/graphwar_target_manager.rs:74` |
| `subscribe_to_events_event_handler` | `GraphwarTargetManager` | `0x465870` | `src/graphwar_target_manager.rs:84` |
| `new` | `GraphwarTargetManager` | `0x465910` | `src/graphwar_target_manager.rs:35` |

### graphwar_team_manager（6 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `add_to_team` | `GraphwarTeamManager` | `0x3c17d0` | `src/graphwar_team_manager.rs:89` |
| `remove_from_team` | `GraphwarTeamManager` | `0x3c1980` | `src/graphwar_team_manager.rs:96` |
| `send_all_info_events` | `GraphwarTeamManager` | `0x3c1cf0` | `src/graphwar_team_manager.rs:127` |
| `handle_entity_removed_event` | `GraphwarTeamManager` | `0x3c1f40` | `src/graphwar_team_manager.rs:139` |
| `new` | `GraphwarTeamManager` | `0x3c1fc0` | `src/graphwar_team_manager.rs:63` |
| `get_team` | `GraphwarTeamManager` | `0x3c2120` | `src/graphwar_team_manager.rs:103` |

### graphwar_tick_handler（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_time_to_tick` | `GraphwarTickHandler` | `0x519450` | `/rustc/ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96/library/core/src/time.rs:201` |
| `get_fractional_tick` | `GraphwarTickHandler` | `0x519530` | `/rustc/ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96/library/core/src/time.rs:201` |
| `handle_connected_to_server_event` | `GraphwarTickHandler` | `0x519630` | `src/graphwar_tick_handler.rs:125` |
| `tick` | `GraphwarTickHandler` | `0x519840` | `src/graphwar_tick_handler.rs:76` |

### graphwar_turn_manager（13 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `get_current_turn_players` | `GraphwarTurnData` | `0x4627c0` | `src/graphwar_turn_manager.rs:80` |
| `get_current_turn_soldiers` | `GraphwarTurnData` | `0x4628d0` | `src/graphwar_turn_manager.rs:98` |
| `get_turn_mode` | `GraphwarTurnManager` | `0x462c10` | `src/graphwar_turn_manager.rs:172` |
| `set_turn_mode` | `GraphwarTurnManager` | `0x462d20` | `src/graphwar_turn_manager.rs:159` |
| `get_end_of_turn_tick` | `GraphwarTurnManager` | `0x462ec0` | `src/graphwar_turn_manager.rs:406` |
| `send_all_info_events` | `GraphwarTurnManager` | `0x462fe0` | `src/graphwar_turn_manager.rs:410` |
| `set_end_of_turn_tick` | `GraphwarTurnManager` | `0x463170` | `src/graphwar_turn_manager.rs:366` |
| `get_current_turn_players` | `GraphwarTurnManager` | `0x463340` | `src/graphwar_turn_manager.rs:336` |
| `get_current_turn_soldiers` | `GraphwarTurnManager` | `0x463460` | `src/graphwar_turn_manager.rs:340` |
| `handle_entity_removed_event` | `GraphwarTurnManager` | `0x463580` | `src/graphwar_turn_manager.rs:423` |
| `new` | `GraphwarTurnManager` | `0x463790` | `src/graphwar_turn_manager.rs:133` |
| `set_turn` | `GraphwarTurnManager` | `0x4638f0` | `src/graphwar_turn_manager.rs:379` |
| `next_turn` | `GraphwarTurnManager` | `0x463da0` | `src/graphwar_turn_manager.rs:261` |

### graphwar_video_writer（3 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `write_video` | `GraphwarVideoWriter` | `0x4f0610` | `src/graphwar_video_writer.rs:154` |
| `new` | `GraphwarVideoWriter` | `0x4f0bb0` | `src/graphwar_video_writer.rs:61` |
| `add_frame` | `GraphwarVideoWriter` | `0x4f0e20` | `src/graphwar_video_writer.rs:125` |

### math_function（5 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `remove_offset` | `MathFunction` | `0x4eb86a` | `src/math_function.rs:52` |
| `to_string_recursive` | `MathFunction` | `0x4eccb3` | `src/math_function.rs:285` |
| `new` | `MathFunction` | `0x4ed670` | `src/math_function.rs:75` |
| `mutate_fine_tune` | `FunctionRandomizer` | `0x4ee720` | `src/math_function.rs:452` |
| `mutate_function` | `FunctionRandomizer` | `0x748000` | `/rustc/ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96/library/core/src/slice/index.rs:1020` |

### obstacle_drawer（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `update` | `ObstacleDrawer` | `0x3ee190` | `src/obstacle_drawer.rs:51` |

### player_showcase（4 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `draw` | `PlayerShowcase` | `0x4e5400` | `src/player_showcase.rs:72` |
| `ready` | `PlayerShowcase` | `0x4e6480` | `src/player_showcase.rs:18` |
| `process` | `PlayerShowcase` | `0x4e68f0` | `src/player_showcase.rs:59` |
| `on_text_change` | `PlayerShowcase` | `0x4efd70` | `src/player_showcase.rs:31` |

### polish_notation_converter（1 个）

| 方法 | 类型 | x86 地址 | 源位置 |
|---|---|---|---|
| `reorder_recursive` | `PolishNotationConverter` | `0x4f5450` | `src/polish_notation_converter.rs:36` |

**合计 327 个**（去重后；不含 `歧义` 171 与 `未知` 260，那两类见 TSV）。

---

## 22. `graphwar_module_container.rs` 全解：容器是 `HashMap<TypeId, Rc<dyn Any>>`，不是"按偏移持有 manager"（2026-11）

> **本节解决什么**：这是剩下最大的单模块（**68 个函数**，`§18` A 表），此前
> `docs/02` §14 / §17、`docs/149` 都写「未实现」，且**多处文档假定它是"按固定偏移
> 持有 25 个 manager 的结构体"**（例如把 `graphwar_lan_vbox` 的 `+0x128` / `+0x240`、
> `GraphwarRootNode` 的 `+0x16a0` / `+0x16a8` 当成"容器的字段"）。
> 本节用两侧反汇编把这条假定**否掉**，并给出：容器真身（1 个字段 / 3 个方法 /
> 68 个单态化）、25 个被注册模块的真名与尺寸、三处装配点、以及为什么
> `panic::Location` 在这模块上几乎全哑。
>
> **地址口径**：一律 **ELF vaddr**；Ghidra/REA 地址 = 本地址 + `0x100000`（只加一次）。
> 两个二进制都是 **2.0.60**：
>
> * **arm64**（Android，未 strip，`.symtab` 有 18793 个 `FUNC`，本模块全部类型名的来源）；
> * **x86-64**（Linux Steam，stripped，`0x100000` 偏移的落点）。
>
> 2024-07-30 版只在**方法名**上被引用（`add_module` / `get_module` / `get_module_wk`），
> 它的地址与 2.0.60 不可混用（`§18` 的口径）。

### 22.1 一页结论

| 项 | 结果 | 档位 |
|---|---|---|
| 类型本体 | `pub struct GraphwarModuleContainer { modules: HashMap<TypeId, Rc<dyn Any>> }`，**48 字节、一个字段** | **观测** |
| 方法数 | **3 个泛型方法**：`add_module` / `get_module` / `get_module_wk` | **观测**（2024 符号表 + 两侧反汇编） |
| 68 个函数 | 25 + 23 + 20 个**单态化**，三块各自等距且**背靠背** | **观测** |
| 有没有 `init` / `process` | **没有**。装配在调用者：`GraphwarGameClient::new` 22 条、`game_server_thread` 20 条、`GraphwarBackend::start_*` 1 条 ×2 | **观测** |
| "各 manager 在 `+0xNN`" | **不成立**。容器里只有 `TypeId` 键，没有偏移 | **观测**（否定结论） |
| 注册的模块数 | **25**（另有第 26 个 `Rc<RefCell<_>>` 型 `GraphwarEventQueue` **不**注册） | **观测** |
| `panic::Location` | `add_module`/`get_module` **零锚**；`get_module_wk` 只有 **1 个**（`30:28`，20 个单态化共用） | **观测** |
| x86 地址 | **全部 68 个已定名**（本节 §22.3 表），主证据是 `get_module_wk` 的 `Location` 锚 + 三块等距 + 装配点调用序 | **观测** |
| 原版源码行号 | 只有 `30:28` 一个（且是 `RefCell::borrow` 的 `#[track_caller]` 传下来的） | **推断**（行号本身无法直接复核） |

**为什么这条结论重要**：它一次性回答了"全局对象叫什么、多大、放哪"——
25 个 manager 的**类型名 + 尺寸 + 三个入口地址**（§22.3）可以直接被别的模块引用；
`graphwar_event_queue` 不在此列这件事本身也是证据（它挂在别处，`未知`）。

### 22.2 复算方法（三路证据，缺一路都不够）

1. **方法名 + 单态化个数**（两版符号表都量，`§18`）：

   ```text
   python3 tools/symtab_dump.py methods graphwar_module_container
   → GraphwarModuleContainer: add_module / get_module / get_module_wk

   # 分项计数（三方各跑一遍；注意 `grep -c FUNC` 会多算一条
   # drop_in_place<Weak<RefCell<GraphwarModuleContainer>>>）
   readelf -sW <so> | grep graphwar_module_container | grep FUNC \
     | grep -oE '(10add_module|10get_module|13get_module_wk)' | sort | uniq -c
   ```

   | 二进制 | `add_module` | `get_module` | `get_module_wk` | 合计 |
   |---|---|---|---|---|
   | x86 2024-07-30（未 strip） | 24 | 23 | 19 | **66**（= `§18` A 表的 66） |
   | arm64 2.0.60（未 strip） | 25 | 23 | 20 | **68** |
   | x86 2.0.60（stripped） | 25 | 23 | 20 | **68**（本节 §22.2 第 3 步量的） |

   ⇒ 「68 个函数」是 **2.0.60** 的数字；2024 版是 **66**，两年里**多注册 1 个模块、
   多要 1 个弱句柄**。究竟哪一型新增：`未知`（本表没做跨版本逐型对齐）。
   ⚠️ 仓库里那句「2024 版符号表显示本模块有 68 个函数」是**旧误记**，
   `§18` A 表一直写着 66。
2. **每个 `add_module` 属于哪个型**（arm64 有名字，x86 没有）：

   ```text
   # 函数体形如：ldr q0,[TypeId] → adrp/add x3,<vtable> → bl HashMap::insert
   llvm-objdump -d --no-show-raw-insn --start-address=0x49e520 --stop-address=0x49e5f4 <arm64 so>
   # vtable 第 0 字的 R_AARCH64_RELATIVE addend =
   #   drop_in_place<core::cell::RefCell<graphwarrust::<module>::<Type>>>
   readelf -rW <arm64 so> | grep 858aa8      # → 4b21b8 等
   ```

   `drop_in_place` 的名字**直接给出类型**；第 2 字 = `size_of::<RefCell<M>>()`。
   注意 LLVM 会**合并**同尺寸、同形状的 drop glue（`GraphwarRankManager` /
   `GraphwarSoldierManager` / `GraphwarPlayerManager` 三个函数体里都指向
   `drop_in_place<…RankManager…>`），所以必须再用**调用点的构造函数**复核：
   每个 `add_module` 前面那条 `bl` 就是 `<Type>::new`。
3. **x86 侧定址**（`Location` 锚 + 结构）：

   ```text
   # (a) 找到本模块唯一的 Location 记录 {file ptr,len=32,line=30,col=28}
   #     → x86 .data.rel.ro 0x9586b8；谁 lea 它，谁就是 get_module_wk
   #     → 恰好 20 个，0x46c5a0 起、步长 0x220（544）
   # (b) 往低地址按下表展开，得到 add/get 两块
   #     add 25×160 = 0x4694f0..0x46a3f0
   #     get 23×368 = 0x46a490..0x46c430
   # (c) 给 25 个 add_module 定名：读 GraphwarGameClient::new（0x517690）与
   #     服务端线程（0x4500c0）里的调用序，与 arm64 的调用序逐位对齐；
   #     再用 vtable 的 size 字（= size_of::<RefCell<M>>）逐个校验
   ```

   (c) 的校验是**逐位相等**的：两侧的 22/20 条调用序给出的尺寸序列完全相同
   （`80,120,72,80,80,32,304,40,80,40,80,88,40,80,40,176,80,80,56,80,128,80`），
   这是"x86 名字没有对错位"的硬证据。

### 22.3 25 个被注册的模块（真名 / 尺寸 / 六个入口）

`size` = `size_of::<M>()` = vtable 第 2 字 − 8（`RefCell` 借用标志占 8 字节）。
`—` 表示**该型没有这个单态化**（不是数据缺失）。

| 类型 | 原版模块 | `size` | arm64 `add` | arm64 `get` | arm64 `wk` | x86 `add` | x86 `get` | x86 `wk` |
|---|---|---|---|---|---|---|---|---|
| `GraphwarModeManager` | `graphwar_mode_manager` | 32 | `0x49e520` | `0x4a0754` | `0x4a5070` | `0x469a90` | `0x46abc0` | `0x46e140` |
| `GraphwarTickHandler` | `graphwar_tick_handler` | 72 | `0x49e870` | `0x4a0ab4` | `0x4a34c4` | `0x469810` | `0x46aa50` | `0x46d040` |
| `GraphwarPlayerManager` | `graphwar_player_manager` | 72 | `0x49f260` | `0x4a0244` | `0x4a3c50` | `0x469f90` | `0x46a490` | `0x46d8c0` |
| `GraphwarSoldierManager` | `graphwar_soldier_manager` | 72 | `0x49f18c` | `0x4a1b94` | `0x4a2d38` | `0x46a170` | `0x46a770` | `0x46e580` |
| `GraphwarTeamManager` | `graphwar_team_manager` | 32 | `0x49f4dc` | `0x4a0fc4` | `0x4a43dc` | `0x469770` | `0x46b8b0` | `0x46e7a0` |
| `GraphwarTurnManager` | `graphwar_turn_manager` | 32 | `0x49f900` | `0x49fd34` | `0x4a2fbc` | `0x46a030` | `0x46b740` | `0x46ce20` |
| `GraphwarLifeManager` | `graphwar_life_manager` | 72 | `0x49eaec` | `0x4a1324` | `0x4a48e4` | `0x469b30` | `0x46c2c0` | `0x46d6a0` |
| `GraphwarEffectManager` | `graphwar_effect_manager` | 72 | `0x49f0b8` | `0x49f9d4` | `0x4a2328` | `0x469630` | `0x46a600` | `0x46cc00` |
| `GraphwarTargetManager` | `graphwar_target_manager` | 72 | `0x49f408` | `0x4a1d44` | `0x4a4158` | `0x46a350` | `0x46c150` | `0x46ee00` |
| `GraphwarNameManager` | `graphwar_name_manager` | 72 | `0x49ec94` | `0x4a1174` | `0x4a3ed4` | `0x46a2b0` | `0x46ba20` | `0x46df20` |
| `GraphwarColorManager` | `graphwar_color_manager` | 120 | `0x49e6c8` | `0x4a1684` | `0x4a20a4` | `0x469590` | `0x46b5d0` | `0x46dd00` |
| `GraphwarRankManager` | `graphwar_rank_manager` | 72 | `0x49ef10` | `0x49fee4` | `0x4a2ab4` | `0x469ef0` | `0x46bfe0` | `0x46e360` |
| `GraphwarPlanePosManager` | `graphwar_plane_pos_manager` | 24 | `0x49efe4` | `0x4a0904` | `0x4a25ac` | `0x46a0d0` | `0x46ad30` | `0x46c9e0` |
| `GraphwarFunctionManager` | `graphwar_function_manager` | 32 | `0x49ea18` | `0x4a1834` | `0x4a39cc` | `0x469db0` | `0x46bb90` | `0x46e9c0` |
| `GraphwarGameStateManager` | `graphwar_game_state_manager` | 296 | `0x49ee3c` | `0x4a0094` | `0x4a3748` | `0x46a210` | `0x46be70` | `0x46c7c0` |
| `GraphwarGraphicsManager` | `graphwar_graphics_manager` | 168 | `0x49e944` | `0x49fb84` | `0x4a4dec` | `0x4698b0` | `0x46a8e0` | `0x46d480` |
| `GraphwarEntityEffectManager` | `graphwar_entity_effect_manager` | 72 | `0x49ebc0` | — | `0x4a2830` | `0x469d10` | — | `0x46dae0` |
| `GraphwarComputerPlayerManager` | `graphwar_computer_player_manager` | 72 | `0x49e5f4` | `0x4a0e14` | `0x4a4660` | `0x469e50` | `0x46b460` | `0x46c5a0` |
| `FunctionHistoryRecorder` | `function_history_recorder` | 48 | `0x49ed68` | `0x4a03f4` | — | `0x469bd0` | `0x46b010` | — |
| `GraphwarObstacleManager` | `graphwar_obstacle_manager` | 80 | `0x49f82c` | `0x4a14d4` | `0x4a4b68` | `0x46a3f0` | `0x46bd00` | `0x46ebe0` |
| `GraphwarLocalPlayerManager` | `graphwar_local_player_manager` | 64 | `0x49f334` | `0x4a0c64` | — | `0x4694f0` | `0x46c430` | — |
| `GraphwarClientConnectionHandler` | `graphwar_client_connection_handler` | 112 | `0x49f758` | `0x4a05a4` | — | `0x469950` | `0x46b180` | — |
| `GraphwarServerConnectionHandler` | `graphwar_server_connection_handler` | 128 | `0x49f684` | `0x4a1ef4` | — | `0x4696d0` | `0x46b2f0` | — |
| `GraphwarBanList` | `graphwar_ban_list` | 96 | `0x49e79c` | — | `0x4a3240` | `0x4699f0` | — | `0x46d260` |
| `GraphwarGameRecorder` | `graphwar_game_recorder` | 56 | `0x49f5b0` | `0x4a19e4` | — | `0x469c70` | `0x46aea0` | — |

**覆盖关系的含义**（这三条是 §22.2 三路证据的**交集**，`tests/` 逐条断言）：

* 25 个 `add` = 全部；
* 23 个 `get` = 25 − {`GraphwarBanList`, `GraphwarEntityEffectManager`}（只用弱引用取）；
* 20 个 `wk` = 25 − {`FunctionHistoryRecorder`, `GraphwarLocalPlayerManager`,
  `GraphwarClientConnectionHandler`, `GraphwarServerConnectionHandler`,
  `GraphwarGameRecorder`}（只被强持有，从不需要弱句柄）。

**尺寸分布**（arm64 实测，也是指纹）：`72 × 10`、`32 × 4`、其余 11 个型各不相同
（24 / 48 / 56 / 64 / 80 / 96 / 112 / 120 / 128 / 168 / 296）。
10 个同为 72 字节的 manager 说明它们结构同形（若干 `Rc` + 小状态），
`0x128` 级别的大结构**一个都没有**。

### 22.4 68 个函数的块结构 + 装配点

| 方法 | 个数 | arm64 块（首 / 步长 / 尾） | x86 块（首 / 步长 / 尾） |
|---|---|---|---|
| `add_module` | 25 | `0x49e520` / `212` / `0x49f9d4` | `0x4694f0` / `160` / `0x46a490` |
| `get_module` | 23 | `0x49f9d4` / `432` / `0x4a20a4` | `0x46a490` / `368` / `0x46c5a0` |
| `get_module_wk` | 20 | `0x4a20a4` / `644` / — | `0x46c5a0` / `544` / — |

三块**首尾相接、零空隙**（`add` 的块尾 == `get` 的块首，依此类推），
两侧都是如此。这条把"68 个孤立地址"变成"一条链"，`tests/graphwar_module_container.rs`
用它反查整张表（块内任何一位抄错都会立刻红）。

**装配点**（容器自己没有 `init`，25 个 `add_module` 由这些调用者发出）：

| 调用者 | arm64 | x86 | `add_module` 条数 |
|---|---|---|---|
| `GraphwarGameClient::new` | `0x409c70` | `0x517690` | **22** |
| `graphwar_game_server::game_server_thread` | `0x483574` | `0x4500c0` | **20** |
| `GraphwarBackend::start_*`（两条孪生单态化） | `0x407848` / `0x407b9c` | `0x515590` / `0x5159a0` | 1 + 1（都是 `GraphwarGameRecorder`） |

`22 + 20 − 18（交集）+ 1 = 25`：两个装配点的**并集**是 24 型，
唯一只由 backend 注册的是 `GraphwarGameRecorder`。

**这与 `docs/16-client.md` §17.1 的关系**：那里记「`0x517690` 装配 **14** 个 manager」，
是当时按"看起来像 manager 构造"的 `call` 数出来的**下限**；按本节口径
（`TypeId` + vtable 对齐）实际是 **22** 条。旧数字不删，就地更正：
**22 才对**，14 是漏数（§17.1 的其余结论不受影响）。

### 22.5 三个方法的逐指令形状

```text
add_module<M>(&mut self, module: Rc<RefCell<M>>)         x86 0x4694f0
  incq   (%rsi)                    ; Rc::clone
  movups <TypeId::of::<M>()>,%xmm0 ; 键：16 字节，来自 .rodata
  lea    <dyn Any vtable>,%rcx     ; 值：胖指针 (data, vtable)
  call   hashbrown::HashMap::insert
  ...    decq/test/decq            ; insert 的返回值（旧值）当场 drop
  decq   (%rsi)                    ; 放掉入参那份

get_module<M>(&self) -> Option<Rc<RefCell<M>>>           arm64 0x49f9d4
  ldr    x8,[x0,#0x18] ; cbz       ; RawTable.items == 0 → None（空表快路径）
  add    x0,x0,#0x20 ; bl hash_one ; RandomState 在 +0x20
  ...    探测 32 字节实体            ; sub x15,x10,x15,lsl #5
  ldp    x19,x20,[x15,#-0x10]      ; 命中：Rc<dyn Any> 胖指针
  incq   (%x19)                    ; Rc::clone
  ldp    x8,x9,[x20,#0x10]         ; vtable 第 3/4 字 = align / Any::type_id
  blr    x9                        ; 取回 TypeId::of::<RefCell<M>>() 比较值
  ...    eor/eor/orr/cbz           ; 与常量比 → 相等给 Rc，不等给 None

get_module_wk<M>(container: &Weak<RefCell<Self>>)        arm64 0x4a20a4
  ldr    x20,[x0] ; cmn x20,#1 ; b.eq →None   ; Weak::new() 的 ptr==usize::MAX 哨兵
  ldr    x8,[x20] ; cbz →None                 ; Weak::upgrade: strong == 0 → 死
  incq   (%x20)                               ; upgrade 成功：strong += 1
  ldr    x8,[x20,#0x10] ; cmp x8,#0x7fff… ; b.hs →panic_already_mutably_borrowed
  add    x8,x8,#1 ; str x8,[x20,#0x10]        ; Rc::borrow（← 30:28 的来源）
  ldr    x9,[x20,#0x30] ; cbz →None           ; items（容器基址 = x20+0x18）
  add    x0,x20,#0x38 ; bl hash_one           ; RandomState（+0x18+0x20）
  ...    同上探测 + downcast
  ...    Rc::downgrade（incq 0x8(%x19)）→ 返还 Ref
```

**签名级的两条证据**（不是猜）：

1. `get_module` 的 `self` 是**容器值**（`&Self`）——`+0x18` 读 `items`、`+0x20` 是
   `RandomState`，正是 `HashMap` 的字段序；调用点形如
   `……borrow().get_module::<X>()`（arm64 `0x39d3e4`，前一指令 `add x0,x21,#0x18`，
   `x21 = [x20+0x238]` 即某 manager 里那个 `Rc<RefCell<GraphwarModuleContainer>>` 字段）。
2. `get_module_wk` 的实参是 **`&Weak<RefCell<Self>>`**，不是 `&Rc`：
   `cmn x20,#1` 就是 `Weak::inner()` 的 `usize::MAX` 哨兵判据。
   ⇒ 各 manager 手里握的是 `Weak`（打断 container ↔ module 环），
   取弱句柄时先 `upgrade` 再 `borrow`；这也是它**必须**能 panic 的原因。

### 22.6 为什么 `panic::Location` 在这模块上全哑

* `add_module` / `get_module` **没有任何 panic 点**（无 `unwrap` / 索引 / `expect`），
  所以 2024 版符号表里它们没有 Location 锚；`fixtures/name_to_addr.tsv` 把
  68 条全标成「未知」，`docs/02` §20 的 join 也一个都没配上——**这是正确结果**，
  不是工具坏了，也不是数据残缺。
  ⚠️ 上一轮曾把 `0x46c5a0` / `0x46c7c0` / `0x46c9e0` / `0x46cc00` 当
  "`get_module_wk` 的 4 个候选"（TSV 里是 `歧义`）。重扫结果：**20 个**，
  那 4 个只是前 4 个（块间距 `0x220`），TSV 那 4 条候选**不完整**，不能当"只有 4 个"。
* `get_module_wk` 有**且仅有 1 个**锚：`src/graphwar_module_container.rs:30:28`。
  它来自 `RefCell::borrow` 的 `#[track_caller]`——`panic_already_mutably_borrowed`
  抛错时带的是**调用者**的 `Location`。所以第 30 行第 28 列就是那个 `.borrow()`
  （`推断`：行号本身无法直接复核，但"`borrow` 在泛型体里"这一点与 20 个单态化
  共用一个锚**完全一致**）。
* 于是 x86 侧的定址顺序是：**先用这唯一的锚把 20 个 `get_module_wk` 钉住**，
  再靠"三块等距 + 尺寸序列 + 装配点调用序"把另两块推出来（§22.2）。

### 22.7 剩下的未知（不装懂）

| 问题 | 档位 | 缺什么 |
|---|---|---|
| `graphwar_event_queue::GraphwarEventQueue` 也是 `Rc<RefCell<_>>`，为何**不**注册进容器 | **未知** | 需要找到它的持有者（它没有 `add_module` 单态化，说明挂在别处；容器不解释它） |
| 原版 `get_module` 返回 `Option` 还是 `Rc` + panic | **观测**（`Option`） | 反汇编里未命中走 `x0 = 0` 直接返回，无 panic 调用 ⇒ 是 `Option`；但"源码是否写了 `expect` 后被优化掉"无法排除 |
| `add_module` 对"同型重复注册"的意图（替换 vs 覆盖） | **观测**（替换） | `insert` 的返回值被 drop，行为确定；**意图**（是否期望重复）无证据 |
| 各 manager 内部字段偏移 | **不属于本模块** | 例如 `GameRecorder` 的 `Weak` 字段在 `+0x28`（`Rc` 分配 `+0x40`）、
`GraphwarRootNode` 的容器在 `+0x238`；这些要写到各自模块去 |
| 25 个型的 `new` 里各自做了什么 | **不属于本模块** | 本模块只负责"注册/取回"，构造函数在各自文件里 |

### 22.8 落码与测试

* 实现：`src/graphwar_module_container.rs`（3 个方法 + 25 型台账 + 六块几何 + 4 个装配点）。
* 验收：`tests/graphwar_module_container.rs`，17 条，全部围绕**可复算的结构**：
  块内等距 / 块间背靠背 / `slot_of` 反查唯一 / 尺寸分布 / 覆盖关系 /
  三处装配点的并集 = 25 / 容器 48 字节 / 替换语义与 drop / 弱引用生死 /
  `Weak::new()` 哨兵 / 可变借用时 panic（`30:28`）。
* ⚠️ 按 `AGENTS.md` §0.2：这些断言是**回归保护**，不是复原证据；
  本节的证据是 §22.2 那三条复算路径。

### 22.9 就地更正（旧文档里被本节推翻的四条）

按 `AGENTS.md` §0.6「文档不许虚报」——旧文不删，就地标明。

| 旧说法（出处） | 本节结论 | 证据 |
|---|---|---|
| 「`graphwar_module_container.rs`（20/1）… 无实现 / `未做`」（§14、§17 表、`docs/149`） | 已实现，档位 `观测` | 本文件 §22 + `src/graphwar_module_container.rs` + `tests/` 17 条 |
| 「2024 版符号表显示本模块有 **68** 个函数」（本模块骨架注释、`docs/HANDOVER.md`） | 2024 版是 **66**（24/23/19）；**68** 是 2.0.60（25/23/20）。`§18` A 表一直写的是 66 | §22.2 第 1 步的三行表 |
| 「`container+0x128` / `container+0x240` 是**模块槽**（在根节点的模块容器里）」（`docs/09-network.md` §10.4） | 它们是 **`GraphwarLanVBox` 自己的字段**（`docs/144-lobby-fixes.md` §1 已按 `LanVBox.lan_broadcaster` / `.server_broker_handler` 记）；容器**没有槽** | 本文件 §22.1：容器 = `HashMap<TypeId, Rc<dyn Any>>`，`add_module` 的 `self` 偏移 0 就是 `HashMap` |
| 「`0x517690` 装配 **14** 个 manager」（`docs/16-client.md` §17.1） | **22** 条 `add_module`（14 是漏数；那 14 条只是"看起来像构造 manager"的 `call`） | 本文件 §22.4：x86 `0x517690` 22 条、arm64 `0x409c70` 22 条，调用序逐位相同 |
| 「`get_module_wk` 的 x86 候选：`0x46c5a0`/`0x46c7c0`/`0x46c9e0`/`0x46cc00`（4 个，`歧义`）」（`fixtures/name_to_addr.tsv`） | **20 个**：`0x46c5a0 + i×0x220`；那 4 个只是前 4 个，TSV 那 4 条**不完整**（不可当"只有 4 个"） | 本文件 §22.2 第 3 步：Location 记录 `0x9586b8` 的引用者恰好 20 个 |

⚠️ 同样地，`docs/02` §8 表里本模块「68 个归属函数 / 311 个引用点」是**旧口径**
（`fixtures/module_funcs.tsv` 的 panic 串启发式），与本节的 68（= 25+23+20 个**单态化**）
**数字碰巧相同但含义不同**：前者是"函数体里出现该模块 panic 串"，后者是符号表里
属于该模块的 `FUNC`。别把两个 68 混用。

---

## 34. 名字匹配三表的过程记录（第 34–40 节）—— 已整体搬入 archive，这里只留索引

> 2026-11-08 搬迁，**内容未改、节号不变**：原第 34–40 节（7 节 / 1,869 行 / 搬迁前第 2800–4668 行）
> 现在在 [`archive/157-name-matching.md`](archive/157-name-matching.md)（第 23–33 节本来就是空号，沿用原编号，不重排）。

| 节（在 [`157`](archive/157-name-matching.md) 里） | 那一节回答了 |
|---|---|
| `157` §34 | `weak_caller` 是 **crate 内模块**（27 个 direct 符号）、**39 个单态化 = 2 个泛型方法 × 26 个类型** ⇒ 计入 `other`，作废 §17.2 的"第三方依赖"读法 |
| `157` §35–§36 | 没有 `Location` 锚的那 **970** 个怎么跨架构匹配；多候选定夺与无候选归因 |
| `157` §37–§38 | Windows 2.0.60 `graphwarrust.dll` 把可定名从 325 提到 **351**；三向匹配的天花板 **2,034**，之外只值 ≈42 个 |
| `157` §39–§40 | 符号名判据统一；无名函数 **179**（138 个能给出源函数名 + 41 个只到角色/未知） |

**数字结论留在主文档**（其余全是过程与逐函数证据）：并集 **351** 个地址、**527 内 348**
（索引见 §20/§21 与 [`149-coverage-matrix.md`](149-coverage-matrix.md)）；`sym_match` 复算的
**762**（unique 681 + weak 81 = 63.0%）与"327 单表"的差别在 `157` §40.2。

## 35. 没有 `Location` 的 12,736 个函数：结构身份指纹 —— **结构同源候选，精度 ≈80–95%、逐条待核**（跨 ISA 6.5% 是负结论）；不给名字（2026-11，接 §37 / §39 / §40）

> **工具**：`tools/callgraph_fingerprint.py`（`build` / `verify` / `tune2` / `calib` / `run` / `merge` / `report` / `probe`）；
> **夹具**：`fixtures/callgraph_fingerprint.tsv`（**新增**，12,736 行 × 16 列，本节 §41.5 的表由它生成）。

### 35.0 一句话结论 + 四问四答

**13,263 个函数里，527 个引用了 `src/*.rs` 的 `panic::Location`（§37/§39/§40 已处理），
剩下 12,736 个一个 `src/` `Location` 都不引用 —— 三份符号表里都没有它们，名字要不回来了。**
（**审计 159 发现原值「都没有」不精确，实测 6 个有名字**：工具自己的 `report` 打印「目标集合里有一个名字的 6 个」= `named_linux()` 354 − ∩527 348。）
本节给这 12,736 个做**结构身份指纹**：四份构建全量建库，再用**不含名字、不含地址、不含 `Location`**
的打分器跨构建配对，最后用 348 个已定名函数做**分半正对照**量出准确率。

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 指纹库** | 四份构建**全量**建成：Linux 13,263 / Android 12,609 / Windows 51,985 / WinEarly2026 43,096（每份都有 `size`/指令直方图/常量/被调者/调用者/引用字符串/机器码 hash） | **观测** |
| **(b) 正对照** | 同 ISA（Linux↔Windows 2.0.60）留出半 **top-1 84.2% / top-5 93.3%**；调参半 89.8% / 95.8%。⚠ 这是**「已定名 348」这一批上的数**，不是 12,736 上的数（见下面的口径块） | **观测**（只对 348） |
| **(c) 12,736 的结果** | **高置信同源 2,745 / 推断同源 528 / 歧义（打平）5,591 / 未知 3,872**（单射修好后重出，见 §35.5.1；修前 2,803 / 468 / 5,592 / 3,873）。高置信那 2,745 里 **610 个 <256 B，精度未校准** | **推断**（不是名字） |
| **(d) 跨 ISA（Android）** | 同一套打法 top-1 只有 **6.5%**（留出半）⇒ **跨 ISA 不可匹配，本节把它当负结论报，不当证据用** | **未知** |

> **口径（`docs/159` 对抗性审计后改写；引用本节必须连这段一起引）：**
>
> **结论 = 「结构同源候选，精度 ≈80–95%，逐条待核」。**
>
> ```
> 84.2% / 98.0% 是在「已定名 348」上测的（体量中位 624 B、100% 带 Location 文件名串），
> 不能直接搬到 2,803 这批量级；去掉泄漏特征后是 73.3%
> ```
>
> 三件事一起看才不误读：
>
> 1. **它们是上界，不是估计。** 348 这一批**系统性更容易**：体量中位 **624 B**（目标集合 58.8% <256 B），
>    而且 **348/348 = 100%** 携带 `src/*.rs` 文件名字符串，目标集合只有 **1,500/12,736 = 11.8%**。
>    （624 B 的出处见 §35.7 第 7 条：那是 `named_linux()` 的 **354** 个上的中位，正对照真正用的
>    348 个是 **640 B**。两个数都是"已定名那一批"，都远大于目标集合。）
> 2. **`strs`(F7) 就是 `Location` 的文件名**（`panic::Location` 前 16 B 是 `(&'static str, len)`，
>    被扫描器当成「(ptr,len) 槽」解成文件名字符串）。把它权重清零重跑（同真值同分半）：
>    留出半 top-1 **84.2% → 73.3%**（`strs`+`consts` 都清零 **71.5%**）。
>    **这条按审计意见只打折、不"修掉"** —— 特征保留，但精度只能引用打过折的数。
> 3. **`hist`(F2) 在最终配置里可以清零**：0/331 翻转，两半 top-1 一个都不变 ⇒ 它没有提供信息，
>    §35.2/§35.3 里"唯一出力的两项之一"那句话在**最终权重**下已不成立（同样只打折、不改表）。
>
> ⇒ 能给的只有量级，没有点估计：**独立抽检 20/20「像」**、**反向检验 10 个里 7 个地址+名字都对**
> （6 个高置信行里 5 个对，错的正是同模块近孪生）。见 `docs/159` §3/§4/§7/§9。

**这不是命名。** 本节产出的是「这个 Linux 函数与 Windows 的哪个函数**结构同源**」+ 结构角色，
名字仍然没有 —— 而且 §35.5 的 2,745 个高置信里 **0 个是逐字节相同的机器码**（`bsig` 全不同），
所以连它们也只是**推断**级，不是「逐位验证」。

### 35.1 指纹库：四份构建、每份都是全量

口径与既有夹具一致（不是这里另起的）：Linux 用 Ghidra 起点表 `funcstarts_ghidra_va.pkl`
（与 `fixtures/name_to_addr.tsv` 同口径）、Android 用 `.symtab` 的 FUNC、Windows 用 `.pdata` 的
`RUNTIME_FUNCTION` + 未被覆盖的 COFF FUNC 符号（= `three_way_match` 的 `elf_universe` / `pe_universe`）。

| 构建 | 函数数 | 有指纹 | 有被调者 | 有调用者 | 引用字符串 | ICF 等体组 | `Location` 结构 |
|---|---|---|---|---|---|---|---|
| Linux `2026-08-28`（目标） | **13,263** | 13,263（100%） | 7,228 | 5,836 | 3,568 | 119 组 / 548 地址 | 5,300 |
| Android `2.0.60` | **12,609** | 12,609（100%） | 9,443 | 8,307 | 3,017 | 105 组 / 456 地址 | 5,223 |
| Windows `2.0.60` | **51,985** | 51,985（100%） | 47,216 | 15,170 | 11,565 | 128 组 / 452 地址 | 6,284 |
| Windows `early2026` | **43,096** | 43,096（100%） | 37,427 | 14,393 | 15,595 | 175 组 / 723 地址 | 6,037 |

**两处踩过的坑，写在这里免得后人复发**（都是"扫描器自己停了"而不是"没有函数"）：

1. **capstone 的 `disasm` 遇解码失败会停，不会跳。** 第一版直接扫 Android 的 `.text`（5,223,648 B）
   只出 **673,319** 条指令（51.6% 的字节），后半个 `.text` 的 6,686 个函数**全是零指令**。
   改成「跳洞」（arm64 跳 4 字节、x86 跳 1 字节）后是 **1,305,676** 条指令、12,609 个函数 100% 有指纹。
   洞的个数：Android 15 处、两份 DLL 各 20 处、Linux 0 处。
2. **`&'static str` 常走 (ptr,len) 槽。** 只认"直接指向字符串字节"时全 Linux 只有 2,073 个函数有字符串
   （`lea rdi,[rip+.rodata]` 那种）；把 `.data.rel.ro` 里 8+8 字节的 (ptr,len) 也解出来之后是 **3,568 个**
   （§20.2 的同一条口径）。不解析它会把"字符串特征"整列做成噪声。

### 35.2 判据：指纹算什么，为什么是这几个字段

| 代号 | 字段 | 为什么这么设计 |
|---|---|---|
| **F1** | `size`（字节） | 跨构建最稳的量（真配对 size 比 p05/p95 = **0.86 / 1.07**，见 §41.3），只当**闸门**不当证据 |
| **F2** | `hist`（助记符直方图）+ `hist_hash` | §40.2 实测唯一出力的两项之一；`hist_hash` 是**与 `PYTHONHASHSEED` 无关**的稳定 hash（§39.7 的教训） |
| **F3** | `gram`（助记符**三元组**集合） | 顺序敏感、抗位移差异：`call rel32` 的位移变了但助记符序列不变。**本节最出力的特征**（§41.3） |
| **F4** | `consts`（≥0x100 的立即数，**不含**地址） | 只收"不落在任何节里"的立即数；函数地址**绝不**进常量集（跨构建无意义） |
| **F5** | `callee_sizes` / `callee_bsig`（被调者**多重集**，ICF 归一） | 被调者身份不能存地址（跨构建/跨 ISA 不同），所以存 ① 尺寸多重集（跨 ISA）② 机器码 hash 多重集（同 ISA，**ICF 合并后同一份代码只算一次**） |
| **F6** | `callers` / `caller_sizes` / `caller_bsig`（调用者多重集） | 同上；调用者是"结构角色"最直接的东西 |
| **F7** | `strs`（引用到的字符串内容 sha1 前 8 位） | 内容 hash 而不是偏移，所以跨构建可比 |
| **F8** | `bsig`（自身机器码 sha1 前 12 位） | ICF 判据：**同一份机器码**才算归并；"形状相同"不算（§41.6 第 3 条） |
| **F9** | `cls`（14 类归一（含 `other`）：move/load/store/arith/bit/cmp/sel/jump/call/ret/stack/fpu/pad） | 唯一为**跨 ISA**准备的字段（arm64 的 `ldr` ↔ x86 的 `mov r,[m]`）；评分时必须**不看**偏移/立即数 |

**打分器只用上表**，不读名字、不读地址、不读 `Location` 键；**候选池 = 另一份构建的整个函数宇宙**
（Windows 51,985 个，不是"有名函数池"——12,736 个的对应体大多本身也没名字，池子只放有名的就是作弊）。

> **审计 159 发现原值「不含 Location」在特征层面不成立，实测 `strs`(F7) 就是 Location 的文件名**：
> `panic::Location` 结构体前 16 B 是 `(&'static str, len)`，被扫描器当「(ptr,len) 槽」解成文件名字符串 ⇒
> 正对照 **348/348（100%）** 的函数 `strs` 里都含自己的 `src/*.rs`，而 12,736 目标集合只有 **1,500/12,736（11.8%）**。
> 权重清零重跑（同真值同分半）：留出半 top-1 **84.2% → 73.3%**（`strs`+`consts` 都清零 71.5%）；
> 另：`hist`(F2) 权重在最终配置下**可清零而两半一个 top-1 都不变**（0/331 翻转）。见 `docs/159` §3.2。

### 35.3 正对照（硬门槛）：同 ISA 留出半 **84.2% / 93.3%**，跨 ISA **6.5%**

**真值怎么来的（这一节比准确率重要）：**

1. 测试集 = §40.1 的 **348**（三份夹具的名字并集 ∩ 527）。本工具复算：
   `named_linux()` = **354** 个地址，∩527 后 = **348**（差的那 6 个正是 §40.1 列出的"有名字但不在 527 内"）。
2. Linux↔Windows 的真值 = `fixtures/name_to_addr_v2.tsv` evidence 的 `win_addr=`（**只读不重算**，301 个 1:1 对）
   **∪** 本工具的严格 `Location` join（333 个）。两者重叠的 **295 个地址 100% 一致、冲突 0 个**。
   ⇒ 并集 **331 个**进测试集。
3. **第一版真值是错的，必须记下来**：当时用"把函数引用的 `Location` 键集合的**并**去对侧查"
   这种松 join，它把 Linux `0x3a2e10`（**9,936 B / 1,848 条指令**）对到 Windows 一个 **1,019 B**
   的函数上（**体量比 0.10** —— 肉眼就知道错了）。松 join 会被"很多函数共享的 `Location`"带偏。
   改成 **每个键两侧各唯一 + 正反向都必须指向同一个函数** 之后，真配对 size 比收紧到
   p05/p95 = **0.86 / 1.07**（松 join 那版里 9.75 这种比值也有），准确率随之从
   **16.7% 跳到 89.8%** —— **16.7% 那个数一度被我当成"跨构建不可匹配"，其实是真值坏**。
   这条是本节最该记住的教训：**正对照的真值错了，后面所有结论都是错的**。
4. 分半：按 **Linux 地址** `sha1` 奇偶（地址不在特征里，所以与"难不难"无关；不用名字分半是因为
   名字键有两套 `parse_name`，§40.3 的教训）。调参半 166 / 留出半 165。

| 目标 → 候选池 | 半 | n | **top-1** | **top-5** | 候选数中位（闸门 [0.7,1.4]） | 随机基线 |
|---|---|---|---|---|---|---|
| Linux → **Windows 2.0.60**（同 ISA） | 调参半 | 166 | 89.8% | 95.8% | 6,806 | 0.015% |
| Linux → **Windows 2.0.60**（同 ISA） | **留出半** | 165 | **84.2%** | **93.3%** | 6,310 | 0.016% |
| Linux → Android 2.0.60（跨 ISA） | 调参半 | 167 | 10.2% | 21.0% | 1,862 | 0.054% |
| Linux → Android 2.0.60（跨 ISA） | **留出半** | 169 | **6.5%** | 17.2% | 1,519 | 0.066% |
| Linux → Windows early2026（同 ISA，**不同版本**） | 留出半 | 119 | 52.9% | 66.4% | 6,477 | 0.015% |

**候选窗口的召回代价**（调参半，真配对落在 `size` 比窗口内的比例）：

| 窗口 | 保住真配对（同 ISA） | 候选数中位 | 保住真配对（跨 ISA） |
|---|---|---|---|
| [0.33, 3.00] | 165/166 = 99.4% | 25,147 | 166/167 = 99.4% |
| [0.50, 2.00] | 164/166 = 98.8% | 16,717 | 165/167 = 98.8% |
| **[0.70, 1.40]**（本节用） | **161/166 = 97.0%** | **6,806** | 161/167 = 96.4% |
| [0.80, 1.25] | 159/166 = 95.8% | 3,372 | 140/167 = 83.8% |

**特征消融（只在调参半，窗口 [0.70,1.40]）—— 这张表决定了最终权重：**

| 配置 | top-1 | | 配置 | top-1 |
|---|---|---|---|---|
| size + hist | 23.5% | | size + hist + **cbsig**（被调者机器码多重集） | 23.5% |
| size + **gram** | **74.7%** | | size + hist + **rbsig**（调用者机器码多重集） | 23.5% |
| size + hist + gram | 75.3% | | 全特征（9 项） | 85.5% |
| size + hist + consts | 23.5% | | 全特征 + 计数项 | 77.7% |
| size + hist + **callee**（被调者尺寸多重集） | **3.0%** | | **gram 主导（最终采用）** | **89.8%** |
| size + hist + **caller**（调用者尺寸多重集） | **41.0%** | | gram + 邻域 | 78.3% |
| size + hist + strs | **9.6%**（见下） | | 去掉 callee 的全特征 | 81.3% |

**`strs` 加成反而把 top-1 从 23.5% 拉到 9.6%，这是个真测量不是 bug**：真配对引用的字符串集合
Jaccard **中位只有 0.500**（166/166 对两侧都有字符串，不存在"一侧空"的稀释问题）——
跨构建的字符串引用本身漂移很大（`.rodata` 布局不同、`format!` 展开不同、`&str` 前缀相同但拼接不同），
于是"字符串恰好一致"的诱饵被抬到了真身前面。所以它在最终权重里只留 0.2（配合 `gram` 时不伤害精度）。

最终权重（`W_SAME`）：`size 1.0 / hist 0.5 / gram 2.0 / consts 0.5 / caller 0.3 / strs 0.2`，
被调者、调用者机器码多重集、计数项的权重**都是 0**（理由见 §41.6）。

### 35.4 档位门槛：调参半定、留出半报数

分数门槛（同 ISA，调参半 → 留出半的 top-1 精度）：

| 门槛 | 调参半 | 留出半 | | 门槛 | 调参半 | 留出半 |
|---|---|---|---|---|---|---|
| ≥0.40 | 89.8% (166) | 84.8% (164) | | ≥0.70 | 94.4% (125) | 90.0% (120) |
| ≥0.50 | 90.3% (165) | 85.7% (161) | | ≥0.75 | 93.6% (94) | 90.4% (94) |
| ≥0.55 | 90.7% (161) | 86.2% (159) | | ≥0.80 | 96.2% (53) | 95.8% (48) |
| ≥0.60 | 91.8% (158) | 86.1% (158) | | ≥0.85 | 100% (17) | 88.2% (17) |
| ≥0.65 | 94.5% (146) | 89.1% (147) | | ≥0.90 | 100% (4) | 75.0% (4) |

**真正起作用的是 `margin`（top-1 与 top-2 的分差），不是分数本身** —— 这是本节最实用的一条：

| 规则 | 调参半 | 留出半 |
|---|---|---|
| R1 `score≥0.80` 且 `margin≥0.05` | 100% (36) | 97.1% (34) |
| R3 `score≥0.80` 且 `size≥256` | 100% (51) | 95.7% (46) |
| **R4 `score≥0.65` 且 `margin≥0.05`**（**高置信同源**） | **100% (106)** | **98.0% (98)** |
| R7 `score≥0.50` 且 `margin≥0.05`（**推断同源**） | 98.3% (118) | 96.2% (106) |
| R8 `score≥0.50`（不看 margin） | 90.3% (165) | 85.7% (161) |

R4 与 R7 之差那 8 行（留出半）的精度约 **74%**，所以两档分开写。**单射闸门**
（一个 Windows 函数被多个目标认领就降档）在正对照两半里**各去掉 0 行**，精度不变 ——
但这只是因为正对照的真值本来就是 1:1（调参半 106→106、留出半 98→98），
**不是因为它被验证过**（下一条就是它没生效时的后果）。
最终档位（`tools/callgraph_fingerprint.py::tier_from_score` + `apply_gates`）：

> **审计 159 §6 的硬错误（2026-11-08 已修）：单射闸门原来只在「分片内」生效。**
> `cmd_run()` 的 `claim` 计数只数**本分片**的行，而 `run` 用 `targets[i::8]` **按地址跨步**切分 ⇒
> 结构孪生通常地址相邻，几乎必然落在不同片，闸门永远看不见它们。实测后果：
> **2,803 行只落在 2,772 个 Windows 目标上，25 个目标被 ≥2 行认领（涉及 56 行）**，
> 而且这 56 行都没有 `multi-claim` 注记；见证 Windows `0x1800ead70` 被 **5** 个 Linux 函数认领，
> 工具降了同片的 2 个、漏了跨片的 `0x6baef0`（仍是高置信）⇒ 56 行里至少 **31 行必错**。
> **修法**：把判档 + 单射挪到 `merge`，对**全部 12,736 行**重算一次（`apply_gates()`，`run`/`merge`
> 共用同一个函数）；目标被 ≥2 行认领时这些行**全部**降到「推断同源」+ `note=multi-claim`，
> **不采用"只留分数最高的一个"** —— 近孪生（§35.7 第 6 条的 `update_names_button_texture`）的
> 分数差落在噪声内，"取最高分"等于在一组没有信息的候选里挑一个冒充高置信。
> 重出的结果与逐档对比见 §35.5.1；`calib` 另加了合成**负对照**，证明闸门真会开火
> （`docs/159` §6 建议的那一条）。

| 档位 | 判据 | 实测精度（留出半） |
|---|---|---|
| **高置信同源** | `score≥0.65` 且 `margin≥0.05` 且**全局**单射（`merge` 重算） | **98.0%**（n=98/165）—— ⚠ **只对「已定名 348」这一批成立**；对 12,736 只能说 **≈80–95%、逐条待核**（§35.0 口径块、§35.7、`docs/159` §3/§9） |
| **推断同源** | `score≥0.50` 且 `margin≥0.05`（不含上一档） | ≈74% |
| **歧义** | `score≥0.50` 但 `margin<0.05`（打平/贴太近） | 不当身份用 |
| **未知** | `score<0.50` 或窗口里没有候选 | — |

### 35.5 结果：12,736 个里做了 12,736 个

**全量做完了，没有分批截断**（8 片并行，每片 1,592 个，合计 12,736）。
下表是**单射修好之后重出的夹具**（§35.5.1 给了与修前的逐档对比）：

| 档位 | 个数 | 占 12,736 | 其中 `size<256 B`（**精度未校准**，见 §41.7） |
|---|---|---|---|
| **高置信同源** | **2,745** | 21.6% | 610 |
| 推断同源 | 528 | 4.1% | — |
| 歧义（打平） | 5,591 | 43.9% | — |
| **未知** | **3,872** | 30.4% | — |

⇒ **能给出「高置信同源」的 2,745 个（21.6%）；只能写「未知」的 3,872 个（30.4%）**；
剩下 6,119 个（推断 528 + 歧义 5,591）是"有候选但不够格"的中间态，**不当身份用**。

### 35.5.1 单射修好之后：2,803 → 2,745，必错的那批去哪了

`docs/159` §6 的硬错误修完（判档 + 单射挪到 `merge` 全局重算，规则见 §35.4），
`run` 的 8 个分片原样重跑、`merge` 重出 `fixtures/callgraph_fingerprint.tsv`。
**数字变小是正常的**：修掉的是"同一件事被算成两个高置信好结果"。

| 项 | 修前（片内闸门） | 修后（全局闸门） |
|---|---|---|
| 高置信同源 | 2,803 | **2,745**（−58，−2.1%） |
| 推断同源 | 468 | **528**（+60） |
| 歧义 | 5,592 | 5,591（−1） |
| 未知 | 3,873 | 3,872（−1） |
| 合计 | 12,736 | 12,736 |
| 高置信覆盖的不同 Windows 目标 | 2,772（**2,803 行挤在 2,772 个目标上**） | **2,745 个 = 行数（1:1）** |
| 被 ≥2 行认领的目标组 / 行数 | 25 组 / **56 行**（**且无 `multi-claim` 注记**） | **0 组 / 0 行** |
| `note=multi-claim` 行数 | 16（全是片内撞的） | **75**（跨片共靶全部在内） |
| 这批行里"必错"的下界 | **≥31 行**（56 − 25，§35.4 引审计原话） | **0**（这 75 行已不再是高置信） |

**那 25 组 / 56 行现在怎么样了 —— 全部降档，一行没删：**

* 全局重算看到的共靶比旧夹具更多：**30 组 / 75 行**（旧夹具只看得见 25 组 / 56 行）。
  多出来的 5 组里，**3 组**是"组内只有 1 行是高置信、其余认领者早被**同片**闸门降过档"，
  另 **2 组**更彻底 —— 组内高置信 0 行（全被同片降光了）：旧夹具上完全看不出矛盾，全局一眼就看出来。
  组内行数分布：2 行 ×23 组、3 行 ×4、4 行 ×2、**9 行 ×1**。
* 这 75 行**全部**变成「推断同源」+ `note=multi-claim`：其中 **59 行**原本是「高置信」，
  另 **16 行**原本就被同片闸门降过档（保留原状）。
  两个可核的见证：① 审计 159 §6 点名的 Windows `0x1800ead70`，3 个够分的认领者
  （`0x6dda80` 0.7471 / `0x6d55e0` 0.7264 / `0x6baef0` 0.6855）**全部**降到推断同源 + `multi-claim`；
  ② 最大的共靶组是 **9 个 Linux 函数抢 Windows `0x1804444c0`**（`size=32`、`score=0.6845`、
  `margin=0.1101` 九个**一模一样** ⇒ 典型的 32 B ICF 体组；另有 1 个 0.6008 的也在候选里），
  这 9 个全部降为推断同源。
* 2,803 − 59 = 2,744，再加 **+1** 才是 2,745：档位现在按**发布精度**（夹具里的 4 位小数）判，
  旧版按内存里的全精度浮点判 ⇒ 12,736 行里有 **3 行**的真值正好在门槛内侧、打印出来却被四舍五入
  到门槛上（`0x7aa8c0`、`0x921fa0` 的 `margin` 印成 `0.0500`，`0x936d40` 的 `score` 印成 `0.5000`），
  三行各升一档，净 **+1**。这样**夹具用自己的 `score`/`margin` 列
  就能复算出自己的档位**（实测 0 行不一致），不依赖"跑的时候内存里是多少"。
* 复核方式（任何人不看代码也能查）：高置信行的 `match_addr` 互不相同（2,745 行 / 2,745 个目标，
  `sort | uniq -d` 为空），且每一行 `size<256 B` 必带 `small-uncalibrated`、
  每一行 `multi-claim` 必是「推断同源」。

结构角色（这是本节给的第二个东西，与匹配无关）：12,736 个里 **6,711 个有观测到的被调者**、
**5,585 个有观测到的调用者**、**3,041 个引用了字符串**；**4,475 个两边都没有**
（0 个被调者、0 个调用者 ⇒ 调用图对它们没有任何信息，见 §41.6 第 1 条）。

夹具 `fixtures/callgraph_fingerprint.tsv` 的 16 列：

```
linux_addr  size  ninsn  hist_hash  ncallees  ncallers  nstrs
match_build  match_addr  score  margin  tier  android_cand_addr  android_cand_score  bsig  note
```

* `hist_hash` = 助记符直方图的稳定 hash（`PYTHONHASHSEED` 无关，可跨 run 比对）；
* `match_build` 恒为 `win`（同 ISA 那一份）；`match_addr` 是 **Windows 2.0.60 的 VA**（ImageBase 0x180000000）；
* `android_cand_addr` / `android_cand_score` 是跨 ISA 的候选，**实测 top-1 只有 6.5%，不是证据**，
  只作"结构角色"参考（列名里带 `cand` 就是为了防止有人把它当结论）；
* `note` = `small-uncalibrated`（体量 <256 B，精度未校准）/ `multi-claim`（被多目标认领，已降档）。

### 35.6 四个负结论（不写清楚，后人会重走一遍）

1. **调用图指纹在这批数据上贡献 0 —— 与上一轮（`sym_match` §"为什么调用图几乎没用"）一致，这次是
   在 52k 候选池上复验的。** 证据：`size+hist+cbsig` = 23.5%、`size+hist+rbsig` = 23.5%，
   与"只用 `size+hist`"的 23.5% **一模一样**；被调者**尺寸**多重集更糟，单独加成会把 top-1
   从 23.5% 打到 **3.0%**。
   **为什么是 0**：Linux 侧被调者的"尺寸"由 Ghidra 起点表反推（下一个起点减本起点，含尾部 padding），
   Windows 侧由 `.pdata` 的精确 `begin/end` 给出 —— 两套边界口径系统性不同，真配对的
   `callee_sizes` 相似度**中位数是 0.000**（§41.2 F5）。调用图只在**同一份构建内部**有意义。
2. **跨 ISA（Linux↔Android）不可匹配。** 最好的一组（`size + caller + ncall`）留出半 **6.5%**，
   把 §41.2 的 14 类归一 `cls` 直方图加进去也是 0.6%~10%。原因：arm64 与 x86 的指令构成、
   内联决策、`ninsn` 与 `size` 的比例都不同，`size` 比虽然还贴着 1（p05/p95 = 0.78/1.28），
   但"形状"这一层信息不足以在 12,609 个候选里定位。⇒ **Android 那份构建只能用于"角色"和
   `Location` 桥，不能用于结构配对。**
3. **"形状相同 ≠ ICF"。** 本节用**机器码 hash** 判 ICF：Linux 13,263 个函数里只有
   **119 组 / 548 个地址**是真·逐字节相同（Windows 128 组 / 452 个）。而 348 个正对照里
   `bsig` 相同的是 **0/166**（调参半），`seq_hash`（整条助记符序列）相同的也是 **0/166**
   —— ⇒ 跨构建**永远不要**指望"序列一模一样"，能用的只有三元组草图（真配对 gram Jaccard 中位 **0.724**）。
4. **精确助记符序列（`seq_hash`）作为特征完全无效**（0/166 命中，已在 `W_SAME` 里弃用），
   但它的**三元组集合**（`gram`）是本节最出力的特征（74.7% vs 23.5%）。这两件事必须一起说，
   否则会被误读成"序列有用"。

### 35.7 诚实边界（这条比覆盖率重要）

1. **它不给出名字。** 产出是"同源候选 + 结构角色"，名字仍然只有 §37/§39/§40 的那 348 个。
2. **98.0% 是对"已定名的 348 个"测的，不是对 12,736 测的。** 两者的体量分布差得很远：

   | `size` 桶 | ~~正对照 348 个里~~ **调参半 166 个里**（审计 159：原表头写错了口径，0+11+98+40+17=166；全部 331 个是 **0/27/185/90/29**） | 12,736 个里 | 该桶实测 top-1（调参半） |
   |---|---|---|---|
   | <64 B | 0 | **2,948（23.1%）** | **无样本** |
   | 64–256 B | 11（6.6%） | **4,546（35.7%）** | 63.6%（n=11，样本太少） |
   | 256–1,024 B | 98（59.1%） | 4,110（32.3%） | 92.9% |
   | 1,024–4,096 B | 40（24.1%） | 1,006（7.9%） | 85.0% |
   | ≥4,096 B | 17（10.2%） | 126（1.0%） | 100% |

   ⇒ **12,736 里有 58.8% 的函数在 <256 B 这一档，而这一档的精度没有实测支撑**
   （正对照调参半 11 个样本测出 63.6%，**审计 159 补：留出半 16 个 81.2%，两半合计 20/27 = 74%**）。所以夹具里这 **610** 个"高置信"行都带
   `note=small-uncalibrated`，**要引用它们时必须连这条注记一起引用**。
3. **没有一条能升到「逐位验证」。** 2,745 个高置信里 `bsig` 与被匹配函数相同的是 **0 个**
   —— 跨构建连"同一份机器码"都没有，所以全部是**推断**。
4. **`margin` 门槛会吃掉覆盖率。** R4 在正对照上只覆盖 98/165 = 59% 的**已知有真值**的函数；
   12,736 上的 21.6% 更低，是因为目标集合里小函数更多（见第 2 条），不是"匹配器变差"。
5. **没做的部分**：① Windows `early2026` 的配对只测了准确率（52.9%），**没有**给 12,736 出结果
   —— 它连 80% 都不到（而且它是**另一个版本**，行号/内联都漂了）；② Android 只出了候选列，
   没有参与档位判定；③ 12,736 里的调用者/被调者只做了直接调用（`call rel32` / `bl`），
   **函数指针派发没有追**（`AGENTS.md`「调用图会骗人」那条），所以"有调用者 5,585 个"是**下界**。
6. **已知失败模式：同模块近孪生会被判成高置信（`docs/159` §7）。** 同一控制器里两个只差**一个
   贴图名常量 / 一个关卡号**的兄弟函数，`hist`/`gram` 完全一样、`strs` 几乎一样，
   `consts` 又不收 <0x100 的立即数 ⇒ 分数几乎相同，只能靠 `margin` 撞运气。
   实测的错例：`update_names_button_texture` 被判成 `update_view_function_button_texture`
   （得分仍是**高置信**）；`setup_level_17` ↔ `setup_level_19` 则被 `margin` 降到了歧义。
   ⇒ **引用高置信行时，若目标落在"同模块成对/成串的兄弟函数"里，必须逐条核名字**（这正是
   §35.0 口径里"逐条待核"的含义）。
7. **`report` 打印的"348 个体量中位 624 B"是 354 个上的数**（`docs/159` §0 引的就是它）。
   本版把标签口径改对了：`named_linux()` = 354（三份夹具名字并集），正对照用的 348 是再 ∩527，
   其体量中位是 **640 B**。两个数都在"已定名那一批"里，不影响任何结论。

### 35.8 与既有章节的关系（不改正文，登记在这里）

* §37 / §39 / §40 的 348 / 527 / 179 三个数本节**复算过且一致**（348 = 354 ∩ 527；
  527 = 引用 `src/*.rs` `Location` 的函数）。本节不新增名字，也不改它们的结论。
* §40.3 的 **98.8%/100%** 与本节 **84.2%** 不矛盾，但**不能混着引用**：那是两个不同的实验。
  §40 的候选池是"**同 `Location` 键 + 有名**"的几选一（`unnamed_attrib.Matcher.candidates`），
  本节是**整份构建 51,985 个函数**里排第一（窗口内候选数中位 6,806）。池子大小差三个数量级。
* `.agent-work/` 下的缓存（`.agent-work/cfgfp/*.pkl`）**不是证据**，删了用 `build` 重扫即有。

### 35.9 一条命令复算

```bash
$PY tools/callgraph_fingerprint.py build            # 扫四份构建（约 2.5 min，缓存 .agent-work/cfgfp）
$PY tools/callgraph_fingerprint.py verify           # 正对照：真值来源 + 分半 top-1/top-5（1 min）
$PY tools/callgraph_fingerprint.py tune2            # 窗口召回代价 + 12 组特征配置（**只读调参半**）
$PY tools/callgraph_fingerprint.py calib            # 分数门槛 / 档位规则 / 体量分桶 / 单射闸门（含合成负对照）
# 12,736 分 8 片（每片约 8 min，单进程约 1.2 GB 内存 ⇒ 一次跑 4 片比较稳）
for i in 0 1 2 3 4 5 6 7; do
  $PY tools/callgraph_fingerprint.py run --shard $i/8 --out .agent-work/s$i.tsv
done
$PY tools/callgraph_fingerprint.py merge .agent-work/s?.tsv   # 合并 → 夹具（**判档 + 全局单射闸门在这里重算**）
$PY tools/callgraph_fingerprint.py report           # 指纹库规模 + 目标集合体量分布 + 档位分布
$PY tools/callgraph_fingerprint.py probe 0x3a2e10   # 单函数：四份构建的指纹并排看
```

`$PY` = 带 capstone 5.x 的 Python（本机是 `<WS>/.venv/bin/python`；**只有 `build` 需要 capstone**，
其余子命令只读缓存、标准库即可）。缓存键带 `CACHE_VERSION=cfgfp-v4`。

> **审计 159 §1 的复算命令是坏的（2026-11-08 已修）**：`win` / `win_old` / `FUNCSTARTS` 三个路径
> 写死在已经不存在的目录里（`.reference/windows/graphwarrust-*.dll`、
> `.reference/upstream/analysis/funcstarts_ghidra_va.pkl`），`load_fp()` 直接 `T.die`；
> `probe` 另有一个 `KeyError: 'locs'`（读了不存在的键）也必崩。现在：
> ① 三个路径走 `_first_existing()`（环境变量 `GW2_WIN_DLL` / `GW2_WIN_OLD_DLL` / `GW2_FUNCSTARTS`
> → `.reference/steam/...` → 旧路径），函数起点表直接用 `sym_match._find_funcstarts()` 的上溯；
> ② `probe` 改读 `locrefs`。⇒ **上面这段现在原样可跑**（本机实测：`build` / `verify` / `tune2` /
> `calib` / `report` / `probe` + 8 片 `run` + `merge` 全绿，数字逐位复现）。
> 分片那一步的 `tier` 列**只是临时的**：`--shard`/`--limit` 只看得见自己那批目标，
> 所以单射闸门在 `run` 里必然只能算本片；**落地夹具一律走 `merge`**，它会对全量重算档位与闸门。
改了特征口径就必须抬版本号，否则会静默吃旧缓存（`sym_match` §36.0 踩过这个坑）。
**复跑是确定的**：本轮 8 片全部重跑后，新夹具与旧夹具逐行对比，除 `tier`/`note` 两列外
**14 列逐字节一致**（`docs/159` §6.1）；`merge` 连跑两次输出也逐字节相同
⇒ 结果不随 `PYTHONHASHSEED` 变（§39.7 的坑）。
**本节没有一个数字来自"跑测试"**：`cargo test` 是回归保护，不是复原证据（`AGENTS.md` §0.2）。

## 36. `GraphwarGameClient::process` 的三个邻居：进程链的 x86 锚点表（2026-11，任务 43）

§21 的 `graphwar_game_client` 表里 `process` 早就写着 `0x518400`，但只标了"唯一锚"、没有语义；
第十轮 43 号把它**逐条读通**并补上调用链（详细推导、逐指令与运行时计数见 [`docs/16-client.md` §48](16-client.md)）。
本节只做一件 §21 没做的事：**把这条链上每一环的 x86 地址钉在一张表里**，免得下次再从
`call *GOT` 槽反推。

### 36.1 链上七个地址（x86 ELF 虚地址；Ghidra/REA = +`0x100000`）

| 环节 | 类型 / 方法 | x86 | 尺寸来源 | 名字怎么钉住的 |
|---|---|---|---|---|
| 引擎虚函数派发 | `GraphwarRootNode` 的 `__virtual_call` | `0x40cdc0` | FDE `0x40cdc0..0x40cede` | 体内 `movabs 0x737365636f72705f` = `"_process"` + 哈希 `0x1647d661`（`0x40ce62`/`0x40ce74`）；`"_ready"` 分支给 `0x3aaa60` |
| `_process` 的 gdext 入口 shim | —（生成代码） | `0x3aa9a0` | FDE `0x3aa9a0..0x3aaa56` | 体内两个字面量：`"GraphwarRootNode"`(`0x19e490`,len `0x10`)、`"process"`(`0x1bd9ba`,len 7)；`0x3aaa17 call 0x42a8a0` |
| typed vcall（取 delta、绑定实例） | —（生成代码） | `0x42a8a0` | — | `0x42a8bf movsd xmm0,[rax]`（delta）+ panic 串带类名；`0x42a914 call *GOT 0x9ba898` |
| 引擎 `_process` 的落地 | `GraphwarRootNode::process`（= `<… as INode>::process`） | `0x3a5760` | FDE `0x3a5760..0x3aa696` | 56 个 `panic::Location` 锚（`fixtures/name_to_addr.tsv` 第 38 行）；arm64 `<… as INode>::process` `0x39c8a0`（16984 B） |
| 后端每帧 | `GraphwarBackend::process` | `0x515dc0` | FDE `0x515dc0..0x516369` | `fixtures/name_to_addr.tsv`（`src/graphwar_backend.rs:79:51`）；arm64 `0x407ef0` |
| **客户端每帧** | **`GraphwarGameClient::process`** | **`0x518400`** | FDE `0x518400..0x518527`（`0x127` B） | 3/3 个双向唯一 Location 锚（`src/graphwar_game_client.rs:130:41 / 130:86 / 132:28`）＋ Windows 同 ISA 同名同形 `0x133390` ＋ arm64 `0x40ac00` |
| 事件泵 | `GraphwarEventHandler::process_events` | `0x3d3630` | — | `fixtures/name_to_addr.tsv`（`src/graphwar_event_handler.rs:467`）；arm64 `0x486970`（114 个 `bl` 站点） |

### 36.2 这张表能直接回答的三个问题

1. **`process` 是"每帧"还是"定时"**：`0x518400` 每帧被调**一次**（`0x515df8` 唯一入口），
   体内是 **1/60 s 定步追赶循环**（`0x519450 get_time_to_tick` 的 `nanos==1e9` 哨兵 → `0x518240 tick`
   → `0x3d3630 process_events`）。⇒ **泵频率 = 60 Hz，不由帧率决定**（`docs/16` §48.4 有 frida 计数）。
2. **`0x46aa50` 是谁**：`get_module::<GraphwarTickHandler>`（`src/graphwar_module_container.rs` 的槽表），
   `process` 就是靠它拿 tick handler（`src/graphwar_tick_handler.rs` 已有 `get_time_to_tick()`）。
3. **谁的 `process` 走 GOT**：`0x515dc0` 用的槽是 `0x9bc468`、`0x3a5760` 用的是 `0x9ba938`、
   `0x3a5760` 自身被 `0x9ba898` 取址 —— 这三条都是"地址被当值用"（虚表/gdext 派发），
   所以 `tools/addr_probe.py callers` 的 `call rel32` 一栏是 **0**，别据此写"没人调"。

### 36.3 口径提醒（同 `AGENTS.md` §0.2）

本节与 `docs/16` §48 里的**全部地址**都能用 `tools/addr_probe.py` / `tools/arm64_sym.py` /
`tools/pe_dis.py`（本轮新增，Windows PE 侧）原地复算；`cargo test` 的 1631 条只是回归保护，
**不构成**这些结论的证据。
