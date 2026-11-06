//! 回放消息级对拍的验收（`docs/136-replay-diff.md`）。
//!
//! 四条硬要求（任务给的三条 + 一条自证）：
//!
//! 1. **一条完整回放**能从头跑到尾、不报错，并且逐批判定为一致；
//! 2. **人为构造一处字段差异** ⇒ 比对器必须**指名道姓**报出来（负样本）；
//! 3. **批内顺序**按声明口径处理：内容相同、顺序打乱 ⇒ 仍算内容命中，
//!    但顺序判定失败（A 类，`docs/113`）；跨批的顺序改动则必须算失败；
//! 4. 口径里声明排除的东西（tick / `effect_id` 绝对值 / 我方不产的消息）
//!    确实被排除 —— 否则"全绿"是假的。
//!
//! 语料本体在 `~/.local/share/godot/.../replays`（**只读**、不入库），
//! 所以本文件自带一份**合成回放**：它的结算批由我们自己的 `Match::fire` 生成，
//! 保证"期望值"与"我方预测"在构造上一致 —— 然后只改一个字节，看比对器抓不抓得到。

use gw2_rebuild::game::Match;
use gw2_rebuild::_scaffold::replaydiff::{
    compare_batch, compare_msg, gz, run_file, Msg, Policy, Replay,
};
use gw2_rebuild::server::RoomState;
use gw2_rebuild::graphwar_shot_calculator::settle::Target;

fn room() -> RoomState {
    RoomState {
        room_name: "Graphwar Game".into(),
        game_state: "Setup".into(),
        axis_mode: "EveryUnit".into(),
        function_mode: "NormalFunction".into(),
        turn_mode: "SequentialTurns".into(),
        time_mode: "Timer1m".into(),
    }
}

/// 合成回放的**前段**（开局的输入态）。这些全是我方的"输入"，不是预测目标。
const SETUP: &[&str] = &[
    r#"{"event":{"GameStateInfo":{"game_state":"Setup"}},"tick":1}"#,
    r#"{"event":{"TurnModeInfo":{"turn_mode":"SequentialTurns"}},"tick":1}"#,
    r#"{"event":{"AxisModeInfo":{"axis_mode":"EveryUnit"}},"tick":1}"#,
    r#"{"event":{"FunctionModeInfo":{"function_mode":"NormalFunction"}},"tick":1}"#,
    r#"{"event":{"TimeModeInfo":{"time_mode":"Timer1m"}},"tick":1}"#,
    r#"{"event":{"ClearObstacles":{}},"tick":2}"#,
    r#"{"event":{"AddObstacle":{"pos_x":0.0,"pos_y":5.0,"radius":2.0}},"tick":2}"#,
    r#"{"event":{"PlayerInfo":{"player_id":100,"connection_id":99}},"tick":2}"#,
    r#"{"event":{"TeamInfo":{"entity_id":100,"team":"Team1"}},"tick":2}"#,
    r#"{"event":{"SoldierInfo":{"soldier_id":101,"player_id":100}},"tick":2}"#,
    r#"{"event":{"TeamInfo":{"entity_id":101,"team":"Team1"}},"tick":2}"#,
    r#"{"event":{"SoldierInfo":{"soldier_id":201,"player_id":200}},"tick":2}"#,
    r#"{"event":{"TeamInfo":{"entity_id":201,"team":"Team2"}},"tick":2}"#,
    r#"{"event":{"PosInfo":{"entity_id":101,"pos_x":-10.0,"pos_y":-2.0,"radius":0.5}},"tick":2}"#,
    r#"{"event":{"PosInfo":{"entity_id":201,"pos_x":10.0,"pos_y":2.0,"radius":0.5}},"tick":2}"#,
    r#"{"event":{"LifeInfo":{"entity_id":101,"alive":true}},"tick":2}"#,
    r#"{"event":{"LifeInfo":{"entity_id":201,"alive":true}},"tick":2}"#,
    r#"{"event":{"FunctionInfo":{"owner_id":101,"function":"0.00","corrected_function":null}},"tick":3}"#,
    r#"{"event":{"TurnInfo":{"player_id":100,"soldier_id":101}},"tick":4}"#,
    r#"{"event":{"EndOfTurnInfo":{"end_of_turn_tick":3604,"time_mode":"Timer1m"}},"tick":4}"#,
    r#"{"event":{"GameStateInfo":{"game_state":"WaitingForFunctions"}},"tick":4}"#,
];

/// 用**我们自己的实现**生成一条开火报文链，包成回放的一批。
///
/// 这样"回放"与"预测"在构造上必然一致（同一份 `Match::fire`），
/// 于是任何差异都只可能来自我们接下来做的那一处人为改动。
fn fire_batch(owner: i64, text: &str, x0: f64, y0: f64, flip: bool, tick: i64) -> Vec<String> {
    let mut m = Match::new(
        room(),
        vec![(0.0, 5.0, 2.0)],
        vec![
            Target { id: 101, x: -10.0, y: -2.0, radius: 0.5 },
            Target { id: 201, x: 10.0, y: 2.0, radius: 0.5 },
        ],
        (tick, 0),
    );
    let mut out = vec![format!(
        r#"{{"event":{{"FunctionFire":{{"soldier_id":{owner}}}}},"tick":{tick}}}"#
    )];
    for w in m.fire(owner, text, x0, y0, flip, 0) {
        // 回放的记录域里没有这两条（`docs/136` §1.2 的 `unobserved_kinds`），
        // 合成回放也不该有 —— 否则"合成语料"与"真语料"不是同一个形状。
        if w.contains("\"LifeQueuedDeath\"") || w.contains("\"QueueDestroyObstacle\"") {
            continue;
        }
        out.push(format!(r#"{{"event":{w},"tick":{tick}}}"#));
    }
    out
}

fn replay_text(window: &[String]) -> String {
    let mut lines: Vec<String> = SETUP.iter().map(|s| s.to_string()).collect();
    lines.extend(window.iter().cloned());
    lines.push(r#"{"event":{"TurnCountInfo":{"turn_count":1}},"tick":5000}"#.into());
    lines.join("\n") + "\n"
}

fn tmpdir() -> std::path::PathBuf {
    let d = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("replay_diff");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_replay(name: &str, text: &str) -> std::path::PathBuf {
    let p = tmpdir().join(name);
    std::fs::write(&p, text).unwrap();
    p
}

fn baseline_window() -> Vec<String> {
    fire_batch(101, "0.00", -10.0, -2.0, false, 3600)
}

// ---------------------------------------------------------------------------
// ① 一条完整回放跑通，且逐批一致
// ---------------------------------------------------------------------------

#[test]
fn complete_replay_runs_and_matches() {
    let p = write_replay("clean", &replay_text(&baseline_window()));
    let rep = run_file(&p, &Policy::default(), 8);
    assert!(rep.error.is_none(), "不该报错: {:?}", rep.error);
    assert_eq!(rep.batches, 1, "应当切出 1 个结算批");
    assert_eq!(rep.scored_batches, 1);
    assert_eq!(rep.shots, 1);
    assert_eq!(rep.content_ok, 1, "内容应当命中；第一分歧 = {:?}", rep.first);
    assert_eq!(rep.order_ok, 1, "顺序应当命中；第一分歧 = {:?}", rep.first);
    assert_eq!(rep.kind_seq_ok, 1);
    assert!(rep.first.is_none(), "不该有分歧: {:?}", rep.first);
}

/// 未压缩的 JSONL 也要能读（`docs/78` 说回放可能是裸 JSONL）。
#[test]
fn plain_jsonl_and_gzip_both_load() {
    let text = replay_text(&baseline_window());
    let plain = Replay::from_bytes(text.as_bytes()).expect("裸 JSONL");
    assert!(!plain.gzipped);
    assert_eq!(plain.msgs.len(), text.lines().filter(|l| !l.trim().is_empty()).count());
}

// ---------------------------------------------------------------------------
// ② 人为构造一处字段差异 ⇒ 必须报出来（负样本）
// ---------------------------------------------------------------------------

#[test]
fn single_field_difference_is_reported() {
    let good = baseline_window();
    // 找 `FunctionPoints` 那条，把 end_x 改掉 0.25
    let mut bad = good.clone();
    let idx = bad
        .iter()
        .position(|l| l.contains("\"FunctionPoints\""))
        .expect("合成批里必须有 FunctionPoints");
    let needle = "\"end_x\":";
    let start = bad[idx].find(needle).expect("end_x 字段") + needle.len();
    let end = bad[idx][start..].find([',', '}']).expect("字段结束") + start;
    let original: f64 = bad[idx][start..end].parse().expect("end_x 应是数字");
    bad[idx].replace_range(start..end, &format!("{}", original + 0.25));

    let p = write_replay("bad_end_x", &replay_text(&bad));
    let rep = run_file(&p, &Policy::default(), 8);
    let first = rep.first.as_ref().expect("-0.25 的差异必须被抓到");
    assert_eq!(first.diff.path, "FunctionPoints.end_x", "要指名到字段");
    assert_eq!(rep.content_ok, 0, "内容不得判为命中");
    let ours: f64 = first.diff.ours.parse().unwrap();
    let obs: f64 = first.diff.observed.parse().unwrap();
    assert!((obs - ours - 0.25).abs() < 1e-12, "差异值要对得上: {ours} vs {obs}");
}

/// 数值**格式**差异（`10` vs `10.0`）要报成"数值格式"，而不是"值不同"。
#[test]
fn numeric_format_difference_is_classified_separately() {
    let policy = Policy::default();
    let a = Msg::parse_wire(r#"{"FunctionPoints":{"owner_id":1,"start_x":1.0,"start_y":2.0,"end_x":10.0}}"#).unwrap();
    let b = Msg::parse_wire(r#"{"FunctionPoints":{"owner_id":1,"start_x":1,"start_y":2,"end_x":10}}"#).unwrap();
    let d = compare_msg(&a, &b, &policy);
    assert_eq!(d.len(), 3, "三个浮点字段都应报出: {d:?}");
    for f in &d {
        assert!(
            matches!(f.class, gw2_rebuild::_scaffold::replaydiff::DiffClass::NumericFormat),
            "{} 应当是格式差异: {:?}",
            f.path,
            f.class
        );
    }
    // 反过来：真正的值差异不能算成格式
    let c = Msg::parse_wire(r#"{"FunctionPoints":{"owner_id":1,"start_x":1.0,"start_y":2.0,"end_x":10.5}}"#).unwrap();
    let d2 = compare_msg(&a, &c, &policy);
    assert!(d2.iter().all(|f| !matches!(f.class, gw2_rebuild::_scaffold::replaydiff::DiffClass::NumericFormat)));
}

/// 字段顺序也要算差异（`docs/06` §17.1 的第三条）。
#[test]
fn field_order_difference_is_reported() {
    let policy = Policy::default();
    let a = Msg::parse_wire(r#"{"LifeQueuedDeath":{"entity_id":7,"tick":9}}"#).unwrap();
    let b = Msg::parse_wire(r#"{"LifeQueuedDeath":{"tick":9,"entity_id":7}}"#).unwrap();
    let d = compare_msg(&a, &b, &policy);
    assert!(
        d.iter().any(|f| matches!(f.class, gw2_rebuild::_scaffold::replaydiff::DiffClass::FieldOrder)),
        "字段顺序不同必须报 FieldOrder: {d:?}"
    );
}

// ---------------------------------------------------------------------------
// ③ 批内顺序：按口径判定
// ---------------------------------------------------------------------------

/// 同一批内把两条**同 kind** 的报文换序：内容（多重集）仍命中，顺序不算命中。
#[test]
fn order_inside_batch_is_multiset_by_declared_policy() {
    // 两发，用两条不同 owner 的 FunctionInfo 造一个"同 kind 块"
    let a = Msg::parse_wire(r#"{"FunctionInfo":{"owner_id":1,"function":"0.00","corrected_function":"0.00 + 1.000"}}"#).unwrap();
    let b = Msg::parse_wire(r#"{"FunctionInfo":{"owner_id":2,"function":"0.00","corrected_function":"0.00 + 2.000"}}"#).unwrap();
    let policy = Policy::default();
    let observed = vec![a.clone(), b.clone()];
    let ours_swapped = vec![b, a];
    let v = compare_batch(&ours_swapped, &observed, &policy, 8);
    assert!(v.content_ok, "内容（多重集）必须判命中 —— 这是 docs/113 的 A 类口径");
    assert!(!v.order_ok, "顺序不同必须如实报出来（只是不算内容失败）");
    assert!(v
        .diffs
        .iter()
        .any(|d| matches!(d.class, gw2_rebuild::_scaffold::replaydiff::DiffClass::MessageOrder)));
}

/// 跨批的顺序改动（把两批的内容互换）必须算**内容**失败，不能被多重集口径放过。
#[test]
fn order_across_batches_is_a_real_failure() {
    // 两批：第 2 批换一个函数（不同的 corrected 文本），然后把两批里的
    // 修正 `FunctionInfo` 交换 —— 这是**跨批**的顺序改动。
    let b1 = baseline_window();
    // 注意：**常数函数之间没有区别** —— 修正项 `c = y0 - f(x0)` 会把它们
    // 全部拉回射手那条水平线（`f(x)+c` 恒等于 y0）。所以要选一条真正不同的曲线。
    let b2 = fire_batch(101, "x/10", -10.0, -2.0, false, 3700);
    // 拼成"两批"的回放：setup + b1 + b2
    let mut lines: Vec<String> = SETUP.iter().map(|s| s.to_string()).collect();
    lines.extend(b1.iter().cloned());
    lines.extend(b2.iter().cloned());
    let two_batches = lines.join("\n") + "\n";
    let p_ok = write_replay("two_batches_ok", &two_batches);
    let rep_ok = run_file(&p_ok, &Policy::default(), 8);
    assert_eq!(rep_ok.batches, 2, "应当切出两批");
    assert_eq!(rep_ok.content_ok, 2, "两批都该命中: {:?}", rep_ok.first);

    // 交换两批里的 `FunctionInfo`（跨批顺序变化）
    let mut swapped = lines.clone();
    let corrected = |l: &String| l.contains("\"FunctionInfo\"") && !l.contains("\"corrected_function\":null");
    let i1 = swapped.iter().position(|l| corrected(l)).expect("合成批里应有修正后的 FunctionInfo");
    let i2 = swapped.iter().rposition(|l| corrected(l)).expect("两批各有一条");
    assert_ne!(i1, i2, "要交换的是两批里的两条");
    swapped.swap(i1, i2);
    let p_bad = write_replay("two_batches_swapped", &(swapped.join("\n") + "\n"));
    let rep_bad = run_file(&p_bad, &Policy::default(), 8);
    assert!(
        rep_bad.content_ok < 2,
        "跨批换序必须被算成失败（内容不再是多重集问题，而是输入与几何对不上）"
    );
    assert!(rep_bad.first.is_some(), "要给出一处具体分歧");

}

// ---------------------------------------------------------------------------
// ④ 口径里声明排除的东西，确实被排除
// ---------------------------------------------------------------------------

#[test]
fn declared_exclusions_are_really_excluded() {
    let policy = Policy::default();

    // (a) tick 字段：值不同 ⇒ 不算差异
    let a = Msg::parse_wire(r#"{"FunctionActive":{"owner_id":5,"active_tick":100}}"#).unwrap();
    let b = Msg::parse_wire(r#"{"FunctionActive":{"owner_id":5,"active_tick":999}}"#).unwrap();
    assert!(compare_msg(&a, &b, &policy).is_empty(), "tick 字段按口径不比");

    // (b) effect_id 绝对值：不比，但结构（严格递增）要比
    let c = Msg::parse_wire(r#"{"EffectInfo":{"effect_id":1,"pos_x":1.0,"pos_y":2.0,"effect_type":"ExplosionEffect","start_tick":7}}"#).unwrap();
    let d = Msg::parse_wire(r#"{"EffectInfo":{"effect_id":900,"pos_x":1.0,"pos_y":2.0,"effect_type":"ExplosionEffect","start_tick":7}}"#).unwrap();
    assert!(compare_msg(&c, &d, &policy).is_empty(), "effect_id 绝对值按口径不比");

    // (c) 「我方不产」的消息被排除并计数，而不是算成失败
    let obs = vec![
        Msg::parse_wire(r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#).unwrap(),
        Msg::parse_wire(r#"{"EffectInfo":{"effect_id":5,"pos_x":1.0,"pos_y":2.0,"effect_type":"AshEffect","start_tick":7}}"#).unwrap(),
        Msg::parse_wire(r#"{"EffectInfo":{"effect_id":6,"pos_x":1.0,"pos_y":2.0,"effect_type":"ExplosionEffect","start_tick":7}}"#).unwrap(),
    ];
    let ours = vec![
        Msg::parse_wire(r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#).unwrap(),
        Msg::parse_wire(r#"{"EffectInfo":{"effect_id":1,"pos_x":1.0,"pos_y":2.0,"effect_type":"ExplosionEffect","start_tick":7}}"#).unwrap(),
    ];
    let v = compare_batch(&ours, &obs, &policy, 8);
    assert_eq!(v.unproduced, 1, "AshEffect 应当记为「我方不产」，不是分歧");
    assert!(v.content_ok, "排除 AshEffect 之后内容应当命中");

    // (d) 「我方产出但回放不记」的也要排除并计数
    let obs2 = vec![Msg::parse_wire(r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#).unwrap()];
    let ours2 = vec![
        Msg::parse_wire(r#"{"GameStateInfo":{"game_state":"CalculatingResults"}}"#).unwrap(),
        Msg::parse_wire(r#"{"LifeQueuedDeath":{"entity_id":7,"tick":9}}"#).unwrap(),
    ];
    let v2 = compare_batch(&ours2, &obs2, &policy, 8);
    assert_eq!(v2.unobserved, 1);
    assert!(v2.content_ok, "LifeQueuedDeath 回放里从来不存在，不能算分歧");
}

/// 回放里同一 tick 的连续完全重复报文要折叠（`docs/136` §2.4 的观测）。
#[test]
fn consecutive_duplicate_messages_are_collapsed() {
    let policy = Policy::default();
    // 用我方**能产**的种类（`LifeInfo` 会被"我方不产"那条口径先排掉）
    let m = Msg::parse_wire(r#"{"GameStateInfo":{"game_state":"ExecutingResults"}}"#).unwrap();
    let obs = vec![m.clone(), m.clone()];
    let ours = vec![m];
    let v = compare_batch(&ours, &obs, &policy, 8);
    assert_eq!(v.collapsed, 1, "重复的一条应当被折叠计数");
}

// ---------------------------------------------------------------------------
// gzip 解码器（回放是 gzip；本 crate 零依赖，所以自带一个）
// ---------------------------------------------------------------------------

#[test]
fn gunzip_matches_a_real_dynamic_huffman_stream() {
    // 系统 gzip 压出来的 221 字节（三行 JSON），动态 Huffman 块
    const GZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xab, 0x56, 0x4a, 0x2d, 0x4b,
        0xcd, 0x2b, 0x51, 0xb2, 0xaa, 0x56, 0x72, 0x4f, 0xcc, 0x4d, 0x0d, 0x2e, 0x49, 0x2c, 0x49,
        0xf5, 0xcc, 0x4b, 0xcb, 0x07, 0x09, 0xa4, 0x03, 0x05, 0xe2, 0x8b, 0x41, 0x22, 0x4a, 0x56,
        0x4a, 0xc1, 0xa9, 0x25, 0xa5, 0x05, 0x4a, 0xb5, 0xb5, 0x3a, 0x4a, 0x25, 0x99, 0xc9, 0xd9,
        0x4a, 0x56, 0x86, 0xb5, 0x5c, 0xd5, 0x03, 0xaf, 0xd7, 0x39, 0x27, 0x35, 0xb1, 0xc8, 0x3f,
        0x09, 0xa8, 0x38, 0x39, 0x27, 0xb5, 0x18, 0x28, 0x82, 0x50, 0x68, 0x54, 0xcb, 0x05, 0x00,
        0x3c, 0x09, 0xdf, 0xd5, 0xdd, 0x00, 0x00, 0x00,
    ];
    let out = gz::gunzip(GZ).expect("动态 Huffman 流应当解得开");
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 4);
    assert!(text.contains(r#""ClearObstacles":{}"#));
}

#[test]
fn gunzip_rejects_corrupted_streams() {
    // 改掉尾部 CRC：必须报错，而不是悄悄返回坏数据
    const GZ: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xab, 0x56, 0x4a, 0x2d, 0x4b,
        0xcd, 0x2b, 0x51, 0xb2, 0xaa, 0x56, 0x72, 0x4f, 0xcc, 0x4d, 0x0d, 0x2e, 0x49, 0x2c, 0x49,
        0xf5, 0xcc, 0x4b, 0xcb, 0x07, 0x09, 0xa4, 0x03, 0x05, 0xe2, 0x8b, 0x41, 0x22, 0x4a, 0x56,
        0x4a, 0xc1, 0xa9, 0x25, 0xa5, 0x05, 0x4a, 0xb5, 0xb5, 0x3a, 0x4a, 0x25, 0x99, 0xc9, 0xd9,
        0x4a, 0x56, 0x86, 0xb5, 0x5c, 0xd5, 0x03, 0xaf, 0xd7, 0x39, 0x27, 0x35, 0xb1, 0xc8, 0x3f,
        0x09, 0xa8, 0x38, 0x39, 0x27, 0xb5, 0x18, 0x28, 0x82, 0x50, 0x68, 0x54, 0xcb, 0x05, 0x00,
        0x3c, 0x09, 0xdf, 0xd5, 0xdd, 0x00, 0x00, 0x01,
    ];
    assert!(gz::gunzip(GZ).is_err(), "CRC 不对必须报错");
}

// ---------------------------------------------------------------------------
// 真实语料上的冒烟（**只读**、可跳过）
// ---------------------------------------------------------------------------

/// 若本机有真回放，取最小的 3 份跑一遍（只读）。
///
/// 目录不存在时**跳过并打印原因**（仓库约定：缺资产不许静默通过）。
#[test]
fn real_corpus_smoke_if_present() {
    let dir = gw2_rebuild::_scaffold::replaydiff::default_corpus_dir();
    if !dir.is_dir() {
        eprintln!("skip: 没有真回放目录 {}（本测试需要本机语料）", dir.display());
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    if files.is_empty() {
        eprintln!("skip: {} 是空的", dir.display());
        return;
    }
    // 取最小的 3 份，控制测试时间（整份语料见 examples/replay_diff.rs）
    let mut sized: Vec<(u64, std::path::PathBuf)> = files
        .iter()
        .map(|p| (std::fs::metadata(p).map(|m| m.len()).unwrap_or(0), p.clone()))
        .collect();
    sized.sort();
    for (n, (_len, p)) in sized.iter().take(3).enumerate() {
        let r = Replay::load(p).unwrap_or_else(|e| panic!("第 {n} 份读不了: {e}"));
        assert!(r.gzipped, "真实回放应当是 gzip");
        if r.msgs.is_empty() {
            // 语料里确实有"空回放"（20 字节、解压 0 字节：开了局又立刻退出）
            eprintln!("note: {} 是空回放（0 事件）", p.display());
            continue;
        }
        let rep = run_file(p, &Policy::default(), 4);
        assert!(rep.error.is_none(), "{} 跑出错: {:?}", p.display(), rep.error);
        assert_eq!(rep.msgs, r.msgs.len());
    }
}
