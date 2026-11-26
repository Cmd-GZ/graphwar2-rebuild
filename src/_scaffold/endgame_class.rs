//! **不是原版的东西**：结算弹窗（`EndGamePopUp`）的**数据桥** —— 胜方字节 + 两条文案。
//!
//! # 为什么会有这一份
//!
//! 原版这一屏的数据由 `GraphwarPlayScreenControl` 里那个 **`0x3da130`**（`FN_SHOW_VICTORY`，
//! 128 字节的胜负横幅）填：它写 `SelfVictoryLabel` / `TeamVictoryLabel` 两条文本 +
//! 胜利者头像三层贴图（`SkinIcon` / `FaceIcon` / `HatIcon`）。原版**胜负字节**则更早就在
//! `GraphwarGameStateManager::set_state(EndingGame)` 里算好了（`0x478133`–`0x478236` → `[self+0x11f]`）。
//!
//! 那两段 Rust 逻辑我方**都已复元**：
//!
//! | 要算的东西 | 落点 | 档位 |
//! |---|---|---|
//! | 胜方字节 `(Team1 活, Team2 活, 设过上限) → 0/1/2` | [`crate::turnstate::winner_byte`] | **逐位**（四行表见 `docs/14` §11.4） |
//! | 六条横幅字面量 | [`crate::graphwar_play_screen_control::VICTORY_TEXTS`]（`.rodata` `0x1bf91e` 起） | **观测** |
//!
//! 所以本文件**只做搬运**：把上面两项接给 GDScript，**不新增任何判据**。
//! 界面侧（`client/src/app/app.gd` + `client/src/screens/endgame_pop_up.gd`）此前用的是
//! 自己编的字符串（`"YOU WON"` / `"Team %d Victory!"` 手写、并且给它们加了自定义颜色），
//! 而 `.rodata` 里 `"YOU WON"` **本来就在** —— 这一层就是把"编的"换成"复元的"。
//!
//! # 一处必须标档的地方：哪条结局走哪个变体
//!
//! `VictoryBanner` 的**变体 → 字面量**是观测（每条都能指到 `.rodata`），但
//! **"哪种结局走哪个变体"在原版 `0x3da130` 里没解出来**（缺 `memcmp`/分支输入的证据，
//! 见 [`crate::graphwar_play_screen_control::VictoryBanner`] 的文档）。
//! 本文件按下面这张表选，**逐行标档**：
//!
//! | 结局 | `TeamVictoryLabel` | `SelfVictoryLabel` | 档位 |
//! |---|---|---|---|
//! | 字节 `0`（Team1 胜）+ 我在 Team1 | `Team 1 Victory!` | `YOU WON` | **观测**（真机真帧：`docs/06` §68.4 那一屏就是这个组合，见 `docs/06` §77 的复采） |
//! | 字节 `0`/`1` + 我不在胜方 | `Team 1 Victory!` / `Team 2 Victory!` | `YOU lOST` | **推断**（两条字面量都是观测的；"输的一侧显示哪条"按 `docs/92` §4.13 的既有观测取 `YOU lOST`，`docs/16` §70.4 的静态 `.tscn` 默认值也是它） |
//! | 字节 `2`（没人赢） | `Tie!` | `YOU DID NOT WIN` | **推断**（无真值；两条都是 `.rodata` 里的字面量，"没人赢 ⇒ 我也没赢"是直译） |
//!
//! ⚠️ 除第一行外**都没有真值**。要证它只需换一份"我方输"的真回放收场尾部进
//! `fixtures/frame_endgame_events.txt` 那条管线（判据文件不用改）。

use godot::classes::RefCounted;
use godot::prelude::*;

use crate::graphwar_play_screen_control::{VictoryBanner, VICTORY_TEXTS};

/// 胜方字节 → 界面队伍号（`0` = Team1、`1` = Team2、`2` = 没人赢 ⇒ `-1`）。
fn team_of_byte(byte: i64) -> i64 {
    match byte {
        0 => 0,
        1 => 1,
        _ => -1,
    }
}

/// 两条横幅的**选择逻辑**（纯函数，可 `cargo test` 直接测；外壳只是把它搬给 GDScript）。
///
/// 返回值 = `(TeamVictoryLabel 的变体, SelfVictoryLabel 的变体, 证据档)`，
/// 表在文件头（第一行**观测**，后两行**推断**）。
fn banners_for(winner_byte: i64, human_team: i64) -> (VictoryBanner, VictoryBanner, &'static str) {
    match winner_byte {
        0 | 1 => {
            let winner = team_of_byte(winner_byte);
            let team_banner = if winner == 0 { VictoryBanner::Team1 } else { VictoryBanner::Team2 };
            if winner == human_team {
                // **观测**：真机真帧（Team1 胜、我在 Team1）就是这两个串。
                (team_banner, VictoryBanner::SelfWon, "observed")
            } else {
                // **推断**：字面量是观测的；"输的一侧显示哪条"没有真值。
                (team_banner, VictoryBanner::SelfLost, "inferred")
            }
        }
        // **推断**：字节 2 = 没人赢。
        _ => (VictoryBanner::Tie, VictoryBanner::SelfDidNotWin, "inferred"),
    }
}

/// 结算弹窗的数据桥（**scaffold**；原版这两件事在 `0x3da130` 与 `0x478133`–`0x478236`）。
///
/// GDScript 用法（`client/src/app/app.gd` / `client/src/screens/endgame_pop_up.gd`）：
///
/// ```text
/// var e := GwEndGame.new()
/// e.winner_byte(true, false, false)        # -> 0（Team1 胜），= turnstate::winner_byte
/// e.winner_team(0)                          # -> 0（界面队伍号；字节 2 ⇒ -1）
/// e.label_texts(0, 0)                       # -> {"self":"YOU WON","team":"Team 1 Victory!",...}
/// e.victory_texts()                         # -> 那 6 条 .rodata 字面量（诊断/自检）
/// ```
#[derive(GodotClass)]
#[class(base = RefCounted)]
pub struct GwEndGame {
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GwEndGame {
    fn init(base: Base<RefCounted>) -> Self {
        Self { base }
    }
}

#[godot_api]
impl GwEndGame {
    /// **胜方字节**：`0` = Team1、`1` = Team2、`2` = 没人赢。
    ///
    /// 逐位落点 = [`crate::turnstate::winner_byte`]（`0x478133`–`0x478236` 的四行表，
    /// `docs/14` §11.4）。**这里不重算**，只是把那张表搬到 GDScript 能调的地方。
    ///
    /// `turn_limit_set` 是**"设过回合上限"**（真值 `[+0xc8] == 1`），**不是**"上限已用尽"。
    /// 两者只在"两边都还有人却进了 `EndingGame`"这一种收场上分叉，而那种收场只可能由
    /// 上限触发 ⇒ 可达输入上调用方可以安全地传"两边都还有人"（`docs/14` §11.4 的注）。
    #[func]
    fn winner_byte(&self, team1_alive: bool, team2_alive: bool, turn_limit_set: bool) -> i64 {
        i64::from(crate::turnstate::winner_byte(team1_alive, team2_alive, turn_limit_set))
    }

    /// 胜方字节 → 界面队伍号（`0`/`1`；字节 `2` ⇒ **`-1`** = 没有胜方）。
    #[func]
    fn winner_team(&self, winner_byte: i64) -> i64 {
        team_of_byte(winner_byte)
    }

    /// 两条横幅文本 + 这一格用的是**哪一档证据**。
    ///
    /// 返回 `{self, team, evidence, winner_team, human_team}`：
    /// `self` = `SelfVictoryLabel`，`team` = `TeamVictoryLabel`（字符串全部来自
    /// [`VictoryBanner::text`] ⇒ `.rodata` 那 6 条，**不在这里新写字符串**）；
    /// `evidence` ∈ {`observed`, `inferred`}，对应文件头那张表的三行。
    #[func]
    fn label_texts(&self, winner_byte: i64, human_team: i64) -> VarDictionary {
        let (banner_team, banner_self, evidence) = banners_for(winner_byte, human_team);
        let mut d = VarDictionary::new();
        d.set("self", &GString::from(banner_self.text()));
        d.set("team", &GString::from(banner_team.text()));
        d.set("evidence", &GString::from(evidence));
        d.set("winner_team", team_of_byte(winner_byte));
        d.set("human_team", human_team);
        d
    }

    /// `.rodata` 里那 6 条胜负横幅字面量（**观测**，顺序与原版一致）。
    /// 给自检对拍用 —— 界面只经 [`Self::label_texts`] 取文本。
    #[func]
    fn victory_texts(&self) -> VarArray {
        let mut arr = VarArray::new();
        for t in VICTORY_TEXTS {
            arr.push(&GString::from(t));
        }
        arr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 四行表逐行（与 `turnstate::winner_byte` 的测试同源，这里验**搬运**没走样）。
    #[test]
    fn winner_byte_is_the_binary_table() {
        // 直接测被搬运的那张表（外壳 `#[func]` 只是 `i64::from(...)`）。
        assert_eq!(crate::turnstate::winner_byte(false, false, false), 2);
        assert_eq!(crate::turnstate::winner_byte(false, true, false), 1);
        assert_eq!(crate::turnstate::winner_byte(true, false, false), 0);
        assert_eq!(crate::turnstate::winner_byte(true, true, false), 2);
        assert_eq!(crate::turnstate::winner_byte(true, true, true), 1);
    }

    #[test]
    fn byte_maps_to_team() {
        assert_eq!(team_of_byte(0), 0);
        assert_eq!(team_of_byte(1), 1);
        assert_eq!(team_of_byte(2), -1);
    }

    /// `0x3da130` 那两条串：胜方一侧是 **观测**，其余两条是 **推断**（见文件头那张表）。
    #[test]
    fn texts_are_the_rodata_literals() {
        assert_eq!(VictoryBanner::Team1.text(), "Team 1 Victory!");
        assert_eq!(VictoryBanner::Team2.text(), "Team 2 Victory!");
        assert_eq!(VictoryBanner::SelfWon.text(), "YOU WON");
        assert_eq!(VictoryBanner::SelfLost.text(), "YOU lOST");
        assert_eq!(VictoryBanner::SelfDidNotWin.text(), "YOU DID NOT WIN");
        assert_eq!(VictoryBanner::Tie.text(), "Tie!");
        assert_eq!(VICTORY_TEXTS.len(), 6);
    }

    /// 选文案那张表：**只有第一行是观测**，逐行钉住，别在重构时把档位弄丢。
    #[test]
    fn banner_table_rows() {
        assert_eq!(
            banners_for(0, 0),
            (VictoryBanner::Team1, VictoryBanner::SelfWon, "observed")
        );
        assert_eq!(
            banners_for(0, 1),
            (VictoryBanner::Team1, VictoryBanner::SelfLost, "inferred")
        );
        assert_eq!(
            banners_for(1, 0),
            (VictoryBanner::Team2, VictoryBanner::SelfLost, "inferred")
        );
        assert_eq!(
            banners_for(2, 0),
            (VictoryBanner::Tie, VictoryBanner::SelfDidNotWin, "inferred")
        );
    }
}
