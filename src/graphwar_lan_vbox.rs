//! `graphwar_lan_vbox.rs` —— **大厅的房间列表容器**（原版 `GraphwarLanVBox`）。
//!
//! # 这个模块是什么
//!
//! `VBoxContainer` 的 GDExtension 子类，类名 `GraphwarLanVBox`，挂在
//! `lan_lobby.tscn` 的 `.../ScrollContainer/GraphwarLanVBox`（`docs/16-client.md` §160 的真机 dump）。
//! 它只做三件事，**没有第四件**：
//!
//! 1. **每帧把"房间表"投影成行节点**：表里有的地址 → 有行；表里没了 → 行被 `queue_free`；
//!    行的内容（房名、人数、4 个模式图标、Join 是否可点）**每帧从活数据重取**，不是建行时的快照。
//! 2. **Join 是异步的**：点 Join 不直接发连接，而是把该行的 `SocketAddr` 塞进自己的一条
//!    `mpmc` 通道；**下一帧** `process` 排空通道时才 `start_client_to_remote_server`。
//! 3. **Local / Global 是同一个控件的两套源**：一个 bool 字段选 LAN 广播器的表还是
//!    broker 的表（两段逐字节相同的行更新代码，各带一套自己的表）。
//!
//! 它**不消费任何 `Event`**（`docs/07-protocol.md` 的 74 个事件一个都不在这里）：没有
//! `ready`、没有 `subscribe_to_events_event_handler`、没有任何 `Event` 判别值比较。
//! 房间表由 `graphwar_lan_broadcaster_v6` / `graphwar_server_broker_handler` 各自维护，
//! 本模块每帧**直接读它们的内存表**（`docs/144-lobby-fixes.md` §1.1 / §3.1 已用运行时
//! 现象 + 指令读出同一条结论）。
//!
//! # 地址口径
//!
//! * 本文件的 **x86 地址一律是 ELF 虚地址（`objdump` 口径）**；Ghidra / REA = **+`0x100000`**。
//! * **arm64 地址** = Android 2.0.60 的 `libgraphwarrust.so`（`.symtab` 未 strip）虚地址。
//!   它是本文的**名字与函数边界来源**，**不能**当 x86 地址用：两份构建的函数被整体重排
//!   （`docs/02-module-map.md` §19）。
//! * 复现取法（与 `docs/144` §5 同一条命令）：从 `Graphwar II_1.0.apks` 解
//!   `split_config.arm64_v8a.apk` 里的 `lib/arm64-v8a/libgraphwarrust.so`，然后
//!   `readelf -sW … | c++filt | grep lan_vbox` + `llvm-objdump -d …`。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条）
//!
//! * **观测** —— 能指到具体指令 / `.symtab` 记录 / `.rodata` 字节 / 真机帧；
//! * **推断** —— 有依据但没证，写明缺什么；
//! * **未知** —— 没解出来（见 [`UNKNOWNS`]）。
//!
//! # 名字与地址：`fixtures/name_to_addr.tsv` 只给了 1 个唯一锚
//!
//! ```text
//! awk -F'\t' '$1=="graphwar_lan_vbox"' fixtures/name_to_addr.tsv
//! ```
//!
//! 7 行：`process` = **唯一锚**（x86 `0x4f90b0`，16 个双向唯一 `(file,line,col)`）；
//! `__register_constants` / `update_game_list` / `init` = **歧义**；
//! `__default_virtual_call` / `class_id` / `{{closure}}` = **未知**。
//!
//! ⚠️ **不给歧义项挑候选**（`AGENTS.md` §0 第 1 条）：歧义项登记成候选集，
//! 每个"确定"结论只来自 arm64 `.symtab`（权威边界 + 真名）或 `process` 那一个唯一锚。
//!
//! ## arm64 2.0.60 `.symtab` 里本模块的 13 个 `FUNC`
//!
//! | arm64 vaddr | size | 名字 | 引用的本文件 `(line,col)` | x86 对位 |
//! |---|---|---|---|---|
//! | `0x4380c0` | 156 | `__default_virtual_call` | — | 未知（无锚） |
//! | `0x43815c` | 56 | `class_id` | — | 未知（无锚） |
//! | `0x43e7e8` | 400 | `__register_methods` | 54:1 | 歧义（见下） |
//! | `0x43e978` | 400 | `__register_constants` | 54:1 | 歧义（见下） |
//! | `0x440fbc` | 156 | `__virtual_call` | — | 未知（无锚） |
//! | `0x441058` | 156 | `__virtual_call` | — | 未知（无锚） |
//! | `0x443240` | 512 | `__inner_init` | **145:1** | `0x4e8f80`（跨度 592） |
//! | `0x443440` | 504 | `__inner_init` | **30:10** | `0x4e9380`（跨度 528） |
//! | `0x443638` | 448 | `__inner_init` | 54:1 | 歧义（见下） |
//! | `0x4437f8` | 428 | `__inner_init` | 54:1 | 歧义（见下） |
//! | `0x449124` | 264 | `update_game_list::{{closure}}::{{closure}}` | **59:40** | 歧义（见下） |
//! | `0x450a90` | 400 | `<… as IVBoxContainer>::init` | 无本文件锚 | 未归属 |
//! | `0x450c20` | 14524 | `<… as IVBoxContainer>::process` | **15 个**（90–174 行） | **`0x4f90b0`** |
//!
//! 另有 6 个 `drop_in_place` 胶水（`0x3941ec` / `0x395298` / `0x3af7c4` / `0x457cc8` /
//! `0x4581d0` / `0x4d7d54`）、4 个 `_1_6__init` 函数指针槽（`0x8aea70`–`0x8aea88`）、
//! `CLASS_ID`（`0x8b40b8`，8 B）、`__registration_storage`（`0x8b41b0`，56 B）。
//!
//! ### 歧义项的候选集（**不替原版挑**）
//!
//! `fixtures/module_funcs.tsv` 把 11 个 x86 地址归到本模块：`0x4dec00`(跨度 528)、
//! `0x4e3b00`(384)、`0x4e3c80`(384)、`0x4e8f80`(592)、`0x4e91d0`(432)、`0x4e9380`(528)、
//! `0x4e9590`(416)、`0x4f01c0`(240)、`0x4f90b0`(16640)、`0x500070`(320)、`0x5005b0`(608)
//! （括号里是到下一个 Ghidra 函数起点的跨度，**≥ 真实大小**）。
//!
//! * `(54,1)` 的 4 个（`0x4e3b00` / `0x4e3c80` / `0x4e91d0` / `0x4e9590`）↔ arm64 的 4 个
//!   `(54,1)`（两个 `__register_*` + 两个 `__inner_init`）。4↔4、大小序自洽，但**没有一对一
//!   证据** ⇒ `__register_methods` 与 `__register_constants` 谁是哪个，**未知**。
//! * `(59,40)` 的 4 个（`0x4dec00` / `0x4f01c0` / `0x500070` / `0x5005b0`）↔ arm64 **只有一个**。
//!   ⇒ x86 那份把 `update_game_list` 的多份单态化各留了副本，arm64 只留一份；
//!   "哪个是本体" **未知**。
//! * `init`（arm64 400 B）**没有本文件锚** ⇒ 被 `fixtures/module_funcs.tsv` 归到了别的模块。**未知**。
//!
//! # 版本差异（2.0.60 vs 2024-07-30）
//!
//! 2024 版 `.symtab` 里本模块有 `get_join_address` / `process` / `update_game_list`
//! （`docs/02-module-map.md` §18）。**2.0.60 arm64 里没有 `get_join_address` 符号**
//! （`readelf -sW … | grep get_join_address` = 0 命中）⇒ 删了还是内联了，**未知**；
//! 本文件不凭名字造这个函数。
//!
//! # 数据模型（全部观测，arm64）
//!
//! 实例字段总大小 **`0x70`**（`init` 逐字段写完正好到 `+0x68`）：
//!
//! | 偏移 | 字段 | 证据 |
//! |---|---|---|
//! | `+0x00` | `Sender<SocketAddr>`（16 B） | `init` 在 `0x450b54` 写 `{1, ptr}`；闭包 `0x4491d8` 调 `mpmc::Sender::send`，`&self` 就是它；拷贝侧用 `ldxr/stxr` 原子加一（`0x4522a8`） |
//! | `+0x10` | `Receiver<SocketAddr>`（16 B） | `try_recv(&self+0x10)`（`0x450c54`、`0x450d98`） |
//! | `+0x20` | `base: Base<VBoxContainer>`（24 B） | `assert_constructed(self+0x20)` + `Gd::from_obj_sys_weak([self+0x28])` → `is_visible_in_tree`（`0x450da8`–`0x450dd8`） |
//! | `+0x38` | `HashMap<SocketAddr, Gd<行>>`（48 B） | 清理循环 `0x452018` 读 `[self+0x38]`=ctrl / `[self+0x40]`=mask / `[self+0x48]` / `[self+0x50]`=len；桶步长 `0x30`=48（键 32 + `Gd` 16）；`init` 用三条 16 B 写清零（`0x450b58`/`0x450b64`/`0x450b70`） |
//! | `+0x68` | `show_global: bool` | `init` `strb wzr,[x20,#0x68]`（默认 **false** = Local 页）；`process` `ldrb w8,[x19,#0x68]`（`0x450ed4`）选源 |
//!
//! 通道载荷类型是**硬证据**：`init` 的 unwind 路径里
//! `drop_in_place<Counter<mpmc::list::Channel<core::net::socket_addr::SocketAddr>>>`
//! （`0x450bf0` 引用 `0x433b54`）⇒ **`Sender`/`Receiver<SocketAddr>`、无界 list 通道**。
//!
//! ## 两套源表（都在 `GraphwarRootNode` 里）
//!
//! `process` 先 `get_node_by_name(self, "GraphwarRootNode", 16)`（字面量 `.rodata` `0x198660`），
//! 再取 `root+0x1668`（模块容器）与 `root+0x16a0`（贴图加载器）：
//!
//! | `show_global` | 模块槽 | 它的表 | 逐行取值的函数 |
//! |---|---|---|---|
//! | `false`（Local） | `container+0x128` | `container+0x1a0`（桶步长 `0x78`=120） | `GraphwarLanBroadcasterV6::get_game_info`（arm64 `0x3ddce8`） |
//! | `true`（Global） | `container+0x240` | `container+0x248` | `GraphwarServerBrokerHandler::get_game_info`（arm64 `0x489b98`） |
//!
//! 两条分支各有一份**逐字节相同**的行更新代码（arm64 `0x450f70`–`0x452170` 与
//! `0x45218c`–`0x4533a0`，主循环跨度都是 `0xb14`）。这就是 `process` 有 **14 524 B** 的原因：
//! 不是"一个大函数写了一堆逻辑"，而是**同一段逻辑 ×2**。
//!
//! # `process` 的五步（观测；arm64 地址）
//!
//! ```text
//! ① 0x450c54 / 0x450d98  loop { try_recv(&self+0x10) }          // 排空 Join 通道
//!       Ok  → root.backend.start_client_to_remote_server(&请求)   // 0x407b9c
//!       tag == 2 → 0x450da8（收工，进后面四步；见 UNKNOWNS 第 1 条）
//! ② 0x450dd8  if !self.base().is_visible_in_tree() { return }     // 不可见就一帧都不刷
//! ③ 0x450e50  root = self.get_node("GraphwarRootNode")
//!    0x450ed4  按 self+0x68 选 Local / Global 源表
//! ④ 逐行：源表每个 (addr, info) → self.rows.entry(addr)（0x451108 / 0x452320 的
//!    hashbrown::rustc_entry）；缺行 → load 场景 + instantiate + add_child（一次），
//!    然后无条件刷新房名 / 人数 / 4 个图标 / Join 可点性
//! ⑤ 清理：遍历 self.rows，源表里已没有该地址 → queue_free(行) 并移除（0x452018–0x452110）
//! ```
//!
//! 第 ⑤ 步的判据是**再查一次源表**（`get_game_info(addr)` 返回 null 就删），不是"比两份快照"。
//! ⇒ 出大厅后广播器一停，自己那一行下一帧就没了（与 `docs/144` §1.2 的运行期结论一致）。
//!
//! # 行节点：**新场景实例 + 按键缓存复用**（不是对象池，也不是整页重建）
//!
//! 每个新地址：`load::<PackedScene>("res://game_room_entry.tscn", 26)`（`0x4511a8`）→
//! `instantiate_as()`（`0x4511d4`）→ `add_child_full(…)`（`0x45126c`，就是普通 `add_child`）。
//! 已存在的地址**不重建**，只刷新字段（`self.rows.entry(addr)` 命中）。
//!
//! 行场景里的 7 个节点**全部按名字递归查找**（`GraphwarNodes::get_node_by_name_rec`）：
//!
//! | 名字 | 长度 | `.rodata`（arm64 / x86） | 拿它做什么 |
//! |---|---|---|---|
//! | `JoinButton` | 10 | `0x19dd01` / `0x1c4394` | 挂 Join 处理器 + `set_disabled` |
//! | `RoomName` | 8 | `0x179000` / `0x19ef98` | `Label::set_text(房名)` |
//! | `PlayerCount` | 11 | `0x19dd0b` / `0x1c439e` | `Label::set_text("  N  ")` |
//! | `AxisModeButton` | 14 | `0x19dd16` / `0x1c43a9` | `set_button_icon(贴图表[模式])` |
//! | `FunctionModeButton` | 18 | `0x19dd24` / `0x1c43b7` | 同上 |
//! | `TurnModeButton` | 14 | `0x19dd36` / `0x1c43c9` | 同上 |
//! | `TimeModeButton` | 14 | `0x19dd44` / `0x1c43d7` | 同上 |
//!
//! **x86 侧对位（独立于大小的第二条证据）**：x86 全 `.text` 里引用
//! `res://game_room_entry.tscn`（`0x1c43e5`）的指令**只有 2 条** —— `0x4f97ed`、`0x4fac67`，
//! 两条都在 `process`（`0x4f90b0`–`0x4fd1b0`）里；`JoinButton` 4 条（`0x4f9942`/`0x4f9cef`/
//! `0x4fadc1`/`0x4fb183`，每分支 2 条）；`RoomName` 2 条（`0x4f9db9`/`0x4fb24f`）；
//! `PlayerCount` 2 条（`0x4f9e7c`/`0x4fb30a`）。**全都落在 `process` 一个函数里**，
//! 且 2/4/2/2 的分布与 arm64 的"两分支各一套"逐条相符。〔观测〕
//!
//! 节点树（`game_room_entry.tscn`，20 节点）：
//! `NameContainer/RoomName`、`PlayersContainer/PlayerCount`、
//! `ModeContainer/OptionsContainer/{Axis,Function,Turn,Time}ModeButton`、
//! `JoinButtonContainer/JoinButton`（`docs/16-client.md` §160 的真机 dump；`docs/92` 的 UI 清单同）。
//!
//! ## 人数是**补了空格的字符串**
//!
//! `PlayerCount` 的文本不是裸数字：构造时 `strh w9(0x2020), [buf, len]` 往后写两个空格
//! 再 `Label::set_text`（arm64 `0x4517bc` / `0x4517f8`）；`0x2020` 就是两个 `' '`。
//! 与真机 dump 的字段值 `"  0  "` / `"  1  "`（`docs/16` §160）逐字符相符
//! ⇒ **`"  {n}  "`（前后各 2 个空格）**。〔观测〕
//!
//! ## Join 可点性 = 该行记录里的一个 bool
//!
//! `0x4515a4` 从一个字节取 `w1`（`ldurb w1,[x8,#-0xa]`），`0x4515ac` 调
//! `BaseButton::set_disabled(&join_button, w1)`。⇒ Join 的禁用直接由该行记录的**一个 bool 字节**
//! 决定。9 字段 `GameInfo`（`docs/09` §1.1.1）里唯一的布尔是 `locked`
//! ⇒ [`RoomEntry::join_enabled`] = `!locked`。〔观测：字节→`set_disabled`；**推断**：那个字节叫 `locked`〕
//!
//! # Join：不是"点了就直连"，是"点了投递、下一帧才连"
//!
//! 给 JoinButton 挂的处理器是一个 **56 字节的 boxed 闭包**（`0x4513b0` 的
//! `__rust_alloc(0x38, 8)`），环境正好是
//! `{ sender: Sender<SocketAddr>（16 B）, addr: SocketAddr（32 B）, container: ptr（8 B） }`。
//! 它被 push 进一个 `Vec<Box<dyn FnMut(&str)>>`（元素是 16 B 的 fat pointer，
//! push 代码在 `0x4513e4`–`0x451410`）。闭包本体在 arm64 **`0x449124`**（264 B，就是
//! `.symtab` 里 `update_game_list::{{closure}}::{{closure}}` 那一条）：
//!
//! ```text
//! 0x449174  container+0x128 . get_game_info(&env.addr)   // 先查 LAN 表
//! 0x449188  container+0x240 . get_game_info(&env.addr)   // 再查 broker 表
//!           都没有 → 直接返回（这一行已经不是房间了）
//!           u16 tag = 记录+0x18；tag == 2 → 不投递
//! 0x4491d8  env.sender.send(记录+0x18 起的 32 字节)        // = SocketAddr
//! ```
//!
//! ⇒ **按下的那一刻会重新查两张表**（`docs/144` §3.1 的"现取"结论在这一层也成立），
//! 然后用**本控件自己的通道**把地址丢给自己；`process` 的 ① 步下一帧才
//! `start_client_to_remote_server`。这样"连点两下"在**同一帧内**不会发两次连接
//! （`docs/144` §6 第 3 条把那个闸门字段的复位时机列为未知，这里给出它的实现位置）。
//!
//! # 排序：**没有排序**
//!
//! * `process`（14524 B / x86 跨度 16640 B）里 **0 处** `sort*`，也**没有** `BTreeMap`。
//! * 行的**插入顺序** = 源表 `HashMap<SocketAddr, …>` 的迭代顺序；
//!   行的**显示顺序** = `VBoxContainer` 子节点顺序 = `add_child` 的顺序。
//! * `HashMap` 默认 hasher 是 `RandomState`（每进程随机种子）⇒ 同一张房间表，
//!   **两次启动的显示顺序可以不同**。〔推断：只证到"没有排序 + 顺序来自 hash 迭代"，
//!   跨进程顺序变化未做运行时对拍〕
//! * **去重**由表本身完成：键是 `SocketAddr`，同一地址只可能有一行
//!   （`self.rows` 也是以 `SocketAddr` 为键的 `HashMap`）⇒ 不存在"重复行"。
//!
//! 对攻略/复现的含义：**大厅里房间的先后顺序不是信息**，不要按顺序写任何结论。
//!
//! # 与 `client/lan_lobby.gd` 的出入（**只登记，不改 `client/`**）
//!
//! | # | 原版（本文证据） | 我们的 `client/` | 影响 |
//! |---|---|---|---|
//! | 1 | 行按 `SocketAddr` 键缓存、**复用**同一节点，只改 4 个字段（`self+0x38` 的 `HashMap` + `rustc_entry`） | `_rebuild_list()` 整页重建（近期才加每帧 `_sync_list()` 只改文本） | 滚动位置/动画在人数变化时被重置 |
//! | 2 | Join = 投递地址到内部通道，**下一帧**才连 | Join 直接调 `connect_to()` | 连点两下可能连两次；原版一帧内天然去重 |
//! | 3 | 没有地址就没有行（表键就是地址） | 有 4 条**自定**占位行（无地址、不可点） | 见 `docs/144` §2 |
//! | 4 | `PlayerCount` 文本是 `"  {n}  "`（前后各两空格） | 直接写数字 | 列宽/对齐的像素差 |
//! | 5 | `locked` → `JoinButton.disabled` | 占位行用 `mouse_filter = IGNORE` 代替"不可点" | 语义相近、实现不同 |
//! | 6 | 无排序 | 无排序 | 一致 |
//!
//! # 本文件的边界
//!
//! Godot 运行时（`load` / `instantiate` / `add_child` / `set_text` / `set_button_icon` /
//! `queue_free` / `is_visible_in_tree`）**只写意图**，不模拟引擎；可测的部分（行的增删判据、
//! 人数格式、Join 队列、选源、顺序不变性）写成纯函数 + 模型。

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;

// ---------------------------------------------------------------------------
// 1. 常量与字面量
// ---------------------------------------------------------------------------

/// 原版类名。档位：**观测**（arm64 `.symtab` 的 `GraphwarLanVBox`；`docs/02-module-map.md`
/// §16.3 记的短类名 `@0x1c40fd` / `0x1bfcb0`）。
pub const CLASS: &str = "GraphwarLanVBox";

/// 基类。档位：**观测**（`.symtab` 里的 `IVBoxContainer::process` / `IVBoxContainer::init`）。
pub const BASE_CLASS: &str = "VBoxContainer";

/// 原版源文件名（`panic::Location` 的 `file` 段）。档位：**观测**。
pub const SOURCE_FILE: &str = "src/graphwar_lan_vbox.rs";

/// 行场景的路径字面量。档位：**观测**（arm64 `.rodata` `0x19dd52` / x86 `0x1c43e5`；
/// `process` 的 `0x4511a8` / `0x4523cc` 两处 `save_load::load` 传入）。
pub const ROW_SCENE: &str = "res://game_room_entry.tscn";

/// [`ROW_SCENE`] 的字节数（指令里的立即数 `0x1a`）。档位：**观测**。
pub const ROW_SCENE_LEN: u32 = 26;

/// x86 侧引用 [`ROW_SCENE`] 的指令地址（全 `.text` 只有 2 条，都在 `process` 里）。
/// 档位：**观测**（`R_X86_64` RIP 相对操作数穷举）。⇒ 这是 `process` = `0x4f90b0` 的第二条独立证据。
pub const ROW_SCENE_X86_REFS: [u64; 2] = [0x4f97_ed, 0x4fac_67];

/// 从本控件向上找根节点用的名字。档位：**观测**（arm64 `.rodata` `0x198660`，长度 `0x10`=16；
/// `process` `0x450cf8` / `0x450e50` 的 `GraphwarNodes::get_node_by_name(…, name, 16)`）。
pub const ROOT_NODE_NAME: &str = "GraphwarRootNode";

/// 行场景里本模块读的 7 个节点名与指令里的长度。档位：**观测**。
///
/// 顺序 = `process` 里出现的顺序（`JoinButton` 先挂处理器后 `set_disabled`，再房名/人数，
/// 最后 4 个模式图标）。字面量地址见模块头那张表。
pub const ROW_NODE_NAMES: [(&str, u32); 7] = [
    ("JoinButton", 10),
    ("RoomName", 8),
    ("PlayerCount", 11),
    ("AxisModeButton", 14),
    ("FunctionModeButton", 18),
    ("TurnModeButton", 14),
    ("TimeModeButton", 14),
];

/// x86 侧引用 `JoinButton` 字面量（`0x1c4394`）的 4 条指令：每分支 2 条
/// （挂处理器 + 设图标/禁用）。档位：**观测**。
pub const JOIN_BUTTON_X86_REFS: [u64; 4] = [0x4f99_42, 0x4f9c_ef, 0x4fad_c1, 0x4fb1_83];

/// 人数文本两侧各补几个空格。档位：**观测**（`0x2020` = 两个 `' '` 写到 `PlayerCount`
/// 文本尾部，`0x4517bc`）+ 真机 dump 的 `"  0  "`（`docs/16-client.md` §160）。
pub const PLAYER_COUNT_PAD: usize = 2;

/// `Sender` 字段偏移。档位：**观测**（见模块头的数据模型表）。
pub const SENDER_OFFSET: usize = 0x00;

/// `Receiver` 字段偏移。档位：**观测**（`try_recv(&self+0x10)`）。
pub const RECEIVER_OFFSET: usize = 0x10;

/// `Base<VBoxContainer>` 字段偏移。档位：**观测**。
pub const BASE_OFFSET: usize = 0x20;

/// 行缓存 `HashMap<SocketAddr, Gd<行>>` 的偏移。档位：**观测**（清理循环 `0x452018`）。
pub const ROWS_OFFSET: usize = 0x38;

/// `show_global` 标志的偏移。档位：**观测**（`init` 清零、`process` `0x450ed4` 读）。
pub const SHOW_GLOBAL_OFFSET: usize = 0x68;

/// 实例结构总大小。档位：**观测**（`init` 写到 `+0x68` 为止，共 `0x70`）。
pub const INSTANCE_SIZE: usize = 0x70;

/// `show_global` 的初值：**false = Local 页**。档位：**观测**（`init` `strb wzr,[x20,#0x68]`）；
/// 语义"false = Local"是**推断**（依据：false 分支读的是 LAN 广播器的表）。
pub const DEFAULT_SHOW_GLOBAL: bool = false;

/// 行缓存的桶步长（字节）。档位：**观测**（`0x4514c4` `mov w12,#0x30`；`SocketAddr` 32 + `Gd` 16）。
pub const ROW_BUCKET_BYTES: usize = 48;

/// LAN 广播器那张表的桶步长。档位：**观测**（`docs/144` §1.1 记的 `0x78`=120 B）。
pub const LAN_BUCKET_BYTES: usize = 0x78;

/// 根节点里模块容器的偏移。档位：**观测**（`[root+0x1668]`，`process` `0x450e80`）。
/// "它是模块容器"是**推断**：`+0x128` / `+0x240` 两个槽各自接受
/// `GraphwarLanBroadcasterV6::get_game_info` / `GraphwarServerBrokerHandler::get_game_info` 的 `&self`。
pub const ROOT_MODULE_CONTAINER_OFFSET: usize = 0x1668;

/// 根节点里贴图加载器的偏移。档位：**观测**（`[x1+0x16a0]`，`0x450f94`）；
/// "它是 `GraphwarTextureLoader`"是**推断**（模式 → 贴图的 `HashMap<_, Gd<Texture2D>>`，
/// 桶步长 `0x18`、键比较 1 字节；模块清单里正有 `get_time_mode_texture` 一族）。
pub const ROOT_TEXTURE_LOADER_OFFSET: usize = 0x16a0;

/// 模块容器里 LAN 广播器的槽。档位：**观测**（`0x450f1c`）。
pub const CONTAINER_LAN_SLOT: usize = 0x128;

/// LAN 广播器里房间表的偏移。档位：**观测**（`0x45218c`）。
pub const LAN_TABLE_OFFSET: usize = 0x1a0;

/// 模块容器里 broker 处理器的槽。档位：**观测**（`0x450edc`）。
pub const CONTAINER_BROKER_SLOT: usize = 0x240;

/// broker 处理器里房间表的偏移。档位：**观测**（`0x450f74`）。
pub const BROKER_TABLE_OFFSET: usize = 0x248;

/// 一个房间记录里本模块读的项数（房名 + 人数 + `locked` + 4 个模式）。
/// 档位：**观测**（7 个节点各取一项）+ `docs/09` §1.1.1 的 9 字段清单。
pub const CONSUMED_FIELDS: usize = 7;

// ---------------------------------------------------------------------------
// 2. 函数表
// ---------------------------------------------------------------------------

/// 单条函数的证据记录。
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    /// **arm64** 2.0.60 的 ELF 虚地址（权威边界来自 `.symtab`）。
    pub arm64: u64,
    /// `.symtab` 给的字节数（arm64 是精确值）。
    pub size: u32,
    /// `.symtab` 里的名字（未修饰）。
    pub name: &'static str,
    /// 它引用的 `src/graphwar_lan_vbox.rs` 的 `(line, col)`；`None` = 一个锚都没有。
    pub anchor: Option<(u32, u32)>,
    /// x86 2.0.60 的对位地址；`None` = 没有一对一证据（**不猜**）。
    pub x86: Option<u64>,
    /// 该行的对位档位。
    pub link: Link,
}

/// 跨架构对位的档位（与 `fixtures/name_to_addr.tsv` 的 `join_kind` 同口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// 唯一锚：`(file,line,col)` 两侧一对一。
    UniqueAnchor,
    /// 该锚两侧不是一对一（内联 / macro）⇒ **不替原版挑**。
    Ambiguous,
    /// 没有锚，名字也落不到地址上。
    Unknown,
    /// 有锚，但 x86 侧被归到了别的模块（`init` 就是这种）。
    Unattributed,
}

/// 13 个 `FUNC`（arm64 权威）。档位：地址 / 大小 / 名字 **观测**；x86 列见 [`Link`]。
pub const FUNCTIONS: [FunctionEntry; 13] = [
    FunctionEntry {
        arm64: 0x4380_c0,
        size: 156,
        name: "__default_virtual_call",
        anchor: None,
        x86: None,
        link: Link::Unknown,
    },
    FunctionEntry {
        arm64: 0x4381_5c,
        size: 56,
        name: "class_id",
        anchor: None,
        x86: None,
        link: Link::Unknown,
    },
    FunctionEntry {
        arm64: 0x43e7_e8,
        size: 400,
        name: "__register_methods",
        anchor: Some((54, 1)),
        x86: None,
        link: Link::Ambiguous,
    },
    FunctionEntry {
        arm64: 0x43e9_78,
        size: 400,
        name: "__register_constants",
        anchor: Some((54, 1)),
        x86: None,
        link: Link::Ambiguous,
    },
    FunctionEntry {
        arm64: 0x440f_bc,
        size: 156,
        name: "__virtual_call",
        anchor: None,
        x86: None,
        link: Link::Unknown,
    },
    FunctionEntry {
        arm64: 0x4410_58,
        size: 156,
        name: "__virtual_call",
        anchor: None,
        x86: None,
        link: Link::Unknown,
    },
    FunctionEntry {
        arm64: 0x4432_40,
        size: 512,
        name: "__inner_init",
        anchor: Some((145, 1)),
        x86: Some(0x4e8f_80),
        link: Link::UniqueAnchor,
    },
    FunctionEntry {
        arm64: 0x4434_40,
        size: 504,
        name: "__inner_init",
        anchor: Some((30, 10)),
        x86: Some(0x4e93_80),
        link: Link::UniqueAnchor,
    },
    FunctionEntry {
        arm64: 0x4436_38,
        size: 448,
        name: "__inner_init",
        anchor: Some((54, 1)),
        x86: None,
        link: Link::Ambiguous,
    },
    FunctionEntry {
        arm64: 0x4437_f8,
        size: 428,
        name: "__inner_init",
        anchor: Some((54, 1)),
        x86: None,
        link: Link::Ambiguous,
    },
    FunctionEntry {
        arm64: 0x4491_24,
        size: 264,
        name: "update_game_list::{{closure}}::{{closure}}",
        anchor: Some((59, 40)),
        x86: None,
        link: Link::Ambiguous,
    },
    FunctionEntry {
        arm64: 0x450a_90,
        size: 400,
        name: "<GraphwarLanVBox as IVBoxContainer>::init",
        anchor: None,
        x86: None,
        link: Link::Unattributed,
    },
    FunctionEntry {
        arm64: 0x450c_20,
        size: 14524,
        name: "<GraphwarLanVBox as IVBoxContainer>::process",
        anchor: Some((90, 140)),
        x86: Some(0x4f90_b0),
        link: Link::UniqueAnchor,
    },
];

/// 按 arm64 地址找记录。
pub fn function_at(arm64: u64) -> Option<&'static FunctionEntry> {
    FUNCTIONS.iter().find(|f| f.arm64 == arm64)
}

/// 按 **`.symtab` 原名全等** 找记录（不做前缀/模糊匹配）。
pub fn function_named(name: &str) -> Option<&'static FunctionEntry> {
    FUNCTIONS.iter().find(|f| f.name == name)
}

/// Ghidra / REA 地址（= x86 虚地址 + `0x100000`）。
pub const fn ghidra(x86_vaddr: u64) -> u64 {
    x86_vaddr + 0x10_0000
}

/// `process` 的 x86 虚地址。档位：**观测**（`fixtures/name_to_addr.tsv` 的唯一锚；
/// 20 个 `(file,line,col)` 与 arm64 侧逐一相同，见 [`PROCESS_ANCHORS`]）。
pub const PROCESS_X86: u64 = 0x4f90_b0;

/// `process` 在 x86 侧的跨度**上界**（到下一个 Ghidra 函数起点，**≥ 真实大小**）。
/// 档位：**观测**（`analysis/funcstarts_ghidra_va.pkl`）。
pub const PROCESS_X86_SPAN: u32 = 0x4100;

/// `process` 在 arm64 侧的精确字节数。档位：**观测**（`.symtab`）。
pub const PROCESS_ARM64_SIZE: u32 = 14524;

/// `process` 引用的本文件 `(line, col)` 锚（15 个）。档位：**观测**
/// （`panic::Location` 解 `.data.rel.ro`；x86 侧是同一组）。
pub const PROCESS_ANCHORS: [(u32, u32); 15] = [
    (90, 140),
    (110, 136),
    (113, 124),
    (116, 135),
    (122, 135),
    (123, 56),
    (127, 143),
    (128, 60),
    (132, 135),
    (133, 56),
    (137, 135),
    (138, 56),
    (167, 141),
    (172, 137),
    (174, 62),
];

/// 4 组"成对出现"的锚行 —— 正好对应 4 个模式按钮（每对 = 取节点行 + 它的下一行）。
/// 档位：**观测**（锚的行号成对，且 4 组与 4 个模式按钮一一对应）。
pub const MODE_BUTTON_ANCHOR_PAIRS: [(u32, u32); 4] = [(122, 123), (127, 128), (132, 133), (137, 138)];

/// 3 个"先取节点"的锚行：`JoinButton` / `RoomName` / `PlayerCount`（各出现两次 = 两分支各一次）。
/// 档位：**观测**。
pub const ROW_FIELD_ANCHOR_LINES: [u32; 3] = [110, 113, 116];

/// 本文件行号的**上界**：`process` 锚到 174 行，`init` 在 145 行。档位：**观测**。
///
/// ⇒ `docs/02-module-map.md` §18 A 表里本模块的 `28–135` 是 **2024 版**的数，
/// 2.0.60 的这个文件至少 174 行（`docs/02` 那处需要更新，本文只登记不改表）。
pub const SOURCE_LAST_ANCHOR_LINE: u32 = 174;

// ---------------------------------------------------------------------------
// 3. 行内容：本模块消费的 `GameInfo` 投影
// ---------------------------------------------------------------------------

/// 一行的内容 = 本模块从房间记录里读出来的 7 项。
///
/// ⚠️ 这**不是** `graphwar_game_info::GameInfo` 的复原（那个模块不在本文件范围内）：
/// 字段名取自线上 9 字段（`docs/09-network.md` §1.1.1），本模块实际只读 7 个
/// （4 个模式 + 房名 + 人数 + 一个 bool）。模式在线上是**字符串**
/// （`"EveryUnit"` / `"NormalFunction"` / `"SequentialTurns"` / `"Timer1m"`，实测取值见
/// `docs/09` §1.3.2），这里保留 `&'static str`，因为图标查表用的就是模式值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomEntry {
    /// `room_name`
    pub room_name: String,
    /// `current_player_count`
    pub player_count: u32,
    /// `locked` —— **推断**（唯一能喂给 `set_disabled` 的那个 bool 字段）。
    pub locked: bool,
    /// `axis_mode`（线上字符串）
    pub axis_mode: &'static str,
    /// `function_mode`
    pub function_mode: &'static str,
    /// `turn_mode`
    pub turn_mode: &'static str,
    /// `time_mode`
    pub time_mode: &'static str,
}

impl RoomEntry {
    /// `PlayerCount` 标签的文本：`"  {n}  "`（前后各 [`PLAYER_COUNT_PAD`] 个空格）。
    ///
    /// 档位：**观测**（`0x4517bc` 写 `0x2020` = 两个空格字节；真机 dump `"  0  "`，
    /// `docs/16-client.md` §160）。
    pub fn player_count_text(&self) -> String {
        let pad = " ".repeat(PLAYER_COUNT_PAD);
        format!("{pad}{}{pad}", self.player_count)
    }

    /// `JoinButton` 是否可点 = `!locked`。
    ///
    /// 档位：**观测**（`0x4515a4` 取一个字节 → `0x4515ac` `BaseButton::set_disabled`）
    /// + **推断**（该字节是 `locked`：9 字段里唯一的布尔，`docs/09` §1.1.1）。
    pub fn join_enabled(&self) -> bool {
        !self.locked
    }

    /// 4 个模式的键，顺序 = 4 个按钮的顺序（axis / function / turn / time）。
    pub fn mode_keys(&self) -> [&'static str; 4] {
        [
            self.axis_mode,
            self.function_mode,
            self.turn_mode,
            self.time_mode,
        ]
    }
}

// ---------------------------------------------------------------------------
// 4. 一帧的行同步计划（`process` ④⑤ 两步的纯函数版）
// ---------------------------------------------------------------------------

/// 一个已有行在这一帧要做什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    /// 缓存里没有这个地址 ⇒ `load` 场景 + `instantiate` + `add_child`（只做一次）。
    Create,
    /// 缓存里有 ⇒ **复用**节点，只刷字段（不重建）。
    Refresh,
}

/// 一帧的行计划。顺序即**显示顺序**（`VBoxContainer` 子节点顺序 = `add_child` 顺序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FramePlan {
    /// 源表顺序**原样**保留的地址序列（不排序、不去重、不按人数排）。
    pub order: Vec<SocketAddr>,
    /// 与 [`FramePlan::order`] 一一对应的动作。
    pub actions: Vec<RowAction>,
    /// 源表里已经没有的行 ⇒ `queue_free`（`0x452110` / `0x453340`）。
    pub stale: Vec<SocketAddr>,
}

/// 给定"源表的迭代顺序"与"已有行"，算这一帧的计划。
///
/// 复刻 `process` ④⑤ 两步：
/// * ④ 对源表每个地址 `self.rows.entry(addr)`（`0x451108` / `0x452320`）；
/// * ⑤ 对 `self.rows` 里源表已没有的地址 `queue_free`（`0x452018`–`0x452110`）。
///
/// **不排序**（`process` 里 0 处 `sort*`）：`order` 就是传进来的顺序。
pub fn plan_frame<'a, I>(source_order: I, rows: &HashMap<SocketAddr, u64>) -> FramePlan
where
    I: IntoIterator<Item = &'a SocketAddr>,
{
    let mut order = Vec::new();
    let mut actions = Vec::new();
    for addr in source_order {
        order.push(*addr);
        actions.push(if rows.contains_key(addr) {
            RowAction::Refresh
        } else {
            RowAction::Create
        });
    }
    let mut stale: Vec<SocketAddr> = rows.keys().filter(|a| !order.contains(a)).copied().collect();
    // 只让 `stale` 自己可复现（它是一个"要删的集合"，不参与显示顺序）
    stale.sort_unstable();
    FramePlan {
        order,
        actions,
        stale,
    }
}

// ---------------------------------------------------------------------------
// 5. 选源：Local / Global
// ---------------------------------------------------------------------------

/// 这一帧读哪张表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `show_global == false`：LAN 广播器（模块槽 `container+0x128`，表在 `+0x1a0`）。
    Lan,
    /// `show_global == true`：broker 处理器（模块槽 `container+0x240`，表在 `+0x248`）。
    Broker,
}

impl Source {
    /// `show_global` → 源。档位：**观测**（`0x450ed4` 的分支：非零走 `+0x240`，
    /// 零走 `+0x128`）；"非零 = Global"这层语义是**推断**。
    pub const fn from_show_global(show_global: bool) -> Self {
        if show_global {
            Source::Broker
        } else {
            Source::Lan
        }
    }

    /// 该源的模块槽偏移（相对模块容器）。
    pub const fn slot_offset(self) -> usize {
        match self {
            Source::Lan => CONTAINER_LAN_SLOT,
            Source::Broker => CONTAINER_BROKER_SLOT,
        }
    }

    /// 该源的房间表偏移（相对模块容器）。
    pub const fn table_offset(self) -> usize {
        match self {
            Source::Lan => LAN_TABLE_OFFSET,
            Source::Broker => BROKER_TABLE_OFFSET,
        }
    }
}

// ---------------------------------------------------------------------------
// 6. 模型：不带 Godot 的 `GraphwarLanVBox`
// ---------------------------------------------------------------------------

/// 一行的句柄（Godot 侧是 `Gd<行>`；这里只是一个不透明 id）。
pub type RowId = u64;

/// `GraphwarLanVBox` 的可测模型。
///
/// 只建模**能被证据钉住**的四样东西：Join 通道（`Sender`+`Receiver` 合成一个队列）、
/// 行缓存、`show_global`、可见性闸门。Godot 调用一律不模拟。
#[derive(Debug, Default)]
pub struct LanVBox {
    show_global: bool,
    visible: bool,
    join_queue: VecDeque<SocketAddr>,
    rows: HashMap<SocketAddr, RowId>,
    next_row: RowId,
}

impl LanVBox {
    /// `IVBoxContainer::init` 的初值：`show_global = false`、行表空、通道空。
    ///
    /// 档位：**观测**（`init` 的 `strb wzr,[x20,#0x68]` + 三条 16 B 清零写）。
    /// 可见性不是原版字段（原版每帧问引擎），模型里默认 `true`。
    pub fn new() -> Self {
        Self {
            show_global: DEFAULT_SHOW_GLOBAL,
            visible: true,
            join_queue: VecDeque::new(),
            rows: HashMap::new(),
            next_row: 1,
        }
    }

    /// 当前 `show_global`。
    pub fn show_global(&self) -> bool {
        self.show_global
    }

    /// 切换 Local / Global。
    ///
    /// ⚠️ **谁写这个字段未定**：`process` 只读它，本模块的 [`FUNCTIONS`] 里没有任何写点，
    /// 2.0.60 也没有对应的对外方法证据（[`UNKNOWNS`] 第 3 条）。
    pub fn set_show_global(&mut self, v: bool) {
        self.show_global = v;
    }

    /// 这一帧用哪个源（= [`Source::from_show_global`]）。
    pub fn source(&self) -> Source {
        Source::from_show_global(self.show_global)
    }

    /// 可见性闸门（`process` ②：`is_visible_in_tree()` 为假就整帧不刷）。
    pub fn set_visible(&mut self, v: bool) {
        self.visible = v;
    }

    /// 可见性。
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// 点一下某行的 Join：把地址投进通道（**不**直接连）。
    ///
    /// 档位：**观测**（`0x4491d8` `mpmc::Sender::send`，实参是闭包环境里的 `SocketAddr`）。
    /// 原版按下的一瞬间会先重查两张表（`0x449174`/`0x449188`）——这一层不模拟
    /// （它只影响"房间已消失时点了没反应"，不影响计划）。
    pub fn press_join(&mut self, addr: SocketAddr) {
        self.join_queue.push_back(addr);
    }

    /// `process` ①：排空通道，返回这一帧要连的地址（顺序 = 投递顺序）。
    ///
    /// 档位：**观测**（`0x450c54` / `0x450d98` 的 `try_recv` 循环 + `0x450d68`
    /// `GraphwarBackend::start_client_to_remote_server`）。
    pub fn drain_join_requests(&mut self) -> Vec<SocketAddr> {
        self.join_queue.drain(..).collect()
    }

    /// 通道里还没被取走的请求数（测试用）。
    pub fn pending_join_requests(&self) -> usize {
        self.join_queue.len()
    }

    /// `process` ②–⑤：不可见就返回空计划；否则按源表顺序建 / 复用行，并给出要删的行。
    pub fn sync_rows(&mut self, source_order: &[SocketAddr]) -> FramePlan {
        if !self.visible {
            return FramePlan {
                order: Vec::new(),
                actions: Vec::new(),
                stale: Vec::new(),
            };
        }
        let plan = plan_frame(source_order.iter(), &self.rows);
        for (addr, action) in plan.order.iter().zip(plan.actions.iter()) {
            if *action == RowAction::Create {
                let id = self.next_row;
                self.next_row += 1;
                self.rows.insert(*addr, id);
            }
        }
        for addr in plan.stale.iter() {
            self.rows.remove(addr);
        }
        plan
    }

    /// 当前行数。
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// 某地址的行 id（没有则 `None`）。
    pub fn row_of(&self, addr: &SocketAddr) -> Option<RowId> {
        self.rows.get(addr).copied()
    }
}

// ---------------------------------------------------------------------------
// 7. 未知 / 差异（写文档时照抄）
// ---------------------------------------------------------------------------

/// 未解清单。
pub const UNKNOWNS: [&str; 6] = [
    "`try_recv` 的收尾判据：`process` 用 `cmp w8,#2` + `b.eq` 收工（0x450c60、0x450da0）。\
按 `Result<SocketAddr, TryRecvError>` 的 niche 编码（0=V4、1=V6、2=Empty、3=Disconnected）读，\
`2` 就是 `Empty` ⇒ 排空即收工；那么 `Disconnected`（3）会被当成一条地址丢给 \
`start_client_to_remote_server` —— 一个**永不触发的潜在 bug**（Sender 就在同一个结构体里）。\
这套映射是推断，未做运行时验证。",
    "闭包 `0x449124` 在投递前有一个 `记录+0x18` 的 `u16 == 2` 跳过判据（0x4491a4 `cmp w8,#2`）。\
通道载荷是 `SocketAddr`（`init` 的 drop 符号逐字给出 `Channel<SocketAddr>`），而 `SocketAddr` 的\
判别值只有 0/1 ⇒ 这个 `2` 指什么**未知**（候选：记录里 `+0x18` 不是地址的开头，而是某个三值枚举）。",
    "`show_global`（`self+0x68`）**谁写**：`process` 只读它，本模块没有第二个写点，2.0.60 的\
`.symtab` 里也没有对应的 `#[func]`。`docs/archive/112-meta-managers.md` §2 记的 `0x3ef990`（取 \
`ShowLocalButton`/`ShowGlobalButton`/`GraphwarLanVBox` 节点并设文本/图标）是**最像**的写者，\
但没有逐指令对上这个字节 ⇒ **未知**。",
    "`get_join_address` 在 2024-07-30 版 `.symtab` 里有（`docs/02` §18），2.0.60 arm64 里\
**没有**这个符号。删了还是内联了——未知。",
    "`load(\"res://game_room_entry.tscn\")` / `instantiate` / `add_child` 在 arm64 里各 2 次\
（每分支 1 次）。从地址布局看创建块在逐行循环头之前，语言级结构（`entry().or_insert_with(..)` \
还是「循环里发现缺就建」）**未定**。不影响可测判据：新建一行必须 load+instantiate+add_child 一次，\
复用行不重建。",
    "Join 处理器那个 `Vec<Box<dyn FnMut(&str)>>` 落在**哪个结构体**的 `+0x38`：push 代码\
（0x4513fc–0x451410）作用的对象也是 `bind`/`bind_mut` 的目标，而同一个对象还带 `+0x1668`\
（模块容器）⇒ 它更像一个**大对象**（`GraphwarRootNode`）而不是行里的 JoinButton。未定。",
];

/// 与 `client/lan_lobby.gd` 的出入条数（详见模块头那张表）。**只登记，不改 `client/`**。
pub const CLIENT_DIFF_COUNT: usize = 6;

/// 大厅顺序**不是**信息：原版不排序，顺序 = `HashMap` 的迭代顺序。
/// 档位：**观测**（0 处 `sort*`）+ **推断**（跨进程会变）。
pub const ORDER_IS_HASH_ORDER: bool = true;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn addr(last: u8) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, last), 35245))
    }

    fn entry(name: &str, players: u32, locked: bool) -> RoomEntry {
        RoomEntry {
            room_name: name.to_string(),
            player_count: players,
            locked,
            axis_mode: "EveryUnit",
            function_mode: "NormalFunction",
            turn_mode: "SequentialTurns",
            time_mode: "Timer1m",
        }
    }

    #[test]
    fn thirteen_functions_and_ambiguous_ones_stay_unnamed() {
        assert_eq!(FUNCTIONS.len(), 13);
        let unique: Vec<&str> = FUNCTIONS
            .iter()
            .filter(|f| f.link == Link::UniqueAnchor)
            .map(|f| f.name)
            .collect();
        assert_eq!(unique.len(), 3, "{unique:?}");
        assert!(unique.iter().any(|n| n.contains("process")));
        let p = function_named("<GraphwarLanVBox as IVBoxContainer>::process").unwrap();
        assert_eq!(p.x86, Some(PROCESS_X86));
        assert_eq!(ghidra(PROCESS_X86), 0x5f90_b0);
        // 歧义项一个都不许被写成确定值
        for f in FUNCTIONS.iter().filter(|f| f.link == Link::Ambiguous) {
            assert_eq!(f.x86, None, "{} 不许挑候选", f.name);
        }
        for f in FUNCTIONS.iter().filter(|f| f.link == Link::Unknown) {
            assert_eq!(f.x86, None);
            assert_eq!(f.anchor, None);
        }
        // 文档里登记的 11 个 x86 地址，只有 process 被认领
        assert_eq!(
            FUNCTIONS.iter().filter(|f| f.x86.is_some()).count(),
            3,
            "只有 2 个唯一锚 + process 有 x86 地址"
        );
    }

    #[test]
    fn process_size_is_an_upper_bound_not_a_fingerprint() {
        assert_eq!(function_at(0x450c_20).unwrap().size, PROCESS_ARM64_SIZE);
        assert!(PROCESS_X86_SPAN >= PROCESS_ARM64_SIZE, "跨度是上界");
        // docs/02 §19：这一版函数被整体重排，大小比只能当上界，不能当判据
        let ratio = f64::from(PROCESS_X86_SPAN) / f64::from(PROCESS_ARM64_SIZE);
        assert!(ratio > 1.0 && ratio < 1.2, "ratio={ratio}");
    }

    #[test]
    fn anchors_match_the_four_mode_button_shape() {
        assert_eq!(PROCESS_ANCHORS.len(), 15);
        for (a, b) in MODE_BUTTON_ANCHOR_PAIRS.iter() {
            assert!(
                PROCESS_ANCHORS.contains(&(*a, 135)) || PROCESS_ANCHORS.contains(&(*a, 143)),
                "{a}"
            );
            assert!(PROCESS_ANCHORS.iter().any(|(l, _)| l == b));
            assert_eq!(*b, a + 1, "每个模式按钮占两行");
        }
        for line in ROW_FIELD_ANCHOR_LINES.iter() {
            assert!(PROCESS_ANCHORS.iter().any(|(l, _)| l == line), "{line}");
        }
        assert!(PROCESS_ANCHORS
            .iter()
            .all(|(l, _)| *l <= SOURCE_LAST_ANCHOR_LINE));
        // docs/02 §18 A 表的 28–135 是 2024 版的数；2.0.60 至少到 174 行
        assert!(SOURCE_LAST_ANCHOR_LINE > 135);
    }

    #[test]
    fn instance_layout_is_the_seventy_byte_shape() {
        assert_eq!(SENDER_OFFSET, 0x00);
        assert_eq!(RECEIVER_OFFSET, 0x10);
        assert_eq!(BASE_OFFSET, 0x20);
        assert_eq!(ROWS_OFFSET, 0x38);
        assert_eq!(SHOW_GLOBAL_OFFSET, 0x68);
        assert_eq!(INSTANCE_SIZE, 0x70);
        assert!(SENDER_OFFSET + 16 <= RECEIVER_OFFSET);
        assert!(RECEIVER_OFFSET + 16 <= BASE_OFFSET);
        assert!(BASE_OFFSET + 24 <= ROWS_OFFSET);
        assert!(ROWS_OFFSET + 48 <= SHOW_GLOBAL_OFFSET);
        assert!(SHOW_GLOBAL_OFFSET + 1 <= INSTANCE_SIZE);
        assert_eq!(DEFAULT_SHOW_GLOBAL, false, "默认 Local 页");
        assert_eq!(ROW_BUCKET_BYTES, 32 + 16, "SocketAddr + Gd");
    }

    #[test]
    fn source_selection_is_the_show_global_flag() {
        assert_eq!(Source::from_show_global(false), Source::Lan);
        assert_eq!(Source::from_show_global(true), Source::Broker);
        assert_eq!(Source::Lan.slot_offset(), 0x128);
        assert_eq!(Source::Broker.slot_offset(), 0x240);
        assert_eq!(Source::Lan.table_offset(), 0x1a0);
        assert_eq!(Source::Broker.table_offset(), 0x248);
        assert!(Source::Lan.table_offset() > Source::Lan.slot_offset());
        assert!(Source::Broker.table_offset() > Source::Broker.slot_offset());
        assert_ne!(Source::Lan.slot_offset(), Source::Broker.slot_offset());
        assert_eq!(LAN_BUCKET_BYTES, 120, "docs/144 §1.1 的 0x78");
        assert_eq!(ROOT_MODULE_CONTAINER_OFFSET, 0x1668);
        assert_eq!(ROOT_TEXTURE_LOADER_OFFSET, 0x16a0);
    }

    #[test]
    fn player_count_text_is_padded_with_two_spaces() {
        let e = entry("US Room 1", 0, false);
        assert_eq!(e.player_count_text(), "  0  ");
        assert_eq!(e.player_count_text().len(), 1 + 2 * PLAYER_COUNT_PAD);
        assert_eq!(entry("x", 12, false).player_count_text(), "  12  ");
        assert_eq!(e.player_count_text().trim(), "0");
        assert_eq!(PLAYER_COUNT_PAD, 2);
    }

    #[test]
    fn join_is_disabled_exactly_for_locked_rooms() {
        assert!(entry("a", 1, false).join_enabled());
        assert!(!entry("b", 1, true).join_enabled());
        assert_eq!(CONSUMED_FIELDS, 7);
        let e = entry("mode", 3, false);
        assert_eq!(
            e.mode_keys(),
            ["EveryUnit", "NormalFunction", "SequentialTurns", "Timer1m"]
        );
    }

    #[test]
    fn frame_plan_keeps_source_order_and_never_sorts() {
        let existing: HashMap<SocketAddr, u64> =
            [(addr(3), 1u64), (addr(1), 2u64)].into_iter().collect();
        // 故意传一个"倒"的源顺序：计划必须原样保留
        let source = vec![addr(9), addr(3), addr(1)];
        let plan = plan_frame(source.iter(), &existing);
        assert_eq!(plan.order, source, "顺序原样保留（无排序）");
        assert_eq!(
            plan.actions,
            vec![RowAction::Create, RowAction::Refresh, RowAction::Refresh]
        );
        assert!(plan.stale.is_empty());

        // 源表少一条 ⇒ 那一条进 stale，其余顺序不变
        let plan = plan_frame(vec![addr(1)].iter(), &existing);
        assert_eq!(plan.order, vec![addr(1)]);
        assert_eq!(plan.stale, vec![addr(3)]);
        assert!(ORDER_IS_HASH_ORDER);
    }

    #[test]
    fn model_creates_reuses_and_frees_rows() {
        let mut vb = LanVBox::new();
        assert_eq!(vb.row_count(), 0);
        assert_eq!(vb.show_global(), false);
        assert_eq!(vb.source(), Source::Lan);

        let a = addr(1);
        let b = addr(2);
        let plan = vb.sync_rows(&[a, b]);
        assert_eq!(plan.actions, vec![RowAction::Create, RowAction::Create]);
        assert_eq!(vb.row_count(), 2);

        // 同一帧再来一次：行被复用，id 不变
        let first_id = vb.row_of(&a).unwrap();
        let plan = vb.sync_rows(&[a, b]);
        assert_eq!(plan.actions, vec![RowAction::Refresh, RowAction::Refresh]);
        assert_eq!(vb.row_of(&a), Some(first_id), "复用同一个行节点");

        // b 从源表消失 ⇒ 被清掉，a 留着
        let plan = vb.sync_rows(&[a]);
        assert_eq!(plan.stale, vec![b]);
        assert_eq!(vb.row_count(), 1);
        assert_eq!(vb.row_of(&b), None);

        // Global 页换源（同一个行缓存；原版也是同一个 HashMap）
        vb.set_show_global(true);
        assert_eq!(vb.source(), Source::Broker);
        assert_eq!(vb.sync_rows(&[a]).actions, vec![RowAction::Refresh]);
    }

    #[test]
    fn invisible_vbox_does_nothing() {
        let mut vb = LanVBox::new();
        vb.set_visible(false);
        assert_eq!(vb.visible(), false);
        let plan = vb.sync_rows(&[addr(1), addr(2)]);
        assert!(plan.order.is_empty());
        assert!(plan.stale.is_empty());
        assert_eq!(vb.row_count(), 0, "不可见 ⇒ 一行都不建（0x450ddc 的 tbz）");
        vb.set_visible(true);
        assert_eq!(vb.sync_rows(&[addr(1)]).actions, vec![RowAction::Create]);
        assert_eq!(vb.row_count(), 1);
    }

    #[test]
    fn join_goes_through_the_channel_and_lands_next_frame() {
        let mut vb = LanVBox::new();
        let a = addr(7);
        vb.press_join(a);
        // 按下不等于立刻连：请求还在通道里
        assert_eq!(vb.pending_join_requests(), 1);
        assert_eq!(vb.row_count(), 0);
        // 下一帧 ① 步才排空
        assert_eq!(vb.drain_join_requests(), vec![a]);
        assert_eq!(vb.pending_join_requests(), 0);
        assert!(vb.drain_join_requests().is_empty());
        // 连点两次就是两条（通道不去重 —— 原版也没有）
        vb.press_join(a);
        vb.press_join(a);
        assert_eq!(vb.drain_join_requests(), vec![a, a]);
    }

    #[test]
    fn literals_and_names_are_the_observed_bytes() {
        assert_eq!(CLASS, "GraphwarLanVBox");
        assert_eq!(BASE_CLASS, "VBoxContainer");
        assert_eq!(SOURCE_FILE, "src/graphwar_lan_vbox.rs");
        assert_eq!(ROW_SCENE, "res://game_room_entry.tscn");
        assert_eq!(ROW_SCENE.len() as u32, ROW_SCENE_LEN);
        assert_eq!(ROOT_NODE_NAME, "GraphwarRootNode");
        assert_eq!(ROOT_NODE_NAME.len(), 16, "指令里的长度 0x10");
        for (name, len) in ROW_NODE_NAMES.iter() {
            assert_eq!(name.len() as u32, *len, "{name} 的长度和指令立即数不符");
        }
        assert_eq!(ROW_NODE_NAMES.len(), CONSUMED_FIELDS);
        let mut names: Vec<&str> = ROW_NODE_NAMES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 7);
        // x86 侧的字面量引用必须都落在 process 里（独立于"函数大小"的第二条证据）
        for a in ROW_SCENE_X86_REFS.iter() {
            assert!(
                *a >= PROCESS_X86 && *a < PROCESS_X86 + u64::from(PROCESS_X86_SPAN),
                "{a:#x} 不在 process 里"
            );
        }
        for a in JOIN_BUTTON_X86_REFS.iter() {
            assert!(
                *a >= PROCESS_X86 && *a < PROCESS_X86 + u64::from(PROCESS_X86_SPAN),
                "{a:#x} 不在 process 里"
            );
        }
        // 两条分支各一条 load，所以正好 2 条
        assert_eq!(ROW_SCENE_X86_REFS.len(), 2);
        // 每分支 JoinButton 两处（挂处理器 + 设图标/禁用）
        assert_eq!(JOIN_BUTTON_X86_REFS.len(), 4);
    }

    #[test]
    fn unknowns_are_registered_not_guessed() {
        assert_eq!(UNKNOWNS.len(), 6);
        assert_eq!(CLIENT_DIFF_COUNT, 6);
        for u in UNKNOWNS.iter() {
            assert!(!u.is_empty());
        }
        assert!(UNKNOWNS.iter().any(|u| u.contains("未知")));
        assert!(UNKNOWNS.iter().any(|u| u.contains("未定")));
    }
}
