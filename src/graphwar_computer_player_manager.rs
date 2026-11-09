//! 电脑玩家（Bot）：**谁会成为 Bot**、Bot 的**身份**（名字/皮肤/脸/帽）、
//! **难度档**（`level` → 六档）、以及"轮到 Bot 时谁去算函数"的**门**。
//!
//! 本模块回答攻略侧最要紧的一句：**Bot 不是"掉线托管"，是房间里显式加进来的一个
//! 连接**；难度不是一个连续参数，而是 `level` 落进六个区间之一。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律写 **vaddr**（Ghidra = vaddr + `0x100000`）。
//!
//! ## 1. 函数清单（全部已定位）
//!
//! | 函数 | x86 vaddr | arm64 vaddr / 长度 | 职责 | 等级 |
//! |---|---|---|---|---|
//! | `handle_tick` | `0x4785f0` | `0x4ae624` / 1940 B | 每 tick 扫一遍 Bot 表 | 部分解（§6-A） |
//! | `add_computer_player` | `0x478fc0` | `0x4aedb8` / 1184 B | 建档 + 随机名字 + 皮肤/脸/帽 + 4 条身份事件 | 观测 |
//! | `handle_turn_info_event` | `0x479520` | `0x4af258` / 3224 B | 断言 tag=43；`state == 3` 时**起线程**算函数 | 观测（门）/ 推断（体内） |
//! | `computer_function_thread` | `0x47a300`（**推断**） | `0x4afef0` / 424 B | 线程体：拼请求 → 调进化器 → 写回 | 推断 |
//! | `handle_entity_removed_event` | `0x47a520` | `0x4b0098` / 380 B | 断言 tag=14；把该连接从 Bot 表里 erase | 观测 |
//! | `new` | `0x47a720` | `0x4b02a4` / 500 B | 订阅 `tick_event` / `turn_info_event` / `entity_removed_event` | 观测 |
//! | 三个 `{{closure}}` | `0x47a6c0` / `0x47a6e0` / `0x47a700` | `0x4b0214` / `0x4b0244` / `0x4b0274`（各 48 B） | 分别转发到上面三个方法 | 观测 |
//!
//! 地址来源：`add_computer_player`（`:195:26`）、`handle_tick`（`:221:72`）、
//! `handle_turn_info_event`（`:276:126`，**16 个**双向唯一锚）、
//! `handle_entity_removed_event`（`:331:18`）、`new`（`:84:33`）都在
//! `fixtures/name_to_addr.tsv` 里，全部是唯一锚。`computer_function_thread` 无锚
//! （不引用任何 `panic::Location`），它的地址是**按 arm64 符号顺序 + 区间夹逼**推的：
//! arm64 里它夹在 `handle_turn_info_event`(`0x4af258`) 与
//! `handle_entity_removed_event`(`0x4b0098`) 之间，x86 侧对应区间里只有一个
//! 未命名的 424 B 级函数 `0x47a300`（`0x579520..0x57a300..0x57a520`）。
//!
//! ## 2. 谁会成为 Bot（本篇核心，观测）
//!
//! **本模块的三个订阅里没有任何"接管"逻辑**——它只订阅
//! `tick_event` / `turn_info_event` / `entity_removed_event`，**没有**订阅
//! `connection_removed_event`、没有 `game_state_info_event`、也没有任何"某人超时/
//! 掉线 ⇒ 我来算"的分支。
//!
//! Bot 的建立只有一条入口（观测）：**收到 `bot_added_event`（判别值 10）**
//! 时，`graphwar_remote_event_handler`（`0x3e2830`，`switch` 在 `tag - 2` 上，
//! 所以 `case 0x8` = 判别值 10）调用本模块的 `add_computer_player`：
//!
//! ```text
//! case 10 (bot_added_event):
//!     connection_id = event[0xc]                       ; u32
//!     level         = event[0x10]                      ; u32
//!     elite         = level < 4 ? 10 : level < 11 ? 20
//!                   : level < 26 ? 30 : level < 101 ? 40 : 50
//!     add_computer_player(conn, level, elite, 5)
//! ```
//!
//! ⇒ **Bot 是"房间里多出来的一个连接"**，由房主在准备界面里加（客户端发
//! `new_bot_request_event`，服务端把它落成 `bot_added_event` 广播出来）。
//! 房间设置、掉线、难度滑块**都不是**触发条件；`level` 只决定**它有多强**，
//! 不决定**它是不是 Bot**。
//!
//! ## 3. 难度：`level` → 六档（观测，且与 `docs/13-ai.md` §1 的"代数"列互证）
//!
//! `add_computer_player` 里对 `level` 是一条链式比较，产出**皮肤序号**：
//!
//! | `level` 区间 | 档 | 皮肤 | 精英数 `s`（`= 种群/5`） | `limit` |
//! |---|---|---|---|---|
//! | `0..=3` | 0 | `bot_0` | 10 | 5 |
//! | `4..=10` | 1 | `bot_1` | 20 | 5 |
//! | `11..=25` | 2 | `bot_2` | 30 | 10 |
//! | `26..=100` | 3 | `bot_3` | 40 | 15 |
//! | `101..=250` | 4 | `bot_4` | 50 | 25 |
//! | `>= 251` | 5 | `bot_5` | 50 | 200 |
//!
//! 阈值就是 `4 / 11 / 26 / 101 / 251`，**等于 `docs/13-ai.md` §1 那张表的
//! "代数"列 + 1**（3→4、10→11、25→26、100→101、250→251）。也就是说
//! **`level` 的语义是"最多跑几代"**：`level <= 代数上限` 就落进那一档。
//! 这不是巧合能凑出来的五对数字，两条独立证据（本文的 `0x478fc0` 比较链、
//! `docs/13-ai.md` 的进化器参数表）在这里对上了。
//!
//! 第三列 `limit` 是**调用点直接读到的立即数**（`0x3e4573` `mov $0x28,%ecx` 的那
//! 条链旁边的 `mov $0xf,%r9d` / `cmovb $0x19,$0xc8`），与 `docs/13-ai.md` §1 的
//! `limit` 列 `5 / 5 / 10 / 15 / 25 / 200` **逐值一致**。
//!
//! 同一张链在**调用点**（`0x3e28xx` 的 `case 10`）还算了一次**精英数**：
//! `10 / 20 / 30 / 40 / 50 / 50`，恰好是 `docs/13-ai.md` §1"种群"列
//! （`50 / 100 / 150 / 200 / 250 / 250`）**除以 5** —— 与 `docs/13-ai.md` §18
//! "精英 = 每代前 20%（`s = 种群/5`）"逐值吻合。第四个参数是**常数 5**，
//! 与同文档"字面量 5 是每槽个体数（1 克隆 + 4 工厂子代）"吻合。
//!
//! ⚠️ **对 `docs/13-ai.md` §0.1 / §1 的一处更正**：那张表把"机器人"列写成
//! 等级 → 名字（1=ChatGPT、4=Multivac、6=Deep Thought …）。**名字与难度无关**：
//! `0x478fc0` 里名字来自一张 **6 项表 + 随机下标**，与 `level` 完全没有数据流。
//! 那张表的"名字"列应当只当作"在 replay 里**碰巧**见过的名字"，不能反推难度；
//! `docs/13-ai.md` 自己也已经记过"名字池不止 6 个，名字不能当难度用"，
//! 本模块把这条升级为**机制级**结论。
//!
//! ## 4. 名字池：**10 项**，随机取（观测）
//!
//! `0x478fc0` 里 `0x47910b..0x479129` 是"除以 10 取余、再乘 16 去索引"：
//!
//! ```text
//! 4790fe: movabs $0xcccccccccccccccd,%rcx
//! 47910b: mul %rcx ; shr $2,%edx ; and $0xfffffffe,%edx   ; edx = 2*floor(x/10)
//! 479114: lea (%rdx,%rdx,4),%eax                          ; eax = 5*edx = 10*floor(x/10)
//! 479117: sub %eax,%r15d ; shl $4,%r15d                   ; r15 = (x mod 10) * 16
//! 479125: mov (%r15,%rax,1),%rbx / 0x8(...),%r15          ; (ptr, len)
//! ```
//!
//! ⇒ **索引 = `rand % 10`**，表在 `.data.rel.ro 0x959cd8`，**10 项 × 16 字节**。
//! 逐项读出来的（顺序即池序）：
//!
//! ```text
//! 0 Deep Thought   1 HAL        2 Skynet     3 Agent Smith  4 Multivac
//! 5 Deep Blue      6 Cleverbot  7 Alpha Zordon  8 Wolfram   9 ChatGPT
//! ```
//!
//! 这些字符串**紧邻**在 `.rodata 0x1c19dd..0x1c1a29`（`Multivac` 在 `0x19f198`，
//! 是池里唯一被共享的字符串）。
//!
//! ## 5. 建档时广播的 4 条身份事件（观测）
//!
//! `add_computer_player` 在插表之后按顺序发（tag 都是 `0x8000…` + 判别值）：
//!
//! | 顺序 | 判别值 | 事件 | 载荷 |
//! |---|---|---|---|
//! | 1 | 59 | `name_request_event` | `{ name: String, player_id: u32 }`（随机名） |
//! | 2 | 50 | `skin_graphics_request_event` | `{ "bot_N", player_id }` |
//! | 3 | 52 | `face_graphics_request_event` | `{ "no_face", player_id }` |
//! | 4 | 54 | `hat_graphics_request_event` | `{ "no_hat", player_id }` |
//! | 5 | 66 | `rank_request_event` | `{ …, player_id }` |
//!
//! 皮肤名 `bot_0..bot_5` 与 `res://skins/bot_0.svg..bot_5.svg` 对得上（资源清单里有
//! 这 6 个文件）。脸/帽是字面量 `no_face` / `no_hat`（反编译里是两条
//! `mov` 拼出来的 7 / 6 字节）。
//!
//! 注意发的是 **request**（客户端→服务端的"我要改成这个"），不是 info——所以
//! **Bot 的身份是它自己"申报"的**，服务端再转成 info 广播。
//!
//! ## 6. `BotData` 与"轮到它了"的门（观测）
//!
//! Bot 表是 `HashMap<u32, BotData>`，**桶步长 `0x78` = 120 字节**
//! （`handle_entity_removed_event` 里 `iVar10 * -0x78`、`handle_tick` 同），
//! **key = 该 Bot 的 `player_id`**（`bot_added_event` 载荷 `+0x0c`），`BotData` 从 `entry+0x08` 起
//! 共 112 字节。任务 79 的运行时样本（两局）把这一段钉到了字节：
//!
//! ```text
//! 样本（AI-vs-AI，bot 232，level = 1000）：120 字节桶的原文
//!   00: e8 00 00 00            ; +0x00 key = 232（u32），+0x04 是桶内填充
//!   08: 03 00 00 00            ; +0x08 **state = 3**（= BotData+0x00）
//!   18: 00 00 80 3f 00 00 80 3f; BotData+0x10/+0x14 = 两个 f32 1.0
//!   68: e8 03 00 00 32 00 00 00 05 00 00 00 c8 00 00 00
//!       ; BotData+0x60 level=1000 · +0x64 精英数=50 · +0x68 每槽个体=5 · +0x6c limit=200
//! ```
//!
//! 那四个数**与静态两条链一致**：`0x479017`–`0x47902e` 把 `edx/ecx/r8d/r9d` 写进
//! `[rsp+0xe0..0xf0]`（= 值的 `+0x60..+0x70`），而运行时读回来的原文正好在 `entry+0x68`。
//!
//! | 偏移（相对 `BotData`，基 = `entry+0x08`） | 内容 | 等级 |
//! |---|---|---|
//! | `+0x00` | **状态** `i32`；`== 3` 才放行算函数 | 观测（静态写入点 + 运行时直读） |
//! | `+0x10 / +0x14` | 两个 `f32` = `1.0`（样本里两次都是 `0x3f800000`） | 观测（一局样本） |
//! | `+0x60` | `level`（原始难度参数） | 观测（静态 + 运行时） |
//! | `+0x64` | 精英数（`= 种群/5`） | 观测（同上） |
//! | `+0x68` | 每槽个体数（恒 5） | 观测（同上） |
//! | `+0x6c` | `limit`（复杂度上限） | 观测（同上） |
//! | `+0x3c` 一带 | 第二层 `HashMap<u32 /*soldier_id*/, SoldierBotData>`（桶步长 **`0x58` = 88**） | 观测（起点未逐位钉死） |
//! | `SoldierBotData + 0x50` | 一个 `u8`（`handle_turn_info_event` 读出来当参数） | 观测 |
//!
//! ⚠️ **更正（任务 79）**：旧版本把 key 写成 `connection_id`、把状态写成 `BotData+0x08`。
//! 运行时两局都是：bot_added 的 key = 232，而该 Bot 的 `player_info_event` 是
//! `(player_id=232, connection_id=228)` —— 228 是**我（房主）的连接**。⇒ key 是 **player_id**，
//! 不是 connection id；状态在桶内 `+0x08`（= `BotData+0x00`），`0x479610 cmp dword [r14-0x70],3`
//! 与 `0x4795d2 cmp r15d,[r14+rdi-0x78]` 两条指令就是这个口径。
//!
//! `state` 的完整轨迹（**观测**，两局 × 两个 Bot，100 ms 直读 + 三条写入点）：
//!
//! ```text
//! 建档        0x478fe3/0x479036 → BotData+0x00 = 3       （空闲，可算）
//! 过 turn_info 门 0x479610 读到 3 ⇒ 放行；thread::spawn 0x479da4
//!           0x479dfa mov [r14-0x70],rbx（rbx = [rsp+0xa8] = 1）→ state = 1   （线程在跑）
//! handle_tick 0x47864f mov qword [r15],3                → state = 3        （收尾，样本里 25 s 后）
//! ```
//!
//! `0` 与 `2`：**在 `.text` 里找不到写 `BotData+0x00` 的指令**（有界否定，正对照见 §7-B）。
//!
//! `handle_turn_info_event` 的门是**三道**（全部观测）：
//!
//! ```text
//! 1) tag 必须恰为 43（turn_info_event），否则 explicit panic
//! 2) self.bots 非空
//! 3) 事件里的 player_id 在表里，且 BotData.state == 3        ← 否则直接 return
//! ```
//!
//! 过了门才干活：把**七个 manager** 的快照拼成一份请求（`0x200` 字节的
//! `0x56ebe0 / 0x56c9e0 / 0x56d6a0 / 0x56ce20 / 0x56e9c0 / 0x56e7a0
//! （= `team_manager`）/ 0x56e140` 各取一次，每个都上锁、拷出、解锁），再用
//! 该士兵在 `SoldierBotData` 里的那个 `u8` 作参数，最后
//! `std::thread::spawn`（失败路径的字面量是 `"failed to spawn thread"`）。
//! ⇒ **Bot 的思考在独立线程里跑**，主线程不等它。
//!
//! ## 7. 仍未确定
//!
//! | # | 未知 | 缺什么 |
//! |---|---|---|
//! | A | `handle_tick` 到底做什么 | ⚠️ **任务 79 更正**：旧版说"`state != 3` 走一条**不返回**的路径（`0x5ec160`）"——**`0x4785f0` 体内没有 `0x5ec160`**。实测那条路是**收尾**：`0x4786b2` 取 `&BotData` → 判 `state != 3` → `0x478e0e` 再判一次 → drop（`0x47b5e0`/`0x404df0`）→ `0x47864f` 把 `state` 写回 3；体内真正的 panic 是 `0x478ebb` 那条 **`"Failed to join computer player thread."`**（`0x478eda` 调 panic、`0x478ee0 ud2`）。**体内逐步语义仍只到"收尾 + 复位"这一档** |
//! | B | `state` 的取值 | ✅ **任务 79 结案（观测）**：`3` = 空闲/可算（建档 `0x478fe3`；`handle_tick` 收尾 `0x47864f`），`1` = 已 spawn 未收（`0x479dfa`，`rbx = [rsp+0xa8] = 1`，见 `0x4796f9`）。**`0`/`2` 没有任何写入点**（有界否定）：函数块 `0x4785f0–0x47a800` 的 `mov [mem], imm∈{0,1,2,3}` 扫描只命中 `0x47864f`/`0x478ef9`/`0x478fe3`（三条写 3）与 `0x4796f9`（写 1 到栈，再经 `0x479dfa` 进 `BotData`）；**正对照**：同一次扫描确实抓到了这三条 3 与那条 1。运行时两局 100 ms 采样 + 三条写入点，只见 `3→1→3` |
//! | C | `computer_function_thread` 的 x86 地址 | 无锚，靠顺序夹逼（见 §1 表注）；`0x47a300` 的形参表与本模块其它函数不连号，**推断** |
//! | D | `rank_request_event`（66）的载荷第二个字段 | 反编译只看到 `player_id` 与一个常量 `0` |
//! | E | 难度档与 `docs/13-ai.md` 的 `limit` 列（5/5/10/15/25/200）的接口点 | 那列由进化器自己按 `level` 再算一次；本模块只把 `level` 原样塞进 `BotData`（运行时佐证：`level=1000` 的 Bot 在表里就是 `level=1000 / 精英 50 / 每槽 5 / limit 200`） |
//! | F | 房间里"人 + Bot"的上限 | 玩家上限（10）在 `graphwar_player_manager`（`docs/10`）；Bot 是否占这个名额未查 |

use std::collections::HashMap;

/// `graphwar_computer_player_manager` 订阅的三条事件（名字, 名字字节数）。
///
/// 长度就是 `strlen`；反编译里是 `0xa` / `0xf` / `0x14`。
pub const SUBSCRIBED_EVENTS: [(&str, usize); 3] = [
    ("tick_event", 10),
    ("turn_info_event", 15),
    ("entity_removed_event", 20),
];

/// `turn_info_event` 的判别值。
pub const EVENT_TURN_INFO: u8 = 43;
/// `entity_removed_event` 的判别值。
pub const EVENT_ENTITY_REMOVED: u8 = 14;
/// `bot_added_event` 的判别值——**唯一**的建档入口。
pub const EVENT_BOT_ADDED: u8 = 10;

/// `turn_info_event` 的 tag 字面量（`handle_turn_info_event` 断言它）。
pub const TAG_TURN_INFO: u64 = 0x8000_0000_0000_002b;
/// `entity_removed_event` 的 tag 字面量。
pub const TAG_ENTITY_REMOVED: u64 = 0x8000_0000_0000_000e;

/// `BotData` 里那个"可以开始算"的状态值（观测：`state == 3` 才放行）。
///
/// 写入点：`0x478fe3`（建档时 `mov qword [rsp+0x10],3` → 搬进 `BotData+0x00`）、
/// `0x47864f`（`handle_tick` 收尾 `mov qword [r15],3`）、`0x478ef9`（panic 路径复位）。
pub const BOT_STATE_READY: i32 = 3;

/// "已经 spawn、线程还没收"的状态（**观测**，任务 79）。
///
/// `handle_turn_info_event` 过了门、`thread::spawn`（`0x479da4 call 0x438d20`）之后
/// 立刻写 `BotData+0x00 = 1`：`0x479dfa mov qword [r14-0x70],rbx`，其中
/// `rbx = [rsp+0xa8] = 1`（`0x4796f9`）。运行时抓到的完整轨迹是
/// **3 → 1 →（25 s 后）3**：`handle_tick` 的 `0x47864f` 把它写回 3。
pub const BOT_STATE_RUNNING: i32 = 1;

/// 每槽个体数（`add_computer_player` 的第 4 个实参是常数 5）。
///
/// `docs/13-ai.md` §18：字面量 5 = 每槽个体数（1 克隆 + 4 工厂子代），
/// 精英率是 `1/5`。
pub const PER_SLOT_INDIVIDUALS: u32 = 5;

/// 名字池（`.data.rel.ro 0x959cd8` 的 **10 项**，逐项读出来的原文，顺序即池序）。
pub const BOT_NAMES: [&str; 10] = [
    "Deep Thought",
    "HAL",
    "Skynet",
    "Agent Smith",
    "Multivac",
    "Deep Blue",
    "Cleverbot",
    "Alpha Zordon",
    "Wolfram",
    "ChatGPT",
];

/// Bot 脸的固定字面量。
pub const BOT_FACE: &str = "no_face";
/// Bot 帽子的固定字面量。
pub const BOT_HAT: &str = "no_hat";

/// 六档难度的分界（`level` 的**闭区间上界**）。
///
/// 最后一档没有上界；这五个数等于 `docs/13-ai.md` §1 的"代数"列 + 1。
pub const LEVEL_BOUNDS: [u32; 5] = [3, 10, 25, 100, 250];

/// 六档的精英数（`= 种群/5`，`docs/13-ai.md` §1 的"种群"列除以 5）。
pub const ELITE_PER_TIER: [u32; 6] = [10, 20, 30, 40, 50, 50];

/// 六档的 `limit`（复杂度上限）——**调用点直接读到的**。
///
/// `0x3e45c7` 一带：`level < 101 ⇒ r9d = 15`；`101..250 ⇒ 25`；`>= 251 ⇒ 200`；
/// 相邻块里 `level < 4 ⇒ 5`、`11..25 ⇒ 10`。与 `docs/13-ai.md` §1 的 `limit` 列
/// （`5 / 5 / 10 / 15 / 25 / 200`）**逐值一致**。
pub const LIMIT_PER_TIER: [u32; 6] = [5, 5, 10, 15, 25, 200];

/// 六档的皮肤名（与 `res://skins/bot_N.svg` 对得上）。
pub const SKIN_PER_TIER: [&str; 6] = ["bot_0", "bot_1", "bot_2", "bot_3", "bot_4", "bot_5"];

/// `level` → 档号 `0..=5`（照抄 `0x478fc0` 的链式比较，**顺序不能换**）。
pub const fn tier_for_level(level: u32) -> u8 {
    if level < 4 {
        0
    } else if level < 11 {
        1
    } else if level < 26 {
        2
    } else if level < 101 {
        3
    } else if level < 251 {
        4
    } else {
        5
    }
}

/// `level` → 皮肤名。
pub const fn skin_for_level(level: u32) -> &'static str {
    SKIN_PER_TIER[tier_for_level(level) as usize]
}

/// `level` → 精英数（调用点 `0x3e28xx` 的 `case 10` 里那张链）。
///
/// 与 [`ELITE_PER_TIER`] 逐值相同；单独写一遍是为了让"调用点算一次、
/// 建档时用一次"这个**两处各算一次**的形状留在代码里。
pub const fn elite_for_level(level: u32) -> u32 {
    if level < 4 {
        10
    } else if level < 11 {
        20
    } else if level < 26 {
        30
    } else if level < 101 {
        40
    } else {
        50
    }
}

/// `level` → `limit`（复杂度上限）。
pub const fn limit_for_level(level: u32) -> u32 {
    LIMIT_PER_TIER[tier_for_level(level) as usize]
}

/// 名字池取名字。`index` 就是那个随机数（二进制里是 `% 10 * 16` 的索引）。
///
/// 超出范围时回绕，而不是 panic——调用方本来就该先取模。
pub const fn bot_name(index: u64) -> &'static str {
    BOT_NAMES[(index % 10) as usize]
}

/// 一个 Bot 的身份档案（建档时一次性决定，之后不变）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BotProfile {
    /// 随机名（[`BOT_NAMES`] 之一）。
    pub name: &'static str,
    /// 按难度档取的皮肤（[`SKIN_PER_TIER`]）。
    pub skin: &'static str,
    /// 恒为 [`BOT_FACE`]。
    pub face: &'static str,
    /// 恒为 [`BOT_HAT`]。
    pub hat: &'static str,
}

impl BotProfile {
    /// 照抄 `add_computer_player`：名字来自**随机数**，皮肤来自 `level`，
    /// 脸/帽是常量。
    ///
    /// `name_index` 就是那次取模之前的随机数，传进来便于复现与测试。
    pub const fn new(level: u32, name_index: u64) -> Self {
        Self {
            name: bot_name(name_index),
            skin: skin_for_level(level),
            face: BOT_FACE,
            hat: BOT_HAT,
        }
    }
}

/// `bot_added_event` 落到 `add_computer_player` 之后要写进 `BotData` 的两组数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewBot {
    /// 这条 Bot 用的连接号（同时是 Bot 表的键）。
    pub connection_id: u32,
    /// 原始难度参数（= "最多跑几代"）。
    pub level: u32,
    /// 精英数（`= 种群/5`）。
    pub elite_count: u32,
    /// 每槽个体数，恒为 5。
    pub per_slot_individuals: u32,
    /// 复杂度上限（`limit`）。
    pub limit: u32,
}

impl NewBot {
    /// 复刻调用点：`level` 进，其余两个表算出来。
    pub const fn from_level(connection_id: u32, level: u32) -> Self {
        Self {
            connection_id,
            level,
            elite_count: elite_for_level(level),
            per_slot_individuals: PER_SLOT_INDIVIDUALS,
            limit: limit_for_level(level),
        }
    }

    /// 档号。
    pub const fn tier(&self) -> u8 {
        tier_for_level(self.level)
    }
}

/// 一条要广播出去的身份事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotEvent {
    /// `name_request_event`（59）：`{ name, player_id }`。
    NameRequest { player_id: u32, name: &'static str },
    /// `skin_graphics_request_event`（50）。
    SkinGraphicsRequest { player_id: u32, graphics: &'static str },
    /// `face_graphics_request_event`（52）。
    FaceGraphicsRequest { player_id: u32, graphics: &'static str },
    /// `hat_graphics_request_event`（54）。
    HatGraphicsRequest { player_id: u32, graphics: &'static str },
    /// `rank_request_event`（66）。
    RankRequest { player_id: u32 },
}

impl BotEvent {
    /// 线上 tag 字面量（判别值 | 最高位）。
    pub const fn tag(&self) -> u64 {
        0x8000_0000_0000_0000
            | match self {
                BotEvent::NameRequest { .. } => 59,
                BotEvent::SkinGraphicsRequest { .. } => 50,
                BotEvent::FaceGraphicsRequest { .. } => 52,
                BotEvent::HatGraphicsRequest { .. } => 54,
                BotEvent::RankRequest { .. } => 66,
            }
    }

    /// 判别值（低 8 位）。
    pub const fn discriminant(&self) -> u8 {
        (self.tag() & 0xff) as u8
    }
}

/// 建档时要发的 5 条事件（顺序就是反编译里的发送顺序）。
pub fn add_computer_player_events(connection_id: u32, profile: &BotProfile) -> Vec<BotEvent> {
    vec![
        BotEvent::NameRequest {
            player_id: connection_id,
            name: profile.name,
        },
        BotEvent::SkinGraphicsRequest {
            player_id: connection_id,
            graphics: profile.skin,
        },
        BotEvent::FaceGraphicsRequest {
            player_id: connection_id,
            graphics: profile.face,
        },
        BotEvent::HatGraphicsRequest {
            player_id: connection_id,
            graphics: profile.hat,
        },
        BotEvent::RankRequest {
            player_id: connection_id,
        },
    ]
}

/// 一个士兵的 Bot 侧状态。**只有偏移 `+0x50` 的那个 `u8` 有观测**。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SoldierBotData {
    /// `+0x50` 的那个字节：`handle_turn_info_event` 读出来当参数用。语义未知。
    pub byte_50: u8,
}

/// 一个 Bot 的运行时状态。
#[derive(Debug, Clone)]
pub struct BotData {
    /// `+0x00`（= 桶内的 `entry+0x08`）：状态；`== 3` 才放行算函数。
    ///
    /// 三个取值里**只有 3 和 1 有写入点**（任务 79）：3 = 空闲（建档 / `handle_tick` 收尾），
    /// 1 = "已 spawn、还没收"。`0` 与 `2` 在 `.text` 里**找不到写 `BotData+0x00` 的指令**
    /// （有界否定 + 正对照，见模块文档 §7-B）；运行时两局 100 ms 采样也没见过它们。
    pub state: i32,
    /// 建档时那两组数。
    pub new_bot: NewBot,
    /// 第二层哈希表：`soldier_id → SoldierBotData`（桶步长 `0x58`）。
    ///
    /// 起点相对 `BotData` **未逐位钉死**：`0x479b2b` 的 `lea rdi,[r14-0x20]`（= `entry+0x58`）
    /// 与 `0x478a55` 的 `cmp dword [rbp+0x28],0`（= `entry+0xa0`）都指向它那一带。
    pub soldiers: HashMap<u32, SoldierBotData>,
}

impl BotData {
    /// 新建一个处于"可算"状态的 BotData。
    pub fn new(new_bot: NewBot) -> Self {
        Self {
            state: BOT_STATE_READY,
            new_bot,
            soldiers: HashMap::new(),
        }
    }

    /// 门的核心（观测）：`state == 3`。
    pub const fn can_think(&self) -> bool {
        self.state == BOT_STATE_READY
    }
}

/// `turn_info_event` 里本模块关心的两个字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnInfo {
    /// 轮到谁（= 查 Bot 表的键）。
    pub player_id: u32,
    /// 用哪个士兵开火。
    pub soldier_id: u32,
}

/// 过了门之后要交给线程的那一次"思考请求"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkRequest {
    /// 谁在算。
    pub player_id: u32,
    /// 用哪个士兵。
    pub soldier_id: u32,
    /// 该士兵在 [`SoldierBotData`] 里的那个字节（没有记录时是 `0`）。
    pub flag: u8,
    /// 建线程时从 `BotData` 里拿的那两组难度参数。
    pub new_bot: NewBot,
}

/// Bot 表（原版在 `Mutex` 里，这里由调用方决定锁）。
#[derive(Debug, Default, Clone)]
pub struct ComputerPlayers {
    bots: HashMap<u32, BotData>,
}

impl ComputerPlayers {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// `add_computer_player`：写表。身份事件由
    /// [`add_computer_player_events`] 单独给（二进制里是插表**之后**才发）。
    pub fn add_computer_player(&mut self, new_bot: NewBot) {
        self.bots.insert(new_bot.connection_id, BotData::new(new_bot));
    }

    /// `handle_entity_removed_event`（`0x47a520`）：断言 tag=14，然后 erase。
    pub fn handle_entity_removed_event(&mut self, entity_id: u32) -> bool {
        self.bots.remove(&entity_id).is_some()
    }

    /// 查一个 Bot。
    pub fn get(&self, connection_id: u32) -> Option<&BotData> {
        self.bots.get(&connection_id)
    }

    /// 可变的查一个 Bot（给状态机用）。
    pub fn get_mut(&mut self, connection_id: u32) -> Option<&mut BotData> {
        self.bots.get_mut(&connection_id)
    }

    /// `handle_turn_info_event` 的三道门（观测）。过了门返回一次思考请求。
    ///
    /// 门：① tag 必须是 [`TAG_TURN_INFO`]（调用方保证）；② 表非空；③ 该玩家在表里
    /// 且 `state == 3`。**否则返回 `None`——不是"跳过一回合"，是"什么都不做"**
    /// （回合推进仍由 `graphwar_turn_manager` 负责，本模块没有否决权）。
    pub fn handle_turn_info_event(&self, turn: TurnInfo) -> Option<ThinkRequest> {
        if self.bots.is_empty() {
            return None;
        }
        let bot = self.bots.get(&turn.player_id)?;
        if !bot.can_think() {
            return None;
        }
        let flag = bot
            .soldiers
            .get(&turn.soldier_id)
            .map(|s| s.byte_50)
            .unwrap_or(0);
        Some(ThinkRequest {
            player_id: turn.player_id,
            soldier_id: turn.soldier_id,
            flag,
            new_bot: bot.new_bot,
        })
    }

    /// `handle_turn_info_event` + **状态迁移**（任务 79，`0x479dfa`）。
    ///
    /// 与 [`Self::handle_turn_info_event`] 的区别只有一件：过了门的那个 Bot 的
    /// `state` 被置成 [`BOT_STATE_RUNNING`]（原版在 `thread::spawn` **之后**写
    /// `BotData+0x00 = 1`，见 `0x479da4` / `0x479dfa`）。
    /// 之后 `handle_tick` 才把它写回 3 —— 运行时的轨迹是 **3 → 1 → 3**。
    pub fn handle_turn_info_event_with_spawn(&mut self, turn: TurnInfo) -> Option<ThinkRequest> {
        if self.bots.is_empty() {
            return None;
        }
        let bot = self.bots.get(&turn.player_id)?;
        if !bot.can_think() {
            return None;
        }
        let flag = bot
            .soldiers
            .get(&turn.soldier_id)
            .map(|s| s.byte_50)
            .unwrap_or(0);
        let new_bot = bot.new_bot;
        if let Some(b) = self.bots.get_mut(&turn.player_id) {
            b.state = BOT_STATE_RUNNING;
        }
        Some(ThinkRequest {
            player_id: turn.player_id,
            soldier_id: turn.soldier_id,
            flag,
            new_bot,
        })
    }

    /// `handle_tick`（`0x4785f0`）的状态部分：**把每条 entry 的 `state` 写回 3**。
    ///
    /// 逐位：`0x4786ab cmp dword [..],3` → `state == 3` 直接跳到 `0x47864f`；
    /// `state != 3` 先走收尾（drop / 取线程结果那一段，`0x478e0e` 再判一次），
    /// **两条路最后都执行** `0x47864f mov qword [r15],3`。
    /// ⇒ 返回"这次被写回 3 的那些 `player_id`（原本不是 3 的）"。
    ///
    /// ⚠️ 这条不是"跳过一回合"：`state` 回到 3 之后下一次 `turn_info` 才能再过门。
    pub fn handle_tick(&mut self) -> Vec<u32> {
        let mut reset = Vec::new();
        for (id, bot) in self.bots.iter_mut() {
            if bot.state != BOT_STATE_READY {
                reset.push(*id);
                bot.state = BOT_STATE_READY;
            }
        }
        reset.sort_unstable();
        reset
    }

    /// 表里当前"可以动手"的 Bot 数——攻略侧读数，二进制里没有这个函数。
    pub fn ready_count(&self) -> usize {
        self.bots.values().filter(|b| b.can_think()).count()
    }

    /// 表长。
    pub fn len(&self) -> usize {
        self.bots.len()
    }

    /// 表是否为空。
    pub fn is_empty(&self) -> bool {
        self.bots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `level` → 六档的**边界**逐个钉住：`0/3` 一档、`4/10` 一档、`11/25` 一档、
    /// `26/100` 一档、`101/250` 一档、`251` 起一档。
    #[test]
    fn level_bounds_map_to_six_tiers() {
        assert_eq!(tier_for_level(0), 0);
        assert_eq!(tier_for_level(3), 0);
        assert_eq!(tier_for_level(4), 1);
        assert_eq!(tier_for_level(10), 1);
        assert_eq!(tier_for_level(11), 2);
        assert_eq!(tier_for_level(25), 2);
        assert_eq!(tier_for_level(26), 3);
        assert_eq!(tier_for_level(100), 3);
        assert_eq!(tier_for_level(101), 4);
        assert_eq!(tier_for_level(250), 4);
        assert_eq!(tier_for_level(251), 5);
        assert_eq!(tier_for_level(1_000_000), 5);
        // 阈值 = docs/13-ai.md §1 的"代数"列 + 1
        for (bound, generations) in LEVEL_BOUNDS.iter().zip([3u32, 10, 25, 100, 250]) {
            assert_eq!(*bound, generations, "上界 = 代数上限");
            assert_eq!(tier_for_level(generations) + 1, tier_for_level(generations + 1));
        }
    }

    /// 精英数 = `docs/13-ai.md` §1 的"种群"列 / 5，且第五条与第六条相同。
    #[test]
    fn elite_is_population_over_five() {
        let population = [50u32, 100, 150, 200, 250, 250];
        for (tier, pop) in population.iter().enumerate() {
            let level = match tier {
                0 => 0,
                1 => 4,
                2 => 11,
                3 => 26,
                4 => 101,
                _ => 251,
            };
            assert_eq!(tier_for_level(level) as usize, tier);
            assert_eq!(elite_for_level(level), pop / 5);
            assert_eq!(ELITE_PER_TIER[tier], pop / 5);
        }
        assert_eq!(PER_SLOT_INDIVIDUALS, 5, "第 4 个实参是常数 5");
    }

    /// `limit` 六档 = `docs/13-ai.md` §1 的 `limit` 列（调用点直接读到的立即数）。
    #[test]
    fn limit_matches_the_documented_column() {
        let probe = [0u32, 4, 11, 26, 101, 251];
        let documented = [5u32, 5, 10, 15, 25, 200];
        for (tier, level) in probe.iter().enumerate() {
            assert_eq!(limit_for_level(*level), documented[tier], "tier {tier}");
            assert_eq!(LIMIT_PER_TIER[tier], documented[tier]);
            assert_eq!(NewBot::from_level(1, *level).limit, documented[tier]);
        }
    }

    /// 名字池 6 项、与 `level` **无数据流**：同一个 level 换下标就换名字。
    #[test]
    fn name_is_independent_of_level() {
        assert_eq!(BOT_NAMES.len(), 10);
        assert_eq!(bot_name(0), "Deep Thought");
        assert_eq!(bot_name(5), "Deep Blue");
        assert_eq!(bot_name(9), "ChatGPT");
        assert_eq!(bot_name(10), "Deep Thought", "% 10 回绕");
        let a = BotProfile::new(0, 0);
        let b = BotProfile::new(0, 5);
        assert_eq!(a.skin, b.skin, "同一个 level ⇒ 同一个皮肤");
        assert_ne!(a.name, b.name);
        assert_eq!(a.face, BOT_FACE);
        assert_eq!(a.hat, BOT_HAT);
    }

    /// 建档会按顺序发 5 条 request；tag 的低 8 位就是判别值。
    #[test]
    fn profile_events_cover_five_requests() {
        let profile = BotProfile::new(26, 4);
        assert_eq!(profile.skin, "bot_3");
        let events = add_computer_player_events(42, &profile);
        let tags: Vec<u8> = events.iter().map(|e| e.discriminant()).collect();
        assert_eq!(tags, vec![59, 50, 52, 54, 66]);
        assert_eq!(events[0], BotEvent::NameRequest { player_id: 42, name: "Multivac" });
        assert_eq!(events[1].tag(), 0x8000_0000_0000_0032);
    }

    /// 三道门：表空 / 不在表里 / `state != 3` 都返回 `None`。
    #[test]
    fn turn_info_gate_requires_a_ready_bot() {
        let mut bots = ComputerPlayers::new();
        let turn = TurnInfo { player_id: 7, soldier_id: 70 };
        assert_eq!(bots.handle_turn_info_event(turn), None, "表空");

        bots.add_computer_player(NewBot::from_level(7, 26));
        let req = bots.handle_turn_info_event(turn).expect("state=3 应放行");
        assert_eq!(req.new_bot.level, 26);
        assert_eq!(req.new_bot.tier(), 3);
        assert_eq!(req.flag, 0, "没有 SoldierBotData 时读 0");

        bots.get_mut(7).unwrap().state = 0;
        assert_eq!(bots.handle_turn_info_event(turn), None, "state != 3");

        bots.get_mut(7).unwrap().state = BOT_STATE_READY;
        assert_eq!(
            bots.handle_turn_info_event(TurnInfo { player_id: 8, soldier_id: 70 }),
            None,
            "不是 Bot"
        );
        assert_eq!(bots.ready_count(), 1);
    }

    /// `Computer.state` 的整条时间线（任务 79，运行时观测的代码版）：
    /// 建档 **3** → 过门 spawn **1** → `handle_tick` 收尾 **3**。
    #[test]
    fn state_goes_three_one_three() {
        let mut bots = ComputerPlayers::new();
        bots.add_computer_player(NewBot::from_level(232, 1000));
        assert_eq!(bots.get(232).unwrap().state, BOT_STATE_READY, "建档 = 3");
        assert_eq!(bots.get(232).unwrap().new_bot.limit, 200, "level 1000 落进第 6 档");

        let turn = TurnInfo { player_id: 232, soldier_id: 70 };
        assert!(bots.handle_turn_info_event_with_spawn(turn).is_some());
        assert_eq!(bots.get(232).unwrap().state, BOT_STATE_RUNNING, "spawn 之后 = 1");
        assert_eq!(bots.ready_count(), 0, "1 期间不能再过门");
        assert_eq!(bots.handle_turn_info_event_with_spawn(turn), None, "state != 3 ⇒ 拦");

        assert_eq!(bots.handle_tick(), vec![232], "handle_tick 把它写回 3");
        assert_eq!(bots.get(232).unwrap().state, BOT_STATE_READY);
        assert_eq!(bots.handle_tick(), Vec::<u32>::new(), "已经是 3 ⇒ 无事可做");
        assert!(bots.handle_turn_info_event_with_spawn(turn).is_some(), "又能算了");
    }

    /// 实体删除会把那条 Bot 从表里摘掉（断言 tag=14 的那一段）。
    #[test]
    fn entity_removed_erases_the_bot() {
        let mut bots = ComputerPlayers::new();
        bots.add_computer_player(NewBot::from_level(7, 4));
        assert_eq!(bots.len(), 1);
        assert!(bots.handle_entity_removed_event(7));
        assert!(!bots.handle_entity_removed_event(7), "再删一次返回 false");
        assert!(bots.is_empty());
    }

    /// 订阅的三条事件，名字与长度对得上（长度就是 strlen）。
    #[test]
    fn subscription_names_and_lengths() {
        for (name, len) in SUBSCRIBED_EVENTS {
            assert_eq!(name.len(), len, "{name}");
        }
    }
}
