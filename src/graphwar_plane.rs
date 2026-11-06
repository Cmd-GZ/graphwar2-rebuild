//! `graphwar_plane` —— 原版同名模块的还原（**占位**，等待实现）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。
//!
//! ## 与 `src/plane.rs` 的关系
//!
//! `plane.rs` 是历史上的"混装"文件（`plane` + `plane_pos_manager` + 疑似的 `obstacle_manager`），
//! 已登记在 `docs/02-module-map.md` §17.4。本文件是 `graphwar_plane` 的**正式归属地**。
//!
//! ## 先查真名，别猜
//!
//! ```text
//! awk -F'\t' '$1=="graphwar_plane"' fixtures/name_to_addr.tsv
//! python3 tools/symtab_dump.py modules | grep graphwar_plane
//! ```
