//! `graphwar_module_container` —— 原版同名模块的还原（**占位**，等待实现）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。
//!
//! ## 这是剩下的最大单模块
//!
//! 2024 版未 strip 的符号表显示本模块有 **68 个函数**（`docs/02` §18 A 表），
//! 全库单一模块里最多。它是各 manager 的**持有者/装配点**——
//! `graphwar_lan_vbox` 就从它 `+0x128`（LAN 广播器）/ `+0x240`（broker）取表，
//! `player_showcase` 从根节点 `+0x16a0`/`+0x16a8` 取 `GraphwarTextureLoader` / `GraphwarPlayerConfig`。
//!
//! ## 先查真名，别猜
//!
//! ```text
//! awk -F'\t' '$1=="graphwar_module_container"' fixtures/name_to_addr.tsv
//! python3 tools/symtab_dump.py modules | grep module_container
//! ```
