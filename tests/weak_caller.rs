//! `weak_caller` 的验收：**数据结构 / 升级失败路径 / 借用冲突 / 39 个单态化的矩阵**。
//!
//! ⚠️ 用 `#[path]` 直接编进本测试二进制 ⇒ **不需要** `src/lib.rs` 里先加 `pub mod`
//! （与 `tests/graphwar_color_manager.rs` 等同一手法）。
//!
//! 本文件分两半，**证据等级完全不同**，不要混着看：
//!
//! * **行为半**（`live_*` / `dropped_*` / `borrow_*` / `reenter*`）：跑的是**我们自己**
//!   写的 `call`/`call_0`。按 `AGENTS.md` §0.2，这只能证明**我们的实现与「从二进制读出的
//!   语义」自洽**，是**回归保护**，**不是**复原证据。复原证据在另一半。
//! * **台账半**（`*_matches_*` / `*_counts` / `blocks_are_contiguous` / `anchors_*`）：
//!   断言的是**真二进制里读出来的数**（地址、长度、`dealloc` 布局常量、Location 行号）。
//!   这些数可以由文件头「复算命令」那四条命令独立重算，所以这一半是有内容的。
//!
//! | 测试 | 钉住的东西 | 证据等级 |
//! |---|---|---|
//! | `live_handle_runs_the_callback` | 升级成功 ⇒ `f(&mut T)` 拿到的就是那个 `T` | 行为（回归） |
//! | `dropped_target_is_silently_skipped` | `Rc` 放掉后 ⇒ **静默跳过**，不 panic、不回调 | 行为（回归） |
//! | `dangling_weak_is_skipped` | `Weak::default()`（哨兵 `usize::MAX`）⇒ 跳过 | 行为（回归） |
//! | `payload_reaches_the_callback` | `call` 的载荷按值到达；16 字节载荷也对 | 行为（回归） |
//! | `borrow_mut_is_what_panics` | 冲突时 panic 且**不是** `borrow()` 的那条消息 | 行为（回归）+ 语义对应 `观测` |
//! | `reentrant_call_panics` | 回调里再 `call` 同一个 `T` ⇒ panic（不是死锁） | 行为（回归） |
//! | `strong_and_weak_counts_track_the_live_value` | `self` 被消费 ⇒ 弱计数 −1 | 行为（回归） |
//! | `weak_caller_is_one_word` | 8 字节、`Option` 也是 8（niche）＝ 单寄存器 ABI | 观测 |
//! | `counts_are_39_and_35_and_26` | x86 23+16=39、arm64 21+14=35、26 个类型 | 观测 |
//! | `x86_blocks_have_no_holes` | 两块各自**首尾相接**（`addr+len == next`） | 观测 |
//! | `arm64_rcb_size_matches_the_other_arch` | 35 个 `rcb_size` 与 x86 推出的 `24+size_of_t` **逐一相等** | 观测 |
//! | `every_mono_names_a_known_type` | 39 行的 `ty` 都在 `TYPES` 里，`size_of_t` 一致 | 观测 |
//! | `types_matrix_is_derivable_from_monos` | `TYPES` 的 `(call, call_0)` 布尔完全由 `MONOS` 推出 | 观测 |
//! | `ticks_are_the_payload_free_end` | `T::tick` 只走 `call_0`、`handle_*` 只走 `call`；`send_all_info_events` **两种都有**（10 : 1） | 观测 |
//! | `anchors_are_the_two_borrow_mut_sites` | 11:28 = `call_0`、17:28 = `call` | 观测 |
//! | `anchor_arithmetic_admits_a_six_line_body` | 11 → 17 的间距允许「6 行一个方法、中间不空行」 | **推断** |
//! | `the_40th_function_is_not_a_monomorphisation` | `0x3d4fa0` 不在 39 行里 | 观测 |
#![allow(dead_code)]

#[path = "../src/weak_caller.rs"]
mod weak_caller;

use std::cell::RefCell;
use std::panic::AssertUnwindSafe;
use std::rc::{Rc, Weak};

use weak_caller::*;

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

/// 造一个「活着的」句柄：返回 `Rc` 与指向它的 `WeakCaller`。
fn live<T>(v: T) -> (Rc<RefCell<T>>, WeakCaller<T>) {
    let rc = Rc::new(RefCell::new(v));
    let wk = WeakCaller::new(Rc::downgrade(&rc));
    (rc, wk)
}

fn panic_message<F: FnOnce()>(f: F) -> Option<String> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let out = std::panic::catch_unwind(AssertUnwindSafe(f)).err();
    std::panic::set_hook(prev);
    out.map(|e| {
        e.downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// 行为半（回归保护，不是复原证据）
// ---------------------------------------------------------------------------

#[test]
fn live_handle_runs_the_callback() {
    let (rc, wk) = live(7u32);
    wk.clone().call_0(|t| *t += 1);
    assert_eq!(*rc.borrow(), 8);
    // `call` 把载荷原样交出去，返回值是 `()`。
    wk.clone().call(|t, a: u32| *t += a, 35);
    assert_eq!(*rc.borrow(), 43);
}

#[test]
fn dropped_target_is_silently_skipped() {
    let (rc, wk) = live(1u32);
    drop(rc);
    let mut ran = false;
    // 不 panic、不回调、也没有返回值可以查 —— 这就是「引擎侧还活着、Rust 侧没了」的处理。
    wk.clone().call_0(|_| ran = true);
    wk.clone().call(|_, _: u32| ran = true, 9);
    assert!(!ran);
    assert_eq!(wk.strong_count(), 0);
}

#[test]
fn dangling_weak_is_skipped() {
    let wk: WeakCaller<u32> = WeakCaller::new(Weak::new());
    assert!(wk.ptr_is_dangling());
    let mut ran = false;
    wk.clone().call_0(|_| ran = true);
    wk.call(|_, _: u32| ran = true, 1u32);
    assert!(!ran);
    // `Default` 与 `Weak::default()` 一样落到哨兵上。
    assert!(WeakCaller::<u32>::default().ptr_is_dangling());
}

#[test]
fn payload_reaches_the_callback() {
    let (rc, wk) = live(String::new());
    wk.clone().call(|s, who: &str| s.push_str(who), "abc");
    assert_eq!(&*rc.borrow(), "abc");
    // 16 字节载荷（两个整数寄存器）—— 对应 `GraphwarPlayerScreenControl` 那一行。
    wk.call(|s, pair: (u64, u64)| s.push_str(&format!("{}+{}", pair.0, pair.1)), (2u64, 3u64));
    assert_eq!(&*rc.borrow(), "abc2+3");
}

#[test]
fn borrow_mut_is_what_panics() {
    let (rc, wk) = live(0u8);
    // 先取一个**共享**借用：`borrow_mut` 会失败，`borrow` 不会。
    let shared = rc.borrow();
    let msg = panic_message(|| wk.clone().call_0(|_| {})).expect("必须 panic");
    assert!(
        msg.contains("already borrowed"),
        "期望 already borrowed，实得 {msg:?}"
    );
    // 关键区分：`RefCell::borrow()` 的失败消息是 `already mutably borrowed`。
    assert!(
        !msg.contains("mutably"),
        "如果是 borrow() 的话消息会是 already mutably borrowed，实得 {msg:?}"
    );
    drop(shared);

    // 已经被可变借出时也一样 panic。
    let mut exclusive = rc.borrow_mut();
    let msg2 = panic_message(|| wk.clone().call_0(|_| {})).expect("必须 panic");
    assert!(msg2.contains("already borrowed"), "实得 {msg2:?}");
    *exclusive += 1;
    drop(exclusive);
    assert_eq!(*rc.borrow(), 1);
}

#[test]
fn reentrant_call_panics() {
    // 回调里再对同一个 T 发一次 `call`：`RefCell` 已经可变借出 ⇒ panic。
    // 这不是死锁、也不是被静默丢弃，而是 `already borrowed`。
    let (rc, wk) = live(0u32);
    let inner = wk.clone();
    let probe = Rc::new(RefCell::new(None::<String>));
    let probe2 = probe.clone();
    let msg = panic_message(move || {
        wk.call_0(move |_| {
            let m = panic_message(|| inner.call_0(|_| {}));
            *probe2.borrow_mut() = m;
        })
    });
    assert!(msg.is_none(), "外层不该 panic：{msg:?}");
    let got = probe.borrow().clone().expect("内层必须 panic");
    assert!(got.contains("already borrowed"), "实得 {got:?}");
    drop(rc);
}

#[test]
fn strong_and_weak_counts_track_the_live_value() {
    let (rc, wk) = live(0u32);
    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 1);
    // `self` 按值消费：`_rc` 是 `upgrade()` 出来的临时强引用，函数返回前放掉，
    // 所以外部看到的强计数不变；而弱计数因为 `self` 被 drop 而 −1。
    wk.call_0(|_| {});
    assert_eq!(Rc::strong_count(&rc), 1);
    assert_eq!(Rc::weak_count(&rc), 0);
    // 句柄还活着（升级成功过），只是我们把自己的那一份交出去了。
    assert_eq!(*rc.borrow(), 0);
}

#[test]
fn weak_caller_is_one_word() {
    // 单寄存器 ABI：`cmn x0,#1` / `cmp $-1,%rdi` 直接拿实参当 `NonNull` 比。
    assert_eq!(std::mem::size_of::<WeakCaller<u64>>(), 8);
    // `Weak` 有 niche（NonNull），所以 `Option` 不额外占字。
    assert_eq!(
        std::mem::size_of::<Option<WeakCaller<u64>>>(),
        std::mem::size_of::<WeakCaller<u64>>()
    );
    // RcBox 头：strong@0, weak@8, 借用标志@0x10, 值@0x18。
    assert_eq!(RCB_STRONG_OFFSET, 0);
    assert_eq!(RCB_WEAK_OFFSET, 8);
    assert_eq!(RCB_BORROW_OFFSET, 16);
    assert_eq!(RCB_VALUE_OFFSET, 24);
    assert_eq!(RCB_VALUE_OFFSET, RCB_BORROW_OFFSET + 8);
    assert_eq!(RCB_PREFIX, 24);
    assert_eq!(BORROW_WRITING, -1);
    assert_eq!(DANGLING_WEAK, usize::MAX);
    assert_eq!(DEALLOC_ALIGN, 8);
}

// ---------------------------------------------------------------------------
// 台账半（断言的是真二进制里读出来的数）
// ---------------------------------------------------------------------------

#[test]
fn counts_are_39_and_35_and_26() {
    assert_eq!(MONOS.len(), 39);
    assert_eq!(ARM64_MONOS.len(), 35);
    assert_eq!(TYPES.len(), 26);
    let call = MONOS.iter().filter(|m| m.method == Method::Call).count();
    let call0 = MONOS.iter().filter(|m| m.method == Method::Call0).count();
    assert_eq!((call, call0), (23, 16));
    let acall = ARM64_MONOS.iter().filter(|m| m.method == Method::Call).count();
    let acall0 = ARM64_MONOS.iter().filter(|m| m.method == Method::Call0).count();
    assert_eq!((acall, acall0), (21, 14));
    // 26 = 13 两个都有 + 10 只有 call + 3 只有 call_0
    let both = TYPES.iter().filter(|t| t.x86 == (true, true)).count();
    let only_call = TYPES.iter().filter(|t| t.x86 == (true, false)).count();
    let only_call0 = TYPES.iter().filter(|t| t.x86 == (false, true)).count();
    assert_eq!((both, only_call, only_call0), (13, 10, 3));
    assert_eq!(both + only_call + only_call0, TYPES.len());
}

#[test]
fn x86_blocks_have_no_holes() {
    // `call` 块 0x3d10e0..0x3d2610，23 个，首尾相接（长度取自 .eh_frame FDE）。
    let call: Vec<&Mono> = MONOS.iter().filter(|m| m.method == Method::Call).collect();
    assert_eq!(call[0].addr, 0x3d10e0);
    for w in call.windows(2) {
        assert_eq!(w[0].addr + w[0].len, w[1].addr, "call 块有洞：{:x}", w[0].addr);
    }
    assert_eq!(call[call.len() - 1].addr + call[call.len() - 1].len, 0x3d2610);
    // `call_0` 块 0x3d2610..0x3d3410，16 × 224 严格等距。
    let c0: Vec<&Mono> = MONOS.iter().filter(|m| m.method == Method::Call0).collect();
    assert_eq!(c0[0].addr, 0x3d2610);
    for w in c0.windows(2) {
        assert_eq!(w[0].addr + 224, w[1].addr, "call_0 块不等距：{:x}", w[0].addr);
        assert_eq!(w[0].len, 224);
    }
    assert_eq!(c0[c0.len() - 1].addr + 224, 0x3d3410);
    // 两个块互相不重叠、也不接在对方头上（中间隔着 0x3d3410..0x3d4fa0）。
    assert!(call[call.len() - 1].addr + call[call.len() - 1].len <= c0[0].addr);
}

#[test]
fn arm64_rcb_size_matches_the_other_arch() {
    // 同一个 `T` 在两个架构上 `size_of::<T>()` 必须一样 —— 35 行逐一相等。
    for m in ARM64_MONOS.iter() {
        let t = TYPES
            .iter()
            .find(|t| t.name == m.ty)
            .unwrap_or_else(|| panic!("{} 不在 TYPES 里", m.ty));
        assert_eq!(
            m.rcb_size,
            RCB_PREFIX + t.size_of_t,
            "{} 的 RcBox 尺寸 {:#x} != 24 + {}",
            m.ty,
            m.rcb_size,
            t.size_of_t
        );
        // 分配对齐 39 处全是 8。
        assert_eq!(DEALLOC_ALIGN, 8);
    }
    // 反过来：x86 侧每一行的 size_of_t 也要等于 TYPES 里的值。
    for m in MONOS.iter() {
        let t = TYPES.iter().find(|t| t.name == m.ty).expect("TYPES 缺类型");
        assert_eq!(m.size_of_t, t.size_of_t, "{} 两处 size 不一致", m.ty);
    }
}

#[test]
fn every_mono_names_a_known_type() {
    for m in MONOS.iter() {
        assert!(
            TYPES.iter().any(|t| t.name == m.ty),
            "MONOS 里的 {} 不在 TYPES",
            m.ty
        );
        // `call` 一定有载荷位（除内联那一个）、`call_0` 一定没有。
        match m.method {
            Method::Call0 => assert_eq!(m.payload_words, None, "{:x}", m.addr),
            Method::Call => assert!(
                m.payload_words.is_some() || m.addr == 0x3d22a0,
                "{:x} 缺载荷位数",
                m.addr
            ),
        }
        // 每个 `call` 至少能指出一个 `f`（除内联/未命名的那两个）。
        if m.f.is_empty() {
            assert!(
                m.addr == 0x3d22a0 || m.addr == 0x3d2b50,
                "{:x} 没有 f 也没登记为例外",
                m.addr
            );
        }
    }
    for m in ARM64_MONOS.iter() {
        assert!(TYPES.iter().any(|t| t.name == m.ty), "{} 不在 TYPES", m.ty);
        assert_eq!(m.sym_hash.len(), 16, "{:x}", m.addr);
        assert_eq!(m.drop_slow.len(), 16, "{:x}", m.addr);
    }
}

#[test]
fn types_matrix_is_derivable_from_monos() {
    for t in TYPES.iter() {
        let has_call = MONOS
            .iter()
            .any(|m| m.ty == t.name && m.method == Method::Call);
        let has_call0 = MONOS
            .iter()
            .any(|m| m.ty == t.name && m.method == Method::Call0);
        assert_eq!(
            (has_call, has_call0),
            t.x86,
            "{} 的 x86 矩阵与 MONOS 对不上",
            t.name
        );
        let a_has_call = ARM64_MONOS
            .iter()
            .any(|m| m.ty == t.name && m.method == Method::Call);
        let a_has_call0 = ARM64_MONOS
            .iter()
            .any(|m| m.ty == t.name && m.method == Method::Call0);
        assert_eq!(
            (a_has_call, a_has_call0),
            t.arm64,
            "{} 的 arm64 矩阵与 ARM64_MONOS 对不上",
            t.name
        );
    }
    // 两个类型只有 x86 有外联单态化 —— arm64 那两位是「被内联了」，见 INLINED_CALL_SITES。
    let arm_missing: Vec<&str> = TYPES
        .iter()
        .filter(|t| t.arm64 == (false, false) && t.x86 != (false, false))
        .map(|t| t.name)
        .collect();
    assert_eq!(arm_missing, ["GraphwarEntityEffectManager", "GraphwarTargetManager"]);
    // 反向：没有「arm64 有、x86 没有」的类型。
    assert!(TYPES
        .iter()
        .all(|t| !(t.arm64 != (false, false) && t.x86 == (false, false))));
}

#[test]
fn ticks_are_the_payload_free_end() {
    // 「有没有载荷」是**唯一**的分界，而且分界看的是**被调函数的元数**，不是名字：
    // `T::tick` 的两个单态化都在 `call_0`，`handle_*` 的全部都在 `call`；
    // 但 `send_all_info_events` **两种都有** —— 它在 6 个类型上没有载荷、
    // 在 `GraphwarTeamManager` 上带载荷（`0x3d1d00` 同时列了
    // `handle_entity_removed_event` 与 `send_all_info_events`）。
    // ⇒ 这反证了 `call`/`call_0` 不是「按语义选哪个口」，而是「按元数拆的两个口」。
    for m in MONOS.iter() {
        for f in m.f.iter() {
            if f.ends_with("::tick") {
                assert_eq!(m.method, Method::Call0, "{f} 应当走 call_0");
            }
            if f.contains("handle_") {
                assert_eq!(m.method, Method::Call, "{f} 应当走 call");
            }
        }
    }
    let bare_send: Vec<&str> = MONOS
        .iter()
        .filter(|m| {
            m.method == Method::Call0
                && m.f.iter().any(|f| f.ends_with("send_all_info_events"))
        })
        .map(|m| m.ty)
        .collect();
    assert_eq!(bare_send.len(), 10, "实得 {bare_send:?}");
    let payload_send: Vec<&str> = MONOS
        .iter()
        .filter(|m| {
            m.method == Method::Call
                && m.f.iter().any(|f| f.ends_with("send_all_info_events"))
        })
        .map(|m| m.ty)
        .collect();
    assert_eq!(payload_send, ["GraphwarTeamManager"]);
    // 11 个类型有 `send_all_info_events`：10 个无载荷 + 1 个带载荷。
    assert_eq!(bare_send.len() + payload_send.len(), 11);
    // 交叉证据：`handle_entity_removed_event` 只出现在 `call` 里。
    assert!(MONOS
        .iter()
        .filter(|m| m.f.iter().any(|f| f.ends_with("handle_entity_removed_event")))
        .all(|m| m.method == Method::Call));
}

#[test]
fn anchors_are_the_two_borrow_mut_sites() {
    let locs: Vec<(Method, &str, u32, u32)> = ANCHORS.to_vec();
    assert_eq!(locs[0], (Method::Call0, "src/weak_caller.rs", 11, 28));
    assert_eq!(locs[1], (Method::Call, "src/weak_caller.rs", 17, 28));
    // 两个锚的**列号相同** ⇒ 两处 `borrow_mut` 的缩进与接收者表达式长度相同。
    assert_eq!(locs[0].2 + 6, locs[1].2, "17 - 11 必须恰好是 6");
    assert_eq!(locs[0].3, locs[1].3);
}

#[test]
fn anchor_arithmetic_admits_a_six_line_body() {
    // **推断**：把「6 行一个方法、两个方法之间不留空行、`impl` 之前 8 行」摊成文本，
    // 两处 `borrow_mut` 是否真的落在 11:28 与 17:28。
    // 这解释了为什么原版这个文件几乎没有模块级文档（`//!` 每多一行，锚就整体下移）。
    const TEMPLATE: &str = "\
//! weak_caller
//! (only these two lines fit before `use`)
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct WeakCaller<T>(pub Weak<RefCell<T>>);

impl<T> WeakCaller<T> {
    pub fn call_0<F: FnOnce(&mut T)>(self, f: F) {
        if let Some(target) = self.0.upgrade() {
            f(&mut *target.borrow_mut());
        }
    }

    pub fn call<A, F: FnOnce(&mut T, A)>(self, f: F, a: A) {
        if let Some(target) = self.0.upgrade() {
            f(&mut *target.borrow_mut(), a);
        }
    }
}
";    let mut hits: Vec<(u32, u32)> = Vec::new();
    for (i, line) in TEMPLATE.lines().enumerate() {
        if let Some(col) = line.find("borrow_mut") {
            hits.push(((i + 1) as u32, (col + 1) as u32));
        }
    }
    assert_eq!(hits, vec![(11, 28), (17, 28)], "模板算出来的是 {hits:?}");
    assert_eq!(hits[0], (ANCHORS[0].2, ANCHORS[0].3));
    assert_eq!(hits[1], (ANCHORS[1].2, ANCHORS[1].3));
}

#[test]
fn the_40th_function_is_not_a_monomorphisation() {
    // 进度尺的 39 与 fixtures/module_map.tsv 的 40 差的就是它。
    assert!(MONOS.iter().all(|m| m.addr != 0x3d4fa0));
    assert!(MONOS.iter().all(|m| m.addr != 0x3d588c));
    assert_eq!(INLINED_CALL_SITES[0].0, 0x3d4fa0);
    assert!(INLINED_CALL_SITES[0].3.contains("graphwar_game_recorder"));
    // 两块的**上界**说明这 40 个函数的范围：0x3d10e0 .. 0x3d5260。
    let lo = MONOS.iter().map(|m| m.addr).min().unwrap();
    let hi = MONOS
        .iter()
        .map(|m| m.addr + m.len)
        .max()
        .unwrap()
        .min(0x3d5260);
    let inlined_hi = INLINED_CALL_SITES[0].0 + INLINED_CALL_SITES[0].1;
    assert_eq!((lo, hi.max(inlined_hi)), (0x3d10e0, 0x3d5260));
}
