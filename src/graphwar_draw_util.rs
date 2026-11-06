//! 原版 `src/graphwar_draw_util.rs` —— **不画东西的"绘制工具"**。
//!
//! ## 定性（2026-11-07，`docs/16-client.md` §33.6，`观测`）
//!
//! 2.0.60 arm64 符号表里本模块**只有一个函数**：
//! `GraphwarDrawUtil::get_team_overview`（`0x4c3b38`，1548 字节）。
//! `GraphwarDrawUtil::set_plane_transform` 在 2024 版的**方法名表**里存在
//! （`docs/02-module-map.md` §18.5），但 2.0.60 的 `.symtab` 里**没有同名符号**
//! ⇒ 要么被内联掉、要么已删除（`推断`，缺"调用点在哪"的证据）。
//!
//! ## `get_team_overview` 到底做什么
//!
//! 它的函数体里**一次 `call *slot` 也没有**指向绘制 API（`draw_line` / `draw_arc` /
//! `draw_polygon` / `draw_texture`），却 **4 次**调用
//! `GraphwarModuleContainer::get_module`，并 **8 次** `Rc::drop_slow`。
//! ⇒ 它是**取数据**的：从模块容器里把"队伍总览"需要的几个 manager
//! （`team_manager` / `color_manager` / `player_manager` / `local_player_manager`）
//! 逐个 `get_module` 取出来、拼成一个快照返回。
//!
//! 这与上一轮 agent 的**有界否定**一致："它不画东西"——
//! 现在可以把它从"否定"升级为**正面的定性**：它是
//! **`GraphwarModuleContainer` 的一组类型化取用**，被绘制方当作"读数据"的入口。
//!
//! ## 7 个调用者全部是 draw/process（`观测`，arm64 `bl` 图）
//!
//! | 调用者 | 性质 |
//! |---|---|
//! | `EffectDrawer::draw` | 画 |
//! | `EntityDrawer::draw` | 画 |
//! | `EntityDrawer::update_name_label` | 画（名字标签） |
//! | `EntityDrawer::update_sprite` | 画（贴图） |
//! | `GraphwarPlane::draw` | 画 |
//! | `GraphwarPlane::process` | 每帧 |
//! | `GraphwarPlayScreenControl::update_end_game_pop_up` | 结算面板 |
//!
//! 覆盖了"实体 / 特效 / 平面 / 结算"四块，正好是**都需要知道队伍归属与颜色**的地方。
//! 名字里的 `draw_util` 因此是"绘制用的工具函数"，不是"绘制器"本身。
//!
//! ## 它在 x86-64 目标上的地址（`逐位`）
//!
//! `0x4be6e0`，`src/graphwar_draw_util.rs:28:53 / 30:51 / 31:41 / 33:42` 四个
//! `panic::Location` 锚全部落在它身上（`tools/loc_xrefs.py` 单次扫描，
//! 见 `docs/16-client.md` §27）。同一函数体的锚里还有两条
//! `src/graphwar_game_client.rs:152:22/152:49` —— 那是**泛型被两边内联**造成的
//! （`docs/02-module-map.md` §18.2 的"7 个不一致"之一），不改变归属。

/// 队伍总览里的一行：某个玩家的队伍归属 + 配色索引。
///
/// 字段只保留**能从调用方用法反推**的三个（`推断`：`TeamManager`/`ColorManager`
/// 的实际字段在本 crate 里已有实现，这里只是"绘制方需要的投影"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamOverviewEntry {
    /// 玩家 id（`GraphwarPlayerManager` 的键）。
    pub player_id: u64,
    /// 队伍判别值：0 = `Team1`、1 = `Team2`（`docs/07` §5.2，`逐位`）。
    pub team: u8,
    /// 该队伍的颜色索引（`GraphwarColorManager::add_team_color` 的顺序）。
    pub color_index: u8,
}

/// `get_team_overview` 的**形态**：把容器里的几份表合成一个只读快照。
///
/// 原版没有把结果做成 `Vec`（函数返回的是借用/守卫的组合，1548 字节里
/// 有 8 次 `Rc::drop_slow`），本 crate 用 `Vec` 表达"取出来的一份拷贝"，
/// 差异已在 `docs/16-client.md` §33.6 标注为 `推断`。
pub fn get_team_overview(
    players: &[(u64, u8)],
    team_colors: &[u8],
) -> Vec<TeamOverviewEntry> {
    players
        .iter()
        .map(|&(player_id, team)| TeamOverviewEntry {
            player_id,
            team,
            // 颜色索引 = 该队第一次出现在 add_team_color 里的位置；
            // 没有登记就退化成队伍判别值本身（原版这条兜底未正向观测，标 `推断`）。
            color_index: team_colors
                .iter()
                .position(|&t| t == team)
                .map(|i| i as u8)
                .unwrap_or(team),
        })
        .collect()
}
