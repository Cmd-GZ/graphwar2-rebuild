//! `player_showcase.rs` 的复验收据（逐条对拍 `src/player_showcase.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0 第 2/5 条：**这些断言是回归保护，不是复原证据。**
//! 每条右侧的真值要么是**指令立即数**、要么是 `.rodata` 比特、要么是 arm64 `.symtab` 真名
//! ——正文里的出处逐条写在 `src/player_showcase.rs` 的注释里。
//!
//! 地址口径：全文件 **ELF 虚地址**（`objdump`），Ghidra = 本值 + `0x100000`。
//! arm64 地址只用于定名，**不在**断言里当 x86 地址用。

use gw2_rebuild::player_showcase as ps;

// ---------------------------------------------------------------------------
// 判据 1：函数表 = 10 个，且 21 条 Location 引用点与 module_map 对得上
// ---------------------------------------------------------------------------

/// 判据 1：本模块 **10 个函数**、**21 条** Location 指令级引用点、**18 个**去重 Location。
///
/// 真值：`fixtures/module_map.tsv` 给 `player_showcase.rs` 的行是
/// `functions=10 / refs=21`；arm64 `.symtab` 侧同样是 10 个函数
/// （4 个方法体 + 6 个 gdext 生成物）。
#[test]
fn ten_functions_and_twenty_one_anchor_sites() {
    assert_eq!(ps::FUNCTIONS.len(), 10);
    assert_eq!(ps::total_anchor_sites(), 21);
    assert_eq!(ps::LOCATIONS.len(), 21);
    assert_eq!(ps::distinct_location_count(), 18);
    // 4 个方法体 + 6 个样板
    assert_eq!(ps::method_bodies().count(), 4);
    let boiler = ps::FUNCTIONS
        .iter()
        .filter(|f| !f.role.is_game_logic())
        .count();
    assert_eq!(boiler, 6);
}

/// 判据 2：4 个方法体的 x86 地址就是 `name_to_addr.tsv` 的 4 条"唯一锚"。
///
/// 真值：TSV 的 `draw`/`ready`/`process`/`on_text_change` 四行（都标 `唯一锚`）。
#[test]
fn four_method_bodies_match_the_unique_anchors() {
    let expect = [
        (0x4e54_00, ps::Role::Draw),
        (0x4e64_80, ps::Role::Ready),
        (0x4e68_f0, ps::Role::Process),
        (0x4efd_70, ps::Role::OnTextChange),
    ];
    for (addr, role) in expect {
        let f = ps::function_at(addr).expect("地址必须在表里");
        assert_eq!(f.role, role);
        assert!(!f.anchors.is_empty(), "四个方法体都必须有锚");
    }
    // 表按地址升序
    let mut prev = 0;
    for f in ps::FUNCTIONS.iter() {
        assert!(f.x86 > prev, "FUNCTIONS 必须按地址升序");
        prev = f.x86;
    }
}

/// 判据 3：Ghidra 地址 = vaddr + `0x100000`（只加一次）。
#[test]
fn ghidra_bias_is_one_megabyte() {
    assert_eq!(ps::GHIDRA_BIAS, 0x10_0000);
    assert_eq!(ps::ghidra(0x4e54_00), 0x5e54_00);
    assert_eq!(ps::ghidra(0x4efd_70), 0x5efd_70);
}

// ---------------------------------------------------------------------------
// 判据 4：arm64 真名（2.0.60 的 Android 构建没 strip）
// ---------------------------------------------------------------------------

/// 判据 4：4 个方法体在 arm64 `.symtab` 里的真名与真边界。
///
/// 真值：`readelf -sW $ARM | c++filt | grep player_showcase`；
/// 边界是 `st_size`（`draw` 2652 / `ready` 1108 / `process` 656 / `on_text_change` 704）。
#[test]
fn arm64_symbols_give_real_names_and_sizes() {
    let draw = ps::by_role(ps::Role::Draw).unwrap();
    assert_eq!(draw.name, "IControl::draw");
    assert_eq!(draw.arm64, 0x43fe7c);
    assert_eq!(ps::arm_size_of(0x43fe7c), Some(2652));
    assert!(ps::arm_sym(0x43fe7c).unwrap().contains("as IControl>::draw"));

    let on_text = ps::by_role(ps::Role::OnTextChange).unwrap();
    assert_eq!(on_text.name, "on_text_change");
    assert_eq!(on_text.arm64, 0x448d10);
    assert_eq!(on_text.arm_size, 704);
    assert!(ps::arm_sym(0x448d10)
        .unwrap()
        .contains("PlayerShowcase::on_text_change"));

    // x86 → arm64 的配对（每个方法体都能查到）
    assert_eq!(ps::arm_of(0x4e64_80), Some(0x4408d8));
    assert_eq!(ps::arm_of(0x4e68_f0), Some(0x440d2c));
    assert_eq!(ps::arm_of(0x4efd_70), Some(0x448d10));
}

/// 判据 5："虚函数 3 个"有两条互相独立的证据。
///
/// 真值：arm64 `.symtab` 里三个 156 字节的
/// `ImplementsGodotVirtual::__virtual_call::virtual_fn`（**都不引用任何 Location**），
/// 与函数表里 `ready`/`process`/`draw` 三条一致。
#[test]
fn three_virtual_calls_corroborate_three_virtual_methods() {
    assert_eq!(ps::ARM64_VIRTUAL_FN.len(), 3);
    for a in ps::ARM64_VIRTUAL_FN {
        assert!(ps::arm_sym(a).unwrap().contains("__virtual_call"));
    }
    let virtuals = ps::FUNCTIONS
        .iter()
        .filter(|f| {
            matches!(
                f.role,
                ps::Role::Draw | ps::Role::Ready | ps::Role::Process
            )
        })
        .count();
    assert_eq!(virtuals, 3);
}

// ---------------------------------------------------------------------------
// 判据 6：18 个 Location → panic 种类（本章最强的证据）
// ---------------------------------------------------------------------------

/// 判据 6：每个方法体引用的 Location `(line, col)` **逐条**对拍。
///
/// 真值：两侧 `.data.rel.ro` 的 `Location` 结构体（`{file,line,col}`，编译器常量、
/// 与指令集无关）；`(line,col)` 集合两侧完全相同。
#[test]
fn location_sets_are_identical_on_both_architectures() {
    assert_eq!(
        ps::by_role(ps::Role::Draw).unwrap().anchors,
        &[(72, 129), (76, 38), (77, 38), (78, 37), (80, 39), (81, 39), (82, 38)]
    );
    assert_eq!(
        ps::by_role(ps::Role::Ready).unwrap().anchors,
        &[(18, 129), (21, 132), (22, 47), (52, 132)]
    );
    assert_eq!(
        ps::by_role(ps::Role::Process).unwrap().anchors,
        &[(59, 133), (62, 41)]
    );
    assert_eq!(
        ps::by_role(ps::Role::OnTextChange).unwrap().anchors,
        &[(31, 129), (33, 23)]
    );
    // 6 个样板函数：9:10 / 27:1(×4) / 37:1
    assert_eq!(ps::by_role(ps::Role::DeriveInit).unwrap().anchors, &[(9, 10)]);
}

/// 判据 7：Location → panic 种类的对应（决定"这一行在借东西还是在取节点"）。
///
/// 真值：arm64 冷路径里的 `bl` 目标符号 —— `core::option::unwrap_failed` /
/// `core::cell::panic_already_mutably_borrowed` / `core::cell::panic_already_borrowed` /
/// `core::result::unwrap_failed`（消息 47 字节 = `"Upcast failed. …"`）。
#[test]
fn panic_kind_is_read_off_the_called_symbol() {
    let draw = 0x4e54_00;
    let borrows: Vec<(u32, u32)> = ps::borrow_sites_of(draw)
        .map(|a| (a.line, a.col))
        .collect();
    assert_eq!(
        borrows,
        vec![(76, 38), (77, 38), (78, 37), (80, 39), (81, 39), (82, 38)]
    );
    for a in ps::borrow_sites_of(draw) {
        assert_eq!(a.kind, ps::PanicKind::SharedBorrowConflict);
    }

    // 取 root 的那一行是 Option unwrap，不是借用
    let root_unwrap = ps::locations_of(draw)
        .find(|a| a.line == 72)
        .expect("draw 必须有一个 72 行的锚");
    assert_eq!(root_unwrap.kind, ps::PanicKind::OptionUnwrap);
    assert_eq!(root_unwrap.col, 129);

    // on_text_change 的 33:23 是**独占**借用（唯一的一条）
    let exclusive = ps::locations_of(0x4efd_70)
        .find(|a| a.line == 33)
        .expect("on_text_change 必须有一个 33 行的锚");
    assert_eq!(exclusive.kind, ps::PanicKind::MutBorrowConflict);
    assert_eq!(exclusive.col, 23);
    // 独占借用的标记值
    assert_eq!(ps::EXCLUSIVE_BORROW_MARK, -1);

    // 全场只有一处 Result::unwrap(Upcast)，而且它落在 gdext 自己文件里，
    // 所以本文件的 18 个 Location 里**一条都没有**这个种类。
    assert!(ps::LOCATIONS
        .iter()
        .all(|a| a.kind != ps::PanicKind::ResultUnwrapUpcast));
}

/// 判据 8：**列号签名** —— 两组三行的第三行都比同组少 1 列。
///
/// 真值：`Location.col` 的实测值。少 1 列的原因可证：`hat`(3) 比 `skin`/`face`(4)
/// 少一个字符，`get_hat_texture`(15) 比 `get_skin_texture`/`get_face_texture`(16) 少一个。
/// ⇒ "78 行 = 帽子、82 行 = 取帽子贴图"是推出来的。
#[test]
fn hat_lines_are_one_column_shorter() {
    let sig = ps::borrow_column_signature();
    assert_eq!(sig[0], (76, 38));
    assert_eq!(sig[1], (77, 38));
    assert_eq!(sig[2], (78, 37));
    assert_eq!(sig[3], (80, 39));
    assert_eq!(sig[4], (81, 39));
    assert_eq!(sig[5], (82, 38));
    assert_eq!(ps::group_column_shortfall(), (1, 1));

    // 名字长度差正好解释列号差
    assert_eq!(ps::str_len("skin"), ps::str_len("face"));
    assert_eq!(ps::str_len("hat") + 1, ps::str_len("skin"));
    assert_eq!(
        ps::str_len(ps::TextureKind::Hat.getter()) + 1,
        ps::str_len(ps::TextureKind::Skin.getter())
    );
    assert_eq!(
        ps::str_len(ps::TextureKind::Hat.getter()) + 1,
        ps::str_len(ps::TextureKind::Face.getter())
    );
}

// ---------------------------------------------------------------------------
// 判据 9：节点名 / 方法名 / 信号名（(ptr,len) 自校验）
// ---------------------------------------------------------------------------

/// 判据 9：四个名字的 `(ptr, len)` 立即数对**全部自校验**。
///
/// 真值：`.rodata 0x19e490`(16) / `0x1c42ed`(18) / `0x1c42ff`(14) / `0x1c430d`(12)；
/// 后三条首尾相接（地址差 = 前一条长度：18、14）。
#[test]
fn node_and_signal_names_self_check() {
    assert_eq!(ps::NODE_ROOT, "GraphwarRootNode");
    assert_eq!(ps::str_len(ps::NODE_ROOT), ps::NODE_ROOT_LEN);
    assert_eq!(ps::NODE_NAME_EDIT, "PlayerNameLineEdit");
    assert_eq!(ps::str_len(ps::NODE_NAME_EDIT), ps::NODE_NAME_EDIT_LEN);
    assert_eq!(ps::METHOD_ON_TEXT_CHANGE, "on_text_change");
    assert_eq!(
        ps::str_len(ps::METHOD_ON_TEXT_CHANGE),
        ps::METHOD_ON_TEXT_CHANGE_LEN
    );
    assert_eq!(ps::SIGNAL_TEXT_CHANGED, "text_changed");
    assert_eq!(
        ps::str_len(ps::SIGNAL_TEXT_CHANGED),
        ps::SIGNAL_TEXT_CHANGED_LEN
    );

    assert!(ps::node_name_ok(), "四条 (ptr,len) 必须全部自洽");

    // 首尾相接：地址差 == 前一条长度
    let run = ps::LITERAL_RUN;
    assert_eq!(run[1].0 - run[0].0, ps::NODE_NAME_EDIT_LEN as u64);
    assert_eq!(run[2].0 - run[1].0, ps::METHOD_ON_TEXT_CHANGE_LEN as u64);

    // 自校验函数本身要能拒错
    assert!(!ps::literal_pair_ok(0x1c42ed, 17, ps::NODE_NAME_EDIT));
    assert!(ps::literal_pair_ok(0x1c42ed, 18, ps::NODE_NAME_EDIT));
}

/// 判据 10：连线的三元组 = `text_changed` → `on_text_change`。
#[test]
fn ready_connects_text_changed_to_on_text_change() {
    assert_eq!(
        ps::TEXT_CHANGED_LINK,
        ("text_changed", "on_text_change"),
        "信号名 → 自己方法名"
    );
    assert_eq!(ps::READY_PLAN.len(), 6);
    assert_eq!(ps::READY_PLAN[0], ps::ReadyStep::LookupRoot);
    assert_eq!(ps::READY_PLAN[5], ps::ReadyStep::ConnectTextChanged);
    // 名字输入框被取两次（21:132 / 52:132 两个不同的 unwrap）
    let lookups = ps::READY_PLAN
        .iter()
        .filter(|s| {
            matches!(
                s,
                ps::ReadyStep::LookupNameEdit | ps::ReadyStep::LookupNameEditAgain
            )
        })
        .count();
    assert_eq!(lookups, 2);
    let edit_unwraps = ps::locations_of(0x4e64_80)
        .filter(|a| a.line == 21 || a.line == 52)
        .count();
    assert_eq!(edit_unwraps, 2, "两次取节点各有一个 Option unwrap");
    assert_ne!(ps::ready_step_label(ps::ReadyStep::LookupRoot), "");
}

// ---------------------------------------------------------------------------
// 判据 11：它读谁（根节点两个单元 + 档案布局 + String 三字序）
// ---------------------------------------------------------------------------

/// 判据 11：根节点的两个持有单元 `+0x16a0`（贴图缓存）/ `+0x16a8`（玩家档案），
/// 且**写者唯一**（arm64 `GraphwarRootNode::ready`）。
#[test]
fn root_units_are_owned_by_the_root_node() {
    assert_eq!(ps::ROOT_TEXTURE_LOADER, 0x16a0);
    assert_eq!(ps::ROOT_PLAYER_CONFIG, 0x16a8);
    assert_eq!(
        ps::root_unit_writer(0x16a0),
        Some("<GraphwarRootNode as INode>::init")
    );
    assert_eq!(
        ps::root_unit_writer(0x16a8),
        Some("<GraphwarRootNode as INode>::init")
    );
    assert_eq!(ps::root_unit_writer(0x1600), None);
    assert_eq!(ps::ROOT_UNIT_WRITERS.len(), 2);
}

/// 判据 12：`GraphwarPlayerConfig` 的四个字段偏移 + 变更计数器。
///
/// 真值：`name=+0x00`（`on_text_change` 覆盖 `+0x00..0x18`）、
/// `skin=+0x60`（`set_skin` 的 `ldr x1,[x21,#0x60]!`）、`face=+0x78`、`hat=+0x90`、
/// 计数器 `+0xa8`（`+1` 的那条指令读的就是它）。
#[test]
fn player_config_field_offsets() {
    assert_eq!(ps::CFG_NAME, 0x00);
    assert_eq!(ps::CFG_SKIN, 0x60);
    assert_eq!(ps::CFG_FACE, ps::CFG_SKIN + 0x18);
    assert_eq!(ps::CFG_HAT, ps::CFG_FACE + 0x18);
    assert_eq!(ps::CFG_CHANGE_COUNTER, ps::CFG_HAT + ps::StringLayout::SIZE);
    assert_eq!(ps::CFG_LOCK_FLAG, 0xac);

    // 单元 → 载荷差 0x18：draw 读 cell+0x80 == CFG_SKIN 的 ptr 位
    let (ptr, len) = ps::StringLayout::ptr_len(ps::CFG_SKIN);
    assert_eq!(ptr + ps::UNIT_PAYLOAD, 0x80);
    assert_eq!(len + ps::UNIT_PAYLOAD, 0x88);
    assert_eq!(ps::StringLayout::SIZE, 0x18);
    assert_eq!(ps::StringLayout::slots(ps::CFG_NAME), (0x00, 0x08, 0x10));
}

/// 判据 13：`String` 的三字序是 **(cap, ptr, len)** —— 不是常见的 `(ptr, cap, len)`。
///
/// 真值：`on_text_change` 的 `__rust_dealloc(x0=ptr, x1=size, 1)` 实参序；
/// `ready` 把 `+0x08`/`+0x10` 当 `(ptr,len)` 传给 `LineEdit::set_text`。
#[test]
fn string_layout_is_cap_ptr_len() {
    assert_eq!(ps::StringLayout::CAP, 0x00);
    assert_eq!(ps::StringLayout::PTR, 0x08);
    assert_eq!(ps::StringLayout::LEN, 0x10);
    assert!(
        ps::StringLayout::PTR != 0,
        "cap 在前 ⇒ ptr 不在 0 位（照搬常见布局会读错 len）"
    );
    // 三个默认外观名的长度与 new() 里的 mov w0,#0x6/#0xc/#0xe 一致
    assert_eq!(ps::DEFAULT_LENS, (6, 12, 14));
    assert_eq!(ps::DEFAULT_SKIN, "yellow");
    assert_eq!(ps::DEFAULT_FACE, "regular_eyes");
    assert_eq!(ps::DEFAULT_HAT, "soldier_helmet");
}

// ---------------------------------------------------------------------------
// 判据 14：draw 画什么（三层贴图 + 五个实参）
// ---------------------------------------------------------------------------

/// 判据 14：绘制顺序 = 皮肤 → 脸 → 帽子，且每层都是
/// `draw_texture_rect_full(tex, Rect2(0,0,size), tile=false, WHITE, transpose=false)`。
#[test]
fn draw_layers_are_skin_face_hat_over_the_full_rect() {
    assert_eq!(
        ps::DRAW_ORDER,
        [
            ps::TextureKind::Skin,
            ps::TextureKind::Face,
            ps::TextureKind::Hat
        ]
    );
    let plan = ps::draw_plan(120.0, 80.0, [true, true, true]);
    assert_eq!(plan.len(), 3);
    assert_eq!(plan[0].kind, ps::TextureKind::Skin);
    assert_eq!(plan[2].kind, ps::TextureKind::Hat);
    for call in &plan {
        assert_eq!(call.rect, ps::Rect2 { x: 0.0, y: 0.0, w: 120.0, h: 80.0 });
        assert_eq!(call.rect, ps::Rect2::from_size(120.0, 80.0));
        assert!(!call.tile, "tile 实参恒 false");
        assert!(!call.transpose, "transpose 实参恒 false");
        assert_eq!(call.modulate, ps::WHITE);
        assert_eq!(call.modulate, [1.0, 1.0, 1.0, 1.0]);
    }
    assert_eq!(ps::DRAW_COLOR_RODATA, 0x19ed00);
}

/// 判据 15：缺贴图的那层**不画**（`if let Some`），且没有兜底图元。
#[test]
fn missing_textures_are_skipped_not_substituted() {
    assert!(ps::draw_plan(10.0, 10.0, [false, false, false]).is_empty());
    let only_hat = ps::draw_plan(10.0, 10.0, [false, false, true]);
    assert_eq!(only_hat.len(), 1);
    assert_eq!(only_hat[0].kind, ps::TextureKind::Hat);
    let skin_hat = ps::draw_plan(10.0, 10.0, [true, false, true]);
    assert_eq!(
        skin_hat.iter().map(|c| c.kind).collect::<Vec<_>>(),
        vec![ps::TextureKind::Skin, ps::TextureKind::Hat]
    );
    // 三个 getter 就是本模块用到的全部取图方法
    assert_eq!(ps::TEXTURE_GETTERS_USED.len(), 3);
    assert!(ps::TEXTURE_GETTERS_USED.contains(&ps::TextureKind::Skin.getter()));
}

/// 判据 16：三张表（皮肤/脸/帽子）是**三张不同的 HashMap**，间距 `0x30`。
///
/// 真值：arm64 三个 296 字节的 getter 各读一处
/// （`0x40c1cc`→`+0x168`/`+0x170`、`0x40c0a4`→`+0x198`/`+0x1a0`、
/// `0x40bf7c`→`+0x1c8`/`+0x1d0`），间距 `0x30`。
#[test]
fn texture_loader_has_three_separate_maps() {
    assert_eq!(ps::TextureKind::Skin.map_ctrl(), 0x168);
    assert_eq!(ps::TextureKind::Face.map_ctrl(), 0x198);
    assert_eq!(ps::TextureKind::Hat.map_ctrl(), 0x1c8);
    assert_eq!(
        ps::TextureKind::Face.map_ctrl() - ps::TextureKind::Skin.map_ctrl(),
        0x30
    );
    assert_eq!(
        ps::TextureKind::Hat.map_ctrl() - ps::TextureKind::Face.map_ctrl(),
        0x30
    );
    assert_eq!(
        ps::TextureKind::Face.map_hasher() - ps::TextureKind::Skin.map_hasher(),
        0x30
    );
    assert_eq!(ps::texture_kind_of_map(0x198), Some(ps::TextureKind::Face));
    assert_eq!(ps::texture_kind_of_map(0x170), None);

    // 三个 getter 的 arm64 地址各不同（没有 ICF 合并）
    let addrs = [
        ps::TextureKind::Skin.arm_getter(),
        ps::TextureKind::Face.arm_getter(),
        ps::TextureKind::Hat.arm_getter(),
    ];
    assert!(addrs[0] != addrs[1] && addrs[1] != addrs[2] && addrs[0] != addrs[2]);
}

/// 判据 17：**不碰玩家颜色 / 段位 / 队色**（有界否定）。
#[test]
fn no_color_manager_involvement() {
    assert!(!ps::touches_color_manager());
    for callee in ps::CALLEES {
        let lower = callee.to_lowercase();
        assert!(
            !lower.contains("color") && !lower.contains("rank") && !lower.contains("team"),
            "本模块的具名被调者里不该有颜色/段位/队色函数：{callee}"
        );
    }
    // 绘制只有贴图（没有多边形图元）
    for callee in ps::CALLEES {
        assert!(!callee.contains("draw_circle"));
        assert!(!callee.contains("draw_rect_full") || callee.contains("texture_rect"));
        assert!(!callee.contains("draw_line"));
        assert!(!callee.contains("draw_string"));
    }
    assert!(ps::CALLEES.contains(&"CanvasItem::draw_texture_rect_full"));
}

// ---------------------------------------------------------------------------
// 判据 18：process 的闸门与计数器
// ---------------------------------------------------------------------------

/// 判据 18：`process` = 可见性闸门 + 计数器轮询。
///
/// 真值：`is_visible_in_tree()` 的 `cbz` 早退；`[self+0x18]` 与档案 `+0xa8` 的 `cmp`；
/// **先** `queue_redraw()` 再写回 `[self+0x18]`。
#[test]
fn process_polls_the_change_counter_behind_a_visibility_gate() {
    assert_eq!(ps::Showcase::LAST_COUNTER_OFFSET, 0x18);
    let mut s = ps::Showcase::new();
    assert_eq!(s.last_counter, 0);

    // 不可见 ⇒ 直接返回，连计数器都不看
    assert_eq!(s.tick(false, 7), ps::ProcessOutcome::NotVisible);
    assert_eq!(s.last_counter, 0, "不可见时不该更新计数器");

    // 可见 + 未变 ⇒ 什么都不做
    assert_eq!(s.tick(true, 0), ps::ProcessOutcome::Unchanged);

    // 可见 + 变了 ⇒ 要求重画，并记住新值
    assert_eq!(s.tick(true, 1), ps::ProcessOutcome::QueueRedraw);
    assert_eq!(s.last_counter, 1);
    assert_eq!(s.tick(true, 1), ps::ProcessOutcome::Unchanged);
    assert_eq!(s.tick(true, 2), ps::ProcessOutcome::QueueRedraw);

    assert!(ps::Showcase::gate(true));
    assert!(!ps::Showcase::gate(false));
    assert_eq!(ps::Showcase::default(), ps::Showcase::new());
}

// ---------------------------------------------------------------------------
// 判据 19：on_text_change（写回 + 存档 + 计数 +1）
// ---------------------------------------------------------------------------

/// 判据 19：`on_text_change` 写名字 → `save()` → 计数器 `+1`（顺序是先存档再加计数）。
#[test]
fn on_text_change_writes_saves_and_bumps_the_counter() {
    let e = ps::on_text_change_effect("新名字");
    assert_eq!(e.new_name, "新名字");
    assert!(e.saved, "必须调 GraphwarPlayerConfig::save()");
    assert_eq!(e.counter_delta, 1);
    // 空名字也是合法的（原版没有长度校验）
    assert_eq!(ps::on_text_change_effect("").new_name, "");
}

// ---------------------------------------------------------------------------
// 判据 20：name_to_addr.tsv 的 3 条非唯一锚都登记了处置
// ---------------------------------------------------------------------------

/// 判据 20：TSV 的 `未知`/`歧义` 行没有被静默丢掉。
///
/// 真值：`__default_virtual_call` / `class_id` 无 Location（`未知`）；
/// `__register_constants` 的 4 个候选全是真函数（`歧义`）。
#[test]
fn name_bridge_conflicts_are_registered() {
    assert_eq!(ps::NAME_BRIDGE_CONFLICTS.len(), 3);
    let unknown = ps::conflict("__default_virtual_call").unwrap();
    assert_eq!(unknown.1, "未知");
    assert!(unknown.2.contains("Location"));

    let class_id = ps::conflict("class_id").unwrap();
    assert_eq!(class_id.1, "未知");

    let rc = ps::conflict("__register_constants").unwrap();
    assert_eq!(rc.1, "歧义");
    for cand in [0x4e1160u64, 0x4e12e0, 0x4e87d0, 0x4e8970] {
        assert!(ps::function_at(cand).is_some(), "4 个候选都必须是真函数");
    }
    // 本节采用的定位
    assert_eq!(
        ps::function_at(0x4e12e0).unwrap().role,
        ps::Role::RegisterFns
    );
    assert!(ps::conflict("draw").is_none(), "唯一锚不该出现在冲突表里");
}

/// 判据 21：未解出的东西**照实登记**（6 条，每条都写了缺什么）。
#[test]
fn unknowns_are_recorded_with_missing_evidence() {
    assert_eq!(ps::UNKNOWNS.len(), 6);
    assert!(ps::UNKNOWNS.iter().any(|u| u.contains("class_id")));
    assert!(ps::UNKNOWNS.iter().any(|u| u.contains("取两次")));
    assert!(ps::UNKNOWNS.iter().any(|u| u.contains("借用")));
    assert_eq!(ps::ARM64_CLASS_ID, 0x438088);
    assert_eq!(ps::ARM64_CLASS_ID_SLOT, 0x8b40b0);
    assert_eq!(ps::DOC_SECTIONS.len(), 3);
    assert!(ps::REPRO.contains("objdump"));
    assert_eq!(ps::SOURCE_FILE, "src/player_showcase.rs");
    assert_eq!(ps::CLASS_NAME, "PlayerShowcase");
}

/// 判据 22：`draw` 的真实字节数与 Ghidra 跨度**不同**（尾部 240 字节是外置冷路径）。
///
/// 真值：收尾 `ret` 在 `0x4e60be`（`0x4e5400 + 0xcc0 - 1`）；Ghidra 因为
/// 后面没有新起点，把 `0x4e60bf..0x4e6480` 的冷路径也算进了跨度。
#[test]
fn draw_body_ends_before_the_outlined_cold_paths() {
    let draw = ps::by_role(ps::Role::Draw).unwrap();
    assert_eq!(draw.body, Some(0xcc0));
    assert_eq!(draw.effective_len(), 0xcc0);
    assert!(draw.span > draw.body.unwrap(), "Ghidra 跨度包含外置冷路径");
    assert_eq!(draw.x86 + 0xcc0, 0x4e60_c0, "ret 后第一条冷路径指令");
    // 未逐指令定界的函数用 Ghidra 跨度
    let ready = ps::by_role(ps::Role::Ready).unwrap();
    assert_eq!(ready.body, None);
    assert_eq!(ready.effective_len(), ready.span);
    assert_eq!(ready.span, 1136);
}
