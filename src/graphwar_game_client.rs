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
//! 见 [`ORIG_FUNCS`] 的 `class` / `role` 与 [`ENTRY_POINTS`] 的调用者。分档口径：
//!
//! * 有**具体证据**（键字面量 / 事件判别值 / 唯一调用者模块）支撑的，写一行结论；
//! * 只知道"被谁调、调了谁"的，写 `Role::Undetermined` + 调用图，**不**编语义。
//!
//! ## 7. 本文件**没有**做的事（诚实清单）
//!
//! | 缺口 | 缺什么证据 |
//! |---|---|
//! | `0x516510` / `0x516ad0` / `0x516fd0` 的语义 | 函数体未逐指令读完；`0x516510` 调了 `turn_manager` 一族（`0x46b740`/`0x46c430`/`0x46b460`/`0x463340`）并 push 一条 `Event`，但判别值走的是 `lea`+`add` 编码，未定位 |
//! | `0x515590` / `0x5159a0` / `0x5175b0` 的语义 | 只观测到「构造 `Event` #22 `game_name_request_event`」与「调用 `graphwar_backend.rs`」 |
//! | `0x516450` / `0x5164b0` / `0x5172d0` / `0x517490` / `0x516970` | 都是「取 `+0x40` 句柄 → 递增引用 → 调内层一次 → 递减」，内层 `0x46aa50`/`0x46aea0`/`0x46b180` 未解 |
//! | ~~`0x518240` / `0x518400`~~ **已解，见 §8** | 上面那一行只描述了这两个函数**前 8 条指令**（取句柄 → `get_module`）。真实主体：`0x518240` = **`GraphwarGameClient::tick`**（推进一个 tick）、`0x518400` = **`GraphwarGameClient::process`**（每帧进来一次、里面按 1/60 s **定步追赶**、每圈**先 tick 后泵**）。逐指令读法在 `docs/16-client.md` §48.3，落地在本文 §8。`ORIG_FUNCS` 里这两条的 `class` **仍记 `StateQuery`**（`tests/graphwar_game_client.rs` 把 4 条那个划分写死了，本任务不许改 `tests/`），但 `role` 已改成 `Role::Tick` / `Role::ProcessLoop` |
//! | `0x3d6080` / `0x3d7380` / `0x3d7630` | 同时引用本模块与 `graphwar_replay_player.rs` 的 Location（contested），归属本身有歧义 |
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
