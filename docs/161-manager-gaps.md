# 161 · 9 格 `assembled` 的升档清单（**3 已升 / 余 6**）：每格要什么证据

> **一句话**：档位分布现在是 **18 `wired` / 2 `layout` / 2 `assembled` / 0 `stub`**
> （本文定稿时是 11 / 2 / 9 / 0；任务 74 把 `Rank` / `Name` / `LocalPlayer` 三格升到 `wired`
> → 14 / 2 / 6 / 0；**任务 77 把 `Color` / `Soldier` / `Graphics` / `FunctionHistoryRecorder`
> 四格升到 `wired`** → 18 / 2 / 2 / 0，见 §2.1 / §2.2 与 [`16-client.md`](16-client.md) §54/§55）。
> 本轮（161）把 9 个里的 **8 个**的
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
| 判据函数本体 | `src/_scaffold/root_wiring.rs` 的 `Wired::status()`（`match` 臂） | `wired` 臂 **18** 个 / `layout` 臂 **2** 个 / 其余落 `_ =>` = **2** |
| 四个计数常量 + 编译期闸 | 同文件 `WIRED_TIER_COUNT` / `LAYOUT_TIER_COUNT` / `ASSEMBLED_TIER_COUNT` / `STUB_TIER_COUNT` | **18 / 2 / 2 / 0**；`const _: () = assert!(四者之和 == Wired::COUNT)`（= 22）**在编译期就红**，改一槽漏改这里编不过 |
| 运行时断言表 | `client/tools/assemble_check.gd` 的 `WIRED_TIER[22]` + `TIER_TALLY` | 逐槽字符串 / 计数，与上二者一致（**18 / 2 / 2 / 0**） |

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
**任务 77 再升 4 个**（→ `wired`）：`5 Soldier` / `16 Graphics` / `19 FunctionHistoryRecorder` / `21 Color`（§2.2）。
**现在剩的 2 个**：`7 GameState`（⛔ 禁区 `src/game.rs`）/ `18 ComputerPlayer`（要抓一次 bot 局）。

### 0.3 ⚠️ `docs/00` 那条"7"是**陈旧数据**（本轮登记，不改）

[`00-status.md`](00-status.md) §0.1b.2 的倒数第二行写：

```text
| ~~4 个 manager 仍是 `assembled`~~（**已由协调方就地更正为 9**） | 只构造注册、没接事件流（**11 wired / 2 layout / 9 assembled / 0 stub = 22**） | 缺工程 | ✅ 可做 |

> ⚠️ 上面这段是 `docs/00` 的**历史引文**。任务 74 之后是 **14 / 2 / 6 / 0**、任务 77 之后**现值是 18 / 2 / 2 / 0**
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
| 3 | `GraphwarLocalPlayerManager` / 64 / `graphwar_local_player_manager::GraphwarLocalPlayerManager` | `new` **内联**进 `GraphwarGameClient::new`（arm64 `0x409eac` 处 `alloc 0x58`，无独立符号）；**订阅 3 条**（全在 `0x517690` 体内）：`connected_to_server_event`(7) `@0x5179d4` → `0x51a570`、`player_info_event`(13) `@0x517a27` → `0x519f40`、`entity_removed_event`(14) `@0x517a77` → `0x51a610`；**发 6 条**：`handle_player_info`(`0x519f40`) 里 5 处 `add_event` = **59**`@0x51a170` / **50**`@0x51a247` / **52**`@0x51a31e` / **54**`@0x51a3f5` / **66**`@0x51a451`，`handle_connected_to_server`(`0x51a570`) 里 1 处 = **8**`@0x51a5b1`（`0x51a577` 那处 `movabs …07` 是**入向断言**；`docs/162` §D4）；档案四字段 `+0x18/+0x30/+0x48/+0x60`（模块文档 §3） | ✅ **任务 74 已升 `wired`**（§2.1）：① `+0x20` = `Weak<RefCell<Self>>` 自句柄（写点 `0x517965`，值 = 盒基）② 5 条请求内部**没有**逐条判据，门是**五道**（`0x519f68` connected / `0x519f71` conn 相等 / `0x519f9d` 旧 len==0 / **`0x519fb1`→`0x51a058`**）③ `outbox`/`subscribed` 已落码。**任务 79 结案（§2.3）**：`+0x30` = **模块容器弱句柄**；第五道门 = `get_module_wk::<ComputerPlayerManager>`（`0x46c5a0`）之后在 CPM 的 Bot 表里按 `player_id` 查一次 —— 命中就**不发** 5 连发 | ① `new` 的 `alloc 0x58` 之后 24 条写指令逐条读（arm64 `0x409eac`–`0x409f60`），判 `+0x20` 是 `Option`/`Weak`/标志 ② `0x519f40` 的 5 段：每段 `add_event` 前的 `cmp`/`jne` 就是门（`addr_probe.py dis 0x519f40`，5 段各 40–60 行）③ 这一条不用新证据，照 `docs/archive/156-client-rounds.md` §50.1 的写法加 `subscribed`/`outbox` |
| 5 | `GraphwarSoldierManager` / 72 / `graphwar_soldier_manager::SoldierTable` | `new` = `0x455760`（call 点 `0x517af8`，arm64 `0x49d590`）；**订阅 2 条**：`new_connection_event`(5) `@0x4557e5` → `0x455570` `send_all_info_events`、`entity_removed_event`(14) `@0x455836` → `0x4556d0`；**发 1 条** `SoldierInfo`(15) `@0x455616`；模块文档 §6 已列 5 条未知 | ✅ **任务 77 已升 `wired`**（§2.2）。原缺：① `add_soldier` 的 x86 地址**没锚到**（`name_to_addr.tsv` 两个候选 `0x3e2830` / `0x474a10` 都被证否）⇒ 它**发不发 `SoldierInfo`(15)** 未知（模块 §6-B）② 表里**键值极性**只有间接证据（§6-D）③ 72 B 逐字段 | ① 用 arm64 侧**已知的 144 B 体**（`0x49d2fc`）做 x86 指纹匹配：`tools/func_diff.py` / `tools/sym_match.py`（arm64 有符号、x86 无符号，这是现成路子）② 极性：读 `send_all_info_events` `0x455570` 的载荷装配（哪个寄存器进 `+0x00` / `+0x08`）+ `handle_entity_removed_event` `0x4556d0` 拿 `event+8` 当 key 的那条 `cmp` ③ 同上判据 §1.2 的"写入点 + 偏移"清单 |
| 7 | `GraphwarGameStateManager` / 296 / `game::Match` | `new` = `0x477bc0`（**虚调**：经槽 `0x9bbce8`，站点 `0x517b65`；arm64 `0x4ade6c`）；**订阅 7 条**，全部**逐字节复核**且与 `156` §50.0 那 7 个名字**完全一致**：`tick`(0)`@0x477c92`→`0x470840`、`new_connection`(5)`@0x477ce8`→`0x477140`、`entity_removed`(14)`@0x477d3e`→`0x476a20`、`connection_removed`(6)`@0x477d94`→`0x477540`、`function_points`(37)`@0x477dea`→`0x4758b0`、`skin_graphics_info`(51)`@0x477e40`→`0x4741a0`、`chat_message`(71)`@0x477e96`→`0x473d00` | **不是证据缺，是禁区块**：容器里装的是 `src/game.rs` 的 `Match`（结算内核，`fire`/`batch` 三条驱动轴都在用它）⇒ 接事件层**必须改 `src/game.rs`**；`grep -n "outbox\|subscribed" src/game.rs` 零命中。另：296 B 布局**没有**逐字段（`Match` 不是布局复原，是行为复原） | 两条路都已被 `156` §50.4 否定（加壳会改容器类型、另起旁表=把猜测写成行为）。**唯一的正路**：等 `src/game.rs` 解禁后按 §50.1 的六个 manager 同款写法加 `SUBSCRIBED_EVENTS[7]` + `outbox` + 7 个 `handle_*` 入口。**在此之前派活只会撞墙** |
| 16 | `GraphwarGraphicsManager` / 168 / `graphwar_graphics_manager::GraphwarGraphicsManager` | `new` = `0x469390`（call 点 `0x517d73`，arm64 `0x49e394`）；**订阅 2 条**：`new_connection_event`(5)`@0x469415`→`0x468d80`、`entity_removed_event`(14)`@0x469466`→`0x469220`；**发 4 处**：`set_hat_graphics`(`0x468a50`)→**55**`@0x468abe`、`set_face_graphics`(`0x468b60`)→**53**`@0x468bce`、`set_skin_graphics`(`0x468c70`)→**51**`@0x468cde`、`send_all_info_events`(`0x468d80`) 三处重播 `@0x468eac`/`@0x468fe6`/`@0x469122`；三张表在 `+0x18/+0x48/+0x78`（hasher 各自 `+0x20`，模块文档 §2）；`outbox`/`take_outbox` **代码里已有** | ✅ **任务 77 已升 `wired`**（§2.2）。原缺：① 168 B 里**三张表以外**还有 4 个字（`0x18×3 = 0x78`，到 `0xa8` 之间有 `0x30` 字节）没解 ② `send_all_info_events` 的**第三处**重播 tag 是**间接载入**（`mov rax,[rsp+0x10]`）⇒ 我方扫描只读到两处字面量 51/53，第三处按模块文档是 **55**，**未逐位复核** ③ 贴图来源不在本模块（`GraphwarTextureLoader`，模块 §3 的有界否定） | ① `new` 的 `alloc` 之后全部写指令（`0x469390`–`0x4694e9`）+ 三个 getter（`+0x150/+0x180/+0x1b0` 一族）逐条读 ② `0x469122` 往前 20 条，跟 `[rsp+0x10]` 的来源（三表循环的公共 tag） ③ 贴图侧去找 `GraphwarTextureLoader` 的 `new`（不是本槽的事，别混） |
| 18 | `GraphwarComputerPlayerManager` / 72 / `graphwar_computer_player_manager::ComputerPlayers` | `new` = `0x47a720`（call 点 `0x517deb`，arm64 `0x4b02a4`）；**订阅 3 条**：`tick_event`(0)`@0x47a7a5`→`0x4785f0` `handle_tick`、`turn_info_event`(43)`@0x47a7fa`→`0x479520`、`entity_removed_event`(14)`@0x47a84b`→`0x47a520`；**发 5 条**（全在 `add_computer_player` `0x478fc0`）：**59**`@0x4791f9`、50-base`@0x479271`（tag 待读）、**52**`@0x4792ec`、**54**`@0x479366`、**66**`@0x4793a6`；难度六档阈值 `4/11/26/101/251`（模块 §3，与 [`13-ai.md`](13-ai.md) §1 互证） | ✅ **任务 79 已结案（§2.3）**：① `state` = **3/1 两个值**（`0x478fe3` 建档 3、`0x479dfa` spawn 后 1、`0x47864f` 收尾 3；**`0`/`2` 无写入点**，有界否定 + 正对照）② `BotData` 的 `+0x00`=state / `+0x60`=level / `+0x64`=精英 / `+0x68`=每槽 5 / `+0x6c`=limit （静态 + 运行时原文）③ 键是 **`player_id`** 不是 connection id（运行时：key 232 的 Bot，`player_info= (232, conn=228=我的)`) ④ 第五道门 G7 **结案**：那张 120 B 表**就是** CPM 的 Bot 表（§2.3） **仍缺**：`handle_tick` 收尾段的逐句语义（§7-A）、`computer_function_thread` 的 x86 地址（§7-C，仍是推断） | ① `addr_probe.py dis 0x4785f0`（收尾段 `0x4786b2`–`0x478e38`）② `0x47a300` 的形参表逐句读 ③ 抓一次 `turn_info` 里 `spawn` 的闭包指针与线程体入口对拍 |
| 19 | `FunctionHistoryRecorder` / 48 / `function_history_recorder::History` | **`new` = `0x3f10a0`（本轮新钉；`name_to_addr.tsv` 原记"未知"）**，call 点 `0x517e27`，arm64 `0x49d9a0`；**订阅 1 条**：名字逐字节 `function_info_event`（`0x1bfccd`，len `0x13`）`@0x3f115a` → 处理器 `0x3f0e40`，其体内**判别值闸 = 35**（`lea rax,[isize::MAX+0x24]`）；**发 0 条**（`0x3f0e40` 体内无 `add_event`）；容量/去重两条已有硬结论（模块文档，观测） | ✅ **任务 77 已升 `wired`**（§2.2）。原缺：① **口径冲突（必须先裁）**：本模块把 tag **35** 叫 `function_fire_event`（`EVENT_FIRE_TAG = 0x8000…0023`），而 [`07-protocol.md`](07-protocol.md) §7.2 的 72 条表里 **33 = `function_fire_event`、35 = `function_info_event`**；本轮字节证据（订阅名字 + 判别值闸）**两边都指向 35** ② `Vec<FunctionData>` 的**消费点**（上下键）全库无落点（有界否定，模块文档 §"上下键"）③ 48 B 逐字段 | ① 裁定判据：全 `.text` 找**构造 33** 的地方（`movabs 0x8000…><0021` 或 base+`0x21`）；若 33 只在**解码器**里出现、且没有任何订阅者，则"35 才是 recorder 订阅的那条"成立，我方改名 ② 消费点：真机按一次 `ui_up` 抓 `LineEdit.text`（`docs/06` §13 的 `tools/frida_live.js` 可用），或者接受"只写不读"并登记 ③ 同上逐字段 |
| 20 | `GraphwarNameManager` / 72 / `graphwar_name_manager::GraphwarNameManager` | **`new` = `0x519100`（本轮新钉；`name_to_addr.tsv` 给的 4 个候选 `0x3b6b20`/`0x3b6c70`/`0x420b40`/`0x4500c0` 全错——`0x4500c0` 是 `game_server_thread`）**，call 点 `0x517e63`，arm64 `0x40af68`；**订阅 2 条**：`new_connection_event`(5)`@0x519209`→`0x518f10`、`entity_removed_event`(14)`@0x519255`→`0x519030`；**发 2 条** `NameInfo`(60)：`@0x518fe5`（整表重播）、`@0x5193e8`（`set_name` `0x519350`）；表 `+0x18`、hasher `+0x38`、entry `0x20`（模块文档 §2） | ✅ **任务 74 已升 `wired`**（§2.1）：① 结案——盒是 `alloc 0x60`（RcInner 16 + RefCell 80），`M` 基 = `盒+0x18`；`+0x38` 起那 **16** 字节是 `RandomState`（两个 `u64`，写点 `0x5191a5`/`0x5191a9`），**没有空洞**，72 B 全部有写入点 ② `outbox`/`subscribed` 已落码。**无遗留** | ① 先定基址：arm64 `0x40af68` 的 `alloc` 尺寸（`RefCell<M>` = 80 = 72+8 ⇒ **`0x50`**，不是 `0x58`；`0x58` 是 `RcInner`），再把写入点减去 `0x18` 对齐 ② 断言照 `156` §50.5 的写法：`set_name(9010,"x")` → 读回表 → `take_outbox()` 应恰好 1 条 (60) |
| 21 | `GraphwarColorManager` / 120 / `graphwar_color_manager::GraphwarColorManager` | `new` = `0x465620`（call 点 `0x517e9f`，arm64 `0x49cbe8`）；**订阅 2 条**：`new_connection_event`(5)`@0x4656a5`→`0x465460`、`entity_removed_event`(14)`@0x4656f6`→`0x465570`；**发 2 条** `ColorInfo`(61)：`@0x464fb6`（`add_team_color` `0x464d10`）、`@0x465517`（整表重播）；两张 6 色池字面量**逐位**（模块 §2）、分配规则"队号选池 + 随机起点 + 线性探测"（§3）、载荷 8 字节 `{id@+0, r@+4, g@+5, b@+6}`（§5） | ✅ **任务 77 已升 `wired`**（§2.2）。原缺：① `new` 的第三个字段 `self+0x10` **没被本模块任何函数读过**（模块 §6 第一条）② 120 B 逐字段 ③ 随机流**消耗顺序**：起点 1 次 `u64` + 兜底 3 次 `u8` ⇒ 复现第 n 个玩家必须真 `rand_chacha` 流（模块 §6 第三条，标推断） | ① `+0x10` 只在 `new` 里写、读侧零命中 ⇒ 判"留给别人的借用句柄 or 缓存"，定案要**读 `new` 的初始化闭包**（`Rc::new_cyclic_in`，`0x465620` 里那段）② 逐字段同上 ③ 随机流：抓一次 `request_new_player` 前后的 ChaCha 状态（`tools/capture_rand_values.js` 已有；见 `docs/06` §13 的 frida 活体） |
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

### 2.2 任务 77 完成的四格（地址级：补了什么 / 还剩什么）

四格都是同一个判据：**容器 + 事件流两个方向都指到指令 + 最小交互断言**（§1.1）。
证据全部在 **x86** 上逐条复核（arm64 只用来定名，不抄结论）。

| # | manager | 订阅（x86 调用点 → 闭包槽`+0x20` → 处理器） | 发布（x86 `add_event` 点） | 交互读数 | 剩余 |
|---|---|---|---|---|---|
| 21 | `ColorManager` | `new_connection_event` `0x4656a5` → `0x957dc0` → **`0x465460`**；`entity_removed_event` `0x4656f6` → `0x957de8` → **`0x465570`** | **61** `@0x464fb6`（`add_team_color`）/ `@0x465517`（整表重播）；`0x465570` 体内**零** `add_event` | **8** 条 `color.*` | 随机流口径（不阻塞） |
| 5 | `SoldierManager` | `0x4557e5` → `0x956980` → **`0x455570`**；`0x455836` → `0x9569a8` → **`0x4556d0`** | **15** `@0x455616`（整表重播）＋ **15** `@0x474ba5`（`add_soldier`，内联进 `0x474a10`）＋ **14**（级联 `0x444fd0`） | **7** 条 `soldier.*` | 无（`add_soldier` 极性/发送已结案） |
| 16 | `GraphicsManager` | `0x469415` → `0x958330` → **`0x468d80`**；`0x469466` → `0x958358` → **`0x469220`** | **51** `@0x468cde` / **53** `@0x468bce` / **55** `@0x468abe`（三个 setter）；整表重播三处 `@0x468eac`/`@0x468fe6`/`@0x469122` = 51/53/**55** | **9** 条 `graphics.*` | 贴图侧（`GraphwarTextureLoader`，另一个模块） |
| 19 | `FunctionHistoryRecorder` | `function_info_event`（`0x1bfccd`, len `0x13`）`0x3f115a` → `0x955050` → **`0x3f0e40`** | **0 条**（`0x3f0e40` 体内零 `add_event`；发方向是**正面证据**） | **8** 条 `history.*` | 消费点（上下键）仍未知 |
| 3 | `LocalPlayerManager` | （任务 74：`0x5179d4`/`0x517a27`/`0x517a77`） | 6 处 | 12 条 | 第五道门的表（G7） |

四格各自补上的**证据**（这一轮新解出来的东西，逐条能指到指令）：

**① `Color` 的 `+0x10`（G2）—— 容器弱引用，且本模块零读点**

```text
建盒 0x524140（唯一调用点 0x465632）：alloc 0x90 = RcInner 16 + RefCell 8 + 120
  0x52422f mov [rbx+0x18], rbx      ; M+0x00 = 盒基（弱自句柄）
  0x524233 mov [rbx+0x20], r14      ; M+0x08 = 事件通道 Rc（调用点 0x517e83 inc [r14] = 强计数）
  0x524237 mov [rbx+0x28], r15      ; M+0x10 = **容器 Weak**（调用点 0x517e8c inc [r15+8] = 弱计数）
两张 6 色表由 **0x465070**（`init_team_colors`，唯一调用点 0x5241e9）建，落在 M+0x48
  ⇒ 120 = 0x18 头 + 0x30 占用表 + 0x30 队池表（没有空洞）
读点普查（有界否定，带正对照）：4 个函数体里对 M+0x10 的读 = **0**；
  同一次扫描能看到 +0x18/+0x30/+0x38/+0x48/+0x60/+0x68 的读写 ⇒ 不是扫法坏了
```

**② `Soldier` 的 `add_soldier`（G4）—— 它**发** `SoldierInfo`(15)，而且是内联的**

```text
x86 侧没有独立函数：正文内联进 GraphwarGameStateManager::add_soldier_to_player（0x474a10）
  判据① 该函数正是调三个外观 setter 的那个（0x474d91 / 0x474eca / 0x47500c = skin/face/hat）
  判据② addr_probe.py locs 0x474a10 命中 src/graphwar_soldier_manager.rs:65:26（另外 17 条是 game_state_manager）
  判据③ 体内用的是本模块的表基/通道：0x474b5d lea rdi,[r14+0x30]（= M+0x18）、0x474b6a mov r15,[r14+0x20]（= M+0x08）
块内（0x474b38–0x474bab）：
  0x474b50 mov ebx,1 ; 0x474b55 lock xadd [0x9c10c0], ebx   ; 全局原子计数器发一个**新兵 id**
  0x474b61 mov esi,ebx / 0x474b63 mov edx,ebp / 0x474b65 call 0x442330   ; insert(新兵, owner)
  0x474b85/0x474b89 mov [rsp+0x30],[rsp+0x34] = (新兵, owner)            ; 载荷 +0x08 / +0x0c
  0x474b8d movabs rax,0x8000000000000000 ; 0x474b97 add rax,0xf          ; tag 15
  0x474ba5 call [0x9bac80] → 0x3d3530                                   ; add_event
⇒ 结论：**会发 (15)**；载荷 = (新兵 id, owner)；键值极性 = **士兵 → 主人**（两条独立证据同向）
```

> ⚠️ 旧普查（`addr_probe.py tags`）只认三种 materialize 形态，**漏了第四种**
> `movabs 0x8000…000 ; add rax,N`：它把这批记进 **tag 0**（修前 2235 处），所以上一轮只看到
> `send_all_info_events` 的 1 处、误以为 `add_soldier` 不发。判据见 §7 C10。
>
> ✅ **2026-11-10 任务 83 已修**：`tags` 认第四形态后 tag 0 = **2106** 处（−129），
> `0x474b8d` 归 tag 15（tag 15 由 45 → **46** 处），本文这条结论**不变**、且现在用工具就能复现。
> 新口径与全量复扫表见 [`docs/07`](07-protocol.md) §7.1.1 + `fixtures/tag_counts83.tsv`。

**③ `Graphics` 的 168 B 与第三处重播（G-16）**

```text
建盒 0x522710（唯一调用点 0x4693a2）：alloc 0xc0 = RcInner 16 + RefCell 8 + 168
  [rbx+0x18]=rbx / [rbx+0x20]=r14 / [rbx+0x28]=r15      ; 头三个字（同 Color：弱自句柄/通道 Rc/容器 Weak）
  [rbx+0x30..0x50] + [rbx+0x50]=rax,[rbx+0x58]=rdx      ; 皮肤表 + hasher(M+0x38/+0x40)
  [rbx+0x60..0x80] + [rbx+0x80]=rcx,[rbx+0x88]=rdx      ; 脸表 + hasher(M+0x68/+0x70)
  [rbx+0x90..0xb0] + [rbx+0xb0],[rbx+0xb8]              ; 帽子表 + hasher(M+0x98/+0xa0)
⇒ 168 = **0x18 头 + 3×0x30 表**，**表后面没有字节**（旧说法"0xa8 之后还有 0x30 字节"是错的，见 §7 C8）
第三处重播 tag（旧说法"间接载入、未逐位复核"）：
  0x469035 movabs rdx,0x8000000000000033 ; 0x46903f or rdx,4     ; = 55
  0x46910e mov rax,[rsp+0x10] ; 0x469113 mov [rsp+0x30],rax      ; 三支共用同一段提交代码
  0x46911d lea rsi,[rsp+0x30] ; 0x469122 call [0x9bac80]         ; ⇒ 第三处 = 55（帽子）
```

**④ `FunctionHistoryRecorder` 的 33/35 与 `new` 的 48 B**

```text
new = 0x3f10a0（alloc 0x48 = 16 + 8 + 48）
  M+0x00/+0x08/+0x10 = Vec 的 cap/ptr/len（0 / 8 = dangling / 0）
  M+0x18 = 盒基（弱自句柄）· M+0x20 = 事件通道 Rc · M+0x28 = 容器 Weak
裁决：订阅的是 **function_info_event = 35**
  订阅名 0x3f1141 引 0x1bfccd（"function_info_event"，全 .text 只有这一个引用点）+ 0x3f114f mov edx,0x13
  处理器闸 0x3f0e4e/0x3f0e58 lea rax,[isize::MAX+0x24] = 0x8000…0023 = 35
  33 的反证：串 0x1bf607 只有 2 个引用点（0x3d53d4 / 0x3dca26，都在名字↔tag 表里），
             tag 33 的 materialize 只有 3 处（0x3cab13/0x3caba7/0x470f3a），**没有任何订阅者**
             〔任务 83 用四形态复扫复核：**仍是这 3 处**，地址一字不差 ⇒ 这条不变〕
⇒ 常量改名 EVENT_FUNCTION_INFO_TAG（值仍是 35），旧名 EVENT_FIRE_TAG 保留为别名（tests/last_five.rs 在用）
```

四格升完后四个档位口径同批改动：`Wired::status()` 四臂 / `WIRED_TIER_COUNT` 14→18 与
`ASSEMBLED_TIER_COUNT` 6→2 / `assemble_check.gd` 的 `WIRED_TIER` + `TIER_TALLY` / 本文。
`assemble_check` **226 → 238**（+12 条：四格各 3 条，分别指订阅 / 发布 / 状态）。

### 2.3 任务 79 完成的一格（`ComputerPlayerManager`）+ G7 结案

`GraphwarComputerPlayerManager` **`assembled` → `wired`**（第 18 行）。判据仍是 §1.1 那三件，
第三件（最小交互断言）本轮补上 **10 条** `computer.*` 读数；前两件的地址级证据原本就在，本轮补的是**语义**：

| # | 项 | 本轮拿到的 | 档 |
|---|---|---|---|
| 1 | `BotData` 的**键** | **`player_id`**（`bot_added_event+0x0c`），不是 connection id：运行时 key 232 的 Bot，其 `player_info_event` 是 `(player_id=232, connection_id=228)`，228 是**房主（我）的连接** | 观测 |
| 2 | `BotData` 字段 | 桶 120 B = key(8) + 112 B；`+0x00` state / `+0x60` level / `+0x64` 精英 / `+0x68` 每槽 5 / `+0x6c` limit（`+0x10/+0x14` 是两个 `f32 1.0`）| 静态 + 运行时原文 |
| 3 | `state` 状态机 | **3 → 1 → 3**：`0x478fe3`（建档 3）/ `0x479dfa`（`thread::spawn` 后写 1，`rbx=[rsp+0xa8]=1` 见 `0x4796f9`）/ `0x47864f`（`handle_tick` 收尾 3）。`0`/`2` **无写入点** | 观测（3 次迁移，100 ms 直读） |
| 4 | `handle_tick` 的"不返回" | 旧说法（`0x5ec160`）**不成立**：体内没有它；`state != 3` 是**收尾 + 复位**，真正的 panic 串是 `"Failed to join computer player thread."`（`0x478ebb`/`0x478ee0 ud2`）| 逐位（字节） |
| 5 | **G7 第五道门** | `M+0x30` = **容器**弱句柄；门 = `get_module_wk::<ComputerPlayerManager>`（`0x46c5a0`）→ 在 CPM 的 Bot 表按 `player_id` 查一次 → 命中则**不发** 5 连发 | 逐位 + 观测（返回值 == CPM 盒基 **3/3**） |

**G7 的四条独立链**（详见 `docs/06` §46.2）：

```text
① 指令形状  0x519fb1 lea rdi,[r14+0x30] ; 0x519fb5 call 0x46c5a0（get_module_wk 的 20 个单态化之一）
② 返回形状  0x519fe6 [rax+0x48] = items；+0x18 ctrl / +0x20 mask / +0x38 RandomState —— 与 CPM（72 B）逐字段相同
③ 同套偏移  CPM 自己三个函数（0x478fc0 插 / 0x47a520 删 / 0x479520 查）用的就是 stride 0x78、key@entry+0x00、state@entry+0x08
④ 运行时    0x46c5a0 的返回值 3/3 == CPM 盒基（`cpmSelf-0x18`）；其中 1 次来自本门、2 次来自 bot_added 分派 0x3e3d7f
```

⚠️ **命中分支没走到**（照实记）：3 局（LAN 人+Bot / AI-vs-AI / 战役）门⑤ 各只求值一次，**都走未命中**（`0x51a084`）并发 5 条；
原因结构性 —— 门④ 只放"本连接的第一个 `player_info`"，而那个"第一发"永远是本机玩家自己（pid 229，t≈29.5–36.8 s），
Bot 只能更晚加进来。要打中它需要"本连接的第一个玩家是 Bot"的局（纯 Bot 客户端 / 连接重建），本轮没造出来。
**正对照**：同一条观测链在 3 局里都完整打出 `gate5_enter → get_module_wk → gate5_miss → 5 条 send`。

本轮四个档位口径同批改动：`Wired::status()` 加一格 / `WIRED_TIER_COUNT` 18→19 与 `ASSEMBLED_TIER_COUNT` 2→1 /
`assemble_check.gd` 的 `WIRED_TIER` + `TIER_TALLY` / 本文。`assemble_check` **238 → 241**（+3，指订阅/发布/状态），
交互读数 **135 → 145**（+10）。

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
| ~~G2~~ **已结案（任务 77）** | ColorManager `+0x10` | **容器弱引用**（`Weak<RefCell<GraphwarModuleContainer>>`）：建盒 `0x524237` 写入、调用点 `0x517e8c` 按**弱计数** +1；本模块 4 个函数零读点（有界否定 + 正对照） | x86 `0x524140` 建盒 + `0x465620` 内读点普查 | ✅ 已做 |
| G3 | RankManager `+0x08` | 是否进序列化 | `0x455090`–`0x45511f` 逐条 + `docs/07` §3.3 的 `rank_info` 线上体 | ✅ |
| ~~G4~~ **已结案（任务 77）** | SoldierManager `add_soldier` | x86 落点 = **内联进 `0x474a10`**（`Location src/graphwar_soldier_manager.rs:65:26`）；**它发 (15)**（`0x474b8d`+`0x474b97` → `0x474ba5`）；键值极性 = 士兵→主人（`0x474b50` 原子计数器发号当 key） | x86 逐条（§2.2 ②） | ✅ 已做 |
| ~~G5~~ **已结案（任务 77）** | FHR 33 vs 35 | **35 才是本记录器订阅的那条**（`function_info_event`）：订阅名串 `0x1bfccd` 全文唯一 + 处理器闸 = 35；`function_fire_event`(33) **没有任何订阅者** | x86 逐条（§2.2 ④） | ✅ 已做 |
| ~~G6~~ **已结案（任务 79）** | ComputerPlayerManager `state` 0/1/2 + `handle_tick` 的 no-return 路径 | **只有 3 和 1**：`3`=空闲（`0x478fe3` / `0x47864f`）、`1`=已 spawn 未收（`0x479dfa`）；`0`/`2` 无写入点（有界否定 + 正对照）。`handle_tick` 体内**没有** `0x5ec160`：`state != 3` 是**收尾**，panic 串是 `"Failed to join computer player thread."` | 运行时 bot 局 3 次 + `addr_probe.py dis 0x4785f0`（`docs/06` §46.4） | ✅ 已做 |
| ~~G7~~ **已结案（任务 79）** | LocalPlayerManager `handle_player_info` 的**第五道门** | 表 = **`GraphwarComputerPlayerManager` 的 Bot 表**（`HashMap<u32 /*player_id*/, BotData>`，桶 120 B）；`M+0x30` 是**容器**弱句柄，门里 `get_module_wk::<CPM>`（`0x46c5a0`）→ 按 `player_id` 查一次；命中 ⇒ 不发 5 连发（Bot 身份由 `add_computer_player` 自己申报）。四条链：指令形状 / 返回类型形状 / CPM 自己三个函数的同套偏移 / 运行时返回值 == CPM 盒基 3/3 | `docs/06` §46.2–46.3；`tools/frida_cpm_gates.js` | ✅ 静态 + 运行时（**命中分支未观测到**，见 §2.3 与 `docs/06` §46.5） |

其余"缺"全是**工程**：`outbox` / `subscribed` 两个口 + `interaction_selftest` 里的读数
（写法照 `docs/archive/156-client-rounds.md` §50.1 / §50.5，38 条读数是现成模板）。

---

## 5. 能不能本机静态做完

| manager | 判定 | 依据 |
|---|---|---|
| `GraphwarRankManager` | ✅ **已升 `wired`（任务 74）** | 2 订阅 + 2 发布全部在 `0x454f30`/`0x454d80`/`0x455090` 体内（已落码 + 9 条读数）；`+0x08` 序列化口径仍开（G3） |
| `GraphwarNameManager` | ✅ **已升 `wired`（任务 74）** | 同上（`0x519100`/`0x518f10`/`0x519350`）；72 B 布局 + 基址口径同批结案（G1） |
| `GraphwarColorManager` | ✅ **已升 `wired`（任务 77）** | `+0x10` = 容器弱引用（G2 结案）+ 2 订阅 + 2 发布全指到指令 + 8 条读数；随机流口径仍开（不阻塞） |
| `GraphwarSoldierManager` | ✅ **已升 `wired`（任务 77）** | `add_soldier` 内联落点 + 它发 (15) + 极性全部结案（§2.2 ②）；2 订阅 + 3 处发布 + 7 条读数 |
| `GraphwarGraphicsManager` | ✅ **已升 `wired`（任务 77）** | 168 = 0x18 头 + 3×0x30（旧说法已改正）+ 第三处重播 tag = 55 逐位复核 + 9 条读数 |
| `GraphwarLocalPlayerManager` | ✅ **已升 `wired`（任务 74）** | 内联构造的 8 条写入点（+0x20 = 自句柄）+ 5 道门；x86 侧**不需要**找 `new`（就在 `0x517690` 里）；第五道门那张表仍开（G7） |
| `FunctionHistoryRecorder` | ✅ **已升 `wired`（任务 77）** | 33/35 已裁决（35）；1 订阅 + **发 0 条**（正面证据）+ 8 条读数；消费点仍未知（不阻塞） |
| `GraphwarComputerPlayerManager` | ✅ **已升 `wired`（任务 79）** | 3 订阅 + 5 发布 + 难度六档**静态**；`state` 语义运行时结案（3/1，3 次迁移）+ 10 条交互读数；只剩 `handle_tick` 收尾段逐句与线程体地址（§7-A/C，不阻塞） |
| `GraphwarGameStateManager` | ⛔ 禁区（`src/game.rs`） | 证据早就够了（7 条订阅本轮逐字节复核），**卡的是代码归属** |

**✅ 7 格 / ⚠️ 1 格 / ⛔ 1 格**（任务 74 之前）；**任务 74 之后 ✅ 7 格里的 3 格已落地**
（`Rank` / `Name` / `LocalPlayer` → `wired`），剩 4 格 ✅（`Soldier` / `Graphics` / `Color` / `FunctionHistoryRecorder`）。

**现值（任务 79 之后）：9 格里 8 格 `wired`，唯一剩下的 ⛔ 是 `GraphwarGameStateManager`（禁区 `src/game.rs`）。**
档位分布 **19 / 2 / 1 / 0**（任务 77 是 18/2/2/0；任务 79 把 `ComputerPlayerManager` 从 `assembled` 提到 `wired`）。

---

## 6. 下一轮可派排序（"静态可解 × 影响面 ÷ 成本"）

| 序 | manager | 为什么排这里 | 派活形态（一句话） |
|---|---|---|---|
| ~~1~~ **已完成（任务 74）** | `GraphwarRankManager` | 九格里**最便宜**：2 订阅 + 2 发布 + 1 张表 | 已落码 + **9** 条读数 ⇒ `wired` |
| ~~2~~ **已完成（任务 74）** | `GraphwarNameManager` | 结构最干净（`map +0x18`、hasher `+0x38`），`new` 地址已钉 | 已落码 + **8** 条读数 ⇒ `wired`（`fixtures/name_to_addr.tsv` 仍是 §7 C2 的登记，本轮禁改 `fixtures/`） |
| ~~3~~ **已完成（任务 74）** | `GraphwarLocalPlayerManager` | **影响面最大的一格**：50/52/54/59/66 五条"请求"的唯一发起侧 | `+0x20` = 自句柄；门读成 5 道；`outbox` + **12** 条读数 ⇒ `wired` |
| ~~4~~ **已完成（任务 77）** | `GraphwarSoldierManager` | — | `add_soldier` 内联落点 + 发 15 + 极性；**7** 条读数 ⇒ `wired` |
| ~~5~~ **已完成（任务 77）** | `GraphwarGraphicsManager` | — | 168 B 全解（0x18 头 + 3×0x30）+ 第三处重播 = 55；**9** 条读数 ⇒ `wired` |
| ~~6~~ **已完成（任务 77）** | `GraphwarColorManager` | — | G2 结案（容器弱引用）+ **8** 条读数 ⇒ `wired` |
| ~~7~~ **已完成（任务 77）** | `FunctionHistoryRecorder` | — | 33/35 裁决 + **8** 条读数（发 0 条是正面证据）⇒ `wired` |

**不派**：`GraphwarGameStateManager`（⛔ 禁区 `src/game.rs`）。**九格已全部收口**（任务 79 把最后那格
`ComputerPlayerManager` 升到 `wired`，见 §2.3）。剩下的都是**不阻塞**的小口子，登记在 §4 与各模块文档 §7。

---

## 7. 本轮附带更正（4 条，都只登记、不改别人负责的文件）

| # | 对象 | 旧说法 | 本轮实测 | 处置 |
|---|---|---|---|---|
| ~~C1~~ **已修（2026-11-09）** | `docs/00-status.md` §0.1b.2 | "4 个 manager 仍是 `assembled`（11 wired / 2 layout / **7** assembled）" | **9**（三个独立口径：判据函数 / 四常量 + 编译期闸 / `assemble_check.gd`）；且旧行自身 4≠7、11+2+7=20≠22 | 登记；下一轮改那个文件的人顺手改。**本文是文档侧唯一给出 9 的现值页**（代码侧的三处口径都在 §0.1） |
| C2 | `fixtures/name_to_addr.tsv` 的 `graphwar_name_manager / new` 行 | "歧义；候选 `0x3b6b20`, `0x3b6c70`, `0x420b40`, `0x4500c0`" | 真值 **`0x519100`**（4 个候选里 `0x4500c0` 是 `game_server_thread`，另 3 个不在本模块邻域）；判据：它同时订阅本模块的两条事件、处理器正是 `0x518f10`/`0x519030`，且被 `0x517690` 的 `0x517e63` 调用 | 登记（`fixtures/` 本轮禁改） |
| C3 | `fixtures/name_to_addr.tsv` 的 `function_history_recorder / new` 行 | "未知（不引用任何 Location）" | 真值 **`0x3f10a0`**（判据：唯一订阅 `function_info_event`、处理器 `0x3f0e40`、被 `0x517690` 的 `0x517e27` 调用） | 登记（同上） |
| C4 | `src/function_history_recorder.rs` 的 `EVENT_FIRE_TAG` 与模块文档首段 | 把 tag **35** 叫 `function_fire_event` | 字节：订阅名 `function_info_event`（`0x1bfccd`, len `0x13`，**全文唯一引用点**）+ 处理器体内判别值闸 **35**；`docs/07` §7.2 的 72 条表里 **33 = `function_fire_event`** | ✅ **任务 77 已裁并落码**：35 = `function_info_event`，常量改名 `EVENT_FUNCTION_INFO_TAG`（值不变），旧名留别名（`tests/last_five.rs` 在用）；33 无订阅者的反证见 §2.2 ④ |
| C5 | 本文 §2 第 20 行（G1） | "`+0x38`→`+0x48` 那 8 字节没有写入点"、"基址口径 `0x50` vs `0x58`" | x86 `0x519100`：`alloc 0x60`（16 + 80），`M` 基 = 盒 + `0x18`；`+0x38` 起是 **16** 字节 `RandomState`，两个 `u64` 都有写点（`0x5191a5` / `0x5191a9`） | **任务 74 就地更正**：不是空洞、也不是 8 字节；`0x60`/`0x58`/`0x50` 三个数的差别见 `src/graphwar_name_manager.rs` §7 |
| C6 | 本文 §2 第 3 行 | "`+0x20` 那个 8 字节槽语义未知"、"5 条请求的触发条件没逐条读" | `+0x20` = 自句柄（写点 `0x517965`，值 = 盒基）；5 条请求**内部无判据**，门是 5 道（第五道 `0x519fb1`→`0x51a058` 是任务 74 新发现的） | **任务 74 就地更正**；第五道门那张表登记为 G7 |
| C8 | `client/tools/assemble_check.gd` :237-238 的文案"档位 stub 的**那 1 个**" | 现值 `stub = 0` | 文案已过期（不是断言错） | **待改**（下一轮动那个文件的人顺手改；本轮不碰以免与 77 号冲突） |
| C9 | `src/graphwar_local_player_manager.rs` :165 注释把 `mov edx,0x14` 标在 `0x517a6c` | 实际在 **`0x517a6f`** | 注释级 | **待改**（同上：属 `src/`，等 77/79 号的改动落定后一并修） |

> **`docs/162` 的独立复核（2026-11-09）已经把 C1–C7 全部结案**：
> C1（"9 个"→6）、C4（LocalPlayer 第 6 条发布 tag **7 → 8**）已在 master 就地改掉；
> C2/C3/C5/C6/C7 的过期登记随 `be74cbd` 一起失效（现四处口径都是 14/2/6/0）。
| C7 | `docs/00-status.md` §0.1b.2 与 `docs/HANDOVER.md` §遗留 | 两处都写"9 个 manager 仍是 `assembled`（11 wired / 2 layout / 9 assembled / 0 stub）" | 任务 74 之后是 **14 / 2 / 6 / 0**（三格升 `wired`），见 §9 | **登记**（这两个文件不在任务 74 的允许清单里）。下一轮改它们的人顺手改：`00` 那行要把"9 个"改成"6 个"、"11/2/9/0"改成"14/2/6/0"；`HANDOVER` 同理 |
| C7 | `docs/00-status.md` §0.1b.2 与 `docs/HANDOVER.md` §遗留 | 两处都写"9 个 manager 仍是 `assembled`（11 wired / 2 layout / 9 assembled / 0 stub）" | 任务 74 之后是 **14 / 2 / 6 / 0**（三格升 `wired`），见 §9 | **登记**（这两个文件不在任务 74/77 的允许清单里）。下一轮改它们的人顺手改：`00` 那行写**现值**（任务 77 之后 = "2 个"、`18/2/2/0`）；`HANDOVER` 同理 |

---

| C8 | 本文 §2 第 16 行 / §3.5（`Graphics`） | "168 B 里三张表以外还有 **4 个字**（到 `0xa8` 之间有 `0x30` 字节）没解"、"`0xa8` 之后还有 `0x30` 字节" | **表后面没有字节**：`0xa8 = 168` 正好是 `size_of`，三张表 `+0x18/+0x48/+0x78` 各 `0x30` 铺到 `+0xa8` 就结束；没解的是**表前面**的 `0x18` 头（`+0x00` 弱自句柄 / `+0x08` 事件通道 Rc / `+0x10` 容器 Weak，建盒 `0x522710` 逐条写） | **任务 77 就地更正**（§2.2 ③ 与 `src/graphwar_graphics_manager.rs` §2.2） |
| C9 | 本文 §3.5 / §2 第 16 行（`Graphics`） | 第三处重播 tag "是间接载入（`mov rax,[rsp+0x10]`）⇒ 按模块文档是 55，**未逐位复核**" | **逐位复核完**：`0x469035 movabs rdx,0x8000…0033` + `0x46903f or rdx,4` = **55**；三支共用提交代码，tag 从 `rsp+0x10` 取（这就是"字面量扫描只看到基址"的原因） | **任务 77 就地更正**（§2.2 ③） |
| C10 | `tools/addr_probe.py tags`（**工具盲区，不是本文的错**） | 该命令自称扫"三种 materialize 形态"，于是"tag 15 只有 1 处在 soldier 模块" | **漏了第四种**：`movabs 0x8000000000000000` + `add rax,N`。这类全被记进 **tag 0**（修前 2235 处）。`add_soldier` 的 (15) 就是这么漏掉的（`0x474b8d`+`0x474b97`） | ✅ **任务 83 已修**：`_tag_after_base()` 认四种形态（`add/or/inc/lea reg,[reg+n]`，`n` 从 0 起算；`n=0` 即基址本身仍是 tag 0），正对照 4/4 全绿（42/41/44 + tag 15 形态④）。全量复扫：**tag 0 2235 → 2106（−129）**，tag 15 **45 → 46**，129 处换桶落在 33 个 tag 上；逐 tag 对照见 `fixtures/tag_counts83.tsv`，站点表 `fixtures/tag_sites83.tsv`（`tag_legacy` 列 = 修前口径）。**旧"某 tag 只有 N 处"的结论已逐条复核，详见 `docs/07` §7.1.1** |
| C11 | `src/graphwar_soldier_manager.rs` §2（旧） | "`+0x10/+0x14` 是锁字与 poison 字节（`Mutex`）" | **没有锁**：`+0x10` 是 `RefCell` 的借用字（`0` = 未借、`-1` = 写借），逐条见 `0x4555d9`/`0x474b3d`/`0x474b48` | **任务 77 就地更正**（该文件 §2） |
| C12 | 本文 §2 第 18 行 / `src/graphwar_computer_player_manager.rs` §6（旧） | Bot 表是 `HashMap<u32 /*connection_id*/, BotData>` | **key = `player_id`**：运行时 key 232 的 Bot，其 `player_info_event` 是 `(player_id=232, connection_id=228)`，228 是**房主（我）的连接**；LPM 门⑤ 比的也是 `event+0x08`（`player_id`） | **任务 79 就地更正**（模块文档 §6 + 本文 §2.3 ①） |
| C13 | 同上 | 状态在 `BotData+0x08` | 状态在桶内 `entry+0x08` = **`BotData+0x00`**（`0x479610` / `0x4786ab` / `0x479dfa` 三条指令同一口径）；`BotData+0x60`/`+0x64`/`+0x68`/`+0x6c` = level/精英/每槽/limit | **任务 79 就地更正**（模块文档 §6） |
| C14 | 同上 §7-A（旧） | "`handle_tick` 里 `state != 3` 走一条**不返回**路径（`0x5ec160`，Ghidra 标 `hasNoReturn`）" | **`0x4785f0` 体内没有 `0x5ec160`**：那条路是**收尾**（`0x4786b2` → `0x478e0e` 再判一次 → drop `0x47b5e0`/`0x404df0` → `0x47864f` 把 state 写回 3）；体内真正的 panic 是 `0x478ebb` 的串 **`"Failed to join computer player thread."`**（`0x478eda` 调 panic、`0x478ee0 ud2`）。`addr_probe.py callers 0x5ec160` 也不在 `0x4785f0` 体内 | **任务 79 就地更正**（模块文档 §7-A） |

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

## 9.1 任务 77 的验收数字（本人重跑；同一条分支的私有 target）

| 项 | task 74 后 | task 77 后 |
|---|---|---|
| 档位分布 | 14 / 2 / 6 / 0 | **18 / 2 / 2 / 0** |
| `cargo test --offline` | 1710 | **1730**（+20：新增内联测试 10 条 × 两个编译单元——`tests/graphwar_{color,soldier,graphics}_manager.rs` 也 `#[path]` 编了同一份文件） |
| `cargo test --offline --features gdext-bindings` | 1816 | **1836**（同上；`root_wiring` 的 22 槽台账与 135 条探针台账都在这一侧） |
| `client/tools/assemble_check.gd` | 226 / 0 | **238 / 0**（+12：四格各 3 条，分别指 订阅 / 发布 / 状态） |
| headless 14 个脚本合计 | 1827 | **1839**（其余 13 个逐项不变：24 / 96 / 116 / 132 / 235 / 96 / 366 / 117 / 155 / 23 / 135 / 49 / 57） |
| `selftest` | 714 / 0 / 1 skip | **714 / 0 / 1 skip**（**只跑了一次**，一次就过；本轮的 flake 未复现） |
| 三轴 `fire` / `batch` / `batch_conv` | 647 / 647 / 637 | **647 / 647 / 637**（不变；文件 286 / 读失败 117 / 结算批 1080 也没变） |
| coverage 两份生成物 | — | 重跑 `tools/coverage_matrix.py`：`docs/coverage-matrix.tsv` + `docs/149-coverage-matrix.md`（527 可归属 / 527 已实现；逐位 88 / 观测 439） |

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

## 9.3 任务 79 的验收数字（本人重跑；同一条分支的私有 target）

| 项 | task 77 后 | task 79 后 |
|---|---|---|
| 档位分布 | 18 / 2 / 2 / 0 | **19 / 2 / 1 / 0** |
| `cargo test --offline` | 1730 | **1736**（+6：LPM 3 条门测试 + CPM 1 条状态机测试，× 两个编译单元） |
| `cargo test --offline --features gdext-bindings` | 1836 | **1842**（同上） |
| `client/tools/assemble_check.gd` | 238 / 0 | **241 / 0**（+3：Computer 的 订阅 / 发布 / 状态） |
| 交互读数（`manager_probe_selftest`） | 135 条 / 18 个 manager | **145 条 / 19 个 manager**（+10 条 `computer.*`） |
| headless 14 个脚本合计 | 1839 | **1842**（其余 13 个逐项不变：24 / 117 / 96 / 116 / 132 / 235 / 96 / 366 / 155 / 23 / 135 / 49 / 57） |
| `selftest` | 714 / 0 / 1 skip | **714 / 0 / 1 skip**（不变；⚠️ 必须 `HOME` 指到工作区、回放样本放对 `user://`、`examples/serve_match` 编好，否则会多出 2 条 SKIP） |
| 三轴 `fire` / `batch` / `batch_conv` | 647 / 647 / 637 | **647 / 647 / 637**（不变） |
| coverage 两份生成物 | 527 / 527 | **527 / 527**（`src_fns` 3009 → 3016，其余不变） |
