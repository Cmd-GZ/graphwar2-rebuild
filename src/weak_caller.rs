//! 原版 `src/weak_caller.rs` —— **弱自句柄借出 `&mut self` 再调一个函数的那个小工具**。
//!
//! 这是进度尺上的最后一个模块，也是 Rust ↔ Godot 之间的**回调管道**：
//! 每个 manager 手里握着一个指向**自己**的 `Weak`，事件队列里存的闭包通过它
//! 反向升级成 `Rc` 再回调。所以它回答的是「引擎侧还活着、Rust 侧没了怎么办」。
//!
//! # 四问四答（先看这一段）
//!
//! | 问题 | 结论 | 档位 |
//! |---|---|---|
//! | **(a) 包的是什么** | `Weak<RefCell<T>>`，**一个**类型参数 `T`，整个结构体 **8 字节**（一个 `NonNull`）。不是 `Mutex`、不是 `Rc<RefCell<..>>`、**不是 `dyn Trait`**（`&mut T` 是瘦指针，只占一个寄存器） | **观测** |
//! | **(b) 升级失败怎么办** | **静默跳过**：`upgrade()` 返回 `None` 就什么都不做、返回 `()`；不 panic、不打日志、没有返回值可查。唯一会 panic 的是 **`RefCell` 已经被可变借出**（`borrow_mut` 的 `already borrowed`） | **观测** |
//! | **(c) 那些 direct 符号是同一个泛型方法吗** | **不是**。是 **2 个泛型方法 × 26 个类型**：`call_0<F>`（无载荷）与 `call<A, F>`（带一个载荷），`F` 也泛型。x86 目标有 **39 = 23 + 16** 个外联单态化；arm64 2.0.60 只有 **35 = 21 + 14**（少的那几个被内联了，见 §5） | **观测** + 个别 **推断** |
//! | **(d) 谁在用它** | **26 个 manager / control 自己**。`subscribe_to_events` / `new` 里造的闭包捕获自己的弱句柄，回调时把 `T::handle_xxx`（`fn` 项）交给 `call`；`call_0` 专门给 `tick` / `send_all_info_events` 这类**无载荷**回调用 | **观测** |
//!
//! # (a) 数据结构：`Weak<RefCell<T>>`，8 字节，`self` 按值传入
//!
//! 一条 `call` 的完整骨架（arm64 2.0.60 `0x3d519c`，逐指令）：
//!
//! ```text
//!   cmn   x0, #1              ; x0 == -1  ⇒ Weak::new() 的哨兵（usize::MAX），直接跳过
//!   ldr   x9, [x0]            ; RcBox 头 +0x00 = strong
//!   cbz   x9, <weak–1>        ; strong == 0 ⇒ upgrade() 给出 None ⇒ 跳过
//!   adds  x9, x9, #1 ; str x9,[x0] ; b.hs <brk>   ; strong += 1（溢出 → abort）
//!   ldr   x9, [x0, #0x10]     ; RcBox +0x10 = RefCell 借用标志
//!   cbnz  x9, <panic_already_borrowed>            ; 已被借出 ⇒ panic
//!   mov   x9, #-1 ; str x9,[x0,#0x10]             ; 标志置 -1（WRITING）
//!   add   x0, x0, #0x18       ; &mut *guard == RcBox + 0x18 == &mut T
//!   mov   x8, x1 ; mov x1, x2 ; blr x8            ; f(&mut T, a)
//!   ldr   x8,[x0,#0x10] ; add x8,#1 ; str           ; 释放借用（-1 + 1 = 0）
//!   ldr   x8,[x19] ; sub ; str ; b.eq <Rc::drop_slow>   ; Rc 强计数 -1
//!   ldr   x8,[x19,#8] ; sub ; str ; b.eq <dealloc>      ; **self 的** Weak 弱计数 -1
//! ```
//!
//! 由此可以逐条钉死布局（`观测`）：
//!
//! | 事实 | 依据 |
//! |---|---|
//! | 整型指针，8 字节，无胖指针 | `cmn x0,#1` / `cmp $-1,%rdi` 直接拿**实参本身**当 `NonNull` 比；`&mut T` 只进一个寄存器（`add x0,x19,#0x18` / `lea 0x18(%rbx),%rdi`） |
//! | `RcBox` 布局 `{strong@0, weak@8, value@0x10}` | 三处偏移在 39 个单态化里**无一例外** |
//! | 值类型是 `RefCell<T>`，**不是** `Mutex` | 借用标志是 `Cell<isize>`：写 `-1`、释放 `+1`；失败走 `core::cell::panic_already_borrowed`，且该符号**存在**于 `.symtab` |
//! | 方法是 **`borrow_mut()`**，不是 `borrow()` | 见 §3 的两条独立证据 |
//! | `RcBox<RefCell<T>>` = `24 + size_of::<T>()` | 39 处内联 `__rust_dealloc(ptr, size, 8)` 的 `size` 常量；`24 = 8(strong)+8(weak)+8(借用标志)` |
//! | `self` **按值**消费 | 调用点只把指针装进 `x0`/`%rdi`（不取地址）；被调函数尾部**自己**做弱计数 `-1` 甚至 `dealloc` |
//!
//! `align` 常量在 39 处**全部是 8** ⇒ 26 个类型的 `align_of::<T>() <= 8`（没有一个含 `u128`/`#[repr(align(16))]`）。
//!
//! # (b) 升级失败：静默跳过；借用冲突：panic
//!
//! 三条**不同的**失败路径，处理方式完全不同（`观测`）：
//!
//! 1. **`Weak` 是哨兵**（`Weak::new()` / 默认构造出来的空句柄，指针 = `usize::MAX`）：
//!    `cmn x0,#1; b.eq <尾部>` —— 直接跳过，连 `strong` 都不读。
//! 2. **`strong == 0`**（`Rc` 已经被放掉）：`cbz` 跳过。
//! 3. **`RefCell` 已被借出**：`panic!`（`already borrowed`）；`strong` 溢出：`abort`。
//!
//! 前两条**没有任何**补偿动作：没有日志、没有 `unwrap`、没有返回值（`()`），
//! 也就是说 **「引擎侧还活着、Rust 侧没了」= 这次回调不存在**，不是错误。
//! 这与本项目的用法吻合：事件队列的闭包是**弱**订阅，module 被销毁后订阅自然失效，
//! 不需要反注册（`GraphwarModuleContainer` 那边也没有 `remove_module`）。
//!
//! # (c) 39 个 direct 符号 = 2 个泛型方法 × 26 个类型（不是 1 个方法 × 39 个类型）
//!
//! 两个方法名来自 `.symtab` 原文（arm64 2.0.60）：
//! `graphwarrust::weak_caller::WeakCaller::{call, call_0}`。
//! x86 目标被 strip，但两个 `panic::Location` 锚把它们**分成两块、互不重叠**：
//!
//! ```text
//! src/weak_caller.rs:11:28   16 个函数   0x3d2610..0x3d3410    全部等价于 call_0
//! src/weak_caller.rs:17:28   24 个函数   0x3d10e0..0x3d5260    其中 23 个是 call、1 个是「被内联进去的调用者」
//! ```
//!
//! 两块的判据是**实参寄存器个数**：`call_0` 只把 `f` 交给间接调用（`blr x1` / `call *%rsi`），
//! `call` 还要把载荷搬进后面的寄存器（arm64 `mov x1,x2`；x86 `mov %rdx,%rsi`）。
//! `GraphwarPlayerScreenControl` 那一个搬了**两个**寄存器 ⇒ 载荷 16 字节。
//!
//! 泛型参数一共三个：`T`（`Weak` 的目标）、`A`（载荷类型，`call` 独有）、`F`（回调类型）。
//! `F` 泛型（**不是**写死的 `fn(..)`）有一条硬证据：`GraphwarPlayScreenControl` 那个
//! 160/196 字节的单态化里**根本没有间接调用** —— 闭包体（`t.<+0x8b> = true`）被内联了进去；
//! 如果 `f` 是 `fn` 指针就不可能内联。反过来，同一个 `(T, A)` 的 5~7 个调用点**共用 1 个**
//! 单态化（`GraphwarGameStateManager` 7 个、`GraphwarServerConnectionHandler` 5 个），
//! 说明那些调用点把 `fn` 项**强制成了同一个 `fn` 指针类型**再传进来。
//!
//! 矩阵（`TYPES`）：**26 个类型**，其中 13 个 `call`/`call_0` 都有、10 个只有 `call`、3 个只有 `call_0`。
//! 类型名来自 x86 目标调用点交出的 `f`（`T::handle_xxx`），也就是**签名级的自证**：
//! `WeakCaller<GraphwarLifeManager>::call(..)` 的回调就是
//! `GraphwarLifeManager::handle_entity_removed_event`。**37/39** 行由此直接命名，
//! 剩下 2 行（`0x3d22a0` / `0x3d2b50`，闭包被内联、`f` 没有符号可查）按 arm64 孪生体补名，
//! 两条都在 `MONOS` 里留了 `f: &[]`，好让读者一眼看到「这两行是推断」。
//!
//! # (d) 谁在用：26 个 manager / control 的**自己**
//!
//! 句柄指向的就是 `T` 自己，不是容器、不是邻居。两条证据：
//!
//! * 回调收到的是 `&mut T`，而 `f` 一律是 **`T` 自己的方法**（`MONOS[].f` 的每一项都以
//!   该行 `ty` 开头，`tests/weak_caller.rs` 有断言）；
//! * `T` 的构造函数走 `Rc::new_cyclic`（arm64 2.0.60 里 21 个
//!   `alloc::rc::Rc::<..>::new_cyclic_in` 实例）—— 这是「先有 `Rc`、再由闭包拿到
//!   `Weak` 装进结构体」的唯一标准写法，对应
//!   `GraphwarPlanePosManager::new` 的 `bl Rc::new_cyclic_in`（arm64 `0x3b89bc`）。
//!
//! 用法分工非常整齐（`观测`，26 个类型无一例外）：
//!
//! | 方法 | 被调的回调 | 典型 |
//! |---|---|---|
//! | `call_0` | `T::tick`、`T::send_all_info_events`、`T::show_*_button` | 无载荷，「走一帧」/「重播全量状态」 |
//! | `call` | `T::handle_*`、`T::*_event`、`T::face_button_pressed` | 带 1 个事件载荷（`PlayerScreenControl` 的按钮回调带 2 个字） |
//!
//! 分界看的是**被调函数本身的元数**，不是名字，也不是类：`handle_entity_removed_event`
//! 出现在 **13 个**类型的 `call` 里（同一个方法名、同一个载荷类型，所以只有 13 个单态化里
//! 的 13 个 `T`）；而 `send_all_info_events` **两种都有** ——
//! 10 个类型上它不带载荷（走 `call_0`），`GraphwarTeamManager` 上它带一个载荷（走 `call`，
//! 见 `0x3d1d00`，那一行同时列了 `handle_entity_removed_event`）。
//! 这条反证很有力：如果 `call`/`call_0` 是「语义上选哪个口」，同一个方法名不会两边都出现。
//!
//! 所以它不是「回调派发器」（没有注册表、没有 id、没有队列、没有返回值），
//! 只是一个**把弱句柄、可变借出、一次函数调用绑在一起的 6 行工具**。
//!
//! # 版本差：x86 26 个类型，arm64 只有 24 个
//!
//! `GraphwarTargetManager` 与 `GraphwarEntityEffectManager` 在 arm64 2.0.60 里
//! **没有外联单态化**，但它们的
//! `..::subscribe_to_events_event_handler::{{closure}}` 分别是 **740 / 744 字节**
//! （其它 manager 的同类闭包只有 ~60 字节）——正是 `call` 被**内联进闭包**的尺寸。
//! 所以两边是**同一份源码**，差别只在「内联还是外联」：
//!
//! | | `call` 外联 | `call_0` 外联 | 被内联掉的 |
//! |---|---|---|---|
//! | x86 2026-08-28（目标） | 23 | 16 | `GraphwarGameRecorder`（`0x3d4fa0`，704 字节） |
//! | arm64 2.0.60 | 21 | 14 | `GraphwarTargetManager`、`GraphwarEntityEffectManager`、`GraphwarGameRecorder` |
//!
//! # 两个 `panic::Location` = 源码行号（**推断**）
//!
//! `weak_caller.rs` 全模块**只有 2 个** panic 点，都是 `borrow_mut` 的 `#[track_caller]`：
//! `call_0` 在 **11:28**、`call` 在 **17:28**（`Location` 结构体在 x86 `.data.rel.ro`
//! 的 `0x9531f8` / `0x9531e0`；arm64 在 `0x84c7c8` / `0x84c7b0`）。
//!
//! 推论（`推断`，不是观测）：两行相距 6、列号**相同** ⇒ 两个方法体形状一样、缩进一样，
//! 且 `17 = 11 + 6` 正好允许「6 行一个方法、两方法之间不留空行」，
//! 于是 `impl` 之前只有 ~8 行 ⇒ **原版这个文件几乎没有模块级文档**，很可能就是一个
//! 一行 `pub struct WeakCaller<T>(pub Weak<RefCell<T>>);` 加两个 6 行方法的小文件。
//! 列号 28 进一步要求 `borrow_mut` 那一行的**接收者表达式**长度固定
//! （缩进 12 + 6 字符接收者，或缩进 8 + 10 字符接收者）。
//! 本文件为了写清结论自带大段 `//!` 文档，**不复现这两个行号**；
//! `tests/weak_caller.rs` 里用一段最小模板把这条算术单独钉住。
//!
//! # 档位
//!
//! | 结论 | 档位 | 依据 |
//! |---|---|---|
//! | `WeakCaller<T>` = `Weak<RefCell<T>>`，8 字节，`Clone` | **观测** | 39 个单态化的头 20 条指令 + 调用点的弱计数 `+1` |
//! | `call`/`call_0` 按值消费 `self` 并在尾部放掉弱计数 | **观测** | 尾部 `ldr [x19,#8]; subs 1; b.eq dealloc`，`self` 也在 unwind 槽里 |
//! | `borrow_mut` 而不是 `borrow` | **观测** | ①失败符号 = `core::cell::panic_already_borrowed`；②标志写 `-1`、释放 `+1` |
//! | 失败静默跳过 / `()` 返回 | **观测** | 两条跳过分支都不写 `x0`，直接落到尾部 |
//! | `RcBox` = `24 + size_of::<T>()`、对齐 8 | **观测** | 39 处 `dealloc` 布局常量 |
//! | 39 / 35 个单态化的地址与归属 | **观测** | x86：两锚分组 + 反汇编形状；arm64：`.symtab` + `adr x1` 的 `f` |
//! | 26 个类型名 | **观测**（36 行）/ **推断**（3 行） | 见 §5 |
//! | `size_of::<T>()` 逐型数值 | **观测** | 两架构**逐一相等**（`tests/weak_caller.rs` 断言） |
//! | `graphwarrust::weak_caller` 是 crate 内模块 | **观测** | `.symtab` 符号路径（旧文档判第三方依赖，已推翻） |
//! | 11:28 / 17:28 是 `borrow_mut` 那一行 | **推断** | `#[track_caller]` 是 std 的实现事实；行号本身无法直接复核 |
//! | `F` 泛型而非 `fn` 指针 | **观测** | 160 字节单态化里没有间接调用 ⇒ 闭包被内联 |
//! | `WeakCaller` 的字段名 | **未知** | 只有布局与类型，没有字段名证据 |
//!
//! # 复算命令（不要凭本文件的数字，自己跑）
//!
//! ```text
//! # 1) arm64 2.0.60（未 strip）：两个方法名 + 35 个单态化
//! readelf -sW .reference/steam/android-2.0.60/libgraphwarrust.so \
//!   | grep weak_caller | grep FUNC | wc -l            # 35（21 个 call + 14 个 call_0）
//!
//! # 2) 每个单态化里内联的 dealloc 布局常量 = size_of::<RcBox<RefCell<T>>>()
//! llvm-objdump -d --start-address=0x3d519c --stop-address=0x3d52c4 <arm64 so> \
//!   | grep -E 'mov\s+w1, #0x|panic_already'
//!
//! # 3) x86 目标（stripped）：两个 Location 锚把 40 个函数分成 16 + 24
//! .venv/bin/python tools/loc_xrefs.py loc src/weak_caller.rs:11:28
//! .venv/bin/python tools/loc_xrefs.py loc src/weak_caller.rs:17:28
//!
//! # 4) 类型名：找每个 call 的调用者，看它把哪个 fn 项交给 f
//! #    arm64 是 `adr x1, <T>::method`；x86 是 `lea rsi, [rip+..]`
//! #    再把那个地址查 fixtures/name_to_addr.tsv
//! ```
//!
//! # 本文件里哪些东西**不是**原版的
//!
//! * [`MONOS`] / [`ARM64_MONOS`] / [`TYPES`] / [`ANCHORS`] / [`INLINED_CALL_SITES`]
//!   是**我们的恢复台账**，原版里没有这些常量；
//! * [`WeakCaller::new`] / [`WeakCaller::as_weak`] 是便利口（构造函数被内联进
//!   每个 manager 的 `new`，看不到符号；`as_weak` 纯粹为测试）；
//! * [`WeakCaller::strong_count`] / [`WeakCaller::ptr_is_dangling`] 同理，只为可测。

use std::cell::RefCell;
use std::rc::Weak;

// ---------------------------------------------------------------------------
// RcBox / RefCell 布局常量：39 个单态化里一处不差
// ---------------------------------------------------------------------------

/// `RcBox<RefCell<T>>` 里 `strong` 的偏移。
pub const RCB_STRONG_OFFSET: usize = 0x00;
/// `RcBox<RefCell<T>>` 里 `weak` 的偏移。
pub const RCB_WEAK_OFFSET: usize = 0x08;
/// `RcBox<RefCell<T>>` 里 `RefCell.borrow` 标志的偏移。
pub const RCB_BORROW_OFFSET: usize = 0x10;
/// `RcBox<RefCell<T>>` 里 `T` 的偏移 —— 也就是 `&mut T` 的来源（`add x0,x19,#0x18`）。
pub const RCB_VALUE_OFFSET: usize = 0x18;

/// `RefCell` 的 `WRITING` 标志值：单态化里 `mov x9, #-1; str x9, [x0,#0x10]`
/// （x86 `movq $0xffffffffffffffff, 0x10(%rbx)`），释放时 `+1` 回到 `0`。
pub const BORROW_WRITING: isize = -1;

/// `Weak::new()` 的哨兵：`cmn x0, #1` / `cmp $0xffffffffffffffff, %rdi`。
pub const DANGLING_WEAK: usize = usize::MAX;

/// 39 处内联 `__rust_dealloc` 的**对齐**常量 —— 无一例外是 8。
pub const DEALLOC_ALIGN: usize = 8;

/// `RcBox<RefCell<T>>` 的分配尺寸 = `24 + size_of::<T>()`
/// （`24 = strong 8 + weak 8 + 借用标志 8`；`RefCell<T>` 自己不再补齐，
/// 因为 26 个类型的对齐都 ≤ 8）。
pub const RCB_PREFIX: usize = RCB_VALUE_OFFSET;

// ---------------------------------------------------------------------------
// 原版类型
// ---------------------------------------------------------------------------

/// 原版 `graphwarrust::weak_caller::WeakCaller<T>`。
///
/// 唯一字段是一个 `Weak<RefCell<T>>`（8 字节）。**`self` 按值消费**：每次 `call`
/// 结束都会把弱计数减一，所以调用方必须先 `clone()`（这正是每个事件闭包做的事）。
///
/// 字段名是 `未知`（原版只有布局证据）；这里叫 `weak` 只是为了实现可读。
pub struct WeakCaller<T> {
    weak: Weak<RefCell<T>>,
}

impl<T> Default for WeakCaller<T> {
    /// 空句柄 = `Weak::default()`（指针是 [`DANGLING_WEAK`]），`call`/`call_0` 一律静默跳过。
    /// 手写而不是 `derive`，理由同 `Clone`。
    fn default() -> Self {
        Self {
            weak: Weak::default(),
        }
    }
}

impl<T> Clone for WeakCaller<T> {
    /// 观测：调用点在把句柄交出去之前执行 `ldr x8,[x0,#8]; adds 1; str`
    /// （x86 `mov 0x8(%rbx),%rax; inc; mov %rax,0x8(%rbx)`），即 `Weak::clone`。
    /// 手写而不是 `derive`，是为了不引入 `T: Clone` 这个多余的界。
    fn clone(&self) -> Self {
        Self {
            weak: self.weak.clone(),
        }
    }
}

impl<T> WeakCaller<T> {
    /// 便利口（**不是**观测到的 API）：原版的构造被内联进各 manager 的 `new`，
    /// 看不到符号。语义上只能这样写 —— 一个 `WeakCaller` 就是包起来的 `Weak`。
    pub fn new(weak: Weak<RefCell<T>>) -> Self {
        Self { weak }
    }

    /// 便利口：取回内部句柄。原版是否导出过它 `未知`。
    pub fn as_weak(&self) -> &Weak<RefCell<T>> {
        &self.weak
    }

    /// 便利口：`Weak::strong_count` 的转发，只为测试能观测「值还在不在」。
    pub fn strong_count(&self) -> usize {
        self.weak.strong_count()
    }

    /// 便利口：内部指针是否是 `Weak::new()` 的哨兵。
    pub fn ptr_is_dangling(&self) -> bool {
        Weak::as_ptr(&self.weak).cast::<()>() as usize == DANGLING_WEAK
    }

    /// 无载荷版本：`Weak::upgrade()` 成功就 `f(&mut T)`，否则**静默跳过**。
    ///
    /// panic 点：`borrow_mut()` —— 原版锚在 `src/weak_caller.rs:11:28`（**推断**）。
    pub fn call_0<F: FnOnce(&mut T)>(self, f: F) {
        if let Some(rc) = self.weak.upgrade() {
            let mut target = rc.borrow_mut();
            f(&mut target);
        }
    }

    /// 带一个载荷：`Weak::upgrade()` 成功就 `f(&mut T, a)`，否则 `a` 与 `f` **一起被丢掉**。
    ///
    /// panic 点：`borrow_mut()` —— 原版锚在 `src/weak_caller.rs:17:28`（**推断**）。
    pub fn call<A, F: FnOnce(&mut T, A)>(self, f: F, a: A) {
        if let Some(rc) = self.weak.upgrade() {
            let mut target = rc.borrow_mut();
            f(&mut target, a);
        }
    }
}

// ---------------------------------------------------------------------------
// 恢复台账：两个泛型方法的单态化矩阵
// ---------------------------------------------------------------------------

/// 两个泛型方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `WeakCaller::<T>::call(&self, f, a)` —— 带一个载荷 `A`。
    Call,
    /// `WeakCaller::<T>::call_0(&self, f)` —— 无载荷。
    Call0,
}

/// 一个**外联**单态化（x86 目标 2026-08-28）。
#[derive(Debug, Clone, Copy)]
pub struct Mono {
    /// ELF 虚地址（Ghidra / REA 地址 = 本值 + `0x100000`）。
    pub addr: usize,
    /// `.eh_frame` FDE 长度。
    pub len: usize,
    /// 属于哪个方法。
    pub method: Method,
    /// `T` 的类型名（来自调用点交出的 `f`）。
    pub ty: &'static str,
    /// `size_of::<T>()`，由内联 `dealloc` 布局常量 − 24 得到。
    pub size_of_t: usize,
    /// 载荷 `A` 占几个整数寄存器（`None` = 该单态化把闭包内联了，看不到间接调用）。
    pub payload_words: Option<u8>,
    /// 调用点实际交出去的 `f`（`T::method`）——「谁在用它」的签名级证据。
    pub f: &'static [&'static str],
}

/// arm64 2.0.60（未 strip）的落点，用符号哈希对齐同一个 `T`。
#[derive(Debug, Clone, Copy)]
pub struct Arm64Mono {
    /// ELF 虚地址。
    pub addr: usize,
    /// 函数长度（`st_size`）。
    pub len: usize,
    /// 属于哪个方法。
    pub method: Method,
    /// `T` 的类型名。
    pub ty: &'static str,
    /// `RcBox<RefCell<T>>` 的分配尺寸（= `24 + size_of::<T>()` = 内联 `dealloc` 的 size 常量）。
    pub rcb_size: usize,
    /// legacy mangling 的 `h<16 hex>` 后缀（同一泛型实参 = 同一哈希；链接器 ICF 会让**多个**
    /// 哈希落在同一地址，见 `docs/02-module-map.md` §34）。
    pub sym_hash: &'static str,
    /// 内联进来的 `Rc::<RefCell<T>>::drop_slow` 实例哈希。
    pub drop_slow: &'static str,
}

/// 一个 `T` 在两个架构上的出现情况。
#[derive(Debug, Clone, Copy)]
pub struct Ty {
    /// 类型名。
    pub name: &'static str,
    /// `size_of::<T>()`。
    pub size_of_t: usize,
    /// `(有 call, 有 call_0)` —— x86 目标。
    pub x86: (bool, bool),
    /// `(有 call, 有 call_0)` —— arm64 2.0.60。
    pub arm64: (bool, bool),
}

/// 两个 `panic::Location` 锚 = 两个 `borrow_mut` 的源码行号（**推断**）。
pub const ANCHORS: [(Method, &str, u32, u32); 2] = [
    (Method::Call0, "src/weak_caller.rs", 11, 28),
    (Method::Call, "src/weak_caller.rs", 17, 28),
];

/// 内联站点：`call` 被**整个抄进调用者**、因而没有外联单态化的地方。
///
/// 这正是 `fixtures/module_map.tsv` 把 `weak_caller.rs` 记成 **40** 而进度尺记成
/// **39** 的原因 —— 那个多出来的 `0x3d4fa0` 是 `GraphwarGameRecorder` 自己的方法，
/// 只因内联了 `call` 才引用到 `src/weak_caller.rs:17:28`。
/// （`docs/02-module-map.md` §20 与 §21 里按「锚」数出来的 40 要按本条读。）
pub const INLINED_CALL_SITES: [(usize, usize, &str, &str); 3] = [
    (
        0x3d4fa0,
        704,
        "x86",
        "GraphwarGameRecorder 的某个方法（自身 panic 锚 src/graphwar_game_recorder.rs:107:105 / 108:41）",
    ),
    (
        0x3da9bc,
        740,
        "arm64",
        "GraphwarTargetManager::subscribe_to_events_event_handler::{{closure}}",
    ),
    (
        0x3e9c58,
        744,
        "arm64",
        "GraphwarEntityEffectManager::subscribe_to_events_event_handler::{{closure}}",
    ),
];

/// 39 个外联单态化（x86 目标 2026-08-28，stripped）。
pub const MONOS: [Mono; 39] = [
    Mono { addr: 0x3d10e0, len: 240, method: Method::Call, ty: "GraphwarPlayerScreenControl", size_of_t: 32, payload_words: Some(2), f: &["GraphwarPlayerScreenControl::face_button_pressed", "GraphwarPlayerScreenControl::hat_button_pressed", "GraphwarPlayerScreenControl::skin_button_pressed"] },
    Mono { addr: 0x3d11d0, len: 240, method: Method::Call, ty: "GraphwarTargetManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarTargetManager::subscribe_to_events_event_handler"] },
    Mono { addr: 0x3d12c0, len: 240, method: Method::Call, ty: "GraphwarColorManager", size_of_t: 120, payload_words: Some(1), f: &["GraphwarColorManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d13b0, len: 224, method: Method::Call, ty: "GraphwarTickHandler", size_of_t: 72, payload_words: Some(1), f: &["GraphwarTickHandler::handle_connected_to_server_event"] },
    Mono { addr: 0x3d1490, len: 240, method: Method::Call, ty: "GraphwarLocalPlayerManager", size_of_t: 64, payload_words: Some(1), f: &["GraphwarLocalPlayerManager::handle_connected_to_server", "GraphwarLocalPlayerManager::handle_entity_removed_event", "GraphwarLocalPlayerManager::handle_player_info"] },
    Mono { addr: 0x3d1580, len: 240, method: Method::Call, ty: "GraphwarNameManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarNameManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d1670, len: 240, method: Method::Call, ty: "GraphwarClientConnectionHandler", size_of_t: 112, payload_words: Some(1), f: &["GraphwarClientConnectionHandler::forward_event", "GraphwarClientConnectionHandler::handle_connected_to_server"] },
    Mono { addr: 0x3d1760, len: 240, method: Method::Call, ty: "GraphwarPlanePosManager", size_of_t: 24, payload_words: Some(1), f: &["GraphwarPlanePosManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d1850, len: 240, method: Method::Call, ty: "GraphwarComputerPlayerManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarComputerPlayerManager::handle_entity_removed_event", "GraphwarComputerPlayerManager::handle_tick", "GraphwarComputerPlayerManager::handle_turn_info_event"] },
    Mono { addr: 0x3d1940, len: 240, method: Method::Call, ty: "GraphwarLifeManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarLifeManager::handle_entity_removed_event", "GraphwarLifeManager::handle_tick_event"] },
    Mono { addr: 0x3d1a30, len: 240, method: Method::Call, ty: "GraphwarGameStateManager", size_of_t: 296, payload_words: Some(1), f: &["GraphwarGameStateManager::handle_chat_message", "GraphwarGameStateManager::handle_connection_removed_event", "GraphwarGameStateManager::handle_entity_removed_event", "GraphwarGameStateManager::handle_function_points", "GraphwarGameStateManager::handle_graphics_info", "GraphwarGameStateManager::handle_new_connection_event"] },
    Mono { addr: 0x3d1b20, len: 240, method: Method::Call, ty: "GraphwarRankManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarRankManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d1c10, len: 240, method: Method::Call, ty: "GraphwarTurnManager", size_of_t: 32, payload_words: Some(1), f: &["GraphwarTurnManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d1d00, len: 240, method: Method::Call, ty: "GraphwarTeamManager", size_of_t: 32, payload_words: Some(1), f: &["GraphwarTeamManager::handle_entity_removed_event", "GraphwarTeamManager::send_all_info_events"] },
    Mono { addr: 0x3d1df0, len: 240, method: Method::Call, ty: "GraphwarAudioControl", size_of_t: 216, payload_words: Some(1), f: &["GraphwarAudioControl::effect_info_event", "GraphwarAudioControl::game_state_event"] },
    Mono { addr: 0x3d1ee0, len: 240, method: Method::Call, ty: "GraphwarSoldierManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarSoldierManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d1fd0, len: 240, method: Method::Call, ty: "GraphwarGraphicsManager", size_of_t: 168, payload_words: Some(1), f: &["GraphwarGraphicsManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d20c0, len: 240, method: Method::Call, ty: "GraphwarObstacleManager", size_of_t: 80, payload_words: Some(1), f: &["GraphwarObstacleManager::handle_tick_event"] },
    Mono { addr: 0x3d21b0, len: 240, method: Method::Call, ty: "GraphwarServerConnectionHandler", size_of_t: 128, payload_words: Some(1), f: &["GraphwarServerConnectionHandler::forward_connection_specific_event", "GraphwarServerConnectionHandler::forward_event", "GraphwarServerConnectionHandler::handle_ban_connection_request_event", "GraphwarServerConnectionHandler::handle_clear_bans_request_event", "GraphwarServerConnectionHandler::handle_connection_removed_event"] },
    Mono { addr: 0x3d22a0, len: 160, method: Method::Call, ty: "GraphwarPlayScreenControl", size_of_t: 144, payload_words: None, f: &[] },
    Mono { addr: 0x3d2340, len: 240, method: Method::Call, ty: "GraphwarEntityEffectManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarEntityEffectManager::subscribe_to_events_event_handler"] },
    Mono { addr: 0x3d2430, len: 240, method: Method::Call, ty: "GraphwarPlayerManager", size_of_t: 72, payload_words: Some(1), f: &["GraphwarPlayerManager::handle_entity_removed_event"] },
    Mono { addr: 0x3d2520, len: 240, method: Method::Call, ty: "FunctionHistoryRecorder", size_of_t: 48, payload_words: Some(1), f: &["FunctionHistoryRecorder::handle_function_fire_event"] },
    Mono { addr: 0x3d2610, len: 224, method: Method::Call0, ty: "GraphwarLifeManager", size_of_t: 72, payload_words: None, f: &["GraphwarLifeManager::send_all_info_events"] },
    Mono { addr: 0x3d26f0, len: 224, method: Method::Call0, ty: "GraphwarPlayerManager", size_of_t: 72, payload_words: None, f: &["GraphwarPlayerManager::send_all_info_events"] },
    Mono { addr: 0x3d27d0, len: 224, method: Method::Call0, ty: "GraphwarRankManager", size_of_t: 72, payload_words: None, f: &["GraphwarRankManager::send_all_info_events"] },
    Mono { addr: 0x3d28b0, len: 224, method: Method::Call0, ty: "GraphwarGraphicsManager", size_of_t: 168, payload_words: None, f: &["GraphwarGraphicsManager::send_all_info_events"] },
    Mono { addr: 0x3d2990, len: 224, method: Method::Call0, ty: "GraphwarModeManager", size_of_t: 32, payload_words: None, f: &["GraphwarModeManager::send_all_info_events"] },
    Mono { addr: 0x3d2a70, len: 224, method: Method::Call0, ty: "GraphwarServerConnectionHandler", size_of_t: 128, payload_words: None, f: &["GraphwarServerConnectionHandler::tick"] },
    Mono { addr: 0x3d2b50, len: 224, method: Method::Call0, ty: "GraphwarLobbyControl", size_of_t: 96, payload_words: None, f: &[] },
    Mono { addr: 0x3d2c30, len: 224, method: Method::Call0, ty: "GraphwarColorManager", size_of_t: 120, payload_words: None, f: &["GraphwarColorManager::send_all_info_events"] },
    Mono { addr: 0x3d2d10, len: 224, method: Method::Call0, ty: "GraphwarNameManager", size_of_t: 72, payload_words: None, f: &["GraphwarNameManager::send_all_info_events"] },
    Mono { addr: 0x3d2df0, len: 224, method: Method::Call0, ty: "GraphwarEntityEffectManager", size_of_t: 72, payload_words: None, f: &["GraphwarEntityEffectManager::subscribe_to_events_event_handler"] },
    Mono { addr: 0x3d2ed0, len: 224, method: Method::Call0, ty: "GraphwarTurnManager", size_of_t: 32, payload_words: None, f: &["GraphwarTurnManager::send_all_info_events"] },
    Mono { addr: 0x3d2fb0, len: 224, method: Method::Call0, ty: "GraphwarEffectManager", size_of_t: 72, payload_words: None, f: &["GraphwarEffectManager::tick"] },
    Mono { addr: 0x3d3090, len: 224, method: Method::Call0, ty: "GraphwarPlanePosManager", size_of_t: 24, payload_words: None, f: &["GraphwarPlanePosManager::send_all_info_events"] },
    Mono { addr: 0x3d3170, len: 224, method: Method::Call0, ty: "GraphwarSoldierManager", size_of_t: 72, payload_words: None, f: &["GraphwarSoldierManager::send_all_info_events"] },
    Mono { addr: 0x3d3250, len: 224, method: Method::Call0, ty: "GraphwarClientConnectionHandler", size_of_t: 112, payload_words: None, f: &["GraphwarClientConnectionHandler::tick"] },
    Mono { addr: 0x3d3330, len: 224, method: Method::Call0, ty: "GraphwarTargetManager", size_of_t: 72, payload_words: None, f: &["GraphwarTargetManager::subscribe_to_events_event_handler"] },
];

/// 35 个外联单态化（arm64 2.0.60，未 strip）。
pub const ARM64_MONOS: [Arm64Mono; 35] = [
    Arm64Mono { addr: 0x3d519c, len: 296, method: Method::Call, ty: "GraphwarPlanePosManager", rcb_size: 48, sym_hash: "00e436b2988aa846", drop_slow: "01e7d713d29a0fe1" },
    Arm64Mono { addr: 0x3d52c4, len: 296, method: Method::Call, ty: "GraphwarPlayerManager", rcb_size: 96, sym_hash: "0b1b6597a01f8b72", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d53ec, len: 296, method: Method::Call, ty: "GraphwarClientConnectionHandler", rcb_size: 136, sym_hash: "120a738c27fd00b8", drop_slow: "32babd08acb4c7c9" },
    Arm64Mono { addr: 0x3d5514, len: 296, method: Method::Call, ty: "GraphwarRankManager", rcb_size: 96, sym_hash: "282f772c6deba39c", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d563c, len: 296, method: Method::Call, ty: "GraphwarLocalPlayerManager", rcb_size: 88, sym_hash: "2a3210aab08a2aaa", drop_slow: "52fb93de21a12888" },
    Arm64Mono { addr: 0x3d5764, len: 296, method: Method::Call, ty: "GraphwarTurnManager", rcb_size: 56, sym_hash: "3254d08c48305ae6", drop_slow: "2889975f7ec3b8ab" },
    Arm64Mono { addr: 0x3d588c, len: 196, method: Method::Call, ty: "GraphwarPlayScreenControl", rcb_size: 168, sym_hash: "3bfa43109446b39a", drop_slow: "30478d50b63ac2ff" },
    Arm64Mono { addr: 0x3d5950, len: 296, method: Method::Call, ty: "GraphwarGraphicsManager", rcb_size: 192, sym_hash: "5330184ec8123d95", drop_slow: "843f69af095548ba" },
    Arm64Mono { addr: 0x3d5a78, len: 296, method: Method::Call, ty: "GraphwarSoldierManager", rcb_size: 96, sym_hash: "5e77054d42116aed", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d5ba0, len: 296, method: Method::Call, ty: "GraphwarAudioControl", rcb_size: 240, sym_hash: "61a362a3df84f831", drop_slow: "ddf440e452177e9a" },
    Arm64Mono { addr: 0x3d5cc8, len: 296, method: Method::Call, ty: "GraphwarObstacleManager", rcb_size: 104, sym_hash: "62ce47fb4a7007f4", drop_slow: "842337f993eb9c61" },
    Arm64Mono { addr: 0x3d5df0, len: 296, method: Method::Call, ty: "GraphwarGameStateManager", rcb_size: 320, sym_hash: "76a37f0675e9dac3", drop_slow: "919984934fa7c29c" },
    Arm64Mono { addr: 0x3d5f18, len: 296, method: Method::Call, ty: "GraphwarLifeManager", rcb_size: 96, sym_hash: "85c2bad31525142a", drop_slow: "847280a99c1c2479" },
    Arm64Mono { addr: 0x3d6040, len: 296, method: Method::Call, ty: "GraphwarServerConnectionHandler", rcb_size: 152, sym_hash: "8a7689704334bfe7", drop_slow: "0dafa793e65eeaec" },
    Arm64Mono { addr: 0x3d6168, len: 296, method: Method::Call, ty: "GraphwarTeamManager", rcb_size: 56, sym_hash: "909fcfdcf944cd74", drop_slow: "a0dcc915dbb095ff" },
    Arm64Mono { addr: 0x3d6290, len: 288, method: Method::Call, ty: "GraphwarTickHandler", rcb_size: 96, sym_hash: "998b7fb5eda26fc8", drop_slow: "d1046d7bfe4a3f60" },
    Arm64Mono { addr: 0x3d63b0, len: 296, method: Method::Call, ty: "GraphwarColorManager", rcb_size: 144, sym_hash: "b73415c3b3ac3398", drop_slow: "6e8aa413316f6a57" },
    Arm64Mono { addr: 0x3d64d8, len: 296, method: Method::Call, ty: "GraphwarNameManager", rcb_size: 96, sym_hash: "b83253edf58d89ff", drop_slow: "85479827b2c500ef" },
    Arm64Mono { addr: 0x3d6600, len: 296, method: Method::Call, ty: "GraphwarComputerPlayerManager", rcb_size: 96, sym_hash: "cefa3b7c7bd4d534", drop_slow: "3971363ef8d504a4" },
    Arm64Mono { addr: 0x3d6728, len: 300, method: Method::Call, ty: "GraphwarPlayerScreenControl", rcb_size: 56, sym_hash: "e1a277a20a054ef4", drop_slow: "d3f5d1b294e5ffb4" },
    Arm64Mono { addr: 0x3d6854, len: 296, method: Method::Call, ty: "FunctionHistoryRecorder", rcb_size: 72, sym_hash: "e8cddffb03ebe5dd", drop_slow: "9a24933500f76c29" },
    Arm64Mono { addr: 0x3d697c, len: 288, method: Method::Call0, ty: "GraphwarEffectManager", rcb_size: 96, sym_hash: "14a774dc55097e4a", drop_slow: "54306ce41595f772" },
    Arm64Mono { addr: 0x3d6a9c, len: 288, method: Method::Call0, ty: "GraphwarServerConnectionHandler", rcb_size: 152, sym_hash: "564f4745d2e76c1c", drop_slow: "0dafa793e65eeaec" },
    Arm64Mono { addr: 0x3d6bbc, len: 288, method: Method::Call0, ty: "GraphwarGraphicsManager", rcb_size: 192, sym_hash: "71387b189f98a59b", drop_slow: "843f69af095548ba" },
    Arm64Mono { addr: 0x3d6cdc, len: 288, method: Method::Call0, ty: "GraphwarLobbyControl", rcb_size: 120, sym_hash: "7c9d55e7bedf2e76", drop_slow: "50474f20e6524cf1" },
    Arm64Mono { addr: 0x3d6dfc, len: 288, method: Method::Call0, ty: "GraphwarClientConnectionHandler", rcb_size: 136, sym_hash: "9b79d391f827ae5b", drop_slow: "32babd08acb4c7c9" },
    Arm64Mono { addr: 0x3d6f1c, len: 288, method: Method::Call0, ty: "GraphwarRankManager", rcb_size: 96, sym_hash: "9dc227088edcdcb3", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d703c, len: 288, method: Method::Call0, ty: "GraphwarPlanePosManager", rcb_size: 48, sym_hash: "a9b1a2aad86129df", drop_slow: "01e7d713d29a0fe1" },
    Arm64Mono { addr: 0x3d715c, len: 288, method: Method::Call0, ty: "GraphwarSoldierManager", rcb_size: 96, sym_hash: "c18637a6cfd79a81", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d727c, len: 288, method: Method::Call0, ty: "GraphwarPlayerManager", rcb_size: 96, sym_hash: "c2424e0c4ad615e1", drop_slow: "224f049bc5e37134" },
    Arm64Mono { addr: 0x3d739c, len: 288, method: Method::Call0, ty: "GraphwarTurnManager", rcb_size: 56, sym_hash: "d22e599d4eceece3", drop_slow: "2889975f7ec3b8ab" },
    Arm64Mono { addr: 0x3d74bc, len: 288, method: Method::Call0, ty: "GraphwarNameManager", rcb_size: 96, sym_hash: "d85bd4dfcbc46294", drop_slow: "85479827b2c500ef" },
    Arm64Mono { addr: 0x3d75dc, len: 288, method: Method::Call0, ty: "GraphwarLifeManager", rcb_size: 96, sym_hash: "dfb23eb897cbef1c", drop_slow: "847280a99c1c2479" },
    Arm64Mono { addr: 0x3d76fc, len: 288, method: Method::Call0, ty: "GraphwarModeManager", rcb_size: 56, sym_hash: "e323d0311ec314fe", drop_slow: "506992fcac9b06a7" },
    Arm64Mono { addr: 0x3d781c, len: 288, method: Method::Call0, ty: "GraphwarColorManager", rcb_size: 144, sym_hash: "e4ea168028b0113e", drop_slow: "6e8aa413316f6a57" },
];

/// 26 个类型。
/// `Method` × `T` 的实例化矩阵（26 个类型）。
pub const TYPES: [Ty; 26] = [
    Ty { name: "FunctionHistoryRecorder", size_of_t: 48, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarAudioControl", size_of_t: 216, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarClientConnectionHandler", size_of_t: 112, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarColorManager", size_of_t: 120, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarComputerPlayerManager", size_of_t: 72, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarEffectManager", size_of_t: 72, x86: (false, true), arm64: (false, true) },
    Ty { name: "GraphwarEntityEffectManager", size_of_t: 72, x86: (true, true), arm64: (false, false) },
    Ty { name: "GraphwarGameStateManager", size_of_t: 296, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarGraphicsManager", size_of_t: 168, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarLifeManager", size_of_t: 72, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarLobbyControl", size_of_t: 96, x86: (false, true), arm64: (false, true) },
    Ty { name: "GraphwarLocalPlayerManager", size_of_t: 64, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarModeManager", size_of_t: 32, x86: (false, true), arm64: (false, true) },
    Ty { name: "GraphwarNameManager", size_of_t: 72, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarObstacleManager", size_of_t: 80, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarPlanePosManager", size_of_t: 24, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarPlayScreenControl", size_of_t: 144, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarPlayerManager", size_of_t: 72, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarPlayerScreenControl", size_of_t: 32, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarRankManager", size_of_t: 72, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarServerConnectionHandler", size_of_t: 128, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarSoldierManager", size_of_t: 72, x86: (true, true), arm64: (true, true) },
    Ty { name: "GraphwarTargetManager", size_of_t: 72, x86: (true, true), arm64: (false, false) },
    Ty { name: "GraphwarTeamManager", size_of_t: 32, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarTickHandler", size_of_t: 72, x86: (true, false), arm64: (true, false) },
    Ty { name: "GraphwarTurnManager", size_of_t: 32, x86: (true, true), arm64: (true, true) },
];
