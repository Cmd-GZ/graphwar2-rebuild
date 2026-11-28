//! `graphwar_version`：版本串（原版模块，Android 侧**只有 1 个函数**）。
//!
//! 证据分级沿用仓库约定：**逐位验证** / **观测** / **推断** / **未知**。
//! 地址一律 **arm64 vaddr**（未 strip 的 Android 2.0.60 `.symtab`，符号名就是源码名）。
//!
//! ## 0. 这个模块此前也不在任何清单里（本轮发现，2026-11-28 T109）
//!
//! * `docs/coverage-matrix.tsv`（79 行）里**没有** `graphwar_version`；
//! * `docs/02-module-map.md` 的 78 模块表里也**没有**它；
//! * Linux 2.0.60 的 `.so` 里 `"graphwar_version.rs"` 这个 Location 文件名串出现 **0 次**
//!   ⇒ 一条 `panic::Location` 都没有 ⇒ Location 口径的 527 分母与模块清单都看不见它。
//!
//! （模块清单的完整复核见 `docs/06` §153：Android `.symtab` 里出现
//! `graphwarrust::graphwar_*::` 形式的模块路径有 **65** 个，而 `coverage-matrix` 的
//! `scope=graphwar` 只有 **62** 行 —— 差的 3 个是 `graphwar_texture_loader`（被记成
//! `scope=other`）、`graphwar_ban_list`、`graphwar_version`（后两个**整行缺失**）。）
//!
//! ## 1. 函数清单
//!
//! | 函数 | arm64 vaddr / 字节 | 职责 | 档位 |
//! |---|---|---|---|
//! | `graphwar_version_get_full_version` | `0x3913e4` / 272 | 返回完整版本串 `String` | **观测** |
//!
//! 符号是**自由函数**（`graphwarrust::graphwar_version::graphwar_version_get_full_version`，
//! 中间没有类型段）⇒ 不属于任何 `struct` 的 `impl`，是模块级 `pub fn`。
//!
//! ## 2. 返回值是**编译期常量** `"2.0.60"`（**观测**，逐条指令）
//!
//! 函数体是"分配 + 三处写字节"，没有任何输入、没有任何运行时来源：
//!
//! ```text
//! 0x3913fc  mov w0,#0x3 / 0x391400 mov w1,#0x1 / bl __rust_alloc   ; 分配 3 字节
//! 0x391410  mov w9,#0x2e32   ; 小端 = 0x32 '2', 0x2e '.'
//! 0x39141c  strh w9,[x0]     ; [0..2] = "2."
//! 0x39140c  mov w8,#0x30     ; '0'
//! 0x391414  strb w8,[x0,#0x2]; [2] = '0'          ⇒ 此刻 "2.0"（len=3）
//! 0x391448  mov w11,#0x2e    ; '.'
//! 0x391458  strb w11,[x10,x8]; 追加 '.'           ⇒ "2.0."（len=4）
//! 0x391474  mov w9,#0x3036   ; 小端 = 0x36 '6', 0x30 '0'
//! 0x391478  strh w9,[x8,x1]  ; 追加 "60"          ⇒ "2.0.60"（len=6）
//! 0x391480  str q0,[x19] / 0x391484 str x8,[x19,#0x10]   ; 写 sret（String = 3 个字）
//! ```
//!
//! 两次容量增长也读得出：`0x39143c RawVecInner::do_reserve_and_handle(len=3, add=1)`、
//! `0x3914a8 同(len=4, add=2)` ⇒ **逐字节追加**，最终长度 **6**。
//!
//! ⚠️ 版本号与 **Android 2.0.60 这份构建**一致；`2.0.60` 是这份 `.so` 的编译期常量，
//! **不是**从资源/网络读来的。Linux 2.0.60 那份是否返回同一串**未验证**
//! （它 stripped、且本函数在 Linux 侧没有 Location 锚 ⇒ 无从配对，见 §4）。
//!
//! ## 3. 未观测到的东西（**未知**，不编）
//!
//! * 是否还有 `graphwar_version_get_version` / `get_major` 之类**被内联**掉的同伴函数：
//!   Android `.symtab` 里本模块只有这 1 个符号；Linux 侧无锚 ⇒ **证不到"没有"**，
//!   只能说"未观测到"（`AGENTS.md`：零调用/零符号 ≠ 不存在，函数可能被内联）。
//! * 版本串的**消费者**：本轮没有追调用者（`arm64_sym.py callers 0x3913e4` 的结果留给下一轮；
//!   从语义看它是给日志/关于框用的，但**没有证据**）。

/// 原版 `GraphwarVersion::graphwar_version_get_full_version` 的返回值。
///
/// 档位：**观测**（`0x391410`/`0x39140c`/`0x391448`/`0x391474` 四处立即数拼出来的 6 个字节，
/// `0x391480` 的 sret 写 3 个字 = `String`）。
pub const FULL_VERSION: &str = "2.0.60";

/// `graphwar_version::graphwar_version_get_full_version`（arm64 **`0x3913e4`** / **272** B）。
///
/// 原版返回 `String`（sret 3 字）；值恒为 [`FULL_VERSION`]（编译期常量）。
pub fn graphwar_version_get_full_version() -> String {
    FULL_VERSION.to_string()
}
