# 161 · 停在 `assembled` 的 9 个 manager：升档要什么证据（2026-11-09，第十三轮之后）

> **一句话**：档位分布现在是 **14 `wired` / 2 `layout` / 6 `assembled` / 0 `stub`**
> （本文定稿时是 11 / 2 / 9 / 0，任务 74 把 `Rank` / `Name` / `LocalPlayer` 三格升到 `wired`，
> 见 §2 的 ✅ 行与 [`16-client.md`](16-client.md) §54）。本轮（161）把 9 个里的 **8 个**的
> **构造点 + 订阅 + 发布方向**全部钉到指令，其中 **2 个 `new` 的 x86 地址是当时才定下来的**
> （`GraphwarNameManager` / `FunctionHistoryRecorder`）。
> 结论：下一轮的活**大部分是工程（接线层 + 交互断言），不是再挖证据**——任务 74 就是照这个做的
> （三格全部升档成功；顺带把 G1 结案、把 `+0x20` 判成自句柄、把 5 条请求的门读成 5 道）。
> 真正还缺证据的只剩 **5 条**（§4，G1 已结案）。
>
> 本文只写"要什么、去哪拿、能不能本机做完"，**不改任何代码**。证据分级沿用
> [`06-verification.md`](06-verification.md) §1 的四档（逐位 / 观测 / 推断 / 未知）。
> 地址一律 **ELF 虚地址**；Ghidra/REA 地址 = 本值 **+ `0x100000`**（只加一次）。

---

## 0. 复核：档位分布，以及"7"是怎么来的

### 0.1 三个互相独立的口径，全部给出 **9**

| 口径 | 位置 | 结果 |
|---|---|---|
| 判据函数本体 | `src/_scaffold/root_wiring.rs` 的 `Wired::status()`（`match` 臂） | `wired` 臂 **14** 个 / `layout` 臂 **2** 个 / 其余落 `_ =>` = **6** |
| 四个计数常量 + 编译期闸 | 同文件 `WIRED_TIER_COUNT` / `LAYOUT_TIER_COUNT` / `ASSEMBLED_TIER_COUNT` / `STUB_TIER_COUNT` | **14 / 2 / 6 / 0**；`const _: () = assert!(四者之和 == Wired::COUNT)`（= 22）**在编译期就红**，改一槽漏改这里编不过 |
| 运行时断言表 | `client/tools/assemble_check.gd` 的 `WIRED_TIER[22]` + `TIER_TALLY` | 逐槽字符串 / 计数，与上二者一致（**14 / 2 / 6 / 0**） |

复核命令（5 秒，不需要编译）：

```bash
# ① 判据函数的臂数
sed -n '/pub const fn status(self)/,/^    }/p' src/_scaffold/root_wiring.rs
# ② 四个计数
grep -n "_TIER_COUNT: usize =" src/_scaffold/root_wiring.rs
# ③ 逐槽表
sed -n '/const WIRED_TIER :=/,/^]/p' client/tools/assemble_check.gd | grep -c '"wired"\|"layout"\|"assembled"\|"stub"'
```

### 0.2 `assembled` 的清单：原来是 9 个，任务 74 之后剩 6 个

**原 9 个**（按原版注册序，编号同 `SLOTS` 的下标 + 1）：
`3 GraphwarLocalPlayerManager` / `5 GraphwarSoldierManager` / `7 GraphwarGameStateManager` /
`16 GraphwarGraphicsManager` / `18 GraphwarComputerPlayerManager` / `19 FunctionHistoryRecorder` /
`20 GraphwarNameManager` / `21 GraphwarColorManager` / `22 GraphwarRankManager`。

**任务 74 升掉的 3 个**（→ `wired`）：`3 LocalPlayer` / `20 Name` / `22 Rank`（§2 的 ✅ 行）。
**现在剩的 6 个**：`5 Soldier` / `7 GameState`（⛔ 禁区）/ `16 Graphics` /
`18 ComputerPlayer` / `19 FunctionHistoryRecorder` / `21 Color`。

### 0.3 ⚠️ `docs/00` 那条"7"是**陈旧数据**（本轮登记，不改）

[`00-status.md`](00-status.md) §0.1b.2 的倒数第二行写：

```text
| ~~4 个 manager 仍是 `assembled`~~（**已由协调方就地更正为 9**） | 只构造注册、没接事件流（**11 wired / 2 layout / 9 assembled / 0 stub = 22**） | 缺工程 | ✅ 可做 |

> ⚠️ 上面这段是 `docs/00` 的**历史引文**。任务 74 之后现值是 **14 / 2 / 6 / 0**
> （见 §9），`docs/00-status.md` §0.1b.2 与 `docs/HANDOVER.md` §遗留 那两处仍写着旧的
> 9 格 / 11-2-9-0 —— 这两个文件**不在任务 74 的允许改动清单里**，只能登记（§7 C7）。
```

**同一行里两个数还互相矛盾**（4 vs 7），而且 **11 + 2 + 7 = 20 ≠ 22** —— 三个独立口径都不支持。
正确的现值是 **9**。该文件不在本轮允许改动的清单里，所以**只在本文登记**；下一轮谁动 `docs/00`
顺手把它改掉（同时改那半句"4 个"）。

---

## 1. 判据：什么才算"这一槽有证据"

### 1.1 升到 `wired` 要三件，缺一件都不算

判据原文在 `root_wiring.rs` 的模块文档"槽位复原档位"一节（**按序判断，第一个命中者为准**）：
**① 接容器 + ② 事件流两个方向都指到指令 + ③ 有最小交互断言**（`interaction_selftest` 覆盖这一槽）。
`assembled` 的定义是"①②③ 都不满足，但容器里装的是我方复原类型"。

### 1.2 三条机械判据（本轮就是用它们把 8 个钉死的）

| 方向 | 判据（三条同时成立） | 全仓唯一性 |
|---|---|---|
| **订阅** | `lea rsi,'<事件名>'` → `mov edx,<len>` → `call [slot 0x9bac88]` | 槽 `0x9bac88` 的重定位 addend = **`0x3d38f0`**（`GraphwarEventHandler::subscribe_to_event`），**全仓只此一槽** |
| **处理器** | `lea r8,<闭包槽>` → 槽 **`+0x20`** 的 addend = 跳板 → 跳板里 `lea rsi,[rip+X]`（或 `mov rsi,[slot]`）= 真处理器 | — |
| **发布** | `call [slot 0x9bac80]` + 事件 tag word `0x8000_0000_0000_00NN` | 槽 `0x9bac80` 的 addend = **`0x3d3530`**（`GraphwarEventQueue::add_event`），**全仓只此一槽** |

tag word 的两种构造都出现过，读的时候别只找 `movabs` 立即数：

```text
① movabs rbx, 0x8000000000000032 ; lea rax,[rbx+9]        → 0x…3B = 59
② movabs rbp, 0x7fffffffffffffff ; lea rax,[rbp+0x3c]     → 0x…3B = 59   ← isize::MAX 当基址，lea 回绕
   …                            ; add rbp, 0x43           → 0x…42 = 66
```

### 1.3 全量数字（本轮实测，可复算）

| 项 | 数 | 说明 |
|---|---|---|
| `subscribe_to_event` 调用点 | **66** | 落在 **24** 个函数里（22 个 manager 的 `new` 占 21 个 + `game_server_thread` + `graphwar_game_recorder::subscribe_to_events` + `GraphwarBackend::subscribe_to_event`） |
| `add_event` 调用点 | **222** | 落在 **113** 个函数里 |
| 订阅名去重后 | **20** 种 | `entity_removed_event` 16 次、`new_connection_event` 15 次、`tick_event` 7 次…… |

> `graphwar_game_recorder` 走**另一个注册口**（`0x3d4fa0`，不经 `0x9bac88`），所以不在 66 里；
> 这与 `docs/02` §22.3 的"56 B / 独立注册"一致。

---

## 2. 九行升档清单（主表）

「已知」列里的 `new` = **原版构造函数的 x86 地址**（本轮新钉的用 **粗体**）；
「构造点」= `GraphwarGameClient::new`（x86 `0x517690`）里调它的那条 `call`。

| # | manager（原版型 / size / 我方型） | 已知什么 | **缺什么证据** | 拿证据的手段 |
|---|---|---|---|---|
| 3 | `GraphwarLocalPlayerManager` / 64 / `graphwar_local_player_manager::GraphwarLocalPlayerManager` | `new` **内联**进 `GraphwarGameClient::new`（arm64 `0x409eac` 处 `alloc 0x58`，无独立符号）；**订阅 3 条**（全在 `0x517690` 体内）：`connected_to_server_event`(7) `@0x5179d4` → `0x51a570`、`player_info_event`(13) `@0x517a27` → `0x519f40`、`entity_removed_event`(14) `@0x517a77` → `0x51a610`；**发 6 条**：`handle_player_info`(`0x519f40`) 里 5 处 `add_event` = **59**`@0x51a170` / **50**`@0x51a247` / **52**`@0x51a31e` / **54**`@0x51a3f5` / **66**`@0x51a451`，`handle_connected_to_server`(`0x51a570`) 里 1 处 = **7**`@0x51a5bb`；档案四字段 `+0x18/+0x30/+0x48/+0x60`（模块文档 §3） | ✅ **任务 74 已升 `wired`**（§2.1）：① `+0x20` = `Weak<RefCell<Self>>` 自句柄（写点 `0x517965`，值 = 盒基）② 5 条请求内部**没有**逐条判据，门是**五道**（`0x519f68` connected / `0x519f71` conn 相等 / `0x519f9d` 旧 len==0 / **`0x519fb1`→`0x51a058` 新发现的一张表**）③ `outbox`/`subscribed` 已落码。**仍缺**：第五道门那张表（key = `player_id`、entry 120 B）的语义（§4 G7） | ① `new` 的 `alloc 0x58` 之后 24 条写指令逐条读（arm64 `0x409eac`–`0x409f60`），判 `+0x20` 是 `Option`/`Weak`/标志 ② `0x519f40` 的 5 段：每段 `add_event` 前的 `cmp`/`jne` 就是门（`addr_probe.py dis 0x519f40`，5 段各 40–60 行）③ 这一条不用新证据，照 `docs/archive/156-client-rounds.md` §50.1 的写法加 `subscribed`/`outbox` |
| 5 | `GraphwarSoldierManager` / 72 / `graphwar_soldier_manager::SoldierTable` | `new` = `0x455760`（call 点 `0x517af8`，arm64 `0x49d590`）；**订阅 2 条**：`new_connection_event`(5) `@0x4557e5` → `0x455570` `send_all_info_events`、`entity_removed_event`(14) `@0x455836` → `0x4556d0`；**发 1 条** `SoldierInfo`(15) `@0x455616`；模块文档 §6 已列 5 条未知 | ① `add_soldier` 的 x86 地址**没锚到**（`name_to_addr.tsv` 两个候选 `0x3e2830` / `0x474a10` 都被证否）⇒ 它**发不发 `SoldierInfo`(15)** 未知（模块 §6-B）② 表里**键值极性**只有间接证据（§6-D）③ 72 B 逐字段 | ① 用 arm64 侧**已知的 144 B 体**（`0x49d2fc`）做 x86 指纹匹配：`tools/func_diff.py` / `tools/sym_match.py`（arm64 有符号、x86 无符号，这是现成路子）② 极性：读 `send_all_info_events` `0x455570` 的载荷装配（哪个寄存器进 `+0x00` / `+0x08`）+ `handle_entity_removed_event` `0x4556d0` 拿 `event+8` 当 key 的那条 `cmp` ③ 同上判据 §1.2 的"写入点 + 偏移"清单 |
| 7 | `GraphwarGameStateManager` / 296 / `game::Match` | `new` = `0x477bc0`（**虚调**：经槽 `0x9bbce8`，站点 `0x517b65`；arm64 `0x4ade6c`）；**订阅 7 条**，全部**逐字节复核**且与 `156` §50.0 那 7 个名字**完全一致**：`tick`(0)`@0x477c92`→`0x470840`、`new_connection`(5)`@0x477ce8`→`0x477140`、`entity_removed`(14)`@0x477d3e`→`0x476a20`、`connection_removed`(6)`@0x477d94`→`0x477540`、`function_points`(37)`@0x477dea`→`0x4758b0`、`skin_graphics_info`(51)`@0x477e40`→`0x4741a0`、`chat_message`(71)`@0x477e96`→`0x473d00` | **不是证据缺，是禁区块**：容器里装的是 `src/game.rs` 的 `Match`（结算内核，`fire`/`batch` 三条驱动轴都在用它）⇒ 接事件层**必须改 `src/game.rs`**；`grep -n "outbox\|subscribed" src/game.rs` 零命中。另：296 B 布局**没有**逐字段（`Match` 不是布局复原，是行为复原） | 两条路都已被 `156` §50.4 否定（加壳会改容器类型、另起旁表=把猜测写成行为）。**唯一的正路**：等 `src/game.rs` 解禁后按 §50.1 的六个 manager 同款写法加 `SUBSCRIBED_EVENTS[7]` + `outbox` + 7 个 `handle_*` 入口。**在此之前派活只会撞墙** |
| 16 | `GraphwarGraphicsManager` / 168 / `graphwar_graphics_manager::GraphwarGraphicsManager` | `new` = `0x469390`（call 点 `0x517d73`，arm64 `0x49e394`）；**订阅 2 条**：`new_connection_event`(5)`@0x469415`→`0x468d80`、`entity_removed_event`(14)`@0x469466`→`0x469220`；**发 4 处**：`set_hat_graphics`(`0x468a50`)→**55**`@0x468abe`、`set_face_graphics`(`0x468b60`)→**53**`@0x468bce`、`set_skin_graphics`(`0x468c70`)→**51**`@0x468cde`、`send_all_info_events`(`0x468d80`) 三处重播 `@0x468eac`/`@0x468fe6`/`@0x469122`；三张表在 `+0x18/+0x48/+0x78`（hasher 各自 `+0x20`，模块文档 §2）；`outbox`/`take_outbox` **代码里已有** | ① 168 B 里**三张表以外**还有 4 个字（`0x18×3 = 0x78`，到 `0xa8` 之间有 `0x30` 字节）没解 ② `send_all_info_events` 的**第三处**重播 tag 是**间接载入**（`mov rax,[rsp+0x10]`）⇒ 我方扫描只读到两处字面量 51/53，第三处按模块文档是 **55**，**未逐位复核** ③ 贴图来源不在本模块（`GraphwarTextureLoader`，模块 §3 的有界否定） | ① `new` 的 `alloc` 之后全部写指令（`0x469390`–`0x4694e9`）+ 三个 getter（`+0x150/+0x180/+0x1b0` 一族）逐条读 ② `0x469122` 往前 20 条，跟 `[rsp+0x10]` 的来源（三表循环的公共 tag） ③ 贴图侧去找 `GraphwarTextureLoader` 的 `new`（不是本槽的事，别混） |
| 18 | `GraphwarComputerPlayerManager` / 72 / `graphwar_computer_player_manager::ComputerPlayers` | `new` = `0x47a720`（call 点 `0x517deb`，arm64 `0x4b02a4`）；**订阅 3 条**：`tick_event`(0)`@0x47a7a5`→`0x4785f0` `handle_tick`、`turn_info_event`(43)`@0x47a7fa`→`0x479520`、`entity_removed_event`(14)`@0x47a84b`→`0x47a520`；**发 5 条**（全在 `add_computer_player` `0x478fc0`）：**59**`@0x4791f9`、50-base`@0x479271`（tag 待读）、**52**`@0x4792ec`、**54**`@0x479366`、**66**`@0x4793a6`；难度六档阈值 `4/11/26/101/251`（模块 §3，与 [`13-ai.md`](13-ai.md) §1 互证） | ① `handle_tick` 里 `state != 3` 走的那条**不返回**路径（`0x5ec160`，Ghidra 标 `hasNoReturn`）是什么（模块 §7-A）② `state` 的 `0/1/2` 语义（§7-B）③ `computer_function_thread` 的 x86 地址靠夹逼（§7-C）④ `BotData` 那个 120 字节**大半未解**（模块文档字段表末行）⑤ `0x479271` 那处的 tag | ① `addr_probe.py dis 0x4785f0`，追 `0x5ec160` 的调用点前后（它是 `noreturn`，所以"不返回"其实是 `panic!`/`abort`，判据是它前面那条 `lea rdi,<&str>` 的串）② 全 `.text` 搜 `BotData` 的 state 字（`+0x00`）**写入点**：立即数 0..3 的 `mov dword ptr [reg+disp], N`，且 `reg` 来自 `0x478fc0` 的插入路径 ③ 与 ① 同一条线索（线程体就在 `add_computer_player` 的 `spawn` 参数里）④ `new`/`add_computer_player` 里对 `+0x18` 起的 120 字节逐字段 |
| 19 | `FunctionHistoryRecorder` / 48 / `function_history_recorder::History` | **`new` = `0x3f10a0`（本轮新钉；`name_to_addr.tsv` 原记"未知"）**，call 点 `0x517e27`，arm64 `0x49d9a0`；**订阅 1 条**：名字逐字节 `function_info_event`（`0x1bfccd`，len `0x13`）`@0x3f115a` → 处理器 `0x3f0e40`，其体内**判别值闸 = 35**（`lea rax,[isize::MAX+0x24]`）；**发 0 条**（`0x3f0e40` 体内无 `add_event`）；容量/去重两条已有硬结论（模块文档，观测） | ① **口径冲突（必须先裁）**：本模块把 tag **35** 叫 `function_fire_event`（`EVENT_FIRE_TAG = 0x8000…0023`），而 [`07-protocol.md`](07-protocol.md) §7.2 的 72 条表里 **33 = `function_fire_event`、35 = `function_info_event`**；本轮字节证据（订阅名字 + 判别值闸）**两边都指向 35** ② `Vec<FunctionData>` 的**消费点**（上下键）全库无落点（有界否定，模块文档 §"上下键"）③ 48 B 逐字段 | ① 裁定判据：全 `.text` 找**构造 33** 的地方（`movabs 0x8000…><0021` 或 base+`0x21`）；若 33 只在**解码器**里出现、且没有任何订阅者，则"35 才是 recorder 订阅的那条"成立，我方改名 ② 消费点：真机按一次 `ui_up` 抓 `LineEdit.text`（`docs/06` §13 的 `tools/frida_live.js` 可用），或者接受"只写不读"并登记 ③ 同上逐字段 |
| 20 | `GraphwarNameManager` / 72 / `graphwar_name_manager::GraphwarNameManager` | **`new` = `0x519100`（本轮新钉；`name_to_addr.tsv` 给的 4 个候选 `0x3b6b20`/`0x3b6c70`/`0x420b40`/`0x4500c0` 全错——`0x4500c0` 是 `game_server_thread`）**，call 点 `0x517e63`，arm64 `0x40af68`；**订阅 2 条**：`new_connection_event`(5)`@0x519209`→`0x518f10`、`entity_removed_event`(14)`@0x519255`→`0x519030`；**发 2 条** `NameInfo`(60)：`@0x518fe5`（整表重播）、`@0x5193e8`（`set_name` `0x519350`）；表 `+0x18`、hasher `+0x38`、entry `0x20`（模块文档 §2） | ✅ **任务 74 已升 `wired`**（§2.1）：① 结案——盒是 `alloc 0x60`（RcInner 16 + RefCell 80），`M` 基 = `盒+0x18`；`+0x38` 起那 **16** 字节是 `RandomState`（两个 `u64`，写点 `0x5191a5`/`0x5191a9`），**没有空洞**，72 B 全部有写入点 ② `outbox`/`subscribed` 已落码。**无遗留** | ① 先定基址：arm64 `0x40af68` 的 `alloc` 尺寸（`RefCell<M>` = 80 = 72+8 ⇒ **`0x50`**，不是 `0x58`；`0x58` 是 `RcInner`），再把写入点减去 `0x18` 对齐 ② 断言照 `156` §50.5 的写法：`set_name(9010,"x")` → 读回表 → `take_outbox()` 应恰好 1 条 (60) |
| 21 | `GraphwarColorManager` / 120 / `graphwar_color_manager::GraphwarColorManager` | `new` = `0x465620`（call 点 `0x517e9f`，arm64 `0x49cbe8`）；**订阅 2 条**：`new_connection_event`(5)`@0x4656a5`→`0x465460`、`entity_removed_event`(14)`@0x4656f6`→`0x465570`；**发 2 条** `ColorInfo`(61)：`@0x464fb6`（`add_team_color` `0x464d10`）、`@0x465517`（整表重播）；两张 6 色池字面量**逐位**（模块 §2）、分配规则"队号选池 + 随机起点 + 线性探测"（§3）、载荷 8 字节 `{id@+0, r@+4, g@+5, b@+6}`（§5） | ① `new` 的第三个字段 `self+0x10` **没被本模块任何函数读过**（模块 §6 第一条）② 120 B 逐字段 ③ 随机流**消耗顺序**：起点 1 次 `u64` + 兜底 3 次 `u8` ⇒ 复现第 n 个玩家必须真 `rand_chacha` 流（模块 §6 第三条，标推断） | ① `+0x10` 只在 `new` 里写、读侧零命中 ⇒ 判"留给别人的借用句柄 or 缓存"，定案要**读 `new` 的初始化闭包**（`Rc::new_cyclic_in`，`0x465620` 里那段）② 逐字段同上 ③ 随机流：抓一次 `request_new_player` 前后的 ChaCha 状态（`tools/capture_rand_values.js` 已有；见 `docs/06` §13 的 frida 活体） |
| 22 | `GraphwarRankManager` / 72 / `graphwar_rank_manager::GraphwarRankManager` | `new` = `0x454f30`（call 点 `0x517edb`，arm64 `0x498d2c`）；**订阅 2 条**：`new_connection_event`(5)`@0x454fb5`→`0x454d80`、`entity_removed_event`(14)`@0x455006`→`0x454e80`；**发 2 条** `RankInfo`(67)：`@0x454e35`（整表重播）、`@0x45511f`（`set_rank` `0x455090`）；表基 `+0x18` / hasher `+0x38` / 载荷 `{+0x08 flag, +0x0c rank, +0x10 player}`（模块 §3，并就地更正了 `docs/112` §5.3 的"两张 map"误读） | ✅ **任务 74 已升 `wired`**（§2.1）：① `outbox`/`subscribed` 已落码；x86 两条订阅 + 两处 `add_event` 逐条复核（`0x454fb5`/`0x455006`、`0x454e35`/`0x45511f`）② 72 B 里 `+0x00` = 自句柄（`0x454f62` 读它当 `Weak`）、`+0x18` 表、`+0x38` hasher 16 B。**仍缺**：`+0x08` 那个标志**是否进序列化**（§4 G3，**不阻塞** `wired`） | ① 判据：`serde` 的 `RankInfo` 字段数（`docs/07` §3.3 记 `rank_info` 的线上体 = `entity_id` + `rank`，wire 只有 2 个）＋ `set_rank` 里那三处 `mov` 的**顺序**；定案不需要新证据，读 `0x455090`–`0x45511f` 逐条即可 ② 断言：`set_rank(9011, Some(3))` → 读回 → outbox 1 条 (67) |

### 2.1 任务 74 完成的三格（地址级：补了什么 / 还剩什么）

| # | manager | 订阅（x86 调用点 → 跳板 → 处理器） | 发布（x86 `add_event` 点） | 交互读数 | 剩余 |
|---|---|---|---|---|---|
| 22 | `RankManager` | `new_connection_event` `0x454fb5` → `0x454f10` → `0x454d80`；`entity_removed_event` `0x455006` → `0x454ef0` → `0x454e80` | **67** `@0x454e35`（整表重播，`flag` 恒 1）/ `@0x45511f`（`set_rank`，两路共用） | **9** 条 `rank.*` | `+0x08` 序列化口径（G3） |
| 20 | `NameManager` | `0x519209` → `0x5190c0` → `0x518f10`；`0x519255` → `0x5190e0` → `0x519030`（`new` = **`0x519100`**，`alloc 0x60`） | **60** `@0x518fe5` / `@0x5193e8` | **8** 条 `name.*` | 无（G1 结案） |
| 3 | `LocalPlayerManager` | `0x5179d4` → `0x51a730` → `0x51a570`；`0x517a27` → `0x51a710` → `0x519f40`；`0x517a77` → `0x51a6f0` → `0x51a610`（`new` 内联：`0x51792a alloc 0x58`） | **59/50/52/54/66** `@0x51a170`/`0x51a247`/`0x51a31e`/`0x51a3f5`/`0x51a451`；**8** `@0x51a5bb` | **12** 条 `local_player.*` | 第五道门那张表（G7） |

三格都是"**订阅 + 发布两个方向都指到指令 + 最小交互断言**"⇒ 按 §1.1 升 `wired` 成立。
四个档位口径同批改动：`Wired::status()` 三个臂 / `WIRED_TIER_COUNT` 11→14 与
`ASSEMBLED_TIER_COUNT` 9→6 / `assemble_check.gd` 的 `WIRED_TIER` + `TIER_TALLY` / 本文。
`assemble_check` **217 → 226**（+9 条：三格各 3 条，分别指订阅 / 发布 / 状态）。

---

## 3. 逐格展开（地址级细节，可直接照着读）

### 3.1 `GraphwarRankManager`（`#22`，72 B）—— 最便宜的一格

```text
原版 new      arm64 0x498d2c / x86 0x454f30        ← 我方构造点：0x517690 + 0x84b 处 call 0x454f30
订阅①  new_connection_event (5, len 0x14)  @0x454fb5  r8槽 0x9567d8  thunk 0x454f10 → 0x454d80 send_all_info_events
订阅②  entity_removed_event (14, len 0x14) @0x455006  r8槽 0x956800  thunk 0x454ef0 → 0x454e80 handle_entity_removed_event
发布    RankInfo(67)  @0x454e35（0x454d80 内，movabs 0x8000…0000 43 → lea rax,[..+0x43]）
                     @0x45511f（0x455090 set_rank 内）
已知字段 +0x18 HashMap<u32,u32> 基址 / +0x38 RandomState（hasher 在表基 +0x20）/ +0x08 标志
```

**缺**：`+0x08` 的序列化口径（读 30 条指令就能定）；72 B 逐字段表。
**判定**：✅ **纯静态可解**，且是本轮九格里最便宜的一格（2 订阅 + 2 发布 + 1 张表）。

### 3.2 `GraphwarNameManager`（`#20`，72 B）—— 本轮新钉了 `new`

```text
原版 new      arm64 0x40af68 / x86 **0x519100**      ← 我方构造点：0x517690 + 0x7d3 处 call 0x519100
             （`name_to_addr.tsv` 的 4 个"歧义候选"全错；0x4500c0 是 game_server_thread）
订阅①  new_connection_event (5, len 0x14)  @0x519209  thunk 0x5190c0 → 0x518f10 send_all_info_events
订阅②  entity_removed_event (14, len 0x14) @0x519255  thunk 0x5190e0 → 0x519030 handle_entity_removed_event
发布    NameInfo(60)  @0x518fe5（整表重播）/ @0x5193e8（0x519350 set_name，add rax,0x3c）
已知字段 map +0x18 / hasher +0x38 / entry 0x20
```

**缺**：`+0x38`→`+0x48` 那 8 字节；基址口径（`0x50` vs `0x58`，见主表）。
**判定**：✅ **纯静态可解**。

### 3.3 `GraphwarColorManager`（`#21`，120 B）

```text
原版 new      arm64 0x49cbe8 / x86 0x465620        ← 构造点：0x517690 + 0x80f 处 call 0x465620
订阅① new_connection_event (5)  @0x4656a5 → 0x465460 send_all_info_events
订阅② entity_removed_event (14) @0x4656f6 → 0x465570 handle_entity_removed_event
发布   ColorInfo(61) @0x464fb6（0x464d10 add_team_color）/ @0x465517（整表重播）
已知  占用表 +0x18（hasher +0x38）、两池 6 色字面量（逐位）、载荷 8 字节
```

**缺**：`+0x10` 字段语义（**本模块零读点**）、120 B 逐字段、随机流消耗口径。
**判定**：✅ **前两条静态可解**；随机流那一条要么接受"我方用判定规则"（现状），要么开游戏抓一次（`docs/06` §13），**不阻塞升档**。

### 3.4 `GraphwarSoldierManager`（`#5`，72 B）

```text
原版 new      arm64 0x49d590 / x86 0x455760        ← 构造点：0x517690 + 0x468 处 call 0x455760
订阅① new_connection_event (5)  @0x4557e5 → 0x455570 send_all_info_events
订阅② entity_removed_event (14) @0x455836 → 0x4556d0 handle_entity_removed_event
发布   SoldierInfo(15) @0x455616（在 0x455570 内）
未定   add_soldier 的 x86 地址（arm64 0x49d2fc / 144 B 已知）⇒ 它发不发 (15) 未知
```

**缺**：`add_soldier` 落点（**这一条有现成解法**：arm64 侧 144 B 的体做指纹）、键值极性、72 B 逐字段。
**判定**：✅ **纯静态可解**（`tools/func_diff.py` / `sym_match.py` 已有；arm64 有符号是锚）。

### 3.5 `GraphwarGraphicsManager`（`#16`，168 B）

```text
原版 new      arm64 0x49e394 / x86 0x469390        ← 构造点：0x517690 + 0x6e3 处 call 0x469390
订阅① new_connection_event (5)  @0x469415 → 0x468d80 send_all_info_events
订阅② entity_removed_event (14) @0x469466 → 0x469220 handle_entity_removed_event
发布   set_hat(0x468a50) → 55 @0x468abe      set_face(0x468b60) → 53 @0x468bce
       set_skin(0x468c70) → 51 @0x468cde    整表重播(0x468d80) → 3 处 @0x468eac/0x468fe6/0x469122
已知  三张表 +0x18 / +0x48 / +0x78（hasher 各自 +0x20）；outbox/take_outbox 代码已有
```

**缺**：`0xa8` 之后那 `0x30` 字节；第三处重播的 tag（间接载入，未逐位复核）；贴图装载器归属（不在本模块）。
**判定**：✅ **静态可解**（三条都在 `new`/`send_all_info_events` 的体内）。

### 3.6 `GraphwarComputerPlayerManager`（`#18`，72 B）

```text
原版 new      arm64 0x4b02a4 / x86 0x47a720        ← 构造点：0x517690 + 0x75b 处 call 0x47a720
订阅① tick_event (0)          @0x47a7a5 → 0x4785f0 handle_tick
订阅② turn_info_event (43)    @0x47a7fa → 0x479520 handle_turn_info_event
订阅③ entity_removed_event(14)@0x47a84b → 0x47a520 handle_entity_removed_event
发布   add_computer_player 0x478fc0 内 5 处：59 @0x4791f9 / 50-base @0x479271 / 52 @0x4792ec / 54 @0x479366 / 66 @0x4793a6
```

**缺**：`handle_tick` 的 no-return 路径语义、`state` 0/1/2、`computer_function_thread` 落点、`BotData` 120 B、`0x479271` 的 tag。
**判定**：⚠️ **4 条静态可读**；`state` 的三个值的语义很可能是**运行时**（要抓一次 bot 局的 `BotData` 写入）——`docs/06` §13/§14 的活体 harness 能做，但**不阻塞升档**（升档只要"订阅/发/交互断言"，不要求把 AI 的每个状态解完）。

### 3.7 `FunctionHistoryRecorder`（`#19`，48 B）—— 本轮新钉了 `new`，但先裁口径

```text
原版 new      arm64 0x49d9a0 / x86 **0x3f10a0**   ← 构造点：0x517690 + 0x797 处 call 0x3f10a0
订阅   名字 = `function_info_event`（0x1bfccd，len 0x13）@0x3f115a → 处理器 0x3f0e40
       处理器体内判别值闸 `lea rax,[0x7fff…+0x24]` = **35**
发布   0 条（0x3f0e40 里没有 add_event）
```

**缺**：**33/35 的口径冲突**（本模块叫 `function_fire_event`、`docs/07` §7.2 说 35 是 `function_info_event`）、
`Vec<FunctionData>` 的消费点、48 B 逐字段。
**判定**：✅ 口径冲突**静态可裁**（全 `.text` 找构造 33 的点）；消费点那条是**运行时**（真机按上下键），但**不阻塞升档**（本模块不发事件、订阅已在）。

### 3.8 `GraphwarLocalPlayerManager`（`#3`，64 B）—— `new` 是内联的，这是它难的原因

```text
原版 new      内联（arm64 0x409eac 处 alloc 0x58；无独立符号）⇒ x86 也在 0x517690 体内
订阅① connected_to_server_event (7, len 0x19) @0x5179d4 → 0x51a570 handle_connected_to_server
订阅② player_info_event (13, len 0x11)        @0x517a27 → 0x519f40 handle_player_info
订阅③ entity_removed_event (14)               @0x517a77 → 0x51a610 handle_entity_removed_event
发布   handle_player_info 内 5 处：59 @0x51a170 / 50 @0x51a247 / 52 @0x51a31e / 54 @0x51a3f5 / 66 @0x51a451
       handle_connected_to_server 内 1 处：7 @0x51a5bb（重入队）
已知   档案四字段 +0x18/+0x30/+0x48/+0x60；+0x20 槽零读点
```

**缺**：`+0x20` 语义、5 条请求的门、代码侧 `outbox`/`subscribed`。
**判定**：✅ **静态可解**（`alloc 0x58` 之后 24 条写指令 + 5 段门），是本轮第二便宜的一格。

### 3.9 `GraphwarGameStateManager`（`#7`，296 B）—— ⛔ 唯一被禁区挡住的一格

```text
原版 new      arm64 0x4ade6c / x86 0x477bc0   ← 无 call rel32：经槽 0x9bbce8 虚调（站点 0x517b65）
订阅 7 条（本轮逐字节复核，与 156 §50.0 完全一致）
  tick(0) @0x477c92 → 0x470840        new_connection(5) @0x477ce8 → 0x477140
  entity_removed(14) @0x477d3e → 0x476a20   connection_removed(6) @0x477d94 → 0x477540
  function_points(37) @0x477dea → 0x4758b0  skin_graphics_info(51) @0x477e40 → 0x4741a0
  chat_message(71) @0x477e96 → 0x473d00
```

**缺**：**不是证据**——容器里装的是 `src/game.rs` 的 `Match`，它是结算内核（`fire` / `batch` / `batch_conv`
三条驱动轴都吃它），加"7 条订阅 + outbox"必须改那个文件。`156` §50.4 已否掉两条退路。
**判定**：⛔ **被禁区挡住**（`src/game.rs`）。**在解禁之前不要派这一格**——派了也只能得出"绕不过去"。

---

## 4. 仍然真缺的证据（这 9 格里只剩 5 条；G1 任务 74 结案）

| # | 对象 | 缺什么 | 怎么拿 | 静态？ |
|---|---|---|---|---|
| ~~G1~~ **已结案（任务 74）** | NameManager `+0x38`→`+0x48` 的 8 字节 / 基址口径（`0x50` vs `0x58`） | 两个问题同一个误读：盒是 `alloc 0x60`（16 + 80，`M` 基 = 盒 + `0x18`）；`+0x38` 起是 **16** 字节 `RandomState`（写点 `0x5191a5`/`0x5191a9`），72 B 无空洞 | x86 `0x519100` 的 8 条写入点逐条读（`src/graphwar_name_manager.rs` §7） | ✅ 已做 |
| G2 | ColorManager `+0x10` | 字段语义（本模块零读点） | `new` 里 `Rc::new_cyclic_in` 初始化闭包那 10 条写指令 | ✅ |
| G3 | RankManager `+0x08` | 是否进序列化 | `0x455090`–`0x45511f` 逐条 + `docs/07` §3.3 的 `rank_info` 线上体 | ✅ |
| G4 | SoldierManager `add_soldier` | x86 落点（⇒ 它发不发 15） | arm64 `0x49d2fc` 那 144 B 的指纹匹配（`tools/func_diff.py` / `sym_match.py`） | ✅ |
| G5 | FHR 33 vs 35 | 一个**口径判决**（本仓库内部两处说法冲突） | 全 `.text` 找"构造 33"的点；判据：33 有没有订阅者/发布者 | ✅ |
| G6 | ComputerPlayerManager `state` 0/1/2 + `handle_tick` 的 no-return 路径 | 三个取值的语义（写入点全 `.text` 没找到） | 抓一次 bot 局的 `BotData`（`docs/06` §13 的 frida 活体 / `hook_ai_*.js` 已有 6 个） | ⚠️ 建议运行时（**不阻塞升档**） |
| **G7**（任务 74 新登记） | LocalPlayerManager `handle_player_info` 的**第五道门**：`M+0x30` 那个 `Weak` 升级后的一张表 | 表是谁的、entry 为什么 120 字节、为什么"已在表里就不发 5 连发" | `0x519fb1` 的 `call 0x46c5a0` 反查（`tools/addr_probe.py callers`）+ 那张表按 120 B 步长找 value 类型 | ✅ 静态（**不阻塞** `wired`：档位只要求订阅/发布/断言） |

其余"缺"全是**工程**：`outbox` / `subscribed` 两个口 + `interaction_selftest` 里的读数
（写法照 `docs/archive/156-client-rounds.md` §50.1 / §50.5，38 条读数是现成模板）。

---

## 5. 能不能本机静态做完

| manager | 判定 | 依据 |
|---|---|---|
| `GraphwarRankManager` | ✅ **已升 `wired`（任务 74）** | 2 订阅 + 2 发布全部在 `0x454f30`/`0x454d80`/`0x455090` 体内（已落码 + 9 条读数）；`+0x08` 序列化口径仍开（G3） |
| `GraphwarNameManager` | ✅ **已升 `wired`（任务 74）** | 同上（`0x519100`/`0x518f10`/`0x519350`）；72 B 布局 + 基址口径同批结案（G1） |
| `GraphwarColorManager` | ✅ 静态可解 | 三条缺口都在 `new` 与 `add_team_color` 体内；随机流那条**不阻塞** |
| `GraphwarSoldierManager` | ✅ 静态可解 | `add_soldier` 可指纹匹配（arm64 有符号） |
| `GraphwarGraphicsManager` | ✅ 静态可解 | 三张表 + 4 处发布全在 `new`/`send_all_info_events` 体内 |
| `GraphwarLocalPlayerManager` | ✅ **已升 `wired`（任务 74）** | 内联构造的 8 条写入点（+0x20 = 自句柄）+ 5 道门；x86 侧**不需要**找 `new`（就在 `0x517690` 里）；第五道门那张表仍开（G7） |
| `FunctionHistoryRecorder` | ✅ 静态可裁（口径） | 33/35 一条 grep；消费点是运行时，但不阻塞 |
| `GraphwarComputerPlayerManager` | ⚠️ 部分运行时 | 订阅/发布/难度档**静态**；`state` 语义建议抓一次 bot 局（不阻塞） |
| `GraphwarGameStateManager` | ⛔ 禁区（`src/game.rs`） | 证据早就够了（7 条订阅本轮逐字节复核），**卡的是代码归属** |

**✅ 7 格 / ⚠️ 1 格 / ⛔ 1 格**（任务 74 之前）；**任务 74 之后 ✅ 7 格里的 3 格已落地**
（`Rank` / `Name` / `LocalPlayer` → `wired`），剩 4 格 ✅（`Soldier` / `Graphics` / `Color` / `FunctionHistoryRecorder`）。

---

## 6. 下一轮可派排序（"静态可解 × 影响面 ÷ 成本"）

| 序 | manager | 为什么排这里 | 派活形态（一句话） |
|---|---|---|---|
| ~~1~~ **已完成（任务 74）** | `GraphwarRankManager` | 九格里**最便宜**：2 订阅 + 2 发布 + 1 张表 | 已落码 + **9** 条读数 ⇒ `wired` |
| ~~2~~ **已完成（任务 74）** | `GraphwarNameManager` | 结构最干净（`map +0x18`、hasher `+0x38`），`new` 地址已钉 | 已落码 + **8** 条读数 ⇒ `wired`（`fixtures/name_to_addr.tsv` 仍是 §7 C2 的登记，本轮禁改 `fixtures/`） |
| ~~3~~ **已完成（任务 74）** | `GraphwarLocalPlayerManager` | **影响面最大的一格**：50/52/54/59/66 五条"请求"的唯一发起侧 | `+0x20` = 自句柄；门读成 5 道；`outbox` + **12** 条读数 ⇒ `wired` |
| **4** | `GraphwarSoldierManager` | 士兵表是 `turn_manager` 轮转与 `life_manager` 判活的共同输入；`add_soldier` 一锚就通 | "用 arm64 `0x49d2fc` 指纹锚 x86 `add_soldier`；定它发不发 `SoldierInfo`(15)" |
| **5** | `GraphwarGraphicsManager` | 168 B 是九格里最大的结构，且贴图链影响**渲染保真**（`docs/16` §21/§22 的贴图来自它） | "解 `0xa8` 之后 0x30 字节；复核第三处重播 tag；2 订阅 + 4 发布落码" |

**不派**：`GraphwarGameStateManager`（⛔ 禁区）。**可排后**：`ComputerPlayerManager`（要抓 bot 局）、
`FunctionHistoryRecorder`（先裁 33/35，再落码 —— 它不发事件，升档只要订阅 + 断言，成本极低，
但**口径不裁就落码等于把错名字写进代码**）。

---

## 7. 本轮附带更正（4 条，都只登记、不改别人负责的文件）

| # | 对象 | 旧说法 | 本轮实测 | 处置 |
|---|---|---|---|---|
| ~~C1~~ **已修（2026-11-09）** | `docs/00-status.md` §0.1b.2 | "4 个 manager 仍是 `assembled`（11 wired / 2 layout / **7** assembled）" | **9**（三个独立口径：判据函数 / 四常量 + 编译期闸 / `assemble_check.gd`）；且旧行自身 4≠7、11+2+7=20≠22 | 登记；下一轮改那个文件的人顺手改。**本文是文档侧唯一给出 9 的现值页**（代码侧的三处口径都在 §0.1） |
| C2 | `fixtures/name_to_addr.tsv` 的 `graphwar_name_manager / new` 行 | "歧义；候选 `0x3b6b20`, `0x3b6c70`, `0x420b40`, `0x4500c0`" | 真值 **`0x519100`**（4 个候选里 `0x4500c0` 是 `game_server_thread`，另 3 个不在本模块邻域）；判据：它同时订阅本模块的两条事件、处理器正是 `0x518f10`/`0x519030`，且被 `0x517690` 的 `0x517e63` 调用 | 登记（`fixtures/` 本轮禁改） |
| C3 | `fixtures/name_to_addr.tsv` 的 `function_history_recorder / new` 行 | "未知（不引用任何 Location）" | 真值 **`0x3f10a0`**（判据：唯一订阅 `function_info_event`、处理器 `0x3f0e40`、被 `0x517690` 的 `0x517e27` 调用） | 登记（同上） |
| C4 | `src/function_history_recorder.rs` 的 `EVENT_FIRE_TAG` 与模块文档首段 | 把 tag **35** 叫 `function_fire_event` | 字节：订阅名 `function_info_event`（`0x1bfccd`, len `0x13`）+ 处理器体内判别值闸 **35**；`docs/07` §7.2 的 72 条表里 **33 = `function_fire_event`** | 登记为**待裁口径**（G5）；裁决前不要照那个名字落码 |
| C5 | 本文 §2 第 20 行（G1） | "`+0x38`→`+0x48` 那 8 字节没有写入点"、"基址口径 `0x50` vs `0x58`" | x86 `0x519100`：`alloc 0x60`（16 + 80），`M` 基 = 盒 + `0x18`；`+0x38` 起是 **16** 字节 `RandomState`，两个 `u64` 都有写点（`0x5191a5` / `0x5191a9`） | **任务 74 就地更正**：不是空洞、也不是 8 字节；`0x60`/`0x58`/`0x50` 三个数的差别见 `src/graphwar_name_manager.rs` §7 |
| C6 | 本文 §2 第 3 行 | "`+0x20` 那个 8 字节槽语义未知"、"5 条请求的触发条件没逐条读" | `+0x20` = 自句柄（写点 `0x517965`，值 = 盒基）；5 条请求**内部无判据**，门是 5 道（第五道 `0x519fb1`→`0x51a058` 是任务 74 新发现的） | **任务 74 就地更正**；第五道门那张表登记为 G7 |
| ~~C7~~ **已修** | `docs/00-status.md` §0.1b.2 与 `docs/HANDOVER.md` §4 | 两处都写"9 个 manager 仍是 `assembled`（11 wired / 2 layout / 9 assembled / 0 stub）" | 任务 74 之后是 **14 / 2 / 6 / 0**（三格升 `wired`），见 §9 | **已改**：`00` §0.1b.2 与 §0.4 第 5 条、`README.md` 第 5 条、`HANDOVER` §4 现都写 **6 格 / 14 wired / 2 layout / 6 assembled / 0 stub** |

---

## 8. 复现（干净树，5 分钟）

```bash
# 0) 目标 .so（本文一律用 Linux 2.0.60 / 2026-08-28）
#    <WS>/.reference/steam/2026-08-28/libgraphwarrust.so
#    Ghidra/REA 地址 = 本文地址 + 0x100000

# 1) 单个 new：订阅三条判据 + 处理器跳板（addr_probe 会把 .rodata 串与 GOT addend 就地注掉）
../.venv/bin/python tools/addr_probe.py dis 0x455760      # SoldierManager::new
../.venv/bin/python tools/addr_probe.py dis 0x3f10a0      # FunctionHistoryRecorder::new（本轮新钉）

# 2) 构造点：谁调这个 new（应看到 0x517690 = GraphwarGameClient::new 与 0x4500c0 = game_server_thread 两条）
../.venv/bin/python tools/addr_probe.py callers 0x455760

# 3) 发布 tag：两种构造都要看（§1.2）
../.venv/bin/python tools/addr_probe.py dis 0x455090      # set_rank：lea rax,[reg+0x43] → 67
../.venv/bin/python tools/addr_probe.py dis 0x519f40      # handle_player_info：isize::MAX + 0x3c → 59

# 4) 全量普查（本轮的一次性脚本没入库：tools/ 不在允许改动的目录里）
#    配方：用 addr_probe 的 rip 索引取"引用槽 0x9bac88 / 0x9bac80"的全部调用点
#          → 按 FDE 归组 → 逐组跑 §1.2 三条判据（约 40 行 Python，capstone 已在 ../.venv 里）
#    实测：订阅 66 点 / 24 函数；add_event 222 点 / 113 函数
```

**边界（本文不主张的部分）**：不主张"22 个 manager 的订阅表已全部逐个处理器读完"——只主张
"**9 个 `assembled` 的构造点 + 订阅（名字/长度/调用点/处理器跳板）+ 发布（tag/调用点）** 已到指令级"；
处理器**体内**的逐条语义仍按各模块文档的"部分观测"记；`interaction_selftest` 的读数条数**只是回归保护**，
不是复原证据（`AGENTS.md` §0.2）。

---

## 9. 任务 74 的验收数字（本人重跑，不是抄报告）

| 项 | before | after |
|---|---|---|
| 档位分布 | 11 / 2 / 9 / 0 | **14 / 2 / 6 / 0** |
| `cargo test --offline` | 1694 | **1710**（+16：三份 manager 的内联测试 8 条 × 两个编译单元） |
| `cargo test --offline --features gdext-bindings` | 1800 | **1816**（同上；`root_wiring` 的单测随该 feature 编进来） |
| `client/tools/assemble_check.gd` | 217 / 0 | **226 / 0**（+9：三格各 3 条，分别指 订阅 / 发布 / 状态） |
| headless 14 个脚本合计 | 1818 | **1827**（其余 13 个逐项不变） |
| `selftest` | 714 / 0 / 1 skip | **714 / 0 / 1 skip**（不变） |
| 三轴 `fire` / `batch` / `batch_conv` | 647 / 647 / 637 | **647 / 647 / 637**（不变） |

复跑（全部在本分支的私有 target 上）：

```bash
cargo test --offline && cargo test --offline --features gdext-bindings
cargo build --offline --features gdext-bindings && cp $CARGO_TARGET_DIR/debug/libgw2_rebuild.so client/libgw2_rebuild.so
"$GODOT" --headless --path client --import
for s in gdext assemble plane_on_screen obstacle_drawer function_drawer draw_classes curve_on_screen \
         entity_drawer base_classes signal_classes spike_class play_flow process_loop recovered_view; do
  "$GODOT" --headless --path client --script res://tools/${s}_check.gd | tail -2
done
"$GODOT" --headless --path client -- --selftest        # 714 / 0 / 1 skip
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive fire  --policy default --json f.jsonl
cargo run --release --offline -q --example replay_attrib -- --dir <快照> --drive batch --policy default --json b.jsonl
cargo run --release --offline -q --example batch_conv    -- --dir <快照> --policy strict
```
