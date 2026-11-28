//! 原版 `src/graphwar_event.rs` 的**手写部分** —— 也就是 `fixtures/denom_outside.tsv` 里
//! 那 16 条机器码中**唯一真缺的 3 条**（G1/G2/G3，全部是**固有方法**）。
//!
//! ## 原版这个文件是什么（T98 §147.3 + T107 §153）
//!
//! **≥1154 行的纯类型/枚举定义文件**：72 个 `*Event` 载荷结构体 + 主枚举 `GraphwarEvent`
//! （74 个 newtype 变体，每个变体包一个 `*Event`）。编译后机器码里只有 **3 个手写方法**，
//! 其余全是 `#[derive(Serialize, Deserialize, Clone)]` 的展开产物与 drop glue：
//!
//! | android vaddr | st_size | 符号（`.symtab` 真名） | 是什么 |
//! |---|---|---|---|
//! | `0x4691a4` | 540 | `graphwarrust::graphwar_event::GraphwarEvent::deserialize` | **固有**方法（手写） |
//! | `0x4693c0` | 188 | `…::GraphwarEvent::make_new_connection_request_event` | **固有**方法（手写） |
//! | `0x46947c` | 304 | `…::GraphwarEvent::serialize` | **固有**方法（手写） |
//! | `0x4695ac` | 1704 | `…::_::_<impl serde::ser::Serialize for …GraphwarEvent>::serialize` | **derive 展开** |
//! | `0x469c54` | 5564 | `…::_::_<impl serde::de::Deserialize for …GraphwarEvent>::deserialize` | **derive 展开** |
//!
//! 判别依据（`fixtures/denom_serde_plan.txt` §1，P1 是决定性的那条，语言事实）：
//! **`#[derive(X)]` 只生成 `impl X for T`，永远不生成固有方法** ⇒ 名字是
//! `module::Type::method`（固有）的那 3 条一定是人手写的；名字是
//! `module::_::_<impl serde::… for Type>::method` 的那些必须另找证据（`__Visitor`/
//! `__FieldVisitor` 家族 + 体内 own=0 的模板体）才判 derive。
//! 逐条证据表：`fixtures/denom_serde_evidence.tsv`（16 行，含 5 组对照）。
//!
//! ## 本文件目前落到哪一步（诚实清单）
//!
//! * ✅ **G2 `make_new_connection_request_event`：函数体已复原**（`0x4693c0` 全 47 条指令读完，
//!   见函数文档）。
//! * 🟡 **G1 `deserialize` / G3 `serialize`：只落到"结构 + 全部常量"**，函数体**没写**。
//!   原因不是缺证据（体内已 100 % 读完，见两条 `…_EVIDENCE` 文档），而是缺**依赖**：
//!   原版体内是 `serde_json::to_vec` / `serde_json::from_str` + `chrono::Utc::now()` +
//!   `println!`；本 crate **零依赖**（`Cargo.toml` 只有可选的 `godot`），
//!   照抄函数体等于要引入 `serde` / `serde_json` / `chrono` 三个 crate + 给 72 个载荷结构体
//!   加 `#[derive]` —— 这是**项目级决定**，不是本文件能定的。
//!   ⇒ 不写伪实现（AGENTS.md §0.3：不许把推断写成结论；也不许拿我方仿制逻辑顶上去）。
//!
//! ⚠️ **本文件当前不在 crate 里**：`src/lib.rs` 是 `AGENTS.md`「合并纪律」列出的**禁区**，
//! 本轮不动它。文件通过 `tests/graphwar_event.rs` 的 `#[path = "../src/graphwar_event.rs"]`
//! 编进测试二进制（先例：`tests/graphwar_computer_player_manager.rs`）⇒ 代码**有编译、有单测**，
//! 但还没被生产路径调用。要真正进 crate 需要**二选一**（都在本轮写范围之外，交 lead 决定）：
//! ① `src/lib.rs` 加一行 `pub mod graphwar_event;`；
//! ② `src/events.rs` 加两行 `#[path = "graphwar_event.rs"] pub mod graphwar_event;`。
//!
//! 地址口径：本文件所有 `0x…` 都是 **Android 2.0.60 arm64 的 ELF vaddr**（未 strip，
//! `.symtab` 真名可用）。**不要**与 Linux x86-64 那份换算 —— 两份布局无关。

// ---------------------------------------------------------------------------
// 1. `GraphwarEvent` 的判别值 / 机器布局（全部来自指令立即数与 `.rodata` 常量）
// ---------------------------------------------------------------------------

/// `GraphwarEvent` 内部判别值的高位基（全库一致）。
///
/// 真值：`0x469244`（`mov x8,#0x4a` + `movk x8,#0x8000,lsl#48`）、
/// `0x469268`/`0x469318`（同一个 `0x8000_0000_0000_004a`）、
/// `0x196c60` 的 16 字节常量首字 = `04 00 00 00 00 00 00 80`。**观测**。
pub const TAG_BASE: u64 = 0x8000_0000_0000_0000;

/// `NewConnectionRequest`（= `events::Event::NewConnectionRequest` = 4）的判别值。
///
/// 真值：`0x46941c` `adrp x8,0x196000` + `0x46942c` `ldr q0,[x8,#0xc60]` 取 16 字节
/// `04 00 00 00 00 00 00 80 | 03 00 00 00 00 00 00 00`，`0x469434` `str q0,[x19]`
/// ⇒ 写进返回值的 `+0x00`。第二字 `3` = `major_version` 的长度。**观测**。
pub const TAG_NEW_CONNECTION_REQUEST: u64 = 0x8000_0000_0000_0004;

/// **INVALID 哨兵**：`deserialize` 两条失败路径写进返回值的判别值。
///
/// 真值：`0x469244`（UTF-8 失败臂）与 `0x469268`+`0x469318`（serde_json 失败臂）
/// 把 `0x8000_0000_0000_004a` 写进 `[x19]`（返回值首字）；成功路径
/// `0x469288 ldr x8,[sp,#0x38]; cmp x8,x20; b.ne 0x469334` 用它做"是不是错误"的判据。
/// `0x4a` = 74 = **变体数**，落在 `0..=73` 之外 ⇒ 不可能是合法变体。**观测**。
///
/// ⚠️ 同一个常量 `0x8000_0000_0000_004a` 在
/// [`crate::graphwar_client_connection_handler::STREAM_READ_NONE`] 里出现过（语义未知，见那里）；
/// 两处**是不是同一个哨兵**未证 —— 地址不同构件、无跨构建键，**未知**。
pub const TAG_INVALID: u64 = 0x8000_0000_0000_004a;

/// `GraphwarEvent` 的**机器宽度**：72 字节。
///
/// 真值：`deserialize` 成功路径 `0x469334..0x469350` 逐字拷
/// `stp q0,q1,[x19,#0x20]` + `str x8,[x19,#0x40]` + `stp q1,q0,[x19]`
/// = `16+16+8` 落在 `+0x20..+0x48`、`32` 落在 `+0x00..+0x20` ⇒ 共 **0x48** 字节。**观测**。
pub const EVENT_BYTES: usize = 0x48;

/// `serialize` 里 `Vec::with_capacity(128)` 的那个 128。
///
/// 真值：`0x469498` `mov w0,#0x80` + `0x46949c` `mov w1,#0x1` + `bl __rust_alloc`
/// （`__rust_alloc(0x80, 1)`），`0x4694a8 mov w8,#0x80` 再把 cap 存进栈上的 Vec。
/// **128 = `serde_json::to_vec` 内部 `Vec::with_capacity(128)` 的字面量** —— 这是
/// "`serialize` 就是 `serde_json::to_vec(self)`" 的关键判据之一。**观测**。
pub const SERIALIZE_VEC_CAPACITY: usize = 128;

// ---------------------------------------------------------------------------
// 2. `deserialize` 的两条失败路径（`println!` 格式串，含原版 source 行号）
// ---------------------------------------------------------------------------
//
// 原版 `GraphwarEvent::deserialize`（`0x4691a4`，540 B）的控制流（全读，**观测**）：
//
//   0x4691bc  x8 = sp+0x10; stp x0,x1,[sp]      ; 入参 (&bytes, len) 落栈
//   0x4691c4  bl core::str::converts::from_utf8 ; ← 先做 UTF-8 校验
//   0x4691c8  ldr x8,[sp,#0x10]; cmp x8,#1; b.ne 0x469264
//             └─ x8 == 1（Err）⇒ 继续往下 = **UTF-8 失败臂**（0x4691d4..0x469260）
//   0x469264  （UTF-8 成功）ldp x8,x9,[sp,#0x18] …
//   0x469284  bl serde_json::de::from_trait    ; = serde_json::from_str::<GraphwarEvent>
//   0x469288  ldr x8,[sp,#0x38]; cmp x8,x20(=TAG_INVALID); b.ne 0x469334
//             └─ 相等 = 错误 ⇒ 继续 = **JSON 失败臂**（0x469294..0x469330）
//   0x469334  （成功）逐字拷 0x48 字节到返回值 = GraphwarEvent
//
// 两个失败臂的公共形状：`bl chrono::offset::utc::Utc::now` → `bl alloc::fmt::format`
//   （内层串 = `MSG_DESERIALIZE_INNER`）→ `bl std::io::stdio::_print`（外层串 = 带行号那条）
//   → 返回值首字写 `TAG_INVALID`。
//   ⚠️ 是 **stdout**（`std::io::stdio::_print`），不是 `eprintln!`。
//
// 行号归属（**本轮新增，T98 只知道两条串存在、没说哪条对应哪个分支**）：
//   0x469220 `adrp x0,0x17c000` + `add x0,x0,#0x634` ⇒ vaddr `0x17c634` ⇒ **:1154** ⇒ **UTF-8 臂**
//   0x4692f0 `adrp x0,0x18c000` + `add x0,x0,#0xc2c` ⇒ vaddr `0x18cc2c` ⇒ **:1149** ⇒ **JSON 臂**

/// 两条失败臂共用的内层格式串（`0x4691f0` / `0x4692ac` 都取 `0x180e61`，长度前缀 `0x16` = 22）。**观测**。
pub const MSG_DESERIALIZE_INNER: &str = "Failed to deserialize {}";

/// **JSON 解析失败**（`serde_json::from_str` 返回 Err）时的外层格式串。
///
/// 真值：`0x4692f0` → `.rodata 0x18cc2c`（字节 `01 5b c0 1f 5d 20 73 72 63 …`）。**观测**。
pub const MSG_DESERIALIZE_JSON_FAIL: &str = "[{}] src/graphwar_event.rs:1149 - {}\n";

/// **UTF-8 校验失败**（`core::str::from_utf8` 返回 Err）时的外层格式串。
///
/// 真值：`0x469220` → `.rodata 0x17c634`。**观测**。
pub const MSG_DESERIALIZE_UTF8_FAIL: &str = "[{}] src/graphwar_event.rs:1154 - {}\n";

/// `deserialize` 的失败阶段 —— 两条臂各自带一条**不同的 source 行号**，这就是"手写"的铁证：
/// derive 展开的代码不会有 `[{}] …:1149 - …` 这种手搓 `print!`（它连 `panic!` 都不走，
/// 所以 `panic::Location` 一条都没有 ⇒ 这 3 条永远进不了 527 分母，见 T98 §147.3.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeserializeFailure {
    /// `core::str::from_utf8` 失败（`0x4691c8` 的 `cmp x8,#1` 走 Err）。
    InvalidUtf8,
    /// `serde_json::from_str` 失败（`0x469288` 的 `cmp x8,x20`）。
    InvalidJson,
}

impl DeserializeFailure {
    /// 该失败阶段对应的 `print!` 外层格式串（含原版 source 行号）。**观测**。
    pub const fn message(self) -> &'static str {
        match self {
            DeserializeFailure::InvalidUtf8 => MSG_DESERIALIZE_UTF8_FAIL,
            DeserializeFailure::InvalidJson => MSG_DESERIALIZE_JSON_FAIL,
        }
    }

    /// 该失败阶段对应的原版 source 行号（从上面两条格式串里取出来，便于断言）。**观测**。
    pub const fn source_line(self) -> u32 {
        match self {
            DeserializeFailure::InvalidUtf8 => 1154,
            DeserializeFailure::InvalidJson => 1149,
        }
    }
}

// ---------------------------------------------------------------------------
// 3. `make_new_connection_request_event`（`0x4693c0`，188 B）—— 函数体已复原
// ---------------------------------------------------------------------------

/// `NewConnectionRequestEvent` 的载荷（原版是 `graphwar_event.rs` 里 72 个 `*Event` 之一）。
///
/// 字段名与顺序来自**线上报文**（`{"NewConnectionRequest":{"major_version":"2.0",
/// "minor_version":"60"}}`，抓包侧一致）与 `0x4693c0` 里两个堆串的写入顺序。**观测**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConnectionRequestEvent {
    /// `0x4693e4..0x4693f4`：`mov w8,#0x30`（`'0'`）写 `[x0,#2]`、`mov w9,#0x2e32`
    /// （小端 `'2'`,`'.'`）写 `[x0]` ⇒ 3 字节 `"2.0"`，cap = len = 3。
    pub major_version: String,
    /// `0x469410` `mov w9,#0x3036`（`'6'`,`'0'`）写 `[x0]` ⇒ 2 字节 `"60"`，cap = len = 2。
    pub minor_version: String,
}

/// 原版 `GraphwarEvent::make_new_connection_request_event`（arm64 **`0x4693c0`**，**188 B**）。
///
/// ## 证据（47 条指令全读，**观测**）
///
/// ```text
/// 0x4693c0  stp x29,x30 / stp x20,x19 / mov x29,sp
/// 0x4693cc  mov x19,x8                     ; x8 = 返回值的 sret 指针（**无入参**：x0–x7 一个都没读）
/// 0x4693d0  bl __rust_no_alloc_shim_is_unstable_v2
/// 0x4693d4  mov w0,#0x3 / mov w1,#0x1 / bl __rust_alloc       ; String::from("2.0") 的 3 字节堆
/// 0x4693e4  mov w8,#0x30                   ; '0'
/// 0x4693e8  mov w9,#0x2e32                 ; 小端 = '2','.'
/// 0x4693f0  strb w8,[x0,#0x2]
/// 0x4693f4  strh w9,[x0]                   ; → 堆串 "2.0"
/// 0x4693fc  mov w0,#0x2 / mov w1,#0x1 / bl __rust_alloc       ; String::from("60") 的 2 字节堆
/// 0x469410  mov w9,#0x3036                 ; '6','0'
/// 0x46941c  adrp x8,0x196000 ; 0x46942c ldr q0,[x8,#0xc60]     ; 常量 { TAG_NEW_CONNECTION_REQUEST, 3 }
/// 0x469434  str q0,[x19]                   ; +0x00 tag word, +0x08 = 3
/// 0x469414  str x20,[x19,#0x10]            ; +0x10 = "2.0" 堆指针
/// 0x469428  stur q0,[x19,#0x18]            ; 常量 { 3, 2 }  ← adrp 0x199000 +0x90
/// 0x469430  stp x0,x9,[x19,#0x28]          ; +0x28 = "60" 堆指针, +0x30 = 2
/// 0x469440  ret
/// ```
///
/// ⇒ **无入参**、返回 `GraphwarEvent::NewConnectionRequest(NewConnectionRequestEvent{major:"2.0", minor:"60"})`
/// （原版返回整枚举，判别值 `TAG_NEW_CONNECTION_REQUEST`；本 crate 没有载荷型枚举，见模块文档）。
///
/// ## 与原版的差异（诚实标注）
///
/// 原版把载荷**包在 `GraphwarEvent` 枚举里**返回（60 字节左右的 sret）。本 crate 的
/// [`crate::events::Event`] 是**只有判别值的 74 变体枚举**、没有载荷 ⇒ 这里返回**载荷本身**，
/// 判别值由 [`TAG_NEW_CONNECTION_REQUEST`] 单独给出。**这是数据模型缺口，不是语义差异。**
pub fn make_new_connection_request_event() -> NewConnectionRequestEvent {
    NewConnectionRequestEvent {
        major_version: NEW_CONNECTION_REQUEST_MAJOR.to_string(),
        minor_version: NEW_CONNECTION_REQUEST_MINOR.to_string(),
    }
}

/// `0x4693e4`/`0x4693e8`/`0x4693f4` 的三条立即数拼出来的第一个版本串（3 字节）。**观测**。
pub const NEW_CONNECTION_REQUEST_MAJOR: &str = "2.0";

/// `0x469410` `mov w9,#0x3036` 拼出来的第二个版本串（2 字节）。**观测**。
pub const NEW_CONNECTION_REQUEST_MINOR: &str = "60";

// ---------------------------------------------------------------------------
// 4. 未落码的两条 + 缺什么（**诚实清单**，见模块文档）
// ---------------------------------------------------------------------------
//
// `serialize`（`0x46947c`，304 B）：`__rust_alloc(0x80,1)` 造 `Vec::with_capacity(128)` →
// `bl 0x4695ac`（= `<GraphwarEvent as serde::ser::Serialize>::serialize`，derive 本体）→
// 成功把 24 字节 Vec 写进 sret、失败释放缓冲 ⇒ **就是 `serde_json::to_vec(self)` 的形态**。
// 落码缺：`serde_json` crate + `GraphwarEvent`（含载荷）的 `impl Serialize`（= 要 72 个
// `#[derive]`）。**登记 real-missing，不写伪实现。**
//
// `deserialize`（`0x4691a4`，540 B）：`str::from_utf8` → `serde_json::from_str` → 两个失败臂
// `Utc::now()` + `format!` + `print!`，失败返回 `TAG_INVALID`。落码缺：`serde_json` + `chrono`
// 两个 crate + 载荷枚举。**结构化的那部分已经落在本节常量与 [`DeserializeFailure`] 里。**

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_words_and_layout_are_the_immediates_from_0x4693c0_and_0x4691a4() {
        // 0x46941c/0x46942c 的常量首字
        assert_eq!(TAG_NEW_CONNECTION_REQUEST, 0x8000_0000_0000_0004);
        assert_eq!(TAG_NEW_CONNECTION_REQUEST & 0xffff_ffff, 4); // = Event::NewConnectionRequest
        // 0x469244 / 0x469318 的哨兵
        assert_eq!(TAG_INVALID, 0x8000_0000_0000_004a);
        assert_eq!(TAG_INVALID & 0xffff_ffff, 74); // = 变体数 ⇒ 落在 0..=73 之外
        assert_eq!(TAG_BASE, 0x8000_0000_0000_0000);
        // 0x469334..0x469350 逐字拷 0x48 字节
        assert_eq!(EVENT_BYTES, 72);
        // 0x469498 mov w0,#0x80
        assert_eq!(SERIALIZE_VEC_CAPACITY, 128);
    }

    #[test]
    fn make_new_connection_request_event_fills_the_two_heap_strings_from_immediates() {
        let e = make_new_connection_request_event();
        // 0x4693e8 mov w9,#0x2e32 = '2','.'  + 0x4693e4 mov w8,#0x30 = '0'
        assert_eq!(e.major_version, "2.0");
        assert_eq!(e.minor_version, "60");
        // cap/len 是两个串各自分配时的字节数（0x4693d4 alloc(3) / 0x4693fc alloc(2)）
        assert_eq!(e.major_version.len(), 3);
        assert_eq!(e.minor_version.len(), 2);
        // 线上报文字段顺序 = major_version 先、minor_version 后（写进 +0x10 / +0x28）
        assert_eq!(
            format!(
                r#"{{"major_version":"{}","minor_version":"{}"}}"#,
                e.major_version, e.minor_version
            ),
            r#"{"major_version":"2.0","minor_version":"60"}"#
        );
    }

    #[test]
    fn deserialize_failure_lines_are_1149_for_json_and_1154_for_utf8() {
        // 0x4692f0 → .rodata 0x18cc2c（:1149）；0x469220 → .rodata 0x17c634（:1154）
        assert_eq!(DeserializeFailure::InvalidJson.source_line(), 1149);
        assert_eq!(DeserializeFailure::InvalidUtf8.source_line(), 1154);
        assert!(DeserializeFailure::InvalidJson.message().contains("graphwar_event.rs:1149"));
        assert!(DeserializeFailure::InvalidUtf8.message().contains("graphwar_event.rs:1154"));
        // 两条臂共用的内层串：`.rodata 0x180e61` 的长度前缀 `0x16` = 22，
        // 正好是 `{}` **之前**那截 "Failed to deserialize "（22 字符），`{}` 另占 2 字节
        assert_eq!(MSG_DESERIALIZE_INNER, "Failed to deserialize {}");
        assert_eq!(MSG_DESERIALIZE_INNER.len(), 24);
        assert_eq!(MSG_DESERIALIZE_INNER.strip_suffix("{}").unwrap().len(), 22);
    }
}
