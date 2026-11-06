//! `graphwar_module_container.rs` 的复验收据。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 它们的作用是：把 `src/graphwar_module_container.rs` 里的那张台账
//! 与"从二进制量出来的数"钉在一起，任何一边被改动都会立刻红。
//!
//! 地址口径：本文件所有地址是 **ELF 虚地址**（`objdump` 口径），
//! Ghidra / REA 地址 = 本地址 + `0x100000`。
//! 两个二进制都是 **2.0.60**：arm64（Android，未 strip）与 x86-64（Linux Steam）。
//!
//! # 真值从哪来（每条判据都能自己重跑）
//!
//! 1. **三个方法名**：2024-07-30 版符号表
//!    （`python3 tools/symtab_dump.py methods graphwar_module_container`）；
//!    arm64 / x86 两侧同名同形，只有单态化个数不同。
//! 2. **68 = 25 + 23 + 20**：arm64 `readelf -sW | grep graphwar_module_container
//!    | grep FUNC` 得 68 条（25 `add_module` / 23 `get_module` / 20 `get_module_wk`）。
//! 3. **每个 `add_module` 属于哪个型**：它的第 3 个实参是 `dyn Any` vtable，
//!    该 vtable 首个槽的 `R_AARCH64_RELATIVE` addend 就是
//!    `drop_in_place::<RefCell<M>>`（arm64 有名字）；x86 侧无名字，
//!    改用"两个装配点的调用序 + 尺寸序列"对齐，结果与 arm64 逐型相等。
//! 4. **`size_of::<M>()`**：= vtable 第 2 字 − 8（`RefCell` 的借用标志占 8 字节）。
//! 5. **块结构**：三个块在两侧都严格等距、无空洞 —— 见
//!    [`blocks_are_strictly_regular`]，这条同时是"地址不是抄错"的自校验。

use std::cell::RefCell;
use std::panic::AssertUnwindSafe;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};

use gw2_rebuild::graphwar_module_container as mc;

/// 25 个类型名（arm64 2.0.60 `.symtab` 原文），用于锁死 [`mc::MODULES`]。
const NAMES: &[&str] = &[
    "FunctionHistoryRecorder",
    "GraphwarBanList",
    "GraphwarClientConnectionHandler",
    "GraphwarColorManager",
    "GraphwarComputerPlayerManager",
    "GraphwarEffectManager",
    "GraphwarEntityEffectManager",
    "GraphwarFunctionManager",
    "GraphwarGameRecorder",
    "GraphwarGameStateManager",
    "GraphwarGraphicsManager",
    "GraphwarLifeManager",
    "GraphwarLocalPlayerManager",
    "GraphwarModeManager",
    "GraphwarNameManager",
    "GraphwarObstacleManager",
    "GraphwarPlanePosManager",
    "GraphwarPlayerManager",
    "GraphwarRankManager",
    "GraphwarServerConnectionHandler",
    "GraphwarSoldierManager",
    "GraphwarTargetManager",
    "GraphwarTeamManager",
    "GraphwarTickHandler",
    "GraphwarTurnManager",
];

/// `GraphwarGameClient::new` 的注册序（arm64 `0x409c70`，22 条 `add_module`；
/// x86 `0x517690` 同序、同数）。这是"哪个 `add_module` 属于哪个型"的一路证据。
const CLIENT_ORDER: &[&str] = &[
    "GraphwarTickHandler",
    "GraphwarClientConnectionHandler",
    "GraphwarLocalPlayerManager",
    "GraphwarPlayerManager",
    "GraphwarSoldierManager",
    "GraphwarPlanePosManager",
    "GraphwarGameStateManager",
    "GraphwarFunctionManager",
    "GraphwarTargetManager",
    "GraphwarTeamManager",
    "GraphwarLifeManager",
    "GraphwarObstacleManager",
    "GraphwarTurnManager",
    "GraphwarEffectManager",
    "GraphwarModeManager",
    "GraphwarGraphicsManager",
    "GraphwarEntityEffectManager",
    "GraphwarComputerPlayerManager",
    "FunctionHistoryRecorder",
    "GraphwarNameManager",
    "GraphwarColorManager",
    "GraphwarRankManager",
];

/// 服务端线程的注册序（arm64 `0x483574`，20 条；x86 `0x4500c0` 同序、同数）。
const SERVER_ORDER: &[&str] = &[
    "GraphwarTickHandler",
    "GraphwarBanList",
    "GraphwarServerConnectionHandler",
    "GraphwarPlayerManager",
    "GraphwarSoldierManager",
    "GraphwarPlanePosManager",
    "GraphwarGameStateManager",
    "GraphwarFunctionManager",
    "GraphwarTargetManager",
    "GraphwarTeamManager",
    "GraphwarLifeManager",
    "GraphwarObstacleManager",
    "GraphwarTurnManager",
    "GraphwarEffectManager",
    "GraphwarModeManager",
    "GraphwarGraphicsManager",
    "GraphwarEntityEffectManager",
    "GraphwarNameManager",
    "GraphwarColorManager",
    "GraphwarRankManager",
];

// ---------------------------------------------------------------------------
// 判据 1–3：台账的结构（个数 / 名字 / 覆盖关系）
// ---------------------------------------------------------------------------

/// 判据 1：68 = 25 + 23 + 20，且台账的条数与之一致。
///
/// 真值：arm64 `readelf -sW … | grep graphwar_module_container | grep FUNC` 得 68，
/// 其中 `add_module` 25 / `get_module` 23 / `get_module_wk` 20
/// ；x86 侧三块条数相同（`0x4694f0+160i` / `0x46a490+368i` / `0x46c5a0+544i`）。
#[test]
fn counts_match_the_symbol_table() {
    assert_eq!(mc::ADD_MODULE_COUNT, 25);
    assert_eq!(mc::GET_MODULE_COUNT, 23);
    assert_eq!(mc::GET_MODULE_WK_COUNT, 20);
    assert_eq!(mc::FUNCTION_COUNT, 68);
    assert_eq!(mc::MODULES.len(), 25);

    let add = mc::MODULES.iter().filter(|m| m.arm64.add != 0).count();
    let get = mc::MODULES.iter().filter(|m| m.arm64.get != 0).count();
    let wk = mc::MODULES.iter().filter(|m| m.arm64.wk != 0).count();
    assert_eq!((add, get, wk), (25, 23, 20), "台账里的槽数与块条数必须相等");

    // 两架构的覆盖关系必须完全一致（单态化的取舍不因架构而变）。
    let add_x = mc::MODULES.iter().filter(|m| m.x86.add != 0).count();
    let get_x = mc::MODULES.iter().filter(|m| m.x86.get != 0).count();
    let wk_x = mc::MODULES.iter().filter(|m| m.x86.wk != 0).count();
    assert_eq!((add_x, get_x, wk_x), (25, 23, 20));
}

/// 判据 2：25 个类型名就是 arm64 `.symtab` 里那 25 个，一个不多一个不少。
#[test]
fn names_are_exactly_the_symbol_table_names() {
    let mut got: Vec<&str> = mc::MODULES.iter().map(|m| m.name).collect();
    got.sort_unstable();
    let mut want: Vec<&str> = NAMES.to_vec();
    want.sort_unstable();
    assert_eq!(got, want);

    // 台账里不许有重复（查表函数是 `find`，重复会静默取第一个）。
    for (i, a) in mc::MODULES.iter().enumerate() {
        for b in &mc::MODULES[i + 1..] {
            assert_ne!(a.name, b.name);
            assert_ne!(a.module_path, b.module_path);
        }
    }

    // 模块路径 ↔ 类型名的一对一（原版是 `graphwar_<x>.rs` 里一个同名结构体）。
    for m in mc::MODULES {
        assert!(!m.name.is_empty());
        assert!(!m.module_path.contains(".rs"), "module_path 不带扩展名：{}", m.module_path);
        assert_eq!(mc::find(m.name).map(|e| e.module_path), Some(m.module_path));
        assert_eq!(mc::find_by_module(m.module_path).map(|e| e.name), Some(m.name));
    }
    assert!(mc::find_by_module("graphwar_event_queue").is_none(), "event_queue 不在容器里");
}

/// 判据 3：`get_module` / `get_module_wk` 的**缺口**是确定的，不是数据缺失。
///
/// * 没有 `get_module` 的恰好是 `GraphwarBanList` 与 `GraphwarEntityEffectManager`
///   （两者只用弱引用取）；
/// * 没有 `get_module_wk` 的恰好是 `FunctionHistoryRecorder` /
///   `GraphwarLocalPlayerManager` / `GraphwarClientConnectionHandler` /
///   `GraphwarServerConnectionHandler` / `GraphwarGameRecorder`
///   （五个只被强持有，不需要弱句柄）。
#[test]
fn missing_monomorphizations_are_exactly_the_known_five_plus_two() {
    let no_get: Vec<&str> = mc::MODULES.iter().filter(|m| !m.arm64.has_get()).map(|m| m.name).collect();
    assert_eq!(no_get, ["GraphwarBanList", "GraphwarEntityEffectManager"]);
    let no_wk: Vec<&str> = mc::MODULES.iter().filter(|m| !m.arm64.has_wk()).map(|m| m.name).collect();
    assert_eq!(
        no_wk,
        [
            "GraphwarClientConnectionHandler",
            "GraphwarGameRecorder",
            "GraphwarLocalPlayerManager",
            "GraphwarServerConnectionHandler",
            "FunctionHistoryRecorder",
        ]
    );

    // 缺口在两架构上必须是同一个集合（否则说明有一侧对齐错了）。
    for m in mc::MODULES {
        assert_eq!(m.arm64.has_get(), m.x86.has_get(), "{} 的 get 缺口两架构不一致", m.name);
        assert_eq!(m.arm64.has_wk(), m.x86.has_wk(), "{} 的 wk 缺口两架构不一致", m.name);
    }
}

// ---------------------------------------------------------------------------
// 判据 4：三个块严格等距（地址不是抄错的）
// ---------------------------------------------------------------------------

/// 判据 4：每个块都是 `base + i × stride`，`i` 恰好取遍 `0..count-1`。
///
/// 这一条把"地址表"变成"结构"：任何一个地址写错一位，`index_of` 立刻失败，
/// 或者出现重复/跳号。
#[test]
fn blocks_are_strictly_regular() {
    for arch in [mc::Arch::Arm64, mc::Arch::X86] {
        for method in [mc::Method::Add, mc::Method::Get, mc::Method::Wk] {
            let block = mc::block_of(arch, method);
            let mut seen: Vec<usize> = Vec::new();
            for m in mc::MODULES {
                let Some(addr) = mc::addr_of(m, arch, method) else { continue };
                let i = block
                    .index_of(addr)
                    .unwrap_or_else(|| panic!("{arch:?}/{method:?}: {} 的 {addr:#x} 不在块内", m.name));
                assert_eq!(block.nth(i), addr);
                seen.push(i);
            }
            seen.sort_unstable();
            let want: Vec<usize> = (0..block.count).collect();
            assert_eq!(seen, want, "{arch:?}/{method:?} 的块有跳号或重复");
        }
    }
}

/// 判据 4b：三个块互不重叠 —— `slot_of` 反查唯一的根据。
#[test]
fn blocks_do_not_overlap() {
    for arch in [mc::Arch::Arm64, mc::Arch::X86] {
        let spans: Vec<(u32, u32)> = [mc::Method::Add, mc::Method::Get, mc::Method::Wk]
            .iter()
            .map(|m| {
                let b = mc::block_of(arch, *m);
                (b.base, b.base + b.stride * (b.count as u32 - 1))
            })
            .collect();
        for (i, a) in spans.iter().enumerate() {
            for b in &spans[i + 1..] {
                assert!(a.1 < b.0 || b.1 < a.0, "{arch:?}: 块 {a:?} 与 {b:?} 重叠");
            }
        }
    }
}

/// 判据 4c：`slot_of` 对每个真地址都回得去，对块内非法地址一律 `None`。
#[test]
fn slot_of_roundtrips_and_rejects_off_grid_addresses() {
    for arch in [mc::Arch::Arm64, mc::Arch::X86] {
        for m in mc::MODULES {
            for method in [mc::Method::Add, mc::Method::Get, mc::Method::Wk] {
                if let Some(addr) = mc::addr_of(m, arch, method) {
                    let (got, got_method) = mc::slot_of(arch, addr)
                        .unwrap_or_else(|| panic!("{arch:?}/{method:?}: {addr:#x} 查不回去"));
                    assert_eq!(got.name, m.name);
                    assert_eq!(got_method, method);
                }
            }
        }
        // 块首减 1、块内不对齐 —— 都必须落空。
        for method in [mc::Method::Add, mc::Method::Get, mc::Method::Wk] {
            let b = mc::block_of(arch, method);
            assert!(mc::slot_of(arch, b.base - 1).is_none());
            assert!(mc::slot_of(arch, b.base + 1).is_none(), "块内不对齐必须查不到");
            // 越过块尾一个名额：本块自己必须认不出（可能正好是下一块的块首）。
            assert!(b.index_of(b.nth(b.count)).is_none());
        }
    }
}

// ---------------------------------------------------------------------------
// 判据 5：尺寸 = vtable 的 size 字 − 8
// ---------------------------------------------------------------------------

/// 判据 5：`size` 字段是 `size_of::<M>()`，且指纹形状对得上。
///
/// 真值：每个 `add_module` 的第 3 实参里第 2 字是 `size_of::<RefCell<M>>()`，
/// 减 8（`RefCell` 借用标志）得 `size_of::<M>()`。三个有辨识度的例子：
/// `GraphwarGameStateManager` 296（最大）、`GraphwarGraphicsManager` 168、
/// `GraphwarPlanePosManager` 24（最小）。
#[test]
fn sizes_are_vtable_size_minus_borrow_flag() {
    assert_eq!(mc::find("GraphwarGameStateManager").unwrap().size, 296);
    assert_eq!(mc::find("GraphwarGraphicsManager").unwrap().size, 168);
    assert_eq!(mc::find("GraphwarColorManager").unwrap().size, 120);
    assert_eq!(mc::find("GraphwarServerConnectionHandler").unwrap().size, 128);
    assert_eq!(mc::find("GraphwarClientConnectionHandler").unwrap().size, 112);
    assert_eq!(mc::find("GraphwarBanList").unwrap().size, 96);
    assert_eq!(mc::find("GraphwarObstacleManager").unwrap().size, 80);
    assert_eq!(mc::find("GraphwarLocalPlayerManager").unwrap().size, 64);
    assert_eq!(mc::find("GraphwarGameRecorder").unwrap().size, 56);
    assert_eq!(mc::find("FunctionHistoryRecorder").unwrap().size, 48);
    assert_eq!(mc::find("GraphwarModeManager").unwrap().size, 32);
    assert_eq!(mc::find("GraphwarPlanePosManager").unwrap().size, 24);

    // 全部尺寸都是 8 的倍数（`RefCell<M>` 至少 8 字节对齐 ⇒ `size_of::<M>()`
    // 也必然是 8 的倍数，因为这些型里都有 Rc/Vec/HashMap 之类的 8 字节字段）。
    for m in mc::MODULES {
        assert_eq!(m.size % 8, 0, "{}: size {} 不是 8 的倍数", m.name, m.size);
        assert!(m.size >= 16, "{}: size {} 太小，连 drop 钩子都放不下", m.name, m.size);
    }

    // 尺寸分布（arm64 侧实测）。数字本身就是指纹：13 个 72 字节的 manager
    // 说明它们结构同形（都是若干 Rc + 小状态）。
    let mut hist = std::collections::BTreeMap::new();
    for m in mc::MODULES {
        *hist.entry(m.size).or_insert(0usize) += 1;
    }
    assert_eq!(hist.get(&72), Some(&10), "72 字节的 manager 有 10 个");
    assert_eq!(hist.get(&32), Some(&4), "32 字节的有 4 个");
    assert_eq!(hist.get(&24), Some(&1));
    assert_eq!(hist.get(&48), Some(&1));
    assert_eq!(hist.get(&56), Some(&1));
    assert_eq!(hist.get(&64), Some(&1));
    assert_eq!(hist.get(&80), Some(&1));
    assert_eq!(hist.get(&96), Some(&1));
    assert_eq!(hist.get(&112), Some(&1));
    assert_eq!(hist.get(&120), Some(&1));
    assert_eq!(hist.get(&128), Some(&1));
    assert_eq!(hist.get(&168), Some(&1));
    assert_eq!(hist.get(&296), Some(&1));
    assert_eq!(hist.values().sum::<usize>(), 25);
}

// ---------------------------------------------------------------------------
// 判据 6：装配点（谁在往容器里塞模块）
// ---------------------------------------------------------------------------

/// 判据 6：装配点的个数与调用数，以及"两处装配的并集 + GameRecorder = 全部 25 型"。
///
/// 真值：`GraphwarGameClient::new`（arm64 `0x409c70` / x86 `0x517690`）22 条，
/// `game_server_thread`（arm64 `0x483574` / x86 `0x4500c0`）20 条，
/// `GraphwarBackend::start_*` 两条孪生单态化各 1 条（只为 `GraphwarGameRecorder`）。
/// 22 + 20 − 18（交集） = 24，加 `GameRecorder` = 25。
#[test]
fn registration_sites_cover_all_25_modules() {
    assert_eq!(mc::REGISTRATION_SITES.len(), 4);
    let counts: Vec<usize> = mc::REGISTRATION_SITES.iter().map(|s| s.count).collect();
    assert_eq!(counts, [22, 20, 1, 1]);

    // 每个名字都必须在台账里（避免"调用序表里多写一个型"这种静默错误）。
    for list in [CLIENT_ORDER, SERVER_ORDER] {
        for name in list {
            assert!(mc::find(name).is_some(), "调用序里出现了台账里没有的 {name}");
        }
    }
    assert_eq!(CLIENT_ORDER.len(), 22);
    assert_eq!(SERVER_ORDER.len(), 20);

    // 交集 18 个、并集 24 个，唯一由 `GraphwarBackend::start_*` 注册的是 GameRecorder。
    let client: std::collections::BTreeSet<&str> = CLIENT_ORDER.iter().copied().collect();
    let server: std::collections::BTreeSet<&str> = SERVER_ORDER.iter().copied().collect();
    assert_eq!(client.intersection(&server).count(), 18);
    let union: std::collections::BTreeSet<&str> = client.union(&server).copied().collect();
    assert_eq!(union.len(), 24);

    let missing: Vec<&str> =
        NAMES.iter().copied().filter(|n| !union.contains(n)).collect();
    assert_eq!(missing, ["GraphwarGameRecorder"]);

    // 调用序无重复（每处装配对同一型只调一次）。
    for list in [CLIENT_ORDER, SERVER_ORDER] {
        let set: std::collections::BTreeSet<&str> = list.iter().copied().collect();
        assert_eq!(set.len(), list.len());
    }
}

// ---------------------------------------------------------------------------
// 判据 7：容器本体 —— 48 字节、TypeId 索引、RefCell 语义
// ---------------------------------------------------------------------------

/// 判据 7：容器实例 48 字节，就是 `HashMap<TypeId, Rc<dyn Any>>` 的尺寸。
///
/// 真值：`GraphwarGameClient::new` 申请 `0x48` = 72 字节给
/// `Rc<RefCell<GraphwarModuleContainer>>`，减去 Rc 头 16 + `RefCell` 借用标志 8。
/// 另外 `TypeId` 必须 16 字节（键是 128 位）、`dyn Any` 的 vtable 必须 4 字。
#[test]
fn container_layout_is_a_type_id_hash_map() {
    use std::any::{Any, TypeId};
    use std::collections::HashMap;

    assert_eq!(std::mem::size_of::<mc::GraphwarModuleContainer>(), mc::CONTAINER_SIZE);
    assert_eq!(mc::CONTAINER_SIZE, 48);
    assert_eq!(std::mem::size_of::<TypeId>(), 16, "键是 128 位");
    assert_eq!(
        std::mem::size_of::<HashMap<TypeId, Rc<dyn Any>>>(),
        mc::CONTAINER_SIZE,
        "容器的 48 字节就是这个 HashMap 的尺寸"
    );
    assert_eq!(std::mem::size_of::<Rc<dyn Any>>(), 16, "胖指针 = data + vtable");
    assert_eq!(mc::ANY_VTABLE_WORDS, 4);
    // vtable 是 `drop` / `size` / `align` / `type_id` 四字，二进制里相邻两型的
    // vtable 间距 0x20（arm64 `0x858aa8`→`0x858ac8`）= 4 × 8 字节。
    assert_eq!(mc::ANY_VTABLE_WORDS * 8, 0x20);
    assert_eq!(mc::BUCKET_ENTRY_SIZE, 16 + 16, "实体 = TypeId 键 + 胖指针值");
}

/// 判据 7c：三个块在内存里**背靠背、零空隙**——一条链，不是三个孤立区间。
///
/// 真值（两侧都用块首/步长算，不查表）：
///
/// * x86：`0x4694f0 + 25×160 = 0x46a490`（get 块首）、
///   `0x46a490 + 23×368 = 0x46c5a0`（wk 块首）；
/// * arm64：`0x49e520 + 25×212 = 0x49f9d4`（get 块首）、
///   `0x49f9d4 + 23×432 = 0x4a20a4`（wk 块首）。
///
/// 即 `add` 的块尾正好是 `get` 的块首、`get` 的块尾正好是 `wk` 的块首：
/// 三块是**一次连续发射**的，编译单元把它们挨着排。这条是地址表没有抄错的
/// 第二重校验（第一重是块内等距，见 [`blocks_are_strictly_regular`]）。
#[test]
fn blocks_are_back_to_back() {
    assert_eq!(mc::ARM64_ADD_MODULE.nth(mc::ARM64_ADD_MODULE.count), mc::ARM64_GET_MODULE.base);
    assert_eq!(
        mc::ARM64_GET_MODULE.nth(mc::ARM64_GET_MODULE.count),
        mc::ARM64_GET_MODULE_WK.base
    );
    assert_eq!(mc::X86_ADD_MODULE.nth(mc::X86_ADD_MODULE.count), mc::X86_GET_MODULE.base);
    assert_eq!(mc::X86_GET_MODULE.nth(mc::X86_GET_MODULE.count), mc::X86_GET_MODULE_WK.base);

    // 相接处那个地址是**下一块的第 0 个**，不是上一块的溢出。
    let (e, m) = mc::slot_of(mc::Arch::X86, mc::X86_GET_MODULE.base).unwrap();
    assert_eq!(m, mc::Method::Get);
    assert_eq!(mc::X86_GET_MODULE.index_of(e.x86.get), Some(0));
    // arm64 同理：`0x49f9d4` 是 get 块的第 0 个（GraphwarEffectManager）。
    let (e, m) = mc::slot_of(mc::Arch::Arm64, mc::ARM64_GET_MODULE.base).unwrap();
    assert_eq!((e.name, m), ("GraphwarEffectManager", mc::Method::Get));
}

/// 判据 7b：`RefCell<M>` 的尺寸恰好是 `size_of::<M>() + 8`（借用标志）——
/// 这是台账 `size` 字段的换算依据（vtable 第 2 字 = `size_of::<RefCell<M>>()`）。
#[test]
fn refcell_adds_exactly_the_borrow_flag() {
    #[repr(align(8))]
    struct Eight(u64);
    let probe = Eight(0);
    assert_eq!(probe.0, 0);
    assert_eq!(std::mem::size_of_val(&probe), 8);
    assert_eq!(std::mem::size_of::<RefCell<Eight>>(), std::mem::size_of::<Eight>() + 8);
    assert_eq!(std::mem::size_of::<RefCell<Eight>>(), 16);

    // 换一个更大的 M：差值恒为 8（借用标志是 `Cell<BorrowFlag>`，一个机器字）。
    #[repr(align(8))]
    struct Big([u64; 5]);
    let big = Big([0; 5]);
    assert_eq!(big.0.len(), 5);
    assert_eq!(std::mem::size_of_val(&big), 40);
    assert_eq!(std::mem::size_of::<RefCell<Big>>(), 48);
}

/// 判据 8：注册是**按类型**而不是按实例——两个同型实例后到者替换先到者，
/// 被替换的那个立刻 drop。
///
/// 真值：`add_module` 的 `HashMap::insert` 返回值被就地丢弃
/// （x86 `0x4694f0`：`call HashMap::insert` 之后紧接 `test %rax,%rax` + `decq (%rax)`）。
#[test]
fn add_module_replaces_and_drops_the_previous_instance() {
    static DROPPED: AtomicUsize = AtomicUsize::new(0);
    struct Tracked(u32);
    impl Drop for Tracked {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::SeqCst);
        }
    }

    let mut c = mc::GraphwarModuleContainer::new();
    assert!(c.is_empty() && c.len() == 0);

    c.add_module(Rc::new(RefCell::new(Tracked(1))));
    assert_eq!(c.len(), 1);
    assert_eq!(c.get_module::<Tracked>().unwrap().borrow().0, 1);

    // 同型再来一个：表不长大，旧值被 drop（此时 Rc 计数归零）。
    c.add_module(Rc::new(RefCell::new(Tracked(2))));
    assert_eq!(c.len(), 1);
    assert_eq!(DROPPED.load(Ordering::SeqCst), 1, "被替换的旧实例必须当场 drop");
    assert_eq!(c.get_module::<Tracked>().unwrap().borrow().0, 2);

    // 不同型各自独立（键是 TypeId）。
    c.add_module(Rc::new(RefCell::new(7u8)));
    assert_eq!(c.len(), 2);
    assert_eq!(*c.get_module::<u8>().unwrap().borrow(), 7);
    assert_eq!(c.get_module::<Tracked>().unwrap().borrow().0, 2);
}

/// 判据 9：`get_module` 的返回值是**克隆**（`Rc::clone`），不是移动；
/// 类型不符只返回 `None`，不 panic。
///
/// 真值：`get_module` 命中路径是 `ldp x19, x20, [x15, #-0x10]` + `incq (%x19)`
/// （取胖指针并加计数），不等分支只做 `decq` + `Rc::drop_slow`。
#[test]
fn get_module_clones_and_never_panics_on_type_mismatch() {
    let mut c = mc::GraphwarModuleContainer::new();
    let m = Rc::new(RefCell::new(41u32));
    c.add_module(Rc::clone(&m));
    assert_eq!(Rc::strong_count(&m), 2, "容器自己持一份");

    let a = c.get_module::<u32>().unwrap();
    let b = c.get_module::<u32>().unwrap();
    assert_eq!(Rc::strong_count(&m), 4, "两次取回 = 两份新克隆 + 容器一份 + 原变量");
    assert!(Rc::ptr_eq(&a, &m) && Rc::ptr_eq(&b, &a), "取回的是同一个分配");
    assert_eq!(*a.borrow(), 41);

    // 未注册的类型、以及"注册了别的类型"都不 panic。
    assert!(c.get_module::<u64>().is_none());
    assert!(c.get_module::<String>().is_none());
    assert!(!c.contains::<u64>() && c.contains::<u32>());
    // RefCell 可变借用中也能取（容器只借用自己那一层，不碰 M）。
    *a.borrow_mut() = 42;
    assert_eq!(*c.get_module::<u32>().unwrap().borrow(), 42);
}

/// 判据 10：`get_module_wk` 返回的 `Weak` 不延长生命；容器一死就取不回来。
///
/// 真值：`get_module_wk` 的 Some 路径最后是 `Rc::downgrade`（`incq 0x8(%x19)`），
/// 返回值是可空指针（`Option<Weak>` 的 niche，arm64 里 None 就是 `x0 = 0`）。
#[test]
fn get_module_wk_is_weak_and_dies_with_the_container() {
    let container = Rc::new(RefCell::new(mc::GraphwarModuleContainer::new()));
    {
        let weak = Rc::downgrade(&container);
        let module = Rc::new(RefCell::new(vec![1u8, 2, 3]));
        container.borrow_mut().add_module(Rc::clone(&module));

        let wk = mc::GraphwarModuleContainer::get_module_wk::<Vec<u8>>(&weak)
            .expect("注册过的类型要能取到弱引用");
        let up = wk.upgrade().expect("容器活着时弱引用能升级");
        assert_eq!(*up.borrow(), vec![1, 2, 3]);
        assert!(Rc::ptr_eq(&up, &module));
        assert!(!wk.upgrade().unwrap().borrow().is_empty());

        // 未注册的类型：None（而不是 panic）。
        assert!(mc::GraphwarModuleContainer::get_module_wk::<String>(&weak).is_none());
    }

    // 容器还在：弱引用仍有效。
    let weak = Rc::downgrade(&container);
    let wk = mc::GraphwarModuleContainer::get_module_wk::<Vec<u8>>(&weak).unwrap();
    assert!(wk.upgrade().is_some());

    drop(container);
    assert!(weak.upgrade().is_none(), "容器死了，upgrade 必须为 None");
    assert!(wk.upgrade().is_none(), "模块随容器里的那份 Rc 一起死");
}

/// 判据 11：`Weak::new()`（`ptr == usize::MAX` 的哨兵）直接给 `None`。
///
/// 真值：`get_module_wk` 的第一条比较就是 `cmn x20, #1`（arm64）
/// / 对应 x86 `cmp $-1`，即 `Weak::inner()` 的判据。
#[test]
fn get_module_wk_handles_the_empty_weak_sentinel() {
    let empty: Weak<RefCell<mc::GraphwarModuleContainer>> = Weak::new();
    assert!(mc::GraphwarModuleContainer::get_module_wk::<u8>(&empty).is_none());
}

/// 判据 12：容器已被可变借用时，`get_module_wk` **panic**
/// （`panic_already_mutably_borrowed`），这就是 `30:28` 那个 `Location` 的来源。
///
/// 真值：`get_module_wk` 里 `ldr x8,[x20,#0x10]; cmp x8, #0x7fff…ff; b.hs
/// panic_already_mutably_borrowed`；20 个单态化共用同一个
/// `src/graphwar_module_container.rs:30:28`。
#[test]
fn get_module_wk_panics_when_the_container_is_mutably_borrowed() {
    assert_eq!(mc::WK_BORROW_LOCATION, (30, 28));

    let container = Rc::new(RefCell::new(mc::GraphwarModuleContainer::new()));
    container.borrow_mut().add_module(Rc::new(RefCell::new(1u8)));
    let weak = Rc::downgrade(&container);

    // 先把容器可变借住，然后调 get_module_wk —— 必须炸。
    let guard = container.borrow_mut();
    let hooked = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // 安静点，别把 panic 打到测试输出
    let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let _ = mc::GraphwarModuleContainer::get_module_wk::<u8>(&weak);
    }));
    std::panic::set_hook(hooked);
    drop(guard);

    assert!(caught.is_err(), "可变借用中的 borrow() 必须 panic（原版 30:28）");
}

/// 判据 13：容器自己不带任何"按偏移取值"的口——只有 TypeId 查表。
///
/// 这条是**否定判据**：如果哪天有人给容器加上 `field_at(0x128)` 之类的 API，
/// 本测试不会失败，但 `docs/02-module-map.md` §22 的结论就必须改写。
/// 这里用"同型两个实例互相不可区分"来表达：
/// 容器只认类型，不认装配顺序、不认偏移。
#[test]
fn lookup_is_purely_type_based() {
    let mut a = mc::GraphwarModuleContainer::new();
    let mut b = mc::GraphwarModuleContainer::new();
    let x = Rc::new(RefCell::new(1u16));
    let y = Rc::new(RefCell::new(2u16));
    a.add_module(Rc::clone(&x));
    b.add_module(Rc::clone(&y));

    // 两个容器各自取回自己那个（不是"第一个注册的"或"偏移靠前的"）。
    assert!(Rc::ptr_eq(&a.get_module::<u16>().unwrap(), &x));
    assert!(Rc::ptr_eq(&b.get_module::<u16>().unwrap(), &y));
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
}
