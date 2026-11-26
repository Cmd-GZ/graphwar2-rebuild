//! `graphwar_remote_event_handler.rs`：客户端侧 / 服务端侧的**远端事件处理器**
//! （原版 `GraphwarRemoteEventHandler`）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **ELF vaddr**（= 模块内偏移 = RVA；Ghidra/REA 的反编译地址 = 本地址 + `0x100000`）。
//!
//! # 1. 为什么这个文件存在（边界证据）
//!
//! 本模块此前**没有同名文件**，内容混在 `src/events.rs`。T24 的逐函数边界审计
//! （[`docs/06`](../../docs/06-verification.md) §74、`fixtures/boundary_audit.tsv`，
//! 判据 = `panic::Location` 硬归属）判出两条 `mismatch`：
//!
//! | 地址 | 符号名（三向真名，`fixtures/name_to_addr_v2.tsv`） | `panic::Location` 主文件 | 首锚 `(line:col)` | 锚点行数 | 判定 |
//! |---|---|---|---|---|---|
//! | `0x3dee00` | `GraphwarRemoteEventHandler::handle_client_remote_event` | `src/graphwar_remote_event_handler.rs` | `143:99` | **80** | `mismatch`（原 `ours=src/events.rs`） |
//! | `0x3e5910` | `GraphwarRemoteEventHandler::is_leader` | 同上 | `135:103` | 2（`:135` `:136`） | 同上 |
//!
//! 两条都单模块、一边倒 ⇒ `AGENTS.md` §4「**文件名就是证据**」：原版有这个模块，
//! 我们就要有这个同名 `.rs`。本文件即按该证据建立（T30 / `task-27`，2026-11-26）。
//!
//! ⚠️ **本文件的成立只说明"布局/文件名对上"，不代表这两个函数的行为复原了。**
//! 这与 `docs/06` §74.3 对台账 `match` 的口径一致（`match` 数的是"能被 Location 钉住的
//! 函数里原版文件与我方布局同名"，不是"复原了多少行为"）。
//!
//! # 2. 我方现状（诚实清单）
//!
//! | 原版函数 | 我方状态 | 说明 |
//! |---|---|---|
//! | `0x3dee00` `handle_client_remote_event` | **未落码（0 个函数体）** | 原版这一条就有 80 个 Location 锚、反编译约 2449 行：它是"客户端/服务端收到一条事件 → 落到对应 manager"的大分派体，体内还内联了大批别的模块的 setter |
//! | `0x3e5910` `is_leader` | **未落码（0 个函数体）** | 见 §3 的指令级观测；语义仍缺 arm64 对照与调用点分析 |
//! | `0x3e2830` `handle_server_remote_event`（第三个别名函数） | 不搬 | 台账判 **`multi`**（主文件 88 锚 + 另 8 个模块各 1–5 锚，`docs/06` §74.5）⇒ `AGENTS.md` §4「边界无证据不猜不搬」，**只登记、不碰** |
//!
//! 我方现在能指到"远端事件"这条链路的只有**自造桥接**，按 §4 不进复原层：
//! `src/_scaffold/net_events_class.rs`（`GwNetEvents`：线上事件 → 复原 manager 的派发桥，
//! 文件头自述"**不是复原物**"，因为本模块没复原所以客户端这条链路是断的）。
//! 枚举/判别值侧的证据在 `src/events.rs`（`enum Event`，74 变体）与
//! `src/graphwar_event_handler.rs`（`DISPATCH_NAME_TAGS`，72 条名字→判别值）。
//!
//! # 3. `is_leader` 的指令级观测（`tools/addr_probe.py dis 0x3e5910`，**观测**）
//!
//! ```text
//! 0x3e5910  push rbp / r14 / rbx ; sub rsp,0x10
//! 0x3e5918  mov  ebx, esi                     ; 第二参数（u32）留着后面用
//! 0x3e591a  call 0x46d8c0                     ; module_container::get_module_wk 的单态化族
//! 0x3e591f  test rax,rax ; je -> panic        ; Rc 为空 ⇒ unwrap 失败路径
//! 0x3e592d  mov  rcx,[rax+0x10]               ; Rc 强计数
//! 0x3e5931  movabs rdx,0x7fffffffffffffff ; cmp rcx,rdx ; jae -> panic   ; Rc 溢出检查
//! 0x3e5944  lea  rdx,[rcx+1] ; mov [rax+0x10],rdx                          ; 计数 +1
//! 0x3e594c  mov  r14,[rax+0x48] ; test r14,r14 ; je -> 尾部                ; 表长度/桶数
//! 0x3e5959  mov  rdx,[rax+0x30]                                            ; hashbrown 控制字节块
//! 0x3e595d  movdqa xmm0,[rdx] ; pmovmskb r9d,xmm0 ; xor r9d,0xffff
//! 0x3e5980  ... add rdx,-0x80 / add rsi,0x10 / pmovmskb ...（0x80 字节一组扫）
//! 0x3e599a  tzcnt edi,r9d ; shl edi,3 ; sub r8,rdi                          ; 命中槽地址
//! 0x3e59a8  mov  ebp,[r8-4]                                                ; 槽里的键（u32）
//! ```
//!
//! **观测**：形状是 `is_leader(u32) -> bool`（取第二个参数）；先 `get_module_wk` 拿一个
//! `Rc` 对象（强计数 +0x10，`Rc` 溢出检查在 `0x7fff…`），再对 `+0x30`/`+0x48` 那对字段做
//! **hashbrown** 查表（`pmovmskb`/`tzcnt` 是标准库 `HashMap` 的控制字节扫描），键是 4 字节。
//! **推断**：传入的 u32 是 connection id / player id，答案是"这个连接是不是房主"。
//! **缺的证据**：arm64 2.0.60 的同函数对照、`0x46d8c0` 到底是哪个单态化实例、调用点分析
//! （谁判 leader）——这三样都没有 ⇒ **只记观测，不落码**。
//!
//! # 4. 公开路径
//!
//! 本文件**没有 `fn`**，因此没有需要 `pub use` 再导出的老路径；原 `src/events.rs` 里
//! 属于本模块的只是"枚举 + 名字表"，那两样按 `panic::Location` 分别归
//! `graphwar_event.rs`（留在 `events.rs`）与 `graphwar_event_handler.rs`（T30 同日切出）。

/// 原版模块名（arm64 `.symtab` 里的 `graphwarrust::graphwar_remote_event_handler`）。
pub const MODULE: &str = "graphwar_remote_event_handler";

/// `panic::Location` 里逐字节出现的源文件名 —— 与上面 `MODULE` 同名，这就是
/// `AGENTS.md` §4 说的"文件名就是证据"的物证侧（台账判据直接读这个字面量）。
pub const SOURCE_FILE: &str = "src/graphwar_remote_event_handler.rs";

/// 与真二进制对照的锚定强度。分级沿用 `fixtures/name_to_addr.tsv` 的判据：
/// 同一个 `(file, line, col)` 在两条架构上都**只被这一个函数引用**才叫 `Unique`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 双向唯一 `(file,line,col)` 锚。
    Unique,
    /// 无直接一对一锚，但按「一个名字只对应一个地址」消去后只剩一个候选。
    DeducedByElimination,
    /// 该函数不引用任何 `Location`，名字无法落地（纯胶水/纯内联）。
    NoAnchor,
}

/// 一个已归属的函数（证据行，不是行为实现）。
///
/// ⚠️ 为什么没有 arm64 列：本机 checkout 的 `.reference/upstream/` 只有 x86 的
/// `libgraphwarrust.so` 与 `analysis/funcstarts_ghidra_va.pkl`，**没有** arm64 `.so`；
/// 别的文档里引用的 arm64 地址在本 checkout 无法复算 ⇒ 宁可不给，也不抄二手数字
/// （`AGENTS.md` §0.3「每条结论必须标证据等级」）。
#[derive(Debug, Clone, Copy)]
pub struct Func {
    pub name: &'static str,
    /// x86 steam 2026-08-28 构建的 ELF vaddr（RVA）。
    pub x86: u64,
    /// `fixtures/name_to_addr_v2.tsv` 记的第一个 `(line, col)` 锚（真名的落地键）。
    pub loc_first: (u32, u32),
    /// `fixtures/boundary_audit.tsv` 里该函数落在本文件的**锚点行数**（不是"行号个数"以外的口径）。
    pub loc_lines: u32,
    pub link: Link,
}

/// 本模块**这两条**（= 台账里判 `mismatch` 的那两条）函数的证据行。
///
/// 第三个别名函数 `handle_server_remote_event`（`0x3e2830`，主文件 88 锚 + 另 8 个模块各 1–5 锚）
/// 台账判 `multi` ⇒ 按 `AGENTS.md` §4 **只登记、不搬、不列进本表**（`docs/06` §74.5）。
pub const FUNCTIONS: &[Func] = &[
    Func {
        name: "GraphwarRemoteEventHandler::handle_client_remote_event",
        x86: 0x3d_ee00,
        loc_first: (143, 99),
        loc_lines: 80,
        link: Link::Unique,
    },
    Func {
        name: "GraphwarRemoteEventHandler::is_leader",
        x86: 0x3e_5910,
        loc_first: (135, 103),
        loc_lines: 2,
        link: Link::Unique,
    },
];
