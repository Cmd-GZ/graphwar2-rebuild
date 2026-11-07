//! 收敛层与内核**逐位一致**的验收（`docs/06-verification.md` §30.5，任务 16 的核心验收）。
//!
//! 比的不是"报文对上回放"（那是 `tests/diff_conv.rs` 的事），而是**两条路径对同一批回放
//! 算出的中间结果是否逐位相同**：
//!
//! | 路径 | 是什么 |
//! |---|---|
//! | K | `game::Match::fire_functions` + `Match::fire_batch` —— 真实实现（内核） |
//! | C | `diffconv::ConvPredictor::fire_batch` —— 收敛层 |
//!
//! 五项逐位比：**点序列**（逐坐标 `to_bits`）/ **stop** / **blocked** / **killed**（含顺序）/
//! **修正文本**；外加命中弧长、挖洞点。
//!
//! ⚠️ **DiffEq 批 2026-11-08 起也在分母里**（`docs/06` §33）：内核开火路径现在按
//! `Match::sampling_mode()` 选采样器，Ode 局两条路径走同一套坐标。此前它们被 `continue`
//! 掉、只计数——那正是 515 批 / 550 发"不可比"的那一段。断言因此**变严了**（不多比一条，
//! 只是把原来跳过的 550 发也要求逐位相同）。
//!
//! 全语料（1080 批 / 1290 发）用同一条代码路径跑：
//!
//! ```text
//! cargo run --release --offline --example diffconv_parity -- --dir <快照>
//! ```
//!
//! 本文件默认只跑一个**固定样本**（语料在时），并支持 `GW2_PARITY_FULL=1` 跑全语料。

use gw2_rebuild::_scaffold::diffconv::{parity_dir, parity_file, Parity};
use gw2_rebuild::_scaffold::replaydiff::default_corpus_dir;

/// 固定样本：覆盖 Normal 单发自由飞行 / Normal 单发撞障碍 / Normal 多发批（11 批）/
/// Normal 撞障碍但零死亡 / DiffEq（**现在也要求逐位相同**，见 `docs/06` §33）。
///
/// 名字与内容都来自只读语料；挑选依据见 `docs/06` §30.5（按 `batch_conv --json` 的
/// `shots` / `obs_destroy` / `obs_deaths` 三个字段筛的）。
const SAMPLE: &[&str] = &[
    "1782640359", // Normal 单发，打到靶子（SparkEffect）
    "1782640836", // Normal 单发，撞障碍 + 有死亡
    "1782674519", // Normal 单发，撞障碍、零死亡
    "1793899173", // Normal 多发批 11 批（同时回合制）
    "1782760054", // DiffEq（Ode 轴：内核与收敛层都必须走 RK4）
];

fn report(tag: &str, p: &Parity) {
    println!("[{tag}] {}", p.summary());
    println!(
        "[{tag}] 挖洞点逐位相同 {} / {}；爆炸点相同 {} / {}（差的是内核 ④ 仍用 stop，§30.5）",
        p.destroy_equal, p.destroy_compared, p.explosion_equal, p.explosion_compared
    );
    println!(
        "[{tag}] DiffEq 批 {} / {} 发：**在分母里**（内核开火路径已补 `Mode`，`docs/06` §33）",
        p.diff_eq_batches, p.diff_eq_shots
    );
    for m in &p.mismatches {
        println!(
            "[{tag}] ! {} 批 #{} 第 {} 发 · {}：C={} K={}",
            m.file, m.batch, m.shot, m.field, m.ours, m.kernel
        );
    }
}

/// **核心断言**：固定样本上，Normal 轴的每一发都逐位相同。
#[test]
fn conv_layer_matches_the_kernel_bit_for_bit_on_the_fixed_sample() {
    let dir = default_corpus_dir();
    if !dir.is_dir() {
        eprintln!("跳过：回放语料目录不存在（{}）", dir.display());
        return;
    }
    let mut p = Parity::default();
    let mut files = 0usize;
    for name in SAMPLE {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("跳过 {name}：语料里没有这一份");
            continue;
        }
        files += 1;
        parity_file(&path, &mut p, 8);
    }
    if files == 0 {
        eprintln!("跳过：样本里的回放一份都不在语料里（{}）", dir.display());
        return;
    }
    report("样本", &p);
    assert!(p.shots > 0, "样本里应当有 Normal 结算批（实际 0 发）");
    assert!(
        p.mismatches_total == 0,
        "两条路径必须逐位相同，实际 {} 处不一致（见上面的 ! 行）",
        p.mismatches_total
    );
    assert_eq!(
        p.shots_identical, p.shots,
        "五项逐位全同的发数必须等于参评发数"
    );
    // 正对照（反向）：这条轴确实在"比东西" —— 挖洞点必须真的被比到过，
    // 否则"逐位相同"可能只是"没比较"（§30.5 的正对照 ①②）。
    assert!(p.destroy_compared > 0, "样本里应当有撞障碍的批（挖洞点要真的被比过）");
    assert_eq!(p.destroy_equal, p.destroy_compared, "挖洞点也必须逐位相同");
    // DiffEq 批**真的进了分母**（2026-11-08 起）：既要被比到过，又要逐位相同。
    // 这两条是防空转的：`diff_eq_shots == 0` 说明样本里没有 DiffEq 局，
    // 那样"DiffEq 也逐位相同"就是一句没被检验的话（`docs/06` §33.4）。
    assert!(
        p.diff_eq_batches > 0,
        "样本里应当有一份 DiffEq 回放（`diff_eq_batches > 0`）"
    );
    assert!(
        p.diff_eq_shots > 0 && p.diff_eq_shots < p.shots,
        "DiffEq 发数必须 > 0 且小于总发数（实际 {} / {}）",
        p.diff_eq_shots,
        p.shots
    );
}

/// 全语料：`GW2_PARITY_FULL=1` 时跑（约 1080 批 / 1290 发；debug 下很慢，默认不跑）。
#[test]
fn full_corpus_parity_matches_the_documented_command() {
    if std::env::var("GW2_PARITY_FULL").is_err() {
        eprintln!(
            "跳过：默认只跑固定样本。全语料用 `GW2_PARITY_FULL=1 cargo test --offline --test diffconv_parity`，\
             或 `cargo run --release --offline --example diffconv_parity -- --dir <快照>`（docs/06 §30.5）"
        );
        return;
    }
    let dir = default_corpus_dir();
    if !dir.is_dir() {
        eprintln!("跳过：回放语料目录不存在（{}）", dir.display());
        return;
    }
    let p: Parity = parity_dir(&dir, 8);
    report("全语料", &p);
    assert!(p.mismatches_total == 0, "全语料必须逐位相同");
    assert_eq!(p.shots_identical, p.shots);
    assert_eq!(p.destroy_equal, p.destroy_compared);
}
