//! `graphwar_ad_switch.rs` —— **广告开关**（原版 `GraphwarAdSwitch`）。
//!
//! # 这个模块是什么
//!
//! 一个只有 **2 个导出方法**、**1 个字段**的 `Node` 派生类：
//!
//! * `are_ads_enabled() -> bool` —— **`!owned`**；
//! * `get_ad_unit_id() -> GString` —— 返回一个 **38 字节** 的 AdMob 单元 id 字面量。
//!
//! 它**不是**联机层：`graphwar_backend.rs` 与广告无关（`docs/19-android.md` 已更正
//! `docs/02-module-map.md` §14.4 的旧归类）。真正的"广告什么时候出现"由
//! `GraphwarRootNode` 的命令链决定：命令 → `0x3af850` →
//! `call_deferred("create_ad_view")` / `call_deferred("destroy_ad_view")`
//! （`docs/16-client.md` §15）。本模块只提供**两个只读判据**给 GDScript 用。
//!
//! # 地址约定
//!
//! 本文件所有地址是 **ELF 虚地址（vaddr，`objdump` 口径）**；
//! Ghidra / REA 地址 = 本文件地址 + `0x100000`。
//! ⚠️ `docs/19-android.md` 里的地址是 **Android arm64** 的，**不能**当 x86 用；
//! 下面凡是引自 `docs/19` 的结论都标了「arm64」。
//!
//! # 证据档位（`AGENTS.md` §0 第 3 条）
//!
//! * **观测** —— 能指到具体指令 / `.rodata` 字节；
//! * **推断** —— 有依据但没证，写明缺什么；
//! * **未知** —— 没解出来。
//!
//! # 名字与地址
//!
//! `fixtures/name_to_addr.tsv` 里本模块**只有一个**方法级记录
//! （`__register_constants`，标「歧义」，候选 4 个），两个真方法名来自
//! **扩展 API 导出表**（`docs/18-build.md` §6.5）与 **注册闭包引用的 `.rodata` 字面量**
//! ——两条互相独立、结论一致：
//!
//! | 方法 | 名字字面量 | 长度 | 注册闭包 A / B | trampoline A / B | 档位 |
//! |---|---|---|---|---|---|
//! | `are_ads_enabled` | `0x1c4425` | 15 | `0x4f02b0` / `0x4f0520` | `0x422ab0` / `0x42b7e0` | 观测 |
//! | `get_ad_unit_id` | `0x1c4434` | 14 | `0x4f0370` / `0x4f0430` | `0x42e920` / `0x42e500` | 观测 |
//!
//! 类名字面量 `0x19e6a0`（长度 `0x10` = 16 = `"GraphwarAdSwitch"`），
//! 被全部 4 个方法注册闭包引用（`0x4f02d2` / `0x4f0392` / `0x4f045f` / `0x4f054f`）。
//!
//! # 6 个函数（全部是 gdext 惰性注册驱动）
//!
//! `docs/19-android.md` §3.0.3 记「全二进制只有 **6 条** `FUNC` 符号」，与 x86 侧
//! 用 `panic::Location` 定出的 6 个函数**数量一致**：
//!
//! | vaddr | 跨度 | Location | 形状 |
//! |---|---|---|---|
//! | `0x4e5100` | `0x180` | `14:1` | once 锁 + 遍历静态表 |
//! | `0x4e5280` | `0x180` | `14:1` | 同上 |
//! | `0x4e9730` | `0x1b0` | `14:1` | 同上 |
//! | `0x4e98e0` | `0x210` | `7:10` | 同上 |
//! | `0x4e9af0` | `0x1a0` | `14:1` | 同上 |
//! | `0x4e9c90` | `0x250` | `45:1` | 同上 |
//!
//! 六条的形状逐条相同（`lock cmpxchg` 起手 + `movzx` 检查 + 遍历时 `call` 每个槽），
//! 且这些**跨度与 `graphwar_function_input` 的 6 个注册驱动完全一样**
//! （`0x180/0x180/0x1b0/0x210/0x1a0/0x250`）——同一组 gdext 单态化被两个类各实例化一次。
//!
//! ⇒ 这个文件**极短**：Location 只落在 `7:10`、`14:1`、`45:1` 三行，与
//! `docs/02-module-map.md` §18 A 表给的 `7–45` 行范围一致。
//!
//! # 唯一的字段：`owned`，偏移 `+0x18`
//!
//! 档位：**逐位（arm64）+ 推断（"该字节就是 owned"）**，见 `docs/19-android.md` §2.10。
//!
//! * `are_ads_enabled` 的 ptrcall 实例 `0x377cf4`（arm64）是
//!   `borrow_mut` → `ldrb w22,[obj+0x18]` → `eor w8,w22,#1` → 写回 out 参数；
//! * 初值 `1`（`create` 里 `mov w8,#0x1` 写进该槽；跨类判别器：
//!   另一个 Control 派生类在同槽写 `3`，所以该槽是**按类不同的实例字段**）；
//! * 全 `.text` 里落在 `[reg,#0x18]` 的字节写共 167 处，**只有 2 处**触及本类：
//!   `GraphwarRootNode::purchase_confirmed`（写 `1`）与 `purchase_denied`（写 `0`）；
//! * **没有任何 `graphwar_ad_switch` 函数**写这个字节 ⇒ `owned` 只能由 GDScript 改。
//!
//! ⚠️ x86 侧的等价指令（`0x422ab0` / `0x42b7e0` 里 `!owned` 的那一步）本轮**没有**
//! 逐指令钉死：`0x422ab0` 在 `0x422b26` 处有一条 `movb $0x0,(%r15)` 写返回槽，
//! 但它是"借用成功"分支的常量写，不是 `!owned` 本身。缺的证据：x86 上
//! `ldrb/eor` 那一对的地址（arm64 侧已有）。
//!
//! # 38 字节的 AdMob 单元 id
//!
//! 字面量在 `.rodata` `0x1c43ff`，长度 **38**（`docs/19-android.md` §3.0.3 记的
//! arm64 地址是 `0x19dd6c`，长度同样是 38）。内容是 **Google 官方的测试广告单元 id**
//! ——即原版在所有平台都只用测试单元，不接真实收益。见 [`AD_UNIT_ID`]。

// ---------------------------------------------------------------------------
// 1. 常量与字面量
// ---------------------------------------------------------------------------

/// 原版类名（`.rodata` `0x19e6a0`，长度 `0x10` = 16）。档位：**观测**。
pub const CLASS: &str = "GraphwarAdSwitch";

/// 原版源文件名（`panic::Location` 的 `file` 段）。档位：**观测**。
pub const SOURCE_FILE: &str = "src/graphwar_ad_switch.rs";

/// 类名字面量地址。档位：**观测**（`0x4f02d2` 等 4 处 `lea` + `movq $0x10,0x28(%rsp)`）。
pub const CLASS_LITERAL: u64 = 0x19e6_a0;

/// 类名字面量长度。档位：**观测**。
pub const CLASS_LEN: u32 = 16;

/// `get_ad_unit_id()` 返回的那个 38 字节字面量（`.rodata` `0x1c43ff`）。
///
/// 档位：**观测**（唯一引用它的两个函数 `0x42e500` / `0x42e920` 正是
/// `get_ad_unit_id` 的两个 trampoline；长度 38 = `docs/19-android.md` §3.0.3 的 38）。
///
/// ⚠️ 这是 **Google 官方的测试广告单元**（`ca-app-pub-3940256099942544/...`）。
/// 原版没有为正式发布换 id —— 所以"游戏靠广告赚钱"这件事在 2.0.60 的 Linux 构建里
/// **没有证据**；`are_ads_enabled()` 的返回值才是闸门。
pub const AD_UNIT_ID: &str = "ca-app-pub-3940256099942544/6300978111";

/// 上面那条字面量的地址。档位：**观测**。
pub const AD_UNIT_ID_LITERAL: u64 = 0x1c43_ff;

/// 上面那条字面量的字节数。档位：**观测**。
pub const AD_UNIT_ID_LEN: u32 = 38;

/// 全 `.text` 里**唯一**引用 [`AD_UNIT_ID_LITERAL`] 的两条指令所属函数 ——
/// 正是 `get_ad_unit_id` 的两个 trampoline（`0x4f0430`/`0x4f0370` 两个注册闭包调用的
/// 就是它们）。档位：**观测**（穷举 `.rodata` 交叉引用）。
pub const AD_UNIT_ID_REFS: [u64; 2] = [0x42e5_00, 0x42e9_20];

/// `owned` 字段在实例结构里的偏移（`docs/19-android.md` §2.10①，arm64 逐位）。档位：**观测**。
pub const OWNED_FIELD_OFFSET: usize = 0x18;

/// `owned` 的初值。档位：**观测**（`create` 里写 `1`；跨类判别器见 `docs/19` §2.10②）。
pub const DEFAULT_OWNED: bool = true;

// ---------------------------------------------------------------------------
// 2. 函数表
// ---------------------------------------------------------------------------

/// 6 个函数的地址（`panic::Location` 归属，档位 **观测**）。
pub const FUNCTIONS: [FunctionEntry; 6] = [
    FunctionEntry { vaddr: 0x4e51_00, span: 0x180, anchor: Some((14, 1)) },
    FunctionEntry { vaddr: 0x4e52_80, span: 0x180, anchor: Some((14, 1)) },
    FunctionEntry { vaddr: 0x4e97_30, span: 0x1b0, anchor: Some((14, 1)) },
    FunctionEntry { vaddr: 0x4e98_e0, span: 0x210, anchor: Some((7, 10)) },
    FunctionEntry { vaddr: 0x4e9a_f0, span: 0x1a0, anchor: Some((14, 1)) },
    FunctionEntry { vaddr: 0x4e9c_90, span: 0x250, anchor: Some((45, 1)) },
];

/// 单条函数的证据记录。
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    /// ELF 虚地址（Ghidra = 本值 + `0x100000`）。
    pub vaddr: u64,
    /// 线性"下一个起点表"给出的跨度（`analysis/funcstarts_ghidra_va.pkl`）。
    pub span: u32,
    /// 它引用的 `src/graphwar_ad_switch.rs` 的 `(line, col)`。
    pub anchor: Option<(u32, u32)>,
}

/// 按地址找记录。
pub fn function_at(vaddr: u64) -> Option<&'static FunctionEntry> {
    FUNCTIONS.iter().find(|f| f.vaddr == vaddr)
}

/// Ghidra / REA 地址（= vaddr + `0x100000`）。
pub const fn ghidra(vaddr: u64) -> u64 {
    vaddr + 0x10_0000
}

/// 这个文件只有三个 Location，都在这里。档位：**观测**。
pub const ANCHOR_LINES: [(u32, u32); 3] = [(7, 10), (14, 1), (45, 1)];

// ---------------------------------------------------------------------------
// 3. 两个导出方法
// ---------------------------------------------------------------------------

/// 一个导出方法的注册记录。字段口径同
/// [`crate::graphwar_function_input::MethodRegistration`]（A/B 不硬指 ptrcall/varcall）。
#[derive(Debug, Clone, Copy)]
pub struct MethodRegistration {
    /// Godot 侧名字。
    pub name: &'static str,
    /// 名字字面量 vaddr。
    pub name_literal: u64,
    /// 指令里的长度。
    pub name_len: u32,
    /// 第一个注册闭包。
    pub closure_a: u64,
    /// 第二个注册闭包。
    pub closure_b: u64,
    /// `closure_a` 调用的 trampoline。
    pub trampoline_a: u64,
    /// `closure_b` 调用的 trampoline。
    pub trampoline_b: u64,
}

/// **导出方法 2 个**。档位：**观测**（字面量 + 长度 + 4 个闭包 + 4 个 trampoline）。
pub const EXPORTS: [MethodRegistration; 2] = [
    MethodRegistration {
        name: "are_ads_enabled",
        name_literal: 0x1c44_25,
        name_len: 15,
        closure_a: 0x4f02_b0,
        closure_b: 0x4f052_0,
        trampoline_a: 0x422a_b0,
        trampoline_b: 0x42b7_e0,
    },
    MethodRegistration {
        name: "get_ad_unit_id",
        name_literal: 0x1c44_34,
        name_len: 14,
        closure_a: 0x4f03_70,
        closure_b: 0x4f043_0,
        trampoline_a: 0x42e9_20,
        trampoline_b: 0x42e5_00,
    },
];

/// 按名字查注册记录。
pub fn export(name: &str) -> Option<&'static MethodRegistration> {
    EXPORTS.iter().find(|e| e.name == name)
}

// ---------------------------------------------------------------------------
// 4. 语义：`are_ads_enabled() = !owned`
// ---------------------------------------------------------------------------

/// `owned` → `are_ads_enabled()` 的返回值。
///
/// 档位：**观测 +（arm64 逐指令）**。arm64 侧的实现在 `0x377cf4`
/// （ptrcall 实例）：`ldrb w22,[obj+0x18]` → `eor w8,w22,#1` → 写回 out。
/// 见 `docs/19-android.md` §3.0.3 那张表。
///
/// 语义推论（**推断**，缺"这个字节叫 owned"的直接证据）：`owned == true` 表示
/// **已买断/无广告**，所以广告开关 = `!owned`。`docs/19` §2.10② 用跨类判别器
/// 证明初值是 `true` ⇒ **首次启动时 `are_ads_enabled()` 返回 `false`**。
pub const fn ads_enabled_from_owned(owned: bool) -> bool {
    !owned
}

/// 持有 `owned` 的类的最小模型。
///
/// 只有**一个**字段（`+0x18`）、**两个**只读导出方法、**没有** Rust 内部写点
/// ——这三条都能指到证据，见模块头。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdSwitch {
    owned: bool,
}

impl AdSwitch {
    /// `#[class(init)]` 的初值：`owned = true`（档位：**观测**，见 [`DEFAULT_OWNED`]）。
    pub const fn new() -> Self {
        Self { owned: DEFAULT_OWNED }
    }

    /// 指定 `owned` 构造（测试与"存档恢复"用；原版**没有**从存档读 `owned` 的路径，
    /// 见 `docs/19-android.md` §2.10③）。
    pub const fn with_owned(owned: bool) -> Self {
        Self { owned }
    }

    /// `are_ads_enabled()`：**`!owned`**。档位：**观测（arm64 逐指令）+ 推断（字段语义）**。
    pub const fn are_ads_enabled(&self) -> bool {
        ads_enabled_from_owned(self.owned)
    }

    /// `get_ad_unit_id()`：返回那个 38 字节字面量。档位：**观测**。
    pub const fn get_ad_unit_id(&self) -> &'static str {
        AD_UNIT_ID
    }

    /// 当前 `owned`。
    pub const fn owned(&self) -> bool {
        self.owned
    }

    /// `GraphwarRootNode.purchase_confirmed()` 的效果：`owned = 1`。
    ///
    /// 档位：**观测**（arm64 `0x3a658c` `strb w10(=1),[x8,#0x18]`；
    /// `docs/19-android.md` §2.10③）。x86 侧对应函数本轮未逐指令定位。
    pub fn purchase_confirmed(&mut self) {
        self.owned = true;
    }

    /// `GraphwarRootNode.purchase_denied()` 的效果：`owned = 0`。
    ///
    /// 档位：**观测**（arm64 `0x3a61b0` `strb wzr,[x8,#0x18]`）。同上，x86 未定位。
    pub fn purchase_denied(&mut self) {
        self.owned = false;
    }
}

impl Default for AdSwitch {
    fn default() -> Self {
        Self::new()
    }
}

/// 全二进制里**唯一**两个改 `owned` 的地方（`docs/19-android.md` §2.10③ 的穷举结论）。
///
/// 档位：**观测**（arm64；穷举 `[reg,#0x18]` 的 167 处字节写，只有这两处属于本类）。
pub const OWNED_WRITERS: [(&str, bool, u64); 2] = [
    // (写者, 写进去的值, arm64 指令地址)
    ("GraphwarRootNode::purchase_confirmed", true, 0x3a65_8c),
    ("GraphwarRootNode::purchase_denied", false, 0x3a61_b0),
];

/// 本模块自己**没有**写 `owned` 的函数。档位：**观测**（同一份穷举）。
///
/// ⇒ 想把"已买断"状态灌进去，只能走 GDScript 调
/// `GraphwarRootNode.purchase_confirmed()`；Rust 启动流程里**没有**"重新查单"的代码。
pub const MODULE_HAS_NO_OWNED_WRITER: bool = true;

/// 广告视图的真正开关链（**不在**本模块里）。
///
/// 命令 → `0x3af850` → `call_deferred("create_ad_view" | "destroy_ad_view")`
/// （`docs/16-client.md` §15）。两个 `call_deferred` 的字面量在
/// `0x1bd9c1` / `0x1bd9cf`（`docs/16-client.md` §15 记的地址）。
///
/// 档位：`docs/16-client.md` §15 的**观测**；本文件不复述它的细节。
pub const AD_VIEW_CHAIN: (&str, &str, u64) = ("create_ad_view", "destroy_ad_view", 0x3af_850);

// ---------------------------------------------------------------------------
// 5. 未知
// ---------------------------------------------------------------------------

/// 未解清单。写文档时照抄。
pub const UNKNOWNS: [&str; 3] = [
    "x86 侧 `!owned` 的指令位置：arm64 是 0x377cf4 的 `ldrb/eor`（docs/19 §3.0.3），\
x86 的等价两条在 trampoline 0x422ab0 / 0x42b7e0 里，本轮只确认了 0x422b26 的常量写返回槽，没有逐指令钉死。",
    "6 个注册驱动各自注册的是哪一张表（methods / constants / properties / signals / virtuals）：\
形状一致，归属未逐项还原；name_to_addr.tsv 把 __register_constants 标“歧义”，候选正是 0x4e5100 / \
0x4e5280 / 0x4e9730 / 0x4e9af0 四个。",
    "`owned` 这个**字段名**本身没有符号证据：名字来自 docs/19 §2.10 的语义推断（“已买断 ⇒ 无广告”），\
严格说只观测到“偏移 0x18 的一个 bool + 取反后返回”。",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_functions_match_the_arm64_count() {
        assert_eq!(FUNCTIONS.len(), 6);
        // docs/19 §3.0.3：「全二进制只有 6 条 FUNC 符号」——x86 侧同样是 6
        let mut addrs: Vec<u64> = FUNCTIONS.iter().map(|f| f.vaddr).collect();
        addrs.sort_unstable();
        assert_eq!(
            addrs,
            vec![0x4e51_00, 0x4e52_80, 0x4e97_30, 0x4e98_e0, 0x4e9a_f0, 0x4e9c_90]
        );
        assert!(function_at(0x4e51_00).is_some());
        assert!(function_at(0x4e51_01).is_none());
        for f in FUNCTIONS.iter() {
            assert_eq!(ghidra(f.vaddr), f.vaddr + 0x10_0000);
        }
    }

    #[test]
    fn spans_are_the_same_multiset_as_the_function_input_drivers() {
        // 两组 gdext 单态化被两个类各实例化一次：跨度多重集相同
        let mut here: Vec<u32> = FUNCTIONS.iter().map(|f| f.span).collect();
        here.sort_unstable();
        let mut there: Vec<u32> = crate::graphwar_function_input::FUNCTIONS[..6]
            .iter()
            .map(|f| f.span)
            .collect();
        there.sort_unstable();
        assert_eq!(here, there);
    }

    #[test]
    fn anchors_are_exactly_three_lines() {
        assert_eq!(ANCHOR_LINES, [(7, 10), (14, 1), (45, 1)]);
        let mut seen: Vec<(u32, u32)> = FUNCTIONS.iter().filter_map(|f| f.anchor).collect();
        seen.sort_unstable();
        seen.dedup();
        for a in seen.iter() {
            assert!(ANCHOR_LINES.contains(a), "{a:?}");
        }
        // 45 行 ⇒ 原版这个文件很短（docs/02 §18 A 表：7–45）
        assert!(ANCHOR_LINES.iter().all(|(l, _)| *l <= 45));
    }

    #[test]
    fn two_exports_with_documented_lengths() {
        let names: Vec<&str> = EXPORTS.iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["are_ads_enabled", "get_ad_unit_id"]);
        for e in EXPORTS.iter() {
            assert_eq!(e.name.len() as u32, e.name_len, "{}", e.name);
            assert!(export(e.name).is_some());
        }
        assert!(export("set_owned").is_none());
    }

    #[test]
    fn ad_unit_id_is_the_google_test_id_and_38_bytes() {
        assert_eq!(AD_UNIT_ID.len(), 38);
        assert_eq!(AD_UNIT_ID.len() as u32, AD_UNIT_ID_LEN);
        assert!(AD_UNIT_ID.starts_with("ca-app-pub-3940256099942544/"));
        assert_eq!(AdSwitch::new().get_ad_unit_id(), AD_UNIT_ID);
    }

    #[test]
    fn ads_enabled_is_not_owned() {
        assert!(ads_enabled_from_owned(false));
        assert!(!ads_enabled_from_owned(true));
        // 初值 true（arm64 create 写 1）⇒ 首次启动「广告关闭」
        assert_eq!(DEFAULT_OWNED, true);
        let s = AdSwitch::new();
        assert_eq!(s.owned(), true);
        assert_eq!(s.are_ads_enabled(), false);
        assert_eq!(AdSwitch::with_owned(false).are_ads_enabled(), true);
        assert_eq!(AdSwitch::default(), AdSwitch::new());
    }

    #[test]
    fn only_the_root_node_writes_owned() {
        assert_eq!(OWNED_WRITERS.len(), 2);
        assert!(MODULE_HAS_NO_OWNED_WRITER);
        for (who, _v, _addr) in OWNED_WRITERS.iter() {
            assert!(who.starts_with("GraphwarRootNode::"));
        }
        // 写者的效果与 AdSwitch 的两个方法一致
        let mut s = AdSwitch::new();
        s.purchase_denied();
        assert_eq!(s.owned(), OWNED_WRITERS[1].1);
        assert_eq!(s.are_ads_enabled(), true);
        s.purchase_confirmed();
        assert_eq!(s.owned(), OWNED_WRITERS[0].1);
        assert_eq!(s.are_ads_enabled(), false);
    }

    #[test]
    fn field_offset_and_class_literals() {
        assert_eq!(OWNED_FIELD_OFFSET, 0x18);
        assert_eq!(CLASS, "GraphwarAdSwitch");
        assert_eq!(CLASS.len() as u32, CLASS_LEN);
        assert_eq!(CLASS_LEN, 16); // 0x10，指令立即数
        assert_eq!(SOURCE_FILE, "src/graphwar_ad_switch.rs");
        assert_eq!(CLASS_LITERAL, 0x19e6_a0);
        assert_eq!(AD_UNIT_ID_LITERAL, 0x1c43_ff);
    }

    #[test]
    fn ad_view_chain_lives_outside_this_module() {
        assert_eq!(AD_VIEW_CHAIN.0, "create_ad_view");
        assert_eq!(AD_VIEW_CHAIN.1, "destroy_ad_view");
        assert_eq!(ghidra(AD_VIEW_CHAIN.2), 0x4af_850);
    }
}
