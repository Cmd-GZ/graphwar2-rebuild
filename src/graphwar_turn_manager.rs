//! 回合推进：**射手轮转**（谁开火）、**队伍决定 `flip`**（朝哪边开火）、
//! 以及"回合超时 = 跳过该玩家"的分支。
//!
//! 证据分级沿用仓库约定：**逐位验证**（`cargo test` 覆盖）/ **观测**（能指到地址或字节）/
//! **推断**（写明未证实）/ **未知**。地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 射手由 `turn_manager` 轮转决定，AI **不选**射手（`docs/98`）
//!
//! 每个玩家一张 `Vec<soldier_id>` + 一个游标 `index`（`0x28` 步长的 HashMap entry）：
//!
//! ```text
//! index   = (index + 1) % len       ① 先自增再取（旧值不用）
//! soldier = vec[index]              ②
//! if life_state(soldier) & 1 { 发 TurnInfo{player, soldier}; 该玩家结束 }   ③④
//! 否则继续转，最多 len 次
//! 一个活的都没有 → 发 TurnInfo{player, 0}                                    ⑤
//! ```
//!
//! 本实现的地址级依据（用 objdump 逐条读的，不是抄文档）：
//!
//! | 行为 | 指令 | vaddr | 等级 |
//! |---|---|---|---|
//! | entry 步长 `0x28`、`len = entry-0x10` | `mov -0x10(%rdx,%rax,8),%r15`（`rax = 5k`） | `0x463f27` | 观测 |
//! | 循环次数 = `len` | `dec %r14` / `jne`（`r14 = r15`） | `0x463fc7`–`0x463fca` | 观测 |
//! | `index = (index+1) % len` 写回 | `inc` → `div %r15` → `mov %rdx,-0x8(%rbp)` | `0x463f40`–`0x463f58` | 观测 |
//! | 取士兵 `vec[index]` | `mov -0x18(%rbp),%rcx` / `mov (%rcx,%rdx,4),%ebx` | `0x463f5c`–`0x463fa6` | 观测 |
//! | 存活判定 = 调 `life_manager` 后 `test $0x1` | `call 454950` / `test $0x1,%al` / `jne` | `0x463fb6`–`0x463fbd` | 观测 |
//! | **`len == 0` 与"全灭"走同一个发 0 的块** | `test %r15,%r15` / `je 463fd0`；循环耗尽也落到 `463fd0` | `0x463f2f`、`0x463fca` → `0x463fd0` | 观测 |
//! | 发 `{player_id, soldier_id}` 事件（活着的士兵） | `mov -0x28(%rbp),%eax`→`+0x60`；`mov %ebx,0x64` | `0x46406b`–`0x464072` | 观测 |
//! | 发 `{player_id, 0}`（空 / 全灭） | `mov %ebx,0x60`；`movl $0x0,0x64` | `0x464016`–`0x464021` | 观测 |
//!
//! 交叉证据：`docs/98` §2 的三局 replay **逐发吻合**（本文三条测试原样复刻）。
//!
//! > ⚠️ **一处文档与二进制的字面冲突**：`docs/98` §7 的伪代码注释写
//! > `if n == 0 { return None; }  // 不发 TurnInfo`，而 §1.4 的观测说 `0x463da0` 的
//! > 分支对 `len == 0` **直接发 `TurnInfo{p, 0}`**。objdump 站 §1.4 这边：
//! > `0x463f2f` 的 `je 0x463fd0` 与"试满 len 次"落下的是**同一个块**，那块把
//! > `soldier_id` 写成 0 后发事件。所以广播层默认 [`EmptyRoster::AnnounceZero`]；
//! > [`EmptyRoster::Skip`] 保留给"按 §7 字面"的调用方。**两层语义分开**：
//! > [`PlayerTurns::next_shooter`] 对空 `Vec` 仍返回 `None`（`0x4628d0` getter 不 push，
//! > 见 `docs/98` §1.4 上表第一行），广播层再按策略决定发不发 0。
//!
//! ## 2. 存活判定：状态字节 `& 1`，查不到是 2
//!
//! `0x454950`（`graphwar_life_manager.rs`）返回**一个字节**，查不到时返回 **2**
//! （`docs/98` §1.3）；调用方只认最低位，所以 `0`（死）与 `2`（未上场/查不到）
//! 都算"不能开火"。这条规则落在 [`crate::graphwar_life_manager::is_alive_byte`]。
//!
//! ## 3. `flip` 由**队伍**决定，不看坐标（`docs/105`）
//!
//! Team1 恒 `x ≤ 0`、朝 `+x`（`flip = false`）；Team2 恒 `x ≥ 0`、朝 `-x`（`flip = true`）。
//! 773 个 `PosInfo` 样本 0 例外。注意本模块的接口里**没有任何坐标参数**——
//! 结构上就不可能"用起点/终点 x 比大小去猜"。
//!
//! ## 4. 回合超时 = **跳过该玩家**（⚠️ **用户口述，未独立验证**）
//!
//! 用户口述："回合超时 = 跳过该玩家"（`docs/HANDOVER.md` §7.10 第 2 条）。
//! 未在二进制或 replay 里独立验证过，所以这里只把它做成一个**显式分支**
//! （[`TurnDecision::Skip`] + [`SkipReason::TurnTimeout`]），不改其它语义。
//!
//! **仍未确定**（实现里**没有**替它做主）：
//!
//! * 超时跳过时，该玩家的士兵游标**是否推进**——本实现**不推进**（等于"这回合没轮到过他"）；
//! * 超时的那一刻，`TurnInfo` 是否**已经广播过**（若已广播，调用方拿到 `Skip` 时应忽略它）；
//! * 超时是"这个玩家跳过一次"还是"这个玩家的士兵也算用掉一次轮转"。
//!
//! ## 5. 仍未确定（不属本模块，但会喂进来）
//!
//! | # | 未知 | 谁负责 |
//! |---|---|---|
//! | A | 游标初值（`docs/98` §6-B）：三个 replay 例都吻合初值 0（首取 `soldiers[1]`），但 `1782763022` 首取 `2509` 不符 | 需回二进制/更多样本；本实现默认 0，另给 [`PlayerTurns::with_index`] |
//! | B | `Vec<soldier_id>` 的**填充顺序**（`docs/98` §6-C）：实测 = `SoldierInfo` 创建顺序，但没跟到 push 点；若来自 HashMap 遍历则每局不同 ⇒ 不可复现 | 调用方（`SoldierInfo` 的容器） |
//! | C | **玩家出场顺序**（`docs/47`）：判为 HashMap 迭代序 ⇒ 不可确定性复现。本模块**不管**玩家顺序，由调用方给 | 调用方 |
//! | D | `SequentialTurns` 与 `SimultaneousTurns` 两个分支的发射时序（`docs/98` §6-D） | 未解 |
//! | E | 空 `Vec` 到底走哪条分支（§1 上方那条冲突） | 需 REA 复核调用点；本实现给两个策略 |
//!
//! ## 6. 只读查询：`is_on_turn` 数的是**两个集合的并集**（任务 46，`观测`）
//!
//! `EntityDrawer::draw` 的门 2 是 `GraphwarTurnManager::is_on_turn`（vaddr **`0x462b50`**，
//! FDE `0x462b50..0x462c07`，调用点 `0x4b31ac`）。逐条读下来它**既不是**"当前回合方"，
//! **也不是**"本回合已开火"，而是**两个集合的成员判定**：
//!
//! ```text
//! 0x462b64  call 0x463460          ; get_current_turn_soldiers() → Vec<u32>（sret）
//! 0x462b85  cmp  [rdi+rcx], ebx    ; id == soldiers[i]？是 ⇒ 0x462bde（bl = 1）
//! 0x462bac  call 0x463340          ; get_current_turn_players()  → Vec<u32>
//! 0x462bd5  cmp  [rdi+rcx], ebx    ; id == players[i]？ 是 ⇒ 0x462bde（bl = 1）
//! 0x462bfe  xor  ebx, ebx          ; 两个集合都不中 ⇒ 返回 0
//! ```
//!
//! ⇒ **`is_on_turn(id)` ⇔ `id ∈ soldiers ∪ players`**（`观测`）。两个 getter 都是
//! `Mutex` 锁住 `TurnData` 后转发给 `GraphwarTurnData::get_current_turn_players`
//! （x86 `0x4627c0`）/ `get_current_turn_soldiers`（x86 `0x4628d0`），而这两个函数按
//! `TurnData[+0x58]`（回合模式位）分两支：
//!
//! | `TurnData[+0x58]` | players | soldiers | 证据 |
//! |---|---|---|---|
//! | `== 0`（顺序回合） | **单元素**：玩家表 `[+0x8]` 那个 `Vec` 的第 `[+0x18]` 个（当前玩家） | **单元素**：该玩家 entry 的 `soldiers[cursor]` | `0x462846`–`0x46286f` / `0x4629de`–`0x462ab0` |
//! | `!= 0`（同时回合） | 玩家表 `[+0x28]` 哈希表的**全部键**（`0x46282c call 0x3c03b0` 是 collect 适配器，取键在 `0x3c0484 mov eax,[r13+rax*8-0x28]`） | 每个 entry 的 `soldiers[cursor]`（`0x4629bc mov ebx,[rax+rdi*4]`；空 `Vec` 跳过 `0x46299f`） | `0x4627e9`–`0x462844` / `0x46290b`–`0x4629d3` |
//!
//! ⚠️ 任务 46 当时**只实现"同时回合"那一支**（[`TurnManager`] 是"每玩家一个游标"那张表，
//! 没有 `+0x58` mode 字段、也没有顺序回合那条"当前玩家游标"）⇒ 那时
//! [`TurnManager::is_on_turn`] 在 `SequentialTurns` 下是原版的**超集**。
//! **任务 50 补上了那一支**：[`TurnMode`]（`+0x58` 那个字节）+ 玩家**出场序表** +
//! 当前玩家游标 + [`TurnManager::set_turn`]（`0x4638f0`），见下面的 §7。
//!
//! ## 7. `TurnData[+0x58]` = `TurnMode` 字节；顺序支用"出场序表 + 当前玩家游标"（任务 50，`观测`）
//!
//! ### 7.1 那个字节是什么（`观测`）
//!
//! **`TurnData[+0x58]` 是一个 1 字节的 `TurnMode`**，两个变体（判别值就在字节里）：
//!
//! | 字节 | 变体 | 谁读 | 谁写 |
//! |---|---|---|---|
//! | `0` | `SequentialTurns` | 两个 getter：`0x4627e3` / `0x4628fc` `cmp byte [rsi+0x58],0` + `je` 进顺序支 | [`TurnManager::set_turn`] 不写它；`set_turn_mode` 写 |
//! | `1` | `SimultaneousTurns` | 同上（`!= 0` 进同时支） | 同上 |
//!
//! 证据链（全部 x86 2026-08-28，vaddr）：
//!
//! ```text
//! 0x462c10  get_turn_mode   : movzx eax, byte ptr [r14+0x70]   ; r14 = ArcInner ⇒ data+0x58
//! 0x462d20  set_turn_mode   : mov   byte ptr [r14+0x70], sil   ; 写同一个字节
//!                             0x462d9a movzx eax,[r14+0x70] / 0x462dae call add_event
//!                             ; ⇒ set_turn_mode 还把新值发一条事件（tag 0x2f = 47 = TurnModeInfo）
//! 0x3cbf70  TurnMode::serialize（serde）:
//!                             cmp byte ptr [r14], 1
//!                             0x3cbfd3 lea 'SimultaneousTurns'(17) / 0x3cbfe1 lea 'SequentialTurns'(15)
//!                             ⇒ **判别值 1 = SimultaneousTurns，0 = SequentialTurns**
//! 0x1c18d5  'SequentialTurns' / 0x1c18e4 'SimultaneousTurns'  ; 两串紧挨，且紧跟在
//!           0x1c18ad 'new_connection_event' / 0x1c18c1 'entity_removed_event' 之后
//!           （同模块 .rodata 簇；`GraphwarTurnManager::new` 0x4637fc 就取前两串）
//! ```
//!
//! 判别值的**语义**（哪一支是"顺序"）由两个 getter 自己给出：`== 0` 那一支只返回
//! **一个**玩家的 id 与**一个**士兵（下节），`!= 0` 那一支返回全部 —— 与
//! `graphwar_level_manager::TurnMode` 里 `SequentialTurns = 0` 的既有记录一致。
//!
//! 初值：**Windows 2.0.60**（同版本、同名函数 `GraphwarTurnData::new`，RVA `0x4a6f0`）
//! `mov byte ptr [rsi+0x58], 0` ⇒ 默认 `SequentialTurns`。ELF 侧的构造函数没逐位定位
//! （`TurnData` 由 `Arc<Mutex<…>>` 的分配路径里初始化，未单独确认），这一条标 **观测（跨构建）**。
//!
//! ### 7.2 顺序支返回什么（`观测`）
//!
//! ```text
//! get_current_turn_players  (rsi = &TurnData)
//!   0x462846 mov rax,[rsi+0x10]      ; 出场序表的 len（空 ⇒ 返回空 Vec，0x46284d je 0x462878）
//!   0x46284f mov rdi,[rsi+0x18]      ; **当前玩家游标**（usize）
//!   0x462853 cmp rdi,rax ; 0x462856 jae panic（index out of bounds）
//!   0x462858 mov rax,[rsi+8]         ; 出场序表的 ptr
//!   0x46285c mov ebp,[rax+rdi*4]     ; players[current] ⇒ 单元素
//!
//! get_current_turn_soldiers (rsi = &TurnData)
//!   0x4629de…0x4629fc               ; 同上取当前玩家 id（同一对 [+0x8]/[+0x10]/[+0x18]）
//!   0x462a03 cmp qword [rsi+0x40],0 ; 玩家表（HashMap）空 ⇒ 返回空 Vec
//!   0x462a6d cmp ebp,[rcx+r8*8-0x28]; 用玩家 id 查表
//!   0x462a9e mov rsi,[rax-0x10]     ; entry 的士兵表 len（空 ⇒ 返回空 Vec）
//!   0x462aa7 mov rdi,[rax-8]        ; **该玩家的士兵游标**
//!   0x462ab0 mov rax,[rax-0x18]     ; 士兵表 ptr
//!   0x462ab4 mov ebx,[rax+rdi*4]    ; soldiers[cursor] ⇒ 单元素
//! ```
//!
//! ⇒ **顺序支 = 「`players[current]`」+「`players[current]` 的 `soldiers[cursor]`」**，
//! 与同时支共用同一张玩家表（entry 形状也一样：士兵表 ptr/len/游标）。
//!
//! ### 7.3 那个游标是谁写的（`观测`）
//!
//! `GraphwarTurnManager::set_turn`（x86 **`0x4638f0`**，`rcx=self` / `esi=player_id` /
//! `edx=soldier_id`，都是 `u32`）——**它是顺序支那两个游标的唯一写者**：
//!
//! ```text
//! 0x463957/0x463970  在出场序表（data+0x8 ptr / +0x10 len）里线性找 player_id
//! 0x463a77/0x463a7a  找到第 rcx 个 ⇒ mov [rbx+0x30], rcx      ; data+0x18 = 当前玩家游标
//! 0x463997/0x4639a3  没找到 ⇒ 追加到表尾（len+1），游标 = 新位置（r13 = 旧 len）
//! 0x4639ab test r14d,r14d / je 0x463a8b                        ; soldier_id == 0 走"删记录"
//! 0x463a8b…0x463aaa  soldier_id == 0 ⇒ HashMap::remove(player)（0x486f20）⇒ **整条记录没了，
//!                    但出场序表里那一格不删**（下游因此不对称：players 仍有它、soldiers 没有）
//! 0x463ba0 cmp [rax+rcx*4], r14d ; 0x463c5d mov [r13-8], rcx   ; 士兵已在表里 ⇒ 游标 = 它的位置
//! 0x463bc6 mov [rax+rbp*4], r14d ; 0x463bd2 mov [r13-8], rbp   ; 否则追加，游标 = 新位置
//! 0x463c09 lea rax,[r14+0x2c] / 0x463c17 call add_event(0x3d3530)
//!                    ; ⇒ 发一条 {player_id, soldier_id}（tag 0x2b = 43 = **TurnInfo**）
//! ```
//!
//! 喂它的是远端事件处理器 **`0x3e2830`**（`graphwar_remote_event_handler`）的一支：
//! `0x3e3131 mov ebp,[rbx+8]` / `0x3e3134 mov r14d,[rbx+0xc]` / `0x3e3170 call 0x4638f0`
//! ⇒ 载荷就是 `TurnInfo{player_id, soldier_id}` 那两个 `u32`。
//!
//! `TurnMode` 的写者：`set_turn_mode`（`0x462d20`）被远端事件（`0x3e41ae`，**TurnModeInfo**）、
//! 游戏服（`0x4500c0`）与 `graphwar_game_state_manager` 的两态翻转（`0x476730`：
//! `get_turn_mode` → `xor al,1` → `set_turn_mode`）调用。
//!
//! ### 7.4 本层怎么建模 + **回落开关**（我方接线，写清差异）
//!
//! * [`TurnMode`] 是**本模块新定义的枚举**（`0`/`1` 与二进制一致）。`graphwar_level_manager`
//!   里已有一份同名同判别值的 `TurnMode`（关卡模式字节 `+0x3a`）；本轮**允许改的文件**
//!   不含那个模块，所以没有合并成一个类型 —— 这是**我方结构差异**，不是证据。
//!   Windows 符号里只有 `graphwar_turn_manager..TurnMode` 的 serde impl（`level_manager`
//!   没有），**旁证**该类型原本定义在本模块。
//! * [`TurnManager`] 加 `order`（出场序表）/`current`（游标）/`mode`（`+0x58`）。
//!   `order`/`current` **只由 [`TurnManager::set_turn`] 喂**（照二进制）；
//!   [`TurnManager::add_player`]（我方的轮转模型入口，二进制没有这个函数）**不喂它们**。
//! * **回落开关** `sequential_fallback`（默认 `true`）：顺序支拿不到"当前玩家"
//!   （出场序表空 / 游标越界，即 `set_turn` 还没被调用过）时，回落到同时支的全集
//!   = 任务 46 之前的旧行为。原版在这条状态上是**空集**（`0x46284d je` 直接返回空 Vec），
//!   所以把开关关掉就逐字回到二进制语义（`.with_sequential_fallback(false)`）。
//!   为什么默认开：本层的 `set_turn` 还没有真生产者（远端事件处理器没接），
//!   一把关掉会让画面"一具都不画"（`AGENTS.md` 合并纪律 3b 记过同类事故）。
//!
//! ### 7.5 仍是未知（诚实清单）
//!
//! | # | 项 | 档位 | 缺什么 |
//! |---|---|---|---|
//! | 1 | `set_turn` 那条事件的**转义/载荷编码** | 推断 | tag 由 `0x7fff…ffff + 0x2c` 反推为 43（`TurnInfo`），payload 两个 `u32` 观测；具体包装层（`add_event` 0x3d3530）未逐位解 |
//! | 2 | 出场序表的**遍历/使用**者（除顺序支 getter 外） | 未知 | 只观测到 `set_turn` 写、两个 getter 读 `[current]`；`next_turn`（0x463da0）走的是玩家表（HashMap），不是这张表 |
//! | 3 | `soldier_id == 0` 删记录后 `players` 仍返回已删玩家 | **观测**（0x463a8b vs 0x462846） | 原版就是这样不对称；是否真是"全灭即退场"的意图，未知 |

use std::collections::BTreeMap;

use crate::events::Event;
use crate::graphwar_life_manager::{is_alive_byte, LifeState, Status, MISSING_STATE};

/// "没有射手"的哨兵 id。
///
/// 真游戏的 `soldier_id` 从 1 起（`SoldierInfo` 里从未出现 0），所以 0 被用作
/// "全灭 / 空队"的哨兵（`docs/98` §1.4，replay `1793884702` 实测到 `TurnInfo{soldier_id: 0}`）。
/// [`PlayerTurns::add`] 会拒绝把 0 当真实士兵 id。
pub const SOLDIER_ID_NONE: i64 = 0;

// ---------------------------------------------------------------------------
// 回合模式（`TurnData[+0x58]`）
// ---------------------------------------------------------------------------

/// 是回合制还是同时回合制 —— **`TurnData[+0x58]` 那个字节**（`0x4627e3` / `0x4628fc`）。
///
/// 判别值与名字（`观测`，模块文档 §7.1）：
///
/// ```text
/// 0x3cbf70  TurnMode::serialize  : cmp byte [r14],1
///                                  == 1 ⇒ 'SimultaneousTurns'(17 B) @0x1c18e4
///                                  != 1 ⇒ 'SequentialTurns'  (15 B) @0x1c18d5
/// ```
///
/// 两个 getter 用的是 `cmp byte [rsi+0x58], 0` + `je` ⇒ **只区分「0」与「非 0」**，
/// 所以 [`TurnMode::from_byte`] 对任何非 0 都归到 [`TurnMode::SimultaneousTurns`]
/// （照二进制，不替它拒绝未知值）。
///
/// ⚠️ `graphwar_level_manager` 里也有一份同名同判别值的 `TurnMode`（关卡模式字节 `+0x3a`）。
/// Windows 符号里只有本模块这个类型带 serde impl，**旁证**原版只定义了一份；
/// 本轮允许改的文件不含那个模块，所以两份暂时并存（**我方结构差异**，模块文档 §7.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub enum TurnMode {
    /// 顺序回合：一回合只有"当前玩家 + 它当前的那个士兵"在回合内。判别值 `0`
    /// （= `GraphwarTurnData::new` 的初值，模块文档 §7.1）。
    #[default]
    SequentialTurns = 0,
    /// 同时回合：全部登记玩家 + 每人当前的那个士兵。判别值 `1`。
    SimultaneousTurns = 1,
}

impl TurnMode {
    /// 写进 `TurnData[+0x58]` 的那个字节。
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// 按二进制的判据读一个字节：**`0` = 顺序，非 `0` = 同时**（`0x4628fc cmp …,0`）。
    pub const fn from_byte(b: u8) -> Self {
        if b == 0 {
            TurnMode::SequentialTurns
        } else {
            TurnMode::SimultaneousTurns
        }
    }

    /// 只认 `0` / `1` 的严格版本（给"改模式"的验收入口用；二进制本身不校验）。
    pub const fn from_known_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(TurnMode::SequentialTurns),
            1 => Some(TurnMode::SimultaneousTurns),
            _ => None,
        }
    }

    /// 顺序回合？（顺序支 = 单玩家 + 单士兵）
    pub const fn is_sequential(self) -> bool {
        matches!(self, TurnMode::SequentialTurns)
    }
}

// ---------------------------------------------------------------------------
// 队伍
// ---------------------------------------------------------------------------

/// 一支队伍。只建模观测到的两方（`docs/105` §4：Team3/Team4 未观测到）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Team {
    /// `x ≤ 0`，朝 `+x` 开火。
    One,
    /// `x ≥ 0`，朝 `-x` 开火。
    Two,
}

impl Team {
    /// 协议里的队伍编号（`TeamInfo` / `Match::set_team` 用的是 `1`/`2`）。
    pub const fn id(self) -> u8 {
        match self {
            Team::One => 1,
            Team::Two => 2,
        }
    }

    /// 编号 → 队伍。**3 及以上返回 `None`**：`docs/105` §4 明确"未观测到四人以上队伍"，
    /// 与其猜一个方向，不如让调用方知道它落在已知模型之外。
    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Team::One),
            2 => Some(Team::Two),
            _ => None,
        }
    }

    /// 开火方向翻转位。**由队伍决定**，与坐标无关（`docs/105`，773 个位置样本 0 例外）。
    ///
    /// `flip = false` ⇒ 朝 `+x`；`flip = true` ⇒ 朝 `-x`（与 `graphwar_path_calculator::calculate` /
    /// `graphwar_shot_calculator::correction` 的 `flip` 参数同义）。
    pub const fn flip(self) -> bool {
        match self {
            Team::One => false,
            Team::Two => true,
        }
    }
}

// ---------------------------------------------------------------------------
// 一个玩家的轮转状态
// ---------------------------------------------------------------------------

/// 一个玩家的 `{ Vec<soldier_id>, index }`——`0x28` 步长那张表的 Rust 形态
/// （`docs/98` §1.1：key、`Vec` 指针、`len`、游标四个字段，已在 `0x463f27`–`0x463fa6`
/// 逐条核对）。
///
/// 顺序 = 这张 `Vec` 的存储顺序，**不是** id 排序、不是坐标、不是随机
/// （`docs/98` §2.4 逐条推翻）。`Vec` 里装谁、什么顺序由调用方决定（§5-B）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerTurns {
    /// 该玩家的士兵 id，顺序即轮转顺序。
    soldiers: Vec<i64>,
    /// 游标。语义是"**上一次**取到的位置"——下一次先自增再取。
    index: usize,
}

impl PlayerTurns {
    /// 游标从 0 起（`docs/98` §2 三个 replay 例都吻合；初值本身仍是未知，见 §5-A）。
    pub fn new(soldiers: Vec<i64>) -> Self {
        let mut p = Self { soldiers: Vec::new(), index: 0 };
        for id in soldiers {
            p.add(id);
        }
        p
    }

    /// 指定游标初值——`docs/98` §6-B 说初值未知（`1782763022` 首取不是 `soldiers[1]`），
    /// 所以留一个显式入口，默认值不被当成结论。
    pub fn with_index(soldiers: Vec<i64>, index: usize) -> Self {
        let mut p = Self::new(soldiers);
        p.index = index;
        p
    }

    /// 追加一个士兵。`soldier_id == 0` 是哨兵，不允许入表（会与"全灭"混淆）。
    pub fn add(&mut self, soldier_id: i64) {
        assert_ne!(
            soldier_id, SOLDIER_ID_NONE,
            "soldier_id 0 是\"全灭\"哨兵，不能作为真实士兵入表（docs/98 §1.4）"
        );
        self.soldiers.push(soldier_id);
    }

    /// **公告一个士兵**（`GraphwarTurnData::set_turn` 里那段，x86 `0x463ba0`–`0x463bd2`）：
    /// 已在表里 ⇒ 游标指到**它的位置**（`0x463c5d mov [r13-8], rcx`）；
    /// 不在 ⇒ 追加到表尾，游标 = 新位置（`0x463bc6` / `0x463bd2`）。
    ///
    /// 与 [`PlayerTurns::add`] 的区别：`add` 只追加（建表用），`announce` 会**去重**并
    /// 移动游标 —— 这正是二进制那条"同一玩家的士兵表会被反复公告同一批 id"的路径。
    /// 返回公告后游标的位置。
    pub fn announce(&mut self, soldier_id: i64) -> usize {
        assert_ne!(
            soldier_id, SOLDIER_ID_NONE,
            "soldier_id 0 是\"全灭\"哨兵：set_turn 对它走的是\"删记录\"分支（0x463a8b），不是入表"
        );
        match self.soldiers.iter().position(|&id| id == soldier_id) {
            Some(i) => self.index = i,
            None => {
                self.soldiers.push(soldier_id);
                self.index = self.soldiers.len() - 1;
            }
        }
        self.index
    }

    /// 士兵列表（顺序 = 轮转顺序）。
    pub fn soldiers(&self) -> &[i64] {
        &self.soldiers
    }

    pub fn len(&self) -> usize {
        self.soldiers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.soldiers.is_empty()
    }

    /// 当前游标。
    pub fn index(&self) -> usize {
        self.index
    }

    /// "当前回合的士兵"——即 `0x4628d0` 那个 getter 的语义。
    ///
    /// 它**不推进**游标，空表返回 `None`（`docs/98` §1.4 上表第一行：getter 对
    /// `len == 0` 不 push）。
    pub fn current(&self) -> Option<i64> {
        self.soldiers.get(self.index).copied()
    }

    /// 取**下一个**射手。返回值语义（`docs/98` §7，两者协议后果不同）：
    ///
    /// * `Some(id)`，`id != 0`：发 `TurnInfo{player_id, id}`；
    /// * `Some(`[`SOLDIER_ID_NONE`]`)` = `Some(0)`：**全灭**，仍要发
    ///   `TurnInfo{player_id, 0}`（`0x463fd0`，replay `1793884702` 实测到）；
    /// * `None`：`Vec` 为空 ⇒ 没有"当前士兵"。**广播与否由 [`TurnManager`] 决定**
    ///   （见 [`EmptyRoster`]）；这一层只如实回答"取不到"。
    ///
    /// `alive` 是"能开火吗"的谓词；要直接用 `life_manager` 的原始状态字节，
    /// 走 [`PlayerTurns::next_shooter_by_state`]。
    pub fn next_shooter(&mut self, alive: impl Fn(i64) -> bool) -> Option<i64> {
        let n = self.soldiers.len();
        if n == 0 {
            return None;
        }
        // 调用方可能在两次调用之间改过 Vec，先把游标拉回合法区间。
        if self.index >= n {
            self.index %= n;
        }
        // ① 先自增再取；③ 不活就继续转；最多 len 次（④）。
        for _ in 0..n {
            self.index = (self.index + 1) % n;
            let soldier = self.soldiers[self.index];
            if alive(soldier) {
                return Some(soldier);
            }
        }
        // ⑤ 试满 len 次仍无 → 全灭。
        Some(SOLDIER_ID_NONE)
    }

    /// 直接用 `life_manager` 的状态字节判活：**只有 `& 1` 为真才算活**
    /// （`0x463fbb` 的 `test $0x1,%al`；查不到返回 2 ⇒ 不可开火，`docs/98` §1.3）。
    pub fn next_shooter_by_state(&mut self, state: impl Fn(i64) -> u8) -> Option<i64> {
        self.next_shooter(|id| is_alive_byte(state(id)))
    }
}

// ---------------------------------------------------------------------------
// 空士兵表的两种处理
// ---------------------------------------------------------------------------

/// `Vec<soldier_id>` 为空时，广播层怎么做。
///
/// 这两个变体对应 `docs/98` 里**字面上不一致**的两处：
///
/// * [`EmptyRoster::AnnounceZero`]：`0x463da0` 的 `len == 0` 分支与"全灭"共用
///   `0x463fd0` 这个发 0 的块（objdump 核实：`0x463f2f` 的 `je 463fd0`）。
/// * [`EmptyRoster::Skip`]：`docs/98` §7 伪代码的注释
///   `if n == 0 { return None; } // 不发 TurnInfo`。
///
/// 默认按前者（二进制优先，见模块注释 §1 的冲突说明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmptyRoster {
    /// 仍发 `TurnInfo{player_id, 0}`（`0x463fd0`，观测）。
    #[default]
    AnnounceZero,
    /// 不发 `TurnInfo`（`docs/98` §7 伪代码的注释）。
    Skip,
}

// ---------------------------------------------------------------------------
// 一次"轮到该玩家"的判决
// ---------------------------------------------------------------------------

/// 为什么不发 `TurnInfo`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 这个 `player_id` 没登记过（调用侧的错，不该出现在正常流程里）。
    UnknownPlayer,
    /// 士兵表为空（`EmptyRoster::Skip` 时才会出现；`docs/98` §1.4 的"两种空"之一）。
    NoSoldiers,
    /// 回合超时——**用户口述**（`docs/HANDOVER.md` §7.10 第 2 条），**未独立验证**。
    TurnTimeout,
}

/// 轮到该玩家时的判决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDecision {
    /// 广播 `TurnInfo{player_id, soldier_id}`。`soldier_id == 0` 表示全灭/空队
    /// （同样要广播，见 [`EmptyRoster`]）。
    Announce {
        player_id: i64,
        soldier_id: i64,
        /// 这一回合的开火方向翻转位，由**队伍**决定（`docs/105`）。
        flip: bool,
    },
    /// 本回合不发 `TurnInfo`。
    Skip { player_id: i64, reason: SkipReason },
}

impl TurnDecision {
    /// 要广播的话是 `Some((player_id, soldier_id))`。
    ///
    /// 这个函数就是 `Some(0)` 与 `None` 的**协议区别**所在：
    /// 全灭时返回 `Some((p, 0))`（发），空队 + `EmptyRoster::Skip` 时返回 `None`（不发）。
    pub fn broadcast(self) -> Option<(i64, i64)> {
        match self {
            TurnDecision::Announce { player_id, soldier_id, .. } => Some((player_id, soldier_id)),
            TurnDecision::Skip { .. } => None,
        }
    }

    /// 这一回合的射手（不需要区分"发不发"时用它）。
    pub fn soldier_id(self) -> Option<i64> {
        self.broadcast().map(|(_, soldier_id)| soldier_id)
    }

    /// 这一回合的 `flip`；被跳过时是 `None`。
    pub fn flip(self) -> Option<bool> {
        match self {
            TurnDecision::Announce { flip, .. } => Some(flip),
            TurnDecision::Skip { .. } => None,
        }
    }

    pub fn skip_reason(self) -> Option<SkipReason> {
        match self {
            TurnDecision::Skip { reason, .. } => Some(reason),
            TurnDecision::Announce { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// 管理器：玩家 → (队伍, 轮转状态)
// ---------------------------------------------------------------------------

/// 一个玩家的登记项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    pub team: Team,
    pub turns: PlayerTurns,
}

/// 玩家 → `{队伍, Vec<soldier_id>, 游标}`。
///
/// **不**建模玩家出场顺序：`docs/47` 已判定那是 HashMap 迭代序（OS 熵播种的 SipHash），
/// 不可确定性复现，必须由调用方给。这里只用 `BTreeMap` 保证**玩家之间的遍历顺序确定**，
/// 免得同一个测试两次跑出两种顺序。
///
/// # 事件流（任务 40 补，`wired` 的第二件）
///
/// `new`（x86 `0x463790`，`src/graphwar_turn_manager.rs:133` 的锚点）**订阅 2 条**（`观测`）：
///
/// ```text
/// 0x4637fc  lea rsi,'new_connection_event' / 0x46380a mov edx,0x14 / 0x463815 call subscribe
///           r8 = slot 0x957be8 (+0x20 = 0x957c08 → 跳板 0x463750 → 0x462fe0 send_all_info_events)
/// 0x46384d  lea rsi,'entity_removed_event' / 0x46385b mov edx,0x14 / 0x463866 call subscribe
///           r8 = slot 0x957c10 (+0x20 = 0x957c30 → 跳板 0x463770 → 0x463580 handle_entity_removed_event)
/// ```
///
/// 发方向：`next_turn`（`0x463da0`）在"有活着的射手"分支发 `TurnInfo{player_id, soldier_id}`
/// （`0x46406b`–`0x464072`，见模块注释 §1 的表），全灭/空队分支发 `TurnInfo{player, 0}`
/// （`0x464016`–`0x464021`）——本文档开头那张表是**逐条指令**的证据，本节只把它接进发件箱。
///
/// `handle_entity_removed_event`（`0x463580`）**不发**任何事件（体内没有 `add_event`
/// 那槽 `0x9bac80` 的调用）：它断言 tag = 14（`0x46367f mov esi,0xe`），
/// 取 `event+8` 当 key（`0x4635a5`），lock 后 `hash(self+0x40)` + `remove_entry`，
/// 删到的值按 **4 字节元素**的 `Vec` 释放（`0x46362e shl rsi,2` / `0x463632 mov edx,4`）
/// ⇒ **整条玩家记录被删掉**（值里的 `Vec<soldier_id>` 一起释放），
/// 不是"从某个 Vec 里摘一个士兵"（`观测`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnManager {
    players: BTreeMap<i64, PlayerEntry>,
    /// 玩家**出场序表**（`TurnData[+0x8]` 指针 / `[+0x10]` len，x86 `0x462858` / `0x462846`）。
    ///
    /// 只由 [`TurnManager::set_turn`] 追加（二进制同：`0x463997` 写入、`0x46399f` 加 len），
    /// **去重**、保持 first-seen 顺序。`add_player` 不碰它（见我方的模型差异，模块文档 §7.4）。
    order: Vec<i64>,
    /// **当前玩家游标**（`TurnData[+0x18]`）：`order` 的下标，`set_turn` 每次都写它
    /// （`0x463a7a` 命中已有玩家 / `0x4639a3` 新追加）。顺序支的两个 getter 只读这一格。
    current: usize,
    /// `TurnData[+0x58]`：回合模式字节（初值 `0` = [`TurnMode::SequentialTurns`]）。
    mode: TurnMode,
    /// **我方开关**：顺序支拿不到"当前玩家"时，是否回落到同时支的全集。
    ///
    /// 原版在"出场序表空 / 游标越界"这条状态上返回**空集**（`0x46284d je 0x462878`）；
    /// 开关关掉就逐字回到那个语义。默认开 —— 本层的 `set_turn` 还没有真生产者
    /// （远端事件处理器 `0x3e2830` 没接），一把关掉会让画面一具都不画。模块文档 §7.4。
    sequential_fallback: bool,
    empty_roster: EmptyRoster,
    /// 订阅名（`new` 的两条，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 发件箱（判别值；载荷由 [`TurnDecision`] 返回）。
    pub outbox: Vec<Event>,
}

/// `new`（`0x463790`）订阅的两条事件名。
pub const SUBSCRIBED_EVENTS: [&str; 2] = ["new_connection_event", "entity_removed_event"];

impl Default for TurnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnManager {
    pub fn new() -> Self {
        Self {
            players: BTreeMap::new(),
            order: Vec::new(),
            current: 0,
            mode: TurnMode::default(),
            sequential_fallback: true,
            empty_roster: EmptyRoster::default(),
            subscribed: SUBSCRIBED_EVENTS.to_vec(),
            outbox: Vec::new(),
        }
    }

    /// `handle_entity_removed_event`（`0x463580`）：把该 id 的玩家记录整条删掉。
    ///
    /// 返回"真删到了"，对齐二进制里 `HashMap::remove` 的 `Option` 判别（`0x463659 cmp eax,2`）。
    /// **不发事件**（见类型文档）。id 空间按 `player_id` 用——这条与
    /// `graphwar_player_manager` 的同一处推断同源（实体删 = 该玩家退出）。
    pub fn handle_entity_removed_event(&mut self, entity_id: i64) -> bool {
        self.players.remove(&entity_id).is_some()
    }

    /// `send_all_info_events`（`0x462fe0`）：把当前玩家表整表重播一遍。
    ///
    /// ⚠️ **载荷是我方形状（`推断`）**：`0x462fe0` 里能确证的是"加锁遍历表 + 一条
    /// `add_event`（`0x46306a`）"，但**载荷构造没解**——它**不**调用
    /// `get_current_turn_players`(`0x463340`) / `get_current_turn_soldiers`(`0x463460`)
    /// （`addr_probe.py callers` 两个地址都没有 `0x462fe0` 的调用点）。
    /// 本实现按"每人一条 `TurnInfo{player_id, 该玩家当前射手}`"发，**别当证据用**。
    pub fn send_all_info_events(&mut self) -> Vec<TurnDecision> {
        // 先拍快照再写发件箱（同一个 `&mut self` 里不能边遍历边 push）
        let snapshot: Vec<(i64, i64, bool)> = self
            .players
            .iter()
            .map(|(&player_id, entry)| {
                (player_id, entry.turns.current().unwrap_or(SOLDIER_ID_NONE), entry.team.flip())
            })
            .collect();
        let mut out = Vec::with_capacity(snapshot.len());
        for (player_id, soldier_id, flip) in snapshot {
            self.outbox.push(Event::TurnInfo);
            out.push(TurnDecision::Announce { player_id, soldier_id, flip });
        }
        out
    }

    /// `new_connection_event` 的处理（跳板 `0x463750` → `0x462fe0`）：整表重播。
    pub fn handle_new_connection_event(&mut self) -> Vec<TurnDecision> {
        self.send_all_info_events()
    }

    /// 取走发件箱。
    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    /// 换掉"空士兵表"的策略（默认 [`EmptyRoster::AnnounceZero`]，理由见模块注释 §1）。
    pub fn with_empty_roster(mut self, policy: EmptyRoster) -> Self {
        self.empty_roster = policy;
        self
    }

    pub fn empty_roster(&self) -> EmptyRoster {
        self.empty_roster
    }

    /// 登记一个玩家。同一 `player_id` 再登记一次会**替换**（返回旧值）。
    pub fn add_player(&mut self, player_id: i64, team: Team, soldiers: Vec<i64>) -> Option<PlayerEntry> {
        self.players.insert(player_id, PlayerEntry { team, turns: PlayerTurns::new(soldiers) })
    }

    pub fn player(&self, player_id: i64) -> Option<&PlayerEntry> {
        self.players.get(&player_id)
    }

    /// 玩家 → `flip`。**只有队伍参与**，没有坐标参数（`docs/105` §3）。
    pub fn flip_for(&self, player_id: i64) -> Option<bool> {
        self.players.get(&player_id).map(|e| e.team.flip())
    }

    // -----------------------------------------------------------------------
    // 只读查询：`is_on_turn` 那条链（任务 46 读语义 / 任务 50 补顺序支）
    // -----------------------------------------------------------------------

    // -- 回合模式（`TurnData[+0x58]`，任务 50） ---------------------------------

    /// `GraphwarTurnManager::get_turn_mode`（x86 **`0x462c10`**）：读 `TurnData[+0x58]`
    /// （`0x462c4e movzx eax, byte ptr [r14+0x70]`，`r14` = `ArcInner` ⇒ data+`0x58`）。
    pub fn turn_mode(&self) -> TurnMode {
        self.mode
    }

    /// `GraphwarTurnManager::set_turn_mode`（x86 **`0x462d20`**）：写 `TurnData[+0x58]`
    /// （`0x462d6e mov byte ptr [r14+0x70], sil`），然后**发一条带该字节的事件**
    /// （`0x462d9a movzx eax,[r14+0x70]` → `0x462dae call add_event`）。
    ///
    /// 事件判别值由 payload 的 tag 字反推为 **`0x2f` = 47 = [`Event::TurnModeInfo`]**
    /// （`0x462da3 lea rax,[r13+0x30]`，`r13 = 0x7fff…ffff` ⇒ `0x8000…002f`）。
    /// payload 的形状（`{turn_mode: 字节}`）与 `src/_scaffold/protocol.rs` 的
    /// `TurnModeInfo` 一致（那一侧是字符串，不是这里发的字节——**转义层未逐位解**，§7.5 第 1 条）。
    pub fn set_turn_mode(&mut self, mode: TurnMode) {
        self.mode = mode;
        self.outbox.push(Event::TurnModeInfo);
    }

    /// 玩家**出场序表**（`TurnData[+0x8]/[+0x10]`，只由 [`TurnManager::set_turn`] 追加）。
    pub fn turn_order(&self) -> &[i64] {
        &self.order
    }

    /// 当前玩家游标指向谁（`TurnData[+0x18]` 那一格 `order[current]`）；表空 ⇒ `None`。
    pub fn current_player(&self) -> Option<i64> {
        self.order.get(self.current).copied()
    }

    /// 顺序支需要的那份状态**齐不齐**（模式 = 顺序 **且** 游标指得到一个玩家）。
    ///
    /// `false` 有两种可能：① 现在是同时回合；② 顺序回合但没人 `set_turn` 过
    /// （`order` 空 / 游标越界）—— 后者就是回落开关起作用的那条路径。
    pub fn sequential_state_available(&self) -> bool {
        self.mode.is_sequential() && self.current_player().is_some()
    }

    /// 回落开关当前值（默认 `true`，见模块文档 §7.4）。
    pub fn sequential_fallback(&self) -> bool {
        self.sequential_fallback
    }

    /// 换回落开关（`false` = 逐字二进制：顺序支缺状态时返回空集）。
    pub fn with_sequential_fallback(mut self, on: bool) -> Self {
        self.sequential_fallback = on;
        self
    }

    /// 就地改回落开关，返回改完的值。
    pub fn set_sequential_fallback(&mut self, on: bool) -> bool {
        self.sequential_fallback = on;
        on
    }

    /// `GraphwarTurnData::set_turn`（x86 **`0x4638f0`**）：公告"这个玩家现在用这个士兵"。
    ///
    /// 这是**顺序支两个游标的唯一写者**（模块文档 §7.3）：
    ///
    /// 1. 玩家不在出场序表里 ⇒ 追加到表尾（`0x463997`/`0x46399f`）；在 ⇒ 取它那格；
    /// 2. **当前玩家游标 = 那个位置**（`0x463a7a` / `0x4639a3`，`TurnData[+0x18]`）；
    /// 3. `soldier_id == 0` ⇒ 把该玩家的**整条记录删掉**（`0x463a8b` → `HashMap::remove`
    ///    `0x486f20`），**出场序表里那一格不删**（观测：下游因此不对称）；
    ///    否则把士兵公告进该玩家的表（去重 + 游标，见 [`PlayerTurns::announce`]）；
    /// 4. 发一条 `TurnInfo{player_id, soldier_id}`（`0x463c09` tag `0x2b` = 43 + `add_event`）。
    ///
    /// ⚠️ 我方差异：二进制这条记录里**没有队伍**（队伍在 `player_manager` 里），本层把
    /// `team` 挂在同一条 [`PlayerEntry`] 上 ⇒ 未知玩家按 [`Team::One`] 建记录。
    /// 这不是证据，只是模型限制（模块文档 §7.4）。
    pub fn set_turn(&mut self, player_id: i64, soldier_id: i64) {
        // ① 出场序表：命中取位置，否则追加到表尾（0x463970 / 0x463997）
        let pos = match self.order.iter().position(|&id| id == player_id) {
            Some(i) => i,
            None => {
                self.order.push(player_id);
                self.order.len() - 1
            }
        };
        // ② 当前玩家游标 = 该位置（0x463a7a / 0x4639a3）
        self.current = pos;
        // ③ 士兵：0 ⇒ 整条记录删掉；否则公告（0x463a8b / 0x463ba0–0x463bd2）
        if soldier_id == SOLDIER_ID_NONE {
            self.players.remove(&player_id);
        } else {
            let entry = self.players.entry(player_id).or_insert_with(|| PlayerEntry {
                team: Team::One,
                turns: PlayerTurns::new(Vec::new()),
            });
            entry.turns.announce(soldier_id);
        }
        // ④ 发 TurnInfo{player_id, soldier_id}（0x463c09–0x463c17）
        self.outbox.push(Event::TurnInfo);
    }

    /// `GraphwarTurnData::get_current_turn_players`（x86 `0x4627c0`）：本回合**玩家 id** 集合。
    ///
    /// 二进制按 `TurnData[+0x58]` 分两支（`观测`）：
    ///
    /// * **`== 0`（顺序回合，`0x462846`–`0x46286f`）**：**单元素** = 出场序表
    ///   `[+0x8]/[+0x10]` 的第 `[+0x18]`（当前玩家游标）个。表空 ⇒ 空 `Vec`（`0x46284d je`）。
    /// * **`!= 0`（同时回合，`0x4627e9`–`0x462844`）**：玩家表（HashMap）的**全部键**
    ///   （collect 适配器 `0x46282c call 0x3c03b0`，取键在 `0x3c0484`）。
    ///
    /// 顺序支缺状态（没人 `set_turn` 过）时本层按 [`TurnManager::sequential_fallback`]
    /// 决定回落（默认回落到同时支全集）——原版那里是**空集**。模块文档 §7.4。
    pub fn get_current_turn_players(&self) -> Vec<i64> {
        if !self.mode.is_sequential() {
            return self.players.keys().copied().collect();
        }
        if let Some(current) = self.current_player() {
            // 照二进制：单个元素，**不**检查它还在不在玩家表里（删记录后仍在出场序表）
            return vec![current];
        }
        if self.sequential_fallback {
            self.players.keys().copied().collect()
        } else {
            Vec::new()
        }
    }

    /// `GraphwarTurnData::get_current_turn_soldiers`（x86 `0x4628d0`）：本回合**士兵 id** 集合。
    ///
    /// * **同时回合那一支（`0x46290b`–`0x4629d3`）**：遍历玩家表，每个 entry 取 `soldiers[cursor]`
    ///   （`0x4629bc mov ebx,[rax+rdi*4]`，`rdi` = `[entry-8]` 的游标），空 `Vec` 跳过
    ///   （`0x46299f test rsi,rsi / je`）⇒ 每个玩家最多一个，与 [`PlayerTurns::current`] 同义。
    /// * **顺序回合那一支（`0x4629de`–`0x462ab4`）**：先取当前玩家，再在玩家表里查它
    ///   （`0x462a6d`），然后取它 `soldiers[cursor]`。查不到该玩家 ⇒ **空 `Vec`**
    ///   （删记录那条路径，§7.3 第 3 条）——**不**回落到别的玩家。
    pub fn get_current_turn_soldiers(&self) -> Vec<i64> {
        if !self.mode.is_sequential() {
            return self.players.values().filter_map(|entry| entry.turns.current()).collect();
        }
        if let Some(current) = self.current_player() {
            return self
                .players
                .get(&current)
                .and_then(|entry| entry.turns.current())
                .into_iter()
                .collect();
        }
        if self.sequential_fallback {
            self.players.values().filter_map(|entry| entry.turns.current()).collect()
        } else {
            Vec::new()
        }
    }

    /// `GraphwarTurnManager::is_on_turn`（x86 **`0x462b50`**，FDE `0x462b50..0x462c07`；
    /// 调用点：`EntityDrawer::draw` 的 `0x4b31ac`）。
    ///
    /// **判据 = `id ∈ get_current_turn_soldiers() ∪ get_current_turn_players()`**（`观测`）：
    /// 先查 soldiers（`0x462b64` 取、`0x462b85` 比），不中再查 players（`0x462bac` 取、
    /// `0x462bd5` 比），命中返回 `1`，都不中返回 `0`（`0x462bfe xor ebx,ebx`）。
    ///
    /// ⚠️ 语义**不是**"当前回合方"，也**不是**"本回合已开火"：同时回合下**所有登记玩家**与
    /// **每个玩家的当前士兵**都为真；顺序回合下只有**当前玩家 + 它当前那个士兵**为真
    /// （任务 50 已按二进制建模，含 `+0x58` 模式；见模块文档 §7）。
    ///
    /// 实例 id 域：二进制参数是 `u32`（`esi`），本层实体 id 是 `i64`；调用方负责域转换
    /// （`EntityDrawer` 侧按 `u32` 转换，越界 = 不通过）。
    pub fn is_on_turn(&self, entity_id: i64) -> bool {
        // 顺序照二进制：先 soldiers（0x462b64），再 players（0x462bac）。
        if self.get_current_turn_soldiers().iter().any(|&id| id == entity_id) {
            return true;
        }
        self.get_current_turn_players().iter().any(|&id| id == entity_id)
    }

    /// 直接推进该玩家的游标（不产生判决）。未知玩家返回 `None`。
    pub fn next_shooter(&mut self, player_id: i64, state: impl Fn(i64) -> u8) -> Option<i64> {
        self.players.get_mut(&player_id)?.turns.next_shooter_by_state(state)
    }

    /// 本回合轮到 `player_id`：决定"发不发 `TurnInfo`、发给谁、朝哪边"。
    ///
    /// * `state`：`life_manager` 的状态字节（`& 1` 为真才算活，`docs/98` §1.3）；
    /// * `timed_out`：回合是否超时。**用户口述**"超时 = 跳过该玩家"，未独立验证，
    ///   且**不推进游标**（见模块注释 §4）。
    ///
    /// 注意：它**不**产生 `FunctionFire`。全灭时 `soldier_id = 0`，真游戏也没有
    /// `FunctionFire`（`docs/98` §1.4）。
    pub fn begin_turn(
        &mut self,
        player_id: i64,
        state: impl Fn(i64) -> u8,
        timed_out: bool,
    ) -> TurnDecision {
        let policy = self.empty_roster;
        let Some(entry) = self.players.get_mut(&player_id) else {
            return TurnDecision::Skip { player_id, reason: SkipReason::UnknownPlayer };
        };
        let flip = entry.team.flip();
        // 超时分支（用户口述）：连射手都不选，游标原地不动。
        if timed_out {
            return TurnDecision::Skip { player_id, reason: SkipReason::TurnTimeout };
        }
        match entry.turns.next_shooter_by_state(state) {
            Some(soldier_id) => {
                self.outbox.push(Event::TurnInfo);
                TurnDecision::Announce { player_id, soldier_id, flip }
            }
            None => match policy {
                // 观测（0x463fd0）：空表也发 {player, 0}。
                EmptyRoster::AnnounceZero => {
                    self.outbox.push(Event::TurnInfo);
                    TurnDecision::Announce { player_id, soldier_id: SOLDIER_ID_NONE, flip }
                }
                EmptyRoster::Skip => TurnDecision::Skip { player_id, reason: SkipReason::NoSoldiers },
            },
        }
    }
}

/// 把 [`LifeState`] 适配成"状态字节查询"。
///
/// 表里没有记录时返回 [`MISSING_STATE`]（= 2，`0x454950` 查不到时的返回值），
/// 于是它和"死"一样不可开火（`docs/98` §1.3）。
pub fn life_state_of(life: &LifeState) -> impl Fn(i64) -> u8 + '_ {
    move |soldier_id: i64| {
        match u32::try_from(soldier_id).ok().and_then(|id| life.status(id)) {
            Some(Status::Alive) => 1,
            Some(Status::Dead) => 0,
            None => MISSING_STATE,
        }
    }
}

// ---------------------------------------------------------------------------
// 同时回合制的**收集阶段**（`docs/141` §1）
// ---------------------------------------------------------------------------

/// 一次提交的受理结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitResult {
    /// 收下了。
    Accepted,
    /// 这个人已经交过，后交的被丢弃（一人一发，`docs/86`：一回合一个士兵只出一次手）。
    Duplicate,
    /// 这个人不在这回合的公告里（没上场 / 全灭 / 记错 id）。
    NotExpected,
    /// 这一轮已经结算过了，不再受理。
    Closed,
}

/// 同时回合制的**收集阶段**：攒齐"本回合每个能开火的玩家各一发"，或超时。
///
/// 为什么需要它：`SequentialTurns` 一回合只有一发，轮转给出射手后**立刻**就能结算；
/// `SimultaneousTurns` 一回合是 **N 发一个事务**（130 个真回放批，2~8 发），必须先把
/// 提交攒起来。`docs/141` §1 把这一段拆成"收集 / 结算 / 广播"三阶段，这里只负责第一段：
/// **它不碰几何、不碰报文**，所以可以单独测。
///
/// 顺序语义（`观测`，`docs/141` §4，130/130 批）：
///
/// > 批内射击者的顺序 = 本回合 `TurnInfo` 的**公告顺序**，取"实际提交了函数"的那些。
///
/// 所以 [`RoundCollection::resolve`] 按**公告顺序**交还，而不是按提交到达顺序 ——
/// 后者是网络竞态（B 类），前者是输入决定的。公告顺序本身来自 `turn_manager`
/// 遍历玩家表（HashMap 迭代序，`docs/113` 的 A 类），本类型**照抄调用方给的顺序**。
pub struct RoundCollection<T> {
    /// 本回合的公告顺序（`TurnInfo` 里 `soldier_id != 0` 的那些玩家，按公告先后）。
    announced: Vec<i64>,
    /// 已收到的提交，键 = `player_id`。
    got: std::collections::BTreeMap<i64, T>,
    /// 回合截止 tick（`EndOfTurnInfo.end_of_turn_tick`）。`None` = 不计时。
    ///
    /// ⚠️ 真值里这个字段实测恒为 **0**（`EndOfTurnInfo{end_of_turn_tick:0, time_mode:"TimerInf"}`
    /// 在 `1793884702` 的每一回合都是 0），而 60Hz tick 在回合开始时就已上万 ——
    /// 所以**不能用它当截止**。本结构的 `deadline` 是调用方给的**墙钟/tick 预算**，
    /// 与那个字段无关。超时语义只做过"用户口述"级别的核对（模块注释 §4）。
    deadline: Option<i64>,
    closed: bool,
}

impl<T> RoundCollection<T> {
    /// `announced` = 本回合拿到 `TurnInfo{player_id, soldier_id != 0}` 的玩家，**按公告顺序**。
    pub fn new(announced: impl IntoIterator<Item = i64>, deadline: Option<i64>) -> Self {
        RoundCollection {
            announced: announced.into_iter().collect(),
            got: std::collections::BTreeMap::new(),
            deadline,
            closed: false,
        }
    }

    /// 本回合在等谁（公告顺序）。
    pub fn announced(&self) -> &[i64] {
        &self.announced
    }

    /// 还没交的玩家（公告顺序）。
    pub fn waiting_on(&self) -> Vec<i64> {
        self.announced
            .iter()
            .copied()
            .filter(|p| !self.got.contains_key(p))
            .collect()
    }

    /// 已经交了几发。
    pub fn len(&self) -> usize {
        self.got.len()
    }

    pub fn is_empty(&self) -> bool {
        self.got.is_empty()
    }

    /// 收一发。
    pub fn submit(&mut self, player_id: i64, value: T) -> SubmitResult {
        if self.closed {
            return SubmitResult::Closed;
        }
        if !self.announced.contains(&player_id) {
            return SubmitResult::NotExpected;
        }
        if self.got.contains_key(&player_id) {
            return SubmitResult::Duplicate;
        }
        self.got.insert(player_id, value);
        SubmitResult::Accepted
    }

    /// 都交了？（空公告时算"齐了"，因为没人在等。）
    pub fn is_complete(&self) -> bool {
        self.announced.iter().all(|p| self.got.contains_key(p))
    }

    /// 到点了吗。
    pub fn expired(&self, now_tick: i64) -> bool {
        matches!(self.deadline, Some(d) if now_tick >= d)
    }

    /// 现在该不该结算：**攒齐**或**超时**，二者之一。
    ///
    /// 两者都没有 ⇒ 继续等。这就是"等所有存活玩家提交（或超时）"的全部逻辑。
    /// 已经 [`RoundCollection::close`] 过的轮次不再要求结算（调用方已经收走了）。
    pub fn should_resolve(&self, now_tick: i64) -> bool {
        !self.closed && (self.is_complete() || self.expired(now_tick))
    }

    /// 提前关掉这一轮（例如对局结束 / 被外部取消）。之后 [`RoundCollection::submit`]
    /// 返回 [`SubmitResult::Closed`]，[`RoundCollection::should_resolve`] 恒 `false`。
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// 这一轮关掉了吗。
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// 结束收集，按**公告顺序**交还本回合真正提交了的那些（`(player_id, value)`）。
    ///
    /// 没交的玩家**不出现在返回值里** —— 真回放里正是这样：`TurnInfo` 公告了 3 个人、
    /// 结算批只有 2 发（`docs/141` §4 的 9 个反例，全部由"公告了但没交"造成）。
    pub fn resolve(mut self) -> Vec<(i64, T)> {
        self.closed = true;
        let mut out = Vec::with_capacity(self.got.len());
        for p in &self.announced {
            if let Some(v) = self.got.remove(p) {
                out.push((*p, v));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 用"活着的士兵集合"造状态字节查询：集合里是 1，不在里面是 0（死）。
    fn alive_set(ids: &[i64]) -> impl Fn(i64) -> u8 + '_ {
        let set: BTreeSet<i64> = ids.iter().copied().collect();
        move |id: i64| u8::from(set.contains(&id))
    }

    /// 一直活着。
    fn all_alive(_: i64) -> u8 {
        1
    }

    // -----------------------------------------------------------------------
    // ① 轮转顺序：先自增再取
    // -----------------------------------------------------------------------

    /// `docs/98` §1.2 第 ① 步：游标**先自增**，所以初值 0 时首取 `soldiers[1]`。
    #[test]
    fn cursor_increments_before_reading() {
        let mut p = PlayerTurns::new(vec![10, 20, 30]);
        assert_eq!(p.index(), 0);
        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(p.next_shooter_by_state(all_alive).unwrap());
        }
        assert_eq!(got, vec![20, 30, 10, 20], "先自增再取，且按 len 回绕");
        assert_eq!(p.index(), 1, "游标写回的是取到的位置");
    }

    /// 用 `with_index` 指定别的初值（`docs/98` §6-B：初值本身未定）。
    #[test]
    fn explicit_initial_cursor_is_honoured() {
        let mut p = PlayerTurns::with_index(vec![10, 20, 30], 2);
        assert_eq!(p.next_shooter_by_state(all_alive), Some(10), "(2+1)%3 = 0");
    }

    // -----------------------------------------------------------------------
    // ② 跳过死人：三局 replay 原样复刻（docs/98 §2）
    // -----------------------------------------------------------------------

    /// `1782763805` 玩家 7443（HAL）：士兵 `[7444..7448]`，实际 `7445 → 7448 → 7444`。
    #[test]
    fn replay_1782763805_player_7443_rotation() {
        let mut p = PlayerTurns::new(vec![7444, 7445, 7446, 7447, 7448]);
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(p.next_shooter_by_state(alive_set(&[7444, 7445, 7448])).unwrap());
        }
        assert_eq!(got, vec![7445, 7448, 7444], "2 与 3 号已死，应被跳过");
    }

    /// `1793884702` 玩家 1053：士兵 `[1083,1054,1055,1084,1085]`，实际 `1054 → 1055 → 1085 → 1083`。
    #[test]
    fn replay_1793884702_player_1053_rotation() {
        let mut p = PlayerTurns::new(vec![1083, 1054, 1055, 1084, 1085]);
        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(p.next_shooter_by_state(alive_set(&[1083, 1054, 1055, 1085])).unwrap());
        }
        assert_eq!(got, vec![1054, 1055, 1085, 1083], "1084 死后才轮到 1085，然后回绕");
    }

    /// `1793874543` 玩家 621（只剩两个活人）：严格交替 7 次。
    ///
    /// 注意这一局的 `alive` 是**会变的**：第 4 发之前 623 也阵亡了
    /// （`docs/98` §2.3 的预测里 1 号位已经是死人），之后只剩 627 / 628。
    #[test]
    fn replay_1793874543_player_621_alternates() {
        let mut p = PlayerTurns::new(vec![622, 623, 627, 629, 628]);
        let mut alive: Vec<i64> = vec![623, 627, 628];
        let expected = [623, 627, 628, 627, 628, 627, 628, 627];
        for (i, expect) in expected.iter().enumerate() {
            if i == 3 {
                alive.retain(|id| *id != 623); // 623 在这一发之前已死
            }
            assert_eq!(p.next_shooter_by_state(alive_set(&alive)), Some(*expect), "第 {i} 发");
        }
    }

    // -----------------------------------------------------------------------
    // ③ 全灭 → Some(0)（与 None 不同）
    // -----------------------------------------------------------------------

    /// 试满 `len` 次仍无活人 ⇒ `Some(0)`；且再次调用还是 `Some(0)`（游标回到原点）。
    #[test]
    fn all_dead_returns_some_zero() {
        let mut p = PlayerTurns::new(vec![7, 8, 9]);
        let dead = alive_set(&[]);
        assert_eq!(p.next_shooter_by_state(&dead), Some(SOLDIER_ID_NONE));
        assert_eq!(p.next_shooter_by_state(&dead), Some(SOLDIER_ID_NONE));
        assert_eq!(p.index(), 0, "试满 len 次后游标回到起点");
    }

    /// `Some(0)` 与 `None` 的**协议后果**不同：前者发 `TurnInfo{0}`，后者不发。
    #[test]
    fn some_zero_broadcasts_but_none_does_not() {
        // 全灭：Some(0) → 仍然广播
        let mut manager = TurnManager::new();
        manager.add_player(621, Team::One, vec![622, 623]);
        let dead = manager.begin_turn(621, alive_set(&[]), false);
        assert_eq!(dead, TurnDecision::Announce { player_id: 621, soldier_id: 0, flip: false });
        assert_eq!(dead.broadcast(), Some((621, 0)), "全灭仍要广播 soldier_id = 0");

        // 空表 + Skip 策略：None → 不广播
        let mut skip = TurnManager::new().with_empty_roster(EmptyRoster::Skip);
        skip.add_player(621, Team::One, vec![]);
        let none = skip.begin_turn(621, alive_set(&[]), false);
        assert_eq!(none, TurnDecision::Skip { player_id: 621, reason: SkipReason::NoSoldiers });
        assert_eq!(none.broadcast(), None, "None 语义：不发 TurnInfo");
    }

    // -----------------------------------------------------------------------
    // ④ 单士兵
    // -----------------------------------------------------------------------

    /// `len == 1`：`(0+1)%1 = 0`，永远是同一个人；死了就是全灭。
    #[test]
    fn single_soldier_is_taken_every_turn() {
        let mut p = PlayerTurns::new(vec![42]);
        assert_eq!(p.next_shooter_by_state(all_alive), Some(42));
        assert_eq!(p.next_shooter_by_state(all_alive), Some(42));
        assert_eq!(p.next_shooter_by_state(alive_set(&[])), Some(SOLDIER_ID_NONE));
        assert_eq!(p.len(), 1);
    }

    // -----------------------------------------------------------------------
    // ⑤ 空队
    // -----------------------------------------------------------------------

    /// 空 `Vec`：原语返回 `None`（getter 没有"当前士兵"），不 panic、不推进。
    #[test]
    fn empty_roster_returns_none() {
        let mut p = PlayerTurns::new(vec![]);
        assert!(p.is_empty());
        assert_eq!(p.current(), None);
        assert_eq!(p.next_shooter_by_state(all_alive), None);
        assert_eq!(p.index(), 0, "空表不该改游标");
    }

    /// 广播层：默认（`0x463fd0` 观测）空表也发 `soldier_id = 0`。
    #[test]
    fn empty_roster_announces_zero_by_default() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::Two, vec![]);
        assert_eq!(manager.empty_roster(), EmptyRoster::AnnounceZero);
        let d = manager.begin_turn(1, alive_set(&[]), false);
        assert_eq!(d, TurnDecision::Announce { player_id: 1, soldier_id: 0, flip: true });
        assert_eq!(d.broadcast(), Some((1, 0)));
    }

    /// 反过来显式选 `Skip` 也能拿到"不发"的语义。
    #[test]
    fn empty_roster_skip_does_not_broadcast() {
        let mut manager = TurnManager::new().with_empty_roster(EmptyRoster::Skip);
        manager.add_player(1, Team::Two, vec![]);
        assert_eq!(manager.begin_turn(1, alive_set(&[]), false).broadcast(), None);
    }

    // -----------------------------------------------------------------------
    // ⑥ `is_on_turn`（任务 46；`0x462b50` = soldiers ∪ players）
    // -----------------------------------------------------------------------

    /// `0x462b50` 的判据是**两个集合的并集**：士兵 id 命中 `get_current_turn_soldiers`，
    /// 玩家 id 命中 `get_current_turn_players`（`0x462bac` 那一段）。
    #[test]
    fn is_on_turn_is_soldiers_union_players() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10, 11]);
        manager.add_player(2, Team::Two, vec![20]);
        assert_eq!(manager.get_current_turn_soldiers(), vec![10, 20], "每玩家取游标处那一个");
        assert_eq!(manager.get_current_turn_players(), vec![1, 2], "同时回合分支：玩家表全部键");
        assert!(manager.is_on_turn(10), "当前士兵 ⇒ 真");
        assert!(manager.is_on_turn(20));
        assert!(manager.is_on_turn(1), "玩家 id 也算（players 那一支）");
        assert!(manager.is_on_turn(2));
        assert!(!manager.is_on_turn(11), "同一玩家的**别的**士兵不在此列");
        assert!(!manager.is_on_turn(99), "两个集合都不中 ⇒ 假（0x462bfe）");
    }

    /// 读的是**当前游标**：游标一动，集合跟着动（不是初始表快照）。
    #[test]
    fn is_on_turn_follows_the_cursor() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10, 11, 12]);
        assert!(manager.is_on_turn(10) && !manager.is_on_turn(11));
        assert_eq!(manager.next_shooter(1, all_alive), Some(11));
        assert!(!manager.is_on_turn(10), "游标走了 ⇒ 上一个不再是当前士兵");
        assert!(manager.is_on_turn(11));
        assert_eq!(manager.get_current_turn_soldiers(), vec![11]);
    }

    /// 空玩家表 ⇒ 两个集合都空；空士兵表的玩家**玩家 id 仍为真**（players 分支取键）。
    #[test]
    fn empty_tables_make_is_on_turn_false_except_player_ids() {
        let empty = TurnManager::new();
        assert!(empty.get_current_turn_players().is_empty());
        assert!(empty.get_current_turn_soldiers().is_empty());
        assert!(!empty.is_on_turn(1));

        let mut manager = TurnManager::new();
        manager.add_player(7, Team::Two, vec![]);
        assert_eq!(manager.get_current_turn_players(), vec![7]);
        assert!(manager.get_current_turn_soldiers().is_empty(), "空 Vec 不 push（0x46299f）");
        assert!(manager.is_on_turn(7), "玩家 id 在 players 里就为真");
        assert!(!manager.is_on_turn(0), "0 是哨兵，从不在表里");
    }

    // -----------------------------------------------------------------------
    // ⑦ 回合模式 + 顺序支（任务 50；`+0x58`、`0x4628d0` 顺序支、`set_turn` 0x4638f0）
    // -----------------------------------------------------------------------

    /// `TurnData[+0x58]` 的初值是 `0` = `SequentialTurns`（`GraphwarTurnData::new`，模块文档 §7.1），
    /// 读它的是 `get_turn_mode`（`0x462c10`）。
    #[test]
    fn turn_mode_defaults_to_sequential_turns() {
        let manager = TurnManager::new();
        assert_eq!(manager.turn_mode(), TurnMode::SequentialTurns);
        assert_eq!(manager.turn_mode().byte(), 0, "写进 +0x58 的那个字节");
        assert_eq!(TurnMode::SimultaneousTurns.byte(), 1);
        assert_eq!(TurnMode::from_byte(0), TurnMode::SequentialTurns);
        assert_eq!(TurnMode::from_byte(1), TurnMode::SimultaneousTurns);
        assert_eq!(TurnMode::from_byte(7), TurnMode::SimultaneousTurns, "二进制只判 == 0");
        assert_eq!(TurnMode::from_known_byte(2), None, "严格版只认 0/1");
    }

    /// `set_turn_mode`（`0x462d20`）：写字节 + 发一条 `TurnModeInfo`（tag 47）。
    #[test]
    fn set_turn_mode_writes_the_byte_and_emits_turn_mode_info() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10]);
        manager.take_outbox();
        manager.set_turn_mode(TurnMode::SimultaneousTurns);
        assert_eq!(manager.turn_mode(), TurnMode::SimultaneousTurns);
        assert_eq!(manager.get_current_turn_players(), vec![1], "同时支：玩家表全部键");
        assert_eq!(manager.get_current_turn_soldiers(), vec![10]);
        assert_eq!(manager.take_outbox(), vec![Event::TurnModeInfo], "0x462dae add_event");
    }

    /// 顺序支：**只有当前玩家 + 它当前那个士兵**（`0x462846` / `0x4629de`）。
    #[test]
    fn sequential_branch_narrows_to_the_current_player() {
        let mut manager = TurnManager::new().with_sequential_fallback(false);
        manager.add_player(1, Team::One, vec![10, 11]);
        manager.add_player(2, Team::Two, vec![20, 21]);
        // 还没人 set_turn ⇒ 出场序表空 ⇒ 原版返回空集（0x46284d je）
        assert!(manager.get_current_turn_players().is_empty(), "序表空 ⇒ 空集（原版语义）");
        assert!(manager.get_current_turn_soldiers().is_empty());
        assert!(!manager.is_on_turn(1) && !manager.is_on_turn(10));

        manager.set_turn(2, 21);
        assert_eq!(manager.get_current_turn_players(), vec![2], "单元素：当前玩家");
        assert_eq!(manager.get_current_turn_soldiers(), vec![21], "单元素：它当前的士兵");
        assert!(manager.is_on_turn(2) && manager.is_on_turn(21));
        assert!(!manager.is_on_turn(1), "别的玩家不在回合内（同人不同支的差别就在这）");
        assert!(!manager.is_on_turn(20), "同一玩家的**别的**士兵也不在");
        assert!(manager.sequential_state_available());
    }

    /// 回落开关（我方）：顺序支缺状态时默认回落到同时支的全集 —— 任务 46 的旧行为。
    #[test]
    fn sequential_falls_back_to_the_union_when_no_turn_was_announced() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10]);
        manager.add_player(2, Team::Two, vec![20]);
        assert!(manager.sequential_fallback(), "默认开");
        assert!(!manager.sequential_state_available(), "序表空 ⇒ 顺序支状态不齐");
        assert_eq!(manager.get_current_turn_players(), vec![1, 2], "回落 = 同时支全集");
        assert_eq!(manager.get_current_turn_soldiers(), vec![10, 20]);
        assert!(manager.is_on_turn(10) && manager.is_on_turn(2));

        assert_eq!(manager.set_sequential_fallback(false), false);
        assert!(manager.get_current_turn_players().is_empty(), "关掉 = 逐字二进制（空集）");
        assert!(manager.get_current_turn_soldiers().is_empty());
        assert!(!manager.is_on_turn(10) && !manager.is_on_turn(1));

        // 一旦有人 set_turn，状态就齐了 ⇒ 开关不再起作用
        assert!(manager.set_sequential_fallback(true));
        manager.set_turn(1, 10);
        assert_eq!(manager.get_current_turn_players(), vec![1]);
        assert_eq!(manager.get_current_turn_soldiers(), vec![10]);
        manager.set_sequential_fallback(false);
        assert_eq!(manager.get_current_turn_players(), vec![1], "状态齐了 ⇒ 关不关都一样");
        assert_eq!(manager.get_current_turn_soldiers(), vec![10]);
    }

    /// `set_turn`（`0x4638f0`）把**当前玩家游标**移到被公告的那个人身上（出场序 = first-seen）。
    #[test]
    fn set_turn_moves_the_current_player_cursor() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10, 11]);
        manager.add_player(2, Team::Two, vec![20, 21]);
        manager.take_outbox();

        manager.set_turn(1, 10);
        assert_eq!(manager.turn_order(), &[1], "首次出现 ⇒ 追加到序表尾（0x463997）");
        assert_eq!(manager.current_player(), Some(1));
        assert_eq!(manager.get_current_turn_players(), vec![1]);

        manager.set_turn(2, 20);
        assert_eq!(manager.turn_order(), &[1, 2]);
        assert_eq!(manager.current_player(), Some(2), "游标跟着换人（0x463a7a）");
        assert_eq!(manager.get_current_turn_players(), vec![2]);
        assert_eq!(manager.get_current_turn_soldiers(), vec![20]);

        manager.set_turn(1, 11);
        assert_eq!(manager.turn_order(), &[1, 2], "序表不重复加（0x463970 找到就走 0x463a77）");
        assert_eq!(manager.current_player(), Some(1), "回指第 0 格");
        assert_eq!(manager.get_current_turn_soldiers(), vec![11], "该玩家自己的游标也动了");
        assert_eq!(manager.take_outbox(), vec![Event::TurnInfo; 3], "每条 set_turn 发 TurnInfo");
    }

    /// 士兵公告**去重**并把游标指到那个位置（`0x463ba0` / `0x463c5d` vs `0x463bc6` / `0x463bd2`）。
    #[test]
    fn announce_dedups_soldiers_and_sets_the_cursor() {
        let mut turns = PlayerTurns::new(vec![10, 11, 12]);
        assert_eq!(turns.announce(12), 2, "已在表里 ⇒ 游标移到它的位置");
        assert_eq!(turns.soldiers(), &[10, 11, 12], "不追加重复项");
        assert_eq!(turns.announce(11), 1);
        assert_eq!(turns.announce(99), 3, "新 id ⇒ 追加到表尾，游标 = 新位置");
        assert_eq!(turns.soldiers(), &[10, 11, 12, 99]);
        assert_eq!(turns.current(), Some(99));
    }

    /// `soldier_id == 0` ⇒ 删掉该玩家的**整条记录**（`0x463a8b` → `HashMap::remove`），
    /// 但出场序表里那一格**不删** ⇒ 两个 getter 在这一点上不对称（`观测`）。
    #[test]
    fn set_turn_with_zero_removes_the_record_but_keeps_the_order_slot() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10]);
        manager.add_player(2, Team::Two, vec![20]);
        manager.set_turn(1, 10);
        manager.set_turn(2, 20);
        manager.set_turn(1, SOLDIER_ID_NONE);
        assert_eq!(manager.turn_order(), &[1, 2], "序表不删");
        assert_eq!(manager.current_player(), Some(1), "游标仍指向它");
        assert_eq!(manager.get_current_turn_players(), vec![1], "players 仍含已删玩家");
        assert!(manager.get_current_turn_soldiers().is_empty(), "soldiers 查不到 ⇒ 空（0x462a81）");
        assert!(manager.player(1).is_none(), "玩家表里整条没了");
        assert!(!manager.is_on_turn(10));
        assert!(manager.is_on_turn(1));
    }

    /// 同时回合那一支不受顺序游标影响：全员 + 每人当前士兵（`0x46290b`–`0x4629d3`）。
    #[test]
    fn simultaneous_branch_ignores_the_sequential_cursor() {
        let mut manager = TurnManager::new().with_sequential_fallback(false);
        manager.add_player(1, Team::One, vec![10, 11]);
        manager.add_player(2, Team::Two, vec![20]);
        manager.set_turn(2, 20);
        assert_eq!(manager.get_current_turn_players(), vec![2], "顺序支：只有 2");
        manager.set_turn_mode(TurnMode::SimultaneousTurns);
        assert_eq!(manager.get_current_turn_players(), vec![1, 2], "同时支：全部键");
        assert_eq!(manager.get_current_turn_soldiers(), vec![10, 20], "每人取自己的游标");
        assert!(manager.is_on_turn(10) && manager.is_on_turn(2));
        assert!(!manager.is_on_turn(11));
    }

    // -----------------------------------------------------------------------
    // 队伍 → flip（docs/105）
    // -----------------------------------------------------------------------

    /// `flip` 只由队伍决定：Team1 → 向右（`false`），Team2 → 向左（`true`）。
    #[test]
    fn flip_comes_from_the_team() {
        assert!(!Team::One.flip(), "Team1 在 x<0 侧，朝 +x");
        assert!(Team::Two.flip(), "Team2 在 x>0 侧，朝 -x");
        assert_eq!(Team::from_id(1), Some(Team::One));
        assert_eq!(Team::from_id(2), Some(Team::Two));
        assert_eq!(Team::from_id(3), None, "3 队及以上未观测，不猜方向");
        assert_eq!(Team::One.id(), 1);
        assert_eq!(Team::Two.id(), 2);
    }

    /// 判决里带的 `flip` 与队伍一致——注意接口里**根本没有**坐标参数，
    /// 所以"用起点/终点 x 比大小去猜方向"在结构上就不可能（`docs/105` §3）。
    #[test]
    fn begin_turn_carries_team_flip() {
        let mut manager = TurnManager::new();
        manager.add_player(229, Team::One, vec![230, 231]);
        manager.add_player(232, Team::Two, vec![233, 234]);
        assert_eq!(manager.flip_for(229), Some(false));
        assert_eq!(manager.flip_for(232), Some(true));
        let a = manager.begin_turn(229, all_alive, false);
        let b = manager.begin_turn(232, all_alive, false);
        assert_eq!(a.flip(), Some(false));
        assert_eq!(b.flip(), Some(true));
        assert_eq!(a.soldier_id(), Some(231), "初值 0 ⇒ 首取 soldiers[1]");
        assert_eq!(b.soldier_id(), Some(234));
    }

    /// 抓包里十局的"相邻两次出手来自不同队伍"——那是**玩家顺序**的性质（`docs/47`），
    /// 本模块不管顺序，但轮转模型必须能复现它（给定每个玩家的初始游标）。
    ///
    /// ⚠️ 初始游标是两个自由参数（`docs/98` §6-B 未定），这里显式给出**只为复现抓包**，
    /// 不当结论。
    #[test]
    fn captured_sequence_is_reachable_with_per_player_cursors() {
        let captured = [230, 234, 231, 233, 230, 234, 231, 233, 230];
        let player_of = |s: i64| if s == 230 || s == 231 { 229 } else { 232 };
        let mut manager = TurnManager::new();
        manager.add_player(229, Team::One, vec![230, 231]);
        manager.add_player(232, Team::Two, vec![233, 234]);
        // 抓包里 229 的首发是 230 ⇒ 该玩家初值 1；232 的首发是 234 ⇒ 初值 0。
        manager.players.get_mut(&229).unwrap().turns = PlayerTurns::with_index(vec![230, 231], 1);

        let mut got = Vec::new();
        for &expect in captured.iter() {
            // 玩家顺序沿用抓包（上一发属于谁，下一发就换另一个玩家）
            let player_id = player_of(expect);
            got.push(manager.begin_turn(player_id, all_alive, false).soldier_id().unwrap());
        }
        assert_eq!(got, captured);
    }

    // -----------------------------------------------------------------------
    // 回合超时 = 跳过该玩家（用户口述，未独立验证）
    // -----------------------------------------------------------------------

    /// 超时 ⇒ `Skip(TurnTimeout)`，且**不推进游标**（这一条是实现选择，游戏里未知）。
    #[test]
    fn timeout_skips_the_player_without_advancing_the_cursor() {
        let mut manager = TurnManager::new();
        manager.add_player(1, Team::One, vec![10, 20, 30]);

        let timed_out = manager.begin_turn(1, all_alive, true);
        assert_eq!(timed_out, TurnDecision::Skip { player_id: 1, reason: SkipReason::TurnTimeout });
        assert_eq!(timed_out.broadcast(), None, "超时不发 TurnInfo");
        assert_eq!(timed_out.flip(), None);
        assert_eq!(manager.player(1).unwrap().turns.index(), 0, "超时不动游标");

        // 下一回合正常轮到该玩家时，仍按原游标取 —— 等于"这回合没轮到过他"。
        let normal = manager.begin_turn(1, all_alive, false);
        assert_eq!(normal.soldier_id(), Some(20));
        assert_eq!(manager.player(1).unwrap().turns.index(), 1);
    }

    #[test]
    fn unknown_player_is_reported() {
        let mut manager = TurnManager::new();
        let d = manager.begin_turn(999, all_alive, false);
        assert_eq!(d, TurnDecision::Skip { player_id: 999, reason: SkipReason::UnknownPlayer });
        assert_eq!(manager.flip_for(999), None);
        assert_eq!(manager.next_shooter(999, all_alive), None);
    }

    // -----------------------------------------------------------------------
    // 存活判定（life_manager 状态字节）
    // -----------------------------------------------------------------------

    /// 只有 `& 1` 为真才算活：0（死）与 2（查不到）都不可开火。
    #[test]
    fn state_byte_low_bit_decides_life() {
        assert!(!is_alive_byte(0));
        assert!(is_alive_byte(1));
        assert!(!is_alive_byte(MISSING_STATE), "查不到 = 2 ⇒ 不可开火");
        assert!(is_alive_byte(3), "任何奇数都算活（只看最低位）");
        assert!(is_alive_byte(0xFF));

        let mut p = PlayerTurns::new(vec![1, 2, 3]);
        // 1 号是 3（奇=活）⇒ 直接选中；2 号是 2（查不到）、3 号是 0（死）都不选。
        let states = |id: i64| match id {
            1 => 3,
            2 => MISSING_STATE,
            _ => 0,
        };
        assert_eq!(p.next_shooter_by_state(states), Some(1));
    }

    /// `LifeState` 适配器：Alive → 1、Dead → 0、没记录 → 2。
    #[test]
    fn life_state_adapter_maps_missing_to_two() {
        let mut life = LifeState::default();
        life.set(5, Status::Alive, None);
        life.set(6, Status::Dead, None);
        let f = life_state_of(&life);
        assert_eq!(f(5), 1);
        assert_eq!(f(6), 0);
        assert_eq!(f(7), MISSING_STATE);

        let mut p = PlayerTurns::new(vec![6, 7, 5]);
        assert_eq!(p.next_shooter_by_state(f), Some(5), "死人与查不到的都要跳过");
    }

    /// `soldier_id == 0` 不能当真实士兵（会与"全灭"混淆）。
    #[test]
    #[should_panic(expected = "全灭")]
    fn zero_is_rejected_as_a_soldier_id() {
        let _ = PlayerTurns::new(vec![1, 0]);
    }
}

#[cfg(test)]
mod event_flow_tests {
    use super::*;

    /// 订阅方向：`new`（`0x463790`）订阅的就是那两条。
    #[test]
    fn new_registers_the_two_observed_subscriptions() {
        let tm = TurnManager::new();
        assert_eq!(tm.subscribed, vec!["new_connection_event", "entity_removed_event"]);
        assert_eq!(SUBSCRIBED_EVENTS.len(), 2);
        assert_eq!(TurnManager::default().subscribed, tm.subscribed);
        assert_eq!(tm.outbox_len(), 0);
    }

    /// 发方向：`begin_turn` 落到 `Announce` 就发一条 `TurnInfo`(43)；`Skip` 一条都不发。
    #[test]
    fn begin_turn_emits_turn_info_only_when_announcing() {
        let mut tm = TurnManager::new();
        tm.add_player(1, Team::One, vec![1, 2]);
        let alive = |_id: i64| 1u8;
        // 观测（0x463f40–0x463f58）：先自增再取 ⇒ 第一次拿到 soldiers[1] = 2
        let decided = tm.begin_turn(1, alive, false);
        assert_eq!(decided, TurnDecision::Announce { player_id: 1, soldier_id: 2, flip: false });
        assert_eq!(tm.outbox, vec![Event::TurnInfo]);
        assert_eq!(Event::TurnInfo.wire_tag(), 43);
        // 超时 ⇒ Skip，不发
        let _ = tm.take_outbox();
        let skipped = tm.begin_turn(1, alive, true);
        assert_eq!(skipped.skip_reason(), Some(SkipReason::TurnTimeout));
        assert_eq!(tm.outbox_len(), 0, "超时跳过不发 TurnInfo");
        // 未知玩家 ⇒ Skip，不发
        let _ = tm.begin_turn(99, alive, false);
        assert_eq!(tm.outbox_len(), 0);
    }

    /// 全灭：`AnnounceZero` 策略下发的是 `{player, 0}`（观测 0x464016–0x464021）。
    #[test]
    fn empty_roster_still_announces_zero() {
        let mut tm = TurnManager::new();
        tm.add_player(1, Team::Two, vec![7]);
        let dead = |_id: i64| 0u8;
        let decided = tm.begin_turn(1, dead, false);
        assert_eq!(decided, TurnDecision::Announce { player_id: 1, soldier_id: 0, flip: true });
        assert_eq!(tm.outbox, vec![Event::TurnInfo]);
    }

    /// `entity_removed_event`（`0x463580`）：整条玩家记录被删，**不发**事件。
    #[test]
    fn entity_removed_drops_the_player_entry_without_emitting() {
        let mut tm = TurnManager::new();
        tm.add_player(1, Team::One, vec![1, 2]);
        tm.add_player(2, Team::Two, vec![3]);
        assert!(tm.handle_entity_removed_event(1));
        assert!(tm.player(1).is_none());
        assert!(tm.player(2).is_some(), "只删命中的那一条");
        assert!(!tm.handle_entity_removed_event(1), "再删一次返回 false（Option 的 None 支）");
        assert!(!tm.handle_entity_removed_event(99));
        assert_eq!(tm.outbox_len(), 0, "0x463580 体内没有 add_event");
    }

    /// `new_connection_event`（跳板 `0x463750` → `0x462fe0`）：每人一条重播。
    ///
    /// ⚠️ 载荷是**我方形状**（`发送` 的载荷未解），所以这条只钉"每人一条 + 进发件箱"。
    #[test]
    fn new_connection_replays_one_turn_info_per_player() {
        let mut tm = TurnManager::new();
        tm.add_player(1, Team::One, vec![1, 2]);
        tm.add_player(2, Team::Two, vec![3]);
        let replayed = tm.handle_new_connection_event();
        assert_eq!(replayed.len(), 2);
        assert_eq!(tm.outbox_len(), 2);
        assert!(tm.outbox.iter().all(|e| *e == Event::TurnInfo));
        assert_eq!(replayed[0].soldier_id(), Some(1), "还没转过 ⇒ 游标在 0，current() 是 soldiers[0]");
    }
}
