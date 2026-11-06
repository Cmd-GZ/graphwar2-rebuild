//! `player_showcase` —— 原版同名模块的还原（**占位**，等待实现）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。
//!
//! ## 先查真名，别猜
//!
//! `fixtures/name_to_addr.tsv` 有本模块的**原版方法名 ↔ x86 地址**：
//!
//! ```text
//! awk -F'\t' '$1=="player_showcase"' fixtures/name_to_addr.tsv
//! ```
//!
//! 表里没有的函数，用 **arm64 2.0.60 的 `.symtab`**（未 strip）拿真名与真边界，
//! 再用「引用哪个 panic 锚 / 哪个类型名立即数 / 哪个常量」做跨架构指纹在 x86 上定位。
//!
//! 模块权威清单：`python3 tools/symtab_dump.py modules`，以及 `docs/02-module-map.md` §18。
