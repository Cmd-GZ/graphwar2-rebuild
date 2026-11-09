//! 外观索引：`GraphwarGraphicsManager`（皮肤 / 脸 / 帽子三张表 + 三条广播）。
//!
//! 本模块回答三件事：**外观是"谁设置的"**、**三条 `*_graphics_info_event` 怎么发**、
//! **`GraphwarTextureLoader` 到底在哪**（→ §3，答案是**不在本模块**）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **vaddr**（Ghidra = vaddr + `0x100000`）；arm64 侧来自未 strip 的
//! 2.0.60 `libgraphwarrust.so` 的 `.symtab`。
//!
//! ## 1. 函数清单（全部已定位）
//!
//! | 函数 | x86 vaddr | arm64 vaddr / 字节 | 源码行 | 职责 | 等级 |
//! |---|---|---|---|---|---|
//! | `new` | `0x469390` | `0x49e394` / 396 | 38 | 订阅 `new_connection_event` + `entity_removed_event` | 逐位 |
//! | `set_skin_graphics` | `0x468c70` | `0x49dd00` / 276 | 60 | 写皮肤表 + 发 `SkinGraphicsInfo`(51) | 逐位 |
//! | `set_face_graphics` | `0x468b60` | `0x49dbec` / 276 | 69 | 写脸表 + 发 `FaceGraphicsInfo`(53) | 逐位 |
//! | `set_hat_graphics` | `0x468a50` | `0x49dad8` / 276 | 78 | 写帽子表 + 发 `HatGraphicsInfo`(55) | 逐位 |
//! | `send_all_info_events` | `0x468d80` | `0x49de14` / 1036 | 88 | 遍历三张表逐条重播 | 逐位 |
//! | `handle_entity_removed_event` | `0x469220` | `0x49e220` / 280 | 108 | 从三张表删 | 逐位 |
//! | `subscribe_to_events_event_handler` | 内联进 `new` | 2 个闭包 `0x49e338` / `0x49e368` | — | 订阅胶合层 | 逐位 |
//!
//! 6 个方法名 + 6 个 `panic::Location` 锚（`fixtures/name_to_addr.tsv` 的
//! `graphwar_graphics_manager` 行，`:38/:60/:69/:78/:88/:108`）双向唯一，
//! 所以这张表的名字与地址**都不是猜的**。
//!
//! ## 2. 数据形状（逐位）
//!
//! 三个 setter 的骨架完全同形（每个 276 B），差别只有三处常量：
//!
//! ```text
//! set_X_graphics(self, player_id: u32, name: &str)
//!   st = self[8]                       ; 事件通道（+0x10 borrow 标志、+0x18 emit 入口）
//!   if st[0x10] != 0 → panic           ; "src/graphwar_graphics_manager.rs"
//!   event = { tag: 0x8000…|TAG          ; TAG = 0x33 / 0x35 / 0x37
//!             String(cap,ptr,len) @+8,  ; name
//!             u32 player_id       @+0x20 }
//!   emit(st + 0x18, &event)
//!   map.insert(player_id, name)        ; map = self + 0x18 / 0x48 / 0x78
//! ```
//!
//! | 表 | 起始偏移 | 事件 | 判别值 | 名字长度 |
//! |---|---|---|---|---|
//! | 皮肤 | `+0x18` | `skin_graphics_info_event` | **51** = `0x33` | 22 |
//! | 脸 | `+0x48` | `face_graphics_info_event` | **53** = `0x35` | 22 |
//! | 帽子 | `+0x78` | `hat_graphics_info_event` | **55** = `0x37` | 21 |
//!
//! 三张表**间距 0x30**，每张是 `HashMap { ctrl@+0, bucket_mask@+8, growth_left@+0x10,
//! items@+0x18, RandomState@+0x20 }`（`handle_entity_removed_event` 里
//! `hash(self+0x38/+0x68/+0x98)` = 表基 + 0x20，正好是各自的 hasher）。
//! 键是 `u32`（**实体 id**），值是 `String`（24 字节，cap 在前）。
//!
//! ⚠️ **2026-11-08（任务 42）就地更正**：本节此前写"键是 `player_id`"。线格式里这三个事件的
//! 字段名是 **`entity_id`**（`{"SkinGraphicsInfo":{"entity_id":229,"graphics_str":"yellow"}}`，
//! `src/_scaffold/protocol.rs`，**逐位**），而 `EntityDrawer::get_entity_*_sprite` 用的也是
//! 同一个键（`docs/16-client.md` §46.3）。⇒ 键空间是**实体 id**，不是玩家 id。
//! 形状（`u32` 键 + 24 B `String` 值 + 表间距 `0x30`）是 `逐位`，语义这一条改成**观测**。
//!
//! `send_all_info_events` 用基数 **`0x8000…0033` 再加 `k`**（`k = 0/2/4`）造三个判别值，
//! 因此三个循环分别播 51 / 53 / 55：先皮肤（表 `+0x18`，直接用基数）、
//! 再脸（`+0x48`，`add x19, x9, #2`）、再帽子（`+0x78`，`orr x27, x9, #4`）。
//! 注意 `0x33 | 2 == 0x33`，所以"脸"那一支**只能**是加法——编译器在两条支上
//! 混用了 `add` 与 `orr`，这也正好证明基数确实是 `0x33` 而不是别的。
//!
//! ### 2.1 第三处重播的 tag = **55**（2026-11-09，任务 77 在 x86 上逐位复核）
//!
//! 上一轮留下的问题是"第三处是 `mov rax,[rsp+0x10]` 间接载入，只读到字面量 51/53"。
//! x86 `send_all_info_events`（`0x468d80`）里三处是**同一个套路**，第三个的真值在
//! `0x469035` 就写死了：
//!
//! ```text
//! 皮肤 0x468e93 movabs rax, 0x8000000000000033          → 0x468eac add_event    (= 51)
//! 脸   0x468ef8 movabs rcx, 0x8000000000000033
//!      0x468f02 add   rcx, 2                            → 存 [rsp+0x10]        (= 53)
//! 帽   0x469035 movabs rdx, 0x8000000000000033
//!      0x46903f or    rdx, 4                            → 存 [rsp+0x10]        (= 55)
//! 公共尾 0x46910e mov rax,[rsp+0x10] ; 0x469113 mov [rsp+0x30], rax
//!        0x46911d lea rsi,[rsp+0x30] ; 0x469122 call [0x9bac80]
//! ```
//!
//! ⇒ 第三处**就是 55（帽子）**，而且"间接载入"的谜底是：三支共用同一段提交代码，
//! tag 从栈槽 `rsp+0x10` 取，所以字面量扫描只看到基址。
//! 
//! ### 2.2 168 B = **0x18 头** + 三张 0x30 的表（任务 77 结案；旧说法已改正）
//!
//! 上一轮（`docs/161` §2 第 16 行 / §3.5）写的是「`0xa8` 之后还有 `0x30` 字节没解」。
//! **这句是错的**：`0xa8 = 168` 正好是 `size_of`，三张表从 `+0x18` 起、每张 `0x30`，
//! 到 `+0xa8` 就结束了，**表后面没有字节**。没解的是**表前面**那 `0x18` 的头三个字。
//! 建盒函数 `0x522710`（唯一调用点 `0x4693a2`）逐条写下来就是它们：
//!
//! ```text
//! 0x522731 mov edi,0xc0 / call __rust_alloc        ; 0xc0 = 16 + 8 + 168
//! 0x5227ca mov [rbx+0x10], 0                       ; RefCell 借用字 = 未借
//! 0x5227d2 mov [rbx+0x18], rbx                     ; M+0x00 = 盒基 = 弱自句柄
//! 0x5227d6 mov [rbx+0x20], r14                     ; M+0x08 = 事件通道 Rc（调用点 0x517d60 inc [r14] 强计数）
//! 0x5227da mov [rbx+0x28], r15                     ; M+0x10 = 容器弱引用（调用点 0x517d6a inc [r15+8] 弱计数）
//! 0x522796/0x5227a1 → [rbx+0x30..0x50]             ; 皮肤表（ctrl/mask/growth/items 32 B）
//! 0x5227ac mov [rbx+0x50], rax ; 0x5227b0 mov [rbx+0x58], rdx   ; 皮肤表 RandomState（M+0x38/+0x40）
//! 0x5227b4/0x5227b8 → [rbx+0x60..0x80]             ; 脸表
//! 0x5227bc mov [rbx+0x80], rcx ; 0x5227c3 mov [rbx+0x88], rdx   ; 脸表 RandomState（M+0x68/+0x70）
//! 0x5227de/…      → [rbx+0x90..0xb0]               ; 帽子表
//!                                                  ; 帽子表 RandomState 在 M+0x98/+0xa0 = 盒+0xb0/+0xb8
//! ```
//!
//! 三个头的**语义**与 `graphwar_color_manager` §6 完全同款（弱自句柄 / 事件通道 /
//! 容器弱引用），这也是 22 个 manager 的公共头形状。
//!
//! ## 2.3 事件流接线（任务 77 逐条复核，x86；`wired` 的第二件）
//!
//! ```text
//! new 0x469390
//!   0x4693fc lea rsi,[0x1c18ad] 'new_connection_event' + 0x46940a mov edx,0x14
//!   0x469403 lea r8,<0x958330>  +0x20 = 0x469350 → lea rsi,[0x468d80] send_all_info_events
//!   0x469415 call [0x9bac88] → 0x3d38f0    == subscribe_to_event
//!   0x46944d lea rsi,[0x1c18c1] 'entity_removed_event' + 0x46945b mov edx,0x14
//!   0x469454 lea r8,<0x958358>  +0x20 = 0x469370 → lea rsi,[0x469220] handle_entity_removed_event
//!   0x469466 call [0x9bac88] → 0x3d38f0
//!
//! 发（六处 = 三个 setter + 三处整表重播）
//!   set_skin_graphics 0x468c70: 0x468cde call [0x9bac80]（51）
//!   set_face_graphics 0x468b60: 0x468bce call [0x9bac80]（53）
//!   set_hat_graphics  0x468a50: 0x468abe call [0x9bac80]（55）
//!   send_all_info_events 0x468d80: 0x468eac（51）/ 0x468fe6（53）/ 0x469122（55）
//! ```
//!
//! ## 3. `GraphwarTextureLoader` 在哪？（问题的正面答案：**不在本模块**）
//!
//! arm64 `.symtab` 里模块段就是 **`graphwar_texture_loader`**：
//!
//! ```text
//! _ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader3new
//! _ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader16get_skin_texture
//! _ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader16get_face_texture
//! _ZN12graphwarrust23graphwar_texture_loader21GraphwarTextureLoader15get_hat_texture
//! ```
//!
//! ⇒ `GraphwarTextureLoader` 属于 **`src/graphwar_texture_loader.rs`**，与
//! `graphwar_graphics_manager.rs` 是两个模块。`graphics_manager` 只管
//! **`player_id → 名字`** 三张索引表；贴图本体在 loader 里。
//!
//! ### 3.1 它管哪些贴图、怎么索引（逐位）
//!
//! | getter | arm64 vaddr / 字节 | 表偏移 | 表内容 |
//! |---|---|---|---|
//! | `get_skin_texture` | `0x40c1cc` / 296 | `+0x150` | `HashMap<String, Gd<Texture2D>>` |
//! | `get_face_texture` | `0x40c0a4` / 296 | `+0x180` | 同上 |
//! | `get_hat_texture` | `0x40bf7c` / 296 | `+0x1b0` | 同上 |
//! | `new` | `0x40c2f4` / **21212** | — | 一次把所有资源读进来 |
//!
//! 三个 getter 也是完全同形（296 B），核心是 hashbrown 查表：
//!
//! ```text
//! if self[MAP+0x18] == 0 (items) → None           ; 空表快路
//! h = hash(name, state = self[MAP+0x20])
//! ctrl = self[MAP+0];  mask = self[MAP+8]
//! probe: 逐字节比 ctrl 的 tag 位 → 命中桶后
//!        if key.len != entry.len → 下一个
//!        if memcmp(key.ptr, entry.ptr, key.len) == 0 → 命中
//! 命中 → clone 值（8 字节 Gd）
//! ```
//!
//! ⇒ **按名字（`String`）索引**，不是按编号；键是 SVG/资源名本身。
//! 表项 **0x28 = 40 字节**（`mov w25, #0x28`；1 字节 ctrl + 24 字节 `String` + 8 字节 `Gd`，对齐到 40）。
//!
//! **`docs/02` §1187 还列了 `get_chat_texture` / `get_time_mode_texture`**：
//! arm64 `.symtab` 里**没有**这两个符号（只有上面 4 个），所以它们要么被内联/合并，
//! 要么是旧文档多列了 ⇒ **标未知**（`docs/02` §1187 的 6 个方法名里 4 个有符号）。
//!
//! ### 3.2 谁创建、何时加载（观测）
//!
//! `GraphwarTextureLoader::new` 全二进制**只有一个调用点 `0x39a328`**，落在
//! `impl INode for GraphwarRootNode`（arm64 `0x399dd0` 起）里 ⇒ **由
//! `GraphwarRootNode` 在 ready 阶段构造一次**（这与
//! `GraphwarRootNode + 0x16a0` 持有 `Weak<Rc<RefCell<GraphwarTextureLoader>>>`
//! 的 drop 符号一致：`core::ptr::drop_in_place<alloc::rc::Weak<core::cell::RefCell<
//! graphwarrust::graphwar_texture_loader::GraphwarTextureLoader>>>`）。
//!
//! `new` 有 **21212 字节** ⇒ 加载是**一次性、饿汉式**的（不是按需懒加载）。
//!
//! ### 3.3 谁消费（getter 的全部调用点，观测）
//!
//! | getter | 调用点（arm64） | 所在函数 |
//! |---|---|---|
//! | skin | `0x3e0fa0` `0x440174` `0x44e034` `0x44fe64` `0x4c1270` `0x4c8018` | `PlayScreenControl::add_player_victory_icon`、`PlayerShowcase`、`PlayerScreenControl::load_items` / `update_buttons_background`、`EntityDrawer::get_entity_skin_sprite`、`SetupControl::update_soldiers` |
//! | face | `0x3e1154` `0x4401b0` `0x44e3b8` `0x4c0d9c` `0x4c8238` | `add_player_victory_icon`、`PlayerShowcase`、`load_items`、`EntityDrawer::get_entity_face_sprite`、`update_soldiers` |
//! | hat | `0x3e1308` `0x4401ec` `0x44e81c` `0x4bfdb8` | `add_player_victory_icon`、`PlayerShowcase`、`load_items`、`EntityDrawer::get_entity_hat_sprite` |
//!
//! `EntityDrawer::get_entity_{skin,face,hat}_sprite` 三个函数名本身就是证据：
//! **画一个士兵时的三张图，就是这三个 getter 取的**（`docs/16` §21/§22）。
//!
//! ⚠️ `docs/16` §21/§22 记的"`PlayerShowcase` 自己有三张 `HashMap`（`+0x168/+0x198/+0x1c8`，
//! 间距 0x30）"：**这三个偏移正好是 loader 三张表的 `items` 字段**
//! （`+0x150+0x18` / `+0x180+0x18` / `+0x1b0+0x18`）。两种读法都能解释字节：
//! ① 那三处是 `get_*_texture` 内联后被误当成 PlayerShowcase 的字段（**推断**）；
//! ② PlayerShowcase 另有一套同形的缓存表（**未证**）。本模块**不选边**，只记录这条歧义。
//!
//! ## 4. 外观数据从哪来（观测）
//!
//! 三个 setter 的调用点（arm64 全量扫描）：
//!
//! | 调用点 | 所在函数 |
//! |---|---|
//! | `0x3e4df4` `0x3e4e84` `0x3e5060` `0x3e50f0` `0x3e5294` `0x3e5324` | `GraphwarRemoteEventHandler::handle_client_remote_event`（`0x3e3ad0`） |
//! | `0x3e6f34` `0x3e794c` `0x3e7f18` | `GraphwarRemoteEventHandler::handle_server_remote_event`（`0x3e6df0`） |
//! | `0x4a5868`（skin） | `GraphwarGameStateManager::add_target`（`0x4a5704`） |
//! | `0x4aaf24` `0x4ab050`（face/hat） | `GraphwarGameStateManager::add_soldier_to_player`（`0x4aaab4`） |
//!
//! ⇒ 外观**只从远端事件**（`*_graphics_info_event`）进入本管理器；本地不生成名字。
//! `add_target` / `add_soldier_to_player` 是"新实体落地时补一次外观"的两条旁路。
//!
//! ## 5. 未确定
//!
//! * 表值到底是 `String` 还是 Godot `GString`：setter 里先 `bl 0x82fda8` 把入参转成
//!   24 字节三字结构（cap/ptr/len），`HashMap` 值也是 24 字节 ⇒ **形状**是
//!   `{cap,ptr,len}`（**逐位**）；具体是哪个类型**标推断**。
//! * `self[8]` 那个"事件通道"对象的类型（它有 `+0x10` borrow 标志与 `+0x18` emit 入口）。
//! * `docs/02` 多列的两个 getter（§3.1 末）。

/// `skin_graphics_info_event` 的判别值。
pub const EVENT_SKIN_GRAPHICS_INFO: u16 = 51;
/// `face_graphics_info_event` 的判别值。
pub const EVENT_FACE_GRAPHICS_INFO: u16 = 53;
/// `hat_graphics_info_event` 的判别值。
pub const EVENT_HAT_GRAPHICS_INFO: u16 = 55;
/// `new_connection_event` 的判别值（订阅 1）。
pub const EVENT_NEW_CONNECTION: u16 = 5;
/// `entity_removed_event` 的判别值（订阅 2）。
pub const EVENT_ENTITY_REMOVED: u16 = 14;

/// 三类外观。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphicsKind {
    /// 皮肤（`SkinGraphicsInfo` = 51）。
    Skin,
    /// 脸（`FaceGraphicsInfo` = 53）。
    Face,
    /// 帽子（`HatGraphicsInfo` = 55）。
    Hat,
}

impl GraphicsKind {
    /// 事件判别值。
    pub const fn tag(self) -> u16 {
        match self {
            GraphicsKind::Skin => EVENT_SKIN_GRAPHICS_INFO,
            GraphicsKind::Face => EVENT_FACE_GRAPHICS_INFO,
            GraphicsKind::Hat => EVENT_HAT_GRAPHICS_INFO,
        }
    }

    /// 事件名（snake_case，`.rodata` 里被比较的键）。
    pub const fn event_name(self) -> &'static str {
        match self {
            GraphicsKind::Skin => "skin_graphics_info_event",
            GraphicsKind::Face => "face_graphics_info_event",
            GraphicsKind::Hat => "hat_graphics_info_event",
        }
    }

    /// 这一条事件在 `src/events.rs` 里的变体（判别值见 [`Event::wire_tag`]）。
    pub const fn event(self) -> crate::events::Event {
        match self {
            GraphicsKind::Skin => crate::events::Event::SkinGraphicsInfo,
            GraphicsKind::Face => crate::events::Event::FaceGraphicsInfo,
            GraphicsKind::Hat => crate::events::Event::HatGraphicsInfo,
        }
    }

    /// 对应的表在 `self` 里的字节偏移。
    pub const fn map_offset(self) -> usize {
        match self {
            GraphicsKind::Skin => 0x18,
            GraphicsKind::Face => 0x48,
            GraphicsKind::Hat => 0x78,
        }
    }

    /// `player_id` 字段在事件载荷里的偏移（payload `+0x20`，事件结构从 `+0` 的 tag 算起）。
    pub const ID_OFFSET: usize = 0x20;

    /// 三种外观，按 `send_all_info_events` 的播放顺序（皮肤 → 脸 → 帽子）。
    pub const ALL: [GraphicsKind; 3] = [GraphicsKind::Skin, GraphicsKind::Face, GraphicsKind::Hat];

    /// `send_all_info_events` 造判别值的方式：基数 `0x8000…0033` 再加 `0/2/4`。
    ///
    /// 三处编码各不相同但等价：皮肤直接用基数，脸是 `add x19, x9, #2`，
    /// 帽子是 `orr x27, x9, #4`（`0x33 | 4 == 0x33 + 4`，而 `0x33 | 2 == 0x33`，
    /// 所以脸**必须**是加法——这一条是从 `add` 与 `orr` 的混用反推出来的）。
    pub const fn tag_bits(self) -> u64 {
        0x8000_0000_0000_0033u64
            + match self {
                GraphicsKind::Skin => 0,
                GraphicsKind::Face => 2,
                GraphicsKind::Hat => 4,
            }
    }
}

/// 弱自句柄在 `self` 里的偏移（值 = 盒基）。
pub const SELF_WEAK_OFFSET: usize = 0x00;
/// 事件通道 `Rc<RefCell<GraphwarEventQueue>>` 在 `self` 里的偏移。
pub const EVENT_QUEUE_OFFSET: usize = 0x08;
/// 容器弱引用在 `self` 里的偏移（模块文档 §2.2）。
pub const CONTAINER_WEAK_OFFSET: usize = 0x10;
/// 三个字的头 —— 三张表从第一张的 0x18 起。
pub const HEADER_SIZE: usize = 0x18;
/// 一张外观表的步长（字节）：三张表首尾相接。
pub const MAP_STRIDE: usize = 0x30;
/// `HashMap` 里 `items` 字段相对表基的偏移。
pub const MAP_ITEMS_OFFSET: usize = 0x18;
/// `HashMap` 里 hasher（`RandomState`）相对表基的偏移。
pub const MAP_HASHER_OFFSET: usize = 0x20;
/// 事件载荷里名字字段的偏移（24 字节 `{cap,ptr,len}`）。
pub const NAME_OFFSET: usize = 0x08;

/// 一条订阅。
#[derive(Debug, Clone, Copy)]
pub struct Subscription {
    /// 事件名。
    pub name: &'static str,
    /// 判别值。
    pub tag: u16,
    /// 处理方法名。
    pub handler: &'static str,
}

/// `GraphwarGraphicsManager::new` 的两条订阅（逐位，顺序即代码顺序）。
pub const SUBSCRIPTIONS: [Subscription; 2] = [
    Subscription {
        name: "new_connection_event",
        tag: EVENT_NEW_CONNECTION,
        handler: "send_all_info_events",
    },
    Subscription {
        name: "entity_removed_event",
        tag: EVENT_ENTITY_REMOVED,
        handler: "handle_entity_removed_event",
    },
];

/// 一条广播出去的外观信息（事件载荷的可判定部分）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsInfo {
    /// 哪一类外观。
    pub kind: GraphicsKind,
    /// 玩家 id（载荷 `+0x20`）。
    pub player_id: u32,
    /// 资源名（载荷 `+0x08` 的三字结构）。
    pub name: String,
}

impl GraphicsInfo {
    /// 事件判别值（= `kind.tag()`）。
    pub fn tag(&self) -> u16 {
        self.kind.tag()
    }

    /// 完整 tag word（`0x8000_0000_0000_0000 | tag`）。
    pub fn tag_word(&self) -> u64 {
        0x8000_0000_0000_0000 | u64::from(self.kind.tag())
    }
}

/// `GraphwarGraphicsManager` 的可判定状态：三张 `player_id → 名字` 表。
///
/// # 事件流（任务 77 补，`wired` 的第二件；见模块文档 §2.3）
///
/// `new`（x86 `0x469390`）订阅 [`SUBSCRIPTIONS`] 那两条：
/// `new_connection_event`@`0x469415` → 闭包槽 `0x958330`（`+0x20` = `0x469350`）→ `0x468d80`；
/// `entity_removed_event`@`0x469466` → 闭包槽 `0x958358`（`+0x20` = `0x469370`）→ `0x469220`。
/// 发方向六处：三个 setter 各一条（51/53/55）+ `send_all_info_events` 三处重播
/// （`0x468eac`/`0x468fe6`/`0x469122`，tag 也是 51/53/55，逐位见模块文档 §2.1）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphwarGraphicsManager {
    /// 皮肤表（`self + 0x18`）。
    pub skins: std::collections::BTreeMap<u32, String>,
    /// 脸表（`self + 0x48`）。
    pub faces: std::collections::BTreeMap<u32, String>,
    /// 帽子表（`self + 0x78`）。
    pub hats: std::collections::BTreeMap<u32, String>,
    /// 订阅名（`new` 的两条 `subscribe`，顺序 = 调用序，`观测`）。
    pub subscribed: Vec<&'static str>,
    /// 出站缓冲：三个 setter 里那条 `GraphwarEventQueue::add_event`（`观测`）。
    ///
    /// 原版 setter **自己**把事件塞进 `self[8]` 那条队列（x86 `0x468cde` 的
    /// `call *GOT 0x9bac80 -> 0x3d3530`）；本 crate 的 `GraphwarEventQueue` 是
    /// **无载荷的 tag 队列**，而这里的事件带 `(entity_id, name)`，所以事件先落在这条
    /// `outbox`，由持有队列的一方（`_scaffold::root_wiring::KernelBus`）搬进去。
    /// **差异**：搬运点从 setter 体内挪到了调用点；tag 与顺序不变。
    pub outbox: Vec<crate::events::Event>,
}

impl GraphwarGraphicsManager {
    /// 新建空管理器 + 两条订阅（原版 `new` 做的同一件事）。
    ///
    /// 原版 `new`（x86 `0x469390`）= 建盒（`0x522710`，`0xc0` = 16+8+168）+ 两条订阅；
    /// 168 B 的切法是 `0x18` 头 + 三张 `0x30` 表（模块文档 §2.2）。
    pub fn new() -> Self {
        Self {
            subscribed: SUBSCRIPTIONS.iter().map(|s| s.name).collect(),
            ..Self::default()
        }
    }

    /// 取走发件箱（`set_*_graphics` 已经写进去了；原版没有这个函数，事件直接进队列）。
    pub fn take_outbox(&mut self) -> Vec<crate::events::Event> {
        std::mem::take(&mut self.outbox)
    }

    /// 发件箱里现在有几条。
    pub fn outbox_len(&self) -> usize {
        self.outbox.len()
    }

    fn table_mut(&mut self, kind: GraphicsKind) -> &mut std::collections::BTreeMap<u32, String> {
        match kind {
            GraphicsKind::Skin => &mut self.skins,
            GraphicsKind::Face => &mut self.faces,
            GraphicsKind::Hat => &mut self.hats,
        }
    }

    /// 某个 kind 的表。
    pub fn table(&self, kind: GraphicsKind) -> &std::collections::BTreeMap<u32, String> {
        match kind {
            GraphicsKind::Skin => &self.skins,
            GraphicsKind::Face => &self.faces,
            GraphicsKind::Hat => &self.hats,
        }
    }

    /// 三个 setter 的共同实现：写表 + 把该发的那条事件放进发件箱 + 返回它的载荷。
    ///
    /// 顺序照原版：**先发事件、再写表**（x86 `0x468cde` 的 `add_event` 在 `HashMap::insert`
    /// 之前；arm64 `0x49dd00` 同序）。本函数把"发事件"落成 `outbox.push`（见字段注释），
    /// 所以两件事的**先后**保持为 先 push 后 insert。
    pub fn set_graphics(&mut self, kind: GraphicsKind, player_id: u32, name: &str) -> GraphicsInfo {
        self.outbox.push(kind.event());
        self.table_mut(kind).insert(player_id, name.to_string());
        GraphicsInfo { kind, player_id, name: name.to_string() }
    }

    /// `set_skin_graphics`（`SkinGraphicsInfo` = 51，表 `+0x18`）。
    pub fn set_skin_graphics(&mut self, player_id: u32, name: &str) -> GraphicsInfo {
        self.set_graphics(GraphicsKind::Skin, player_id, name)
    }

    /// `set_face_graphics`（`FaceGraphicsInfo` = 53，表 `+0x48`）。
    pub fn set_face_graphics(&mut self, player_id: u32, name: &str) -> GraphicsInfo {
        self.set_graphics(GraphicsKind::Face, player_id, name)
    }

    /// `set_hat_graphics`（`HatGraphicsInfo` = 55，表 `+0x78`）。
    pub fn set_hat_graphics(&mut self, player_id: u32, name: &str) -> GraphicsInfo {
        self.set_graphics(GraphicsKind::Hat, player_id, name)
    }

    /// `send_all_info_events`：把三张表全部重播一遍。
    ///
    /// 播放顺序是**表顺序**（皮肤 → 脸 → 帽子），表内顺序由 `HashMap` 的
    /// `RandomState` 决定 ⇒ **原版这里的顺序不可复现**（与 `team_manager` 的
    /// `TeamInfo` 广播同源，`docs/10` §5.4）。本实现用有序表，只为让断言稳定，
    /// **不代表**原版的遍历顺序（标推断）。
    pub fn send_all_info_events(&self) -> Vec<GraphicsInfo> {
        let mut out = Vec::new();
        for kind in GraphicsKind::ALL {
            for (id, name) in self.table(kind) {
                out.push(GraphicsInfo { kind, player_id: *id, name: name.clone() });
            }
        }
        out
    }

    /// `new_connection_event` 的处理（闭包槽 `0x958330` → `0x469350` → `0x468d80`）：整表重播。
    ///
    /// 原版三个循环逐条 `add_event`（51 / 53 / 55）；本实现把同样条数记进发件箱并返回载荷。
    pub fn handle_new_connection_event(&mut self) -> Vec<GraphicsInfo> {
        let events = self.send_all_info_events();
        self.outbox.extend(events.iter().map(|e| e.kind.event()));
        events
    }

    /// `handle_entity_removed_event`（`0x469220`）：从三张表各删一次。
    ///
    /// 返回真正删掉的条数（原版三张表各无条件 `remove` 一次，不看返回值）。
    pub fn handle_entity_removed_event(&mut self, player_id: u32) -> usize {
        let mut n = 0;
        for kind in GraphicsKind::ALL {
            if self.table_mut(kind).remove(&player_id).is_some() {
                n += 1;
            }
        }
        n
    }

    /// 某个玩家的三类外观，按 皮肤/脸/帽子 顺序给出。
    pub fn graphics_of(&self, player_id: u32) -> [Option<String>; 3] {
        [
            self.skins.get(&player_id).cloned(),
            self.faces.get(&player_id).cloned(),
            self.hats.get(&player_id).cloned(),
        ]
    }
}

// ---------------------------------------------------------------------------
// `graphwar_texture_loader` 的事实表
// ---------------------------------------------------------------------------
//
// 这些常量属于**另一个模块**（`src/graphwar_texture_loader.rs`，本分支未落码），
// 放在这里是因为它们是"外观从名字到贴图"那一步的**唯一**索引依据，
// 与本模块的三张 `player_id → 名字` 表是同一套体系的两端。

/// `GraphwarTextureLoader` 的三张贴图表在结构体里的偏移（间距 [`MAP_STRIDE`]）。
pub const TEXTURE_MAP_OFFSETS: [usize; 3] = [0x150, 0x180, 0x1b0];

/// 表项大小（字节）：ctrl 1 + `String` 24 + `Gd` 8，对齐到 40。
pub const TEXTURE_ENTRY_SIZE: usize = 0x28;

/// 贴图表与 getter 的对应关系。
#[derive(Debug, Clone, Copy)]
pub struct TextureGetter {
    /// 哪一类外观。
    pub kind: GraphicsKind,
    /// getter 名字。
    pub name: &'static str,
    /// arm64 vaddr。
    pub arm64_addr: u64,
    /// arm64 字节数。
    pub arm64_size: u32,
    /// 表在 loader 里的偏移。
    pub map_offset: usize,
}

/// 三个 getter（逐位；`new` 另有一个，见 [`TEXTURE_LOADER_NEW`]）。
pub const TEXTURE_GETTERS: [TextureGetter; 3] = [
    TextureGetter {
        kind: GraphicsKind::Skin,
        name: "get_skin_texture",
        arm64_addr: 0x40c1cc,
        arm64_size: 296,
        map_offset: TEXTURE_MAP_OFFSETS[0],
    },
    TextureGetter {
        kind: GraphicsKind::Face,
        name: "get_face_texture",
        arm64_addr: 0x40c0a4,
        arm64_size: 296,
        map_offset: TEXTURE_MAP_OFFSETS[1],
    },
    TextureGetter {
        kind: GraphicsKind::Hat,
        name: "get_hat_texture",
        arm64_addr: 0x40bf7c,
        arm64_size: 296,
        map_offset: TEXTURE_MAP_OFFSETS[2],
    },
];

/// `GraphwarTextureLoader::new`（饿汉式加载）。
pub const TEXTURE_LOADER_NEW: (u64, u32) = (0x40c2f4, 21212);
/// 唯一构造点（`impl INode for GraphwarRootNode` 里，arm64）。
pub const TEXTURE_LOADER_CTOR_SITE: u64 = 0x39a328;
/// `GraphwarRootNode` 持有它的偏移。
pub const TEXTURE_LOADER_OWNER_OFFSET: usize = 0x16a0;

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn maps_are_thirty_bytes_apart() {
        let offs: Vec<usize> = GraphicsKind::ALL.iter().map(|k| k.map_offset()).collect();
        assert_eq!(offs, vec![0x18, 0x48, 0x78]);
        for w in offs.windows(2) {
            assert_eq!(w[1] - w[0], MAP_STRIDE);
        }
    }

    #[test]
    fn tags_are_0x33_or_two_bits() {
        assert_eq!(GraphicsKind::Skin.tag(), 0x33);
        assert_eq!(GraphicsKind::Face.tag(), 0x35);
        assert_eq!(GraphicsKind::Hat.tag(), 0x37);
        assert_eq!(GraphicsKind::Hat.tag_bits(), 0x8000_0000_0000_0037);
        // 三个 setter 各发一条事件（x86 `0x468cde` 那句 `add_event`；tag = 51/53/55）
        assert_eq!(
            GraphicsKind::ALL.map(|k| k.event().wire_tag()),
            [EVENT_SKIN_GRAPHICS_INFO, EVENT_FACE_GRAPHICS_INFO, EVENT_HAT_GRAPHICS_INFO]
        );
        assert_eq!([51u16, 53, 55], GraphicsKind::ALL.map(|k| k.event().wire_tag()));
    }

    /// `set_*_graphics` 把事件放进发件箱（原版直接进队列），写表的语义不变。
    #[test]
    fn setters_leave_one_event_in_the_outbox() {
        let mut g = GraphwarGraphicsManager::new();
        assert_eq!(g.outbox_len(), 0);
        g.set_skin_graphics(229, "target");
        assert_eq!(g.skins.get(&229).map(String::as_str), Some("target"));
        g.set_face_graphics(229, "no_face");
        g.set_hat_graphics(229, "no_hat");
        assert_eq!(g.outbox_len(), 3);
        assert_eq!(
            g.take_outbox(),
            vec![
                crate::events::Event::SkinGraphicsInfo,
                crate::events::Event::FaceGraphicsInfo,
                crate::events::Event::HatGraphicsInfo
            ]
        );
        assert_eq!(g.outbox_len(), 0, "取走之后清空");
        assert_eq!(g.graphics_of(229)[0].as_deref(), Some("target"));
    }

    /// 两个订阅在 `new` 里落码；头部三个字的偏移与 168 B 的切法（模块文档 §2.2）。
    #[test]
    fn new_subscribes_and_168_splits_into_header_plus_three_tables() {
        let g = GraphwarGraphicsManager::new();
        assert_eq!(g.subscribed, vec!["new_connection_event", "entity_removed_event"]);
        assert_eq!(g.subscribed.len(), SUBSCRIPTIONS.len());
        assert_eq!(SUBSCRIPTIONS[0].handler, "send_all_info_events");
        assert_eq!(SUBSCRIPTIONS[1].handler, "handle_entity_removed_event");
        // 168 = 0x18 头 + 3 × 0x30 表，**表后面没有字节**（任务 77 改正）
        assert_eq!(HEADER_SIZE + 3 * MAP_STRIDE, 168);
        assert_eq!(MAP_STRIDE, 0x30);
        assert_eq!(
            (SELF_WEAK_OFFSET, EVENT_QUEUE_OFFSET, CONTAINER_WEAK_OFFSET),
            (0x00, 0x08, 0x10)
        );
    }

    /// 整表重播：三张表逐条发，tag 分别是 51/53/55（第三处 = 55，`0x46903f or rdx,4`）。
    #[test]
    fn replay_emits_51_53_55_in_table_order() {
        let mut g = GraphwarGraphicsManager::new();
        let _ = g.set_skin_graphics(1, "yellow");
        let _ = g.set_face_graphics(1, "smile");
        let _ = g.set_hat_graphics(1, "cap");
        let _ = g.take_outbox();
        let replayed = g.handle_new_connection_event();
        assert_eq!(replayed.len(), 3);
        let tags: Vec<u16> = replayed.iter().map(|i| i.tag()).collect();
        assert_eq!(tags, vec![51, 53, 55]);
        assert_eq!(
            g.outbox,
            vec![
                crate::events::Event::SkinGraphicsInfo,
                crate::events::Event::FaceGraphicsInfo,
                crate::events::Event::HatGraphicsInfo
            ]
        );
        // 删一条：三张表各删一次，**一条事件都不发**（0x469220 体内零 add_event）
        let _ = g.take_outbox();
        assert_eq!(g.handle_entity_removed_event(1), 3);
        assert_eq!(g.outbox_len(), 0);
        assert_eq!(g.graphics_of(1), [None, None, None]);
    }
}
