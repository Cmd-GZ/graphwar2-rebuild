//! 原版 `src/graphwar_plane_pos_manager.rs` —— **24 字节的 `GraphwarPlanePosManager`**
//! （第八轮任务 36，`dispatch/36-plane-pos-manager.md`）。
//!
//! 这是 22 个 manager 里**最后一个**没有 `src/` 文件的槽（`docs/16-client.md` §47.2 的
//! `stub`）。本文件把它解到**逐字段**（`观测`，三份构建交叉验证），并把**已复原的行为**
//! （位置表 + `PosInfo` 事件的两个方向）落成代码；`size_of` / `offset_of` 有**编译期硬闸**。
//!
//! > 档位口径同 `docs/16-client.md` §42 / §44 / §46：**逐位** = 与真二进制逐字节/逐位对拍；
//! > **观测** = 能指到反汇编地址 / 指令级读写；**推断** = 有依据未证；**未知** = 没解出来。
//! > `cargo test` / `assemble_check.gd` 的条数**只是回归保护**，不是复原证据。
//!
//! # 〇、先纠正任务书的一条前提（`AGENTS.md` §0.6 就地更正）
//!
//! 任务书 §1 写「它的**用途**其实从别处已知：`PlanePos` 的坐标换算（世界坐标 ↔ 像素）
//! —— `GraphwarPlane` 的 `update_transform` 三实参就是它在管」。**这条是错的**，本轮证否：
//!
//! | 说法 | 实测 | 证据 |
//! |---|---|---|
//! | 它管 `update_transform` 的三个实参 | **否**。三个实参在 `GraphwarPlane::draw` 体内**就地算出**，来自 `Control::get_size()` 与两个常量（`0.5f32` / `50.0f32`），`flip` 来自 `get_team_overview` 的位组合 | `update_transform`（`0x499050`）**全 `.text` 只有 1 个调用点**：`0x4b6147`（在 `GraphwarPlane::draw` `0x4b5df0` 内）。调用点前 4 条：`0x4b60ce call 0x8925f0`（`Control::get_size`）→ `0x4b60f6 mulss xmm1,xmm0(=0.5)`（y = h/2）→ `0x4b6100 mulss xmm0,xmm2(=w)`（x = w/2）→ `0x4b6104 divss xmm2,50.0`（scale = w/50）。**这一段里没有任何 `PlanePosManager` 调用**（正对照见下） |
//! | 它是"坐标换算（世界 ↔ 像素）" | **否**。它是一张**位置表**：`HashMap<u32, 24 B POD>`，键 = 实体 id，值 = `(pos_x, pos_y, radius)`；`px = 20x + 500` 那条换算属于 `graphwar_plane.rs`（`obstacle_partial_update_event` `0x4be3e0`，`docs/16` §42），不在本模块 | `set_position` 只是 `HashMap::insert` + 发事件（arm64 `0x3b81d8` `bl hashbrown::map::HashMap::insert`），全程**没有**乘加 |
//!
//! **正对照**（证明"扫不到"不是扫法坏了）：同一条 `addr_probe.py callers` 对
//! `get_position`（`0x3c62b0`）给出 **35** 个调用点（`EntityDrawer::draw` `0x4b2890` 在列），
//! 所以扫描器是灵的；`GraphwarPlane::draw`（`0x4b5df0`）**不在**这 35 个里。
//!
//! ⇒ `GraphwarPlane::update_transform` 的三实参**不是**本管理器提供的，我方
//! `client/tools/plane_on_screen_check.gd` 与 `_scaffold/graphwar_plane_class.rs` 那三个值
//! **不动**（§48.2 的"算出来的"仍然是原版的来源）。
//!
//! # 一、这个模块里有什么（`观测`）
//!
//! arm64 2.0.60 未 strip，`.symtab` 原文给出 **8** 个 `FUNC` 符号；Windows DLL 另有 3 个
//! `GraphwarPlanePosData::*` 的**独立副本**（arm64 侧那三个被内联进了 manager 的同名方法）：
//!
//! | 函数 | arm64 2.0.60 | x86 Linux 2.0.60 | Windows 2.0.60 DLL |
//! |---|---|---|---|
//! | `GraphwarPlanePosManager::new` | `0x3b89a0`（392 B） | `0x3c6d30` | `0x1800da460` |
//! | `GraphwarPlanePosManager::subscribe_to_events_event_handler` | **内联进 `new`**（只剩闭包 `0x3b8944`(44 B) / `0x3b8970`(48 B)） | 闭包 `0x3c6cf0` / `0x3c6d10` / `0x3c71e0` / `0x3c7170` | 闭包 `0x1800da420` / `0x1800da440` |
//! | `GraphwarPlanePosManager::get_position` | `0x3b7f58`（520 B） | `0x3c62b0` | `0x1800d9bf0` |
//! | `GraphwarPlanePosManager::set_position` | `0x3b8160`（488 B） | `0x3c64c0` | `0x1800d9de0` |
//! | `GraphwarPlanePosManager::get_all_positions` | `0x3b8348`（792 B） | `0x3c66a0` | `0x1800d9fd0` |
//! | `GraphwarPlanePosManager::send_all_info_events` | `0x3b8660`（312 B） | `0x3c6a10` | `0x1800da140` |
//! | `GraphwarPlanePosManager::handle_entity_removed_event` | `0x3b8798`（428 B） | `0x3c6b40` | `0x1800da280` |
//! | `GraphwarPlanePosData::new` | 内联 | 内联 | `0x1800cd770`（与 `GraphwarTeamData::new` **被 LLVM 合并成同一份**，见 §四） |
//! | `GraphwarPlanePosData::get_position` | 内联进 `manager::get_position` | 同 | `0x1800d9ac0` |
//! | `GraphwarPlanePosData::get_all_positions` | 内联进 `manager::get_all_positions` | 同 | `0x1800d9bb0` |
//!
//! 复算（三条，互不依赖）：
//!
//! ```text
//! readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so | grep graphwar_plane_pos_manager
//! grep -E "graphwar_plane_pos_manager" fixtures/name_to_addr.tsv
//! objdump -t .reference/windows/graphwarrust-2.0.60.dll | grep GraphwarPlanePos    # VA = 0x180001000 + 值
//! ```
//!
//! ⚠️ `docs/02-module-map.md` §17 的模块表把本模块记作 **9** 个符号；上面那条 `readelf`
//! 数出来是 **8**（6 个公开方法 + 2 个闭包）。9 的那一份把 `GraphwarPlanePosData` 的某个
//! 内联副本也算进去了，**按实测的 8 为准**（这一行在 `docs/02` 已就地改正）。
//!
//! # 二、24 字节 = 3 个 8 字节字段（`观测`，三份构建逐偏移一致）
//!
//! | 偏移 | 宽 | 类型 | 证据（指令地址） | 档位 |
//! |---|---|---|---|---|
//! | `+0x00` | 8 | `Weak<RefCell<GraphwarPlanePosManager>>`（弱自句柄） | 构造点把 **`RcBox` 指针本身**写进该槽：arm64 `0x4199a0` `stp xzr, x20, [x20, #0x10]`（第 2 字 = `RcBox+0x18` = `m+0x00`，`RcBox` 头 = `{strong@0, weak@8, RefCell 借用位@0x10}`）、x86 `0x523356` `mov [r14+0x18], r14`、DLL `0x1801413xx` 同一形状；`new` 里做 `Weak::clone`（`cmn x21,#1` / `ldr x8,[x21,#8]` / `adds` / `str`）塞进订阅闭包：arm64 `0x3b89d0`–`0x3b89ec`、x86 `0x3c6d5f`–`0x3c6d69`（`cmp r15,-1` / `inc [r15+8]`）；析构 = 弱计数 `-1`，归零时 `dealloc(ptr, 0x30, 8)`：arm64 `0x4b306c`–`0x4b30b0`、x86/DLL 同名 `drop_in_place<RefCell<GraphwarPlanePosManager>>`（DLL `0x180064139`–`0x180064168`） | **观测** |
//! | `+0x08` | 8 | `Rc<RefCell<GraphwarEventQueue>>` | `set_position` 读它 → `+0x10` 是 `RefCell` 借用位（写 `-1`）→ `+0x18` 当 `&mut GraphwarEventQueue` 传给 `add_event`：arm64 `0x3b8200` `ldr x20,[x20,#8]` → `0x3b8234` `bl 0x3d7a40`（`.symtab` 名 = `GraphwarEventQueue::add_event`）；x86 `0x3c656b` `mov r14,[r14+8]` → `0x3c65ad` `call *GOT 0x9bac80 → 0x3d3530`。`send_all_info_events` 同（arm64 `0x3b869c` / x86 `0x3c6a..`）。析构做 **非原子** 强计数 `-1` + `Rc::drop_slow`（arm64 `0x4b308c` `ldr x8,[x0,#0x10]!` → `0x41a4b8`；DLL `0x180064149` `decq` → `Rc::drop_slow 0x1801431a0`）⇒ 这一格是 `Rc` 而不是 `Arc` | **观测** |
//! | `+0x10` | 8 | `Arc<Mutex<GraphwarPlanePosData>>` | **5 个方法的第一句**都是读它：arm64 `0x3b7f70`（get_position）/ `0x3b8180`（set_position）/ `0x3b83xx`（get_all_positions）/ `0x3b87c4`（handle_entity_removed_event）；x86 `0x3c62c3`（`mov r12,[rsi+0x10]`）/ `0x3c64e8` / `0x3c66ae` / `0x3c6b69`。`ArcInner` = `{strong@0, weak@8, Mutex@0x10}`，锁字 `+0x10`、中毒位 `+0x14`、数据 `+0x18`（arm64 `ldaxr w9,[x+0x10]` / `ldrb w8,[x+0x14]`；x86 `lock cmpxchg dword [x+0x10]`）。析构做**原子** `-1` + `Arc::drop_slow`（arm64 `0x4b30cc` `ldxr/stlxr` → `0x45f358`；DLL `0x180064176` `lock decq` → `0x18007f540`）⇒ 这一格是 `Arc` | **观测** |
//!
//! **尺寸的两条独立证据**（都不依赖 `docs/02` §22.3 的 vtable 表）：
//!
//! 1. `Rc<RefCell<GraphwarPlanePosManager>>` 的 `RcBox` 分配常量 = **`0x30` = 48 = 24 + 24**
//!    （24 = `{strong, weak, 借用位}`）：arm64 `0x419900` `mov w0, #0x30`（错误路径
//!    `0x4199dc` `handle_alloc_error(size=0x30)`）；x86 `0x523269` `mov edi, 0x30`；
//!    DLL `0x180141347` `mov $0x30,%ecx`（错误路径 `0x18014145f` 的 `dealloc(0x30)` 对得上）。
//! 2. **析构侧**同一个常量：arm64 `0x4b30a8` `mov w1,#0x30; mov w2,#8; bl __rust_dealloc`、
//!    DLL `0x18006415d` `mov $0x30,%edx`。两条路径（构造/释放）都指 0x30 ⇒ `size_of::<M>() = 24`。
//!
//! 交叉对照：`docs/02` §22.3 的 25 型 vtable 表也写 24（`dyn Any` vtable 第 2 字 − 8），
//! 两架构逐字相同 —— 那是**第三条**独立证据，但上两条已经足够，不依赖文档。
//!
//! # 三、`new` 与事件流（`观测`）
//!
//! ```text
//! new(event_handler: &mut GraphwarEventHandler, event_queue: Rc<RefCell<GraphwarEventQueue>>)
//!     -> Rc<RefCell<GraphwarPlanePosManager>>
//!   ├─ Rc::new_cyclic_in(...)                 arm64 0x3b89bc / x86 0x3c6d3f / DLL 0x1800da46d
//!   │    └─ 闭包体（构造 M）在 arm64 0x4198e0（412 B）、x86 0x523250、DLL 0x180141330
//!   ├─ subscribe_to_events_event_handler(&mut event_handler)   —— 内联，两条订阅：
//!   │    ├─ "new_connection_event" (len 20) → send_all_info_events        ← 闭包用 WeakCaller::call_0
//!   │    └─ "entity_removed_event" (len 20) → handle_entity_removed_event ← 闭包用 WeakCaller::call
//!   └─ 返回前 Rc 强计数 +1（arm64 0x3b8a84–0x3b8a90）
//! ```
//!
//! **订阅名**是 `.rodata` 里的原文：arm64 `0x19b2ac` = `new_connection_event`（`x2 = 0x14` = 20）、
//! `0x19b2c0` = `entity_removed_event`（同长），`0x3b8a2c` / `0x3b8a80` 都
//! `bl 0x486bc8`（`.symtab` 名 = `GraphwarEventHandler::subscribe_to_event`）；x86 同两条
//! （`0x1be298` / `0x1be2ac`，`0x3c6db2` / `0x3c6e03` 经 GOT `0x9bac88 → 0x3d38f0`）。
//!
//! **哪条订阅接哪个处理器**（x86 `.data.rel.ro` 的重定位 addend，`readelf -r` 直读）：
//!
//! ```text
//! 第一次 subscribe（"new_connection_event"）描述符 = 0x952b70
//!     reloc 0x952b88 -> 0x3c7170 ... lea 0x3c7189 -> 0x3c6a10  send_all_info_events
//!     reloc 0x952b90 -> 0x3c6d10 ... lea 0x3c6d1f -> 0x3c6a10  同上
//! 第二次 subscribe（"entity_removed_event"）描述符 = 0x952b98
//!     reloc 0x952bb0 -> 0x3c71e0 ... lea 0x3c71fc -> 0x3c6b40  handle_entity_removed_event
//!     reloc 0x952bb8 -> 0x3c6cf0 ... lea 0x3c6d02 -> 0x3c6b40  同上
//! ```
//!
//! **它往出发的那一条**：`set_position` 与 `send_all_info_events` **都发 `pos_info_event`**
//! = **tag 26 = `PosInfo`**（`src/events.rs` 的判别值表，`docs/114`）：
//!
//! ```text
//! arm64 set_position  0x3b8214  mov x8,#0x1a; 0x3b8218 movk x8,#0x8000,lsl #48  ⇒ 0x8000_0000_0000_001a
//!                     载荷 = { tag@+0, pos[0..2]@+8, pos[2]@+0x18, id@+0x20 }
//! arm64 send_all_info 0x3b86a0/0x3b86ac 同一个常量（x27）
//! x86   set_position  0x3c659f  add r12,0x1b（r12 = 0x7fff_ffff_ffff_ffff）⇒ 同一个 tag
//! ```
//!
//! **它处理的那一条**：`handle_entity_removed_event` 先比 tag，不等就 `panic!("explicit panic")`
//! （arm64 `0x3b87b4` `mov x9,#0xe; movk 0x8000,lsl #48` = tag **14 = `EntityRemoved`**；
//! x86 `0x3c6b58` `lea rax,[r13+0xf]`），命中后 `HashMap::remove(&id)`（arm64 `0x3b8824`
//! `bl hashbrown::raw::RawTable::remove_entry`）—— **不发事件**。
//!
//! # 四、内层 `GraphwarPlanePosData` = 48 字节 = 一张 `HashMap<u32, 24 B POD>`（`观测`）
//!
//! `ArcInner<Mutex<GraphwarPlanePosData>>` 的分配常量是 **`0x48` = 72 = 16 + 8 + 48**
//! （arm64 `0x419988` `mov w0, #0x48`、x86 `0x52330d` `mov edi, 0x48`、DLL 释放路径
//! `0x180141470` 的 `dealloc(0x48)`）⇒ 内层 **48 字节**；72 − 16（ArcInner 头）− 8
//! （`Mutex` 的 `{futex u32, poison bool}`）= 48。
//!
//! 48 = `HashMap<K, V>` 的固定尺寸（`RawTable` 32 + `RandomState` 16），字段偏移逐条对上：
//!
//! | 偏移 | 是什么 | 证据 |
//! |---|---|---|
//! | `+0x00` | `RawTable.ctrl`（`NonNull<u8>`） | arm64 `0x3b7fdc` `ldp x9,x8,[x22,#0x18]`（x9 = ctrl = Data+0x00） |
//! | `+0x08` | `RawTable.bucket_mask` | 同上（x8 = Data+0x08），`and x11, x0, x8` |
//! | `+0x10` | `RawTable.growth_left` | 未单独观测（`RawTable` 的既有布局，`156` §46.4 同口径） |
//! | `+0x18` | `RawTable.items`（长度） | arm64 `0x3b7fb4` `ldr x8,[x22,#0x30]` + `cbz` = 空表短路；x86 `0x3c630c` `cmp qword [r12+0x30],0` |
//! | `+0x20` | `RandomState`（16 B，`k0`/`k1`） | arm64 `0x3b7fc0` `add x0,x22,#0x38` → `bl BuildHasher::hash_one`；x86 `0x3c6318` `lea rdi,[r12+0x38]` → `0x47dec0` |
//!
//! **桶步长 = 32**（键 4 B → 对齐到 8，值 24 B）：arm64 `0x3b8008` `sub x13, x9, x13, lsl #5`
//! （×32）、`0x3b800c` `ldur w14,[x13,#-0x20]`（键 = 桶首 **u32**，与 `cmp w21, w14` 比）、
//! `0x3b805c` `ldur q0,[x13,#-0x18]`（16 B）+ `0x3b8058` `ldur x8,[x13,#-8]`（8 B）= 值 **24 B**；
//! x86 同一套：`0x3c6377` `shl r10,5`、`0x3c6381` `cmp ebp,[r9-0x20]`、
//! `0x3c63c3` `movdqu xmm0,[r9-0x18]` + `0x3c63bb` `mov rax,[r9-8]`。
//!
//! **为什么值没有析构**：`insert` / `remove_entry` 之后**没有任何 drop 调用**
//! （`set_position` 0x3b81dc–0x3b81ec 只是解锁；`handle_entity_removed_event` 0x3b8828
//! 之后直接解锁）⇒ 值是 **POD**（没有 `Vec` / `String` 那种 24 B 类型）。
//!
//! **值的语义（`推断`，两条依据）**：24 B POD 最可能是 `[f64; 3]`。① `set_position` 从
//! `x2`/`rdx` 搬三份：arm64 `0x3b8184` `ldp d8,d9,[x2]` + `0x3b8188` `ldr d10,[x2,#0x10]`
//! （`d` 寄存器 = 三个 f64）；② 事件载荷与**线格式** `PosInfo` 的字段逐一对应
//! （`src/_scaffold/protocol.rs`：`{"PosInfo":{"entity_id":..,"pos_x":..,"pos_y":..,"radius":..}}`，
//! 三个 f64 + 一个 id）⇒ 值 = `(pos_x, pos_y, radius)`。**缺的证据**：没有一处把这三份当
//! f64 参与算术（全程只搬不算），所以"是不是 `[f64;3]` 而不是任意 24 B POD"**没有指令级铁证**。
//!
//! `GraphwarPlanePosData::new` 只在 DLL 里有独立副本（`0x1800cd770`），而且**被 LLVM 与
//! `GraphwarTeamData::new` 合并成同一份**（两个型都是"空 HashMap"，代码逐字节相同）——这本身
//! 就是"48 B = 一张空 HashMap"的旁证：`0x1800cd778` 读 `RandomState::new::KEYS` 那个
//! `thread_local`（经 `__imp_…RandomState3new4KEYS`）→ `0x1800cd7a6` `movups %xmm0,0x20(%rsi)`
//! （`k0/k1` 落在 **+0x20**）、`0x1800cd798`/`0x1800cd7a3` 把空表常量写到 `+0x10` / `+0x00`。
//!
//! # 五、落码：本文件复原了什么、没复原什么
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 24 B / 3 字段布局 | 上表 | [`GraphwarPlanePosManager`]，`const` 硬闸 + 逐字段单测 | **观测**（三构建一致） |
//! | 内层 48 B | 一张 `HashMap<u32, [f64;3]>` | [`GraphwarPlanePosData`]（单字段包一层，尺寸不变） | **观测** |
//! | `get_position` / `get_all_positions` / `set_position` / `handle_entity_removed_event` | 见 §一 | 同名方法，语义逐条对上（锁 → 表操作 → 解锁 → 发事件） | **观测** |
//! | `send_all_info_events` | 遍历所有键，逐个 `get_position` 后发 `PosInfo` | 同（顺序 = `HashMap` 迭代序，**不可依赖**） | **观测** |
//! | **读表接口**（任务 41） | 原版没有；消费方直接用 `get_all_positions` 的散列序 | [`GraphwarPlanePosManager::get_all_positions_sorted`] / [`GraphwarPlanePosManager::positions_sorted`]（先调原版同名方法再按 id 排序，§八） | 我方补充（顺序口径差异） |
//! | 发事件 | `PosInfo`(26)，载荷 `{pos, id}` | 只把 **tag** 放进 [`GraphwarEventQueue`]（本 crate 的 `Event` 是**无载荷**枚举）；载荷由表内容唯一决定 | **观测**（tag）/ 载荷**未建模** |
//! | 订阅 | 2 条（`new_connection_event` / `entity_removed_event`），注册在 `&mut GraphwarEventHandler` 上 | **本 crate 没有 `GraphwarEventHandler` 的实现** ⇒ 订阅名以 [`SUBSCRIBED`] 常量记录 + 单测钉死，`new` 不注册 | 我方差异（**不是**原版行为） |
//! | `new` 的第一个实参 | `&mut GraphwarEventHandler` | 去掉（同上） | 我方差异 |
//! | `Mutex` 中毒 | `lock().unwrap()`（arm64 `0x3b8140` `bl core::result::unwrap_failed`，消息 = `called \`Result::unwrap()\` on an \`Err\` value` 43 B） | 同（`.lock().unwrap()`） | **观测** |
//!
//! # 六、仍是推断 / 未知（诚实清单）
//!
//! | 项 | 档位 | 缺什么证据 |
//! |---|---|---|
//! | 值类型精确写法（`[f64;3]` vs `(f64,f64,f64)` vs `struct PlanePos{f64×3}`） | **推断**（24 B POD 是**观测**） | 没有任何一处读到字段名/算术；只能由线格式 `PosInfo` 反推 |
//! | 三个 f64 的**语义**（`pos_x` / `pos_y` / `radius`？） | **推断** | 同上；`protocol.rs` 的字段名来自真实报文，但"表里存的就是这三个"没做点对点验证 |
//! | `get_all_positions` 的**顺序** | **未知** | 就是 `RawTable` 的迭代序（散列序），原版没有排序调用；跨进程不确定。我方消费方一律走 §8.2 的排序接口 |
//! | `PosInfo` 的**字段排布**（`{pos, id}` 还是 `{id, pos}`） | **观测**（`{pos@+8, id@+0x20}`，两架构一致） | — |
//! | 谁调 `set_position` | **观测**：`graphwar_remote_event_handler`（x86 `0x3e2830` 内 `0x3e3347`）与 `0x465a70`（`locs` 投 `graphwar_level_generator.rs`）；全部 35 个 `get_position` 调用点在 §〇 已列 | — |
//! | `+0x00` 弱自句柄**谁在用** | **观测**（`new` 里为两条订阅闭包各 `Weak::clone` 一次） | 管理器内部**没有**一处读它；"谁替它订阅"= 它自己（在 `new` 里） |
//! | 订阅后 `GraphwarEventHandler` 里存的是什么 | **未知** | 本 crate 没有该类型的实现；闭包 = `WeakCaller::{call_0, call}`（`src/weak_caller.rs` 有 2 行 `GraphwarPlanePosManager` 的记录） |
//!
//! # 七、与我方其它模块的关系（**别再重复实现**）
//!
//! * `px = 20x + 500` 那条坐标换算 **不属于本模块**（§〇），它在
//!   `src/graphwar_plane.rs::to_pixel`（`graphwar_plane.rs:174` 的锚，`docs/16` §42）；
//! * `EntityDrawer::draw`（`0x4b2890`）与 `EntityDrawer::process`（`0x4b4b60`）**都读**
//!   `get_all_positions` / `get_position` ⇒ 准备圈/实体绘制的位置来源就是这张表
//!   （接法见 §八；`src/_scaffold/entity_drawer_class.rs` 从任务 41 起读这张表）。
//!
//! # 八、读表接口：原版的顺序是**散列序**，消费方必须自己排序（任务 41）
//!
//! ## 8.1 消费方到底怎么读这张表（`观测`，逐个调用点）
//!
//! `EntityDrawer::draw`（x86 `0x4b2890`，ELF FDE `0x4b2890..0x4b3feb` = 5979 B）里这条链：
//!
//! ```text
//! 0x4b2b08  mov  rbx, [r12+0x238]   ; r12 = [root+0x1668]（根节点里的游戏客户端单元）
//! 0x4b2b24  lea  rdi, [rbx+0x18]    ; &RefCell<GraphwarModuleContainer>
//! 0x4b2b28  call 0x46ad30           ; GraphwarModuleContainer::get_module::<GraphwarPlanePosManager>
//! 0x4b2b2d  mov  rbp, rax           ; Rc<RefCell<GraphwarPlanePosManager>>（RcBox 指针）
//! ...
//! 0x4b2dc3  call 0x46aa50           ; get_module::<GraphwarTickHandler>（同一段里 9 个 get_module）
//! 0x4b2e10  call 0x519530           ; GraphwarTickHandler::get_fractional_tick ⇒ V
//! 0x4b2e2f  mov  rsi, rbp           ; rbp = Manager RcBox + 0x18 = &GraphwarPlanePosManager
//! 0x4b2e32  call 0x3c66a0           ; get_all_positions(&Manager, &mut Vec<u32>)   ← 就是这张表
//! 0x4b2e88  lea  rax, [rcx+rax*4]   ; 元素宽度 4 ⇒ Vec<u32>（键 = 实体 id）
//! 0x4b3135  mov  r15d, [rcx]        ; 循环体：按 **Vec 原序**逐个取 id
//! 0x4b314b  call 0x3c62b0           ; get_position(id) —— None 就跳过（0x4b3155 jne 0x4b37f0）
//! 0x4b315b  movsd xmm0, [rsp+0x80]  ; x = pos[0] → [rsp+0xb0]
//! 0x4b316d  movsd xmm0, [rsp+0x88]  ; y = pos[1] → [rsp+0xa8]
//! 0x4b317f  movsd xmm0, [rsp+0x90]  ; r = pos[2] → [rsp+0x60]（乘 1.5 才是画出来的半径）
//! 0x4b3199  call 0x454950           ; GraphwarLifeManager::get_alive(id)  —— 门 1
//! 0x4b31ac  call 0x462b50           ; GraphwarTurnManager::is_on_turn(id) —— 门 2
//! ```
//!
//! 同一张表的第二个消费方：`EntityDrawer::process`（x86 `0x4b4b60`）在 `0x4b4f24`
//! 调 `get_all_positions`、`0x4b5004` 调 `get_position`（同一套"取 id → 取三份 f64"）。
//!
//! **`0x4b2e32`（取表）与 `0x4b3130`（循环头）之间没有任何排序调用**：这一段共 5 条 `call`
//! = `get_all_positions` 自己 + 1 条 `BuildHasher::hash_one`（`0x47dec0`，查 `map[i]` 用）
//! + 3 条箭头绘制（`0x4b9a70`）；没有 `slice::sort*` / `sort_unstable*` 的调用点，
//! 循环直接从 `Vec` 的第 0 个元素开始。⇒ **原版用散列序直接画**（与 §六"顺序未知"一致）。
//!
//! ## 8.2 我方接口（**原版没有这两个方法**）
//!
//! 散列序跨帧/跨进程不定 ⇒ 绘制顺序会抖（同一帧的内容一样，但 `draw_arc`/`draw_polygon`
//! 的先后变，`last_arc` 这类观测口也就不稳定）。任务 41 起，所有消费方都改走：
//!
//! | 方法 | 返回 | 用途 |
//! |---|---|---|
//! | [`GraphwarPlanePosManager::get_all_positions_sorted`] | `Vec<u32>`（id 升序） | **键**的稳定顺序（原版 `get_all_positions` 的排序版） |
//! | [`GraphwarPlanePosManager::positions_sorted`] | `Vec<(u32, PlanePos)>` | id 与三份 f64 一起给，绘制侧的唯一读表入口 |
//!
//! 两者都**先调原版同名方法再排序**（`get_all_positions()` / `get_position(id)`），
//! 不是另开一条读 `HashMap` 的路——顺序是唯一被我方改动的量（`推断`不到，直接是口径差异）。
//! 排序键 = **实体 id 升序**（`sort_unstable`，id 唯一所以稳定）。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::events::Event;
use crate::graphwar_event_queue::GraphwarEventQueue;

// ---------------------------------------------------------------------------
// 常量：尺寸 / 事件 tag / 订阅名（全部有地址证据，见模块文档 §二–§四）
// ---------------------------------------------------------------------------

/// 原版 `GraphwarPlanePosManager` 的 `size_of`（`观测`：三构建的 `RcBox` 分配/释放常量 `0x30`）。
pub const MANAGER_SIZE: usize = 24;

/// 原版 `GraphwarPlanePosData` 的 `size_of`（`观测`：`ArcInner<Mutex<…>>` 分配常量 `0x48` = 16+8+48）。
pub const DATA_SIZE: usize = 48;

/// `set_position` / `send_all_info_events` 发的那条事件：`pos_info_event` = **tag 26**。
///
/// 证据：arm64 `0x3b8214`/`0x3b86ac` 的 `mov x8,#0x1a + movk #0x8000,lsl #48`；x86 `0x3c659f`。
pub const POS_INFO_EVENT_TAG: u16 = Event::PosInfo.wire_tag();

/// `handle_entity_removed_event` 认的那条事件：`entity_removed_event` = **tag 14**。
///
/// 证据：arm64 `0x3b87b4` 的 `mov x9,#0xe`（不等就 `panic!("explicit panic")`）；x86 `0x3c6b58`。
pub const ENTITY_REMOVED_EVENT_TAG: u16 = Event::EntityRemoved.wire_tag();

/// `new` 里的两条订阅（原文来自 `.rodata`：arm64 `0x19b2ac` / `0x19b2c0`，各 20 字节）。
///
/// ⚠️ 本 crate 没有 `GraphwarEventHandler` 的实现 ⇒ 这**不是**一个活的订阅表，
/// 只是一份"原版订了哪两条"的证据记录（`size_of == 24` 的硬闸不允许再加字段）。
pub const SUBSCRIBED: [&str; 2] = ["new_connection_event", "entity_removed_event"];

/// 值类型：24 字节 POD（`观测`），语义按线格式读作 `(pos_x, pos_y, radius)`（**推断**，见 §四）。
pub type PlanePos = [f64; 3];

// ---------------------------------------------------------------------------
// 内层数据：48 字节 = 一张 HashMap（`观测`）
// ---------------------------------------------------------------------------

/// 原版 `GraphwarPlanePosData` —— **48 字节**，就是一张 `HashMap<u32, PlanePos>`。
///
/// 原版是 `type` 别名还是单字段 newtype **未知**（尺寸上二者相同：48 B），本文件用单字段
/// 结构体，好处是可以在类型上挂方法名（`new` / `get_position` / `get_all_positions`）。
#[derive(Debug, Default)]
pub struct GraphwarPlanePosData {
    /// `实体 id → (pos_x, pos_y, radius)`。
    pub positions: HashMap<u32, PlanePos>,
}

impl GraphwarPlanePosData {
    /// `GraphwarPlanePosData::new`（DLL `0x1800cd770`，被 LLVM 与 `GraphwarTeamData::new`
    /// 合并成同一份）：一张空表。
    pub fn new() -> Self {
        Self { positions: HashMap::new() }
    }

    /// `GraphwarPlanePosData::get_position`（DLL `0x1800d9ac0`；arm64 内联进 `manager` 同名方法）。
    pub fn get_position(&self, id: u32) -> Option<PlanePos> {
        self.positions.get(&id).copied()
    }

    /// `GraphwarPlanePosData::get_all_positions`（DLL `0x1800d9bb0`）：**键的集合**。
    ///
    /// 顺序 = `HashMap` 迭代序（散列序）；原版没有排序调用（§六）。
    pub fn get_all_positions(&self) -> Vec<u32> {
        self.positions.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------------
// 管理器本体：24 字节 = 3 个 8 字节字段（`观测`）
// ---------------------------------------------------------------------------

/// 原版 `GraphwarPlanePosManager` —— **24 字节 / 3 个 8 字节字段**（`观测`）。
///
/// 字段顺序与偏移来自三份构建（arm64 2.0.60 / x86 Linux 2.0.60 / Windows 2.0.60 DLL）
/// 的一致指令序列；`size_of` 与 `offset_of` 由本文件末尾的 `const` 硬闸 + 单测钉死。
///
/// ⚠️ 原版的 `new` 还有一个 `&mut GraphwarEventHandler` 实参（用来注册两条订阅）；
/// 本 crate 没有那个类型，所以 [`GraphwarPlanePosManager::new`] 只收队列（§五）。
#[derive(Debug)]
pub struct GraphwarPlanePosManager {
    /// `+0x00`：指向**自己**的弱句柄（`Rc::new_cyclic` 给的那个 `Weak` 的克隆；
    /// `new` 里为两条订阅闭包各 `Weak::clone` 一次）。
    pub self_weak: Weak<RefCell<GraphwarPlanePosManager>>,
    /// `+0x08`：事件队列（`set_position` / `send_all_info_events` 用它 `add_event`）。
    pub event_queue: Rc<RefCell<GraphwarEventQueue>>,
    /// `+0x10`：`Arc<Mutex<…>>` —— 4 个数据方法都先拿这把锁。
    pub data: Arc<Mutex<GraphwarPlanePosData>>,
}

// —— 编译期硬闸：改错一个字段类型/顺序就编不过（`观测`，三构建逐偏移一致）——
/// 原版 `GraphwarPlanePosManager` 是 **24 字节**。
const _: () = assert!(std::mem::size_of::<GraphwarPlanePosManager>() == MANAGER_SIZE);
/// 原版 `GraphwarPlanePosData` 是 **48 字节**（= 一张 `HashMap`）。
const _: () = assert!(std::mem::size_of::<GraphwarPlanePosData>() == DATA_SIZE);
/// 48 = `HashMap<u32, [f64;3]>` 的固定尺寸（`RawTable` 32 + `RandomState` 16）。
const _: () = assert!(std::mem::size_of::<HashMap<u32, PlanePos>>() == DATA_SIZE);
/// 三个字段的偏移（`+0x00` / `+0x08` / `+0x10`）。
const _: () = assert!(std::mem::offset_of!(GraphwarPlanePosManager, self_weak) == 0x00);
const _: () = assert!(std::mem::offset_of!(GraphwarPlanePosManager, event_queue) == 0x08);
const _: () = assert!(std::mem::offset_of!(GraphwarPlanePosManager, data) == 0x10);

impl GraphwarPlanePosManager {
    /// 原版尺寸（24 B），给 `root_wiring` 的台账与验收用。
    pub const SIZE: usize = MANAGER_SIZE;

    /// 原版 `new`（arm64 `0x3b89a0` / x86 `0x3c6d30` / DLL `0x1800da460`）。
    ///
    /// 形状照原版：**返回 `Rc<RefCell<Self>>`**，弱自句柄由 `Rc::new_cyclic` 的 `&Weak`
    /// 直接给出（所以 `+0x00` 里存的就是 `RcBox` 指针，见模块文档 §二）。
    ///
    /// ⚠️ 与 §五 一样，原版的第一个实参 `&mut GraphwarEventHandler` 去掉了；
    /// 两条订阅的名字记在 [`SUBSCRIBED`]，**不注册**（本 crate 没有那个类型）。
    pub fn new(event_queue: Rc<RefCell<GraphwarEventQueue>>) -> Rc<RefCell<Self>> {
        Rc::new_cyclic(|weak: &Weak<RefCell<Self>>| {
            RefCell::new(Self {
                self_weak: weak.clone(),
                event_queue,
                data: Arc::new(Mutex::new(GraphwarPlanePosData::new())),
            })
        })
    }

    /// 原版 4 个数据方法都先拿的那把锁（`Arc<Mutex<GraphwarPlanePosData>>`）。
    ///
    /// 原版是 `lock().unwrap()`：中毒时 `core::result::unwrap_failed`（arm64 `0x3b8140`，
    /// 消息 = `called \`Result::unwrap()\` on an \`Err\` value`）。
    fn lock(&self) -> MutexGuard<'_, GraphwarPlanePosData> {
        self.data.lock().expect("GraphwarPlanePosData: Mutex 中毒（原版是 unwrap 后 panic）")
    }

    /// 原版 `Arc<Mutex<Data>>` 的地址（**我方探针**，原版没有这个方法；用它证明"那一格确实是
    /// 这个 `Arc` 占着"）。
    pub fn data_arc_ptr(&self) -> *const Mutex<GraphwarPlanePosData> {
        Arc::as_ptr(&self.data)
    }

    /// `get_position`（arm64 `0x3b7f58` / x86 `0x3c62b0`）：`data.lock().positions[id]`。
    pub fn get_position(&self, id: u32) -> Option<PlanePos> {
        self.lock().get_position(id)
    }

    /// `set_position`（arm64 `0x3b8160` / x86 `0x3c64c0`）：写表 **+ 发 `PosInfo`(26)**。
    ///
    /// 逐指令对照（arm64）：`0x3b8180` 取 `M+0x10` → 加锁 → `0x3b81d8` `HashMap::insert` →
    /// 解锁（`0x3b81ec`）→ `0x3b8200` 取 `M+0x08`（队列）→ 写借用位 `0x3b820c` →
    /// 组载荷（pos, id）→ `0x3b8234` `add_event`。**先解锁再发**，本函数同序。
    pub fn set_position(&self, id: u32, pos: PlanePos) {
        {
            let mut data = self.lock();
            data.positions.insert(id, pos);
        }
        self.emit_pos_info(id, pos);
    }

    /// `get_all_positions`（arm64 `0x3b8348` / x86 `0x3c66a0`）：**键**的集合（`Vec<u32>`）。
    ///
    /// ⚠️ 顺序 = `RawTable` 的迭代序（散列序，§六 标 `未知`）——原版 `EntityDrawer::draw`
    /// 就是**照这个原序**画的（§8.1：`0x4b2e32` 之后直接 `0x4b3135 mov r15d,[rcx]`）。
    /// 需要可复现的绘制顺序时用 [`Self::get_all_positions_sorted`]。
    pub fn get_all_positions(&self) -> Vec<u32> {
        self.lock().get_all_positions()
    }

    /// **我方补充**（原版没有这个方法）：`get_all_positions` 的**稳定顺序**版本 —— id 升序。
    ///
    /// 先调原版同名方法取键，再按 id 排序；只改顺序，不改集合（见 §8.2）。
    pub fn get_all_positions_sorted(&self) -> Vec<u32> {
        let mut ids = self.get_all_positions();
        ids.sort_unstable();
        ids
    }

    /// **我方补充**：按 id 升序的 `(id, (pos_x, pos_y, radius))` 快照 —— 绘制侧唯一的读表入口。
    ///
    /// 形状照原版 `draw` 的循环（`get_all_positions` → 逐 id `get_position`，§8.1），
    /// 只是**先排序**：`0x4b314b` 那个 `None 就跳过` 的分支在这里是 `filter_map`。
    pub fn positions_sorted(&self) -> Vec<(u32, PlanePos)> {
        self.get_all_positions_sorted()
            .into_iter()
            .filter_map(|id| self.get_position(id).map(|pos| (id, pos)))
            .collect()
    }

    /// `send_all_info_events`（arm64 `0x3b8660` / x86 `0x3c6a10`）：把**当前全表**逐条重发一遍。
    ///
    /// 原版逐指令：`0x3b8688` `bl get_all_positions` → 对每个 id `0x3b86d0` `bl get_position`
    /// → 非 `None` 才 `0x3b870c` `add_event(PosInfo)`。返回值我方给"发了几条"（原版返回 `()`，
    /// 但**发射条数**是原版行为的确定量，探针需要它；语义未改）。
    pub fn send_all_info_events(&self) -> usize {
        let mut emitted = 0;
        for id in self.get_all_positions() {
            // 原版这里也是"再查一次"，`None` 就跳过（`0x3b86dc b.ne`）。
            if let Some(pos) = self.get_position(id) {
                self.emit_pos_info(id, pos);
                emitted += 1;
            }
        }
        emitted
    }

    /// `handle_entity_removed_event`（arm64 `0x3b8798` / x86 `0x3c6b40`）：
    /// **只认 tag 14**，命中后把该 id 从表里删掉（**不发事件**）。
    ///
    /// 原版 tag 不符时 `panic!("explicit panic")`（arm64 `0x3b8878` `bl core::panicking::panic`，
    /// 字符串 = `0x19b29e` 的 `explicit panic`，14 B）；本实现返回 `Err` 交给调用方，
    /// **不 panic**（gdext FFI 里 panic = abort，我方口径同 `_scaffold` 其它 manager）。
    ///
    /// 返回 `Ok(true)` = 原来有、删掉了；`Ok(false)` = 表里本来没有。
    pub fn handle_entity_removed_event(&self, tag: u16, id: u32) -> Result<bool, &'static str> {
        if tag != ENTITY_REMOVED_EVENT_TAG {
            return Err("GraphwarPlanePosManager::handle_entity_removed_event 只认 tag 14（EntityRemoved）");
        }
        Ok(self.lock().positions.remove(&id).is_some())
    }

    /// 发一条 `PosInfo`（tag 26）。**载荷 `{pos, id}` 未建模**（本 crate 的 `Event` 是无载荷
    /// 枚举），只有 tag 进队列；载荷由调用点给出，见模块文档 §三。
    fn emit_pos_info(&self, _id: u32, _pos: PlanePos) {
        self.event_queue.borrow_mut().add_event(Event::PosInfo);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    /// 24 字节 = 3 个 8 字节字段，偏移 `+0x00` / `+0x08` / `+0x10`
    /// （`观测`：三份构建逐偏移一致，见文件头 §二）。
    #[test]
    fn layout_matches_the_original_24_bytes() {
        assert_eq!(size_of::<GraphwarPlanePosManager>(), 24, "原版 24 字节");
        assert_eq!(offset_of!(GraphwarPlanePosManager, self_weak), 0x00);
        assert_eq!(offset_of!(GraphwarPlanePosManager, event_queue), 0x08);
        assert_eq!(offset_of!(GraphwarPlanePosManager, data), 0x10);
        assert_eq!(size_of::<Weak<RefCell<GraphwarPlanePosManager>>>(), 8, "弱自句柄一个指针");
        assert_eq!(size_of::<Rc<RefCell<GraphwarEventQueue>>>(), 8, "事件队列一个指针");
        assert_eq!(size_of::<Arc<Mutex<GraphwarPlanePosData>>>(), 8, "数据 Arc 一个指针");
        // 3 × 8 = 24：没有空洞、没有尾巴
        assert_eq!(
            8 + 8 + 8,
            size_of::<GraphwarPlanePosManager>(),
            "3 个字段正好铺满 24 字节"
        );
    }

    /// 内层 48 字节 = 一张 `HashMap<u32, [f64;3]>`（`观测`，`ArcInner` 分配常量 `0x48`）。
    #[test]
    fn inner_data_is_one_hashmap_of_48_bytes() {
        assert_eq!(size_of::<GraphwarPlanePosData>(), 48);
        assert_eq!(offset_of!(GraphwarPlanePosData, positions), 0);
        assert_eq!(size_of::<HashMap<u32, PlanePos>>(), 48);
        // 桶步长 32 = key(4 → 补到 8) + value(24)，与 arm64 `lsl #5` / x86 `shl r10,5` 一致。
        assert_eq!(size_of::<(u32, PlanePos)>(), 32);
        assert_eq!(size_of::<PlanePos>(), 24, "值 24 字节（三个 f64，`推断`）");
    }

    /// 事件 tag 的两条硬数字：发 26、收 14（`观测`，见文件头 §三）。
    #[test]
    fn event_tags_are_the_original_constants() {
        assert_eq!(POS_INFO_EVENT_TAG, 26, "pos_info_event");
        assert_eq!(ENTITY_REMOVED_EVENT_TAG, 14, "entity_removed_event");
        // 名字 → 判别值的那张表（`0x3d38f0` 的派发层）必须同源
        assert_eq!(Event::from_dispatch_name("pos_info_event"), Some(Event::PosInfo));
        assert_eq!(Event::from_dispatch_name("entity_removed_event"), Some(Event::EntityRemoved));
        // 订阅名也必须是那张表里的原文（原版订的就是这两条）
        for name in SUBSCRIBED {
            assert!(Event::from_dispatch_name(name).is_some(), "{name} 不在派发表里");
        }
        assert_eq!(SUBSCRIBED.len(), 2, "原版 new 里正好两条 subscribe");
        // 与兄弟模块的同名常量必须同值（`graphwar_life_manager` 也在用 tag 14）
        assert_eq!(
            ENTITY_REMOVED_EVENT_TAG,
            crate::graphwar_life_manager::ENTITY_REMOVED_EVENT_TAG,
            "两个模块认的是同一条 `entity_removed_event`"
        );
    }

    /// 写 → 读回；全表；删除；`send_all_info_events` 的条数。
    #[test]
    fn position_table_reads_back_what_was_written() {
        let queue = Rc::new(RefCell::new(GraphwarEventQueue::new()));
        let manager = GraphwarPlanePosManager::new(Rc::clone(&queue));
        let m = manager.borrow();

        assert_eq!(m.get_position(7), None);
        assert!(m.get_all_positions().is_empty());

        m.set_position(7, [1.5, -2.5, 0.5]);
        assert_eq!(m.get_position(7), Some([1.5, -2.5, 0.5]));
        assert_eq!(m.get_all_positions(), vec![7]);
        // set_position 发一条 PosInfo
        assert_eq!(queue.borrow().len(), 1);

        // 覆盖同一格：不新增条目，仍发一条
        m.set_position(7, [0.0, 1.0, 2.0]);
        assert_eq!(m.get_position(7), Some([0.0, 1.0, 2.0]));
        assert_eq!(m.get_all_positions().len(), 1);
        assert_eq!(queue.borrow().len(), 2);

        // 全量重播：表里 1 格 ⇒ 1 条
        let _ = queue.borrow_mut().send_buffer();
        assert_eq!(m.send_all_info_events(), 1);
        assert_eq!(queue.borrow().len(), 1);

        // 删除：tag 不符报错（原版 panic），tag 14 删掉且不发事件
        assert!(m.handle_entity_removed_event(26, 7).is_err());
        assert_eq!(queue.borrow().len(), 1, "报错那次一条都不发");
        let _ = queue.borrow_mut().send_buffer();
        assert_eq!(m.handle_entity_removed_event(ENTITY_REMOVED_EVENT_TAG, 7), Ok(true));
        assert_eq!(m.get_position(7), None);
        assert!(queue.borrow().is_empty(), "handle_entity_removed_event 不发事件");
        assert_eq!(m.handle_entity_removed_event(ENTITY_REMOVED_EVENT_TAG, 7), Ok(false));
    }

    /// **两顺序插入 → 排序后的读法一致**（任务 41 的核心断言，见 §8.2）。
    ///
    /// 原版 `get_all_positions` 是散列序（§六 `未知`）⇒ 同一批 id 的 `Vec` 顺序不可依赖。
    /// 这条测试钉住"**排序后的读法与插入序无关**"，并且排序后 id 与自己的三份 f64 不错位。
    #[test]
    fn sorted_read_is_independent_of_insertion_order() {
        let pairs: [(u32, PlanePos); 6] = [
            (9, [9.0, 0.5, 1.0]),
            (2, [2.0, 0.5, 1.0]),
            (7, [7.0, 0.5, 1.0]),
            (1, [1.0, 0.5, 1.0]),
            (4, [4.0, 0.5, 1.0]),
            (3, [3.0, 0.5, 1.0]),
        ];
        let new_manager = || {
            GraphwarPlanePosManager::new(Rc::new(RefCell::new(GraphwarEventQueue::new())))
        };
        let forward = new_manager();
        let backward = new_manager();
        // 正序插入 vs 逆序插入（同一批 6 对，一个字符都没差）
        for &(id, pos) in &pairs {
            forward.borrow().set_position(id, pos);
        }
        for &(id, pos) in pairs.iter().rev() {
            backward.borrow().set_position(id, pos);
        }

        // ① 两个顺序插入 ⇒ 稳定读法完全一致（id 升序 = 1..9 里那 6 个）
        assert_eq!(forward.borrow().get_all_positions_sorted(), vec![1, 2, 3, 4, 7, 9]);
        assert_eq!(
            forward.borrow().get_all_positions_sorted(),
            backward.borrow().get_all_positions_sorted(),
            "两顺序插入 ⇒ 排序后的键序一致"
        );
        // ② 带值的快照也逐字节一致（id + 三份 f64）
        assert_eq!(
            forward.borrow().positions_sorted(),
            backward.borrow().positions_sorted(),
            "两顺序插入 ⇒ 排序后的 (id, pos) 快照一致"
        );
        // ③ 排序没有把值和键拆散：值的第一份就是自己的 id（构造时这么填的）
        for (id, pos) in forward.borrow().positions_sorted() {
            assert_eq!(pos, [id as f64, 0.5, 1.0], "id {id} 的三份 f64 错位了");
        }
        // ④ 只改顺序、不改集合：排序版的长度 = 原版 get_all_positions() 的长度
        assert_eq!(
            forward.borrow().positions_sorted().len(),
            forward.borrow().get_all_positions().len()
        );
        // ⑤ 一条数据都没有时是空表（不能"排出一个假条目"）
        let empty = new_manager();
        assert!(empty.borrow().positions_sorted().is_empty());
        assert!(empty.borrow().get_all_positions_sorted().is_empty());
    }

    /// `Rc::new_cyclic` 的弱自句柄指向自己（`+0x00` 那一格真的是 `Weak`），
    /// 且 `Arc<Mutex<…>>` 在 `+0x10`（指针非空、强计数 1）。
    #[test]
    fn self_weak_points_at_itself_and_data_arc_is_live() {
        let m = GraphwarPlanePosManager::new(Rc::new(RefCell::new(GraphwarEventQueue::new())));
        assert_eq!(m.borrow().self_weak.strong_count(), 1, "弱句柄能升成强引用 ⇒ 指的就是自己");
        assert!(Rc::ptr_eq(
            &m.borrow().self_weak.upgrade().expect("弱自句柄必须能升级"),
            &m
        ));
        assert!(!m.borrow().data_arc_ptr().is_null());
        assert_eq!(Arc::strong_count(&m.borrow().data), 1);
        assert_eq!(GraphwarPlanePosManager::SIZE, 24);
    }
}
