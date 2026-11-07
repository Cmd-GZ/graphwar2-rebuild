//! `fire` 驱动的**判据**：按**批内发数**选结算路径（`docs/06-verification.md` §39）。
//!
//! 为什么要有这条测试：`fire` 轴的口径在 2026-11-08 变了 —— 一个结算批里有 ≥2 发时，
//! 不再逐发调 `Match::fire`，而是一次 `Match::fire_batch`（一个批 = 一次结算 = 一次广播，
//! `docs/141` §3）。语料侧的护栏（`fixtures/guardrails.json` + `--check-baseline`）只在
//! **有定格快照的机器**上才有意义；本文件自带合成回放，没有语料也能跑，专门锁住那个分支。
//!
//! 断言只用一条可数、与语料无关的性质：
//! **一个结算批里 `GameStateInfo` 必须是 3 条**（开始 / 同步 / 执行）。
//! 逐发驱动会给出 `3×N` 条 —— 那正是 `S` 桶（126 批）与 `SD` 桶（4 批）的成因。
//!
//! 最后一条测试是本文件的重点：**判据是"批内发数"，不是"文件里有没有多发射击"**。
//! 同一份回放里单发批与多发批混排时，前面的单发批必须**一点不受影响**。

use gw2_rebuild::_scaffold::replaydiff::{split_batches, Msg, Policy, Predictor, Replay};

/// 回放的一行：`{"event":<kind_body>,"tick":<tick>}`。
///
/// 刻意用拼接而不是 `format!`：JSON 的 `{}` 与格式串的 `{}` 混在一起时，
/// 手数括号层数非常容易多一层（第一版就多写了一层，直接编译不过）。
fn line(kind_body: &str, tick: i64) -> String {
    format!("{{\"event\":{kind_body},\"tick\":{tick}}}")
}

/// 造一份最小可跑的合成回放，`sizes[i]` = 第 `i` 个结算批里有几发。
///
/// 站位固定为三个（射手 1、射手 2、远处的敌方 3），`Predictor::new` 只要见到 `PosInfo`
/// 就能建出局面；函数用 `"x"`（`y = x`），弹道必然有终点，不会退化。
fn replay_with_batch_sizes(sizes: &[usize]) -> Replay {
    let mut lines: Vec<String> = Vec::new();
    for (id, x, y) in [(1i64, -10.0f64, 0.0f64), (2, -10.0, 5.0), (3, 10.0, 0.0)] {
        lines.push(line(
            &format!(r#"{{"PosInfo":{{"entity_id":{id},"pos_x":{x},"pos_y":{y}}}}}"#),
            1,
        ));
    }
    for (bi, &n) in sizes.iter().enumerate() {
        let tick = 1000 + bi as i64 * 100;
        lines.push(line(
            r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#,
            tick,
        ));
        for i in 0..n {
            // 轮流由两个射手提交，保证多发批里 owner 不重复得太无聊
            let owner = (i as i64 % 2) + 1;
            let y0 = if owner == 1 { 0.0 } else { 5.0 };
            lines.push(line(
                &format!(
                    r#"{{"FunctionInfo":{{"owner_id":{owner},"function":"x","corrected_function":"x"}}}}"#
                ),
                tick,
            ));
            lines.push(line(
                &format!(
                    r#"{{"FunctionPoints":{{"owner_id":{owner},"start_x":-10.0,"start_y":{y0},"end_x":-5.0,"end_y":{y0}}}}}"#
                ),
                tick,
            ));
        }
        lines.push(line(
            r#"{"GameStateInfo":{"game_state":"SynchronizingResults"}}"#,
            tick,
        ));
        lines.push(line(
            r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#,
            tick,
        ));
    }
    let msgs: Vec<Msg> = lines
        .iter()
        .map(|l| Msg::parse_line(l).expect("合成回放的行必须是合法 JSON"))
        .collect();
    Replay { msgs, plain_bytes: 0, gzipped: false }
}

/// 逐批驱动一遍，返回 `(批内发数, 我方产出的 GameStateInfo 条数)`。
fn run(sizes: &[usize]) -> Vec<(usize, usize)> {
    let r = replay_with_batch_sizes(sizes);
    let batches = split_batches(&r);
    assert_eq!(batches.len(), sizes.len(), "切批数应当等于合成批数");
    let mut p = Predictor::new(&r, batches[0].start, Policy::default()).expect("建不出局面");
    batches
        .iter()
        .map(|b| {
            let out = p.predict(b, r.msgs[b.start].tick);
            let gs = out.msgs.iter().filter(|m| m.kind == "GameStateInfo").count();
            (b.shots.len(), gs)
        })
        .collect()
}

#[test]
fn single_shot_batch_has_exactly_three_game_states() {
    // 单发批：`fire` 与 `fire_batch(&[f])` 是同一件事，口径不变
    assert_eq!(run(&[1]), vec![(1, 3)]);
}

#[test]
fn multi_shot_batch_is_one_broadcast_not_n_windows() {
    // 2 发一个批 ⇒ 3 条，不是 6 条。逐发驱动会给 3×2 = 6。
    assert_eq!(run(&[2]), vec![(2, 3)]);
    // 3 发一个批 ⇒ 仍然是 3 条（旧驱动会给 9）
    assert_eq!(run(&[3]), vec![(3, 3)]);
    assert_eq!(run(&[5]), vec![(5, 3)]);
}

#[test]
fn multi_shot_batch_keeps_the_grouped_prefix() {
    // 结构不止"条数"：前缀 `FunctionInfo`/`FunctionPoints` 按类型连排、各自 N 条
    // （`docs/141` §3 ①）。逐发驱动会把它们拆成 FInfo,FPoints,FInfo,FPoints…
    let r = replay_with_batch_sizes(&[3]);
    let batches = split_batches(&r);
    let mut p = Predictor::new(&r, batches[0].start, Policy::default()).expect("建不出局面");
    let out = p.predict(&batches[0], r.msgs[batches[0].start].tick);
    let kinds: Vec<&str> = out.msgs.iter().map(|m| m.kind.as_str()).collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "FunctionInfo").count(),
        3,
        "三发就该有三条 FunctionInfo"
    );
    assert_eq!(kinds.iter().filter(|k| **k == "FunctionPoints").count(), 3);
    let first_fi = kinds.iter().position(|k| *k == "FunctionInfo").unwrap();
    let last_fi = kinds.iter().rposition(|k| *k == "FunctionInfo").unwrap();
    assert_eq!(last_fi - first_fi + 1, 3, "FunctionInfo 必须连排（不能被 FunctionPoints 打断）");
    assert!(
        kinds[last_fi + 1..].iter().take(3).all(|k| *k == "FunctionPoints"),
        "FunctionPoints 必须紧跟在 FunctionInfo 块之后连排"
    );
}

#[test]
fn criterion_is_the_batch_not_the_file() {
    // ★ 本文件最重要的一条：同一份回放里混排。
    // 判据取"文件里有没有多发射击"的实现，会把第 0、2 个单发批也改口径；
    // 取"批内发数"的实现，每个批都应当是干净的 3 条。
    assert_eq!(run(&[1, 2, 1]), vec![(1, 3), (2, 3), (1, 3)]);
    assert_eq!(run(&[2, 1]), vec![(2, 3), (1, 3)]);
    assert_eq!(run(&[1, 4, 1, 2]), vec![(1, 3), (4, 3), (1, 3), (2, 3)]);
}
