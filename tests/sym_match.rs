//! `fixtures/sym_match.tsv` 的一致性断言（`tools/sym_match.py build` 的产物）。
//!
//! ⚠️ 按 `AGENTS.md` §0.2 / §0.5：**这些断言是回归保护，不是复原证据。**
//! 它们只保证"夹具没被手改坏、内部自洽、几个已知点没跑偏"；
//! 匹配方法本身对不对，只由 `tools/sym_match.py --verify` 对着
//! `fixtures/name_to_addr.tsv` 的 326 行唯一锚判定。
//!
//! 地址口径：本文件全部是 **ELF 虚地址**（Ghidra/REA = +0x100000）。

const TSV: &str = include_str!("../fixtures/sym_match.tsv");

struct Row<'a> {
    name: &'a str,
    addr: u64,
    size: u64,
    cands: Vec<u64>,
    conf: &'a str,
}

fn rows() -> Vec<Row<'static>> {
    let mut out = Vec::new();
    for line in TSV.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&'static str> = line.split('\t').collect();
        if f[0] == "arm64_name" {
            continue; // 表头
        }
        assert_eq!(f.len(), 6, "每行必须 6 列：{line}");
        out.push(Row {
            name: f[0],
            addr: u64::from_str_radix(f[1].trim_start_matches("0x"), 16).expect("arm64_addr"),
            size: f[2].parse().expect("arm64_size"),
            cands: f[3]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("候选地址"))
                .collect(),
            conf: f[4],
        });
    }
    out
}

/// 判据 1：表是 1,300 行，正好是 arm64 2.0.60 里名字含 `graphwar` 的 FUNC 符号数。
#[test]
fn fixture_has_1300_rows() {
    let r = rows();
    assert_eq!(r.len(), 1300, "arm64 名字含 graphwar 的已定义 FUNC 符号");
}

/// 判据 2：`confidence` 只会是这 6 个值，且四类计数加得回 1,300。
#[test]
fn confidence_values_and_totals() {
    let r = rows();
    for x in &r {
        assert!(
            matches!(x.conf, "unique" | "weak" | "multi" | "none" | "split" | "merge"),
            "未知 confidence：{}（{}）",
            x.conf,
            x.name
        );
        match x.conf {
            "none" => assert!(x.cands.is_empty(), "none 不该有候选：{}", x.name),
            "multi" => assert!(x.cands.len() >= 2, "multi 至少 2 个候选：{}", x.name),
            _ => assert_eq!(x.cands.len(), 1, "唯一类结论必须恰好 1 个候选：{}", x.name),
        }
    }
    let n = |c: &str| r.iter().filter(|x| x.conf == c).count();
    assert_eq!(
        n("unique") + n("weak") + n("multi") + n("none") + n("split") + n("merge"),
        1300
    );
}

/// 判据 3：**一对一结论之间必须是单射**——两个不同的 arm64 函数不能把同一个 x86 地址
/// 说成各自唯一的答案。
///
/// 这条是"绝不允许猜"的可执行形式：如果某处实现了"多候选里挑一个"，这里会立刻撞地址。
/// `merge` 行**故意**排除：它的语义就是"N 个 arm64 函数 ↔ 1 个 x86 函数"（那个 x86 函数把
/// 它们内联掉了），所以它们本来就该指向同一个地址——但那个地址必须**恰好有一个**非 merge
/// 的 arm64 函数作为主配对。
#[test]
fn unique_answers_are_injective() {
    let r = rows();
    let mut owner: std::collections::HashMap<u64, (u64, &str)> = std::collections::HashMap::new();
    for x in &r {
        if matches!(x.conf, "none" | "multi" | "merge") {
            continue;
        }
        for c in &x.cands {
            // 同一个 arm64 地址上可能挂着多个符号名（arm64 侧也被 ICF 合并了），
            // 那是同一份代码，不算撞车。
            if owner.get(c).map(|p| p.0) == Some(x.addr) {
                continue;
            }
            if let Some(prev) = owner.insert(*c, (x.addr, x.name)) {
                panic!(
                    "x86 0x{c:x} 被两个 arm64 函数同时认领：0x{:x} {}\n  vs 0x{:x} {}",
                    prev.0, prev.1, x.addr, x.name
                );
            }
        }
    }
    // merge 的母函数必须落在 x86 的 `.text` 里（0x37b940..0x94eb84）。
    // 更严的"母函数必须有一个非 merge 的主配对"做不到：母函数可能是 std/core 函数，
    // 不在本夹具的 1,300 行里。
    for x in &r {
        if x.conf != "merge" {
            continue;
        }
        let home = x.cands[0];
        assert!(
            (0x37b940..0x94eb84).contains(&home),
            "merge 行的母函数 0x{home:x} 不在 x86 .text 里（{}）",
            x.name
        );
    }
}

/// 判据 4：已知锚点。这三条在 `fixtures/name_to_addr.tsv` 里有 Location 唯一锚，
/// 本夹具是**独立**（不读 Location、不读该文件）对出来的，必须同址。
#[test]
fn known_anchors_agree() {
    let r = rows();
    let find = |needle: &str| -> &Row<'_> {
        let mut hit = r.iter().filter(|x| x.name.ends_with(needle));
        let one = hit.next().unwrap_or_else(|| panic!("fixture 里没有 {needle}"));
        assert!(hit.next().is_none(), "{needle} 在 fixture 里不唯一");
        one
    };
    // calculate_paths：arm64 0x3dca1c（4,812 B）↔ x86 0x467530（真值 9 个 Location 锚）
    let cp = find("GraphwarPathCalculator::calculate_paths");
    assert_eq!(cp.addr, 0x3dca1c);
    assert_eq!(cp.size, 4812);
    assert_eq!(cp.cands, vec![0x467530], "calculate_paths 的 x86 地址");

    // calculate_shot_results：真值 0x3c2bc0
    let csr = find("GraphwarShotCalculator::calculate_shot_results");
    assert_eq!(csr.cands, vec![0x3c2bc0]);

    // 采样器本体（calculate_function）：两侧都留成独立函数 ⇒ 一对一，不是"一拆多"
    let cf = find("GraphwarShotCalculator::calculate_function");
    assert_eq!(cf.cands, vec![0x3c2530], "采样器 0x3c2530");
}

/// 判据 5：`calculate_paths` **不是** `split`——它在两侧都是一对一。
///
/// 这条断言是专门为"任务书里的校准例子被实测推翻"写的：README / docs 曾按
/// "arm64 把采样器内联进 calculate_paths" 描述，§35.6 用两条独立证据否掉了它。
#[test]
fn calculate_paths_is_not_a_split() {
    let r = rows();
    let cp = r
        .iter()
        .find(|x| x.name.ends_with("GraphwarPathCalculator::calculate_paths"))
        .expect("calculate_paths");
    assert_ne!(cp.conf, "split");
    assert_eq!(cp.cands.len(), 1);
}
