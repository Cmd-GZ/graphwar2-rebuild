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
//! | `0x518240` / `0x518400` / `0x516450` / `0x5164b0` / `0x5172d0` / `0x517490` / `0x516970` | 都是「取 `+0x40` 句柄 → 递增引用 → 调内层一次 → 递减」，内层 `0x46aa50`/`0x46aea0`/`0x46b180` 未解 |
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
    OrigFn { vaddr: 0x516450, size: 0x60, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &[] },
    OrigFn { vaddr: 0x5164b0, size: 0x60, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(152, 22), (152, 49)], also_in: &[] },
    OrigFn { vaddr: 0x518240, size: 0xe0, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(139, 41), (139, 86), (140, 22)], also_in: &[] },
    OrigFn { vaddr: 0x518400, size: 0x130, class: Class::StateQuery, role: Role::Undetermined, anchors: &[(130, 41), (130, 86), (132, 28)], also_in: &[] },
    // --- setup 界面回调 ---
    OrigFn { vaddr: 0x516970, size: 0x160, class: Class::SetupCallback, role: Role::AddBotValue, anchors: &[(152, 22), (152, 49), (234, 57), (235, 30)], also_in: &[] },
    OrigFn { vaddr: 0x5172d0, size: 0x1c0, class: Class::SetupCallback, role: Role::Undetermined, anchors: &[(152, 22), (152, 49), (241, 57), (242, 30)], also_in: &[] },
    OrigFn { vaddr: 0x517490, size: 0x120, class: Class::SetupCallback, role: Role::Undetermined, anchors: &[(152, 22), (152, 49), (227, 40), (228, 26)], also_in: &[] },
];

/// 按地址查清单条目。
pub fn orig_fn(vaddr: u32) -> Option<&'static OrigFn> {
    ORIG_FUNCS.iter().find(|f| f.vaddr == vaddr)
}

/// 某一类里有多少个函数（`docs/16` §16 的分类表就是这张表算出来的）。
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
