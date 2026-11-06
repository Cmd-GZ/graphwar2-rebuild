//! 原版 `src/graphwar_campaign_control.rs` —— **战役模式的关卡进度**。
//!
//! ## 定性（2026-11-07，`docs/16-client.md` §33.8，`观测`）
//!
//! 4 个函数（2.0.60 arm64 符号表，名字与 `docs/02-module-map.md` §18.5 完全一致）：
//!
//! | 符号 | arm64 | 字节 | 谁调 | 干什么（`逐位`：`FileAccess` 调用序列） |
//! |---|---|---|---|---|
//! | `get_current_level` | `0x392f18` | 528 | `RootNode::ready`、`LocalPlayerManager::handle_player_info` | `FileAccess::open` → `get_as_text` → `close` → `unreference`，再 `get_level_as_int` |
//! | `save_current_level` | `0x393128` | 1072 | `RootNode::check_save_level` | `open`/`get_as_text`/`store_string`/`close` **×2 组**，中间 `get_level_as_int` ×3 |
//! | `get_level_as_int` | `0x392cc0` | 600 | `get_current_level`、`save_current_level` | `StrSearcher::new` + `SpecFromIterNested::from_iter` ⇒ 用 `str::split`/`matches` 从关卡名里抠整数 |
//! | `show_hide_campaign_levels` | `0x393558` | 660 | `RootNode::ready`、`save_current_level` | `GraphwarNodes::get_node_by_name` ×1 + `<u32 as Display>::_fmt` ⇒ 拼节点名，按键显隐 |
//!
//! ## 它怎么表示"当前关卡"
//!
//! **存的是字符串，不是整数**：`get_current_level` 把整个存档文本读出来当关卡名用，
//! `get_level_as_int` 再从里面**抠数字**（`StrSearcher` = `str::split`/`str::find` 的
//! 单态化）。这和 `docs/10-campaign.md` §7 解出来的存档格式（21 关、逐关一行）吻合：
//! 关卡标识本身就是文件名/行首那串 `level_N`。
//!
//! ## `show_hide_campaign_levels` 是"关卡按钮的显隐"
//!
//! 它 `bl` 到 `GraphwarNodes::get_node_by_name`（唯一一次），又用 `<u32>::_fmt`
//! 拼出一个含数字的节点名 —— 典型的
//! `format!("Campaign{i}Button")` 再 `get_node_by_name` 后 `set_visible(已解锁)`。
//! 具体的名字模板**未解**（格式化字符串在 `.rodata` 里，本轮没逐字节还原），
//! 标 `未知`；节点命名规律可参考 `docs/16-client.md` §28（40 个节点名）。
//!
//! ## 缺的两处归属（沿用旧结论，不重推）
//!
//! `docs/02-module-map.md` §17.4 记过：`level_info`(`0x4ad380`) 与 `campaign.save` 簇
//! (`0x417ef0`/`0x418170`/`0x4183b0`) **没有 `Location` 锚**，暂时挂在
//! `graphwar_level_manager.rs` 名下。本轮**没有**给它们找到新锚点，
//! 所以那条"待拆"状态**不变**（不多也不少一件事）。

/// 战役关卡数（`docs/10-campaign.md` §7 解出的存档里 21 关，`逐位`）。
pub const CAMPAIGN_LEVEL_COUNT: u32 = 21;

/// 从关卡标识里抠整数（`get_level_as_int` 的语义）。
///
/// `逐位`依据：函数体里 `core::str::pattern::StrSearcher::new` +
/// `SpecFromIterNested::from_iter` 两条 `bl`，说明它做的是一次**字符串切分**后
/// 取第一个可解析段，而不是 `parse::<u32>()` 一把梭（后者不会带 `StrSearcher`）。
///
/// 找不到数字时原版走 `unwrap_failed`（`get_current_level` 里 3 次），
/// 也就是**会 panic**；这里返回 `None` 并把"原版会炸"写在文档里。
pub fn get_level_as_int(level: &str) -> Option<u32> {
    let digits: String = level
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    // 关卡号从 1 数起；0 在原版里等价于"没有进度"。
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// 已解锁到第几关（`get_current_level` 的结果形状）。
///
/// 原版返回的是**字符串**（整个存档文本），这里换成整数是因为
/// 所有调用方都只是拿它去 `show_hide_campaign_levels` 比较。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CampaignProgress {
    /// 已解锁的最高关（0 = 一关都没过）。
    pub unlocked_through: u32,
}

impl CampaignProgress {
    pub fn from_save_text(text: &str) -> Self {
        Self {
            unlocked_through: get_level_as_int(text).unwrap_or(0),
        }
    }

    /// `show_hide_campaign_levels` 的判据：第 `level` 关（1 起）是否显示。
    pub fn is_level_visible(&self, level: u32) -> bool {
        level >= 1 && level <= CAMPAIGN_LEVEL_COUNT && level <= self.unlocked_through + 1
    }
}
