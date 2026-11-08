# 157 · `docs/02` 的名字匹配三表过程记录（§34–§40，原样搬迁）

> **出处**：摘自 [`docs/02-module-map.md`](../02-module-map.md) §34–§40，2026-11-08 搬迁，**内容未改**。
> 搬迁范围 = 该文件搬迁前（`2e69ed5e`）的**第 2800–4668 行**（§34–§40，7 节，1,869 行），逐字复制；
> 只在本文件开头加了这一段出处说明，**其余一个字节都没动，节号也不变**（archive 里仍叫 §34–§40）。
> 自证：`git show 2e69ed5e:docs/02-module-map.md | sed -n '2800,4668p'` 与本文件从下面
> 第一条 `## 34.` 起的全部内容 `cmp` 相同（命令与 md5 见 `docs/154-docs-audit.md` 附录）。
>
> **怎么读**：结论表不在本文 —— 名字 ↔ 地址的索引在 [`docs/02`](../02-module-map.md) §20/§21，
> 进度分母在 [`docs/149-coverage-matrix.md`](../149-coverage-matrix.md)（生成物）。
> 本文是**过程与逐函数证据**（怎么把 325 提到 351、179 个无名函数怎么分的、哪些还定不了）。
> 本文里 `docs/02 §NN` 一律指**本文自己的 §NN**（同一批文字，只是搬了家）。
> `§23–§33` 本来就是空号（原 §34 有说明：那是并行任务的预分配号），不是丢步。

---

## 34. `weak_caller.rs` 全解：包一个 `Weak<RefCell<T>>` 的 6 行工具，**39 个单态化 = 2 个泛型方法 × 26 个类型**（2026-11）

> 编号说明：本节编号按本轮并行任务的预分配取 **34**（23–33 留给同时落地的其它模块），
> 文件当前到 §22 为止，中间是**空号不是丢步**。
>
> 本节对应 `src/weak_caller.rs` + `tests/weak_caller.rs`（17 条）。
> 按 `AGENTS.md` §0.2，那 17 条是**回归保护**；本节给出的证据是 §34.7 的复算命令与
> 两架构的**交叉相等**（§34.5），不是 `cargo test`。

### 34.1 一句话结论 + 四问四答

`graphwarrust::weak_caller::WeakCaller<T>` 是 **crate 内模块**（旧文档判「第三方依赖」，
已由 §18.7 推翻）：一个 8 字节的 `Weak<RefCell<T>>` 包装，两个方法把
「升级 → 可变借出 → 调一个函数 → 放掉」压成一次调用。**它不是回调派发器**
（无注册表 / 无 id / 无队列 / 无返回值），而是每个 manager 的**弱自句柄**。

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 数据结构** | `Weak<RefCell<T>>`，**一个**类型参数，整型 8 字节；`Weak` 的目标是**它自己**那个 manager。不是 `Mutex`、不是 `Rc<RefCell<_>>`、**不是 `dyn Trait`**（`&mut T` 是瘦指针，只占一个整数寄存器） | **观测** |
| **(b) 升级失败** | **静默跳过**：`upgrade() == None` 就什么都不做、返回 `()`。不 panic、不打日志、没有返回值可查。唯一的 panic 是 **`RefCell` 已被可变借出**（`RefCell already borrowed`）；`strong` 溢出是 `abort` | **观测** |
| **(c) 是不是同一个泛型方法** | **不是**。是 **`call_0<F>` 与 `call<A, F>` 两个**泛型方法，`F` 也泛型。x86 目标 **39 = 23 + 16** 个外联单态化；arm64 2.0.60 **35 = 21 + 14** | **观测** |
| **(d) 谁在用** | **26 个 manager / control 自己**（`GraphwarPlanePosManager` … `GraphwarModeManager`）。`new` 里用 `Rc::new_cyclic` 造弱句柄，`subscribe_to_events` / `new` 里的闭包克隆它并把 `T::方法` 交给 `call` / `call_0` | **观测** |

### 34.2 数据结构与指令骨架（arm64 2.0.60 `0x3d519c`，逐指令）

```text
  cmn   x0, #1                     ; x0 == -1 ⇒ Weak::new() 的哨兵 usize::MAX ⇒ 跳过
  ldr   x9, [x0]                   ; RcBox +0x00 = strong
  cbz   x9, <尾部>                 ; strong == 0 ⇒ upgrade() 给 None ⇒ 跳过
  adds  x9, x9, #1 ; str x9,[x0]   ; strong += 1（b.hs ⇒ brk，溢出 abort）
  ldr   x9, [x0, #0x10]            ; RcBox +0x10 = RefCell 借用标志
  cbnz  x9, <panic_already_borrowed>
  mov   x9, #-1 ; str x9,[x0,#0x10]; 标志 = WRITING(-1)
  add   x0, x0, #0x18              ; &mut *guard == RcBox+0x18 == &mut T
  mov   x8, x1 ; mov x1, x2 ; blr x8   ; f(&mut T, a)     ← call_0 是 blr x1
  ldr   x8,[x0,#0x10] ; add 1 ; str    ; 释放借用（-1+1=0）
  ldr   x8,[x19]      ; sub 1 ; str    ; Rc 强计数 -1（归零 ⇒ Rc::drop_slow）
  ldr   x8,[x19,#8]   ; sub 1 ; str    ; **self 的** Weak 弱计数 -1（归零 ⇒ dealloc）
```

由此钉死：`RcBox<RefCell<T>>` = `{strong@0x00, weak@0x08, 借用标志@0x10, T@0x18}`；
`size_of::<RcBox<RefCell<T>>>() = 24 + size_of::<T>()`（39 处内联 `__rust_dealloc` 的
`size` 常量），`align` **39 处全是 8**（⇒ 26 个类型的对齐都 ≤ 8）。

**`self` 按值消费**是一条硬事实、不是风格选择：调用点只把指针装进 `x0` / `%rdi`
（**不取地址**），被调函数尾部**自己**把弱计数减一并可能 `dealloc`。
所以调用方必须先 `clone()` —— 每个事件闭包开头那三行
（`ldr x8,[x0,#8]; adds 1; str` / x86 `incq 0x8(%rbx)`）就是 `Weak::clone`。

### 34.3 `borrow_mut` 而不是 `borrow`：两条独立证据

1. 冲突时调的是 `core::cell::panic_already_borrowed`（arm64 `0x836c70`，x86 GOT 槽
   `0x9ba6d0`）。std 里 `panic_already_borrowed` 只被 `borrow_mut` 用，消息是
   `RefCell already borrowed`；`borrow()` 走的是 `panic_already_mutably_borrowed`，
   消息 `RefCell already mutably borrowed`。两条消息在 arm64 2.0.60 的 `.rodata` 里**都在**。
2. 标志位：写入 `-1`、释放 `+1`。`BorrowFlag::WRITING == -1` 是 `borrow_mut` 的写法
   （`borrow()` 是 `+1` 计数）。

### 34.4 矩阵：`Method` × `T`（26 个类型）

`size_of(T)` 由 `dealloc` 布局常量减 24 得到；x86 地址是目标版（stripped）、arm64 地址是
2.0.60（未 strip）。最后一列是**调用点实际交出去的 `f`** —— 它就是「谁在用它」的签名级证据，
也是这 26 个类型名本身的来源。

| `T` | `size_of(T)` | x86 `call` | x86 `call_0` | arm64 `call` | arm64 `call_0` | 观测到的 `f`（`T::*`） |
|---|---|---|---|---|---|---|
| `FunctionHistoryRecorder` | 48 | `0x3d2520` | — | `0x3d6854` | — | handle_function_fire_event |
| `GraphwarAudioControl` | 216 | `0x3d1df0` | — | `0x3d5ba0` | — | effect_info_event / game_state_event |
| `GraphwarClientConnectionHandler` | 112 | `0x3d1670` | `0x3d3250` | `0x3d53ec` | `0x3d6dfc` | forward_event / handle_connected_to_server / tick |
| `GraphwarColorManager` | 120 | `0x3d12c0` | `0x3d2c30` | `0x3d63b0` | `0x3d781c` | handle_entity_removed_event / send_all_info_events |
| `GraphwarComputerPlayerManager` | 72 | `0x3d1850` | — | `0x3d6600` | — | handle_entity_removed_event / handle_tick / handle_turn_info_event |
| `GraphwarEffectManager` | 72 | — | `0x3d2fb0` | — | `0x3d697c` | tick |
| `GraphwarEntityEffectManager` | 72 | `0x3d2340` | `0x3d2df0` | — | — | subscribe_to_events_event_handler |
| `GraphwarGameStateManager` | 296 | `0x3d1a30` | — | `0x3d5df0` | — | handle_chat_message / handle_connection_removed_event / handle_entity_removed_event / handle_function_points / handle_graphics_info / handle_new_connection_event |
| `GraphwarGraphicsManager` | 168 | `0x3d1fd0` | `0x3d28b0` | `0x3d5950` | `0x3d6bbc` | handle_entity_removed_event / send_all_info_events |
| `GraphwarLifeManager` | 72 | `0x3d1940` | `0x3d2610` | `0x3d5f18` | `0x3d75dc` | handle_entity_removed_event / handle_tick_event / send_all_info_events |
| `GraphwarLobbyControl` | 96 | — | `0x3d2b50` | — | `0x3d6cdc` | （闭包被内联，`f` 无符号） |
| `GraphwarLocalPlayerManager` | 64 | `0x3d1490` | — | `0x3d563c` | — | handle_connected_to_server / handle_entity_removed_event / handle_player_info |
| `GraphwarModeManager` | 32 | — | `0x3d2990` | — | `0x3d76fc` | send_all_info_events |
| `GraphwarNameManager` | 72 | `0x3d1580` | `0x3d2d10` | `0x3d64d8` | `0x3d74bc` | handle_entity_removed_event / send_all_info_events |
| `GraphwarObstacleManager` | 80 | `0x3d20c0` | — | `0x3d5cc8` | — | handle_tick_event |
| `GraphwarPlanePosManager` | 24 | `0x3d1760` | `0x3d3090` | `0x3d519c` | `0x3d703c` | handle_entity_removed_event / send_all_info_events |
| `GraphwarPlayScreenControl` | 144 | `0x3d22a0` | — | `0x3d588c` | — | （闭包被内联，`f` 无符号） |
| `GraphwarPlayerManager` | 72 | `0x3d2430` | `0x3d26f0` | `0x3d52c4` | `0x3d727c` | handle_entity_removed_event / send_all_info_events |
| `GraphwarPlayerScreenControl` | 32 | `0x3d10e0` | — | `0x3d6728` | — | face_button_pressed / hat_button_pressed / skin_button_pressed |
| `GraphwarRankManager` | 72 | `0x3d1b20` | `0x3d27d0` | `0x3d5514` | `0x3d6f1c` | handle_entity_removed_event / send_all_info_events |
| `GraphwarServerConnectionHandler` | 128 | `0x3d21b0` | `0x3d2a70` | `0x3d6040` | `0x3d6a9c` | forward_connection_specific_event / forward_event / handle_ban_connection_request_event / handle_clear_bans_request_event / handle_connection_removed_event / tick |
| `GraphwarSoldierManager` | 72 | `0x3d1ee0` | `0x3d3170` | `0x3d5a78` | `0x3d715c` | handle_entity_removed_event / send_all_info_events |
| `GraphwarTargetManager` | 72 | `0x3d11d0` | `0x3d3330` | — | — | subscribe_to_events_event_handler |
| `GraphwarTeamManager` | 32 | `0x3d1d00` | — | `0x3d6168` | — | handle_entity_removed_event / send_all_info_events |
| `GraphwarTickHandler` | 72 | `0x3d13b0` | — | `0x3d6290` | — | handle_connected_to_server_event |
| `GraphwarTurnManager` | 32 | `0x3d1c10` | `0x3d2ed0` | `0x3d5764` | `0x3d739c` | handle_entity_removed_event / send_all_info_events |

* `call_0` 的 14 个 arm64 单态化里只有 **3 个**是 `call` 没有的新类型
  （`GraphwarEffectManager` / `GraphwarLobbyControl` / `GraphwarModeManager`），
  其余 11 个类型两个方法都用到 ⇒ **13 个类型两个都有、10 个只有 `call`、3 个只有 `call_0`**。
* `call` 与 `call_0` 的分界是**被调函数的元数**，不是名字、也不是类：
  `handle_entity_removed_event` 出现在 **13 个**类型的 `call` 里；
  而 `send_all_info_events` **两种都有** —— 10 个类型上它不带载荷（`call_0`），
  **`GraphwarTeamManager` 上它带一个载荷**（`call`，`0x3d1d00` 那一行同时列了
  `handle_entity_removed_event`）。同一个方法名两边出现，直接否掉「按语义选口」的猜测。
* `GraphwarPlayerScreenControl` 那一行搬了**两个**整数寄存器（16 字节载荷）：
  `face_button_pressed` / `skin_button_pressed` / `hat_button_pressed`。
* `GraphwarPlayScreenControl`（`0x3d22a0`，160 字节；arm64 孪生 `0x3d588c`，196 字节）
  是个**形状例外**：里面**没有间接调用** —— 闭包体
  （`movb $0x1, 0xa3(%rdi)` / `strb w9,[x0,#0xa3]`，即 `t.<+0x8b> = true`）被内联了进去。
  这一条同时证明 **`F` 是泛型参数而不是写死的 `fn` 指针**（`fn` 指针不可能被内联）。
* 反过来，同一个 `(T, A)` 的多个调用点**共用 1 个**单态化
  （`GraphwarGameStateManager` 7 个调用点、`GraphwarServerConnectionHandler` 5 个），
  说明那些点把 `fn` 项**强制成同一个 `fn` 指针类型**才传进来。

### 34.5 谁在用它（d）＋ 两架构交叉相等

句柄指向 `T` 自己，不是容器。两条证据：

* 回调收到 `&mut T`，而 `f` 一律是 **`T` 自己的方法**（上表最后一列，每一项都以该行 `T` 开头）；
* `T` 的构造函数走 **`Rc::new_cyclic`**（arm64 2.0.60 有 21 个
  `alloc::rc::Rc::<..>::new_cyclic_in` 实例），例：`GraphwarPlanePosManager::new`
  在 `0x3b89bc` 就是 `bl Rc::new_cyclic_in`。这是「先有 `Rc`、再在闭包里拿到 `Weak`
  装进结构体」的唯一标准写法。

**交叉相等（本节最强的一条，`观测`）**：同一个 `T` 在两个架构上的
`size_of::<T>()` **逐一相等** —— 35 个 arm64 单态化的内联 `dealloc` 常量
`== 24 + size_of(T)(由 x86 侧读出)`，**零个例外**
（`tests/weak_caller.rs::arm64_rcb_size_matches_the_other_arch`）。
这一条同时验证了「`RcBox` 头部 24 字节」「类型 → 尺寸映射」「39/35 个地址归属」三件事。

### 34.6 版本差：x86 26 个类型，arm64 24 个 —— 是**内联**差，不是源码差

| | `call` 外联 | `call_0` 外联 | 被内联掉的 |
|---|---|---|---|
| x86 2026-08-28（目标） | 23 | 16 | `GraphwarGameRecorder`（`0x3d4fa0`，704 字节） |
| arm64 2.0.60 | 21 | 14 | `GraphwarTargetManager`、`GraphwarEntityEffectManager`、`GraphwarGameRecorder` |

`GraphwarTargetManager` / `GraphwarEntityEffectManager` 的
`subscribe_to_events_event_handler::{{closure}}` 在 arm64 上分别是
`0x3da9bc`（**740** 字节）与 `0x3e9c58`（**744** 字节），而其它 manager 的同类闭包只有
~60 字节 —— 差的正是 `call` 被**整个内联**进去的体积。所以两边是同一份源码，
只是内联决策不同；**26 个类型是源码事实，39 / 35 是构建产物事实**。

### 34.7 两个陷阱（都是本轮踩到的）

1. **`drop_slow` 的同一地址 ≠ 同一个类型。** `Rc::<RefCell<T>>::drop_slow` 按 `T`
   单态化，但**代码完全相同的实例被链接器 ICF 合并**：arm64 `0x41a668` 上同时挂着
   `drop_slow17h{224f049b,a03b81ec,75485ae1}…` **三个** `FUNC` 符号
   （`PlayerManager` / `RankManager` / `SoldierManager`，三者 `size_of == 72`、字段布局相同）。
   所以「按 `drop_slow` 地址分组 = 按类型分组」**是错的**；本轮改按
   **调用点交出的 `f`** 与 **`dealloc` 尺寸**双重定名。
2. **`fixtures/module_map.tsv` 的 40 与进度尺的 39 差一个。** `0x3d4fa0`（704 字节）
   引用了 `src/weak_caller.rs:17:28`，但它**不是**单态化：它自己的 panic 锚在
   `src/graphwar_game_recorder.rs:107:105` / `108:41`，是 `GraphwarGameRecorder`
   的方法内联了 `call` 之后的产物。⇒ **按「锚」数出来的 40 要减掉它才是单态化数**
   （`docs/02` §20 / §21 里那两张按锚统计的表，遇到本模块要按本条读）。

### 34.8 两个 `panic::Location` = 源码行号（**推断**）

本模块**只有 2 个** panic 点，都是 `borrow_mut` 的 `#[track_caller]`：

| 方法 | 锚 | x86 `Location` 结构体 | arm64 |
|---|---|---|---|
| `call_0` | `src/weak_caller.rs:11:28` | `0x9531f8` | `0x84c7c8` |
| `call` | `src/weak_caller.rs:17:28` | `0x9531e0` | `0x84c7b0` |

`track_caller` 记的列号是**方法名**那一列（本机 rustc 1.99 实测：
`let b = c.borrow_mut();` 报 `5:15`，即 `borrow_mut` 的 `b`）。由此（**推断**）：

* 两行列号**相同**（28）⇒ 两个方法体形状与缩进相同；
* `17 − 11 = 6` ⇒ 每个方法 6 行、两方法之间**可以不留空行**，
  于是 `impl` 之前只有 ~8 行 ⇒ **原版这个文件几乎没有模块级文档**，
  最可能是「一行 `pub struct WeakCaller<T>(pub Weak<RefCell<T>>);` + 两个 6 行方法」；
* 列 28 只钉死 `borrow_mut` 那一行**接收者表达式的长度**（缩进 12 + 6 字符接收者，
  或缩进 8 + 10 字符接收者），**不钉死标识符**。
  `tests/weak_caller.rs::anchor_arithmetic_admits_a_six_line_body` 用一个最小模板
  把这条算术钉住了，模板正好落在 `(11,28)` / `(17,28)`。

⚠️ 我们的 `src/weak_caller.rs` **不复现**这两个行号（它自带写结论用的 `//!` 文档）。

### 34.9 就地更正（旧文档里被本节推翻或需改口径的几条）

按 `AGENTS.md` §0.6「文档不许虚报」：旧文不删，在这里指明该按哪条读。

| 旧说法（出处） | 本节结论 | 证据 |
|---|---|---|
| 「`weak_caller.rs`（`dep`，39 个函数）是第三方依赖库，不算游戏代码」（§17 表、`docs/149`、`docs/coverage-matrix.tsv` 的 `scope=dep`） | 是 **crate 内模块**（`graphwarrust::weak_caller::…`），且**不是** 39 个函数的「库」，是 2 个泛型方法的 39 个单态化 | §18.7 + §34.4 |
| 「`weak_caller` 是 **1** 个泛型方法 `call`（27 个 direct 符号）」 | **2** 个方法 `call` / `call_0`；`call` 本身再按 `(T, A, F)` 单态化。27 这个数是某次按 `call` 名字前缀数出来的，漏了 `call_0` 的 14~16 个 | §34.4 的两块地址区间 |
| 「`weak_caller.rs` 有 **40** 个归属函数」（`fixtures/module_map.tsv`、`docs/16-client.md` §24 的「13 个模块没有 fixture 兜底」表（该表把本模块记成 `gdext 侧模块`，同样要改）、`docs/149`） | **39** 个单态化 + **1** 个内联了 `call` 的调用者（`0x3d4fa0`，属 `graphwar_game_recorder.rs`） | §34.7 第 2 条 |
| 「`weak_caller.rs` 有 **37** 个函数（2024-07-30 符号表）」（§18 A 表） | 那一版确实是 37（22 `call` + 15 `call_0`）；**2.0.60 是 35**（21 + 14）——两年里少了一型的两条 | `readelf -sW` 逐条数 |
| 「`weak_caller` 的闭包落在 `weak_caller.rs:11–17`，所以它是 gdext 绑定胶水」（§18 表注） | 11–17 是**两个 `borrow_mut`** 的行号，不是「胶水」的证据；它是**弱自句柄工具**，与 gdext 无关（不进 `__register_methods` / `__virtual_call` 任何一条链） | §34.2 / §34.8 |

### 34.10 复算命令

```text
# 1) arm64 2.0.60（未 strip）：2 个方法名 + 35 个单态化
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
  | grep weak_caller | grep FUNC | wc -l                       # 35
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
  | grep -c 'WeakCaller4call1'                                  # 21   （call）
readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
  | grep -c 'WeakCaller6call_01'                                # 14   （call_0）

# 2) 每个单态化内联的 dealloc 布局常量 = 24 + size_of(T)
llvm-objdump -d --start-address=0x3d519c --stop-address=0x3d52c4 <arm64 so>

# 3) x86 目标（stripped）：两个 Location 锚把 40 个函数分成 16 + 24
.venv/bin/python tools/loc_xrefs.py loc src/weak_caller.rs:11:28   # 16 个 -> call_0
.venv/bin/python tools/loc_xrefs.py loc src/weak_caller.rs:17:28   # 24 个 -> 23 call + 1 内联调用者

# 4) 类型名：找每个 call 的调用者，看它把哪个 fn 项交给 f
#    arm64 = adr x1, <T>::method ；x86 = lea rsi,[rip+..]
#    再把那个地址查 fixtures/name_to_addr.tsv
```

### 34.11 还缺什么（`未知`）

* **字段名**：只有布局与类型证据，没有字段名证据。
* **`WeakCaller` 的构造函数**：被内联进 21 个 `new_cyclic` 闭包，符号表里**没有**
  `WeakCaller::new` / `from` / `Clone` 的符号，所以构造写法（`WeakCaller(w)` 还是
  `w.into()` 还是 `WeakCaller::new(w)`）**未知**。`src/` 里给的是便利口，已标注。
* **`F` 的上界**：观测只能说明它是泛型且被当 `FnOnce(&mut T[, A])` 调用；
  是否写了 `move`、是否 `+ 'static`，**未知**。
* **`T: ?Sized`**：26 个类型全是 `Sized`，没有 `dyn` 实例，所以**无法判断**
  原版有没有写 `?Sized`（写了也不会产生不同的单态化）。
* **`GraphwarTeamManager::send_all_info_events` 为什么带载荷**（其它 10 个同类不带）：
  载荷类型未定，`未知`。

## 35. 跨架构函数匹配：没有 Location 锚的那 970 个怎么对（2026-11）

> 编号说明：本节按本轮并行任务的预分配取 **35**（23–33 留给同时落地的其它模块），
> 文件当前到 §22 / §34 为止，中间是**空号不是丢步**。
>
> 工具：`tools/sym_match.py`（`build` / `report` / `--verify`）；
> 产物：`fixtures/sym_match.tsv`（1,300 行，已入库，可逐行复核）；
> 回归断言：`tests/sym_match.rs`（读 fixture 做一致性断言，**不是复原证据**，见 `AGENTS.md` §0.2）。

### 35.1 一句话结论 + 四问四答

`tools/name_bridge.py` 用 `panic::Location` 的 `(file, line, col)` 把 **326 行**唯一锚钉死；
但它只能覆盖"代码里留了 Location 常量"的函数。**剩下约 970 个两边都有函数体、没有任何
Location 锚**。本节的工具**完全不读 Location、完全不读 `name_to_addr.tsv`**，只从两份 `.so`
的字节里取结构证据，独立地把它们对到 x86 地址上。

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 一致性** | 299 个唯一锚 key 里 282 个进分母（17 个在 arm64 上有多份符号、真值本身不唯一），**独立复现为"唯一且地址相同"222 条 = 78.7%**；真值出现在候选集里 **251 = 89.0%**；给出唯一候选的 234 条里正确的 222 条 **= 94.9%**。**没到 90% 门槛**，原因见 §35.5 | **观测** |
| **(b) 覆盖率** | 1,300 行里：唯一匹配 **733**（`unique` 655 + `weak` 78）/ 多候选 **63** / 无候选 **444** / 一拆多 **35** / 多合一 **25**。除以 1,300 个符号，**落地率 61.0%**（793/1300） | **观测** |
| **(c) 一拆多 / 多合一** | `split` **35** 例、`merge` **25** 例。**`calculate_paths` 不在一拆多里**——它是干净的一对一，见 §35.6 | **观测** |
| **(d) 永远对不上的** | ①**被 ICF 合并掉的**：`GraphwarTargetManager::new` 与 `GraphwarPlayerManager::new` 在 arm64 是两份 396 字节 / 99 条指令**逐条同构**的函数，x86 只留了一份，真值只能二选一；②**极小的转发/析构 shim**（`drop_in_place` 一行、8–20 字节）；③**纯数据/跳转表**（`adrp`+`ldr` 取值就返回）；④**名存实亡的 `{{closure}}`**：x86 侧被内联进母函数后不再有独立函数 | **推断**（①有逐条同构的观测支持；②③④是"这类结构本身没有可比特征"的论证） |

### 35.2 方法：五类证据，权重按实测排序

`build` 的输出字段就按这个顺序写：

| 证据 | 取法 | 实测作用 |
|---|---|---|
| **`order` 函数顺序** | Rust 按 CGU/模块排函数：同一模块块内两侧**索引同步平移**。把已配对的 `(arm索引 − x86索引)` 分组 = 一条"对角线"，簇内整段直接配出、簇两端延伸到体量比出闸为止；簇与簇之间的空档用**仿射空档 DP** 对齐（一次 gap = 一次内联事件） | **主力**。269 对真值里 **226/285 相邻对的索引差完全相同** |
| **`lit` 字面量** | `&'static str` 的 16 字节 `(ptr,len)` 槽——`.rela.dyn` 的 `R_*_RELATIVE` addend 才是真指针（ELF 里那格是 0）。解析出来就是**精确**文本，不受 `.rodata` 合并串表邻接影响；直接指向 `.rodata` 的只能当 ≥16 字节前缀用 | 独占字面量种子 1,142 条，**真值上 73/75 命中** |
| **`f64` / 稀有整数常量** | `movabs` 的 imm64、`.rodata` 里的 f64、`movz/movk` 的整数；只取两侧都罕见的 | 20 + 2 条种子 |
| **`call` 直接调用图** | 谁调谁（重复算重数），互选式投票 | **贡献 0**，原因见 §35.3 |
| **`size` / `ninsn` 闸门** | 体量比只当闸门（0.40–2.20）；**指令条数比 0.70–1.45 只进 DP 打分** | 见 §35.4 |

### 35.3 为什么"沿调用图做 WL 指纹"几乎没用（实测，不是没试）

本节值得单独写，因为按可靠度它本来排第一。实测加进去 **0 条**。

拓扑在这里**不是一对一**的：一个被几十个函数调用的工具函数（`drop_in_place`、
`RawGd::check_rtti`、`memcpy` 包装……）只对应一个 x86 目标。它一旦被定下来，所有**还没定下来**
的调用者的投票全部指向同一个已被占用的槽——按"不许抢"的规则只能丢掉。于是"从已定邻居投票"
在收敛到一半时就自动失效，剩下能投的只有冷门被调者。

改成**互选**（arm 侧选 x86、x86 侧也选 arm，两边都要对方是自己的一选）也没救回来：
被过滤掉的量太大（实测 5,677 个未定 arm 函数有 ≥1 个已定被调者，过滤后剩下 0 个候选）。
真正携带信息的是**顺序**，不是拓扑。这条写下来，免得下一个 agent 再走一遍。

### 35.4 体量比：不是 0.61–1.19，是 0.61–2.20

用 307 对 usable 真值量出来的分布（`arm64 尺寸 / x86 尺寸`）：

```
min 0.01  p10 0.80  p25 0.91  median 1.00  p75 1.11  p90 1.29  max 50.86
落在 0.61–1.19: 252/307 = 82.1%
落在 0.45–1.80: 287/307 = 93.5%
落在 0.40–2.20: 296/307 = 96.4%
```

**指令条数比更紧**：269 对的 p10/p90 = 0.83/1.17，89% 落在 0.80–1.25。因为 arm64 是定长
4 字节、x86 平均 ~3.5 字节，字节数会被指令宽度差放大，而"编出来几条指令"几乎相同。
所以：**闸门用字节（宽），排序/DP 打分用指令数（紧）**。把两个都当闸门反而更差
（实测 78.7% → 58.5%，因为闸一紧整段对角线就断）。

### 35.5 没到 90% 的 60 条，逐类归因（这就是本节最该被复核的部分）

`--verify` 的分母是 282，正确 222，剩 60：

| 类别 | 条数 | 原因 |
|---|---|---|
| **候选含真值、但没定成唯一** | **29** | 字面量/常量证据把 3–30 个 x86 函数都列为候选。按"绝不允许猜"的规则**没有挑**。这是本工具的主要损失来源：宁可列出来也不赌 |
| **唯一候选是错的** | **16** | ①ICF：两侧函数在 x86 被合并成一份，另一份 arm64 副本抢走了槽（`GraphwarTargetManager::new` 就是）；②`{{closure}}` 与母函数互换（x86 把闭包内联进母函数，arm64 留成两个函数，DP 把母函数配给了闭包） |
| **名字键没对上** | **13** | 这 13 个 key 在 arm64 `.symtab` 里没有独立符号（被 ICF/内联吃掉了），fixture 里根本没有这一行 |
| **无候选** | **2** | — |

结论：**78.7% 是"唯一且正确"的严格口径**。同一份 fixture 的召回是 **89.0%**、精确是 94.9%。
三个口径都不换判据，只是看不同的行；不达标的那 17.6 个百分点里，**29/60 是"拒绝猜"的代价**，
**16/60 是内联/ICF 造成的真歧义**。要冲 90% 只有一条路：把"多候选"强行收窄——那正好违反本工具的硬规则。

### 35.6 已知案例：`calculate_paths` **不是**一拆多（纠正）

任务书里的校准例子说 `calculate_paths` 在 arm64 是一个 4,812 字节函数（采样器被内联进去）、
在 x86 是两个（`0x467530` + 采样器 `0x3c2530`）。**实测不是这样**：

| 函数 | arm64 | x86 | 体量比 | 证据 |
|---|---|---|---|---|
| `GraphwarPathCalculator::calculate_paths` | `0x3dca1c` 4,812 B | **`0x467530`** 5,408 B | 0.89 | `lit=1/2;call 一致`；与 `name_to_addr.tsv` 的 **9 个 Location 锚**同址 |
| `GraphwarShotCalculator::calculate_function`（**采样器本体**） | `0x3b5374` 1,020 B | **`0x3c2530`** 1,680 B | 0.61 | `f64=1/1;call 一致`——两侧**都**留了独立的采样器函数 |

也就是说两侧的内联决策在这一点上**是一样的**，不存在"一拆二"。`0x3c2530` 的调用者数为 **0**
（它经函数指针被调用），这也解释了为什么它既拿不到 Location 锚、也不在调用图上——但它
**有** arm64 的独立副本，所以能被 f64 + 调用图对出来。

同模块另外两条也对上了，且与真值同址：
`calculate_shot_results` = `0x3b5770`→`0x3c2bc0`（真值 0x3c2bc0 ✅）、
`calculate_corrected_function` = `0x3b79a8`→`0x3c5c50`（无真值，`via=call+order`）、
`calculate_function_offset_str` = `0x3b7c70`→`0x3c5f70`（无真值）。

> ⚠️ `AGENTS.md`「Hard facts」把 `0x3c2530` 叫 sampler、`0x3c5f70` 叫 correction。
> 本节给出的名字是 **`.symtab` 权威名字**经结构匹配落到 x86 的结果：`0x3c2530` =
> `calculate_function`、`0x3c5f70` = `calculate_function_offset_str`。两者不冲突（前者是项目
> 早期按行为起的别名），但引用时以本表的符号名为准。

### 35.7 一拆多 / 多合一：怎么判、判出多少

判据是**地址相邻 + 特征被包含 + 体量不超过对方**（不是"DP 空档"本身——一个长空档只说明
这一段对不上，不说明 20 个函数都被内联进了同一个母函数，所以空档长度 > 3 时不下结论）：

* `split`（1 arm64 ↔ N x86）：arm64 把某个 x86 函数内联掉了，x86 侧那个函数在地址上紧挨着
  已配对的 x86 函数，且它的字面量/常量集合被 arm64 那个函数的集合**包含**。
  **35 例**。例：`graphwar_version::graphwar_version_get_full_version`（8 字节）被内联进 `0x4166b0`。
* `merge`（N arm64 ↔ 1 x86）：反过来，arm64 侧多出一个函数，x86 把它内联进母函数。
  **25 例**。例：`drop_in_place<graphwarrust::button_manager::…>`（`0x3b07ec`）`size=0.12`
  被内联进 `0x3b7460`。

### 35.8 复算命令

```bash
# 1) 生成（首次 1–2 分钟：要反汇编 12,609 + 13,263 个函数；cache 命中后 1 秒）
../.venv/bin/python tools/sym_match.py build

# 2) 统计：唯一 / 多候选 / 无候选 / 一拆多 / 多合一
python3 tools/sym_match.py report

# 3) 自校验：拿 326 行唯一锚当 ground truth，退出码 0/1（90% 门槛）
python3 tools/sym_match.py --verify ; echo "exit=$?"

# 4) 单条复核：fixture 里一行 = 一个 arm64 符号，evidence 栏写清靠哪几类证据
awk -F'\t' '$1 ~ /calculate_paths/' fixtures/sym_match.tsv
```

`build` 需要 capstone（走仓库 venv）；`report` / `--verify` 只读 TSV，**标准库即可**。
特征提取结果缓存在 `.agent-work/symmatch/cache/`（已 gitignore），按 mtime+size 失效。

### 35.9 还缺什么（`未知`）

* **多候选的 63 行没有裁决**：29 条其实是真值所在的那些。要收窄只能引入新证据
  （例如把 `graphwar_module_container` 的 `TypeId` 哈希顺序、或 `.data.rel.ro` 的
  vtable 槽顺序拿来当"模块内相对位置"的独立约束），本轮没做。
* **`ninsn` 闸门没有进闸**：只进了 DP 打分。进闸会掉 20 个百分点（§35.4），
  为什么只有这么多解释、没有机理证明，`推断`。
* **ICF 的判定是"逐条同构"的手工对齐**，没有做成工具规则：`_contained` 只比字面量/常量集合
  与体量，不比指令序列。真正的 ICF 判定要逐指令归一化后比，本轮没做。

---

## 36. 多候选定夺 + 无候选归因（2026-11，接 §35）

> 编号说明：§23–§33 留给同时落地的其它模块，本节按预分配取 **36**，接在 §35 后面。
> 工具：`tools/sym_match.py`（`build` / `report [--name]` / `--verify`）；
> 产物：`fixtures/sym_match.tsv`（1,300 行，已入库，可逐行复核）；
> 回归断言：`tests/sym_match.rs`（一致性断言，**不是复原证据**，见 `AGENTS.md` §0.2）。

### 36.0 必须先说的一件事：入库的夹具**不能**由入库的工具重跑复现（观测）

本轮第一件事是照 §35.8 重跑一遍 `build`，结果与入库夹具**不一致**：

| 口径 | unique | weak | 唯一合计 | multi | none | split | merge |
|---|---|---|---|---|---|---|---|
| 入库夹具（§35 的数字） | 655 | 78 | **733** | 63 | 444 | 35 | 25 |
| 同一份代码重跑 | 656 | 79 | **735** | 66 | 443 | 33 | 23 |

逐行比：**38 行不同，其中 14 行连结论类别都不一样**。举两个：

* `0x43d748`（`EffectDrawer::…::{{closure}}`）与 `0x43d930`（同级闭包）在两次生成里**互换**了
  x86 地址（0x4e2a30 ↔ 0x4e2d50）——就是 §35.5 说的"闭包与母函数互换"那一类。
* `0x3da9bc`（`GraphwarTargetManager::subscribe_to_events_event_handler`）入库时是
  `unique 0x3d86a0`，重跑是 `none`。

**这不是随机性**：4 个不同的 `PYTHONHASHSEED`（0 / 1 / 7 / 12345）+ 连续两次运行，
输出**逐字节相同**。所以只能是**夹具与工具不同步**（夹具是开发过程中某一版代码的产物，
后来工具改过、没有再生成）。结论：

* **733 这个数字不可逐位复现**；本轮所有"改动前 / 改动后"的比较都用**同代码重跑**的 735，
  同时给出 733 口径，两个都写。
* 顺手修掉两个会让这个问题继续复发的坑（都在 `tools/sym_match.py`）：
  ① 缓存 key 只有 `tag + 二进制大小`，**没有版本号**——改了特征提取逻辑会静默吃旧缓存
     而不报错。现在 key 里带 `CACHE_VERSION`（本轮 `v4-bsig`）。
  ② `analysis/funcstarts_ghidra_va.pkl` 的路径写死成 `GW2/analysis/…`，在
     `.agent-work/<topic>/wt-NN/` 这种工作区里跑 `build` 直接 `FileNotFoundError`。
     现在按 `GW2_FUNCSTARTS` → 逐级上溯 `analysis/` → `.reference/upstream/analysis/` 找。

### 36.1 一句话结论 + 四问四答

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 多候选定夺** | 多候选 **63 → 37**；其中**定下 26 条**（`modblock` 17 + `score` 9，逐条带 `arbi=` 证据）。**唯一数 733 → 762**（同代码基线 735 → 762，**+27**）。另有一条 `merge` 行的候选被裁决后转成唯一、两条「抢同一个 x86 槽」的行按单射要求记成 `merge`（§36.7）。**没到 +30**，差的见 §36.4 / §36.7 | **观测** + **推断**（每条裁决的证据都写进 `arbi=` 段） |
| **(b) 自校验** | `--verify` 从 **78.7%（222/282）→ 86.2%（243/282）**；召回 89.0% 不变；精确 94.9% 不变。**仍 < 90% 门槛，退出码 1**。多候选→唯一、且有唯一锚可判的 **21 条全部正确（0 错）**；verify 的"唯一候选是错的"从 16 变 17，多的那 1 条是 `0x3dae34`（TargetManager::new）：别的行把它的 5 个候选抢掉 4 个、只剩 `0x5186c0`，而真值是 `0x465910` —— 它不是裁决出来的，是候选集被压缩后剩下的，现在是 `merge;dup_claim` | **观测** |
| **(c) 444 个无候选** | `icf` **0** / `inlined` **6** / `tiny` **172** / `data` **0** / `other` **266**，加起来 **444**。`other` 内部：`no_feature` 207 / `pfx_only` 55 / `feature_no_fit` 4 | `tiny`/`no_feature` 是**观测**（体量、特征集合都能量）；`inlined` 是**推断** |
| **(d) 被否掉的通道** | §35.9 建议的"`.data.rel.ro` vtable 槽顺序当独立约束"——**实测不成立**：两张架构的表内槽序**不一致**（§36.3） | **观测** |

### 36.2 两条裁决规则，以及为什么这不算"猜"

§35 的硬规则是"多候选就把全部候选列出来，不挑一个"。本轮加的是**受限裁决**：
只在"结构事实**唯一**"时收窄，仍然不给任何一条编候选。

**R1 `modblock`——模块块整体指派**

一个 graphwar 模块的 `panic::Location` 文件名是一个**精确**的 `&'static str` 槽
（`src/graphwar_life_manager.rs` 这类）。"引用这个文件名的函数"在两侧都可枚举，
于是整块可以**一起**指派。采纳条件（全部满足）：

1. 两侧成员数相等，或差 ≤1（差 1 = 恰好一个成员在 x86 侧没有独立副本）；
2. 用**体量比 / 指令数比 / 罕见字面量 / f64 / 已配对邻居一致**打分，位掩码 DP 求最优指派；
3. **最优解恰好 1 个**（DP 数出来），且**与"模块内地址序号对齐"给出同一答案**（两条独立规则互证）；
4. 每一对都落在**实测同源区间**（体量比 0.61–1.19、指令数比 0.70–1.45）；
5. 本模块的 x86 成员**被块外函数占用 ≤1 个**（>1 个说明两侧集合已经不一一对应，整块放弃，
   实测就靠这条挡住了 `graphwar_team_manager`：7 对 7，但真值落在两个已被别人占用的槽上，
   不设这条会**整块平移一位**、给出 4 条错答案）。
6. 需要"跳过一行"（两侧差 1）时，枚举跳过位置；**只有一个位置**能让其余各行全部落在同源区间；
   若有多个位置，再要求结构得分的第一名领先第二名 ≥1.0。

**R2 `score`——候选集内唯一最高分**

只在**已经存在的候选集**里比（不新增候选）：最高分与次高差 ≥1.0，且至少有一条
**强特征**（罕见精确字面量 / f64）或 ≥2 条已配对邻居一致。另加两条保护：

* **争用保护**：两行的首选是同一条 x86 槽、得分又拉不开 ≥3.0 时，**两条都留 multi**（谁上都是赌）。
* **整组指派**：互相抢槽的那几行（≤4 行、≤8 槽）整组枚举单射，最优解唯一且领先 ≥1.0 才采纳。
* **迭代**：一行认领后，别的行的候选集少一个竞争项，原来"差一点"的裕度可能变清楚，所以跑到收敛。

**两条规则都只填空**（绝不改已有配对），且保持**单射**（一个 x86 只能被一个 arm64 认领；
`tests/sym_match.rs::unique_answers_are_injective` 会立刻抓住漏网）。

**独立验证（这是本节最该被复核的部分）**：把规则套在**已经有结论的 389 条"multi 型"行**上
（把它们自己的配对挖掉、重算候选、再裁决）：

```text
能定案 245 条；与工具自身结论一致 238 条 = 97.1%
其中有独立 Location 真值的 123 条：对 122、错 1
```

也就是说这条规则的精度在 **97% 量级**、不是"看起来像"。

**关键几条（本轮定下的、最值得复核的）**：

| arm64 | → x86 | 依据 | 档位 |
|---|---|---|---|
| `GraphwarRankManager::{send_all_info_events,handle_entity_removed_event,new,set_rank}` = `0x498b5c / 0x498c58 / 0x498d2c / 0x498eb8` | `0x454d80 / 0x454e80 / 0x454f30 / 0x455090` | 模块块 4:4、最优解唯一、与序号对齐一致、四对体量比 0.98–1.12；**四条全部命中真值** | **观测**（体量/指令数/最优解唯一）+ **推断**（同模块同类） |
| `GraphwarLifeManager::{get_death_tick,set_queued_death,new,get_alive,set_alive,…}` = `0x496af0 / 0x496ea4 / 0x4977b8 / 0x4979a8 / 0x497bb0` | `0x453a60 / 0x453de0 / 0x454790 / 0x454950 / 0x454b40` | 模块块 9 对 8（`get_all_entities` 的 x86 副本被占）⇒ 跳过位置唯一，其余 8 对体量比 0.83–1.11；**8 条全部命中真值** | 同上 |
| `GraphwarTeamManager::add_to_team` `0x498f8c` | `0x3c17d0` | 候选 121 个，但结构得分**领先次高 26.9**、**20 条已配对邻居一致** | **观测**；真值命定 |
| `GraphwarLevelGenerator::place_entity` `0x3dafc0` | `0x465a70` | 模块块 1 行 1 槽 + 体量比 1.08 + 共享 `3.141592654` 与模块文件 lit | **观测** |
| `<GraphwarRootNode as INode>::ready::{{closure}}` `0x39c6e0` | `0x3a5660` | 唯一最高分（领先 2.17）+ 共享模块 lit + 2 条邻居一致 | **推断** |

### 36.3 被否掉的通道：函数指针表的槽序**不**守恒（观测）

§35.9 把"`.data.rel.ro` 的 vtable 槽顺序"列为下一步最该试的独立约束。**本轮试了，不成立。**

做法：扫两侧 `.data.rel.ro` / `.data` / `.got`，把"8 字节一格、值落在 `.text`"的槽收起来，
相邻槽（+8）连成表；arm64 侧 4,872 个槽 / 2,224 张表，x86 侧 8,184 / 2,528。
然后拿**已配对的 6,046 对**当标尺，看"arm 表内第 k 槽 ↔ x86 表内第 k 槽"保不保。

实测反例（gdext 的虚调用 trampoline 表，arm64 表起址 `0x8aed38`、45 槽）：

```text
arm 槽  9 -> arm 0x4d43bc   已配对 -> x86 0x4d7f80，落在 x86 槽 0x9bb528
arm 槽 11 -> arm 0x43f8ec   已配对 -> x86 0x4e4e00，落在 x86 槽 0x9bb4f8   ← 地址**更小**
```

也就是说 x86 那张表里**槽序和 arm64 不一致**（28 个已配对成员里只有一部分单调），
用槽号直接映射会成片配错（自洽率只有 77%）。原因 `未知`（推测与 ICF/哈希式 interning 有关，
但没有证据）。**所以本轮没有把槽序当证据**，只在文档里记下"此路不通"，免得下一个 agent 再试。

### 36.4 还定不下来的 37 个多候选：缺什么证据

| 组 | 行数 | 候选项数 | 卡在哪 / 缺什么证据 |
|---|---|---|---|
| gdext 虚调用 trampoline（`__virtual_call::virtual_fn` / `__default_virtual_call::virtual_fn`，全是 156 B） | **12** | 9–10 | 区分它们的是**代码里嵌的 class 特有数据地址**；本工具的字面量/f64 特征抓不到"指向 `.data.rel.ro` 的引用"，而表槽序又不守恒（§36.3）。缺：**跨架构的 vtable/类描述符对应** |
| `FunctionDrawer::{remove_function,update_transform,update_function_fade,draw}` | 4 | 29–36 | 候选全部来自一条**无区分度**的公共前缀（`upcast_refnull i`，两侧各 80± 个函数引用）；这一段又没有任何 order 锚（前后锚点跨 `0x498c70 → 0x416350`）。缺：这一段的对角线锚 |
| `GraphwarFunctionEvolutionator::{score_function,calculate_functions,calculate_functions_old,new}` | 4 | 6–15 | 模块块 16:16，但**没有完整可指派**：集合里有成员体量比 0.04 / 5.61，说明"引用同一 Location 文件名"的两侧集合在实质上不是同一批函数。缺：区分"目录函数"与"被内联进来的引用者" |
| `GraphwarTeamManager::{remove_from_team,send_all_info_events,new,get_team}` | 4 | 4–115 | 块外占用 2 个 ⇒ 整块放弃（§36.2 条件 5）；两行抢 `0x3c1980`。缺：块外那两条占用的复核（它们可能是错的，但不能靠"推翻别人"来定自己） |
| `GraphwarModeManager::{set_axis_mode,set_time_mode,set_function_mode}` | 3 | 2 | **3 行抢 2 个槽**：x86 只有两份。哪一条被 ICF/内联掉，无法从现有特征判定。缺：三条函数的**字段偏移**（存的是不同成员，但 ours 不抽 `str [x,#off]` 的位移） |
| `GraphwarSoldierManager::{add_soldier,send_all_info_events}` | 2 | 2 | 两行抢 `0x455570`，得分拉不开 3.0 ⇒ 两条都留 multi |
| `MathFunction::{remove_offset,evaluate_token}` | 2 | 3–4 | 候选集里**不含真值**（`0x4eb830` 是 `remove_offset` 的真值）——说明候选生成就错了，不是裁决不够 |
| `GraphwarBanList::new` / `GraphwarEffectManager::new` | 2 | 3–4 | 构造函数共享泛型字面量，得分领先 0.20 / 0.27 |
| `GraphwarLifeManager::get_all_entities`、`TargetManager::subscribe…::{{closure}}`、`FunctionInput::delete_chars_at_caret`、`GraphwarSetupControl::ready::{{closure}}`、其余 trampoline | 8 | 2–124 | 各缺一条独立证据；`get_all_entities` 的 x86 副本被别的 arm64 函数占着（**观测**：模块块 9:8 的"跳过位置"就落在它身上） |

### 36.5 444 个无候选：按"为什么对不上"分类

分类是 `build` 时**逐条**算的，写进 `evidence` 栏：`no_candidate;why=<类别>;<证据>`，
所以 `awk -F'\t' '$5=="none"' fixtures/sym_match.tsv` 就能复算。判定顺序 =
"先能证死的、后靠猜的"：

| 类别 | 条数 | 判据（可复核） | 档位 |
|---|---|---|---|
| **ICF 合并** | **0** | `bsig`（函数体**逐字节** sha1）相同的孪生且该孪生已被配对 | **观测**（判据能跑，但这 444 条里一条都没有） |
| **被内联** | **6** | ① 闭包 + 母函数已配对 + 地址相邻；② 函数体助记符序列被相邻已配对兄弟包含；③ 特征集合被某个已配对 x86 函数包含 | **推断** |
| **太小**（`drop_in_place` / 转发 shim） | **172** | 指令数 ≤ 12 | **观测** |
| **纯数据 / 跳转表** | **0** | 体内没有任何控制流指令，或反汇编为空 | **观测**（444 条全部至少有一条 `b`/`bl`/`ret`，没有一条是 `.text` 里的数据） |
| **其它（未知）** | **266** | 以上都不成立 | **未知**（但把哑火原因写进证据串） |

`其它` 的 266 条内部（同一列的可复核子串）：`no_feature` **207**（**没有字符串字面量、没有 f64、
没有罕见整数常量**——本工具的跨架构证据全部哑火，只剩顺序，而那几段顺序块整块没对上）、
`pfx_only` **55**（只有"两侧几百个函数都引用"的公共前缀）、`feature_no_fit` **4**（有特征，
但共享它的 x86 函数体量比全部出闸）。

**举三个具体例子**：

* `0x3ae8cc` / `0x3ae8cc` / `0x3ae8cc`（同一地址挂三个符号）
  `core::ptr::drop_in_place<graphwarrust::axis_drawer::AxisDrawer>` 等，**7 条指令**：
  就是一个 `Rc` 计数递减 + 条件调用析构的转发 shim，没有字面量、没有常数 ⇒ `why=tiny;ninsn=7`。
* `0x469c54` `graphwar_event::_::<impl Deserialize for GraphwarEvent>::deserialize`，
  **5,564 字节 / 1,391 条指令**，26 个字面量：**有特征、但体量比全出闸** ⇒
  `why=other;feature_no_fit`。它是 serde 生成的巨型 `match`，与 x86 那份的差别在
  内联深度，不是同一份函数体。
* `0x3af898` / `0x395550` / `0x4d7f34` `drop_in_place<GraphwarRootNode>`：
  1,244–1,252 字节、311 条指令、**零字面量零 f64** ⇒ `why=other;no_feature`。
  三条同名同尺寸，但**逐字节不同**（所以不是 ICF，理由见下）。

**两个必须写下来的负结果**：

1. **"形状相同" ≠ ICF。** 这 444 条里有 **338 条**存在"助记符序列逐条相同"的孪生
   （其中 150 条的孪生已经被配对），但**逐字节相同的一条都没有**；而且孪生往往是
   **完全不同的源函数**——例：
   `drop_in_place<Result<TickEvent,_>>`（`0x34acfc`，88 B / 22 条）与
   `drop_in_place<http::extensions::Extensions>`（`0x4933d4`，同尺寸同形状）。
   拿形状判 ICF 会得到 150 条假阳性。**ICF 只能按逐字节相同判。**
2. **§35.1(d)① 那个 ICF 例子是错的，就地更正。** §35 说
   `GraphwarTargetManager::new` 与 `GraphwarPlayerManager::new` 在 arm64 是"396 字节 / 99 条指令
   逐条同构"、x86 只留一份、真值只能二选一。实测：
   * 两条都是 396 B / 99 条指令，但**助记符序列并不逐条相同**（99 个位置里 13 个不同，
     差的是 `adrp+add` 与 `mov/mov` 的**地址物化次序**），`bsig` 也不同；
   * 真值给了**两个不同的 x86 地址**：`0x465910`（TargetManager）与 `0x455410`（PlayerManager，
     本轮由模块块指派定下、命中真值）。
   ⇒ 它们在 x86 是**两条**函数，"ICF 只能二选一"这个说法不成立。真正的困难是
   TargetManager 那一侧的候选集里根本没有 `0x465910`（它被块外函数占用了）。

### 36.6 复算命令

```bash
# 1) 生成（首次实测 22 秒：要反汇编 12,609 + 13,263 个函数；缓存命中 1.3 秒）
../.venv/bin/python tools/sym_match.py build

# 2) 统计：唯一 / 多候选 / 无候选 / 一拆多 / 多合一 + 裁决明细 + 无候选归因
python3 tools/sym_match.py report

# 3) 单条复核（任务书里说的 report --name，本轮加进工具了）
python3 tools/sym_match.py report --name GraphwarLifeManager

# 4) 自校验：拿 326 行唯一锚当 ground truth，退出码 0/1（90% 门槛）
python3 tools/sym_match.py --verify ; echo "exit=$?"

# 5) 无候选归因的原始计数
awk -F'\t' '$5=="none"{split($6,a,";why=");split(a[2],b,";");print b[1]}' fixtures/sym_match.tsv \
  | sort | uniq -c
```

`build` 需要 capstone（走仓库 venv）；`report` / `--verify` 只读 TSV，**标准库即可**。
特征提取缓存在 `.agent-work/symmatch/cache/`（已 gitignore），key 里带
`CACHE_VERSION`——**改了提取逻辑必须同时改版本号**，否则会静默吃旧缓存（§36.0）。

### 36.7 还缺什么（`未知`）

* **唯一数没到 +30**：733 → 762 是 **+29**（同代码基线 735 → 762 是 **+27**）。差的那几条
  卡在 §36.4 的「3 行抢 2 个槽」（`GraphwarModeManager` 的 3 个 setter 对 2 个槽）和
  「块外占用 >1」（`graphwar_team_manager` 等）上；另外本轮加的单射收尾把 **2 行**记成
  `merge`（两条不同的 arm64 函数抢同一个 x86 槽，工具无法裁决）：
  `0x3dae34`（TargetManager::new，**本来就是错的**，真值 `0x465910`）与
  `0x46d0b0`（`…SkinGraphicsRequestEvent…::expecting`，`rival=0x46cd88`）。
* **`--verify` 仍 86.2% < 90%**，退出码 1。剩下 39 条：17 条"唯一候选是错的"（多是
  §35.5 说的闭包/母函数互换与 block 被挤平移）、13 条"名字键没对上"（arm64 没有独立符号）、
  8 条"候选含真值但裁决不了"、1 条无候选。
* **裁决的规则参数是实测定出来的**（`SCORE_MARGIN=1.0`、`CONTEST_MARGIN=3.0`、`RARE_N=24`、
  `TINY_NINSN=12`），不是从原理推的。留一验证给了精度量级（97%），但**这些阈值在唯一锚上
  被看过**，所以严格说"独立性"比主匹配弱一档——这一点必须写明，不能当成完全独立的证据链。
* **`inlined` 那 6 条是推断**：本工具没有做"跨架构的函数体包含"（两侧指令不可比），
  用的是"arm64 侧序列包含 + 母函数已配对"这种**间接**证据，缺一条直接的。
* **`data`/`icf` 两档都是 0**：判据是能跑的（`bsig` 在这份二进制里确实找到 105 组、
  456 个逐字节相同的函数），只是这 444 条里一条都不落进去。如果以后有人质疑"分类是我硬凑的"，
  把判据跑一遍即可（§36.6 第 5 条 + `bsig`）。

## 37. 换一份**同架构**符号表：Windows 2.0.60 的 `graphwarrust.dll` 把可定名函数从 325 提到 351（2026-11，接 §20）

### 37.0 一句话

§20 的名字来自 **Android arm64** 的 `.symtab`，桥是 `panic::Location` 的
`(file,line,col)` —— **跨架构**，而且只有 520 个逻辑名。
本节换成 **同版本（2.0.60）、同指令集（x86-64）、带 COFF 符号表**的 Windows DLL：

* **先证明是同一份源码**：`src/*.rs` 的 `(file,line,col)` 三元组
  Windows **2,051** / Linux **2,035** / **交集 2,034**（§20 那个 2,035 在这里被独立复现）；
* 名字从 520 → **822** 个逻辑名（`module/type/method` 口径与 `name_to_addr.tsv` 相同）；
* 可定名的 Linux 地址：Android **325** → Windows **333** → **两者并集 351**（+26）。

分母是 **527** 个「引用了 `src/*.rs` Location」的函数（`tools/coverage_matrix.py --check`
独立打出同一个 527）。⇒ 覆盖率 **351 / 527 = 66.6%**。

> ⚠️ **2026-11-08 任务 30 更正（就地，见 §40.7）**：这个 351 是 **v1+v2+Windows 三份夹具的地址并集**，
> 其中 **6 个地址不在 527 内**（它们不引用任何 `src/*.rs` Location：`0x3bd640` clap、
> `0x519450`/`0x519530` std time、`0x4eb830`/`0x4ec160` 无 Location、`0x748000` 引用 25 个 miniz_oxide Location）。
> 统一到"527 内"的口径后是 **348 / 527 = 66.0%**，**无名 179 个**（不是 176）。
> §37 下面那些 `351 / 66.6%` 的行都按这个更正读；§40 是逐条对账后的版本。

**必须一并记住的负面结论**：`2,035` 是 **Location 的个数，不是函数的个数**。
2,035 个三元组只被 **527** 个函数引用（大函数平均带 3.9 个），
所以「理论天花板 2,035 个函数」这个说法是错的，真天花板是 527。

### 37.1 物证

| 项 | 值 | 档位 |
|---|---|---|
| 文件 | `.reference/steam/windows-2.0.60/graphwarrust.dll` | `观测` |
| 大小 / sha256 | 41,259,197 B / `ab490af6686122487656966997c4bae396098dd18fb4af67213d4f5236b69fb8` | `观测` |
| 头 | `pei-x86-64`，`ImageBase = 0x180000000`，11 节，COFF 符号记录 147,444 | `观测` |
| `.text` | `0x14b50b8` = 21,713,080 B；线性反汇编 **4,473,993** 条指令 | `观测` |
| `.reloc` | 38,355 条 `IMAGE_REL_BASED_DIR64`（type 10）+ 189 条 ABSOLUTE（type 0） | `观测` |
| `Location` 结构体 | **6,285**（唯一三元组 5,464） | `观测` |
| 其中 `src/*.rs` | **2,056** 个结构体 / **2,051** 个三元组 / 73 个文件 | `观测` |
| 备用差分 | `.reference/steam/windows-early2026/graphwarrust.dll`（35 MB，sha `05c1830b…`） | `观测` |

### 37.2 先验版本，再谈名字

`(file,line,col)` 是编译期常量，与指令集无关，所以它是**唯一能跨构建稳定对齐的键**。
两侧各算一遍，直接对拍：

```
Windows src/*.rs 三元组 2,051     Linux 2,035
交集 2,034       仅 Windows 17    仅 Linux 1
```

* **仅 Linux 的 1 个**：`src/graphwar_target_manager.rs:35:24`。
* **仅 Windows 的 17 个**：分布在 `graphwar_game_server.rs` / `graphwar_game_client.rs` /
  `server_broker_*.rs` / `graphwar_name_manager.rs` 等 11 个文件里，是 `#[cfg]` 分支差异。

差值 1 → 2,035 个里 2,034 个逐字相同。**这不是「差不多」，这是同一份源码**；
`docs/02` §20.1 用 Android 侧得到的结论，在 Windows 侧独立复现了一次。

⇒ **没有发现「DLL 与本目标不同版本」的证据**。反过来，若哪天换了 DLL 而交集掉到
1,900 以下，那就是版本不符 —— 这个数是版本判据，别当装饰。

### 37.3 PE 上 Location 的形态与 ELF 不同（这一步必须自己验，不能照搬 §20）

| | ELF（`loc_xrefs.py`） | PE（本工具） |
|---|---|---|
| 存放 | `.data.rel.ro` | `.rdata` / `.data` |
| `file_ptr` 落盘值 | `R_X86_64_RELATIVE` 的 **addend**（= 目标虚地址） | **`ImageBase + RVA` 的 8 字节绝对常量** |
| 改基址靠 | `.rela.dyn` | `.reloc` 的 `IMAGE_REL_BASED_DIR64` |
| `{file,len,line,col}` | 24 字节 | 24 字节（**同构**） |

结构体本身没变，变的只是「`+0` 那个 8 字节怎么读」。验的办法是**反向用重定位表**：
`.reloc` 里每一条 DIR64 的地址就是一个「落盘的是绝对地址」的槽，
于是 Location 的首字段必然在其中。

这不是优化，是**主路径**：`.reloc` 对「落盘值是绝对地址」的槽是**穷尽且可证**的
（链接器必须为每一个需要改基址的槽发一条 DIR64），而盲扫只是启发式。
本版两条路径的**结果完全一致**（下文数字，`win_names.py` 的两个分支都实跑过）：

| | `.reloc` 路径 | 盲扫（8 字节对齐 + 自校验） |
|---|---|---|
| 全部 Location 结构体 | 6,285 | 6,285（差集 0 / 0） |
| `src/` 结构体 | 2,056 | 2,056 |
| `src/` 三元组 | 2,051 | 2,051 |

⇒ 本版**盲扫并不漏**。选 `.reloc` 当主路径的理由只有两条：可证穷尽、不依赖对齐假设；
盲扫保留为 `.reloc` 被剥掉时的兜底（`find_locations` 里 `if not dir64:` 那一支）。
⚠️ 这一条**初稿写错过一次**：初版说「盲扫只找到 1,046 个，漏掉一半」，那是脚手架里
字面量边界用「向后走路径字符」判断导致的**测法错误**，不是二进制的事实。
改成两侧同口径（`len` 与正向可打印长度严格相等 + 同一套 `line/col` 窗口）后差集为 0。

### 37.4 COFF 符号的 `Value` 是**节内偏移**，不是 RVA（一个静默平移 4 KB 的坑）

`objdump -t` 里第一条 `_ZN12graphwarrust…` 的 value 是 `0x2b120`。两种读法：

* 当 RVA 读 → `ImageBase + 0x2b120` → 文件偏移 `0x2a520` → 字节 `48 85 d2 0f 84 …`
  （一条 `test rdx,rdx`，**不是函数头**）；
* 当节内偏移读 → `ImageBase + .text.VirtualAddress(0x1000) + 0x2b120`
  → 文件偏移 `0x2b520` → 字节 `41 57 41 56 57 55 53 48 81 ec 98 00 00 00`
  （`push r15;push r14;push r12;push r13;push rbx;sub rsp,0x98`，**标准 Rust 序言**）。

独立佐证：符号 value 落在 `.pdata` 的 `BeginAddress` 上的只有 **34 / 1,018**；
加上 `.text` 的 RVA 之后是 **910 / 1,018**。⇒ 节内偏移，正确。

这个错**不报任何异常**，只会让所有地址整体偏 4 KB，然后 join 全空。
所以 `win_names.py stats` 把「命名函数落在函数起点上」当成常驻自检打印出来
（现在是 1,317 / 1,317）。

### 37.5 `.pdata` **不穷尽**：leaf 函数没有 `RUNTIME_FUNCTION`

x64 的 unwinding 表只覆盖「需要栈展开信息」的函数；**纯 leaf 函数可以没有条目**。
本 DLL：`.pdata` 48,853 条，覆盖 `.text` 的 **96.5%**；另有 **6,429** 个 `.text`
函数符号的地址**不在**任何 `.pdata` 起点上 —— 其中 106 个是 `graphwarrust` 的具名函数
（大多是 `{{closure}}`）。

拿 `.pdata` 单独当边界，这 106 个函数引用的 Location 会被算到**前一个**函数头上，
「Windows 侧唯一引用」的判定就被污染。所以边界取
**`.pdata` 起点 ∪ `.text` 函数符号（`ty == 0x20`）地址**，并集 **55,282**。

这是为了与 Linux 侧对齐：`loc_xrefs.py` 用的是 `.eh_frame` FDE，Rust 在 ELF 上给所有
函数（含 leaf）都发 FDE，本来就是穷尽的。
（实测：并集口径与 `.pdata` 口径下 tier-1 结果**完全一致**（334 个函数 / 332 个地址，
`|S(W)| == |T(L)|` 精确一致率同为 95.5%），所以这个改动只提高正确性、不制造数字。）

### 37.6 名字过滤：用 `_ZN12graphwarrust` 前缀会**静默丢掉 59 个函数**

GDExtension 的 trait impl 名字长这样：

```
_ZN133_$LT$graphwarrust..axis_drawer..AxisDrawer$u20$as$u20$godot_core..obj..traits..cap..ImplementsGodotApi$GT$18__register_methods…
```

注意 crate 名在这里是 `graphwarrust..`，**没有长度前缀**（`12`）。
只按 `_ZN12graphwarrust` 前缀过滤会漏掉 **59** 个函数，
其中 **25 个**是本工具**唯一**能钉住的地址 —— 收益凭空少 25 个。

最终判据（`win_names.is_game_name`，两侧**完全相同**）：

| 类 | 形态 | 收 | 理由 |
|---|---|---|---|
| A | `_ZN12graphwarrust…` | ✅ | crate 内直接路径 |
| B | `_ZN<len>_$LT$graphwarrust..Type…$GT$…` | ✅ | **显式 impl 在游戏类型上**的 trait 方法（GDExtension 虚拟） |
| C | `core::ptr::drop_in_place<…graphwarrust::…>` | ❌ | 库函数恰好以游戏类型为泛型实参，编译器生成的胶水，不是原版源码函数 |

C 类有 327 个名。实测把它算进来**不会多钉住任何地址**（A+B 与「包含 graphwarrust」
两种口径的 tier-1 都是 334 / 332），只会污染名字集合 —— 而 `parse_name` 对 C 类只能给出
`(module, type, None)`，写进夹具就是虚报。

### 37.7 两级 join

**`唯一锚`（硬配对）**：某个三元组在 Windows 侧只被一个函数引用、在 Linux 侧也只被一个
函数引用。判据与 `name_bridge.py` 同一套（**两侧必须同一套规则**，否则一对一会被人为破坏）。
这一级**不看名字**，所以它同时是下一级的独立裁判。

**`集合锚`（集合互最佳）**：单个三元组经常被内联共享，但**一组**三元组的重叠可以唯一。
四条全中才取：① Linux 侧重叠数严格唯一最大；② 反向也成立（互为最佳，剔掉多对一）；
③ 重叠 ≥ 2；④ 与 `唯一锚` 冲突的降级为 `歧义`，**不静默取一个**。

结果：`唯一锚 334` + `集合锚 1` + `歧义 2` + `无一对一锚 214` = 551 行。
自检：**集合锚与唯一锚的重叠配对 0 个不一致**（本版两者只交叠 0 个，因为集合锚
在唯一锚覆盖处都被唯一锚先接走了 —— 这条自检的效力因此有限，**记为 `推断`**）。

> 三级证据的原话：`唯一锚` / `集合锚` 都是 `观测`（可指到反汇编地址）；
> 「集合锚的精度」只有间接自检，**不给 `逐位`**。

### 37.8 量化（全部可用 §37.11 的一条命令复跑）

| 指标 | 值 |
|---|---|
| Windows 游戏符号 | 1,402 个 raw → **822** 个逻辑名（`module/type/method`）→ 1,317 个函数地址 |
| 引用了 `src/*.rs` Location 的命名函数 | **551**（其余 766 个无锚） |
| Windows 落地 | 297 个逻辑名（36.1%）/ **333 个 Linux 地址** |
| 分母（`coverage_matrix --check` 独立复核） | **527** |
| Windows 覆盖率 | 333 / 527 = **63.2%** |
| §20（Android）已有 | 326 行 `唯一锚` / **325 个地址** |
| **并集** | **351 个地址 = 66.6%**（+26；Android 独有 18，Windows 独有 26）<br>⚠️ 其中 6 个不在 527 内 ⇒ **527 内是 348 = 66.0%**，无名 **179**（见上注与 §40.7） |
| Windows 独有逻辑名 | **255** 个，其中落地 **4** 个；**单独钉住**（Android 钉不到）的地址 **3** 个 |

**「名字落地前后」按两种口径都给**（避免口径歧义）：

* `唯一锚` **行数**（与 `name_to_addr.tsv` 的 326 同口径）：326 → **334**；
* 可定名 **Linux 地址**：325 → **351（并集）**。

**Windows 独有名字落了 4 个 —— 这个数小，原因要写清楚**：Windows 比 Android 多出的
255 个名字，绝大多数落在 `graphwar_event` 的 serde 派生、`__register_methods` 这类
**没有 panic 点**的包装函数上；而能不能落地只取决于「它引不引用 `src/*.rs` Location」。
⇒ **增量主要不是来自「新名字」，而是来自「同一批名字在 x86-64 上钉得更准」**：
Android 侧 171 行「歧义」里有一大批是 arm64 内联形态造成的，Windows 侧收敛了。

### 37.9 冲突逐条（7 条，**0 条无法解释**）

两侧都钉住、但名字不一致的 Linux 地址共 **7** 个（一致 **300** 个）。逐条给证据 ——
证据是「谁的三元组集合把 Linux 侧这个函数的集合覆盖得更全」：

| Linux 地址 | Android 名 | Windows 名 | Linux 引用三元组 | 裁决 |
|---|---|---|---|---|
| `0x3a2e10` | `GraphwarRootNode::ready` | `ready` (14/17) + `ButtonManager::new` (2/17) | 17 | Windows 是**超集**：`ButtonManager::new` 在 Linux 上被内联进 `ready` |
| `0x3e2830` | `handle_server_remote_event` + `set_color` + `report_fired_function` | `handle_server_remote_event` (89/103) | 103 | Windows 更收敛；Android 那 2 个是 arm64 侧的内联见证 |
| `0x4500c0` | `game_server_thread` | `game_server_thread` (66/76) + `GraphwarServerConnectionHandler::new` (4/76) | 76 | Windows 是**超集**，`new` 被内联 |
| `0x45b020` | `GraphwarServerConnectionHandler::tick` | `…::subscribe_to_events_event_handler` **8/8** | 8 | **集合相等** ⇒ Windows 名是定义点 |
| `0x465780` | `GraphwarTargetManager::subscribe_to_events_event_handler` | `…::send_all_info_events` **1/1** | 1 | 集合相等 |
| `0x465870` | `GraphwarTargetManager::subscribe_to_events_event_handler` | `…::handle_entity_removed_event` **1/1** | 1 | 集合相等 |
| `0x519d80` | `GraphwarEffectManager::tick` | `…::subscribe_to_events_event_handler` **2/2** | 2 | 集合相等 |

两类，都不是「谁错了」：

1. **内联见证（3 条）**：Windows 侧一个地址上出现两个名字 —— 说明在 Linux 上
   小的那个被内联进了大的。这是**信息**，不是冲突。
2. **容器名（4 条，4 条全部 `集合相等`）**：Linux 函数引用的三元组集合与 Windows 侧
   某一个函数的集合**完全相等** ⇒ Linux 上那个函数就是 Windows 那个函数；
   Android 给出的名字是**内联容器**（arm64 上把被调函数内联进来了）。

⇒ 本节的口径：**同一地址多名字 = 内联见证，保留全部名字，不挑一个**。
夹具里 `linux_addr` 一列因此允许一行多地址、也允许多行同地址（见 §37.10 限制 3）。

### 37.10 还缺什么（`未知`）

1. **527 里有 176 个仍定不了名**（351 已覆盖）。原因分三类，都不在「符号表」这一侧：
   ① Linux 上那个函数引用的 Location，在 Windows 上被**无名函数**引用（Windows 侧
   591 个引用 `src/` Location 的函数里 40 个没有任何 graphwarrust 符号）；
   ② Windows 上对应函数被内联进更大的函数，三元组归属变了；
   ③ 互最佳不成立（本版反向诊断：527 个里有 **342** 个存在唯一互最佳伙伴，
   其中 **333** 个伙伴有名字 —— 与落地 333 个地址同量级，两条独立链路互相印证）。
2. **`无一对一锚` 214 个 Windows 命名函数**仍是 `未知`：它们的 Location 全部被共享。
   用「独有字符串 + 函数大小 + 调用邻居」兜底会落到 `推断` 档，本节**没做**，
   留作下一步（`name_bridge.py` 的 `次级证据` 那一档可以复用）。
3. **夹具 `fixtures/windows_name_to_addr.tsv` 一行 ≠ 一个 Linux 函数**：
   一行是一个 Windows 命名函数，`linux_addr` 可能是多个（`歧义` 行为空）。
   消费方应当按 `linux_addr` 反向聚合，**不要假设 1:1**。
4. **`windows-early2026` 那份（sha `05c1830b…`）没有跑完**：它是更老的构建
   （35 MB vs 41 MB），本来作为版本差分用；本节只用到 2.0.60 那一份。
5. **没做 PE 侧的函数体级对拍**（大小 / 指令序列）。Windows 与 Linux 的 x86-64 指令
   可以逐条比，但涉及重定位、常量池、`__chkstk` 差异，是另一个工具的工作量。

**一个必须举出来的例子（否则会被反复问）**：**求值器 `0x4eb9d0` 仍然没有名字。**
它只引用一个 Location `src/math_function.rs:184:24`，而这一行在 Windows 侧被
**3 个**函数同时引用 —— `MathFunction::evaluate_token` / `evaluate_recursive` /
`simplify_recursive`（三者都把那一行的 panic 点内联了）。于是 `|W| = 3`，
一对一不成立，**换任何一份符号表都一样**。夹具里这 3 个落在 `无一对一锚`
（Android 侧 `evaluate_token` / `evaluate_recursive` 同样只能到 `歧义`）。
采样器 `0x3c2530` / 修正 `0x3c5f70` 更彻底：**一个 Location 都不引用**，
不在 527 这个分母里，名字无从谈起。

反过来，**`AGENTS.md` 记的 parser 入口 `0x4ed670` 被稳稳定名**：本工具独立得到
`math_function::MathFunction::new`（`唯一锚`，match `1/1`），
与 §20 用 Android 表得到的 `name_to_addr.tsv` 里那一条**逐字相同** ——
这是两条互不相干的链路在同一地址上的一致，可作为整条管线的端到端自检。

### 37.11 一条命令复现

```bash
# 0) Linux 侧的 Location 表（约 40 s，只读目标 .so）
.venv/bin/python tools/loc_xrefs.py dump locs > /tmp/linux_locs.tsv

# 1) 全部验收数字（Location 数 / 交集 / 落地 / 与 Android 比 / 冲突逐条带证据）
.venv/bin/python tools/win_names.py report --linux-locs /tmp/linux_locs.tsv

# 2) 生成夹具（不要手改）
.venv/bin/python tools/win_names.py bridge --linux-locs /tmp/linux_locs.tsv
```

`win_names.py` 缺省 `--dll` 是 `.reference/steam/windows-2.0.60/graphwarrust.dll`
（环境变量 `GW2_WIN_DLL` 可换）；`--cache FILE` 把 60 s 的全量反汇编结果落盘复用
（缓存里带 identity **和 schema 版本**：改了事实口径会失效，不会拿旧结论冒充新结论）。
`report` 里的「Linux 侧分母 527」与 `tools/coverage_matrix.py --check` 打出的
「原版可归属函数 527 个」是**同一条独立链路上的同一个数**。

---

## 38. 三向符号匹配：Windows ×2 + Android + Linux —— 天花板确实是 2,034，天花板之外只值 ≈42 个（2026-11）

> 接 §20（Location 桥）与 §36（多候选裁决）。上一轮只用 **Android×Linux** 两向；
> 本节把同一份数据加到**四向**（Linux 目标 / Android 2.0.60 / Windows 2.0.60 / Windows early2026）。
> 工具：`tools/three_way_match.py`（`build` / `report` / `verify` / `ablate` / `probe` / `key` / `nm`）；
> 产物：`fixtures/name_to_addr_v2.tsv`（**980** 行；原 990 行 —— §39.9 收紧名字判据后去掉了 10 行
> 「编译器胶水」的假名字，**它们本来就没有地址**，`唯一锚` 317 行一行未变）；
> 本节的每个数字都能用 §37.9 的命令复算。

### 38.0 一句话结论 + 四问四答

**Windows 2.0.60 与 Linux 目标是同一版本**：`src/*.rs` 的 `panic::Location` 三元组 2,035 个里
**2,034 个三方同时存在**（99.95%）。于是"同 ISA + 有 COFF 名字"的第二来源可用，
但它**没有把天花板抬起来，也没有把天花板之外变成可用**：

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 三方 Location 数** | Linux **2,035** / Android **2,035** / Win 2.0.60 **2,051**（被引用 2,049）/ Win early2026 **2,011**；三方交集 **2,034**，只在 Linux 侧 **0**、只在 Windows 侧 **17** | **观测** |
| **(b) 名字落地** | 唯一名并集 990 → 同版本来源落地 **317 个名字键**（v1 是 326 行 / 299 键 ⇒ 净 **+18**）；另有 **30 个**只有 early2026 支持，单列「跨版本锚」**不进**唯一锚计数 | **观测**（每个都写出票数与键） |
| **(c) 冲突** | 同版本来源互相矛盾的 **8 个键**（不给地址）；**262 个** Location 键被 ≥2 个不同名字引用（gdext 样板占多数）；**33 个** Linux 地址被 ≥2 个名字认领 | **观测** |
| **(d) 天花板之外** | 12,736 个无锚函数里，结构最近邻能给出"高置信 + 单射"的只有 **44 个**；正对照留出半准确率 **96.3%** ⇒ 上限估计 **≈42 个**。匹配器本身没坏：真值 top-10 命中 **81.9%**、互为 top-1 命中 **100%**（留出半 n=26~38），随机基线 0.31% | **观测**（准确率）+ **推断**（42 这个数） |

⚠ **天花板之外没有任何一条被写进夹具当名字**。`name_to_addr_v2.tsv` 里
「未知」的 625 行是硬边界，不是没算（§39.9 收紧判据前是 635 行）。

### 38.1 方法：一份代码吃四种构建

| 构建 | 格式 / ISA | 函数宇宙（口径写死） | 名字来源 | Location 载体 |
|---|---|---|---|---|
| Linux 2026-08-28（**目标**） | ELF x86-64，stripped | **Ghidra 起点表** 13,263（与 `name_to_addr.tsv` 同口径，换了表就两边比的不是同一批函数） | ❌ 无 | `.data.rel.ro` + `R_X86_64_RELATIVE` addend |
| Android 2.0.60 | ELF arm64 | `.symtab` FUNC 12,609 | ✅ 621 唯一名 | 同上（`R_AARCH64_RELATIVE`） |
| **Windows 2.0.60** | PE32+ x86-64（MinGW/GNU ld） | **`.pdata` 的 `RUNTIME_FUNCTION`** 48,853 条 + 未被覆盖的 COFF FUNC 符号 ⇒ 51,985 | ✅ **871 唯一名**（1,206 个函数） | `.rdata`/`.data`，**绝对 VA** + `.reloc` 的 `DIR64` 门控 |
| Windows early2026 | 同上 | 43,096 | ✅ 876 唯一名 | 同上 |

四种构建走**同一套**特征抽取与同一套「什么算引用了一个 Location」的规则
（首址 / `+8`(len) / `+16`(line,col) 三者都算，`LocHit.lookup`）——规则不对称会人为制造假歧义（§20.2 第 2 条）。

### 38.2 PE 的 `panic::Location` **不是** ELF 那一套（本节最容易翻车的地方）

任务书要求「自己判断，别照搬 ELF 的假设」——实测结论：

| | ELF（Linux / Android） | **PE（两份 DLL）** |
|---|---|---|
| 指针槽里存的是什么 | **0**（真值只在 `R_*_RELATIVE` 的 addend 里） | **已经是绝对 VA**（`ImageBase 0x180000000` + RVA，磁盘上就写好了） |
| 对应物 | `R_X86_64_RELATIVE` / `R_AARCH64_RELATIVE` | `.reloc` 里的 `IMAGE_REL_BASED_DIR64`（type 10），38,355 个槽 |
| 结构体布局 | 24 字节 `{ptr, len(u64), line(u32), col(u32)}` | **完全相同**（`core::panic::Location` 的布局与 ISA 无关） |

判定五件套（`Pe.locations()`）：① 槽在 `.reloc` 的 DIR64 表里；② ptr 落在 `.rdata`；
③ `4 < len ≤ 300`；④ `ptr` 处 len 字节全可打印 ASCII **且以 `.rs` 结尾**；⑤ `1 ≤ line < 10^5`、`1 ≤ col < 2000`。

**自校验（这是"不是裸扫撞上的"的证据）**：2.0.60 扫出 **6,284** 个 Location 结构体，
其中 **0 个**不在 `.reloc` 表里；early2026 扫出 **6,037** 个，同样 **0 个**不在。
随机 8 字节同时满足"在 reloc 表 + 指向 `.rdata` + 长度像字符串 + 是 `.rs` + 行列合理"的概率极低。

另外两个必须写下来的 PE 细节：

* Windows 侧的 `rustc`/`cargo` 路径用反斜杠（`...\\library\\core\\src\\str\\pattern.rs`），
  而**游戏自己的 `src/*.rs` 用正斜杠**，与 ELF 侧逐字符相同 ⇒ `src/` 前缀可以直接当跨构建的 join 键。
* Windows 侧 std 的路径更长，`MAX_STR` 卡 128 会整片丢掉 std 的 Location（本工具用 300）；
  这只影响"总 Location 数"，不影响 `src/*.rs` 的交集。

### 38.3 ① 三方 Location 数与交集（同版本的自证）

| 构建 | 函数 | Location 结构体 | 去重 `(file,line)` | `src/` 结构体 | 被引用的 `src/` 键 |
|---|---|---|---|---|---|
| linux（目标） | 13,263 | 5,300 | 4,666 | 2,042 | **2,035** |
| android 2.0.60 | 12,609 | 5,223 | 4,599 | 2,040 | **2,035** |
| win 2.0.60 | 51,985 | 6,284 | 5,463 | 2,056 | 2,049 |
| win early2026 | 43,096 | 6,037 | 5,191 | 2,011 | 2,009 |

```text
linux ∩ android      = 2,035      ← 上一轮就是这个数（§20.3）
linux ∩ win(2.0.60)  = 2,034
android ∩ win(2.0.60)= 2,034
linux ∩ android ∩ win= 2,034      ← 三方交集；任务书说"应接近 2,035"，差 1
只在 linux 侧        = 0
只在 win 侧          = 17
linux ∩ win(early2026)= 1,176     ← 57.8%，早一版
```

**结论（`观测`）：Windows 2.0.60 与 Linux 目标同版本**，不是"接近"而是 2,034/2,035。
**"版本不一致"这条判据也是实测过的**：把 early2026 当来源，交集立刻掉到 1,176（57.8%）——
如果哪天拿到的是错版本的 DLL，`report` 的第一张表会立刻显示出来，不需要猜。

**那 17 个"只在 Windows 侧"的键不是版本差异**（`观测`）：按文件归类，
**9 个落在 `src/server_broker_*.rs`**（`server_broker_http_handler` /
`provider_connection`×3 / `request_connection` / `room_provider_handler`×2 / `room_request_handler`×2），
其余在 `graphwar_game_server.rs`×3、`function_history_recorder.rs`、`graphwar_effect_manager.rs`、
`graphwar_game_client.rs`、`graphwar_name_manager.rs`、`graphwar_player_screen_control.rs` 各 1。
broker（LAN 服务端）在 Windows 构建里多留了几个 panic 点、Linux 侧被内联/删掉了。

**唯一一个"只在 Linux 侧"的键**是 `src/graphwar_target_manager.rs:35:24`，引用它的只有
**`0x465910`**（= `GraphwarTargetManager::new`，与 §36.5 记的真值一致）。Windows 2.0.60 里
这个键**连结构体都没有**——同一份源码在两个目标上的内联结果不同，属正常。

### 38.4 ② 名字落地：唯一锚 326 行 / 299 键 → **317 键**（+18），另有 30 个跨版本锚单列

| 来源 | 唯一名 | 落地数 | 说明 |
|---|---|---|---|
| **win 2.0.60** | 871 | **313** | 同 ISA 同版本，最强 |
| android 2.0.60 | 621 | 317 | 跨 ISA 同版本 |
| win early2026 | 876 | 240 | **不同版本**，只作交叉校验 |
| 并集 | 990 | 355（有条目） | 去重后见下 |

落地来源组合（每个名字键只算一次）：`android+win+win_old` **202** / `android+win` **103** /
`win_old` 30 / `win` 6 / `android+win_old` 6 / `android` 6 / `win+win_old` 2。

⇒ **同版本来源支持且互不矛盾 = 317 个名字键**（= 202+103+6+6+6+2 − 8 个矛盾）。

| 与 v1（`fixtures/name_to_addr.tsv`）对账 | 数 |
|---|---|
| v1 唯一锚 | **326 行 / 299 个名字键** |
| 本工具独立复现 v1 的名字键 | **278 / 299（93.0%）** |
| 其中地址与 v1 完全相同 | 270 |
| 其中地址与 v1 不同 | 8（逐条见 §37.7） |
| 本工具新落地（v1 里是歧义/未知） | **77** |
| v1 有、本工具没落地 | 21（其中 **10 条**是 v1 的「消去法」锚——本节**不做消去**；其余 11 条是规则差） |
| **⇒ 唯一锚（同版本口径）** | **299 键 → 317 键（净 +18）**；行口径 326 → 317 |

**外部真值逐项复核（不依赖本节，`观测`）**：拿 §18/§20/§36 已经独立定过的四条名字，
用 `key` 子命令把「某个 Location 键在四种构建里各被哪个函数引用」打出来：

| 校验用的 Location 键 | linux | win 2.0.60 | 该键在 Windows 侧的符号名 | 与旧文档一致？ |
|---|---|---|---|---|
| `src/graphwar_path_calculator.rs:86:37`（§20.5 的 9 个锚之一） | **0x467530** | 0x18004d490 | `GraphwarPathCalculator::calculate_paths` | ✅ §18/§20.5/§36.5 |
| `src/graphwar_path_calculator.rs:45:108` | **0x467530** | 0x18004d490 | 同上 | ✅ |
| `src/graphwar_shot_calculator.rs:181:59` | **0x3c2bc0** | 0x18011d640（该函数共 **13** 个 `src` 锚） | `GraphwarShotCalculator::calculate_shot_results` | ✅ §20.5/§36 |
| `src/math_function.rs:75:10` | **0x4ed670** | 0x18002ead0 | `MathFunction::new` | ✅ §18/§20.5 |
| `src/graphwar_target_manager.rs:35:24` | **0x465910** | **0 个**（Windows 侧连这个 Location 都没有） | — | ✅ 与 §36.5 的真值 `0x465910` 一致 |

前四条四向一致（`android` / `win` / `win_old` 的符号名逐字符相同，只有 `::h…` 后缀不同）；
第五条正好落在"唯一一个只在 Linux 侧"的键上，**Windows 帮不上忙、Android 帮得上**——
这也说明三向不是冗余，是互补。

**"Windows 独有"那批为什么几乎落不了（`观测`，逐条归因）**：

| 口径 | 名字数 | 落地 |
|---|---|---|
| `win − android` | **257** | **6（2.3%）** |
| `win − android − win_old` | 48 | 3 |
| `win − win_old` | 110 | 7 |

257 个的归因（可复算，`report` 会打印）：

| 原因 | 条数 | 判据 |
|---|---|---|
| ① 函数体**零** `src` Location | **212** | 这 212 个 Windows 函数的 `locrefs` 里没有一个 `src/*.rs` 键 ⇒ 这条路**永远**定不了名 |
| ② 有键但没有一个双向唯一 | **40** | 某一侧多重数 >1（Windows 侧 23,348 个函数引用 Location、Linux 侧只有 3,107，比例本来就不同） |
| ③ 有双向唯一键（应当已落地） | 5 | 落地了 6 行（有重名） |

⚠ 「206」这个数我**没复现出来**：本工具口径下 `win − android` 是 **257 个名字键**
（按"去哈希全串"口径是 **420**）。差别的根源是"唯一名"的定义（`name_key` 三元组 vs 全串），
见 §37.7 第 3 条。落地比例（约 2%）在两种口径下都一样低。

### 38.5 ③ 冲突清单

**（1a）一个源函数的多条唯一键指向两个不同的 Linux 地址：4 条**（`report` 逐条打印）。全部列出：

```text
win     <GraphwarPlane as IControl>::draw              9 票 → 0x4b5df0 | 1 票 → 0x456df0
win     GraphwarAudioControl::process                  9 票 → 0x4bfe10 | 1 票 → 0x456f80
win_old GraphwarLevelManager::setup_level             83 票 → 0x499740 | 22 票 → 0x49f510
android GraphwarGameStateManager::request_new_player   8 票 → 0x4739c0 | 1 票 → 0x3c1b40
```

成因（`推断`）：键集里混进了"被内联进来的别人的 panic 点"——例如 `GraphwarPlane::draw`
的键里有一条 `src/graphwar_function_manager.rs:208:37`，那条属于另一个模块的函数。
票数 9:1 也**不自动选**：本节按「宁缺勿错」记歧义。

**(1b) 最终记「歧义」不给地址的名字键：8 个**，全部是 `{{closure}}` / `__inner_init` 这类
"同一个源函数在 Linux 侧被拆成两份"的情形（与 §36.7 说的拆分同类）：

```text
entity_drawer::_::__init::__inner_init        android 0x4bc240  vs  win 0x4bbcc0
graphwar_lan_vbox::_::__init::__inner_init    android 0x4e9380  vs  win 0x4e8f80
graphwar_plane::_::__init::__inner_init       android 0x4bc490  vs  win 0x4bc860
player_showcase::_::__init::__inner_init      android 0x4e8b20  vs  win 0x4e8d70
GraphwarLevelManager::setup_level_0::{{closure}}   android 0x49e570  vs win 0x49e650
GraphwarLevelManager::setup_level_12::{{closure}}  android 0x4a5e80  vs win 0x4a6020
GraphwarLevelManager::setup_level_16::{{closure}}  android 0x4a91b0  vs win 0x4a93e0
GraphwarSetupControl::ready::{{closure}}          android 0x4d1f10  vs win 0x4d2f70
```

两条同版本来源各执一词、而两侧都"双向唯一"——这说明 Linux 里这两个地址**都**引用了
同一个键，`(1,1)` 唯一性在这个键上本来就不成立，属于真正的两难。

**(2) 同一个 `src` Location 键被 ≥2 个不同名字引用：262 个键**（`观测`）。三个成因，都不是错：

| 成因 | 例 |
|---|---|
| **gdext 样板共用一行**：`__register_methods` / `__register_constants` / `_::__init::__inner_init` 全指向 `src/<X>.rs:<行>:1` | `src/axis_drawer.rs:27:1`、`src/graphwar_button.rs:83:1`…（6 个类） |
| 递归/互递归共用一行 | `src/math_function.rs:184:24` ← `evaluate_recursive` / `evaluate_token` / `simplify_recursive` |
| trait 默认实现被多个 impl 共用 | `src/graphwar_game_client.rs:152:49` ← 4 个 `*Drawer::{draw,process,ready}` |

**(3) 同一个 Linux 地址被 ≥2 个名字认领：33 个地址**（`观测`）。绝大多数是
`X::handle_entity_removed_event` 与它的 `::panic_cold_explicit`（同一个冷块被两条符号指到）、
或 `{{closure}}` 与母函数共用锚点——**不是**"两个真名抢一个地址"。
真正需要人工复核的只有 4 条（上一行的"多键指向不同地址"）。

### 38.6 ④ 天花板之外：上限估计 **≈42 个**，正对照留出半 96.3%

**做法**（不改任何既有结论，只做"能不能再定名"的估计）：
query = Linux 目标函数，候选池 = **1,206 个有 `graphwarrust` 名字的 Windows 函数**
（只有它们能携带名字）。打分只用**与地址无关**的四个量：
`log 尺寸`、`log 指令数`、**助记符直方图 cosine**、**整数常量集合 Jaccard**；
候选窗口 = 尺寸比 `[0.5, 2.0]`。**完全不看 Location、不看名字。**

**正对照**：真值来自 Location（330 对，两侧都一对一），匹配器看不见它；真值按名字键 sha1
**分半**，窗口/权重/阈值只在调参半定，留出半只报数。

| | n | top-1 | 真值在 top-10 | 互为 top-1 | margin≥0.30 准确率 | 互为 top-1 时准确率 |
|---|---|---|---|---|---|---|
| 调参半 | 175 | 47.4% | 81.7% | 38.9% | **96.3%**（n=27） | **100%**（n=25） |
| **留出半** | 155 | 40.0% | **81.9%** | 30.3% | **96.3%**（n=27） | **100%**（n=26） |
| 随机基线 | — | ≈1/322 = **0.31%** | — | — | — | — |

⇒ 匹配器**是工作的**（比随机高 130 倍，真值 82% 落在候选集里），
"高置信"档（领先次名 ≥0.30）在留出半上 96.3%，"互为 top-1"档 **100%**。

**消融（`ablate`，调参半上的 top-1）——把"任务书点名的被调者多重集"这条负结果写下来**：

| 变体 | top-1 | Δ |
|---|---|---|
| 全量（默认权重） | 47.4% | — |
| 去掉 `hist` | 34.3% | **−13.1%** ← 主力 |
| 去掉 `const` | 35.4% | **−12.0%** ← 第二主力 |
| 去掉 `size` | 46.9% | −0.6%（窗口已经用了尺寸） |
| 去掉 `insn` | 52.6% | +5.1%（但高置信档准确率 96.3%→92.0%，见下） |
| **加「被调者多重集」（按实测 0.38 归一）** | **39.4%** | **−8.0%** |

**被调者多重集为什么救不了**（`观测`）：两个构建的「被调者数」之比中位数是 **0.38**
（Linux 侧更少，因为 LTO/ICF 把子调用内联进去了；p25 0.27 / p75 0.50），不归一化会系统性
把正确答案打低分；归一化后仍然掉 8 个百分点。**它是跨构建最不可比的一项**，不是没试。

`insn` 的取舍：去掉它 top-1 更好（52.6%），但 margin≥0.30 的准确率从 **96.3% 掉到 92.0%**。
本节的产出是"高置信档能落地多少"，所以我们**保留** `insn`——这是在调参半上做的选择，留出半只用来复核。

**天花板之外的实际数字**（Linux 12,736 个无 `src` Location 的函数）：

| 阈值 | 有 top-1 候选 | 单射（该 Windows 名只被一个 Linux 函数认领） |
|---|---|---|
| margin ≥ 0.05 | 3,244 | 60 |
| margin ≥ 0.10 | 1,287 | 52 |
| margin ≥ 0.20 | 409 | 45 |
| **margin ≥ 0.30** | 201 | **44** |

⇒ **上限估计 = 44 × 96.3% ≈ 42 个**（天花板 2,035 之外的"可能再定名"）。
**这就是"不硬凑"的答案**：不是 11,468，也不是 1,287，而是**几十个**，并且每个都得再验。

**两条必须写明的偏差**：
1. **选择偏差**：正对照的真值函数都带 Location（代码形态更独特），而 12,736 个无锚函数里
   大量是纯数学 / 闭包 / `drop_in_place` shim ⇒ 42 这个数**偏乐观**，真实只会更少。
2. **阈值是在调参半上挑的**，虽然留出半复核了，但 `SIZE_WIN`/权重这些超参在整个真值集上被看过一眼，
   独立性比 Location 那条链弱一档。

### 38.7 与既有文档 / 夹具冲突的地方（逐条，就地说明）

1. **§36.4 说 `MathFunction::remove_offset` 的真值是 `0x4eb830` → 本工具判 `0x4eb86a`（`观测`）**。
   证据：三方来源（android + win2.0.60 + early2026）一致地把该名字的 2 个双向唯一键
   （`src/math_function.rs:52:43`、`53:43`）指向 **`0x4eb86a`**（182 字节 / 53 条指令 / 4 条间接调用）。
   而 `0x4eb830` 只有 **58 字节 / 19 条指令 / 0 个 Location 引用**（`probe 0x4eb830 0x4eb86a`）。
   §36.4 那句是走 `sym_match` 裁决时写的，那句自己也说"候选集里不含真值"——**以本节为准**。
2. **v1 夹具里 8 条地址与三方证据不一致**（其余 270 条一致）。两类：
   * v1 记录的是"拆分后的两个地址"（如 `setup_level_4` 有 `0x49f510`/`0x49fea0`，三方证据只支持 `0x49f510`，
     22 票）；本节只给票数最高的那一个地址。
   * `setup_level_10/11/12/13` 在 v1 里分别是 `0x4a5390`/`0x4a5940`/`0x4a6020`/`0x4a63d0`，
     而 **early2026** 的同名函数一致指向 `0x499740` —— 这正是 §37.4 把跨版本单列的原因
     （early2026 的行号与 2.0.60 不同，`setup_level_10` 在旧版本里带的 25 个键落到另一个函数上）。
   * 21 个 v1 唯一锚本工具**没复现**（如 `graphwar_level_manager::GraphwarLevelManager::setup_level_0..3`、
     `graphwar_tick_handler::GraphwarTickHandler::tick`）：其中 10 条是 v1 的「消去法唯一」
     （§20.3 记 16 条消去法锚），本节**不做消去**，只认双向唯一；其余 11 条是抽取规则差
     （口径差：本节的 arm64 引用规则比 `name_bridge.py` 窄，带锚函数 **1,571 vs 1,722**；
     差的是哪几种寻址形态本节**没逐条分离**，`未知`。同一套规则放到 x86 侧两侧一致：3,107）。
3. **「Windows 独有 206 个」这个数没能复现**：本工具口径 `win − android` = **257 个名字键**
   （"去哈希全串"口径 = **420**）。两种口径下落地率都只有 **2% 量级**（6 / 257；若按全串口径
   落地 6 个 / 420）。原因是硬的（212/257 函数体内零 `src` Location），不随口径变。
4. **§18/§20 的 2,035 与本节一致**（Linux 2,035 / Android 2,035），本节没有推翻它，
   只是补了 Windows 2.0.60 的 2,051 与三方交集 2,034。

### 38.8 局限（`未知`，别当结论）

* **两份 Windows 构建的函数数差 1.2 万**（51,985 / 43,096），Linux 只有 13,263 ——
  Linux 侧 LTO/ICF 把大量函数合并/内联掉了。结构相似度只在"尺寸接近"的那个子集里可用
  （正对照的 size 比中位数 1.01、p10 0.96、p90 1.10，说明**能对上的那批几乎等长**；
  对不上的根本没进候选窗口）。这个不对称的成因（ICF？`-C opt-level` 不同？）**没有证据**，不写。
* **PE 的 `.pdata` 只覆盖 `.text` 的 88.6%**（0x8f1fc/12 = 48,853 条，覆盖 0x13fb914 / 0x14b50b8）。
  剩下的 11.4% 用 COFF FUNC 符号补（7,213 个符号落在已覆盖的函数体内、被并入），
  漏网的极可能是没有 unwind 信息的叶子函数。**没有逐条核对**。
* **Windows 侧 212 个"零 Location"名字**（§37.4）是硬边界：它们不是"没算出来"，是**没有键**。
* **`{{closure}}` 与母函数、`panic_cold_explicit` 与本体共用锚点**导致 33 个地址多名字——
  本节不裁决（§36 的 `modblock`/`score` 规则能裁一部分，本节没引入，避免与 §36 的口径打架）。
* **上限估计 42 没有逐条人工复核**。要落成名字，得对那 44 条（margin≥0.30 的单射）逐条看反汇编。

### 38.9 怎么复算（不依赖本节，任何人可重跑）

```bash
# 0) 卡座：capstone 在仓库 .reference/venv 里；四份构建都在 .reference/ 下（只读）
export PY=/…/.reference/venv/bin/python

# 1) 全量扫描 + 写夹具（首次约 2 分钟：反汇编 13,263 + 12,609 + 51,985 + 43,096 个函数）
$PY tools/three_way_match.py build

# 2) ①②③④ 的全部数字（约 45 秒，缓存命中后）
$PY tools/three_way_match.py report

# 3) 正对照：调参半 / 留出半 + 阈值选择 + 天花板之外的上限估计
$PY tools/three_way_match.py verify

# 4) 特征消融（调参半）
$PY tools/three_way_match.py ablate

# 5) 单条复核
$PY tools/three_way_match.py probe 0x3c2bc0 0x4eb830 0x4eb86a      # 某地址在四种构建里的样子
$PY tools/three_way_match.py key src/graphwar_shot_calculator.rs:86:37
$PY tools/three_way_match.py nm calculate_shot_results

# 6) 只看夹具（不需要 capstone）
awk -F'\t' 'NR>6{print $8}' fixtures/name_to_addr_v2.tsv | sort | uniq -c
```

输入路径可用 `--linux / --android / --win / --win-old / --funcstarts` 或环境变量
`GW2_LINUX_SO` / `GW2_ARM64_SO` / `GW2_WIN_DLL` / `GW2_WIN_OLD_DLL` / `GW2_FUNCSTARTS` 覆盖。
特征缓存落在 `.agent-work/threeway/cache/`（gitignore），key 里带 `CACHE_VERSION`
（本节为 `tw-v3`）——**改了抽取逻辑必须同时抬版本号**，否则会静默吃旧缓存（§36.0 的坑）。

### 38.10 产物：`fixtures/name_to_addr_v2.tsv` 的列

```text
module  type  method  x86_addr  src_file  src_line  src_col  join_kind  sources  evidence
```

* `x86_addr`：Linux 2.0.60 的 **ELF 虚地址**；Ghidra/REA 地址 = 该值 + `0x100000`（只加一次）。
  空 = 没落地。
* `join_kind` ∈
  **`唯一锚`**（同版本来源支持且互不矛盾，**317 行**）/
  **`跨版本锚`**（只有 early2026 支持，**30 行**，不进唯一锚计数）/
  **`歧义`**（同版本来源矛盾，**8 行**）/
  **`未知`**（无锚，**625 行**）。
* `sources`：这个名字在哪些构建的符号表里出现（`win`/`win_old`/`android` 的组合）。
* `evidence`：`votes=该地址拿到的双向唯一键数;keys=其中前 4 个键;src=真正落地它的来源;xver=early2026 是否一致;win_addr=Windows 侧地址`。
* 行数 **980** = 980 个**名字键**（`(module,type,method)` 去重）。与 v1 的 **743** 行不是同一分母：
  v1 的 327 行带地址（326 唯一锚 + 1 消去法）只有 **300** 个名字键（同一名字的多行是"拆分/消去"产物）。

**可复现性（这次是验过的，不是"应该没问题"）**：删掉缓存冷启动重建一次（1 分 42 秒）
与缓存命中重跑的夹具 **md5 完全相同**（`956eae05119f3703e5dcc7c6d320c88d`，`tw-v4` 冷/热两跑一致），
另外两个 `PYTHONHASHSEED`（0 / 7）也相同。§36.0 那条"入库夹具不能由入库工具复现"的坑，
本节**没有**再踩：`CACHE_VERSION=tw-v3` 与夹具一起提交，抽取逻辑一改就必须抬版本号。

## 39. 符号名判据统一：什么叫「这条符号是 `graphwarrust` 的函数」（2026-11）

> 接 §20（Location 桥）/ §35（无锚配对）/ §37（Windows 符号表）/ §38（三向匹配）。
> 工具：`tools/rust_sym.py`（判据唯一实现）、`tools/name_crate_audit.py`（普查夹具）、
> `tools/name_accuracy_check.py`（正对照）；
> 产物：`fixtures/name_crate_audit.tsv`（**新增**）、`fixtures/name_to_addr.tsv`（多一列 `sym_form`）。

### 39.0 一句话 + 四问四答

判据从「mangled 里有 `graphwarrust` 这个子串 / 以 `_ZN12graphwarrust` 开头」改成
**按 legacy mangling 的十进制长度前缀切段、取路径首段的 crate**（§39.1）。
改完之后每个构建能认出的名字都变多了，但**已交付的两项指标一个都没变** —— 原因见 §39.4，
这不是失败，而是「上一轮已经把该补的补上了，而 `name_bridge`/`sym_match` 用的是**更宽**的判据」。

| 问题 | 结论 | 档位 |
|---|---|---|
| **(①) 判据是什么** | `_ZN<len><elem>…17h<hex>E` 按长度前缀切段；路径首段（或 `<SELF as TRAIT>` 的 SELF 首段）= crate 名；等于 `graphwarrust` 才算。**不是子串匹配** | `观测`（结构可证，反例见 §39.1） |
| **(②) 三个构建各多认出多少** | 前缀口径 → 统一判据：Android **465 → 574**（+109）/ Windows 2.0.60 **647 → 823**（+176）/ Windows early2026 **659 → 775**（+116）；**目标 Linux 0 → 0**（被 strip，没有符号表，名字只能从上面三个构建桥过来） | `观测`（`fixtures/name_crate_audit.tsv` ① 段） |
| **(③) 唯一锚 / 可定名地址** | **334 → 334**；**351 / 527 = 66.6% → 351 / 527 = 66.6%** | `观测` |
| **(④) 正对照** | 从 351 里随机 30 条（seed 20261107）：**入口 30/30、锚归属/溯源 30/30、两侧名字一致 26/26**；换 seed 抽 50 条：入口 50/50、归属 50/50、两侧一致 44/45（唯一不一致 = §39.5 已列冲突 `0x456df0`） | `观测` |
| **(⑤) 冲突** | 跨构建地址级冲突 **7** 条（逐条列在 §39.5）；`name_bridge` 内部硬矛盾 **9** 条；Windows 侧「同一 Location 多名字」**54** 个三元组 | `观测` |

### 39.1 判据：结构解析，不是子串

rustc 的 legacy mangling（`-C symbol-mangling-version=legacy`，本版二进制用的就是它）是
**带长度前缀**的路径编码：

```text
_ZN <elem> <elem> … 17h<16 hex> E         每个 <elem> = <十进制长度><该长度的字节>
```

长度前缀让「段的边界」可以**证**出来，于是「属于哪个 crate」= **路径首段**：

| 形态 | 例 | crate |
|---|---|---|
| `direct` | `_ZN12graphwarrust22graphwar_game_recorder…` | `graphwarrust`（`12` 之后恰好 12 字节） |
| `impl-head` | `_ZN133_$LT$graphwarrust..axis_drawer..AxisDrawer$u20$as$u20$godot_core..X$GT$…` | `graphwarrust` —— **crate 名没有长度前缀**，整数段 `133` 数的是整个 `_$LT$…$GT$` |
| `nested-impl` | `_ZN12graphwarrust17graphwar_event…$LT$impl$u20$serde..Serialize$u20$for$u20$…$GT$…` | `graphwarrust`（derive 的匿名 impl 出现在中段） |
| `other-crate` | `_ZN4core3ptr…drop_in_place$LT$…graphwarrust..X…$GT$…` | `core` —— **排除** |

**为什么不能用子串**（不是洁癖，是两个方向都会错）：

* 「以 `_ZN12graphwarrust` 开头」会把 `impl-head` **整类丢掉**（crate 名前面没有 `12`）；
* 「正文里有 `graphwarrust`」会把 `core::ptr::drop_in_place<…graphwarrust::X…>` 收进来，
  再让 `parse_name` 从**泛型实参**里抠出一个 `(module, type, method)` —— 那个「名字」不是源码里的函数。
  本版这类符号：Android **304** 个、Windows 2.0.60 **327** 个（`fixtures/name_crate_audit.tsv` ② 段）。

判据实现只有一份：`tools/rust_sym.py::classify / crate_of / is_game_name / form_label`；
`name_bridge.py` / `win_names.py` / `three_way_match.py` / `sym_match.py` 全部转发到它。

### 39.2 三个构建各多认出多少名字

`fixtures/name_crate_audit.tsv` ① 段（数字可直接复算，见 §39.8）：

| 构建 | 符号 | 游戏自有 | 其中 direct | impl-head | nested-impl | 逻辑名 prefix | 逻辑名 unified | 逻辑名 coarse |
|---|---|---|---|---|---|---|---|---|
| Android 2.0.60（arm64 `.symtab`） | 17,875 | **970** | 740 | 230 | 13 | 465 | **574** | 584 |
| Windows 2.0.60（COFF `.text`） | 77,510 | **1,402** | 1,018 | 384 | 95 | 647 | **823** | 834 |
| Windows early2026 | 56,430 | 1,447 | 1,009 | 438 | 93 | 659 | 775 | 784 |
| **Linux 2026-08-28（目标）** | 131 | **0** | 0 | 0 | 0 | 0 | 0 | 0 |

* 「逻辑名」= 哈希归一后的 `(module, type, method)` 唯一键数。
* 目标本身 **0 个名字**（stripped，`.symtab` 只剩 131 个非 FUNC 条目）——它的名字永远是
  「从另外两个构建经 `panic::Location` 桥过来」的结果，所以 ② 段对它的增量恒为 0；
  真正变化的是**可定名地址**（§39.4）。
* 上一轮 §37.0 写的「Android 520 个逻辑名」是旧口径的估计，实测是 574（unified）/ 584（coarse）。

### 39.3 「多出来的名字」逐条来源

**（a）前缀口径丢掉的那一整类**：Android 109 / Windows 176（early2026 116）个名字键
全部来自 `impl-head` 与 `nested-impl`（GDExtension 的 `__register_methods`、`class_id`、
`INode::ready`、`INode::process`、`IUserClass::…`、以及 `#[derive]` 的
`serialize`/`deserialize`）。**这些是原版源码里的函数**，不是胶水。
证据：`fixtures/name_crate_audit.tsv` ① 段 `keys_missed_by_prefix` 列；逐条符号可用
`readelf -sW` + `rust_sym.form_label` 复算。

**（b）结构解析救回的**：**1 条**（`fixtures/name_crate_audit.tsv` ③ 段）

| 构建 | 名字键 | 符号 | 落地址 |
|---|---|---|---|
| Windows 2.0.60 | `(GraphwarExtension, _, on_stage_init)` | `_ZN86_$LT$graphwarrust..GraphwarExtension$u20$as$u20$godot_core..init..ExtensionLibrary$GT$13on_stage_init17hbb4cce76204560b0E` | **否** |

为什么旧版丢：自类型直接在 crate 根（`graphwarrust::GraphwarExtension`），**没有 `<module>` 段**，
而旧 `parse_name` 的兜底正则要求 `graphwarrust::X::Y` 两段 ⇒ 匹配不到 ⇒ 整条符号返回 `None`。
新解析按 `<SELF as TRAIT>` 结构取 SELF，给出 `(GraphwarExtension, _, on_stage_init)`。
它**没有 Location 锚**（Windows 侧不引用任何 `src/*.rs` 常量），所以落地地址 **+0**。

**（c）收紧后被去掉的**：Android **10** 个键 / Windows **11** 个键（`fixtures/name_crate_audit.tsv` ② 段
逐条列出，含示例 mangled）。全部是 `other-crate` 胶水产生的假名字 —— 12 个里 2 个与真名字撞键
（`graphwar_root_node::GraphwarRootNode::ready`、`graphwar_setup_control::GraphwarSetupControl::ready`），
所以去重后是 10。**它们落地 0 个地址**（② 段 `lands_addr` 列全 0）：`name_to_addr.tsv` 因此少了
15 行「未知」，而带地址的行一行没少。

### 39.4 为什么 334 / 351 没变（负结论 + 它成立的证据）

先把三个数拆开：

| 指标 | 构成 | 本轮 |
|---|---|---|
| **唯一锚 334** | `fixtures/windows_name_to_addr.tsv` 里 `join_kind=唯一锚` 的行数（332 个不同地址，2 个地址被两个函数共享） | **334（不变）** |
| **Android 可定名地址 325** | `fixtures/name_to_addr.tsv` 里带地址的行（326 唯一锚 + 1 消去法 = 327 行 / 325 个地址） | **325（不变）** |
| **Windows 可定名地址 333** | 唯一锚 332 + 集合锚 1 | **333（不变）** |
| **并集** | 325 ∪ 333（交集 307，Android 独有 18） | **351（不变）** |

**为什么不变**（三条，缺一条这个负结论就不成立）：

1. **「前缀判据」的缺口上一轮已经补进 `win_names.py` 了**。§37.0 的 325 → 351（+26）
   正是补这个缺口的结果：假如仍用 `_ZN12graphwarrust` 前缀，`fixtures/windows_name_to_addr.tsv`
   里会有 **26 个已交付名字 / 26 个地址**认不出来（`fixtures/name_crate_audit.tsv` ④ 段）。
   ⇒ 上一轮「可能多钉住 25 个」的估计**成立**，而且**已经兑现**（本轮只是把它写成可复用判据并复算）。
2. **`name_bridge.py` / `sym_match.py` 用的从来不是前缀判据，而是更宽的「含子串」**。
   所以本轮把判据换成 unified 只会**去掉**胶水假名字（§39.3c），不会增加名字 —— 实测也是 0 增量。
   ④ 段还给出同一件事的另一面：若真的把前缀判据用到 Android 侧，会丢 **28 个已交付地址**。
3. **多出来的那 1 个名字没有锚**（§39.3b），所以不产生地址。

**这就是负结论的正对照**：不是「扫不出来」，而是「扫出来了但落地不了」，并且**同一套扫描在
已知真值上是准的**（§39.6 的正对照：入口 30/30、锚归属 30/30、两侧一致 26/26）。

### 39.5 冲突（同 Location / 同地址不同名）—— 一律列出，不静默取一个

**（a）跨构建、地址级冲突 7 条**（`win_names.py report` ⑤ 段；Android 名 × Windows 名）：

| Linux 地址 | Android 名 | Windows 名 | 解释 |
|---|---|---|---|
| `0x3a2e10` | `root_node::ready` | `root_node::ready` + `button_manager::ButtonManager::new` | Windows 侧该函数**内联了** `ButtonManager::new`（17 个 src 三元组里 2 个属于 button_manager）⇒ 一个函数体两个来源名 |
| `0x3e2830` | `color_manager::set_color` + `game_state_manager::report_fired_function` + `remote_event_handler::handle_server_remote_event` | `remote_event_handler::handle_server_remote_event` | Android 侧三个名字共用同一批锚：该函数是**内联容器**（103 个 src 三元组，3 个模块的文件都在里面） |
| `0x4500c0` | `game_server::game_server_thread` | 同上 + `server_connection_handler::new` | 同上（76 个三元组，66 个属 game_server） |
| `0x45b020` | `server_connection_handler::tick` | `server_connection_handler::subscribe_to_events_event_handler` | **集合完全相等**（8/8）⇒ 两个名字在两侧指同一段代码；这是「定义点 vs 内联容器」的典型，不是错名 |
| `0x465780` | `target_manager::subscribe_to_events_event_handler` | `target_manager::send_all_info_events` | 同上（1/1） |
| `0x465870` | `target_manager::subscribe_to_events_event_handler` | `target_manager::handle_entity_removed_event` | 同上（1/1） |
| `0x519d80` | `effect_manager::tick` | `effect_manager::subscribe_to_events_event_handler` | 同上（2/2） |

一致 **300** / 不一致 **7**。

**（b）`name_bridge` 内部硬矛盾 9 条**（同一个 arm64 函数被双向唯一锚指向多个 x86 地址 ⇒ 该函数在
x86 侧被拆开）。`name_bridge.py conflicts` 直接打出，代表例：

* `graphwar_game_recorder::GraphwarGameRecorder::save_game`（arm64 `0x3d8184`）→
  `0x508aba`（2 票）/ `0x3d5740`（2 票）/ `0x50b130`（1 票）。**三票之差的消去法**，见 §39.7；
  独立证据（Windows 侧唯一锚 2/2、`sym_match` 的 unique 候选、`name_to_addr_v2` 三方一致）都指向
  **`0x3d5740`**，夹具按证据取它。
* 其余 8 条里 7 条是 `?`（无游戏名字的胶水函数），只有 `graphwar_game_state_manager::request_new_player`
  有名字（`0x4739c0` 8 票 vs `0x3c1b40` 1 票）—— 夹具按 §20 的规则**不给地址**（保持「歧义」）。

**（c）同 Location 多名字（不是冲突，是共用锚）**：Windows 侧 54 个三元组被 ≥2 个函数引用。
这属于内联的常态，不影响 join（唯一性是在**两侧**同时判定的）。

### 39.6 正对照（`tools/name_accuracy_check.py`）

抽已知真值，四项独立检查：`entry`（地址必须是 Ghidra 函数起点）、`anchor`（该函数引用的
Location 里必须有 `src/<module>.rs`；若只引用依赖 crate 的 Location，则改查「夹具展示的那个锚
是否真在该函数的引用集合里」）、`cross`（两个构建都钉住时名字键必须相交）、`stable`：

```text
$ .venv/bin/python tools/name_accuracy_check.py --sample 30
可定名地址（两侧并集）：351 个（android 325 / windows 333）
准确率：入口 30/30 = 100.0%；锚归属/溯源 30/30 = 100.0%（其中 src 模块匹配 29、仅依赖 Location 溯源 1）
两侧都钉住的 26 个里，名字一致 26 = 100.0%（不一致 0）

$ .venv/bin/python tools/name_accuracy_check.py --sample 50 --seed 7
准确率：入口 50/50 = 100.0%；锚归属/溯源 50/50 = 100.0%（其中 src 模块匹配 50、仅依赖 Location 溯源 0）
两侧都钉住的 44 个里，名字一致 44 = 100.0%（不一致 0）
```

唯一一条「仅依赖 Location 溯源」的样本：`0x519450`（`graphwar_tick_handler::GraphwarTickHandler::get_time_to_tick`），
它只引用 `library/core/src/time.rs:201` 与 `:1181` —— 归属靠「两侧都唯一引用这两个 std Location」，
**不是**靠 `src/*.rs` 的模块文件。这一档在夹具里本来就写的是 dependency 路径（`src_file` 列），没有虚报成模块锚。

### 39.7 顺带修掉一个真 bug：工具输出曾随 `PYTHONHASHSEED` 变

`name_bridge.py` 的消去法（§20.5）是**顺序敏感**的贪心：某一步「只剩一个候选」是在**当时已占用**
的地址集合上判定的。而 ④ 的 `owner_a/owner_b` 是按 `for k in s`（`s` 是 `(file,line,col)` 字符串
元组的 **set**）插入的 ⇒ Python 的 per-process 哈希随机化会改变 `shared` 的顺序 ⇒ 不同的「只剩一个」。

实测（同一份二进制、同一个命令，只换 `PYTHONHASHSEED`）：

```text
seed=0  save_game=0x50b130      seed=3  save_game=0x3d5740
seed=1  save_game=0x508aba      seed=4  save_game=0x3d5740
seed=2  save_game=0x508aba
```

影响 **6 行**（`assign` 621 条里的 6 条），其中 `save_game` 一行正是夹具里那条手工裁决。
修法：把「顺序会泄漏到结果」的迭代全部钉成字典序/数值序（`sorted(anch_a.items())`、
`for k in sorted(s)`、`for fa in sorted(anch_a)` …）。修后 4 个 seed 的输出 **md5 完全相同**。

**顺带**：夹具里的 4 条手工裁决（它们的"为什么要改"逐条写在 `docs/16-client.md` §27.4 的审计表里）（`remove_offset` `0x4eb86a→0x4eb830`、`to_string_recursive`
`0x4eccb3→0x4ec160`、`save_game`→`0x3d5740`、`mutate_function` 的 `join_kind`）原来只写在注释里，
**重跑 `build` 就会被覆盖**。现在内建为 `name_bridge.py::MANUAL_FIXES`，`build` 直接产出与入库
夹具一致的 743 行（`git diff` 只剩「去掉 15 行胶水 + 加一列 `sym_form`」）。

第一次实现时踩的坑：`MANUAL_FIXES` 的键把 `src_line` 写成字符串，而行里是 `int` ⇒ 裁决**静默不命中**。
现在统一 `str()`，并在 `apply_manual_fixes` 里写明为什么。

### 39.8 夹具与工具变更清单（都能一键复算）

| 文件 | 变更 |
|---|---|
| `tools/rust_sym.py` | **新增**：判据唯一实现（`classify`/`crate_of`/`is_game_name`/`form_label`/`impl_head_tail`） |
| `tools/name_crate_audit.py` | **新增**：三口径普查 → `fixtures/name_crate_audit.tsv` |
| `tools/name_accuracy_check.py` | **新增**：正对照（§39.6） |
| `tools/name_bridge.py` | 符号名来源改用 `rust_sym`；`parse_name` 增补 `<SELF as TRAIT>` 分支；④ 迭代排序（§39.7）；`MANUAL_FIXES`；夹具加 `sym_form` 列 |
| `tools/win_names.py` | `is_game_name` 转发到 `rust_sym`（行为不变：Windows 仍是 1,402 raw / 823 逻辑名 / 334 唯一锚） |
| `tools/sym_match.py` | `parse_name` 增补 `<SELF as TRAIT>` 分支 + 胶水符号不出名字键；**符号宇宙**保持「提到本 crate」（配对表不是名字表，见函数 docstring）；夹具 **1,300 行不变**（重建后 md5 与入库一致 `db252b8d…`） |
| `tools/three_way_match.py` | 名字筛选改用 `rust_sym`；`CACHE_VERSION` `tw-v3 → tw-v4` |
| `fixtures/name_to_addr.tsv` | 758 → **743** 行（去掉 15 行胶水假名字，带地址行一行没少）；**新增 `sym_form` 列**（`direct` 637 / `impl-head` 93 / `nested-impl` 13） |
| `fixtures/name_crate_audit.tsv` | **新增**（四段：普查 / 胶水键 / 结构解析救回 / prefix-only 的 what-if） |
| `fixtures/windows_name_to_addr.tsv` | **未改**（重建逐字节相同） |
| `fixtures/name_to_addr_v2.tsv` | 990 → **980** 行：去掉 10 行胶水假名字、新增 1 行 `GraphwarExtension::on_stage_init`，**两边都没有地址**；`唯一锚 317 / 跨版本锚 30 / 歧义 8` 三档行数**一行未变**，只有 `未知` 635→625（§39.9）。`tw-v4` 冷/热两跑 md5 一致 |

复算命令：

```bash
$PY tools/name_crate_audit.py                    # 写 fixtures/name_crate_audit.tsv（约 10 s）
$PY tools/name_bridge.py build                   # 写 fixtures/name_to_addr.tsv（约 20 s）
$PY tools/win_names.py bridge                    # 写 fixtures/windows_name_to_addr.tsv（约 90 s）
$PY tools/sym_match.py build                     # 写 fixtures/sym_match.tsv（约 30 s）
$PY tools/name_accuracy_check.py --sample 30     # 正对照（约 25 s）
for s in 0 1 3 7; do PYTHONHASHSEED=$s $PY tools/name_bridge.py build --out /tmp/s$s.tsv; done
md5sum /tmp/s{0,1,3,7}.tsv                       # 四份必须相同（§39.7）
```

### 39.9 对既有章节的影响（就地更正）

* §35.1(b)(c) 的 `unique 655 + weak 78 / multi 63 / split 35 / merge 25` 与入库夹具**不符**：
  按 `fixtures/sym_match.tsv` 实数应为 **unique 681 / weak 81 / multi 37 / none 444 / split 33 / merge 24**
  （和仍是 1,300）。改用与 §35 同一口径的算式：
  唯一匹配 **762**（unique 681 + weak 81）、落地 **819/1300 = 63.0%**（681+81+33+24）。
  这几行是更早一次 build 的数字，本轮复算时发现并就地改正
  （`cargo test` 的 `fixture_has_1300_rows` 只锁行数，锁不住分布）。
* §37.0 的「Android 520 个逻辑名」→ 实测 **574**（unified）/ 584（coarse）（§39.2）。
* §38.10 的「v1 的 759 行」→ 现在 **743 行**（§39.3c）；
  §38.10 的 `未知 635 行` / `行数 990` → **625 / 980**（地址档位一行未变，见 §39.8 那一行）。

---

## 40. 无名函数的归属：179 个「有 `Location` 但三份符号表都没名字」的函数 —— 138 个能给出源函数名，41 个只能到角色/未知（2026-11，接 §37 / §38 / §39）

> **编号说明**：任务书（`dispatch/30-unnamed-176.md`）预分配的是 §38，但 §38 / §39 已被并行任务占用，
> 本节取 **§40**（冲突由协调者合并）。
> **工具**：`tools/unnamed_attrib.py`（`build` / `list` / `control` / `attrib` / `probe`）；
> **夹具**：`fixtures/unnamed_attrib.tsv`（**新增**，179 行，本节表格由它生成）。

### 40.0 一句话结论 + 四问四答

**527 个可归属函数里已经有名字的 348 个（§37 / §38 / §39 的并集），剩下 179 个没有任何一份符号表认得。**
本节不猜名字，只做三件事：把每个无名函数的**模块**钉死（`Location` 的 `file` 直接给出）、
在**同模块、同 `Location` 键**的 Windows 有名函数里按**结构**排序找源函数、
再用**地址级正对照**量出这套排序到底准不准。结论：**138 个能给出源函数名（推断）、41 个只能写未知**；
其中 94 个是「单态化族」——**源函数名可信，缺的只是类型参数**。

| 问题 | 结论 | 档位 |
|---|---|---|
| **(a) 模块** | **179 个全部有模块**（`Location.file` 的 stem，多文件按锚点数取多数、与 `coverage_matrix.attribute` 同规则）；分布在 **27 个模块**（`weak_caller` 39 / `graphwar_module_container` 20 / `graphwar_game_client` 20 最多） | **观测** |
| **(b) 对应另一个构建的哪个函数** | **138 个**给出了 Windows 2.0.60 的对应函数（其中 **20 个**是 1:1、**94 个**是单态化族、**24 个**是「二选一」的注册样板）；**41 个**没有（其中 **39 个**的真对应体在 Windows 侧**本身就没有名字**） | **推断**（准确率见 §40.3） |
| **(c) 角色** | 24 个「二选一」的注册样板仍写得出角色（「该类的 GDExtension 注册样板之一」）；41 个未知里，**16 个**能写「最接近的有名候选 `X`」、**7 个**能写「被已定名的 `Y` 调用」、**11 个**有字符串证据，其余只能写「对应 Windows 无名函数 `0x…`」 | **推断**（族级）+ **未知**（个体） |
| **(d) 任务书的 176 差在哪** | **176 = 527 − 351**，但那个 351 里有 **6 个地址根本不在 527 内**（它们不引用任何 `src/*.rs` Location）；把分子与分母统一后是 **527 − 348 = 179**。所以本文给 **179**（包含任务书的 176 个），逐条见 §40.1 与 §40.7 | **观测** |

### 40.1 分母：527 / 348 / 179 —— 以及任务书的「176」差在哪

`527` 是「引用了 `src/*.rs` 的 `panic::Location`」的函数数（Ghidra 起点表口径，
`tools/unnamed_attrib.py list` 与 `tools/coverage_matrix.py --check` 两条独立链路打同一个数）。

「有名字」取**三份夹具的全集**（口径最宽 = 对"还没名字"最保守：宁可少数，不可把已有名字的算进无名）：

| 来源 | 可定名地址（全部） | 其中落在 527 内 |
|---|---|---|
| `fixtures/name_to_addr.tsv`（Android arm64 `.symtab` 桥过来的，含 1 条 `消去法`） | 325 个地址 | **319** |
| `fixtures/windows_name_to_addr.tsv`（Windows 2.0.60 锚，`唯一锚` + `集合锚`） | 333 个地址 | **331** |
| 上面两者的并集（= §37.8 那个 **351**） | 351 个地址 | **345** |
| `fixtures/name_to_addr_v2.tsv`（三向，`唯一锚` 310 + `跨版本锚` 27）补进来之后 | 354 个地址 | **348** |
| **⇒ 527 − 348** | | **179 个无名** |

**那 6 个「有名字但不在 527 内」的地址，逐个列出来**（它们**不引用任何 `src/*.rs` 的 `Location`**，
所以根本不是"可归属函数"，§37.8 的 351 把它们算进分子了）：

| 地址 | 夹具给的名字 | 它实际引用的 `Location` | 判断 |
|---|---|---|---|
| `0x3bd640` | `graphwar_root_node::Args::from_arg_matches_mut` | `clap_builder-4.5.11/src/parser/error.rs:32` | 第三方（clap） |
| `0x519450` | `graphwar_tick_handler::GraphwarTickHandler::get_time_to_tick` | `core/src/time.rs:201`（std） | 第三方（std） |
| `0x519530` | `…::get_fractional_tick` | `core/src/time.rs:201`（std） | 第三方（std） |
| `0x4eb830` | `math_function::MathFunction::remove_offset` | **一个 `Location` 都没有** | 靠人工裁决/内联注记落地 |
| `0x4ec160` | `math_function::MathFunction::to_string_recursive` | **一个 `Location` 都没有** | 同上 |
| `0x748000` | `math_function::FunctionRandomizer::mutate_function` | **25 个全在 `miniz_oxide-0.7.4/src/inflate/core.rs`** | **推断：这行 v1 结论可疑**（缺一次人工反汇编复核） |

⇒ 任务书的 **176 = 527 − 351**；把 351 换成交集 345 得 **182**；再把 v2 的三向落地并进来得
**179**。本节按 **179** 干活（它包含任务书的 176 个），差异全部登记在 §40.7。

### 40.2 判据：五条，**没有一条是「名字像」**

`AGENTS.md` §0.3 与第五轮 `prefix-fix` 的教训是同一条：名字相似度不是证据。本节的判据只有结构：

| 代号 | 判据 | 为什么它是结构而不是名字 |
|---|---|---|
| **K** | 两个函数引用**同一个** `panic::Location` 三元组 `(file,line,col)` | 三元组是编译期常量，跨构建 / 跨 ISA 都不变（§20.2） |
| **M** | 候选名属于 `Location.file` 对应的**同一个模块**（文件名 = 模块名） | 模块由同一份三元组的 `file` 字段给出，不由名字给出 |
| **S** | 结构相似度 = 尺寸 log 比 + 助记符直方图余弦 + 整数常量 Jaccard + f64/f32 常量 Jaccard + 字符串前缀 Jaccard | 只用字节形态，**不读名字、不读地址、不读 `Location` 键** |
| **E** | 消去：某候选名**已经唯一落在别的 Linux 地址**上 ⇒ 本函数上降权 0.15 | 「一个名字只对应一个地址」，与 v1 夹具的 `消去法` 同源 |
| **N** | 调用邻域：调用者 / 被调者有已定名的函数 | 调用图是结构（只作证据，不参与打分） |

候选池只放 **Windows 2.0.60**：同版本、同 ISA（x86-64）、带 COFF 名字。Android 是 arm64，
助记符直方图与 x86 不可比，所以**不参与**结构排序（它的名字经 `fixtures/name_to_addr.tsv` 早已并入名字集合）。

**特征消融（调参半，`control --ablate`）—— 两个负结论要一起读：**

| 变体 | top-1 | Δ |
|---|---|---|
| 基线（尺寸 .5 / 指令数 .5 / 直方图 1.0 / 整数常量 1.0 / f64 1.0 / f32 .5 / 字符串 .5） | 157/176 = 89.2% | — |
| 去掉直方图 | 156/176 = 88.6% | **−0.6** |
| 只用尺寸 | 153/176 = 86.9% | **−2.3** |
| 去掉 f64 / f32 / 字符串 / 整数常量 | 157/176（不变） | **0.0** |
| 加「被调者条数」 | 157/176（不变） | 0.0 |
| 不做模块过滤 | 157/176（不变） | 0.0 |

⇒ **只有「直方图 + 尺寸」在真出力**；f64/f32/字符串/整数常量对 top-1 **没有贡献**（它们仍然写进证据串，
因为那是**人读的证据**，但不要声称它们提升了准确率）。「被调者条数」与 TWM §37.6 的结论一致：不帮忙。
「模块过滤」在这里也是零增益 —— 因为 **K 判据本身已经把候选锁在同一批 `Location` 上**，模块几乎自动一致。

### 40.3 正对照：**地址级 98.8% / 100%**（这才是匹配器的准确率）

正对照的真值**不能**用名字：v1 夹具（`name_bridge`）与 `sym_match.name_key` 是两套 `parse_name`，
同一函数会被写成不同的 `(module,type,method)`。所以真值改用**地址**：`windows_name_to_addr.tsv` 的
`func_va` + `name_to_addr_v2.tsv` evidence 列的 `win_addr=` ⇒「这个 Linux 地址对应的 Windows 函数是哪个」，
共 **337 个**（全在 527 内）。这才是"同一个函数"的定义，与名字怎么拼无关。

按地址 hash 分半：**调参半定权重与阈值，留出半只报数**（`AGENTS.md` 合并纪律 A4）。

| 半 | 目标 | 有候选 | top-1 | top-3 | 「真身无名」闸门误报 |
|---|---|---|---|---|---|
| 调参半 | 174 | **173** | **171/173 = 98.8%** | 173/173 = 100% | **0 / 173** |
| 留出半 | 163 | **160** | **160/160 = 100%** | 160/160 = 100% | **0 / 160** |

分桶精度（调参半 / 留出半）：候选=1 → **136/136** 与 **134/134**（全中）；候选=2 → 11/11 与 5/5；
候选=3 → 7/7 与 3/3；候选≥4 → 17/19（89.5%）与 18/18。⇒ **多候选才是唯一会出错的地方**。

**名字级正对照（任务书要求的口径）也给，并解释差在哪：**

| 半 | 目标 | top-1 | top-3 | 名字级失败里，**Windows 地址其实对上了**的 |
|---|---|---|---|---|
| 调参半 | 178 | 157/176 = **89.2%** | 159/176 = 90.3% | 17 / 19 |
| 留出半 | 170 | 146/166 = **88.0%** | 146/166 = 88.0% | 20 / 20 |

⇒ 名字级 89.2% 与地址级 98.8% 的差 **几乎全部来自两套 `name_key` 解析的口径差**（留出半 20 个失败里
20 个的 Windows 函数是完全对的）。真正的匹配错误：调参半 **2/173**、留出半 **0/160**。

**`margin` 桶没有区分度**（`margin≥0.3` 占了 165/176，精度 89.7%），所以本节**没有**用 margin 当门槛，
改用两个**分布口径**的阈值（都在调参半上定）：

| 阈值 | 取值 | 依据（调参半） | 代价 |
|---|---|---|---|
| `GATE_RATIO`（候选 size 比窗口） | **[1/3, 3]** | 正确配对 ratio 的 p1 / p99 = **0.57 / 1.51**（留出半 0.81 / 2.14），取整并放宽 | 漏掉 **2/335** 个真配对（0.6%） |
| `MIN_SCORE`（结构相似度下限） | **1.0** | 正确配对 score 中位数 **2.37**、p5 1.48、min 0.57 | **会否掉 4/173 个真配对（2.3%）**，是**保守方向**的取舍（宁可少给名字） |
| `TIE_EPS`（打平） | **0.05** | 正对照里没有出现"打平却误判" | — |

### 40.4 结果：桶分布 + 模块分布

**179 个逐条落在 5 个桶里**（决策规则**先写死再看结果**，见 `tools/unnamed_attrib.py::decide`）：

| 桶 | 判据 | 个数 | 档位 | 能给出的东西 |
|---|---|---|---|---|
| **A 个体** | 结构最优有名，且该 Windows 函数只被本函数认领（1:1） | **20** | 推断 | 具体函数名 |
| **B 族** | 结构最优有名，但同键下 Linux 侧有 k 个同构函数（单态化族） | **94** | 推断 | **源函数名可信，类型参数未知** |
| **C 族歧义** | 两个**不同名字**同分（差 < 0.05） | **24** | 推断 | 「这两个之一」，名字不唯一 |
| **D 无名字** | 不筛名字时最优的 Windows 函数**本身没有名字** | **39** | 未知 | 只能写「对应 Windows 无名函数 + 调用邻域」 |
| **低分** | 结构相似度 < 1.0（形状不像） | **2** | 未知 | — |
| **合计** | | **179** | **推断 138 / 未知 41** | |

**模块 × 桶**（每行相加 = 该模块的无名函数数）：

| 模块 | 合计 | A 个体 | B 族 | C 族歧义 | D 无名字 | 低分 |
|---|---|---|---|---|---|---|
| `weak_caller` | 39 | 2 | 37 | 0 | 0 | 0 |
| `graphwar_module_container` | 20 | 0 | 20 | 0 | 0 | 0 |
| `graphwar_game_client` | 20 | 10 | 2 | 0 | 8 | 0 |
| `graphwar_function_evolutionator` | 12 | 0 | 0 | 0 | **12** | 0 |
| `graphwar_root_node` | 12 | 3 | 2 | 2 | 5 | 0 |
| `graphwar_nodes` | 11 | 0 | 11 | 0 | 0 | 0 |
| `graphwar_lan_vbox` | 8 | 1 | 2 | 2 | 3 | 0 |
| `graphwar_setup_control` | 6 | 1 | 2 | 2 | 1 | 0 |
| 9 个 GDExtension 类（`axis_drawer`/`effect_drawer`/`entity_drawer`/`graphwar_ad_switch`/`graphwar_button`/`graphwar_function_input`/`graphwar_long_press_animation`/`graphwar_plane`/`player_showcase`） | 各 4 | 0 | 2 | 2 | 0 | 0 |
| `math_function` | 3 | 1 | 0 | 0 | 0 | 2 |
| `button_manager` | 2 | 1 | 0 | 0 | 1 | 0 |
| `graphwar_audio_control` | 2 | 0 | 0 | 0 | 2 | 0 |
| `graphwar_backend` | 2 | 1 | 0 | 0 | 1 | 0 |
| `graphwar_client_connection_handler` / `graphwar_game_state_manager` / `graphwar_logger` / `graphwar_play_screen_control` / `graphwar_replay_control` / `graphwar_soldier_manager` | 各 1 | 0 | 0 | 0 | 1 | 0 |
| **合计** | **179** | **20** | **94** | **24** | **39** | **2** |

**B 族（94 个）的构成**（同一族的成员拿到**同一个**源函数名，缺的只是类型参数）：

| 族 | 个数 | 该键下 Linux 侧 / Windows 侧的引用数 |
|---|---|---|
| `weak_caller::WeakCaller::call` | 21 | 24 / 24 |
| `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 20 | 20 / 20 |
| `weak_caller::WeakCaller::call_0` | 16 | 16 / 16 |
| `graphwar_nodes::GraphwarNodes::get_node_by_name` | 11 | 12 / 12 |
| `_::__init::__inner_init`（12 个类的 gdext 初始化样板，**每类 2 个**） | 24 | 每类 4 / 4 |
| `graphwar_game_client::GraphwarGameClient::get_module` | 2 | — |

**这一栏与 §34 的结果互相独立却完全吻合**：§34 用**两个 `Location` 锚的行号**
（`weak_caller.rs:11:28` / `17:28`）把 40 个归属函数分成 **16 + 24**（16 个 `call_0`、23 个 `call` + 1 个内联调用者）；
本节用 **Windows 结构匹配**把 39 个无名函数分成 **16 个 `call_0` + 23 个 `call`**（21 个 B 族 + 2 个 A 个体）。
两条互不相干的判据落到同一个划分上 —— 这是本节的端到端自检。

### 40.5 三个负结论（不写清楚会被后人当成"没做完"）

1. **`__register_methods` 与 `__register_constants` 在结构上分不开。**
   24 个 C 族歧义行 = **12 个类 × 2**。这两个函数在 Windows 侧：`0x1801ca9c0`（367 B，95 条指令）
   与 `0x1801cab30`（367 B，95 条指令），**只有 51/367 个字节不同**，尺寸、指令直方图、调用数（9）、
   字符串集**全部相同**；Linux 侧对应的两个函数也是同尺寸、同直方图。⇒ 用本节的特征集**无法**判定，
   只能写「该类的 GDExtension 注册样板之一」。要分开它们需要**逐指令序列对齐**（另一个工具的工作量）。
2. **「单态化族」不是"没认出来"，是"名字对、类型参数缺"。**
   例：`WeakCaller::call` 在 Linux 有 24 个、Windows 有 24 个，都引用 `weak_caller.rs:17:28`；
   21 个无名函数（本节 B 族）全部落在同一个 Windows 符号上。逐字节比函数体：**21 个 sha1 各不相同**
   （`4e8262f7…` / `7d20eedc…` / …），所以不是同一段代码的拷贝，而是**同一泛型函数的 21 个单态化**。
   名字可信、**类型参数没有证据**（`name_key` 的 `module/type/method` 三列不带泛型实参）。
3. **「真身无名」闸门是可靠的**：不筛名字重排一遍，若最优的 Windows 函数没有名字，就判未知。
   在 333 个「真值确实有名」的对照上，这条闸门**一次都没有误报**（0/173、0/160）。
   反过来它解释了 §37.10 的例子：求值器 `0x4eb9d0` 不在本节的分母里（它**一个 `src/` `Location` 都不引用**，
   `probe 0x4eb9d0` 打出 `locs=[]`），而 `math_function.rs:184:24` 那 3 个函数里，
   `0x4ebbe0`（1277 B）被定到 `simplify_recursive`（1429 B，ratio 1.12，次名差 2.26），
   另两个（133 B / 120 B，ratio 2.16 / 2.39，score 0.50 / 0.45）落到「低分」桶 —— 它们是冷片段，
   本节**不**给名字。

### 40.6 逐条明细（179 行）

> 每一行都能用 `tools/unnamed_attrib.py probe <addr>` 复算；表格由
> `tools/unnamed_attrib.py attrib --write` 生成的 `fixtures/unnamed_attrib.tsv` 渲染，**不要手改**。
> 证据栏的 `score` 是 §40.2 的结构相似度，`size A/B` 是「Linux 尺寸 / 候选 Windows 尺寸」，
> `hist` 是指令直方图余弦，`共享键 n/m` 是「两个函数共同引用的 `Location` 数 / 本函数引用的总数」，
> `次名差` 是第一与第二候选的分数差。
| linux_addr | 模块（from Location） | 候选角色 | 证据 | 档位 |
|---|---|---|---|---|
| `0x3a54e0` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【A 个体】score=1.81；size 256/254=0.99；hist=0.99；共享键 1/1（集合相等）；次名差 0.42 | 推断 |
| `0x3a55e0` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【A 个体】score=1.48；size 64/70=1.09；hist=0.74；共享键 1/1（集合相等）；次名差 0.76 | 推断 |
| `0x3a5620` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【A 个体】score=1.48；size 64/70=1.09；hist=0.74；共享键 1/1（集合相等）；次名差 0.76 | 推断 |
| `0x3aa6a0` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::__register_constants 或 graphwar_root_node::GraphwarRootNode::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_root_node::GraphwarRootNode::__register_constants、graphwar_root_node::GraphwarRootNode::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x3aa820` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::__register_constants 或 graphwar_root_node::GraphwarRootNode::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_root_node::GraphwarRootNode::__register_constants、graphwar_root_node::GraphwarRootNode::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x3aae60` | `button_manager` | `button_manager::ButtonManager::new::{{closure}}` | 【A 个体】score=1.65；size 64/72=1.12；hist=0.85；共享键 1/1（集合相等）；次名差 0.81 | 推断 |
| `0x3aaea0` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_lobby_control::GraphwarLobbyControl::new;graphwar_setup_control::GraphwarSetupControl::ready 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3aafa0` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_setup_control::GraphwarSetupControl::process;graphwar_player_screen_control::GraphwarPlayerScreenControl::load_items 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab0a0` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=1.87；size 240/255=1.06；hist=0.85；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_root_node::GraphwarRootNode::process_button_pressed 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab190` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_play_screen_control::GraphwarPlayScreenControl::enable_replay_buttons;graphwar_play_screen_control::GraphwarPlayScreenControl::disable_replay_buttons 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab290` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_root_node::GraphwarRootNode::process;graphwar_setup_control::GraphwarSetupControl::process 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab390` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_campaign_control::GraphwarCampaignControl::show_hide_campaign_levels;graphwar_function_input::GraphwarFunctionInput::force_update_cb 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab490` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_root_node::GraphwarRootNode::process_button_pressed;graphwar_setup_control::GraphwarSetupControl::ready 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab590` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 entity_drawer::EntityDrawer::draw;entity_drawer::EntityDrawer::ready 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab690` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_lobby_control::GraphwarLobbyControl::new 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab790` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_play_screen_control::GraphwarPlayScreenControl::new;graphwar_setup_control::GraphwarSetupControl::ready 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3ab890` | `graphwar_nodes` | `graphwar_nodes::GraphwarNodes::get_node_by_name` | 【B 族】score=2.02；size 256/255=1.00；hist=0.97；共享键 1/1（集合相等）；该键下 Linux 侧 12 个函数（其中 11 个无名）↔ Windows 侧 12 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_setup_control::GraphwarSetupControl::process 调用；字符串 upcast_refUpcast,Upcast failed. T | 推断 |
| `0x3b5910` | `graphwar_root_node` | `graphwar_root_node::_::__init::__inner_init` | 【B 族】score=2.41；size 432/426=0.99；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.03；字符串 called `Result:: | 推断 |
| `0x3b5ce0` | `graphwar_root_node` | `graphwar_root_node::_::__init::__inner_init` | 【B 族】score=2.41；size 432/426=0.99；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.08；字符串 called `Result:: | 推断 |
| `0x3b6c70` | `graphwar_logger` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180044ed0`，size 比 1.03，hist=0.98，共享键 1/1）；— | 未知 |
| `0x3b6e00` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801d8e50`，size 比 0.97，hist=0.75，共享键 1/1）；次名差 0.98；最接近的**有名**候选 `graphwar_root_node::GraphwarRootNode::ready::{{closure}}`（score=0.52，比无名那个低 0.98） | 未知 |
| `0x3b6ed0` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801d8f20`，size 比 0.96，hist=0.98，共享键 1/1）；次名差 0.74；最接近的**有名**候选 `graphwar_root_node::GraphwarRootNode::ready::{{closure}}`（score=1.14，比无名那个低 0.74） | 未知 |
| `0x3b7020` | `graphwar_root_node` | `graphwar_root_node::GraphwarRootNode::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801d8b00`，size 比 0.97，hist=0.75，共享键 1/1）；次名差 0.98；最接近的**有名**候选 `graphwar_root_node::GraphwarRootNode::ready::{{closure}}`（score=0.52，比无名那个低 0.98） | 未知 |
| `0x3b70b0` | `button_manager` | `button_manager::ButtonManager::new::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801126d0`，size 比 0.99，hist=0.81，共享键 1/1）；次名差 0.84；最接近的**有名**候选 `button_manager::ButtonManager::new::{{closure}}`（score=0.77，比无名那个低 0.84） | 未知 |
| `0x3bfdd0` | `graphwar_audio_control` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180118970`，size 比 1.00，hist=0.99，共享键 1/1）；被 graphwar_audio_control::GraphwarAudioControl::new 调用；字符串 called `Result:: | 未知 |
| `0x3caa20` | `graphwar_game_state_manager` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180126480`，size 比 0.37，hist=0.96，共享键 2/2）；被 graphwar_game_state_manager::GraphwarGameStateManager::request_fire_function 调用 | 未知 |
| `0x3d10e0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.64；size 240/217=0.90；hist=0.93；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d11d0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d12c0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d13b0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.85；size 224/217=0.97；hist=0.94；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1490` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1580` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1670` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1760` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1850` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1940` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1a30` | `weak_caller` | `weak_caller::WeakCaller::call` | 【A 个体】score=2.52；size 240/214=0.89；hist=0.88；共享键 1/1（集合相等）；次名差 0.96 | 推断 |
| `0x3d1b20` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1c10` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1d00` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1df0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1ee0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d1fd0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d20c0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d21b0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d22a0` | `weak_caller` | `weak_caller::WeakCaller::call` | 【A 个体】score=1.53；size 160/154=0.96；hist=0.81；共享键 1/1（集合相等）；次名差 0.18 | 推断 |
| `0x3d2340` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d2430` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d2520` | `weak_caller` | `weak_caller::WeakCaller::call` | 【B 族】score=1.56；size 240/217=0.90；hist=0.88；共享键 1/1（集合相等）；该键下 Linux 侧 24 个函数（其中 21 个无名）↔ Windows 侧 24 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.04 | 推断 |
| `0x3d2610` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d26f0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d27d0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d28b0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2990` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2a70` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2b50` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2c30` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2d10` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2df0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2ed0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d2fb0` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d3090` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d3170` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d3250` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x3d3330` | `weak_caller` | `weak_caller::WeakCaller::call_0` | 【B 族】score=1.83；size 224/204=0.91；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 16 个函数（其中 16 个无名）↔ Windows 侧 16 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00 | 推断 |
| `0x41a9d0` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801af570`，size 比 1.02，hist=0.94，共享键 1/1）；次名差 0.37 | 未知 |
| `0x41ab40` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801af8b0`，size 比 1.02，hist=0.94，共享键 1/1）；次名差 0.37 | 未知 |
| `0x41b500` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801b02c0`，size 比 1.01，hist=0.93，共享键 1/1）；次名差 0.35；被 graphwar_function_evolutionator::GraphwarFunctionEvolutionator::calculate_functions 调用 | 未知 |
| `0x41b720` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801b0150`，size 比 1.01，hist=0.93，共享键 1/1）；次名差 0.35；被 graphwar_function_evolutionator::GraphwarFunctionEvolutionator::calculate_functions_old 调用 | 未知 |
| `0x41b880` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801b0a70`，size 比 0.99，hist=0.98，共享键 1/1）；次名差 0.32 | 未知 |
| `0x41bdf0` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801b0500`，size 比 0.99，hist=0.98，共享键 1/1）；次名差 0.32 | 未知 |
| `0x426430` | `graphwar_root_node` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800acdc0`，size 比 1.08，hist=1.00，共享键 2/2）；次名差 0.59；调用 graphwar_player_screen_control::GraphwarPlayerScreenControl::pending；字符串 graphwarrust::gr,;attempted to ac,called `Result:: | 未知 |
| `0x433c00` | `graphwar_root_node` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800af490`，size 比 1.04，hist=1.00，共享键 2/2）；次名差 0.43；调用 graphwar_player_screen_control::GraphwarPlayerScreenControl::pending；字符串 godot_core::priv,/home/lucas/.car,graphwarrust::gr | 未知 |
| `0x438880` | `graphwar_client_connection_handler` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180186800`，size 比 0.99，hist=0.98，共享键 4/4）；字符串 called `Result::,Cannot set clien | 未知 |
| `0x444fd0` | `graphwar_soldier_manager` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800f84c0`，size 比 1.06，hist=1.00，共享键 1/1）；被 graphwar_soldier_manager::GraphwarSoldierManager::handle_entity_removed_event+graphwar_soldier_manager::GraphwarSoldierManager::handle_entity_removed_event::panic_cold_explicit 调用 | 未知 |
| `0x4600e0` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180067620`，size 比 1.06，hist=1.00，共享键 1/1）；次名差 1.72 | 未知 |
| `0x4602f0` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180067510`，size 比 1.06，hist=1.00，共享键 1/1）；次名差 1.72 | 未知 |
| `0x46c5a0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_local_player_manager::GraphwarLocalPlayerManager::handle_player_info+graphwar_local_player_manager::GraphwarLocalPlayerManager::handle_player_info::panic_cold_explicit 调用 | 推断 |
| `0x46c7c0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46c9e0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_level_generator::GraphwarLevelGenerator::place_entity 调用 | 推断 |
| `0x46cc00` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_game_state_manager::GraphwarGameStateManager::execute_results 调用 | 推断 |
| `0x46ce20` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_game_state_manager::GraphwarGameStateManager::start_game+graphwar_turn_manager::GraphwarTurnManager::initialize_turns 调用 | 推断 |
| `0x46d040` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_game_recorder::GraphwarGameRecorder::save_event+graphwar_game_recorder::GraphwarGameRecorder::subscribe_to_events;graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event 调用 | 推断 |
| `0x46d260` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_server_connection_handler::GraphwarServerConnectionHandler::handle_clear_bans_request_event 调用 | 推断 |
| `0x46d480` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46d6a0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_turn_manager::GraphwarTurnManager::next_turn 调用 | 推断 |
| `0x46d8c0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46dae0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_game_state_manager::GraphwarGameStateManager::add_target 调用 | 推断 |
| `0x46dd00` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_game_state_manager::GraphwarGameStateManager::request_new_player 调用 | 推断 |
| `0x46df20` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46e140` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_turn_manager::GraphwarTurnManager::next_turn 调用 | 推断 |
| `0x46e360` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46e580` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_remote_event_handler::GraphwarRemoteEventHandler::handle_client_remote_event;graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function 调用 | 推断 |
| `0x46e7a0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_level_generator::GraphwarLevelGenerator::place_entity 调用 | 推断 |
| `0x46e9c0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_game_state_manager::GraphwarGameStateManager::start_game+graphwar_turn_manager::GraphwarTurnManager::initialize_turns 调用 | 推断 |
| `0x46ebe0` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_level_generator::GraphwarLevelGenerator::place_entity 调用 | 推断 |
| `0x46ee00` | `graphwar_module_container` | `graphwar_module_container::GraphwarModuleContainer::get_module_wk` | 【B 族】score=1.83；size 544/526=0.97；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 20 个函数（其中 20 个无名）↔ Windows 侧 20 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_color_manager::GraphwarColorManager::set_color+graphwar_game_state_manager::GraphwarGameStateManager::report_fired_function;graphwar_level_generator::GraphwarLevelGenerator::generate_level_custom 调用 | 推断 |
| `0x481140` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180082960`，size 比 1.01，hist=0.99，共享键 1/1）；次名差 0.40 | 未知 |
| `0x481870` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180081ae0`，size 比 1.01，hist=0.99，共享键 1/1）；次名差 0.40 | 未知 |
| `0x4825b0` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800830b0`，size 比 0.98，hist=1.00，共享键 1/1）；次名差 1.36；字符串 mid > lenVAR_XVA | 未知 |
| `0x482c20` | `graphwar_function_evolutionator` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180083710`，size 比 0.98，hist=1.00，共享键 1/1）；次名差 1.36；字符串 mid > lenVAR_XVA | 未知 |
| `0x4b1f60` | `entity_drawer` | `entity_drawer::EntityDrawer::__register_constants 或 entity_drawer::EntityDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：entity_drawer::EntityDrawer::__register_constants、entity_drawer::EntityDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4b20e0` | `entity_drawer` | `entity_drawer::EntityDrawer::__register_constants 或 entity_drawer::EntityDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：entity_drawer::EntityDrawer::__register_constants、entity_drawer::EntityDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4b2260` | `graphwar_plane` | `graphwar_plane::GraphwarPlane::__register_constants 或 graphwar_plane::GraphwarPlane::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_plane::GraphwarPlane::__register_constants、graphwar_plane::GraphwarPlane::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4b23e0` | `graphwar_plane` | `graphwar_plane::GraphwarPlane::__register_constants 或 graphwar_plane::GraphwarPlane::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_plane::GraphwarPlane::__register_constants、graphwar_plane::GraphwarPlane::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4b88d0` | `graphwar_setup_control` | `graphwar_setup_control::GraphwarSetupControl::__register_constants 或 graphwar_setup_control::GraphwarSetupControl::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_setup_control::GraphwarSetupControl::__register_constants、graphwar_setup_control::GraphwarSetupControl::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4b8a50` | `graphwar_setup_control` | `graphwar_setup_control::GraphwarSetupControl::__register_constants 或 graphwar_setup_control::GraphwarSetupControl::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_setup_control::GraphwarSetupControl::__register_constants、graphwar_setup_control::GraphwarSetupControl::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4bbee0` | `entity_drawer` | `entity_drawer::_::__init::__inner_init` | 【B 族】score=2.41；size 432/426=0.99；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.02；字符串 called `Result:: | 推断 |
| `0x4bc090` | `entity_drawer` | `entity_drawer::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4bc6b0` | `graphwar_plane` | `graphwar_plane::_::__init::__inner_init` | 【B 族】score=2.41；size 432/426=0.99；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.02；字符串 called `Result:: | 推断 |
| `0x4bcab0` | `graphwar_plane` | `graphwar_plane::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4bce80` | `graphwar_setup_control` | `graphwar_setup_control::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4bd280` | `graphwar_setup_control` | `graphwar_setup_control::_::__init::__inner_init` | 【B 族】score=2.41；size 432/426=0.99；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.02；字符串 called `Result:: | 推断 |
| `0x4bd650` | `graphwar_long_press_animation` | `graphwar_long_press_animation::_::__init::__inner_init` | 【B 族】score=2.42；size 432/423=0.98；hist=0.96；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.03；字符串 called `Result:: | 推断 |
| `0x4bda50` | `graphwar_long_press_animation` | `graphwar_long_press_animation::_::__init::__inner_init` | 【B 族】score=2.44；size 432/423=0.98；hist=0.96；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.10；字符串 called `Result:: | 推断 |
| `0x4c5b50` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【A 个体】score=1.76；size 176/173=0.98；hist=0.89；共享键 2/2（集合相等）；次名差 0.69 | 推断 |
| `0x4c5c00` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【A 个体】score=1.76；size 176/173=0.98；hist=0.89；共享键 2/2（集合相等）；次名差 0.69 | 推断 |
| `0x4c5cb0` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【A 个体】score=1.76；size 176/173=0.98；hist=0.89；共享键 2/2（集合相等）；次名差 0.69 | 推断 |
| `0x4c5d60` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【A 个体】score=1.76；size 176/173=0.98；hist=0.89；共享键 2/2（集合相等）；次名差 0.69 | 推断 |
| `0x4d1ca0` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【A 个体】score=1.32；size 176/166=0.94；hist=0.68；共享键 2/2（集合相等）；次名差 0.23 | 推断 |
| `0x4d1d50` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【A 个体】score=1.32；size 176/166=0.94；hist=0.68；共享键 2/2（集合相等）；次名差 0.23 | 推断 |
| `0x4d1e00` | `graphwar_setup_control` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【A 个体】score=1.48；size 96/102=1.06；hist=0.68；共享键 1/1（集合相等）；次名差 0.81 | 推断 |
| `0x4d1e60` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【A 个体】score=1.32；size 176/166=0.94；hist=0.68；共享键 2/2（集合相等）；次名差 0.23 | 推断 |
| `0x4d2550` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【A 个体】score=1.32；size 176/166=0.94；hist=0.68；共享键 2/2（集合相等）；次名差 0.23 | 推断 |
| `0x4d7bc0` | `graphwar_long_press_animation` | `graphwar_long_press_animation::GraphwarLongPressAnimation::__register_constants 或 graphwar_long_press_animation::GraphwarLongPressAnimation::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_long_press_animation::GraphwarLongPressAnimation::__register_constants、graphwar_long_press_animation::GraphwarLongPressAnimation::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4d7d40` | `graphwar_long_press_animation` | `graphwar_long_press_animation::GraphwarLongPressAnimation::__register_constants 或 graphwar_long_press_animation::GraphwarLongPressAnimation::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_long_press_animation::GraphwarLongPressAnimation::__register_constants、graphwar_long_press_animation::GraphwarLongPressAnimation::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4d88b0` | `graphwar_setup_control` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3640`，size 比 0.99，hist=0.87，共享键 1/1）；次名差 0.90；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}`（score=0.82，比无名那个低 0.90） | 未知 |
| `0x4d8a00` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e2f60`，size 比 0.96，hist=0.88，共享键 2/2）；次名差 0.84；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}`（score=0.85，比无名那个低 0.84） | 未知 |
| `0x4d8d30` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3440`，size 比 0.96，hist=0.88，共享键 2/2）；次名差 0.84；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}`（score=0.85，比无名那个低 0.84） | 未知 |
| `0x4d8ff0` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3a60`，size 比 0.99，hist=0.99，共享键 2/2）；次名差 0.87；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}`（score=1.05，比无名那个低 0.87） | 未知 |
| `0x4d9270` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3c20`，size 比 0.96，hist=0.88，共享键 2/2）；次名差 0.84；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}`（score=0.85，比无名那个低 0.84） | 未知 |
| `0x4d9370` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3340`，size 比 0.96，hist=0.88，共享键 2/2）；次名差 0.84；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::connect_entry_buttons::{{closure}}`（score=0.85，比无名那个低 0.84） | 未知 |
| `0x4d9500` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3d20`，size 比 0.98，hist=0.99，共享键 2/2）；次名差 0.87；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}`（score=1.04，比无名那个低 0.87） | 未知 |
| `0x4d95f0` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e2e70`，size 比 0.98，hist=0.99，共享键 2/2）；次名差 0.87；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}`（score=1.04，比无名那个低 0.87） | 未知 |
| `0x4d96e0` | `graphwar_game_client` | `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1800e3970`，size 比 0.74，hist=0.98，共享键 2/2）；次名差 0.29；调用 graphwar_setup_control::GraphwarSetupControl::ready；最接近的**有名**候选 `graphwar_setup_control::GraphwarSetupControl::ready::{{closure}}`（score=0.90，比无名那个低 0.29） | 未知 |
| `0x4dcd70` | `axis_drawer` | `axis_drawer::AxisDrawer::__register_constants 或 axis_drawer::AxisDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：axis_drawer::AxisDrawer::__register_constants、axis_drawer::AxisDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4dcef0` | `axis_drawer` | `axis_drawer::AxisDrawer::__register_constants 或 axis_drawer::AxisDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：axis_drawer::AxisDrawer::__register_constants、axis_drawer::AxisDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4dec00` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180045140`，size 比 0.60，hist=0.90，共享键 1/1）；次名差 0.13；字符串 cannot have more；最接近的**有名**候选 `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}`（score=0.85，比无名那个低 0.13） | 未知 |
| `0x4e02b0` | `effect_drawer` | `effect_drawer::EffectDrawer::__register_constants 或 effect_drawer::EffectDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：effect_drawer::EffectDrawer::__register_constants、effect_drawer::EffectDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e0430` | `effect_drawer` | `effect_drawer::EffectDrawer::__register_constants 或 effect_drawer::EffectDrawer::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：effect_drawer::EffectDrawer::__register_constants、effect_drawer::EffectDrawer::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e0e60` | `graphwar_button` | `graphwar_button::GraphwarButton::__register_constants 或 graphwar_button::GraphwarButton::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_button::GraphwarButton::__register_constants、graphwar_button::GraphwarButton::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e0fe0` | `graphwar_button` | `graphwar_button::GraphwarButton::__register_constants 或 graphwar_button::GraphwarButton::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_button::GraphwarButton::__register_constants、graphwar_button::GraphwarButton::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e1160` | `player_showcase` | `player_showcase::PlayerShowcase::__register_constants 或 player_showcase::PlayerShowcase::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：player_showcase::PlayerShowcase::__register_constants、player_showcase::PlayerShowcase::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e12e0` | `player_showcase` | `player_showcase::PlayerShowcase::__register_constants 或 player_showcase::PlayerShowcase::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：player_showcase::PlayerShowcase::__register_constants、player_showcase::PlayerShowcase::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e3b00` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::__register_constants 或 graphwar_lan_vbox::GraphwarLanVBox::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_lan_vbox::GraphwarLanVBox::__register_constants、graphwar_lan_vbox::GraphwarLanVBox::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e3c80` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::__register_constants 或 graphwar_lan_vbox::GraphwarLanVBox::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_lan_vbox::GraphwarLanVBox::__register_constants、graphwar_lan_vbox::GraphwarLanVBox::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e5100` | `graphwar_ad_switch` | `graphwar_ad_switch::GraphwarAdSwitch::__register_constants 或 graphwar_ad_switch::GraphwarAdSwitch::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_ad_switch::GraphwarAdSwitch::__register_constants、graphwar_ad_switch::GraphwarAdSwitch::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e5280` | `graphwar_ad_switch` | `graphwar_ad_switch::GraphwarAdSwitch::__register_constants 或 graphwar_ad_switch::GraphwarAdSwitch::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_ad_switch::GraphwarAdSwitch::__register_constants、graphwar_ad_switch::GraphwarAdSwitch::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e6dc0` | `graphwar_function_input` | `graphwar_function_input::GraphwarFunctionInput::__register_constants 或 graphwar_function_input::GraphwarFunctionInput::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_function_input::GraphwarFunctionInput::__register_constants、graphwar_function_input::GraphwarFunctionInput::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e6f40` | `graphwar_function_input` | `graphwar_function_input::GraphwarFunctionInput::__register_constants 或 graphwar_function_input::GraphwarFunctionInput::__register_methods` | 【C 族歧义】**2 个候选名同分**（差 < 0.05）：graphwar_function_input::GraphwarFunctionInput::__register_constants、graphwar_function_input::GraphwarFunctionInput::__register_methods；score=2.36；size 384/367=0.96；hist=0.96；共享键 1/1（集合相等）；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e70c0` | `axis_drawer` | `axis_drawer::_::__init::__inner_init` | 【B 族】score=2.43；size 416/423=1.02；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.03；字符串 called `Result:: | 推断 |
| `0x4e76c0` | `axis_drawer` | `axis_drawer::_::__init::__inner_init` | 【B 族】score=2.39；size 432/423=0.98；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.10；字符串 called `Result:: | 推断 |
| `0x4e7870` | `effect_drawer` | `effect_drawer::_::__init::__inner_init` | 【B 族】score=2.39；size 432/423=0.98；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.10；字符串 called `Result:: | 推断 |
| `0x4e7a20` | `effect_drawer` | `effect_drawer::_::__init::__inner_init` | 【B 族】score=2.43；size 416/423=1.02；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.03；字符串 called `Result:: | 推断 |
| `0x4e8230` | `graphwar_button` | `graphwar_button::_::__init::__inner_init` | 【B 族】score=2.40；size 416/426=1.02；hist=0.94；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e83d0` | `graphwar_button` | `graphwar_button::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4e87d0` | `player_showcase` | `player_showcase::_::__init::__inner_init` | 【B 族】score=2.40；size 416/426=1.02；hist=0.94；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e8970` | `player_showcase` | `player_showcase::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4e91d0` | `graphwar_lan_vbox` | `graphwar_lan_vbox::_::__init::__inner_init` | 【B 族】score=2.39；size 432/423=0.98；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.10；字符串 called `Result:: | 推断 |
| `0x4e9590` | `graphwar_lan_vbox` | `graphwar_lan_vbox::_::__init::__inner_init` | 【B 族】score=2.43；size 416/423=1.02；hist=0.95；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.03；字符串 called `Result:: | 推断 |
| `0x4e9730` | `graphwar_ad_switch` | `graphwar_ad_switch::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4e9af0` | `graphwar_ad_switch` | `graphwar_ad_switch::_::__init::__inner_init` | 【B 族】score=2.40；size 416/426=1.02；hist=0.94；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4e9ee0` | `graphwar_function_input` | `graphwar_function_input::_::__init::__inner_init` | 【B 族】score=2.40；size 416/426=1.02；hist=0.94；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；字符串 called `Result:: | 推断 |
| `0x4ea2d0` | `graphwar_function_input` | `graphwar_function_input::_::__init::__inner_init` | 【B 族】score=2.40；size 432/426=0.99；hist=0.98；共享键 1/1（集合相等）；该键下 Linux 侧 4 个函数（其中 2 个无名）↔ Windows 侧 4 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.11；字符串 called `Result:: | 推断 |
| `0x4eb94b` | `math_function` | — | 【低分】结构相似度只有 0.50（低于下限 1.00，正对照正确配对中位数 2.36）；最接近的有名候选 `math_function::MathFunction::evaluate_token`（size 比 2.16）；score=0.50；size 133/288=2.16；hist=0.50；共享键 1/1（集合相等） | 未知 |
| `0x4ebb68` | `math_function` | — | 【低分】结构相似度只有 0.45（低于下限 1.00，正对照正确配对中位数 2.36）；最接近的有名候选 `math_function::MathFunction::evaluate_token`（size 比 2.39）；score=0.45；size 120/288=2.39；hist=0.45；共享键 1/1（集合相等） | 未知 |
| `0x4ebbe0` | `math_function` | `math_function::MathFunction::simplify_recursive` | 【A 个体】score=2.70；size 1277/1429=1.12；hist=0.99；共享键 1/1（集合相等）；次名差 2.26；被 graphwar_function_evolutionator::GraphwarFunctionEvolutionator::calculate_functions;graphwar_function_evolutionator::GraphwarFunctionEvolutionator::calculate_functions_old 调用 | 推断 |
| `0x4f01c0` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}` | 【A 个体】score=1.89；size 240/240=1.00；hist=0.95；共享键 1/1（集合相等）；次名差 0.67 | 推断 |
| `0x500070` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180045140`，size 比 1.00，hist=0.97，共享键 1/1）；次名差 0.78；最接近的**有名**候选 `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}`（score=1.16，比无名那个低 0.78） | 未知 |
| `0x5005b0` | `graphwar_lan_vbox` | `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}` | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180045140`，size 比 0.53，hist=0.91，共享键 1/1）；次名差 0.19；字符串 on_text_changete；最接近的**有名**候选 `graphwar_lan_vbox::GraphwarLanVBox::update_game_list::{{closure}}`（score=0.85，比无名那个低 0.19） | 未知 |
| `0x5153f0` | `graphwar_backend` | `graphwar_backend::GraphwarBackend::subscribe_to_event_with_id::{{closure}}` | 【A 个体】score=1.90；size 272/271=1.00；hist=0.95；共享键 1/1（集合相等）；次名差 0.44 | 推断 |
| `0x5159a0` | `graphwar_game_client` | `graphwar_backend::GraphwarBackend::start_client_to_remote_server` | 【A 个体】score=1.79；size 400/390=0.98；hist=0.92；共享键 1/1（集合相等）；次名差 1.08；调用 graphwar_backend::GraphwarBackend::resubscribe_to_events | 推断 |
| `0x516450` | `graphwar_game_client` | `graphwar_game_client::GraphwarGameClient::get_module` | 【B 族】score=1.68；size 96/101=1.05；hist=0.79；共享键 2/2（集合相等）；该键下 Linux 侧 45 个函数（其中 2 个无名）↔ Windows 侧 65 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_root_node::GraphwarRootNode::process_button_pressed 调用 | 推断 |
| `0x5164b0` | `graphwar_game_client` | `graphwar_game_client::GraphwarGameClient::get_module` | 【B 族】score=1.68；size 96/101=1.05；hist=0.79；共享键 2/2（集合相等）；该键下 Linux 侧 45 个函数（其中 2 个无名）↔ Windows 侧 65 个函数 —— 单态化族，源函数名可信、**类型参数未知**；次名差 0.00；被 graphwar_root_node::GraphwarRootNode::process;graphwar_root_node::GraphwarRootNode::process_button_pressed 调用 | 推断 |
| `0x5175b0` | `graphwar_game_client` | `graphwar_game_client::GraphwarGameClient::request_game_name_change` | 【A 个体】score=1.65；size 224/215=0.96；hist=0.89；共享键 1/1（集合相等） | 推断 |
| `0x520100` | `graphwar_backend` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x18013c880`，size 比 0.33，hist=0.86，共享键 1/1）；— | 未知 |
| `0x522070` | `graphwar_play_screen_control` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x180140ee0`，size 比 0.95，hist=0.99，共享键 5/5）；被 graphwar_play_screen_control::GraphwarPlayScreenControl::new 调用；字符串 cloneinit_ref() ,TimerLabelViewFu,ViewFunctionButt | 未知 |
| `0x523410` | `graphwar_replay_control` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x18013f860`，size 比 0.77，hist=0.97，共享键 1/1）；字符串 cloneinit_ref() ,ReplaysContainer | 未知 |
| `0x5238c0` | `graphwar_audio_control` | — | 【D 无名字】结构最优的 Windows 对应体**没有名字**（`0x1801405b0`，size 比 0.89，hist=0.98，共享键 1/1）；被 graphwar_audio_control::GraphwarAudioControl::new 调用；字符串 cloneinit_ref() ,MuteButtonExplos | 未知 |

### 40.7 就地更正（按 `AGENTS.md` §0.6，**不改 §37 正文，登记在这里**）

本节只许"追加一节"（任务书 §5），所以 §37 / §38 的旧数字**一个字都没有动**，
但它们现在该怎么读，记在这里：

| 旧说法（出处） | 复算结果 | 证据 |
|---|---|---|
| 「可定名的 Linux 地址 **351** 个 = 66.6%」（§37.0 / §37.8 / §39.0 ②） | 那个 351 是**两份夹具的地址并集**，其中 **6 个不在 527 内**（§40.1 表）；落在分母上的只有 **345** ⇒ **345/527 = 65.5%**，不是 66.6% | `unnamed_attrib.py list` + §40.1 表 |
| 「527 里有 **176** 个仍定不了名」（§37.10 第 1 条 / 任务书） | 把 v2 的三向落地（`唯一锚` 310 + `跨版本锚` 27）并进来后，可定名 **348** ⇒ 无名 **179**（**+3**） | 同上 |
| 「`0x748000` = `FunctionRandomizer::mutate_function`」（`fixtures/name_to_addr.tsv` 的 `消去法` 行） | 该地址引用的 25 个 `Location` **全在 `miniz_oxide`**，与 `FunctionRandomizer` 无关 ⇒ **推断：这行结论可疑**（缺一次人工反汇编复核） | `probe` / `loc_xrefs.py func 0x748000` |
| 「求值器 `0x4eb9d0` 只引用 `math_function.rs:184:24`，所以定不了名」（§37.10） | 本节 `probe 0x4eb9d0` 打出 **`locs=[]`**：它**一个 `src/` `Location` 都不引用**，因此**不在 527 这个分母里**（这是两条不同的事实：它确实没有名字，但理由不是"锚被三个函数共享"） | `unnamed_attrib.py probe 0x4eb9d0` |

**没有改的**：`fixtures/` 里既有的五个夹具、`docs/02` 的其它任何行、`src/`、`client/`、`dispatch/`。

### 40.8 还缺什么（`未知` 的 41 个，逐类说清缺什么）

1. **D 桶 39 个：结构最优的对应体在 Windows 侧没有符号。**
   39 行指向 **37 个不同的无名 Windows 函数**（`0x180045140` 被 3 行共用；其余 1:1）。
   注意它们**不是**"键下只有无名函数"：其中 16 行的同键候选里**有**有名函数，
   但结构上明显更差（证据栏写了差多少）。要给出名字，缺的是**第三种来源**：另一个带符号表的
   同版本构建（macOS 或 Linux 未 strip 版），或者人工逆向。本节**不**用"最像"硬凑。
2. **低分 2 个（`0x4eb94b` / `0x4ebb68`）：`math_function.rs:184:24` 的冷片段。**
   它们只有 133 B / 120 B，与三个候选（288 / 683 / 1429 B）的 size 比 2.16 / 2.39，score 0.50 / 0.45。
   要定名，缺的是**函数边界复核**（Ghidra 起点表把冷块切了出来，它可能不是独立源码函数）
   + 调用者上下文分析。
3. **C 桶 24 个：名字二选一。** 缺的是逐指令序列对齐（见 §40.5 第 1 条）。
4. **B 桶 94 个：类型参数。** `name_key` 的三列不带泛型实参，所以
   `WeakCaller::call::<T>` 的 `T` 只能从**调用点传入的 fn 项地址**反推（§34.7 第 4 条给了办法：
   x86 上 `lea rsi,[rip+…]` 的地址查 `fixtures/name_to_addr.tsv`）。本节**没有**做这一步。
5. **`0x748000` 那条 v1 结论**（§40.7）：缺一次人工复核，本节只标"可疑"。
6. **名字仍然不是"逐位验证"**：本节的一切都是**结构推断**（K/M/S/E），
   没有任何一条与源码逐位对拍 —— 本项目目前也**没有**可对拍的源码。

### 40.9 一条命令复算

```bash
$PY tools/unnamed_attrib.py build                  # 扫 Linux + Windows（首次约 55 s，之后读缓存）
$PY tools/unnamed_attrib.py list                   # 527 / 348 / 179 + 模块分布 + 地址清单（-v）
$PY tools/unnamed_attrib.py control --ablate       # 两个正对照（地址级 / 名字级）+ 分桶精度 + 消融
$PY tools/unnamed_attrib.py attrib --write         # 写 fixtures/unnamed_attrib.tsv（179 行）
$PY tools/unnamed_attrib.py attrib --table         # 打印本节 §40.6 那张表
$PY tools/unnamed_attrib.py probe 0x3a54e0 0x4eb9d0  # 单条复核：候选、分数、常量、字符串
```

`$PY` = 仓库 `.reference/venv/bin/python`（要有 capstone 5.x）。缓存键带 `SCHEMA=ua-v2`：
改了特征口径就必须抬版本号，否则会静默吃旧缓存（`sym_match` §36.0 踩过这个坑）。
**本节没有一个数字来自"跑测试"**：`cargo test` 是回归保护，不是复原证据（`AGENTS.md` §0.2）。
