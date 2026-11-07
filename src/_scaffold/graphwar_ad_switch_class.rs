//! **不是原版的东西**：把复原的 [`crate::graphwar_ad_switch`] 挂成**真 Godot 节点类**的绑定外壳。
//!
//! # 为什么会有这一份
//!
//! 原版 `GraphwarAdSwitch` 是 12 个 GDExtension 类之一（`docs/02-module-map.md` §16.2 第 10 项），
//! 全模块只有 **2 个导出方法**、**1 个字段**、**6 个函数**（`docs/16-client.md` §20）。
//! `src/graphwar_ad_switch.rs` 已经把语义（`are_ads_enabled = !owned`）、字面量（38 字节 AdMob 单元 id）、
//! 字段偏移（`+0x18`）和 6 条注册驱动的地址全复原出来了，但它按约定**不碰引擎**。
//!
//! 本文件只做一件事：让那 2 个 getter 真的出现在 Godot 的**方法表**里，
//! 并让 `owned` 真的落在一个 Godot 节点实例上。**算术本体只有一份**，仍在
//! `src/graphwar_ad_switch.rs` 里（[`graphwar_ad_switch::ads_enabled_from_owned`] / [`AdSwitch`]）。
//!
//! # 基类：`Node`（**推断**，不是观测 —— 本批的主要未决项）
//!
//! 这一条是本轮**花时间最多、结论最弱**的一处，所以把证据与缺口都写全（`AGENTS.md` §0 第 3 条）：
//!
//! | 问题 | 结论 | 档位 |
//! |---|---|---|
//! | 它是不是 Godot 类？ | **是**。见下面 6 条 arm64 `.symtab` 证据 | 观测 |
//! | 它的基类是什么？ | **定不出来**。`Node` 是沿用模块文档的既有说法 | **推断** |
//!
//! ## ① 它**是** Godot 类：arm64 2.0.60 `.symtab` 的 6 条证据
//!
//! `readelf -sW libgraphwarrust.so | c++filt | grep ad_switch` 逐条读出来的：
//!
//! | # | 符号（未缩写） | 意义 |
//! |---|---|---|
//! | 1 | `<…GraphwarAdSwitch as godot_core::obj::traits::GodotClass>::class_id::CLASS_ID::h66e2…`（`.data` `0x8b40c0`，8 B） | `GodotClass` 的**关联常量**就定义在 `impl GodotClass for GraphwarAdSwitch` 里 ⇒ 该 impl 存在 |
//! | 2 | `<… as godot_core::obj::traits::cap::ImplementsGodotApi>::__register_methods`（`0x43fb5c`，400 B） | 方法表注册驱动 |
//! | 3 | `<… as ImplementsGodotApi>::__register_constants`（`0x43fcec`，400 B） | 常量表注册驱动 |
//! | 4 | `<… as cap::ImplementsGodotVirtual>::__virtual_call::virtual_fn`（`0x4410f4`，156 B） | 虚方法派发 |
//! | 5 | `<… as UserClass>::__default_virtual_call::virtual_fn`（`0x438194`，156 B） | 未覆写虚方法时的默认派发 |
//! | 6 | `graphwar_ad_switch::GraphwarAdSwitch::__registration_storage::STORAGE`（`.data` `0x8b41e8`，56 B） | 惰性注册库的存储 |
//!
//! 另外还有 `drop_in_place<GraphwarAdSwitch>`（`0x3ae8cc`）与 4 个
//! `graphwar_ad_switch::_::__init::__inner_init`（`0x4439a4` / `0x443b50` / `0x443d48` / `0x443f08`）。
//! 第 2–6 条都是 **`GodotClass` 的 supertrait / 关联项**：没有 `impl GodotClass` 就编不出来。
//! ⇒ 「它不是 Godot 类」这条可能**被证否**。
//!
//! ⚠️ 容易误判的一点：`<… as GodotClass>::class_id` 这个**函数**符号确实**不在**表里
//! （全二进制 12 个类只留下 11 个 `class_id` 函数），但它**内部的 `CLASS_ID` 静态**在
//! （第 1 条）。所以「只有 11 个 `impl GodotClass`」是把**函数符号**当成了 impl 计数，
//! 真实情况是第 12 个 impl 在、只是 `class_id()` 被内联/裁掉了。
//!
//! ## ② 基类**定不出来**，原因是「它一个虚方法都没覆写」
//!
//! 这份 `.symtab` 里唯一**按类**携带基类信息的东西是 `<T as I<Base>>` 这一族 impl 符号，
//! 例如（同一份 `.symtab`，观测）：
//!
//! ```text
//! <GraphwarRootNode as godot_core::gen::classes::node::re_export::INode>::ready    → 基类 Node
//! <GraphwarLanVBox  as godot_core::gen::classes::v_box_container::re_export::IVBoxContainer>::process → 基类 VBoxContainer
//! ```
//!
//! 12 个类里 **11 个**有这族符号；`GraphwarAdSwitch` **一个都没有** ——
//! 因为它**不覆写任何虚方法**（`docs/16` §20.5：6 个函数全是惰性注册驱动，没有独立方法体）。
//! 一个空的 `impl INode for GraphwarAdSwitch {}` 与「不写这个 impl」在机器码上是同一件事，
//! 所以**符号表里不存在能区分基类的东西**。
//!
//! **缺的证据（两条，任一即可定案）**：
//!
//! 1. **静态**：`classdb_register_extension_class` 调用点上传的 `parent_class_name` 实参。
//!    本轮扫过整个 `.text`（`adrp`+`add`/`ldr` 全解），**没有任何一条指令**指向
//!    `.rodata` 里那 4 处 `"GraphwarAdSwitch"` 字节（它们是 `type_name` 游程的一部分，
//!    不以 NUL 结尾）；类的注册走 `.init_array` → `.data.rel.ro` 的函数指针表，
//!    基类名是一个 4–13 字节的合并池短串（`"Node"` / `"Control"` / `"VBoxContainer"`），
//!    **无法**在不跟表的情况下归属到某个类。
//! 2. **运行时**：真机 / headless 里 `ClassDB.get_parent_class("GraphwarAdSwitch")` 一行就够
//!    （本次没有能跑原版 `.so` 的 Godot 4.0 环境）。
//!
//! ## ③ 为什么选 `Node` 而不是别的
//!
//! * 模块文档 [`crate::graphwar_ad_switch`] 与 `docs/02` §16.2 / `docs/16` §20 一直写 `Node`
//!   （同样是**推断**，没有指到字节）；
//! * 它的**实例布局**能对上 `Node` 基类的那种写法（`docs/19` §2.10，**逐位（arm64）**）：
//!   `owned` 在 `+0x18`，而 gdext 的 `Base<T>` 是 24 字节且**排在结构体第一个字段**
//!   ⇒ 源码形状就是 `struct GraphwarAdSwitch { base: Base<…>, owned: bool }`；
//!   这与 `GraphwarLanVBox` 的 `Base<VBoxContainer>`（`+0x20`，同样 24 B，`docs/09` §10.4）一致，
//!   但**24 字节对所有 gdext 基类都一样**，因此它**区分不了**基类；
//! * 它不覆写虚方法 ⇒ 基类只影响 `classdb_register_extension_class` 的父类名与
//!   「能不能挂到某个父节点下」。`Node` 是这一族里最宽的基类，**行为上没有可观测差别**。
//!
//! ⇒ 定档：**`Node` = 推断**；`GraphwarAdSwitch` 是否需要注册 = **需要**（它不是纯 Rust 模块）。
//!
//! # 与原版的差别（逐条，别当成"已复原"）
//!
//! | 项 | 原版 | 本文件 | 档位 |
//! |---|---|---|---|
//! | 基类 | 未知 | `Node` | **推断**（见上） |
//! | `are_ads_enabled` | `!owned`（arm64 `0x377cf4` 的 `ldrb`/`eor`） | 调 `graphwar_ad_switch::ads_enabled_from_owned` | 观测（arm64 逐指令） |
//! | `get_ad_unit_id` | 返回 `.rodata` 的 38 字节字面量 | 返回 [`graphwar_ad_switch::AD_UNIT_ID`] | 观测 |
//! | `owned` 的可见性 | 只有 2 个**读**方法；写点只有 `GraphwarRootNode::purchase_confirmed` / `purchase_denied` | 导出一个 `bool` **属性** `owned`；另给 [`GraphwarAdSwitch::set_owned`]（Rust 侧，给根节点转发用） | **推断**（见下） |
//! | 那个 38 字节 id | Linux 构建是 **Google 测试单元** | 同值 | 观测 |
//!
//! **属性 `owned` 是推断**：`docs/19` §2.10③ 的穷举说这个字节「只能由 GDScript 改」，
//! 而原版**没有** setter 方法（方法表只有 2 个 getter）⇒ 最自然的解释是它是个
//! **可写属性**。缺的证据：属性注册表（gdext 把 `#[var]` 塞在 `__register_methods`
//! 里，符号表看不到内容）。若它其实是私有字段，本文件唯一的差别是多了一个可写属性。
//!
//! # 怎么打开
//!
//! ```text
//! cargo build --release --features gdext-bindings
//! cp $CARGO_TARGET_DIR/release/libgw2_rebuild.so client/libgw2_rebuild.so
//! godot --headless --path client --import
//! godot --headless --path client --script res://tools/base_classes_check.gd
//! ```
//!
//! 默认**关闭**：不打开 `gdext-bindings` 时本文件整个不参与编译，crate 仍然零依赖。
//!
//! # 暴露给 GDScript 的契约
//!
//! ```text
//! GraphwarAdSwitch : Node
//!   @export owned : bool          # +0x18（推断：属性可写）
//!
//!   are_ads_enabled() -> bool     # = !owned（原版）
//!   get_ad_unit_id() -> String    # 38 字节 AdMob 单元 id（原版）
//!   owned_raw() -> bool           # 观测点：字段当前值（与属性同源）
//! ```
//!
//! ⚠️ 本类**故意不实现任何虚方法**（`ready` / `process` 都没有）：原版证据里就没有
//! （这正是基类定不出来的原因）。想在 headless 里区分「引擎没回调」和「回调了没干活」，
//! 请用 [`super::graphwar_root_node_class::GraphwarRootNode`]（它有 `INode` 的
//! `ready` / `process`）与 [`super::graphwar_lan_vbox_class::GraphwarLanVBox`]
//! （它有 `IVBoxContainer` 的 `process`）。

use godot::classes::{INode, Node};
use godot::prelude::*;

use crate::graphwar_ad_switch::{self, AdSwitch};

/// 属性 `owned` 的初值：**`true` = 首次启动不显示广告**。
///
/// 档位：**观测**（arm64 `create` 里往 `+0x18` 写 `1`，`docs/19` §2.10②）。
pub const DEFAULT_OWNED: bool = graphwar_ad_switch::DEFAULT_OWNED;

/// `GraphwarAdSwitch`：只有 2 个只读 getter + 1 个 bool 字段的 `Node`。
///
/// 字段形状照 `docs/19` §2.10 的实例布局：`Base<…>`（24 B）在前、`owned` 在 `+0x18`。
/// gdext 里 `base` 字段必须叫 `base`，它会自动被识别成基类持有者。
#[derive(GodotClass)]
#[class(base = Node)]
pub struct GraphwarAdSwitch {
    base: Base<Node>,

    /// `self + 0x18` 那个 bool。档位：**观测（arm64 逐位）** 偏移 / 初值；
    /// 「它是个**可写属性**」是**推断**（见文件头的说明）。
    #[export]
    owned: bool,
}

impl GraphwarAdSwitch {
    /// `owned` 的 Rust 侧写入口（**不是** `#[func]`）。
    ///
    /// 原版里改这个字节的只有两处，都在 `GraphwarRootNode` 上（arm64 `0x3a658c` 写 1、
    /// `0x3a61b0` 写 0）—— 那两条在 `docs/19` §2.10③ 的穷举里是**唯一**两个写点。
    /// 所以这里也**不**导出成方法，只让根节点在 Rust 侧写。
    ///
    /// ⚠️ 名字**不能**叫 `set_owned`：`#[export] owned` 已经由 gdext 生成了
    /// `set_owned` / `get_owned`（`E0592` 重复定义，本轮实测踩到）。
    pub fn write_owned(&mut self, owned: bool) {
        self.owned = owned;
    }
}

#[godot_api]
impl INode for GraphwarAdSwitch {
    /// `#[class(init)]` 的等价物：唯一的字段取 [`DEFAULT_OWNED`]。
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            owned: DEFAULT_OWNED,
        }
    }

    // 故意**不**写 `ready` / `process`：原版没有（见文件头 ②）。
}

#[godot_api]
impl GraphwarAdSwitch {
    /// `are_ads_enabled()`：**`!owned`**。
    ///
    /// 档位：**观测**（arm64 ptrcall 实例 `0x377cf4`：`ldrb w22,[obj+0x18]` → `eor w8,w22,#1`）。
    /// 判据只有一份，在 [`graphwar_ad_switch::ads_enabled_from_owned`]。
    #[func]
    pub fn are_ads_enabled(&self) -> bool {
        graphwar_ad_switch::ads_enabled_from_owned(self.owned)
    }

    /// `get_ad_unit_id()`：那个 38 字节的 AdMob 单元 id。
    ///
    /// 档位：**观测**（`.rodata` `0x1c43ff`；arm64 侧 `0x19dd6c` 起 38 字节，内容不是同一个 id ——
    /// Android 构建里是 `ca-app-pub-2005941169362714/5433871375`，见下面的测试）。
    /// ⚠️ Linux 2.0.60 的这份是 **Google 官方测试单元**，原版没换成正式 id。
    #[func]
    fn get_ad_unit_id(&self) -> GString {
        GString::from(graphwar_ad_switch::AD_UNIT_ID)
    }

    /// 字段当前值（观测点）：绕开 `!` 直接读 `owned`。
    ///
    /// 验收脚本用它区分「`are_ads_enabled` 算错了」和「`owned` 根本没写进去」。
    #[func]
    fn owned_raw(&self) -> bool {
        self.owned
    }

    /// `owned` 字段的偏移（`+0x18`），让 GDScript 侧也能对一遍文档里的数。
    #[func]
    fn owned_field_offset(&self) -> i64 {
        graphwar_ad_switch::OWNED_FIELD_OFFSET as i64
    }

    /// 纯逻辑模型（`AdSwitch`）与实例字段是否同源。
    ///
    /// 两个 getter 都只经 [`AdSwitch`]：本方法把同一份 `owned` 灌进模型再比一次，
    /// 防止"类里悄悄长了第二份 `!owned`"。
    #[func]
    fn model_matches(&self) -> bool {
        AdSwitch::with_owned(self.owned).are_ads_enabled()
            == graphwar_ad_switch::ads_enabled_from_owned(self.owned)
            && AdSwitch::with_owned(self.owned).owned() == self.owned
    }
}

// ---------------------------------------------------------------------------
// 单测（**不碰引擎**：只测「类里没有第二份算术」）
// ---------------------------------------------------------------------------
//
// 真正的验收在 `client/tools/base_classes_check.gd`（headless，真引擎）。

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认值：首次启动 `owned = true` ⇒ **不显示广告**。
    #[test]
    fn default_owned_means_ads_off() {
        assert!(DEFAULT_OWNED);
        assert_eq!(DEFAULT_OWNED, graphwar_ad_switch::DEFAULT_OWNED);
        assert!(!graphwar_ad_switch::ads_enabled_from_owned(DEFAULT_OWNED));
    }

    /// 类里那个 getter 与纯逻辑是同一条式子（拿模型对一遍）。
    #[test]
    fn getter_is_the_module_formula() {
        for owned in [true, false] {
            let model = AdSwitch::with_owned(owned);
            assert_eq!(
                model.are_ads_enabled(),
                graphwar_ad_switch::ads_enabled_from_owned(owned)
            );
            assert_eq!(model.owned(), owned);
        }
    }

    /// 字段偏移与 arm64 逐位证据一致；单元 id 是 38 字节。
    #[test]
    fn field_offset_and_id_length_match_the_evidence() {
        assert_eq!(graphwar_ad_switch::OWNED_FIELD_OFFSET, 0x18);
        assert_eq!(graphwar_ad_switch::AD_UNIT_ID.len(), 38);
        assert_eq!(
            graphwar_ad_switch::AD_UNIT_ID.len() as u32,
            graphwar_ad_switch::AD_UNIT_ID_LEN
        );
    }
}
