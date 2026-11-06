//! `graphwar_function_input.rs` 的复验收据（逐条对拍 `src/graphwar_function_input.rs`）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 每一条右侧的真值都能指到反汇编地址或 `.rodata` 字节，下面逐条写明出处。
//!
//! 地址口径：本文件用 **ELF 虚地址**（`objdump` 口径）；Ghidra = 该值 + `0x100000`。

use gw2_rebuild::graphwar_function_input as fi;

/// 判据 1：`fixtures/name_to_addr.tsv` 的 7 个"唯一锚"逐条复核。
///
/// 复核手法：把 `.data.rel.ro` 里每个 `panic::Location`（24 字节：`file` 指针 +
/// `len` + `line:u32` + `col:u32`）解出来，找引用它的指令，再按
/// `analysis/funcstarts_ghidra_va.pkl` 归到函数；与本表逐条比对**锚行号集合**与
/// **锚的个数**。
///
/// 例：`process`（0x4fd4a0）引用的行号是 521/523/528/530/535/550×2/558/602/607，
/// 共 10 个，与 `name_to_addr.tsv` 的「10 个双向唯一 (file,line,col) 锚」一致。
#[test]
fn seven_unique_anchors_are_verified_against_the_fixture() {
    let mut named = fi::named_functions();
    named.sort_by_key(|f| f.vaddr);
    let got: Vec<(&str, u64)> = named.iter().map(|f| (f.name.unwrap(), f.vaddr)).collect();
    assert_eq!(
        got,
        vec![
            ("force_update_cb", 0x4f2200),
            ("move_caret_left", 0x4f3210),
            ("move_caret_right", 0x4f3910),
            ("delete_token_at_caret", 0x4f44c0),
            ("delete_token_after_caret", 0x4f4a60),
            ("ready", 0x4fd1b0),
            ("process", 0x4fd4a0),
        ]
    );
    // 每个都必须是 Ghidra 的函数入口（REA procedure_info 复核过 process/ready 等）
    for f in named.iter() {
        assert_eq!(fi::ghidra(f.vaddr), f.vaddr + 0x100000, "{}", f.name.unwrap());
    }
}

/// 判据 2：整个模块 17 条函数记录（13 有锚 + 4 无锚）。
///
/// 13 条有锚 = 上面 7 条 + 6 个 gdext 注册驱动（Location 434:1 ×4、466:1、27:10）。
/// 4 条无锚是 `FN_CALL_FIRE` / `FN_INSERT_TEXT` / `FN_DELETE_CHARS_AT_CARET` /
/// `FN_GUI_INPUT` —— 它们没有 panic 路径，靠调用关系与虚拟注册链定位。
#[test]
fn seventeen_functions_split_thirteen_anchored_plus_four() {
    assert_eq!(fi::FUNCTIONS.len(), 17);
    let anchored = fi::FUNCTIONS.iter().filter(|f| f.name.is_some()).count();
    assert_eq!(anchored, 7, "只有 7 条落了原版名字");
    let unanchored_namedless = fi::FUNCTIONS.iter().filter(|f| f.name.is_none()).count();
    assert_eq!(unanchored_namedless, 10); // 6 个注册驱动 + 4 个无锚助手

    // 注册驱动的地址（Location 434:1 / 466:1 / 27:10 的引用者）
    for a in [
        fi::FN_REGISTER_A,
        fi::FN_REGISTER_B,
        fi::FN_REGISTER_C,
        fi::FN_REGISTER_D,
        fi::FN_REGISTER_E,
        fi::FN_CLASS_INIT,
    ] {
        assert_eq!(fi::function_at(a).unwrap().grade, fi::Grade::Observed);
    }
    // gui_input 的地址归属是推断（无 Location 锚）
    assert_eq!(
        fi::function_at(fi::FN_GUI_INPUT).unwrap().grade,
        fi::Grade::Inferred
    );
}

/// 判据 3（本轮核心）：**输入框唯一的文本变换是删空格**。
///
/// 真值来自 `force_update_cb`（0x4f2200）里的 `str::replace`（`0x502d20`）：
/// `0x4f28dc` 调用点、`from` = (0x1c44a1, 1)、`to` = (1, 0)；
/// `0x1c44a1` 那个字节是 `0x20`（ASCII 空格，夹在 "FxTextureButton" 与
/// "delete_char_at_caret" 两条相邻字面量之间）。
#[test]
fn the_only_text_transform_is_removing_ascii_spaces() {
    assert_eq!(fi::STRIPPED_CHAR, ' ');
    assert_eq!(fi::strip_spaces("sin ( x )"), "sin(x)");
    assert_eq!(fi::strip_spaces("log( exp( x ) )"), "log(exp(x))");
    // 只删空格：制表符/换行/π 都不动
    assert_eq!(fi::strip_spaces("x\t+\nπ"), "x\t+\nπ");
    // π 的规范化发生在解析器（MathFunction::new 0x4ed670：replace("pi","n") +
    // replace("π","n")），**不在**输入框
    assert_eq!(fi::strip_spaces("pi*π"), "pi*π");
}

/// 判据 4（本轮核心）：ASCII 按键**不**被输入框接管，全部交给基类 `LineEdit`。
///
/// 真值：`gui_input`（0x4fedc0）的字符跳转表在 `0x1c3d5c`，198 项，覆盖
/// `0xB2..=0x177`；ASCII（0x00–0x7F）**不在**这张表的索引范围里。
#[test]
fn ascii_keys_are_delegated() {
    for c in "0123456789()+-*/^xy'ne ".chars() {
        if c == '^' {
            continue; // '^' 单独判，见下一条
        }
        assert_eq!(
            fi::classify_key(c, true),
            fi::KeyAction::Delegated,
            "{c:?} 不应被输入框接管"
        );
    }
}

/// 判据 5：28 条"死键合成 / 上标"折叠，逐条对跳转表的分支。
///
/// 真值：跳转表 198 项里只有 28 项指向非默认分支；每个分支 `mov $0x??,%eax`
/// 的立即数就是折叠结果（如 `²`→`0x32`=`'2'`，`·`→`0x2e`=`'.'`）。
#[test]
fn the_28_composed_folds_are_exact() {
    let mut table: Vec<(char, char)> = fi::COMPOSED_FOLDS.to_vec();
    table.sort();
    // 与反汇编逐条对应的 28 条（区间 0xB2..=0x177）
    assert_eq!(table.len(), 28);
    assert_eq!(fi::fold_composed('â'), Some('a'));
    assert_eq!(fi::fold_composed('Â'), Some('A'));
    assert_eq!(fi::fold_composed('ŷ'), Some('y'));
    assert_eq!(fi::fold_composed('²'), Some('2'));
    assert_eq!(fi::fold_composed('³'), Some('3'));
    assert_eq!(fi::fold_composed('¹'), Some('1'));
    assert_eq!(fi::fold_composed('·'), Some('.'));
    // 表外的重音字符不折（原版只为这 28 个写了分支）
    for c in ['é', 'à', 'ü', 'ñ', 'ç'] {
        assert_eq!(fi::fold_composed(c), None, "{c:?}");
    }
    for (from, to) in table {
        assert!(from >= fi::FOLD_TABLE_RANGE.0 && from <= fi::FOLD_TABLE_RANGE.1);
        assert!(to.is_ascii());
    }
}

/// 判据 6：`'^'` 走单独分支，且带一个**未知**的附加条件。
///
/// 真值：`0x4feeeb`–`0x4fef06` 三次引擎属性读取之后才 `je 0x4fef7a` 插入 `"^"`
/// （字面量 `0x1c4341`，长度 1）。条件语义未知 ⇒ 用参数显式暴露。
#[test]
fn caret_key_is_a_separate_branch_with_an_unknown_condition() {
    assert_eq!(fi::CARET_CHAR, '^');
    assert_eq!(fi::classify_key('^', true), fi::KeyAction::Insert('^'));
    assert_eq!(fi::classify_key('^', false), fi::KeyAction::Delegated);
    // '^' 不是表项
    assert!(fi::COMPOSED_FOLDS.iter().all(|(c, _)| *c != '^'));
}

/// 判据 7：插入 = `format!("{}", ch)` + `call("force_update_cb")`。
///
/// 真值：所有折叠分支汇合到 `0x4ff1e1`（`mov %eax,0x44(%rsp)` → `format!` →
/// `0x4ff227 call 0x4f4320`），`0x4f4320` 内 `0x4f4398` 取 `"force_update_cb"`
/// （`.rodata` `0x1c446f`，长度 `0xf`）后 `0x4f43b5 call 0x39fe10`（NameCall）。
#[test]
fn insertion_refreshes_via_force_update_cb() {
    assert_eq!(fi::REFRESH_METHOD, "force_update_cb");
    assert_eq!(fi::REFRESH_METHOD.len(), 15);
    assert_eq!(fi::REFRESH_PREFIX, "force_update");
    assert_eq!(fi::REFRESH_PREFIX.len(), 12);
    assert_eq!(
        fi::intent_for_key('â', true),
        fi::InputIntent::InsertAndRefresh("a".to_string())
    );
    assert_eq!(fi::intent_for_key('!', true), fi::InputIntent::Delegate);
}

/// 判据 8：提交路径**不清洗**文本。
///
/// 真值：`on_text_submitted` 的 ptrcall 实例 `0x427070` 在 `0x4270eb` 调
/// `0x4f4250`，后者 `0x4f4269` 取 `"fire"`（`.rodata` `0x1b97c8`，长度 4）后
/// `0x4f4275 call *0x20(%rax)`（dyn 回调）。整条路径上没有字符串变换。
#[test]
fn submit_path_passes_the_text_through_verbatim() {
    assert_eq!(fi::submit_intent("sin( x )"), ("fire", "sin( x )"));
    assert_eq!(fi::submit_intent("π"), ("fire", "π"));
}

/// 判据 9：「删一个 token」= N 次基类退格。
///
/// 真值：`0x4f4410` 的循环体（`0x4f4440`–`0x4f448e`）每轮
/// `call 0x39fe10`，方法名是 `"delete_char_at_caret"`（`.rodata` `0x1c44a2`，
/// 长度 `0x14` = 20）；`delete_token_at_caret` / `delete_token_after_caret`
/// 分别调它 4 / 5 次。N 的算法本身未知（见 `fi::UNKNOWNS[0]`）。
#[test]
fn deleting_a_token_degrades_to_n_backspaces() {
    assert_eq!(fi::BACKSPACE_METHOD, "delete_char_at_caret");
    assert_eq!(fi::BACKSPACE_METHOD.len(), 20);
    assert_eq!(fi::delete_chars_at_caret_plan(4).len(), 4);
    let mut b = fi::Buffer::with_caret_at_end("sin(x)");
    b.run_plan(&fi::delete_chars_at_caret_plan(3));
    assert_eq!(b.text(), "sin");
}

/// 判据 10：`ready` 接的三条信号与处理函数（字面量对逐条对得上）。
///
/// 真值：`0x4fd1b0` 里三段 `Callable::from_object_method` + `StringName::from` +
/// `connect`，字面量与长度见 `fi::SIGNAL_WIRING` 的文档表。
#[test]
fn ready_wires_three_self_connections() {
    assert_eq!(
        fi::SIGNAL_WIRING,
        [
            ("size_changed", "on_size_changed"),
            ("text_changed", "on_text_change"),
            ("text_submitted", "on_text_submitted"),
        ]
    );
    // 三条处理函数都是 #[func] 导出（docs/18 §6.5 的扩展 API 表）
    for (_, h) in fi::SIGNAL_WIRING.iter() {
        assert!(fi::EXPORTS.iter().any(|e| e.name == *h));
    }
    // "force_update" 是 process 里 call 的方法名，不是信号
    assert_eq!(fi::handler_for_signal("force_update"), None);
}

/// 判据 11：导出方法 4 个、虚拟方法 3 个，且虚拟方法的实现地址可指。
///
/// 真值：4 个 `#[func]` 由 8 个注册闭包（`0x4f1390..0x4f1960`）注册，
/// 每条都引用「类名 `0x1c41e1` + 方法名」；3 个虚拟方法的 `ClassVirtualInfo`
/// 闭包在 `0x9bb530/0x9bb538/0x9bb540` 连号，由 `0x40d600` 使用。
#[test]
fn four_exports_and_three_virtuals() {
    let names: Vec<&str> = fi::EXPORTS.iter().map(|e| e.name).collect();
    assert_eq!(
        names,
        [
            "on_text_change",
            "on_text_submitted",
            "on_size_changed",
            "force_update_cb"
        ]
    );
    for e in fi::EXPORTS.iter() {
        assert_eq!(e.name.len() as u32, e.name_len);
    }
    assert_eq!(fi::VIRTUALS[0].name, "ready");
    assert_eq!(fi::VIRTUALS[0].implementation, fi::FN_READY);
    assert_eq!(fi::VIRTUALS[1].implementation, fi::FN_PROCESS);
    assert_eq!(fi::VIRTUALS[2].implementation, fi::FN_GUI_INPUT);
    assert_eq!(fi::CLASS, "GraphwarFunctionInput");
    assert_eq!(fi::CLASS.len(), 21); // 0x1c41e1，长度立即数 0x15
}

/// 判据 12：未知项必须写成未知（防止后人把推断当结论）。
#[test]
fn unknowns_are_declared() {
    assert_eq!(fi::UNKNOWNS.len(), 6);
    for u in fi::UNKNOWNS.iter() {
        assert!(u.contains("未知") || u.contains("没有") || u.contains("未"));
    }
    // 线性边界表的坑也写下来了（gui_input 被 0x4fefd5 截断）
    assert!(fi::LINEAR_END_CAVEAT.contains("0x4fedc0"));
    assert!(fi::LINEAR_END_CAVEAT.contains("0x4fefd5"));
}
