//! `graphwar_long_press_animation` —— 原版同名模块的还原（**占位**，等待实现）
//!
//! 按 `AGENTS.md` §0：目标是**还原源码**，不是"写得像"。
//!
//! ## 真名与地址（本轮新增的杠杆）
//!
//! `fixtures/name_to_addr.tsv` 里有本模块的**原版源码级方法名 ↔ x86 地址**，
//! 由 `tools/name_bridge.py` 用 `panic::Location` 跨架构 join 得到。
//! 先查那张表，**不要自己猜方法名**：
//!
//! ```text
//! awk -F'\t' '$1=="graphwar_long_press_animation"' fixtures/name_to_addr.tsv
//! ```
//!
//! 模块的权威函数数/类型名/方法数见 `docs/02-module-map.md` §18 的 A 表
//! （`python3 tools/symtab_dump.py modules`）。
//!
//! ## 证据要求
//!
//! 每个函数都要能指到**反汇编地址**或**真机真帧**；指不到的写
//! `// 推断：缺什么证据`，不许把推断写成陈述句。
