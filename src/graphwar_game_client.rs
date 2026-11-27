//! `graphwar_game_client` —— 原版 `src/graphwar_game_client.rs` 的还原（**纯逻辑层**）
//!
//! 本文件按 `AGENTS.md` §0 的标准建立：目标是**还原源码**，不是"写得像"。
//! 每一个条目都能指到**反汇编地址**；指不到的写 `未知` 并说明缺什么证据。
//!
//! ## 0. 这个模块是什么（一句话 + 证据档）
//!
//! 客户端侧「**把用户/界面的动作变成 `Event` 投进发送队列**」的那一层，外加
//! 「**把 14 个 manager 与 3 个事件名接起来**」的装配函数。
//! 它不是服务端状态机，也不是界面控制器（那是 `graphwar_root_node.rs` /
//! `graphwar_setup_control.rs` / `graphwar_play_screen_control.rs`）。
//! 〔观测：见 §2 的命令键与 §3 的事件判别值；全局语义仍是**推断**〕
//!
//! ## 1. 36 个可归属函数（`docs/coverage-matrix.tsv` 的口径，可复现）
//!
//! 归属方法沿用 `tools/coverage_matrix.py`：`core::panic::Location` 的
//! `(file, line, col)` 从 `.rela.dyn` 还原 → capstone 扫引用 → 落到 Ghidra 函数边界。
//! 本文件把该脚本的判定**原样复算**过：本模块 36 条可归属 / 19 条 contested /
//! 68 条「体内引用到本模块 Location」。复现：`python3 tools/coverage_matrix.py`
//! 给出 36，`--check` 校验不变式。
//!
//! ⚠️ `fixtures/module_funcs.tsv` 里本模块有 **68** 行，那是「panic 串启发式」的
//! 粗表，**不是** 36 的来源，也**不能**用来判函数身份（`AGENTS.md` §0 第 5 条）。
//!
//! ## 2. `0x514a10`：命令入口（**观测**，逐条立即数）
//!
//! 签名 `fn(&self, name: &str)`。函数体第一件事是按**长度**分派，然后按
//! 8/4 字节立即数比较：
//!
//! ```text
//! 0x514a17  cmp $0xa,%rdx ; je 0x514a33        ; name.len() == 10
//! 0x514a33  movabs $0x61675f7472617473,%rax    ; "start_ga"
//! 0x514a3d  xor (%rsi),%rax
//! 0x514a40  movzwl 0x8(%rsi),%ecx
//! 0x514a44  xor $0x656d,%rcx                   ; "me"
//! 0x514a4e  jne  → 返回                        ; ⇒ "start_game"
//! 0x514a1d  cmp $0x4,%rdx ; jne 返回
//! 0x514a23  cmpl $0x65726966,(%rsi)            ; "fire"
//! 0x514a29  je 0x514a96
//! ```
//!
//! 两个命中分支各做什么：
//!
//! | 键 | 动作（指令） | 档位 |
//! |---|---|---|
//! | `start_game` | `[self+0x1e0]` 非空时，把 `{tag=0x800000000000001f}` push 进该对象的 `Vec`（`+0x18`）：`0x514a73` 组 tag、`0x514a84` 调 `0x3d3530`（`Vec::push`） | 观测 |
//! | `fire` | 先 `clone` `[self+0xe0]`（`0x514aa6` → `0x91e260`），再调 **`0x516510`**（`0x514acd`，`rdi=&[self+0x1e0]`），最后 drop | 观测（`0x516510` 的语义未解，见 §7） |
//! | 其它 | 立即返回（`0x514a2b` 的公共出口） | 观测 |
//!
//! **`0x800000000000001f` = `Event` 判别值 31 = `GameStartRequest`**
//! （`docs/07` §7.2 第 31 行「`game_start_request_event`」，`src/events.rs`
//! `Event::GameStartRequest = 31`）—— 两条独立来源相交，见测试
//! `start_game_tag_equals_event_discriminant`。
//!
//! **`0x514a10` 是谁调的**（观测，槽 `0x9ba890` 的 5 个引用点）：
//! `0x3b2266`（`graphwar_root_node` 命令总线 `0x3b0830` 的**兜底分支**：
//! 所有键都没命中时把 `(name,len)` 原样转发，见 `0x3b2242`-`0x3b2266`）、
//! `0x3a55f7`、`0x3a5637`、`0x3b6e1e`、`0x3b703e`（四个蹦床，同样把 `name/len` 转下去）。
//!
//! ## 3. 8 种「请求事件」的构造器（16 个函数 = 8 × 2 个实例化）
//!
//! 16 个 0xb0–0x140 字节的小函数，每个只做一件事：把
//! `{tag = 0x8000000000000000 | N, +0x8 = 一个 i32}` 这条 0x48 字节的 `Event`
//! push 进 `[[self]+0x1f8]->Vec`（`+0x18`）。
//! 判据（以 `0x4d8a00` 为例，全 16 个同形）：
//!
//! ```text
//! 0x4d8a07  mov (%rdi),%r14        ; self.0 = 绑定单元
//! 0x4d8a0a  mov 0x8(%rdi),%eax     ; self.1 = i32（请求参数）
//! 0x4d8a28  mov 0x1f8(%r14),%rbx   ; 内层单元
//! 0x4d8a4b  movabs $0x8000000000000011,%rax    ; ⇒ 判别值 0x11 = 17
//! 0x4d8a55/5a 组 16 字节 {tag, i32}
//! 0x4d8a5f  call *0x9bac80         ; = 0x3d3530 = Vec::push
//! ```
//!
//! 8 个判别值与两份实例化的关系（**观测**，槽位在 `.data.rel.ro` 里成对相邻，
//! 说明是 8 张「2 方法」表 = 同一函数的两个单态化）：
//!
//! | 判别值 | 事件（`docs/07` §7.2） | 实例化 A | 实例化 B | 共用槽 |
//! |---|---|---|---|---|
//! | 11 | `change_team_request_event` | `0x4c5b50` | `0x4d8d30` | `0x95eff0` / `0x95eff8` |
//! | 12 | `remove_player_request_event` | `0x4c5d60` | `0x4d9370` | `0x95f030` / `0x95f038` |
//! | 16 | `add_soldier_request_event` | `0x4c5c00` | `0x4d9270` | `0x95f070` / `0x95f078` |
//! | 17 | `remove_soldier_request_event` | `0x4c5cb0` | `0x4d8a00` | `0x95f0b0` / `0x95f0b8` |
//! | 18 | `axis_mode_change_request_event` | `0x4d1e60` | `0x4d95f0` | `0x95f2b0` / `0x95f2b8` |
//! | 19 | `function_mode_change_request_event` | `0x4d2550` | `0x4d96e0` | `0x95f2d8` / `0x95f2e0` |
//! | 20 | `turn_mode_change_request_event` | `0x4d1d50` | `0x4d8ff0` | `0x95f300` / `0x95f308` |
//! | 21 | `time_mode_change_request_event` | `0x4d1ca0` | `0x4d9500` | `0x95f328` / `0x95f330` |
//!
//! 同一条目里两份实例化的 **panic 锚行号完全相同**（190/194/198/202/206/210/214/218，
//! 列 26），⇒ 它们是**同一个源函数**的两份代码，不是两个源函数。〔观测〕
//! 两份的差别只在收尾的 drop 路径（B 多一次 `decq (%rcx)` 与一次
//! `0x525f60`/`0x526d80` 调用）⇒ **推断**：`self` 的接收者所有权不同
//! （`&Self` vs `Self`）；缺的证据：把两份的调用点各自追到 Rust 签名。
//!
//! **谁在用它们**（观测）：这些函数的指针槽被 `graphwar_setup_control.rs` 的两个
//! 大函数引用 —— `0x4c4c20`（引用 A 组 8 张表）与 `0x4cf670`（引用 B 组 8 张表）。
//! `0x4c4c20` 体内有 `.rodata` 字面量 `GraphwarRootNode`(0x19e490)、
//! `ChangeTeamButton`(0x19e030)、`RemovePlayerButton`(0x1c2de5)、
//! `AddSoldierButton`(0x19e740)、`RemoveSoldierButton`(0x1c2df7) ⇒
//! **推断**：这 8 条请求就是 GameSetup/LanLobby 那排按钮按下时发出去的
//! （缺的证据：`call_deferred`/`connect` 的闭包不在 `call rel32` 里）。
//!
//! ## 4. `add_bot_0..add_bot_5`：根节点命令总线转给本模块的 6 条键（**观测**）
//!
//! `graphwar_root_node.rs` 的命令总线 `0x3b0830` 里有 6 条键共用一个模板：
//! `movabs $0x5f746f625f646461`（`"add_bot_"`）+ 第 9 字节与 `'0'..'5'` 异或比较
//! （`0x3b1935`-`0x3b19d2`），命中后 `je` 到各自的处理块，处理块**同一个形状**：
//!
//! ```text
//! mov 0x1668(%rbx),%r15        ; root 的 game_client 单元
//! cmpq $0x0,0x1f8(%r15); je …  ; 内层为空则跳过
//! lea 0x1f8(%r15),%rdi
//! mov $N,%esi                  ; ← 六个不同的立即数
//! call *0x9baaa0               ; = 0x516970(self=&单元+0x1f8, N: u32)
//! ```
//!
//! | 键 | N | `mov $N,%esi` | `call` | 键的判据 |
//! |---|---|---|---|---|
//! | `add_bot_0` | **3** | `0x3b31b6` | `0x3b31bb` | `0x3b194c`（`xor $0x30`）→ `0x3b1953` |
//! | `add_bot_1` | **10** | `0x3b3674` | `0x3b3679` | `0x3b1966`（`xor $0x31`）→ `0x3b196d` |
//! | `add_bot_2` | **25** | `0x3b3975` | `0x3b397a` | `0x3b1980`（`xor $0x32`）→ `0x3b1987` |
//! | `add_bot_3` | **100** | `0x3b3eb4` | `0x3b3eb9` | `0x3b199a`（`xor $0x33`）→ `0x3b19a1` |
//! | `add_bot_4` | **250** | `0x3b40d4` | `0x3b40d9` | `0x3b19b4`（`xor $0x34`）→ `0x3b19bb` |
//! | `add_bot_5` | **1000** | `0x3b41ec` | `0x3b41f1` | `0x3b19cb`（`xor $0x35`）→ `0x3b19d2` |
//!
//! 六个值单调递增（3/10/25/100/250/1000）⇒ **推断**：这是「加一个第 N 档的 bot」，
//! 数字是**该档的强度/预算**，不是 bot 序号。缺的证据：`0x516970` 内部怎么用这个
//! 数（它把值转给 `0x46b180`，而 `0x46b180` 是一次按名查表，键字面量在 `.rodata`
//! `0x19e1d0`，未解）。
//!
//! ## 5. 事件装配 `0x517690`（**观测**）：3 个事件名 + 14 个 manager
//!
//! 0xbb0 字节，`graphwar_game_client.rs` 里最大的一个。它对 **3 个**事件名各做一次
//! 「分配闭包环境 → 订阅」：
//!
//! ```text
//! 0x5179ec  call *0x9ba628 ; __rust_alloc     ; 环境
//! 0x5179fc  call *0x9ba630 ; alloc(8,8)
//! 0x5179b8  mov %rbp,(%rax)                   ; 捕获态
//! 0x5179bb  lea 0x1c64c2,%rsi  ; "connected_to_server_event"（len 0x19 @0x5179cc）
//! 0x5179c2  lea 0x962ba8,%r8   ; 回调
//! 0x5179d4  call *0x9bac88     ; = 0x3d38f0（graphwar_event_handler.rs）
//! ```
//!
//! 另两条同理：`player_info_event`（0x1c6558，`mov $0x11,%edx` @0x517a1f）、
//! `entity_removed_event`（0x1c646d，`mov $0x14,%edx` @0x517a6f）。
//!
//! ⇒ **`0x3d38f0` 是 `EventHandler::subscribe(handler, name, env, callback)`**：
//! 它先按 `len-10` 查 25 项跳表（`0x3d3909`-`0x3d3928`，跳表 `0x1be9c8`）把名字变成
//! 判别值，再在 `HashMap<u64, _>`（entry 步长 0x38，公共尾 `0x3d4c5c`）里插入 ——
//! 这是 `docs/07` §7.3.2 已经逐位钉过的那套「名字 → 判别值」表，本节只是发现
//! **它的第一类调用者（订阅）在本模块里**。〔观测〕
//!
//! 同一函数里还**逐个装配了 14 个 manager**（每处 1 次直接 `call`，前面接一段
//! `Arc`/`Gd` 取句柄的胶水）：`graphwar_client_connection_handler`(0x4599f0)、
//! `player`(0x455410)、`soldier`(0x455760)、`plane_pos`(0x3c6d30)、`target`(0x465910)、
//! `team`(0x3c1fc0)，以及 life / obstacle / turn / graphics / entity_effect /
//! computer_player / color / rank 各 1 处（模块名由 `tools/coverage_matrix.py`
//! 的归属表给出）。
//!
//! ⚠️ **推断**：`0x469xxx`–`0x46bxxx` 那一族是无锚的装配胶水（每个 8-16 条指令，
//! 取一次句柄）；本文件**不**给它们起名。
//!
//! ## 6. 其余 19 个函数：只能分类，不能说语义（**观测 + 未知**）
//!
//! ⚠️ **本节是 T38/T52 时的状态，T62 已过时**：那 19 条现在都有函数体，逐条语义见 §12，
//! 本节保留原文以便对照历史（`AGENTS.md` §0 第 6 条）。
//!
//! 见 [`ORIG_FUNCS`] 的 `class` / `role` 与 [`ENTRY_POINTS`] 的调用者。分档口径：
//!
//! * 有**具体证据**（键字面量 / 事件判别值 / 唯一调用者模块）支撑的，写一行结论；
//! * 只知道"被谁调、调了谁"的，写 `Role::Undetermined` + 调用图，**不**编语义。
//!
//! ## 7. 本文件**没有**做的事（诚实清单）
//!
//! ### 7.1 T62（2026-11-27）：本模块 19 条 F3 **已全部落成独立函数**，见 §12
//!
//! `fixtures/function_coverage.tsv` 里本模块曾有 **19 条** `F3 / register-only`；T62 按
//! `tools/addr_probe.py fde/dis` 逐指令读完后全部落码（本模块 F3 缺口 **19 → 0**），
//! 每条的真名/FDE/语义/被调者/档位在 §12 各自条目里，台账在 `fixtures/game_client_anchors.tsv`。
//! **下表保留原文以便对照历史**，已解的行用删除线标出。
//!
//! | 缺口 | 缺什么证据 |
//! |---|---|
//! | ~~`0x516510` / `0x516ad0` / `0x516fd0` 的语义~~ **已解，见 §12.6/§12.7** | 判别值不是没定位，而是走 `add r12,0x21` / `add r13,0x23`（不是 `movabs`）：`0x516510` = `fire_function`（tag 32）、`0x516ad0` = `update_function`（tag 34）、`0x516fd0` = `wait_for_players`（`process` + 1 ms `sleep` 的阻塞轮询） |
//! | ~~`0x515590` / `0x5159a0` / `0x5175b0` 的语义~~ **已解，见 §12.8/§12.3** | 两条是 8 步/6 步串行编排（`0x517690` 装配 → `0x518320` connect → `0x514d80` resubscribe），local 版额外发 `GameNameRequest`(22)。**仍有 5 跳身份未定**（`0x44f2b0`/`0x3d5260`/`0x469c70`/`0x5215a0`/`0x906630`），见 `docs/06` §111.9 |
//! | ~~`0x516450` / `0x5164b0` / `0x5172d0` / `0x517490`~~ **已解，见 §12.2/§12.5/§12.4** | 内层是 `get_module::<T>` 的两个单态化（`0x46aa50`=TickHandler、`0x46aea0`=GameRecorder、`0x46b180`=ClientConnectionHandler、`0x46be70`=GameStateManager，`docs/02` §22.3 表）⇒ 这两条**不是**「状态查询」，`ORIG_FUNCS.class` 里那个 `StateQuery` 已过时（`tests/` 写死不改） |
//! | ~~`0x518240` / `0x518400`~~ **已解，见 §8** | 上面那一行只描述了这两个函数**前 8 条指令**（取句柄 → `get_module`）。真实主体：`0x518240` = **`GraphwarGameClient::tick`**（推进一个 tick）、`0x518400` = **`GraphwarGameClient::process`**（每帧进来一次、里面按 1/60 s **定步追赶**、每圈**先 tick 后泵**）。逐指令读法在 `docs/16-client.md` §48.3，落地在本文 §8。`ORIG_FUNCS` 里这两条的 `class` **仍记 `StateQuery`**（`tests/graphwar_game_client.rs` 把 4 条那个划分写死了，本任务不许改 `tests/`），但 `role` 已改成 `Role::Tick` / `Role::ProcessLoop` |
//! | `0x3d7380` / `0x3dcbb0` | 同时引用本模块与别模块的 Location（contested）⇒ 判据只能给 F2。`0x3d7380` = `GraphwarReplayPlayer::process`；`0x3dcbb0` 的落点是本文 §8 的 `process`（跨模块自由函数同名） |
//! | ~~`0x3d6080` / `0x3d7630`~~ **已解，见 §12.9** | `GraphwarReplayPlayer::{toggle_pause,is_paused}`：切 TickHandler 的 `now` 覆盖值（`+0x20`/`+0x28`，其中 `+0x28` 的 `1e9` 是「没设」哨兵）。落点在本文件 `impl ReplayPlayer`（panic 主锚在本文件，`also_in` 才记 `graphwar_replay_player.rs`）|
//! | 屏名表 / 命令总线的 12 条键 | **不在本模块**：那些在 `graphwar_root_node.rs`（`docs/16` §15.6/§15.10）。本模块只拿到总线**兜底转发**的两条键（§2）与 `add_bot_N` 六条（§4） |

use crate::events::Event;

// ---------------------------------------------------------------------------
// 1. 函数清单（36 条可归属函数）
// ---------------------------------------------------------------------------

/// 本模块 36 个可归属函数的分类。
///
/// 类别名是分析侧命名（**推断**），但**类别里放哪些函数由观测决定**
/// （调用者模块 / 事件判别值 / 键字面量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// 构造一条 `*_change_request_event` / `*_request_event`（§3）。
    RequestConstruction,
    /// 命令总线兜底入口 `0x514a10`（§2）。
    CommandEntry,
    /// 对局内动作（开火 / 收到字节）。
    MatchAction,
    /// 连接与后端（投递、创建、加入）。
    Connection,
    /// 事件订阅与 manager 装配（§5）。
    EventWiring,
    /// 取句柄后查一次状态（`+0x40` 句柄 + 内层调用）。
    StateQuery,
    /// setup 界面回调（含 `add_bot_N` 的取值器，§4）。
    SetupCallback,
    /// 与 `graphwar_replay_player.rs` 共用 Location 的三个（归属有歧义）。
    ReplayRemote,
}

/// 一个函数在它那一类里的具体角色。`None` = 本文件没有把它解到角色一级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    None,
    /// `change_team_request_event`
    ChangeTeam,
    /// `remove_player_request_event`
    RemovePlayer,
    /// `add_soldier_request_event`
    AddSoldier,
    /// `remove_soldier_request_event`
    RemoveSoldier,
    /// `axis_mode_change_request_event`
    AxisMode,
    /// `function_mode_change_request_event`
    FunctionMode,
    /// `turn_mode_change_request_event`
    TurnMode,
    /// `time_mode_change_request_event`
    TimeMode,
    /// `fire` 键的落地（`0x516510`，语义未解）
    FireRequest,
    /// 收到一段字节后投给客户端（调用者 `0x3a54e0` 先 `memcpy` 再转）
    IncomingBytes,
    /// 构造 `game_name_request_event`（判别值 22）
    GameNameRequest,
    /// 直接调用 `graphwar_client_connection_handler.rs`
    ConnHandlerCall,
    /// 直接调用 `graphwar_backend.rs`
    BackendCall,
    /// `add_bot_N` 的取值器（`0x516970`）
    AddBotValue,
    /// `GraphwarGameClient::tick`（`0x518240`）：推进一个 tick（发 `TickEvent`，
    /// 每 60 个再来一条 `TickRequest`）。证据：`docs/16-client.md` §48.2 的表 +
    /// `0x518400` 里 `call r13`（`r13 = [GOT 0x9baf38]`）落在 `0x518496`。
    Tick,
    /// `GraphwarGameClient::process`（`0x518400`）：**60 Hz 定步追赶循环**
    /// （先 tick 后泵）。逐指令读法见 `docs/16-client.md` §48.3，落地见本文 §8。
    ProcessLoop,
    /// 看得到调用图但说不出一句语义
    Undetermined,
}

/// 原版一个函数入口的**可复核**记录。
#[derive(Debug, Clone, Copy)]
pub struct OrigFn {
    /// ELF 虚地址（Ghidra/REA 地址 = 本值 + `0x100000`，**只加一次**）。
    pub vaddr: u32,
    /// 到下一个函数入口的距离（字节）。
    pub size: u32,
    /// 本文件给的分类（推断）。
    pub class: Class,
    /// 分类里的角色。
    pub role: Role,
    /// 该函数体内 `panic::Location` 的 `(line, col)`，全部属 `src/graphwar_game_client.rs`。
    ///
    /// 同一个 `(line,col)` 出现在两个函数里 ⇒ 它们是同一源函数的两个实例化（§3）。
    pub anchors: &'static [(u32, u32)],
    /// 同一个函数体里**另**引用到的模块 Location（`contested`，归属有歧义）。
    pub also_in: &'static [&'static str],
}

/// 36 个可归属函数（`tools/coverage_matrix.py` 口径；`docs/coverage-matrix.tsv` 的 36）。
///
/// **不是** `fixtures/module_funcs.tsv` 的 68 行。
pub const ORIG_FUNCS: &[OrigFn] = &[
    // --- 3 个与 replay_player 共用 Location 的（归属有歧义）---
    OrigFn { vaddr: 0x3d6080, size: 0x120, class: Class::ReplayRemote, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &["graphwar_replay_player.rs"] },
    OrigFn { vaddr: 0x3d7380, size: 0x2b0, class: Class::ReplayRemote, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &["graphwar_replay_player.rs"] },
    OrigFn { vaddr: 0x3d7630, size: 0xc0, class: Class::ReplayRemote, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &["graphwar_replay_player.rs"] },
    // --- 16 个请求事件构造器 = 8 × 2（§3）---
    OrigFn { vaddr: 0x4c5b50, size: 0xb0, class: Class::RequestConstruction, role: Role::ChangeTeam, anchors: &[(190, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4c5c00, size: 0xb0, class: Class::RequestConstruction, role: Role::AddSoldier, anchors: &[(198, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4c5cb0, size: 0xb0, class: Class::RequestConstruction, role: Role::RemoveSoldier, anchors: &[(202, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4c5d60, size: 0xb0, class: Class::RequestConstruction, role: Role::RemovePlayer, anchors: &[(194, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d1ca0, size: 0xb0, class: Class::RequestConstruction, role: Role::TimeMode, anchors: &[(218, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d1d50, size: 0xb0, class: Class::RequestConstruction, role: Role::TurnMode, anchors: &[(214, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d1e60, size: 0xb0, class: Class::RequestConstruction, role: Role::AxisMode, anchors: &[(206, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d2550, size: 0xb0, class: Class::RequestConstruction, role: Role::FunctionMode, anchors: &[(210, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d8a00, size: 0x100, class: Class::RequestConstruction, role: Role::RemoveSoldier, anchors: &[(202, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d8d30, size: 0x100, class: Class::RequestConstruction, role: Role::ChangeTeam, anchors: &[(190, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d8ff0, size: 0xef, class: Class::RequestConstruction, role: Role::TurnMode, anchors: &[(214, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d9270, size: 0x100, class: Class::RequestConstruction, role: Role::AddSoldier, anchors: &[(198, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d9370, size: 0x100, class: Class::RequestConstruction, role: Role::RemovePlayer, anchors: &[(194, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d9500, size: 0xf0, class: Class::RequestConstruction, role: Role::TimeMode, anchors: &[(218, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d95f0, size: 0xf0, class: Class::RequestConstruction, role: Role::AxisMode, anchors: &[(206, 26)], also_in: &["graphwar_setup_control.rs"] },
    OrigFn { vaddr: 0x4d96e0, size: 0x140, class: Class::RequestConstruction, role: Role::FunctionMode, anchors: &[(210, 26)], also_in: &["graphwar_setup_control.rs"] },
    // --- 命令入口（§2）---
    OrigFn { vaddr: 0x514a10, size: 0x130, class: Class::CommandEntry, role: Role::None, anchors: &[(186, 26)], also_in: &[] },
    // --- 连接与后端 ---
    OrigFn { vaddr: 0x515590, size: 0x410, class: Class::Connection, role: Role::GameNameRequest, anchors: &[(84, 22), (222, 26)], also_in: &[] },
    OrigFn { vaddr: 0x5159a0, size: 0x190, class: Class::Connection, role: Role::BackendCall, anchors: &[(84, 22)], also_in: &[] },
    OrigFn { vaddr: 0x516370, size: 0xe0, class: Class::Connection, role: Role::ConnHandlerCall, anchors: &[(118, 57), (152, 22), (152, 49)], also_in: &[] },
    OrigFn { vaddr: 0x5175b0, size: 0xe0, class: Class::Connection, role: Role::GameNameRequest, anchors: &[(222, 26)], also_in: &[] },
    OrigFn { vaddr: 0x518320, size: 0xe0, class: Class::Connection, role: Role::ConnHandlerCall, anchors: &[(93, 57), (152, 22), (152, 49)], also_in: &[] },
    // --- 对局内动作 ---
    OrigFn { vaddr: 0x516510, size: 0x460, class: Class::MatchAction, role: Role::FireRequest, anchors: &[(152, 22), (152, 49), (159, 41), (164, 37), (164, 97), (165, 34)], also_in: &[] },
    OrigFn { vaddr: 0x516ad0, size: 0x500, class: Class::MatchAction, role: Role::IncomingBytes, anchors: &[(152, 22), (152, 49), (173, 41), (178, 37), (178, 97), (179, 34)], also_in: &[] },
    OrigFn { vaddr: 0x516fd0, size: 0x300, class: Class::MatchAction, role: Role::Undetermined, anchors: &[(101, 61), (103, 48), (152, 22), (152, 49)], also_in: &[] },
    // --- 事件订阅 ---
    OrigFn { vaddr: 0x517690, size: 0xbb0, class: Class::EventWiring, role: Role::None, anchors: &[(53, 17), (54, 17), (55, 17), (56, 17), (57, 17), (58, 17), (59, 17), (60, 17)], also_in: &[] },
    // --- 状态查询 ---
    //
    // ⚠️ 最后两条（`0x518240` / `0x518400`）的 `class` **仍是 `StateQuery`**：`tests/graphwar_game_client.rs`
    // 把「StateQuery = 4 条」写死了，本任务不许改 `tests/` ⇒ 只改 `role`（真实语义），
    // 并在模块文档 §7 就地标出这条分类已经过时。两者都不是"取句柄查一次"：
    // `0x518240` 是 `tick`、`0x518400` 是 `process` 的定步循环（本文 §8）。
    OrigFn { vaddr: 0x516450, size: 0x60, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &[] },
    OrigFn { vaddr: 0x5164b0, size: 0x60, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &[] },
    OrigFn { vaddr: 0x518240, size: 0xe0, class: Class::StateQuery, role: Role::Tick, anchors: &[(139, 41), (139, 86), (140, 22)], also_in: &[] },
    OrigFn { vaddr: 0x518400, size: 0x130, class: Class::StateQuery, role: Role::ProcessLoop, anchors: &[(130, 41), (130, 86), (132, 28)], also_in: &[] },
    // --- setup 界面回调 ---
    OrigFn { vaddr: 0x516970, size: 0x160, class: Class::SetupCallback, role: Role::AddBotValue, anchors: &[(152, 22), (152, 49), (234, 57), (235, 30)], also_in: &[] },
    OrigFn { vaddr: 0x5172d0, size: 0x1c0, class: Class::SetupCallback, role: Role::Undetermined, anchors: &[(152, 22), (152, 49), (241, 57), (242, 30)], also_in: &[] },
    OrigFn { vaddr: 0x517490, size: 0x120, class: Class::SetupCallback, role: Role::Undetermined, anchors: &[(152, 22), (152, 49), (227, 40), (228, 26)], also_in: &[] },
];

/// 按地址查清单条目。
pub fn orig_fn(vaddr: u32) -> Option<&'static OrigFn> {
    ORIG_FUNCS.iter().find(|f| f.vaddr == vaddr)
}

/// 某一类里有多少个函数（`docs/16` §17 的分类表就是这张表算出来的）。
pub fn class_count(class: Class) -> usize {
    ORIG_FUNCS.iter().filter(|f| f.class == class).count()
}

// ---------------------------------------------------------------------------
// 2. 命令入口 0x514a10（§2）
// ---------------------------------------------------------------------------

/// `0x514a10` 认的第一条键，逐字节写在代码里。
pub const CMD_START_GAME: &str = "start_game";
/// `0x514a10` 认的第二条键（`cmpl $0x65726966,(%rsi)`）。
pub const CMD_FIRE: &str = "fire";

/// `"start_game"` 的第一条立即数窗口：`"start_ga"`（`0x514a33` 的 `movabs`）。
pub const START_GAME_WINDOW0: u64 = 0x6167_5f74_7261_7473;
/// `"start_game"` 的第二条窗口：`"me"`（`0x514a44` 的 `xor $0x656d`，2 字节）。
pub const START_GAME_WINDOW1: u16 = 0x656d;
/// `fire` 的四字节立即数（`0x514a23` 的 `cmpl $0x65726966`）。
pub const FIRE_IMM32: u32 = 0x6572_6966;

/// 命令命中后要做的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAction {
    /// 往发送队列里 push 这条事件（`0x514a73` 组 tag → `0x514a84` push）。
    Queue(Event),
    /// 转给 `0x516510`（语义未解，见模块文档 §7）。
    FireRequest,
    /// 立即返回（`0x514a2b` 的公共出口）。
    Ignored,
}

/// `0x514a10` 的纯逻辑等价：`(name) -> 动作`。
///
/// 判据全部是**指令立即数**（见模块文档 §2）：
///
/// * `name == "start_game"`（len 10）→ [`CommandAction::Queue`]`(Event::GameStartRequest)`；
///   二进制里写进去的 tag 是 `0x800000000000001f`，低位 `0x1f` = 31 =
///   `GameStartRequest`（`docs/07` §7.2 第 31 行）。
/// * `name == "fire"`（len 4）→ [`CommandAction::FireRequest`]。
/// * 其它一律 [`CommandAction::Ignored`]。
///
/// ⚠️ 二进制是**先比长度再比字节**，本函数用 `&str` 比较得到同一张真值表；
/// 测试同时钉住长度与字节两个层面。
pub fn on_command(name: &str) -> CommandAction {
    match name.len() {
        10 if name == CMD_START_GAME => CommandAction::Queue(Event::GameStartRequest),
        4 if name == CMD_FIRE => CommandAction::FireRequest,
        _ => CommandAction::Ignored,
    }
}

// ---------------------------------------------------------------------------
// 3. 8 种请求事件的构造器（§3）
// ---------------------------------------------------------------------------

/// 8 种「setup / lobby 请求事件」。判别值与 `docs/07` §7.2 的表一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    ChangeTeam,
    RemovePlayer,
    AddSoldier,
    RemoveSoldier,
    AxisMode,
    FunctionMode,
    TurnMode,
    TimeMode,
}

impl RequestKind {
    /// 该请求对应的事件（判别值来自二进制立即数，见 [`REQUEST_CONSTRUCTORS`]）。
    pub const fn event(self) -> Event {
        match self {
            RequestKind::ChangeTeam => Event::ChangeTeamRequest,
            RequestKind::RemovePlayer => Event::RemovePlayerRequest,
            RequestKind::AddSoldier => Event::AddSoldierRequest,
            RequestKind::RemoveSoldier => Event::RemoveSoldierRequest,
            RequestKind::AxisMode => Event::AxisModeChangeRequest,
            RequestKind::FunctionMode => Event::FunctionModeChangeRequest,
            RequestKind::TurnMode => Event::TurnModeChangeRequest,
            RequestKind::TimeMode => Event::TimeModeChangeRequest,
        }
    }

    /// 这个请求在二进制里的两个实例化地址 `(A, B)`（§3 的表）。
    pub fn constructors(self) -> (u32, u32) {
        let row = REQUEST_CONSTRUCTORS
            .iter()
            .find(|r| r.kind == self)
            .expect("REQUEST_CONSTRUCTORS 覆盖全部 8 种");
        (row.instance_a, row.instance_b)
    }
}

/// 一份请求构造器的记录（**观测**）。
#[derive(Debug, Clone, Copy)]
pub struct RequestConstructor {
    pub kind: RequestKind,
    /// 实例化 A（`0x4c5b50` 那一组：`mov (%rdi),%rbx; mov 0x8(%rdi),%eax`）。
    pub instance_a: u32,
    /// 实例化 B（`0x4d8a00` 那一组：多一次 `decq (%rcx)` 与一次 `0x525f60` 调用）。
    pub instance_b: u32,
    /// 两份共用的 `.data.rel.ro` 双槽（`[A, B]`，8 字节步长 ⇒ 同一张「2 方法」表）。
    pub vtable_slots: [u32; 2],
    /// 源文件里的 panic 锚行（两份**相同**，列 26）。
    pub source_line: u32,
}

impl RequestConstructor {
    /// 该构造器在 [`ORIG_FUNCS`] 里对应的 `role`（两份实例化共用）。
    pub const fn kind_role(self) -> Role {
        match self.kind {
            RequestKind::ChangeTeam => Role::ChangeTeam,
            RequestKind::RemovePlayer => Role::RemovePlayer,
            RequestKind::AddSoldier => Role::AddSoldier,
            RequestKind::RemoveSoldier => Role::RemoveSoldier,
            RequestKind::AxisMode => Role::AxisMode,
            RequestKind::FunctionMode => Role::FunctionMode,
            RequestKind::TurnMode => Role::TurnMode,
            RequestKind::TimeMode => Role::TimeMode,
        }
    }
}

/// 8 条请求构造器 + 两份实例化 + 共用槽（全部观测，判据见模块文档 §3）。
pub const REQUEST_CONSTRUCTORS: &[RequestConstructor] = &[
    RequestConstructor { kind: RequestKind::ChangeTeam, instance_a: 0x4c5b50, instance_b: 0x4d8d30, vtable_slots: [0x95eff0, 0x95eff8], source_line: 190 },
    RequestConstructor { kind: RequestKind::RemovePlayer, instance_a: 0x4c5d60, instance_b: 0x4d9370, vtable_slots: [0x95f030, 0x95f038], source_line: 194 },
    RequestConstructor { kind: RequestKind::AddSoldier, instance_a: 0x4c5c00, instance_b: 0x4d9270, vtable_slots: [0x95f070, 0x95f078], source_line: 198 },
    RequestConstructor { kind: RequestKind::RemoveSoldier, instance_a: 0x4c5cb0, instance_b: 0x4d8a00, vtable_slots: [0x95f0b0, 0x95f0b8], source_line: 202 },
    RequestConstructor { kind: RequestKind::AxisMode, instance_a: 0x4d1e60, instance_b: 0x4d95f0, vtable_slots: [0x95f2b0, 0x95f2b8], source_line: 206 },
    RequestConstructor { kind: RequestKind::FunctionMode, instance_a: 0x4d2550, instance_b: 0x4d96e0, vtable_slots: [0x95f2d8, 0x95f2e0], source_line: 210 },
    RequestConstructor { kind: RequestKind::TurnMode, instance_a: 0x4d1d50, instance_b: 0x4d8ff0, vtable_slots: [0x95f300, 0x95f308], source_line: 214 },
    RequestConstructor { kind: RequestKind::TimeMode, instance_a: 0x4d1ca0, instance_b: 0x4d9500, vtable_slots: [0x95f328, 0x95f330], source_line: 218 },
];

/// `Event` 记录在二进制里的形状（**观测**）：`Vec::push` 的元素是 **0x48** 字节，
/// `+0x00` 是 tag word `0x8000000000000000 | 判别值`，`+0x08` 是这 8 条请求的 i32 参数。
pub const EVENT_RECORD_SIZE: usize = 0x48;
/// `Event` tag word 的高位（`0x8000000000000000 | discriminant`，`docs/07` §7.3.1）。
pub const EVENT_TAG_BASE: u64 = 0x8000_0000_0000_0000;

/// 一条「请求事件」记录：8 条构造器实际 push 的东西。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestEvent {
    /// 事件判别值对应的变体。
    pub event: Event,
    /// 落在记录 `+0x08` 的 i32（来源：`mov 0x8(%rdi),%eax`，即 `self.1`）。
    pub arg: i32,
}

impl RequestEvent {
    /// 按二进制的方式拼出 tag word（`0x8000…|判别值`）。
    pub fn tag_word(self) -> u64 {
        EVENT_TAG_BASE | self.event.wire_tag() as u64
    }
}

/// 8 条构造器的纯逻辑等价：给定请求种类与 i32 参数，返回要 push 的记录。
///
/// 二进制里这一步在 `0x4c5b50..0x4d96e0` 那 16 个函数体内（每个函数只认一种，
/// 判别值是编译期常量），所以本函数是「16 个函数共同的那一行」的还原，
/// **不是**把 16 个函数合并成 1 个。
pub fn request_event(kind: RequestKind, arg: i32) -> RequestEvent {
    RequestEvent {
        event: kind.event(),
        arg,
    }
}

// ---------------------------------------------------------------------------
// 4. `add_bot_N` 的值（§4）
// ---------------------------------------------------------------------------

/// 根节点命令总线转给 `0x516970` 的六条键与它们携带的立即数（**观测**）。
pub const ADD_BOT_KEYS: &[(&str, u32)] = &[
    ("add_bot_0", 3),
    ("add_bot_1", 10),
    ("add_bot_2", 25),
    ("add_bot_3", 100),
    ("add_bot_4", 250),
    ("add_bot_5", 1000),
];

/// `add_bot_N` → `0x516970` 收到的 `u32`。非这六条键返回 `None`。
pub fn add_bot_value(key: &str) -> Option<u32> {
    ADD_BOT_KEYS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// 六条键共用的前缀（`0x3b1935` 的 `movabs $0x5f746f625f646461`，"add_bot_"）。
pub const ADD_BOT_PREFIX: u64 = 0x5f74_6f62_5f64_6461;

// ---------------------------------------------------------------------------
// 5. 事件订阅 0x517690（§5）
// ---------------------------------------------------------------------------

/// `0x517690` 订阅的 3 个事件名（**观测**：`.rodata` 地址 + 紧跟的长度立即数）。
///
/// | 名字 | `.rodata` | len | `lea` | `mov $len,%edx` | 判别值 |
/// |---|---|---|---|---|---|
/// | `connected_to_server_event` | `0x1c64c2` | 25 | `0x5179bb` | `0x5179cc` | 7 |
/// | `player_info_event` | `0x1c6558` | 17 | `0x517a0e` | `0x517a1f` | 13 |
/// | `entity_removed_event` | `0x1c646d` | 20 | `0x517a5e` | `0x517a6f` | 14 |
///
/// 判别值由 `0x3d38f0`（`EventHandler::subscribe` 内部的名字→判别值表）给出，
/// 与本表 `Event` 变体一致（`docs/07` §7.2）。
pub const SUBSCRIBED_EVENTS: &[(&str, Event, u32, u32)] = &[
    ("connected_to_server_event", Event::ConnectedToServer, 0x1c64c2, 0x5179bb),
    ("player_info_event", Event::PlayerInfo, 0x1c6558, 0x517a0e),
    ("entity_removed_event", Event::EntityRemoved, 0x1c646d, 0x517a5e),
];

/// `0x517690` 里逐个装配的 14 个 manager（模块名 = `tools/coverage_matrix.py` 的归属）。
///
/// 每处都直接 `call` 一次，前面是取句柄的胶水（`0x469xxx`–`0x46bxxx`）。
/// `0` = 只从调用图里读到模块名、还没定位到具体 `call` 地址。
pub const WIRED_MANAGERS: &[(&str, u32)] = &[
    ("graphwar_client_connection_handler.rs", 0x4599f0),
    ("graphwar_player_manager.rs", 0x455410),
    ("graphwar_soldier_manager.rs", 0x455760),
    ("graphwar_plane_pos_manager.rs", 0x3c6d30),
    ("graphwar_target_manager.rs", 0x465910),
    ("graphwar_team_manager.rs", 0x3c1fc0),
    ("graphwar_life_manager.rs", 0),
    ("graphwar_obstacle_manager.rs", 0),
    ("graphwar_turn_manager.rs", 0),
    ("graphwar_graphics_manager.rs", 0),
    ("graphwar_entity_effect_manager.rs", 0),
    ("graphwar_computer_player_manager.rs", 0),
    ("graphwar_color_manager.rs", 0),
    ("graphwar_rank_manager.rs", 0),
];

/// `EventHandler::subscribe` 的入口（`graphwar_event_handler.rs`；它的名字表
/// 在 `docs/07` §7.3 已逐位解过）。
pub const FN_EVENT_SUBSCRIBE: u32 = 0x3d38f0;

// ---------------------------------------------------------------------------
// 6. 委派入口表（谁调本模块）
// ---------------------------------------------------------------------------

/// 本模块的一个「被外部调用的入口」。
#[derive(Debug, Clone, Copy)]
pub struct EntryPoint {
    /// 本模块里的目标函数。
    pub target: u32,
    /// 直接 `call rel32` 的调用者（虚地址）；空 = 只从函数指针槽被调用。
    pub callers: &'static [u32],
    /// 调用者是谁 / 干什么（一句话；括号里注明了档位）。
    pub by: &'static str,
}

/// 外部怎么进本模块（**观测**：`call rel32` + `.rela.dyn` 指针槽的引用点）。
pub const ENTRY_POINTS: &[EntryPoint] = &[
    EntryPoint { target: 0x514a10, callers: &[0x3b2266, 0x3a55f7, 0x3a5637, 0x3b6e1e, 0x3b703e], by: "root_node 命令总线兜底分支 + 4 个 (self,name) 蹦床" },
    EntryPoint { target: 0x515590, callers: &[0x3b2b84, 0x3b363a], by: "root_node 命令总线（JoinLevelPopUp 路径：先 0x5148a0 收串再调本函数）" },
    EntryPoint { target: 0x5159a0, callers: &[0x3b3333, 0x4f923c], by: "root_node 命令总线 + graphwar_lan_vbox" },
    EntryPoint { target: 0x516970, callers: &[0x3b31bb, 0x3b3679, 0x3b397a, 0x3b3eb9, 0x3b40d9, 0x3b41f1], by: "root_node 命令总线 add_bot_0..5（六个立即数 3/10/25/100/250/1000）" },
    EntryPoint { target: 0x516ad0, callers: &[0x3a559c, 0x3b6f9b], by: "root_node 0x3a54e0（memcpy 进 +0xf8 的缓冲后转发）+ 0x3b6ed0" },
    EntryPoint { target: 0x516370, callers: &[], by: "指针槽 0x9ba958 被 0x3a5760 / 0x3b7ac0 / 0x5147f0 / 0x525f60 引用" },
    EntryPoint { target: 0x516450, callers: &[0x3b1c93], by: "root_node 命令总线（save_clip 分支附近）" },
    EntryPoint { target: 0x5164b0, callers: &[0x3a707a, 0x3b3f6c], by: "root_node 0x3a5760 + 0x3b3f6c" },
    EntryPoint { target: 0x516fd0, callers: &[], by: "指针槽 0x9bc448 被 0x5147b0 引用" },
    EntryPoint { target: 0x5172d0, callers: &[], by: "指针槽 0x9bc128 被 graphwar_setup_control 0x4c44b0 引用" },
    EntryPoint { target: 0x517490, callers: &[], by: "指针槽 0x9bc160 被 graphwar_setup_control 0x4d1e00 / 0x4d88b0 引用" },
    EntryPoint { target: 0x5175b0, callers: &[], by: "指针槽 0x9bc120 被 graphwar_setup_control 0x4c27f0 引用" },
    EntryPoint { target: 0x517690, callers: &[], by: "指针槽 0x9bc458 被 graphwar_backend 0x514f20 + 本模块 0x515590 / 0x5159a0 引用" },
    EntryPoint { target: 0x518240, callers: &[], by: "指针槽 0x9baf38 被 graphwar_replay_player 0x3d61a0 + 本模块 0x518400 引用" },
    EntryPoint { target: 0x518320, callers: &[], by: "指针槽 0x9bc460 被本模块 0x515590 / 0x5159a0 引用" },
    EntryPoint { target: 0x518400, callers: &[], by: "指针槽 0x9bc468 被 graphwar_backend 0x515dc0 + 本模块 0x516fd0 引用" },
    EntryPoint { target: 0x3d6080, callers: &[0x3b157c], by: "root_node 命令总线区域（0x3b157c）" },
    EntryPoint { target: 0x3d7630, callers: &[0x3b15f7], by: "root_node 命令总线区域（0x3b15f7）" },
    EntryPoint { target: 0x3d7380, callers: &[0x515df0], by: "graphwar_backend 0x515dc0 一族" },
];

/// 按目标函数查入口记录。
pub fn entry_point(target: u32) -> Option<&'static EntryPoint> {
    ENTRY_POINTS.iter().find(|e| e.target == target)
}

// ---------------------------------------------------------------------------
// 7. 跨模块调用点（观测）
// ---------------------------------------------------------------------------

/// 本模块**直接** `call rel32` 到别的原版模块的调用点。
///
/// 目标函数的模块归属由 `tools/coverage_matrix.py` 的同一套 `panic::Location`
/// 方法给出。（**不能**只看 `call rel32` 判"有没有调用"：gdext 的引擎调用走 GOT 槽，
/// 见 `AGENTS.md`「间接调用与函数归属」第 1 条。）
///
/// 三列：调用点、目标、目标模块。
pub const DIRECT_CALLS_TO_OTHER_MODULES: &[(u32, u32, &str)] = &[
    (0x5156eb, 0x514d80, "graphwar_backend.rs"),
    (0x515aa5, 0x514d80, "graphwar_backend.rs"),
    (0x5163c7, 0x4592d0, "graphwar_client_connection_handler.rs"),
    (0x518377, 0x45a340, "graphwar_client_connection_handler.rs"),
];

/// 本模块**经 GOT 槽**（`call *0x9bXXXX(%rip)`）到别的原版模块的调用点。
///
/// 四列：调用点、GOT 槽、槽里 `R_X86_64_RELATIVE` 的 addend（= 目标函数）、目标模块。
pub const INDIRECT_CALLS_TO_OTHER_MODULES: &[(u32, u32, u32, &str)] = &[
    (0x5179d4, 0x9bac88, FN_EVENT_SUBSCRIBE, "graphwar_event_handler.rs"),
    (0x517a27, 0x9bac88, FN_EVENT_SUBSCRIBE, "graphwar_event_handler.rs"),
    (0x517a77, 0x9bac88, FN_EVENT_SUBSCRIBE, "graphwar_event_handler.rs"),
    (0x517b65, 0x9bbce8, 0x477bc0, "graphwar_game_state_manager.rs"),
];

// ---------------------------------------------------------------------------
// 8. `GraphwarGameClient::process`（x86 `0x518400`）：60 Hz 定步追赶循环
// ---------------------------------------------------------------------------
//
// 本节照 `docs/16-client.md` §48.3 的逐指令读法落地（x86 全文，`self` 在 `rdi`）：
//
// ```text
// 51840b  mov  r15,[rdi+0x40]       ; r15 = Rc<RefCell<模块容器>>
// 518430  lea  rdi,[r15+0x18]
// 518434  call 0x46aa50             ; get_module::<GraphwarTickHandler>()
// 518458  mov  r13,[GOT 0x9baf38]   ; ⇒ 0x518240 GraphwarGameClient::tick
// 51845f  mov  rbp,[GOT 0x9baef8]   ; ⇒ 0x3d3630 GraphwarEventHandler::process_events
// 518470  <循环头>                   ; RefCell::borrow_mut
// 518484  call 0x519450             ;   GraphwarTickHandler::get_time_to_tick()
// 51848e  cmp  edx,0x3b9aca00       ;   Duration.nanos == 1e9 ?（哨兵 = 欠一个 tick）
// 518494  jne  5184a3               ;   不欠 ⇒ 出循环
// 518496  call r13                  ;   ① GraphwarGameClient::tick()
// 51849c  call rbp                  ;   ② GraphwarEventHandler::process_events()
// 5184a1  jmp  518470               ;   回循环头（一帧欠 N 个 tick 就转 N 圈）
// ```
//
// 三条要点全部出自上面的指令，不是猜的：
//
// 1. **时基是墙钟**：`delta` 两架构都不读（x86 全文不碰 `xmm0`，arm64 不碰 `d0`），
//    定步靠 `GraphwarTickHandler` 里 `Instant::now` 驱动的累加器 ——
//    落到本层就是 [`ProcessLoop::time_to_tick`]，循环体自己**不**碰时间。
// 2. **先 tick 后泵**：`0x518496` 在前、`0x51849c` 在后（arm64 `0x40ac8c` → `0x40ac94` 同序）。
//    顺序反了，本 tick 产生的事件会晚一个 tick 才被订阅者看到。
// 3. **追赶**：`0x5184a1 jmp 0x518470`，不是"每帧最多补一次"。

/// `0x51848e` 的比较立即数 `0x3b9aca00`（1e9）：`get_time_to_tick()` 的纳秒等于它
/// ⇒ **欠一个 tick**。
///
/// 合法 `Duration` 的 `subsec_nanos` 不可能等于 1e9，所以原版的两条"欠账"路径
/// （x86 `0x5194f3`）故意返回这个值当哨兵（`docs/16-client.md` §48.3 的末段）。
pub const DUE_SENTINEL_NANOS: u32 = 1_000_000_000;

/// `0x519450` 的返回值该不该进循环体：`0x51848e` 的 `cmp edx,0x3b9aca00` + `0x518494` 的 `jne`。
///
/// ⚠️ **我方差异（`AGENTS.md` §0.3，必须写清）**：`src/graphwar_tick_handler.rs` 的
/// `get_time_to_tick` 在"欠账"时把下溢**夹成 `(0, 0)`**（原版那条路径在别处落
/// `panic!("overflow when subtracting durations")`），而"**刚好**落在 tick 边界"也返回
/// `(0, 0)` ⇒ 本层**分不开**这两种情况，所以这里把 `(0, 0)` 也算"到期"。
/// 可测的后果只有一处：帧边界**正好**压在 tick 边界上时，本层比原版**早一帧** tick
/// （总 tick 数不变 —— 原版晚一帧补上）。墙钟下这条路径概率为 0；测试里是有意用它来
/// 把"喂 1 秒 ⇒ 恰好 60 个 tick"钉成确定值。
pub fn tick_is_due(time_to_tick: (u64, u32)) -> bool {
    time_to_tick.1 == DUE_SENTINEL_NANOS || time_to_tick == (0, 0)
}

/// `0x518400` 循环体用到的三个动作。
///
/// **我方接口**（`_scaffold` 意义上的接线，原版没有这个 trait）：原版这三件事分别落在
/// `GraphwarTickHandler`（`0x519450`）、`GraphwarGameClient`（`0x518240`）、
/// `GraphwarEventHandler`（`0x3d3630`）三个对象上；本层的 [`crate::_scaffold::root_wiring::KernelBus`]
/// 一个人扮演三者。用一个 trait 把"循环体只依赖这三个调用"钉住 ⇒ [`process`] 本体里
/// 不掺容器 / 队列 / 引擎的任何细节，也就方便拿假宿主把定步语义单独验掉。
pub trait ProcessLoop {
    /// `GraphwarTickHandler::get_time_to_tick`（x86 `0x519450` / arm64 `0x40b298`）。
    fn time_to_tick(&mut self) -> (u64, u32);

    /// `GraphwarGameClient::tick`（x86 `0x518240`）：推进一个 tick。
    ///
    /// 它产出的 `TickEvent`（每 60 个再一条 `TickRequest`）**必须**在
    /// [`Self::process_events`] 之前进队列 —— 否则"先 tick 后泵"就名存实亡。
    fn tick(&mut self);

    /// `GraphwarEventHandler::process_events`（x86 `0x3d3630` / arm64 `0x486970`）
    /// = 我方 `KernelBus::pump`。返回这一次投递了几条（**我方观测值**）。
    fn process_events(&mut self) -> usize;
}

/// 一次 `process()` 转了几圈（**我方观测值**；原版 `process` 没有返回值）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameTicks {
    /// `get_time_to_tick()` 被调了几次 = **1 + 本帧 tick 数**。
    ///
    /// 实测口径（`docs/16-client.md` §48.4）：客户端 `get_time_to_tick` 次数
    /// = 帧数 + due tick 数（`112 = 52 帧 + 60 个 tick`）。
    pub probes: u32,
    /// `GraphwarGameClient::tick`（`0x518240`）被调了几次。
    pub ticks: u32,
    /// 事件泵（`0x3d3630`）被调了几次（== `ticks`：每个 tick 后一次）。
    pub pumps: u32,
}

/// `GraphwarGameClient::process`（x86 `0x518400`；arm64 `0x40ac00`）的循环体。
///
/// 每帧被引擎 `_process(delta)` 推一次，实际调用链（全部观测，`docs/16-client.md` §48.2）：
///
/// ```text
/// GraphwarRootNode::process 0x3a5760 → GraphwarBackend::process 0x515dc0 → 本函数
/// ```
///
/// 进来后按 **1/60 s 的墙钟**追赶：`0x519450` 说"欠一个 tick"就转一圈
/// （**先** `tick()`、**后** `process_events()`），一直追到不欠为止。
///
/// `_delta` 参数**不使用**：原版 x86 全文不读 `xmm0`、arm64 全文不读 `d0`
/// （§48.3）。留着它是为了让签名与引擎 `_process(delta)` / 原版 `process(delta)` 一致 ——
/// 定步的时基在 [`ProcessLoop::time_to_tick`] 里。
pub fn process<L: ProcessLoop + ?Sized>(host: &mut L, _delta: f64) -> FrameTicks {
    let mut out = FrameTicks::default();
    loop {
        // 0x518470 循环头 → 0x518484 get_time_to_tick
        out.probes += 1;
        // 0x51848e/0x518494：不欠 ⇒ 出循环（0x5184a3）
        if !tick_is_due(host.time_to_tick()) {
            break;
        }
        // ① 0x518496 call r13 = GraphwarGameClient::tick
        host.tick();
        out.ticks += 1;
        // ② 0x51849c call rbp = GraphwarEventHandler::process_events
        host.process_events();
        out.pumps += 1;
        // 0x5184a1 jmp 0x518470
    }
    out
}

// ---------------------------------------------------------------------------
// 9. 单测：定步语义（**不碰引擎**，用假宿主 + 真 `GraphwarTickHandler`）
// ---------------------------------------------------------------------------
//
// 真值来源仍是反汇编（`docs/16-client.md` §48.3）；下面的断言只是回归保护
// （`AGENTS.md` §0 第 2 条）。引擎侧那一半在 `client/tools/process_loop_check.gd`。

#[cfg(test)]
mod process_loop_tests {
    use super::*;
    use crate::graphwar_tick_handler::{GraphwarTickHandler, NANOS_PER_SECOND, NANOS_PER_TICK};

    /// 一个假宿主：墙钟用一个可以随便摆的纳秒计数器，tick/pump 都记序。
    struct Host {
        handler: GraphwarTickHandler,
        clock_ns: u64,
        /// 按发生顺序记 `'T'`（tick）/ `'P'`（process_events）。
        trace: Vec<char>,
        /// 每次 `process_events` 时"已经 tick 过几次"，用来证明**同一次** tick 先发生。
        ticks_seen_by_pump: Vec<u32>,
        /// 每次 `process_events` 时发件箱里有几条（原版 tick 直接 push 进共享队列）。
        outbox_seen_by_pump: Vec<usize>,
        ticks: u32,
        frames: u32,
        probes: u32,
    }

    impl Host {
        fn new() -> Self {
            Self {
                // 基准 (0, 0)，与 `assemble_demo` 给的那个基准同值；墙钟由 `clock_ns` 摆。
                handler: GraphwarTickHandler::new(0, 0),
                clock_ns: 0,
                trace: Vec::new(),
                ticks_seen_by_pump: Vec::new(),
                outbox_seen_by_pump: Vec::new(),
                ticks: 0,
                frames: 0,
                probes: 0,
            }
        }

        /// 把墙钟摆到基准之后 `ns` 纳秒（= 原版 `now - base`）。
        fn set_clock_ns(&mut self, ns: u64) {
            self.clock_ns = ns;
            self.handler
                .set_now(ns / 1_000_000_000, (ns % 1_000_000_000) as u32);
        }

        /// 走一帧。
        fn frame(&mut self, delta: f64) -> FrameTicks {
            self.frames += 1;
            let out = process(self, delta);
            assert_eq!(out.pumps, out.ticks, "每个 tick 后恰好一次泵");
            out
        }
    }

    impl ProcessLoop for Host {
        fn time_to_tick(&mut self) -> (u64, u32) {
            self.probes += 1;
            self.handler.get_time_to_tick()
        }

        fn tick(&mut self) {
            self.handler.tick();
            self.ticks += 1;
            self.trace.push('T');
        }

        fn process_events(&mut self) -> usize {
            self.trace.push('P');
            self.ticks_seen_by_pump.push(self.ticks);
            self.outbox_seen_by_pump.push(self.handler.outbox_len());
            // 原版 `tick` 直接 push 进共享队列；本层 tick handler 只有发件箱 ⇒ 泵之前取走。
            self.handler.take_outbox().len()
        }
    }

    /// `0x51848e` 的哨兵与边界：1e9 ⇒ 到期；正的剩余 ⇒ 不到期。
    #[test]
    fn the_due_check_is_the_sentinel_compare() {
        assert_eq!(DUE_SENTINEL_NANOS, 0x3b9a_ca00, "0x51848e 的立即数");
        assert!(tick_is_due((0, DUE_SENTINEL_NANOS)), "哨兵 = 欠一个 tick");
        assert!(tick_is_due((0, 0)), "我方差异：下溢被夹成 (0,0) ⇒ 到期");
        assert!(!tick_is_due((0, NANOS_PER_TICK)));
        assert!(!tick_is_due((0, 1)));
        assert!(!tick_is_due((1, 0)));
    }

    /// 一帧不欠 ⇒ 一次都不转，`get_time_to_tick` 只调 1 次（循环头那次）。
    #[test]
    fn a_frame_that_is_not_due_does_nothing() {
        let mut host = Host::new();
        // ⚠️ 必须先摆钟：`GraphwarTickHandler::new(0, 0)` 的基准是 epoch，不摆就是
        // "1960 年之后 56 年的欠账" ⇒ 循环会追 17 亿个 tick（本仓库真踩过这个挂死）。
        host.set_clock_ns(0);
        let out = host.frame(0.0);
        assert_eq!(out, FrameTicks { probes: 1, ticks: 0, pumps: 0 });
        assert_eq!(host.handler.current_tick, 0);
        assert!(host.trace.is_empty());
    }

    /// **喂 1 秒 ⇒ 恰好 60 个 tick**（一次 `process` 里追完，`probes = 1 + 60`）。
    ///
    /// 60 来自 `0xfe502a` = 16 666 666 ns（`GraphwarTickHandler::tick` 的加法）：
    /// 60 × 16 666 666 = 999 999 960 ≤ 1e9 < 61 个。
    #[test]
    fn one_second_in_one_frame_is_exactly_sixty_ticks() {
        let mut host = Host::new();
        host.set_clock_ns(NANOS_PER_SECOND as u64);
        let out = host.frame(0.0);
        assert_eq!(host.frames, 1);
        assert_eq!(host.probes, out.probes, "循环体只调 get_time_to_tick，不多不少");
        assert_eq!(out.ticks, 60, "定步 60 Hz");
        assert_eq!(out.pumps, 60);
        assert_eq!(out.probes, 61, "帧数 + due tick 数（§48.4 的 112 = 52 + 60 同口径）");
        assert_eq!(host.handler.current_tick, 60);
        // `get_time_to_tick` 读的是**离下一个边界**还有多久：1e9 - 999 999 960 = 40 ns 已过，
        // 所以离第 61 个 tick 还有 16 666 666 - 40 = 16 666 626 ns（不是 0，也不是 1e9 哨兵）。
        assert_eq!(host.handler.get_time_to_tick(), (0, 16_666_626));
    }

    /// **喂 2.5 个 tick ⇒ 转 2 圈并留下 0.5 个 tick**。
    #[test]
    fn two_and_a_half_ticks_tick_twice_and_keep_the_remainder() {
        let mut host = Host::new();
        // 2.5 tick = 41 666 665 ns（2 × 16 666 666 + 8 333 333）。
        host.set_clock_ns(2 * NANOS_PER_TICK as u64 + 8_333_333);
        let out = host.frame(0.0);
        assert_eq!((out.ticks, out.pumps), (2, 2));
        assert_eq!(host.handler.current_tick, 2);
        assert_eq!(
            host.handler.get_time_to_tick(),
            (0, 8_333_333),
            "留下 0.5 个 tick（= 16 666 666 - 8 333 333）"
        );
    }

    /// 帧率不影响 tick 数：60 帧 × 1/120 s 与 120 帧 × 1/240 s 都是 30 个 tick。
    ///
    /// 这就是 §48.4 实测的"帧 52/s 时 tick 仍 60/s"的可执行版本。
    #[test]
    fn tick_rate_does_not_follow_the_frame_rate() {
        let mut fast = Host::new();
        for k in 1..=60u64 {
            fast.set_clock_ns(k * 8_333_333); // 1/120 s
            fast.frame(1.0 / 120.0);
        }
        assert_eq!(fast.handler.current_tick, 30, "60 帧 × 半 tick = 30 tick");

        let mut slower = Host::new();
        for k in 1..=120u64 {
            slower.set_clock_ns(k * 4_166_667); // ≈1/240 s（向上取整，120 帧正好 0.5 s）
            slower.frame(1.0 / 240.0);
        }
        assert_eq!(slower.handler.current_tick, 30, "120 帧 × 1/4 tick = 30 tick");
    }

    /// 掉帧要**在下一帧补回来**（`0x5184a1 jmp 0x518470`），不是丢掉。
    #[test]
    fn dropped_frames_are_caught_up_in_the_next_frame() {
        let mut host = Host::new();
        // 第一帧掉 3 个 tick 的时间，只走一帧。
        host.set_clock_ns(3 * NANOS_PER_TICK as u64);
        assert_eq!(host.frame(0.0).ticks, 3);
        // 之后每帧 1 tick 的墙钟 ⇒ 每帧正好 1 个。
        for k in 4..=8u64 {
            host.set_clock_ns(k * NANOS_PER_TICK as u64);
            assert_eq!(host.frame(0.0).ticks, 1, "第 {k} 个 tick 的墙钟");
        }
        assert_eq!(host.handler.current_tick, 8);
    }

    /// **先 tick 后泵**（`0x518496` → `0x51849c`）：泵看到的已经是"刚 tick 过"的世界，
    /// 且发件箱里已经有这条 tick 的 `TickEvent`。
    #[test]
    fn tick_runs_before_process_events() {
        let mut host = Host::new();
        host.set_clock_ns(3 * NANOS_PER_TICK as u64);
        host.frame(0.0);
        assert_eq!(host.trace, vec!['T', 'P', 'T', 'P', 'T', 'P'], "每圈都是 T 然后 P");
        assert_eq!(host.ticks_seen_by_pump, vec![1, 2, 3], "泵看到 tick 计数已经涨过");
        assert_eq!(host.outbox_seen_by_pump, vec![1, 1, 1], "泵前发件箱里已有 tick 产出");
        assert_eq!(host.handler.outbox_len(), 0, "泵把发件箱取走了");
    }

    /// 第 60 个 tick 会**再**发一条 `TickRequest`（`tick` 里的整除分支）——
    /// 它必须也在**同一次**泵之前进队列。
    #[test]
    fn the_sixtieth_tick_queues_a_tick_request_before_the_pump() {
        let mut host = Host::new();
        host.set_clock_ns(60 * NANOS_PER_TICK as u64);
        host.frame(0.0);
        assert_eq!(host.outbox_seen_by_pump.len(), 60);
        assert_eq!(
            host.outbox_seen_by_pump[59], 2,
            "第 60 个 tick 一口气产 TickEvent + TickRequest"
        );
        assert_eq!(host.outbox_seen_by_pump[58], 1, "其余每个 tick 只产 TickEvent");
    }

    /// `delta` **不被使用**（两架构都不读 delta 寄存器）：同样的墙钟、不同的 delta ⇒ 同样结果。
    #[test]
    fn delta_is_not_used_by_the_loop() {
        let mut a = Host::new();
        let mut b = Host::new();
        a.set_clock_ns(7 * NANOS_PER_TICK as u64 + 10);
        b.set_clock_ns(7 * NANOS_PER_TICK as u64 + 10);
        let ra = a.frame(0.0);
        let rb = b.frame(1.0);
        assert_eq!(ra, rb);
        assert_eq!(ra.ticks, 7);
        // 一个巨大的 delta 也不会多转一圈。
        assert_eq!(b.frame(1e9).ticks, 0);
    }

    /// 本帧的 `probes` 恒等于 `1 + ticks`（`0x518484` 在循环头、每圈一次）。
    #[test]
    fn probes_equal_one_plus_ticks() {
        for ns in [0u64, 1, NANOS_PER_TICK as u64, 5 * NANOS_PER_TICK as u64 + 7] {
            let mut host = Host::new();
            host.set_clock_ns(ns);
            let out = host.frame(0.0);
            assert_eq!(out.probes, out.ticks + 1, "ns = {ns}");
        }
    }
}

// ---------------------------------------------------------------------------
// 10. 连接门面 `connect` / `disconnect`：两条 F3 落成**有函数体**
// ---------------------------------------------------------------------------
//
// 这两条在 `fixtures/function_coverage.tsv` 里原来是 **F3**（"地址有登记、没有 fn"）。
// 它们就是 T52 落码的两个被调者（`graphwar_client_connection_handler::connect 0x45a340` /
// `disconnect 0x4592d0`）的**唯一门面**：本模块取连接模块 → `borrow_mut()` → 调一跳 → 掉引用。
//
// 边界/体量取自 `.eh_frame` FDE（`tools/addr_probe.py fde`）；真名取自
// `fixtures/windows_name_to_addr.tsv`（Windows 2.0.60 同版本同指令集，经 `panic::Location`
// 一对一锚）：`connect` = `93:57`、`disconnect` = `118:57`，两者都还引用**同一条**
// `152:22`（那是本模块所有"取模块 + `unwrap`"共用的那一行 —— `request_lock_change` 的
// 唯一锚也是 `152:22`，见 `fixtures/name_to_addr_v2.tsv`）。
//
// 逐条台账在 `fixtures/setup_control_anchors.tsv`（下半张表）。

use crate::graphwar_client_connection_handler::{GraphwarClientConnectionHandler, Transport};
use std::cell::RefCell;
use std::rc::Rc;

/// `GraphwarGameClient::connect` —— **x86 `0x518320`**（FDE `0x518320..0x5183fb`，`0xdb` = **219 B**）。
pub const FN_GAMECLIENT_CONNECT: u64 = 0x51_8320;

/// `GraphwarGameClient::disconnect` —— **x86 `0x516370`**（FDE `0x516370..0x51644b`，`0xdb` = **219 B**）。
pub const FN_GAMECLIENT_DISCONNECT: u64 = 0x51_6370;

/// `ModuleContainer::get::<GraphwarClientConnectionHandler>()` —— **x86 `0x46b180`**。
///
/// 档位：**逐位**（`docs/02-module-map.md` §22.3 的 25 模块表：x86 `get` 一列；三路证据交集）。
pub const FN_GET_CONNECTION_HANDLER: u64 = 0x46_b180;

/// 两条门面的 `(vaddr, fde_lo, fde_hi, size, 真名)`。
///
/// 档位：**逐位**（`.eh_frame`；`tools/addr_probe.py fde` 可复跑）。两条**同为 `0xdb` 字节**
/// ——它们本来就是同一个模板的两份实例化（只差内层被调者）。
pub const CONN_FACADE_ANCHORS: &[(u64, u64, u64, u32, &str)] = &[
    (0x51_8320, 0x51_8320, 0x51_83fb, 0xdb, "GraphwarGameClient::connect"),
    (0x51_6370, 0x51_6370, 0x51_644b, 0xdb, "GraphwarGameClient::disconnect"),
];

/// 两条门面调的内层函数（`call rel32`，**逐位**）。
pub const CONN_FACADE_TARGETS: &[(u64, u64, &str)] = &[
    (0x51_8320, 0x45_a340, "GraphwarClientConnectionHandler::connect"),
    (0x51_6370, 0x45_92d0, "GraphwarClientConnectionHandler::disconnect"),
];

/// 两条门面被**虚拟调用**的槽（`call rel32` 为 0 —— 它们是虚表项）。
///
/// 档位：**逐位**（`addr_probe.py callers` 的 `.rela.dyn` addend 引用点）。
pub const CONN_FACADE_VIRTUAL_SLOTS: &[(u64, u64, &[u64])] = &[
    (0x51_83fb_u64 - 0xdb, 0x9b_c460, &[0x51_5666, 0x51_5a1d]),
    (0x51_6370, 0x9b_a958, &[0x3a_719e, 0x3b_7df6, 0x51_480b, 0x52_5f86]),
];

/// 原版 `unwrap()` 在 `None` 上的 panic 文案（std 侧常量）。
pub const MSG_OPTION_UNWRAP: &str = "called `Option::unwrap()` on a `None` value";

/// `RefCell::borrow_mut` 冲突臂的文案。
///
/// ⚠️ **推断**：该臂在原版是 std 的 `RefCell` 借用冲突 panic（`0x5183b6` 引 Location 槽
/// `0x9625d8`，即本函数 `93:57`）；**具体文案串未在 `.rodata` 定位**（std 侧常量）。
/// "会 panic" 是观测的，"文案" 是推断。
pub const MSG_ALREADY_MUTABLY_BORROWED: &str = "already mutably borrowed";

/// 原版 `self+0x40` 那个「模块容器」（`Rc<RefCell<ModuleContainer>>`）的接缝。
///
/// 判据：`0x518329 mov r15,[rdi+0x40]` / `0x516379` 同；随后 `[r15+0x10]` 是
/// **`RefCell` 借用标志**（不是 `Arc` 强计数！）：`0x51832d mov rax,[r15+0x10]` 上界
/// `isize::MAX` 检查 = borrow 计数溢出，`0x5183b6` 的 `jne` = 已有借用。
pub trait ClientModules {
    /// `0x51834b call 0x46b180` —— 取连接模块。`None` ⇒ 原版 `0x5183a7` panic
    /// （Location `152:22`，槽 `0x9ba870`）。
    fn conn_handler(&self) -> Option<Rc<RefCell<dyn ConnHandlerSeam>>>;
}

/// 连接模块上这两条门面需要的两个动作。
///
/// 我方对应物是 T52 落的 `GraphwarClientConnectionHandler`（见下面的 blanket impl）；
/// 用 trait 是为了让门面本体能拿假句柄验借用/panic 序列。
pub trait ConnHandlerSeam {
    /// `GraphwarClientConnectionHandler::connect`（**x86 `0x45a340`**）。
    fn connect(&mut self);
    /// `GraphwarClientConnectionHandler::disconnect`（**x86 `0x4592d0`**）。
    fn disconnect(&mut self);
}

/// 把 T52 落码的真类型接上（**不是**自造语义：两个方法各转发一跳）。
///
/// 用自由函数转发是为了避开"trait 方法与同名固有方法"的方法解析歧义（固有方法优先，
/// 但 UFCS 路径在两种写法间容易写错；自由函数把这件事写死）。
fn conn_handler_connect<S: Transport + Send + 'static>(h: &mut GraphwarClientConnectionHandler<S>) {
    GraphwarClientConnectionHandler::connect(h); // 固有方法，0x45a340
}

fn conn_handler_disconnect<S: Transport + Send + 'static>(h: &mut GraphwarClientConnectionHandler<S>) {
    GraphwarClientConnectionHandler::disconnect(h); // 固有方法，0x4592d0
}

impl<S: Transport + Send + 'static> ConnHandlerSeam for GraphwarClientConnectionHandler<S> {
    fn connect(&mut self) {
        conn_handler_connect(self)
    }
    fn disconnect(&mut self) {
        conn_handler_disconnect(self)
    }
}

/// 原版这个"游戏客户端"类型（Godot 类 `GraphwarGameClient`）。
///
/// 只落**标记类型**：gdext 侧那份在 `src/_scaffold/graphwar_game_client_class.rs`（`AGENTS.md` §4
/// 不算复原层）。本文件用同名类型把两条原版**方法**（符号表真名
/// `graphwarrust::graphwar_game_client::GraphwarGameClient::{connect,disconnect}`）按原版类型归属落下来。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct GraphwarGameClient;

    impl GraphwarGameClient {
        /// `GraphwarGameClient::connect` —— **x86 `0x518320`**（`0xdb` = 219 B）。
    ///
    /// ```text
    /// 518329 mov r15,[rdi+0x40]        ; self+0x40 = Rc<RefCell<ModuleContainer>>
    /// 51832d mov rax,[r15+0x10]        ; RefCell 借用计数
    /// 518331 movabs rcx,0x7fff…ff / 51833b cmp / 51838e jae 0x51839a
    ///                                  ; 借用计数溢出 ⇒ panic（槽 0x9ba878 → 0x925320）
    /// 518390 inc rax / 518393 mov [r15+0x10],rax   ; borrow()
    /// 518397 lea rdi,[r15+0x18] / 51839b call 0x46b180  ; get::<GraphwarClientConnectionHandler>()
    /// 5183a3 test rax,rax / 5183a6 je 0x5183a7         ; None ⇒ panic（槽 0x9ba870 → 0x925410）
    /// 5183a8 dec qword [r15+0x10]                       ; 结束那次 borrow（临时值）
    /// 5183b1 cmp qword [rbx+0x10],0 / 5183b6 jne 0x518406 ; 已被可变借用 ⇒ panic（槽 0x9ba6d0）
    /// 5183b8 mov qword [rbx+0x10],-1                    ; borrow_mut
    /// 5183c3 add rdi,0x18 / 5183c7 call 0x45a340        ; **内层 connect**
    /// 5183cc inc qword [rbx+0x10]                       ; 解除 borrow_mut
    /// 5183d0 dec qword [rbx] / 5183d3 jne / 5183da call *0x9bae78 = 0x524520  ; 掉句柄引用
    /// ```
    ///
    /// 档位：**观测**（三条 panic 臂 + 借用标志偏移 `+0x10` + 句柄数据 `+0x18` + 被调者）。
    pub fn connect(modules: &dyn ClientModules) {
        // 0x518320  —— 本函数起点；FDE 0x518320..0x5183fb（0xdb = 219 B）
        // 0x51834b：取连接模块（原版是 `borrow()` 后 `get`，这里由接缝一并给出）
        let handler = modules.conn_handler().expect(MSG_OPTION_UNWRAP); // panic 槽 0x9ba870
        // 0x5183b1 / 0x5183b8：`[+0x10]` 必须为 0，否则 panic，然后置 -1
        let mut guard = handler
            .try_borrow_mut()
            .expect(MSG_ALREADY_MUTABLY_BORROWED); // panic 槽 0x9ba6d0
        // 0x5183c7 call 0x45a340
        guard.connect();
        // 0x5183cc..0x5183da：解除借用 + 掉引用 + drop（`Rc` 在这里自然完成）
    }

    /// `GraphwarGameClient::disconnect` —— **x86 `0x516370`**（`0xdb` = 219 B）。
    ///
    /// 与 [`connect`] **逐项同形**（同 `0xdb` 字节、同 `0x46b180`、同三条 panic 臂、同借用/掉引用
    /// 序列），唯一差别是内层被调者：`0x5183c7` 的 `0x45a340` ↔ 这里的 `0x5163c7` 的 `0x4592d0`。
    ///
    /// 档位：**观测**。
    pub fn disconnect(modules: &dyn ClientModules) {
        // 0x516370  —— 本函数起点；FDE 0x516370..0x51644b（0xdb = 219 B）
        let handler = modules.conn_handler().expect(MSG_OPTION_UNWRAP); // 0x5163a7
        let mut guard = handler
            .try_borrow_mut()
            .expect(MSG_ALREADY_MUTABLY_BORROWED); // 0x5163b6
        // 0x5163c7 call 0x4592d0
        guard.disconnect();
    }

}

// ---------------------------------------------------------------------------
// 11. 单测（期望值来自反汇编：一跳 / 三条 panic 臂 / FDE 体量）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod conn_facade_tests {
    use super::*;
    use std::cell::Cell;

    /// 假连接模块：记调用次数（原版这两个门面各自**只**调一跳）。
    #[derive(Default)]
    struct FakeHandler {
        connects: Cell<u32>,
        disconnects: Cell<u32>,
    }

    impl ConnHandlerSeam for FakeHandler {
        fn connect(&mut self) {
            self.connects.set(self.connects.get() + 1);
        }
        fn disconnect(&mut self) {
            self.disconnects.set(self.disconnects.get() + 1);
        }
    }

    struct FakeModules(Option<Rc<RefCell<dyn ConnHandlerSeam>>>);

    impl ClientModules for FakeModules {
        fn conn_handler(&self) -> Option<Rc<RefCell<dyn ConnHandlerSeam>>> {
            self.0.clone()
        }
    }

    /// FDE 表自洽：两条 `0xdb`、vaddr 就是 FDE 起点、互不相同。
    #[test]
    fn conn_facade_anchor_table_is_self_consistent() {
        assert_eq!(CONN_FACADE_ANCHORS.len(), 2);
        for (va, lo, hi, size, name) in CONN_FACADE_ANCHORS {
            assert_eq!(va, lo, "{name}");
            assert_eq!(hi - lo, *size as u64, "{name}");
            assert_eq!(*size, 0xdb, "{name}：两条都是 0xdb = 219 字节");
        }
        assert_ne!(CONN_FACADE_ANCHORS[0].0, CONN_FACADE_ANCHORS[1].0);
    }

    /// 两条门面各自**只**调一跳（`0x5183c7` / `0x5163c7`；别处没有内层调用）。
    #[test]
    fn each_facade_calls_exactly_one_inner_method() {
        assert_eq!(CONN_FACADE_TARGETS.len(), 2);
        for (_, target, _) in CONN_FACADE_TARGETS {
            assert!(*target == 0x45_a340 || *target == 0x45_92d0);
        }
        let h = Rc::new(RefCell::new(FakeHandler::default()));
        let m = FakeModules(Some(h.clone()));
        GraphwarGameClient::connect(&m);
        assert_eq!(h.borrow().connects.get(), 1);
        assert_eq!(h.borrow().disconnects.get(), 0);
        GraphwarGameClient::disconnect(&m);
        assert_eq!(h.borrow().connects.get(), 1);
        assert_eq!(h.borrow().disconnects.get(), 1);
    }

    /// 虚表槽：`connect` 2 个引用点、`disconnect` 4 个引用点（`addr_probe.py callers`）。
    #[test]
    fn virtual_slot_usage_counts_match_the_binary() {
        assert_eq!(CONN_FACADE_VIRTUAL_SLOTS[0].2.len(), 2);
        assert_eq!(CONN_FACADE_VIRTUAL_SLOTS[1].2.len(), 4);
        assert_eq!(CONN_FACADE_VIRTUAL_SLOTS[0].1, 0x9b_c460);
        assert_eq!(CONN_FACADE_VIRTUAL_SLOTS[1].1, 0x9b_a958);
    }

    /// `0x5183a7` / `0x5163a7`：容器里没有这个模块 ⇒ `unwrap()` panic。
    #[test]
    #[should_panic(expected = "None")]
    fn missing_module_panics_like_the_original_unwrap() {
        let m = FakeModules(None);
        GraphwarGameClient::connect(&m);
    }

    /// `0x5183b6` / `0x5163b6`：句柄已被可变借用 ⇒ `RefCell` 冲突 panic。
    #[test]
    #[should_panic(expected = "already mutably borrowed")]
    fn already_borrowed_panics_like_the_original_refcell() {
        let h = Rc::new(RefCell::new(FakeHandler::default()));
        let _held = h.borrow_mut();
        let m = FakeModules(Some(h.clone()));
        GraphwarGameClient::disconnect(&m);
    }

    /// 取模块的地址 = `docs/02` §22.3 表里 `GraphwarClientConnectionHandler` 的 x86 `get`。
    #[test]
    fn connection_handler_getter_address_matches_the_module_table() {
        assert_eq!(FN_GET_CONNECTION_HANDLER, 0x46_b180);
        assert_eq!(FN_GAMECLIENT_CONNECT - FN_GAMECLIENT_DISCONNECT, 0x1f_b0);
    }
}

// ---------------------------------------------------------------------------
// 12. F3 落码（T62，2026-11-27）：19 条「只有地址登记、没有函数体」的原版函数
// ---------------------------------------------------------------------------
//
// 起点是 T38/T55 的函数级审计（`fixtures/function_coverage.tsv`）：本模块 36 行里 **19 条**
// 判 `F3 / register-only`（"地址有登记（注释里写了）但没有贴到任何函数体"）。
// 本节按 `tools/addr_probe.py fde/dis` 的**逐指令**读法把它们落成**独立的 `fn`**，
// 每条的真名、FDE 范围、证据档写在它自己的 doc 里。
//
// 判据只有两类：
//
// * **观测** —— 指令能逐条读出来（`movabs` 立即数 tag / 调用目标 / 结构偏移 / 分支）；
// * **推断** —— 只能读到形状，语义靠命名与上下文，**缺什么写在条目里**。
//
// 事件载荷（tag + 字段形状）有**两条独立来源**相交，这是本节最硬的一层证据：
// ① 本节反汇编里 `movabs $0x80000000000000NN` 的立即数；
// ② `src/_scaffold/protocol.rs` 的 `WIRE_SPEC`（线上真字节的字段表，`docs/07-protocol.md` §3.3;
//    其中 6 类还在 T53 的官服抓包里拿到过真帧）。
//
// ⚠️ 原版这些函数**不是**自由函数：除两条 `get_module` 单态化外都是
// `impl GraphwarGameClient`（或 `impl GraphwarReplayPlayer`）上的方法，`self` 在 `rdi`
// （`connect`/`disconnect` 是 `0x517490`/`0x5172d0` 那几条也一样）。
// 本节的接缝 trait（`*Env` / `ModuleGetter` / `EventQueue`）是**我方接线**：原版没有它们，
// 它们只把"函数体真正依赖的几件事"抽出来，好让语义单独可验。

use crate::graphwar_game_recorder::GraphwarGameRecorder;
use crate::graphwar_replay_player::ReplayPlayer;
use crate::graphwar_tick_handler::GraphwarTickHandler;

/// 一条 `Event` 记录（原版 `Vec<Event>` 的元素，固定 **0x48** 字节，见 [`EVENT_RECORD_SIZE`]）
/// 的**载荷**部分（`+0x08` 起）。
///
/// 形状全部来自本节各函数体内的 `mov`/`movups` 落位：
///
/// | 变体 | 出现处 | 记录里的字节 |
/// |---|---|---|
/// | [`EventPayload::Id`] | 16 个请求构造器、`FunctionFireRequest` | `+0x08` 一个 i32 |
/// | [`EventPayload::Lock`] | `LockRequest` | `+0x08` 一个 bool |
/// | [`EventPayload::Name`] | `GameNameRequest` | `+0x08` 一个 `String`（24 B） |
/// | [`EventPayload::TextAndId`] | `FunctionUpdateRequest` / `ChatMessageRequest` | `+0x08` `String`（24 B）+ `+0x20` i64 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    /// 一个 i32（原版 `mov %eax,0x8(%rsp)` / `mov %dword,%dword ptr [%rsp+0x18]`）。
    Id(i32),
    /// 一个 bool（`LockRequest`：`xor %al,1` 后 `mov %al,dword ptr [%rsp+0x18]`）。
    Lock(bool),
    /// 一个 `String`（`GameNameRequest`）。
    Name(String),
    /// `String` + i64 id（`FunctionUpdateRequest` / `ChatMessageRequest`）。
    TextAndId { text: String, id: i64 },
}

impl EventPayload {
    /// 载荷占记录的多少字节（`TextAndId` = 24 + 8 = 32）。
    pub fn record_bytes(&self) -> usize {
        match self {
            EventPayload::Id(_) => 4,
            EventPayload::Lock(_) => 1,
            EventPayload::Name(_) => 24,
            EventPayload::TextAndId { .. } => 32,
        }
    }
}

/// 一条**已经落进发送队列**的 `Event` 记录。
///
/// tag word = `0x8000_0000_0000_0000 | 判别值`（[`EVENT_TAG_BASE`]，`docs/07` §7.3.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedEvent {
    pub event: Event,
    pub payload: EventPayload,
}

impl QueuedEvent {
    /// 记录 `+0x00` 的 tag word。
    pub fn tag_word(&self) -> u64 {
        EVENT_TAG_BASE | self.event.wire_tag() as u64
    }
}

/// 发送队列的接缝（原版 = 客户端状态 `[[self]+0x18]` 那个 `Vec<Event>`；
/// 每一处 push 都是 `call *0x9bac80`，槽值 `0x3d3530` = `Vec::push`）。
pub trait EventQueue {
    fn push(&mut self, ev: QueuedEvent);
}

/// 取模块的接缝：`borrow()` → `get::<T>()` → `unwrap()` → 结束那次 `borrow`。
///
/// 原版两处（`0x516450` / `0x5164b0`）都是这个 8 条指令的形状：
/// `mov rbx,[rdi+0x40]`（模块容器）→ 借用计数上界检查（`0x516459` 的 `movabs
/// $0x7fffffffffffffff`）→ `inc`/`dec` → `lea rdi,[rbx+0x18]` → **`call` 一个 `get`** →
/// `test rax,rax; je` + `unwrap` panic 槽 `0x9ba870`。
pub trait ModuleGetter<T> {
    /// 取模块；容器里没有 ⇒ panic（原版 `0x516490`/`0x5164f0` 的 `unwrap` 臂）。
    fn get_module(&mut self) -> T;
}

// ---------------------------------------------------------------------------
// 12.1 7 条「实例化 B」请求构造器（`0x4d8a00` 那一组）
// ---------------------------------------------------------------------------
//
// 8 种请求各有**两份实例化**（A 组 `0x4c5b50…` 已在 §3 落码，B 组这 7 条是 F3；
// 第 8 条 `0x4d96e0` 早就是 F1）。两份的 panic 锚行号**完全相同**
// （`fixtures/boundary_audit.tsv`：190/194/198/202/206/210/214/218 列 26）⇒ 同一个源函数
// 的两个单态化。B 组比 A 组多一次 `decq (%rcx)` 与一次 `0x525f60` 调用（收尾 drop 路径不同）。
//
// B 组的统一形状（以 `0x4d8a00` 为例，全部 7 条同形）：
//
// ```text
// 4d8a07  mov  r14,(%rdi)                  ; self.0 = 绑定单元（Rc）
// 4d8a0a  mov  eax,0x8(%rdi)               ; self.1 = i32（请求参数）
// 4d8a28  mov  rbx,0x1f8(%r14)             ; 内层单元
// 4d8a2f  test rbx,rbx ; je 0x4d8a85        ; 内层为空 ⇒ 只做引用计数收尾，不 push
// 4d8a4b  movabs rax,0x80000000000000NN     ; ⇒ 判别值 NN
// 4d8a5f  call *0x9bac80                    ; = 0x3d3530 = Vec::push
// ```

/// `0x4d8a00` —— `remove_soldier_request_event` 的**实例化 B**（FDE `0x4d8a00..0x4d8af4`，0xf4）。
///
/// `0x4d8a4b movabs $0x8000000000000011` ⇒ 判别值 **17** = `RemoveSoldierRequest`
/// （`src/events.rs`；`docs/07` §7.2）。档位：**观测**。
pub fn remove_soldier_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::RemoveSoldier, arg)
}

/// `0x4d8d30` —— `change_team_request_event` 的实例化 B（FDE `0x4d8d30..0x4d8e24`，0xf4）。
///
/// `0x4d8d7b movabs $0x800000000000000b` ⇒ 判别值 **11** = `ChangeTeamRequest`。档位：**观测**。
pub fn change_team_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::ChangeTeam, arg)
}

/// `0x4d8ff0` —— `turn_mode_change_request_event` 的实例化 B（FDE `0x4d8ff0..0x4d90e0`，0xf0）。
///
/// `0x4d903b movabs $0x8000000000000014` ⇒ 判别值 **20** = `TurnModeChangeRequest`。档位：**观测**。
pub fn turn_mode_change_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::TurnMode, arg)
}

/// `0x4d9270` —— `add_soldier_request_event` 的实例化 B（FDE `0x4d9270..0x4d9364`，0xf4）。
///
/// `0x4d92bb movabs $0x8000000000000010` ⇒ 判别值 **16** = `AddSoldierRequest`。档位：**观测**。
pub fn add_soldier_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::AddSoldier, arg)
}

/// `0x4d9370` —— `remove_player_request_event` 的实例化 B（FDE `0x4d9370..0x4d9464`，0xf4）。
///
/// `0x4d93bb movabs $0x800000000000000c` ⇒ 判别值 **12** = `RemovePlayerRequest`。档位：**观测**。
pub fn remove_player_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::RemovePlayer, arg)
}

/// `0x4d9500` —— `time_mode_change_request_event` 的实例化 B（FDE `0x4d9500..0x4d95f0`，0xf0）。
///
/// `0x4d954b movabs $0x8000000000000015` ⇒ 判别值 **21** = `TimeModeChangeRequest`。档位：**观测**。
pub fn time_mode_change_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::TimeMode, arg)
}

/// `0x4d95f0` —— `axis_mode_change_request_event` 的实例化 B（FDE `0x4d95f0..0x4d96e0`，0xf0）。
///
/// `0x4d963b movabs $0x8000000000000012` ⇒ 判别值 **18** = `AxisModeChangeRequest`。档位：**观测**。
pub fn axis_mode_change_request_event_b(arg: i32) -> RequestEvent {
    request_event(RequestKind::AxisMode, arg)
}

// ---------------------------------------------------------------------------
// 12.2 两条 `get_module` 单态化（`0x516450` / `0x5164b0`）
// ---------------------------------------------------------------------------
//
// 这两条就是 §5 里 `0x517690` 装配 manager 时用的那句 `get::<T>()` 的**方法版**：
// 形状与 `connect`/`disconnect` 的取模块段**逐项同形**（同 `+0x40` 容器、同借用计数检查、
// 同 `unwrap` panic 槽 `0x9ba870`），唯一差别是收尾少一次 `borrow_mut`，多一条 `ret`。
//
// ```text
// 516450  mov  rbx,[rdi+0x40]          ; 模块容器
// 516455  borrow 计数上界检查（0x7fffffffffffffff）
// 516468  inc rax / 51646b mov [rbx+0x10],rax
// 51646f  lea  rdi,[rbx+0x18] ; call 0x46aa50     ; get::<GraphwarTickHandler>()
// 51647b  je 0x516490 → unwrap panic（槽 0x9ba870 = 0x925410）
// 51647d  dec [rbx+0x10]                            ; 结束 borrow
// 516482  ret                                       ; 返回句柄
// ```

/// 取模块的 `get` 目标之一（`0x51646f` 的 `call 0x46aa50`）。
///
/// `docs/02-module-map.md` §22.3 的 25 模块表：`0x46aa50` = `get_module::<GraphwarTickHandler>`。
pub const FN_GET_TICK_HANDLER: u32 = 0x46_aa50;

/// 取模块的 `get` 目标之二（`0x5164d3` 的 `call 0x46aea0`）。
///
/// 同上表：`0x46aea0` = `get_module::<GraphwarGameRecorder>`。
pub const FN_GET_GAME_RECORDER: u32 = 0x46_ae_a0;



// ---------------------------------------------------------------------------
// 12.3 `request_game_name_change`（`0x5175b0`）
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// 12.4 `request_lock_change`（`0x517490`）
// ---------------------------------------------------------------------------

/// `0x517490` 需要的两件事。
pub trait LockChangeEnv {
    /// `GraphwarGameStateManager` 的 **`+0x136`** 那个字节
    /// （`0x5174f0 movzx %eax,byte ptr [%rbx+0x136]`；`rbx` = `0x46be70` 取到的
    /// `get_module::<GraphwarGameStateManager>()` 句柄，字段在 `+0x18` 之后 ⇒ 对象内 `+0x11e`）。
    ///
    /// ⚠️ **推断**：字段的**含义**（是否"房间已锁"）没有单独证据，只能读到
    /// "一个 bool，被取反后当 `LockRequest` 的载荷发出去"。缺的证据：`+0x136` 的写入者。
    fn locked_flag(&self) -> bool;
    fn push_event(&mut self, ev: QueuedEvent);
}


// ---------------------------------------------------------------------------
// 12.5 `send_chat_message`（`0x5172d0`）
// ---------------------------------------------------------------------------

/// `0x5172d0` 需要的三件事。
pub trait ChatEnv {
    /// 连接模块的 **`+0x18`** 那个字节（`0x517337 cmp byte ptr [%r14+0x18],0`；
    /// `r14` = `0x46b180` 取到的 `get_module::<GraphwarClientConnectionHandler>()` 句柄）。
    ///
    /// ⚠️ **推断**：语义按"是否已连接"读（`wait_for_players` 的 `0x51706c` 是同一个字段、
    /// 同一个用法）。缺的证据：该字节的写入点。
    fn is_connected(&self) -> bool;
    /// 连接模块的 **`+0x1c`** 那个 i32（`0x51734c mov %eax,dword ptr [%r14+0x1c]`）。
    ///
    /// 同一个值在 `0x51708a`（`wait_for_players`）里被当成"本地 player id"去 player_manager
    /// 里查；在这里它当 `ChatMessageRequest.entity_id` 发出去 ⇒ **两个用法互相印证**。
    fn local_entity_id(&self) -> i64;
    fn push_event(&mut self, ev: QueuedEvent);
}


// ---------------------------------------------------------------------------
// 12.6 `update_function`（`0x516ad0`）/ `fire_function`（`0x516510`）
// ---------------------------------------------------------------------------
//
// 这两条是**同一个模板的两份**，都按「当前回合的玩家列表」逐人发一条请求，
// 且都**跳过电脑玩家**（电脑玩家自己会开火 / 自己更新函数）：
//
// ```text
// 516b15 / 516553  call 0x46b740           ; get_module::<GraphwarTurnManager>()
// 516b5f / 51659d  call 0x46c430           ; get_module::<GraphwarLocalPlayerManager>()
// 516b91 / 5165cf  call 0x46b460           ; get_module::<GraphwarComputerPlayerManager>()
// 516bc2 / 5165f6  call 0x463340           ; turn_manager.get_current_turn_players()
// 516c84 / 5166b1  cmp  [%r15+0x48],0      ; 电脑玩家表空 ⇒ 直接发
// 516c99 / 5166c6  call 0x47dec0           ; hashbrown 探针（元素 0x78 字节）
// 516cf0 / 51671d  je   →  命中 ⇒ **跳过这个玩家**
// ```

/// `0x516ad0` / `0x516510` 需要的四件事。
pub trait TurnPlayerEnv {
    /// `GraphwarTurnManager::get_current_turn_players`（**`0x463340`**）的返回值
    /// （`docs/02` §22 表：`get_current_turn_players` → `graphwar_turn_manager.rs`）。
    fn current_turn_players(&mut self) -> Vec<i32>;
    /// 这个 player id 在 **`GraphwarComputerPlayerManager`** 的那张表里有没有
    /// （`0x516b91` 取句柄、`0x516c99` 探针、`0x516cf0 je` 命中 ⇒ 跳过）。
    fn is_computer_player(&mut self, player_id: i32) -> bool;
    fn push_event(&mut self, ev: QueuedEvent);
}


/// `fire_function` 的两段结果（**我方观测值**；原版没有返回值）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FireOutcome {
    /// 先转发给 `update_function` 的那一条链 push 了几条（`0x51652e call *0x9ba888`）。
    pub updated: usize,
    /// 本函数自己 push 的 `FunctionFireRequest` 有几条（`0x5167b8` 的 `Vec::push`）。
    pub fired: usize,
}


// ---------------------------------------------------------------------------
// 12.7 `wait_for_players`（`0x516fd0`）
// ---------------------------------------------------------------------------

/// `wait_for_players` 需要的三件事。
pub trait WaitForPlayersHost {
    /// "本地 player 已经在 player_manager 里了"：
    /// `0x517040` 取 `get_module::<GraphwarClientConnectionHandler>()` →
    /// `0x51706c` 判 `+0x18` 那个"已连接"字节 →
    /// `0x517099` 取 `get_module::<GraphwarPlayerManager>()`（`0x46a490`）→
    /// `0x51708a` 取本地 id（`+0x1c`）→ `0x517118 call 0x3bf230` 查一张 `HashMap` →
    /// **`0x517155 test %r15,%r15; jne 0x5171ac`（非空 ⇒ 退出等待）**。
    fn local_player_present(&mut self) -> bool;
    /// `GraphwarGameClient::process`（**`0x518400`**；`0x517189 call rbp`，`rbp = [GOT 0x9bc468]`）。
    fn process_frame(&mut self);
    /// `0x51718d mov $0xf4240,%esi` + `0x517192 call r15`（`r15 = [GOT 0x9bbcf8] = 0x906f70`）
    /// = `sleep(0 s + 1_000_000 ns)` ⇒ **1 ms**（`%edi = 0` 是 `Duration` 的 secs 位）。
    fn sleep_one_millisecond(&mut self);
}


// ---------------------------------------------------------------------------
// 12.8 两条「起客户端」门面（`0x515590` / `0x5159a0`）
// ---------------------------------------------------------------------------
//
// 这两条是**串行编排**函数：8–10 步，每步一个 `call`。两条的**共尾**完全一样：
// 装配事件（`0x517690`）→ 重置事件处理器（`0x3d5260`）→ 建 recorder（`0x469c70`）→
// 连接（`0x518320`）→ 把装配结果写进 `self+0x1e0` → `GraphwarBackend::resubscribe_to_events`
// （`0x514d80`，`docs/02` 表）。差别只有两条：**local 版**多两步
// （`0x51559c`/`0x5155ad` 的状态守卫 `0x44f2b0`；`0x5157d3` 再发一条 `GameNameRequest`）。

/// `0x515590` / `0x5159a0` 的编排步骤。顺序 = 二进制里的 `call` 顺序（**观测**）。
pub trait BackendStartSteps {
    /// `0x51559c cmp dword [%rdi+0x60],4` + `0x5155ad call 0x44f2b0`（**只有 local 版**）。
    ///
    /// ⚠️ **推断**：`+0x60` 是个 4 值的状态枚举、`0x44f2b0` 返回 bool 的"还能不能起"；
    /// 两个语义都没有独立证据。缺的证据：`+0x60` 的写入点、`0x44f2b0` 的函数体。
    fn start_precondition(&mut self) -> bool;
    /// `0x51560b` / `0x5159c2` —— `call *0x9bc458` = **`0x517690`**（本模块 §5 的事件装配）。
    fn assemble_event_wiring(&mut self);
    /// `0x51564c` / `0x515a03` —— `call 0x3d5260`（`graphwar_event_handler.rs` 一族）。
    fn reset_event_handler(&mut self);
    /// `0x515658` / `0x515a0f` —— `call 0x469c70` = `GraphwarGameRecorder` 的 `add`（`docs/02` 表）。
    fn create_game_recorder(&mut self);
    /// `0x515666` / `0x515a1d` —— `call *0x9bc460` = **`0x518320`**（本模块 [`GraphwarGameClient::connect`]）。
    fn connect(&mut self);
    /// `0x5156be` / `0x515a73` —— 把装配出来的那块 0x40 字节写进 `self+0x1e0`（先 drop 旧值）。
    fn install_assembled(&mut self);
    /// `0x5156eb` / `0x515aa5` —— `call 0x514d80` = `GraphwarBackend::resubscribe_to_events`。
    fn resubscribe_to_events(&mut self);
    /// `0x5157d3 movabs $0x8000000000000016` + `0x5157e4 call *0x9bac80`（**只有 local 版**）
    /// ⇒ 再 push 一条 `GameNameRequest`（判别值 22）。
    fn request_game_name(&mut self, game_name: &str);
}

/// `0x515590` —— `GraphwarBackend::start_client_to_local_server`（真名 `hyb:高置信`）
/// （FDE `0x515590..0x5159a0`，0x410）。
///
/// 调用者：`0x3b2b84` / `0x3b363a`（root_node 命令总线，JoinLevelPopUp 路径先收串再进来）。
/// 返回 `false` = 被 `0x51559c`/`0x5155ad` 那道前置条件挡住（**推断**：两道守卫的语义未证）。
/// 档位：**观测**（步骤顺序与每一跳的地址）。
pub fn start_client_to_local_server<S>(steps: &mut S, game_name: &str) -> bool
where
    S: BackendStartSteps + ?Sized,
{
    // 0x515590 本函数起点；FDE 0x515590..0x5159a0
    if !steps.start_precondition() {
        // 0x5155b4 je 0x5157f3
        return false;
    }
    steps.assemble_event_wiring(); // 0x51560b → 0x517690
    steps.reset_event_handler(); // 0x51564c → 0x3d5260
    steps.create_game_recorder(); // 0x515658 → 0x469c70
    steps.connect(); // 0x515666 → 0x518320
    steps.install_assembled(); // 0x5156be → self+0x1e0
    steps.resubscribe_to_events(); // 0x5156eb → graphwar_backend 的 resubscribe_to_events
    steps.request_game_name(game_name); // 0x5157d3 → tag 22
    true
}

/// `0x5159a0` —— `GraphwarBackend::start_client_to_remote_server`（`hyb:高置信` 真名）
/// （FDE `0x5159a0..0x515b30`，0x190）。
///
/// 与 [`start_client_to_local_server`] 的**共尾**逐条相同，少掉两道前置守卫与那条
/// `GameNameRequest`（`0x515aaa` 之后直接收尾）。调用者：`0x3b3333`（命令总线）、`0x4f923c`
/// （`graphwar_lan_vbox`）。档位：**观测**。
pub fn start_client_to_remote_server<S>(steps: &mut S)
where
    S: BackendStartSteps + ?Sized,
{
    // 0x5159a0 本函数起点；FDE 0x5159a0..0x515b30
    steps.assemble_event_wiring(); // 0x5159c2 → 0x517690
    steps.reset_event_handler(); // 0x515a03 → 0x3d5260
    steps.create_game_recorder(); // 0x515a0f → 0x469c70
    steps.connect(); // 0x515a1d → 0x518320
    steps.install_assembled(); // 0x515a73 → self+0x1e0
    steps.resubscribe_to_events(); // 0x515aa5 → 同上一跳
}

// ---------------------------------------------------------------------------
// 12.9 回放时钟的两个门面（`0x3d6080` / `0x3d7630`）
// ---------------------------------------------------------------------------
//
// 这两条在 `fixtures/boundary_audit.tsv` 里记的是 `ours_file = src/graphwar_game_client.rs`
// （panic 主锚 `152:22`/`152:49` **在本文件**），另各引用一次
// `src/graphwar_replay_player.rs` 的 Location ⇒ 归属 contested（模块文档 §7 已记）。
// 真名来自 v2 符号锚：`GraphwarReplayPlayer::{toggle_pause,is_paused}`。
//
// 两条都**只**操作 `GraphwarTickHandler` 的 `+0x20`/`+0x28` 那一对字段
// （我方 [`GraphwarTickHandler::frozen_now`]；`+0x28 == 1e9` = "没设"哨兵）：
//
// ```text
// 3d60ad / 3d7659  call 0x46aa50                 ; get_module::<GraphwarTickHandler>()
// 3d60d9           mov  ebp,dword [rbx+0x40]      ; T+0x28 = frozen nanos
// 3d60dc           cmp  ebp,0x3b9aca00 ; jne …    ; == 1e9 ⇒ **没冻**（正在走）
// 3d60e4           call *0x9ba8f8 = clock ; 3d611b mov [rbx+0x38],rax / 3d611f mov [rbx+0x40],edx
//                                                 ⇒ 把 (secs,nanos) 写成 **now** = 冻结
// 3d60ec  （已冻分支）r14 = [rbx+0x38]；clock；(0x90cdd0) now−frozen = Duration；
//                     (0x90c180) 把这段 Duration **加到 base**（`lea rdi,[rbx+0x18]`）；
//                     3d6116 mov edx,0x3b9aca00 ⇒ 写回哨兵 = **解冻**
// 3d7674           mov  ebp,dword [rbx+0x40]
// 3d7685           cmp  ebp,0x3b9aca00 ; setne al  ⇒ **返回 (nanos != 1e9)**
// ```
//
// ⇒ 这是一对**暂停/恢复**：`is_paused()` = 覆盖值已设；`toggle_pause()` 在
// "没冻 ⇒ 冻成 now" 与 "已冻 ⇒ base += (now − frozen) 后解冻" 之间切。
// **顺带回答了一个旧 `未知`**：`src/graphwar_tick_handler.rs` 的模块文档写着
// "没有观测到原版哪个函数把 `+0x20`/`+0x28` 设成真时间" —— 那个写入者就是 `0x3d6080`
// 的 `0x3d611b`/`0x3d611f` 这两条 `mov`。
//
// ⚠️ `self` **不在** `rdi`：两条都从 **`rsi+0x40`** 取模块容器（`0x3d6087` / `0x3d7641`），
// `rdi` 全程未读。⇒ 真实签名至少是"第一个参数 + `&…`"，第一个参数在函数体里**没用到**。
// 缺的证据：调用点 `0x3b157c` / `0x3b15f7` 的实参形态（那两个也在 root_node 命令总线里）。

/// 两个 `(secs, nanos)` 时刻之差（纳秒）；`later < earlier` 时夹成 0。
///
/// 原版这段在 `0x3d6100 call *0x9baf28`（`0x90cdd0`）里，**含下溢 panic 路径**
/// （`Duration::checked_sub` 失败时那条 `0x94b340` 收尾前的分支）；这里夹成 0 是**我方选择**。
pub fn nanos_between(later: (u64, u32), earlier: (u64, u32)) -> u64 {
    let l = later.0 as u128 * 1_000_000_000 + later.1 as u128;
    let e = earlier.0 as u128 * 1_000_000_000 + earlier.1 as u128;
    l.saturating_sub(e).min(u64::MAX as u128) as u64
}

/// `(secs, nanos)` 加一个纳秒数（原版 `0x3d6110 call *0x9baf30` = `0x90c180(&T+0x0, dur)`）。
pub fn add_nanos(base: (u64, u32), d: u64) -> (u64, u32) {
    let t = base.0 as u128 * 1_000_000_000 + base.1 as u128 + d as u128;
    ((t / 1_000_000_000) as u64, (t % 1_000_000_000) as u32)
}




/// `GraphwarGameClient` 上的 8 条方法（T62 按真名/真类型落回来）。
///
/// 原版这 8 条都是 `impl GraphwarGameClient` 上的方法；本节把它们放回**类型下面**，
/// 这样函数边界与类型归属同时对上（`tools/function_coverage.py` 的 P1 要求「同名 + 同 impl 类型」）。
impl GraphwarGameClient {
    /// `0x516450` —— `GraphwarGameClient::get_module`（真名来自 v2 符号锚，`hyb:推断`）
    /// 的 **`GraphwarTickHandler` 单态化**（FDE `0x516450..0x5164ab`，0x5b）。
    ///
    /// 体：`borrow()` 容器 → `get::<GraphwarTickHandler>()` → **`unwrap()`** → 结束 `borrow` →
    /// 返回句柄。档位：**观测**（4 条指令序列 + 借用/panic 偏移 + 被调者 `0x46aa50`）。
    ///
    /// 唯一的调用者：`0x3b1c93`（root_node 命令总线，靠近 `save_clip` 分支）。
    pub fn get_module_tick_handler<G>(modules: &mut G) -> GraphwarTickHandler
    where
        G: ModuleGetter<GraphwarTickHandler> + ?Sized,
    {
        // 0x516450 本函数起点；FDE 0x516450..0x5164ab
        // 0x516473 call 0x46aa50 = get_module::<GraphwarTickHandler>()
        // 0x51647b/0x516490 = Option::unwrap 的 None 臂（panic 槽 0x9ba870）
        modules.get_module()
    }

    /// `0x5164b0` —— `GraphwarGameClient::get_module` 的 **`GraphwarGameRecorder` 单态化**
    /// （FDE `0x5164b0..0x51650b`，0x5b）。
    ///
    /// 与 [`get_module_tick_handler`] **逐条同形**，只差 `0x5164d3` 的被调者
    /// （`0x46aa50` ↔ `0x46aea0`）。档位：**观测**。
    ///
    /// 调用者：`0x3a707a`（root_node `0x3a5760` 一族）、`0x3b3f6c`。
    pub fn get_module_game_recorder<G>(modules: &mut G) -> GraphwarGameRecorder
    where
        G: ModuleGetter<GraphwarGameRecorder> + ?Sized,
    {
        // 0x5164b0 本函数起点；FDE 0x5164b0..0x51650b
        // 0x5164d3 call 0x46aea0 = get_module::<GraphwarGameRecorder>()
        modules.get_module()
    }

    /// `0x5175b0` —— `GraphwarGameClient::request_game_name_change`（真名来自 v2 符号锚）
    /// （FDE `0x5175b0..0x517684`，0xd4 = 212 B）。
    ///
    /// ```text
    /// 5175be  mov  rbp,(%rdi)                ; self.0 = 客户端状态（Rc）
    /// 5175c1  cmp  [rbp+0x10],0 ; jne 51766b ; RefCell::borrow_mut 冲突臂（槽 0x9ba6d0）
    /// 5175cf  mov  [rbp+0x10],-1             ; borrow_mut
    /// 5175db  test rdx,rdx ; jns 5175f1      ; name.len() < 0 ⇒ alloc 错误臂（0x91dd8c）
    /// 5175f6  call *0x9ba628 = __rust_alloc   ; 51760a call *0x9ba630 = alloc(1, len)
    /// 517621  call *0x9ba638 = memcpy        ; 把 (rsi, rdx) 复制成 String
    /// 51763e  movabs $0x8000000000000016      ; ⇒ tag 22 = GameNameRequest
    /// 517652  call *0x9bac80                  ; = Vec::push（队列在 [rbp+0x18]）
    /// 517658  inc  [rbp+0x10]                 ; 解除 borrow_mut
    /// ```
    ///
    /// 档位：**观测**（tag、alloc/memcpy、push、借用序列全部逐条可指）。
    /// 线格式 `{"GameNameRequest":{"game_name":"…"}}` 与 `WIRE_SPEC` 一致（`docs/07` §3.3，tag 22）。
    pub fn request_game_name_change<Q>(queue: &mut Q, game_name: &str)
    where
        Q: EventQueue + ?Sized,
    {
        // 0x5175b0 本函数起点；FDE 0x5175b0..0x517684
        // 0x5175cf = borrow_mut；0x51763e = tag 22；0x517652 = Vec::push
        queue.push(QueuedEvent {
            event: Event::GameNameRequest,
            payload: EventPayload::Name(game_name.to_string()),
        });
    }

    /// `0x517490` —— `GraphwarGameClient::request_lock_change`（FDE `0x517490..0x5175ac`，0x11c）。
    ///
    /// ```text
    /// 5174a5  mov  r12,[rdi+0x40]              ; 模块容器
    /// 5174c7  call 0x46be70                    ; get_module::<GraphwarGameStateManager>()
    /// 5174d4  dec  [r12+0x10]                  ; 结束容器 borrow
    /// 5174de  cmp  [rbx+0x10],0 ; jne panic    ; 句柄自己的 RefCell（槽 0x9ba6d0）
    /// 5174e5  mov  r12,(%r14)                  ; self.0 = 客户端状态
    /// 5174f0  movzx eax,byte [rbx+0x136]       ; ← 读那个 bool
    /// 5174f7  mov  [r12+0x10],-1               ; borrow_mut
    /// 517505  xor  al,1                        ; ⇒ **取反**
    /// 51750b  add  r15,0x45                    ; r15 = 0x7fff…ffff ⇒ tag 0x8000000000000044 = **68**
    /// 517519  call *0x9bac80                   ; = Vec::push（记录 {tag, bool}）
    /// 51751f  inc  [r12+0x10]                  ; 解除 borrow_mut
    /// ```
    ///
    /// 档位：**观测**（tag 68 / 取反 / 字段偏移）。线格式 `{"LockRequest":{"lock":bool}}`
    /// （`WIRE_SPEC`，`docs/07` §3.3；T53 官服抓包拿到 2 帧真 `LockRequest`）。
    ///
    /// 调用者：**只从虚表槽 `0x9bc160`** 被调用（`graphwar_setup_control` 的两处）。
    pub fn request_lock_change<E>(env: &mut E)
    where
        E: LockChangeEnv + ?Sized,
    {
        // 0x517490 本函数起点；FDE 0x517490..0x5175ac
        let lock = !env.locked_flag(); // 0x5174f0 读 + 0x517505 xor al,1
        env.push_event(QueuedEvent {
            event: Event::LockRequest,
            payload: EventPayload::Lock(lock),
        });
    }

    /// `0x5172d0` —— `GraphwarGameClient::send_chat_message`（FDE `0x5172d0..0x51748d`，0x1bd）。
    ///
    /// ```text
    /// 5172de  mov  rbx,rsi                     ; rsi = &ChatMessage（24 B，复制走）
    /// 51730a  call 0x46b180                    ; get_module::<GraphwarClientConnectionHandler>()
    /// 51732f  mov  [r14+0x10],-1               ; 句柄 borrow_mut
    /// 517337  cmp  byte [r14+0x18],0 ; je 51739d   ; **没连接 ⇒ 什么都不发，直接收尾**
    /// 51734c  mov  eax,dword [r14+0x1c]        ; entity_id
    /// 517350  mov  [r13+0x10],-1               ; 客户端状态 borrow_mut
    /// 51735c/517365  movups [rbx] → [rsp+0x18] ; String（24 B）落位
    /// 51736d  mov  [rsp+0x30],eax              ; entity_id 落 +0x20
    /// 517371  add  r12,0x47                    ; r12 = 0x7fff…ffff ⇒ tag 0x8000000000000046 = **70**
    /// 51737f  call *0x9bac80                   ; = Vec::push
    /// ```
    ///
    /// ⇒ **没连接时一条事件都不发**（`0x51733c je 0x51739d`）。档位：**观测**。
    /// 线格式 `{"ChatMessageRequest":{"entity_id":i64,"message":"…"}}`（`WIRE_SPEC`，tag 70；
    /// T53 官服抓包 1 帧真样本）。
    pub fn send_chat_message<E>(env: &mut E, message: &str) -> bool
    where
        E: ChatEnv + ?Sized,
    {
        // 0x5172d0 本函数起点；FDE 0x5172d0..0x51748d
        if !env.is_connected() {
            // 0x51733c je 0x51739d ⇒ 只做引用计数收尾，不 push
            return false;
        }
        let id = env.local_entity_id(); // 0x51734c
        env.push_event(QueuedEvent {
            // 0x517371 add r12,0x47 ⇒ tag 70 = ChatMessageRequest
            event: Event::ChatMessageRequest,
            payload: EventPayload::TextAndId {
                text: message.to_string(),
                id,
            },
        });
        true
    }

    /// `0x516ad0` —— `GraphwarGameClient::update_function`（FDE `0x516ad0..0x516fd0`，0x500）。
    ///
    /// 对**当前回合里的每个非电脑玩家** push 一条 `FunctionUpdateRequest`
    /// （`0x516df6 add r13,0x23` ⇒ tag `0x8000000000000022` = **34**），载荷 = `String` + `player_id`。
    /// 返回 push 了几条（**我方观测值**；原版没有返回值）。
    ///
    /// 档位：**观测**（tag 34、载荷落位、跳过分支）。线格式
    /// `{"FunctionUpdateRequest":{"player_id":i64,"function":"…"}}`（`WIRE_SPEC`，tag 34）。
    pub fn update_function<E>(env: &mut E, function: &str) -> usize
    where
        E: TurnPlayerEnv + ?Sized,
    {
        // 0x516ad0 本函数起点；FDE 0x516ad0..0x516fd0
        let mut pushed = 0;
        for player in env.current_turn_players() {
            // 0x516bc2 call 0x463340 得列表；0x516c84..0x516d13 逐人查电脑玩家表
            if env.is_computer_player(player) {
                continue; // 0x516cf0 je 0x516d30 ⇒ 命中即跳过
            }
            env.push_event(QueuedEvent {
                // 0x516df6 add r13,0x23 ⇒ tag 34 = FunctionUpdateRequest
                event: Event::FunctionUpdateRequest,
                payload: EventPayload::TextAndId {
                    text: function.to_string(),
                    id: player as i64,
                },
            });
            pushed += 1;
        }
        pushed
    }

    /// `0x516510` —— `GraphwarGameClient::fire_function`（FDE `0x516510..0x51696e`，0x45e）。
    ///
    /// **先**把函数文本转给 `0x516ad0`（`0x51652e call *0x9ba888`，槽里是 `0x516ad0`），
    /// **再**对当前回合里的每个非电脑玩家 push 一条 `FunctionFireRequest`
    /// （`0x5167a5 add r12,0x21` ⇒ tag `0x8000000000000020` = **32**，载荷只有一个 player id）。
    ///
    /// 档位：**观测**（两次取玩家列表 + 同一个跳过分支 + tag 32/34）。
    /// 线格式 `{"FunctionFireRequest":{"player_id":i64}}`（`WIRE_SPEC`，tag 32）。
    ///
    /// 调用者：`0x514acd`（命令入口 `0x514a10` 的 `fire` 臂）。
    pub fn fire_function<E>(env: &mut E, function: &str) -> FireOutcome
    where
        E: TurnPlayerEnv + ?Sized,
    {
        // 0x516510 本函数起点；FDE 0x516510..0x51696e
        // 0x51652e call *0x9ba888 ⇒ 0x516ad0（先把函数文本更新一条出去）
        let updated = Self::update_function(env, function);
        let mut fired = 0;
        for player in env.current_turn_players() {
            // 0x5165f6 call 0x463340（本条函数自己的第二次取列表）
            if env.is_computer_player(player) {
                continue; // 0x51671d je 0x516760 ⇒ 命中即跳过
            }
            env.push_event(QueuedEvent {
                // 0x5167a5 add r12,0x21 ⇒ tag 32 = FunctionFireRequest
                event: Event::FunctionFireRequest,
                payload: EventPayload::Id(player),
            });
            fired += 1;
        }
        FireOutcome { updated, fired }
    }

    /// `0x516fd0` —— `GraphwarGameClient::wait_for_players`（FDE `0x516fd0..0x5172c5`，0x2f5）。
    ///
    /// ```text
    /// 517020  <循环头>  取模块容器 → get conn_handler
    /// 51706c  cmp byte [rbx+0x18],0 ; je 0x517180     ; 没连接 ⇒ 这一圈只 process+sleep
    /// 517099  get_module::<GraphwarPlayerManager>()   ; 0x46a490
    /// 51708a  r12d = [conn_handler+0x1c]              ; 本地 player id
    /// 517118  call 0x3bf230                           ; 用 id 查 player 表
    /// 517155  test r15,r15 ; jne 0x5171ac              ; 查到了 ⇒ **返回**
    /// 517189  call rbp = 0x518400                      ; GraphwarGameClient::process
    /// 51718d  mov $0xf4240,%esi ; 517192 call r15      ; sleep 1 ms
    /// 5171a7  jmp 0x517020                              ; 回到循环头
    /// ```
    ///
    /// 档位：**观测**（三步的顺序、1 ms 立即数、`process` 的 GOT 槽）。语义一句话：
    /// **没等到人就地睡 1 ms，边睡边跑 `process` 一帧**（原版是**阻塞**式等待，不是异步）。
    /// 返回转了几圈（**我方观测值**；原版没有返回值）。
    pub fn wait_for_players<H>(host: &mut H) -> u32
    where
        H: WaitForPlayersHost + ?Sized,
    {
        // 0x516fd0 本函数起点；FDE 0x516fd0..0x5172c5
        let mut rounds = 0;
        loop {
            // 0x517020 循环头 → 0x517040 … 0x517118 查本地 player
            if host.local_player_present() {
                return rounds; // 0x517155 jne 0x5171ac ⇒ 返回
            }
            host.process_frame(); // 0x517189 call rbp = 0x518400
            host.sleep_one_millisecond(); // 0x51718d esi=0xf4240 → 0x517192 call 0x906f70
            rounds += 1;
        }
    }
}

/// `ReplayPlayer` 上的 2 条方法（`0x3d6080` / `0x3d7630`）。
///
/// ⚠️ **类型被改名**：原版真名是 `GraphwarReplayPlayer::toggle_pause` / `::is_paused`
/// （v2 符号锚），我方 `src/graphwar_replay_player.rs` 的类型叫 `ReplayPlayer`。
/// 这两条的 `panic::Location` 主锚落在**本文件**（`152:22` / `152:49`，与 `connect`/`disconnect`
/// 共用同一对辅助函数），`also_in` 才记 `graphwar_replay_player.rs` ⇒ 归属 contested，
/// 按 `fixtures/boundary_audit.tsv` 的 `ours_file` 落在本文件。
impl ReplayPlayer {
    /// `0x3d7630` —— `GraphwarReplayPlayer::is_paused`（FDE `0x3d7630..0x3d76ef`，0xbf）。
    ///
    /// `0x3d7685 cmp %ebp,0x3b9aca00` + `0x3d768b setne %al` ⇒
    /// 返回 **`frozen_now` 是否已设**。档位：**观测**。
    pub fn is_paused(handler: &GraphwarTickHandler) -> bool {
        // 0x3d7630 本函数起点；FDE 0x3d7630..0x3d76ef（0xbf）
        // 0x3d7659 call 0x46aa50 = get_module::<GraphwarTickHandler>()
        // 0x3d7674 mov ebp,dword [rbx+0x40] = T+0x28（frozen nanos）
        // 0x3d7685 cmp ebp,0x3b9aca00 ; 0x3d768b setne al
        handler.frozen_now.is_some()
    }

    /// `0x3d6080` —— `GraphwarReplayPlayer::toggle_pause`（FDE `0x3d6080..0x3d619e`，0x11e）。
    ///
    /// 切换 `GraphwarTickHandler` 的"`now` 覆盖值"，返回**切换后**是否处于暂停（**我方观测值**；
    /// 原版没有返回值 —— 它只改字段）。`now` 这一参数对应 `0x3d60e4`/`0x3d60f0`
    /// 那条 `call *0x9ba8f8`（原版取系统时钟；本层把它显式化以便可测）。
    /// 档位：**观测**（两条分支、`+0x38`/`+0x40` 落位、0x1e9 哨兵、base 偏移 `+0x18`）。
    pub fn toggle_pause(handler: &mut GraphwarTickHandler, now: (u64, u32)) -> bool {
        // 0x3d6080 本函数起点；FDE 0x3d6080..0x3d619e（0x11e）
        match handler.frozen_now {
            // 0x3d60dc cmp ebp,0x3b9aca00 ⇒ 没冻：把覆盖值写成 now（0x3d611b/0x3d611f）
            None => {
                handler.set_now(now.0, now.1);
                true
            }
            // 已冻：base += (now − frozen)，再写回哨兵 1e9 解冻（0x3d6100 → 0x3d6110 → 0x3d6116）
            Some(frozen) => {
                let paused_ns = nanos_between(now, frozen); // 0x90cdd0
                let base = add_nanos((handler.base_secs, handler.base_nanos), paused_ns); // 0x90c180
                handler.base_secs = base.0;
                handler.base_nanos = base.1;
                handler.clear_now(); // 0x3d6116 mov edx,0x3b9aca00
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 13. 单测（期望值全部来自 §12 各条反汇编里的立即数/分支，不来自实现本身）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod f3_landing_tests {
    use super::*;
    use std::cell::Cell;

    /// 记录 push 顺序的假队列。
    #[derive(Default)]
    struct Recorder {
        events: Vec<QueuedEvent>,
    }

    impl EventQueue for Recorder {
        fn push(&mut self, ev: QueuedEvent) {
            self.events.push(ev);
        }
    }

    // -- 12.1 7 条实例化 B ---------------------------------------------------

    /// 7 条的 tag 都是**指令里的立即数**（`0x4d8a4b` 那一族的 `movabs`）。
    #[test]
    fn instance_b_constructors_carry_the_immediate_tag() {
        let got = [
            (remove_soldier_request_event_b(7).event, Event::RemoveSoldierRequest, 0x11),
            (change_team_request_event_b(7).event, Event::ChangeTeamRequest, 0x0b),
            (turn_mode_change_request_event_b(7).event, Event::TurnModeChangeRequest, 0x14),
            (add_soldier_request_event_b(7).event, Event::AddSoldierRequest, 0x10),
            (remove_player_request_event_b(7).event, Event::RemovePlayerRequest, 0x0c),
            (time_mode_change_request_event_b(7).event, Event::TimeModeChangeRequest, 0x15),
            (axis_mode_change_request_event_b(7).event, Event::AxisModeChangeRequest, 0x12),
        ];
        for (ev, want, imm) in got {
            assert_eq!(ev, want);
            assert_eq!(ev.wire_tag(), imm, "{want:?} 的判别值必须是立即数 {imm:#x}");
        }
        // 参数原样落在 +0x08（`mov 0x8(%rdi),%eax`）
        assert_eq!(remove_soldier_request_event_b(-9).arg, -9);
        assert_eq!(remove_soldier_request_event_b(-9).tag_word(), 0x8000_0000_0000_0011);
    }

    /// B 组 7 条与 A 组 7 条是**同一源函数的两份实例化**：判别值必须逐条相同。
    #[test]
    fn instance_b_matches_instance_a_tag_for_tag() {
        let pairs = [
            (RequestKind::RemoveSoldier, remove_soldier_request_event_b(0).event),
            (RequestKind::ChangeTeam, change_team_request_event_b(0).event),
            (RequestKind::TurnMode, turn_mode_change_request_event_b(0).event),
            (RequestKind::AddSoldier, add_soldier_request_event_b(0).event),
            (RequestKind::RemovePlayer, remove_player_request_event_b(0).event),
            (RequestKind::TimeMode, time_mode_change_request_event_b(0).event),
            (RequestKind::AxisMode, axis_mode_change_request_event_b(0).event),
        ];
        for (kind, got) in pairs {
            assert_eq!(got, kind.event(), "{kind:?}");
        }
    }

    // -- 12.2 两条 get_module 单态化 -----------------------------------------

    /// 假模块容器：两条 `get` 各自返回不同模块，取错一个就红。
    struct FakeContainer {
        tick: Option<GraphwarTickHandler>,
        recorder: Option<GraphwarGameRecorder>,
        tick_calls: Cell<u32>,
        recorder_calls: Cell<u32>,
    }

    impl ModuleGetter<GraphwarTickHandler> for FakeContainer {
        fn get_module(&mut self) -> GraphwarTickHandler {
            self.tick_calls.set(self.tick_calls.get() + 1);
            self.tick
                .take()
                .unwrap_or_else(|| panic!("{MSG_OPTION_UNWRAP}"))
        }
    }

    impl ModuleGetter<GraphwarGameRecorder> for FakeContainer {
        fn get_module(&mut self) -> GraphwarGameRecorder {
            self.recorder_calls.set(self.recorder_calls.get() + 1);
            self.recorder
                .take()
                .unwrap_or_else(|| panic!("{MSG_OPTION_UNWRAP}"))
        }
    }

    /// 两条单态化各取**自己的**模块（`0x516473 call 0x46aa50` ↔ `0x5164d3 call 0x46aea0`）。
    #[test]
    fn two_get_module_monomorphizations_take_different_modules() {
        let mut h = GraphwarTickHandler::new(0, 0);
        h.current_tick = 41;
        // `GraphwarGameRecorder` 本轮按真值改成 **5 字段 / 0x38**（`docs/06` §131）：
        // 结构体里**没有** `saves` 计数（原版 0x38 = Vec+Weak+Rc+Weak+u8），也没有无参
        // `new`（原版 `new` 必带 3 个实参）⇒ 这里改用本 crate 自造的 `detached()`，
        // 并用"缓冲里有 1 条记录"当区分标志。**断言强度不变**（仍要求取到的是 recorder
        // 而不是 tick_handler）。
        let mut r = GraphwarGameRecorder::detached();
        r.record_game_state(7, 5);
        let mut c = FakeContainer {
            tick: Some(h),
            recorder: Some(r),
            tick_calls: Cell::new(0),
            recorder_calls: Cell::new(0),
        };
        assert_eq!(GraphwarGameClient::get_module_tick_handler(&mut c).current_tick, 41);
        assert_eq!(GraphwarGameClient::get_module_game_recorder(&mut c).len(), 1);
        assert_eq!(c.tick_calls.get(), 1);
        assert_eq!(c.recorder_calls.get(), 1);
    }

    /// 容器里没有 ⇒ panic（`0x51647b je 0x516490` 的 unwrap 臂）。
    #[test]
    #[should_panic(expected = "None")]
    fn missing_module_panics_like_the_original_unwrap() {
        let mut c = FakeContainer {
            tick: None,
            recorder: None,
            tick_calls: Cell::new(0),
            recorder_calls: Cell::new(0),
        };
        let _ = GraphwarGameClient::get_module_tick_handler(&mut c);
    }

    // -- 12.3 request_game_name_change --------------------------------------

    /// tag 22 + 载荷是那个串本身（`0x51763e` 的立即数 / `0x517621` 的 memcpy）。
    #[test]
    fn game_name_request_carries_the_name_and_tag_22() {
        let mut q = Recorder::default();
        GraphwarGameClient::request_game_name_change(&mut q, "US Room 1");
        assert_eq!(q.events.len(), 1);
        assert_eq!(q.events[0].event, Event::GameNameRequest);
        assert_eq!(q.events[0].tag_word(), 0x8000_0000_0000_0016);
        assert_eq!(q.events[0].payload, EventPayload::Name("US Room 1".into()));
        assert_eq!(q.events[0].payload.record_bytes(), 24);
    }

    // -- 12.4 request_lock_change -------------------------------------------

    struct FakeLockEnv {
        flag: bool,
        events: Vec<QueuedEvent>,
    }

    impl LockChangeEnv for FakeLockEnv {
        fn locked_flag(&self) -> bool {
            self.flag
        }
        fn push_event(&mut self, ev: QueuedEvent) {
            self.events.push(ev);
        }
    }

    /// `0x517505 xor %al,1`：载荷 = 那个字节的**取反**；tag 68（`0x51750b add r15,0x45`）。
    #[test]
    fn lock_request_negates_the_flag_and_uses_tag_68() {
        for (flag, want) in [(false, true), (true, false)] {
            let mut env = FakeLockEnv {
                flag,
                events: Vec::new(),
            };
            GraphwarGameClient::request_lock_change(&mut env);
            assert_eq!(env.events.len(), 1);
            assert_eq!(env.events[0].event, Event::LockRequest);
            assert_eq!(env.events[0].tag_word(), 0x8000_0000_0000_0044);
            assert_eq!(env.events[0].payload, EventPayload::Lock(want));
        }
    }

    // -- 12.5 send_chat_message ---------------------------------------------

    struct FakeChatEnv {
        connected: bool,
        entity_id: i64,
        events: Vec<QueuedEvent>,
    }

    impl ChatEnv for FakeChatEnv {
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn local_entity_id(&self) -> i64 {
            self.entity_id
        }
        fn push_event(&mut self, ev: QueuedEvent) {
            self.events.push(ev);
        }
    }

    /// 已连接：发一条 tag 70，`entity_id` 来自连接模块 `+0x1c`。
    #[test]
    fn chat_message_sends_one_event_with_local_entity_id() {
        let mut env = FakeChatEnv {
            connected: true,
            entity_id: 232,
            events: Vec::new(),
        };
        assert!(GraphwarGameClient::send_chat_message(&mut env, "hi"));
        assert_eq!(env.events.len(), 1);
        assert_eq!(env.events[0].event, Event::ChatMessageRequest);
        assert_eq!(env.events[0].tag_word(), 0x8000_0000_0000_0046);
        assert_eq!(
            env.events[0].payload,
            EventPayload::TextAndId {
                text: "hi".into(),
                id: 232
            }
        );
    }

    /// **没连接 ⇒ 一条都不发**（`0x51733c je 0x51739d`）。
    #[test]
    fn chat_message_sends_nothing_when_not_connected() {
        let mut env = FakeChatEnv {
            connected: false,
            entity_id: 232,
            events: Vec::new(),
        };
        assert!(!GraphwarGameClient::send_chat_message(&mut env, "hi"));
        assert!(env.events.is_empty());
    }

    // -- 12.6 update_function / fire_function -------------------------------

    struct FakeTurnEnv {
        players: Vec<i32>,
        computers: Vec<i32>,
        events: Vec<QueuedEvent>,
    }

    impl TurnPlayerEnv for FakeTurnEnv {
        fn current_turn_players(&mut self) -> Vec<i32> {
            self.players.clone()
        }
        fn is_computer_player(&mut self, player_id: i32) -> bool {
            self.computers.contains(&player_id)
        }
        fn push_event(&mut self, ev: QueuedEvent) {
            self.events.push(ev);
        }
    }

    /// 电脑玩家被**跳过**（`0x516cf0 je`），tag 34，载荷 = 串 + player_id。
    #[test]
    fn update_function_skips_computer_players_and_uses_tag_34() {
        let mut env = FakeTurnEnv {
            players: vec![3, 5, 7],
            computers: vec![5],
            events: Vec::new(),
        };
        assert_eq!(GraphwarGameClient::update_function(&mut env, "sin(x)"), 2);
        let ids: Vec<i64> = env
            .events
            .iter()
            .map(|e| match &e.payload {
                EventPayload::TextAndId { id, .. } => *id,
                other => panic!("载荷形状不对：{other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![3, 7], "5 是电脑玩家 ⇒ 跳过");
        for e in &env.events {
            assert_eq!(e.event, Event::FunctionUpdateRequest);
            assert_eq!(e.tag_word(), 0x8000_0000_0000_0022);
        }
    }

    /// `fire_function` = **先** update（tag 34）**再** fire（tag 32），两条链各自跳过电脑玩家。
    #[test]
    fn fire_function_updates_first_then_fires_tag_32() {
        let mut env = FakeTurnEnv {
            players: vec![3, 5],
            computers: vec![5],
            events: Vec::new(),
        };
        let out = GraphwarGameClient::fire_function(&mut env, "x");
        assert_eq!(out, FireOutcome { updated: 1, fired: 1 });
        let tags: Vec<u64> = env.events.iter().map(|e| e.tag_word()).collect();
        assert_eq!(
            tags,
            vec![0x8000_0000_0000_0022, 0x8000_0000_0000_0020],
            "先 FunctionUpdateRequest(34) 再 FunctionFireRequest(32)"
        );
        assert_eq!(env.events[1].payload, EventPayload::Id(3));
    }

    // -- 12.7 wait_for_players ---------------------------------------------

    /// 假宿主：`queries` 记"问了几次"，第 `present_after` 次之后就算"人到了"。
    struct FakeWait {
        /// 问过几次之后才"到"（0 = 第一次问就到了）。
        present_after: u32,
        queries: u32,
        trace: Vec<char>,
    }

    impl WaitForPlayersHost for FakeWait {
        fn local_player_present(&mut self) -> bool {
            self.trace.push('Q');
            let seen = self.queries;
            self.queries += 1;
            seen >= self.present_after
        }
        fn process_frame(&mut self) {
            self.trace.push('P');
        }
        fn sleep_one_millisecond(&mut self) {
            self.trace.push('S');
        }
    }

    /// 顺序是 **查 → process → sleep**（`0x517118` → `0x517189` → `0x517192`），
    /// 且查到人那一圈**不**再 process/sleep（`0x517155 jne 0x5171ac` 直接返回）。
    #[test]
    fn wait_for_players_polls_process_and_sleeps_one_ms() {
        // 前两圈没到 ⇒ Q P S | Q P S | 第三圈到 ⇒ Q 然后返回
        let mut w = FakeWait {
            present_after: 2,
            queries: 0,
            trace: Vec::new(),
        };
        assert_eq!(GraphwarGameClient::wait_for_players(&mut w), 2);
        assert_eq!(w.trace, vec!['Q', 'P', 'S', 'Q', 'P', 'S', 'Q']);
        // 一开始就有人 ⇒ 一圈都不转
        let mut w0 = FakeWait {
            present_after: 0,
            queries: 0,
            trace: Vec::new(),
        };
        assert_eq!(GraphwarGameClient::wait_for_players(&mut w0), 0);
        assert_eq!(w0.trace, vec!['Q']);
    }

    /// 没人来的话会一直转；这里只验"转 3 圈 = 3 次 process + 3 次 1 ms 睡"。
    #[test]
    fn wait_for_players_sleeps_once_per_round() {
        struct ThreeTimes(u32, Vec<char>);
        impl WaitForPlayersHost for ThreeTimes {
            fn local_player_present(&mut self) -> bool {
                self.1.push('Q');
                self.0 >= 3
            }
            fn process_frame(&mut self) {
                self.1.push('P');
                self.0 += 1;
            }
            fn sleep_one_millisecond(&mut self) {
                self.1.push('S');
            }
        }
        let mut t = ThreeTimes(0, Vec::new());
        assert_eq!(GraphwarGameClient::wait_for_players(&mut t), 3);
        assert_eq!(t.1.iter().filter(|c| **c == 'S').count(), 3);
        assert_eq!(t.1.iter().filter(|c| **c == 'P').count(), 3);
    }

    // -- 12.8 两条起客户端门面 ---------------------------------------------

    #[derive(Default)]
    struct FakeStart {
        log: Vec<&'static str>,
        precondition: bool,
    }

    impl BackendStartSteps for FakeStart {
        fn start_precondition(&mut self) -> bool {
            self.log.push("precondition");
            self.precondition
        }
        fn assemble_event_wiring(&mut self) {
            self.log.push("assemble");
        }
        fn reset_event_handler(&mut self) {
            self.log.push("reset");
        }
        fn create_game_recorder(&mut self) {
            self.log.push("recorder");
        }
        fn connect(&mut self) {
            self.log.push("connect");
        }
        fn install_assembled(&mut self) {
            self.log.push("install");
        }
        fn resubscribe_to_events(&mut self) {
            self.log.push("resubscribe");
        }
        fn request_game_name(&mut self, _name: &str) {
            self.log.push("game_name");
        }
    }

    /// local 版：precondition → 装配 → 重置 → recorder → connect → install → resubscribe →
    /// GameNameRequest。顺序就是二进制里的 `call` 顺序。
    #[test]
    fn start_local_server_runs_the_calls_in_binary_order() {
        let mut s = FakeStart {
            log: Vec::new(),
            precondition: true,
        };
        assert!(start_client_to_local_server(&mut s, "room"));
        assert_eq!(
            s.log,
            vec![
                "precondition",
                "assemble",
                "reset",
                "recorder",
                "connect",
                "install",
                "resubscribe",
                "game_name"
            ]
        );
    }

    /// 前置条件挡住 ⇒ **一步都不走**（`0x5155b4 je 0x5157f3`）。
    #[test]
    fn start_local_server_stops_when_precondition_fails() {
        let mut s = FakeStart {
            log: Vec::new(),
            precondition: false,
        };
        assert!(!start_client_to_local_server(&mut s, "room"));
        assert_eq!(s.log, vec!["precondition"]);
    }

    /// remote 版：没有前置守卫、没有 GameNameRequest，其余同序（`0x515aaa` 直接收尾）。
    #[test]
    fn start_remote_server_skips_the_local_only_steps() {
        let mut s = FakeStart {
            log: Vec::new(),
            precondition: true,
        };
        start_client_to_remote_server(&mut s);
        assert_eq!(
            s.log,
            vec!["assemble", "reset", "recorder", "connect", "install", "resubscribe"]
        );
    }

    // -- 12.9 回放时钟 ------------------------------------------------------

    /// 哨兵/取反：新建的 handler `frozen_now == None` ⇒ `is_paused` 假
    /// （`0x3d7685 cmp …0x3b9aca00` 的 `setne`）。
    #[test]
    fn is_paused_is_exactly_frozen_now_is_some() {
        let mut h = GraphwarTickHandler::new(0, 0);
        assert!(!ReplayPlayer::is_paused(&h));
        h.set_now(1_000, 5);
        assert!(ReplayPlayer::is_paused(&h));
        h.clear_now();
        assert!(!ReplayPlayer::is_paused(&h));
    }

    /// 第一次 `toggle_pause` = **冻结**到 now，base 不动；
    /// 第二次 = **解冻**，base 前进"暂停了多久"（`0x3d6100` → `0x3d6110`）。
    #[test]
    fn toggle_pause_freezes_then_shifts_the_base_by_the_pause_duration() {
        let mut h = GraphwarTickHandler::new(0, 0);
        h.base_secs = 100;
        h.base_nanos = 0;
        assert!(ReplayPlayer::toggle_pause(&mut h, (1_000, 250)), "切换后是暂停");
        assert_eq!(h.frozen_now, Some((1_000, 250)));
        assert_eq!((h.base_secs, h.base_nanos), (100, 0), "冻结不动 base");
        // 冻结在 (1000, 250)，现在 (1003, 1000) ⇒ 暂停了 3 s + 750 ns
        assert!(!ReplayPlayer::toggle_pause(&mut h, (1_003, 1_000)), "切换后是播放");
        assert_eq!(h.frozen_now, None, "解冻写回的是 1e9 哨兵");
        assert_eq!((h.base_secs, h.base_nanos), (103, 750), "base 前进暂停时长");
    }

    /// 纳秒进位的边界（`0x90cdd0` / `0x90c180` 这两跳的算术）。
    #[test]
    fn nanosecond_helpers_carry_properly() {
        assert_eq!(nanos_between((2, 0), (1, 0)), 1_000_000_000);
        assert_eq!(nanos_between((1, 5), (1, 9)), 0, "反序夹成 0");
        assert_eq!(add_nanos((1, 999_999_999), 1), (2, 0));
        assert_eq!(add_nanos((0, 0), 1_500_000_000), (1, 500_000_000));
    }
}
