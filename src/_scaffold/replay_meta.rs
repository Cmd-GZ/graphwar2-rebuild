//! **不是原版的东西**：回放**列表行需要的元数据**（关卡名 / 四个模式）的提取器。
//!
//! # 为什么需要它
//!
//! 原版的回放列表行（`replay_entry.tscn`）第 2 列显示**关卡名**、第 3 列是 4 个模式图标。
//! 这两样都**只存在于回放文件内容里**（文件名只是 unix 时间戳，`docs/78` §1）：
//!
//! ```text
//! {"event":{"GameNameInfo":{"game_name":"Graphwar Game"}},"tick":2}   ← 默认房间名
//! {"event":{"GameNameInfo":{"game_name":"Level 0"}},"tick":6}         ← 关卡名（后到者胜）
//! {"event":{"TimeModeInfo":{"time_mode":"TimerInf"}},"tick":6}        ← `∞` 图标
//! ```
//!
//! 观测（`docs/06` §56，原版真帧）：冻结快照两份回放放进原版 `user://replays` 之后，
//! 列表第 1 行显示 `Level 0`、第 2 行显示 `Level 16`，且第 2 行的函数模式图标是
//! `dx_dy_option`（不是第 1 行的 `fx_option`）—— 说明**图标也随回放内容变**，
//! 不是 `.tscn` 里的静态图。
//!
//! # 取哪一条（**观测**，不是猜）
//!
//! 同一条回放里 `GameNameInfo` 会出现两次：tick 2 是 `Graphwar Game`，tick 6/14 才是
//! `Level N`。原版真帧显示的是 `Level N` ⇒ 规则是"**最后一个值胜出**"，与
//! `src/graphwar_replay_player.rs::apply` 逐事件覆盖的语义一致。四个模式同理
//! （tick 2 的 `Timer1m` 是默认值，tick 6 的 `TimerInf` 才是这一局实际用的）。
//!
//! # 解压多少（**观测**，实测数见 `docs/06` §56）
//!
//! **解压是整份的**，解析只走到 [`HEAD_CAP`]。这一条与本任务最初的设想（"只解压头部"）
//! 不同，理由是实测：
//!
//! * 语料 403 份的 gzip 合计 **1.09 MB**、明文合计 **≈ 24 MB**（最大单份 169505 B）；
//!   旧注释里"400 份 ≈ 64 MB，整份解压太贵"与实测不符 —— 整份解压在 Rust 侧是毫秒级，
//!   客户端 GDScript 那一份（`replay_file.gd`）才是真正贵的（每行一次 `JSON.parse_string`）。
//! * 想"只解压头部"就必须有一个**能中途停下**的 inflate，而现有解码器
//!   （`_scaffold/replay_inflate.rs`，纯 Rust、逐位解码）只提供"整份解压"这一个入口；
//!   在本文件里再写一份能停的解码器 = 第二份 DEFLATE 实现，正是 `AGENTS.md` 反复禁止的
//!   （"一个文件混了多个模块而边界无证据时不猜"、"这里没有第二份逻辑"）。
//!   而 `replay_inflate.rs` 不在本任务的允许改动清单里 ⇒ 本轮**如实登记为未做**，
//!   不假装做到了。
//! * 作为替代，解析上限 [`HEAD_CAP`] 取得足够大：语料里"最后一个相关事件"的最远字节偏移是
//!   **62 401 B**（文件 `1793884550`）⇒ [`HEAD_CAP`] = 64 KiB 时，403 份的解析结果与
//!   "整份解析"**逐份相同**（复跑命令见 `docs/06` §56.3）。
//!
//! # 证据等级
//!
//! | 结论 | 档位 |
//! |---|---|
//! | 第 2 列 = `GameNameInfo.game_name`（最后一个） | **观测**（原版真帧 `Level 0` / `Level 16`，与"取第一个"的 `Graphwar Game` 不同） |
//! | 4 个图标随内容变 | **观测**（第 2 行 `dx_dy_option` vs 第 1 行 `fx_option`） |
//! | 具体"原版是不是整份解压" | **未知**（原版 `libgraphwarrust.so` 里这条读取路径未定位） |
//! | 语料上的字节/耗时数 | **观测**（本文件 §4 的复跑命令） |

use std::path::Path;

use crate::graphwar_replay_player::{gunzip, is_gzip};

/// 解析上限（明文字节）。语料最大需要 62 401 B ⇒ 64 KiB 时与整份解析同结果。
pub const HEAD_CAP: usize = 64 * 1024;

/// 一份回放的列表元数据。字段全是**字符串原值**（不翻译、不美化）：
/// 它们来自回放里的枚举名（`EveryUnit` / `SequentialTurns` / …），由界面层决定用哪张图。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayMeta {
    /// 读到至少一条事件 ⇒ true；`false` 时看 [`Self::error`]。
    pub ok: bool,
    pub error: String,
    /// `GameNameInfo.game_name`（**最后一个**）。空串 = 回放里没有这条事件。
    pub game_name: String,
    /// `AxisModeInfo.axis_mode`
    pub axis_mode: String,
    /// `FunctionModeInfo.function_mode`
    pub function_mode: String,
    /// `TurnModeInfo.turn_mode`
    pub turn_mode: String,
    /// `TimeModeInfo.time_mode`
    pub time_mode: String,
    /// 解析进去的明文字节数（≤ [`HEAD_CAP`]）。
    pub scanned: usize,
    /// 整份明文有多少字节（不含被截掉的部分时仍有意义：超 [`HEAD_CAP`] 时它就是真实大小）。
    pub plain_len: usize,
    /// 输入是不是 gzip（真实回放是；明文回放不是）。
    pub gzipped: bool,
    /// 语料里出现过解析不了的行（跳过并计数，与 `tools/parse_replay.py` 同口径）。
    pub skipped_lines: usize,
}

/// 从一段**已经解压好**的明文里抽字段 —— 也是单元测试的入口。
///
/// 只认这 5 个事件，且**后面的覆盖前面的**（见模块文档"取哪一条"）。
pub fn parse_head(text: &str) -> ReplayMeta {
    parse_with_cap(text, HEAD_CAP)
}

/// 同 [`parse_head`]，但扫描上限可由调用方给（`usize::MAX` = 整份）。
/// 语料校验用：拿"整份的结果"当基准去比"头部上限的结果"。
pub fn parse_with_cap(text: &str, cap_bytes: usize) -> ReplayMeta {
    let mut m = ReplayMeta::default();
    let bytes = text.as_bytes();
    let cap = bytes.len().min(cap_bytes);
    // 按行切：只在 `cap` 之内找换行，**不**切整份字符串（大回放上省一次分配）。
    let mut start = 0usize;
    while start < cap {
        let end = match text[start..cap].find('\n') {
            Some(i) => start + i,
            None => cap,
        };
        let line = text[start..end].trim();
        if !line.is_empty() {
            if !apply_line(line, &mut m) {
                m.skipped_lines += 1;
            }
        }
        m.scanned = end;
        start = end + 1;
    }
    m.plain_len = bytes.len();
    m.ok = !m.game_name.is_empty()
        || !m.axis_mode.is_empty()
        || !m.function_mode.is_empty()
        || !m.turn_mode.is_empty()
        || !m.time_mode.is_empty();
    if !m.ok && m.error.is_empty() {
        m.error = "前 64 KiB 里没有任何模式/关卡事件".to_string();
    }
    m
}

/// 处理一行 JSONL；认出来返回 true。**不做完整 JSON 解析**（没有 serde 依赖，
/// 见 `Cargo.toml`）：只认"事件名 + 那一个字符串字段"，其余字节原样跳过。
fn apply_line(line: &str, m: &mut ReplayMeta) -> bool {
    // 事件名 = `"event":{` 之后的第一个 JSON 键。
    let body = match line.find("\"event\":{") {
        Some(i) => &line[i + 9..],
        None => return false,
    };
    let key = match first_quoted(body) {
        Some(k) => k,
        None => return false,
    };
    let field = match key {
        "GameNameInfo" => "game_name",
        "AxisModeInfo" => "axis_mode",
        "FunctionModeInfo" => "function_mode",
        "TurnModeInfo" => "turn_mode",
        "TimeModeInfo" => "time_mode",
        _ => return false,
    };
    let value = match string_field(body, field) {
        Some(v) => v,
        None => return true, // 事件认得、字段缺 ⇒ 不算坏行
    };
    match key {
        "GameNameInfo" => m.game_name = value.to_string(),
        "AxisModeInfo" => m.axis_mode = value.to_string(),
        "FunctionModeInfo" => m.function_mode = value.to_string(),
        "TurnModeInfo" => m.turn_mode = value.to_string(),
        "TimeModeInfo" => m.time_mode = value.to_string(),
        _ => unreachable!(),
    }
    true
}

/// `"<name>":"<value>"` 里的 `<value>`（第一个非转义引号前的内容）。
fn string_field<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("\"{name}\":");
    let i = s.find(&pat)?;
    let rest = s[i + pat.len()..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out_end = None;
    let mut esc = false;
    for (k, ch) in rest.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        match ch {
            '\\' => esc = true,
            '"' => {
                out_end = Some(k);
                break;
            }
            _ => {}
        }
    }
    let e = out_end?;
    // 转义序列**不解码**（这些字段全是 ASCII 枚举名/关卡名，实测 403 份语料里出现 0 次）。
    Some(&rest[..e])
}

/// 第一个 JSON 字符串键（`"xxx"` 里的 xxx）。
fn first_quoted(s: &str) -> Option<&str> {
    let rest = s.trim_start().strip_prefix('"')?;
    let e = rest.find('"')?;
    Some(&rest[..e])
}

impl ReplayMeta {
    /// 从**原始文件字节**抽取（gzip 或明文都吃，与 `tools/parse_replay.py` 同口径）。
    pub fn from_raw(raw: &[u8]) -> Self {
        let mut m = ReplayMeta::default();
        if raw.is_empty() {
            m.error = "空文件".to_string();
            return m;
        }
        let plain = if is_gzip(raw) {
            m.gzipped = true;
            match gunzip(raw) {
                Ok(p) => p,
                Err(e) => {
                    m.error = format!("gzip 解压失败：{}", e.as_str());
                    return m;
                }
            }
        } else {
            raw.to_vec()
        };
        if plain.is_empty() {
            m.error = "解压后是空的（空回放）".to_string();
            m.plain_len = 0;
            return m;
        }
        let text = String::from_utf8_lossy(&plain);
        let mut out = parse_head(&text);
        out.gzipped = m.gzipped;
        out
    }

    /// 从磁盘上的一个文件抽取（只读）。
    pub fn from_path(p: &Path) -> Self {
        match std::fs::read(p) {
            Ok(raw) => Self::from_raw(&raw),
            Err(e) => ReplayMeta { error: format!("读不到：{e}"), ..Default::default() },
        }
    }

    /// 界面用的四个模式图标名（`GwIcons` 的键）。**空串 = 回放里没有这个模式事件**，
    /// 此时返回空串，由界面层决定画不画（**不猜默认档**：猜错就是自造画面）。
    pub fn axis_icon(&self) -> &'static str {
        match self.axis_mode.as_str() {
            "NoAxis" => "axis_option_0",
            "OnlyMainAxes" | "OnlyMainAxis" => "axis_option_1",
            "EveryFiveUnits" | "EveryFive" => "axis_option_2",
            "EveryUnit" => "axis_option_3",
            _ => "",
        }
    }

    pub fn function_icon(&self) -> &'static str {
        match self.function_mode.as_str() {
            "NormalFunction" => "fx_option",
            "DiffEqFunction" => "dx_dy_option",
            _ => "",
        }
    }

    pub fn turn_icon(&self) -> &'static str {
        match self.turn_mode.as_str() {
            "SequentialTurns" => "turns_sequential",
            "SimultaneousTurns" => "turns_simultaneous",
            _ => "",
        }
    }

    pub fn time_icon(&self) -> &'static str {
        match self.time_mode.as_str() {
            "Timer30s" => "turn_30s",
            "Timer1m" => "turn_1m",
            "Timer2m" => "turn_2m",
            "Timer3m" => "turn_3m",
            "Timer5m" => "turn_5m",
            "TimerInf" => "turn_inf",
            _ => "",
        }
    }

    /// 四个图标名，顺序 = `replay_entry.tscn` 的 `AxisModeButton` / `FunctionModeButton` /
    /// `TurnModeButton` / `TimeModeButton`。
    pub fn mode_icons(&self) -> [&'static str; 4] {
        [self.axis_icon(), self.function_icon(), self.turn_icon(), self.time_icon()]
    }
}

/// 文件名 → 录制时间。**文件名就是 unix 时间戳、没有扩展名**（`docs/78` §1）。
///
/// 原版真帧证据（`docs/06` §56.1）：`1793897298` 显示 `2026-11-05 16:48:18` = 该时间戳的
/// **UTC** 时间，而文件的 `mtime` 是拷贝时间（6 月 28 日）⇒ 原版读的是**文件名**，
/// 不是 `mtime`，且按 **UTC** 格式化。
pub fn timestamp_from_name(name: &str) -> Option<i64> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name.parse::<i64>().ok()
}

/// unix 秒 → `YYYY-MM-DD HH:MM:SS`（UTC，无闰秒），形状与原版真帧逐字符相同。
///
/// 只用整数运算（`docs/35` §2：tick 时钟 60 Hz，与时区无关）；算法是
/// Howard Hinnant 的 `civil_from_days`（`days_from_civil` 的逆）。
pub fn format_utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    // 1970-01-01 起的天数 → (y, m, d)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        r#"{"event":{"GameStateInfo":{"game_state":"Connecting"}},"tick":1}"#,
        "\n",
        r#"{"event":{"AxisModeInfo":{"axis_mode":"EveryUnit"}},"tick":2}"#,
        "\n",
        r#"{"event":{"FunctionModeInfo":{"function_mode":"NormalFunction"}},"tick":2}"#,
        "\n",
        r#"{"event":{"TimeModeInfo":{"time_mode":"Timer1m"}},"tick":2}"#,
        "\n",
        r#"{"event":{"GameNameInfo":{"game_name":"Graphwar Game"}},"tick":2}"#,
        "\n",
        r#"{"event":{"TurnModeInfo":{"turn_mode":"SequentialTurns"}},"tick":2}"#,
        "\n",
        r#"{"event":{"GameNameInfo":{"game_name":"Level 0"}},"tick":6}"#,
        "\n",
        r#"{"event":{"TimeModeInfo":{"time_mode":"TimerInf"}},"tick":6}"#,
        "\n",
    );

    /// 最后一个值胜出：tick 2 的 `Graphwar Game` / `Timer1m` 必须被 tick 6 的覆盖。
    #[test]
    fn last_value_wins_and_matches_original_frame() {
        let m = parse_head(SAMPLE);
        assert!(m.ok);
        assert_eq!(m.game_name, "Level 0");
        assert_eq!(m.time_mode, "TimerInf");
        assert_eq!(m.axis_mode, "EveryUnit");
        assert_eq!(m.function_mode, "NormalFunction");
        assert_eq!(m.turn_mode, "SequentialTurns");
        assert_eq!(m.mode_icons(), ["axis_option_3", "fx_option", "turns_sequential", "turn_inf"]);
    }

    /// 第 2 份冻结回放的函数模式是 `DiffEqFunction` ⇒ `dx_dy_option`（原版真帧第 2 行）。
    #[test]
    fn diff_eq_function_picks_dx_dy_icon() {
        let text = SAMPLE.replace("NormalFunction", "DiffEqFunction");
        assert_eq!(parse_head(&text).function_icon(), "dx_dy_option");
    }

    /// 坏行跳过并计数；完全不认识的 JSONL 不炸、也不是 ok。
    #[test]
    fn bad_lines_are_skipped_not_fatal() {
        let text = "\nnot json\n{\"event\":{\"PosInfo\":{\"entity_id\":230}}}\n{\"event\":\n";
        let m = parse_head(text);
        assert!(!m.ok);
        assert_eq!(m.skipped_lines, 3);
        assert!(!m.error.is_empty());
    }

    /// 解析上限是 64 KiB：超出的行不读（`plain_len` 仍是真实长度）。
    #[test]
    fn head_cap_stops_scanning() {
        let pad = "{\"event\":{\"PosInfo\":{\"entity_id\":1}}}\n".repeat(4000);
        assert!(pad.len() > HEAD_CAP);
        let text = format!("{pad}{}", r#"{"event":{"GameNameInfo":{"game_name":"Level 9"}},"tick":9}"#);
        let m = parse_head(&text);
        assert_eq!(m.scanned, HEAD_CAP);
        assert_eq!(m.plain_len, text.len());
        assert_eq!(m.game_name, "", "上限之后的事件不该被读到");
    }

    /// 明文回放（未压缩）也吃；空文件报错不抛。
    #[test]
    fn plaintext_and_empty_inputs() {
        let m = ReplayMeta::from_raw(SAMPLE.as_bytes());
        assert!(!m.gzipped);
        assert_eq!(m.game_name, "Level 0");
        let e = ReplayMeta::from_raw(b"");
        assert!(!e.ok);
        assert_eq!(e.error, "空文件");
    }

    /// 时间戳：文件名 → UTC 字符串（原版真帧两个样本逐字符对上）。
    #[test]
    fn timestamp_from_name_formats_utc_like_the_original_frame() {
        assert_eq!(timestamp_from_name("1793897298"), Some(1_793_897_298));
        assert_eq!(format_utc(1_793_897_298), "2026-11-05 16:48:18");
        assert_eq!(format_utc(1_783_255_495), "2026-07-05 12:44:55");
        // 带扩展名/非数字 ⇒ 不认（真实回放名是纯数字）
        assert_eq!(timestamp_from_name("1793897298.replay"), None);
        assert_eq!(timestamp_from_name(""), None);
        // 闰年 2 月 29 日与纪元点
        assert_eq!(format_utc(0), "1970-01-01 00:00:00");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(format_utc(-1), "1969-12-31 23:59:59");
    }

    /// **冻结语料校验**（403 份只读快照）：`HEAD_CAP` 上限下的结果与"整份解析"逐份一致。
    ///
    /// 目录由环境变量给（默认不跑，避免 `cargo test` 依赖仓库外的数据）：
    ///
    /// ```text
    /// GW2_REPLAYS_DIR=<快照>/replays cargo test --offline --lib replay_meta -- --nocapture
    /// ```
    #[test]
    fn corpus_head_cap_equals_full_parse() {
        let dir = match std::env::var("GW2_REPLAYS_DIR") {
            Ok(d) if !d.is_empty() => d,
            _ => return, // 没给目录 ⇒ 静默跳过（见文档里的复跑命令）
        };
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .expect("读不到 GW2_REPLAYS_DIR")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        let (mut n, mut bad, mut diff, mut plain_total, mut scanned_total) = (0, 0, 0, 0usize, 0usize);
        let mut max_last_hit = 0usize;
        let t0 = std::time::Instant::now();
        for p in &files {
            let raw = std::fs::read(p).unwrap();
            n += 1;
            let plain = if is_gzip(&raw) {
                match gunzip(&raw) {
                    Ok(v) => v,
                    Err(_) => {
                        bad += 1;
                        continue;
                    }
                }
            } else {
                raw.clone()
            };
            plain_total += plain.len();
            let text = String::from_utf8_lossy(&plain);
            let head = parse_with_cap(&text, HEAD_CAP);
            let full = parse_with_cap(&text, usize::MAX);
            scanned_total += head.scanned;
            if head.game_name != full.game_name
                || head.axis_mode != full.axis_mode
                || head.function_mode != full.function_mode
                || head.turn_mode != full.turn_mode
                || head.time_mode != full.time_mode
            {
                diff += 1;
            }
            // "最后一个相关事件"的明文偏移（决定 HEAD_CAP 要多大）
            let mut off = 0usize;
            for line in text.split('\n') {
                if line.contains("GameNameInfo")
                    || line.contains("AxisModeInfo")
                    || line.contains("FunctionModeInfo")
                    || line.contains("TurnModeInfo")
                    || line.contains("TimeModeInfo")
                {
                    max_last_hit = max_last_hit.max(off);
                }
                off += line.len() + 1;
            }
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "语料 {n} 份：解压失败 {bad}；与整份解析**不一致** {diff} 份；\
             明文合计 {:.2} MB；解析合计 {} B；最后一个相关事件最远偏移 {} B（HEAD_CAP={}）；\
             整轮（解压+解析）{:.1} ms",
            plain_total as f64 / 1e6,
            scanned_total,
            max_last_hit,
            HEAD_CAP,
            ms
        );
        assert_eq!(bad, 0, "快照里不该有解不开的回放");
        assert_eq!(diff, 0, "HEAD_CAP 太小：有文件的头部结果与整份不同");
        assert!(max_last_hit < HEAD_CAP);
    }
}
